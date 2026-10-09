mod attendance;
mod auth;
mod calendar;
mod classroom;
mod files;
mod requests;
mod transport;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8};
use std::sync::Arc;
use std::time::{Duration, Instant};

use hongsi_core::calendar::{CalendarData, SubmitRejection};
use hongsi_core::models::{ActiveLectures, AttendanceCourse, Timetable};
pub use hongsi_core::submission;
use hongsi_core::{CoreError, SchoolSession, SchoolSessionSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{Mutex, RwLock};

const CN2: &str = "https://cn2.hongik.ac.kr/";
const AUTOLOGIN_GAP: Duration = Duration::from_secs(6 * 60);
const MB: u64 = 1024 * 1024;

#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub body: Value,
}

impl Reply {
    fn ok(body: Value) -> Self {
        Self { status: 200, body }
    }

    pub fn error(status: u16, code: &str, message: impl Into<String>) -> Self {
        Self {
            status,
            body: json!({ "error": { "code": code, "message": message.into() } }),
        }
    }

    pub fn code(&self) -> Option<&str> {
        self.body["error"]["code"].as_str()
    }

    pub fn needs_login(&self) -> bool {
        self.status == 401 && self.code() != Some("login_rejected")
    }
}

impl From<CoreError> for Reply {
    fn from(err: CoreError) -> Self {
        let message = err.to_string();
        match err {
            CoreError::SessionExpired => Reply::error(401, "session_expired", message),
            CoreError::ClassroomTokenExpired => Reply::error(401, "classroom_token_expired", message),
            CoreError::LoginRejected(_) => Reply::error(401, "login_rejected", message),
            CoreError::Network(e) => {
                tracing::warn!(timeout = e.is_timeout(), connect = e.is_connect(), "학교 서버 연결 실패");
                Reply::error(502, "school_unreachable", message)
            }
            CoreError::Parse(_) => Reply::error(502, "school_changed", "학교 응답을 확인하지 못했어요."),
            CoreError::Upstream(_) => Reply::error(502, "school_error", message),
            CoreError::NotFound(_) => Reply::error(404, "not_found", message),
        }
    }
}

fn rejection(r: SubmitRejection) -> Reply {
    match r {
        SubmitRejection::BadRequest(m) => Reply::error(400, "bad_request", m),
        SubmitRejection::Conflict(m) => Reply::error(409, "conflict", m),
        SubmitRejection::LateConfirmRequired => {
            Reply::error(428, "late_confirm_required", "마감이 지난 과제예요. 지각 제출을 확인해 주세요.")
        }
    }
}

type R<T> = Result<T, Reply>;

fn cached<T: Clone>(slot: &Option<(Instant, T)>, max_age: Duration) -> Option<T> {
    slot.as_ref().filter(|(at, _)| at.elapsed() < max_age).map(|(_, v)| v.clone())
}

struct School {
    session: SchoolSession,
    student_id: String,
    calendar: Mutex<Option<(Instant, CalendarData)>>,
    lectures: Mutex<Option<(Instant, ActiveLectures)>>,
    timetable: Mutex<Option<(Instant, Timetable)>>,
    courses: Mutex<HashMap<String, (Instant, AttendanceCourse)>>,
    autologin_at: std::sync::Mutex<Option<Instant>>,
}

impl School {
    fn new(session: SchoolSession, student_id: String) -> Self {
        Self {
            session,
            student_id,
            calendar: Mutex::new(None),
            lectures: Mutex::new(None),
            timetable: Mutex::new(None),
            courses: Mutex::new(HashMap::new()),
            autologin_at: std::sync::Mutex::new(None),
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
struct ServerAuth {
    token: String,
    expires_at: i64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthSnapshot {
    student_id: String,
    school: SchoolSessionSnapshot,
    server_base: String,
    server: Option<ServerAuth>,
}

pub struct Direct {
    http: reqwest::Client,
    base: String,
    server_auth: std::sync::Mutex<Option<ServerAuth>>,
    server_login_lock: Mutex<()>,
    revoked: AtomicBool,
    remember: AtomicBool,
    server_ok: AtomicU8,
    school: RwLock<Option<Arc<School>>>,
    generation: AtomicU64,
    submissions: submission::Jobs,
}

impl Direct {
    pub fn new(base: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP 클라이언트");
        Self {
            http,
            base,
            server_auth: Default::default(),
            server_login_lock: Mutex::new(()),
            revoked: AtomicBool::new(false),
            remember: AtomicBool::new(false),
            server_ok: AtomicU8::new(0),
            school: RwLock::new(None),
            generation: AtomicU64::new(0),
            submissions: Default::default(),
        }
    }
}

fn ids(rest: &str, n: usize) -> R<Vec<i64>> {
    let parts: Vec<i64> = rest.split('/').map(str::parse).collect::<Result<_, _>>().unwrap_or_default();
    if parts.len() == n {
        Ok(parts)
    } else {
        Err(Reply::error(400, "bad_request", "잘못된 주소예요."))
    }
}

fn decode(segment: &str) -> String {
    url::form_urlencoded::parse(format!("k={}", segment.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
}

fn encode(segment: &str) -> String {
    url::form_urlencoded::byte_serialize(segment.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
}

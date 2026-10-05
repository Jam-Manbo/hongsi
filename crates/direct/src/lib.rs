use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use hongsi_core::calendar::{self, CalendarData, SnapshotInfo, SubmitRejection};
use hongsi_core::models::{ActiveLectures, Assignment, AttendanceCourse, Course, Notification, SemesterDisplay, SubmissionInfo, Timetable};
use hongsi_core::{api_error, CoreError, SchoolSession, SchoolSessionSnapshot};
pub use hongsi_core::submission;
use submission::{Job, Progress, Stage, Status, JOB_TIMEOUT};
use reqwest::Method;
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
        Self { status, body: json!({ "error": { "code": code, "message": message.into() } }) }
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
    calendar: Mutex<Option<(Instant, (CalendarData, Vec<Assignment>))>>,
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
    expires_at: Option<i64>,
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
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().expect("HTTP 클라이언트");
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

    pub fn server_reachable(&self) -> Option<bool> {
        match self.server_ok.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    pub async fn logged_in(&self) -> bool {
        self.school.read().await.is_some()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn session_revoked(&self) -> bool {
        self.revoked.load(Ordering::SeqCst)
    }

    fn revoked_reply() -> Reply {
        Reply::error(401, "session_revoked", "다시 로그인해 주세요.")
    }

    async fn current(&self) -> R<Arc<School>> {
        if self.session_revoked() {
            return Err(Self::revoked_reply());
        }
        self.school.read().await.clone().ok_or_else(|| Reply::error(401, "unauthorized", "로그인이 필요해요."))
    }

    pub async fn snapshot(&self) -> Option<AuthSnapshot> {
        if self.session_revoked() {
            return None;
        }
        let school = self.school.read().await;
        let school = school.as_ref()?;
        Some(AuthSnapshot {
            student_id: school.student_id.clone(),
            school: school.session.snapshot(),
            server_base: self.base.clone(),
            server: self.server_auth.lock().ok()?.clone(),
        })
    }

    pub async fn restore(&self, id: &str, snapshot: AuthSnapshot) -> bool {
        if snapshot.student_id != id.trim().to_uppercase() {
            return false;
        }
        let Ok(session) = SchoolSession::from_snapshot(snapshot.school) else { return false };
        let Some(server) = snapshot.server.filter(|auth| {
            snapshot.server_base == self.base && auth.token.len() == 64 && auth.token.bytes().all(|b| b.is_ascii_hexdigit())
        }) else { return false };
        let _guard = self.server_login_lock.lock().await;
        self.set_server_auth(Some(server));
        self.revoked.store(false, Ordering::SeqCst);
        self.remember.store(true, Ordering::SeqCst);
        *self.school.write().await = Some(Arc::new(School::new(session, snapshot.student_id)));
        self.generation.fetch_add(1, Ordering::SeqCst);
        true
    }

    pub async fn login(&self, id: &str, password: &str, remember: bool) -> Reply {
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let id = id.trim().to_uppercase();
        if id.is_empty() || password.is_empty() || id.len() > 32 {
            return Reply::error(400, "bad_request", "학번과 비밀번호를 입력해 주세요.");
        }
        let session = match SchoolSession::login(&id, password).await {
            Ok(s) => s,
            Err(e) => return e.into(),
        };
        let profile = session.profile().await.ok();
        let school = Arc::new(School::new(session, id));
        {
            let _guard = self.server_login_lock.lock().await;
            if self.session_revoked() {
                return Self::revoked_reply();
            }
            let same_account = self.school.read().await.as_ref().is_some_and(|current| current.student_id == school.student_id);
            if !same_account || self.remember.load(Ordering::SeqCst) != remember {
                let reply = self.revoke_server_login().await;
                if reply.status != 200 {
                    return reply;
                }
                self.set_server_auth(None);
            }
            self.remember.store(remember, Ordering::SeqCst);
            *self.school.write().await = Some(school.clone());
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
        if !self.server_auth_valid() {
            let previous = self.token();
            if let Err(e) = self.server_login(&school, previous.as_deref()).await {
                if e.code() == Some("session_revoked") {
                    return e;
                }
                tracing::warn!(status = e.status, "홍시 서버 기기 로그인 실패 — 학교 기능만 먼저 쓴다");
            }
        }
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let name = profile.map(|p| p.name).unwrap_or_default();
        Reply::ok(json!({ "profile": { "name": name } }))
    }

    pub async fn refresh_classroom(&self) -> Reply {
        let school = match self.current().await {
            Ok(school) => school,
            Err(reply) => return reply,
        };
        let session = match school.session.refresh_moodle().await {
            Ok(session) => session,
            Err(error) => return error.into(),
        };
        let _guard = self.server_login_lock.lock().await;
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let mut current = self.school.write().await;
        if !current.as_ref().is_some_and(|value| Arc::ptr_eq(value, &school)) {
            return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
        }
        *current = Some(Arc::new(School::new(session, school.student_id.clone())));
        self.generation.fetch_add(1, Ordering::SeqCst);
        Reply::ok(json!({ "ok": true }))
    }

    pub async fn revoke_login(&self) -> Reply {
        let _guard = self.server_login_lock.lock().await;
        self.revoke_server_login().await
    }

    async fn revoke_server_login(&self) -> Reply {
        if self.token().is_some() {
            let reply = self.send_server(&Method::POST, "/api/auth/logout", None).await;
            if reply.code() == Some("session_revoked") {
                return reply;
            }
            if reply.status != 200 || reply.body["ok"] != true {
                return Reply::error(503, "logout_cleanup_failed", "서버의 로그인 정보를 삭제하지 못했어요. 연결을 확인한 뒤 로그아웃을 다시 시도해 주세요.");
            }
        }
        Reply::ok(json!({ "ok": true }))
    }

    pub async fn revoke_all_logins(&self) -> Reply {
        let reply = self.server(&Method::POST, "/api/auth/logout-all", None).await;
        if reply.code() == Some("session_revoked") {
            return reply;
        }
        if reply.status != 200 || reply.body["ok"] != true {
            return Reply::error(503, "logout_cleanup_failed", "모든 기기의 로그인 정보를 삭제하지 못했어요. 연결을 확인한 뒤 다시 시도해 주세요.");
        }
        reply
    }

    pub async fn clear_login(&self) {
        let _guard = self.server_login_lock.lock().await;
        self.set_server_auth(None);
        self.revoked.store(false, Ordering::SeqCst);
        self.remember.store(false, Ordering::SeqCst);
        *self.school.write().await = None;
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.submissions.clear();
    }


    fn token(&self) -> Option<String> {
        self.server_auth.lock().ok().and_then(|auth| auth.as_ref().map(|auth| auth.token.clone()))
    }

    fn set_server_auth(&self, auth: Option<ServerAuth>) {
        if let Ok(mut value) = self.server_auth.lock() {
            *value = auth;
        }
    }

    fn server_auth_valid(&self) -> bool {
        self.server_auth.lock().ok().is_some_and(|auth| {
            auth.as_ref().is_some_and(|auth| auth.expires_at.is_none_or(|at| at > Utc::now().timestamp()))
        })
    }

    async fn server_login(&self, school: &Arc<School>, previous: Option<&str>) -> R<()> {
        let _guard = self.server_login_lock.lock().await;
        if self.session_revoked() {
            return Err(Self::revoked_reply());
        }
        if !self.school.read().await.as_ref().is_some_and(|current| Arc::ptr_eq(current, school)) {
            return Err(Reply::error(409, "account_changed", "로그인 상태가 변경됐어요."));
        }
        if self.token().as_deref() != previous && self.server_auth_valid() {
            return Ok(());
        }
        let token = school.session.moodle_token().await?;
        let r = self.send_server(&Method::POST, "/api/auth/device", Some(&json!({ "token": token, "remember": self.remember.load(Ordering::SeqCst) }))).await;
        if r.status == 401 && r.code() == Some("login_rejected") {
            return Err(CoreError::ClassroomTokenExpired.into());
        }
        if r.status != 200 {
            return Err(r);
        }
        if self.session_revoked() {
            return Err(Self::revoked_reply());
        }
        let token = r.body["token"].as_str().filter(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| Reply::error(502, "server_error", "서버의 로그인 응답을 확인하지 못했어요."))?;
        if !self.school.read().await.as_ref().is_some_and(|current| Arc::ptr_eq(current, school)) {
            let _ = self.http.post(format!("{}/api/auth/logout", self.base)).bearer_auth(token).header("X-Client", "tauri").send().await;
            return Err(Reply::error(409, "account_changed", "로그인 상태가 변경됐어요."));
        }
        self.set_server_auth(Some(ServerAuth { token: token.to_string(), expires_at: r.body["expiresAt"].as_i64() }));
        Ok(())
    }

    async fn send_server(&self, method: &Method, path: &str, body: Option<&Value>) -> Reply {
        let token = self.token();
        self.send_server_as(method, path, body, token.as_deref()).await
    }

    async fn send_server_as(&self, method: &Method, path: &str, body: Option<&Value>, token: Option<&str>) -> Reply {
        let mut request = self.http.request(method.clone(), format!("{}{path}", self.base)).header("X-Client", "tauri");
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        match request.send().await {
            Ok(response) => {
                self.server_ok.store(1, Ordering::Relaxed);
                let status = response.status().as_u16();
                let format = api_error::response_format(response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or(""));
                let body = response.json::<Value>().await.unwrap_or(Value::Null);
                let reply = if status >= 400 {
                    let (code, message) = api_error::fallback(status);
                    let code = body["error"]["code"].as_str().filter(|value| !value.trim().is_empty()).unwrap_or(code);
                    let message = body["error"]["message"].as_str().filter(|value| !value.trim().is_empty()).unwrap_or(message);
                    api_error::report(method.as_str(), path, status, code, format);
                    Reply::error(status, code, message)
                } else if !body.is_object() && !body.is_array() {
                    api_error::report(method.as_str(), path, status, "invalid_response", format);
                    Reply::error(502, "invalid_response", "서버 응답을 확인하지 못했어요.")
                } else { Reply { status, body } };
                if reply.status == 401 && reply.code() == Some("session_revoked") {
                    let auth = self.server_auth.lock().ok();
                    if token.is_none() || auth.as_ref().and_then(|auth| auth.as_ref().map(|auth| auth.token.as_str())) != token {
                        return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
                    }
                    self.revoked.store(true, Ordering::SeqCst);
                }
                reply
            }
            Err(e) => {
                self.server_ok.store(2, Ordering::Relaxed);
                let (code, message) = if e.is_timeout() { ("timeout", "응답이 늦어지고 있어요.") }
                    else { ("server_unreachable", "서버에 연결하지 못했어요.") };
                api_error::report(method.as_str(), path, 503, code, "none");
                Reply::error(503, code, message)
            }
        }
    }

    async fn server(&self, method: &Method, path: &str, body: Option<&Value>) -> Reply {
        let school = match self.current().await {
            Ok(school) => school,
            Err(reply) => return reply,
        };
        let previous = self.token();
        if previous.is_none() {
            if let Err(reply) = self.server_login(&school, previous.as_deref()).await {
                return reply;
            }
        }
        let previous = {
            let current = self.school.read().await;
            if !current.as_ref().is_some_and(|value| Arc::ptr_eq(value, &school)) {
                return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
            }
            self.token()
        };
        let first = self.send_server_as(method, path, body, previous.as_deref()).await;
        if first.status != 401 || first.code() != Some("unauthorized") {
            return first;
        }
        match self.server_login(&school, previous.as_deref()).await {
            Ok(()) => {
                let current = self.school.read().await;
                if !current.as_ref().is_some_and(|value| Arc::ptr_eq(value, &school)) {
                    return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
                }
                let token = self.token();
                drop(current);
                self.send_server_as(method, path, body, token.as_deref()).await
            }
            Err(reply) => reply,
        }
    }


    pub async fn request(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let reply = match self.route(method, path, body).await {
            Ok(reply) => reply,
            Err(reply) => reply,
        };
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        reply
    }

    async fn route(&self, method: &str, path: &str, body: Option<Value>) -> R<Reply> {
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let q: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let refresh = q.get("refresh").is_some_and(|v| v == "1");
        let method_upper = method.to_ascii_uppercase();
        let json_body = body.unwrap_or(Value::Null);

        match (method_upper.as_str(), route) {
            ("GET", "/api/me") => self.me().await.map(Reply::ok),
            ("GET", "/api/attendance/active") => {
                let s = self.current().await?;
                Ok(Reply::ok(json!(self.lectures(&s).await?)))
            }
            ("POST", "/api/attendance/submit") => self.submit_attendance(&json_body).await.map(Reply::ok),
            ("GET", "/api/attendance/status") => {
                let s = self.current().await?;
                Ok(Reply::ok(json!(s.session.attendance_status().await?)))
            }
            ("GET", "/api/attendance/course") => {
                let code = q.get("code").map(String::as_str).unwrap_or("");
                self.attendance_course(code).await.map(Reply::ok)
            }
            ("GET", "/api/timetable") => self.timetable(refresh).await.map(Reply::ok),
            ("GET", "/api/notifications") => self.notifications().await.map(Reply::ok),
            ("POST", "/api/background/session" | "/api/background/renew") => {
                if !self.base.starts_with("https://") && !self.base.starts_with("http://127.0.0.1:") {
                    return Err(Reply::error(400, "insecure_server", "학교 세션을 보관하려면 HTTPS 서버가 필요해요."));
                }
                if route == "/api/background/session" && json_body["consent"] != true {
                    return Err(Reply::error(400, "bad_request", "백그라운드 동기화 동의가 필요해요."));
                }
                let school = self.current().await?;
                let mut payload = json_body.clone();
                payload["cookies"] = json!(school.session.sso_cookies());
                Ok(self.server(&Method::POST, route, Some(&payload)).await)
            }
            ("GET", "/api/calendar") => {
                let display = match q.get("semester").map(String::as_str).unwrap_or("current") {
                    "current" => SemesterDisplay::Current,
                    "all" => SemesterDisplay::All,
                    _ => return Err(Reply::error(400, "bad_request", "학기 표시 설정을 확인해 주세요.")),
                };
                self.calendar(refresh, display).await.map(Reply::ok)
            }
            ("GET", "/api/meals") => Ok(self.meals().await),
            ("POST", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/verify") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/verify"));
                let course: i64 = q.get("course").and_then(|c| c.parse().ok()).unwrap_or(0);
                self.verify(&key, course).await.map(Reply::ok)
            }
            ("PUT", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/done") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/done"));
                Ok(self.set_done(&key, &json_body).await)
            }
            ("PUT", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/alert") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/alert"));
                Ok(self.set_alert(&key, &json_body).await)
            }
            ("PUT", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/alert-leads") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/alert-leads"));
                Ok(self.set_alert_leads(&key, &json_body).await)
            }
            ("GET", r) if r.starts_with("/api/assign/") && r.ends_with("/submission") => {
                let cmid: i64 = r
                    .trim_start_matches("/api/assign/")
                    .trim_end_matches("/submission")
                    .parse()
                    .map_err(|_| Reply::error(400, "bad_request", "잘못된 과제예요."))?;
                self.submission_view(cmid).await.map(Reply::ok)
            }
            ("GET", r) if r.starts_with("/api/modules/") => {
                let cmid = ids(r.trim_start_matches("/api/modules/"), 1)?[0];
                let s = self.current().await?;
                Ok(Reply::ok(json!(s.session.module_contents(cmid).await?)))
            }
            ("GET", r) if r.starts_with("/api/board/") => {
                let id = ids(r.trim_start_matches("/api/board/"), 2)?;
                let s = self.current().await?;
                Ok(Reply::ok(json!(s.session.board_article(id[0], id[1]).await?)))
            }
            _ => {
                let method = Method::from_bytes(method_upper.as_bytes())
                    .map_err(|_| Reply::error(400, "bad_request", "잘못된 요청 방식이에요."))?;
                let body = (!json_body.is_null()).then_some(&json_body);
                Ok(self.server(&method, path, body).await)
            }
        }
    }

    async fn me(&self) -> R<Value> {
        let s = self.current().await?;
        let p = s.session.profile().await?;
        Ok(json!({
            "profile": { "name": p.name, "hasPicture": p.has_picture, "studentId": s.student_id, "department": p.department },
            "remembered": false,
        }))
    }


    async fn lectures(&self, s: &School) -> R<ActiveLectures> {
        let mut slot = s.lectures.lock().await;
        if let Some(data) = cached(&slot, Duration::from_secs(3)) {
            return Ok(data);
        }
        let data = s.session.active_lectures().await?;
        *slot = Some((Instant::now(), data.clone()));
        Ok(data)
    }

    async fn submit_attendance(&self, b: &Value) -> R<Value> {
        let generation = self.generation();
        let s = self.current().await?;
        let key = b["lectureKey"].as_str().unwrap_or("");
        let code = b["code"].as_str().unwrap_or("").trim();
        let (lat, lon) = (b["latitude"].as_f64().unwrap_or(f64::NAN), b["longitude"].as_f64().unwrap_or(f64::NAN));
        if code.is_empty() || code.len() > 12 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Reply::error(400, "bad_request", "인증번호를 확인해 주세요."));
        }
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(Reply::error(400, "bad_request", "위치 정보가 올바르지 않아요."));
        }
        let submission = s.session.submit_attendance(key, code, lat, lon).await.map_err(|error| {
            let mut reply = Reply::from(error);
            if reply.status >= 500 { reply.body["error"]["message"] = json!("출석 결과를 확인하지 못했어요. 출석 상태를 확인해 주세요."); }
            reply
        })?;
        if self.generation() != generation { return Err(Reply::error(409, "account_changed", "이전 계정의 출석 요청이에요.")); }
        *s.lectures.lock().await = None;
        s.courses.lock().await.clear();
        let synced = if let Some(receipt) = &submission.receipt {
            let body = json!({"receipt":receipt,"account":s.student_id});
            tokio::time::timeout(Duration::from_secs(3), self.server(&Method::PUT, "/api/attendance/receipts", Some(&body))).await
                .is_ok_and(|response| response.status == 200 && response.body["ok"] == true)
        } else { true };
        Ok(json!({ "message": submission.message, "receipt": submission.receipt, "synced": synced }))
    }

    async fn attendance_course(&self, code: &str) -> R<Value> {
        let valid = code.len() <= 16
            && code.split_once('-').is_some_and(|(h, b)| {
                !h.is_empty() && !b.is_empty() && h.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit())
            });
        if !valid {
            return Err(Reply::error(400, "bad_request", "과목 코드가 올바르지 않아요."));
        }
        let s = self.current().await?;
        let mut map = s.courses.lock().await;
        if let Some((at, data)) = map.get(code) {
            if at.elapsed() < Duration::from_secs(60) {
                return Ok(json!(data));
            }
        }
        let data = s.session.attendance_course(code).await?;
        map.insert(code.to_string(), (Instant::now(), data.clone()));
        Ok(json!(data))
    }

    async fn timetable(&self, refresh: bool) -> R<Value> {
        let s = self.current().await?;
        let mut slot = s.timetable.lock().await;
        let max_age = Duration::from_secs(if refresh { 60 } else { 6 * 3600 });
        if let Some(data) = cached(&slot, max_age) {
            return Ok(json!(data));
        }
        let data = s.session.timetable().await?;
        *slot = Some((Instant::now(), data.clone()));
        Ok(json!(data))
    }


    async fn notifications(&self) -> R<Value> {
        let s = self.current().await?;
        let days = |w: &str| -> u32 {
            let n: u32 = w.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
            if w.contains('일') {
                n
            } else if w.contains('주') {
                n * 7
            } else if w.contains('달') || w.contains("개월") || w.contains('년') {
                365
            } else {
                0
            }
        };
        let mut all: Vec<Notification> = Vec::new();
        for page in 1..=6 {
            let items = s.session.notifications(page).await?;
            let mut stop = items.len() < 15;
            for n in items {
                if days(&n.when) >= 7 {
                    stop = true;
                    continue;
                }
                if !all.iter().any(|x| x.url == n.url && x.when == n.when) {
                    all.push(n);
                }
            }
            if stop {
                break;
            }
        }
        Ok(json!(all))
    }

    async fn calendar(&self, refresh: bool, display: SemesterDisplay) -> R<Value> {
        let s = self.current().await?;
        let mut slot = s.calendar.lock().await;
        if !refresh {
            if let Some((mut data, _)) = cached(&slot, Duration::from_secs(180))
                .filter(|(data, _)| data.semester_display == display) {
                let reply = self.server(&Method::GET, "/api/calendar/state", None).await;
                if reply.status == 200 {
                    if let Ok(state) = serde_json::from_value::<calendar::CalendarState>(reply.body) {
                        state.apply(&mut data.items);
                        if let Some((_, (cached_data, _))) = slot.as_mut() {
                            *cached_data = data.clone();
                        }
                    }
                }
                return Ok(json!(data));
            }
        }
        let (current_term, courses) = s.session.courses_for(display).await?;
        let (assignments, vods) = tokio::join!(s.session.assignments(&courses), s.session.vods(&courses));
        let (assignments, vods) = (assignments?, vods?);
        let (snapshots, checks, alerts_off, alert_leads) = self.calendar_state(&courses, &assignments).await;
        let now = Utc::now().timestamp();
        let data = CalendarData {
            semester_display: display,
            current_term,
            items: calendar::build(&assignments, &vods, &snapshots, &checks, &alerts_off, &alert_leads, now),
            courses,
            fetched_at: now,
        };
        *slot = Some((Instant::now(), (data.clone(), assignments)));
        Ok(json!(data))
    }

    async fn calendar_state(
        &self,
        courses: &[Course],
        assignments: &[Assignment],
    ) -> (HashMap<i64, SnapshotInfo>, HashMap<String, bool>, HashSet<String>, HashMap<String, Vec<i32>>) {
        let body = json!({ "courses": courses, "assignments": assignments });
        let r = self.server(&Method::POST, "/api/calendar/state", Some(&body)).await;
        if r.status != 200 {
            tracing::warn!(status = r.status, "서버 기록(완료 체크·첫 기록)을 받지 못해 학교 기준만 보여준다");
            return (HashMap::new(), HashMap::new(), HashSet::new(), HashMap::new());
        }
        let snapshots = serde_json::from_value(r.body["snapshots"].clone()).unwrap_or_default();
        let checks = serde_json::from_value(r.body["checks"].clone()).unwrap_or_default();
        let alerts_off = serde_json::from_value(r.body["alertsOff"].clone()).unwrap_or_default();
        let alert_leads = serde_json::from_value(r.body["alertLeads"].clone()).unwrap_or_default();
        (snapshots, checks, alerts_off, alert_leads)
    }

    async fn patch_items(&self, s: &School, key: &str, f: impl Fn(&mut calendar::CalendarItem)) {
        if let Some((_, (data, _))) = s.calendar.lock().await.as_mut() {
            data.items.iter_mut().filter(|i| i.key == key).for_each(f);
        }
    }

    async fn verify(&self, key: &str, course: i64) -> R<Value> {
        let s = self.current().await?;
        let bad = || Reply::error(400, "bad_request", "잘못된 항목이에요.");
        let (kind, id) = key.split_once(':').ok_or_else(bad)?;
        let cmid: i64 = id.parse().map_err(|_| bad())?;
        let status = match kind {
            "assign" => calendar::submission_status(s.session.submission_state(course, cmid).await?),
            "vod" => calendar::vod_status(s.session.vod_state(course, cmid).await?),
            _ => return Err(Reply::error(400, "bad_request", "과제와 강의만 확인할 수 있어요.")),
        };
        if status != "unknown" {
            self.patch_items(&s, key, |item| {
                item.status = status;
                if item.done_override.is_none() {
                    item.done = status == "submitted" || status == "done";
                }
            })
            .await;
        }
        Ok(json!({ "key": key, "status": status, "finished": status == "submitted" || status == "done" }))
    }

    async fn set_done(&self, key: &str, body: &Value) -> Reply {
        let path = format!("/api/calendar/items/{}/done", encode(key));
        let reply = self.server(&Method::PUT, &path, Some(body)).await;
        if reply.status == 200 {
            if let Ok(s) = self.current().await {
                self.patch_items(&s, key, |item| {
                    item.done_override = body["done"].as_bool();
                    item.done = item.done_override.unwrap_or(item.status == "submitted" || item.status == "done");
                })
                .await;
            }
        }
        reply
    }

    async fn set_alert(&self, key: &str, body: &Value) -> Reply {
        let path = format!("/api/calendar/items/{}/alert", encode(key));
        let reply = self.server(&Method::PUT, &path, Some(body)).await;
        if reply.status == 200 {
            if let (Ok(s), Some(on)) = (self.current().await, body["on"].as_bool()) {
                self.patch_items(&s, key, |item| item.alert = on).await;
            }
        }
        reply
    }

    async fn set_alert_leads(&self, key: &str, body: &Value) -> Reply {
        let path = format!("/api/calendar/items/{}/alert-leads", encode(key));
        let reply = self.server(&Method::PUT, &path, Some(body)).await;
        if reply.status == 200 {
            if let Ok(s) = self.current().await {
                let leads: Option<Vec<i32>> = serde_json::from_value(reply.body["leads"].clone()).unwrap_or_default();
                self.patch_items(&s, key, |item| item.alert_leads = leads.clone()).await;
            }
        }
        reply
    }

    async fn find_assignment(&self, s: &School, cmid: i64) -> R<Assignment> {
        let cached = s
            .calendar
            .lock()
            .await
            .as_ref()
            .and_then(|(_, (_, list))| list.iter().find(|a| a.cmid == cmid).cloned());
        if let Some(a) = cached {
            return Ok(a);
        }
        let courses = s.session.all_courses().await?;
        s.session
            .assignments(&courses)
            .await?
            .into_iter()
            .find(|a| a.cmid == cmid)
            .ok_or_else(|| Reply::error(404, "not_found", "과제를 찾지 못했어요."))
    }

    fn view(a: &Assignment, info: &SubmissionInfo) -> Value {
        let now = Utc::now().timestamp();
        json!({
            "info": info,
            "config": a.config,
            "due": a.due,
            "cutoff": a.cutoff,
            "late": a.due.is_some_and(|d| now > d),
            "closed": a.cutoff.is_some_and(|c| now > c) || info.locked || !info.can_edit,
        })
    }

    async fn submission_view(&self, cmid: i64) -> R<Value> {
        let s = self.current().await?;
        let a = self.find_assignment(&s, cmid).await?;
        let info = s.session.submission_info(a.id).await?;
        Ok(Self::view(&a, &info))
    }

    pub async fn submit(
        &self,
        cmid: i64,
        keep: Vec<String>,
        files: Vec<(String, Vec<u8>)>,
        late_confirmed: bool,
        accept_statement: bool,
    ) -> Reply {
        match self.submit_inner(cmid, keep, files, late_confirmed, accept_statement, None).await {
            Ok(v) => Reply::ok(v),
            Err(r) => r,
        }
    }

    pub async fn begin_submission(&self, cmid: i64, id: &str) -> Result<(Arc<Job>, bool), Reply> {
        let school = self.current().await?;
        self.submissions.start(&school.student_id, cmid, id).map_err(|message| Reply::error(409, "conflict", message))
    }

    pub async fn submit_job(
        &self, cmid: i64, keep: Vec<String>, files: Vec<(String, Vec<u8>)>, late: bool, statement: bool, job: Arc<Job>,
    ) -> Reply {
        match tokio::time::timeout(JOB_TIMEOUT, self.submit_inner(cmid, keep, files, late, statement, Some(job.clone()))).await {
            Ok(Ok(result)) => job.complete(result),
            Ok(Err(reply)) => job.fail(reply.status, reply.code().unwrap_or("school_error"), reply.body["error"]["message"].as_str().unwrap_or("제출 결과를 확인하지 못했어요.")),
            Err(_) => job.fail(504, "timeout", "학교의 응답이 늦어지고 있어요."),
        }
        Reply::ok(json!(job.snapshot()))
    }

    pub async fn submission_status(&self, cmid: i64, id: &str, verify: bool) -> Reply {
        let result = async {
            let school = self.current().await?;
            let job = self.submissions.get(&school.student_id, cmid, id)
                .ok_or_else(|| Reply::error(404, "not_found", "제출 진행 기록을 찾지 못했어요. 클래스룸에서 제출 상태를 확인해 주세요."))?;
            if verify && job.snapshot().status == Status::Uncertain {
                let assignment = self.find_assignment(&school, cmid).await?;
                let info = school.session.submission_info(assignment.id).await?;
                if job.matches(&info) {
                    let key = format!("assign:{cmid}");
                    let _ = self.set_done(&key, &json!({ "done": true })).await;
                    self.patch_items(&school, &key, |item| { item.status = "submitted"; item.done = true; }).await;
                    job.complete(Self::view(&assignment, &info));
                }
            }
            Ok(json!(job.snapshot()))
        }.await;
        match result { Ok(value) => Reply::ok(value), Err(reply) => reply }
    }

    async fn submit_inner(
        &self,
        cmid: i64,
        keep: Vec<String>,
        files: Vec<(String, Vec<u8>)>,
        late_confirmed: bool,
        accept_statement: bool,
        job: Option<Arc<Job>>,
    ) -> R<Value> {
        let s = self.current().await?;
        let a = self.find_assignment(&s, cmid).await?;
        let info = s.session.submission_info(a.id).await?;
        let sizes: Vec<(String, usize)> = files.iter().map(|(n, b)| (n.clone(), b.len())).collect();
        calendar::check_submission(&a, &info, Utc::now().timestamp(), &keep, &sizes, late_confirmed, accept_statement)
            .map_err(rejection)?;
        let mut all = Vec::with_capacity(keep.len() + files.len());
        for name in &keep {
            let existing = info
                .files
                .iter()
                .find(|f| &f.name == name)
                .ok_or_else(|| Reply::error(400, "bad_request", format!("'{name}' 파일을 찾을 수 없어요.")))?;
            let (_, bytes) = s.session.download(&existing.url, 1024 * MB).await?;
            all.push((existing.name.clone(), bytes));
        }
        all.extend(files);
        if let Some(job) = &job { job.expect(&info, &all); }
        s.session.submit_files_with_progress(a.id, all, a.config.drafts, accept_statement, job.clone()).await?;
        if let Some(job) = &job {
            job.acknowledge();
            job.progress(Progress::at(Stage::Verify));
        }
        let info = s.session.submission_info(a.id).await?;
        if job.as_ref().is_some_and(|job| !job.matches(&info)) {
            return Err(Reply::error(502, "submission_unconfirmed", "제출 상태와 파일 목록이 아직 확인되지 않았어요."));
        }
        tracing::info!(cmid, "과제 제출 완료 (기기)");
        let key = format!("assign:{cmid}");
        let _ = self.set_done(&key, &json!({ "done": true })).await;
        self.patch_items(&s, &key, |item| {
            item.status = "submitted";
            item.done = true;
        })
        .await;
        Ok(Self::view(&a, &info))
    }

    pub async fn attachment(&self, cmid: i64, index: usize) -> Result<(String, Vec<u8>), Reply> {
        let s = self.current().await?;
        let a = self.find_assignment(&s, cmid).await?;
        let file = a.attachments.get(index).cloned().ok_or_else(|| Reply::error(404, "not_found", "파일을 찾지 못했어요."))?;
        let (_, bytes) = s.session.download(&file.url, 1024 * MB).await?;
        Ok((file.name, bytes))
    }

    pub async fn module_file(&self, cmid: i64, index: usize) -> Result<(String, Vec<u8>), Reply> {
        let s = self.current().await?;
        let m = s.session.module_contents(cmid).await?;
        let file = m.files.get(index).cloned().ok_or_else(|| Reply::error(404, "not_found", "파일을 찾지 못했어요."))?;
        let (_, bytes) = s.session.download(&file.url, 1024 * MB).await?;
        Ok((file.name, bytes))
    }

    pub async fn board_file(&self, cmid: i64, bwid: i64, index: usize) -> Result<(String, Vec<u8>), Reply> {
        let s = self.current().await?;
        let article = s.session.board_article(cmid, bwid).await?;
        let file =
            article.attachments.get(index).cloned().ok_or_else(|| Reply::error(404, "not_found", "파일을 찾지 못했어요."))?;
        let (_, bytes) = s.session.download(&file.url, 1024 * MB).await?;
        Ok((file.name, bytes))
    }

    pub async fn avatar(&self) -> Option<(String, Vec<u8>)> {
        let s = self.current().await.ok()?;
        s.session.avatar().await.ok().flatten().filter(|(mime, _)| mime.starts_with("image/"))
    }

    pub async fn browser_url(&self, url: &str) -> String {
        if !url.starts_with(CN2) {
            return url.to_string();
        }
        let Ok(s) = self.current().await else { return url.to_string() };
        if s.autologin_at.lock().ok().and_then(|t| *t).is_some_and(|t| t.elapsed() < AUTOLOGIN_GAP) {
            return url.to_string();
        }
        match s.session.autologin_url(url).await {
            Ok(link) => {
                if let Ok(mut t) = s.autologin_at.lock() {
                    *t = Some(Instant::now());
                }
                link
            }
            Err(e) => {
                tracing::warn!("클래스룸 자동 로그인 주소를 받지 못함: {e}");
                url.to_string()
            }
        }
    }


    async fn meals(&self) -> Reply {
        let reply = self.send_server(&Method::GET, "/api/meals", None).await;
        if reply.status == 200 {
            return reply;
        }
        match hongsi_core::food::fetch_week(&hongsi_core::public_client()).await {
            Ok(days) => Reply::ok(json!(days)),
            Err(e) => e.into(),
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
    url::form_urlencoded::byte_serialize(segment.as_bytes()).collect::<String>().replace('+', "%20")
}

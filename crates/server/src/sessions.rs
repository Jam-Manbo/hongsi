use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use chrono::{DateTime, Duration as Span, Utc};
use hongsi_core::models::{ActiveLectures, AttendanceCourse, Timetable};
use hongsi_core::SchoolSession;

use crate::calendar::CalendarData;
use crate::error::ApiError;

pub const REMEMBER_DAYS: i32 = 14;
pub const SESSION_HOURS: i64 = 12;

pub struct UserSession {
    pub school: Option<SchoolSession>,
    pub user_id: i64,
    pub name: String,
    pub student_id: String,
    pub remembered: bool,
    pub expires_at: DateTime<Utc>,
    last_used: Mutex<Instant>,
    pub calendar_cache: tokio::sync::Mutex<Option<(Instant, CalendarData)>>,
    pub lectures_cache: tokio::sync::Mutex<Option<(Instant, ActiveLectures)>>,
    pub timetable_cache: tokio::sync::Mutex<Option<(Instant, Timetable)>>,
    pub course_cache: tokio::sync::Mutex<HashMap<String, (Instant, AttendanceCourse)>>,
    pub device_courses: Mutex<Vec<i64>>,
}

impl UserSession {
    pub fn new(school: Option<SchoolSession>, user_id: i64, name: String, student_id: String, remembered: bool) -> Self {
        Self {
            school,
            user_id,
            name,
            student_id,
            remembered,
            expires_at: Utc::now() + if remembered { Span::days(REMEMBER_DAYS as i64) } else { Span::hours(SESSION_HOURS) },
            last_used: Mutex::new(Instant::now()),
            calendar_cache: tokio::sync::Mutex::new(None),
            lectures_cache: tokio::sync::Mutex::new(None),
            timetable_cache: tokio::sync::Mutex::new(None),
            course_cache: tokio::sync::Mutex::new(HashMap::new()),
            device_courses: Mutex::new(Vec::new()),
        }
    }

    pub fn school(&self) -> Result<&SchoolSession, ApiError> {
        self.school.as_ref().ok_or_else(|| {
            ApiError::new(StatusCode::CONFLICT, "device_session", "앱에서는 학교 요청을 기기에서 직접 보내요")
        })
    }

    fn touch(&self) {
        *self.last_used.lock().expect("세션 잠금") = Instant::now();
    }

    fn idle(&self) -> Duration {
        self.last_used.lock().expect("세션 잠금").elapsed()
    }
}

#[derive(Default)]
pub struct Sessions {
    inner: Mutex<HashMap<String, Arc<UserSession>>>,
}

impl Sessions {
    pub fn insert(&self, session: UserSession) -> String {
        let token = hex::encode(rand::random::<[u8; 32]>());
        self.inner.lock().expect("세션 잠금").insert(token.clone(), Arc::new(session));
        token
    }

    pub fn restore(&self, token: &str, session: UserSession) -> Arc<UserSession> {
        let session = Arc::new(session);
        self.inner.lock().expect("세션 잠금").insert(token.to_string(), session.clone());
        session
    }

    pub fn get(&self, token: &str) -> Option<Arc<UserSession>> {
        let mut map = self.inner.lock().expect("세션 잠금");
        if map.get(token).is_some_and(|session| session.expires_at <= Utc::now()) {
            map.remove(token);
            return None;
        }
        let session = map.get(token).cloned();
        if let Some(s) = &session {
            s.touch();
        }
        session
    }

    pub fn remove(&self, token: &str) {
        self.inner.lock().expect("세션 잠금").remove(token);
    }

    pub fn sweep(&self, max_idle: Duration) -> usize {
        let mut map = self.inner.lock().expect("세션 잠금");
        let before = map.len();
        let now = Utc::now();
        map.retain(|_, s| s.expires_at > now && s.idle() < max_idle);
        before - map.len()
    }
}

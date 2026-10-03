use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock, Weak};
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
    pub school: RwLock<Option<Arc<SchoolSession>>>,
    pub maintenance: tokio::sync::Mutex<Option<hongsi_core::SchoolSessionSnapshot>>,
    pub recovery: tokio::sync::Mutex<HashMap<String, (Instant, bool)>>,
    pub user_id: i64,
    pub name: String,
    pub student_id: String,
    pub remembered: bool,
    pub expires_at: Mutex<DateTime<Utc>>,
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
            school: RwLock::new(school.map(Arc::new)),
            maintenance: tokio::sync::Mutex::new(None),
            recovery: tokio::sync::Mutex::new(HashMap::new()),
            user_id,
            name,
            student_id,
            remembered,
            expires_at: Mutex::new(Utc::now() + if remembered { Span::days(REMEMBER_DAYS as i64) } else { Span::hours(SESSION_HOURS) }),
            last_used: Mutex::new(Instant::now()),
            calendar_cache: tokio::sync::Mutex::new(None),
            lectures_cache: tokio::sync::Mutex::new(None),
            timetable_cache: tokio::sync::Mutex::new(None),
            course_cache: tokio::sync::Mutex::new(HashMap::new()),
            device_courses: Mutex::new(Vec::new()),
        }
    }

    pub fn school(&self) -> Result<Arc<SchoolSession>, ApiError> {
        self.school.read().expect("세션 잠금").clone().ok_or_else(|| {
            ApiError::new(StatusCode::CONFLICT, "device_session", "앱에서는 학교 요청을 기기에서 직접 보내요")
        })
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        *self.expires_at.lock().expect("세션 잠금")
    }

    pub fn replace_school(&self, school: SchoolSession) {
        *self.school.write().expect("세션 잠금") = Some(Arc::new(school));
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
    access: Mutex<HashMap<String, Weak<tokio::sync::RwLock<()>>>>,
    accounts: Mutex<HashMap<i64, Weak<tokio::sync::RwLock<()>>>>,
}

impl Sessions {
    pub fn access(&self, token: &str) -> Arc<tokio::sync::RwLock<()>> {
        let mut access = self.access.lock().expect("세션 잠금");
        access.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = access.get(token).and_then(Weak::upgrade) { return lock; }
        let lock = Arc::new(tokio::sync::RwLock::new(()));
        access.insert(token.to_string(), Arc::downgrade(&lock));
        lock
    }

    pub fn account_access(&self, user_id: i64) -> Arc<tokio::sync::RwLock<()>> {
        let mut accounts = self.accounts.lock().expect("세션 잠금");
        accounts.retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = accounts.get(&user_id).and_then(Weak::upgrade) { return lock; }
        let lock = Arc::new(tokio::sync::RwLock::new(()));
        accounts.insert(user_id, Arc::downgrade(&lock));
        lock
    }

    pub fn remove_hashes(&self, hashes: &[String]) {
        self.inner.lock().expect("세션 잠금").retain(|token, _| !hashes.contains(&crate::vault::token_hash(token)));
    }

    pub fn remove_user(&self, user_id: i64) {
        self.inner.lock().expect("세션 잠금").retain(|_, session| session.user_id != user_id);
    }

    pub fn insert(&self, session: UserSession) -> String {
        let token = hex::encode(rand::random::<[u8; 32]>());
        self.inner.lock().expect("세션 잠금").insert(token.clone(), Arc::new(session));
        token
    }

    pub fn restore(&self, token: &str, session: UserSession) -> Arc<UserSession> {
        let session = Arc::new(session);
        self.inner.lock().expect("세션 잠금").entry(token.to_string()).or_insert(session).clone()
    }

    pub fn get(&self, token: &str) -> Option<Arc<UserSession>> {
        let mut map = self.inner.lock().expect("세션 잠금");
        if map.get(token).is_some_and(|session| session.expires_at() <= Utc::now()) {
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
        map.retain(|_, s| s.expires_at() > now && s.idle() < max_idle);
        before - map.len()
    }
}

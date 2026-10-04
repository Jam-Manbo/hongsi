use std::sync::Arc;

use axum::extract::FromRequestParts;
use axum::http::header;
use axum::http::request::Parts;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::ApiError;
use crate::sessions::UserSession;
use crate::state::Shared;
use crate::{db, vault};

pub const COOKIE: &str = "hsid";

pub struct CurrentUser {
    pub session: Arc<UserSession>,
    pub token: String,
    _access: tokio::sync::OwnedRwLockReadGuard<()>,
    _account_access: tokio::sync::OwnedRwLockReadGuard<()>,
}

pub struct LoginToken(pub String);

impl FromRequestParts<Shared> for LoginToken {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &Shared) -> Result<Self, Self::Rejection> {
        token_from(parts).map(Self).ok_or_else(ApiError::unauthorized)
    }
}

impl FromRequestParts<Shared> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let token = token_from(parts).ok_or_else(ApiError::unauthorized)?;
        let access = state.sessions.access(&token).read_owned().await;
        let hash = vault::token_hash(&token);
        let uid = match state.sessions.get(&token) {
            Some(session) => session.user_id,
            None => db::login_record(&state.db, &hash).await?.ok_or_else(ApiError::session_revoked)?.user_id,
        };
        let account_access = state.sessions.account_access(uid).read_owned().await;
        let record = db::login_record(&state.db, &hash).await?.ok_or_else(ApiError::session_revoked)?;
        if record.revoked_at.is_some() {
            state.sessions.remove(&token);
            return Err(ApiError::session_revoked());
        }
        if record.user_id != uid || record.expires_at <= chrono::Utc::now() {
            state.sessions.remove(&token);
            return Err(ApiError::unauthorized());
        }
        let session = match state.sessions.get(&token) {
            Some(session) => session,
            None => restore(state, &token).await?.ok_or_else(ApiError::unauthorized)?,
        };
        if let Some(accessed) = parts.extensions.get::<AccessedSession>() {
            *accessed.0.lock().expect("세션 잠금") = Some((token.clone(), session.clone()));
        }
        Ok(Self { session, token, _access: access, _account_access: account_access })
    }
}

async fn restore(state: &Shared, token: &str) -> Result<Option<Arc<UserSession>>, ApiError> {
    let hash = vault::token_hash(token);
    let Some((user_id, nonce, ciphertext, expires_at)) = db::load_remembered(&state.db, &hash).await? else { return Ok(None) };
    let Some(sealed) = vault::open(&state.pepper, token, &nonce, &ciphertext) else {
        db::delete_remembered(&state.db, &hash).await?;
        return Ok(None);
    };
    let school = if sealed.device { None } else { Some(sealed.school_session()?) };
    tracing::info!("보관된 로그인 세션 복원");
    let session = UserSession::new(school, user_id, sealed.name, sealed.student_id, true);
    *session.expires_at.lock().expect("세션 잠금") = expires_at;
    if session.expires_at() <= chrono::Utc::now() {
        return Ok(None);
    }
    Ok(Some(state.sessions.restore(token, session)))
}

fn token_from(parts: &Parts) -> Option<String> {
    token_from_headers(&parts.headers)
}

pub fn token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(token) = value.strip_prefix("Bearer ") {
            return valid_token(token.trim());
        }
    }
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(&format!("{COOKIE}=")).and_then(valid_token))
}

fn valid_token(token: &str) -> Option<String> {
    (token.len() == 64 && token.bytes().all(|c| c.is_ascii_hexdigit())).then(|| token.to_string())
}

pub fn user_key(pepper: &[u8], student_id: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(pepper).expect("HMAC 키");
    mac.update(student_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[derive(Clone, Default)]
struct AccessedSession(Arc<std::sync::Mutex<Option<(String, Arc<UserSession>)>>>);

pub async fn maintain_session(
    axum::extract::State(state): axum::extract::State<Shared>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let accessed = AccessedSession::default();
    let cookie_request = !request.headers().contains_key(header::AUTHORIZATION);
    request.extensions_mut().insert(accessed.clone());
    let mut response = next.run(request).await;
    let session = accessed.0.lock().expect("세션 잠금").take();
    if let Some((token, session)) = session {
        match persist_web(&state, &token, &session).await {
            Ok(Some(expires)) if cookie_request => {
                let secure = if state.config.cookie_secure { "; Secure" } else { "" };
                let remaining = (expires - chrono::Utc::now()).num_seconds().max(0);
                let cookie = format!("{COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={remaining}{secure}");
                if let Ok(value) = cookie.parse() { response.headers_mut().append(header::SET_COOKIE, value); }
            }
            Err(_) => tracing::warn!("자동 로그인 정보 갱신 실패"),
            _ => {}
        }
        response.headers_mut().insert(header::CACHE_CONTROL, axum::http::HeaderValue::from_static("private, no-store"));
    }
    response
}

pub async fn persist_web(state: &Shared, token: &str, session: &Arc<UserSession>) -> Result<Option<chrono::DateTime<chrono::Utc>>, ApiError> {
    if !session.remembered || session.school().is_err() { return Ok(None); }
    let _access = state.sessions.access(token).read_owned().await;
    let _account = state.sessions.account_access(session.user_id).read_owned().await;
    let mut persisted = session.maintenance.lock().await;
    let hash = vault::token_hash(token);
    let Some(record) = db::login_record(&state.db, &hash).await? else { return Ok(None); };
    let now = chrono::Utc::now();
    if record.revoked_at.is_some() || record.expires_at <= now { return Ok(None); }
    let school = session.school()?;
    let snapshot = school.snapshot();
    let changed = persisted.as_ref() != Some(&snapshot);
    let extend = record.expires_at < now + chrono::Duration::days(crate::sessions::REMEMBER_DAYS as i64) - chrono::Duration::hours(1);
    let expires = if extend { now + chrono::Duration::days(crate::sessions::REMEMBER_DAYS as i64) } else { record.expires_at };
    if changed || extend {
        let sealed = vault::Sealed {
            device: false, name: session.name.clone(), student_id: session.student_id.clone(),
            cookies: school.sso_cookies().to_vec(), school: Some(snapshot.clone()),
        };
        let (nonce, ciphertext) = vault::seal(&state.pepper, token, &sealed)
            .ok_or_else(|| ApiError::conflict("로그인 상태를 저장하지 못했어요."))?;
        let mut tx = state.db.begin().await?;
        sqlx::query("select pg_advisory_xact_lock($1)").bind(-session.user_id).execute(&mut *tx).await?;
        let saved = sqlx::query("update remembered_sessions r set nonce=$2,ciphertext=$3,expires_at=$4 from auth_sessions a where r.token_hash=$1 and a.token_hash=r.token_hash and a.revoked_at is null and a.expires_at>now()")
            .bind(&hash).bind(nonce).bind(ciphertext).bind(expires).execute(&mut *tx).await?;
        if saved.rows_affected() == 0 { return Ok(None); }
        sqlx::query("update auth_sessions set expires_at=$2 where token_hash=$1 and revoked_at is null and expires_at>now()")
            .bind(&hash).bind(expires).execute(&mut *tx).await?;
        if changed {
            let devices: Vec<String> = sqlx::query_scalar("select id from background_devices where user_id=$1 and session_hash=$2 and expires_at>now()")
                .bind(session.user_id).bind(&hash).fetch_all(&mut *tx).await?;
            for device in devices {
                let (nonce, ciphertext) = vault::seal(&state.pepper, &format!("background:{}:{device}", session.user_id), &sealed)
                    .ok_or_else(|| ApiError::conflict("학교 연결 정보를 저장하지 못했어요."))?;
                sqlx::query("update background_devices set nonce=$2,ciphertext=$3 where id=$1 and user_id=$4")
                    .bind(device).bind(nonce).bind(ciphertext).bind(session.user_id).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        *persisted = Some(snapshot);
    }
    *session.expires_at.lock().expect("세션 잠금") = expires;
    Ok(Some(expires))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryKind { Classroom, ClassroomToken, Attendance, Timetable }

#[derive(serde::Deserialize)]
pub struct RecoveryBody { kind: RecoveryKind }

fn school_reauth() -> ApiError {
    ApiError::new(axum::http::StatusCode::CONFLICT, "school_reauth_required", "학교에 다시 로그인해 주세요.")
}

pub async fn recover(
    user: CurrentUser,
    axum::Json(body): axum::Json<RecoveryBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    let key = match body.kind { RecoveryKind::Classroom => "classroom", RecoveryKind::ClassroomToken => "classroom_token", RecoveryKind::Attendance => "attendance", RecoveryKind::Timetable => "timetable" };
    let mut recovery = user.session.recovery.lock().await;
    if let Some((_, ok)) = recovery.get(key).filter(|(at, _)| at.elapsed() < std::time::Duration::from_secs(30)) {
        return if *ok { Ok(axum::Json(serde_json::json!({ "ok": true }))) } else { Err(school_reauth()) };
    }
    let old = user.session.school()?;
    let restored = async {
        let school = match body.kind {
            RecoveryKind::ClassroomToken => old.refresh_moodle().await?,
            _ => hongsi_core::SchoolSession::from_snapshot(old.snapshot())?,
        };
        match body.kind {
            RecoveryKind::Attendance => { school.active_lectures().await?; }
            RecoveryKind::Timetable => { school.timetable().await?; }
            RecoveryKind::Classroom => { school.restore_classroom_web().await?; }
            RecoveryKind::ClassroomToken => {}
        }
        Ok::<_, hongsi_core::CoreError>(school)
    }.await;
    match restored {
        Ok(school) => {
            user.session.replace_school(school);
            recovery.insert(key.into(), (std::time::Instant::now(), true));
            Ok(axum::Json(serde_json::json!({ "ok": true })))
        }
        Err(hongsi_core::CoreError::SessionExpired | hongsi_core::CoreError::ClassroomTokenExpired) => {
            recovery.insert(key.into(), (std::time::Instant::now(), false));
            Err(school_reauth())
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(serde::Deserialize)]
pub struct ReconnectBody { password: String }

pub async fn reconnect(
    axum::extract::State(state): axum::extract::State<Shared>,
    user: CurrentUser,
    axum::Json(body): axum::Json<ReconnectBody>,
) -> Result<axum::Json<serde_json::Value>, ApiError> {
    user.session.school()?;
    if body.password.is_empty() { return Err(ApiError::bad_request("비밀번호를 입력해 주세요.")); }
    state.login_attempts.check(&user_key(&state.pepper, &user.session.student_id))?;
    let mut recovery = user.session.recovery.lock().await;
    let school = hongsi_core::SchoolSession::login(&user.session.student_id, &body.password).await?;
    let _ = school.profile().await;
    user.session.replace_school(school);
    recovery.clear();
    *user.session.lectures_cache.lock().await = None;
    user.session.course_cache.lock().await.clear();
    Ok(axum::Json(serde_json::json!({ "ok": true })))
}

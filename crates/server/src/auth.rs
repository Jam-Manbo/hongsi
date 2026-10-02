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
    let school = if sealed.device { None } else { Some(hongsi_core::SchoolSession::from_sso_cookies(sealed.cookies)?) };
    tracing::info!("보관된 로그인 세션 복원");
    let mut session = UserSession::new(school, user_id, sealed.name, sealed.student_id, true);
    session.expires_at = expires_at;
    if session.expires_at <= chrono::Utc::now() {
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

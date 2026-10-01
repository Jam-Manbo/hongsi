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
}

impl FromRequestParts<Shared> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &Shared) -> Result<Self, Self::Rejection> {
        let token = token_from(parts).ok_or_else(ApiError::unauthorized)?;
        if let Some(session) = state.sessions.get(&token) {
            return Ok(Self { session, token });
        }
        let session = restore(state, &token).await?.ok_or_else(ApiError::unauthorized)?;
        Ok(Self { session, token })
    }
}

async fn restore(state: &Shared, token: &str) -> Result<Option<Arc<UserSession>>, ApiError> {
    let hash = vault::token_hash(token);
    let Some((user_id, nonce, ciphertext, expires_at)) = db::load_remembered(&state.db, &hash).await? else { return Ok(None) };
    let Some(sealed) = vault::open(&state.pepper, token, &nonce, &ciphertext) else {
        db::delete_remembered(&state.db, &hash).await?;
        return Ok(None);
    };
    let school = hongsi_core::SchoolSession::from_sso_cookies(sealed.cookies)?;
    tracing::info!("보관된 로그인 세션 복원");
    let mut session = UserSession::new(Some(school), user_id, sealed.name, sealed.student_id, true);
    session.expires_at = expires_at;
    if session.expires_at <= chrono::Utc::now() {
        return Ok(None);
    }
    Ok(Some(state.sessions.restore(token, session)))
}

fn token_from(parts: &Parts) -> Option<String> {
    if let Some(value) = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        if let Some(token) = value.strip_prefix("Bearer ") {
            return Some(token.trim().to_string());
        }
    }
    parts
        .headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(&format!("{COOKIE}=")).map(str::to_string))
}

pub fn user_key(pepper: &[u8], student_id: &str) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(pepper).expect("HMAC 키");
    mac.update(student_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

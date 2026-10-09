use axum::extract::State;
use axum::http::{header, HeaderMap};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use hongsi_core::SchoolSession;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, CurrentUser, LoginToken};
use crate::db;
use crate::error::ApiError;
use crate::sessions::{UserSession, REMEMBER_DAYS, SESSION_HOURS};
use crate::state::Shared;
use crate::vault;

use crate::api::ApiResult;

#[derive(Deserialize)]
pub(crate) struct LoginBody {
    id: String,
    password: String,
    remember: bool,
}

pub(crate) async fn login(State(st): State<Shared>, headers: HeaderMap, Json(body): Json<LoginBody>) -> Result<Response, ApiError> {
    let id = body.id.trim().to_uppercase();
    if id.is_empty() || body.password.is_empty() || id.len() > 32 {
        return Err(ApiError::bad_request("학번과 비밀번호를 입력해 주세요."));
    }
    st.login_attempts.check(&auth::user_key(&st.pepper, &id))?;
    let school = SchoolSession::login(&id, &body.password).await?;
    let name = match school.profile().await {
        Ok(p) => p.name,
        Err(e) => {
            tracing::warn!("프로필 조회 실패: {e}");
            String::new()
        }
    };
    let user_id = db::upsert_user(&st.db, &auth::user_key(&st.pepper, &id)).await?;
    let _account_access = st.sessions.account_access(user_id).read_owned().await;
    let sealed = vault::Sealed {
        device: false,
        name: name.clone(),
        student_id: id.clone(),
        school: Some(school.snapshot()),
    };
    let session = UserSession::new(Some(school), user_id, name.clone(), id.clone(), body.remember);
    let expires_at = session.expires_at();
    let token = st.sessions.insert(session);
    let encrypted = if body.remember {
        let Some(encrypted) = vault::seal(&st.pepper, &token, &sealed) else {
            st.sessions.remove(&token);
            return Err(ApiError::conflict("로그인 상태를 저장하지 못했어요."));
        };
        Some(encrypted)
    } else {
        None
    };
    if let Err(error) = db::save_login(
        &st.db,
        &vault::token_hash(&token),
        user_id,
        encrypted.as_ref().map(|(nonce, data)| (nonce.as_slice(), data.as_slice())),
        expires_at,
        None,
    )
    .await
    {
        st.sessions.remove(&token);
        return Err(error.into());
    }

    let mut payload = json!({ "profile": { "name": name } });
    if headers.get("x-client").and_then(|v| v.to_str().ok()) == Some("tauri") {
        payload["token"] = json!(token);
    }
    let secure = if st.config.cookie_secure { "; Secure" } else { "" };
    let max_age = if body.remember {
        REMEMBER_DAYS as i64 * 86_400
    } else {
        SESSION_HOURS * 3600
    };
    let cookie = format!(
        "{}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{secure}",
        auth::COOKIE
    );
    Ok(([(header::SET_COOKIE, cookie)], Json(payload)).into_response())
}

pub(crate) async fn logout(State(st): State<Shared>, LoginToken(token): LoginToken) -> Result<Response, ApiError> {
    let _access = st.sessions.access(&token).write_owned().await;
    let record = db::login_record(&st.db, &vault::token_hash(&token)).await?;
    let _account_access = match record {
        Some(record) => Some(st.sessions.account_access(record.user_id).write_owned().await),
        None => None,
    };
    let hashes = match crate::background::revoke_login(&st.db, &token).await {
        Ok(hashes) => hashes,
        Err(error) => {
            tracing::error!(code=?error.as_database_error().and_then(|e|e.code()), "로그아웃 인증정보 정리 실패");
            return Err(ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "logout_cleanup_failed",
                "서버에 보관된 로그인 정보를 삭제하지 못했어요. 연결을 확인하고 로그아웃을 다시 시도해 주세요.",
            ));
        }
    };
    st.sessions.remove_hashes(&hashes);
    st.sessions.remove(&token);
    let secure = if st.config.cookie_secure { "; Secure" } else { "" };
    let cookie = format!("{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{secure}", auth::COOKIE);
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({ "ok": true }))).into_response())
}

pub(crate) async fn logout_all(State(st): State<Shared>, LoginToken(token): LoginToken) -> Result<Response, ApiError> {
    let _access = st.sessions.access(&token).read_owned().await;
    let hash = vault::token_hash(&token);
    let record = db::login_record(&st.db, &hash).await?.ok_or_else(ApiError::session_revoked)?;
    let uid = record.user_id;
    let _account_access = st.sessions.account_access(uid).write_owned().await;
    let record = db::login_record(&st.db, &hash).await?.ok_or_else(ApiError::session_revoked)?;
    if record.logout_all_at.is_none() {
        if record.revoked_at.is_some() {
            return Err(ApiError::session_revoked());
        }
        if record.expires_at <= Utc::now() {
            return Err(ApiError::unauthorized());
        }
        if let Err(error) = crate::background::revoke_account(&st.db, uid, &hash).await {
            tracing::error!(code=?error.as_database_error().and_then(|e|e.code()), "전체 로그아웃 인증정보 정리 실패");
            return Err(ApiError::new(
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                "logout_cleanup_failed",
                "모든 기기의 로그인 정보를 삭제하지 못했어요. 연결을 확인하고 다시 시도해 주세요.",
            ));
        }
        st.sessions.remove_user(uid);
    }
    let secure = if st.config.cookie_secure { "; Secure" } else { "" };
    let cookie = format!("{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0{secure}", auth::COOKIE);
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({ "ok": true }))).into_response())
}

pub(crate) async fn me(user: CurrentUser) -> ApiResult<Value> {
    let Ok(school) = user.session.school() else {
        return Ok(Json(json!({
            "profile": { "name": user.session.name, "hasPicture": false, "studentId": user.session.student_id },
            "remembered": user.session.remembered,
        })));
    };
    let profile = school.cached_profile().unwrap_or_else(|| hongsi_core::models::Profile {
        name: user.session.name.clone(),
        has_picture: false,
        department: None,
    });
    Ok(Json(json!({
        "profile": {
            "name": if profile.name.is_empty() { user.session.name.clone() } else { profile.name },
            "hasPicture": profile.has_picture,
            "studentId": user.session.student_id,
            "department": profile.department,
        },
        "remembered": user.session.remembered,
    })))
}

#[derive(Deserialize)]
pub(crate) struct DeviceLoginBody {
    token: String,
    remember: bool,
}

pub(crate) async fn device_login(State(st): State<Shared>, headers: HeaderMap, Json(b): Json<DeviceLoginBody>) -> ApiResult<Value> {
    let previous = auth::token_from_headers(&headers);
    let _previous_access = match previous.as_ref() {
        Some(token) => {
            let access = st.sessions.access(token).read_owned().await;
            let record = db::login_record(&st.db, &vault::token_hash(token))
                .await?
                .ok_or_else(ApiError::session_revoked)?;
            if record.revoked_at.is_some() {
                return Err(ApiError::session_revoked());
            }
            Some(access)
        }
        None => None,
    };
    let token = b.token.trim();
    if token.is_empty() || token.len() > 128 || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::bad_request("잘못된 요청이에요."));
    }
    let (student_id, name) = hongsi_core::classroom::token_owner(&st.http, token).await.map_err(|e| match e {
        hongsi_core::CoreError::SessionExpired | hongsi_core::CoreError::ClassroomTokenExpired => ApiError::new(
            axum::http::StatusCode::UNAUTHORIZED,
            "login_rejected",
            "클래스룸 로그인을 확인하지 못했어요.",
        ),
        other => ApiError::from(other),
    })?;
    let user_id = db::upsert_user(&st.db, &auth::user_key(&st.pepper, &student_id)).await?;
    let _account_access = st.sessions.account_access(user_id).read_owned().await;
    let previous_hash = previous.as_deref().map(vault::token_hash);
    if let Some(hash) = previous_hash.as_ref() {
        let record = db::login_record(&st.db, hash).await?.ok_or_else(ApiError::session_revoked)?;
        if record.revoked_at.is_some() {
            return Err(ApiError::session_revoked());
        }
        if record.user_id != user_id {
            return Err(ApiError::conflict("기존 계정에서 먼저 로그아웃해 주세요."));
        }
    }
    let sealed = vault::Sealed {
        device: true,
        name: name.clone(),
        student_id: student_id.clone(),
        school: None,
    };
    let session = UserSession::new(None, user_id, name.clone(), student_id, b.remember);
    let expires_at = session.expires_at();
    let token = st.sessions.insert(session);
    let encrypted = if b.remember {
        let Some(encrypted) = vault::seal(&st.pepper, &token, &sealed) else {
            st.sessions.remove(&token);
            return Err(ApiError::conflict("로그인 상태를 저장하지 못했어요."));
        };
        Some(encrypted)
    } else {
        None
    };
    if let Err(error) = db::save_login(
        &st.db,
        &vault::token_hash(&token),
        user_id,
        encrypted.as_ref().map(|(nonce, data)| (nonce.as_slice(), data.as_slice())),
        expires_at,
        previous_hash.as_deref(),
    )
    .await
    {
        st.sessions.remove(&token);
        if matches!(error, sqlx::Error::RowNotFound) {
            return Err(ApiError::session_revoked());
        }
        return Err(error.into());
    }
    if let Some(previous) = previous {
        st.sessions.remove(&previous);
    }
    tracing::info!("앱 기기 로그인");
    Ok(Json(
        json!({ "token": token, "expiresAt": expires_at.timestamp(), "profile": { "name": name } }),
    ))
}

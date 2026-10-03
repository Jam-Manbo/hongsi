use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hongsi_core::CoreError;
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
    retry_after: Option<u64>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into(), retry_after: None }
    }

    pub fn rate_limited(seconds: u64) -> Self {
        let mut error = Self::new(StatusCode::TOO_MANY_REQUESTS, "login_rate_limited",
            format!("로그인은 계정당 1분에 5회까지 시도할 수 있어요. {seconds}초 뒤에 다시 시도해 주세요."));
        error.retry_after = Some(seconds);
        error
    }

    pub fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "로그인이 필요해요")
    }

    pub fn session_revoked() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "session_revoked", "로그아웃됐어요. 다시 로그인해 주세요.")
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut response = (self.status, Json(json!({ "error": { "code": self.code, "message": self.message } }))).into_response();
        if self.code == "session_revoked" {
            response.headers_mut().insert(header::SET_COOKIE, HeaderValue::from_static("hsid=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0"));
        }
        if let Some(seconds) = self.retry_after {
            response.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(seconds));
        }
        response
    }
}

impl From<CoreError> for ApiError {
    fn from(err: CoreError) -> Self {
        let message = err.to_string();
        match err {
            CoreError::ClassroomTokenExpired => Self::new(StatusCode::UNAUTHORIZED, "session_expired", message),
            CoreError::SessionExpired => Self::new(StatusCode::UNAUTHORIZED, "session_expired", message),
            CoreError::LoginRejected(_) => Self::new(StatusCode::UNAUTHORIZED, "login_rejected", message),
            CoreError::Network(e) => {
                tracing::warn!("학교 서버 연결 실패: {e}");
                Self::new(StatusCode::BAD_GATEWAY, "school_unreachable", message)
            }
            CoreError::Parse(_) => {
                tracing::warn!("{message}");
                Self::new(StatusCode::BAD_GATEWAY, "school_changed", message)
            }
            CoreError::Upstream(_) => Self::new(StatusCode::BAD_GATEWAY, "school_error", message),
            CoreError::NotFound(_) => Self::new(StatusCode::NOT_FOUND, "not_found", message),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!("DB 오류: {err}");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "서버에서 데이터를 처리하지 못했어요")
    }
}

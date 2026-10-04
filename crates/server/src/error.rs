use axum::body::Body;
use axum::extract::Request;
use axum::middleware::Next;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use hongsi_core::{api_error, CoreError};
use serde_json::json;

#[derive(Clone)]
struct ErrorCode(&'static str);

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
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "로그인이 필요해요.")
    }

    pub fn session_revoked() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "session_revoked", "다시 로그인해 주세요.")
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
        response.extensions_mut().insert(ErrorCode(self.code));
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
            CoreError::ClassroomTokenExpired => Self::new(StatusCode::UNAUTHORIZED, "classroom_token_expired", message),
            CoreError::SessionExpired => Self::new(StatusCode::UNAUTHORIZED, "session_expired", message),
            CoreError::LoginRejected(_) => Self::new(StatusCode::UNAUTHORIZED, "login_rejected", message),
            CoreError::Network(e) => {
                tracing::warn!(timeout = e.is_timeout(), connect = e.is_connect(), "학교 서버 연결 실패");
                Self::new(StatusCode::BAD_GATEWAY, "school_unreachable", message)
            }
            CoreError::Parse(_) => Self::new(StatusCode::BAD_GATEWAY, "school_changed", "학교 응답을 확인하지 못했어요."),
            CoreError::Upstream(_) => Self::new(StatusCode::BAD_GATEWAY, "school_error", message),
            CoreError::NotFound(_) => Self::new(StatusCode::NOT_FOUND, "not_found", message),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(code = ?err.as_database_error().and_then(|error| error.code()), "DB 오류");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "db_error", "서버에서 데이터를 처리하지 못했어요.")
    }
}

pub async fn normalize_response(request: Request, next: Next) -> Response {
    let path = request.uri().path().to_owned();
    let method = request.method().as_str().to_owned();
    let mut response = next.run(request).await;
    if (path != "/api" && !path.starts_with("/api/")) || response.status().as_u16() < 400 { return response; }
    let status = response.status();
    let format = api_error::response_format(response.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or(""));
    let (fallback_code, message) = api_error::fallback(status.as_u16());
    let code = response.extensions().get::<ErrorCode>().map(|value| value.0).unwrap_or(fallback_code);
    api_error::report(&method, &path, status.as_u16(), code, format);
    if format != "json" {
        *response.body_mut() = Body::from(json!({ "error": { "code": fallback_code, "message": message } }).to_string());
        response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        response.headers_mut().remove(header::CONTENT_LENGTH);
        response.headers_mut().remove(header::CONTENT_ENCODING);
    }
    response
}

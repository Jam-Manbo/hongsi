use axum::extract::DefaultBodyLimit;
use axum::routing::{any, get, post, put};
use axum::{Json, Router};
use serde_json::json;

use crate::error::ApiError;
use crate::state::Shared;
use crate::{auth, routes};

pub(crate) type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/api", any(api_not_found))
        .route("/api/{*path}", any(api_not_found))
        .route("/api/push/status", get(crate::background::status))
        .route(
            "/api/push/device",
            post(crate::background::register).delete(crate::background::disable),
        )
        .route("/api/background/session", post(crate::background::enable_sync))
        .route("/api/background/renew", post(crate::background::renew))
        .route("/api/push/preferences", put(crate::background::preferences))
        .route("/api/health", get(|| async { Json(json!({ "ok": true })) }))
        .route("/api/app-update", get(crate::updates::latest))
        .route("/api/app-releases", get(crate::updates::history))
        .route("/api/app-releases/{version}", get(crate::updates::detail))
        .route("/api/auth/login", post(routes::auth::login))
        .route("/api/auth/logout", post(routes::auth::logout))
        .route("/api/auth/logout-all", post(routes::auth::logout_all))
        .route("/api/auth/device", post(routes::auth::device_login))
        .route("/api/auth/recover", post(auth::recover))
        .route("/api/auth/reconnect", post(auth::reconnect))
        .route("/api/me", get(routes::auth::me))
        .route("/api/preferences", get(crate::preferences::get).patch(crate::preferences::patch))
        .route("/api/me/avatar", get(routes::files::avatar))
        .route("/api/files/{cmid}/{index}", get(routes::files::download_file))
        .route("/api/modules/{cmid}", get(routes::classroom::module_get))
        .route("/api/modules/{cmid}/files/{index}", get(routes::files::module_file))
        .route("/api/board/{cmid}/{bwid}", get(routes::classroom::board_get))
        .route("/api/board/{cmid}/{bwid}/files/{index}", get(routes::files::board_file))
        .route("/api/notifications", get(routes::classroom::notifications))
        .route(
            "/api/notices/seen",
            get(routes::classroom::notices_seen_get).post(routes::classroom::notices_seen_add),
        )
        .route("/api/assign/{cmid}/submission", get(routes::classroom::submission_get))
        .route(
            "/api/assign/{cmid}/submission/jobs/{id}",
            get(crate::submissions::status)
                .post(crate::submissions::start)
                .layer(DefaultBodyLimit::max(110 * 1024 * 1024)),
        )
        .route("/api/todos", get(routes::todos::todo_list).post(routes::todos::todo_create))
        .route(
            "/api/todos/{id}",
            put(routes::todos::todo_update).delete(routes::todos::todo_delete),
        )
        .route("/api/todos/{id}/done", post(routes::todos::todo_done))
        .route(
            "/api/attendance/receipts",
            get(crate::attendance::list).put(crate::attendance::record),
        )
        .route("/api/attendance/active", get(routes::attendance::attendance_active))
        .route("/api/attendance/submit", post(routes::attendance::attendance_submit))
        .route("/api/attendance/status", get(routes::attendance::attendance_status))
        .route("/api/attendance/course", get(routes::attendance::attendance_course))
        .route("/api/timetable", get(routes::attendance::timetable))
        .route("/api/calendar", get(routes::calendar::calendar_data))
        .route(
            "/api/calendar/state",
            get(routes::calendar::calendar_state_get).post(routes::calendar::calendar_state),
        )
        .route("/api/calendar/items/{key}/done", put(routes::calendar::set_done))
        .route("/api/calendar/items/{key}/alert", put(routes::calendar::set_alert))
        .route("/api/calendar/items/{key}/alert-leads", put(routes::calendar::set_alert_leads))
        .route("/api/meals", get(routes::meals::meals))
        .route("/api/seats", get(routes::seats::seats))
        .route("/api/seats/recent", get(routes::seats::seats_recent))
        .route(
            "/api/seats/session",
            get(routes::seats::seat_session)
                .post(routes::seats::seat_start)
                .patch(routes::seats::seat_adjust),
        )
        .route("/api/seats/session/extend", post(routes::seats::seat_extend))
        .route("/api/seats/session/end", post(routes::seats::seat_end))
}

async fn api_not_found() -> ApiError {
    ApiError::new(
        axum::http::StatusCode::NOT_FOUND,
        "api_not_found",
        "현재 서버에서는 이 기능을 사용할 수 없어요.",
    )
}

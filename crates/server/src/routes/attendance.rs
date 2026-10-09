use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::Json;
use hongsi_core::models::{ActiveLectures, AttendanceCourse, Timetable};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::error::ApiError;
use crate::state::Shared;

use crate::api::ApiResult;

pub(crate) async fn attendance_active(user: CurrentUser) -> ApiResult<ActiveLectures> {
    let mut cache = user.session.lectures_cache.lock().await;
    if let Some((at, data)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(3) {
            return Ok(Json(data.clone()));
        }
    }
    let data = user.session.school()?.active_lectures().await?;
    *cache = Some((Instant::now(), data.clone()));
    Ok(Json(data))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SubmitBody {
    lecture_key: String,
    code: String,
    latitude: f64,
    longitude: f64,
}

pub(crate) async fn attendance_submit(State(st): State<Shared>, user: CurrentUser, Json(b): Json<SubmitBody>) -> ApiResult<Value> {
    let code = b.code.trim();
    if code.is_empty() || code.len() > 12 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(ApiError::bad_request("인증번호를 확인해 주세요."));
    }
    if !(-90.0..=90.0).contains(&b.latitude) || !(-180.0..=180.0).contains(&b.longitude) {
        return Err(ApiError::bad_request("위치 정보가 올바르지 않아요."));
    }
    let submission = user
        .session
        .school()?
        .submit_attendance(&b.lecture_key, code, b.latitude, b.longitude)
        .await?;
    *user.session.lectures_cache.lock().await = None;
    user.session.course_cache.lock().await.clear();
    let synced = if let Some(receipt) = &submission.receipt {
        match crate::attendance::save(&st.db, user.session.user_id, receipt).await {
            Ok(()) => true,
            Err(_) => {
                tracing::warn!("출석 성공 기록 공유 실패");
                false
            }
        }
    } else {
        true
    };
    Ok(Json(
        json!({ "message": submission.message, "receipt": submission.receipt, "synced": synced }),
    ))
}

pub(crate) async fn attendance_status(user: CurrentUser) -> ApiResult<Vec<AttendanceCourse>> {
    Ok(Json(user.session.school()?.attendance_status().await?))
}

#[derive(Deserialize)]
pub(crate) struct CourseQuery {
    code: String,
}

fn valid_course_code(code: &str) -> bool {
    code.len() <= 16
        && code.split_once('-').is_some_and(|(haksu, bunban)| {
            !haksu.is_empty()
                && !bunban.is_empty()
                && haksu.chars().all(|c| c.is_ascii_digit())
                && bunban.chars().all(|c| c.is_ascii_digit())
        })
}

pub(crate) async fn attendance_course(user: CurrentUser, Query(q): Query<CourseQuery>) -> ApiResult<AttendanceCourse> {
    let code = q.code.trim();
    if !valid_course_code(code) {
        return Err(ApiError::bad_request("과목 코드가 올바르지 않아요."));
    }
    let mut cache = user.session.course_cache.lock().await;
    if let Some((at, data)) = cache.get(code) {
        if at.elapsed() < Duration::from_secs(60) {
            return Ok(Json(data.clone()));
        }
    }
    let data = user.session.school()?.attendance_course(code).await?;
    cache.insert(code.to_string(), (Instant::now(), data.clone()));
    Ok(Json(data))
}

pub(crate) async fn timetable(user: CurrentUser, Query(q): Query<RefreshQuery>) -> ApiResult<Timetable> {
    let max_age = if q.refresh == Some(1) { 60 } else { 6 * 3600 };
    let mut cache = user.session.timetable_cache.lock().await;
    if let Some((at, data)) = cache.as_ref() {
        if at.elapsed() < Duration::from_secs(max_age) {
            return Ok(Json(data.clone()));
        }
    }
    let data = user.session.school()?.timetable().await?;
    *cache = Some((Instant::now(), data.clone()));
    Ok(Json(data))
}

#[derive(Deserialize)]
pub(crate) struct RefreshQuery {
    refresh: Option<u8>,
}

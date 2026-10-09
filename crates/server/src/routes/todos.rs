use axum::extract::{Path, State};
use axum::Json;
use chrono::{TimeZone, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::calendar::{self};
use crate::error::ApiError;
use crate::state::Shared;
use crate::todos::{self, Todo, TodoInput};

use crate::api::ApiResult;

async fn check_todo(user: &CurrentUser, t: &TodoInput) -> Result<chrono::DateTime<Utc>, ApiError> {
    if t.alert_leads.as_deref().is_some_and(|leads| !calendar::valid_alert_leads(leads)) {
        return Err(ApiError::bad_request("알림 시간이 올바르지 않아요."));
    }
    let title = t.title.trim();
    if title.is_empty() || title.chars().count() > 200 || t.note.chars().count() > 2000 {
        return Err(ApiError::bad_request("제목은 1~200자, 메모는 2000자까지예요."));
    }
    let timestamp = t.due.ok_or_else(|| ApiError::bad_request("날짜를 선택해 주세요."))?;
    let due = Utc
        .timestamp_opt(timestamp, 0)
        .single()
        .ok_or_else(|| ApiError::bad_request("날짜가 올바르지 않아요."))?;
    if let Some(key) = &t.parent_key {
        if !(key.starts_with("assign:") || key.starts_with("vod:")) || key.len() > 300 {
            return Err(ApiError::bad_request("잘못된 연결 항목이에요."));
        }
    }
    if let Some(course) = t.course_id {
        let cached = user
            .session
            .calendar_cache
            .lock()
            .await
            .as_ref()
            .map(|(_, d)| d.courses.iter().any(|c| c.id == course));
        let known = match (cached, user.session.school().ok()) {
            (Some(true), _) => true,
            (_, Some(school)) => school.all_courses().await?.iter().any(|c| c.id == course),
            (_, None) => {
                let known = user.session.device_courses.lock().expect("세션 잠금");
                known.is_empty() || known.contains(&course)
            }
        };
        if !known {
            return Err(ApiError::bad_request("수강 과목을 확인할 수 없어요."));
        }
    }
    Ok(due)
}

pub(crate) async fn todo_list(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Vec<Todo>> {
    let parents = calendar::known_parents(&st.db, &user.session).await?;
    todos::sync_parents(&st.db, user.session.user_id, &parents).await?;
    Ok(Json(todos::list(&st.db, user.session.user_id).await?))
}

pub(crate) async fn todo_create(State(st): State<Shared>, user: CurrentUser, Json(mut t): Json<TodoInput>) -> ApiResult<Todo> {
    let parent = match &t.parent_key {
        Some(key) => Some(
            calendar::parent(&st.db, &user.session, key)
                .await?
                .ok_or_else(|| ApiError::bad_request("연결할 과제·강의를 확인할 수 없어요. 일정을 새로고침해 주세요."))?,
        ),
        None => None,
    };
    if let Some(parent) = &parent {
        t.course_id = Some(parent.course_id);
    }
    let due = check_todo(&user, &t).await?;
    todos::create(&st.db, user.session.user_id, &t, due, parent.is_some_and(|p| p.finished))
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::conflict("완료된 일정에는 할 일을 추가할 수 없어요."))
}

pub(crate) async fn todo_update(
    State(st): State<Shared>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(t): Json<TodoInput>,
) -> ApiResult<Todo> {
    let parents = calendar::known_parents(&st.db, &user.session).await?;
    todos::sync_parents(&st.db, user.session.user_id, &parents).await?;
    let existing = todos::get(&st.db, user.session.user_id, id)
        .await?
        .ok_or_else(|| ApiError::not_found("할 일을 찾지 못했어요."))?;
    if t.parent_key != existing.parent_key || (existing.parent_key.is_some() && t.course_id != existing.course_id) {
        return Err(ApiError::bad_request("연결된 과제·강의와 과목은 바꿀 수 없어요."));
    }
    let due = check_todo(&user, &t).await?;
    todos::update(&st.db, user.session.user_id, id, &t, due)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("할 일을 찾지 못했어요."))
}

#[derive(Deserialize)]
pub(crate) struct DoneBody {
    done: bool,
}

pub(crate) async fn todo_done(
    State(st): State<Shared>,
    user: CurrentUser,
    Path(id): Path<i64>,
    Json(b): Json<DoneBody>,
) -> ApiResult<Todo> {
    let existing = todos::get(&st.db, user.session.user_id, id)
        .await?
        .ok_or_else(|| ApiError::not_found("할 일을 찾지 못했어요."))?;
    let school_done = match existing.parent_key {
        Some(key) => calendar::parent(&st.db, &user.session, &key).await?.is_some_and(|p| p.finished),
        None => false,
    };
    todos::set_done(&st.db, user.session.user_id, id, b.done, school_done)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::not_found("할 일을 찾지 못했어요."))
}

pub(crate) async fn todo_delete(State(st): State<Shared>, user: CurrentUser, Path(id): Path<i64>) -> ApiResult<Value> {
    if !todos::delete(&st.db, user.session.user_id, id).await? {
        return Err(ApiError::not_found("할 일을 찾지 못했어요."));
    }
    Ok(Json(json!({ "ok": true })))
}

use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::Utc;
use hongsi_core::models::SemesterDisplay;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::calendar::{self, CalendarData};
use crate::db;
use crate::error::ApiError;
use crate::state::Shared;

use crate::api::ApiResult;

#[derive(Deserialize)]
pub(crate) struct CalendarQuery {
    refresh: Option<u8>,
    #[serde(default)]
    semester: SemesterDisplay,
}

pub(crate) async fn calendar_data(State(st): State<Shared>, user: CurrentUser, Query(q): Query<CalendarQuery>) -> ApiResult<CalendarData> {
    let s = &user.session;
    let mut cache = s.calendar_cache.lock().await;
    if q.refresh.unwrap_or(0) == 0 {
        if let Some((at, data)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(180) && data.semester_display == q.semester {
                let mut data = data.clone();
                drop(cache);
                calendar::load_state(&st.db, s.user_id).await?.apply(&mut data.items);
                calendar::sync_parents(&st.db, s, data.items.iter().map(calendar::TodoParent::from).collect()).await?;
                return Ok(Json(data));
            }
        }
    }
    let school = s.school()?;
    let (current_term, courses) = school.courses_for(q.semester).await?;
    let (assignments, vods) = tokio::join!(school.assignments(&courses), school.vods(&courses));
    let (assignments, vods) = (assignments?, vods?);
    let snapshots = db::sync_assignments(&st.db, s.user_id, &assignments).await?;
    let checks = db::item_checks(&st.db, s.user_id).await?;
    let alerts_off = db::item_alerts_off(&st.db, s.user_id).await?;
    let alert_leads = db::item_alert_leads(&st.db, s.user_id).await?;
    let now = Utc::now().timestamp();
    let data = CalendarData {
        semester_display: q.semester,
        current_term,
        items: calendar::build(
            &assignments,
            &vods,
            &calendar::snapshot_infos(snapshots),
            &checks,
            &alerts_off,
            &alert_leads,
            now,
        ),
        courses,
        fetched_at: now,
    };
    calendar::sync_parents(&st.db, s, data.items.iter().map(calendar::TodoParent::from).collect()).await?;
    *cache = Some((Instant::now(), data.clone()));
    Ok(Json(data))
}

pub(crate) async fn calendar_state_get(State(st): State<Shared>, user: CurrentUser) -> ApiResult<calendar::CalendarState> {
    Ok(Json(calendar::load_state(&st.db, user.session.user_id).await?))
}

#[derive(Deserialize)]
pub(crate) struct StateBody {
    #[serde(default)]
    courses: Vec<hongsi_core::models::Course>,
    #[serde(default)]
    assignments: Vec<hongsi_core::models::Assignment>,
    #[serde(default)]
    vods: Vec<hongsi_core::models::Vod>,
    #[serde(default)]
    parents: Vec<calendar::TodoParent>,
}

pub(crate) async fn calendar_state(State(st): State<Shared>, user: CurrentUser, Json(b): Json<StateBody>) -> ApiResult<Value> {
    if b.courses.len() > 200 || b.assignments.len() > 3000 || b.vods.len() > 6000 || b.parents.len() > 6000 {
        return Err(ApiError::bad_request("요청이 너무 커요."));
    }
    if b.parents
        .iter()
        .any(|p| p.key.len() > 300 || !(p.key.starts_with("assign:") || p.key.starts_with("vod:")))
    {
        return Err(ApiError::bad_request("잘못된 연결 항목이에요."));
    }
    if !b.courses.is_empty() {
        *user.session.device_courses.lock().expect("세션 잠금") = b.courses.iter().map(|c| c.id).collect();
    }
    let snapshots = db::sync_assignments(&st.db, user.session.user_id, &b.assignments).await?;
    let checks = db::item_checks(&st.db, user.session.user_id).await?;
    let alerts_off = db::item_alerts_off(&st.db, user.session.user_id).await?;
    let alert_leads = db::item_alert_leads(&st.db, user.session.user_id).await?;
    let snapshots = calendar::snapshot_infos(snapshots);
    let items = calendar::build(
        &b.assignments,
        &b.vods,
        &snapshots,
        &checks,
        &alerts_off,
        &alert_leads,
        Utc::now().timestamp(),
    );
    let mut parents: Vec<_> = items.iter().map(calendar::TodoParent::from).collect();
    parents.extend(b.parents);
    calendar::sync_parents(&st.db, &user.session, parents).await?;
    Ok(Json(
        json!({ "snapshots": snapshots, "checks": checks, "alertsOff": alerts_off, "alertLeads": alert_leads }),
    ))
}

#[derive(Deserialize)]
pub(crate) struct CalendarDoneBody {
    #[serde(deserialize_with = "Option::<bool>::deserialize")]
    done: Option<bool>,
}

pub(crate) async fn set_done(
    State(st): State<Shared>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(b): Json<CalendarDoneBody>,
) -> ApiResult<Value> {
    if key.len() > 300 || !(key.starts_with("assign:") || key.starts_with("vod:")) {
        return Err(ApiError::bad_request("잘못된 항목이에요."));
    }
    match b.done {
        Some(done) => db::set_item_check(&st.db, user.session.user_id, &key, done).await?,
        None => {
            let parent = calendar::parent(&st.db, &user.session, &key).await?;
            db::clear_item_check(&st.db, user.session.user_id, &key, parent.is_some_and(|p| p.finished)).await?;
        }
    }
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|i| i.key == key) {
            item.done = b.done.unwrap_or(item.status == "submitted" || item.status == "done");
            item.done_override = b.done;
        }
    }
    Ok(Json(json!({ "key": key, "done": b.done })))
}

#[derive(Deserialize)]
pub(crate) struct AlertBody {
    on: bool,
}

pub(crate) async fn set_alert(
    State(st): State<Shared>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(b): Json<AlertBody>,
) -> ApiResult<Value> {
    if key.len() > 300 || !(key.starts_with("assign:") || key.starts_with("vod:")) {
        return Err(ApiError::bad_request("잘못된 항목이에요."));
    }
    db::set_item_alert(&st.db, user.session.user_id, &key, b.on).await?;
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|i| i.key == key) {
            item.alert = b.on;
        }
    }
    Ok(Json(json!({ "key": key, "on": b.on })))
}

#[derive(Deserialize)]
pub(crate) struct AlertLeadsBody {
    leads: Option<Vec<i32>>,
}

pub(crate) async fn set_alert_leads(
    State(st): State<Shared>,
    user: CurrentUser,
    Path(key): Path<String>,
    Json(b): Json<AlertLeadsBody>,
) -> ApiResult<Value> {
    if key.len() > 300 || !(key.starts_with("assign:") || key.starts_with("vod:")) {
        return Err(ApiError::bad_request("잘못된 항목이에요."));
    }
    if b.leads.as_deref().is_some_and(|leads| !calendar::valid_alert_leads(leads)) {
        return Err(ApiError::bad_request("알림 시간이 올바르지 않아요."));
    }
    db::set_item_alert_leads(&st.db, user.session.user_id, &key, b.leads.as_deref()).await?;
    if let Some((_, data)) = user.session.calendar_cache.lock().await.as_mut() {
        for item in data.items.iter_mut().filter(|i| i.key == key) {
            item.alert_leads = b.leads.clone();
        }
    }
    Ok(Json(json!({ "key": key, "leads": b.leads })))
}

use axum::extract::{Query, State};
use axum::Json;
use chrono::{Duration as Span, TimeZone, Utc};
use hongsi_core::seats::validity_hours;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::CurrentUser;
use crate::db::{self, NewSeatSession, RecentSeat, SeatSession};
use crate::error::ApiError;
use crate::state::{SeatSnapshot, Shared};

use crate::api::ApiResult;

async fn ensure_snapshot(st: &Shared) -> Result<(), ApiError> {
    if let Some(s) = st.seats.read().await.as_ref() {
        if (Utc::now() - s.fetched_at).num_seconds() < 90 {
            return Ok(());
        }
    }
    let buildings = hongsi_core::seats::fetch_all(&st.http).await?;
    *st.seats.write().await = Some(SeatSnapshot {
        fetched_at: Utc::now(),
        buildings,
    });
    Ok(())
}

pub(crate) async fn seats(State(st): State<Shared>) -> ApiResult<Value> {
    ensure_snapshot(&st).await?;
    let snap = st.seats.read().await;
    let s = snap.as_ref().ok_or_else(|| ApiError::not_found("좌석 정보를 받지 못했어요."))?;
    Ok(Json(
        json!({ "fetchedAt": s.fetched_at.timestamp(), "watching": st.config.seat_poll_secs > 0, "buildings": s.buildings }),
    ))
}

#[derive(Deserialize)]
pub(crate) struct RecentQuery {
    building: String,
    room: i32,
    minutes: Option<i64>,
}

pub(crate) async fn seats_recent(State(st): State<Shared>, Query(q): Query<RecentQuery>) -> ApiResult<Vec<RecentSeat>> {
    let minutes = q.minutes.unwrap_or(15).clamp(1, 120);
    Ok(Json(
        db::recent_assigned(&st.db, &q.building, q.room, Utc::now() - Span::minutes(minutes)).await?,
    ))
}

async fn seat_info(st: &Shared, building: &str, room_no: i32, seat_no: i32) -> Option<(String, String)> {
    let snap = st.seats.read().await;
    let b = snap.as_ref()?.buildings.iter().find(|b| b.id == building)?;
    let room = b.rooms.iter().find(|r| r.no as i32 == room_no)?;
    let state = room.grid.iter().flatten().flatten().find(|c| c.no as i32 == seat_no)?.state;
    Some((room.name.clone(), state.as_str().to_string()))
}

fn building_name(id: &str) -> &'static str {
    hongsi_core::seats::BUILDINGS
        .iter()
        .find(|(b, _, _)| *b == id)
        .map(|(_, name, _)| *name)
        .unwrap_or("")
}

fn normalize_period(period: Option<&str>) -> &'static str {
    match period {
        Some("exam") => "exam",
        Some("vacation") => "vacation",
        _ => "semester",
    }
}

async fn view(st: &Shared, s: SeatSession) -> Value {
    let seat_state = seat_info(st, &s.building, s.room_no, s.seat_no).await.map(|(_, state)| state);
    let mut v = serde_json::to_value(&s).unwrap_or_else(|_| json!({}));
    v["seatState"] = json!(seat_state);
    v["validityHours"] = json!(validity_hours(&s.period));
    v["buildingName"] = json!(building_name(&s.building));
    v
}

pub(crate) async fn seat_session(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let _ = ensure_snapshot(&st).await;
    let session = db::active_seat_session(&st.db, user.session.user_id).await?;
    let session = match session {
        Some(s) => Some(view(&st, s).await),
        None => None,
    };
    Ok(Json(json!({ "session": session })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StartBody {
    building: String,
    room_no: i32,
    seat_no: i32,
    period: Option<String>,
}

pub(crate) async fn seat_start(State(st): State<Shared>, user: CurrentUser, Json(b): Json<StartBody>) -> ApiResult<Value> {
    let user_id = user.session.user_id;
    if db::active_seat_session(&st.db, user_id).await?.is_some() {
        return Err(ApiError::conflict("이미 사용 중인 좌석이 있어요."));
    }
    ensure_snapshot(&st).await?;
    let (room_name, _) = seat_info(&st, &b.building, b.room_no, b.seat_no)
        .await
        .ok_or_else(|| ApiError::not_found("선택한 좌석을 찾지 못했어요."))?;
    let period = normalize_period(b.period.as_deref());
    let now = Utc::now();
    let (started_at, source) = match db::seat_state(&st.db, &b.building, b.room_no, b.seat_no).await? {
        Some(s) if s.state == "used" && s.since_known && now - s.since <= Span::minutes(30) => (s.since, "detected"),
        _ => (now, "manual"),
    };
    let session = db::insert_seat_session(
        &st.db,
        user_id,
        NewSeatSession {
            building: &b.building,
            room_no: b.room_no,
            room_name: &room_name,
            seat_no: b.seat_no,
            period,
            started_at,
            start_source: source,
            hours: validity_hours(period) as i32,
        },
    )
    .await?;
    Ok(Json(json!({ "session": view(&st, session).await })))
}

pub(crate) async fn seat_extend(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let user_id = user.session.user_id;
    let current = db::active_seat_session(&st.db, user_id)
        .await?
        .ok_or_else(|| ApiError::not_found("사용 중인 좌석이 없어요."))?;
    let session = db::extend_seat_session(&st.db, user_id, validity_hours(&current.period) as i32)
        .await?
        .ok_or_else(|| ApiError::not_found("사용 중인 좌석이 없어요."))?;
    Ok(Json(json!({ "session": view(&st, session).await })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AdjustBody {
    started_at: i64,
    period: Option<String>,
}

pub(crate) async fn seat_adjust(State(st): State<Shared>, user: CurrentUser, Json(b): Json<AdjustBody>) -> ApiResult<Value> {
    let now = Utc::now();
    let started_at = Utc
        .timestamp_opt(b.started_at, 0)
        .single()
        .filter(|t| *t <= now + Span::minutes(5) && *t >= now - Span::hours(12))
        .ok_or_else(|| ApiError::bad_request("입실 시각은 최근 12시간 이내로 설정해 주세요."))?;
    let period = normalize_period(b.period.as_deref());
    let session = db::adjust_seat_session(&st.db, user.session.user_id, started_at, validity_hours(period) as i32, period)
        .await?
        .ok_or_else(|| ApiError::not_found("사용 중인 좌석이 없어요."))?;
    Ok(Json(json!({ "session": view(&st, session).await })))
}

pub(crate) async fn seat_end(State(st): State<Shared>, user: CurrentUser) -> ApiResult<Value> {
    let ended = db::end_seat_session(&st.db, user.session.user_id, "manual")
        .await?
        .ok_or_else(|| ApiError::not_found("사용 중인 좌석이 없어요."))?;
    Ok(Json(json!({ "session": null, "ended": ended })))
}

use crate::{
    auth::CurrentUser,
    calendar, db,
    error::ApiError,
    push::{self, PushError},
    state::Shared,
    todos, vault,
};
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
use std::time::Duration;

const POLL_SECS: i64 = 5 * 60;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInput {
    device_id: String,
    kind: String,
    destination: Value,
    leads: Vec<i32>,
    seat_leads: Vec<i32>,
    change_alerts: Option<bool>,
    consent: bool,
    #[serde(default)]
    cookies: Option<Vec<(String, String)>>,
    #[serde(default)]
    only_this_device: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    device_id: String,
    leads: Vec<i32>,
    seat_leads: Vec<i32>,
    change_alerts: Option<bool>,
    kind: Option<String>,
    destination: Option<Value>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Renewal {
    device_id: String,
    #[serde(default)]
    cookies: Option<Vec<(String, String)>>,
}
#[derive(Deserialize)]
pub struct DeviceQuery {
    device: String,
}
#[derive(FromRow)]
struct Device {
    id: String,
    kind: String,
    destination: Value,
    leads: Vec<i32>,
    seat_leads: Vec<i32>,
}
#[derive(FromRow)]
struct Background {
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    snapshot: Option<Value>,
}
#[derive(FromRow)]
struct Job {
    device_id: String,
    event_key: String,
    payload: Value,
    attempts: i32,
}

fn valid_id(id: &str) -> bool {
    (20..=80).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
fn valid_leads(leads: &[i32], seats: &[i32]) -> bool {
    leads.len() <= 4
        && leads.iter().all(|l| [1440, 180, 60, 10].contains(l))
        && seats.len() <= 4
        && seats.iter().all(|l| [60, 30, 10, 0].contains(l))
}
fn valid_destination(kind: &str, destination: &Value) -> bool {
    match kind {
        "web" => push::valid_web_destination(destination),
        "apns" => destination["token"].as_str().is_some_and(|s| {
            (32..=256).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit())
        }),
        "fcm" => destination["token"].as_str().is_some_and(|s| {
            (20..=4096).contains(&s.len())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_:".contains(c))
        }),
        _ => false,
    }
}
pub async fn status(
    State(st): State<Shared>,
    user: CurrentUser,
    Query(q): Query<DeviceQuery>,
) -> Result<Json<Value>, ApiError> {
    let uid = user.session.user_id;
    let device: Option<(Option<String>, bool)> = sqlx::query_as("select last_error,change_alerts from notification_devices where id=$1 and user_id=$2 and expires_at>now()") .bind(&q.device).bind(uid).fetch_optional(&st.db).await?;
    let bg: Option<(DateTime<Utc>, Option<DateTime<Utc>>, Option<String>)> = sqlx::query_as(
        "select expires_at,last_poll_at,last_error from background_sessions where user_id=$1 and expires_at>now()",
    )
    .bind(uid)
    .fetch_optional(&st.db)
    .await?;
    let count: (i64, Option<DateTime<Utc>>) = sqlx::query_as("select count(*),min(due_at) from notification_outbox where device_id=$1 and sent_at is null and expires_at>now() and device_id in(select id from notification_devices where user_id=$2)").bind(&q.device).bind(uid).fetch_one(&st.db).await?;
    Ok(Json(
        json!({"registered":device.is_some(),"changeAlerts":device.as_ref().is_none_or(|d|d.1),"error":device.and_then(|d|d.0),"consented":bg.is_some(),"expiresAt":bg.as_ref().map(|b|b.0.timestamp()),"lastPollAt":bg.as_ref().and_then(|b|b.1).map(|d|d.timestamp()),"schoolError":bg.and_then(|b|b.2),"scheduled":count.0,"nextAt":count.1.map(|d|d.timestamp()),"pollMinutes":POLL_SECS/60,"available":{"web":st.push.ready("web"),"fcm":st.push.ready("fcm"),"apns":st.push.ready("apns")},"publicKey":st.push.public_key}),
    ))
}
async fn verified_cookies(st: &Shared, user: &CurrentUser, supplied: Option<Vec<(String, String)>>) -> Result<Vec<(String, String)>, ApiError> {
    let cookies = match (&user.session.school, supplied) {
        (Some(school), _) => school.sso_cookies().to_vec(),
        (None, Some(cookies)) if !cookies.is_empty() && cookies.len() <= 30
            && cookies.iter().all(|(k, v)| k.len() <= 100 && v.len() <= 8000) => cookies,
        _ => return Err(ApiError::bad_request("학교 로그인 세션이 필요해요")),
    };
    let school = hongsi_core::SchoolSession::from_sso_cookies(cookies.clone())?;
    let token = school.moodle_token().await?;
    let (owner, _) = hongsi_core::classroom::token_owner(&st.http, &token).await?;
    if owner != user.session.student_id {
        return Err(ApiError::bad_request("학교 세션의 계정이 현재 계정과 달라요"));
    }
    Ok(cookies)
}

pub async fn renew(State(st): State<Shared>, user: CurrentUser, Json(b): Json<Renewal>) -> Result<Json<Value>, ApiError> {
    if !valid_id(&b.device_id) { return Err(ApiError::bad_request("기기 정보를 확인해 주세요")); }
    let uid = user.session.user_id;
    let (eligible,): (bool,) = sqlx::query_as("select exists(select 1 from background_sessions b join notification_devices d on d.user_id=b.user_id where b.user_id=$1 and d.id=$2 and b.expires_at>now() and d.expires_at>now())")
        .bind(uid).bind(&b.device_id).fetch_one(&st.db).await?;
    if !eligible { return Err(ApiError::conflict("백그라운드 알림에 다시 동의하고 연결해 주세요.")); }
    let cookies = verified_cookies(&st, &user, b.cookies).await?;
    let sealed = vault::Sealed { name: user.session.name.clone(), student_id: user.session.student_id.clone(), cookies };
    let (nonce, ciphertext) = vault::seal(&st.pepper, &format!("background:{uid}"), &sealed)
        .ok_or_else(|| ApiError::conflict("학교 세션을 암호화하지 못했어요"))?;
    let expires = renew_stored_session(&st.db, uid, &b.device_id, &vault::token_hash(&user.token), &nonce, &ciphertext).await?
        .ok_or_else(|| ApiError::conflict("백그라운드 알림에 다시 동의하고 연결해 주세요."))?;
    Ok(Json(json!({"ok":true,"expiresAt":expires.timestamp()})))
}

async fn renew_stored_session(db: &PgPool, uid: i64, device: &str, session_hash: &str, nonce: &[u8], ciphertext: &[u8]) -> sqlx::Result<Option<DateTime<Utc>>> {
    let mut tx = db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-uid).execute(&mut *tx).await?;
    let expires: Option<(DateTime<Utc>,)> = sqlx::query_as("update background_sessions b set nonce=$3,ciphertext=$4,expires_at=now()+interval '14 days',next_poll_at=case when last_error is not null then now() else next_poll_at end,last_error=null where user_id=$1 and expires_at>now() and exists(select 1 from notification_devices d where d.user_id=b.user_id and d.id=$2 and d.expires_at>now()) returning expires_at")
        .bind(uid).bind(device).bind(nonce).bind(ciphertext).fetch_optional(&mut *tx).await?;
    if expires.is_some() {
        sqlx::query("update notification_devices set session_hash=$3,expires_at=now()+interval '90 days' where user_id=$1 and id=$2")
            .bind(uid).bind(device).bind(session_hash).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(expires.map(|v| v.0))
}

pub async fn register(
    State(st): State<Shared>,
    user: CurrentUser,
    Json(b): Json<DeviceInput>,
) -> Result<Json<Value>, ApiError> {
    if !b.consent
        || !valid_id(&b.device_id)
        || !valid_leads(&b.leads, &b.seat_leads)
        || !valid_destination(&b.kind, &b.destination)
    {
        return Err(ApiError::bad_request(
            "알림 동의와 기기 정보를 확인해 주세요",
        ));
    }
    if !st.push.ready(&b.kind) {
        return Err(ApiError::conflict(
            "서버 오류입니다. 문제가 지속되면 문의해 주세요.",
        ));
    }
    let cookies = verified_cookies(&st, &user, b.cookies).await?;
    let uid = user.session.user_id;
    let sealed = vault::Sealed {
        name: user.session.name.clone(),
        student_id: user.session.student_id.clone(),
        cookies,
    };
    let (nonce, ct) = vault::seal(&st.pepper, &format!("background:{uid}"), &sealed)
        .ok_or_else(|| ApiError::conflict("학교 세션을 암호화하지 못했어요"))?;
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(-uid)
        .execute(&mut *tx)
        .await?;
    if b.only_this_device {
        sqlx::query("delete from notification_devices where user_id=$1 and id<>$2")
            .bind(uid)
            .bind(&b.device_id)
            .execute(&mut *tx)
            .await?;
    }
    let (count,): (i64,) =
        sqlx::query_as("select count(*) from notification_devices where user_id=$1 and id<>$2")
            .bind(uid)
            .bind(&b.device_id)
            .fetch_one(&mut *tx)
            .await?;
    if count >= 10 {
        return Err(ApiError::bad_request("등록할 수 있는 기기는 10개까지예요"));
    }
    let saved = sqlx::query("insert into notification_devices(id,user_id,session_hash,kind,destination,leads,seat_leads,change_alerts) values($1,$2,$3,$4,$5,$6,$7,coalesce($8,true)) on conflict(id) do update set session_hash=excluded.session_hash,kind=excluded.kind,destination=excluded.destination,leads=excluded.leads,seat_leads=excluded.seat_leads,change_alerts=coalesce($8,notification_devices.change_alerts),expires_at=now()+interval '90 days',last_error=null where notification_devices.user_id=excluded.user_id")
        .bind(&b.device_id).bind(uid).bind(vault::token_hash(&user.token)).bind(&b.kind).bind(&b.destination).bind(&b.leads).bind(&b.seat_leads).bind(b.change_alerts).execute(&mut *tx).await?;
    if saved.rows_affected() != 1 {
        return Err(ApiError::conflict(
            "이 기기의 이전 계정 알림을 먼저 해제해 주세요",
        ));
    }
    if b.change_alerts == Some(false) {
        clear_change_alerts(&mut tx, &b.device_id).await?;
    }
    sqlx::query("insert into background_sessions(user_id,nonce,ciphertext,expires_at) values($1,$2,$3,now()+interval '14 days') on conflict(user_id) do update set nonce=excluded.nonce,ciphertext=excluded.ciphertext,expires_at=excluded.expires_at,next_poll_at=now(),last_error=null")
        .bind(uid).bind(nonce).bind(ct).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn preferences(
    State(st): State<Shared>,
    user: CurrentUser,
    Json(b): Json<Preferences>,
) -> Result<Json<Value>, ApiError> {
    if !valid_id(&b.device_id) || !valid_leads(&b.leads, &b.seat_leads) {
        return Err(ApiError::bad_request("알림 시간을 확인해 주세요"));
    }
    if let (Some(kind), Some(destination)) = (&b.kind, &b.destination) {
        if !valid_destination(kind, destination) {
            return Err(ApiError::bad_request("기기 푸시 주소를 확인해 주세요"));
        }
    }
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(-user.session.user_id).execute(&mut *tx).await?;
    let saved = sqlx::query(
        "update notification_devices set leads=$3,seat_leads=$4,change_alerts=coalesce($5,change_alerts) where id=$1 and user_id=$2 and expires_at>now()",
    )
    .bind(&b.device_id)
    .bind(user.session.user_id)
    .bind(&b.leads)
    .bind(&b.seat_leads)
    .bind(b.change_alerts)
    .execute(&mut *tx)
    .await?;
    if saved.rows_affected() != 1 {
        return Err(ApiError::conflict("백그라운드 일정 확인을 다시 연결해 주세요."));
    }
    if b.change_alerts == Some(false) {
        clear_change_alerts(&mut tx, &b.device_id).await?;
    }
    if let (Some(kind), Some(destination)) = (&b.kind, &b.destination) {
        sqlx::query("update notification_devices set destination=$3,session_hash=$4 where id=$1 and user_id=$2 and kind=$5").bind(&b.device_id).bind(user.session.user_id).bind(destination).bind(vault::token_hash(&user.token)).bind(kind).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
async fn clear_change_alerts(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, device: &str) -> sqlx::Result<()> {
    sqlx::query("delete from notification_outbox where device_id=$1 and sent_at is null and event_key like 'change:%'")
        .bind(device).execute(&mut **tx).await?;
    Ok(())
}

pub async fn disable(
    State(st): State<Shared>,
    user: CurrentUser,
    Query(q): Query<DeviceQuery>,
) -> Result<Json<Value>, ApiError> {
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(-user.session.user_id)
        .execute(&mut *tx)
        .await?;
    if q.device == "all" {
        sqlx::query("delete from notification_devices where user_id=$1")
            .bind(user.session.user_id)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query("delete from notification_devices where user_id=$1 and id=$2")
            .bind(user.session.user_id)
            .bind(q.device)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("delete from background_sessions b where user_id=$1 and not exists(select 1 from notification_devices d where d.user_id=b.user_id)").bind(user.session.user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}
pub async fn revoke_login(db: &PgPool, token: &str) -> sqlx::Result<()> {
    let hash = vault::token_hash(token);
    let users: Vec<(i64,)> = sqlx::query_as(
        "select distinct user_id from notification_devices where session_hash=$1 order by user_id",
    )
    .bind(&hash)
    .fetch_all(db)
    .await?;
    let mut tx = db.begin().await?;
    for (uid,) in users {
        sqlx::query("select pg_advisory_xact_lock($1)")
            .bind(-uid)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("delete from notification_devices where session_hash=$1")
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from background_sessions b where not exists(select 1 from notification_devices d where d.user_id=b.user_id)").execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn run(st: Shared) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        if let Err(e) = cycle(&st).await {
            tracing::warn!(code=?e.as_database_error().map(|e|e.code()), "백그라운드 알림 처리 실패");
        }
    }
}
async fn cycle(st: &Shared) -> sqlx::Result<()> {
    sqlx::query("delete from notification_devices where expires_at<now()")
        .execute(&st.db)
        .await?;
    sqlx::query("delete from background_sessions b where expires_at<now() or not exists(select 1 from notification_devices d where d.user_id=b.user_id)").execute(&st.db).await?;
    sqlx::query("delete from notification_outbox where expires_at < now()-interval '7 days'")
        .execute(&st.db)
        .await?;
    let users: Vec<(i64,)> =
        sqlx::query_as("select user_id from background_sessions order by next_poll_at")
            .fetch_all(&st.db)
            .await?;
    for (uid,) in users {
        let mut lock = st.db.begin().await?;
        let (acquired,): (bool,) = sqlx::query_as("select pg_try_advisory_xact_lock($1)")
            .bind(-uid)
            .fetch_one(&mut *lock)
            .await?;
        if !acquired {
            continue;
        }
        if let Err(e) = process_user(st, uid).await {
            tracing::warn!(code=?e.as_database_error().map(|e|e.code()), "계정 알림 처리 실패");
        }
        lock.commit().await?;
    }
    Ok(())
}
async fn poll_due(db: &PgPool, uid: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar("select next_poll_at<=now() or (last_error is null and last_poll_at is not null and last_poll_at<=now()-make_interval(secs=>$2)) from background_sessions where user_id=$1")
        .bind(uid).bind(POLL_SECS as f64).fetch_one(db).await
}

async fn process_user(st: &Shared, uid: i64) -> sqlx::Result<()> {
    let Some(mut bg) = sqlx::query_as::<_,Background>("select user_id,nonce,ciphertext,expires_at,snapshot from background_sessions where user_id=$1 and expires_at>now()").bind(uid).fetch_optional(&st.db).await? else { return Ok(()) };
    let Some(sealed) = vault::open(
        &st.pepper,
        &format!("background:{uid}"),
        &bg.nonce,
        &bg.ciphertext,
    ) else {
        return Ok(());
    };
    if poll_due(&st.db, uid).await? {
        match fetch_calendar(st, uid, sealed.cookies, bg.snapshot.as_ref()).await {
            Ok(snapshot) => {
                if let Some(previous) = &bg.snapshot {
                    queue_changes(st, uid, &sealed.student_id, previous, &snapshot).await?;
                }
                sqlx::query("update background_sessions set snapshot=$2,last_poll_at=now(),next_poll_at=now()+make_interval(secs=>$3),last_error=null where user_id=$1") .bind(uid).bind(&snapshot).bind(POLL_SECS as f64).execute(&st.db).await?;
                bg.snapshot = Some(snapshot);
            }
            Err(e) => {
                let expired = e.code == "session_expired" || e.code == "login_rejected";
                let error = if expired {
                    "학교 로그인이 만료됐어요. 백그라운드 알림을 다시 연결해 주세요.".to_owned()
                } else {
                    format!("학교의 최신 일정을 확인하지 못했어요. {}분 뒤 다시 확인해요.", POLL_SECS / 60)
                };
                sqlx::query("update background_sessions set last_error=$2,next_poll_at=case when $3 then expires_at else now()+make_interval(secs=>$4) end where user_id=$1").bind(uid).bind(error).bind(expired).bind(POLL_SECS as f64).execute(&st.db).await?;
            }
        }
    }
    let devices: Vec<Device> = sqlx::query_as("select id,user_id,kind,destination,leads,seat_leads from notification_devices where user_id=$1 and expires_at>now()").bind(uid).fetch_all(&st.db).await?;
    let items = bg
        .snapshot
        .as_ref()
        .and_then(|v| v["items"].as_array())
        .cloned()
        .unwrap_or_default();
    let tasks = todos::list(&st.db, uid).await?;
    let seat = db::active_seat_session(&st.db, uid).await?;
    let checks = db::item_checks(&st.db, uid).await?;
    let off = db::item_alerts_off(&st.db, uid).await?;
    for device in devices {
        let now = Utc::now().timestamp();
        let mut reminders = Vec::new();
        for item in &items {
            let key = item["key"].as_str().unwrap_or("");
            if checks
                .get(key)
                .copied()
                .unwrap_or(item["done"].as_bool().unwrap_or(false))
                || off.contains(key)
            {
                continue;
            }
            if let Some(due) = item["due"].as_i64() {
                for lead in &device.leads {
                    reminders.push(event(
                        format!("due:{key}:{due}:{lead}"),
                        due - i64::from(*lead) * 60,
                        &format!("마감 {lead}분 전이에요"),
                        item["title"].as_str().unwrap_or("과제·강의"),
                        &sealed.student_id,
                        json!({"kind":"item","key":key}),
                    ));
                }
            }
        }
        for todo in &tasks {
            if todo.done_at.is_some() || !todo.notify {
                continue;
            }
            if let Some(due) = todo.due_at {
                let due = due.timestamp() + if todo.all_day { 86400 } else { 0 };
                for lead in &device.leads {
                    reminders.push(event(
                        format!("due:todo:{}:{due}:{lead}", todo.id),
                        due - i64::from(*lead) * 60,
                        &format!("할 일 마감 {lead}분 전이에요"),
                        &todo.title,
                        &sealed.student_id,
                        json!({"kind":"todo","id":todo.id}),
                    ));
                }
            }
        }
        if let Some(s) = &seat {
            for lead in &device.seat_leads {
                reminders.push(event(
                    format!("due:seat:{}:{}:{lead}", s.id, s.expires_at.timestamp()),
                    s.expires_at.timestamp() - i64::from(*lead) * 60,
                    &format!("좌석 만료 {lead}분 전이에요"),
                    &format!("{} {}번", s.room_name, s.seat_no),
                    &sealed.student_id,
                    json!({"kind":"seat","id":s.id}),
                ));
            }
        }
        let keys: Vec<String> = reminders
            .iter()
            .filter(|(_, at, _)| *at >= now - 900 && *at < now + 30 * 86400)
            .map(|(k, _, _)| k.clone())
            .collect();
        sqlx::query("delete from notification_outbox where device_id=$1 and sent_at is null and event_key like 'due:%' and not (event_key=any($2))").bind(&device.id).bind(&keys).execute(&st.db).await?;
        for (key, at, payload) in reminders.into_iter().filter(|(k, _, _)| keys.contains(k)) {
            enqueue(&st.db, &device.id, &key, at, &payload, 900).await?;
        }
        deliver(st, &device).await?;
    }
    Ok(())
}
async fn fetch_calendar(
    st: &Shared,
    uid: i64,
    cookies: Vec<(String, String)>,
    previous: Option<&Value>,
) -> Result<Value, ApiError> {
    let school = hongsi_core::SchoolSession::from_sso_cookies(cookies)?;
    let courses = school.courses().await?;
    let (assignments, vods, notices) = tokio::join!(
        school.assignments(&courses), school.vods(&courses), fetch_notices(&school, previous)
    );
    let (assignments, vods, notices) = (assignments?, vods?, notices?);
    let snapshots = db::sync_assignments(&st.db, uid, &assignments).await?;
    let checks = db::item_checks(&st.db, uid).await?;
    let off = db::item_alerts_off(&st.db, uid).await?;
    let mut snapshot = json!(calendar::CalendarData {
        items: calendar::build(
            &assignments,
            &vods,
            &calendar::snapshot_infos(snapshots),
            &checks,
            &off,
            Utc::now().timestamp()
        ),
        courses,
        fetched_at: Utc::now().timestamp()
    });
    snapshot["notices"] = json!(notices);
    Ok(snapshot)
}

async fn fetch_notices(school: &hongsi_core::SchoolSession, previous: Option<&Value>) -> Result<Vec<Value>, ApiError> {
    let known = previous.and_then(|p| p["notices"].as_array());
    let mut notices = Vec::new();
    for page in 1..=6 {
        let items = school.notifications(page).await?;
        let count = items.len();
        let mut overlap = false;
        for item in items {
            let notice = json!(item);
            let key = notice_key(&notice);
            overlap |= known.is_some_and(|old| old.iter().any(|n| notice_key(n) == key));
            if !notices.iter().any(|n| notice_key(n) == key) { notices.push(notice); }
        }
        if known.is_none() || count < 15 || overlap { break; }
    }
    Ok(notices)
}

fn notice_key(notice: &Value) -> String {
    vault::token_hash(&json!([notice["url"], notice["course"], notice["section"], notice["message"], notice["kind"]]).to_string())
}

fn event(
    key: String,
    at: i64,
    title: &str,
    body: &str,
    account: &str,
    target: Value,
) -> (String, i64, Value) {
    (
        key,
        at,
        json!({"title":title,"body":body,"intent":{"version":1,"account":account,"target":target,"at":at*1000}}),
    )
}
async fn queue_changes(
    st: &Shared,
    uid: i64,
    account: &str,
    old: &Value,
    new: &Value,
) -> sqlx::Result<()> {
    let devices: Vec<(String,)> =
        sqlx::query_as("select id from notification_devices where user_id=$1 and change_alerts and expires_at>now()")
            .bind(uid)
            .fetch_all(&st.db)
            .await?;
    let item_changes = changes(old, new);
    for (key, title, item) in &item_changes {
        let (_, at, payload) = event(
            key.clone(),
            Utc::now().timestamp(),
            title,
            item["title"].as_str().unwrap_or("과제·강의"),
            account,
            json!({"kind":"item","key":item["key"]}),
        );
        for (id,) in &devices {
            enqueue(&st.db, id, &key, at, &payload, 3600).await?;
        }
    }
    if let (Some(before), Some(after)) = (old["notices"].as_array(), new["notices"].as_array()) {
        for notice in after {
            let fingerprint = notice_key(notice);
            if before.iter().any(|n| notice_key(n) == fingerprint) { continue; }
            if let Some(key) = notice_item_key(notice) {
                if item_changes.iter().any(|(_, _, item)| item["key"] == key)
                    || new["items"].as_array().is_some_and(|items| items.iter().any(|item|
                        item["key"] == key && (item["done"] == true || item["alert"] == false))) { continue; }
            }
            let (key, at, payload) = event(
                format!("change:notice:{fingerprint}"), Utc::now().timestamp(), "새 클래스룸 알림이 있어요",
                &format!("{} · {}", notice["course"].as_str().unwrap_or("클래스룸"), notice["message"].as_str().unwrap_or("새 알림")),
                account, json!({"kind":"notices"}),
            );
            for (id,) in &devices { enqueue(&st.db, id, &key, at, &payload, 3600).await?; }
        }
    }
    Ok(())
}

fn notice_item_key(notice: &Value) -> Option<String> {
    let url = reqwest::Url::parse(notice["url"].as_str()?).ok()?;
    let kind = match url.path() { "/mod/assign/view.php" => "assign", "/mod/vod/view.php" => "vod", _ => return None };
    let (_, id) = url.query_pairs().find(|(key, _)| key == "id")?;
    let id: i64 = id.parse().ok()?;
    Some(format!("{kind}:{id}"))
}

fn item_revision(item: &Value) -> String {
    vault::token_hash(&json!([
        item["title"], item["start"], item["due"], item["lateUntil"], item["modified"],
        item["introHtml"], item["attachments"], item["submit"], item["watch"]["required"]
    ]).to_string())
}

fn changes<'a>(old: &Value, new: &'a Value) -> Vec<(String, &'static str, &'a Value)> {
    let mut result = Vec::new();
    for item in new["items"].as_array().into_iter().flatten() {
        if item["done"] == true || item["alert"] == false {
            continue;
        }
        let prior = old["items"]
            .as_array()
            .and_then(|a| a.iter().find(|o| o["key"] == item["key"]));
        let label = match prior {
            None => "새 과제·강의가 등록됐어요",
            Some(p) if p["due"] != item["due"] => "마감 시간이 바뀌었어요",
            Some(p) if item_revision(p) != item_revision(item) => "과제·강의 정보가 수정됐어요",
            _ => continue,
        };
        result.push((
            format!(
                "change:{}:{}",
                item["key"], item_revision(item)
            ),
            label,
            item,
        ));
    }
    result
}
async fn enqueue(
    db: &PgPool,
    device: &str,
    key: &str,
    at: i64,
    payload: &Value,
    ttl: i64,
) -> sqlx::Result<()> {
    sqlx::query("insert into notification_outbox(device_id,event_key,payload,due_at,expires_at) values($1,$2,$3,to_timestamp($4),to_timestamp($5)) on conflict(device_id,event_key) do update set payload=excluded.payload where notification_outbox.sent_at is null")
        .bind(device).bind(key).bind(payload).bind(at as f64).bind((at+ttl) as f64).execute(db).await?;
    Ok(())
}
async fn deliver(st: &Shared, device: &Device) -> sqlx::Result<()> {
    let jobs:Vec<Job>=sqlx::query_as("select device_id,event_key,payload,attempts from notification_outbox where device_id=$1 and sent_at is null and due_at<=now() and expires_at>now() and next_try_at<=now() and attempts<5 and payload#>>'{intent,target,kind}' in ('item','todo','seat','notices') order by due_at limit 20").bind(&device.id).fetch_all(&st.db).await?;
    for j in jobs {
        match st
            .push
            .send(&device.kind, &device.destination, &j.payload, &j.event_key)
            .await
        {
            Ok(()) => {
                sqlx::query("update notification_outbox set sent_at=now() where device_id=$1 and event_key=$2").bind(&j.device_id).bind(&j.event_key).execute(&st.db).await?;
                sqlx::query("update notification_devices set last_error=null where id=$1")
                    .bind(&device.id)
                    .execute(&st.db)
                    .await?;
            }
            Err(PushError::Gone) => {
                sqlx::query("delete from notification_devices where id=$1")
                    .bind(&device.id)
                    .execute(&st.db)
                    .await?;
                break;
            }
            Err(e) => {
                let error = if matches!(e, PushError::Configuration) {
                    "서버 오류입니다. 문제가 지속되면 문의해 주세요."
                } else {
                    "푸시 전달에 실패해 다시 시도하고 있어요"
                };
                sqlx::query("update notification_devices set last_error=$2 where id=$1")
                    .bind(&device.id)
                    .bind(error)
                    .execute(&st.db)
                    .await?;
                sqlx::query("update notification_outbox set attempts=attempts+1,next_try_at=now()+make_interval(secs=>$3) where device_id=$1 and event_key=$2").bind(&j.device_id).bind(&j.event_key).bind((30*2_i32.pow(j.attempts as u32)) as f64).execute(&st.db).await?;
            }
        }
    }
    Ok(())
}

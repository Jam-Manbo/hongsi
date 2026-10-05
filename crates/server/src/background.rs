use crate::{
    auth::CurrentUser,
    db,
    error::ApiError,
    push,
    state::Shared,
    vault,
};
use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{FromRow, PgPool};
mod worker;

const POLL_SECS: i64 = 5 * 60;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInput {
    device_id: String,
    kind: String,
    destination: Value,
    seat_leads: Vec<i32>,
    classroom_alerts: bool,
    classroom_epoch: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    device_id: String,
    seat_leads: Vec<i32>,
    classroom_alerts: bool,
    classroom_epoch: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Renewal {
    device_id: String,
    #[serde(default)]
    consent: bool,
    cookies: Option<Vec<(String, String)>>,
}
#[derive(Deserialize)]
pub struct DeviceQuery {
    device: String,
}
#[derive(FromRow)]
struct Device {
    id: String,
    seat_leads: Vec<i32>,
}
#[derive(FromRow)]
struct Background {
    id: String,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    snapshot: Option<Value>,
}

fn valid_id(id: &str) -> bool {
    (20..=80).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}
fn valid_seat_leads(seats: &[i32]) -> bool {
    seats.len() <= 4
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
    let device: Option<(Option<String>, bool)> = sqlx::query_as("select last_error,classroom_alerts from notification_devices where id=$1 and user_id=$2 and expires_at>now()") .bind(&q.device).bind(uid).fetch_optional(&st.db).await?;
    let bg: Option<(DateTime<Utc>, Option<DateTime<Utc>>, Option<String>)> = sqlx::query_as(
        "select d.expires_at,b.last_poll_at,b.last_error from background_sessions b join background_devices d on d.user_id=b.user_id where b.user_id=$1 and d.id=$2 and d.expires_at>now()",
    )
    .bind(uid)
    .bind(&q.device)
    .fetch_optional(&st.db)
    .await?;
    let count: (i64, Option<DateTime<Utc>>) = sqlx::query_as("select count(*),min(due_at) from notification_outbox where device_id=$1 and sent_at is null and expires_at>now() and device_id in(select id from notification_devices where user_id=$2)").bind(&q.device).bind(uid).fetch_one(&st.db).await?;
    Ok(Json(
        json!({"registered":device.is_some(),"classroomAlerts":device.as_ref().is_some_and(|d|d.1),"error":device.and_then(|d|d.0),"consented":bg.is_some(),"expiresAt":bg.as_ref().map(|b|b.0.timestamp()),"lastPollAt":bg.as_ref().and_then(|b|b.1).map(|d|d.timestamp()),"schoolError":bg.and_then(|b|b.2),"scheduled":count.0,"nextAt":count.1.map(|d|d.timestamp()),"pollMinutes":POLL_SECS/60,"available":{"web":st.push.ready("web"),"fcm":st.push.ready("fcm"),"apns":st.push.ready("apns")},"publicKey":st.push.public_key}),
    ))
}
async fn verified_school(st: &Shared, user: &CurrentUser, supplied: Option<Vec<(String, String)>>) -> Result<std::sync::Arc<hongsi_core::SchoolSession>, ApiError> {
    let school = match (user.session.school().ok(), supplied) {
        (Some(school), _) => school,
        (None, Some(cookies)) if !cookies.is_empty() && cookies.len() <= 30
            && cookies.iter().all(|(k, v)| k.len() <= 100 && v.len() <= 8000) => std::sync::Arc::new(hongsi_core::SchoolSession::from_sso_cookies(cookies)?),
        _ => return Err(ApiError::bad_request("학교 로그인 세션이 필요해요.")),
    };
    let token = school.moodle_token().await?;
    let (owner, _) = hongsi_core::classroom::token_owner(&st.http, &token).await?;
    if owner != user.session.student_id {
        return Err(ApiError::bad_request("학교 세션의 계정이 현재 계정과 달라요."));
    }
    Ok(school)
}

pub async fn enable_sync(State(st): State<Shared>, user: CurrentUser, Json(b): Json<Renewal>) -> Result<Json<Value>, ApiError> {
    if !b.consent || !valid_id(&b.device_id) {
        return Err(ApiError::bad_request("백그라운드 동기화 동의와 기기 정보를 확인해 주세요."));
    }
    save_sync(&st, &user, b, false).await
}

pub async fn renew(State(st): State<Shared>, user: CurrentUser, Json(b): Json<Renewal>) -> Result<Json<Value>, ApiError> {
    if !valid_id(&b.device_id) { return Err(ApiError::bad_request("기기 정보를 확인해 주세요.")); }
    save_sync(&st, &user, b, true).await
}

async fn save_sync(st: &Shared, user: &CurrentUser, b: Renewal, renew_only: bool) -> Result<Json<Value>, ApiError> {
    let uid = user.session.user_id;
    let school = verified_school(st, user, b.cookies).await?;
    let sealed = vault::Sealed { device: false, name: user.session.name.clone(), student_id: user.session.student_id.clone(), school: Some(school.snapshot()) };
    let (nonce, ciphertext) = vault::seal(&st.pepper, &format!("background:{uid}:{}", b.device_id), &sealed)
        .ok_or_else(|| ApiError::conflict("학교 세션을 암호화하지 못했어요."))?;
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-uid).execute(&mut *tx).await?;
    if renew_only {
        let eligible: bool = sqlx::query_scalar("select exists(select 1 from background_devices d join background_sessions b on b.user_id=d.user_id where d.id=$1 and d.user_id=$2 and d.expires_at>now())")
            .bind(&b.device_id).bind(uid).fetch_one(&mut *tx).await?;
        if !eligible { return Err(ApiError::conflict("백그라운드 동기화 설정을 다시 적용해 주세요.")); }
    }
    let count: i64 = sqlx::query_scalar("select count(*) from background_devices where user_id=$1 and id<>$2 and expires_at>now()")
        .bind(uid).bind(&b.device_id).fetch_one(&mut *tx).await?;
    if count >= 10 { return Err(ApiError::bad_request("동기화할 수 있는 기기는 10개까지예요.")); }
    sqlx::query("insert into background_sessions(user_id) values($1) on conflict(user_id) do update set next_poll_at=case when background_sessions.last_error is not null then now() else background_sessions.next_poll_at end,last_error=null,poll_token=null,poll_until=null")
        .bind(uid).execute(&mut *tx).await?;
    let expires: Option<DateTime<Utc>> = sqlx::query_scalar("insert into background_devices(id,user_id,session_hash,nonce,ciphertext) values($1,$2,$3,$4,$5) on conflict(id) do update set session_hash=excluded.session_hash,nonce=excluded.nonce,ciphertext=excluded.ciphertext,expires_at=now()+interval '14 days' where background_devices.user_id=excluded.user_id returning expires_at")
        .bind(&b.device_id).bind(uid).bind(vault::token_hash(&user.token)).bind(nonce).bind(ciphertext).fetch_optional(&mut *tx).await?;
    let expires = expires.ok_or_else(|| ApiError::conflict("이 기기에서 이전에 로그인한 계정의 동기화를 먼저 해제해 주세요."))?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true,"expiresAt":expires.timestamp()})))
}

async fn notice_baseline(st: &Shared, uid: i64) -> Option<Vec<String>> {
    let row = sqlx::query_as::<_, Background>("select d.id,d.nonce,d.ciphertext,b.snapshot from background_devices d join background_sessions b on b.user_id=d.user_id where d.user_id=$1 and d.expires_at>now() order by d.expires_at desc limit 1")
        .bind(uid).fetch_optional(&st.db).await.ok()??;
    let sealed = vault::open(&st.pepper, &format!("background:{uid}:{}", row.id), &row.nonce, &row.ciphertext)?;
    let school = sealed.school_session().ok()?;
    let notices = fetch_notices(&school, None).await.ok()?;
    Some(notices.iter().map(notice_key).collect())
}

async fn prepare_baseline(st: &Shared, uid: i64, device: &str, enabled: bool, epoch: &str) -> Option<Vec<String>> {
    if !enabled { return None; }
    let current: Option<(bool, String)> = sqlx::query_as("select classroom_alerts,classroom_epoch from notification_devices where id=$1 and user_id=$2")
        .bind(device).bind(uid).fetch_optional(&st.db).await.ok()?;
    if current.is_some_and(|(on, previous)| on && previous == epoch) { return None; }
    notice_baseline(st, uid).await
}

pub async fn register(State(st): State<Shared>, user: CurrentUser, Json(b): Json<DeviceInput>) -> Result<Json<Value>, ApiError> {
    if !valid_id(&b.device_id) || !valid_id(&b.classroom_epoch) || !valid_seat_leads(&b.seat_leads) || !valid_destination(&b.kind, &b.destination) {
        return Err(ApiError::bad_request("알림 설정과 기기 정보를 확인해 주세요."));
    }
    if !st.push.ready(&b.kind) { return Err(ApiError::conflict("알림 서버에 연결하지 못했어요. 다시 시도해 주세요.")); }
    let uid = user.session.user_id;
    let baseline = prepare_baseline(&st, uid, &b.device_id, b.classroom_alerts, &b.classroom_epoch).await;
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-uid).execute(&mut *tx).await?;
    let eligible: bool = sqlx::query_scalar("select exists(select 1 from background_devices d join background_sessions b on b.user_id=d.user_id where d.id=$1 and d.user_id=$2 and d.session_hash=$3 and d.expires_at>now())")
        .bind(&b.device_id).bind(uid).bind(vault::token_hash(&user.token)).fetch_one(&mut *tx).await?;
    if !eligible { return Err(ApiError::conflict("백그라운드 동기화를 먼저 켜 주세요.")); }
    sqlx::query("insert into notification_devices(id,user_id,session_hash,kind,destination,seat_leads,classroom_epoch) values($1,$2,$3,$4,$5,$6,$7) on conflict(id) do update set session_hash=excluded.session_hash,kind=excluded.kind,destination=excluded.destination,seat_leads=excluded.seat_leads,expires_at=now()+interval '90 days',last_error=null where notification_devices.user_id=excluded.user_id")
        .bind(&b.device_id).bind(uid).bind(vault::token_hash(&user.token)).bind(&b.kind).bind(&b.destination).bind(&b.seat_leads).bind(&b.classroom_epoch).execute(&mut *tx).await?;
    apply_classroom(&mut tx, uid, &b.device_id, b.classroom_alerts, &b.classroom_epoch, baseline).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

pub async fn preferences(State(st): State<Shared>, user: CurrentUser, Json(b): Json<Preferences>) -> Result<Json<Value>, ApiError> {
    if !valid_id(&b.device_id) || !valid_id(&b.classroom_epoch) || !valid_seat_leads(&b.seat_leads) {
        return Err(ApiError::bad_request("알림 설정을 확인해 주세요."));
    }
    let baseline = prepare_baseline(&st, user.session.user_id, &b.device_id, b.classroom_alerts, &b.classroom_epoch).await;
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-user.session.user_id).execute(&mut *tx).await?;
    let saved = sqlx::query("update notification_devices set seat_leads=$3 where id=$1 and user_id=$2 and expires_at>now()")
        .bind(&b.device_id).bind(user.session.user_id).bind(&b.seat_leads).execute(&mut *tx).await?;
    if saved.rows_affected() != 1 { return Err(ApiError::conflict("알림 설정을 다시 적용해 주세요.")); }
    apply_classroom(&mut tx, user.session.user_id, &b.device_id, b.classroom_alerts, &b.classroom_epoch, baseline).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

async fn apply_classroom(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, uid: i64, device: &str, enabled: bool, epoch: &str, baseline: Option<Vec<String>>) -> Result<(), ApiError> {
    let current: Option<(bool, String)> = sqlx::query_as("select classroom_alerts,classroom_epoch from notification_devices where id=$1 and user_id=$2")
        .bind(device).bind(uid).fetch_optional(&mut **tx).await?;
    let changed = current.as_ref().is_some_and(|(was_enabled, was_epoch)| *was_enabled != enabled || was_epoch != epoch);
    if !changed { return Ok(()); }
    sqlx::query("delete from notification_outbox where device_id=$1 and sent_at is null and event_key like 'notice:%'")
        .bind(device).execute(&mut **tx).await?;
    let keys = if enabled { baseline } else { None };
    sqlx::query("update notification_devices set classroom_alerts=$2,classroom_epoch=$3,notice_keys=$4 where id=$1")
        .bind(device).bind(enabled).bind(epoch).bind(keys).execute(&mut **tx).await?;
    if enabled {
        sqlx::query("update background_sessions set next_poll_at=now(),poll_token=null,poll_until=null where user_id=$1").bind(uid).execute(&mut **tx).await?;
    }
    Ok(())
}

pub async fn disable(State(st): State<Shared>, user: CurrentUser, Query(q): Query<DeviceQuery>) -> Result<Json<Value>, ApiError> {
    if !valid_id(&q.device) { return Err(ApiError::bad_request("기기 정보를 확인해 주세요.")); }
    let mut tx = st.db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-user.session.user_id).execute(&mut *tx).await?;
    sqlx::query("delete from notification_devices where user_id=$1 and id=$2")
        .bind(user.session.user_id).bind(&q.device).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Json(json!({"ok":true})))
}

pub async fn revoke_login(db: &PgPool, token: &str) -> sqlx::Result<Vec<String>> {
    let hash = vault::token_hash(token);
    let mut tx = db.begin().await?;
    let Some(record) = sqlx::query_as::<_, db::LoginRecord>("select user_id,family_hash,expires_at,revoked_at,logout_all_at from auth_sessions where token_hash=$1")
        .bind(&hash).fetch_optional(&mut *tx).await? else { return Ok(Vec::new()); };
    let uid = record.user_id;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-uid).execute(&mut *tx).await?;
    let hashes: Vec<String> = sqlx::query_scalar("select token_hash from auth_sessions where family_hash=$1 and user_id=$2")
        .bind(&record.family_hash).bind(uid).fetch_all(&mut *tx).await?;
    sqlx::query("delete from notification_devices where session_hash=any($1)").bind(&hashes).execute(&mut *tx).await?;
    sqlx::query("delete from background_devices where session_hash=any($1)").bind(&hashes).execute(&mut *tx).await?;
    sqlx::query("delete from background_sessions b where user_id=$1 and not exists(select 1 from background_devices d where d.user_id=b.user_id)").bind(uid).execute(&mut *tx).await?;
    sqlx::query("update background_sessions set next_poll_at=now(),last_error=null where user_id=$1 and last_error is not null").bind(uid).execute(&mut *tx).await?;
    sqlx::query("delete from remembered_sessions where token_hash=any($1)").bind(&hashes).execute(&mut *tx).await?;
    sqlx::query("update auth_sessions set revoked_at=coalesce(revoked_at,now()) where token_hash=any($1)").bind(&hashes).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(hashes)
}

pub async fn revoke_account(db: &PgPool, user_id: i64, initiator_hash: &str) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-user_id).execute(&mut *tx).await?;
    sqlx::query("delete from notification_devices where user_id=$1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("delete from background_sessions where user_id=$1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("delete from remembered_sessions where user_id=$1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("update auth_sessions set revoked_at=coalesce(revoked_at,now()) where user_id=$1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("update auth_sessions set logout_all_at=now() where token_hash=$1 and user_id=$2").bind(initiator_hash).bind(user_id).execute(&mut *tx).await?;
    tx.commit().await
}

pub async fn run(st: Shared) {
    worker::run(st).await;
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

fn deadline_title(kind: &str, lead: i32) -> String {
    match lead {
        0 => format!("{kind} 마감 시간이에요."),
        1440 => format!("{kind} 마감 하루 전이에요."),
        180 => format!("{kind} 마감 3시간 전이에요."),
        60 => format!("{kind} 마감 1시간 전이에요."),
        _ => format!("{kind} 마감 {lead}분 전이에요."),
    }
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
async fn queue_notices(db: &mut sqlx::PgConnection, uid: i64, account: &str, snapshot: &Value) -> sqlx::Result<()> {
    let Some(notices) = snapshot["notices"].as_array() else { return Ok(()); };
    let devices: Vec<(String, String, Option<Vec<String>>)> = sqlx::query_as("select id,classroom_epoch,notice_keys from notification_devices where user_id=$1 and classroom_alerts and expires_at>now() order by id for update")
        .bind(uid).fetch_all(&mut *db).await?;
    for (id, epoch, baseline) in devices {
        let mut known = baseline.clone().unwrap_or_default();
        if let Some(baseline) = baseline {
            for notice in notices {
                let fingerprint = notice_key(notice);
                if baseline.contains(&fingerprint) { break; }
                let (key, at, payload) = event(
                    format!("notice:{epoch}:{fingerprint}"), Utc::now().timestamp(), "홍시 · 클래스룸 알림",
                    &format!("{}\n{}", notice["course"].as_str().unwrap_or("클래스룸"), notice["message"].as_str().unwrap_or("새 알림")),
                    account, json!({"kind":"notices","url":notice["url"]}),
                );
                enqueue(&mut *db, &id, &key, at, &payload, 3600).await?;
            }
        }
        let mut keys: Vec<String> = notices.iter().map(notice_key).collect();
        known.retain(|key| !keys.contains(key));
        keys.extend(known);
        keys.truncate(500);
        sqlx::query("update notification_devices set notice_keys=$2 where id=$1")
            .bind(id).bind(keys).execute(&mut *db).await?;
    }
    Ok(())
}

async fn enqueue(
    db: &mut sqlx::PgConnection,
    device: &str,
    key: &str,
    at: i64,
    payload: &Value,
    ttl: i64,
) -> sqlx::Result<()> {
    sqlx::query(
        "insert into notification_outbox(device_id,event_key,payload,due_at,expires_at)
         select $1,$2,$3,to_timestamp($4),to_timestamp($5)
         where $2 not like 'due:%' or to_timestamp($4)>now()
         on conflict(device_id,event_key) do update set payload=excluded.payload
         where notification_outbox.sent_at is null"
    )
        .bind(device).bind(key).bind(payload).bind(at as f64).bind((at+ttl) as f64).execute(db).await?;
    Ok(())
}

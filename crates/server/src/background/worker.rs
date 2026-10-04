use super::{deadline_title, event, fetch_notices, queue_notices, Device, POLL_SECS};
use crate::{
    calendar, db, error::ApiError, preferences, push::PushError, state::Shared, todos, vault,
};
use chrono::Utc;
use futures::{stream, StreamExt};
use hongsi_core::models::{Assignment, Course, Vod};
use serde_json::{json, Value};
use sqlx::{FromRow, PgConnection, PgPool};
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

const POLL_WORKERS: usize = 3;
const SEND_WORKERS: usize = 4;
const POLL_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_LEASE_SECS: f64 = 120.0;
const SEND_TIMEOUT: Duration = Duration::from_secs(45);
const SEND_LEASE_SECS: f64 = 90.0;

#[derive(FromRow)]
struct PollJob {
    user_id: i64,
    id: String,
    session_hash: String,
    nonce: Vec<u8>,
    ciphertext: Vec<u8>,
    snapshot: Option<Value>,
    poll_token: String,
}

struct SchoolData {
    account: String,
    courses: Vec<Course>,
    assignments: Vec<Assignment>,
    vods: Vec<Vod>,
    notices: Vec<Value>,
}

#[derive(FromRow)]
struct SendJob {
    user_id: i64,
    device_id: String,
    event_key: String,
    claim_token: String,
}

struct Delivery {
    kind: String,
    destination: Value,
    session_hash: String,
    payload: Value,
}

type Reminder = (String, i64, Value);
struct ReminderState {
    items: Vec<Value>,
    defaults: Vec<i32>,
    tasks: Vec<todos::Todo>,
    seat: Option<db::SeatSession>,
    checks: HashMap<String, bool>,
    off: HashSet<String>,
    leads: HashMap<String, Vec<i32>>,
}

fn token() -> String {
    hex::encode(rand::random::<[u8; 16]>())
}

pub(super) async fn run(st: Shared) {
    // Each lane has its own capacity; a slow school cannot occupy a sender.
    tokio::join!(
        futures::future::join_all((0..POLL_WORKERS).map(|_| poll_loop(&st))),
        schedule_loop(&st),
        futures::future::join_all((0..SEND_WORKERS).map(|_| send_loop(&st))),
    );
}

async fn poll_loop(st: &Shared) {
    loop {
        match claim_poll(&st.db).await {
            Ok(Some(job)) => {
                if let Err(e) = poll(st, job).await {
                    tracing::warn!(code=?e.as_database_error().and_then(|e|e.code()), "학교 동기화 저장 실패");
                }
            }
            result => {
                if let Err(e) = result {
                    tracing::warn!(code=?e.as_database_error().and_then(|e|e.code()), "학교 동기화 작업 조회 실패");
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn claim_poll(db: &PgPool) -> sqlx::Result<Option<PollJob>> {
    sqlx::query_as("with candidate as (
        select b.user_id from background_sessions b
        where (b.next_poll_at<=now() or (b.last_error is null and b.last_poll_at<=now()-make_interval(secs=>$3)))
        and (b.poll_until is null or b.poll_until<=now())
        and exists(select 1 from background_devices d join auth_sessions a on a.token_hash=d.session_hash
            where d.user_id=b.user_id and d.expires_at>now() and a.revoked_at is null)
        order by b.next_poll_at for update of b skip locked limit 1
    ), claimed as (
        update background_sessions b set poll_token=$1,poll_until=now()+make_interval(secs=>$2)
        from candidate c where b.user_id=c.user_id returning b.*
    ) select b.user_id,d.id,d.session_hash,d.nonce,d.ciphertext,b.snapshot,b.poll_token from claimed b
      cross join lateral (select d.* from background_devices d join auth_sessions a on a.token_hash=d.session_hash
        where d.user_id=b.user_id and d.expires_at>now() and a.revoked_at is null order by d.expires_at desc limit 1) d")
        .bind(token()).bind(POLL_LEASE_SECS).bind(POLL_SECS as f64).fetch_optional(db).await
}

async fn fetch_school(
    sealed: &vault::Sealed,
    previous: Option<&Value>,
) -> Result<SchoolData, ApiError> {
    let school = sealed.school_session()?;
    let courses = school.courses().await?;
    let (assignments, vods, notices) = tokio::join!(
        school.assignments(&courses),
        school.vods(&courses),
        fetch_notices(&school, previous)
    );
    Ok(SchoolData {
        account: sealed.student_id.clone(),
        courses,
        assignments: assignments?,
        vods: vods?,
        notices: notices?,
    })
}

async fn poll(st: &Shared, job: PollJob) -> sqlx::Result<()> {
    let result = match vault::open(
        &st.pepper,
        &format!("background:{}:{}", job.user_id, job.id),
        &job.nonce,
        &job.ciphertext,
    ) {
        Some(sealed) => {
            match tokio::time::timeout(POLL_TIMEOUT, fetch_school(&sealed, job.snapshot.as_ref()))
                .await
            {
                Ok(result) => result.map_err(|e| e.code.to_owned()),
                Err(_) => Err("school_unreachable".into()),
            }
        }
        None => Err("session_expired".into()),
    };
    finish_poll(&st.db, &job, result).await
}

async fn finish_poll(
    db: &PgPool,
    job: &PollJob,
    result: Result<SchoolData, String>,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(-job.user_id)
        .execute(&mut *tx)
        .await?;
    // A renewal, logout, expired lease or replacement worker invalidates this result.
    let valid: Option<i64> = sqlx::query_scalar("select b.user_id from background_sessions b
        where b.user_id=$1 and b.poll_token=$2 and b.poll_until>now()
        and exists(select 1 from background_devices d join auth_sessions a on a.token_hash=d.session_hash
            where d.id=$3 and d.user_id=b.user_id and d.session_hash=$4 and d.nonce=$5 and d.expires_at>now() and a.revoked_at is null)
        for update of b")
        .bind(job.user_id).bind(&job.poll_token).bind(&job.id).bind(&job.session_hash).bind(&job.nonce)
        .fetch_optional(&mut *tx).await?;
    if valid.is_none() {
        return Ok(());
    }
    match result {
        Ok(data) => {
            let snapshots =
                db::sync_assignments_in(&mut tx, job.user_id, &data.assignments).await?;
            let snapshot = json!({
                "courses": data.courses,
                "items": calendar::build(&data.assignments, &data.vods, &calendar::snapshot_infos(snapshots),
                    &HashMap::new(), &HashSet::new(), &HashMap::new(), Utc::now().timestamp()),
                "notices": data.notices,
                "fetchedAt": Utc::now().timestamp(),
            });
            // Only a fresh, still-owned poll may advance notice baselines. Replaying an older
            // cached snapshot after enabling notifications could otherwise notify old notices.
            queue_notices(&mut tx, job.user_id, &data.account, &snapshot).await?;
            // User overrides are applied from current DB state when scheduling/sending.
            sqlx::query("update background_sessions set snapshot=$3,last_poll_at=now(),next_poll_at=now()+make_interval(secs=>$4),last_error=null,poll_token=null,poll_until=null where user_id=$1 and poll_token=$2")
                .bind(job.user_id).bind(&job.poll_token).bind(snapshot).bind(POLL_SECS as f64).execute(&mut *tx).await?;
        }
        Err(code) => {
            let expired = matches!(
                code.as_str(),
                "session_expired" | "classroom_token_expired" | "login_rejected"
            );
            let message = if expired {
                "학교 로그인이 만료됐어요. 백그라운드 동기화 설정을 다시 적용해 주세요.".to_owned()
            } else {
                format!(
                    "학교의 최신 일정을 확인하지 못했어요. {}분 뒤 다시 확인할게요.",
                    POLL_SECS / 60
                )
            };
            sqlx::query("update background_sessions set last_error=$3,next_poll_at=case when $4 then coalesce((select max(expires_at) from background_devices where user_id=$1),now()) else now()+make_interval(secs=>$5) end,poll_token=null,poll_until=null where user_id=$1 and poll_token=$2")
                .bind(job.user_id).bind(&job.poll_token).bind(message).bind(expired).bind(POLL_SECS as f64).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}

async fn schedule_loop(st: &Shared) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        if let Err(e) = schedule_cycle(st).await {
            tracing::warn!(code=?e.as_database_error().and_then(|e|e.code()), "알림 예약 처리 실패");
        }
    }
}

async fn schedule_cycle(st: &Shared) -> sqlx::Result<()> {
    sqlx::query("delete from background_devices where expires_at<now()")
        .execute(&st.db)
        .await?;
    sqlx::query("delete from notification_devices where expires_at<now()")
        .execute(&st.db)
        .await?;
    sqlx::query("delete from background_sessions b where not exists(select 1 from background_devices d where d.user_id=b.user_id)").execute(&st.db).await?;
    sqlx::query("delete from notification_outbox where expires_at<now()-interval '7 days'")
        .execute(&st.db)
        .await?;
    let users: Vec<i64> = sqlx::query_scalar("select user_id from background_sessions")
        .fetch_all(&st.db)
        .await?;
    stream::iter(users).for_each_concurrent(2, |uid| async move {
        if let Err(e) = schedule_user(st, uid).await { tracing::warn!(code=?e.as_database_error().and_then(|e|e.code()), "계정 알림 예약 실패"); }
    }).await;
    Ok(())
}

async fn schedule_user(st: &Shared, uid: i64) -> sqlx::Result<()> {
    let mut tx = st.db.begin().await?;
    let acquired: bool = sqlx::query_scalar("select pg_try_advisory_xact_lock($1)")
        .bind(-uid)
        .fetch_one(&mut *tx)
        .await?;
    if !acquired {
        return Ok(());
    }
    let bg: Option<super::Background> = sqlx::query_as("select d.id,d.nonce,d.ciphertext,b.snapshot from background_devices d
        join background_sessions b on b.user_id=d.user_id join auth_sessions a on a.token_hash=d.session_hash
        where d.user_id=$1 and d.expires_at>now() and a.revoked_at is null order by d.expires_at desc limit 1")
        .bind(uid).fetch_optional(&mut *tx).await?;
    let Some(bg) = bg else {
        return Ok(());
    };
    let Some(sealed) = vault::open(
        &st.pepper,
        &format!("background:{uid}:{}", bg.id),
        &bg.nonce,
        &bg.ciphertext,
    ) else {
        return Ok(());
    };
    let devices: Vec<Device> = sqlx::query_as("select n.id,n.seat_leads from notification_devices n
        join background_devices d on d.id=n.id and d.session_hash=n.session_hash join auth_sessions a on a.token_hash=d.session_hash
        where n.user_id=$1 and n.expires_at>now() and d.expires_at>now() and a.revoked_at is null order by n.id for update of n")
        .bind(uid).fetch_all(&mut *tx).await?;
    let state = load_reminders(&mut tx, uid, bg.snapshot.as_ref()).await?;
    for device in devices {
        let now = Utc::now().timestamp();
        let reminders: Vec<_> = reminders(&state, &device.seat_leads, &sealed.student_id)
            .into_iter()
            .filter(|(_, at, _)| *at >= now - 900 && *at < now + 30 * 86400)
            .collect();
        let keys: Vec<_> = reminders.iter().map(|(key, _, _)| key).collect();
        sqlx::query("delete from notification_outbox where device_id=$1 and sent_at is null and event_key like 'due:%' and not(event_key=any($2))")
            .bind(&device.id).bind(&keys).execute(&mut *tx).await?;
        for (key, at, payload) in reminders {
            super::enqueue(&mut tx, &device.id, &key, at, &payload, 900).await?;
        }
    }
    tx.commit().await
}

async fn load_reminders(
    db: &mut PgConnection,
    uid: i64,
    snapshot: Option<&Value>,
) -> sqlx::Result<ReminderState> {
    Ok(ReminderState {
        items: snapshot
            .and_then(|v| v["items"].as_array())
            .cloned()
            .unwrap_or_default(),
        defaults: preferences::load(&mut *db, uid).await?.alert_leads,
        tasks: todos::list(&mut *db, uid).await?,
        seat: db::active_seat_session(&mut *db, uid).await?,
        checks: db::item_checks(&mut *db, uid).await?,
        off: db::item_alerts_off(&mut *db, uid).await?,
        leads: db::item_alert_leads(&mut *db, uid).await?,
    })
}

fn reminders(state: &ReminderState, seat_leads: &[i32], account: &str) -> Vec<Reminder> {
    let mut reminders = Vec::new();
    for item in &state.items {
        let key = item["key"].as_str().unwrap_or("");
        let school_done = matches!(item["status"].as_str(), Some("submitted" | "done"));
        if state.checks.get(key).copied().unwrap_or(school_done) || state.off.contains(key) {
            continue;
        }
        if let Some(due) = item["due"].as_i64() {
            for lead in state.leads.get(key).unwrap_or(&state.defaults) {
                reminders.push(event(
                    format!("due:{key}:{due}:{lead}"),
                    due - i64::from(*lead) * 60,
                    &deadline_title(
                        if item["kind"] == "vod" {
                            "온라인 강의"
                        } else {
                            "과제"
                        },
                        *lead,
                    ),
                    item["title"].as_str().unwrap_or("과제·강의"),
                    account,
                    json!({"kind":"item","key":key}),
                ));
            }
        }
    }
    for task in &state.tasks {
        if task.done_at.is_some() || !task.notify {
            continue;
        }
        if let Some(due) = task.due_at {
            let due = due.timestamp() + if task.all_day { 86400 } else { 0 };
            for lead in task.alert_leads.as_ref().unwrap_or(&state.defaults) {
                reminders.push(event(
                    format!("due:todo:{}:{due}:{lead}", task.id),
                    due - i64::from(*lead) * 60,
                    &deadline_title("할 일", *lead),
                    &task.title,
                    account,
                    json!({"kind":"todo","id":task.id}),
                ));
            }
        }
    }
    if let Some(seat) = &state.seat {
        for lead in seat_leads {
            reminders.push(event(
                format!(
                    "due:seat:{}:{}:{lead}",
                    seat.id,
                    seat.expires_at.timestamp()
                ),
                seat.expires_at.timestamp() - i64::from(*lead) * 60,
                &format!("좌석 이용 종료까지 {lead}분 남았어요."),
                "",
                account,
                json!({"kind":"seat","id":seat.id}),
            ));
        }
    }
    reminders
}

async fn send_loop(st: &Shared) {
    loop {
        match claim_send(&st.db).await {
            Ok(Some(job)) => {
                if let Err(e) = deliver(st, job).await {
                    tracing::warn!(code=?e.as_database_error().and_then(|e|e.code()), "알림 발송 결과 저장 실패");
                }
            }
            result => {
                if let Err(e) = result {
                    tracing::warn!(code=?e.as_database_error().and_then(|e|e.code()), "알림 발송 작업 조회 실패");
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn claim_send(db: &PgPool) -> sqlx::Result<Option<SendJob>> {
    sqlx::query_as("with candidate as (
        select o.device_id,o.event_key,d.user_id from notification_outbox o
        join notification_devices d on d.id=o.device_id
        join background_devices b on b.id=d.id and b.session_hash=d.session_hash
        join auth_sessions a on a.token_hash=b.session_hash
        where o.sent_at is null and o.due_at<=now() and o.expires_at>now() and o.next_try_at<=now() and o.attempts<5
        and (o.claim_until is null or o.claim_until<=now()) and d.expires_at>now() and b.expires_at>now() and a.revoked_at is null
        order by o.due_at for update of o skip locked limit 1
    ) update notification_outbox o set claim_token=$1,claim_until=now()+make_interval(secs=>$2),attempts=o.attempts+1
      from candidate c where o.device_id=c.device_id and o.event_key=c.event_key
      returning c.user_id,o.device_id,o.event_key,o.claim_token")
        .bind(token()).bind(SEND_LEASE_SECS).fetch_optional(db).await
}

async fn prepare_delivery(db: &PgPool, job: &SendJob) -> sqlx::Result<Option<Delivery>> {
    let mut tx = db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(-job.user_id)
        .execute(&mut *tx)
        .await?;
    type Current = (
        String,
        Value,
        String,
        Vec<i32>,
        bool,
        String,
        Value,
        Option<Value>,
    );
    let current: Option<Current> = sqlx::query_as("select d.kind,d.destination,d.session_hash,d.seat_leads,d.classroom_alerts,d.classroom_epoch,o.payload,b.snapshot
        from notification_outbox o join notification_devices d on d.id=o.device_id
        join background_devices c on c.id=d.id and c.session_hash=d.session_hash
        join background_sessions b on b.user_id=d.user_id join auth_sessions a on a.token_hash=d.session_hash
        where o.device_id=$1 and o.event_key=$2 and o.claim_token=$3 and o.claim_until>now() and o.sent_at is null and o.expires_at>now()
        and d.user_id=$4 and d.expires_at>now() and c.expires_at>now() and a.revoked_at is null")
        .bind(&job.device_id).bind(&job.event_key).bind(&job.claim_token).bind(job.user_id).fetch_optional(&mut *tx).await?;
    let Some((kind, destination, session_hash, leads, notices, epoch, payload, snapshot)) = current
    else {
        return Ok(None);
    };
    let payload = if job.event_key.starts_with("due:") {
        let state = load_reminders(&mut tx, job.user_id, snapshot.as_ref()).await?;
        let account = payload["intent"]["account"].as_str().unwrap_or("");
        reminders(&state, &leads, account)
            .into_iter()
            .find(|(key, _, _)| *key == job.event_key)
            .map(|(_, _, p)| p)
    } else if notices
        && job.event_key.starts_with(&format!("notice:{epoch}:"))
        && payload["intent"]["target"]["kind"] == "notices"
    {
        Some(payload)
    } else {
        None
    };
    let Some(payload) = payload else {
        sqlx::query("delete from notification_outbox where device_id=$1 and event_key=$2 and claim_token=$3")
            .bind(&job.device_id).bind(&job.event_key).bind(&job.claim_token).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(None);
    };
    // Revalidate ownership after reading current settings, before starting any external request.
    let renewed = sqlx::query("update notification_outbox set claim_until=now()+make_interval(secs=>$4)
        where device_id=$1 and event_key=$2 and claim_token=$3 and claim_until>now() and expires_at>now()")
        .bind(&job.device_id).bind(&job.event_key).bind(&job.claim_token).bind(SEND_LEASE_SECS)
        .execute(&mut *tx).await?;
    if renewed.rows_affected() != 1 {
        return Ok(None);
    }
    tx.commit().await?;
    Ok(Some(Delivery {
        kind,
        destination,
        session_hash,
        payload,
    }))
}

async fn deliver(st: &Shared, job: SendJob) -> sqlx::Result<()> {
    let db = &st.db;
    let Some(Delivery {
        kind,
        destination,
        session_hash,
        payload,
    }) = prepare_delivery(db, &job).await?
    else {
        return Ok(());
    };
    let result = tokio::time::timeout(
        SEND_TIMEOUT,
        st.push.send(&kind, &destination, &payload, &job.event_key),
    )
    .await
    .unwrap_or(Err(PushError::Retry));
    let mut tx = db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)")
        .bind(-job.user_id)
        .execute(&mut *tx)
        .await?;
    // Match device-deletion lock order (device, then its outbox) to avoid a cascade deadlock.
    let device: Option<String> =
        sqlx::query_scalar("select id from notification_devices where id=$1 for update")
            .bind(&job.device_id)
            .fetch_optional(&mut *tx)
            .await?;
    if device.is_none() {
        return Ok(());
    }
    let current: Option<i32> = sqlx::query_scalar("select attempts from notification_outbox where device_id=$1 and event_key=$2 and claim_token=$3 for update")
        .bind(&job.device_id).bind(&job.event_key).bind(&job.claim_token).fetch_optional(&mut *tx).await?;
    if current.is_none() {
        return Ok(());
    }
    match result {
        Ok(()) => {
            sqlx::query("update notification_outbox set sent_at=now(),claim_token=null,claim_until=null where device_id=$1 and event_key=$2 and claim_token=$3")
                .bind(&job.device_id).bind(&job.event_key).bind(&job.claim_token).execute(&mut *tx).await?;
            sqlx::query("update notification_devices set last_error=null where id=$1 and session_hash=$2 and kind=$3 and destination=$4")
                .bind(&job.device_id).bind(&session_hash).bind(&kind).bind(&destination).execute(&mut *tx).await?;
        }
        Err(PushError::Gone) => {
            sqlx::query("delete from notification_devices where id=$1 and session_hash=$2 and kind=$3 and destination=$4")
                .bind(&job.device_id).bind(&session_hash).bind(&kind).bind(&destination).execute(&mut *tx).await?;
        }
        Err(error) => {
            let message = if matches!(error, PushError::Configuration) {
                "서버 오류입니다. 문제가 지속되면 문의해 주세요."
            } else {
                "알림을 보내지 못해 다시 시도하고 있어요."
            };
            sqlx::query("update notification_devices set last_error=$2 where id=$1 and session_hash=$3 and kind=$4 and destination=$5")
                .bind(&job.device_id).bind(message).bind(&session_hash).bind(&kind).bind(&destination).execute(&mut *tx).await?;
            sqlx::query("update notification_outbox set next_try_at=now()+make_interval(secs=>30*power(2,attempts-1)),claim_token=null,claim_until=null where device_id=$1 and event_key=$2 and claim_token=$3")
                .bind(&job.device_id).bind(&job.event_key).bind(&job.claim_token).execute(&mut *tx).await?;
        }
    }
    tx.commit().await
}

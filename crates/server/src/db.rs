use std::collections::{HashMap, HashSet};

use chrono::{DateTime, TimeZone, Utc};
use hongsi_core::models::Assignment;
use serde::Serialize;
use sqlx::{FromRow, PgPool, Row};

pub fn ts(t: Option<i64>) -> Option<DateTime<Utc>> {
    t.and_then(|t| Utc.timestamp_opt(t, 0).single())
}

pub async fn upsert_user(db: &PgPool, user_key: &str) -> sqlx::Result<i64> {
    sqlx::query_scalar(
        "insert into users (user_key) values ($1)
         on conflict (user_key) do update set last_login_at = now()
         returning id",
    )
    .bind(user_key)
    .fetch_one(db)
    .await
}


pub async fn save_login(
    db: &PgPool,
    token_hash: &str,
    user_id: i64,
    sealed: Option<(&[u8], &[u8])>,
    expires_at: DateTime<Utc>,
    previous_hash: Option<&str>,
) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-user_id).execute(&mut *tx).await?;
    let family_hash = if let Some(previous) = previous_hash {
        sqlx::query_scalar::<_, String>("select family_hash from auth_sessions where token_hash=$1 and user_id=$2 and revoked_at is null")
            .bind(previous).bind(user_id).fetch_optional(&mut *tx).await?.ok_or(sqlx::Error::RowNotFound)?
    } else { token_hash.to_string() };
    sqlx::query("insert into auth_sessions(token_hash,family_hash,user_id,expires_at) values($1,$2,$3,$4)")
        .bind(token_hash).bind(family_hash).bind(user_id).bind(expires_at).execute(&mut *tx).await?;
    if let Some((nonce, ciphertext)) = sealed {
        sqlx::query("insert into remembered_sessions(token_hash,user_id,nonce,ciphertext,expires_at) values($1,$2,$3,$4,$5)")
            .bind(token_hash).bind(user_id).bind(nonce).bind(ciphertext).bind(expires_at).execute(&mut *tx).await?;
    }
    if let Some(previous) = previous_hash {
        sqlx::query("update background_devices set session_hash=$2 where session_hash=$1 and user_id=$3")
            .bind(previous).bind(token_hash).bind(user_id).execute(&mut *tx).await?;
        sqlx::query("delete from remembered_sessions where token_hash=$1 and user_id=$2")
            .bind(previous).bind(user_id).execute(&mut *tx).await?;
        sqlx::query("update auth_sessions set expires_at=least(expires_at,now()) where token_hash=$1 and user_id=$2")
            .bind(previous).bind(user_id).execute(&mut *tx).await?;
    }
    tx.commit().await
}

#[derive(FromRow)]
pub struct LoginRecord {
    pub user_id: i64,
    pub family_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub logout_all_at: Option<DateTime<Utc>>,
}

pub async fn login_record(db: &PgPool, token_hash: &str) -> sqlx::Result<Option<LoginRecord>> {
    sqlx::query_as("select user_id,family_hash,expires_at,revoked_at,logout_all_at from auth_sessions where token_hash=$1")
        .bind(token_hash).fetch_optional(db).await
}

pub async fn load_remembered(db: &PgPool, token_hash: &str) -> sqlx::Result<Option<(i64, Vec<u8>, Vec<u8>, DateTime<Utc>)>> {
    sqlx::query_as("select user_id, nonce, ciphertext, expires_at from remembered_sessions where token_hash = $1 and expires_at > now()")
        .bind(token_hash)
        .fetch_optional(db)
        .await
}

pub async fn delete_remembered(db: &PgPool, token_hash: &str) -> sqlx::Result<()> {
    sqlx::query("delete from remembered_sessions where token_hash = $1")
        .bind(token_hash)
        .execute(db)
        .await?;
    Ok(())
}


pub async fn clean_remembered(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query("delete from remembered_sessions where expires_at <= now()").execute(db).await?;
    sqlx::query("delete from auth_sessions a where coalesce(revoked_at,expires_at) < now()-interval '14 days' and not exists(select 1 from background_devices d join auth_sessions linked on linked.token_hash=d.session_hash where linked.family_hash=a.family_hash) and not exists(select 1 from auth_sessions sibling where sibling.family_hash=a.family_hash and sibling.revoked_at is null and sibling.expires_at>now())").execute(db).await?;
    Ok(())
}


pub struct Snapshot {
    pub first_seen: DateTime<Utc>,
    pub first_due: Option<DateTime<Utc>>,
    pub first_intro_html: String,
    pub change_count: i32,
}

pub async fn sync_assignments(db: &PgPool, user_id: i64, items: &[Assignment]) -> sqlx::Result<HashMap<i64, Snapshot>> {
    let mut tx = db.begin().await?;
    let out = sync_assignments_in(&mut tx, user_id, items).await?;
    tx.commit().await?;
    Ok(out)
}

pub async fn sync_assignments_in(db: &mut sqlx::PgConnection, user_id: i64, items: &[Assignment]) -> sqlx::Result<HashMap<i64, Snapshot>> {
    let mut out = HashMap::new();
    for a in items {
        let row = sqlx::query(
            "insert into assignment_snapshots
                 (user_id, cmid, course_id, name, first_due_at, first_intro_html, due_at, modified_at)
             values ($1, $2, $3, $4, $5, $6, $5, $7)
             on conflict (user_id, cmid) do update set
                 change_count = assignment_snapshots.change_count
                     + case when assignment_snapshots.modified_at is distinct from excluded.modified_at then 1 else 0 end,
                 last_changed_at = case when assignment_snapshots.modified_at is distinct from excluded.modified_at
                     then now() else assignment_snapshots.last_changed_at end,
                 course_id = excluded.course_id,
                 name = excluded.name,
                 due_at = excluded.due_at,
                 modified_at = excluded.modified_at
             returning first_seen_at, first_due_at, first_intro_html, change_count",
        )
        .bind(user_id)
        .bind(a.cmid)
        .bind(a.course_id)
        .bind(&a.name)
        .bind(ts(a.due))
        .bind(&a.intro_html)
        .bind(ts(Some(a.modified)))
        .fetch_one(&mut *db)
        .await?;
        out.insert(
            a.cmid,
            Snapshot {
                first_seen: row.try_get(0)?,
                first_due: row.try_get(1)?,
                first_intro_html: row.try_get(2)?,
                change_count: row.try_get(3)?,
            },
        );
    }
    Ok(out)
}

pub async fn item_checks(db: impl sqlx::Executor<'_, Database = sqlx::Postgres>, user_id: i64) -> sqlx::Result<HashMap<String, bool>> {
    let rows: Vec<(String, bool)> = sqlx::query_as("select item_key, done from item_checks where user_id = $1")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().collect())
}

pub async fn set_item_check(db: &PgPool, user_id: i64, key: &str, done: bool) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    crate::todos::lock(&mut tx, user_id).await?;
    sqlx::query(
        "insert into item_checks (user_id, item_key, done) values ($1, $2, $3)
         on conflict (user_id, item_key) do update set done = excluded.done, updated_at = now()",
    )
    .bind(user_id)
    .bind(key)
    .bind(done)
    .execute(&mut *tx)
    .await?;
    if done {
        crate::todos::complete_children_in(&mut tx, user_id, &[key.to_string()]).await?;
    }
    tx.commit().await
}

pub async fn clear_item_check(db: &PgPool, user_id: i64, key: &str, school_done: bool) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    crate::todos::lock(&mut tx, user_id).await?;
    sqlx::query("delete from item_checks where user_id=$1 and item_key=$2")
        .bind(user_id).bind(key).execute(&mut *tx).await?;
    if school_done {
        crate::todos::complete_children_in(&mut tx, user_id, &[key.to_string()]).await?;
    }
    tx.commit().await
}

pub async fn item_alerts_off(db: impl sqlx::Executor<'_, Database = sqlx::Postgres>, user_id: i64) -> sqlx::Result<HashSet<String>> {
    let rows: Vec<(String,)> = sqlx::query_as("select item_key from item_alerts_off where user_id = $1")
        .bind(user_id)
        .fetch_all(db)
        .await?;
    Ok(rows.into_iter().map(|(k,)| k).collect())
}

pub async fn set_item_alert(db: &PgPool, user_id: i64, key: &str, on: bool) -> sqlx::Result<()> {
    let q = if on {
        sqlx::query("delete from item_alerts_off where user_id = $1 and item_key = $2")
    } else {
        sqlx::query("insert into item_alerts_off (user_id, item_key) values ($1, $2) on conflict do nothing")
    };
    q.bind(user_id).bind(key).execute(db).await?;
    Ok(())
}


pub async fn item_alert_leads(db: impl sqlx::Executor<'_, Database = sqlx::Postgres>, user_id: i64) -> sqlx::Result<HashMap<String, Vec<i32>>> {
    let rows: Vec<(String, Vec<i32>)> = sqlx::query_as("select item_key, leads from item_alert_leads where user_id = $1")
        .bind(user_id).fetch_all(db).await?;
    Ok(rows.into_iter().collect())
}

pub async fn set_item_alert_leads(db: &PgPool, user_id: i64, key: &str, leads: Option<&[i32]>) -> sqlx::Result<()> {
    if let Some(leads) = leads {
        sqlx::query("insert into item_alert_leads (user_id, item_key, leads) values ($1, $2, $3) on conflict (user_id, item_key) do update set leads = excluded.leads")
            .bind(user_id).bind(key).bind(leads).execute(db).await?;
    } else {
        sqlx::query("delete from item_alert_leads where user_id = $1 and item_key = $2")
            .bind(user_id).bind(key).execute(db).await?;
    }
    Ok(())
}

pub async fn notices_seen(db: &PgPool, user_id: i64) -> sqlx::Result<Vec<String>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "select url from notices_seen where user_id = $1 and seen_at > now() - interval '30 days' order by seen_at desc limit 500",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(u,)| u).collect())
}

pub async fn add_notices_seen(db: &PgPool, user_id: i64, urls: &[String]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("insert into notices_seen (user_id, url) select $1, unnest($2::text[]) on conflict do nothing")
        .bind(user_id)
        .bind(urls)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from notices_seen where user_id = $1 and seen_at < now() - interval '30 days'")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await
}


#[derive(Debug, Clone, FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeatSession {
    pub id: i64,
    pub building: String,
    pub room_no: i32,
    pub room_name: String,
    pub seat_no: i32,
    pub period: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub started_at: DateTime<Utc>,
    pub start_source: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expires_at: DateTime<Utc>,
    pub extend_count: i32,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub ended_at: Option<DateTime<Utc>>,
}

const SESSION_COLUMNS: &str =
    "id, building, room_no, room_name, seat_no, period, started_at, start_source, expires_at, extend_count, ended_at";

pub async fn active_seat_session(db: impl sqlx::Executor<'_, Database = sqlx::Postgres>, user_id: i64) -> sqlx::Result<Option<SeatSession>> {
    sqlx::query_as(&format!("select {SESSION_COLUMNS} from seat_sessions where user_id = $1 and ended_at is null"))
        .bind(user_id)
        .fetch_optional(db)
        .await
}

pub struct NewSeatSession<'a> {
    pub building: &'a str,
    pub room_no: i32,
    pub room_name: &'a str,
    pub seat_no: i32,
    pub period: &'a str,
    pub started_at: DateTime<Utc>,
    pub start_source: &'a str,
    pub hours: i32,
}

pub async fn insert_seat_session(db: &PgPool, user_id: i64, s: NewSeatSession<'_>) -> sqlx::Result<SeatSession> {
    sqlx::query_as(&format!(
        "insert into seat_sessions
             (user_id, building, room_no, room_name, seat_no, period, started_at, start_source, expires_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $7 + make_interval(hours => $9))
         returning {SESSION_COLUMNS}"
    ))
    .bind(user_id)
    .bind(s.building)
    .bind(s.room_no)
    .bind(s.room_name)
    .bind(s.seat_no)
    .bind(s.period)
    .bind(s.started_at)
    .bind(s.start_source)
    .bind(s.hours)
    .fetch_one(db)
    .await
}

pub async fn extend_seat_session(db: &PgPool, user_id: i64, hours: i32) -> sqlx::Result<Option<SeatSession>> {
    sqlx::query_as(&format!(
        "update seat_sessions set expires_at = now() + make_interval(hours => $2), extend_count = extend_count + 1
         where user_id = $1 and ended_at is null
         returning {SESSION_COLUMNS}"
    ))
    .bind(user_id)
    .bind(hours)
    .fetch_optional(db)
    .await
}

pub async fn adjust_seat_session(
    db: &PgPool,
    user_id: i64,
    started_at: DateTime<Utc>,
    hours: i32,
    period: &str,
) -> sqlx::Result<Option<SeatSession>> {
    sqlx::query_as(&format!(
        "update seat_sessions
         set started_at = $2, start_source = 'adjusted', period = $4,
             expires_at = $2 + make_interval(hours => $3), extend_count = 0
         where user_id = $1 and ended_at is null
         returning {SESSION_COLUMNS}"
    ))
    .bind(user_id)
    .bind(started_at)
    .bind(hours)
    .bind(period)
    .fetch_optional(db)
    .await
}

pub async fn end_seat_session(db: &PgPool, user_id: i64, source: &str) -> sqlx::Result<Option<SeatSession>> {
    sqlx::query_as(&format!(
        "update seat_sessions set ended_at = now(), end_source = $2
         where user_id = $1 and ended_at is null
         returning {SESSION_COLUMNS}"
    ))
    .bind(user_id)
    .bind(source)
    .fetch_optional(db)
    .await
}


pub struct SeatStateRow {
    pub state: String,
    pub since: DateTime<Utc>,
    pub since_known: bool,
}

pub async fn seat_state(db: &PgPool, building: &str, room_no: i32, seat_no: i32) -> sqlx::Result<Option<SeatStateRow>> {
    let row: Option<(String, DateTime<Utc>, bool)> = sqlx::query_as(
        "select state, since, since_known from seat_state where building = $1 and room_no = $2 and seat_no = $3",
    )
    .bind(building)
    .bind(room_no)
    .bind(seat_no)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(state, since, since_known)| SeatStateRow { state, since, since_known }))
}

pub type SeatKey = (String, i32, i32);

pub async fn all_seat_states(db: &PgPool) -> sqlx::Result<HashMap<SeatKey, String>> {
    let rows: Vec<(String, i32, i32, String)> =
        sqlx::query_as("select building, room_no, seat_no, state from seat_state").fetch_all(db).await?;
    Ok(rows.into_iter().map(|(b, r, s, state)| ((b, r, s), state)).collect())
}

pub struct SeatChange {
    pub key: SeatKey,
    pub from: Option<String>,
    pub to: String,
    pub known: bool,
}

pub async fn record_seat_changes(db: &PgPool, changes: &[SeatChange], now: DateTime<Utc>) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    if !changes.is_empty() {
        let buildings: Vec<&str> = changes.iter().map(|c| c.key.0.as_str()).collect();
        let rooms: Vec<i32> = changes.iter().map(|c| c.key.1).collect();
        let seats: Vec<i32> = changes.iter().map(|c| c.key.2).collect();
        let states: Vec<&str> = changes.iter().map(|c| c.to.as_str()).collect();
        let known: Vec<bool> = changes.iter().map(|c| c.known).collect();
        sqlx::query(
            "insert into seat_state (building, room_no, seat_no, state, since, since_known)
             select b, r, s, st, $6, k from unnest($1::text[], $2::int[], $3::int[], $4::text[], $5::bool[]) as t(b, r, s, st, k)
             on conflict (building, room_no, seat_no) do update
                 set state = excluded.state, since = excluded.since, since_known = excluded.since_known",
        )
        .bind(&buildings)
        .bind(&rooms)
        .bind(&seats)
        .bind(&states)
        .bind(&known)
        .bind(now)
        .execute(&mut *tx)
        .await?;

        let events: Vec<&SeatChange> = changes.iter().filter(|c| c.from.is_some()).collect();
        if !events.is_empty() {
            let eb: Vec<&str> = events.iter().map(|c| c.key.0.as_str()).collect();
            let er: Vec<i32> = events.iter().map(|c| c.key.1).collect();
            let es: Vec<i32> = events.iter().map(|c| c.key.2).collect();
            let ef: Vec<&str> = events.iter().map(|c| c.from.as_deref().unwrap_or("")).collect();
            let et: Vec<&str> = events.iter().map(|c| c.to.as_str()).collect();
            sqlx::query(
                "insert into seat_events (building, room_no, seat_no, from_state, to_state, at)
                 select b, r, s, f, t, $6 from unnest($1::text[], $2::int[], $3::int[], $4::text[], $5::text[]) as x(b, r, s, f, t)",
            )
            .bind(&eb)
            .bind(&er)
            .bind(&es)
            .bind(&ef)
            .bind(&et)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }
    sqlx::query(
        "insert into seat_watch_meta (id, last_poll_at) values (1, $1)
         on conflict (id) do update set last_poll_at = excluded.last_poll_at",
    )
    .bind(now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

pub async fn last_poll(db: &PgPool) -> sqlx::Result<Option<DateTime<Utc>>> {
    sqlx::query_scalar("select last_poll_at from seat_watch_meta where id = 1").fetch_optional(db).await
}

#[derive(Debug, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct RecentSeat {
    pub seat_no: i32,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub at: DateTime<Utc>,
}

pub async fn recent_assigned(db: &PgPool, building: &str, room_no: i32, since: DateTime<Utc>) -> sqlx::Result<Vec<RecentSeat>> {
    sqlx::query_as(
        "select seat_no, max(at) as at from seat_events
         where building = $1 and room_no = $2 and to_state = 'used' and at >= $3
         group by seat_no order by max(at) desc limit 12",
    )
    .bind(building)
    .bind(room_no)
    .bind(since)
    .fetch_all(db)
    .await
}

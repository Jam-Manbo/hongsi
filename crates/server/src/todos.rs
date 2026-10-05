use chrono::{DateTime, Utc};
use hongsi_core::calendar::TodoParent;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection, PgPool};

#[derive(Debug, Clone, Serialize, FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    pub id: i64,
    pub course_id: Option<i64>,
    pub parent_key: Option<String>,
    pub title: String,
    pub note: String,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub due_at: Option<DateTime<Utc>>,
    pub all_day: bool,
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub done_at: Option<DateTime<Utc>>,
    pub notify: bool,
    pub alert_leads: Option<Vec<i32>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoInput {
    pub title: String,
    #[serde(default)]
    pub note: String,
    pub course_id: Option<i64>,
    pub parent_key: Option<String>,
    pub due_at: Option<i64>,
    #[serde(default = "yes")]
    pub all_day: bool,
    #[serde(default = "yes")]
    pub notify: bool,
    pub alert_leads: Option<Vec<i32>>,
}

fn yes() -> bool {
    true
}

const COLUMNS: &str = "id, course_id, parent_key, title, note, due_at, all_day, done_at, notify, alert_leads";

pub async fn list(db: impl sqlx::Executor<'_, Database = sqlx::Postgres>, user_id: i64) -> sqlx::Result<Vec<Todo>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from todos
         where user_id = $1
         order by due_at nulls last, id"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn get(db: &PgPool, user_id: i64, id: i64) -> sqlx::Result<Option<Todo>> {
    sqlx::query_as(&format!("select {COLUMNS} from todos where user_id=$1 and id=$2"))
        .bind(user_id).bind(id).fetch_optional(db).await
}

pub async fn create(db: &PgPool, user_id: i64, t: &TodoInput, due: DateTime<Utc>, school_done: bool) -> sqlx::Result<Todo> {
    let mut tx = db.begin().await?;
    lock(&mut tx, user_id).await?;
    let todo = sqlx::query_as(&format!(
        "insert into todos (user_id, course_id, parent_key, title, note, due_at, all_day, notify, alert_leads, done_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9,
            case when $3::text is not null and coalesce((select done from item_checks where user_id=$1 and item_key=$3), $10) then now() end)
         returning {COLUMNS}"
    ))
    .bind(user_id)
    .bind(t.course_id)
    .bind(&t.parent_key)
    .bind(t.title.trim())
    .bind(t.note.trim())
    .bind(due)
    .bind(t.all_day)
    .bind(t.notify)
    .bind(&t.alert_leads)
    .bind(school_done)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(todo)
}

pub async fn update(db: &PgPool, user_id: i64, id: i64, t: &TodoInput, due: DateTime<Utc>) -> sqlx::Result<Option<Todo>> {
    sqlx::query_as(&format!(
        "update todos set course_id = case when parent_key is null then $3 else course_id end,
                title = $5, note = $6, due_at = $7, all_day = $8, notify = $9, alert_leads = $10,
                updated_at = now()
         where user_id = $1 and id = $2 and parent_key is not distinct from $4::text returning {COLUMNS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(t.course_id)
    .bind(&t.parent_key)
    .bind(t.title.trim())
    .bind(t.note.trim())
    .bind(due)
    .bind(t.all_day)
    .bind(t.notify)
    .bind(&t.alert_leads)
    .fetch_optional(db)
    .await
}

pub async fn set_done(db: &PgPool, user_id: i64, id: i64, done: bool, school_done: bool) -> sqlx::Result<Option<Todo>> {
    let mut tx = db.begin().await?;
    lock(&mut tx, user_id).await?;
    let todo = sqlx::query_as(&format!(
        "update todos set done_at = case when $3 or (parent_key is not null and
            coalesce((select done from item_checks where user_id=$1 and item_key=todos.parent_key), $4))
            then coalesce(done_at, now()) else null end, updated_at = now()
         where user_id = $1 and id = $2 returning {COLUMNS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(done)
    .bind(school_done)
    .fetch_optional(&mut *tx)
    .await?;
    cancel_completed_alerts(&mut tx, user_id).await?;
    tx.commit().await?;
    Ok(todo)
}

pub async fn lock(db: &mut PgConnection, user_id: i64) -> sqlx::Result<()> {
    sqlx::query("select pg_advisory_xact_lock($1)").bind(-user_id).execute(db).await?;
    Ok(())
}

pub async fn complete_children_in(db: &mut PgConnection, user_id: i64, keys: &[String]) -> sqlx::Result<()> {
    sqlx::query("update todos set done_at=now(),updated_at=now() where user_id=$1 and parent_key=any($2) and done_at is null")
        .bind(user_id).bind(keys).execute(&mut *db).await?;
    cancel_completed_alerts(db, user_id).await
}

async fn cancel_completed_alerts(db: &mut PgConnection, user_id: i64) -> sqlx::Result<()> {
    sqlx::query("delete from notification_outbox o using notification_devices d, todos t
        where o.device_id=d.id and d.user_id=$1 and t.user_id=$1 and t.done_at is not null
        and o.sent_at is null and o.event_key like 'due:todo:%' and split_part(o.event_key,':',3)=t.id::text")
        .bind(user_id).execute(db).await?;
    Ok(())
}

pub async fn sync_parents(db: &PgPool, user_id: i64, parents: &[TodoParent]) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    lock(&mut tx, user_id).await?;
    sync_parents_in(&mut tx, user_id, parents).await?;
    tx.commit().await
}

pub async fn sync_parents_in(db: &mut PgConnection, user_id: i64, parents: &[TodoParent]) -> sqlx::Result<()> {
    let keys: Vec<_> = parents.iter().map(|p| p.key.clone()).collect();
    let courses: Vec<_> = parents.iter().map(|p| p.course_id).collect();
    sqlx::query("update todos t set course_id=p.course_id,updated_at=now()
        from unnest($2::text[],$3::bigint[]) as p(item_key,course_id)
        where t.user_id=$1 and t.parent_key=p.item_key and t.course_id is distinct from p.course_id")
        .bind(user_id).bind(&keys).bind(&courses).execute(&mut *db).await?;
    let checks = crate::db::item_checks(&mut *db, user_id).await?;
    let mut completed: Vec<_> = checks.iter().filter(|(_, done)| **done).map(|(key, _)| key.clone()).collect();
    completed.extend(parents.iter().filter(|p| checks.get(&p.key).copied().unwrap_or(p.finished)).map(|p| p.key.clone()));
    complete_children_in(db, user_id, &completed).await
}

pub async fn delete(db: &PgPool, user_id: i64, id: i64) -> sqlx::Result<bool> {
    let r = sqlx::query("delete from todos where user_id = $1 and id = $2").bind(user_id).bind(id).execute(db).await?;
    Ok(r.rows_affected() > 0)
}

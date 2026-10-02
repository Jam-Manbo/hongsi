use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

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

pub async fn list(db: &PgPool, user_id: i64) -> sqlx::Result<Vec<Todo>> {
    sqlx::query_as(&format!(
        "select {COLUMNS} from todos
         where user_id = $1 and (done_at is null or done_at > now() - interval '30 days')
         order by due_at nulls last, id"
    ))
    .bind(user_id)
    .fetch_all(db)
    .await
}

pub async fn create(db: &PgPool, user_id: i64, t: &TodoInput, due: Option<DateTime<Utc>>) -> sqlx::Result<Todo> {
    sqlx::query_as(&format!(
        "insert into todos (user_id, course_id, parent_key, title, note, due_at, all_day, notify, alert_leads)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9) returning {COLUMNS}"
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
    .fetch_one(db)
    .await
}

pub async fn update(db: &PgPool, user_id: i64, id: i64, t: &TodoInput, due: Option<DateTime<Utc>>) -> sqlx::Result<Option<Todo>> {
    sqlx::query_as(&format!(
        "update todos set course_id = $3, parent_key = $4, title = $5, note = $6, due_at = $7, all_day = $8, notify = $9, alert_leads = $10,
                updated_at = now()
         where user_id = $1 and id = $2 returning {COLUMNS}"
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

pub async fn set_done(db: &PgPool, user_id: i64, id: i64, done: bool) -> sqlx::Result<Option<Todo>> {
    sqlx::query_as(&format!(
        "update todos set done_at = case when $3 then coalesce(done_at, now()) else null end, updated_at = now()
         where user_id = $1 and id = $2 returning {COLUMNS}"
    ))
    .bind(user_id)
    .bind(id)
    .bind(done)
    .fetch_optional(db)
    .await
}

pub async fn delete(db: &PgPool, user_id: i64, id: i64) -> sqlx::Result<bool> {
    let r = sqlx::query("delete from todos where user_id = $1 and id = $2").bind(user_id).bind(id).execute(db).await?;
    Ok(r.rows_affected() > 0)
}

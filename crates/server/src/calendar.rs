use std::collections::HashMap;

pub use hongsi_core::calendar::{build, valid_alert_leads, CalendarData, CalendarState, SnapshotInfo, TodoParent};

use crate::db::{self, Snapshot};

pub async fn sync_parents(db: &sqlx::PgPool, session: &crate::sessions::UserSession, parents: Vec<TodoParent>) -> sqlx::Result<()> {
    crate::todos::sync_parents(db, session.user_id, &parents).await?;
    session.todo_parents.lock().expect("세션 잠금").extend(parents.into_iter().map(|p| (p.key.clone(), p)));
    Ok(())
}

pub async fn known_parents(db: &sqlx::PgPool, session: &crate::sessions::UserSession) -> sqlx::Result<Vec<TodoParent>> {
    let snapshot: Option<serde_json::Value> = sqlx::query_scalar("select snapshot from background_sessions where user_id=$1")
        .bind(session.user_id).fetch_optional(db).await?.flatten();
    let mut parents: HashMap<String, TodoParent> = snapshot.as_ref().and_then(|s| s["items"].as_array()).into_iter().flatten()
        .filter_map(|item| Some(TodoParent {
            key: item["key"].as_str()?.to_string(),
            course_id: item["courseId"].as_i64()?,
            finished: matches!(item["status"].as_str(), Some("submitted" | "done")),
        })).map(|p| (p.key.clone(), p)).collect();
    parents.extend(session.todo_parents.lock().expect("세션 잠금").clone());
    Ok(parents.into_values().collect())
}

pub async fn parent(db: &sqlx::PgPool, session: &crate::sessions::UserSession, key: &str) -> sqlx::Result<Option<TodoParent>> {
    Ok(known_parents(db, session).await?.into_iter().find(|p| p.key == key))
}

pub async fn load_state(db: &sqlx::PgPool, user_id: i64) -> sqlx::Result<CalendarState> {
    let (checks, alerts_off, alert_leads) = tokio::try_join!(
        db::item_checks(db, user_id),
        db::item_alerts_off(db, user_id),
        db::item_alert_leads(db, user_id),
    )?;
    Ok(CalendarState { checks, alerts_off, alert_leads })
}

pub fn snapshot_infos(snapshots: HashMap<i64, Snapshot>) -> HashMap<i64, SnapshotInfo> {
    snapshots
        .into_iter()
        .map(|(cmid, s)| {
            let info = SnapshotInfo {
                first_seen: s.first_seen.timestamp(),
                first_due: s.first_due.map(|d| d.timestamp()),
                first_intro_html: s.first_intro_html,
                change_count: s.change_count,
            };
            (cmid, info)
        })
        .collect()
}

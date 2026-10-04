use std::collections::HashMap;

pub use hongsi_core::calendar::{build, valid_alert_leads, CalendarData, CalendarState, SnapshotInfo};

use crate::db::{self, Snapshot};

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

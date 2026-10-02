use std::collections::HashMap;

pub use hongsi_core::calendar::{build, valid_alert_leads, CalendarData, SnapshotInfo};

use crate::db::Snapshot;

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

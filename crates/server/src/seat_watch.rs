use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use hongsi_core::models::Building;

use crate::db::{self, SeatChange, SeatKey};
use crate::state::{SeatSnapshot, Shared};

pub async fn run(state: Shared) {
    let secs = state.config.seat_poll_secs;
    if secs == 0 {
        tracing::info!("열람실 좌석 감시 꺼짐 (HONGSI_SEAT_POLL_SECS=0)");
        return;
    }
    let mut ticker = tokio::time::interval(Duration::from_secs(secs));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut previous: Option<HashMap<SeatKey, String>> = None;
    loop {
        ticker.tick().await;
        match hongsi_core::seats::fetch_all(&state.http).await {
            Ok(buildings) => {
                match record(&state, &buildings, previous.take(), secs).await {
                    Ok(current) => previous = Some(current),
                    Err(e) => tracing::warn!("좌석 기록 실패: {e}"),
                }
                *state.seats.write().await = Some(SeatSnapshot { fetched_at: Utc::now(), buildings });
            }
            Err(e) => tracing::warn!("열람실 조회 실패: {e}"),
        }
    }
}

async fn record(
    state: &Shared,
    buildings: &[Building],
    previous: Option<HashMap<SeatKey, String>>,
    poll_secs: u64,
) -> sqlx::Result<HashMap<SeatKey, String>> {
    let now = Utc::now();
    let (baseline, known) = match previous {
        Some(prev) => (prev, true),
        None => {
            let stored = db::all_seat_states(&state.db).await?;
            let recent = db::last_poll(&state.db)
                .await?
                .is_some_and(|t| (now - t).num_seconds() <= (poll_secs as i64) * 3);
            (stored, recent)
        }
    };
    let mut current = HashMap::new();
    let mut changes = Vec::new();
    for b in buildings {
        for room in &b.rooms {
            for cell in room.grid.iter().flatten().flatten() {
                let key: SeatKey = (b.id.clone(), room.no as i32, cell.no as i32);
                let to = cell.state.as_str().to_string();
                match baseline.get(&key) {
                    Some(from) if *from == to => {}
                    Some(from) => changes.push(SeatChange { key: key.clone(), from: Some(from.clone()), to: to.clone(), known }),
                    None => changes.push(SeatChange { key: key.clone(), from: None, to: to.clone(), known: false }),
                }
                current.insert(key, to);
            }
        }
    }
    db::record_seat_changes(&state.db, &changes, now).await?;
    Ok(current)
}

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::Utc;
use hongsi_core::calendar::{self, CalendarData, SnapshotInfo};
use hongsi_core::models::{Assignment, Course, SemesterDisplay, Vod};
use reqwest::Method;
use serde_json::{json, Value};

use crate::encode;
use crate::{cached, Direct, Reply, School, R};

impl Direct {
    pub(super) async fn calendar(&self, refresh: bool, display: SemesterDisplay) -> R<Value> {
        let s = self.current().await?;
        let mut slot = s.calendar.lock().await;
        if !refresh {
            if let Some(mut data) = cached(&slot, Duration::from_secs(180)).filter(|data| data.semester_display == display) {
                let parents: Vec<_> = data.items.iter().map(calendar::TodoParent::from).collect();
                let body = json!({ "courses": data.courses, "parents": parents });
                let reply = self.server(&Method::POST, "/api/calendar/state", Some(&body)).await;
                if reply.status == 200 {
                    if let Ok(state) = serde_json::from_value::<calendar::CalendarState>(reply.body) {
                        state.apply(&mut data.items);
                        if let Some((_, cached_data)) = slot.as_mut() {
                            *cached_data = data.clone();
                        }
                    }
                }
                return Ok(json!(data));
            }
        }
        let (current_term, courses) = s.session.courses_for(display).await?;
        let (assignments, vods) = tokio::join!(s.session.assignments(&courses), s.session.vods(&courses));
        let (assignments, vods) = (assignments?, vods?);
        let (snapshots, checks, alerts_off, alert_leads) = self.calendar_state(&courses, &assignments, &vods).await;
        let now = Utc::now().timestamp();
        let data = CalendarData {
            semester_display: display,
            current_term,
            items: calendar::build(&assignments, &vods, &snapshots, &checks, &alerts_off, &alert_leads, now),
            courses,
            fetched_at: now,
        };
        *slot = Some((Instant::now(), data.clone()));
        Ok(json!(data))
    }

    pub(super) async fn calendar_state(
        &self,
        courses: &[Course],
        assignments: &[Assignment],
        vods: &[Vod],
    ) -> (
        HashMap<i64, SnapshotInfo>,
        HashMap<String, bool>,
        HashSet<String>,
        HashMap<String, Vec<i32>>,
    ) {
        let body = json!({ "courses": courses, "assignments": assignments, "vods": vods });
        let r = self.server(&Method::POST, "/api/calendar/state", Some(&body)).await;
        if r.status != 200 {
            tracing::warn!(status = r.status, "서버 기록(완료 체크·첫 기록)을 받지 못해 학교 기준만 보여준다");
            return (HashMap::new(), HashMap::new(), HashSet::new(), HashMap::new());
        }
        let snapshots = serde_json::from_value(r.body["snapshots"].clone()).unwrap_or_default();
        let checks = serde_json::from_value(r.body["checks"].clone()).unwrap_or_default();
        let alerts_off = serde_json::from_value(r.body["alertsOff"].clone()).unwrap_or_default();
        let alert_leads = serde_json::from_value(r.body["alertLeads"].clone()).unwrap_or_default();
        (snapshots, checks, alerts_off, alert_leads)
    }

    pub(super) async fn patch_items(&self, s: &School, key: &str, f: impl Fn(&mut calendar::CalendarItem)) {
        if let Some((_, data)) = s.calendar.lock().await.as_mut() {
            data.items.iter_mut().filter(|i| i.key == key).for_each(f);
        }
    }

    pub(super) async fn set_done(&self, key: &str, body: &Value) -> Reply {
        let path = format!("/api/calendar/items/{}/done", encode(key));
        let reply = self.server(&Method::PUT, &path, Some(body)).await;
        if reply.status == 200 {
            if let Ok(s) = self.current().await {
                self.patch_items(&s, key, |item| {
                    item.done_override = body["done"].as_bool();
                    item.done = item.done_override.unwrap_or(item.status == "submitted" || item.status == "done");
                })
                .await;
            }
        }
        reply
    }

    pub(super) async fn set_alert(&self, key: &str, body: &Value) -> Reply {
        let path = format!("/api/calendar/items/{}/alert", encode(key));
        let reply = self.server(&Method::PUT, &path, Some(body)).await;
        if reply.status == 200 {
            if let (Ok(s), Some(on)) = (self.current().await, body["on"].as_bool()) {
                self.patch_items(&s, key, |item| item.alert = on).await;
            }
        }
        reply
    }

    pub(super) async fn set_alert_leads(&self, key: &str, body: &Value) -> Reply {
        let path = format!("/api/calendar/items/{}/alert-leads", encode(key));
        let reply = self.server(&Method::PUT, &path, Some(body)).await;
        if reply.status == 200 {
            if let Ok(s) = self.current().await {
                let leads: Option<Vec<i32>> = serde_json::from_value(reply.body["leads"].clone()).unwrap_or_default();
                self.patch_items(&s, key, |item| item.alert_leads = leads.clone()).await;
            }
        }
        reply
    }
}

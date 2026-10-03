use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::classroom::CN2;
use crate::models::{Assignment, Attachment, Course, SubmissionState, SubmitConfig, Vod, VodState};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub first_seen: i64,
    pub first_due: Option<i64>,
    pub first_intro_html: String,
    pub change_count: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarData {
    pub courses: Vec<Course>,
    pub items: Vec<CalendarItem>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchInfo {
    pub required: Option<String>,
    pub watched: Option<String>,
    pub mark: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarItem {
    pub key: String,
    pub kind: &'static str,
    pub course_id: i64,
    pub title: String,
    pub start: Option<i64>,
    pub due: Option<i64>,
    pub status: &'static str,
    pub done: bool,
    pub done_override: Option<bool>,
    pub alert: bool,
    pub alert_leads: Option<Vec<i32>>,
    pub url: String,
    pub intro_html: Option<String>,
    pub attachments: Vec<Attachment>,
    pub watch: Option<WatchInfo>,
    pub late_until: Option<i64>,
    pub modified: Option<i64>,
    pub first_seen: Option<i64>,
    pub first_due: Option<i64>,
    pub first_intro_html: Option<String>,
    pub change_count: i32,
    pub submit: Option<SubmitConfig>,
}

pub fn submission_status(s: SubmissionState) -> &'static str {
    match s {
        SubmissionState::Submitted => "submitted",
        SubmissionState::NotSubmitted => "not_submitted",
        SubmissionState::Unknown => "unknown",
    }
}

pub fn vod_status(s: Option<VodState>) -> &'static str {
    match s {
        Some(VodState::Done) => "done",
        Some(VodState::Partial) => "partial",
        Some(VodState::Missed) => "missed",
        Some(VodState::Todo) => "todo",
        Some(VodState::Upcoming) => "upcoming",
        None => "unknown",
    }
}

pub fn valid_alert_leads(leads: &[i32]) -> bool {
    leads.len() <= 5
        && leads.iter().enumerate().all(|(i, lead)| [1440, 180, 60, 10, 0].contains(lead) && !leads[..i].contains(lead))
}

pub fn build(
    assignments: &[Assignment],
    vods: &[Vod],
    snapshots: &HashMap<i64, SnapshotInfo>,
    checks: &HashMap<String, bool>,
    alerts_off: &HashSet<String>,
    alert_leads: &HashMap<String, Vec<i32>>,
    now: i64,
) -> Vec<CalendarItem> {
    let mut items = Vec::with_capacity(assignments.len() + vods.len());
    let mut seen = HashSet::new();
    for a in assignments {
        let key = format!("assign:{}", a.cmid);
        if !seen.insert(key.clone()) {
            continue;
        }
        let snap = snapshots.get(&a.cmid);
        let status = match a.submission {
            SubmissionState::Submitted => "submitted",
            _ if a.due.is_some_and(|d| d < now) => "overdue",
            SubmissionState::NotSubmitted => "not_submitted",
            SubmissionState::Unknown => "unknown",
        };
        let changed = snap.is_some_and(|s| s.change_count > 0 && s.first_intro_html != a.intro_html);
        items.push(CalendarItem {
            done: checks.get(&key).copied().unwrap_or(status == "submitted"),
            done_override: checks.get(&key).copied(),
            alert: !alerts_off.contains(&key),
            alert_leads: alert_leads.get(&key).cloned(),
            key,
            kind: "assignment",
            course_id: a.course_id,
            title: a.name.clone(),
            start: a.opens,
            due: a.due,
            status,
            url: format!("{CN2}/mod/assign/view.php?id={}", a.cmid),
            intro_html: Some(a.intro_html.clone()).filter(|h| !h.trim().is_empty()),
            attachments: a.attachments.clone(),
            watch: None,
            late_until: a.cutoff,
            modified: Some(a.modified),
            first_seen: snap.map(|s| s.first_seen),
            first_due: snap.and_then(|s| s.first_due),
            first_intro_html: snap.filter(|_| changed).map(|s| s.first_intro_html.clone()),
            change_count: snap.map_or(0, |s| s.change_count),
            submit: Some(a.config.clone()),
        });
    }
    for v in vods {
        let key = match v.cmid {
            Some(cmid) => format!("vod:{cmid}"),
            None => format!("vod:{}:{}", v.course_id, v.name),
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        let status = vod_status(Some(v.state));
        items.push(CalendarItem {
            done: checks.get(&key).copied().unwrap_or(status == "done"),
            done_override: checks.get(&key).copied(),
            alert: !alerts_off.contains(&key),
            alert_leads: alert_leads.get(&key).cloned(),
            key,
            kind: "vod",
            course_id: v.course_id,
            title: v.name.clone(),
            start: Some(v.start),
            due: Some(v.end),
            status,
            url: match v.cmid {
                Some(cmid) => format!("{CN2}/mod/vod/view.php?id={cmid}"),
                None => format!("{CN2}/course/view.php?id={}", v.course_id),
            },
            intro_html: None,
            attachments: vec![],
            watch: Some(WatchInfo { required: v.required.clone(), watched: v.watched.clone(), mark: v.mark.clone() }),
            late_until: v.late_until,
            modified: None,
            first_seen: None,
            first_due: None,
            first_intro_html: None,
            change_count: 0,
            submit: None,
        });
    }
    items.sort_by_key(|i| i.due.unwrap_or(i64::MAX));
    items
}

#[derive(Debug, PartialEq, Eq)]
pub enum SubmitRejection {
    BadRequest(String),
    Conflict(String),
    LateConfirmRequired,
}

pub fn check_submission(
    a: &Assignment,
    info: &crate::models::SubmissionInfo,
    now: i64,
    keep: &[String],
    new_files: &[(String, usize)],
    late_confirmed: bool,
    accept_statement: bool,
) -> Result<(), SubmitRejection> {
    use SubmitRejection::*;
    if !a.config.files {
        return Err(BadRequest("파일 제출 과제가 아니에요. 클래스룸에서 제출해 주세요".into()));
    }
    if a.cutoff.is_some_and(|c| now > c) {
        return Err(Conflict("제출 기한이 지나 더 이상 제출할 수 없어요".into()));
    }
    if info.locked || !info.can_edit {
        return Err(Conflict("지금은 파일을 제출하거나 수정할 수 없어요".into()));
    }
    if a.due.is_some_and(|d| now > d) && !late_confirmed {
        return Err(LateConfirmRequired);
    }
    if a.config.statement && !accept_statement {
        return Err(BadRequest("제출 서약에 동의해야 제출할 수 있어요".into()));
    }
    let total = keep.len() + new_files.len();
    if total == 0 {
        return Err(BadRequest("제출할 파일을 골라 주세요".into()));
    }
    if a.config.max_files > 0 && total > a.config.max_files as usize {
        return Err(BadRequest(format!("파일은 최대 {}개까지 제출할 수 있어요", a.config.max_files)));
    }
    if a.config.max_bytes > 0 {
        if let Some((name, _)) = new_files.iter().find(|(_, n)| *n as i64 > a.config.max_bytes) {
            return Err(BadRequest(format!("'{name}' 파일의 용량이 제한을 초과했어요")));
        }
    }
    if let Some(name) = keep.iter().find(|k| !info.files.iter().any(|f| &f.name == *k)) {
        return Err(BadRequest(format!("'{name}' 파일을 찾을 수 없어요")));
    }
    Ok(())
}

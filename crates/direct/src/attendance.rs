use std::time::{Duration, Instant};

use hongsi_core::models::ActiveLectures;
use reqwest::Method;
use serde_json::{json, Value};

use crate::{cached, Direct, Reply, School, R};

impl Direct {
    pub(super) async fn lectures(&self, s: &School) -> R<ActiveLectures> {
        let mut slot = s.lectures.lock().await;
        if let Some(data) = cached(&slot, Duration::from_secs(3)) {
            return Ok(data);
        }
        let data = s.session.active_lectures().await?;
        *slot = Some((Instant::now(), data.clone()));
        Ok(data)
    }

    pub(super) async fn submit_attendance(&self, b: &Value) -> R<Value> {
        let generation = self.generation();
        let s = self.current().await?;
        let key = b["lectureKey"].as_str().unwrap_or("");
        let code = b["code"].as_str().unwrap_or("").trim();
        let (lat, lon) = (
            b["latitude"].as_f64().unwrap_or(f64::NAN),
            b["longitude"].as_f64().unwrap_or(f64::NAN),
        );
        if code.is_empty() || code.len() > 12 || !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Reply::error(400, "bad_request", "인증번호를 확인해 주세요."));
        }
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return Err(Reply::error(400, "bad_request", "위치 정보가 올바르지 않아요."));
        }
        let submission = s.session.submit_attendance(key, code, lat, lon).await.map_err(|error| {
            let mut reply = Reply::from(error);
            if reply.status >= 500 {
                reply.body["error"]["message"] = json!("출석 결과를 확인하지 못했어요. 출석 상태를 확인해 주세요.");
            }
            reply
        })?;
        if self.generation() != generation {
            return Err(Reply::error(409, "account_changed", "이전 계정의 출석 요청이에요."));
        }
        *s.lectures.lock().await = None;
        s.courses.lock().await.clear();
        let synced = if let Some(receipt) = &submission.receipt {
            let body = json!({"receipt":receipt,"account":s.student_id});
            tokio::time::timeout(
                Duration::from_secs(3),
                self.server(&Method::PUT, "/api/attendance/receipts", Some(&body)),
            )
            .await
            .is_ok_and(|response| response.status == 200 && response.body["ok"] == true)
        } else {
            true
        };
        Ok(json!({ "message": submission.message, "receipt": submission.receipt, "synced": synced }))
    }

    pub(super) async fn attendance_course(&self, code: &str) -> R<Value> {
        let valid = code.len() <= 16
            && code.split_once('-').is_some_and(|(h, b)| {
                !h.is_empty() && !b.is_empty() && h.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit())
            });
        if !valid {
            return Err(Reply::error(400, "bad_request", "과목 코드가 올바르지 않아요."));
        }
        let s = self.current().await?;
        let mut map = s.courses.lock().await;
        if let Some((at, data)) = map.get(code) {
            if at.elapsed() < Duration::from_secs(60) {
                return Ok(json!(data));
            }
        }
        let data = s.session.attendance_course(code).await?;
        map.insert(code.to_string(), (Instant::now(), data.clone()));
        Ok(json!(data))
    }

    pub(super) async fn timetable(&self, refresh: bool) -> R<Value> {
        let s = self.current().await?;
        let mut slot = s.timetable.lock().await;
        let max_age = Duration::from_secs(if refresh { 60 } else { 6 * 3600 });
        if let Some(data) = cached(&slot, max_age) {
            return Ok(json!(data));
        }
        let data = s.session.timetable().await?;
        *slot = Some((Instant::now(), data.clone()));
        Ok(json!(data))
    }
}

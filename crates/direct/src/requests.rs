use std::collections::HashMap;

use hongsi_core::models::SemesterDisplay;
use reqwest::Method;
use serde_json::{json, Value};

use crate::{decode, ids};
use crate::{Direct, Reply, R};

impl Direct {
    pub async fn request(&self, method: &str, path: &str, body: Option<Value>) -> Reply {
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let reply = match self.route(method, path, body).await {
            Ok(reply) => reply,
            Err(reply) => reply,
        };
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        reply
    }

    pub(super) async fn route(&self, method: &str, path: &str, body: Option<Value>) -> R<Reply> {
        let (route, query) = path.split_once('?').unwrap_or((path, ""));
        let q: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
        let refresh = q.get("refresh").is_some_and(|v| v == "1");
        let method_upper = method.to_ascii_uppercase();
        let json_body = body.unwrap_or(Value::Null);

        match (method_upper.as_str(), route) {
            ("GET", "/api/me") => self.me().await.map(Reply::ok),
            ("GET", "/api/attendance/active") => {
                let s = self.current().await?;
                Ok(Reply::ok(json!(self.lectures(&s).await?)))
            }
            ("POST", "/api/attendance/submit") => self.submit_attendance(&json_body).await.map(Reply::ok),
            ("GET", "/api/attendance/status") => {
                let s = self.current().await?;
                Ok(Reply::ok(json!(s.session.attendance_status().await?)))
            }
            ("GET", "/api/attendance/course") => {
                let code = q.get("code").map(String::as_str).unwrap_or("");
                self.attendance_course(code).await.map(Reply::ok)
            }
            ("GET", "/api/timetable") => self.timetable(refresh).await.map(Reply::ok),
            ("GET", "/api/notifications") => self.notifications().await.map(Reply::ok),
            ("POST", "/api/background/session" | "/api/background/renew") => {
                if !self.base.starts_with("https://") && !self.base.starts_with("http://127.0.0.1:") {
                    return Err(Reply::error(
                        400,
                        "insecure_server",
                        "학교 세션을 보관하려면 HTTPS 서버가 필요해요.",
                    ));
                }
                if route == "/api/background/session" && json_body["consent"] != true {
                    return Err(Reply::error(400, "bad_request", "백그라운드 동기화 동의가 필요해요."));
                }
                let school = self.current().await?;
                let mut payload = json_body.clone();
                payload["cookies"] = json!(school.session.sso_cookies());
                Ok(self.server(&Method::POST, route, Some(&payload)).await)
            }
            ("GET", "/api/calendar") => {
                let display = match q.get("semester").map(String::as_str).unwrap_or("current") {
                    "current" => SemesterDisplay::Current,
                    "all" => SemesterDisplay::All,
                    _ => return Err(Reply::error(400, "bad_request", "학기 표시 설정을 확인해 주세요.")),
                };
                self.calendar(refresh, display).await.map(Reply::ok)
            }
            ("GET", "/api/meals") => Ok(self.meals().await),
            ("PUT", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/done") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/done"));
                Ok(self.set_done(&key, &json_body).await)
            }
            ("PUT", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/alert") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/alert"));
                Ok(self.set_alert(&key, &json_body).await)
            }
            ("PUT", r) if r.starts_with("/api/calendar/items/") && r.ends_with("/alert-leads") => {
                let key = decode(r.trim_start_matches("/api/calendar/items/").trim_end_matches("/alert-leads"));
                Ok(self.set_alert_leads(&key, &json_body).await)
            }
            ("GET", r) if r.starts_with("/api/assign/") && r.ends_with("/submission") => {
                let cmid: i64 = r
                    .trim_start_matches("/api/assign/")
                    .trim_end_matches("/submission")
                    .parse()
                    .map_err(|_| Reply::error(400, "bad_request", "잘못된 과제예요."))?;
                self.submission_view(cmid).await.map(Reply::ok)
            }
            ("GET", r) if r.starts_with("/api/modules/") => {
                let cmid = ids(r.trim_start_matches("/api/modules/"), 1)?[0];
                let s = self.current().await?;
                Ok(Reply::ok(json!(s.session.module_contents(cmid).await?)))
            }
            ("GET", r) if r.starts_with("/api/board/") => {
                let id = ids(r.trim_start_matches("/api/board/"), 2)?;
                let s = self.current().await?;
                Ok(Reply::ok(json!(s.session.board_article(id[0], id[1]).await?)))
            }
            _ => {
                let method =
                    Method::from_bytes(method_upper.as_bytes()).map_err(|_| Reply::error(400, "bad_request", "잘못된 요청 방식이에요."))?;
                let body = (!json_body.is_null()).then_some(&json_body);
                Ok(self.server(&method, path, body).await)
            }
        }
    }

    pub(super) async fn meals(&self) -> Reply {
        let reply = self.send_server(&Method::GET, "/api/meals", None).await;
        if reply.status == 200 {
            return reply;
        }
        match hongsi_core::food::fetch_week(&hongsi_core::public_client()).await {
            Ok(days) => Reply::ok(json!(days)),
            Err(e) => e.into(),
        }
    }
}

use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::Utc;
use hongsi_core::{api_error, CoreError};
use reqwest::Method;
use serde_json::{json, Value};

use crate::{Direct, Reply, School, ServerAuth, R};

impl Direct {
    pub fn server_reachable(&self) -> Option<bool> {
        match self.server_ok.load(Ordering::Relaxed) {
            1 => Some(true),
            2 => Some(false),
            _ => None,
        }
    }

    pub(super) fn token(&self) -> Option<String> {
        self.server_auth
            .lock()
            .ok()
            .and_then(|auth| auth.as_ref().map(|auth| auth.token.clone()))
    }

    pub(super) fn set_server_auth(&self, auth: Option<ServerAuth>) {
        if let Ok(mut value) = self.server_auth.lock() {
            *value = auth;
        }
    }

    pub(super) fn server_auth_valid(&self) -> bool {
        self.server_auth
            .lock()
            .ok()
            .is_some_and(|auth| auth.as_ref().is_some_and(|auth| auth.expires_at > Utc::now().timestamp()))
    }

    pub(super) async fn server_login(&self, school: &Arc<School>, previous: Option<&str>) -> R<()> {
        let _guard = self.server_login_lock.lock().await;
        if self.session_revoked() {
            return Err(Self::revoked_reply());
        }
        if !self
            .school
            .read()
            .await
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, school))
        {
            return Err(Reply::error(409, "account_changed", "로그인 상태가 변경됐어요."));
        }
        if self.token().as_deref() != previous && self.server_auth_valid() {
            return Ok(());
        }
        let token = school.session.moodle_token().await?;
        let r = self
            .send_server(
                &Method::POST,
                "/api/auth/device",
                Some(&json!({ "token": token, "remember": self.remember.load(Ordering::SeqCst) })),
            )
            .await;
        if r.status == 401 && r.code() == Some("login_rejected") {
            return Err(CoreError::ClassroomTokenExpired.into());
        }
        if r.status != 200 {
            return Err(r);
        }
        if self.session_revoked() {
            return Err(Self::revoked_reply());
        }
        let token = r.body["token"]
            .as_str()
            .filter(|token| token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| Reply::error(502, "server_error", "서버의 로그인 응답을 확인하지 못했어요."))?;
        if !self
            .school
            .read()
            .await
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, school))
        {
            let _ = self
                .http
                .post(format!("{}/api/auth/logout", self.base))
                .bearer_auth(token)
                .header("X-Client", "tauri")
                .send()
                .await;
            return Err(Reply::error(409, "account_changed", "로그인 상태가 변경됐어요."));
        }
        let expires_at = r.body["expiresAt"]
            .as_i64()
            .ok_or_else(|| Reply::error(502, "server_error", "서버의 로그인 응답을 확인하지 못했어요."))?;
        self.set_server_auth(Some(ServerAuth {
            token: token.to_string(),
            expires_at,
        }));
        Ok(())
    }

    pub(super) async fn send_server(&self, method: &Method, path: &str, body: Option<&Value>) -> Reply {
        let token = self.token();
        self.send_server_as(method, path, body, token.as_deref()).await
    }

    pub(super) async fn send_server_as(&self, method: &Method, path: &str, body: Option<&Value>, token: Option<&str>) -> Reply {
        let mut request = self
            .http
            .request(method.clone(), format!("{}{path}", self.base))
            .header("X-Client", "tauri");
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        match request.send().await {
            Ok(response) => {
                self.server_ok.store(1, Ordering::Relaxed);
                let status = response.status().as_u16();
                let format = api_error::response_format(
                    response
                        .headers()
                        .get(reqwest::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or(""),
                );
                let body = response.json::<Value>().await.unwrap_or(Value::Null);
                let reply = if status >= 400 {
                    let (code, message) = api_error::fallback(status);
                    let code = body["error"]["code"]
                        .as_str()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or(code);
                    let message = body["error"]["message"]
                        .as_str()
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or(message);
                    api_error::report(method.as_str(), path, status, code, format);
                    Reply::error(status, code, message)
                } else if !body.is_object() && !body.is_array() {
                    api_error::report(method.as_str(), path, status, "invalid_response", format);
                    Reply::error(502, "invalid_response", "서버 응답을 확인하지 못했어요.")
                } else {
                    Reply { status, body }
                };
                if reply.status == 401 && reply.code() == Some("session_revoked") {
                    let auth = self.server_auth.lock().ok();
                    if token.is_none() || auth.as_ref().and_then(|auth| auth.as_ref().map(|auth| auth.token.as_str())) != token {
                        return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
                    }
                    self.revoked.store(true, Ordering::SeqCst);
                }
                reply
            }
            Err(e) => {
                self.server_ok.store(2, Ordering::Relaxed);
                let (code, message) = if e.is_timeout() {
                    ("timeout", "응답이 늦어지고 있어요.")
                } else {
                    ("server_unreachable", "서버에 연결하지 못했어요.")
                };
                api_error::report(method.as_str(), path, 503, code, "none");
                Reply::error(503, code, message)
            }
        }
    }

    pub(super) async fn server(&self, method: &Method, path: &str, body: Option<&Value>) -> Reply {
        let school = match self.current().await {
            Ok(school) => school,
            Err(reply) => return reply,
        };
        let previous = self.token();
        if previous.is_none() {
            if let Err(reply) = self.server_login(&school, previous.as_deref()).await {
                return reply;
            }
        }
        let previous = {
            let current = self.school.read().await;
            if !current.as_ref().is_some_and(|value| Arc::ptr_eq(value, &school)) {
                return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
            }
            self.token()
        };
        let first = self.send_server_as(method, path, body, previous.as_deref()).await;
        if first.status != 401 || first.code() != Some("unauthorized") {
            return first;
        }
        match self.server_login(&school, previous.as_deref()).await {
            Ok(()) => {
                let current = self.school.read().await;
                if !current.as_ref().is_some_and(|value| Arc::ptr_eq(value, &school)) {
                    return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
                }
                let token = self.token();
                drop(current);
                self.send_server_as(method, path, body, token.as_deref()).await
            }
            Err(reply) => reply,
        }
    }
}

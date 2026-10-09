use std::sync::atomic::Ordering;
use std::sync::Arc;

use hongsi_core::SchoolSession;
use reqwest::Method;
use serde_json::{json, Value};

use crate::{AuthSnapshot, Direct, Reply, School, R};

impl Direct {
    pub async fn logged_in(&self) -> bool {
        self.school.read().await.is_some()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    pub fn session_revoked(&self) -> bool {
        self.revoked.load(Ordering::SeqCst)
    }

    pub(super) fn revoked_reply() -> Reply {
        Reply::error(401, "session_revoked", "다시 로그인해 주세요.")
    }

    pub(super) async fn current(&self) -> R<Arc<School>> {
        if self.session_revoked() {
            return Err(Self::revoked_reply());
        }
        self.school
            .read()
            .await
            .clone()
            .ok_or_else(|| Reply::error(401, "unauthorized", "로그인이 필요해요."))
    }

    pub async fn snapshot(&self) -> Option<AuthSnapshot> {
        if self.session_revoked() {
            return None;
        }
        let school = self.school.read().await;
        let school = school.as_ref()?;
        Some(AuthSnapshot {
            student_id: school.student_id.clone(),
            school: school.session.snapshot(),
            server_base: self.base.clone(),
            server: self.server_auth.lock().ok()?.clone(),
        })
    }

    pub async fn restore(&self, id: &str, snapshot: AuthSnapshot) -> bool {
        if snapshot.student_id != id.trim().to_uppercase() {
            return false;
        }
        let Ok(session) = SchoolSession::from_snapshot(snapshot.school) else {
            return false;
        };
        let Some(server) = snapshot.server.filter(|auth| {
            snapshot.server_base == self.base && auth.token.len() == 64 && auth.token.bytes().all(|b| b.is_ascii_hexdigit())
        }) else {
            return false;
        };
        let _guard = self.server_login_lock.lock().await;
        self.set_server_auth(Some(server));
        self.revoked.store(false, Ordering::SeqCst);
        self.remember.store(true, Ordering::SeqCst);
        *self.school.write().await = Some(Arc::new(School::new(session, snapshot.student_id)));
        self.generation.fetch_add(1, Ordering::SeqCst);
        true
    }

    pub async fn login(&self, id: &str, password: &str, remember: bool) -> Reply {
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let id = id.trim().to_uppercase();
        if id.is_empty() || password.is_empty() || id.len() > 32 {
            return Reply::error(400, "bad_request", "학번과 비밀번호를 입력해 주세요.");
        }
        let session = match SchoolSession::login(&id, password).await {
            Ok(s) => s,
            Err(e) => return e.into(),
        };
        let profile = session.profile().await.ok();
        let school = Arc::new(School::new(session, id));
        {
            let _guard = self.server_login_lock.lock().await;
            if self.session_revoked() {
                return Self::revoked_reply();
            }
            let same_account = self
                .school
                .read()
                .await
                .as_ref()
                .is_some_and(|current| current.student_id == school.student_id);
            if !same_account || self.remember.load(Ordering::SeqCst) != remember {
                let reply = self.revoke_server_login().await;
                if reply.status != 200 {
                    return reply;
                }
                self.set_server_auth(None);
            }
            self.remember.store(remember, Ordering::SeqCst);
            *self.school.write().await = Some(school.clone());
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
        if !self.server_auth_valid() {
            let previous = self.token();
            if let Err(e) = self.server_login(&school, previous.as_deref()).await {
                if e.code() == Some("session_revoked") {
                    return e;
                }
                tracing::warn!(status = e.status, "홍시 서버 기기 로그인 실패 — 학교 기능만 먼저 쓴다");
            }
        }
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        Reply::ok(json!({ "profile": {
                "name": profile.as_ref().map(|p| p.name.as_str()).unwrap_or(""),
                "hasPicture": profile.as_ref().is_some_and(|p| p.has_picture),
                "department": profile.and_then(|p| p.department),
                "studentId": school.student_id,
            } }))
    }

    pub async fn refresh_classroom(&self) -> Reply {
        let school = match self.current().await {
            Ok(school) => school,
            Err(reply) => return reply,
        };
        let session = match school.session.refresh_moodle().await {
            Ok(session) => session,
            Err(error) => return error.into(),
        };
        let _guard = self.server_login_lock.lock().await;
        if self.session_revoked() {
            return Self::revoked_reply();
        }
        let mut current = self.school.write().await;
        if !current.as_ref().is_some_and(|value| Arc::ptr_eq(value, &school)) {
            return Reply::error(409, "account_changed", "로그인 상태가 변경됐어요.");
        }
        *current = Some(Arc::new(School::new(session, school.student_id.clone())));
        self.generation.fetch_add(1, Ordering::SeqCst);
        Reply::ok(json!({ "ok": true }))
    }

    pub async fn revoke_login(&self) -> Reply {
        let _guard = self.server_login_lock.lock().await;
        self.revoke_server_login().await
    }

    pub(super) async fn revoke_server_login(&self) -> Reply {
        if self.token().is_some() {
            let reply = self.send_server(&Method::POST, "/api/auth/logout", None).await;
            if reply.code() == Some("session_revoked") {
                return reply;
            }
            if reply.status != 200 || reply.body["ok"] != true {
                return Reply::error(
                    503,
                    "logout_cleanup_failed",
                    "서버의 로그인 정보를 삭제하지 못했어요. 연결을 확인한 뒤 로그아웃을 다시 시도해 주세요.",
                );
            }
        }
        Reply::ok(json!({ "ok": true }))
    }

    pub async fn revoke_all_logins(&self) -> Reply {
        let reply = self.server(&Method::POST, "/api/auth/logout-all", None).await;
        if reply.code() == Some("session_revoked") {
            return reply;
        }
        if reply.status != 200 || reply.body["ok"] != true {
            return Reply::error(
                503,
                "logout_cleanup_failed",
                "모든 기기의 로그인 정보를 삭제하지 못했어요. 연결을 확인한 뒤 다시 시도해 주세요.",
            );
        }
        reply
    }

    pub async fn clear_login(&self) {
        let _guard = self.server_login_lock.lock().await;
        self.set_server_auth(None);
        self.revoked.store(false, Ordering::SeqCst);
        self.remember.store(false, Ordering::SeqCst);
        *self.school.write().await = None;
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.submissions.clear();
    }

    pub(super) async fn me(&self) -> R<Value> {
        let s = self.current().await?;
        let p = s.session.profile().await?;
        Ok(json!({
            "profile": { "name": p.name, "hasPicture": p.has_picture, "studentId": s.student_id, "department": p.department },
            "remembered": false,
        }))
    }
}

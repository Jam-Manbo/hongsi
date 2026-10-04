use std::sync::Arc;
use jni::{objects::{GlobalRef, JObject, JString, JValue}, sys::jstring, JNIEnv, JavaVM};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use crate::{call, credentials, ensure_login, shared_shell, Reply, SHARED_SHELL};

pub struct Vault { vm: JavaVM, object: GlobalRef }
impl Vault {
    fn invoke(&self, method: &str, secret: Option<&str>) -> Result<Option<String>, String> {
        let failure = match method {
            "save" => credentials::SAVE_ERROR,
            "clear" => credentials::DELETE_ERROR,
            _ => credentials::READ_ERROR,
        };
        let mut env = self.vm.attach_current_thread().map_err(|_| failure)?;
        let result = (|| -> jni::errors::Result<Option<String>> {
            if let Some(secret) = secret {
                let value = env.new_string(secret)?;
                env.call_method(self.object.as_obj(), method, "(Ljava/lang/String;)V", &[JValue::Object(value.as_ref())])?;
                Ok(None)
            } else if method == "load" {
                let value = env.call_method(self.object.as_obj(), method, "()Ljava/lang/String;", &[])?.l()?;
                if value.is_null() { Ok(None) } else { Ok(Some(env.get_string(&JString::from(value))?.into())) }
            } else {
                env.call_method(self.object.as_obj(), method, "()V", &[])?;
                Ok(None)
            }
        })();
        if result.is_err() { let _ = env.exception_clear(); }
        result.map_err(|_| failure.into())
    }
    pub fn load(&self) -> Result<Option<String>, String> { self.invoke("load", None) }
    pub fn save(&self, secret: &str) -> Result<(), String> { self.invoke("save", Some(secret)).map(|_| ()) }
    pub fn clear(&self) -> Result<(), String> { self.invoke("clear", None).map(|_| ()) }
}

#[derive(Deserialize)]
struct Request { method: String, path: String, body: Option<Value>, #[serde(default)] owner: String }
fn allowed(r: &Request) -> bool {
    match r.method.as_str() {
        "GET" => matches!(r.path.as_str(), "/api/preferences" | "/api/timetable" | "/api/timetable?refresh=1" | "/api/calendar" | "/api/calendar?refresh=1" | "/api/todos" | "/api/attendance/active" | "/api/attendance/receipts" | "/api/seats/session") || r.path.starts_with("/api/attendance/course?code="),
        "POST" => matches!(r.path.as_str(), "/api/attendance/submit" | "/api/seats/session/extend" | "/api/seats/session/end") && !r.owner.is_empty(),
        "PUT" => r.path == "/api/attendance/receipts" && !r.owner.is_empty(),
        _ => false,
    }
}
async fn request(r: Request) -> Value {
    let Some(shell) = SHARED_SHELL.get() else { return json!({"status":503}); };
    let _operations = shell.operations.read().await;
    if !allowed(&r) { return json!({"status":400}); }
    if let Err(reply) = ensure_login(shell).await { return json!({"status":reply.status,"body":reply.body}); }
    let account = shell.direct.snapshot().await.and_then(|s| serde_json::to_value(s).ok())
        .and_then(|s| s["student_id"].as_str().map(str::to_string)).unwrap_or_default();
    let owner = format!("{:x}", Sha256::digest(account.as_bytes()));
    if account.is_empty() || (!r.owner.is_empty() && owner != r.owner) {
        return json!({"status":409,"body":{"error":{"code":"account_changed","message":"위젯을 새로고침해 주세요."}}});
    }
    let mut body = r.body;
    if r.path == "/api/attendance/receipts" && r.method == "PUT" {
        if let Some(Value::Object(value)) = body.as_mut() { value.insert("account".into(), json!(account)); }
    }
    let reply = call(shell, || shell.direct.request(&r.method, &r.path, body.clone())).await;
    json!({"status":reply.status,"body":reply.body,"owner":owner})
}

#[no_mangle]
pub extern "system" fn Java_dev_kyuyoung_hongsi_widget_WidgetNative_initialize(mut env: JNIEnv, _class: JObject, vault: JObject) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> jni::errors::Result<()> {
        let vault = Vault { vm: env.get_java_vm()?, object: env.new_global_ref(vault)? };
        tauri::async_runtime::block_on(async { shared_shell(credentials::Store::Native(Arc::new(vault))); });
        Ok(())
    }));
    if !matches!(result, Ok(Ok(()))) { let _ = env.exception_clear(); let _ = env.throw_new("java/lang/IllegalStateException", "위젯을 준비하지 못했어요."); }
}

#[no_mangle]
pub extern "system" fn Java_dev_kyuyoung_hongsi_widget_WidgetNative_request(mut env: JNIEnv, _class: JObject, input: JString) -> jstring {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Option<Value> {
        let input: String = env.get_string(&input).ok()?.into();
        if input.len() > 65536 { return None; }
        let r: Request = serde_json::from_str(&input).ok()?;
        Some(tauri::async_runtime::block_on(async {
            tokio::time::timeout(std::time::Duration::from_secs(90), request(r)).await.unwrap_or_else(|_| {
                let reply = Reply::error(504, "timeout", "처리 결과를 확인해 주세요.");
                json!({"status":reply.status,"body":reply.body})
            })
        }))
    }));
    let value = result.ok().flatten().unwrap_or_else(|| json!({"status":503,"body":{"error":{"message":"정보를 불러오지 못했어요."}}}));
    let _ = env.exception_clear();
    env.new_string(value.to_string()).map(JString::into_raw).unwrap_or(std::ptr::null_mut())
}

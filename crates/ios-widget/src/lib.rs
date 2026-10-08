use std::{ffi::{c_char, CStr, CString}, sync::OnceLock, time::Duration};
use hongsi_direct::{AuthSnapshot, Direct, Reply};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Deserialize, Serialize)]
struct Saved { id: String, password: String, auth: AuthSnapshot }

#[derive(Deserialize)]
struct Request {
    method: String,
    path: String,
    body: Option<Value>,
    owner: String,
    credentials: Saved,
    #[serde(default, rename = "timeoutMs")]
    timeout_ms: Option<u64>,
}

fn allowed(method: &str, path: &str, owner: &str) -> bool {
    if owner.len() != 64 || !owner.bytes().all(|b| b.is_ascii_hexdigit()) { return false; }
    match method {
        "GET" => matches!(path, "/api/preferences" | "/api/calendar?refresh=1&semester=current" |
            "/api/calendar?refresh=1&semester=all" | "/api/todos" | "/api/attendance/active" |
            "/api/attendance/receipts" | "/api/seats/session") ||
            (path.starts_with("/api/attendance/course?code=") && path.len() < 256 && !path.contains('&')),
        "POST" => matches!(path, "/api/attendance/submit" | "/api/seats/session/extend"),
        "PUT" => path == "/api/attendance/receipts",
        _ => false,
    }
}

async fn perform(mut r: Request) -> Value {
    if !allowed(&r.method, &r.path, &r.owner) { return envelope(Reply::error(400, "bad_request", "허용되지 않은 위젯 요청이에요.")); }
    let account = r.credentials.id.trim().to_uppercase();
    if format!("{:x}", Sha256::digest(account.as_bytes())) != r.owner {
        return envelope(Reply::error(409, "account_changed", "위젯을 새로고침해 주세요."));
    }
    let auth = serde_json::to_value(&r.credentials.auth).unwrap_or_default();
    let base = auth["server_base"].as_str().unwrap_or_default();
    if !base.starts_with("https://") && !(cfg!(debug_assertions) && base.starts_with("http://")) {
        return envelope(Reply::error(400, "bad_request", "서버 설정을 확인해 주세요."));
    }
    let direct = Direct::new(base.to_string());
    if !direct.restore(&account, r.credentials.auth.clone()).await {
        return envelope(Reply::error(401, "unauthorized", "앱에서 다시 로그인해 주세요."));
    }
    if r.method == "PUT" {
        if let Some(Value::Object(body)) = r.body.as_mut() { body.insert("account".into(), json!(account)); }
    }
    let mut reply = direct.request(&r.method, &r.path, r.body.clone()).await;
    if reply.needs_login() && reply.code() != Some("session_revoked") {
        let mut recovery = if reply.code() == Some("classroom_token_expired") {
            direct.refresh_classroom().await
        } else { Reply::error(401, "session_expired", "다시 로그인해 주세요.") };
        if recovery.needs_login() && !direct.session_revoked() {
            recovery = direct.login(&account, &r.credentials.password, true).await;
        }
        reply = if recovery.status == 200 { direct.request(&r.method, &r.path, r.body).await } else { recovery };
    }
    let revoked = direct.session_revoked() || reply.code() == Some("session_revoked");
    let mut result = envelope(reply);
    result["revoked"] = json!(revoked);
    if !revoked {
        if let Some(auth) = direct.snapshot().await {
            r.credentials.auth = auth;
            result["credentials"] = serde_json::to_value(r.credentials).unwrap_or(Value::Null);
        }
    }
    result
}

fn envelope(reply: Reply) -> Value { json!({"status": reply.status, "body": reply.body}) }

fn request(input: &str) -> Value {
    let Ok(r) = serde_json::from_str::<Request>(input) else {
        return envelope(Reply::error(400, "bad_request", "위젯 요청을 읽지 못했어요."));
    };
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2).enable_all().build().expect("widget runtime"));
    let timeout = Duration::from_millis(r.timeout_ms.unwrap_or(20_000).clamp(1, 20_000));
    runtime.block_on(async {
        tokio::time::timeout(timeout, perform(r)).await.unwrap_or_else(|_| {
            envelope(Reply::error(504, "timeout", "처리 결과를 확인해 주세요."))
        })
    })
}

#[no_mangle]
pub unsafe extern "C" fn hongsi_widget_request(input: *const c_char) -> *mut c_char {
    let result = std::panic::catch_unwind(|| {
        if input.is_null() { return envelope(Reply::error(400, "bad_request", "잘못된 요청이에요.")); }
        let bytes = CStr::from_ptr(input).to_bytes();
        if bytes.len() > 262_144 { return envelope(Reply::error(413, "too_large", "요청이 너무 커요.")); }
        match std::str::from_utf8(bytes) {
            Ok(input) => request(input),
            Err(_) => envelope(Reply::error(400, "bad_request", "잘못된 요청이에요.")),
        }
    }).unwrap_or_else(|_| envelope(Reply::error(503, "unavailable", "위젯을 새로고침해 주세요.")));
    CString::new(result.to_string()).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn hongsi_widget_free(value: *mut c_char) {
    if !value.is_null() { drop(CString::from_raw(value)); }
}

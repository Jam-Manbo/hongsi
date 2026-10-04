use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

pub fn fallback(status: u16) -> (&'static str, &'static str) {
    match status {
        401 => ("http_error", "로그인이 필요해요."),
        403 => ("forbidden", "이 요청은 허용되지 않았어요."),
        404 => ("http_error", "요청한 정보를 찾지 못했어요."),
        405 => ("method_not_allowed", "이 요청은 허용되지 않았어요."),
        413 => ("too_large", "파일이나 요청의 용량이 너무 커요."),
        415 => ("invalid_format", "요청을 처리하지 못했어요."),
        429 => ("rate_limited", "잠시 후 다시 시도해 주세요."),
        500..=599 => ("server_error", "잠시 후 다시 시도해 주세요."),
        _ => ("http_error", "요청을 처리하지 못했어요."),
    }
}

pub fn response_format(content_type: &str) -> &'static str {
    let mime = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if mime.is_empty() { "none" }
    else if mime == "application/json" || mime.ends_with("+json") { "json" }
    else if mime == "text/html" { "html" }
    else if mime.starts_with("text/") { "text" }
    else { "other" }
}

pub fn report(method: &str, path: &str, status: u16, code: &str, format: &str) {
    static RECENT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let method = match method { "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" => method, _ => "OTHER" };
    let words = "api auth login logout logout-all device recover reconnect me avatar preferences push status background session renew health app-update app-releases files modules board calendar items verify state done alert alert-leads assign submission todos attendance receipts active submit course timetable notifications notices seen meals seats recent extend end";
    let path = path.split(['?', '#']).next().unwrap_or("").split('/').map(|part| {
        if part.is_empty() || words.split(' ').any(|word| word == part) { part } else { ":id" }
    }).collect::<Vec<_>>().join("/");
    let codes = "error http_error invalid_request invalid_response unauthorized session_revoked session_expired classroom_token_expired login_rejected login_rate_limited school_reauth_required school_retry_required school_unreachable school_changed school_error server_unreachable server_error api_not_found not_found bad_request conflict account_changed db_error offline timeout native invalid_format too_large forbidden rate_limited method_not_allowed late_confirm_required attendance_result_unknown";
    let code = if codes.split(' ').any(|item| item == code) { code } else { "other" };
    let format = match format { "json" | "html" | "text" | "none" | "native" => format, _ => "other" };
    let key = format!("{method}:{path}:{status}:{code}:{format}");
    let now = Instant::now();
    let Ok(mut recent) = RECENT.get_or_init(|| Mutex::new(HashMap::new())).lock() else { return; };
    recent.retain(|_, at| now.duration_since(*at) < Duration::from_secs(60));
    if recent.contains_key(&key) || recent.len() >= 128 { return; }
    recent.insert(key, now);
    drop(recent);
    tracing::warn!(method, path, status, code, response_format = format, "API 요청 실패");
}

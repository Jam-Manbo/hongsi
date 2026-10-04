use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

#[cfg(target_os = "android")]
mod widgets;

use hongsi_direct::{AuthSnapshot, Direct, Reply};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::async_runtime::Mutex;
use tauri::{AppHandle, Manager, State};

#[cfg(target_os = "android")]
mod android_external;
#[cfg(target_os = "android")]
mod android_update;
mod credentials;

#[tauri::command]
async fn sync_widgets(app: AppHandle, shell: State<'_, Arc<Shell>>, value: String) -> Result<(), String> {
    #[cfg(target_os = "android")]
    {
        let _operations = shell.operations.read().await;
        if !value.is_empty() {
            let owner = serde_json::from_str::<Value>(&value).ok().and_then(|v| v["owner"].as_str().map(str::to_string));
            let account = shell.direct.snapshot().await.and_then(|s| serde_json::to_value(s).ok()).and_then(|v| v["student_id"].as_str().map(str::to_string));
            if owner.is_none() || owner != account { return Err("로그인한 계정의 위젯만 갱신할 수 있어요.".into()); }
        }
        return android_external::open(&app, "syncWidget", &value).await;
    }
    #[cfg(not(target_os = "android"))]
    { let _ = (app, shell, value); Ok(()) }
}

#[tauri::command]
async fn set_widget_theme(app: AppHandle, value: String) -> Result<(), String> {
    if !matches!(value.as_str(), "system" | "light" | "dark") { return Err("테마 설정을 확인해 주세요.".into()); }
    #[cfg(target_os = "android")]
    return android_external::open(&app, "setWidgetTheme", &value).await;
    #[cfg(not(target_os = "android"))]
    { let _ = (app, value); Ok(()) }
}

#[tauri::command]
async fn widget_intent(app: AppHandle) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    return android_external::take_widget_intent(&app).await;
    #[cfg(not(target_os = "android"))]
    { let _ = app; Ok(Value::Null) }
}

#[tauri::command]
async fn app_update(
    app: AppHandle,
    action: String,
    version_code: Option<i64>,
) -> Result<Value, String> {
    #[cfg(target_os = "android")]
    return android_update::call(&app, &action, version_code).await;
    #[cfg(not(target_os = "android"))]
    {
        let _ = (app, action, version_code);
        Err("Android 앱에서 사용할 수 있어요.".into())
    }
}

struct Shell {
    direct: Direct,
    credentials: credentials::Store,
    login_lock: Mutex<()>,
    saved: Mutex<CredentialState>,
    operations: tokio::sync::RwLock<()>,
    revoked: AtomicBool,
}

static SHARED_SHELL: OnceLock<Arc<Shell>> = OnceLock::new();
fn shared_shell(credentials: credentials::Store) -> Arc<Shell> {
    SHARED_SHELL.get_or_init(|| Arc::new(Shell {
        direct: Direct::new(server_base()), credentials,
        login_lock: Mutex::new(()), saved: Mutex::new(CredentialState::default()),
        operations: tokio::sync::RwLock::new(()), revoked: AtomicBool::new(false),
    })).clone()
}

#[derive(Serialize)]
struct ApiResponse {
    status: u16,
    body: Value,
    server: Option<bool>,
}

fn respond(shell: &Shell, r: Reply) -> ApiResponse {
    ApiResponse {
        status: r.status,
        body: r.body,
        server: shell.direct.server_reachable(),
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct Saved {
    id: String,
    password: String,
    #[serde(default)]
    auth: Option<AuthSnapshot>,
}

#[derive(Default)]
struct CredentialState {
    loaded: bool,
    value: Option<Saved>,
}

async fn saved_login(shell: &Shell) -> Result<Option<Saved>, String> {
    let mut cached = shell.saved.lock().await;
    if !cached.loaded {
        cached.value = shell
            .credentials
            .load()
            .await?
            .map(|s| {
                serde_json::from_str(&s).map_err(|_| {
                    credentials::READ_ERROR.to_string()
                })
            })
            .transpose()?;
        cached.loaded = true;
    }
    Ok(cached.value.clone())
}

async fn save_credentials(shell: &Shell, saved: Option<Saved>) -> Result<(), String> {
    let mut cached = shell.saved.lock().await;
    if let Some(saved) = &saved {
        let encoded = serde_json::to_string(saved).map_err(|_| credentials::SAVE_ERROR.to_string())?;
        shell.credentials.save(&encoded).await?;
    } else {
        shell.credentials.clear().await?;
    }
    cached.value = saved;
    cached.loaded = true;
    Ok(())
}

async fn persist_auth_locked(shell: &Shell) -> Result<(), String> {
    if shell.revoked.load(Ordering::Acquire) || shell.direct.session_revoked() {
        return Ok(());
    }
    if let Some(mut saved) = saved_login(shell).await? {
        let auth = shell.direct.snapshot().await;
        if auth.is_some() && saved.auth != auth {
            saved.auth = auth;
            save_credentials(shell, Some(saved)).await?;
        }
    }
    Ok(())
}

async fn persist_auth(shell: &Shell) {
    if shell.direct.session_revoked() {
        clear_revoked_login(shell).await;
        return;
    }
    let _guard = shell.login_lock.lock().await;
    if persist_auth_locked(shell).await.is_err() {
        tracing::warn!("갱신한 자동 로그인 인증값을 기기 보안 저장소에 보관하지 못했어요");
    }
}

async fn clear_revoked_login(shell: &Shell) {
    shell.revoked.store(true, Ordering::Release);
    let _guard = shell.login_lock.lock().await;
    if save_credentials(shell, None).await.is_err() {
        tracing::warn!("철회된 로그인 정보를 기기 보안 저장소에서 삭제하지 못했어요");
    }
    shell.direct.clear_login().await;
}

fn message(r: &Reply) -> String {
    r.body["error"]["message"]
        .as_str()
        .unwrap_or("요청을 처리하지 못했어요")
        .to_string()
}

fn need_login() -> Reply {
    Reply::error(401, "unauthorized", "로그인이 필요해요")
}

async fn relogin(shell: &Shell, seen: u64) -> Result<(), Reply> {
    let _guard = shell.login_lock.lock().await;
    if shell.revoked.load(Ordering::Acquire) || shell.direct.session_revoked() {
        return Err(Reply::error(
            401,
            "session_revoked",
            "다시 로그인해 주세요.",
        ));
    }
    if shell.direct.generation() != seen && shell.direct.logged_in().await {
        return Ok(());
    }
    let Some(saved) = saved_login(shell)
        .await
        .map_err(|e| Reply::error(503, "credentials_unavailable", e))?
    else {
        return Err(need_login());
    };
    let reply = shell.direct.login(&saved.id, &saved.password, true).await;
    match reply.status {
        200 => {
            if persist_auth_locked(shell).await.is_err() {
                tracing::warn!("갱신한 자동 로그인 인증값을 기기 보안 저장소에 보관하지 못했어요");
            }
            Ok(())
        }
        _ if reply.code() == Some("login_rejected") => Err(Reply::error(
            401,
            "session_expired",
            "저장된 비밀번호로 로그인하지 못했어요.",
        )),
        _ if reply.code() == Some("session_revoked") => Err(reply),
        s if s >= 500 => Err(reply),
        _ => Err(need_login()),
    }
}

async fn ensure_login(shell: &Shell) -> Result<(), Reply> {
    if shell.revoked.load(Ordering::Acquire) || shell.direct.session_revoked() {
        clear_revoked_login(shell).await;
        return Err(Reply::error(
            401,
            "session_revoked",
            "다시 로그인해 주세요.",
        ));
    }
    if shell.direct.logged_in().await {
        return Ok(());
    }
    {
        let _guard = shell.login_lock.lock().await;
        if shell.revoked.load(Ordering::Acquire) || shell.direct.session_revoked() {
            return Err(Reply::error(
                401,
                "session_revoked",
                "다시 로그인해 주세요.",
            ));
        }
        if shell.direct.logged_in().await {
            return Ok(());
        }
        let saved = saved_login(shell)
            .await
            .map_err(|e| Reply::error(503, "credentials_unavailable", e))?;
        if let Some(saved) = saved {
            if let Some(auth) = saved.auth {
                if shell.direct.restore(&saved.id, auth).await {
                    return Ok(());
                }
            }
            save_credentials(shell, None)
                .await
                .map_err(|e| Reply::error(503, "credentials_unavailable", e))?;
        }
    }
    Err(need_login())
}

async fn refresh_classroom(shell: &Shell, seen: u64) -> Result<(), Reply> {
    let reply = {
        let _guard = shell.login_lock.lock().await;
        if shell.revoked.load(Ordering::Acquire) || shell.direct.session_revoked() {
            return Err(Reply::error(
                401,
                "session_revoked",
                "다시 로그인해 주세요.",
            ));
        }
        if shell.direct.generation() != seen && shell.direct.logged_in().await {
            return Ok(());
        }
        let reply = shell.direct.refresh_classroom().await;
        if reply.status == 200 && persist_auth_locked(shell).await.is_err() {
            tracing::warn!("갱신한 클래스룸 인증값을 기기 보안 저장소에 보관하지 못했어요");
        }
        reply
    };
    if reply.status == 200 {
        Ok(())
    } else if reply.needs_login() || reply.code() == Some("classroom_token_expired") {
        relogin(shell, seen).await
    } else {
        Err(reply)
    }
}

async fn call<F, Fut>(shell: &Shell, f: F) -> Reply
where
    F: Fn() -> Fut,
    Fut: Future<Output = Reply>,
{
    if let Err(reply) = ensure_login(shell).await {
        if reply.code() == Some("session_revoked") {
            clear_revoked_login(shell).await;
        }
        return reply;
    }
    let seen = shell.direct.generation();
    let mut reply = f().await;
    let recovery = if reply.code() == Some("classroom_token_expired") {
        Some(refresh_classroom(shell, seen).await)
    } else if reply.needs_login() && reply.code() != Some("session_revoked") {
        Some(relogin(shell, seen).await)
    } else {
        None
    };
    if let Some(recovery) = recovery {
        reply = match recovery {
            Ok(()) => f().await,
            Err(e) => e,
        };
    }
    if reply.code() == Some("session_revoked") || shell.direct.session_revoked() {
        clear_revoked_login(shell).await;
        return Reply::error(
            401,
            "session_revoked",
            "다시 로그인해 주세요.",
        );
    } else {
        persist_auth(shell).await;
    }
    reply
}

#[tauri::command]
async fn api(
    shell: State<'_, Arc<Shell>>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<ApiResponse, String> {
    if !path.starts_with("/api/") {
        return Err("잘못된 요청 경로예요".into());
    }

    if path == "/api/auth/logout" || path == "/api/auth/logout-all" {
        let _operations = shell.operations.write().await;
        let _guard = shell.login_lock.lock().await;
        if !shell.direct.logged_in().await {
            if let Some(saved) = saved_login(&shell).await? {
                if let Some(auth) = saved.auth {
                    shell.direct.restore(&saved.id, auth).await;
                }
            }
        }
        let reply = if path == "/api/auth/logout-all" {
            shell.direct.revoke_all_logins().await
        } else {
            shell.direct.revoke_login().await
        };
        if reply.status == 200 && reply.body["ok"] == true {
            save_credentials(&shell, None).await?;
            shell.direct.clear_login().await;
        } else if reply.code() == Some("session_revoked") {
            shell.revoked.store(true, Ordering::Release);
            save_credentials(&shell, None).await?;
            shell.direct.clear_login().await;
        }
        return Ok(respond(&shell, reply));
    }

    if path == "/api/auth/login" {
        let _operations = shell.operations.write().await;
        let body = body.unwrap_or(Value::Null);
        let id = body["id"].as_str().unwrap_or("").to_string();
        let password = body["password"].as_str().unwrap_or("").to_string();
        let remember = body["remember"].as_bool().unwrap_or(false);
        let _guard = shell.login_lock.lock().await;
        let reply = shell.direct.login(&id, &password, remember).await;
        if reply.status == 200 {
            if remember {
                save_credentials(
                    &shell,
                    Some(Saved {
                        id: id.trim().to_uppercase(),
                        password,
                        auth: shell.direct.snapshot().await,
                    }),
                )
                .await?;
            } else {
                save_credentials(&shell, None).await?;
            }
            shell.revoked.store(false, Ordering::Release);
        }
        return Ok(respond(&shell, reply));
    }

    let _operations = shell.operations.read().await;
    let reply = call(&shell, || {
        shell.direct.request(&method, &path, body.clone())
    })
    .await;
    Ok(respond(&shell, reply))
}

#[tauri::command]
async fn auto_login_enabled(shell: State<'_, Arc<Shell>>) -> Result<bool, String> {
    let _operations = shell.operations.read().await;
    Ok(!shell.revoked.load(Ordering::Acquire) && saved_login(&shell).await?.is_some())
}

#[tauri::command]
async fn avatar(shell: State<'_, Arc<Shell>>) -> Result<Option<String>, String> {
    use base64::Engine;
    let _operations = shell.operations.read().await;
    if ensure_login(&shell).await.is_err() {
        return Ok(None);
    }
    Ok(shell.direct.avatar().await.map(|(mime, bytes)| {
        format!(
            "data:{mime};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }))
}

#[derive(Deserialize)]
struct UploadFile {
    name: String,
    data: String,
}

#[tauri::command]
async fn submit_assignment(
    shell: State<'_, Arc<Shell>>,
    cmid: i64,
    keep: Vec<String>,
    files: Vec<UploadFile>,
    late_confirmed: bool,
    accept_statement: bool,
) -> Result<ApiResponse, String> {
    use base64::Engine;
    let _operations = shell.operations.read().await;
    let mut decoded = Vec::with_capacity(files.len());
    for f in files {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(f.data)
            .map_err(|_| format!("'{}' 파일을 읽지 못했어요", f.name))?;
        decoded.push((f.name, bytes));
    }
    if let Err(reply) = ensure_login(&shell).await {
        return Ok(respond(&shell, reply));
    }
    let reply = shell
        .direct
        .submit(cmid, keep, decoded, late_confirmed, accept_statement)
        .await;
    persist_auth(&shell).await;
    Ok(respond(&shell, reply))
}

fn download_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .download_dir()
        .map_err(|_| "다운로드 폴더를 찾지 못했어요.".to_string())?;
    let dir = base.join("홍시");
    std::fs::create_dir_all(&dir).map_err(|_| "폴더를 만들지 못했어요.".to_string())?;
    Ok(dir)
}

fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').to_string();
    if cleaned.is_empty() {
        "첨부파일".into()
    } else {
        cleaned
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (1..)
        .map(|n| dir.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .expect("빈 이름")
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum FileSource {
    Assign { cmid: i64, index: usize },
    Module { cmid: i64, index: usize },
    Board { cmid: i64, bwid: i64, index: usize },
}

async fn fetch_file(direct: &Direct, source: &FileSource) -> Result<(String, Vec<u8>), Reply> {
    match *source {
        FileSource::Assign { cmid, index } => direct.attachment(cmid, index).await,
        FileSource::Module { cmid, index } => direct.module_file(cmid, index).await,
        FileSource::Board { cmid, bwid, index } => direct.board_file(cmid, bwid, index).await,
    }
}

#[tauri::command]
async fn download(
    app: AppHandle,
    shell: State<'_, Arc<Shell>>,
    source: FileSource,
    name: String,
) -> Result<String, String> {
    let _operations = shell.operations.read().await;
    if let Err(reply) = ensure_login(&shell).await {
        return Err(message(&reply));
    }
    let seen = shell.direct.generation();
    let mut result = fetch_file(&shell.direct, &source).await;
    if let Err(reply) = &result {
        let recovery = if reply.code() == Some("classroom_token_expired") {
            Some(refresh_classroom(&shell, seen).await)
        } else if reply.needs_login() {
            Some(relogin(&shell, seen).await)
        } else {
            None
        };
        if let Some(recovery) = recovery {
            result = match recovery {
                Ok(()) => fetch_file(&shell.direct, &source).await,
                Err(e) => Err(e),
            };
        }
    }
    persist_auth(&shell).await;
    let (_, bytes) = result.map_err(|r| message(&r))?;
    let name = name.rsplit('/').next().unwrap_or(&name).to_string();
    let path = unique_path(&download_dir(&app)?, &safe_name(&name));
    std::fs::write(&path, &bytes).map_err(|_| "파일을 저장하지 못했어요.".to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

fn checked(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let dir = download_dir(app)?
        .canonicalize()
        .map_err(|_| "다운로드 폴더를 열지 못했어요.".to_string())?;
    let file = Path::new(path)
        .canonicalize()
        .map_err(|_| "파일이 없어요.".to_string())?;
    if !file.starts_with(&dir) {
        return Err("열 수 없는 경로예요.".into());
    }
    Ok(file)
}

#[cfg(not(target_os = "android"))]
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|_| "파일이나 링크를 열지 못했어요.".to_string())
}

#[cfg(not(target_os = "android"))]
fn open_with_os(target: &std::ffi::OsStr) -> Result<(), String> {
    if cfg!(target_os = "macos") {
        run("open", &[target])
    } else if cfg!(target_os = "windows") {
        run("explorer", &[target])
    } else {
        run("xdg-open", &[target])
    }
}

#[tauri::command]
async fn open_file(app: AppHandle, path: String) -> Result<(), String> {
    let file = checked(&app, &path)?;
    #[cfg(target_os = "android")]
    return android_external::open(&app, "openFile", &file.to_string_lossy()).await;
    #[cfg(not(target_os = "android"))]
    open_with_os(file.as_os_str())
}

#[tauri::command]
async fn reveal_file(app: AppHandle, path: String) -> Result<(), String> {
    let file = checked(&app, &path)?;
    #[cfg(target_os = "android")]
    return android_external::open(&app, "revealFile", &file.to_string_lossy()).await;
    #[cfg(not(target_os = "android"))]
    if cfg!(target_os = "macos") {
        run("open", &["-R".as_ref(), file.as_os_str()])
    } else if cfg!(target_os = "windows") {
        let arg = format!("/select,{}", file.display());
        run("explorer", &[arg.as_ref()])
    } else {
        run("xdg-open", &[file.parent().unwrap_or(&file).as_os_str()])
    }
}

#[tauri::command]
async fn open_url(app: AppHandle, shell: State<'_, Arc<Shell>>, url: String) -> Result<(), String> {
    let _operations = shell.operations.read().await;
    let allowed = (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() < 4096
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !allowed {
        return Err("열 수 없는 주소예요".into());
    }
    if url.starts_with("https://cn2.hongik.ac.kr/") {
        ensure_login(&shell).await.map_err(|r| message(&r))?;
    }
    let target = shell.direct.browser_url(&url).await;
    persist_auth(&shell).await;
    #[cfg(target_os = "android")]
    return android_external::open(&app, "openUrl", &target).await;
    #[cfg(not(target_os = "android"))]
    let _ = app;
    #[cfg(not(target_os = "android"))]
    open_with_os(std::ffi::OsStr::new(&target))
}

const DEV_SERVER: &str = "http://127.0.0.1:8787";

fn server_base() -> String {
    fn clean(url: &str) -> Option<String> {
        let url = url.trim().trim_end_matches('/');
        (!url.is_empty()).then(|| url.to_string())
    }
    let runtime = if cfg!(debug_assertions) {
        std::env::var("HONGSI_API_URL")
            .ok()
            .as_deref()
            .and_then(clean)
    } else {
        None
    };
    runtime
        .or_else(|| option_env!("HONGSI_API_URL").and_then(clean))
        .unwrap_or_else(|| {
            if cfg!(debug_assertions) {
                DEV_SERVER.into()
            } else {
                "https://hongsi.kyuyoung.dev".into()
            }
        })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run_app() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(android_external::init())
        .plugin(android_update::init());
    builder
        .plugin(tauri_plugin_notifications::init())
        .plugin(credentials::init())
        .setup(move |app| {
            app.manage(shared_shell(app.state::<credentials::Store>().inner().clone()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api,
            app_update,
            sync_widgets,
            set_widget_theme,
            widget_intent,
            auto_login_enabled,
            avatar,
            download,
            open_file,
            open_url,
            reveal_file,
            submit_assignment
        ])
        .run(tauri::generate_context!())
        .expect("앱을 실행하지 못했어요");
}

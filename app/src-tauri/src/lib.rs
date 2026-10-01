use std::future::Future;
use std::path::{Path, PathBuf};

use hongsi_direct::{Direct, Reply};
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

#[derive(Serialize, Deserialize)]
struct Saved {
    id: String,
    password: String,
}

async fn saved_login(shell: &Shell) -> Result<Option<Saved>, String> {
    shell
        .credentials
        .load()
        .await?
        .map(|s| {
            serde_json::from_str(&s)
                .map_err(|_| "자동 로그인 정보를 읽지 못했어요. 다시 로그인해 주세요.".into())
        })
        .transpose()
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
    if shell.direct.generation() != seen && shell.direct.logged_in().await {
        return Ok(());
    }
    let Some(saved) = saved_login(shell)
        .await
        .map_err(|e| Reply::error(503, "credentials_unavailable", e))?
    else {
        return Err(need_login());
    };
    let reply = shell.direct.login(&saved.id, &saved.password).await;
    match reply.status {
        200 => Ok(()),
        _ if reply.code() == Some("login_rejected") => {
            shell
                .credentials
                .clear()
                .await
                .map_err(|e| Reply::error(503, "credentials_unavailable", e))?;
            Err(Reply::error(
                401,
                "unauthorized",
                "저장된 비밀번호로 로그인하지 못했어요. 다시 로그인해 주세요",
            ))
        }
        s if s >= 500 => Err(reply),
        _ => Err(need_login()),
    }
}

async fn ensure_login(shell: &Shell) -> Result<(), Reply> {
    if shell.direct.logged_in().await {
        return Ok(());
    }
    relogin(shell, shell.direct.generation()).await
}

async fn call<F, Fut>(shell: &Shell, f: F) -> Reply
where
    F: Fn() -> Fut,
    Fut: Future<Output = Reply>,
{
    if let Err(reply) = ensure_login(shell).await {
        return reply;
    }
    let seen = shell.direct.generation();
    let reply = f().await;
    if reply.needs_login() {
        return match relogin(shell, seen).await {
            Ok(()) => f().await,
            Err(e) if e.status >= 500 => e,
            Err(_) => reply,
        };
    }
    reply
}

#[tauri::command]
async fn api(
    shell: State<'_, Shell>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<ApiResponse, String> {
    if !path.starts_with("/api/") {
        return Err("잘못된 요청 경로예요".into());
    }

    if path == "/api/auth/login" {
        let body = body.unwrap_or(Value::Null);
        let id = body["id"].as_str().unwrap_or("").to_string();
        let password = body["password"].as_str().unwrap_or("").to_string();
        let remember = body["remember"].as_bool().unwrap_or(false);
        let _guard = shell.login_lock.lock().await;
        let reply = shell.direct.login(&id, &password).await;
        if reply.status == 200 {
            if remember {
                let secret = serde_json::to_string(&Saved {
                    id: id.trim().to_uppercase(),
                    password,
                })
                .map_err(|e| e.to_string())?;
                shell.credentials.save(&secret).await?;
            } else {
                shell.credentials.clear().await?;
            }
        }
        return Ok(respond(&shell, reply));
    }

    if path == "/api/auth/logout" {
        shell.credentials.clear().await?;
        let reply = shell.direct.logout().await;
        return Ok(respond(&shell, reply));
    }

    let reply = call(&shell, || {
        shell.direct.request(&method, &path, body.clone())
    })
    .await;
    Ok(respond(&shell, reply))
}

#[tauri::command]
async fn auto_login_enabled(shell: State<'_, Shell>) -> Result<bool, String> {
    Ok(saved_login(&shell).await?.is_some())
}

#[tauri::command]
async fn avatar(shell: State<'_, Shell>) -> Result<Option<String>, String> {
    use base64::Engine;
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
    shell: State<'_, Shell>,
    cmid: i64,
    keep: Vec<String>,
    files: Vec<UploadFile>,
    late_confirmed: bool,
    accept_statement: bool,
) -> Result<ApiResponse, String> {
    use base64::Engine;
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
    Ok(respond(&shell, reply))
}

fn download_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .download_dir()
        .map_err(|_| "다운로드 폴더를 찾지 못했어요".to_string())?;
    let dir = base.join("홍시");
    std::fs::create_dir_all(&dir).map_err(|e| format!("폴더를 만들지 못했어요: {e}"))?;
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
    shell: State<'_, Shell>,
    source: FileSource,
    name: String,
) -> Result<String, String> {
    if let Err(reply) = ensure_login(&shell).await {
        return Err(message(&reply));
    }
    let seen = shell.direct.generation();
    let mut result = fetch_file(&shell.direct, &source).await;
    if matches!(&result, Err(r) if r.needs_login()) && relogin(&shell, seen).await.is_ok() {
        result = fetch_file(&shell.direct, &source).await;
    }
    let (_, bytes) = result.map_err(|r| message(&r))?;
    let name = name.rsplit('/').next().unwrap_or(&name).to_string();
    let path = unique_path(&download_dir(&app)?, &safe_name(&name));
    std::fs::write(&path, &bytes).map_err(|e| format!("파일을 저장하지 못했어요: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

fn checked(app: &AppHandle, path: &str) -> Result<PathBuf, String> {
    let dir = download_dir(app)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let file = Path::new(path)
        .canonicalize()
        .map_err(|_| "파일이 없어요. 옮겼거나 지웠을 수 있어요".to_string())?;
    if !file.starts_with(&dir) {
        return Err("열 수 없는 경로예요".into());
    }
    Ok(file)
}

#[cfg(not(target_os = "android"))]
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<(), String> {
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("열지 못했어요: {e}"))
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
async fn open_url(app: AppHandle, shell: State<'_, Shell>, url: String) -> Result<(), String> {
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
    let base = server_base();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "android")]
    let builder = builder
        .plugin(android_external::init())
        .plugin(android_update::init());
    builder
        .plugin(tauri_plugin_notifications::init())
        .plugin(credentials::init())
        .setup(move |app| {
            app.manage(Shell {
                direct: Direct::new(base),
                credentials: app.state::<credentials::Store>().inner().clone(),
                login_lock: Mutex::new(()),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api,
            app_update,
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

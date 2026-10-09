use hongsi_student_card::{CardResult, Context, Failure};
use std::sync::Arc;
use tauri::{
    plugin::{Builder, TauriPlugin},
    AppHandle, Manager, Wry,
};
use zeroize::Zeroizing;

use crate::{ensure_login, saved_login, Shell};

#[cfg(target_os = "android")]
struct Native(tauri::plugin::PluginHandle<Wry>);

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("student-card")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(Native(api.register_android_plugin(
                "dev.kyuyoung.hongsi",
                "StudentCardPlugin",
            )?));
            #[cfg(not(target_os = "android"))]
            let _ = (app, api);
            Ok(())
        })
        .on_event(|app, event| {
            #[cfg(mobile)]
            if let tauri::RunEvent::WindowEvent { event, label, .. } = event {
                match event {
                    tauri::WindowEvent::Suspended | tauri::WindowEvent::Destroyed => {
                        if let Some(shell) = app.try_state::<Arc<Shell>>() {
                            shell.student_cards.suspend();
                        }
                        if let Some(window) = app.get_webview_window(label) {
                            let _ = window.eval("window.dispatchEvent(new Event('hongsi-pause'))");
                        }
                    }
                    #[cfg(target_os = "android")]
                    tauri::WindowEvent::Resumed => {
                        if let Some(window) = app.get_webview_window(label) {
                            let _ = window.eval("window.dispatchEvent(new Event('hongsi-resume'))");
                        }
                    }
                    _ => {}
                }
            }
            #[cfg(not(mobile))]
            let _ = (app, event);
        })
        .build()
}

async fn native(app: &AppHandle, command: &str, value: &str) -> Result<serde_json::Value, Failure> {
    #[cfg(target_os = "android")]
    return app
        .state::<Native>()
        .0
        .run_mobile_plugin_async(command, serde_json::json!({"value": value}))
        .await
        .map_err(|_| Failure::unavailable());
    #[cfg(target_os = "ios")]
    return tauri_plugin_hongsi_ios::call(app, command, value)
        .await
        .map_err(|_| Failure::unavailable());
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let _ = (app, command, value);
        Err(Failure::unavailable())
    }
}

pub async fn request(app: &AppHandle, shell: &Shell, view_id: &str, fresh: bool) -> CardResult {
    let ticket = match shell.student_cards.begin(view_id, fresh) {
        Ok(ticket) => ticket,
        Err(error) => return CardResult::Failed(error),
    };
    let operation = async {
        let _operations = shell.operations.read().await;
        ensure_login(shell).await.map_err(|_| {
            Failure::authentication("홍시에 다시 로그인한 후 학생증 QR을 열어 주세요.")
        })?;
        let mut saved = saved_login(shell)
            .await
            .map_err(|_| Failure::credentials())?
            .ok_or_else(Failure::credentials)?;
        let password = Zeroizing::new(std::mem::take(&mut saved.password));
        let owner = shell
            .direct
            .snapshot()
            .await
            .and_then(|s| serde_json::to_value(s).ok())
            .and_then(|value| value["student_id"].as_str().map(str::to_owned));
        if owner.as_deref() != Some(saved.id.as_str()) || password.is_empty() {
            return Err(Failure::credentials());
        }
        let context: Context = serde_json::from_value(native(app, "studentCardContext", "").await?)
            .map_err(|_| Failure::unavailable())?;
        ticket.trace.stage("account", ticket.trace.elapsed_ms());
        shell
            .student_cards
            .issue(&ticket, &saved.id, &password, &context)
            .await
    };
    let result = tokio::select! {
        biased;
        _ = ticket.cancelled() => Err(Failure::cancelled()),
        result = operation => result,
    };
    match result {
        Ok((ready, locale_version)) => {
            if !locale_version.is_empty() {
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = native(&app, "saveStudentCardLocale", &locale_version).await;
                });
            }
            CardResult::Ready(ready)
        }
        Err(error) => {
            shell.student_cards.failed(&ticket);
            ticket.trace.stage("failed", ticket.trace.elapsed_ms());
            CardResult::Failed(error)
        }
    }
}

use tauri::{plugin::{Builder, TauriPlugin}, Runtime};
#[cfg(target_os = "ios")]
use tauri::{plugin::PluginHandle, AppHandle, Emitter, Manager};

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_hongsi_ios);

#[cfg(target_os = "ios")]
pub struct Ios<R: Runtime>(pub PluginHandle<R>);

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("hongsi-ios").setup(|app, api| {
        #[cfg(target_os = "ios")]
        app.manage(Ios(api.register_ios_plugin(init_plugin_hongsi_ios)?));
        #[cfg(not(target_os = "ios"))]
        let _ = (app, api);
        Ok(())
    }).on_event(|app, event| {
        #[cfg(target_os = "ios")]
        if let tauri::RunEvent::Opened { urls } = event {
            for url in urls.iter().filter(|url| url.scheme() == "hongsi-widget") {
                let app = app.clone(); let value = url.to_string();
                tauri::async_runtime::spawn(async move {
                    if call(&app, "acceptWidgetUrl", &value).await.is_ok() {
                        let _ = app.emit("hongsi-widget", ());
                    }
                });
            }
        }
        #[cfg(not(target_os = "ios"))]
        let _ = (app, event);
    }).build()
}

#[cfg(target_os = "ios")]
pub async fn call<R: Runtime>(app: &AppHandle<R>, command: &str, value: &str) -> Result<serde_json::Value, String> {
    app.state::<Ios<R>>().0.run_mobile_plugin_async(command, serde_json::json!({"value": value}))
        .await.map_err(|error| match error {
            tauri::plugin::mobile::PluginInvokeError::InvokeRejected(response) =>
                response.message.unwrap_or_else(|| "요청을 처리하지 못했어요.".into()),
            _ => "요청을 처리하지 못했어요.".into(),
        })
}

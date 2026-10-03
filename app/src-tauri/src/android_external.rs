use tauri::{plugin::{Builder, TauriPlugin}, AppHandle, Manager, Wry};

struct External(tauri::plugin::PluginHandle<Wry>);

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("external")
        .setup(|app, api| {
            app.manage(External(api.register_android_plugin("dev.kyuyoung.hongsi", "ExternalPlugin")?));
            Ok(())
        }).build()
}

pub async fn open(app: &AppHandle, command: &str, value: &str) -> Result<(), String> {
    app.state::<External>().0.run_mobile_plugin_async::<serde_json::Value>(command, serde_json::json!({ "value": value })).await
        .map(|_| ()).map_err(|e| e.to_string())
}

pub async fn take_widget_intent(app: &AppHandle) -> Result<serde_json::Value, String> {
    app.state::<External>().0.run_mobile_plugin_async("takeWidgetIntent", ()).await.map_err(|e| e.to_string())
}

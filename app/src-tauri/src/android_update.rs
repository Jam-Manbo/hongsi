use tauri::{plugin::{Builder, TauriPlugin}, AppHandle, Manager, Wry};

struct Updater(tauri::plugin::PluginHandle<Wry>);

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("hongsi-update").setup(|app, api| {
        app.manage(Updater(api.register_android_plugin("dev.kyuyoung.hongsi", "UpdatePlugin")?));
        Ok(())
    }).build()
}

pub async fn call(app: &AppHandle, action: &str, version_code: Option<i64>) -> Result<serde_json::Value, String> {
    if !["check", "status", "install", "permissions"].contains(&action) {
        return Err("지원하지 않는 업데이트 요청이에요.".into());
    }
    app.state::<Updater>().0.run_mobile_plugin_async(action, serde_json::json!({ "versionCode": version_code })).await
        .map_err(|e| e.to_string())
}

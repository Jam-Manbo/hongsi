use tauri::{plugin::{Builder, TauriPlugin}, AppHandle, Manager, Wry};

struct StudentCard(tauri::plugin::PluginHandle<Wry>);

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("student-card")
        .setup(|app, api| {
            app.manage(StudentCard(api.register_android_plugin("dev.kyuyoung.hongsi", "StudentCardPlugin")?));
            Ok(())
        }).build()
}

pub async fn request(app: &AppHandle, view_id: &str, id: &str, password: &str, fresh: bool, rust_preparation_ms: u64) -> Result<serde_json::Value, String> {
    app.state::<StudentCard>().0.run_mobile_plugin_async("request", serde_json::json!({
        "viewId": view_id, "owner": id, "password": password, "fresh": fresh,
        "rustPreparationMs": rust_preparation_ms,
    })).await.map_err(|_| "학생증 QR을 불러오지 못했어요.".to_string())
}

pub async fn close(app: &AppHandle, view_id: &str) -> Result<(), String> {
    app.state::<StudentCard>().0.run_mobile_plugin_async::<serde_json::Value>("close", serde_json::json!({ "viewId": view_id }))
        .await.map(|_| ()).map_err(|_| "학생증 QR을 닫지 못했어요.".to_string())
}

pub async fn displayed(app: &AppHandle, view_id: &str, timing_id: u64, elapsed_ms: u64, remaining_ms: u64) -> Result<(), String> {
    app.state::<StudentCard>().0.run_mobile_plugin_async::<serde_json::Value>("displayed", serde_json::json!({
        "viewId": view_id, "timingId": timing_id, "elapsedMs": elapsed_ms, "remainingMs": remaining_ms,
    })).await.map(|_| ()).map_err(|_| "측정값을 기록하지 못했어요.".to_string())
}

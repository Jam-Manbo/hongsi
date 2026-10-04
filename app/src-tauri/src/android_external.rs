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
        .map(|_| ()).map_err(|e| {
            if let tauri::plugin::mobile::PluginInvokeError::InvokeRejected(response) = e {
                if let Some(message) = response.message {
                    if matches!(message.as_str(),
                        "파일이 없어요." | "파일이나 링크를 열지 못했어요." |
                        "링크를 열 브라우저가 없어요." | "이 파일 형식을 열 수 있는 앱이 없어요." |
                        "파일 앱을 열지 못했어요." | "위젯 테마를 적용하지 못했어요." |
                        "위젯 정보를 저장하지 못했어요."
                    ) { return message; }
                }
            }
            match command {
                "setWidgetTheme" => "위젯 테마를 적용하지 못했어요.",
                "syncWidget" => "위젯 정보를 저장하지 못했어요.",
                "revealFile" => "폴더를 열지 못했어요.",
                _ => "파일이나 링크를 열지 못했어요.",
            }.to_string()
        })
}

pub async fn take_widget_intent(app: &AppHandle) -> Result<serde_json::Value, String> {
    app.state::<External>().0.run_mobile_plugin_async("takeWidgetIntent", ()).await.map_err(|_| "위젯을 새로고침해 주세요.".to_string())
}

use tauri::{plugin::{Builder, TauriPlugin}, Manager, Wry};

#[cfg(target_os = "android")]
#[derive(Clone)]
pub struct Store(tauri::plugin::PluginHandle<Wry>);
#[cfg(not(target_os = "android"))]
#[derive(Clone)]
pub struct Store;

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("credentials")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(Store(api.register_android_plugin("dev.kyuyoung.hongsi", "CredentialsPlugin")?));
            #[cfg(not(target_os = "android"))]
            { let _ = api; app.manage(Store); }
            Ok(())
        }).build()
}

#[cfg(target_os = "android")]
impl Store {
    pub async fn load(&self) -> Result<Option<String>, String> {
        #[derive(serde::Deserialize)]
        struct Loaded { secret: Option<String> }
        self.0.run_mobile_plugin_async::<Loaded>("load", ()).await
            .map(|r| r.secret).map_err(|_| "자동 로그인 정보를 읽지 못했어요. 기기 잠금을 해제한 뒤 다시 시도해 주세요.".into())
    }
    pub async fn save(&self, secret: &str) -> Result<(), String> {
        self.0.run_mobile_plugin_async::<serde_json::Value>("save", serde_json::json!({"secret": secret})).await
            .map(|_| ()).map_err(|_| "자동 로그인 정보를 보안 저장소에 저장하지 못했어요.".into())
    }
    pub async fn clear(&self) -> Result<(), String> {
        self.0.run_mobile_plugin_async::<serde_json::Value>("clear", ()).await
            .map(|_| ()).map_err(|_| "자동 로그인 정보를 삭제하지 못했어요. 다시 시도해 주세요.".into())
    }
}

#[cfg(not(target_os = "android"))]
impl Store {
    fn entry(&self) -> Result<keyring::Entry, String> {
        keyring::Entry::new("hongsi-app", "auto-login").map_err(|_| "보안 저장소를 열지 못했어요.".into())
    }
    pub async fn load(&self) -> Result<Option<String>, String> {
        match self.entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("자동 로그인 정보를 읽지 못했어요. 기기의 보안 저장소를 확인해 주세요.".into()),
        }
    }
    pub async fn save(&self, secret: &str) -> Result<(), String> {
        self.entry()?.set_password(secret).map_err(|_| "자동 로그인 정보를 저장하지 못했어요.".into())
    }
    pub async fn clear(&self) -> Result<(), String> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("자동 로그인 정보를 삭제하지 못했어요. 다시 시도해 주세요.".into()),
        }
    }
}

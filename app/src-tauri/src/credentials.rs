use tauri::{plugin::{Builder, TauriPlugin}, Manager, Wry};

pub const READ_ERROR: &str = "자동 로그인 정보를 읽지 못했어요.";
pub const SAVE_ERROR: &str = "자동 로그인 정보를 저장하지 못했어요.";
pub const DELETE_ERROR: &str = "자동 로그인 정보를 삭제하지 못했어요.";

#[cfg(target_os = "android")]
#[derive(Clone)]
pub enum Store {
    Plugin(tauri::plugin::PluginHandle<Wry>),
    Native(std::sync::Arc<crate::widgets::Vault>),
}
#[cfg(target_os = "ios")]
#[derive(Clone)]
pub struct Store(tauri::AppHandle);
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[derive(Clone)]
pub struct Store;

pub fn init() -> TauriPlugin<Wry> {
    Builder::new("credentials")
        .setup(|app, api| {
            #[cfg(target_os = "android")]
            app.manage(Store::Plugin(api.register_android_plugin("dev.kyuyoung.hongsi", "CredentialsPlugin")?));
            #[cfg(target_os = "ios")]
            { let _ = api; app.manage(Store(app.clone())); }
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            { let _ = api; app.manage(Store); }
            Ok(())
        }).build()
}

#[cfg(target_os = "android")]
impl Store {
    pub async fn load(&self) -> Result<Option<String>, String> {
        if let Self::Native(vault) = self { return vault.load(); }
        #[derive(serde::Deserialize)]
        struct Loaded { secret: Option<String> }
        let Self::Plugin(plugin) = self else { unreachable!() };
        plugin.run_mobile_plugin_async::<Loaded>("load", ()).await
            .map(|r| r.secret).map_err(|_| READ_ERROR.into())
    }
    pub async fn save(&self, secret: &str) -> Result<(), String> {
        if let Self::Native(vault) = self { return vault.save(secret); }
        let Self::Plugin(plugin) = self else { unreachable!() };
        plugin.run_mobile_plugin_async::<serde_json::Value>("save", serde_json::json!({"secret": secret})).await
            .map(|_| ()).map_err(|_| SAVE_ERROR.into())
    }
    pub async fn clear(&self) -> Result<(), String> {
        if let Self::Native(vault) = self { return vault.clear(); }
        let Self::Plugin(plugin) = self else { unreachable!() };
        plugin.run_mobile_plugin_async::<serde_json::Value>("clear", ()).await
            .map(|_| ()).map_err(|_| DELETE_ERROR.into())
    }
}

#[cfg(target_os = "ios")]
impl Store {
    pub async fn load(&self) -> Result<Option<String>, String> {
        tauri_plugin_hongsi_ios::call(&self.0, "loadCredentials", "").await
            .map(|v| v["secret"].as_str().map(str::to_string)).map_err(|_| READ_ERROR.into())
    }
    pub async fn save(&self, secret: &str) -> Result<(), String> {
        tauri_plugin_hongsi_ios::call(&self.0, "saveCredentials", secret).await.map(|_| ()).map_err(|_| SAVE_ERROR.into())
    }
    pub async fn clear(&self) -> Result<(), String> {
        tauri_plugin_hongsi_ios::call(&self.0, "clearCredentials", "").await.map(|_| ()).map_err(|_| DELETE_ERROR.into())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
impl Store {
    fn entry(&self) -> Result<keyring::Entry, keyring::Error> {
        keyring::Entry::new("hongsi-app", "auto-login")
    }
    pub async fn load(&self) -> Result<Option<String>, String> {
        match self.entry().and_then(|entry| entry.get_password()) {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(READ_ERROR.into()),
        }
    }
    pub async fn save(&self, secret: &str) -> Result<(), String> {
        self.entry().and_then(|entry| entry.set_password(secret)).map_err(|_| SAVE_ERROR.into())
    }
    pub async fn clear(&self) -> Result<(), String> {
        match self.entry().and_then(|entry| entry.delete_credential()) {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(DELETE_ERROR.into()),
        }
    }
}

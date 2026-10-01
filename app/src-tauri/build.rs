fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut api_url = std::env::var("HONGSI_API_URL").ok();
    for name in [".env.server", ".env"] {
        let path = root.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        if api_url.is_some() { continue; }
        match dotenvy::from_path_iter(&path) {
            Ok(entries) => {
                for entry in entries {
                    let (key, value) = entry.unwrap_or_else(|_| panic!("{name} 형식이 잘못되었습니다. KEY=VALUE 형식을 확인하세요."));
                    if key == "HONGSI_API_URL" && api_url.is_none() { api_url = Some(value); }
                }
            }
            Err(dotenvy::Error::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => panic!("{name} 파일을 읽을 수 없습니다."),
        }
    }
    if let Some(url) = &api_url {
        assert!(!url.contains(['\n', '\r']), "HONGSI_API_URL에는 줄바꿈을 넣을 수 없습니다.");
        println!("cargo:rustc-env=HONGSI_API_URL={url}");
    }
    println!("cargo:rerun-if-env-changed=HONGSI_API_URL");
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        let url = api_url.unwrap_or_default();
        let url = url.trim();
        assert!(url.is_empty() || url.starts_with("https://"), "release 빌드의 HONGSI_API_URL은 HTTPS여야 합니다.");
    }
    tauri_build::build()
}

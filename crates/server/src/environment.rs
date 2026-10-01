use std::{collections::BTreeMap, io, path::Path};

fn read_files(dir: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    for name in [".env.server", ".env"] {
        let path = dir.join(name);
        let entries = match dotenvy::from_path_iter(&path) {
            Ok(entries) => entries,
            Err(dotenvy::Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => return Err(io::Error::other(format!("{name} 파일을 읽을 수 없습니다. 파일 권한과 형식을 확인하세요."))),
        };
        for entry in entries {
            let (key, value) = entry.map_err(|_| io::Error::other(format!("{name} 형식이 잘못되었습니다. 따옴표와 KEY=VALUE 형식을 확인하세요.")))?;
            if key.starts_with("HONGSI_") || key == "DATABASE_URL" || key == "RUST_LOG" {
                values.entry(key).or_insert(value);
            }
        }
    }
    Ok(values)
}

pub fn load() -> io::Result<()> {
    let values = read_files(&std::env::current_dir()?)?;
    for (key, value) in values {
        if std::env::var_os(&key).is_none() {
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

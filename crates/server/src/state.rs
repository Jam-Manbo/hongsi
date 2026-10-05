use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use hongsi_core::models::{Building, MealDay};
use sqlx::PgPool;
use tokio::sync::{Mutex, RwLock};

use crate::sessions::Sessions;

pub struct Config {
    pub bind: String,
    pub database_url: String,
    pub static_dir: PathBuf,
    pub secret_file: PathBuf,
    pub seat_poll_secs: u64,
    pub cookie_secure: bool,
}

impl Config {
    pub fn from_env() -> Self {
        let env = |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.to_string());
        Self {
            bind: env("HONGSI_BIND", "127.0.0.1:8787"),
            database_url: env("DATABASE_URL", "postgres://127.0.0.1:5432/hongsi"),
            static_dir: PathBuf::from(env("HONGSI_STATIC_DIR", "app/dist")),
            secret_file: PathBuf::from(env("HONGSI_SECRET_FILE", ".server-secret")),
            seat_poll_secs: env("HONGSI_SEAT_POLL_SECS", "60").parse().unwrap_or(60),
            cookie_secure: env("HONGSI_COOKIE_SECURE", "0") == "1",
        }
    }
}

pub struct SeatSnapshot {
    pub fetched_at: DateTime<Utc>,
    pub buildings: Vec<Building>,
}

pub struct AppState {
    pub push: crate::push::Push,
    pub config: Config,
    pub db: PgPool,
    pub sessions: Sessions,
    pub login_attempts: crate::login_limit::LoginAttempts,
    pub http: reqwest::Client,
    pub pepper: Vec<u8>,
    pub meals: Mutex<Option<(Instant, Vec<MealDay>)>>,
    pub seats: RwLock<Option<SeatSnapshot>>,
    pub submissions: hongsi_core::submission::Jobs,
}

pub type Shared = Arc<AppState>;

pub fn load_secret(path: &Path) -> std::io::Result<Vec<u8>> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        std::io::Error::new(error.kind(), format!("서버 암호화 키 파일을 읽지 못했어요 ({}). HONGSI_SECRET_FILE 경로에 기존 키 파일이 필요합니다.", path.display()))
    })?;
    let bytes = hex::decode(text.trim()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "서버 암호화 키는 16진수 형식이어야 합니다. 기존 키 파일을 확인하세요.")
    })?;
    if bytes.len() < 32 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "서버 암호화 키는 32바이트 이상이어야 합니다. 기존 키 파일을 확인하세요."));
    }
    Ok(bytes)
}

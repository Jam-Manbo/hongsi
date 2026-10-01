mod api;
mod background;
mod push;
mod auth;
mod calendar;
mod db;
mod error;
mod environment;
mod seat_watch;
mod sessions;
mod login_limit;
mod state;
mod todos;
mod vault;
mod updates;

use std::sync::Arc;
use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::state::{load_secret, AppState, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    environment::load()?;
    run()
}

#[tokio::main]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "hongsi_server=info,hongsi_core=info".into()),
        )
        .init();

    let config = Config::from_env();
    let pepper = load_secret(&config.secret_file)?;
    let db = PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    sqlx::raw_sql(include_str!("../schema.sql")).execute(&db).await?;

    let push = push::Push::from_env()?;
    tracing::info!(web = push.ready("web"), android = push.ready("fcm"), ios = push.ready("apns"), "알림 발송 설정");
    let state = Arc::new(AppState {
        push,
        http: hongsi_core::public_client(),
        db,
        pepper,
        sessions: Default::default(),
        login_attempts: Default::default(),
        meals: Default::default(),
        seats: Default::default(),
        config,
    });

    tokio::spawn(seat_watch::run(state.clone()));
    tokio::spawn(background::run(state.clone()));
    {
        let st = state.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(600));
            loop {
                tick.tick().await;
                let removed = st.sessions.sweep(Duration::from_secs(6 * 3600));
                st.login_attempts.sweep();
                if removed > 0 {
                    tracing::info!(removed, "오래된 세션 정리");
                }
            }
        });
    }

    let static_dir = state.config.static_dir.clone();
    let app = api::router()
        .fallback_service(ServeDir::new(&static_dir).fallback(ServeFile::new(static_dir.join("index.html"))))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.config.bind).await?;
    tracing::info!("홍시 서버 실행: http://{}", state.config.bind);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

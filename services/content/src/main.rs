//! Kalks content service (:8105). See src/api.rs for the API contract.

use std::sync::Arc;

use content::api::{self, AppState};
use content::config::Config;
use content::{storage, store};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // repo-root .env.local in development; real env vars win in production
    let env_file = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    let cfg = Config::from_env()?;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into());
    if cfg.json_logs {
        tracing_subscriber::fmt().json().with_current_span(false).with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout())).init();
    }
    if let Err(e) = env_file
        && !e.not_found()
    {
        tracing::warn!(error = %e, ".env.local could not be fully parsed");
    }
    tracing::info!(?cfg, "content starting");
    if cfg.internal_token.is_empty() {
        tracing::warn!("CONTENT_INTERNAL_TOKEN is empty: any local process can call the content admin API (dev only)");
    }
    if cfg.storage == "disk" {
        tokio::fs::create_dir_all(&cfg.storage_dir).await?;
    }
    let storage = storage::from_config(&cfg)?;
    let pool = store::connect(&cfg.database_url).await?;

    let bind = cfg.bind.clone();
    let app = api::router(AppState { pool, cfg: Arc::new(cfg), storage });
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "http listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

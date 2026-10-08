//! Kalks Academy service (:8098). See src/api.rs for the API contract.

use std::path::Path;
use std::sync::Arc;

use academy::api::{self, AppState};
use academy::config::Config;
use academy::store;

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
    tracing::info!(?cfg, "academy starting");
    if cfg.internal_token.is_empty() {
        tracing::warn!("ACADEMY_INTERNAL_TOKEN is empty: any local process can call the academy (dev only)");
    }
    let pool = store::connect(&cfg.database_url).await?;
    store::seed_all(&pool, Path::new(&cfg.content_dir)).await?;

    let bind = cfg.bind.clone();
    // module switches: a read-only lazy connection to the gateway database (modules.rs)
    let gateway = (!cfg.gateway_database_url.is_empty())
        .then(|| sqlx::postgres::PgPoolOptions::new().max_connections(2).connect_lazy(&cfg.gateway_database_url))
        .and_then(|r| r.map_err(|e| tracing::warn!(error = %e, "gateway DB not usable: the Academy stays on for every broker")).ok());
    let app = api::router(AppState { pool, cfg: Arc::new(cfg), gateway });
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, "http listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

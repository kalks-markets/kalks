//! Kalks Circle service (127.0.0.1:8105). See docs/CIRCLE-API.md.

use circle::{api, config, db, state::AppState, workers};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // repo-root .env.local (+ .env.claude for the AI key) in development; real env vars win in production
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let env_file = dotenvy::from_path(format!("{root}/.env.local"));
    let _ = dotenvy::from_path(format!("{root}/.env.claude"));
    let cfg = config::Config::from_env()?;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into());
    if cfg.log_json {
        tracing_subscriber::fmt().json().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }
    if let Err(e) = env_file
        && !e.not_found()
    {
        tracing::warn!(error = %e, ".env.local could not be fully parsed");
    }
    tracing::info!(?cfg, "circle service starting");
    if cfg.internal_token.is_empty() {
        tracing::warn!("CIRCLE_INTERNAL_TOKEN is empty: any local process can call the circle service (dev only)");
    }
    if cfg.anthropic_key.is_empty() {
        tracing::warn!("ANTHROPIC_API_KEY is empty: text passes on the rules only; photos and videos follow CIRCLE_MODERATION_FALLBACK");
    }
    tokio::fs::create_dir_all(&cfg.work_dir).await?;
    if cfg.storage == "local" {
        tokio::fs::create_dir_all(&cfg.media_dir).await?;
    }
    let pool = db::connect(&cfg.database_url).await?;
    let st = AppState::new(pool, cfg);
    if !circle::video::available(&st.cfg).await {
        tracing::warn!("ffmpeg / ffprobe not found: video uploads fail with a clear reason, voice notes are kept as sent (install: apt install ffmpeg)");
    }
    if st.cfg.workers {
        workers::spawn(&st);
    }
    let listener = tokio::net::TcpListener::bind(&st.cfg.bind).await?;
    tracing::info!(bind = %st.cfg.bind, "http listening");
    axum::serve(listener, api::router(st))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

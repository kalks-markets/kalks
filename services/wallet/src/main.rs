//! Kalks wallet service (:8095). See services/wallet/README.md.

use std::collections::HashMap;
use std::sync::Arc;

use wallet::chain::{Chain, ChainId, bsc::Bsc, tron::Tron};
use wallet::config::Config;
use wallet::engine::HttpEngine;
use wallet::state::{AppState, Tenants};
use wallet::users::GatewayUsers;
use wallet::{api, db, ib, ledger, notifier, settings, watcher};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // repo-root .env.local (and .env.tron for the TronGrid key) in development; real env vars win in production
    let env_file = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.tron"));
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
    tracing::info!(?cfg, "wallet starting");
    if cfg.internal_token.is_empty() {
        tracing::warn!("WALLET_INTERNAL_TOKEN is empty: any local process can call the wallet (dev only)");
    }
    if cfg.trongrid_api_key.is_empty() {
        tracing::warn!("TRONGRID_API_KEY is empty: TronGrid rate limits apply");
    }

    let pool = db::connect(&cfg.database_url).await?;
    let problems = ledger::verify(&pool).await?;
    if !problems.is_empty() {
        for p in &problems {
            tracing::error!(problem = %p, "ledger invariant violated");
        }
        anyhow::bail!("{} ledger invariant violations — refusing to start", problems.len());
    }
    settings::seed(&pool, &cfg).await?;

    let mut chains: HashMap<ChainId, Arc<dyn Chain>> = HashMap::new();
    if !cfg.bsc_rpc_urls.is_empty() {
        chains.insert(ChainId::Bsc, Arc::new(Bsc::new(cfg.bsc_rpc_urls.clone())));
    }
    chains.insert(ChainId::Tron, Arc::new(Tron::new(cfg.trongrid_url.clone(), cfg.trongrid_api_key.clone())));

    let st = AppState {
        tenants: Tenants::load(&pool).await?,
        pool,
        engine: Arc::new(HttpEngine::new(cfg.trading_url.clone(), cfg.trading_token.clone())),
        users: Arc::new(GatewayUsers::new(cfg.gateway_url.clone(), cfg.gateway_token.clone())),
        chains: Arc::new(chains),
        cfg: Arc::new(cfg.clone()),
        transfer_lock: Default::default(),
    };
    if cfg.workers {
        watcher::spawn(st.clone());
        notifier::spawn(st.clone());
        ib::spawn(st.clone());
    }
    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!(bind = %cfg.bind, workers = cfg.workers, "http listening");
    axum::serve(listener, api::router(st))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

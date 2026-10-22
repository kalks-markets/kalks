//! Kalks market-data service.
//!
//! Ingests live prices from Infoway, builds broker-standard OHLC candles (M1…MN) from the raw feed,
//! backfills history, stores everything in PostgreSQL and serves candles + live quotes to the apps.
//! Account-group spread markups (Back Office) are applied only to outgoing quotes.

mod api;
mod backfill;
mod config;
mod db;
mod demand;
mod depth;
mod gate;
mod ingest;
mod instruments;
mod spreads;
mod state;
mod timeframes;

#[cfg(test)]
mod tests_candles;
#[cfg(test)]
mod tests_catalogue;

use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Several TLS backends are compiled into the workspace; pick one explicitly so TLS clients
    // (provider WebSocket, HTTPS history) never fail to find a crypto provider.
    let _ = rustls::crypto::ring::default_provider().install_default();
    // repo-root .env.local in development; real env vars win in production
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn,tokio_tungstenite=warn".into()))
        .init();

    let cfg = config::Config::from_env()?;
    let cat = instruments::Catalogue::load(&cfg.instruments_file, &cfg.holidays_dir)?;
    let pool = db::connect(&cfg.database_url).await?;
    db::sync_instruments(&pool, &cat).await?;
    let spreads = spreads::Spreads::load(&pool).await?;
    // on-demand provider subscriptions within the plan (demand.rs); the core instruments always stream by default
    let always = cfg.always_on.clone().unwrap_or_else(|| cat.core());
    let limits = demand::Limits {
        per_market: cfg.max_symbols_by_market.iter().cloned().collect(),
        default_per_market: cfg.max_symbols_per_market,
        total: cfg.max_symbols_total,
        grace: Duration::from_secs(cfg.idle_grace_secs),
        // relay mode: the upstream service applies the plan; locally every wanted symbol is forwarded
        streamable: cfg.upstream.is_empty().then(|| cfg.markets.iter().cloned().collect()),
    };
    let core = cat.core().len();
    let market = state::Market::new(cat, pool, spreads, cfg.store_ticks, demand::Demand::new(always, limits.clone()));
    market.restore().await?;
    tracing::info!(instruments = market.cat.list.len(), core, ?limits, "market-data starting (last prices restored)");

    // stop signal for the provider connections: they unsubscribe and close before the process exits
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let ingest_tasks = ingest::spawn_all(&cfg, market.clone(), stop_rx.clone());
    if cfg.upstream.is_empty() {
        backfill::spawn(&cfg, market.clone());
    } else {
        tracing::info!(upstream = %cfg.upstream, "relay mode: provider backfill and reconciliation are off");
    }

    // tick archive retention: hourly, in small batches (the archive is write-only; nothing reads old ticks)
    if cfg.store_ticks && cfg.ticks_retention_hours > 0 {
        let mk = market.clone();
        let hours = cfg.ticks_retention_hours;
        tokio::spawn(async move {
            let mut every = tokio::time::interval(Duration::from_secs(3600));
            loop {
                every.tick().await;
                // ticks are archived only for streamed symbols; the core always, the rest while streamed
                let symbols: Vec<String> = mk.cat.list.iter().filter(|i| i.is_core() || mk.is_streaming(&i.symbol)).map(|i| i.symbol.clone()).collect();
                let before = chrono::Utc::now() - chrono::Duration::hours(hours);
                match db::prune_ticks(&mk.pool, &symbols, before).await {
                    Ok(0) => {}
                    Ok(n) => tracing::info!(deleted = n, hours, "tick archive pruned"),
                    Err(e) => tracing::warn!(error = %e, "tick archive prune failed"),
                }
            }
        });
    }

    // persist forming bars + ticks every second; log throughput every minute
    {
        let mk = market.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            let mut n = 0u64;
            loop {
                tick.tick().await;
                if let Err(e) = mk.flush().await {
                    tracing::warn!(error = %e, "flush failed");
                }
                n += 1;
                if n % 60 == 0 {
                    let per_min = mk.reset_minute_counter();
                    tracing::info!(ticks_per_min = per_min, streams = ?mk.stats().connected_markets, "feed");
                }
            }
        });
    }

    let app = api::router(api::AppState { market: market.clone(), admin_token: cfg.admin_token.clone() });
    // TCP_NODELAY: every quote frame is tiny — never let Nagle hold one back waiting for an ACK
    let listener = axum::serve::ListenerExt::tap_io(tokio::net::TcpListener::bind(&cfg.bind).await?, |tcp| {
        let _ = tcp.set_nodelay(true);
    });
    tracing::info!(bind = %cfg.bind, "http listening");
    // SIGTERM (systemd stop / restart) or SIGINT: stop the provider connections first (unsubscribe + close
    // handshake, so the provider frees the slots at once and the next start is not refused), stop accepting HTTP,
    // give open client streams a moment, flush the last bars, exit
    let mut stopping = stop_rx.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = stopping.wait_for(|s| *s).await;
    });
    let server = tokio::spawn(async move { server.await });
    shutdown_signal().await;
    tracing::info!("stopping: closing provider connections");
    let _ = stop_tx.send(true);
    let closing = futures_util::future::join_all(ingest_tasks);
    if tokio::time::timeout(ingest::CLOSE_WAIT + Duration::from_secs(2), closing).await.is_err() {
        tracing::warn!("provider connections did not all close in time");
    }
    // browser / engine streams stay open until they go: they reconnect to the next process anyway
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
    if let Err(e) = market.flush().await {
        tracing::warn!(error = %e, "final flush failed");
    }
    tracing::info!("market-data stopped");
    Ok(())
}

/// SIGTERM (systemd) or SIGINT (Ctrl-C).
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

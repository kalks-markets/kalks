//! Kalks trading engine service (:8090). See services/trading/README.md.

use chrono::{Duration, NaiveDate, Utc};
use serde_json::Value;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};

use trading::api::{self, AppState, LoginAlloc};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::Ids;
use trading::feed::{self, QuoteBook};
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::specs::{Specs, next_rollover, server_date, server_midnight};
use trading::persist;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // several TLS backends are compiled in: pick one so HTTPS clients (corporate-actions import) find a provider
    let _ = rustls::crypto::ring::default_provider().install_default();
    // repo-root .env.local in development; real env vars win in production
    let env_file = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    let cfg = Config::from_env()?;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn,tokio_tungstenite=warn,tungstenite=warn".into());
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
    tracing::info!(?cfg, "trading engine starting");
    if cfg.internal_token.is_empty() {
        tracing::warn!("TRADING_INTERNAL_TOKEN is empty: any local process can call the engine (dev only)");
    }

    let pool = persist::connect(&cfg.database_url).await?;
    // instrument catalogue + the Back Office template overrides and live-trading switch (specs.rs, catalogue.rs)
    let specs = Specs::load(&cfg.instruments_file, &cfg.specs_file)?;
    let overrides = trading::catalogue::load_overrides(&pool).await?;
    let specs = Arc::new(specs.with_overrides(overrides)?);
    {
        let core = specs.all().filter(|s| s.core).count();
        let live = specs.all().filter(|s| !s.core && s.live).count();
        tracing::info!(instruments = specs.len(), core, catalogue = specs.len() - core, catalogue_live = live, "contract specs loaded");
    }
    let registry = Registry::default();
    for t in persist::load_registry(&pool).await? {
        tracing::info!(tenant = t.tenant_id, slug = %t.slug, groups = t.groups.len(), "tenant loaded");
        registry.put(t);
    }

    // recovery: rebuild every account from its event stream, then cross-check the ledger projection
    let started = std::time::Instant::now();
    let states = persist::replay_all(&pool).await?;
    let events: usize = states.values().map(|s| s.version as usize).sum();
    let mismatches = persist::verify_balances(&pool, &states).await?;
    if !mismatches.is_empty() {
        for m in &mismatches {
            tracing::error!(mismatch = %m, "ledger / replay mismatch");
        }
        anyhow::bail!("{} ledger mismatches after replay — refusing to start", mismatches.len());
    }
    tracing::info!(accounts = states.len(), events, ms = started.elapsed().as_millis() as u64, "state rebuilt from events; ledger verified");

    let (ticket, deal, txn, live, demo) = persist::max_ids(&pool).await?;
    let quotes = Arc::new(QuoteBook::default());
    // Kalks FX Options: snapshot from the options service, raw spots from market-data (src/options)
    let options = Arc::new(trading::options::OptionsCtx::new(&cfg.options_url, &cfg.options_token, quotes.clone()));
    if options.configured() {
        let _ = options.load_stored(&pool).await;
    }
    let shared = Arc::new(Shared {
        pool: pool.clone(),
        registry: registry.clone(),
        specs: specs.clone().into(),
        held: Default::default(),
        quotes: quotes.clone(),
        ids: Arc::new(Ids::new(ticket, deal, txn)),
        index: Arc::new(RwLock::new(Index::default())),
        streams: Streams::default(),
        stats: Arc::new(Stats::default()),
        lp: Arc::new(NullLp),
        max_quote_age_ms: cfg.max_quote_age_ms,
        restrictions: Default::default(),
        options: options.clone(),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
        risk: Default::default(),
    });
    let hub = Hub::start(shared, cfg.shards, states);
    feed::spawn(hub.clone(), cfg.market_data_ws.clone(), specs.symbols());
    if options.configured() {
        feed::spawn_raw(hub.clone(), cfg.market_data_ws.clone(), specs.symbols());
        options.spawn_poller(pool.clone());
    }

    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let wallet = trading::social::wallet::WalletClient::new(&cfg.wallet_url, &cfg.wallet_token);
    let social = trading::social::Social::new(pool.clone(), hub.clone(), wallet, logins.clone()).await?;
    let _ = social.ib.set(trading::social::wallet::WalletClient::new(&cfg.ib_url, &cfg.ib_token));
    social.start();
    // client notifications (bell + email) through support POST /v1/notify (src/notify.rs; no-op without SUPPORT_URL)
    trading::notify::spawn(pool.clone());
    let st = AppState {
        hub: hub.clone(),
        pool: pool.clone(),
        keys: Keys::new(&cfg.session_secret),
        cfg: Arc::new(cfg.clone()),
        limiter: Limiter::default(),
        tickets: StreamTickets::default(),
        logins,
        open_lock: Arc::new(tokio::sync::Mutex::new(())),
        social,
        presence: Arc::new(trading::controls::Presence::default()),
        gateway: Arc::new(trading::controls::Gateway::new(&cfg.gateway_url, &cfg.gateway_token)),
    };
    // client controls: restrictions cache from the gateway, Kalks Trader presence reports (controls.rs)
    trading::controls::spawn(hub.clone(), pool.clone(), st.gateway.clone(), st.presence.clone());

    // Kalks FX Options order book (src/book): venues, then crash recovery — load every stored book (ephemeral MM
    // quotes are gone), rebuild the shards' reservations, reconcile book and account positions (a mismatch puts
    // that underlying in cancel-only; CFD trading stays up), re-dispatch the outbox, resubmit fired stops
    if cfg.options_mm_user > 0 {
        hub.shared.books.lp_users.write().unwrap().insert(cfg.options_mm_user);
    }
    match trading::book::recover(&hub).await {
        Ok(r) => {
            for m in &r.mismatches {
                tracing::error!(mismatch = %m, "options book reconcile mismatch: cancel-only");
            }
        }
        Err(e) => tracing::error!(error = %e, "options order book recovery failed; books load on first use"),
    }
    let _ = trading::auth::dummy_hash();

    if cfg.rollover_enabled {
        tokio::spawn(rollovers(hub.clone(), pool.clone(), registry.clone()));
        // stock splits and dividends: apply approved actions at their ex-date, the daily EODHD import, the Infoway
        // cross-check (src/corporate)
        trading::corporate::spawn(hub.clone(), pool.clone(), st.cfg.clone());
        // options order book housekeeping: GTD expiry, deadman switches, expiry cut-off, session-open band check,
        // the throttled market-data feed
        trading::book::spawn_scheduler(hub.clone());
        // the liquidator of order-book positions at stop-out (docs §8) and the Kalks market maker (docs §4): it
        // quotes only where a tenant's order book is enabled
        trading::book::liquidator::spawn(st.clone());
        if options.configured() {
            tokio::spawn(trading::book::mm::run(st.clone()));
        }
        // option expiry settlement (single-instance job, like the rollover) and the house delta hedger
        if options.configured() {
            tokio::spawn(trading::options::settle::scheduler(st.clone()));
            if cfg.options_hedger {
                tokio::spawn(trading::options::hedger::run(st.clone()));
            }
        }
    }
    tokio::spawn(housekeeping(hub.clone(), pool.clone(), st.limiter.clone()));
    // account jobs (api/lifecycle.rs): expired-demo auto-archive, dormancy flag + auto-archive, retention anonymiser
    tokio::spawn(api::lifecycle::account_jobs(st.clone()));
    // risk desk (src/risk): exposure index + alerts, news windows, client scores, routing suggestions
    trading::risk::spawn(st.clone());

    let app = api::router(st);
    // TCP_NODELAY: a fill publishes several small frames back to back (position, deal, account); Nagle would hold
    // the later ones until the proxy ACKs the first (up to ~40 ms with delayed ACKs)
    let listener = axum::serve::ListenerExt::tap_io(tokio::net::TcpListener::bind(&cfg.bind).await?, |tcp| {
        let _ = tcp.set_nodelay(true);
    });
    tracing::info!(bind = %cfg.bind, shards = cfg.shards, "http listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

async fn run_rollover(hub: &Hub, pool: &sqlx::PgPool, registry: &Registry, day: NaiveDate) {
    let at = server_midnight(day.succ_opt().unwrap());
    let n = hub.rollover(day, at).await;
    for t in registry.all() {
        let r = sqlx::query("INSERT INTO rollovers (tenant_id, day, at, accounts) VALUES ($1,$2,$3,$4) ON CONFLICT (tenant_id, day) DO NOTHING")
            .bind(t.tenant_id)
            .bind(day)
            .bind(at)
            .bind(n as i32)
            .execute(pool)
            .await;
        if let Err(e) = r {
            tracing::error!(error = %e, %day, "recording rollover failed");
        }
    }
    tracing::info!(%day, accounts = n, "swap rollover done");
}

/// Swap rollover at 00:00 server time (NY close, GMT+2/+3), with catch-up of up to 10 missed days after
/// downtime (positions are charged at most once per day via `last_swap_day`).
async fn rollovers(hub: Hub, pool: sqlx::PgPool, registry: Registry) {
    // let the feed deliver prices first (swap conversion uses current quotes)
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    let yesterday = server_date(Utc::now()).pred_opt().unwrap();
    let last: Option<NaiveDate> = sqlx::query_scalar("SELECT max(day) FROM rollovers").fetch_one(&pool).await.unwrap_or(None);
    let mut day = match last {
        Some(d) => d.succ_opt().unwrap().max(yesterday - Duration::days(9)),
        None => yesterday,
    };
    while day <= yesterday {
        run_rollover(&hub, &pool, &registry, day).await;
        day = day.succ_opt().unwrap();
    }
    loop {
        let (at, day) = next_rollover(Utc::now());
        let wait = (at - Utc::now()).to_std().unwrap_or_default();
        tracing::info!(next = %at, %day, "next swap rollover scheduled");
        tokio::time::sleep(wait).await;
        run_rollover(&hub, &pool, &registry, day).await;
    }
}

/// Demo expiry (D8: no activity for `expiry_days`), session / token cleanup, limiter sweep.
async fn housekeeping(hub: Hub, pool: sqlx::PgPool, limiter: Limiter) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(600));
    loop {
        tick.tick().await;
        limiter.sweep(std::time::Duration::from_secs(3600));
        let _ = sqlx::query("DELETE FROM sso_tokens WHERE expires_at < now() - interval '1 day'").execute(&pool).await;
        let _ = sqlx::query("DELETE FROM terminal_sessions WHERE expires_at < now() - interval '30 days'").execute(&pool).await;
        let due: Vec<i64> = sqlx::query_scalar(
            "SELECT login FROM accounts WHERE kind = 'demo' AND status = 'active'
               AND last_activity_at < now() - make_interval(days => COALESCE((demo->>'expiry_days')::int, 10))",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
        for login in due {
            let op: Op = Box::new(|tx, _| {
                trading::engine::funds::set_status(tx, trading::model::Status::Expired)?;
                Ok(Value::Null)
            });
            match hub.exec(login, "system", None, "", "", None, op).await {
                Ok(_) => tracing::info!(login, "demo account expired"),
                Err(e) => tracing::warn!(login, error = ?e, "demo expiry failed"),
            }
        }
    }
}


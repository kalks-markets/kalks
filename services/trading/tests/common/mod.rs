//! Harness for the order-book tests against the REAL local services (no mocked market data): market-data
//! (:8081, raw and group quotes over its WebSocket), the options service (:8104, the real snapshot) and PostgreSQL
//! (:5433, a throw-away database per test process). Env: OPTIONS_URL, OPTIONS_INTERNAL_TOKEN, MARKET_DATA_WS_URL,
//! TRADING_TEST_DATABASE_URL.
#![allow(dead_code)]

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use trading::api::{self, AppState, Body, Ctx, LoginAlloc, StaffCtx};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::Ids;
use trading::feed::{self, QuoteBook};
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, Status};
use trading::money::D;
use trading::options::{OptionPricing, OptionsCtx, Suitability};
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Shared, Staff, Stats, Streams};
use trading::specs::Specs;

pub fn env(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| d.to_string())
}

pub fn dec(v: &Value) -> D {
    v.as_f64().and_then(trading::money::from_f64).or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or_default()
}

pub fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

pub fn q<T: serde::de::DeserializeOwned>(v: Value) -> Query<T> {
    Query(serde_json::from_value(v).unwrap())
}

pub const MM_USER: i64 = 990_001;

pub struct Rig {
    pub st: AppState,
    pub hub: Hub,
    pub pool: sqlx::PgPool,
    pub options: Arc<OptionsCtx>,
    pub tenant: Arc<trading::rules::TenantConfig>,
    pub db: String,
    pub server: PgConnectOptions,
}

impl Rig {
    /// A fresh database, the real options snapshot and market-data feeds, an app state with the market maker user.
    pub async fn boot(name: &str, shards: usize) -> Rig {
        let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
        let opt_url = env("OPTIONS_URL", "http://127.0.0.1:8104");
        let md_ws = env("MARKET_DATA_WS_URL", "ws://127.0.0.1:8081/v1/stream");
        let base = env("TRADING_TEST_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/postgres");
        let db = format!("kalks_trading_{name}_{}", std::process::id());
        let server = PgConnectOptions::from_str(&base).unwrap();
        let url = server.clone().database(&db).to_url_lossy().to_string();
        let pool = trading::persist::connect(&url).await.expect("PostgreSQL :5433 (connect + migrate)");
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
        let specs = Arc::new(Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap());
        let registry = Registry::default();
        for t in trading::persist::load_registry(&pool).await.unwrap() {
            registry.put(t);
        }
        let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&pool).await.unwrap();
        let quotes = Arc::new(QuoteBook::default());
        let options = Arc::new(OptionsCtx::new(&opt_url, &env("OPTIONS_INTERNAL_TOKEN", ""), quotes.clone()));
        assert!(options.poll(Some(&pool)).await.expect("options service :8104"), "snapshot loaded");
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
            max_quote_age_ms: 300_000,
            restrictions: Default::default(),
            options: options.clone(),
            clock: Default::default(),
            books: Default::default(),
            corp: Default::default(),
        });
        let hub = Hub::start(shared, shards, Default::default());
        feed::spawn(hub.clone(), md_ws.clone(), specs.symbols());
        feed::spawn_raw(hub.clone(), md_ws.clone(), specs.symbols());
        // keep the snapshot fresh (it goes stale after staleAfterSecs without a poll)
        let o2 = options.clone();
        let p2 = pool.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let _ = o2.poll(Some(&p2)).await;
            }
        });
        let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
        let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
        let mut cfg = Config::for_tests(&url);
        cfg.options_mm_user = MM_USER;
        hub.shared.books.lp_users.write().unwrap().insert(MM_USER);
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
        let tenant = registry.by_slug("kalks").unwrap();
        Rig { st, hub, pool, options, tenant, db, server }
    }

    /// A process restart on the same database: the account replay (checked against the ledger), new shards and an
    /// empty book registry, then `book::recover` (books loaded, reservations rebuilt, reconcile, outbox re-dispatched).
    /// The market-data feeds keep filling the shared quote book.
    pub async fn restart(&mut self) -> trading::book::Recovery {
        // the old process dies: its actors stop without flushing what they still buffer
        self.hub.shared.books.hooks.dead.store(true, std::sync::atomic::Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        let states = trading::persist::replay_all(&self.pool).await.unwrap();
        assert!(trading::persist::verify_balances(&self.pool, &states).await.unwrap().is_empty(), "ledger vs replay");
        let old = self.hub.shared.clone();
        let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&self.pool).await.unwrap();
        let shared = Arc::new(Shared {
            pool: self.pool.clone(),
            registry: old.registry.clone(),
            specs: old.specs.load().into(),
            held: Default::default(),
            quotes: old.quotes.clone(),
            ids: Arc::new(Ids::new(ticket, deal, txn)),
            index: Arc::new(RwLock::new(Index::default())),
            streams: Streams::default(),
            stats: Arc::new(Stats::default()),
            lp: Arc::new(NullLp),
            max_quote_age_ms: 300_000,
            restrictions: Default::default(),
            options: self.options.clone(),
            clock: Default::default(),
            books: Default::default(),
            corp: Default::default(),
        });
        let hub = Hub::start(shared, 4, states);
        hub.shared.books.lp_users.write().unwrap().insert(MM_USER);
        let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
        let social = trading::social::Social::new(self.pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
        self.st = AppState { hub: hub.clone(), logins, social, ..self.st.clone() };
        self.hub = hub;
        let rep = trading::book::recover(&self.hub).await.unwrap();
        trading::book::mm::load(&self.st).await.unwrap();
        rep
    }

    /// Waits for a raw mid of `symbol` from market-data.
    pub async fn spot(&self, symbol: &str) -> f64 {
        for _ in 0..300 {
            if let Some((m, _)) = self.options.spot(symbol) {
                return m;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("no raw {symbol} mid from market-data :8081 (is the market open?)");
    }

    pub fn ctx(&self, bearer: &str) -> Ctx {
        Ctx { tenant: self.tenant.clone(), ip: "198.51.100.9".into(), user_agent: "e2e".into(), bearer: Some(bearer.to_string()) }
    }

    pub fn staff(&self, id: &str) -> StaffCtx {
        StaffCtx { ctx: Ctx { tenant: self.tenant.clone(), ip: "198.51.100.10".into(), user_agent: "e2e".into(), bearer: None }, staff: Staff { id: id.into(), name: format!("Staff {id}"), role: "super_admin".into() }, perms: None }
    }

    /// Opens a demo client account (eligible for options) and returns its terminal session token.
    pub async fn client(&self, login: i64, user: i64, balance: i64) -> String {
        self.account(login, user, balance).await;
        let ctx = || Ctx { tenant: self.tenant.clone(), ip: "198.51.100.9".into(), user_agent: "e2e".into(), bearer: None };
        let mut h = HeaderMap::new();
        h.insert("x-kalks-user-id", user.to_string().parse().unwrap());
        let Json(sso) = api::accounts::sso(State(self.st.clone()), ctx(), h, Path(login), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
        let Json(s) = api::terminal::sso(State(self.st.clone()), ctx(), body(json!({"token": sso["token"]}))).await.unwrap();
        s["token"].as_str().unwrap().to_string()
    }

    /// Opens a demo client account eligible for options (no terminal session).
    pub async fn account(&self, login: i64, user: i64, balance: i64) {
        let acc = Account {
            tenant_id: 1,
            login,
            user_id: user,
            kind: AccountKind::Demo,
            group: "options-standard".into(),
            mode: Mode::Hedging,
            cent: false,
            leverage: 100,
            status: Status::Active,
            name: "Book E2E".into(),
            route_override: None,
            controls: Controls::default(),
            demo: Some(DemoCfg { initial_balance: D::from(balance), refills_per_day: 3, expiry_days: 10 }),
            created_at: Utc::now(),
            lifecycle: None,
        };
        self.hub.open(acc, ("h".into(), "i".into()), "e2e").await.unwrap();
        self.options.set_suitability(user, Suitability { eligible: true, kyc_verified: true, disclosure_accepted: true, quiz_passed: true, source: "gateway" }, 3_600_000);
    }

    pub async fn drained(&self) {
        for _ in 0..400 {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM book_outbox WHERE status <> 'applied'").fetch_one(&self.pool).await.unwrap();
            if n == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("outbox not drained");
    }

    pub async fn ledger_sum(&self, code: &str) -> D {
        sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code = $1").bind(code).fetch_one(&self.pool).await.unwrap().unwrap_or_default()
    }

    pub async fn ledger_like(&self, pat: &str) -> D {
        sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code LIKE $1").bind(pat).fetch_one(&self.pool).await.unwrap().unwrap_or_default()
    }

    /// Book positions of an account (series → signed contracts).
    pub async fn book_pos(&self, login: i64) -> std::collections::BTreeMap<String, D> {
        let v = self.hub.read(login, Box::new(|x| json!(x.unwrap().0.positions.values().filter(|p| p.on_book()).map(|p| (p.symbol.clone(), (p.volume * p.side.sign()).to_string())).collect::<Vec<_>>()))).await;
        let mut m = std::collections::BTreeMap::new();
        for x in v.as_array().cloned().unwrap_or_default() {
            *m.entry(x[0].as_str().unwrap().to_string()).or_insert(D::ZERO) += x[1].as_str().unwrap().parse::<D>().unwrap();
        }
        m
    }

    pub async fn reserve(&self, login: i64) -> D {
        self.hub.read(login, Box::new(|x| json!(x.unwrap().0.book.reserve().to_string()))).await.as_str().unwrap().parse().unwrap()
    }

    /// Money invariants: every transaction balances, the ledger nets per currency, the account replay equals the
    /// live state and the ledger balances, every book's journal replays byte for byte.
    pub async fn money_and_replay_ok(&self, logins: &[i64]) {
        let unbalanced: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT txn_id FROM ledger_postings GROUP BY txn_id, currency HAVING sum(amount) <> 0) x").fetch_one(&self.pool).await.unwrap();
        assert_eq!(unbalanced, 0, "every ledger transaction balances");
        let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(&self.pool).await.unwrap();
        assert!(nets.iter().all(|(_, v)| v.is_zero()), "{nets:?}");
        let states = trading::persist::replay_all(&self.pool).await.unwrap();
        assert!(trading::persist::verify_balances(&self.pool, &states).await.unwrap().is_empty(), "ledger vs replay");
        for l in logins {
            let live = self.hub.read(*l, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await;
            assert_eq!(serde_json::to_value(&states[l]).unwrap(), live, "replay diverged for {l}");
        }
        for (book, res) in trading::book::replay_audit(&self.hub).await {
            assert!(res.is_ok(), "replay audit {book}: {res:?}");
        }
    }

    pub async fn drop_db(self) {
        drop(self.hub);
        self.pool.close().await;
        let mut admin = self.server.database("postgres").connect().await.unwrap();
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.db))).execute(&mut admin).await;
    }
}

/// (series, cut) of the call nearest to `spot` (offset `k` strikes up) of the first expiry at least `min_hours` away.
pub fn pick(snap: &trading::options::OptSnapshot, underlying: &str, spot: f64, min_hours: i64, k: usize, right: &str) -> Option<(String, chrono::DateTime<Utc>)> {
    let now = Utc::now();
    let mut ex: Vec<_> = snap.expiries.iter().filter(|e| e.symbol == underlying && e.status == "listed" && e.cut_at > now + chrono::Duration::hours(min_hours)).collect();
    ex.sort_by_key(|e| e.cut_at);
    let e = ex.first()?;
    let mut ss: Vec<_> = snap.series.values().filter(|s| s.expiry_id == e.id && s.kind == right && s.status == "active").collect();
    ss.sort_by(|a, b| a.strike.total_cmp(&b.strike));
    let i = ss.iter().enumerate().min_by(|a, b| (a.1.strike - spot).abs().total_cmp(&(b.1.strike - spot).abs()))?.0;
    let s = ss.get(i + k)?;
    Some((s.code.clone(), e.cut_at))
}

//! Option EXPIRY and SETTLEMENT end to end on LIVE prices (docs/OPTIONS-EXCHANGE.md §9, §13: no mocked market
//! data). The real local market-data (:8081, a relay of production prices), the real options service (:8104: the
//! snapshot, the TWAP sampler and the fixing job) and PostgreSQL (:5433, a throw-away database that survives the
//! engine crashes on purpose).
//!
//! Real expiry cuts are 14:00 UTC; to run the TWAP fixing on live prices at any time of day the local options
//! service lists an ad-hoc expiry a few minutes ahead (`OPTIONS_TEST_EXPIRIES=1`, local only, never in
//! production; `POST /v1/admin/options/test/expiries {symbol, cutInMinutes}`). The test picks the nearest listed
//! EURUSD expiry whose cut is 16 min to 6 h ahead, and the XAUUSD expiry with the same cut (override with
//! `EXPIRY_E2E_EURUSD` / `EXPIRY_E2E_XAUUSD` = `SYMBOL:YYYY-MM-DD`).
//!
//! Four accounts of tenant `kalks`: a live USD and a live cent (USC) account trade the house-priced B-book
//! (vanilla calls / puts, a knock-out barrier, a cent short), a demo USD and a demo cent account trade each other
//! on the order book (the cross-currency case: premium and settlement through the expiry's clearing account,
//! the cent side via `house:fx`). The engine is the real one (the same boot as main.rs: event replay, ledger
//! verification, book recovery, the settlement scheduler), crashed with `abort()` (or `kill -9`) on purpose.
//!
//! Phases (`EXPIRY_E2E_PHASE`), one process each (the database `EXPIRY_E2E_DB`, default `kalks_expiry_e2e`):
//! 1. `open` (before cut − 15 min): fresh database, accounts, trades; then the engine stays up past the cut
//!    (book `Expire` at cut − closeOnlyMinutes cancels the resting order) and CRASHES (`abort`) at cut + 20 s, or
//!    earlier with `kill -9`.
//! 2. `partial` (after the cut): restart (replay, verify, recover); nothing settled while down; waits for the real
//!    fixing; settles the demo cent account alone, then CRASHES in the middle of the pass.
//! 3. `settle`: restart; the settlement scheduler catches up (exactly once: the cent account is not settled
//!    again); a second pass and a direct second settle are no-ops; every payout = the fixing payoff; the ledger
//!    balances per currency, the clearing account nets to 0 per tenant and kind, the payouts are held; the hold
//!    releases one hour later; a re-run at a corrected fixing (options service re-fix, run 2) moves every balance
//!    by exactly the difference; replay = live; the book journal replays identically.
//! 4. `hold` (optional, ≥ 1 h after `settle`, real clock): restart; every settlement hold has released.
//!
//! ```text
//! EXPIRY_E2E_PHASE=open cargo test -p trading --test expiry_e2e -- --ignored --nocapture
//! ```

use axum::Json;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::HeaderMap;
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use trading::api::{self, AppState, Body, Ctx, LoginAlloc, StaffCtx};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::{Ids, funds};
use trading::feed::{self, QuoteBook};
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, Status};
use trading::money::{D, r2};
use trading::options::{OptionPricing, OptionsCtx, Suitability};
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::specs::Specs;

const LUSD: i64 = 10_009_401;
const LUSC: i64 = 10_009_402;
const DUSD: i64 = 50_009_403;
const DUSC: i64 = 50_009_404;
const USERS: [(i64, i64); 4] = [(LUSD, 9401), (LUSC, 9402), (DUSD, 9403), (DUSC, 9404)];

fn env(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| d.to_string())
}

fn dec(v: &Value) -> D {
    v.as_f64().and_then(trading::money::from_f64).or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or_default()
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

fn account(login: i64, user: i64) -> Account {
    let kind = if login >= 50_000_000 { AccountKind::Demo } else { AccountKind::Live };
    let cent = login == LUSC || login == DUSC;
    Account {
        tenant_id: 1,
        login,
        user_id: user,
        kind,
        group: if cent { "options-cent".into() } else { "options-standard".into() },
        mode: Mode::Hedging,
        cent,
        leverage: 100,
        status: Status::Active,
        name: format!("Expiry E2E {}{}", if kind == AccountKind::Demo { "demo " } else { "live " }, if cent { "USC" } else { "USD" }),
        route_override: None,
        controls: Controls::default(),
        demo: (kind == AccountKind::Demo).then(|| DemoCfg { initial_balance: D::from(10_000), refills_per_day: 3, expiry_days: 10 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

/// Plain HTTP/1.1 to the local options service (internal + staff headers).
async fn http(method: &str, url: &str, body: Option<Value>) -> Value {
    let rest = url.trim_start_matches("http://");
    let (host, path) = rest.split_once('/').map(|(h, p)| (h.to_string(), format!("/{p}"))).unwrap_or((rest.to_string(), "/".into()));
    let mut s = tokio::net::TcpStream::connect(&host).await.expect("options service");
    let b = body.map(|v| v.to_string()).unwrap_or_default();
    let tok = env("OPTIONS_INTERNAL_TOKEN", "");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nhost: {host}\r\nconnection: close\r\ncontent-type: application/json\r\nx-kalks-internal: {tok}\r\nx-kalks-staff: expiry-e2e\r\ncontent-length: {}\r\n\r\n{b}",
        b.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let (_, payload) = text.split_once("\r\n\r\n").unwrap_or(("", ""));
    serde_json::from_str(payload).unwrap_or_else(|_| json!({"raw": payload}))
}

struct Eng {
    st: AppState,
    hub: Hub,
    pool: sqlx::PgPool,
    options: Arc<OptionsCtx>,
    tenant: Arc<trading::rules::TenantConfig>,
}

/// The engine's boot (main.rs): connect + migrate, tenants, every account replayed from its events and checked
/// against the ledger, the hub, the real feeds and options snapshot, then the order book recovery.
async fn boot(url: &str, fresh: bool) -> Eng {
    let pool = trading::persist::connect(url).await.expect("PostgreSQL :5433");
    // an Options cent group for the cent accounts (CFD / Options account split; only `options-standard` is seeded)
    sqlx::query("INSERT INTO groups (tenant_id, code, name, mode, cent, leverages, default_leverage, margin_call_pct, stop_out_pct, spread_group, product) VALUES (1, 'options-cent', 'Options Cent', 'hedging', true, '{100}', 100, 60, 20, 'cent', 'options') ON CONFLICT DO NOTHING").execute(&pool).await.unwrap();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    let specs = Arc::new(Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap());
    let registry = Registry::default();
    for t in trading::persist::load_registry(&pool).await.unwrap() {
        registry.put(t);
    }
    let states = trading::persist::replay_all(&pool).await.unwrap();
    let mism = trading::persist::verify_balances(&pool, &states).await.unwrap();
    assert!(mism.is_empty(), "ledger / replay mismatch at boot: {mism:?}");
    assert!(!fresh || states.is_empty());
    eprintln!("[boot] {} accounts replayed, ledger verified", states.len());
    let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&pool).await.unwrap();
    let quotes = Arc::new(QuoteBook::default());
    let options = Arc::new(OptionsCtx::new(&env("OPTIONS_URL", "http://127.0.0.1:8104"), &env("OPTIONS_INTERNAL_TOKEN", ""), quotes.clone()));
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
    let hub = Hub::start(shared, 2, states);
    let md = env("MARKET_DATA_WS_URL", "ws://127.0.0.1:8081/v1/stream");
    feed::spawn(hub.clone(), md.clone(), specs.symbols());
    feed::spawn_raw(hub.clone(), md, specs.symbols());
    options.spawn_poller(pool.clone());
    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config::for_tests(url);
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
    if !fresh {
        let rep = trading::book::recover(&hub).await.unwrap();
        eprintln!("[boot] book recovery: {} books, {} orders, {} pending, mismatches {:?}", rep.books, rep.orders, rep.pending, rep.mismatches);
        assert!(rep.mismatches.is_empty(), "book reconcile after the crash: {:?}", rep.mismatches);
    }
    for (_, u) in USERS {
        options.set_suitability(u, Suitability { eligible: true, kyc_verified: true, disclosure_accepted: true, quiz_passed: true, source: "gateway" }, 86_400_000);
    }
    let tenant = registry.by_slug("kalks").unwrap();
    Eng { st, hub, pool, options, tenant }
}

async fn token(e: &Eng, login: i64, user: i64) -> String {
    let ctx = || Ctx { tenant: e.tenant.clone(), ip: "198.51.100.11".into(), user_agent: "expiry-e2e".into(), bearer: None };
    let mut h = HeaderMap::new();
    h.insert("x-kalks-user-id", user.to_string().parse().unwrap());
    let Json(sso) = api::accounts::sso(State(e.st.clone()), ctx(), h, Path(login), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    let Json(s) = api::terminal::sso(State(e.st.clone()), ctx(), body(json!({"token": sso["token"]}))).await.unwrap();
    s["token"].as_str().unwrap().to_string()
}

async fn staff(st: &AppState) -> StaffCtx {
    let req = axum::http::Request::builder().header("x-kalks-staff-id", "45").header("x-kalks-staff-name", "Expiry%20E2E").header("x-kalks-staff-role", "admin").body(()).unwrap();
    let (mut parts, _) = req.into_parts();
    StaffCtx::from_request_parts(&mut parts, st).await.unwrap()
}

async fn state(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await
}

async fn balance(hub: &Hub, login: i64) -> D {
    dec(&state(hub, login).await["balance"])
}

/// Option positions of an account: (ticket, series, expiry key, signed contracts, venue).
async fn opt_positions(hub: &Hub, login: i64) -> Vec<Value> {
    let s = state(hub, login).await;
    s["positions"].as_object().map(|m| m.values().filter(|p| !p["option"].is_null()).cloned().collect()).unwrap_or_default()
}

async fn ledger_sum(pool: &sqlx::PgPool, code: &str) -> D {
    sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code = $1").bind(code).fetch_one(pool).await.unwrap().unwrap_or_default()
}

async fn drained(pool: &sqlx::PgPool) {
    for _ in 0..400 {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM book_outbox WHERE status <> 'applied'").fetch_one(pool).await.unwrap();
        if n == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("outbox not drained");
}

/// The expiry under test: env override, else the nearest listed one of `symbol` with a cut 16 min to 6 h ahead
/// (`open`), or the given key (later phases read it back from the database).
fn pick_expiry(snap: &trading::options::OptSnapshot, symbol: &str, cut: Option<DateTime<Utc>>) -> Option<(String, NaiveDate, DateTime<Utc>)> {
    if let Ok(k) = std::env::var(format!("EXPIRY_E2E_{symbol}"))
        && let Some((s, d)) = trading::options::settle::parse_key(&k)
    {
        return snap.expiry(&s, d).map(|e| (e.key(), e.expiry_date, e.cut_at));
    }
    let t = now();
    let mut ex: Vec<_> = snap
        .expiries
        .iter()
        .filter(|e| e.symbol == symbol && e.status == "listed" && e.cut_at > t + chrono::Duration::minutes(16) && e.cut_at < t + chrono::Duration::hours(6) && cut.is_none_or(|c| e.cut_at == c))
        .collect();
    ex.sort_by_key(|e| e.cut_at);
    ex.first().map(|e| (e.key(), e.expiry_date, e.cut_at))
}

/// Nearest-ATM series code of an expiry.
fn atm(snap: &trading::options::OptSnapshot, symbol: &str, date: NaiveDate, kind: &str, spot: f64) -> String {
    let e = snap.expiry(symbol, date).unwrap();
    snap.series.values().filter(|s| s.expiry_id == e.id && s.kind == kind && s.status == "active").min_by(|a, b| (a.strike - spot).abs().total_cmp(&(b.strike - spot).abs())).unwrap().code.clone()
}

/// The nearest in-the-money series (a call at or below the spot, a put at or above): it has a bid even minutes
/// before the cut, so it can be sold.
fn itm(snap: &trading::options::OptSnapshot, symbol: &str, date: NaiveDate, kind: &str, spot: f64) -> String {
    let e = snap.expiry(symbol, date).unwrap();
    snap.series
        .values()
        .filter(|s| s.expiry_id == e.id && s.kind == kind && s.status == "active" && if kind == "call" { s.strike <= spot } else { s.strike >= spot })
        .min_by(|a, b| (a.strike - spot).abs().total_cmp(&(b.strike - spot).abs()))
        .unwrap()
        .code
        .clone()
}

/// What a position pays at `fixing` in its account currency (USD-quoted underlyings: q = 1), rounded like the
/// engine (per position, 2 dp).
fn payout(p: &Value, fixing: D, factor: D) -> D {
    let t = &p["option"];
    let k = dec(&t["strike"]);
    let intrinsic = match t["right"].as_str() {
        Some("call") => (fixing - k).max(D::ZERO),
        _ => (k - fixing).max(D::ZERO),
    };
    // a barrier the fixing reached was touched in the window: knock-out → rebate, knock-in → the vanilla
    let pay = match t["barrier"].as_object() {
        Some(b) => {
            let kind = b["kind"].as_str().unwrap_or("");
            let level = dec(&b["level"]);
            let hit = if kind.starts_with('U') { fixing >= level } else { fixing <= level };
            let knocked_in = b.get("knocked_in").and_then(Value::as_bool) == Some(true);
            match (kind.ends_with('I'), hit) {
                (false, true) => dec(&b["rebate"]),
                (true, false) if !knocked_in => dec(&b["rebate"]),
                _ => intrinsic,
            }
        }
        None => intrinsic,
    };
    let sign = if p["side"] == "buy" { D::ONE } else { -D::ONE };
    r2(sign * dec(&p["volume"]) * dec(&t["contract_size"]) * pay * factor)
}

fn factor(login: i64) -> D {
    if login == LUSC || login == DUSC { D::from(100) } else { D::ONE }
}

/// The run's bookkeeping between the phases (expiries, series), in a table of the test database.
async fn save(pool: &sqlx::PgPool, k: &str, v: &Value) {
    sqlx::query("CREATE TABLE IF NOT EXISTS expiry_e2e (k TEXT PRIMARY KEY, v JSONB NOT NULL)").execute(pool).await.unwrap();
    sqlx::query("INSERT INTO expiry_e2e (k, v) VALUES ($1, $2) ON CONFLICT (k) DO UPDATE SET v = EXCLUDED.v").bind(k).bind(sqlx::types::Json(v)).execute(pool).await.unwrap();
}

async fn load(pool: &sqlx::PgPool, k: &str) -> Value {
    sqlx::query_scalar::<_, sqlx::types::Json<Value>>("SELECT v FROM expiry_e2e WHERE k = $1").bind(k).fetch_one(pool).await.map(|j| j.0).unwrap_or(Value::Null)
}

/// Every transaction balances and every currency nets to 0.
async fn ledger_balanced(pool: &sqlx::PgPool) {
    let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(pool).await.unwrap();
    assert!(nets.iter().all(|(_, v)| v.is_zero()), "ledger nets per currency: {nets:?}");
    let bad: Vec<(i64, String, D)> = sqlx::query_as("SELECT txn_id, currency, sum(amount) FROM ledger_postings GROUP BY txn_id, currency HAVING sum(amount) <> 0").fetch_all(pool).await.unwrap();
    assert!(bad.is_empty(), "unbalanced transactions: {bad:?}");
}

/// The published fixing of an expiry and its fixing runs (samples, coverage) from the options service.
async fn fixing_record(symbol: &str, date: NaiveDate) -> Value {
    let v = http("GET", &format!("{}/v1/internal/options/fixings?expiry={date}&u={symbol}", env("OPTIONS_URL", "http://127.0.0.1:8104")), None).await;
    v["fixings"].as_array().and_then(|a| a.iter().find(|x| x["symbol"] == symbol).cloned()).unwrap_or(Value::Null)
}

#[tokio::test]
#[ignore = "needs the real market-data (:8081), the options service (:8104) with OPTIONS_TEST_EXPIRIES=1 and PostgreSQL (:5433); see the header"]
async fn expiry_and_settlement_on_live_prices() {
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    let phase = env("EXPIRY_E2E_PHASE", "open");
    let base = env("TRADING_TEST_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/postgres");
    let db = env("EXPIRY_E2E_DB", "kalks_expiry_e2e");
    let server = PgConnectOptions::from_str(&base).unwrap();
    server.clone().database("postgres").connect().await.expect("PostgreSQL :5433 (this test never skips)");
    let url = server.clone().database(&db).to_url_lossy().to_string();
    eprintln!("[{phase}] database {db}, {}", now());
    match phase.as_str() {
        "open" => open(&server, &db, &url).await,
        "partial" => partial(&url).await,
        "settle" => settle(&url).await,
        "hold" => hold(&url).await,
        p => panic!("unknown EXPIRY_E2E_PHASE {p}"),
    }
}

/* ------------------------------------------------------------------ */
/* 1. open                                                             */
/* ------------------------------------------------------------------ */

async fn open(server: &PgConnectOptions, db: &str, url: &str) {
    let mut admin = server.clone().database("postgres").connect().await.unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await.unwrap();
    let e = boot(url, true).await;
    let (hub, st, pool) = (e.hub.clone(), e.st.clone(), e.pool.clone());
    let mut spot = BTreeMap::new();
    for s in ["EURUSD", "XAUUSD"] {
        for _ in 0..300 {
            if let Some((m, _)) = e.options.spot(s) {
                spot.insert(s, m);
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
    assert_eq!(spot.len(), 2, "raw EURUSD and XAUUSD mids from market-data :8081 (is the market open?)");
    let snap = e.options.snapshot().unwrap();
    let (keu, deu, cut) = pick_expiry(&snap, "EURUSD", None).expect("a listed EURUSD expiry 16 min to 6 h ahead (POST /v1/admin/options/test/expiries)");
    let (kxau, dxau, cut_x) = pick_expiry(&snap, "XAUUSD", Some(cut)).expect("a XAUUSD expiry with the same cut");
    assert_eq!(cut, cut_x);
    eprintln!("[open] EURUSD {keu}, XAUUSD {kxau}, cut {cut}, spots {spot:?}");

    for (l, u) in USERS {
        hub.open(account(l, u), ("h".into(), "i".into()), "expiry-e2e").await.unwrap();
    }
    for (l, amt) in [(LUSD, D::from(20_000)), (LUSC, D::from(20_000))] {
        let key = format!("expiry-e2e-fund-{l}");
        let op: Op = Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, amt, &key, None).map(|_| Value::Null));
        hub.exec(l, "test", None, "", "", None, op).await.unwrap();
    }
    let mut tok = BTreeMap::new();
    for (l, u) in USERS {
        tok.insert(l, token(&e, l, u).await);
    }
    let ctx = |l: i64| Ctx { tenant: e.tenant.clone(), ip: "198.51.100.11".into(), user_agent: "expiry-e2e".into(), bearer: Some(tok[&l].clone()) };
    // the order book is live for demo accounts; live accounts keep the house-priced B-book
    hub.shared.books.enable_venue(&pool, 1, AccountKind::Demo, "expiry-e2e", "expiry e2e").await.unwrap();
    assert!(trading::book::recover(&hub).await.unwrap().mismatches.is_empty());

    let c_eu = itm(&snap, "EURUSD", deu, "call", spot["EURUSD"]);
    let p_eu = itm(&snap, "EURUSD", deu, "put", spot["EURUSD"]);
    let p_xau = atm(&snap, "XAUUSD", dxau, "put", spot["XAUUSD"]);
    eprintln!("[open] series {c_eu} {p_eu} {p_xau}");

    // ---- house (B-book), live accounts ----
    let house = |legs: Value, cid: &str| json!({"legs": legs, "type": "market", "clientOrderId": cid});
    let leg = |s: &str, side: &str, c: i64| json!({"series": s, "side": side, "contracts": c});
    let mut placed = Vec::new();
    for (l, legs, cid) in [
        (LUSD, json!([leg(&c_eu, "buy", 2)]), "h1"),
        (LUSD, json!([leg(&p_eu, "sell", 1)]), "h2"),
        (LUSC, json!([leg(&p_xau, "buy", 3)]), "h4"),
        (LUSC, json!([leg(&c_eu, "sell", 1)]), "h5"),
    ] {
        let Json(o) = api::options::place(State(st.clone()), ctx(l), body(house(legs, cid))).await.unwrap_or_else(|x| panic!("{cid}: {x:?}"));
        assert_eq!(o["status"], "filled", "{cid}: {o}");
        placed.push(json!({"login": l, "cid": cid, "positions": o["positions"].as_array().unwrap().iter().map(|p| json!({"ticket": p["ticket"], "series": p["symbol"], "price": p["openPrice"]})).collect::<Vec<_>>()}));
    }
    // a knock-out barrier: an up-and-out call 0.8 % above the spot (it settles like the vanilla unless a real tick
    // knocks it before the cut)
    let k = trading::money::from_f64(spot["EURUSD"] * 1.008).unwrap().round_dp(4);
    let ko = json!([{"series": c_eu, "side": "buy", "contracts": 1, "barrier": {"kind": "UO", "level": k.to_string().parse::<f64>().unwrap(), "rebate": 0.0001}}]);
    let Json(b) = api::options::place(State(st.clone()), ctx(LUSD), body(house(ko, "h3"))).await.unwrap_or_else(|x| panic!("barrier: {x:?}"));
    assert_eq!((b["status"].as_str(), b["positions"][0]["option"]["barrier"]["kind"].as_str()), (Some("filled"), Some("UO")), "{b}");
    placed.push(json!({"login": LUSD, "cid": "h3", "barrier": k.to_string(), "ticket": b["positions"][0]["ticket"]}));
    for l in [LUSD, LUSC] {
        assert!(opt_positions(&hub, l).await.iter().all(|p| p["venue"].is_null() || p["venue"] == "house"), "house venue on live");
    }

    // ---- order book, demo USD vs demo cent (cross currency) ----
    let order = |series: &str, side: &str, kind: &str, qty: i64, price: Option<D>, cid: &str| json!({"series": series, "side": side, "type": kind, "qty": qty, "price": price.map(|p| p.to_string()), "tif": if kind == "market" { "ioc" } else { "gtc" }, "clientOrderId": cid});
    let tick = D::new(1, 5);
    let mark = |s: &str| {
        let (t, _) = trading::engine::options_book::terms_of(&snap, s).unwrap();
        ((e.options.price("kalks", "*", &t, now()).unwrap().mark / tick).floor() * tick).max(tick)
    };
    let (pc, pp) = (mark(&c_eu), mark(&p_eu));
    // USD rests a sell of 4 calls; cent buys 3 (1 stays working until the expiry cut-off cancels it)
    let Json(o1) = api::options_book::place(State(st.clone()), ctx(DUSD), body(order(&c_eu, "sell", "limit", 4, Some(pc), "b1"))).await.unwrap();
    assert_eq!(o1["status"], "working", "{o1}");
    let Json(o2) = api::options_book::place(State(st.clone()), ctx(DUSC), body(order(&c_eu, "buy", "limit", 3, Some(pc), "b2"))).await.unwrap();
    assert_eq!(o2["status"], "filled", "{o2}");
    // cent rests a bid for 2 puts; USD sells into it at market
    let Json(o3) = api::options_book::place(State(st.clone()), ctx(DUSC), body(order(&p_eu, "buy", "limit", 2, Some(pp), "b3"))).await.unwrap();
    assert_eq!(o3["status"], "working", "{o3}");
    let Json(o4) = api::options_book::place(State(st.clone()), ctx(DUSD), body(order(&p_eu, "sell", "market", 2, None, "b4"))).await.unwrap();
    assert_eq!(o4["status"], "filled", "{o4}");
    drained(&pool).await;
    let book_pos = |l: i64| {
        let hub = hub.clone();
        async move { opt_positions(&hub, l).await.iter().filter(|p| p["venue"] == "book").map(|p| (p["symbol"].as_str().unwrap().to_string(), dec(&p["volume"]) * if p["side"] == "buy" { D::ONE } else { -D::ONE })).collect::<BTreeMap<_, _>>() }
    };
    let (pu, pcent) = (book_pos(DUSD).await, book_pos(DUSC).await);
    assert_eq!(pu, BTreeMap::from([(c_eu.clone(), D::from(-3)), (p_eu.clone(), D::from(-2))]));
    assert_eq!(pcent, BTreeMap::from([(c_eu.clone(), D::from(3)), (p_eu.clone(), D::from(2))]));
    let clearing = format!("house:options_clearing.EURUSD.{}:USD", deu.format("%Y%m%d"));
    assert_eq!(ledger_sum(&pool, &clearing).await, D::ZERO, "the fills net to 0 in the clearing account");
    ledger_balanced(&pool).await;
    let reserve = |l: i64| {
        let hub = hub.clone();
        async move { dec(&hub.read(l, Box::new(|x| json!(x.unwrap().0.book.reserve().to_string()))).await) }
    };
    assert!(reserve(DUSD).await > D::ZERO, "the working sell of 1 call reserves");

    let mut summary = json!({"expiries": {"EURUSD": {"key": keu, "date": deu}, "XAUUSD": {"key": kxau, "date": dxau}}, "cut": cut, "series": {"call": c_eu, "put": p_eu, "xauPut": p_xau},
        "spots": spot, "bookPrices": {"call": pc.to_string(), "put": pp.to_string()}, "house": placed});
    for (l, _) in USERS {
        summary["balancesAfterOpen"][l.to_string()] = json!(balance(&hub, l).await.to_string());
        summary["positionsAfterOpen"][l.to_string()] = json!(opt_positions(&hub, l).await.iter().map(|p| json!({"ticket": p["ticket"], "series": p["symbol"], "side": p["side"], "volume": p["volume"], "venue": p["venue"], "premium": p["premium"]})).collect::<Vec<_>>());
    }
    save(&pool, "open", &summary).await;
    eprintln!("[open] {}", serde_json::to_string_pretty(&summary).unwrap());

    // ---- the engine stays up into the cut, then crashes ----
    trading::book::spawn_scheduler(hub.clone());
    let abort_at = cut + chrono::Duration::seconds(env("EXPIRY_E2E_ABORT_AFTER_CUT_SECS", "20").parse().unwrap_or(20));
    let close_only = snap.underlying("EURUSD").map(|u| u.close_only_minutes).unwrap_or(1) as i64;
    let mut expired_checked = false;
    loop {
        let t = now();
        if !expired_checked && t > cut - chrono::Duration::minutes(close_only) + chrono::Duration::seconds(8) {
            // the cut-off cancelled the working order and released its reserve
            drained(&pool).await;
            let open_orders: i64 = sqlx::query_scalar("SELECT count(*) FROM book_orders WHERE login = $1 AND status = 'open'").bind(DUSD).fetch_one(&pool).await.unwrap();
            let statuses: Vec<(i64, String)> = sqlx::query_as("SELECT id, status FROM book_orders WHERE login = $1 ORDER BY id").bind(DUSD).fetch_all(&pool).await.unwrap();
            eprintln!("[open] DUSD book orders {statuses:?}");
            let r = reserve(DUSD).await;
            eprintln!("[open] expiry cut-off: {open_orders} resting orders stored, reserve {r}");
            assert_eq!((open_orders, r), (0, D::ZERO), "Expire at cut − closeOnlyMinutes cancels and releases");
            save(&pool, "expire_checked", &json!({"at": t, "reserve": r.to_string()})).await;
            expired_checked = true;
        }
        if t >= abort_at {
            let n: usize = futures_util::future::join_all([LUSD, LUSC, DUSD, DUSC].map(|l| opt_positions(&hub, l))).await.iter().map(Vec::len).sum();
            eprintln!("[open] {t}: CRASH (abort) {} s after the cut with {n} option positions open, nothing settled", (t - cut).num_seconds());
            std::process::abort();
        }
        if t.timestamp() % 60 == 0 {
            eprintln!("[open] {t}: up, {} s to the cut", (cut - t).num_seconds());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/* ------------------------------------------------------------------ */
/* 2. partial                                                          */
/* ------------------------------------------------------------------ */

async fn wait_fixing(e: &Eng, symbol: &str, date: NaiveDate) -> (D, i32) {
    for _ in 0..240 {
        let _ = e.options.poll(Some(&e.pool)).await;
        if let Some((f, run, _)) = trading::options::settle::fixing_for(&e.st, symbol, date).await {
            return (f, run);
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    panic!("no fixing for {symbol}:{date} within 20 min");
}

async fn partial(url: &str) {
    let e = boot(url, false).await;
    let o = load(&e.pool, "open").await;
    let cut: DateTime<Utc> = serde_json::from_value(o["cut"].clone()).unwrap();
    assert!(now() >= cut, "run `partial` after the cut ({cut})");
    let keu = o["expiries"]["EURUSD"]["key"].as_str().unwrap().to_string();
    let deu: NaiveDate = serde_json::from_value(o["expiries"]["EURUSD"]["date"].clone()).unwrap();
    // nothing settled while the engine was down: every option position is still open past its cut
    let mut open_n = 0;
    for l in [LUSD, LUSC, DUSD, DUSC] {
        open_n += opt_positions(&e.hub, l).await.len();
    }
    let settled: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key LIKE 'settle:%'").fetch_one(&e.pool).await.unwrap();
    eprintln!("[partial] after the crash: {open_n} option positions open past the cut, {settled} settlements");
    assert_eq!(settled, 0);
    let (fixing, run) = wait_fixing(&e, "EURUSD", deu).await;
    eprintln!("[partial] EURUSD fixing {fixing} run {run}");
    // the pass starts: the demo cent account settles first ...
    let k = keu.clone();
    let op: Op = Box::new(move |tx, env| Ok(json!(trading::engine::options::settle(tx, env, &k, fixing, run, None).tickets)));
    let first = e.hub.exec(DUSC, "system", None, "", "", None, op).await.unwrap();
    let n = first.value.as_array().map(Vec::len).unwrap_or(0);
    assert_eq!(n, 2, "the cent account's call and put");
    save(&e.pool, "partial", &json!({"settled": [DUSC], "tickets": first.value, "fixing": fixing.to_string(), "run": run, "balance": balance(&e.hub, DUSC).await.to_string()})).await;
    eprintln!("[partial] {DUSC} settled {} at {fixing}: CRASH (abort) in the middle of the pass", first.value);
    // ... and the engine dies before the others
    std::process::abort();
}

/* ------------------------------------------------------------------ */
/* 3. settle                                                           */
/* ------------------------------------------------------------------ */

async fn settle(url: &str) {
    let e = boot(url, false).await;
    let (hub, st, pool) = (e.hub.clone(), e.st.clone(), e.pool.clone());
    let o = load(&pool, "open").await;
    let part = load(&pool, "partial").await;
    let mut exp = BTreeMap::new();
    for s in ["EURUSD", "XAUUSD"] {
        let d: NaiveDate = serde_json::from_value(o["expiries"][s]["date"].clone()).unwrap();
        exp.insert(s, (o["expiries"][s]["key"].as_str().unwrap().to_string(), d));
    }
    let mut fix = BTreeMap::new();
    for (s, (_, d)) in &exp {
        fix.insert(*s, wait_fixing(&e, s, *d).await);
    }
    let mut records = json!({});
    for (s, (_, d)) in &exp {
        let r = fixing_record(s, *d).await;
        eprintln!("[settle] fixing record {s}: {r}");
        records[*s] = r;
    }
    // the cent account settled before the crash and nothing else did
    assert!(opt_positions(&hub, DUSC).await.is_empty(), "settled before the crash (persisted)");
    let before: BTreeMap<i64, D> = futures_util::future::join_all([LUSD, LUSC, DUSD, DUSC].map(|l| {
        let hub = hub.clone();
        async move { (l, balance(&hub, l).await) }
    }))
    .await
    .into_iter()
    .collect();
    // expected payouts at the fixing (by position, before settlement)
    let mut expect: BTreeMap<i64, D> = BTreeMap::new();
    let mut pending = 0usize;
    let mut detail = Vec::new();
    for l in [LUSD, LUSC, DUSD] {
        for p in opt_positions(&hub, l).await {
            let sym = p["option"]["underlying"].as_str().unwrap().to_string();
            let f = fix[sym.as_str()].0;
            let pay = payout(&p, f, factor(l));
            *expect.entry(l).or_default() += pay;
            pending += 1;
            detail.push(json!({"login": l, "ticket": p["ticket"], "series": p["symbol"], "side": p["side"], "volume": p["volume"], "venue": p["venue"], "barrier": p["option"]["barrier"], "fixing": f.to_string(), "payout": pay.to_string()}));
        }
    }
    eprintln!("[settle] {pending} positions to settle: {}", serde_json::to_string_pretty(&detail).unwrap());
    // the engine's settlement scheduler catches up after the restart
    tokio::spawn(trading::options::settle::scheduler(st.clone()));
    let t0 = now();
    loop {
        let mut left = 0;
        for l in [LUSD, LUSC, DUSD] {
            left += opt_positions(&hub, l).await.len();
        }
        if left == 0 {
            break;
        }
        assert!(now() - t0 < chrono::Duration::seconds(120), "the scheduler did not settle within 2 min ({left} left)");
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    eprintln!("[settle] scheduler settled everything in {} s", (now() - t0).num_seconds());
    tokio::time::sleep(Duration::from_secs(2)).await;
    for (l, x) in &expect {
        let got = balance(&hub, *l).await - before[l];
        eprintln!("[settle] {l}: settlement cash {got} (expected {x})");
        assert_eq!(got, *x, "settlement cash of {l}");
    }
    // the cent account's settlement before the crash, at the same fixing
    let dusc_cash: Vec<(String, D)> = sqlx::query_as("SELECT t.idempotency_key, p.amount FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id WHERE t.idempotency_key LIKE 'settle:%' AND p.account_code = $1").bind(format!("acct:{DUSC}:balance")).fetch_all(&pool).await.unwrap();
    assert_eq!(dusc_cash.len(), 2, "{dusc_cash:?}");
    // exactly once: one transaction per (expiry, run, ticket); a second pass and a direct second settle book nothing
    assert_eq!(trading::options::settle::run_due(&st, &mut HashSet::new()).await, 0, "a second pass is a no-op");
    for (s, (k, _)) in &exp {
        let rep = hub.settle(k, fix[s].0, fix[s].1, None).await;
        assert_eq!(rep.accounts.iter().map(|a| a.2).sum::<usize>(), 0, "a second settle of {k} is a no-op");
    }
    let keys: Vec<(String, i64)> = sqlx::query_as("SELECT idempotency_key, count(*) FROM ledger_txns WHERE idempotency_key LIKE 'settle:%' GROUP BY 1 ORDER BY 1").fetch_all(&pool).await.unwrap();
    assert!(keys.iter().all(|(_, n)| *n == 1), "{keys:?}");
    let deals: i64 = sqlx::query_scalar("SELECT count(*) FROM deals WHERE reason = 'expiry'").fetch_one(&pool).await.unwrap();
    eprintln!("[settle] {} settlement transactions, {deals} expiry deals", keys.len());
    assert_eq!(deals as usize, pending + 2, "one expiry deal per position, the cent account's two included");
    let runs: Vec<(i64, String, i32, i32, i32)> = sqlx::query_as("SELECT tenant_id, expiry_key, run, positions, accounts FROM option_settlement_runs ORDER BY expiry_key").fetch_all(&pool).await.unwrap();
    eprintln!("[settle] runs {runs:?}");
    // the run record covers the whole run, the cent account's settlement before the crash included
    for (_, k, run, positions, _) in &runs {
        let keys: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key LIKE $1").bind(format!("settle:{k}:{run}:%")).fetch_one(&pool).await.unwrap();
        assert!(*positions as i64 >= keys, "{k} run {run}: recorded {positions} positions, {keys} settlement transactions");
    }
    // no position open past its cut
    for l in [LUSD, LUSC, DUSD, DUSC] {
        assert!(opt_positions(&hub, l).await.is_empty());
    }
    // money: balanced per currency; the expiry's clearing nets to 0 (book positions settle through it, docs §9)
    ledger_balanced(&pool).await;
    let deu = exp["EURUSD"].1;
    let clearing = format!("house:options_clearing.EURUSD.{}:USD", deu.format("%Y%m%d"));
    let clr = ledger_sum(&pool, &clearing).await;
    let sett: Vec<(String, D)> = sqlx::query_as("SELECT account_code, sum(amount) FROM ledger_postings WHERE account_code LIKE 'house:options_%' OR account_code LIKE 'house:fx:%' GROUP BY 1 ORDER BY 1").fetch_all(&pool).await.unwrap();
    eprintln!("[settle] house accounts {sett:?}");
    assert_eq!(clr, D::ZERO, "{clearing} nets to 0 after settlement (and the rounding sweep)");
    // the house B-book: the settlement account carries exactly the house side of the live payouts
    let settled_against = |code: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<D>>("SELECT sum(p.amount) FROM ledger_postings p JOIN ledger_txns t ON t.id = p.txn_id WHERE p.account_code = $1 AND t.idempotency_key LIKE 'settle:%'")
                .bind(code)
                .fetch_one(&pool)
                .await
                .unwrap()
                .unwrap_or_default()
        }
    };
    let house_usd = settled_against("house:options_settlement:USD").await;
    let house_usc = settled_against("house:options_settlement:USC").await;
    assert_eq!((house_usd, house_usc), (-expect[&LUSD], -expect[&LUSC]), "only the house venue settles against options_settlement");
    // held for the re-run window, released one hour later
    let mut holds = json!({});
    for l in [LUSD, LUSC, DUSD, DUSC] {
        let v = api::terminal::account_view(&st, l).await;
        holds[l.to_string()] = json!({"settlementHold": v["settlementHold"], "withdrawable": v["withdrawable"], "balance": v["balance"]});
    }
    eprintln!("[settle] holds now {holds}");
    for l in [LUSD, LUSC, DUSD, DUSC] {
        let paid: D = sqlx::query_scalar::<_, Option<D>>("SELECT sum(p.amount) FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id WHERE t.idempotency_key LIKE 'settle:%' AND p.account_code = $1 AND p.amount > 0")
            .bind(format!("acct:{l}:balance"))
            .fetch_one(&pool)
            .await
            .unwrap()
            .unwrap_or_default();
        assert_eq!(dec(&holds[l.to_string()]["settlementHold"]), r2(paid), "{l}: the positive payouts are held (account currency)");
    }
    hub.shared.clock.set(Some(now() + chrono::Duration::seconds(trading::options::settle::RERUN_WINDOW_SECS + 5)));
    for l in [LUSD, LUSC, DUSD, DUSC] {
        let v = api::terminal::account_view(&st, l).await;
        assert_eq!(dec(&v["settlementHold"]), D::ZERO, "{l}: released after 1 h");
    }
    hub.shared.clock.set(None);

    // ---- a re-run at a corrected fixing (within 1 h): every balance moves by exactly the difference ----
    let mut fix2 = BTreeMap::new();
    let mut rr_out = json!({});
    for (s, (k, _)) in &exp {
        let id = records[*s]["expiryId"].as_i64().or_else(|| records[*s]["id"].as_i64()).expect("expiry id in the fixing record");
        let bump = if *s == "EURUSD" { D::new(5, 4) } else { D::from(2) };
        let new = fix[s].0 + bump;
        let r = http("POST", &format!("{}/v1/admin/options/expiries/{id}/refix", env("OPTIONS_URL", "http://127.0.0.1:8104")), Some(json!({"price": new.to_string().parse::<f64>().unwrap(), "reason": "expiry e2e: re-run at a corrected fixing"}))).await;
        assert_eq!(r["expiry"]["fixingRun"].as_i64().or_else(|| r["expiry"]["run"].as_i64()), Some(fix[s].1 as i64 + 1), "re-fixed: {r}");
        fix2.insert(*s, (new, fix[s].1 + 1));
        let _ = e.options.poll(Some(&pool)).await;
        let b0: BTreeMap<i64, D> = futures_util::future::join_all([LUSD, LUSC, DUSD, DUSC].map(|l| {
            let hub = hub.clone();
            async move { (l, balance(&hub, l).await) }
        }))
        .await
        .into_iter()
        .collect();
        let Json(rr) = api::options::rerun(State(st.clone()), staff(&st).await, Path(k.clone()), body(json!({"reason": "expiry e2e: corrected fixing"}))).await.unwrap_or_else(|x| panic!("rerun {k}: {x:?}"));
        eprintln!("[settle] re-run {k}: {rr}");
        // expected difference per account, from the expiry deals' snapshots (the settled positions)
        let rows: Vec<(i64, sqlx::types::Json<Value>)> = sqlx::query_as("SELECT login, data FROM deals WHERE reason = 'expiry' AND reversed AND option->'terms'->>'underlying' = $1").bind(*s).fetch_all(&pool).await.unwrap();
        let mut diff: BTreeMap<i64, D> = BTreeMap::new();
        for (l, dd) in &rows {
            let p = dd.0["snapshot"].clone();
            *diff.entry(*l).or_default() += payout(&p, new, factor(*l)) - payout(&p, fix[s].0, factor(*l));
        }
        for (l, x) in &diff {
            let got = balance(&hub, *l).await - b0[l];
            assert_eq!(got, *x, "re-run of {k}: {l} moves by the difference");
        }
        rr_out[*s] = json!({"fixing": new.to_string(), "run": fix[s].1 + 1, "positions": rr["positions"], "cashChange": rr["cashChange"], "diff": diff.iter().map(|(l, x)| (l.to_string(), x.to_string())).collect::<BTreeMap<_, _>>()});
        let again = api::options::rerun(State(st.clone()), staff(&st).await, Path(k.clone()), body(json!({"reason": "again"}))).await;
        assert!(again.is_err(), "a re-run applies once");
    }
    ledger_balanced(&pool).await;
    assert_eq!(ledger_sum(&pool, &clearing).await, D::ZERO, "the clearing still nets to 0 after the re-run");

    // ---- replay = live; the book journal replays identically ----
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &replayed).await.unwrap().is_empty());
    for l in [LUSD, LUSC, DUSD, DUSC] {
        assert_eq!(serde_json::to_value(&replayed[&l]).unwrap(), state(&hub, l).await, "replay diverged for {l}");
    }
    let key = trading::book::BookKey::new(1, AccountKind::Demo, "EURUSD");
    let entries = trading::book::journal::entries(&pool, &key, 1).await.unwrap();
    let rb = trading::book::journal::replay(trading::book::types::UnderlyingBooks::new(key.clone()), &entries).expect("journal replay identical");
    let live_fp = hub.shared.books.handle(&key).unwrap().read(Box::new(|b| trading::book::journal::fingerprint(b))).await.unwrap();
    assert_eq!(trading::book::journal::fingerprint(&rb), live_fp);
    let mut fin = json!({"fixings": fix.iter().map(|(s, (f, r))| (s.to_string(), json!({"fixing": f.to_string(), "run": r}))).collect::<BTreeMap<_, _>>(), "records": records, "expected": expect.iter().map(|(l, x)| (l.to_string(), x.to_string())).collect::<BTreeMap<_, _>>(),
        "partial": part, "rerun": rr_out, "journal": entries.len(), "settledAt": now()});
    for l in [LUSD, LUSC, DUSD, DUSC] {
        fin["balances"][l.to_string()] = json!(balance(&hub, l).await.to_string());
    }
    save(&pool, "settle", &fin).await;
    eprintln!("[settle] OK {}", serde_json::to_string_pretty(&fin).unwrap());
}

/* ------------------------------------------------------------------ */
/* 4. hold (≥ 1 h after settle, real clock)                            */
/* ------------------------------------------------------------------ */

async fn hold(url: &str) {
    let e = boot(url, false).await;
    let s = load(&e.pool, "settle").await;
    let at: DateTime<Utc> = serde_json::from_value(s["settledAt"].clone()).unwrap();
    let last: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT max(t.created_at) FROM ledger_txns t WHERE t.idempotency_key LIKE 'settle%'").fetch_one(&e.pool).await.unwrap();
    let last = last.unwrap_or(at);
    assert!(now() > last + chrono::Duration::seconds(trading::state::SETTLEMENT_HOLD_SECS), "run `hold` after {}", last + chrono::Duration::seconds(trading::state::SETTLEMENT_HOLD_SECS));
    for l in [LUSD, LUSC, DUSD, DUSC] {
        let v = api::terminal::account_view(&e.st, l).await;
        eprintln!("[hold] {l}: balance {} withdrawable {} settlementHold {}", v["balance"], v["withdrawable"], v["settlementHold"]);
        assert_eq!(dec(&v["settlementHold"]), D::ZERO, "{l}: the settlement hold released after 1 h (real clock)");
    }
    eprintln!("[hold] OK");
}

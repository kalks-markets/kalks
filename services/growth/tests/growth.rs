//! End-to-end growth tests against a throw-away database `kalks_growth_test_<pid>_<n>` on the local PostgreSQL
//! (GROWTH_TEST_DATABASE_URL, default :5433). Skipped when PostgreSQL is unreachable. The trading engine and the
//! wallet are one mock HTTP server, so engine legs, idempotency keys and wallet retries are exercised for real.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use growth::bonus::{self, ClaimOpts};
use growth::calc::Segment;
use growth::clients;
use growth::config::Config;
use growth::deals::{self, AccountFacts, DealIn, Programme};
use growth::money::{D, dec};
use growth::state::AppState;
use growth::profiles::Profile;
use growth::{contests, db, loyalty, payouts, promos, shares};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static N: AtomicUsize = AtomicUsize::new(0);
static DEAL: AtomicI64 = AtomicI64::new(2_000_001);

// ---------------------------------------------------------------- mock engine + wallet

#[derive(Default)]
struct Mock {
    accounts: HashMap<i64, Value>,
    ledgers: HashMap<i64, Vec<Value>>,
    /// (login, type, amount, key)
    postings: Vec<(i64, String, D, String)>,
    keys: HashSet<String>,
    /// wallet transfers booked: (key, user, amount, kind)
    wallet: Vec<(String, i64, D, String)>,
    wallet_fail_next: usize,
    /// gateway suitability: clients who accepted the options intro
    options_ok: HashSet<i64>,
    /// dealing desk open positions per login (symbol, side, open time)
    positions: HashMap<i64, Vec<Value>>,
    /// client history deals per login (engine deal_json)
    history: HashMap<i64, Vec<Value>>,
}

type M = Arc<Mutex<Mock>>;

async fn m_account(State(m): State<M>, Path(login): Path<i64>) -> (axum::http::StatusCode, Json<Value>) {
    match m.lock().unwrap().accounts.get(&login) {
        Some(a) => (axum::http::StatusCode::OK, Json(json!({"account": a}))),
        None => (axum::http::StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found"}}))),
    }
}

async fn m_accounts(State(m): State<M>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let user: i64 = q.get("user_id").and_then(|u| u.parse().ok()).unwrap_or(0);
    let kind = q.get("type").cloned();
    let g = m.lock().unwrap();
    let items: Vec<Value> = g.accounts.values().filter(|a| a["userId"] == user && kind.as_ref().is_none_or(|k| a["type"] == k.as_str())).cloned().collect();
    Json(json!({"items": items, "total": items.len()}))
}

async fn m_balance(State(m): State<M>, Path(login): Path<i64>, Json(b): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let mut g = m.lock().unwrap();
    let key = b["idempotencyKey"].as_str().unwrap_or("").to_string();
    if !g.keys.insert(key.clone()) {
        return (axum::http::StatusCode::CONFLICT, Json(json!({"error": {"code": "duplicate_idempotency_key", "message": "already booked"}})));
    }
    let amount = D::from_str(b["amount"].as_str().unwrap()).unwrap();
    let kind = b["type"].as_str().unwrap().to_string();
    assert!(b["reasonCode"].as_str().unwrap().starts_with("GRW-"), "every growth posting carries a GRW reason code");
    let acc = g.accounts.get_mut(&login).expect("account");
    let field = match kind.as_str() {
        "bonus" => "bonus",
        "credit" => "credit",
        _ => "balance",
    };
    let cur = D::from_str(&acc[field].to_string()).unwrap();
    acc[field] = json!((cur + amount).to_string().parse::<f64>().unwrap());
    g.postings.push((login, kind, amount, key));
    (axum::http::StatusCode::OK, Json(json!({"data": {"txn": 1}})))
}

async fn m_ledger(State(m): State<M>, Path(login): Path<i64>) -> Json<Value> {
    Json(json!({"items": m.lock().unwrap().ledgers.get(&login).cloned().unwrap_or_default()}))
}

async fn m_wallet(State(m): State<M>, Json(b): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let mut g = m.lock().unwrap();
    if g.wallet_fail_next > 0 {
        g.wallet_fail_next -= 1;
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": {"code": "unavailable"}})));
    }
    let key = b["idempotency_key"].as_str().unwrap().to_string();
    let replay = g.wallet.iter().any(|w| w.0 == key);
    if !replay {
        g.wallet.push((key, b["user_id"].as_i64().unwrap(), D::from_str(b["amount"].as_str().unwrap()).unwrap(), b["kind"].as_str().unwrap().to_string()));
    }
    (axum::http::StatusCode::OK, Json(json!({"status": "completed", "txn_id": g.wallet.len(), "replayed": replay})))
}

async fn m_suitability(State(m): State<M>, Path(user): Path<i64>) -> Json<Value> {
    Json(json!({"userId": user, "product": "options", "eligible": m.lock().unwrap().options_ok.contains(&user)}))
}

async fn m_positions(State(m): State<M>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let login: i64 = q.get("login").and_then(|l| l.parse().ok()).unwrap_or(0);
    Json(json!(m.lock().unwrap().positions.get(&login).cloned().unwrap_or_default()))
}

async fn m_history(State(m): State<M>, Path(login): Path<i64>) -> Json<Value> {
    Json(json!({"deals": m.lock().unwrap().history.get(&login).cloned().unwrap_or_default()}))
}

async fn mock() -> (String, M) {
    let m: M = Arc::new(Mutex::new(Mock::default()));
    let app = Router::new()
        .route("/v1/admin/accounts", get(m_accounts))
        .route("/v1/admin/accounts/{login}", get(m_account))
        .route("/v1/admin/accounts/{login}/balance", post(m_balance))
        .route("/v1/accounts/{login}/ledger", get(m_ledger))
        .route("/v1/accounts/{login}/history", get(m_history))
        .route("/v1/dealing/positions", get(m_positions))
        .route("/v1/internal/suitability/{user}", get(m_suitability))
        .route("/v1/wallets/transfers", post(m_wallet))
        .with_state(m.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (url, m)
}

fn account(login: i64, user: i64, kind: &str) -> Value {
    json!({"login": login, "userId": user, "type": kind, "group": "standard", "cent": false, "currency": "USD", "status": "active",
           "balance": 1000.0, "credit": 0.0, "bonus": 0.0, "equity": 1000.0})
}

// ---------------------------------------------------------------- env

struct Env {
    st: AppState,
    m: M,
    admin: PgConnectOptions,
    name: String,
}

async fn env() -> Option<Env> {
    let base = std::env::var("GROWTH_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let admin = PgConnectOptions::from_str(&base).ok()?.database("postgres");
    if admin.connect().await.is_err() {
        eprintln!("skipping growth DB tests: no PostgreSQL at {base}");
        return None;
    }
    let name = format!("kalks_growth_test_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
    let url = admin.clone().database(&name).to_url_lossy().to_string();
    let pool = db::connect(&url).await.expect("create + migrate");
    db::seed(&pool, "kalks").await.unwrap();
    let (mock_url, m) = mock().await;
    let mut cfg = Config::for_tests(&url);
    cfg.trading_url = mock_url.clone();
    cfg.gateway_url = mock_url.clone();
    cfg.wallet_url = mock_url;
    Some(Env { st: AppState::new(pool, cfg), m, admin, name })
}

impl Env {
    async fn drop(self) {
        self.st.pool.close().await;
        if let Ok(mut c) = self.admin.connect().await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))).execute(&mut c).await;
        }
    }

    fn add_account(&self, login: i64, user: i64, kind: &str) {
        self.m.lock().unwrap().accounts.insert(login, account(login, user, kind));
    }

    async fn deal(&self, login: i64, user: i64, kind: &str, symbol: &str, lots: &str, profit: &str, opened_ago_min: i64) -> Option<deals::Produced> {
        let p = Programme::load(&self.st, "kalks").await.unwrap();
        let now = Utc::now();
        let d = DealIn {
            deal_id: DEAL.fetch_add(1, Ordering::SeqCst),
            tenant: "kalks".into(),
            login,
            user_id: user,
            symbol: symbol.into(),
            side: "buy".into(),
            volume: dec(lots),
            profit: dec(profit),
            open_time: now - Duration::minutes(opened_ago_min),
            close_time: now,
            kind: "close".into(),
            reversed: false,
            account: AccountFacts { kind: kind.into(), group: "standard".into(), cent: false },
            option: false,
            premium: None,
            fill_id: None,
        };
        deals::ingest(&self.st, &p, &d).await.unwrap()
    }

    /// A Kalks FX Options closing deal (volume = contracts).
    async fn option_deal(&self, login: i64, user: i64, contracts: &str, profit: &str, opened_ago_min: i64) -> (i64, Option<deals::Produced>) {
        let p = Programme::load(&self.st, "kalks").await.unwrap();
        let now = Utc::now();
        let id = DEAL.fetch_add(1, Ordering::SeqCst);
        let d = DealIn {
            deal_id: id,
            tenant: "kalks".into(),
            login,
            user_id: user,
            symbol: "EURUSD-20261009-1.1650-C".into(),
            side: "buy".into(),
            volume: dec(contracts),
            profit: dec(profit),
            open_time: now - Duration::minutes(opened_ago_min),
            close_time: now,
            kind: "expiry".into(),
            reversed: false,
            account: AccountFacts { kind: "live".into(), group: "standard".into(), cent: false },
            option: true,
            premium: Some(dec("120")),
            fill_id: None,
        };
        (id, deals::ingest(&self.st, &p, &d).await.unwrap())
    }

    /// An option exit: `side` = the position's side, opened / closed `open_ago` / `close_ago` minutes ago,
    /// `premium` = its opening premium (USD), `fill` = the order-book fill (None = house-priced).
    #[allow(clippy::too_many_arguments)]
    async fn opt(&self, login: i64, user: i64, kind: &str, symbol: &str, side: &str, contracts: &str, profit: &str, premium: &str, open_ago: i64, close_ago: i64, fill: Option<&str>) -> (i64, deals::Produced) {
        let p = Programme::load(&self.st, "kalks").await.unwrap();
        let now = Utc::now();
        let id = DEAL.fetch_add(1, Ordering::SeqCst);
        let d = DealIn {
            deal_id: id,
            tenant: "kalks".into(),
            login,
            user_id: user,
            symbol: symbol.into(),
            side: side.into(),
            volume: dec(contracts),
            profit: dec(profit),
            open_time: now - Duration::minutes(open_ago),
            close_time: now - Duration::minutes(close_ago),
            kind: "close".into(),
            reversed: false,
            account: AccountFacts { kind: kind.into(), group: "standard".into(), cent: false },
            option: true,
            premium: Some(dec(premium)),
            fill_id: fill.map(str::to_string),
        };
        (id, deals::ingest(&self.st, &p, &d).await.unwrap().unwrap())
    }

    async fn one<T: for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres> + Send + Unpin>(&self, sql: &str) -> T {
        sqlx::query_scalar::<_, T>(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&self.st.pool).await.unwrap()
    }
}

// ---------------------------------------------------------------- tests

#[tokio::test]
async fn deals_earn_points_and_cashback_once() {
    let Some(e) = env().await else { return };
    sqlx::query("INSERT INTO cashback_programmes (tenant, name, asset_classes, usd_per_lot, max_per_month) VALUES ('kalks', 'FX cashback', '{forex}', 2.5, 6)").execute(&e.st.pool).await.unwrap();

    // 2 lots EURUSD on live, held 5 min: forex 10 pts/lot × Bronze 1.0 = 20 pts; cashback 2 × 2.5 = 5
    let p = e.deal(10000001, 42, "live", "EURUSD", "2", "35", 5).await.unwrap();
    assert_eq!(p.points, 20);
    assert_eq!(p.cashback, dec("5"));
    // a deal id seen before is ignored
    let dup = 9_000_001;
    let prog = Programme::load(&e.st, "kalks").await.unwrap();
    let again = DealIn {
        deal_id: dup,
        tenant: "kalks".into(),
        login: 10000001,
        user_id: 42,
        symbol: "EURUSD".into(),
        side: "buy".into(),
        volume: dec("2"),
        profit: D::ZERO,
        open_time: Utc::now() - Duration::minutes(5),
        close_time: Utc::now(),
        kind: "close".into(),
        reversed: false,
        account: AccountFacts { kind: "live".into(), group: "standard".into(), cent: false },
        option: false,
        premium: None,
        fill_id: None,
    };
    assert_eq!(deals::ingest(&e.st, &prog, &again).await.unwrap().unwrap().points, 20);
    assert!(deals::ingest(&e.st, &prog, &again).await.unwrap().is_none());
    // monthly cap 6: 5 + 1 reached, the next 2 lots accrue nothing (points still earn)
    let p = e.deal(10000001, 42, "live", "GBPUSD", "2", "0", 5).await.unwrap();
    assert_eq!((p.points, p.cashback), (20, D::ZERO));
    // scalps (< 60 s) earn no points; demo earns no points or cashback
    let p = e.deal(10000001, 42, "live", "XAUUSD", "1", "0", 0).await.unwrap();
    assert_eq!(p.points, 0);
    let p = e.deal(50000001, 42, "demo", "EURUSD", "5", "0", 10).await.unwrap();
    assert_eq!((p.points, p.cashback), (0, D::ZERO));
    // metals rule (15/lot) on 1.5 lots
    let p = e.deal(10000001, 42, "live", "XAUUSD", "1.5", "0", 10).await.unwrap();
    assert_eq!(p.points, 22);
    assert_eq!(loyalty::balance(&e.st.pool, "kalks", 42).await.unwrap(), 20 + 20 + 20 + 22);

    // cashback payout: one wallet transfer (refund) for the client, idempotent
    let (n, total) = payouts::cashback_batch(&e.st, "kalks", true).await.unwrap();
    assert_eq!((n, total), (1, dec("6")), "cap 6 reached: 5 + 1 + 0");
    e.m.lock().unwrap().wallet_fail_next = 1;
    assert_eq!(payouts::cashback_tick(&e.st).await.unwrap(), 0, "wallet down: retried later");
    sqlx::query("UPDATE cashback_payouts SET next_try_at = now()").execute(&e.st.pool).await.unwrap();
    assert_eq!(payouts::cashback_tick(&e.st).await.unwrap(), 1);
    let w = e.m.lock().unwrap().wallet.clone();
    assert_eq!(w.len(), 1);
    assert_eq!((w[0].1, w[0].2, w[0].3.as_str()), (42, dec("6"), "refund"));
    assert_eq!(e.one::<i64>("SELECT count(*) FROM cashback_accruals WHERE status = 'paid'").await, 2, "the capped-out deal accrued nothing");

    // reversal: points reversed, nothing else double counted
    deals::reverse_deal(&e.st, dup).await.unwrap();
    assert_eq!(loyalty::balance(&e.st.pool, "kalks", 42).await.unwrap(), 62);
    e.drop().await;
}

#[tokio::test]
async fn bonus_grant_releases_per_lot_and_is_removed_on_withdrawal() {
    let Some(e) = env().await else { return };
    let login = 10000011;
    e.add_account(login, 7, "live");
    let cid: i64 = sqlx::query_scalar("INSERT INTO bonus_campaigns (tenant, name, kind, fixed_amount, release_per_lot, expiry_days, status) VALUES ('kalks','Welcome $100','fixed',100,10,30,'active') RETURNING id")
        .fetch_one(&e.st.pool)
        .await
        .unwrap();
    let acc = clients::account(&e.st, "kalks", login).await.unwrap().unwrap();
    let mut tx = e.st.pool.begin().await.unwrap();
    let opts = ClaimOpts { source: "claim", require_public: true, amount_override: None, note: None, skip_limits: false };
    let gid = bonus::claim_in(&mut tx, "kalks", 7, cid, Some(&acc), &Segment::default(), &opts).await.unwrap();
    tx.commit().await.unwrap();
    // a second claim is refused (per-user limit 1)
    let mut tx = e.st.pool.begin().await.unwrap();
    assert!(bonus::claim_in(&mut tx, "kalks", 7, cid, Some(&acc), &Segment::default(), &opts).await.is_err());
    drop(tx);

    assert_eq!(bonus::post_tick(&e.st).await.unwrap(), 1);
    assert_eq!(e.one::<String>(&format!("SELECT status FROM bonus_grants WHERE id = {gid}")).await, "active");
    assert_eq!(e.m.lock().unwrap().accounts[&login]["bonus"], json!(100.0));
    sqlx::query("UPDATE bonus_grants SET granted_at = now() - interval '1 hour'").execute(&e.st.pool).await.unwrap();

    // 3 lots release $30: bonus −30, balance +30
    let p = e.deal(login, 7, "live", "EURUSD", "3", "0", 10).await.unwrap();
    assert_eq!(p.bonus_released, dec("30"));
    assert_eq!(bonus::post_tick(&e.st).await.unwrap(), 1);
    {
        let g = e.m.lock().unwrap();
        assert_eq!(g.accounts[&login]["bonus"], json!(70.0));
        assert_eq!(g.accounts[&login]["balance"], json!(1030.0));
        assert_eq!(g.postings.len(), 3);
    }
    // nothing pending: no new postings
    assert_eq!(bonus::post_tick(&e.st).await.unwrap(), 0);
    assert_eq!(e.m.lock().unwrap().postings.len(), 3);

    // withdrawal while the bonus is active: the remaining $70 is removed
    e.m.lock().unwrap().ledgers.insert(login, vec![json!({"txn": 9, "kind": "transfer_out", "subLedger": "balance", "amount": -50.0, "currency": "USD", "at": Utc::now().to_rfc3339()})]);
    assert_eq!(bonus::lifecycle_tick(&e.st).await.unwrap(), 1);
    assert_eq!(e.one::<String>(&format!("SELECT status FROM bonus_grants WHERE id = {gid}")).await, "forfeited");
    bonus::post_tick(&e.st).await.unwrap();
    assert_eq!(e.m.lock().unwrap().accounts[&login]["bonus"], json!(0.0));
    // later lots release nothing
    let p = e.deal(login, 7, "live", "EURUSD", "3", "0", 10).await.unwrap();
    assert_eq!(p.bonus_released, D::ZERO);
    e.drop().await;
}

#[tokio::test]
async fn deposit_bonus_completes_after_enough_lots() {
    let Some(e) = env().await else { return };
    let login = 10000021;
    e.add_account(login, 8, "live");
    let cid: i64 = sqlx::query_scalar(
        "INSERT INTO bonus_campaigns (tenant, name, kind, pct, cap, min_deposit, release_per_lot, expiry_days, status) VALUES ('kalks','50% deposit','deposit',50,120,100,20,30,'active') RETURNING id",
    )
    .fetch_one(&e.st.pool)
    .await
    .unwrap();
    let mut tx = e.st.pool.begin().await.unwrap();
    let opts = ClaimOpts { source: "claim", require_public: true, amount_override: None, note: None, skip_limits: false };
    let gid = bonus::claim_in(&mut tx, "kalks", 8, cid, None, &Segment::default(), &opts).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(bonus::deposit_tick(&e.st).await.unwrap(), 0, "no deposit yet");
    // a $300 deposit → 50% = 150, capped at 120
    e.m.lock().unwrap().ledgers.insert(login, vec![json!({"txn": 5, "kind": "transfer_in", "subLedger": "balance", "amount": 300.0, "currency": "USD", "at": (Utc::now() + Duration::seconds(1)).to_rfc3339()})]);
    sqlx::query("UPDATE bonus_grants SET ledger_checked = NULL").execute(&e.st.pool).await.unwrap();
    assert_eq!(bonus::deposit_tick(&e.st).await.unwrap(), 1);
    assert_eq!(e.one::<D>(&format!("SELECT amount FROM bonus_grants WHERE id = {gid}")).await, dec("120"));
    bonus::post_tick(&e.st).await.unwrap();
    sqlx::query("UPDATE bonus_grants SET granted_at = now() - interval '1 hour'").execute(&e.st.pool).await.unwrap();
    // 120 / 20 per lot = 6 lots: 4 + 4 → the second deal releases only the last 40
    assert_eq!(e.deal(login, 8, "live", "EURUSD", "4", "0", 10).await.unwrap().bonus_released, dec("80"));
    assert_eq!(e.deal(login, 8, "live", "EURUSD", "4", "0", 10).await.unwrap().bonus_released, dec("40"));
    assert_eq!(e.one::<String>(&format!("SELECT status FROM bonus_grants WHERE id = {gid}")).await, "completed");
    bonus::post_tick(&e.st).await.unwrap();
    let g = e.m.lock().unwrap();
    assert_eq!(g.accounts[&login]["bonus"], json!(0.0));
    assert_eq!(g.accounts[&login]["balance"], json!(1120.0));
    drop(g);
    e.drop().await;
}

#[tokio::test]
async fn promo_limits_hold_under_concurrency() {
    let Some(e) = env().await else { return };
    sqlx::query("INSERT INTO promo_codes (tenant, code, kind, points, max_uses, per_user_limit) VALUES ('kalks','FIVE','points',500,5,1)").execute(&e.st.pool).await.unwrap();
    let mut tasks = vec![];
    for u in 0..20 {
        let st = e.st.clone();
        tasks.push(tokio::spawn(async move { promos::redeem(&st, "kalks", 1000 + u, "five", None, &Segment::default()).await.is_ok() }));
    }
    let mut ok = 0;
    for t in tasks {
        if t.await.unwrap() {
            ok += 1;
        }
    }
    assert_eq!(ok, 5);
    assert_eq!(e.one::<i32>("SELECT uses FROM promo_codes WHERE code = 'FIVE'").await, 5);
    assert_eq!(e.one::<i64>("SELECT count(*) FROM promo_redemptions WHERE status = 'blocked'").await, 15);
    assert_eq!(e.one::<i64>("SELECT sum(points)::bigint FROM points_ledger WHERE kind = 'promo'").await, 2500);

    // per-client limit, window and segment rules
    sqlx::query("INSERT INTO promo_codes (tenant, code, kind, points, per_user_limit, countries, kyc_required) VALUES ('kalks','INONLY','points',100,1,'{IN}',true)").execute(&e.st.pool).await.unwrap();
    let verified_in = Segment { country: "IN".into(), kyc: "verified".into(), signed_up_at: None };
    assert!(promos::redeem(&e.st, "kalks", 1, "INONLY", None, &Segment { country: "AE".into(), ..verified_in.clone() }).await.is_err());
    assert!(promos::redeem(&e.st, "kalks", 1, "INONLY", None, &Segment { kyc: "pending".into(), ..verified_in.clone() }).await.is_err());
    assert!(promos::redeem(&e.st, "kalks", 1, "INONLY", None, &verified_in).await.is_ok());
    assert!(promos::redeem(&e.st, "kalks", 1, "inonly", None, &verified_in).await.is_err(), "once per client");
    assert!(promos::redeem(&e.st, "kalks", 1, "NOPE", None, &verified_in).await.is_err());
    e.drop().await;
}

#[tokio::test]
async fn contest_scores_ranks_and_pays() {
    let Some(e) = env().await else { return };
    let cid: i64 = sqlx::query_scalar(
        "INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring, min_trades, starting_balance, prizes)
         VALUES ('kalks','sprint','Sprint','demo', now() - interval '2 hours', now() + interval '1 hour','return_pct',2,10000,
                 '[{\"rankFrom\":1,\"rankTo\":1,\"amount\":300,\"payout\":\"wallet\"},{\"rankFrom\":2,\"rankTo\":3,\"amount\":50,\"payout\":\"wallet\"}]') RETURNING id",
    )
    .fetch_one(&e.st.pool)
    .await
    .unwrap();
    for (i, (login, user)) in [(50000101, 101), (50000102, 102), (50000103, 103)].iter().enumerate() {
        e.add_account(*login, *user, "demo");
        e.m.lock().unwrap().accounts.get_mut(login).unwrap()["equity"] = json!(1000.0 + i as f64 * 10.0);
        sqlx::query("INSERT INTO contest_entries (contest_id, tenant, user_id, login, display_name, start_equity, joined_at) VALUES ($1,'kalks',$2,$3,$4,10000, now() - make_interval(mins => $5))")
            .bind(cid)
            .bind(user)
            .bind(login)
            .bind(format!("Trader {user}"))
            .bind(30 - i as i32)
            .execute(&e.st.pool)
            .await
            .unwrap();
    }
    // 101: +500 over 2 trades (5%); 102: +900 over 1 trade (unqualified); 103: +200 over 3 trades (2%)
    e.deal(50000101, 101, "demo", "EURUSD", "1", "300", 30).await;
    e.deal(50000101, 101, "demo", "EURUSD", "1", "200", 30).await;
    e.deal(50000102, 102, "demo", "EURUSD", "5", "900", 30).await;
    for _ in 0..3 {
        e.deal(50000103, 103, "demo", "XAUUSD", "1", "66.67", 30).await;
    }
    // a deal opened before the contest start does not count
    e.deal(50000103, 103, "demo", "XAUUSD", "1", "5000", 600).await;
    contests::refresh(&e.st, cid).await.unwrap();
    let ranks: Vec<(i64, Option<i32>)> = sqlx::query_as("SELECT user_id, rank FROM contest_entries WHERE contest_id = $1 ORDER BY user_id").bind(cid).fetch_all(&e.st.pool).await.unwrap();
    assert_eq!(ranks, vec![(101, Some(1)), (102, Some(3)), (103, Some(2))]);
    assert_eq!(e.one::<D>("SELECT return_pct FROM contest_entries WHERE user_id = 101").await, dec("5"));

    let actor = growth::audit::Actor { id: "staff:1".into(), name: Some("T".into()) };
    assert!(contests::finalize(&e.st, cid, &actor).await.is_err(), "not ended yet");
    sqlx::query("UPDATE contests SET ends_at = now() - interval '1 second' WHERE id = $1").bind(cid).execute(&e.st.pool).await.unwrap();
    contests::finalize(&e.st, cid, &actor).await.unwrap();
    // 102 is unqualified: ranked 3 but wins nothing
    let prizes: Vec<(i64, Option<D>)> = sqlx::query_as("SELECT user_id, prize_amount FROM contest_entries WHERE contest_id = $1 ORDER BY user_id").bind(cid).fetch_all(&e.st.pool).await.unwrap();
    assert_eq!(prizes, vec![(101, Some(dec("300"))), (102, None), (103, Some(dec("50")))]);
    let r = contests::pay(&e.st, cid, &actor).await.unwrap();
    assert_eq!(r["paid"], 2);
    let paid: i64 = e.one("SELECT count(*) FROM contest_entries WHERE prize_status = 'paid'").await;
    assert_eq!(paid, 2);
    // paying again books nothing new
    contests::pay(&e.st, cid, &actor).await.unwrap();
    assert_eq!(e.m.lock().unwrap().wallet.len(), 2);
    e.drop().await;
}

#[tokio::test]
async fn redemption_pays_wallet_with_retry() {
    let Some(e) = env().await else { return };
    sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, description) VALUES ('kalks', 9, 'promo', 1500, 't', 'test')").execute(&e.st.pool).await.unwrap();
    let item: i64 = e.one("SELECT id FROM catalogue WHERE kind = 'cashback' ORDER BY cost_points LIMIT 1").await;
    e.m.lock().unwrap().wallet_fail_next = 1;
    let r = loyalty::redeem(&e.st, "kalks", 9, item, None).await.unwrap();
    assert_eq!(r["balance"], 500);
    assert!(loyalty::redeem(&e.st, "kalks", 9, item, None).await.is_err(), "not enough points");
    payouts::wallet_tick(&e.st).await.unwrap();
    assert_eq!(e.one::<String>("SELECT status FROM redemptions").await, "pending");
    sqlx::query("UPDATE wallet_credits SET next_try_at = now()").execute(&e.st.pool).await.unwrap();
    payouts::wallet_tick(&e.st).await.unwrap();
    assert_eq!(e.one::<String>("SELECT status FROM redemptions").await, "completed");
    let w = e.m.lock().unwrap().wallet.clone();
    assert_eq!(w.len(), 1);
    assert_eq!((w[0].2, w[0].3.as_str()), (dec("10"), "adjustment"));
    // voucher redemption + internal voucher use is idempotent on ref
    sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, description) VALUES ('kalks', 9, 'promo', 3000, 't2', 'test')").execute(&e.st.pool).await.unwrap();
    let disc: i64 = e.one("SELECT id FROM catalogue WHERE kind = 'fee_discount'").await;
    let r = loyalty::redeem(&e.st, "kalks", 9, disc, None).await.unwrap();
    assert!(r["redemption"]["voucherCode"].as_str().unwrap().starts_with("KV-"));
    e.drop().await;
}

#[tokio::test]
async fn option_deals_and_premiums_earn_no_rewards_and_never_move_contests() {
    let Some(e) = env().await else { return };
    let login = 10000031;
    e.add_account(login, 51, "live");
    // every reward is switched on for this client: FX cashback, a bonus releasing per lot, a running live contest
    sqlx::query("INSERT INTO cashback_programmes (tenant, name, usd_per_lot) VALUES ('kalks', 'All cashback', 3)").execute(&e.st.pool).await.unwrap();
    let cid: i64 = sqlx::query_scalar("INSERT INTO bonus_campaigns (tenant, name, kind, fixed_amount, release_per_lot, expiry_days, status) VALUES ('kalks','Welcome $100','fixed',100,10,30,'active') RETURNING id")
        .fetch_one(&e.st.pool)
        .await
        .unwrap();
    let acc = clients::account(&e.st, "kalks", login).await.unwrap().unwrap();
    let mut tx = e.st.pool.begin().await.unwrap();
    let opts = ClaimOpts { source: "claim", require_public: true, amount_override: None, note: None, skip_limits: false };
    let gid = bonus::claim_in(&mut tx, "kalks", 51, cid, Some(&acc), &Segment::default(), &opts).await.unwrap();
    tx.commit().await.unwrap();
    bonus::post_tick(&e.st).await.unwrap();
    sqlx::query("UPDATE bonus_grants SET granted_at = now() - interval '1 hour'").execute(&e.st.pool).await.unwrap();
    let contest: i64 = sqlx::query_scalar(
        "INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring, min_trades) VALUES ('kalks','opt','Live sprint','live', now() - interval '2 hours', now() + interval '1 hour','profit',0) RETURNING id",
    )
    .fetch_one(&e.st.pool)
    .await
    .unwrap();
    let entry: i64 = sqlx::query_scalar("INSERT INTO contest_entries (contest_id, tenant, user_id, login, display_name, start_equity, joined_at) VALUES ($1,'kalks',51,$2,'T51',1000, now() - interval '90 minutes') RETURNING id")
        .bind(contest)
        .bind(login)
        .fetch_one(&e.st.pool)
        .await
        .unwrap();

    // 20 contracts settled with a big profit, held long enough: nothing at all
    let (oid, p) = e.option_deal(login, 51, "20", "450", 60).await;
    assert_eq!(p.unwrap(), deals::Produced::default(), "no points, cashback, bonus release or contest entry");
    assert_eq!(e.one::<i64>(&format!("SELECT count(*) FROM deals WHERE deal_id = {oid} AND instrument = 'option' AND lots = 0 AND asset_class = 'options'")).await, 1);
    assert_eq!(e.one::<i64>("SELECT count(*) FROM points_ledger WHERE user_id = 51").await, 0);
    assert_eq!(e.one::<i64>("SELECT count(*) FROM cashback_accruals WHERE user_id = 51").await, 0);
    assert_eq!(e.one::<i64>("SELECT count(*) FROM contest_trades").await, 0);
    assert_eq!(e.one::<D>(&format!("SELECT lots_traded FROM bonus_grants WHERE id = {gid}")).await, D::ZERO);
    assert_eq!(e.one::<D>(&format!("SELECT released FROM bonus_grants WHERE id = {gid}")).await, D::ZERO);
    // seen once (the poller overlap re-reads it), and its reversal (an options void) is harmless
    let prog = Programme::load(&e.st, "kalks").await.unwrap();
    let again = DealIn {
        deal_id: oid,
        tenant: "kalks".into(),
        login,
        user_id: 51,
        symbol: "EURUSD-20261009-1.1650-C".into(),
        side: "buy".into(),
        volume: dec("20"),
        profit: dec("450"),
        open_time: Utc::now() - Duration::minutes(60),
        close_time: Utc::now(),
        kind: "expiry".into(),
        reversed: false,
        account: AccountFacts { kind: "live".into(), group: "standard".into(), cent: false },
        option: false, // even without the flag, the series code gives it away
        premium: None,
        fill_id: None,
    };
    assert!(deals::ingest(&e.st, &prog, &again).await.unwrap().is_none());
    deals::reverse_deal(&e.st, oid).await.unwrap();
    assert_eq!(e.one::<i64>("SELECT count(*) FROM points_ledger WHERE user_id = 51").await, 0);
    // an unflagged deal with an option series code is still treated as an option
    let p = Programme::load(&e.st, "kalks").await.unwrap();
    let sneaky = DealIn { deal_id: DEAL.fetch_add(1, Ordering::SeqCst), ..again.clone() };
    assert_eq!(deals::ingest(&e.st, &p, &sneaky).await.unwrap().unwrap(), deals::Produced::default());

    // a CFD deal of the same client still earns everything (cashback 2 × 3, release 2 × 10, contest trade)
    let pr = e.deal(login, 51, "live", "EURUSD", "2", "35", 60).await.unwrap();
    assert_eq!((pr.cashback, pr.bonus_released, pr.contest_entries), (dec("6"), dec("20"), 1));
    assert!(pr.points > 0);

    // the contest score: the client holds a bought option worth 300 in equity (the premium already left the
    // balance) plus a CFD floating +10. Only the CFD part counts; premium / settlement postings are no balance change.
    {
        let mut g = e.m.lock().unwrap();
        let a = g.accounts.get_mut(&login).unwrap();
        // equity = balance 700 + bonus 100 + CFD floating 10 + options 300
        a["balance"] = json!(700.0);
        a["bonus"] = json!(100.0);
        a["equity"] = json!(1110.0);
        a["optionValue"] = json!(300.0);
        g.ledgers.insert(
            login,
            vec![
                json!({"txn": 41, "kind": "option_premium", "subLedger": "balance", "amount": -300.0, "currency": "USD", "at": Utc::now().to_rfc3339()}),
                json!({"txn": 42, "kind": "option_settlement", "subLedger": "balance", "amount": 450.0, "currency": "USD", "at": Utc::now().to_rfc3339()}),
            ],
        );
    }
    let acc = clients::account(&e.st, "kalks", login).await.unwrap().unwrap();
    assert_eq!((acc.floating_usd(), acc.equity_ex_options_usd()), (dec("10"), dec("810")));
    contests::refresh(&e.st, contest).await.unwrap();
    let (realised, floating, score, status): (D, D, D, String) = sqlx::query_as("SELECT realised, floating, score, status FROM contest_entries WHERE id = $1").bind(entry).fetch_one(&e.st.pool).await.unwrap();
    assert_eq!((realised, floating, score), (dec("35"), dec("10"), dec("45")), "the CFD trade only");
    assert_eq!(status, "active", "option premiums and settlements are not deposits or withdrawals");
    assert_eq!(e.one::<i64>(&format!("SELECT count(*) FROM contest_flags WHERE entry_id = {entry} AND kind = 'balance_change'")).await, 0);
    // nor do they forfeit the bonus like a withdrawal would
    sqlx::query("UPDATE bonus_grants SET ledger_checked = NULL").execute(&e.st.pool).await.unwrap();
    assert_eq!(bonus::lifecycle_tick(&e.st).await.unwrap(), 0);
    assert_eq!(e.one::<String>(&format!("SELECT status FROM bonus_grants WHERE id = {gid}")).await, "active");
    e.drop().await;
}

// ---------------------------------------------------------------- options contests and share cards (O36)

fn profile(first: &str) -> Profile {
    Profile { first_name: first.into(), last_name: "Tester".into(), country: "IN".into(), kyc: "verified".into(), referral_code: "KALKS42".into(), ..Default::default() }
}

#[tokio::test]
async fn options_contest_scores_realised_option_pnl_in_contracts_without_self_trades() {
    let Some(e) = env().await else { return };
    let (l1, l2, l3, prop, l5) = (10000061, 10000062, 10000063, 10000064, 10000065);
    for (login, user) in [(l1, 61), (l2, 61), (l3, 63), (prop, 64), (l5, 64)] {
        e.add_account(login, user, "live");
    }
    e.m.lock().unwrap().accounts.get_mut(&prop).unwrap()["group"] = json!("prop-10k");
    e.m.lock().unwrap().options_ok.extend([61, 64]);
    let cid: i64 = sqlx::query_scalar(
        "INSERT INTO contests (tenant, slug, name, kind, instrument, starts_at, ends_at, scoring, min_trades, min_premium, prizes)
         VALUES ('kalks','opt-live','Options sprint','live','options', now() - interval '2 hours', now() + interval '1 hour','profit',2,20,
                 '[{\"rankFrom\":1,\"rankTo\":1,\"amount\":200,\"payout\":\"wallet\"}]') RETURNING id",
    )
    .fetch_one(&e.st.pool)
    .await
    .unwrap();
    let cfd: i64 = sqlx::query_scalar("INSERT INTO contests (tenant, slug, name, kind, starts_at, ends_at, scoring) VALUES ('kalks','cfd-live','CFD sprint','live', now() - interval '2 hours', now() + interval '1 hour','profit') RETURNING id")
        .fetch_one(&e.st.pool)
        .await
        .unwrap();

    // eligibility: the options intro (gateway suitability) and an account that may trade options
    match contests::join(&e.st, "kalks", 63, cid, Some(l3), &profile("Nia")).await {
        Err(growth::error::ApiError::Conflict { code, .. }) => assert_eq!(code, "options_intro_required"),
        other => panic!("not eligible for options: {other:?}"),
    }
    match contests::join(&e.st, "kalks", 64, cid, Some(prop), &profile("Omar")).await {
        Err(growth::error::ApiError::Validation { field, .. }) => assert_eq!(field, "login", "prop accounts never trade options"),
        other => panic!("prop account: {other:?}"),
    }
    let j = contests::join(&e.st, "kalks", 64, cid, Some(l5), &profile("Omar")).await.unwrap();
    assert_eq!(j["entry"]["contracts"], json!(0.0));
    contests::join(&e.st, "kalks", 61, cid, Some(l1), &profile("Ravi")).await.unwrap();
    contests::join(&e.st, "kalks", 61, cfd, Some(l1), &profile("Ravi")).await.unwrap();
    // CFD contests stay open to everyone, options intro or not
    contests::join(&e.st, "kalks", 63, cfd, Some(l3), &profile("Nia")).await.unwrap();

    const S1: &str = "EURUSD-20261009-1.1650-C";
    const S3: &str = "USDJPY-20261009-150.00-C";
    const S5: &str = "EURUSD-20261016-1.1700-P";
    // counts: 5 contracts, +150 on a $100 premium
    let (_, p) = e.opt(l1, 61, "live", S1, "buy", "5", "150", "100", 60, 30, None).await;
    assert_eq!(p, deals::Produced { contest_entries: 1, ..Default::default() }, "the options contest only: no points, cashback, bonus or CFD contest");
    // a penny option: premium $10 < $20 minimum: its loss counts, its 50 contracts and the trade don't
    e.opt(l1, 61, "live", "GBPUSD-20261009-1.3400-P", "sell", "50", "-40", "10", 55, 25, None).await;
    // a hedge across the client's own accounts: long on the contest account, short on the other one at the same time
    e.opt(l1, 61, "live", S3, "buy", "2", "300", "80", 50, 20, None).await;
    let (_, other) = e.opt(l2, 61, "live", S3, "sell", "2", "-310", "90", 45, 10, None).await;
    assert_eq!(other.contest_entries, 0, "the other account isn't entered");
    // one order-book fill on both accounts
    e.opt(l1, 61, "live", "AUDUSD-20261009-0.6600-C", "buy", "3", "60", "45", 40, 18, Some("F-9")).await;
    e.opt(l2, 61, "live", "NZDUSD-20261009-0.5900-C", "sell", "3", "-60", "45", 40, 18, Some("F-9")).await;
    // the other leg is still open on the other account: caught once the contest ends
    e.opt(l1, 61, "live", S5, "buy", "4", "90", "50", 40, 15, None).await;
    e.m.lock().unwrap().positions.insert(l2, vec![json!({"login": l2.to_string(), "symbol": S5, "side": "sell", "volume": 4, "openTime": (Utc::now() - Duration::minutes(35)).to_rfc3339()})]);
    // a CFD trade on the same account: the CFD contest only
    let pr = e.deal(l1, 61, "live", "EURUSD", "1", "500", 30).await.unwrap();
    assert_eq!(pr.contest_entries, 1);
    // the other entrant: two trades above the minimum
    e.opt(l5, 64, "live", S1, "buy", "10", "20", "30", 70, 40, None).await;
    e.opt(l5, 64, "live", S3, "sell", "10", "30", "40", 70, 35, None).await;
    // an option trade opened before the start doesn't count
    e.opt(l5, 64, "live", S1, "buy", "10", "900", "30", 200, 30, None).await;

    contests::refresh(&e.st, cid).await.unwrap();
    contests::refresh(&e.st, cfd).await.unwrap();
    type Row = (D, D, D, i32, i32, i32, D, D, Option<i32>);
    let row = |user: i64, contest: i64| {
        let pool = e.st.pool.clone();
        async move {
            sqlx::query_as::<_, Row>("SELECT realised, contracts, lots, trades, self_trades, small_trades, score, return_pct, rank FROM contest_entries WHERE contest_id = $1 AND user_id = $2")
                .bind(contest)
                .bind(user)
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    // 61: +150 − 40 + 90 realised (the hedge and the shared fill left out), 5 + 4 contracts, 2 trades
    assert_eq!(row(61, cid).await, (dec("200"), dec("9"), D::ZERO, 2, 2, 1, dec("200"), dec("20"), Some(1)));
    assert_eq!(row(64, cid).await, (dec("50"), dec("20"), D::ZERO, 2, 0, 0, dec("50"), dec("5"), Some(2)));
    let excluded: Vec<(String, Option<String>)> = sqlx::query_as("SELECT d.symbol, t.excluded FROM contest_trades t JOIN deals d ON d.deal_id = t.deal_id JOIN contest_entries x ON x.id = t.entry_id WHERE t.contest_id = $1 AND x.user_id = 61 ORDER BY d.symbol")
        .bind(cid)
        .fetch_all(&e.st.pool)
        .await
        .unwrap();
    assert_eq!(
        excluded,
        vec![
            ("AUDUSD-20261009-0.6600-C".into(), Some("self_trade".into())),
            (S1.into(), None),
            (S5.into(), None),
            ("GBPUSD-20261009-1.3400-P".into(), Some("min_premium".into())),
            (S3.into(), Some("self_trade".into())),
        ]
    );
    assert_eq!(e.one::<i64>(&format!("SELECT count(*) FROM contest_flags WHERE contest_id = {cid} AND kind = 'self_trade' AND status = 'open'")).await, 1);
    // the CFD contest saw the CFD trade only; options never earned points
    assert_eq!((row(61, cfd).await.0, row(61, cfd).await.3), (dec("500"), 1));
    assert_eq!(e.one::<i64>("SELECT count(*) FROM points_ledger WHERE user_id = 61 AND deal_id IN (SELECT deal_id FROM deals WHERE instrument = 'option')").await, 0);
    // standings carry contracts and (for the client / staff) the left-out trades
    let (board, mine) = contests::leaderboard(&e.st, cid, Some(61), false, 100).await.unwrap();
    let me = mine.unwrap();
    assert_eq!((me["contracts"].as_f64(), me["selfTrades"].as_i64(), me["smallTrades"].as_i64()), (Some(9.0), Some(2), Some(1)));
    let them = board.iter().find(|s| s["me"] == json!(false)).unwrap();
    assert!(them["selfTrades"].is_null() && them["login"].is_null(), "other entrants' details stay private");

    // after the end the engine is asked for legs still open: the open short on the other account turns S5 into a
    // self-trade, so 61 falls below the 2-trade minimum and wins nothing
    sqlx::query("UPDATE contests SET ends_at = now() - interval '1 second' WHERE id = $1").bind(cid).execute(&e.st.pool).await.unwrap();
    contests::refresh(&e.st, cid).await.unwrap();
    assert_eq!(row(61, cid).await, (dec("110"), dec("5"), D::ZERO, 1, 3, 1, dec("110"), dec("11"), Some(2)));
    let actor = growth::audit::Actor { id: "staff:1".into(), name: Some("T".into()) };
    contests::finalize(&e.st, cid, &actor).await.unwrap();
    let prizes: Vec<(i64, Option<i32>, Option<D>)> = sqlx::query_as("SELECT user_id, rank, prize_amount FROM contest_entries WHERE contest_id = $1 ORDER BY user_id").bind(cid).fetch_all(&e.st.pool).await.unwrap();
    assert_eq!(prizes, vec![(61, Some(2), None), (64, Some(1), Some(dec("200")))]);
    e.drop().await;
}

#[tokio::test]
async fn options_share_card_shows_terms_premiums_and_no_balance() {
    let Some(e) = env().await else { return };
    let login = 10000071;
    e.add_account(login, 71, "live");
    let now = Utc::now();
    let deal = |id: i64, symbol: &str, side: &str, contracts: f64, price: f64, open: f64, profit: f64, commission: f64, reason: &str, option: Value| {
        json!({"id": id, "login": login, "symbol": symbol, "side": if side == "buy" { "sell" } else { "buy" }, "positionSide": side, "entry": "out",
               "volume": contracts, "price": price, "openPrice": open, "profit": profit, "swap": 0.0, "commission": commission, "reason": reason,
               "time": now.to_rfc3339(), "openTime": (now - Duration::hours(5)).to_rfc3339(), "instrument": "option", "option": option})
    };
    e.m.lock().unwrap().history.insert(
        login,
        vec![
            // bought 5 calls at 0.0024 (24 USD per contract of 10 000), sold at 0.0062 (62 USD per contract)
            deal(901, "EURUSD-20261009-1.1650-C", "buy", 5.0, 0.0062, 0.0024, 190.0, 3.5, "client",
                 json!({"series": "EURUSD-20261009-1.1650-C", "underlying": "EURUSD", "right": "call", "strike": 1.165, "expiry": "2026-10-09", "style": "vanilla", "cash": 310.0, "usdPerQuote": 1.0, "spot": 1.1712})),
            // sold 5 puts for 80 USD in total, expired worthless (no cash at expiry)
            deal(902, "USDJPY-20261002-150.00-P", "sell", 5.0, 0.0, 0.24, 80.0, 1.0, "expiry", json!({"cash": 0.0, "fixing": 151.2})),
        ],
    );
    let share = |deal_id: i64, show: bool| {
        let st = e.st.clone();
        async move {
            let req = shares::ShareReq { kind: "trade".into(), login, deal_id: Some(deal_id), from: None, to: None, show_amounts: show };
            shares::create(&st, "kalks", 71, &req, &profile("Lina")).await.unwrap()["share"].clone()
        }
    };
    let s = share(901, false).await;
    let d = &s["data"];
    let o = &d["option"];
    assert_eq!(d["instrument"], "option");
    assert_eq!((d["contracts"].as_f64(), d["lots"].is_null()), (Some(5.0), true), "contracts, never lots");
    assert_eq!((o["underlying"].as_str(), o["right"].as_str(), o["strike"].as_f64(), o["expiry"].as_str()), (Some("EURUSD"), Some("call"), Some(1.165), Some("2026-10-09")));
    assert_eq!((o["side"].as_str(), o["entryPremium"].as_f64(), o["exitPremium"].as_f64()), (Some("buy"), Some(24.0), Some(62.0)));
    assert_eq!((o["pnlPct"].as_f64(), d["movePct"].as_f64()), (Some(158.33), Some(158.33)));
    assert_eq!((o["breakeven"].as_f64(), o["settle"].as_f64(), o["reason"].as_str()), (Some(1.1674), Some(1.1712), Some("closed")));
    assert!(d["profit"].is_null(), "no money without showAmounts");
    assert_eq!(d["referralCode"], "KALKS42");
    let text = d.to_string();
    for k in ["balance", "equity", "\"login\""] {
        assert!(!text.contains(k), "the card never carries {k}");
    }
    // with amounts: the net P&L (after commission)
    assert_eq!(share(901, true).await["data"]["profit"].as_f64(), Some(186.5));
    // an expired short put, terms from the series code only
    let o = share(902, false).await["data"]["option"].clone();
    assert_eq!((o["underlying"].as_str(), o["right"].as_str(), o["strike"].as_f64(), o["expiry"].as_str()), (Some("USDJPY"), Some("put"), Some(150.0), Some("2026-10-02")));
    assert_eq!((o["side"].as_str(), o["entryPremium"].as_f64(), o["exitPremium"].as_f64(), o["pnlPct"].as_f64()), (Some("sell"), Some(16.0), Some(0.0), Some(100.0)));
    assert_eq!((o["reason"].as_str(), o["breakeven"].as_f64(), o["settle"].as_f64()), (Some("expired"), Some(149.76), Some(151.2)));
    // the public view hides the account
    let code = s["code"].as_str().unwrap().to_string();
    let r = sqlx::query("SELECT * FROM shares WHERE code = $1").bind(&code).fetch_one(&e.st.pool).await.unwrap();
    assert!(shares::share_json(&r, true)["login"].is_null());
    e.drop().await;
}

/// Module switches (gateway): while a broker has `rewards` off, the rewards routes answer 403 module_disabled before
/// any handler runs; banners and share cards stay; a broker with the module on (or a key the gateway doesn't send) is
/// unaffected. No database needed (the switch answers first).
#[tokio::test]
async fn module_switch_refuses_the_rewards_routes() {
    use axum::extract::Path;
    let gw = axum::Router::new().route(
        "/v1/internal/tenants/{slug}",
        axum::routing::get(|Path(slug): Path<String>| async move { Json(json!({"id": 2, "slug": slug, "modules": if slug == "qa-rewards-off" { json!({"rewards": false}) } else { json!({"prop": false}) }})) }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, gw).await.unwrap() });
    let pool = sqlx::postgres::PgPoolOptions::new().acquire_timeout(std::time::Duration::from_millis(300)).connect_lazy("postgres://postgres@127.0.0.1:1/unused").unwrap();
    let mut cfg = Config::for_tests("postgres://postgres@127.0.0.1:1/unused");
    cfg.gateway_url = gw_url;
    let st = AppState::new(pool, cfg);
    assert!(!growth::modules::on(&st, "qa-rewards-off", "rewards").await);
    assert!(growth::modules::on(&st, "qa-rewards-on", "rewards").await, "a key the gateway doesn't send is on");
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, growth::api::router(st)).await.unwrap() });
    let http = reqwest::Client::new();
    for path in ["/v1/growth/me/rewards", "/v1/growth/me/points", "/v1/growth/me/contests"] {
        let r = http.get(format!("{base}{path}")).header("x-kalks-tenant", "qa-rewards-off").header("x-kalks-user-id", "7").send().await.unwrap();
        assert_eq!(r.status().as_u16(), 403, "{path}");
        assert_eq!(r.json::<Value>().await.unwrap()["error"]["code"], "module_disabled");
    }
    let r = http.get(format!("{base}/v1/growth/me/rewards")).header("x-kalks-tenant", "qa-rewards-on").header("x-kalks-user-id", "7").send().await.unwrap();
    assert_ne!(r.status().as_u16(), 403);
    let r = http.get(format!("{base}/v1/growth/me/banners")).header("x-kalks-tenant", "qa-rewards-off").header("x-kalks-user-id", "7").send().await.unwrap();
    assert_ne!(r.status().as_u16(), 403, "banners stay");
}

//! Phase state machine end to end against a throw-away database (`kalks_prop_test_<pid>`) with an in-process
//! mock trading engine and mock wallet. Skipped when the local Postgres (127.0.0.1:5433) is unreachable.

use axum::extract::{Path, Query, State};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use prop::config::Config;
use prop::ops::{self, Svc};
use prop::{evaluator, plans, store};

type D = Decimal;
fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

#[derive(Default, Clone)]
struct Acct {
    user: i64,
    balance: D,
    floating: D,
    status: String,
    version: i64,
    positions: Vec<Value>,
    deals: Vec<Value>,
}

#[derive(Default)]
struct Mock {
    next_login: i64,
    accounts: HashMap<i64, Acct>,
    keys: Vec<String>,
    wallet: HashMap<i64, D>,
    wallet_keys: HashMap<String, Value>,
    bulk_closes: usize,
    notified: Vec<Value>,
}

type M = Arc<Mutex<Mock>>;

fn num(x: D) -> Value {
    json!(x.to_string().parse::<f64>().unwrap())
}

async fn open(State(m): State<M>, Json(b): Json<Value>) -> Json<Value> {
    let mut m = m.lock().unwrap();
    m.next_login += 1;
    let login = 10_000_000 + m.next_login;
    m.accounts.insert(login, Acct { user: b["userId"].as_i64().unwrap(), status: "active".into(), ..Default::default() });
    Json(json!({"account": {"login": login}, "credentials": {"login": login, "password": "Gen3rated", "investorPassword": "Inv3stor"}}))
}

async fn balance(State(m): State<M>, Path(login): Path<i64>, Json(b): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let mut m = m.lock().unwrap();
    let key = b["idempotencyKey"].as_str().unwrap().to_string();
    if m.keys.contains(&key) {
        return (axum::http::StatusCode::CONFLICT, Json(json!({"error": {"code": "duplicate_idempotency_key", "message": "dup"}})));
    }
    m.keys.push(key);
    let amt = prop::money::dec_of(&b["amount"]).unwrap();
    let a = m.accounts.get_mut(&login).unwrap();
    a.balance += amt;
    a.version += 1;
    (axum::http::StatusCode::OK, Json(json!({"data": {"balance": num(a.balance)}})))
}

async fn account(State(m): State<M>, Path(login): Path<i64>) -> Json<Value> {
    let m = m.lock().unwrap();
    let a = &m.accounts[&login];
    Json(json!({"account": {"login": login, "userId": a.user, "status": a.status, "group": "prop", "balance": num(a.balance), "equity": num(a.balance + a.floating), "version": a.version},
               "positions": a.positions, "orders": []}))
}

async fn status(State(m): State<M>, Path(login): Path<i64>, Json(b): Json<Value>) -> Json<Value> {
    let mut m = m.lock().unwrap();
    let a = m.accounts.get_mut(&login).unwrap();
    a.status = b["status"].as_str().unwrap().into();
    Json(json!({"data": {"status": a.status}}))
}

async fn bulk(State(m): State<M>, Json(b): Json<Value>) -> Json<Value> {
    let mut m = m.lock().unwrap();
    m.bulk_closes += 1;
    let tickets: Vec<String> = b["tickets"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_string()).collect();
    for a in m.accounts.values_mut() {
        if a.positions.iter().any(|p| tickets.contains(&p["ticket"].as_str().unwrap().to_string())) {
            a.balance += a.floating;
            a.floating = D::ZERO;
            a.positions.clear();
            a.version += 1;
        }
    }
    Json(json!({"data": {"done": tickets, "failed": []}}))
}

async fn deals(State(m): State<M>, Query(q): Query<HashMap<String, String>>) -> Json<Value> {
    let m = m.lock().unwrap();
    let login: i64 = q["login"].parse().unwrap();
    Json(json!(m.accounts[&login].deals.iter().rev().cloned().collect::<Vec<_>>()))
}

async fn groups() -> Json<Value> {
    Json(json!({"groups": [{"code": "prop", "leverages": [30, 50, 100], "enabled": true}]}))
}

async fn wallet(State(m): State<M>, Json(b): Json<Value>) -> (axum::http::StatusCode, Json<Value>) {
    let mut m = m.lock().unwrap();
    let key = b["idempotency_key"].as_str().unwrap().to_string();
    if let Some(v) = m.wallet_keys.get(&key) {
        return (axum::http::StatusCode::OK, Json(v.clone()));
    }
    let user = b["user_id"].as_i64().unwrap();
    let amt = prop::money::dec_of(&b["amount"]).unwrap();
    let bal = *m.wallet.get(&user).unwrap_or(&D::ZERO);
    let next = if b["direction"] == "debit" { bal - amt } else { bal + amt };
    if next < D::ZERO {
        return (axum::http::StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": {"code": "insufficient_funds", "message": "Insufficient balance"}})));
    }
    m.wallet.insert(user, next);
    let v = json!({"status": "completed", "balance": num(next), "kind": b["kind"]});
    m.wallet_keys.insert(key, v.clone());
    (axum::http::StatusCode::OK, Json(v))
}

async fn support_notify(State(m): State<M>, Json(b): Json<Value>) -> Json<Value> {
    m.lock().unwrap().notified.push(b);
    Json(json!({"results": []}))
}

async fn start_mock(m: M) -> String {
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/v1/accounts", post(open))
        .route("/v1/admin/accounts/{login}", get(account))
        .route("/v1/admin/accounts/{login}/balance", post(balance))
        .route("/v1/admin/accounts/{login}/status", post(status))
        .route("/v1/dealing/positions/bulk", post(bulk))
        .route("/v1/dealing/orders/cancel", post(|| async { Json(json!({"data": {"done": [], "failed": []}})) }))
        .route("/v1/dealing/deals", get(deals))
        .route("/v1/admin/groups", get(groups))
        .route("/v1/wallets/transfers", post(wallet))
        .route("/v1/notify", post(support_notify))
        .with_state(m);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    format!("http://{addr}")
}

fn config(db: &str, mock: &str) -> Config {
    Config {
        bind: "127.0.0.1:0".into(),
        database_url: db.into(),
        internal_token: String::new(),
        dev_mode: true,
        json_logs: false,
        trading_url: mock.into(),
        trading_token: String::new(),
        wallet_url: mock.into(),
        wallet_token: String::new(),
        gateway_database_url: String::new(),
        poll_ms: 1000,
        poll_concurrency: 4,
        evaluator_enabled: false,
        verify_base_url: "https://app.example/verify".into(),
        support_url: String::new(),
        support_token: String::new(),
    }
}

fn deal(ticket: &str, profit: &str) -> Value {
    let now = Utc::now();
    json!({"id": ticket, "ticket": ticket, "symbol": "EURUSD", "side": "buy", "volume": 1, "openTime": now.to_rfc3339(),
           "closeTime": (now + chrono::Duration::seconds(90)).to_rfc3339(), "openPrice": 1.1, "closePrice": 1.2, "profit": profit.parse::<f64>().unwrap(), "reversed": false})
}

#[tokio::test]
async fn phase_state_machine_and_payouts() {
    let db = format!("postgres://postgres@127.0.0.1:5433/kalks_prop_test_{}", std::process::id());
    let pool = match store::connect(&db).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping: Postgres unavailable ({e})");
            return;
        }
    };
    let m: M = Arc::new(Mutex::new(Mock::default()));
    let url = start_mock(m.clone()).await;
    let app = Arc::new(Svc::new(config(&db, &url), pool.clone(), None));
    let t = "kalks";

    // a fast 2-step plan: 1 trading day per phase, payouts from day 0, fee refunded on the first payout
    let mut p = plans::get(&pool, t, "classic-2-step").await.unwrap();
    p.id = "flow-2-step".into();
    p.name = "Flow 2-Step".into();
    p.first_payout_days = 0;
    p.phases[0].min_days = 1;
    p.phases[1].min_days = 1;
    p.min_payout = d("10");
    p.validate().unwrap();
    plans::save(&pool, t, &p, "test", true).await.unwrap();

    let user = 7;
    m.lock().unwrap().wallet.insert(user, d("1000"));

    // --- purchase 1: daily-loss breach → fail + close all + disabled
    let p1 = ops::purchase(&app, t, user, "Flow Tester", "flow-2-step", d("10000"), "k1").await.unwrap();
    assert_eq!(p1.challenge.status, "active");
    assert!(p1.credentials.is_some());
    assert_eq!(m.lock().unwrap().wallet[&user], d("911"));
    let a1 = store::phases_of(&pool, p1.challenge.id).await.unwrap().remove(0);
    let l1 = a1.login.unwrap();
    assert_eq!(m.lock().unwrap().accounts[&l1].balance, d("10000"));
    // retry with the same key returns the same purchase, charged once
    let again = ops::purchase(&app, t, user, "Flow Tester", "flow-2-step", d("10000"), "k1").await.unwrap();
    assert_eq!(again.challenge.id, p1.challenge.id);
    assert_eq!(m.lock().unwrap().wallet[&user], d("911"));

    evaluator::evaluate_account(&app, a1.id, &[]).await.unwrap();
    assert_eq!(store::phase(&pool, a1.id).await.unwrap().unwrap().status, "active");
    {
        let mut mm = m.lock().unwrap();
        let a = mm.accounts.get_mut(&l1).unwrap();
        a.floating = d("-510");
        a.positions = vec![json!({"ticket": "1000001", "symbol": "EURUSD", "side": "buy", "volume": 5, "openTime": Utc::now().to_rfc3339(), "profit": -510})];
        a.version += 1;
    }
    evaluator::evaluate_account(&app, a1.id, &[]).await.unwrap();
    let a1 = store::phase(&pool, a1.id).await.unwrap().unwrap();
    assert_eq!(a1.status, "failed");
    assert_eq!(a1.end_reason.as_deref(), Some("daily_loss"));
    {
        let mm = m.lock().unwrap();
        assert_eq!(mm.accounts[&l1].status, "disabled");
        assert!(mm.accounts[&l1].positions.is_empty(), "close-all ran");
        assert_eq!(mm.accounts[&l1].balance, d("9490"));
        assert_eq!(mm.bulk_closes, 1);
    }
    assert_eq!(store::challenge(&pool, p1.challenge.id).await.unwrap().unwrap().status, "failed");
    let breaches: i64 = sqlx::query_scalar("SELECT count(*) FROM rule_events WHERE account_id = $1 AND severity = 'breach' AND rule = 'daily_loss'").bind(a1.id).fetch_one(&pool).await.unwrap();
    assert_eq!(breaches, 1);
    // a failed account is not evaluated again
    evaluator::evaluate_account(&app, a1.id, &[]).await.unwrap();

    // --- purchase 2: profit target → Phase 2 → Funded
    let p2 = ops::purchase(&app, t, user, "Flow Tester", "flow-2-step", d("10000"), "k2").await.unwrap();
    let a2 = store::phases_of(&pool, p2.challenge.id).await.unwrap().remove(0);
    let l2 = a2.login.unwrap();
    {
        let mut mm = m.lock().unwrap();
        let a = mm.accounts.get_mut(&l2).unwrap();
        a.balance += d("900");
        a.deals.push(deal("2000001", "900"));
        a.version += 1;
    }
    evaluator::evaluate_account(&app, a2.id, &[]).await.unwrap();
    let phases = store::phases_of(&pool, p2.challenge.id).await.unwrap();
    assert_eq!(phases.len(), 2);
    assert_eq!(phases[0].status, "passed");
    assert_eq!(phases[1].phase_name, "Phase 2");
    assert_eq!(phases[1].status, "active");
    assert_eq!(m.lock().unwrap().accounts[&l2].status, "read_only");
    let l3 = phases[1].login.unwrap();
    assert_eq!(m.lock().unwrap().accounts[&l3].balance, d("10000"));
    // Phase 2: 5% target
    {
        let mut mm = m.lock().unwrap();
        let a = mm.accounts.get_mut(&l3).unwrap();
        a.balance += d("499");
        a.deals.push(deal("2000002", "499"));
        a.version += 1;
    }
    evaluator::evaluate_account(&app, phases[1].id, &[]).await.unwrap();
    assert_eq!(store::phase(&pool, phases[1].id).await.unwrap().unwrap().status, "active", "499 < 500 target");
    {
        let mut mm = m.lock().unwrap();
        let a = mm.accounts.get_mut(&l3).unwrap();
        a.balance += d("1");
        a.version += 1;
    }
    evaluator::evaluate_account(&app, phases[1].id, &[]).await.unwrap();
    let phases = store::phases_of(&pool, p2.challenge.id).await.unwrap();
    assert_eq!(phases.len(), 3);
    assert!(phases[2].funded);
    assert_eq!(phases[2].status, "active");
    let c2 = store::challenge(&pool, p2.challenge.id).await.unwrap().unwrap();
    assert_eq!(c2.status, "funded");
    let kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM certificates WHERE challenge_id = $1 ORDER BY issued_at").bind(c2.id).fetch_all(&pool).await.unwrap();
    assert_eq!(kinds, vec!["pass", "pass", "funded"]);

    // --- funded payouts
    let fa = phases[2].clone();
    let l4 = fa.login.unwrap();
    {
        let mut mm = m.lock().unwrap();
        let a = mm.accounts.get_mut(&l4).unwrap();
        a.balance += d("1000");
        a.deals.push(deal("2000003", "1000"));
        a.version += 1;
    }
    evaluator::evaluate_account(&app, fa.id, &[]).await.unwrap();
    // KYC gate
    let e = ops::request_payout(&app, t, user, c2.id, Some("pending")).await.unwrap_err();
    assert!(format!("{e:?}").contains("kyc_required"));
    let po = ops::request_payout(&app, t, user, c2.id, Some("verified")).await.unwrap();
    assert_eq!(po["profit"], json!(1000.0));
    assert_eq!(po["traderAmount"], json!(800.0));
    assert_eq!(po["feeRefund"], json!(89.0));
    assert_eq!(m.lock().unwrap().accounts[&l4].balance, d("10000"), "profit withdrawn for review");
    // one request at a time
    assert!(ops::request_payout(&app, t, user, c2.id, Some("verified")).await.is_err());
    // rejected: the profit goes back on the account
    let staff = store::Actor { id: "staff:1".into(), name: "Risk".into(), role: "risk_manager".into() };
    let id = po["id"].as_i64().unwrap();
    let r = ops::reject_payout(&app, t, id, &staff, "Trades under review").await.unwrap();
    assert_eq!(r["status"], "rejected");
    assert_eq!(m.lock().unwrap().accounts[&l4].balance, d("11000"));
    // request again and approve → wallet credit (share + fee refund)
    evaluator::evaluate_account(&app, fa.id, &[]).await.unwrap();
    let po = ops::request_payout(&app, t, user, c2.id, Some("verified")).await.unwrap();
    let id = po["id"].as_i64().unwrap();
    let before = m.lock().unwrap().wallet[&user];
    let done = ops::approve_payout(&app, t, id, &staff, Some("ok"), Some("verified")).await.unwrap();
    assert_eq!(done["status"], "paid");
    assert_eq!(m.lock().unwrap().wallet[&user], before + d("889"));
    assert!(store::challenge(&pool, c2.id).await.unwrap().unwrap().fee_refunded);
    assert!(ops::approve_payout(&app, t, id, &staff, None, Some("verified")).await.is_err(), "decided once");
    let payout_certs: i64 = sqlx::query_scalar("SELECT count(*) FROM certificates WHERE challenge_id = $1 AND kind = 'payout'").bind(c2.id).fetch_one(&pool).await.unwrap();
    assert_eq!(payout_certs, 1);

    // --- manual fail override (audited)
    let e = ops::override_challenge(&app, t, c2.id, "fail", &staff, "Terms breach", "test").await.unwrap();
    assert_eq!(e["status"], "failed");
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'override.fail'").fetch_one(&pool).await.unwrap();
    assert_eq!(audited, 1);

    // --- insufficient wallet funds
    m.lock().unwrap().wallet.insert(user, d("10"));
    let e = ops::purchase(&app, t, user, "Flow Tester", "flow-2-step", d("10000"), "k3").await.unwrap_err();
    assert!(format!("{e:?}").contains("insufficient_funds"));
    let st: String = sqlx::query_scalar("SELECT status FROM challenges WHERE purchase_key = 'u7:k3'").fetch_one(&pool).await.unwrap();
    assert_eq!(st, "payment_failed");

    // audit log is append-only
    assert!(sqlx::query("DELETE FROM audit_log").execute(&pool).await.is_err());

    // every inbox row reaches the support service once (bell + email), with a stable dedupe key
    let mut cfg = config(&db, &url);
    cfg.support_url = url.clone();
    let pusher = Arc::new(Svc::new(cfg, pool.clone(), None));
    let http = reqwest::Client::new();
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications").fetch_one(&pool).await.unwrap();
    let mut sent = 0;
    loop {
        let (n, failed) = prop::notifier::push_pending(&pusher, &http).await.unwrap();
        assert_eq!(failed, 0);
        if n == 0 {
            break;
        }
        sent += n;
    }
    assert_eq!(sent as i64, total);
    let pushed = m.lock().unwrap().notified.clone();
    assert_eq!(pushed.len() as i64, total);
    let types: Vec<&str> = pushed.iter().filter_map(|b| b["type"].as_str()).collect();
    for want in ["prop.passed", "prop.failed", "prop.funded", "prop.payout_paid", "prop.payout_rejected"] {
        assert!(types.contains(&want), "{want} pushed: {types:?}");
    }
    let paid = pushed.iter().find(|b| b["type"] == "prop.payout_paid").unwrap();
    assert_eq!((paid["userId"].as_i64(), paid["link"].as_str(), paid["severity"].as_str()), (Some(user), Some("/prop/payouts"), Some("success")));
    assert!(paid["dedupeKey"].as_str().unwrap().starts_with("prop:n:"));

    pool.close().await;
    let admin = sqlx::PgPool::connect("postgres://postgres@127.0.0.1:5433/postgres").await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS kalks_prop_test_{} WITH (FORCE)", std::process::id()))).execute(&admin).await;
}

/// Module switches (gateway): while a broker has `prop` off, its client routes answer 403 module_disabled before any
/// handler runs; public certificate checks and the Back Office routes stay; another broker is unaffected.
#[tokio::test]
async fn module_switch_refuses_the_client_routes() {
    // no database is reached: the switch answers first, and the other requests only need a status
    let pool = sqlx::postgres::PgPoolOptions::new().acquire_timeout(std::time::Duration::from_millis(300)).connect_lazy("postgres://postgres@127.0.0.1:1/kalks_prop_unused").unwrap();
    let app = Arc::new(Svc::new(config("postgres://postgres@127.0.0.1:1/x", "http://127.0.0.1:1"), pool, None));
    prop::modules::prime("qa-prop-off", "prop", false);
    prop::modules::prime("qa-prop-on", "prop", true);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, prop::api::router(app)).await.unwrap() });
    let http = reqwest::Client::new();
    let call = |m: reqwest::Method, path: &str, tenant: &str| http.request(m, format!("{base}{path}")).header("x-kalks-tenant", tenant).header("x-kalks-user-id", "7").send();
    for (m, path) in [(reqwest::Method::GET, "/v1/plans"), (reqwest::Method::GET, "/v1/challenges"), (reqwest::Method::POST, "/v1/challenges"), (reqwest::Method::GET, "/v1/payouts")] {
        let r = call(m, path, "qa-prop-off").await.unwrap();
        assert_eq!(r.status().as_u16(), 403, "{path}");
        let v: Value = r.json().await.unwrap();
        assert_eq!(v["error"]["code"], "module_disabled");
    }
    // the same route for a broker with prop on gets past the switch (and fails later on the missing database)
    assert_ne!(call(reqwest::Method::GET, "/v1/plans", "qa-prop-on").await.unwrap().status().as_u16(), 403);
    // certificates stay verifiable, the Back Office keeps its routes
    assert_ne!(call(reqwest::Method::GET, "/v1/public/certificates/KP-NOPE", "qa-prop-off").await.unwrap().status().as_u16(), 403);
    let r = http.get(format!("{base}/v1/admin/overview")).header("x-kalks-tenant", "qa-prop-off").header("x-kalks-staff-id", "1").header("x-kalks-staff-role", "admin").send().await.unwrap();
    assert_ne!(r.status().as_u16(), 403);
}

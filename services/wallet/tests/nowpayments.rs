//! NOWPayments crypto deposits against PostgreSQL through the real HTTP router, with a mock NOWPayments API (an axum
//! server on a random port, `WALLET_NOWPAYMENTS_API_URL`). Throw-away database per test (skipped when PostgreSQL is
//! not reachable).
//! Covers: disabled until switched on with both keys, write-only encrypted keys and their audit, settings
//! permissions, test connection, limits and minimums, create → signed IPN → re-read from the API → credited exactly
//! once (duplicate IPNs, IPN + poller racing), the IPN body never trusted on its own, bad / missing signatures,
//! partially paid / overpaid / late payments to review, four-eyes approval, rejection, expiry, repeated deposits.

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use wallet::config::Config;
use wallet::engine::MockEngine;
use wallet::nowpayments::{canonical, sign};
use wallet::ops::nowpayments as npo;
use wallet::state::{AppState, Tenants};
use wallet::users::MockUsers;
use wallet::{db, ledger};

const API_KEY: &str = "NP-TEST-KEY-0001-ABCD";
const IPN_SECRET: &str = "np-ipn-secret-9f8e7d";
const ADMIN: &str = "finance.read,finance.write,finance.adjust_approve,finance.settings";
const FINANCE: &str = "finance.read,finance.write,finance.adjust_approve";

/* ---------------- mock NOWPayments ---------------- */

#[derive(Default)]
struct Mock {
    payments: Mutex<HashMap<String, Value>>,
    next_id: AtomicUsize,
    gets: AtomicUsize,
    creates: AtomicUsize,
    /// coin → USD minimum (fiat_equivalent)
    mins: Mutex<HashMap<String, f64>>,
    last_create: Mutex<Option<Value>>,
}

type M = Arc<Mock>;

fn authed(h: &HeaderMap) -> bool {
    h.get("x-api-key").and_then(|v| v.to_str().ok()) == Some(API_KEY)
}

fn denied() -> (StatusCode, axum::Json<Value>) {
    (StatusCode::FORBIDDEN, axum::Json(json!({"statusCode": 403, "code": "INVALID_API_KEY", "message": "Invalid api key"})))
}

async fn m_status() -> axum::Json<Value> {
    axum::Json(json!({"message": "OK"}))
}

async fn m_coins(h: HeaderMap) -> (StatusCode, axum::Json<Value>) {
    if !authed(&h) {
        return denied();
    }
    (StatusCode::OK, axum::Json(json!({"selectedCurrencies": ["BTC", "usdttrc20", "usdtbsc", "eth", "xrp"]})))
}

async fn m_min(State(m): State<M>, h: HeaderMap, Query(q): Query<HashMap<String, String>>) -> (StatusCode, axum::Json<Value>) {
    if !authed(&h) {
        return denied();
    }
    let coin = q.get("currency_from").cloned().unwrap_or_default();
    let usd = m.mins.lock().unwrap().get(&coin).copied().unwrap_or(1.0);
    (StatusCode::OK, axum::Json(json!({"currency_from": coin, "currency_to": coin, "min_amount": 0.5, "fiat_equivalent": usd})))
}

async fn m_create(State(m): State<M>, h: HeaderMap, axum::Json(b): axum::Json<Value>) -> (StatusCode, axum::Json<Value>) {
    if !authed(&h) {
        return denied();
    }
    m.creates.fetch_add(1, Ordering::SeqCst);
    *m.last_create.lock().unwrap() = Some(b.clone());
    let id = (5_745_459_400 + m.next_id.fetch_add(1, Ordering::SeqCst)).to_string();
    let price = b["price_amount"].as_f64().unwrap();
    let coin = b["pay_currency"].as_str().unwrap().to_string();
    // a made-up rate: 1 USD = 1.005 units (+ fees paid by the user)
    let pay_amount = (price * 1.005 * 1e6).round() / 1e6;
    let p = json!({
        "payment_id": id, "payment_status": "waiting", "pay_address": format!("addr-{id}"), "price_amount": price, "price_currency": "usd",
        "pay_amount": pay_amount, "pay_currency": coin, "order_id": b["order_id"], "order_description": b["order_description"],
        "ipn_callback_url": b["ipn_callback_url"], "created_at": "2026-10-23T10:00:00.000Z", "updated_at": "2026-10-23T10:00:00.000Z",
        "purchase_id": "1", "amount_received": null, "payin_extra_id": if coin == "xrp" { json!("771234") } else { Value::Null },
        "network": coin, "expiration_estimate_date": "2099-01-01T00:00:00.000Z", "is_fixed_rate": b["is_fixed_rate"], "is_fee_paid_by_user": b["is_fee_paid_by_user"],
    });
    let mut stored = p.clone();
    stored["payment_id"] = json!(id.parse::<u64>().unwrap());
    stored["actually_paid"] = json!(0);
    m.payments.lock().unwrap().insert(id, stored);
    (StatusCode::CREATED, axum::Json(p))
}

async fn m_payment(State(m): State<M>, h: HeaderMap, Path(id): Path<String>) -> (StatusCode, axum::Json<Value>) {
    if !authed(&h) {
        return denied();
    }
    m.gets.fetch_add(1, Ordering::SeqCst);
    match m.payments.lock().unwrap().get(&id) {
        Some(p) => (StatusCode::OK, axum::Json(p.clone())),
        None => (StatusCode::NOT_FOUND, axum::Json(json!({"message": "Payment not found"}))),
    }
}

impl Mock {
    /// What NOWPayments' API reports from now on for payment `id`.
    fn set(&self, id: &str, status: &str, actually_paid: f64) {
        let mut ps = self.payments.lock().unwrap();
        let p = ps.get_mut(id).expect("known payment");
        p["payment_status"] = json!(status);
        p["actually_paid"] = json!(actually_paid);
        p["outcome_amount"] = json!(actually_paid * 0.99);
        p["outcome_currency"] = p["pay_currency"].clone();
        p["payin_hash"] = json!(format!("hash-{id}"));
    }
    fn get(&self, id: &str) -> Value {
        self.payments.lock().unwrap().get(id).cloned().unwrap()
    }
}

/* ---------------- harness ---------------- */

struct T {
    st: AppState,
    base: String,
    http: reqwest::Client,
    np: M,
    users: Arc<MockUsers>,
    admin: PgConnectOptions,
    name: String,
}

impl T {
    async fn new(what: &str) -> Option<Self> {
        let base = std::env::var("WALLET_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
        let admin = PgConnectOptions::from_str(&base).ok()?.database("postgres");
        if admin.connect().await.is_err() {
            eprintln!("skipping {what}: no PostgreSQL at {base}");
            return None;
        }
        let mut b = [0u8; 5];
        getrandom::fill(&mut b).unwrap();
        let name = format!("kalks_wallet_test_{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>());
        let url = admin.clone().database(&name).to_url_lossy().to_string();
        let pool = db::connect(&url).await.expect("create + migrate test db");

        let np: M = Arc::new(Mock::default());
        let mock = Router::new()
            .route("/v1/status", get(m_status))
            .route("/v1/merchant/coins", get(m_coins))
            .route("/v1/min-amount", get(m_min))
            .route("/v1/payment", post(m_create))
            .route("/v1/payment/{id}", get(m_payment))
            .with_state(np.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let np_url = format!("http://{}/v1", l.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(l, mock).await.unwrap() });

        let mut cfg = Config::for_tests(&url);
        cfg.nowpayments_api_url = np_url;
        wallet::settings::seed(&pool, &cfg).await.unwrap();
        let users = Arc::new(MockUsers::default());
        let st = AppState {
            tenants: Tenants::load(&pool).await.unwrap(),
            pool,
            cfg: Arc::new(cfg),
            chains: Arc::new(HashMap::new()),
            engine: Arc::new(MockEngine::default()),
            users: users.clone(),
            transfer_lock: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let app = wallet::api::router(st.clone());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Some(T { st, base: base_url, http: reqwest::Client::new(), np, users, admin, name })
    }

    async fn req(&self, method: &str, path: &str, body: Option<Value>, staff: Option<(&str, &str, &str)>) -> (u16, Value) {
        let url = format!("{}{path}", self.base);
        let mut rb = match method {
            "GET" => self.http.get(url),
            "PUT" => self.http.put(url),
            _ => self.http.post(url),
        };
        rb = rb.header("x-kalks-internal", "test-internal-token").header("x-forwarded-for", "203.0.113.9");
        if let Some((id, role, perms)) = staff {
            rb = rb.header("x-kalks-staff-id", id).header("x-kalks-staff-name", format!("Staff%20{id}")).header("x-kalks-staff-role", role).header("x-kalks-staff-perms", perms);
        }
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let r = rb.send().await.unwrap();
        let s = r.status().as_u16();
        (s, r.json().await.unwrap_or(Value::Null))
    }
    async fn get(&self, path: &str) -> (u16, Value) {
        self.req("GET", path, None, None).await
    }
    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.req("POST", path, Some(body), None).await
    }
    async fn admin(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        self.req(method, path, body, Some(("1", "admin", ADMIN))).await
    }

    /// Switches NOWPayments on with both keys (an admin with finance.settings).
    async fn configure(&self) {
        let (s, v) = self
            .admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"api_key": API_KEY, "ipn_secret": IPN_SECRET, "enabled": true, "reason": "Go live with NOWPayments"})))
            .await;
        assert_eq!(s, 200, "{v}");
    }

    /// The raw IPN, as NOWPayments sends it: the payment as JSON, signed over the sorted body.
    async fn ipn_raw(&self, raw: &str, sig: Option<&str>) -> (u16, Value) {
        let mut rb = self.http.post(format!("{}/v1/nowpayments/ipn", self.base)).header("x-kalks-internal", "test-internal-token").header("content-type", "application/json").body(raw.to_string());
        if let Some(s) = sig {
            rb = rb.header("x-nowpayments-sig", s);
        }
        let r = rb.send().await.unwrap();
        let s = r.status().as_u16();
        (s, r.json().await.unwrap_or(Value::Null))
    }
    async fn ipn(&self, body: &Value) -> (u16, Value) {
        let raw = serde_json::to_string(body).unwrap();
        self.ipn_raw(&raw, Some(&sign(IPN_SECRET, &canonical(body)))).await
    }

    async fn create(&self, user: i64, amount: &str, coin: &str) -> Value {
        let (s, v) = self.post("/v1/nowpayments/payments", json!({"user_id": user, "amount": amount, "pay_currency": coin})).await;
        assert_eq!(s, 200, "{v}");
        v["payment"].clone()
    }

    async fn np_id(&self, id: i64) -> String {
        sqlx::query_scalar("SELECT np_payment_id FROM nowpayments_payments WHERE id = $1").bind(id).fetch_one(&self.st.pool).await.unwrap()
    }

    async fn state(&self, id: i64) -> String {
        sqlx::query_scalar("SELECT status FROM nowpayments_payments WHERE id = $1").bind(id).fetch_one(&self.st.pool).await.unwrap()
    }

    async fn available(&self, user: i64) -> String {
        let (_, v) = self.get(&format!("/v1/wallets/{user}")).await;
        v["balances"][0]["available"].as_str().unwrap().to_string()
    }

    async fn credits(&self, np_id: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key = $1").bind(npo::ledger_key(np_id)).fetch_one(&self.st.pool).await.unwrap()
    }

    async fn invariants(&self) {
        let p = ledger::verify(&self.st.pool).await.unwrap();
        assert!(p.is_empty(), "ledger invariants: {p:?}");
    }

    async fn drop_db(self) {
        self.st.pool.close().await;
        if let Ok(mut c) = self.admin.connect().await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))).execute(&mut c).await;
        }
    }
}

/* ---------------- tests ---------------- */

#[tokio::test]
async fn disabled_until_configured_and_keys_are_write_only() {
    let Some(t) = T::new("nowpayments settings").await else { return };
    // off by default: clients see nothing and can't create
    let (s, v) = t.get("/v1/nowpayments/config").await;
    assert_eq!((s, v["enabled"].as_bool()), (200, Some(false)), "{v}");
    let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 7, "amount": "100", "pay_currency": "usdttrc20"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("nowpayments_disabled")), "{v}");

    // finance staff without finance.settings can read but not change
    let (s, _) = t.req("GET", "/v1/admin/nowpayments/settings", None, Some(("2", "finance", FINANCE))).await;
    assert_eq!(s, 200);
    let (s, _) = t.req("PUT", "/v1/admin/nowpayments/settings", Some(json!({"enabled": false, "reason": "try"})), Some(("2", "finance", FINANCE))).await;
    assert_eq!(s, 403);
    // switching on needs both keys, and a reason
    let (s, v) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"enabled": true, "reason": "Go live"}))).await;
    assert_eq!((s, v["error"]["field"].as_str()), (422, Some("enabled")), "{v}");
    let (s, _) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"api_key": API_KEY}))).await;
    assert_eq!(s, 422);
    let (s, _) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"api_key": "short", "reason": "Paste key"}))).await;
    assert_eq!(s, 422);

    // test the key before saving it
    let (s, v) = t.admin("POST", "/v1/admin/nowpayments/test", Some(json!({"api_key": API_KEY}))).await;
    assert_eq!((s, v["ok"].as_bool()), (200, Some(true)), "{v}");
    assert!(v["merchant_coins"].as_array().unwrap().iter().any(|c| c["code"] == "btc"), "{v}");
    assert!(v["missing"].as_array().unwrap().iter().any(|c| c == "ltc"), "{v}");
    let (s, v) = t.admin("POST", "/v1/admin/nowpayments/test", Some(json!({"api_key": "WRONG-KEY-123456"}))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (502, Some("provider_error")), "{v}");

    t.configure().await;
    let (s, v) = t.admin("GET", "/v1/admin/nowpayments/settings", None).await;
    assert_eq!(s, 200);
    let text = v.to_string();
    assert!(!text.contains(API_KEY) && !text.contains(IPN_SECRET), "secrets leaked: {text}");
    assert_eq!(v["settings"]["api_key"]["set"], true);
    assert_eq!(v["settings"]["api_key"]["last4"], "ABCD");
    assert_eq!(v["settings"]["ipn_secret"]["last4"], "8e7d");
    assert_eq!(v["settings"]["enabled"], true);
    assert_eq!(v["settings"]["min_deposit_usd"], "10");
    assert_eq!(v["settings"]["fee_paid_by_user"], true);
    assert_eq!(v["settings"]["ipn_url"], "https://app.kalkstrade.com/api/payments/nowpayments/ipn");
    assert_eq!(v["settings"]["coins"], json!(["usdttrc20", "usdtbsc", "usdterc20", "btc", "eth", "ltc", "trx", "bnbbsc"]));
    // encrypted at rest; the audit log never holds a secret
    let enc: String = sqlx::query_scalar("SELECT api_key_enc FROM nowpayments_settings WHERE tenant_id = 1").fetch_one(&t.st.pool).await.unwrap();
    assert!(enc.starts_with("v1:") && !enc.contains(API_KEY));
    let audit: Vec<String> = sqlx::query_scalar("SELECT COALESCE(before::text, '') || COALESCE(after::text, '') FROM audit_log WHERE action = 'wallet.settings.nowpayments'").fetch_all(&t.st.pool).await.unwrap();
    assert!(!audit.is_empty());
    assert!(audit.iter().all(|a| !a.contains(API_KEY) && !a.contains(IPN_SECRET)), "{audit:?}");
    // an empty key field leaves the key as it is
    let (s, v) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"api_key": "", "min_deposit_usd": "20", "coins": ["usdttrc20", "BTC", "xrp"], "reason": "Tune"}))).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["settings"]["api_key"]["set"], true);
    assert_eq!(v["settings"]["coins"], json!(["usdttrc20", "btc", "xrp"]));

    // client config: coins with their minimum (the broker's $20 or NOWPayments' own when higher)
    t.np.mins.lock().unwrap().insert("btc".into(), 25.004);
    let (s, v) = t.get("/v1/nowpayments/config").await;
    assert_eq!((s, v["enabled"].as_bool()), (200, Some(true)), "{v}");
    let coins = v["coins"].as_array().unwrap();
    assert_eq!(coins.len(), 3);
    assert_eq!(coins[0]["min_usd"], "20");
    assert_eq!(coins[1]["code"], "btc");
    assert_eq!(coins[1]["min_usd"], "25.01");
    assert_eq!(coins[2]["needs_extra_id"], true);
    // a coin the merchant account doesn't accept (ltc) is not offered to clients
    let (s, _) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"coins": ["usdttrc20", "ltc"], "reason": "Add LTC"}))).await;
    assert_eq!(s, 200);
    let (_, v) = t.get("/v1/nowpayments/config").await;
    assert_eq!(v["coins"].as_array().unwrap().iter().map(|c| c["code"].as_str().unwrap()).collect::<Vec<_>>(), vec!["usdttrc20"]);

    // switching off hides it again; clearing a key needs it off first
    let (s, _) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"clear_api_key": true, "reason": "Rotate"}))).await;
    assert_eq!(s, 422);
    let (s, _) = t.admin("PUT", "/v1/admin/nowpayments/settings", Some(json!({"enabled": false, "clear_api_key": true, "reason": "Rotate"}))).await;
    assert_eq!(s, 200);
    let (_, v) = t.get("/v1/nowpayments/config").await;
    assert_eq!(v["enabled"], false);
    t.drop_db().await;
}

#[tokio::test]
async fn limits_and_minimums() {
    let Some(t) = T::new("nowpayments limits").await else { return };
    t.configure().await;
    t.np.mins.lock().unwrap().insert("btc".into(), 25.004);
    for (amount, coin, code) in [("5", "usdttrc20", "below_minimum"), ("50001", "usdttrc20", "above_maximum"), ("20", "btc", "below_minimum")] {
        let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 7, "amount": amount, "pay_currency": coin})).await;
        assert_eq!((s, v["error"]["code"].as_str()), (422, Some(code)), "{amount} {coin}: {v}");
    }
    let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 7, "amount": "100", "pay_currency": "doge"})).await;
    assert_eq!((s, v["error"]["field"].as_str()), (422, Some("pay_currency")), "{v}");
    let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 7, "amount": "100.123", "pay_currency": "usdttrc20"})).await;
    assert_eq!((s, v["error"]["field"].as_str()), (422, Some("amount")), "{v}");
    // a client the Back Office restricted can't deposit
    t.users.set(9, "verified");
    t.users.restrict(9, &["deposits"]);
    let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 9, "amount": "100", "pay_currency": "usdttrc20"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("restricted")), "{v}");
    // five payments waiting for money at once at most
    for _ in 0..5 {
        t.create(7, "10", "usdttrc20").await;
    }
    let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 7, "amount": "10", "pay_currency": "usdttrc20"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (429, Some("too_many_open")), "{v}");
    assert_eq!(t.np.creates.load(Ordering::SeqCst), 5);
    t.drop_db().await;
}

#[tokio::test]
async fn create_ipn_finished_credits_exactly_once() {
    let Some(t) = T::new("nowpayments credit").await else { return };
    t.configure().await;
    let p = t.create(7, "100", "usdttrc20").await;
    assert_eq!(p["status"], "waiting");
    assert_eq!(p["pay_amount"], "100.5");
    assert_eq!(p["price_amount"], "100");
    assert!(p["pay_address"].as_str().unwrap().starts_with("addr-"));
    // what we sent NOWPayments
    let sent = t.np.last_create.lock().unwrap().clone().unwrap();
    assert_eq!(sent["price_currency"], "usd");
    assert_eq!(sent["is_fee_paid_by_user"], true);
    assert_eq!(sent["ipn_callback_url"], "https://app.kalkstrade.com/api/payments/nowpayments/ipn");
    assert_eq!(sent["order_id"], p["order_id"]);
    let id = p["id"].as_i64().unwrap();
    let pid = t.np_id(id).await;

    // an IPN claiming "finished" while NOWPayments' API still says waiting: nothing is credited
    let mut fake = t.np.get(&pid);
    fake["payment_status"] = json!("finished");
    fake["actually_paid"] = json!(100.5);
    let (s, v) = t.ipn(&fake).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(t.state(id).await, "open");
    assert_eq!(t.available(7).await, "0");

    // confirming, then finished
    t.np.set(&pid, "confirming", 100.5);
    let (s, _) = t.ipn(&t.np.get(&pid)).await;
    assert_eq!(s, 200);
    let (_, v) = t.get(&format!("/v1/nowpayments/payments/{id}?user_id=7")).await;
    assert_eq!(v["payment"]["status"], "confirming");
    t.np.set(&pid, "finished", 100.5);
    let body = t.np.get(&pid);
    let (s, _) = t.ipn(&body).await;
    assert_eq!(s, 200);
    assert_eq!(t.state(id).await, "credited");
    assert_eq!(t.available(7).await, "100");

    // the same IPN again, the poller and several concurrent re-checks: still credited once
    let (s, _) = t.ipn(&body).await;
    assert_eq!(s, 200);
    let mut handles = vec![];
    for _ in 0..6 {
        let st = t.st.clone();
        handles.push(tokio::spawn(async move { npo::sync(&st, id).await }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    sqlx::query("UPDATE nowpayments_payments SET last_checked_at = NULL").execute(&t.st.pool).await.unwrap();
    npo::poll(&t.st).await.unwrap();
    assert_eq!(t.available(7).await, "100");
    assert_eq!(t.credits(&pid).await, 1);

    // ledger: user +100, sys:USDT:deposit:nowpayments -100; IB deposit event and client notification queued
    let sys: rust_decimal::Decimal = sqlx::query_scalar("SELECT sum(amount) FROM ledger_postings WHERE account_code = 'sys:USDT:deposit:nowpayments'").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(sys, rust_decimal::Decimal::from(-100));
    let ib: i64 = sqlx::query_scalar("SELECT count(*) FROM ib_deposit_events WHERE user_id = 7 AND kind = 'deposit'").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(ib, 1);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE user_id = 7 AND kind = 'deposit.credited'").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(n, 1);
    // client views: the payment, the activity and the withdrawal cooldown
    let (_, v) = t.get(&format!("/v1/nowpayments/payments/{id}?user_id=7")).await;
    assert_eq!((v["payment"]["status"].as_str(), v["payment"]["credit_amount"].as_str()), (Some("credited"), Some("100")));
    let (s, _) = t.get(&format!("/v1/nowpayments/payments/{id}?user_id=8")).await;
    assert_eq!(s, 404);
    let (_, v) = t.get("/v1/wallets/7/activity?type=deposit").await;
    let item = &v["items"][0];
    assert_eq!((item["kind"].as_str(), item["status"].as_str(), item["amount"].as_str(), item["note"].as_str()), (Some("nowpayments"), Some("credited"), Some("100"), Some("usdttrc20")), "{v}");
    let (_, v) = t.get("/v1/wallets/7/overview").await;
    assert!(v["limits"]["cooldown_until"].is_string(), "{v}");
    let (_, v) = t.get("/v1/nowpayments/payments?user_id=7").await;
    assert_eq!(v["total"], 1);
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn ipn_signature_is_checked() {
    let Some(t) = T::new("nowpayments ipn signature").await else { return };
    t.configure().await;
    let p = t.create(7, "50", "usdtbsc").await;
    let id = p["id"].as_i64().unwrap();
    let pid = t.np_id(id).await;
    t.np.set(&pid, "finished", 50.25);
    let body = t.np.get(&pid);
    let raw = serde_json::to_string(&body).unwrap();
    let good = sign(IPN_SECRET, &canonical(&body));
    let gets = t.np.gets.load(Ordering::SeqCst);
    // missing, wrong secret, tampered body: 401 and NOWPayments is not even asked
    let (s, v) = t.ipn_raw(&raw, None).await;
    assert_eq!((s, v["error"]["code"].as_str()), (401, Some("invalid_signature")));
    let (s, _) = t.ipn_raw(&raw, Some(&sign("another-secret", &canonical(&body)))).await;
    assert_eq!(s, 401);
    let (s, _) = t.ipn_raw(&raw.replace("50.25", "5025"), Some(&good)).await;
    assert_eq!(s, 401);
    assert_eq!(t.np.gets.load(Ordering::SeqCst), gets);
    assert_eq!(t.available(7).await, "0");
    // unknown payment, garbage
    let mut other = body.clone();
    other["payment_id"] = json!(1);
    other["order_id"] = Value::Null;
    let (s, _) = t.ipn(&other).await;
    assert_eq!(s, 404);
    let (s, _) = t.ipn_raw("not json", Some(&good)).await;
    assert_eq!(s, 400);
    // the real one (keys in another order, as received) is accepted
    let reordered = format!("{{\"payment_status\":\"finished\",{}", &raw[1..].replace("\"payment_status\":\"finished\",", ""));
    let (s, v) = t.ipn_raw(&reordered, Some(&good)).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(t.available(7).await, "50");
    let n: i32 = sqlx::query_scalar("SELECT ipn_count FROM nowpayments_payments WHERE id = $1").bind(id).fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(n, 1);
    // without an IPN secret the IPN is refused
    sqlx::query("UPDATE nowpayments_settings SET ipn_secret_enc = NULL").execute(&t.st.pool).await.unwrap();
    let (s, _) = t.ipn_raw(&raw, Some(&good)).await;
    assert_eq!(s, 503);
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn partial_payment_goes_to_review_with_four_eyes() {
    let Some(t) = T::new("nowpayments review").await else { return };
    t.configure().await;
    // four-eyes above 50 USD (the manual adjustments threshold)
    sqlx::query("UPDATE tenant_settings SET adjust_approval_usd = 50").execute(&t.st.pool).await.unwrap();
    let p = t.create(7, "200", "usdttrc20").await;
    let id = p["id"].as_i64().unwrap();
    let pid = t.np_id(id).await;
    t.np.set(&pid, "partially_paid", 100.5); // half of 201
    let (s, _) = t.ipn(&t.np.get(&pid)).await;
    assert_eq!(s, 200);
    assert_eq!(t.state(id).await, "review");
    assert_eq!(t.available(7).await, "0");
    let (_, v) = t.get(&format!("/v1/nowpayments/payments/{id}?user_id=7")).await;
    assert_eq!(v["payment"]["status"], "review");
    let (s, v) = t.admin("GET", &format!("/v1/admin/nowpayments/payments/{id}"), None).await;
    assert_eq!(s, 200);
    assert_eq!(v["payment"]["suggested_credit"], "100");
    assert!(v["payment"]["review_reason"].as_str().unwrap().contains("Partially paid"));
    assert_eq!(v["payment"]["last_ipn"]["payment_status"], "partially_paid");
    let (_, v) = t.admin("GET", "/v1/admin/nowpayments/payments?status=queue", None).await;
    assert_eq!((v["total"].as_i64(), v["counts"]["review"].as_i64()), (Some(1), Some(1)), "{v}");

    // no reason → 422; a reader can't approve
    let (s, _) = t.admin("POST", &format!("/v1/admin/nowpayments/payments/{id}/approve"), Some(json!({"amount": "100"}))).await;
    assert_eq!(s, 422);
    let (s, _) = t.req("POST", &format!("/v1/admin/nowpayments/payments/{id}/approve"), Some(json!({"amount": "100", "reason": "ok"})), Some(("3", "compliance", "finance.read"))).await;
    assert_eq!(s, 403);
    // 100 > 50: staff 1 proposes, the payment waits for a second approver
    let (s, v) = t.admin("POST", &format!("/v1/admin/nowpayments/payments/{id}/approve"), Some(json!({"amount": "100", "reason": "Client sent half, confirmed by chat"}))).await;
    assert_eq!((s, v["payment"]["state"].as_str()), (200, Some("approval")), "{v}");
    assert_eq!(t.available(7).await, "0");
    let (s, _) = t.admin("POST", &format!("/v1/admin/nowpayments/payments/{id}/approve"), Some(json!({}))).await;
    assert_eq!(s, 403, "the proposer can't approve their own proposal");
    let (s, _) = t.req("POST", &format!("/v1/admin/nowpayments/payments/{id}/approve"), Some(json!({"amount": "150"})), Some(("2", "finance", FINANCE))).await;
    assert_eq!(s, 409, "another amount than proposed");
    let (s, v) = t.req("POST", &format!("/v1/admin/nowpayments/payments/{id}/approve"), Some(json!({})), Some(("2", "finance", FINANCE))).await;
    assert_eq!((s, v["payment"]["state"].as_str()), (200, Some("credited")), "{v}");
    assert_eq!(t.available(7).await, "100");
    // a late "finished" IPN for the same payment books nothing more
    t.np.set(&pid, "finished", 201.0);
    let (s, _) = t.ipn(&t.np.get(&pid)).await;
    assert_eq!(s, 200);
    assert_eq!(t.available(7).await, "100");
    assert_eq!(t.credits(&pid).await, 1);
    let actions: Vec<String> = sqlx::query_scalar("SELECT action FROM audit_log WHERE target_kind = 'nowpayments_payment' ORDER BY id").fetch_all(&t.st.pool).await.unwrap();
    assert_eq!(actions, vec!["wallet.nowpayments.approval_requested", "wallet.nowpayments.credited"]);

    // below the threshold one staff member credits at once; reject tells the client
    let p2 = t.create(8, "40", "usdttrc20").await;
    let id2 = p2["id"].as_i64().unwrap();
    let pid2 = t.np_id(id2).await;
    t.np.set(&pid2, "partially_paid", 20.1);
    t.ipn(&t.np.get(&pid2)).await;
    let (s, v) = t.admin("POST", &format!("/v1/admin/nowpayments/payments/{id2}/approve"), Some(json!({"reason": "Credit what arrived"}))).await;
    assert_eq!((s, v["payment"]["state"].as_str(), v["payment"]["credit_amount"].as_str()), (200, Some("credited"), Some("20")), "{v}");
    assert_eq!(t.available(8).await, "20");
    let p3 = t.create(8, "40", "usdttrc20").await;
    let id3 = p3["id"].as_i64().unwrap();
    let pid3 = t.np_id(id3).await;
    t.np.set(&pid3, "partially_paid", 1.0);
    t.ipn(&t.np.get(&pid3)).await;
    let (s, v) = t.admin("POST", &format!("/v1/admin/nowpayments/payments/{id3}/reject"), Some(json!({"reason": "Refunded to the client by NOWPayments"}))).await;
    assert_eq!((s, v["payment"]["state"].as_str()), (200, Some("rejected")), "{v}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE user_id = 8 AND kind = 'deposit.rejected'").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(n, 1);
    let (s, _) = t.admin("POST", &format!("/v1/admin/nowpayments/payments/{id3}/approve"), Some(json!({"amount": "1", "reason": "x y z"}))).await;
    assert_eq!(s, 409);
    assert_eq!(t.available(8).await, "20");
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn overpaid_late_expired_and_repeated_deposits() {
    let Some(t) = T::new("nowpayments edge cases").await else { return };
    t.configure().await;
    // overpaid by more than 1% → review
    let a = t.create(7, "100", "usdttrc20").await["id"].as_i64().unwrap();
    let pa = t.np_id(a).await;
    t.np.set(&pa, "finished", 150.0);
    t.ipn(&t.np.get(&pa)).await;
    assert_eq!(t.state(a).await, "review");
    // overpaid within 1% → credited the price
    let b = t.create(7, "100", "usdttrc20").await["id"].as_i64().unwrap();
    let pb = t.np_id(b).await;
    t.np.set(&pb, "finished", 101.0);
    npo::sync(&t.st, b).await.unwrap();
    assert_eq!(t.state(b).await, "credited");
    assert_eq!(t.available(7).await, "100");
    // expired (poller), then paid late → review
    let c = t.create(7, "30", "usdttrc20").await["id"].as_i64().unwrap();
    let pc = t.np_id(c).await;
    t.np.set(&pc, "expired", 0.0);
    sqlx::query("UPDATE nowpayments_payments SET last_checked_at = NULL").execute(&t.st.pool).await.unwrap();
    npo::poll(&t.st).await.unwrap();
    assert_eq!(t.state(c).await, "closed");
    let (_, v) = t.get(&format!("/v1/nowpayments/payments/{c}?user_id=7")).await;
    assert_eq!(v["payment"]["status"], "expired");
    t.np.set(&pc, "finished", 30.15);
    t.ipn(&t.np.get(&pc)).await;
    assert_eq!(t.state(c).await, "review");
    // a repeated deposit to the address of payment b (NOWPayments parent_payment_id) → its own row, in review
    let child = json!({"payment_id": 9_000_000_001u64, "parent_payment_id": pb.parse::<u64>().unwrap(), "payment_status": "finished", "price_amount": 100,
                       "price_currency": "usd", "pay_amount": 100.5, "actually_paid": 100.5, "pay_currency": "usdttrc20", "order_id": null});
    t.np.payments.lock().unwrap().insert("9000000001".into(), child.clone());
    let (s, v) = t.ipn(&child).await;
    assert_eq!(s, 200, "{v}");
    let (st, user, parent): (String, i64, Option<i64>) = sqlx::query_as("SELECT status, user_id, parent_id FROM nowpayments_payments WHERE np_payment_id = '9000000001'").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!((st.as_str(), user, parent), ("review", 7, Some(b)));
    // a failed create call: the client gets an error, the row is kept as error and not shown
    sqlx::query("UPDATE nowpayments_settings SET api_key_enc = $1").bind(wallet::secrets::seal(&wallet::secrets::key(&t.st.cfg).unwrap(), "1:nowpayments.api_key", "WRONG-KEY-12345").unwrap()).execute(&t.st.pool).await.unwrap();
    let (s, v) = t.post("/v1/nowpayments/payments", json!({"user_id": 7, "amount": "100", "pay_currency": "usdttrc20"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (502, Some("provider_error")), "{v}");
    let errors: i64 = sqlx::query_scalar("SELECT count(*) FROM nowpayments_payments WHERE status = 'error'").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(errors, 1);
    assert_eq!(t.available(7).await, "100");
    t.invariants().await;
    t.drop_db().await;
}

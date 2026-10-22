//! End-to-end flows against PostgreSQL through the real HTTP router, with a mock chain, engine and gateway.
//! Each test uses a throw-away database `kalks_wallet_test_<random>` on the local server (127.0.0.1:5433,
//! override with WALLET_TEST_DATABASE_URL); tests are skipped with a note when PostgreSQL is not reachable.
//! After every flow the ledger invariants are checked (Σ postings = 0 per txn, balances = Σ postings).

use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use wallet::chain::mock::MockChain;
use wallet::chain::{Chain, ChainId, Incoming, TokenTransfer, TxLookup, TxStatus};
use wallet::config::Config;
use wallet::engine::MockEngine;
use wallet::money::D;
use wallet::state::{AppState, Tenants};
use wallet::users::MockUsers;
use wallet::{db, ledger, watcher};

const TRON_ADDR: &str = "TU7PHUS22Hw632YsnAyjxNh4gu3u8PzcHZ";
const BSC_ADDR: &str = "0x11e9373d598703f83582e34378e086ebeec5da11";
const CLIENT_TRON: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t"; // any valid base58 address works as a client wallet
const OTHER_TRON: &str = "TDVSPgBZDmNjkYpSH9GdLLbV6LCLhrrYnx";

struct T {
    st: AppState,
    base: String,
    http: reqwest::Client,
    tron: Arc<MockChain>,
    bsc: Arc<MockChain>,
    engine: Arc<MockEngine>,
    users: Arc<MockUsers>,
    admin: PgConnectOptions,
    name: String,
}

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn hash(n: u64) -> String {
    format!("{n:064x}")
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
        let cfg = Config::for_tests(&url);
        wallet::settings::seed(&pool, &cfg).await.unwrap();
        let tron = Arc::new(MockChain::new(ChainId::Tron));
        let bsc = Arc::new(MockChain::new(ChainId::Bsc));
        let mut chains: HashMap<ChainId, Arc<dyn Chain>> = HashMap::new();
        chains.insert(ChainId::Tron, tron.clone());
        chains.insert(ChainId::Bsc, bsc.clone());
        let engine = Arc::new(MockEngine::default().with_account(7, 10000001, "live").with_account(7, 50000001, "demo").with_account(8, 10000002, "live"));
        let users = Arc::new(MockUsers::default());
        let st = AppState {
            tenants: Tenants::load(&pool).await.unwrap(),
            pool,
            cfg: Arc::new(cfg),
            chains: Arc::new(chains),
            engine: engine.clone(),
            users: users.clone(),
            transfer_lock: Default::default(),
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let app = wallet::api::router(st.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Some(T { st, base: base_url, http: reqwest::Client::new(), tron, bsc, engine, users, admin, name })
    }

    async fn call(&self, method: &str, path: &str, body: Option<Value>, staff_role: Option<&str>) -> (u16, Value) {
        let mut rb = match method {
            "GET" => self.http.get(format!("{}{path}", self.base)),
            "PUT" => self.http.put(format!("{}{path}", self.base)),
            _ => self.http.post(format!("{}{path}", self.base)),
        };
        rb = rb.header("x-kalks-internal", "test-internal-token").header("x-forwarded-for", "203.0.113.7");
        if let Some(role) = staff_role {
            rb = rb.header("x-kalks-staff-id", "5").header("x-kalks-staff-name", "Finance%20Desk").header("x-kalks-staff-role", role);
        }
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let r = rb.send().await.unwrap();
        let status = r.status().as_u16();
        (status, r.json().await.unwrap_or(Value::Null))
    }
    /// As a staff member whose BFF forwarded the gateway permission keys (`X-Kalks-Staff-Perms`).
    async fn with_perms(&self, role: &str, perms: &str, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        let mut rb = match method {
            "GET" => self.http.get(format!("{}{path}", self.base)),
            "PUT" => self.http.put(format!("{}{path}", self.base)),
            _ => self.http.post(format!("{}{path}", self.base)),
        };
        rb = rb
            .header("x-kalks-internal", "test-internal-token")
            .header("x-kalks-staff-id", "9")
            .header("x-kalks-staff-name", "Desk")
            .header("x-kalks-staff-role", role)
            .header("x-kalks-staff-perms", perms);
        if let Some(b) = body {
            rb = rb.json(&b);
        }
        let r = rb.send().await.unwrap();
        let status = r.status().as_u16();
        (status, r.json().await.unwrap_or(Value::Null))
    }
    async fn get(&self, path: &str) -> (u16, Value) {
        self.call("GET", path, None, None).await
    }
    async fn post(&self, path: &str, body: Value) -> (u16, Value) {
        self.call("POST", path, Some(body), None).await
    }
    async fn staff(&self, method: &str, path: &str, body: Option<Value>) -> (u16, Value) {
        self.call(method, path, body, Some("finance")).await
    }

    async fn balance(&self, user: i64) -> (String, String) {
        let (_, v) = self.get(&format!("/v1/wallets/{user}")).await;
        let b = &v["balances"][0];
        (b["available"].as_str().unwrap().to_string(), b["locked"].as_str().unwrap().to_string())
    }

    async fn invariants(&self) {
        let p = ledger::verify(&self.st.pool).await.unwrap();
        assert!(p.is_empty(), "ledger invariants: {p:?}");
    }

    async fn credit(&self, user: i64, amount: &str, key: &str) {
        let (s, v) = self.post("/v1/wallets/transfers", json!({"idempotency_key": key, "user_id": user, "currency": "USDT", "amount": amount, "direction": "credit", "kind": "commission"})).await;
        assert_eq!(s, 200, "{v}");
    }

    async fn drop_db(self) {
        self.st.pool.close().await;
        if let Ok(mut c) = self.admin.connect().await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))).execute(&mut c).await;
        }
    }
}

fn usdt_tx(chain: ChainId, from: &str, to: &str, amount: &str, block: i64) -> TxLookup {
    TxLookup {
        status: TxStatus::Success,
        block: Some(block),
        block_time: Some(chrono::Utc::now()),
        transfers: vec![TokenTransfer { log_index: Some(0), token: chain.usdt_contract().into(), from: from.into(), to: to.into(), amount: d(amount) }],
    }
}

#[tokio::test]
async fn transfer_contract_is_idempotent_and_balanced() {
    let Some(t) = T::new("transfer contract").await else { return };
    let body = json!({"idempotency_key": "ib:payout:1", "user_id": 42, "currency": "USDT", "amount": "100.25", "direction": "credit", "kind": "ib_payout", "ref": "P-1", "note": "September"});
    let (s, first) = t.post("/v1/wallets/transfers", body.clone()).await;
    assert_eq!(s, 200, "{first}");
    assert_eq!(first["replayed"], false);
    assert_eq!(first["balance"]["available"], "100.25");
    let (s, again) = t.post("/v1/wallets/transfers", body.clone()).await;
    assert_eq!(s, 200);
    assert_eq!(again["replayed"], true);
    assert_eq!(again["txn_id"], first["txn_id"]);
    // same key, different body
    let mut other = body.clone();
    other["amount"] = json!("100.26");
    let (s, v) = t.post("/v1/wallets/transfers", other).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("idempotency_conflict")));
    // debit beyond balance
    let (s, v) = t.post("/v1/wallets/transfers", json!({"idempotency_key": "prop:buy:1", "user_id": 42, "currency": "USDT", "amount": "150", "direction": "debit", "kind": "prop_purchase"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("insufficient_funds")));
    let (s, v) = t.post("/v1/wallets/transfers", json!({"idempotency_key": "prop:buy:2", "user_id": 42, "currency": "USDT", "amount": 40, "direction": "debit", "kind": "prop_purchase"})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(t.balance(42).await, ("60.25".into(), "0".into()));
    // validation
    for bad in [
        json!({"idempotency_key": "x1", "user_id": 42, "currency": "EUR", "amount": "1", "direction": "credit", "kind": "refund"}),
        json!({"idempotency_key": "x2", "user_id": 42, "currency": "USDT", "amount": "1.1234567", "direction": "credit", "kind": "refund"}),
        json!({"idempotency_key": "x3", "user_id": 42, "currency": "USDT", "amount": "-1", "direction": "credit", "kind": "refund"}),
        json!({"idempotency_key": "x4", "user_id": 42, "currency": "USDT", "amount": "1", "direction": "sideways", "kind": "refund"}),
        json!({"idempotency_key": "x5", "user_id": 42, "currency": "USDT", "amount": "1", "direction": "credit", "kind": "gift"}),
        json!({"idempotency_key": "", "user_id": 42, "currency": "USDT", "amount": "1", "direction": "credit", "kind": "refund"}),
    ] {
        let (s, v) = t.post("/v1/wallets/transfers", bad).await;
        assert_eq!(s, 422, "{v}");
    }
    // concurrent duplicates: exactly one booking
    let mut hs = vec![];
    for _ in 0..12 {
        let http = t.http.clone();
        let url = format!("{}/v1/wallets/transfers", t.base);
        hs.push(tokio::spawn(async move {
            http.post(url)
                .header("x-kalks-internal", "test-internal-token")
                .json(&json!({"idempotency_key": "race", "user_id": 42, "currency": "USDT", "amount": "5", "direction": "credit", "kind": "refund"}))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        }));
    }
    let mut txns = std::collections::HashSet::new();
    for h in hs {
        let v = h.await.unwrap();
        txns.insert(v["txn_id"].as_i64().expect("every duplicate gets the stored result"));
    }
    assert_eq!(txns.len(), 1);
    assert_eq!(t.balance(42).await.0, "65.25");
    // lookup + ledger listing
    let (s, v) = t.get("/v1/wallets/transfers/ib:payout:1").await;
    assert_eq!((s, v["amount"].as_str()), (200, Some("100.25")));
    let (_, v) = t.get("/v1/wallets/42/ledger?page=1&limit=10").await;
    assert_eq!(v["total"], 3);
    assert_eq!(v["items"][0]["kind"], "refund");
    // unknown user has a zero USDT balance
    assert_eq!(t.balance(999).await, ("0".into(), "0".into()));
    // internal token required
    let r = t.http.get(format!("{}/v1/wallets/42", t.base)).send().await.unwrap();
    assert_eq!(r.status().as_u16(), 403);
    // append-only
    assert!(sqlx::query("UPDATE ledger_postings SET amount = amount * 2").execute(&t.st.pool).await.is_err());
    assert!(sqlx::query("DELETE FROM ledger_txns").execute(&t.st.pool).await.is_err());
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn deposits_verify_confirm_and_credit() {
    let Some(t) = T::new("deposits").await else { return };
    // TRON: intent → submit → confirmations → credited
    let (s, v) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "tron", "amount": "50"})).await;
    assert_eq!(s, 200, "{v}");
    let intent = v["intent"].clone();
    assert_eq!(intent["address"], TRON_ADDR);
    assert_eq!(intent["token_contract"], "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t");
    let h1 = hash(1);
    t.tron.put(&h1, usdt_tx(ChainId::Tron, CLIENT_TRON, TRON_ADDR, "50", 1000));
    t.tron.set_head(1005);
    let (s, v) = t.post("/v1/deposits/submit", json!({"user_id": 7, "intent_id": intent["id"], "tx_hash": format!("0x{}", h1.to_uppercase())})).await;
    assert_eq!(s, 200, "{v}");
    let dep_id = v["deposit"]["id"].as_i64().unwrap();
    // idempotent re-submit of the same hash for the same intent
    let (s, _) = t.post("/v1/deposits/submit", json!({"user_id": 7, "intent_id": intent["id"], "tx_hash": h1})).await;
    assert_eq!(s, 200);
    watcher::tick(&t.st).await.unwrap();
    let (_, v) = t.get(&format!("/v1/deposits/{dep_id}?user_id=7")).await;
    assert_eq!(v["deposit"]["status"], "confirming");
    assert_eq!(v["deposit"]["confirmations"], 6);
    assert_eq!(v["deposit"]["required_confirmations"], 20);
    assert_eq!(v["deposit"]["from_address"], CLIENT_TRON);
    t.tron.set_head(1019);
    watcher::tick(&t.st).await.unwrap();
    let (_, v) = t.get(&format!("/v1/deposits/{dep_id}?user_id=7")).await;
    assert_eq!(v["deposit"]["status"], "credited", "{v}");
    assert_eq!(t.balance(7).await.0, "50");
    watcher::tick(&t.st).await.unwrap();
    assert_eq!(t.balance(7).await.0, "50", "credited once");
    let (_, n) = t.get("/v1/wallets/7/notifications").await;
    assert_eq!(n["items"][0]["kind"], "deposit.credited");

    // the same hash can't be claimed again (by anyone)
    let (_, v) = t.post("/v1/deposits/intents", json!({"user_id": 8, "chain": "tron", "amount": "50"})).await;
    let (s, v) = t.post("/v1/deposits/submit", json!({"user_id": 8, "intent_id": v["intent"]["id"], "tx_hash": h1})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("tx_already_used")));

    // BSC: amount differs from the intent → review → staff approve → credited
    let (_, v) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "bsc", "amount": "25"})).await;
    assert_eq!(v["intent"]["evm_chain_id"], 56);
    let h2 = format!("0x{}", hash(2));
    t.bsc.put(&h2, usdt_tx(ChainId::Bsc, "0x00000000000000000000000000000000000000aa", BSC_ADDR, "24.999999999999999999", 500));
    t.bsc.set_head(520);
    let (s, v) = t.post("/v1/deposits/submit", json!({"user_id": 7, "intent_id": v["intent"]["id"], "tx_hash": h2})).await;
    assert_eq!(s, 200, "{v}");
    let bsc_dep = v["deposit"]["id"].as_i64().unwrap();
    watcher::tick(&t.st).await.unwrap();
    let (_, v) = t.staff("GET", &format!("/v1/admin/deposits/{bsc_dep}"), None).await;
    assert_eq!(v["deposit"]["status"], "review", "{v}");
    assert!(v["deposit"]["review_reason"].as_str().unwrap().contains("differs"));
    let (s, v) = t.staff("POST", &format!("/v1/admin/deposits/{bsc_dep}/assign"), Some(json!({"user_id": 7, "reason": "Client confirmed the amount"}))).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["deposit"]["status"], "credited");
    assert_eq!(t.balance(7).await.0, "74.999999", "BEP20 amount truncated to 6 decimals");

    // wrong recipient / reverted
    for (n, lookup, why) in [
        (3, usdt_tx(ChainId::Tron, CLIENT_TRON, OTHER_TRON, "10", 1000), "does not send USDT"),
        (4, TxLookup { status: TxStatus::Failed, block: Some(1000), block_time: None, transfers: vec![] }, "failed"),
    ] {
        let (_, v) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "tron", "amount": "10"})).await;
        t.tron.put(&hash(n), lookup);
        let (_, v) = t.post("/v1/deposits/submit", json!({"user_id": 7, "intent_id": v["intent"]["id"], "tx_hash": hash(n)})).await;
        let id = v["deposit"]["id"].as_i64().unwrap();
        watcher::tick(&t.st).await.unwrap();
        let (_, v) = t.get(&format!("/v1/deposits/{id}")).await;
        assert_eq!(v["deposit"]["status"], "failed");
        assert!(v["deposit"]["failure_reason"].as_str().unwrap().contains(why), "{v}");
    }
    // a sender used by another client goes to review
    let (_, v) = t.post("/v1/deposits/intents", json!({"user_id": 8, "chain": "tron", "amount": "15"})).await;
    t.tron.put(&hash(5), usdt_tx(ChainId::Tron, CLIENT_TRON, TRON_ADDR, "15", 1010));
    let (_, v) = t.post("/v1/deposits/submit", json!({"user_id": 8, "intent_id": v["intent"]["id"], "tx_hash": hash(5)})).await;
    let id = v["deposit"]["id"].as_i64().unwrap();
    t.tron.set_head(1100);
    watcher::tick(&t.st).await.unwrap();
    let (_, v) = t.get(&format!("/v1/deposits/{id}")).await;
    assert_eq!(v["deposit"]["status"], "review");
    assert!(v["deposit"]["review_reason"].as_str().unwrap().contains("another client"));
    // validation
    let (s, _) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "eth", "amount": "10"})).await;
    assert_eq!(s, 422);
    let (s, v) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "tron", "amount": "1"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("below_minimum")));
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn scanner_finds_unmatched_and_staff_assign() {
    let Some(t) = T::new("scanner").await else { return };
    t.tron.set_head(2000);
    t.tron.incoming.lock().unwrap().push(Incoming { tx_hash: hash(77), log_index: None, from: OTHER_TRON.into(), to: TRON_ADDR.into(), amount: d("120"), block: None, time: Some(chrono::Utc::now()) });
    t.tron.put(&hash(77), usdt_tx(ChainId::Tron, OTHER_TRON, TRON_ADDR, "120", 1900));
    watcher::scan_all(&t.st).await;
    let (_, v) = t.staff("GET", "/v1/admin/deposits?status=queue", None).await;
    assert_eq!(v["total"], 1, "{v}");
    let id = v["items"][0]["id"].as_i64().unwrap();
    assert_eq!(v["items"][0]["status"], "unmatched");
    // unmatched deposits are never credited by the watcher
    watcher::tick(&t.st).await.unwrap();
    assert_eq!(t.balance(9).await.0, "0");
    // support can't assign
    let (s, _) = t.call("POST", &format!("/v1/admin/deposits/{id}/assign"), Some(json!({"user_id": 9, "reason": "x"})), Some("support")).await;
    assert_eq!(s, 403);
    let (s, v) = t.staff("POST", &format!("/v1/admin/deposits/{id}/assign"), Some(json!({"user_id": 9, "reason": "Matched by support ticket #12"}))).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["deposit"]["status"], "credited");
    assert_eq!(t.balance(9).await.0, "120");
    let (_, a) = t.staff("GET", "/v1/admin/audit", None).await;
    assert_eq!(a["items"][0]["action"], "wallet.deposit.assigned");
    assert_eq!(a["items"][0]["actor_name"], "Finance Desk");

    // a later transfer from the same sender with an open request of that amount is claimed automatically
    let (_, iv) = t.post("/v1/deposits/intents", json!({"user_id": 9, "chain": "tron", "amount": "30"})).await;
    t.tron.incoming.lock().unwrap().push(Incoming { tx_hash: hash(78), log_index: None, from: OTHER_TRON.into(), to: TRON_ADDR.into(), amount: d("30"), block: None, time: Some(chrono::Utc::now()) });
    t.tron.put(&hash(78), usdt_tx(ChainId::Tron, OTHER_TRON, TRON_ADDR, "30", 1995));
    watcher::scan_all(&t.st).await;
    t.tron.set_head(2100);
    watcher::tick(&t.st).await.unwrap();
    assert_eq!(t.balance(9).await.0, "150");
    let (_, v) = t.get(&format!("/v1/deposits/intents/{}?user_id=9", iv["intent"]["id"].as_str().unwrap())).await;
    assert_eq!(v["intent"]["status"], "completed");
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn withdrawals_lifecycle() {
    let Some(t) = T::new("withdrawals").await else { return };
    t.credit(7, "500", "seed-7").await;
    let req = |amount: &str, key: &str| json!({"user_id": 7, "amount": amount, "chain": "tron", "to_address": CLIENT_TRON, "idempotency_key": key});
    // KYC gate
    t.users.set(7, "pending");
    let (s, v) = t.post("/v1/withdrawals", req("100", "w1")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("kyc_required")));
    t.users.set(7, "verified");
    // bad address / company address
    let (s, _) = t.post("/v1/withdrawals", json!({"user_id": 7, "amount": "100", "chain": "tron", "to_address": "TXYZ"})).await;
    assert_eq!(s, 422);
    let (s, _) = t.post("/v1/withdrawals", json!({"user_id": 7, "amount": "100", "chain": "tron", "to_address": TRON_ADDR})).await;
    assert_eq!(s, 422);
    // limits
    let (s, v) = t.post("/v1/withdrawals", req("5", "w-min")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("below_minimum")));
    let (s, v) = t.post("/v1/withdrawals", req("600", "w-max")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("insufficient_funds")), "{v}");

    // quote: every check, nothing locked
    let (s, v) = t.post("/v1/withdrawals/quote", req("100", "q")).await;
    assert_eq!((s, v["quote"]["net_amount"].as_str()), (200, Some("99")), "{v}");
    assert_eq!(t.balance(7).await, ("500".into(), "0".into()));
    // request → locked; idempotent
    let (s, v) = t.post("/v1/withdrawals", req("100", "w1")).await;
    assert_eq!(s, 200, "{v}");
    let w1 = v["withdrawal"]["id"].as_i64().unwrap();
    assert_eq!(v["withdrawal"]["fee"], "1");
    assert_eq!(v["withdrawal"]["net_amount"], "99");
    let (_, again) = t.post("/v1/withdrawals", req("100", "w1")).await;
    assert_eq!(again["withdrawal"]["id"], w1);
    assert_eq!(t.balance(7).await, ("400".into(), "100".into()));

    // cancel by client unlocks
    let (_, v) = t.post("/v1/withdrawals", req("50", "w2")).await;
    let w2 = v["withdrawal"]["id"].as_i64().unwrap();
    let (s, v) = t.post(&format!("/v1/withdrawals/{w2}/cancel"), json!({"user_id": 7})).await;
    assert_eq!((s, v["withdrawal"]["status"].as_str()), (200, Some("cancelled")));
    assert_eq!(t.balance(7).await, ("400".into(), "100".into()));

    // reject by staff unlocks
    let (_, v) = t.post("/v1/withdrawals", req("60", "w3")).await;
    let w3 = v["withdrawal"]["id"].as_i64().unwrap();
    let (s, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w3}/reject"), Some(json!({"reason": "Address belongs to an exchange we can't pay"}))).await;
    assert_eq!((s, v["withdrawal"]["status"].as_str()), (200, Some("rejected")));
    assert_eq!(t.balance(7).await, ("400".into(), "100".into()));

    // risk checklist, approve, paid with a wrong payout → back to approved, then the right one → completed
    let (_, v) = t.staff("GET", &format!("/v1/admin/withdrawals/{w1}"), None).await;
    assert_eq!(v["risk"]["kyc"]["ok"], true);
    assert_eq!(v["risk"]["ip"]["address"], "203.0.113.7");
    let (s, _) = t.call("POST", &format!("/v1/admin/withdrawals/{w1}/approve"), Some(json!({})), Some("compliance")).await;
    assert_eq!(s, 403, "compliance can read but not approve");
    let (s, _) = t.staff("POST", &format!("/v1/admin/withdrawals/{w1}/paid"), Some(json!({"tx_hash": hash(90)}))).await;
    assert_eq!(s, 409, "must be approved first");
    let (s, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w1}/approve"), Some(json!({"note": "ok"}))).await;
    assert_eq!((s, v["withdrawal"]["status"].as_str()), (200, Some("approved")));
    t.tron.put(&hash(90), usdt_tx(ChainId::Tron, TRON_ADDR, CLIENT_TRON, "100", 3000)); // gross instead of net
    t.tron.set_head(3030);
    let (s, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w1}/paid"), Some(json!({"tx_hash": hash(90)}))).await;
    assert_eq!(s, 200);
    assert_eq!(v["withdrawal"]["status"], "approved", "{v}");
    assert!(v["withdrawal"]["payout_error"].as_str().unwrap().contains("exactly 99"));
    t.tron.put(&hash(91), usdt_tx(ChainId::Tron, TRON_ADDR, CLIENT_TRON, "99", 3010));
    t.tron.set_head(3015);
    let (_, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w1}/paid"), Some(json!({"tx_hash": hash(91)}))).await;
    assert_eq!(v["withdrawal"]["status"], "paid");
    assert_eq!(v["withdrawal"]["payout_confirmations"], 6);
    t.tron.set_head(3040);
    watcher::tick(&t.st).await.unwrap();
    let (_, v) = t.staff("GET", &format!("/v1/admin/withdrawals/{w1}"), None).await;
    assert_eq!(v["withdrawal"]["status"], "completed", "{v}");
    assert_eq!(t.balance(7).await, ("400".into(), "0".into()));
    // the payout hash can't be reused
    let (_, v) = t.post("/v1/withdrawals", req("20", "w4")).await;
    let w4 = v["withdrawal"]["id"].as_i64().unwrap();
    t.staff("POST", &format!("/v1/admin/withdrawals/{w4}/approve"), Some(json!({}))).await;
    let (s, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w4}/paid"), Some(json!({"tx_hash": hash(91)}))).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("tx_already_used")));

    // settings: daily limit and post-deposit cooldown
    let (s, v) = t.call("PUT", "/v1/admin/settings", Some(json!({"limits": {"withdraw_daily_max": "150"}, "reason": "Tighter daily limit"})), Some("admin")).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = t.post("/v1/withdrawals", req("40", "w5")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("daily_limit")), "{v}");
    sqlx::query("INSERT INTO deposits (tenant_id, user_id, chain, tx_hash, amount, required_confirmations, status, source, credited_at) VALUES (1, 7, 'tron', 'ff', 1, 20, 'credited', 'client', now())")
        .execute(&t.st.pool)
        .await
        .unwrap();
    let (s, v) = t.post("/v1/withdrawals", req("10", "w6")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("deposit_cooldown")), "{v}");
    // settings are audited with before / after
    let (_, v) = t.staff("GET", "/v1/admin/settings", None).await;
    assert_eq!(v["history"][0]["action"], "wallet.settings.limits");
    assert_eq!(v["history"][0]["before"]["withdraw_daily_max"], "100000");
    // only admins change settings
    let (s, _) = t.staff("PUT", "/v1/admin/settings", Some(json!({"chains": [{"chain": "tron", "receiving_address": OTHER_TRON}], "reason": "rotate"}))).await;
    assert_eq!(s, 403);
    let (s, v) = t.call("PUT", "/v1/admin/settings", Some(json!({"chains": [{"chain": "tron", "receiving_address": OTHER_TRON}], "reason": "rotate"})), Some("super_admin")).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["history"][0]["action"], "wallet.settings.address");
    let (s, _) = t.call("PUT", "/v1/admin/settings", Some(json!({"chains": [{"chain": "bsc", "receiving_address": "0x1234"}], "reason": "bad"})), Some("super_admin")).await;
    assert_eq!(s, 422);
    // reconciliation
    let (s, v) = t.staff("GET", "/v1/admin/reconciliation", None).await;
    assert_eq!(s, 200);
    assert_eq!(v["invariants"]["ok"], true, "{v}");
    let tron = v["chains"].as_array().unwrap().iter().find(|c| c["chain"] == "tron").unwrap().clone();
    assert_eq!(tron["withdrawals_paid"], "99");
    assert_eq!(tron["ledger_paid_out"], "99");
    assert_eq!(v["fees_earned"], "1");
    t.invariants().await;
    t.drop_db().await;
}

/// Reject returns the locked funds, so it is only allowed before approval: once approved the payout may already
/// have been sent by hand (even when its on-chain check failed), and a reject would refund the client twice.
#[tokio::test]
async fn withdrawals_cannot_be_rejected_after_approval() {
    let Some(t) = T::new("withdrawal reject states").await else { return };
    t.credit(7, "500", "seed-7").await;
    t.users.set(7, "verified");
    let req = |amount: &str, key: &str| json!({"user_id": 7, "amount": amount, "chain": "tron", "to_address": CLIENT_TRON, "idempotency_key": key});
    let tr = &t;
    let reject = |id: i64| async move { tr.staff("POST", &format!("/v1/admin/withdrawals/{id}/reject"), Some(json!({"reason": "Changed our mind"}))).await };

    // approved: payout sent but its check failed (wrong amount) → back to approved; reject is refused
    let (_, v) = t.post("/v1/withdrawals", req("100", "a")).await;
    let w = v["withdrawal"]["id"].as_i64().unwrap();
    t.staff("POST", &format!("/v1/admin/withdrawals/{w}/approve"), Some(json!({}))).await;
    let (s, v) = reject(w).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("invalid_state")), "{v}");
    t.tron.put(&hash(70), usdt_tx(ChainId::Tron, TRON_ADDR, CLIENT_TRON, "100", 3000));
    t.tron.set_head(3030);
    let (_, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w}/paid"), Some(json!({"tx_hash": hash(70)}))).await;
    assert_eq!(v["withdrawal"]["status"], "approved", "{v}");
    let (s, v) = reject(w).await;
    assert_eq!((s, v["error"]["code"].as_str()), (409, Some("invalid_state")), "{v}");
    // paid (verifying) and completed: refused too
    t.tron.put(&hash(71), usdt_tx(ChainId::Tron, TRON_ADDR, CLIENT_TRON, "99", 3040));
    t.tron.set_head(3041);
    let (_, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{w}/paid"), Some(json!({"tx_hash": hash(71)}))).await;
    assert_eq!(v["withdrawal"]["status"], "paid", "{v}");
    let (s, _) = reject(w).await;
    assert_eq!(s, 409);
    t.tron.set_head(3100);
    watcher::tick(&t.st).await.unwrap();
    let (s, _) = reject(w).await;
    assert_eq!(s, 409);
    assert_eq!(t.balance(7).await, ("400".into(), "0".into()), "no refund on top of the payout");

    // racing approve and reject on one request: exactly one wins, the funds are released at most once
    for i in 0..5 {
        let (_, v) = t.post("/v1/withdrawals", req("10", &format!("race-{i}"))).await;
        let w = v["withdrawal"]["id"].as_i64().unwrap();
        let approve = format!("/v1/admin/withdrawals/{w}/approve");
        let ((sa, _), (sr, _)) = tokio::join!(t.staff("POST", &approve, Some(json!({}))), reject(w));
        assert!((sa == 200) ^ (sr == 200), "approve {sa}, reject {sr}");
        assert!(sa == 409 || sr == 409);
        let (s, _) = reject(w).await;
        assert_eq!(s, 409, "a second reject never refunds again");
    }
    t.invariants().await;
    t.drop_db().await;
}

/// Back Office routes check the forwarded permission keys, so a custom role (sent with the covering built-in role
/// name from the gateway) gets exactly what it was granted; the role decides only when no keys were forwarded.
#[tokio::test]
async fn back_office_routes_check_permission_keys() {
    let Some(t) = T::new("permission keys").await else { return };
    t.credit(7, "500", "seed-7").await;
    t.users.set(7, "verified");
    let req = |key: &str| json!({"user_id": 7, "amount": "20", "chain": "tron", "to_address": CLIENT_TRON, "idempotency_key": key});
    let (_, v) = t.post("/v1/withdrawals", req("p1")).await;
    let w1 = v["withdrawal"]["id"].as_i64().unwrap();
    let (_, v) = t.post("/v1/withdrawals", req("p2")).await;
    let w2 = v["withdrawal"]["id"].as_i64().unwrap();

    // a custom "Payouts desk" role: read + approve, nothing else (sent as compliance, which can't approve by role)
    let desk = "finance.read,finance.approve";
    let (s, v) = t.with_perms("compliance", desk, "GET", "/v1/admin/withdrawals", None).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = t.with_perms("compliance", desk, "POST", &format!("/v1/admin/withdrawals/{w1}/approve"), Some(json!({}))).await;
    assert_eq!((s, v["withdrawal"]["status"].as_str()), (200, Some("approved")), "{v}");
    let (s, v) = t.with_perms("compliance", desk, "POST", &format!("/v1/admin/withdrawals/{w2}/reject"), Some(json!({"reason": "Wrong network"}))).await;
    assert_eq!(s, 200, "{v}");
    // ... but not mark paid (finance.write) or settings (finance.settings)
    let (s, _) = t.with_perms("compliance", desk, "POST", &format!("/v1/admin/withdrawals/{w1}/paid"), Some(json!({"tx_hash": hash(80)}))).await;
    assert_eq!(s, 403);
    let (s, _) = t.with_perms("compliance", desk, "PUT", "/v1/admin/settings", Some(json!({"limits": {"withdraw_daily_max": "900"}, "reason": "custom"}))).await;
    assert_eq!(s, 403);

    // the role name no longer grants what the keys don't: "finance" holding only finance.read
    let (_, v) = t.post("/v1/withdrawals", req("p3")).await;
    let w3 = v["withdrawal"]["id"].as_i64().unwrap();
    let (s, _) = t.with_perms("finance", "finance.read", "POST", &format!("/v1/admin/withdrawals/{w3}/approve"), Some(json!({}))).await;
    assert_eq!(s, 403);
    let (s, _) = t.with_perms("finance", "finance.read", "POST", "/v1/admin/deposits/1/reject", Some(json!({"reason": "nope"}))).await;
    assert_eq!(s, 403);
    let (s, _) = t.with_perms("finance", "-", "GET", "/v1/admin/summary", None).await;
    assert_eq!(s, 403, "no finance keys at all");

    // a custom settings role: finance.settings works whatever the role name
    let (s, v) = t.with_perms("compliance", "finance.read,finance.settings", "PUT", "/v1/admin/settings", Some(json!({"limits": {"withdraw_daily_max": "900"}, "reason": "custom"}))).await;
    assert_eq!(s, 200, "{v}");
    let (s, v) = t.with_perms("compliance", "finance.read", "GET", "/v1/admin/reconciliation", None).await;
    assert_eq!(s, 200, "{v}");

    // Super Admin / Platform Owner keep full access
    let (s, v) = t.with_perms("super_admin", "-", "POST", &format!("/v1/admin/withdrawals/{w3}/approve"), Some(json!({}))).await;
    assert_eq!(s, 200, "{v}");
    let (s, _) = t.with_perms("platform_owner", "-", "GET", "/v1/admin/settings", None).await;
    assert_eq!(s, 200);
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn wallet_trading_transfers_are_two_phase_safe() {
    let Some(t) = T::new("trading transfers").await else { return };
    t.credit(7, "300", "seed").await;
    let to = |login: i64, amount: &str, key: &str| json!({"login": login, "amount": amount, "idempotency_key": key});
    // not own / demo account
    let (s, v) = t.post("/v1/wallets/7/to-trading", to(10000002, "10", "a")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("not_own_account")));
    let (s, _) = t.post("/v1/wallets/7/to-trading", to(50000001, "10", "b")).await;
    assert_eq!(s, 422);
    let (s, _) = t.post("/v1/wallets/7/to-trading", to(10000001, "10.001", "c")).await;
    assert_eq!(s, 422, "engine precision is cents");
    // success
    let (s, v) = t.post("/v1/wallets/7/to-trading", to(10000001, "100", "t1")).await;
    assert_eq!((s, v["transfer"]["status"].as_str()), (200, Some("completed")), "{v}");
    assert_eq!(t.balance(7).await, ("200".into(), "0".into()));
    let (_, again) = t.post("/v1/wallets/7/to-trading", to(10000001, "100", "t1")).await;
    assert_eq!(again["transfer"]["id"], v["transfer"]["id"]);
    let (s, _) = t.post("/v1/wallets/7/to-trading", to(10000001, "101", "t1")).await;
    assert_eq!(s, 409);
    // engine refuses → released
    t.engine.set_mode("reject");
    let (s, v) = t.post("/v1/wallets/7/from-trading", to(10000001, "50", "f-rej")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("insufficient_funds")));
    let (s, _) = t.post("/v1/wallets/7/to-trading", to(10000001, "20", "t-rej")).await;
    assert_eq!(s, 422);
    assert_eq!(t.balance(7).await, ("200".into(), "0".into()));
    // engine booked it but the answer was lost → pending (reserved) → recovery commits
    t.engine.set_mode("timeout_after");
    let (s, v) = t.post("/v1/wallets/7/to-trading", to(10000001, "30", "t-lost")).await;
    assert_eq!((s, v["transfer"]["status"].as_str()), (200, Some("pending")));
    assert_eq!(t.balance(7).await, ("170".into(), "30".into()));
    t.engine.set_mode("ok");
    wallet::ops::trading::recover(&t.st, -1).await.unwrap();
    assert_eq!(t.balance(7).await, ("170".into(), "0".into()));
    // the request never reached the engine → recovery re-sends it (same key) → completed once
    t.engine.set_mode("timeout_before");
    let (_, v) = t.post("/v1/wallets/7/from-trading", to(10000001, "45.5", "f-lost")).await;
    assert_eq!(v["transfer"]["status"], "pending");
    assert_eq!(t.balance(7).await.0, "170");
    t.engine.set_mode("ok");
    wallet::ops::trading::recover(&t.st, -1).await.unwrap();
    wallet::ops::trading::recover(&t.st, -1).await.unwrap();
    assert_eq!(t.balance(7).await.0, "215.5");
    let booked = t.engine.booked.lock().unwrap().len();
    assert_eq!(booked, 3, "t1, t-lost, f-lost booked exactly once each");
    // insufficient wallet balance
    let (s, v) = t.post("/v1/wallets/7/to-trading", to(10000001, "1000", "t-big")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (422, Some("insufficient_funds")));
    let (_, v) = t.get("/v1/wallets/7/activity?type=transfer").await;
    assert_eq!(v["total"], 5, "{v}");
    let (_, v) = t.get("/v1/wallets/7/overview").await;
    assert_eq!(v["balances"][0]["available"], "215.5");
    t.invariants().await;
    t.drop_db().await;
}

/// Mock support service: records every `POST /v1/notify`; answers 503 while `down` is set.
#[derive(Default)]
struct MockSupport {
    calls: std::sync::Mutex<Vec<(String, Value)>>,
    down: std::sync::atomic::AtomicBool,
}

async fn mock_support() -> (Arc<MockSupport>, String) {
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    let m = Arc::new(MockSupport::default());
    let app = axum::Router::new()
        .route(
            "/v1/notify",
            axum::routing::post(|State(m): State<Arc<MockSupport>>, h: HeaderMap, axum::Json(b): axum::Json<Value>| async move {
                if m.down.load(std::sync::atomic::Ordering::SeqCst) {
                    return (StatusCode::SERVICE_UNAVAILABLE, axum::Json(json!({})));
                }
                let tenant = h.get("x-kalks-tenant").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                m.calls.lock().unwrap().push((tenant, b));
                (StatusCode::OK, axum::Json(json!({"results": []})))
            }),
        )
        .with_state(m.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (m, url)
}

#[tokio::test]
async fn notifications_are_pushed_to_support_once() {
    let Some(t) = T::new("support push").await else { return };
    let (m, url) = mock_support().await;
    let mut st = t.st.clone();
    let mut cfg = (*st.cfg).clone();
    cfg.support_url = url;
    cfg.support_token = "support-token".into();
    st.cfg = Arc::new(cfg);
    let http = reqwest::Client::new();
    // a withdrawal request, then a staff rejection: two wallet notifications
    t.credit(7, "100", "seed-n").await;
    t.users.set(7, "verified");
    let (s, v) = t.post("/v1/withdrawals", json!({"user_id": 7, "amount": "40", "chain": "tron", "to_address": CLIENT_TRON, "idempotency_key": "w-n"})).await;
    assert_eq!(s, 200, "{v}");
    let id = v["withdrawal"]["id"].as_i64().unwrap();
    // support is down: nothing is lost, the rows back off
    m.down.store(true, std::sync::atomic::Ordering::SeqCst);
    let (sent, failed) = wallet::notifier::push_pending(&st, &http).await.unwrap();
    assert_eq!((sent, failed), (0, 1));
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications WHERE pushed_at IS NULL").fetch_one(&st.pool).await.unwrap();
    assert!(pending >= 1, "the withdrawal request waits for delivery");
    m.down.store(false, std::sync::atomic::Ordering::SeqCst);
    sqlx::query("UPDATE notifications SET push_next_at = NULL").execute(&st.pool).await.unwrap();
    let (s, v) = t.staff("POST", &format!("/v1/admin/withdrawals/{id}/reject"), Some(json!({"reason": "Address on a sanctions list"}))).await;
    assert_eq!(s, 200, "{v}");
    let (sent, failed) = wallet::notifier::push_pending(&st, &http).await.unwrap();
    assert_eq!(failed, 0);
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM notifications").fetch_one(&st.pool).await.unwrap();
    assert!(total >= 2);
    assert_eq!(sent as i64, total, "every notification delivered");
    // a second pass sends nothing again
    assert_eq!(wallet::notifier::push_pending(&st, &http).await.unwrap(), (0, 0));
    let calls = m.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), sent);
    let rejected = calls.iter().find(|(_, b)| b["type"] == "wallet.withdrawal_rejected").expect("rejection pushed");
    assert_eq!(rejected.0, "kalks");
    assert_eq!(rejected.1["userId"], 7);
    assert_eq!(rejected.1["link"], "/wallet/history");
    assert_eq!(rejected.1["severity"], "warning");
    assert_eq!(rejected.1["data"]["withdrawal_id"], id);
    let nid: i64 = sqlx::query_scalar("SELECT id FROM notifications WHERE kind = 'withdrawal.rejected'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(rejected.1["dedupeKey"], format!("wallet:n:{nid}"), "same key as support's polling adapter");
    t.drop_db().await;
}

#[tokio::test]
async fn back_office_restrictions_are_enforced() {
    let Some(t) = T::new("restrictions").await else { return };
    t.users.set(7, "verified");
    t.credit(7, "500", "seed-r7").await;
    let wd = |key: &str| json!({"user_id": 7, "amount": "100", "chain": "tron", "to_address": CLIENT_TRON, "idempotency_key": key});
    let to = |amount: &str, key: &str| json!({"login": 10000001, "amount": amount, "idempotency_key": key});

    // withdrawals disabled: request and quote are refused with a readable reason; nothing is locked
    t.users.restrict(7, &["withdrawals"]);
    let (s, v) = t.post("/v1/withdrawals", wd("r-w1")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("restricted")), "{v}");
    assert!(v["error"]["message"].as_str().unwrap().contains("Withdrawals are disabled"));
    let (s, _) = t.post("/v1/withdrawals/quote", wd("r-wq")).await;
    assert_eq!(s, 403);
    assert_eq!(t.balance(7).await, ("500".into(), "0".into()));
    // deposits still work while only withdrawals are disabled
    let (s, _) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "tron", "amount": "50"})).await;
    assert_eq!(s, 200);

    // deposits disabled
    t.users.restrict(7, &["deposits"]);
    let (s, v) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "tron", "amount": "50"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("restricted")));
    // wallet <-> trading transfers disabled, both ways
    t.users.restrict(7, &["transfers"]);
    let (s, v) = t.post("/v1/wallets/7/to-trading", to("100", "r-t1")).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("restricted")));
    let (s, _) = t.post("/v1/wallets/7/from-trading", to("10", "r-f1")).await;
    assert_eq!(s, 403);
    // IB commissions and payouts on hold; other credits still go through
    t.users.restrict(7, &["ib"]);
    let (s, v) = t.post("/v1/wallets/transfers", json!({"idempotency_key": "r-ib1", "user_id": 7, "currency": "USDT", "amount": "25", "direction": "credit", "kind": "ib_payout"})).await;
    assert_eq!((s, v["error"]["code"].as_str()), (403, Some("restricted")));
    let (s, _) = t.post("/v1/wallets/transfers", json!({"idempotency_key": "r-ib2", "user_id": 7, "currency": "USDT", "amount": "25", "direction": "credit", "kind": "refund"})).await;
    assert_eq!(s, 200);
    // lifted: everything works again
    t.users.restrict(7, &[]);
    let (s, v) = t.post("/v1/wallets/7/to-trading", to("100", "r-t2")).await;
    assert_eq!((s, v["transfer"]["status"].as_str()), (200, Some("completed")), "{v}");
    let (s, v) = t.post("/v1/withdrawals", wd("r-w2")).await;
    assert_eq!(s, 200, "{v}");
    t.invariants().await;
    t.drop_db().await;
}

#[tokio::test]
async fn a_new_broker_gets_its_wallet_rows_on_its_first_request() {
    let Some(t) = T::new("broker provisioning").await else { return };
    t.users.1.lock().unwrap().insert("qa-northwind".into(), (77_001, "QA Northwind".into()));
    let call = |slug: &'static str| {
        let (http, base) = (t.http.clone(), t.base.clone());
        async move {
            let r = http.get(format!("{base}/v1/admin/settings")).header("x-kalks-internal", "test-internal-token").header("x-kalks-tenant", slug)
                .header("x-kalks-staff-id", "5").header("x-kalks-staff-name", "Owner").header("x-kalks-staff-role", "super_admin").send().await.unwrap();
            let s = r.status().as_u16();
            (s, r.json::<Value>().await.unwrap_or(Value::Null))
        }
    };
    // a broker the gateway knows: provisioned with the gateway's id, default limits and no company addresses
    let (s, v) = call("qa-northwind").await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(t.st.tenants.get("qa-northwind"), Some(77_001));
    let addresses: i64 = sqlx::query_scalar("SELECT count(*) FROM chain_settings WHERE tenant_id = 77001").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(addresses, 0, "another broker's receiving addresses are never copied");
    let limits: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_settings WHERE tenant_id = 77001").fetch_one(&t.st.pool).await.unwrap();
    assert_eq!(limits, 1);
    // the next request finds it without the gateway
    let (s, _) = call("qa-northwind").await;
    assert_eq!(s, 200);
    // a broker the gateway doesn't know is refused
    let (s, _) = call("qa-nobody").await;
    assert_eq!(s, 400);
    t.drop_db().await;
}

/// Mock IB service: records every `POST /v1/ib/events/deposit` with its headers; answers 503 while `down` is
/// set and 404 for users in `unknown` (not mirrored yet); replays by idempotency key like the real service.
#[derive(Default)]
struct MockIb {
    calls: std::sync::Mutex<Vec<(String, String, Value)>>,
    down: std::sync::atomic::AtomicBool,
    unknown: std::sync::Mutex<Vec<i64>>,
}

async fn mock_ib() -> (Arc<MockIb>, String) {
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    let m = Arc::new(MockIb::default());
    let app = axum::Router::new()
        .route(
            "/v1/ib/events/deposit",
            axum::routing::post(|State(m): State<Arc<MockIb>>, h: HeaderMap, axum::Json(b): axum::Json<Value>| async move {
                if m.down.load(std::sync::atomic::Ordering::SeqCst) {
                    return (StatusCode::SERVICE_UNAVAILABLE, axum::Json(json!({})));
                }
                if m.unknown.lock().unwrap().contains(&b["userId"].as_i64().unwrap_or_default()) {
                    return (StatusCode::NOT_FOUND, axum::Json(json!({"error": {"code": "not_found"}})));
                }
                let hv = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                let mut calls = m.calls.lock().unwrap();
                let seen = calls.iter().any(|(_, _, c)| c["idempotencyKey"] == b["idempotencyKey"]);
                let first = !calls.iter().any(|(_, _, c)| c["userId"] == b["userId"]);
                calls.push((hv("x-kalks-tenant"), hv("x-kalks-internal"), b));
                (StatusCode::OK, axum::Json(json!({"status": if seen { "duplicate" } else { "recorded" }, "firstDeposit": first})))
            }),
        )
        .with_state(m.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    (m, url)
}

#[tokio::test]
async fn real_deposits_are_reported_to_the_ib_service_once() {
    let Some(t) = T::new("ib deposit events").await else { return };
    let (m, url) = mock_ib().await;
    let http = reqwest::Client::new();
    let count = |sql: &'static str| {
        let pool = t.st.pool.clone();
        async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(&pool).await.unwrap() }
    };
    // not configured (development): events are queued, nothing is sent, the deposit is unaffected
    assert!(t.st.cfg.ib_url.is_empty());
    assert_eq!(wallet::ib::push_pending(&t.st, &http).await.unwrap(), (0, 0));

    // 1. on-chain deposit credited
    let (_, v) = t.post("/v1/deposits/intents", json!({"user_id": 7, "chain": "tron", "amount": "50"})).await;
    t.tron.put(&hash(11), usdt_tx(ChainId::Tron, CLIENT_TRON, TRON_ADDR, "50", 1000));
    t.tron.set_head(1100);
    let (s, v) = t.post("/v1/deposits/submit", json!({"user_id": 7, "intent_id": v["intent"]["id"], "tx_hash": hash(11)})).await;
    assert_eq!(s, 200, "{v}");
    watcher::tick(&t.st).await.unwrap();
    assert_eq!(t.balance(7).await.0, "50");
    // 2. Back Office external deposit (manual_deposit) to the wallet
    let adj = |key: &str, category: &str, amount: &str| {
        json!({"idempotency_key": key, "user_id": 8, "target": "wallet", "currency": "USDT", "op": "add", "category": category, "amount": amount, "comment": "Bank wire ref 4471", "notify": false})
    };
    let (s, v) = t.staff("POST", "/v1/admin/adjustments", Some(adj("ib-a1", "deposit", "700"))).await;
    assert_eq!((s, v["adjustment"]["ledger_kind"].as_str()), (200, Some("manual_deposit")), "{v}");
    // not deposits: a bonus / compensation adjustment, a commission and an IB payout credit, money back from trading
    let (s, _) = t.staff("POST", "/v1/admin/adjustments", Some(adj("ib-a2", "bonus", "20"))).await;
    assert_eq!(s, 200);
    t.credit(7, "5", "ib-c1").await;
    let (s, _) = t.post("/v1/wallets/transfers", json!({"idempotency_key": "ib:payout:x", "user_id": 7, "currency": "USDT", "amount": "9", "direction": "credit", "kind": "ib_payout"})).await;
    assert_eq!(s, 200);
    let (s, v) = t.post("/v1/wallets/7/from-trading", json!({"login": 10000001, "amount": "3", "idempotency_key": "ft-ib"})).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(count("SELECT count(*) FROM ib_deposit_events").await, 2, "only the two real deposits are queued");
    assert_eq!(count("SELECT count(*) FROM ib_deposit_events WHERE (kind, amount) IN (('deposit', 50), ('manual_deposit', 700))").await, 2);

    // configured: the IB service is down → nothing lost, the row backs off
    let mut st = t.st.clone();
    let mut cfg = (*st.cfg).clone();
    cfg.ib_url = url;
    cfg.ib_token = "ib-token".into();
    st.cfg = Arc::new(cfg);
    m.down.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(wallet::ib::push_pending(&st, &http).await.unwrap(), (0, 1));
    assert_eq!(count("SELECT count(*) FROM ib_deposit_events WHERE pushed_at IS NULL AND push_next_at > now()").await, 1);
    // back up; client 8 isn't mirrored in the IB service yet (404): retried later, client 7 still goes through
    m.down.store(false, std::sync::atomic::Ordering::SeqCst);
    m.unknown.lock().unwrap().push(8);
    sqlx::query("UPDATE ib_deposit_events SET push_next_at = NULL").execute(&st.pool).await.unwrap();
    assert_eq!(wallet::ib::push_pending(&st, &http).await.unwrap(), (1, 1));
    m.unknown.lock().unwrap().clear();
    sqlx::query("UPDATE ib_deposit_events SET push_next_at = NULL").execute(&st.pool).await.unwrap();
    assert_eq!(wallet::ib::push_pending(&st, &http).await.unwrap(), (1, 0));
    assert_eq!(wallet::ib::push_pending(&st, &http).await.unwrap(), (0, 0), "each event is delivered once");
    let calls = m.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    let (tenant, token, onchain) = calls.iter().find(|(_, _, b)| b["userId"] == 7).unwrap().clone();
    assert_eq!((tenant.as_str(), token.as_str()), ("kalks", "ib-token"));
    let txn: i64 = sqlx::query_scalar("SELECT txn_id FROM ib_deposit_events WHERE kind = 'deposit'").fetch_one(&st.pool).await.unwrap();
    let tenant_id: i64 = sqlx::query_scalar("SELECT tenant_id FROM ib_deposit_events WHERE kind = 'deposit'").fetch_one(&st.pool).await.unwrap();
    assert_eq!(onchain["idempotencyKey"], format!("wallet:deposit:{tenant_id}:{txn}"), "key derived from the wallet ledger txn");
    assert_eq!((onchain["amount"].as_str(), onchain["currency"].as_str(), onchain["source"].as_str()), (Some("50"), Some("USDT"), Some("wallet")));
    let at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT created_at FROM ledger_txns WHERE id = $1").bind(txn).fetch_one(&st.pool).await.unwrap();
    assert_eq!(onchain["at"].as_str().unwrap().parse::<chrono::DateTime<chrono::Utc>>().unwrap(), at, "the ledger time, stable across retries");
    let manual = &calls.iter().find(|(_, _, b)| b["userId"] == 8).unwrap().2;
    assert_eq!((manual["amount"].as_str(), manual["kind"].as_str()), (Some("700"), Some("manual_deposit")));
    assert_eq!(count("SELECT count(*) FROM ib_deposit_events WHERE pushed_at IS NOT NULL AND first_deposit").await, 2);

    // reconciliation: a real deposit without an outbox row (booked before the outbox existed) is queued by the
    // sweep, with the same idempotency key, and the sweep is a no-op afterwards
    sqlx::query("DELETE FROM ib_deposit_events WHERE kind = 'manual_deposit'").execute(&st.pool).await.unwrap();
    assert_eq!(wallet::ib::sweep(&st.pool).await.unwrap(), 1);
    assert_eq!(wallet::ib::sweep(&st.pool).await.unwrap(), 0);
    assert_eq!(wallet::ib::push_pending(&st, &http).await.unwrap(), (1, 0));
    let calls = m.calls.lock().unwrap().clone();
    assert_eq!(calls[2].2["idempotencyKey"], calls.iter().find(|(_, _, b)| b["userId"] == 8).unwrap().2["idempotencyKey"], "a resend reuses the key: the IB answers duplicate");
    let (_, v) = t.staff("GET", "/v1/admin/reconciliation", None).await;
    assert_eq!(v["ib_deposit_events"]["queued"], 0, "{v}");
    t.invariants().await;
    t.drop_db().await;
}

//! End-to-end programme tests against a throw-away database `kalks_ib_test_<pid>_<n>` on the local
//! PostgreSQL (IB_TEST_DATABASE_URL, default :5433). Skipped when PostgreSQL is unreachable.
//! The wallet is a mock HTTP server, so payout retries and idempotency are exercised for real.

use axum::Json;
use axum::extract::State;
use chrono::{Duration, Utc};
use ib::api::Tenant;
use ib::audit::Actor;
use ib::clients::AccountInfo;
use ib::config::Config;
use ib::deals::{self, DealInput, Outcome};
use ib::money::{D, dec};
use ib::state::AppState;
use ib::{db, payouts, sync};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

static N: AtomicUsize = AtomicUsize::new(0);

struct Env {
    st: AppState,
    admin: PgConnectOptions,
    name: String,
}

async fn env(wallet_url: Option<String>) -> Option<Env> {
    let base = std::env::var("IB_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let admin = PgConnectOptions::from_str(&base).ok()?.database("postgres");
    if admin.connect().await.is_err() {
        eprintln!("skipping IB DB tests: no PostgreSQL at {base}");
        return None;
    }
    let name = format!("kalks_ib_test_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst));
    let url = admin.clone().database(&name).to_url_lossy().to_string();
    let pool = db::connect(&url).await.expect("create + migrate");
    let mut cfg = Config::for_tests(&url);
    if let Some(w) = wallet_url {
        cfg.wallet_url = w;
    }
    Some(Env { st: AppState::new(pool, cfg), admin, name })
}

impl Env {
    async fn drop(self) {
        self.st.pool.close().await;
        if let Ok(mut c) = self.admin.connect().await {
            let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)", self.name))).execute(&mut c).await;
        }
    }

    async fn member(&self, id: i64, parent: Option<i64>, level: &str) {
        sqlx::query(
            "INSERT INTO members (user_id, tenant, email, first_name, last_name, country, referral_code, parent_id, signup_parent_id, level_key, joined_at, changed_at, devices, identity)
             VALUES ($1, 'kalks', $2, $3, 'Test', 'IN', $4, $5, $5, $6, now() - interval '10 days', now(), $7, $8)",
        )
        .bind(id)
        .bind(format!("u{id}@example.com"))
        .bind(format!("User{id}"))
        .bind(format!("CODE{id}"))
        .bind(parent)
        .bind(level)
        .bind(vec![format!("dev-{id}")])
        .bind(vec![format!("idn-{id}")])
        .execute(&self.st.pool)
        .await
        .unwrap();
    }

    async fn sum(&self, sql: &str) -> D {
        sqlx::query_scalar::<_, Option<D>>(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&self.st.pool).await.unwrap().unwrap_or(D::ZERO)
    }

    async fn count(&self, sql: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql.to_string())).fetch_one(&self.st.pool).await.unwrap()
    }
}

fn deal(id: i64, user: i64, symbol: &str, lots: &str, secs: i64) -> DealInput {
    let close = Utc::now() - Duration::minutes(5);
    DealInput {
        source: "engine".into(),
        deal_id: id,
        tenant: "kalks".into(),
        login: Some(10_000_000 + user),
        user_id: user,
        symbol: symbol.into(),
        side: "buy".into(),
        volume: dec(lots),
        open_time: close - Duration::seconds(secs),
        close_time: close,
        kind: "close".into(),
        reversed: false,
        account: Some(AccountInfo { login: 10_000_000 + user, user_id: user, kind: "live".into(), group: "standard".into(), cent: false }),
        option: false,
    }
}

/// A Kalks FX Options closing deal: volume = contracts.
fn option_deal(id: i64, user: i64, contracts: &str, secs: i64) -> DealInput {
    DealInput { symbol: "EURUSD-20261009-1.1650-C".into(), option: true, ..deal(id, user, "EURUSD-20261009-1.1650-C", contracts, secs) }
}

#[tokio::test]
async fn option_deals_earn_per_contract_at_the_options_rate_never_per_lot() {
    let Some(e) = env(None).await else { return };
    // A (gold) ← B (silver) ← C (client)
    e.member(60, None, "gold").await;
    e.member(61, Some(60), "silver").await;
    e.member(62, Some(61), "bronze").await;
    sqlx::query("UPDATE members SET rebate_pct = 10 WHERE user_id = 61").execute(&e.st.pool).await.unwrap();

    // default options rate 0: the deal qualifies but pays nothing — the CFD fx-major rate is never applied
    let out = deals::ingest(&e.st, &option_deal(9601, 62, "5", 600)).await.unwrap();
    assert_eq!(out, Outcome::Recorded { qualified: true, reason: None, lines: 0 });
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE deal_id = 9601").await, 0);
    let row: (String, D, D, Option<String>) = sqlx::query_as("SELECT instrument, lots, contracts, symbol_group FROM deals WHERE deal_id = 9601").fetch_one(&e.st.pool).await.unwrap();
    assert_eq!(row, ("option".into(), D::ZERO, dec("5"), Some("options".into())), "contracts are never lots");

    // the broker sets per-contract rates in the Back Office (PUT levels); a level sent without the field keeps it
    let staff = || ib::api::StaffCtx { tenant: "kalks".into(), id: "1".into(), name: "Partner manager".into(), role: "partner_manager".into() };
    let mut levels = serde_json::to_value(db::levels(&e.st.pool, "kalks").await.unwrap()).unwrap();
    for l in levels.as_array_mut().unwrap() {
        let rate = match l["key"].as_str().unwrap() {
            "silver" => json!(2),
            "gold" => json!("3"),
            _ => json!(0),
        };
        l["optionsRate"] = rate;
    }
    let body = serde_json::from_value(json!({"levels": levels, "reason": "Options launch rate card"})).unwrap();
    let v = ib::api::admin::put_levels(State(e.st.clone()), staff(), Json(body)).await.unwrap().0;
    let silver = v["levels"].as_array().unwrap().iter().find(|l| l["key"] == "silver").unwrap().clone();
    assert_eq!(silver["optionsRate"], json!(2.0));
    let mut stripped = levels.clone();
    for l in stripped.as_array_mut().unwrap() {
        l.as_object_mut().unwrap().remove("optionsRate");
    }
    let body = serde_json::from_value(json!({"levels": stripped, "reason": "Edit by an older Back Office build"})).unwrap();
    let _ = ib::api::admin::put_levels(State(e.st.clone()), staff(), Json(body)).await.unwrap();
    let lv = db::levels(&e.st.pool, "kalks").await.unwrap();
    assert_eq!(lv.iter().find(|l| l.key == "gold").unwrap().option_rate(), dec("3"), "kept, never reset to 0");

    // 4 contracts: B (tier 1, silver $2) 4 × 2 × 100% = 8, of which 10% (0.80) rebated to C; A (tier 2, gold $3) 4 × 3 × 20% = 2.40
    let out = deals::ingest(&e.st, &option_deal(9602, 62, "4", 600)).await.unwrap();
    assert!(matches!(out, Outcome::Recorded { qualified: true, .. }), "{out:?}");
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9602 AND beneficiary_id = 61 AND kind = 'lot'").await, dec("7.2"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9602 AND beneficiary_id = 62 AND kind = 'rebate'").await, dec("0.8"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9602 AND beneficiary_id = 60").await, dec("2.4"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9602").await, dec("10.4"));
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE deal_id = 9602 AND (lots <> 0 OR contracts <> 4 OR symbol_group <> 'options')").await, 0);
    assert_eq!(e.sum("SELECT sum(rate) FROM commissions WHERE deal_id = 9602 AND beneficiary_id = 61 AND kind = 'lot'").await, dec("2"), "the per-contract rate");

    // the usual filters apply: a scalp, a demo account, a reopened (voided) deal
    let short = deals::ingest(&e.st, &option_deal(9603, 62, "4", 30)).await.unwrap();
    assert_eq!(short, Outcome::Recorded { qualified: false, reason: Some("short_duration".into()), lines: 0 });
    let mut demo = option_deal(9604, 62, "4", 600);
    demo.account.as_mut().unwrap().kind = "demo".into();
    assert!(matches!(deals::ingest(&e.st, &demo).await.unwrap(), Outcome::Recorded { qualified: false, .. }));
    deals::reverse_deal(&e.st, "kalks", "engine", 9602).await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE deal_id = 9602 AND status = 'void'").await, 3);

    // a CFD deal of the same client still earns per lot (fx-major: silver $7, gold $9 × 20%)
    deals::ingest(&e.st, &deal(9605, 62, "EURUSD", "1", 600)).await.unwrap();
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9605").await, dec("8.8"));

    // level upgrades count lots only: the option contracts never add to the network's monthly lots
    let (m0, m1) = ib::calc::month_bounds(Utc::now());
    assert_eq!(ib::stats::network_lots(&e.st.pool, "kalks", 61, 3, m0 - Duration::days(1), m1).await.unwrap(), dec("1"));
    // an option series pushed through the PAMM / copy lots route is treated as contracts too
    let mut pushed = option_deal(9606, 62, "3", 600);
    pushed.source = "copy".into();
    pushed.option = ib::model::is_option_series(&pushed.symbol);
    deals::ingest(&e.st, &pushed).await.unwrap();
    assert_eq!(e.sum("SELECT sum(lots) FROM deals WHERE deal_id = 9606").await, D::ZERO);
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9606").await, dec("7.8"));

    // the engine feed parser recognises option deals by `option`, `instrument` or the series code
    let base = json!({"id": 1, "login": 10000062, "clientId": 62, "symbol": "EURUSD", "side": "buy", "volume": 1, "openTime": "2026-10-01T10:00:00Z", "closeTime": "2026-10-01T11:00:00Z"});
    assert!(!ib::clients::parse_deal(&base).unwrap().option);
    let mut o = base.clone();
    o["option"] = json!({"series": "EURUSD-20261009-1.1650-C"});
    assert!(ib::clients::parse_deal(&o).unwrap().option);
    let mut o = base.clone();
    o["instrument"] = json!("option");
    assert!(ib::clients::parse_deal(&o).unwrap().option);
    let mut o = base.clone();
    o["symbol"] = json!("USDJPY-20261009-150.00-P");
    assert!(ib::clients::parse_deal(&o).unwrap().option);
    e.drop().await;
}

#[tokio::test]
async fn multi_tier_commissions_are_idempotent_and_filtered() {
    let Some(e) = env(None).await else { return };
    // A (gold) ← B (silver) ← C (client);  D is a client of C
    e.member(1, None, "gold").await;
    e.member(2, Some(1), "silver").await;
    e.member(3, Some(2), "bronze").await;
    e.member(4, Some(3), "bronze").await;
    sqlx::query("UPDATE members SET rebate_pct = 10 WHERE user_id = 2").execute(&e.st.pool).await.unwrap();
    sqlx::query("UPDATE members SET split_pct = 25 WHERE user_id = 1").execute(&e.st.pool).await.unwrap();

    // C trades 2 lots of XAUUSD for 10 minutes: B tier 1 silver $10 → 20; A tier 2 gold $12 × 20% → 4.80
    let out = deals::ingest(&e.st, &deal(9001, 3, "XAUUSD", "2", 600)).await.unwrap();
    assert!(matches!(out, Outcome::Recorded { qualified: true, .. }), "{out:?}");
    // rebate: 10% of B's 20 → 2 to C; split: 25% of A's 4.80 → 1.20 to B
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE beneficiary_id = 2 AND kind = 'lot'").await, dec("18"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE beneficiary_id = 3 AND kind = 'rebate'").await, dec("2"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE beneficiary_id = 2 AND kind = 'split'").await, dec("1.2"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE beneficiary_id = 1 AND kind = 'lot'").await, dec("3.6"));
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions").await, dec("24.8"));

    // the same deal again (poller overlap, restart): nothing new
    assert_eq!(deals::ingest(&e.st, &deal(9001, 3, "XAUUSD", "2", 600)).await.unwrap(), Outcome::Duplicate);
    assert_eq!(e.count("SELECT count(*) FROM commissions").await, 4);

    // D's trade reaches three tiers: C (tier 1), B (tier 2), A (tier 3)
    deals::ingest(&e.st, &deal(9002, 4, "EURUSD", "1", 300)).await.unwrap();
    assert_eq!(e.count("SELECT count(DISTINCT beneficiary_id) FROM commissions WHERE deal_id = 9002 AND kind = 'lot'").await, 3);
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9002 AND beneficiary_id = 3").await, dec("5")); // bronze fx-major $5

    // anti-abuse: a 60-second trade, a demo account and an excluded group earn nothing
    let short = deals::ingest(&e.st, &deal(9003, 3, "XAUUSD", "5", 60)).await.unwrap();
    assert_eq!(short, Outcome::Recorded { qualified: false, reason: Some("short_duration".into()), lines: 0 });
    let mut demo = deal(9004, 3, "XAUUSD", "5", 600);
    demo.account.as_mut().unwrap().kind = "demo".into();
    assert!(matches!(deals::ingest(&e.st, &demo).await.unwrap(), Outcome::Recorded { qualified: false, .. }));
    let mut prop = deal(9005, 3, "XAUUSD", "5", 600);
    prop.account.as_mut().unwrap().group = "prop".into();
    assert!(matches!(deals::ingest(&e.st, &prop).await.unwrap(), Outcome::Recorded { qualified: false, .. }));
    // a root client (no referrer) earns nobody anything
    assert!(matches!(deals::ingest(&e.st, &deal(9006, 1, "XAUUSD", "1", 600)).await.unwrap(), Outcome::Recorded { qualified: false, .. }));
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE deal_id IN (9003, 9004, 9005, 9006)").await, 0);

    // PAMM allocation lots count like the investor's own trades (D64), keyed separately from engine deals
    let mut pamm = deal(9001, 3, "XAUUSD", "1", 600);
    pamm.source = "pamm".into();
    assert!(matches!(deals::ingest(&e.st, &pamm).await.unwrap(), Outcome::Recorded { qualified: true, .. }));
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE deal_source = 'pamm'").await, 4);

    // a deal for a client the gateway feed hasn't delivered yet is re-evaluated when the client arrives
    let early = deals::ingest(&e.st, &deal(9007, 5, "XAUUSD", "1", 600)).await.unwrap();
    assert_eq!(early, Outcome::Recorded { qualified: false, reason: Some("unknown_client".into()), lines: 0 });
    e.member(5, Some(2), "bronze").await;
    deals::reprocess_unknown(&e.st, 5).await.unwrap();
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE deal_id = 9007 AND beneficiary_id = 2 AND kind = 'lot'").await, dec("9"));
    e.drop().await;
}

#[tokio::test]
async fn self_referral_blocks_until_cleared_and_wash_pairs_are_flagged() {
    let Some(e) = env(None).await else { return };
    e.member(10, None, "bronze").await;
    e.member(11, Some(10), "bronze").await;
    e.member(12, Some(10), "bronze").await;
    // client 11 signs in from the IB's device
    sqlx::query("UPDATE members SET devices = ARRAY['dev-10'] WHERE user_id = 11").execute(&e.st.pool).await.unwrap();
    let s = db::settings(&e.st.pool, "kalks").await.unwrap();
    sync::check_self_referral(&e.st, "kalks", 11, &s).await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM fraud_flags WHERE kind = 'self_referral_device' AND client_id = 11").await, 1);
    let blocked = deals::ingest(&e.st, &deal(9101, 11, "EURUSD", "1", 600)).await.unwrap();
    assert_eq!(blocked, Outcome::Recorded { qualified: false, reason: Some("self_referral".into()), lines: 0 });
    // shared loopback IPs are not a signal
    sqlx::query("UPDATE members SET ips = ARRAY['127.0.0.1'] WHERE user_id IN (10, 12)").execute(&e.st.pool).await.unwrap();
    sync::check_self_referral(&e.st, "kalks", 12, &s).await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM fraud_flags WHERE client_id = 12").await, 0);
    // an admin dismisses the flag: the block lifts and stays lifted on the next sync
    sqlx::query("UPDATE members SET abuse_cleared = true WHERE user_id = 11").execute(&e.st.pool).await.unwrap();
    sync::check_self_referral(&e.st, "kalks", 11, &s).await.unwrap();
    assert!(matches!(deals::ingest(&e.st, &deal(9102, 11, "EURUSD", "1", 600)).await.unwrap(), Outcome::Recorded { qualified: true, .. }));

    // wash trading: 12 sells what 11 buys, same size, same minute
    let mut a = deal(9103, 11, "GBPUSD", "3", 900);
    let mut b = deal(9104, 12, "GBPUSD", "3", 890);
    a.side = "buy".into();
    b.side = "sell".into();
    deals::ingest(&e.st, &a).await.unwrap();
    deals::ingest(&e.st, &b).await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM fraud_flags WHERE kind = 'wash_trading'").await, 1);
    e.drop().await;
}

#[tokio::test]
async fn cpa_needs_deposit_and_trade_once() {
    let Some(e) = env(None).await else { return };
    e.member(20, None, "gold").await;
    e.member(21, Some(20), "bronze").await;
    // deposit below the minimum: no CPA
    deals::record_deposit(&e.st, "kalks", 21, Utc::now() - Duration::days(2), dec("100")).await.unwrap();
    deals::ingest(&e.st, &deal(9201, 21, "EURUSD", "1", 600)).await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE kind = 'cpa'").await, 0);
    // an earlier, larger first deposit replaces it; trade already done → CPA $300 (gold), held 30 days
    deals::record_deposit(&e.st, "kalks", 21, Utc::now() - Duration::days(3), dec("600")).await.unwrap();
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE kind = 'cpa' AND beneficiary_id = 20").await, dec("300"));
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE kind = 'cpa' AND available_at > now() + interval '29 days'").await, 1);
    // never twice
    assert!(!deals::maybe_cpa(&e.st, "kalks", 21).await.unwrap());
    e.drop().await;
}

/// Mock wallet: fails the first `fail_first` calls with 503, then credits; replays by idempotency key.
async fn mock_wallet(fail_first: usize) -> (String, Arc<Mutex<Vec<Value>>>) {
    let calls: Arc<Mutex<Vec<Value>>> = Arc::default();
    let c2 = calls.clone();
    let app = axum::Router::new().route(
        "/v1/wallets/transfers",
        axum::routing::post(move |Json(body): Json<Value>| {
            let calls = c2.clone();
            async move {
                let mut v = calls.lock().unwrap();
                v.push(body.clone());
                if v.len() <= fail_first {
                    return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": {"code": "unavailable"}})));
                }
                let replayed = v.iter().filter(|b| b["idempotency_key"] == body["idempotency_key"]).count() > 1;
                (axum::http::StatusCode::OK, Json(json!({"status": "completed", "txn_id": 700 + v.len(), "replayed": replayed})))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, calls)
}

#[tokio::test]
async fn batch_approval_pays_wallet_with_retry_and_reversal_claws_back() {
    let (url, calls) = mock_wallet(1).await;
    let Some(e) = env(Some(url)).await else { return };
    e.member(30, None, "silver").await;
    e.member(31, Some(30), "bronze").await;
    deals::ingest(&e.st, &deal(9301, 31, "XAUUSD", "3", 600)).await.unwrap(); // 30 USD to IB 30
    deals::ingest(&e.st, &deal(9302, 31, "EURUSD", "0.5", 600)).await.unwrap(); // 3.50
    // below-minimum payees are carried over: add a tiny one for another IB
    e.member(32, None, "bronze").await;
    e.member(33, Some(32), "bronze").await;
    deals::ingest(&e.st, &deal(9303, 33, "EURUSD", "0.1", 600)).await.unwrap(); // 0.50 < $10 minimum

    let batch = payouts::create_batch(&e.st, "kalks", &Actor::system(), None, Utc::now() + Duration::seconds(1)).await.unwrap().expect("batch");
    assert_eq!(e.count(&format!("SELECT count(*) FROM payouts WHERE batch_id = {batch}")).await, 1);
    assert_eq!(e.sum(&format!("SELECT total FROM payout_batches WHERE id = {batch}")).await, dec("33.5"));
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE beneficiary_id = 32 AND batch_id IS NULL").await, 1);
    // nothing left to batch
    assert!(payouts::create_batch(&e.st, "kalks", &Actor::system(), None, Utc::now() + Duration::seconds(1)).await.unwrap().is_none());

    payouts::approve(&e.st, "kalks", batch, &Actor { id: "staff:1".into(), name: Some("Finance".into()) }, None).await.unwrap();
    // first attempt: wallet down → stays pending transfer with backoff
    assert_eq!(payouts::transfer_tick(&e.st).await.unwrap(), (0, 1, 0));
    assert_eq!(e.count("SELECT count(*) FROM payouts WHERE status = 'transfer_pending' AND attempts = 1 AND next_attempt_at > now()").await, 1);
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE status = 'approved'").await, 2);
    // retry is due
    sqlx::query("UPDATE payouts SET next_attempt_at = now()").execute(&e.st.pool).await.unwrap();
    assert_eq!(payouts::transfer_tick(&e.st).await.unwrap(), (1, 0, 0));
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE status = 'paid' AND beneficiary_id = 30").await, 2);
    assert_eq!(e.count(&format!("SELECT count(*) FROM payout_batches WHERE id = {batch} AND status = 'paid'")).await, 1);
    {
        let c = calls.lock().unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0]["idempotency_key"], c[1]["idempotency_key"], "the retry reuses the idempotency key");
        assert_eq!(c[1]["amount"], "33.5");
        assert_eq!(c[1]["kind"], "ib_payout");
        assert_eq!(c[1]["direction"], "credit");
        assert_eq!(c[1]["user_id"], 30);
    }
    // the partner hears about it once through the support service (bell + email)
    let notes: Arc<Mutex<Vec<(String, Value)>>> = Arc::default();
    let n2 = notes.clone();
    let support = axum::Router::new().route(
        "/v1/notify",
        axum::routing::post(move |h: axum::http::HeaderMap, Json(body): Json<Value>| {
            let notes = n2.clone();
            async move {
                notes.lock().unwrap().push((h.get("x-kalks-service").and_then(|v| v.to_str().ok()).unwrap_or("").to_string(), body));
                Json(json!({"results": []}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let support_url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, support).await.unwrap() });
    let mut cfg = (*e.st.cfg).clone();
    cfg.support_url = support_url;
    let st = AppState { cfg: Arc::new(cfg), ..e.st.clone() };
    assert_eq!(ib::notifier::push_paid(&st).await.unwrap(), (1, 0));
    assert_eq!(ib::notifier::push_paid(&st).await.unwrap(), (0, 0), "announced once");
    {
        let n = notes.lock().unwrap();
        assert_eq!(n.len(), 1);
        let (svc, b) = &n[0];
        assert_eq!(svc, "ib");
        assert_eq!(b["type"], "ib.commission_paid");
        assert_eq!(b["userId"], 30);
        assert_eq!(b["link"], "/partner/payouts");
        assert!(b["body"].as_str().unwrap().starts_with("33.50 USDT"), "{b}");
        assert!(b["dedupeKey"].as_str().unwrap().starts_with("ib:payout:"));
    }
    // a paid deal reopened by the dealing desk is clawed back with a negative pending line
    deals::reverse_deal(&e.st, "kalks", "engine", 9301).await.unwrap();
    assert_eq!(e.sum("SELECT sum(amount) FROM commissions WHERE kind = 'clawback' AND beneficiary_id = 30").await, dec("-30"));
    // an unpaid pending line of a reopened deal is voided instead
    deals::reverse_deal(&e.st, "kalks", "engine", 9303).await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE deal_id = 9303 AND status = 'void'").await, 1);
    e.drop().await;
}

#[tokio::test]
async fn batch_reject_releases_lines_and_clicks_are_tracked() {
    let Some(e) = env(None).await else { return };
    e.member(40, None, "bronze").await;
    e.member(41, Some(40), "bronze").await;
    deals::ingest(&e.st, &deal(9401, 41, "XAUUSD", "2", 600)).await.unwrap(); // 16
    let b = payouts::create_batch(&e.st, "kalks", &Actor::system(), None, Utc::now() + Duration::seconds(1)).await.unwrap().unwrap();
    payouts::reject(&e.st, "kalks", b, &Actor::system(), "wrong period").await.unwrap();
    assert_eq!(e.count("SELECT count(*) FROM commissions WHERE status = 'pending' AND batch_id IS NULL").await, 1);
    assert!(payouts::approve(&e.st, "kalks", b, &Actor::system(), None).await.is_err(), "a rejected batch can't be approved");

    sqlx::query("INSERT INTO campaigns (tenant, user_id, slug, name) VALUES ('kalks', 40, 'yt', 'YouTube')").execute(&e.st.pool).await.unwrap();
    let click = |code: &str, camp: Option<&str>, ip: &str| {
        let st = e.st.clone();
        let body: ib::api::public::ClickReq = serde_json::from_value(json!({"code": code, "campaign": camp, "ip": ip, "userAgent": "UA"})).unwrap();
        async move { ib::api::public::click(State(st), Tenant("kalks".into()), Json(body)).await.unwrap().0 }
    };
    let v = click("code40", Some("YT"), "198.51.100.7").await;
    assert_eq!(v["valid"], true);
    assert_eq!(v["campaign"], "yt");
    assert_eq!(v["unique"], true);
    assert_eq!(click("CODE40", Some("yt"), "198.51.100.7").await["unique"], false, "same visitor within 24 h");
    assert_eq!(click("CODE40", Some("unknown"), "198.51.100.8").await["campaign"], Value::Null);
    assert_eq!(click("NOPE999", None, "198.51.100.9").await["valid"], false);
    assert_eq!(e.count("SELECT count(*) FROM clicks WHERE user_id = 40").await, 3);
    e.drop().await;
}

/// Module switches (gateway): while a broker has `ib` off, the partner's own routes answer 403 module_disabled before
/// any handler runs; tracking events stay; a broker with the module on (or a key the gateway doesn't send) is
/// unaffected. No database needed (the switch answers first).
#[tokio::test]
async fn module_switch_refuses_the_partner_routes() {
    use axum::extract::Path;
    let gw = axum::Router::new().route(
        "/v1/internal/tenants/{slug}",
        axum::routing::get(|Path(slug): Path<String>| async move { Json(json!({"id": 2, "slug": slug, "modules": if slug == "qa-ib-off" { json!({"ib": false}) } else { json!({"prop": false}) }})) }),
    );
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gw_url = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, gw).await.unwrap() });
    let pool = sqlx::postgres::PgPoolOptions::new().acquire_timeout(std::time::Duration::from_millis(300)).connect_lazy("postgres://postgres@127.0.0.1:1/unused").unwrap();
    let mut cfg = Config::for_tests("postgres://postgres@127.0.0.1:1/unused");
    cfg.gateway_url = gw_url;
    let st = AppState::new(pool, cfg);
    assert!(!ib::modules::on(&st, "qa-ib-off", "ib").await);
    assert!(ib::modules::on(&st, "qa-ib-on", "ib").await, "a key the gateway doesn't send is on");
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", l.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(l, ib::api::router(st)).await.unwrap() });
    let http = reqwest::Client::new();
    for path in ["/v1/ib/me", "/v1/ib/me/clients", "/v1/ib/me/payouts"] {
        let r = http.get(format!("{base}{path}")).header("x-kalks-tenant", "qa-ib-off").header("x-kalks-user-id", "7").send().await.unwrap();
        assert_eq!(r.status().as_u16(), 403, "{path}");
        assert_eq!(r.json::<Value>().await.unwrap()["error"]["code"], "module_disabled");
    }
    let r = http.get(format!("{base}/v1/ib/me")).header("x-kalks-tenant", "qa-ib-on").header("x-kalks-user-id", "7").send().await.unwrap();
    assert_ne!(r.status().as_u16(), 403);
    let r = http.post(format!("{base}/v1/ib/clicks")).header("x-kalks-tenant", "qa-ib-off").json(&json!({"code": "NOPE1234"})).send().await.unwrap();
    assert_ne!(r.status().as_u16(), 403, "tracking stays");
}

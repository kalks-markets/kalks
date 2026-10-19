//! Integration test against PostgreSQL (:5433, a throw-away database; fails, never skips, without it): stock
//! corporate actions end to end through the Back Office API handlers, the scheduler and the shards.
//!
//! A split and a dividend on MSFT are proposed, approved (four-eyes: the proposer can't approve the split), and
//! applied at 00:00 New York of the ex-date to a live, a cent and a demo account. The scheduler "crashes" after the
//! first account (applied directly, no run row, status left `applying`); the next pass finishes the rest and skips
//! it; a third pass changes nothing. While the action is due and not applied, the symbol does not trade. Replay of
//! every account equals the live state and the ledger reconciles. The EODHD import is idle without a key; recorded
//! EODHD data becomes proposed actions, and a changed amount sends an approved action back for approval.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};

use trading::api::{self, ApiError, AppState, Body, Ctx, LoginAlloc, StaffCtx};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::trade::{self, OrderReq};
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, OrderType, Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{ExecError, Hub, Index, NullLp, Op, Shared, Staff, Stats, Streams};
use trading::specs::{Overrides, Specs};

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn account(login: i64, group: &str, kind: AccountKind, cent: bool) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: 77,
        kind,
        group: group.into(),
        mode: Mode::Hedging,
        cent,
        leverage: 100,
        status: Status::Active,
        name: "IT".into(),
        route_override: None,
        controls: Controls::default(),
        demo: (kind == AccountKind::Demo).then(|| DemoCfg { initial_balance: d("10000"), refills_per_day: 3, expiry_days: 30 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

async fn exec(hub: &Hub, login: i64, op: Op) -> Result<Value, ExecError> {
    hub.exec(login, "test", None, "", "", None, op).await.map(|d| d.value)
}

fn staff(st: &AppState, id: &str, role: &str) -> StaffCtx {
    let tenant = st.hub.shared.registry.by_slug("kalks").unwrap();
    StaffCtx { ctx: Ctx { tenant, ip: "127.0.0.1".into(), user_agent: "it".into(), bearer: None }, staff: Staff { id: id.into(), name: format!("IT {id}"), role: role.into() }, perms: None }
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

fn code(e: ApiError) -> String {
    match e {
        ApiError::Forbidden(_) => "forbidden".into(),
        ApiError::Validation { field, .. } => format!("validation:{field}"),
        ApiError::Conflict { code, .. } => format!("conflict:{code}"),
        ApiError::Status { status, code, .. } => format!("{status}:{code}"),
        other => format!("{other:?}").chars().take(80).collect(),
    }
}

async fn state(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await
}

#[tokio::test]
async fn splits_and_dividends_end_to_end_with_a_crash_and_replay() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_corporate_{}", std::process::id());
    let server = PgConnectOptions::from_str(&base).unwrap();
    server.clone().database("postgres").connect().await.expect("PostgreSQL :5433 must be running for this test");
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate");

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    // stocks live for this test (in production they stay off until the founder switches them on)
    let specs = Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap();
    let specs = Arc::new(specs.with_overrides(Overrides { live_classes: BTreeMap::from([("stocks".to_string(), true)]), ..Default::default() }).unwrap());
    let registry = Registry::default();
    for tn in trading::persist::load_registry(&pool).await.unwrap() {
        registry.put(tn);
    }
    let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&pool).await.unwrap();
    let quotes = Arc::new(QuoteBook::default());
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
        max_quote_age_ms: 0,
        restrictions: Default::default(),
        options: Arc::new(trading::options::OptionsCtx::disabled(quotes.clone())),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
        risk: Default::default(),
    });
    let hub = Hub::start(shared, 2, Default::default());
    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config::for_tests(&url);
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
        gateway: Arc::new(trading::controls::Gateway::new("", "")),
    };
    let set_quote = |bid: &str, ask: &str| {
        for g in ["standard", "pro", "ecn", "cent", "raw"] {
            quotes.set(g, "MSFT", Quote { bid: d(bid), ask: d(ask), t_ms: Utc::now().timestamp_millis() });
        }
    };

    // Wednesday 2026-10-14 15:00 UTC: US session open
    hub.shared.clock.set(Some(t("2026-10-14T15:00:00Z")));
    set_quote("400.00", "400.10");
    let (lv, ct, dm) = (10_000_501i64, 10_000_502i64, 50_000_501i64);
    hub.open(account(lv, "standard", AccountKind::Live, false), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(ct, "cent", AccountKind::Live, true), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(dm, "standard", AccountKind::Demo, false), ("h".into(), "i".into()), "test").await.unwrap();
    for (l, k) in [(lv, "it-c-1"), (ct, "it-c-2")] {
        exec(&hub, l, Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, d("50000"), k, None).map(|_| Value::Null))).await.unwrap();
    }
    let open = |_login: i64, side: Side, v: &'static str| -> Op { Box::new(move |tx, env| trade::place_order(tx, env, OrderReq::market("MSFT", side, d(v))).map(|r| json!(format!("{r:?}")))) };
    exec(&hub, lv, open(lv, Side::Buy, "10")).await.unwrap();
    exec(&hub, lv, open(lv, Side::Sell, "3")).await.unwrap();
    exec(&hub, lv, Box::new(|tx, env| trade::place_order(tx, env, OrderReq { kind: OrderType::Limit, price: Some(d("390.00")), ..OrderReq::market("MSFT", Side::Buy, d("5")) }).map(|_| Value::Null))).await.unwrap();
    exec(&hub, ct, open(ct, Side::Buy, "2")).await.unwrap();
    exec(&hub, dm, open(dm, Side::Buy, "5")).await.unwrap();

    // proposals: a 4-for-1 split and a 0.83 USD dividend, ex-date Friday 2026-10-16
    let r = api::corporate::create(State(st.clone()), staff(&st, "11", "dealer"), body(json!({"symbol": "MSFT", "kind": "split", "exDate": "2026-10-16", "ratioFrom": 1, "ratioTo": 4, "reason": "MSFT 4-for-1 announced"}))).await.unwrap().0;
    let split_id = r["id"].as_i64().unwrap();
    assert_eq!((r["status"].as_str(), r["fourEyes"].as_bool(), r["applyAt"].as_str()), (Some("proposed"), Some(true), Some("2026-10-16T04:00:00Z")));
    let r = api::corporate::create(State(st.clone()), staff(&st, "11", "dealer"), body(json!({"symbol": "MSFT", "kind": "dividend", "exDate": "2026-10-16", "amount": 0.83, "reason": "Q3 dividend"}))).await.unwrap().0;
    let div_id = r["id"].as_i64().unwrap();
    assert_eq!(r["withholdingPct"].as_f64(), Some(30.0), "US default withholding");
    assert_eq!(r["fourEyes"].as_bool(), Some(false), "0.2 % of the price");
    // validation
    let e = api::corporate::create(State(st.clone()), staff(&st, "11", "dealer"), body(json!({"symbol": "MSFT", "kind": "split", "exDate": "2026-10-16", "ratioFrom": 1, "ratioTo": 4, "reason": "dup"}))).await.unwrap_err();
    assert_eq!(code(e), "conflict:duplicate");
    let e = api::corporate::create(State(st.clone()), staff(&st, "11", "dealer"), body(json!({"symbol": "EURUSD", "kind": "dividend", "exDate": "2026-10-16", "amount": 1, "reason": "x y z"}))).await.unwrap_err();
    assert_eq!(code(e), "validation:symbol");
    let e = api::corporate::create(State(st.clone()), staff(&st, "11", "dealer"), body(json!({"symbol": "AAPL", "kind": "dividend", "exDate": "2026-10-13", "amount": 1, "reason": "late"}))).await.unwrap_err();
    assert_eq!(code(e), "validation:exDate");
    // approval: dealers can't, the proposer can't approve a four-eyes action, another admin can
    assert_eq!(code(api::corporate::approve(State(st.clone()), staff(&st, "11", "dealer"), Path(split_id), body(json!({"reason": "ok go"}))).await.unwrap_err()), "forbidden");
    assert_eq!(code(api::corporate::approve(State(st.clone()), staff(&st, "11", "admin"), Path(split_id), body(json!({"reason": "ok go"}))).await.unwrap_err()), "conflict:four_eyes");
    let a = api::corporate::approve(State(st.clone()), staff(&st, "12", "admin"), Path(split_id), body(json!({"reason": "checked against the press release"}))).await.unwrap().0;
    assert_eq!(a["status"], "approved");
    let _ = api::corporate::approve(State(st.clone()), staff(&st, "11", "admin"), Path(div_id), body(json!({"reason": "small dividend"}))).await.unwrap();

    // the Back Office detail previews what will happen
    let det = api::corporate::detail(State(st.clone()), staff(&st, "13", "dealer"), Path(split_id)).await.unwrap().0;
    let lv_prev = det["accounts"].as_array().unwrap().iter().find(|a| a["login"] == lv).unwrap();
    assert_eq!(lv_prev["positions"].as_array().unwrap().len(), 2);
    assert_eq!(lv_prev["orders"][0]["newVolume"].as_f64(), Some(20.0));

    // just before the ex-date: nothing applies
    hub.shared.clock.set(Some(t("2026-10-16T03:59:00Z")));
    assert_eq!(trading::corporate::pass(&hub, &pool).await.unwrap(), 0);
    // 05:00 UTC: the scheduler "crashes" after the split of the live account (no run row, status applying)
    hub.shared.clock.set(Some(t("2026-10-16T05:00:00Z")));
    trading::corporate::refresh_due(&hub, &pool).await.unwrap();
    let split_row = trading::corporate::load(&pool, split_id).await.unwrap().unwrap();
    let act = split_row.action().unwrap();
    exec(&hub, lv, Box::new(move |tx, env| trading::engine::corporate::apply(tx, env, &act))).await.unwrap();
    sqlx::query("UPDATE corporate_actions SET status = 'applying' WHERE id = $1").bind(split_id).execute(&pool).await.unwrap();
    // meanwhile the symbol does not trade on accounts still waiting
    match exec(&hub, dm, open(dm, Side::Buy, "1")).await {
        Err(ExecError::Reject(r)) => assert_eq!(r.code, "corporate_action"),
        other => panic!("{other:?}"),
    }
    // the next pass finishes both actions; the live account is not split twice
    assert_eq!(trading::corporate::pass(&hub, &pool).await.unwrap(), 2);
    for id in [split_id, div_id] {
        let row = trading::corporate::load(&pool, id).await.unwrap().unwrap();
        assert_eq!(row.status, "applied", "action {id}: {:?}", row.report);
    }
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM corporate_action_runs WHERE action_id = $1").bind(split_id).fetch_one(&pool).await.unwrap();
    assert_eq!(runs, 3, "the crashed account was recorded on the resume");
    let s = state(&hub, lv).await;
    let vols: Vec<f64> = s["positions"].as_object().unwrap().values().map(|p| p["volume"].as_str().unwrap().parse().unwrap()).collect();
    assert_eq!(vols.iter().sum::<f64>(), 52.0, "40 long + 12 short, once: {vols:?}");
    assert_eq!(s["orders"].as_object().unwrap().values().next().unwrap()["volume"].as_str(), Some("20"));
    // the dividend applied after the split (same ex-date, approved after it): per post-split share
    let div: Vec<(String, D)> = sqlx::query_as("SELECT t.idempotency_key, p.amount FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id WHERE t.kind = 'dividend' AND p.account_code LIKE 'acct:%' ORDER BY 1").fetch_all(&pool).await.unwrap();
    assert_eq!(div.len(), 4, "{div:?}");
    let total_lv: D = div.iter().filter(|(k, _)| s["positions"].as_object().unwrap().contains_key(k.split(':').nth(2).unwrap())).map(|(_, a)| *a).sum();
    assert_eq!(total_lv, d("23.24") - d("9.96"), "long 40 × 0.83 × 0.7 = 23.24, short −12 × 0.83 = −9.96");
    // a third pass changes nothing
    let before: Vec<Value> = futures_versions(&hub, &[lv, ct, dm]).await;
    assert_eq!(trading::corporate::pass(&hub, &pool).await.unwrap(), 0);
    assert_eq!(futures_versions(&hub, &[lv, ct, dm]).await, before);

    // replay of every account equals the live state; the ledger reconciles
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for l in [lv, ct, dm] {
        assert_eq!(serde_json::to_value(&replayed[&l]).unwrap(), state(&hub, l).await, "replay of {l}");
    }
    assert!(trading::persist::verify_balances(&pool, &replayed).await.unwrap().is_empty());
    // the client's own history, and the audit trail
    let mut h = HeaderMap::new();
    h.insert("x-kalks-user-id", "77".parse().unwrap());
    let ctx = Ctx { tenant: registry.by_slug("kalks").unwrap(), ip: "1.1.1.1".into(), user_agent: "it".into(), bearer: None };
    let mine = api::corporate::account(State(st.clone()), ctx, h, Path(lv), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap().0;
    let labels: Vec<&str> = mine["items"].as_array().unwrap().iter().filter_map(|i| i["label"].as_str()).collect();
    assert!(labels.contains(&"4-for-1 split") && labels.contains(&"Dividend adjustment"), "{labels:?}");
    let events: Vec<String> = sqlx::query_scalar("SELECT event FROM corporate_action_audit WHERE action_id = $1 ORDER BY id").bind(split_id).fetch_all(&pool).await.unwrap();
    assert_eq!(events, vec!["proposed", "approved", "applied"]);
    assert!(sqlx::query("DELETE FROM corporate_action_audit").execute(&pool).await.is_err(), "append-only");
    // the provider cross-check reports no data without market-data (it never changes anything)
    let c = api::corporate::check(State(st.clone()), staff(&st, "13", "dealer"), Path(split_id)).await.unwrap().0;
    assert_eq!(c["status"], "no_data");

    // EODHD: idle without a key; recorded data becomes proposed actions
    assert_eq!(code(api::corporate::import(State(st.clone()), staff(&st, "11", "dealer")).await.unwrap_err()), "409:not_configured");
    let div_aapl = include_str!("fixtures/eodhd/div_AAPL.US.json");
    let today = chrono::NaiveDate::from_ymd_opt(2026, 10, 7).unwrap();
    let props = trading::corporate::eodhd::parse_dividends("AAPL", "AAPL.US", div_aapl, today).unwrap();
    assert_eq!(trading::corporate::eodhd::upsert(&hub, &pool, &props[0], "EODHD import").await.unwrap(), "new");
    assert_eq!(trading::corporate::eodhd::upsert(&hub, &pool, &props[0], "EODHD import").await.unwrap(), "unchanged");
    let aapl_id: i64 = sqlx::query_scalar("SELECT id FROM corporate_actions WHERE symbol = 'AAPL'").fetch_one(&pool).await.unwrap();
    let row = trading::corporate::load(&pool, aapl_id).await.unwrap().unwrap();
    assert_eq!((row.status.as_str(), row.source.as_str(), row.created_by_id.as_str()), ("proposed", "eodhd", ""));
    let _ = api::corporate::approve(State(st.clone()), staff(&st, "12", "admin"), Path(aapl_id), body(json!({"reason": "matches the press release"}))).await.unwrap();
    let mut changed = props[0].clone();
    changed.amount = Some(d("0.28"));
    assert_eq!(trading::corporate::eodhd::upsert(&hub, &pool, &changed, "EODHD import").await.unwrap(), "updated");
    let row = trading::corporate::load(&pool, aapl_id).await.unwrap().unwrap();
    assert_eq!((row.status.as_str(), row.amount), ("proposed", Some(d("0.28"))), "an upstream change needs approval again");
    let applied_split = trading::corporate::eodhd::Proposal { symbol: "MSFT".into(), ticker: "MSFT.US".into(), kind: "split", ex_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 16).unwrap(), ratio_from: Some(D::ONE), ratio_to: Some(D::from(5)), amount: None, currency: None, record_date: None, pay_date: None };
    assert_eq!(trading::corporate::eodhd::upsert(&hub, &pool, &applied_split, "EODHD import").await.unwrap(), "locked", "an applied action never changes");

    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
    eprintln!("corporate-actions DB test ran against {base}");
}

/// Every account's stream version (unchanged = nothing was applied again).
async fn futures_versions(hub: &Hub, logins: &[i64]) -> Vec<Value> {
    let mut out = Vec::new();
    for l in logins {
        out.push(state(hub, *l).await["version"].clone());
    }
    out
}

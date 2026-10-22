//! Copy trading + PAMM end to end against PostgreSQL, through the real shards, the event tap, the copier,
//! the PAMM rollover and a mock wallet service (axum on a random port). Checks:
//! - a master's open / partial close / close are mirrored into the follower's copy account with the right size;
//! - stopping with "keep positions" leaves the copied position open and unmanaged (no guard, no more mirroring);
//! - the copy allocation arrives through the wallet (`to-trading`) and moves the HWM; a performance fee is
//!   charged above the HWM at settlement and recorded pending; rejecting it refunds the whole fee to the
//!   follower's wallet (`refund` credit) and pays the master nothing;
//! - a PAMM fund: seed → invest request → rollover (units at NAV) → profit → rollover with fee + redemption;
//!   units = Σ unit ledger, NAV unchanged by fee and redemption; IB lots split by the units held at the deal's
//!   open time (a later investor gets none, a later redemption keeps its share, shares add up to the volume);
//! - replaying the `events` table rebuilds exactly the live state of every account (master, follower, fund),
//!   and every ledger transaction balances.
//!
//! Needs the local Postgres (127.0.0.1:5433, see README); skipped when it is not reachable.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::post;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::{ConnectOptions, Row};
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use trading::engine::trade::{self, CloseReq, OrderReq};
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::social::Social;
use trading::social::math::{Sizing, SizingMode};
use trading::specs::Specs;

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

#[derive(Clone)]
struct MockWallet {
    hub: Hub,
    calls: Arc<Mutex<Vec<Value>>>,
}

async fn wallet_transfer(State(w): State<MockWallet>, axum::Json(b): axum::Json<Value>) -> axum::Json<Value> {
    w.calls.lock().unwrap().push(b.clone());
    axum::Json(json!({"status": "completed", "idempotency_key": b["idempotency_key"]}))
}

async fn wallet_trading(State(w): State<MockWallet>, Path((user, dir)): Path<(i64, String)>, axum::Json(b): axum::Json<Value>) -> axum::Json<Value> {
    w.calls.lock().unwrap().push(json!({"user": user, "dir": dir, "body": b}));
    let login = b["login"].as_i64().unwrap();
    let amount: D = b["amount"].as_str().unwrap().parse().unwrap();
    let key = format!("transfer:{}", b["idempotency_key"].as_str().unwrap());
    let direction = if dir == "to-trading" { funds::Direction::In } else { funds::Direction::Out };
    let op: Op = Box::new(move |tx, env| funds::transfer(tx, env, direction, amount, &key, None).map(|_| Value::Null));
    w.hub.exec(login, "wallet", None, "", "", None, op).await.unwrap();
    axum::Json(json!({"status": "completed"}))
}

fn account(login: i64, user: i64, group: &str) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: user,
        kind: AccountKind::Live,
        group: group.into(),
        mode: trading::model::Mode::Hedging,
        cent: false,
        leverage: 100,
        status: Status::Active,
        name: "IT".into(),
        route_override: None,
        controls: Controls::default(),
        demo: None,
        created_at: Utc::now() - chrono::Duration::days(60),
        lifecycle: None,
    }
}

async fn exec(hub: &Hub, login: i64, op: Op) -> Value {
    hub.exec(login, "test", None, "", "", None, op).await.unwrap().value
}

async fn state(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await
}

/// Positions of `login` as (volume, side, source).
async fn positions(hub: &Hub, login: i64) -> Vec<(D, String, String)> {
    let s = state(hub, login).await;
    s["positions"].as_object().map(|m| m.values().map(|p| (p["volume"].as_str().unwrap().parse().unwrap(), p["side"].as_str().unwrap().to_string(), p["source"].as_str().unwrap().to_string())).collect()).unwrap_or_default()
}

async fn wait_for<F: Fn(&[(D, String, String)]) -> bool>(hub: &Hub, login: i64, f: F) -> Vec<(D, String, String)> {
    for _ in 0..100 {
        let p = positions(hub, login).await;
        if f(&p) {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not reached on {login}: {:?}", positions(hub, login).await);
}

fn set_btc(q: &QuoteBook, bid: &str, ask: &str) {
    q.set("standard", "BTCUSD", Quote { bid: d(bid), ask: d(ask), t_ms: Utc::now().timestamp_millis() });
}

#[tokio::test]
async fn copy_and_pamm_end_to_end_with_replay() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_social_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate");
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    let specs = Arc::new(Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap());
    let registry = Registry::default();
    for t in trading::persist::load_registry(&pool).await.unwrap() {
        registry.put(t);
    }
    let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&pool).await.unwrap();
    let quotes = Arc::new(QuoteBook::default());
    let shared = Arc::new(Shared {
        pool: pool.clone(),
        registry,
        specs: specs.into(),
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
    let hub = Hub::start(shared, 4, Default::default());
    set_btc(&quotes, "80000", "80020");

    // mock wallet
    let calls: Arc<Mutex<Vec<Value>>> = Default::default();
    let app = Router::new()
        .route("/v1/wallets/transfers", post(wallet_transfer))
        .route("/v1/wallets/{user}/{dir}", post(wallet_trading))
        .with_state(MockWallet { hub: hub.clone(), calls: calls.clone() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let logins = Arc::new(trading::api::LoginAlloc { live: AtomicI64::new(live.max(10_000_500)), demo: AtomicI64::new(demo) });
    let wallet = trading::social::wallet::WalletClient::new(&format!("http://{addr}"), "t");
    let social = Social::new(pool.clone(), hub.clone(), wallet, logins).await.unwrap();
    social.start();

    // master: live account with 10 000, approved
    let master_login = 10_000_201;
    hub.open(account(master_login, 501, "standard"), ("h".into(), "i".into()), "test").await.unwrap();
    exec(&hub, master_login, Box::new(|tx, env| funds::transfer(tx, env, funds::Direction::In, d("10000"), "m-fund", None).map(|_| Value::Null))).await;
    let mid: i64 = sqlx::query_scalar("INSERT INTO social_masters (tenant_id, user_id, login, nickname, program, perf_fee_pct, fee_period, status, approved_at) VALUES (1, 501, $1, 'Tester', 'both', 20, 'daily', 'approved', now()) RETURNING id")
        .bind(master_login)
        .fetch_one(&pool)
        .await
        .unwrap();
    let reg = trading::social::load(&pool).await.unwrap();
    social.reg.write().unwrap().masters = reg.masters;

    // follower subscribes with 2 500 (equity-proportional) → copy account funded through the wallet
    let sub = social.create_sub(1, 777, mid, Sizing { mode: SizingMode::Equity, value: D::ONE }, d("2500"), None, None, None, vec![]).await.unwrap();
    social.wallet.to_trading("kalks", &format!("copy:alloc:{}", sub.id), 777, sub.login, d("2500")).await.unwrap();
    for _ in 0..50 {
        if social.reg.read().unwrap().subs[&sub.id].net_deposits == d("2500") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].net_deposits, d("2500"), "copy deposit reaches the HWM tracking");

    // master opens 0.40 BTC → follower 0.10 (2 500 / ~10 000); partial close 0.20 → 0.05; close
    let open = exec(&hub, master_login, Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, d("0.4"))).map(|r| match r {
        trade::PlaceResult::Filled { position_ticket, .. } => json!(position_ticket),
        _ => Value::Null,
    })))
    .await;
    let mt = open.as_i64().unwrap();
    let p = wait_for(&hub, sub.login, |p| p.len() == 1).await;
    assert_eq!((p[0].0, p[0].1.as_str(), p[0].2.as_str()), (d("0.1"), "buy", "copy"), "0.4 × 2 500 / (10 000 − 8 spread) = 0.10008 → 0.10");
    set_btc(&quotes, "81000", "81020");
    exec(&hub, master_login, Box::new(move |tx, env| trade::close_position(tx, env, mt, CloseReq { volume: Some(d("0.2")), ..Default::default() }).map(|_| Value::Null))).await;
    wait_for(&hub, sub.login, |p| p.len() == 1 && p[0].0 == d("0.05")).await;
    exec(&hub, master_login, Box::new(move |tx, env| trade::close_position(tx, env, mt, CloseReq::default()).map(|_| Value::Null))).await;
    wait_for(&hub, sub.login, |p| p.is_empty()).await;
    // the copy log rows are written just after each mirrored step commits: wait for them
    let mut log = social.copy_log(sub.id, 10).await;
    for _ in 0..40 {
        if log.iter().filter(|e| e["status"] == "done").count() >= 3 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        log = social.copy_log(sub.id, 10).await;
    }
    assert!(log.iter().filter(|e| e["status"] == "done").count() >= 3, "{log:?}");

    // the follower made ~ +96 USD: the daily fee (20 % above the 2 500 HWM) is charged and recorded pending
    let fee = social.settle_copy(sub.id, Utc::now(), false).await.unwrap().expect("fee charged");
    assert!(fee > D::ZERO);
    let hwm = social.reg.read().unwrap().subs[&sub.id].hwm;
    let eq = social.account_brief(sub.login).await.unwrap().equity;
    assert_eq!(hwm, eq, "HWM = equity after the fee");
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM social_fees WHERE sub_id = $1 AND status = 'pending'").bind(sub.id).fetch_one(&pool).await.unwrap();
    assert_eq!(pending, 1);
    // no second fee without new profit
    assert!(social.settle_copy(sub.id, Utc::now(), false).await.unwrap().is_none());

    // staff reject the fee (D76): the whole fee goes back to the follower's wallet as a `refund` credit, the master
    // gets nothing and the fee can't be reviewed twice
    let fee_id: i64 = sqlx::query_scalar("SELECT id FROM social_fees WHERE sub_id = $1 AND status = 'pending'").bind(sub.id).fetch_one(&pool).await.unwrap();
    let row = social.review_fee(1, fee_id, false, "IT", "charged by mistake").await.unwrap().expect("pending fee reviewed");
    assert_eq!(row.get::<String, _>("status"), "rejected");
    let refunds: Vec<(i64, D, String, String, String)> = sqlx::query_as("SELECT user_id, amount, direction, kind, reference FROM wallet_outbox WHERE idempotency_key = $1")
        .bind(format!("fee-refund:{fee_id}"))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(refunds, vec![(777, fee, "credit".into(), "refund".into(), format!("fee-refund:{fee_id}"))], "full fee refunded to the payer");
    let payouts: i64 = sqlx::query_scalar("SELECT count(*) FROM wallet_outbox WHERE idempotency_key = $1").bind(format!("fee:{fee_id}")).fetch_one(&pool).await.unwrap();
    assert_eq!(payouts, 0, "nothing paid to the master");
    assert!(social.review_fee(1, fee_id, true, "IT", "again").await.unwrap().is_none(), "a rejected fee is no longer pending");
    trading::social::wallet::flush(&pool, &social.wallet, |_| "kalks".into()).await;
    assert!(calls.lock().unwrap().iter().any(|x| x["kind"] == "refund" && x["direction"] == "credit" && x["user_id"] == 777 && x["amount"] == json!(fee.normalize().to_string())), "refund delivered to the wallet");

    // stop copying: nothing open, balance goes back to the wallet
    let out = social.stop_sub(sub.id, "client", true, true).await.unwrap();
    assert!(out["returned"].as_f64().unwrap() > 2500.0, "{out}");

    // stop copying but keep the positions (closePositions: false): mirroring stops, the copied position stays on
    // the copy account as an ordinary trade the client manages (no terminal guard), and the master's later close
    // no longer reaches it
    let sub2 = social.create_sub(1, 778, mid, Sizing { mode: SizingMode::Equity, value: D::ONE }, d("2000"), None, None, None, vec![]).await.unwrap();
    social.wallet.to_trading("kalks", &format!("copy:alloc:{}", sub2.id), 778, sub2.login, d("2000")).await.unwrap();
    for _ in 0..50 {
        if social.reg.read().unwrap().subs[&sub2.id].net_deposits == d("2000") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let open2 = exec(&hub, master_login, Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, d("0.4"))).map(|r| match r {
        trade::PlaceResult::Filled { position_ticket, .. } => json!(position_ticket),
        _ => Value::Null,
    })))
    .await;
    let mt2 = open2.as_i64().unwrap();
    wait_for(&hub, sub2.login, |p| p.len() == 1).await;
    assert_eq!(social.terminal_guard(sub2.login, false).map(|g| g.0), Some("copy_managed"), "copied trades are managed while copying");
    let kept = social.stop_sub(sub2.id, "client", false, false).await.unwrap();
    assert!(kept["closed"].as_array().unwrap().is_empty(), "{kept}");
    assert!(kept["returned"].is_null(), "{kept}");
    assert_eq!(positions(&hub, sub2.login).await.len(), 1, "the copied position stays open");
    assert!(social.terminal_guard(sub2.login, false).is_none(), "after the stop the client manages it");
    exec(&hub, master_login, Box::new(move |tx, env| trade::close_position(tx, env, mt2, CloseReq::default()).map(|_| Value::Null))).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(positions(&hub, sub2.login).await.len(), 1, "a stopped subscription mirrors nothing");
    // stopping it again (a repeated client call, a stale screen, a Back Office stop) never closes the positions the
    // client kept, and keeps the first stop's reason; the balance can still go back: only the free margin, rounded
    // down to the cent so the engine accepts the transfer while the position is open
    let free = social.account_brief(sub2.login).await.unwrap().withdrawable;
    let again = social.stop_sub(sub2.id, "admin", true, true).await.unwrap();
    assert!(again["closed"].as_array().unwrap().is_empty(), "{again}");
    assert_eq!(positions(&hub, sub2.login).await.len(), 1, "the kept position is the client's own trade");
    assert_eq!(social.reg.read().unwrap().subs[&sub2.id].stop_reason.as_deref(), Some("client"));
    assert!(again["returnError"].is_null(), "{again}");
    let back: D = again["returned"].to_string().parse().unwrap();
    assert!(back > D::ZERO && back <= free, "returned {back} of {free} free");

    // ---------------- PAMM ----------------
    let m = social.reg.read().unwrap().masters[&mid].clone();
    let (fund, creds) = social.create_fund(&m, "IT Fund", "weekly", d("20"), 0, d("100"), None, d("1000")).await.unwrap();
    let fl = creds["login"].as_i64().unwrap();
    let inv = social.invest_request(fund.id, 888, d("4000"), None).await.unwrap();
    assert_eq!(inv["status"], "pending");
    tokio::time::sleep(Duration::from_millis(20)).await;
    let before_r1 = Utc::now();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let r1 = social.rollover(fund.id, "manual").await.unwrap();
    assert_eq!(r1["nav"].as_f64().unwrap(), 1.0);
    assert_eq!(r1["unitsAfter"].as_f64().unwrap(), 5000.0);
    tokio::time::sleep(Duration::from_millis(20)).await;
    let after_r1 = Utc::now();
    // IB lots (D64) follow the units held when the position OPENED: a deal opened before the investor's units
    // were issued is the master's alone (the master's own units count), one opened after is split 1 000 : 4 000
    assert_eq!(social.ib_lot_shares(fund.id, d("0.5"), before_r1).await, vec![(501, d("0.5"))], "the investor joined after the open");
    let shares = social.ib_lot_shares(fund.id, d("0.37"), after_r1).await;
    assert_eq!(shares, vec![(501, d("0.074")), (888, d("0.296"))]);
    let tiny = social.ib_lot_shares(fund.id, d("0.0003"), after_r1).await;
    assert_eq!(tiny.iter().map(|s| s.1).sum::<D>(), d("0.0003"), "shares add up to the deal volume: {tiny:?}");
    // the manager makes 10 %: +500 on 5 000
    set_btc(&quotes, "80000", "80000");
    exec(&hub, fl, Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, d("0.5"))).map(|_| Value::Null))).await;
    set_btc(&quotes, "81000", "81000");
    exec(&hub, fl, Box::new(|tx, env| {
        trade::bulk_close(tx, env, trade::BulkFilter::All, None);
        Ok(Value::Null)
    }))
    .await;
    social.redeem_request(fund.id, 888, None, None, true).await.unwrap();
    let r2 = social.rollover(fund.id, "manual").await.unwrap();
    assert_eq!(r2["nav"].as_f64().unwrap(), 1.1);
    // investor: 4 000 units, fee 20 % × 0.1 × 4 000 = 80; redeems the rest: 4 400 − 80 = 4 320
    assert_eq!(r2["feesTotal"].as_f64().unwrap(), 80.0);
    assert_eq!(r2["redeemed"].as_f64().unwrap(), 4320.0);
    assert_eq!(r2["unitsAfter"].as_f64().unwrap(), 1000.0);
    // the investor has redeemed everything now, but a deal opened while they held units is still partly theirs
    assert_eq!(social.ib_lot_shares(fund.id, d("0.37"), after_r1).await, shares, "redeemed after the open: still credited");
    assert_eq!(social.ib_lot_shares(fund.id, d("0.37"), Utc::now()).await, vec![(501, d("0.37"))]);
    let b = social.account_brief(fl).await.unwrap();
    assert_eq!(trading::social::math::nav(b.equity, d("1000")), d("1.1"), "NAV unchanged by fee and redemption");
    // units = Σ unit ledger per investor
    let bad: i64 = sqlx::query_scalar("SELECT count(*) FROM pamm_investors i WHERE i.units <> COALESCE((SELECT sum(units) FROM pamm_unit_ledger l WHERE l.investor_id = i.id), 0)").fetch_one(&pool).await.unwrap();
    assert_eq!(bad, 0);
    // wallet calls: seed + invest debits; redemption credit through the outbox
    trading::social::wallet::flush(&pool, &social.wallet, |_| "kalks".into()).await;
    let c = calls.lock().unwrap().clone();
    assert!(c.iter().any(|x| x["kind"] == "pamm_invest" && x["direction"] == "debit" && x["amount"] == "4000"));
    assert!(c.iter().any(|x| x["kind"] == "pamm_redeem" && x["direction"] == "credit" && x["amount"] == "4320"));

    // replay determinism: every account rebuilt from `events` equals the live state; the ledger balances
    let mut live_states = Vec::new();
    let all: Vec<i64> = sqlx::query_scalar("SELECT login FROM accounts ORDER BY login").fetch_all(&pool).await.unwrap();
    for l in &all {
        live_states.push((*l, state(&hub, *l).await));
    }
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for (l, v) in live_states {
        assert_eq!(serde_json::to_value(&replayed[&l]).unwrap(), v, "replay diverged for {l}");
    }
    assert!(trading::persist::verify_balances(&pool, &replayed).await.unwrap().is_empty());

    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

//! Copy trading following lifecycle end to end against PostgreSQL (Task 1 A6–A11), through the real shards, the
//! event tap, the copier and a mock wallet + support service (axum on a random port). Checks:
//! - A9 auto SL: a copied trade gets the follower's SL N pips from its entry; a tighter master SL wins, a looser
//!   one does not loosen it;
//! - A10 execution report: copy log rows carry master / follower price, slippage and delay;
//! - A7 alerts: opened (in-app only), protection stop, terms, master stopped land in `notify_outbox` and are
//!   delivered to support `POST /v1/notify` with their dedupe keys;
//! - A6: add funds / withdraw (capped at the free margin) move the HWM tracking;
//! - A8: a higher fee waits for acceptance and pauses the copy after the deadline; accepting settles the period so
//!   far and switches the terms; a lower fee applies at once; a frozen master flags its followers;
//! - replaying the `events` table rebuilds the live state of every account.
//!
//! Needs the local Postgres (127.0.0.1:5433, see README); skipped when it is not reachable.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::post;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use trading::engine::trade::{self, CloseReq, OrderReq, PositionPatch};
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
struct Mock {
    hub: Hub,
    calls: Arc<Mutex<Vec<Value>>>,
}

async fn wallet_trading(State(w): State<Mock>, Path((user, dir)): Path<(i64, String)>, axum::Json(b): axum::Json<Value>) -> axum::Json<Value> {
    w.calls.lock().unwrap().push(json!({"user": user, "dir": dir, "body": b}));
    let login = b["login"].as_i64().unwrap();
    let amount: D = b["amount"].as_str().unwrap().parse().unwrap();
    let key = format!("transfer:{}", b["idempotency_key"].as_str().unwrap());
    let direction = if dir == "to-trading" { funds::Direction::In } else { funds::Direction::Out };
    let op: Op = Box::new(move |tx, env| funds::transfer(tx, env, direction, amount, &key, None).map(|_| Value::Null));
    w.hub.exec(login, "wallet", None, "", "", None, op).await.unwrap();
    axum::Json(json!({"status": "completed"}))
}

async fn support_notify(State(w): State<Mock>, axum::Json(b): axum::Json<Value>) -> axum::Json<Value> {
    w.calls.lock().unwrap().push(json!({"notify": b}));
    axum::Json(json!({"results": [{"duplicate": false}]}))
}

fn account(login: i64, user: i64) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: user,
        kind: AccountKind::Live,
        group: "standard".into(),
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

/// Positions of `login` as (volume, sl).
async fn positions(hub: &Hub, login: i64) -> Vec<(D, Option<D>)> {
    let s = state(hub, login).await;
    s["positions"].as_object().map(|m| m.values().map(|p| (p["volume"].as_str().unwrap().parse().unwrap(), p["sl"].as_str().map(|x| x.parse().unwrap()))).collect()).unwrap_or_default()
}

async fn wait_for<F: Fn(&[(D, Option<D>)]) -> bool>(hub: &Hub, login: i64, f: F) -> Vec<(D, Option<D>)> {
    for _ in 0..100 {
        let p = positions(hub, login).await;
        if f(&p) {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not reached on {login}: {:?}", positions(hub, login).await);
}

async fn outbox(pool: &sqlx::PgPool, user: i64, kind: &str) -> Vec<Value> {
    sqlx::query_scalar::<_, sqlx::types::Json<Value>>("SELECT data FROM notify_outbox WHERE user_id = $1 AND kind = $2 ORDER BY id").bind(user).bind(kind).fetch_all(pool).await.unwrap().into_iter().map(|j| j.0).collect()
}

async fn wait_outbox(pool: &sqlx::PgPool, user: i64, kind: &str) -> Vec<Value> {
    for _ in 0..60 {
        let v = outbox(pool, user, kind).await;
        if !v.is_empty() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("no {kind} notification for {user}");
}

fn set_btc(q: &QuoteBook, bid: &str, ask: &str) {
    q.set("standard", "BTCUSD", Quote { bid: d(bid), ask: d(ask), t_ms: Utc::now().timestamp_millis() });
}

#[tokio::test]
async fn following_lifecycle_end_to_end() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_copylife_{}", std::process::id());
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

    // mock wallet + support
    let calls: Arc<Mutex<Vec<Value>>> = Default::default();
    let app = Router::new()
        .route("/v1/wallets/{user}/{dir}", post(wallet_trading))
        .route("/v1/notify", post(support_notify))
        .with_state(Mock { hub: hub.clone(), calls: calls.clone() });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let logins = Arc::new(trading::api::LoginAlloc { live: AtomicI64::new(live.max(10_000_700)), demo: AtomicI64::new(demo) });
    let wallet = trading::social::wallet::WalletClient::new(&format!("http://{addr}"), "t");
    let social = Social::new(pool.clone(), hub.clone(), wallet, logins).await.unwrap();
    social.start();

    // master: approved, 20 % monthly
    let master_login = 10_000_601;
    hub.open(account(master_login, 601), ("h".into(), "i".into()), "test").await.unwrap();
    exec(&hub, master_login, Box::new(|tx, env| funds::transfer(tx, env, funds::Direction::In, d("10000"), "m-fund", None).map(|_| Value::Null))).await;
    let mid: i64 = sqlx::query_scalar("INSERT INTO social_masters (tenant_id, user_id, login, nickname, program, perf_fee_pct, fee_period, status, approved_at) VALUES (1, 601, $1, 'Lifecycle', 'copy', 20, 'monthly', 'approved', now()) RETURNING id")
        .bind(master_login)
        .fetch_one(&pool)
        .await
        .unwrap();
    let reg = trading::social::load(&pool).await.unwrap();
    social.reg.write().unwrap().masters = reg.masters;
    let m = social.reg.read().unwrap().masters[&mid].clone();
    assert!(m.accept_new && !m.invite_only && m.max_followers.is_none(), "A11 defaults");

    // follower with an auto SL of 100 pips (BTCUSD pip = 1)
    let user = 901;
    let mut sub = social.create_sub(1, user, mid, Sizing { mode: SizingMode::Equity, value: D::ONE }, d("2500"), None, None, None, vec![]).await.unwrap();
    sub.auto_sl_pips = Some(d("100"));
    social.save_sub(&sub).await.unwrap();
    social.wallet.to_trading("kalks", &format!("copy:alloc:{}", sub.id), user, sub.login, d("2500")).await.unwrap();
    for _ in 0..50 {
        if social.reg.read().unwrap().subs[&sub.id].net_deposits == d("2500") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].auto_sl_pips, Some(d("100")), "auto SL persisted");

    // A9: master buys without SL → the copy gets SL = entry (ask 80 020) − 100
    let open = exec(&hub, master_login, Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, d("0.4"))).map(|r| match r {
        trade::PlaceResult::Filled { position_ticket, .. } => json!(position_ticket),
        _ => Value::Null,
    })))
    .await;
    let mt = open.as_i64().unwrap();
    let p = wait_for(&hub, sub.login, |p| p.len() == 1).await;
    assert_eq!(p[0].1, Some(d("79920")), "auto SL 100 pips below the entry");
    // a tighter master SL wins …
    exec(&hub, master_login, Box::new(move |tx, env| trade::modify_position(tx, env, mt, PositionPatch { sl: Some(Some(d("79950"))), tp: None, trailing_points: None }, None).map(|_| Value::Null))).await;
    wait_for(&hub, sub.login, |p| p.len() == 1 && p[0].1 == Some(d("79950"))).await;
    // … a looser one is capped by the follower's own SL
    exec(&hub, master_login, Box::new(move |tx, env| trade::modify_position(tx, env, mt, PositionPatch { sl: Some(Some(d("79000"))), tp: None, trailing_points: None }, None).map(|_| Value::Null))).await;
    wait_for(&hub, sub.login, |p| p.len() == 1 && p[0].1 == Some(d("79920"))).await;

    // A10: the copied open carries the execution figures
    let mut rep = social.execution_report(sub.id, 10).await;
    for _ in 0..40 {
        if rep["summary"]["trades"].as_u64().unwrap_or(0) >= 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        rep = social.execution_report(sub.id, 10).await;
    }
    let row = &rep["items"][0];
    assert_eq!(row["action"], "open", "{rep}");
    assert_eq!(row["masterPrice"].as_f64(), Some(80020.0), "{rep}");
    assert_eq!(row["followerPrice"].as_f64(), Some(80020.0), "{rep}");
    assert_eq!(row["slippagePips"].as_f64(), Some(0.0), "{rep}");
    assert!(row["delayMs"].as_i64().unwrap() >= 0, "{rep}");

    // A7: the open is an in-app alert (email off)
    let opened = wait_outbox(&pool, user, "copy.trade_opened").await;
    assert_eq!(opened[0]["email"], false, "{opened:?}");
    assert_eq!(opened[0]["data"]["subscriptionId"], sub.id);

    // the master closes at a higher price: the follower closes too, with the close's figures
    set_btc(&quotes, "80100", "80120");
    exec(&hub, master_login, Box::new(move |tx, env| trade::close_position(tx, env, mt, CloseReq::default()).map(|_| Value::Null))).await;
    wait_for(&hub, sub.login, |p| p.is_empty()).await;
    let closed = wait_outbox(&pool, user, "copy.trade_closed").await;
    assert_eq!(closed.len(), 1);

    // A6: add funds from the wallet, withdraw up to the free margin
    let (added, _) = social.sub_funds(sub.id, true, d("500")).await.unwrap();
    assert_eq!(added, d("500"));
    let before = social.reg.read().unwrap().subs[&sub.id].net_deposits;
    for _ in 0..50 {
        if social.reg.read().unwrap().subs[&sub.id].net_deposits > d("2500") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].net_deposits, d("3000"), "the deposit moves the HWM tracking (was {before})");
    let free = social.account_brief(sub.login).await.unwrap().withdrawable;
    let too_much = social.sub_funds(sub.id, false, (free + d("1")).round_dp(2)).await.unwrap_err();
    assert_eq!(too_much.code, "insufficient_funds");
    assert!(social.sub_funds(sub.id, false, d("0.001")).await.is_err(), "cents only");
    social.sub_funds(sub.id, false, d("200")).await.unwrap();
    for _ in 0..50 {
        if social.reg.read().unwrap().subs[&sub.id].net_deposits == d("2800") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].net_deposits, d("2800"));

    // A8: a higher fee waits for acceptance …
    let mut m2 = m.clone();
    m2.perf_fee_pct = d("30");
    social.save_master(&m2).await.unwrap();
    assert_eq!(social.on_terms_changed(&m2).await, (0, 1));
    let s = social.reg.read().unwrap().subs[&sub.id].clone();
    assert_eq!((s.perf_fee_pct, s.pending_fee_pct, s.terms_deadline.is_some()), (d("20"), Some(d("30")), true));
    assert_eq!(wait_outbox(&pool, user, "copy.terms_changed").await.len(), 1);
    // … and the copy pauses after the deadline
    assert_eq!(social.pause_expired_terms(Utc::now()).await, 0, "not yet");
    assert_eq!(social.pause_expired_terms(Utc::now() + chrono::Duration::days(8)).await, 1);
    let s = social.reg.read().unwrap().subs[&sub.id].clone();
    assert_eq!((s.status.as_str(), s.pause_reason.as_deref()), ("paused", Some("terms")));
    assert_eq!(wait_outbox(&pool, user, "copy.paused_terms").await.len(), 1);
    // accepting settles at the old rate, switches the terms and resumes
    let s = social.accept_terms(sub.id).await.unwrap();
    assert_eq!((s.status.as_str(), s.perf_fee_pct, s.pending_fee_pct, s.terms_deadline, s.pause_reason.clone()), ("active", d("30"), None, None, None));
    assert!(social.accept_terms(sub.id).await.is_err(), "nothing left to accept");
    // a lower fee applies at once
    let mut m3 = m2.clone();
    m3.perf_fee_pct = d("10");
    social.save_master(&m3).await.unwrap();
    assert_eq!(social.on_terms_changed(&m3).await, (1, 0));
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].perf_fee_pct, d("10"));

    // A8: a frozen master flags its followers; unfreezing clears the flag
    let mut m4 = m3.clone();
    m4.frozen = true;
    social.save_master(&m4).await.unwrap();
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].attention.as_deref(), Some("master_stopped"));
    assert_eq!(wait_outbox(&pool, user, "copy.master_stopped").await.len(), 1);
    m4.frozen = false;
    social.save_master(&m4).await.unwrap();
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].attention, None);

    // A7: the equity stop stops the copy and alerts the follower
    let mut s = social.reg.read().unwrap().subs[&sub.id].clone();
    s.equity_stop = Some(d("100000"));
    social.save_sub(&s).await.unwrap();
    social.guard_once().await;
    assert_eq!(social.reg.read().unwrap().subs[&sub.id].status, "stopped");
    let stop = wait_outbox(&pool, user, "copy.protection_stop").await;
    assert_eq!(stop[0]["severity"], "critical");

    // the outbox goes to support with the dedupe keys; a second flush sends nothing again
    let support = trading::social::wallet::WalletClient::new(&format!("http://{addr}"), "t");
    let sent = trading::notify::flush(&pool, &support).await;
    assert!(sent >= 7, "sent {sent}");
    assert_eq!(trading::notify::flush(&pool, &support).await, 0);
    let c = calls.lock().unwrap().clone();
    let n: Vec<&Value> = c.iter().filter_map(|x| x.get("notify")).collect();
    assert!(n.iter().any(|b| b["type"] == "copy.trade_opened" && b["email"] == false && b["userId"] == user && b["dedupeKey"].as_str().unwrap().starts_with("copy:")));
    assert!(n.iter().any(|b| b["type"] == "copy.protection_stop" && b.get("email").is_none()));
    // enqueueing the same event twice is a no-op
    trading::notify::enqueue(&pool, 1, user, "copy.test", json!({"title": "x"}), "dup-key").await.unwrap();
    trading::notify::enqueue(&pool, 1, user, "copy.test", json!({"title": "x"}), "dup-key").await.unwrap();
    assert_eq!(outbox(&pool, user, "copy.test").await.len(), 1);

    // replay determinism
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

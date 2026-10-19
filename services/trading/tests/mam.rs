//! MAM end to end against PostgreSQL, through the real shards, the event tap and the copier:
//! - an approved master opens a MAM programme (engine-opened master account in group `mam`);
//! - two clients link their own live accounts with the consent hash of the current terms;
//! - a block on the master account is allocated by equity share (0.50 lot → 0.30 / 0.20), executed on both
//!   client accounts with source `mam`, and recorded in the allocation audit;
//! - the client terminal refuses to close a MAM position (readable reason);
//! - the master's partial and full close follow on both accounts;
//! - the performance fee is 20 % of the MAM result above the HWM, debited and recorded pending;
//! - a revoked link gets no further allocations (the whole next block goes to the remaining account);
//! - the equity stop closes the MAM trades and stops the link;
//! - replaying `events` rebuilds every account exactly and every ledger transaction balances.
//!
//! Needs the local Postgres (127.0.0.1:5433, see README); skipped when it is not reachable.

use chrono::Utc;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use trading::engine::trade::{self, CloseReq, OrderReq};
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::social::Social;
use trading::social::allocation::Method;
use trading::specs::Specs;

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
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

async fn fund(hub: &Hub, login: i64, amount: &str) {
    let (a, k) = (d(amount), format!("it-fund-{login}"));
    exec(hub, login, Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, a, &k, None).map(|_| Value::Null))).await;
}

/// Positions of `login` as (ticket, volume, source).
async fn positions(hub: &Hub, login: i64) -> Vec<(i64, D, String)> {
    let s = hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await;
    let mut v: Vec<(i64, D, String)> = s["positions"]
        .as_object()
        .map(|m| m.values().map(|p| (p["ticket"].as_i64().unwrap(), p["volume"].as_str().unwrap().parse().unwrap(), p["source"].as_str().unwrap().to_string())).collect())
        .unwrap_or_default();
    v.sort();
    v
}

async fn wait_for<F: Fn(&[(i64, D, String)]) -> bool>(hub: &Hub, login: i64, f: F) -> Vec<(i64, D, String)> {
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

fn buy(volume: &str) -> Op {
    let v = d(volume);
    Box::new(move |tx, env| {
        trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, v)).map(|r| match r {
            trade::PlaceResult::Filled { position_ticket, .. } => json!(position_ticket),
            _ => Value::Null,
        })
    })
}

#[tokio::test]
async fn mam_link_allocate_close_fee_revoke_with_replay() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_mam_{}", std::process::id());
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
    set_btc(&quotes, "80000", "80000");

    let logins = Arc::new(trading::api::LoginAlloc { live: AtomicI64::new(live.max(10_000_700)), demo: AtomicI64::new(demo) });
    let wallet = trading::social::wallet::WalletClient::new("", "");
    let social = Social::new(pool.clone(), hub.clone(), wallet, logins).await.unwrap();
    social.start();

    // the master (manager) and two clients with their own live accounts: 6 000 and 4 000
    let own = 10_000_401;
    hub.open(account(own, 601), ("h".into(), "i".into()), "test").await.unwrap();
    let mid: i64 = sqlx::query_scalar("INSERT INTO social_masters (tenant_id, user_id, login, nickname, program, perf_fee_pct, fee_period, status, approved_at) VALUES (1, 601, $1, 'Alpha', 'copy', 20, 'daily', 'approved', now()) RETURNING id")
        .bind(own)
        .fetch_one(&pool)
        .await
        .unwrap();
    social.reg.write().unwrap().masters = trading::social::load(&pool).await.unwrap().masters;
    let (c1, c2) = (10_000_411, 10_000_412);
    hub.open(account(c1, 701), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(c2, 702), ("h".into(), "i".into()), "test").await.unwrap();
    fund(&hub, c1, "6000").await;
    fund(&hub, c2, "4000").await;

    // MAM programme: equity share, 20 % performance fee, daily
    let master = social.reg.read().unwrap().masters[&mid].clone();
    let (m, creds) = social.create_manager(&master, "Alpha MAM", "", Method::Equity, d("20"), D::ZERO, "daily", d("100"), None).await.map_err(|e| e.message).unwrap();
    let ml = creds["login"].as_i64().unwrap();
    assert_eq!(ml, m.login);
    assert_eq!(social.account_brief(ml).await.unwrap().group, "mam");
    fund(&hub, ml, "5000").await;

    // consent: a stale hash is refused, the current one links
    let (_, hash) = social.manager_terms(&m);
    let stale = social.create_link(1, 701, m.id, c1, None, None, &"0".repeat(64), None, None).await;
    assert_eq!(stale.err().map(|e| e.code), Some("terms_changed"));
    // the manager can't link an account of someone else, nor their own programme
    assert!(social.create_link(1, 702, m.id, c1, None, None, &hash, None, None).await.is_err());
    assert_eq!(social.create_link(1, 601, m.id, own, None, None, &hash, None, None).await.err().map(|e| e.code), Some("own_programme"));
    let l1 = social.create_link(1, 701, m.id, c1, None, None, &hash, Some("127.0.0.1".into()), Some("it".into())).await.map_err(|e| e.message).unwrap();
    let l2 = social.create_link(1, 702, m.id, c2, None, None, &hash, None, None).await.map_err(|e| e.message).unwrap();
    assert_eq!(social.create_link(1, 701, m.id, c1, None, None, &hash, None, None).await.err().map(|e| e.code), Some("not_eligible"), "already managed");

    // block 0.50 BTC → 0.30 (6 000 / 10 000) and 0.20
    let mt = exec(&hub, ml, buy("0.5")).await.as_i64().unwrap();
    let p1 = wait_for(&hub, c1, |p| p.len() == 1).await;
    let p2 = wait_for(&hub, c2, |p| p.len() == 1).await;
    assert_eq!((p1[0].1, p1[0].2.as_str()), (d("0.3"), "mam"));
    assert_eq!((p2[0].1, p2[0].2.as_str()), (d("0.2"), "mam"));
    let mut alloc = Vec::new();
    for _ in 0..50 {
        alloc = social.allocations("manager_id = $1", m.id, 10, false).await;
        if !alloc.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    assert_eq!(alloc.len(), 1, "one allocation audit row");
    assert_eq!((alloc[0]["block"].as_f64(), alloc[0]["allocated"].as_f64(), alloc[0]["accounts"].as_i64()), (Some(0.5), Some(0.5), Some(2)));
    let det = alloc[0]["details"].as_array().unwrap();
    let vol = |login: i64| det.iter().find(|x| x["login"] == login).map(|x| (x["volume"].as_f64(), x["status"].as_str().map(str::to_string)));
    assert_eq!(vol(c1), Some((Some(0.3), Some("done".into()))));
    assert_eq!(vol(c2), Some((Some(0.2), Some("done".into()))));

    // the client's terminal refuses to touch the MAM trade, but a manual trade next to it is fine
    let g = social.mam_terminal_guard(c1, vec![p1[0].0], false).await;
    assert_eq!(g.as_ref().map(|x| x.0), Some("mam_managed"));
    assert!(g.unwrap().1.contains("Alpha MAM"));
    assert!(social.mam_terminal_guard(c1, vec![], true).await.is_some(), "bulk close blocked while MAM trades are open");
    let manual = exec(&hub, c1, buy("0.01")).await.as_i64().unwrap();
    assert!(social.mam_terminal_guard(c1, vec![manual], false).await.is_none());
    exec(&hub, c1, Box::new(move |tx, env| trade::close_position(tx, env, manual, CloseReq::default()).map(|_| Value::Null))).await;

    // +1 000 on BTC; the master closes 0.25 of 0.50 → the clients close half (0.15 / 0.10), then the rest
    set_btc(&quotes, "81000", "81000");
    exec(&hub, ml, Box::new(move |tx, env| trade::close_position(tx, env, mt, CloseReq { volume: Some(d("0.25")), ..Default::default() }).map(|_| Value::Null))).await;
    wait_for(&hub, c1, |p| p.len() == 1 && p[0].1 == d("0.15")).await;
    wait_for(&hub, c2, |p| p.len() == 1 && p[0].1 == d("0.1")).await;
    exec(&hub, ml, Box::new(move |tx, env| trade::close_position(tx, env, mt, CloseReq::default()).map(|_| Value::Null))).await;
    wait_for(&hub, c1, |p| p.is_empty()).await;
    wait_for(&hub, c2, |p| p.is_empty()).await;

    // MAM result of client 1: 0.30 × 1 000 = +300 (no commission on the standard group) → fee 20 % = 60
    let l1v = social.link(l1.id).unwrap();
    let res = social.link_result(&l1v).await.unwrap();
    assert_eq!((res.realized, res.floating), (d("300"), D::ZERO), "{res:?}");
    let fee = social.settle_link(l1.id, Utc::now(), false).await.unwrap().expect("fee charged");
    assert_eq!(fee, d("60.00"));
    assert_eq!(social.link(l1.id).unwrap().hwm, d("300.00"));
    let row: (String, D, D, Option<i64>) = sqlx::query_as("SELECT status, amount, perf_amount, link_id FROM social_fees WHERE source = 'mam' AND link_id = $1").bind(l1.id).fetch_one(&pool).await.unwrap();
    assert_eq!(row, ("pending".to_string(), d("60.00"), d("60.00"), Some(l1.id)));
    assert_eq!(social.account_brief(c1).await.unwrap().balance, d("6240.00"), "6 000 + 300 − 60");
    // nothing new above the HWM → no second fee
    assert!(social.settle_link(l1.id, Utc::now(), false).await.unwrap().is_none());

    // client 2 revokes: the next block goes entirely to client 1
    let out = social.end_link(l2.id, "revoked", "client", "user:702", false).await.unwrap();
    assert!(out["closed"].as_array().unwrap().is_empty());
    assert_eq!(social.link(l2.id).unwrap().status, "revoked");
    // client 2's own 20 % fee was settled on revoke: 0.20 × 1 000 = 200 → 40
    assert_eq!(out["fee"].as_f64(), Some(40.0));
    let mt2 = exec(&hub, ml, buy("0.4")).await.as_i64().unwrap();
    let p1 = wait_for(&hub, c1, |p| p.len() == 1).await;
    assert_eq!(p1[0].1, d("0.4"));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(positions(&hub, c2).await.is_empty(), "a revoked link gets nothing");
    assert!(social.mam_terminal_guard(c2, vec![], true).await.is_none(), "no guard once revoked");

    // equity stop: client 1 sets 6 100; BTC falls 400 → equity ≈ 6 080 → the MAM trade is closed, link stopped
    {
        let mut l = social.link(l1.id).unwrap();
        l.equity_stop = Some(d("6100"));
        social.save_link(&l).await.unwrap();
    }
    set_btc(&quotes, "80600", "80600");
    social.guard_once().await;
    wait_for(&hub, c1, |p| p.is_empty()).await;
    assert_eq!(social.link(l1.id).unwrap().status, "stopped");
    assert_eq!(social.link(l1.id).unwrap().stop_reason.as_deref(), Some("equity_stop"));
    exec(&hub, ml, Box::new(move |tx, env| trade::close_position(tx, env, mt2, CloseReq::default()).map(|_| Value::Null))).await;

    // replay determinism and balanced ledger
    let mut live_states = Vec::new();
    let all: Vec<i64> = sqlx::query_scalar("SELECT login FROM accounts ORDER BY login").fetch_all(&pool).await.unwrap();
    for l in &all {
        live_states.push((*l, hub.read(*l, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await));
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

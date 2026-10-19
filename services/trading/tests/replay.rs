//! Integration test against PostgreSQL: runs a realistic session through the shards (single writer,
//! commits to the database), then rebuilds every account by replaying the `events` table and checks the
//! result is identical to the live in-memory state. Also checks the database-level ledger guarantees
//! (Σ postings = 0 per transaction, append-only tables, idempotency keys).
//!
//! Needs the local Postgres (127.0.0.1:5433, see README). Uses a throw-away database
//! `kalks_trading_test_<pid>`; skipped with a message when Postgres is not reachable.
//! Override with TRADING_TEST_DATABASE_URL (a server URL; the database name is replaced).

use chrono::Utc;
use serde_json::Value;
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};

use trading::engine::trade::{CloseReq, DealerCtx, OrderReq};
use trading::engine::{Ids, Quote, dealing, funds, risk, trade};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, DemoCfg, OrderType, Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{ExecError, Hub, Index, NullLp, Op, Shared, Staff, Stats, Streams};
use trading::specs::Specs;

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn account(login: i64, group: &str, kind: AccountKind, cent: bool, mode: trading::model::Mode) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: 77,
        kind,
        group: group.into(),
        mode,
        cent,
        leverage: 100,
        status: Status::Active,
        name: "IT".into(),
        route_override: None,
        controls: Controls::default(),
        demo: (kind == AccountKind::Demo).then(|| DemoCfg { initial_balance: d("10000"), refills_per_day: 3, expiry_days: 10 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

async fn exec(hub: &Hub, login: i64, op: Op) -> Result<Value, ExecError> {
    hub.exec(login, "test", Some(Staff { id: "1".into(), name: "IT".into(), role: "dealer".into() }), "DLR-01", "it", None, op).await.map(|d| d.value)
}

async fn snapshot(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await
}

fn set_quotes(q: &QuoteBook, eur: (&str, &str), btc: (&str, &str)) {
    let t = Utc::now().timestamp_millis();
    for g in ["standard", "pro", "ecn", "cent"] {
        q.set(g, "EURUSD", Quote { bid: d(eur.0), ask: d(eur.1), t_ms: t });
        q.set(g, "BTCUSD", Quote { bid: d(btc.0), ask: d(btc.1), t_ms: t });
        q.set(g, "USDJPY", Quote { bid: d("150.000"), ask: d("150.010"), t_ms: t });
    }
}

#[tokio::test]
async fn replay_rebuilds_identical_state_and_ledger_holds() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_test_{}", std::process::id());
    let server = match PgConnectOptions::from_str(&base) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("SKIP: bad TRADING_TEST_DATABASE_URL: {e}");
            return;
        }
    };
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
    let (ticket, deal, txn, _, _) = trading::persist::max_ids(&pool).await.unwrap();
    let quotes = Arc::new(QuoteBook::default());
    let shared = Arc::new(Shared {
        pool: pool.clone(),
        registry: registry.clone(),
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
    let _ = AtomicI64::new(0);
    let hub = Hub::start(shared, 4, Default::default());
    set_quotes(&quotes, ("1.10000", "1.10010"), ("80000", "80020"));

    // accounts: hedging live (ECN, commission), netting live, cent live, demo
    let logins = [10_000_101i64, 10_000_102, 10_000_103, 50_000_101];
    hub.open(account(logins[0], "ecn", AccountKind::Live, false, trading::model::Mode::Hedging), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(logins[1], "pro-netting", AccountKind::Live, false, trading::model::Mode::Netting), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(logins[2], "cent", AccountKind::Live, true, trading::model::Mode::Hedging), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(logins[3], "standard", AccountKind::Demo, false, trading::model::Mode::Hedging), ("h".into(), "i".into()), "test").await.unwrap();

    for (i, l) in logins[..3].iter().enumerate() {
        let key = format!("it-fund-{i}");
        exec(&hub, *l, Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, d("10000"), &key, None).map(|_| Value::Null))).await.unwrap();
    }
    // ledger idempotency at the database: the same key again is refused by the unique index
    let dup = exec(&hub, logins[0], Box::new(|tx, env| funds::transfer(tx, env, funds::Direction::In, d("10000"), "it-fund-0", None).map(|_| Value::Null))).await;
    assert!(matches!(dup, Err(ExecError::Duplicate(_))), "duplicate key must be refused: {dup:?}");

    // trading on every account
    for l in logins {
        exec(&hub, l, Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("1"))).map(|_| Value::Null))).await.unwrap();
        exec(&hub, l, Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Sell, d("0.5"))).map(|_| Value::Null))).await.unwrap();
        exec(&hub, l, Box::new(|tx, env| trade::place_order(tx, env, OrderReq { kind: OrderType::Limit, price: Some(d("1.09")), ..OrderReq::market("EURUSD", Side::Buy, d("0.3")) }).map(|_| Value::Null))).await.unwrap();
    }
    // netting reversal
    exec(&hub, logins[1], Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Sell, d("2.5"))).map(|_| Value::Null))).await.unwrap();

    // prices move: limit orders fill, partial closes, SL, swaps, dealer ops
    set_quotes(&quotes, ("1.08990", "1.09000"), ("81000", "81020"));
    for l in logins {
        exec(&hub, l, Box::new(|tx, env| {
            risk::on_tick(tx, env, "EURUSD");
            risk::on_tick(tx, env, "BTCUSD");
            Ok(Value::Null)
        }))
        .await
        .unwrap();
    }
    let hedge = snapshot(&hub, logins[0]).await;
    let first: i64 = hedge["positions"].as_object().unwrap().keys().next().unwrap().parse().unwrap();
    exec(&hub, logins[0], Box::new(move |tx, env| trade::close_position(tx, env, first, CloseReq { volume: Some(d("0.4")), ..Default::default() }).map(|_| Value::Null))).await.unwrap();
    let dealer = DealerCtx { staff: "IT".into(), reason_code: "DLR-03".into(), force: false };
    let dl = dealer.clone();
    exec(&hub, logins[0], Box::new(move |tx, env| dealing::transfer_book(tx, env, first, trading::model::Book::A, dealing::BookMove::Volume(d("0.2")), &dl).map(|_| Value::Null))).await.unwrap();
    let day = trading::specs::server_date(Utc::now()).pred_opt().unwrap();
    for l in logins {
        exec(&hub, l, Box::new(move |tx, env| {
            risk::rollover(tx, env, day, Utc::now());
            Ok(Value::Null)
        }))
        .await
        .unwrap();
    }
    exec(&hub, logins[3], Box::new(|tx, env| trade::bulk_close(tx, env, trade::BulkFilter::All, None).done.is_empty().then_some(()).map_or(Ok(Value::Null), |_| Ok(Value::Null)))).await.unwrap();
    exec(&hub, logins[3], Box::new(|tx, env| funds::demo_refill(tx, env).map(|_| Value::Null))).await.ok();
    exec(&hub, logins[2], Box::new(|tx, env| funds::adjust(tx, env, funds::AdjustKind::Credit, d("500"), "it-credit", "BON", "it").map(|_| Value::Null))).await.unwrap();
    // Back Office "Balance & credit": manual adjustments replay too, and one idempotency key books once
    let manual = |op: funds::AdjustOp, cat: &'static str, amt: &'static str, key: &'static str| -> Op {
        Box::new(move |tx, env| funds::staff_adjust(tx, env, funds::StaffAdjust { op, category: cat, amount: d(amt), force: false, key, reason_code: "ADJ", statement: "IT" }).map(|_| Value::Null))
    };
    exec(&hub, logins[2], manual(funds::AdjustOp::Add, "compensation", "100", "adj:it-1")).await.unwrap();
    exec(&hub, logins[2], manual(funds::AdjustOp::CreditOut, "correction", "200", "adj:it-2")).await.unwrap();
    let again = exec(&hub, logins[2], manual(funds::AdjustOp::Add, "compensation", "100", "adj:it-1")).await;
    assert!(matches!(again, Err(ExecError::Duplicate(_))), "a repeated adjustment key must not book twice: {again:?}");

    // live state vs. replayed state
    let mut live = Vec::new();
    for l in logins {
        live.push(snapshot(&hub, l).await);
    }
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    assert_eq!(replayed.len(), logins.len());
    for (i, l) in logins.iter().enumerate() {
        let r = serde_json::to_value(&replayed[l]).unwrap();
        assert_eq!(r, live[i], "replay diverged for {l}");
    }
    assert!(trading::persist::verify_balances(&pool, &replayed).await.unwrap().is_empty());
    // positions exist and ledger nets to zero per currency
    assert!(replayed.values().any(|s| !s.positions.is_empty()));
    let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(&pool).await.unwrap();
    assert!(nets.iter().all(|(_, v)| v.is_zero()), "ledger nets: {nets:?}");

    // database guarantees
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO ledger_txns (id, tenant_id, idempotency_key, kind, login, created_at) VALUES (9999999, 1, 'it-bad', 'adjustment', 1, now())").execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO ledger_postings (tenant_id, txn_id, account_code, currency, amount) VALUES (1, 9999999, 'x', 'USD', 5)").execute(&mut *tx).await.unwrap();
    assert!(tx.commit().await.is_err(), "an unbalanced ledger transaction must not commit");
    assert!(sqlx::query("UPDATE events SET kind = 'x' WHERE seq = 1").execute(&pool).await.is_err(), "events are append-only");
    assert!(sqlx::query("DELETE FROM ledger_postings").execute(&pool).await.is_err(), "postings are append-only");

    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

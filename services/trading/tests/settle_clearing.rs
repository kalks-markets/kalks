//! Order book SETTLEMENT through the clearing account on the PostgreSQL harness (docs/OPTIONS-EXCHANGE.md §9):
//! a USD account short 3 and three cent accounts long 1 each trade through the real book actor, outbox and shards;
//! at the fixing each side settles against `house:options_clearing.{U}.{YYYYMMDD}:USD` (the cent ones in the
//! 4-leg form through `house:fx`), never against `house:options_settlement`; the per-position rounding left in the
//! clearing account (here 0.01 USD) is swept to `house:options_rounding` only once every position is settled
//! (a crash in the middle of the pass leaves the sweep waiting), exactly once; a re-run at a corrected fixing
//! reverses both sides through the same accounts and its own sweep leaves the clearing at 0; every transaction
//! balances, the ledger agrees with the account replay. No market data is involved (the fills and the fixing are
//! given), like tests/book.rs; the real-price version is tests/expiry_e2e.rs.
//!
//! Skipped with a message when PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use trading::book::reserve::Working;
use trading::book::types::*;
use trading::book::{BookKey, entry};
use trading::engine::Ids;
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, OptRight, OptionTerms, Side, Status};
use trading::money::D;
use trading::options::OptionsCtx;
use trading::options::settle::sweep_clearing;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::specs::Specs;

const SERIES: &str = "EURUSD-20991218-1.1600-C";
const KEY: &str = "EURUSD:2099-12-18";
const CLEARING: &str = "house:options_clearing.EURUSD.20991218:USD";

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn cut() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2099-12-18T15:00:00Z").unwrap().with_timezone(&Utc)
}

fn spec() -> SeriesSpec {
    SeriesSpec {
        terms: OptionTerms { series: SERIES.into(), underlying: "EURUSD".into(), right: OptRight::Call, strike: d("1.16"), expiry: cut().date_naive(), expiry_at: cut(), contract_size: d("10000"), quote_ccy: "USD".into(), barrier: None },
        tick: d("0.00001"),
        step: D::ONE,
    }
}

fn key() -> BookKey {
    BookKey::new(1, AccountKind::Demo, "EURUSD")
}

fn account(login: i64, user: i64, cent: bool) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: user,
        kind: AccountKind::Demo,
        group: if cent { "options-cent".into() } else { "options-standard".into() },
        mode: Mode::Hedging,
        cent,
        leverage: 100,
        status: Status::Active,
        name: "Settle IT".into(),
        route_override: None,
        controls: Controls::default(),
        demo: Some(DemoCfg { initial_balance: d("10000"), refills_per_day: 3, expiry_days: 10 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

async fn boot(pool: &sqlx::PgPool, states: std::collections::HashMap<i64, trading::state::AccountState>) -> Hub {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    let specs = Arc::new(Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap());
    let registry = Registry::default();
    for t in trading::persist::load_registry(pool).await.unwrap() {
        registry.put(t);
    }
    let (ticket, deal, txn, _, _) = trading::persist::max_ids(pool).await.unwrap();
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
        options: Arc::new(OptionsCtx::disabled(quotes)),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
    });
    Hub::start(shared, 2, states)
}

/// A working order with its reservation in the shard, then sent to the book (as tests/book.rs).
async fn place(hub: &Hub, login: i64, user: i64, side: Side, px: i64, qty: i64) -> Out {
    let id = hub.shared.ids.ticket();
    let rps = d("1.5");
    let order = Resting {
        id,
        login,
        stp: user,
        side,
        px,
        qty,
        left: qty,
        filled: 0,
        notional: 0,
        prio: 0,
        tif: Tif::Gtc,
        flags: 0,
        expire_ms: None,
        reserve_per_step: rps,
        ext: OrderExt { origin: "client".into(), source: "manual".into(), ccy: "USD".into(), created_ms: Utc::now().timestamp_millis(), kind: "limit".into(), opening: qty, ..Default::default() },
    };
    let w = Working {
        id,
        underlying: "EURUSD".into(),
        series: SERIES.into(),
        side,
        px,
        price: D::from(px) * spec().tick,
        tick: spec().tick,
        qty,
        left: qty,
        step: D::ONE,
        reserve_per_step: rps,
        hold: None,
        tif: Tif::Gtc,
        flags: 0,
        expire_ms: None,
        ext: order.ext.clone(),
        opening: qty,
        created: Utc::now(),
    };
    let op: Op = Box::new(move |tx, _| {
        tx.st.book.orders.insert(w.id, w);
        tx.book_dirty = true;
        Ok(Value::Null)
    });
    hub.exec(login, "client", None, "", "", None, op).await.unwrap();
    let cmd = Cmd::New { series: SERIES.into(), spec: spec(), order, usd_per_quote: D::ONE, at: Utc::now().timestamp_millis() };
    entry::call(hub, login, &key(), cmd).await.map(|(o, _, _)| o).unwrap()
}

async fn drained(pool: &sqlx::PgPool) {
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM book_outbox WHERE status <> 'applied'").fetch_one(pool).await.unwrap();
        if n == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the outbox did not drain");
}

async fn sum(pool: &sqlx::PgPool, code: &str) -> D {
    sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code = $1").bind(code).fetch_one(pool).await.unwrap().unwrap_or_default()
}

async fn balance(hub: &Hub, login: i64) -> D {
    hub.read(login, Box::new(|x| json!(x.unwrap().0.balance.to_string()))).await.as_str().unwrap().parse().unwrap()
}

async fn balanced(pool: &sqlx::PgPool) {
    let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(pool).await.unwrap();
    assert!(nets.iter().all(|(_, v)| v.is_zero()), "{nets:?}");
    let bad: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT txn_id FROM ledger_postings GROUP BY txn_id, currency HAVING sum(amount) <> 0) x").fetch_one(pool).await.unwrap();
    assert_eq!(bad, 0);
    let acct: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_accounts a WHERE a.balance <> (SELECT coalesce(sum(p.amount), 0) FROM ledger_postings p WHERE p.tenant_id = a.tenant_id AND p.account_code = a.code)").fetch_one(pool).await.unwrap();
    assert_eq!(acct, 0, "ledger_accounts = Σ postings");
}

fn settle_op(fixing: D, run: i32) -> Op {
    Box::new(move |tx, env| Ok(json!(trading::engine::options::settle(tx, env, KEY, fixing, run, None).tickets)))
}

#[tokio::test]
async fn book_settlement_goes_through_clearing_nets_across_usd_and_cent_and_sweeps_rounding_once() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_settle_clearing_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate");
    // an Options cent group for the cent accounts (CFD / Options account split; only `options-standard` is seeded)
    sqlx::query("INSERT INTO groups (tenant_id, code, name, mode, cent, leverages, default_leverage, margin_call_pct, stop_out_pct, spread_group, product) VALUES (1, 'options-cent', 'Options Cent', 'hedging', true, '{100}', 100, 60, 20, 'cent', 'options') ON CONFLICT DO NOTHING").execute(&pool).await.unwrap();
    let hub = boot(&pool, Default::default()).await;
    let a = (50_000_911i64, 911i64);
    let cents = [(50_000_912i64, 912i64), (50_000_913, 913), (50_000_914, 914)];
    hub.open(account(a.0, a.1, false), ("h".into(), "i".into()), "test").await.unwrap();
    for (l, u) in cents {
        hub.open(account(l, u, true), ("h".into(), "i".into()), "test").await.unwrap();
    }
    hub.shared.books.enable_venue(&pool, 1, AccountKind::Demo, "test", "settlement clearing test").await.unwrap();

    // A (USD) sells 3 at 0.00500; each cent account buys 1
    place(&hub, a.0, a.1, Side::Sell, 500, 3).await;
    for (l, u) in cents {
        let o = place(&hub, l, u, Side::Buy, 510, 1).await;
        assert_eq!((o.fills.len(), o.fills[0].px), (1, 500));
    }
    drained(&pool).await;
    assert_eq!(sum(&pool, CLEARING).await, D::ZERO, "the fills net to 0");
    for (l, _) in cents {
        assert_eq!(balance(&hub, l).await, d("1000000") - d("5000"), "50 USD premium = 5 000 USC");
    }

    // after the cut, at a fixing with 7 decimals: 0.0073333 × 10 000 = 73.333 USD per contract
    hub.shared.clock.set(Some(cut() + chrono::Duration::minutes(5)));
    let fixing = d("1.1673333");
    // the pass starts and "crashes" after the first cent account: the sweep waits for the rest
    let first = hub.exec(cents[0].0, "system", None, "", "", None, settle_op(fixing, 1)).await.unwrap();
    assert_eq!(first.value.as_array().unwrap().len(), 1);
    let w = sweep_clearing(&hub, "EURUSD", cut().date_naive(), 1).await;
    assert_eq!(w[0]["swept"], false, "{w:?}");
    assert_eq!(w[0]["waiting"]["accounts"], 3, "three accounts still hold the expiry: {w:?}");
    // the engine restarts here (no `Expire` reached the book: the cut-off pass never ran): the account replay, the
    // ledger check and the book recovery — the reconcile must not take the settled account for a mismatch
    drop(hub);
    let states = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &states).await.unwrap().is_empty());
    let hub = boot(&pool, states).await;
    hub.shared.clock.set(Some(cut() + chrono::Duration::minutes(6)));
    let rec = trading::book::recover(&hub).await.unwrap();
    assert!(rec.mismatches.is_empty(), "a partly settled expiry is not a reconcile mismatch: {:?}", rec.mismatches);
    // the catch-up settles the rest exactly once (the first account is not settled again)
    let before: Vec<D> = futures_util::future::join_all([a.0, cents[0].0, cents[1].0, cents[2].0].map(|l| balance(&hub, l))).await;
    let rep = hub.settle(KEY, fixing, 1, None).await;
    assert_eq!((rep.accounts.iter().map(|x| x.2).sum::<usize>(), rep.failures), (3, 0));
    let after: Vec<D> = futures_util::future::join_all([a.0, cents[0].0, cents[1].0, cents[2].0].map(|l| balance(&hub, l))).await;
    assert_eq!(after[0] - before[0], d("-220.00"), "the USD short pays r2(219.999)");
    assert_eq!(after[1] - before[1], D::ZERO, "settled before the crash, not again");
    assert_eq!(after[2] - before[2], d("7333.00"), "a cent long gets 100 × r2(73.333)");
    assert_eq!(after[3] - before[3], d("7333.00"));
    // nothing went to the house B-book account; the clearing keeps the rounding: 220.00 − 3 × 73.33
    assert_eq!((sum(&pool, "house:options_settlement:USD").await, sum(&pool, "house:options_settlement:USC").await), (D::ZERO, D::ZERO));
    assert_eq!(sum(&pool, CLEARING).await, d("0.01"));
    let legs: Vec<(String, String, D)> = sqlx::query_as(
        "SELECT p.account_code, p.currency, p.amount FROM ledger_postings p JOIN ledger_txns t ON t.id = p.txn_id WHERE t.idempotency_key LIKE 'settle:%' AND t.login = $1 ORDER BY p.id",
    )
    .bind(cents[1].0)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        legs,
        vec![
            (format!("acct:{}:balance", cents[1].0), "USC".into(), d("7333.00")),
            ("house:fx:USC".into(), "USC".into(), d("-7333.00")),
            ("house:fx:USD".into(), "USD".into(), d("73.33")),
            (CLEARING.into(), "USD".into(), d("-73.33")),
        ],
        "the cent settlement's 4-leg form"
    );
    // the engine "crashes" between the settlement and its sweep: the scheduler's catch-up (`sweep_recent`, every
    // minute over the runs of the last 2 h) sweeps 0.01 ≤ 0.005 × 4 settled positions to house:options_rounding
    sqlx::query("INSERT INTO option_settlement_runs (tenant_id, expiry_key, symbol, expiry_date, run, fixing, status, kind, positions, accounts, payout_usd, failures, reason, created_by, started_at, finished_at) VALUES (1, $1, 'EURUSD', $2, 1, $3, 'done', 'settle', 4, 4, 0, 0, '', 'system', now(), now())")
        .bind(KEY)
        .bind(cut().date_naive())
        .bind(fixing)
        .execute(&pool)
        .await
        .unwrap();
    trading::options::settle::sweep_recent(&hub, &mut Default::default()).await;
    assert_eq!(sum(&pool, CLEARING).await, D::ZERO, "the clearing nets to 0 after the sweep");
    let w = sweep_clearing(&hub, "EURUSD", cut().date_naive(), 1).await;
    assert_eq!((w.len(), w[0]["swept"].as_bool(), w[0]["kind"].as_str(), w[0]["settled"].as_i64()), (1, Some(false), Some("demo"), Some(4)), "already swept: {w:?}");
    assert_eq!(sum(&pool, "house:options_rounding:USD").await, d("0.01"));
    let sweeps: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key = 'clrsweep:1:demo:EURUSD:2099-12-18:1'").fetch_one(&pool).await.unwrap();
    assert_eq!(sweeps, 1);
    balanced(&pool).await;

    // a re-run at a corrected fixing 1.1673337 (run 2): 73.337 per contract → short −220.01, longs +73.34 each
    let deals: Vec<(i64, sqlx::types::Json<trading::model::Deal>)> = sqlx::query_as("SELECT login, data FROM deals WHERE reason = 'expiry' ORDER BY id").fetch_all(&pool).await.unwrap();
    let fixing2 = d("1.1673337");
    for (l, _) in [a, cents[0], cents[1], cents[2]] {
        let ds: Vec<trading::model::Deal> = deals.iter().filter(|(x, _)| *x == l).map(|(_, j)| j.0.clone()).collect();
        let b0 = balance(&hub, l).await;
        let op: Op = Box::new(move |tx, env| trading::engine::options::rerun(tx, env, KEY, &ds, fixing2, 2, None).map(|s| json!(s.tickets)));
        hub.exec(l, "system", None, "", "", None, op).await.unwrap();
        let diff = balance(&hub, l).await - b0;
        assert_eq!(diff, if l == a.0 { d("-0.01") } else { d("1.00") }, "login {l}: by exactly the difference");
    }
    // reversals mirror the settlements: the clearing holds the run-1 sweep's −0.01 back and run 2's rounding
    // (220.01 − 3 × 73.34 = −0.01)
    assert_eq!(sum(&pool, CLEARING).await, d("-0.02"));
    let rev4: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_postings p JOIN ledger_txns t ON t.id = p.txn_id WHERE t.idempotency_key LIKE 'settle-rev:%' AND t.login = $1").bind(cents[2].0).fetch_one(&pool).await.unwrap();
    assert_eq!(rev4, 4, "a cent reversal is 4 legs too");
    let w = sweep_clearing(&hub, "EURUSD", cut().date_naive(), 2).await;
    assert_eq!(w[0]["swept"], true, "{w:?}");
    assert_eq!((sum(&pool, CLEARING).await, sum(&pool, "house:options_rounding:USD").await), (D::ZERO, d("-0.01")), "the rounding account ends with run 2's rounding");
    balanced(&pool).await;

    // the ledger agrees with the account replay; the replayed states are the live ones
    let states = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &states).await.unwrap().is_empty());
    for (l, _) in [a, cents[0], cents[1], cents[2]] {
        let live = hub.read(l, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await;
        assert_eq!(serde_json::to_value(&states[&l]).unwrap(), live, "replay of {l}");
    }
    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

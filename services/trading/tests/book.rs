//! Options order book through the real book actor, journal, outbox dispatcher, account shards and PostgreSQL
//! (docs/OPTIONS-EXCHANGE.md §13 "ledger and reserve properties on the Postgres harness" and the crash kill
//! points). No market data is involved: orders are placed with their reservation directly in the shard (the
//! entry gates, which need live prices, are covered by `tests/book_e2e.rs` against the real services).
//!
//! * a trade between two clients: premium through the expiry's clearing account (nets to 0), positions on the
//!   book venue, book projections (orders, fills, positions, outbox all applied), reserve 0 when idle;
//! * the journal replays to byte-identical outputs and the actor's state;
//! * kill point 1: the actor commits a batch and dies before dispatching — after a restart (account replay +
//!   `book::recover`) each side is booked exactly once and the reconcile is clean;
//! * kill point 2: the dispatcher dies after applying one side, before marking it — after a restart nothing is
//!   booked twice;
//! * a corrupted book position is caught by the reconcile: that underlying goes cancel-only;
//! * every ledger transaction balances, `verify_balances` and the account replay agree with the live state.
//!
//! Skipped with a message when PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use trading::book::reserve::Working;
use trading::book::types::*;
use trading::book::{BookKey, entry, journal};
use trading::engine::Ids;
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, OptRight, OptionTerms, Side, Status};
use trading::money::D;
use trading::options::OptionsCtx;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::specs::Specs;

const SERIES: &str = "EURUSD-20991218-1.1600-C";
const CLEARING: &str = "house:options_clearing.EURUSD.20991218:USD";

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn spec() -> SeriesSpec {
    let at: DateTime<Utc> = DateTime::parse_from_rfc3339("2099-12-18T15:00:00Z").unwrap().with_timezone(&Utc);
    SeriesSpec {
        terms: OptionTerms { series: SERIES.into(), underlying: "EURUSD".into(), right: OptRight::Call, strike: d("1.16"), expiry: at.date_naive(), expiry_at: at, contract_size: d("10000"), quote_ccy: "USD".into(), barrier: None },
        tick: d("0.00001"),
        step: D::ONE,
    }
}

fn key() -> BookKey {
    BookKey::new(1, AccountKind::Demo, "EURUSD")
}

fn account(login: i64, user: i64) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: user,
        kind: AccountKind::Demo,
        group: "options-standard".into(),
        mode: Mode::Hedging,
        cent: false,
        leverage: 100,
        status: Status::Active,
        name: "Book IT".into(),
        route_override: None,
        controls: Controls::default(),
        demo: Some(DemoCfg { initial_balance: d("10000"), refills_per_day: 3, expiry_days: 10 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

/// A fresh "process" on the database: account replay, shards, an empty book registry.
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
        // no options service and no market data: fills, margin and valuation run on the stored terms
        options: Arc::new(OptionsCtx::disabled(quotes)),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
    });
    Hub::start(shared, 2, states)
}

/// Places a working order with its reservation in the shard (the gates are the e2e test's) and sends it.
async fn place(hub: &Hub, login: i64, user: i64, side: Side, px: i64, qty: i64, tif: Tif) -> Result<(i64, Out), String> {
    let id = hub.shared.ids.ticket();
    let rps = d("1.5"); // any reservation: what matters is that it is released exactly
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
        tif,
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
        tif,
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
    hub.exec(login, "client", None, "", "", None, op).await.map_err(|e| format!("{e:?}"))?;
    let cmd = Cmd::New { series: SERIES.into(), spec: spec(), order, usd_per_quote: D::ONE, at: Utc::now().timestamp_millis() };
    entry::call(hub, login, &key(), cmd).await.map(|(o, _, _)| (id, o)).map_err(|e| format!("{e:?}"))
}

async fn read(hub: &Hub, login: i64) -> Value {
    hub.read(
        login,
        Box::new(|x| match x {
            Some((a, _)) => json!({
                "balance": a.balance.to_string(),
                "reserve": a.book.reserve().to_string(),
                "working": a.book.orders.len(),
                "book": a.positions.values().filter(|p| p.on_book()).map(|p| (p.volume * p.side.sign()).to_string()).collect::<Vec<_>>(),
            }),
            None => Value::Null,
        }),
    )
    .await
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

/// Every fill ledger key exists exactly once per (fill, login).
async fn fill_keys_once(pool: &sqlx::PgPool) -> i64 {
    let dup: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT idempotency_key FROM ledger_txns WHERE idempotency_key LIKE 'fill:%' GROUP BY 1 HAVING count(*) > 1) x").fetch_one(pool).await.unwrap();
    assert_eq!(dup, 0);
    let fills: i64 = sqlx::query_scalar("SELECT count(*) FROM book_fills").fetch_one(pool).await.unwrap();
    let keys: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key LIKE 'fill:%:prem'").fetch_one(pool).await.unwrap();
    assert_eq!(keys, fills * 2, "two premium postings (buyer, seller) per fill");
    keys
}

async fn clearing(pool: &sqlx::PgPool) -> D {
    sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code = $1").bind(CLEARING).fetch_one(pool).await.unwrap().unwrap_or_default()
}

async fn restart(pool: &sqlx::PgPool) -> (Hub, trading::book::Recovery) {
    let states = trading::persist::replay_all(pool).await.unwrap();
    assert!(trading::persist::verify_balances(pool, &states).await.unwrap().is_empty(), "ledger vs replay");
    let hub = boot(pool, states).await;
    let rep = trading::book::recover(&hub).await.unwrap();
    (hub, rep)
}

#[tokio::test]
async fn order_book_through_the_actor_outbox_shards_and_postgres() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_book_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate (incl. the order book migration)");

    let hub = boot(&pool, Default::default()).await;
    let (a, b, ua, ub) = (50_000_901i64, 50_000_902i64, 901i64, 902i64);
    hub.open(account(a, ua), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(b, ub), ("h".into(), "i".into()), "test").await.unwrap();
    // dormant until the venue row exists
    assert!(!hub.shared.books.venue_enabled(1, AccountKind::Demo));
    hub.shared.books.enable_venue(&pool, 1, AccountKind::Demo, "test", "integration test").await.unwrap();
    assert!(hub.shared.books.venue_enabled(1, AccountKind::Demo));

    // ---------- a trade: A rests 2 @ 0.00500, B buys 3 @ 0.00510 → 2 trade at 0.00500, 1 rests ----------
    let (ida, o) = place(&hub, a, ua, Side::Sell, 500, 2, Tif::Gtc).await.unwrap();
    assert_eq!(o.rested, vec![ida]);
    assert_eq!(read(&hub, a).await["reserve"], "3.0", "2 steps × 1.5 reserved");
    let (idb, o) = place(&hub, b, ub, Side::Buy, 510, 3, Tif::Gtc).await.unwrap();
    assert_eq!((o.fills.len(), o.fills[0].px, o.fills[0].qty, o.fills[0].premium_usd), (1, 500, 2, d("100.00")));
    drained(&pool).await;
    let (sa, sb) = (read(&hub, a).await, read(&hub, b).await);
    assert_eq!((sa["balance"].as_str(), sa["reserve"].as_str(), sa["book"].clone()), (Some("10100.00"), Some("0"), json!(["-2"])));
    assert_eq!((sb["balance"].as_str(), sb["reserve"].as_str(), sb["book"].clone()), (Some("9900.00"), Some("1.5"), json!(["2"])));
    assert_eq!(clearing(&pool).await, D::ZERO, "clearing nets to 0 per fill");
    let st: Vec<(i64, String, i64)> = sqlx::query_as("SELECT id, status, left_qty FROM book_orders ORDER BY id").fetch_all(&pool).await.unwrap();
    assert_eq!(st, vec![(ida, "filled".into(), 0), (idb, "open".into(), 1)]);
    let pos: Vec<(i64, i64)> = sqlx::query_as("SELECT login, steps FROM book_positions ORDER BY login").fetch_all(&pool).await.unwrap();
    assert_eq!(pos, vec![(a, -2), (b, 2)]);
    // B cancels the rest: reserve 0 everywhere once idle
    let (out, _, _) = entry::call(&hub, b, &key(), Cmd::Cancel { series: SERIES.into(), id: idb, login: b, reason: "cancelled".into(), at: Utc::now().timestamp_millis() }).await.unwrap();
    assert!(out.ok);
    drained(&pool).await;
    for l in [a, b] {
        let s = read(&hub, l).await;
        assert_eq!((s["reserve"].as_str(), s["working"].as_u64()), (Some("0"), Some(0)), "reserve = 0 when no order works");
    }
    // the journal replays to identical outputs and the actor's state
    let entries = journal::entries(&pool, &key(), 1).await.unwrap();
    assert_eq!(entries.len(), 3);
    let replayed = journal::replay(UnderlyingBooks::new(key()), &entries).expect("replay identical");
    let live = hub.shared.books.handle(&key()).unwrap().read(Box::new(|b| journal::fingerprint(b))).await.unwrap();
    assert_eq!(journal::fingerprint(&replayed), live);

    // ---------- kill point 1: the batch commits, the actor dies before dispatching / replying ----------
    let (_, o) = place(&hub, a, ua, Side::Sell, 500, 1, Tif::Gtc).await.unwrap();
    assert_eq!(o.rested.len(), 1);
    hub.shared.books.hooks.crash_after_journal.store(true, Ordering::SeqCst);
    let e = place(&hub, b, ub, Side::Buy, 500, 1, Tif::Ioc).await.unwrap_err();
    assert!(e.contains("stopped") || e.contains("Book"), "{e}");
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM book_outbox WHERE status = 'pending'").fetch_one(&pool).await.unwrap();
    assert!(pending >= 2, "the fill's two items are committed but not applied ({pending})");
    drop(hub);
    let (hub, rep) = restart(&pool).await;
    assert!(rep.mismatches.is_empty(), "{:?}", rep.mismatches);
    assert!(rep.pending >= 2);
    drained(&pool).await;
    assert_eq!(fill_keys_once(&pool).await, 4);
    assert_eq!(read(&hub, a).await["book"], json!(["-3"]));
    assert_eq!(read(&hub, b).await["book"], json!(["3"]));
    assert_eq!(clearing(&pool).await, D::ZERO);

    // ---------- kill point 2: one side applied, then the dispatcher dies before marking it ----------
    let (_, o) = place(&hub, a, ua, Side::Sell, 505, 2, Tif::Gtc).await.unwrap();
    assert_eq!(o.rested.len(), 1);
    hub.shared.books.hooks.crash_after_apply.store(1, Ordering::SeqCst);
    let _ = place(&hub, b, ub, Side::Buy, 505, 2, Tif::Ioc).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(hub);
    let (hub, rep) = restart(&pool).await;
    assert!(rep.mismatches.is_empty(), "{:?}", rep.mismatches);
    drained(&pool).await;
    assert_eq!(fill_keys_once(&pool).await, 6, "exactly once after the restart");
    assert_eq!(read(&hub, a).await["book"], json!(["-5"]));
    assert_eq!(read(&hub, b).await["book"], json!(["5"]));
    assert_eq!(clearing(&pool).await, D::ZERO);
    let (ba, bb) = (read(&hub, a).await, read(&hub, b).await);
    // A received 100 + 50 + 101, B paid the same
    assert_eq!((ba["balance"].as_str(), bb["balance"].as_str()), (Some("10251.00"), Some("9749.00")));
    // a resting order survives a restart with its reservation
    let (idr, _) = place(&hub, a, ua, Side::Sell, 600, 4, Tif::Gtc).await.unwrap();
    drop(hub);
    let (hub, rep) = restart(&pool).await;
    assert!(rep.mismatches.is_empty() && rep.orders == 1, "{rep:?}");
    assert_eq!(read(&hub, a).await["reserve"], "6.0", "4 steps × 1.5 rebuilt from book_orders");
    let (out, _, _) = entry::call(&hub, a, &key(), Cmd::Cancel { series: SERIES.into(), id: idr, login: a, reason: "cancelled".into(), at: Utc::now().timestamp_millis() }).await.unwrap();
    assert!(out.ok);
    drained(&pool).await;
    assert_eq!(read(&hub, a).await["reserve"], "0");

    // ---------- the replay audit over everything, restarts included ----------
    let entries = journal::entries(&pool, &key(), 1).await.unwrap();
    let replayed = journal::replay(UnderlyingBooks::new(key()), &entries).expect("replay identical across restarts");
    let live = hub.shared.books.handle(&key()).unwrap().read(Box::new(|b| journal::fingerprint(b))).await.unwrap();
    assert_eq!(journal::fingerprint(&replayed), live);
    let sums = journal::position_sums(&replayed);
    assert_eq!(sums[SERIES], (0, 5), "Σ positions = 0, open interest 5");
    let audit = trading::book::replay_audit(&hub).await;
    assert!(audit.len() == 1 && audit[0].1.is_ok(), "replay audit OK: {audit:?}");

    // ---------- a corrupted book position: reconcile puts the underlying in cancel-only ----------
    sqlx::query("UPDATE book_positions SET steps = steps + 1 WHERE login = $1").bind(a).execute(&pool).await.unwrap();
    drop(hub);
    let (hub, rep) = restart(&pool).await;
    assert_eq!(rep.mismatches.len(), 1, "{rep:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let e = place(&hub, a, ua, Side::Sell, 500, 1, Tif::Gtc).await.unwrap();
    assert_eq!(e.1.code.as_deref(), Some("series_cancel_only"));
    drained(&pool).await;
    assert_eq!(read(&hub, a).await["reserve"], "0", "the refused order released its reservation");
    let halts: i64 = sqlx::query_scalar("SELECT count(*) FROM book_halts WHERE mode = 'cancel_only'").fetch_one(&pool).await.unwrap();
    assert_eq!(halts, 1);
    sqlx::query("UPDATE book_positions SET steps = steps - 1 WHERE login = $1").bind(a).execute(&pool).await.unwrap();

    // ---------- money: every transaction balances, the ledger nets per currency, replay = live ----------
    let unbalanced: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT txn_id FROM ledger_postings GROUP BY txn_id, currency HAVING sum(amount) <> 0) x").fetch_one(&pool).await.unwrap();
    assert_eq!(unbalanced, 0);
    let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(&pool).await.unwrap();
    assert!(nets.iter().all(|(_, v)| v.is_zero()), "{nets:?}");
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for l in [a, b] {
        let live = hub.read(l, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await;
        assert_eq!(serde_json::to_value(&replayed[&l]).unwrap(), live, "replay diverged for {l}");
    }

    drop(hub);
    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

const SERIES2: &str = "EURUSD-20991218-1.1700-C";

fn spec2() -> SeriesSpec {
    let mut s = spec();
    s.terms.series = SERIES2.into();
    s.terms.strike = d("1.17");
    s
}

/// A leg order of an accepted combo, held (reserved) on the account like `enter` would.
async fn hold_leg(hub: &Hub, login: i64, user: i64, series: &str, side: Side, qty: i64) -> Resting {
    let id = hub.shared.ids.ticket();
    let rps = d("2");
    let order = Resting {
        id,
        login,
        stp: user,
        side,
        px: 0,
        qty,
        left: qty,
        filled: 0,
        notional: 0,
        prio: 0,
        tif: Tif::Ioc,
        flags: 0,
        expire_ms: None,
        reserve_per_step: rps,
        ext: OrderExt { origin: "rfq".into(), source: "manual".into(), ccy: "USD".into(), created_ms: Utc::now().timestamp_millis(), kind: "rfq".into(), opening: qty, fee_taker: d("0.25"), fee_cap_pct: d("10"), ..Default::default() },
    };
    let w = Working {
        id,
        underlying: "EURUSD".into(),
        series: series.into(),
        side,
        px: 0,
        price: D::ZERO,
        tick: spec().tick,
        qty,
        left: qty,
        step: D::ONE,
        reserve_per_step: rps,
        hold: None,
        tif: Tif::Ioc,
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
    order
}

/// Combo RFQ and bust through the real actor, journal, outbox and PostgreSQL (no market data): every leg of a
/// combo in ONE outbox item per account, booked exactly once across a crash between applying and marking it
/// (kill point 2); a bust reversed on both sides exactly once across the same crash; ledger keys once, clearing 0,
/// positions = book, reserves 0, journal replay and account replay identical.
#[tokio::test]
async fn combo_rfq_and_bust_book_exactly_once_across_crashes() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_book_rfq_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate");
    let hub = boot(&pool, Default::default()).await;
    let (t, m, ut, um) = (50_000_911i64, 50_000_912i64, 911i64, 912i64);
    hub.open(account(t, ut), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(m, um), ("h".into(), "i".into()), "test").await.unwrap();
    hub.shared.books.enable_venue(&pool, 1, AccountKind::Demo, "test", "rfq test").await.unwrap();
    let at = Utc::now().timestamp_millis();
    // the responder's firm quote on a call spread (buy 1.16 C, sell 1.17 C), 3 units
    let legs = vec![RfqLeg { series: SERIES.into(), spec: spec(), side: Side::Buy, ratio: 1 }, RfqLeg { series: SERIES2.into(), spec: spec2(), side: Side::Sell, ratio: 1 }];
    let quote = RfqQuoteIn { rfq: 7001, quote: 7002, requester: t, requester_stp: ut, login: m, stp: um, legs, qty: 3, reduce_only: false, bid: Some(190), ask: Some(210), theos: vec![520, 320], valid_until: at + 60_000, usd_per_quote: D::ONE };
    let (o, _, _) = entry::call(&hub, m, &key(), Cmd::RfqQuote { quote, at }).await.unwrap();
    assert!(o.ok, "{o:?}");
    // the requester's leg orders (reserved), then the accept — the dispatcher dies after applying one item
    let l1 = hold_leg(&hub, t, ut, SERIES, Side::Buy, 3).await;
    let l2 = hold_leg(&hub, t, ut, SERIES2, Side::Sell, 3).await;
    assert_eq!(read(&hub, t).await["reserve"], "12", "3 × 2 per leg, two series");
    hub.shared.books.hooks.crash_after_apply.store(1, Ordering::SeqCst);
    let (o, _, _) = entry::call(&hub, t, &key(), Cmd::RfqAccept { rfq: 7001, quote: 7002, login: t, stp: ut, side: Side::Buy, limit_net: 210, orders: vec![l1, l2], at: at + 10 }).await.unwrap();
    assert!(o.ok && o.fills.len() == 2, "{o:?}");
    assert_eq!(o.fills[0].px - o.fills[1].px, 210, "the legs sum to the accepted net");
    let items: Vec<(String,)> = sqlx::query_as("SELECT item_kind FROM book_outbox ORDER BY seq").fetch_all(&pool).await.unwrap();
    assert_eq!(items.iter().filter(|x| x.0 == "fills").count(), 2, "one item per account holds both legs");
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(hub);
    let (hub, rep) = restart(&pool).await;
    assert!(rep.mismatches.is_empty(), "{rep:?}");
    drained(&pool).await;
    for f in &o.fills {
        for l in [t, m] {
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key = $1").bind(format!("fill:{}:{l}:prem", f.id)).fetch_one(&pool).await.unwrap();
            assert_eq!(n, 1, "{} booked once on {l}", f.id);
        }
    }
    let pos = |v: Value| -> Vec<String> {
        let mut x: Vec<String> = v["book"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
        x.sort();
        x
    };
    assert_eq!(pos(read(&hub, t).await), vec!["-3".to_string(), "3".to_string()]);
    assert_eq!(pos(read(&hub, m).await), vec!["-3".to_string(), "3".to_string()]);
    assert_eq!(read(&hub, t).await["reserve"], "0", "the leg reservations are released");
    let clearing2: D = sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code LIKE 'house:options_clearing.%'").fetch_one(&pool).await.unwrap().unwrap_or_default();
    assert_eq!(clearing2, D::ZERO);
    let fee: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE login = $1 AND idempotency_key LIKE 'fill:%:fee'").bind(t).fetch_one(&pool).await.unwrap();
    assert_eq!(fee, 2, "the taker fee on each leg");
    let balance_t = read(&hub, t).await["balance"].as_str().unwrap().to_string();

    // ---------- bust the first leg; the dispatcher dies after one side ----------
    let fill = o.fills[0].clone();
    hub.shared.books.hooks.crash_after_apply.store(1, Ordering::SeqCst);
    let (b, _, _) = entry::call(&hub, t, &key(), Cmd::Bust { fill: fill.clone(), reason: "test".into(), at: at + 20 }).await.unwrap();
    assert!(b.ok && b.busted.len() == 1);
    tokio::time::sleep(Duration::from_millis(300)).await;
    drop(hub);
    let (hub, rep) = restart(&pool).await;
    assert!(rep.mismatches.is_empty(), "{rep:?}");
    drained(&pool).await;
    for l in [t, m] {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key = $1").bind(format!("bust:{}:{l}:prem", fill.id)).fetch_one(&pool).await.unwrap();
        assert_eq!(n, 1, "bust booked once on {l}");
    }
    let refund: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key = $1").bind(format!("bust:{}:{t}:fee", fill.id)).fetch_one(&pool).await.unwrap();
    assert_eq!(refund, 1, "the taker fee of the busted leg is refunded");
    assert_eq!(read(&hub, t).await["book"], json!(["-3"]), "the bought leg is gone, the sold one stays");
    assert_eq!(read(&hub, m).await["book"], json!(["3"]));
    assert_ne!(read(&hub, t).await["balance"].as_str().unwrap(), balance_t, "premium and fee came back");
    let clearing2: D = sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code LIKE 'house:options_clearing.%'").fetch_one(&pool).await.unwrap().unwrap_or_default();
    assert_eq!(clearing2, D::ZERO);
    // replay: the journal (quote, accept, bust, restarts) and the accounts
    let entries = journal::entries(&pool, &key(), 1).await.unwrap();
    let replayed = journal::replay(UnderlyingBooks::new(key()), &entries).expect("replay identical");
    let live = hub.shared.books.handle(&key()).unwrap().read(Box::new(|b| journal::fingerprint(b))).await.unwrap();
    assert_eq!(journal::fingerprint(&replayed), live);
    let unbalanced: i64 = sqlx::query_scalar("SELECT count(*) FROM (SELECT txn_id FROM ledger_postings GROUP BY txn_id, currency HAVING sum(amount) <> 0) x").fetch_one(&pool).await.unwrap();
    assert_eq!(unbalanced, 0);
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for l in [t, m] {
        let live = hub.read(l, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await;
        assert_eq!(serde_json::to_value(&replayed[&l]).unwrap(), live, "replay diverged for {l}");
    }
    drop(hub);
    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

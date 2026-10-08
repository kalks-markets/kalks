//! Options order book END TO END against the REAL local services (docs/OPTIONS-EXCHANGE.md §13; no mocked
//! market data): market-data (:8081, live raw and group quotes over its WebSocket), the options service (:8104,
//! the real snapshot: underlyings, surfaces, expiries, series) and PostgreSQL (:5433, a throw-away database).
//!
//! Two demo accounts of tenant `kalks` trade through the terminal API handlers (SSO sessions, the same gates,
//! reservations, book actor, outbox and shards as production): a resting sell below the model mid clamps the
//! mark, a market buy fills against it, the tape / snapshot / internal WebSocket show the trade, open interest =
//! the positions, the buyer closes through the generic close route (a reduce-only market IOC on the book), the
//! clearing account nets to 0, reserves are 0 once idle, the journal replays identically and the ledger balances.
//! With `BOOK_E2E_FIXING_WAIT_SECS` > 0 and a 0DTE expiry still open for trading, it also trades the 0DTE
//! series, waits for the real fixing and settles it (clearing and settlement net to 0).
//!
//! Run (from the repo root; FX must be open):
//! ```text
//! cargo run -p market-data            # :8081, reads INFOWAY_* from .env.local
//! (cd services/options && cargo run -p options)   # :8104
//! cargo test -p trading --test book_e2e -- --ignored --nocapture
//! ```
//! Env: OPTIONS_URL (http://127.0.0.1:8104), OPTIONS_INTERNAL_TOKEN, MARKET_DATA_WS_URL
//! (ws://127.0.0.1:8081/v1/stream), TRADING_TEST_DATABASE_URL (postgres://postgres@127.0.0.1:5433/postgres),
//! BOOK_E2E_FIXING_WAIT_SECS (0).

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use chrono::Utc;
use futures_util::StreamExt;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use trading::api::{self, AppState, Body, Ctx, LoginAlloc};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::Ids;
use trading::feed::{self, QuoteBook};
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, Status};
use trading::money::D;
use trading::options::{OptionPricing, OptionsCtx, Suitability};
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Shared, Stats, Streams};
use trading::specs::Specs;

fn env(k: &str, d: &str) -> String {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| d.to_string())
}

fn dec(v: &Value) -> D {
    v.as_f64().and_then(trading::money::from_f64).or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or_default()
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
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
        name: "Book E2E".into(),
        route_override: None,
        controls: Controls::default(),
        demo: Some(DemoCfg { initial_balance: D::from(10_000), refills_per_day: 3, expiry_days: 10 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

async fn token(st: &AppState, tenant: &Arc<trading::rules::TenantConfig>, login: i64, user: i64) -> String {
    let ctx = || Ctx { tenant: tenant.clone(), ip: "198.51.100.9".into(), user_agent: "e2e".into(), bearer: None };
    let mut h = HeaderMap::new();
    h.insert("x-kalks-user-id", user.to_string().parse().unwrap());
    let Json(sso) = api::accounts::sso(State(st.clone()), ctx(), h, Path(login), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    let Json(s) = api::terminal::sso(State(st.clone()), ctx(), body(json!({"token": sso["token"]}))).await.unwrap();
    s["token"].as_str().unwrap().to_string()
}

async fn drained(pool: &sqlx::PgPool) {
    for _ in 0..200 {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM book_outbox WHERE status <> 'applied'").fetch_one(pool).await.unwrap();
        if n == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("outbox not drained");
}

async fn ledger_sum(pool: &sqlx::PgPool, code: &str) -> D {
    sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM ledger_postings WHERE account_code = $1").bind(code).fetch_one(pool).await.unwrap().unwrap_or_default()
}

/// (series code, cut) of the nearest-ATM call of the first EURUSD expiry whose cut is at least `min_minutes` away
/// (and less than `max_hours`).
fn pick(snap: &trading::options::OptSnapshot, spot: f64, min_minutes: i64, max_hours: Option<i64>) -> Option<(String, chrono::DateTime<Utc>)> {
    let now = Utc::now();
    let mut ex: Vec<_> = snap.expiries.iter().filter(|e| e.symbol == "EURUSD" && e.status == "listed" && e.cut_at > now + chrono::Duration::minutes(min_minutes) && max_hours.is_none_or(|h| e.cut_at < now + chrono::Duration::hours(h))).collect();
    ex.sort_by_key(|e| e.cut_at);
    let e = ex.first()?;
    let s = snap.series.values().filter(|s| s.expiry_id == e.id && s.kind == "call" && s.status == "active").min_by(|a, b| (a.strike - spot).abs().total_cmp(&(b.strike - spot).abs()))?;
    Some((s.code.clone(), e.cut_at))
}

#[tokio::test]
#[ignore = "needs the real market-data (:8081), options service (:8104) and PostgreSQL (:5433)"]
async fn order_book_end_to_end_against_the_real_services() {
    let _ = dotenvy::from_path(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.env.local"));
    let opt_url = env("OPTIONS_URL", "http://127.0.0.1:8104");
    let md_ws = env("MARKET_DATA_WS_URL", "ws://127.0.0.1:8081/v1/stream");
    let base = env("TRADING_TEST_DATABASE_URL", "postgres://postgres@127.0.0.1:5433/postgres");
    let db = format!("kalks_trading_book_e2e_{}", std::process::id());
    let server = PgConnectOptions::from_str(&base).unwrap();
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("PostgreSQL :5433");

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    let specs = Arc::new(Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap());
    let registry = Registry::default();
    for t in trading::persist::load_registry(&pool).await.unwrap() {
        registry.put(t);
    }
    let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&pool).await.unwrap();
    let quotes = Arc::new(QuoteBook::default());
    // the REAL options service snapshot
    let options = Arc::new(OptionsCtx::new(&opt_url, &env("OPTIONS_INTERNAL_TOKEN", ""), quotes.clone()));
    assert!(options.poll(Some(&pool)).await.expect("options service :8104"), "snapshot loaded");
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
        max_quote_age_ms: 300_000,
        restrictions: Default::default(),
        options: options.clone(),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
    });
    let hub = Hub::start(shared, 2, Default::default());
    // the REAL market-data feed: group quotes and raw mids
    feed::spawn(hub.clone(), md_ws.clone(), specs.symbols());
    feed::spawn_raw(hub.clone(), md_ws.clone(), specs.symbols());
    let mut spot = None;
    for _ in 0..300 {
        if let Some((m, _)) = options.spot("EURUSD") {
            spot = Some(m);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let spot = spot.expect("a raw EURUSD mid from market-data :8081 (is FX open?)");
    eprintln!("EURUSD raw mid {spot}");
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
        gateway: Arc::new(trading::controls::Gateway::new(&cfg.gateway_url, &cfg.gateway_token)),
    };
    let tenant = registry.by_slug("kalks").unwrap();
    let ctx = |b: &str| Ctx { tenant: tenant.clone(), ip: "198.51.100.9".into(), user_agent: "e2e".into(), bearer: Some(b.to_string()) };
    let (a, b, ua, ub) = (50_009_301i64, 50_009_302i64, 9301i64, 9302i64);
    hub.open(account(a, ua), ("h".into(), "i".into()), "e2e").await.unwrap();
    hub.open(account(b, ub), ("h".into(), "i".into()), "e2e").await.unwrap();
    // both clients accepted the options intro (the gateway's answer; no gateway runs here)
    for u in [ua, ub] {
        options.set_suitability(u, Suitability { eligible: true, kyc_verified: true, disclosure_accepted: true, quiz_passed: true, source: "gateway" }, 3_600_000);
    }
    let (ta, tb) = (token(&st, &tenant, a, ua).await, token(&st, &tenant, b, ub).await);

    // dormant until enabled: the book refuses, the house-priced options are untouched
    let order = |series: &str, side: &str, kind: &str, qty: i64, price: Option<D>, cid: &str| json!({"series": series, "side": side, "type": kind, "qty": qty, "price": price.map(|p| p.to_string()), "tif": if kind == "market" { "ioc" } else { "gtc" }, "clientOrderId": cid});
    let snap = options.snapshot().unwrap();
    let (series, cut) = pick(&snap, spot, 24 * 60, None).expect("a EURUSD expiry more than a day out");
    eprintln!("series {series} (cut {cut})");
    let e = api::options_book::place(State(st.clone()), ctx(&ta), body(order(&series, "sell", "limit", 2, Some(D::new(1, 2)), "x0"))).await.unwrap_err();
    assert!(format!("{e:?}").contains("book_disabled"), "{e:?}");
    hub.shared.books.enable_venue(&pool, 1, AccountKind::Demo, "e2e", "book e2e").await.unwrap();
    let rep = trading::book::recover(&hub).await.unwrap();
    assert!(rep.mismatches.is_empty());
    // once the book is live, listed options no longer open at the house price
    let house = json!({"legs": [{"series": series, "side": "buy", "contracts": 1}], "type": "market", "clientOrderId": "h1"});
    let e = api::options::place(State(st.clone()), ctx(&tb), body(house)).await.unwrap_err();
    assert!(format!("{e:?}").contains("book_venue"), "{e:?}");

    // the model mark from the real snapshot and the live spot
    let s2 = series.clone();
    let mv = hub
        .read(
            a,
            Box::new(move |x| {
                let (acc, env) = x.unwrap();
                let (terms, _) = trading::engine::options_book::terms_of(&env.options.snapshot().unwrap(), &s2).unwrap();
                let q = env.options.mark(&env.tenant.slug, &env.group.code, &terms, env.now).unwrap();
                json!({"mark": q.mark.to_string(), "bid": q.bid.to_string(), "ask": q.ask.to_string(), "clamped": trading::engine::options::mark_of(env, &acc.account, &terms).unwrap().mark.to_string()})
            }),
        )
        .await;
    let model: D = mv["mark"].as_str().unwrap().parse().unwrap();
    let tick = D::new(1, 5);
    assert!(model > D::from(30) * tick, "an ATM weekly call is worth more than 30 ticks: {model}");
    eprintln!("model {mv}");
    // A rests a sell of 2 three ticks under the model mid (the passive side may be any price)
    let px = (model / tick).floor() * tick - D::from(3) * tick;
    let Json(o) = api::options_book::place(State(st.clone()), ctx(&ta), body(order(&series, "sell", "limit", 2, Some(px), "a1"))).await.unwrap();
    assert_eq!(o["status"], "working", "{o}");
    assert!(dec(&o["order"]["reserved"]) > D::ZERO);
    // the same clientOrderId again: no second order
    let Json(dup) = api::options_book::place(State(st.clone()), ctx(&ta), body(order(&series, "sell", "limit", 2, Some(px), "a1"))).await.unwrap();
    assert_eq!((dup["duplicate"].as_bool(), dup["order"]["id"].clone()), (Some(true), o["order"]["id"].clone()));
    // the mark clamps to the book: a qualifying ask below the model mid
    let top = options.top.get("kalks", AccountKind::Demo, &series).expect("top of book published");
    assert_eq!(top.ask, Some((px, D::from(2))));
    let s3 = series.clone();
    let clamped = hub.read(b, Box::new(move |x| {
        let (acc, env) = x.unwrap();
        let (terms, _) = trading::engine::options_book::terms_of(&env.options.snapshot().unwrap(), &s3).unwrap();
        json!(trading::engine::options::mark_of(env, &acc.account, &terms).unwrap().mark.to_string())
    })).await;
    assert_eq!(clamped.as_str().unwrap().parse::<D>().unwrap(), px, "mark = min(model, ask) with one qualifying side");

    // the internal market-data WebSocket (the options service's feed)
    let app = api::router(st.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/v1/internal/options/book/stream?tenant=kalks&kind=demo")).await.unwrap();
    let mut first = Vec::new();
    while first.len() < 2 {
        let m = tokio::time::timeout(Duration::from_secs(5), ws.next()).await.unwrap().unwrap().unwrap();
        first.push(serde_json::from_str::<Value>(m.to_text().unwrap()).unwrap());
    }
    assert_eq!((first[0]["type"].as_str(), first[1]["type"].as_str()), (Some("depth"), Some("top")));
    assert_eq!(dec(&first[1]["ask"]), px);

    // B buys 1 at market: an IOC at the band, filled at A's resting price
    let Json(f) = api::options_book::place(State(st.clone()), ctx(&tb), body(order(&series, "buy", "market", 1, None, "b1"))).await.unwrap();
    assert_eq!(f["status"], "filled", "{f}");
    assert_eq!(dec(&f["fills"][0]["price"]), px);
    assert!(f["fills"][0]["positionTicket"].as_i64().is_some(), "the taker's own fill is booked before the answer: {f}");
    drained(&pool).await;
    // the trade on the feed (throttled top + the trade frame)
    let mut saw_trade = false;
    for _ in 0..20 {
        let Ok(Some(Ok(m))) = tokio::time::timeout(Duration::from_secs(3), ws.next()).await else { break };
        let v: Value = serde_json::from_str(m.to_text().unwrap_or("{}")).unwrap_or_default();
        if v["type"] == "trade" {
            assert_eq!((dec(&v["price"]), dec(&v["qty"]), v["side"].as_str()), (px, D::ONE, Some("buy")));
            saw_trade = true;
            break;
        }
    }
    assert!(saw_trade, "a trade frame on the internal stream");
    let Json(snapv) = api::book_feed::snapshot(State(st.clone()), Path(("kalks".into(), "demo".into()))).await.unwrap();
    let sv = snapv["books"][0]["series"].as_array().unwrap().iter().find(|x| x["series"] == series.as_str()).unwrap().clone();
    assert_eq!((dec(&sv["oi"]), dec(&sv["vol"]), dec(&sv["last"]), dec(&sv["asks"][0]["qty"])), (D::ONE, D::ONE, px, D::ONE), "{sv}");
    let Json(tape) = api::book_feed::trades(State(st.clone()), Path(("kalks".into(), "demo".into())), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!(tape["trades"].as_array().unwrap().len(), 1);
    // OI = the positions; clearing nets to zero
    let book_pos = |l: i64| {
        let hub = hub.clone();
        async move { hub.read(l, Box::new(|x| json!(x.unwrap().0.positions.values().filter(|p| p.on_book()).map(|p| (p.volume * p.side.sign()).to_string()).collect::<Vec<_>>()))).await }
    };
    assert_eq!((book_pos(a).await, book_pos(b).await), (json!(["-1"]), json!(["1"])));
    let clearing = format!("house:options_clearing.EURUSD.{}:USD", snap.expiry_by_id(snap.series[&series].expiry_id).unwrap().expiry_date.format("%Y%m%d"));
    assert_eq!(ledger_sum(&pool, &clearing).await, D::ZERO);
    let prem = trading::money::r2(px * D::from(10_000));
    assert!(ledger_sum(&pool, &format!("acct:{b}:balance")).await <= D::from(10_000) - prem, "B paid the premium and the fee");

    // fills list and preview
    let Json(fl) = api::options_book::fills(State(st.clone()), ctx(&tb), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!((fl["fills"][0]["role"].as_str(), fl["fills"][0]["side"].as_str()), (Some("taker"), Some("buy")));
    let Json(pv) = api::options_book::preview(State(st.clone()), ctx(&tb), body(order(&series, "buy", "market", 1, None, "pv"))).await.unwrap();
    assert_eq!(pv["ok"], true, "{pv}");
    assert_eq!(dec(&pv["estAvgPrice"]), px);

    // B closes through the generic close route: a reduce-only market IOC against A's bid one tick lower
    let bid = px - tick;
    let Json(ob) = api::options_book::place(State(st.clone()), ctx(&ta), body(order(&series, "buy", "limit", 1, Some(bid), "a2"))).await.unwrap();
    assert_eq!(ob["status"], "working", "{ob}");
    let ticket = f["fills"][0]["positionTicket"].as_i64().unwrap();
    let Json(cl) = api::terminal::close_position(State(st.clone()), ctx(&tb), Path(ticket.to_string()), None).await.unwrap();
    assert_eq!((cl["status"].as_str(), dec(&cl["filled"]), dec(&cl["avgPrice"]), dec(&cl["left"])), (Some("filled"), D::ONE, bid, D::ZERO), "{cl}");
    drained(&pool).await;
    assert_eq!((book_pos(a).await, book_pos(b).await), (json!([]), json!([])));
    assert_eq!(ledger_sum(&pool, &clearing).await, D::ZERO);
    // amend: a size increase is reserved first and loses priority; a price change too
    let Json(am) = api::options_book::place(State(st.clone()), ctx(&ta), body(order(&series, "sell", "limit", 2, Some(px + D::from(5) * tick), "a3"))).await.unwrap();
    let aid = am["order"]["id"].as_i64().unwrap();
    let r0 = dec(&am["order"]["reserved"]);
    let Json(a2) = api::options_book::amend(State(st.clone()), ctx(&ta), Path(aid.to_string()), body(json!({"qty": 3}))).await.unwrap();
    assert_eq!((dec(&a2["order"]["qty"]), a2["status"].as_str()), (D::from(3), Some("working")), "{a2}");
    assert!(dec(&a2["order"]["reserved"]) > r0, "more reserved for the larger order");
    let Json(a3) = api::options_book::amend(State(st.clone()), ctx(&ta), Path(aid.to_string()), body(json!({"price": (px + D::from(6) * tick).to_string()}))).await.unwrap();
    assert_eq!(dec(&a3["order"]["price"]), px + D::from(6) * tick, "{a3}");
    // a stop_market on the mark: working, nothing reserved until it fires
    let stop = json!({"series": series, "side": "buy", "type": "stop_market", "qty": 1, "trigger": {"source": "mark", "op": "above", "price": (px + D::from(500) * tick).to_string()}, "clientOrderId": "s1"});
    let Json(so) = api::options_book::place(State(st.clone()), ctx(&ta), body(stop)).await.unwrap();
    assert_eq!((so["status"].as_str(), so["order"]["type"].as_str(), so["order"]["trigger"]["source"].as_str()), (Some("working"), Some("stop_market"), Some("mark")), "{so}");
    let Json(open) = api::options_book::orders(State(st.clone()), ctx(&ta), Query(serde_json::from_value(json!({"status": "open"})).unwrap())).await.unwrap();
    assert_eq!(open["orders"].as_array().unwrap().len(), 3, "{open}");
    // A cancels everything (the rest of the first offer, the amended one and the stop): nothing reserved once idle
    let Json(c) = api::options_book::cancel_many(State(st.clone()), ctx(&ta), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!(c["cancelled"].as_array().unwrap().len(), 3, "{c}");
    let Json(hist) = api::options_book::orders(State(st.clone()), ctx(&ta), Query(serde_json::from_value(json!({"status": "history"})).unwrap())).await.unwrap();
    assert!(hist["orders"].as_array().unwrap().iter().any(|o| o["id"].as_i64() == Some(aid) && o["status"] == "cancelled" && dec(&o["qty"]) == D::from(3)), "{hist}");
    drained(&pool).await;
    for l in [a, b] {
        let r = hub.read(l, Box::new(|x| json!(x.unwrap().0.book.reserve().to_string()))).await;
        assert_eq!(r.as_str(), Some("0"), "reserve = 0 when idle");
    }
    let key = trading::book::BookKey::new(1, AccountKind::Demo, "EURUSD");
    let entries = trading::book::journal::entries(&pool, &key, 1).await.unwrap();
    let replayed = trading::book::journal::replay(trading::book::types::UnderlyingBooks::new(key.clone()), &entries).expect("replay identical");
    let live_fp = hub.shared.books.handle(&key).unwrap().read(Box::new(|b| trading::book::journal::fingerprint(b))).await.unwrap();
    assert_eq!(trading::book::journal::fingerprint(&replayed), live_fp);

    // ---------- optional: a real 0DTE fixing settles ----------
    let wait: u64 = env("BOOK_E2E_FIXING_WAIT_SECS", "0").parse().unwrap_or(0);
    let mut fixing_leg = false;
    if wait > 0
        && let Some((s0, cut0)) = pick(&snap, spot, 16, Some(24))
    {
        eprintln!("0DTE {s0}, cut {cut0}: trading, then waiting for the fixing");
        fixing_leg = true;
        let m0 = (options.price("kalks", "*", &trading::engine::options_book::terms_of(&snap, &s0).unwrap().0, Utc::now()).unwrap().mark / tick).floor() * tick;
        let p0 = m0.max(tick);
        let _ = api::options_book::place(State(st.clone()), ctx(&ta), body(order(&s0, "sell", "limit", 3, Some(p0), "z1"))).await.unwrap();
        let Json(z) = api::options_book::place(State(st.clone()), ctx(&tb), body(order(&s0, "buy", "limit", 3, Some(p0), "z2"))).await.unwrap();
        assert_eq!(z["status"], "filled", "{z}");
        drained(&pool).await;
        let deadline = Utc::now() + chrono::Duration::seconds(wait as i64);
        let mut settled = false;
        let mut done = std::collections::HashSet::new();
        while Utc::now() < deadline {
            let _ = options.poll(Some(&pool)).await;
            if trading::options::settle::run_due(&st, &mut done).await > 0 {
                settled = true;
                break;
            }
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        assert!(settled, "the 0DTE fixing was not published within {wait} s");
        let snap0 = options.snapshot().unwrap();
        let e0 = snap0.expiry_by_id(snap.series[&s0].expiry_id).map(|e| (e.key(), e.fixing, e.fixing_run, e.fixing_source.clone()));
        let cash: Vec<(String, D)> = sqlx::query_as("SELECT t.idempotency_key, p.amount FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id WHERE t.idempotency_key LIKE 'settle:%' AND p.account_code LIKE 'acct:%' ORDER BY 1")
            .fetch_all(&pool)
            .await
            .unwrap();
        eprintln!("0DTE settled: expiry {e0:?}, traded 3 at {p0}, settlement cash {cash:?}");
        // the clearing account is named by the expiry DATE (= the cut's date for real listings, not for the
        // weekend-dated test expiries of OPTIONS_TEST_EXPIRIES)
        let d0 = snap.expiry_by_id(snap.series[&s0].expiry_id).unwrap().expiry_date;
        let c0 = format!("house:options_clearing.EURUSD.{}:USD", d0.format("%Y%m%d"));
        let n0: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_postings WHERE account_code = $1").bind(&c0).fetch_one(&pool).await.unwrap();
        assert!(n0 > 0, "the 0DTE fills went through {c0}");
        assert_eq!(ledger_sum(&pool, &c0).await, D::ZERO, "the expiry's clearing nets to 0");
        assert_eq!(ledger_sum(&pool, "house:options_settlement:USD").await, D::ZERO, "long payouts = short charges");
        assert_eq!((book_pos(a).await, book_pos(b).await), (json!([]), json!([])));
    }

    if wait > 0 && !fixing_leg {
        eprintln!("0DTE fixing leg SKIPPED: no listed EURUSD expiry 16 min to 24 h ahead");
    }
    // money: every transaction balances and nets per currency; the account replay = live
    let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(&pool).await.unwrap();
    assert!(nets.iter().all(|(_, v)| v.is_zero()), "{nets:?}");
    let states = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &states).await.unwrap().is_empty());
    for l in [a, b] {
        let live = hub.read(l, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await;
        assert_eq!(serde_json::to_value(&states[&l]).unwrap(), live);
    }
    eprintln!("book e2e OK: {} journal entries, fill at {px}, close at {bid}", entries.len());
    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

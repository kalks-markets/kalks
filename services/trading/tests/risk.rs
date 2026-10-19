//! Risk desk through the real shards, API handlers and PostgreSQL (like tests/controls.rs): server-side exposure,
//! risk limits saved (audited) and enforced at the order gate, client scores computed from real closed trades and
//! stored with their history, routing suggestions proposed, approved (the book transfer, audited) and rejected (never
//! proposed again), alerts raised, escalated and acknowledged.
//!
//! BTCUSD only (it trades every day), so the test does not depend on the weekday. Skipped with a message when
//! PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};

use trading::api::{self, ApiError, AppState, Body, Ctx, LoginAlloc, StaffCtx};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::trade::{self, CloseReq, OrderReq};
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Book, Controls, Mode, Side, Status};
use trading::money::D;
use trading::rules::{Registry, RoutingAction, RoutingCondition, RoutingRule};
use trading::shard::{Hub, Index, NullLp, Op, Shared, Staff, Stats, Streams};
use trading::specs::Specs;

const L1: i64 = 10_000_401;
const L2: i64 = 10_000_402;

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

fn q<T: serde::de::DeserializeOwned>(v: Value) -> Query<T> {
    Query(serde_json::from_value(v).unwrap())
}

fn code(e: &ApiError) -> String {
    match e {
        ApiError::Reject { reject, .. } => reject.code.into(),
        ApiError::Status { code, .. } | ApiError::Conflict { code, .. } => (*code).into(),
        ApiError::Validation { field, .. } => format!("validation:{field}"),
        other => format!("{other:?}"),
    }
}

async fn place(hub: &Hub, login: i64, side: Side, vol: &str) -> Result<(), String> {
    let req = OrderReq::market("BTCUSD", side, d(vol));
    let op: Op = Box::new(move |tx, env| trade::place_order(tx, env, req).map(|_| Value::Null));
    match hub.exec(login, "client", None, "", "", None, op).await {
        Ok(_) => Ok(()),
        Err(trading::shard::ExecError::Reject(r)) => Err(r.code.to_string()),
        Err(e) => Err(format!("{e:?}")),
    }
}

async fn positions(hub: &Hub, login: i64) -> Vec<(i64, Book, D)> {
    let v = hub.read(login, Box::new(|x| json!(x.unwrap().0.positions.values().map(|p| (p.ticket, p.book.as_str(), p.volume.to_string())).collect::<Vec<_>>()))).await;
    v.as_array().unwrap().iter().map(|x| (x[0].as_i64().unwrap(), Book::parse(x[1].as_str().unwrap()).unwrap(), d(x[2].as_str().unwrap()))).collect()
}

#[tokio::test]
async fn risk_desk_end_to_end() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_risk_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
    // no news service in this test (the score's news factor says "not checked")
    unsafe { std::env::set_var("NEWS_URL", "http://127.0.0.1:9") };
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
    let hub = Hub::start(shared, 2, Default::default());
    let set = |bid: &str, ask: &str| quotes.set("standard", "BTCUSD", Quote { bid: d(bid), ask: d(ask), t_ms: Utc::now().timestamp_millis() });
    set("80000", "80020");
    for (login, user) in [(L1, 901), (L2, 902)] {
        let account = Account {
            tenant_id: 1, login, user_id: user, kind: AccountKind::Live, group: "standard".into(), mode: Mode::Hedging, cent: false, leverage: 100,
            status: Status::Active, name: "Risk".into(), route_override: None, controls: Controls::default(), demo: None, created_at: Utc::now(), lifecycle: None,
        };
        hub.open(account, ("h".into(), "i".into()), "test").await.unwrap();
        hub.exec(login, "test", None, "", "", None, Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, d("1000000"), &format!("fund-{login}"), None).map(|_| Value::Null))).await.unwrap();
    }
    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live.max(10_000_900)), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config::for_tests(&url);
    let st = AppState {
        hub: hub.clone(), pool: pool.clone(), keys: Keys::new(&cfg.session_secret), cfg: Arc::new(cfg.clone()), limiter: Limiter::default(),
        tickets: StreamTickets::default(), logins, open_lock: Arc::new(tokio::sync::Mutex::new(())), social,
        presence: Arc::new(trading::controls::Presence::default()),
        gateway: Arc::new(trading::controls::Gateway::new(&cfg.gateway_url, &cfg.gateway_token)),
    };
    let staff = |role: &str| StaffCtx {
        ctx: Ctx { tenant: registry.by_slug("kalks").unwrap(), ip: "198.51.100.9".into(), user_agent: "it".into(), bearer: None },
        staff: Staff { id: "12".into(), name: "Julia Novak".into(), role: role.into() },
        perms: None,
    };
    let reason = json!({"reasonCode": "DLR-03 · Risk management", "note": "risk desk test"});
    let with_reason = |mut v: Value| {
        v.as_object_mut().unwrap().extend(reason.as_object().unwrap().clone());
        v
    };

    /* ---------- exposure ---------- */
    place(&hub, L1, Side::Buy, "2").await.unwrap();
    place(&hub, L2, Side::Sell, "0.5").await.unwrap();
    let Json(x) = api::risk::exposure(State(st.clone()), staff("viewer"), q(json!({}))).await.unwrap();
    let btc = &x["symbols"][0];
    assert_eq!(btc["symbol"], "BTCUSD");
    assert_eq!((btc["longLots"].as_f64(), btc["shortLots"].as_f64(), btc["netLots"].as_f64()), (Some(2.0), Some(0.5), Some(1.5)));
    // mid 80 010 × 1.5 lots (1 BTC per lot)
    assert_eq!(btc["netUsd"].as_f64(), Some(120015.0));
    assert_eq!(btc["books"]["B"]["netUsd"].as_f64(), Some(120015.0));
    assert_eq!(btc["topClients"][0]["login"], L1.to_string());
    assert_eq!(btc["accounts"], 2);
    assert_eq!(x["totals"]["positions"], 2);
    assert!(btc["limit"].is_null());
    // demo exposure is separate (no demo accounts here)
    let Json(xd) = api::risk::exposure(State(st.clone()), staff("viewer"), q(json!({"kind": "demo"}))).await.unwrap();
    assert_eq!(xd["totals"]["positions"], 0);

    /* ---------- limits: validated, audited, enforced ---------- */
    let bad = api::risk::save_limits(State(st.clone()), staff("dealer"), body(with_reason(json!({"limits": {"exposure": [{"symbol": "NOPE", "capUsd": 1}]}})))).await.unwrap_err();
    assert_eq!(code(&bad), "validation:exposure");
    assert_eq!(code(&api::risk::save_limits(State(st.clone()), staff("viewer"), body(with_reason(json!({"limits": {}})))).await.unwrap_err()), "Forbidden(\"Role viewer may not do this\")");
    let Json(r) = api::risk::save_limits(State(st.clone()), staff("risk_manager"), body(with_reason(json!({"limits": {"exposure": [{"symbol": "btcusd", "capUsd": 200000, "warnPct": 50, "mode": "enforce"}], "groups": [{"group": "standard", "maxLot": 10, "mode": "enforce"}]}})))).await.unwrap();
    assert_eq!(r["audit"][0]["action"], "risk.limits");
    assert_eq!(r["data"]["limits"]["exposure"][0]["symbol"], "BTCUSD");
    let Json(l) = api::risk::limits(staff("viewer")).await.unwrap();
    assert_eq!(l["limits"]["exposure"][0]["mode"], "enforce");
    assert_eq!(l["settings"]["routingMode"], "recommend");
    assert_eq!(l["alertOnly"], false);
    // a fresh load from the database sees the same limits
    let reloaded = trading::persist::load_tenant(&pool, 1).await.unwrap();
    assert_eq!(reloaded.risk.limits.exposure_for("BTCUSD").unwrap().cap_usd, d("200000"));
    // the exposure loop's scan feeds the order gate
    let started = Utc::now().timestamp_millis();
    let pos = trading::risk::exposure::scan(&st, 1, AccountKind::Live).await;
    hub.shared.risk.set_index(1, trading::risk::exposure::index_of(&pos, started));
    assert_eq!(place(&hub, L1, Side::Buy, "1").await.unwrap_err(), "exposure_limit", "1.5 → 2.5 lots = 200 025 USD > 200 000");
    assert!(place(&hub, L1, Side::Buy, "0.9").await.is_ok());
    assert!(place(&hub, L2, Side::Sell, "1").await.is_ok(), "reducing");
    assert_eq!(place(&hub, L1, Side::Buy, "11").await.unwrap_err(), "group_max_lot");
    // the view shows the usage
    let Json(x) = api::risk::exposure(State(st.clone()), staff("viewer"), q(json!({}))).await.unwrap();
    assert_eq!(x["symbols"][0]["limit"]["level"], "amber", "{}", x["symbols"][0]);

    /* ---------- client scores from real trades ---------- */
    for i in 0..14 {
        set("80000", "80020");
        let req = OrderReq::market("BTCUSD", Side::Sell, d("0.1"));
        let op: Op = Box::new(move |tx, env| trade::place_order(tx, env, req).map(|_| Value::Null));
        hub.exec(L2, "client", None, "", "", None, op).await.unwrap();
        set(if i % 7 == 6 { "80300" } else { "79700" }, if i % 7 == 6 { "80320" } else { "79720" });
        let t = positions(&hub, L2).await.iter().filter(|p| p.2 == d("0.1")).map(|p| p.0).next().unwrap();
        let op: Op = Box::new(move |tx, env| trade::close_position(tx, env, t, CloseReq::default()).map(|_| Value::Null));
        hub.exec(L2, "client", None, "", "", None, op).await.unwrap();
    }
    set("80000", "80020");
    let n = trading::risk::score::score_tenant(&st, 1, Some(L2), "manual").await.unwrap();
    assert_eq!(n, 1);
    let Json(sc) = api::risk::scores(State(st.clone()), staff("viewer"), q(json!({}))).await.unwrap();
    let it = sc["items"].as_array().unwrap().iter().find(|i| i["login"] == L2.to_string()).unwrap().clone();
    assert_eq!(it["trades"], 14);
    // 12 of 14 won 300 − 20 spread, 2 lost: 85.7 %
    assert_eq!(it["winRate"].as_f64(), Some(85.7));
    assert!(it["score"].as_f64().unwrap() > 55.0, "{it}");
    assert!(it["why"].as_array().unwrap().iter().any(|w| w["factor"] == "news" && w["value"] == "not checked"));
    assert_eq!(it["facts"]["depositsUsd"].as_f64(), Some(1000000.0));
    assert!(hub.shared.risk.score(L2).is_some(), "routing conditions read the new score");
    trading::risk::score::score_tenant(&st, 1, Some(L2), "manual").await.unwrap();
    let Json(one) = api::risk::score(State(st.clone()), staff("viewer"), Path(L2.to_string())).await.unwrap();
    assert_eq!(one["history"].as_array().unwrap().len(), 2, "history kept");
    assert_eq!(one["current"]["login"], L2.to_string());
    let Json(f) = api::risk::scores(State(st.clone()), staff("viewer"), q(json!({"category": "toxic"}))).await.unwrap();
    assert!(f["items"].as_array().unwrap().iter().all(|i| i["category"] == "toxic"));
    let Json(run) = api::risk::run_scores(State(st.clone()), staff("dealer"), body(json!({"login": L2.to_string()}))).await.unwrap();
    assert_eq!(run["data"]["queued"], true);

    /* ---------- routing suggestions ---------- */
    let rule = RoutingRule { id: "RR-T".into(), name: "BTC to A".into(), conditions: vec![RoutingCondition { field: "Symbol".into(), op: "in".into(), value: "BTCUSD".into() }], join: "AND".into(), action: RoutingAction { book: Book::A, pct: 100.0, lp: None }, enabled: true, hits24h: 0, lots24h: 0.0 };
    registry.update(1, |c| c.routing_rules = vec![rule]);
    // recommend mode: a new trade stays on B with the label
    place(&hub, L1, Side::Sell, "0.1").await.unwrap();
    let v = hub.read(L1, Box::new(|x| json!(x.unwrap().0.positions.values().last().unwrap().route_history[0].reason))).await;
    assert!(v.as_str().unwrap().contains("RR-T recommends A-book"), "{v}");
    let added = trading::risk::routing::detect(&st, 1).await.unwrap();
    let Json(list) = api::risk::suggestions(State(st.clone()), staff("viewer"), q(json!({}))).await.unwrap();
    let list = list.as_array().unwrap().clone();
    let pos_sugs: Vec<&Value> = list.iter().filter(|s| s["kind"] == "position").collect();
    assert_eq!(pos_sugs.len(), positions(&hub, L1).await.len() + positions(&hub, L2).await.len(), "{list:?}");
    assert!(added >= pos_sugs.len());
    assert!(pos_sugs.iter().all(|s| s["from"] == "B" && s["to"] == "A" && s["rule"] == "RR-T" && s["status"] == "pending"));
    // detecting again adds nothing
    assert_eq!(trading::risk::routing::detect(&st, 1).await.unwrap(), 0);
    let first = pos_sugs.iter().find(|s| s["login"] == L1.to_string()).unwrap();
    let ticket: i64 = first["ticket"].as_str().unwrap().parse().unwrap();
    assert_eq!(code(&api::risk::approve(State(st.clone()), staff("dealer"), Path(first["id"].as_str().unwrap().to_string()), body(json!({"reasonCode": "", "note": ""}))).await.unwrap_err()), "validation:reasonCode");
    let Json(ok) = api::risk::approve(State(st.clone()), staff("dealer"), Path(first["id"].as_str().unwrap().to_string()), body(reason.clone())).await.unwrap();
    assert_eq!(ok["data"]["suggestion"]["status"], "approved");
    assert_eq!(ok["data"]["done"], json!([ticket.to_string()]));
    let actions: Vec<&str> = ok["audit"].as_array().unwrap().iter().filter_map(|a| a["action"].as_str()).collect();
    assert_eq!(actions, vec!["book.transfer", "routing.suggestion"]);
    assert!(positions(&hub, L1).await.iter().any(|p| p.0 == ticket && p.1 == Book::A), "moved by transfer_book");
    // approving twice is refused
    assert_eq!(code(&api::risk::approve(State(st.clone()), staff("dealer"), Path(first["id"].as_str().unwrap().to_string()), body(reason.clone())).await.unwrap_err()), "already_decided");
    // reject another: never proposed again for that position
    let second = pos_sugs.iter().find(|s| s["login"] == L2.to_string()).unwrap();
    let Json(rj) = api::risk::reject(State(st.clone()), staff("dealer"), Path(second["id"].as_str().unwrap().to_string()), body(reason.clone())).await.unwrap();
    assert_eq!(rj["data"]["suggestion"]["status"], "rejected");
    trading::risk::routing::detect(&st, 1).await.unwrap();
    let Json(all) = api::risk::suggestions(State(st.clone()), staff("viewer"), q(json!({"status": "all"}))).await.unwrap();
    let all = all.as_array().unwrap();
    assert_eq!(all.iter().filter(|s| s["ticket"] == second["ticket"]).count(), 1, "not proposed again");
    assert_eq!(all.iter().filter(|s| s["ticket"] == first["ticket"]).count(), 1, "already on A: nothing more");
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'routing.suggestion'").fetch_one(&pool).await.unwrap();
    assert_eq!(audited, 2);
    // the routing mode is tenant policy
    assert!(api::risk::save_settings(State(st.clone()), staff("dealer"), body(with_reason(json!({"routingMode": "execute"})))).await.is_err());
    let Json(s2) = api::risk::save_settings(State(st.clone()), staff("admin"), body(with_reason(json!({"routingMode": "execute"})))).await.unwrap();
    assert_eq!(s2["data"]["settings"]["routingMode"], "execute");
    place(&hub, L1, Side::Sell, "0.2").await.unwrap();
    assert!(positions(&hub, L1).await.iter().any(|p| p.2 == d("0.2") && p.1 == Book::A), "execute mode routes");

    /* ---------- alerts ---------- */
    use trading::risk::alerts::{AlertSpec, Level, raise};
    let spec = |level| AlertSpec { key: "exposure:BTCUSD".into(), kind: "exposure", level, title: "BTCUSD".into(), message: "test".into(), symbol: Some("BTCUSD".into()), login: None, data: json!({}) };
    let a = raise(&st, 1, spec(Level::Amber)).await.unwrap();
    assert!(raise(&st, 1, spec(Level::Amber)).await.is_none(), "same level: no new push");
    let id = a["id"].as_str().unwrap().to_string();
    let Json(acked) = api::risk::ack_alert(State(st.clone()), staff("dealer"), Path(id.clone()), body(json!({"note": "watching"}))).await.unwrap();
    assert_eq!(acked["data"]["status"], "acked");
    assert_eq!(acked["audit"][0]["action"], "risk.alert_ack");
    let red = raise(&st, 1, spec(Level::Red)).await.unwrap();
    assert_eq!((red["level"].as_str(), red["status"].as_str(), red["count"].as_i64(), red["id"].as_str()), (Some("red"), Some("open"), Some(2), Some(id.as_str())), "escalation re-opens");
    let Json(open) = api::risk::alerts(State(st.clone()), staff("viewer"), q(json!({"status": "open"}))).await.unwrap();
    assert!(open.as_array().unwrap().iter().any(|x| x["id"] == id.as_str()));
    trading::risk::alerts::resolve_missing(&st, 1, &["exposure"], &Default::default()).await;
    let Json(open) = api::risk::alerts(State(st.clone()), staff("viewer"), q(json!({"status": "open"}))).await.unwrap();
    assert!(open.as_array().unwrap().iter().all(|x| x["kind"] != "exposure"));

    // everything still replays and balances
    let states = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &states).await.unwrap().is_empty());
    drop(hub);
    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

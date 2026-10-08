//! Kalks FX Options through the real API handlers, shards and PostgreSQL, with a mock options service (snapshot
//! with ETag, fixings) and a mock gateway (suitability):
//! eligibility (404 = not eligible, live and demo alike), preview, market orders, the position JSON contract,
//! combos (fill and close together), partial close through the generic close route, a barrier knocked once by a
//! raw tick (`option_knocks`), void, the settlement scheduler (crash catch-up exactly once, idempotent, run
//! records), the client settlement list, a settlement re-run (nets the difference, window), the options book,
//! and replay of every event stream giving the live state with a balanced ledger.
//!
//! Skipped with a message when PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use axum::Router;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Json;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use trading::api::{self, ApiError, AppState, Body, Ctx, LoginAlloc, StaffCtx};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, Status};
use trading::money::D;
use trading::options::{OptionPricing, OptionsCtx};
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams, TickMsg};
use trading::specs::Specs;

const ELIGIBLE: i64 = 5101;
const NOT_YET: i64 = 5102;
const C116: &str = "EURUSD-20260925-1.1600-C";
const C117: &str = "EURUSD-20260925-1.1700-C";
const P115: &str = "EURUSD-20260925-1.1500-P";
const KEY: &str = "EURUSD:2026-09-25";

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

fn code(e: &ApiError) -> String {
    match e {
        ApiError::Status { code, .. } | ApiError::Conflict { code, .. } => (*code).into(),
        ApiError::Reject { reject, .. } => reject.code.into(),
        ApiError::Validation { field, .. } => format!("validation:{field}"),
        other => format!("{other:?}"),
    }
}

/// The options service's snapshot for the test week (Monday 2026-09-21 → Friday 2026-09-25 10:00 NY cut).
fn snapshot(version: i64, fixing: Option<(f64, i32)>) -> Value {
    let mut series = Vec::new();
    for k in ["1.1500", "1.1550", "1.1600", "1.1650", "1.1700"] {
        for (kind, c) in [("call", "C"), ("put", "P")] {
            series.push(json!({"code": format!("EURUSD-20260925-{k}-{c}"), "symbol": "EURUSD", "expiryId": 1, "strike": k.parse::<f64>().unwrap(), "kind": kind, "status": "active"}));
        }
    }
    let (status, fx, run) = match fixing {
        Some((f, r)) => ("fixed", json!(f), r),
        None => ("listed", Value::Null, 0),
    };
    json!({
        "version": version, "staleAfterSecs": 300, "conventions": {},
        "underlyings": [{"symbol": "EURUSD", "model": "gk", "baseCcy": "EUR", "quoteCcy": "USD", "calendarCodes": ["EUR", "USD"], "contractSize": 10000,
                         "digits": 5, "pipSize": 0.0001, "cutTime": "10:00", "cutZone": "America/New_York", "noOpenMinutes": 15, "closeOnlyMinutes": 1,
                         "weekendVolWeight": 0.15, "holidayVolWeight": 0.5, "priceScan": 0.03, "volScan": 0.03, "extremeMultiple": 3, "extremeCover": 0.35,
                         "minContracts": 1, "maxContracts": 100, "contractStep": 1, "barriersEnabled": true, "enabled": true, "deltaConvention": "spot"}],
        "rates": [{"ccy": "USD", "rate": 0.04}, {"ccy": "EUR", "rate": 0.02}],
        "holidays": {"USD": [], "EUR": []},
        "surfaces": [{"symbol": "EURUSD", "version": 1, "blendWeight": 0.7, "pillars": [{"tenor": "1W", "days": 7, "atm": 0.07, "rr25": -0.002, "bf25": 0.002}, {"tenor": "1M", "days": 30, "atm": 0.075, "rr25": -0.003, "bf25": 0.0025}]}],
        "realizedVol": [{"symbol": "EURUSD", "estimator": "yang_zhang", "tf": "D1", "windowBars": 20, "value": 0.065}],
        "expiries": [{"id": 1, "symbol": "EURUSD", "expiryDate": "2026-09-25", "cutAt": "2026-09-25T14:00:00Z", "twapStart": "2026-09-25T13:30:00Z",
                      "status": status, "fixing": fx, "fixingSource": "twap", "fixingRun": run}],
        "series": series,
        "tenants": [{"tenant": "kalks", "enabledDemo": true, "enabledLive": true, "publicChain": false, "underlyings": null}],
        "groups": [{"tenant": "kalks", "groupCode": "*", "symbol": "*", "volSpread": 0.004, "minSpreadUsd": 0.5, "commissionPerContract": 0.25,
                    "commissionCapPct": 10, "maxContractsPerClient": 200, "weekendMarginPct": 25, "enabled": true}],
        "controls": [], "clientLimits": []
    })
}

#[derive(Clone, Default)]
struct Mock {
    snapshot: Arc<Mutex<Value>>,
    fixing: Arc<Mutex<Value>>,
    snapshot_calls: Arc<AtomicUsize>,
    not_modified: Arc<AtomicUsize>,
}

async fn mock_snapshot(State(m): State<Mock>, h: HeaderMap) -> Response {
    m.snapshot_calls.fetch_add(1, Ordering::SeqCst);
    let v = m.snapshot.lock().unwrap().clone();
    let etag = format!("\"opt-{}\"", v["version"]);
    if h.get("if-none-match").and_then(|x| x.to_str().ok()) == Some(etag.as_str()) {
        m.not_modified.fetch_add(1, Ordering::SeqCst);
        return (StatusCode::NOT_MODIFIED, [("etag", etag)]).into_response();
    }
    (StatusCode::OK, [("etag", etag)], Json(v)).into_response()
}

async fn mock_fixings(State(m): State<Mock>) -> Json<Value> {
    Json(json!({"fixings": [m.fixing.lock().unwrap().clone()]}))
}

async fn mock_suitability(Path(user): Path<i64>) -> Response {
    if user == ELIGIBLE {
        Json(json!({"eligible": true, "kycVerified": true, "disclosureAccepted": true, "quizPassed": true})).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found"}}))).into_response()
    }
}

async fn serve(m: Mock) -> String {
    let app = Router::new()
        .route("/v1/internal/options/snapshot", get(mock_snapshot))
        .route("/v1/internal/options/fixings", get(mock_fixings))
        .route("/v1/internal/suitability/{user}", get(mock_suitability))
        .with_state(m);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn account(login: i64, user: i64, kind: AccountKind) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: user,
        kind,
        group: "options-standard".into(),
        mode: Mode::Hedging,
        cent: false,
        leverage: 100,
        status: Status::Active,
        name: "Options IT".into(),
        route_override: None,
        controls: Controls::default(),
        demo: (kind == AccountKind::Demo).then(|| DemoCfg { initial_balance: d("10000"), refills_per_day: 3, expiry_days: 10 }),
        created_at: Utc::now(),
        lifecycle: None,
    }
}

async fn staff(st: &AppState) -> StaffCtx {
    let req = axum::http::Request::builder().header("x-kalks-staff-id", "44").header("x-kalks-staff-name", "Rita%20Risk").header("x-kalks-staff-role", "admin").body(()).unwrap();
    let (mut parts, _) = req.into_parts();
    StaffCtx::from_request_parts(&mut parts, st).await.unwrap()
}

/// A Kalks Trader session through the Client Area's Trade button (SSO).
async fn token(st: &AppState, tenant: &Arc<trading::rules::TenantConfig>, login: i64, user: i64) -> String {
    let ctx = || Ctx { tenant: tenant.clone(), ip: "198.51.100.7".into(), user_agent: "it".into(), bearer: None };
    let mut h = HeaderMap::new();
    h.insert("x-kalks-user-id", user.to_string().parse().unwrap());
    let Json(sso) = api::accounts::sso(State(st.clone()), ctx(), h, Path(login), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    let Json(s) = api::terminal::sso(State(st.clone()), ctx(), body(json!({"token": sso["token"]}))).await.unwrap();
    s["token"].as_str().unwrap().to_string()
}

async fn snap_state(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await
}

async fn balance(hub: &Hub, login: i64) -> D {
    hub.read(login, Box::new(|x| x.map(|(s, _)| json!(s.balance.to_string())).unwrap_or(Value::Null))).await.as_str().map(d).unwrap()
}

#[tokio::test]
async fn options_through_the_api_shards_and_postgres() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_options_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate");

    let mock = Mock::default();
    *mock.snapshot.lock().unwrap() = snapshot(1, None);
    let mock_url = serve(mock.clone()).await;

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    let specs = Arc::new(Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap());
    let registry = Registry::default();
    for t in trading::persist::load_registry(&pool).await.unwrap() {
        registry.put(t);
    }
    let (ticket, deal, txn, live, demo) = trading::persist::max_ids(&pool).await.unwrap();
    let quotes = Arc::new(QuoteBook::default());
    let options = Arc::new(OptionsCtx::new(&mock_url, "opt-token", quotes.clone()));
    assert!(options.poll(Some(&pool)).await.unwrap(), "snapshot loaded");
    assert_eq!(options.version(), Some(1));
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
        options: options.clone(),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
    });
    // Monday of the test week, 12:00 UTC (in the past: the snapshot's freshness is measured on the wall clock)
    shared.clock.set(Some(t("2026-09-21T12:00:00Z")));
    let hub = Hub::start(shared.clone(), 2, Default::default());
    let mid = |m: &str| {
        let now = Utc::now().timestamp_millis();
        let (b, a) = (d(m) - d("0.00005"), d(m) + d("0.00005"));
        quotes.set("raw", "EURUSD", Quote { bid: b, ask: a, t_ms: now });
        quotes.set("standard", "EURUSD", Quote { bid: b, ask: a, t_ms: now });
    };
    mid("1.16");
    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let mut cfg = Config::for_tests(&url);
    cfg.options_url = mock_url.clone();
    cfg.gateway_url = mock_url.clone();
    cfg.options_hedge_limit_usd = 100; // hedge anything above 100 USD of delta
    cfg.options_hedge_user = 777;
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
    let ctx = |bearer: Option<&str>| Ctx { tenant: tenant.clone(), ip: "198.51.100.7".into(), user_agent: "it".into(), bearer: bearer.map(str::to_string) };

    // accounts: live (eligible client), live (no answer for this client yet), demo of the eligible client, demo of the other
    let (live_ok, live_no, demo_no, demo_nn) = (10_000_501i64, 10_000_502i64, 50_000_501i64, 50_000_502i64);
    hub.open(account(live_ok, ELIGIBLE, AccountKind::Live), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(live_no, NOT_YET, AccountKind::Live), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(demo_no, ELIGIBLE, AccountKind::Demo), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(demo_nn, NOT_YET, AccountKind::Demo), ("h".into(), "i".into()), "test").await.unwrap();
    for (i, l) in [live_ok, live_no].iter().enumerate() {
        let key = format!("it-opt-fund-{i}");
        let op: Op = Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, d("10000"), &key, None).map(|_| Value::Null));
        hub.exec(*l, "test", None, "", "", None, op).await.unwrap();
    }
    let tok_ok = token(&st, &tenant, live_ok, ELIGIBLE).await;
    let tok_no = token(&st, &tenant, live_no, NOT_YET).await;
    let tok_demo = token(&st, &tenant, demo_no, ELIGIBLE).await;
    let tok_demo_no = token(&st, &tenant, demo_nn, NOT_YET).await;
    let order = |legs: Value, cid: &str| json!({"legs": legs, "type": "market", "clientOrderId": cid});
    let leg = |s: &str, side: &str, c: i64| json!({"series": s, "side": side, "contracts": c});

    // eligibility: the gateway has no answer for this client yet (404): refused on live and on demo
    let e = api::options::place(State(st.clone()), ctx(Some(&tok_no)), body(order(json!([leg(C116, "buy", 1)]), "n1"))).await.unwrap_err();
    assert_eq!(code(&e), "not_eligible");
    let e = api::options::place(State(st.clone()), ctx(Some(&tok_demo_no)), body(order(json!([leg(C116, "buy", 1)]), "n2"))).await.unwrap_err();
    assert_eq!(code(&e), "not_eligible", "demo accounts need the options intro too");
    let Json(pv) = api::options::preview(State(st.clone()), ctx(Some(&tok_no)), body(order(json!([leg(C116, "buy", 1)]), "n1"))).await.unwrap();
    assert_eq!((pv["ok"].as_bool(), pv["reasons"][0]["code"].as_str()), (Some(false), Some("not_eligible")));
    let Json(dm) = api::options::place(State(st.clone()), ctx(Some(&tok_demo)), body(order(json!([leg(C116, "buy", 1)]), "d1"))).await.unwrap();
    assert_eq!(dm["status"], "filled");
    // clientOrderId is required
    assert_eq!(code(&api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(json!({"legs": [leg(C116, "buy", 1)]}))).await.unwrap_err()), "validation:clientOrderId");

    // preview then the same order: the preview's money matches the fill
    let Json(pv) = api::options::preview(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg(C116, "buy", 2)]), "o1"))).await.unwrap();
    assert_eq!(pv["ok"], true, "{pv}");
    for k in ["legs", "netPremium", "commission", "marginBefore", "marginAfter", "freeMarginAfter", "cashAfter", "maxProfit", "maxLoss", "breakevens", "greeks"] {
        assert!(pv.get(k).is_some(), "preview field {k}: {pv}");
    }
    assert!(pv["maxProfit"].is_null(), "a long call's profit is unlimited");
    let bal0 = balance(&hub, live_ok).await;
    let Json(o) = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg(C116, "buy", 2)]), "o1"))).await.unwrap();
    assert_eq!(o["status"], "filled", "{o}");
    let p = &o["positions"][0];
    let long = p["ticket"].as_i64().unwrap();
    assert_eq!((p["symbol"].as_str(), p["volume"].as_f64(), p["option"]["series"].as_str()), (Some(C116), Some(2.0), Some(C116)));
    assert_eq!((p["option"]["right"].as_str(), p["option"]["style"].as_str(), p["option"]["expiry"].as_str(), p["option"]["contractSize"].as_f64()), (Some("call"), Some("vanilla"), Some("2026-09-25"), Some(10000.0)));
    assert!(p["comboId"].is_null() && p["mark"].as_f64().is_some() && p["greeks"]["delta"].as_f64().is_some_and(|x| x > 0.0), "{p}");
    let paid = bal0 - balance(&hub, live_ok).await;
    assert_eq!(paid, D::from_str(&pv["netPremium"].to_string()).unwrap() + D::from_str(&pv["commission"].to_string()).unwrap());
    // a repeated submit is harmless
    let Json(o2) = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg(C116, "buy", 2)]), "o1"))).await.unwrap();
    assert_eq!((o2["duplicate"].as_bool(), o2["positions"].as_array().map(Vec::len)), (Some(true), Some(1)));

    // the terminal state lists the option with the contract's fields
    let Json(state) = api::terminal::state(State(st.clone()), ctx(Some(&tok_ok)), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert!(state["positions"].as_array().unwrap().iter().any(|x| x["option"]["series"] == C116));
    assert!(state["account"]["optionValue"].as_f64().unwrap() > 0.0);

    // a combo fills as one strategy and closes as one
    let Json(c) = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg(C116, "buy", 1), leg(C117, "sell", 1)]), "c1"))).await.unwrap();
    let combo = c["comboId"].as_i64().expect("combo id");
    assert_eq!(c["positions"].as_array().unwrap().len(), 2);
    assert!(c["positions"].as_array().unwrap().iter().all(|x| x["comboId"] == combo));
    let Json(cc) = api::options::close_combo(State(st.clone()), ctx(Some(&tok_ok)), Path(combo.to_string())).await.unwrap();
    assert_eq!(cc["legs"].as_array().unwrap().len(), 2);
    // all-or-nothing: a second leg over the per-order maximum books nothing
    let v0 = snap_state(&hub, live_ok).await["version"].clone();
    let e = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg(C116, "buy", 1), leg(P115, "sell", 500)]), "c2"))).await.unwrap_err();
    assert_eq!(code(&e), "invalid_volume");
    assert_eq!(snap_state(&hub, live_ok).await["version"], v0);

    // partial close through the generic terminal route
    let Json(pc) = api::terminal::close_position(State(st.clone()), ctx(Some(&tok_ok)), Path(long.to_string()), Some(Json(serde_json::from_value(json!({"volume": 1})).unwrap()))).await.unwrap();
    assert_eq!(pc["status"], "closed");
    assert_eq!(snap_state(&hub, live_ok).await["positions"][long.to_string()]["volume"], "1");

    // a barrier option knocked out by a raw tick: once
    let ko = json!([{"series": C116, "side": "buy", "contracts": 1, "barrier": {"kind": "UO", "level": 1.175, "rebate": 0.0002}}]);
    let Json(k) = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(ko, "k1"))).await.unwrap();
    let kt = k["positions"][0]["ticket"].as_i64().unwrap();
    assert_eq!(k["positions"][0]["option"]["barrier"]["kind"], "UO");
    mid("1.1751");
    for _ in 0..3 {
        hub.tick(Arc::new(TickMsg { group: Arc::from("raw"), symbol: Arc::from("EURUSD"), recv_ms: Utc::now().timestamp_millis() })).await;
    }
    let mut knocks = 0i64;
    for _ in 0..50 {
        knocks = sqlx::query_scalar("SELECT count(*) FROM option_knocks WHERE ticket = $1").bind(kt).fetch_one(&pool).await.unwrap();
        if knocks > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(knocks, 1);
    let rebates: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE idempotency_key = $1").bind(format!("knock:{kt}")).fetch_one(&pool).await.unwrap();
    assert_eq!(rebates, 1);
    assert!(snap_state(&hub, live_ok).await["positions"].get(kt.to_string()).is_none());
    mid("1.16");

    // void an erroneous trade (Back Office): every cash flow back, shown as corrections
    let before_void = balance(&hub, live_ok).await;
    let Json(vo) = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg(P115, "buy", 3)]), "v1"))).await.unwrap();
    let vt = vo["positions"][0]["ticket"].as_i64().unwrap();
    let e = api::options::void(State(st.clone()), staff(&st).await, Path(vt.to_string()), body(json!({"reasonCode": "DLR-02"}))).await.unwrap_err();
    assert_eq!(code(&e), "validation:note");
    let Json(vd) = api::options::void(State(st.clone()), staff(&st).await, Path(vt.to_string()), body(json!({"reasonCode": "DLR-02", "note": "fat finger"}))).await.unwrap();
    assert_eq!(vd["audit"][0]["action"], "options.void");
    assert_eq!(balance(&hub, live_ok).await, before_void);
    let status: String = sqlx::query_scalar("SELECT status FROM positions WHERE ticket = $1").bind(vt).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "voided");

    // a short for the other side of the settlement and a second account to settle
    let Json(sh) = api::options::place(State(st.clone()), ctx(Some(&tok_ok)), body(order(json!([leg("EURUSD-20260925-1.1650-C", "sell", 1)]), "s1"))).await.unwrap();
    let short = sh["positions"][0]["ticket"].as_i64().unwrap();

    // the options book (Back Office)
    let Json(book) = api::options::book(State(st.clone()), staff(&st).await, Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    let u = &book["underlyings"][0];
    assert_eq!(u["symbol"], "EURUSD");
    assert_eq!((u["longContracts"].as_f64(), u["shortContracts"].as_f64(), u["clients"].as_u64()), (Some(1.0), Some(1.0), Some(1)), "{book}");
    assert!(book["topClients"].as_array().is_some_and(|a| a.len() == 1));

    // the house delta hedger: the house is short the live clients' delta and hedges it with CFDs on its hedge account
    let tenant_cfg = registry.get(1).unwrap();
    let client = trading::options::hedger::client_delta(&st, 1, None).await;
    let units = client["EURUSD"];
    assert!(units > 1000.0, "the live client is net long calls: {units}");
    let placed = trading::options::hedger::hedge_tenant(&st, &tenant_cfg).await.unwrap();
    assert_eq!(placed, 1);
    let hedge_login: i64 = sqlx::query_scalar("SELECT login FROM option_hedge_accounts WHERE tenant_id = 1").fetch_one(&pool).await.unwrap();
    let hedge = trading::options::hedger::hedge_units(&st, hedge_login).await;
    assert!(hedge["EURUSD"] > 0.0 && (hedge["EURUSD"] - units).abs() < 1000.0, "hedged within one lot step: {hedge:?} vs {units}");
    assert_eq!(trading::options::hedger::hedge_tenant(&st, &tenant_cfg).await.unwrap(), 0, "already within the limit");
    let (side, status): (String, String) = sqlx::query_as("SELECT side, status FROM option_hedges WHERE tenant_id = 1").fetch_one(&pool).await.unwrap();
    assert_eq!((side.as_str(), status.as_str()), ("buy", "filled"));
    let Json(book) = api::options::book(State(st.clone()), staff(&st).await, Query(serde_json::from_value(json!({"kind": "all"})).unwrap())).await.unwrap();
    assert!(book["underlyings"][0]["hedgeContracts"].as_f64().unwrap() > 0.0, "{book}");
    assert_eq!(book["hedgeAccount"], hedge_login);

    // ---------- settlement ----------
    // the cut passes, the options service publishes the fixing
    shared.clock.set(Some(t("2026-09-25T14:05:00Z")));
    *mock.snapshot.lock().unwrap() = snapshot(2, Some((1.17, 1)));
    assert!(options.poll(Some(&pool)).await.unwrap());
    // a crash in the middle of a pass: the demo account was settled before the engine stopped
    let op: Op = Box::new(|tx, env| Ok(json!(trading::engine::options::settle(tx, env, KEY, d("1.17"), 1, None).tickets)));
    let first = hub.exec(demo_no, "system", None, "", "", None, op).await.unwrap();
    assert_eq!(first.value.as_array().unwrap().len(), 1);
    let ok_before = balance(&hub, live_ok).await;
    let mut done = HashSet::new();
    let n = trading::options::settle::run_due(&st, &mut done).await;
    assert_eq!(n, 2, "the rest settles: the long call and the short call of the live account");
    // long 1 × (1.17 - 1.16) × 10 000 = +100, short 1 × (1.17 - 1.165) × 10 000 = -50
    assert_eq!(balance(&hub, live_ok).await - ok_before, d("50.00"));
    assert_eq!(trading::options::settle::run_due(&st, &mut HashSet::new()).await, 0, "idempotent");
    let keys: Vec<(String, i64)> = sqlx::query_as("SELECT idempotency_key, count(*) FROM ledger_txns WHERE idempotency_key LIKE 'settle:%' GROUP BY 1").fetch_all(&pool).await.unwrap();
    assert!(keys.iter().all(|(_, n)| *n == 1), "exactly once per ticket: {keys:?}");
    assert_eq!(keys.len(), 3, "{keys:?}");
    let runs: Vec<(String, i32, i32)> = sqlx::query_as("SELECT kind, run, positions FROM option_settlement_runs WHERE expiry_key = $1").bind(KEY).fetch_all(&pool).await.unwrap();
    // the run's record counts the whole run from the ledger, the demo position settled before the crash included
    assert_eq!(runs, vec![("settle".to_string(), 1, 3)]);
    // the proceeds are held for the re-run window
    let acc = api::terminal::account_view(&st, live_ok).await;
    assert_eq!(acc["settlementHold"].as_f64(), Some(100.0));

    // the client's settlement list
    let Json(sl) = api::options::settlements(State(st.clone()), ctx(Some(&tok_ok)), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    let items = sl["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i["fixing"].as_f64() == Some(1.17) && i["run"] == 1));
    assert!(items.iter().any(|i| i["ticket"] == long && i["payout"].as_f64() == Some(100.0)));
    assert!(items.iter().any(|i| i["ticket"] == short && i["payout"].as_f64() == Some(-50.0)));

    // ---------- re-run at a corrected fixing ----------
    let e = api::options::rerun(State(st.clone()), staff(&st).await, Path(KEY.into()), body(json!({"reason": "bad tick"}))).await.unwrap_err();
    assert_eq!(code(&e), "nothing_to_rerun", "not re-fixed yet");
    *mock.fixing.lock().unwrap() = json!({"symbol": "EURUSD", "date": "2026-09-25", "cutAt": "2026-09-25T14:00:00Z", "status": "fixed", "fixing": 1.168, "run": 2, "source": "twap"});
    let e = api::options::rerun(State(st.clone()), staff(&st).await, Path(KEY.into()), body(json!({"reason": ""}))).await.unwrap_err();
    assert_eq!(code(&e), "validation:reason");
    // outside the window it is refused
    shared.clock.set(Some(t("2026-09-25T15:30:00Z")));
    let e = api::options::rerun(State(st.clone()), staff(&st).await, Path(KEY.into()), body(json!({"reason": "bad tick"}))).await.unwrap_err();
    assert_eq!(code(&e), "rerun_window_closed");
    shared.clock.set(Some(t("2026-09-25T14:40:00Z")));
    let ok_before = balance(&hub, live_ok).await;
    let demo_before = balance(&hub, demo_no).await;
    let Json(rr) = api::options::rerun(State(st.clone()), staff(&st).await, Path(KEY.into()), body(json!({"reason": "bad tick in the TWAP window"}))).await.unwrap();
    assert_eq!((rr["run"].as_i64(), rr["positions"].as_u64(), rr["accounts"].as_u64()), (Some(2), Some(3), Some(2)), "{rr}");
    // long +80 instead of +100, short -30 instead of -50: net unchanged for the live account; the demo long: -20
    assert_eq!(balance(&hub, live_ok).await - ok_before, d("0.00"));
    assert_eq!(balance(&hub, demo_no).await - demo_before, d("-20.00"));
    let reversed: i64 = sqlx::query_scalar("SELECT count(*) FROM deals WHERE reason = 'expiry' AND reversed").fetch_one(&pool).await.unwrap();
    assert_eq!(reversed, 3);
    let e = api::options::rerun(State(st.clone()), staff(&st).await, Path(KEY.into()), body(json!({"reason": "again"}))).await.unwrap_err();
    assert_eq!(code(&e), "nothing_to_rerun", "a re-run applies once");
    let kinds: Vec<(String, i32)> = sqlx::query_as("SELECT kind, run FROM option_settlement_runs WHERE expiry_key = $1 ORDER BY run").bind(KEY).fetch_all(&pool).await.unwrap();
    assert_eq!(kinds, vec![("settle".to_string(), 1), ("rerun".to_string(), 2)]);
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'options.settlement_rerun'").fetch_one(&pool).await.unwrap();
    assert_eq!(audited, 2);

    // ---------- replay ----------
    let mut live_states = HashMap::new();
    for l in [live_ok, live_no, demo_no] {
        live_states.insert(l, snap_state(&hub, l).await);
    }
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for (l, v) in &live_states {
        assert_eq!(&serde_json::to_value(&replayed[l]).unwrap(), v, "replay diverged for {l}");
    }
    assert!(trading::persist::verify_balances(&pool, &replayed).await.unwrap().is_empty());
    let nets: Vec<(String, D)> = sqlx::query_as("SELECT currency, sum(amount) FROM ledger_postings GROUP BY currency").fetch_all(&pool).await.unwrap();
    assert!(nets.iter().all(|(_, v)| v.is_zero()), "{nets:?}");
    let house: Vec<(String, D)> = sqlx::query_as("SELECT code, balance FROM ledger_accounts WHERE code LIKE 'house:options%'").fetch_all(&pool).await.unwrap();
    assert_eq!(house.len(), 2, "premium and settlement house accounts: {house:?}");

    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
}

/// The snapshot poller: ETag / 304, the version only moves forward, staleness after `staleAfterSecs`.
#[tokio::test]
async fn snapshot_poller_honours_etags_and_goes_stale() {
    let mock = Mock::default();
    *mock.snapshot.lock().unwrap() = snapshot(7, None);
    let url = serve(mock.clone()).await;
    let ctx = OptionsCtx::new(&url, "tok", Arc::new(QuoteBook::default()));
    assert!(ctx.stale(Utc::now()), "no snapshot yet = close-only");
    assert!(ctx.poll(None).await.unwrap());
    assert_eq!(ctx.version(), Some(7));
    assert!(!ctx.poll(None).await.unwrap(), "304: nothing new");
    assert_eq!(mock.not_modified.load(Ordering::SeqCst), 1);
    assert!(!ctx.stale(Utc::now()));
    assert!(ctx.stale(Utc::now() + chrono::Duration::seconds(301)), "older than staleAfterSecs");
    *mock.snapshot.lock().unwrap() = snapshot(8, None);
    assert!(ctx.poll(None).await.unwrap());
    assert_eq!((ctx.version(), mock.snapshot_calls.load(Ordering::SeqCst)), (Some(8), 3));
    // a broken options service keeps the last snapshot
    let dead = OptionsCtx::new("127.0.0.1:1", "tok", Arc::new(QuoteBook::default()));
    assert!(dead.poll(None).await.is_err());
}

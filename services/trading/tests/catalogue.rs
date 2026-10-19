//! Integration test against PostgreSQL (:5433, a throw-away database): the Back Office symbol settings of the
//! instrument catalogue through the real API handlers and the shards. Crypto trades live by default, the platform
//! owner can switch a class off (and a symbol back on), the switches and template overrides persist and hot-swap the
//! engine's specs, only the platform owner may change them, a contract-size change waits while positions are open,
//! and every change is audited. Fails (does not skip) when PostgreSQL is not reachable.

use axum::extract::{Path, State};
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
use trading::engine::trade::{self, OrderReq};
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, DemoCfg, Mode, Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{ExecError, Hub, Index, NullLp, Op, Shared, Staff, Stats, Streams};
use trading::specs::Specs;

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn account(login: i64, kind: AccountKind) -> Account {
    Account {
        tenant_id: 1,
        login,
        user_id: 77,
        kind,
        group: "standard".into(),
        mode: Mode::Hedging,
        cent: false,
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
    hub.exec(login, "test", None, "", "", None, op).await.map(|d| d.value)
}

fn staff(st: &AppState, role: &str) -> StaffCtx {
    let tenant = st.hub.shared.registry.by_slug("kalks").unwrap();
    StaffCtx { ctx: Ctx { tenant, ip: "127.0.0.1".into(), user_agent: "it".into(), bearer: None }, staff: Staff { id: "1".into(), name: format!("IT {role}"), role: role.into() }, perms: None }
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

fn code(e: ApiError) -> String {
    match e {
        ApiError::Forbidden(_) => "forbidden".into(),
        ApiError::Validation { field, .. } => format!("validation:{field}"),
        ApiError::StatusData { status, code, .. } => format!("{status}:{code}"),
        ApiError::NotFound(_) => "not_found".into(),
        other => format!("{other:?}").chars().take(60).collect(),
    }
}

#[tokio::test]
async fn back_office_live_switch_and_templates_drive_the_engine() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_catalogue_{}", std::process::id());
    let server = PgConnectOptions::from_str(&base).unwrap();
    server.clone().database("postgres").connect().await.expect("PostgreSQL :5433 must be running for this test");
    let url = server.clone().database(&db).to_url_lossy().to_string();
    let pool = trading::persist::connect(&url).await.expect("connect + migrate");

    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
    let specs = Specs::load(&format!("{root}/instruments.json"), &format!("{root}/trading-specs.json")).unwrap();
    let specs = Arc::new(specs.with_overrides(trading::catalogue::load_overrides(&pool).await.unwrap()).unwrap());
    assert!(specs.len() > 1000);
    let registry = Registry::default();
    for t in trading::persist::load_registry(&pool).await.unwrap() {
        registry.put(t);
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

    // BNBUSD: a catalogue crypto (24/7, so the test runs any day)
    let now = Utc::now().timestamp_millis();
    for g in ["standard", "pro", "ecn", "cent", "raw"] {
        quotes.set(g, "BNBUSD", Quote { bid: d("610.00"), ask: d("610.40"), t_ms: now });
    }
    let (live_login, demo_login) = (10_000_301i64, 50_000_301i64);
    hub.open(account(live_login, AccountKind::Live), ("h".into(), "i".into()), "test").await.unwrap();
    hub.open(account(demo_login, AccountKind::Demo), ("h".into(), "i".into()), "test").await.unwrap();
    exec(&hub, live_login, Box::new(|tx, env| funds::transfer(tx, env, funds::Direction::In, d("10000"), "it-cat-fund", None).map(|_| Value::Null))).await.unwrap();
    let buy = || -> Op { Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BNBUSD", Side::Buy, d("0.5"))).map(|r| json!(format!("{r:?}")))) };

    // crypto trades live by default; demo too; stocks wait (demo only)
    exec(&hub, live_login, buy()).await.expect("crypto is live by default");
    exec(&hub, demo_login, buy()).await.expect("demo trades catalogue symbols");
    assert!(!hub.shared.specs.load().get("MSFT").unwrap().live, "stocks are not live by default");

    // only the platform owner may switch a class off (or on)
    let r = api::catalogue::set_live(State(st.clone()), staff(&st, "admin"), body(json!({"scope": "class", "key": "crypto", "enabled": false, "reason": "pause crypto"}))).await;
    assert_eq!(code(r.unwrap_err()), "forbidden");
    let r = api::catalogue::set_live(State(st.clone()), staff(&st, "platform_owner"), body(json!({"scope": "symbol", "key": "EURUSD", "enabled": false, "reason": "core"}))).await;
    assert_eq!(code(r.unwrap_err()), "validation:key", "core instruments are always live");
    let r = api::catalogue::set_live(State(st.clone()), staff(&st, "platform_owner"), body(json!({"scope": "class", "key": "crypto", "enabled": false, "reason": ""}))).await;
    assert_eq!(code(r.unwrap_err()), "validation:reason");
    let _ = api::catalogue::set_live(State(st.clone()), staff(&st, "platform_owner"), body(json!({"scope": "class", "key": "crypto", "enabled": false, "reason": "pause crypto"}))).await.unwrap();
    // the shards use the new specs at once: new live positions refused, demo unaffected
    match exec(&hub, live_login, buy()).await {
        Err(ExecError::Reject(r)) => assert_eq!(r.code, "symbol_demo_only"),
        other => panic!("live order must be refused once crypto is off: {other:?}"),
    }
    exec(&hub, demo_login, buy()).await.expect("demo is never switched off");

    // a symbol switched on wins over its class
    let _ = api::catalogue::set_live(State(st.clone()), staff(&st, "super_admin"), body(json!({"scope": "symbol", "key": "BNBUSD", "enabled": true, "reason": "BNB only"}))).await.unwrap();
    exec(&hub, live_login, buy()).await.expect("symbol switch on");
    // stored: a restart loads the same switches
    let ov = trading::catalogue::load_overrides(&pool).await.unwrap();
    assert_eq!((ov.live_classes.get("crypto"), ov.live_symbols.get("BNBUSD")), (Some(&false), Some(&true)));
    // back to the defaults (null removes a switch)
    let _ = api::catalogue::set_live(State(st.clone()), staff(&st, "platform_owner"), body(json!({"scope": "symbol", "key": "BNBUSD", "enabled": null, "reason": "back to class"}))).await.unwrap();
    let ok = api::catalogue::set_live(State(st.clone()), staff(&st, "platform_owner"), body(json!({"scope": "class", "key": "crypto", "enabled": null, "reason": "back to default"}))).await.unwrap();
    assert!(ok.0["catalogueLive"].as_u64().unwrap() > 200);
    exec(&hub, live_login, buy()).await.expect("default again");
    let ov = trading::catalogue::load_overrides(&pool).await.unwrap();
    assert!(ov.live_classes.is_empty() && ov.live_symbols.is_empty());

    // templates: validation, contract size blocked while positions are open, other fields apply at once
    let r = api::catalogue::set_template(State(st.clone()), staff(&st, "platform_owner"), Path("crypto".into()), body(json!({"fields": {"max_leverage": 5000}, "reason": "typo"}))).await;
    assert_eq!(code(r.unwrap_err()), "422:validation");
    let r = api::catalogue::set_template(State(st.clone()), staff(&st, "platform_owner"), Path("crypto".into()), body(json!({"fields": {"bogus": 1}, "reason": "typo"}))).await;
    assert_eq!(code(r.unwrap_err()), "validation:fields");
    let r = api::catalogue::set_template(State(st.clone()), staff(&st, "platform_owner"), Path("crypto".into()), body(json!({"fields": {"contract_size": 10}, "reason": "bigger lots"}))).await;
    assert_eq!(code(r.unwrap_err()), "409:open_interest");
    let r = api::catalogue::set_template(State(st.clone()), staff(&st, "admin"), Path("crypto".into()), body(json!({"fields": {"max_leverage": 3}, "reason": "safer"}))).await;
    assert_eq!(code(r.unwrap_err()), "forbidden");
    let _ = api::catalogue::set_template(State(st.clone()), staff(&st, "platform_owner"), Path("crypto".into()), body(json!({"fields": {"max_leverage": 3, "lot_max": 2}, "reason": "safer"}))).await.unwrap();
    let now_specs = hub.shared.specs.load();
    assert_eq!(now_specs.get("BNBUSD").unwrap().max_leverage, 3);
    assert_eq!(now_specs.get("ETHUSD").unwrap().max_leverage, specs.get("ETHUSD").unwrap().max_leverage, "core crypto untouched");
    // back to the file template
    let _ = api::catalogue::set_template(State(st.clone()), staff(&st, "platform_owner"), Path("crypto".into()), body(json!({"fields": null, "reason": "revert"}))).await.unwrap();
    assert_eq!(hub.shared.specs.load().get("BNBUSD").unwrap().max_leverage, 10);

    // the Back Office listing and the audit trail
    let cat = api::catalogue::catalogue(State(st.clone()), staff(&st, "dealer")).await.unwrap().0;
    assert_eq!(cat["canChange"], false);
    assert!(cat["symbols"].as_array().unwrap().len() > 1000);
    assert_eq!(cat["counts"]["forex"]["core"], 9);
    assert!(cat["liveDefaultClasses"].as_array().unwrap().iter().any(|c| c == "crypto"));
    assert!(!cat["liveDefaultClasses"].as_array().unwrap().iter().any(|c| c == "stocks"));
    assert!(cat["templates"].as_array().unwrap().iter().any(|t| t["key"] == "stocks-us" && t["symbols"].as_u64().unwrap() > 100));
    let audit = api::catalogue::audit_log(State(st.clone()), staff(&st, "dealer")).await.unwrap().0;
    let actions: Vec<&str> = audit["changes"].as_array().unwrap().iter().filter_map(|c| c["action"].as_str()).collect();
    assert_eq!(actions.iter().filter(|a| **a == "symbols.live").count(), 4);
    assert_eq!(actions.iter().filter(|a| **a == "symbols.template").count(), 2);
    assert!(sqlx::query("DELETE FROM symbol_catalogue_audit").execute(&pool).await.is_err(), "audit is append-only");

    // replay of every account is unchanged by the hot-swapped specs
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &replayed).await.unwrap().is_empty());
    assert_eq!(replayed[&live_login].positions.len(), 3);
    assert_eq!(replayed[&demo_login].positions.len(), 2);

    pool.close().await;
    let mut admin = server.database("postgres").connect().await.unwrap();
    let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut admin).await;
    eprintln!("catalogue DB test ran against {base}");
}

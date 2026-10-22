//! Risk leverage caps through the real shards, API handlers and PostgreSQL (docs/RISK-DESK.md › Leverage control): the
//! preview, apply (audited, alert raised, open positions re-margined and margin-checked at once), the account view's
//! effective leverage, a client's leverage change above the cap refused, a reload from the database, and disabling.
//!
//! BTCUSD only (it trades every day). Skipped with a message when PostgreSQL is not reachable
//! (TRADING_TEST_DATABASE_URL, default :5433).

use axum::Json;
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
use trading::model::{Account, AccountKind, Controls, Mode, Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Staff, Stats, Streams};
use trading::specs::Specs;

const L1: i64 = 10_000_501;
const L2: i64 = 10_000_502;
const L3: i64 = 10_000_503;

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
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

/// The account view (with margin, margin call flag and the effective leverage fields).
async fn view(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| {
        let (st, env) = x.unwrap();
        let mut v = trading::views::account_json(env, st);
        v["marginCallFlag"] = json!(st.margin_call);
        v
    }))
    .await
}

#[tokio::test]
async fn leverage_caps_end_to_end() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_levcap_{}", std::process::id());
    let Ok(server) = PgConnectOptions::from_str(&base) else { return };
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return;
    }
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
    quotes.set("standard", "BTCUSD", Quote { bid: d("80000"), ask: d("80020"), t_ms: Utc::now().timestamp_millis() });
    for (login, user, fund) in [(L1, 911, "10000"), (L2, 912, "1000000"), (L3, 913, "1000")] {
        let account = Account {
            tenant_id: 1, login, user_id: user, kind: AccountKind::Live, group: "standard".into(), mode: Mode::Hedging, cent: false, leverage: 100,
            status: Status::Active, name: "Cap".into(), route_override: None, controls: Controls::default(), demo: None, created_at: Utc::now(), lifecycle: None,
        };
        hub.open(account, ("h".into(), "i".into()), "test").await.unwrap();
        hub.exec(login, "test", None, "", "", None, Box::new(move |tx, env| funds::transfer(tx, env, funds::Direction::In, d(fund), &format!("fund-{login}"), None).map(|_| Value::Null))).await.unwrap();
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
    let reason = |v: Value, note: &str| {
        let mut v = v;
        v["reasonCode"] = json!("DLR-03 · Risk management");
        v["note"] = json!(note);
        v
    };

    // 1 BTC each: margin 80 010 / 20 (BTCUSD's own max leverage) = 4 000.50
    place(&hub, L1, Side::Buy, "1").await.unwrap();
    place(&hub, L2, Side::Buy, "1").await.unwrap();
    assert_eq!(view(&hub, L1).await["margin"].as_f64(), Some(4000.5));

    let Json(list) = api::risk::leverage_caps(staff("viewer")).await.unwrap();
    assert_eq!(list["caps"], json!([]));
    assert_eq!(list["assetClasses"].as_array().unwrap().len(), 6);

    /* ---------- preview ---------- */
    let crypto = json!({"scope": "asset_class", "target": "crypto", "maxLeverage": 8, "appliesTo": "all_positions"});
    assert_eq!(code(&api::risk::preview_leverage_cap(State(st.clone()), staff("viewer"), body(crypto.clone())).await.unwrap_err()), "Forbidden(\"Role viewer may not do this\")");
    assert_eq!(code(&api::risk::preview_leverage_cap(State(st.clone()), staff("dealer"), body(json!({"scope": "group", "target": "nope", "maxLeverage": 8}))).await.unwrap_err()), "validation:target");
    assert_eq!(code(&api::risk::preview_leverage_cap(State(st.clone()), staff("dealer"), body(json!({"scope": "account", "target": "99999999", "maxLeverage": 8}))).await.unwrap_err()), "validation:target");
    let Json(p) = api::risk::preview_leverage_cap(State(st.clone()), staff("dealer"), body(crypto.clone())).await.unwrap();
    assert_eq!(p["totals"]["accountsInScope"], 2, "{p}");
    assert_eq!(p["totals"]["accountsAffected"], 2);
    assert_eq!(p["totals"]["marginCall"], 1);
    assert_eq!(p["totals"]["stopOut"], 0);
    assert_eq!(p["totals"]["marginBeforeUsd"].as_f64(), Some(8001.0));
    // 80 010 / 8 = 10 001.25 each
    assert_eq!(p["totals"]["marginAfterUsd"].as_f64(), Some(20002.5));
    let row = &p["marginCallAccounts"][0];
    assert_eq!(row["login"], L1.to_string());
    assert_eq!(row["marginAfter"].as_f64(), Some(10001.25));
    // nothing changed yet
    assert_eq!(view(&hub, L1).await["margin"].as_f64(), Some(4000.5));

    /* ---------- apply ---------- */
    assert_eq!(code(&api::risk::apply_leverage_cap(State(st.clone()), staff("dealer"), body(reason(crypto.clone(), ""))).await.unwrap_err()), "validation:note");
    let Json(r) = api::risk::apply_leverage_cap(State(st.clone()), staff("risk_manager"), body(reason(crypto.clone(), "house at risk: crypto rally"))).await.unwrap();
    assert_eq!(r["audit"][0]["action"], "risk.leverage_cap");
    assert_eq!(r["data"]["recheck"]["accounts"], 2);
    assert_eq!(r["data"]["recheck"]["marginCalls"], 1);
    let cap_id = r["data"]["cap"]["id"].as_i64().unwrap();
    // the engine's numbers are the preview's, and the margin call was raised at once
    let v1 = view(&hub, L1).await;
    assert_eq!(v1["margin"].as_f64(), row["marginAfter"].as_f64());
    assert_eq!(v1["marginLevel"].as_f64(), row["marginLevelAfter"].as_f64());
    assert_eq!(v1["marginCallFlag"], true);
    assert_eq!(v1["effectiveLeverage"], 100);
    assert_eq!(v1["effectiveLeverageByClass"]["crypto"], 8);
    let alert: Option<String> = sqlx::query_scalar("SELECT level FROM risk_alerts WHERE key = $1 AND resolved_at IS NULL").bind(format!("leverage_cap:{cap_id}")).fetch_optional(&pool).await.unwrap();
    assert_eq!(alert.as_deref(), Some("amber"));

    /* ---------- account-wide cap: the client's leverage choice ---------- */
    let all = reason(json!({"scope": "all", "maxLeverage": 50, "appliesTo": "new_positions"}), "cap everyone at 1:50");
    let _ = api::risk::apply_leverage_cap(State(st.clone()), staff("dealer"), body(all)).await.unwrap();
    let op: Op = Box::new(|tx, env| funds::change_leverage(tx, env, 200, false).map(|_| Value::Null));
    match hub.exec(L3, "client", None, "", "", None, op).await {
        Err(trading::shard::ExecError::Reject(r)) => assert_eq!(r.code, "leverage_capped"),
        other => panic!("expected leverage_capped, got {:?}", other.map(|d| d.value)),
    }
    let v3 = view(&hub, L3).await;
    assert_eq!((v3["effectiveLeverage"].as_u64(), v3["leverageCap"].as_u64()), (Some(50), Some(50)));
    // a fresh load from the database sees both caps
    let reloaded = trading::persist::load_tenant(&pool, 1).await.unwrap();
    assert_eq!(reloaded.risk.leverage_caps.iter().filter(|c| c.enabled).count(), 2);
    let Json(list) = api::risk::leverage_caps(staff("viewer")).await.unwrap();
    assert_eq!(list["caps"].as_array().unwrap().len(), 2);

    /* ---------- disable ---------- */
    assert!(matches!(api::risk::disable_leverage_cap(State(st.clone()), staff("dealer"), Path("424242".into()), body(reason(json!({}), "x"))).await.unwrap_err(), ApiError::NotFound(_)));
    let Json(dr) = api::risk::disable_leverage_cap(State(st.clone()), staff("dealer"), Path(cap_id.to_string()), body(reason(json!({}), "rally over"))).await.unwrap();
    assert_eq!(dr["data"]["cap"]["enabled"], false);
    assert_eq!(dr["audit"][0]["flags"][0], "disabled");
    let v1 = view(&hub, L1).await;
    assert_eq!(v1["margin"].as_f64(), Some(4000.5));
    assert_eq!(v1["marginCallFlag"], false, "the margin call clears at once");
    let open: i64 = sqlx::query_scalar("SELECT count(*) FROM risk_alerts WHERE key = $1 AND resolved_at IS NULL").bind(format!("leverage_cap:{cap_id}")).fetch_one(&pool).await.unwrap();
    assert_eq!(open, 0);
    let audits: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE action = 'risk.leverage_cap'").fetch_one(&pool).await.unwrap();
    assert_eq!(audits, 3);

    // margin call / leverage stamps replay, ledger balanced
    let states = trading::persist::replay_all(&pool).await.unwrap();
    assert!(trading::persist::verify_balances(&pool, &states).await.unwrap().is_empty());
    drop(st);
    drop(hub);
    pool.close().await;
    if let Ok(mut c) = server.database("postgres").connect().await {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut c).await;
    }
}

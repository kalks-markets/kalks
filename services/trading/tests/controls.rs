//! Client controls through the real API handlers and shards (PostgreSQL, like tests/replay.rs): restrictions
//! the Back Office sets in the gateway (served here by a mock gateway) are enforced by the engine — close-only,
//! trading disabled, sign-in blocked (Kalks Trader sessions end, sign-in and SSO refused), copy / PAMM / MAM
//! participation — and a staff session opened as the client from the Back Office is read-only and ends after
//! its time. Kalks Trader connections are reported to the gateway for presence.
//!
//! Skipped with a message when PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use axum::Router;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, extract::Path};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use trading::api::{self, ApiError, AppState, Body, Ctx, LoginAlloc, StaffCtx};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::{Ids, Quote, funds};
use trading::feed::QuoteBook;
use trading::model::{Account, AccountKind, Controls, Mode, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Shared, Stats, Streams};
use trading::specs::Specs;

const USER: i64 = 901;
const LOGIN: i64 = 10_000_301;

#[derive(Clone, Default)]
struct MockGateway {
    restrictions: Arc<Mutex<Value>>,
    reports: Arc<Mutex<Vec<Value>>>,
    /// the module map `/v1/internal/tenants/{slug}` answers (module switches)
    modules: Arc<Mutex<Value>>,
}

async fn gw_tenant(State(g): State<MockGateway>, Path(slug): Path<String>) -> Json<Value> {
    Json(json!({"id": 1, "slug": slug, "name": slug, "status": "active", "modules": g.modules.lock().unwrap().clone()}))
}

async fn gw_restrictions(State(g): State<MockGateway>) -> Json<Value> {
    Json(g.restrictions.lock().unwrap().clone())
}

async fn gw_presence(State(g): State<MockGateway>, Json(v): Json<Value>) -> Json<Value> {
    g.reports.lock().unwrap().push(v);
    Json(json!({"status": "ok"}))
}

fn code(e: &ApiError) -> String {
    match e {
        ApiError::Status { code, .. } => (*code).into(),
        ApiError::Reject { reject, .. } => reject.code.into(),
        ApiError::ReadOnly => "read_only".into(),
        ApiError::Unauthorized => "unauthorized".into(),
        other => format!("{other:?}"),
    }
}

fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

/// Staff identity as the Back Office BFF forwards it (through the real extractor).
async fn staff_ctx(st: &AppState) -> StaffCtx {
    use axum::extract::FromRequestParts;
    let req = axum::http::Request::builder()
        .header("x-kalks-staff-id", "12")
        .header("x-kalks-staff-name", "Maya%20Support")
        .header("x-kalks-staff-role", "compliance")
        .body(())
        .unwrap();
    let (mut parts, _) = req.into_parts();
    StaffCtx::from_request_parts(&mut parts, st).await.unwrap()
}

#[tokio::test]
async fn restrictions_and_staff_sessions_through_the_api() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_controls_{}", std::process::id());
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
    });
    let hub = Hub::start(shared, 2, Default::default());
    let t = Utc::now().timestamp_millis();
    quotes.set("standard", "BTCUSD", Quote { bid: d("80000"), ask: d("80020"), t_ms: t });

    // mock gateway: the restrictions it serves are set by the test; presence reports are collected
    let gw = MockGateway::default();
    *gw.restrictions.lock().unwrap() = json!({"items": []});
    let app = Router::new()
        .route("/v1/internal/restrictions", get(gw_restrictions))
        .route("/v1/internal/presence/trader", post(gw_presence))
        .route("/v1/internal/tenants/{slug}", get(gw_tenant))
        .with_state(gw.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let hash = |p: &str| trading::auth::hash_password(p).unwrap();
    let account = Account {
        tenant_id: 1, login: LOGIN, user_id: USER, kind: AccountKind::Live, group: "standard".into(), mode: Mode::Hedging, cent: false, leverage: 100,
        status: Status::Active, name: "Controls".into(), route_override: None, controls: Controls::default(), demo: None, created_at: Utc::now(), lifecycle: None,
    };
    hub.open(account, (hash("Trade2026x"), hash("Watch2026x")), "test").await.unwrap();
    hub.exec(LOGIN, "test", None, "", "", None, Box::new(|tx, env| funds::transfer(tx, env, funds::Direction::In, d("50000"), "ctl-fund", None).map(|_| Value::Null))).await.unwrap();

    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live.max(10_000_900)), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config {
        bind: String::new(), database_url: url.clone(), internal_token: String::new(), session_secret: "s".repeat(40), dev_mode: true,
        market_data_ws: String::new(), instruments_file: String::new(), specs_file: String::new(), shards: 2, max_quote_age_ms: 0, session_ttl_hours: 12,
        json_logs: false, rollover_enabled: false, wallet_url: String::new(), wallet_token: String::new(), ib_url: String::new(), ib_token: String::new(),
        gateway_url: format!("http://{addr}"), gateway_token: "t".into(),
        options_url: String::new(), options_token: String::new(), options_hedger: false, options_hedge_user: 0,
        options_hedge_group: "standard".into(), options_hedge_capital: 1_000_000, options_hedge_limit_usd: 250_000, options_mm_user: 0, options_mm_capital: 25_000_000,
        eodhd_key: String::new(), eodhd_url: String::new(), market_data_url: String::new(), market_data_admin_token: String::new(),
    };
    let st = AppState {
        hub: hub.clone(), pool: pool.clone(), keys: Keys::new(&cfg.session_secret), cfg: Arc::new(cfg.clone()), limiter: Limiter::default(),
        tickets: StreamTickets::default(), logins, open_lock: Arc::new(tokio::sync::Mutex::new(())), social,
        presence: Arc::new(trading::controls::Presence::default()),
        gateway: Arc::new(trading::controls::Gateway::new(&cfg.gateway_url, &cfg.gateway_token)),
    };
    let tenant = registry.by_slug("kalks").unwrap();
    let ctx = |bearer: Option<&str>| Ctx { tenant: tenant.clone(), ip: "198.51.100.9".into(), user_agent: "it".into(), bearer: bearer.map(str::to_string) };
    let restrict = |kinds: Value| *gw.restrictions.lock().unwrap() = json!({"items": [{"user_id": USER, "tenant_id": 1, "kinds": kinds}]});
    let refresh = || api::controls::refresh(State(st.clone()), body(json!({"userId": USER})));

    // a normal Kalks Trader session
    let Json(s) = api::terminal::login(State(st.clone()), ctx(None), body(json!({"login": LOGIN, "password": "Trade2026x"}))).await.unwrap();
    let tok = s["token"].as_str().unwrap().to_string();
    let order = |v: &str| json!({"symbol": "BTCUSD", "side": "buy", "type": "market", "volume": v});
    let Json(o) = api::terminal::place(State(st.clone()), ctx(Some(&tok)), body(order("0.1"))).await.unwrap();
    let pos = o["positionTicket"].as_i64().unwrap();
    let _ = api::terminal::place(State(st.clone()), ctx(Some(&tok)), body(order("0.1"))).await.unwrap();

    // close-only: a new order is refused with a readable reason, a close still goes through
    restrict(json!([{"kind": "close_only", "expires_at": null}]));
    let Json(r) = refresh().await.unwrap();
    assert_eq!(r["restrictions"], json!(["close_only"]));
    let e = api::terminal::place(State(st.clone()), ctx(Some(&tok)), body(order("0.1"))).await.unwrap_err();
    assert_eq!(code(&e), "close_only");
    let _ = api::terminal::close_position(State(st.clone()), ctx(Some(&tok)), Path(pos.to_string()), None).await.unwrap();
    let Json(state) = api::terminal::state(State(st.clone()), ctx(Some(&tok)), axum::extract::Query(serde_json::from_value(json!({})).unwrap())).await.unwrap();
    assert_eq!(state["restrictions"], json!(["close_only"]));
    assert!(state["staff"].is_null());

    // trading disabled: nothing, not even a close
    restrict(json!([{"kind": "trading", "expires_at": null}]));
    let _ = refresh().await.unwrap();
    assert_eq!(code(&api::terminal::place(State(st.clone()), ctx(Some(&tok)), body(order("0.1"))).await.unwrap_err()), "trading_disabled");
    let other = state["positions"][0]["ticket"].as_i64().unwrap();
    assert_eq!(code(&api::terminal::close_position(State(st.clone()), ctx(Some(&tok)), Path(other.to_string()), None).await.unwrap_err()), "trading_disabled");

    // copy trading / PAMM / MAM participation
    restrict(json!([{"kind": "social", "expires_at": null}]));
    let _ = refresh().await.unwrap();
    let mut h = HeaderMap::new();
    h.insert("x-kalks-user-id", USER.to_string().parse().unwrap());
    let e = api::social::subscribe(State(st.clone()), ctx(None), h.clone(), body(json!({"masterId": 1, "allocation": 100}))).await.unwrap_err();
    assert_eq!(code(&e), "restricted");
    assert_eq!(code(&api::social::invest(State(st.clone()), ctx(None), h.clone(), Path(1), body(json!({"amount": 100}))).await.unwrap_err()), "restricted");

    // sign-in blocked: the open session ends at once, sign-in and the Client Area's Trade button are refused
    restrict(json!([{"kind": "login", "expires_at": null}]));
    let _ = refresh().await.unwrap();
    let revoked: i64 = sqlx::query_scalar("SELECT count(*) FROM terminal_sessions WHERE login = $1 AND revoked_at IS NOT NULL").bind(LOGIN).fetch_one(&pool).await.unwrap();
    assert_eq!(revoked, 1);
    assert!(api::terminal::state(State(st.clone()), ctx(Some(&tok)), axum::extract::Query(serde_json::from_value(json!({})).unwrap())).await.is_err());
    let e = api::terminal::login(State(st.clone()), ctx(None), body(json!({"login": LOGIN, "password": "Trade2026x"}))).await.unwrap_err();
    assert_eq!(code(&e), "account_suspended");
    let e = api::accounts::sso(State(st.clone()), ctx(None), h.clone(), Path(LOGIN), axum::extract::Query(serde_json::from_value(json!({})).unwrap())).await.unwrap_err();
    assert_eq!(code(&e), "account_suspended");
    assert_eq!(code(&api::controls::staff_sso(State(st.clone()), staff_ctx(&st).await, Path(LOGIN), body(json!({"userId": USER}))).await.unwrap_err()), "account_suspended");
    // the periodic reload does the same: lifted in the gateway = allowed again
    restrict(json!([]));
    let _ = refresh().await.unwrap();
    let _ = api::terminal::login(State(st.clone()), ctx(None), body(json!({"login": LOGIN, "password": "Trade2026x"}))).await.unwrap();

    // module switches (gateway, Platform Owner): the broker's map comes from the gateway, a missing key is on
    *gw.modules.lock().unwrap() = json!({"copy_trading": false, "options": true});
    assert!(!trading::modules::on(&st, "qa-modules", "copy_trading").await, "read from the gateway");
    assert!(trading::modules::on(&st, "qa-modules", "options").await && trading::modules::on(&st, "qa-modules", "news").await);
    // new copy / PAMM / MAM participation is refused while its module is off (403 module_disabled)
    let off = json!({"copy_trading": false, "pamm": false, "mam": false}).as_object().unwrap().clone();
    trading::modules::prime("kalks", off);
    let refused = |e: ApiError| matches!(e, ApiError::Status { status: 403, code: "module_disabled", .. });
    assert!(refused(api::social::subscribe(State(st.clone()), ctx(None), h.clone(), body(json!({"masterId": 1, "allocation": 100}))).await.unwrap_err()));
    assert!(refused(api::social::invest(State(st.clone()), ctx(None), h.clone(), Path(1), body(json!({"amount": 100}))).await.unwrap_err()));
    assert!(refused(api::social::create_fund(State(st.clone()), ctx(None), h.clone(), body(json!({}))).await.unwrap_err()));
    assert!(refused(api::mam::create_link(State(st.clone()), ctx(None), h.clone(), body(json!({}))).await.unwrap_err()));
    assert!(refused(api::mam::create_manager(State(st.clone()), ctx(None), h.clone(), body(json!({}))).await.unwrap_err()));
    // on again: the request gets past the switch (and fails on its own merits)
    trading::modules::prime("kalks", serde_json::Map::new());
    assert!(!refused(api::social::subscribe(State(st.clone()), ctx(None), h.clone(), body(json!({"masterId": 1, "allocation": 100}))).await.unwrap_err()));

    // a staff session from the Back Office: the account must be the client's, read-only by default, 30 minutes
    let e = api::controls::staff_sso(State(st.clone()), staff_ctx(&st).await, Path(LOGIN), body(json!({"userId": 12345}))).await.unwrap_err();
    assert!(matches!(e, ApiError::NotFound(_)));
    let Json(link) = api::controls::staff_sso(State(st.clone()), staff_ctx(&st).await, Path(LOGIN), body(json!({"userId": USER, "readOnly": true, "minutes": 30}))).await.unwrap();
    let Json(ss) = api::terminal::sso(State(st.clone()), ctx(None), body(json!({"token": link["token"]}))).await.unwrap();
    assert_eq!(ss["readOnly"], true);
    assert_eq!(ss["staff"]["name"], "Maya Support");
    let exp = chrono::DateTime::parse_from_rfc3339(ss["expiresAt"].as_str().unwrap()).unwrap().with_timezone(&Utc);
    assert!(exp <= Utc::now() + chrono::Duration::minutes(30) && exp > Utc::now() + chrono::Duration::minutes(29));
    let stok = ss["token"].as_str().unwrap().to_string();
    // every write is refused on the server
    assert_eq!(code(&api::terminal::place(State(st.clone()), ctx(Some(&stok)), body(order("0.1"))).await.unwrap_err()), "read_only");
    assert_eq!(code(&api::terminal::close_position(State(st.clone()), ctx(Some(&stok)), Path(other.to_string()), None).await.unwrap_err()), "read_only");
    let Json(c) = api::controls::terminal_controls(State(st.clone()), ctx(Some(&stok))).await.unwrap();
    assert_eq!((c["readOnly"].as_bool(), c["staff"]["id"].as_i64()), (Some(true), Some(12)));
    // one-time link
    assert!(api::terminal::sso(State(st.clone()), ctx(None), body(json!({"token": link["token"]}))).await.is_err());
    // the session ends on time
    sqlx::query("UPDATE terminal_sessions SET expires_at = now() - interval '1 second' WHERE staff_id = 12").execute(&pool).await.unwrap();
    assert_eq!(code(&api::controls::terminal_controls(State(st.clone()), ctx(Some(&stok))).await.unwrap_err()), "unauthorized");
    // full access (Super Admin, checked by the Back Office): trades are recorded as the staff member
    let Json(link) = api::controls::staff_sso(State(st.clone()), staff_ctx(&st).await, Path(LOGIN), body(json!({"userId": USER, "readOnly": false}))).await.unwrap();
    let Json(ss) = api::terminal::sso(State(st.clone()), ctx(None), body(json!({"token": link["token"]}))).await.unwrap();
    let ftok = ss["token"].as_str().unwrap().to_string();
    let _ = api::terminal::place(State(st.clone()), ctx(Some(&ftok)), body(order("0.1"))).await.unwrap();
    let actor: String = sqlx::query_scalar("SELECT actor FROM events WHERE login = $1 ORDER BY version DESC LIMIT 1").bind(LOGIN).fetch_one(&pool).await.unwrap();
    assert_eq!(actor, "staff:12");

    // presence: Kalks Trader connections are reported to the gateway (staff sessions never are)
    trading::controls::spawn(hub.clone(), pool.clone(), st.gateway.clone(), st.presence.clone());
    let guard = st.presence.register(trading::controls::Conn { user_id: USER, login: LOGIN, ip: Some("198.51.100.9".into()), country: Some("in".into()), user_agent: None, since: Utc::now() });
    let seen = |pred: &dyn Fn(&Value) -> bool| gw.reports.lock().unwrap().iter().any(pred);
    for _ in 0..40 {
        if seen(&|r| r["items"].as_array().is_some_and(|i| i.iter().any(|c| c["user_id"] == USER))) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(seen(&|r| r["items"][0]["login"] == LOGIN), "connection reported");
    drop(guard);
    for _ in 0..40 {
        if seen(&|r| r["ended"].as_array().is_some_and(|e| !e.is_empty())) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(seen(&|r| r["ended"].as_array().is_some_and(|e| !e.is_empty())), "closed connection reported");

    pool.close().await;
    if let Ok(mut c) = server.database("postgres").connect().await {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut c).await;
    }
}

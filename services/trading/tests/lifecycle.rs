//! Account lifecycle through the Client Area handlers (api/lifecycle.rs) against PostgreSQL: archive is refused
//! with open positions unless `empty`; a demo with a position is emptied and archived; archived accounts don't
//! count towards the account limit; restore brings the prior status back; rename; replaying the `events` table
//! after all of it gives the same state.
//!
//! Skipped with a message when PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::HeaderMap;
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};

use trading::api::{AppState, Body, Ctx, LoginAlloc, accounts, lifecycle};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::trade::{self, OrderReq};
use trading::engine::{Ids, Quote};
use trading::feed::QuoteBook;
use trading::model::{Side, Status};
use trading::money::D;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Op, Shared, Stats, Streams};
use trading::specs::Specs;

const USER: i64 = 4242;

fn headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("x-kalks-user-id", USER.to_string().parse().unwrap());
    h
}

fn q<T: serde::de::DeserializeOwned>() -> Query<T> {
    Query(serde_json::from_value(json!({})).unwrap())
}

fn body<T: serde::de::DeserializeOwned>(v: Value) -> Body<T> {
    Body(serde_json::from_value(v).unwrap())
}

async fn ctx(st: &AppState) -> Ctx {
    let req = axum::http::Request::builder().header("x-kalks-tenant", "kalks").body(()).unwrap();
    let (mut parts, _) = req.into_parts();
    Ctx::from_request_parts(&mut parts, st).await.unwrap_or_else(|_| panic!("tenant"))
}

async fn open_demo(st: &AppState, group: &str) -> Result<i64, trading::api::ApiError> {
    let body = serde_json::from_value(json!({"type": "demo", "group": group, "password": "Passw0rd!x1", "investorPassword": "Inv3stor!x2"})).unwrap();
    let r = accounts::open(State(st.clone()), ctx(st).await, headers(), Body(body)).await?;
    Ok(r.0["account"]["login"].as_i64().unwrap())
}

async fn snapshot(hub: &Hub, login: i64) -> Value {
    hub.read(login, Box::new(|x| x.map(|(s, _)| serde_json::to_value(s).unwrap()).unwrap_or(Value::Null))).await
}

#[test]
fn account_json_without_lifecycle_still_reads() {
    let old = json!({
        "tenant_id": 1, "login": 5, "user_id": 7, "kind": "demo", "group": "standard", "mode": "hedging", "cent": false,
        "leverage": 100, "status": "active", "name": "", "route_override": null,
        "controls": {"trading_disabled": false, "close_only": false, "max_lot": null, "exec_delay_ms": 0, "markup_pips": "0", "reason": "", "set_by": "", "updated": null},
        "demo": null, "created_at": "2026-01-01T00:00:00Z"
    });
    let a: trading::model::Account = serde_json::from_value(old).unwrap();
    assert!(a.lifecycle.is_none());
    // and it is not written back when absent (old readers see the same JSON)
    assert!(serde_json::to_value(&a).unwrap().get("lifecycle").is_none());
    assert_eq!(Status::parse("archived"), Some(Status::Archived));
    assert!(Status::Closed.is_retired() && !Status::Expired.is_retired());
}

#[tokio::test]
async fn archive_restore_rename_and_replay() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_lifecycle_{}", std::process::id());
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
    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config {
        bind: String::new(), database_url: url.clone(), internal_token: String::new(), session_secret: "s".repeat(40), dev_mode: true,
        market_data_ws: String::new(), instruments_file: String::new(), specs_file: String::new(), shards: 1, max_quote_age_ms: 0, session_ttl_hours: 12,
        json_logs: false, rollover_enabled: false, wallet_url: String::new(), wallet_token: String::new(), ib_url: String::new(), ib_token: String::new(),
        gateway_url: String::new(), gateway_token: String::new(),
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
    let c = ctx(&st).await;
    let g = c.tenant.groups.values().filter(|g| g.enabled && g.allows("demo") && !g.code.starts_with("prop")).min_by_key(|g| g.code.clone()).expect("a demo group").clone();
    quotes.set(&g.spread_group, "BTCUSD", Quote { bid: D::from(80_000), ask: D::from(80_020), t_ms: Utc::now().timestamp_millis() });

    // fill the account limit of the group
    let mut mine = Vec::new();
    for _ in 0..g.max_accounts_per_user {
        mine.push(open_demo(&st, &g.code).await.unwrap_or_else(|e| panic!("open: {e:?}")));
    }
    assert!(open_demo(&st, &g.code).await.is_err(), "limit reached");
    // CFD / Options account split: the limit counts per (kind, product). Every other CFD group is full now too, an
    // Options account still opens (and the Options limit counts only Options accounts)
    assert_eq!(c.tenant.groups[&g.code].product, trading::rules::Product::Cfd);
    let other_cfd = c.tenant.groups.values().filter(|x| x.enabled && x.allows("demo") && x.product == trading::rules::Product::Cfd && x.code != g.code && !x.code.starts_with("prop")).min_by_key(|x| x.code.clone()).expect("another CFD group").clone();
    let e = open_demo(&st, &other_cfd.code).await.unwrap_err();
    assert!(matches!(&e, trading::api::ApiError::Conflict { code: "account_limit", message } if message.contains("CFD")), "{e:?}");
    let opt = open_demo(&st, "options-standard").await.expect("an Options account opens beside full CFD groups");
    let listed = accounts::list(State(st.clone()), ctx(&st).await, headers(), q()).await.unwrap().0;
    let product_of = |l: i64| listed["accounts"].as_array().unwrap().iter().find(|x| x["login"] == json!(l)).map(|x| x["product"].clone());
    assert_eq!((product_of(opt), product_of(mine[0])), (Some(json!("options")), Some(json!("cfd"))));
    mine.push(opt);
    // the product of a group with accounts is locked; an older Back Office that omits it never flips the group
    let mut gj = serde_json::to_value(&c.tenant.groups["options-standard"]).unwrap();
    gj["reasonCode"] = json!("GRP-01");
    gj["note"] = json!("it");
    gj["product"] = json!("cfd");
    let e = trading::api::admin::update_group(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path("options-standard".into()), body(gj.clone())).await.unwrap_err();
    assert_eq!(err_code(e), "field:product");
    gj.as_object_mut().unwrap().remove("product");
    gj["name"] = json!("Options Standard ·");
    let r = trading::api::admin::update_group(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path("options-standard".into()), body(gj)).await.unwrap().0;
    assert_eq!((r["data"]["product"].as_str(), r["data"]["name"].as_str()), (Some("options"), Some("Options Standard ·")), "{r}");
    let a = mine[0];

    // a position: archive without `empty` is refused, the check says why
    let op: Op = Box::new(|tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, D::from_str("0.01").unwrap())).map(|_| Value::Null));
    hub.exec(a, "test", None, "", "", None, op).await.unwrap();
    let chk = lifecycle::archive_check(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.unwrap().0;
    assert_eq!((chk["positions"].as_u64(), chk["needsEmpty"].as_bool(), chk["canArchive"].as_bool()), (Some(1), Some(true), Some(true)), "{chk}");
    let refused = lifecycle::archive(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"empty": false}))).await;
    assert!(refused.is_err());
    assert_eq!(hub.meta(a).unwrap().status, Status::Active);

    // someone else's account is a 404
    let mut other = HeaderMap::new();
    other.insert("x-kalks-user-id", "999".parse().unwrap());
    assert!(lifecycle::archive_check(State(st.clone()), ctx(&st).await, other, Path(a), q()).await.is_err());

    // empty = true: the position is closed and the demo archived
    let done = lifecycle::archive(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"empty": true}))).await.unwrap().0;
    assert_eq!(done["ok"], json!(true), "{done}");
    assert_eq!(done["steps"].as_array().unwrap().iter().map(|s| s["step"].as_str().unwrap()).collect::<Vec<_>>(), vec!["close_positions", "archive"]);
    let snap = snapshot(&hub, a).await;
    assert!(snap["positions"].as_object().unwrap().is_empty());
    assert_eq!(snap["account"]["status"], json!("archived"));
    assert_eq!(snap["account"]["lifecycle"]["prior_status"], json!("active"));
    assert_eq!(snap["account"]["lifecycle"]["client_restorable"], json!(true));
    // idempotent
    let again = lifecycle::archive(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({}))).await.unwrap().0;
    assert_eq!((again["ok"].as_bool(), again["steps"].as_array().map(Vec::len)), (Some(true), Some(0)));
    // retired accounts can't get a terminal token
    assert!(accounts::sso(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.is_err());

    // the archived account no longer counts: one more can be opened, and then restore is over the limit
    let extra = open_demo(&st, &g.code).await.expect("archived accounts don't count");
    assert!(lifecycle::restore(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.is_err(), "restore re-checks the limit");
    let op: Op = Box::new(|tx, env| trading::engine::funds::archive(tx, env, "test", "TEST", true).map(|_| Value::Null));
    hub.exec(extra, "test", None, "", "", None, op).await.unwrap();
    let r = lifecycle::restore(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.unwrap().0;
    assert_eq!(r, json!({"ok": true, "status": "active"}));
    assert!(snapshot(&hub, a).await["account"].get("lifecycle").is_none());

    // rename
    let r = lifecycle::rename(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"name": "  Swing book "}))).await.unwrap().0;
    assert_eq!(r, json!({"ok": true}));
    assert_eq!(hub.meta(a).unwrap().name, "Swing book");
    assert!(lifecycle::rename(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"name": "x".repeat(33)}))).await.is_err());

    // replay = live
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for l in mine.iter().chain([&extra]) {
        assert_eq!(serde_json::to_value(&replayed[l]).unwrap(), snapshot(&hub, *l).await, "replay diverged for {l}");
    }
    assert_eq!(replayed[&extra].account.status, Status::Archived);
}

/* ------------------------------------------------------------------ */
/* Close permanently, the closure queue, account jobs (M2 / M7)        */
/* ------------------------------------------------------------------ */

async fn setup(tag: &str) -> Option<(AppState, Arc<QuoteBook>, sqlx::PgPool)> {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_{tag}_{}", std::process::id());
    let server = PgConnectOptions::from_str(&base).ok()?;
    if server.clone().database("postgres").connect().await.is_err() {
        eprintln!("SKIP: PostgreSQL not reachable at {base}");
        return None;
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
    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config {
        bind: String::new(), database_url: url.clone(), internal_token: String::new(), session_secret: "s".repeat(40), dev_mode: true,
        market_data_ws: String::new(), instruments_file: String::new(), specs_file: String::new(), shards: 1, max_quote_age_ms: 0, session_ttl_hours: 12,
        json_logs: false, rollover_enabled: false, wallet_url: String::new(), wallet_token: String::new(), ib_url: String::new(), ib_token: String::new(),
        gateway_url: String::new(), gateway_token: String::new(),
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
    Some((st, quotes, pool))
}

fn staff(st: &AppState, c: Ctx, id: &str, role: &str) -> trading::api::StaffCtx {
    let _ = st;
    trading::api::StaffCtx { ctx: c, staff: trading::shard::Staff { id: id.into(), name: format!("Staff {id}"), role: role.into() }, perms: None }
}

async fn open_live(st: &AppState, group: &str) -> i64 {
    let body = serde_json::from_value(json!({"type": "live", "group": group, "password": "Passw0rd!x1", "investorPassword": "Inv3stor!x2"})).unwrap();
    let r = accounts::open(State(st.clone()), ctx(st).await, headers(), Body(body)).await.unwrap_or_else(|e| panic!("open live: {e:?}"));
    r.0["account"]["login"].as_i64().unwrap()
}

fn err_code(e: trading::api::ApiError) -> String {
    match e {
        trading::api::ApiError::Conflict { code, .. } => code.into(),
        trading::api::ApiError::Forbidden(_) => "forbidden".into(),
        trading::api::ApiError::Validation { field, .. } => format!("field:{field}"),
        trading::api::ApiError::StatusData { code, .. } => code.into(),
        trading::api::ApiError::Reject { reject, .. } => reject.code.into(),
        other => format!("{other:?}"),
    }
}

#[tokio::test]
async fn closure_queue_four_eyes_reopen_and_jobs() {
    use trading::api::closures;
    let Some((st, _quotes, pool)) = setup("closures").await else { return };
    let hub = st.hub.clone();
    let c = ctx(&st).await;
    let live_g = c.tenant.groups.values().filter(|g| g.enabled && g.allows("live") && !g.cent && !g.code.starts_with("prop") && !["copy", "copy-netting", "pamm", "mam"].contains(&g.code.as_str())).min_by_key(|g| g.code.clone()).expect("a live group").clone();
    let a = open_live(&st, &live_g.code).await;

    // policy: four-eyes above 10 USD
    let admin = staff(&st, ctx(&st).await, "1", "admin");
    let _ = closures::put_policy(State(st.clone()), admin, body(json!({"demoArchiveDays": 30, "dormantDays": 180, "dormantAutoArchive": true, "closeFourEyesUsd": 10, "retentionYears": 7, "reasonCode": "ACC-01", "note": "test"}))).await.unwrap();

    // 50 USD credit: the request needs two approvers; the credit is forfeited on closure
    let op: Op = Box::new(|tx, env| trading::engine::funds::adjust(tx, env, trading::engine::funds::AdjustKind::Credit, D::from(50), "t-credit", "TEST", "credit").map(|_| Value::Null));
    hub.exec(a, "test", None, "", "", None, op).await.unwrap();

    let s0 = lifecycle::closure_status(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.unwrap().0;
    assert_eq!((s0["canRequest"].as_bool(), s0["request"].is_null()), (Some(true), true), "{s0}");
    // bad survey reason, missing forfeit confirmation
    let e = lifecycle::request_closure(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"reasonCode": "nope"}))).await.unwrap_err();
    assert_eq!(err_code(e), "field:reasonCode");
    let e = lifecycle::request_closure(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"reasonCode": "costs"}))).await.unwrap_err();
    assert_eq!(err_code(e), "field:ackForfeit");
    let r = lifecycle::request_closure(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"reasonCode": "costs", "survey": {"reasons": ["platform", "bogus"], "comment": "Spreads too wide"}, "ackForfeit": true}))).await.unwrap().0;
    assert_eq!((r["ok"].as_bool(), r["request"]["status"].as_str()), (Some(true), Some("pending")), "{r}");
    // one open request at a time
    let e = lifecycle::request_closure(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"reasonCode": "costs", "ackForfeit": true}))).await.unwrap_err();
    assert_eq!(err_code(e), "request_pending");
    let s1 = lifecycle::closure_status(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.unwrap().0;
    assert_eq!(s1["canRequest"], json!(false));

    let list = closures::list(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Query(serde_json::from_value(json!({"status": "pending"})).unwrap())).await.unwrap().0;
    assert_eq!(list["total"], json!(1), "{list}");
    let item = &list["items"][0];
    assert_eq!((item["fourEyes"].as_bool(), item["survey"]["reasons"].clone()), (Some(true), json!(["costs", "platform"])));
    let id = item["id"].as_i64().unwrap();
    // live checks: everything passes (flat, zero balance, wallet skipped in dev, no links, no hold)
    let d = closures::detail(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(id)).await.unwrap().0;
    assert_eq!(d["data"]["checksPassed"], json!(true), "{d}");

    // dealers can't approve; four-eyes: the first approver can't also be the second
    let e = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "9", "dealer"), Path(id), body(json!({}))).await.unwrap_err();
    assert_eq!(err_code(e), "forbidden");
    let first = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(id), body(json!({"note": "ok"}))).await.unwrap().0;
    assert_eq!(first["stage"], json!("first_approval"));
    assert_eq!(hub.meta(a).unwrap().status, Status::Active);
    let e = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(id), body(json!({}))).await.unwrap_err();
    assert_eq!(err_code(e), "forbidden");
    let done = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(id), body(json!({"note": "second"}))).await.unwrap().0;
    assert_eq!((done["stage"].as_str(), done["data"]["status"].as_str(), done["data"]["clientReason"].as_str()), (Some("done"), Some("approved"), Some("closed_as_requested")), "{done}");
    let snap = snapshot(&hub, a).await;
    assert_eq!(snap["account"]["status"], json!("closed"));
    assert!(snap["account"]["lifecycle"]["closed_at"].is_string());
    // decimals are JSON strings, or numbers when another workspace member turns on rust_decimal's serde-float
    let credit: D = snap["credit"].as_str().map(|s| s.parse().unwrap()).or_else(|| snap["credit"].as_f64().map(|f| D::try_from(f).unwrap())).unwrap();
    assert_eq!(credit, D::ZERO, "credit forfeited");
    // closed is final for the client
    assert!(lifecycle::restore(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.is_err());
    assert!(lifecycle::archive(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({}))).await.is_err());
    let listed = accounts::list(State(st.clone()), ctx(&st).await, headers(), q()).await.unwrap().0;
    let row = listed["accounts"].as_array().unwrap().iter().find(|x| x["login"] == json!(a)).unwrap().clone();
    assert!(row["closedAt"].is_string() && row["closureRequest"]["status"] == json!("approved"), "{row}");

    // the report counts it
    let rep = closures::report(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Query(serde_json::from_value(json!({})).unwrap())).await.unwrap().0;
    assert_eq!((rep["data"]["total"].as_i64(), rep["data"]["byReason"][0]["reason"].as_str()), (Some(1), Some("costs")), "{rep}");

    // reopen: Super Admin only, always four-eyes
    let e = closures::reopen_request(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(a), body(json!({"reasonCode": "RST-02", "note": "closed in error"}))).await.unwrap_err();
    assert_eq!(err_code(e), "forbidden");
    let rq = closures::reopen_request(State(st.clone()), staff(&st, ctx(&st).await, "100", "super_admin"), Path(a), body(json!({"reasonCode": "RST-02", "note": "closed in error"}))).await.unwrap().0;
    let rid = rq["data"]["id"].as_i64().unwrap();
    let e = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "100", "super_admin"), Path(rid), body(json!({}))).await.unwrap_err();
    assert_eq!(err_code(e), "forbidden");
    let e = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(rid), body(json!({}))).await.unwrap_err();
    assert_eq!(err_code(e), "forbidden");
    let first = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "101", "super_admin"), Path(rid), body(json!({}))).await.unwrap().0;
    assert_eq!(first["stage"], json!("first_approval"));
    let done = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "102", "platform_owner"), Path(rid), body(json!({}))).await.unwrap().0;
    assert_eq!(done["stage"], json!("done"), "{done}");
    assert_eq!(hub.meta(a).unwrap().status, Status::Active);

    // a staff request, rejected with a client-facing template (the "other" template needs a message)
    let e = closures::staff_request(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(a), body(json!({"reasonCode": "ACC-02", "note": ""}))).await.unwrap_err();
    assert_eq!(err_code(e), "field:note");
    let sr = closures::staff_request(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(a), body(json!({"reasonCode": "ACC-02", "note": "duplicate profile"}))).await.unwrap().0;
    let sid = sr["data"]["id"].as_i64().unwrap();
    assert_eq!(sr["data"]["fourEyes"], json!(false));
    let e = closures::reject(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(sid), body(json!({"clientReason": "other"}))).await.unwrap_err();
    assert_eq!(err_code(e), "field:clientMessage");
    let rj = closures::reject(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(sid), body(json!({"clientReason": "compliance_review", "note": "hold"}))).await.unwrap().0;
    assert_eq!(rj["data"]["status"], json!("rejected"));
    assert!(closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(sid), body(json!({}))).await.is_err(), "decided requests stay decided");

    // a closure is refused while the account holds a balance
    let op: Op = Box::new(|tx, env| trading::engine::funds::transfer(tx, env, trading::engine::funds::Direction::In, D::from(25), "t-in", None).map(|_| Value::Null));
    hub.exec(a, "test", None, "", "", None, op).await.unwrap();
    let sr = closures::staff_request(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(a), body(json!({"reasonCode": "ACC-02", "note": "client asked by phone"}))).await.unwrap().0;
    let sid = sr["data"]["id"].as_i64().unwrap();
    let e = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(sid), body(json!({}))).await.unwrap_err();
    assert_eq!(err_code(e), "checks_failed");
    let _ = closures::reject(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(sid), body(json!({"clientReason": "balance_remaining"}))).await.unwrap();

    // demo accounts are archived, not closed
    let demo_g = c.tenant.groups.values().filter(|g| g.enabled && g.allows("demo") && !g.cent && !g.code.starts_with("prop")).min_by_key(|g| g.code.clone()).unwrap().clone();
    let d1 = open_demo(&st, &demo_g.code).await.unwrap();
    let e = lifecycle::request_closure(State(st.clone()), ctx(&st).await, headers(), Path(d1), q(), body(json!({"reasonCode": "other"}))).await.unwrap_err();
    assert_eq!(err_code(e), "demo_account");

    // B9: default star, self-service type change, B10: demo balance, health
    let p = lifecycle::put_prefs(State(st.clone()), ctx(&st).await, headers(), q(), body(json!({"defaultLogin": a}))).await.unwrap().0;
    assert_eq!(p["defaultLogin"], json!(a));
    let listed = accounts::list(State(st.clone()), ctx(&st).await, headers(), q()).await.unwrap().0;
    assert!(listed["accounts"].as_array().unwrap().iter().any(|x| x["login"] == json!(a) && x["isDefault"] == json!(true)));
    let opts = lifecycle::group_options(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.unwrap().0;
    assert!(opts["groups"].as_array().unwrap().iter().all(|g| !g["code"].as_str().unwrap().starts_with("prop")), "{opts}");
    // a CFD account changes type within CFD groups only (CFD / Options account split)
    assert!(!opts["groups"].as_array().unwrap().is_empty() && opts["groups"].as_array().unwrap().iter().all(|g| !g["code"].as_str().unwrap().starts_with("options-")), "{opts}");
    let e = lifecycle::change_group(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"group": "options-standard"}))).await.unwrap_err();
    assert_eq!(err_code(e), "product_mismatch");
    let r = lifecycle::demo_balance(State(st.clone()), ctx(&st).await, headers(), Path(d1), q(), body(json!({"amount": 2500}))).await.unwrap().0;
    assert_eq!(r["balance"], json!(2500.0), "{r}");
    assert!(lifecycle::demo_balance(State(st.clone()), ctx(&st).await, headers(), Path(d1), q(), body(json!({"amount": 5}))).await.is_err());
    let h = lifecycle::health(State(st.clone()), ctx(&st).await, headers(), Path(a), q()).await.unwrap().0;
    assert!(h["score"].as_i64().unwrap() > 0 && h["items"].as_array().unwrap().len() == 5, "{h}");

    // jobs: an expired demo past the archive window is archived; an empty dormant live account is flagged and archived
    let op: Op = Box::new(|tx, _| trading::engine::funds::set_status(tx, Status::Expired).map(|_| Value::Null));
    hub.exec(d1, "test", None, "", "", None, op).await.unwrap();
    sqlx::query("UPDATE accounts SET updated_at = now() - interval '40 days' WHERE login = $1").bind(d1).execute(&pool).await.unwrap();
    let b = open_live(&st, &live_g.code).await;
    sqlx::query("UPDATE accounts SET last_activity_at = now() - interval '200 days' WHERE login = $1").bind(b).execute(&pool).await.unwrap();
    let rep = lifecycle::run_account_jobs(&st).await.unwrap();
    assert_eq!((rep.demos_archived, rep.dormant_flagged, rep.dormant_archived), (1, 1, 1), "{rep:?}");
    assert_eq!(hub.meta(d1).unwrap().status, Status::Archived);
    assert_eq!(hub.meta(b).unwrap().status, Status::Archived);
    // the funded account a (activity: a transfer today) is not dormant
    assert_eq!(hub.meta(a).unwrap().status, Status::Active);

    // retention: a closed account is anonymised after the retention period
    let op: Op = Box::new(|tx, env| trading::engine::funds::transfer(tx, env, trading::engine::funds::Direction::Out, D::from(25), "t-out", None).map(|_| Value::Null));
    hub.exec(a, "test", None, "", "", None, op).await.unwrap();
    let _ = lifecycle::rename(State(st.clone()), ctx(&st).await, headers(), Path(a), q(), body(json!({"name": "Old book"}))).await.unwrap();
    let sr = closures::staff_request(State(st.clone()), staff(&st, ctx(&st).await, "7", "compliance"), Path(a), body(json!({"reasonCode": "ACC-01", "note": "client asked"}))).await.unwrap().0;
    let sid = sr["data"]["id"].as_i64().unwrap();
    let _ = closures::approve(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), Path(sid), body(json!({}))).await.unwrap();
    assert_eq!(hub.meta(a).unwrap().status, Status::Closed);
    sqlx::query("UPDATE account_closures SET decided_at = now() - interval '8 years' WHERE id = $1").bind(sid).execute(&pool).await.unwrap();
    let rep = lifecycle::run_account_jobs(&st).await.unwrap();
    assert_eq!(rep.anonymised, 1, "{rep:?}");
    assert_eq!(hub.meta(a).unwrap().name, "");
    assert_eq!(lifecycle::run_account_jobs(&st).await.unwrap().anonymised, 0, "once");

    // bulk dry run lists nothing left to archive; replay = live
    let bulk = closures::bulk(State(st.clone()), staff(&st, ctx(&st).await, "8", "admin"), body(json!({"action": "archive", "target": "expired_demos", "dryRun": true}))).await.unwrap().0;
    assert_eq!(bulk["data"]["count"], json!(0));
    let replayed = trading::persist::replay_all(&pool).await.unwrap();
    for l in [a, b, d1] {
        assert_eq!(serde_json::to_value(&replayed[&l]).unwrap(), snapshot(&hub, l).await, "replay diverged for {l}");
    }
}

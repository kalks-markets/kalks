//! A broker the Platform Owner creates (gateway, D110) is provisioned in the engine on its first request: through
//! the real tenant extractor, a request for an unknown slug asks the gateway (a mock here), creates the tenant
//! with the gateway's id, the default dealing policy and the platform broker's groups, and serves it; a slug the
//! gateway doesn't know is refused and not asked again right away.
//!
//! Skipped with a message when PostgreSQL is not reachable (TRADING_TEST_DATABASE_URL, default :5433).

use axum::Router;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, response::IntoResponse};
use serde_json::json;
use sqlx::ConnectOptions;
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use trading::api::{AppState, Ctx, LoginAlloc};
use trading::auth::{Keys, Limiter, StreamTickets};
use trading::config::Config;
use trading::engine::Ids;
use trading::feed::QuoteBook;
use trading::rules::Registry;
use trading::shard::{Hub, Index, NullLp, Shared, Stats, Streams};
use trading::specs::Specs;

const NEW_ID: i64 = 77_001;

#[derive(Clone, Default)]
struct MockGateway {
    lookups: Arc<AtomicUsize>,
}

async fn gw_tenant(State(g): State<MockGateway>, Path(slug): Path<String>) -> axum::response::Response {
    g.lookups.fetch_add(1, Ordering::SeqCst);
    if slug == "qa-northwind" {
        Json(json!({"id": NEW_ID, "slug": slug, "name": "QA Northwind", "status": "active"})).into_response()
    } else {
        (StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found"}}))).into_response()
    }
}

async fn ctx_for(st: &AppState, slug: &str) -> Result<Ctx, trading::api::ApiError> {
    let req = axum::http::Request::builder().header("x-kalks-tenant", slug).body(()).unwrap();
    let (mut parts, _) = req.into_parts();
    Ctx::from_request_parts(&mut parts, st).await
}

#[tokio::test]
async fn a_new_broker_is_provisioned_on_its_first_request() {
    let base = std::env::var("TRADING_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
    let db = format!("kalks_trading_tenants_{}", std::process::id());
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
    let raw_quotes = Arc::new(QuoteBook::default());
    let shared = Arc::new(Shared {
        pool: pool.clone(),
        registry: registry.clone(),
        specs: specs.into(),
        held: Default::default(),
        quotes: raw_quotes.clone(),
        ids: Arc::new(Ids::new(ticket, deal, txn)),
        index: Arc::new(RwLock::new(Index::default())),
        streams: Streams::default(),
        stats: Arc::new(Stats::default()),
        lp: Arc::new(NullLp),
        max_quote_age_ms: 0,
        restrictions: Default::default(),
        options: Arc::new(trading::options::OptionsCtx::disabled(raw_quotes.clone())),
        clock: Default::default(),
        books: Default::default(),
        corp: Default::default(),
    });
    let hub = Hub::start(shared, 1, Default::default());

    let gw = MockGateway::default();
    let app = Router::new().route("/v1/internal/tenants/{slug}", get(gw_tenant)).with_state(gw.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

    let logins = Arc::new(LoginAlloc { live: AtomicI64::new(live), demo: AtomicI64::new(demo) });
    let social = trading::social::Social::new(pool.clone(), hub.clone(), trading::social::wallet::WalletClient::new("", ""), logins.clone()).await.unwrap();
    let cfg = Config {
        bind: String::new(), database_url: url.clone(), internal_token: String::new(), session_secret: "s".repeat(40), dev_mode: true,
        market_data_ws: String::new(), instruments_file: String::new(), specs_file: String::new(), shards: 1, max_quote_age_ms: 0, session_ttl_hours: 12,
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

    // the platform broker is served from memory, no lookup
    assert_eq!(ctx_for(&st, "kalks").await.unwrap().tenant.tenant_id, 1);
    assert_eq!(gw.lookups.load(Ordering::SeqCst), 0);

    // a broker the gateway knows: provisioned with the gateway's id and the platform broker's groups
    let template = registry.get(1).unwrap().groups.len();
    let ctx = ctx_for(&st, "qa-northwind").await.unwrap();
    assert_eq!((ctx.tenant.tenant_id, ctx.tenant.slug.as_str()), (NEW_ID, "qa-northwind"));
    assert_eq!(ctx.tenant.groups.len(), template);
    assert!(ctx.tenant.groups.contains_key("standard"));
    // CFD / Options account split (migration 20261022120000): every existing group is a CFD group, the market
    // maker's an Options group, Options Standard (enabled) and Options Pro (disabled) are seeded, and a new broker
    // copies the products with its groups
    use trading::rules::Product;
    let platform = registry.get(1).unwrap();
    for (code, product, enabled) in [("standard", Product::Cfd, true), ("ecn", Product::Cfd, true), ("copy", Product::Cfd, false), ("options-mm", Product::Options, false), ("options-standard", Product::Options, true), ("options-pro", Product::Options, false)] {
        let g = &platform.groups[code];
        assert_eq!((g.product, g.enabled), (product, enabled), "{code}");
        assert_eq!(ctx.tenant.groups[code].product, product, "provisioned {code}");
    }
    assert!(platform.groups.values().filter(|g| g.product == Product::Options).all(|g| g.code.starts_with("options-")));
    // its groups price from its own spread groups, never from the platform broker's markups
    assert_eq!(ctx.tenant.groups["standard"].spread_group, "qa-northwind-standard");
    assert!(ctx.tenant.groups.values().all(|g| g.spread_group.starts_with("qa-northwind-")));
    let policy: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_policies WHERE tenant_id = $1").bind(NEW_ID).fetch_one(&pool).await.unwrap();
    assert_eq!(policy, 1);
    // then it's in memory: no second lookup
    let _ = ctx_for(&st, "qa-northwind").await.unwrap();
    assert_eq!(gw.lookups.load(Ordering::SeqCst), 1);

    // a slug the gateway doesn't know is refused, and not looked up again within the miss window
    assert!(ctx_for(&st, "qa-nobody").await.is_err());
    assert!(ctx_for(&st, "qa-nobody").await.is_err());
    assert_eq!(gw.lookups.load(Ordering::SeqCst), 2);
    // a malformed slug never reaches the gateway
    assert!(ctx_for(&st, "Bad Slug").await.is_err());
    assert_eq!(gw.lookups.load(Ordering::SeqCst), 2);

    // the broker's Back Office can't point a group at the platform broker's spread group (its markups)
    let staff = {
        let req = axum::http::Request::builder()
            .header("x-kalks-tenant", "qa-northwind")
            .header("x-kalks-staff-id", "3")
            .header("x-kalks-staff-name", "Owner")
            .header("x-kalks-staff-role", "super_admin")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        trading::api::StaffCtx::from_request_parts(&mut parts, &st).await.unwrap()
    };
    let mut g = serde_json::to_value(&ctx.tenant.groups["standard"]).unwrap();
    g["spreadGroup"] = json!("standard");
    g["reasonCode"] = json!("config_change");
    g["note"] = json!("share the platform markups");
    let e = trading::api::admin::update_group(State(st.clone()), staff, Path("standard".into()), trading::api::Body(serde_json::from_value(g).unwrap())).await.unwrap_err();
    assert!(format!("{e:?}").contains("spreadGroup"), "{e:?}");

    // provisioning is idempotent and an engine restart loads the broker like any other
    assert!(!trading::tenants::ensure(&pool, NEW_ID, "qa-northwind", "QA Northwind").await.unwrap());
    let reloaded = trading::persist::load_registry(&pool).await.unwrap();
    assert!(reloaded.iter().any(|t| t.tenant_id == NEW_ID && t.groups.len() == template));

    drop(st);
    pool.close().await;
    if let Ok(mut c) = server.database("postgres").connect().await {
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{db}\" WITH (FORCE)"))).execute(&mut c).await;
    }
}

//! HTTP + WebSocket API (axum, 127.0.0.1:8090). Every route except `/health` and the ticket-authenticated
//! stream sockets requires `X-Kalks-Internal` (the apps' BFFs and internal services call this service; the
//! browser never does, except for the stream sockets which it opens with a one-time ticket).

pub mod accounts;
pub mod admin;
pub mod catalogue;
pub mod book_feed;
pub mod closures;
pub mod controls;
pub mod corporate;
pub mod dealing;
pub mod ledger;
pub mod risk;
pub mod lifecycle;
pub mod mam;
pub mod options;
pub mod options_book;
pub mod book_admin;
pub mod social;
pub mod social_admin;
pub mod social_house;
pub mod stream;
pub mod terminal;

use axum::Json;
use axum::extract::{DefaultBodyLimit, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post, put};
use axum::Router;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use crate::auth::{Keys, Limiter, StreamTickets};
use crate::config::Config;
use crate::engine::Reject;
use crate::rules::TenantConfig;
use crate::shard::{ExecError, Hub, Staff};

/// Next free logins (MT5-style 8 digits: live 10 000 001+, demo 50 000 001+).
pub struct LoginAlloc {
    pub live: AtomicI64,
    pub demo: AtomicI64,
}

#[derive(Clone)]
pub struct AppState {
    pub hub: Hub,
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    pub keys: Keys,
    pub limiter: Limiter,
    pub tickets: StreamTickets,
    pub logins: Arc<LoginAlloc>,
    /// Serialises account creation (max-accounts check + login allocation).
    pub open_lock: Arc<tokio::sync::Mutex<()>>,
    /// Copy trading and PAMM (src/social).
    pub social: Arc<crate::social::Social>,
    /// Kalks Trader connections reported to the gateway (client presence, controls.rs).
    pub presence: Arc<crate::controls::Presence>,
    /// The gateway's internal API (client restrictions, controls.rs).
    pub gateway: Arc<crate::controls::Gateway>,
}

pub fn router(st: AppState) -> Router {
    let internal = Router::new()
        // terminal (MT5-style login per account)
        .route("/v1/terminal/login", post(terminal::login))
        .route("/v1/terminal/sso", post(terminal::sso))
        .route("/v1/terminal/logout", post(terminal::logout))
        .route("/v1/terminal/state", get(terminal::state))
        .route("/v1/terminal/history", get(terminal::history))
        .route("/v1/terminal/orders", post(terminal::place))
        .route("/v1/terminal/orders/{ticket}", patch(terminal::modify_order).delete(terminal::cancel_order))
        .route("/v1/terminal/positions/close-by", post(terminal::close_by))
        .route("/v1/terminal/positions/{ticket}", patch(terminal::modify_position))
        .route("/v1/terminal/positions/{ticket}/close", post(terminal::close_position))
        .route("/v1/terminal/bulk-close", post(terminal::bulk_close))
        .route("/v1/terminal/stream-ticket", post(terminal::stream_ticket))
        .route("/v1/terminal/mam", get(mam::terminal))
        // Kalks FX Options (api/options.rs; closing one position: /v1/terminal/positions/{ticket}/close)
        .route("/v1/terminal/options/preview", post(options::preview))
        .route("/v1/terminal/options/orders", post(options::place))
        .route("/v1/terminal/options/combos/{combo}/close", post(options::close_combo))
        .route("/v1/terminal/options/settlements", get(options::settlements))
        // Kalks FX Options order book (api/options_book.rs, api/book_feed.rs; docs/OPTIONS-EXCHANGE.md)
        .route("/v1/terminal/options/book/orders", post(options_book::place).get(options_book::orders).delete(options_book::cancel_many))
        .route("/v1/terminal/options/book/orders/{id}", patch(options_book::amend).delete(options_book::cancel))
        .route("/v1/terminal/options/book/fills", get(options_book::fills))
        .route("/v1/terminal/options/book/preview", post(options_book::preview))
        .route("/v1/terminal/options/book/deadman", post(options_book::deadman))
        .route("/v1/terminal/options/book/mass-quote", post(options_book::mass_quote))
        .route("/v1/terminal/options/rfq", post(options_book::rfq_open))
        .route("/v1/terminal/options/rfq/{id}", get(options_book::rfq_get).delete(options_book::rfq_cancel))
        .route("/v1/terminal/options/rfq/{id}/accept", post(options_book::rfq_accept))
        // Back Office: order books, market maker, liquidations, clearing, RFQs, busts, rollout (api/book_admin.rs)
        .route("/v1/admin/options/books", get(book_admin::books))
        .route("/v1/admin/options/books/halt", post(book_admin::halt))
        .route("/v1/admin/options/books/halt/{id}", axum::routing::delete(book_admin::unhalt))
        .route("/v1/admin/options/books/{series}", get(book_admin::depth))
        .route("/v1/admin/options/mm", get(book_admin::mm))
        .route("/v1/admin/options/mm/{action}", post(book_admin::mm_pause))
        .route("/v1/admin/options/liquidations", get(book_admin::liquidations))
        .route("/v1/admin/options/clearing", get(book_admin::clearing))
        .route("/v1/admin/options/rfqs", get(book_admin::rfqs))
        .route("/v1/admin/options/fills/{id}/bust", post(book_admin::bust))
        .route("/v1/admin/options/approvals", get(book_admin::approvals))
        .route("/v1/admin/options/book/enable/plan", get(book_admin::enable_plan))
        .route("/v1/admin/options/book/enable", post(book_admin::enable))
        .route("/v1/internal/options/book/stream", get(book_feed::stream))
        .route("/v1/internal/options/book/{tenant}/{kind}/snapshot", get(book_feed::snapshot))
        .route("/v1/internal/options/book/{tenant}/{kind}/trades", get(book_feed::trades))
        .route("/v1/admin/options/book", get(options::book))
        .route("/v1/admin/options/status", get(options::status))
        .route("/v1/admin/options/settlements", get(options::settlement_list))
        .route("/v1/admin/options/settlements/{expiry}/rerun", post(options::rerun))
        .route("/v1/admin/options/trades/{ticket}/void", post(options::void))
        .route("/v1/terminal/controls", get(controls::terminal_controls))
        // client controls: staff sessions in Kalks Trader, restriction refresh (Back Office BFF)
        .route("/v1/admin/accounts/{login}/staff-sso", post(controls::staff_sso))
        .route("/v1/internal/restrictions/refresh", post(controls::refresh))
        // Client Area (CRM BFF, with the gateway user id)
        .route("/v1/accounts", post(accounts::open).get(accounts::list))
        .route("/v1/accounts/{login}", get(accounts::detail).patch(lifecycle::rename))
        .route("/v1/accounts/{login}/archive-check", get(lifecycle::archive_check))
        .route("/v1/accounts/{login}/archive", post(lifecycle::archive))
        .route("/v1/accounts/{login}/restore", post(lifecycle::restore))
        .route("/v1/accounts/{login}/closure", get(lifecycle::closure_status).post(lifecycle::request_closure))
        .route("/v1/accounts/{login}/closure/cancel", post(lifecycle::cancel_closure))
        .route("/v1/accounts/prefs", get(lifecycle::get_prefs).put(lifecycle::put_prefs).post(lifecycle::put_prefs))
        .route("/v1/accounts/{login}/group-options", get(lifecycle::group_options))
        .route("/v1/accounts/{login}/group", post(lifecycle::change_group))
        .route("/v1/accounts/{login}/demo-balance", post(lifecycle::demo_balance))
        .route("/v1/accounts/{login}/health", get(lifecycle::health))
        .route("/v1/accounts/{login}/demo-refill", post(accounts::demo_refill))
        .route("/v1/accounts/{login}/passwords", post(accounts::passwords))
        .route("/v1/accounts/{login}/leverage", post(accounts::leverage))
        .route("/v1/accounts/{login}/history", get(accounts::history))
        .route("/v1/accounts/{login}/ledger", get(accounts::ledger))
        .route("/v1/accounts/{login}/sso", post(accounts::sso))
        .route("/v1/groups", get(accounts::groups))
        .route("/v1/symbols", get(accounts::symbols))
        // instrument catalogue: live-trading switch and spec templates (api/catalogue.rs)
        .route("/v1/admin/symbols/catalogue", get(catalogue::catalogue))
        .route("/v1/admin/symbols/catalogue/audit", get(catalogue::audit_log))
        .route("/v1/admin/symbols/live", put(catalogue::set_live))
        .route("/v1/admin/symbols/templates/{key}", put(catalogue::set_template))
        // stock corporate actions: splits and dividends (api/corporate.rs)
        .route("/v1/admin/corporate-actions", get(corporate::list).post(corporate::create))
        .route("/v1/admin/corporate-actions/import", post(corporate::import))
        .route("/v1/admin/corporate-actions/{id}", get(corporate::detail).patch(corporate::edit))
        .route("/v1/admin/corporate-actions/{id}/approve", post(corporate::approve))
        .route("/v1/admin/corporate-actions/{id}/reject", post(corporate::reject))
        .route("/v1/admin/corporate-actions/{id}/check", post(corporate::check))
        .route("/v1/accounts/{login}/corporate-actions", get(corporate::account))
        // wallet service
        .route("/v1/ledger/transfers", post(ledger::transfer))
        .route("/v1/ledger/transfers/{key}", get(ledger::transfer_status))
        // Back Office dealing desk
        .route("/v1/dealing/state", get(dealing::state))
        .route("/v1/dealing/positions", get(dealing::positions))
        .route("/v1/dealing/orders", get(dealing::orders))
        .route("/v1/dealing/deals", get(dealing::deals))
        .route("/v1/dealing/trades", post(dealing::create_trade))
        .route("/v1/dealing/positions/bulk", post(dealing::bulk))
        .route("/v1/dealing/positions/{ticket}", patch(dealing::modify_position))
        .route("/v1/dealing/positions/{ticket}/close", post(dealing::close))
        .route("/v1/dealing/positions/{ticket}/add", post(dealing::add_volume))
        .route("/v1/dealing/positions/{ticket}/price-correction", post(dealing::price_correction))
        .route("/v1/dealing/positions/{ticket}/charges", post(dealing::charges))
        .route("/v1/dealing/positions/{ticket}/void", post(dealing::void))
        .route("/v1/dealing/deals/{id}/reopen", post(dealing::reopen))
        .route("/v1/dealing/book-transfers", post(dealing::book_transfers))
        .route("/v1/dealing/orders/cancel", post(dealing::cancel_orders))
        .route("/v1/dealing/orders/{ticket}", patch(dealing::modify_order))
        .route("/v1/dealing/orders/{ticket}/fill", post(dealing::fill_order))
        .route("/v1/dealing/controls", get(dealing::controls))
        .route("/v1/dealing/controls/symbols/{symbol}", put(dealing::symbol_control))
        .route("/v1/dealing/controls/accounts/{login}", put(dealing::account_control))
        .route("/v1/dealing/controls/tenant", put(dealing::tenant_policy))
        .route("/v1/dealing/routing/rules", get(dealing::routing_rules).put(dealing::save_routing_rules))
        .route("/v1/dealing/routing/quick", put(dealing::quick_route))
        .route("/v1/dealing/audit", get(dealing::audit))
        .route("/v1/dealing/stream-ticket", post(dealing::stream_ticket))
        // risk desk (api/risk.rs, docs/RISK-DESK.md)
        .route("/v1/dealing/exposure", get(risk::exposure))
        .route("/v1/dealing/risk/limits", get(risk::limits).put(risk::save_limits))
        .route("/v1/dealing/risk/settings", put(risk::save_settings))
        .route("/v1/dealing/risk/alerts", get(risk::alerts))
        .route("/v1/dealing/risk/alerts/{id}/ack", post(risk::ack_alert))
        .route("/v1/dealing/risk/scores", get(risk::scores))
        .route("/v1/dealing/risk/scores/run", post(risk::run_scores))
        .route("/v1/dealing/risk/scores/{login}", get(risk::score))
        .route("/v1/dealing/risk/news", get(risk::news))
        .route("/v1/dealing/routing/suggestions", get(risk::suggestions))
        .route("/v1/dealing/routing/suggestions/{id}/approve", post(risk::approve))
        .route("/v1/dealing/routing/suggestions/{id}/reject", post(risk::reject))
        // Back Office account operations
        .route("/v1/admin/accounts", get(admin::accounts))
        .route("/v1/admin/accounts/{login}", get(admin::account))
        .route("/v1/admin/accounts/{login}/balance", post(admin::balance))
        .route("/v1/admin/accounts/{login}/adjust", post(admin::adjust))
        .route("/v1/admin/accounts/{login}/status", post(admin::status))
        .route("/v1/admin/accounts/{login}/archive", post(admin::archive))
        .route("/v1/admin/accounts/{login}/restore", post(admin::restore))
        .route("/v1/admin/accounts/{login}/closure-check", get(closures::account_check))
        .route("/v1/admin/accounts/{login}/closure", post(closures::staff_request))
        .route("/v1/admin/accounts/{login}/reopen", post(closures::reopen_request))
        .route("/v1/admin/accounts/bulk", post(closures::bulk))
        .route("/v1/admin/account-policy", get(closures::get_policy).put(closures::put_policy))
        .route("/v1/admin/closures", get(closures::list))
        .route("/v1/admin/closures/report", get(closures::report))
        .route("/v1/admin/closures/{id}", get(closures::detail))
        .route("/v1/admin/closures/{id}/approve", post(closures::approve))
        .route("/v1/admin/closures/{id}/reject", post(closures::reject))
        .route("/v1/admin/accounts/{login}/group", post(admin::group))
        .route("/v1/admin/accounts/{login}/leverage", post(admin::leverage))
        .route("/v1/admin/groups", get(admin::groups).post(admin::create_group))
        .route("/v1/admin/groups/{code}", put(admin::update_group))
        .route("/v1/admin/ledger/accounts", get(admin::ledger_accounts))
        // copy trading and PAMM: Client Area (X-Kalks-User-Id) and public reads
        .route("/v1/social/leaderboard", get(social::leaderboard))
        .route("/v1/social/masters/{id}", get(social::master_profile))
        .route("/v1/social/masters/{id}/preview", get(social::master_preview))
        .route("/v1/social/master/me", get(social::master_me).patch(social::master_update))
        .route("/v1/social/master/apply", post(social::master_apply))
        .route("/v1/social/master/dashboard", get(social::master_dashboard))
        .route("/v1/social/subscriptions", post(social::subscribe).get(social::subscriptions))
        .route("/v1/social/subscriptions/{id}", get(social::subscription).patch(social::update_subscription))
        .route("/v1/social/subscriptions/{id}/stop", post(social::stop_subscription))
        .route("/v1/social/subscriptions/{id}/funds", post(social::sub_funds))
        .route("/v1/social/subscriptions/{id}/accept-terms", post(social::accept_terms))
        .route("/v1/social/subscriptions/{id}/execution", get(social::execution))
        .route("/v1/social/master/announcements", post(social::announce))
        .route("/v1/social/funds", get(social::funds).post(social::create_fund))
        .route("/v1/social/funds/{id}", get(social::fund).patch(social::update_fund))
        .route("/v1/social/funds/{id}/invest", post(social::invest))
        .route("/v1/social/funds/{id}/redeem", post(social::redeem))
        .route("/v1/social/funds/{id}/statement", get(social::statement))
        .route("/v1/social/requests/{id}/cancel", post(social::cancel_request))
        .route("/v1/social/investments", get(social::investments))
        .route("/v1/social/investments/{fund_id}", patch(social::update_investment))
        // copy trading and PAMM: Back Office
        .route("/v1/social/admin/overview", get(social_admin::overview))
        .route("/v1/social/admin/masters", get(social_admin::masters))
        .route("/v1/social/admin/masters/{id}/review", post(social_admin::review))
        .route("/v1/social/admin/masters/{id}/status", post(social_admin::master_status))
        .route("/v1/social/admin/masters/{id}/emergency", post(social_admin::emergency))
        .route("/v1/social/admin/subscriptions", get(social_admin::subscriptions))
        .route("/v1/social/admin/subscriptions/{id}/stop", post(social_admin::stop_subscription))
        .route("/v1/social/admin/copy-dashboard", get(social_admin::copy_dashboard))
        .route("/v1/social/admin/funds", get(social_admin::funds))
        .route("/v1/social/admin/funds/{id}/freeze", post(social_admin::freeze_fund))
        .route("/v1/social/admin/funds/{id}/rollover", post(social_admin::fund_rollover))
        .route("/v1/social/admin/rollover", post(social_admin::rollover_all))
        .route("/v1/social/admin/snapshots", post(social_admin::snapshots))
        .route("/v1/social/admin/settings", get(social_admin::settings).put(social_admin::save_settings))
        // MAM (multi-account manager): Client Area, manager, Back Office
        .route("/v1/social/mam/managers", get(mam::managers))
        .route("/v1/social/mam/managers/{id}", get(mam::manager_detail))
        .route("/v1/social/mam/links", get(mam::links).post(mam::create_link))
        .route("/v1/social/mam/links/{id}", get(mam::link).patch(mam::update_link))
        .route("/v1/social/mam/links/{id}/revoke", post(mam::revoke_link))
        .route("/v1/social/mam/manager", get(mam::manager_me).post(mam::create_manager).patch(mam::update_manager))
        .route("/v1/social/mam/manager/links/{id}", patch(mam::set_link_value))
        .route("/v1/social/mam/manager/preview", get(mam::preview))
        .route("/v1/social/mam/manager/allocations", get(mam::manager_allocations))
        .route("/v1/social/admin/mam/managers", get(mam::admin_managers))
        .route("/v1/social/admin/mam/managers/{id}/emergency", post(mam::admin_emergency))
        .route("/v1/social/admin/mam/links", get(mam::admin_links))
        .route("/v1/social/admin/mam/links/{id}/stop", post(mam::admin_stop_link))
        .route("/v1/social/admin/mam/allocations", get(mam::admin_allocations))
        .route("/v1/social/admin/fees", get(social_admin::fees))
        .route("/v1/social/admin/fees/{id}/review", post(social_admin::review_fee))
        .route("/v1/social/admin/audit", get(social_admin::audit))
        .route("/v1/social/admin/house", post(social_house::provision))
        .route("/v1/social/admin/house/{id}/capital", post(social_house::capital))
        .route("/v1/social/admin/house/{id}/retire", post(social_house::retire))
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(st.clone(), internal_only));
    Router::new()
        .route("/health", get(health))
        .route("/v1/terminal/stream", get(stream::terminal))
        .route("/v1/dealing/stream", get(stream::dealing))
        .merge(internal)
        .fallback(|| async { (StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found", "message": "Not found."}}))) })
        .with_state(st)
}

async fn internal_only(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let expected = st.cfg.internal_token.as_bytes();
    if !expected.is_empty() {
        let got = req.headers().get("x-kalks-internal").map(|v| v.as_bytes()).unwrap_or_default();
        if !crate::auth::ct_eq(got, expected) {
            return ApiError::Forbidden("Not allowed.".into()).into_response();
        }
    }
    next.run(req).await
}

async fn health(State(st): State<AppState>) -> impl IntoResponse {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&st.pool).await.is_ok();
    let feeds = st.hub.shared.quotes.status();
    let feed_ok = !feeds.is_empty() && feeds.values().all(|(c, age)| *c && *age < 30_000);
    let stats = &st.hub.shared.stats;
    let accounts = st.hub.shared.index.read().unwrap().accounts.len();
    let feed: serde_json::Map<String, Value> = feeds.into_iter().map(|(g, (c, age))| (g, json!({"connected": c, "lastFrameMs": age}))).collect();
    Json(json!({
        "status": if db && feed_ok { "ok" } else { "degraded" },
        "service": "trading",
        "db": db,
        "feed": feed,
        "feedConnected": feed_ok,
        "accountsLoaded": accounts,
        "ticksProcessed": stats.ticks.load(Ordering::Relaxed),
        "commits": stats.commits.load(Ordering::Relaxed),
        "commitErrors": stats.commit_errors.load(Ordering::Relaxed),
        "lagMs": stats.last_tick_lag_ms.load(Ordering::Relaxed),
        "maxLagMs": stats.max_tick_lag_ms.swap(0, Ordering::Relaxed),
        "lp": {"name": st.hub.shared.lp.name(), "connected": st.hub.shared.lp.connected()},
        "options": {
            "configured": st.hub.shared.options.configured(),
            "version": st.hub.shared.options.version(),
            "stale": crate::options::OptionPricing::stale(st.hub.shared.options.as_ref(), st.hub.shared.clock.now()),
            "lastOkAt": chrono::DateTime::from_timestamp_millis(st.hub.shared.options.last_ok_ms()),
        },
    }))
}

/* ------------------------------------------------------------------ */
/* Errors                                                              */
/* ------------------------------------------------------------------ */

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Validation { field: &'static str, message: String },
    Unauthorized,
    Forbidden(String),
    /// Investor (view-only) session tried to write (D107).
    ReadOnly,
    NotFound(String),
    Conflict { code: &'static str, message: String },
    RateLimited(u64),
    Reject { reject: Reject, audit: Vec<Value> },
    /// Any status with our error shape (social API: 422 business rules, 503 wallet unavailable, …).
    Status { status: u16, code: &'static str, message: String },
    /// Same, with extra fields merged into the error object (e.g. `checks`).
    StatusData { status: u16, code: &'static str, message: String, data: Value },
    Internal(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError::Internal(e.into())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    pub fn reject(r: Reject) -> Self {
        ApiError::Reject { reject: r, audit: vec![] }
    }
}

impl From<ExecError> for ApiError {
    fn from(e: ExecError) -> Self {
        match e {
            ExecError::Reject(r) => ApiError::reject(r),
            ExecError::NotFound => ApiError::NotFound("Account not found".into()),
            ExecError::Duplicate(k) => ApiError::Conflict { code: "duplicate_idempotency_key", message: format!("Idempotency key {k} was already used") },
            ExecError::Internal(m) => ApiError::Internal(anyhow::anyhow!(m)),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, body, audit, retry) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, json!({"code": "bad_request", "message": m}), None, None),
            ApiError::Validation { field, message } => (StatusCode::UNPROCESSABLE_ENTITY, json!({"code": "validation", "field": field, "message": message}), None, None),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, json!({"code": "unauthorized", "message": "Please sign in."}), None, None),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, json!({"code": "forbidden", "message": m}), None, None),
            ApiError::ReadOnly => (StatusCode::FORBIDDEN, json!({"code": "read_only", "message": "This is a view-only (investor) session: trading is not allowed."}), None, None),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, json!({"code": "not_found", "message": m}), None, None),
            ApiError::Conflict { code, message } => (StatusCode::CONFLICT, json!({"code": code, "message": message}), None, None),
            ApiError::RateLimited(s) => (StatusCode::TOO_MANY_REQUESTS, json!({"code": "rate_limited", "message": format!("Too many requests. Try again in {s}s."), "retryAfter": s}), None, Some(s)),
            ApiError::Reject { reject, audit } => {
                let status = match reject.code {
                    "not_found" => StatusCode::NOT_FOUND,
                    "requote" => StatusCode::CONFLICT,
                    _ => StatusCode::UNPROCESSABLE_ENTITY,
                };
                let mut b = json!({"code": reject.code, "message": reject.message});
                if let Some((bid, ask)) = reject.requote {
                    b["bid"] = crate::money::num(bid);
                    b["ask"] = crate::money::num(ask);
                }
                (status, b, Some(audit), None)
            }
            ApiError::Status { status, code, message } => (StatusCode::from_u16(status).unwrap_or(StatusCode::UNPROCESSABLE_ENTITY), json!({"code": code, "message": message}), None, None),
            ApiError::StatusData { status, code, message, data } => {
                let mut b = json!({"code": code, "message": message});
                if let (Value::Object(o), Value::Object(d)) = (&mut b, data) {
                    o.extend(d);
                }
                (StatusCode::from_u16(status).unwrap_or(StatusCode::UNPROCESSABLE_ENTITY), b, None, None)
            }
            ApiError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, json!({"code": "internal", "message": "Something went wrong. Please try again."}), None, None)
            }
        };
        let mut out = json!({"error": body});
        if let Some(a) = audit.filter(|a| !a.is_empty()) {
            out["audit"] = json!(a);
        }
        let mut res = (status, Json(out)).into_response();
        if let Some(s) = retry
            && let Ok(v) = HeaderValue::from_str(&s.to_string())
        {
            res.headers_mut().insert(header::RETRY_AFTER, v);
        }
        res
    }
}

/// JSON body with our error format on parse failures.
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: serde::de::DeserializeOwned> axum::extract::FromRequest<S> for Body<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(v)) => Ok(Body(v)),
            Err(e) => Err(ApiError::BadRequest(e.body_text())),
        }
    }
}

/* ------------------------------------------------------------------ */
/* Request context                                                     */
/* ------------------------------------------------------------------ */

pub fn header(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

/// Tenant (`X-Kalks-Tenant` slug, default `kalks`), client ip and user agent as forwarded by the BFF.
pub struct Ctx {
    pub tenant: Arc<TenantConfig>,
    pub ip: String,
    pub user_agent: String,
    pub bearer: Option<String>,
}

impl FromRequestParts<AppState> for Ctx {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let slug = header(parts, "x-kalks-tenant").unwrap_or_else(|| "kalks".into()).to_lowercase();
        // a broker created in the Owner panel is provisioned here on its first request (tenants.rs)
        let tenant = match st.hub.shared.registry.by_slug(&slug) {
            Some(t) => t,
            None => crate::tenants::provision(st, &slug).await.ok_or_else(|| ApiError::BadRequest(format!("Unknown tenant {slug}")))?,
        };
        let ip = header(parts, "x-forwarded-for").and_then(|v| v.split(',').next().map(|s| s.trim().to_string())).filter(|v| v.len() <= 64).unwrap_or_else(|| "unknown".into());
        let user_agent: String = header(parts, "user-agent").unwrap_or_default().chars().take(400).collect();
        let bearer = header(parts, "authorization").and_then(|v| v.strip_prefix("Bearer ").map(|t| t.trim().to_string())).filter(|t| !t.is_empty() && t.len() <= 128);
        Ok(Ctx { tenant, ip, user_agent, bearer })
    }
}

/// Staff identity forwarded by the Back Office BFF after it verified the staff session with the gateway:
/// `X-Kalks-Staff-Id`, `X-Kalks-Staff-Name` (percent-encoded UTF-8), `X-Kalks-Staff-Role`.
pub struct StaffCtx {
    pub ctx: Ctx,
    pub staff: Staff,
    /// `X-Kalks-Staff-Perms`: the gateway permission keys the BFF (or the wallet service, for balance
    /// adjustments) forwards. None when the caller sent no list (older callers): then only the role is checked.
    pub perms: Option<Vec<String>>,
}

pub const ROLES_DEALING: &[&str] = &["platform_owner", "super_admin", "admin", "dealer", "risk_manager"];
pub const ROLES_FINANCE: &[&str] = &["platform_owner", "super_admin", "admin", "finance"];
pub const ROLES_CONFIG: &[&str] = &["platform_owner", "super_admin", "admin"];
/// Forcing a deduction past the free margin (finance.adjust_force): Super Admin only.
pub const ROLES_FORCE: &[&str] = &["platform_owner", "super_admin"];

impl StaffCtx {
    pub fn require(&self, roles: &[&str]) -> ApiResult<()> {
        if roles.contains(&self.staff.role.as_str()) { Ok(()) } else { Err(ApiError::Forbidden(format!("Role {} may not do this", self.staff.role))) }
    }

    /// The exact gateway permission when the caller forwarded the list; otherwise the role fallback.
    pub fn require_perm(&self, perm: &str, fallback_roles: &[&str]) -> ApiResult<()> {
        match &self.perms {
            Some(p) if p.iter().any(|x| x == perm) => Ok(()),
            Some(_) => Err(ApiError::Forbidden(format!("Missing permission {perm}"))),
            None => self.require(fallback_roles),
        }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl FromRequestParts<AppState> for StaffCtx {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let ctx = Ctx::from_request_parts(parts, st).await?;
        let id = header(parts, "x-kalks-staff-id").ok_or(ApiError::Unauthorized)?;
        let role = header(parts, "x-kalks-staff-role").ok_or(ApiError::Unauthorized)?;
        let name = header(parts, "x-kalks-staff-name").map(|n| percent_decode(&n)).unwrap_or_else(|| format!("Staff {id}"));
        if id.len() > 64 || role.len() > 32 {
            return Err(ApiError::BadRequest("Invalid staff headers".into()));
        }
        let perms = header(parts, "x-kalks-staff-perms").map(|v| v.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty() && p.len() <= 64).take(200).collect());
        Ok(StaffCtx { ctx, staff: Staff { id, name: name.chars().take(120).collect(), role }, perms })
    }
}

/// The gateway client id, from `X-Kalks-User-Id` or `?user_id=`.
pub fn user_id(parts_user: Option<String>, query_user: Option<i64>) -> ApiResult<i64> {
    query_user.or_else(|| parts_user.and_then(|v| v.parse().ok())).ok_or(ApiError::Validation { field: "user_id", message: "user_id is required".into() })
}

pub fn ok(v: Value) -> Json<Value> {
    Json(v)
}

pub use terminal::parse_time;

pub fn parse_ticket(s: &str) -> ApiResult<i64> {
    s.trim().parse::<i64>().map_err(|_| ApiError::BadRequest(format!("Invalid ticket {s}")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_staff_names() {
        assert_eq!(super::percent_decode("Julia%20Novak"), "Julia Novak");
        assert_eq!(super::percent_decode("Jos%C3%A9"), "José");
        assert_eq!(super::percent_decode("100%"), "100%");
    }
}

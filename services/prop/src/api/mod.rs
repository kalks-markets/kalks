//! HTTP API (axum). Every route except `/health` needs `X-Kalks-Internal: $PROP_INTERNAL_TOKEN`; only the
//! Client Area / Back Office BFFs hold it. Tenant: `X-Kalks-Tenant` (default `kalks`).
//! - Client routes: the BFF forwards the signed-in gateway user in `X-Kalks-User-Id` (+ `X-Kalks-User-Name`,
//!   `X-Kalks-User-Kyc`).
//! - Staff routes (`/v1/admin/*`): `X-Kalks-Staff-Id`, `X-Kalks-Staff-Name` (percent-encoded), `X-Kalks-Staff-Role`
//!   from the verified staff session; the role is checked against [`PERMS`].

pub mod admin;
pub mod client;
pub mod public;

use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::de::DeserializeOwned;
use serde_json::json;
use subtle::ConstantTimeEq;

use crate::error::ApiError;
use crate::ops::App;
use crate::store::Actor;

pub fn router(app: App) -> Router {
    // client routes: refused while the broker has the prop module off (modules.rs)
    let client = Router::new()
        .route("/v1/plans", get(client::plans))
        .route("/v1/challenges", get(client::challenges).post(client::purchase))
        .route("/v1/challenges/{id}", get(client::challenge))
        .route("/v1/challenges/{id}/equity", get(client::equity))
        .route("/v1/challenges/{id}/events", get(client::events))
        .route("/v1/challenges/{id}/trades", get(client::trades))
        .route("/v1/challenges/{id}/payouts", post(client::request_payout))
        .route("/v1/payouts", get(client::payouts))
        .route("/v1/certificates", get(client::certificates))
        .route("/v1/notifications", get(client::notifications))
        .route("/v1/notifications/read", post(client::notifications_read))
        .route_layer(middleware::from_fn_with_state(app.clone(), crate::modules::gate));
    let api = Router::new()
        // public (through the BFF): certificates stay verifiable
        .route("/v1/public/certificates/{code}", get(public::certificate))
        .route("/v1/public/certificates/{code}/image.svg", get(public::certificate_svg))
        .merge(client)
        // staff
        .route("/v1/admin/overview", get(admin::overview))
        .route("/v1/admin/plans", get(admin::plans).post(admin::create_plan))
        .route("/v1/admin/plans/{id}", put(admin::update_plan))
        .route("/v1/admin/plans/{id}/status", post(admin::plan_status))
        .route("/v1/admin/engine-groups", get(admin::engine_groups))
        .route("/v1/admin/challenges", get(admin::challenges))
        .route("/v1/admin/challenges/{id}", get(admin::challenge))
        .route("/v1/admin/challenges/{id}/override", post(admin::override_challenge))
        .route("/v1/admin/accounts/{id}/scale", post(admin::scale))
        .route("/v1/admin/events", get(admin::events))
        .route("/v1/admin/payouts", get(admin::payouts))
        .route("/v1/admin/payouts/{id}/approve", post(admin::approve_payout))
        .route("/v1/admin/payouts/{id}/reject", post(admin::reject_payout))
        .route("/v1/admin/certificates", get(admin::certificates))
        .route("/v1/admin/certificates/{code}/revoke", post(admin::revoke_certificate))
        .route("/v1/admin/flags", get(admin::flags))
        .route("/v1/admin/flags/{id}/review", post(admin::review_flag))
        .route("/v1/admin/news", get(admin::news).post(admin::create_news))
        .route("/v1/admin/news/{id}", delete(admin::delete_news))
        .route("/v1/admin/audit", get(admin::audit))
        .layer(middleware::from_fn_with_state(app.clone(), internal_only));
    Router::new().route("/health", get(health)).merge(api).with_state(app)
}

async fn health(State(app): State<App>) -> impl IntoResponse {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&app.pool).await.is_ok();
    let engine = app.engine.health().await;
    let status = if db { axum::http::StatusCode::OK } else { axum::http::StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(json!({"status": if db { "ok" } else { "degraded" }, "service": "prop", "db": db, "engine": engine})))
}

async fn internal_only(State(app): State<App>, req: Request, next: Next) -> Response {
    let want = app.cfg.internal_token.as_bytes();
    if !want.is_empty() {
        let got = req.headers().get("x-kalks-internal").map(|v| v.as_bytes()).unwrap_or_default();
        if got.len() != want.len() || !bool::from(got.ct_eq(want)) {
            return ApiError::Forbidden("Missing or invalid internal token".into()).into_response();
        }
    }
    next.run(req).await
}

fn header(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get(name).and_then(|v| v.to_str().ok()).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Tenant slug from `X-Kalks-Tenant`.
pub struct Tenant(pub String);

impl<S: Send + Sync> FromRequestParts<S> for Tenant {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let t = header(parts, "x-kalks-tenant").unwrap_or_else(|| "kalks".into()).to_lowercase();
        if t.len() > 40 || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return Err(ApiError::BadRequest("Invalid tenant".into()));
        }
        Ok(Tenant(t))
    }
}

/// The signed-in client (forwarded by the Client Area BFF).
pub struct User {
    pub tenant: String,
    pub id: i64,
    pub name: String,
    pub kyc: Option<String>,
}

impl<S: Send + Sync> FromRequestParts<S> for User {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, s: &S) -> Result<Self, Self::Rejection> {
        let Tenant(tenant) = Tenant::from_request_parts(parts, s).await?;
        let id = header(parts, "x-kalks-user-id").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).ok_or(ApiError::Unauthorized)?;
        let name = header(parts, "x-kalks-user-name").map(|n| percent_decode(&n)).unwrap_or_default().chars().take(120).collect();
        let kyc = header(parts, "x-kalks-user-kyc").filter(|k| k.len() <= 20);
        Ok(User { tenant, id, name, kyc })
    }
}

/// Back Office permissions for the prop module (the admin BFF checks the same map first).
///
/// | permission | allows | roles |
/// |---|---|---|
/// | `prop.read` | plans, challenges, rule events, payouts, certificates, flags, news, audit | every staff role except marketing, partner_manager |
/// | `prop.write` | plan builder, manual pass/fail, flag review, news calendar, scaling, certificate revoke | platform_owner, super_admin, admin, risk_manager, dealer |
/// | `prop.approve` | payout approve / reject | platform_owner, super_admin, admin, finance, risk_manager |
pub const PERMS: &[(&str, &[&str])] = &[
    ("prop.read", &["platform_owner", "super_admin", "admin", "dealer", "risk_manager", "compliance", "finance", "support", "viewer"]),
    ("prop.write", &["platform_owner", "super_admin", "admin", "risk_manager", "dealer"]),
    ("prop.approve", &["platform_owner", "super_admin", "admin", "finance", "risk_manager"]),
];

pub struct Staff {
    pub tenant: String,
    pub actor: Actor,
}

impl Staff {
    pub fn require(&self, perm: &str) -> Result<(), ApiError> {
        let ok = PERMS.iter().find(|(p, _)| *p == perm).is_some_and(|(_, roles)| roles.contains(&self.actor.role.as_str()));
        if ok { Ok(()) } else { Err(ApiError::Forbidden(format!("Role {} may not do this ({perm})", self.actor.role))) }
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Staff {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, s: &S) -> Result<Self, Self::Rejection> {
        let Tenant(tenant) = Tenant::from_request_parts(parts, s).await?;
        let id = header(parts, "x-kalks-staff-id").ok_or(ApiError::Unauthorized)?;
        let role = header(parts, "x-kalks-staff-role").ok_or(ApiError::Unauthorized)?;
        if id.len() > 64 || role.len() > 32 {
            return Err(ApiError::BadRequest("Invalid staff headers".into()));
        }
        let name = header(parts, "x-kalks-staff-name").map(|n| percent_decode(&n)).unwrap_or_else(|| format!("Staff {id}"));
        Ok(Staff { tenant, actor: Actor { id: format!("staff:{id}"), name: name.chars().take(120).collect(), role } })
    }
}

/// JSON body with a validation error instead of axum's plain-text rejection.
pub struct Body<T>(pub T);

impl<S: Send + Sync, T: DeserializeOwned> axum::extract::FromRequest<S> for Body<T> {
    type Rejection = ApiError;
    async fn from_request(req: Request, s: &S) -> Result<Self, Self::Rejection> {
        let bytes = axum::body::Bytes::from_request(req, s).await.map_err(|_| ApiError::BadRequest("Invalid body".into()))?;
        let v = if bytes.is_empty() { serde_json::Value::Object(Default::default()) } else { serde_json::from_slice(&bytes).map_err(|e| ApiError::BadRequest(format!("Invalid JSON: {e}")))? };
        serde_json::from_value(v).map(Body).map_err(|e| ApiError::BadRequest(format!("Invalid request: {e}")))
    }
}

pub fn page(page: Option<i64>, limit: Option<i64>, max: i64) -> (i64, i64) {
    let limit = limit.unwrap_or(50).clamp(1, max);
    let page = page.unwrap_or(1).max(1);
    (page, limit)
}

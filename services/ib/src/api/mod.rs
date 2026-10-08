//! HTTP API on 127.0.0.1:8096. Every route except `GET /health` needs `X-Kalks-Internal: $IB_INTERNAL_TOKEN`.
//! Client routes take the signed-in gateway user in `X-Kalks-User-Id` (the CRM BFF resolves it from the
//! session cookie); admin routes take the staff identity headers the admin BFF verified with the gateway.

pub mod admin;
pub mod client;
pub mod public;

use crate::audit::Actor;
use crate::error::{ApiError, ApiResult};
use crate::state::AppState;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use subtle::ConstantTimeEq;

pub fn router(st: AppState) -> Router {
    // Client Area: refused while the broker has the ib module off (modules.rs)
    let me = Router::new()
        .route("/v1/ib/me", get(client::dashboard))
        .route("/v1/ib/me/programme", get(client::programme))
        .route("/v1/ib/me/campaigns", get(client::campaigns).post(client::create_campaign))
        .route("/v1/ib/me/campaigns/{id}", axum::routing::patch(client::update_campaign))
        .route("/v1/ib/me/clients", get(client::clients))
        .route("/v1/ib/me/clients/{id}/trades", get(client::client_trades))
        .route("/v1/ib/me/network", get(client::network))
        .route("/v1/ib/me/commissions", get(client::commissions))
        .route("/v1/ib/me/payouts", get(client::payouts))
        .route("/v1/ib/me/settings", axum::routing::put(client::update_settings))
        .route_layer(middleware::from_fn_with_state(st.clone(), crate::modules::gate));
    let v1 = Router::new()
        // public tracking + events from other services
        .route("/v1/ib/clicks", post(public::click))
        .route("/v1/ib/events/deposit", post(public::deposit_event))
        .route("/v1/ib/events/lots", post(public::lots_event))
        .merge(me)
        // Back Office
        .route("/v1/ib/admin/overview", get(admin::overview))
        .route("/v1/ib/admin/settings", get(admin::get_settings).put(admin::put_settings))
        .route("/v1/ib/admin/levels", get(admin::get_levels).put(admin::put_levels))
        .route("/v1/ib/admin/partners", get(admin::partners))
        .route("/v1/ib/admin/partners/{id}", get(admin::partner).patch(admin::patch_partner))
        .route("/v1/ib/admin/partners/{id}/reassign", post(admin::reassign))
        .route("/v1/ib/admin/commissions", get(admin::commissions))
        .route("/v1/ib/admin/commissions/{id}/reject", post(admin::reject_commission))
        .route("/v1/ib/admin/batches", get(admin::batches).post(admin::create_batch))
        .route("/v1/ib/admin/batches/{id}", get(admin::batch))
        .route("/v1/ib/admin/batches/{id}/approve", post(admin::approve_batch))
        .route("/v1/ib/admin/batches/{id}/reject", post(admin::reject_batch))
        .route("/v1/ib/admin/batches/{id}/retry", post(admin::retry_batch))
        .route("/v1/ib/admin/flags", get(admin::flags))
        .route("/v1/ib/admin/flags/{id}/resolve", post(admin::resolve_flag))
        .route("/v1/ib/admin/audit", get(admin::audit))
        .route("/v1/ib/admin/run/{job}", post(admin::run_job))
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(st.clone(), internal_only));
    Router::new()
        .route("/health", get(health))
        .merge(v1)
        .fallback(|| async { (axum::http::StatusCode::NOT_FOUND, Json(json!({"error": {"code": "not_found", "message": "Not found."}}))) })
        .with_state(st)
}

async fn health(State(st): State<AppState>) -> impl IntoResponse {
    let db = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&st.pool).await.is_ok();
    Json(json!({"status": if db { "ok" } else { "degraded" }, "db": db, "service": "ib"}))
}

async fn internal_only(State(st): State<AppState>, req: Request, next: Next) -> Response {
    let expected = st.cfg.internal_token.as_bytes();
    if !expected.is_empty() {
        let got = req.headers().get("x-kalks-internal").map(|v| v.as_bytes()).unwrap_or_default();
        if got.len() != expected.len() || !bool::from(got.ct_eq(expected)) {
            return ApiError::Forbidden("Missing or wrong internal token.".into()).into_response();
        }
    }
    next.run(req).await
}

fn header(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

fn tenant_of(parts: &Parts) -> ApiResult<String> {
    let t = header(parts, "x-kalks-tenant").unwrap_or_else(|| "kalks".into()).to_lowercase();
    if t.len() > 40 || !t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::BadRequest("Invalid tenant.".into()));
    }
    Ok(t)
}

/// Tenant only (public / service-to-service routes).
pub struct Tenant(pub String);

impl<S: Send + Sync> FromRequestParts<S> for Tenant {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(Tenant(tenant_of(parts)?))
    }
}

/// The signed-in client (Client Area BFF).
pub struct UserCtx {
    pub tenant: String,
    pub user_id: i64,
}

impl<S: Send + Sync> FromRequestParts<S> for UserCtx {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let user_id = header(parts, "x-kalks-user-id").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).ok_or(ApiError::Unauthorized)?;
        Ok(UserCtx { tenant: tenant_of(parts)?, user_id })
    }
}

/// Back Office staff, as verified by the admin BFF.
pub struct StaffCtx {
    pub tenant: String,
    pub id: String,
    pub name: String,
    pub role: String,
}

/// Roles for programme changes (settings, levels, partner edits, reassignment, flags, batch creation).
pub const ROLES_WRITE: &[&str] = &["platform_owner", "super_admin", "admin", "partner_manager"];
/// Roles that may approve / reject payout batches and reject commissions (money decisions).
pub const ROLES_APPROVE: &[&str] = &["platform_owner", "super_admin", "admin", "finance"];

impl StaffCtx {
    pub fn require(&self, roles: &[&str]) -> ApiResult<()> {
        if roles.contains(&self.role.as_str()) { Ok(()) } else { Err(ApiError::Forbidden("Your role doesn't allow this.".into())) }
    }
    pub fn actor(&self) -> Actor {
        Actor { id: format!("staff:{}", self.id), name: Some(self.name.clone()) }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl<S: Send + Sync> FromRequestParts<S> for StaffCtx {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let id = header(parts, "x-kalks-staff-id").ok_or(ApiError::Unauthorized)?;
        let role = header(parts, "x-kalks-staff-role").ok_or(ApiError::Unauthorized)?;
        if id.len() > 64 || role.len() > 32 {
            return Err(ApiError::BadRequest("Invalid staff headers.".into()));
        }
        let name = header(parts, "x-kalks-staff-name").map(|n| percent_decode(&n)).unwrap_or_else(|| format!("Staff {id}"));
        Ok(StaffCtx { tenant: tenant_of(parts)?, id, name: name.chars().take(80).collect(), role })
    }
}

/// `page` / `limit` from a query, clamped.
pub fn paging(page: Option<i64>, limit: Option<i64>, default: i64, max: i64) -> (i64, i64, i64) {
    let limit = limit.unwrap_or(default).clamp(1, max);
    let page = page.unwrap_or(1).max(1);
    (page, limit, (page - 1) * limit)
}

pub fn parse_time(v: &Option<String>) -> ApiResult<Option<chrono::DateTime<chrono::Utc>>> {
    let Some(s) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()) else { return Ok(None) };
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(Some(t.with_timezone(&chrono::Utc)));
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map(|d| Some(chrono::DateTime::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), chrono::Utc)))
        .map_err(|_| ApiError::BadRequest("Dates must be YYYY-MM-DD or RFC 3339.".into()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_staff_names() {
        assert_eq!(super::percent_decode("Julia%20Novak"), "Julia Novak");
        assert_eq!(super::percent_decode("A%C3%AFda"), "Aïda");
        assert_eq!(super::percent_decode("50%"), "50%");
    }
}

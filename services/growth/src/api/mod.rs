//! HTTP API on 127.0.0.1:8101. Every route except `GET /health` needs `X-Kalks-Internal: $GROWTH_INTERNAL_TOKEN`.
//! Client routes take the signed-in gateway user in `X-Kalks-User-Id` plus segment headers (country, KYC,
//! sign-up time, name, referral code) the Client Area BFF reads from the gateway session; admin routes take the
//! staff identity headers the admin BFF verified with the gateway.

pub mod admin;
pub mod client;
pub mod journeys;
pub mod public;

use crate::audit::Actor;
use crate::error::{ApiError, ApiResult};
use crate::profiles::Hints;
use crate::state::AppState;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use serde_json::json;
use subtle::ConstantTimeEq;

pub fn router(st: AppState) -> Router {
    // Client Area rewards: refused while the broker has the rewards module off (modules.rs)
    let rewards = Router::new()
        .route("/v1/growth/me/rewards", get(client::rewards))
        .route("/v1/growth/me/points", get(client::points))
        .route("/v1/growth/me/redeem", post(client::redeem))
        .route("/v1/growth/me/redemptions", get(client::redemptions))
        .route("/v1/growth/me/vouchers", get(client::vouchers))
        .route("/v1/growth/me/cashback", get(client::cashback))
        .route("/v1/growth/me/cashback/{id}/enrol", post(client::enrol))
        .route("/v1/growth/me/promotions", get(client::promotions))
        .route("/v1/growth/me/bonuses/{id}/claim", post(client::claim))
        .route("/v1/growth/me/promo", post(client::promo))
        .route("/v1/growth/me/contests", get(client::contests))
        .route("/v1/growth/me/contests/{id}", get(client::contest))
        .route("/v1/growth/me/contests/{id}/join", post(client::join))
        .route_layer(middleware::from_fn_with_state(st.clone(), crate::modules::gate));
    let v1 = Router::new()
        // public (Client Area server) + other services
        .route("/v1/growth/public/shares/{code}", get(public::share))
        .route("/v1/growth/internal/vouchers", get(public::vouchers))
        .route("/v1/growth/internal/vouchers/redeem", post(public::redeem_voucher))
        .route("/v1/growth/internal/accounts/{login}/retired", post(public::account_retired))
        .merge(rewards)
        // Client Area: banners and share cards
        .route("/v1/growth/me/banners", get(client::banners))
        .route("/v1/growth/me/banners/{id}/events", post(client::banner_event))
        .route("/v1/growth/me/shares", get(client::shares).post(client::create_share))
        // Back Office
        .route("/v1/growth/admin/overview", get(admin::overview))
        .route("/v1/growth/admin/settings", get(admin::get_settings).put(admin::put_settings))
        .route("/v1/growth/admin/tiers", get(admin::get_tiers).put(admin::put_tiers))
        .route("/v1/growth/admin/rules", get(admin::rules).post(admin::create_rule))
        .route("/v1/growth/admin/rules/{id}", patch(admin::patch_rule))
        .route("/v1/growth/admin/catalogue", get(admin::catalogue).post(admin::create_item))
        .route("/v1/growth/admin/catalogue/{id}", patch(admin::patch_item))
        .route("/v1/growth/admin/redemptions", get(admin::redemptions))
        .route("/v1/growth/admin/redemptions/{id}/retry", post(admin::retry_redemption))
        .route("/v1/growth/admin/members", get(admin::members))
        .route("/v1/growth/admin/points/adjust", post(admin::adjust_points))
        .route("/v1/growth/admin/cashback/programmes", get(admin::programmes).post(admin::create_programme))
        .route("/v1/growth/admin/cashback/programmes/{id}", patch(admin::patch_programme))
        .route("/v1/growth/admin/cashback/accruals", get(admin::accruals))
        .route("/v1/growth/admin/cashback/payouts", get(admin::cashback_payouts))
        .route("/v1/growth/admin/cashback/payouts/run", post(admin::run_cashback))
        .route("/v1/growth/admin/bonuses/campaigns", get(admin::campaigns).post(admin::create_campaign))
        .route("/v1/growth/admin/bonuses/campaigns/{id}", patch(admin::patch_campaign))
        .route("/v1/growth/admin/bonuses/grants", get(admin::grants).post(admin::manual_grant))
        .route("/v1/growth/admin/bonuses/grants/{id}/cancel", post(admin::cancel_grant))
        .route("/v1/growth/admin/promos", get(admin::promos).post(admin::create_promo))
        .route("/v1/growth/admin/promos/redemptions", get(admin::promo_redemptions))
        .route("/v1/growth/admin/promos/{id}", patch(admin::patch_promo))
        .route("/v1/growth/admin/banners", get(admin::banners).post(admin::create_banner))
        .route("/v1/growth/admin/banners/preview", get(admin::banner_preview))
        .route("/v1/growth/admin/banners/{id}", patch(admin::patch_banner))
        .route("/v1/growth/admin/contests", get(admin::contests).post(admin::create_contest))
        .route("/v1/growth/admin/contests/{id}", get(admin::contest).patch(admin::patch_contest))
        .route("/v1/growth/admin/contests/{id}/cancel", post(admin::cancel_contest))
        .route("/v1/growth/admin/contests/{id}/refresh", post(admin::refresh_contest))
        .route("/v1/growth/admin/contests/{id}/finalize", post(admin::finalize_contest))
        .route("/v1/growth/admin/contests/{id}/pay", post(admin::pay_contest))
        .route("/v1/growth/admin/contests/{id}/entries/{entry}/{action}", post(admin::entry_action))
        .route("/v1/growth/admin/contests/{id}/flags/{flag}/resolve", post(admin::resolve_flag))
        .route("/v1/growth/admin/journeys", get(journeys::list).post(journeys::create))
        .route("/v1/growth/admin/journeys/meta", get(journeys::meta))
        .route("/v1/growth/admin/journeys/{id}", get(journeys::get).patch(journeys::patch))
        .route("/v1/growth/admin/journeys/{id}/status", post(journeys::set_status))
        .route("/v1/growth/admin/journeys/{id}/enrollments", get(journeys::enrollments))
        .route("/v1/growth/admin/journeys/{id}/events", get(journeys::events))
        .route("/v1/growth/admin/journeys/{id}/test", post(journeys::test))
        .route("/v1/growth/admin/reports", get(admin::reports))
        .route("/v1/growth/admin/audit", get(admin::audit))
        .route("/v1/growth/admin/run/{job}", post(admin::run_job))
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
    Json(json!({"status": if db { "ok" } else { "degraded" }, "db": db, "service": "growth"}))
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

/// The signed-in client (Client Area BFF), with the segment hints the BFF read from the gateway session.
pub struct UserCtx {
    pub tenant: String,
    pub user_id: i64,
    pub hints: Hints,
}

impl<S: Send + Sync> FromRequestParts<S> for UserCtx {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        let user_id = header(parts, "x-kalks-user-id").and_then(|v| v.parse::<i64>().ok()).filter(|v| *v > 0).ok_or(ApiError::Unauthorized)?;
        let clip = |v: String, n: usize| -> String { v.chars().take(n).collect() };
        let name = header(parts, "x-kalks-name").map(|n| clip(percent_decode(&n), 80));
        let (first, last) = match name.as_deref().map(str::trim) {
            Some(n) if !n.is_empty() => match n.split_once(' ') {
                Some((f, l)) => (Some(f.to_string()), Some(l.trim().to_string())),
                None => (Some(n.to_string()), Some(String::new())),
            },
            _ => (None, None),
        };
        let hints = Hints {
            country: header(parts, "x-kalks-country").filter(|c| c.len() == 2 && c.chars().all(|x| x.is_ascii_alphabetic())),
            kyc: header(parts, "x-kalks-kyc").filter(|k| matches!(k.as_str(), "unverified" | "pending" | "verified" | "rejected")),
            created_at: header(parts, "x-kalks-created-at").and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok()).map(|t| t.with_timezone(&chrono::Utc)),
            first_name: first,
            last_name: last,
            referral_code: header(parts, "x-kalks-referral-code").filter(|c| c.len() <= 24 && c.chars().all(|x| x.is_ascii_alphanumeric())),
        };
        Ok(UserCtx { tenant: tenant_of(parts)?, user_id, hints })
    }
}

/// Back Office staff, as verified by the admin BFF.
pub struct StaffCtx {
    pub tenant: String,
    pub id: String,
    pub name: String,
    pub role: String,
    /// `marketing.*` permissions the admin BFF resolved from the gateway RBAC (`X-Kalks-Staff-Perms`). When
    /// present they decide; the role lists are the fallback.
    pub perms: Option<Vec<String>>,
}

/// Roles that may change marketing configuration (campaigns, promos, banners, contests, rules, catalogue).
pub const ROLES_WRITE: &[&str] = &["platform_owner", "super_admin", "admin", "marketing"];
/// Roles that may move money out (prizes, manual grants and cancellations, points adjustments, cashback runs).
pub const ROLES_APPROVE: &[&str] = &["platform_owner", "super_admin", "admin", "finance"];
/// Roles that may read the Marketing module.
pub const ROLES_READ: &[&str] = &["platform_owner", "super_admin", "admin", "marketing", "finance", "compliance", "support", "risk_manager", "partner_manager", "viewer"];

impl StaffCtx {
    pub fn require(&self, roles: &[&str]) -> ApiResult<()> {
        let perm = if roles == ROLES_APPROVE {
            "marketing.approve"
        } else if roles == ROLES_WRITE {
            "marketing.write"
        } else {
            "marketing.read"
        };
        let ok = match &self.perms {
            Some(p) => p.iter().any(|x| x == perm),
            None => roles.contains(&self.role.as_str()),
        };
        if ok { Ok(()) } else { Err(ApiError::Forbidden("Your role doesn't allow this.".into())) }
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
        let perms = header(parts, "x-kalks-staff-perms").map(|p| p.split(',').map(|x| x.trim().to_string()).filter(|x| x.starts_with("marketing.")).collect());
        Ok(StaffCtx { tenant: tenant_of(parts)?, id, name: name.chars().take(80).collect(), role, perms })
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

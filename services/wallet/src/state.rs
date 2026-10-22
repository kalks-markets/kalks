//! Shared state and request context (tenant, client ip / user agent, calling service, staff identity).

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::chain::{Chain, ChainId};
use crate::config::Config;
use crate::engine::Engine;
use crate::error::ApiError;
use crate::users::Users;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    pub chains: Arc<HashMap<ChainId, Arc<dyn Chain>>>,
    pub engine: Arc<dyn Engine>,
    pub users: Arc<dyn Users>,
    pub tenants: Tenants,
    /// Serialises the settle step of wallet <-> trading transfers per process (request path vs recovery loop).
    pub transfer_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn chain(&self, id: ChainId) -> Result<Arc<dyn Chain>, ApiError> {
        self.chains.get(&id).cloned().ok_or_else(|| ApiError::unprocessable("chain_disabled", format!("{} is not available", id.network())))
    }
}

#[derive(Clone, Debug)]
pub struct Tenant {
    pub id: i64,
    pub slug: String,
}

/// slug → tenant id, loaded from the `tenants` table (reloaded on a miss).
#[derive(Clone, Default)]
pub struct Tenants(Arc<RwLock<HashMap<String, i64>>>);

impl Tenants {
    pub async fn load(pool: &PgPool) -> anyhow::Result<Self> {
        let t = Self::default();
        t.reload(pool).await?;
        Ok(t)
    }
    pub async fn reload(&self, pool: &PgPool) -> anyhow::Result<()> {
        let rows: Vec<(String, i64)> = sqlx::query_as("SELECT slug, id FROM tenants").fetch_all(pool).await?;
        *self.0.write().unwrap() = rows.into_iter().collect();
        Ok(())
    }
    pub fn get(&self, slug: &str) -> Option<i64> {
        self.0.read().unwrap().get(slug).copied()
    }
    pub fn slug_of(&self, id: i64) -> Option<String> {
        self.0.read().unwrap().iter().find(|(_, v)| **v == id).map(|(k, _)| k.clone())
    }
    pub fn all(&self) -> Vec<(i64, String)> {
        self.0.read().unwrap().iter().map(|(k, v)| (*v, k.clone())).collect()
    }

    /// A broker the Platform Owner created in the gateway (D110) gets its wallet rows on its first request: the
    /// tenant (the id mirrors the gateway's) and default limits. No company addresses are copied: deposits stay
    /// off until the broker sets its own in Wallet settings. Unknown slugs are remembered for 30 s.
    pub async fn provision(&self, pool: &PgPool, users: &dyn crate::users::Users, slug: &str) -> Option<i64> {
        if slug.is_empty() || slug.len() > 64 || !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') || missed_recently(slug) {
            return None;
        }
        let (id, name) = match users.tenant(slug).await {
            Ok(Some(t)) => t,
            Ok(None) => {
                remember_miss(slug);
                return None;
            }
            Err(e) => {
                tracing::warn!(slug, error = %e, "tenant lookup failed");
                return None;
            }
        };
        let res: anyhow::Result<bool> = async {
            let mut tx = pool.begin().await?;
            let created = sqlx::query("INSERT INTO tenants (id, slug, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING").bind(id).bind(slug).bind(&name).execute(&mut *tx).await?.rows_affected() == 1;
            sqlx::query("INSERT INTO tenant_settings (tenant_id) VALUES ($1) ON CONFLICT DO NOTHING").bind(id).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(created)
        }
        .await;
        match res {
            Ok(true) => tracing::info!(tenant = id, slug, "broker provisioned in the wallet (no company addresses yet)"),
            Ok(false) => {}
            Err(e) => {
                tracing::error!(tenant = id, slug, error = %e, "broker could not be provisioned");
                return None;
            }
        }
        let _ = self.reload(pool).await;
        self.get(slug).filter(|got| *got == id)
    }
}

static MISSES: std::sync::LazyLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> = std::sync::LazyLock::new(Default::default);

fn missed_recently(slug: &str) -> bool {
    let mut m = MISSES.lock().unwrap();
    m.retain(|_, at| at.elapsed() < std::time::Duration::from_secs(30));
    m.contains_key(slug)
}

fn remember_miss(slug: &str) {
    let mut m = MISSES.lock().unwrap();
    if m.len() > 1000 {
        m.clear();
    }
    m.insert(slug.to_string(), std::time::Instant::now());
}

pub fn header(parts: &Parts, name: &str) -> Option<String> {
    parts.headers.get(name).and_then(|v| v.to_str().ok()).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

/// Tenant (`X-Kalks-Tenant`, default `kalks`), client ip, user agent and the calling service.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub tenant: Tenant,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// `X-Kalks-Service` (ib, prop, pamm, copy, crm, …), recorded as the actor of external transfers.
    pub service: Option<String>,
}

impl Ctx {
    pub fn actor(&self) -> String {
        format!("service:{}", self.service.as_deref().unwrap_or("internal"))
    }
}

impl FromRequestParts<AppState> for Ctx {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, Self::Rejection> {
        let slug = header(parts, "x-kalks-tenant").unwrap_or_else(|| "kalks".into()).to_lowercase();
        let id = match st.tenants.get(&slug) {
            Some(id) => id,
            None => {
                let _ = st.tenants.reload(&st.pool).await;
                match st.tenants.get(&slug) {
                    Some(id) => id,
                    None => st.tenants.provision(&st.pool, st.users.as_ref(), &slug).await.ok_or_else(|| ApiError::BadRequest(format!("Unknown tenant {slug}")))?,
                }
            }
        };
        let ip = header(parts, "x-forwarded-for").and_then(|v| v.split(',').next().map(|s| s.trim().to_string())).filter(|v| !v.is_empty() && v.len() <= 64);
        let user_agent = header(parts, "user-agent").map(|u| u.chars().take(400).collect());
        let service = header(parts, "x-kalks-service").filter(|s| s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
        Ok(Ctx { tenant: Tenant { id, slug }, ip, user_agent, service })
    }
}

/// Staff identity forwarded by the Back Office BFF after it verified the staff session with the gateway.
#[derive(Clone, Debug)]
pub struct Staff {
    pub id: String,
    pub name: String,
    pub role: String,
    /// `X-Kalks-Staff-Perms`: the staff member's gateway permission keys as forwarded by the admin BFF. None when
    /// the caller sent no list (older BFFs): then only the role is checked.
    pub perms: Option<Vec<String>>,
}

impl Staff {
    pub fn tag(&self) -> String {
        format!("staff:{}", self.id)
    }
}

pub struct StaffCtx {
    pub ctx: Ctx,
    pub staff: Staff,
}

pub const ROLES_READ: &[&str] = &["platform_owner", "super_admin", "admin", "finance", "compliance", "risk_manager"];
pub const ROLES_WRITE: &[&str] = &["platform_owner", "super_admin", "admin", "finance"];
pub const ROLES_APPROVE: &[&str] = &["platform_owner", "super_admin", "admin", "finance"];
pub const ROLES_SETTINGS: &[&str] = &["platform_owner", "super_admin", "admin"];
/// Forcing a trading-account deduction past the free margin (finance.adjust_force): Super Admin only.
pub const ROLES_FORCE: &[&str] = &["platform_owner", "super_admin"];

/// System roles (not editable, every permission): allowed whatever list was forwarded.
const ROLES_SYSTEM: &[&str] = &["platform_owner", "super_admin"];

impl StaffCtx {
    /// Whether the staff member holds `perm` (a gateway key, services/gateway/src/rbac.rs): the forwarded list when
    /// present, so custom roles get exactly what they were granted; else the role map (older BFFs).
    pub fn has_perm(&self, perm: &str, fallback_roles: &[&str]) -> bool {
        if ROLES_SYSTEM.contains(&self.staff.role.as_str()) {
            return true;
        }
        match &self.staff.perms {
            Some(p) => p.iter().any(|x| x == perm),
            None => fallback_roles.contains(&self.staff.role.as_str()),
        }
    }

    pub fn require_perm(&self, perm: &str, fallback_roles: &[&str]) -> Result<(), ApiError> {
        if self.has_perm(perm, fallback_roles) { Ok(()) } else { Err(ApiError::Forbidden(format!("Your role doesn't allow this ({perm})"))) }
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
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
        if id.len() > 64 || role.len() > 32 {
            return Err(ApiError::BadRequest("Invalid staff headers".into()));
        }
        let name = header(parts, "x-kalks-staff-name").map(|n| percent_decode(&n)).unwrap_or_else(|| format!("Staff {id}"));
        let perms = header(parts, "x-kalks-staff-perms").map(|v| v.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty() && p.len() <= 64).take(200).collect());
        Ok(StaffCtx { ctx, staff: Staff { id, name: name.chars().take(120).collect(), role, perms } })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn decodes_staff_names() {
        assert_eq!(super::percent_decode("Julia%20Novak"), "Julia Novak");
        assert_eq!(super::percent_decode("Jos%C3%A9"), "José");
    }
}

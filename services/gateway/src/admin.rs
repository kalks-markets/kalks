//! Back Office read APIs: /v1/admin/{stats,users,audit,staff,sessions}
//!
//! Every endpoint needs a live staff session (bearer token forwarded by the admin BFF) and a role that holds
//! the matching permission. All queries are scoped to the staff member's tenant. SQL is static: optional filters
//! are bound as NULLable parameters, never spliced into the query text.

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::error::{ApiError, ApiResult};
use crate::identity::{self, Kind};
use crate::state::{AppState, Ctx};

// ---------- permissions ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perm {
    StatsRead,
    ClientsRead,
    AuditRead,
    SessionsRead,
    SessionsRevoke,
    StaffRead,
    #[allow(dead_code)]
    SpreadsRead,
    SpreadsWrite,
    /// KYC queue, case files and documents (read).
    KycRead,
    /// KYC decisions: approve, reject, request more information, notes.
    KycReview,
}

impl Perm {
    pub fn as_str(self) -> &'static str {
        match self {
            Perm::StatsRead => "stats.read",
            Perm::ClientsRead => "clients.read",
            Perm::AuditRead => "audit.read",
            Perm::SessionsRead => "sessions.read",
            Perm::SessionsRevoke => "sessions.revoke",
            Perm::StaffRead => "staff.read",
            Perm::SpreadsRead => "spreads.read",
            Perm::SpreadsWrite => "spreads.write",
            Perm::KycRead => "kyc.read",
            Perm::KycReview => "kyc.review",
        }
    }
}

/// Whether a built-in role holds a permission by default (see `rbac::preset_perms`). Live checks use the staff
/// member's stored role (`Staff::can`), which a tenant may have customised.
#[cfg(test)]
pub fn role_allows(role: &str, p: Perm) -> bool {
    crate::rbac::preset_perms(role).is_some_and(|v| v.contains(&p.as_str()))
}

/// The staff member behind the request.
pub struct Staff {
    pub id: i64,
    pub tenant_id: i64,
    /// Role key (built-in key such as `dealer`, or a custom role key).
    pub role: String,
    pub session_id: i64,
    /// Effective permission keys of the role.
    pub perms: Vec<String>,
}

impl Staff {
    pub fn can(&self, key: &str) -> bool {
        self.perms.iter().any(|p| p == key)
    }
    pub fn is_owner(&self) -> bool {
        self.perms.iter().any(|p| crate::rbac::is_owner_perm(p))
    }
}

/// Resolves the live staff session: active staff, active tenant, role permissions, IP allow-list (D111).
/// 401 without a live session, 403 `ip_not_allowed` / `tenant_suspended` when blocked.
pub async fn current(st: &AppState, ctx: &Ctx) -> ApiResult<Staff> {
    let s = identity::resolve_session(st, ctx, Kind::Staff).await?;
    let row = sqlx::query(
        "SELECT s.status, s.role, COALESCE(r.id, 0) AS role_id, COALESCE(r.key, s.role) AS rkey, COALESCE(r.name, s.role) AS rname,
                COALESCE(r.kind, 'preset') AS rkind, COALESCE(r.customised, false) AS rcustom, COALESCE(r.permissions, '{}') AS rperms,
                t.status AS tstatus, t.ip_allowlist_enabled, t.ip_owner_bypass
         FROM staff s JOIN tenants t ON t.id = s.tenant_id LEFT JOIN roles r ON r.id = s.role_id
         WHERE s.id = $1 AND s.tenant_id = $2",
    )
    .bind(s.subject_id)
    .bind(s.tenant_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    if row.get::<String, _>("status") != "active" {
        return Err(ApiError::Unauthorized);
    }
    if row.get::<String, _>("tstatus") != "active" {
        return Err(crate::staff_admin::tenant_suspended());
    }
    let role: String = row.get("rkey");
    let kind: String = row.get("rkind");
    let perms = crate::rbac::effective(&kind, &role, row.get("rcustom"), &row.get::<Vec<String>, _>("rperms"));
    let me = Staff { id: s.subject_id, tenant_id: s.tenant_id, role, session_id: s.session_id, perms };
    if row.get::<bool, _>("ip_allowlist_enabled") && !(row.get::<bool, _>("ip_owner_bypass") && me.is_owner()) {
        crate::staff_admin::session_ip_check(st, ctx, &me).await?;
    }
    Ok(me)
}

/// Resolves the staff session and checks `perm`. 401 without a live session, 403 without the permission.
pub async fn require(st: &AppState, ctx: &Ctx, perm: Perm) -> ApiResult<Staff> {
    require_key(st, ctx, perm.as_str()).await
}

/// Same as `require`, by permission key (see `rbac::PERMS`).
pub async fn require_key(st: &AppState, ctx: &Ctx, key: &str) -> ApiResult<Staff> {
    let me = current(st, ctx).await?;
    if !me.can(key) {
        return Err(ApiError::Forbidden);
    }
    Ok(me)
}

// ---------- helpers ----------

/// Server time for "today" is GMT+3 (Postgres `Etc/GMT-3`).
const SERVER_TZ: &str = "Etc/GMT-3";

/// `%term%` for ILIKE with `%`, `_` and `\` escaped (Postgres' default LIKE escape is `\`).
pub fn like_pattern(raw: &str) -> Option<String> {
    let t: String = raw.trim().chars().take(100).collect();
    if t.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(t.len() + 2);
    out.push('%');
    for c in t.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    Some(out)
}

/// 1-based page + page size, clamped. Returns (page, per_page, offset).
pub fn paging(page: Option<i64>, per_page: Option<i64>, default: i64, max: i64) -> (i64, i64, i64) {
    let per = per_page.unwrap_or(default).clamp(1, max);
    let page = page.unwrap_or(1).clamp(1, 100_000);
    (page, per, (page - 1) * per)
}

fn clean(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

/// `YYYY-MM-DD` (a whole server-time day; `end = true` → the next midnight) or RFC 3339.
pub fn parse_when(raw: &str, end: bool) -> Result<DateTime<Utc>, &'static str> {
    let raw = raw.trim();
    if let Ok(d) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let tz = chrono::FixedOffset::east_opt(3 * 3600).expect("offset");
        let day = if end { d + Duration::days(1) } else { d };
        return tz.from_local_datetime(&day.and_time(NaiveTime::MIN)).single().map(|t| t.with_timezone(&Utc)).ok_or("Invalid date.");
    }
    DateTime::parse_from_rfc3339(raw).map(|t| t.with_timezone(&Utc)).map_err(|_| "Use a date like 2026-09-28.")
}

/// Audit action filter: `staff.*` / `staff.` → prefix, otherwise exact. Returns a LIKE pattern.
pub fn action_pattern(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() || t.len() > 80 || !t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '*' | '-')) {
        return None;
    }
    let esc = t.replace('_', "\\_");
    Some(if let Some(p) = esc.strip_suffix('*') { format!("{p}%") } else if esc.ends_with('.') { format!("{esc}%") } else { esc })
}

/// Audit actor filter: `staff`, `user`, `system`, `anonymous`, optionally `:<id>`.
pub fn parse_actor(raw: &str) -> Result<(Option<String>, Option<i64>), &'static str> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok((None, None));
    }
    let (kind, id) = match t.split_once(':') {
        Some((k, i)) => (k, Some(i.parse::<i64>().map_err(|_| "Invalid actor id.")?)),
        None => (t, None),
    };
    if !matches!(kind, "staff" | "user" | "system" | "anonymous") {
        return Err("Unknown actor type.");
    }
    Ok((Some(kind.to_string()), id))
}

/// Non-reversible short id for a session (first 6 bytes of the stored token HMAC).
pub fn fingerprint(token_hash: &[u8]) -> String {
    token_hash.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

fn q_err(_: QueryRejection) -> ApiError {
    ApiError::BadRequest("Invalid query parameters.")
}

fn ts(r: &PgRow, col: &str) -> Option<DateTime<Utc>> {
    r.get::<Option<DateTime<Utc>>, _>(col)
}

fn user_row(r: &PgRow) -> Value {
    let first: String = r.get("first_name");
    let last: String = r.get("last_name");
    let locked_until = ts(r, "locked_until");
    json!({
        "id": r.get::<i64, _>("id"),
        "email": r.get::<String, _>("email"),
        "first_name": first,
        "last_name": last,
        "name": format!("{first} {last}"),
        "phone_dial": r.get::<String, _>("phone_dial"),
        "phone": r.get::<String, _>("phone"),
        "country": r.get::<String, _>("country").trim().to_lowercase(),
        "date_of_birth": r.get::<NaiveDate, _>("date_of_birth"),
        "referral_code": r.get::<String, _>("referral_code"),
        "referred_by": r.get::<Option<i64>, _>("referred_by"),
        "kyc_status": r.get::<String, _>("kyc_status"),
        "status": r.get::<String, _>("status"),
        // personal data erased (client_erase.rs): the profile fields are placeholders
        "erased_at": ts(r, "erased_at"),
        "email_verified": r.get::<Option<DateTime<Utc>>, _>("email_verified_at").is_some(),
        "email_verified_at": ts(r, "email_verified_at"),
        "locked": locked_until.is_some_and(|t| t > Utc::now()),
        "locked_until": locked_until,
        "last_login_at": ts(r, "last_login_at"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
        "active_sessions": r.get::<i64, _>("active_sessions"),
        // presence and restrictions (client_controls.rs)
        "last_active_at": ts(r, "last_active_at"),
        "presence": crate::client_controls::presence(ts(r, "last_active_at"), Utc::now()).as_str(),
        "apps": r.try_get::<Option<Vec<String>>, _>("apps").ok().flatten().unwrap_or_default(),
        "restrictions": r.try_get::<Option<Vec<String>>, _>("restrictions").ok().flatten().unwrap_or_default(),
    })
}

/// Idle windows (seconds) that make a session count as active, per kind. Client sessions are further limited
/// by their tenant's `client_idle_minutes` (see `live_sql`), so the user bound here is the longest allowed window.
fn idle_secs(kind: Kind) -> f64 {
    match kind {
        Kind::Staff => identity::policy(kind).session_idle.num_seconds() as f64,
        Kind::User => (10080 * 60) as f64,
    }
}

/// Replaces `{LIVE}` in `sql` with a "session `se` is live" predicate whose idle windows (staff, user) are bound
/// as parameters `$n` and `$n+1`.
fn live_sql(sql: &str, n: usize) -> String {
    let live = format!(
        "se.revoked_at IS NULL AND se.expires_at > now()
         AND se.last_seen_at > now() - make_interval(secs => CASE WHEN se.subject_kind = 'staff' THEN ${}::float8
             ELSE LEAST(${}::float8, (SELECT tt.client_idle_minutes * 60 FROM tenants tt WHERE tt.id = se.tenant_id)::float8) END)",
        n,
        n + 1
    );
    sql.replace("{LIVE}", &live)
}

// ---------- GET /v1/admin/stats ----------

pub async fn stats(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require(&st, &ctx, Perm::StatsRead).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(live_sql(
        "SELECT
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house) AS clients_total,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND email_verified_at IS NOT NULL) AS email_verified,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND kyc_status = 'verified') AS kyc_verified,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND kyc_status = 'pending') AS kyc_pending,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND (created_at AT TIME ZONE $2)::date = (now() AT TIME ZONE $2)::date) AS registered_today,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND created_at > now() - interval '7 days') AS registered_7d,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND created_at > now() - interval '30 days') AS registered_30d,
            (SELECT count(*) FROM sessions se WHERE se.tenant_id = $1 AND se.subject_kind = 'user' AND {LIVE}) AS sessions_user,
            (SELECT count(*) FROM sessions se WHERE se.tenant_id = $1 AND se.subject_kind = 'staff' AND {LIVE}) AS sessions_staff,
            (SELECT count(*) FROM staff WHERE tenant_id = $1 AND status = 'active') AS staff_active,
            (SELECT count(*) FROM audit_log WHERE tenant_id = $1 AND action = 'user.login' AND created_at > now() - interval '24 hours') AS logins_24h,
            (SELECT count(*) FROM audit_log WHERE tenant_id = $1 AND action IN ('user.login_failed', 'staff.login_failed', 'user.locked', 'staff.locked') AND created_at > now() - interval '24 hours') AS failed_logins_24h,
            (SELECT count(*) FROM audit_log WHERE tenant_id = $1 AND created_at > now() - interval '24 hours') AS audit_24h,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND last_active_at > now() - interval '2 minutes') AS online_now,
            (SELECT count(*) FROM users WHERE tenant_id = $1 AND NOT is_house AND last_active_at <= now() - interval '2 minutes' AND last_active_at > now() - interval '15 minutes') AS away_now,
            (SELECT count(DISTINCT user_id) FROM client_restrictions WHERE tenant_id = $1 AND lifted_at IS NULL AND (expires_at IS NULL OR expires_at > now())) AS restricted",
        3,
    )))
    .bind(me.tenant_id)
    .bind(SERVER_TZ)
    .bind(idle_secs(Kind::Staff))
    .bind(idle_secs(Kind::User))
    .fetch_one(&st.pool)
    .await?;
    let n = |c: &str| r.get::<i64, _>(c);

    // registrations per server-time day, last 14 days (oldest first, zero-filled)
    let series = sqlx::query(
        "SELECT d::date AS day, count(u.id) AS n
         FROM generate_series((now() AT TIME ZONE $2)::date - 13, (now() AT TIME ZONE $2)::date, interval '1 day') d
         LEFT JOIN users u ON u.tenant_id = $1 AND (u.created_at AT TIME ZONE $2)::date = d::date
         GROUP BY d ORDER BY d",
    )
    .bind(me.tenant_id)
    .bind(SERVER_TZ)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| json!({ "day": r.get::<NaiveDate, _>("day"), "count": r.get::<i64, _>("n") }))
    .collect::<Vec<_>>();

    Ok(Json(json!({
        "clients": {
            "total": n("clients_total"),
            "email_verified": n("email_verified"),
            "kyc_verified": n("kyc_verified"),
            "kyc_pending": n("kyc_pending"),
            "registered_today": n("registered_today"),
            "registered_7d": n("registered_7d"),
            "registered_30d": n("registered_30d"),
            "online": n("online_now"),
            "away": n("away_now"),
            "restricted": n("restricted"),
        },
        "sessions": { "clients": n("sessions_user"), "staff": n("sessions_staff") },
        "staff": { "active": n("staff_active") },
        "security": { "logins_24h": n("logins_24h"), "failed_logins_24h": n("failed_logins_24h"), "audit_events_24h": n("audit_24h") },
        "registrations": series,
        "generated_at": Utc::now(),
    })))
}

// ---------- GET /v1/admin/users ----------

#[derive(Deserialize, Default)]
pub struct UsersQuery {
    pub q: Option<String>,
    pub kyc: Option<String>,
    pub verified: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    /// Bulk export (CSV): needs `clients.export`, allows 200 rows a page and is audited once per export (page 1).
    pub export: Option<bool>,
    /// `online` | `away` | `offline` | `active` (online or away), client_controls.rs.
    pub presence: Option<String>,
    /// `true`: only clients with an active restriction; `false`: only clients without.
    pub restricted: Option<String>,
    /// `online`: most recently active first (default: newest registration first).
    pub sort: Option<String>,
}

/// Page size cap for the client list: browsing reads at most 100 rows a page; exports (`clients.export`) 200.
pub fn users_page_cap(export: bool) -> i64 {
    if export { 200 } else { 100 }
}

const USER_COLS: &str = "u.id, u.email, u.first_name, u.last_name, u.phone_dial, u.phone, u.country, u.date_of_birth, u.referral_code,
     u.referred_by, u.kyc_status, u.status, u.email_verified_at, u.locked_until, u.last_login_at, u.created_at, u.last_active_at, u.erased_at,
     (SELECT array_agg(cr.kind ORDER BY cr.kind) FROM client_restrictions cr
       WHERE cr.user_id = u.id AND cr.lifted_at IS NULL AND (cr.expires_at IS NULL OR cr.expires_at > now())) AS restrictions,
     CASE WHEN u.last_active_at > now() - interval '15 minutes' THEN array_remove(ARRAY[
       CASE WHEN EXISTS (SELECT 1 FROM sessions sa WHERE sa.subject_kind = 'user' AND sa.subject_id = u.id AND sa.viewer_id IS NULL
              AND sa.impersonator_id IS NULL AND sa.revoked_at IS NULL AND sa.expires_at > now() AND sa.last_seen_at > now() - interval '15 minutes')
            THEN 'client_area' END,
       CASE WHEN EXISTS (SELECT 1 FROM client_presence cp WHERE cp.user_id = u.id AND cp.ended_at IS NULL AND cp.last_active > now() - interval '60 seconds')
            THEN 'trader' END], NULL) END AS apps";

pub async fn users(State(st): State<AppState>, ctx: Ctx, q: Result<Query<UsersQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(q_err)?;
    let export = q.export.unwrap_or(false);
    let me = require_key(&st, &ctx, if export { "clients.export" } else { Perm::ClientsRead.as_str() }).await?;
    let (page, per, offset) = paging(q.page, q.per_page, 25, users_page_cap(export));
    let kyc = clean(&q.kyc).filter(|k| *k != "all");
    if let Some(k) = kyc
        && !matches!(k, "unverified" | "pending" | "verified" | "rejected")
    {
        return Err(ApiError::BadRequest("Unknown KYC status."));
    }
    let verified = match clean(&q.verified) {
        None | Some("all") => None,
        Some("true" | "1" | "yes") => Some(true),
        Some("false" | "0" | "no") => Some(false),
        _ => return Err(ApiError::BadRequest("verified must be true or false.")),
    };
    let status = clean(&q.status).filter(|s| *s != "all");
    if let Some(s) = status
        && !matches!(s, "active" | "blocked" | "closed")
    {
        return Err(ApiError::BadRequest("Unknown status."));
    }
    let term = clean(&q.q);
    let pat = term.and_then(like_pattern);
    let id_eq = term.and_then(|t| t.trim_start_matches('#').parse::<i64>().ok());
    let presence = clean(&q.presence).filter(|p| *p != "all");
    if let Some(p) = presence
        && !matches!(p, "online" | "away" | "offline" | "active")
    {
        return Err(ApiError::BadRequest("Unknown presence."));
    }
    let restricted = match clean(&q.restricted) {
        None | Some("all") => None,
        Some("true" | "1" | "yes") => Some(true),
        Some("false" | "0" | "no") => Some(false),
        _ => return Err(ApiError::BadRequest("restricted must be true or false.")),
    };
    let by_activity = clean(&q.sort) == Some("online");

    let sql = live_sql(&format!(
        "SELECT {USER_COLS},
            (SELECT count(*) FROM sessions se WHERE se.subject_kind = 'user' AND se.subject_id = u.id AND {{LIVE}}) AS active_sessions,
            count(*) OVER () AS total
         FROM users u
         WHERE u.tenant_id = $1 AND NOT u.is_house
           AND ($2::text IS NULL OR u.email ILIKE $2 OR (u.first_name || ' ' || u.last_name) ILIKE $2
                OR (u.phone_dial || u.phone) ILIKE $2 OR u.phone ILIKE $2 OR u.referral_code ILIKE $2 OR u.id = $3)
           AND ($4::text IS NULL OR u.kyc_status = $4)
           AND ($5::bool IS NULL OR (u.email_verified_at IS NOT NULL) = $5)
           AND ($6::text IS NULL OR u.status = $6)
           AND ($9::text IS NULL OR CASE $9
                 WHEN 'online' THEN u.last_active_at > now() - make_interval(secs => $12)
                 WHEN 'away' THEN u.last_active_at <= now() - make_interval(secs => $12) AND u.last_active_at > now() - make_interval(secs => $13)
                 WHEN 'active' THEN u.last_active_at > now() - make_interval(secs => $13)
                 ELSE u.last_active_at IS NULL OR u.last_active_at <= now() - make_interval(secs => $13) END)
           AND ($10::bool IS NULL OR EXISTS (SELECT 1 FROM client_restrictions cr WHERE cr.user_id = u.id AND cr.lifted_at IS NULL
                                              AND (cr.expires_at IS NULL OR cr.expires_at > now())) = $10)
         ORDER BY CASE WHEN $11 THEN u.last_active_at END DESC NULLS LAST, u.created_at DESC, u.id DESC
         LIMIT $7 OFFSET $8"
    ), 14);
    // defense in depth: the listing runs under the tenant's RLS scope (kalks_tenant role), not only `WHERE tenant_id`
    let mut tx = crate::domains::tenant_tx(&st.pool, me.tenant_id).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(me.tenant_id)
        .bind(pat)
        .bind(id_eq)
        .bind(kyc)
        .bind(verified)
        .bind(status)
        .bind(per)
        .bind(offset)
        .bind(presence)
        .bind(restricted)
        .bind(by_activity)
        .bind(crate::client_controls::ONLINE_SECS as f64)
        .bind(crate::client_controls::AWAY_SECS as f64)
        .bind(idle_secs(Kind::Staff))
        .bind(idle_secs(Kind::User))
        .fetch_all(&mut *tx)
        .await?;
    tx.commit().await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    if export && page == 1 {
        let meta = json!({"q": term, "kyc": kyc, "verified": verified, "status": status, "rows": total});
        audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "clients.exported", target: None, meta }).await;
    }
    Ok(Json(json!({ "items": rows.iter().map(user_row).collect::<Vec<_>>(), "total": total, "page": page, "per_page": per })))
}

// ---------- GET /v1/admin/users/{id} ----------

pub async fn user_detail(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require(&st, &ctx, Perm::ClientsRead).await?;
    let sql = live_sql(&format!(
        "SELECT {USER_COLS}, u.terms_accepted_at, u.failed_logins, u.updated_at,
            (SELECT count(*) FROM sessions se WHERE se.subject_kind = 'user' AND se.subject_id = u.id AND {{LIVE}}) AS active_sessions,
            (SELECT count(*) FROM sessions se WHERE se.subject_kind = 'user' AND se.subject_id = u.id) AS sessions_total,
            (SELECT count(*) FROM trusted_devices td WHERE td.subject_kind = 'user' AND td.subject_id = u.id) AS trusted_devices,
            (SELECT count(*) FROM users r WHERE r.referred_by = u.id) AS referrals_total
         FROM users u WHERE u.id = $1 AND u.tenant_id = $2"
    ), 3);
    // defense in depth: the client's record and history are read under the tenant's RLS scope
    let mut tx = crate::domains::tenant_tx(&st.pool, me.tenant_id).await?;
    let r = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(me.tenant_id)
        .bind(idle_secs(Kind::Staff))
        .bind(idle_secs(Kind::User))
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let mut user = user_row(&r);
    user["terms_accepted_at"] = json!(ts(&r, "terms_accepted_at"));
    user["failed_logins"] = json!(r.get::<i32, _>("failed_logins"));
    user["updated_at"] = json!(ts(&r, "updated_at"));
    user["referred_code_raw"] = Value::Null;

    let referrer = match r.get::<Option<i64>, _>("referred_by") {
        Some(rid) => sqlx::query("SELECT id, email, first_name, last_name, referral_code FROM users WHERE id = $1 AND tenant_id = $2")
            .bind(rid)
            .bind(me.tenant_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(|x| {
                json!({
                    "id": x.get::<i64, _>("id"),
                    "email": x.get::<String, _>("email"),
                    "name": format!("{} {}", x.get::<String, _>("first_name"), x.get::<String, _>("last_name")),
                    "referral_code": x.get::<String, _>("referral_code"),
                })
            }),
        None => None,
    };
    let code_raw: Option<String> = sqlx::query_scalar("SELECT referred_code_raw FROM users WHERE id = $1").bind(id).fetch_one(&mut *tx).await?;
    user["referred_code_raw"] = json!(code_raw);

    let referrals = sqlx::query(
        "SELECT id, email, first_name, last_name, kyc_status, email_verified_at IS NOT NULL AS verified, created_at
         FROM users WHERE referred_by = $1 AND tenant_id = $2 ORDER BY created_at DESC LIMIT 50",
    )
    .bind(id)
    .bind(me.tenant_id)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|x| {
        json!({
            "id": x.get::<i64, _>("id"),
            "email": x.get::<String, _>("email"),
            "name": format!("{} {}", x.get::<String, _>("first_name"), x.get::<String, _>("last_name")),
            "kyc_status": x.get::<String, _>("kyc_status"),
            "email_verified": x.get::<bool, _>("verified"),
            "created_at": x.get::<DateTime<Utc>, _>("created_at"),
        })
    })
    .collect::<Vec<_>>();

    let last_login = sqlx::query(
        "SELECT created_at, ip, user_agent, meta->>'via' AS via FROM audit_log
         WHERE tenant_id = $1 AND actor_kind = 'user' AND actor_id = $2 AND action = 'user.login' ORDER BY id DESC LIMIT 1",
    )
    .bind(me.tenant_id)
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?
    .map(|x| json!({ "at": x.get::<DateTime<Utc>, _>("created_at"), "ip": x.get::<Option<String>, _>("ip"), "user_agent": x.get::<Option<String>, _>("user_agent"), "via": x.get::<Option<String>, _>("via") }));

    let events = sqlx::query(
        "SELECT id, actor_kind, actor_id, action, target_kind, target_id, ip, user_agent, meta, created_at FROM audit_log
         WHERE tenant_id = $1 AND ((actor_kind = 'user' AND actor_id = $2) OR (target_kind = 'user' AND target_id = $2))
         ORDER BY id DESC LIMIT 30",
    )
    .bind(me.tenant_id)
    .bind(id)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(|x| {
        json!({
            "id": x.get::<i64, _>("id"),
            "actor": { "kind": x.get::<String, _>("actor_kind"), "id": x.get::<Option<i64>, _>("actor_id") },
            "action": x.get::<String, _>("action"),
            "target": { "kind": x.get::<Option<String>, _>("target_kind"), "id": x.get::<Option<i64>, _>("target_id") },
            "ip": x.get::<Option<String>, _>("ip"),
            "user_agent": x.get::<Option<String>, _>("user_agent"),
            "meta": x.get::<sqlx::types::Json<Value>, _>("meta").0,
            "created_at": x.get::<DateTime<Utc>, _>("created_at"),
        })
    })
    .collect::<Vec<_>>();
    tx.commit().await?;

    Ok(Json(json!({
        "user": user,
        "referrer": referrer,
        "referrals": { "total": r.get::<i64, _>("referrals_total"), "items": referrals },
        "sessions": { "active": r.get::<i64, _>("active_sessions"), "total": r.get::<i64, _>("sessions_total") },
        "trusted_devices": r.get::<i64, _>("trusted_devices"),
        "last_login": last_login,
        "attribution": crate::marketing::attribution_json(&st.pool, me.tenant_id, id).await?,
        "events": events,
    })))
}

// ---------- GET /v1/admin/audit ----------

#[derive(Deserialize, Default)]
pub struct AuditQuery {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub q: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
    /// Bulk export (CSV): needs `audit.export`, allows 1000 rows a page and is itself audited (page 1).
    pub export: Option<bool>,
}

pub async fn audit_log(State(st): State<AppState>, ctx: Ctx, q: Result<Query<AuditQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(q_err)?;
    let export = q.export.unwrap_or(false);
    let me = require_key(&st, &ctx, if export { "audit.export" } else { Perm::AuditRead.as_str() }).await?;
    let (page, per, offset) = paging(q.page, q.per_page, 50, if export { 1000 } else { 200 });
    let (actor_kind, actor_id) = parse_actor(q.actor.as_deref().unwrap_or("")).map_err(ApiError::BadRequest)?;
    let action = match clean(&q.action) {
        None => None,
        Some(a) => Some(action_pattern(a).ok_or(ApiError::BadRequest("Invalid action filter."))?),
    };
    let from = clean(&q.from).map(|v| parse_when(v, false)).transpose().map_err(ApiError::BadRequest)?;
    let to = clean(&q.to).map(|v| parse_when(v, true)).transpose().map_err(ApiError::BadRequest)?;
    let pat = clean(&q.q).and_then(like_pattern);

    let rows = sqlx::query(
        "SELECT a.id, a.actor_kind, a.actor_id, a.action, a.target_kind, a.target_id, a.ip, a.user_agent, a.meta, a.created_at,
                COALESCE(au.first_name || ' ' || au.last_name, ast.name) AS actor_name, COALESCE(au.email, ast.email) AS actor_email,
                COALESCE(ar.name, ast.role) AS actor_role,
                COALESCE(tu.first_name || ' ' || tu.last_name, tst.name) AS target_name, COALESCE(tu.email, tst.email) AS target_email,
                count(*) OVER () AS total
         FROM audit_log a
         LEFT JOIN users au ON a.actor_kind = 'user' AND au.id = a.actor_id
         LEFT JOIN staff ast ON a.actor_kind = 'staff' AND ast.id = a.actor_id
         LEFT JOIN roles ar ON ar.id = ast.role_id
         LEFT JOIN users tu ON a.target_kind = 'user' AND tu.id = a.target_id
         LEFT JOIN staff tst ON a.target_kind = 'staff' AND tst.id = a.target_id
         WHERE a.tenant_id = $1
           AND ($2::text IS NULL OR a.actor_kind = $2)
           AND ($3::bigint IS NULL OR a.actor_id = $3)
           AND ($4::text IS NULL OR a.action LIKE $4)
           AND ($5::text IS NULL OR a.action ILIKE $5 OR a.ip ILIKE $5 OR a.meta::text ILIKE $5
                OR au.email ILIKE $5 OR ast.email ILIKE $5 OR ast.name ILIKE $5 OR (au.first_name || ' ' || au.last_name) ILIKE $5
                OR tu.email ILIKE $5 OR tst.email ILIKE $5)
           AND ($6::timestamptz IS NULL OR a.created_at >= $6)
           AND ($7::timestamptz IS NULL OR a.created_at < $7)
         ORDER BY a.id DESC
         LIMIT $8 OFFSET $9",
    )
    .bind(me.tenant_id)
    .bind(actor_kind)
    .bind(actor_id)
    .bind(action)
    .bind(pat)
    .bind(from)
    .bind(to)
    .bind(per)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items = rows
        .iter()
        .map(|x| {
            let role: Option<String> = x.get("actor_role");
            json!({
                "id": x.get::<i64, _>("id"),
                "actor": {
                    "kind": x.get::<String, _>("actor_kind"),
                    "id": x.get::<Option<i64>, _>("actor_id"),
                    "name": x.get::<Option<String>, _>("actor_name"),
                    "email": x.get::<Option<String>, _>("actor_email"),
                    "role_label": role.map(|r| crate::rbac::builtin(&r).map(|b| b.name.to_string()).unwrap_or(r)),
                },
                "action": x.get::<String, _>("action"),
                "target": {
                    "kind": x.get::<Option<String>, _>("target_kind"),
                    "id": x.get::<Option<i64>, _>("target_id"),
                    "name": x.get::<Option<String>, _>("target_name"),
                    "email": x.get::<Option<String>, _>("target_email"),
                },
                "ip": x.get::<Option<String>, _>("ip"),
                "user_agent": x.get::<Option<String>, _>("user_agent"),
                "meta": x.get::<sqlx::types::Json<Value>, _>("meta").0,
                "created_at": x.get::<DateTime<Utc>, _>("created_at"),
            })
        })
        .collect::<Vec<_>>();
    let actions: Vec<String> = sqlx::query_scalar("SELECT DISTINCT action FROM audit_log WHERE tenant_id = $1 ORDER BY 1 LIMIT 300")
        .bind(me.tenant_id)
        .fetch_all(&st.pool)
        .await?;
    if export && page == 1 {
        let meta = json!({"actor": q.actor, "action": q.action, "q": q.q, "from": q.from, "to": q.to, "rows": total});
        audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "audit.exported", target: None, meta }).await;
    }
    Ok(Json(json!({ "items": items, "total": total, "page": page, "per_page": per, "actions": actions })))
}

// ---------- GET /v1/admin/staff ----------

pub async fn staff_list(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require(&st, &ctx, Perm::StaffRead).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(live_sql(
        "SELECT s.id, s.email, s.name, s.role, s.status, s.locked_until, s.last_login_at, s.created_at, s.invited_at, s.disabled_at,
            s.disabled_reason, r.id AS role_id, COALESCE(r.name, s.role) AS role_name, COALESCE(r.kind, 'preset') AS role_kind,
            (SELECT count(*) FROM sessions se WHERE se.subject_kind = 'staff' AND se.subject_id = s.id AND {LIVE}) AS active_sessions,
            (SELECT count(*) FROM trusted_devices td WHERE td.subject_kind = 'staff' AND td.subject_id = s.id) AS trusted_devices,
            (SELECT max(created_at) FROM audit_log a WHERE a.actor_kind = 'staff' AND a.actor_id = s.id) AS last_activity_at,
            (SELECT max(expires_at) FROM staff_invites i WHERE i.staff_id = s.id AND i.accepted_at IS NULL AND i.revoked_at IS NULL) AS invite_expires_at
         FROM staff s LEFT JOIN roles r ON r.id = s.role_id WHERE s.tenant_id = $1
         ORDER BY CASE s.role WHEN 'platform_owner' THEN 0 WHEN 'super_admin' THEN 1 WHEN 'admin' THEN 2 ELSE 3 END, s.name",
        2,
    )))
    .bind(me.tenant_id)
    .bind(idle_secs(Kind::Staff))
    .bind(idle_secs(Kind::User))
    .fetch_all(&st.pool)
    .await?;
    let items = rows
        .iter()
        .map(|x| {
            let role: String = x.get("role");
            let locked_until = ts(x, "locked_until");
            json!({
                "id": x.get::<i64, _>("id"),
                "email": x.get::<String, _>("email"),
                "name": x.get::<String, _>("name"),
                "role": role,
                "role_id": x.get::<Option<i64>, _>("role_id"),
                "role_label": x.get::<String, _>("role_name"),
                "role_kind": x.get::<String, _>("role_kind"),
                "status": x.get::<String, _>("status"),
                "invited_at": ts(x, "invited_at"),
                "invite_expires_at": ts(x, "invite_expires_at"),
                "disabled_at": ts(x, "disabled_at"),
                "disabled_reason": x.get::<Option<String>, _>("disabled_reason"),
                "locked": locked_until.is_some_and(|t| t > Utc::now()),
                "last_login_at": ts(x, "last_login_at"),
                "last_activity_at": ts(x, "last_activity_at"),
                "created_at": x.get::<DateTime<Utc>, _>("created_at"),
                "active_sessions": x.get::<i64, _>("active_sessions"),
                "trusted_devices": x.get::<i64, _>("trusted_devices"),
                "is_me": x.get::<i64, _>("id") == me.id,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "items": items, "total": items.len() })))
}

// ---------- GET /v1/admin/sessions ----------

#[derive(Deserialize, Default)]
pub struct SessionsQuery {
    pub kind: Option<String>,
    pub subject: Option<i64>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

pub async fn sessions(State(st): State<AppState>, ctx: Ctx, q: Result<Query<SessionsQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(q_err)?;
    let kind = clean(&q.kind).filter(|k| *k != "all");
    if let Some(k) = kind
        && !matches!(k, "staff" | "user")
    {
        return Err(ApiError::BadRequest("kind must be staff or user."));
    }
    // a client's own sessions (client detail page) only need clients.read
    let perm = if kind == Some("user") && q.subject.is_some() { Perm::ClientsRead } else { Perm::SessionsRead };
    let me = require(&st, &ctx, perm).await?;
    let (page, per, offset) = paging(q.page, q.per_page, 50, 200);
    let rows = sqlx::query(sqlx::AssertSqlSafe(live_sql(
        "SELECT se.id, se.subject_kind, se.subject_id, se.token_hash, se.ip, se.user_agent, se.created_at, se.last_seen_at, se.expires_at,
                se.country, se.viewer_id, cv.label AS viewer_label,
                COALESCE(u.first_name || ' ' || u.last_name, s.name) AS name, COALESCE(u.email, s.email) AS email, COALESCE(sr.name, s.role) AS role,
                count(*) OVER () AS total
         FROM sessions se
         LEFT JOIN client_viewers cv ON cv.id = se.viewer_id
         LEFT JOIN users u ON se.subject_kind = 'user' AND u.id = se.subject_id
         LEFT JOIN staff s ON se.subject_kind = 'staff' AND s.id = se.subject_id
         LEFT JOIN roles sr ON sr.id = s.role_id
         WHERE se.tenant_id = $1 AND ($2::text IS NULL OR se.subject_kind = $2) AND ($3::bigint IS NULL OR se.subject_id = $3) AND {LIVE}
         ORDER BY se.last_seen_at DESC
         LIMIT $4 OFFSET $5",
        6,
    )))
    .bind(me.tenant_id)
    .bind(kind)
    .bind(q.subject)
    .bind(per)
    .bind(offset)
    .bind(idle_secs(Kind::Staff))
    .bind(idle_secs(Kind::User))
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items = rows
        .iter()
        .map(|x| {
            let role: Option<String> = x.get("role");
            let sid: i64 = x.get("id");
            json!({
                "id": sid,
                "fingerprint": fingerprint(&x.get::<Vec<u8>, _>("token_hash")),
                "subject": {
                    "kind": x.get::<String, _>("subject_kind"),
                    "id": x.get::<i64, _>("subject_id"),
                    "name": x.get::<Option<String>, _>("name"),
                    "email": x.get::<Option<String>, _>("email"),
                    "role_label": role.map(|r| crate::rbac::builtin(&r).map(|b| b.name.to_string()).unwrap_or(r)),
                },
                "ip": x.get::<Option<String>, _>("ip"),
                "country": x.get::<Option<String>, _>("country"),
                "user_agent": x.get::<Option<String>, _>("user_agent"),
                "created_at": x.get::<DateTime<Utc>, _>("created_at"),
                "last_seen_at": x.get::<DateTime<Utc>, _>("last_seen_at"),
                "expires_at": x.get::<DateTime<Utc>, _>("expires_at"),
                "viewer": x.get::<Option<i64>, _>("viewer_id").map(|v| json!({ "id": v, "label": x.get::<Option<String>, _>("viewer_label") })),
                "current": sid == me.session_id,
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "items": items, "total": total, "page": page, "per_page": per })))
}

// ---------- POST /v1/admin/sessions/{id}/revoke ----------

#[derive(Deserialize, Default)]
pub struct RevokeReq {
    pub reason: Option<String>,
}

pub async fn revoke_session(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<RevokeReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let me = require(&st, &ctx, Perm::SessionsRevoke).await?;
    let reason: Option<String> = match req {
        Ok(Json(r)) => r.reason.map(|s| s.trim().chars().take(300).collect::<String>()).filter(|s| !s.is_empty()),
        Err(_) => None,
    };
    if id == me.session_id {
        return Err(ApiError::BadRequest("This is your current session. Use Sign out instead."));
    }
    let target = sqlx::query(
        "SELECT se.subject_kind, se.subject_id, se.revoked_at IS NOT NULL AS revoked, s.role
         FROM sessions se LEFT JOIN staff s ON se.subject_kind = 'staff' AND s.id = se.subject_id
         WHERE se.id = $1 AND se.tenant_id = $2",
    )
    .bind(id)
    .bind(me.tenant_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let kind: String = target.get("subject_kind");
    let subject_id: i64 = target.get("subject_id");
    if target.get::<Option<String>, _>("role").as_deref() == Some("platform_owner") && !me.is_owner() {
        return Err(ApiError::Forbidden);
    }
    if target.get::<bool, _>("revoked") {
        return Ok(Json(json!({ "status": "ok", "already_revoked": true })));
    }
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND tenant_id = $2 AND revoked_at IS NULL")
        .bind(id)
        .bind(me.tenant_id)
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &ctx, Entry {
        tenant_id: me.tenant_id,
        actor_kind: "staff",
        actor_id: Some(me.id),
        action: "admin.session_revoked",
        target: Some((if kind == "staff" { "staff" } else { "user" }, subject_id)),
        meta: json!({ "session_id": id, "subject_kind": kind, "reason": reason }),
    })
    .await;
    Ok(Json(json!({ "status": "ok" })))
}

// ---------- POST /v1/admin/audit/record ----------

#[derive(Deserialize)]
pub struct RecordReq {
    pub action: String,
    pub reason: String,
    #[serde(default)]
    pub meta: Value,
}

/// Actions the Back Office BFF may record for changes it makes in other services (e.g. market-data spreads).
fn recordable(action: &str) -> Option<Perm> {
    match action {
        "spreads.update" => Some(Perm::SpreadsWrite),
        _ => None,
    }
}

pub async fn record(State(st): State<AppState>, ctx: Ctx, req: Result<Json<RecordReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let perm = recordable(&r.action).ok_or(ApiError::BadRequest("Unknown action."))?;
    let me = require(&st, &ctx, perm).await?;
    let reason = r.reason.trim();
    if reason.chars().count() < 3 || reason.chars().count() > 300 {
        return Err(ApiError::Validation { field: "reason", message: "Give a reason (3–300 characters)." });
    }
    if !(r.meta.is_object() || r.meta.is_null()) || r.meta.to_string().len() > 4096 {
        return Err(ApiError::BadRequest("meta must be a small JSON object."));
    }
    let mut meta = if r.meta.is_null() { json!({}) } else { r.meta };
    meta["reason"] = json!(reason);
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: &r.action, target: None, meta }).await;
    Ok(Json(json!({ "status": "ok" })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_matrix() {
        for p in [Perm::StatsRead, Perm::AuditRead, Perm::SessionsRevoke, Perm::KycReview, Perm::SpreadsWrite] {
            assert!(role_allows("platform_owner", p));
            assert!(role_allows("super_admin", p));
        }
        assert!(role_allows("compliance", Perm::AuditRead));
        assert!(!role_allows("compliance", Perm::SessionsRevoke));
        assert!(!role_allows("support", Perm::AuditRead));
        assert!(role_allows("support", Perm::ClientsRead));
        assert!(role_allows("dealer", Perm::SpreadsWrite));
        assert!(!role_allows("viewer", Perm::SpreadsWrite));
        assert!(!role_allows("marketing", Perm::ClientsRead));
        assert!(role_allows("compliance", Perm::KycReview));
        assert!(!role_allows("support", Perm::KycRead));
        assert!(!role_allows("dealer", Perm::KycReview));
        assert!(!role_allows("nonsense", Perm::StatsRead));
    }

    #[test]
    fn like_patterns_are_escaped() {
        assert_eq!(like_pattern("  "), None);
        assert_eq!(like_pattern("ann"), Some("%ann%".into()));
        assert_eq!(like_pattern("50%_off\\"), Some("%50\\%\\_off\\\\%".into()));
        assert_eq!(like_pattern(&"x".repeat(500)).unwrap().len(), 102);
    }

    #[test]
    fn paging_is_clamped() {
        assert_eq!(paging(None, None, 25, 200), (1, 25, 0));
        assert_eq!(paging(Some(3), Some(10), 25, 200), (3, 10, 20));
        assert_eq!(paging(Some(-4), Some(100_000), 25, 200), (1, 200, 0));
        assert_eq!(paging(Some(2), Some(0), 25, 200), (2, 1, 1));
    }

    #[test]
    fn action_and_actor_filters() {
        assert_eq!(action_pattern("staff.login").as_deref(), Some("staff.login"));
        assert_eq!(action_pattern("staff.*").as_deref(), Some("staff.%"));
        assert_eq!(action_pattern("user.").as_deref(), Some("user.%"));
        assert_eq!(action_pattern("user.login_failed").as_deref(), Some("user.login\\_failed"));
        assert_eq!(action_pattern("x'; drop"), None);
        assert_eq!(parse_actor("").unwrap(), (None, None));
        assert_eq!(parse_actor("staff:7").unwrap(), (Some("staff".into()), Some(7)));
        assert_eq!(parse_actor("user").unwrap(), (Some("user".into()), None));
        assert!(parse_actor("root").is_err());
        assert!(parse_actor("staff:x").is_err());
    }

    #[test]
    fn dates_use_server_time() {
        // 2026-09-28 00:00 GMT+3 = 2026-09-27 21:00 UTC
        assert_eq!(parse_when("2026-09-28", false).unwrap().to_rfc3339(), "2026-09-27T21:00:00+00:00");
        // `to` is inclusive of the whole day
        assert_eq!(parse_when("2026-09-28", true).unwrap().to_rfc3339(), "2026-09-28T21:00:00+00:00");
        assert!(parse_when("2026-09-28T10:00:00Z", false).is_ok());
        assert!(parse_when("yesterday", false).is_err());
    }

    #[test]
    fn fingerprints_are_short_hex() {
        assert_eq!(fingerprint(&[0xab, 0x01, 0xff, 0, 1, 2, 3, 4]), "ab01ff000102");
    }

    #[test]
    fn only_known_actions_can_be_recorded() {
        assert_eq!(recordable("spreads.update"), Some(Perm::SpreadsWrite));
        assert_eq!(recordable("staff.login"), None);
    }
}

/// Database-backed tests. They use `GATEWAY_TEST_DATABASE_URL` (default: the local dev cluster, database
/// `kalks_core_test`, created and migrated on first run) and skip when no database is reachable.
#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::config::Config;
    use crate::crypto::Keys;
    use std::sync::Arc;

    async fn test_state() -> Option<AppState> {
        let url = std::env::var("GATEWAY_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/kalks_core_test".into());
        let pool = match tokio::time::timeout(std::time::Duration::from_secs(5), crate::db::connect(&url)).await {
            Ok(Ok(p)) => p,
            _ => {
                eprintln!("skipping admin db tests: no database at GATEWAY_TEST_DATABASE_URL");
                return None;
            }
        };
        let secret = "t".repeat(40);
        let cfg = Config {
            bind: String::new(),
            database_url: url,
            session_secret: secret.clone(),
            internal_token: String::new(),
            dev_mode: true,
            smtp_configured: false,
            smtp_host: String::new(),
            smtp_port: 0,
            smtp_user: String::new(),
            smtp_password: String::new(),
            smtp_from: String::new(),
            site_url: String::new(),
            app_url: String::new(),
            trade_url: String::new(),
            support_email: String::new(),
            staff_otp_every_login: true,
            super_admin_email: String::new(),
            super_admin_password: String::new(),
            super_admin_name: String::new(),
        };
        Some(AppState { pool, keys: Keys::new(&secret), cfg: Arc::new(cfg), limiter: Default::default(), mailer: None })
    }

    fn ctx(bearer: Option<&str>) -> Ctx {
        Ctx { ip: "203.0.113.9".into(), user_agent: "test-agent".into(), device: None, tenant_slug: "kalks".into(), bearer: bearer.map(str::to_string) }
    }

    async fn tenant(st: &AppState, tag: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO tenants (slug, name) VALUES ($1, $1) RETURNING id").bind(format!("t-{tag}")).fetch_one(&st.pool).await.unwrap()
    }

    async fn staff(st: &AppState, tenant: i64, tag: &str, role: &str) -> i64 {
        sqlx::query_scalar("INSERT INTO staff (tenant_id, email, password_hash, name, role) VALUES ($1,$2,'x',$3,$4) RETURNING id")
            .bind(tenant)
            .bind(format!("{role}-{tag}@example.com"))
            .bind(format!("Staff {role}"))
            .bind(role)
            .fetch_one(&st.pool)
            .await
            .unwrap()
    }

    async fn client(st: &AppState, tenant: i64, email: &str, first: &str, verified: bool, referred_by: Option<i64>) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth,
                                referral_code, referred_by, email_verified_at, terms_accepted_at)
             VALUES ($1,$2,'secret-hash',$3,'Tester','+91','9876543210','IN','1990-01-01',$4,$5, CASE WHEN $6 THEN now() END, now()) RETURNING id",
        )
        .bind(tenant)
        .bind(email)
        .bind(first)
        .bind(format!("R{}", crate::crypto::random_token(6)))
        .bind(referred_by)
        .bind(verified)
        .fetch_one(&st.pool)
        .await
        .unwrap()
    }

    async fn session(st: &AppState, kind: Kind, tenant: i64, id: i64) -> String {
        identity::create_session(st, &ctx(None), kind, tenant, id).await.unwrap().token
    }

    #[tokio::test]
    async fn admin_endpoints_end_to_end() {
        let Some(st) = test_state().await else { return };
        let tag = crate::crypto::random_token(6).to_lowercase().replace(['-', '_'], "x");
        let t1 = tenant(&st, &tag).await;
        let t2 = tenant(&st, &format!("{tag}b")).await;
        let owner = staff(&st, t1, &tag, "platform_owner").await;
        let support = staff(&st, t1, &tag, "support").await;
        let owner_tok = session(&st, Kind::Staff, t1, owner).await;
        let support_tok = session(&st, Kind::Staff, t1, support).await;

        let alice = client(&st, t1, &format!("alice-{tag}@example.com"), "Alice", true, None).await;
        let bob = client(&st, t1, &format!("bob-{tag}@example.com"), "Bob", false, Some(alice)).await;
        let _other = client(&st, t2, &format!("carol-{tag}@example.com"), "Carol", true, None).await;
        let bob_tok = session(&st, Kind::User, t1, bob).await;
        audit::record(&st.pool, &ctx(None), Entry { tenant_id: t1, actor_kind: "user", actor_id: Some(bob), action: "user.login", target: None, meta: json!({"via": "password"}) }).await;

        // no session / wrong role
        assert!(matches!(stats(State(st.clone()), ctx(None)).await, Err(ApiError::Unauthorized)));
        assert!(matches!(audit_log(State(st.clone()), ctx(Some(&support_tok)), Ok(Query(AuditQuery::default()))).await, Err(ApiError::Forbidden)));
        assert!(matches!(staff_list(State(st.clone()), ctx(Some(&support_tok))).await, Err(ApiError::Forbidden)));

        // users: tenant-scoped, no secrets, filters
        let Json(v) = users(State(st.clone()), ctx(Some(&support_tok)), Ok(Query(UsersQuery::default()))).await.unwrap();
        assert_eq!(v["total"], 2);
        let text = v.to_string();
        assert!(!text.contains("secret-hash") && !text.contains("password"));
        assert!(!text.contains("carol-"));
        let Json(v) = users(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(UsersQuery { q: Some("ALICE".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 1);
        assert_eq!(v["items"][0]["id"], alice);
        let Json(v) = users(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(UsersQuery { verified: Some("false".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["items"][0]["id"], bob);
        assert_eq!(v["items"][0]["active_sessions"], 1);
        let Json(v) = users(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(UsersQuery { q: Some("%".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 0, "LIKE wildcards are escaped");

        // browsing is capped at 100 a page; bulk export needs clients.export (support has only clients.read) and is audited
        let Json(v) = users(State(st.clone()), ctx(Some(&support_tok)), Ok(Query(UsersQuery { per_page: Some(500), ..Default::default() }))).await.unwrap();
        assert_eq!(v["per_page"], 100);
        let export = || Ok(Query(UsersQuery { per_page: Some(500), export: Some(true), ..Default::default() }));
        assert!(matches!(users(State(st.clone()), ctx(Some(&support_tok)), export()).await, Err(ApiError::Forbidden)));
        let Json(v) = users(State(st.clone()), ctx(Some(&owner_tok)), export()).await.unwrap();
        assert_eq!((v["per_page"].as_i64(), v["total"].as_i64()), (Some(200), Some(2)));
        let exported: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE tenant_id = $1 AND action = 'clients.exported' AND actor_id = $2")
            .bind(t1)
            .bind(owner)
            .fetch_one(&st.pool)
            .await
            .unwrap();
        assert_eq!(exported, 1);

        // user detail: referral + last login + sessions
        let Json(v) = user_detail(State(st.clone()), ctx(Some(&owner_tok)), Path(bob)).await.unwrap();
        assert_eq!(v["referrer"]["id"], alice);
        assert_eq!(v["sessions"]["active"], 1);
        assert_eq!(v["last_login"]["ip"], "203.0.113.9");
        let Json(v) = user_detail(State(st.clone()), ctx(Some(&owner_tok)), Path(alice)).await.unwrap();
        assert_eq!(v["referrals"]["total"], 1);
        assert!(matches!(user_detail(State(st.clone()), ctx(Some(&owner_tok)), Path(_other)).await, Err(ApiError::NotFound)));

        // stats
        let Json(v) = stats(State(st.clone()), ctx(Some(&support_tok))).await.unwrap();
        assert_eq!(v["clients"]["total"], 2);
        assert_eq!(v["clients"]["email_verified"], 1);
        assert_eq!(v["clients"]["registered_today"], 2);
        assert_eq!(v["sessions"]["clients"], 1);
        assert_eq!(v["sessions"]["staff"], 2);
        assert_eq!(v["registrations"].as_array().unwrap().len(), 14);

        // staff list
        let Json(v) = staff_list(State(st.clone()), ctx(Some(&owner_tok))).await.unwrap();
        assert_eq!(v["total"], 2);
        assert_eq!(v["items"][0]["role"], "platform_owner");
        assert_eq!(v["items"][0]["is_me"], true);

        // sessions + revoke (audited)
        let Json(v) = sessions(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(SessionsQuery { kind: Some("user".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 1);
        let sid = v["items"][0]["id"].as_i64().unwrap();
        assert_eq!(v["items"][0]["fingerprint"].as_str().unwrap().len(), 12);
        assert!(matches!(
            revoke_session(State(st.clone()), ctx(Some(&support_tok)), Path(sid), Ok(Json(RevokeReq::default()))).await,
            Err(ApiError::Forbidden)
        ));
        let _ = revoke_session(State(st.clone()), ctx(Some(&owner_tok)), Path(sid), Ok(Json(RevokeReq { reason: Some("suspicious".into()) }))).await.unwrap();
        assert!(identity::resolve_session(&st, &ctx(Some(&bob_tok)), Kind::User).await.is_err());
        // browsing is capped at 200 a page; the CSV export needs audit.export and is recorded
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { per_page: Some(5000), ..Default::default() }))).await.unwrap();
        assert_eq!(v["per_page"], 200);
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { per_page: Some(5000), export: Some(true), ..Default::default() }))).await.unwrap();
        assert_eq!(v["per_page"], 1000);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE tenant_id = $1 AND action = 'audit.exported'").bind(t1).fetch_one(&st.pool).await.unwrap();
        assert_eq!(n, 1);
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { action: Some("admin.*".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 1);
        assert_eq!(v["items"][0]["action"], "admin.session_revoked");
        assert_eq!(v["items"][0]["target"]["id"], bob);
        assert_eq!(v["items"][0]["meta"]["reason"], "suspicious");
        assert_eq!(v["items"][0]["actor"]["role_label"], "Platform Owner");

        // own current session can't be revoked here; other tenant's sessions are invisible
        let Json(mine) = sessions(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(SessionsQuery { kind: Some("staff".into()), ..Default::default() }))).await.unwrap();
        let cur = mine["items"].as_array().unwrap().iter().find(|s| s["current"] == true).unwrap()["id"].as_i64().unwrap();
        assert!(matches!(revoke_session(State(st.clone()), ctx(Some(&owner_tok)), Path(cur), Ok(Json(RevokeReq::default()))).await, Err(ApiError::BadRequest(_))));
        let t2_owner = staff(&st, t2, &tag, "platform_owner").await;
        let t2_tok = session(&st, Kind::Staff, t2, t2_owner).await;
        assert!(matches!(revoke_session(State(st.clone()), ctx(Some(&t2_tok)), Path(sid), Ok(Json(RevokeReq::default()))).await, Err(ApiError::NotFound)));

        // audit filters
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { actor: Some(format!("user:{bob}")), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 1);
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { q: Some(format!("bob-{tag}")), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 2);
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { from: Some("2000-01-01".into()), to: Some("2000-01-02".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["total"], 0);

        // audit record: only allow-listed actions, needs a reason and the permission
        let rec = |tok: &str, action: &str, reason: &str| {
            let st = st.clone();
            let c = ctx(Some(tok));
            let body = RecordReq { action: action.into(), reason: reason.into(), meta: json!({"group_code": "pro", "symbol": "*", "before": {"markup_points": 3}, "after": {"markup_points": 4}}) };
            async move { record(State(st), c, Ok(Json(body))).await }
        };
        assert!(matches!(rec(&owner_tok, "staff.login", "because").await, Err(ApiError::BadRequest(_))));
        assert!(matches!(rec(&support_tok, "spreads.update", "because").await, Err(ApiError::Forbidden)));
        assert!(matches!(rec(&owner_tok, "spreads.update", "").await, Err(ApiError::Validation { .. })));
        let _ = rec(&owner_tok, "spreads.update", "Tighten pro").await.unwrap();
        let Json(v) = audit_log(State(st.clone()), ctx(Some(&owner_tok)), Ok(Query(AuditQuery { action: Some("spreads.update".into()), ..Default::default() }))).await.unwrap();
        assert_eq!(v["items"][0]["meta"]["reason"], "Tighten pro");
    }
}

//! Staff administration and Back Office security (D108, D109, D111):
//!
//! * staff: detail, invite by email (link → set password → email code), edit name / role, disable / enable,
//!   reset 2FA (trusted devices), force sign-out, resend invite
//! * roles: role builder (custom roles, editable presets), reset preset, delete custom role
//! * IP allow-list per tenant for staff sign-in and every staff request, with an optional owner bypass
//!
//! Every change is written to the append-only audit log with before / after values. A staff member can only
//! grant permissions they hold themselves and can't manage anyone with more access than they have.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::admin::{Staff, current, require_key};
use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::error::{ApiError, ApiResult, field};
use crate::identity::{self, Kind};
use crate::rbac;
use crate::state::{AppState, Ctx};
use crate::validate;

pub const INVITE_TTL_HOURS: i64 = 72;
/// Password hash stored until an invite is accepted (never a valid PHC string, so no password matches).
pub const NO_PASSWORD: &str = "!invited";

pub fn tenant_suspended() -> ApiError {
    ApiError::Coded { status: StatusCode::FORBIDDEN, code: "tenant_suspended", message: "This broker's workspace is suspended. Contact the platform owner." }
}

pub fn ip_not_allowed() -> ApiError {
    ApiError::Coded {
        status: StatusCode::FORBIDDEN,
        code: "ip_not_allowed",
        message: "Back Office access from this network isn't allowed. Ask your Super Admin to add your IP address to the allow-list.",
    }
}

fn denied(message: &'static str) -> ApiError {
    ApiError::Coded { status: StatusCode::FORBIDDEN, code: "forbidden", message }
}

fn conflict(code: &'static str, message: &'static str) -> ApiError {
    ApiError::Coded { status: StatusCode::CONFLICT, code, message }
}

fn subset(perms: &[String], of: &[String]) -> bool {
    perms.iter().all(|p| of.contains(p))
}

fn staff_entry<'a>(me: &Staff, action: &'a str, target: Option<(&'a str, i64)>, meta: Value) -> Entry<'a> {
    Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action, target, meta }
}

// ---------- IP allow-list enforcement ----------

pub async fn allowlist(st: &AppState, tenant_id: i64) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar("SELECT cidr FROM tenant_ip_allowlist WHERE tenant_id = $1").bind(tenant_id).fetch_all(&st.pool).await?)
}

/// Every staff request (called from `admin::current` when the tenant's allow-list is on).
pub async fn session_ip_check(st: &AppState, ctx: &Ctx, me: &Staff) -> ApiResult<()> {
    if rbac::ip_allowed(&ctx.ip, &allowlist(st, me.tenant_id).await?) {
        return Ok(());
    }
    // audit once per session and IP every 10 minutes, not on every request
    if st.limiter.hit(&format!("ipblock:{}:{}", me.session_id, ctx.ip), 1, std::time::Duration::from_secs(600)).is_ok() {
        audit::record(&st.pool, ctx, staff_entry(me, "security.ip_blocked", Some(("staff", me.id)), json!({"ip": ctx.ip, "stage": "session", "session_id": me.session_id}))).await;
    }
    Err(ip_not_allowed())
}

pub enum Who<'a> {
    Email(&'a str),
    Id(i64),
}

/// Sign-in gate: runs before the password check (so a blocked network gets no password oracle) and again
/// before a session is issued.
pub async fn login_ip_check(st: &AppState, ctx: &Ctx, tenant_id: i64, who: Who<'_>) -> ApiResult<()> {
    let t = sqlx::query("SELECT ip_allowlist_enabled, ip_owner_bypass FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(&st.pool).await?;
    if !t.get::<bool, _>("ip_allowlist_enabled") || rbac::ip_allowed(&ctx.ip, &allowlist(st, tenant_id).await?) {
        return Ok(());
    }
    let row = match who {
        Who::Email(e) => sqlx::query(
            "SELECT s.id, COALESCE(r.key, s.role) AS rkey, COALESCE(r.kind, 'preset') AS rkind, COALESCE(r.customised, false) AS rc, COALESCE(r.permissions, '{}') AS rp
             FROM staff s LEFT JOIN roles r ON r.id = s.role_id WHERE s.tenant_id = $1 AND s.email = $2",
        )
        .bind(tenant_id)
        .bind(e),
        Who::Id(id) => sqlx::query(
            "SELECT s.id, COALESCE(r.key, s.role) AS rkey, COALESCE(r.kind, 'preset') AS rkind, COALESCE(r.customised, false) AS rc, COALESCE(r.permissions, '{}') AS rp
             FROM staff s LEFT JOIN roles r ON r.id = s.role_id WHERE s.tenant_id = $1 AND s.id = $2",
        )
        .bind(tenant_id)
        .bind(id),
    }
    .fetch_optional(&st.pool)
    .await?;
    let (staff_id, owner) = match &row {
        Some(r) => {
            let perms = rbac::effective(r.get("rkind"), r.get("rkey"), r.get("rc"), &r.get::<Vec<String>, _>("rp"));
            (Some(r.get::<i64, _>("id")), perms.iter().any(|p| rbac::is_owner_perm(p)))
        }
        None => (None, false),
    };
    if owner && t.get::<bool, _>("ip_owner_bypass") {
        return Ok(());
    }
    audit::record(&st.pool, ctx, Entry {
        tenant_id,
        actor_kind: if staff_id.is_some() { "staff" } else { "anonymous" },
        actor_id: staff_id,
        action: "security.ip_blocked",
        target: staff_id.map(|id| ("staff", id)),
        meta: json!({"ip": ctx.ip, "stage": "sign_in", "email": match who { Who::Email(e) => Some(e), Who::Id(_) => None }}),
    })
    .await;
    Err(ip_not_allowed())
}

// ---------- staff ----------

struct Target {
    id: i64,
    email: String,
    name: String,
    status: String,
    role_id: Option<i64>,
    role_key: String,
    role_name: String,
    perms: Vec<String>,
    password_set: bool,
}

async fn target(st: &AppState, me: &Staff, id: i64) -> ApiResult<Target> {
    let r = sqlx::query(
        "SELECT s.id, s.email, s.name, s.status, s.password_hash <> $3 AS password_set, r.id AS role_id, COALESCE(r.key, s.role) AS rkey,
                COALESCE(r.name, s.role) AS rname, COALESCE(r.kind, 'preset') AS rkind, COALESCE(r.customised, false) AS rc, COALESCE(r.permissions, '{}') AS rp
         FROM staff s LEFT JOIN roles r ON r.id = s.role_id WHERE s.id = $1 AND s.tenant_id = $2",
    )
    .bind(id)
    .bind(me.tenant_id)
    .bind(NO_PASSWORD)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let key: String = r.get("rkey");
    Ok(Target {
        id: r.get("id"),
        email: r.get("email"),
        name: r.get("name"),
        status: r.get("status"),
        role_id: r.get("role_id"),
        perms: rbac::effective(r.get("rkind"), &key, r.get("rc"), &r.get::<Vec<String>, _>("rp")),
        role_key: key,
        role_name: r.get("rname"),
        password_set: r.get("password_set"),
    })
}

/// A staff member may manage someone only if they hold every permission that person has.
fn can_manage(me: &Staff, t: &Target) -> ApiResult<()> {
    if t.id == me.id {
        return Err(ApiError::BadRequest("You can't change your own account here."));
    }
    let above = match t.role_key.as_str() {
        "platform_owner" => !me.is_owner(),
        // Super Admins are managed by Super Admins and the Platform Owner only (D108)
        "super_admin" => !(me.is_owner() || me.role == "super_admin"),
        _ => false,
    };
    if above || !subset(&t.perms, &me.perms) {
        return Err(denied("You can't manage a staff member who has more access than you."));
    }
    Ok(())
}

/// A role can be assigned only if the assigner holds all of its permissions; the owner role never.
async fn assignable_role(st: &AppState, me: &Staff, role_id: i64) -> ApiResult<rbac::RoleInfo> {
    let role = rbac::role_by_id(&st.pool, me.tenant_id, role_id).await?.ok_or(ApiError::Validation { field: "role_id", message: "Choose a role." })?;
    if role.key == "platform_owner" {
        return Err(denied("The Platform Owner role can't be assigned."));
    }
    if role.key == "super_admin" && !(me.is_owner() || me.role == "super_admin") {
        return Err(denied("Only a Super Admin can make someone a Super Admin."));
    }
    if !subset(&role.perms, &me.perms) {
        return Err(denied("You can only assign roles whose permissions you hold yourself."));
    }
    Ok(role)
}

fn admin_url() -> String {
    std::env::var("PUBLIC_ADMIN_URL").ok().filter(|v| !v.trim().is_empty()).unwrap_or_else(|| "https://admin.kalkstrade.com".into()).trim_end_matches('/').to_string()
}

/// Creates a fresh invite (older pending ones are revoked) and emails the link. Returns (token, expires_at).
async fn issue_invite(st: &AppState, me: &Staff, staff_id: i64, email: &str, name: &str, role: &str) -> ApiResult<(String, DateTime<Utc>)> {
    create_invite(st, me.tenant_id, me.id, staff_id, email, name, role).await
}

/// Invite for any tenant (the Platform Owner invites a new tenant's first Super Admin).
pub async fn create_invite(st: &AppState, tenant_id: i64, inviter_id: i64, staff_id: i64, email: &str, name: &str, role: &str) -> ApiResult<(String, DateTime<Utc>)> {
    let token = crate::crypto::random_token(32);
    let expires_at = Utc::now() + Duration::hours(INVITE_TTL_HOURS);
    sqlx::query("UPDATE staff_invites SET revoked_at = now() WHERE staff_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL").bind(staff_id).execute(&st.pool).await?;
    sqlx::query("INSERT INTO staff_invites (tenant_id, staff_id, token_hash, created_by, expires_at) VALUES ($1,$2,$3,$4,$5)")
        .bind(tenant_id)
        .bind(staff_id)
        .bind(st.keys.hash("invite", &token))
        .bind(inviter_id)
        .bind(expires_at)
        .execute(&st.pool)
        .await?;
    let tenant: String = sqlx::query_scalar("SELECT name FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(&st.pool).await?;
    let inviter: String = sqlx::query_scalar("SELECT name FROM staff WHERE id = $1").bind(inviter_id).fetch_one(&st.pool).await?;
    let url = format!("{}/invite/{token}", admin_url());
    if let Some(m) = st.mailer.clone() {
        let (to, name, role, url) = (email.to_string(), name.to_string(), role.to_string(), url.clone());
        tokio::spawn(async move {
            if let Err(e) = m.send_staff_invite(&to, &name, &inviter, &tenant, &role, &url, INVITE_TTL_HOURS).await {
                tracing::error!(to = %validate::mask_email(&to), error = %e, "staff invite email could not be sent");
            }
        });
    } else if st.cfg.dev_mode {
        tracing::info!(target: "otp", to = %email, %url, "DEV staff invite (SMTP not configured)");
    } else {
        tracing::error!(to = %validate::mask_email(email), "cannot deliver staff invite: SMTP not configured");
    }
    Ok((token, expires_at))
}

pub fn invite_json(st: &AppState, token: &str, expires_at: DateTime<Utc>) -> Value {
    let mut v = json!({ "expires_at": expires_at, "email_sent": st.mailer.is_some() });
    if st.cfg.dev_mode && st.mailer.is_none() {
        v["dev_invite_path"] = json!(format!("/invite/{token}"));
    }
    v
}

#[derive(Deserialize)]
pub struct InviteReq {
    #[serde(default)]
    email: String,
    #[serde(default)]
    name: String,
    role_id: Option<i64>,
}

pub async fn invite(State(st): State<AppState>, ctx: Ctx, req: Result<Json<InviteReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "staff.write").await?;
    let email = validate::email(&r.email).map_err(field("email"))?;
    let name = validate::name(&r.name, "Enter the staff member's name.").map_err(field("name"))?;
    let role = assignable_role(&st, &me, r.role_id.ok_or(ApiError::Validation { field: "role_id", message: "Choose a role." })?).await?;
    identity::limit(&st, format!("staff-invite:{}", me.id), 30, 3600)?;
    let existing: Option<String> = sqlx::query_scalar("SELECT status FROM staff WHERE tenant_id = $1 AND email = $2").bind(me.tenant_id).bind(&email).fetch_optional(&st.pool).await?;
    match existing.as_deref() {
        Some("invited") => return Err(conflict("already_invited", "This person already has a pending invite. Resend it from their row instead.")),
        Some(_) => return Err(conflict("staff_exists", "A staff account with this email already exists.")),
        None => {}
    }
    let (count, max): (i64, Option<i64>) = {
        let row = sqlx::query(
            "SELECT (SELECT count(*) FROM staff WHERE tenant_id = $1 AND status IN ('active', 'invited')) AS n,
                    (SELECT (limits->>'max_staff')::bigint FROM tenants WHERE id = $1) AS max",
        )
        .bind(me.tenant_id)
        .fetch_one(&st.pool)
        .await?;
        (row.get("n"), row.get("max"))
    };
    if let Some(max) = max
        && max > 0
        && count >= max
    {
        return Err(conflict("limit_reached", "Your plan's staff limit is reached. Disable an account or ask the platform owner to raise the limit."));
    }
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO staff (tenant_id, email, password_hash, name, role, role_id, status, invited_by, invited_at)
         VALUES ($1,$2,$3,$4,$5,$6,'invited',$7, now()) RETURNING id",
    )
    .bind(me.tenant_id)
    .bind(&email)
    .bind(NO_PASSWORD)
    .bind(&name)
    .bind(&role.key)
    .bind(role.id)
    .bind(me.id)
    .fetch_one(&st.pool)
    .await?;
    let (token, expires_at) = issue_invite(&st, &me, id, &email, &name, &role.name).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "staff.invited", Some(("staff", id)), json!({"email": email, "name": name, "role": role.key, "role_name": role.name}))).await;
    Ok((StatusCode::CREATED, Json(json!({ "staff": { "id": id, "email": email, "name": name, "status": "invited", "role_id": role.id, "role_label": role.name }, "invite": invite_json(&st, &token, expires_at) }))))
}

pub async fn resend_invite(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.write").await?;
    let t = target(&st, &me, id).await?;
    can_manage(&me, &t)?;
    if t.status != "invited" {
        return Err(ApiError::BadRequest("This person has already accepted their invite."));
    }
    identity::limit(&st, format!("staff-invite:{}", me.id), 30, 3600)?;
    let (token, expires_at) = issue_invite(&st, &me, id, &t.email, &t.name, &t.role_name).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "staff.invite_resent", Some(("staff", id)), json!({"email": t.email}))).await;
    Ok(Json(json!({ "status": "ok", "invite": invite_json(&st, &token, expires_at) })))
}

pub async fn detail(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.read").await?;
    let t = target(&st, &me, id).await?;
    let r = sqlx::query(
        "SELECT s.created_at, s.last_login_at, s.locked_until, s.invited_at, s.disabled_at, s.disabled_reason, iv.name AS invited_by
         FROM staff s LEFT JOIN staff iv ON iv.id = s.invited_by WHERE s.id = $1",
    )
    .bind(id)
    .fetch_one(&st.pool)
    .await?;
    let idle = identity::policy(Kind::Staff).session_idle.num_seconds() as f64;
    let sessions = sqlx::query(
        "SELECT id, ip, user_agent, created_at, last_seen_at, expires_at FROM sessions
         WHERE subject_kind = 'staff' AND subject_id = $1 AND revoked_at IS NULL AND expires_at > now()
           AND last_seen_at > now() - make_interval(secs => $2::float8)
         ORDER BY last_seen_at DESC",
    )
    .bind(id)
    .bind(idle)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|x| {
        let sid: i64 = x.get("id");
        json!({ "id": sid, "ip": x.get::<Option<String>, _>("ip"), "user_agent": x.get::<Option<String>, _>("user_agent"),
                "created_at": x.get::<DateTime<Utc>, _>("created_at"), "last_seen_at": x.get::<DateTime<Utc>, _>("last_seen_at"),
                "expires_at": x.get::<DateTime<Utc>, _>("expires_at"), "current": sid == me.session_id })
    })
    .collect::<Vec<_>>();
    let devices = sqlx::query("SELECT user_agent, created_at, last_seen_at FROM trusted_devices WHERE subject_kind = 'staff' AND subject_id = $1 ORDER BY last_seen_at DESC")
        .bind(id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|x| json!({ "user_agent": x.get::<Option<String>, _>("user_agent"), "created_at": x.get::<DateTime<Utc>, _>("created_at"), "last_seen_at": x.get::<DateTime<Utc>, _>("last_seen_at") }))
        .collect::<Vec<_>>();
    let events = sqlx::query(
        "SELECT id, actor_kind, actor_id, action, target_kind, target_id, ip, meta, created_at FROM audit_log
         WHERE tenant_id = $1 AND ((actor_kind = 'staff' AND actor_id = $2) OR (target_kind = 'staff' AND target_id = $2))
         ORDER BY id DESC LIMIT 25",
    )
    .bind(me.tenant_id)
    .bind(id)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|x| {
        json!({ "id": x.get::<i64, _>("id"), "actor": { "kind": x.get::<String, _>("actor_kind"), "id": x.get::<Option<i64>, _>("actor_id") },
                "action": x.get::<String, _>("action"), "target": { "kind": x.get::<Option<String>, _>("target_kind"), "id": x.get::<Option<i64>, _>("target_id") },
                "ip": x.get::<Option<String>, _>("ip"), "meta": x.get::<sqlx::types::Json<Value>, _>("meta").0, "created_at": x.get::<DateTime<Utc>, _>("created_at") })
    })
    .collect::<Vec<_>>();
    let locked_until: Option<DateTime<Utc>> = r.get("locked_until");
    let manageable = can_manage(&me, &t).is_ok();
    Ok(Json(json!({
        "staff": {
            "id": t.id, "email": t.email, "name": t.name, "status": t.status, "role_id": t.role_id, "role": t.role_key, "role_label": t.role_name,
            "permissions": t.perms, "password_set": t.password_set, "created_at": r.get::<DateTime<Utc>, _>("created_at"),
            "last_login_at": r.get::<Option<DateTime<Utc>>, _>("last_login_at"), "locked": locked_until.is_some_and(|u| u > Utc::now()),
            "invited_at": r.get::<Option<DateTime<Utc>>, _>("invited_at"), "invited_by": r.get::<Option<String>, _>("invited_by"),
            "disabled_at": r.get::<Option<DateTime<Utc>>, _>("disabled_at"), "disabled_reason": r.get::<Option<String>, _>("disabled_reason"),
            "is_me": t.id == me.id, "manageable": manageable,
        },
        "sessions": sessions,
        "trusted_devices": devices,
        "events": events,
    })))
}

#[derive(Deserialize)]
pub struct UpdateReq {
    name: Option<String>,
    role_id: Option<i64>,
}

pub async fn update(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<UpdateReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "staff.write").await?;
    let t = target(&st, &me, id).await?;
    can_manage(&me, &t)?;
    let name = match r.name.as_deref() {
        Some(n) => Some(validate::name(n, "Enter the staff member's name.").map_err(field("name"))?),
        None => None,
    };
    let role = match r.role_id {
        Some(rid) if Some(rid) != t.role_id => Some(assignable_role(&st, &me, rid).await?),
        _ => None,
    };
    if name.is_none() && role.is_none() {
        return Ok(Json(json!({ "status": "ok", "changed": false })));
    }
    sqlx::query("UPDATE staff SET name = COALESCE($2, name), role = COALESCE($3, role), role_id = COALESCE($4, role_id), updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(name.as_deref())
        .bind(role.as_ref().map(|r| r.key.as_str()))
        .bind(role.as_ref().map(|r| r.id))
        .execute(&st.pool)
        .await?;
    let mut before = json!({});
    let mut after = json!({});
    if let Some(n) = &name {
        before["name"] = json!(t.name);
        after["name"] = json!(n);
    }
    if let Some(ro) = &role {
        before["role"] = json!(t.role_name);
        after["role"] = json!(ro.name);
        // a role change takes effect on the next request; sessions stay signed in
    }
    audit::record(&st.pool, &ctx, staff_entry(&me, if role.is_some() { "staff.role_changed" } else { "staff.updated" }, Some(("staff", id)), json!({"before": before, "after": after}))).await;
    Ok(Json(json!({ "status": "ok", "changed": true })))
}

#[derive(Deserialize, Default)]
pub struct ReasonReq {
    reason: Option<String>,
}

fn reason(r: Result<Json<ReasonReq>, JsonRejection>) -> Option<String> {
    r.ok().and_then(|Json(x)| x.reason).map(|s| s.trim().chars().take(300).collect::<String>()).filter(|s| !s.is_empty())
}

pub async fn disable(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<ReasonReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.write").await?;
    let t = target(&st, &me, id).await?;
    can_manage(&me, &t)?;
    let why = reason(req);
    if t.status == "disabled" {
        return Ok(Json(json!({ "status": "ok", "already": true })));
    }
    sqlx::query("UPDATE staff SET status = 'disabled', disabled_at = now(), disabled_reason = $2, updated_at = now() WHERE id = $1").bind(id).bind(&why).execute(&st.pool).await?;
    sqlx::query("UPDATE staff_invites SET revoked_at = now() WHERE staff_id = $1 AND accepted_at IS NULL AND revoked_at IS NULL").bind(id).execute(&st.pool).await?;
    identity::revoke_all(&st.pool, Kind::Staff, id).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "staff.disabled", Some(("staff", id)), json!({"reason": why, "before": {"status": t.status}, "after": {"status": "disabled"}}))).await;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn enable(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.write").await?;
    let t = target(&st, &me, id).await?;
    can_manage(&me, &t)?;
    if t.status != "disabled" {
        return Ok(Json(json!({ "status": "ok", "already": true })));
    }
    // someone disabled before accepting their invite goes back to "invited" (they still need a new link)
    let next = if t.password_set { "active" } else { "invited" };
    sqlx::query("UPDATE staff SET status = $2, disabled_at = NULL, disabled_reason = NULL, failed_logins = 0, locked_until = NULL, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(next)
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "staff.enabled", Some(("staff", id)), json!({"before": {"status": "disabled"}, "after": {"status": next}}))).await;
    Ok(Json(json!({ "status": "ok", "account_status": next })))
}

/// Email-code 2FA reset: forgets every trusted device, clears a lockout and signs the person out, so the next
/// sign-in needs a fresh emailed code.
pub async fn reset_2fa(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.write").await?;
    let t = target(&st, &me, id).await?;
    can_manage(&me, &t)?;
    let devices = sqlx::query("DELETE FROM trusted_devices WHERE subject_kind = 'staff' AND subject_id = $1").bind(id).execute(&st.pool).await?.rows_affected();
    sqlx::query("UPDATE staff SET failed_logins = 0, locked_until = NULL, updated_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    sqlx::query("UPDATE email_otps SET consumed_at = now() WHERE subject_kind = 'staff' AND subject_id = $1 AND consumed_at IS NULL").bind(id).execute(&st.pool).await?;
    identity::revoke_all(&st.pool, Kind::Staff, id).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "staff.2fa_reset", Some(("staff", id)), json!({"trusted_devices_removed": devices}))).await;
    Ok(Json(json!({ "status": "ok", "trusted_devices_removed": devices })))
}

pub async fn sign_out(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.write").await?;
    let t = target(&st, &me, id).await?;
    can_manage(&me, &t)?;
    let n = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE subject_kind = 'staff' AND subject_id = $1 AND revoked_at IS NULL AND expires_at > now()")
        .bind(id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    audit::record(&st.pool, &ctx, staff_entry(&me, "staff.signed_out", Some(("staff", id)), json!({"sessions_revoked": n}))).await;
    Ok(Json(json!({ "status": "ok", "sessions_revoked": n })))
}

// ---------- roles ----------

fn catalogue(for_owner: bool) -> Value {
    json!({
        "modules": rbac::MODULES.iter().filter(|m| for_owner || m.key != "owner").map(|m| json!({"key": m.key, "label": m.label, "description": m.description})).collect::<Vec<_>>(),
        "permissions": rbac::PERMS.iter().filter(|p| for_owner || !rbac::is_owner_perm(p.key)).map(|p| json!({"key": p.key, "module": p.module, "action": p.action, "label": p.label})).collect::<Vec<_>>(),
        "actions": ["view", "create", "edit", "approve", "export"],
    })
}

pub async fn permissions_catalogue(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = current(&st, &ctx).await?;
    Ok(Json(catalogue(me.is_owner())))
}

pub async fn roles(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.read").await?;
    let rows = sqlx::query(
        "SELECT r.id, r.key, r.name, r.description, r.kind, r.customised, r.permissions, r.created_at, r.updated_at,
                (SELECT count(*) FROM staff s WHERE s.role_id = r.id AND s.status <> 'disabled') AS members
         FROM roles r WHERE r.tenant_id = $1
         ORDER BY CASE r.kind WHEN 'system' THEN 0 WHEN 'preset' THEN 1 ELSE 2 END, r.id",
    )
    .bind(me.tenant_id)
    .fetch_all(&st.pool)
    .await?;
    let items = rows
        .iter()
        .filter(|r| me.is_owner() || r.get::<String, _>("key") != "platform_owner")
        .map(|r| {
            let key: String = r.get("key");
            let kind: String = r.get("kind");
            let perms = rbac::effective(&kind, &key, r.get("customised"), &r.get::<Vec<String>, _>("permissions"));
            let within = subset(&perms, &me.perms);
            json!({
                "id": r.get::<i64, _>("id"), "key": key, "name": r.get::<String, _>("name"), "description": r.get::<String, _>("description"),
                "kind": kind, "customised": r.get::<bool, _>("customised"), "permissions": perms, "members": r.get::<i64, _>("members"),
                "service_role": rbac::service_role(&key, &perms),
                "editable": kind != "system" && within && me.can("staff.roles"),
                "assignable": key != "platform_owner" && within && me.can("staff.write"),
                "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "items": items, "catalogue": catalogue(me.is_owner()), "my_permissions": me.perms })))
}

#[derive(Deserialize)]
pub struct RoleReq {
    name: Option<String>,
    description: Option<String>,
    permissions: Option<Vec<String>>,
}

fn role_name(raw: &str) -> Result<String, &'static str> {
    let n = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.chars().count() < 2 || n.chars().count() > 48 {
        return Err("Role names are 2–48 characters.");
    }
    Ok(n)
}

fn slug(name: &str) -> String {
    let s: String = name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let s = s.split('-').filter(|p| !p.is_empty()).collect::<Vec<_>>().join("-");
    let s: String = s.chars().take(32).collect();
    if s.is_empty() { "role".into() } else { s }
}

fn checked_perms(me: &Staff, raw: &[String]) -> ApiResult<Vec<String>> {
    let perms = rbac::normalize(raw).map_err(|m| ApiError::Validation { field: "permissions", message: m })?;
    if perms.iter().any(|p| rbac::is_owner_perm(p)) {
        return Err(denied("Platform owner permissions can't be given to a role."));
    }
    if perms.iter().any(|p| rbac::SUPER_ADMIN_ONLY.contains(&p.as_str())) {
        return Err(denied("Full-access staff sessions and erasing a client's personal data stay with the Super Admin."));
    }
    if !subset(&perms, &me.perms) {
        return Err(denied("You can only grant permissions you hold yourself."));
    }
    Ok(perms)
}

pub async fn create_role(State(st): State<AppState>, ctx: Ctx, req: Result<Json<RoleReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "staff.roles").await?;
    let name = role_name(r.name.as_deref().unwrap_or("")).map_err(field("name"))?;
    let description: String = r.description.unwrap_or_default().trim().chars().take(200).collect();
    let perms = checked_perms(&me, &r.permissions.unwrap_or_default())?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM roles WHERE tenant_id = $1 AND kind = 'custom'").bind(me.tenant_id).fetch_one(&st.pool).await?;
    if n >= 50 {
        return Err(conflict("limit_reached", "A workspace can have up to 50 custom roles."));
    }
    let key = format!("c-{}-{}", slug(&name), crate::crypto::random_token(3).to_lowercase().replace(['-', '_'], "x"));
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO roles (tenant_id, key, name, description, kind, permissions, created_by) VALUES ($1,$2,$3,$4,'custom',$5,$6)
         ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(me.tenant_id)
    .bind(&key)
    .bind(&name)
    .bind(&description)
    .bind(&perms)
    .bind(me.id)
    .fetch_optional(&st.pool)
    .await?;
    let id = id.ok_or(conflict("name_taken", "A role with this name already exists."))?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "role.created", Some(("role", id)), json!({"name": name, "after": {"permissions": perms}}))).await;
    Ok((StatusCode::CREATED, Json(json!({ "role": { "id": id, "key": key, "name": name, "permissions": perms } }))))
}

async fn editable_role(st: &AppState, me: &Staff, id: i64) -> ApiResult<rbac::RoleInfo> {
    let role = rbac::role_by_id(&st.pool, me.tenant_id, id).await?.ok_or(ApiError::NotFound)?;
    if role.kind == "system" {
        return Err(denied("Platform Owner and Super Admin always have full access and can't be edited."));
    }
    if !subset(&role.perms, &me.perms) {
        return Err(denied("You can't edit a role that has more access than you."));
    }
    Ok(role)
}

pub async fn update_role(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<RoleReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "staff.roles").await?;
    let role = editable_role(&st, &me, id).await?;
    let name = match r.name.as_deref() {
        Some(n) if role.kind == "custom" => Some(role_name(n).map_err(field("name"))?),
        _ => None,
    };
    let description: Option<String> = r.description.map(|d| d.trim().chars().take(200).collect());
    let perms = match &r.permissions {
        Some(p) => Some(checked_perms(&me, p)?),
        None => None,
    };
    let res = sqlx::query(
        "UPDATE roles SET name = COALESCE($2, name), description = COALESCE($3, description),
                permissions = COALESCE($4, permissions), customised = customised OR ($4 IS NOT NULL AND kind = 'preset'), updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(name.as_deref())
    .bind(description.as_deref())
    .bind(perms.as_ref())
    .execute(&st.pool)
    .await;
    if let Err(sqlx::Error::Database(e)) = &res
        && e.is_unique_violation()
    {
        return Err(conflict("name_taken", "A role with this name already exists."));
    }
    res?;
    let mut meta = json!({"role": role.name});
    if let Some(p) = &perms {
        let added: Vec<&String> = p.iter().filter(|k| !role.perms.contains(k)).collect();
        let removed: Vec<&String> = role.perms.iter().filter(|k| !p.contains(k)).collect();
        meta["before"] = json!({"permissions": role.perms});
        meta["after"] = json!({"permissions": p});
        meta["added"] = json!(added);
        meta["removed"] = json!(removed);
    }
    if let Some(n) = &name {
        meta["renamed_to"] = json!(n);
    }
    audit::record(&st.pool, &ctx, staff_entry(&me, "role.updated", Some(("role", id)), meta)).await;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn reset_role(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.roles").await?;
    let role = editable_role(&st, &me, id).await?;
    if role.kind != "preset" {
        return Err(ApiError::BadRequest("Only preset roles can be reset."));
    }
    let defaults: Vec<String> = rbac::preset_perms(&role.key).unwrap_or_default().into_iter().map(str::to_string).collect();
    if !subset(&defaults, &me.perms) {
        return Err(denied("You can't reset a role to more access than you have."));
    }
    sqlx::query("UPDATE roles SET customised = false, permissions = '{}', updated_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "role.reset", Some(("role", id)), json!({"role": role.name, "before": {"permissions": role.perms}, "after": {"permissions": defaults}}))).await;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn delete_role(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "staff.roles").await?;
    let role = editable_role(&st, &me, id).await?;
    if role.kind != "custom" {
        return Err(ApiError::BadRequest("Preset roles can't be deleted. Reset them instead."));
    }
    let members: i64 = sqlx::query_scalar("SELECT count(*) FROM staff WHERE role_id = $1").bind(id).fetch_one(&st.pool).await?;
    if members > 0 {
        return Err(conflict("role_in_use", "Move everyone to another role before deleting this one."));
    }
    sqlx::query("DELETE FROM roles WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "role.deleted", Some(("role", id)), json!({"role": role.name, "before": {"permissions": role.perms}}))).await;
    Ok(Json(json!({ "status": "ok" })))
}

// ---------- IP allow-list ----------

pub async fn ip_list(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "security.read").await?;
    let t = sqlx::query("SELECT ip_allowlist_enabled, ip_owner_bypass FROM tenants WHERE id = $1").bind(me.tenant_id).fetch_one(&st.pool).await?;
    let rows = sqlx::query(
        "SELECT a.id, a.cidr, a.label, a.created_at, s.name AS created_by FROM tenant_ip_allowlist a
         LEFT JOIN staff s ON s.id = a.created_by WHERE a.tenant_id = $1 ORDER BY a.id",
    )
    .bind(me.tenant_id)
    .fetch_all(&st.pool)
    .await?;
    let cidrs: Vec<String> = rows.iter().map(|r| r.get("cidr")).collect();
    let last_hits = sqlx::query(
        "SELECT ip, max(created_at) AS at FROM audit_log WHERE tenant_id = $1 AND actor_kind = 'staff' AND action = 'staff.login'
           AND created_at > now() - interval '30 days' GROUP BY ip",
    )
    .bind(me.tenant_id)
    .fetch_all(&st.pool)
    .await?;
    let items = rows
        .iter()
        .map(|r| {
            let cidr: String = r.get("cidr");
            let last = last_hits
                .iter()
                .filter(|h| h.get::<Option<String>, _>("ip").is_some_and(|ip| rbac::ip_allowed(&ip, std::slice::from_ref(&cidr))))
                .filter_map(|h| h.get::<Option<DateTime<Utc>>, _>("at"))
                .max();
            json!({ "id": r.get::<i64, _>("id"), "cidr": cidr, "label": r.get::<String, _>("label"), "created_at": r.get::<DateTime<Utc>, _>("created_at"),
                    "created_by": r.get::<Option<String>, _>("created_by"), "last_sign_in_at": last })
        })
        .collect::<Vec<_>>();
    let blocked_24h: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log WHERE tenant_id = $1 AND action = 'security.ip_blocked' AND created_at > now() - interval '24 hours'")
        .bind(me.tenant_id)
        .fetch_one(&st.pool)
        .await?;
    Ok(Json(json!({
        "enabled": t.get::<bool, _>("ip_allowlist_enabled"),
        "owner_bypass": t.get::<bool, _>("ip_owner_bypass"),
        "items": items,
        "your_ip": ctx.ip,
        "your_ip_allowed": rbac::ip_allowed(&ctx.ip, &cidrs),
        "blocked_24h": blocked_24h,
        "can_edit": me.can("security.write"),
    })))
}

#[derive(Deserialize)]
pub struct IpReq {
    #[serde(default)]
    cidr: String,
    #[serde(default)]
    label: String,
}

pub async fn ip_add(State(st): State<AppState>, ctx: Ctx, req: Result<Json<IpReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "security.write").await?;
    let (cidr, _, _) = rbac::parse_cidr(&r.cidr).map_err(field("cidr"))?;
    let label: String = r.label.trim().chars().take(80).collect();
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM tenant_ip_allowlist WHERE tenant_id = $1").bind(me.tenant_id).fetch_one(&st.pool).await?;
    if n >= 200 {
        return Err(conflict("limit_reached", "The allow-list holds up to 200 entries. Use wider ranges."));
    }
    let id: Option<i64> = sqlx::query_scalar("INSERT INTO tenant_ip_allowlist (tenant_id, cidr, label, created_by) VALUES ($1,$2,$3,$4) ON CONFLICT DO NOTHING RETURNING id")
        .bind(me.tenant_id)
        .bind(&cidr)
        .bind(&label)
        .bind(me.id)
        .fetch_optional(&st.pool)
        .await?;
    let id = id.ok_or(conflict("duplicate", "That address is already on the list."))?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "security.ip_added", None, json!({"cidr": cidr, "label": label, "entry_id": id}))).await;
    Ok((StatusCode::CREATED, Json(json!({ "id": id, "cidr": cidr, "label": label }))))
}

/// Whether `me` could still work with the given list and switch (owner bypass counts).
fn keeps_access(me: &Staff, ip: &str, enabled: bool, bypass: bool, cidrs: &[String]) -> bool {
    !enabled || (bypass && me.is_owner()) || rbac::ip_allowed(ip, cidrs)
}

pub async fn ip_remove(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "security.write").await?;
    let cidr: String = sqlx::query_scalar("SELECT cidr FROM tenant_ip_allowlist WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(me.tenant_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let t = sqlx::query("SELECT ip_allowlist_enabled, ip_owner_bypass FROM tenants WHERE id = $1").bind(me.tenant_id).fetch_one(&st.pool).await?;
    let (enabled, bypass): (bool, bool) = (t.get("ip_allowlist_enabled"), t.get("ip_owner_bypass"));
    let rest: Vec<String> = allowlist(&st, me.tenant_id).await?.into_iter().filter(|c| *c != cidr).collect();
    if enabled && rest.is_empty() {
        return Err(conflict("would_lock_out", "This is the last entry. Turn the allow-list off before removing it."));
    }
    if !keeps_access(&me, &ctx.ip, enabled, bypass, &rest) {
        return Err(conflict("would_lock_out", "Removing this entry would block your own connection. Add your current IP first."));
    }
    sqlx::query("DELETE FROM tenant_ip_allowlist WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "security.ip_removed", None, json!({"cidr": cidr, "entry_id": id}))).await;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize)]
pub struct IpSettingsReq {
    enabled: Option<bool>,
    owner_bypass: Option<bool>,
}

pub async fn ip_settings(State(st): State<AppState>, ctx: Ctx, req: Result<Json<IpSettingsReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "security.write").await?;
    let t = sqlx::query("SELECT ip_allowlist_enabled, ip_owner_bypass FROM tenants WHERE id = $1").bind(me.tenant_id).fetch_one(&st.pool).await?;
    let before = (t.get::<bool, _>("ip_allowlist_enabled"), t.get::<bool, _>("ip_owner_bypass"));
    let enabled = r.enabled.unwrap_or(before.0);
    let bypass = r.owner_bypass.unwrap_or(before.1);
    if r.owner_bypass.is_some() && !me.is_owner() && r.owner_bypass != Some(before.1) {
        return Err(denied("Only the platform owner can change the owner bypass."));
    }
    let list = allowlist(&st, me.tenant_id).await?;
    if enabled && list.is_empty() {
        return Err(conflict("empty_list", "Add at least one IP address or range before turning the allow-list on."));
    }
    if !keeps_access(&me, &ctx.ip, enabled, bypass, &list) {
        return Err(conflict("would_lock_out", "Your current IP address isn't on the list. Add it first so you don't lock yourself out."));
    }
    sqlx::query("UPDATE tenants SET ip_allowlist_enabled = $2, ip_owner_bypass = $3, updated_at = now() WHERE id = $1")
        .bind(me.tenant_id)
        .bind(enabled)
        .bind(bypass)
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &ctx, staff_entry(&me, "security.ip_settings", None, json!({
        "before": {"enabled": before.0, "owner_bypass": before.1}, "after": {"enabled": enabled, "owner_bypass": bypass}
    })))
    .await;
    Ok(Json(json!({ "status": "ok", "enabled": enabled, "owner_bypass": bypass })))
}

// ---------- invite acceptance (public, via the Back Office BFF) ----------

#[derive(Deserialize)]
pub struct InviteLookup {
    token: Option<String>,
}

struct Invite {
    id: i64,
    tenant_id: i64,
    staff_id: i64,
    email: String,
    name: String,
    role: String,
    tenant: String,
    expires_at: DateTime<Utc>,
}

async fn find_invite(st: &AppState, token: &str) -> ApiResult<Invite> {
    if token.len() < 20 || token.len() > 128 {
        return Err(invite_gone());
    }
    let r = sqlx::query(
        "SELECT i.id, i.tenant_id, i.staff_id, i.expires_at, i.accepted_at IS NOT NULL AS accepted, i.revoked_at IS NOT NULL AS revoked,
                s.email, s.name, s.status, COALESCE(r.name, s.role) AS role, t.name AS tenant, t.status AS tstatus
         FROM staff_invites i JOIN staff s ON s.id = i.staff_id JOIN tenants t ON t.id = i.tenant_id LEFT JOIN roles r ON r.id = s.role_id
         WHERE i.token_hash = $1",
    )
    .bind(st.keys.hash("invite", token))
    .fetch_optional(&st.pool)
    .await?
    .ok_or_else(invite_gone)?;
    let expires_at: DateTime<Utc> = r.get("expires_at");
    if r.get::<bool, _>("accepted") || r.get::<bool, _>("revoked") || expires_at <= Utc::now() || r.get::<String, _>("status") != "invited" {
        return Err(invite_gone());
    }
    if r.get::<String, _>("tstatus") != "active" {
        return Err(tenant_suspended());
    }
    Ok(Invite {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        staff_id: r.get("staff_id"),
        email: r.get("email"),
        name: r.get("name"),
        role: r.get("role"),
        tenant: r.get("tenant"),
        expires_at,
    })
}

fn invite_gone() -> ApiError {
    ApiError::Coded { status: StatusCode::GONE, code: "invite_expired", message: "This invite link has expired or was already used. Ask your administrator for a new one." }
}

pub async fn invite_info(State(st): State<AppState>, ctx: Ctx, q: Result<Query<InviteLookup>, axum::extract::rejection::QueryRejection>) -> ApiResult<Json<Value>> {
    identity::limit(&st, format!("invite-info:ip:{}", ctx.ip), 30, 600)?;
    let token = q.ok().and_then(|Query(q)| q.token).unwrap_or_default();
    let i = find_invite(&st, &token).await?;
    Ok(Json(json!({ "email": i.email, "email_masked": validate::mask_email(&i.email), "name": i.name, "role_label": i.role, "tenant": { "name": i.tenant }, "expires_at": i.expires_at })))
}

#[derive(Deserialize)]
pub struct AcceptReq {
    #[serde(default)]
    token: String,
    #[serde(default)]
    password: String,
}

/// Sets the password and sends the first sign-in code (the invite proves the link, the code proves the mailbox).
pub async fn invite_accept(State(st): State<AppState>, ctx: Ctx, req: Result<Json<AcceptReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    identity::limit(&st, format!("invite-accept:ip:{}", ctx.ip), 10, 600)?;
    let i = find_invite(&st, &r.token).await?;
    validate::password(&r.password).map_err(field("password"))?;
    login_ip_check(&st, &ctx, i.tenant_id, Who::Id(i.staff_id)).await?;
    let hash = identity::hash_password_blocking(r.password).await?;
    let mut tx = st.pool.begin().await?;
    let n = sqlx::query("UPDATE staff_invites SET accepted_at = now() WHERE id = $1 AND accepted_at IS NULL AND revoked_at IS NULL").bind(i.id).execute(&mut *tx).await?.rows_affected();
    if n == 0 {
        return Err(invite_gone());
    }
    sqlx::query("UPDATE staff SET password_hash = $2, status = 'active', failed_logins = 0, locked_until = NULL, updated_at = now() WHERE id = $1 AND status = 'invited'")
        .bind(i.staff_id)
        .bind(&hash)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit::record(&st.pool, &ctx, Entry { tenant_id: i.tenant_id, actor_kind: "staff", actor_id: Some(i.staff_id), action: "staff.invite_accepted", target: Some(("staff", i.staff_id)), meta: json!({}) }).await;
    let (challenge, code) = identity::send_otp(&st, &ctx, Kind::Staff, i.tenant_id, i.staff_id, &i.email, identity::Purpose::Login).await?;
    Ok(Json(identity::challenge_json(&st, &challenge, &i.email, identity::Purpose::Login, Kind::Staff, Some(&code))))
}

#[cfg(test)]
mod tests;

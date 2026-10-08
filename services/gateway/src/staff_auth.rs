//! Back Office staff auth: /v1/admin/auth/*  (separate identities, sessions and cookies from clients)

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use serde_json::{Value, json};
use sqlx::Row;

use crate::audit::{self, Entry};
use crate::client_auth::{LoginReq, ResendReq, VerifyReq, body};
use crate::error::{ApiError, ApiResult, field};
use crate::flows;
use crate::identity::{self, Kind, Purpose};
use crate::state::{AppState, Ctx};
use crate::validate;

const K: Kind = Kind::Staff;

/// The staff member as the apps see them. `permissions` is the full, authoritative list (`rbac: true`); `role`
/// is the built-in role name downstream services understand (see `rbac::service_role`). `tenant.modules` is the
/// tenant's effective module map: the Back Office hides a switched-off module from that tenant's staff.
async fn staff_json(st: &AppState, id: i64) -> ApiResult<Value> {
    let r = sqlx::query(
        "SELECT s.id, s.email, s.name, s.role, s.last_login_at, t.id AS tenant_id, t.slug, t.name AS tenant_name,
                r.id AS role_id, COALESCE(r.key, s.role) AS rkey, COALESCE(r.name, s.role) AS rname, COALESCE(r.kind, 'preset') AS rkind,
                COALESCE(r.customised, false) AS rc, COALESCE(r.permissions, '{}') AS rp
         FROM staff s JOIN tenants t ON t.id = s.tenant_id LEFT JOIN roles r ON r.id = s.role_id
         WHERE s.id = $1 AND s.status = 'active'",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    let key: String = r.get("rkey");
    let perms = crate::rbac::effective(r.get("rkind"), &key, r.get("rc"), &r.get::<Vec<String>, _>("rp"));
    let owner = perms.iter().any(|p| crate::rbac::is_owner_perm(p));
    let tenant_id: i64 = r.get("tenant_id");
    let modules = crate::tenancy::module_map(&st.pool, tenant_id).await?;
    Ok(json!({
        "id": r.get::<i64, _>("id"),
        "email": r.get::<String, _>("email"),
        "name": r.get::<String, _>("name"),
        "role": crate::rbac::service_role(&key, &perms),
        "role_key": key,
        "role_id": r.get::<Option<i64>, _>("role_id"),
        "role_label": r.get::<String, _>("rname"),
        "permissions": perms,
        "rbac": true,
        "is_owner": owner,
        "tenant": { "id": tenant_id, "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("tenant_name"), "modules": modules },
    }))
}

async fn signed_in(st: &AppState, ctx: &Ctx, tenant_id: i64, staff_id: i64, via: &str) -> ApiResult<Value> {
    identity::mark_login(&st.pool, K, staff_id).await?;
    let s = identity::create_session(st, ctx, K, tenant_id, staff_id).await?;
    audit::record(&st.pool, ctx, Entry { tenant_id, actor_kind: "staff", actor_id: Some(staff_id), action: "staff.login", target: None, meta: json!({"via": via}) }).await;
    Ok(json!({ "status": "ok", "session": { "token": s.token, "expires_at": s.expires_at }, "staff": staff_json(st, staff_id).await? }))
}

pub async fn login(State(st): State<AppState>, ctx: Ctx, req: Result<Json<LoginReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let email = validate::email(&r.email).map_err(field("email"))?;
    if r.password.is_empty() || r.password.len() > 256 {
        return Err(ApiError::Validation { field: "password", message: "Enter your password." });
    }
    identity::limit(&st, format!("staff-login:ip:{}", ctx.ip), 20, 5 * 60)?;
    identity::limit(&st, format!("staff-login:email:{email}"), 10, 15 * 60)?;
    let tenant_id = identity::tenant_id(&st.pool, &ctx.tenant_slug).await?;
    crate::staff_admin::login_ip_check(&st, &ctx, tenant_id, crate::staff_admin::Who::Email(&email)).await?;
    let p = flows::check_password(&st, &ctx, K, tenant_id, &email, &r.password).await?;
    // Back Office: an emailed code on every sign-in (STAFF_OTP_EVERY_LOGIN=false falls back to new devices only).
    if st.cfg.staff_otp_every_login || !identity::device_trusted(&st, &ctx, K, p.id).await? {
        let (challenge, code) = identity::send_otp(&st, &ctx, K, tenant_id, p.id, &p.email, Purpose::Login).await?;
        return Ok(Json(identity::challenge_json(&st, &challenge, &p.email, Purpose::Login, K, Some(&code))));
    }
    st.limiter.clear(&format!("staff-login:email:{email}"));
    Ok(Json(signed_in(&st, &ctx, tenant_id, p.id, "password").await?))
}

pub async fn verify_otp(State(st): State<AppState>, ctx: Ctx, req: Result<Json<VerifyReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    identity::limit(&st, format!("staff-otp:ip:{}", ctx.ip), 20, 10 * 60)?;
    let v = identity::verify_otp(&st, K, &r.challenge, &r.code).await?;
    if v.purpose != Purpose::Login {
        return Err(ApiError::CodeExpired);
    }
    crate::staff_admin::login_ip_check(&st, &ctx, v.tenant_id, crate::staff_admin::Who::Id(v.subject_id)).await?;
    identity::trust_device(&st, &ctx, K, v.tenant_id, v.subject_id, v.device_hash).await?;
    Ok(Json(signed_in(&st, &ctx, v.tenant_id, v.subject_id, "email_otp").await?))
}

pub async fn resend(State(st): State<AppState>, ctx: Ctx, req: Result<Json<ResendReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    identity::limit(&st, format!("staff-resend:ip:{}", ctx.ip), 10, 10 * 60)?;
    Ok(Json(identity::resend_otp(&st, K, &r.challenge).await?))
}

pub async fn logout(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    if let Some(token) = ctx.bearer.as_deref()
        && let Some((tenant_id, staff_id)) = identity::revoke_token(&st, token, K).await?
    {
        audit::record(&st.pool, &ctx, Entry { tenant_id, actor_kind: "staff", actor_id: Some(staff_id), action: "staff.logout", target: None, meta: json!({}) }).await;
    }
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn me(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    // session-level checks (tenant active, IP allow-list); the session itself was resolved above
    let me = crate::admin::current(&st, &ctx).await?;
    Ok(Json(json!({ "staff": staff_json(&st, me.id).await?, "session": { "expires_at": s.expires_at } })))
}

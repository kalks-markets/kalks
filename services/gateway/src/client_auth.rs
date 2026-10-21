//! Client Area auth: /v1/auth/*

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::audit::{self, Entry};
use crate::crypto;
use crate::error::{ApiError, ApiResult, field};
use crate::flows;
use crate::identity::{self, Kind, Purpose};
use crate::state::{AppState, Ctx};
use crate::validate;

const K: Kind = Kind::User;

pub fn body<T>(r: Result<Json<T>, JsonRejection>) -> ApiResult<T> {
    r.map(|Json(v)| v).map_err(|_| ApiError::BadRequest("Invalid request body."))
}

pub async fn user_json(st: &AppState, id: i64) -> ApiResult<Value> {
    let r = sqlx::query(
        "SELECT u.id, u.email, u.first_name, u.last_name, u.phone_dial, u.phone, u.country, u.date_of_birth, u.kyc_status,
                u.email_verified_at IS NOT NULL AS email_verified, u.google_sub IS NOT NULL AS google_linked, u.referral_code, u.created_at, t.slug, t.name AS tenant_name,
                u.identity_locked_at IS NOT NULL AS identity_locked,
                (SELECT k.status FROM kyc_cases k WHERE k.user_id = u.id ORDER BY k.id DESC LIMIT 1) AS kyc_case_status
         FROM users u JOIN tenants t ON t.id = u.tenant_id WHERE u.id = $1",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::Unauthorized)?;
    let first: String = r.get("first_name");
    let last: String = r.get("last_name");
    Ok(json!({
        "id": r.get::<i64, _>("id"),
        "email": r.get::<String, _>("email"),
        "first_name": first,
        "last_name": last,
        "name": format!("{first} {last}"),
        "phone_dial": r.get::<String, _>("phone_dial"),
        "phone": r.get::<String, _>("phone"),
        "country": r.get::<String, _>("country"),
        "date_of_birth": r.get::<NaiveDate, _>("date_of_birth"),
        "kyc_status": r.get::<String, _>("kyc_status"),
        // latest KYC case status (draft | submitted | in_review | more_info | approved | rejected), null before any case
        "kyc_case_status": r.get::<Option<String>, _>("kyc_case_status"),
        // D92: name and date of birth can no longer change (set when KYC is approved)
        "identity_locked": r.get::<bool, _>("identity_locked"),
        "email_verified": r.get::<bool, _>("email_verified"),
        "google_linked": r.get::<bool, _>("google_linked"),
        "referral_code": r.get::<String, _>("referral_code"),
        "created_at": r.get::<chrono::DateTime<Utc>, _>("created_at"),
        "tenant": { "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("tenant_name") },
    }))
}

pub(crate) async fn signed_in(st: &AppState, ctx: &Ctx, tenant_id: i64, user_id: i64, via: &str) -> ApiResult<Value> {
    identity::mark_login(&st.pool, K, user_id).await?;
    let s = identity::create_session(st, ctx, K, tenant_id, user_id).await?;
    let country = identity::COUNTRY.try_with(|c| c.clone()).ok().flatten();
    audit::record(&st.pool, ctx, Entry { tenant_id, actor_kind: "user", actor_id: Some(user_id), action: "user.login", target: None, meta: json!({"via": via, "country": country}) }).await;
    Ok(json!({ "status": "ok", "session": { "token": s.token, "expires_at": s.expires_at }, "user": user_json(st, user_id).await? }))
}

/// Welcome email, sent in the background once per account (right after its email address is verified).
pub(crate) fn send_welcome(st: &AppState, email: String, first_name: String) {
    if let Some(mailer) = st.mailer.clone() {
        let locale = crate::mail_i18n::current();
        tokio::spawn(async move {
            if let Err(e) = mailer.send_welcome_in(&locale, &email, &first_name).await {
                tracing::error!(error = %e, "welcome email could not be sent");
            }
        });
    }
}

// ---------- register ----------

#[derive(Deserialize)]
pub struct RegisterReq {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    phone_dial: String,
    #[serde(default)]
    phone: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    date_of_birth: String,
    referral_code: Option<String>,
    /// Partner campaign slug (IB programme), kept only when the referral code resolves to a client.
    referral_campaign: Option<String>,
    /// First-touch campaign attribution (utm_*, landing page, referrer) captured by the Client Area.
    #[serde(default)]
    attribution: Option<crate::marketing::AttributionReq>,
    /// "Email me news and offers" (journeys, campaigns). Transactional emails are sent regardless.
    marketing_consent: Option<bool>,
    #[serde(default)]
    password: String,
    #[serde(default)]
    accept_terms: bool,
}

pub(crate) fn referral_prefix(first: &str) -> String {
    let p: String = first.chars().filter(|c| c.is_ascii_alphabetic()).take(6).collect::<String>().to_uppercase();
    if p.len() >= 2 { p } else { "KALKS".into() }
}

/// A client account about to be created (email sign-up or Google sign-up).
pub(crate) struct NewUser<'a> {
    pub tenant_id: i64,
    pub email: &'a str,
    pub password_hash: &'a str,
    pub first_name: &'a str,
    pub last_name: &'a str,
    pub phone_dial: &'a str,
    pub phone: &'a str,
    pub country: &'a str,
    pub date_of_birth: NaiveDate,
    pub referred_by: Option<i64>,
    pub referral_raw: Option<&'a str>,
    /// Partner campaign slug; stored only together with `referred_by`.
    pub referral_campaign: Option<&'a str>,
    /// Google sign-up: (sub, picture). The email counts as verified (Google verified it).
    pub google: Option<(&'a str, Option<&'a str>)>,
}

/// Inserts a user with a fresh referral code. `Ok(None)` when the email (or Google account) is already taken.
pub(crate) async fn insert_user(st: &AppState, u: NewUser<'_>) -> ApiResult<Option<i64>> {
    let prefix = referral_prefix(u.first_name);
    let (google_sub, avatar) = match u.google {
        Some((sub, pic)) => (Some(sub), pic),
        None => (None, None),
    };
    for _ in 0..6 {
        let n = u32::from_le_bytes(crypto::random_bytes::<4>()) % 9000 + 1000;
        let code = format!("{prefix}{n}");
        let res = sqlx::query_scalar::<_, i64>(
            "INSERT INTO users (tenant_id, email, password_hash, first_name, last_name, phone_dial, phone, country, date_of_birth,
                                referral_code, referred_by, referred_code_raw, terms_accepted_at,
                                google_sub, google_linked_at, avatar_url, email_verified_at, referral_campaign)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12, now(),
                     $13, CASE WHEN $13::text IS NULL THEN NULL ELSE now() END, $14,
                     CASE WHEN $13::text IS NULL THEN NULL ELSE now() END, $15)
             ON CONFLICT DO NOTHING RETURNING id",
        )
        .bind(u.tenant_id)
        .bind(u.email)
        .bind(u.password_hash)
        .bind(u.first_name)
        .bind(u.last_name)
        .bind(u.phone_dial)
        .bind(u.phone)
        .bind(u.country)
        .bind(u.date_of_birth)
        .bind(&code)
        .bind(u.referred_by)
        .bind(u.referral_raw)
        .bind(google_sub)
        .bind(avatar)
        .bind(u.referred_by.and(u.referral_campaign))
        .fetch_optional(&st.pool)
        .await?;
        if let Some(id) = res {
            return Ok(Some(id));
        }
        // conflict: the email (or Google account) raced in, or the referral code collided
        let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND (email = $2 OR google_sub = $3)")
            .bind(u.tenant_id)
            .bind(u.email)
            .bind(google_sub)
            .fetch_optional(&st.pool)
            .await?;
        if taken.is_some() {
            return Ok(None);
        }
    }
    Err(anyhow::anyhow!("could not allocate referral code").into())
}

const UNKNOWN_REFERRAL: &str = "This referral code doesn't exist. Check it or leave it empty.";

/// Resolves a referral code to the referring client. A code that matches no client in the tenant is refused
/// (field `referral_code`) rather than dropped, so the visitor can fix or clear it and the partner keeps the client.
pub(crate) async fn referrer(st: &AppState, tenant_id: i64, code: Option<&str>) -> ApiResult<Option<i64>> {
    let Some(code) = code else { return Ok(None) };
    let id: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND referral_code = $2").bind(tenant_id).bind(code).fetch_optional(&st.pool).await?;
    id.map(Some).ok_or(ApiError::Validation { field: "referral_code", message: UNKNOWN_REFERRAL })
}

pub async fn register(State(st): State<AppState>, ctx: Ctx, req: Result<Json<RegisterReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    identity::limit(&st, format!("register:ip:{}", ctx.ip), 10, 15 * 60)?;

    let first = validate::name(&r.first_name, "Enter your first name.").map_err(field("first_name"))?;
    let last = validate::name(&r.last_name, "Enter your last name.").map_err(field("last_name"))?;
    let email = validate::email(&r.email).map_err(field("email"))?;
    let country = validate::country(&r.country).map_err(field("country"))?;
    let dial = validate::dial(&r.phone_dial).map_err(field("phone"))?;
    let phone = validate::phone(&r.phone).map_err(field("phone"))?;
    let dob = validate::date_of_birth(&r.date_of_birth, Utc::now().date_naive()).map_err(field("date_of_birth"))?;
    let referral = validate::referral(r.referral_code.as_deref()).map_err(field("referral_code"))?;
    let campaign = validate::campaign(r.referral_campaign.as_deref());
    validate::password(&r.password).map_err(field("password"))?;
    if !r.accept_terms {
        return Err(ApiError::Validation { field: "accept_terms", message: "Please confirm you are over 18 and accept the terms." });
    }

    let tenant_id = identity::tenant_id(&st.pool, &ctx.tenant_slug).await?;
    crate::tenancy::client_gate(&st, tenant_id).await?;
    crate::tenancy::require_feature(&st, tenant_id, "client_registration").await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE tenant_id = $1 AND email = $2").bind(tenant_id).bind(&email).fetch_optional(&st.pool).await?;
    if exists.is_some() {
        return Err(ApiError::EmailTaken);
    }
    let referred_by = referrer(&st, tenant_id, referral.as_deref()).await?;
    let hash = identity::hash_password_blocking(r.password).await?;

    let user_id = insert_user(&st, NewUser {
        tenant_id,
        email: &email,
        password_hash: &hash,
        first_name: &first,
        last_name: &last,
        phone_dial: &dial,
        phone: &phone,
        country: &country,
        date_of_birth: dob,
        referred_by,
        referral_raw: referral.as_deref(),
        referral_campaign: campaign.as_deref(),
        google: None,
    })
    .await?
    .ok_or(ApiError::EmailTaken)?;
    let attribution = crate::marketing::clean_attribution(&r.attribution.unwrap_or_default());
    crate::marketing::store_signup(&st.pool, user_id, &attribution, r.marketing_consent.unwrap_or(true)).await?;

    audit::record(&st.pool, &ctx, Entry {
        tenant_id,
        actor_kind: "user",
        actor_id: Some(user_id),
        action: "user.register",
        target: Some(("user", user_id)),
        meta: json!({"country": country, "referral_code": referral, "referred_by": referred_by, "referral_campaign": campaign,
                     "utm_source": attribution.source, "utm_medium": attribution.medium, "utm_campaign": attribution.campaign}),
    })
    .await;
    tracing::info!(user_id, "client registered");

    let (challenge, code) = identity::send_otp(&st, &ctx, K, tenant_id, user_id, &email, Purpose::VerifyEmail).await?;
    Ok((StatusCode::CREATED, Json(identity::challenge_json(&st, &challenge, &email, Purpose::VerifyEmail, K, Some(&code)))))
}

// ---------- login ----------

#[derive(Deserialize)]
pub struct LoginReq {
    #[serde(default)]
    pub(crate) email: String,
    #[serde(default)]
    pub(crate) password: String,
}

pub async fn login(State(st): State<AppState>, ctx: Ctx, req: Result<Json<LoginReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    if r.password.is_empty() || r.password.len() > 256 {
        return Err(ApiError::Validation { field: "password", message: "Enter your password." });
    }
    // view-only logins (D90) sign in with an id that is never an email address
    if !r.email.contains('@') && !r.email.trim().is_empty() {
        identity::limit(&st, format!("login:ip:{}", ctx.ip), 30, 5 * 60)?;
        let tenant_id = identity::tenant_id(&st.pool, &ctx.tenant_slug).await?;
        crate::tenancy::client_gate(&st, tenant_id).await?;
        return Ok(Json(crate::client_security::viewer_login(&st, &ctx, tenant_id, &r.email, &r.password).await?));
    }
    let email = validate::email(&r.email).map_err(field("email"))?;
    identity::limit(&st, format!("login:ip:{}", ctx.ip), 30, 5 * 60)?;
    identity::limit(&st, format!("login:email:{email}"), 15, 15 * 60)?;

    let tenant_id = identity::tenant_id(&st.pool, &ctx.tenant_slug).await?;
    crate::tenancy::client_gate(&st, tenant_id).await?;
    let p = flows::check_password(&st, &ctx, K, tenant_id, &email, &r.password).await?;

    if !p.email_verified {
        let (challenge, code) = identity::send_otp(&st, &ctx, K, tenant_id, p.id, &p.email, Purpose::VerifyEmail).await?;
        return Ok(Json(identity::challenge_json(&st, &challenge, &p.email, Purpose::VerifyEmail, K, Some(&code))));
    }
    if !identity::device_trusted(&st, &ctx, K, p.id).await? {
        let (challenge, code) = identity::send_otp(&st, &ctx, K, tenant_id, p.id, &p.email, Purpose::Login).await?;
        audit::record(&st.pool, &ctx, Entry { tenant_id, actor_kind: "user", actor_id: Some(p.id), action: "user.login_challenge", target: None, meta: json!({"reason": "new_device"}) }).await;
        return Ok(Json(identity::challenge_json(&st, &challenge, &p.email, Purpose::Login, K, Some(&code))));
    }
    st.limiter.clear(&format!("login:email:{email}"));
    Ok(Json(signed_in(&st, &ctx, tenant_id, p.id, "password").await?))
}

// ---------- verify email / new-device code ----------

#[derive(Deserialize)]
pub struct VerifyReq {
    #[serde(default)]
    pub(crate) challenge: String,
    #[serde(default)]
    pub(crate) code: String,
}

pub async fn verify_email(State(st): State<AppState>, ctx: Ctx, req: Result<Json<VerifyReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    identity::limit(&st, format!("otp:ip:{}", ctx.ip), 30, 10 * 60)?;
    let v = identity::verify_otp(&st, K, &r.challenge, &r.code).await?;
    match v.purpose {
        Purpose::VerifyEmail => {
            // Only the first verification returns a row, so the welcome email is sent once.
            let first = sqlx::query(
                "UPDATE users SET email_verified_at = now(), updated_at = now() WHERE id = $1 AND email_verified_at IS NULL RETURNING email, first_name",
            )
            .bind(v.subject_id)
            .fetch_optional(&st.pool)
            .await?;
            if let Some(row) = first {
                send_welcome(&st, row.get("email"), row.get("first_name"));
            }
            audit::record(&st.pool, &ctx, Entry { tenant_id: v.tenant_id, actor_kind: "user", actor_id: Some(v.subject_id), action: "user.email_verified", target: Some(("user", v.subject_id)), meta: json!({}) }).await;
        }
        Purpose::Login => {}
        Purpose::ResetPassword => return Err(ApiError::BadRequest("Use the reset form for this code.")),
        // step-up codes confirm a change inside a session; they never sign anyone in
        Purpose::Confirm => return Err(ApiError::CodeExpired),
    }
    identity::trust_device(&st, &ctx, K, v.tenant_id, v.subject_id, v.device_hash).await?;
    Ok(Json(signed_in(&st, &ctx, v.tenant_id, v.subject_id, v.purpose.as_str()).await?))
}

#[derive(Deserialize)]
pub struct ResendReq {
    #[serde(default)]
    pub(crate) challenge: String,
}

pub async fn resend(State(st): State<AppState>, ctx: Ctx, req: Result<Json<ResendReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    identity::limit(&st, format!("resend:ip:{}", ctx.ip), 10, 10 * 60)?;
    Ok(Json(identity::resend_otp(&st, K, &r.challenge).await?))
}

// ---------- session ----------

pub async fn logout(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    if let Some(token) = ctx.bearer.as_deref()
        && let Some((tenant_id, user_id)) = identity::revoke_token(&st, token, K).await?
    {
        let staff = crate::client_controls::logout_entry(&st, token).await?;
        let entry = if let Some((sid, _, _, staff_id)) = staff {
            Entry { tenant_id, actor_kind: "staff", actor_id: Some(staff_id), action: "client.impersonation_ended", target: Some(("user", user_id)), meta: json!({ "session_id": sid, "via": "ended", "app": "client_area" }) }
        } else if token.starts_with(identity::VIEWER_TOKEN_PREFIX) {
            let vid: Option<i64> = sqlx::query_scalar("SELECT viewer_id FROM sessions WHERE token_hash = $1").bind(st.keys.hash("session", token)).fetch_optional(&st.pool).await?.flatten();
            Entry { tenant_id, actor_kind: "viewer", actor_id: vid, action: "viewer.logout", target: Some(("user", user_id)), meta: json!({}) }
        } else {
            Entry { tenant_id, actor_kind: "user", actor_id: Some(user_id), action: "user.logout", target: None, meta: json!({}) }
        };
        audit::record(&st.pool, &ctx, entry).await;
    }
    Ok(Json(json!({ "status": "ok" })))
}

/// The signed-in client. View-only sessions (D90) get the owner's reduced record plus `viewer` (label, accounts,
/// sections, expiry); the apps use it to render a read-only Client Area.
pub async fn me(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session_any(&st, &ctx, K).await?;
    let idle: i32 = sqlx::query_scalar("SELECT client_idle_minutes FROM tenants WHERE id = $1").bind(s.tenant_id).fetch_one(&st.pool).await?;
    let session = json!({ "id": s.session_id, "expires_at": s.expires_at, "tenant_id": s.tenant_id, "idle_minutes": idle });
    // restrictions (banner) and, for a staff session, who opened it (client_controls.rs); also inside `user` so
    // the apps' cached session record carries them
    let (restrictions, impersonation) = crate::client_controls::me_extras(&st, &s).await?;
    let mut user = match s.viewer_id {
        Some(_) => crate::client_security::viewer_user_json(&st, s.subject_id).await?,
        None => user_json(&st, s.subject_id).await?,
    };
    user["restrictions"] = restrictions["restrictions"].clone();
    user["restricted"] = restrictions["restricted"].clone();
    user["impersonation"] = impersonation.clone();
    let viewer = match s.viewer_id {
        Some(v) => crate::client_security::viewer_scope(&st, v).await?,
        None => Value::Null,
    };
    Ok(Json(json!({ "user": user, "viewer": viewer, "session": session, "impersonation": impersonation })))
}

// ---------- password reset ----------

#[derive(Deserialize)]
pub struct ForgotReq {
    #[serde(default)]
    email: String,
}

pub async fn forgot(State(st): State<AppState>, ctx: Ctx, req: Result<Json<ForgotReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let email = validate::email(&r.email).map_err(field("email"))?;
    identity::limit(&st, format!("forgot:ip:{}", ctx.ip), 10, 15 * 60)?;
    identity::limit(&st, format!("forgot:email:{email}"), 3, 15 * 60)?;
    let tenant_id = identity::tenant_id(&st.pool, &ctx.tenant_slug).await?;
    match identity::find_principal(&st.pool, K, tenant_id, &email).await? {
        Some(p) if p.active => {
            let (challenge, code) = identity::send_otp(&st, &ctx, K, tenant_id, p.id, &p.email, Purpose::ResetPassword).await?;
            audit::record(&st.pool, &ctx, Entry { tenant_id, actor_kind: "user", actor_id: Some(p.id), action: "user.password_reset_requested", target: None, meta: json!({}) }).await;
            Ok(Json(identity::challenge_json(&st, &challenge, &p.email, Purpose::ResetPassword, K, Some(&code))))
        }
        // Same response shape for unknown emails, so the form can't be used to discover accounts.
        _ => Ok(Json(identity::challenge_json(&st, &crypto::random_token(24), &email, Purpose::ResetPassword, K, None))),
    }
}

#[derive(Deserialize)]
pub struct ResetReq {
    #[serde(default)]
    pub(crate) challenge: String,
    #[serde(default)]
    pub(crate) code: String,
    #[serde(default)]
    password: String,
}

pub async fn reset(State(st): State<AppState>, ctx: Ctx, req: Result<Json<ResetReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    validate::password(&r.password).map_err(field("password"))?;
    identity::limit(&st, format!("otp:ip:{}", ctx.ip), 30, 10 * 60)?;
    let v = identity::verify_otp(&st, K, &r.challenge, &r.code).await?;
    if v.purpose != Purpose::ResetPassword {
        return Err(ApiError::CodeExpired);
    }
    let hash = identity::hash_password_blocking(r.password).await?;
    sqlx::query(
        "UPDATE users SET password_hash = $2, failed_logins = 0, locked_until = NULL,
                email_verified_at = COALESCE(email_verified_at, now()), updated_at = now() WHERE id = $1",
    )
    .bind(v.subject_id)
    .bind(hash)
    .execute(&st.pool)
    .await?;
    identity::revoke_all(&st.pool, K, v.subject_id).await?;
    identity::trust_device(&st, &ctx, K, v.tenant_id, v.subject_id, v.device_hash).await?;
    audit::record(&st.pool, &ctx, Entry { tenant_id: v.tenant_id, actor_kind: "user", actor_id: Some(v.subject_id), action: "user.password_reset", target: Some(("user", v.subject_id)), meta: json!({}) }).await;
    Ok(Json(json!({ "status": "ok" })))
}

#[cfg(test)]
mod tests;

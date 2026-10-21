//! Client Area security (D32, D90, D93, D94): the client's own sessions and login history, view-only logins,
//! and account-closure / data-export requests. Back Office counterparts live at the end of this file.
//!
//! Client (session bearer, like every /v1/auth route):
//! * `GET  /v1/auth/sessions`                    live sessions of this client (current one marked, viewer sessions labelled)
//! * `POST /v1/auth/sessions/{id}/revoke`        signs one session out
//! * `POST /v1/auth/sessions/revoke-others`      signs every other session out (viewer sessions included)
//! * `GET  /v1/auth/logins`                      sign-in history, last 90 days
//! * `GET  /v1/auth/viewers`                     view-only logins + their recent activity
//! * `POST /v1/auth/viewers`                     `{label, username?, accounts, sections, expires_at?, stepup_token}` → password shown once
//! * `PATCH /v1/auth/viewers/{id}`               `{label?, accounts?, sections?, expires_at?}` (null expiry = never)
//! * `POST /v1/auth/viewers/{id}/password`       `{stepup_token}` → new password shown once; the viewer's sessions end
//! * `POST /v1/auth/viewers/{id}/revoke`         access ends at once
//! * `POST /v1/auth/viewer/activity`             `{path}` (viewer sessions only): page views for the owner's activity log
//! * `GET  /v1/auth/requests`, `POST /v1/auth/requests {kind, reason?}`, `POST /v1/auth/requests/{id}/cancel`
//! * `GET  /v1/auth/requests/{id}/export`        the client's personal data held by the gateway, once staff completed the export
//!
//! View-only sessions can call the GET routes above only where noted; every change is refused with 403
//! `viewer_read_only` (`identity::resolve_session`).

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::admin::require_key;
use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::crypto;
use crate::error::{ApiError, ApiResult};
use crate::identity::{self, Kind};
use crate::state::{AppState, Ctx};

const K: Kind = Kind::User;

/// Client Area sections a viewer can be given (the Client Area maps them to pages and read APIs).
pub const VIEWER_SECTIONS: &[&str] = &["dashboard", "accounts", "history", "wallet", "partner"];
pub const MAX_VIEWERS: i64 = 10;
const MAX_VIEWER_ACCOUNTS: usize = 50;

fn entry<'a>(tenant_id: i64, user_id: i64, action: &'a str, meta: Value) -> Entry<'a> {
    Entry { tenant_id, actor_kind: "user", actor_id: Some(user_id), action, target: Some(("user", user_id)), meta }
}

// ---------- sessions ----------

/// "session `se` is live" for client sessions, with the tenant's idle window.
const LIVE: &str = "se.revoked_at IS NULL AND se.expires_at > now()
    AND se.last_seen_at > now() - make_interval(mins => (SELECT t.client_idle_minutes FROM tenants t WHERE t.id = se.tenant_id))";

pub async fn sessions(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT se.id, se.ip, se.user_agent, se.country, se.created_at, se.last_seen_at, se.expires_at, se.viewer_id, v.label AS viewer_label
         FROM sessions se LEFT JOIN client_viewers v ON v.id = se.viewer_id
         WHERE se.subject_kind = 'user' AND se.subject_id = $1 AND {LIVE}
         ORDER BY (se.id = $2) DESC, se.last_seen_at DESC LIMIT 100"
    )))
    .bind(s.subject_id)
    .bind(s.session_id)
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id: i64 = r.get("id");
            let viewer: Option<i64> = r.get("viewer_id");
            json!({
                "id": id,
                "current": id == s.session_id,
                "ip": r.get::<Option<String>, _>("ip"),
                "user_agent": r.get::<Option<String>, _>("user_agent"),
                "country": r.get::<Option<String>, _>("country"),
                "created_at": r.get::<DateTime<Utc>, _>("created_at"),
                "last_seen_at": r.get::<DateTime<Utc>, _>("last_seen_at"),
                "expires_at": r.get::<DateTime<Utc>, _>("expires_at"),
                "viewer": viewer.map(|v| json!({ "id": v, "label": r.get::<Option<String>, _>("viewer_label") })),
            })
        })
        .collect();
    let idle: i32 = sqlx::query_scalar("SELECT client_idle_minutes FROM tenants WHERE id = $1").bind(s.tenant_id).fetch_one(&st.pool).await?;
    Ok(Json(json!({ "items": items, "idle_minutes": idle, "max_days": identity::policy(K).session_ttl.num_days() })))
}

pub async fn revoke_session(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    if id == s.session_id {
        return Err(ApiError::BadRequest("This is the session you're using. Use Log out instead."));
    }
    let row = sqlx::query(
        "UPDATE sessions SET revoked_at = now() WHERE id = $1 AND subject_kind = 'user' AND subject_id = $2 AND revoked_at IS NULL
         RETURNING viewer_id, ip, user_agent",
    )
    .bind(id)
    .bind(s.subject_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "user.session_revoked", json!({
        "session_id": id, "viewer_id": row.get::<Option<i64>, _>("viewer_id"), "ip": row.get::<Option<String>, _>("ip"),
    })))
    .await;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn revoke_others(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let n = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE subject_kind = 'user' AND subject_id = $1 AND id <> $2 AND revoked_at IS NULL")
        .bind(s.subject_id)
        .bind(s.session_id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "user.sessions_revoked", json!({ "count": n }))).await;
    Ok(Json(json!({ "status": "ok", "revoked": n })))
}

// ---------- login history ----------

/// Sign-in related audit actions shown to the client, with the result the Client Area displays.
fn login_result(action: &str, meta: &Value) -> Option<&'static str> {
    Some(match action {
        "user.login" => match meta["via"].as_str() {
            Some("login") => "new_device",
            Some("google") => "google",
            Some("verify_email") => "verified",
            _ => "success",
        },
        "user.login_challenge" => "code_sent",
        "user.login_failed" => "failed",
        "user.locked" => "locked",
        "user.logout" => "logout",
        "user.password_reset" => "password_reset",
        "user.session_revoked" | "user.sessions_revoked" => "signed_out_device",
        "admin.session_revoked" | "admin.sessions_revoked" => "signed_out_by_staff",
        // a staff member opened the account from the Back Office (client_controls.rs)
        "client.impersonation_started" => "staff_access",
        _ => return None,
    })
}

pub async fn logins(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let rows = sqlx::query(
        "SELECT id, action, ip, user_agent, meta, created_at FROM audit_log
         WHERE tenant_id = $1 AND created_at > now() - interval '90 days'
           AND ((actor_kind = 'user' AND actor_id = $2) OR (target_kind = 'user' AND target_id = $2 AND actor_kind = 'staff'))
           AND action = ANY($3)
         ORDER BY id DESC LIMIT 200",
    )
    .bind(s.tenant_id)
    .bind(s.subject_id)
    .bind(
        [
            "user.login", "user.login_challenge", "user.login_failed", "user.locked", "user.logout", "user.password_reset",
            "user.session_revoked", "user.sessions_revoked", "admin.session_revoked", "admin.sessions_revoked", "client.impersonation_started",
        ]
        .map(String::from)
        .to_vec(),
    )
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .filter_map(|r| {
            let action: String = r.get("action");
            let meta: Value = r.get::<sqlx::types::Json<Value>, _>("meta").0;
            let result = login_result(&action, &meta)?;
            // staff access: the broker's support, never the staff member's own network details
            let staff = result == "staff_access";
            Some(json!({
                "id": r.get::<i64, _>("id"),
                "at": r.get::<DateTime<Utc>, _>("created_at"),
                "result": result,
                "ip": if staff { None } else { r.get::<Option<String>, _>("ip") },
                "user_agent": if staff { None } else { r.get::<Option<String>, _>("user_agent") },
                "country": if staff { None } else { meta["country"].as_str() },
                "app": if staff { meta["app"].as_str() } else { None },
            }))
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

// ---------- view-only logins (D90, D93) ----------

/// Normalises a requested viewer username; `None` when it isn't allowed. Never contains '@', so the sign-in
/// form can tell viewer ids from email addresses.
pub fn clean_username(raw: &str) -> Option<String> {
    let u = raw.trim().to_ascii_lowercase();
    let mut chars = u.chars();
    let first = chars.next()?;
    let ok = (4..=32).contains(&u.len())
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    ok.then_some(u)
}

fn default_username(first_name: &str) -> String {
    let base: String = first_name.to_ascii_lowercase().chars().filter(|c| c.is_ascii_lowercase()).take(10).collect();
    let base = if base.len() >= 2 { base } else { "client".into() };
    let n = u32::from_le_bytes(crypto::random_bytes::<4>()) % 9000 + 1000;
    format!("{base}-view-{n}")
}

/// 16-character password with every character class the password rules need, no look-alike characters.
pub fn generate_password() -> String {
    const UPPER: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ";
    const LOWER: &[u8] = b"abcdefghijkmnpqrstuvwxyz";
    const DIGIT: &[u8] = b"23456789";
    const SYMBOL: &[u8] = b"!#%+=?@";
    let all: Vec<u8> = [UPPER, LOWER, DIGIT, SYMBOL].concat();
    let rnd = crypto::random_bytes::<32>();
    let pick = |set: &[u8], b: u8| set[b as usize % set.len()] as char;
    let mut out: Vec<char> = vec![pick(UPPER, rnd[0]), pick(LOWER, rnd[1]), pick(DIGIT, rnd[2]), pick(SYMBOL, rnd[3])];
    out.extend((4..16).map(|i| pick(&all, rnd[i])));
    // shuffle so the classes don't sit in fixed positions
    for i in (1..out.len()).rev() {
        let j = rnd[16 + (i % 16)] as usize % (i + 1);
        out.swap(i, j);
    }
    out.into_iter().collect()
}

fn clean_label(raw: &str) -> ApiResult<String> {
    let l: String = raw.trim().chars().filter(|c| !c.is_control()).take(60).collect();
    if l.is_empty() {
        return Err(ApiError::Validation { field: "label", message: "Give this login a name, e.g. My accountant." });
    }
    Ok(l)
}

fn clean_sections(raw: &[String]) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for s in raw {
        let s = s.trim();
        if !VIEWER_SECTIONS.contains(&s) {
            return Err(ApiError::Validation { field: "sections", message: "Unknown section." });
        }
        if !out.iter().any(|x| x == s) {
            out.push(s.to_string());
        }
    }
    if out.is_empty() {
        return Err(ApiError::Validation { field: "sections", message: "Choose at least one section." });
    }
    Ok(out)
}

/// Trading account logins (digits). Ownership is checked by the Client Area against the trading engine before
/// the request reaches the gateway; here only the shape is validated.
fn clean_accounts(raw: &[String]) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for a in raw {
        let a = a.trim();
        if a.is_empty() || a.len() > 20 || !a.chars().all(|c| c.is_ascii_digit()) {
            return Err(ApiError::Validation { field: "accounts", message: "Invalid trading account." });
        }
        if !out.iter().any(|x| x == a) {
            out.push(a.to_string());
        }
    }
    if out.len() > MAX_VIEWER_ACCOUNTS {
        return Err(ApiError::Validation { field: "accounts", message: "Too many accounts." });
    }
    Ok(out)
}

fn clean_expiry(v: Option<DateTime<Utc>>) -> ApiResult<Option<DateTime<Utc>>> {
    match v {
        Some(t) if t <= Utc::now() + Duration::minutes(5) => Err(ApiError::Validation { field: "expires_at", message: "The expiry date must be in the future." }),
        Some(t) if t > Utc::now() + Duration::days(3 * 366) => Err(ApiError::Validation { field: "expires_at", message: "Choose an expiry within 3 years." }),
        other => Ok(other),
    }
}

fn viewer_json(r: &sqlx::postgres::PgRow) -> Value {
    let revoked: Option<DateTime<Utc>> = r.get("revoked_at");
    let expires: Option<DateTime<Utc>> = r.get("expires_at");
    let status = if revoked.is_some() {
        "revoked"
    } else if expires.is_some_and(|e| e <= Utc::now()) {
        "expired"
    } else {
        "active"
    };
    json!({
        "id": r.get::<i64, _>("id"),
        "label": r.get::<String, _>("label"),
        "username": r.get::<String, _>("username"),
        "accounts": r.get::<Vec<String>, _>("accounts"),
        "sections": r.get::<Vec<String>, _>("sections"),
        "expires_at": expires,
        "revoked_at": revoked,
        "status": status,
        "last_login_at": r.get::<Option<DateTime<Utc>>, _>("last_login_at"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

const VIEWER_COLS: &str = "id, label, username, accounts, sections, expires_at, revoked_at, last_login_at, created_at";

/// The scope of a viewer session, for `/v1/auth/me`.
pub async fn viewer_scope(st: &AppState, viewer_id: i64) -> ApiResult<Value> {
    let r = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {VIEWER_COLS} FROM client_viewers WHERE id = $1")))
        .bind(viewer_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    Ok(viewer_json(&r))
}

pub async fn viewers(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {VIEWER_COLS} FROM client_viewers WHERE user_id = $1 ORDER BY revoked_at IS NOT NULL, created_at DESC"
    )))
    .bind(s.subject_id)
    .fetch_all(&st.pool)
    .await?;
    let activity = sqlx::query(
        "SELECT a.id, a.actor_id, a.action, a.ip, a.user_agent, a.meta, a.created_at, v.label
         FROM audit_log a LEFT JOIN client_viewers v ON v.id = a.actor_id
         WHERE a.tenant_id = $1 AND a.target_kind = 'user' AND a.target_id = $2
           AND (a.actor_kind = 'viewer' OR a.action LIKE 'viewer.%')
         ORDER BY a.id DESC LIMIT 100",
    )
    .bind(s.tenant_id)
    .bind(s.subject_id)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| {
        let meta: Value = r.get::<sqlx::types::Json<Value>, _>("meta").0;
        json!({
            "id": r.get::<i64, _>("id"),
            "viewer_id": meta["viewer_id"].as_i64().or(r.get::<Option<i64>, _>("actor_id")),
            "label": r.get::<Option<String>, _>("label").or_else(|| meta["label"].as_str().map(String::from)),
            "action": r.get::<String, _>("action"),
            "path": meta["path"].as_str(),
            "ip": r.get::<Option<String>, _>("ip"),
            "user_agent": r.get::<Option<String>, _>("user_agent"),
            "at": r.get::<DateTime<Utc>, _>("created_at"),
        })
    })
    .collect::<Vec<_>>();
    Ok(Json(json!({ "items": rows.iter().map(viewer_json).collect::<Vec<_>>(), "activity": activity, "sections": VIEWER_SECTIONS, "max": MAX_VIEWERS })))
}

#[derive(Deserialize)]
pub struct CreateViewerReq {
    #[serde(default)]
    label: String,
    username: Option<String>,
    #[serde(default)]
    accounts: Vec<String>,
    #[serde(default)]
    sections: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    stepup_token: String,
}

pub async fn create_viewer(State(st): State<AppState>, ctx: Ctx, req: Result<Json<CreateViewerReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let s = identity::resolve_session(&st, &ctx, K).await?;
    identity::limit(&st, format!("viewers:user:{}", s.subject_id), 20, 60 * 60)?;
    let label = clean_label(&r.label)?;
    let sections = clean_sections(&r.sections)?;
    let accounts = clean_accounts(&r.accounts)?;
    let expires_at = clean_expiry(r.expires_at)?;
    let requested = match r.username.as_deref().map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => Some(clean_username(u).ok_or(ApiError::Validation {
            field: "username",
            message: "Use 4–32 lowercase letters, digits, dots, dashes or underscores.",
        })?),
        None => None,
    };
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM client_viewers WHERE user_id = $1 AND revoked_at IS NULL").bind(s.subject_id).fetch_one(&st.pool).await?;
    if active >= MAX_VIEWERS {
        return Err(ApiError::Coded { status: StatusCode::CONFLICT, code: "viewer_limit", message: "You can have up to 10 view-only logins. Revoke one first." });
    }
    let first: String = sqlx::query_scalar("SELECT first_name FROM users WHERE id = $1").bind(s.subject_id).fetch_one(&st.pool).await?;
    if let Some(u) = &requested {
        let taken: Option<i64> = sqlx::query_scalar("SELECT id FROM client_viewers WHERE tenant_id = $1 AND username = $2").bind(s.tenant_id).bind(u).fetch_optional(&st.pool).await?;
        if taken.is_some() {
            return Err(ApiError::Validation { field: "username", message: "This username is taken. Try another." });
        }
    }
    crate::stepup::consume_token(&st, s.subject_id, "viewer_access", "", &r.stepup_token).await?;

    let password = generate_password();
    let hash = identity::hash_password_blocking(password.clone()).await?;
    let mut created = None;
    for attempt in 0..5 {
        let username = match (&requested, attempt) {
            (Some(u), _) => u.clone(),
            (None, _) => default_username(&first),
        };
        let row = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO client_viewers (tenant_id, user_id, label, username, password_hash, accounts, sections, expires_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (tenant_id, username) DO NOTHING RETURNING {VIEWER_COLS}"
        )))
        .bind(s.tenant_id)
        .bind(s.subject_id)
        .bind(&label)
        .bind(&username)
        .bind(&hash)
        .bind(&accounts)
        .bind(&sections)
        .bind(expires_at)
        .fetch_optional(&st.pool)
        .await?;
        if let Some(row) = row {
            created = Some(row);
            break;
        }
        if requested.is_some() {
            return Err(ApiError::Validation { field: "username", message: "This username is taken. Try another." });
        }
    }
    let row = created.ok_or_else(|| anyhow::anyhow!("could not allocate a viewer username"))?;
    let v = viewer_json(&row);
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "viewer.created", json!({
        "viewer_id": v["id"], "label": label, "username": v["username"], "accounts": accounts, "sections": sections, "expires_at": expires_at,
    })))
    .await;
    Ok((StatusCode::CREATED, Json(json!({ "viewer": v, "password": password }))))
}

#[derive(Deserialize)]
pub struct UpdateViewerReq {
    label: Option<String>,
    accounts: Option<Vec<String>>,
    sections: Option<Vec<String>>,
    /// Present + null = never expires; absent = unchanged.
    #[serde(default, deserialize_with = "double_option")]
    expires_at: Option<Option<DateTime<Utc>>>,
}

fn double_option<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<DateTime<Utc>>>, D::Error> {
    Option::<DateTime<Utc>>::deserialize(d).map(Some)
}

async fn owned_viewer(st: &AppState, user_id: i64, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {VIEWER_COLS} FROM client_viewers WHERE id = $1 AND user_id = $2")))
        .bind(id)
        .bind(user_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)
}

pub async fn update_viewer(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<UpdateViewerReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let before = owned_viewer(&st, s.subject_id, id).await?;
    if before.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some() {
        return Err(ApiError::BadRequest("This login was revoked. Create a new one."));
    }
    let label = r.label.as_deref().map(clean_label).transpose()?;
    let sections = r.sections.as_deref().map(clean_sections).transpose()?;
    let accounts = r.accounts.as_deref().map(clean_accounts).transpose()?;
    let expires = match r.expires_at {
        Some(e) => Some(clean_expiry(e)?),
        None => None,
    };
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE client_viewers SET label = COALESCE($3, label), accounts = COALESCE($4, accounts), sections = COALESCE($5, sections),
                expires_at = CASE WHEN $6 THEN $7 ELSE expires_at END, updated_at = now()
         WHERE id = $1 AND user_id = $2 RETURNING {VIEWER_COLS}"
    )))
    .bind(id)
    .bind(s.subject_id)
    .bind(&label)
    .bind(&accounts)
    .bind(&sections)
    .bind(expires.is_some())
    .bind(expires.flatten())
    .fetch_one(&st.pool)
    .await?;
    let v = viewer_json(&row);
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "viewer.updated", json!({
        "viewer_id": id, "label": v["label"],
        "before": { "accounts": before.get::<Vec<String>, _>("accounts"), "sections": before.get::<Vec<String>, _>("sections"), "expires_at": before.get::<Option<DateTime<Utc>>, _>("expires_at") },
        "after": { "accounts": v["accounts"], "sections": v["sections"], "expires_at": v["expires_at"] },
    })))
    .await;
    Ok(Json(json!({ "viewer": v })))
}

#[derive(Deserialize)]
pub struct ViewerPasswordReq {
    #[serde(default)]
    stepup_token: String,
}

pub async fn viewer_password(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<ViewerPasswordReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let v = owned_viewer(&st, s.subject_id, id).await?;
    if v.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some() {
        return Err(ApiError::BadRequest("This login was revoked. Create a new one."));
    }
    crate::stepup::consume_token(&st, s.subject_id, "viewer_access", "", &r.stepup_token).await?;
    let password = generate_password();
    let hash = identity::hash_password_blocking(password.clone()).await?;
    sqlx::query("UPDATE client_viewers SET password_hash = $2, failed_logins = 0, locked_until = NULL, updated_at = now() WHERE id = $1").bind(id).bind(hash).execute(&st.pool).await?;
    let ended = end_viewer_sessions(&st, id).await?;
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "viewer.password_reset", json!({ "viewer_id": id, "label": v.get::<String, _>("label"), "sessions_ended": ended }))).await;
    Ok(Json(json!({ "password": password, "username": v.get::<String, _>("username") })))
}

async fn end_viewer_sessions(st: &AppState, viewer_id: i64) -> ApiResult<u64> {
    Ok(sqlx::query("UPDATE sessions SET revoked_at = now() WHERE viewer_id = $1 AND revoked_at IS NULL").bind(viewer_id).execute(&st.pool).await?.rows_affected())
}

pub async fn revoke_viewer(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let v = owned_viewer(&st, s.subject_id, id).await?;
    sqlx::query("UPDATE client_viewers SET revoked_at = COALESCE(revoked_at, now()), updated_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    let ended = end_viewer_sessions(&st, id).await?;
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "viewer.revoked", json!({ "viewer_id": id, "label": v.get::<String, _>("label"), "sessions_ended": ended }))).await;
    Ok(Json(json!({ "status": "ok" })))
}

/// Sign-in with a viewer id (`client_auth::login` routes ids without '@' here). Password + lockout, no email
/// code (viewers have no mailbox of their own); the session is the owner's, marked with the viewer.
pub async fn viewer_login(st: &AppState, ctx: &Ctx, tenant_id: i64, raw_username: &str, password: &str) -> ApiResult<Value> {
    let username = clean_username(raw_username).ok_or(ApiError::InvalidCredentials)?;
    identity::limit(st, format!("login:viewer:{username}"), 15, 15 * 60)?;
    let row = sqlx::query(
        "SELECT v.id, v.user_id, v.label, v.password_hash, v.failed_logins, v.locked_until, v.revoked_at, v.expires_at, u.status = 'active' AS owner_active
         FROM client_viewers v JOIN users u ON u.id = v.user_id WHERE v.tenant_id = $1 AND v.username = $2",
    )
    .bind(tenant_id)
    .bind(&username)
    .fetch_optional(&st.pool)
    .await?;
    let Some(row) = row else {
        identity::verify_password_blocking(password.to_string(), crypto::dummy_hash().to_string()).await;
        return Err(ApiError::InvalidCredentials);
    };
    let (vid, owner): (i64, i64) = (row.get("id"), row.get("user_id"));
    let label: String = row.get("label");
    let vctx = |action: &'static str, meta: Value| Entry { tenant_id, actor_kind: "viewer", actor_id: Some(vid), action, target: Some(("user", owner)), meta };
    let now = Utc::now();
    if let Some(until) = row.get::<Option<DateTime<Utc>>, _>("locked_until")
        && until > now
    {
        return Err(ApiError::Locked { retry_after: (until - now).num_seconds() });
    }
    if !identity::verify_password_blocking(password.to_string(), row.get("password_hash")).await {
        let pol = identity::policy(K);
        let (n, lock) = identity::after_failure(row.get("failed_logins"), now, &pol);
        sqlx::query("UPDATE client_viewers SET failed_logins = $2, locked_until = $3 WHERE id = $1").bind(vid).bind(n).bind(lock).execute(&st.pool).await?;
        audit::record(&st.pool, ctx, vctx(if lock.is_some() { "viewer.locked" } else { "viewer.login_failed" }, json!({ "label": label }))).await;
        if let Some(until) = lock {
            return Err(ApiError::Locked { retry_after: (until - now).num_seconds() });
        }
        return Err(ApiError::InvalidCredentials);
    }
    let expired = row.get::<Option<DateTime<Utc>>, _>("expires_at").is_some_and(|e| e <= now);
    if row.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some() || expired || !row.get::<bool, _>("owner_active") {
        audit::record(&st.pool, ctx, vctx("viewer.login_blocked", json!({ "label": label, "reason": if expired { "expired" } else { "revoked" } }))).await;
        return Err(ApiError::Coded {
            status: StatusCode::FORBIDDEN,
            code: "viewer_inactive",
            message: if expired { "This view-only login has expired. Ask the account holder for a new one." } else { "This view-only login is no longer active." },
        });
    }
    sqlx::query("UPDATE client_viewers SET failed_logins = 0, locked_until = NULL, last_login_at = now() WHERE id = $1").bind(vid).execute(&st.pool).await?;
    let sess = identity::create_session_as(st, ctx, K, tenant_id, owner, Some(vid)).await?;
    audit::record(&st.pool, ctx, vctx("viewer.login", json!({ "label": label }))).await;
    st.limiter.clear(&format!("login:viewer:{username}"));
    Ok(json!({
        "status": "ok",
        "session": { "token": sess.token, "expires_at": sess.expires_at },
        "user": viewer_user_json(st, owner).await?,
        "viewer": viewer_scope(st, vid).await?,
    }))
}

/// What a viewer session sees of the owning client: name and ids, no contact details or date of birth.
pub async fn viewer_user_json(st: &AppState, owner: i64) -> ApiResult<Value> {
    let mut u = crate::client_auth::user_json(st, owner).await?;
    let email = u["email"].as_str().unwrap_or("").to_string();
    u["email"] = json!(crate::validate::mask_email(&email));
    u["phone"] = json!("");
    u["phone_dial"] = json!("");
    u["date_of_birth"] = json!("");
    u["referral_code"] = json!("");
    Ok(u)
}

#[derive(Deserialize)]
pub struct ActivityReq {
    #[serde(default)]
    path: String,
}

/// Page views of a viewer session, for the owner's activity log. The same page is recorded at most once
/// every 5 minutes per viewer.
pub async fn viewer_activity(State(st): State<AppState>, ctx: Ctx, req: Result<Json<ActivityReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let s = identity::resolve_session_any(&st, &ctx, K).await?;
    let Some(vid) = s.viewer_id else { return Ok(Json(json!({ "status": "ignored" }))) };
    let path: String = r.path.trim().chars().take(120).collect();
    if !path.starts_with('/') || path.starts_with("/api/") || !path.chars().all(|c| c.is_ascii_graphic()) {
        return Err(ApiError::Validation { field: "path", message: "Invalid path." });
    }
    identity::limit(&st, format!("viewer-activity:{vid}"), 120, 60 * 60)?;
    if st.limiter.hit(&format!("viewer-view:{vid}:{path}"), 1, std::time::Duration::from_secs(300)).is_ok() {
        audit::record(&st.pool, &ctx, Entry { tenant_id: s.tenant_id, actor_kind: "viewer", actor_id: Some(vid), action: "viewer.page_view", target: Some(("user", s.subject_id)), meta: json!({ "path": path }) }).await;
    }
    Ok(Json(json!({ "status": "ok" })))
}

// ---------- closure / data export requests (D94) ----------

fn request_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "kind": r.get::<String, _>("kind"),
        "status": r.get::<String, _>("status"),
        "reason": r.get::<Option<String>, _>("reason"),
        "staff_note": r.get::<Option<String>, _>("staff_note"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
        "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
        "closed_at": r.get::<Option<DateTime<Utc>>, _>("closed_at"),
    })
}

const REQUEST_COLS: &str = "id, kind, status, reason, staff_note, created_at, updated_at, closed_at";

pub async fn requests(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {REQUEST_COLS} FROM client_requests WHERE user_id = $1 ORDER BY id DESC LIMIT 50")))
        .bind(s.subject_id)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(json!({ "items": rows.iter().map(request_json).collect::<Vec<_>>() })))
}

#[derive(Deserialize)]
pub struct CreateRequestReq {
    #[serde(default)]
    kind: String,
    reason: Option<String>,
}

pub async fn create_request(State(st): State<AppState>, ctx: Ctx, req: Result<Json<CreateRequestReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let kind = match r.kind.as_str() {
        "closure" => "closure",
        "data_export" => "data_export",
        _ => return Err(ApiError::Validation { field: "kind", message: "Unknown request." }),
    };
    let reason: Option<String> = r.reason.map(|x| x.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(1000).collect::<String>()).filter(|x| !x.is_empty());
    let s = identity::resolve_session(&st, &ctx, K).await?;
    identity::limit(&st, format!("client-requests:user:{}", s.subject_id), 10, 24 * 60 * 60)?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO client_requests (tenant_id, user_id, kind, reason) VALUES ($1,$2,$3,$4)
         ON CONFLICT (user_id, kind) WHERE status IN ('open', 'in_progress') DO NOTHING RETURNING {REQUEST_COLS}"
    )))
    .bind(s.tenant_id)
    .bind(s.subject_id)
    .bind(kind)
    .bind(&reason)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::Coded { status: StatusCode::CONFLICT, code: "request_pending", message: "You already have a request of this kind in progress." })?;
    let v = request_json(&row);
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, if kind == "closure" { "user.closure_requested" } else { "user.data_export_requested" }, json!({ "request_id": v["id"] }))).await;
    Ok((StatusCode::CREATED, Json(json!({ "request": v }))))
}

pub async fn cancel_request(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE client_requests SET status = 'cancelled', updated_at = now(), closed_at = now()
         WHERE id = $1 AND user_id = $2 AND status = 'open' RETURNING {REQUEST_COLS}"
    )))
    .bind(id)
    .bind(s.subject_id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::BadRequest("Only a request that staff haven't started can be cancelled."))?;
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "user.request_cancelled", json!({ "request_id": id }))).await;
    Ok(Json(json!({ "request": request_json(&row) })))
}

/// Personal data the gateway holds about the client (JSON), available once staff completed an export request.
pub async fn export(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let s = identity::resolve_session(&st, &ctx, K).await?;
    let ok: Option<String> = sqlx::query_scalar("SELECT status FROM client_requests WHERE id = $1 AND user_id = $2 AND kind = 'data_export'")
        .bind(id)
        .bind(s.subject_id)
        .fetch_optional(&st.pool)
        .await?;
    if ok.as_deref() != Some("completed") {
        return Err(ApiError::NotFound);
    }
    identity::limit(&st, format!("export:user:{}", s.subject_id), 10, 60 * 60)?;
    let profile = crate::client_auth::user_json(&st, s.subject_id).await?;
    let sessions = sqlx::query("SELECT created_at, last_seen_at, ip, user_agent, country, revoked_at FROM sessions WHERE subject_kind = 'user' AND subject_id = $1 ORDER BY id DESC LIMIT 500")
        .bind(s.subject_id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| json!({ "created_at": r.get::<DateTime<Utc>, _>("created_at"), "last_seen_at": r.get::<DateTime<Utc>, _>("last_seen_at"), "ip": r.get::<Option<String>, _>("ip"), "user_agent": r.get::<Option<String>, _>("user_agent"), "country": r.get::<Option<String>, _>("country"), "ended_at": r.get::<Option<DateTime<Utc>>, _>("revoked_at") }))
        .collect::<Vec<_>>();
    let activity = sqlx::query("SELECT action, ip, user_agent, created_at FROM audit_log WHERE tenant_id = $1 AND ((actor_kind = 'user' AND actor_id = $2) OR (target_kind = 'user' AND target_id = $2)) ORDER BY id DESC LIMIT 2000")
        .bind(s.tenant_id)
        .bind(s.subject_id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| json!({ "action": r.get::<String, _>("action"), "ip": r.get::<Option<String>, _>("ip"), "user_agent": r.get::<Option<String>, _>("user_agent"), "at": r.get::<DateTime<Utc>, _>("created_at") }))
        .collect::<Vec<_>>();
    let kyc = sqlx::query("SELECT id, status, created_at, decided_at FROM kyc_cases WHERE user_id = $1 ORDER BY id")
        .bind(s.subject_id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| json!({ "id": r.get::<i64, _>("id"), "status": r.get::<String, _>("status"), "created_at": r.get::<DateTime<Utc>, _>("created_at"), "decided_at": r.get::<Option<DateTime<Utc>>, _>("decided_at") }))
        .collect::<Vec<_>>();
    let viewers = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {VIEWER_COLS} FROM client_viewers WHERE user_id = $1 ORDER BY id"))).bind(s.subject_id).fetch_all(&st.pool).await?.iter().map(viewer_json).collect::<Vec<_>>();
    let reqs = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {REQUEST_COLS} FROM client_requests WHERE user_id = $1 ORDER BY id"))).bind(s.subject_id).fetch_all(&st.pool).await?.iter().map(request_json).collect::<Vec<_>>();
    audit::record(&st.pool, &ctx, entry(s.tenant_id, s.subject_id, "user.data_exported", json!({ "request_id": id }))).await;
    Ok(Json(json!({
        "generated_at": Utc::now(),
        "profile": profile,
        "sessions": sessions,
        "activity": activity,
        "kyc_cases": kyc,
        "viewers": viewers,
        "requests": reqs,
        "note": "Trading history, statements and wallet records are available in the Client Area (Portfolio → Statements, Wallet → History).",
    })))
}

// ---------- Back Office ----------

/// `GET /v1/admin/users/{id}/security`: the client's closure / export requests and view-only logins.
pub async fn admin_user_security(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "clients.read").await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1 AND tenant_id = $2").bind(id).bind(me.tenant_id).fetch_optional(&st.pool).await?;
    exists.ok_or(ApiError::NotFound)?;
    let reqs = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT r.{}, s.name AS handled_by_name FROM client_requests r LEFT JOIN staff s ON s.id = r.handled_by WHERE r.user_id = $1 ORDER BY r.id DESC LIMIT 50",
        REQUEST_COLS.replace(", ", ", r.")
    )))
    .bind(id)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| {
        let mut v = request_json(r);
        v["handled_by"] = json!(r.get::<Option<String>, _>("handled_by_name"));
        v
    })
    .collect::<Vec<_>>();
    let viewers = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {VIEWER_COLS} FROM client_viewers WHERE user_id = $1 ORDER BY revoked_at IS NOT NULL, id DESC")))
        .bind(id)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(viewer_json)
        .collect::<Vec<_>>();
    Ok(Json(json!({ "requests": reqs, "viewers": viewers, "can_process": me.can("clients.write"), "can_revoke": me.can("sessions.revoke") })))
}

#[derive(Deserialize, Default)]
pub struct RevokeAllReq {
    reason: Option<String>,
}

/// `POST /v1/admin/users/{id}/sessions/revoke-all`: signs a client out everywhere (viewer sessions included).
pub async fn admin_revoke_all(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<RevokeAllReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "sessions.revoke").await?;
    let reason: Option<String> = req.ok().and_then(|Json(r)| r.reason).map(|s| s.trim().chars().take(300).collect::<String>()).filter(|s| !s.is_empty());
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1 AND tenant_id = $2").bind(id).bind(me.tenant_id).fetch_optional(&st.pool).await?;
    exists.ok_or(ApiError::NotFound)?;
    let n = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE subject_kind = 'user' AND subject_id = $1 AND tenant_id = $2 AND revoked_at IS NULL")
        .bind(id)
        .bind(me.tenant_id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "admin.sessions_revoked", target: Some(("user", id)), meta: json!({ "count": n, "reason": reason }) }).await;
    Ok(Json(json!({ "status": "ok", "revoked": n })))
}

/// `POST /v1/admin/users/{id}/viewers/{vid}/revoke`: ends a client's view-only login (e.g. on a complaint).
pub async fn admin_revoke_viewer(State(st): State<AppState>, ctx: Ctx, Path((id, vid)): Path<(i64, i64)>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "clients.write").await?;
    let label: Option<String> = sqlx::query_scalar(
        "UPDATE client_viewers SET revoked_at = COALESCE(revoked_at, now()), updated_at = now() WHERE id = $1 AND user_id = $2 AND tenant_id = $3 RETURNING label",
    )
    .bind(vid)
    .bind(id)
    .bind(me.tenant_id)
    .fetch_optional(&st.pool)
    .await?;
    let label = label.ok_or(ApiError::NotFound)?;
    let ended = end_viewer_sessions(&st, vid).await?;
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "viewer.revoked", target: Some(("user", id)), meta: json!({ "viewer_id": vid, "label": label, "sessions_ended": ended, "by": "staff" }) }).await;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize, Default)]
pub struct QueueQuery {
    status: Option<String>,
}

/// `GET /v1/admin/requests?status=open|in_progress|completed|rejected|cancelled|pending`: the tenant's queue.
pub async fn admin_requests(State(st): State<AppState>, ctx: Ctx, q: Result<Query<QueueQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "clients.read").await?;
    let Query(q) = q.map_err(|_| ApiError::BadRequest("Invalid query."))?;
    let status = q.status.unwrap_or_else(|| "pending".into());
    let statuses: Vec<String> = match status.as_str() {
        "pending" => vec!["open".into(), "in_progress".into()],
        "open" | "in_progress" | "completed" | "rejected" | "cancelled" => vec![status.clone()],
        "all" => ["open", "in_progress", "completed", "rejected", "cancelled"].map(String::from).to_vec(),
        _ => return Err(ApiError::BadRequest("Unknown status.")),
    };
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT r.{}, r.user_id, u.email, u.first_name || ' ' || u.last_name AS name FROM client_requests r JOIN users u ON u.id = r.user_id
         WHERE r.tenant_id = $1 AND r.status = ANY($2) ORDER BY r.id DESC LIMIT 200",
        REQUEST_COLS.replace(", ", ", r.")
    )))
    .bind(me.tenant_id)
    .bind(&statuses)
    .fetch_all(&st.pool)
    .await?;
    let items = rows
        .iter()
        .map(|r| {
            let mut v = request_json(r);
            v["client"] = json!({ "id": r.get::<i64, _>("user_id"), "email": r.get::<String, _>("email"), "name": r.get::<String, _>("name") });
            v
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "items": items })))
}

#[derive(Deserialize)]
pub struct ProcessReq {
    #[serde(default)]
    status: String,
    note: Option<String>,
}

/// `POST /v1/admin/requests/{id}` `{status: in_progress|completed|rejected, note?}`. Completing a closure closes
/// the client record (sign-in blocked, sessions and view-only logins ended); the data is retained until a Super
/// Admin erases it (`POST /v1/admin/users/{id}/erase`, client_erase.rs).
pub async fn admin_process_request(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<ProcessReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "clients.write").await?;
    let status = match r.status.as_str() {
        "in_progress" => "in_progress",
        "completed" => "completed",
        "rejected" => "rejected",
        _ => return Err(ApiError::Validation { field: "status", message: "Choose in progress, completed or rejected." }),
    };
    let note: Option<String> = r.note.map(|n| n.trim().chars().take(1000).collect::<String>()).filter(|n| !n.is_empty());
    if status == "rejected" && note.is_none() {
        return Err(ApiError::Validation { field: "note", message: "Tell the client why the request was rejected." });
    }
    let mut tx = st.pool.begin().await?;
    let cur = sqlx::query("SELECT user_id, kind, status FROM client_requests WHERE id = $1 AND tenant_id = $2 FOR UPDATE")
        .bind(id)
        .bind(me.tenant_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let before: String = cur.get("status");
    if !matches!(before.as_str(), "open" | "in_progress") {
        return Err(ApiError::BadRequest("This request is already closed."));
    }
    let (user_id, kind): (i64, String) = (cur.get("user_id"), cur.get("kind"));
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "UPDATE client_requests SET status = $2, staff_note = COALESCE($3, staff_note), handled_by = $4, updated_at = now(),
                closed_at = CASE WHEN $2 IN ('completed', 'rejected') THEN now() ELSE NULL END
         WHERE id = $1 RETURNING {REQUEST_COLS}"
    )))
    .bind(id)
    .bind(status)
    .bind(&note)
    .bind(me.id)
    .fetch_one(&mut *tx)
    .await?;
    let mut ended = 0;
    if kind == "closure" && status == "completed" {
        sqlx::query("UPDATE users SET status = 'closed', updated_at = now() WHERE id = $1").bind(user_id).execute(&mut *tx).await?;
        ended = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE subject_kind = 'user' AND subject_id = $1 AND revoked_at IS NULL").bind(user_id).execute(&mut *tx).await?.rows_affected();
        sqlx::query("UPDATE client_viewers SET revoked_at = COALESCE(revoked_at, now()) WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    audit::record(&st.pool, &ctx, Entry {
        tenant_id: me.tenant_id,
        actor_kind: "staff",
        actor_id: Some(me.id),
        action: "admin.client_request_processed",
        target: Some(("user", user_id)),
        meta: json!({ "request_id": id, "kind": kind, "before": before, "after": status, "note": note, "sessions_ended": ended }),
    })
    .await;
    Ok(Json(json!({ "request": request_json(&row) })))
}

// ---------- Back Office: client session settings ----------

pub async fn get_session_settings(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "settings.read").await?;
    let idle: i32 = sqlx::query_scalar("SELECT client_idle_minutes FROM tenants WHERE id = $1").bind(me.tenant_id).fetch_one(&st.pool).await?;
    Ok(Json(json!({ "client_idle_minutes": idle, "client_max_days": identity::policy(K).session_ttl.num_days(), "can_edit": me.can("settings.write") })))
}

#[derive(Deserialize)]
pub struct SessionSettingsReq {
    client_idle_minutes: i32,
}

pub async fn set_session_settings(State(st): State<AppState>, ctx: Ctx, req: Result<Json<SessionSettingsReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "settings.write").await?;
    if !(5..=10080).contains(&r.client_idle_minutes) {
        return Err(ApiError::Validation { field: "client_idle_minutes", message: "Choose between 5 minutes and 7 days." });
    }
    let before: i32 = sqlx::query_scalar("UPDATE tenants t SET client_idle_minutes = $2, updated_at = now() FROM tenants o WHERE t.id = $1 AND o.id = t.id RETURNING o.client_idle_minutes")
        .bind(me.tenant_id)
        .bind(r.client_idle_minutes)
        .fetch_one(&st.pool)
        .await?;
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "settings.client_idle_updated", target: None, meta: json!({ "before": before, "after": r.client_idle_minutes }) }).await;
    Ok(Json(json!({ "client_idle_minutes": r.client_idle_minutes })))
}

#[cfg(test)]
mod tests;

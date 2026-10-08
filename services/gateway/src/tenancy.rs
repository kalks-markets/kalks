//! Per-tenant runtime configuration (D112, D146): module toggles, feature flags and maintenance mode.
//!
//! * Modules (copy trading, PAMM, MAM, prop, IB, algo, public API, academy, wallet, rewards, options, news,
//!   calendar, markets, AI, support chat, Kalks Circle) are switched per tenant by the Platform Owner only. Off = hidden in the
//!   Client Area, Kalks Trader, the app and the Back Office nav of the tenant's staff; the BFFs and the services
//!   refuse its client calls with 403 `module_disabled` (they read the effective map from
//!   `/v1/internal/tenants/{slug}`, internal.rs, or `tenant_features` directly).
//! * Feature flags are switched by the tenant (`settings.write`); the Platform Owner can add new flags.
//! * Maintenance mode: clients see a maintenance page and can't sign in; staff keep working.
//!
//! `GET /v1/public/tenant-config` gives the apps everything they need in one call (internal token only),
//! including the tenant's branding; the tenant is the one of the forwarded browser host (domains.rs).

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};

use crate::admin::{Staff, require_key};
use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, Ctx};

pub struct FeatureDef {
    pub key: &'static str,
    pub kind: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub default_enabled: bool,
}

/// Built-in modules and flags (upserted on every start).
pub const BUILTIN_FEATURES: &[FeatureDef] = &[
    FeatureDef { key: "copy_trading", kind: "module", name: "Copy trading", description: "Masters, followers and the social discovery pages.", default_enabled: true },
    FeatureDef { key: "pamm", kind: "module", name: "PAMM funds", description: "Managed funds with performance fees.", default_enabled: true },
    FeatureDef { key: "prop", kind: "module", name: "Prop firm", description: "Challenges, funded accounts and payouts.", default_enabled: true },
    FeatureDef { key: "ib", kind: "module", name: "IB programme", description: "Partner dashboard, referral links and commissions.", default_enabled: true },
    FeatureDef { key: "algo", kind: "module", name: "Strategy builder", description: "Visual / DSL strategies, backtests, deployments, marketplace.", default_enabled: true },
    FeatureDef { key: "api", kind: "module", name: "Public API & webhooks", description: "Client API keys, webhooks and API docs.", default_enabled: true },
    FeatureDef { key: "academy", kind: "module", name: "Academy", description: "Courses, quizzes, exams and certificates.", default_enabled: true },
    FeatureDef { key: "wallet", kind: "module", name: "USDT wallet", description: "Crypto deposits, withdrawals and transfers.", default_enabled: true },
    FeatureDef { key: "rewards", kind: "module", name: "Rewards", description: "Contests, loyalty and cashback.", default_enabled: true },
    FeatureDef { key: "options", kind: "module", name: "Options", description: "FX options: the Options page, Kalks Trader's options workspace and new option orders.", default_enabled: true },
    FeatureDef { key: "news", kind: "module", name: "News", description: "Market news, the news map and the daily brief.", default_enabled: true },
    FeatureDef { key: "calendar", kind: "module", name: "Economic calendar", description: "Economic events, reminders and event alerts.", default_enabled: true },
    FeatureDef { key: "markets", kind: "module", name: "Markets", description: "The market overview pages: quotes, movers and heat maps.", default_enabled: true },
    FeatureDef { key: "ai", kind: "module", name: "AI assistants", description: "Ask Kalks AI and AI Trader.", default_enabled: true },
    FeatureDef { key: "mam", kind: "module", name: "MAM", description: "Multi-account managers and the accounts they manage.", default_enabled: true },
    FeatureDef { key: "support_chat", kind: "module", name: "Support chat", description: "Live chat with the support bot and team. Email support stays.", default_enabled: true },
    FeatureDef { key: "circle", kind: "module", name: "Kalks Circle", description: "The trader community: profiles, posts, stories, chat, rooms and video.", default_enabled: true },
    FeatureDef { key: "client_registration", kind: "flag", name: "New client sign-ups", description: "Visitors can open an account. Off: existing clients can still sign in.", default_enabled: true },
    FeatureDef { key: "google_login", kind: "flag", name: "Continue with Google", description: "Clients can sign in and sign up with Google.", default_enabled: true },
    FeatureDef { key: "trade_sharing", kind: "flag", name: "Trade share links", description: "Clients can publish read-only links to their trades.", default_enabled: true },
    FeatureDef { key: "demo_accounts", kind: "flag", name: "Demo accounts", description: "Clients can open demo trading accounts.", default_enabled: true },
];

/// Modules split out of an older one: when the new key first appears, a broker that had the old module off keeps
/// the split-off part off too (MAM pages lived under copy trading's `/social`).
const SPLIT_FROM: &[(&str, &str)] = &[("mam", "copy_trading")];

pub async fn seed_catalogue(pool: &PgPool) -> anyhow::Result<()> {
    for f in BUILTIN_FEATURES {
        let inserted: bool = sqlx::query_scalar(
            "INSERT INTO feature_flags (key, kind, name, description, default_enabled, builtin) VALUES ($1,$2,$3,$4,$5,true)
             ON CONFLICT (key) DO UPDATE SET kind = EXCLUDED.kind, name = EXCLUDED.name, description = EXCLUDED.description,
                 default_enabled = EXCLUDED.default_enabled, builtin = true
             RETURNING (xmax = 0)",
        )
        .bind(f.key)
        .bind(f.kind)
        .bind(f.name)
        .bind(f.description)
        .bind(f.default_enabled)
        .fetch_one(pool)
        .await?;
        if let Some((_, from)) = SPLIT_FROM.iter().find(|(k, _)| *k == f.key).filter(|_| inserted) {
            sqlx::query(
                "INSERT INTO tenant_features (tenant_id, key, enabled, updated_by)
                 SELECT tenant_id, $1, false, updated_by FROM tenant_features WHERE key = $2 AND NOT enabled
                 ON CONFLICT (tenant_id, key) DO NOTHING",
            )
            .bind(f.key)
            .bind(from)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// The broker's effective modules (`key -> on`): its overrides, else the platform defaults.
pub async fn module_map(pool: &PgPool, tenant_id: i64) -> ApiResult<Map<String, Value>> {
    Ok(features(pool, tenant_id).await?.into_iter().filter(|f| f.kind == "module").map(|f| (f.key, json!(f.enabled))).collect())
}

pub struct Feature {
    pub key: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub default_enabled: bool,
    pub builtin: bool,
    pub enabled: bool,
    pub overridden: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

pub async fn features(pool: &PgPool, tenant_id: i64) -> ApiResult<Vec<Feature>> {
    let rows = sqlx::query(
        "SELECT f.key, f.kind, f.name, f.description, f.default_enabled, f.builtin, tf.enabled, tf.updated_at
         FROM feature_flags f LEFT JOIN tenant_features tf ON tf.key = f.key AND tf.tenant_id = $1
         ORDER BY f.kind DESC, f.builtin DESC, f.created_at, f.key",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let over: Option<bool> = r.get("enabled");
            let def: bool = r.get("default_enabled");
            Feature {
                key: r.get("key"),
                kind: r.get("kind"),
                name: r.get("name"),
                description: r.get("description"),
                default_enabled: def,
                builtin: r.get("builtin"),
                enabled: over.unwrap_or(def),
                overridden: over.is_some(),
                updated_at: r.get("updated_at"),
            }
        })
        .collect())
}

pub async fn enabled(pool: &PgPool, tenant_id: i64, key: &str) -> ApiResult<bool> {
    let v: Option<bool> = sqlx::query_scalar(
        "SELECT COALESCE(tf.enabled, f.default_enabled) FROM feature_flags f
         LEFT JOIN tenant_features tf ON tf.key = f.key AND tf.tenant_id = $1 WHERE f.key = $2",
    )
    .bind(tenant_id)
    .bind(key)
    .fetch_optional(pool)
    .await?;
    Ok(v.unwrap_or(true))
}

/// Rejects a client action behind a switched-off flag or module.
pub async fn require_feature(st: &AppState, tenant_id: i64, key: &str) -> ApiResult<()> {
    if enabled(&st.pool, tenant_id, key).await? {
        return Ok(());
    }
    Err(ApiError::Coded { status: StatusCode::FORBIDDEN, code: "feature_disabled", message: "This feature isn't available right now." })
}

pub struct Maintenance {
    pub enabled: bool,
    pub active: bool,
    pub message: Option<String>,
    pub until: Option<DateTime<Utc>>,
    pub since: Option<DateTime<Utc>>,
}

pub async fn maintenance(pool: &PgPool, tenant_id: i64) -> ApiResult<Maintenance> {
    let r = sqlx::query("SELECT maintenance_enabled, maintenance_message, maintenance_until, maintenance_since FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(pool).await?;
    let enabled: bool = r.get("maintenance_enabled");
    let until: Option<DateTime<Utc>> = r.get("maintenance_until");
    Ok(Maintenance {
        enabled,
        // a window with an end time switches itself off once it has passed
        active: enabled && until.is_none_or(|u| u > Utc::now()),
        message: r.get("maintenance_message"),
        until,
        since: r.get("maintenance_since"),
    })
}

pub const DEFAULT_MAINTENANCE_MESSAGE: &str = "We're upgrading the platform. Your funds and open positions are safe. Please check back shortly.";

/// Client sign-in / sign-up gate during maintenance (staff are never affected).
pub async fn client_gate(st: &AppState, tenant_id: i64) -> ApiResult<()> {
    if maintenance(&st.pool, tenant_id).await?.active {
        return Err(ApiError::Coded { status: StatusCode::SERVICE_UNAVAILABLE, code: "maintenance", message: "The Client Area is under maintenance. Please try again shortly." });
    }
    Ok(())
}

fn maintenance_json(m: &Maintenance) -> Value {
    json!({ "enabled": m.enabled, "active": m.active, "message": m.message.clone().unwrap_or_else(|| DEFAULT_MAINTENANCE_MESSAGE.into()), "until": m.until, "since": m.since })
}

fn feature_json(f: &Feature) -> Value {
    json!({ "key": f.key, "kind": f.kind, "name": f.name, "description": f.description, "enabled": f.enabled, "default": f.default_enabled,
            "overridden": f.overridden, "builtin": f.builtin, "updated_at": f.updated_at })
}

// ---------- GET /v1/public/tenant-config ----------

pub async fn public_config(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let t = sqlx::query("SELECT id, slug, name, status, brand FROM tenants WHERE slug = $1").bind(&ctx.tenant_slug).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let id: i64 = t.get("id");
    let m = maintenance(&st.pool, id).await?;
    let mut modules = Map::new();
    let mut flags = Map::new();
    for f in features(&st.pool, id).await? {
        if f.kind == "module" { &mut modules } else { &mut flags }.insert(f.key, json!(f.enabled));
    }
    let status: String = t.get("status");
    Ok(Json(json!({
        "tenant": { "slug": t.get::<String, _>("slug"), "name": t.get::<String, _>("name"), "status": status, "brand": t.get::<sqlx::types::Json<Value>, _>("brand").0 },
        // a suspended tenant looks like maintenance to its clients
        "maintenance": if status == "active" { maintenance_json(&m) } else { json!({ "enabled": true, "active": true, "message": "This service is temporarily unavailable.", "until": null, "since": null }) },
        "modules": modules,
        "flags": flags,
        // name, logo, colours, support email and domains by kind (domains.rs)
        "branding": crate::domains::branding(&st, id).await?,
    })))
}

// ---------- tenant settings: /v1/admin/settings/* ----------

fn entry<'a>(me: &Staff, action: &'a str, meta: Value) -> Entry<'a> {
    Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action, target: None, meta }
}

pub async fn get_maintenance(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "settings.read").await?;
    let m = maintenance(&st.pool, me.tenant_id).await?;
    let mut v = maintenance_json(&m);
    v["default_message"] = json!(DEFAULT_MAINTENANCE_MESSAGE);
    v["can_edit"] = json!(me.can("settings.write"));
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct MaintenanceReq {
    enabled: bool,
    message: Option<String>,
    until: Option<DateTime<Utc>>,
}

pub async fn set_maintenance(State(st): State<AppState>, ctx: Ctx, req: Result<Json<MaintenanceReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "settings.write").await?;
    let message: Option<String> = r.message.map(|m| m.trim().chars().take(500).collect::<String>()).filter(|m| !m.is_empty());
    if let Some(u) = r.until
        && r.enabled
        && u <= Utc::now()
    {
        return Err(ApiError::Validation { field: "until", message: "The end time must be in the future." });
    }
    let before = maintenance(&st.pool, me.tenant_id).await?;
    sqlx::query(
        "UPDATE tenants SET maintenance_enabled = $2, maintenance_message = $3, maintenance_until = $4,
                maintenance_since = CASE WHEN $2 AND NOT maintenance_enabled THEN now() WHEN $2 THEN maintenance_since ELSE NULL END, updated_at = now()
         WHERE id = $1",
    )
    .bind(me.tenant_id)
    .bind(r.enabled)
    .bind(&message)
    .bind(if r.enabled { r.until } else { None })
    .execute(&st.pool)
    .await?;
    let action = match (before.active, r.enabled) {
        (false, true) => "settings.maintenance_on",
        (true, false) => "settings.maintenance_off",
        _ => "settings.maintenance_updated",
    };
    audit::record(&st.pool, &ctx, entry(&me, action, json!({
        "before": {"enabled": before.enabled, "message": before.message, "until": before.until},
        "after": {"enabled": r.enabled, "message": message, "until": r.until},
    })))
    .await;
    let after = maintenance(&st.pool, me.tenant_id).await?;
    Ok(Json(maintenance_json(&after)))
}

pub async fn get_features(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "settings.read").await?;
    let all = features(&st.pool, me.tenant_id).await?;
    Ok(Json(json!({
        "modules": all.iter().filter(|f| f.kind == "module").map(feature_json).collect::<Vec<_>>(),
        "flags": all.iter().filter(|f| f.kind == "flag").map(feature_json).collect::<Vec<_>>(),
        "can_edit_flags": me.can("settings.write"),
        "can_edit_modules": me.can("owner.tenants"),
    })))
}

#[derive(Deserialize)]
pub struct FeatureReq {
    /// true / false, or null to go back to the platform default.
    enabled: Option<bool>,
    /// Why (shown in the audit log).
    #[serde(default)]
    reason: Option<String>,
}

/// An optional reason for a switch, trimmed to 300 characters (None when blank).
pub fn clean_reason(r: Option<&str>) -> Option<String> {
    r.map(|r| r.trim().chars().take(300).collect::<String>()).filter(|r| !r.is_empty())
}

/// Sets (or clears) one tenant override and audits it (with the optional `reason`). Modules need `owner.tenants`.
pub async fn write_feature(st: &AppState, ctx: &Ctx, me: &Staff, tenant_id: i64, key: &str, value: Option<bool>, reason: Option<&str>) -> ApiResult<Value> {
    let f = features(&st.pool, tenant_id).await?.into_iter().find(|f| f.key == key).ok_or(ApiError::NotFound)?;
    if f.kind == "module" && !me.can("owner.tenants") {
        return Err(ApiError::Coded { status: StatusCode::FORBIDDEN, code: "forbidden", message: "Modules are switched by the platform owner." });
    }
    match value {
        Some(v) => {
            sqlx::query(
                "INSERT INTO tenant_features (tenant_id, key, enabled, updated_by) VALUES ($1,$2,$3,$4)
                 ON CONFLICT (tenant_id, key) DO UPDATE SET enabled = EXCLUDED.enabled, updated_by = EXCLUDED.updated_by, updated_at = now()",
            )
            .bind(tenant_id)
            .bind(key)
            .bind(v)
            .bind(me.id)
            .execute(&st.pool)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM tenant_features WHERE tenant_id = $1 AND key = $2").bind(tenant_id).bind(key).execute(&st.pool).await?;
        }
    }
    let after = value.unwrap_or(f.default_enabled);
    audit::record(&st.pool, ctx, Entry {
        tenant_id: me.tenant_id,
        actor_kind: "staff",
        actor_id: Some(me.id),
        action: if f.kind == "module" { "settings.module_toggled" } else { "settings.flag_toggled" },
        target: (tenant_id != me.tenant_id).then_some(("tenant", tenant_id)),
        meta: json!({"key": key, "tenant_id": tenant_id, "reason": clean_reason(reason), "before": {"enabled": f.enabled}, "after": {"enabled": after, "default": value.is_none()}}),
    })
    .await;
    Ok(json!({ "key": key, "enabled": after, "overridden": value.is_some() }))
}

pub async fn set_feature(State(st): State<AppState>, ctx: Ctx, Path(key): Path<String>, req: Result<Json<FeatureReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "settings.write").await?;
    Ok(Json(write_feature(&st, &ctx, &me, me.tenant_id, &key, r.enabled, r.reason.as_deref()).await?))
}

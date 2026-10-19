//! Client Area sections and their targeting rules (docs/SECTIONS.md).
//!
//! * A section is a Client Area page / menu entry (`page.wallet.deposit`) or a dashboard block
//!   (`dashboard.wallet_card`), kind `section` in the feature catalogue. The same keys, with their paths, nav groups
//!   and modules, are in packages/ui/src/lib/sections.ts (the apps' copy; a test below keeps both lists equal).
//! * The broker's staff with `settings.write` switch sections (modules stay with the Platform Owner, tenancy.rs), for
//!   everyone or by rule: the client's country (their registered country), the account group of any of their trading
//!   accounts, or the client. Precedence: user > group > country > all > catalogue default. A section inside a
//!   switched-off module stays off whatever its rules say. Switching a section off only hides it; nothing is deleted.
//! * Account groups live in the trading engine, which the gateway doesn't call: the Client Area BFF reads the client's
//!   accounts and sends their group codes with `GET /v1/me/features?groups=` (internal token only, never from the
//!   browser). The answer says whether the broker has any group rule (`group_rules`), so the BFF only asks the engine
//!   when one exists.
//! * Every change is audited with its reason and bumps `tenants.features_version`, which /v1/public/tenant-config
//!   carries: the apps drop their per-client caches when it moves.
//!
//! Rules are read and written inside a tenant-scoped transaction (`domains::tenant_tx`, row-level security).

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};

use crate::admin::{Staff, require_key};
use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::domains::tenant_tx;
use crate::error::{ApiError, ApiResult};
use crate::identity::{self, Kind};
use crate::state::{AppState, Ctx};
use crate::tenancy::{self, clean_reason};

pub struct SectionDef {
    pub key: &'static str,
    pub name: &'static str,
    /// The module it belongs to: off while that module is off.
    pub module: Option<&'static str>,
}

const fn s(key: &'static str, name: &'static str, module: Option<&'static str>) -> SectionDef {
    SectionDef { key, name, module }
}

/// Every switchable Client Area page and dashboard block, on by default (upserted on every start).
pub const BUILTIN_SECTIONS: &[SectionDef] = &[
    s("dashboard.ask_ai", "Ask Kalks AI bar", Some("ai")),
    s("dashboard.balance_total", "Total balance", None),
    s("dashboard.balance_equity", "Equity", None),
    s("dashboard.wallet_card", "Wallet balance", Some("wallet")),
    s("dashboard.today_pnl", "Today's P&L", None),
    s("dashboard.open_positions", "Open positions", None),
    s("dashboard.rewards_card", "Reward points", Some("rewards")),
    s("dashboard.banners", "Promotion banners", None),
    s("dashboard.shortcuts", "Shortcuts", None),
    s("dashboard.accounts", "Your accounts", None),
    s("page.markets", "Markets", Some("markets")),
    s("page.news", "News", Some("news")),
    s("page.calendar", "Calendar", Some("calendar")),
    s("page.accounts", "My accounts", None),
    s("page.accounts.new", "Open account", None),
    s("page.wallet", "Wallet overview", Some("wallet")),
    s("page.wallet.deposit", "Deposit", Some("wallet")),
    s("page.wallet.withdraw", "Withdraw", Some("wallet")),
    s("page.wallet.transfer", "Transfer", Some("wallet")),
    s("page.wallet.history", "Wallet history", Some("wallet")),
    s("page.portfolio", "Portfolio overview", None),
    s("page.portfolio.analytics", "Analytics", None),
    s("page.portfolio.history", "Trade history", None),
    s("page.portfolio.ledger", "Ledger", None),
    s("page.portfolio.statements", "Statements", None),
    s("page.options", "Options", Some("options")),
    s("page.partner", "Partner dashboard", Some("ib")),
    s("page.partner.clients", "Partner clients", Some("ib")),
    s("page.partner.network", "Network", Some("ib")),
    s("page.partner.commissions", "Commissions", Some("ib")),
    s("page.partner.links", "Links & materials", Some("ib")),
    s("page.partner.payouts", "Partner payouts", Some("ib")),
    s("page.social", "Discover", Some("copy_trading")),
    s("page.social.copy", "Copy trading", Some("copy_trading")),
    s("page.social.pamm", "PAMM funds", Some("pamm")),
    s("page.social.investments", "My investments", Some("pamm")),
    s("page.social.managed", "Managed accounts", Some("mam")),
    s("page.social.master", "Become a master", Some("copy_trading")),
    s("page.social.mam", "MAM manager", Some("mam")),
    s("page.prop", "Challenges", Some("prop")),
    s("page.prop.mine", "My challenges", Some("prop")),
    s("page.prop.payouts", "Prop payouts", Some("prop")),
    s("page.prop.certificates", "Certificates", Some("prop")),
    s("page.rewards", "Contests", Some("rewards")),
    s("page.rewards.loyalty", "Loyalty", Some("rewards")),
    s("page.rewards.cashback", "Cashback", Some("rewards")),
    s("page.rewards.promotions", "Promotions", Some("rewards")),
    s("page.developer", "API keys", Some("api")),
    s("page.developer.webhooks", "Webhooks", Some("api")),
    s("page.developer.strategies", "Strategy builder", Some("algo")),
    s("page.developer.deployments", "Running strategies", Some("algo")),
    s("page.developer.backtests", "Backtests", Some("algo")),
    s("page.developer.marketplace", "Marketplace", Some("algo")),
    s("page.developer.docs", "API docs", Some("api")),
    s("page.academy", "Courses", Some("academy")),
    s("page.academy.glossary", "Glossary", Some("academy")),
    s("page.academy.progress", "My progress", Some("academy")),
    s("page.academy.coach", "AI Coach", Some("academy")),
    s("page.profile", "Profile", None),
    s("page.profile.security", "Security", None),
    s("page.profile.verification", "Verification", None),
    s("page.profile.viewers", "View-only access", None),
    s("page.profile.notifications", "Notification settings", None),
    s("page.profile.preferences", "Preferences", None),
    s("page.support", "Support", None),
];

pub fn def(key: &str) -> Option<&'static SectionDef> {
    BUILTIN_SECTIONS.iter().find(|d| d.key == key)
}

/// What a section is: a page (with its Client Area path) or a dashboard block.
fn describe(d: &SectionDef) -> String {
    match d.key.strip_prefix("page.") {
        Some(p) => format!("Client Area page /{}", p.replace('.', "/")),
        None => "Dashboard block".into(),
    }
}

/// Upserts the built-in sections and drops built-in ones that are gone (their rules go with them).
pub async fn seed(pool: &PgPool) -> anyhow::Result<()> {
    for d in BUILTIN_SECTIONS {
        sqlx::query(
            "INSERT INTO feature_flags (key, kind, name, description, default_enabled, builtin) VALUES ($1,'section',$2,$3,true,true)
             ON CONFLICT (key) DO UPDATE SET kind = 'section', name = EXCLUDED.name, description = EXCLUDED.description, builtin = true",
        )
        .bind(d.key)
        .bind(d.name)
        .bind(describe(d))
        .execute(pool)
        .await?;
    }
    let keys: Vec<&str> = BUILTIN_SECTIONS.iter().map(|d| d.key).collect();
    sqlx::query("DELETE FROM feature_flags WHERE kind = 'section' AND builtin AND NOT (key = ANY($1))").bind(&keys).execute(pool).await?;
    Ok(())
}

/// Tells the apps that the broker's switches changed (their per-client caches compare this number).
pub async fn bump(pool: &PgPool, tenant_id: i64) -> ApiResult<()> {
    sqlx::query("UPDATE tenants SET features_version = features_version + 1 WHERE id = $1").bind(tenant_id).execute(pool).await?;
    Ok(())
}

pub async fn version(pool: &PgPool, tenant_id: i64) -> ApiResult<i64> {
    Ok(sqlx::query_scalar("SELECT features_version FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(pool).await?)
}

// ---------- evaluation ----------

/// Who a section is judged for. `None` fields don't match any rule of that scope.
#[derive(Default, Clone, Debug)]
pub struct Who {
    pub user_id: Option<i64>,
    /// ISO code, lowercase (the client's registered country).
    pub country: Option<String>,
    /// Account group codes of the client's trading accounts.
    pub groups: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Rule {
    pub id: i64,
    pub key: String,
    pub scope: String,
    pub value: String,
    pub enabled: bool,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<String>,
    /// User rules: the client's email.
    pub label: Option<String>,
}

/// One section's verdict and where it came from: `module` (its module is off), `user`, `group`, `country`, `all`
/// or `default`. Two group rules that disagree: on wins (one of the client's accounts is in a group that has it).
pub fn decide(default: bool, module_on: bool, rules: &[&Rule], who: &Who) -> (bool, &'static str) {
    if !module_on {
        return (false, "module");
    }
    let uid = who.user_id.map(|u| u.to_string());
    if let Some(r) = rules.iter().find(|r| r.scope == "user" && Some(&r.value) == uid.as_ref()) {
        return (r.enabled, "user");
    }
    let groups: Vec<&&Rule> = rules.iter().filter(|r| r.scope == "group" && who.groups.contains(&r.value)).collect();
    if !groups.is_empty() {
        return (groups.iter().any(|r| r.enabled), "group");
    }
    if let Some(r) = rules.iter().find(|r| r.scope == "country" && Some(&r.value) == who.country.as_ref()) {
        return (r.enabled, "country");
    }
    if let Some(r) = rules.iter().find(|r| r.scope == "all") {
        return (r.enabled, "all");
    }
    (default, "default")
}

/// The broker's rules (all sections), read under row-level security.
pub async fn rules(pool: &PgPool, tenant_id: i64) -> ApiResult<Vec<Rule>> {
    let mut tx = tenant_tx(pool, tenant_id).await?;
    let rows = sqlx::query(
        "SELECT r.id, r.key, r.scope, r.value, r.enabled, r.updated_at, s.name AS by_name, u.email
         FROM feature_rules r LEFT JOIN staff s ON s.id = r.updated_by
         LEFT JOIN users u ON r.scope = 'user' AND u.id::text = r.value
         ORDER BY CASE r.scope WHEN 'all' THEN 0 WHEN 'country' THEN 1 WHEN 'group' THEN 2 ELSE 3 END, r.value, r.id",
    )
    .fetch_all(&mut *tx)
    .await?;
    Ok(rows
        .iter()
        .map(|r| Rule {
            id: r.get("id"),
            key: r.get("key"),
            scope: r.get("scope"),
            value: r.get("value"),
            enabled: r.get("enabled"),
            updated_at: r.get("updated_at"),
            updated_by: r.get("by_name"),
            label: r.get("email"),
        })
        .collect())
}

/// Defaults of the section catalogue (`key -> default_enabled`), in catalogue order.
async fn catalogue(pool: &PgPool) -> ApiResult<Vec<(String, bool)>> {
    let rows = sqlx::query("SELECT key, default_enabled FROM feature_flags WHERE kind = 'section'").fetch_all(pool).await?;
    let mut out: Vec<(String, bool)> = rows.iter().map(|r| (r.get("key"), r.get("default_enabled"))).collect();
    let pos = |k: &str| BUILTIN_SECTIONS.iter().position(|d| d.key == k).unwrap_or(usize::MAX);
    out.sort_by_key(|(k, _)| pos(k));
    Ok(out)
}

pub struct Effective {
    pub modules: Map<String, Value>,
    pub sections: Map<String, Value>,
    /// Per section: where its verdict came from (see `decide`).
    pub sources: Map<String, Value>,
    pub group_rules: bool,
}

/// The broker's modules and sections for `who`; `None` = tenant-wide (only rules for everyone count).
pub async fn effective(pool: &PgPool, tenant_id: i64, who: Option<&Who>) -> ApiResult<Effective> {
    let modules = tenancy::module_map(pool, tenant_id).await?;
    let all = rules(pool, tenant_id).await?;
    let nobody = Who::default();
    let who = who.unwrap_or(&nobody);
    let (mut sections, mut sources) = (Map::new(), Map::new());
    for (key, default) in catalogue(pool).await? {
        let module_on = def(&key).and_then(|d| d.module).is_none_or(|m| modules.get(m) != Some(&json!(false)));
        let mine: Vec<&Rule> = all.iter().filter(|r| r.key == key).collect();
        let (on, from) = decide(default, module_on, &mine, who);
        sections.insert(key.clone(), json!(on));
        sources.insert(key, json!(from));
    }
    Ok(Effective { modules, sections, sources, group_rules: all.iter().any(|r| r.scope == "group") })
}

/// Account group codes from a comma-separated list (trading group codes: `[a-z0-9-]{1,40}`), at most 50.
pub fn clean_groups(raw: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = raw
        .unwrap_or_default()
        .split(',')
        .map(|g| g.trim().to_ascii_lowercase())
        .filter(|g| valid_group(g))
        .collect();
    out.sort();
    out.dedup();
    out.truncate(50);
    out
}

fn valid_group(g: &str) -> bool {
    (1..=40).contains(&g.len()) && g.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

// ---------- GET /v1/me/features (client session; groups from the BFF) ----------

#[derive(Deserialize)]
pub struct MeQuery {
    groups: Option<String>,
}

/// The signed-in client's effective modules, flags and sections.
pub async fn me(State(st): State<AppState>, ctx: Ctx, q: Result<Query<MeQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(|_| ApiError::BadRequest("Invalid query."))?;
    let s = identity::resolve_session_any(&st, &ctx, Kind::User).await?;
    let country: Option<String> = sqlx::query_scalar("SELECT country FROM users WHERE id = $1 AND tenant_id = $2").bind(s.subject_id).bind(s.tenant_id).fetch_optional(&st.pool).await?;
    let who = Who { user_id: Some(s.subject_id), country: identity::clean_country(country.as_deref()), groups: clean_groups(q.groups.as_deref()) };
    let e = effective(&st.pool, s.tenant_id, Some(&who)).await?;
    let flags: Map<String, Value> = tenancy::features(&st.pool, s.tenant_id).await?.into_iter().filter(|f| f.kind == "flag").map(|f| (f.key, json!(f.enabled))).collect();
    Ok(Json(json!({
        "user_id": s.subject_id,
        "country": who.country,
        "groups": who.groups,
        "group_rules": e.group_rules,
        "modules": e.modules,
        "flags": flags,
        "sections": e.sections,
        "version": version(&st.pool, s.tenant_id).await?,
    })))
}

// ---------- Back Office: /v1/admin/settings/sections ----------

fn rule_json(r: &Rule) -> Value {
    json!({ "id": r.id, "scope": r.scope, "value": r.value, "enabled": r.enabled, "updated_at": r.updated_at, "updated_by": r.updated_by, "label": r.label })
}

pub async fn list(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "settings.read").await?;
    let e = effective(&st.pool, me.tenant_id, None).await?;
    let all = rules(&st.pool, me.tenant_id).await?;
    let names = sqlx::query("SELECT key, name, description, default_enabled FROM feature_flags WHERE kind = 'section'").fetch_all(&st.pool).await?;
    let items: Vec<Value> = catalogue(&st.pool)
        .await?
        .iter()
        .map(|(key, default)| {
            let n = names.iter().find(|r| r.get::<String, _>("key") == *key);
            let module = def(key).and_then(|d| d.module);
            let mine: Vec<Value> = all.iter().filter(|r| r.key == *key).map(rule_json).collect();
            json!({
                "key": key,
                "name": n.map(|r| r.get::<String, _>("name")),
                "description": n.map(|r| r.get::<String, _>("description")),
                "module": module,
                "module_on": module.is_none_or(|m| e.modules.get(m) != Some(&json!(false))),
                "default": default,
                // for everyone (rules for everyone and the module; targeted rules can still differ)
                "enabled": e.sections.get(key),
                "rules": mine,
            })
        })
        .collect();
    Ok(Json(json!({
        "sections": items,
        "modules": e.modules,
        "can_edit": me.can("settings.write"),
        "version": version(&st.pool, me.tenant_id).await?,
    })))
}

/// Reason for a section change: required (3+ characters), audited with it.
fn need_reason(r: Option<&str>) -> ApiResult<String> {
    clean_reason(r).filter(|r| r.chars().count() >= 3).ok_or(ApiError::Validation { field: "reason", message: "Give a reason (shown in the audit log)." })
}

fn section_key(key: &str) -> ApiResult<&'static SectionDef> {
    def(key).ok_or(ApiError::NotFound)
}

/// The value of a rule, cleaned for its scope; a user must be a client of this broker.
async fn rule_value(st: &AppState, tenant_id: i64, scope: &str, raw: &str) -> ApiResult<String> {
    let v = raw.trim().to_ascii_lowercase();
    match scope {
        "all" => Ok(String::new()),
        "country" => identity::clean_country(Some(&v)).ok_or(ApiError::Validation { field: "value", message: "Pick a country." }),
        "group" if valid_group(&v) => Ok(v),
        "group" => Err(ApiError::Validation { field: "value", message: "Pick an account group." }),
        "user" => {
            let id: i64 = v.trim_start_matches('#').parse().map_err(|_| ApiError::Validation { field: "value", message: "Pick a client." })?;
            let found: Option<i64> = sqlx::query_scalar("SELECT id FROM users WHERE id = $1 AND tenant_id = $2").bind(id).bind(tenant_id).fetch_optional(&st.pool).await?;
            found.map(|i| i.to_string()).ok_or(ApiError::Validation { field: "value", message: "No client with this ID." })
        }
        _ => Err(ApiError::Validation { field: "scope", message: "Scope is all, country, group or user." }),
    }
}

/// Sets (`Some`) or removes (`None`) one rule of a section and audits it. Returns the rule before the change.
pub async fn write_rule(st: &AppState, ctx: &Ctx, me: &Staff, tenant_id: i64, key: &str, scope: &str, value: &str, enabled: Option<bool>, reason: Option<&str>) -> ApiResult<Value> {
    let d = section_key(key)?;
    let mut tx = tenant_tx(&st.pool, tenant_id).await?;
    let before: Option<bool> = sqlx::query_scalar("SELECT enabled FROM feature_rules WHERE key = $1 AND scope = $2 AND value = $3")
        .bind(d.key)
        .bind(scope)
        .bind(value)
        .fetch_optional(&mut *tx)
        .await?;
    match enabled {
        Some(v) => {
            sqlx::query(
                "INSERT INTO feature_rules (tenant_id, key, scope, value, enabled, updated_by) VALUES ($1,$2,$3,$4,$5,$6)
                 ON CONFLICT (tenant_id, key, scope, value) DO UPDATE SET enabled = EXCLUDED.enabled, updated_by = EXCLUDED.updated_by, updated_at = now()",
            )
            .bind(tenant_id)
            .bind(d.key)
            .bind(scope)
            .bind(value)
            .bind(v)
            .bind(me.id)
            .execute(&mut *tx)
            .await?;
        }
        None => {
            sqlx::query("DELETE FROM feature_rules WHERE key = $1 AND scope = $2 AND value = $3").bind(d.key).bind(scope).bind(value).execute(&mut *tx).await?;
        }
    }
    tx.commit().await?;
    bump(&st.pool, tenant_id).await?;
    let action = match (scope, enabled) {
        ("all", _) => "settings.section_toggled",
        (_, Some(_)) => "settings.section_rule_set",
        (_, None) => "settings.section_rule_removed",
    };
    audit::record(&st.pool, ctx, Entry {
        tenant_id: me.tenant_id,
        actor_kind: "staff",
        actor_id: Some(me.id),
        action,
        target: (tenant_id != me.tenant_id).then_some(("tenant", tenant_id)),
        meta: json!({"key": d.key, "scope": scope, "value": value, "tenant_id": tenant_id, "reason": clean_reason(reason), "before": {"enabled": before}, "after": {"enabled": enabled}}),
    })
    .await;
    Ok(json!({ "key": d.key, "scope": scope, "value": value, "enabled": enabled }))
}

#[derive(Deserialize)]
pub struct ToggleReq {
    /// true / false for everyone, or null to go back to the catalogue default.
    enabled: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
}

/// PUT /v1/admin/settings/sections/{key}: the switch for everyone.
pub async fn toggle(State(st): State<AppState>, ctx: Ctx, Path(key): Path<String>, req: Result<Json<ToggleReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "settings.write").await?;
    section_key(&key)?;
    let reason = need_reason(r.reason.as_deref())?;
    Ok(Json(write_rule(&st, &ctx, &me, me.tenant_id, &key, "all", "", r.enabled, Some(&reason)).await?))
}

#[derive(Deserialize)]
pub struct RuleReq {
    #[serde(default)]
    scope: String,
    #[serde(default)]
    value: String,
    enabled: Option<bool>,
    #[serde(default)]
    reason: Option<String>,
}

/// POST /v1/admin/settings/sections/{key}/rules: adds or changes a targeted rule (`enabled: null` removes it).
pub async fn set_rule(State(st): State<AppState>, ctx: Ctx, Path(key): Path<String>, req: Result<Json<RuleReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "settings.write").await?;
    section_key(&key)?;
    let scope = r.scope.trim().to_ascii_lowercase();
    let value = rule_value(&st, me.tenant_id, &scope, &r.value).await?;
    let reason = need_reason(r.reason.as_deref())?;
    let v = write_rule(&st, &ctx, &me, me.tenant_id, &key, &scope, &value, r.enabled, Some(&reason)).await?;
    Ok((if r.enabled.is_some() { StatusCode::CREATED } else { StatusCode::OK }, Json(v)))
}

#[derive(Deserialize)]
pub struct RemoveReq {
    #[serde(default)]
    reason: Option<String>,
}

/// DELETE /v1/admin/settings/sections/rules/{id}.
pub async fn remove_rule(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<RemoveReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "settings.write").await?;
    let reason = need_reason(r.reason.as_deref())?;
    let row = {
        let mut tx = tenant_tx(&st.pool, me.tenant_id).await?;
        sqlx::query("SELECT key, scope, value FROM feature_rules WHERE id = $1").bind(id).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?
    };
    let (key, scope, value): (String, String, String) = (row.get("key"), row.get("scope"), row.get("value"));
    Ok(Json(write_rule(&st, &ctx, &me, me.tenant_id, &key, &scope, &value, None, Some(&reason)).await?))
}

#[derive(Deserialize)]
pub struct PreviewQuery {
    country: Option<String>,
    groups: Option<String>,
    user_id: Option<i64>,
}

/// GET /v1/admin/settings/sections/preview?country=&groups=&user_id=: what a client would see. A client brings
/// their registered country (an explicit `country` wins); groups come from the caller (the Back Office reads the
/// client's accounts from the trading engine).
pub async fn preview(State(st): State<AppState>, ctx: Ctx, q: Result<Query<PreviewQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(|_| ApiError::BadRequest("Invalid query."))?;
    let me = require_key(&st, &ctx, "settings.read").await?;
    let mut who = Who { user_id: None, country: identity::clean_country(q.country.as_deref()), groups: clean_groups(q.groups.as_deref()) };
    let mut client = Value::Null;
    if let Some(uid) = q.user_id {
        let u = sqlx::query("SELECT id, email, first_name, last_name, country FROM users WHERE id = $1 AND tenant_id = $2").bind(uid).bind(me.tenant_id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
        who.user_id = Some(uid);
        if who.country.is_none() {
            who.country = identity::clean_country(u.get::<Option<String>, _>("country").as_deref());
        }
        client = json!({ "id": uid, "email": u.get::<String, _>("email"), "name": format!("{} {}", u.get::<String, _>("first_name"), u.get::<String, _>("last_name")) });
    }
    let e = effective(&st.pool, me.tenant_id, Some(&who)).await?;
    Ok(Json(json!({ "who": { "user_id": who.user_id, "country": who.country, "groups": who.groups, "client": client }, "modules": e.modules, "sections": e.sections, "sources": e.sources })))
}

#[cfg(test)]
mod tests;

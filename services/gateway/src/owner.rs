//! Platform Owner panel (D108, D110, D112, D122): /v1/owner/*
//!
//! Cross-tenant dashboard, tenants (create / edit / suspend / activate, domains, branding, limits, modules),
//! the feature-flag catalogue, tenant billing plans (setup fee + monthly licence + revenue share) and invoices.
//! Billing is tracking only: invoices are recorded and their status moved by hand, no payment is processed.
//! Every endpoint needs an `owner.*` permission, which only the Platform Owner role holds.

use axum::Json;
use axum::extract::rejection::{JsonRejection, QueryRejection};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;

use crate::admin::{Staff, require_key};
use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::error::{ApiError, ApiResult, field};
use crate::rbac;
use crate::state::{AppState, Ctx};
use crate::{staff_admin, tenancy, validate};

fn entry<'a>(me: &Staff, action: &'a str, tenant: i64, meta: Value) -> Entry<'a> {
    Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action, target: Some(("tenant", tenant)), meta }
}

fn conflict(code: &'static str, message: &'static str) -> ApiError {
    ApiError::Coded { status: StatusCode::CONFLICT, code, message }
}

/// Dollars (JSON number) → cents, 0 ..= 100M.
pub fn cents(v: f64) -> Result<i64, &'static str> {
    if !v.is_finite() || !(0.0..=100_000_000.0).contains(&v) {
        return Err("Enter an amount between 0 and 100,000,000.");
    }
    Ok((v * 100.0).round() as i64)
}

/// Percent (0–100, two decimals) → basis points.
pub fn bps(v: f64) -> Result<i32, &'static str> {
    if !v.is_finite() || !(0.0..=100.0).contains(&v) {
        return Err("Enter a percentage between 0 and 100.");
    }
    Ok((v * 100.0).round() as i32)
}

/// Revenue share of `base_cents` at `bps`, rounded to the cent.
pub fn share(base_cents: i64, bps: i32) -> i64 {
    ((base_cents as i128 * bps as i128 + 5_000) / 10_000) as i64
}

const RESERVED_SLUGS: &[&str] = &["admin", "api", "app", "www", "trade", "status", "owner", "platform", "demo", "static", "assets"];

pub fn clean_slug(raw: &str) -> Result<String, &'static str> {
    let s = raw.trim().to_lowercase();
    let ok = (3..=32).contains(&s.len())
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.ends_with('-')
        && !s.contains("--");
    if !ok {
        return Err("Use 3–32 lowercase letters, digits and dashes, starting with a letter.");
    }
    if RESERVED_SLUGS.contains(&s.as_str()) {
        return Err("That identifier is reserved.");
    }
    Ok(s)
}

pub fn clean_domain(raw: &str) -> Result<String, &'static str> {
    let d = raw.trim().trim_end_matches('.').to_lowercase();
    let d = d.trim_start_matches("https://").trim_start_matches("http://").trim_end_matches('/').to_string();
    let labels: Vec<&str> = d.split('.').collect();
    let ok = d.len() <= 253
        && labels.len() >= 2
        && labels.iter().all(|l| !l.is_empty() && l.len() <= 63 && !l.starts_with('-') && !l.ends_with('-') && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
        && labels.last().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()));
    if !ok {
        return Err("Enter domains like broker.com or app.broker.com.");
    }
    Ok(d)
}

/// Broker brand names: 2–80 characters, letters, digits and punctuation (unlike person names).
pub fn brand_name(raw: &str) -> Result<String, &'static str> {
    let n = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.chars().count() < 2 || n.chars().count() > 80 || n.chars().any(char::is_control) {
        return Err("Enter the broker's brand name (2–80 characters).");
    }
    Ok(n)
}

fn clean_color(raw: &str) -> Result<String, &'static str> {
    let c = raw.trim().to_lowercase();
    if c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit()) {
        Ok(c)
    } else {
        Err("Colours are hex values like #ff5a1f.")
    }
}

#[derive(Deserialize, Default)]
pub struct Brand {
    primary: Option<String>,
    accent: Option<String>,
    logo_url: Option<String>,
}

fn brand_json(b: &Brand) -> ApiResult<Value> {
    let mut v = Map::new();
    if let Some(p) = b.primary.as_deref().filter(|s| !s.trim().is_empty()) {
        v.insert("primary".into(), json!(clean_color(p).map_err(field("brand.primary"))?));
    }
    if let Some(a) = b.accent.as_deref().filter(|s| !s.trim().is_empty()) {
        v.insert("accent".into(), json!(clean_color(a).map_err(field("brand.accent"))?));
    }
    if let Some(l) = b.logo_url.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if !l.starts_with("https://") || l.len() > 500 || l.contains(char::is_whitespace) {
            return Err(ApiError::Validation { field: "brand.logo_url", message: "The logo must be an https:// URL." });
        }
        v.insert("logo_url".into(), json!(l));
    }
    Ok(Value::Object(v))
}

#[derive(Deserialize, Default)]
pub struct Limits {
    max_clients: Option<i64>,
    max_staff: Option<i64>,
    max_accounts: Option<i64>,
}

fn limits_json(l: &Limits) -> ApiResult<Value> {
    let mut v = Map::new();
    for (k, x) in [("max_clients", l.max_clients), ("max_staff", l.max_staff), ("max_accounts", l.max_accounts)] {
        if let Some(n) = x {
            if !(0..=10_000_000).contains(&n) {
                return Err(ApiError::Validation { field: "limits", message: "Limits are 0 (unlimited) to 10,000,000." });
            }
            v.insert(k.into(), json!(n));
        }
    }
    Ok(Value::Object(v))
}

async fn domains_free(st: &AppState, domains: &[String], except: Option<i64>) -> ApiResult<()> {
    if crate::domains::taken(&st.pool, domains, except).await?.is_some() {
        return Err(conflict("domain_taken", "One of these domains already belongs to another tenant."));
    }
    Ok(())
}

fn clean_domains(raw: &[String]) -> ApiResult<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for d in raw.iter().filter(|d| !d.trim().is_empty()) {
        let d = clean_domain(d).map_err(field("domains"))?;
        if !out.contains(&d) {
            out.push(d);
        }
    }
    if out.len() > 12 {
        return Err(ApiError::Validation { field: "domains", message: "Up to 12 domains per tenant." });
    }
    Ok(out)
}

// ---------- dashboard ----------

pub async fn dashboard(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let _me = require_key(&st, &ctx, "owner.system").await?;
    let idle_staff = crate::identity::policy(crate::identity::Kind::Staff).session_idle.num_seconds() as f64;
    let idle_user = crate::identity::policy(crate::identity::Kind::User).session_idle.num_seconds() as f64;
    let rows = sqlx::query(
        "SELECT t.id, t.slug, t.name, t.status, t.plan, t.created_at, t.maintenance_enabled AND (t.maintenance_until IS NULL OR t.maintenance_until > now()) AS maintenance,
            (SELECT count(*) FROM users u WHERE u.tenant_id = t.id) AS clients,
            (SELECT count(*) FROM users u WHERE u.tenant_id = t.id AND u.created_at > now() - interval '30 days') AS clients_30d,
            (SELECT count(*) FROM users u WHERE u.tenant_id = t.id AND u.kyc_status = 'verified') AS kyc_verified,
            (SELECT count(*) FROM staff s WHERE s.tenant_id = t.id AND s.status = 'active') AS staff,
            (SELECT count(*) FROM sessions se WHERE se.tenant_id = t.id AND se.revoked_at IS NULL AND se.expires_at > now()
                AND se.last_seen_at > now() - make_interval(secs => CASE WHEN se.subject_kind = 'staff' THEN $1::float8 ELSE $2::float8 END)) AS live_sessions,
            COALESCE(b.monthly_licence_cents, 0) AS licence_cents, COALESCE(b.revenue_share_bps, 0) AS share_bps, COALESCE(b.currency, 'USD') AS currency,
            (SELECT COALESCE(sum(total_cents), 0) FROM tenant_invoices i WHERE i.tenant_id = t.id AND i.status IN ('issued', 'overdue'))::bigint AS outstanding_cents,
            (SELECT count(*) FROM tenant_invoices i WHERE i.tenant_id = t.id AND i.status = 'overdue') AS overdue
         FROM tenants t LEFT JOIN tenant_billing b ON b.tenant_id = t.id ORDER BY t.id",
    )
    .bind(idle_staff)
    .bind(idle_user)
    .fetch_all(&st.pool)
    .await?;
    let n = |r: &PgRow, c: &str| r.get::<i64, _>(c);
    let tenants: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": n(r, "id"), "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("name"), "status": r.get::<String, _>("status"),
                "plan": r.get::<String, _>("plan"), "created_at": r.get::<DateTime<Utc>, _>("created_at"), "maintenance": r.get::<bool, _>("maintenance"),
                "clients": n(r, "clients"), "clients_30d": n(r, "clients_30d"), "kyc_verified": n(r, "kyc_verified"), "staff": n(r, "staff"),
                "live_sessions": n(r, "live_sessions"), "licence_cents": n(r, "licence_cents"), "revenue_share_bps": r.get::<i32, _>("share_bps"),
                "currency": r.get::<String, _>("currency"), "outstanding_cents": n(r, "outstanding_cents"), "overdue": n(r, "overdue"),
            })
        })
        .collect();
    let sum = |c: &str| rows.iter().map(|r| n(r, c)).sum::<i64>();
    let active = rows.iter().filter(|r| r.get::<String, _>("status") == "active").count();
    let mrr: i64 = rows.iter().filter(|r| r.get::<String, _>("status") == "active").map(|r| n(r, "licence_cents")).sum();
    let paid_30d: i64 = sqlx::query_scalar("SELECT COALESCE(sum(total_cents), 0)::bigint FROM tenant_invoices WHERE status = 'paid' AND paid_at > now() - interval '30 days'").fetch_one(&st.pool).await?;
    let series = sqlx::query(
        "SELECT d::date AS day, count(u.id) AS n FROM generate_series(current_date - 29, current_date, interval '1 day') d
         LEFT JOIN users u ON u.created_at::date = d::date GROUP BY d ORDER BY d",
    )
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| json!({ "day": r.get::<NaiveDate, _>("day"), "count": r.get::<i64, _>("n") }))
    .collect::<Vec<_>>();
    let activity = sqlx::query(
        "SELECT a.id, a.action, a.target_kind, a.target_id, a.meta, a.created_at, s.name AS actor FROM audit_log a
         LEFT JOIN staff s ON a.actor_kind = 'staff' AND s.id = a.actor_id
         WHERE a.action LIKE 'owner.%' ORDER BY a.id DESC LIMIT 12",
    )
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|r| json!({ "id": r.get::<i64, _>("id"), "action": r.get::<String, _>("action"), "target_id": r.get::<Option<i64>, _>("target_id"),
                     "meta": r.get::<sqlx::types::Json<Value>, _>("meta").0, "created_at": r.get::<DateTime<Utc>, _>("created_at"), "actor": r.get::<Option<String>, _>("actor") }))
    .collect::<Vec<_>>();
    Ok(Json(json!({
        "totals": {
            "tenants": rows.len(), "active": active, "suspended": rows.len() - active,
            "clients": sum("clients"), "clients_30d": sum("clients_30d"), "staff": sum("staff"), "live_sessions": sum("live_sessions"),
            "mrr_cents": mrr, "outstanding_cents": sum("outstanding_cents"), "overdue_invoices": sum("overdue"), "paid_30d_cents": paid_30d,
        },
        "tenants": tenants,
        "registrations": series,
        "activity": activity,
        "generated_at": Utc::now(),
    })))
}

// ---------- tenants ----------

pub async fn tenants(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let _me = require_key(&st, &ctx, "owner.tenants").await?;
    let rows = sqlx::query(
        "SELECT t.id, t.slug, t.name, t.legal_name, t.status, t.plan, t.domains, t.brand, t.limits, t.country, t.contact_email, t.created_at,
                t.suspended_at, t.suspended_reason,
                (SELECT count(*) FROM users u WHERE u.tenant_id = t.id) AS clients,
                (SELECT count(*) FROM staff s WHERE s.tenant_id = t.id AND s.status IN ('active', 'invited')) AS staff,
                (SELECT count(*) FROM tenant_features tf JOIN feature_flags f ON f.key = tf.key WHERE tf.tenant_id = t.id AND f.kind = 'module' AND NOT tf.enabled) AS modules_off,
                (SELECT count(*) FROM feature_flags WHERE kind = 'module') AS modules_total
         FROM tenants t ORDER BY t.id",
    )
    .fetch_all(&st.pool)
    .await?;
    let items = rows.iter().map(tenant_row).collect::<Vec<_>>();
    Ok(Json(json!({ "items": items, "total": items.len() })))
}

fn tenant_row(r: &PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("name"), "legal_name": r.get::<Option<String>, _>("legal_name"),
        "status": r.get::<String, _>("status"), "plan": r.get::<String, _>("plan"), "domains": r.get::<Vec<String>, _>("domains"),
        "brand": r.get::<sqlx::types::Json<Value>, _>("brand").0, "limits": r.get::<sqlx::types::Json<Value>, _>("limits").0,
        "country": r.get::<Option<String>, _>("country"), "contact_email": r.get::<Option<String>, _>("contact_email"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"), "suspended_at": r.get::<Option<DateTime<Utc>>, _>("suspended_at"),
        "suspended_reason": r.get::<Option<String>, _>("suspended_reason"), "clients": r.get::<i64, _>("clients"), "staff": r.get::<i64, _>("staff"),
        "modules_enabled": r.get::<i64, _>("modules_total") - r.get::<i64, _>("modules_off"), "modules_total": r.get::<i64, _>("modules_total"),
    })
}

#[derive(Deserialize, Default)]
pub struct AdminInvite {
    email: Option<String>,
    name: Option<String>,
}

#[derive(Deserialize, Default)]
pub struct BillingReq {
    currency: Option<String>,
    setup_fee: Option<f64>,
    monthly_licence: Option<f64>,
    revenue_share_pct: Option<f64>,
    billing_email: Option<String>,
    starts_on: Option<NaiveDate>,
    payment_terms_days: Option<i32>,
    notes: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateTenantReq {
    #[serde(default)]
    slug: String,
    #[serde(default)]
    name: String,
    legal_name: Option<String>,
    country: Option<String>,
    contact_email: Option<String>,
    #[serde(default)]
    domains: Vec<String>,
    #[serde(default)]
    brand: Brand,
    plan: Option<String>,
    #[serde(default)]
    limits: Limits,
    /// Module key → enabled (missing keys keep the platform default).
    #[serde(default)]
    modules: Map<String, Value>,
    #[serde(default)]
    billing: BillingReq,
    #[serde(default)]
    admin: AdminInvite,
}

fn clean_text(v: Option<&str>, max: usize) -> Option<String> {
    v.map(|s| s.trim().chars().take(max).collect::<String>()).filter(|s| !s.is_empty())
}

fn clean_plan(v: Option<&str>) -> ApiResult<Option<String>> {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(p) if ["starter", "growth", "enterprise", "standard"].contains(&p) => Ok(Some(p.to_string())),
        Some(_) => Err(ApiError::Validation { field: "plan", message: "Choose Starter, Growth or Enterprise." }),
    }
}

fn clean_country(v: Option<&str>) -> ApiResult<Option<String>> {
    match v.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(c) => Ok(Some(validate::country(c).map_err(field("country"))?)),
    }
}

pub async fn create_tenant(State(st): State<AppState>, ctx: Ctx, req: Result<Json<CreateTenantReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let slug = clean_slug(&r.slug).map_err(field("slug"))?;
    let name = brand_name(&r.name).map_err(field("name"))?;
    let domains = clean_domains(&r.domains)?;
    let brand = brand_json(&r.brand)?;
    let limits = limits_json(&r.limits)?;
    let plan = clean_plan(r.plan.as_deref())?.unwrap_or_else(|| "standard".into());
    let country = clean_country(r.country.as_deref())?;
    let contact = match r.contact_email.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(e) => Some(validate::email(e).map_err(field("contact_email"))?),
        None => None,
    };
    let admin = match r.admin.email.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(e) => Some((
            validate::email(e).map_err(field("admin.email"))?,
            validate::name(r.admin.name.as_deref().unwrap_or(""), "Enter the Super Admin's name.").map_err(field("admin.name"))?,
        )),
        None => None,
    };
    let modules = module_values(&st, &r.modules).await?;
    let billing = billing_values(&r.billing)?;
    domains_free(&st, &domains, None).await?;

    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO tenants (slug, name, legal_name, domains, brand, limits, plan, country, contact_email)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (slug) DO NOTHING RETURNING id",
    )
    .bind(&slug)
    .bind(&name)
    .bind(clean_text(r.legal_name.as_deref(), 120).unwrap_or_else(|| name.clone()))
    .bind(&domains)
    .bind(sqlx::types::Json(&brand))
    .bind(sqlx::types::Json(&limits))
    .bind(&plan)
    .bind(&country)
    .bind(&contact)
    .fetch_optional(&st.pool)
    .await?;
    let id = id.ok_or(conflict("slug_taken", "A tenant with this identifier already exists."))?;
    crate::domains::replace_all(&st.pool, id, &domains, Some(me.id)).await?;
    rbac::seed_tenant_roles(&st.pool, id).await?;
    for (k, v) in &modules {
        tenancy::write_feature(&st, &ctx, &me, id, k, Some(*v), Some("Set when the tenant was created")).await?;
    }
    save_billing(&st, id, &billing).await?;
    let mut invite = Value::Null;
    if let Some((email, admin_name)) = &admin {
        invite = invite_super_admin(&st, &ctx, &me, id, email, admin_name).await?;
    }
    audit::record(&st.pool, &ctx, entry(&me, "owner.tenant_created", id, json!({"slug": slug, "name": name, "domains": domains, "plan": plan, "modules": modules.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<Map<_, _>>(), "admin": admin.as_ref().map(|a| &a.0)}))).await;
    Ok((StatusCode::CREATED, Json(json!({ "tenant": { "id": id, "slug": slug, "name": name }, "admin_invite": invite }))))
}

async fn invite_super_admin(st: &AppState, ctx: &Ctx, me: &Staff, tenant_id: i64, email: &str, name: &str) -> ApiResult<Value> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM staff WHERE tenant_id = $1 AND email = $2").bind(tenant_id).bind(email).fetch_optional(&st.pool).await?;
    if exists.is_some() {
        return Err(conflict("staff_exists", "This person already has a staff account at this broker."));
    }
    let role_id: i64 = sqlx::query_scalar("SELECT id FROM roles WHERE tenant_id = $1 AND key = 'super_admin'").bind(tenant_id).fetch_one(&st.pool).await?;
    let sid: i64 = sqlx::query_scalar(
        "INSERT INTO staff (tenant_id, email, password_hash, name, role, role_id, status, invited_at) VALUES ($1,$2,$3,$4,'super_admin',$5,'invited', now()) RETURNING id",
    )
    .bind(tenant_id)
    .bind(email)
    .bind(staff_admin::NO_PASSWORD)
    .bind(name)
    .bind(role_id)
    .fetch_one(&st.pool)
    .await?;
    let (token, expires) = staff_admin::create_invite(st, tenant_id, me.id, sid, email, name, "Super Admin").await?;
    audit::record(&st.pool, ctx, Entry { tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "staff.invited", target: Some(("staff", sid)), meta: json!({"email": email, "role": "super_admin", "by": "platform_owner"}) }).await;
    let mut v = staff_admin::invite_json(st, &token, expires);
    v["staff_id"] = json!(sid);
    v["email"] = json!(email);
    Ok(v)
}

async fn module_values(st: &AppState, m: &Map<String, Value>) -> ApiResult<Vec<(String, bool)>> {
    let known: Vec<String> = sqlx::query_scalar("SELECT key FROM feature_flags").fetch_all(&st.pool).await?;
    let mut out = Vec::new();
    for (k, v) in m {
        if !known.contains(k) {
            return Err(ApiError::Validation { field: "modules", message: "Unknown module." });
        }
        out.push((k.clone(), v.as_bool().ok_or(ApiError::Validation { field: "modules", message: "Modules are on or off." })?));
    }
    Ok(out)
}

struct BillingValues {
    currency: String,
    setup_fee: i64,
    licence: i64,
    share_bps: i32,
    email: Option<String>,
    starts_on: Option<NaiveDate>,
    terms: i32,
    notes: String,
}

fn billing_values(b: &BillingReq) -> ApiResult<BillingValues> {
    let currency = b.currency.as_deref().map(|c| c.trim().to_uppercase()).filter(|c| !c.is_empty()).unwrap_or_else(|| "USD".into());
    if !["USD", "EUR", "GBP", "USDT", "AED", "INR"].contains(&currency.as_str()) {
        return Err(ApiError::Validation { field: "billing.currency", message: "Choose USD, EUR, GBP, USDT, AED or INR." });
    }
    let email = match b.billing_email.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(e) => Some(validate::email(e).map_err(field("billing.billing_email"))?),
        None => None,
    };
    let terms = b.payment_terms_days.unwrap_or(14);
    if !(0..=120).contains(&terms) {
        return Err(ApiError::Validation { field: "billing.payment_terms_days", message: "Payment terms are 0–120 days." });
    }
    Ok(BillingValues {
        currency,
        setup_fee: cents(b.setup_fee.unwrap_or(0.0)).map_err(field("billing.setup_fee"))?,
        licence: cents(b.monthly_licence.unwrap_or(0.0)).map_err(field("billing.monthly_licence"))?,
        share_bps: bps(b.revenue_share_pct.unwrap_or(0.0)).map_err(field("billing.revenue_share_pct"))?,
        email,
        starts_on: b.starts_on,
        terms,
        notes: clean_text(b.notes.as_deref(), 1000).unwrap_or_default(),
    })
}

async fn save_billing(st: &AppState, tenant_id: i64, b: &BillingValues) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO tenant_billing (tenant_id, currency, setup_fee_cents, monthly_licence_cents, revenue_share_bps, billing_email, starts_on, payment_terms_days, notes)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
         ON CONFLICT (tenant_id) DO UPDATE SET currency = EXCLUDED.currency, setup_fee_cents = EXCLUDED.setup_fee_cents,
             monthly_licence_cents = EXCLUDED.monthly_licence_cents, revenue_share_bps = EXCLUDED.revenue_share_bps, billing_email = EXCLUDED.billing_email,
             starts_on = EXCLUDED.starts_on, payment_terms_days = EXCLUDED.payment_terms_days, notes = EXCLUDED.notes, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(&b.currency)
    .bind(b.setup_fee)
    .bind(b.licence)
    .bind(b.share_bps)
    .bind(&b.email)
    .bind(b.starts_on)
    .bind(b.terms)
    .bind(&b.notes)
    .execute(&st.pool)
    .await?;
    Ok(())
}

fn billing_json(r: Option<&PgRow>) -> Value {
    match r {
        None => json!({ "currency": "USD", "setup_fee_cents": 0, "monthly_licence_cents": 0, "revenue_share_bps": 0, "billing_email": null, "starts_on": null, "payment_terms_days": 14, "notes": "", "configured": false }),
        Some(b) => json!({
            "currency": b.get::<String, _>("currency"), "setup_fee_cents": b.get::<i64, _>("setup_fee_cents"),
            "monthly_licence_cents": b.get::<i64, _>("monthly_licence_cents"), "revenue_share_bps": b.get::<i32, _>("revenue_share_bps"),
            "billing_email": b.get::<Option<String>, _>("billing_email"), "starts_on": b.get::<Option<NaiveDate>, _>("starts_on"),
            "payment_terms_days": b.get::<i32, _>("payment_terms_days"), "notes": b.get::<String, _>("notes"), "configured": true,
        }),
    }
}

pub async fn tenant_detail(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let _me = require_key(&st, &ctx, "owner.tenants").await?;
    let r = sqlx::query(
        "SELECT t.id, t.slug, t.name, t.legal_name, t.status, t.plan, t.domains, t.brand, t.limits, t.country, t.contact_email, t.created_at,
                t.suspended_at, t.suspended_reason, t.maintenance_enabled, t.ip_allowlist_enabled,
                (SELECT count(*) FROM users u WHERE u.tenant_id = t.id) AS clients,
                (SELECT count(*) FROM users u WHERE u.tenant_id = t.id AND u.created_at > now() - interval '30 days') AS clients_30d,
                (SELECT count(*) FROM users u WHERE u.tenant_id = t.id AND u.kyc_status = 'verified') AS kyc_verified,
                (SELECT count(*) FROM staff s WHERE s.tenant_id = t.id AND s.status IN ('active', 'invited')) AS staff,
                (SELECT count(*) FROM tenant_features tf JOIN feature_flags f ON f.key = tf.key WHERE tf.tenant_id = t.id AND f.kind = 'module' AND NOT tf.enabled) AS modules_off,
                (SELECT count(*) FROM feature_flags WHERE kind = 'module') AS modules_total
         FROM tenants t WHERE t.id = $1",
    )
    .bind(id)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let mut tenant = tenant_row(&r);
    tenant["clients_30d"] = json!(r.get::<i64, _>("clients_30d"));
    tenant["kyc_verified"] = json!(r.get::<i64, _>("kyc_verified"));
    tenant["maintenance"] = json!(r.get::<bool, _>("maintenance_enabled"));
    tenant["ip_allowlist"] = json!(r.get::<bool, _>("ip_allowlist_enabled"));
    tenant["is_owner_tenant"] = json!(id == rbac::owner_tenant_id(&st.pool).await?);
    tenant["domain_records"] = json!(crate::domains::records(&st.pool, id).await?);
    let features = tenancy::features(&st.pool, id).await?;
    let billing = sqlx::query("SELECT * FROM tenant_billing WHERE tenant_id = $1").bind(id).fetch_optional(&st.pool).await?;
    let admins = sqlx::query(
        "SELECT s.id, s.email, s.name, s.status, s.last_login_at FROM staff s JOIN roles r ON r.id = s.role_id
         WHERE s.tenant_id = $1 AND r.key IN ('super_admin', 'platform_owner') ORDER BY s.id",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?
    .iter()
    .map(|x| json!({ "id": x.get::<i64, _>("id"), "email": x.get::<String, _>("email"), "name": x.get::<String, _>("name"), "status": x.get::<String, _>("status"), "last_login_at": x.get::<Option<DateTime<Utc>>, _>("last_login_at") }))
    .collect::<Vec<_>>();
    let invoices = invoice_rows(&st, Some(id), None, 24).await?;
    Ok(Json(json!({
        "tenant": tenant,
        "modules": features.iter().filter(|f| f.kind == "module").map(|f| json!({"key": f.key, "name": f.name, "description": f.description, "enabled": f.enabled, "overridden": f.overridden, "default": f.default_enabled})).collect::<Vec<_>>(),
        "flags": features.iter().filter(|f| f.kind == "flag").map(|f| json!({"key": f.key, "name": f.name, "enabled": f.enabled, "overridden": f.overridden})).collect::<Vec<_>>(),
        "billing": billing_json(billing.as_ref()),
        "admins": admins,
        "invoices": invoices,
    })))
}

#[derive(Deserialize)]
pub struct UpdateTenantReq {
    name: Option<String>,
    legal_name: Option<String>,
    country: Option<String>,
    contact_email: Option<String>,
    domains: Option<Vec<String>>,
    brand: Option<Brand>,
    plan: Option<String>,
    limits: Option<Limits>,
}

pub async fn update_tenant(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<UpdateTenantReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let before = sqlx::query("SELECT name, legal_name, domains, brand, plan, limits, country, contact_email FROM tenants WHERE id = $1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let name = match r.name.as_deref() {
        Some(n) => Some(brand_name(n).map_err(field("name"))?),
        None => None,
    };
    let domains = match &r.domains {
        Some(d) => {
            let d = clean_domains(d)?;
            domains_free(&st, &d, Some(id)).await?;
            Some(d)
        }
        None => None,
    };
    let brand = r.brand.as_ref().map(brand_json).transpose()?;
    let limits = r.limits.as_ref().map(limits_json).transpose()?;
    let plan = clean_plan(r.plan.as_deref())?;
    let country = clean_country(r.country.as_deref())?;
    let contact = match r.contact_email.as_deref().map(str::trim) {
        Some("") => Some(String::new()),
        Some(e) => Some(validate::email(e).map_err(field("contact_email"))?),
        None => None,
    };
    sqlx::query(
        "UPDATE tenants SET name = COALESCE($2, name), legal_name = COALESCE($3, legal_name), domains = COALESCE($4, domains),
                brand = COALESCE($5, brand), limits = COALESCE($6, limits), plan = COALESCE($7, plan), country = COALESCE($8, country),
                contact_email = CASE WHEN $9::text IS NULL THEN contact_email WHEN $9 = '' THEN NULL ELSE $9 END, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&name)
    .bind(clean_text(r.legal_name.as_deref(), 120))
    .bind(&domains)
    .bind(brand.as_ref().map(sqlx::types::Json))
    .bind(limits.as_ref().map(sqlx::types::Json))
    .bind(&plan)
    .bind(&country)
    .bind(&contact)
    .execute(&st.pool)
    .await?;
    if let Some(d) = &domains {
        crate::domains::replace_all(&st.pool, id, d, Some(me.id)).await?;
    }
    audit::record(&st.pool, &ctx, entry(&me, "owner.tenant_updated", id, json!({
        "before": { "name": before.get::<String, _>("name"), "domains": before.get::<Vec<String>, _>("domains"), "brand": before.get::<sqlx::types::Json<Value>, _>("brand").0,
                    "plan": before.get::<String, _>("plan"), "limits": before.get::<sqlx::types::Json<Value>, _>("limits").0 },
        "after": { "name": name, "domains": domains, "brand": brand, "plan": plan, "limits": limits },
    })))
    .await;
    Ok(Json(json!({ "status": "ok" })))
}

#[derive(Deserialize, Default)]
pub struct SuspendReq {
    reason: Option<String>,
}

pub async fn suspend_tenant(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<SuspendReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let reason = req.ok().and_then(|Json(r)| clean_text(r.reason.as_deref(), 300));
    if reason.as_deref().is_none_or(|r| r.chars().count() < 3) {
        return Err(ApiError::Validation { field: "reason", message: "Give a reason (it is shown in the audit log)." });
    }
    if id == rbac::owner_tenant_id(&st.pool).await? {
        return Err(ApiError::BadRequest("The platform owner's own workspace can't be suspended."));
    }
    let n = sqlx::query("UPDATE tenants SET status = 'suspended', suspended_at = now(), suspended_reason = $2, updated_at = now() WHERE id = $1 AND status = 'active'")
        .bind(id)
        .bind(&reason)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM tenants WHERE id = $1").bind(id).fetch_optional(&st.pool).await?;
        return if exists.is_some() { Ok(Json(json!({ "status": "ok", "already": true }))) } else { Err(ApiError::NotFound) };
    }
    let sessions = sqlx::query("UPDATE sessions SET revoked_at = now() WHERE tenant_id = $1 AND revoked_at IS NULL").bind(id).execute(&st.pool).await?.rows_affected();
    audit::record(&st.pool, &ctx, entry(&me, "owner.tenant_suspended", id, json!({"reason": reason, "sessions_revoked": sessions, "before": {"status": "active"}, "after": {"status": "suspended"}}))).await;
    Ok(Json(json!({ "status": "ok", "sessions_revoked": sessions })))
}

pub async fn activate_tenant(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let n = sqlx::query("UPDATE tenants SET status = 'active', suspended_at = NULL, suspended_reason = NULL, updated_at = now() WHERE id = $1 AND status = 'suspended'")
        .bind(id)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n > 0 {
        audit::record(&st.pool, &ctx, entry(&me, "owner.tenant_activated", id, json!({"before": {"status": "suspended"}, "after": {"status": "active"}}))).await;
    }
    Ok(Json(json!({ "status": "ok", "changed": n > 0 })))
}

pub async fn set_tenant_features(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<Map<String, Value>>, JsonRejection>) -> ApiResult<Json<Value>> {
    let mut r = body(req)?;
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let _: i64 = sqlx::query_scalar("SELECT id FROM tenants WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    // `reason` rides along with the switches (audited with each of them)
    let reason = match r.remove("reason") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => tenancy::clean_reason(Some(&s)),
        Some(_) => return Err(ApiError::Validation { field: "reason", message: "The reason must be text." }),
    };
    let known: Vec<String> = sqlx::query_scalar("SELECT key FROM feature_flags").fetch_all(&st.pool).await?;
    let mut changes = Vec::new();
    for (k, v) in &r {
        if !known.contains(k) {
            return Err(ApiError::Validation { field: "key", message: "Unknown module or flag." });
        }
        let value = match v {
            Value::Bool(b) => Some(*b),
            Value::Null => None,
            _ => return Err(ApiError::Validation { field: "enabled", message: "Use true, false or null (platform default)." }),
        };
        changes.push((k.clone(), value));
    }
    let mut out = Vec::new();
    for (k, v) in changes {
        out.push(tenancy::write_feature(&st, &ctx, &me, id, &k, v, reason.as_deref()).await?);
    }
    Ok(Json(json!({ "items": out })))
}

#[derive(Deserialize)]
pub struct InviteAdminReq {
    #[serde(default)]
    email: String,
    #[serde(default)]
    name: String,
}

pub async fn invite_admin(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<InviteAdminReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let status: String = sqlx::query_scalar("SELECT status FROM tenants WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if status != "active" {
        return Err(ApiError::BadRequest("Activate the tenant before inviting staff."));
    }
    let email = validate::email(&r.email).map_err(field("email"))?;
    let name = validate::name(&r.name, "Enter the Super Admin's name.").map_err(field("name"))?;
    let v = invite_super_admin(&st, &ctx, &me, id, &email, &name).await?;
    audit::record(&st.pool, &ctx, entry(&me, "owner.admin_invited", id, json!({"email": email}))).await;
    Ok((StatusCode::CREATED, Json(json!({ "invite": v }))))
}

// ---------- feature catalogue ----------

pub async fn feature_catalogue(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let _me = require_key(&st, &ctx, "owner.tenants").await?;
    let flags = sqlx::query("SELECT key, kind, name, description, default_enabled, builtin, created_at FROM feature_flags ORDER BY kind DESC, builtin DESC, created_at, key")
        .fetch_all(&st.pool)
        .await?;
    let tenants = sqlx::query("SELECT id, slug, name, status FROM tenants ORDER BY id").fetch_all(&st.pool).await?;
    let overrides = sqlx::query("SELECT tenant_id, key, enabled FROM tenant_features").fetch_all(&st.pool).await?;
    let mut matrix = Map::new();
    for t in &tenants {
        let tid: i64 = t.get("id");
        let mut row = Map::new();
        for f in &flags {
            let key: String = f.get("key");
            let over = overrides.iter().find(|o| o.get::<i64, _>("tenant_id") == tid && o.get::<String, _>("key") == key).map(|o| o.get::<bool, _>("enabled"));
            row.insert(key, json!({ "enabled": over.unwrap_or(f.get("default_enabled")), "overridden": over.is_some() }));
        }
        matrix.insert(tid.to_string(), Value::Object(row));
    }
    Ok(Json(json!({
        "features": flags.iter().map(|f| {
            let key: String = f.get("key");
            let on = tenants.iter().filter(|t| matrix[&t.get::<i64, _>("id").to_string()][&key]["enabled"] == json!(true)).count();
            json!({ "key": key, "kind": f.get::<String, _>("kind"), "name": f.get::<String, _>("name"), "description": f.get::<String, _>("description"),
                    "default": f.get::<bool, _>("default_enabled"), "builtin": f.get::<bool, _>("builtin"), "created_at": f.get::<DateTime<Utc>, _>("created_at"),
                    "tenants_on": on })
        }).collect::<Vec<_>>(),
        "tenants": tenants.iter().map(|t| json!({ "id": t.get::<i64, _>("id"), "slug": t.get::<String, _>("slug"), "name": t.get::<String, _>("name"), "status": t.get::<String, _>("status") })).collect::<Vec<_>>(),
        "matrix": matrix,
    })))
}

#[derive(Deserialize)]
pub struct FlagReq {
    key: Option<String>,
    name: Option<String>,
    description: Option<String>,
    default_enabled: Option<bool>,
}

pub async fn create_flag(State(st): State<AppState>, ctx: Ctx, req: Result<Json<FlagReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let key = r.key.unwrap_or_default().trim().to_lowercase();
    if !(2..=48).contains(&key.len()) || !key.starts_with(|c: char| c.is_ascii_lowercase()) || !key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err(ApiError::Validation { field: "key", message: "Keys are 2–48 lowercase letters, digits and underscores, e.g. new_chart." });
    }
    let name = clean_text(r.name.as_deref(), 80).ok_or(ApiError::Validation { field: "name", message: "Give the flag a name." })?;
    let description = clean_text(r.description.as_deref(), 300).unwrap_or_default();
    let default = r.default_enabled.unwrap_or(false);
    let n = sqlx::query("INSERT INTO feature_flags (key, kind, name, description, default_enabled) VALUES ($1,'flag',$2,$3,$4) ON CONFLICT DO NOTHING")
        .bind(&key)
        .bind(&name)
        .bind(&description)
        .bind(default)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(conflict("duplicate", "A flag or module with this key already exists."));
    }
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "owner.flag_created", target: None, meta: json!({"key": key, "name": name, "default": default}) }).await;
    Ok((StatusCode::CREATED, Json(json!({ "key": key }))))
}

pub async fn update_flag(State(st): State<AppState>, ctx: Ctx, Path(key): Path<String>, req: Result<Json<FlagReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let before = sqlx::query("SELECT name, description, default_enabled, builtin FROM feature_flags WHERE key = $1").bind(&key).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let builtin: bool = before.get("builtin");
    // built-in names and descriptions come from the gateway code; only their platform default can change here
    let name = if builtin { None } else { clean_text(r.name.as_deref(), 80) };
    let description = if builtin { None } else { r.description.as_deref().map(|d| d.trim().chars().take(300).collect::<String>()) };
    if builtin && r.default_enabled.is_some() {
        return Err(ApiError::BadRequest("Built-in modules and flags keep their default. Switch them per tenant instead."));
    }
    sqlx::query("UPDATE feature_flags SET name = COALESCE($2, name), description = COALESCE($3, description), default_enabled = COALESCE($4, default_enabled) WHERE key = $1")
        .bind(&key)
        .bind(&name)
        .bind(&description)
        .bind(r.default_enabled)
        .execute(&st.pool)
        .await?;
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "owner.flag_updated", target: None, meta: json!({
        "key": key, "before": {"name": before.get::<String, _>("name"), "default": before.get::<bool, _>("default_enabled")}, "after": {"name": name, "default": r.default_enabled}
    }) }).await;
    Ok(Json(json!({ "status": "ok" })))
}

pub async fn delete_flag(State(st): State<AppState>, ctx: Ctx, Path(key): Path<String>) -> ApiResult<Json<Value>> {
    let me = require_key(&st, &ctx, "owner.tenants").await?;
    let n = sqlx::query("DELETE FROM feature_flags WHERE key = $1 AND NOT builtin").bind(&key).execute(&st.pool).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::BadRequest("Only flags you created can be deleted."));
    }
    audit::record(&st.pool, &ctx, Entry { tenant_id: me.tenant_id, actor_kind: "staff", actor_id: Some(me.id), action: "owner.flag_deleted", target: None, meta: json!({"key": key}) }).await;
    Ok(Json(json!({ "status": "ok" })))
}

// ---------- billing + invoices ----------

pub async fn billing_overview(State(st): State<AppState>, ctx: Ctx) -> ApiResult<Json<Value>> {
    let _me = require_key(&st, &ctx, "owner.billing").await?;
    let rows = sqlx::query(
        "SELECT t.id, t.slug, t.name, t.status, b.*,
            (SELECT COALESCE(sum(total_cents), 0) FROM tenant_invoices i WHERE i.tenant_id = t.id AND i.status IN ('issued', 'overdue'))::bigint AS outstanding_cents,
            (SELECT COALESCE(sum(total_cents), 0) FROM tenant_invoices i WHERE i.tenant_id = t.id AND i.status = 'paid')::bigint AS paid_cents,
            (SELECT max(period_end) FROM tenant_invoices i WHERE i.tenant_id = t.id AND i.status <> 'void') AS last_period_end,
            EXISTS (SELECT 1 FROM tenant_invoices i WHERE i.tenant_id = t.id AND i.setup_fee_cents > 0 AND i.status <> 'void') AS setup_invoiced
         FROM tenants t LEFT JOIN tenant_billing b ON b.tenant_id = t.id ORDER BY t.id",
    )
    .fetch_all(&st.pool)
    .await?;
    let items = rows
        .iter()
        .map(|r| {
            let configured = r.get::<Option<String>, _>("currency").is_some();
            json!({
                "tenant": { "id": r.get::<i64, _>("id"), "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("name"), "status": r.get::<String, _>("status") },
                "billing": billing_json(configured.then_some(r)),
                "outstanding_cents": r.get::<i64, _>("outstanding_cents"),
                "paid_cents": r.get::<i64, _>("paid_cents"),
                "last_period_end": r.get::<Option<NaiveDate>, _>("last_period_end"),
                "setup_invoiced": r.get::<bool, _>("setup_invoiced"),
            })
        })
        .collect::<Vec<_>>();
    let mrr: i64 = rows.iter().filter(|r| r.get::<String, _>("status") == "active").map(|r| r.get::<Option<i64>, _>("monthly_licence_cents").unwrap_or(0)).sum();
    let outstanding: i64 = rows.iter().map(|r| r.get::<i64, _>("outstanding_cents")).sum();
    let paid: i64 = rows.iter().map(|r| r.get::<i64, _>("paid_cents")).sum();
    let invoices = invoice_rows(&st, None, None, 100).await?;
    Ok(Json(json!({ "items": items, "totals": { "mrr_cents": mrr, "outstanding_cents": outstanding, "paid_cents": paid }, "invoices": invoices })))
}

pub async fn set_billing(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<BillingReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.billing").await?;
    let _: i64 = sqlx::query_scalar("SELECT id FROM tenants WHERE id = $1").bind(id).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = sqlx::query("SELECT * FROM tenant_billing WHERE tenant_id = $1").bind(id).fetch_optional(&st.pool).await?;
    let v = billing_values(&r)?;
    save_billing(&st, id, &v).await?;
    let after = sqlx::query("SELECT * FROM tenant_billing WHERE tenant_id = $1").bind(id).fetch_one(&st.pool).await?;
    let after = billing_json(Some(&after));
    audit::record(&st.pool, &ctx, entry(&me, "owner.billing_updated", id, json!({"before": billing_json(before.as_ref()), "after": after}))).await;
    Ok(Json(json!({ "billing": after })))
}

#[derive(Deserialize, Default)]
pub struct InvoiceQuery {
    tenant_id: Option<i64>,
    status: Option<String>,
}

async fn invoice_rows(st: &AppState, tenant: Option<i64>, status: Option<&str>, limit: i64) -> ApiResult<Vec<Value>> {
    let rows = sqlx::query(
        "SELECT i.*, t.slug, t.name AS tenant_name FROM tenant_invoices i JOIN tenants t ON t.id = i.tenant_id
         WHERE ($1::bigint IS NULL OR i.tenant_id = $1) AND ($2::text IS NULL OR i.status = $2)
         ORDER BY i.period_start DESC, i.id DESC LIMIT $3",
    )
    .bind(tenant)
    .bind(status)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    Ok(rows.iter().map(invoice_json).collect())
}

fn invoice_json(r: &PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "number": r.get::<String, _>("number"),
        "tenant": { "id": r.get::<i64, _>("tenant_id"), "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("tenant_name") },
        "period_start": r.get::<NaiveDate, _>("period_start"), "period_end": r.get::<NaiveDate, _>("period_end"), "currency": r.get::<String, _>("currency"),
        "setup_fee_cents": r.get::<i64, _>("setup_fee_cents"), "licence_cents": r.get::<i64, _>("licence_cents"),
        "revenue_base_cents": r.get::<i64, _>("revenue_base_cents"), "revenue_share_bps": r.get::<i32, _>("revenue_share_bps"),
        "revenue_share_cents": r.get::<i64, _>("revenue_share_cents"), "adjustment_cents": r.get::<i64, _>("adjustment_cents"),
        "total_cents": r.get::<i64, _>("total_cents"), "status": r.get::<String, _>("status"), "issued_at": r.get::<Option<DateTime<Utc>>, _>("issued_at"),
        "due_at": r.get::<Option<NaiveDate>, _>("due_at"), "paid_at": r.get::<Option<DateTime<Utc>>, _>("paid_at"), "notes": r.get::<String, _>("notes"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
    })
}

pub async fn invoices(State(st): State<AppState>, ctx: Ctx, q: Result<Query<InvoiceQuery>, QueryRejection>) -> ApiResult<Json<Value>> {
    let Query(q) = q.map_err(|_| ApiError::BadRequest("Invalid query parameters."))?;
    let _me = require_key(&st, &ctx, "owner.billing").await?;
    let status = q.status.as_deref().filter(|s| !s.is_empty() && *s != "all");
    if let Some(s) = status
        && !["draft", "issued", "paid", "overdue", "void"].contains(&s)
    {
        return Err(ApiError::BadRequest("Unknown invoice status."));
    }
    let items = invoice_rows(&st, q.tenant_id, status, 500).await?;
    Ok(Json(json!({ "items": items, "total": items.len() })))
}

#[derive(Deserialize)]
pub struct CreateInvoiceReq {
    tenant_id: i64,
    period_start: NaiveDate,
    period_end: NaiveDate,
    #[serde(default)]
    include_setup_fee: bool,
    /// Broker revenue for the period the share is computed on (currency units).
    revenue_base: Option<f64>,
    /// Signed adjustment (credit < 0) in currency units.
    adjustment: Option<f64>,
    notes: Option<String>,
    #[serde(default)]
    issue: bool,
}

pub async fn create_invoice(State(st): State<AppState>, ctx: Ctx, req: Result<Json<CreateInvoiceReq>, JsonRejection>) -> ApiResult<(StatusCode, Json<Value>)> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.billing").await?;
    if r.period_end < r.period_start || (r.period_end - r.period_start).num_days() > 366 {
        return Err(ApiError::Validation { field: "period_end", message: "The period must end after it starts (at most a year)." });
    }
    let b = sqlx::query("SELECT * FROM tenant_billing WHERE tenant_id = $1")
        .bind(r.tenant_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::Validation { field: "tenant_id", message: "Set up this tenant's billing plan first." })?;
    let setup = if r.include_setup_fee { b.get::<i64, _>("setup_fee_cents") } else { 0 };
    // licence pro-rated by whole months in the period (at least one)
    let months = (((r.period_end - r.period_start).num_days() + 1) as f64 / 30.44).round().max(1.0) as i64;
    let licence = b.get::<i64, _>("monthly_licence_cents") * months;
    let base = cents(r.revenue_base.unwrap_or(0.0)).map_err(field("revenue_base"))?;
    let share_bps: i32 = b.get("revenue_share_bps");
    let rev = share(base, share_bps);
    let adj = match r.adjustment {
        Some(a) if a.is_finite() && a.abs() <= 100_000_000.0 => (a * 100.0).round() as i64,
        Some(_) => return Err(ApiError::Validation { field: "adjustment", message: "Invalid adjustment." }),
        None => 0,
    };
    let total = setup + licence + rev + adj;
    if total < 0 {
        return Err(ApiError::Validation { field: "adjustment", message: "The invoice total can't be negative." });
    }
    let terms: i32 = b.get("payment_terms_days");
    let status = if r.issue { "issued" } else { "draft" };
    let mut id = None;
    for _ in 0..3 {
        let seq: i64 = sqlx::query_scalar("SELECT count(*) + 1 FROM tenant_invoices").fetch_one(&st.pool).await?;
        let number = format!("INV-{}-{:05}", r.period_start.format("%Y%m"), seq);
        id = sqlx::query_scalar(
            "INSERT INTO tenant_invoices (tenant_id, number, period_start, period_end, currency, setup_fee_cents, licence_cents, revenue_base_cents,
                 revenue_share_bps, revenue_share_cents, adjustment_cents, total_cents, status, issued_at, due_at, notes, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13, CASE WHEN $13 = 'issued' THEN now() END,
                     CASE WHEN $13 = 'issued' THEN current_date + $14::int END, $15, $16)
             ON CONFLICT (number) DO NOTHING RETURNING id",
        )
        .bind(r.tenant_id)
        .bind(&number)
        .bind(r.period_start)
        .bind(r.period_end)
        .bind(b.get::<String, _>("currency"))
        .bind(setup)
        .bind(licence)
        .bind(base)
        .bind(share_bps)
        .bind(rev)
        .bind(adj)
        .bind(total)
        .bind(status)
        .bind(terms)
        .bind(clean_text(r.notes.as_deref(), 1000).unwrap_or_default())
        .bind(me.id)
        .fetch_optional(&st.pool)
        .await?;
        if id.is_some() {
            break;
        }
    }
    let id: i64 = id.ok_or_else(|| anyhow::anyhow!("could not allocate an invoice number"))?;
    let row = sqlx::query("SELECT i.*, t.slug, t.name AS tenant_name FROM tenant_invoices i JOIN tenants t ON t.id = i.tenant_id WHERE i.id = $1").bind(id).fetch_one(&st.pool).await?;
    let v = invoice_json(&row);
    audit::record(&st.pool, &ctx, entry(&me, "owner.invoice_created", r.tenant_id, json!({"invoice_id": id, "number": v["number"], "total_cents": total, "status": status}))).await;
    Ok((StatusCode::CREATED, Json(json!({ "invoice": v }))))
}

/// Allowed status moves (paid and void are final).
pub fn transition_ok(from: &str, to: &str) -> bool {
    matches!((from, to), ("draft", "issued") | ("draft", "void") | ("issued", "paid") | ("issued", "overdue") | ("issued", "void") | ("overdue", "paid") | ("overdue", "void"))
}

#[derive(Deserialize)]
pub struct InvoiceStatusReq {
    #[serde(default)]
    status: String,
    note: Option<String>,
}

pub async fn invoice_status(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<InvoiceStatusReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "owner.billing").await?;
    let row = sqlx::query("SELECT i.status, i.tenant_id, b.payment_terms_days FROM tenant_invoices i LEFT JOIN tenant_billing b ON b.tenant_id = i.tenant_id WHERE i.id = $1")
        .bind(id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    let from: String = row.get("status");
    if !transition_ok(&from, &r.status) {
        return Err(ApiError::BadRequest("That status change isn't allowed."));
    }
    let terms = row.get::<Option<i32>, _>("payment_terms_days").unwrap_or(14);
    sqlx::query(
        "UPDATE tenant_invoices SET status = $2, updated_at = now(),
            issued_at = CASE WHEN $2 = 'issued' THEN now() ELSE issued_at END,
            due_at = CASE WHEN $2 = 'issued' THEN current_date + $3::int ELSE due_at END,
            paid_at = CASE WHEN $2 = 'paid' THEN now() ELSE paid_at END,
            notes = CASE WHEN $4::text IS NULL THEN notes ELSE trim(both from notes || E'\\n' || $4) END
         WHERE id = $1",
    )
    .bind(id)
    .bind(&r.status)
    .bind(terms)
    .bind(clean_text(r.note.as_deref(), 300))
    .execute(&st.pool)
    .await?;
    audit::record(&st.pool, &ctx, entry(&me, "owner.invoice_status", row.get("tenant_id"), json!({"invoice_id": id, "before": {"status": from}, "after": {"status": r.status}, "note": r.note}))).await;
    Ok(Json(json!({ "status": "ok" })))
}

/// Marks issued invoices past their due date as overdue (runs with the gateway's housekeeping loop).
pub async fn mark_overdue(pool: &sqlx::PgPool) -> anyhow::Result<u64> {
    Ok(sqlx::query("UPDATE tenant_invoices SET status = 'overdue', updated_at = now() WHERE status = 'issued' AND due_at < current_date").execute(pool).await?.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_and_rules() {
        assert_eq!(cents(1500.5).unwrap(), 150050);
        assert!(cents(-1.0).is_err());
        assert!(cents(f64::NAN).is_err());
        assert_eq!(bps(12.5).unwrap(), 1250);
        assert!(bps(101.0).is_err());
        assert_eq!(share(1_000_000, 1250), 125_000);
        assert_eq!(share(333, 3333), 111);
        assert!(transition_ok("draft", "issued") && transition_ok("overdue", "paid"));
        assert!(!transition_ok("paid", "void") && !transition_ok("void", "issued") && !transition_ok("draft", "paid"));
    }

    #[test]
    fn slugs_and_domains() {
        assert_eq!(clean_slug(" Acme-FX ").unwrap(), "acme-fx");
        assert!(clean_slug("ab").is_err());
        assert!(clean_slug("admin").is_err());
        assert!(clean_slug("1abc").is_err());
        assert!(clean_slug("a--b").is_err());
        assert_eq!(clean_domain("https://App.Acme.com/").unwrap(), "app.acme.com");
        assert!(clean_domain("localhost").is_err());
        assert!(clean_domain("bad_domain.com").is_err());
        assert!(clean_color("#FF5A1F").is_ok() && clean_color("red").is_err());
        assert_eq!(brand_name("  Acme  FX 24 ").unwrap(), "Acme FX 24");
        assert!(brand_name("A").is_err());
    }
}

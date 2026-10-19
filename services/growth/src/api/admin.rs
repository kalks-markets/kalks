//! Back Office routes (`/v1/growth/admin/*`). Reads need a marketing read role, configuration changes
//! `ROLES_WRITE`, money out `ROLES_APPROVE`. Every change is audited with before / after.
//!
//! Updates are PATCH-by-merge: the current row's JSON view is merged with the body and the result is validated
//! and written as a whole, so a partial body can never leave a row in a state `POST` would have refused.

use super::{ROLES_APPROVE, ROLES_READ, ROLES_WRITE, StaffCtx, paging, parse_time};
use crate::audit;
use crate::bonus;
use crate::calc::{self, Segment};
use crate::cashback;
use super::client::{BANNER_LIVE, banner_matches, banner_view};
use crate::contests::{self, CONTEST_SELECT};
use crate::db;
use crate::error::{ApiError, ApiResult, invalid};
use crate::loyalty;
use crate::model::{ASSET_CLASSES, AntiCheat, Prize, Settings, Tier, validate_prizes, validate_tiers};
use crate::money::{D, ZERO, de_dec, de_opt_dec, num};
use crate::payouts;
use crate::promos;
use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sqlx::Row;

pub(crate) fn merge(mut base: Value, patch: &Value) -> Value {
    if let (Some(b), Some(p)) = (base.as_object_mut(), patch.as_object()) {
        for (k, v) in p {
            b.insert(k.clone(), v.clone());
        }
    }
    base
}

pub(crate) fn parse<T: DeserializeOwned>(v: Value) -> ApiResult<T> {
    serde_json::from_value(v).map_err(|e| ApiError::BadRequest(format!("Invalid request: {e}")))
}

fn time_req(v: &Option<String>, field: &'static str) -> ApiResult<Option<DateTime<Utc>>> {
    parse_time(v).map_err(|_| invalid(field, "Use YYYY-MM-DD or an RFC 3339 time."))
}

pub(crate) fn clean_list(v: &[String], upper: bool) -> Vec<String> {
    let mut out: Vec<String> = v.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| if upper { s.to_uppercase() } else { s.to_lowercase() }).collect();
    out.sort();
    out.dedup();
    out
}

pub(crate) fn text(v: &str, field: &'static str, max: usize, required: bool) -> ApiResult<String> {
    let t = v.trim().to_string();
    if required && t.is_empty() {
        return Err(invalid(field, "Required."));
    }
    if t.chars().count() > max {
        return Err(invalid(field, format!("At most {max} characters.")));
    }
    Ok(t)
}

// ---------------------------------------------------------------- overview

pub async fn overview(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let t = &s.tenant;
    let settings = db::settings(&st.pool, t).await?;
    let l = sqlx::query(
        "SELECT count(DISTINCT user_id) AS members,
                COALESCE(sum(points) FILTER (WHERE points > 0 AND created_at > now() - interval '30 days'), 0)::bigint AS issued,
                COALESCE(-sum(points) FILTER (WHERE kind = 'redeem' AND created_at > now() - interval '30 days'), 0)::bigint AS redeemed,
                COALESCE(sum(points), 0)::bigint AS liability
         FROM points_ledger WHERE tenant = $1",
    )
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    let liability: i64 = l.get("liability");
    let cb = sqlx::query("SELECT COALESCE(sum(amount) FILTER (WHERE status = 'accrued'), 0) AS accrued FROM cashback_accruals WHERE tenant = $1").bind(t).fetch_one(&st.pool).await?;
    let cbp: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM cashback_payouts WHERE tenant = $1 AND status = 'paid' AND paid_at > now() - interval '30 days'").bind(t).fetch_one(&st.pool).await?;
    let b = sqlx::query(
        "SELECT (SELECT count(*) FROM bonus_campaigns WHERE tenant = $1 AND status = 'active' AND created_by IS DISTINCT FROM 'system:loyalty') AS campaigns,
                (SELECT count(*) FROM bonus_grants WHERE tenant = $1 AND status = 'active') AS grants,
                (SELECT COALESCE(sum(amount), 0) FROM bonus_events WHERE tenant = $1 AND kind = 'grant' AND status = 'booked' AND booked_at > now() - interval '30 days') AS issued,
                (SELECT COALESCE(sum(amount), 0) FROM bonus_events WHERE tenant = $1 AND kind = 'release' AND status = 'booked' AND booked_at > now() - interval '30 days') AS released,
                (SELECT COALESCE(sum(amount), 0) FROM bonus_events WHERE tenant = $1 AND kind = 'remove' AND status = 'booked' AND booked_at > now() - interval '30 days') AS forfeited,
                (SELECT COALESCE(sum(amount - released - removed), 0) FROM bonus_grants WHERE tenant = $1 AND status IN ('active','pending')) AS outstanding",
    )
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    let c = sqlx::query(
        "SELECT count(*) FILTER (WHERE status NOT IN ('draft','cancelled','finalized','paid') AND starts_at <= now() AND ends_at > now()) AS running,
                count(*) FILTER (WHERE status NOT IN ('draft','cancelled') AND starts_at > now()) AS scheduled,
                (SELECT count(*) FROM contest_entries e JOIN contests x ON x.id = e.contest_id WHERE x.tenant = $1 AND x.starts_at <= now() AND x.ends_at > now() AND x.status NOT IN ('draft','cancelled')) AS entrants
         FROM contests WHERE tenant = $1",
    )
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    let p = sqlx::query(
        "SELECT (SELECT count(*) FROM promo_codes WHERE tenant = $1 AND active AND (ends_at IS NULL OR ends_at > now())) AS active,
                count(*) FILTER (WHERE status = 'applied') AS applied, count(*) FILTER (WHERE status = 'blocked') AS blocked
         FROM promo_redemptions WHERE tenant = $1 AND created_at > now() - interval '30 days'",
    )
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    let bn = sqlx::query(
        "SELECT (SELECT count(*) FROM banners WHERE tenant = $1 AND active AND (ends_at IS NULL OR ends_at > now())) AS active,
                count(*) FILTER (WHERE e.kind = 'impression') AS imp, count(*) FILTER (WHERE e.kind = 'click') AS clk
         FROM banner_events e JOIN banners b ON b.id = e.banner_id WHERE b.tenant = $1 AND e.day > current_date - 30",
    )
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({
        "loyalty": {"members": l.get::<i64, _>("members"), "pointsIssued30d": l.get::<i64, _>("issued"), "pointsRedeemed30d": l.get::<i64, _>("redeemed"),
                    "liabilityPoints": liability, "liabilityUsd": num(D::from(liability) * settings.point_value)},
        "cashback": {"accrued": num(cb.get("accrued")), "paid30d": num(cbp.unwrap_or(ZERO))},
        "bonus": {"activeCampaigns": b.get::<i64, _>("campaigns"), "activeGrants": b.get::<i64, _>("grants"), "issued30d": num(b.get("issued")),
                  "released30d": num(b.get("released")), "forfeited30d": num(b.get("forfeited")), "outstanding": num(b.get("outstanding"))},
        "contests": {"running": c.get::<i64, _>("running"), "scheduled": c.get::<i64, _>("scheduled"), "entrants": c.get::<i64, _>("entrants")},
        "promos": {"active": p.get::<i64, _>("active"), "redemptions30d": p.get::<i64, _>("applied"), "blocked30d": p.get::<i64, _>("blocked")},
        "banners": {"active": bn.get::<i64, _>("active"), "impressions30d": bn.get::<i64, _>("imp"), "clicks30d": bn.get::<i64, _>("clk")},
    })))
}

// ---------------------------------------------------------------- settings, tiers

pub async fn get_settings(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    Ok(Json(serde_json::to_value(db::settings(&st.pool, &s.tenant).await?)?))
}

pub async fn put_settings(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let before = serde_json::to_value(db::settings(&st.pool, &s.tenant).await?)?;
    let next: Settings = parse(merge(before.clone(), &body))?;
    next.validate().map_err(|(f, m)| invalid(f, m))?;
    let mut tx = st.pool.begin().await?;
    sqlx::query("INSERT INTO settings (tenant, data, updated_by) VALUES ($1,$2,$3) ON CONFLICT (tenant) DO UPDATE SET data = $2, updated_by = $3, updated_at = now()")
        .bind(&s.tenant)
        .bind(sqlx::types::Json(&next))
        .bind(s.actor().label())
        .execute(&mut *tx)
        .await?;
    let after = serde_json::to_value(&next)?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "settings.update", None, Some(before), Some(after.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(after))
}

pub async fn get_tiers(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    Ok(Json(json!({"tiers": db::tiers(&st.pool, &s.tenant).await?})))
}

#[derive(Deserialize)]
pub struct TiersBody {
    tiers: Vec<Tier>,
}

pub async fn put_tiers(State(st): State<AppState>, s: StaffCtx, Json(b): Json<TiersBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    validate_tiers(&b.tiers).map_err(|m| invalid("tiers", m))?;
    let before = db::tiers(&st.pool, &s.tenant).await?;
    let mut tx = st.pool.begin().await?;
    db::replace_tiers(&mut tx, &s.tenant, &b.tiers).await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "tiers.update", None, Some(json!(before)), Some(json!(b.tiers)), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"tiers": db::tiers(&st.pool, &s.tenant).await?})))
}

// ---------------------------------------------------------------- earning rules

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleIn {
    name: String,
    #[serde(default)]
    asset_class: Option<String>,
    #[serde(default)]
    symbols: Vec<String>,
    #[serde(default)]
    account_groups: Vec<String>,
    #[serde(default = "live")]
    account_type: String,
    #[serde(deserialize_with = "de_dec")]
    points_per_lot: D,
    #[serde(default = "hundred")]
    priority: i32,
    #[serde(default = "yes")]
    active: bool,
}

fn live() -> String {
    "live".into()
}
fn hundred() -> i32 {
    100
}
fn yes() -> bool {
    true
}

impl RuleIn {
    fn check(mut self) -> ApiResult<Self> {
        self.name = text(&self.name, "name", 60, true)?;
        self.asset_class = self.asset_class.map(|a| a.trim().to_lowercase()).filter(|a| !a.is_empty());
        if let Some(a) = &self.asset_class
            && !ASSET_CLASSES.contains(&a.as_str())
        {
            return Err(invalid("assetClass", format!("Asset class must be one of {}.", ASSET_CLASSES.join(", "))));
        }
        self.symbols = clean_list(&self.symbols, true);
        self.account_groups = clean_list(&self.account_groups, false);
        if !matches!(self.account_type.as_str(), "live" | "demo" | "any") {
            return Err(invalid("accountType", "live, demo or any."));
        }
        if self.points_per_lot < ZERO || self.points_per_lot > D::from(100_000) {
            return Err(invalid("pointsPerLot", "0 – 100000 points per lot."));
        }
        Ok(self)
    }
}

pub async fn rules(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    Ok(Json(json!({"items": db::rules(&st.pool, &s.tenant).await?.iter().map(|r| r.json()).collect::<Vec<_>>()})))
}

async fn write_rule(st: &AppState, s: &StaffCtx, id: Option<i64>, r: RuleIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => sqlx::query("INSERT INTO earn_rules (tenant, name, asset_class, symbols, account_groups, account_type, points_per_lot, priority, active) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING *"),
        Some(_) => sqlx::query("UPDATE earn_rules SET name = $2, asset_class = $3, symbols = $4, account_groups = $5, account_type = $6, points_per_lot = $7, priority = $8, active = $9, updated_at = now() WHERE id = $10 AND tenant = $1 RETURNING *"),
    }
    .bind(&s.tenant)
    .bind(&r.name)
    .bind(&r.asset_class)
    .bind(&r.symbols)
    .bind(&r.account_groups)
    .bind(&r.account_type)
    .bind(r.points_per_lot)
    .bind(r.priority)
    .bind(r.active)
    .bind(id.unwrap_or(0))
    .fetch_one(&mut *tx)
    .await?;
    let v = db::rule_row(&row).json();
    audit::record(&mut *tx, &s.tenant, &s.actor(), if id.is_some() { "rule.update" } else { "rule.create" }, Some(format!("rule:{}", v["id"])), before, Some(v.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"rule": v})))
}

pub async fn create_rule(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let r = parse::<RuleIn>(body)?.check()?;
    write_rule(&st, &s, None, r, None).await
}

pub async fn patch_rule(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let cur = sqlx::query("SELECT * FROM earn_rules WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = db::rule_row(&cur).json();
    let r = parse::<RuleIn>(merge(before.clone(), &body))?.check()?;
    write_rule(&st, &s, Some(id), r, Some(before)).await
}

// ---------------------------------------------------------------- catalogue

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemIn {
    name: String,
    #[serde(default)]
    description: String,
    kind: String,
    cost_points: i64,
    #[serde(deserialize_with = "de_dec")]
    value: D,
    #[serde(default)]
    min_tier: Option<String>,
    #[serde(default)]
    stock: Option<i32>,
    #[serde(default)]
    params: Value,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default = "hundred")]
    sort: i32,
}

impl ItemIn {
    fn check(mut self, tiers: &[Tier]) -> ApiResult<Self> {
        self.name = text(&self.name, "name", 80, true)?;
        self.description = text(&self.description, "description", 400, false)?;
        if !matches!(self.kind.as_str(), "cashback" | "bonus_credit" | "fee_discount") {
            return Err(invalid("kind", "cashback, bonus_credit or fee_discount."));
        }
        if self.cost_points <= 0 || self.cost_points > 100_000_000 {
            return Err(invalid("costPoints", "Cost must be a positive number of points."));
        }
        if self.value <= ZERO || (self.kind == "fee_discount" && self.value > D::from(100)) || self.value > D::from(100_000) {
            return Err(invalid("value", if self.kind == "fee_discount" { "Discount must be 1–100%." } else { "Value must be positive (USD)." }));
        }
        self.min_tier = self.min_tier.filter(|t| !t.is_empty());
        if let Some(t) = &self.min_tier
            && !tiers.iter().any(|x| &x.key == t)
        {
            return Err(invalid("minTier", "Unknown tier."));
        }
        if self.stock.is_some_and(|s| s < 0) {
            return Err(invalid("stock", "Stock cannot be negative."));
        }
        if !self.params.is_object() {
            self.params = json!({});
        }
        match self.kind.as_str() {
            "bonus_credit" => {
                let rpl = self.params["releasePerLot"].as_f64().unwrap_or(5.0);
                let exp = self.params["expiryDays"].as_i64().unwrap_or(60);
                if rpl <= 0.0 || !(1..=365).contains(&exp) {
                    return Err(invalid("params", "Bonus credit needs releasePerLot > 0 and expiryDays 1–365."));
                }
                self.params = json!({"releasePerLot": rpl, "expiryDays": exp});
            }
            "fee_discount" => {
                let scope = self.params["appliesTo"].as_str().unwrap_or("any").to_string();
                let days = self.params["validDays"].as_i64().unwrap_or(90);
                if !matches!(scope.as_str(), "any" | "prop" | "commission") || !(1..=730).contains(&days) {
                    return Err(invalid("params", "Discount needs appliesTo any|prop|commission and validDays 1–730."));
                }
                self.params = json!({"appliesTo": scope, "validDays": days});
            }
            _ => self.params = json!({}),
        }
        Ok(self)
    }
}

pub async fn catalogue(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let rows = sqlx::query(
        "SELECT c.*, (SELECT count(*) FROM redemptions r WHERE r.item_id = c.id) AS redemptions, (SELECT COALESCE(sum(points), 0) FROM redemptions r WHERE r.item_id = c.id)::bigint AS burned
         FROM catalogue c WHERE c.tenant = $1 ORDER BY c.sort, c.id",
    )
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = loyalty::item_json(r);
        v["redemptions"] = json!(r.get::<i64, _>("redemptions"));
        v["pointsBurned"] = json!(r.get::<i64, _>("burned"));
        v
    }).collect::<Vec<_>>()})))
}

async fn write_item(st: &AppState, s: &StaffCtx, id: Option<i64>, i: ItemIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => sqlx::query("INSERT INTO catalogue (tenant, name, description, kind, cost_points, value, min_tier, stock, params, active, sort) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING *"),
        Some(_) => sqlx::query("UPDATE catalogue SET name = $2, description = $3, kind = $4, cost_points = $5, value = $6, min_tier = $7, stock = $8, params = $9, active = $10, sort = $11, updated_at = now() WHERE id = $12 AND tenant = $1 RETURNING *"),
    }
    .bind(&s.tenant)
    .bind(&i.name)
    .bind(&i.description)
    .bind(&i.kind)
    .bind(i.cost_points)
    .bind(i.value)
    .bind(&i.min_tier)
    .bind(i.stock)
    .bind(sqlx::types::Json(&i.params))
    .bind(i.active)
    .bind(i.sort)
    .bind(id.unwrap_or(0))
    .fetch_one(&mut *tx)
    .await?;
    let v = loyalty::item_json(&row);
    audit::record(&mut *tx, &s.tenant, &s.actor(), if id.is_some() { "catalogue.update" } else { "catalogue.create" }, Some(format!("item:{}", v["id"])), before, Some(v.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"item": v})))
}

pub async fn create_item(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let tiers = db::tiers(&st.pool, &s.tenant).await?;
    let i = parse::<ItemIn>(body)?.check(&tiers)?;
    write_item(&st, &s, None, i, None).await
}

pub async fn patch_item(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let cur = sqlx::query("SELECT * FROM catalogue WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = loyalty::item_json(&cur);
    let tiers = db::tiers(&st.pool, &s.tenant).await?;
    let i = parse::<ItemIn>(merge(before.clone(), &body))?.check(&tiers)?;
    write_item(&st, &s, Some(id), i, Some(before)).await
}

#[derive(Deserialize)]
pub struct ListQ {
    status: Option<String>,
    campaign: Option<i64>,
    user: Option<i64>,
    promo: Option<i64>,
    programme: Option<i64>,
    q: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

fn st_filter(v: &Option<String>) -> Option<String> {
    v.as_ref().filter(|s| !s.is_empty() && *s != "all").cloned()
}

fn with_name(mut v: Value, r: &sqlx::postgres::PgRow) -> Value {
    let f: Option<String> = r.try_get("first_name").ok().flatten();
    let l: Option<String> = r.try_get("last_name").ok().flatten();
    let name = format!("{} {}", f.unwrap_or_default(), l.unwrap_or_default()).trim().to_string();
    let uid = v["userId"].as_i64().or_else(|| r.try_get::<i64, _>("user_id").ok()).unwrap_or(0);
    v["userId"] = json!(uid);
    v["name"] = json!(if name.is_empty() { format!("Client #{uid}") } else { name });
    v["email"] = json!(r.try_get::<Option<String>, _>("email").ok().flatten().unwrap_or_default());
    v
}

const PROFILE_JOIN: &str = "LEFT JOIN profiles pr ON pr.tenant = x.tenant AND pr.user_id = x.user_id";

pub async fn redemptions(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 200);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT x.*, pr.first_name, pr.last_name, pr.email, count(*) OVER () AS total FROM redemptions x {PROFILE_JOIN}
         WHERE x.tenant = $1 AND ($2::text IS NULL OR x.status = $2) AND ($3::bigint IS NULL OR x.user_id = $3) ORDER BY x.id DESC OFFSET $4 LIMIT $5"
    )))
    .bind(&s.tenant)
    .bind(st_filter(&q.status))
    .bind(q.user)
    .bind(off)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(Json(json!({"items": rows.iter().map(|r| with_name(loyalty::redemption_json(r), r)).collect::<Vec<_>>(), "total": total, "page": page, "limit": limit})))
}

pub async fn retry_redemption(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let r = sqlx::query("SELECT * FROM redemptions WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if r.get::<String, _>("status") != "failed" {
        return Err(ApiError::Conflict { code: "state", message: "Only failed redemptions can be retried.".into() });
    }
    payouts::retry_credit(&st, &format!("redemption:{id}")).await?;
    sqlx::query("UPDATE redemptions SET status = 'pending', error = NULL WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, &s.tenant, &s.actor(), "redemption.retry", Some(format!("redemption:{id}")), None, None, None).await?;
    let _ = payouts::wallet_tick(&st).await;
    let r = sqlx::query("SELECT * FROM redemptions WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({"redemption": loyalty::redemption_json(&r)})))
}

pub async fn members(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 200);
    let search = q.q.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(|v| format!("%{}%", v.to_lowercase()));
    let tiers = db::tiers(&st.pool, &s.tenant).await?;
    let rows = sqlx::query(
        "SELECT x.user_id, pr.first_name, pr.last_name, pr.email, pr.country, sum(x.points)::bigint AS balance,
                COALESCE(sum(x.points) FILTER (WHERE x.points > 0 AND x.kind IN ('earn','bonus','promo')), 0)::bigint AS lifetime,
                COALESCE(sum(x.points) FILTER (WHERE x.kind IN ('earn','bonus','promo','reversal') AND x.created_at > now() - interval '365 days'), 0)::bigint AS earned12m,
                max(x.created_at) FILTER (WHERE x.kind = 'earn') AS last_earn, count(*) OVER () AS total
         FROM points_ledger x LEFT JOIN profiles pr ON pr.tenant = x.tenant AND pr.user_id = x.user_id
         WHERE x.tenant = $1 AND ($2::text IS NULL OR lower(pr.first_name || ' ' || pr.last_name || ' ' || pr.email) LIKE $2 OR x.user_id::text = $3)
         GROUP BY x.user_id, pr.first_name, pr.last_name, pr.email, pr.country ORDER BY balance DESC OFFSET $4 LIMIT $5",
    )
    .bind(&s.tenant)
    .bind(&search)
    .bind(q.q.as_deref().unwrap_or("").trim())
    .bind(off)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let e12: i64 = r.get("earned12m");
            let v = json!({
                "userId": r.get::<i64, _>("user_id"), "country": r.get::<Option<String>, _>("country"), "balance": r.get::<i64, _>("balance"),
                "lifetime": r.get::<i64, _>("lifetime"), "earned12m": e12, "tier": crate::calc::tier_for(&tiers, e12).map(|t| t.key.clone()),
                "lastEarnAt": r.get::<Option<DateTime<Utc>>, _>("last_earn"),
            });
            with_name(v, r)
        })
        .collect();
    Ok(Json(json!({"items": items, "total": total, "page": page, "limit": limit})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustBody {
    user_id: i64,
    points: i64,
    note: String,
}

pub async fn adjust_points(State(st): State<AppState>, s: StaffCtx, Json(b): Json<AdjustBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let note = text(&b.note, "note", 300, true)?;
    if b.points == 0 || b.points.abs() > 10_000_000 {
        return Err(invalid("points", "Enter a non-zero number of points."));
    }
    let bal = loyalty::adjust(&st, &s.tenant, b.user_id, b.points, &note, &s.actor()).await?;
    Ok(Json(json!({"balance": bal})))
}

// ---------------------------------------------------------------- cashback programmes

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgrammeIn {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    asset_classes: Vec<String>,
    #[serde(default)]
    symbols: Vec<String>,
    #[serde(default)]
    account_groups: Vec<String>,
    #[serde(deserialize_with = "de_dec")]
    usd_per_lot: D,
    #[serde(default, deserialize_with = "de_opt_dec")]
    max_per_month: Option<D>,
    #[serde(default)]
    opt_in: bool,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
}

pub async fn programmes(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let rows = sqlx::query(
        "SELECT p.*, (SELECT count(*) FROM cashback_enrolments e WHERE e.programme_id = p.id) AS enrolled,
                (SELECT COALESCE(sum(amount), 0) FROM cashback_accruals a WHERE a.programme_id = p.id AND a.status = 'accrued') AS accrued,
                (SELECT COALESCE(sum(amount), 0) FROM cashback_accruals a WHERE a.programme_id = p.id AND a.status = 'paid') AS paid,
                (SELECT COALESCE(sum(lots), 0) FROM cashback_accruals a WHERE a.programme_id = p.id AND a.status <> 'void' AND a.created_at > now() - interval '30 days') AS lots30d
         FROM cashback_programmes p WHERE p.tenant = $1 ORDER BY p.id DESC",
    )
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = cashback::programme_json(r);
        v["enrolled"] = json!(r.get::<i64, _>("enrolled"));
        v["accrued"] = num(r.get("accrued"));
        v["paid"] = num(r.get("paid"));
        v["lots30d"] = num(r.get("lots30d"));
        v
    }).collect::<Vec<_>>()})))
}

async fn write_programme(st: &AppState, s: &StaffCtx, id: Option<i64>, mut p: ProgrammeIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    p.name = text(&p.name, "name", 80, true)?;
    p.description = text(&p.description, "description", 400, false)?;
    p.asset_classes = clean_list(&p.asset_classes, false);
    if let Some(bad) = p.asset_classes.iter().find(|a| !ASSET_CLASSES.contains(&a.as_str())) {
        return Err(invalid("assetClasses", format!("Unknown asset class {bad}.")));
    }
    p.symbols = clean_list(&p.symbols, true);
    p.account_groups = clean_list(&p.account_groups, false);
    if p.usd_per_lot <= ZERO || p.usd_per_lot > D::from(1000) {
        return Err(invalid("usdPerLot", "Cashback must be 0–1000 USD per lot."));
    }
    if p.max_per_month.is_some_and(|m| m <= ZERO) {
        return Err(invalid("maxPerMonth", "Monthly cap must be positive (or empty for none)."));
    }
    let starts = time_req(&p.starts_at, "startsAt")?.unwrap_or_else(Utc::now);
    let ends = time_req(&p.ends_at, "endsAt")?;
    if ends.is_some_and(|e| e <= starts) {
        return Err(invalid("endsAt", "The end must be after the start."));
    }
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => sqlx::query("INSERT INTO cashback_programmes (tenant, name, description, asset_classes, symbols, account_groups, usd_per_lot, max_per_month, opt_in, active, starts_at, ends_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING *"),
        Some(_) => sqlx::query("UPDATE cashback_programmes SET name = $2, description = $3, asset_classes = $4, symbols = $5, account_groups = $6, usd_per_lot = $7, max_per_month = $8, opt_in = $9, active = $10, starts_at = $11, ends_at = $12, updated_at = now() WHERE id = $13 AND tenant = $1 RETURNING *"),
    }
    .bind(&s.tenant)
    .bind(&p.name)
    .bind(&p.description)
    .bind(&p.asset_classes)
    .bind(&p.symbols)
    .bind(&p.account_groups)
    .bind(p.usd_per_lot)
    .bind(p.max_per_month)
    .bind(p.opt_in)
    .bind(p.active)
    .bind(starts)
    .bind(ends)
    .bind(id.unwrap_or(0))
    .fetch_one(&mut *tx)
    .await?;
    let v = cashback::programme_json(&row);
    audit::record(&mut *tx, &s.tenant, &s.actor(), if id.is_some() { "cashback.update" } else { "cashback.create" }, Some(format!("programme:{}", v["id"])), before, Some(v.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"programme": v})))
}

pub async fn create_programme(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_programme(&st, &s, None, parse(body)?, None).await
}

pub async fn patch_programme(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let cur = sqlx::query("SELECT * FROM cashback_programmes WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = cashback::programme_json(&cur);
    write_programme(&st, &s, Some(id), parse(merge(before.clone(), &body))?, Some(before)).await
}

pub async fn accruals(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 500);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT x.*, p.name AS programme, pr.first_name, pr.last_name, pr.email, count(*) OVER () AS total
         FROM cashback_accruals x JOIN cashback_programmes p ON p.id = x.programme_id {PROFILE_JOIN}
         WHERE x.tenant = $1 AND ($2::bigint IS NULL OR x.programme_id = $2) AND ($3::bigint IS NULL OR x.user_id = $3) AND ($4::text IS NULL OR x.status = $4)
         ORDER BY x.id DESC OFFSET $5 LIMIT $6"
    )))
    .bind(&s.tenant)
    .bind(q.programme)
    .bind(q.user)
    .bind(st_filter(&q.status))
    .bind(off)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(Json(json!({"items": rows.iter().map(|r| with_name(cashback::accrual_json(r), r)).collect::<Vec<_>>(), "total": total, "page": page, "limit": limit})))
}

pub async fn cashback_payouts(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 500);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT x.*, pr.first_name, pr.last_name, pr.email, count(*) OVER () AS total FROM cashback_payouts x {PROFILE_JOIN}
         WHERE x.tenant = $1 AND ($2::text IS NULL OR x.status = $2) ORDER BY x.id DESC OFFSET $3 LIMIT $4"
    )))
    .bind(&s.tenant)
    .bind(st_filter(&q.status))
    .bind(off)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            with_name(
                json!({"id": r.get::<i64, _>("id"), "userId": r.get::<i64, _>("user_id"), "amount": num(r.get("amount")), "status": r.get::<String, _>("status"),
                   "attempts": r.get::<i32, _>("attempts"), "error": r.get::<Option<String>, _>("error"), "walletTxn": r.get::<Option<String>, _>("wallet_txn"),
                   "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "paidAt": r.get::<Option<DateTime<Utc>>, _>("paid_at")}),
                r,
            )
        })
        .collect();
    Ok(Json(json!({"items": items, "total": total, "page": page, "limit": limit})))
}

pub async fn run_cashback(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let (created, amount) = payouts::cashback_batch(&st, &s.tenant, true).await?;
    audit::record(&st.pool, &s.tenant, &s.actor(), "cashback.run", None, None, Some(json!({"payouts": created, "amount": num(amount)})), None).await?;
    let _ = payouts::cashback_tick(&st).await;
    Ok(Json(json!({"created": created, "amount": num(amount)})))
}

// ---------------------------------------------------------------- bonus campaigns

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignIn {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    terms: String,
    kind: String,
    #[serde(default, deserialize_with = "de_dec")]
    pct: D,
    #[serde(default, deserialize_with = "de_dec")]
    cap: D,
    #[serde(default, deserialize_with = "de_dec")]
    fixed_amount: D,
    #[serde(default, deserialize_with = "de_dec")]
    min_deposit: D,
    #[serde(deserialize_with = "de_dec")]
    release_per_lot: D,
    expiry_days: i32,
    #[serde(default = "yes")]
    forfeit_on_withdrawal: bool,
    #[serde(default = "thirty")]
    claim_window_days: i32,
    #[serde(default)]
    account_groups: Vec<String>,
    #[serde(default)]
    max_claims: Option<i32>,
    #[serde(default = "one")]
    per_user_limit: i32,
    #[serde(default)]
    new_users_days: Option<i32>,
    #[serde(default)]
    kyc_required: bool,
    #[serde(default = "public")]
    visibility: String,
    #[serde(default = "draft")]
    status: String,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
}

fn thirty() -> i32 {
    30
}
fn one() -> i32 {
    1
}
fn public() -> String {
    "public".into()
}
fn draft() -> String {
    "draft".into()
}

async fn write_campaign(st: &AppState, s: &StaffCtx, id: Option<i64>, mut c: CampaignIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    c.name = text(&c.name, "name", 80, true)?;
    c.description = text(&c.description, "description", 500, false)?;
    c.terms = text(&c.terms, "terms", 5000, false)?;
    match c.kind.as_str() {
        "deposit" => {
            if c.pct <= ZERO || c.pct > D::from(500) {
                return Err(invalid("pct", "Bonus % must be between 0 and 500."));
            }
            if c.cap < ZERO {
                return Err(invalid("cap", "Cap cannot be negative (0 = no cap)."));
            }
            if c.min_deposit < ZERO {
                return Err(invalid("minDeposit", "Minimum deposit cannot be negative."));
            }
        }
        "fixed" => {
            if c.fixed_amount <= ZERO || c.fixed_amount > D::from(100_000) {
                return Err(invalid("fixedAmount", "Fixed bonus must be positive."));
            }
        }
        _ => return Err(invalid("kind", "kind must be deposit or fixed.")),
    }
    if c.release_per_lot <= ZERO || c.release_per_lot > D::from(10_000) {
        return Err(invalid("releasePerLot", "Release per lot must be positive."));
    }
    if !(1..=365).contains(&c.expiry_days) {
        return Err(invalid("expiryDays", "Expiry must be 1–365 days."));
    }
    if !(1..=365).contains(&c.claim_window_days) {
        return Err(invalid("claimWindowDays", "Claim window must be 1–365 days."));
    }
    if c.per_user_limit < 0 || c.max_claims.is_some_and(|m| m < 1) || c.new_users_days.is_some_and(|d| d < 1) {
        return Err(invalid("perUserLimit", "Limits must be positive."));
    }
    if !matches!(c.visibility.as_str(), "public" | "code_only") {
        return Err(invalid("visibility", "public or code_only."));
    }
    if !matches!(c.status.as_str(), "draft" | "active" | "paused" | "ended") {
        return Err(invalid("status", "draft, active, paused or ended."));
    }
    c.account_groups = clean_list(&c.account_groups, false);
    let starts = time_req(&c.starts_at, "startsAt")?.unwrap_or_else(Utc::now);
    let ends = time_req(&c.ends_at, "endsAt")?;
    if ends.is_some_and(|e| e <= starts) {
        return Err(invalid("endsAt", "The end must be after the start."));
    }
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => sqlx::query(
            "INSERT INTO bonus_campaigns (tenant, name, description, terms, kind, pct, cap, fixed_amount, min_deposit, release_per_lot, expiry_days, forfeit_on_withdrawal, claim_window_days,
               account_groups, max_claims, per_user_limit, new_users_days, kyc_required, visibility, status, starts_at, ends_at, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23) RETURNING *",
        ),
        Some(_) => sqlx::query(
            "UPDATE bonus_campaigns SET name = $2, description = $3, terms = $4, kind = $5, pct = $6, cap = $7, fixed_amount = $8, min_deposit = $9, release_per_lot = $10,
               expiry_days = $11, forfeit_on_withdrawal = $12, claim_window_days = $13, account_groups = $14, max_claims = $15, per_user_limit = $16, new_users_days = $17,
               kyc_required = $18, visibility = $19, status = $20, starts_at = $21, ends_at = $22, updated_at = now() WHERE id = $24 AND tenant = $1 AND $23::text IS NOT NULL RETURNING *",
        ),
    }
    .bind(&s.tenant)
    .bind(&c.name)
    .bind(&c.description)
    .bind(&c.terms)
    .bind(&c.kind)
    .bind(c.pct)
    .bind(c.cap)
    .bind(c.fixed_amount)
    .bind(c.min_deposit)
    .bind(c.release_per_lot)
    .bind(c.expiry_days)
    .bind(c.forfeit_on_withdrawal)
    .bind(c.claim_window_days)
    .bind(&c.account_groups)
    .bind(c.max_claims)
    .bind(c.per_user_limit)
    .bind(c.new_users_days)
    .bind(c.kyc_required)
    .bind(&c.visibility)
    .bind(&c.status)
    .bind(starts)
    .bind(ends)
    .bind(s.actor().label())
    .bind(id.unwrap_or(0))
    .fetch_one(&mut *tx)
    .await?;
    let v = bonus::campaign_json(&row);
    audit::record(&mut *tx, &s.tenant, &s.actor(), if id.is_some() { "campaign.update" } else { "campaign.create" }, Some(format!("campaign:{}", v["id"])), before, Some(v.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"campaign": v})))
}

pub async fn campaigns(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let rows = sqlx::query(
        "SELECT c.*, (SELECT count(*) FROM bonus_grants g WHERE g.campaign_id = c.id AND g.status NOT IN ('cancelled','failed')) AS claims,
                (SELECT count(*) FROM bonus_grants g WHERE g.campaign_id = c.id AND g.status = 'active') AS active_grants,
                (SELECT COALESCE(sum(e.amount), 0) FROM bonus_events e JOIN bonus_grants g ON g.id = e.grant_id WHERE g.campaign_id = c.id AND e.kind = 'grant' AND e.status = 'booked') AS issued,
                (SELECT COALESCE(sum(e.amount), 0) FROM bonus_events e JOIN bonus_grants g ON g.id = e.grant_id WHERE g.campaign_id = c.id AND e.kind = 'release' AND e.status = 'booked') AS released,
                (SELECT COALESCE(sum(e.amount), 0) FROM bonus_events e JOIN bonus_grants g ON g.id = e.grant_id WHERE g.campaign_id = c.id AND e.kind = 'remove' AND e.status = 'booked') AS forfeited,
                (SELECT COALESCE(sum(g.amount - g.released - g.removed), 0) FROM bonus_grants g WHERE g.campaign_id = c.id AND g.status IN ('active','pending')) AS outstanding
         FROM bonus_campaigns c WHERE c.tenant = $1 ORDER BY c.id DESC",
    )
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = bonus::campaign_json(r);
        v["claims"] = json!(r.get::<i64, _>("claims"));
        v["active"] = json!(r.get::<i64, _>("active_grants"));
        v["issued"] = num(r.get("issued"));
        v["released"] = num(r.get("released"));
        v["forfeited"] = num(r.get("forfeited"));
        v["outstanding"] = num(r.get("outstanding"));
        v["system"] = json!(r.get::<Option<String>, _>("created_by").as_deref() == Some("system:loyalty"));
        v
    }).collect::<Vec<_>>()})))
}

pub async fn create_campaign(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_campaign(&st, &s, None, parse(body)?, None).await
}

pub async fn patch_campaign(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let cur = sqlx::query("SELECT * FROM bonus_campaigns WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = bonus::campaign_json(&cur);
    write_campaign(&st, &s, Some(id), parse(merge(before.clone(), &body))?, Some(before)).await
}

pub async fn grants(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 500);
    let rows = sqlx::query(
        "SELECT g.*, c.name AS campaign_name, pr.first_name, pr.last_name, pr.email, count(*) OVER () AS total
         FROM bonus_grants g JOIN bonus_campaigns c ON c.id = g.campaign_id LEFT JOIN profiles pr ON pr.tenant = g.tenant AND pr.user_id = g.user_id
         WHERE g.tenant = $1 AND ($2::bigint IS NULL OR g.campaign_id = $2) AND ($3::text IS NULL OR g.status = $3) AND ($4::bigint IS NULL OR g.user_id = $4)
         ORDER BY g.id DESC OFFSET $5 LIMIT $6",
    )
    .bind(&s.tenant)
    .bind(q.campaign)
    .bind(st_filter(&q.status))
    .bind(q.user)
    .bind(off)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = with_name(bonus::grant_json(r), r);
        v["userId"] = json!(r.get::<i64, _>("user_id"));
        v
    }).collect::<Vec<_>>(), "total": total, "page": page, "limit": limit})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualGrant {
    campaign_id: i64,
    user_id: i64,
    login: i64,
    #[serde(default, deserialize_with = "de_opt_dec")]
    amount: Option<D>,
    #[serde(default)]
    note: String,
}

pub async fn manual_grant(State(st): State<AppState>, s: StaffCtx, Json(b): Json<ManualGrant>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let note = text(&b.note, "note", 300, true)?;
    let c = sqlx::query("SELECT * FROM bonus_campaigns WHERE id = $1 AND tenant = $2").bind(b.campaign_id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let acc = bonus::check_account(&st, &s.tenant, b.user_id, b.login, &c.get::<Vec<String>, _>("account_groups")).await?;
    let amount = match (c.get::<String, _>("kind").as_str(), b.amount) {
        (_, Some(a)) if a > ZERO => a,
        ("fixed", _) => c.get("fixed_amount"),
        _ => return Err(invalid("amount", "Enter the bonus amount.")),
    };
    let mut tx = st.pool.begin().await?;
    let gid = bonus::queue_grant(&mut tx, &s.tenant, b.campaign_id, b.user_id, "admin", &acc, amount, None, None, c.get("release_per_lot"), c.get("expiry_days"), Some(&note)).await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "bonus.manual_grant", Some(format!("grant:{gid}")), None, Some(json!({"userId": b.user_id, "login": b.login, "amount": num(amount)})), Some(&note)).await?;
    tx.commit().await?;
    st.wake.notify_one();
    Ok(Json(json!({"grant": bonus::grant_by_id(&st, gid).await?})))
}

#[derive(Deserialize, Default)]
pub struct NoteBody {
    #[serde(default)]
    note: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    action: String,
}

pub async fn cancel_grant(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(b): Json<NoteBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let note = text(&b.note, "note", 300, true)?;
    let tenant: Option<String> = sqlx::query_scalar("SELECT tenant FROM bonus_grants WHERE id = $1").bind(id).fetch_optional(&st.pool).await?;
    if tenant.as_deref() != Some(s.tenant.as_str()) {
        return Err(ApiError::NotFound);
    }
    let mut tx = st.pool.begin().await?;
    let removed = bonus::end_grant(&mut tx, id, "cancelled", "cancelled").await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "bonus.cancel", Some(format!("grant:{id}")), None, Some(json!({"removed": num(removed)})), Some(&note)).await?;
    tx.commit().await?;
    st.wake.notify_one();
    Ok(Json(json!({"grant": bonus::grant_by_id(&st, id).await?})))
}

// ---------------------------------------------------------------- promo codes

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoIn {
    code: String,
    #[serde(default)]
    description: String,
    kind: String,
    #[serde(default)]
    campaign_id: Option<i64>,
    #[serde(default)]
    points: Option<i64>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    discount_pct: Option<D>,
    #[serde(default = "any")]
    discount_applies_to: String,
    #[serde(default)]
    max_uses: Option<i32>,
    #[serde(default = "one")]
    per_user_limit: i32,
    #[serde(default)]
    new_users_days: Option<i32>,
    #[serde(default)]
    countries: Vec<String>,
    #[serde(default)]
    kyc_required: bool,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
}

fn any() -> String {
    "any".into()
}

async fn write_promo(st: &AppState, s: &StaffCtx, id: Option<i64>, mut p: PromoIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    p.code = promos::clean_code(&p.code).ok_or_else(|| invalid("code", "3–32 letters, digits, - or _."))?;
    p.description = text(&p.description, "description", 300, false)?;
    match p.kind.as_str() {
        "bonus" => {
            let cid = p.campaign_id.ok_or_else(|| invalid("campaignId", "Choose the bonus campaign."))?;
            let ok: Option<i64> = sqlx::query_scalar("SELECT id FROM bonus_campaigns WHERE id = $1 AND tenant = $2").bind(cid).bind(&s.tenant).fetch_optional(&st.pool).await?;
            if ok.is_none() {
                return Err(invalid("campaignId", "Unknown campaign."));
            }
            p.points = None;
            p.discount_pct = None;
        }
        "points" => {
            if !p.points.is_some_and(|x| x > 0 && x <= 1_000_000) {
                return Err(invalid("points", "Points must be 1–1000000."));
            }
            p.campaign_id = None;
            p.discount_pct = None;
        }
        "discount" => {
            if !p.discount_pct.is_some_and(|x| x > ZERO && x <= D::from(100)) {
                return Err(invalid("discountPct", "Discount must be 1–100%."));
            }
            if !matches!(p.discount_applies_to.as_str(), "any" | "prop" | "commission") {
                return Err(invalid("discountAppliesTo", "any, prop or commission."));
            }
            p.campaign_id = None;
            p.points = None;
        }
        _ => return Err(invalid("kind", "bonus, points or discount.")),
    }
    if p.per_user_limit < 0 || p.max_uses.is_some_and(|m| m < 1) || p.new_users_days.is_some_and(|d| d < 1) {
        return Err(invalid("maxUses", "Limits must be positive."));
    }
    p.countries = clean_list(&p.countries, true);
    if p.countries.iter().any(|c| c.len() != 2) {
        return Err(invalid("countries", "Use ISO-2 country codes."));
    }
    let starts = time_req(&p.starts_at, "startsAt")?.unwrap_or_else(Utc::now);
    let ends = time_req(&p.ends_at, "endsAt")?;
    if ends.is_some_and(|e| e <= starts) {
        return Err(invalid("endsAt", "The end must be after the start."));
    }
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => sqlx::query(
            "INSERT INTO promo_codes (tenant, code, description, kind, campaign_id, points, discount_pct, discount_applies_to, max_uses, per_user_limit, new_users_days, countries, kyc_required, active, starts_at, ends_at, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) RETURNING *",
        ),
        Some(_) => sqlx::query(
            "UPDATE promo_codes SET description = $3, kind = $4, campaign_id = $5, points = $6, discount_pct = $7, discount_applies_to = $8, max_uses = $9, per_user_limit = $10,
               new_users_days = $11, countries = $12, kyc_required = $13, active = $14, starts_at = $15, ends_at = $16, updated_at = now()
             WHERE id = $18 AND tenant = $1 AND upper(code) = $2 AND $17::text IS NOT NULL RETURNING *",
        ),
    }
    .bind(&s.tenant)
    .bind(&p.code)
    .bind(&p.description)
    .bind(&p.kind)
    .bind(p.campaign_id)
    .bind(p.points)
    .bind(p.discount_pct)
    .bind(&p.discount_applies_to)
    .bind(p.max_uses)
    .bind(p.per_user_limit)
    .bind(p.new_users_days)
    .bind(&p.countries)
    .bind(p.kyc_required)
    .bind(p.active)
    .bind(starts)
    .bind(ends)
    .bind(s.actor().label())
    .bind(id.unwrap_or(0))
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => ApiError::Conflict { code: "exists", message: "That code already exists.".into() },
        _ => ApiError::Internal(e.into()),
    })?
    .ok_or_else(|| invalid("code", "The code of an existing promo can't be changed."))?;
    let v = promos::promo_json(&row);
    audit::record(&mut *tx, &s.tenant, &s.actor(), if id.is_some() { "promo.update" } else { "promo.create" }, Some(format!("promo:{}", v["id"])), before, Some(v.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"promo": v})))
}

pub async fn promos(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let rows = sqlx::query(
        "SELECT p.*, c.name AS campaign_name,
                (SELECT count(*) FROM promo_redemptions r WHERE r.promo_id = p.id AND r.status = 'blocked') AS blocked,
                (SELECT max(created_at) FROM promo_redemptions r WHERE r.promo_id = p.id AND r.status = 'applied') AS last_used
         FROM promo_codes p LEFT JOIN bonus_campaigns c ON c.id = p.campaign_id WHERE p.tenant = $1 ORDER BY p.id DESC",
    )
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = promos::promo_json(r);
        v["campaign"] = json!(r.get::<Option<String>, _>("campaign_name"));
        v["blocked"] = json!(r.get::<i64, _>("blocked"));
        v["lastUsedAt"] = json!(r.get::<Option<DateTime<Utc>>, _>("last_used"));
        v
    }).collect::<Vec<_>>()})))
}

pub async fn create_promo(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_promo(&st, &s, None, parse(body)?, None).await
}

pub async fn patch_promo(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(mut body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let cur = sqlx::query("SELECT * FROM promo_codes WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = promos::promo_json(&cur);
    if let Some(o) = body.as_object_mut() {
        o.remove("code");
    }
    write_promo(&st, &s, Some(id), parse(merge(before.clone(), &body))?, Some(before)).await
}

pub async fn promo_redemptions(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 500);
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT x.*, p.kind, pr.first_name, pr.last_name, pr.email, count(*) OVER () AS total
         FROM promo_redemptions x LEFT JOIN promo_codes p ON p.id = x.promo_id {PROFILE_JOIN}
         WHERE x.tenant = $1 AND ($2::bigint IS NULL OR x.promo_id = $2) AND ($3::text IS NULL OR x.status = $3) ORDER BY x.id DESC OFFSET $4 LIMIT $5"
    )))
    .bind(&s.tenant)
    .bind(q.promo)
    .bind(st_filter(&q.status))
    .bind(off)
    .bind(limit)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = with_name(promos::use_json(r), r);
        v["result"] = r.get::<Option<sqlx::types::Json<Value>>, _>("result").map(|j| j.0).unwrap_or(Value::Null);
        v
    }).collect::<Vec<_>>(), "total": total, "page": page, "limit": limit})))
}

// ---------------------------------------------------------------- banners

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BannerIn {
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    cta_label: Option<String>,
    #[serde(default)]
    cta_url: Option<String>,
    #[serde(default)]
    image_url: Option<String>,
    #[serde(default = "ember")]
    tone: String,
    placement: String,
    #[serde(default)]
    countries: Vec<String>,
    #[serde(default)]
    kyc: Vec<String>,
    #[serde(default)]
    account_types: Vec<String>,
    #[serde(default)]
    new_users_days: Option<i32>,
    #[serde(default = "hundred")]
    priority: i32,
    #[serde(default = "yes")]
    dismissible: bool,
    #[serde(default = "yes")]
    active: bool,
    #[serde(default)]
    starts_at: Option<String>,
    #[serde(default)]
    ends_at: Option<String>,
}

fn ember() -> String {
    "ember".into()
}

fn safe_url(u: &Option<String>, field: &'static str) -> ApiResult<Option<String>> {
    match u.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        None => Ok(None),
        Some(x) if x.len() <= 500 && ((x.starts_with('/') && !x.starts_with("//")) || x.starts_with("https://")) => Ok(Some(x.to_string())),
        Some(_) => Err(invalid(field, "Use a path starting with / or an https:// URL.")),
    }
}

pub fn banner_json(r: &sqlx::postgres::PgRow) -> Value {
    let mut v = banner_view(r);
    v["countries"] = json!(r.get::<Vec<String>, _>("countries"));
    v["kyc"] = json!(r.get::<Vec<String>, _>("kyc"));
    v["accountTypes"] = json!(r.get::<Vec<String>, _>("account_types"));
    v["newUsersDays"] = json!(r.get::<Option<i32>, _>("new_users_days"));
    v["priority"] = json!(r.get::<i32, _>("priority"));
    v["active"] = json!(r.get::<bool, _>("active"));
    v["startsAt"] = json!(r.get::<DateTime<Utc>, _>("starts_at"));
    v["endsAt"] = json!(r.get::<Option<DateTime<Utc>>, _>("ends_at"));
    v["createdAt"] = json!(r.get::<DateTime<Utc>, _>("created_at"));
    v
}

async fn write_banner(st: &AppState, s: &StaffCtx, id: Option<i64>, mut b: BannerIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    b.title = text(&b.title, "title", 100, true)?;
    b.body = text(&b.body, "body", 400, false)?;
    b.cta_label = b.cta_label.map(|l| l.trim().chars().take(40).collect::<String>()).filter(|l| !l.is_empty());
    b.cta_url = safe_url(&b.cta_url, "ctaUrl")?;
    b.image_url = safe_url(&b.image_url, "imageUrl")?;
    if !matches!(b.tone.as_str(), "ember" | "gold" | "neutral" | "up") {
        return Err(invalid("tone", "ember, gold, neutral or up."));
    }
    if !matches!(b.placement.as_str(), "dashboard" | "wallet" | "rewards" | "terminal") {
        return Err(invalid("placement", "dashboard, wallet, rewards or terminal."));
    }
    b.countries = clean_list(&b.countries, true);
    if b.countries.iter().any(|c| c.len() != 2) {
        return Err(invalid("countries", "Use ISO-2 country codes."));
    }
    b.kyc = clean_list(&b.kyc, false);
    if b.kyc.iter().any(|k| !matches!(k.as_str(), "unverified" | "pending" | "verified" | "rejected")) {
        return Err(invalid("kyc", "unverified, pending, verified or rejected."));
    }
    b.account_types = clean_list(&b.account_types, false);
    if b.account_types.iter().any(|k| !matches!(k.as_str(), "live" | "demo" | "none")) {
        return Err(invalid("accountTypes", "live, demo or none."));
    }
    if b.new_users_days.is_some_and(|d| d < 1) {
        return Err(invalid("newUsersDays", "Days must be positive."));
    }
    let starts = time_req(&b.starts_at, "startsAt")?.unwrap_or_else(Utc::now);
    let ends = time_req(&b.ends_at, "endsAt")?;
    if ends.is_some_and(|e| e <= starts) {
        return Err(invalid("endsAt", "The end must be after the start."));
    }
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => sqlx::query(
            "INSERT INTO banners (tenant, title, body, cta_label, cta_url, image_url, tone, placement, countries, kyc, account_types, new_users_days, priority, dismissible, active, starts_at, ends_at, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) RETURNING *",
        ),
        Some(_) => sqlx::query(
            "UPDATE banners SET title = $2, body = $3, cta_label = $4, cta_url = $5, image_url = $6, tone = $7, placement = $8, countries = $9, kyc = $10, account_types = $11,
               new_users_days = $12, priority = $13, dismissible = $14, active = $15, starts_at = $16, ends_at = $17, updated_at = now() WHERE id = $19 AND tenant = $1 AND $18::text IS NOT NULL RETURNING *",
        ),
    }
    .bind(&s.tenant)
    .bind(&b.title)
    .bind(&b.body)
    .bind(&b.cta_label)
    .bind(&b.cta_url)
    .bind(&b.image_url)
    .bind(&b.tone)
    .bind(&b.placement)
    .bind(&b.countries)
    .bind(&b.kyc)
    .bind(&b.account_types)
    .bind(b.new_users_days)
    .bind(b.priority)
    .bind(b.dismissible)
    .bind(b.active)
    .bind(starts)
    .bind(ends)
    .bind(s.actor().label())
    .bind(id.unwrap_or(0))
    .fetch_one(&mut *tx)
    .await?;
    let v = banner_json(&row);
    audit::record(&mut *tx, &s.tenant, &s.actor(), if id.is_some() { "banner.update" } else { "banner.create" }, Some(format!("banner:{}", v["id"])), before, Some(v.clone()), None).await?;
    tx.commit().await?;
    Ok(Json(json!({"banner": v})))
}

pub async fn banners(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let rows = sqlx::query(
        "SELECT b.*, (SELECT count(*) FROM banner_events e WHERE e.banner_id = b.id AND e.kind = 'impression') AS imp,
                (SELECT count(*) FROM banner_events e WHERE e.banner_id = b.id AND e.kind = 'click') AS clk,
                (SELECT count(*) FROM banner_events e WHERE e.banner_id = b.id AND e.kind = 'dismiss') AS dis
         FROM banners b WHERE b.tenant = $1 ORDER BY b.priority DESC, b.id DESC",
    )
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = banner_json(r);
        let imp: i64 = r.get("imp");
        let clk: i64 = r.get("clk");
        v["impressions"] = json!(imp);
        v["clicks"] = json!(clk);
        v["dismissals"] = json!(r.get::<i64, _>("dis"));
        v["ctr"] = json!(if imp > 0 { (clk as f64 / imp as f64 * 10000.0).round() / 100.0 } else { 0.0 });
        v
    }).collect::<Vec<_>>()})))
}

pub async fn create_banner(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_banner(&st, &s, None, parse(body)?, None).await
}

pub async fn patch_banner(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let cur = sqlx::query("SELECT * FROM banners WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let before = banner_json(&cur);
    write_banner(&st, &s, Some(id), parse(merge(before.clone(), &body))?, Some(before)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewQ {
    placement: Option<String>,
    country: Option<String>,
    kyc: Option<String>,
    account_type: Option<String>,
    signup_days: Option<i64>,
}

pub async fn banner_preview(State(st): State<AppState>, s: StaffCtx, Query(q): Query<PreviewQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let seg = Segment {
        country: q.country.clone().unwrap_or_default().to_uppercase(),
        kyc: q.kyc.clone().unwrap_or_else(|| "unverified".into()),
        signed_up_at: Some(Utc::now() - Duration::days(q.signup_days.unwrap_or(365))),
    };
    let types = vec![q.account_type.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| "live".into())];
    let placement = q.placement.clone().filter(|p| !p.is_empty() && p != "all");
    let rows = sqlx::query(BANNER_LIVE).bind(&s.tenant).bind(&placement).fetch_all(&st.pool).await?;
    let items: Vec<Value> = rows.iter().filter(|r| banner_matches(r, &seg, Some(&types))).map(banner_view).collect();
    let audience: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM profiles WHERE tenant = $1 AND ($2::text IS NULL OR country = $2) AND ($3::text IS NULL OR kyc_status = $3)
           AND ($4::bigint IS NULL OR signed_up_at > now() - make_interval(days => $4::int))",
    )
    .bind(&s.tenant)
    .bind(q.country.as_ref().filter(|c| !c.is_empty()).map(|c| c.to_uppercase()))
    .bind(q.kyc.as_ref().filter(|c| !c.is_empty()))
    .bind(q.signup_days)
    .fetch_one(&st.pool)
    .await?;
    Ok(Json(json!({"items": items, "audience": audience})))
}

// ---------------------------------------------------------------- contests

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ContestIn {
    #[serde(default)]
    slug: Option<String>,
    name: String,
    #[serde(default)]
    description: String,
    kind: String,
    /// cfd (default) | options (O36)
    #[serde(default = "cfd")]
    instrument: String,
    starts_at: String,
    ends_at: String,
    #[serde(default = "ret")]
    scoring: String,
    #[serde(default)]
    min_trades: i32,
    #[serde(default)]
    max_entrants: Option<i32>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    starting_balance: Option<D>,
    #[serde(default)]
    demo_group: Option<String>,
    #[serde(default)]
    account_groups: Vec<String>,
    #[serde(default)]
    kyc_required: bool,
    #[serde(default, deserialize_with = "de_opt_dec")]
    min_equity: Option<D>,
    #[serde(default)]
    prizes: Vec<Prize>,
    #[serde(default)]
    rules: String,
    #[serde(default)]
    anti_cheat: AntiCheat,
    /// Options contests: minimum opening premium per trade (USD) for a trade to add volume and count as a trade.
    #[serde(default, deserialize_with = "de_opt_dec")]
    min_premium: Option<D>,
    #[serde(default = "scheduled")]
    status: String,
    /// Phase 4: CFD asset classes that count (empty = all), the template it came from, the contest it was cloned from.
    #[serde(default)]
    asset_classes: Vec<String>,
    #[serde(default)]
    template_id: Option<i64>,
    #[serde(default)]
    cloned_from: Option<i64>,
}

fn ret() -> String {
    "return_pct".into()
}
fn cfd() -> String {
    "cfd".into()
}
fn scheduled() -> String {
    "scheduled".into()
}

pub(crate) fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(50).collect()
}

pub(crate) async fn write_contest(st: &AppState, s: &StaffCtx, id: Option<i64>, mut c: ContestIn, before: Option<Value>) -> ApiResult<Json<Value>> {
    c.name = text(&c.name, "name", 80, true)?;
    c.description = text(&c.description, "description", 1000, false)?;
    c.rules = text(&c.rules, "rules", 5000, false)?;
    if !matches!(c.kind.as_str(), "demo" | "live") {
        return Err(invalid("kind", "demo or live."));
    }
    if !calc::CONTEST_INSTRUMENTS.contains(&c.instrument.as_str()) {
        return Err(invalid("instrument", "cfd or options."));
    }
    let options = c.instrument == "options";
    if !calc::scoring_allowed(&c.instrument, &c.scoring) {
        return Err(invalid("scoring", if options { "Options contests score return_pct, profit or contracts." } else { "return_pct, profit or lots." }));
    }
    if options {
        if c.min_premium.is_some_and(|m| m < ZERO || m > D::from(1_000_000)) {
            return Err(invalid("minPremium", "Minimum premium must be 0 – 1 000 000 USD."));
        }
        c.min_premium = c.min_premium.filter(|m| *m > ZERO);
    } else {
        c.min_premium = None;
    }
    let starts = time_req(&Some(c.starts_at.clone()), "startsAt")?.ok_or_else(|| invalid("startsAt", "Required."))?;
    let ends = time_req(&Some(c.ends_at.clone()), "endsAt")?.ok_or_else(|| invalid("endsAt", "Required."))?;
    if ends <= starts {
        return Err(invalid("endsAt", "The end must be after the start."));
    }
    if ends - starts > Duration::days(366) {
        return Err(invalid("endsAt", "A contest can last at most a year."));
    }
    if !(0..=10_000).contains(&c.min_trades) {
        return Err(invalid("minTrades", "0–10000."));
    }
    if c.max_entrants.is_some_and(|m| m < 1) {
        return Err(invalid("maxEntrants", "Must be positive (or empty for no limit)."));
    }
    if c.kind == "demo" {
        let b = c.starting_balance.unwrap_or(D::from(10_000));
        if b < D::from(100) || b > D::from(1_000_000) {
            return Err(invalid("startingBalance", "Demo balance must be 100 – 1 000 000."));
        }
        c.starting_balance = Some(b);
        c.demo_group = Some(c.demo_group.clone().map(|g| g.trim().to_lowercase()).filter(|g| !g.is_empty()).unwrap_or_else(|| "standard".into()));
        c.min_equity = None;
        if options && c.demo_group.as_deref().is_some_and(calc::options_system_group) {
            return Err(invalid("demoGroup", "Options can't be traded in copy-trading, PAMM, MAM or prop groups. Choose a demo group with options."));
        }
    } else {
        c.starting_balance = None;
        c.demo_group = None;
        if c.min_equity.is_some_and(|m| m < ZERO) {
            return Err(invalid("minEquity", "Cannot be negative."));
        }
    }
    c.account_groups = clean_list(&c.account_groups, false);
    if options && let Some(g) = c.account_groups.iter().find(|g| calc::options_system_group(g)) {
        return Err(invalid("accountGroups", format!("Options can't be traded in the {g} group (copy-trading, PAMM, MAM and prop accounts never trade options).")));
    }
    c.asset_classes = clean_list(&c.asset_classes, false);
    if options {
        c.asset_classes.clear();
    }
    if let Some(a) = c.asset_classes.iter().find(|a| !ASSET_CLASSES.contains(&a.as_str())) {
        return Err(invalid("assetClasses", format!("Unknown asset class '{a}' ({}).", ASSET_CLASSES.join(", "))));
    }
    if let Some(t) = c.template_id {
        let ok: Option<i64> = sqlx::query_scalar("SELECT id FROM contest_templates WHERE id = $1 AND tenant = $2").bind(t).bind(&s.tenant).fetch_optional(&st.pool).await?;
        if ok.is_none() {
            c.template_id = None;
        }
    }
    if let Some(t) = c.cloned_from {
        let ok: Option<i64> = sqlx::query_scalar("SELECT id FROM contests WHERE id = $1 AND tenant = $2").bind(t).bind(&s.tenant).fetch_optional(&st.pool).await?;
        if ok.is_none() {
            c.cloned_from = None;
        }
    }
    validate_prizes(&c.prizes).map_err(|m| invalid("prizes", m))?;
    if c.anti_cheat.max_single_trade_pct <= ZERO || c.anti_cheat.max_single_trade_pct > D::from(100) || c.anti_cheat.min_hold_seconds < 0 {
        return Err(invalid("antiCheat", "Single-trade share 1–100%, hold ≥ 0 s."));
    }
    let slug = match c.slug.as_deref().map(slugify).filter(|x| !x.is_empty()) {
        Some(x) => x,
        None => format!("{}-{}", slugify(&c.name), db::random_code(4).to_lowercase()),
    };
    let mut tx = st.pool.begin().await?;
    let row = match id {
        None => {
            if !matches!(c.status.as_str(), "draft" | "scheduled") {
                return Err(invalid("status", "draft or scheduled."));
            }
            sqlx::query(
                "INSERT INTO contests (tenant, slug, name, description, kind, status, starts_at, ends_at, scoring, min_trades, max_entrants, starting_balance, demo_group, account_groups,
                   kyc_required, min_equity, prizes, rules, anti_cheat, created_by, instrument, min_premium, asset_classes, template_id, cloned_from)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$22,$23,$24,$25,$26) RETURNING *",
            )
        }
        Some(_) => sqlx::query(
            "UPDATE contests SET slug = $2, name = $3, description = $4, kind = $5, status = $6, starts_at = $7, ends_at = $8, scoring = $9, min_trades = $10, max_entrants = $11,
               starting_balance = $12, demo_group = $13, account_groups = $14, kyc_required = $15, min_equity = $16, prizes = $17, rules = $18, anti_cheat = $19,
               instrument = $22, min_premium = $23, asset_classes = $24, template_id = COALESCE($25, template_id), cloned_from = COALESCE($26, cloned_from), updated_at = now()
             WHERE id = $21 AND tenant = $1 AND $20::text IS NOT NULL RETURNING *",
        ),
    }
    .bind(&s.tenant)
    .bind(&slug)
    .bind(&c.name)
    .bind(&c.description)
    .bind(&c.kind)
    .bind(&c.status)
    .bind(starts)
    .bind(ends)
    .bind(&c.scoring)
    .bind(c.min_trades)
    .bind(c.max_entrants)
    .bind(c.starting_balance)
    .bind(&c.demo_group)
    .bind(&c.account_groups)
    .bind(c.kyc_required)
    .bind(c.min_equity)
    .bind(sqlx::types::Json(&c.prizes))
    .bind(&c.rules)
    .bind(sqlx::types::Json(&c.anti_cheat))
    .bind(s.actor().label())
    .bind(id.unwrap_or(0))
    .bind(&c.instrument)
    .bind(c.min_premium)
    .bind(&c.asset_classes)
    .bind(c.template_id)
    .bind(c.cloned_from)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.code().as_deref() == Some("23505") => invalid("slug", "That slug is taken."),
        _ => ApiError::Internal(e.into()),
    })?;
    let cid: i64 = row.get("id");
    tx.commit().await?;
    let full = sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.id = $1"))).bind(cid).fetch_one(&st.pool).await?;
    let v = contests::contest_json(&full);
    audit::record(&st.pool, &s.tenant, &s.actor(), if id.is_some() { "contest.update" } else { "contest.create" }, Some(format!("contest:{cid}")), before, Some(v.clone()), None).await?;
    Ok(Json(json!({"contest": v})))
}

pub async fn contests(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "{CONTEST_SELECT} WHERE c.tenant = $1 ORDER BY CASE WHEN c.status IN ('draft','cancelled') THEN 1 ELSE 0 END, c.starts_at DESC LIMIT 200"
    )))
    .bind(&s.tenant)
    .fetch_all(&st.pool)
    .await?;
    let flags: Vec<(i64, i64)> = sqlx::query_as("SELECT f.contest_id, count(*) FROM contest_flags f JOIN contests c ON c.id = f.contest_id WHERE c.tenant = $1 AND f.status = 'open' GROUP BY f.contest_id")
        .bind(&s.tenant)
        .fetch_all(&st.pool)
        .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| {
        let mut v = contests::contest_json(r);
        let id: i64 = r.get("id");
        v["openFlags"] = json!(flags.iter().find(|f| f.0 == id).map(|f| f.1).unwrap_or(0));
        v
    }).collect::<Vec<_>>()})))
}

pub async fn create_contest(State(st): State<AppState>, s: StaffCtx, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    write_contest(&st, &s, None, parse(body)?, None).await
}

async fn load_contest(st: &AppState, s: &StaffCtx, id: i64) -> ApiResult<sqlx::postgres::PgRow> {
    sqlx::query(sqlx::AssertSqlSafe(format!("{CONTEST_SELECT} WHERE c.id = $1 AND c.tenant = $2"))).bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

pub async fn contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let c = load_contest(&st, &s, id).await?;
    let (board, _) = contests::leaderboard(&st, id, None, true, 1000).await?;
    let flags = sqlx::query("SELECT f.*, e.display_name, e.login, e.user_id FROM contest_flags f JOIN contest_entries e ON e.id = f.entry_id WHERE f.contest_id = $1 ORDER BY f.status = 'open' DESC, f.id DESC")
        .bind(id)
        .fetch_all(&st.pool)
        .await?;
    let flag_json: Vec<Value> = flags.iter().map(contests::flag_json).collect();
    let board: Vec<Value> = board
        .into_iter()
        .map(|mut e| {
            let eid = e["entryId"].as_i64();
            e["flags"] = json!(flag_json.iter().filter(|f| f["entryId"].as_i64() == eid && f["status"] == "open").map(|f| f["kind"].clone()).collect::<Vec<_>>());
            e
        })
        .collect();
    Ok(Json(json!({"contest": contests::contest_json(&c), "leaderboard": board, "flags": flag_json})))
}

pub async fn patch_contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(body): Json<Value>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let c = load_contest(&st, &s, id).await?;
    let before = contests::contest_json(&c);
    let stored: String = c.get("status");
    let effective = contests::status_of(&c);
    let mut base = before.clone();
    base["status"] = json!(stored);
    let started = !matches!(effective.as_str(), "draft" | "scheduled");
    if started {
        if matches!(effective.as_str(), "finalized" | "paid" | "cancelled") {
            return Err(ApiError::Conflict { code: "state", message: "This contest can no longer be edited.".into() });
        }
        const LOCKED: &[&str] = &["kind", "instrument", "startsAt", "endsAt", "scoring", "minTrades", "minPremium", "startingBalance", "demoGroup", "accountGroups", "assetClasses", "prizes", "minEquity", "status", "slug"];
        if let Some(o) = body.as_object()
            && let Some(k) = o.keys().find(|k| LOCKED.contains(&k.as_str()) && o[k.as_str()] != before[k.as_str()])
        {
            return Err(invalid("status", format!("{k} can't change after the contest has started.")));
        }
    } else if let Some(sv) = body.get("status").and_then(|v| v.as_str())
        && !matches!(sv, "draft" | "scheduled")
    {
        return Err(invalid("status", "draft or scheduled."));
    }
    write_contest(&st, &s, Some(id), parse(merge(base, &body))?, Some(before)).await
}

pub async fn cancel_contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(b): Json<NoteBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let c = load_contest(&st, &s, id).await?;
    if matches!(contests::status_of(&c).as_str(), "finalized" | "paid" | "cancelled") {
        return Err(ApiError::Conflict { code: "state", message: "This contest can't be cancelled now.".into() });
    }
    sqlx::query("UPDATE contests SET status = 'cancelled', updated_at = now() WHERE id = $1").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, &s.tenant, &s.actor(), "contest.cancel", Some(format!("contest:{id}")), None, None, Some(&b.note)).await?;
    Ok(Json(json!({"contest": contests::contest_json(&load_contest(&st, &s, id).await?)})))
}

pub async fn refresh_contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    load_contest(&st, &s, id).await?;
    contests::refresh(&st, id).await?;
    Ok(Json(json!({"contest": contests::contest_json(&load_contest(&st, &s, id).await?)})))
}

pub async fn finalize_contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    load_contest(&st, &s, id).await?;
    contests::finalize(&st, id, &s.actor()).await?;
    let (board, _) = contests::leaderboard(&st, id, None, true, 1000).await?;
    Ok(Json(json!({"contest": contests::contest_json(&load_contest(&st, &s, id).await?), "leaderboard": board})))
}

pub async fn pay_contest(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    load_contest(&st, &s, id).await?;
    Ok(Json(contests::pay(&st, id, &s.actor()).await?))
}

pub async fn entry_action(State(st): State<AppState>, s: StaffCtx, Path((id, entry, action)): Path<(i64, i64, String)>, body: Option<Json<NoteBody>>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let c = load_contest(&st, &s, id).await?;
    if matches!(contests::status_of(&c).as_str(), "finalized" | "paid") {
        return Err(ApiError::Conflict { code: "state", message: "Ranks are frozen after finalizing.".into() });
    }
    let reason = body.map(|b| if b.0.reason.is_empty() { b.0.note.clone() } else { b.0.reason.clone() }).unwrap_or_default();
    let n = match action.as_str() {
        "disqualify" => {
            let r = text(&reason, "reason", 300, true)?;
            sqlx::query("UPDATE contest_entries SET status = 'disqualified', disqualify_reason = $3, rank = NULL WHERE id = $1 AND contest_id = $2").bind(entry).bind(id).bind(&r).execute(&st.pool).await?
        }
        "reinstate" => sqlx::query("UPDATE contest_entries SET status = 'active', disqualify_reason = NULL WHERE id = $1 AND contest_id = $2").bind(entry).bind(id).execute(&st.pool).await?,
        _ => return Err(ApiError::NotFound),
    };
    if n.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    audit::record(&st.pool, &s.tenant, &s.actor(), &format!("contest.{action}"), Some(format!("entry:{entry}")), None, None, Some(&reason)).await?;
    contests::refresh(&st, id).await?;
    let r = sqlx::query("SELECT * FROM contest_entries WHERE id = $1").bind(entry).fetch_one(&st.pool).await?;
    Ok(Json(json!({"entry": contests::standing_json(&r, None, true, c.get("min_trades"))})))
}

pub async fn resolve_flag(State(st): State<AppState>, s: StaffCtx, Path((id, flag)): Path<(i64, i64)>, Json(b): Json<NoteBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    load_contest(&st, &s, id).await?;
    let status = match b.action.as_str() {
        "clear" => "cleared",
        "disqualify" => "disqualified",
        _ => return Err(invalid("action", "clear or disqualify.")),
    };
    let f = sqlx::query("UPDATE contest_flags SET status = $3, resolved_by = $4, resolved_at = now(), note = $5 WHERE id = $1 AND contest_id = $2 RETURNING *")
        .bind(flag)
        .bind(id)
        .bind(status)
        .bind(s.actor().label())
        .bind(&b.note)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    if status == "disqualified" {
        let reason = if b.note.trim().is_empty() { format!("Integrity review: {}", f.get::<String, _>("kind")) } else { b.note.trim().to_string() };
        sqlx::query("UPDATE contest_entries SET status = 'disqualified', disqualify_reason = $2, rank = NULL WHERE id = $1").bind(f.get::<i64, _>("entry_id")).bind(reason).execute(&st.pool).await?;
        contests::refresh(&st, id).await?;
    }
    audit::record(&st.pool, &s.tenant, &s.actor(), "contest.flag_resolve", Some(format!("flag:{flag}")), None, Some(json!({"status": status})), Some(&b.note)).await?;
    Ok(Json(json!({"flag": contests::flag_json(&f)})))
}

// ---------------------------------------------------------------- reports (cost of promotions)

#[derive(Deserialize)]
pub struct RangeQ {
    from: Option<String>,
    to: Option<String>,
}

pub async fn reports(State(st): State<AppState>, s: StaffCtx, Query(q): Query<RangeQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let to = parse_time(&q.to)?.unwrap_or_else(|| Utc::now() + Duration::days(1));
    let from = parse_time(&q.from)?.unwrap_or(to - Duration::days(31));
    if to <= from || to - from > Duration::days(800) {
        return Err(invalid("from", "Choose a range up to 800 days."));
    }
    let t = &s.tenant;
    let settings = db::settings(&st.pool, t).await?;
    let ev = |kind: &'static str| {
        sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM bonus_events WHERE tenant = $1 AND kind = $2 AND status = 'booked' AND booked_at >= $3 AND booked_at < $4").bind(t.clone()).bind(kind).bind(from).bind(to)
    };
    let issued = ev("grant").fetch_one(&st.pool).await?.unwrap_or(ZERO);
    let released = ev("release").fetch_one(&st.pool).await?.unwrap_or(ZERO);
    let forfeited = ev("remove").fetch_one(&st.pool).await?.unwrap_or(ZERO);
    let cb_paid: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM cashback_payouts WHERE tenant = $1 AND status = 'paid' AND paid_at >= $2 AND paid_at < $3").bind(t).bind(from).bind(to).fetch_one(&st.pool).await?;
    let cb_acc: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM cashback_accruals WHERE tenant = $1 AND status <> 'void' AND created_at >= $2 AND created_at < $3").bind(t).bind(from).bind(to).fetch_one(&st.pool).await?;
    let prizes_wallet: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM wallet_credits WHERE tenant = $1 AND status = 'paid' AND ref LIKE 'prize:%' AND paid_at >= $2 AND paid_at < $3").bind(t).bind(from).bind(to).fetch_one(&st.pool).await?;
    let prizes_credit: Option<D> = sqlx::query_scalar(
        "SELECT sum(e.prize_amount) FROM contest_entries e JOIN contests c ON c.id = e.contest_id WHERE c.tenant = $1 AND e.prize_status = 'paid' AND e.prize_payout = 'credit' AND c.paid_at >= $2 AND c.paid_at < $3",
    )
    .bind(t)
    .bind(from)
    .bind(to)
    .fetch_one(&st.pool)
    .await?;
    let redeemed_pts: Option<i64> = sqlx::query_scalar("SELECT (-sum(points))::bigint FROM points_ledger WHERE tenant = $1 AND kind = 'redeem' AND created_at >= $2 AND created_at < $3").bind(t).bind(from).bind(to).fetch_one(&st.pool).await?;
    let redemption_cash: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM wallet_credits WHERE tenant = $1 AND status = 'paid' AND ref LIKE 'redemption:%' AND paid_at >= $2 AND paid_at < $3").bind(t).bind(from).bind(to).fetch_one(&st.pool).await?;
    let promo_uses: i64 = sqlx::query_scalar("SELECT count(*) FROM promo_redemptions WHERE tenant = $1 AND status = 'applied' AND created_at >= $2 AND created_at < $3").bind(t).bind(from).bind(to).fetch_one(&st.pool).await?;
    let prizes = prizes_wallet.unwrap_or(ZERO) + prizes_credit.unwrap_or(ZERO);
    let points_usd = crate::money::r2(D::from(redeemed_pts.unwrap_or(0)) * settings.point_value);
    let total = released + cb_paid.unwrap_or(ZERO) + prizes + points_usd;
    let by_campaign = sqlx::query(
        "SELECT c.id, c.name,
                COALESCE(sum(e.amount) FILTER (WHERE e.kind = 'grant'), 0) AS issued, COALESCE(sum(e.amount) FILTER (WHERE e.kind = 'release'), 0) AS released,
                COALESCE(sum(e.amount) FILTER (WHERE e.kind = 'remove'), 0) AS forfeited,
                (SELECT count(*) FROM bonus_grants g WHERE g.campaign_id = c.id AND g.claimed_at >= $2 AND g.claimed_at < $3) AS claims
         FROM bonus_campaigns c LEFT JOIN bonus_grants g ON g.campaign_id = c.id
         LEFT JOIN bonus_events e ON e.grant_id = g.id AND e.status = 'booked' AND e.booked_at >= $2 AND e.booked_at < $3
         WHERE c.tenant = $1 GROUP BY c.id, c.name ORDER BY released DESC, c.id DESC",
    )
    .bind(t)
    .bind(from)
    .bind(to)
    .fetch_all(&st.pool)
    .await?;
    let by_prog = sqlx::query(
        "SELECT p.id, p.name, COALESCE(sum(a.amount) FILTER (WHERE a.status <> 'void'), 0) AS accrued, COALESCE(sum(a.amount) FILTER (WHERE a.status = 'paid'), 0) AS paid,
                COALESCE(sum(a.lots) FILTER (WHERE a.status <> 'void'), 0) AS lots
         FROM cashback_programmes p LEFT JOIN cashback_accruals a ON a.programme_id = p.id AND a.created_at >= $2 AND a.created_at < $3
         WHERE p.tenant = $1 GROUP BY p.id, p.name ORDER BY accrued DESC",
    )
    .bind(t)
    .bind(from)
    .bind(to)
    .fetch_all(&st.pool)
    .await?;
    let by_contest = sqlx::query(
        "SELECT c.id, c.name, (SELECT count(*) FROM contest_entries e WHERE e.contest_id = c.id) AS entrants,
                COALESCE((SELECT sum(prize_amount) FROM contest_entries e WHERE e.contest_id = c.id AND e.prize_status = 'paid'), 0) AS prizes
         FROM contests c WHERE c.tenant = $1 AND c.ends_at >= $2 AND c.starts_at < $3 ORDER BY c.starts_at DESC",
    )
    .bind(t)
    .bind(from)
    .bind(to)
    .fetch_all(&st.pool)
    .await?;
    let series = sqlx::query(
        "SELECT to_char(d, 'YYYY-MM-DD') AS day,
           COALESCE((SELECT sum(amount) FROM bonus_events e WHERE e.tenant = $1 AND e.kind = 'release' AND e.status = 'booked' AND e.booked_at >= d AND e.booked_at < d + interval '1 day'), 0) AS bonus,
           COALESCE((SELECT sum(amount) FROM cashback_payouts p WHERE p.tenant = $1 AND p.status = 'paid' AND p.paid_at >= d AND p.paid_at < d + interval '1 day'), 0) AS cashback,
           COALESCE((SELECT sum(amount) FROM wallet_credits w WHERE w.tenant = $1 AND w.status = 'paid' AND w.ref LIKE 'prize:%' AND w.paid_at >= d AND w.paid_at < d + interval '1 day'), 0) AS prizes,
           COALESCE((SELECT -sum(points) FROM points_ledger l WHERE l.tenant = $1 AND l.kind = 'redeem' AND l.created_at >= d AND l.created_at < d + interval '1 day'), 0)::bigint AS points
         FROM generate_series(date_trunc('day', $2::timestamptz), date_trunc('day', $3::timestamptz - interval '1 microsecond'), interval '1 day') d ORDER BY d",
    )
    .bind(t)
    .bind(from)
    .bind(to)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({
        "from": from, "to": to,
        "totals": {
            "bonusIssued": num(issued), "bonusReleased": num(released), "bonusForfeited": num(forfeited), "cashbackPaid": num(cb_paid.unwrap_or(ZERO)),
            "cashbackAccrued": num(cb_acc.unwrap_or(ZERO)), "prizesPaid": num(prizes), "pointsRedeemed": redeemed_pts.unwrap_or(0), "pointsRedeemedUsd": num(points_usd),
            "redemptionCashPaid": num(redemption_cash.unwrap_or(ZERO)), "promoRedemptions": promo_uses, "total": num(total),
        },
        "byCampaign": by_campaign.iter().map(|r| json!({"id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "issued": num(r.get("issued")), "released": num(r.get("released")), "forfeited": num(r.get("forfeited")), "claims": r.get::<i64, _>("claims")})).collect::<Vec<_>>(),
        "byProgramme": by_prog.iter().map(|r| json!({"id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "accrued": num(r.get("accrued")), "paid": num(r.get("paid")), "lots": num(r.get("lots"))})).collect::<Vec<_>>(),
        "byContest": by_contest.iter().map(|r| json!({"id": r.get::<i64, _>("id"), "name": r.get::<String, _>("name"), "prizes": num(r.get("prizes")), "entrants": r.get::<i64, _>("entrants")})).collect::<Vec<_>>(),
        "series": series.iter().map(|r| {
            let pts: D = r.get::<Option<i64>, _>("points").map(D::from).unwrap_or(ZERO);
            json!({"day": r.get::<String, _>("day"), "bonus": num(r.get("bonus")), "cashback": num(r.get("cashback")), "prizes": num(r.get("prizes")), "points": num(crate::money::r2(pts * settings.point_value))})
        }).collect::<Vec<_>>(),
    })))
}

// ---------------------------------------------------------------- audit, jobs

#[derive(Deserialize)]
pub struct AuditQ {
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn audit(State(st): State<AppState>, s: StaffCtx, Query(q): Query<AuditQ>) -> ApiResult<Json<Value>> {
    s.require(ROLES_READ)?;
    let (page, limit, off) = paging(q.page, q.limit, 50, 200);
    let rows = sqlx::query("SELECT *, count(*) OVER () AS total FROM audit_log WHERE tenant = $1 ORDER BY id DESC OFFSET $2 LIMIT $3").bind(&s.tenant).bind(off).bind(limit).fetch_all(&st.pool).await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(Json(json!({"items": rows.iter().map(|r| json!({
        "id": r.get::<i64, _>("id"), "at": r.get::<DateTime<Utc>, _>("at"), "actor": r.get::<String, _>("actor"), "actorName": r.get::<Option<String>, _>("actor_name"),
        "action": r.get::<String, _>("action"), "target": r.get::<Option<String>, _>("target"),
        "before": r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0), "after": r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0),
        "note": r.get::<Option<String>, _>("note"),
    })).collect::<Vec<_>>(), "total": total, "page": page, "limit": limit})))
}

pub async fn run_job(State(st): State<AppState>, s: StaffCtx, Path(job): Path<String>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let result = crate::workers::run_job(&st, &job).await.map_err(|e| if e.to_string() == "unknown job" { ApiError::NotFound } else { ApiError::Unavailable(e.to_string()) })?;
    Ok(Json(json!({"ok": true, "result": result})))
}

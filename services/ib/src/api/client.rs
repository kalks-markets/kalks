//! Client Area partner routes (`/v1/ib/me/*`). The caller is the signed-in client (X-Kalks-User-Id).

use super::{UserCtx, paging};
use crate::audit::{self, Actor};
use crate::calc;
use crate::db;
use crate::error::{ApiError, ApiResult, invalid};
use crate::model::{Level, Settings};
use crate::money::{D, ZERO, de_dec, num};
use crate::payouts;
use crate::state::AppState;
use crate::stats;
use crate::sync;
use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashMap;

pub struct Me {
    pub user_id: i64,
    pub code: String,
    pub first_name: String,
    pub last_name: String,
    pub level_key: String,
    pub level_since: DateTime<Utc>,
    pub joined_at: DateTime<Utc>,
    pub rebate_pct: D,
    pub split_pct: D,
    pub status: String,
    pub parent_id: Option<i64>,
}

/// The caller's member row; a client who signed up seconds ago is pulled from the gateway first.
pub async fn me(st: &AppState, u: &UserCtx) -> ApiResult<Me> {
    for attempt in 0..2 {
        if let Some(r) = sqlx::query("SELECT * FROM members WHERE user_id = $1 AND tenant = $2").bind(u.user_id).bind(&u.tenant).fetch_optional(&st.pool).await? {
            return Ok(Me {
                user_id: u.user_id,
                code: r.get("referral_code"),
                first_name: r.get("first_name"),
                last_name: r.get("last_name"),
                level_key: r.get("level_key"),
                level_since: r.get("level_since"),
                joined_at: r.get("joined_at"),
                rebate_pct: r.get("rebate_pct"),
                split_pct: r.get("split_pct"),
                status: r.get("status"),
                parent_id: r.get("parent_id"),
            });
        }
        if attempt == 0 && let Err(e) = sync::sync_once(st).await {
            tracing::warn!(error = %e, "on-demand gateway sync failed");
        }
    }
    Err(ApiError::Conflict { code: "not_ready", message: "Your partner profile is being set up. Try again in a moment.".into() })
}

pub fn level_json(l: &Level) -> Value {
    json!({
        "key": l.key, "name": l.name, "rank": l.rank, "icon": l.icon, "perks": l.perks,
        "minActiveClients": l.min_active_clients, "minMonthlyLots": num(l.min_monthly_lots), "cpaAmount": num(l.cpa_amount),
        "rates": l.rates.iter().map(|(k, v)| (k.clone(), num(*v))).collect::<serde_json::Map<_, _>>(),
        "optionsRate": num(l.option_rate()),
    })
}

/// Name as the IB may see it (D61): full, or first name + last initial when the tenant masks client details.
pub fn shown_name(first: &str, last: &str, full: bool) -> String {
    if full {
        format!("{first} {last}").trim().to_string()
    } else {
        let f: String = first.chars().take(1).collect();
        let l: String = last.chars().take(1).collect();
        format!("{f}. {l}.")
    }
}

fn programme_json(s: &Settings, levels: &[Level]) -> Value {
    json!({
        "levels": levels.iter().map(level_json).collect::<Vec<_>>(),
        "symbolGroups": s.symbol_groups,
        "tiers": s.tiers.iter().enumerate().map(|(i, p)| json!({"tier": i + 1, "pct": num(*p)})).collect::<Vec<_>>(),
        "cpa": {"enabled": s.cpa.enabled, "minFirstDeposit": num(s.cpa.min_first_deposit), "requireFirstTrade": s.cpa.require_first_trade, "holdDays": s.cpa.hold_days},
        "minTradeSeconds": s.min_trade_seconds,
        "payout": {"schedule": s.payout.schedule, "minAmount": num(s.payout.min_amount), "nextClose": payouts::next_close(&s.payout.schedule, s.payout.weekday, s.payout.month_day)},
        "maxRebatePct": num(s.max_rebate_pct),
        "maxSplitPct": num(s.max_split_pct),
        "clientVisibility": s.client_visibility,
        "excludedGroups": s.excluded_groups,
    })
}

pub async fn programme(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let s = db::settings(&st.pool, &u.tenant).await?;
    let levels = db::levels(&st.pool, &u.tenant).await?;
    Ok(Json(programme_json(&s, &levels)))
}

async fn sum_status(st: &AppState, tenant: &str, me: i64) -> anyhow::Result<HashMap<String, D>> {
    let rows = sqlx::query("SELECT status, sum(amount) AS amount FROM commissions WHERE tenant = $1 AND beneficiary_id = $2 GROUP BY status").bind(tenant).bind(me).fetch_all(&st.pool).await?;
    Ok(rows.iter().map(|r| (r.get::<String, _>("status"), r.get::<D, _>("amount"))).collect())
}

/// `GET /v1/ib/me`: everything the partner dashboard shows.
pub async fn dashboard(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let t = u.tenant.as_str();
    let s = db::settings(&st.pool, t).await?;
    let levels = db::levels(&st.pool, t).await?;
    let now = Utc::now();
    let (m0, m1) = calc::month_bounds(now);
    let (p0, _) = calc::month_bounds(m0 - Duration::days(1));
    let depth = s.tiers.len() as i32;
    let active = stats::active_clients(&st.pool, t, me.user_id, m0, m1).await?;
    let lots = stats::network_lots(&st.pool, t, me.user_id, depth, m0, m1).await?;
    let lots_prev = stats::network_lots(&st.pool, t, me.user_id, depth, p0, m0).await?;
    let level = levels.iter().find(|l| l.key == me.level_key);
    let next = calc::next_level(&levels, &me.level_key);
    let by = sum_status(&st, t, me.user_id).await?;
    let get = |k: &str| by.get(k).copied().unwrap_or(ZERO);

    let r = sqlx::query(
        "SELECT
            coalesce(sum(amount) FILTER (WHERE created_at >= $3 AND status NOT IN ('void','rejected')), 0) AS month,
            coalesce(sum(amount) FILTER (WHERE created_at >= $4 AND created_at < $3 AND status NOT IN ('void','rejected')), 0) AS prev,
            coalesce(sum(amount) FILTER (WHERE kind = 'cpa' AND status NOT IN ('void','rejected')), 0) AS cpa,
            count(*) FILTER (WHERE kind = 'cpa' AND status NOT IN ('void','rejected')) AS cpa_count
         FROM commissions WHERE tenant = $1 AND beneficiary_id = $2",
    )
    .bind(t)
    .bind(me.user_id)
    .bind(m0)
    .bind(p0)
    .fetch_one(&st.pool)
    .await?;
    let c = sqlx::query(
        "SELECT count(*) AS total, count(*) FILTER (WHERE joined_at >= $3) AS month,
                count(*) FILTER (WHERE first_deposit_at IS NOT NULL) AS funded,
                count(*) FILTER (WHERE first_deposit_at IS NOT NULL AND NOT EXISTS (SELECT 1 FROM commissions k WHERE k.kind = 'cpa' AND k.client_id = members.user_id)) AS cpa_waiting,
                count(*) FILTER (WHERE EXISTS (SELECT 1 FROM members g WHERE g.parent_id = members.user_id)) AS sub_ibs
         FROM members WHERE tenant = $1 AND parent_id = $2",
    )
    .bind(t)
    .bind(me.user_id)
    .bind(m0)
    .fetch_one(&st.pool)
    .await?;
    let clicks: i64 = sqlx::query_scalar("SELECT count(*) FILTER (WHERE unique_click) FROM clicks WHERE tenant = $1 AND user_id = $2").bind(t).bind(me.user_id).fetch_one(&st.pool).await?;

    // daily accrual, last 180 days
    let daily = sqlx::query(
        "SELECT date_trunc('day', created_at) AS d, sum(amount) AS amount FROM commissions
         WHERE tenant = $1 AND beneficiary_id = $2 AND created_at >= $3 AND status NOT IN ('void','rejected')
         GROUP BY 1 ORDER BY 1",
    )
    .bind(t)
    .bind(me.user_id)
    .bind(now - Duration::days(180))
    .fetch_all(&st.pool)
    .await?;
    let before: D = sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM commissions WHERE tenant = $1 AND beneficiary_id = $2 AND created_at < $3 AND status NOT IN ('void','rejected')")
        .bind(t)
        .bind(me.user_id)
        .bind(now - Duration::days(180))
        .fetch_one(&st.pool)
        .await?
        .unwrap_or(ZERO);
    let mut cum = before;
    let series: Vec<Value> = daily
        .iter()
        .map(|r| {
            let a: D = r.get("amount");
            cum += a;
            json!({"date": r.get::<DateTime<Utc>, _>("d"), "amount": num(a), "cumulative": num(cum)})
        })
        .collect();
    let weekly = sqlx::query(
        "SELECT date_trunc('week', created_at) AS w, sum(amount) AS amount FROM commissions
         WHERE tenant = $1 AND beneficiary_id = $2 AND created_at >= date_trunc('week', now()) - interval '11 weeks' AND status NOT IN ('void','rejected')
         GROUP BY 1 ORDER BY 1",
    )
    .bind(t)
    .bind(me.user_id)
    .fetch_all(&st.pool)
    .await?;
    let full = s.client_visibility == "full";
    let top = sqlx::query(
        "SELECT m.user_id, m.first_name, m.last_name, m.country, coalesce(sum(d.lots), 0) AS lots FROM members m
         JOIN deals d ON d.user_id = m.user_id AND d.qualified AND NOT d.reversed AND d.close_time >= $3
         WHERE m.tenant = $1 AND m.parent_id = $2 GROUP BY m.user_id ORDER BY lots DESC LIMIT 6",
    )
    .bind(t)
    .bind(me.user_id)
    .bind(m0)
    .fetch_all(&st.pool)
    .await?;
    let recent = commission_rows(&st, t, me.user_id, &CommQ::default(), 8, 0, full).await?;

    Ok(Json(json!({
        "member": {
            "userId": me.user_id, "code": me.code, "name": format!("{} {}", me.first_name, me.last_name),
            "level": level.map(level_json), "levelSince": me.level_since, "joinedAt": me.joined_at,
            "rebatePct": num(me.rebate_pct), "splitPct": num(me.split_pct), "status": me.status, "hasUpline": me.parent_id.is_some(),
        },
        "progress": {
            "month": m0.format("%Y-%m").to_string(), "monthEnds": m1,
            "activeClients": active, "monthlyLots": num(lots), "prevMonthLots": num(lots_prev),
            "next": next.map(level_json),
            "levels": levels.iter().map(level_json).collect::<Vec<_>>(),
        },
        "earnings": {
            "pending": num(get("pending")), "approved": num(get("approved")), "paid": num(get("paid")), "rejected": num(get("rejected")),
            "month": num(r.get("month")), "prevMonth": num(r.get("prev")),
            "lifetime": num(get("pending") + get("approved") + get("paid")),
            "cpaEarned": num(r.get("cpa")), "cpaCount": r.get::<i64, _>("cpa_count"), "cpaWaiting": c.get::<i64, _>("cpa_waiting"),
        },
        "counts": {"referrals": c.get::<i64, _>("total"), "referralsThisMonth": c.get::<i64, _>("month"), "funded": c.get::<i64, _>("funded"), "subIbs": c.get::<i64, _>("sub_ibs")},
        "funnel": {"clicks": clicks, "signups": c.get::<i64, _>("total"), "ftds": c.get::<i64, _>("funded")},
        "series": series,
        "weekly": weekly.iter().map(|r| json!({"week": r.get::<DateTime<Utc>, _>("w"), "amount": num(r.get("amount"))})).collect::<Vec<_>>(),
        "topClients": top.iter().map(|r| json!({
            "id": r.get::<i64, _>("user_id"), "name": shown_name(&r.get::<String, _>("first_name"), &r.get::<String, _>("last_name"), full),
            "country": r.get::<String, _>("country"), "lotsMonth": num(r.get("lots")),
        })).collect::<Vec<_>>(),
        "recent": recent.0,
        "programme": programme_json(&s, &levels),
    })))
}

// ---------------------------------------------------------------- campaigns

pub async fn campaigns(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let t = u.tenant.as_str();
    let rows = sqlx::query(
        "WITH c AS (
            SELECT id, slug, name, landing, utm_source, utm_medium, utm_campaign, active, created_at FROM campaigns WHERE tenant = $1 AND user_id = $2
            UNION ALL SELECT NULL, '', 'Default link', '/register', NULL, NULL, NULL, true, NULL
         )
         SELECT c.*,
            (SELECT count(*) FROM clicks k WHERE k.tenant = $1 AND k.user_id = $2 AND k.campaign_id IS NOT DISTINCT FROM c.id) AS clicks,
            (SELECT count(*) FROM clicks k WHERE k.tenant = $1 AND k.user_id = $2 AND k.campaign_id IS NOT DISTINCT FROM c.id AND k.unique_click) AS unique_clicks,
            (SELECT count(*) FROM members m WHERE m.tenant = $1 AND m.parent_id = $2 AND m.campaign_id IS NOT DISTINCT FROM c.id) AS signups,
            (SELECT count(*) FROM members m WHERE m.tenant = $1 AND m.parent_id = $2 AND m.campaign_id IS NOT DISTINCT FROM c.id AND m.first_deposit_at IS NOT NULL) AS ftds,
            (SELECT coalesce(sum(m.first_deposit_amount), 0) FROM members m WHERE m.tenant = $1 AND m.parent_id = $2 AND m.campaign_id IS NOT DISTINCT FROM c.id) AS deposits,
            (SELECT coalesce(sum(d.lots), 0) FROM deals d JOIN members m ON m.user_id = d.user_id
               WHERE m.tenant = $1 AND m.parent_id = $2 AND m.campaign_id IS NOT DISTINCT FROM c.id AND d.qualified AND NOT d.reversed) AS lots,
            ARRAY(SELECT count(k.id)::int FROM generate_series(now()::date - 29, now()::date, interval '1 day') g(day)
                    LEFT JOIN clicks k ON k.tenant = $1 AND k.user_id = $2 AND k.campaign_id IS NOT DISTINCT FROM c.id AND k.at::date = g.day::date
                    GROUP BY g.day ORDER BY g.day) AS trend
         FROM c ORDER BY c.created_at DESC NULLS LAST",
    )
    .bind(t)
    .bind(me.user_id)
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<Option<i64>, _>("id"), "slug": r.get::<String, _>("slug"), "name": r.get::<String, _>("name"),
                "landing": r.get::<String, _>("landing"), "utmSource": r.get::<Option<String>, _>("utm_source"),
                "utmMedium": r.get::<Option<String>, _>("utm_medium"), "utmCampaign": r.get::<Option<String>, _>("utm_campaign"),
                "active": r.get::<bool, _>("active"), "createdAt": r.get::<Option<DateTime<Utc>>, _>("created_at"),
                "clicks": r.get::<i64, _>("clicks"), "uniqueClicks": r.get::<i64, _>("unique_clicks"), "signups": r.get::<i64, _>("signups"),
                "ftds": r.get::<i64, _>("ftds"), "deposits": num(r.get("deposits")), "lots": num(r.get("lots")), "trend": r.get::<Vec<i32>, _>("trend"),
            })
        })
        .collect();
    Ok(Json(json!({"code": me.code, "items": items})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CampaignReq {
    name: Option<String>,
    slug: Option<String>,
    landing: Option<String>,
    utm_source: Option<String>,
    utm_medium: Option<String>,
    utm_campaign: Option<String>,
    active: Option<bool>,
}

fn slugify(name: &str) -> String {
    let mut s = String::new();
    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            s.push(c);
        } else if !s.ends_with('-') && !s.is_empty() {
            s.push('-');
        }
    }
    s.trim_matches('-').chars().take(40).collect::<String>().trim_matches('-').to_string()
}

fn short(v: &Option<String>, field: &'static str, max: usize) -> ApiResult<Option<String>> {
    match v.as_deref().map(str::trim).filter(|x| !x.is_empty()) {
        None => Ok(None),
        Some(x) if x.chars().count() <= max && !x.chars().any(char::is_control) => Ok(Some(x.to_string())),
        Some(_) => Err(invalid(field, format!("At most {max} characters."))),
    }
}

fn landing_ok(v: &str) -> bool {
    v.starts_with('/') && !v.starts_with("//") && v.len() <= 120 && v.chars().all(|c| c.is_ascii_alphanumeric() || "/-_".contains(c))
}

pub async fn create_campaign(State(st): State<AppState>, u: UserCtx, Json(r): Json<CampaignReq>) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let name = short(&r.name, "name", 60)?.ok_or_else(|| invalid("name", "Give the campaign a name."))?;
    let slug = match r.slug.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => super::public::clean_slug(s).ok_or_else(|| invalid("slug", "Use 1–40 letters, digits, - or _."))?,
        None => slugify(&name),
    };
    if slug.is_empty() {
        return Err(invalid("slug", "Use 1–40 letters, digits, - or _."));
    }
    let landing = r.landing.clone().unwrap_or_else(|| "/register".into());
    if !landing_ok(&landing) {
        return Err(invalid("landing", "Landing must be a path on this site, e.g. /register."));
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM campaigns WHERE tenant = $1 AND user_id = $2").bind(&u.tenant).bind(me.user_id).fetch_one(&st.pool).await?;
    if count >= 100 {
        return Err(ApiError::Conflict { code: "limit", message: "You can have up to 100 campaign links.".into() });
    }
    let id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO campaigns (tenant, user_id, slug, name, landing, utm_source, utm_medium, utm_campaign) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
         ON CONFLICT (tenant, user_id, slug) DO NOTHING RETURNING id",
    )
    .bind(&u.tenant)
    .bind(me.user_id)
    .bind(&slug)
    .bind(&name)
    .bind(&landing)
    .bind(short(&r.utm_source, "utmSource", 60)?)
    .bind(short(&r.utm_medium, "utmMedium", 60)?)
    .bind(short(&r.utm_campaign, "utmCampaign", 60)?)
    .fetch_optional(&st.pool)
    .await?;
    let Some(id) = id else { return Err(ApiError::Conflict { code: "exists", message: format!("You already have a link called '{slug}'.") }) };
    audit::record(&st.pool, &u.tenant, &Actor { id: format!("user:{}", me.user_id), name: None }, "campaign.create", Some(format!("campaign:{id}")), None, Some(json!({"slug": slug, "name": name})), None).await?;
    Ok(Json(json!({"id": id, "slug": slug, "name": name, "landing": landing})))
}

pub async fn update_campaign(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>, Json(r): Json<CampaignReq>) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let name = short(&r.name, "name", 60)?;
    let n = sqlx::query("UPDATE campaigns SET active = COALESCE($4, active), name = COALESCE($5, name) WHERE id = $1 AND tenant = $2 AND user_id = $3")
        .bind(id)
        .bind(&u.tenant)
        .bind(me.user_id)
        .bind(r.active)
        .bind(name)
        .execute(&st.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound);
    }
    Ok(Json(json!({"status": "ok"})))
}

// ---------------------------------------------------------------- clients & network

#[derive(Deserialize, Default)]
pub struct ClientsQ {
    tier: Option<i32>,
    q: Option<String>,
}

pub async fn clients(State(st): State<AppState>, u: UserCtx, Query(q): Query<ClientsQ>) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let t = u.tenant.as_str();
    let s = db::settings(&st.pool, t).await?;
    let full = s.client_visibility == "full";
    let (m0, _) = calc::month_bounds(Utc::now());
    let depth = s.tiers.len() as i32;
    let rows = sqlx::query(
        "WITH RECURSIVE down(user_id, tier) AS (
            SELECT user_id, 1 FROM members WHERE tenant = $1 AND parent_id = $2
            UNION ALL SELECT m.user_id, d.tier + 1 FROM members m JOIN down d ON m.parent_id = d.user_id WHERE d.tier < $3
         )
         SELECT m.user_id, d.tier, m.first_name, m.last_name, m.email, m.country, m.joined_at, m.kyc_status, m.level_key, m.parent_id,
                m.first_deposit_at, m.first_deposit_amount, m.first_trade_at, c.name AS campaign,
                (SELECT count(*) FROM members x WHERE x.parent_id = m.user_id) AS referrals,
                (SELECT coalesce(sum(x.lots), 0) FROM deals x WHERE x.user_id = m.user_id AND x.qualified AND NOT x.reversed AND x.close_time >= $4) AS lots_month,
                (SELECT coalesce(sum(x.lots), 0) FROM deals x WHERE x.user_id = m.user_id AND x.qualified AND NOT x.reversed) AS lots_total,
                (SELECT coalesce(sum(x.contracts), 0) FROM deals x WHERE x.user_id = m.user_id AND x.qualified AND NOT x.reversed AND x.close_time >= $4) AS contracts_month,
                (SELECT max(x.close_time) FROM deals x WHERE x.user_id = m.user_id AND x.tenant = $1 AND x.qualified) AS last_trade,
                (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = $2 AND k.client_id = m.user_id AND k.status NOT IN ('void','rejected')) AS earned
         FROM down d JOIN members m ON m.user_id = d.user_id LEFT JOIN campaigns c ON c.id = m.campaign_id
         WHERE ($5::int IS NULL OR d.tier = $5)
           AND ($6::text IS NULL OR (m.first_name || ' ' || m.last_name) ILIKE $6 OR ($7 AND m.email ILIKE $6))
         ORDER BY m.joined_at DESC LIMIT 2000",
    )
    .bind(t)
    .bind(me.user_id)
    .bind(depth)
    .bind(m0)
    .bind(q.tier)
    .bind(q.q.as_deref().map(str::trim).filter(|x| !x.is_empty() && x.len() <= 60).map(|x| format!("%{}%", x.replace(['%', '_'], ""))))
    .bind(full)
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let lots_month: D = r.get("lots_month");
            let funded = r.get::<Option<DateTime<Utc>>, _>("first_deposit_at").is_some();
            let contracts_month: D = r.get("contracts_month");
            let status = if lots_month > ZERO || contracts_month > ZERO { "active" } else if funded { "funded" } else { "registered" };
            json!({
                "id": r.get::<i64, _>("user_id"), "tier": r.get::<i32, _>("tier"),
                "name": shown_name(&r.get::<String, _>("first_name"), &r.get::<String, _>("last_name"), full),
                "email": if full { json!(r.get::<String, _>("email")) } else { Value::Null },
                "country": r.get::<String, _>("country"), "joinedAt": r.get::<DateTime<Utc>, _>("joined_at"),
                "kycStatus": r.get::<String, _>("kyc_status"), "level": r.get::<String, _>("level_key"), "parentId": r.get::<Option<i64>, _>("parent_id"),
                "campaign": r.get::<Option<String>, _>("campaign"), "referrals": r.get::<i64, _>("referrals"),
                "firstDepositAt": r.get::<Option<DateTime<Utc>>, _>("first_deposit_at"),
                "firstDepositAmount": if full { r.get::<Option<D>, _>("first_deposit_amount").map(num).unwrap_or(Value::Null) } else { Value::Null },
                "firstTradeAt": r.get::<Option<DateTime<Utc>>, _>("first_trade_at"), "lastTradeAt": r.get::<Option<DateTime<Utc>>, _>("last_trade"),
                "lotsMonth": num(lots_month), "lotsTotal": num(r.get("lots_total")), "optionContractsMonth": num(contracts_month), "earned": num(r.get("earned")), "status": status,
            })
        })
        .collect();
    Ok(Json(json!({"items": items, "visibility": s.client_visibility, "tiers": depth})))
}

/// A client's closed trades (only when the tenant shows full client details, D61; only for the caller's network).
pub async fn client_trades(State(st): State<AppState>, u: UserCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let s = db::settings(&st.pool, &u.tenant).await?;
    if s.client_visibility != "full" {
        return Err(ApiError::Forbidden("Client trades are not shared by this broker.".into()));
    }
    let tier: Option<i32> = sqlx::query_scalar(
        "WITH RECURSIVE down(user_id, tier) AS (
            SELECT user_id, 1 FROM members WHERE tenant = $1 AND parent_id = $2
            UNION ALL SELECT m.user_id, d.tier + 1 FROM members m JOIN down d ON m.parent_id = d.user_id WHERE d.tier < $3
         ) SELECT tier FROM down WHERE user_id = $4 LIMIT 1",
    )
    .bind(&u.tenant)
    .bind(me.user_id)
    .bind(s.tiers.len() as i32)
    .bind(id)
    .fetch_optional(&st.pool)
    .await?;
    if tier.is_none() {
        return Err(ApiError::NotFound);
    }
    let rows = sqlx::query(
        "SELECT d.source, d.origin, d.deal_id, d.login, d.symbol, d.side, d.volume, d.lots, d.instrument, d.contracts, d.open_time, d.close_time, d.qualified, d.reason, d.reversed,
                (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.deal_source = d.source AND k.deal_id = d.deal_id AND k.beneficiary_id = $2 AND k.status NOT IN ('void','rejected')) AS earned
         FROM deals d WHERE d.user_id = $1 AND d.tenant = $3 ORDER BY d.close_time DESC LIMIT 200",
    )
    .bind(id)
    .bind(me.user_id)
    .bind(&u.tenant)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({
        "dealId": r.get::<i64, _>("deal_id"), "source": r.get::<String, _>("origin"), "feed": r.get::<String, _>("source"), "login": r.get::<Option<i64>, _>("login"), "symbol": r.get::<String, _>("symbol"),
        "side": r.get::<String, _>("side"), "volume": num(r.get("volume")), "lots": num(r.get("lots")),
        "instrument": r.get::<String, _>("instrument"), "contracts": num(r.get("contracts")),
        "openTime": r.get::<DateTime<Utc>, _>("open_time"), "closeTime": r.get::<DateTime<Utc>, _>("close_time"),
        "qualified": r.get::<bool, _>("qualified"), "reason": r.get::<Option<String>, _>("reason"), "reversed": r.get::<bool, _>("reversed"), "earned": num(r.get("earned")),
    })).collect::<Vec<_>>()})))
}

pub async fn network(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let t = u.tenant.as_str();
    let s = db::settings(&st.pool, t).await?;
    let full = s.client_visibility == "full";
    let (m0, _) = calc::month_bounds(Utc::now());
    let rows = sqlx::query(
        "WITH RECURSIVE down(user_id, parent_id, tier) AS (
            SELECT user_id, parent_id, 1 FROM members WHERE tenant = $1 AND parent_id = $2
            UNION ALL SELECT m.user_id, m.parent_id, d.tier + 1 FROM members m JOIN down d ON m.parent_id = d.user_id WHERE d.tier < $3
         )
         SELECT d.user_id, d.parent_id, d.tier, m.first_name, m.last_name, m.country, m.level_key, m.joined_at,
                (SELECT coalesce(sum(x.lots), 0) FROM deals x WHERE x.user_id = d.user_id AND x.qualified AND NOT x.reversed AND x.close_time >= $4) AS lots_month,
                (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = $2 AND k.client_id = d.user_id AND k.created_at >= $4 AND k.status NOT IN ('void','rejected')) AS earned_month
         FROM down d JOIN members m ON m.user_id = d.user_id ORDER BY d.tier, m.joined_at LIMIT 2000",
    )
    .bind(t)
    .bind(me.user_id)
    .bind(s.tiers.len() as i32)
    .bind(m0)
    .fetch_all(&st.pool)
    .await?;
    let nodes: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("user_id"), "parentId": r.get::<Option<i64>, _>("parent_id"), "tier": r.get::<i32, _>("tier"),
                "name": shown_name(&r.get::<String, _>("first_name"), &r.get::<String, _>("last_name"), full),
                "country": r.get::<String, _>("country"), "level": r.get::<String, _>("level_key"), "joinedAt": r.get::<DateTime<Utc>, _>("joined_at"),
                "lotsMonth": num(r.get("lots_month")), "earnedMonth": num(r.get("earned_month")),
            })
        })
        .collect();
    Ok(Json(json!({"root": {"id": me.user_id, "name": format!("{} {}", me.first_name, me.last_name), "level": me.level_key, "code": me.code}, "nodes": nodes, "tiers": s.tiers.len()})))
}

// ---------------------------------------------------------------- commissions & payouts

#[derive(Deserialize, Default)]
pub struct CommQ {
    pub status: Option<String>,
    pub kind: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn commission_rows(st: &AppState, tenant: &str, me: i64, q: &CommQ, limit: i64, offset: i64, full: bool) -> anyhow::Result<(Vec<Value>, i64)> {
    let rows = sqlx::query(
        "SELECT k.*, m.first_name, m.last_name, m.country, count(*) OVER () AS total FROM commissions k LEFT JOIN members m ON m.user_id = k.client_id
         WHERE k.tenant = $1 AND k.beneficiary_id = $2 AND ($3::text IS NULL OR k.status = $3) AND ($4::text IS NULL OR k.kind = $4)
         ORDER BY k.created_at DESC, k.id DESC LIMIT $5 OFFSET $6",
    )
    .bind(tenant)
    .bind(me)
    .bind(q.status.as_deref().filter(|s| *s != "all"))
    .bind(q.kind.as_deref().filter(|s| *s != "all"))
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok((
        rows.iter()
            .map(|r| {
                let first: Option<String> = r.get("first_name");
                let last: Option<String> = r.get("last_name");
                let self_rebate = r.get::<i64, _>("client_id") == me;
                json!({
                    "id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "status": r.get::<String, _>("status"),
                    "amount": num(r.get("amount")), "tier": r.get::<i32, _>("tier"), "rate": num(r.get("rate")), "sharePct": num(r.get("share_pct")),
                    "lots": num(r.get("lots")), "contracts": num(r.get("contracts")), "symbol": r.get::<Option<String>, _>("symbol"), "symbolGroup": r.get::<Option<String>, _>("symbol_group"),
                    "dealId": r.get::<Option<i64>, _>("deal_id"), "source": r.get::<Option<String>, _>("origin").or_else(|| r.get("deal_source")), "feed": r.get::<Option<String>, _>("deal_source"), "levelKey": r.get::<Option<String>, _>("level_key"),
                    "client": {
                        "id": r.get::<i64, _>("client_id"),
                        "name": if self_rebate { "You (rebate)".to_string() } else { shown_name(first.as_deref().unwrap_or(""), last.as_deref().unwrap_or(""), full) },
                        "country": r.get::<Option<String>, _>("country"),
                    },
                    "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "availableAt": r.get::<DateTime<Utc>, _>("available_at"),
                    "batchId": r.get::<Option<i64>, _>("batch_id"), "note": r.get::<Option<String>, _>("note"),
                })
            })
            .collect(),
        total,
    ))
}

pub async fn commissions(State(st): State<AppState>, u: UserCtx, Query(q): Query<CommQ>) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let s = db::settings(&st.pool, &u.tenant).await?;
    let (page, limit, offset) = paging(q.page, q.limit, 50, 500);
    let (items, total) = commission_rows(&st, &u.tenant, me.user_id, &q, limit, offset, s.client_visibility == "full").await?;
    let by = sum_status(&st, &u.tenant, me.user_id).await?;
    let totals: serde_json::Map<String, Value> = ["pending", "approved", "paid", "rejected", "void"].iter().map(|k| (k.to_string(), num(by.get(*k).copied().unwrap_or(ZERO)))).collect();
    Ok(Json(json!({"items": items, "page": page, "limit": limit, "total": total, "totals": totals})))
}

pub async fn payouts(State(st): State<AppState>, u: UserCtx) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let rows = sqlx::query(
        "SELECT p.*, b.period_start, b.period_end, b.schedule, b.status AS batch_status FROM payouts p JOIN payout_batches b ON b.id = p.batch_id
         WHERE p.tenant = $1 AND p.user_id = $2 ORDER BY p.created_at DESC LIMIT 200",
    )
    .bind(&u.tenant)
    .bind(me.user_id)
    .fetch_all(&st.pool)
    .await?;
    let s = db::settings(&st.pool, &u.tenant).await?;
    let unbatched: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM commissions WHERE tenant = $1 AND beneficiary_id = $2 AND status = 'pending' AND batch_id IS NULL")
        .bind(&u.tenant)
        .bind(me.user_id)
        .fetch_one(&st.pool)
        .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let status: String = r.get("status");
            // clients see a simple state: awaiting approval / processing / paid / rejected
            let shown = match status.as_str() {
                "pending_approval" => "awaiting_approval",
                "transfer_pending" | "failed" => "processing",
                s => s,
            };
            json!({
                "id": r.get::<i64, _>("id"), "batchId": r.get::<i64, _>("batch_id"), "amount": num(r.get("amount")), "lines": r.get::<i32, _>("lines"),
                "status": shown, "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "paidAt": r.get::<Option<DateTime<Utc>>, _>("paid_at"),
                "periodStart": r.get::<Option<DateTime<Utc>>, _>("period_start"), "periodEnd": r.get::<DateTime<Utc>, _>("period_end"),
                "schedule": r.get::<String, _>("schedule"), "destination": "Wallet · USDT",
            })
        })
        .collect();
    Ok(Json(json!({
        "items": items,
        "unbatched": num(unbatched.unwrap_or(ZERO)),
        "schedule": s.payout.schedule, "minAmount": num(s.payout.min_amount),
        "nextClose": payouts::next_close(&s.payout.schedule, s.payout.weekday, s.payout.month_day),
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsReq {
    #[serde(deserialize_with = "de_dec")]
    rebate_pct: D,
    #[serde(deserialize_with = "de_dec")]
    split_pct: D,
}

/// Rebate to own clients and split with sub-IBs (D60), within the programme maximums. Applies to deals closed from now on.
pub async fn update_settings(State(st): State<AppState>, u: UserCtx, Json(r): Json<SettingsReq>) -> ApiResult<Json<Value>> {
    let me = me(&st, &u).await?;
    let s = db::settings(&st.pool, &u.tenant).await?;
    if r.rebate_pct < ZERO || r.rebate_pct > s.max_rebate_pct {
        return Err(invalid("rebatePct", format!("Rebate must be between 0 and {}%.", s.max_rebate_pct.normalize())));
    }
    if r.split_pct < ZERO || r.split_pct > s.max_split_pct {
        return Err(invalid("splitPct", format!("Split must be between 0 and {}%.", s.max_split_pct.normalize())));
    }
    let (rebate, split) = (r.rebate_pct.round_dp(2), r.split_pct.round_dp(2));
    sqlx::query("UPDATE members SET rebate_pct = $2, split_pct = $3 WHERE user_id = $1").bind(me.user_id).bind(rebate).bind(split).execute(&st.pool).await?;
    audit::record(
        &st.pool,
        &u.tenant,
        &Actor { id: format!("user:{}", me.user_id), name: None },
        "member.rates",
        Some(format!("user:{}", me.user_id)),
        Some(json!({"rebatePct": me.rebate_pct.to_string(), "splitPct": me.split_pct.to_string()})),
        Some(json!({"rebatePct": rebate.to_string(), "splitPct": split.to_string()})),
        None,
    )
    .await?;
    Ok(Json(json!({"rebatePct": num(rebate), "splitPct": num(split)})))
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_and_slugs() {
        assert_eq!(super::shown_name("Priya", "Sharma", true), "Priya Sharma");
        assert_eq!(super::shown_name("Priya", "Sharma", false), "P. S.");
        assert_eq!(super::slugify("  Diwali Gold -- Promo! "), "diwali-gold-promo");
        assert!(super::landing_ok("/register"));
        assert!(!super::landing_ok("//evil.com"));
        assert!(!super::landing_ok("https://evil.com"));
    }
}

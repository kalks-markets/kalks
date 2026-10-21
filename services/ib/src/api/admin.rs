//! Back Office routes (`/v1/ib/admin/*`). Reads: any staff role. Programme changes: ROLES_WRITE. Money
//! decisions (batch approve / reject, commission reject): ROLES_APPROVE. Every change is audited.

use super::client::{level_json, shown_name};
use super::{ROLES_APPROVE, ROLES_WRITE, StaffCtx, paging, parse_time};
use crate::audit;
use crate::calc;
use crate::db;
use crate::deals;
use crate::error::{ApiError, ApiResult, invalid};
use crate::model::{self, Level, Settings};
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

fn reason_of(v: &Option<String>) -> ApiResult<String> {
    let r = v.as_deref().map(str::trim).unwrap_or("");
    if r.len() < 3 || r.len() > 500 {
        return Err(invalid("reason", "Give a reason (3–500 characters). It is kept in the audit log."));
    }
    Ok(r.to_string())
}

// ---------------------------------------------------------------- settings & levels

pub async fn get_settings(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let v = db::settings(&st.pool, &s.tenant).await?;
    let meta = sqlx::query("SELECT version, updated_at, updated_by FROM settings WHERE tenant = $1").bind(&s.tenant).fetch_one(&st.pool).await?;
    Ok(Json(json!({
        "settings": v, "version": meta.get::<i32, _>("version"), "updatedAt": meta.get::<DateTime<Utc>, _>("updated_at"), "updatedBy": meta.get::<Option<String>, _>("updated_by"),
        "nextPayoutClose": payouts::next_close(&v.payout.schedule, v.payout.weekday, v.payout.month_day),
    })))
}

#[derive(Deserialize)]
pub struct PutSettings {
    settings: Settings,
    reason: Option<String>,
}

pub async fn put_settings(State(st): State<AppState>, s: StaffCtx, Json(r): Json<PutSettings>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let reason = reason_of(&r.reason)?;
    r.settings.validate().map_err(|(f, m)| invalid(f, m))?;
    let before = db::settings(&st.pool, &s.tenant).await?;
    let levels = db::levels(&st.pool, &s.tenant).await?;
    // every rate must still point at a symbol group that exists
    model::validate_levels(&levels, &r.settings).map_err(|(_, m)| invalid("symbolGroups", format!("{m} Update the levels first.")))?;
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE settings SET data = $2, version = version + 1, updated_at = now(), updated_by = $3 WHERE tenant = $1")
        .bind(&s.tenant)
        .bind(sqlx::types::Json(&r.settings))
        .bind(format!("{} ({})", s.name, s.actor().id))
        .execute(&mut *tx)
        .await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "settings.update", Some("settings".into()), Some(serde_json::to_value(&before)?), Some(serde_json::to_value(&r.settings)?), Some(&reason)).await?;
    tx.commit().await?;
    get_settings(State(st), s).await
}

pub async fn get_levels(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let levels = db::levels(&st.pool, &s.tenant).await?;
    let counts = sqlx::query("SELECT level_key, count(*) AS n FROM members WHERE tenant = $1 GROUP BY level_key").bind(&s.tenant).fetch_all(&st.pool).await?;
    let settings = db::settings(&st.pool, &s.tenant).await?;
    let items: Vec<Value> = levels
        .iter()
        .map(|l| {
            let mut v = level_json(l);
            v["members"] = json!(counts.iter().find(|c| c.get::<String, _>("level_key") == l.key).map(|c| c.get::<i64, _>("n")).unwrap_or(0));
            v
        })
        .collect();
    Ok(Json(json!({"levels": items, "symbolGroups": settings.symbol_groups})))
}

#[derive(Deserialize)]
pub struct PutLevels {
    levels: Vec<Level>,
    reason: Option<String>,
}

/// Replaces the level table. Members on a removed level move to the entry level (audited).
pub async fn put_levels(State(st): State<AppState>, s: StaffCtx, Json(r): Json<PutLevels>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let reason = reason_of(&r.reason)?;
    let settings = db::settings(&st.pool, &s.tenant).await?;
    model::validate_levels(&r.levels, &settings).map_err(|(f, m)| invalid(f, m))?;
    let before = db::levels(&st.pool, &s.tenant).await?;
    // a level sent without `optionsRate` keeps its current per-contract options rate (0 for a new level)
    let mut r = r;
    for l in r.levels.iter_mut() {
        if l.options_rate.is_none() {
            l.options_rate = Some(before.iter().find(|b| b.key == l.key).map(|b| b.option_rate()).unwrap_or(ZERO));
        }
    }
    let entry = r.levels.iter().min_by_key(|l| l.rank).map(|l| l.key.clone()).unwrap();
    let keys: Vec<String> = r.levels.iter().map(|l| l.key.clone()).collect();
    let mut tx = st.pool.begin().await?;
    sqlx::query("DELETE FROM levels WHERE tenant = $1").bind(&s.tenant).execute(&mut *tx).await?;
    for l in &r.levels {
        db::insert_level(&mut tx, &s.tenant, l).await?;
    }
    let moved = sqlx::query("UPDATE members SET level_key = $2, level_since = now() WHERE tenant = $1 AND NOT (level_key = ANY($3))")
        .bind(&s.tenant)
        .bind(&entry)
        .bind(&keys)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    audit::record(&mut *tx, &s.tenant, &s.actor(), "levels.update", Some("levels".into()), Some(serde_json::to_value(&before)?), Some(json!({"levels": r.levels, "membersMovedToEntry": moved})), Some(&reason)).await?;
    tx.commit().await?;
    get_levels(State(st), s).await
}

// ---------------------------------------------------------------- overview

pub async fn overview(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let t = s.tenant.as_str();
    let (m0, _) = calc::month_bounds(Utc::now());
    let (p0, _) = calc::month_bounds(m0 - Duration::days(1));
    let k = sqlx::query(
        "SELECT
           (SELECT count(*) FROM members WHERE tenant = $1) AS members,
           (SELECT count(*) FROM members WHERE tenant = $1 AND parent_id IS NOT NULL) AS referred,
           (SELECT count(DISTINCT parent_id) FROM members WHERE tenant = $1 AND parent_id IS NOT NULL) AS ibs,
           (SELECT count(DISTINCT beneficiary_id) FROM commissions WHERE tenant = $1 AND created_at >= $2 AND kind IN ('lot','cpa','split')) AS earning_ibs,
           (SELECT coalesce(sum(amount), 0) FROM commissions WHERE tenant = $1 AND created_at >= $2 AND status NOT IN ('void','rejected')) AS month,
           (SELECT coalesce(sum(amount), 0) FROM commissions WHERE tenant = $1 AND created_at >= $3 AND created_at < $2 AND status NOT IN ('void','rejected')) AS prev,
           (SELECT coalesce(sum(amount), 0) FROM commissions WHERE tenant = $1 AND status = 'pending') AS pending,
           (SELECT coalesce(sum(amount), 0) FROM commissions WHERE tenant = $1 AND status = 'approved') AS approved,
           (SELECT coalesce(sum(amount), 0) FROM commissions WHERE tenant = $1 AND status = 'paid') AS paid,
           (SELECT coalesce(sum(lots), 0) FROM deals WHERE tenant = $1 AND qualified AND NOT reversed AND close_time >= $2) AS lots,
           (SELECT count(*) FROM fraud_flags WHERE tenant = $1 AND status = 'open') AS flags,
           (SELECT count(*) FROM payout_batches WHERE tenant = $1 AND status = 'pending_approval') AS batches,
           (SELECT count(*) FROM payouts WHERE tenant = $1 AND status IN ('transfer_pending','failed')) AS transfers",
    )
    .bind(t)
    .bind(m0)
    .bind(p0)
    .fetch_one(&st.pool)
    .await?;
    let months = sqlx::query(
        "SELECT to_char(date_trunc('month', created_at), 'YYYY-MM') AS m,
                coalesce(sum(amount) FILTER (WHERE kind = 'lot' AND tier = 1), 0) AS t1,
                coalesce(sum(amount) FILTER (WHERE kind = 'lot' AND tier = 2), 0) AS t2,
                coalesce(sum(amount) FILTER (WHERE kind = 'lot' AND tier >= 3), 0) AS t3,
                coalesce(sum(amount) FILTER (WHERE kind = 'split'), 0) AS split,
                coalesce(sum(amount) FILTER (WHERE kind = 'rebate'), 0) AS rebate,
                coalesce(sum(amount) FILTER (WHERE kind = 'cpa'), 0) AS cpa,
                coalesce(sum(amount) FILTER (WHERE kind = 'clawback'), 0) AS clawback
         FROM commissions WHERE tenant = $1 AND created_at >= date_trunc('month', now()) - interval '11 months' AND status NOT IN ('void','rejected')
         GROUP BY 1 ORDER BY 1",
    )
    .bind(t)
    .fetch_all(&st.pool)
    .await?;
    let lots = sqlx::query("SELECT to_char(date_trunc('month', close_time), 'YYYY-MM') AS m, sum(lots) AS lots FROM deals WHERE tenant = $1 AND qualified AND NOT reversed AND close_time >= date_trunc('month', now()) - interval '11 months' GROUP BY 1")
        .bind(t)
        .fetch_all(&st.pool)
        .await?;
    let f = sqlx::query(
        "SELECT (SELECT count(*) FROM clicks WHERE tenant = $1 AND unique_click AND at > now() - interval '30 days') AS clicks,
                (SELECT count(*) FROM members WHERE tenant = $1 AND parent_id IS NOT NULL AND joined_at > now() - interval '30 days') AS signups,
                (SELECT count(*) FROM members WHERE tenant = $1 AND parent_id IS NOT NULL AND joined_at > now() - interval '30 days' AND kyc_status = 'verified') AS kyc,
                (SELECT count(*) FROM members WHERE tenant = $1 AND parent_id IS NOT NULL AND joined_at > now() - interval '30 days' AND first_deposit_at IS NOT NULL) AS ftds,
                (SELECT count(*) FROM members WHERE tenant = $1 AND parent_id IS NOT NULL AND joined_at > now() - interval '30 days' AND first_trade_at IS NOT NULL) AS traders",
    )
    .bind(t)
    .fetch_one(&st.pool)
    .await?;
    let levels = db::levels(&st.pool, t).await?;
    let counts = sqlx::query("SELECT level_key, count(*) AS n FROM members WHERE tenant = $1 GROUP BY level_key").bind(t).fetch_all(&st.pool).await?;
    let top = sqlx::query(
        "SELECT m.user_id, m.first_name, m.last_name, m.country, m.level_key, coalesce(sum(k.amount), 0) AS amount,
                (SELECT count(*) FROM members c WHERE c.parent_id = m.user_id) AS clients
         FROM commissions k JOIN members m ON m.user_id = k.beneficiary_id
         WHERE k.tenant = $1 AND k.created_at >= $2 AND k.status NOT IN ('void','rejected') AND k.kind <> 'rebate'
         GROUP BY m.user_id ORDER BY amount DESC LIMIT 8",
    )
    .bind(t)
    .bind(m0)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({
        "kpis": {
            "members": k.get::<i64, _>("members"), "referredClients": k.get::<i64, _>("referred"), "ibsWithClients": k.get::<i64, _>("ibs"),
            "earningIbs": k.get::<i64, _>("earning_ibs"), "commissionMonth": num(k.get("month")), "commissionPrevMonth": num(k.get("prev")),
            "pending": num(k.get("pending")), "approved": num(k.get("approved")), "paid": num(k.get("paid")), "lotsMonth": num(k.get("lots")),
            "openFlags": k.get::<i64, _>("flags"), "pendingBatches": k.get::<i64, _>("batches"), "transfersInFlight": k.get::<i64, _>("transfers"),
        },
        "months": months.iter().map(|r| {
            let m: String = r.get("m");
            json!({"month": m, "tier1": num(r.get("t1")), "tier2": num(r.get("t2")), "tier3": num(r.get("t3")), "split": num(r.get("split")), "rebate": num(r.get("rebate")),
                   "cpa": num(r.get("cpa")), "clawback": num(r.get("clawback")),
                   "lots": num(lots.iter().find(|l| l.get::<String, _>("m") == m).map(|l| l.get::<D, _>("lots")).unwrap_or(ZERO))})
        }).collect::<Vec<_>>(),
        "funnel": {"clicks": f.get::<i64, _>("clicks"), "signups": f.get::<i64, _>("signups"), "kyc": f.get::<i64, _>("kyc"), "ftds": f.get::<i64, _>("ftds"), "traders": f.get::<i64, _>("traders")},
        "levels": levels.iter().map(|l| json!({"key": l.key, "name": l.name, "rank": l.rank, "members": counts.iter().find(|c| c.get::<String, _>("level_key") == l.key).map(|c| c.get::<i64, _>("n")).unwrap_or(0)})).collect::<Vec<_>>(),
        "top": top.iter().map(|r| json!({
            "id": r.get::<i64, _>("user_id"), "name": shown_name(&r.get::<String, _>("first_name"), &r.get::<String, _>("last_name"), true),
            "country": r.get::<String, _>("country"), "level": r.get::<String, _>("level_key"), "commissionMonth": num(r.get("amount")), "clients": r.get::<i64, _>("clients"),
        })).collect::<Vec<_>>(),
    })))
}

// ---------------------------------------------------------------- partners

#[derive(Deserialize)]
pub struct PartnersQ {
    q: Option<String>,
    level: Option<String>,
    /// `ibs` (default: members with clients) | `all`
    scope: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn partners(State(st): State<AppState>, s: StaffCtx, Query(q): Query<PartnersQ>) -> ApiResult<Json<Value>> {
    let (page, limit, offset) = paging(q.page, q.limit, 25, 200);
    let (m0, _) = calc::month_bounds(Utc::now());
    let term = q.q.as_deref().map(str::trim).filter(|x| !x.is_empty() && x.len() <= 80);
    let pat = term.map(|x| format!("%{}%", x.replace(['%', '_'], "")));
    let id_eq = term.and_then(|x| x.trim_start_matches('#').parse::<i64>().ok());
    let rows = sqlx::query(
        "SELECT m.*, p.first_name AS p_first, p.last_name AS p_last,
            (SELECT count(*) FROM members c WHERE c.parent_id = m.user_id) AS clients,
            (SELECT count(DISTINCT d.user_id) FROM deals d JOIN members c ON c.user_id = d.user_id WHERE c.parent_id = m.user_id AND d.qualified AND NOT d.reversed AND d.close_time >= $3) AS active,
            (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = m.user_id AND k.created_at >= $3 AND k.status NOT IN ('void','rejected')) AS month,
            (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = m.user_id AND k.status = 'pending') AS pending,
            (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = m.user_id AND k.status = 'paid') AS paid,
            (SELECT count(*) FROM fraud_flags f WHERE (f.ib_id = m.user_id OR f.client_id = m.user_id) AND f.status = 'open') AS flags,
            count(*) OVER () AS total
         FROM members m LEFT JOIN members p ON p.user_id = m.parent_id
         WHERE m.tenant = $1
           AND ($2::text IS NULL OR m.level_key = $2)
           AND ($4::text IS NULL OR (m.first_name || ' ' || m.last_name) ILIKE $4 OR m.email ILIKE $4 OR m.referral_code ILIKE $4 OR m.user_id = $5)
           AND ($6 = 'all' OR EXISTS (SELECT 1 FROM members c WHERE c.parent_id = m.user_id) OR m.level_key <> (SELECT key FROM levels WHERE tenant = $1 ORDER BY rank LIMIT 1))
         ORDER BY month DESC, clients DESC, m.user_id DESC LIMIT $7 OFFSET $8",
    )
    .bind(&s.tenant)
    .bind(q.level.as_deref().filter(|l| *l != "all"))
    .bind(m0)
    .bind(pat)
    .bind(id_eq)
    .bind(q.scope.as_deref().unwrap_or("ibs"))
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items: Vec<Value> = rows.iter().map(partner_row).collect();
    Ok(Json(json!({"items": items, "page": page, "limit": limit, "total": total})))
}

fn partner_row(r: &sqlx::postgres::PgRow) -> Value {
    let pf: Option<String> = r.get("p_first");
    let pl: Option<String> = r.get("p_last");
    json!({
        "id": r.get::<i64, _>("user_id"), "name": format!("{} {}", r.get::<String, _>("first_name"), r.get::<String, _>("last_name")),
        "email": r.get::<String, _>("email"), "country": r.get::<String, _>("country"), "code": r.get::<String, _>("referral_code"),
        "level": r.get::<String, _>("level_key"), "levelLocked": r.get::<bool, _>("level_locked"), "status": r.get::<String, _>("status"),
        "parentId": r.get::<Option<i64>, _>("parent_id"), "parentName": pf.map(|f| format!("{f} {}", pl.unwrap_or_default())),
        "parentSource": r.get::<String, _>("parent_source"),
        "clients": r.get::<i64, _>("clients"), "activeClients": r.get::<i64, _>("active"),
        "commissionMonth": num(r.get("month")), "pending": num(r.get("pending")), "paid": num(r.get("paid")), "openFlags": r.get::<i64, _>("flags"),
        "rebatePct": num(r.get("rebate_pct")), "splitPct": num(r.get("split_pct")), "selfReferral": r.get::<Option<String>, _>("self_referral"),
        "joinedAt": r.get::<DateTime<Utc>, _>("joined_at"), "kycStatus": r.get::<String, _>("kyc_status"),
        "firstDepositAt": r.get::<Option<DateTime<Utc>>, _>("first_deposit_at"), "firstDepositAmount": r.get::<Option<D>, _>("first_deposit_amount").map(num),
    })
}

pub async fn partner(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let t = s.tenant.as_str();
    let (m0, m1) = calc::month_bounds(Utc::now());
    let (p0, _) = calc::month_bounds(m0 - Duration::days(1));
    let r = sqlx::query(
        "SELECT m.*, p.first_name AS p_first, p.last_name AS p_last,
            (SELECT count(*) FROM members c WHERE c.parent_id = m.user_id) AS clients,
            (SELECT count(DISTINCT d.user_id) FROM deals d JOIN members c ON c.user_id = d.user_id WHERE c.parent_id = m.user_id AND d.qualified AND NOT d.reversed AND d.close_time >= $3) AS active,
            (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = m.user_id AND k.created_at >= $3 AND k.status NOT IN ('void','rejected')) AS month,
            (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = m.user_id AND k.status = 'pending') AS pending,
            (SELECT coalesce(sum(k.amount), 0) FROM commissions k WHERE k.beneficiary_id = m.user_id AND k.status = 'paid') AS paid,
            (SELECT count(*) FROM fraud_flags f WHERE (f.ib_id = m.user_id OR f.client_id = m.user_id) AND f.status = 'open') AS flags
         FROM members m LEFT JOIN members p ON p.user_id = m.parent_id WHERE m.user_id = $1 AND m.tenant = $2",
    )
    .bind(id)
    .bind(t)
    .bind(m0)
    .fetch_optional(&st.pool)
    .await?
    .ok_or(ApiError::NotFound)?;
    let settings = db::settings(&st.pool, t).await?;
    let depth = settings.tiers.len() as i32;
    let mut partner = partner_row(&r);
    partner["lotsMonth"] = num(stats::network_lots(&st.pool, t, id, depth, m0, m1).await?);
    partner["lotsPrevMonth"] = num(stats::network_lots(&st.pool, t, id, depth, p0, m0).await?);
    partner["campaign"] = json!(r.get::<Option<String>, _>("campaign_raw"));
    // upline chain
    let mut upline = vec![];
    let mut cur: Option<i64> = r.get("parent_id");
    let mut seen = std::collections::HashSet::new();
    while let Some(pid) = cur {
        if !seen.insert(pid) || upline.len() >= 10 {
            break;
        }
        let Some(p) = sqlx::query("SELECT user_id, first_name, last_name, level_key, parent_id FROM members WHERE user_id = $1").bind(pid).fetch_optional(&st.pool).await? else { break };
        upline.push(json!({"id": pid, "name": format!("{} {}", p.get::<String, _>("first_name"), p.get::<String, _>("last_name")), "level": p.get::<String, _>("level_key")}));
        cur = p.get("parent_id");
    }
    let tree = sqlx::query(
        "WITH RECURSIVE down(user_id, parent_id, tier) AS (
            SELECT user_id, parent_id, 1 FROM members WHERE tenant = $1 AND parent_id = $2
            UNION ALL SELECT m.user_id, m.parent_id, d.tier + 1 FROM members m JOIN down d ON m.parent_id = d.user_id WHERE d.tier < $3
         )
         SELECT d.user_id, d.parent_id, d.tier, m.first_name, m.last_name, m.country, m.level_key, m.joined_at, m.first_deposit_at, m.self_referral,
                (SELECT coalesce(sum(x.lots), 0) FROM deals x WHERE x.user_id = d.user_id AND x.qualified AND NOT x.reversed AND x.close_time >= $4) AS lots_month
         FROM down d JOIN members m ON m.user_id = d.user_id ORDER BY d.tier, m.joined_at LIMIT 1000",
    )
    .bind(t)
    .bind(id)
    .bind(depth)
    .bind(m0)
    .fetch_all(&st.pool)
    .await?;
    let comms = sqlx::query(
        "SELECT k.id, k.kind, k.status, k.amount, k.tier, k.symbol, k.lots, k.deal_id, k.client_id, k.created_at, c.first_name, c.last_name
         FROM commissions k LEFT JOIN members c ON c.user_id = k.client_id WHERE k.beneficiary_id = $1 ORDER BY k.created_at DESC LIMIT 25",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    let pays = sqlx::query("SELECT id, batch_id, amount, status, paid_at, created_at, last_error FROM payouts WHERE user_id = $1 ORDER BY created_at DESC LIMIT 20").bind(id).fetch_all(&st.pool).await?;
    let flags = sqlx::query("SELECT id, kind, severity, status, client_id, ib_id, details, created_at FROM fraud_flags WHERE ib_id = $1 OR client_id = $1 ORDER BY created_at DESC LIMIT 20").bind(id).fetch_all(&st.pool).await?;
    let moves = sqlx::query(
        "SELECT r.from_parent, r.to_parent, r.staff, r.reason, r.at, f.first_name || ' ' || f.last_name AS from_name, t.first_name || ' ' || t.last_name AS to_name
         FROM reassignments r LEFT JOIN members f ON f.user_id = r.from_parent LEFT JOIN members t ON t.user_id = r.to_parent WHERE r.user_id = $1 ORDER BY r.at DESC",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    let lh = sqlx::query("SELECT from_key, to_key, reason, month, active_clients, lots, at FROM level_history WHERE user_id = $1 ORDER BY at DESC LIMIT 20").bind(id).fetch_all(&st.pool).await?;
    Ok(Json(json!({
        "partner": partner,
        "upline": upline,
        "tree": tree.iter().map(|x| json!({
            "id": x.get::<i64, _>("user_id"), "parentId": x.get::<Option<i64>, _>("parent_id"), "tier": x.get::<i32, _>("tier"),
            "name": format!("{} {}", x.get::<String, _>("first_name"), x.get::<String, _>("last_name")), "country": x.get::<String, _>("country"),
            "level": x.get::<String, _>("level_key"), "joinedAt": x.get::<DateTime<Utc>, _>("joined_at"), "funded": x.get::<Option<DateTime<Utc>>, _>("first_deposit_at").is_some(),
            "selfReferral": x.get::<Option<String>, _>("self_referral"), "lotsMonth": num(x.get("lots_month")),
        })).collect::<Vec<_>>(),
        "commissions": comms.iter().map(|x| json!({
            "id": x.get::<i64, _>("id"), "kind": x.get::<String, _>("kind"), "status": x.get::<String, _>("status"), "amount": num(x.get("amount")),
            "tier": x.get::<i32, _>("tier"), "symbol": x.get::<Option<String>, _>("symbol"), "lots": num(x.get("lots")), "dealId": x.get::<Option<i64>, _>("deal_id"),
            "clientId": x.get::<i64, _>("client_id"), "clientName": format!("{} {}", x.get::<Option<String>, _>("first_name").unwrap_or_default(), x.get::<Option<String>, _>("last_name").unwrap_or_default()).trim().to_string(),
            "createdAt": x.get::<DateTime<Utc>, _>("created_at"),
        })).collect::<Vec<_>>(),
        "payouts": pays.iter().map(|x| json!({
            "id": x.get::<i64, _>("id"), "batchId": x.get::<i64, _>("batch_id"), "amount": num(x.get("amount")), "status": x.get::<String, _>("status"),
            "paidAt": x.get::<Option<DateTime<Utc>>, _>("paid_at"), "createdAt": x.get::<DateTime<Utc>, _>("created_at"), "lastError": x.get::<Option<String>, _>("last_error"),
        })).collect::<Vec<_>>(),
        "flags": flags.iter().map(flag_json_min).collect::<Vec<_>>(),
        "reassignments": moves.iter().map(|x| json!({
            "fromParent": x.get::<Option<i64>, _>("from_parent"), "toParent": x.get::<Option<i64>, _>("to_parent"), "staff": x.get::<String, _>("staff"),
            "fromName": x.get::<Option<String>, _>("from_name"), "toName": x.get::<Option<String>, _>("to_name"),
            "reason": x.get::<String, _>("reason"), "at": x.get::<DateTime<Utc>, _>("at"),
        })).collect::<Vec<_>>(),
        "levelHistory": lh.iter().map(|x| json!({
            "from": x.get::<Option<String>, _>("from_key"), "to": x.get::<String, _>("to_key"), "reason": x.get::<String, _>("reason"),
            "month": x.get::<Option<String>, _>("month"), "activeClients": x.get::<Option<i64>, _>("active_clients"), "lots": x.get::<Option<D>, _>("lots").map(num), "at": x.get::<DateTime<Utc>, _>("at"),
        })).collect::<Vec<_>>(),
    })))
}

fn flag_json_min(x: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": x.get::<i64, _>("id"), "kind": x.get::<String, _>("kind"), "severity": x.get::<String, _>("severity"), "status": x.get::<String, _>("status"),
        "clientId": x.get::<Option<i64>, _>("client_id"), "ibId": x.get::<Option<i64>, _>("ib_id"),
        "details": x.get::<sqlx::types::Json<Value>, _>("details").0, "createdAt": x.get::<DateTime<Utc>, _>("created_at"),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchPartner {
    level: Option<String>,
    level_locked: Option<bool>,
    status: Option<String>,
    #[serde(default, deserialize_with = "opt_dec")]
    rebate_pct: Option<D>,
    #[serde(default, deserialize_with = "opt_dec")]
    split_pct: Option<D>,
    reason: Option<String>,
}

fn opt_dec<'de, De: serde::Deserializer<'de>>(d: De) -> Result<Option<D>, De::Error> {
    #[derive(Deserialize)]
    struct W(#[serde(deserialize_with = "de_dec")] D);
    Ok(Option::<W>::deserialize(d)?.map(|w| w.0))
}

pub async fn patch_partner(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(r): Json<PatchPartner>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let reason = reason_of(&r.reason)?;
    let settings = db::settings(&st.pool, &s.tenant).await?;
    let levels = db::levels(&st.pool, &s.tenant).await?;
    let before = sqlx::query("SELECT level_key, level_locked, status, rebate_pct, split_pct FROM members WHERE user_id = $1 AND tenant = $2")
        .bind(id)
        .bind(&s.tenant)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    if let Some(l) = &r.level
        && !levels.iter().any(|x| &x.key == l)
    {
        return Err(invalid("level", "Unknown level."));
    }
    if let Some(v) = &r.status
        && !matches!(v.as_str(), "active" | "suspended")
    {
        return Err(invalid("status", "Status must be active or suspended."));
    }
    for (f, v, max) in [("rebatePct", r.rebate_pct, settings.max_rebate_pct), ("splitPct", r.split_pct, settings.max_split_pct)] {
        if let Some(v) = v
            && (v < ZERO || v > max)
        {
            return Err(invalid(f, format!("Must be between 0 and {}%.", max.normalize())));
        }
    }
    let old_level: String = before.get("level_key");
    let mut tx = st.pool.begin().await?;
    sqlx::query(
        "UPDATE members SET level_key = COALESCE($2, level_key), level_since = CASE WHEN $2 IS NOT NULL AND $2 <> level_key THEN now() ELSE level_since END,
            level_locked = COALESCE($3, level_locked), status = COALESCE($4, status), rebate_pct = COALESCE($5, rebate_pct), split_pct = COALESCE($6, split_pct)
         WHERE user_id = $1",
    )
    .bind(id)
    .bind(&r.level)
    .bind(r.level_locked)
    .bind(&r.status)
    .bind(r.rebate_pct)
    .bind(r.split_pct)
    .execute(&mut *tx)
    .await?;
    if let Some(l) = r.level.as_ref().filter(|l| **l != old_level) {
        sqlx::query("INSERT INTO level_history (tenant, user_id, from_key, to_key, reason) VALUES ($1,$2,$3,$4,'admin')").bind(&s.tenant).bind(id).bind(&old_level).bind(l).execute(&mut *tx).await?;
    }
    audit::record(
        &mut *tx,
        &s.tenant,
        &s.actor(),
        "partner.update",
        Some(format!("user:{id}")),
        Some(json!({"level": old_level, "levelLocked": before.get::<bool, _>("level_locked"), "status": before.get::<String, _>("status"),
                    "rebatePct": before.get::<D, _>("rebate_pct").to_string(), "splitPct": before.get::<D, _>("split_pct").to_string()})),
        Some(json!({"level": r.level, "levelLocked": r.level_locked, "status": r.status, "rebatePct": r.rebate_pct.map(|d| d.to_string()), "splitPct": r.split_pct.map(|d| d.to_string())})),
        Some(&reason),
    )
    .await?;
    tx.commit().await?;
    partner(State(st), s, Path(id)).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReassignReq {
    parent_id: Option<i64>,
    reason: Option<String>,
}

/// Moves a client under another IB (or detaches it). Attribution is otherwise permanent (D63). Future deals
/// follow the new tree; commissions already accrued stay where they are.
pub async fn reassign(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(r): Json<ReassignReq>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let reason = reason_of(&r.reason)?;
    let t = s.tenant.as_str();
    let from: Option<i64> = sqlx::query_scalar::<_, Option<i64>>("SELECT parent_id FROM members WHERE user_id = $1 AND tenant = $2").bind(id).bind(t).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    if r.parent_id == from {
        return Err(ApiError::Conflict { code: "no_change", message: "The client is already under that partner.".into() });
    }
    if let Some(p) = r.parent_id {
        if p == id {
            return Err(invalid("parentId", "A client can't be its own partner."));
        }
        let exists: Option<i64> = sqlx::query_scalar("SELECT user_id FROM members WHERE user_id = $1 AND tenant = $2").bind(p).bind(t).fetch_optional(&st.pool).await?;
        if exists.is_none() {
            return Err(invalid("parentId", "Unknown partner."));
        }
        // the new partner must not sit in this client's own downline (no cycles)
        let cycle: Option<i32> = sqlx::query_scalar(
            "WITH RECURSIVE down(user_id) AS (SELECT user_id FROM members WHERE parent_id = $1 UNION SELECT m.user_id FROM members m JOIN down d ON m.parent_id = d.user_id)
             SELECT 1 FROM down WHERE user_id = $2 LIMIT 1",
        )
        .bind(id)
        .bind(p)
        .fetch_optional(&st.pool)
        .await?;
        if cycle.is_some() {
            return Err(invalid("parentId", "That partner is in this client's own network; the move would create a loop."));
        }
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE members SET parent_id = $2, parent_source = 'admin', campaign_id = NULL, self_referral = NULL, abuse_cleared = false WHERE user_id = $1").bind(id).bind(r.parent_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO reassignments (tenant, user_id, from_parent, to_parent, staff, reason) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(t)
        .bind(id)
        .bind(from)
        .bind(r.parent_id)
        .bind(format!("{} ({})", s.name, s.actor().id))
        .bind(&reason)
        .execute(&mut *tx)
        .await?;
    audit::record(&mut *tx, t, &s.actor(), "partner.reassign", Some(format!("user:{id}")), Some(json!({"parentId": from})), Some(json!({"parentId": r.parent_id})), Some(&reason)).await?;
    tx.commit().await?;
    let settings = db::settings(&st.pool, t).await?;
    sync::check_self_referral(&st, t, id, &settings).await?;
    partner(State(st), s, Path(id)).await
}

// ---------------------------------------------------------------- commissions

#[derive(Deserialize)]
pub struct AdminCommQ {
    status: Option<String>,
    kind: Option<String>,
    ib: Option<i64>,
    client: Option<i64>,
    batch: Option<i64>,
    from: Option<String>,
    to: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn commissions(State(st): State<AppState>, s: StaffCtx, Query(q): Query<AdminCommQ>) -> ApiResult<Json<Value>> {
    let (page, limit, offset) = paging(q.page, q.limit, 50, 500);
    let rows = sqlx::query(
        "SELECT k.*, b.first_name AS b_first, b.last_name AS b_last, c.first_name AS c_first, c.last_name AS c_last, count(*) OVER () AS total,
                sum(k.amount) OVER () AS sum_amount
         FROM commissions k LEFT JOIN members b ON b.user_id = k.beneficiary_id LEFT JOIN members c ON c.user_id = k.client_id
         WHERE k.tenant = $1 AND ($2::text IS NULL OR k.status = $2) AND ($3::text IS NULL OR k.kind = $3) AND ($4::bigint IS NULL OR k.beneficiary_id = $4)
           AND ($5::bigint IS NULL OR k.client_id = $5) AND ($6::bigint IS NULL OR k.batch_id = $6)
           AND ($7::timestamptz IS NULL OR k.created_at >= $7) AND ($8::timestamptz IS NULL OR k.created_at < $8)
         ORDER BY k.created_at DESC, k.id DESC LIMIT $9 OFFSET $10",
    )
    .bind(&s.tenant)
    .bind(q.status.as_deref().filter(|x| *x != "all"))
    .bind(q.kind.as_deref().filter(|x| *x != "all"))
    .bind(q.ib)
    .bind(q.client)
    .bind(q.batch)
    .bind(parse_time(&q.from)?)
    .bind(parse_time(&q.to)?)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let sum = rows.first().map(|r| r.get::<D, _>("sum_amount")).unwrap_or(ZERO);
    let nm = |f: Option<String>, l: Option<String>| format!("{} {}", f.unwrap_or_default(), l.unwrap_or_default()).trim().to_string();
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "status": r.get::<String, _>("status"), "amount": num(r.get("amount")),
                "tier": r.get::<i32, _>("tier"), "rate": num(r.get("rate")), "sharePct": num(r.get("share_pct")), "lots": num(r.get("lots")),
                "contracts": num(r.get("contracts")), "symbol": r.get::<Option<String>, _>("symbol"), "symbolGroup": r.get::<Option<String>, _>("symbol_group"), "levelKey": r.get::<Option<String>, _>("level_key"),
                "dealId": r.get::<Option<i64>, _>("deal_id"), "source": r.get::<Option<String>, _>("origin").or_else(|| r.get("deal_source")), "feed": r.get::<Option<String>, _>("deal_source"), "login": r.get::<Option<i64>, _>("login"),
                "beneficiary": {"id": r.get::<i64, _>("beneficiary_id"), "name": nm(r.get("b_first"), r.get("b_last"))},
                "client": {"id": r.get::<i64, _>("client_id"), "name": nm(r.get("c_first"), r.get("c_last"))},
                "batchId": r.get::<Option<i64>, _>("batch_id"), "note": r.get::<Option<String>, _>("note"),
                "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "availableAt": r.get::<DateTime<Utc>, _>("available_at"),
            })
        })
        .collect();
    Ok(Json(json!({"items": items, "page": page, "limit": limit, "total": total, "sum": num(sum)})))
}

#[derive(Deserialize)]
pub struct NoteReq {
    reason: Option<String>,
    note: Option<String>,
    action: Option<String>,
}

pub async fn reject_commission(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(r): Json<NoteReq>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let reason = reason_of(&r.reason)?;
    let mut tx = st.pool.begin().await?;
    let row = sqlx::query("SELECT status, batch_id, amount FROM commissions WHERE id = $1 AND tenant = $2 FOR UPDATE").bind(id).bind(&s.tenant).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    let status: String = row.get("status");
    if status != "pending" || row.get::<Option<i64>, _>("batch_id").is_some() {
        return Err(ApiError::Conflict { code: "invalid_state", message: "Only pending commissions that are not in a batch can be rejected. Reject the batch first.".into() });
    }
    sqlx::query("UPDATE commissions SET status = 'rejected', note = $2, updated_at = now() WHERE id = $1").bind(id).bind(&reason).execute(&mut *tx).await?;
    audit::record(&mut *tx, &s.tenant, &s.actor(), "commission.reject", Some(format!("commission:{id}")), Some(json!({"status": status, "amount": row.get::<D, _>("amount").to_string()})), Some(json!({"status": "rejected"})), Some(&reason)).await?;
    tx.commit().await?;
    Ok(Json(json!({"status": "rejected"})))
}

// ---------------------------------------------------------------- batches

#[derive(Deserialize)]
pub struct BatchesQ {
    status: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn batches(State(st): State<AppState>, s: StaffCtx, Query(q): Query<BatchesQ>) -> ApiResult<Json<Value>> {
    let (page, limit, offset) = paging(q.page, q.limit, 25, 200);
    let rows = sqlx::query(
        "SELECT b.*, count(*) OVER () AS total_rows,
            (SELECT count(*) FROM payouts p WHERE p.batch_id = b.id AND p.status = 'paid') AS paid_n,
            (SELECT count(*) FROM payouts p WHERE p.batch_id = b.id AND p.status = 'transfer_pending') AS pending_n,
            (SELECT count(*) FROM payouts p WHERE p.batch_id = b.id AND p.status = 'failed') AS failed_n
         FROM payout_batches b WHERE b.tenant = $1 AND ($2::text IS NULL OR b.status = $2) ORDER BY b.created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&s.tenant)
    .bind(q.status.as_deref().filter(|x| *x != "all"))
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total_rows")).unwrap_or(0);
    let unbatched = sqlx::query("SELECT coalesce(sum(amount), 0) AS amount, count(DISTINCT beneficiary_id) AS payees FROM commissions WHERE tenant = $1 AND status = 'pending' AND batch_id IS NULL AND available_at <= now()")
        .bind(&s.tenant)
        .fetch_one(&st.pool)
        .await?;
    let settings = db::settings(&st.pool, &s.tenant).await?;
    Ok(Json(json!({
        "items": rows.iter().map(|r| {
            let mut v = payouts::batch_json(r);
            v["transfers"] = json!({"paid": r.get::<i64, _>("paid_n"), "pending": r.get::<i64, _>("pending_n"), "failed": r.get::<i64, _>("failed_n")});
            v
        }).collect::<Vec<_>>(),
        "page": page, "limit": limit, "total": total,
        "unbatched": {"amount": num(unbatched.get("amount")), "payees": unbatched.get::<i64, _>("payees")},
        "schedule": settings.payout.schedule, "minAmount": num(settings.payout.min_amount),
        "nextClose": payouts::next_close(&settings.payout.schedule, settings.payout.weekday, settings.payout.month_day),
    })))
}

pub async fn batch(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let b = sqlx::query("SELECT * FROM payout_batches WHERE id = $1 AND tenant = $2").bind(id).bind(&s.tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let lines = sqlx::query(
        "SELECT p.*, m.first_name, m.last_name, m.email, m.level_key,
            (SELECT count(*) FROM fraud_flags f WHERE (f.ib_id = p.user_id OR f.client_id = p.user_id) AND f.status = 'open') AS flags,
            (SELECT coalesce(sum(k.amount) FILTER (WHERE k.kind IN ('lot','split')), 0) FROM commissions k WHERE k.payout_id = p.id) AS lot_amount,
            (SELECT coalesce(sum(k.amount) FILTER (WHERE k.kind = 'cpa'), 0) FROM commissions k WHERE k.payout_id = p.id) AS cpa_amount,
            (SELECT coalesce(sum(k.amount) FILTER (WHERE k.kind = 'rebate'), 0) FROM commissions k WHERE k.payout_id = p.id) AS rebate_amount,
            (SELECT coalesce(sum(k.amount) FILTER (WHERE k.kind IN ('clawback','adjustment')), 0) FROM commissions k WHERE k.payout_id = p.id) AS adj_amount
         FROM payouts p LEFT JOIN members m ON m.user_id = p.user_id WHERE p.batch_id = $1 ORDER BY p.amount DESC",
    )
    .bind(id)
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({
        "batch": payouts::batch_json(&b),
        "payouts": lines.iter().map(|r| json!({
            "id": r.get::<i64, _>("id"), "userId": r.get::<i64, _>("user_id"),
            "name": format!("{} {}", r.get::<Option<String>, _>("first_name").unwrap_or_default(), r.get::<Option<String>, _>("last_name").unwrap_or_default()).trim().to_string(),
            "email": r.get::<Option<String>, _>("email"), "level": r.get::<Option<String>, _>("level_key"),
            "amount": num(r.get("amount")), "lines": r.get::<i32, _>("lines"), "status": r.get::<String, _>("status"),
            "attempts": r.get::<i32, _>("attempts"), "lastError": r.get::<Option<String>, _>("last_error"), "nextAttemptAt": r.get::<Option<DateTime<Utc>>, _>("next_attempt_at"),
            "walletTxn": r.get::<Option<String>, _>("wallet_txn"), "paidAt": r.get::<Option<DateTime<Utc>>, _>("paid_at"), "openFlags": r.get::<i64, _>("flags"),
            "breakdown": {"lots": num(r.get("lot_amount")), "cpa": num(r.get("cpa_amount")), "rebates": num(r.get("rebate_amount")), "adjustments": num(r.get("adj_amount"))},
        })).collect::<Vec<_>>(),
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateBatchReq {
    period_end: Option<String>,
}

pub async fn create_batch(State(st): State<AppState>, s: StaffCtx, Json(r): Json<CreateBatchReq>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE.iter().chain(ROLES_APPROVE).copied().collect::<Vec<_>>().as_slice())?;
    let end = parse_time(&r.period_end)?.unwrap_or_else(Utc::now).min(Utc::now());
    match payouts::create_batch(&st, &s.tenant, &s.actor(), None, end).await? {
        Some(id) => batch(State(st), s, Path(id)).await,
        None => Err(ApiError::Conflict { code: "nothing_to_pay", message: "No payable commissions above the minimum payout.".into() }),
    }
}

pub async fn approve_batch(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(r): Json<NoteReq>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    payouts::approve(&st, &s.tenant, id, &s.actor(), r.note.as_deref().map(str::trim).filter(|n| !n.is_empty())).await?;
    // first transfer attempt right away so the response already shows paid / pending transfer
    let _ = payouts::transfer_tick(&st).await;
    batch(State(st), s, Path(id)).await
}

pub async fn reject_batch(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(r): Json<NoteReq>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    let reason = reason_of(&r.reason.clone().or(r.note.clone()))?;
    payouts::reject(&st, &s.tenant, id, &s.actor(), &reason).await?;
    batch(State(st), s, Path(id)).await
}

pub async fn retry_batch(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require(ROLES_APPROVE)?;
    payouts::retry(&st, &s.tenant, id, &s.actor()).await?;
    let _ = payouts::transfer_tick(&st).await;
    batch(State(st), s, Path(id)).await
}

// ---------------------------------------------------------------- fraud flags

#[derive(Deserialize)]
pub struct FlagsQ {
    status: Option<String>,
    kind: Option<String>,
    id: Option<i64>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn flags(State(st): State<AppState>, s: StaffCtx, Query(q): Query<FlagsQ>) -> ApiResult<Json<Value>> {
    let (page, limit, offset) = paging(q.page, q.limit, 50, 200);
    let rows = sqlx::query(
        "SELECT f.*, c.first_name AS c_first, c.last_name AS c_last, c.email AS c_email, i.first_name AS i_first, i.last_name AS i_last, i.referral_code AS i_code,
                count(*) OVER () AS total
         FROM fraud_flags f LEFT JOIN members c ON c.user_id = f.client_id LEFT JOIN members i ON i.user_id = f.ib_id
         WHERE f.tenant = $1 AND ($2::text IS NULL OR f.status = $2) AND ($5::text IS NULL OR f.kind LIKE $5 || '%') AND ($6::bigint IS NULL OR f.id = $6)
         ORDER BY (f.status = 'open') DESC, f.created_at DESC LIMIT $3 OFFSET $4",
    )
    .bind(&s.tenant)
    .bind(q.status.as_deref().filter(|x| *x != "all"))
    .bind(limit)
    .bind(offset)
    .bind(q.kind.as_deref().filter(|x| *x != "all" && x.len() <= 40))
    .bind(q.id)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let counts = sqlx::query("SELECT status, count(*) AS n FROM fraud_flags WHERE tenant = $1 GROUP BY status").bind(&s.tenant).fetch_all(&st.pool).await?;
    let nm = |f: Option<String>, l: Option<String>| format!("{} {}", f.unwrap_or_default(), l.unwrap_or_default()).trim().to_string();
    Ok(Json(json!({
        "items": rows.iter().map(|r| {
            let mut v = flag_json_min(r);
            v["client"] = json!({"id": r.get::<Option<i64>, _>("client_id"), "name": nm(r.get("c_first"), r.get("c_last")), "email": r.get::<Option<String>, _>("c_email")});
            v["ib"] = json!({"id": r.get::<Option<i64>, _>("ib_id"), "name": nm(r.get("i_first"), r.get("i_last")), "code": r.get::<Option<String>, _>("i_code")});
            v["resolvedBy"] = json!(r.get::<Option<String>, _>("resolved_by"));
            v["resolvedAt"] = json!(r.get::<Option<DateTime<Utc>>, _>("resolved_at"));
            v["note"] = json!(r.get::<Option<String>, _>("note"));
            v
        }).collect::<Vec<_>>(),
        "page": page, "limit": limit, "total": total,
        "counts": counts.iter().map(|c| (c.get::<String, _>("status"), json!(c.get::<i64, _>("n")))).collect::<serde_json::Map<_, _>>(),
    })))
}

/// `dismiss`: a false positive. A self-referral block is lifted (the client's future deals earn again).
/// `confirm`: abuse confirmed. The client's pending, unbatched commission lines are voided.
pub async fn resolve_flag(State(st): State<AppState>, s: StaffCtx, Path(id): Path<i64>, Json(r): Json<NoteReq>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE)?;
    let action = r.action.as_deref().unwrap_or("");
    if !matches!(action, "dismiss" | "confirm") {
        return Err(invalid("action", "Action must be dismiss or confirm."));
    }
    let note = reason_of(&r.note.clone().or(r.reason.clone()))?;
    let mut tx = st.pool.begin().await?;
    let f = sqlx::query("SELECT kind, status, client_id, ib_id, details FROM fraud_flags WHERE id = $1 AND tenant = $2 FOR UPDATE").bind(id).bind(&s.tenant).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    if f.get::<String, _>("status") != "open" {
        return Err(ApiError::Conflict { code: "invalid_state", message: "This flag is already resolved.".into() });
    }
    let kind: String = f.get("kind");
    let client: Option<i64> = f.get("client_id");
    let status = if action == "dismiss" { "dismissed" } else { "confirmed" };
    sqlx::query("UPDATE fraud_flags SET status = $2, resolved_by = $3, resolved_at = now(), note = $4 WHERE id = $1").bind(id).bind(status).bind(format!("{} ({})", s.name, s.actor().id)).bind(&note).execute(&mut *tx).await?;
    let mut effect = json!({});
    if let Some(c) = client {
        if action == "dismiss" && kind.starts_with("self_referral") {
            sqlx::query("UPDATE fraud_flags SET status = 'dismissed', resolved_by = $3, resolved_at = now(), note = $4 WHERE tenant = $1 AND client_id = $2 AND kind LIKE 'self_referral%' AND status = 'open'")
                .bind(&s.tenant)
                .bind(c)
                .bind(format!("{} ({})", s.name, s.actor().id))
                .bind(&note)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE members SET abuse_cleared = true, self_referral = NULL WHERE user_id = $1").bind(c).execute(&mut *tx).await?;
            effect = json!({"selfReferralCleared": c});
        }
        if action == "confirm" {
            let voided = sqlx::query("UPDATE commissions SET status = 'void', note = $2, updated_at = now() WHERE client_id = $1 AND status = 'pending' AND batch_id IS NULL AND kind <> 'clawback'")
                .bind(c)
                .bind(format!("Fraud flag #{id} confirmed"))
                .execute(&mut *tx)
                .await?
                .rows_affected();
            effect = json!({"voidedLines": voided});
        }
    }
    audit::record(&mut *tx, &s.tenant, &s.actor(), &format!("flag.{action}"), Some(format!("flag:{id}")), Some(json!({"status": "open", "kind": kind})), Some(json!({"status": status, "effect": effect})), Some(&note)).await?;
    tx.commit().await?;
    Ok(Json(json!({"status": status, "effect": effect})))
}

// ---------------------------------------------------------------- audit & jobs

#[derive(Deserialize)]
pub struct AuditQ {
    limit: Option<i64>,
    before: Option<i64>,
    target: Option<String>,
}

pub async fn audit(State(st): State<AppState>, s: StaffCtx, Query(q): Query<AuditQ>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT * FROM audit_log WHERE tenant = $1 AND ($2::bigint IS NULL OR id < $2) AND ($3::text IS NULL OR target = $3) ORDER BY id DESC LIMIT $4",
    )
    .bind(&s.tenant)
    .bind(q.before)
    .bind(q.target.as_deref())
    .bind(q.limit.unwrap_or(50).clamp(1, 200))
    .fetch_all(&st.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(|r| json!({
        "id": r.get::<i64, _>("id"), "actor": r.get::<String, _>("actor"), "actorName": r.get::<Option<String>, _>("actor_name"), "action": r.get::<String, _>("action"),
        "target": r.get::<Option<String>, _>("target"), "before": r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0),
        "after": r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0), "note": r.get::<Option<String>, _>("note"), "at": r.get::<DateTime<Utc>, _>("at"),
    })).collect::<Vec<_>>()})))
}

/// Runs a background job now: `sync`, `deals`, `deposits`, `reversals`, `payouts`, `levels` (re-evaluates the
/// current month so far, for previews and tests).
pub async fn run_job(State(st): State<AppState>, s: StaffCtx, Path(job): Path<String>) -> ApiResult<Json<Value>> {
    s.require(ROLES_WRITE.iter().chain(ROLES_APPROVE).copied().collect::<Vec<_>>().as_slice())?;
    let t = s.tenant.as_str();
    let out = match job.as_str() {
        "sync" => json!({"synced": sync::sync_once(&st).await.map_err(|e| ApiError::Unavailable(format!("Gateway sync failed: {e}")))?}),
        "deals" => json!({"processed": deals::poll_engine(&st, t).await.map_err(|e| ApiError::Unavailable(format!("Engine poll failed: {e}")))?}),
        "deposits" => {
            deals::deposits_tick(&st, t).await.map_err(|e| ApiError::Unavailable(format!("Deposit scan failed: {e}")))?;
            json!({"status": "ok"})
        }
        "reversals" => json!({"reversed": deals::sweep_reversals(&st, t).await.map_err(|e| ApiError::Unavailable(format!("Reversal sweep failed: {e}")))?}),
        "payouts" => {
            let (paid, pending, failed) = payouts::transfer_tick(&st).await?;
            json!({"paid": paid, "pending": pending, "failed": failed})
        }
        "levels" => {
            let (m0, m1) = calc::month_bounds(Utc::now());
            json!({"changed": stats::evaluate(&st, t, m0, m1, &m0.format("%Y-%m").to_string()).await?})
        }
        _ => return Err(ApiError::NotFound),
    };
    audit::record(&st.pool, t, &s.actor(), &format!("job.{job}"), None, None, Some(out.clone()), None).await?;
    Ok(Json(out))
}

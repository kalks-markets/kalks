//! Back Office routes (`/v1/admin/*`): plan builder, challenges with live rule status, rule-event log,
//! manual overrides, payouts approval, certificates, banned-strategy review, news calendar, audit.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use super::{Body, Staff, page};
use crate::certs;
use crate::error::{ApiError, ApiResult};
use crate::money::{D, jnum_opt, num};
use crate::ops::{self, App};
use crate::plans::{self, Plan};
use crate::store;

fn need_reason(reason: &str) -> ApiResult<()> {
    if reason.trim().is_empty() || reason.len() > 200 {
        return Err(ApiError::Validation { field: "reason", message: "A reason is required".into() });
    }
    Ok(())
}

pub async fn overview(State(app): State<App>, s: Staff) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let r = sqlx::query(
        "SELECT
           (SELECT count(*) FROM challenges WHERE tenant = $1 AND status = 'active') AS active,
           (SELECT count(*) FROM challenges WHERE tenant = $1 AND status = 'funded') AS funded,
           (SELECT count(*) FROM challenges WHERE tenant = $1 AND status = 'failed') AS failed,
           (SELECT count(*) FROM phase_accounts WHERE tenant = $1 AND status = 'passed') AS phases_passed,
           (SELECT count(*) FROM phase_accounts WHERE tenant = $1 AND status IN ('passed','failed') AND NOT funded) AS phases_ended,
           (SELECT COALESCE(sum(fee), 0) FROM challenges WHERE tenant = $1 AND status NOT IN ('pending_payment','payment_failed') AND created_at > now() - interval '30 days') AS fees_30d,
           (SELECT count(*) FROM challenges WHERE tenant = $1 AND status NOT IN ('pending_payment','payment_failed') AND created_at > now() - interval '30 days') AS sold_30d,
           (SELECT COALESCE(sum(trader_amount + fee_refund), 0) FROM payouts WHERE tenant = $1 AND status = 'paid' AND decided_at > now() - interval '30 days') AS paid_30d,
           (SELECT count(*) FROM payouts WHERE tenant = $1 AND status IN ('pending','awaiting_second_approval','failed')) AS payouts_pending,
           (SELECT COALESCE(sum(trader_amount + fee_refund), 0) FROM payouts WHERE tenant = $1 AND status IN ('pending','awaiting_second_approval','approved','failed')) AS payouts_pending_amount,
           (SELECT count(*) FROM strategy_flags WHERE tenant = $1 AND status = 'open') AS flags_open,
           (SELECT count(*) FROM rule_events WHERE tenant = $1 AND severity = 'breach' AND at > now() - interval '24 hours') AS breaches_24h,
           (SELECT COALESCE(sum(initial_balance), 0) FROM phase_accounts WHERE tenant = $1 AND funded AND status = 'active') AS funded_capital",
    )
    .bind(&s.tenant)
    .fetch_one(&app.pool)
    .await?;
    let passed: i64 = r.get("phases_passed");
    let ended: i64 = r.get("phases_ended");
    let breaches = sqlx::query("SELECT rule, count(*) AS n FROM rule_events WHERE tenant = $1 AND severity = 'breach' AND at > now() - interval '30 days' GROUP BY rule ORDER BY n DESC")
        .bind(&s.tenant)
        .fetch_all(&app.pool)
        .await?;
    Ok(Json(json!({
        "activeChallenges": r.get::<i64, _>("active"), "funded": r.get::<i64, _>("funded"), "failed": r.get::<i64, _>("failed"),
        "passRate": if ended == 0 { json!(null) } else { num((D::from(passed) * D::from(100) / D::from(ended)).round_dp(1)) },
        "fees30d": num(r.get("fees_30d")), "sold30d": r.get::<i64, _>("sold_30d"), "paid30d": num(r.get("paid_30d")),
        "payoutsPending": r.get::<i64, _>("payouts_pending"), "payoutsPendingAmount": num(r.get("payouts_pending_amount")),
        "flagsOpen": r.get::<i64, _>("flags_open"), "breaches24h": r.get::<i64, _>("breaches_24h"), "fundedCapital": num(r.get("funded_capital")),
        "breachReasons": breaches.iter().map(|b| json!({"rule": b.get::<String, _>("rule"), "count": b.get::<i64, _>("n")})).collect::<Vec<_>>(),
    })))
}

/* ---------------- plans ---------------- */

pub async fn plans(State(app): State<App>, s: Staff) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let list = plans::list(&app.pool, &s.tenant, true).await?;
    let stats = sqlx::query(
        "SELECT plan_id,
                count(*) FILTER (WHERE status IN ('active','funded','provisioning')) AS active,
                count(*) FILTER (WHERE created_at > now() - interval '30 days' AND status NOT IN ('pending_payment','payment_failed')) AS sold_30d,
                COALESCE(sum(fee) FILTER (WHERE created_at > now() - interval '30 days' AND status NOT IN ('pending_payment','payment_failed')), 0) AS revenue_30d,
                count(*) FILTER (WHERE status = 'funded') AS funded,
                count(*) FILTER (WHERE status = 'failed') AS failed
         FROM challenges WHERE tenant = $1 GROUP BY plan_id",
    )
    .bind(&s.tenant)
    .fetch_all(&app.pool)
    .await?;
    let out: Vec<Value> = list
        .iter()
        .map(|p| {
            let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
            let st = stats.iter().find(|r| r.get::<String, _>("plan_id") == p.id);
            let (funded, failed) = st.map(|r| (r.get::<i64, _>("funded"), r.get::<i64, _>("failed"))).unwrap_or((0, 0));
            v["stats"] = json!({
                "active": st.map(|r| r.get::<i64, _>("active")).unwrap_or(0),
                "sold30d": st.map(|r| r.get::<i64, _>("sold_30d")).unwrap_or(0),
                "revenue30d": num(st.map(|r| r.get::<D, _>("revenue_30d")).unwrap_or_default()),
                "passRate": if funded + failed == 0 { json!(null) } else { num((D::from(funded) * D::from(100) / D::from(funded + failed)).round_dp(1)) },
            });
            v
        })
        .collect();
    Ok(Json(json!({"plans": out})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanWrite {
    #[serde(flatten)]
    plan: Plan,
    #[serde(default)]
    reason: String,
}

async fn check_group(app: &App, tenant: &str, p: &Plan) -> ApiResult<()> {
    // the engine must know the group and offer every leverage the plan sells
    if let Ok(groups) = app.engine.groups(tenant).await {
        let g = groups.iter().find(|g| g["code"].as_str() == Some(p.group.as_str())).ok_or(ApiError::Validation { field: "group", message: format!("The trading engine has no group '{}'", p.group) })?;
        let levs: Vec<u64> = g["leverages"].as_array().map(|a| a.iter().filter_map(Value::as_u64).collect()).unwrap_or_default();
        if let Some(bad) = p.sizes.iter().find(|s| !levs.contains(&(s.leverage as u64))) {
            return Err(ApiError::Validation { field: "sizes", message: format!("Leverage 1:{} isn't offered by group '{}' ({:?})", bad.leverage, p.group, levs) });
        }
        if g["enabled"].as_bool() == Some(false) {
            return Err(ApiError::Validation { field: "group", message: format!("Group '{}' is disabled in the trading engine", p.group) });
        }
    }
    Ok(())
}

pub async fn create_plan(State(app): State<App>, s: Staff, Body(w): Body<PlanWrite>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    w.plan.validate()?;
    check_group(&app, &s.tenant, &w.plan).await?;
    let saved = plans::save(&app.pool, &s.tenant, &w.plan, &s.actor.name, true).await?;
    store::audit(&app.pool, &s.tenant, &s.actor, "plan.create", "plan", &saved.id, None, Some(serde_json::to_value(&saved)?), Some(&w.reason), None).await;
    Ok(Json(json!({"plan": saved})))
}

pub async fn update_plan(State(app): State<App>, s: Staff, Path(id): Path<String>, Body(mut w): Body<PlanWrite>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    w.plan.id = id.clone();
    w.plan.validate()?;
    check_group(&app, &s.tenant, &w.plan).await?;
    let before = plans::get(&app.pool, &s.tenant, &id).await?;
    let saved = plans::save(&app.pool, &s.tenant, &w.plan, &s.actor.name, false).await?;
    store::audit(&app.pool, &s.tenant, &s.actor, "plan.update", "plan", &id, Some(serde_json::to_value(&before)?), Some(serde_json::to_value(&saved)?), Some(&w.reason), None).await;
    Ok(Json(json!({"plan": saved})))
}

#[derive(Deserialize)]
pub struct StatusBody {
    status: String,
    #[serde(default)]
    reason: String,
}

pub async fn plan_status(State(app): State<App>, s: Staff, Path(id): Path<String>, Body(b): Body<StatusBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    need_reason(&b.reason)?;
    if !["draft", "active", "paused", "archived"].contains(&b.status.as_str()) {
        return Err(ApiError::Validation { field: "status", message: "status must be draft, active, paused or archived".into() });
    }
    let before = plans::get(&app.pool, &s.tenant, &id).await?;
    let saved = plans::set_status(&app.pool, &s.tenant, &id, &b.status, &s.actor.name).await?;
    store::audit(&app.pool, &s.tenant, &s.actor, "plan.status", "plan", &id, Some(json!({"status": before.status})), Some(json!({"status": saved.status})), Some(&b.reason), None).await;
    Ok(Json(json!({"plan": saved})))
}

pub async fn engine_groups(State(app): State<App>, s: Staff) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let groups = app.engine.groups(&s.tenant).await?;
    Ok(Json(json!({"groups": groups.iter().map(|g| json!({"code": g["code"], "name": g["name"], "leverages": g["leverages"], "enabled": g["enabled"], "maxAccountsPerUser": g["maxAccountsPerUser"]})).collect::<Vec<_>>()})))
}

/* ---------------- challenges ---------------- */

#[derive(Deserialize)]
pub struct ListQ {
    status: Option<String>,
    plan: Option<String>,
    q: Option<String>,
    user_id: Option<i64>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn challenges(State(app): State<App>, s: Staff, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let (page, limit) = page(q.page, q.limit, 200);
    let search = q.q.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(|x| format!("%{}%", x.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {cols}, count(*) OVER () AS total FROM challenges c
         WHERE c.tenant = $1 AND ($2::text IS NULL OR c.status = $2) AND ($3::text IS NULL OR c.plan_id = $3) AND ($4::bigint IS NULL OR c.user_id = $4)
           AND ($5::text IS NULL OR c.trader_name ILIKE $5 OR c.user_id::text LIKE $5 OR c.id::text LIKE $5
                OR EXISTS (SELECT 1 FROM phase_accounts p WHERE p.challenge_id = c.id AND p.login::text LIKE $5))
         ORDER BY c.id DESC OFFSET $6 LIMIT $7",
        cols = store::CHALLENGE_COLS.split(", ").map(|c| format!("c.{}", c.trim())).collect::<Vec<_>>().join(", ")
    )))
    .bind(&s.tenant)
    .bind(&q.status)
    .bind(&q.plan)
    .bind(q.user_id)
    .bind(&search)
    .bind((page - 1) * limit)
    .bind(limit)
    .fetch_all(&app.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let mut items = vec![];
    for r in &rows {
        let c = store::challenge_from_row(r);
        let phases = store::phases_of(&app.pool, c.id).await?;
        let mut v = store::challenge_json(&c, &phases);
        v["flags"] = json!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM strategy_flags WHERE challenge_id = $1 AND status = 'open'").bind(c.id).fetch_one(&app.pool).await?);
        items.push(v);
    }
    Ok(Json(json!({"items": items, "page": page, "limit": limit, "total": total})))
}

pub async fn challenge(State(app): State<App>, s: Staff, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let c = store::challenge(&app.pool, id).await?.filter(|c| c.tenant == s.tenant).ok_or_else(|| ApiError::NotFound("Challenge not found".into()))?;
    let phases = store::phases_of(&app.pool, c.id).await?;
    let mut v = store::challenge_json(&c, &phases);
    v["events"] = json!(super::client::events_of(&app, c.id, 200).await?);
    let flags = sqlx::query("SELECT * FROM strategy_flags WHERE challenge_id = $1 ORDER BY id DESC").bind(c.id).fetch_all(&app.pool).await?;
    v["flags"] = json!(flags.iter().map(ops::flag_json).collect::<Vec<_>>());
    let pays = sqlx::query("SELECT p.*, c.plan_name, c.size, c.trader_name FROM payouts p JOIN challenges c ON c.id = p.challenge_id WHERE p.challenge_id = $1 ORDER BY p.id DESC").bind(c.id).fetch_all(&app.pool).await?;
    let threshold = ops::four_eyes_threshold(&app.pool, &s.tenant).await?;
    v["payouts"] = json!(pays.iter().map(ops::payout_row_json).map(|mut p| {
        ops::with_threshold(&mut p, threshold);
        p
    }).collect::<Vec<_>>());
    let audit = sqlx::query(
        "SELECT * FROM audit_log WHERE tenant = $1 AND ((entity = 'challenge' AND entity_id = $2) OR (entity = 'phase_account' AND entity_id = ANY($3))) ORDER BY id DESC LIMIT 100",
    )
    .bind(&s.tenant)
    .bind(c.id.to_string())
    .bind(phases.iter().map(|p| p.id.to_string()).collect::<Vec<_>>())
    .fetch_all(&app.pool)
    .await?;
    v["audit"] = json!(audit.iter().map(audit_json).collect::<Vec<_>>());
    if let Some(a) = phases.iter().find(|p| p.funded && p.status == "active") {
        v["payout"] = ops::payout_quote(&app, &c, a).await?;
    }
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct OverrideBody {
    action: String,
    #[serde(default)]
    reason: String,
    #[serde(default)]
    note: String,
}

pub async fn override_challenge(State(app): State<App>, s: Staff, Path(id): Path<i64>, Body(b): Body<OverrideBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    need_reason(&b.reason)?;
    Ok(Json(ops::override_challenge(&app, &s.tenant, id, &b.action, &s.actor, b.reason.trim(), b.note.trim()).await?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaleBody {
    #[serde(default, with = "jnum_opt")]
    to_size: Option<D>,
    #[serde(default)]
    reason: String,
}

pub async fn scale(State(app): State<App>, s: Staff, Path(id): Path<i64>, Body(b): Body<ScaleBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    need_reason(&b.reason)?;
    let a = store::phase(&app.pool, id).await?.filter(|a| a.tenant == s.tenant).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    let c = store::challenge(&app.pool, a.challenge_id).await?.unwrap();
    let to = match b.to_size {
        Some(t) => t,
        None => crate::money::r2(a.initial_balance + crate::money::pct_of(a.initial_balance, c.rules.scaling_increase)),
    };
    if to <= a.initial_balance || (c.rules.scaling_cap > D::ZERO && to > c.rules.scaling_cap) {
        return Err(ApiError::Validation { field: "toSize", message: "The new size must be above the current size and within the plan's cap".into() });
    }
    let r = ops::maybe_scale(&app, &c, &a, Some(to), &s.actor, Some(b.reason.trim())).await?;
    Ok(Json(json!({"scaledTo": r.map(num)})))
}

/* ---------------- rule events ---------------- */

#[derive(Deserialize)]
pub struct EventsQ {
    severity: Option<String>,
    rule: Option<String>,
    login: Option<i64>,
    limit: Option<i64>,
    before: Option<i64>,
}

pub async fn events(State(app): State<App>, s: Staff, Query(q): Query<EventsQ>) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let rows = sqlx::query(
        "SELECT e.id, e.account_id, e.login, e.rule, e.severity, e.at, e.equity, e.balance, e.threshold, e.message, e.details, e.challenge_id, e.user_id,
                c.trader_name, c.plan_name, c.size, p.phase_name
         FROM rule_events e JOIN challenges c ON c.id = e.challenge_id JOIN phase_accounts p ON p.id = e.account_id
         WHERE e.tenant = $1 AND ($2::text IS NULL OR e.severity = $2) AND ($3::text IS NULL OR e.rule = $3) AND ($4::bigint IS NULL OR e.login = $4)
           AND ($5::bigint IS NULL OR e.id < $5)
         ORDER BY e.id DESC LIMIT $6",
    )
    .bind(&s.tenant)
    .bind(&q.severity)
    .bind(&q.rule)
    .bind(q.login)
    .bind(q.before)
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&app.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = super::client::event_json(r);
            v["challengeId"] = json!(r.get::<i64, _>("challenge_id"));
            v["userId"] = json!(r.get::<i64, _>("user_id"));
            v["traderName"] = json!(r.get::<String, _>("trader_name"));
            v["planName"] = json!(r.get::<String, _>("plan_name"));
            v["size"] = num(r.get("size"));
            v["phase"] = json!(r.get::<String, _>("phase_name"));
            v
        })
        .collect();
    Ok(Json(json!({"events": items})))
}

/* ---------------- payouts ---------------- */

#[derive(Deserialize)]
pub struct PayoutsQ {
    status: Option<String>,
    limit: Option<i64>,
}

pub async fn payouts(State(app): State<App>, s: Staff, Query(q): Query<PayoutsQ>) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let rows = sqlx::query(
        "SELECT p.*, c.plan_name, c.size, c.trader_name FROM payouts p JOIN challenges c ON c.id = p.challenge_id
         WHERE p.tenant = $1 AND ($2::text IS NULL OR p.status = $2) ORDER BY (p.status IN ('pending','awaiting_second_approval','failed','approved')) DESC, p.requested_at DESC LIMIT $3",
    )
    .bind(&s.tenant)
    .bind(&q.status)
    .bind(q.limit.unwrap_or(200).clamp(1, 500))
    .fetch_all(&app.pool)
    .await?;
    let threshold = ops::four_eyes_threshold(&app.pool, &s.tenant).await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut p = ops::payout_row_json(r);
            ops::with_threshold(&mut p, threshold);
            p
        })
        .collect();
    Ok(Json(json!({"payouts": items, "fourEyesThreshold": num(threshold)})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecideBody {
    #[serde(default)]
    note: String,
    /// The client's KYC status as the admin BFF read it from the gateway (used when the prop service has no
    /// gateway DB connection).
    #[serde(default)]
    kyc_status: Option<String>,
}

pub async fn approve_payout(State(app): State<App>, s: Staff, Path(id): Path<i64>, Body(b): Body<DecideBody>) -> ApiResult<Json<Value>> {
    s.require("prop.approve")?;
    let note = Some(b.note.trim()).filter(|n| !n.is_empty());
    Ok(Json(json!({"payout": ops::approve_payout(&app, &s.tenant, id, &s.actor, note, b.kyc_status.as_deref()).await?})))
}

pub async fn reject_payout(State(app): State<App>, s: Staff, Path(id): Path<i64>, Body(b): Body<DecideBody>) -> ApiResult<Json<Value>> {
    s.require("prop.approve")?;
    if b.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "Tell the trader why the payout is rejected".into() });
    }
    Ok(Json(json!({"payout": ops::reject_payout(&app, &s.tenant, id, &s.actor, b.note.trim()).await?})))
}

/* ---------------- settings (four-eyes) ---------------- */

pub async fn settings(State(app): State<App>, s: Staff) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    Ok(Json(json!({"settings": ops::settings_json(&app.pool, &s.tenant).await?})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBody {
    #[serde(default, with = "jnum_opt")]
    four_eyes_threshold: Option<D>,
    #[serde(default)]
    reason: String,
}

/// `PUT /v1/admin/settings` `{fourEyesThreshold, reason}` (prop.write; audited `settings.four_eyes_threshold`).
pub async fn update_settings(State(app): State<App>, s: Staff, Body(b): Body<SettingsBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    need_reason(&b.reason)?;
    let t = b.four_eyes_threshold.ok_or(ApiError::Validation { field: "fourEyesThreshold", message: "Enter a threshold in USD (0 = every payout)".into() })?;
    Ok(Json(json!({"settings": ops::set_four_eyes_threshold(&app, &s.tenant, &s.actor, t, b.reason.trim()).await?})))
}

/* ---------------- certificates ---------------- */

pub async fn certificates(State(app): State<App>, s: Staff) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let list = certs::list(&app.pool, &s.tenant, None, 500).await?;
    Ok(Json(json!({"certificates": list.iter().map(|c| certs::json(c, &app.cfg.verify_base_url)).collect::<Vec<_>>()})))
}

#[derive(Deserialize)]
pub struct ReasonBody {
    #[serde(default)]
    reason: String,
}

pub async fn revoke_certificate(State(app): State<App>, s: Staff, Path(code): Path<String>, Body(b): Body<ReasonBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    need_reason(&b.reason)?;
    let n = sqlx::query("UPDATE certificates SET revoked = true WHERE tenant = $1 AND code = $2 AND NOT revoked").bind(&s.tenant).bind(&code).execute(&app.pool).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound("Certificate not found or already revoked".into()));
    }
    store::audit(&app.pool, &s.tenant, &s.actor, "certificate.revoke", "certificate", &code, None, None, Some(&b.reason), None).await;
    let c = certs::get(&app.pool, &code).await?.unwrap();
    Ok(Json(json!({"certificate": certs::json(&c, &app.cfg.verify_base_url)})))
}

/* ---------------- banned-strategy flags ---------------- */

#[derive(Deserialize)]
pub struct FlagsQ {
    status: Option<String>,
}

pub async fn flags(State(app): State<App>, s: Staff, Query(q): Query<FlagsQ>) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let rows = sqlx::query(
        "SELECT f.*, c.trader_name, c.plan_name, p.phase_name, p.status AS account_status FROM strategy_flags f
         JOIN challenges c ON c.id = f.challenge_id JOIN phase_accounts p ON p.id = f.account_id
         WHERE f.tenant = $1 AND ($2::text IS NULL OR f.status = $2) ORDER BY (f.status = 'open') DESC, f.updated_at DESC LIMIT 500",
    )
    .bind(&s.tenant)
    .bind(&q.status)
    .fetch_all(&app.pool)
    .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut v = ops::flag_json(r);
            v["traderName"] = json!(r.get::<String, _>("trader_name"));
            v["planName"] = json!(r.get::<String, _>("plan_name"));
            v["phase"] = json!(r.get::<String, _>("phase_name"));
            v["accountStatus"] = json!(r.get::<String, _>("account_status"));
            v
        })
        .collect();
    Ok(Json(json!({"flags": items})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewBody {
    decision: String,
    #[serde(default)]
    fail_account: bool,
    #[serde(default)]
    note: String,
}

pub async fn review_flag(State(app): State<App>, s: Staff, Path(id): Path<i64>, Body(b): Body<ReviewBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    if b.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "A review note is required".into() });
    }
    Ok(Json(json!({"flag": ops::review_flag(&app, &s.tenant, id, &b.decision, b.fail_account, &s.actor, b.note.trim()).await?})))
}

/* ---------------- news calendar ---------------- */

pub async fn news(State(app): State<App>, s: Staff) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let rows = sqlx::query("SELECT id, at, title, currency, impact, symbols, created_by FROM news_events WHERE tenant = $1 AND at > now() - interval '30 days' ORDER BY at LIMIT 500")
        .bind(&s.tenant)
        .fetch_all(&app.pool)
        .await?;
    Ok(Json(json!({"events": rows.iter().map(|r| json!({
        "id": r.get::<i64, _>("id"), "at": r.get::<DateTime<Utc>, _>("at"), "title": r.get::<String, _>("title"), "currency": r.get::<String, _>("currency"),
        "impact": r.get::<String, _>("impact"), "symbols": r.get::<Vec<String>, _>("symbols"), "createdBy": r.get::<Option<String>, _>("created_by"),
    })).collect::<Vec<_>>()})))
}

#[derive(Deserialize)]
pub struct NewsBody {
    at: DateTime<Utc>,
    title: String,
    currency: String,
    #[serde(default = "high")]
    impact: String,
    #[serde(default)]
    symbols: Vec<String>,
}

fn high() -> String {
    "high".into()
}

pub async fn create_news(State(app): State<App>, s: Staff, Body(b): Body<NewsBody>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    let ccy = b.currency.trim().to_ascii_uppercase();
    if ccy.len() != 3 || !ccy.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(ApiError::Validation { field: "currency", message: "Currency must be a 3-letter code".into() });
    }
    if b.title.trim().is_empty() || b.title.len() > 120 {
        return Err(ApiError::Validation { field: "title", message: "Title is required (max 120 characters)".into() });
    }
    if !["high", "medium", "low"].contains(&b.impact.as_str()) {
        return Err(ApiError::Validation { field: "impact", message: "impact must be high, medium or low".into() });
    }
    let symbols: Vec<String> = b.symbols.iter().map(|x| x.trim().to_ascii_uppercase()).filter(|x| !x.is_empty() && x.len() <= 20).take(40).collect();
    let id: i64 = sqlx::query_scalar("INSERT INTO news_events (tenant, at, title, currency, impact, symbols, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id")
        .bind(&s.tenant)
        .bind(b.at)
        .bind(b.title.trim())
        .bind(&ccy)
        .bind(&b.impact)
        .bind(&symbols)
        .bind(&s.actor.name)
        .fetch_one(&app.pool)
        .await?;
    store::audit(&app.pool, &s.tenant, &s.actor, "news.create", "news_event", &id.to_string(), None, Some(json!({"at": b.at, "title": b.title, "currency": ccy, "symbols": symbols})), None, None).await;
    Ok(Json(json!({"id": id})))
}

pub async fn delete_news(State(app): State<App>, s: Staff, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s.require("prop.write")?;
    let n = sqlx::query("DELETE FROM news_events WHERE tenant = $1 AND id = $2").bind(&s.tenant).bind(id).execute(&app.pool).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::NotFound("Event not found".into()));
    }
    store::audit(&app.pool, &s.tenant, &s.actor, "news.delete", "news_event", &id.to_string(), None, None, None, None).await;
    Ok(Json(json!({"status": "deleted"})))
}

/* ---------------- audit ---------------- */

pub fn audit_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"), "at": r.get::<DateTime<Utc>, _>("at"), "actor": r.get::<String, _>("actor"), "actorName": r.get::<String, _>("actor_name"),
        "actorRole": r.get::<String, _>("actor_role"), "action": r.get::<String, _>("action"), "entity": r.get::<String, _>("entity"), "entityId": r.get::<String, _>("entity_id"),
        "before": r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0), "after": r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0),
        "reason": r.get::<Option<String>, _>("reason"), "note": r.get::<Option<String>, _>("note"),
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditQ {
    entity: Option<String>,
    entity_id: Option<String>,
    limit: Option<i64>,
    before: Option<i64>,
}

pub async fn audit(State(app): State<App>, s: Staff, Query(q): Query<AuditQ>) -> ApiResult<Json<Value>> {
    s.require("prop.read")?;
    let rows = sqlx::query(
        "SELECT * FROM audit_log WHERE tenant = $1 AND ($2::text IS NULL OR entity = $2) AND ($3::text IS NULL OR entity_id = $3) AND ($4::bigint IS NULL OR id < $4)
         ORDER BY id DESC LIMIT $5",
    )
    .bind(&s.tenant)
    .bind(&q.entity)
    .bind(&q.entity_id)
    .bind(q.before)
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
    .fetch_all(&app.pool)
    .await?;
    Ok(Json(json!({"items": rows.iter().map(audit_json).collect::<Vec<_>>()})))
}

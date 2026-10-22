//! Client Area routes (the CRM BFF forwards the signed-in user).

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use super::{Body, User};
use crate::certs;
use crate::error::{ApiError, ApiResult};
use crate::money::{D, jnum, num};
use crate::ops::{self, App};
use crate::plans;
use crate::store::{self, Challenge};

/// Catalogue: plans on sale (active), sizes enabled.
pub async fn plans(State(app): State<App>, u: User) -> ApiResult<Json<Value>> {
    let mut list = plans::list(&app.pool, &u.tenant, false).await?;
    list.retain(|p| p.status == "active");
    for p in &mut list {
        p.sizes.retain(|s| s.enabled);
        p.updated_by = None;
    }
    Ok(Json(json!({"plans": list})))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseReq {
    plan_id: String,
    #[serde(with = "jnum")]
    size: D,
    idempotency_key: String,
}

pub async fn purchase(State(app): State<App>, u: User, Body(r): Body<PurchaseReq>) -> ApiResult<Json<Value>> {
    let p = ops::purchase(&app, &u.tenant, u.id, &u.name, &r.plan_id, r.size, &r.idempotency_key).await?;
    let phases = store::phases_of(&app.pool, p.challenge.id).await?;
    Ok(Json(json!({"challenge": store::challenge_json(&p.challenge, &phases), "credentials": p.credentials})))
}

async fn own(app: &App, u: &User, id: i64) -> ApiResult<Challenge> {
    store::challenge(&app.pool, id).await?.filter(|c| c.user_id == u.id && c.tenant == u.tenant).ok_or_else(|| ApiError::NotFound("Challenge not found".into()))
}

pub async fn challenges(State(app): State<App>, u: User) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {} FROM challenges WHERE tenant = $1 AND user_id = $2 AND status NOT IN ('payment_failed') ORDER BY id DESC LIMIT 100",
        store::CHALLENGE_COLS
    )))
    .bind(&u.tenant)
    .bind(u.id)
    .fetch_all(&app.pool)
    .await?;
    let mut out = vec![];
    for r in &rows {
        let c = store::challenge_from_row(r);
        let phases = store::phases_of(&app.pool, c.id).await?;
        out.push(store::challenge_json(&c, &phases));
    }
    Ok(Json(json!({"challenges": out})))
}

pub async fn challenge(State(app): State<App>, u: User, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let c = own(&app, &u, id).await?;
    let phases = store::phases_of(&app.pool, c.id).await?;
    let mut v = store::challenge_json(&c, &phases);
    if let Some(a) = phases.iter().find(|p| p.funded && p.status == "active") {
        v["payout"] = ops::payout_quote(&app, &c, a).await?;
    }
    v["events"] = json!(events_of(&app, c.id, 20).await?);
    let certs = certs::list(&app.pool, &u.tenant, Some(u.id), 200).await?;
    v["certificates"] = json!(certs.iter().filter(|x| x.challenge_id == c.id).map(|x| certs::json(x, &app.cfg.verify_base_url)).collect::<Vec<_>>());
    Ok(Json(v))
}

pub async fn events_of(app: &App, challenge_id: i64, limit: i64) -> ApiResult<Vec<Value>> {
    let rows = sqlx::query("SELECT id, account_id, login, rule, severity, at, equity, balance, threshold, message, details FROM rule_events WHERE challenge_id = $1 ORDER BY at DESC, id DESC LIMIT $2")
        .bind(challenge_id)
        .bind(limit)
        .fetch_all(&app.pool)
        .await?;
    Ok(rows.iter().map(event_json).collect())
}

pub fn event_json(r: &sqlx::postgres::PgRow) -> Value {
    let details: sqlx::types::Json<Value> = r.get("details");
    json!({
        "id": r.get::<i64, _>("id"), "accountId": r.get::<i64, _>("account_id"), "login": r.get::<Option<i64>, _>("login"),
        "rule": r.get::<String, _>("rule"), "severity": r.get::<String, _>("severity"), "at": r.get::<chrono::DateTime<chrono::Utc>, _>("at"),
        "equity": r.get::<Option<D>, _>("equity").map(num), "balance": r.get::<Option<D>, _>("balance").map(num),
        "threshold": r.get::<Option<D>, _>("threshold").map(num), "message": r.get::<String, _>("message"), "details": details.0,
    })
}

#[derive(Deserialize)]
pub struct PhaseQ {
    phase: Option<i32>,
    limit: Option<i64>,
}

async fn phase_of(app: &App, c: &Challenge, idx: Option<i32>) -> ApiResult<store::PhaseAccount> {
    let phases = store::phases_of(&app.pool, c.id).await?;
    match idx {
        Some(i) => phases.into_iter().find(|p| p.phase_index == i),
        None => phases.into_iter().last(),
    }
    .ok_or_else(|| ApiError::NotFound("Phase not found".into()))
}

/// Equity curve samples of one phase account (default: the latest).
pub async fn equity(State(app): State<App>, u: User, Path(id): Path<i64>, Query(q): Query<PhaseQ>) -> ApiResult<Json<Value>> {
    let c = own(&app, &u, id).await?;
    let a = phase_of(&app, &c, q.phase).await?;
    let limit = q.limit.unwrap_or(2000).clamp(10, 5000);
    let rows = sqlx::query("SELECT at, balance, equity FROM (SELECT at, balance, equity FROM equity_points WHERE account_id = $1 ORDER BY at DESC LIMIT $2) x ORDER BY at")
        .bind(a.id)
        .bind(limit)
        .fetch_all(&app.pool)
        .await?;
    let points: Vec<Value> = rows.iter().map(|r| json!({"at": r.get::<chrono::DateTime<chrono::Utc>, _>("at"), "balance": num(r.get("balance")), "equity": num(r.get("equity"))})).collect();
    Ok(Json(json!({"accountId": a.id, "login": a.login, "initialBalance": num(a.initial_balance), "points": points})))
}

pub async fn events(State(app): State<App>, u: User, Path(id): Path<i64>, Query(q): Query<PhaseQ>) -> ApiResult<Json<Value>> {
    let c = own(&app, &u, id).await?;
    Ok(Json(json!({"events": events_of(&app, c.id, q.limit.unwrap_or(100).clamp(1, 500)).await?})))
}

/// Closed trades of one phase account (from the engine), newest first.
pub async fn trades(State(app): State<App>, u: User, Path(id): Path<i64>, Query(q): Query<PhaseQ>) -> ApiResult<Json<Value>> {
    let c = own(&app, &u, id).await?;
    let a = phase_of(&app, &c, q.phase).await?;
    let Some(login) = a.login else { return Ok(Json(json!({"trades": []}))) };
    let since = a.started_at.unwrap_or_else(chrono::Utc::now) - chrono::Duration::minutes(1);
    let deals = app.engine.deals(&u.tenant, login, since).await?;
    let trades: Vec<Value> = deals
        .iter()
        .rev()
        .map(|d| {
            json!({
                "ticket": d.ticket, "symbol": d.symbol, "side": d.side, "volume": num(d.volume), "openTime": d.open_time, "closeTime": d.close_time,
                "openPrice": num(d.open_price), "closePrice": num(d.close_price), "profit": num(d.profit),
                "durationSecs": (d.close_time - d.open_time).num_seconds(),
            })
        })
        .collect();
    Ok(Json(json!({"login": login, "trades": trades})))
}

pub async fn request_payout(State(app): State<App>, u: User, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    let p = ops::request_payout(&app, &u.tenant, u.id, id, u.kyc.as_deref()).await?;
    Ok(Json(json!({"payout": ops::client_payout(p)})))
}

pub async fn payouts(State(app): State<App>, u: User) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT p.*, c.plan_name, c.size, c.trader_name FROM payouts p JOIN challenges c ON c.id = p.challenge_id
         WHERE p.tenant = $1 AND p.user_id = $2 ORDER BY p.requested_at DESC LIMIT 200",
    )
    .bind(&u.tenant)
    .bind(u.id)
    .fetch_all(&app.pool)
    .await?;
    // funded accounts and their eligibility, for the request form
    let funded = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {} FROM challenges WHERE tenant = $1 AND user_id = $2 AND status = 'funded' ORDER BY id DESC", store::CHALLENGE_COLS)))
        .bind(&u.tenant)
        .bind(u.id)
        .fetch_all(&app.pool)
        .await?;
    let mut accounts = vec![];
    for r in &funded {
        let c = store::challenge_from_row(r);
        if let Some(a) = store::phases_of(&app.pool, c.id).await?.into_iter().find(|p| p.funded && p.status == "active") {
            let q = ops::payout_quote(&app, &c, &a).await?;
            accounts.push(json!({"challengeId": c.id, "planName": c.plan_name, "size": num(c.size), "login": a.login, "balance": a.balance.map(num), "equity": a.equity.map(num), "quote": q, "refundFee": c.rules.refund_fee, "feeRefunded": c.fee_refunded}));
        }
    }
    let kyc = app.kyc_status(u.id, u.kyc.as_deref()).await;
    Ok(Json(json!({"payouts": rows.iter().map(|r| ops::client_payout(ops::payout_row_json(r))).collect::<Vec<_>>(), "funded": accounts, "kycStatus": kyc})))
}

pub async fn certificates(State(app): State<App>, u: User) -> ApiResult<Json<Value>> {
    let list = certs::list(&app.pool, &u.tenant, Some(u.id), 200).await?;
    Ok(Json(json!({"certificates": list.iter().map(|c| certs::json(c, &app.cfg.verify_base_url)).collect::<Vec<_>>()})))
}

pub async fn notifications(State(app): State<App>, u: User) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT id, challenge_id, kind, title, body, at, read_at FROM notifications WHERE tenant = $1 AND user_id = $2 ORDER BY id DESC LIMIT 50")
        .bind(&u.tenant)
        .bind(u.id)
        .fetch_all(&app.pool)
        .await?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({"id": r.get::<i64, _>("id"), "challengeId": r.get::<Option<i64>, _>("challenge_id"), "kind": r.get::<String, _>("kind"), "title": r.get::<String, _>("title"),
                   "body": r.get::<String, _>("body"), "at": r.get::<chrono::DateTime<chrono::Utc>, _>("at"), "read": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("read_at").is_some()})
        })
        .collect();
    Ok(Json(json!({"notifications": items})))
}

pub async fn notifications_read(State(app): State<App>, u: User) -> ApiResult<Json<Value>> {
    let n = sqlx::query("UPDATE notifications SET read_at = now() WHERE tenant = $1 AND user_id = $2 AND read_at IS NULL").bind(&u.tenant).bind(u.id).execute(&app.pool).await?.rows_affected();
    Ok(Json(json!({"status": "ok", "read": n})))
}

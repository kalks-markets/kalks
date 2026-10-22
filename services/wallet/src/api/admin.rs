//! Back Office routes (staff identity headers from the admin BFF). The `finance.*` key is checked here again from
//! `X-Kalks-Staff-Perms`; without it the role is checked.

use axum::Json;
use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;

use super::{Body, Q, ok, paging};
use crate::audit::{self, Entry};
use crate::chain::ChainId;
use crate::error::{ApiError, ApiResult};
use crate::ledger;
use crate::money::{D, WALLET_DP, de_opt_dec, s};
use crate::ops::{deposits, withdrawals};
use crate::settings;
use crate::state::{AppState, ROLES_APPROVE, ROLES_READ, ROLES_SETTINGS, ROLES_WRITE, StaffCtx};

#[derive(Deserialize, Default)]
pub struct ListQ {
    status: Option<String>,
    chain: Option<String>,
    user_id: Option<i64>,
    q: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
    user_ids: Option<String>,
    min_balance: Option<String>,
}

fn opt<'a>(v: &'a Option<String>) -> Option<&'a str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty() && *s != "all")
}

/* ---------------- summary ---------------- */

pub async fn summary(State(st): State<AppState>, s_ctx: StaffCtx) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let t = s_ctx.ctx.tenant.id;
    let day = withdrawals::server_day_start(Utc::now());
    let r = sqlx::query(
        "SELECT
            (SELECT count(*) FROM deposits WHERE tenant_id = $1 AND status IN ('pending', 'confirming')) AS dep_open,
            (SELECT count(*) FROM deposits WHERE tenant_id = $1 AND status = 'unmatched') AS dep_unmatched,
            (SELECT count(*) FROM deposits WHERE tenant_id = $1 AND status = 'review') AS dep_review,
            (SELECT COALESCE(sum(amount), 0) FROM deposits WHERE tenant_id = $1 AND status = 'credited' AND credited_at >= $2) AS dep_today,
            (SELECT count(*) FROM deposits WHERE tenant_id = $1 AND status = 'credited' AND credited_at >= $2) AS dep_today_n,
            (SELECT count(*) FROM withdrawals WHERE tenant_id = $1 AND status = 'requested') AS wd_requested,
            (SELECT count(*) FROM withdrawals WHERE tenant_id = $1 AND status = 'approved') AS wd_approved,
            (SELECT count(*) FROM withdrawals WHERE tenant_id = $1 AND status = 'paid') AS wd_paid,
            (SELECT COALESCE(sum(amount), 0) FROM withdrawals WHERE tenant_id = $1 AND status IN ('requested', 'approved', 'paid')) AS wd_open_amount,
            (SELECT COALESCE(sum(net_amount), 0) FROM withdrawals WHERE tenant_id = $1 AND status = 'completed' AND completed_at >= $2) AS wd_today,
            (SELECT COALESCE(sum(available + locked), 0) FROM wallet_balances WHERE tenant_id = $1) AS liabilities,
            (SELECT count(*) FROM wallet_balances WHERE tenant_id = $1 AND available + locked > 0) AS funded_wallets,
            (SELECT count(*) FROM trading_transfers WHERE tenant_id = $1 AND status = 'pending') AS tt_pending",
    )
    .bind(t)
    .bind(day)
    .fetch_one(&st.pool)
    .await?;
    let n = |c: &str| r.get::<i64, _>(c);
    let d = |c: &str| s(r.get::<D, _>(c));
    Ok(ok(json!({
        "deposits": {"open": n("dep_open"), "unmatched": n("dep_unmatched"), "review": n("dep_review"), "credited_today": d("dep_today"), "credited_today_count": n("dep_today_n")},
        "withdrawals": {"requested": n("wd_requested"), "approved": n("wd_approved"), "paid": n("wd_paid"), "open_amount": d("wd_open_amount"), "completed_today": d("wd_today")},
        "wallets": {"liabilities": d("liabilities"), "funded": n("funded_wallets")},
        "trading_transfers_pending": n("tt_pending"),
        "generated_at": Utc::now(),
    })))
}

/* ---------------- deposits ---------------- */

pub async fn deposits(State(st): State<AppState>, s_ctx: StaffCtx, Q(q): Q<ListQ>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let (page, limit, offset) = paging(q.page, q.limit, 50, 200);
    let status = opt(&q.status);
    let queue = status == Some("queue");
    let chain = opt(&q.chain).map(|c| ChainId::parse(c).map(|c| c.as_str()).ok_or_else(|| ApiError::BadRequest("Unknown chain".into()))).transpose()?;
    let term = opt(&q.q).map(|t| t.to_ascii_lowercase());
    let rows = sqlx::query(
        "SELECT *, count(*) OVER () AS total FROM deposits
         WHERE tenant_id = $1
           AND ($2::text IS NULL OR status = $2 OR ($2 = 'queue' AND status IN ('unmatched', 'review')))
           AND ($3::text IS NULL OR chain = $3)
           AND ($4::bigint IS NULL OR user_id = $4)
           AND ($5::text IS NULL OR lower(tx_hash) LIKE '%' || $5 || '%' OR lower(from_address) = $5 OR intent_id = $5)
         ORDER BY id DESC LIMIT $6 OFFSET $7",
    )
    .bind(s_ctx.ctx.tenant.id)
    .bind(if queue { Some("queue") } else { status })
    .bind(chain)
    .bind(q.user_id)
    .bind(term)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(ok(json!({"items": rows.iter().map(deposits::deposit_json).collect::<Vec<_>>(), "page": page, "limit": limit, "total": total})))
}

pub async fn deposit(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let t = s_ctx.ctx.tenant.id;
    let r = sqlx::query("SELECT * FROM deposits WHERE id = $1 AND tenant_id = $2").bind(id).bind(t).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Deposit"))?;
    let mut v = deposits::deposit_json(&r);
    let intent = match r.get::<Option<String>, _>("intent_id") {
        Some(iid) => sqlx::query("SELECT * FROM deposit_intents WHERE id = $1").bind(iid).fetch_optional(&st.pool).await?.as_ref().map(deposits::intent_json),
        None => None,
    };
    // other deposits from the same sender (matching hint for the unmatched queue)
    let senders: Vec<Value> = match r.get::<Option<String>, _>("from_address") {
        Some(from) => sqlx::query("SELECT user_id, count(*) AS n, max(created_at) AS last FROM deposits WHERE tenant_id = $1 AND chain = $2 AND from_address = $3 AND user_id IS NOT NULL AND id <> $4 GROUP BY user_id ORDER BY n DESC LIMIT 10")
            .bind(t)
            .bind(r.get::<String, _>("chain"))
            .bind(from)
            .bind(id)
            .fetch_all(&st.pool)
            .await?
            .iter()
            .map(|x| json!({"user_id": x.get::<i64, _>("user_id"), "deposits": x.get::<i64, _>("n"), "last": x.get::<DateTime<Utc>, _>("last")}))
            .collect(),
        None => vec![],
    };
    v["intent"] = json!(intent);
    v["sender_history"] = json!(senders);
    Ok(ok(json!({"deposit": v})))
}

#[derive(Deserialize)]
pub struct AssignBody {
    #[serde(default)]
    user_id: i64,
    #[serde(default)]
    reason: String,
}

pub async fn assign_deposit(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<AssignBody>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.write", ROLES_WRITE)?;
    Ok(ok(json!({"deposit": deposits::assign(&st, &s_ctx, id, b.user_id, &b.reason).await?})))
}

#[derive(Deserialize)]
pub struct ReasonBody {
    #[serde(default)]
    reason: String,
}

pub async fn reject_deposit(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<ReasonBody>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.write", ROLES_WRITE)?;
    Ok(ok(json!({"deposit": deposits::reject(&st, &s_ctx, id, &b.reason).await?})))
}

pub async fn recheck_deposit(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.write", ROLES_WRITE)?;
    Ok(ok(json!({"deposit": deposits::recheck(&st, &s_ctx, id).await?})))
}

/* ---------------- withdrawals ---------------- */

pub async fn withdrawals(State(st): State<AppState>, s_ctx: StaffCtx, Q(q): Q<ListQ>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let (page, limit, offset) = paging(q.page, q.limit, 50, 200);
    let status = opt(&q.status);
    let chain = opt(&q.chain).map(|c| ChainId::parse(c).map(|c| c.as_str()).ok_or_else(|| ApiError::BadRequest("Unknown chain".into()))).transpose()?;
    let term = opt(&q.q).map(|t| t.to_string());
    let rows = sqlx::query(
        "SELECT *, count(*) OVER () AS total FROM withdrawals
         WHERE tenant_id = $1
           AND ($2::text IS NULL OR status = $2 OR ($2 = 'open' AND status IN ('requested', 'approved', 'paid')))
           AND ($3::text IS NULL OR chain = $3)
           AND ($4::bigint IS NULL OR user_id = $4)
           AND ($5::text IS NULL OR to_address = $5 OR payout_tx_hash = lower($5) OR id::text = $5)
         ORDER BY CASE status WHEN 'requested' THEN 0 WHEN 'approved' THEN 1 WHEN 'paid' THEN 2 ELSE 3 END, id DESC
         LIMIT $6 OFFSET $7",
    )
    .bind(s_ctx.ctx.tenant.id)
    .bind(status)
    .bind(chain)
    .bind(q.user_id)
    .bind(term)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(ok(json!({"items": rows.iter().map(withdrawals::withdrawal_json).collect::<Vec<_>>(), "page": page, "limit": limit, "total": total})))
}

pub async fn withdrawal(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let r = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND tenant_id = $2").bind(id).bind(s_ctx.ctx.tenant.id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Withdrawal"))?;
    let risk = withdrawals::risk(&st, &s_ctx.ctx.tenant.slug, &r).await?;
    let history: Vec<Value> = sqlx::query("SELECT * FROM audit_log WHERE tenant_id = $1 AND target_kind = 'withdrawal' AND target_id = $2 ORDER BY id")
        .bind(s_ctx.ctx.tenant.id)
        .bind(id.to_string())
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(audit_json)
        .collect();
    Ok(ok(json!({"withdrawal": withdrawals::withdrawal_json(&r), "risk": risk, "history": history})))
}

#[derive(Deserialize, Default)]
pub struct NoteBody {
    #[serde(default)]
    note: Option<String>,
}

pub async fn approve_withdrawal(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<NoteBody>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.approve", ROLES_APPROVE)?;
    Ok(ok(json!({"withdrawal": withdrawals::approve(&st, &s_ctx, id, b.note.as_deref()).await?})))
}

pub async fn reject_withdrawal(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<ReasonBody>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.approve", ROLES_APPROVE)?;
    Ok(ok(json!({"withdrawal": withdrawals::reject(&st, &s_ctx, id, &b.reason).await?})))
}

#[derive(Deserialize)]
pub struct PaidBody {
    #[serde(default)]
    tx_hash: String,
}

pub async fn paid_withdrawal(State(st): State<AppState>, s_ctx: StaffCtx, Path(id): Path<i64>, Body(b): Body<PaidBody>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.write", ROLES_WRITE)?;
    Ok(ok(json!({"withdrawal": withdrawals::mark_paid(&st, &s_ctx, id, &b.tx_hash).await?})))
}

/* ---------------- wallets ---------------- */

pub async fn wallets(State(st): State<AppState>, s_ctx: StaffCtx, Q(q): Q<ListQ>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let (page, limit, offset) = paging(q.page, q.limit, 50, 200);
    let ids: Option<Vec<i64>> = opt(&q.user_ids).map(|v| v.split(',').filter_map(|x| x.trim().parse::<i64>().ok()).take(200).collect());
    let min = opt(&q.min_balance).and_then(crate::money::parse);
    let rows = sqlx::query(
        "SELECT b.user_id, b.currency, b.available, b.locked, b.updated_at,
                (SELECT COALESCE(sum(amount), 0) FROM deposits d WHERE d.tenant_id = b.tenant_id AND d.user_id = b.user_id AND d.status = 'credited') AS deposited,
                (SELECT COALESCE(sum(amount), 0) FROM withdrawals w WHERE w.tenant_id = b.tenant_id AND w.user_id = b.user_id AND w.status = 'completed') AS withdrawn,
                count(*) OVER () AS total
         FROM wallet_balances b
         WHERE b.tenant_id = $1 AND ($2::bigint[] IS NULL OR b.user_id = ANY($2)) AND ($3::numeric IS NULL OR b.available + b.locked >= $3)
         ORDER BY b.available + b.locked DESC, b.user_id LIMIT $4 OFFSET $5",
    )
    .bind(s_ctx.ctx.tenant.id)
    .bind(ids)
    .bind(min)
    .bind(limit)
    .bind(offset)
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let (a, l): (D, D) = (r.get("available"), r.get("locked"));
            json!({
                "user_id": r.get::<i64, _>("user_id"), "currency": r.get::<String, _>("currency"),
                "available": s(a), "locked": s(l), "total": s(a + l),
                "deposited": s(r.get::<D, _>("deposited")), "withdrawn": s(r.get::<D, _>("withdrawn")),
                "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
            })
        })
        .collect();
    Ok(ok(json!({"items": items, "page": page, "limit": limit, "total": total})))
}

pub async fn wallet(State(st): State<AppState>, s_ctx: StaffCtx, Path(user_id): Path<i64>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let t = s_ctx.ctx.tenant.id;
    let b = ledger::balances(&st.pool, t, user_id).await?;
    let ledger_rows = sqlx::query(
        "SELECT t.id, t.kind, t.currency, t.reference, t.note, t.actor, t.created_at,
                COALESCE(sum(p.amount) FILTER (WHERE p.account_code LIKE '%:available'), 0) AS a,
                COALESCE(sum(p.amount) FILTER (WHERE p.account_code LIKE '%:locked'), 0) AS l
         FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id AND p.user_id = $2
         WHERE t.tenant_id = $1 GROUP BY t.id ORDER BY t.id DESC LIMIT 100",
    )
    .bind(t)
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?;
    let ledger_items: Vec<Value> = ledger_rows
        .iter()
        .map(|r| {
            let (a, l): (D, D) = (r.get("a"), r.get("l"));
            json!({"txn_id": r.get::<i64, _>("id"), "kind": r.get::<String, _>("kind"), "currency": r.get::<String, _>("currency"), "amount": s(a + l),
                   "available_delta": s(a), "locked_delta": s(l), "ref": r.get::<Option<String>, _>("reference"), "note": r.get::<Option<String>, _>("note"),
                   "actor": r.get::<String, _>("actor"), "created_at": r.get::<DateTime<Utc>, _>("created_at")})
        })
        .collect();
    let deps: Vec<Value> = sqlx::query("SELECT * FROM deposits WHERE tenant_id = $1 AND user_id = $2 ORDER BY id DESC LIMIT 50").bind(t).bind(user_id).fetch_all(&st.pool).await?.iter().map(deposits::deposit_json).collect();
    let wds: Vec<Value> = sqlx::query("SELECT * FROM withdrawals WHERE tenant_id = $1 AND user_id = $2 ORDER BY id DESC LIMIT 50").bind(t).bind(user_id).fetch_all(&st.pool).await?.iter().map(withdrawals::withdrawal_json).collect();
    let tts: Vec<Value> = sqlx::query("SELECT * FROM trading_transfers WHERE tenant_id = $1 AND user_id = $2 ORDER BY id DESC LIMIT 50").bind(t).bind(user_id).fetch_all(&st.pool).await?.iter().map(crate::ops::trading::transfer_json).collect();
    Ok(ok(json!({"user_id": user_id, "balances": ledger::balances_json(&b), "ledger": ledger_items, "deposits": deps, "withdrawals": wds, "transfers": tts})))
}

/* ---------------- settings ---------------- */

pub async fn settings(State(st): State<AppState>, s_ctx: StaffCtx) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let t = s_ctx.ctx.tenant.id;
    let chains = settings::chains(&st.pool, t).await?;
    let limits = settings::limits(&st.pool, t).await?;
    let book: Vec<Value> = sqlx::query("SELECT chain, address, role, active, created_at FROM company_addresses WHERE tenant_id = $1 ORDER BY chain, role, created_at DESC")
        .bind(t)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| json!({"chain": r.get::<String, _>("chain"), "address": r.get::<String, _>("address"), "role": r.get::<String, _>("role"), "active": r.get::<bool, _>("active"), "created_at": r.get::<DateTime<Utc>, _>("created_at")}))
        .collect();
    let history: Vec<Value> = sqlx::query("SELECT * FROM audit_log WHERE tenant_id = $1 AND action LIKE 'wallet.settings%' ORDER BY id DESC LIMIT 50").bind(t).fetch_all(&st.pool).await?.iter().map(audit_json).collect();
    let row_meta = sqlx::query("SELECT updated_at, updated_by FROM tenant_settings WHERE tenant_id = $1").bind(t).fetch_optional(&st.pool).await?;
    Ok(ok(json!({
        "limits": limits.json(),
        "limits_updated_at": row_meta.as_ref().map(|r| r.get::<DateTime<Utc>, _>("updated_at")),
        "limits_updated_by": row_meta.as_ref().and_then(|r| r.get::<Option<String>, _>("updated_by")),
        "chains": chains.iter().map(|c| { let mut v = c.json(); v["available"] = json!(st.chains.contains_key(&c.chain)); v }).collect::<Vec<_>>(),
        "missing_chains": ChainId::ALL.iter().filter(|c| !chains.iter().any(|x| x.chain == **c)).map(|c| c.as_str()).collect::<Vec<_>>(),
        "addresses": book,
        "history": history,
    })))
}

#[derive(Deserialize, Default)]
pub struct LimitsPatch {
    #[serde(default, deserialize_with = "de_opt_dec")]
    withdraw_min: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    withdraw_max: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    withdraw_daily_max: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    withdraw_fee_flat: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    withdraw_fee_pct: Option<D>,
    #[serde(default)]
    deposit_cooldown_hours: Option<i32>,
    #[serde(default)]
    intent_ttl_minutes: Option<i32>,
}

#[derive(Deserialize)]
pub struct ChainPatch {
    chain: String,
    #[serde(default)]
    receiving_address: Option<String>,
    /// "" clears (payouts then must come from a receiving address).
    #[serde(default)]
    payout_address: Option<String>,
    #[serde(default)]
    confirmations: Option<i32>,
    #[serde(default)]
    deposits_enabled: Option<bool>,
    #[serde(default)]
    withdrawals_enabled: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    min_deposit: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    withdraw_fee: Option<D>,
}

#[derive(Deserialize)]
pub struct SettingsBody {
    #[serde(default)]
    limits: Option<LimitsPatch>,
    #[serde(default)]
    chains: Option<Vec<ChainPatch>>,
    #[serde(default)]
    reason: String,
}

fn nonneg(field: &'static str, v: Option<D>) -> ApiResult<Option<D>> {
    match v {
        Some(d) if d < D::ZERO || d > crate::money::max_amount() || d.normalize().scale() > WALLET_DP => Err(ApiError::validation(field, "Enter a valid non-negative amount")),
        x => Ok(x),
    }
}

pub async fn update_settings(State(st): State<AppState>, s_ctx: StaffCtx, Body(b): Body<SettingsBody>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.settings", ROLES_SETTINGS)?;
    let reason = b.reason.trim().to_string();
    if reason.len() < 3 {
        return Err(ApiError::validation("reason", "Enter a reason for the change (kept in the audit log)"));
    }
    let t = s_ctx.ctx.tenant.id;
    let mut tx = st.pool.begin().await?;
    if let Some(l) = b.limits {
        let cur = settings::limits(&st.pool, t).await?;
        let next = settings::Limits {
            withdraw_min: nonneg("withdraw_min", l.withdraw_min)?.unwrap_or(cur.withdraw_min),
            withdraw_max: nonneg("withdraw_max", l.withdraw_max)?.unwrap_or(cur.withdraw_max),
            withdraw_daily_max: nonneg("withdraw_daily_max", l.withdraw_daily_max)?.unwrap_or(cur.withdraw_daily_max),
            withdraw_fee_flat: nonneg("withdraw_fee_flat", l.withdraw_fee_flat)?.unwrap_or(cur.withdraw_fee_flat),
            withdraw_fee_pct: nonneg("withdraw_fee_pct", l.withdraw_fee_pct)?.unwrap_or(cur.withdraw_fee_pct),
            deposit_cooldown_hours: l.deposit_cooldown_hours.unwrap_or(cur.deposit_cooldown_hours),
            intent_ttl_minutes: l.intent_ttl_minutes.unwrap_or(cur.intent_ttl_minutes),
        };
        if next.withdraw_min <= D::ZERO {
            return Err(ApiError::validation("withdraw_min", "The minimum withdrawal must be above 0"));
        }
        if next.withdraw_max < next.withdraw_min {
            return Err(ApiError::validation("withdraw_max", "The maximum must be at least the minimum"));
        }
        if next.withdraw_fee_pct >= D::TEN {
            return Err(ApiError::validation("withdraw_fee_pct", "The percentage fee must be below 10%"));
        }
        if !(0..=720).contains(&next.deposit_cooldown_hours) {
            return Err(ApiError::validation("deposit_cooldown_hours", "Use 0 to 720 hours"));
        }
        if !(5..=1440).contains(&next.intent_ttl_minutes) {
            return Err(ApiError::validation("intent_ttl_minutes", "Use 5 to 1440 minutes"));
        }
        sqlx::query(
            "INSERT INTO tenant_settings (tenant_id, withdraw_min, withdraw_max, withdraw_daily_max, withdraw_fee_flat, withdraw_fee_pct, deposit_cooldown_hours, intent_ttl_minutes, updated_at, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now(), $9)
             ON CONFLICT (tenant_id) DO UPDATE SET withdraw_min = $2, withdraw_max = $3, withdraw_daily_max = $4, withdraw_fee_flat = $5, withdraw_fee_pct = $6,
                 deposit_cooldown_hours = $7, intent_ttl_minutes = $8, updated_at = now(), updated_by = $9",
        )
        .bind(t)
        .bind(next.withdraw_min)
        .bind(next.withdraw_max)
        .bind(next.withdraw_daily_max)
        .bind(next.withdraw_fee_flat)
        .bind(next.withdraw_fee_pct)
        .bind(next.deposit_cooldown_hours)
        .bind(next.intent_ttl_minutes)
        .bind(s_ctx.staff.tag())
        .execute(&mut *tx)
        .await?;
        audit::staff(&mut tx, &s_ctx, Entry { action: "wallet.settings.limits", target_kind: "settings", target_id: "limits".into(), reason: Some(&reason), before: Some(cur.json()), after: Some(next.json()) }).await?;
    }
    for c in b.chains.unwrap_or_default() {
        let chain = ChainId::parse(&c.chain).ok_or_else(|| ApiError::validation("chain", "chain must be bsc or tron"))?;
        let cur = settings::chain(&st.pool, t, chain).await?;
        let receiving = match (&c.receiving_address, &cur) {
            (Some(a), _) => chain.normalize_address(a).ok_or_else(|| ApiError::validation("receiving_address", format!("Invalid {} address", chain.network())))?,
            (None, Some(cur)) => cur.receiving_address.clone(),
            (None, None) => return Err(ApiError::validation("receiving_address", format!("Set the receiving address for {}", chain.network()))),
        };
        let payout = match &c.payout_address {
            Some(a) if a.trim().is_empty() => None,
            Some(a) => Some(chain.normalize_address(a).ok_or_else(|| ApiError::validation("payout_address", format!("Invalid {} address", chain.network())))?),
            None => cur.as_ref().and_then(|x| x.payout_address.clone()),
        };
        let confirmations = c.confirmations.or(cur.as_ref().map(|x| x.confirmations)).unwrap_or(match chain {
            ChainId::Bsc => st.cfg.bsc_confirmations,
            ChainId::Tron => st.cfg.tron_confirmations,
        });
        if !(1..=500).contains(&confirmations) {
            return Err(ApiError::validation("confirmations", "Use 1 to 500 confirmations"));
        }
        let next = settings::ChainCfg {
            chain,
            receiving_address: receiving.clone(),
            payout_address: payout.clone(),
            confirmations,
            deposits_enabled: c.deposits_enabled.or(cur.as_ref().map(|x| x.deposits_enabled)).unwrap_or(true),
            withdrawals_enabled: c.withdrawals_enabled.or(cur.as_ref().map(|x| x.withdrawals_enabled)).unwrap_or(true),
            min_deposit: nonneg("min_deposit", c.min_deposit)?.or(cur.as_ref().map(|x| x.min_deposit)).unwrap_or(D::TEN),
            withdraw_fee: nonneg("withdraw_fee", c.withdraw_fee)?.or(cur.as_ref().map(|x| x.withdraw_fee)).unwrap_or(D::ZERO),
        };
        sqlx::query(
            "INSERT INTO chain_settings (tenant_id, chain, receiving_address, payout_address, confirmations, deposits_enabled, withdrawals_enabled, min_deposit, withdraw_fee, updated_at, updated_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), $10)
             ON CONFLICT (tenant_id, chain) DO UPDATE SET receiving_address = $3, payout_address = $4, confirmations = $5, deposits_enabled = $6,
                 withdrawals_enabled = $7, min_deposit = $8, withdraw_fee = $9, updated_at = now(), updated_by = $10",
        )
        .bind(t)
        .bind(chain.as_str())
        .bind(&next.receiving_address)
        .bind(&next.payout_address)
        .bind(next.confirmations)
        .bind(next.deposits_enabled)
        .bind(next.withdrawals_enabled)
        .bind(next.min_deposit)
        .bind(next.withdraw_fee)
        .bind(s_ctx.staff.tag())
        .execute(&mut *tx)
        .await?;
        settings::remember_address(&mut tx, t, chain, &receiving, "receiving").await?;
        if let Some(p) = &payout {
            settings::remember_address(&mut tx, t, chain, p, "payout").await?;
        } else {
            sqlx::query("UPDATE company_addresses SET active = false WHERE tenant_id = $1 AND chain = $2 AND role = 'payout'").bind(t).bind(chain.as_str()).execute(&mut *tx).await?;
        }
        let address_changed = cur.as_ref().map(|x| x.receiving_address != receiving || x.payout_address != payout).unwrap_or(true);
        audit::staff(
            &mut tx,
            &s_ctx,
            Entry {
                action: if address_changed { "wallet.settings.address" } else { "wallet.settings.chain" },
                target_kind: "chain",
                target_id: chain.as_str().into(),
                reason: Some(&reason),
                before: cur.as_ref().map(|x| x.json()),
                after: Some(next.json()),
            },
        )
        .await?;
    }
    tx.commit().await?;
    settings(State(st), s_ctx).await
}

/* ---------------- reconciliation ---------------- */

pub async fn reconciliation(State(st): State<AppState>, s_ctx: StaffCtx) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let t = s_ctx.ctx.tenant.id;
    let pool = &st.pool;
    let sys: BTreeMap<String, D> = sqlx::query_as::<_, (String, D)>("SELECT account_code, sum(amount) FROM ledger_postings WHERE tenant_id = $1 AND user_id IS NULL GROUP BY account_code ORDER BY account_code")
        .bind(t)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
    let get = |k: &str| sys.get(k).copied().unwrap_or_default();
    let liab = sqlx::query("SELECT COALESCE(sum(available), 0) AS a, COALESCE(sum(locked), 0) AS l FROM wallet_balances WHERE tenant_id = $1").bind(t).fetch_one(pool).await?;
    let (avail, locked): (D, D) = (liab.get("a"), liab.get("l"));
    let mut chains = vec![];
    for cfg in settings::chains(pool, t).await? {
        let c = cfg.chain;
        let r = sqlx::query(
            "SELECT
                (SELECT COALESCE(sum(trunc(amount, 6)), 0) FROM deposits WHERE tenant_id = $1 AND chain = $2 AND status = 'credited') AS credited,
                (SELECT count(*) FROM deposits WHERE tenant_id = $1 AND chain = $2 AND status = 'credited') AS credited_n,
                (SELECT COALESCE(sum(amount), 0) FROM deposits WHERE tenant_id = $1 AND chain = $2 AND status IN ('unmatched', 'review')) AS held,
                (SELECT count(*) FROM deposits WHERE tenant_id = $1 AND chain = $2 AND status IN ('unmatched', 'review')) AS held_n,
                (SELECT COALESCE(sum(amount), 0) FROM deposits WHERE tenant_id = $1 AND chain = $2 AND status = 'confirming') AS confirming,
                (SELECT COALESCE(sum(amount), 0) FROM deposits WHERE tenant_id = $1 AND chain = $2 AND status = 'rejected' AND amount IS NOT NULL) AS rejected,
                (SELECT COALESCE(sum(net_amount), 0) FROM withdrawals WHERE tenant_id = $1 AND chain = $2 AND status = 'completed') AS paid_out,
                (SELECT COALESCE(sum(fee), 0) FROM withdrawals WHERE tenant_id = $1 AND chain = $2 AND status = 'completed') AS fees,
                (SELECT count(*) FROM withdrawals WHERE tenant_id = $1 AND chain = $2 AND status = 'completed') AS paid_n,
                (SELECT COALESCE(sum(net_amount), 0) FROM withdrawals WHERE tenant_id = $1 AND chain = $2 AND status = 'paid') AS paying",
        )
        .bind(t)
        .bind(c.as_str())
        .fetch_one(pool)
        .await?;
        let ledger_in = -get(&format!("sys:USDT:deposit:{}", c.as_str()));
        let ledger_out = get(&format!("sys:USDT:withdrawal:{}", c.as_str()));
        let credited: D = r.get("credited");
        let paid_out: D = r.get("paid_out");
        let mut balances = vec![];
        let mut onchain_total = D::ZERO;
        let mut onchain_ok = true;
        let addrs: Vec<(String, String, bool)> = sqlx::query_as("SELECT address, role, active FROM company_addresses WHERE tenant_id = $1 AND chain = $2 ORDER BY active DESC, role").bind(t).bind(c.as_str()).fetch_all(pool).await?;
        for (addr, role, active) in &addrs {
            let bal = match st.chains.get(&c) {
                Some(client) => match tokio::time::timeout(std::time::Duration::from_secs(10), client.usdt_balance(addr)).await {
                    Ok(Ok(b)) => {
                        onchain_total += b;
                        json!(s(b))
                    }
                    _ => {
                        onchain_ok = false;
                        Value::Null
                    }
                },
                None => {
                    onchain_ok = false;
                    Value::Null
                }
            };
            balances.push(json!({"address": addr, "role": role, "active": active, "usdt": bal}));
        }
        // money that should still sit on the company addresses: received (credited + held + confirming) − paid out
        let held: D = r.get("held");
        let confirming: D = r.get("confirming");
        let expected = credited + held + confirming - paid_out - r.get::<D, _>("paying");
        chains.push(json!({
            "chain": c.as_str(),
            "network": c.network(),
            "ledger_received": s(ledger_in),
            "deposits_credited": s(credited),
            "deposits_credited_count": r.get::<i64, _>("credited_n"),
            "deposits_held": s(held),
            "deposits_held_count": r.get::<i64, _>("held_n"),
            "deposits_confirming": s(confirming),
            "deposits_rejected": s(r.get::<D, _>("rejected")),
            "ledger_paid_out": s(ledger_out),
            "withdrawals_paid": s(paid_out),
            "withdrawals_paid_count": r.get::<i64, _>("paid_n"),
            "withdrawals_in_flight": s(r.get::<D, _>("paying")),
            "fees": s(r.get::<D, _>("fees")),
            "ledger_matches_deposits": ledger_in == credited,
            "ledger_matches_withdrawals": ledger_out == paid_out,
            "expected_onchain": s(expected),
            "onchain_balance": if onchain_ok { json!(s(onchain_total)) } else { Value::Null },
            "onchain_difference": if onchain_ok { json!(s(onchain_total - expected)) } else { Value::Null },
            "addresses": balances,
        }));
    }
    // real deposits reported to the IB service (src/ib.rs outbox)
    let ibq = sqlx::query(
        "SELECT count(*) FILTER (WHERE pushed_at IS NOT NULL) AS reported,
                count(*) FILTER (WHERE pushed_at IS NULL AND push_attempts < $2) AS queued,
                count(*) FILTER (WHERE pushed_at IS NULL AND push_attempts >= $2) AS failed
           FROM ib_deposit_events WHERE tenant_id = $1",
    )
    .bind(t)
    .bind(crate::ib::MAX_ATTEMPTS)
    .fetch_one(pool)
    .await?;
    let problems = ledger::verify(pool).await?;
    Ok(ok(json!({
        "liabilities": {"available": s(avail), "locked": s(locked), "total": s(avail + locked)},
        "system_accounts": sys.iter().map(|(k, v)| json!({"account": k, "balance": s(*v)})).collect::<Vec<_>>(),
        "trading_net_in": s(get("sys:USDT:trading")),
        "fees_earned": s(get("sys:USDT:fees")),
        "chains": chains,
        "ib_deposit_events": {"enabled": !st.cfg.ib_url.is_empty(), "reported": ibq.get::<i64, _>("reported"), "queued": ibq.get::<i64, _>("queued"), "failed": ibq.get::<i64, _>("failed")},
        "invariants": {"ok": problems.is_empty(), "problems": problems},
        "generated_at": Utc::now(),
    })))
}

/* ---------------- audit ---------------- */

pub fn audit_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "actor_kind": r.get::<String, _>("actor_kind"),
        "actor_id": r.get::<Option<String>, _>("actor_id"),
        "actor_name": r.get::<Option<String>, _>("actor_name"),
        "actor_role": r.get::<Option<String>, _>("actor_role"),
        "action": r.get::<String, _>("action"),
        "target_kind": r.get::<Option<String>, _>("target_kind"),
        "target_id": r.get::<Option<String>, _>("target_id"),
        "reason": r.get::<Option<String>, _>("reason"),
        "before": r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0),
        "after": r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0),
        "ip": r.get::<Option<String>, _>("ip"),
        "at": r.get::<DateTime<Utc>, _>("at"),
    })
}

pub async fn audit(State(st): State<AppState>, s_ctx: StaffCtx, Q(q): Q<ListQ>) -> ApiResult<Json<Value>> {
    s_ctx.require_perm("finance.read", ROLES_READ)?;
    let (page, limit, offset) = paging(q.page, q.limit, 50, 200);
    let rows = sqlx::query("SELECT *, count(*) OVER () AS total FROM audit_log WHERE tenant_id = $1 ORDER BY id DESC LIMIT $2 OFFSET $3").bind(s_ctx.ctx.tenant.id).bind(limit).bind(offset).fetch_all(&st.pool).await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    Ok(ok(json!({"items": rows.iter().map(audit_json).collect::<Vec<_>>(), "page": page, "limit": limit, "total": total})))
}

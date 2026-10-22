//! Withdrawals (D11, D36): request (locks funds; KYC gate, limits, fees, post-deposit cooldown) → approve /
//! reject (staff) → paid (staff enter the payout hash; the watcher verifies it on chain) → completed.
//! The client can cancel while `requested`.

use chrono::{DateTime, Duration, FixedOffset, TimeZone, Utc};
use serde_json::{Value, json};
use sqlx::postgres::PgRow;
use sqlx::{Postgres, Row, Transaction};

use super::{notify, record_ip};
use crate::audit::{self, Entry};
use crate::chain::{ChainId, TxStatus, confirmations};
use crate::error::{ApiError, ApiResult};
use crate::ledger::{self, Leg, NewTxn, PostError};
use crate::money::{D, ENGINE_DP, ceil_dp, check_amount, s, s_opt};
use crate::settings::{self, ChainCfg, Limits};
use crate::state::{AppState, Ctx, StaffCtx};

/// A payout hash that is still not on chain after this long is rejected (the withdrawal returns to approved).
const PAYOUT_NOT_FOUND_HOURS: i64 = 6;

pub fn withdrawal_json(r: &PgRow) -> Value {
    let chain = ChainId::parse(r.get::<String, _>("chain").as_str()).unwrap_or(ChainId::Tron);
    let hash: Option<String> = r.get("payout_tx_hash");
    json!({
        "id": r.get::<i64, _>("id"),
        "user_id": r.get::<i64, _>("user_id"),
        "chain": chain.as_str(),
        "network": chain.network(),
        "currency": r.get::<String, _>("currency"),
        "to_address": r.get::<String, _>("to_address"),
        "amount": s(r.get::<D, _>("amount")),
        "fee": s(r.get::<D, _>("fee")),
        "net_amount": s(r.get::<D, _>("net_amount")),
        "status": r.get::<String, _>("status"),
        "kyc_status": r.get::<Option<String>, _>("kyc_status"),
        "reason": r.get::<Option<String>, _>("reason"),
        "review_note": r.get::<Option<String>, _>("review_note"),
        "reviewed_by": r.get::<Option<String>, _>("reviewed_by"),
        "reviewed_at": r.get::<Option<DateTime<Utc>>, _>("reviewed_at"),
        "payout_tx_hash": hash,
        "explorer_url": hash.as_deref().map(|h| chain.explorer_tx(h)),
        "payout_error": r.get::<Option<String>, _>("payout_error"),
        "payout_confirmations": r.get::<i32, _>("payout_confirmations"),
        "paid_by": r.get::<Option<String>, _>("paid_by"),
        "paid_at": r.get::<Option<DateTime<Utc>>, _>("paid_at"),
        "completed_at": r.get::<Option<DateTime<Utc>>, _>("completed_at"),
        "ip": r.get::<Option<String>, _>("ip"),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
        "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

/// Client view (no staff identities / internal notes).
pub fn client_json(r: &PgRow) -> Value {
    let mut v = withdrawal_json(r);
    if let Some(o) = v.as_object_mut() {
        for k in ["reviewed_by", "paid_by", "review_note", "ip", "kyc_status", "payout_error"] {
            o.remove(k);
        }
    }
    v
}

/// Withdrawal fee: flat + percentage (rounded up to cents) + the chain's network fee.
pub fn fee_for(limits: &Limits, chain: &ChainCfg, amount: D) -> D {
    limits.withdraw_fee_flat + ceil_dp(amount * limits.withdraw_fee_pct / D::ONE_HUNDRED, ENGINE_DP) + chain.withdraw_fee
}

/// Start of the current server day (GMT+3) in UTC.
pub fn server_day_start(now: DateTime<Utc>) -> DateTime<Utc> {
    let tz = FixedOffset::east_opt(3 * 3600).expect("offset");
    let local = now.with_timezone(&tz).date_naive().and_hms_opt(0, 0, 0).expect("midnight");
    tz.from_local_datetime(&local).single().expect("unambiguous").with_timezone(&Utc)
}

/// Withdrawn today (not rejected / cancelled), for the daily limit.
pub async fn used_today(pool: &sqlx::PgPool, tenant_id: i64, user_id: i64) -> sqlx::Result<D> {
    let v: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM withdrawals WHERE tenant_id = $1 AND user_id = $2 AND status NOT IN ('rejected', 'cancelled') AND created_at >= $3")
        .bind(tenant_id)
        .bind(user_id)
        .bind(server_day_start(Utc::now()))
        .fetch_one(pool)
        .await?;
    Ok(v.unwrap_or_default())
}

/// Until when the post-deposit cooldown blocks withdrawals (None = not blocked).
pub async fn cooldown_until(pool: &sqlx::PgPool, tenant_id: i64, user_id: i64, hours: i32) -> sqlx::Result<Option<DateTime<Utc>>> {
    if hours <= 0 {
        return Ok(None);
    }
    let last: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT max(credited_at) FROM deposits WHERE tenant_id = $1 AND user_id = $2 AND status = 'credited'").bind(tenant_id).bind(user_id).fetch_one(pool).await?;
    Ok(last.map(|t| t + Duration::hours(hours as i64)).filter(|u| *u > Utc::now()))
}

pub struct RequestIn {
    pub user_id: i64,
    pub amount: D,
    pub chain: String,
    pub to_address: String,
    pub idempotency_key: Option<String>,
}

pub async fn request(st: &AppState, ctx: &Ctx, r: RequestIn) -> ApiResult<Value> {
    check_or_request(st, ctx, r, false).await
}

/// Runs every check of a withdrawal request (KYC, limits, cooldown, balance) without locking anything and
/// returns the fee / net amount: the CRM calls it before asking for the step-up code.
pub async fn quote(st: &AppState, ctx: &Ctx, r: RequestIn) -> ApiResult<Value> {
    check_or_request(st, ctx, r, true).await
}

async fn check_or_request(st: &AppState, ctx: &Ctx, r: RequestIn, dry_run: bool) -> ApiResult<Value> {
    let tenant_id = ctx.tenant.id;
    if r.user_id <= 0 {
        return Err(ApiError::validation("user_id", "user_id is required"));
    }
    let chain = ChainId::parse(&r.chain).ok_or_else(|| ApiError::validation("chain", "chain must be bsc or tron"))?;
    let to = chain.normalize_address(&r.to_address).ok_or_else(|| {
        ApiError::validation("to_address", match chain {
            ChainId::Bsc => "Enter a BNB Chain address (0x followed by 40 hexadecimal characters)",
            ChainId::Tron => "Enter a valid TRON address (starts with T, 34 characters)",
        })
    })?;
    let amount = check_amount(r.amount, ENGINE_DP).map_err(|m| ApiError::validation("amount", m))?;
    let key = r.idempotency_key.as_deref().map(str::trim).filter(|k| !k.is_empty()).map(str::to_string);
    if let Some(k) = key.as_ref().filter(|_| !dry_run) {
        if k.len() > 128 {
            return Err(ApiError::validation("idempotency_key", "idempotency_key must be at most 128 characters"));
        }
        if let Some(w) = sqlx::query("SELECT * FROM withdrawals WHERE tenant_id = $1 AND idempotency_key = $2").bind(tenant_id).bind(k).fetch_optional(&st.pool).await? {
            let same = w.get::<i64, _>("user_id") == r.user_id && w.get::<D, _>("amount") == amount && w.get::<String, _>("to_address") == to && w.get::<String, _>("chain") == chain.as_str();
            return if same { Ok(client_json(&w)) } else { Err(ApiError::conflict("idempotency_conflict", "This idempotency key was used for a different withdrawal")) };
        }
    }
    let cfg = settings::chain(&st.pool, tenant_id, chain).await?.filter(|c| c.withdrawals_enabled).ok_or_else(|| ApiError::unprocessable("chain_disabled", format!("Withdrawals on {} are not available", chain.network())))?;
    if settings::company_addresses(&st.pool, tenant_id, chain).await?.contains(&to) {
        return Err(ApiError::validation("to_address", "This is a Kalks deposit address. Enter your own wallet address."));
    }
    let limits = settings::limits(&st.pool, tenant_id).await?;
    if amount < limits.withdraw_min {
        return Err(ApiError::unprocessable("below_minimum", format!("The minimum withdrawal is {} USDT", s(limits.withdraw_min))));
    }
    if amount > limits.withdraw_max {
        return Err(ApiError::unprocessable("above_maximum", format!("The maximum per withdrawal is {} USDT", s(limits.withdraw_max))));
    }
    let fee = fee_for(&limits, &cfg, amount);
    let net = amount - fee;
    if net <= D::ZERO {
        return Err(ApiError::unprocessable("below_minimum", format!("The amount must be above the {} USDT fee", s(fee))));
    }

    // KYC gate (D6)
    let user = st.users.get(&ctx.tenant.slug, r.user_id).await.map_err(|e| {
        tracing::warn!(error = %e, "gateway unavailable for the KYC check");
        ApiError::Coded { status: axum::http::StatusCode::SERVICE_UNAVAILABLE, code: "unavailable", message: "Verification status is unavailable. Please try again shortly.".into() }
    })?;
    let user = user.ok_or_else(|| ApiError::not_found("Client"))?;
    if user.tenant_id != 0 && user.tenant_id != tenant_id {
        return Err(ApiError::not_found("Client"));
    }
    if user.status != "active" {
        return Err(ApiError::Coded { status: axum::http::StatusCode::FORBIDDEN, code: "account_disabled", message: "Your account can't withdraw. Contact support.".into() });
    }
    // withdrawals disabled in the Back Office (client restrictions)
    if user.restrictions.iter().any(|k| k == "withdrawals") {
        return Err(crate::users::restricted("withdrawals"));
    }
    if user.kyc_status != "verified" {
        return Err(ApiError::Coded { status: axum::http::StatusCode::FORBIDDEN, code: "kyc_required", message: "Verify your identity before your first withdrawal.".into() });
    }
    if let Some(until) = cooldown_until(&st.pool, tenant_id, r.user_id, limits.deposit_cooldown_hours).await? {
        return Err(ApiError::unprocessable("deposit_cooldown", format!("Withdrawals open {} hours after your last deposit, at {} UTC", limits.deposit_cooldown_hours, until.format("%Y-%m-%d %H:%M"))));
    }

    if dry_run {
        let used = used_today(&st.pool, tenant_id, r.user_id).await?;
        if used + amount > limits.withdraw_daily_max {
            return Err(ApiError::unprocessable("daily_limit", format!("This exceeds your daily withdrawal limit ({} of {} USDT used today)", s(used), s(limits.withdraw_daily_max))));
        }
        let available = ledger::balances(&st.pool, tenant_id, r.user_id).await?.into_iter().find(|b| b.0 == "USDT").map(|b| b.1).unwrap_or_default();
        if available < amount {
            return Err(ApiError::insufficient());
        }
        return Ok(json!({
            "chain": chain.as_str(), "network": chain.network(), "to_address": to, "amount": s(amount), "fee": s(fee), "net_amount": s(net),
            "used_today": s(used), "daily_max": s(limits.withdraw_daily_max), "available": s(available),
        }));
    }
    let mut tx = st.pool.begin().await?;
    // serialise this user's requests (daily limit + balance) on the balance row
    sqlx::query("SELECT 1 FROM wallet_balances WHERE tenant_id = $1 AND user_id = $2 AND currency = 'USDT' FOR UPDATE").bind(tenant_id).bind(r.user_id).execute(&mut *tx).await?;
    let used: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM withdrawals WHERE tenant_id = $1 AND user_id = $2 AND status NOT IN ('rejected', 'cancelled') AND created_at >= $3")
        .bind(tenant_id)
        .bind(r.user_id)
        .bind(server_day_start(Utc::now()))
        .fetch_one(&mut *tx)
        .await?;
    let used = used.unwrap_or_default();
    if used + amount > limits.withdraw_daily_max {
        return Err(ApiError::unprocessable("daily_limit", format!("This exceeds your daily withdrawal limit ({} of {} USDT used today)", s(used), s(limits.withdraw_daily_max))));
    }
    let w = sqlx::query(
        "INSERT INTO withdrawals (tenant_id, user_id, idempotency_key, chain, to_address, amount, fee, net_amount, status, kyc_status, ip, user_agent)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'requested', $9, $10, $11) ON CONFLICT (tenant_id, idempotency_key) DO NOTHING RETURNING *",
    )
    .bind(tenant_id)
    .bind(r.user_id)
    .bind(&key)
    .bind(chain.as_str())
    .bind(&to)
    .bind(amount)
    .bind(fee)
    .bind(net)
    .bind(&user.kyc_status)
    .bind(&ctx.ip)
    .bind(&ctx.user_agent)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| ApiError::conflict("in_progress", "This withdrawal is already being processed"))?;
    let id: i64 = w.get("id");
    let lock = ledger::post(
        &mut tx,
        NewTxn {
            tenant_id,
            key: format!("wd:{id}:lock"),
            kind: "withdrawal_lock".into(),
            user_id: Some(r.user_id),
            currency: "USDT".into(),
            reference: Some(format!("W{id}")),
            note: Some(format!("Withdrawal to {to}")),
            actor: format!("user:{}", r.user_id),
            request: None,
            legs: vec![Leg::available(r.user_id, "USDT", -amount), Leg::locked(r.user_id, "USDT", amount)],
        },
    )
    .await;
    let lock = match lock {
        Ok(t) => t,
        Err(PostError::Insufficient) => return Err(ApiError::insufficient()),
        Err(e) => return Err(e.into()),
    };
    let w = sqlx::query("UPDATE withdrawals SET lock_txn_id = $2 WHERE id = $1 RETURNING *").bind(id).bind(lock).fetch_one(&mut *tx).await?;
    notify(&mut tx, tenant_id, r.user_id, "withdrawal.requested", "Withdrawal requested", &format!("Your withdrawal of {} USDT is waiting for review.", s(amount)), json!({"withdrawal_id": id})).await?;
    tx.commit().await?;
    record_ip(&st.pool, tenant_id, r.user_id, ctx.ip.as_deref()).await;
    tracing::info!(withdrawal = id, tenant_id, user_id = r.user_id, amount = %amount, chain = %chain, "withdrawal requested");
    Ok(client_json(&w))
}

async fn unlock(tx: &mut Transaction<'_, Postgres>, w: &PgRow, why: &str, actor: &str) -> Result<i64, PostError> {
    let id: i64 = w.get("id");
    let user_id: i64 = w.get("user_id");
    let amount: D = w.get("amount");
    ledger::post(
        tx,
        NewTxn {
            tenant_id: w.get("tenant_id"),
            key: format!("wd:{id}:unlock"),
            kind: "withdrawal_unlock".into(),
            user_id: Some(user_id),
            currency: w.get("currency"),
            reference: Some(format!("W{id}")),
            note: Some(why.into()),
            actor: actor.into(),
            request: None,
            legs: vec![Leg::locked(user_id, "USDT", -amount), Leg::available(user_id, "USDT", amount)],
        },
    )
    .await
}

pub async fn cancel(st: &AppState, tenant_id: i64, user_id: i64, id: i64) -> ApiResult<Value> {
    let mut tx = st.pool.begin().await?;
    let w = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND tenant_id = $2 AND user_id = $3 FOR UPDATE").bind(id).bind(tenant_id).bind(user_id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Withdrawal"))?;
    if w.get::<String, _>("status") != "requested" {
        return Err(ApiError::conflict("invalid_state", "Only a withdrawal waiting for review can be cancelled"));
    }
    let t = unlock(&mut tx, &w, "Cancelled by client", &format!("user:{user_id}")).await?;
    let w = sqlx::query("UPDATE withdrawals SET status = 'cancelled', final_txn_id = $2, updated_at = now() WHERE id = $1 RETURNING *").bind(id).bind(t).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    Ok(client_json(&w))
}

pub async fn approve(st: &AppState, s_ctx: &StaffCtx, id: i64, note: Option<&str>) -> ApiResult<Value> {
    let mut tx = st.pool.begin().await?;
    let w = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(id).bind(s_ctx.ctx.tenant.id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Withdrawal"))?;
    if w.get::<String, _>("status") != "requested" {
        return Err(ApiError::conflict("invalid_state", format!("A {} withdrawal can't be approved", w.get::<String, _>("status"))));
    }
    let note = super::clean_text(note, 500);
    let r = sqlx::query("UPDATE withdrawals SET status = 'approved', review_note = $2, reviewed_by = $3, reviewed_at = now(), updated_at = now() WHERE id = $1 RETURNING *")
        .bind(id)
        .bind(&note)
        .bind(format!("{} ({})", s_ctx.staff.name, s_ctx.staff.tag()))
        .fetch_one(&mut *tx)
        .await?;
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.withdrawal.approved", target_kind: "withdrawal", target_id: id.to_string(), reason: note.as_deref(), before: Some(json!({"status": "requested"})), after: Some(json!({"status": "approved"})) }).await?;
    notify(&mut tx, s_ctx.ctx.tenant.id, r.get("user_id"), "withdrawal.approved", "Withdrawal approved", &format!("Your withdrawal of {} USDT was approved and will be sent shortly.", s(r.get::<D, _>("amount"))), json!({"withdrawal_id": id})).await?;
    tx.commit().await?;
    Ok(withdrawal_json(&r))
}

pub async fn reject(st: &AppState, s_ctx: &StaffCtx, id: i64, reason: &str) -> ApiResult<Value> {
    let reason = reason.trim();
    if reason.len() < 3 {
        return Err(ApiError::validation("reason", "Enter a reason for the client"));
    }
    let mut tx = st.pool.begin().await?;
    let w = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(id).bind(s_ctx.ctx.tenant.id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Withdrawal"))?;
    // Only before approval: payouts are sent by hand, so from Approved on the money may already have left (a failed
    // payout check also returns to Approved) and releasing the lock would refund the client twice. The row lock
    // above serialises this with approve, so a racing approve and reject can't both succeed.
    let status: String = w.get("status");
    if status != "requested" {
        return Err(ApiError::conflict("invalid_state", format!("A {status} withdrawal can't be rejected; only one waiting for review can")));
    }
    let t = unlock(&mut tx, &w, &format!("Rejected: {reason}"), &s_ctx.staff.tag()).await?;
    let r = sqlx::query("UPDATE withdrawals SET status = 'rejected', reason = $2, reviewed_by = $3, reviewed_at = now(), final_txn_id = $4, updated_at = now() WHERE id = $1 RETURNING *")
        .bind(id)
        .bind(reason)
        .bind(format!("{} ({})", s_ctx.staff.name, s_ctx.staff.tag()))
        .bind(t)
        .fetch_one(&mut *tx)
        .await?;
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.withdrawal.rejected", target_kind: "withdrawal", target_id: id.to_string(), reason: Some(reason), before: Some(json!({"status": status})), after: Some(json!({"status": "rejected"})) }).await?;
    notify(&mut tx, s_ctx.ctx.tenant.id, r.get("user_id"), "withdrawal.rejected", "Withdrawal rejected", &format!("Your withdrawal of {} USDT was rejected: {reason}. The funds are back in your wallet.", s(r.get::<D, _>("amount"))), json!({"withdrawal_id": id})).await?;
    tx.commit().await?;
    Ok(withdrawal_json(&r))
}

/// Staff sent the payout from the company wallet and enter its hash; the watcher verifies it on chain.
pub async fn mark_paid(st: &AppState, s_ctx: &StaffCtx, id: i64, tx_hash: &str) -> ApiResult<Value> {
    let mut tx = st.pool.begin().await?;
    let w = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND tenant_id = $2 FOR UPDATE").bind(id).bind(s_ctx.ctx.tenant.id).fetch_optional(&mut *tx).await?.ok_or_else(|| ApiError::not_found("Withdrawal"))?;
    let status: String = w.get("status");
    if status != "approved" {
        return Err(ApiError::conflict("invalid_state", format!("A {status} withdrawal can't be marked as paid; approve it first")));
    }
    let chain = ChainId::parse(w.get::<String, _>("chain").as_str()).unwrap_or(ChainId::Tron);
    let hash = chain.normalize_hash(tx_hash).ok_or_else(|| ApiError::validation("tx_hash", "Enter the payout transaction hash (64 hexadecimal characters)"))?;
    let used: Option<i64> = sqlx::query_scalar("SELECT id FROM withdrawals WHERE chain = $1 AND payout_tx_hash = $2 AND id <> $3").bind(chain.as_str()).bind(&hash).bind(id).fetch_optional(&mut *tx).await?;
    let is_deposit: Option<i64> = sqlx::query_scalar("SELECT id FROM deposits WHERE chain = $1 AND tx_hash = $2").bind(chain.as_str()).bind(&hash).fetch_optional(&mut *tx).await?;
    if used.is_some() || is_deposit.is_some() {
        return Err(ApiError::conflict("tx_already_used", "This transaction is already recorded for another deposit or withdrawal"));
    }
    let r = sqlx::query("UPDATE withdrawals SET status = 'paid', payout_tx_hash = $2, payout_error = NULL, payout_confirmations = 0, paid_by = $3, paid_at = now(), updated_at = now() WHERE id = $1 RETURNING *")
        .bind(id)
        .bind(&hash)
        .bind(format!("{} ({})", s_ctx.staff.name, s_ctx.staff.tag()))
        .fetch_one(&mut *tx)
        .await?;
    audit::staff(&mut tx, s_ctx, Entry { action: "wallet.withdrawal.paid", target_kind: "withdrawal", target_id: id.to_string(), reason: None, before: Some(json!({"status": status})), after: Some(json!({"status": "paid", "payout_tx_hash": hash})) }).await?;
    tx.commit().await?;
    if let Err(e) = verify_payout(st, id).await {
        tracing::warn!(withdrawal = id, error = %e, "payout check failed; the watcher retries");
    }
    let r2 = sqlx::query("SELECT * FROM withdrawals WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    let _ = r;
    Ok(withdrawal_json(&r2))
}

async fn payout_failed(st: &AppState, id: i64, reason: &str) -> anyhow::Result<()> {
    let mut tx = st.pool.begin().await?;
    let r = sqlx::query("UPDATE withdrawals SET status = 'approved', payout_error = $2, payout_tx_hash = NULL, payout_confirmations = 0, updated_at = now() WHERE id = $1 AND status = 'paid' RETURNING tenant_id")
        .bind(id)
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await?;
    if let Some(r) = r {
        audit::system(&mut tx, r.get("tenant_id"), "wallet.withdrawal.payout_rejected", "withdrawal", id.to_string(), Some(json!({"reason": reason}))).await?;
    }
    tx.commit().await?;
    tracing::warn!(withdrawal = id, reason, "payout verification failed");
    Ok(())
}

/// Watcher step for a `paid` withdrawal: the payout must be a successful USDT transfer of exactly the net
/// amount from a company address to the client's address, with the chain's confirmations.
pub async fn verify_payout(st: &AppState, id: i64) -> anyhow::Result<()> {
    let Some(w) = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND status = 'paid'").bind(id).fetch_optional(&st.pool).await? else { return Ok(()) };
    let tenant_id: i64 = w.get("tenant_id");
    let Some(chain) = ChainId::parse(w.get::<String, _>("chain").as_str()) else { return Ok(()) };
    let Ok(client) = st.chain(chain) else { return Ok(()) };
    let Some(hash) = w.get::<Option<String>, _>("payout_tx_hash") else { return Ok(()) };
    let lookup = client.lookup(&hash).await?;
    match lookup.status {
        TxStatus::NotFound => {
            let paid_at: DateTime<Utc> = w.get::<Option<DateTime<Utc>>, _>("paid_at").unwrap_or_else(Utc::now);
            if Utc::now() - paid_at > Duration::hours(PAYOUT_NOT_FOUND_HOURS) {
                payout_failed(st, id, "The payout transaction was not found on chain").await?;
            }
            return Ok(());
        }
        TxStatus::Failed => return payout_failed(st, id, "The payout transaction failed on chain").await,
        TxStatus::Success => {}
    }
    let company = settings::company_addresses(&st.pool, tenant_id, chain).await?;
    let to: String = w.get("to_address");
    let net: D = w.get("net_amount");
    let ok = lookup.transfers.iter().any(|t| t.token == chain.usdt_contract() && company.contains(&t.from) && t.to == to && t.amount == net);
    if !ok {
        return payout_failed(st, id, &format!("The transaction does not send exactly {} USDT from a company address to {to}", s(net))).await;
    }
    let conf = confirmations(client.head().await?, lookup.block.unwrap_or_default());
    let required = settings::chain(&st.pool, tenant_id, chain).await?.map(|c| c.confirmations).unwrap_or(20);
    sqlx::query("UPDATE withdrawals SET payout_confirmations = $2, updated_at = now() WHERE id = $1 AND status = 'paid'").bind(id).bind(conf).execute(&st.pool).await?;
    if conf < required {
        return Ok(());
    }
    complete(st, id).await
}

async fn complete(st: &AppState, id: i64) -> anyhow::Result<()> {
    let mut tx = st.pool.begin().await?;
    let Some(w) = sqlx::query("SELECT * FROM withdrawals WHERE id = $1 AND status = 'paid' FOR UPDATE").bind(id).fetch_optional(&mut *tx).await? else { return Ok(()) };
    let (tenant_id, user_id): (i64, i64) = (w.get("tenant_id"), w.get("user_id"));
    let (amount, fee, net): (D, D, D) = (w.get("amount"), w.get("fee"), w.get("net_amount"));
    let chain: String = w.get("chain");
    let ccy: String = w.get("currency");
    let t = match ledger::post(
        &mut tx,
        NewTxn {
            tenant_id,
            key: format!("wd:{id}:complete"),
            kind: "withdrawal".into(),
            user_id: Some(user_id),
            currency: ccy.clone(),
            reference: w.get("payout_tx_hash"),
            note: Some(format!("Withdrawal to {}", w.get::<String, _>("to_address"))),
            actor: "system".into(),
            request: None,
            legs: vec![Leg::locked(user_id, &ccy, -amount), Leg::sys(&ccy, &format!("withdrawal:{chain}"), net), Leg::sys(&ccy, "fees", fee)],
        },
    )
    .await
    {
        Ok(t) => t,
        Err(PostError::Duplicate) => sqlx::query_scalar("SELECT id FROM ledger_txns WHERE tenant_id = $1 AND idempotency_key = $2").bind(tenant_id).bind(format!("wd:{id}:complete")).fetch_one(&st.pool).await?,
        Err(PostError::Insufficient) => anyhow::bail!("withdrawal {id}: locked balance below the withdrawal amount"),
        Err(PostError::Db(e)) => return Err(e),
    };
    sqlx::query("UPDATE withdrawals SET status = 'completed', final_txn_id = $2, completed_at = now(), updated_at = now() WHERE id = $1").bind(id).bind(t).execute(&mut *tx).await?;
    notify(&mut tx, tenant_id, user_id, "withdrawal.completed", "Withdrawal sent", &format!("{} USDT was sent to your wallet.", s(net)), json!({"withdrawal_id": id, "tx_hash": w.get::<Option<String>, _>("payout_tx_hash")})).await?;
    tx.commit().await?;
    tracing::info!(withdrawal = id, tenant_id, user_id, net = %net, "withdrawal completed");
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Risk checklist (D118)                                               */
/* ------------------------------------------------------------------ */

pub async fn risk(st: &AppState, tenant_slug: &str, w: &PgRow) -> ApiResult<Value> {
    let tenant_id: i64 = w.get("tenant_id");
    let user_id: i64 = w.get("user_id");
    let pool = &st.pool;
    let kyc = match st.users.get(tenant_slug, user_id).await {
        Ok(Some(u)) => json!({"status": u.kyc_status, "account_status": u.status, "email": u.email, "name": u.name, "ok": u.kyc_status == "verified" && u.status == "active"}),
        Ok(None) => json!({"status": null, "ok": false, "error": "Client not found in the gateway"}),
        Err(_) => json!({"status": null, "ok": false, "error": "Gateway unavailable"}),
    };
    let limits = settings::limits(pool, tenant_id).await?;
    let last_dep: Option<(DateTime<Utc>, D)> = sqlx::query_as("SELECT credited_at, amount FROM deposits WHERE tenant_id = $1 AND user_id = $2 AND status = 'credited' ORDER BY credited_at DESC LIMIT 1")
        .bind(tenant_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    let created: DateTime<Utc> = w.get("created_at");
    let recent = last_dep.map(|(t, _)| created - t < Duration::hours(limits.deposit_cooldown_hours.max(24) as i64)).unwrap_or(false);
    let totals = sqlx::query(
        "SELECT
            (SELECT COALESCE(sum(amount), 0) FROM deposits WHERE tenant_id = $1 AND user_id = $2 AND status = 'credited') AS deposited,
            (SELECT COALESCE(sum(amount), 0) FROM withdrawals WHERE tenant_id = $1 AND user_id = $2 AND status = 'completed') AS withdrawn,
            (SELECT COALESCE(sum(amount), 0) FROM trading_transfers WHERE tenant_id = $1 AND user_id = $2 AND status = 'completed' AND direction = 'to_trading') AS to_trading,
            (SELECT COALESCE(sum(amount), 0) FROM trading_transfers WHERE tenant_id = $1 AND user_id = $2 AND status = 'completed' AND direction = 'from_trading') AS from_trading",
    )
    .bind(tenant_id)
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    let (dep, wd, to_t, from_t): (D, D, D, D) = (totals.get("deposited"), totals.get("withdrawn"), totals.get("to_trading"), totals.get("from_trading"));
    let ip: Option<String> = w.get("ip");
    let (ip_seen, ip_others): (bool, Vec<i64>) = match &ip {
        Some(ip) => {
            let seen: Option<i64> = sqlx::query_scalar("SELECT hits FROM client_ips WHERE tenant_id = $1 AND user_id = $2 AND ip = $3 AND first_seen < $4").bind(tenant_id).bind(user_id).bind(ip).bind(created).fetch_optional(pool).await?.map(|h: i32| h as i64);
            let others: Vec<i64> = sqlx::query_scalar("SELECT DISTINCT user_id FROM client_ips WHERE tenant_id = $1 AND ip = $2 AND user_id <> $3 LIMIT 10").bind(tenant_id).bind(ip).bind(user_id).fetch_all(pool).await?;
            (seen.is_some(), others)
        }
        None => (false, vec![]),
    };
    let to: String = w.get("to_address");
    let id: i64 = w.get("id");
    let addr_before: i64 = sqlx::query_scalar("SELECT count(*) FROM withdrawals WHERE tenant_id = $1 AND user_id = $2 AND to_address = $3 AND id <> $4 AND status = 'completed'").bind(tenant_id).bind(user_id).bind(&to).bind(id).fetch_one(pool).await?;
    let addr_others: Vec<i64> = sqlx::query_scalar(
        "SELECT DISTINCT user_id FROM (SELECT user_id FROM withdrawals WHERE tenant_id = $1 AND to_address = $2 AND user_id <> $3
                                       UNION SELECT user_id FROM deposits WHERE tenant_id = $1 AND from_address = $2 AND user_id IS NOT NULL AND user_id <> $3) x LIMIT 10",
    )
    .bind(tenant_id)
    .bind(&to)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
    let deposited_from: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM deposits WHERE tenant_id = $1 AND user_id = $2 AND from_address = $3)").bind(tenant_id).bind(user_id).bind(&to).fetch_one(pool).await?;
    let bal = ledger::balances(pool, tenant_id, user_id).await?;
    Ok(json!({
        "kyc": kyc,
        "recent_deposit": {
            "last_credited_at": last_dep.map(|x| x.0),
            "last_amount": last_dep.map(|x| s(x.1)),
            "cooldown_hours": limits.deposit_cooldown_hours,
            "ok": !recent,
        },
        "flows": {
            "deposited": s(dep), "withdrawn": s(wd), "to_trading": s(to_t), "from_trading": s(from_t),
            "trading_pnl": s(from_t - to_t),
            "ok": wd + w.get::<D, _>("amount") <= dep + (from_t - to_t).max(D::ZERO),
        },
        "ip": {"address": ip, "seen_before": ip_seen, "other_clients": ip_others, "ok": ip_others.is_empty()},
        "destination": {
            "address": to, "paid_before": addr_before, "used_for_deposits": deposited_from, "other_clients": addr_others,
            "ok": addr_others.is_empty(),
        },
        "balances": ledger::balances_json(&bal),
        "amount": s(w.get::<D, _>("amount")),
        "fee": s(w.get::<D, _>("fee")),
        "net_amount": s(w.get::<D, _>("net_amount")),
        "payout_confirmations": w.get::<i32, _>("payout_confirmations"),
        "payout_hash": s_opt(None),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn fees_and_server_day() {
        let l = Limits {
            withdraw_min: D::from(10),
            withdraw_max: D::from(1000),
            withdraw_daily_max: D::from(2000),
            withdraw_fee_flat: D::ONE,
            withdraw_fee_pct: D::from_str("0.5").unwrap(),
            deposit_cooldown_hours: 24,
            intent_ttl_minutes: 60,
        };
        let c = ChainCfg {
            chain: ChainId::Tron,
            receiving_address: String::new(),
            payout_address: None,
            confirmations: 20,
            deposits_enabled: true,
            withdrawals_enabled: true,
            min_deposit: D::from(10),
            withdraw_fee: D::from_str("0.5").unwrap(),
        };
        // 1 flat + 0.5% of 101 = 0.505 → 0.51 (rounded up) + 0.5 network
        assert_eq!(fee_for(&l, &c, D::from(101)), D::from_str("2.01").unwrap());
        let now = DateTime::parse_from_rfc3339("2026-09-28T22:30:00Z").unwrap().with_timezone(&Utc);
        // 22:30 UTC is 01:30 GMT+3 on the 29th → day starts 2026-09-28T21:00Z
        assert_eq!(server_day_start(now).to_rfc3339(), "2026-09-28T21:00:00+00:00");
    }
}

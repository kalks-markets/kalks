//! Cash out to client wallets: cashback payouts (`refund`), loyalty cashback redemptions and contest prizes
//! (`adjustment`). Every transfer has a stable idempotency key; network errors and 5xx retry with backoff.

use crate::audit::{self, Actor};
use crate::clients::{self, WalletOutcome};
use crate::db;
use crate::error::{ApiError, ApiResult};
use crate::four_eyes::{self, AWAITING};
use crate::money::{D, ZERO, num};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;

/// Groups accruals past the hold into one payout per client. Returns (payouts created, total). Four-eyes: a payout
/// at or above `fourEyesThreshold` is created in `approval` (the payout loop pays only `pending`) and waits for two
/// staff members (`decide_cashback`).
pub async fn cashback_batch(st: &AppState, tenant: &str, ignore_hold: bool) -> anyhow::Result<(usize, D)> {
    let settings = db::settings(&st.pool, tenant).await?;
    let hold = if ignore_hold { 0 } else { settings.cashback_hold_hours };
    let mut tx = st.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("growth-cashback:{tenant}")).execute(&mut *tx).await?;
    let users: Vec<(i64, D)> = sqlx::query_as(
        "SELECT user_id, sum(amount) FROM cashback_accruals WHERE tenant = $1 AND status = 'accrued' AND payout_id IS NULL
           AND created_at <= now() - make_interval(hours => $2) GROUP BY user_id HAVING sum(amount) > 0",
    )
    .bind(tenant)
    .bind(hold as i32)
    .fetch_all(&mut *tx)
    .await?;
    let mut total = ZERO;
    for (user, amount) in &users {
        let status = if four_eyes::needs_two(*amount, settings.four_eyes_threshold) { "approval" } else { "pending" };
        let id: i64 = sqlx::query_scalar("INSERT INTO cashback_payouts (tenant, user_id, amount, status) VALUES ($1,$2,$3,$4) RETURNING id")
            .bind(tenant)
            .bind(user)
            .bind(amount)
            .bind(status)
            .fetch_one(&mut *tx)
            .await?;
        if status == "approval" {
            let after = json!({"user": user, "amount": num(*amount), "status": "approval", "fourEyesThreshold": num(settings.four_eyes_threshold)});
            audit::record(&mut *tx, tenant, &Actor::system(), "cashback.hold", Some(format!("cashback_payout:{id}")), None, Some(after), None).await?;
        }
        sqlx::query(
            "UPDATE cashback_accruals SET payout_id = $1 WHERE tenant = $2 AND user_id = $3 AND status = 'accrued' AND payout_id IS NULL AND created_at <= now() - make_interval(hours => $4)",
        )
        .bind(id)
        .bind(tenant)
        .bind(user)
        .bind(hold as i32)
        .execute(&mut *tx)
        .await?;
        total += *amount;
    }
    tx.commit().await?;
    if !users.is_empty() {
        st.wake.notify_one();
    }
    Ok((users.len(), total))
}

fn backoff(attempts: i32) -> f64 {
    (2i64.pow(attempts.clamp(0, 6) as u32) * 15).min(900) as f64
}

pub async fn cashback_tick(st: &AppState) -> anyhow::Result<usize> {
    let rows = sqlx::query("SELECT id, tenant, user_id, amount, attempts FROM cashback_payouts WHERE status = 'pending' AND next_try_at <= now() ORDER BY id LIMIT 100").fetch_all(&st.pool).await?;
    let mut n = 0;
    for r in rows {
        let id: i64 = r.get("id");
        let tenant: String = r.get("tenant");
        let user: i64 = r.get("user_id");
        let amount: D = r.get("amount");
        match clients::wallet_credit(st, &tenant, &format!("growth:cashback:{id}"), user, amount, "refund", &format!("cashback:{id}"), "Cashback on your trading volume").await {
            WalletOutcome::Credited { txn } => {
                let mut tx = st.pool.begin().await?;
                sqlx::query("UPDATE cashback_payouts SET status = 'paid', wallet_txn = $2, paid_at = now(), error = NULL WHERE id = $1").bind(id).bind(&txn).execute(&mut *tx).await?;
                sqlx::query("UPDATE cashback_accruals SET status = 'paid' WHERE payout_id = $1 AND status = 'accrued'").bind(id).execute(&mut *tx).await?;
                tx.commit().await?;
                clients::notify(st, &tenant, user, "cashback.paid", "Cashback paid".into(), format!("${} cashback is in your wallet.", amount.normalize()), "/rewards/cashback");
                n += 1;
            }
            WalletOutcome::Retry(msg) => {
                let a: i32 = r.get::<i32, _>("attempts") + 1;
                sqlx::query("UPDATE cashback_payouts SET attempts = $2, error = $3, next_try_at = now() + make_interval(secs => $4) WHERE id = $1").bind(id).bind(a).bind(&msg).bind(backoff(a)).execute(&st.pool).await?;
            }
            WalletOutcome::Fail(msg) => {
                sqlx::query("UPDATE cashback_payouts SET status = 'failed', error = $2 WHERE id = $1").bind(id).bind(&msg).execute(&st.pool).await?;
                tracing::error!(payout = id, error = %msg, "cashback payout failed; needs review");
            }
        }
    }
    Ok(n)
}

/// Pending wallet credits (loyalty cashback redemptions, contest prizes).
pub async fn wallet_tick(st: &AppState) -> anyhow::Result<usize> {
    let rows = sqlx::query("SELECT * FROM wallet_credits WHERE status = 'pending' AND next_try_at <= now() ORDER BY id LIMIT 100").fetch_all(&st.pool).await?;
    let mut n = 0;
    for r in rows {
        let id: i64 = r.get("id");
        let tenant: String = r.get("tenant");
        let user: i64 = r.get("user_id");
        let reference: String = r.get("ref");
        let amount: D = r.get("amount");
        let out = clients::wallet_credit(st, &tenant, r.get::<&str, _>("idem_key"), user, amount, r.get::<&str, _>("kind"), &reference, r.get::<&str, _>("note")).await;
        let (status, txn, err) = match &out {
            WalletOutcome::Credited { txn } => ("paid", Some(txn.clone()), None),
            WalletOutcome::Fail(m) => ("failed", None, Some(m.clone())),
            WalletOutcome::Retry(m) => ("pending", None, Some(m.clone())),
        };
        let mut tx = st.pool.begin().await?;
        if status == "pending" {
            let a: i32 = r.get::<i32, _>("attempts") + 1;
            sqlx::query("UPDATE wallet_credits SET attempts = $2, error = $3, next_try_at = now() + make_interval(secs => $4) WHERE id = $1").bind(id).bind(a).bind(&err).bind(backoff(a)).execute(&mut *tx).await?;
        } else {
            sqlx::query("UPDATE wallet_credits SET status = $2, wallet_txn = $3, error = $4, paid_at = CASE WHEN $2 = 'paid' THEN now() END WHERE id = $1").bind(id).bind(status).bind(&txn).bind(&err).execute(&mut *tx).await?;
            let done = status == "paid";
            if let Some(rid) = reference.strip_prefix("redemption:").and_then(|x| x.parse::<i64>().ok()) {
                sqlx::query("UPDATE redemptions SET status = $2, wallet_txn = $3, error = $4, completed_at = CASE WHEN $2 = 'completed' THEN now() END WHERE id = $1")
                    .bind(rid)
                    .bind(if done { "completed" } else { "failed" })
                    .bind(&txn)
                    .bind(&err)
                    .execute(&mut *tx)
                    .await?;
            }
            if let Some(eid) = reference.strip_prefix("prize:").and_then(|x| x.parse::<i64>().ok()) {
                sqlx::query("UPDATE contest_entries SET prize_status = $2, prize_ref = COALESCE($3, prize_ref), prize_error = $4 WHERE id = $1")
                    .bind(eid)
                    .bind(if done { "paid" } else { "failed" })
                    .bind(&txn)
                    .bind(&err)
                    .execute(&mut *tx)
                    .await?;
            }
            if let Some(pid) = reference.strip_prefix("mission:").and_then(|x| x.parse::<i64>().ok()) {
                sqlx::query("UPDATE mission_progress SET reward_status = $2, reward_ref = COALESCE($3, reward_ref), reward_error = $4, updated_at = now() WHERE id = $1")
                    .bind(pid)
                    .bind(if done { "paid" } else { "failed" })
                    .bind(&txn)
                    .bind(&err)
                    .execute(&mut *tx)
                    .await?;
            }
            n += 1;
        }
        tx.commit().await?;
        if status == "paid" {
            let title = if reference.starts_with("prize:") {
                "Contest prize paid"
            } else if reference.starts_with("mission:") {
                "Mission reward paid"
            } else {
                "Reward paid"
            };
            clients::notify(st, &tenant, user, "reward.paid", title.into(), format!("${} is in your wallet.", amount.normalize()), "/wallet");
        }
    }
    Ok(n)
}

/// Re-queues a failed wallet credit (staff retry).
pub async fn retry_credit(st: &AppState, reference: &str) -> anyhow::Result<bool> {
    let n = sqlx::query("UPDATE wallet_credits SET status = 'pending', next_try_at = now(), error = NULL WHERE ref = $1 AND status = 'failed'").bind(reference).execute(&st.pool).await?.rows_affected();
    if n > 0 {
        st.wake.notify_one();
    }
    Ok(n > 0)
}

// ---------------------------------------------------------------- cashback four-eyes

/// A cashback payout for the Back Office (`threshold` = the broker's current four-eyes threshold).
pub fn cashback_payout_json(r: &sqlx::postgres::PgRow, threshold: Option<D>) -> Value {
    let status: String = r.get("status");
    json!({
        "id": r.get::<i64, _>("id"), "userId": r.get::<i64, _>("user_id"), "amount": num(r.get("amount")), "status": status,
        "attempts": r.get::<i32, _>("attempts"), "error": r.get::<Option<String>, _>("error"), "walletTxn": r.get::<Option<String>, _>("wallet_txn"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"), "paidAt": r.get::<Option<DateTime<Utc>>, _>("paid_at"),
        "decidedBy": r.get::<Option<String>, _>("decided_by"), "decidedAt": r.get::<Option<DateTime<Utc>>, _>("decided_at"),
        "rejectReason": r.get::<Option<String>, _>("reject_reason"),
        "fourEyes": four_eyes::json_of(r, Some(r.get("amount")), &status, threshold),
    })
}

/// Staff decision on a cashback payout held for approval (`approval` or `awaiting_second_approval`). Approve: at or
/// above `fourEyesThreshold` the first approval only records it; the second approval (another staff member, else 409
/// `four_eyes_same_staff`) or a single approval below the threshold makes it `pending`, and the payout loop pays it
/// with its usual idempotent key `growth:cashback:<payout>`. Reject (reason required, either approver): the payout
/// and its accruals become `rejected` (never paid, never batched again).
pub async fn decide_cashback(st: &AppState, tenant: &str, id: i64, approve: bool, actor: &Actor, note: &str) -> ApiResult<Value> {
    let threshold = db::settings(&st.pool, tenant).await?.four_eyes_threshold;
    let mut tx = st.pool.begin().await?;
    let r = sqlx::query("SELECT * FROM cashback_payouts WHERE id = $1 AND tenant = $2 FOR UPDATE").bind(id).bind(tenant).fetch_optional(&mut *tx).await?.ok_or(ApiError::NotFound)?;
    let status: String = r.get("status");
    if !matches!(status.as_str(), "approval" | AWAITING) {
        return Err(ApiError::Conflict { code: "state", message: "This payout isn't waiting for a decision.".into() });
    }
    let amount: D = r.get("amount");
    let first_id: Option<String> = r.get("first_approved_by_id");
    let first_name: Option<String> = r.get("first_approved_by_name");
    let note_opt = Some(note.trim()).filter(|n| !n.is_empty());
    let target = Some(format!("cashback_payout:{id}"));
    let mut release = false;
    if approve {
        if status == AWAITING {
            if first_id.as_deref() == Some(four_eyes::staff_id(actor).as_str()) {
                return Err(four_eyes::same_staff());
            }
            if !four_eyes::claim_second(&mut tx, "cashback_payouts", "status", id, actor, "pending").await? {
                return Err(ApiError::Conflict { code: "state", message: "This payout isn't waiting for a decision.".into() });
            }
            let mut after = four_eyes::approvers(first_id.as_deref(), first_name.as_deref(), actor);
            after["amount"] = num(amount);
            after["status"] = json!("pending");
            audit::record(&mut *tx, tenant, actor, "cashback.approve", target, Some(json!({"status": status})), Some(after), note_opt).await?;
            release = true;
        } else if four_eyes::needs_two(amount, threshold) && first_id.is_none() {
            four_eyes::record_first(&mut tx, "cashback_payouts", "status", id, &status, actor, note_opt, threshold).await?;
            let after = json!({"amount": num(amount), "status": AWAITING, "fourEyesThreshold": num(threshold)});
            audit::record(&mut *tx, tenant, actor, "cashback.approve_first", target, Some(json!({"status": status})), Some(after), note_opt).await?;
        } else {
            sqlx::query("UPDATE cashback_payouts SET status = 'pending' WHERE id = $1 AND status = $2").bind(id).bind(&status).execute(&mut *tx).await?;
            audit::record(&mut *tx, tenant, actor, "cashback.approve", target, Some(json!({"status": status})), Some(json!({"amount": num(amount), "status": "pending"})), note_opt).await?;
            release = true;
        }
        if release {
            sqlx::query("UPDATE cashback_payouts SET decided_by = $2, decided_at = now(), next_try_at = now() WHERE id = $1").bind(id).bind(actor.label()).execute(&mut *tx).await?;
        }
    } else {
        sqlx::query("UPDATE cashback_payouts SET status = 'rejected', reject_reason = $2, decided_by = $3, decided_at = now() WHERE id = $1").bind(id).bind(note).bind(actor.label()).execute(&mut *tx).await?;
        let n = sqlx::query("UPDATE cashback_accruals SET status = 'rejected' WHERE payout_id = $1 AND status = 'accrued'").bind(id).execute(&mut *tx).await?.rows_affected();
        let before = json!({"status": status, "firstApprovedById": first_id, "firstApprovedBy": first_name});
        let after = json!({"status": "rejected", "from": status, "amount": num(amount), "accrualsRejected": n});
        audit::record(&mut *tx, tenant, actor, "cashback.reject", target, Some(before), Some(after), Some(note)).await?;
    }
    tx.commit().await?;
    if release {
        st.wake.notify_one();
        let _ = cashback_tick(st).await;
    }
    let r = sqlx::query("SELECT * FROM cashback_payouts WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(json!({"payout": cashback_payout_json(&r, Some(threshold))}))
}

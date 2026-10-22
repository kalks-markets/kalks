//! Payout batches (D56): pending commissions are grouped per payee into a batch for a closed period
//! (daily / weekly / monthly). An admin approves or rejects the batch. Approved payouts are credited to the
//! payee's wallet with `POST /v1/wallets/transfers` (kind `ib_payout`), idempotent per payout. While the
//! wallet is unreachable a payout stays `transfer_pending` and is retried with backoff. Batches at or above the
//! four-eyes threshold need two different approvers (see [`approve`]).

use crate::audit::{self, Actor};
use crate::calc;
use crate::clients::{self, WalletOutcome};
use crate::db;
use crate::error::{ApiError, ApiResult};
use crate::money::{D, ZERO, num};
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::Row;

/// Creates a batch of every payable pending commission available before `period_end`. Payees whose net
/// amount is below the programme minimum (or not positive) are carried over. `None` = nothing to pay.
pub async fn create_batch(st: &AppState, tenant: &str, actor: &Actor, period_start: Option<DateTime<Utc>>, period_end: DateTime<Utc>) -> anyhow::Result<Option<i64>> {
    let s = db::settings(&st.pool, tenant).await?;
    let mut tx = st.pool.begin().await?;
    // serialise batch creation per tenant
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext('ib-batch:' || $1))").bind(tenant).execute(&mut *tx).await?;
    let payees = sqlx::query(
        "SELECT beneficiary_id, sum(amount) AS amount, count(*) AS lines FROM commissions
         WHERE tenant = $1 AND status = 'pending' AND batch_id IS NULL AND available_at <= $2 AND created_at < $2
         GROUP BY beneficiary_id
         HAVING sum(amount) > 0 AND sum(amount) >= $3
         ORDER BY beneficiary_id",
    )
    .bind(tenant)
    .bind(period_end)
    .bind(s.payout.min_amount)
    .fetch_all(&mut *tx)
    .await?;
    if payees.is_empty() {
        return Ok(None);
    }
    let total: D = payees.iter().map(|r| r.get::<D, _>("amount")).sum();
    let lines: i64 = payees.iter().map(|r| r.get::<i64, _>("lines")).sum();
    let batch_id: Option<i64> = sqlx::query_scalar(
        "INSERT INTO payout_batches (tenant, schedule, period_start, period_end, total, lines, payees, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING RETURNING id",
    )
    .bind(tenant)
    .bind(&s.payout.schedule)
    .bind(period_start)
    .bind(period_end)
    .bind(total)
    .bind(lines as i32)
    .bind(payees.len() as i32)
    .bind(&actor.id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(batch_id) = batch_id else { return Ok(None) };
    for p in &payees {
        let user: i64 = p.get("beneficiary_id");
        let payout_id: i64 = sqlx::query_scalar(
            "INSERT INTO payouts (tenant, batch_id, user_id, amount, lines, idempotency_key) VALUES ($1,$2,$3,$4,$5,$6) RETURNING id",
        )
        .bind(tenant)
        .bind(batch_id)
        .bind(user)
        .bind(p.get::<D, _>("amount"))
        .bind(p.get::<i64, _>("lines") as i32)
        .bind(format!("ib:payout:{tenant}:{batch_id}:{user}"))
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE commissions SET batch_id = $1, payout_id = $2, updated_at = now()
             WHERE tenant = $3 AND beneficiary_id = $4 AND status = 'pending' AND batch_id IS NULL AND available_at <= $5 AND created_at < $5",
        )
        .bind(batch_id)
        .bind(payout_id)
        .bind(tenant)
        .bind(user)
        .bind(period_end)
        .execute(&mut *tx)
        .await?;
    }
    audit::record(&mut *tx, tenant, actor, "batch.create", Some(format!("batch:{batch_id}")), None, Some(json!({"total": total.to_string(), "payees": payees.len(), "lines": lines, "periodEnd": period_end})), None).await?;
    tx.commit().await?;
    tracing::info!(batch_id, %total, payees = payees.len(), "payout batch created");
    Ok(Some(batch_id))
}

async fn batch_status(st: &AppState, tenant: &str, id: i64) -> ApiResult<String> {
    sqlx::query_scalar("SELECT status FROM payout_batches WHERE id = $1 AND tenant = $2").bind(id).bind(tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)
}

/// What an approval did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    /// The batch is approved and its payouts are queued for the wallet.
    Released,
    /// Four-eyes: first approval recorded; a different staff member must approve before anything is paid.
    AwaitingSecond,
}

/// Approves a batch. Four-eyes: a batch whose total is at or above the programme's `fourEyesThreshold` (USD,
/// 0 = every batch) needs two different staff members. The first approval moves it to
/// `awaiting_second_approval` and pays nothing; the second (another staff member) releases it. The same staff
/// member is refused with `409 four_eyes_same_staff`. The batch row is locked and its status checked inside one
/// transaction, so the release happens once however often (or concurrently) it is approved.
pub async fn approve(st: &AppState, tenant: &str, id: i64, actor: &Actor, note: Option<&str>) -> ApiResult<Approval> {
    let threshold = db::settings(&st.pool, tenant).await?.four_eyes_threshold;
    let mut tx = st.pool.begin().await?;
    let row = sqlx::query("SELECT status, total, first_approved_by_id, first_approved_by_name FROM payout_batches WHERE id = $1 AND tenant = $2 FOR UPDATE")
        .bind(id)
        .bind(tenant)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::NotFound)?;
    let status: String = row.get("status");
    let total: D = row.get("total");
    let first: Option<(String, Option<String>)> = row.get::<Option<String>, _>("first_approved_by_id").map(|i| (i, row.get("first_approved_by_name")));
    match status.as_str() {
        "pending_approval" if total >= threshold => {
            sqlx::query(
                "UPDATE payout_batches SET status = 'awaiting_second_approval', first_approved_by_id = $2, first_approved_by_name = $3,
                    first_approved_at = now(), first_approval_note = $4, four_eyes_threshold = $5 WHERE id = $1",
            )
            .bind(id)
            .bind(&actor.id)
            .bind(actor.label())
            .bind(note)
            .bind(threshold)
            .execute(&mut *tx)
            .await?;
            audit::record(
                &mut *tx,
                tenant,
                actor,
                "batch.approve_first",
                Some(format!("batch:{id}")),
                Some(json!({"status": "pending_approval"})),
                Some(json!({"status": "awaiting_second_approval", "total": total.to_string(), "threshold": threshold.to_string()})),
                note,
            )
            .await?;
            tx.commit().await?;
            return Ok(Approval::AwaitingSecond);
        }
        "pending_approval" => {}
        "awaiting_second_approval" => {
            if first.as_ref().is_some_and(|(by, _)| *by == actor.id) {
                return Err(ApiError::Conflict { code: "four_eyes_same_staff", message: "Four-eyes rule: another staff member must give the second approval.".into() });
            }
        }
        s => return Err(ApiError::Conflict { code: "invalid_state", message: format!("This batch is {s}.") }),
    }
    sqlx::query("UPDATE payout_batches SET status = 'approved', decided_by = $2, decided_by_id = $3, decided_at = now(), decision_note = $4 WHERE id = $1")
        .bind(id)
        .bind(actor.label())
        .bind(&actor.id)
        .bind(note)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE payouts SET status = 'transfer_pending', next_attempt_at = now() WHERE batch_id = $1 AND status = 'pending_approval'").bind(id).execute(&mut *tx).await?;
    sqlx::query("UPDATE commissions SET status = 'approved', updated_at = now() WHERE batch_id = $1 AND status = 'pending'").bind(id).execute(&mut *tx).await?;
    let mut after = json!({"status": "approved", "total": total.to_string()});
    if let Some((_, name)) = &first {
        after["firstApprovedBy"] = json!(name);
        after["secondApprovedBy"] = json!(actor.label());
    }
    audit::record(&mut *tx, tenant, actor, "batch.approve", Some(format!("batch:{id}")), Some(json!({"status": status})), Some(after), note).await?;
    tx.commit().await?;
    st.wake_payouts.notify_one();
    Ok(Approval::Released)
}

/// Rejects a batch (awaiting the first or the second approval; either approver may reject): its commissions go
/// back to the pending pool (a later batch picks them up), unless an admin rejects individual lines.
pub async fn reject(st: &AppState, tenant: &str, id: i64, actor: &Actor, note: &str) -> ApiResult<()> {
    let mut tx = st.pool.begin().await?;
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM payout_batches WHERE id = $1 AND tenant = $2 FOR UPDATE").bind(id).bind(tenant).fetch_optional(&mut *tx).await?;
    let status = match status.as_deref() {
        None => return Err(ApiError::NotFound),
        Some(s @ ("pending_approval" | "awaiting_second_approval")) => s.to_string(),
        Some(s) => return Err(ApiError::Conflict { code: "invalid_state", message: format!("This batch is {s}.") }),
    };
    sqlx::query("UPDATE payout_batches SET status = 'rejected', decided_by = $2, decided_by_id = $3, decided_at = now(), decision_note = $4 WHERE id = $1")
        .bind(id)
        .bind(actor.label())
        .bind(&actor.id)
        .bind(note)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE payouts SET status = 'rejected' WHERE batch_id = $1").bind(id).execute(&mut *tx).await?;
    let released = sqlx::query("UPDATE commissions SET batch_id = NULL, payout_id = NULL, updated_at = now() WHERE batch_id = $1 AND status = 'pending'").bind(id).execute(&mut *tx).await?.rows_affected();
    audit::record(&mut *tx, tenant, actor, "batch.reject", Some(format!("batch:{id}")), Some(json!({"status": status})), Some(json!({"status": "rejected", "released": released})), Some(note)).await?;
    tx.commit().await?;
    Ok(())
}

/// Puts failed payouts of a batch back in the transfer queue.
pub async fn retry(st: &AppState, tenant: &str, id: i64, actor: &Actor) -> ApiResult<u64> {
    let _ = batch_status(st, tenant, id).await?;
    let n = sqlx::query("UPDATE payouts SET status = 'transfer_pending', next_attempt_at = now(), last_error = NULL WHERE batch_id = $1 AND tenant = $2 AND status IN ('failed', 'transfer_pending')")
        .bind(id)
        .bind(tenant)
        .execute(&st.pool)
        .await?
        .rows_affected();
    sqlx::query("UPDATE payout_batches SET status = 'approved', completed_at = NULL WHERE id = $1 AND status = 'partially_paid'").bind(id).execute(&st.pool).await?;
    audit::record(&st.pool, tenant, actor, "batch.retry", Some(format!("batch:{id}")), None, Some(json!({"payouts": n})), None).await?;
    st.wake_payouts.notify_one();
    Ok(n)
}

/// Sends due payouts to the wallet. Returns (paid, still pending, failed).
pub async fn transfer_tick(st: &AppState) -> anyhow::Result<(usize, usize, usize)> {
    // lease due payouts (a concurrent tick skips them), so each one is sent by one caller at a time
    let due = sqlx::query(
        "UPDATE payouts p SET next_attempt_at = now() + interval '5 minutes' FROM payout_batches b
         WHERE b.id = p.batch_id AND p.id IN (
             SELECT id FROM payouts WHERE status = 'transfer_pending' AND (next_attempt_at IS NULL OR next_attempt_at <= now())
             ORDER BY id LIMIT 100 FOR UPDATE SKIP LOCKED)
         RETURNING p.id, p.tenant, p.batch_id, p.user_id, p.amount, p.idempotency_key, p.attempts, b.period_end",
    )
    .fetch_all(&st.pool)
    .await?;
    let (mut paid, mut pending, mut failed) = (0, 0, 0);
    let mut batches = std::collections::BTreeSet::new();
    for p in &due {
        let (id, tenant, batch, user): (i64, String, i64, i64) = (p.get("id"), p.get("tenant"), p.get("batch_id"), p.get("user_id"));
        let amount: D = p.get("amount");
        let key: String = p.get("idempotency_key");
        let period_end: DateTime<Utc> = p.get("period_end");
        let note = format!("Partner commission, period to {}", period_end.format("%d %b %Y"));
        batches.insert((tenant.clone(), batch));
        match clients::wallet_credit(st, &tenant, &key, user, amount, &format!("ib-batch-{batch}"), &note).await {
            WalletOutcome::Credited { txn } => {
                let mut tx = st.pool.begin().await?;
                let n = sqlx::query("UPDATE payouts SET status = 'paid', paid_at = now(), wallet_txn = $2, attempts = attempts + 1, last_error = NULL WHERE id = $1 AND status = 'transfer_pending'")
                    .bind(id)
                    .bind(&txn)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                if n == 0 {
                    continue;
                }
                sqlx::query("UPDATE commissions SET status = 'paid', updated_at = now() WHERE payout_id = $1 AND status = 'approved'").bind(id).execute(&mut *tx).await?;
                audit::record(&mut *tx, &tenant, &Actor::system(), "payout.paid", Some(format!("payout:{id}")), None, Some(json!({"user": user, "amount": amount.to_string(), "walletTxn": txn})), None).await?;
                tx.commit().await?;
                paid += 1;
            }
            WalletOutcome::Retry(e) => {
                let attempts: i32 = p.get("attempts");
                sqlx::query("UPDATE payouts SET attempts = attempts + 1, last_error = $2, next_attempt_at = now() + make_interval(secs => $3) WHERE id = $1")
                    .bind(id)
                    .bind(&e)
                    .bind(calc::backoff_secs(attempts) as f64)
                    .execute(&st.pool)
                    .await?;
                tracing::warn!(payout = id, error = %e, "wallet credit deferred; will retry");
                pending += 1;
            }
            WalletOutcome::Fail(e) => {
                sqlx::query("UPDATE payouts SET status = 'failed', attempts = attempts + 1, last_error = $2 WHERE id = $1").bind(id).bind(&e).execute(&st.pool).await?;
                audit::record(&st.pool, &tenant, &Actor::system(), "payout.failed", Some(format!("payout:{id}")), None, Some(json!({"error": e})), None).await?;
                tracing::error!(payout = id, error = %e, "wallet credit refused");
                failed += 1;
            }
        }
    }
    for (_, b) in batches {
        settle_batch(st, b).await?;
    }
    Ok((paid, pending, failed))
}

async fn settle_batch(st: &AppState, batch: i64) -> anyhow::Result<()> {
    let r = sqlx::query(
        "SELECT count(*) FILTER (WHERE status = 'transfer_pending') AS open, count(*) FILTER (WHERE status = 'failed') AS failed FROM payouts WHERE batch_id = $1",
    )
    .bind(batch)
    .fetch_one(&st.pool)
    .await?;
    let (open, failed): (i64, i64) = (r.get("open"), r.get("failed"));
    if open == 0 {
        let status = if failed > 0 { "partially_paid" } else { "paid" };
        sqlx::query("UPDATE payout_batches SET status = $2, completed_at = now() WHERE id = $1 AND status = 'approved'").bind(batch).bind(status).execute(&st.pool).await?;
    }
    Ok(())
}

/// Scheduler: once a period closes, create its batch for approval (at most once per period).
pub async fn auto_batch_tick(st: &AppState, tenant: &str) -> anyhow::Result<Option<i64>> {
    let s = db::settings(&st.pool, tenant).await?;
    if !s.payout.auto_create {
        return Ok(None);
    }
    let (start, end) = calc::last_period(&s.payout.schedule, s.payout.weekday, s.payout.month_day, Utc::now());
    let marker = format!("batch:{tenant}:{}:{}", s.payout.schedule, end.to_rfc3339());
    if db::cursor(&st.pool, &marker).await?.is_some() {
        return Ok(None);
    }
    // don't back-fill periods from before the programme started running
    let id = create_batch(st, tenant, &Actor::system(), Some(start), end).await?;
    db::set_cursor(&st.pool, &marker, &id.map(|i| i.to_string()).unwrap_or_else(|| "empty".into())).await?;
    Ok(id)
}

/// A batch for the Back Office. `threshold` = the programme's current four-eyes threshold (used while the batch
/// still waits for its first approval; afterwards the snapshot taken at the first approval is shown).
pub fn batch_json(r: &sqlx::postgres::PgRow, threshold: D) -> Value {
    let status: String = r.get("status");
    let total: D = r.get("total");
    let snapshot: Option<D> = r.get("four_eyes_threshold");
    let first_id: Option<String> = r.get("first_approved_by_id");
    let required = match status.as_str() {
        "pending_approval" => total >= threshold,
        _ => first_id.is_some(),
    };
    json!({
        "id": r.get::<i64, _>("id"),
        "schedule": r.get::<String, _>("schedule"),
        "periodStart": r.get::<Option<DateTime<Utc>>, _>("period_start"),
        "periodEnd": r.get::<DateTime<Utc>, _>("period_end"),
        "status": r.get::<String, _>("status"),
        "total": num(r.get::<D, _>("total")),
        "lines": r.get::<i32, _>("lines"),
        "payees": r.get::<i32, _>("payees"),
        "createdBy": r.get::<String, _>("created_by"),
        "createdAt": r.get::<DateTime<Utc>, _>("created_at"),
        "decidedBy": r.get::<Option<String>, _>("decided_by"),
        "decidedAt": r.get::<Option<DateTime<Utc>>, _>("decided_at"),
        "decisionNote": r.get::<Option<String>, _>("decision_note"),
        "completedAt": r.get::<Option<DateTime<Utc>>, _>("completed_at"),
        "decidedById": r.get::<Option<String>, _>("decided_by_id"),
        "fourEyes": {
            "required": required,
            "threshold": num(snapshot.unwrap_or(threshold)),
            // the staff id (without the `staff:` prefix); the Back Office compares it with the signed-in staff member
            "firstApprovedById": first_id.as_deref().map(|s| s.strip_prefix("staff:").unwrap_or(s)),
            "firstApprovedBy": r.get::<Option<String>, _>("first_approved_by_name"),
            "firstApprovedAt": r.get::<Option<DateTime<Utc>>, _>("first_approved_at"),
            "firstApprovalNote": r.get::<Option<String>, _>("first_approval_note"),
        },
    })
}

/// When the next scheduled batch closes (for "pays on …" hints).
pub fn next_close(schedule: &str, weekday: u32, month_day: u32) -> DateTime<Utc> {
    let now = Utc::now();
    let (_, last) = calc::last_period(schedule, weekday, month_day, now);
    let mut probe = last + Duration::days(1);
    loop {
        let (_, e) = calc::last_period(schedule, weekday, month_day, probe);
        if e > last {
            return e;
        }
        probe += Duration::days(1);
    }
}

pub fn zero_if_none(v: Option<D>) -> D {
    v.unwrap_or(ZERO)
}

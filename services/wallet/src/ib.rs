//! Reports real-money deposits to the IB service (`POST $IB_URL/v1/ib/events/deposit`), so a client who funds
//! the wallet counts as a depositor (first deposit, FTD, CPA) even before moving money to a trading account.
//!
//! Real money is exactly two wallet ledger kinds: `deposit` (on-chain USDT deposit credited, auto or after a
//! staff assign / approve) and `manual_deposit` (Back Office "Deposit (external payment received)"). Transfers
//! from trading, IB payouts, commissions, bonuses, refunds and every other adjustment are not deposits.
//!
//! `record` writes an `ib_deposit_events` row inside the money transaction (the outbox); this worker delivers
//! it after the commit, so an IB outage never blocks or rolls back a deposit. Each row is sent with
//! `idempotencyKey = wallet:deposit:<tenant_id>:<txn_id>` and `at` = the ledger txn time, so a retry is a
//! no-op on the IB side, and the IB keeps only the earliest deposit as the first one (the later engine
//! `transfer_in` of the same money never replaces it). Failures back off (15 s doubling, capped at 1 h) for
//! `MAX_ATTEMPTS`; an unknown client (404: the IB has not mirrored the sign-up yet) is retried, a refused
//! event (400 / 409 / 422) is not. `sweep` queues any real deposit that has no row yet (deposits booked
//! before the outbox existed, or by a path that forgot `record`): the reconciliation path.

use std::time::Duration;

use serde_json::{Value, json};
use sqlx::Row;
use tokio::sync::Notify;

use crate::money::{D, s};
use crate::state::AppState;

/// Woken by `record`; the worker also runs every `IDLE_SECS`.
pub static WAKE: Notify = Notify::const_new();
/// About two days of retries at the 1 h cap.
pub const MAX_ATTEMPTS: i32 = 60;
/// Wallet ledger kinds that are real money in (reported to the IB service).
pub const DEPOSIT_KINDS: &[&str] = &["deposit", "manual_deposit"];
const IDLE_SECS: u64 = 10;
const SWEEP_SECS: u64 = 600;
const BATCH: i64 = 50;

const SELECT_DEPOSIT: &str = "SELECT t.tenant_id, t.user_id, t.id, t.kind, t.currency, sum(p.amount), t.created_at
       FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id AND p.user_id = t.user_id";

pub fn idempotency_key(tenant_id: i64, txn_id: i64) -> String {
    format!("wallet:deposit:{tenant_id}:{txn_id}")
}

/// Seconds to wait before the next attempt after `attempts` failures.
pub fn backoff_secs(attempts: i32) -> i64 {
    (15i64 << attempts.clamp(0, 12)).min(3600)
}

/// Queues the IB deposit event for ledger txn `txn_id` inside the caller's transaction. Does nothing for a
/// txn that is not a real deposit, and once per txn (`txn_id` is unique).
pub async fn record<'e>(ex: impl sqlx::PgExecutor<'e>, txn_id: i64) -> sqlx::Result<()> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO ib_deposit_events (tenant_id, user_id, txn_id, kind, currency, amount, occurred_at)
         {SELECT_DEPOSIT}
          WHERE t.id = $1 AND t.kind = ANY($2) AND t.user_id IS NOT NULL
          GROUP BY t.id HAVING sum(p.amount) > 0
         ON CONFLICT (txn_id) DO NOTHING"
    )))
    .bind(txn_id)
    .bind(DEPOSIT_KINDS)
    .execute(ex)
    .await?;
    WAKE.notify_one();
    Ok(())
}

/// Queues every real deposit that has no outbox row yet. Returns how many were added.
pub async fn sweep(pool: &sqlx::PgPool) -> sqlx::Result<u64> {
    let n = sqlx::query(sqlx::AssertSqlSafe(format!(
        "INSERT INTO ib_deposit_events (tenant_id, user_id, txn_id, kind, currency, amount, occurred_at)
         {SELECT_DEPOSIT}
          WHERE t.kind = ANY($1) AND t.user_id IS NOT NULL
            AND NOT EXISTS (SELECT 1 FROM ib_deposit_events e WHERE e.txn_id = t.id)
          GROUP BY t.id HAVING sum(p.amount) > 0
         ON CONFLICT (txn_id) DO NOTHING"
    )))
    .bind(DEPOSIT_KINDS)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n)
}

enum Outcome {
    Sent(Option<bool>),
    /// `down`: the IB service itself failed (stop the pass); otherwise only this client (404) is not ready.
    Retry { error: String, down: bool },
    GiveUp(String),
}

async fn send(st: &AppState, http: &reqwest::Client, tenant: &str, r: &sqlx::postgres::PgRow) -> Outcome {
    let (tenant_id, txn_id): (i64, i64) = (r.get("tenant_id"), r.get("txn_id"));
    let body = json!({
        "userId": r.get::<i64, _>("user_id"),
        "amount": s(r.get::<D, _>("amount")),
        "currency": r.get::<String, _>("currency"),
        "at": r.get::<chrono::DateTime<chrono::Utc>, _>("occurred_at"),
        "idempotencyKey": idempotency_key(tenant_id, txn_id),
        "source": "wallet",
        "kind": r.get::<String, _>("kind"),
    });
    let res = http
        .post(format!("{}/v1/ib/events/deposit", st.cfg.ib_url))
        .header("x-kalks-internal", &st.cfg.ib_token)
        .header("x-kalks-tenant", tenant)
        .header("x-kalks-service", "wallet")
        .json(&body)
        .send()
        .await;
    match res {
        Ok(r) if r.status().is_success() => {
            let v: Value = r.json().await.unwrap_or(Value::Null);
            Outcome::Sent(v["firstDeposit"].as_bool())
        }
        // 404: the client is not mirrored in the IB service yet (gateway sync); 401 / 403: token not set up yet
        Ok(r) if matches!(r.status().as_u16(), 400 | 409 | 422) => Outcome::GiveUp(format!("HTTP {}", r.status())),
        Ok(r) => Outcome::Retry { error: format!("HTTP {}", r.status()), down: r.status() != 404 },
        Err(e) => Outcome::Retry { error: if e.is_timeout() { "timeout".into() } else { "connection failed".into() }, down: true },
    }
}

/// One pass: sends due deposit events. Returns (sent, failed).
pub async fn push_pending(st: &AppState, http: &reqwest::Client) -> anyhow::Result<(usize, usize)> {
    if st.cfg.ib_url.is_empty() {
        return Ok((0, 0));
    }
    let rows = sqlx::query(
        "SELECT id, tenant_id, user_id, txn_id, kind, currency, amount, occurred_at, push_attempts FROM ib_deposit_events
          WHERE pushed_at IS NULL AND push_attempts < $1 AND (push_next_at IS NULL OR push_next_at <= now())
          ORDER BY id LIMIT $2",
    )
    .bind(MAX_ATTEMPTS)
    .bind(BATCH)
    .fetch_all(&st.pool)
    .await?;
    let (mut sent, mut failed) = (0, 0);
    for r in &rows {
        let id: i64 = r.get("id");
        let tenant_id: i64 = r.get("tenant_id");
        let tenant = match st.tenants.slug_of(tenant_id) {
            Some(s) => s,
            None => {
                let _ = st.tenants.reload(&st.pool).await;
                st.tenants.slug_of(tenant_id).unwrap_or_else(|| "kalks".into())
            }
        };
        match send(st, http, &tenant, r).await {
            Outcome::Sent(first) => {
                sqlx::query("UPDATE ib_deposit_events SET pushed_at = now(), push_attempts = push_attempts + 1, push_error = NULL, first_deposit = $2 WHERE id = $1")
                    .bind(id)
                    .bind(first)
                    .execute(&st.pool)
                    .await?;
                sent += 1;
            }
            Outcome::Retry { error: e, down } => {
                let attempts: i32 = r.get("push_attempts");
                sqlx::query("UPDATE ib_deposit_events SET push_attempts = push_attempts + 1, push_error = $2, push_next_at = now() + make_interval(secs => $3) WHERE id = $1")
                    .bind(id)
                    .bind(&e)
                    .bind(backoff_secs(attempts) as f64)
                    .execute(&st.pool)
                    .await?;
                if attempts + 1 >= MAX_ATTEMPTS {
                    tracing::error!(event = id, error = %e, "IB deposit event gave up after retries");
                } else {
                    tracing::warn!(event = id, error = %e, "IB deposit event failed; will retry");
                }
                failed += 1;
                // the IB service is down: leave the rest of the batch for the next pass
                if down {
                    break;
                }
            }
            Outcome::GiveUp(e) => {
                sqlx::query("UPDATE ib_deposit_events SET push_attempts = $2, push_error = $3 WHERE id = $1").bind(id).bind(MAX_ATTEMPTS).bind(&e).execute(&st.pool).await?;
                tracing::error!(event = id, error = %e, "IB service refused the deposit event");
                failed += 1;
            }
        }
    }
    Ok((sent, failed))
}

/// Delivery loop (workers instance only): sweeps for unreported deposits at start and every `SWEEP_SECS`,
/// pushes right after a wake-up (a short pause lets the writing transaction commit) and every `IDLE_SECS`.
pub fn spawn(st: AppState) {
    if st.cfg.ib_url.is_empty() {
        tracing::warn!("IB_URL is empty: deposits are not reported to the IB service (they are queued and sent once it is set)");
        return;
    }
    tokio::spawn(async move {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().expect("http client");
        let mut last_sweep: Option<std::time::Instant> = None;
        loop {
            if last_sweep.is_none_or(|t| t.elapsed() >= Duration::from_secs(SWEEP_SECS)) {
                match sweep(&st.pool).await {
                    Ok(n) if n > 0 => tracing::info!(queued = n, "unreported deposits queued for the IB service"),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, "IB deposit sweep failed"),
                }
                last_sweep = Some(std::time::Instant::now());
            }
            match push_pending(&st, &http).await {
                Ok((sent, _)) if sent > 0 => tracing::info!(sent, "deposits reported to the IB service"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "IB deposit push pass failed"),
            }
            tokio::select! {
                _ = WAKE.notified() => tokio::time::sleep(Duration::from_millis(300)).await,
                _ = tokio::time::sleep(Duration::from_secs(IDLE_SECS)) => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_backoff() {
        assert_eq!(idempotency_key(1, 381), "wallet:deposit:1:381");
        assert_eq!(backoff_secs(0), 15);
        assert_eq!(backoff_secs(2), 60);
        assert_eq!(backoff_secs(30), 3600);
        assert!(DEPOSIT_KINDS.contains(&"deposit") && DEPOSIT_KINDS.contains(&"manual_deposit"));
        for k in ["ib_payout", "commission", "refund", "adjustment_in", "transfer_from_trading", "bonus"] {
            assert!(!DEPOSIT_KINDS.contains(&k), "{k} is not a deposit");
        }
    }
}

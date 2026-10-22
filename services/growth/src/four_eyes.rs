//! Four-eyes (maker-checker) on marketing money out: contest prizes, mission cash / credit rewards, cashback payouts
//! and loyalty cashback redemptions (migration 0006). A payout whose USD amount is at or above the broker's
//! `fourEyesThreshold` (default 1000, 0 = every payout) needs two different staff members: the first approval moves
//! it to `awaiting_second_approval` (nothing is paid), the second staff member's approval releases it. The same staff
//! member is refused with 409 `four_eyes_same_staff`. Auto-pay never releases an amount at or above the threshold.
//!
//! The transitions are conditional UPDATEs (`WHERE <status> = ...`), so concurrent or repeated approvals release a
//! payout once; the wallet / engine idempotency keys stay as they were.

use crate::audit::Actor;
use crate::error::ApiError;
use crate::money::{D, ZERO, num};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;

pub const AWAITING: &str = "awaiting_second_approval";

/// Staff id as sent in `X-Kalks-Staff-Id` (`staff:7` → `7`).
pub fn staff_id(actor: &Actor) -> String {
    actor.id.strip_prefix("staff:").unwrap_or(&actor.id).to_string()
}

/// Whether a payout of `amount` needs a second approver (threshold 0 = always).
pub fn needs_two(amount: D, threshold: D) -> bool {
    amount >= threshold
}

/// What auto-pay may release: amounts below min(autoPayBelow, fourEyesThreshold). `None` = auto-pay off.
pub fn auto_pay_limit(auto_pay_below: D, threshold: D) -> Option<D> {
    if auto_pay_below <= ZERO {
        return None;
    }
    let limit = auto_pay_below.min(threshold);
    if limit <= ZERO { None } else { Some(limit) }
}

pub fn same_staff() -> ApiError {
    ApiError::Conflict { code: "four_eyes_same_staff", message: "Four-eyes rule: another staff member must give the second approval.".into() }
}

/// `fourEyes` for a row carrying the first-approval columns. `threshold` = the broker's current one (when known):
/// `required` is true when the amount is at or above it, or the payout already has a first approval.
pub fn json_of(r: &sqlx::postgres::PgRow, amount: Option<D>, status: &str, threshold: Option<D>) -> Value {
    let first_id: Option<String> = r.try_get("first_approved_by_id").ok().flatten();
    let snap: Option<D> = r.try_get("four_eyes_threshold").ok().flatten();
    let required = status == AWAITING || first_id.is_some() || matches!((amount, threshold), (Some(a), Some(t)) if needs_two(a, t));
    json!({
        "required": required,
        "threshold": snap.or(threshold).map(num),
        "firstApprovedById": first_id.as_deref().map(|s| s.strip_prefix("staff:").unwrap_or(s)),
        "firstApprovedBy": r.try_get::<Option<String>, _>("first_approved_by_name").ok().flatten(),
        "firstApprovedAt": r.try_get::<Option<DateTime<Utc>>, _>("first_approved_at").ok().flatten(),
        "firstApprovalNote": r.try_get::<Option<String>, _>("first_approval_note").ok().flatten(),
    })
}

/// First approval: `from` → `awaiting_second_approval` with the approver and the threshold in force. False when the
/// row was no longer in `from` (someone else decided it meanwhile).
#[allow(clippy::too_many_arguments)]
pub async fn record_first(tx: &mut sqlx::PgConnection, table: &'static str, status_col: &'static str, id: i64, from: &str, actor: &Actor, note: Option<&str>, threshold: D) -> anyhow::Result<bool> {
    let sql = format!(
        "UPDATE {table} SET {status_col} = 'awaiting_second_approval', first_approved_by_id = $2, first_approved_by_name = $3, first_approved_at = now(),
                first_approval_note = $4, four_eyes_threshold = $5 WHERE id = $1 AND {status_col} = $6"
    );
    let n = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(staff_id(actor))
        .bind(actor.name.clone().unwrap_or_else(|| actor.id.clone()))
        .bind(note.map(str::trim).filter(|n| !n.is_empty()))
        .bind(threshold)
        .bind(from)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    Ok(n == 1)
}

/// Second approval: `awaiting_second_approval` → `to`, only by a staff member other than the first approver. False
/// when it was not waiting any more (released or rejected meanwhile) or the caller is the first approver.
pub async fn claim_second(tx: &mut sqlx::PgConnection, table: &'static str, status_col: &'static str, id: i64, actor: &Actor, to: &str) -> anyhow::Result<bool> {
    let sql = format!("UPDATE {table} SET {status_col} = $3 WHERE id = $1 AND {status_col} = 'awaiting_second_approval' AND first_approved_by_id IS DISTINCT FROM $2");
    let n = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(id).bind(staff_id(actor)).bind(to).execute(&mut *tx).await?.rows_affected();
    Ok(n == 1)
}

/// The two approvers for the release audit.
pub fn approvers(first_id: Option<&str>, first_name: Option<&str>, actor: &Actor) -> Value {
    json!({
        "firstApprovedById": first_id, "firstApprovedBy": first_name,
        "secondApprovedById": staff_id(actor), "secondApprovedBy": actor.name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::dec;

    #[test]
    fn threshold_and_auto_pay_limit() {
        assert!(needs_two(dec("1000"), dec("1000")), "at the threshold");
        assert!(!needs_two(dec("999.99"), dec("1000")));
        assert!(needs_two(dec("0.01"), ZERO), "0 = every payout");
        assert_eq!(auto_pay_limit(ZERO, dec("1000")), None, "auto-pay off");
        assert_eq!(auto_pay_limit(dec("5000"), dec("1000")), Some(dec("1000")), "clamped to the four-eyes threshold");
        assert_eq!(auto_pay_limit(dec("100"), dec("1000")), Some(dec("100")));
        assert_eq!(auto_pay_limit(dec("100"), ZERO), None, "threshold 0: nothing is auto-paid");
        assert_eq!(staff_id(&Actor { id: "staff:7".into(), name: None }), "7");
    }
}

//! Erasing a closed client's personal data (GDPR right to erasure).
//!
//! `POST /v1/admin/users/{id}/erase` `{reason, confirm: "ERASE"}` needs `clients.erase`, which only the Super Admin
//! and the Platform Owner hold (`rbac::SUPER_ADMIN_ONLY`). The client must be closed first (a completed closure
//! request, `client_security::admin_process_request`); an active or blocked client is refused with 409
//! `client_not_closed`. The call is idempotent: a client already erased answers `{already_erased: true}`.
//!
//! A regulated broker must keep trading, wallet, IB and audit records for years, so the client is never deleted.
//! The gateway anonymises everything personal it holds and keeps the user id, so the records other services keep
//! under that id (trading accounts, deals, wallet ledger, IB commissions, prop, copy trading) stay consistent.
//!
//! Erased:
//! * `users`: email → `erased-<id>@erased.invalid` (unique per tenant, on the reserved `.invalid` domain);
//!   password_hash → `!erased` (not a PHC string, so no password ever matches); first_name / last_name →
//!   `Erased` / `client`; phone_dial / phone → empty; date_of_birth → 1900-01-01; google_sub, google_linked_at,
//!   avatar_url → NULL (no Google sign-in, no photo); utm_source / utm_medium / utm_campaign / utm_term /
//!   utm_content, landing_page, first_referrer → NULL; marketing_consent → false (with the time); failed_logins and
//!   locked_until reset; erased_at set. Name and date of birth are changed with `kalks.identity_unlock` (D92).
//! * `sessions` (every client, view-only and staff session of the client, with IP / user agent / country),
//!   `email_otps`, `trusted_devices`, `stepup_tokens`, `impersonation_tickets`, `client_presence` (Kalks Trader
//!   connections with IP / user agent) and `client_viewers` (view-only logins): deleted.
//! * KYC: every document file on disk and the `kyc_cases` rows with their documents, events and reviewer notes
//!   (`kyc::erase_user`, the same as `gateway kyc-erase-user`).
//! * `suitability`: the IP and user agent of the options disclosure acceptance (the accepted version, time and quiz
//!   result stay as the regulatory record).
//! * `client_requests`: the client's free-text reason.
//!
//! Kept, on purpose:
//! * `users.id`, `tenant_id`, `referral_code`, `referred_by`, `referred_code_raw`, `referral_campaign`: the IB tree
//!   and commissions are keyed by them.
//! * `users.country`, `kyc_status`, `identity_locked_at`, `status` (`closed`), `terms_accepted_at`, created / last
//!   sign-in / last activity times, `is_house`: not identifying on their own, and needed for jurisdiction reporting
//!   and the record of what the client was.
//! * `audit_log`: append-only by design (the database rejects UPDATE / DELETE) and a legal record; the erasure
//!   itself is written to it with the staff id, the reason and counts, and no personal data.
//! * `client_restrictions` (staff reasons) and the `client_requests` rows (kind, status, staff note): staff records.
//! * `trade_shares`: keyed by trading account login, not by user; public trade snapshots without contact data.
//! * Data held by other services (trading, wallet, IB, support, growth) under the user id: they receive the
//!   anonymised profile through the users feed (`updated_at` moves); erasing their own personal fields is theirs.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;

use crate::admin::require_key;
use crate::audit::{self, Entry};
use crate::client_auth::body;
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, Ctx};

/// The word staff type to confirm.
pub const CONFIRM_WORD: &str = "ERASE";

/// The anonymised login email of an erased client: unique per tenant, never deliverable.
pub fn erased_email(id: i64) -> String {
    format!("erased-{id}@erased.invalid")
}

#[derive(Deserialize)]
pub struct EraseReq {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    confirm: String,
}

/// What an erasure removed (for the audit entry and the response).
#[derive(Default)]
pub struct Erased {
    pub sessions: u64,
    pub viewers: u64,
    pub kyc_files: usize,
}

fn not_closed() -> ApiError {
    ApiError::Coded {
        status: StatusCode::CONFLICT,
        code: "client_not_closed",
        message: "Only a closed client can be erased. Complete the client's closure request first.",
    }
}

/// `POST /v1/admin/users/{id}/erase` `{reason, confirm: "ERASE"}`.
pub async fn erase(State(st): State<AppState>, ctx: Ctx, Path(id): Path<i64>, req: Result<Json<EraseReq>, JsonRejection>) -> ApiResult<Json<Value>> {
    let r = body(req)?;
    let me = require_key(&st, &ctx, "clients.erase").await?;
    // defence in depth: rbac::effective already keeps the key away from every other role
    if !(me.role == "super_admin" || me.is_owner()) {
        return Err(ApiError::Forbidden);
    }
    let reason: String = r.reason.trim().chars().filter(|c| !c.is_control()).take(500).collect();
    if reason.chars().count() < 3 {
        return Err(ApiError::Validation { field: "reason", message: "Give a reason (at least 3 characters), e.g. the request or ticket number." });
    }
    if r.confirm.trim() != CONFIRM_WORD {
        return Err(ApiError::Validation { field: "confirm", message: "Type ERASE to confirm." });
    }
    let row = sqlx::query("SELECT status, is_house, erased_at FROM users WHERE id = $1 AND tenant_id = $2")
        .bind(id)
        .bind(me.tenant_id)
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::NotFound)?;
    if row.get::<bool, _>("is_house") {
        return Err(ApiError::NotFound);
    }
    if let Some(at) = row.get::<Option<DateTime<Utc>>, _>("erased_at") {
        return Ok(Json(json!({ "status": "ok", "already_erased": true, "erased_at": at })));
    }
    if row.get::<String, _>("status") != "closed" {
        return Err(not_closed());
    }
    let Some(done) = erase_personal_data(&st.pool, id).await? else {
        // erased (or reopened) by a concurrent call between the check and the lock
        let at: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT erased_at FROM users WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
        return match at {
            Some(at) => Ok(Json(json!({ "status": "ok", "already_erased": true, "erased_at": at }))),
            None => Err(not_closed()),
        };
    };
    audit::record(&st.pool, &ctx, Entry {
        tenant_id: me.tenant_id,
        actor_kind: "staff",
        actor_id: Some(me.id),
        action: "admin.client_erased",
        target: Some(("user", id)),
        meta: json!({ "reason": reason, "sessions_deleted": done.sessions, "viewers_deleted": done.viewers, "kyc_files_deleted": done.kyc_files }),
    })
    .await;
    let at: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT erased_at FROM users WHERE id = $1").bind(id).fetch_one(&st.pool).await?;
    Ok(Json(json!({
        "status": "ok",
        "already_erased": false,
        "erased_at": at,
        "removed": { "sessions": done.sessions, "viewers": done.viewers, "kyc_files": done.kyc_files },
    })))
}

/// Anonymises a closed client (see the module docs for the full list). Returns None, without changing anything in
/// the database, when the client is not closed or already erased. KYC files go first: removing them is idempotent,
/// so a failure part-way leaves the client unerased and a retry finishes the job.
pub async fn erase_personal_data(pool: &sqlx::PgPool, user_id: i64) -> anyhow::Result<Option<Erased>> {
    const ERASABLE: &str = "SELECT status = 'closed' AND erased_at IS NULL AND NOT is_house FROM users WHERE id = $1";
    let ok: Option<bool> = sqlx::query_scalar(ERASABLE).bind(user_id).fetch_optional(pool).await?;
    if ok != Some(true) {
        return Ok(None);
    }
    let kyc_files = crate::kyc::erase_user(pool, user_id).await?;
    let mut tx = pool.begin().await?;
    let ok: Option<bool> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("{ERASABLE} FOR UPDATE"))).bind(user_id).fetch_optional(&mut *tx).await?;
    if ok != Some(true) {
        return Ok(None);
    }
    // name and date of birth are locked after identity verification (D92); erasure is the deliberate exception
    sqlx::query("SET LOCAL kalks.identity_unlock = 'on'").execute(&mut *tx).await?;
    sqlx::query(
        "UPDATE users SET email = $2, password_hash = '!erased', first_name = 'Erased', last_name = 'client',
                phone_dial = '', phone = '', date_of_birth = DATE '1900-01-01',
                google_sub = NULL, google_linked_at = NULL, avatar_url = NULL,
                utm_source = NULL, utm_medium = NULL, utm_campaign = NULL, utm_term = NULL, utm_content = NULL,
                landing_page = NULL, first_referrer = NULL,
                marketing_consent = false, marketing_consent_at = now(), marketing_unsubscribed_at = COALESCE(marketing_unsubscribed_at, now()),
                failed_logins = 0, locked_until = NULL, erased_at = now(), updated_at = now()
         WHERE id = $1",
    )
    .bind(user_id)
    .bind(erased_email(user_id))
    .execute(&mut *tx)
    .await?;
    let sessions = sqlx::query("DELETE FROM sessions WHERE subject_kind = 'user' AND subject_id = $1").bind(user_id).execute(&mut *tx).await?.rows_affected();
    sqlx::query("DELETE FROM email_otps WHERE subject_kind = 'user' AND subject_id = $1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM trusted_devices WHERE subject_kind = 'user' AND subject_id = $1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM stepup_tokens WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM impersonation_tickets WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM client_presence WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    let viewers = sqlx::query("DELETE FROM client_viewers WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?.rows_affected();
    sqlx::query("UPDATE suitability SET ip = NULL, user_agent = NULL, updated_at = now() WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE client_requests SET reason = NULL WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(Some(Erased { sessions, viewers, kyc_files }))
}

#[cfg(test)]
mod tests;

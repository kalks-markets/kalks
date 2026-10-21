//! Back Office "Balance & credit": manual adjustments of a client's wallet or trading account.
//!
//! | Operation | Wallet (USDT) | Trading account (engine) |
//! |---|---|---|
//! | Add funds | `manual_deposit` (reason "Deposit (external payment received)") or `adjustment_in` | `deposit` or `adjustment` |
//! | Deduct funds | `manual_withdrawal` (reason "Withdrawal (paid externally)") or `adjustment_out` | `withdrawal` or `adjustment` |
//! | Give / take credit | refused: credit exists on trading accounts only | `credit` ± |
//!
//! Only the two "external" reasons are real money (reports count them as deposits / withdrawals and FTDs); every
//! other reason is an adjustment and never a deposit.
//!
//! Flow: the request is validated (permissions, amounts, limits via a dry run), then recorded in `adjustments`
//! with a per-submit idempotency key (a double click books once). Above the tenant's four-eyes threshold
//! (`tenant_settings.adjust_approval_usd`, USD) it waits as `pending` until another staff member with
//! `finance.adjust_approve` approves it; below, it applies at once. Wallet targets post to this ledger in the same
//! transaction as the row update, audit entry and client notification. Trading targets are booked by the engine
//! (`POST /v1/admin/accounts/{login}/adjust`, key `wallet-adj-<tenant>-<id>`, idempotent): the row is
//! `processing` while the call is in flight and the recovery loop re-sends the same key if the answer was lost.
//!
//! Limits: a wallet can never go below 0 (the balance CHECK). A trading-account deduction is limited to the
//! free own funds (`withdrawable`) and taking credit back to the free margin; `force` (finance.adjust_force,
//! Super Admin) lifts the free-margin limit, but negative balance protection is always on, so a forced
//! deduction can take at most the balance and credit taken back at most the credit held.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use sqlx::postgres::PgRow;

use super::{clean_text, notify};
use crate::audit::{self, Entry};
use crate::engine::{EngineError, StaffHeaders};
use crate::error::{ApiError, ApiResult};
use crate::ledger::{self, Leg, NewTxn, PostError};
use crate::money::{D, ENGINE_DP, WALLET_DP, check_amount, s, s_opt};
use crate::state::{AppState, ROLES_APPROVE, ROLES_FORCE, ROLES_WRITE, StaffCtx};

pub const OPS: &[&str] = &["add", "deduct", "credit_in", "credit_out"];
pub const CATEGORIES: &[&str] = &["deposit", "withdrawal", "correction", "compensation", "bonus", "fee", "chargeback", "other"];
/// Wallet ledger kinds booked by manual adjustments (shown in the client's wallet history).
pub const LEDGER_KINDS: &[&str] = &["manual_deposit", "manual_withdrawal", "adjustment_in", "adjustment_out"];

pub fn category_label(c: &str) -> &'static str {
    match c {
        "deposit" => "Deposit (external payment received)",
        "withdrawal" => "Withdrawal (paid externally)",
        "correction" => "Correction",
        "compensation" => "Compensation",
        "bonus" => "Bonus",
        "fee" => "Fee",
        "chargeback" => "Chargeback",
        _ => "Other",
    }
}

/// Reason code recorded on the engine ledger / audit (`ADJ-COR · Correction`).
fn reason_code(c: &str) -> String {
    let short = match c {
        "deposit" => "DEP",
        "withdrawal" => "WDR",
        "correction" => "COR",
        "compensation" => "CMP",
        "bonus" => "BON",
        "fee" => "FEE",
        "chargeback" => "CHB",
        _ => "OTH",
    };
    let label = match c {
        "deposit" => "External deposit",
        "withdrawal" => "External withdrawal",
        other => category_label(other),
    };
    format!("ADJ-{short} · {label}")
}

/// Wallet ledger kind and counter-account for an add / deduct.
fn wallet_kind(op: &str, category: &str) -> (&'static str, &'static str) {
    match (op, category) {
        ("add", "deposit") => ("manual_deposit", "manual_deposit"),
        ("deduct", "withdrawal") => ("manual_withdrawal", "manual_withdrawal"),
        ("add", _) => ("adjustment_in", "adjustment"),
        _ => ("adjustment_out", "adjustment"),
    }
}

/// A new adjustment as submitted by the Back Office.
#[derive(Clone, Debug)]
pub struct NewAdjustment {
    pub idempotency_key: String,
    pub user_id: i64,
    /// "wallet" | "trading"
    pub target: String,
    pub login: Option<i64>,
    pub currency: Option<String>,
    pub op: String,
    pub category: String,
    pub amount: D,
    pub comment: String,
    pub client_note: Option<String>,
    pub notify: bool,
    pub force: bool,
}

/// What the request resolves to: currency, USD value, and (trading) the account view.
struct Resolved {
    currency: String,
    amount: D,
    amount_usd: D,
    account: Option<Value>,
    account_type: Option<String>,
}

pub async fn threshold(st: &AppState, tenant_id: i64) -> sqlx::Result<Option<D>> {
    let v: Option<Option<D>> = sqlx::query_scalar("SELECT adjust_approval_usd FROM tenant_settings WHERE tenant_id = $1").bind(tenant_id).fetch_optional(&st.pool).await?;
    Ok(v.flatten())
}

fn staff_headers(id: &str, name: &str, role: &str, perms: Option<Vec<String>>) -> StaffHeaders {
    StaffHeaders { id: id.into(), name: name.into(), role: role.into(), perms }
}

fn engine_err(e: EngineError) -> ApiError {
    match e {
        EngineError::Rejected { status, code, message } => ApiError::Coded {
            status: axum::http::StatusCode::from_u16(if status == 409 { 409 } else if status == 404 { 404 } else if status == 403 { 403 } else { 422 }).unwrap_or(axum::http::StatusCode::UNPROCESSABLE_ENTITY),
            code: leak(code),
            message,
        },
        EngineError::Unavailable(_) => ApiError::Coded { status: axum::http::StatusCode::SERVICE_UNAVAILABLE, code: "engine_unavailable", message: "The trading engine is not answering. Please try again shortly.".into() },
    }
}

/// Engine error codes are a small closed set; map the known ones to static strings.
fn leak(code: String) -> &'static str {
    const KNOWN: &[&str] = &[
        "insufficient_funds", "insufficient_credit", "negative_balance", "invalid_amount", "invalid_category", "pamm_account", "not_found", "forbidden",
        "validation", "idempotency_conflict", "demo_account", "account_status",
    ];
    KNOWN.iter().find(|k| **k == code).copied().unwrap_or("engine_rejected")
}

/// Permission for an operation: add / deduct need finance.adjust, credit needs finance.credit, force needs
/// finance.adjust_force (Super Admin).
fn check_perms(s_ctx: &StaffCtx, op: &str, force: bool) -> ApiResult<()> {
    if op.starts_with("credit") {
        s_ctx.require_perm("finance.credit", ROLES_WRITE)?;
    } else {
        s_ctx.require_perm("finance.adjust", ROLES_WRITE)?;
    }
    if force {
        s_ctx.require_perm("finance.adjust_force", ROLES_FORCE)?;
    }
    Ok(())
}

/// Shape checks that need no IO.
fn validate(r: &NewAdjustment) -> ApiResult<()> {
    if r.user_id <= 0 {
        return Err(ApiError::validation("user_id", "Choose a client"));
    }
    if !OPS.contains(&r.op.as_str()) {
        return Err(ApiError::validation("op", "Choose add funds, deduct funds, give credit or take credit"));
    }
    if !CATEGORIES.contains(&r.category.as_str()) {
        return Err(ApiError::validation("category", "Choose a reason"));
    }
    if r.op != "add" && r.category == "deposit" {
        return Err(ApiError::validation("category", "“Deposit (external payment received)” can only add funds"));
    }
    if r.op != "deduct" && r.category == "withdrawal" {
        return Err(ApiError::validation("category", "“Withdrawal (paid externally)” can only deduct funds"));
    }
    match r.target.as_str() {
        "wallet" => {
            if r.op.starts_with("credit") {
                return Err(ApiError::validation("op", "Credit exists on trading accounts only; choose a trading account"));
            }
            if r.login.is_some() {
                return Err(ApiError::validation("login", "A wallet adjustment has no trading account"));
            }
            if r.force {
                return Err(ApiError::validation("force", "A wallet can never go below 0; force applies to trading accounts only"));
            }
        }
        "trading" => {
            if r.login.is_none_or(|l| l <= 0) {
                return Err(ApiError::validation("login", "Choose a trading account"));
            }
            if r.force && !matches!(r.op.as_str(), "deduct" | "credit_out") {
                return Err(ApiError::validation("force", "Force applies to deductions and taking credit back only"));
            }
        }
        _ => return Err(ApiError::validation("target", "target must be wallet or trading")),
    }
    Ok(())
}

async fn resolve(st: &AppState, s_ctx: &StaffCtx, r: &NewAdjustment) -> ApiResult<Resolved> {
    if r.target == "wallet" {
        let ccy = r.currency.clone().unwrap_or_else(|| "USDT".into()).trim().to_uppercase();
        if !super::transfers::CURRENCIES.contains(&ccy.as_str()) {
            return Err(ApiError::validation("currency", "The wallet holds USDT"));
        }
        let amount = check_amount(r.amount, WALLET_DP).map_err(|m| ApiError::validation("amount", m))?;
        return Ok(Resolved { currency: ccy, amount, amount_usd: amount, account: None, account_type: None });
    }
    let login = r.login.unwrap_or_default();
    let views = st.engine.account_views(&s_ctx.ctx.tenant.slug, r.user_id).await.map_err(engine_err)?;
    let acc = views.into_iter().find(|v| v["login"].as_i64() == Some(login)).ok_or_else(|| ApiError::validation("login", format!("Account {login} does not belong to this client")))?;
    let amount = check_amount(r.amount, ENGINE_DP).map_err(|m| ApiError::validation("amount", m))?;
    let cent = acc["cent"].as_bool().unwrap_or(false) || acc["currency"].as_str() == Some("USC");
    let amount_usd = if cent { amount / D::from(100) } else { amount };
    Ok(Resolved {
        currency: acc["currency"].as_str().unwrap_or("USD").to_string(),
        amount,
        amount_usd: amount_usd.normalize(),
        account_type: acc["type"].as_str().map(str::to_string),
        account: Some(acc),
    })
}

fn engine_body(row_op: &str, category: &str, amount: D, force: bool, statement: &str, comment: &str, key: Option<String>, dry: bool, approved_by: Option<Value>, request_id: Option<i64>) -> Value {
    json!({
        "op": row_op, "category": category, "amount": s(amount), "force": force, "statementNote": statement,
        "reasonCode": reason_code(category), "note": comment, "idempotencyKey": key.unwrap_or_default(), "dryRun": dry,
        "approvedBy": approved_by, "requestId": request_id,
    })
}

/// Wallet balances of a user in `ccy`: (available, locked).
async fn wallet_balance(st: &AppState, tenant_id: i64, user_id: i64, ccy: &str) -> sqlx::Result<(D, D)> {
    let r: Option<(D, D)> = sqlx::query_as("SELECT available, locked FROM wallet_balances WHERE tenant_id = $1 AND user_id = $2 AND currency = $3").bind(tenant_id).bind(user_id).bind(ccy).fetch_optional(&st.pool).await?;
    Ok(r.unwrap_or_default())
}

fn wallet_snap(ccy: &str, a: D, l: D) -> Value {
    json!({"available": s(a), "locked": s(l), "balance": s(a + l), "currency": ccy})
}

/// Before → after of an adjustment without booking anything, plus whether it needs a second approval.
pub async fn preview(st: &AppState, s_ctx: &StaffCtx, r: &NewAdjustment) -> ApiResult<Value> {
    validate(r)?;
    check_perms(s_ctx, &r.op, r.force)?;
    let res = resolve(st, s_ctx, r).await?;
    let t = s_ctx.ctx.tenant.id;
    let thr = threshold(st, t).await?;
    let needs = thr.is_some_and(|x| res.amount_usd > x);
    let mut out = if r.target == "wallet" {
        let (a, l) = wallet_balance(st, t, r.user_id, &res.currency).await?;
        let after = if r.op == "add" { a + res.amount } else { a - res.amount };
        let mut v = json!({"ok": after >= D::ZERO, "before": wallet_snap(&res.currency, a, l), "limits": {"max": if r.op == "deduct" { json!(s(a)) } else { Value::Null }, "maxForced": Value::Null}});
        if after >= D::ZERO {
            v["after"] = wallet_snap(&res.currency, after, l);
        } else {
            let held = if l > D::ZERO { format!(" ({} {} is held by pending withdrawals / transfers)", s(l), res.currency) } else { String::new() };
            v["error"] = json!({"code": "insufficient_funds", "message": format!("Deducting {} {} exceeds the wallet's available balance of {} {}{held}", s(res.amount), res.currency, s(a), res.currency)});
        }
        v
    } else {
        let body = engine_body(&r.op, &r.category, res.amount, r.force, r.client_note.as_deref().unwrap_or(""), &r.comment, None, true, None, None);
        let staff = staff_headers(&s_ctx.staff.id, &s_ctx.staff.name, &s_ctx.staff.role, s_ctx.staff.perms.clone());
        st.engine.adjust(&s_ctx.ctx.tenant.slug, r.login.unwrap_or_default(), body, &staff).await.map_err(engine_err)?
    };
    out["target"] = json!(r.target);
    out["login"] = json!(r.login);
    out["currency"] = json!(res.currency);
    out["amount"] = json!(s(res.amount));
    out["amount_usd"] = json!(s(res.amount_usd));
    out["threshold_usd"] = s_opt(thr);
    out["needs_approval"] = json!(needs);
    out["account_type"] = json!(res.account_type);
    if let Some(a) = &res.account {
        out["account"] = a.clone();
    }
    Ok(out)
}

pub fn row_json(r: &PgRow) -> Value {
    let err_code: Option<String> = r.get("error_code");
    json!({
        "id": r.get::<i64, _>("id"),
        "user_id": r.get::<i64, _>("user_id"),
        "target": r.get::<String, _>("target"),
        "login": r.get::<Option<i64>, _>("login"),
        "account_type": r.get::<Option<String>, _>("account_type"),
        "currency": r.get::<String, _>("currency"),
        "op": r.get::<String, _>("op"),
        "category": r.get::<String, _>("category"),
        "category_label": category_label(&r.get::<String, _>("category")),
        "amount": s(r.get::<D, _>("amount")),
        "amount_usd": s(r.get::<D, _>("amount_usd")),
        "comment": r.get::<String, _>("comment"),
        "client_note": r.get::<Option<String>, _>("client_note"),
        "notify": r.get::<bool, _>("notify"),
        "force": r.get::<bool, _>("force"),
        "status": r.get::<String, _>("status"),
        "requested_by": {"id": r.get::<String, _>("requested_by_id"), "name": r.get::<String, _>("requested_by_name"), "role": r.get::<String, _>("requested_by_role")},
        "decided_by": r.get::<Option<String>, _>("decided_by_id").map(|id| json!({"id": id, "name": r.get::<Option<String>, _>("decided_by_name")})),
        "decided_at": r.get::<Option<DateTime<Utc>>, _>("decided_at"),
        "decision_note": r.get::<Option<String>, _>("decision_note"),
        "before": r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0),
        "after": r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0),
        "txn_id": r.get::<Option<i64>, _>("txn_id"),
        "ledger_kind": r.get::<Option<String>, _>("ledger_kind"),
        "error": err_code.map(|c| json!({"code": c, "message": r.get::<Option<String>, _>("error_message")})),
        "created_at": r.get::<DateTime<Utc>, _>("created_at"),
        "updated_at": r.get::<DateTime<Utc>, _>("updated_at"),
        "applied_at": r.get::<Option<DateTime<Utc>>, _>("applied_at"),
    })
}

async fn load(st: &AppState, tenant_id: i64, id: i64) -> ApiResult<PgRow> {
    sqlx::query("SELECT * FROM adjustments WHERE tenant_id = $1 AND id = $2").bind(tenant_id).bind(id).fetch_optional(&st.pool).await?.ok_or_else(|| ApiError::not_found("Adjustment"))
}

fn fingerprint_matches(r: &PgRow, n: &NewAdjustment, amount: D) -> bool {
    r.get::<i64, _>("user_id") == n.user_id
        && r.get::<String, _>("target") == n.target
        && r.get::<Option<i64>, _>("login") == n.login
        && r.get::<String, _>("op") == n.op
        && r.get::<String, _>("category") == n.category
        && r.get::<D, _>("amount") == amount
        && r.get::<bool, _>("force") == n.force
}

/// Submits an adjustment: applies it at once, or records it as pending above the four-eyes threshold.
pub async fn create(st: &AppState, s_ctx: &StaffCtx, n: NewAdjustment) -> ApiResult<Value> {
    validate(&n)?;
    check_perms(s_ctx, &n.op, n.force)?;
    let key = n.idempotency_key.trim().to_string();
    if key.is_empty() || key.len() > 128 {
        return Err(ApiError::validation("idempotency_key", "idempotency_key must be 1–128 characters"));
    }
    let comment = clean_text(Some(&n.comment), 500).unwrap_or_default();
    if comment.chars().count() < 3 {
        return Err(ApiError::validation("comment", "Add a comment for the audit (at least 3 characters)"));
    }
    let client_note = clean_text(n.client_note.as_deref(), 200);
    let t = s_ctx.ctx.tenant.id;

    // same key again (double click, retry): the original request, whatever its state
    if let Some(r) = sqlx::query("SELECT * FROM adjustments WHERE tenant_id = $1 AND idempotency_key = $2").bind(t).bind(&key).fetch_optional(&st.pool).await? {
        if !fingerprint_matches(&r, &n, n.amount) {
            return Err(ApiError::conflict("idempotency_conflict", "This submission key was used for a different adjustment"));
        }
        let mut v = row_json(&r);
        v["replayed"] = json!(true);
        return Ok(v);
    }

    let res = resolve(st, s_ctx, &n).await?;
    let n = NewAdjustment { comment: comment.clone(), client_note: client_note.clone(), amount: res.amount, ..n };
    // refuse what can't be booked now (wallet shortfall, engine limits) before recording anything
    let pv = preview(st, s_ctx, &n).await?;
    if pv["ok"].as_bool() != Some(true) {
        let code = pv["error"]["code"].as_str().unwrap_or("refused").to_string();
        let msg = pv["error"]["message"].as_str().unwrap_or("This adjustment can't be booked").to_string();
        let mut tx = st.pool.begin().await?;
        audit::staff(
            &mut tx,
            s_ctx,
            Entry {
                action: "adjustment.refused",
                target_kind: if n.target == "wallet" { "wallet" } else { "trading_account" },
                target_id: n.login.map(|l| l.to_string()).unwrap_or_else(|| n.user_id.to_string()),
                reason: Some(&comment),
                before: Some(pv["before"].clone()),
                after: Some(json!({"user_id": n.user_id, "op": n.op, "category": n.category, "amount": s(res.amount), "currency": res.currency, "force": n.force, "error": {"code": code, "message": msg}})),
            },
        )
        .await?;
        tx.commit().await?;
        return Err(ApiError::Coded { status: axum::http::StatusCode::UNPROCESSABLE_ENTITY, code: leak(code), message: msg });
    }
    let needs = pv["needs_approval"].as_bool() == Some(true);
    let status = if needs { "pending" } else { "processing" };

    let mut tx = st.pool.begin().await?;
    let row = sqlx::query(
        "INSERT INTO adjustments (tenant_id, idempotency_key, user_id, target, login, account_type, currency, op, category, amount, amount_usd, comment, client_note,
                                  notify, force, status, requested_by_id, requested_by_name, requested_by_role, requested_perms, before)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
         ON CONFLICT (tenant_id, idempotency_key) DO NOTHING RETURNING *",
    )
    .bind(t)
    .bind(&key)
    .bind(n.user_id)
    .bind(&n.target)
    .bind(n.login)
    .bind(&res.account_type)
    .bind(&res.currency)
    .bind(&n.op)
    .bind(&n.category)
    .bind(res.amount)
    .bind(res.amount_usd)
    .bind(&comment)
    .bind(&client_note)
    .bind(n.notify)
    .bind(n.force)
    .bind(status)
    .bind(&s_ctx.staff.id)
    .bind(&s_ctx.staff.name)
    .bind(&s_ctx.staff.role)
    .bind(&s_ctx.staff.perms)
    .bind(sqlx::types::Json(pv["before"].clone()))
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        // a concurrent submit with the same key won
        drop(tx);
        let r = sqlx::query("SELECT * FROM adjustments WHERE tenant_id = $1 AND idempotency_key = $2").bind(t).bind(&key).fetch_one(&st.pool).await?;
        let mut v = row_json(&r);
        v["replayed"] = json!(true);
        return Ok(v);
    };
    let id: i64 = row.get("id");
    let target_id = n.login.map(|l| l.to_string()).unwrap_or_else(|| n.user_id.to_string());
    let target_kind = if n.target == "wallet" { "wallet" } else { "trading_account" };
    if needs {
        audit::staff(
            &mut tx,
            s_ctx,
            Entry {
                action: "adjustment.requested",
                target_kind,
                target_id,
                reason: Some(&comment),
                before: Some(pv["before"].clone()),
                after: Some(json!({"adjustment_id": id, "user_id": n.user_id, "op": n.op, "category": n.category, "amount": s(res.amount), "currency": res.currency, "amount_usd": s(res.amount_usd), "threshold_usd": pv["threshold_usd"], "expected_after": pv["after"], "status": "pending"})),
            },
        )
        .await?;
        tx.commit().await?;
        tracing::info!(tenant = t, adjustment = id, user = n.user_id, op = %n.op, amount = %res.amount, "adjustment waiting for approval");
        let mut v = row_json(&load(st, t, id).await?);
        v["replayed"] = json!(false);
        return Ok(v);
    }
    tx.commit().await?;
    apply(st, t, id, Some(s_ctx), None).await?;
    let mut v = row_json(&load(st, t, id).await?);
    v["replayed"] = json!(false);
    Ok(v)
}

/// Books a `processing` adjustment (wallet: this ledger; trading: the engine). `approver` is set for four-eyes.
/// Returns an error for a refusal (the row is then `failed`) or an unknown engine outcome (stays `processing`).
pub async fn apply(st: &AppState, tenant_id: i64, id: i64, actor: Option<&StaffCtx>, approver: Option<&StaffCtx>) -> ApiResult<()> {
    let r = load(st, tenant_id, id).await?;
    if r.get::<String, _>("status") != "processing" {
        return Ok(());
    }
    let (user_id, target, op, category) = (r.get::<i64, _>("user_id"), r.get::<String, _>("target"), r.get::<String, _>("op"), r.get::<String, _>("category"));
    let (amount, ccy, comment) = (r.get::<D, _>("amount"), r.get::<String, _>("currency"), r.get::<String, _>("comment"));
    let client_note: Option<String> = r.get("client_note");
    let notify_client: bool = r.get("notify");
    let login: Option<i64> = r.get("login");
    let approved_by = approver.map(|a| json!({"id": a.staff.id, "name": a.staff.name, "role": a.staff.role}));
    let target_kind = if target == "wallet" { "wallet" } else { "trading_account" };
    let target_id = login.map(|l| l.to_string()).unwrap_or_else(|| user_id.to_string());
    let who = approver.or(actor);

    if target == "wallet" {
        let (kind, counter) = wallet_kind(&op, &category);
        let signed = if op == "add" { amount } else { -amount };
        let note = client_note.clone().unwrap_or_else(|| super::transfers::kind_label(kind).to_string());
        let mut tx = st.pool.begin().await?;
        let before = {
            let b: Option<(D, D)> = sqlx::query_as("SELECT available, locked FROM wallet_balances WHERE tenant_id = $1 AND user_id = $2 AND currency = $3 FOR UPDATE")
                .bind(tenant_id)
                .bind(user_id)
                .bind(&ccy)
                .fetch_optional(&mut *tx)
                .await?;
            b.unwrap_or_default()
        };
        let posted = ledger::post_with(
            &mut tx,
            NewTxn {
                tenant_id,
                key: format!("adj:{id}"),
                kind: kind.into(),
                user_id: Some(user_id),
                currency: ccy.clone(),
                reference: Some(format!("ADJ-{id}")),
                note: Some(note),
                actor: who.map(|w| w.staff.tag()).unwrap_or_else(|| "system".into()),
                request: Some(json!({"adjustment_id": id})),
                legs: vec![Leg::available(user_id, &ccy, signed), Leg::sys(&ccy, counter, -signed)],
            },
            |_, _| None,
        )
        .await;
        match posted {
            Ok((txn, after)) => {
                let (a, l) = after.get(&user_id).copied().unwrap_or_default();
                let before_v = wallet_snap(&ccy, before.0, before.1);
                let after_v = wallet_snap(&ccy, a, l);
                if !finish(&mut tx, tenant_id, id, txn, kind, &before_v, &after_v).await? {
                    return Ok(());
                }
                let entry = Entry {
                    action: "adjustment.applied",
                    target_kind,
                    target_id: target_id.clone(),
                    reason: Some(&comment),
                    before: Some(before_v.clone()),
                    after: Some(json!({"adjustment_id": id, "op": op, "category": category, "amount": s(amount), "currency": ccy, "txn_id": txn, "ledger_kind": kind, "balance": after_v, "approved_by": approved_by, "requested_by": r.get::<String, _>("requested_by_name")})),
                };
                match who {
                    Some(w) => audit::staff(&mut tx, w, entry).await?,
                    None => audit::system(&mut tx, tenant_id, entry.action, entry.target_kind, entry.target_id, entry.after).await?,
                }
                if kind == "manual_deposit" {
                    // an external payment received is real money in: reported to the IB service (first deposit / CPA)
                    crate::ib::record(&mut *tx, txn).await?;
                }
                if notify_client {
                    client_notification(&mut tx, tenant_id, user_id, id, &op, amount, &ccy, None, client_note.as_deref()).await?;
                }
                tx.commit().await?;
                tracing::info!(tenant = tenant_id, adjustment = id, user = user_id, %op, %amount, "wallet adjustment applied");
                Ok(())
            }
            Err(PostError::Insufficient) => {
                drop(tx);
                let (a, _) = wallet_balance(st, tenant_id, user_id, &ccy).await?;
                let msg = format!("Deducting {} {ccy} exceeds the wallet's available balance of {} {ccy}", s(amount), s(a));
                fail(st, tenant_id, id, who, target_kind, &target_id, &comment, "insufficient_funds", &msg).await?;
                Err(ApiError::Coded { status: axum::http::StatusCode::UNPROCESSABLE_ENTITY, code: "insufficient_funds", message: msg })
            }
            Err(PostError::Duplicate) => {
                // booked by a concurrent attempt (recovery vs. request): the row is finished by that attempt
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    } else {
        let login = login.unwrap_or_default();
        let key = format!("wallet-adj-{tenant_id}-{id}");
        let body = engine_body(&op, &category, amount, r.get("force"), client_note.as_deref().unwrap_or(""), &comment, Some(key), false, approved_by.clone(), Some(id));
        // the engine records the requester (maker) as the staff member, with the approver in the audit detail
        let perms: Option<Vec<String>> = r.get("requested_perms");
        let staff = staff_headers(&r.get::<String, _>("requested_by_id"), &r.get::<String, _>("requested_by_name"), &r.get::<String, _>("requested_by_role"), perms);
        let tenant_slug = st.tenants.slug_of(tenant_id).unwrap_or_else(|| "kalks".into());
        match st.engine.adjust(&tenant_slug, login, body, &staff).await {
            Ok(d) => {
                let txn = d["txn"].as_i64().unwrap_or_default();
                let kind = d["kind"].as_str().unwrap_or(if op.starts_with("credit") { "credit" } else { "adjustment" }).to_string();
                let before_v = d.get("before").cloned().filter(|v| !v.is_null()).unwrap_or_else(|| r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0).unwrap_or(Value::Null));
                let after_v = d.get("after").cloned().unwrap_or(Value::Null);
                let mut tx = st.pool.begin().await?;
                if !finish(&mut tx, tenant_id, id, txn, &kind, &before_v, &after_v).await? {
                    // a concurrent attempt (request vs. recovery) finished it: audit and notify once
                    return Ok(());
                }
                let entry = Entry {
                    action: "adjustment.applied",
                    target_kind,
                    target_id: target_id.clone(),
                    reason: Some(&comment),
                    before: Some(before_v.clone()),
                    after: Some(json!({"adjustment_id": id, "login": login, "op": op, "category": category, "amount": s(amount), "currency": ccy, "txn_id": txn, "ledger_kind": kind, "account": after_v, "approved_by": approved_by, "requested_by": r.get::<String, _>("requested_by_name"), "replayed": d["replayed"]})),
                };
                match who {
                    Some(w) => audit::staff(&mut tx, w, entry).await?,
                    None => audit::system(&mut tx, tenant_id, entry.action, entry.target_kind, entry.target_id, entry.after).await?,
                }
                if notify_client {
                    client_notification(&mut tx, tenant_id, user_id, id, &op, amount, &ccy, Some(login), client_note.as_deref()).await?;
                }
                tx.commit().await?;
                tracing::info!(tenant = tenant_id, adjustment = id, login, %op, %amount, "trading adjustment applied");
                Ok(())
            }
            Err(EngineError::Rejected { code, message, .. }) => {
                fail(st, tenant_id, id, who, target_kind, &target_id, &comment, &code, &message).await?;
                Err(engine_err(EngineError::Rejected { status: 422, code, message }))
            }
            Err(EngineError::Unavailable(why)) => {
                tracing::warn!(adjustment = id, %why, "engine adjustment outcome unknown; recovery will settle it");
                Err(ApiError::Coded {
                    status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
                    code: "engine_unavailable",
                    message: "The trading engine did not answer. The adjustment stays in processing and is completed automatically; nothing is booked twice.".into(),
                })
            }
        }
    }
}

/// Marks the row applied. False when another attempt (request vs. recovery) already finished it.
async fn finish(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: i64, id: i64, txn: i64, kind: &str, before: &Value, after: &Value) -> sqlx::Result<bool> {
    let done = sqlx::query("UPDATE adjustments SET status = 'applied', txn_id = $3, ledger_kind = $4, before = $5, after = $6, applied_at = now(), updated_at = now() WHERE tenant_id = $1 AND id = $2 AND status = 'processing'")
        .bind(tenant_id)
        .bind(id)
        .bind(txn)
        .bind(kind)
        .bind(sqlx::types::Json(before))
        .bind(sqlx::types::Json(after))
        .execute(&mut **tx)
        .await?;
    Ok(done.rows_affected() == 1)
}

#[allow(clippy::too_many_arguments)]
async fn fail(st: &AppState, tenant_id: i64, id: i64, who: Option<&StaffCtx>, target_kind: &str, target_id: &str, comment: &str, code: &str, message: &str) -> ApiResult<()> {
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE adjustments SET status = 'failed', error_code = $3, error_message = $4, updated_at = now() WHERE tenant_id = $1 AND id = $2 AND status = 'processing'")
        .bind(tenant_id)
        .bind(id)
        .bind(code)
        .bind(message)
        .execute(&mut *tx)
        .await?;
    let entry = Entry { action: "adjustment.failed", target_kind, target_id: target_id.to_string(), reason: Some(comment), before: None, after: Some(json!({"adjustment_id": id, "error": {"code": code, "message": message}})) };
    match who {
        Some(w) => audit::staff(&mut tx, w, entry).await?,
        None => audit::system(&mut tx, tenant_id, entry.action, entry.target_kind, entry.target_id, entry.after).await?,
    }
    tx.commit().await?;
    Ok(())
}

/// Client bell + email (support, `wallet` preference): kind `adjustment.<wallet|account|credit>_<in|out>`.
#[allow(clippy::too_many_arguments)]
async fn client_notification(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: i64, user_id: i64, id: i64, op: &str, amount: D, ccy: &str, login: Option<i64>, note: Option<&str>) -> sqlx::Result<()> {
    let amt = format!("{} {ccy}", fmt2(amount));
    let (kind, title, body) = match (op, login) {
        ("add", None) => ("adjustment.wallet_in", "Funds added to your wallet".to_string(), format!("{amt} was added to your wallet.")),
        ("deduct", None) => ("adjustment.wallet_out", "Funds deducted from your wallet".to_string(), format!("{amt} was deducted from your wallet.")),
        ("add", Some(l)) => ("adjustment.account_in", format!("Funds added to account {l}"), format!("{amt} was added to trading account {l}.")),
        ("deduct", Some(l)) => ("adjustment.account_out", format!("Funds deducted from account {l}"), format!("{amt} was deducted from trading account {l}.")),
        ("credit_in", Some(l)) => ("adjustment.credit_in", format!("Credit added to account {l}"), format!("{amt} credit was added to trading account {l}. Credit counts toward equity and margin; it can't be withdrawn.")),
        (_, l) => ("adjustment.credit_out", format!("Credit removed from account {}", l.unwrap_or_default()), format!("{amt} credit was removed from trading account {}.", l.unwrap_or_default())),
    };
    let body = match note {
        Some(n) if !n.is_empty() => format!("{body} {n}"),
        _ => body,
    };
    notify(tx, tenant_id, user_id, kind, &title, &body, json!({"adjustment_id": id, "login": login, "amount": s(amount), "currency": ccy, "op": op})).await
}

/// 2-decimal display of an amount ("100.00").
fn fmt2(d: D) -> String {
    let r = d.round_dp(6).normalize();
    if r.scale() <= 2 { format!("{:.2}", r) } else { r.to_string() }
}

/// Four-eyes approval: another staff member with finance.adjust_approve books a pending adjustment.
pub async fn approve(st: &AppState, s_ctx: &StaffCtx, id: i64, note: Option<String>) -> ApiResult<Value> {
    s_ctx.require_perm("finance.adjust_approve", ROLES_APPROVE)?;
    let t = s_ctx.ctx.tenant.id;
    let r = load(st, t, id).await?;
    if r.get::<String, _>("status") != "pending" {
        return Err(ApiError::conflict("invalid_state", format!("This adjustment is {} and can't be approved", r.get::<String, _>("status"))));
    }
    if r.get::<String, _>("requested_by_id") == s_ctx.staff.id {
        return Err(ApiError::Forbidden("Four-eyes rule: another staff member must approve your own request".into()));
    }
    let note = clean_text(note.as_deref(), 500);
    let claimed = sqlx::query(
        "UPDATE adjustments SET status = 'processing', decided_by_id = $3, decided_by_name = $4, decided_at = now(), decision_note = $5, updated_at = now()
         WHERE tenant_id = $1 AND id = $2 AND status = 'pending'",
    )
    .bind(t)
    .bind(id)
    .bind(&s_ctx.staff.id)
    .bind(&s_ctx.staff.name)
    .bind(&note)
    .execute(&st.pool)
    .await?;
    if claimed.rows_affected() == 0 {
        return Err(ApiError::conflict("invalid_state", "This adjustment was decided by someone else just now"));
    }
    let mut tx = st.pool.begin().await?;
    audit::staff(
        &mut tx,
        s_ctx,
        Entry {
            action: "adjustment.approved",
            target_kind: if r.get::<String, _>("target") == "wallet" { "wallet" } else { "trading_account" },
            target_id: r.get::<Option<i64>, _>("login").map(|l| l.to_string()).unwrap_or_else(|| r.get::<i64, _>("user_id").to_string()),
            reason: note.as_deref(),
            before: None,
            after: Some(json!({"adjustment_id": id, "requested_by": r.get::<String, _>("requested_by_name"), "amount": s(r.get::<D, _>("amount")), "currency": r.get::<String, _>("currency"), "op": r.get::<String, _>("op")})),
        },
    )
    .await?;
    tx.commit().await?;
    apply(st, t, id, None, Some(s_ctx)).await?;
    Ok(row_json(&load(st, t, id).await?))
}

/// Rejects (approver) or cancels (the requester) a pending adjustment. Nothing is booked.
pub async fn decline(st: &AppState, s_ctx: &StaffCtx, id: i64, reason: Option<String>, cancel: bool) -> ApiResult<Value> {
    let t = s_ctx.ctx.tenant.id;
    let r = load(st, t, id).await?;
    let own = r.get::<String, _>("requested_by_id") == s_ctx.staff.id;
    if cancel {
        if !own {
            return Err(ApiError::Forbidden("Only the staff member who requested it can cancel it; approvers reject it".into()));
        }
    } else {
        s_ctx.require_perm("finance.adjust_approve", ROLES_APPROVE)?;
    }
    let reason = clean_text(reason.as_deref(), 500);
    if !cancel && reason.as_deref().is_none_or(|x| x.chars().count() < 3) {
        return Err(ApiError::validation("reason", "Give a reason for the rejection"));
    }
    let status = if cancel { "cancelled" } else { "rejected" };
    let done = sqlx::query(
        "UPDATE adjustments SET status = $3, decided_by_id = $4, decided_by_name = $5, decided_at = now(), decision_note = $6, updated_at = now()
         WHERE tenant_id = $1 AND id = $2 AND status = 'pending'",
    )
    .bind(t)
    .bind(id)
    .bind(status)
    .bind(&s_ctx.staff.id)
    .bind(&s_ctx.staff.name)
    .bind(&reason)
    .execute(&st.pool)
    .await?;
    if done.rows_affected() == 0 {
        return Err(ApiError::conflict("invalid_state", format!("This adjustment is {} and can't be {status}", r.get::<String, _>("status"))));
    }
    let mut tx = st.pool.begin().await?;
    audit::staff(
        &mut tx,
        s_ctx,
        Entry {
            action: if cancel { "adjustment.cancelled" } else { "adjustment.rejected" },
            target_kind: if r.get::<String, _>("target") == "wallet" { "wallet" } else { "trading_account" },
            target_id: r.get::<Option<i64>, _>("login").map(|l| l.to_string()).unwrap_or_else(|| r.get::<i64, _>("user_id").to_string()),
            reason: reason.as_deref(),
            before: None,
            after: Some(json!({"adjustment_id": id, "requested_by": r.get::<String, _>("requested_by_name"), "amount": s(r.get::<D, _>("amount")), "currency": r.get::<String, _>("currency"), "op": r.get::<String, _>("op"), "status": status})),
        },
    )
    .await?;
    tx.commit().await?;
    Ok(row_json(&load(st, t, id).await?))
}

/// Recovery loop step: re-sends trading adjustments whose engine outcome is unknown (same idempotency key, so
/// the engine books at most once and replays the original booking).
pub async fn recover(st: &AppState, min_age_secs: i64) -> anyhow::Result<usize> {
    let rows: Vec<(i64, i64)> = sqlx::query_as("SELECT tenant_id, id FROM adjustments WHERE status = 'processing' AND updated_at < now() - make_interval(secs => $1) ORDER BY id LIMIT 50")
        .bind(min_age_secs as f64)
        .fetch_all(&st.pool)
        .await?;
    let mut n = 0;
    for (t, id) in rows {
        match apply(st, t, id, None, None).await {
            Ok(()) => n += 1,
            Err(e) => tracing::warn!(adjustment = id, error = e.code(), "adjustment recovery attempt did not settle"),
        }
    }
    Ok(n)
}

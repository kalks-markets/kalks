//! Back Office account operations: account list/search/detail, balance / credit / bonus adjustments with a
//! reason (ledger postings, D118), status, group, leverage, and group configuration CRUD (D123).

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::Arc;

use super::dealing::{Reason, audit_now, check_reason};
use super::{ApiError, ApiResult, AppState, Body, ROLES_CONFIG, ROLES_DEALING, ROLES_FINANCE, ROLES_FORCE, StaffCtx};
use crate::engine::AuditDraft;
use crate::engine::funds::{self, AdjustKind};
use crate::model::{Book, Mode, Status};
use crate::money::{D, de_dec, num, r2};
use crate::persist::{self, AuditRow};
use crate::rules::Group;
use crate::shard::{ExecError, Op};
use crate::views;

#[derive(Deserialize)]
pub struct ListQ {
    q: Option<String>,
    group: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    status: Option<String>,
    user_id: Option<i64>,
    /// true: only accounts flagged dormant (B11)
    dormant: Option<bool>,
    /// `cfd` | `options`: accounts of that product's groups (CFD / Options account split)
    product: Option<String>,
    page: Option<i64>,
    limit: Option<i64>,
}

pub async fn accounts(State(st): State<AppState>, s: StaffCtx, Query(q): Query<ListQ>) -> ApiResult<Json<Value>> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let page = q.page.unwrap_or(1).max(1);
    let search = q.q.as_deref().map(str::trim).filter(|x| !x.is_empty()).map(|x| format!("%{}%", x.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")));
    let rows = sqlx::query(
        "SELECT login, count(*) OVER () AS total FROM accounts WHERE tenant_id = $1
           AND ($2::text IS NULL OR login::text LIKE $2 OR name ILIKE $2 OR user_id::text LIKE $2)
           AND ($3::text IS NULL OR group_code = $3) AND ($4::text IS NULL OR kind = $4) AND ($5::text IS NULL OR status = $5)
           AND ($6::bigint IS NULL OR user_id = $6) AND ($9::bool IS NOT TRUE OR dormant_since IS NOT NULL)
           AND ($10::text IS NULL OR group_code IN (SELECT code FROM groups WHERE tenant_id = $1 AND product = $10))
         ORDER BY login DESC OFFSET $7 LIMIT $8",
    )
    .bind(s.ctx.tenant.tenant_id)
    .bind(&search)
    .bind(&q.group)
    .bind(&q.kind)
    .bind(&q.status)
    .bind(q.user_id)
    .bind((page - 1) * limit)
    .bind(limit)
    .bind(q.dormant)
    .bind(q.product.as_deref().map(|p| crate::rules::Product::parse(p).as_str()))
    .fetch_all(&st.pool)
    .await?;
    let total = rows.first().map(|r| r.get::<i64, _>("total")).unwrap_or(0);
    let mut items = Vec::new();
    for r in rows {
        let v = super::terminal::account_view(&st, r.get("login")).await;
        if !v.is_null() {
            items.push(v);
        }
    }
    super::lifecycle::decorate(&st, s.ctx.tenant.tenant_id, &mut items).await?;
    Ok(Json(json!({"items": items, "page": page, "limit": limit, "total": total})))
}

pub async fn account(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>) -> ApiResult<Json<Value>> {
    let m = st.hub.meta(login).filter(|m| m.tenant_id == s.ctx.tenant.tenant_id).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    let _ = m;
    let v = st
        .hub
        .read(
            login,
            Box::new(|x| match x {
                Some((a, env)) => json!({
                    "account": views::account_json(env, a),
                    "positions": a.positions.values().map(|p| views::desk_position_json(env, a, p)).collect::<Vec<_>>(),
                    "orders": a.orders.values().map(|o| views::desk_order_json(a, o)).collect::<Vec<_>>(),
                }),
                None => Value::Null,
            }),
        )
        .await;
    let last: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT last_activity_at FROM accounts WHERE login = $1").bind(login).fetch_optional(&st.pool).await?;
    let mut v = v;
    v["lastActivityAt"] = json!(last);
    if v["account"].is_object() {
        let mut one = [v["account"].clone()];
        super::lifecycle::decorate(&st, s.ctx.tenant.tenant_id, &mut one).await?;
        v["account"] = one[0].clone();
    }
    Ok(Json(v))
}

async fn staff_exec(st: &AppState, s: &StaffCtx, login: i64, r: &Reason, attempted: &str, op: Op) -> ApiResult<Json<Value>> {
    st.hub.meta(login).filter(|m| m.tenant_id == s.ctx.tenant.tenant_id).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &r.reason_code, r.note.trim(), None, op).await {
        Ok(d) => Ok(Json(json!({"data": d.value, "audit": d.audit}))),
        Err(ExecError::Reject(rej)) => {
            let a = AuditRow {
                tenant_id: s.ctx.tenant.tenant_id,
                at: chrono::Utc::now(),
                staff_id: s.staff.id.clone(),
                staff_name: s.staff.name.clone(),
                staff_role: s.staff.role.clone(),
                action: "account.rejected".into(),
                tickets: vec![],
                login: Some(login),
                symbol: None,
                before: None,
                after: Some(json!({"attempted": attempted, "error": rej.message})),
                reason_code: r.reason_code.clone(),
                note: r.note.clone(),
                flags: vec!["rejected".into()],
            };
            let entry = audit_now(st, a).await?;
            Err(ApiError::Reject { reject: rej, audit: vec![entry] })
        }
        Err(e) => Err(e.into()),
    }
}

fn draft(action: &'static str, before: Value, after: Value, flags: Vec<&str>) -> AuditDraft {
    AuditDraft { action, tickets: vec![], login: None, symbol: None, before: Some(before), after: Some(after), flags: flags.into_iter().map(String::from).collect() }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceBody {
    /// deposit | withdrawal | adjustment | credit | bonus
    #[serde(rename = "type")]
    kind: String,
    /// Signed, in the account currency (USC for cent accounts). deposit/withdrawal use the absolute value.
    #[serde(deserialize_with = "de_dec")]
    amount: D,
    #[serde(default)]
    idempotency_key: Option<String>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn balance(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(b): Body<BalanceBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_FINANCE)?;
    check_reason(&b.reason)?;
    if b.reason.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "Balance and credit adjustments need a note".into() });
    }
    let kind = AdjustKind::parse(&b.kind).ok_or(ApiError::Validation { field: "type", message: "type must be deposit, withdrawal, adjustment, credit or bonus".into() })?;
    let key = format!("staff:{}", b.idempotency_key.clone().unwrap_or_else(|| crate::auth::random_token(12)));
    let (code, note, kind_s) = (b.reason.reason_code.clone(), b.reason.note.trim().to_string(), b.kind.clone());
    let op: Op = Box::new(move |tx, env| {
        let before = json!({"balance": num(tx.st.balance), "credit": num(tx.st.credit), "bonus": num(tx.st.bonus)});
        let txn = funds::adjust(tx, env, kind, b.amount, &key, &code, &note)?;
        let after = json!({"balance": num(tx.st.balance), "credit": num(tx.st.credit), "bonus": num(tx.st.bonus), "txn": txn, "type": kind_s, "amount": num(b.amount)});
        tx.audit.push(draft(if matches!(kind, AdjustKind::Credit | AdjustKind::Bonus) { "account.credit" } else { "account.balance" }, before, after.clone(), vec!["ledger"]));
        Ok(after)
    });
    match staff_exec(&st, &s, login, &b.reason, "balance adjustment", op).await {
        Err(ApiError::Conflict { code: "duplicate_idempotency_key", .. }) => Err(ApiError::Conflict { code: "duplicate_idempotency_key", message: "This adjustment was already booked".into() }),
        r => r,
    }
}

/* ------------------------------------------------------------------ */
/* Balance & credit (manual adjustments via the wallet service)       */
/* ------------------------------------------------------------------ */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdjustBody {
    /// add | deduct | credit_in | credit_out
    op: String,
    /// deposit | withdrawal | correction | compensation | bonus | fee | chargeback | other
    category: String,
    /// Positive, account currency (USC for cent accounts), at most 2 decimals.
    #[serde(deserialize_with = "de_dec")]
    amount: D,
    #[serde(default)]
    idempotency_key: String,
    #[serde(default)]
    force: bool,
    /// Client-visible statement text.
    #[serde(default)]
    statement_note: String,
    /// Validate and show before → after without booking anything.
    #[serde(default)]
    dry_run: bool,
    /// Four-eyes: the staff member who approved the request (recorded in the audit).
    #[serde(default)]
    approved_by: Option<Value>,
    /// The wallet service's adjustment id.
    #[serde(default)]
    request_id: Option<i64>,
    /// reasonCode + internal comment (the audit note, never shown to the client).
    #[serde(flatten)]
    reason: Reason,
}

async fn adjust_lookup(st: &AppState, tenant: i64, key: &str) -> ApiResult<Option<(i64, String, Option<Value>, chrono::DateTime<chrono::Utc>)>> {
    let r = sqlx::query("SELECT id, kind, request, created_at FROM ledger_txns WHERE tenant_id = $1 AND idempotency_key = $2").bind(tenant).bind(key).fetch_optional(&st.pool).await?;
    Ok(r.map(|r| (r.get("id"), r.get("kind"), r.get::<Option<sqlx::types::Json<Value>>, _>("request").map(|j| j.0), r.get("created_at"))))
}

/// `POST /v1/admin/accounts/{login}/adjust`: Back Office "Balance & credit" (add / deduct funds, give / take
/// credit). Called by the wallet service, which owns the request, the four-eyes approval and the client
/// notification. Idempotent on `idempotencyKey`: the same key and body returns the original booking with
/// `replayed: true`; the same key with another body is a 409. `dryRun` returns before → after only.
pub async fn adjust(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(b): Body<AdjustBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_FINANCE)?;
    let op = funds::AdjustOp::parse(&b.op).ok_or(ApiError::Validation { field: "op", message: "op must be add, deduct, credit_in or credit_out".into() })?;
    s.require_perm(if op.is_credit() { "finance.credit" } else { "finance.adjust" }, ROLES_FINANCE)?;
    if b.force {
        if op == funds::AdjustOp::Add || op == funds::AdjustOp::CreditIn {
            return Err(ApiError::Validation { field: "force", message: "Force applies to deductions and taking credit back only".into() });
        }
        s.require_perm("finance.adjust_force", ROLES_FORCE)?;
    }
    if !funds::ADJUST_CATEGORIES.contains(&b.category.as_str()) {
        return Err(ApiError::Validation { field: "category", message: format!("category must be one of {}", funds::ADJUST_CATEGORIES.join(", ")) });
    }
    check_reason(&b.reason)?;
    let comment = b.reason.note.trim().to_string();
    if !b.dry_run && comment.chars().count() < 3 {
        return Err(ApiError::Validation { field: "note", message: "Add a comment (kept in the audit, not shown to the client)".into() });
    }
    let statement: String = b.statement_note.trim().chars().filter(|c| !c.is_control()).take(200).collect();
    let amount = b.amount;
    let m = st.hub.meta(login).filter(|m| m.tenant_id == s.ctx.tenant.tenant_id).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    if st.social.is_fund(login) {
        return Err(ApiError::Status { status: 422, code: "pamm_account", message: "This is a PAMM fund account: its money moves only through invest and redeem at the rollover".into() });
    }
    let (category, code, force) = (b.category.clone(), b.reason.reason_code.clone(), b.force);

    if b.dry_run {
        let v = st
            .hub
            .read(
                login,
                Box::new(move |x| {
                    let Some((a, env)) = x else { return Value::Null };
                    let before = funds::funds_snapshot(env, a);
                    let (strict, forced) = funds::adjust_limits(env, a, op);
                    let limits = json!({"max": if strict == D::MAX { Value::Null } else { num(strict) }, "maxForced": if forced == D::MAX { Value::Null } else { num(forced) }});
                    let mut tx = crate::engine::Tx::new(a);
                    match funds::staff_adjust(&mut tx, env, funds::StaffAdjust { op, category: &category, amount, force, key: "dry-run", reason_code: &code, statement: &statement }) {
                        Ok(_) => json!({"ok": true, "before": before, "after": funds::funds_snapshot(env, &tx.st), "limits": limits,
                                        "stopOut": tx.events.iter().any(|e| matches!(e, crate::state::Event::StopOut { .. })),
                                        "marginCall": tx.st.margin_call}),
                        Err(r) => json!({"ok": false, "before": before, "limits": limits, "error": {"code": r.code, "message": r.message}}),
                    }
                }),
            )
            .await;
        if v.is_null() {
            return Err(ApiError::NotFound("Account not found".into()));
        }
        let mut v = v;
        v["login"] = json!(login);
        v["userId"] = json!(m.user_id);
        v["type"] = json!(m.kind.as_str());
        return Ok(Json(json!({"data": v})));
    }

    let key = b.idempotency_key.trim();
    if key.is_empty() || key.len() > 128 {
        return Err(ApiError::Validation { field: "idempotencyKey", message: "idempotencyKey must be 1–128 characters".into() });
    }
    let key = format!("adj:{key}");
    let fingerprint = json!({"login": login, "op": op.as_str(), "category": b.category, "amount": r2(amount).to_string(), "force": force});
    let replay = |found: (i64, String, Option<Value>, chrono::DateTime<chrono::Utc>)| -> ApiResult<Json<Value>> {
        if found.2.as_ref() != Some(&fingerprint) {
            return Err(ApiError::Conflict { code: "idempotency_conflict", message: "This idempotency key was used for a different adjustment".into() });
        }
        Ok(Json(json!({"data": {"txn": found.0, "kind": found.1, "at": found.3, "login": login, "replayed": true}, "audit": []})))
    };
    if let Some(found) = adjust_lookup(&st, s.ctx.tenant.tenant_id, &key).await? {
        return replay(found);
    }
    let (k2, cat2, approved_by, request_id) = (key.clone(), b.category.clone(), b.approved_by.clone(), b.request_id);
    let action = if op.is_credit() { "account.credit" } else { "account.balance" };
    let o: Op = Box::new(move |tx, env| {
        let before = funds::funds_snapshot(env, &tx.st);
        let txn = funds::staff_adjust(tx, env, funds::StaffAdjust { op, category: &cat2, amount, force, key: &k2, reason_code: &code, statement: &statement })?;
        let kind = tx.events.iter().find_map(|e| match e {
            crate::state::Event::Ledger { txn: t } if t.id == txn => Some(t.kind.as_str()),
            _ => None,
        });
        let after = funds::funds_snapshot(env, &tx.st);
        let mut flags = vec!["ledger", "manual_adjustment"];
        if force {
            flags.push("forced");
        }
        if approved_by.is_some() {
            flags.push("four_eyes");
        }
        let detail = json!({"op": op.as_str(), "category": cat2, "amount": num(amount), "txn": txn, "kind": kind, "statementNote": statement, "approvedBy": approved_by, "requestId": request_id});
        let mut after_audit = after.clone();
        if let (Value::Object(o), Value::Object(d)) = (&mut after_audit, detail.clone()) {
            o.extend(d);
        }
        tx.audit.push(draft(action, before.clone(), after_audit, flags));
        Ok(json!({"txn": txn, "kind": kind, "op": op.as_str(), "category": cat2, "amount": num(amount), "currency": tx.st.account.ccy(), "before": before, "after": after}))
    });
    let actor = format!("staff:{}", s.staff.id);
    match st.hub.exec(login, &actor, Some(s.staff.clone()), &b.reason.reason_code, &comment, Some(fingerprint.clone()), o).await {
        Ok(d) => {
            let mut v = d.value;
            v["login"] = json!(login);
            v["userId"] = json!(m.user_id);
            v["replayed"] = json!(false);
            tracing::info!(login, op = op.as_str(), category = %b.category, amount = %amount, force, staff = %s.staff.id, "manual adjustment");
            Ok(Json(json!({"data": v, "audit": d.audit})))
        }
        Err(ExecError::Duplicate(_)) => match adjust_lookup(&st, s.ctx.tenant.tenant_id, &key).await? {
            Some(found) => replay(found),
            None => Err(ApiError::Conflict { code: "idempotency_conflict", message: "Duplicate idempotency key".into() }),
        },
        Err(ExecError::Reject(rej)) => {
            let a = AuditRow {
                tenant_id: s.ctx.tenant.tenant_id,
                at: chrono::Utc::now(),
                staff_id: s.staff.id.clone(),
                staff_name: s.staff.name.clone(),
                staff_role: s.staff.role.clone(),
                action: "account.rejected".into(),
                tickets: vec![],
                login: Some(login),
                symbol: None,
                before: None,
                after: Some(json!({"attempted": format!("{} {}", op.as_str(), b.category), "amount": num(amount), "error": rej.message})),
                reason_code: b.reason.reason_code.clone(),
                note: comment.clone(),
                flags: vec!["rejected".into()],
            };
            let entry = audit_now(&st, a).await?;
            Err(ApiError::Reject { reject: rej, audit: vec![entry] })
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize)]
pub struct StatusBody {
    status: String,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn status(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(b): Body<StatusBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let status = Status::parse(&b.status).filter(|x| !x.is_retired()).ok_or(ApiError::Validation { field: "status", message: "status must be active, disabled, close_only, read_only or expired (use archive / restore for archived accounts)".into() })?;
    let op: Op = Box::new(move |tx, _| {
        let before = tx.st.account.status;
        if before.is_retired() {
            return Err(crate::engine::Reject::new("account_status", format!("The account is {}; use restore", before.as_str())));
        }
        funds::set_status(tx, status)?;
        tx.audit.push(draft("account.status", json!({"status": before.as_str()}), json!({"status": status.as_str()}), vec![]));
        Ok(json!({"status": status.as_str()}))
    });
    staff_exec(&st, &s, login, &b.reason, "status", op).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveBody {
    #[serde(default)]
    client_restorable: bool,
    /// Cancel orders and close positions first (as a dealer close). Without it the account must be flat.
    #[serde(default)]
    empty: bool,
    #[serde(flatten)]
    reason: Reason,
}

/// Staff archive (C1): reason code + note, audited. The balance stays on the account (no wallet transfer);
/// credit and bonus are forfeited.
pub async fn archive(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(b): Body<ArchiveBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let by = format!("staff:{}", s.staff.id);
    let dealer = crate::engine::trade::DealerCtx { staff: s.staff.name.clone(), reason_code: b.reason.reason_code.clone(), force: false };
    let (code, restorable, empty) = (b.reason.reason_code.clone(), b.client_restorable, b.empty);
    let op: Op = Box::new(move |tx, env| {
        let before = json!({"status": tx.st.account.status.as_str(), "positions": tx.st.positions.len(), "orders": tx.st.orders.len(), "credit": num(tx.st.credit), "bonus": num(tx.st.bonus)});
        let mut closed = 0;
        if empty {
            let orders: Vec<i64> = tx.st.orders.keys().copied().collect();
            for t in orders {
                crate::engine::trade::cancel_order(tx, env, t, "account archived")?;
            }
            let tickets: Vec<i64> = tx.st.positions.keys().copied().collect();
            for t in tickets {
                crate::engine::trade::close_position(tx, env, t, crate::engine::trade::CloseReq { dealer: Some(dealer.clone()), ..Default::default() })?;
                closed += 1;
            }
        }
        let changed = funds::archive(tx, env, &by, &code, restorable)?;
        if changed {
            tx.audit.push(draft("account.archive", before, json!({"status": "archived", "clientRestorable": restorable, "closed": closed}), vec![]));
        }
        Ok(json!({"status": "archived", "changed": changed, "closed": closed}))
    });
    let r = staff_exec(&st, &s, login, &b.reason, "archive", op).await?;
    super::lifecycle::revoke_sessions(&st, login).await;
    if r.0["data"]["changed"] == true
        && let Some(m) = st.hub.meta(login)
    {
        super::lifecycle::retired_hooks(&st, m.tenant_id, login, m.user_id, "archived").await;
        crate::notify::enqueue_or_log(
            &st.pool,
            m.tenant_id,
            m.user_id,
            "account.archived",
            json!({"title": format!("Account #{login} archived"), "body": format!("Your trading account #{login} was archived by our team. Its statements stay available under Accounts › Archived.{}", if b.client_restorable { " You can restore it there." } else { " Contact support to restore it." }), "link": "/accounts?tab=archived", "severity": "info", "data": {"login": login}}),
            &format!("archive:{login}:{}", chrono::Utc::now().timestamp()),
        )
        .await;
    }
    Ok(r)
}

/// Staff restore (C1) to the prior status, whoever archived it. The account limit is not enforced for staff.
pub async fn restore(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(r): Body<Reason>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&r)?;
    let op: Op = Box::new(move |tx, _| {
        let to = funds::restore(tx, false)?;
        tx.audit.push(draft("account.restore", json!({"status": "archived"}), json!({"status": to.as_str()}), vec![]));
        Ok(json!({"status": to.as_str()}))
    });
    let out = staff_exec(&st, &s, login, &r, "restore", op).await?;
    let _ = sqlx::query("UPDATE accounts SET last_activity_at = now() WHERE login = $1").bind(login).execute(&st.pool).await;
    Ok(out)
}

#[derive(Deserialize)]
pub struct GroupBody {
    group: String,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn group(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(b): Body<GroupBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let g = s.ctx.tenant.groups.get(&b.group).cloned().ok_or(ApiError::Validation { field: "group", message: "Unknown group".into() })?;
    let op: Op = Box::new(move |tx, env| {
        let (from, to) = funds::change_group(tx, env, &g)?;
        tx.audit.push(draft("account.group", json!({"group": from}), json!({"group": to, "leverage": tx.st.account.leverage}), vec![]));
        Ok(json!({"group": to}))
    });
    let r = staff_exec(&st, &s, login, &b.reason, "change group", op).await?;
    // the account may now listen to another spread group's feed
    Ok(r)
}

#[derive(Deserialize)]
pub struct LeverageBody {
    leverage: u32,
    #[serde(flatten)]
    reason: Reason,
}

/// Staff may change leverage with open positions (margin is recomputed at once).
pub async fn leverage(State(st): State<AppState>, s: StaffCtx, Path(login): Path<i64>, Body(b): Body<LeverageBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let op: Op = Box::new(move |tx, env| {
        let (from, to) = funds::change_leverage(tx, env, b.leverage, true)?;
        crate::engine::risk::check_margin(tx, env);
        tx.audit.push(draft("account.leverage", json!({"leverage": from}), json!({"leverage": to}), vec![]));
        Ok(json!({"leverage": to}))
    });
    staff_exec(&st, &s, login, &b.reason, "change leverage", op).await
}

/* ------------------------------------------------------------------ */
/* Groups                                                              */
/* ------------------------------------------------------------------ */

pub async fn groups(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let counts: Vec<(String, i64)> = sqlx::query_as("SELECT group_code, count(*) FROM accounts WHERE tenant_id = $1 GROUP BY group_code").bind(s.ctx.tenant.tenant_id).fetch_all(&st.pool).await?;
    let mut v: Vec<Value> = s
        .ctx
        .tenant
        .groups
        .values()
        .map(|g| {
            let mut j = json!(g);
            j["accounts"] = json!(counts.iter().find(|(c, _)| *c == g.code).map(|(_, n)| *n).unwrap_or(0));
            j
        })
        .collect();
    v.sort_by(|a, b| a["code"].as_str().cmp(&b["code"].as_str()));
    Ok(Json(json!({"groups": v})))
}

#[derive(Deserialize)]
pub struct GroupWrite {
    /// `cfd` | `options`. Absent: a new group is a CFD group, an existing group keeps its product (an older Back
    /// Office that does not send it never flips a group).
    #[serde(default)]
    product: Option<crate::rules::Product>,
    #[serde(flatten)]
    group: Group,
    #[serde(flatten)]
    reason: Reason,
}

/// Spread markups are keyed by spread group across all brokers (market-data): a broker other than the platform
/// broker prices only from its own spread groups (`<slug>-…`, tenants.rs), and the platform broker never from
/// another broker's, so no broker can read or change another broker's client prices.
fn check_spread_group(st: &AppState, t: &crate::rules::TenantConfig, g: &Group) -> ApiResult<()> {
    let bad = |m: String| Err(ApiError::Validation { field: "spreadGroup", message: m });
    if t.tenant_id != crate::tenants::TEMPLATE_TENANT {
        if !crate::tenants::owns_spread_group(&t.slug, &g.spread_group) {
            return bad(format!("spreadGroup must be one of this broker's spread groups ({}-…)", t.slug));
        }
    } else if st.hub.shared.registry.all().iter().any(|o| o.tenant_id != t.tenant_id && crate::tenants::owns_spread_group(&o.slug, &g.spread_group)) {
        return bad("spreadGroup belongs to another broker".into());
    }
    Ok(())
}

fn validate_group(g: &Group) -> ApiResult<()> {
    let bad = |field: &'static str, m: &str| Err(ApiError::Validation { field, message: m.into() });
    if !g.code.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') || g.code.is_empty() || g.code.len() > 40 {
        return bad("code", "code must be lowercase letters, digits or dashes");
    }
    if g.leverages.is_empty() || g.leverages.iter().any(|l| *l == 0 || *l > 3000) || !g.leverages.contains(&g.default_leverage) {
        return bad("leverages", "leverages must be 1–3000 and include the default leverage");
    }
    if g.stop_out_pct >= g.margin_call_pct || g.stop_out_pct < D::ZERO {
        return bad("stopOutPct", "stop-out must be below the margin call level");
    }
    if g.hedged_margin_pct < D::ZERO || g.hedged_margin_pct > D::ONE_HUNDRED {
        return bad("hedgedMarginPct", "hedged margin must be 0–100 %");
    }
    if !["live", "demo", "both"].contains(&g.account_types.as_str()) {
        return bad("accountTypes", "accountTypes must be live, demo or both");
    }
    if g.commission_per_lot < D::ZERO || g.min_deposit < D::ZERO || g.demo_initial_balance <= D::ZERO {
        return bad("commissionPerLot", "amounts must not be negative");
    }
    if g.spread_group.trim().is_empty() {
        return bad("spreadGroup", "spreadGroup is required (market-data spread group)");
    }
    Ok(())
}

async fn save_group(st: &AppState, s: &StaffCtx, g: &Group, r: &Reason, insert: bool) -> ApiResult<Json<Value>> {
    let t = s.ctx.tenant.tenant_id;
    let before = s.ctx.tenant.groups.get(&g.code).map(|x| json!(x));
    let mut tx = st.pool.begin().await?;
    let q = if insert {
        "INSERT INTO groups (tenant_id, code, name, mode, cent, account_types, leverages, default_leverage, margin_call_pct, stop_out_pct, hedged_margin_pct, min_deposit,
                             swap_free, commission_per_lot, route, spread_group, max_accounts_per_user, demo_initial_balance, demo_refills_per_day, demo_expiry_days, enabled, product)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22) ON CONFLICT DO NOTHING"
    } else {
        "UPDATE groups SET name=$3, mode=$4, cent=$5, account_types=$6, leverages=$7, default_leverage=$8, margin_call_pct=$9, stop_out_pct=$10, hedged_margin_pct=$11,
            min_deposit=$12, swap_free=$13, commission_per_lot=$14, route=$15, spread_group=$16, max_accounts_per_user=$17, demo_initial_balance=$18,
            demo_refills_per_day=$19, demo_expiry_days=$20, enabled=$21, product=$22, updated_at=now() WHERE tenant_id=$1 AND code=$2"
    };
    let n = sqlx::query(q)
        .bind(t)
        .bind(&g.code)
        .bind(&g.name)
        .bind(if g.mode == Mode::Netting { "netting" } else { "hedging" })
        .bind(g.cent)
        .bind(&g.account_types)
        .bind(g.leverages.iter().map(|x| *x as i32).collect::<Vec<i32>>())
        .bind(g.default_leverage as i32)
        .bind(g.margin_call_pct)
        .bind(g.stop_out_pct)
        .bind(g.hedged_margin_pct)
        .bind(g.min_deposit)
        .bind(g.swap_free)
        .bind(g.commission_per_lot)
        .bind(if g.route == Book::A { "A" } else { "B" })
        .bind(&g.spread_group)
        .bind(g.max_accounts_per_user as i32)
        .bind(g.demo_initial_balance)
        .bind(g.demo_refills_per_day as i32)
        .bind(g.demo_expiry_days as i32)
        .bind(g.enabled)
        .bind(g.product.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(if insert { ApiError::Conflict { code: "exists", message: format!("Group {} already exists", g.code) } } else { ApiError::NotFound(format!("Group {} not found", g.code)) });
    }
    let a = AuditRow {
        tenant_id: t,
        at: chrono::Utc::now(),
        staff_id: s.staff.id.clone(),
        staff_name: s.staff.name.clone(),
        staff_role: s.staff.role.clone(),
        action: if insert { "group.create".into() } else { "group.update".into() },
        tickets: vec![],
        login: None,
        symbol: None,
        before,
        after: Some(json!(g)),
        reason_code: r.reason_code.clone(),
        note: r.note.clone(),
        flags: vec![],
    };
    let id = persist::insert_audit(&mut *tx, &a).await?;
    tx.commit().await?;
    let mut g = g.clone();
    g.tenant_id = t;
    st.hub.shared.registry.update(t, |cfg| {
        cfg.groups.insert(g.code.clone(), g.clone());
    });
    Ok(Json(json!({"data": g, "audit": [persist::audit_json(id, &a)]})))
}

pub async fn create_group(State(st): State<AppState>, s: StaffCtx, Body(mut b): Body<GroupWrite>) -> ApiResult<Json<Value>> {
    s.require(ROLES_CONFIG)?;
    check_reason(&b.reason)?;
    b.group.product = b.product.unwrap_or_default();
    validate_group(&b.group)?;
    check_spread_group(&st, &s.ctx.tenant, &b.group)?;
    save_group(&st, &s, &b.group, &b.reason, true).await
}

/// Updates a group. Mode, cent flag and product are fixed once the group has accounts (they are copied into each
/// account's ledger currency / position model, and an account trades its group's product); other settings apply to
/// the group's accounts at once.
pub async fn update_group(State(st): State<AppState>, s: StaffCtx, Path(code): Path<String>, Body(mut b): Body<GroupWrite>) -> ApiResult<Json<Value>> {
    s.require(ROLES_CONFIG)?;
    check_reason(&b.reason)?;
    b.group.code = code.clone();
    validate_group(&b.group)?;
    check_spread_group(&st, &s.ctx.tenant, &b.group)?;
    let cur = s.ctx.tenant.groups.get(&code).ok_or_else(|| ApiError::NotFound(format!("Group {code} not found")))?;
    b.group.product = b.product.unwrap_or(cur.product);
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM accounts WHERE tenant_id = $1 AND group_code = $2").bind(s.ctx.tenant.tenant_id).bind(&code).fetch_one(&st.pool).await?;
    if used > 0 && (cur.mode != b.group.mode || cur.cent != b.group.cent) {
        return Err(ApiError::Validation { field: "mode", message: "Mode and cent cannot change while the group has accounts".into() });
    }
    if used > 0 && cur.product != b.group.product {
        return Err(ApiError::Validation { field: "product", message: "The product (CFD / Options) cannot change while the group has accounts".into() });
    }
    let _ = Arc::strong_count(&s.ctx.tenant);
    save_group(&st, &s, &b.group, &b.reason, false).await
}

/// House and client ledger balances (reconciliation, D141).
pub async fn ledger_accounts(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let rows = sqlx::query("SELECT code, currency, balance FROM ledger_accounts WHERE tenant_id = $1 AND code LIKE 'house:%' ORDER BY code").bind(s.ctx.tenant.tenant_id).fetch_all(&st.pool).await?;
    let house: Vec<Value> = rows.iter().map(|r| json!({"code": r.get::<String, _>("code"), "currency": r.get::<String, _>("currency"), "balance": num(r.get("balance"))})).collect();
    let totals: Vec<(String, D)> = sqlx::query_as("SELECT currency, COALESCE(sum(balance),0) FROM ledger_accounts WHERE tenant_id = $1 GROUP BY currency").bind(s.ctx.tenant.tenant_id).fetch_all(&st.pool).await?;
    Ok(Json(json!({"house": house, "netByCurrency": totals.iter().map(|(c, v)| json!({"currency": c, "net": num(*v)})).collect::<Vec<_>>()})))
}

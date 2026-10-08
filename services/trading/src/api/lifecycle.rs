//! Client Area account lifecycle (B1–B8): archive check, archive (with the optional "empty first" steps:
//! close trades and cancel orders, move the withdrawable balance to the wallet), restore and rename. Called by
//! the CRM BFF with the signed-in gateway user id in `X-Kalks-User-Id` (or `?user_id=`), like the other
//! `/v1/accounts` routes; an account the user does not own is a 404.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};

use super::accounts::{UserQ, owned, user_of};
use super::{ApiError, ApiResult, AppState, Body, Ctx};
use crate::engine::trade::{self, BulkFilter};
use crate::engine::{Reject, funds, metrics};
use crate::model::{AccountKind, Status};
use crate::money::{D, ZERO, num};
use crate::shard::{ExecError, Op};

/// What the archive wizard needs to know about one account.
#[derive(Clone, Debug)]
pub struct Check {
    pub login: i64,
    pub kind: AccountKind,
    pub status: Status,
    pub group: String,
    pub positions: usize,
    pub orders: usize,
    pub balance: D,
    pub credit: D,
    pub bonus: D,
    /// Withdrawable, in USD (cent accounts divided by 100).
    pub withdrawable_usd: D,
    pub version: i64,
    /// Account currency per USD (100 for cent accounts).
    pub usd_factor: D,
    pub user_id: i64,
    pub blockers: Vec<(&'static str, String)>,
}

impl Check {
    /// Something must be closed or moved out before the account can be archived.
    pub fn needs_empty(&self) -> bool {
        self.positions > 0 || self.orders > 0 || (self.kind == AccountKind::Live && crate::social::copier::returnable(self.withdrawable_usd) > ZERO)
    }
    pub fn json(&self) -> Value {
        json!({
            "login": self.login,
            "kind": self.kind.as_str(),
            "status": self.status.as_str(),
            "positions": self.positions,
            "orders": self.orders,
            "balance": num(self.balance),
            "credit": num(self.credit),
            "bonus": num(self.bonus),
            "canArchive": self.blockers.is_empty(),
            "needsEmpty": self.needs_empty(),
            "blockers": self.blockers.iter().map(|(c, m)| json!({"code": c, "message": m})).collect::<Vec<_>>(),
        })
    }
}

/// Reads the account and lists what blocks archiving it (copy / master / PAMM / MAM / prop, already retired).
pub async fn check(st: &AppState, login: i64) -> ApiResult<Check> {
    let v = st
        .hub
        .read(
            login,
            Box::new(|x| match x {
                Some((a, env)) => {
                    let m = metrics(env, a);
                    let f = a.account.usd_factor();
                    json!({
                        "kind": a.account.kind.as_str(), "status": a.account.status.as_str(), "group": a.account.group,
                        "positions": a.positions.len(), "orders": a.orders.len(), "balance": a.balance.to_string(),
                        "credit": a.credit.to_string(), "bonus": a.bonus.to_string(),
                        "withdrawableUsd": (m.withdrawable() / f).to_string(), "version": a.version,
                        "factor": f.to_string(), "userId": a.account.user_id,
                    })
                }
                None => Value::Null,
            }),
        )
        .await;
    if v.is_null() {
        return Err(ApiError::NotFound("Account not found".into()));
    }
    let dec = |k: &str| v[k].as_str().and_then(|s| s.parse::<D>().ok()).unwrap_or(ZERO);
    let kind = if v["kind"] == "live" { AccountKind::Live } else { AccountKind::Demo };
    let status = v["status"].as_str().and_then(Status::parse).unwrap_or(Status::Active);
    let group = v["group"].as_str().unwrap_or("").to_string();
    let mut blockers: Vec<(&'static str, String)> = Vec::new();
    match status {
        Status::Archived => blockers.push(("already_archived", "This account is already archived".into())),
        Status::Closed => blockers.push(("closed", "This account is closed".into())),
        _ => {}
    }
    {
        let reg = st.social.reg.read().unwrap();
        if reg.sub_by_login(login).is_some_and(|s| s.copying()) {
            blockers.push(("copy_subscription", "This account is copying a master. Stop copying first (Social → My subscriptions).".into()));
        }
        if let Some(m) = reg.master_by_login(login) {
            let n = reg.subs_of(m.id).len();
            if n > 0 {
                blockers.push(("master_followers", format!("This is a master account with {n} follower(s). Stop offering copy trading first.")));
            }
        }
        if reg.fund_by_login(login).is_some() {
            blockers.push(("pamm_fund", "This is a PAMM fund account and cannot be archived here.".into()));
        }
        if reg.managers.values().any(|m| m.login == login) || reg.links.values().any(|l| l.login == login && l.status == "active") {
            blockers.push(("mam_link", "This account is part of a MAM. Leave the MAM first.".into()));
        }
    }
    if group.to_ascii_lowercase().starts_with("prop") {
        blockers.push(("prop_account", "Prop challenge accounts cannot be archived.".into()));
    }
    Ok(Check {
        login,
        kind,
        status,
        group,
        positions: v["positions"].as_u64().unwrap_or(0) as usize,
        orders: v["orders"].as_u64().unwrap_or(0) as usize,
        balance: dec("balance"),
        credit: dec("credit"),
        bonus: dec("bonus"),
        withdrawable_usd: dec("withdrawableUsd"),
        version: v["version"].as_i64().unwrap_or(0),
        usd_factor: v["factor"].as_str().and_then(|s| s.parse::<D>().ok()).filter(|f| !f.is_zero()).unwrap_or(D::ONE),
        user_id: v["userId"].as_i64().unwrap_or(0),
        blockers,
    })
}

pub async fn archive_check(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    Ok(Json(check(&st, login).await?.json()))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveReq {
    #[serde(default)]
    pub empty: bool,
    #[serde(default)]
    pub ack_forfeit: bool,
}

fn step(name: &str, ok: bool, detail: impl Into<String>) -> Value {
    json!({"step": name, "ok": ok, "detail": detail.into()})
}

fn exec_err(e: ExecError) -> String {
    match e {
        ExecError::Reject(r) => r.message,
        other => format!("{other:?}"),
    }
}

/// Cancels every pending order and closes every position as the client, all or nothing.
pub fn close_everything(tx: &mut crate::engine::Tx, env: &crate::engine::Env) -> Result<Value, Reject> {
    let orders: Vec<i64> = tx.st.orders.keys().copied().collect();
    for t in &orders {
        trade::cancel_order(tx, env, *t, "account archived")?;
    }
    let out = trade::bulk_close(tx, env, BulkFilter::All, None);
    if let Some((t, e)) = out.failed.first() {
        return Err(Reject::new("close_failed", format!("Position #{t} could not be closed: {e}")));
    }
    Ok(json!({"closed": out.done.len(), "cancelled": orders.len(), "profit": num(out.profit)}))
}

/// The archive stepper. Steps that ran are reported even when a later one fails (the account then stays as
/// it was, only emptied). Archiving an archived account is a no-op success.
pub async fn archive(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<ArchiveReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let c = check(&st, login).await?;
    if c.status == Status::Archived {
        return Ok(Json(json!({"ok": true, "status": "archived", "steps": []})));
    }
    if let Some((code, message)) = c.blockers.first() {
        return Err(ApiError::Conflict { code: *code, message: message.clone() });
    }
    if c.needs_empty() && !r.empty {
        return Err(ApiError::Conflict { code: "not_empty", message: "Close all trades and move the balance to your wallet first (or archive with empty = true)".into() });
    }
    if (c.credit > ZERO || c.bonus > ZERO) && !r.ack_forfeit {
        return Err(ApiError::Validation { field: "ackForfeit", message: "Credit and bonus on this account are forfeited when it is archived; confirm to continue".into() });
    }
    let actor = format!("user:{user}");
    let fail = |steps: Vec<Value>, status: Status| Ok(Json(json!({"ok": false, "status": status.as_str(), "steps": steps})));
    let (mut steps, emptied) = empty_steps(&st, &ctx, user, login, &c, "archive").await?;
    if !emptied {
        return fail(steps, c.status);
    }
    let by = actor.clone();
    let op: Op = Box::new(move |tx, env| funds::archive(tx, env, &by, "CLIENT", true).map(|changed| json!({"changed": changed})));
    match st.hub.exec(login, &actor, None, "", "", None, op).await {
        Ok(_) => steps.push(step("archive", true, "Account archived")),
        Err(e) => {
            steps.push(step("archive", false, exec_err(e)));
            return fail(steps, c.status);
        }
    }
    revoke_sessions(&st, login).await;
    retired_hooks(&st, ctx.tenant.tenant_id, login, user, "archived").await;
    tracing::info!(login, user, "account archived by the client");
    Ok(Json(json!({"ok": true, "status": "archived", "steps": steps})))
}

/// The "empty first" steps of archive and close: cancel orders and close positions as the client, then move the
/// withdrawable balance of a live account to the wallet (idempotent key `<purpose>:{login}:{version}`). Returns the
/// steps that ran and whether all of them succeeded.
pub async fn empty_steps(st: &AppState, ctx: &Ctx, user: i64, login: i64, c: &Check, purpose: &str) -> ApiResult<(Vec<Value>, bool)> {
    let actor = format!("user:{user}");
    let mut steps = Vec::new();
    if c.positions > 0 || c.orders > 0 {
        let op: Op = Box::new(close_everything);
        match st.hub.exec(login, &actor, None, "", "", None, op).await {
            Ok(d) => steps.push(step("close_positions", true, format!("{} position(s) closed, {} order(s) cancelled", d.value["closed"], d.value["cancelled"]))),
            Err(e) => {
                steps.push(step("close_positions", false, exec_err(e)));
                return Ok((steps, false));
            }
        }
    }
    if c.kind == AccountKind::Live {
        let now = check(st, login).await?;
        let amt = crate::social::copier::returnable(now.withdrawable_usd);
        if amt > ZERO {
            let key = format!("{purpose}:{login}:{}", now.version);
            match st.social.wallet.from_trading(&st.social.slug(ctx.tenant.tenant_id), &key, user, login, amt).await {
                Ok(_) => steps.push(step("return_balance", true, format!("{} USD moved to the wallet", amt.normalize()))),
                Err(e) => {
                    steps.push(step("return_balance", false, e.message));
                    return Ok((steps, false));
                }
            }
        }
    }
    Ok((steps, true))
}

/// Ends the terminal sessions of a retired account.
pub async fn revoke_sessions(st: &AppState, login: i64) {
    if let Err(e) = sqlx::query("UPDATE terminal_sessions SET revoked_at = now() WHERE login = $1 AND revoked_at IS NULL").bind(login).execute(&st.pool).await {
        tracing::warn!(login, error = %e, "revoking sessions of an archived account failed");
    }
}

/// The account limit of its group still has room for one more (archived accounts do not count).
pub fn limit_allows(st: &AppState, ctx: &Ctx, login: i64) -> ApiResult<()> {
    let m = st.hub.meta(login).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    let Some(g) = ctx.tenant.groups.get(&m.group) else { return Ok(()) };
    if super::accounts::product_accounts_used(st, &ctx.tenant, m.user_id, m.kind, g.product, Some(login)) >= g.max_accounts_per_user {
        return Err(ApiError::Conflict { code: "account_limit", message: super::accounts::limit_message(g, m.kind) });
    }
    Ok(())
}

pub async fn restore(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let _guard = st.open_lock.lock().await;
    limit_allows(&st, &ctx, login)?;
    let op: Op = Box::new(|tx, _| funds::restore(tx, true).map(|s| json!({"status": s.as_str()})));
    let d = st.hub.exec(login, &format!("user:{user}"), None, "", "", None, op).await?;
    // a restored demo starts a fresh inactivity window (else the expiry job takes it straight back)
    let _ = sqlx::query("UPDATE accounts SET last_activity_at = now() WHERE login = $1").bind(login).execute(&st.pool).await;
    tracing::info!(login, user, "account restored by the client");
    Ok(Json(json!({"ok": true, "status": d.value["status"]})))
}

#[derive(Deserialize)]
pub struct RenameReq {
    name: String,
}

pub async fn rename(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<RenameReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    if r.name.trim().chars().count() > 32 {
        return Err(ApiError::Validation { field: "name", message: "The name can be at most 32 characters".into() });
    }
    let op: Op = Box::new(move |tx, _| funds::rename(tx, &r.name).map(|_| Value::Null));
    st.hub.exec(login, &format!("user:{user}"), None, "", "", None, op).await?;
    Ok(Json(json!({"ok": true})))
}

/* ------------------------------------------------------------------ */
/* Close permanently: the client's closure request (B12, C11)          */
/* ------------------------------------------------------------------ */

/// `GET /v1/accounts/{login}/closure`: whether the account can be closed and the latest request (client view).
pub async fn closure_status(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let c = check(&st, login).await?;
    let latest = super::closures::latest(&st, ctx.tenant.tenant_id, login, "close").await?;
    let mut blockers: Vec<Value> = c.blockers.iter().filter(|(code, _)| *code != "already_archived").map(|(code, m)| json!({"code": code, "message": m})).collect();
    if c.kind == AccountKind::Demo {
        blockers.push(json!({"code": "demo_account", "message": "Demo accounts are deleted (archived) instead of closed"}));
    }
    Ok(Json(json!({
        "login": login, "kind": c.kind.as_str(), "status": c.status.as_str(),
        "positions": c.positions, "orders": c.orders, "balance": num(c.balance), "credit": num(c.credit), "bonus": num(c.bonus),
        "needsEmpty": c.needs_empty(), "canRequest": blockers.is_empty() && !latest.as_ref().is_some_and(|r| sqlx::Row::get::<String, _>(r, "status") == "pending"),
        "blockers": blockers, "surveyReasons": super::closures::SURVEY_REASONS,
        "request": latest.as_ref().map(super::closures::client_view),
    })))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClosureReq {
    #[serde(default)]
    pub reason_code: String,
    #[serde(default)]
    pub survey: Survey,
    #[serde(default)]
    pub empty: bool,
    #[serde(default)]
    pub ack_forfeit: bool,
}

#[derive(Deserialize, Default)]
pub struct Survey {
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub comment: String,
}

/// `POST /v1/accounts/{login}/closure`: asks to close a live account permanently (B12). Optionally empties it first
/// (the archive steps: close trades, move the balance to the wallet); then a pending request goes to the Back Office
/// closure queue. The exit survey (C11) is stored with it. The CRM requires the emailed code (`account_close`).
pub async fn request_closure(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<ClosureReq>) -> ApiResult<Json<Value>> {
    use super::closures::{NewRequest, SURVEY_REASONS, insert_request, policy, total_usd};
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    if !SURVEY_REASONS.contains(&r.reason_code.as_str()) {
        return Err(ApiError::Validation { field: "reasonCode", message: format!("reasonCode must be one of {}", SURVEY_REASONS.join(", ")) });
    }
    let mut reasons: Vec<String> = r.survey.reasons.iter().filter(|x| SURVEY_REASONS.contains(&x.as_str())).cloned().collect();
    if !reasons.contains(&r.reason_code) {
        reasons.insert(0, r.reason_code.clone());
    }
    reasons.dedup();
    let comment: String = r.survey.comment.trim().chars().filter(|c| !c.is_control() || *c == '\n').take(1000).collect();
    let c = check(&st, login).await?;
    if c.kind == AccountKind::Demo {
        return Err(ApiError::Conflict { code: "demo_account", message: "Demo accounts are deleted (archived) instead of closed".into() });
    }
    if c.status == Status::Closed {
        return Err(ApiError::Conflict { code: "closed", message: "This account is already closed".into() });
    }
    if let Some((code, message)) = c.blockers.iter().find(|(code, _)| *code != "already_archived") {
        return Err(ApiError::Conflict { code, message: message.clone() });
    }
    if let Some(p) = super::closures::latest(&st, ctx.tenant.tenant_id, login, "close").await?
        && sqlx::Row::get::<String, _>(&p, "status") == "pending"
    {
        return Err(ApiError::Conflict { code: "request_pending", message: "A closure request for this account is already waiting for review".into() });
    }
    if c.needs_empty() && !r.empty {
        return Err(ApiError::Conflict { code: "not_empty", message: "Close all trades and move the balance to your wallet first (or send empty = true)".into() });
    }
    if (c.credit > ZERO || c.bonus > ZERO) && !r.ack_forfeit {
        return Err(ApiError::Validation { field: "ackForfeit", message: "Credit and bonus on this account are forfeited when it is closed; confirm to continue".into() });
    }
    let balance_usd = total_usd(&c);
    let (steps, emptied) = if c.status == Status::Archived { (vec![], true) } else { empty_steps(&st, &ctx, user, login, &c, "close").await? };
    if !emptied {
        return Ok(Json(json!({"ok": false, "steps": steps, "request": Value::Null})));
    }
    let pol = policy(&st.pool, ctx.tenant.tenant_id).await?;
    let row = insert_request(
        &st,
        NewRequest {
            tenant_id: ctx.tenant.tenant_id,
            login,
            user_id: user,
            kind: "close",
            source: "client",
            reason_code: &r.reason_code,
            survey: json!({"reasons": reasons, "comment": comment}),
            note: "",
            balance_usd,
            four_eyes: balance_usd > pol.close_four_eyes_usd,
            by: format!("user:{user}"),
            by_name: "",
        },
    )
    .await?;
    let id: i64 = sqlx::Row::get(&row, "id");
    crate::notify::enqueue_or_log(
        &st.pool,
        ctx.tenant.tenant_id,
        user,
        "account.closure_requested",
        json!({"title": format!("Closure of #{login} requested"), "body": format!("We received your request to close trading account #{login} permanently. Our team reviews it and lets you know the outcome."), "link": format!("/accounts/{login}"), "severity": "info", "data": {"login": login, "request": id}}),
        &format!("closure:{id}:requested"),
    )
    .await;
    tracing::info!(login, user, request = id, "closure requested by the client");
    Ok(Json(json!({"ok": true, "steps": steps, "request": super::closures::client_view(&row)})))
}

/// `POST /v1/accounts/{login}/closure/cancel`: withdraws the client's own pending closure request.
pub async fn cancel_closure(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let r = sqlx::query("UPDATE account_closures SET status = 'cancelled', decided_by = $3, decided_at = now(), updated_at = now() WHERE tenant_id = $1 AND login = $2 AND kind = 'close' AND status = 'pending' AND source = 'client' RETURNING *")
        .bind(ctx.tenant.tenant_id)
        .bind(login)
        .bind(format!("user:{user}"))
        .fetch_optional(&st.pool)
        .await?
        .ok_or(ApiError::Conflict { code: "no_request", message: "There is no pending closure request you can cancel".into() })?;
    Ok(Json(json!({"ok": true, "request": super::closures::client_view(&r)})))
}

/// After an account was archived or closed: open bonus grants end in the growth service (B7) and, for an account
/// closed or archived by staff or the system, the client is told. Best-effort (logged).
pub async fn retired_hooks(st: &AppState, tenant_id: i64, login: i64, user_id: i64, how: &str) {
    // B10: the final statement (PDF, full history) by email, live accounts only (reports service)
    let reports = std::env::var("REPORTS_URL").unwrap_or_default();
    if !reports.trim().is_empty() && st.hub.meta(login).is_some_and(|m| m.kind == AccountKind::Live) {
        let token = std::env::var("REPORTS_INTERNAL_TOKEN").unwrap_or_default();
        let client = crate::social::wallet::WalletClient::new(&reports, &token);
        let slug = st.social.slug(tenant_id);
        let body = json!({"userId": user_id, "reason": how});
        tokio::spawn(async move {
            if let Err(e) = client.post(&slug, &format!("/v1/internal/accounts/{login}/final-statement"), &body).await {
                tracing::warn!(login, error = %e, "reports: final statement email failed");
            }
        });
    }
    let url = std::env::var("GROWTH_URL").unwrap_or_default();
    if !url.trim().is_empty() {
        let token = std::env::var("GROWTH_INTERNAL_TOKEN").unwrap_or_default();
        let growth = crate::social::wallet::WalletClient::new(&url, &token);
        if let Err(e) = growth.post(&st.social.slug(tenant_id), &format!("/v1/growth/internal/accounts/{login}/retired"), &json!({"reason": how})).await {
            tracing::warn!(login, error = %e, "growth: ending the bonus grants of a retired account failed");
        }
    }
}

/* ------------------------------------------------------------------ */
/* Housekeeping (B8, B11, C9)                                          */
/* ------------------------------------------------------------------ */

/// What one housekeeping pass did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct JobReport {
    pub demos_archived: usize,
    pub dormant_flagged: usize,
    pub dormant_cleared: usize,
    pub dormant_archived: usize,
    pub anonymised: usize,
}

/// One pass of the account jobs for every broker:
/// - B8: expired demo accounts are archived `demo_archive_days` after they expired (client-restorable);
/// - B11: live accounts with no activity (Kalks Trader sign-in, a client trade, a wallet transfer) for `dormant_days`
///   are flagged dormant and the client is reminded once (no fee); activity clears the flag; empty dormant
///   accounts are archived when `dormant_auto_archive` is on;
/// - C9: closed accounts are anonymised `retention_years` after closing (the name and the exit-survey text are
///   cleared; trades and ledger are kept; the append-only event log keeps the old name — documented limitation).
pub async fn run_account_jobs(st: &AppState) -> anyhow::Result<JobReport> {
    let mut rep = JobReport::default();
    let tenants: Vec<i64> = sqlx::query_scalar("SELECT id FROM tenants").fetch_all(&st.pool).await?;
    for tenant in tenants {
        let pol = super::closures::policy(&st.pool, tenant).await?;
        // B8: expired demos
        if pol.demo_archive_days > 0 {
            let due: Vec<i64> = sqlx::query_scalar("SELECT login FROM accounts WHERE tenant_id = $1 AND kind = 'demo' AND status = 'expired' AND updated_at < now() - make_interval(days => $2) ORDER BY login LIMIT 1000")
                .bind(tenant)
                .bind(pol.demo_archive_days)
                .fetch_all(&st.pool)
                .await?;
            for login in due {
                let op: Op = Box::new(|tx, env| {
                    if !tx.st.positions.is_empty() || !tx.st.orders.is_empty() || !tx.st.book.is_idle() {
                        close_everything(tx, env)?;
                    }
                    funds::archive(tx, env, "system", "DEMO_EXPIRED", true).map(|c| json!({"changed": c}))
                });
                match st.hub.exec(login, "system", None, "DEMO_EXPIRED", "expired demo auto-archive", None, op).await {
                    Ok(_) => {
                        rep.demos_archived += 1;
                        revoke_sessions(st, login).await;
                    }
                    Err(e) => tracing::warn!(login, error = ?e, "expired demo auto-archive failed"),
                }
            }
        }
        if pol.dormant_days > 0 {
            // activity = latest of Kalks Trader sign-in / restore (last_activity_at), a client trade, a wallet transfer
            let activity = "GREATEST(a.last_activity_at,
                    (SELECT max(d.time) FROM deals d WHERE d.login = a.login AND d.source NOT IN ('system', 'dealer')),
                    (SELECT max(t.created_at) FROM ledger_txns t WHERE t.login = a.login AND t.kind IN ('transfer_in', 'transfer_out')))";
            let cleared = sqlx::query(sqlx::AssertSqlSafe(format!("UPDATE accounts a SET dormant_since = NULL, dormant_notified_at = NULL WHERE a.tenant_id = $1 AND a.dormant_since IS NOT NULL AND {activity} > a.dormant_since")))
                .bind(tenant)
                .execute(&st.pool)
                .await?;
            rep.dormant_cleared += cleared.rows_affected() as usize;
            let flagged = sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE accounts a SET dormant_since = now() WHERE a.tenant_id = $1 AND a.kind = 'live' AND a.dormant_since IS NULL
                   AND a.status NOT IN ('archived', 'closed') AND a.group_code NOT ILIKE 'prop%' AND {activity} < now() - make_interval(days => $2)
                 RETURNING a.login, a.user_id, a.balance"
            )))
            .bind(tenant)
            .bind(pol.dormant_days)
            .fetch_all(&st.pool)
            .await?;
            for r in &flagged {
                let (login, user): (i64, i64) = (sqlx::Row::get(r, "login"), sqlx::Row::get(r, "user_id"));
                crate::notify::enqueue_or_log(
                    &st.pool,
                    tenant,
                    user,
                    "account.dormant",
                    json!({"title": format!("Account #{login} is inactive"), "body": format!("Trading account #{login} has had no activity for {} days. There is no fee. Sign in to Kalks Trader or make a transfer to keep it active{}.", pol.dormant_days, if pol.dormant_auto_archive { "; empty inactive accounts are archived automatically and you can restore them any time" } else { "" }), "link": "/accounts", "severity": "info", "data": {"login": login}}),
                    &format!("dormant:{login}:{}", chrono::Utc::now().date_naive()),
                )
                .await;
                let _ = sqlx::query("UPDATE accounts SET dormant_notified_at = now() WHERE login = $1").bind(login).execute(&st.pool).await;
            }
            rep.dormant_flagged += flagged.len();
            if pol.dormant_auto_archive {
                let empty = super::closures::bulk_candidates(st, tenant, "empty_dormant", &[]).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
                for login in empty {
                    let Ok(c) = check(st, login).await else { continue };
                    if c.blockers.iter().any(|(code, _)| *code != "already_archived") || c.status.is_retired() {
                        continue;
                    }
                    let op: Op = Box::new(|tx, env| funds::archive(tx, env, "system", "DORMANT", true).map(|c| json!({"changed": c})));
                    match st.hub.exec(login, "system", None, "DORMANT", "empty dormant account auto-archive", None, op).await {
                        Ok(_) => {
                            rep.dormant_archived += 1;
                            revoke_sessions(st, login).await;
                            retired_hooks(st, tenant, login, c.user_id, "archived").await;
                            crate::notify::enqueue_or_log(
                                &st.pool,
                                tenant,
                                c.user_id,
                                "account.archived",
                                json!({"title": format!("Inactive account #{login} archived"), "body": format!("Trading account #{login} was empty and inactive, so we archived it. Its statements stay available and you can restore it from Accounts › Archived."), "link": "/accounts?tab=archived", "severity": "info", "data": {"login": login}}),
                                &format!("dormant-archive:{login}:{}", c.version),
                            )
                            .await;
                        }
                        Err(e) => tracing::warn!(login, error = ?e, "dormant auto-archive failed"),
                    }
                }
            }
        }
        // C9: retention
        let due: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT c.login FROM account_closures c JOIN accounts a ON a.login = c.login
             WHERE c.tenant_id = $1 AND c.kind = 'close' AND c.status = 'approved' AND a.status = 'closed' AND a.anonymised_at IS NULL
               AND c.decided_at < now() - make_interval(years => $2) LIMIT 1000",
        )
        .bind(tenant)
        .bind(pol.retention_years)
        .fetch_all(&st.pool)
        .await?;
        for login in due {
            let op: Op = Box::new(|tx, _| funds::rename(tx, "").map(|_| Value::Null));
            if let Err(e) = st.hub.exec(login, "system", None, "RETENTION", "retention anonymisation", None, op).await {
                tracing::warn!(login, error = ?e, "anonymisation failed");
                continue;
            }
            sqlx::query("UPDATE accounts SET anonymised_at = now() WHERE login = $1").bind(login).execute(&st.pool).await?;
            sqlx::query("UPDATE account_closures SET survey = jsonb_build_object('reasons', COALESCE(survey->'reasons', '[]'::jsonb)), note = '', client_message = NULL, decision_note = NULL, updated_at = now() WHERE login = $1")
                .bind(login)
                .execute(&st.pool)
                .await?;
            rep.anonymised += 1;
        }
    }
    Ok(rep)
}

/// The account jobs, hourly (first pass a minute after start).
pub async fn account_jobs(st: AppState) {
    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        tick.tick().await;
        match run_account_jobs(&st).await {
            Ok(r) if r != JobReport::default() => tracing::info!(?r, "account jobs"),
            Ok(_) => {}
            Err(e) => tracing::error!(error = %e, "account jobs failed"),
        }
    }
}

/* ------------------------------------------------------------------ */
/* Account extras (B9, B10)                                            */
/* ------------------------------------------------------------------ */

/// Lifecycle fields for account rows, by login: archive / close dates, dormancy, the open closure request and the
/// client's default account. One query per list.
pub async fn decorate(st: &AppState, tenant_id: i64, items: &mut [Value]) -> ApiResult<()> {
    let logins: Vec<i64> = items.iter().filter_map(|v| v["login"].as_i64()).collect();
    if logins.is_empty() {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT a.login, a.user_id, a.dormant_since, a.last_activity_at, a.anonymised_at,
                (SELECT c.status FROM account_closures c WHERE c.login = a.login AND c.kind = 'close' ORDER BY c.id DESC LIMIT 1) AS closure_status,
                (SELECT c.id FROM account_closures c WHERE c.login = a.login AND c.kind = 'close' ORDER BY c.id DESC LIMIT 1) AS closure_id,
                (p.default_login = a.login) AS is_default
           FROM accounts a LEFT JOIN account_prefs p ON p.tenant_id = a.tenant_id AND p.user_id = a.user_id
          WHERE a.tenant_id = $1 AND a.login = ANY($2)",
    )
    .bind(tenant_id)
    .bind(&logins)
    .fetch_all(&st.pool)
    .await?;
    let mut lc = std::collections::HashMap::new();
    for l in &logins {
        let v = st.hub.read(*l, Box::new(|x| x.and_then(|(a, _)| a.account.lifecycle.as_ref().map(|l| json!({"archivedAt": l.archived_at, "closedAt": l.closed_at, "reasonCode": l.reason_code, "by": l.by, "clientRestorable": l.client_restorable}))).unwrap_or(Value::Null))).await;
        lc.insert(*l, v);
    }
    for v in items.iter_mut() {
        let Some(login) = v["login"].as_i64() else { continue };
        if let Some(l) = lc.get(&login).filter(|l| !l.is_null()) {
            v["archivedAt"] = l["archivedAt"].clone();
            v["closedAt"] = l["closedAt"].clone();
            v["lifecycle"] = l.clone();
        }
        if let Some(r) = rows.iter().find(|r| sqlx::Row::get::<i64, _>(*r, "login") == login) {
            use sqlx::Row as _;
            v["dormantSince"] = json!(r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("dormant_since"));
            v["lastActivityAt"] = json!(r.get::<chrono::DateTime<chrono::Utc>, _>("last_activity_at"));
            v["isDefault"] = json!(r.get::<Option<bool>, _>("is_default").unwrap_or(false));
            v["closureRequest"] = match (r.get::<Option<String>, _>("closure_status"), r.get::<Option<i64>, _>("closure_id")) {
                (Some(s), Some(id)) => json!({"id": id, "status": s}),
                _ => Value::Null,
            };
            if r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("anonymised_at").is_some() {
                v["anonymised"] = json!(true);
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrefsReq {
    /// The default account (null clears it).
    pub default_login: Option<i64>,
}

/// `GET /v1/accounts/prefs` → `{defaultLogin}`.
pub async fn get_prefs(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    let d: Option<Option<i64>> = sqlx::query_scalar("SELECT default_login FROM account_prefs WHERE tenant_id = $1 AND user_id = $2").bind(ctx.tenant.tenant_id).bind(user).fetch_optional(&st.pool).await?;
    Ok(Json(json!({"defaultLogin": d.flatten()})))
}

/// `PUT /v1/accounts/prefs {defaultLogin}`: the starred default account (B9); must be one of the client's own
/// active accounts.
pub async fn put_prefs(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Query(q): Query<UserQ>, Body(r): Body<PrefsReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    if let Some(l) = r.default_login {
        let m = owned(&st, &ctx, l, user)?;
        if m.status.is_retired() {
            return Err(ApiError::Conflict { code: "account_status", message: "An archived or closed account can't be the default".into() });
        }
    }
    sqlx::query("INSERT INTO account_prefs (tenant_id, user_id, default_login, updated_at) VALUES ($1,$2,$3, now()) ON CONFLICT (tenant_id, user_id) DO UPDATE SET default_login = EXCLUDED.default_login, updated_at = now()")
        .bind(ctx.tenant.tenant_id)
        .bind(user)
        .bind(r.default_login)
        .execute(&st.pool)
        .await?;
    Ok(Json(json!({"ok": true, "defaultLogin": r.default_login})))
}

#[derive(Deserialize)]
pub struct GroupReq {
    pub group: String,
}

/// `GET /v1/accounts/{login}/group-options`: groups the client may move this account to, each with why not (B9).
pub async fn group_options(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let c = check(&st, login).await?;
    let mut out = Vec::new();
    // an account changes type within its product only (CFD / Options account split)
    let product = ctx.tenant.groups.get(&c.group).map(|g| g.product).unwrap_or_default();
    let mut groups: Vec<&crate::rules::Group> = ctx.tenant.groups.values().filter(|g| g.enabled && g.allows(c.kind.as_str()) && !is_special_group(&g.code) && g.product == product).collect();
    groups.sort_by(|a, b| a.code.cmp(&b.code));
    for g in groups {
        if g.code == c.group {
            continue;
        }
        let why = group_change_blocker(&st, &ctx, &c, g);
        out.push(json!({"code": g.code, "name": g.name, "mode": g.mode, "cent": g.cent, "minDeposit": num(g.min_deposit), "leverages": g.leverages, "commissionPerLot": num(g.commission_per_lot), "swapFree": g.swap_free,
                        "allowed": why.is_none(), "blocker": why.map(|(code, m)| json!({"code": code, "message": m}))}));
    }
    Ok(Json(json!({"login": login, "group": c.group, "flat": c.positions == 0 && c.orders == 0, "groups": out})))
}

/// copy / PAMM / MAM / prop groups are system-managed.
fn is_special_group(code: &str) -> bool {
    let g = code.to_ascii_lowercase();
    g.starts_with("prop") || g == "copy" || g.starts_with("copy-") || g == "pamm" || g.starts_with("pamm-") || g == "mam" || g.starts_with("mam-")
}

fn group_change_blocker(st: &AppState, ctx: &Ctx, c: &Check, g: &crate::rules::Group) -> Option<(&'static str, String)> {
    if c.status.is_retired() {
        return Some(("account_status", format!("The account is {}", c.status.as_str())));
    }
    if let Some((code, m)) = c.blockers.first() {
        return Some((code, m.clone()));
    }
    if is_special_group(&c.group) || is_special_group(&g.code) {
        return Some(("special_group", "Copy, PAMM, MAM and prop accounts can't change their account type".into()));
    }
    if g.cent != (c.usd_factor != D::ONE) {
        return Some(("cent_mismatch", "An account can't move between cent and standard accounts".into()));
    }
    let product = ctx.tenant.groups.get(&c.group).map(|x| x.product).unwrap_or_default();
    if g.product != product {
        return Some(("product_mismatch", "A CFD account and an Options account can't change into each other: open a new account instead".into()));
    }
    if c.positions > 0 || c.orders > 0 {
        return Some(("positions_open", "Close all trades and orders first".into()));
    }
    if c.kind == AccountKind::Live && c.balance / c.usd_factor < g.min_deposit {
        return Some(("min_deposit", format!("{} needs at least {} USD on the account", g.name, g.min_deposit.normalize())));
    }
    if super::accounts::product_accounts_used(st, &ctx.tenant, c.user_id, c.kind, g.product, Some(c.login)) >= g.max_accounts_per_user {
        return Some(("account_limit", format!("You already have the most {} {} accounts allowed ({})", c.kind.as_str(), funds::product_name(g.product), g.max_accounts_per_user)));
    }
    None
}

/// `POST /v1/accounts/{login}/group {group}`: self-service account type change (B9): flat, minimum deposit met,
/// within the account limit of the new group; reuses the Back Office group change (`funds::change_group`).
pub async fn change_group(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<GroupReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let g = ctx.tenant.groups.get(&r.group).filter(|g| g.enabled).cloned().ok_or(ApiError::Validation { field: "group", message: "Unknown account type".into() })?;
    let _guard = st.open_lock.lock().await;
    let c = check(&st, login).await?;
    if !g.allows(c.kind.as_str()) {
        return Err(ApiError::Validation { field: "group", message: format!("{} doesn't accept {} accounts", g.name, c.kind.as_str()) });
    }
    if let Some((code, message)) = group_change_blocker(&st, &ctx, &c, &g) {
        return Err(ApiError::Conflict { code, message });
    }
    let g2 = g.clone();
    let op: Op = Box::new(move |tx, env| {
        if !tx.st.positions.is_empty() || !tx.st.orders.is_empty() || !tx.st.book.is_idle() {
            return Err(Reject::new("positions_open", "Close all trades and orders first"));
        }
        funds::change_group(tx, env, &g2).map(|(from, to)| json!({"from": from, "to": to, "leverage": tx.st.account.leverage}))
    });
    let d = st.hub.exec(login, &format!("user:{user}"), None, "", "", None, op).await?;
    tracing::info!(login, user, to = %g.code, "account type changed by the client");
    Ok(Json(json!({"ok": true, "group": g.code, "groupName": g.name, "change": d.value})))
}

#[derive(Deserialize)]
pub struct DemoBalanceReq {
    #[serde(deserialize_with = "crate::money::de_dec")]
    pub amount: D,
}

/// `POST /v1/accounts/{login}/demo-balance {amount}`: refill a demo account to a chosen balance (B10).
pub async fn demo_balance(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>, Body(r): Body<DemoBalanceReq>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let op: Op = Box::new(move |tx, env| funds::demo_refill_to(tx, env, r.amount).map(|amt| json!({"status": "ok", "amount": num(amt), "balance": num(tx.st.balance)})));
    let d = st.hub.exec(login, &format!("user:{user}"), None, "", "", None, op).await?;
    Ok(Json(d.value))
}

/// `GET /v1/accounts/{login}/health`: the account health card (B10): margin safety, stop-loss coverage, margin
/// use, floating loss, 30-day results and activity, each scored good / warn / bad, plus an overall 0–100 score.
pub async fn health(State(st): State<AppState>, ctx: Ctx, headers: HeaderMap, Path(login): Path<i64>, Query(q): Query<UserQ>) -> ApiResult<Json<Value>> {
    let user = user_of(&headers, q.user_id)?;
    owned(&st, &ctx, login, user)?;
    let v = st
        .hub
        .read(
            login,
            Box::new(|x| match x {
                Some((a, env)) => {
                    let m = metrics(env, a);
                    let with_sl = a.positions.values().filter(|p| p.sl.is_some()).count();
                    json!({"equity": m.equity.to_string(), "balance": m.balance.to_string(), "margin": m.margin.to_string(), "level": m.level.map(|l| l.to_string()),
                           "floating": (m.profit + m.swap).to_string(), "positions": a.positions.len(), "withSl": with_sl,
                           "stopOut": env.group.stop_out_pct.to_string(), "marginCall": env.group.margin_call_pct.to_string()})
                }
                None => Value::Null,
            }),
        )
        .await;
    if v.is_null() {
        return Err(ApiError::NotFound("Account not found".into()));
    }
    let dec = |k: &str| v[k].as_str().and_then(|s| s.parse::<D>().ok()).unwrap_or(ZERO);
    let since = chrono::Utc::now() - chrono::Duration::days(30);
    let (trades, wins, pnl, last): (i64, i64, Option<D>, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE entry <> 'in' AND time >= $2), count(*) FILTER (WHERE entry <> 'in' AND time >= $2 AND profit + swap - commission > 0),
                sum(profit + swap - commission) FILTER (WHERE entry <> 'in' AND time >= $2), max(time)
           FROM deals WHERE login = $1 AND NOT reversed",
    )
    .bind(login)
    .bind(since)
    .fetch_one(&st.pool)
    .await?;
    let mut items = Vec::new();
    let mut score = 0i64;
    let mut add = |key: &str, status: &str, value: Value| {
        score += match status {
            "good" => 2,
            "warn" => 1,
            _ => 0,
        };
        items.push(json!({"key": key, "status": status, "value": value}));
    };
    let positions = v["positions"].as_u64().unwrap_or(0);
    let equity = dec("equity");
    // margin level vs margin call / stop-out
    match v["level"].as_str().and_then(|s| s.parse::<D>().ok()) {
        Some(l) => {
            let mc = dec("marginCall");
            add("margin_level", if l >= mc * D::from(3) { "good" } else if l >= mc { "warn" } else { "bad" }, json!({"level": num(r2_(l)), "marginCall": num(mc), "stopOut": num(dec("stopOut"))}));
        }
        None => add("margin_level", "good", json!({"level": Value::Null})),
    }
    // stop-loss coverage
    let with_sl = v["withSl"].as_u64().unwrap_or(0);
    add("stop_loss", if positions == 0 || with_sl == positions { "good" } else if with_sl * 2 >= positions { "warn" } else { "bad" }, json!({"positions": positions, "withSl": with_sl}));
    // margin use = margin / equity
    let used = if equity > ZERO { dec("margin") * D::from(100) / equity } else { ZERO };
    add("margin_use", if used <= D::from(20) { "good" } else if used <= D::from(50) { "warn" } else { "bad" }, json!({"pct": num(r2_(used))}));
    // floating loss vs balance
    let bal = dec("balance");
    let fl = dec("floating");
    let loss_pct = if bal > ZERO && fl < ZERO { -fl * D::from(100) / bal } else { ZERO };
    add("floating", if loss_pct <= D::from(5) { "good" } else if loss_pct <= D::from(20) { "warn" } else { "bad" }, json!({"amount": num(r2_(fl)), "pctOfBalance": num(r2_(loss_pct))}));
    // last 30 days
    let pnl = pnl.unwrap_or(ZERO);
    add("results_30d", if trades == 0 || pnl >= ZERO { "good" } else if wins * 2 >= trades { "warn" } else { "bad" }, json!({"trades": trades, "wins": wins, "pnl": num(r2_(pnl)), "winRate": if trades > 0 { json!((wins as f64 * 1000.0 / trades as f64).round() / 10.0) } else { Value::Null }}));
    let n = items.len() as i64;
    Ok(Json(json!({"login": login, "score": if n > 0 { score * 100 / (n * 2) } else { 100 }, "items": items, "lastTradeAt": last})))
}

fn r2_(d: D) -> D {
    crate::money::r2(d)
}

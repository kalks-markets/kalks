//! Business flows: purchase → provision, phase pass / fail (with engine enforcement), payouts, scaling,
//! manual overrides and strategy-flag reviews. Every state change is audited.
//!
//! Concurrency: all read-modify-write of one phase account (evaluator ticks, payouts, overrides) runs under
//! that account's async lock ([`Svc::lock`]); transitions additionally claim the row with a conditional
//! UPDATE (`… WHERE status = 'active'`), so a breach and a pass can never both win.

use chrono::Utc;
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::certs;
use crate::config::Config;
use crate::engine::{Engine, EngineError};
use crate::error::{ApiError, ApiResult};
use crate::money::{D, ZERO, num, r2};
use crate::plans::{self, Plan};
use crate::rules;
use crate::store::{self, Actor, Challenge, PhaseAccount};
use crate::wallet::{Wallet, WalletOutcome};

pub const R_FUND: &str = "PRP-01 · Prop simulated capital";
pub const R_BREACH: &str = "PRP-02 · Prop rule breach";
pub const R_PASS: &str = "PRP-03 · Prop phase passed";
pub const R_PAYOUT: &str = "PRP-04 · Prop payout";
pub const R_RULE: &str = "PRP-05 · Prop trading restriction";
pub const R_OVERRIDE: &str = "PRP-06 · Prop manual override";

pub struct Svc {
    pub cfg: Config,
    pub pool: PgPool,
    pub engine: Engine,
    pub wallet: Wallet,
    /// Read-only gateway DB (users.kyc_status); None = trust the KYC status the BFF forwards.
    pub gateway: Option<PgPool>,
    locks: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
}

pub type App = Arc<Svc>;

impl Svc {
    pub fn new(cfg: Config, pool: PgPool, gateway: Option<PgPool>) -> Self {
        let engine = Engine::new(&cfg.trading_url, &cfg.trading_token);
        let wallet = Wallet::new(&cfg.wallet_url, &cfg.wallet_token);
        Svc { cfg, pool, engine, wallet, gateway, locks: Mutex::new(HashMap::new()) }
    }

    /// Per-phase-account async lock.
    pub fn lock(&self, account_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().unwrap().entry(account_id).or_default().clone()
    }

    /// `users.kyc_status` from the gateway DB when configured, else the status the BFF forwarded.
    pub async fn kyc_status(&self, user_id: i64, forwarded: Option<&str>) -> String {
        if let Some(g) = &self.gateway {
            match sqlx::query_scalar::<_, String>("SELECT kyc_status FROM users WHERE id = $1").bind(user_id).fetch_optional(g).await {
                Ok(Some(s)) => return s,
                Ok(None) => return "unverified".into(),
                Err(e) => tracing::warn!(error = %e, "gateway kyc lookup failed; using forwarded status"),
            }
        }
        forwarded.unwrap_or("unverified").to_string()
    }
}

fn engine_err(e: EngineError) -> ApiError {
    e.into()
}

/* ------------------------------------------------------------------ */
/* Purchase and provisioning                                           */
/* ------------------------------------------------------------------ */

#[derive(Debug)]
pub struct Purchase {
    pub challenge: Challenge,
    pub credentials: Option<Value>,
}

/// Buys a challenge with wallet funds (D147): wallet debit → engine account in the plan's group, funded with
/// the plan's balance → rules armed. `key` is the client's idempotency key (retries return the same purchase).
pub async fn purchase(app: &App, tenant: &str, user_id: i64, trader_name: &str, plan_id: &str, size: D, key: &str) -> ApiResult<Purchase> {
    if key.is_empty() || key.len() > 80 || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(ApiError::Validation { field: "idempotencyKey", message: "idempotencyKey: 1–80 letters, digits, - or _".into() });
    }
    let purchase_key = format!("u{user_id}:{key}");
    if let Some(id) = sqlx::query_scalar::<_, i64>("SELECT id FROM challenges WHERE tenant = $1 AND purchase_key = $2").bind(tenant).bind(&purchase_key).fetch_optional(&app.pool).await? {
        let c = store::challenge(&app.pool, id).await?.ok_or_else(|| ApiError::NotFound("Challenge not found".into()))?;
        if c.user_id != user_id {
            return Err(ApiError::Conflict { code: "idempotency_conflict", message: "This key belongs to another request".into() });
        }
        return continue_purchase(app, c).await;
    }
    let plan = plans::get(&app.pool, tenant, plan_id).await?;
    if plan.status != "active" {
        return Err(ApiError::rule("plan_unavailable", "This plan is not on sale right now"));
    }
    let row = plan.size(size).filter(|s| s.enabled).ok_or(ApiError::Validation { field: "size", message: "This account size isn't offered on the plan".into() })?.clone();
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO challenges (tenant, user_id, trader_name, plan_id, plan_version, plan_name, kind, size, fee, leverage, engine_group, rules, status, purchase_key, split_pct)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,'pending_payment',$13,$14) RETURNING id",
    )
    .bind(tenant)
    .bind(user_id)
    .bind(trader_name.chars().take(120).collect::<String>())
    .bind(&plan.id)
    .bind(plan.version)
    .bind(&plan.name)
    .bind(&plan.kind)
    .bind(row.size)
    .bind(row.fee)
    .bind(row.leverage as i32)
    .bind(&plan.group)
    .bind(sqlx::types::Json(&plan))
    .bind(&purchase_key)
    .bind(plan.split)
    .fetch_one(&app.pool)
    .await?;
    store::audit(&app.pool, tenant, &Actor::user(user_id), "challenge.purchase", "challenge", &id.to_string(), None, Some(json!({"plan": plan.id, "version": plan.version, "size": num(row.size), "fee": num(row.fee)})), None, None).await;
    let c = store::challenge(&app.pool, id).await?.unwrap();
    continue_purchase(app, c).await
}

/// Charges (if not yet charged) and provisions a pending purchase. Safe to call repeatedly.
pub async fn continue_purchase(app: &App, c: Challenge) -> ApiResult<Purchase> {
    match c.status.as_str() {
        "pending_payment" => {
            if c.fee > ZERO {
                let key = format!("prop-purchase-{}", c.id);
                let out = app.wallet.transfer(&app.pool, &c.tenant, &key, c.user_id, "debit", "prop_purchase", c.fee, &format!("prop:challenge:{}", c.id), &format!("{} · {} challenge", c.plan_name, certs::money_text(c.size))).await;
                match out {
                    WalletOutcome::Done(_) => {}
                    WalletOutcome::Rejected { code, message } => {
                        sqlx::query("UPDATE challenges SET status = 'payment_failed', failure_reason = $2, updated_at = now() WHERE id = $1 AND status = 'pending_payment'").bind(c.id).bind(&message).execute(&app.pool).await?;
                        let code: &'static str = match code.as_str() {
                            "insufficient_funds" => "insufficient_funds",
                            "wallet_not_found" | "not_found" => "wallet_not_found",
                            _ => "payment_failed",
                        };
                        return Err(ApiError::rule(code, if code == "insufficient_funds" { "Your wallet balance is too low for this challenge. Deposit USDT and try again.".to_string() } else { message }));
                    }
                    WalletOutcome::Unknown(m) => {
                        tracing::warn!(challenge = c.id, error = %m, "wallet outcome unknown; will retry");
                        return Err(ApiError::Upstream { code: "payment_pending".into(), message: "The wallet didn't confirm the payment yet. We'll retry automatically; nothing is charged twice.".into() });
                    }
                }
            }
            sqlx::query("UPDATE challenges SET status = 'provisioning', wallet_ref = $2, updated_at = now() WHERE id = $1 AND status = 'pending_payment'")
                .bind(c.id)
                .bind(format!("prop-purchase-{}", c.id))
                .execute(&app.pool)
                .await?;
            let c = store::challenge(&app.pool, c.id).await?.unwrap();
            provision(app, c).await
        }
        "provisioning" => provision(app, c).await,
        "payment_failed" => Err(ApiError::rule("payment_failed", c.failure_reason.unwrap_or_else(|| "The payment failed".into()))),
        _ => Ok(Purchase { challenge: c, credentials: None }),
    }
}

/// Terms of phase `index` (index == phases.len() is the funded account).
fn phase_terms(plan: &Plan, index: usize) -> (String, bool, Option<D>, i32, i32) {
    match plan.phases.get(index) {
        Some(p) => (p.name.clone(), false, Some(p.target), p.min_days as i32, p.time_limit as i32),
        None => ("Funded".into(), true, None, 0, 0),
    }
}

/// Opens (or finishes opening) the engine account for the challenge's current phase and funds it.
async fn provision(app: &App, c: Challenge) -> ApiResult<Purchase> {
    let index = c.phase_index as usize;
    let (name, funded, target, min_days, limit) = phase_terms(&c.rules, index);
    sqlx::query(
        "INSERT INTO phase_accounts (tenant, challenge_id, user_id, phase_index, phase_name, funded, initial_balance, target_pct, min_days, time_limit_days)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (challenge_id, phase_index) DO NOTHING",
    )
    .bind(&c.tenant)
    .bind(c.id)
    .bind(c.user_id)
    .bind(c.phase_index)
    .bind(&name)
    .bind(funded)
    .bind(c.size)
    .bind(target)
    .bind(min_days)
    .bind(limit)
    .execute(&app.pool)
    .await?;
    let pa: PhaseAccount = {
        let id: i64 = sqlx::query_scalar("SELECT id FROM phase_accounts WHERE challenge_id = $1 AND phase_index = $2").bind(c.id).bind(c.phase_index).fetch_one(&app.pool).await?;
        store::phase(&app.pool, id).await?.unwrap()
    };
    if pa.status != "provisioning" {
        return Ok(Purchase { challenge: store::challenge(&app.pool, c.id).await?.unwrap(), credentials: None });
    }
    let mut credentials = None;
    let login = match pa.login {
        Some(l) => l,
        None => {
            let label: String = format!("{} · {}", c.plan_name, name).chars().take(80).collect();
            match app.engine.open_account(&c.tenant, c.user_id, &c.engine_group, c.leverage as u32, &label).await {
                Ok((login, creds)) => {
                    sqlx::query("UPDATE phase_accounts SET login = $2 WHERE id = $1").bind(pa.id).bind(login).execute(&app.pool).await?;
                    credentials = Some(creds);
                    login
                }
                Err(EngineError::Refused(status, code, message)) if status < 500 => {
                    // the engine will not open it (limit, group disabled, leverage): refund the fee
                    tracing::error!(challenge = c.id, %code, %message, "engine refused the prop account");
                    refund_purchase(app, &c, &format!("Account could not be opened: {message}")).await?;
                    return Err(ApiError::rule("account_unavailable", format!("We couldn't open the challenge account ({message}). Your fee was refunded to your wallet.")));
                }
                Err(e) => {
                    tracing::warn!(challenge = c.id, error = %e, "engine unavailable while provisioning; will retry");
                    return Err(ApiError::Upstream { code: "provisioning".into(), message: "Your payment is confirmed. The trading account is being opened and will appear in a moment.".into() });
                }
            }
        }
    };
    app.engine
        .adjust(&c.tenant, login, c.size, &format!("prop-fund-{}", pa.id), R_FUND, &format!("Challenge #{} {} · simulated capital", c.id, name))
        .await
        .map_err(engine_err)?;
    let now = Utc::now();
    sqlx::query(
        "UPDATE phase_accounts SET status = 'active', started_at = $2, balance = $3, equity = $3, hwm = $3, min_equity = $3, day_start_balance = $3, day_start_equity = $3
         WHERE id = $1 AND status = 'provisioning'",
    )
    .bind(pa.id)
    .bind(now)
    .bind(c.size)
    .execute(&app.pool)
    .await?;
    let status = if funded { "funded" } else { "active" };
    sqlx::query("UPDATE challenges SET status = $2, updated_at = now() WHERE id = $1").bind(c.id).bind(status).execute(&app.pool).await?;
    store::audit(&app.pool, &c.tenant, &Actor::system(), "phase.started", "phase_account", &pa.id.to_string(), None, Some(json!({"login": login, "phase": name, "balance": num(c.size)})), None, None).await;
    if funded {
        let _ = certs::issue(&app.pool, &c.tenant, c.user_id, c.id, "funded", "Funded trader", &c.trader_name, &c.plan_name, c.size, None, Some(&name), &format!("funded:{}", c.id)).await;
        store::notify(&app.pool, &c.tenant, c.user_id, Some(c.id), "funded", "Your funded account is ready", &format!("Account #{login} ({}) is live. Payouts open after {} days.", certs::money_text(c.size), c.rules.first_payout_days)).await;
    } else {
        store::notify(&app.pool, &c.tenant, c.user_id, Some(c.id), "phase_started", &format!("{name} started"), &format!("Account #{login} is funded with {}. Rules are live from now.", certs::money_text(c.size))).await;
    }
    Ok(Purchase { challenge: store::challenge(&app.pool, c.id).await?.unwrap(), credentials })
}

async fn refund_purchase(app: &App, c: &Challenge, reason: &str) -> ApiResult<()> {
    if c.fee > ZERO {
        let out = app.wallet.transfer(&app.pool, &c.tenant, &format!("prop-purchase-{}-refund", c.id), c.user_id, "credit", "refund", c.fee, &format!("prop:challenge:{}", c.id), "Challenge refund: account could not be opened").await;
        if !matches!(out, WalletOutcome::Done(_)) {
            tracing::error!(challenge = c.id, ?out, "purchase refund not confirmed; left for the reconciler");
        }
    }
    sqlx::query("UPDATE challenges SET status = 'closed', failure_reason = $2, updated_at = now() WHERE id = $1").bind(c.id).bind(reason).execute(&app.pool).await?;
    store::audit(&app.pool, &c.tenant, &Actor::system(), "challenge.refunded", "challenge", &c.id.to_string(), None, Some(json!({"fee": num(c.fee), "reason": reason})), None, None).await;
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Enforcement: fail / pass                                            */
/* ------------------------------------------------------------------ */

/// Closes everything on the engine account and sets its final status. Idempotent; records the outcome in
/// `stats.enforced` so the reconciler retries until the engine confirms.
pub async fn enforce(app: &App, a: &PhaseAccount, engine_status: &str, reason: &str, note: &str) -> bool {
    let Some(login) = a.login else { return true };
    // stop new orders first, then flatten, then lock the account
    let _ = app.engine.set_status(&a.tenant, login, "close_only", reason, note).await;
    let left = match app.engine.close_all(&a.tenant, login, reason, note).await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(login, error = %e, "close-all failed; will retry");
            usize::MAX
        }
    };
    let status_ok = left == 0 && app.engine.set_status(&a.tenant, login, engine_status, reason, note).await.is_ok();
    let _ = sqlx::query("UPDATE phase_accounts SET stats = jsonb_set(stats, '{enforced}', to_jsonb($2::bool)) WHERE id = $1").bind(a.id).bind(status_ok).execute(&app.pool).await;
    status_ok
}

/// Final balance / equity after enforcement (closed accounts are no longer evaluated).
async fn refresh_numbers(app: &App, a: &PhaseAccount) {
    let Some(login) = a.login else { return };
    if let Ok(s) = app.engine.account(&a.tenant, login).await {
        // final trading statistics and dashboard numbers after the close-all
        let mut stats = a.stats.clone();
        if let Ok(deals) = app.engine.deals(&a.tenant, login, a.started_at.unwrap_or_else(Utc::now) - chrono::Duration::minutes(1)).await {
            stats["trading"] = crate::evaluator::trade_stats(&deals, &s, a.started_at.unwrap_or_else(Utc::now)).json;
        }
        if stats.get("rules").is_some_and(Value::is_object) {
            stats["rules"]["balance"] = num(s.balance);
            stats["rules"]["equity"] = num(s.equity);
            stats["rules"]["profit"] = num(s.balance - a.initial_balance);
        }
        let _ = sqlx::query("UPDATE phase_accounts SET balance = $2, equity = $3, open_positions = $4, stats = stats || $5 WHERE id = $1")
            .bind(a.id)
            .bind(s.balance)
            .bind(s.equity)
            .bind(s.positions.len() as i32)
            .bind(sqlx::types::Json(json!({"trading": stats["trading"], "rules": stats["rules"]})))
            .execute(&app.pool)
            .await;
        let _ = sqlx::query("INSERT INTO equity_points (account_id, tenant, at, balance, equity) VALUES ($1,$2,now(),$3,$4) ON CONFLICT DO NOTHING")
            .bind(a.id)
            .bind(&a.tenant)
            .bind(s.balance)
            .bind(s.equity)
            .execute(&app.pool)
            .await;
    }
}

/// Auto-fail (breach) or manual fail. Returns false when the account wasn't active any more.
pub async fn fail(app: &App, a: &PhaseAccount, rule: &str, message: &str, actor: &Actor, equity: Option<D>, threshold: Option<D>) -> ApiResult<bool> {
    let claimed = sqlx::query("UPDATE phase_accounts SET status = 'failed', ended_at = now(), end_reason = $2 WHERE id = $1 AND status = 'active'")
        .bind(a.id)
        .bind(rule)
        .execute(&app.pool)
        .await?
        .rows_affected();
    if claimed == 0 {
        return Ok(false);
    }
    sqlx::query("UPDATE challenges SET status = 'failed', failure_reason = $2, updated_at = now() WHERE id = $1").bind(a.challenge_id).bind(message).execute(&app.pool).await?;
    store::rule_event(&app.pool, a, rule, "breach", equity, a.balance, threshold, message, json!({"actor": actor.id}), Some(format!("breach-{rule}"))).await;
    let enforced = enforce(app, a, "disabled", R_BREACH, message).await;
    refresh_numbers(app, a).await;
    store::audit(&app.pool, &a.tenant, actor, "phase.failed", "phase_account", &a.id.to_string(), Some(json!({"status": "active"})), Some(json!({"status": "failed", "rule": rule, "login": a.login, "enforced": enforced})), Some(rule), Some(message)).await;
    store::notify(&app.pool, &a.tenant, a.user_id, Some(a.challenge_id), "breach", &format!("{} failed", a.phase_name), &format!("{message}. All positions on #{} were closed and the account is disabled.", a.login.unwrap_or(0))).await;
    tracing::info!(login = a.login, rule, "prop account failed");
    Ok(true)
}

/// Phase passed: close + lock the account (read-only), certificate, and open the next phase / funded account.
pub async fn pass(app: &App, a: &PhaseAccount, actor: &Actor) -> ApiResult<bool> {
    let claimed = sqlx::query("UPDATE phase_accounts SET status = 'passed', ended_at = now(), end_reason = 'passed' WHERE id = $1 AND status = 'active'")
        .bind(a.id)
        .execute(&app.pool)
        .await?
        .rows_affected();
    if claimed == 0 {
        return Ok(false);
    }
    let c = store::challenge(&app.pool, a.challenge_id).await?.ok_or_else(|| ApiError::NotFound("Challenge not found".into()))?;
    store::rule_event(&app.pool, a, "profit_target", "info", a.equity, a.balance, None, &format!("{} passed", a.phase_name), json!({"actor": actor.id}), Some("pass".into())).await;
    let enforced = enforce(app, a, "read_only", R_PASS, &format!("{} passed", a.phase_name)).await;
    refresh_numbers(app, a).await;
    let _ = certs::issue(&app.pool, &c.tenant, c.user_id, c.id, "pass", &format!("{} passed", a.phase_name), &c.trader_name, &c.plan_name, c.size, None, Some(&a.phase_name), &format!("pass:{}:{}", c.id, a.phase_index)).await;
    store::audit(&app.pool, &c.tenant, actor, "phase.passed", "phase_account", &a.id.to_string(), Some(json!({"status": "active"})), Some(json!({"status": "passed", "login": a.login, "enforced": enforced})), None, None).await;
    // next phase
    sqlx::query("UPDATE challenges SET phase_index = $2, status = 'provisioning', updated_at = now() WHERE id = $1").bind(c.id).bind(a.phase_index + 1).execute(&app.pool).await?;
    store::notify(&app.pool, &c.tenant, c.user_id, Some(c.id), "passed", &format!("{} passed", a.phase_name), "Congratulations: all objectives met. Your next account is being opened.").await;
    let c = store::challenge(&app.pool, c.id).await?.unwrap();
    if let Err(e) = provision(app, c).await {
        tracing::warn!(challenge = a.challenge_id, error = ?e, "next phase provisioning deferred to the reconciler");
    }
    tracing::info!(login = a.login, "prop phase passed");
    Ok(true)
}

/* ------------------------------------------------------------------ */
/* Payouts (D149)                                                      */
/* ------------------------------------------------------------------ */

pub async fn payout_json(pool: &PgPool, id: i64) -> ApiResult<Value> {
    let r = sqlx::query(
        "SELECT p.*, c.plan_name, c.size, c.trader_name FROM payouts p JOIN challenges c ON c.id = p.challenge_id WHERE p.id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("Payout not found".into()))?;
    let mut v = payout_row_json(&r);
    let threshold = four_eyes_threshold(pool, &r.get::<String, _>("tenant")).await?;
    with_threshold(&mut v, threshold);
    Ok(v)
}

/* ---------------- four-eyes settings ---------------- */

/// Default `fourEyesThreshold` (USD) for a tenant without a settings row.
pub const DEFAULT_FOUR_EYES_THRESHOLD: i64 = 1000;

/// The tenant's four-eyes threshold: payouts whose USD total (trader share + fee refund) is at or above it need a
/// second approver. 0 = every payout.
pub async fn four_eyes_threshold(pool: &PgPool, tenant: &str) -> ApiResult<D> {
    let t: Option<D> = sqlx::query_scalar("SELECT four_eyes_threshold FROM settings WHERE tenant = $1").bind(tenant).fetch_optional(pool).await?;
    Ok(t.unwrap_or_else(|| D::from(DEFAULT_FOUR_EYES_THRESHOLD)))
}

pub async fn settings_json(pool: &PgPool, tenant: &str) -> ApiResult<Value> {
    let r = sqlx::query("SELECT four_eyes_threshold, updated_at, updated_by FROM settings WHERE tenant = $1").bind(tenant).fetch_optional(pool).await?;
    Ok(match r {
        Some(r) => json!({"fourEyesThreshold": num(r.get("four_eyes_threshold")), "updatedAt": r.get::<chrono::DateTime<Utc>, _>("updated_at"), "updatedBy": r.get::<Option<String>, _>("updated_by")}),
        None => json!({"fourEyesThreshold": DEFAULT_FOUR_EYES_THRESHOLD, "updatedAt": null, "updatedBy": null}),
    })
}

/// Sets the tenant's four-eyes threshold (audited `settings.four_eyes_threshold` with before / after).
pub async fn set_four_eyes_threshold(app: &App, tenant: &str, staff: &Actor, threshold: D, reason: &str) -> ApiResult<Value> {
    if threshold < ZERO {
        return Err(ApiError::Validation { field: "fourEyesThreshold", message: "The threshold can't be negative".into() });
    }
    let threshold = r2(threshold);
    let before = four_eyes_threshold(&app.pool, tenant).await?;
    sqlx::query(
        "INSERT INTO settings (tenant, four_eyes_threshold, updated_at, updated_by) VALUES ($1, $2, now(), $3)
         ON CONFLICT (tenant) DO UPDATE SET four_eyes_threshold = EXCLUDED.four_eyes_threshold, updated_at = now(), updated_by = EXCLUDED.updated_by",
    )
    .bind(tenant)
    .bind(threshold)
    .bind(format!("{} ({})", staff.name, staff.id))
    .execute(&app.pool)
    .await?;
    store::audit(&app.pool, tenant, staff, "settings.four_eyes_threshold", "settings", tenant, Some(json!({"fourEyesThreshold": num(before)})), Some(json!({"fourEyesThreshold": num(threshold)})), Some(reason), None).await;
    settings_json(&app.pool, tenant).await
}

/// Fills `fourEyes.required` / `threshold` of a payout that has no snapshot yet (open requests) from the current
/// tenant threshold, so the Back Office knows whether an approval will be the first of two.
pub fn with_threshold(v: &mut Value, threshold: D) {
    if v["fourEyes"]["threshold"].is_null() {
        let total = crate::money::dec_of(&v["total"]).unwrap_or(ZERO);
        let open = matches!(v["status"].as_str(), Some("pending"));
        v["fourEyes"]["required"] = json!(open && total >= threshold);
        v["fourEyes"]["threshold"] = num(threshold);
    }
}

/// A payout as the trader sees it: the internal four-eyes step is still "pending" (in review), staff details stay
/// in the Back Office.
pub fn client_payout(mut v: Value) -> Value {
    if v["status"] == "awaiting_second_approval" {
        v["status"] = json!("pending");
    }
    if let Some(o) = v.as_object_mut() {
        o.remove("fourEyes");
    }
    v
}

/// Staff id as the BFF sent it (`X-Kalks-Staff-Id`), without the `staff:` actor prefix.
fn raw_staff_id(staff: &Actor) -> &str {
    staff.id.strip_prefix("staff:").unwrap_or(&staff.id)
}

pub fn payout_row_json(r: &sqlx::postgres::PgRow) -> Value {
    let first_at = r.try_get::<Option<chrono::DateTime<Utc>>, _>("first_approved_at").ok().flatten();
    let snapshot = r.try_get::<Option<D>, _>("four_eyes_threshold").ok().flatten();
    let four_eyes = json!({
        // required = this payout went (or goes) through two approvals; set at the first approval
        "required": snapshot.is_some(), "threshold": snapshot.map(num),
        "firstApprovedById": r.try_get::<Option<String>, _>("first_approved_by_id").ok().flatten(),
        "firstApprovedBy": r.try_get::<Option<String>, _>("first_approved_by_name").ok().flatten(),
        "firstApprovedAt": first_at, "firstApprovalNote": r.try_get::<Option<String>, _>("first_approval_note").ok().flatten(),
    });
    json!({
        "fourEyes": four_eyes,
        "id": r.get::<i64, _>("id"), "challengeId": r.get::<i64, _>("challenge_id"), "accountId": r.get::<i64, _>("account_id"),
        "userId": r.get::<i64, _>("user_id"), "login": r.get::<i64, _>("login"),
        "profit": num(r.get("profit")), "split": num(r.get("split_pct")), "traderAmount": num(r.get("trader_amount")),
        "firmAmount": num(r.get("firm_amount")), "feeRefund": num(r.get("fee_refund")),
        "total": num(r.get::<D, _>("trader_amount") + r.get::<D, _>("fee_refund")),
        "status": r.get::<String, _>("status"), "kycStatus": r.get::<Option<String>, _>("kyc_status"),
        "requestedAt": r.get::<chrono::DateTime<Utc>, _>("requested_at"), "decidedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("decided_at"),
        "decidedBy": r.get::<Option<String>, _>("decided_by"), "note": r.get::<Option<String>, _>("note"), "error": r.get::<Option<String>, _>("error"),
        "planName": r.try_get::<String, _>("plan_name").ok(), "size": r.try_get::<D, _>("size").ok().map(num), "traderName": r.try_get::<String, _>("trader_name").ok(),
    })
}

/// Payout eligibility for a funded account right now (used by the dashboard and the request).
pub async fn payout_quote(app: &App, c: &Challenge, a: &PhaseAccount) -> ApiResult<Value> {
    let plan = &c.rules;
    let from = rules::payout_eligible_from(plan, a.started_at.unwrap_or_else(Utc::now), a.last_payout_at);
    let profit = a.balance.unwrap_or(a.initial_balance) - a.initial_balance;
    let (trader, firm) = rules::split(profit.max(ZERO), c.split_pct);
    let paid_before: i64 = sqlx::query_scalar("SELECT count(*) FROM payouts WHERE challenge_id = $1 AND status = 'paid'").bind(c.id).fetch_one(&app.pool).await?;
    let refund = if plan.refund_fee && !c.fee_refunded && paid_before == 0 { c.fee } else { ZERO };
    let pending: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM payouts WHERE account_id = $1 AND status IN ('pending','awaiting_second_approval','approved'))").bind(a.id).fetch_one(&app.pool).await?;
    let consistency_ok = a.stats.pointer("/rules/consistencyOk").and_then(Value::as_bool).unwrap_or(true);
    let mut blockers: Vec<&str> = vec![];
    if Utc::now() < from {
        blockers.push("not_yet_eligible");
    }
    if profit <= ZERO || profit < plan.min_payout {
        blockers.push("below_minimum");
    }
    if a.open_positions > 0 {
        blockers.push("positions_open");
    }
    if pending {
        blockers.push("payout_pending");
    }
    if !consistency_ok {
        blockers.push("consistency");
    }
    Ok(json!({
        "eligibleFrom": from, "profit": num(r2(profit)), "split": num(c.split_pct), "traderAmount": num(trader), "firmAmount": num(firm),
        "feeRefund": num(refund), "total": num(trader + refund), "minPayout": num(plan.min_payout), "blockers": blockers, "eligible": blockers.is_empty(),
    }))
}

pub async fn request_payout(app: &App, tenant: &str, user_id: i64, challenge_id: i64, forwarded_kyc: Option<&str>) -> ApiResult<Value> {
    let c = store::challenge(&app.pool, challenge_id).await?.filter(|c| c.user_id == user_id && c.tenant == tenant).ok_or_else(|| ApiError::NotFound("Challenge not found".into()))?;
    if c.status != "funded" {
        return Err(ApiError::rule("not_funded", "Payouts are available on funded accounts only"));
    }
    let kyc = app.kyc_status(user_id, forwarded_kyc).await;
    if kyc != "verified" {
        return Err(ApiError::rule("kyc_required", "Verify your identity before requesting a payout (Profile → Verification)."));
    }
    let a = store::phases_of(&app.pool, c.id).await?.into_iter().find(|p| p.funded && p.status == "active").ok_or_else(|| ApiError::rule("not_funded", "No active funded account"))?;
    let lock = app.lock(a.id);
    let _g = lock.lock().await;
    let login = a.login.unwrap();
    // fresh numbers from the engine (the dashboard may lag by one poll)
    let snap = app.engine.account(tenant, login).await.map_err(engine_err)?;
    let a = store::phase(&app.pool, a.id).await?.unwrap();
    if a.status != "active" {
        return Err(ApiError::rule("not_funded", "The funded account is no longer active"));
    }
    let mut live = a.clone();
    live.balance = Some(snap.balance);
    live.open_positions = snap.positions.len() as i32;
    let q = payout_quote(app, &c, &live).await?;
    if let Some(b) = q["blockers"].as_array().and_then(|b| b.first()).and_then(Value::as_str) {
        let msg = match b {
            "not_yet_eligible" => format!("Payouts open on {}", q["eligibleFrom"].as_str().unwrap_or("")),
            "below_minimum" => format!("The profit is below the minimum payout ({})", certs::money_text(c.rules.min_payout)),
            "positions_open" => "Close all open positions before requesting a payout".into(),
            "payout_pending" => "A payout request is already in review".into(),
            _ => "The consistency rule isn't met yet: keep trading until your best day is within the limit".into(),
        };
        let code: &'static str = match b {
            "not_yet_eligible" => "not_yet_eligible",
            "below_minimum" => "below_minimum",
            "positions_open" => "positions_open",
            "payout_pending" => "payout_pending",
            _ => "consistency",
        };
        return Err(ApiError::rule(code, msg));
    }
    let profit = r2(snap.balance - a.initial_balance);
    let (trader, firm) = rules::split(profit, c.split_pct);
    let refund = crate::money::dec_of(&q["feeRefund"]).unwrap_or(ZERO);
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO payouts (tenant, challenge_id, account_id, user_id, login, profit, split_pct, trader_amount, firm_amount, fee_refund, kyc_status)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id",
    )
    .bind(tenant)
    .bind(c.id)
    .bind(a.id)
    .bind(user_id)
    .bind(login)
    .bind(profit)
    .bind(c.split_pct)
    .bind(trader)
    .bind(firm)
    .bind(refund)
    .bind(&kyc)
    .fetch_one(&app.pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(d) if d.is_unique_violation() => ApiError::rule("payout_pending", "A payout request is already in review"),
        e => e.into(),
    })?;
    // take the profit off the funded account now, so it can't be traded away while in review
    if let Err(e) = app.engine.adjust(tenant, login, -profit, &format!("prop-payout-{id}"), R_PAYOUT, &format!("Payout request #{id}: profit withdrawn for review")).await {
        sqlx::query("UPDATE payouts SET status = 'failed', error = $2 WHERE id = $1").bind(id).bind(e.to_string()).execute(&app.pool).await?;
        return Err(engine_err(e));
    }
    let mut t = a.tracker();
    t.rebase(-profit);
    store::save_tracker(&app.pool, a.id, &t).await?;
    sqlx::query("UPDATE phase_accounts SET balance = balance - $2, equity = equity - $2 WHERE id = $1").bind(a.id).bind(profit).execute(&app.pool).await?;
    store::audit(&app.pool, tenant, &Actor::user(user_id), "payout.requested", "payout", &id.to_string(), None, Some(json!({"profit": num(profit), "trader": num(trader), "refund": num(refund), "login": login})), None, None).await;
    store::notify(&app.pool, tenant, user_id, Some(c.id), "payout_requested", "Payout requested", &format!("{} is in review. It lands in your wallet once approved.", certs::money_text(trader + refund))).await;
    payout_json(&app.pool, id).await
}

/// Approves (or retries) a payout: wallet credit of the trader share (+ fee refund on the first payout).
///
/// Four-eyes: when the USD total is at or above the tenant threshold, the first approval only moves the request to
/// `awaiting_second_approval`; a different staff member's approval releases the credit. KYC is checked on both.
pub async fn approve_payout(app: &App, tenant: &str, id: i64, staff: &Actor, note: Option<&str>, forwarded_kyc: Option<&str>) -> ApiResult<Value> {
    let r = sqlx::query(
        "SELECT status, user_id, challenge_id, account_id, trader_amount, fee_refund, profit, first_approved_by_id, first_approved_by_name, first_approved_at, first_approval_note, four_eyes_threshold
         FROM payouts WHERE id = $1 AND tenant = $2",
    )
    .bind(id)
    .bind(tenant)
    .fetch_optional(&app.pool)
    .await?
    .ok_or_else(|| ApiError::NotFound("Payout not found".into()))?;
    let status: String = r.get("status");
    let user_id: i64 = r.get("user_id");
    if !["pending", "awaiting_second_approval", "approved", "failed"].contains(&status.as_str()) {
        return Err(ApiError::Conflict { code: "already_decided", message: format!("This payout is already {status}") });
    }
    let me = raw_staff_id(staff);
    if status == "awaiting_second_approval" {
        let first_id: Option<String> = r.get("first_approved_by_id");
        if first_id.as_deref() == Some(me) {
            return Err(ApiError::Conflict { code: "four_eyes_same_staff", message: "Four-eyes rule: another staff member must give the second approval.".into() });
        }
        let kyc = app.kyc_status(user_id, forwarded_kyc).await;
        if kyc != "verified" {
            return Err(ApiError::rule("kyc_required", "The client's identity isn't verified: approve after KYC is complete"));
        }
        // conditional claim: concurrent / repeated second approvals release once
        let n = sqlx::query(
            "UPDATE payouts SET status = 'approved', decided_at = now(), decided_by = $2, note = COALESCE($3, note), kyc_status = $4
             WHERE id = $1 AND status = 'awaiting_second_approval' AND first_approved_by_id IS DISTINCT FROM $5",
        )
        .bind(id)
        .bind(format!("{} ({})", staff.name, staff.id))
        .bind(note)
        .bind(&kyc)
        .bind(me)
        .execute(&app.pool)
        .await?
        .rows_affected();
        if n == 0 {
            return Err(ApiError::Conflict { code: "already_decided", message: "Another reviewer decided this payout".into() });
        }
        let first = json!({
            "id": first_id, "name": r.get::<Option<String>, _>("first_approved_by_name"),
            "at": r.get::<Option<chrono::DateTime<Utc>>, _>("first_approved_at"), "note": r.get::<Option<String>, _>("first_approval_note"),
        });
        let second = json!({"id": me, "name": staff.name, "note": note});
        store::audit(
            &app.pool,
            tenant,
            staff,
            "payout.approved",
            "payout",
            &id.to_string(),
            Some(json!({"status": "awaiting_second_approval"})),
            Some(json!({"status": "approved", "fourEyes": true, "threshold": r.get::<Option<D>, _>("four_eyes_threshold").map(num), "firstApprover": first, "secondApprover": second})),
            None,
            note,
        )
        .await;
    } else if status == "pending" {
        let kyc = app.kyc_status(user_id, forwarded_kyc).await;
        if kyc != "verified" {
            return Err(ApiError::rule("kyc_required", "The client's identity isn't verified: approve after KYC is complete"));
        }
        let total: D = r.get::<D, _>("trader_amount") + r.get::<D, _>("fee_refund");
        let threshold = four_eyes_threshold(&app.pool, tenant).await?;
        if total >= threshold {
            // first of two approvals: record the approver, move nothing
            let n = sqlx::query(
                "UPDATE payouts SET status = 'awaiting_second_approval', first_approved_by_id = $2, first_approved_by_name = $3, first_approved_at = now(),
                        first_approval_note = $4, four_eyes_threshold = $5, kyc_status = $6
                 WHERE id = $1 AND status = 'pending'",
            )
            .bind(id)
            .bind(me)
            .bind(&staff.name)
            .bind(note)
            .bind(threshold)
            .bind(&kyc)
            .execute(&app.pool)
            .await?
            .rows_affected();
            if n == 0 {
                return Err(ApiError::Conflict { code: "already_decided", message: "Another reviewer decided this payout".into() });
            }
            store::audit(
                &app.pool,
                tenant,
                staff,
                "payout.approve_first",
                "payout",
                &id.to_string(),
                Some(json!({"status": "pending"})),
                Some(json!({"status": "awaiting_second_approval", "total": num(total), "threshold": num(threshold), "firstApprover": {"id": me, "name": staff.name}})),
                None,
                note,
            )
            .await;
            return payout_json(&app.pool, id).await;
        }
        let n = sqlx::query("UPDATE payouts SET status = 'approved', decided_at = now(), decided_by = $2, note = COALESCE($3, note), kyc_status = $4 WHERE id = $1 AND status = 'pending'")
            .bind(id)
            .bind(format!("{} ({})", staff.name, staff.id))
            .bind(note)
            .bind(&kyc)
            .execute(&app.pool)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(ApiError::Conflict { code: "already_decided", message: "Another reviewer decided this payout".into() });
        }
        store::audit(&app.pool, tenant, staff, "payout.approved", "payout", &id.to_string(), Some(json!({"status": "pending"})), Some(json!({"status": "approved"})), None, note).await;
    } else if status == "failed" {
        sqlx::query("UPDATE payouts SET status = 'approved', error = NULL WHERE id = $1").bind(id).execute(&app.pool).await?;
    }
    pay(app, tenant, id).await?;
    payout_json(&app.pool, id).await
}

/// Wallet leg of an approved payout (idempotent; the reconciler retries unknown outcomes).
pub async fn pay(app: &App, tenant: &str, id: i64) -> ApiResult<()> {
    let r = sqlx::query("SELECT status, user_id, challenge_id, account_id, trader_amount, fee_refund, profit, login FROM payouts WHERE id = $1").bind(id).fetch_one(&app.pool).await?;
    if r.get::<String, _>("status") != "approved" {
        return Ok(());
    }
    let (user_id, challenge_id, account_id): (i64, i64, i64) = (r.get("user_id"), r.get("challenge_id"), r.get("account_id"));
    let (trader, refund, profit): (D, D, D) = (r.get("trader_amount"), r.get("fee_refund"), r.get("profit"));
    let login: i64 = r.get("login");
    let mut outcomes = vec![app.wallet.transfer(&app.pool, tenant, &format!("prop-payout-{id}"), user_id, "credit", "prop_payout", trader, &format!("prop:payout:{id}"), &format!("Prop payout · account #{login}")).await];
    if refund > ZERO {
        outcomes.push(app.wallet.transfer(&app.pool, tenant, &format!("prop-payout-{id}-refund"), user_id, "credit", "prop_payout", refund, &format!("prop:payout:{id}:refund"), "Challenge fee refund (first payout)").await);
    }
    if let Some(WalletOutcome::Rejected { message, .. }) = outcomes.iter().find(|o| matches!(o, WalletOutcome::Rejected { .. })) {
        sqlx::query("UPDATE payouts SET status = 'failed', error = $2 WHERE id = $1").bind(id).bind(message).execute(&app.pool).await?;
        return Err(ApiError::rule("wallet_rejected", format!("The wallet refused the credit: {message}")));
    }
    if outcomes.iter().any(|o| matches!(o, WalletOutcome::Unknown(_))) {
        return Err(ApiError::Upstream { code: "wallet_pending".into(), message: "The wallet didn't confirm yet; the credit is retried automatically".into() });
    }
    let n = sqlx::query("UPDATE payouts SET status = 'paid', wallet_ref = $2, error = NULL WHERE id = $1 AND status = 'approved'").bind(id).bind(format!("prop-payout-{id}")).execute(&app.pool).await?.rows_affected();
    if n == 0 {
        return Ok(());
    }
    if refund > ZERO {
        sqlx::query("UPDATE challenges SET fee_refunded = true, updated_at = now() WHERE id = $1").bind(challenge_id).execute(&app.pool).await?;
    }
    sqlx::query("UPDATE phase_accounts SET last_payout_at = now() WHERE id = $1").bind(account_id).execute(&app.pool).await?;
    let c = store::challenge(&app.pool, challenge_id).await?.unwrap();
    let _ = certs::issue(&app.pool, tenant, user_id, challenge_id, "payout", "Payout", &c.trader_name, &c.plan_name, c.size, Some(trader), None, &format!("payout:{id}")).await;
    store::audit(&app.pool, tenant, &Actor::system(), "payout.paid", "payout", &id.to_string(), None, Some(json!({"trader": num(trader), "refund": num(refund), "profit": num(profit)})), None, None).await;
    store::notify(&app.pool, tenant, user_id, Some(challenge_id), "payout_paid", "Payout sent to your wallet", &format!("{} was credited to your USDT wallet.", certs::money_text(trader + refund))).await;
    // scaling review after every paid payout
    if let Some(a) = store::phase(&app.pool, account_id).await? {
        let _ = maybe_scale(app, &c, &a, None, &Actor::system(), None).await;
    }
    Ok(())
}

/// Rejects a payout: the withdrawn profit goes back on the funded account.
pub async fn reject_payout(app: &App, tenant: &str, id: i64, staff: &Actor, note: &str) -> ApiResult<Value> {
    let r = sqlx::query("SELECT status, account_id, profit, login, user_id, challenge_id, first_approved_by_id, first_approved_by_name FROM payouts WHERE id = $1 AND tenant = $2")
        .bind(id)
        .bind(tenant)
        .fetch_optional(&app.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("Payout not found".into()))?;
    let status: String = r.get("status");
    if !["pending", "awaiting_second_approval", "failed"].contains(&status.as_str()) {
        return Err(ApiError::Conflict { code: "already_decided", message: format!("This payout is already {status}") });
    }
    let (account_id, profit, login, user_id, challenge_id): (i64, D, i64, i64, i64) = (r.get("account_id"), r.get("profit"), r.get("login"), r.get("user_id"), r.get("challenge_id"));
    let lock = app.lock(account_id);
    let _g = lock.lock().await;
    let n = sqlx::query("UPDATE payouts SET status = 'rejected', decided_at = now(), decided_by = $2, note = $3 WHERE id = $1 AND status IN ('pending','awaiting_second_approval','failed')")
        .bind(id)
        .bind(format!("{} ({})", staff.name, staff.id))
        .bind(note)
        .execute(&app.pool)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(ApiError::Conflict { code: "already_decided", message: "Another reviewer decided this payout".into() });
    }
    app.engine.adjust(tenant, login, profit, &format!("prop-payout-{id}-reversal"), R_PAYOUT, &format!("Payout #{id} rejected: profit returned")).await.map_err(engine_err)?;
    if let Some(a) = store::phase(&app.pool, account_id).await? {
        let mut t = a.tracker();
        t.rebase(profit);
        store::save_tracker(&app.pool, a.id, &t).await?;
    }
    store::audit(&app.pool, tenant, staff, "payout.rejected", "payout", &id.to_string(), Some(json!({"status": status, "firstApprover": r.get::<Option<String>, _>("first_approved_by_id").map(|id| json!({"id": id, "name": r.get::<Option<String>, _>("first_approved_by_name")}))})), Some(json!({"status": "rejected", "rejectedFrom": status, "returned": num(profit)})), None, Some(note)).await;
    store::notify(&app.pool, tenant, user_id, Some(challenge_id), "payout_rejected", "Payout not approved", &format!("{note}. The profit was returned to account #{login}.")).await;
    payout_json(&app.pool, id).await
}

/* ------------------------------------------------------------------ */
/* Scaling plan                                                        */
/* ------------------------------------------------------------------ */

/// Applies the scaling plan when due (or `force_to` for a manual scale-up by staff).
pub async fn maybe_scale(app: &App, c: &Challenge, a: &PhaseAccount, force_to: Option<D>, actor: &Actor, reason: Option<&str>) -> ApiResult<Option<D>> {
    if !a.funded || a.status != "active" {
        return Ok(None);
    }
    let since = a.scaled_at.or(a.started_at).unwrap_or_else(Utc::now);
    let to = match force_to {
        Some(t) => Some(t),
        None => {
            let profit: D = sqlx::query_scalar("SELECT COALESCE(sum(profit), 0) FROM payouts WHERE account_id = $1 AND status = 'paid' AND requested_at >= $2")
                .bind(a.id)
                .bind(since)
                .fetch_one(&app.pool)
                .await?;
            rules::scaling_due(&c.rules, a.initial_balance, since, Utc::now(), profit)
        }
    };
    let Some(to) = to.filter(|t| *t > a.initial_balance) else { return Ok(None) };
    let lock = app.lock(a.id);
    let _g = lock.lock().await;
    let delta = to - a.initial_balance;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM scaling_events WHERE account_id = $1").bind(a.id).fetch_one(&app.pool).await?;
    app.engine
        .adjust(&a.tenant, a.login.unwrap(), delta, &format!("prop-scale-{}-{}", a.id, n + 1), R_FUND, &format!("Scaling plan: {} → {}", certs::money_text(a.initial_balance), certs::money_text(to)))
        .await
        .map_err(engine_err)?;
    let a2 = store::phase(&app.pool, a.id).await?.unwrap();
    let mut t = a2.tracker();
    t.rebase(delta);
    store::save_tracker(&app.pool, a.id, &t).await?;
    sqlx::query("UPDATE phase_accounts SET initial_balance = $2, scaled_at = now() WHERE id = $1").bind(a.id).bind(to).execute(&app.pool).await?;
    let split_to = c.rules.split_max;
    sqlx::query("UPDATE challenges SET split_pct = $2, updated_at = now() WHERE id = $1").bind(c.id).bind(split_to).execute(&app.pool).await?;
    sqlx::query("INSERT INTO scaling_events (tenant, account_id, from_size, to_size, split_from, split_to, actor) VALUES ($1,$2,$3,$4,$5,$6,$7)")
        .bind(&a.tenant)
        .bind(a.id)
        .bind(a.initial_balance)
        .bind(to)
        .bind(c.split_pct)
        .bind(split_to)
        .bind(&actor.id)
        .execute(&app.pool)
        .await?;
    store::audit(&app.pool, &a.tenant, actor, "account.scaled", "phase_account", &a.id.to_string(), Some(json!({"size": num(a.initial_balance), "split": num(c.split_pct)})), Some(json!({"size": num(to), "split": num(split_to)})), reason, None).await;
    store::notify(&app.pool, &a.tenant, a.user_id, Some(c.id), "scaled", "Your account was scaled up", &format!("Account #{} grew to {} and your split is now {}%.", a.login.unwrap_or(0), certs::money_text(to), split_to.normalize())).await;
    Ok(Some(to))
}

/* ------------------------------------------------------------------ */
/* Staff actions                                                       */
/* ------------------------------------------------------------------ */

/// Manual pass / fail of the challenge's active phase (audited, reason required).
pub async fn override_challenge(app: &App, tenant: &str, challenge_id: i64, action: &str, staff: &Actor, reason: &str, note: &str) -> ApiResult<Value> {
    let c = store::challenge(&app.pool, challenge_id).await?.filter(|c| c.tenant == tenant).ok_or_else(|| ApiError::NotFound("Challenge not found".into()))?;
    let a = store::phases_of(&app.pool, c.id).await?.into_iter().find(|p| p.status == "active").ok_or_else(|| ApiError::rule("not_active", "The challenge has no active account"))?;
    let lock = app.lock(a.id);
    let _g = lock.lock().await;
    let a = store::phase(&app.pool, a.id).await?.unwrap();
    let msg = format!("Manual {action} by {}: {reason}{}", staff.name, if note.is_empty() { String::new() } else { format!(" ({note})") });
    let done = match action {
        "pass" => {
            if a.funded {
                return Err(ApiError::rule("funded", "A funded account has no next phase"));
            }
            store::audit(&app.pool, tenant, staff, "override.pass", "challenge", &c.id.to_string(), Some(json!({"phase": a.phase_name})), None, Some(reason), Some(note)).await;
            pass(app, &a, staff).await?
        }
        "fail" => {
            store::audit(&app.pool, tenant, staff, "override.fail", "challenge", &c.id.to_string(), Some(json!({"phase": a.phase_name})), None, Some(reason), Some(note)).await;
            fail(app, &a, "manual", &msg, staff, a.equity, None).await?
        }
        _ => return Err(ApiError::Validation { field: "action", message: "action must be pass or fail".into() }),
    };
    if !done {
        return Err(ApiError::Conflict { code: "not_active", message: "The account changed state meanwhile".into() });
    }
    let c = store::challenge(&app.pool, c.id).await?.unwrap();
    Ok(store::challenge_json(&c, &store::phases_of(&app.pool, c.id).await?))
}

/// Review of a banned-strategy flag: clear, or confirm (optionally failing the account).
pub async fn review_flag(app: &App, tenant: &str, id: i64, decision: &str, fail_account: bool, staff: &Actor, note: &str) -> ApiResult<Value> {
    if decision != "clear" && decision != "confirm" {
        return Err(ApiError::Validation { field: "decision", message: "decision must be clear or confirm".into() });
    }
    let r = sqlx::query("SELECT account_id, kind, status, summary FROM strategy_flags WHERE id = $1 AND tenant = $2")
        .bind(id)
        .bind(tenant)
        .fetch_optional(&app.pool)
        .await?
        .ok_or_else(|| ApiError::NotFound("Flag not found".into()))?;
    let status = if decision == "clear" { "cleared" } else { "confirmed" };
    sqlx::query("UPDATE strategy_flags SET status = $2, reviewed_by = $3, reviewed_at = now(), review_note = $4, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(format!("{} ({})", staff.name, staff.id))
        .bind(note)
        .execute(&app.pool)
        .await?;
    store::audit(&app.pool, tenant, staff, &format!("flag.{status}"), "strategy_flag", &id.to_string(), Some(json!({"status": r.get::<String, _>("status")})), Some(json!({"status": status, "failAccount": fail_account})), None, Some(note)).await;
    if decision == "confirm" && fail_account {
        let account_id: i64 = r.get("account_id");
        let lock = app.lock(account_id);
        let _g = lock.lock().await;
        if let Some(a) = store::phase(&app.pool, account_id).await? {
            let kind: String = r.get("kind");
            let summary: String = r.get("summary");
            fail(app, &a, "banned_strategy", &format!("Banned strategy confirmed ({}): {summary}", kind.replace('_', " ")), staff, a.equity, None).await?;
        }
    }
    let row = sqlx::query("SELECT * FROM strategy_flags WHERE id = $1").bind(id).fetch_one(&app.pool).await?;
    Ok(flag_json(&row))
}

pub fn flag_json(r: &sqlx::postgres::PgRow) -> Value {
    let ev: sqlx::types::Json<Value> = r.get("evidence");
    let related: i64 = r.get("related_login");
    json!({
        "id": r.get::<i64, _>("id"), "accountId": r.get::<i64, _>("account_id"), "challengeId": r.get::<i64, _>("challenge_id"),
        "userId": r.get::<i64, _>("user_id"), "login": r.get::<Option<i64>, _>("login"), "kind": r.get::<String, _>("kind"),
        "score": num(r.get("score")), "summary": r.get::<String, _>("summary"), "evidence": ev.0,
        "relatedLogin": (related != 0).then_some(related), "status": r.get::<String, _>("status"),
        "createdAt": r.get::<chrono::DateTime<Utc>, _>("created_at"), "updatedAt": r.get::<chrono::DateTime<Utc>, _>("updated_at"),
        "reviewedBy": r.get::<Option<String>, _>("reviewed_by"), "reviewedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("reviewed_at"),
        "reviewNote": r.get::<Option<String>, _>("review_note"),
    })
}

/* ------------------------------------------------------------------ */
/* Reconciler                                                          */
/* ------------------------------------------------------------------ */

/// Retries everything left half-way by a crash or an unavailable upstream: unconfirmed wallet charges,
/// unopened accounts, unenforced fails/passes and unconfirmed payout credits.
pub async fn reconcile(app: &App) {
    let pending: Vec<i64> = sqlx::query_scalar("SELECT id FROM challenges WHERE status IN ('pending_payment','provisioning') AND updated_at < now() - interval '15 seconds' ORDER BY id LIMIT 50")
        .fetch_all(&app.pool)
        .await
        .unwrap_or_default();
    for id in pending {
        if let Ok(Some(c)) = store::challenge(&app.pool, id).await {
            let _ = sqlx::query("UPDATE challenges SET updated_at = now() WHERE id = $1").bind(id).execute(&app.pool).await;
            if let Err(e) = continue_purchase(app, c).await {
                tracing::debug!(challenge = id, error = ?e, "reconcile purchase");
            }
        }
    }
    let unenforced: Vec<i64> = sqlx::query_scalar(
        "SELECT id FROM phase_accounts WHERE status IN ('failed','passed') AND login IS NOT NULL AND (stats->>'enforced') IS DISTINCT FROM 'true'
           AND ended_at > now() - interval '7 days' AND ended_at < now() - interval '10 seconds' LIMIT 50",
    )
    .fetch_all(&app.pool)
    .await
    .unwrap_or_default();
    for id in unenforced {
        if let Ok(Some(a)) = store::phase(&app.pool, id).await {
            let (st, reason) = if a.status == "failed" { ("disabled", R_BREACH) } else { ("read_only", R_PASS) };
            enforce(app, &a, st, reason, "Prop enforcement retry").await;
        }
    }
    let approved: Vec<(i64, String)> = sqlx::query_as("SELECT id, tenant FROM payouts WHERE status = 'approved' AND decided_at < now() - interval '15 seconds' LIMIT 50").fetch_all(&app.pool).await.unwrap_or_default();
    for (id, tenant) in approved {
        let _ = pay(app, &tenant, id).await;
    }
}

//! Missions (phase 4): daily, weekly and one-off goals that earn XP and, optionally, cash to the wallet or trading
//! credit. Progress is measured from what growth already knows, per client and period:
//!
//! | kind | progress | source |
//! |---|---|---|
//! | `kyc` | 1 once KYC is verified | gateway users feed / BFF segment header (`profiles.kyc_status`) |
//! | `first_deposit` | 1 once the first deposit is credited | reports `GET /v1/internal/client-facts` (`profiles.first_deposit_at`) |
//! | `deposit` | largest single deposit in the period (USD) | engine ledger of the client's live accounts (`transfer_in` / `deposit`), checked at most every 2 min |
//! | `lots` | lots closed in the period | `deals` (engine dealing feed) |
//! | `trading_days` | days (server time) with a closed trade | `deals` |
//! | `hold` | longest hold of a closed trade, hours | `deals` |
//! | `join_contest` | contests joined in the period | `contest_entries` |
//! | `referral` | referred friends whose first deposit landed in the period | gateway feed `referred_by` + client facts |
//!
//! Only qualifying trades count: live accounts, CFDs (never options), not reversed or price corrections, at least
//! `missionMinLots` lots and `missionMinHoldSeconds` held (per mission `minLots` / `minHoldSeconds` override,
//! `assetClasses` filter). A source that is down leaves the progress where it was (never guessed).
//!
//! A mission completes once per period: `mission_progress` is unique per (mission, client, period) and completion
//! is decided under a row lock, the XP booked with ref `mission:<id>:<period>`. A cash reward below the broker's
//! `autoPayBelow` is paid at once (wallet `adjustment` with key `growth:mission:<progress>`, or engine credit), above
//! it waits for staff approval; a reward that needs KYC waits until the client is verified.

use crate::audit::{self, Actor};
use crate::bonus::REASON_PRIZE;
use crate::challenge_calc as cc;
use crate::clients::{self, EngineOutcome};
use crate::db;
use crate::error::{ApiError, ApiResult};
use crate::loyalty;
use crate::model::opt_num;
use crate::money::{D, ZERO, num, r2};
use crate::state::AppState;
use crate::xp;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

pub const KINDS: &[&str] = &["kyc", "first_deposit", "deposit", "lots", "trading_days", "hold", "join_contest", "referral"];
/// Kinds that are a state, not an activity: one-off only.
pub const ONCE_ONLY: &[&str] = &["kyc", "first_deposit"];

#[derive(Clone, Debug)]
pub struct Mission {
    pub id: i64,
    pub tenant: String,
    pub name: String,
    pub kind: String,
    pub schedule: String,
    pub target: D,
    pub params: Value,
    pub xp: i64,
    pub reward_amount: Option<D>,
    pub reward_payout: Option<String>,
    pub kyc_required: bool,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
}

pub fn mission_of(r: &sqlx::postgres::PgRow) -> Mission {
    Mission {
        id: r.get("id"),
        tenant: r.get("tenant"),
        name: r.get("name"),
        kind: r.get("kind"),
        schedule: r.get("schedule"),
        target: r.get("target"),
        params: r.get::<sqlx::types::Json<Value>, _>("params").0,
        xp: r.get::<i32, _>("xp") as i64,
        reward_amount: r.get("reward_amount"),
        reward_payout: r.get("reward_payout"),
        kyc_required: r.get("kyc_required"),
        starts_at: r.get("starts_at"),
        ends_at: r.get("ends_at"),
    }
}

pub fn mission_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "name": r.get::<String, _>("name"),
        "description": r.get::<String, _>("description"),
        "kind": r.get::<String, _>("kind"),
        "schedule": r.get::<String, _>("schedule"),
        "target": num(r.get("target")),
        "params": r.get::<sqlx::types::Json<Value>, _>("params").0,
        "xp": r.get::<i32, _>("xp"),
        "rewardAmount": opt_num(r.get("reward_amount")),
        "rewardPayout": r.get::<Option<String>, _>("reward_payout"),
        "kycRequired": r.get::<bool, _>("kyc_required"),
        "active": r.get::<bool, _>("active"),
        "startsAt": r.get::<DateTime<Utc>, _>("starts_at"),
        "endsAt": r.get::<Option<DateTime<Utc>>, _>("ends_at"),
        "sort": r.get::<i32, _>("sort"),
        "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

pub fn progress_json(r: &sqlx::postgres::PgRow) -> Value {
    json!({
        "id": r.get::<i64, _>("id"),
        "missionId": r.get::<i64, _>("mission_id"),
        "period": r.get::<String, _>("period"),
        "progress": num(r.get("progress")),
        "target": num(r.get("target")),
        "completedAt": r.get::<Option<DateTime<Utc>>, _>("completed_at"),
        "xp": r.get::<i32, _>("xp"),
        "rewardAmount": opt_num(r.get("reward_amount")),
        "rewardPayout": r.get::<Option<String>, _>("reward_payout"),
        "rewardStatus": r.get::<String, _>("reward_status"),
        "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

/// Default missions seeded once per broker (XP only: a cash or credit reward is always the broker's choice):
/// (name, description, kind, schedule, target, xp, reward, payout, sort).
#[allow(clippy::type_complexity)]
const DEFAULTS: &[(&str, &str, &str, &str, &str, i32, Option<&str>, Option<&str>, i32)] = &[
    ("Verify your identity", "Complete KYC to unlock live contests and cash rewards.", "kyc", "once", "1", 100, None, None, 10),
    ("Make your first deposit", "Fund your wallet for the first time.", "first_deposit", "once", "1", 150, None, None, 20),
    ("Daily trader", "Close 0.5 lots today on a live account.", "lots", "daily", "0.5", 20, None, None, 30),
    ("Five-day week", "Trade on 5 different days this week.", "trading_days", "weekly", "5", 100, None, None, 40),
    ("Weekly volume", "Close 5 lots this week on a live account.", "lots", "weekly", "5", 150, None, None, 50),
    ("Join a contest", "Enter any live contest this week.", "join_contest", "weekly", "1", 50, None, None, 60),
];

pub async fn seed(pool: &sqlx::PgPool, tenant: &str) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("growth-missions:{tenant}")).execute(&mut *tx).await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM missions WHERE tenant = $1").bind(tenant).fetch_one(&mut *tx).await?;
    if n == 0 {
        for (name, desc, kind, schedule, target, xp, reward, payout, sort) in DEFAULTS {
            sqlx::query("INSERT INTO missions (tenant, name, description, kind, schedule, target, xp, reward_amount, reward_payout, sort, created_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'system')")
                .bind(tenant)
                .bind(name)
                .bind(desc)
                .bind(kind)
                .bind(schedule)
                .bind(crate::money::dec(target))
                .bind(xp)
                .bind(reward.map(crate::money::dec))
                .bind(payout)
                .bind(sort)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Missions live now for a broker.
pub async fn active(pool: &sqlx::PgPool, tenant: &str) -> anyhow::Result<Vec<sqlx::postgres::PgRow>> {
    Ok(sqlx::query("SELECT * FROM missions WHERE tenant = $1 AND active AND starts_at <= now() AND (ends_at IS NULL OR ends_at > now()) ORDER BY sort, id").bind(tenant).fetch_all(pool).await?)
}

/// The window a mission measures in for the period containing `now`.
pub fn window(m: &Mission, now: DateTime<Utc>) -> (String, DateTime<Utc>, DateTime<Utc>) {
    let (key, s, e) = cc::period(&m.schedule, now);
    let s = s.max(m.starts_at);
    let e = match m.ends_at {
        Some(x) => e.min(x),
        None => e,
    };
    (key, s, e)
}

/// Trade filters: (min lots, min hold seconds, asset classes).
fn trade_rules(m: &Mission, s: &crate::model::Settings) -> (D, i64, Vec<String>) {
    let min_lots = m.params["minLots"].as_f64().and_then(crate::money::from_f64).unwrap_or(s.mission_min_lots);
    let min_hold = m.params["minHoldSeconds"].as_i64().unwrap_or(s.mission_min_hold_seconds);
    let classes: Vec<String> = m.params["assetClasses"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_lowercase)).collect()).unwrap_or_default();
    (min_lots, min_hold, classes)
}

const QUALIFYING: &str = "tenant = $1 AND user_id = $2 AND account_kind = 'live' AND instrument = 'cfd' AND NOT reversed AND deal_kind <> 'price-correction'
    AND close_time >= $3 AND close_time < $4 AND lots >= $5 AND extract(epoch FROM close_time - open_time) >= $6
    AND (cardinality($7::text[]) = 0 OR lower(asset_class) = ANY($7))";

/// Engine deposit checks are throttled per client (2 min).
static DEPOSIT_CHECKS: LazyLock<Mutex<HashMap<(String, i64), (Instant, D)>>> = LazyLock::new(Default::default);

/// Largest single deposit (USD) on the client's live accounts in [from, to). None = engine unreachable.
async fn largest_deposit(st: &AppState, tenant: &str, user_id: i64, from: DateTime<Utc>, to: DateTime<Utc>) -> Option<D> {
    let key = (format!("{tenant}:{from}"), user_id);
    if let Some((at, v)) = DEPOSIT_CHECKS.lock().unwrap().get(&key).cloned()
        && at.elapsed().as_secs() < 120
    {
        return Some(v);
    }
    let accounts = clients::accounts_of(st, tenant, user_id, Some("live")).await.ok()?;
    let mut best = ZERO;
    for a in accounts.iter().filter(|a| a.kind == "live") {
        let ledger = clients::ledger(st, tenant, a.login, user_id, from).await.ok()?;
        for l in ledger.iter().filter(|l| crate::calc::is_deposit_kind(&l.kind) && l.sub_ledger == "balance" && l.at >= from && l.at < to && !l.reason_code.as_deref().is_some_and(|r| r.starts_with("GRW-"))) {
            best = best.max(l.amount);
        }
    }
    let mut c = DEPOSIT_CHECKS.lock().unwrap();
    if c.len() > 20_000 {
        c.clear();
    }
    c.insert(key, (Instant::now(), best));
    Some(best)
}

/// Current progress of a mission for a client. None = the source can't answer now (keep what we had).
pub async fn measure(st: &AppState, m: &Mission, user_id: i64, from: DateTime<Utc>, to: DateTime<Utc>, s: &crate::model::Settings) -> anyhow::Result<Option<D>> {
    let tenant = m.tenant.as_str();
    let (min_lots, min_hold, classes) = trade_rules(m, s);
    let one = |b: bool| Some(if b { D::ONE } else { ZERO });
    Ok(match m.kind.as_str() {
        "kyc" => one(sqlx::query_scalar::<_, String>("SELECT kyc_status FROM profiles WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_optional(&st.pool).await?.as_deref() == Some("verified")),
        "first_deposit" => {
            let at: Option<Option<DateTime<Utc>>> = sqlx::query_scalar("SELECT first_deposit_at FROM profiles WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_optional(&st.pool).await?;
            one(at.flatten().is_some())
        }
        "deposit" => largest_deposit(st, tenant, user_id, from, to).await,
        "lots" => {
            let v: Option<D> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT sum(lots) FROM deals WHERE {QUALIFYING}")))
                .bind(tenant)
                .bind(user_id)
                .bind(from)
                .bind(to)
                .bind(min_lots)
                .bind(min_hold as f64)
                .bind(&classes)
                .fetch_one(&st.pool)
                .await?;
            Some(v.unwrap_or(ZERO))
        }
        "trading_days" => {
            let v: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(DISTINCT (close_time + interval '3 hours')::date) FROM deals WHERE {QUALIFYING}")))
                .bind(tenant)
                .bind(user_id)
                .bind(from)
                .bind(to)
                .bind(min_lots)
                .bind(min_hold as f64)
                .bind(&classes)
                .fetch_one(&st.pool)
                .await?;
            Some(D::from(v))
        }
        "hold" => {
            let v: Option<f64> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT max(extract(epoch FROM close_time - open_time))::float8 FROM deals WHERE {QUALIFYING}")))
                .bind(tenant)
                .bind(user_id)
                .bind(from)
                .bind(to)
                .bind(min_lots)
                .bind(min_hold as f64)
                .bind(&classes)
                .fetch_one(&st.pool)
                .await?;
            Some(r2(crate::money::from_f64(v.unwrap_or(0.0) / 3600.0).unwrap_or(ZERO)))
        }
        "join_contest" => {
            let v: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM contest_entries e JOIN contests c ON c.id = e.contest_id WHERE e.tenant = $1 AND e.user_id = $2 AND e.joined_at >= $3 AND e.joined_at < $4 AND c.kind = 'live'",
            )
            .bind(tenant)
            .bind(user_id)
            .bind(from)
            .bind(to)
            .fetch_one(&st.pool)
            .await?;
            Some(D::from(v))
        }
        "referral" => {
            let v: i64 = sqlx::query_scalar("SELECT count(*) FROM profiles WHERE tenant = $1 AND referred_by = $2 AND first_deposit_at >= $3 AND first_deposit_at < $4")
                .bind(tenant)
                .bind(user_id)
                .bind(from)
                .bind(to)
                .fetch_one(&st.pool)
                .await?;
            Some(D::from(v))
        }
        _ => None,
    })
}

/// Evaluates every active mission of a broker for one client: progress rows for the current period, completion
/// (XP once per period) and rewards. Returns the missions completed by this call.
pub async fn evaluate_user(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<usize> {
    let rows = active(&st.pool, tenant).await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let settings = db::settings(&st.pool, tenant).await?;
    let now = Utc::now();
    let mut done = 0;
    for r in &rows {
        let m = mission_of(r);
        let (period, from, to) = window(&m, now);
        if from >= to {
            continue;
        }
        // completed this period already: nothing to measure
        let fin: Option<Option<DateTime<Utc>>> = sqlx::query_scalar("SELECT completed_at FROM mission_progress WHERE mission_id = $1 AND user_id = $2 AND period = $3").bind(m.id).bind(user_id).bind(&period).fetch_optional(&st.pool).await?;
        if fin.flatten().is_some() {
            continue;
        }
        let Some(progress) = measure(st, &m, user_id, from, to, &settings).await? else { continue };
        if fin.is_none() && progress <= ZERO {
            continue; // no row for missions the client hasn't started
        }
        if record(st, &m, user_id, &period, progress, &settings).await? {
            done += 1;
        }
    }
    Ok(done)
}

/// Stores progress and completes the mission when it reached its target. Returns true when it completed now.
pub async fn record(st: &AppState, m: &Mission, user_id: i64, period: &str, progress: D, settings: &crate::model::Settings) -> anyhow::Result<bool> {
    let tenant = m.tenant.as_str();
    let xp_before = xp::total(&st.pool, tenant, user_id).await?;
    let mut tx = st.pool.begin().await?;
    sqlx::query("INSERT INTO mission_progress (tenant, mission_id, user_id, period, target) VALUES ($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING").bind(tenant).bind(m.id).bind(user_id).bind(period).bind(m.target).execute(&mut *tx).await?;
    let p = sqlx::query("SELECT id, completed_at, progress FROM mission_progress WHERE mission_id = $1 AND user_id = $2 AND period = $3 FOR UPDATE").bind(m.id).bind(user_id).bind(period).fetch_one(&mut *tx).await?;
    let pid: i64 = p.get("id");
    if p.get::<Option<DateTime<Utc>>, _>("completed_at").is_some() {
        tx.commit().await?;
        return Ok(false);
    }
    let complete = progress >= m.target;
    let shown = progress; // the real value (e.g. 7 lots of a 5-lot target)
    if !complete {
        if p.get::<D, _>("progress") != shown {
            sqlx::query("UPDATE mission_progress SET progress = $2, target = $3, updated_at = now() WHERE id = $1").bind(pid).bind(shown).bind(m.target).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        return Ok(false);
    }
    // completed: XP once per period, then the reward decision
    let fresh_xp = xp::award_in(&mut tx, tenant, user_id, m.xp, "mission", &format!("mission:{}:{period}", m.id), &format!("Mission: {}", m.name)).await?;
    let status = match m.reward_amount {
        None => "none",
        Some(_) if m.kyc_required && !kyc_verified(&mut tx, tenant, user_id).await? => "kyc",
        Some(a) => decide(a, settings.auto_pay_below),
    };
    sqlx::query(
        "UPDATE mission_progress SET progress = $2, target = $3, completed_at = now(), xp = $4, reward_amount = $5, reward_payout = $6, reward_status = $7, updated_at = now() WHERE id = $1",
    )
    .bind(pid)
    .bind(shown)
    .bind(m.target)
    .bind(if fresh_xp { m.xp as i32 } else { 0 })
    .bind(m.reward_amount)
    .bind(m.reward_amount.map(|_| m.reward_payout.clone().unwrap_or_else(|| "wallet".into())))
    .bind(status)
    .execute(&mut *tx)
    .await?;
    audit::record(&mut *tx, tenant, &Actor::system(), "mission.complete", Some(format!("mission_progress:{pid}")), None, Some(json!({"mission": m.id, "user": user_id, "period": period, "xp": m.xp, "reward": opt_num(m.reward_amount), "rewardStatus": status})), None).await?;
    tx.commit().await?;
    let reward_text = m.reward_amount.map(|a| format!(" and ${} {}", a.normalize(), if m.reward_payout.as_deref() == Some("credit") { "trading credit" } else { "cash" })).unwrap_or_default();
    clients::notify_dedupe(st, tenant, user_id, "rewards.mission", format!("Mission complete: {}", m.name), format!("+{} XP{reward_text}.", m.xp), "/rewards", Some(format!("mission:{pid}")));
    if fresh_xp {
        xp::level_up_check(st, tenant, user_id, xp_before, xp_before + m.xp).await;
    }
    if status == "pending" {
        pay_reward(st, pid, &Actor::system()).await?;
    }
    Ok(true)
}

/// Below the broker's threshold: paid at once (`pending`); otherwise staff approve it. 0 = always staff.
pub fn decide(amount: D, auto_pay_below: D) -> &'static str {
    if auto_pay_below > ZERO && amount < auto_pay_below { "pending" } else { "approval" }
}

async fn kyc_verified(tx: &mut sqlx::PgConnection, tenant: &str, user_id: i64) -> anyhow::Result<bool> {
    let k: Option<String> = sqlx::query_scalar("SELECT kyc_status FROM profiles WHERE tenant = $1 AND user_id = $2").bind(tenant).bind(user_id).fetch_optional(&mut *tx).await?;
    Ok(k.as_deref() == Some("verified"))
}

/// Pays a mission reward (`pending` or staff-approved): wallet credit (queued, idempotent key) or engine credit on
/// the client's first live account. Safe to call again.
pub async fn pay_reward(st: &AppState, progress_id: i64, actor: &Actor) -> anyhow::Result<&'static str> {
    let r = sqlx::query("SELECT p.*, m.name FROM mission_progress p JOIN missions m ON m.id = p.mission_id WHERE p.id = $1").bind(progress_id).fetch_one(&st.pool).await?;
    let status: String = r.get("reward_status");
    if !matches!(status.as_str(), "pending" | "approval" | "failed") {
        return Ok("skipped");
    }
    let tenant: String = r.get("tenant");
    let user: i64 = r.get("user_id");
    let Some(amount) = r.get::<Option<D>, _>("reward_amount") else { return Ok("skipped") };
    let name: String = r.get("name");
    let note = format!("Mission reward: {name}");
    let out = if r.get::<Option<String>, _>("reward_payout").as_deref() == Some("credit") {
        let login = clients::accounts_of(st, &tenant, user, Some("live")).await.ok().and_then(|v| v.iter().filter(|a| a.kind == "live").map(|a| (a.login, a.cent)).next());
        match login {
            None => {
                sqlx::query("UPDATE mission_progress SET reward_status = 'failed', reward_error = 'The client has no live account for a credit reward', updated_at = now() WHERE id = $1").bind(progress_id).execute(&st.pool).await?;
                "failed"
            }
            Some((login, cent)) => match clients::post_balance(st, &tenant, login, cent, "credit", amount, &format!("growth:mission:{progress_id}:c"), REASON_PRIZE, &note).await {
                EngineOutcome::Booked => {
                    sqlx::query("UPDATE mission_progress SET reward_status = 'paid', reward_ref = $2, reward_error = NULL, updated_at = now() WHERE id = $1").bind(progress_id).bind(format!("credit:{login}")).execute(&st.pool).await?;
                    clients::notify(st, &tenant, user, "rewards.paid", format!("{name}: reward credited"), format!("${} trading credit on account {login}.", amount.normalize()), "/rewards");
                    "paid"
                }
                EngineOutcome::Retry(m) | EngineOutcome::Fail(m) => {
                    sqlx::query("UPDATE mission_progress SET reward_status = 'failed', reward_error = $2, updated_at = now() WHERE id = $1").bind(progress_id).bind(&m).execute(&st.pool).await?;
                    "failed"
                }
            },
        }
    } else {
        let mut tx = st.pool.begin().await?;
        let key = format!("growth:mission:{progress_id}");
        loyalty::queue_wallet_credit(&mut tx, &tenant, user, &key, "adjustment", amount, &format!("mission:{progress_id}"), &note).await?;
        sqlx::query("UPDATE wallet_credits SET status = 'pending', next_try_at = now() WHERE idem_key = $1 AND status = 'failed'").bind(&key).execute(&mut *tx).await?;
        sqlx::query("UPDATE mission_progress SET reward_status = 'pending', reward_error = NULL, updated_at = now() WHERE id = $1").bind(progress_id).execute(&mut *tx).await?;
        tx.commit().await?;
        st.wake.notify_one();
        "pending"
    };
    audit::record(&st.pool, &tenant, actor, "mission.reward_pay", Some(format!("mission_progress:{progress_id}")), None, Some(json!({"amount": num(amount), "result": out})), None).await?;
    Ok(out)
}

/// Staff decision on a reward waiting for approval.
pub async fn decide_reward(st: &AppState, tenant: &str, progress_id: i64, approve: bool, actor: &Actor, note: &str) -> ApiResult<Value> {
    let r = sqlx::query("SELECT reward_status FROM mission_progress WHERE id = $1 AND tenant = $2").bind(progress_id).bind(tenant).fetch_optional(&st.pool).await?.ok_or(ApiError::NotFound)?;
    let status: String = r.get("reward_status");
    if !matches!(status.as_str(), "approval" | "failed") {
        return Err(ApiError::Conflict { code: "state", message: "This reward isn't waiting for a decision.".into() });
    }
    sqlx::query("UPDATE mission_progress SET decided_by = $2 WHERE id = $1").bind(progress_id).bind(actor.label()).execute(&st.pool).await?;
    if approve {
        pay_reward(st, progress_id, actor).await?;
        let _ = crate::payouts::wallet_tick(st).await;
    } else {
        sqlx::query("UPDATE mission_progress SET reward_status = 'rejected', reward_error = $2, updated_at = now() WHERE id = $1").bind(progress_id).bind(note).execute(&st.pool).await?;
        audit::record(&st.pool, tenant, actor, "mission.reward_reject", Some(format!("mission_progress:{progress_id}")), None, None, Some(note)).await?;
    }
    let r = sqlx::query("SELECT * FROM mission_progress WHERE id = $1").bind(progress_id).fetch_one(&st.pool).await?;
    Ok(json!({"reward": progress_json(&r)}))
}

/// The client's missions for the current periods (what the Rewards hub shows).
pub async fn me(st: &AppState, tenant: &str, user_id: i64) -> anyhow::Result<Vec<Value>> {
    let rows = active(&st.pool, tenant).await?;
    let now = Utc::now();
    let mut out = vec![];
    for r in &rows {
        let m = mission_of(r);
        let (period, _, to) = window(&m, now);
        let p = sqlx::query("SELECT * FROM mission_progress WHERE mission_id = $1 AND user_id = $2 AND period = $3").bind(m.id).bind(user_id).bind(&period).fetch_optional(&st.pool).await?;
        let mut v = mission_json(r);
        v["period"] = json!(period);
        v["resetsAt"] = if m.schedule == "once" { json!(m.ends_at) } else { json!(to) };
        v["progress"] = p.as_ref().map(|p| num(p.get("progress"))).unwrap_or(json!(0));
        v["completedAt"] = p.as_ref().map(|p| json!(p.get::<Option<DateTime<Utc>>, _>("completed_at"))).unwrap_or(Value::Null);
        v["rewardStatus"] = p.as_ref().map(|p| json!(p.get::<String, _>("reward_status"))).unwrap_or(json!("none"));
        v["done"] = json!(p.as_ref().is_some_and(|p| p.get::<Option<DateTime<Utc>>, _>("completed_at").is_some()));
        // a finished one-off mission stays listed as done; the rest are shown from not started to done
        out.push(v);
    }
    Ok(out)
}

/// Background step: clients with fresh activity are re-evaluated, trading streaks advanced, rewards waiting for
/// KYC re-decided. Returns missions completed.
pub async fn tick(st: &AppState) -> anyhow::Result<usize> {
    let mut done = 0;
    for tenant in db::tenants(&st.pool).await? {
        let name = format!("missions:{tenant}");
        let since = db::cursor(&st.pool, &name).await?.and_then(|c| DateTime::parse_from_rfc3339(&c).ok()).map(|t| t.with_timezone(&Utc)).unwrap_or_else(|| Utc::now() - Duration::hours(1));
        let started = Utc::now();
        let settings = db::settings(&st.pool, &tenant).await?;
        // trading streaks from the deals processed since the last step
        for (user, day) in xp::trading_days_since(&st.pool, &tenant, since - Duration::minutes(1), settings.mission_min_lots, settings.mission_min_hold_seconds).await? {
            xp::touch(st, &tenant, user, "trading", day).await?;
        }
        let has_missions: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM missions WHERE tenant = $1 AND active)").bind(&tenant).fetch_one(&st.pool).await?;
        if has_missions {
            let users: Vec<i64> = sqlx::query_scalar(
                "SELECT DISTINCT u FROM (
                    SELECT user_id AS u FROM deals WHERE tenant = $1 AND processed_at >= $2
                    UNION SELECT user_id FROM profiles WHERE tenant = $1 AND (synced_at >= $2 OR first_deposit_at >= now() - interval '2 days')
                    UNION SELECT referred_by FROM profiles WHERE tenant = $1 AND referred_by IS NOT NULL AND first_deposit_at >= now() - interval '2 days'
                    UNION SELECT user_id FROM contest_entries WHERE tenant = $1 AND joined_at >= $2
                    UNION SELECT user_id FROM streaks WHERE tenant = $1 AND kind = 'login' AND updated_at >= $2
                 ) x WHERE u IS NOT NULL LIMIT 2000",
            )
            .bind(&tenant)
            .bind(since - Duration::minutes(1))
            .fetch_all(&st.pool)
            .await?;
            for u in users {
                match evaluate_user(st, &tenant, u).await {
                    Ok(n) => done += n,
                    Err(e) => tracing::warn!(%tenant, user = u, error = %e, "mission evaluation failed"),
                }
            }
        }
        // rewards that waited for KYC: decide again for clients verified since
        let waiting: Vec<(i64, D)> = sqlx::query_as(
            "SELECT p.id, p.reward_amount FROM mission_progress p JOIN profiles f ON f.tenant = p.tenant AND f.user_id = p.user_id
              WHERE p.tenant = $1 AND p.reward_status = 'kyc' AND f.kyc_status = 'verified' LIMIT 500",
        )
        .bind(&tenant)
        .fetch_all(&st.pool)
        .await?;
        for (pid, amount) in waiting {
            let s = decide(amount, settings.auto_pay_below);
            sqlx::query("UPDATE mission_progress SET reward_status = $2, updated_at = now() WHERE id = $1 AND reward_status = 'kyc'").bind(pid).bind(s).execute(&st.pool).await?;
            if s == "pending" {
                pay_reward(st, pid, &Actor::system()).await?;
            }
        }
        db::set_cursor(&st.pool, &name, &started.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)).await?;
    }
    Ok(done)
}

/// Keeps the profile fresh and evaluates in the background (after a join, a hub view …). Never fails the caller.
pub fn evaluate_soon(st: &AppState, tenant: &str, user_id: i64) {
    let st = st.clone();
    let tenant = tenant.to_string();
    tokio::spawn(async move {
        if let Err(e) = evaluate_user(&st, &tenant, user_id).await {
            tracing::debug!(error = %e, user = user_id, "mission evaluation skipped");
        }
    });
}

/// Validated mission input (admin).
pub fn check(kind: &str, schedule: &str, target: D, reward_amount: Option<D>, reward_payout: Option<&str>) -> ApiResult<()> {
    use crate::error::invalid;
    if !KINDS.contains(&kind) {
        return Err(invalid("kind", format!("One of {}.", KINDS.join(", "))));
    }
    if !matches!(schedule, "daily" | "weekly" | "once") {
        return Err(invalid("schedule", "daily, weekly or once."));
    }
    if ONCE_ONLY.contains(&kind) && schedule != "once" {
        return Err(invalid("schedule", "KYC and first-deposit missions can only be one-off."));
    }
    if target <= ZERO || target > D::from(10_000_000) {
        return Err(invalid("target", "The target must be above 0."));
    }
    if kind == "trading_days" && ((schedule == "daily" && target > D::ONE) || (schedule == "weekly" && target > D::from(7))) {
        return Err(invalid("target", "More trading days than the period has."));
    }
    if reward_amount.is_some_and(|a| a <= ZERO || a > D::from(100_000)) {
        return Err(invalid("rewardAmount", "Reward must be 0 – 100 000 USD (empty for XP only)."));
    }
    if reward_amount.is_some() && !matches!(reward_payout.unwrap_or("wallet"), "wallet" | "credit") {
        return Err(invalid("rewardPayout", "wallet or credit."));
    }
    Ok(())
}


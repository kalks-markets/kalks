//! Commission engine: closed deals in, commission lines out.
//!
//! Source: the trading engine's `GET /v1/dealing/deals?from&to&limit` (closing deals, newest first, max 2000).
//! The engine has no "deals since cursor" feed, so the poller keeps a close-time cursor, re-reads a 2-minute
//! overlap (a deal can commit slightly after its close time is stamped) and pages backwards when a window
//! holds more than 2000 deals. Every deal is processed once: `deals (source, deal_id, user_id)` is the
//! idempotency key, and commission lines are unique per (deal, client, beneficiary, kind).

use crate::audit::{self, Actor};
use crate::calc::{self, DealFacts, Upline};
use crate::clients::{self, AccountInfo, EngineDeal};
use crate::db;
use crate::model::{self, Level, Settings};
use crate::money::{D, HUNDRED, ZERO, r2};
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::Row;
use std::collections::{HashMap, HashSet};

/// A closed trade to evaluate. `source` is `engine` for trading-engine deals, or `pamm` / `copy` for lots
/// allocated to an investor by the copy/PAMM service (D64).
#[derive(Clone, Debug)]
pub struct DealInput {
    pub source: String,
    pub deal_id: i64,
    pub tenant: String,
    pub login: Option<i64>,
    pub user_id: i64,
    pub symbol: String,
    pub side: String,
    pub volume: D,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub kind: String,
    pub reversed: bool,
    pub account: Option<AccountInfo>,
    /// Kalks FX Options deal: `volume` is contracts, paid at the level's per-contract options rate (O34).
    pub option: bool,
    /// Ledger label of where the trade came from: `copy` / `mam` / `pamm` / `engine` (`clients::deal_origin`).
    /// Stored on the deal and its commission lines; `source` stays the feed (the idempotency key).
    pub origin: String,
}

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Duplicate,
    Recorded { qualified: bool, reason: Option<String>, lines: usize },
}

impl DealInput {
    pub fn from_engine(d: &EngineDeal, tenant: &str, account: Option<AccountInfo>) -> Self {
        DealInput {
            source: "engine".into(),
            deal_id: d.id,
            tenant: tenant.into(),
            login: Some(d.login),
            user_id: d.user_id,
            symbol: d.symbol.clone(),
            side: d.side.clone(),
            volume: d.volume,
            open_time: d.open_time,
            close_time: d.close_time,
            kind: d.kind.clone(),
            reversed: d.reversed,
            account,
            option: d.option,
            origin: d.origin.clone(),
        }
    }
}

/// What a level's rate is applied to: standard lots of a symbol group (CFD per-lot rates) or option contracts
/// (the separate per-contract options rate). A CFD per-lot rate is never applied to an option deal.
#[derive(Clone, Copy, Debug)]
pub enum Basis<'a> {
    Lots(&'a str),
    Contracts,
}

/// Upline chain of a client, tier 1 first, at most `depth` IBs, cycle-safe.
pub async fn chain(ex: &mut sqlx::PgConnection, tenant: &str, first: Option<i64>, depth: usize, levels: &[Level], basis: Basis<'_>) -> anyhow::Result<Vec<Upline>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut next = first;
    while let Some(id) = next {
        if out.len() >= depth || !seen.insert(id) {
            break;
        }
        let Some(r) = sqlx::query("SELECT user_id, parent_id, level_key, rebate_pct, split_pct, status FROM members WHERE user_id = $1 AND tenant = $2")
            .bind(id)
            .bind(tenant)
            .fetch_optional(&mut *ex)
            .await?
        else {
            break;
        };
        let level_key: String = r.get("level_key");
        let rate = levels
            .iter()
            .find(|l| l.key == level_key)
            .map(|l| match basis {
                Basis::Lots(group) => l.rate(group),
                Basis::Contracts => l.option_rate(),
            })
            .unwrap_or(ZERO);
        out.push(Upline {
            user_id: id,
            level_key,
            rate,
            rebate_pct: r.get("rebate_pct"),
            split_pct: r.get("split_pct"),
            active: r.get::<String, _>("status") == "active",
        });
        next = r.get("parent_id");
    }
    Ok(out)
}

pub async fn ingest(st: &AppState, d: &DealInput) -> anyhow::Result<Outcome> {
    let done: Option<i32> = sqlx::query_scalar("SELECT 1 FROM deals WHERE source = $1 AND deal_id = $2 AND user_id = $3").bind(&d.source).bind(d.deal_id).bind(d.user_id).fetch_optional(&st.pool).await?;
    if done.is_some() {
        return Ok(Outcome::Duplicate);
    }
    let settings = db::settings(&st.pool, &d.tenant).await?;
    let levels = db::levels(&st.pool, &d.tenant).await?;
    let acct = d.account.clone();
    let facts = DealFacts {
        account_kind: acct.as_ref().map(|a| a.kind.clone()).unwrap_or_else(|| "live".into()),
        group: acct.as_ref().map(|a| a.group.clone()).unwrap_or_default(),
        cent: acct.as_ref().is_some_and(|a| a.cent),
        kind: d.kind.clone(),
        reversed: d.reversed,
        volume: d.volume,
        open_time: d.open_time,
        close_time: d.close_time,
        option: d.option,
    };
    // option deals: 0 lots (never in lot statistics or level upgrades), paid per contract
    let lots = calc::std_lots(&facts, &settings);
    let contracts = calc::contracts(&facts);
    let units = calc::commission_units(&facts, &settings);
    let group = if d.option { Some(model::OPTIONS_GROUP.to_string()) } else { model::symbol_group(&d.symbol, &settings, &st.instruments) };
    let member = sqlx::query("SELECT parent_id, self_referral, abuse_cleared, joined_at FROM members WHERE user_id = $1 AND tenant = $2").bind(d.user_id).bind(&d.tenant).fetch_optional(&st.pool).await?;
    let parent: Option<i64> = member.as_ref().and_then(|m| m.get("parent_id"));
    let reason: Option<&str> = calc::disqualify(&facts, &settings)
        .or_else(|| member.is_none().then_some("unknown_client"))
        .or_else(|| parent.is_none().then_some("no_referrer"))
        // an engine account older than the client (e.g. a re-used id in a rebuilt environment) never earns
        .or_else(|| member.as_ref().is_some_and(|m| d.open_time < m.get::<DateTime<Utc>, _>("joined_at")).then_some("before_signup"))
        .or_else(|| member.as_ref().and_then(|m| m.get::<Option<String>, _>("self_referral")).map(|_| "self_referral"))
        .or_else(|| group.is_none().then_some("no_symbol_group"));
    let qualified = reason.is_none();

    let mut tx = st.pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO deals (source, deal_id, user_id, tenant, login, symbol, symbol_group, side, volume, lots, open_time, close_time, qualified, reason, reversed, instrument, contracts, origin)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) ON CONFLICT DO NOTHING",
    )
    .bind(&d.source)
    .bind(d.deal_id)
    .bind(d.user_id)
    .bind(&d.tenant)
    .bind(d.login)
    .bind(&d.symbol)
    .bind(&group)
    .bind(&d.side)
    .bind(d.volume)
    .bind(lots)
    .bind(d.open_time)
    .bind(d.close_time)
    .bind(qualified)
    .bind(reason)
    .bind(d.reversed)
    .bind(if d.option { "option" } else { "cfd" })
    .bind(contracts)
    .bind(&d.origin)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        tx.rollback().await?;
        return Ok(Outcome::Duplicate);
    }
    let mut n = 0;
    if let (true, Some(group)) = (qualified, group.as_deref()) {
        let basis = if d.option { Basis::Contracts } else { Basis::Lots(group) };
        let ch = chain(&mut tx, &d.tenant, parent, settings.tiers.len(), &levels, basis).await?;
        let lines = calc::commission_lines(d.user_id, units, &ch, &settings.tiers, settings.max_rebate_pct, settings.max_split_pct);
        for l in &lines {
            n += sqlx::query(
                "INSERT INTO commissions (tenant, beneficiary_id, kind, deal_source, deal_id, client_id, login, symbol, symbol_group, lots, tier, level_key, rate, share_pct, amount, available_at, contracts, origin)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15, now(), $16, $17) ON CONFLICT DO NOTHING",
            )
            .bind(&d.tenant)
            .bind(l.beneficiary)
            .bind(l.kind.as_str())
            .bind(&d.source)
            .bind(d.deal_id)
            .bind(d.user_id)
            .bind(d.login)
            .bind(&d.symbol)
            .bind(group)
            .bind(lots)
            .bind(l.tier)
            .bind(&l.level_key)
            .bind(l.rate)
            .bind(l.share_pct)
            .bind(l.amount)
            .bind(contracts)
            .bind(&d.origin)
            .execute(&mut *tx)
            .await?
            .rows_affected() as usize;
        }
        sqlx::query("UPDATE members SET first_trade_at = LEAST(COALESCE(first_trade_at, $2), $2) WHERE user_id = $1").bind(d.user_id).bind(d.close_time).execute(&mut *tx).await?;
    }
    tx.commit().await?;

    if parent.is_some() {
        if qualified {
            ensure_first_deposit(st, &d.tenant, d.user_id).await?;
            maybe_cpa(st, &d.tenant, d.user_id).await?;
        }
        if reason != Some("unknown_client") {
            abuse_checks(st, d, &settings, parent.unwrap()).await?;
        }
    }
    Ok(Outcome::Recorded { qualified, reason: reason.map(str::to_string), lines: n })
}

/// Deals recorded before their client was mirrored from the gateway are evaluated again.
pub async fn reprocess_unknown(st: &AppState, user_id: i64) -> anyhow::Result<()> {
    let rows = sqlx::query(
        "DELETE FROM deals d WHERE d.user_id = $1 AND d.reason = 'unknown_client'
         RETURNING d.source, d.deal_id, d.tenant, d.login, d.symbol, d.side, d.volume, d.open_time, d.close_time, d.reversed, d.instrument, d.origin",
    )
    .bind(user_id)
    .fetch_all(&st.pool)
    .await?;
    for r in rows {
        let login: Option<i64> = r.get("login");
        let account = match login {
            Some(l) => account_cached(st, r.get::<String, _>("tenant").as_str(), l).await.ok().flatten(),
            None => None,
        };
        let d = DealInput {
            source: r.get("source"),
            deal_id: r.get("deal_id"),
            tenant: r.get("tenant"),
            login,
            user_id,
            symbol: r.get("symbol"),
            side: r.get("side"),
            volume: r.get("volume"),
            open_time: r.get("open_time"),
            close_time: r.get("close_time"),
            kind: "close".into(),
            reversed: r.get("reversed"),
            account,
            option: r.get::<String, _>("instrument") == "option",
            origin: r.get("origin"),
        };
        Box::pin(ingest(st, &d)).await?;
    }
    Ok(())
}

/// One-time CPA for the client's direct IB once the client has a first live deposit ≥ the minimum and a
/// first qualifying live trade (D57). Payable after the hold period.
pub async fn maybe_cpa(st: &AppState, tenant: &str, client: i64) -> anyhow::Result<bool> {
    let s = db::settings(&st.pool, tenant).await?;
    if !s.cpa.enabled {
        return Ok(false);
    }
    let Some(m) = sqlx::query("SELECT parent_id, first_deposit_amount, first_trade_at, self_referral FROM members WHERE user_id = $1 AND tenant = $2").bind(client).bind(tenant).fetch_optional(&st.pool).await? else {
        return Ok(false);
    };
    let (Some(parent), Some(dep)) = (m.get::<Option<i64>, _>("parent_id"), m.get::<Option<D>, _>("first_deposit_amount")) else { return Ok(false) };
    if dep < s.cpa.min_first_deposit || m.get::<Option<String>, _>("self_referral").is_some() {
        return Ok(false);
    }
    if s.cpa.require_first_trade && m.get::<Option<DateTime<Utc>>, _>("first_trade_at").is_none() {
        return Ok(false);
    }
    let Some(ib) = sqlx::query("SELECT level_key, status FROM members WHERE user_id = $1").bind(parent).fetch_optional(&st.pool).await? else { return Ok(false) };
    if ib.get::<String, _>("status") != "active" {
        return Ok(false);
    }
    let level_key: String = ib.get("level_key");
    let levels = db::levels(&st.pool, tenant).await?;
    let amount = r2(levels.iter().find(|l| l.key == level_key).map(|l| l.cpa_amount).unwrap_or(ZERO));
    if amount <= ZERO {
        return Ok(false);
    }
    let n = sqlx::query(
        "INSERT INTO commissions (tenant, beneficiary_id, kind, client_id, tier, level_key, rate, share_pct, amount, available_at, note)
         VALUES ($1,$2,'cpa',$3,1,$4,$5,100,$5, now() + make_interval(days => $6), $7) ON CONFLICT DO NOTHING",
    )
    .bind(tenant)
    .bind(parent)
    .bind(client)
    .bind(&level_key)
    .bind(amount)
    .bind(s.cpa.hold_days as i32)
    .bind(format!("First deposit ${} + first trade", r2(dep)))
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n > 0 {
        tracing::info!(client, ib = parent, %amount, "CPA earned");
    }
    Ok(n > 0)
}

/// Records a first deposit (the earliest one wins) and re-checks CPA.
pub async fn record_deposit(st: &AppState, tenant: &str, client: i64, at: DateTime<Utc>, amount: D) -> anyhow::Result<bool> {
    let n = sqlx::query(
        "UPDATE members SET first_deposit_at = $3, first_deposit_amount = $4
         WHERE user_id = $1 AND tenant = $2 AND (first_deposit_at IS NULL OR first_deposit_at > $3)",
    )
    .bind(client)
    .bind(tenant)
    .bind(at)
    .bind(amount)
    .execute(&st.pool)
    .await?
    .rows_affected();
    if n > 0 {
        maybe_cpa(st, tenant, client).await?;
    }
    Ok(n > 0)
}

/// Looks up the client's first live deposit from the engine ledgers if it isn't known yet.
pub async fn ensure_first_deposit(st: &AppState, tenant: &str, client: i64) -> anyhow::Result<()> {
    let known: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT first_deposit_at FROM members WHERE user_id = $1").bind(client).fetch_optional(&st.pool).await?.flatten();
    if known.is_some() {
        return Ok(());
    }
    if let Err(e) = scan_deposits(st, tenant, client).await {
        tracing::warn!(error = %e, client, "first-deposit lookup failed; retried by the deposit worker");
    }
    Ok(())
}

async fn scan_deposits(st: &AppState, tenant: &str, client: i64) -> anyhow::Result<()> {
    let accounts = clients::live_accounts_of(st, tenant, client).await?;
    let joined: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT joined_at FROM members WHERE user_id = $1").bind(client).fetch_optional(&st.pool).await?;
    let mut first: Option<(DateTime<Utc>, D)> = None;
    for a in &accounts {
        cache_account(st, tenant, a).await?;
        if let Some((at, amt)) = clients::first_deposit(st, tenant, a.login, client).await?
            && joined.is_none_or(|j| at >= j)
            && first.as_ref().is_none_or(|f| at < f.0)
        {
            first = Some((at, amt));
        }
    }
    sqlx::query("UPDATE members SET deposits_checked_at = now() WHERE user_id = $1").bind(client).execute(&st.pool).await?;
    if let Some((at, amt)) = first {
        record_deposit(st, tenant, client, at, amt).await?;
    }
    Ok(())
}

/// Deposit worker: referred clients without a known first deposit (checked at most every 5 minutes each).
pub async fn deposits_tick(st: &AppState, tenant: &str) -> anyhow::Result<()> {
    let ids: Vec<i64> = sqlx::query_scalar(
        "SELECT user_id FROM members WHERE tenant = $1 AND parent_id IS NOT NULL AND first_deposit_at IS NULL
           AND (deposits_checked_at IS NULL OR deposits_checked_at < now() - interval '5 minutes')
           AND joined_at > now() - interval '365 days'
         ORDER BY deposits_checked_at NULLS FIRST LIMIT 100",
    )
    .bind(tenant)
    .fetch_all(&st.pool)
    .await?;
    for id in ids {
        scan_deposits(st, tenant, id).await?;
    }
    Ok(())
}

async fn cache_account(st: &AppState, tenant: &str, a: &AccountInfo) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO accounts (login, tenant, user_id, kind, grp, cent) VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (login) DO UPDATE SET kind = EXCLUDED.kind, grp = EXCLUDED.grp, cent = EXCLUDED.cent, user_id = EXCLUDED.user_id, fetched_at = now()",
    )
    .bind(a.login)
    .bind(tenant)
    .bind(a.user_id)
    .bind(&a.kind)
    .bind(&a.group)
    .bind(a.cent)
    .execute(&st.pool)
    .await?;
    Ok(())
}

/// Account kind / group / cent for a login (cached; the group can change, so entries older than an hour are refreshed).
pub async fn account_cached(st: &AppState, tenant: &str, login: i64) -> anyhow::Result<Option<AccountInfo>> {
    if let Some(r) = sqlx::query("SELECT user_id, kind, grp, cent FROM accounts WHERE login = $1 AND fetched_at > now() - interval '1 hour'").bind(login).fetch_optional(&st.pool).await? {
        return Ok(Some(AccountInfo { login, user_id: r.get("user_id"), kind: r.get("kind"), group: r.get("grp"), cent: r.get("cent") }));
    }
    let a = clients::account(st, tenant, login).await?;
    if let Some(a) = &a {
        cache_account(st, tenant, a).await?;
    }
    Ok(a)
}

// ---------------------------------------------------------------- abuse heuristics

async fn flag(st: &AppState, tenant: &str, kind: &str, severity: &str, client: i64, ib: i64, dedup: String, details: serde_json::Value) -> anyhow::Result<bool> {
    let n = sqlx::query("INSERT INTO fraud_flags (tenant, kind, severity, client_id, ib_id, dedup, details) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (tenant, dedup) DO NOTHING")
        .bind(tenant)
        .bind(kind)
        .bind(severity)
        .bind(client)
        .bind(ib)
        .bind(dedup)
        .bind(sqlx::types::Json(details))
        .execute(&st.pool)
        .await?
        .rows_affected();
    Ok(n > 0)
}

/// Wash trading: an opposite, similar-size trade on the same symbol by the IB itself or another client of the
/// same IB, opened and closed within the window. Short-trade pattern: many sub-minimum trades in 24 h.
async fn abuse_checks(st: &AppState, d: &DealInput, s: &Settings, ib: i64) -> anyhow::Result<()> {
    if !s.wash.enabled {
        return Ok(());
    }
    let w = Duration::seconds(s.wash.window_secs);
    let rows = sqlx::query(
        "SELECT d.source, d.deal_id, d.user_id, d.side, d.volume, d.open_time, d.close_time FROM deals d JOIN members m ON m.user_id = d.user_id
         WHERE d.tenant = $1 AND d.symbol = $2 AND d.close_time BETWEEN $3 AND $4 AND d.user_id <> $5 AND NOT d.reversed
           AND (m.parent_id = $6 OR m.user_id = $6)
         LIMIT 200",
    )
    .bind(&d.tenant)
    .bind(&d.symbol)
    .bind(d.close_time - w)
    .bind(d.close_time + w)
    .bind(d.user_id)
    .bind(ib)
    .fetch_all(&st.pool)
    .await?;
    let me = calc::TradeLite { user_id: d.user_id, side: d.side.clone(), volume: d.volume, open_time: d.open_time, close_time: d.close_time };
    for r in rows {
        let other = calc::TradeLite { user_id: r.get("user_id"), side: r.get("side"), volume: r.get("volume"), open_time: r.get("open_time"), close_time: r.get("close_time") };
        if calc::is_wash_pair(&me, &other, s.wash.window_secs, s.wash.volume_tolerance_pct) {
            let other_deal: i64 = r.get("deal_id");
            let (a, b) = (d.deal_id.min(other_deal), d.deal_id.max(other_deal));
            let added = flag(
                st,
                &d.tenant,
                "wash_trading",
                "high",
                d.user_id,
                ib,
                format!("wash:{}:{a}:{b}", d.source),
                json!({"symbol": d.symbol, "deals": [d.deal_id, other_deal], "users": [d.user_id, other.user_id], "volumes": [d.volume.to_string(), other.volume.to_string()]}),
            )
            .await?;
            if added {
                tracing::warn!(deal = d.deal_id, other = other_deal, "wash-trade pair flagged");
            }
        }
    }
    let r = sqlx::query(
        "SELECT count(*) AS total, count(*) FILTER (WHERE reason = 'short_duration') AS short FROM deals
         WHERE tenant = $1 AND user_id = $2 AND close_time > $3 - interval '24 hours' AND close_time <= $3",
    )
    .bind(&d.tenant)
    .bind(d.user_id)
    .bind(d.close_time)
    .fetch_one(&st.pool)
    .await?;
    let (total, short): (i64, i64) = (r.get("total"), r.get("short"));
    if short >= s.wash.short_trades_min && D::from(short) * HUNDRED >= s.wash.short_trades_pct * D::from(total.max(1)) {
        flag(st, &d.tenant, "short_trades", "medium", d.user_id, ib, format!("short:{}:{}", d.user_id, d.close_time.date_naive()), json!({"short": short, "total": total, "minTradeSeconds": s.min_trade_seconds})).await?;
    }
    Ok(())
}

// ---------------------------------------------------------------- engine poller

/// All closing deals in `[from, now)`, paging backwards when a window holds more than the engine cap.
async fn fetch_window(st: &AppState, tenant: &str, from: Option<DateTime<Utc>>) -> anyhow::Result<Vec<EngineDeal>> {
    const CAP: i64 = 2000;
    let mut all: HashMap<i64, EngineDeal> = HashMap::new();
    let mut to: Option<DateTime<Utc>> = None;
    loop {
        let page = clients::closing_deals(st, tenant, from, to, CAP).await?;
        let full = page.len() as i64 >= CAP;
        let oldest = page.iter().map(|d| d.close_time).min();
        for d in page {
            all.insert(d.id, d);
        }
        match (full, oldest) {
            // re-include the oldest instant: deals sharing it may have been cut off
            (true, Some(o)) if to.is_none_or(|t| o + Duration::microseconds(1) < t) => to = Some(o + Duration::microseconds(1)),
            _ => break,
        }
    }
    let mut v: Vec<EngineDeal> = all.into_values().collect();
    v.sort_by_key(|d| (d.close_time, d.id));
    Ok(v)
}

/// One poll of the engine for a tenant. Returns the number of new deals processed.
pub async fn poll_engine(st: &AppState, tenant: &str) -> anyhow::Result<usize> {
    let name = format!("deals:{tenant}");
    let cur = db::cursor(&st.pool, &name).await?.and_then(|c| DateTime::parse_from_rfc3339(&c).ok()).map(|t| t.with_timezone(&Utc));
    let deals = fetch_window(st, tenant, cur.map(|c| c - Duration::seconds(120))).await?;
    if deals.is_empty() {
        return Ok(0);
    }
    let ids: Vec<i64> = deals.iter().map(|d| d.id).collect();
    let seen: HashSet<i64> = sqlx::query_scalar::<_, i64>("SELECT deal_id FROM deals WHERE source = 'engine' AND deal_id = ANY($1)").bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let mut n = 0;
    let mut max = cur;
    for d in &deals {
        if !seen.contains(&d.id) {
            let account = account_cached(st, tenant, d.login).await?;
            if let Outcome::Recorded { .. } = ingest(st, &DealInput::from_engine(d, tenant, account)).await? {
                n += 1;
            }
        }
        if max.is_none_or(|m| d.close_time > m) {
            max = Some(d.close_time);
        }
    }
    if let Some(m) = max {
        db::set_cursor(&st.pool, &name, &m.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)).await?;
    }
    Ok(n)
}

/// Deals reopened by the dealing desk after they were processed: pending lines are voided, lines already in a
/// batch or paid are clawed back with a negative line.
pub async fn sweep_reversals(st: &AppState, tenant: &str) -> anyhow::Result<usize> {
    let deals = fetch_window(st, tenant, Some(Utc::now() - Duration::days(14))).await?;
    let reversed: Vec<i64> = deals.iter().filter(|d| d.reversed).map(|d| d.id).collect();
    if reversed.is_empty() {
        return Ok(0);
    }
    let fresh: Vec<i64> = sqlx::query_scalar("SELECT deal_id FROM deals WHERE source = 'engine' AND deal_id = ANY($1) AND NOT reversed").bind(&reversed).fetch_all(&st.pool).await?;
    for id in &fresh {
        reverse_deal(st, tenant, "engine", *id).await?;
    }
    Ok(fresh.len())
}

pub async fn reverse_deal(st: &AppState, tenant: &str, source: &str, deal_id: i64) -> anyhow::Result<()> {
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE deals SET reversed = true WHERE source = $1 AND deal_id = $2").bind(source).bind(deal_id).execute(&mut *tx).await?;
    let voided = sqlx::query(
        "UPDATE commissions SET status = 'void', note = 'Deal reopened by the dealing desk', updated_at = now()
         WHERE deal_source = $1 AND deal_id = $2 AND status = 'pending' AND batch_id IS NULL AND kind <> 'clawback'",
    )
    .bind(source)
    .bind(deal_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let rows = sqlx::query(
        "SELECT beneficiary_id, client_id, sum(amount) AS amount, min(symbol) AS symbol, min(login) AS login, min(origin) AS origin FROM commissions
         WHERE deal_source = $1 AND deal_id = $2 AND kind <> 'clawback' AND status <> 'void' AND status <> 'rejected'
         GROUP BY beneficiary_id, client_id",
    )
    .bind(source)
    .bind(deal_id)
    .fetch_all(&mut *tx)
    .await?;
    for r in &rows {
        let amt: D = r.get("amount");
        if amt <= ZERO {
            continue;
        }
        sqlx::query(
            "INSERT INTO commissions (tenant, beneficiary_id, kind, deal_source, deal_id, client_id, login, symbol, amount, note, origin)
             VALUES ($1,$2,'clawback',$3,$4,$5,$6,$7,$8,'Deal reopened by the dealing desk',$9) ON CONFLICT DO NOTHING",
        )
        .bind(tenant)
        .bind(r.get::<i64, _>("beneficiary_id"))
        .bind(source)
        .bind(deal_id)
        .bind(r.get::<i64, _>("client_id"))
        .bind(r.get::<Option<i64>, _>("login"))
        .bind(r.get::<Option<String>, _>("symbol"))
        .bind(-amt)
        .bind(r.get::<Option<String>, _>("origin"))
        .execute(&mut *tx)
        .await?;
    }
    audit::record(&mut *tx, tenant, &Actor::system(), "deal.reversed", Some(format!("deal:{source}:{deal_id}")), None, Some(json!({"voided": voided, "clawbacks": rows.len()})), None).await?;
    tx.commit().await?;
    Ok(())
}

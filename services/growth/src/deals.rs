//! Closed deals in, rewards out: loyalty points, cashback accruals, bonus release legs and contest trades.
//!
//! Source: the engine's `GET /v1/dealing/deals?from&to&limit` (closing deals, newest first, max 2000). Like the
//! IB service, the poller keeps a close-time cursor, re-reads a 2-minute overlap and pages backwards when a
//! window is full. `deals.deal_id` is the idempotency key: a deal is processed exactly once, in one transaction
//! with everything it produces.

use crate::calc;
use crate::clients::{self, EngineDeal};
use crate::db;
use crate::model::{EarnRule, Settings, Tier};
use crate::money::{D, HUNDRED, ZERO};
use crate::state::AppState;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use sqlx::Row;
use std::collections::{HashMap, HashSet};

/// Account facts the rules need.
#[derive(Clone, Debug)]
pub struct AccountFacts {
    pub kind: String,
    pub group: String,
    pub cent: bool,
}

#[derive(Clone, Debug)]
pub struct DealIn {
    pub deal_id: i64,
    pub tenant: String,
    pub login: i64,
    pub user_id: i64,
    pub symbol: String,
    pub side: String,
    pub volume: D,
    /// Account currency (USC on cent accounts), as the engine reports it.
    pub profit: D,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub kind: String,
    pub reversed: bool,
    pub account: AccountFacts,
    /// Kalks FX Options deal (volume = contracts): recorded once, earns no loyalty points, cashback or bonus
    /// lot-release (O34); counts only in options contests (O36), never in CFD contests.
    pub option: bool,
    /// Options: opening premium of the closed contracts (account currency, unsigned), for the contest minimum.
    pub premium: Option<D>,
    /// Options: the order-book fill the deal came from (self-trade detection across a client's accounts).
    pub fill_id: Option<String>,
}

impl DealIn {
    pub fn from_engine(d: &EngineDeal, tenant: &str, account: AccountFacts) -> Self {
        DealIn {
            deal_id: d.id,
            tenant: tenant.into(),
            login: d.login,
            user_id: d.user_id,
            symbol: d.symbol.clone(),
            side: d.side.clone(),
            volume: d.volume,
            profit: d.profit,
            open_time: d.open_time,
            close_time: d.close_time,
            kind: d.kind.clone(),
            reversed: d.reversed,
            account,
            option: d.option,
            premium: d.premium,
            fill_id: d.fill_id.clone(),
        }
    }
}

/// Per-tenant configuration loaded once per poll.
pub struct Programme {
    pub settings: Settings,
    pub rules: Vec<EarnRule>,
    pub tiers: Vec<Tier>,
}

impl Programme {
    pub async fn load(st: &AppState, tenant: &str) -> anyhow::Result<Self> {
        Ok(Programme { settings: db::settings(&st.pool, tenant).await?, rules: db::rules(&st.pool, tenant).await?, tiers: db::tiers(&st.pool, tenant).await? })
    }
}

/// What a deal produced (for tests and logs).
#[derive(Debug, Default, PartialEq)]
pub struct Produced {
    pub points: i64,
    pub cashback: D,
    pub bonus_released: D,
    pub contest_entries: usize,
}

pub fn month_start(t: DateTime<Utc>) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(t.year(), t.month(), 1, 0, 0, 0).unwrap()
}

/// Points earned in the last 12 months (tiering base).
pub async fn earned_12m<'e, E: sqlx::PgExecutor<'e>>(ex: E, tenant: &str, user_id: i64) -> anyhow::Result<i64> {
    let v: Option<i64> = sqlx::query_scalar(
        "SELECT sum(points)::bigint FROM points_ledger WHERE tenant = $1 AND user_id = $2 AND kind IN ('earn','bonus','promo','reversal') AND created_at > now() - interval '365 days'",
    )
    .bind(tenant)
    .bind(user_id)
    .fetch_one(ex)
    .await?;
    Ok(v.unwrap_or(0).max(0))
}

/// Processes one deal once. Returns None when the deal was already seen.
pub async fn ingest(st: &AppState, p: &Programme, d: &DealIn) -> anyhow::Result<Option<Produced>> {
    // option contracts are never lots: an option deal has 0 lots and no asset class of a CFD
    let option = d.option || calc::is_option_series(&d.symbol);
    let lots = if option { ZERO } else { calc::lots(d.volume, d.account.cent) };
    let profit_usd = if d.account.cent { d.profit / HUNDRED } else { d.profit };
    let asset_class = if option { Some("options".to_string()) } else { st.instruments.class_of(&d.symbol) };
    let premium_usd = if option { d.premium.map(|p| if d.account.cent { p / HUNDRED } else { p }) } else { None };
    let mut tx = st.pool.begin().await?;
    let inserted = sqlx::query(
        "INSERT INTO deals (deal_id, tenant, login, user_id, account_kind, account_group, symbol, asset_class, side, volume, lots, profit, deal_kind, open_time, close_time, reversed, instrument, premium, fill_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19) ON CONFLICT (deal_id) DO NOTHING",
    )
    .bind(d.deal_id)
    .bind(&d.tenant)
    .bind(d.login)
    .bind(d.user_id)
    .bind(&d.account.kind)
    .bind(&d.account.group)
    .bind(&d.symbol)
    .bind(&asset_class)
    .bind(&d.side)
    .bind(d.volume)
    .bind(lots)
    .bind(profit_usd)
    .bind(&d.kind)
    .bind(d.open_time)
    .bind(d.close_time)
    .bind(d.reversed)
    .bind(if option { "option" } else { "cfd" })
    .bind(premium_usd)
    .bind(d.fill_id.as_deref().filter(|f| option && !f.is_empty()))
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if inserted == 0 {
        return Ok(None);
    }
    let mut out = Produced::default();
    let hold = (d.close_time - d.open_time).num_seconds();
    // options: no points, cashback or bonus release (O34). They count only in options contests (O36).
    if option {
        if !d.reversed && d.kind != "price-correction" && d.volume > ZERO {
            out.contest_entries = option_contest_trades(&mut tx, d, profit_usd, premium_usd, hold).await?;
        }
        tx.commit().await?;
        return Ok(Some(out));
    }
    if d.reversed || d.kind == "price-correction" || lots <= ZERO {
        tx.commit().await?;
        return Ok(Some(out));
    }

    // ---- loyalty points
    let demo_ok = d.account.kind == "live" || p.settings.demo_points;
    if demo_ok && hold >= p.settings.min_hold_seconds {
        if let Some(rule) = calc::match_rule(&p.rules, &d.symbol, asset_class.as_deref(), &d.account.group, &d.account.kind) {
            let earned = earned_12m(&mut *tx, &d.tenant, d.user_id).await?;
            let mult = calc::tier_for(&p.tiers, earned).map(|t| t.multiplier).unwrap_or(D::ONE);
            let pts = calc::points_for(lots, rule.points_per_lot, mult);
            if pts > 0 {
                sqlx::query(
                    "INSERT INTO points_ledger (tenant, user_id, kind, points, ref, deal_id, login, lots, description) VALUES ($1,$2,'earn',$3,$4,$5,$6,$7,$8)
                     ON CONFLICT DO NOTHING",
                )
                .bind(&d.tenant)
                .bind(d.user_id)
                .bind(pts)
                .bind(format!("deal:{}", d.deal_id))
                .bind(d.deal_id)
                .bind(d.login)
                .bind(lots)
                .bind(format!("{} · {} lots", d.symbol, lots.normalize()))
                .execute(&mut *tx)
                .await?;
                out.points = pts;
            }
        }
    }

    // ---- cashback (live only)
    if d.account.kind == "live" {
        let progs = sqlx::query(
            "SELECT id, asset_classes, symbols, account_groups, usd_per_lot, max_per_month, opt_in FROM cashback_programmes
             WHERE tenant = $1 AND active AND starts_at <= $2 AND (ends_at IS NULL OR ends_at > $2) ORDER BY id",
        )
        .bind(&d.tenant)
        .bind(d.close_time)
        .fetch_all(&mut *tx)
        .await?;
        for r in progs {
            let classes: Vec<String> = r.get("asset_classes");
            let symbols: Vec<String> = r.get("symbols");
            let groups: Vec<String> = r.get("account_groups");
            let sym_ok = if symbols.is_empty() && classes.is_empty() {
                true
            } else {
                symbols.iter().any(|s| s.eq_ignore_ascii_case(&d.symbol)) || asset_class.as_ref().is_some_and(|a| classes.iter().any(|c| c.eq_ignore_ascii_case(a)))
            };
            if !sym_ok || !(groups.is_empty() || groups.iter().any(|g| g.eq_ignore_ascii_case(&d.account.group))) {
                continue;
            }
            let pid: i64 = r.get("id");
            if r.get::<bool, _>("opt_in") {
                let enrolled: Option<i32> = sqlx::query_scalar("SELECT 1 FROM cashback_enrolments WHERE programme_id = $1 AND user_id = $2").bind(pid).bind(d.user_id).fetch_optional(&mut *tx).await?;
                if enrolled.is_none() {
                    continue;
                }
            }
            let month: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM cashback_accruals WHERE programme_id = $1 AND user_id = $2 AND status <> 'void' AND created_at >= $3")
                .bind(pid)
                .bind(d.user_id)
                .bind(month_start(d.close_time))
                .fetch_one(&mut *tx)
                .await?;
            let amount = calc::cashback_amount(lots, r.get("usd_per_lot"), month.unwrap_or(ZERO), r.get("max_per_month"));
            if amount > ZERO {
                sqlx::query("INSERT INTO cashback_accruals (tenant, programme_id, user_id, deal_id, login, symbol, lots, amount) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT DO NOTHING")
                    .bind(&d.tenant)
                    .bind(pid)
                    .bind(d.user_id)
                    .bind(d.deal_id)
                    .bind(d.login)
                    .bind(&d.symbol)
                    .bind(lots)
                    .bind(amount)
                    .execute(&mut *tx)
                    .await?;
                out.cashback += amount;
            }
        }
    }

    // ---- bonus release per lot (D29)
    if let Some(g) = sqlx::query("SELECT id, amount, released, removed, release_per_lot, granted_at FROM bonus_grants WHERE login = $1 AND status = 'active' FOR UPDATE")
        .bind(d.login)
        .fetch_optional(&mut *tx)
        .await?
    {
        let granted: Option<DateTime<Utc>> = g.get("granted_at");
        if granted.is_some_and(|t| d.open_time >= t) {
            let id: i64 = g.get("id");
            let amount: D = g.get("amount");
            let released: D = g.get("released");
            let removed: D = g.get("removed");
            let rel = calc::release_for(lots, g.get("release_per_lot"), amount - released - removed);
            sqlx::query("UPDATE bonus_grants SET lots_traded = lots_traded + $2, released = released + $3 WHERE id = $1").bind(id).bind(lots).bind(rel).execute(&mut *tx).await?;
            if rel > ZERO {
                sqlx::query("INSERT INTO bonus_events (grant_id, tenant, kind, amount, lots, deal_id, ref) VALUES ($1,$2,'release',$3,$4,$5,$6) ON CONFLICT DO NOTHING")
                    .bind(id)
                    .bind(&d.tenant)
                    .bind(rel)
                    .bind(lots)
                    .bind(d.deal_id)
                    .bind(format!("deal:{}", d.deal_id))
                    .execute(&mut *tx)
                    .await?;
                out.bonus_released = rel;
                if released + removed + rel >= amount {
                    sqlx::query("UPDATE bonus_grants SET status = 'completed', ended_at = now(), end_reason = 'released' WHERE id = $1").bind(id).execute(&mut *tx).await?;
                }
            }
        }
    }

    // ---- contests (CFD ones): deals closed inside the window on an entered account
    let entries = sqlx::query(
        "SELECT e.id, e.contest_id FROM contest_entries e JOIN contests c ON c.id = e.contest_id
         WHERE e.login = $1 AND e.status = 'active' AND c.instrument = 'cfd' AND c.status NOT IN ('draft','cancelled','finalized','paid')
           AND c.starts_at <= $2 AND c.ends_at > $2 AND $3 >= c.starts_at
           AND (cardinality(c.asset_classes) = 0 OR lower(COALESCE($4, '')) = ANY(c.asset_classes))",
    )
    .bind(d.login)
    .bind(d.close_time)
    .bind(d.open_time)
    .bind(&asset_class)
    .fetch_all(&mut *tx)
    .await?;
    for e in entries {
        sqlx::query("INSERT INTO contest_trades (contest_id, entry_id, deal_id, profit, lots, hold_seconds, close_time) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING")
            .bind(e.get::<i64, _>("contest_id"))
            .bind(e.get::<i64, _>("id"))
            .bind(d.deal_id)
            .bind(profit_usd)
            .bind(lots)
            .bind(hold)
            .bind(d.close_time)
            .execute(&mut *tx)
            .await?;
        out.contest_entries += 1;
    }
    tx.commit().await?;
    if out.bonus_released > ZERO {
        st.wake.notify_one();
    }
    Ok(Some(out))
}

/// Options contests (O36): an option exit closed inside the window on an entered account, opened at or after the
/// start (like CFD contests). Volume is contracts, never lots. A trade whose opening premium is below the contest's
/// minimum is kept with `excluded = 'min_premium'` (no volume, no trade count; its P&L counts). Self-trades are
/// decided at each leaderboard refresh, once the other account's leg is known. Returns the entries it counted in.
async fn option_contest_trades(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, d: &DealIn, profit_usd: D, premium_usd: Option<D>, hold: i64) -> anyhow::Result<usize> {
    let entries = sqlx::query(
        "SELECT e.id, e.contest_id, c.min_premium FROM contest_entries e JOIN contests c ON c.id = e.contest_id
         WHERE e.login = $1 AND e.status = 'active' AND c.instrument = 'options' AND c.status NOT IN ('draft','cancelled','finalized','paid')
           AND c.starts_at <= $2 AND c.ends_at > $2 AND $3 >= c.starts_at",
    )
    .bind(d.login)
    .bind(d.close_time)
    .bind(d.open_time)
    .fetch_all(&mut **tx)
    .await?;
    for e in &entries {
        let excluded = calc::option_exclusion(false, premium_usd, e.get("min_premium"));
        sqlx::query(
            "INSERT INTO contest_trades (contest_id, entry_id, deal_id, profit, lots, contracts, premium, excluded, hold_seconds, close_time) VALUES ($1,$2,$3,$4,0,$5,$6,$7,$8,$9)
             ON CONFLICT DO NOTHING",
        )
        .bind(e.get::<i64, _>("contest_id"))
        .bind(e.get::<i64, _>("id"))
        .bind(d.deal_id)
        .bind(profit_usd)
        .bind(d.volume)
        .bind(premium_usd)
        .bind(excluded)
        .bind(hold)
        .bind(d.close_time)
        .execute(&mut **tx)
        .await?;
    }
    Ok(entries.len())
}

// ---------------------------------------------------------------- engine poller

pub async fn account_facts(st: &AppState, tenant: &str, login: i64) -> anyhow::Result<Option<AccountFacts>> {
    if let Some(r) = sqlx::query("SELECT kind, group_code, cent FROM accounts WHERE login = $1 AND fetched_at > now() - interval '1 hour'").bind(login).fetch_optional(&st.pool).await? {
        return Ok(Some(AccountFacts { kind: r.get("kind"), group: r.get("group_code"), cent: r.get("cent") }));
    }
    let Some(a) = clients::account(st, tenant, login).await? else { return Ok(None) };
    cache_account(st, tenant, &a).await?;
    Ok(Some(AccountFacts { kind: a.kind, group: a.group, cent: a.cent }))
}

pub async fn cache_account(st: &AppState, tenant: &str, a: &clients::Account) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO accounts (login, tenant, user_id, kind, group_code, cent, fetched_at) VALUES ($1,$2,$3,$4,$5,$6, now())
         ON CONFLICT (login) DO UPDATE SET kind = $4, group_code = $5, cent = $6, fetched_at = now()",
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

/// All closing deals in `[from, now)`, paging backwards when a window holds more than the engine cap.
async fn fetch_window(st: &AppState, tenant: &str, from: Option<DateTime<Utc>>) -> anyhow::Result<Vec<EngineDeal>> {
    const CAP: i64 = 2000;
    let mut all: HashMap<i64, EngineDeal> = HashMap::new();
    let mut to: Option<DateTime<Utc>> = None;
    loop {
        let page = clients::closing_deals(st, tenant, None, from, to, CAP).await?;
        let full = page.len() as i64 >= CAP;
        let oldest = page.iter().map(|d| d.close_time).min();
        for d in page {
            all.insert(d.id, d);
        }
        match (full, oldest) {
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
    // first run: start from now (history before the service existed earns nothing)
    let Some(cur) = cur else {
        db::set_cursor(&st.pool, &name, &Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true)).await?;
        return Ok(0);
    };
    let deals = fetch_window(st, tenant, Some(cur - Duration::seconds(120))).await?;
    if deals.is_empty() {
        return Ok(0);
    }
    let ids: Vec<i64> = deals.iter().map(|d| d.id).collect();
    let seen: HashSet<i64> = sqlx::query_scalar::<_, i64>("SELECT deal_id FROM deals WHERE deal_id = ANY($1)").bind(&ids).fetch_all(&st.pool).await?.into_iter().collect();
    let prog = Programme::load(st, tenant).await?;
    let mut n = 0;
    let mut max = cur;
    for d in &deals {
        if !seen.contains(&d.id) {
            let facts = account_facts(st, tenant, d.login).await?.unwrap_or(AccountFacts { kind: "unknown".into(), group: String::new(), cent: false });
            if ingest(st, &prog, &DealIn::from_engine(d, tenant, facts)).await?.is_some() {
                n += 1;
            }
        }
        if d.close_time > max {
            max = d.close_time;
        }
    }
    db::set_cursor(&st.pool, &name, &max.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)).await?;
    Ok(n)
}

/// Deals the desk reopened after they were processed: points reversed, unpaid cashback voided, contest trade
/// removed (unless the contest is finalized). Bonus releases stay (logged).
pub async fn sweep_reversals(st: &AppState, tenant: &str) -> anyhow::Result<usize> {
    let deals = fetch_window(st, tenant, Some(Utc::now() - Duration::days(14))).await?;
    let reversed: Vec<i64> = deals.iter().filter(|d| d.reversed).map(|d| d.id).collect();
    if reversed.is_empty() {
        return Ok(0);
    }
    let fresh: Vec<i64> = sqlx::query_scalar("SELECT deal_id FROM deals WHERE deal_id = ANY($1) AND NOT reversed").bind(&reversed).fetch_all(&st.pool).await?;
    for id in &fresh {
        reverse_deal(st, *id).await?;
    }
    Ok(fresh.len())
}

pub async fn reverse_deal(st: &AppState, deal_id: i64) -> anyhow::Result<()> {
    let mut tx = st.pool.begin().await?;
    let Some(d) = sqlx::query("UPDATE deals SET reversed = true WHERE deal_id = $1 AND NOT reversed RETURNING tenant, user_id, symbol").bind(deal_id).fetch_optional(&mut *tx).await? else {
        return Ok(());
    };
    let tenant: String = d.get("tenant");
    let earned: Option<(i64, i64)> = sqlx::query_as("SELECT user_id, points FROM points_ledger WHERE tenant = $1 AND kind = 'earn' AND ref = $2").bind(&tenant).bind(format!("deal:{deal_id}")).fetch_optional(&mut *tx).await?;
    if let Some((user, pts)) = earned {
        sqlx::query("INSERT INTO points_ledger (tenant, user_id, kind, points, ref, deal_id, description) VALUES ($1,$2,'reversal',$3,$4,$5,$6) ON CONFLICT DO NOTHING")
            .bind(&tenant)
            .bind(user)
            .bind(-pts)
            .bind(format!("deal:{deal_id}"))
            .bind(deal_id)
            .bind(format!("Deal {deal_id} reopened by the dealing desk"))
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE cashback_accruals SET status = 'void' WHERE deal_id = $1 AND status = 'accrued'").bind(deal_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM contest_trades t USING contests c WHERE t.contest_id = c.id AND t.deal_id = $1 AND c.status NOT IN ('finalized','paid')").bind(deal_id).execute(&mut *tx).await?;
    let released: Option<D> = sqlx::query_scalar("SELECT sum(amount) FROM bonus_events WHERE kind = 'release' AND deal_id = $1").bind(deal_id).fetch_one(&mut *tx).await?;
    if released.is_some_and(|r| r > ZERO) {
        tracing::warn!(deal_id, "reopened deal had released bonus; release kept for review");
    }
    tx.commit().await?;
    Ok(())
}

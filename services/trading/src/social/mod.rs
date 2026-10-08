//! Copy trading and PAMM (D65–D76, D125). See README "Copy trading and PAMM".
//!
//! - `math`: pure sizing, fee, NAV and statistics maths.
//! - `mirror`: the follower side of a mirrored master event (runs inside the follower's shard).
//! - `copier`: consumes committed master events (the shard tap), catch-up after restart, the guard loop
//!   (equity stop / max DD, fund protection) and the scheduler (fee periods, rollovers, snapshots).
//! - `pamm`: funds, invest / redeem requests, rollover execution, stop-loss, freeze.
//! - `stats`: daily snapshots, return index, leaderboard figures.
//! - `wallet`: the wallet-service client and the outbox of credits the engine owes.
//!
//! Social configuration and state live in their own tables (migrations/0002_social.sql); every trade and
//! every money movement on a trading account still goes through the account's shard as ordinary events.

pub mod alerts;
pub mod allocation;
pub mod copier;
pub mod mam;
pub mod math;
pub mod mirror;
pub mod pamm;
pub mod stats;
pub mod wallet;

#[cfg(test)]
mod tests;

use chrono::{DateTime, Datelike, Utc, Weekday};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use crate::api::LoginAlloc;
use crate::model::{Account, AccountKind, Controls, Status};
use crate::money::{D, ZERO, num, num_opt};
use crate::shard::{Hub, Staff};
use crate::specs::{server_date, server_midnight};
use math::{Sizing, SizingMode};
use wallet::WalletClient;

/* ------------------------------------------------------------------ */
/* Types                                                               */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
pub struct Settings {
    pub fee_min_pct: D,
    pub fee_max_pct: D,
    pub platform_cut_pct: D,
    pub min_track_days: i64,
    pub min_own_capital_pct: D,
    pub min_master_equity: D,
    pub min_allocation: D,
    pub trade_delay_minutes: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            fee_min_pct: ZERO,
            fee_max_pct: D::from(50),
            platform_cut_pct: D::from(20),
            min_track_days: 30,
            min_own_capital_pct: D::from(5),
            min_master_equity: D::from(100),
            min_allocation: D::from(50),
            trade_delay_minutes: 30,
        }
    }
}

impl Settings {
    pub fn json(&self) -> Value {
        json!({"feeMinPct": num(self.fee_min_pct), "feeMaxPct": num(self.fee_max_pct), "platformCutPct": num(self.platform_cut_pct),
               "minTrackDays": self.min_track_days, "minOwnCapitalPct": num(self.min_own_capital_pct), "minMasterEquity": num(self.min_master_equity),
               "minAllocation": num(self.min_allocation), "tradeDelayMinutes": self.trade_delay_minutes})
    }
}

#[derive(Clone, Debug)]
pub struct Master {
    pub id: i64,
    pub tenant_id: i64,
    pub user_id: i64,
    pub login: i64,
    pub nickname: String,
    pub strategy: String,
    pub description: String,
    /// copy | pamm | both
    pub program: String,
    pub perf_fee_pct: D,
    /// daily | weekly | monthly
    pub fee_period: String,
    pub min_allocation: D,
    /// pending | approved | rejected | suspended
    pub status: String,
    pub hidden: bool,
    pub frozen: bool,
    pub kyc_verified: bool,
    pub checks: Value,
    pub review_note: Option<String>,
    pub reviewed_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
    /// House account: platform-owned, runs an automated strategy (services/algo "House accounts").
    pub is_house: bool,
    /// A11: at most this many copying followers (None = no limit).
    pub max_followers: Option<i32>,
    /// A11: false = no new followers (existing ones keep copying).
    pub accept_new: bool,
    /// A11: private copy link: hidden from the leaderboard, following needs `invite_code`.
    pub invite_only: bool,
    pub invite_code: Option<String>,
}

impl Master {
    pub fn offers_copy(&self) -> bool {
        self.program == "copy" || self.program == "both"
    }
    pub fn live(&self) -> bool {
        self.status == "approved" || self.status == "suspended"
    }
    /// The master no longer trades for followers (suspended, frozen or removed): followers get the unfollow prompt.
    pub fn stopped(&self) -> bool {
        self.frozen || self.status != "approved"
    }
}

#[derive(Clone, Debug)]
pub struct Sub {
    pub id: i64,
    pub tenant_id: i64,
    pub master_id: i64,
    pub user_id: i64,
    pub login: i64,
    pub sizing: Sizing,
    pub max_lot: Option<D>,
    pub equity_stop: Option<D>,
    pub max_dd_pct: Option<D>,
    pub excluded: Vec<String>,
    /// active | paused | stopped
    pub status: String,
    pub stop_reason: Option<String>,
    pub perf_fee_pct: D,
    pub fee_period: String,
    pub allocation: D,
    pub net_deposits: D,
    pub flows_since_fee: D,
    pub hwm: D,
    pub peak_equity: D,
    pub fees_paid: D,
    pub start_version: i64,
    pub created_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub last_fee_at: Option<DateTime<Utc>>,
    pub next_fee_at: DateTime<Utc>,
    /// Last equity seen by the guard (USD, not persisted).
    pub last_equity: Option<D>,
    pub last_balance: Option<D>,
    pub positions: usize,
    pub orders: usize,
    /// A9: the follower's own stop loss (pips) on every copied trade; the tighter of it and the master's SL wins.
    pub auto_sl_pips: Option<D>,
    /// A8: new terms of the master waiting for the follower's acceptance (until `terms_deadline`).
    pub pending_fee_pct: Option<D>,
    pub pending_fee_period: Option<String>,
    pub terms_deadline: Option<DateTime<Utc>>,
    /// Why the copy is paused by the system (`terms`); None = paused by the follower.
    pub pause_reason: Option<String>,
    /// `master_stopped`: the master was suspended / frozen / removed (the Client Area offers the unfollow wizard).
    pub attention: Option<String>,
    /// A9 trial copy on a demo copy account.
    pub trial: bool,
    pub trial_ends_at: Option<DateTime<Utc>>,
}

impl Sub {
    pub fn copying(&self) -> bool {
        self.status != "stopped"
    }
}

#[derive(Clone, Debug)]
pub struct Fund {
    pub id: i64,
    pub tenant_id: i64,
    pub master_id: i64,
    pub user_id: i64,
    pub login: i64,
    pub name: String,
    pub period: String,
    pub perf_fee_pct: D,
    pub lock_in_days: i64,
    pub min_investment: D,
    pub max_dd_pct: Option<D>,
    pub min_own_pct: D,
    /// active | frozen | closed
    pub status: String,
    pub units: D,
    pub nav_peak: D,
    pub freeze_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_rollover_at: Option<DateTime<Utc>>,
    pub next_rollover_at: DateTime<Utc>,
    /// Last equity seen by the guard (USD, not persisted).
    pub last_equity: Option<D>,
}

impl Fund {
    pub fn nav_now(&self) -> D {
        math::nav(self.last_equity.unwrap_or(ZERO), self.units)
    }
}

#[derive(Default)]
pub struct Reg {
    pub settings: HashMap<i64, Settings>,
    pub masters: BTreeMap<i64, Master>,
    pub subs: BTreeMap<i64, Sub>,
    pub funds: BTreeMap<i64, Fund>,
    /// Per-subscription "still copying" flag, checked inside the follower's shard right before a mirrored
    /// action runs (so a stop that is already queued ahead of it always wins).
    pub flags: HashMap<i64, Arc<AtomicBool>>,
    /// MAM managers and links (see `mam`).
    pub managers: BTreeMap<i64, mam::Manager>,
    pub links: BTreeMap<i64, mam::Link>,
    /// Per-link "still active" flag (same role as `flags` for subscriptions).
    pub link_flags: HashMap<i64, Arc<AtomicBool>>,
}

impl Reg {
    pub fn master_by_login(&self, login: i64) -> Option<&Master> {
        self.masters.values().find(|m| m.login == login && m.live())
    }
    pub fn sub_by_login(&self, login: i64) -> Option<&Sub> {
        self.subs.values().find(|s| s.login == login)
    }
    pub fn fund_by_login(&self, login: i64) -> Option<&Fund> {
        self.funds.values().find(|f| f.login == login)
    }
    pub fn subs_of(&self, master_id: i64) -> Vec<&Sub> {
        self.subs.values().filter(|s| s.master_id == master_id && s.copying()).collect()
    }
    pub fn settings(&self, tenant: i64) -> Settings {
        self.settings.get(&tenant).cloned().unwrap_or_default()
    }
}

/// Handle shared by the API, the copier, the guard and the scheduler.
pub struct Social {
    pub pool: PgPool,
    pub hub: Hub,
    pub wallet: WalletClient,
    pub logins: Arc<LoginAlloc>,
    pub reg: RwLock<Reg>,
    /// Serialises PAMM money movements (requests, rollovers, stop-loss, freeze).
    pub pamm_lock: tokio::sync::Mutex<()>,
    /// Serialises subscription stop / fee settlement.
    pub sub_lock: tokio::sync::Mutex<()>,
    /// Last copier cursor per watched login.
    pub cursors: std::sync::Mutex<HashMap<i64, i64>>,
    /// IB service (D64): PAMM fund deals are allocated to investors by unit share and pushed as lots.
    pub ib: std::sync::OnceLock<WalletClient>,
}

/* ------------------------------------------------------------------ */
/* Periods                                                             */
/* ------------------------------------------------------------------ */

/// The next period end strictly after `ts`, at 00:00 server time: daily = next rollover; weekly = the rollover
/// into Monday; monthly = the rollover into the 1st.
pub fn next_period_end(period: &str, ts: DateTime<Utc>) -> DateTime<Utc> {
    let mut d = server_date(ts).succ_opt().unwrap();
    for _ in 0..40 {
        let ok = match period {
            "weekly" => d.weekday() == Weekday::Mon,
            "monthly" => d.day() == 1,
            _ => true,
        };
        if ok {
            return server_midnight(d);
        }
        d = d.succ_opt().unwrap();
    }
    server_midnight(d)
}

pub fn valid_period(p: &str) -> bool {
    matches!(p, "daily" | "weekly" | "monthly")
}

/* ------------------------------------------------------------------ */
/* Loading / saving                                                    */
/* ------------------------------------------------------------------ */

fn master_from(r: &sqlx::postgres::PgRow) -> Master {
    Master {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        user_id: r.get("user_id"),
        login: r.get("login"),
        nickname: r.get("nickname"),
        strategy: r.get("strategy"),
        description: r.get("description"),
        program: r.get("program"),
        perf_fee_pct: r.get("perf_fee_pct"),
        fee_period: r.get("fee_period"),
        min_allocation: r.get("min_allocation"),
        status: r.get("status"),
        hidden: r.get("hidden"),
        frozen: r.get("frozen"),
        kyc_verified: r.get("kyc_verified"),
        checks: r.get::<sqlx::types::Json<Value>, _>("checks").0,
        review_note: r.get("review_note"),
        reviewed_by: r.get("reviewed_by"),
        created_at: r.get("created_at"),
        approved_at: r.get("approved_at"),
        is_house: r.try_get("is_house").unwrap_or(false),
        max_followers: r.try_get("max_followers").unwrap_or(None),
        accept_new: r.try_get("accept_new").unwrap_or(true),
        invite_only: r.try_get("invite_only").unwrap_or(false),
        invite_code: r.try_get("invite_code").unwrap_or(None),
    }
}

fn sub_from(r: &sqlx::postgres::PgRow) -> Sub {
    Sub {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        master_id: r.get("master_id"),
        user_id: r.get("user_id"),
        login: r.get("login"),
        sizing: Sizing { mode: SizingMode::parse(&r.get::<String, _>("sizing_mode")).unwrap_or(SizingMode::Equity), value: r.get("sizing_value") },
        max_lot: r.get("max_lot"),
        equity_stop: r.get("equity_stop"),
        max_dd_pct: r.get("max_dd_pct"),
        excluded: r.get("excluded_symbols"),
        status: r.get("status"),
        stop_reason: r.get("stop_reason"),
        perf_fee_pct: r.get("perf_fee_pct"),
        fee_period: r.get("fee_period"),
        allocation: r.get("allocation"),
        net_deposits: r.get("net_deposits"),
        flows_since_fee: r.get("flows_since_fee"),
        hwm: r.get("hwm"),
        peak_equity: r.get("peak_equity"),
        fees_paid: r.get("fees_paid"),
        start_version: r.get("start_version"),
        created_at: r.get("created_at"),
        stopped_at: r.get("stopped_at"),
        last_fee_at: r.get("last_fee_at"),
        next_fee_at: r.get("next_fee_at"),
        last_equity: None,
        last_balance: None,
        positions: 0,
        orders: 0,
        auto_sl_pips: r.try_get("auto_sl_pips").unwrap_or(None),
        pending_fee_pct: r.try_get("pending_fee_pct").unwrap_or(None),
        pending_fee_period: r.try_get("pending_fee_period").unwrap_or(None),
        terms_deadline: r.try_get("terms_deadline").unwrap_or(None),
        pause_reason: r.try_get("pause_reason").unwrap_or(None),
        attention: r.try_get("attention").unwrap_or(None),
        trial: r.try_get("trial").unwrap_or(false),
        trial_ends_at: r.try_get("trial_ends_at").unwrap_or(None),
    }
}

fn fund_from(r: &sqlx::postgres::PgRow) -> Fund {
    Fund {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        master_id: r.get("master_id"),
        user_id: r.get("user_id"),
        login: r.get("login"),
        name: r.get("name"),
        period: r.get("period"),
        perf_fee_pct: r.get("perf_fee_pct"),
        lock_in_days: r.get::<i32, _>("lock_in_days") as i64,
        min_investment: r.get("min_investment"),
        max_dd_pct: r.get("max_dd_pct"),
        min_own_pct: r.get("min_own_pct"),
        status: r.get("status"),
        units: r.get("units"),
        nav_peak: r.get("nav_peak"),
        freeze_reason: r.get("freeze_reason"),
        created_at: r.get("created_at"),
        last_rollover_at: r.get("last_rollover_at"),
        next_rollover_at: r.get("next_rollover_at"),
        last_equity: None,
    }
}

fn settings_from(r: &sqlx::postgres::PgRow) -> Settings {
    Settings {
        fee_min_pct: r.get("fee_min_pct"),
        fee_max_pct: r.get("fee_max_pct"),
        platform_cut_pct: r.get("platform_cut_pct"),
        min_track_days: r.get::<i32, _>("min_track_days") as i64,
        min_own_capital_pct: r.get("min_own_capital_pct"),
        min_master_equity: r.get("min_master_equity"),
        min_allocation: r.get("min_allocation"),
        trade_delay_minutes: r.get::<i32, _>("trade_delay_minutes") as i64,
    }
}

pub async fn load(pool: &PgPool) -> anyhow::Result<Reg> {
    let mut reg = Reg::default();
    for r in sqlx::query("SELECT * FROM social_settings").fetch_all(pool).await? {
        reg.settings.insert(r.get("tenant_id"), settings_from(&r));
    }
    for r in sqlx::query("SELECT * FROM social_masters").fetch_all(pool).await? {
        let m = master_from(&r);
        reg.masters.insert(m.id, m);
    }
    for r in sqlx::query("SELECT * FROM copy_subscriptions").fetch_all(pool).await? {
        let s = sub_from(&r);
        reg.flags.insert(s.id, Arc::new(AtomicBool::new(s.copying())));
        reg.subs.insert(s.id, s);
    }
    for r in sqlx::query("SELECT * FROM pamm_funds").fetch_all(pool).await? {
        let f = fund_from(&r);
        reg.funds.insert(f.id, f);
    }
    mam::load_into(pool, &mut reg).await?;
    Ok(reg)
}

impl Social {
    pub async fn new(pool: PgPool, hub: Hub, wallet: WalletClient, logins: Arc<LoginAlloc>) -> anyhow::Result<Arc<Self>> {
        let reg = load(&pool).await?;
        let cursors: HashMap<i64, i64> = sqlx::query_as::<_, (i64, i64)>("SELECT login, version FROM copy_cursors").fetch_all(&pool).await?.into_iter().collect();
        let s = Arc::new(Self { pool, hub, wallet, logins, reg: RwLock::new(reg), pamm_lock: Default::default(), sub_lock: Default::default(), cursors: std::sync::Mutex::new(cursors), ib: Default::default() });
        s.rewatch();
        Ok(s)
    }

    /// Watches every master with a live subscription and every copy account still copying.
    pub fn rewatch(&self) {
        let reg = self.reg.read().unwrap();
        let streams = &self.hub.shared.streams;
        for m in reg.masters.values() {
            if m.live() && !reg.subs_of(m.id).is_empty() {
                streams.watch(m.login);
            } else {
                streams.unwatch(m.login);
            }
        }
        for f in reg.funds.values() {
            if f.status != "closed" {
                streams.watch(f.login);
            }
        }
        for s in reg.subs.values() {
            if s.copying() {
                streams.watch(s.login);
            } else {
                streams.unwatch(s.login);
            }
        }
        for m in reg.managers.values() {
            if m.status != "closed" && reg.links_of(m.id).next().is_some() {
                streams.watch(m.login);
            } else if reg.master_by_login(m.login).is_none() {
                streams.unwatch(m.login);
            }
        }
    }

    pub fn settings(&self, tenant: i64) -> Settings {
        self.reg.read().unwrap().settings(tenant)
    }

    pub fn slug(&self, tenant: i64) -> String {
        self.hub.shared.registry.get(tenant).map(|t| t.slug.clone()).unwrap_or_else(|| "kalks".into())
    }

    pub fn flag(&self, sub: i64) -> Arc<AtomicBool> {
        self.reg.write().unwrap().flags.entry(sub).or_insert_with(|| Arc::new(AtomicBool::new(true))).clone()
    }

    pub async fn save_master(&self, m: &Master) -> anyhow::Result<()> {
        let before = self.reg.read().unwrap().masters.get(&m.id).cloned();
        sqlx::query(
            "UPDATE social_masters SET nickname=$2, strategy=$3, description=$4, program=$5, perf_fee_pct=$6, fee_period=$7, min_allocation=$8,
                status=$9, hidden=$10, frozen=$11, review_note=$12, reviewed_by=$13, approved_at=$14, max_followers=$15, accept_new=$16,
                invite_only=$17, invite_code=$18, updated_at=now() WHERE id=$1",
        )
        .bind(m.id)
        .bind(&m.nickname)
        .bind(&m.strategy)
        .bind(&m.description)
        .bind(&m.program)
        .bind(m.perf_fee_pct)
        .bind(&m.fee_period)
        .bind(m.min_allocation)
        .bind(&m.status)
        .bind(m.hidden)
        .bind(m.frozen)
        .bind(&m.review_note)
        .bind(&m.reviewed_by)
        .bind(m.approved_at)
        .bind(m.max_followers)
        .bind(m.accept_new)
        .bind(m.invite_only)
        .bind(&m.invite_code)
        .execute(&self.pool)
        .await?;
        self.reg.write().unwrap().masters.insert(m.id, m.clone());
        self.rewatch();
        // A8: followers learn when their master stops (or comes back)
        if let Some(b) = before
            && b.approved_at.is_some()
            && b.stopped() != m.stopped()
        {
            self.on_master_stopped(m, m.stopped()).await;
        }
        Ok(())
    }

    pub async fn save_sub(&self, s: &Sub) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE copy_subscriptions SET sizing_mode=$2, sizing_value=$3, max_lot=$4, equity_stop=$5, max_dd_pct=$6, excluded_symbols=$7, status=$8, stop_reason=$9,
                net_deposits=$10, flows_since_fee=$11, hwm=$12, peak_equity=$13, fees_paid=$14, stopped_at=$15, last_fee_at=$16, next_fee_at=$17,
                auto_sl_pips=$18, pending_fee_pct=$19, pending_fee_period=$20, terms_deadline=$21, pause_reason=$22, attention=$23, perf_fee_pct=$24, fee_period=$25,
                trial=$26, trial_ends_at=$27, updated_at=now() WHERE id=$1",
        )
        .bind(s.id)
        .bind(s.sizing.mode.as_str())
        .bind(s.sizing.value)
        .bind(s.max_lot)
        .bind(s.equity_stop)
        .bind(s.max_dd_pct)
        .bind(&s.excluded)
        .bind(&s.status)
        .bind(&s.stop_reason)
        .bind(s.net_deposits)
        .bind(s.flows_since_fee)
        .bind(s.hwm)
        .bind(s.peak_equity)
        .bind(s.fees_paid)
        .bind(s.stopped_at)
        .bind(s.last_fee_at)
        .bind(s.next_fee_at)
        .bind(s.auto_sl_pips)
        .bind(s.pending_fee_pct)
        .bind(&s.pending_fee_period)
        .bind(s.terms_deadline)
        .bind(&s.pause_reason)
        .bind(&s.attention)
        .bind(s.perf_fee_pct)
        .bind(&s.fee_period)
        .bind(s.trial)
        .bind(s.trial_ends_at)
        .execute(&self.pool)
        .await?;
        let mut reg = self.reg.write().unwrap();
        let (le, lb, p, o) = reg.subs.get(&s.id).map(|x| (x.last_equity, x.last_balance, x.positions, x.orders)).unwrap_or((None, None, 0, 0));
        reg.subs.insert(s.id, Sub { last_equity: s.last_equity.or(le), last_balance: s.last_balance.or(lb), positions: p.max(s.positions), orders: o.max(s.orders), ..s.clone() });
        if let Some(f) = reg.flags.get(&s.id) {
            f.store(s.copying(), Ordering::SeqCst);
        }
        drop(reg);
        self.rewatch();
        Ok(())
    }

    pub async fn save_fund(&self, f: &Fund) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE pamm_funds SET name=$2, period=$3, perf_fee_pct=$4, lock_in_days=$5, min_investment=$6, max_dd_pct=$7, status=$8, units=$9, nav_peak=$10,
                freeze_reason=$11, last_rollover_at=$12, next_rollover_at=$13, updated_at=now() WHERE id=$1",
        )
        .bind(f.id)
        .bind(&f.name)
        .bind(&f.period)
        .bind(f.perf_fee_pct)
        .bind(f.lock_in_days as i32)
        .bind(f.min_investment)
        .bind(f.max_dd_pct)
        .bind(&f.status)
        .bind(f.units)
        .bind(f.nav_peak)
        .bind(&f.freeze_reason)
        .bind(f.last_rollover_at)
        .bind(f.next_rollover_at)
        .execute(&self.pool)
        .await?;
        let mut reg = self.reg.write().unwrap();
        let le = reg.funds.get(&f.id).and_then(|x| x.last_equity);
        reg.funds.insert(f.id, Fund { last_equity: f.last_equity.or(le), ..f.clone() });
        Ok(())
    }

    pub async fn set_cursor(&self, tenant: i64, login: i64, version: i64) {
        {
            let mut c = self.cursors.lock().unwrap();
            let cur = c.entry(login).or_insert(0);
            if *cur >= version {
                return;
            }
            *cur = version;
        }
        let r = sqlx::query("INSERT INTO copy_cursors (login, tenant_id, version) VALUES ($1,$2,$3) ON CONFLICT (login) DO UPDATE SET version = GREATEST(copy_cursors.version, EXCLUDED.version), updated_at = now()")
            .bind(login)
            .bind(tenant)
            .bind(version)
            .execute(&self.pool)
            .await;
        if let Err(e) = r {
            tracing::error!(login, version, error = %e, "saving copy cursor failed");
        }
    }

    pub fn cursor(&self, login: i64) -> i64 {
        self.cursors.lock().unwrap().get(&login).copied().unwrap_or(0)
    }

    /* ---------------- engine reads ---------------- */

    /// (equity USD, balance USD, withdrawable USD, positions, orders, version, created_at, group, mode netting?, kind live?)
    pub async fn account_brief(&self, login: i64) -> Option<Brief> {
        let v = self
            .hub
            .read(
                login,
                Box::new(|x| match x {
                    Some((st, env)) => {
                        let m = crate::engine::metrics(env, st);
                        let f = st.account.usd_factor();
                        json!({
                            "equity": (m.equity / f).to_string(), "balance": (m.balance / f).to_string(), "withdrawable": (m.withdrawable() / f).to_string(),
                            "margin": (m.margin / f).to_string(), "freeMargin": (m.free_margin / f).to_string(),
                            "positions": st.positions.len(), "orders": st.orders.len(), "version": st.version, "createdAt": st.account.created_at,
                            "group": st.account.group, "netting": st.account.mode == crate::model::Mode::Netting, "live": st.account.kind == AccountKind::Live,
                            "userId": st.account.user_id, "tenantId": st.account.tenant_id, "status": st.account.status.as_str(),
                            "options": env.group.product == crate::rules::Product::Options,
                        })
                    }
                    None => Value::Null,
                }),
            )
            .await;
        if v.is_null() {
            return None;
        }
        let d = |k: &str| v[k].as_str().and_then(|s| s.parse::<D>().ok()).unwrap_or(ZERO);
        Some(Brief {
            equity: d("equity"),
            balance: d("balance"),
            withdrawable: d("withdrawable"),
            margin: d("margin"),
            free_margin: d("freeMargin"),
            positions: v["positions"].as_u64().unwrap_or(0) as usize,
            orders: v["orders"].as_u64().unwrap_or(0) as usize,
            version: v["version"].as_i64().unwrap_or(0),
            created_at: serde_json::from_value(v["createdAt"].clone()).unwrap_or_else(|_| Utc::now()),
            group: v["group"].as_str().unwrap_or_default().to_string(),
            netting: v["netting"].as_bool().unwrap_or(false),
            live: v["live"].as_bool().unwrap_or(false),
            user_id: v["userId"].as_i64().unwrap_or(0),
            tenant_id: v["tenantId"].as_i64().unwrap_or(0),
            status: v["status"].as_str().unwrap_or_default().to_string(),
            options: v["options"].as_bool().unwrap_or(false),
        })
    }

    /// Opens an engine account owned by `user` (copy account or fund account). Returns (login, password, investor password).
    pub async fn open_account(&self, tenant: i64, user: i64, group: &str, name: &str) -> Result<(i64, String, String), crate::shard::ExecError> {
        let t = self.hub.shared.registry.get(tenant).ok_or_else(|| crate::shard::ExecError::Internal("unknown tenant".into()))?;
        let g = t.groups.get(group).cloned().ok_or_else(|| crate::shard::ExecError::Internal(format!("group {group} is not configured")))?;
        let (pw, inv) = (crate::auth::generate_password(), crate::auth::generate_password());
        let (a, b) = (pw.clone(), inv.clone());
        let (th, ih) = tokio::task::spawn_blocking(move || -> anyhow::Result<(String, String)> { Ok((crate::auth::hash_password(&a)?, crate::auth::hash_password(&b)?)) })
            .await
            .map_err(|e| crate::shard::ExecError::Internal(e.to_string()))?
            .map_err(|e| crate::shard::ExecError::Internal(e.to_string()))?;
        let login = self.logins.live.fetch_add(1, Ordering::SeqCst) + 1;
        let account = Account {
            tenant_id: tenant,
            login,
            user_id: user,
            kind: AccountKind::Live,
            group: g.code.clone(),
            mode: g.mode,
            cent: g.cent,
            leverage: g.default_leverage,
            status: Status::Active,
            name: name.chars().take(80).collect(),
            route_override: None,
            controls: Controls::default(),
            demo: None,
            created_at: Utc::now(),
            lifecycle: None,
        };
        self.hub.open(account, (th, ih), &format!("user:{user}")).await?;
        Ok((login, pw, inv))
    }

    /// `social.*` entry in the audit log (the same table as dealing, shown in the Back Office).
    #[allow(clippy::too_many_arguments)]
    pub async fn audit(&self, tenant: i64, staff: &Staff, action: &str, login: Option<i64>, target: &str, before: Option<Value>, after: Option<Value>, note: &str) -> Value {
        let row = crate::persist::AuditRow {
            tenant_id: tenant,
            at: Utc::now(),
            staff_id: staff.id.clone(),
            staff_name: staff.name.clone(),
            staff_role: staff.role.clone(),
            action: action.to_string(),
            tickets: vec![target.to_string()],
            login,
            symbol: None,
            before,
            after,
            reason_code: "SOC".into(),
            note: note.to_string(),
            flags: vec![],
        };
        match crate::persist::insert_audit(&self.pool, &row).await {
            Ok(id) => crate::persist::audit_json(id, &row),
            Err(e) => {
                tracing::error!(error = %e, action, "social audit write failed");
                Value::Null
            }
        }
    }

    /* ---------------- terminal guards (D70) ---------------- */

    /// Reject a terminal write on an account that is copying: Some((code, message)).
    pub fn terminal_guard(&self, login: i64, opening: bool) -> Option<(&'static str, String)> {
        let reg = self.reg.read().unwrap();
        let s = reg.sub_by_login(login).filter(|s| s.copying())?;
        let master = reg.masters.get(&s.master_id).map(|m| m.nickname.clone()).unwrap_or_else(|| "your master".into());
        Some(if opening {
            ("copy_account", format!("This is a copy account for {master}. Manual trading is disabled while copying. To trade yourself, stop copying in the Client Area (Social → My subscriptions)."))
        } else {
            ("copy_managed", format!("This position is copied from {master}. It closes when the master closes it. To exit, stop copying in the Client Area (Social → My subscriptions)."))
        })
    }

    pub fn is_fund(&self, login: i64) -> bool {
        self.reg.read().unwrap().fund_by_login(login).is_some()
    }
}

/// Snapshot of an engine account for the social layer (all money in USD).
#[derive(Clone, Debug)]
pub struct Brief {
    pub equity: D,
    pub balance: D,
    pub withdrawable: D,
    pub margin: D,
    pub free_margin: D,
    pub positions: usize,
    pub orders: usize,
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub group: String,
    pub netting: bool,
    pub live: bool,
    pub user_id: i64,
    pub tenant_id: i64,
    pub status: String,
    /// An Options account (CFD / Options account split): copy trading, PAMM and MAM trade CFDs only.
    pub options: bool,
}

/* ------------------------------------------------------------------ */
/* Views                                                               */
/* ------------------------------------------------------------------ */

pub fn sizing_json(s: &Sizing) -> Value {
    json!({"mode": s.mode.as_str(), "value": num(s.value)})
}

pub fn sub_json(s: &Sub, master: Option<&Master>, risk: Option<u8>, fees_pending: D) -> Value {
    let equity = s.last_equity.unwrap_or(ZERO);
    let profit = equity - s.net_deposits;
    json!({
        "id": s.id, "masterId": s.master_id,
        "master": master.map(|m| json!({"id": m.id, "nickname": m.nickname, "strategy": m.strategy, "riskScore": risk, "frozen": m.frozen, "status": m.status, "house": m.is_house})),
        "login": s.login, "status": s.status, "stopReason": s.stop_reason, "sizing": sizing_json(&s.sizing),
        "maxLot": num_opt(s.max_lot), "equityStop": num_opt(s.equity_stop), "maxDdPct": num_opt(s.max_dd_pct), "excludedSymbols": s.excluded,
        "perfFeePct": num(s.perf_fee_pct), "feePeriod": s.fee_period, "allocation": num(s.allocation), "netDeposits": num(s.net_deposits),
        "hwm": num(crate::money::r2((s.hwm + s.flows_since_fee).max(ZERO))), "peakEquity": num(s.peak_equity), "feesPaid": num(s.fees_paid), "feesPending": num(fees_pending),
        "balance": num_opt(s.last_balance.map(crate::money::r2)), "equity": num(crate::money::r2(equity)), "profit": num(crate::money::r2(profit)),
        "returnPct": if s.net_deposits > ZERO { num(crate::money::r2(profit / s.net_deposits * D::ONE_HUNDRED)) } else { json!(0) },
        "positions": s.positions, "orders": s.orders, "createdAt": s.created_at, "stoppedAt": s.stopped_at, "nextFeeAt": s.next_fee_at, "lastFeeAt": s.last_fee_at,
        "autoSlPips": num_opt(s.auto_sl_pips), "pauseReason": s.pause_reason, "attention": s.attention,
        "pendingTerms": s.terms_deadline.map(|d| json!({"perfFeePct": num(s.pending_fee_pct.unwrap_or(s.perf_fee_pct)), "feePeriod": s.pending_fee_period.clone().unwrap_or_else(|| s.fee_period.clone()), "deadline": d})),
        "trial": s.trial, "trialEndsAt": s.trial_ends_at,
    })
}

pub fn fund_json(f: &Fund, master: Option<&Master>, investors: i64, master_units: D, owner: bool, returns: Option<(f64, f64)>) -> Value {
    let nav = f.nav_now();
    let equity = f.last_equity.unwrap_or(ZERO);
    let master_share = if f.units > ZERO { master_units / f.units * D::ONE_HUNDRED } else { ZERO };
    let dd = if f.nav_peak > ZERO && nav < f.nav_peak { (D::ONE - nav / f.nav_peak) * D::ONE_HUNDRED } else { ZERO };
    let mut v = json!({
        "id": f.id, "masterId": f.master_id, "master": master.map(|m| json!({"id": m.id, "nickname": m.nickname})), "name": f.name, "status": f.status,
        "period": f.period, "perfFeePct": num(f.perf_fee_pct), "lockInDays": f.lock_in_days, "minInvestment": num(f.min_investment),
        "maxDdPct": num_opt(f.max_dd_pct), "minOwnPct": num(f.min_own_pct), "nav": num(crate::money::rdp(nav, 6)), "units": num(crate::money::rdp(f.units, 4)),
        "equity": num(crate::money::r2(equity)), "aum": num(crate::money::r2((f.units - master_units).max(ZERO) * nav)), "investors": investors,
        "masterSharePct": num(crate::money::r2(master_share)), "navPeak": num(crate::money::rdp(f.nav_peak, 6)), "drawdownPct": num(crate::money::r2(dd)),
        "returnAll": returns.map(|r| r.0).unwrap_or(((nav - D::ONE) * D::ONE_HUNDRED).try_into().unwrap_or(0.0)), "return1m": returns.map(|r| r.1).unwrap_or(0.0),
        "freezeReason": f.freeze_reason, "lastRolloverAt": f.last_rollover_at, "nextRolloverAt": f.next_rollover_at, "createdAt": f.created_at,
    });
    if owner {
        v["login"] = json!(f.login);
    }
    v
}

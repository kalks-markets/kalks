//! MAM (multi-account manager). See README "MAM (multi-account manager)".
//!
//! A manager is an approved social master that runs a MAM programme: one dedicated MAM master account
//! (system group `mam`, opened by the engine for the manager) and any number of linked client accounts. Every
//! opening trade on the master account (market fill, pending order, volume added) is a block: the copier taps
//! the committed master events, allocates the block across the active links (`allocation::allocate`: equity or
//! balance share, multiplier or percent per account, max lot, lot step, minimum lot) and mirrors it into each
//! client account's shard with source `mam` (`mirror::mirror` with `MirrorCfg::mam`). Closes, partial closes,
//! SL/TP changes and cancels follow proportionally, exactly like copy trading.
//!
//! - The client keeps their own live account. They link it with an explicit, signed-in consent to the
//!   manager's terms (text + hash, IP and user agent stored on the link) and can revoke at any time.
//! - The manager has trading authority only: there is no MAM route that moves money, and the engine's own
//!   free-margin rules keep every withdrawal above the margin of open positions.
//! - Per link: max lot per trade and an equity stop (close the MAM trades and stop the link).
//! - Fees per link (the terms the client consented to): performance fee % of the cumulative MAM result
//!   (closed + floating MAM trades) above the high-water mark, plus an optional management fee (% a year of
//!   equity, pro rata), settled per period, debited from the client account and paid to the manager's wallet
//!   after approval (the shared `social_fees` flow, source `mam`).
//! - Every allocation is recorded in `mam_allocations` (the per-account split and what was executed) and every
//!   step in `mam_log`.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::allocation::{self, Method, Plan, Slot};
use super::mirror::{self, LogEntry, MamCfg, MirrorCfg};
use super::pamm::SocErr;
use super::{Master, Reg, Social, next_period_end, valid_period};
use crate::model::{DealEntry, Source, TxnKind};
use crate::money::{D, ZERO, num, num_opt, r2};
use crate::shard::{Committed, ExecError, Op, Staff};
use crate::state::Event;

/// Groups whose accounts can never be linked (system accounts).
pub const SYSTEM_GROUPS: &[&str] = &["copy", "copy-netting", "pamm", "mam"];

/* ------------------------------------------------------------------ */
/* Types                                                               */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
pub struct Manager {
    pub id: i64,
    pub tenant_id: i64,
    pub master_id: i64,
    pub user_id: i64,
    pub login: i64,
    pub name: String,
    pub description: String,
    pub method: Method,
    pub perf_fee_pct: D,
    pub mgmt_fee_pct: D,
    pub fee_period: String,
    pub min_equity: D,
    /// active | frozen | closed
    pub status: String,
    pub freeze_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Manager {
    pub fn open(&self) -> bool {
        self.status != "closed"
    }
}

#[derive(Clone, Debug)]
pub struct Link {
    pub id: i64,
    pub tenant_id: i64,
    pub manager_id: i64,
    pub user_id: i64,
    pub login: i64,
    /// active | revoked | stopped
    pub status: String,
    pub stop_reason: Option<String>,
    pub alloc_value: D,
    pub max_lot: Option<D>,
    pub equity_stop: Option<D>,
    pub perf_fee_pct: D,
    pub mgmt_fee_pct: D,
    pub fee_period: String,
    pub hwm: D,
    pub fees_paid: D,
    pub start_equity: D,
    pub start_version: i64,
    pub consent_hash: String,
    pub consent_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub ended_by: Option<String>,
    pub last_fee_at: Option<DateTime<Utc>>,
    pub next_fee_at: DateTime<Utc>,
    /// Last equity / balance seen by the guard (USD, not persisted).
    pub last_equity: Option<D>,
    pub last_balance: Option<D>,
}

impl Link {
    pub fn active(&self) -> bool {
        self.status == "active"
    }
}

impl Reg {
    /// Active links of a manager.
    pub fn links_of(&self, manager: i64) -> impl Iterator<Item = &Link> {
        self.links.values().filter(move |l| l.manager_id == manager && l.active())
    }
    pub fn manager_by_login(&self, login: i64) -> Option<&Manager> {
        self.managers.values().find(|m| m.login == login && m.open())
    }
    pub fn manager_of_master(&self, master: i64) -> Option<&Manager> {
        self.managers.values().filter(|m| m.master_id == master).max_by_key(|m| (m.open(), m.id))
    }
    pub fn link_by_login(&self, login: i64) -> Option<&Link> {
        self.links.values().find(|l| l.login == login && l.active())
    }
    /// (login, tenant) of the MAM master accounts the copier must follow.
    pub fn mam_watched(&self) -> Vec<(i64, i64)> {
        self.managers.values().filter(|m| m.open() && self.links_of(m.id).next().is_some()).map(|m| (m.login, m.tenant_id)).collect()
    }
}

fn manager_from(r: &sqlx::postgres::PgRow) -> Manager {
    Manager {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        master_id: r.get("master_id"),
        user_id: r.get("user_id"),
        login: r.get("login"),
        name: r.get("name"),
        description: r.get("description"),
        method: Method::parse(&r.get::<String, _>("method")).unwrap_or(Method::Equity),
        perf_fee_pct: r.get("perf_fee_pct"),
        mgmt_fee_pct: r.get("mgmt_fee_pct"),
        fee_period: r.get("fee_period"),
        min_equity: r.get("min_equity"),
        status: r.get("status"),
        freeze_reason: r.get("freeze_reason"),
        created_at: r.get("created_at"),
    }
}

fn link_from(r: &sqlx::postgres::PgRow) -> Link {
    Link {
        id: r.get("id"),
        tenant_id: r.get("tenant_id"),
        manager_id: r.get("manager_id"),
        user_id: r.get("user_id"),
        login: r.get("login"),
        status: r.get("status"),
        stop_reason: r.get("stop_reason"),
        alloc_value: r.get("alloc_value"),
        max_lot: r.get("max_lot"),
        equity_stop: r.get("equity_stop"),
        perf_fee_pct: r.get("perf_fee_pct"),
        mgmt_fee_pct: r.get("mgmt_fee_pct"),
        fee_period: r.get("fee_period"),
        hwm: r.get("hwm"),
        fees_paid: r.get("fees_paid"),
        start_equity: r.get("start_equity"),
        start_version: r.get("start_version"),
        consent_hash: r.get("consent_hash"),
        consent_at: r.get("consent_at"),
        created_at: r.get("created_at"),
        ended_at: r.get("ended_at"),
        ended_by: r.get("ended_by"),
        last_fee_at: r.get("last_fee_at"),
        next_fee_at: r.get("next_fee_at"),
        last_equity: None,
        last_balance: None,
    }
}

pub async fn load_into(pool: &PgPool, reg: &mut Reg) -> anyhow::Result<()> {
    for r in sqlx::query("SELECT * FROM mam_managers").fetch_all(pool).await? {
        let m = manager_from(&r);
        reg.managers.insert(m.id, m);
    }
    for r in sqlx::query("SELECT * FROM mam_links").fetch_all(pool).await? {
        let l = link_from(&r);
        reg.link_flags.insert(l.id, Arc::new(AtomicBool::new(l.active())));
        reg.links.insert(l.id, l);
    }
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Terms                                                               */
/* ------------------------------------------------------------------ */

pub fn method_text(m: Method) -> &'static str {
    match m {
        Method::Equity => "equity share: each linked account receives the block volume × its equity ÷ the total equity of all linked accounts",
        Method::Balance => "balance share: each linked account receives the block volume × its balance ÷ the total balance of all linked accounts",
        Method::Multiplier => "multiplier: each linked account receives the block volume × a multiplier the manager sets for that account (shown in your link)",
        Method::Percent => "percent: each linked account receives a percentage of the block volume that the manager sets for that account (shown in your link)",
    }
}

fn period_text(p: &str) -> &'static str {
    match p {
        "daily" => "daily",
        "weekly" => "weekly (Mondays 00:00 server time)",
        _ => "monthly (the 1st, 00:00 server time)",
    }
}

/// The manager's current terms (what a client consents to) and their hash.
pub fn terms(m: &Manager, nickname: &str, platform_cut_pct: D) -> (String, String) {
    let text = format!(
        "1. I authorise {nickname} (MAM programme \"{}\") to open, modify and close trades on the trading account I link, on my behalf. \
2. Trade sizes are allocated by {}. Each trade is capped at the max lot I set and at the symbol's max lot, rounded down to the lot step, and skipped when it is below the minimum lot. \
3. The manager has no money-movement rights: they cannot deposit, withdraw or transfer funds on my account, and nothing can be withdrawn below the margin of open positions. \
4. Fees: a performance fee of {}% of new gains of the MAM trades on my account above the high-water mark{}, settled {}, debited from my account and paid to the manager after the broker approves it (the broker keeps {}% of fees). \
5. I see every trade on my account. MAM trades are tagged \"MAM\" and are managed by the manager; I can trade my own positions alongside them. \
6. I can revoke this authority at any time in the Client Area. Revoking stops new trades immediately; I choose whether open MAM trades are closed or left for me to manage. Fees due up to that moment are settled.",
        m.name,
        method_text(m.method),
        m.perf_fee_pct.normalize(),
        if m.mgmt_fee_pct > ZERO { format!(" and a management fee of {}% a year of my account equity, charged pro rata", m.mgmt_fee_pct.normalize()) } else { String::new() },
        period_text(&m.fee_period),
        platform_cut_pct.normalize(),
    );
    let hash = Sha256::digest(text.as_bytes()).iter().map(|b| format!("{b:02x}")).collect::<String>();
    (text, hash)
}

/// Last four digits of a login for the manager's view of a client account.
pub fn mask(login: i64) -> String {
    let s = login.to_string();
    format!("••{}", &s[s.len().saturating_sub(4)..])
}

/// MAM trading result of one link (USD): closed MAM trades since the link (price P&L + swap − commission)
/// and the floating P&L of its open MAM positions.
#[derive(Clone, Copy, Debug, Default)]
pub struct MamResult {
    pub realized: D,
    pub floating: D,
    pub positions: usize,
    pub orders: usize,
    pub equity: D,
    pub balance: D,
    pub withdrawable: D,
    pub volume: D,
}

impl MamResult {
    pub fn total(&self) -> D {
        self.realized + self.floating
    }
}

/* ------------------------------------------------------------------ */
/* Social: managers and links                                          */
/* ------------------------------------------------------------------ */

impl Social {
    pub fn manager(&self, id: i64) -> Option<Manager> {
        self.reg.read().unwrap().managers.get(&id).cloned()
    }
    pub fn link(&self, id: i64) -> Option<Link> {
        self.reg.read().unwrap().links.get(&id).cloned()
    }
    pub fn link_flag(&self, id: i64) -> Arc<AtomicBool> {
        self.reg.write().unwrap().link_flags.entry(id).or_insert_with(|| Arc::new(AtomicBool::new(true))).clone()
    }
    fn nickname(&self, master: i64) -> String {
        self.reg.read().unwrap().masters.get(&master).map(|m| m.nickname.clone()).unwrap_or_else(|| "Manager".into())
    }
    pub fn manager_terms(&self, m: &Manager) -> (String, String) {
        terms(m, &self.nickname(m.master_id), self.settings(m.tenant_id).platform_cut_pct)
    }

    pub async fn save_manager(&self, m: &Manager) -> anyhow::Result<()> {
        sqlx::query("UPDATE mam_managers SET name=$2, description=$3, method=$4, perf_fee_pct=$5, mgmt_fee_pct=$6, fee_period=$7, min_equity=$8, status=$9, freeze_reason=$10, updated_at=now() WHERE id=$1")
            .bind(m.id)
            .bind(&m.name)
            .bind(&m.description)
            .bind(m.method.as_str())
            .bind(m.perf_fee_pct)
            .bind(m.mgmt_fee_pct)
            .bind(&m.fee_period)
            .bind(m.min_equity)
            .bind(&m.status)
            .bind(&m.freeze_reason)
            .execute(&self.pool)
            .await?;
        self.reg.write().unwrap().managers.insert(m.id, m.clone());
        self.rewatch();
        Ok(())
    }

    pub async fn save_link(&self, l: &Link) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE mam_links SET status=$2, stop_reason=$3, alloc_value=$4, max_lot=$5, equity_stop=$6, hwm=$7, fees_paid=$8, ended_at=$9, ended_by=$10,
                last_fee_at=$11, next_fee_at=$12, updated_at=now() WHERE id=$1",
        )
        .bind(l.id)
        .bind(&l.status)
        .bind(&l.stop_reason)
        .bind(l.alloc_value)
        .bind(l.max_lot)
        .bind(l.equity_stop)
        .bind(l.hwm)
        .bind(l.fees_paid)
        .bind(l.ended_at)
        .bind(&l.ended_by)
        .bind(l.last_fee_at)
        .bind(l.next_fee_at)
        .execute(&self.pool)
        .await?;
        let mut reg = self.reg.write().unwrap();
        let (le, lb) = reg.links.get(&l.id).map(|x| (x.last_equity, x.last_balance)).unwrap_or((None, None));
        reg.links.insert(l.id, Link { last_equity: l.last_equity.or(le), last_balance: l.last_balance.or(lb), ..l.clone() });
        if let Some(f) = reg.link_flags.get(&l.id) {
            f.store(l.active(), Ordering::SeqCst);
        }
        drop(reg);
        self.rewatch();
        Ok(())
    }

    /// Opens the MAM programme of an approved master: a dedicated MAM master account (group `mam`), optionally
    /// funded from the manager's wallet. Returns the manager and the account credentials (shown once).
    #[allow(clippy::too_many_arguments)]
    pub async fn create_manager(&self, master: &Master, name: &str, description: &str, method: Method, perf: D, mgmt: D, period: &str, min_equity: D, seed: Option<D>) -> Result<(Manager, Value), SocErr> {
        if master.status != "approved" || master.frozen {
            return Err(SocErr::new("not_master", "Only approved masters can run a MAM programme"));
        }
        if self.reg.read().unwrap().managers.values().any(|m| m.master_id == master.id && m.open()) {
            return Err(SocErr { status: 409, code: "exists", message: "You already run a MAM programme".into() });
        }
        if self.reg.read().unwrap().managers.values().any(|m| m.tenant_id == master.tenant_id && m.open() && m.name.eq_ignore_ascii_case(name)) {
            return Err(SocErr { status: 409, code: "exists", message: "This programme name is taken".into() });
        }
        let s = self.settings(master.tenant_id);
        if perf < s.fee_min_pct || perf > s.fee_max_pct {
            return Err(SocErr::new("fee_out_of_range", format!("The performance fee must be between {}% and {}%", s.fee_min_pct.normalize(), s.fee_max_pct.normalize())));
        }
        if mgmt < ZERO || mgmt > D::from(10) {
            return Err(SocErr::new("fee_out_of_range", "The management fee must be between 0% and 10% a year"));
        }
        if !valid_period(period) {
            return Err(SocErr::new("validation", "The fee period must be daily, weekly or monthly"));
        }
        if let Some(sd) = seed
            && (sd <= ZERO || r2(sd) != sd)
        {
            return Err(SocErr::new("validation", "Enter the funding amount (up to 2 decimals)"));
        }
        let (login, pw, inv) = self.open_account(master.tenant_id, master.user_id, "mam", &format!("MAM · {name}")).await.map_err(|e| SocErr::internal(format!("{e:?}")))?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO mam_managers (tenant_id, master_id, user_id, login, name, description, method, perf_fee_pct, mgmt_fee_pct, fee_period, min_equity, status)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,'active') RETURNING id",
        )
        .bind(master.tenant_id)
        .bind(master.id)
        .bind(master.user_id)
        .bind(login)
        .bind(name)
        .bind(description)
        .bind(method.as_str())
        .bind(perf)
        .bind(mgmt)
        .bind(period)
        .bind(min_equity.max(ZERO))
        .fetch_one(&self.pool)
        .await?;
        let row = sqlx::query("SELECT * FROM mam_managers WHERE id = $1").bind(id).fetch_one(&self.pool).await?;
        let m = manager_from(&row);
        self.reg.write().unwrap().managers.insert(id, m.clone());
        let mut funding = Value::Null;
        if let Some(amount) = seed {
            let key = format!("mam:seed:{id}");
            funding = match self.wallet.to_trading(&self.slug(m.tenant_id), &key, m.user_id, login, amount).await {
                Ok(_) => json!({"status": "done", "amount": num(amount)}),
                Err(e) => json!({"status": "failed", "message": format!("Your MAM master account #{login} was opened, but the transfer from your wallet did not go through: {}. Fund it from your wallet before trading.", e.message)}),
            };
        }
        tracing::info!(manager = id, master = master.id, login, "MAM programme created");
        Ok((m, json!({"login": login, "password": pw, "investorPassword": inv, "funding": funding})))
    }

    /// Eligibility of each of the user's accounts for linking to `m` (None = any manager).
    pub async fn link_candidates(&self, tenant: i64, user: i64, m: Option<&Manager>) -> Vec<Value> {
        let logins: Vec<i64> = {
            let idx = self.hub.shared.index.read().unwrap();
            let mut v: Vec<i64> = idx.accounts.iter().filter(|(_, a)| a.tenant_id == tenant && a.user_id == user && a.kind == crate::model::AccountKind::Live && !SYSTEM_GROUPS.contains(&a.group.as_str())).map(|(l, _)| *l).collect();
            v.sort();
            v
        };
        let mut out = Vec::new();
        for login in logins {
            let Some(b) = self.account_brief(login).await else { continue };
            let reason = self.link_block(login, &b, m);
            out.push(json!({"login": login, "group": b.group, "equity": num(r2(b.equity)), "balance": num(r2(b.balance)), "positions": b.positions, "eligible": reason.is_none(), "reason": reason}));
        }
        out
    }

    /// Why `login` cannot be linked (None = it can).
    fn link_block(&self, login: i64, b: &super::Brief, m: Option<&Manager>) -> Option<String> {
        let reg = self.reg.read().unwrap();
        if !b.live {
            return Some("Demo accounts can't be managed".into());
        }
        if b.netting {
            return Some("Netting accounts can't be linked (MAM trades need a hedging account)".into());
        }
        if b.status != "active" {
            return Some(format!("The account is {}", b.status.replace('_', " ")));
        }
        if SYSTEM_GROUPS.contains(&b.group.as_str()) {
            return Some("Copy, PAMM and MAM accounts can't be linked".into());
        }
        if b.options {
            return Some("Options accounts can't be linked: MAM trades CFDs".into());
        }
        if let Some(l) = reg.link_by_login(login) {
            let name = reg.managers.get(&l.manager_id).map(|x| x.name.clone()).unwrap_or_default();
            return Some(format!("Already managed by {name}"));
        }
        if reg.master_by_login(login).is_some() {
            return Some("The account is a copy-trading master account".into());
        }
        if let Some(m) = m {
            if b.equity < m.min_equity {
                return Some(format!("Equity is below the programme minimum of {} USD", m.min_equity.normalize()));
            }
        }
        None
    }

    /// Links a client account to a manager after the signed-in consent to the manager's current terms.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_link(&self, tenant: i64, user: i64, manager_id: i64, login: i64, max_lot: Option<D>, equity_stop: Option<D>, consent_hash: &str, ip: Option<String>, ua: Option<String>) -> Result<Link, SocErr> {
        let m = self.manager(manager_id).filter(|m| m.tenant_id == tenant && m.open()).ok_or_else(|| SocErr::not_found("MAM programme"))?;
        if m.status != "active" {
            return Err(SocErr::new("manager_status", format!("{} is not accepting accounts right now", m.name)));
        }
        if m.user_id == user {
            return Err(SocErr::new("own_programme", "You cannot link an account to your own MAM programme"));
        }
        let (text, hash) = self.manager_terms(&m);
        if consent_hash != hash {
            return Err(SocErr { status: 409, code: "terms_changed", message: "The manager's terms have changed. Please read and accept the current terms.".into() });
        }
        let b = self.account_brief(login).await.filter(|b| b.tenant_id == tenant && b.user_id == user).ok_or_else(|| SocErr::not_found("Account"))?;
        if let Some(why) = self.link_block(login, &b, Some(&m)) {
            return Err(SocErr::new("not_eligible", why));
        }
        if let Some(es) = equity_stop
            && es >= b.equity
        {
            return Err(SocErr::new("validation", format!("The equity stop must be below the account's equity ({} USD)", r2(b.equity).normalize())));
        }
        let mb = self.account_brief(m.login).await.ok_or_else(|| SocErr::internal("MAM master account missing"))?;
        let now = Utc::now();
        let consent = format!("{text}\nAccount: #{login}. Accepted by user {user} at {}.", now.to_rfc3339());
        let default_value = if m.method == Method::Percent { D::ONE_HUNDRED } else { D::ONE };
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO mam_links (tenant_id, manager_id, user_id, login, status, alloc_value, max_lot, equity_stop, perf_fee_pct, mgmt_fee_pct, fee_period,
                start_equity, start_version, consent_terms, consent_hash, consent_ip, consent_ua, consent_at, next_fee_at)
             VALUES ($1,$2,$3,$4,'active',$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18) RETURNING id",
        )
        .bind(tenant)
        .bind(manager_id)
        .bind(user)
        .bind(login)
        .bind(default_value)
        .bind(max_lot)
        .bind(equity_stop)
        .bind(m.perf_fee_pct)
        .bind(m.mgmt_fee_pct)
        .bind(&m.fee_period)
        .bind(r2(b.equity))
        .bind(mb.version)
        .bind(&consent)
        .bind(&hash)
        .bind(ip.map(|s| s.chars().take(64).collect::<String>()))
        .bind(ua.map(|s| s.chars().take(300).collect::<String>()))
        .bind(now)
        .bind(next_period_end(&m.fee_period, now))
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if e.as_database_error().is_some_and(|d| d.is_unique_violation()) {
                SocErr { status: 409, code: "exists", message: "This account is already managed".into() }
            } else {
                SocErr::internal(e)
            }
        })?;
        let row = sqlx::query("SELECT * FROM mam_links WHERE id = $1").bind(id).fetch_one(&self.pool).await?;
        let mut l = link_from(&row);
        l.last_equity = Some(b.equity);
        l.last_balance = Some(b.balance);
        // the master's stream from now on
        self.set_cursor(tenant, m.login, mb.version.max(self.cursor(m.login))).await;
        {
            let mut reg = self.reg.write().unwrap();
            reg.link_flags.insert(id, Arc::new(AtomicBool::new(true)));
            reg.links.insert(id, l.clone());
        }
        self.rewatch();
        tracing::info!(link = id, manager = manager_id, login, user, "MAM account linked");
        Ok(l)
    }

    /// Ends a link (`revoked` by the client, `stopped` by the equity stop, the manager or the risk team): no
    /// more allocations, then optionally closes the MAM trades and settles the fees due so far.
    pub async fn end_link(&self, id: i64, status: &str, reason: &str, by: &str, close: bool) -> anyhow::Result<Value> {
        let _g = self.sub_lock.lock().await;
        let Some(mut l) = self.link(id) else { anyhow::bail!("not found") };
        let was_active = l.active();
        // the flag first: an allocation already queued behind this becomes a no-op
        self.link_flag(id).store(false, Ordering::SeqCst);
        if was_active {
            l.status = status.to_string();
            l.stop_reason = Some(reason.to_string());
            l.ended_at = Some(Utc::now());
            l.ended_by = Some(by.to_string());
            self.save_link(&l).await?;
        }
        let (mut closed, mut failed) = (Vec::new(), Vec::new());
        if close {
            let r = reason.to_string();
            let op: Op = Box::new(move |tx, env| {
                let (d, f) = mirror::close_mam(tx, env, id, &r);
                Ok(json!({"closed": d, "failed": f.iter().map(|(t, e)| json!({"ticket": t, "error": e})).collect::<Vec<_>>()}))
            });
            match self.hub.exec(l.login, "mam", None, "", "", None, op).await {
                Ok(d) => {
                    closed = d.value["closed"].as_array().cloned().unwrap_or_default();
                    failed = d.value["failed"].as_array().cloned().unwrap_or_default();
                }
                Err(e) => failed.push(json!({"ticket": null, "error": format!("{e:?}")})),
            }
        }
        let mut fee = None;
        if was_active {
            drop(_g);
            match self.settle_link(id, Utc::now(), true).await {
                Ok(f) => fee = f,
                Err(e) => tracing::error!(link = id, error = %e, "MAM fee settlement on end failed"),
            }
        }
        tracing::info!(link = id, status, reason, closed = closed.len(), failed = failed.len(), "MAM link ended");
        Ok(json!({"closed": closed, "failed": failed, "fee": num_opt(fee)}))
    }

    /* ---------------- results and fees ---------------- */

    /// MAM trading result of a link (USD) plus the account's equity / balance.
    pub async fn link_result(&self, l: &Link) -> Option<MamResult> {
        let v = self
            .hub
            .read(
                l.login,
                Box::new(|x| match x {
                    Some((st, env)) => {
                        let m = crate::engine::metrics(env, st);
                        let f = st.account.usd_factor();
                        let mut floating = ZERO;
                        let mut volume = ZERO;
                        let mut n = 0usize;
                        for p in st.positions.values().filter(|p| p.source == Source::Mam) {
                            floating += crate::engine::position_floating(env, &st.account, p).unwrap_or(p.swap);
                            volume += p.volume;
                            n += 1;
                        }
                        let orders = st.orders.values().filter(|o| o.source == Source::Mam).count();
                        json!({"floating": (floating / f).to_string(), "factor": f.to_string(), "positions": n, "orders": orders, "volume": volume.to_string(),
                               "equity": (m.equity / f).to_string(), "balance": (m.balance / f).to_string(), "withdrawable": (m.withdrawable() / f).to_string()})
                    }
                    None => Value::Null,
                }),
            )
            .await;
        if v.is_null() {
            return None;
        }
        let d = |k: &str| v[k].as_str().and_then(|s| s.parse::<D>().ok()).unwrap_or(ZERO);
        let factor = d("factor").max(D::ONE);
        let realized: D = sqlx::query_scalar::<_, Option<D>>(
            "SELECT sum(CASE WHEN d.entry = 'in' THEN -d.commission ELSE d.profit + d.swap END) FROM deals d JOIN positions p ON p.ticket = d.position_ticket
             WHERE d.login = $1 AND p.login = $1 AND p.source = 'mam' AND p.open_time >= $2 AND NOT d.reversed",
        )
        .bind(l.login)
        .bind(l.created_at)
        .fetch_one(&self.pool)
        .await
        .ok()
        .flatten()
        .unwrap_or(ZERO);
        Some(MamResult {
            realized: realized / factor,
            floating: d("floating"),
            positions: v["positions"].as_u64().unwrap_or(0) as usize,
            orders: v["orders"].as_u64().unwrap_or(0) as usize,
            equity: d("equity"),
            balance: d("balance"),
            withdrawable: d("withdrawable"),
            volume: d("volume"),
        })
    }

    /// Crystallises the fees of a link for the period that ends `now` (`final_` when the link ends): the
    /// performance fee on the MAM result above the HWM plus the pro-rata management fee, capped at what can
    /// leave the account (free margin). Debited from the client account, recorded pending approval.
    pub async fn settle_link(&self, id: i64, now: DateTime<Utc>, final_: bool) -> anyhow::Result<Option<D>> {
        let _g = self.sub_lock.lock().await;
        let Some(mut l) = self.link(id) else { return Ok(None) };
        let Some(m) = self.manager(l.manager_id) else { return Ok(None) };
        let Some(res) = self.link_result(&l).await else { return Ok(None) };
        let result = r2(res.total());
        let (perf, hwm_after) = allocation::perf_fee(l.perf_fee_pct, l.hwm, result);
        let from = l.last_fee_at.unwrap_or(l.created_at);
        let mgmt = allocation::mgmt_fee(l.mgmt_fee_pct, res.equity, from, now);
        let cap = r2(res.withdrawable.max(ZERO));
        let perf_c = perf.min(cap);
        let mgmt_c = mgmt.min(cap - perf_c);
        let total = perf_c + mgmt_c;
        let mut charged = None;
        if total > ZERO {
            let key = format!("mam:{id}:{}", now.timestamp());
            let (kp, km) = (format!("{key}:perf"), format!("{key}:mgmt"));
            let op: Op = Box::new(move |tx, env| {
                let f = tx.st.account.usd_factor();
                let a = tx.post(env, TxnKind::PerformanceFee, kp.clone(), "balance", "perf_fees", -(perf_c * f), Some(kp.clone()), None, Some("MAM performance fee".into()));
                let b = tx.post(env, TxnKind::PerformanceFee, km.clone(), "balance", "mgmt_fees", -(mgmt_c * f), Some(km.clone()), None, Some("MAM management fee".into()));
                tx.note("balance", format!("MAM fees {} {}", ((perf_c + mgmt_c) * f).normalize(), tx.st.account.ccy()), json!({"performance": num(-(perf_c * f)), "management": num(-(mgmt_c * f)), "txns": [a, b]}));
                Ok(Value::Null)
            });
            match self.hub.exec(l.login, "system", None, "", "", None, op).await {
                Ok(_) | Err(ExecError::Duplicate(_)) => {}
                Err(e) => anyhow::bail!("MAM fee debit failed: {e:?}"),
            }
            let settings = self.settings(l.tenant_id);
            let (cut, to_manager) = super::math::split_fee(total, settings.platform_cut_pct);
            sqlx::query(
                "INSERT INTO social_fees (tenant_id, source, master_id, link_id, payer_user_id, login, ledger_key, amount, platform_cut, master_amount, perf_amount, mgmt_amount,
                    period_start, period_end, hwm_before, hwm_after, equity, status)
                 VALUES ($1,'mam',$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,'pending') ON CONFLICT DO NOTHING",
            )
            .bind(l.tenant_id)
            .bind(m.master_id)
            .bind(id)
            .bind(l.user_id)
            .bind(l.login)
            .bind(&key)
            .bind(total)
            .bind(cut)
            .bind(to_manager)
            .bind(perf_c)
            .bind(mgmt_c)
            .bind(from)
            .bind(now)
            .bind(r2(l.hwm))
            .bind(r2(hwm_after))
            .bind(r2(res.equity))
            .execute(&self.pool)
            .await?;
            l.fees_paid += total;
            charged = Some(total);
            tracing::info!(link = id, perf = %perf_c, mgmt = %mgmt_c, result = %result, hwm = %l.hwm, "MAM fees charged");
        }
        l.hwm = hwm_after;
        l.last_fee_at = Some(now);
        if !final_ {
            l.next_fee_at = next_period_end(&l.fee_period, now);
        }
        self.save_link(&l).await?;
        Ok(charged)
    }

    /// Fee periods that are due (or all active links with `force`). Returns (links settled, fees charged).
    pub async fn mam_run_due(&self, now: DateTime<Utc>, force: bool) -> (usize, usize) {
        let due: Vec<i64> = self.reg.read().unwrap().links.values().filter(|l| l.active() && (force || l.next_fee_at <= now)).map(|l| l.id).collect();
        let mut fees = 0;
        for id in &due {
            match self.settle_link(*id, now, false).await {
                Ok(Some(_)) => fees += 1,
                Ok(None) => {}
                Err(e) => tracing::error!(link = id, error = %e, "MAM fee settlement failed"),
            }
        }
        (due.len(), fees)
    }

    /* ---------------- guard ---------------- */

    /// Equity stop per link; also refreshes the cached equity / balance.
    pub async fn mam_guard(&self) {
        let links: Vec<Link> = self.reg.read().unwrap().links.values().filter(|l| l.active()).cloned().collect();
        for l in links {
            let Some(b) = self.account_brief(l.login).await else { continue };
            {
                let mut reg = self.reg.write().unwrap();
                if let Some(x) = reg.links.get_mut(&l.id) {
                    x.last_equity = Some(b.equity);
                    x.last_balance = Some(b.balance);
                }
            }
            if let Some(stop) = l.equity_stop.filter(|s| *s > ZERO)
                && b.equity <= stop
            {
                tracing::warn!(link = l.id, login = l.login, equity = %b.equity, stop = %stop, "MAM equity stop hit: closing the MAM trades and stopping the link");
                if let Err(e) = self.end_link(l.id, "stopped", "equity_stop", "system", true).await {
                    tracing::error!(link = l.id, error = %e, "MAM protective stop failed");
                }
            }
        }
    }

    /* ---------------- allocation (the copier's MAM side) ---------------- */

    /// One committed transaction of a MAM master account: allocates every opening block across the active
    /// links, mirrors the whole transaction into each linked account and records the allocation audit.
    pub async fn mam_on_commit(&self, c: &Committed, catch_up: bool) {
        let (m, links): (Manager, Vec<Link>) = {
            let reg = self.reg.read().unwrap();
            let Some(m) = reg.manager_by_login(c.login).cloned() else { return };
            let links = reg.links_of(m.id).cloned().collect();
            (m, links)
        };
        if links.is_empty() {
            return;
        }
        let versions: Vec<(i64, &Event)> = c.events.iter().enumerate().map(|(i, e)| (c.first_version + i as i64, e)).collect();
        // opening blocks in this transaction: (version, action, master ticket, symbol, side, block volume)
        let blocks: Vec<(i64, &'static str, i64, String, String, D)> = versions
            .iter()
            .filter_map(|(v, e)| match e {
                Event::PositionOpened { position: p, deal: Some(d) } if d.entry == DealEntry::In => Some((*v, "open", p.ticket, p.symbol.clone(), p.side.as_str().to_string(), p.volume)),
                Event::PositionUpdated { position: p, deal: Some(d), change } if change == "volume_added" && d.entry == DealEntry::In => Some((*v, "add", p.ticket, p.symbol.clone(), p.side.as_str().to_string(), d.volume)),
                Event::OrderPlaced { order: o } => Some((*v, "order", o.ticket, o.symbol.clone(), o.side.as_str().to_string(), o.volume)),
                _ => None,
            })
            .collect();
        let opens = m.status == "active";
        let mut plans: Vec<(i64, &'static str, i64, String, String, D, Plan, HashMap<i64, (D, D)>)> = Vec::new();
        if !blocks.is_empty() && opens {
            let mut slots = Vec::new();
            let mut basis: HashMap<i64, (D, D)> = HashMap::new();
            for l in &links {
                let (eq, bal) = match self.account_brief(l.login).await {
                    Some(b) => (b.equity, b.balance),
                    None => (ZERO, ZERO),
                };
                basis.insert(l.id, (eq, bal));
                slots.push(Slot { link: l.id, equity: eq, balance: bal, value: l.alloc_value, max_lot: l.max_lot });
            }
            for (v, action, ticket, symbol, side, vol) in &blocks {
                let Some(spec) = self.hub.shared.specs.load().get(symbol).cloned() else { continue };
                // a link created after this event gets nothing from it
                let eligible: Vec<Slot> = slots.iter().filter(|s| links.iter().any(|l| l.id == s.link && *v > l.start_version)).cloned().collect();
                let plan = allocation::allocate(m.method, *vol, &eligible, &spec);
                plans.push((*v, action, *ticket, symbol.clone(), side.clone(), *vol, plan, basis.clone()));
            }
        }
        let mut results: HashMap<(i64, i64), (Option<i64>, &'static str, String)> = HashMap::new();
        for l in &links {
            let events: Vec<(i64, Event)> = versions.iter().filter(|(v, _)| *v > l.start_version).map(|(v, e)| (*v, (*e).clone())).collect();
            if events.is_empty() {
                continue;
            }
            let mut volumes = HashMap::new();
            for (v, .., plan, _) in &plans {
                if let Some(a) = plan.allocs.iter().find(|a| a.link == l.id) {
                    volumes.insert(*v, a.volume.ok_or_else(|| reason_text(a.reason)));
                }
            }
            let cfg = MirrorCfg {
                sub_id: l.id,
                sizing: super::math::Sizing { mode: super::math::SizingMode::Multiplier, value: D::ONE },
                max_lot: l.max_lot,
                excluded: vec![],
                master: m.name.clone(),
                opens,
                catch_up,
                master_equity_usd: c.equity_usd,
                mam: Some(MamCfg { volumes }),
                auto_sl_pips: None,
            };
            let flag = self.link_flag(l.id);
            let at = c.at;
            let sink: Arc<std::sync::Mutex<Vec<(i64, LogEntry)>>> = Default::default();
            let out = sink.clone();
            let op: Op = Box::new(move |tx, env| {
                if !flag.load(Ordering::SeqCst) {
                    return Ok(Value::Null);
                }
                let mut log = out.lock().unwrap();
                for (v, ev) in &events {
                    for e in mirror::mirror(tx, env, &cfg, *v, at, ev) {
                        log.push((*v, e));
                    }
                }
                Ok(Value::Null)
            });
            let entries: Vec<(i64, LogEntry)> = match self.hub.exec(l.login, "mam", None, "", "", None, op).await {
                Ok(_) => std::mem::take(&mut *sink.lock().unwrap()),
                Err(e) => {
                    tracing::error!(link = l.id, login = l.login, error = ?e, "MAM allocation failed");
                    vec![(c.first_version, LogEntry::new("allocate", None, "failed", format!("{e:?}")))]
                }
            };
            for (v, e) in entries {
                if matches!(e.action, "open" | "order" | "add") {
                    results.insert((v, l.id), (e.follower_ticket, e.status, e.message.clone()));
                }
                let r = sqlx::query("INSERT INTO mam_log (tenant_id, link_id, master_login, master_version, action, master_ticket, client_ticket, volume, status, message) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
                    .bind(l.tenant_id)
                    .bind(l.id)
                    .bind(c.login)
                    .bind(v)
                    .bind(e.action)
                    .bind(e.master_ticket)
                    .bind(e.follower_ticket)
                    .bind(e.volume)
                    .bind(e.status)
                    .bind(&e.message)
                    .execute(&self.pool)
                    .await;
                if let Err(err) = r {
                    tracing::warn!(error = %err, "MAM log write failed");
                }
            }
        }
        // allocation audit: one row per block with the split and what was executed
        for (v, action, ticket, symbol, side, vol, plan, basis) in plans {
            let details: Vec<Value> = plan
                .allocs
                .iter()
                .map(|a| {
                    let l = links.iter().find(|l| l.id == a.link);
                    let (eq, bal) = basis.get(&a.link).copied().unwrap_or((ZERO, ZERO));
                    let (ticket, status, message) = results.get(&(v, a.link)).cloned().unwrap_or((None, if a.volume.is_some() { "failed" } else { "skipped" }, reason_text(a.reason)));
                    json!({"linkId": a.link, "login": l.map(|l| l.login), "equity": num(r2(eq)), "balance": num(r2(bal)), "value": num_opt(l.map(|l| l.alloc_value)),
                           "maxLot": num_opt(l.and_then(|l| l.max_lot)), "basis": num(a.basis), "raw": num(a.raw), "volume": num_opt(a.volume), "reason": a.reason,
                           "status": status, "ticket": ticket, "message": message})
                })
                .collect();
            let executed: D = plan.allocs.iter().filter(|a| results.get(&(v, a.link)).is_some_and(|r| r.1 == "done")).filter_map(|a| a.volume).sum();
            let r = sqlx::query(
                "INSERT INTO mam_allocations (tenant_id, manager_id, master_login, master_version, master_ticket, action, symbol, side, block_volume, method, allocated, accounts, details)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)",
            )
            .bind(m.tenant_id)
            .bind(m.id)
            .bind(c.login)
            .bind(v)
            .bind(ticket)
            .bind(action)
            .bind(&symbol)
            .bind(&side)
            .bind(vol)
            .bind(m.method.as_str())
            .bind(executed)
            .bind(plan.allocs.iter().filter(|a| a.volume.is_some()).count() as i32)
            .bind(sqlx::types::Json(json!(details)))
            .execute(&self.pool)
            .await;
            if let Err(e) = r {
                tracing::warn!(error = %e, "MAM allocation audit write failed");
            }
        }
    }

    /// What a block of `volume` lots of `symbol` would allocate right now (manager dashboard and terminal).
    pub async fn mam_preview(&self, m: &Manager, symbol: &str, volume: D) -> Result<Value, SocErr> {
        let spec = self.hub.shared.specs.load().get(symbol).cloned().ok_or_else(|| SocErr::new("validation", format!("Unknown symbol {symbol}")))?;
        let links: Vec<Link> = self.reg.read().unwrap().links_of(m.id).cloned().collect();
        let mut slots = Vec::new();
        for l in &links {
            let (eq, bal) = match self.account_brief(l.login).await {
                Some(b) => (b.equity, b.balance),
                None => (ZERO, ZERO),
            };
            slots.push(Slot { link: l.id, equity: eq, balance: bal, value: l.alloc_value, max_lot: l.max_lot });
        }
        let plan = allocation::allocate(m.method, volume, &slots, &spec);
        let rows: Vec<Value> = plan
            .allocs
            .iter()
            .zip(slots.iter())
            .map(|(a, s)| {
                let l = links.iter().find(|l| l.id == a.link);
                json!({"linkId": a.link, "account": l.map(|l| mask(l.login)), "equity": num(r2(s.equity)), "balance": num(r2(s.balance)), "value": num(s.value),
                       "maxLot": num_opt(s.max_lot), "basis": num(a.basis), "raw": num(a.raw), "volume": num_opt(a.volume), "reason": a.reason})
            })
            .collect();
        Ok(json!({"symbol": symbol, "block": num(volume), "method": m.method.as_str(), "lotStep": num(spec.lot_step), "lotMin": num(spec.lot_min),
                  "allocated": num(plan.allocated), "unallocated": num(plan.unallocated), "rows": rows}))
    }

    /* ---------------- terminal guard ---------------- */

    /// A client's terminal write touching MAM trades of an active link: Some((code, message)).
    /// `tickets`: the positions / orders the request touches; `bulk`: a bulk close (any MAM trade blocks it).
    pub async fn mam_terminal_guard(&self, login: i64, tickets: Vec<i64>, bulk: bool) -> Option<(&'static str, String)> {
        let name = {
            let reg = self.reg.read().unwrap();
            let l = reg.link_by_login(login)?;
            reg.managers.get(&l.manager_id).map(|m| m.name.clone()).unwrap_or_else(|| "your manager".into())
        };
        let hit = self
            .hub
            .read(
                login,
                Box::new(move |x| match x {
                    Some((st, _)) => {
                        let mam_pos = |t: &i64| st.positions.get(t).is_some_and(|p| p.source == Source::Mam);
                        let mam_ord = |t: &i64| st.orders.get(t).is_some_and(|o| o.source == Source::Mam);
                        let n = st.positions.values().filter(|p| p.source == Source::Mam).count() + st.orders.values().filter(|o| o.source == Source::Mam).count();
                        json!({"hit": tickets.iter().any(|t| mam_pos(t) || mam_ord(t)), "count": n})
                    }
                    None => json!({"hit": false, "count": 0}),
                }),
            )
            .await;
        let count = hit["count"].as_u64().unwrap_or(0);
        if bulk && count > 0 {
            return Some(("mam_managed", format!("{count} trade{} on this account {} managed by {name} (MAM), so a bulk close is not available. Close your own positions one by one, or revoke the MAM link in the Client Area (Social → Managed accounts).", if count == 1 { "" } else { "s" }, if count == 1 { "is" } else { "are" })));
        }
        if hit["hit"].as_bool() == Some(true) {
            return Some(("mam_managed", format!("This trade is managed by {name} (MAM). It is changed and closed by the manager. To take over, revoke the MAM link in the Client Area (Social → Managed accounts) and keep the trades open.")));
        }
        None
    }

    /* ---------------- views ---------------- */

    pub fn manager_json(&self, m: &Manager, owner: bool) -> Value {
        let reg = self.reg.read().unwrap();
        let links: Vec<&Link> = reg.links_of(m.id).collect();
        let aum: D = links.iter().map(|l| l.last_equity.unwrap_or(l.start_equity).max(ZERO)).sum();
        let master = reg.masters.get(&m.master_id);
        let mut v = json!({
            "id": m.id, "masterId": m.master_id, "nickname": master.map(|x| x.nickname.clone()), "name": m.name, "description": m.description,
            "method": m.method.as_str(), "perfFeePct": num(m.perf_fee_pct), "mgmtFeePct": num(m.mgmt_fee_pct), "feePeriod": m.fee_period, "minEquity": num(m.min_equity),
            "status": m.status, "freezeReason": m.freeze_reason, "createdAt": m.created_at, "accounts": links.len(), "aum": num(r2(aum)),
        });
        if owner {
            v["login"] = json!(m.login);
        }
        v
    }

    /// `view`: client (full login), manager (masked login, no consent details), admin (everything).
    pub async fn link_json(&self, l: &Link, view: &str) -> Value {
        let m = self.manager(l.manager_id);
        let res = self.link_result(l).await.unwrap_or_default();
        let pending: D = sqlx::query_scalar::<_, Option<D>>("SELECT sum(amount) FROM social_fees WHERE link_id = $1 AND status IN ('pending','approved')").bind(l.id).fetch_one(&self.pool).await.ok().flatten().unwrap_or(ZERO);
        let mut v = json!({
            "id": l.id, "managerId": l.manager_id,
            "manager": m.as_ref().map(|m| json!({"id": m.id, "name": m.name, "nickname": self.nickname(m.master_id), "method": m.method.as_str(), "status": m.status})),
            "login": if view == "manager" { json!(mask(l.login)) } else { json!(l.login) },
            "status": l.status, "stopReason": l.stop_reason, "allocValue": num(l.alloc_value), "maxLot": num_opt(l.max_lot), "equityStop": num_opt(l.equity_stop),
            "perfFeePct": num(l.perf_fee_pct), "mgmtFeePct": num(l.mgmt_fee_pct), "feePeriod": l.fee_period,
            "hwm": num(r2(l.hwm)), "feesPaid": num(r2(l.fees_paid)), "feesPending": num(r2(pending)), "startEquity": num(r2(l.start_equity)),
            "equity": num(r2(res.equity)), "balance": num(r2(res.balance)), "mamResult": num(r2(res.total())), "mamRealized": num(r2(res.realized)), "mamFloating": num(r2(res.floating)),
            "mamPositions": res.positions, "mamOrders": res.orders, "mamVolume": num(res.volume),
            "createdAt": l.created_at, "endedAt": l.ended_at, "endedBy": l.ended_by, "lastFeeAt": l.last_fee_at, "nextFeeAt": l.next_fee_at, "consentAt": l.consent_at,
        });
        if view != "manager" {
            v["userId"] = json!(l.user_id);
        }
        if view == "admin" {
            let r = sqlx::query("SELECT consent_ip, consent_ua, consent_hash FROM mam_links WHERE id = $1").bind(l.id).fetch_optional(&self.pool).await.ok().flatten();
            if let Some(r) = r {
                v["consent"] = json!({"ip": r.get::<Option<String>, _>("consent_ip"), "userAgent": r.get::<Option<String>, _>("consent_ua"), "hash": r.get::<String, _>("consent_hash"), "at": l.consent_at});
            }
        }
        if view == "client" {
            v.as_object_mut().unwrap().remove("userId");
        }
        v
    }

    /// Allocation audit rows (`where_` uses $1).
    pub async fn allocations(&self, where_: &str, arg: i64, limit: i64, mask_logins: bool) -> Vec<Value> {
        let q = format!("SELECT * FROM mam_allocations WHERE {where_} ORDER BY id DESC LIMIT {}", limit.clamp(1, 500));
        let rows = sqlx::query(sqlx::AssertSqlSafe(q)).bind(arg).fetch_all(&self.pool).await.unwrap_or_default();
        rows.iter()
            .map(|r| {
                let mut details = r.get::<sqlx::types::Json<Value>, _>("details").0;
                if mask_logins && let Some(a) = details.as_array_mut() {
                    for d in a.iter_mut() {
                        if let Some(l) = d["login"].as_i64() {
                            d["login"] = json!(mask(l));
                        }
                    }
                }
                json!({"id": r.get::<i64, _>("id"), "managerId": r.get::<i64, _>("manager_id"), "masterTicket": r.get::<Option<i64>, _>("master_ticket"), "action": r.get::<String, _>("action"),
                       "symbol": r.get::<String, _>("symbol"), "side": r.get::<String, _>("side"), "block": num(r.get::<D, _>("block_volume")), "method": r.get::<String, _>("method"),
                       "allocated": num(r.get::<D, _>("allocated")), "accounts": r.get::<i32, _>("accounts"), "details": details, "at": r.get::<DateTime<Utc>, _>("at")})
            })
            .collect()
    }

    /// Step log of one link.
    pub async fn mam_log(&self, link: i64, limit: i64) -> Vec<Value> {
        let rows = sqlx::query("SELECT at, action, master_ticket, client_ticket, volume, status, message FROM mam_log WHERE link_id = $1 ORDER BY id DESC LIMIT $2").bind(link).bind(limit).fetch_all(&self.pool).await.unwrap_or_default();
        rows.iter()
            .map(|r| {
                json!({"at": r.get::<DateTime<Utc>, _>("at"), "action": r.get::<String, _>("action"), "masterTicket": r.get::<Option<i64>, _>("master_ticket"),
                       "ticket": r.get::<Option<i64>, _>("client_ticket"), "volume": num_opt(r.get::<Option<D>, _>("volume")), "status": r.get::<String, _>("status"), "message": r.get::<String, _>("message")})
            })
            .collect()
    }

    /// MAM deals of a link (closed trades history, newest first).
    pub async fn mam_deals(&self, l: &Link, limit: i64) -> Vec<Value> {
        let rows = sqlx::query(
            "SELECT d.id, d.position_ticket, d.symbol, d.side, d.entry, d.volume, d.price, d.profit, d.swap, d.commission, d.reason, d.time
             FROM deals d JOIN positions p ON p.ticket = d.position_ticket
             WHERE d.login = $1 AND p.source = 'mam' AND p.open_time >= $2 ORDER BY d.time DESC LIMIT $3",
        )
        .bind(l.login)
        .bind(l.created_at)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default();
        rows.iter()
            .map(|r| {
                json!({"id": r.get::<i64, _>("id"), "positionTicket": r.get::<i64, _>("position_ticket"), "symbol": r.get::<String, _>("symbol"), "side": r.get::<String, _>("side"),
                       "entry": r.get::<String, _>("entry"), "volume": num(r.get::<D, _>("volume")), "price": num(r.get::<D, _>("price")), "profit": num(r.get::<D, _>("profit")),
                       "swap": num(r.get::<D, _>("swap")), "commission": num(r.get::<D, _>("commission")), "reason": r.get::<String, _>("reason"), "time": r.get::<DateTime<Utc>, _>("time")})
            })
            .collect()
    }

    /// Terminal: what the signed-in account is in MAM terms (manager's master account, a linked client account
    /// or neither) with the allocation summary for the manager.
    pub async fn terminal_mam(&self, login: i64, symbol: Option<&str>, volume: Option<D>) -> Value {
        let (mgr, link) = {
            let reg = self.reg.read().unwrap();
            (reg.manager_by_login(login).cloned(), reg.link_by_login(login).cloned())
        };
        if let Some(m) = mgr {
            let links: Vec<Link> = self.reg.read().unwrap().links_of(m.id).cloned().collect();
            let equity: D = links.iter().map(|l| l.last_equity.unwrap_or(l.start_equity)).sum();
            let preview = match symbol {
                Some(s) => self.mam_preview(&m, s, volume.unwrap_or(D::ONE)).await.ok(),
                None => None,
            };
            let recent = self.allocations("manager_id = $1", m.id, 10, true).await;
            return json!({"role": "manager", "manager": self.manager_json(&m, true), "accounts": links.len(), "equity": num(r2(equity)), "preview": preview, "recent": recent});
        }
        if let Some(l) = link {
            let m = self.manager(l.manager_id);
            return json!({"role": "client", "link": {"id": l.id, "since": l.created_at, "maxLot": num_opt(l.max_lot), "equityStop": num_opt(l.equity_stop)},
                          "manager": m.map(|m| json!({"name": m.name, "nickname": self.nickname(m.master_id), "method": m.method.as_str(), "status": m.status}))});
        }
        json!({"role": null})
    }

    /// Back Office: freeze / unfreeze a programme (no new allocations); optionally closes every MAM trade on
    /// the linked accounts.
    pub async fn mam_freeze(&self, id: i64, freeze: bool, close: bool, reason: &str, staff: Option<Staff>) -> anyhow::Result<Value> {
        let Some(mut m) = self.manager(id) else { anyhow::bail!("not found") };
        if m.status == "closed" {
            anyhow::bail!("the programme is closed");
        }
        m.status = if freeze { "frozen".into() } else { "active".into() };
        m.freeze_reason = if freeze { Some(reason.chars().take(300).collect()) } else { None };
        self.save_manager(&m).await?;
        let (mut closed, mut failed) = (Vec::new(), Vec::new());
        if freeze && close {
            let links: Vec<Link> = self.reg.read().unwrap().links_of(id).cloned().collect();
            for l in links {
                let lid = l.id;
                let r = reason.to_string();
                let op: Op = Box::new(move |tx, env| {
                    let (d, f) = mirror::close_mam(tx, env, lid, &format!("emergency stop: {r}"));
                    Ok(json!({"closed": d, "failed": f.iter().map(|(t, e)| json!({"ticket": t, "error": e})).collect::<Vec<_>>()}))
                });
                let actor = staff.as_ref().map(|s| format!("staff:{}", s.id)).unwrap_or_else(|| "system".into());
                match self.hub.exec(l.login, &actor, staff.clone(), "SOC", reason, None, op).await {
                    Ok(d) => {
                        closed.extend(d.value["closed"].as_array().cloned().unwrap_or_default());
                        failed.extend(d.value["failed"].as_array().cloned().unwrap_or_default());
                    }
                    Err(e) => failed.push(json!({"login": l.login, "error": format!("{e:?}")})),
                }
            }
        }
        Ok(json!({"closed": closed, "failed": failed}))
    }

    /// Admin / client link listing helper.
    pub fn links_where(&self, f: impl Fn(&Link) -> bool) -> Vec<Link> {
        let reg = self.reg.read().unwrap();
        let mut v: Vec<Link> = reg.links.values().filter(|l| f(l)).cloned().collect();
        v.sort_by_key(|l| (!l.active(), -l.id));
        v
    }

    pub fn managers_where(&self, f: impl Fn(&Manager) -> bool) -> Vec<Manager> {
        let reg = self.reg.read().unwrap();
        let mut v: Vec<Manager> = reg.managers.values().filter(|m| f(m)).cloned().collect();
        v.sort_by_key(|m| -m.id);
        v
    }

    /// Totals for a manager dashboard: (active links, equity, MAM result, fees pending, fees paid).
    pub async fn manager_totals(&self, m: &Manager) -> Value {
        let links: Vec<Link> = self.reg.read().unwrap().links_of(m.id).cloned().collect();
        let (mut equity, mut result) = (ZERO, ZERO);
        for l in &links {
            if let Some(r) = self.link_result(l).await {
                equity += r.equity;
                result += r.total();
            }
        }
        let fees: BTreeMap<String, D> = sqlx::query_as::<_, (String, D)>("SELECT status, COALESCE(sum(master_amount), 0) FROM social_fees WHERE source = 'mam' AND link_id IN (SELECT id FROM mam_links WHERE manager_id = $1) GROUP BY status")
            .bind(m.id)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .collect();
        let g = |k: &str| fees.get(k).copied().unwrap_or(ZERO);
        json!({"accounts": links.len(), "equity": num(r2(equity)), "mamResult": num(r2(result)), "feesPending": num(r2(g("pending") + g("approved"))), "feesPaid": num(r2(g("paid")))})
    }
}

fn reason_text(r: Option<&str>) -> String {
    match r {
        Some("below_min_lot") => "below the symbol's minimum lot after allocation".into(),
        Some("no_equity") => "no equity to allocate to".into(),
        Some(x) => x.replace('_', " "),
        None => String::new(),
    }
}

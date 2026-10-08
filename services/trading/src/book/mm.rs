//! The Kalks market maker (docs/OPTIONS-EXCHANGE.md §4, decision O49): it quotes every listed vanilla series of
//! every underlying both sides from the model, under the SAME rules as any client.
//!
//! * **Same path.** Quotes go through `entry::mass_quote` → `engine::options_book::enter_mass` → `enter`, the gates,
//!   reservations and matching every order uses (post-only, ephemeral). The MM never takes liquidity, has no
//!   priority (price, then seq), sees only the public book (`md::Top` views) plus its own account, and its quotes
//!   are firm: nothing asks it before a client trades against a quote (no last look).
//! * **Account.** One house account per (tenant, account kind): user `OPTIONS_MM_USER_ID`, group `options-mm` when
//!   the tenant has it (else `standard`), funded with house capital (live: ledger kind `house_capital`; demo: the
//!   demo funding). Its quotes trade at the market-maker tier, 0 / 0 fees.
//! * **Pricing** (per expiry one model context): theo = model price at the smile vol; bid at σ − s, ask at σ + s with
//!   s per tenor bucket (0DTE / ≤ 7 d / ≤ 30 d / longer), at least `minSpreadTicks` apart; vol skewed by
//!   −skewVol × (net vega of the expiry / maxVega), prices shifted by −skewTicksPerContract × inventory; size
//!   `baseSize` scaled by moneyness and shrinking toward the limits; a side that would breach `maxNetDelta`,
//!   `maxGamma`, `maxVega` or `maxContractsPerSeries` is withdrawn; no bid under one tick.
//! * **Refresh**: near-the-money short-dated series every 250 ms, the rest every 2 s when the spot moved, every
//!   series at least every 5 s (theta) and after a snapshot change or an own fill. A series is requoted only when a
//!   side moved by at least max(1 tick, 25 % of the half-spread) or its size changed.
//! * **Pulls everything** of an underlying when its spot is stale for more than 3 s, the snapshot is stale, the
//!   market is closed, a series reaches cut − 1 min (or the close-only cut-off), or the desk pauses it. A 5 s
//!   deadman cancels its quotes if this loop stalls.
//! * **RFQs**: `rfq_quote` prices a combo (summed theos ± a combo spread) and reserves for its worst side.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use super::types::*;
use super::{BookKey, entry};
use crate::api::AppState;
use crate::model::{Account, AccountKind, Controls, DemoCfg, OptRight, OptionTerms, Side, Status};
use crate::money::{D, ZERO};
use crate::options::pricing::{self, Ctx};
use crate::options::snapshot::{MmSettings, OptSnapshot, TradeState, Underlying};
use crate::options::{OptionPricing, dec, f};
use crate::shard::Hub;

/// Quotes pulled when the underlying's raw spot is older than this (docs §4 says 3 s; the production relay has
/// multi-second gaps on quiet pairs, and pulling the whole chain on every gap would empty the book: 10 s).
pub const SPOT_STALE_MS: i64 = 10_000;
/// Deadman of the MM account: its quotes are cancelled when the loop is silent this long.
pub const DEADMAN_MS: i64 = 5_000;
/// Mass-quote lines per account transaction (keeps each shard operation short).
pub const CHUNK: usize = 40;
/// Series requoted per pass and (tenant, kind) at most, nearest to the money first (4 passes a second).
pub const BUDGET: usize = 400;
const NEAR_MS: i64 = 250;
const FAR_MS: i64 = 2_000;
const THETA_MS: i64 = 5_000;

/// A desk pause of the MM (`option_mm_pauses`): scope all | underlying | expiry (`EURUSD:2026-10-09`).
#[derive(Clone, Debug, PartialEq)]
pub struct Pause {
    pub id: i64,
    pub tenant_id: i64,
    pub kind: AccountKind,
    pub scope: String,
    pub target: String,
    pub reason: String,
    pub by: String,
    pub at: DateTime<Utc>,
}

impl Pause {
    pub fn covers(&self, underlying: &str, expiry: Option<chrono::NaiveDate>) -> bool {
        match self.scope.as_str() {
            "all" => true,
            "underlying" => self.target == underlying,
            "expiry" => expiry.is_some_and(|e| self.target == format!("{underlying}:{e}")),
            _ => false,
        }
    }
    pub fn json(&self) -> Value {
        json!({"id": self.id, "scope": self.scope, "target": self.target, "reason": self.reason, "by": self.by, "at": self.at, "kind": self.kind.as_str()})
    }
}

/// Live state of one underlying's quoting.
#[derive(Clone, Debug, Default)]
pub struct UnderStat {
    pub status: String,
    pub series_total: usize,
    /// Series quoted on both sides.
    pub series_quoted: usize,
    pub quotes_live: usize,
    pub inventory: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub withdrawn: Vec<String>,
    pub last_requote_ms: i64,
    pub limits: Option<MmSettings>,
}

/// Live state of the MM of one (tenant, kind).
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub login: Option<i64>,
    pub status: String,
    pub started_ms: i64,
    pub last_quote_ms: i64,
    pub cycles: u64,
    pub degraded_cycles: u64,
    /// In-process latency of mass-quote calls (µs), the last 512.
    pub latency_us: VecDeque<u64>,
    pub under: BTreeMap<String, UnderStat>,
    /// Per underlying: (last full refresh ms, last near refresh ms, spot of the last refresh, snapshot version,
    /// inventory fingerprint).
    pub clock: HashMap<String, (i64, i64, f64, i64, i64)>,
    pub account: Option<Value>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Mm {
    pub runs: Mutex<HashMap<(i64, AccountKind), Run>>,
    pub pauses: Mutex<Vec<Pause>>,
    /// Logins of the MM accounts (also LP: `Books::lp_users` holds the MM user).
    pub logins: Mutex<HashMap<(i64, AccountKind), i64>>,
}

impl Mm {
    pub fn login(&self, tenant_id: i64, kind: AccountKind) -> Option<i64> {
        self.logins.lock().unwrap().get(&(tenant_id, kind)).copied()
    }
    pub fn is_mm_login(&self, login: i64) -> bool {
        self.logins.lock().unwrap().values().any(|l| *l == login)
    }
    pub fn pauses_of(&self, tenant_id: i64, kind: AccountKind) -> Vec<Pause> {
        self.pauses.lock().unwrap().iter().filter(|p| p.tenant_id == tenant_id && p.kind == kind).cloned().collect()
    }
    pub fn run(&self, tenant_id: i64, kind: AccountKind) -> Run {
        self.runs.lock().unwrap().get(&(tenant_id, kind)).cloned().unwrap_or_default()
    }
    fn with_run<R>(&self, tenant_id: i64, kind: AccountKind, f: impl FnOnce(&mut Run) -> R) -> R {
        let mut m = self.runs.lock().unwrap();
        f(m.entry((tenant_id, kind)).or_default())
    }
}

/* ------------------------------------------------------------------ */
/* Account                                                             */
/* ------------------------------------------------------------------ */

/// Loads the MM accounts and the active pauses (start-up, tests).
pub async fn load(st: &AppState) -> anyhow::Result<()> {
    let pool = &st.pool;
    let rows = sqlx::query("SELECT tenant_id, kind, login FROM option_mm_accounts").fetch_all(pool).await?;
    {
        let mut m = st.hub.shared.books.mm.logins.lock().unwrap();
        for r in rows {
            if let Some(k) = AccountKind::parse(&r.get::<String, _>("kind")) {
                m.insert((r.get("tenant_id"), k), r.get("login"));
            }
        }
    }
    reload_pauses(st).await
}

pub async fn reload_pauses(st: &AppState) -> anyhow::Result<()> {
    let rows = sqlx::query("SELECT id, tenant_id, kind, scope, target, reason, staff, created_at FROM option_mm_pauses WHERE lifted_at IS NULL ORDER BY id").fetch_all(&st.pool).await?;
    let v: Vec<Pause> = rows
        .iter()
        .filter_map(|r| {
            Some(Pause {
                id: r.get("id"),
                tenant_id: r.get("tenant_id"),
                kind: AccountKind::parse(&r.get::<String, _>("kind"))?,
                scope: r.get("scope"),
                target: r.get("target"),
                reason: r.get("reason"),
                by: r.get("staff"),
                at: r.get("created_at"),
            })
        })
        .collect();
    *st.hub.shared.books.mm.pauses.lock().unwrap() = v;
    Ok(())
}

/// The MM account of (tenant, kind), opened (with its house capital) on first use.
pub async fn account(st: &AppState, tenant_id: i64, kind: AccountKind) -> anyhow::Result<i64> {
    let books = &st.hub.shared.books;
    if let Some(l) = books.mm.login(tenant_id, kind) {
        return Ok(l);
    }
    let _g = st.open_lock.lock().await;
    if let Some(l) = sqlx::query_scalar::<_, i64>("SELECT login FROM option_mm_accounts WHERE tenant_id = $1 AND kind = $2").bind(tenant_id).bind(kind.as_str()).fetch_optional(&st.pool).await? {
        books.mm.logins.lock().unwrap().insert((tenant_id, kind), l);
        return Ok(l);
    }
    let user = st.cfg.options_mm_user;
    anyhow::ensure!(user > 0, "OPTIONS_MM_USER_ID is not set: the Kalks market maker has no user");
    let t = st.hub.shared.registry.get(tenant_id).ok_or_else(|| anyhow::anyhow!("unknown tenant {tenant_id}"))?;
    let pick = |code: &str| t.groups.get(code).filter(|g| g.allows(kind.as_str()) && !g.cent).map(|g| g.clone());
    // an Options group (CFD / Options account split): `options-mm`, else the broker's first USD Options group
    let any_options = || t.groups.values().filter(|g| g.product == crate::rules::Product::Options && g.allows(kind.as_str()) && !g.cent).min_by(|a, b| a.code.cmp(&b.code)).cloned();
    let g = pick(super::LP_GROUP).or_else(any_options).or_else(|| pick("standard")).ok_or_else(|| anyhow::anyhow!("no USD group for the market-maker account"))?;
    let capital = D::from(st.cfg.options_mm_capital.max(0));
    let (pw, inv) = (crate::auth::generate_password(), crate::auth::generate_password());
    let (th, ih) = tokio::task::spawn_blocking(move || -> anyhow::Result<(String, String)> { Ok((crate::auth::hash_password(&pw)?, crate::auth::hash_password(&inv)?)) }).await??;
    let login = match kind {
        AccountKind::Live => st.logins.live.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
        AccountKind::Demo => st.logins.demo.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1,
    };
    let account = Account {
        tenant_id,
        login,
        user_id: user,
        kind,
        group: g.code.clone(),
        mode: g.mode,
        cent: false,
        leverage: g.default_leverage,
        status: Status::Active,
        name: "Kalks market maker (options order book)".into(),
        route_override: None,
        controls: Controls::default(),
        // demo: the capital is the demo funding; the account never expires and is never refilled
        demo: (kind == AccountKind::Demo).then(|| DemoCfg { initial_balance: capital, refills_per_day: 0, expiry_days: 36_500 }),
        created_at: Utc::now(),
        lifecycle: None,
    };
    st.hub.open(account, (th, ih), "system:options-mm").await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    if kind == AccountKind::Live && capital > ZERO {
        let key = format!("options-mm:{tenant_id}:capital:initial");
        let op: crate::shard::Op = Box::new(move |tx, env| crate::engine::funds::adjust(tx, env, crate::engine::funds::AdjustKind::HouseCapital, capital, &key, "HOUSE", "Options market-maker capital").map(|id| json!({"txn": id})));
        st.hub.exec(login, "system", None, "HOUSE", "options market maker account", None, op).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    }
    sqlx::query("INSERT INTO option_mm_accounts (tenant_id, kind, login, user_id, group_code) VALUES ($1,$2,$3,$4,$5) ON CONFLICT (tenant_id, kind) DO NOTHING")
        .bind(tenant_id)
        .bind(kind.as_str())
        .bind(login)
        .bind(user)
        .bind(&g.code)
        .execute(&st.pool)
        .await?;
    let l: i64 = sqlx::query_scalar("SELECT login FROM option_mm_accounts WHERE tenant_id = $1 AND kind = $2").bind(tenant_id).bind(kind.as_str()).fetch_one(&st.pool).await?;
    books.mm.logins.lock().unwrap().insert((tenant_id, kind), l);
    books.lp_users.write().unwrap().insert(user);
    tracing::info!(tenant = tenant_id, kind = kind.as_str(), login = l, group = %g.code, "options market-maker account opened");
    Ok(l)
}

/* ------------------------------------------------------------------ */
/* Pricing                                                             */
/* ------------------------------------------------------------------ */

/// The MM's own position and Greeks per series, from its account (the only private data it reads).
#[derive(Clone, Debug, Default)]
pub struct Own {
    /// series → signed contracts (book venue).
    pub pos: BTreeMap<String, D>,
    /// series → (side, price, contracts) of its working quotes.
    pub quotes: BTreeMap<String, Vec<(Side, D, D)>>,
    pub equity: D,
    pub balance: D,
    pub margin: D,
    pub free_margin: D,
    /// Firm RFQ quotes whose hold has lapsed (released on the next pass).
    pub lapsed_holds: usize,
}

pub async fn own(hub: &Hub, login: i64) -> Option<Own> {
    let v = hub
        .read(
            login,
            Box::new(|x| {
                let Some((a, env)) = x else { return Value::Null };
                let m = crate::engine::metrics(env, a);
                let pos: BTreeMap<String, String> = {
                    let mut p: BTreeMap<String, D> = BTreeMap::new();
                    for x in a.positions.values().filter(|p| p.on_book()) {
                        *p.entry(x.symbol.clone()).or_default() += x.volume * x.side.sign();
                    }
                    p.into_iter().map(|(k, v)| (k, v.to_string())).collect()
                };
                let quotes: Vec<Value> = a.book.orders.values().filter(|w| w.flags & EPHEMERAL != 0).map(|w| json!([w.series, w.side.as_str(), w.price.to_string(), (D::from(w.left) * w.step).to_string()])).collect();
                let lapsed = a.book.rfq_holds.values().filter(|h| h.1 <= env.now.timestamp_millis()).count();
                json!({"pos": pos, "quotes": quotes, "equity": m.equity.to_string(), "balance": m.balance.to_string(), "margin": m.margin.to_string(), "free": m.free_margin.to_string(), "lapsed": lapsed})
            }),
        )
        .await;
    if v.is_null() {
        return None;
    }
    let p = |k: &str| v[k].as_str().and_then(|s| s.parse::<D>().ok()).unwrap_or(ZERO);
    let mut o = Own { equity: p("equity"), balance: p("balance"), margin: p("margin"), free_margin: p("free"), lapsed_holds: v["lapsed"].as_u64().unwrap_or(0) as usize, ..Default::default() };
    if let Some(m) = v["pos"].as_object() {
        for (k, x) in m {
            o.pos.insert(k.clone(), x.as_str().and_then(|s| s.parse().ok()).unwrap_or(ZERO));
        }
    }
    for q in v["quotes"].as_array().cloned().unwrap_or_default() {
        let (Some(s), Some(side), Some(px), Some(c)) = (q[0].as_str(), q[1].as_str(), q[2].as_str().and_then(|x| x.parse::<D>().ok()), q[3].as_str().and_then(|x| x.parse::<D>().ok())) else { continue };
        let side = if side == "buy" { Side::Buy } else { Side::Sell };
        o.quotes.entry(s.to_string()).or_default().push((side, px, c));
    }
    Some(o)
}

/// One series priced by the MM (ticks / contracts).
#[derive(Clone, Debug, PartialEq)]
pub struct Quote2 {
    pub series: String,
    /// Theo in ticks (rounded).
    pub theo: Ticks,
    pub bid: Option<(Ticks, D)>,
    pub ask: Option<(Ticks, D)>,
    /// Half the model spread in ticks (the requote threshold).
    pub half: f64,
    pub withdrawn: Vec<&'static str>,
    pub near: bool,
    /// Per contract: delta (per unit), gamma (per unit per 1 %), vega USD / vol point, theta USD / day.
    pub greeks: (f64, f64, f64, f64),
}

/// Greeks of the MM per underlying and expiry (delta-weighted contracts, gamma per 1 %, USD vega / theta).
#[derive(Clone, Copy, Debug, Default)]
pub struct Greeks {
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
}

/// Model value per unit at vol `sig` (its effective vol over the vol clock).
fn value(ctx: &Ctx, right: OptRight, k: f64, sig: f64) -> f64 {
    let kind = pricing::kind_of(right);
    if !(ctx.t_cal > 0.0 && ctx.t_vol > 0.0) {
        return (kind.sign() * (ctx.spot - k)).max(0.0);
    }
    optmath::price(kind, ctx.spot, k, ctx.t_cal, ctx.r, ctx.b, optmath::volclock::effective_vol(sig.max(0.0005), ctx.t_vol, ctx.t_cal))
}

/// Per-contract Greeks of a vanilla (same conventions as `options::pricing::vanilla`).
fn greeks(ctx: &Ctx, u: &Underlying, right: OptRight, k: f64, sig: f64) -> (f64, f64, f64, f64) {
    if !(ctx.t_cal > 0.0 && ctx.t_vol > 0.0) {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let kind = pricing::kind_of(right);
    let kf = (ctx.t_vol / ctx.t_cal).sqrt();
    let g = optmath::greeks(kind, ctx.spot, k, ctx.t_cal, ctx.r, ctx.b, sig * kf);
    let usd = u.contract_size * ctx.usd_per_quote;
    let theta = (if ctx.t_cal_1d > 0.0 && ctx.t_vol_1d > 0.0 { optmath::price(kind, ctx.spot, k, ctx.t_cal_1d, ctx.r, ctx.b, optmath::volclock::effective_vol(sig, ctx.t_vol_1d, ctx.t_cal_1d)) } else { (kind.sign() * (ctx.spot - k)).max(0.0) } - value(ctx, right, k, sig)) * usd;
    (g.delta, g.gamma * ctx.spot * 0.01, g.vega * kf * 0.01 * usd, theta)
}

/// Inputs of one quote.
pub struct QuoteIn2<'a> {
    pub ctx: &'a Ctx,
    pub u: &'a Underlying,
    pub s: &'a MmSettings,
    pub right: OptRight,
    pub strike: f64,
    pub tick: f64,
    pub step: f64,
    pub days: f64,
    pub same_day: bool,
    /// MM inventory in this series (contracts, signed).
    pub inventory: f64,
    /// Net vega of the MM in this expiry (USD per vol point).
    pub expiry_vega: f64,
    /// Greeks of the MM on this underlying.
    pub book: Greeks,
}

/// Prices one series (pure: tests run it on fixed inputs).
pub fn quote(series: &str, q: &QuoteIn2) -> Quote2 {
    let s = q.s;
    let sig = pricing::vol_at(q.ctx, q.strike);
    let spread = s.spread_vol(q.days, q.same_day).max(0.0);
    let max_vega = if s.max_vega > 0.0 { s.max_vega } else { f64::INFINITY };
    // long vega in the expiry → lower vols both sides (sell more, buy less)
    let skew = -s.skew_vol * (q.expiry_vega / max_vega).clamp(-1.0, 1.0);
    let theo_v = value(q.ctx, q.right, q.strike, sig);
    let bid_v = value(q.ctx, q.right, q.strike, (sig - spread + skew).max(0.25 * sig).max(0.001));
    let ask_v = value(q.ctx, q.right, q.strike, sig + spread + skew);
    let tick = q.tick;
    let theo = (theo_v / tick).round() as i64;
    let mut bid = (bid_v / tick + 1e-9).floor() as i64;
    let mut ask = (ask_v / tick - 1e-9).ceil() as i64;
    let min_sp = s.min_spread_ticks.max(1);
    if ask - bid < min_sp {
        let need = min_sp - (ask - bid);
        bid -= need / 2;
        ask += need - need / 2;
    }
    // inventory skew: long → lower both prices
    let shift = (-s.skew_ticks_per_contract * q.inventory).round() as i64;
    bid += shift;
    ask += shift;
    let half = ((ask - bid) as f64 / 2.0).max(1.0);
    let g = greeks(q.ctx, q.u, q.right, q.strike, sig);
    // size: moneyness (1 at the money, 0.25 far out) × room left before the limits
    let ad = g.0.abs().min(1.0);
    let money = 0.25 + 0.75 * (1.0 - (2.0 * ad - 1.0).abs());
    let util = [
        if s.max_net_delta > 0.0 { q.book.delta.abs() / s.max_net_delta } else { 0.0 },
        if s.max_gamma > 0.0 { q.book.gamma.abs() / s.max_gamma } else { 0.0 },
        if s.max_vega > 0.0 { q.book.vega.abs() / s.max_vega } else { 0.0 },
        if s.max_contracts_per_series > 0.0 { q.inventory.abs() / s.max_contracts_per_series } else { 0.0 },
    ]
    .into_iter()
    .fold(0.0, f64::max);
    let shrink = (1.0 - util).clamp(0.1, 1.0);
    let step = if q.step > 0.0 { q.step } else { 1.0 };
    let min_c = q.u.min_contracts.max(step);
    let max_c = if q.u.max_contracts > 0.0 { q.u.max_contracts } else { f64::INFINITY };
    let size = ((s.base_size.max(1.0) * money * shrink / step).round() * step).max(min_c).min(max_c);
    // limits: withdraw a side that would push a limit further
    let mut withdrawn = Vec::new();
    let worsens = |now: f64, d: f64, max: f64| max > 0.0 && (now + d).abs() > max && (now + d).abs() > now.abs();
    let mut bid_ok = bid >= 1;
    let mut ask_ok = ask >= 1;
    for (side, sign) in [(Side::Buy, 1.0), (Side::Sell, -1.0)] {
        let dq = sign * size;
        let why = if worsens(q.book.delta, g.0 * dq, s.max_net_delta) {
            Some("delta")
        } else if worsens(q.book.gamma, g.1 * dq, s.max_gamma) {
            Some("gamma")
        } else if worsens(q.book.vega, g.2 * dq, s.max_vega) {
            Some("vega")
        } else if worsens(q.inventory, dq, s.max_contracts_per_series) {
            Some("contracts")
        } else {
            None
        };
        if let Some(w) = why {
            withdrawn.push(w);
            match side {
                Side::Buy => bid_ok = false,
                Side::Sell => ask_ok = false,
            }
        }
    }
    let size = dec(size);
    Quote2 {
        series: series.to_string(),
        theo,
        bid: bid_ok.then_some((bid, size)),
        ask: ask_ok.then_some((ask.max(bid + 1).max(1), size)),
        half,
        withdrawn,
        near: false,
        greeks: g,
    }
}

/// The pricing context of every listed expiry of `u` (the same model as the chain).
fn contexts(snap: &OptSnapshot, u: &Underlying, slug: &str, spot: f64, usdq: f64, now_ms: i64) -> BTreeMap<i64, Ctx> {
    let mut out = BTreeMap::new();
    for e in snap.expiries.iter().filter(|e| e.symbol == u.symbol && e.status == "listed") {
        if let Ok(c) = pricing::context(snap, u, &e.key(), e.cut_at.timestamp_millis(), Some(spot), Some(usdq), now_ms, slug) {
            out.insert(e.id, c);
        }
    }
    out
}

/* ------------------------------------------------------------------ */
/* The loop                                                            */
/* ------------------------------------------------------------------ */

/// Runs forever (single engine instance): one quoting pass per venue every 250 ms.
pub async fn run(st: AppState) {
    if let Err(e) = load(&st).await {
        tracing::warn!(error = %e, "options market maker: accounts / pauses not loaded");
    }
    let mut tick = tokio::time::interval(Duration::from_millis(250));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let venues: Vec<(i64, AccountKind)> = venues_of(&st);
        for (tenant_id, kind) in venues {
            if let Err(e) = pass(&st, tenant_id, kind).await {
                let msg = e.to_string();
                let first = st.hub.shared.books.mm.with_run(tenant_id, kind, |r| {
                    let first = r.error.as_deref() != Some(msg.as_str());
                    r.error = Some(msg.clone());
                    r.status = "stopped".into();
                    first
                });
                if first {
                    tracing::warn!(tenant = tenant_id, kind = kind.as_str(), error = %msg, "options market maker pass failed");
                }
            }
        }
    }
}

pub fn venues_of(st: &AppState) -> Vec<(i64, AccountKind)> {
    let mut v = Vec::new();
    for t in st.hub.shared.registry.all() {
        for k in [AccountKind::Live, AccountKind::Demo] {
            if st.hub.shared.books.venue_enabled(t.tenant_id, k) {
                v.push((t.tenant_id, k));
            }
        }
    }
    v
}

/// Pulls every MM quote of `login` on one underlying (or all of them).
async fn pull(hub: &Hub, login: i64, tenant_id: i64, kind: AccountKind, underlying: Option<&str>, expiry: Option<chrono::NaiveDate>, reason: &str) {
    let at = hub.shared.clock.now().timestamp_millis();
    let keys: Vec<BookKey> = match underlying {
        Some(u) => vec![BookKey::new(tenant_id, kind, u)],
        None => hub.shared.books.handles().into_iter().map(|h| h.key).filter(|k| k.tenant_id == tenant_id && k.kind == kind).collect(),
    };
    for key in keys {
        if hub.shared.books.handle(&key).is_none() {
            continue;
        }
        let _ = entry::call(hub, login, &key, Cmd::CancelAll { login, series: None, expiry, ephemeral_only: true, reason: reason.into(), at }).await;
    }
}

/// One quoting pass for (tenant, kind).
pub async fn pass(st: &AppState, tenant_id: i64, kind: AccountKind) -> anyhow::Result<()> {
    let hub = &st.hub;
    let books = &hub.shared.books;
    let opts = hub.shared.options.clone();
    let now = hub.shared.clock.now();
    let now_ms = now.timestamp_millis();
    let slug = hub.shared.registry.get(tenant_id).map(|t| t.slug.clone()).ok_or_else(|| anyhow::anyhow!("unknown tenant"))?;
    let login = account(st, tenant_id, kind).await?;
    books.heartbeat(login, tenant_id, kind, DEADMAN_MS, now_ms);
    books.mm.with_run(tenant_id, kind, |r| {
        if r.started_ms == 0 {
            r.started_ms = now_ms;
        }
        r.login = Some(login);
        r.cycles += 1;
        r.error = None;
    });
    let Some(snap) = opts.snapshot().filter(|_| !opts.stale(now)) else {
        pull(hub, login, tenant_id, kind, None, None, "mm_snapshot_stale").await;
        books.mm.with_run(tenant_id, kind, |r| r.status = "stale".into());
        return Ok(());
    };
    if !snap.enabled(&slug, kind == AccountKind::Live) {
        pull(hub, login, tenant_id, kind, None, None, "mm_options_off").await;
        books.mm.with_run(tenant_id, kind, |r| r.status = "disabled".into());
        return Ok(());
    }
    let own = own(hub, login).await.ok_or_else(|| anyhow::anyhow!("market-maker account {login} not loaded"))?;
    if own.lapsed_holds > 0 {
        // RFQ quotes nobody accepted: their holds go back
        let op: crate::shard::Op = Box::new(move |tx, _| crate::engine::options_book::release_rfq_holds(tx, now_ms, None));
        let _ = hub.exec(login, "system:options-mm", None, "", "", None, op).await;
    }
    let pauses = books.mm.pauses_of(tenant_id, kind);
    let mut unders: Vec<&Underlying> = snap.underlyings.values().filter(|u| u.enabled && snap.tenant_allows(&slug, &u.symbol)).collect();
    unders.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    // a different underlying first on every pass, so a busy pass never starves the same ones
    let start = books.mm.run(tenant_id, kind).cycles as usize % unders.len().max(1);
    unders.rotate_left(start);
    let mut any_quoting = false;
    let mut any_degraded = false;
    // requotes per pass are bounded (the account's shard stays responsive; the rest follows on the next pass)
    let mut budget = BUDGET;
    for u in unders {
        let sym = u.symbol.clone();
        let mm = snap.mm_for(&slug, kind.as_str(), &sym);
        let key = BookKey::new(tenant_id, kind, &sym);
        let has_quotes = own.quotes.keys().any(|s| s.starts_with(&format!("{sym}-")));
        let set_status = |status: &str| {
            let s = status.to_string();
            books.mm.with_run(tenant_id, kind, |r| {
                let e = r.under.entry(sym.clone()).or_default();
                e.status = s;
                e.quotes_live = 0;
                e.series_quoted = 0;
                e.limits = Some(mm.clone());
            })
        };
        if !mm.enabled {
            if has_quotes {
                pull(hub, login, tenant_id, kind, Some(&sym), None, "mm_disabled").await;
            }
            set_status("disabled");
            continue;
        }
        if pauses.iter().any(|p| p.covers(&sym, None) && p.scope != "expiry") {
            if has_quotes {
                pull(hub, login, tenant_id, kind, Some(&sym), None, "mm_paused").await;
            }
            set_status("paused");
            continue;
        }
        let open = hub.shared.specs.load().get(&sym).is_some_and(|s| s.is_open(now));
        let spot = opts.spot(&sym);
        let usdq = opts.usd_per(&u.quote_ccy);
        let (Some((spot, spot_ms)), Some(usdq), true) = (spot, usdq, open) else {
            if has_quotes {
                pull(hub, login, tenant_id, kind, Some(&sym), None, if open { "mm_no_price" } else { "mm_market_closed" }).await;
            }
            set_status(if open { "stale" } else { "closed" });
            continue;
        };
        if now_ms - spot_ms > SPOT_STALE_MS {
            if has_quotes {
                pull(hub, login, tenant_id, kind, Some(&sym), None, "mm_spot_stale").await;
            }
            set_status("stale");
            any_degraded = true;
            continue;
        }
        // which series are due
        let inv_fp: i64 = own.pos.iter().filter(|(s, _)| s.starts_with(&format!("{sym}-"))).map(|(_, v)| rust_decimal::prelude::ToPrimitive::to_i64(&(*v * D::from(1000))).unwrap_or(0)).fold(0i64, |a, b| a.wrapping_mul(31).wrapping_add(b));
        let (last_full, last_near, last_spot, last_ver, last_inv) = books.mm.run(tenant_id, kind).clock.get(&sym).copied().unwrap_or((0, 0, 0.0, -1, 0));
        let moved = (spot - last_spot).abs() > f64::EPSILON;
        let full = last_ver != snap.version || last_inv != inv_fp || now_ms - last_full >= THETA_MS || (moved && now_ms - last_full >= FAR_MS);
        let near_due = moved && now_ms - last_near >= NEAR_MS;
        if !full && !near_due {
            any_quoting = true;
            continue;
        }
        let (tick, step) = (u.tick(), if u.contract_step > 0.0 { u.contract_step } else { 1.0 });
        if tick <= 0.0 {
            continue;
        }
        let ctxs = contexts(&snap, u, &slug, spot, usdq, now_ms);
        // the MM's own Greeks on this underlying and per expiry (from its positions)
        let mut book_g = Greeks::default();
        let mut exp_vega: HashMap<i64, f64> = HashMap::new();
        let mut series_of: Vec<(&crate::options::snapshot::Series, &crate::options::snapshot::Expiry)> = Vec::new();
        for s in snap.series.values().filter(|s| s.symbol == sym && s.status == "active") {
            if let Some(e) = snap.expiry_by_id(s.expiry_id).filter(|e| e.status == "listed") {
                series_of.push((s, e));
            }
        }
        series_of.sort_by(|a, b| a.0.code.cmp(&b.0.code));
        for (s, e) in &series_of {
            let Some(pos) = own.pos.get(&s.code).copied().filter(|p| !p.is_zero()) else { continue };
            let Some(ctx) = ctxs.get(&e.id) else { continue };
            let Some(right) = OptRight::parse(&s.kind) else { continue };
            let sig = pricing::vol_at(ctx, s.strike);
            let g = greeks(ctx, u, right, s.strike, sig);
            let n = f(pos);
            book_g.delta += g.0 * n;
            book_g.gamma += g.1 * n;
            book_g.vega += g.2 * n;
            book_g.theta += g.3 * n;
            *exp_vega.entry(e.id).or_default() += g.2 * n;
        }
        let top = &opts.top;
        // (priority: pulls first, then nearest to the money and shortest; the line; sides live after it; sides
        // live now)
        let mut cands: Vec<(f64, crate::engine::options_book::QuoteLine, usize, usize)> = Vec::new();
        let mut quoted = 0usize;
        let mut live = 0usize;
        let mut withdrawn: Vec<String> = Vec::new();
        let total = series_of.len();
        let today = crate::specs::server_date(now);
        for (s, e) in &series_of {
            let Some(ctx) = ctxs.get(&e.id) else { continue };
            let Some(right) = OptRight::parse(&s.kind) else { continue };
            let cut = e.cut_at;
            let stop = cut - chrono::Duration::minutes((u.close_only_minutes.max(1)) as i64);
            let state = snap.trade_state(&slug, u, e.expiry_date, cut, Some(&s.code), now);
            let paused = pauses.iter().any(|p| p.covers(&sym, Some(e.expiry_date)));
            let mine = own.quotes.get(&s.code);
            if now >= stop || state != TradeState::Open || paused {
                if let Some(m) = mine {
                    cands.push((-1.0, (s.code.clone(), None, None), 0, m.len()));
                }
                continue;
            }
            let days = (cut - now).num_seconds() as f64 / 86_400.0;
            let near = days <= 7.0 && (s.strike / spot).ln().abs() <= 0.02;
            if !full && !near {
                if mine.is_some() {
                    live += mine.map(|m| m.len()).unwrap_or(0);
                    if mine.is_some_and(|m| m.len() == 2) {
                        quoted += 1;
                    }
                }
                continue;
            }
            let inv = own.pos.get(&s.code).map(|p| f(*p)).unwrap_or(0.0);
            let qi = QuoteIn2 {
                ctx,
                u,
                s: &mm,
                right,
                strike: s.strike,
                tick,
                step,
                days,
                same_day: e.expiry_date == today || days < 1.0,
                inventory: inv,
                expiry_vega: exp_vega.get(&e.id).copied().unwrap_or(0.0),
                book: book_g,
            };
            let mut q = quote(&s.code, &qi);
            // never cross other participants' orders (post-only): stay one tick inside the public book
            if let Some(v) = top.view(&key, &s.code) {
                let mine_px = |side: Side| mine.and_then(|m| m.iter().find(|x| x.0 == side).map(|x| x.1));
                if let (Some(b), Some((ask, _))) = (v.bids.first(), q.ask.as_mut())
                    && mine_px(Side::Buy) != Some(b.0)
                {
                    let bt = (f(b.0) / tick).round() as i64;
                    if *ask <= bt {
                        *ask = bt + 1;
                    }
                }
                if let (Some(a), Some((bid, _))) = (v.asks.first(), q.bid.as_mut())
                    && mine_px(Side::Sell) != Some(a.0)
                {
                    let at = (f(a.0) / tick).round() as i64;
                    if *bid >= at {
                        *bid = at - 1;
                    }
                }
                if q.bid.is_some_and(|b| b.0 < 1) {
                    q.bid = None;
                }
            }
            for w in &q.withdrawn {
                withdrawn.push(format!("{}:{}", s.code, w));
            }
            // requote only when a side moved enough or its size changed
            let th = (0.25 * q.half).max(1.0);
            let tkd = dec(tick);
            let same = |side: Side, new: Option<(Ticks, D)>| -> bool {
                let old = mine.and_then(|m| m.iter().find(|x| x.0 == side)).map(|x| ((f(x.1) / tick).round() as i64, x.2));
                match (old, new) {
                    (None, None) => true,
                    (Some((op, oq)), Some((np, nq))) => ((op - np).abs() as f64) < th && oq == nq,
                    _ => false,
                }
            };
            let n_live = q.bid.is_some() as usize + q.ask.is_some() as usize;
            let n_now = mine.map(|m| m.len()).unwrap_or(0);
            if !(same(Side::Buy, q.bid) && same(Side::Sell, q.ask)) {
                let prio = (s.strike / spot).ln().abs() * (days.max(0.0) + 0.25).sqrt();
                cands.push((prio, (s.code.clone(), q.bid.map(|(p, c)| (D::from(p) * tkd, c)), q.ask.map(|(p, c)| (D::from(p) * tkd, c))), n_live, n_now));
            } else {
                live += n_live;
                if n_live == 2 {
                    quoted += 1;
                }
            }
        }
        cands.sort_by(|a, b| a.0.total_cmp(&b.0));
        let take = cands.len().min(budget);
        budget -= take;
        let complete = take == cands.len();
        for (i, c) in cands.iter().enumerate() {
            let n = if i < take { c.2 } else { c.3 };
            live += n;
            if n == 2 {
                quoted += 1;
            }
        }
        let lines: Vec<crate::engine::options_book::QuoteLine> = cands.into_iter().take(take).map(|c| c.1).collect();
        let mut ok = true;
        for chunk in lines.chunks(CHUNK) {
            let t0 = std::time::Instant::now();
            match entry::mass_quote(hub, login, "system:options-mm", chunk.to_vec()).await {
                Ok(v) => {
                    let us = t0.elapsed().as_micros() as u64;
                    books.mm.with_run(tenant_id, kind, |r| {
                        r.latency_us.push_back(us);
                        while r.latency_us.len() > 512 {
                            r.latency_us.pop_front();
                        }
                        r.last_quote_ms = now_ms;
                    });
                    let refused = v["refused"].as_array().map(|a| a.len()).unwrap_or(0);
                    if refused > 0 {
                        tracing::debug!(underlying = %sym, refused, "market-maker quote sides refused by the entry gates");
                    }
                }
                Err(e) => {
                    ok = false;
                    tracing::debug!(underlying = %sym, error = ?e, "market-maker mass quote failed");
                }
            }
        }
        if !ok {
            any_degraded = true;
        }
        any_quoting = true;
        let inventory: f64 = own.pos.iter().filter(|(s, _)| s.starts_with(&format!("{sym}-"))).map(|(_, v)| f(*v).abs()).sum();
        books.mm.with_run(tenant_id, kind, |r| {
            let c = r.clock.entry(sym.clone()).or_insert((0, 0, 0.0, -1, 0));
            // a pass that ran out of budget leaves the refresh due: the rest goes out on the next pass
            if complete {
                if full {
                    c.0 = now_ms;
                }
                c.1 = now_ms;
                c.2 = spot;
                c.3 = snap.version;
                c.4 = inv_fp;
            }
            let e = r.under.entry(sym.clone()).or_default();
            let limited = !withdrawn.is_empty();
            e.status = if !ok { "degraded".into() } else if limited { "limited".into() } else { "quoting".into() };
            e.series_total = total;
            if full || take > 0 {
                e.series_quoted = quoted;
                e.quotes_live = live;
            }
            e.inventory = inventory;
            e.delta = book_g.delta;
            e.gamma = book_g.gamma;
            e.vega = book_g.vega;
            e.theta = book_g.theta;
            if full {
                e.withdrawn = withdrawn.clone();
            }
            if !lines.is_empty() {
                e.last_requote_ms = now_ms;
            }
            e.limits = Some(mm.clone());
        });
    }
    books.mm.with_run(tenant_id, kind, |r| {
        r.account = Some(json!({"login": login, "equity": crate::money::num(own.equity), "cash": crate::money::num(own.balance), "margin": crate::money::num(own.margin)}));
        if any_degraded {
            r.degraded_cycles += 1;
        }
        r.status = if !pauses.is_empty() && pauses.iter().any(|p| p.scope == "all") {
            "paused".into()
        } else if any_degraded {
            "degraded".into()
        } else if any_quoting {
            "quoting".into()
        } else {
            "closed".into()
        };
    });
    Ok(())
}


/* ------------------------------------------------------------------ */
/* RFQ quotes                                                          */
/* ------------------------------------------------------------------ */

/// The MM's firm quote on a combo (docs §5): legs' theos in ticks, net bid / ask per combo unit. None when a leg
/// cannot be priced (no spot, closed, past its cut, paused). Pure apart from the snapshot and the spot.
pub fn rfq_price(hub: &Hub, tenant_slug: &str, kind: AccountKind, legs: &[(OptionTerms, Side, i64)], now: DateTime<Utc>) -> Option<(Vec<Ticks>, Option<Ticks>, Option<Ticks>)> {
    let opts = &hub.shared.options;
    let snap = opts.snapshot().filter(|_| !opts.stale(now))?;
    let underlying = legs.first()?.0.underlying.clone();
    let u = snap.underlying(&underlying)?;
    let (spot, spot_ms) = opts.spot(&underlying)?;
    if now.timestamp_millis() - spot_ms > SPOT_STALE_MS || !hub.shared.specs.load().get(&underlying).is_some_and(|s| s.is_open(now)) {
        return None;
    }
    let usdq = opts.usd_per(&u.quote_ccy)?;
    let mm = snap.mm_for(tenant_slug, kind.as_str(), &underlying);
    if !mm.enabled {
        return None;
    }
    let tick = u.tick();
    let now_ms = now.timestamp_millis();
    let mut theos = Vec::new();
    let mut half_sum = 0.0;
    let mut net = 0.0;
    for (t, side, ratio) in legs {
        let e = snap.expiry(&t.underlying, t.expiry)?;
        if now >= e.cut_at - chrono::Duration::minutes(u.close_only_minutes.max(1) as i64) {
            return None;
        }
        let ctx = pricing::context(&snap, u, &e.key(), e.cut_at.timestamp_millis(), Some(spot), Some(usdq), now_ms, tenant_slug).ok()?;
        let k = f(t.strike);
        let sig = pricing::vol_at(&ctx, k);
        let days = (e.cut_at - now).num_seconds() as f64 / 86_400.0;
        let sp = mm.spread_vol(days, days < 1.0);
        let theo = value(&ctx, t.right, k, sig) / tick;
        let hi = value(&ctx, t.right, k, sig + sp) / tick;
        let lo = value(&ctx, t.right, k, (sig - sp).max(0.25 * sig).max(0.001)) / tick;
        theos.push(theo.round().max(0.0) as i64);
        half_sum += *ratio as f64 * ((hi - lo) / 2.0).max(1.0);
        net += if *side == Side::Buy { *ratio as f64 * theo } else { -(*ratio as f64) * theo };
    }
    // a combo spread tighter than legging it: 60 % of the summed half-spreads, at least the minimum spread
    let half = (0.6 * half_sum).max(mm.min_spread_ticks.max(1) as f64 / 2.0).max(1.0);
    let bid = (net - half).floor() as i64;
    let ask = (net + half).ceil() as i64;
    Some((theos, Some(bid), Some(ask.max(bid + 1))))
}

/* ------------------------------------------------------------------ */
/* Status (Back Office)                                                */
/* ------------------------------------------------------------------ */

pub fn status_json(hub: &Hub, tenant_id: i64, kind: AccountKind) -> Value {
    let books = &hub.shared.books;
    let r = books.mm.run(tenant_id, kind);
    let now_ms = hub.shared.clock.now().timestamp_millis();
    let mut lat: Vec<u64> = r.latency_us.iter().copied().collect();
    lat.sort_unstable();
    let pct = |q: f64| if lat.is_empty() { 0 } else { lat[((lat.len() - 1) as f64 * q) as usize] };
    let (mut quoted, mut total, mut live) = (0usize, 0usize, 0usize);
    let mut g = Greeks::default();
    let mut unders = Vec::new();
    for (sym, u) in &r.under {
        quoted += u.series_quoted;
        total += u.series_total;
        live += u.quotes_live;
        g.delta += u.delta;
        g.gamma += u.gamma;
        g.vega += u.vega;
        g.theta += u.theta;
        unders.push(json!({
            "symbol": sym, "status": u.status, "coveragePct": if u.series_total > 0 { 100.0 * u.series_quoted as f64 / u.series_total as f64 } else { 0.0 },
            "seriesQuoted": u.series_quoted, "seriesTotal": u.series_total, "inventoryContracts": u.inventory, "netDelta": u.delta, "gamma": u.gamma, "vega": u.vega, "theta": u.theta,
            "limits": u.limits.as_ref().map(|m| json!({"maxNetDelta": m.max_net_delta, "maxGamma": m.max_gamma, "maxVega": m.max_vega, "maxContractsPerSeries": m.max_contracts_per_series})),
            "withdrawnSides": u.withdrawn, "lastRequoteAt": (u.last_requote_ms > 0).then(|| DateTime::from_timestamp_millis(u.last_requote_ms)),
        }));
    }
    let uptime = if r.started_ms > 0 { (now_ms - r.started_ms) / 1000 } else { 0 };
    json!({
        "kind": kind.as_str(),
        "status": if r.status.is_empty() { if books.venue_enabled(tenant_id, kind) { "stopped" } else { "disabled" } } else { r.status.as_str() },
        "startedAt": (r.started_ms > 0).then(|| DateTime::from_timestamp_millis(r.started_ms)),
        "uptimeSecs": uptime,
        "uptimePct": if r.cycles > 0 { Some(100.0 * (r.cycles - r.degraded_cycles.min(r.cycles)) as f64 / r.cycles as f64) } else { None },
        "latency": (!lat.is_empty()).then(|| json!({"p50Us": pct(0.5), "p99Us": pct(0.99)})),
        "coveragePct": if total > 0 { 100.0 * quoted as f64 / total as f64 } else { 0.0 },
        "quotesLive": live,
        "lastQuoteAt": (r.last_quote_ms > 0).then(|| DateTime::from_timestamp_millis(r.last_quote_ms)),
        "account": r.account,
        "greeks": {"delta": g.delta, "gamma": g.gamma, "vega": g.vega, "theta": g.theta},
        "pauses": books.mm.pauses_of(tenant_id, kind).iter().map(Pause::json).collect::<Vec<_>>(),
        "underlyings": unders,
        "error": r.error,
    })
}

/// Coverage of the MM over every listed series of (tenant, kind), percent (enable flow).
pub fn coverage(hub: &Hub, tenant_id: i64, kind: AccountKind) -> f64 {
    let r = hub.shared.books.mm.run(tenant_id, kind);
    let (q, t) = r.under.values().fold((0, 0), |a, u| (a.0 + u.series_quoted, a.1 + u.series_total));
    if t == 0 { 0.0 } else { 100.0 * q as f64 / t as f64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap() -> OptSnapshot {
        OptSnapshot::from_json(json!({
            "version": 1,
            "underlyings": [{"symbol": "EURUSD", "model": "gk", "baseCcy": "EUR", "quoteCcy": "USD", "calendarCodes": ["EUR", "USD"], "contractSize": 10000,
                             "digits": 5, "pipSize": 0.0001, "weekendVolWeight": 0.15, "holidayVolWeight": 0.5, "enabled": true, "premiumTick": 0.00001, "minContracts": 1, "contractStep": 1, "maxContracts": 500}],
            "rates": [{"ccy": "USD", "rate": 0.04}, {"ccy": "EUR", "rate": 0.02}],
            "surfaces": [{"symbol": "EURUSD", "version": 1, "blendWeight": 1.0, "pillars": [{"tenor": "1W", "days": 7, "atm": 0.07, "rr25": -0.002, "bf25": 0.002}, {"tenor": "1M", "days": 30, "atm": 0.075, "rr25": -0.003, "bf25": 0.0025}]}],
            "expiries": [{"id": 1, "symbol": "EURUSD", "expiryDate": "2026-10-09", "cutAt": "2026-10-09T14:00:00Z", "status": "listed"}],
        }))
        .unwrap()
    }

    /// Pricing rules of a quote (pure; fixed model inputs, no market data): bid < theo < ask on ticks, at least
    /// `minSpreadTicks` apart, the inventory skew moves both prices, and a side that would push a limit further is
    /// withdrawn while the side that reduces it stays.
    #[test]
    fn quotes_follow_the_model_the_spread_the_skew_and_the_limits() {
        let s = snap();
        let u = s.underlying("EURUSD").unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-05T12:00:00Z").unwrap().timestamp_millis();
        let cut = chrono::DateTime::parse_from_rfc3339("2026-10-09T14:00:00Z").unwrap().timestamp_millis();
        let ctx = pricing::context(&s, u, "EURUSD:2026-10-09", cut, Some(1.16), Some(1.0), now, "kalks").unwrap();
        let mm = MmSettings::builtin();
        let qi = |inv: f64, book: Greeks| QuoteIn2 { ctx: &ctx, u, s: &mm, right: OptRight::Call, strike: 1.16, tick: 0.00001, step: 1.0, days: 4.0, same_day: false, inventory: inv, expiry_vega: 0.0, book };
        let q = quote("EURUSD-20261009-1.1600-C", &qi(0.0, Greeks::default()));
        let (b, a) = (q.bid.unwrap(), q.ask.unwrap());
        assert!(b.0 < q.theo && q.theo < a.0, "{q:?}");
        assert!(a.0 - b.0 >= mm.min_spread_ticks);
        assert_eq!((b.1, a.1), (D::from(10), D::from(10)), "ATM: the full base size");
        // the model spread: 0.5 vol points each side (≤ 7 days)
        let vb = value(&ctx, OptRight::Call, 1.16, pricing::vol_at(&ctx, 1.16) - 0.005) / 0.00001;
        assert!((b.0 as f64 - vb).abs() <= 1.0, "bid at σ − s: {} vs {vb}", b.0);
        // long 100 contracts: both prices lower by skewTicksPerContract × 100 = 5 ticks
        let l = quote("x", &qi(100.0, Greeks::default()));
        assert_eq!((l.bid.unwrap().0, l.ask.unwrap().0), (b.0 - 5, a.0 - 5));
        // at the delta limit while long delta: no bid (buying a call adds delta), the ask stays
        let g = Greeks { delta: mm.max_net_delta, ..Default::default() };
        let w = quote("x", &qi(0.0, g));
        assert!(w.bid.is_none() && w.ask.is_some() && w.withdrawn == vec!["delta"], "{w:?}");
        // short past the series limit: the ask goes, the bid stays
        let c = quote("x", &qi(-mm.max_contracts_per_series, Greeks::default()));
        assert!(c.ask.is_none() && c.bid.is_some(), "{c:?}");
        // far out of the money: smaller size, no bid under one tick
        let far = QuoteIn2 { strike: 1.40, ..qi(0.0, Greeks::default()) };
        let f = quote("x", &far);
        assert!(f.bid.is_none() || f.bid.unwrap().0 >= 1);
        assert!(f.ask.unwrap().1 < D::from(10), "{f:?}");
    }

    /// The MM reads only the public book and its own account: never another account's orders or the actor state.
    #[test]
    fn the_market_maker_reads_only_the_public_book_and_its_own_account() {
        let src = include_str!("mm.rs");
        let code: String = src.split("#[cfg(test)]").next().unwrap().lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        for word in [".read(Box::new(|b|", "orders_of(", "hub.scan(", ".scan(", "UnderlyingBooks", "sb.orders", "journal::", "matching::"] {
            assert!(!code.contains(word), "mm.rs uses {word}");
        }
        // it enters quotes through the same path as every client
        assert!(code.contains("entry::mass_quote("), "mm.rs must quote through entry::mass_quote");
        assert!(!code.contains("Cmd::MassQuote"), "mm.rs must not build book commands itself");
    }
}

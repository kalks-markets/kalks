//! Kalks FX Options in the engine core (pure: reads `Env`, emits events through `Tx`).
//!
//! **Money model.** European, cash-settled in USD, B-book (the house takes the other side).
//! * Premium is paid in full in cash when a position opens: a buy debits the balance, a sell credits it
//!   (`option_premium`, against `house:options_premium`). Commission is charged on every trade (open and close):
//!   `min(perContract * contracts, capPct % * premium)` (group settings of the snapshot), against
//!   `house:commission`.
//! * Equity includes the options at their model-mid mark (long +, short −). `Position.premium` keeps the premium
//!   cash of the remaining contracts, so unrealised P&L = value + premium and realised P&L = cash at the exit +
//!   the premium share.
//! * Closing sells a long at the bid / buys a short back at the ask. Expiry pays the payoff at the fixing
//!   (`option_settlement`, key `settle:{SYMBOL:DATE}:{run}:{ticket}`): a house-venue position against
//!   `house:options_settlement`, an order book position against its expiry's clearing account in USD
//!   (`house:options_clearing.{U}.{YYYYMMDD}:USD`; cent accounts through `house:fx`, like the fills;
//!   docs/OPTIONS-EXCHANGE.md §9), so the clearing account nets to 0 per ledger code across USD and cent accounts;
//!   a knock-out pays its rebate at the hit (key `knock:{ticket}`).
//! * Margin = CFD margin + per underlying the scenario (SPAN-like) worst loss of the option legs, as far as the
//!   same-underlying CFD exposure does not already cover it (offsets only ever reduce option margin), times the
//!   weekend add-on on Fridays. Long options carry no margin (an underlying without short options has none).
//! * Cash only: premium debits must fit the cash that is free of margin and not made of credit / bonus, and the
//!   margin of new short exposure must be covered by own funds.
//!
//! **Orders.** Every order opens new positions (one per leg, never netted against an existing position); legs of
//! a multi-leg order fill in one transaction (all or nothing) and share a `combo_id`. Fills are always priced
//! fresh from the raw mid and the snapshot; marks (valuation and margin) may be up to 250 ms old.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::trade::{DealerCtx, apply_nbp, gate};
use super::{Env, Metrics, Reject, Tx, metrics};
use crate::model::{
    Account, AccountKind, BarrierKind, BarrierTerms, Book, Deal, DealEntry, DealOption, DealReason, Expiry, LedgerTxn, OptLeg, OptRight, OptionOrder, OptionTerms, Order, OrderStatus, OrderType, Position, Posting,
    RouteEvent, Side, Source, Trigger, TxnKind, acct_code, house_code,
};
use crate::money::{D, HUNDRED, ONE, ZERO, num, r2, rdp};
use crate::options::snapshot::{OptSnapshot, Underlying};
use crate::options::{OptPrice, PriceError, ScenLeg, TradeState, dec, f, weekend_margin};
use crate::rules::Product;
use crate::state::{AccountState, Event};

/// Most legs in one order (iron condor = 4; the builder allows custom strategies).
pub const MAX_LEGS: usize = 8;

/// `not_eligible`: the client has not accepted the options intro yet (gateway suitability).
pub const NOT_ELIGIBLE: &str = "One quick step: read the 1-minute options intro in the Client Area (Options)";

/// House accounts of the options book.
pub const HOUSE_PREMIUM: &str = "options_premium";
pub const HOUSE_SETTLEMENT: &str = "options_settlement";

/// The other side of an option cash flow.
#[derive(Clone, Copy, Debug)]
pub enum Counter<'a> {
    /// A house account in the account currency (`house:{name}:{ccy}`): the house-priced B-book.
    House(&'a str),
    /// An order book position: the expiry's clearing account in USD (docs §9), cent accounts through `house:fx`.
    Clearing(&'a OptionTerms),
}

/// Books `amount` (account currency, + = to the client) against the expiry's clearing account in USD: 2 legs for a
/// USD account, the 4-leg form through `house:fx:USC/USD` for a cent account (as `options_book::apply_fill` does
/// for the premium). Each currency balances on its own; the clearing leg is `amount / usd_factor`.
#[allow(clippy::too_many_arguments)]
pub fn post_clearing(tx: &mut Tx, env: &Env, kind: TxnKind, idem: String, terms: &OptionTerms, amount: D, reference: Option<String>, reason_code: Option<String>, note: Option<String>) -> Option<i64> {
    let amount = r2(amount);
    if amount.is_zero() {
        return None;
    }
    let acc = &tx.st.account;
    let (login, ccy) = (acc.login, acc.ccy());
    let usd = amount / acc.usd_factor();
    let mut postings = vec![Posting { account: acct_code(login, "balance"), ccy: ccy.into(), amount }];
    if acc.cent {
        postings.push(Posting { account: house_code("fx", "USC"), ccy: "USC".into(), amount: -amount });
        postings.push(Posting { account: house_code("fx", "USD"), ccy: "USD".into(), amount: usd });
    }
    postings.push(Posting { account: house_code(&super::options_book::clearing_name(terms), "USD"), ccy: "USD".into(), amount: -usd });
    let txn = LedgerTxn { id: env.ids.txn(), tenant_id: acc.tenant_id, idempotency_key: idem, kind, login, reference, reason_code, note, at: env.now, postings };
    assert!(txn.is_balanced(), "unbalanced clearing transaction");
    let id = txn.id;
    tx.emit(Event::Ledger { txn });
    Some(id)
}

fn rej(code: &'static str, message: impl Into<String>) -> Reject {
    Reject::new(code, message)
}

fn price_err(e: PriceError) -> Reject {
    match e {
        PriceError::UnknownUnderlying(u) => rej("options_disabled", format!("{u} options are not available")),
        PriceError::NoSpot => rej("stale_prices", "There is no live price for the underlying right now"),
        PriceError::NoVol => rej("stale_prices", "Option prices are unavailable right now"),
        PriceError::NoUsdRate(c) => rej("stale_prices", format!("There is no USD conversion rate for {c} right now")),
    }
}

pub fn snapshot(env: &Env) -> Result<Arc<OptSnapshot>, Reject> {
    env.options.snapshot().ok_or_else(|| rej("options_disabled", "Kalks FX Options are not available right now"))
}

/* ------------------------------------------------------------------ */
/* Valuation                                                           */
/* ------------------------------------------------------------------ */

/// Quote currency → USD: the raw mids first, then the account's CFD quotes.
pub fn usd_per_quote(env: &Env, acc: &Account, ccy: &str) -> Option<D> {
    if ccy == "USD" {
        return Some(ONE);
    }
    env.options.usd_per(ccy).map(dec).filter(|x| *x > ZERO).or_else(|| env.to_usd(acc, ccy, ONE, ("", ZERO)).filter(|x| *x > ZERO))
}

/// Raw mid of a symbol (the account's CFD quote mid when the raw feed has none).
pub fn spot_of(env: &Env, acc: &Account, symbol: &str) -> Option<D> {
    env.options.spot(symbol).map(|s| dec(s.0)).filter(|x| *x > ZERO).or_else(|| env.quote(acc, symbol).map(|q| q.mid()))
}

/// The fixing of a position's expiry once the options service has fixed it.
pub fn fixing_of(env: &Env, t: &OptionTerms) -> Option<D> {
    let snap = env.options.snapshot()?;
    let e = snap.expiry(&t.underlying, t.expiry)?;
    e.fixing.filter(|_| e.status == "fixed" || e.fixed_at.is_some()).map(dec).filter(|x| *x > ZERO)
}

/// USD per quote at a fixing: USD-quoted = 1, `USDXXX` underlyings = 1 / fixing, otherwise the current rate.
pub fn usd_per_quote_at(env: &Env, acc: &Account, t: &OptionTerms, fixing: D) -> Option<D> {
    if t.quote_ccy == "USD" {
        return Some(ONE);
    }
    if t.underlying == format!("USD{}", t.quote_ccy) && fixing > ZERO {
        return Some(ONE / fixing);
    }
    usd_per_quote(env, acc, &t.quote_ccy)
}

/// The quote → USD rate of a settlement: 1 for a USD-quoted underlying, 1 / fixing for USDxxx; for a cross the
/// rate the settlement pass fixed for the whole expiry (`usdq`, settle.rs `conversion`: the conversion pair's own
/// fixing of the same expiry, else the live mid when the pass started), so every account of the expiry converts
/// at one rate and a book expiry's clearing nets; the live mid only without one. None = no rate at all: the
/// position waits for the next pass instead of being paid a quote-currency amount as if it were USD.
pub fn settle_usd_per_quote(env: &Env, acc: &Account, t: &OptionTerms, fixing: D, usdq: Option<D>) -> Option<D> {
    if t.quote_ccy == "USD" {
        return Some(ONE);
    }
    if t.underlying == format!("USD{}", t.quote_ccy) && fixing > ZERO {
        return Some(ONE / fixing);
    }
    usdq.filter(|q| *q > ZERO).or_else(|| usd_per_quote(env, acc, &t.quote_ccy))
}

/// Signed units of the underlying a position holds (+ long).
pub fn units(p: &Position, t: &OptionTerms) -> D {
    p.volume * t.contract_size * p.side.sign()
}

/// The mark of a contract for an account: the cached model price with its `mark` clamped inside the account
/// kind's order book (docs/OPTIONS-EXCHANGE.md §6; vanilla series only, model mid without a book).
pub fn mark_of(env: &Env, acc: &Account, t: &OptionTerms) -> Option<OptPrice> {
    let mut q = env.options.mark(&env.tenant.slug, &env.group.code, t, env.now)?;
    if t.barrier.is_none()
        && let Some(top) = env.options.book_top(&env.tenant.slug, acc.kind, &t.series)
        && let Some(snap) = env.options.snapshot()
        && let Some(u) = snap.underlying(&t.underlying)
    {
        q.mark = super::options_book::clamp(q.mark, q.ask - q.bid, Some(top), u);
    }
    Some(q)
}

/// Signed market value (account currency) of an option position at its mark. Never drops a position: after the
/// cut it is the payoff at the fixing; without a model price the intrinsic value at the last spot, else the
/// premium it was opened at.
pub fn position_value(env: &Env, acc: &Account, p: &Position, t: &OptionTerms) -> D {
    let factor = acc.usd_factor();
    let u = units(p, t);
    if env.now >= t.expiry_at
        && let Some(fx) = fixing_of(env, t)
        && let Some(q) = usd_per_quote_at(env, acc, t, fx)
    {
        return u * t.payoff(fx) * q * factor;
    }
    if let Some(px) = mark_of(env, acc, t) {
        return u * px.mark * px.usd_per_quote * factor;
    }
    // no model price: a long at its intrinsic value (the premium paid when there is no spot at all), a short at
    // least at the premium it received (its liability never looks smaller than at the open)
    let q = usd_per_quote(env, acc, &t.quote_ccy).unwrap_or(ONE);
    let intrinsic = spot_of(env, acc, &t.underlying).map(|s| t.payoff(s));
    let unit = match (p.side, intrinsic) {
        (Side::Buy, Some(i)) => i,
        (Side::Sell, Some(i)) => i.max(p.open_price),
        (_, None) => p.open_price,
    };
    u * unit * q * factor
}

/// (price a position closes at now, floating P&L incl. the premium basis), from the cached mark.
pub fn position_now(env: &Env, acc: &Account, p: &Position) -> (Option<D>, Option<D>) {
    let Some(t) = &p.option else { return (None, None) };
    // a book position closes at its mark (the book decides the real price); a house position at bid / ask
    let px = mark_of(env, acc, t).map(|q| if p.on_book() { q.mark } else { q.close_price(p.side) });
    (px, Some(r2(position_value(env, acc, p, t) + p.premium)))
}

/// Position Greeks: delta and gamma in contracts of the underlying (per unit × contracts), vega and theta in USD.
pub fn position_greeks(env: &Env, p: &Position) -> Option<(f64, f64, f64, f64)> {
    let t = p.option.as_ref()?;
    let q = env.options.mark(&env.tenant.slug, &env.group.code, t, env.now)?;
    let n = f(p.volume) * f(p.side.sign());
    Some((q.delta * n, q.gamma * n, q.vega * n, q.theta * n))
}

/* ------------------------------------------------------------------ */
/* Margin                                                              */
/* ------------------------------------------------------------------ */

/// Conservative margin when no scenario can be run (no snapshot / no price): 10 % of the notional of every
/// short contract plus its intrinsic value.
pub(crate) fn fallback_margin(env: &Env, acc: &Account, legs: &[ScenLeg]) -> D {
    let mut m = ZERO;
    for l in legs.iter().filter(|l| l.contracts < ZERO) {
        let t = &l.terms;
        let s = spot_of(env, acc, &t.underlying).unwrap_or(t.strike);
        let q = usd_per_quote(env, acc, &t.quote_ccy).unwrap_or(ONE);
        m += -l.contracts * t.contract_size * (s.max(t.strike) * D::new(10, 2) + t.right.intrinsic(s, t.strike)) * q;
    }
    m * acc.usd_factor()
}

/// Option margin (account currency) of `st`'s option positions minus `exclude`, plus hypothetical `extra` legs,
/// with `cfd` (symbol → long / short lots) as same-underlying offsets.
pub fn margin(env: &Env, st: &AccountState, cfd: &BTreeMap<String, (D, D)>, extra: &[ScenLeg], exclude: &BTreeSet<i64>) -> D {
    let mut by_u: BTreeMap<String, Vec<ScenLeg>> = BTreeMap::new();
    for p in st.positions.values().filter(|p| !exclude.contains(&p.ticket)) {
        if let Some(t) = &p.option {
            by_u.entry(t.underlying.clone()).or_default().push(ScenLeg { terms: t.clone(), contracts: p.volume * p.side.sign() });
        }
    }
    for l in extra {
        by_u.entry(l.terms.underlying.clone()).or_default().push(l.clone());
    }
    if by_u.is_empty() {
        return ZERO;
    }
    let acc = &st.account;
    let snap = env.options.snapshot();
    let weekend = weekend_margin(env.now);
    let mut total = ZERO;
    for (u, legs) in by_u {
        if !legs.iter().any(|l| l.contracts < ZERO) {
            continue; // long options are paid in full: no margin
        }
        // CFD / Options account split: an Options account's option margin never takes CFD offsets (and a CFD account
        // carries no option positions, so no option margin)
        let cfd_units = if env.group.product == Product::Options { 0.0 } else { cfd.get(&u).and_then(|(l, s)| env.specs.get(&u).map(|sp| f((*l - *s) * sp.contract_size))).unwrap_or(0.0) };
        let m = match env.options.scenario(&env.tenant.slug, &u, &legs, cfd_units, env.now) {
            Some(sc) if sc.incremental.is_finite() => dec(sc.incremental) * dec(sc.usd_per_quote) * acc.usd_factor(),
            _ => fallback_margin(env, acc, &legs),
        };
        let add_on = match (&snap, weekend) {
            (Some(s), true) => dec(s.group(&env.tenant.slug, &env.group.code, &u).weekend_margin_pct.max(0.0)),
            _ => ZERO,
        };
        total += m * (ONE + add_on / HUNDRED);
    }
    total
}

/* ------------------------------------------------------------------ */
/* Requests                                                            */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug, PartialEq)]
pub struct BarrierReq {
    pub kind: BarrierKind,
    pub level: D,
    pub rebate: D,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegReq {
    pub series: String,
    pub side: Side,
    pub contracts: D,
    pub barrier: Option<BarrierReq>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptKind {
    Market,
    Limit,
}

#[derive(Clone, Debug)]
pub struct OptOrderReq {
    pub legs: Vec<LegReq>,
    pub kind: OptKind,
    pub limit_premium: Option<D>,
    pub sl: Option<D>,
    pub tp: Option<D>,
    pub trigger: Option<Trigger>,
    /// Gtc or Today.
    pub tif: Expiry,
    pub client_order_id: Option<String>,
    pub source: Source,
    pub platform: String,
    pub comment: String,
    /// Suitability confirmed by the gateway (the client accepted the options intro); live and demo accounts.
    pub eligible: bool,
    /// Open contracts (long, short) on the client's other accounts (per-client limits).
    pub others: (D, D),
}

impl OptOrderReq {
    pub fn market(legs: Vec<LegReq>) -> Self {
        OptOrderReq {
            legs,
            kind: OptKind::Market,
            limit_premium: None,
            sl: None,
            tp: None,
            trigger: None,
            tif: Expiry::Gtc,
            client_order_id: None,
            source: Source::Manual,
            platform: "Web".into(),
            comment: String::new(),
            eligible: true,
            others: (ZERO, ZERO),
        }
    }
}

/// A resolved leg.
#[derive(Clone, Debug, PartialEq)]
pub struct LegSpec {
    pub terms: OptionTerms,
    pub side: Side,
    pub contracts: D,
}

/// A leg priced for a fill.
#[derive(Clone, Debug)]
pub struct Priced {
    pub spec: LegSpec,
    pub quote: OptPrice,
    /// Premium per unit the leg fills at (ask for a buy, bid for a sell).
    pub price: D,
    /// Premium of the leg in USD (unrounded).
    pub premium_usd: D,
    /// Premium in the account currency (absolute, rounded).
    pub premium: D,
    /// Signed balance effect of the premium (buy −, sell +).
    pub cash: D,
    pub commission: D,
}

/* ------------------------------------------------------------------ */
/* Gates                                                               */
/* ------------------------------------------------------------------ */

/// System-managed groups that never trade options, whatever the snapshot's group settings say: prop
/// (`prop*`, evaluated and paid out on lot-based rules), copy-trading followers (`copy`, `copy-netting`,
/// `copy-demo`, `copy-*`), PAMM funds (`pamm*`) and MAM block accounts (`mam*`). Their downstream maths (prop
/// evaluation, PAMM NAV and fees, MAM allocation, IB / loyalty lots) is CFD-only, and option trades of a master
/// are never mirrored anyway.
pub fn system_group(code: &str) -> bool {
    let g = code.trim().to_ascii_lowercase();
    g.starts_with("prop")
        || matches!(g.as_str(), "copy" | "copy-netting" | "copy-demo" | "pamm" | "mam")
        || g.starts_with("copy-")
        || g.starts_with("pamm-")
        || g.starts_with("mam-")
}

/// Refusal of options on a CFD account (CFD / Options account split).
pub const CFD_ACCOUNT: &str = "This is a CFD account: options trade in an Options account";

/// Module switch (system groups, the account's product, tenant, live / demo, underlying allow-list, group setting,
/// underlying enabled).
pub fn module_gate(env: &Env, st: &AccountState, snap: &OptSnapshot, underlying: &str) -> Result<(), Reject> {
    module_gate_for(env, st, snap, underlying, false)
}

/// `module_gate`; `lp` = a liquidity-provider account (the Kalks market maker, `book::Books::is_lp`): the house's own
/// quoting account trades options whatever group it was opened in (its group is `options-mm`, an Options group).
pub fn module_gate_for(env: &Env, st: &AccountState, snap: &OptSnapshot, underlying: &str, lp: bool) -> Result<(), Reject> {
    if system_group(&st.account.group) || system_group(&env.group.code) {
        return Err(rej("options_disabled", "Kalks FX Options are not available on copy-trading, PAMM, MAM or prop accounts"));
    }
    // CFD / Options account split: options (house prices and the order book) trade on Options accounts only
    if env.group.product != Product::Options && !lp {
        return Err(rej("product_mismatch", CFD_ACCOUNT));
    }
    // The gateway's `options` module switch is enforced before an order reaches the engine (api/options.rs,
    // api/options_book.rs → modules::require, 403 module_disabled), so replay never depends on it; the options
    // service's tenant switch below stays the Kalks risk switch.
    let live = st.account.kind == AccountKind::Live;
    let tenant = env.tenant.slug.as_str();
    if !snap.enabled(tenant, live) {
        return Err(rej("options_disabled", format!("Kalks FX Options are not enabled for {} accounts", if live { "live" } else { "demo" })));
    }
    if !snap.tenant_allows(tenant, underlying) || !snap.underlying(underlying).is_some_and(|u| u.enabled) {
        return Err(rej("options_disabled", format!("{underlying} options are not available")));
    }
    if !snap.group(tenant, &env.group.code, underlying).enabled {
        return Err(rej("options_disabled", format!("{underlying} options are not available in your account group")));
    }
    Ok(())
}

/// Per-client switches from the Back Office (blocked, close-only).
pub fn client_gate(env: &Env, st: &AccountState, snap: &OptSnapshot, opening: bool) -> Result<(), Reject> {
    if let Some(l) = snap.client_limit(&env.tenant.slug, st.account.user_id) {
        let why = if l.reason.trim().is_empty() { String::new() } else { format!(" ({})", l.reason.trim()) };
        if l.blocked {
            return Err(rej("not_eligible", format!("Options trading is blocked on your account{why}")));
        }
        if opening && l.close_only {
            return Err(rej("close_only", format!("Your options are close-only: you can close positions but not open new ones{why}")));
        }
    }
    Ok(())
}

/// Underlying session, the series' trade state and the cut-offs (no opens in the last `noOpenMinutes`, no
/// trading at all from `cutAt - closeOnlyMinutes`). `system` (stop-out, SL / TP) and forced dealer closes still
/// run on a halted series.
pub fn session_gate(env: &Env, snap: &OptSnapshot, t: &OptionTerms, state: TradeState, opening: bool, system: bool) -> Result<(), Reject> {
    if let Some(spec) = env.specs.get(&t.underlying)
        && !spec.is_open(env.now)
    {
        return Err(rej("market_closed", format!("The {} market is closed", t.underlying)));
    }
    let u = snap.underlying(&t.underlying);
    let close_only_min = u.map(|u| u.close_only_minutes.max(0)).unwrap_or(1) as i64;
    let no_open_min = u.map(|u| u.no_open_minutes.max(0)).unwrap_or(15) as i64;
    if state == TradeState::Closed || env.now >= t.expiry_at - chrono::Duration::minutes(close_only_min) {
        return Err(rej("cutoff", format!("Trading in {} has ended (expiry cut {})", t.series, t.expiry_at.format("%Y-%m-%d %H:%M UTC"))));
    }
    let key = t.expiry_key();
    if state == TradeState::Halted && (opening || !system) {
        let why = snap.control_reason(&env.tenant.slug, &t.underlying, &key, Some(&t.series), "halt", env.now).map(|r| format!(": {r}")).unwrap_or_default();
        return Err(rej("series_halted", format!("{} is halted{why}", t.series)));
    }
    if opening {
        if env.now >= t.expiry_at - chrono::Duration::minutes(no_open_min) {
            return Err(rej("cutoff", format!("No new positions in the last {no_open_min} minutes before the cut ({})", t.expiry_at.format("%H:%M UTC"))));
        }
        if state == TradeState::CloseOnly {
            let why = snap.control_reason(&env.tenant.slug, &t.underlying, &key, Some(&t.series), "close_only", env.now).map(|r| format!(": {r}")).unwrap_or_default();
            return Err(rej("close_only", format!("{} is close-only{why}", t.series)));
        }
    }
    Ok(())
}

/// The raw spot behind a fill must be fresh (a frozen spot is the dealer's price).
fn fresh_gate(env: &Env, q: &OptPrice) -> Result<(), Reject> {
    if env.max_quote_age_ms > 0 && q.spot_ms > 0 && env.now.timestamp_millis() - q.spot_ms > env.max_quote_age_ms {
        return Err(rej("stale_prices", "The underlying price is stale: options trading is paused"));
    }
    Ok(())
}

/// Contracts on the underlying's step / min / max.
pub fn contracts_gate(u: &Underlying, c: D) -> Result<(), Reject> {
    if c <= ZERO {
        return Err(rej("invalid_volume", "Enter a number of contracts above 0"));
    }
    let min = dec(u.min_contracts);
    if min > ZERO && c < min {
        return Err(rej("invalid_volume", format!("Minimum is {} contracts", min.normalize())));
    }
    let max = dec(u.max_contracts);
    if max > ZERO && c > max {
        return Err(rej("invalid_volume", format!("Maximum is {} contracts per order", max.normalize())));
    }
    let step = dec(u.contract_step);
    if step > ZERO && !(c % step).is_zero() {
        return Err(rej("invalid_volume", format!("Contracts must be a multiple of {}", step.normalize())));
    }
    Ok(())
}

/// Resolves a leg against the snapshot (series, expiry, underlying, barrier terms).
pub fn resolve(env: &Env, acc: &Account, snap: &OptSnapshot, leg: &LegReq) -> Result<LegSpec, Reject> {
    let s = snap.series.get(leg.series.trim()).ok_or_else(|| rej("unknown_series", format!("Unknown option series {}", leg.series)))?;
    if s.status != "active" {
        return Err(rej("series_halted", format!("{} is not open for trading", s.code)));
    }
    let e = snap.expiry_by_id(s.expiry_id).ok_or_else(|| rej("unknown_series", format!("Unknown expiry of {}", s.code)))?;
    let u = snap.underlying(&s.symbol).ok_or_else(|| rej("options_disabled", format!("{} options are not available", s.symbol)))?;
    contracts_gate(u, leg.contracts)?;
    let right = OptRight::parse(&s.kind).ok_or_else(|| rej("unknown_series", format!("Unknown option type {}", s.kind)))?;
    let strike = dec(s.strike);
    let barrier = match &leg.barrier {
        None => None,
        Some(b) => {
            if !u.barriers_enabled {
                return Err(rej("invalid_barrier", format!("Barrier options are not available on {}", u.symbol)));
            }
            let level = rdp(b.level, u.digits.clamp(0, 12) as u32);
            if level <= ZERO || b.rebate < ZERO {
                return Err(rej("invalid_barrier", "Enter a barrier level above 0 and a rebate of 0 or more"));
            }
            if let Some(s) = spot_of(env, acc, &u.symbol)
                && b.kind.hit(s, level)
            {
                return Err(rej("invalid_barrier", format!("The barrier {} is already reached (spot {})", level.normalize(), s.normalize())));
            }
            // knock-outs that can never pay: an up-and-out call at or below its strike, a down-and-out put at or above
            if (b.kind == BarrierKind::UO && right == OptRight::Call && level <= strike) || (b.kind == BarrierKind::DO && right == OptRight::Put && level >= strike) {
                return Err(rej("invalid_barrier", "This knock-out can never pay: move the barrier beyond the strike"));
            }
            Some(BarrierTerms { kind: b.kind, level, rebate: b.rebate, knocked_in: false, knocked_at: None, knock_spot: None })
        }
    };
    Ok(LegSpec {
        terms: OptionTerms { series: s.code.clone(), underlying: s.symbol.clone(), right, strike, expiry: e.expiry_date, expiry_at: e.cut_at, contract_size: dec(u.contract_size), quote_ccy: u.quote_ccy.clone(), barrier },
        side: leg.side,
        contracts: leg.contracts,
    })
}

/// Commission of one fill: `min(perContract * n, capPct % * premium)` in the account currency.
pub fn commission(env: &Env, snap: &OptSnapshot, acc: &Account, underlying: &str, contracts: D, premium_usd: D) -> D {
    let g = snap.group(&env.tenant.slug, &env.group.code, underlying);
    let per = dec(g.commission_per_contract.max(0.0)) * contracts;
    let cap = dec(g.commission_cap_pct.max(0.0)) / HUNDRED * premium_usd.abs();
    r2(per.min(cap).max(ZERO) * acc.usd_factor())
}

/// Prices one leg for a fill at the fresh quote.
fn price_leg(env: &Env, snap: &OptSnapshot, acc: &Account, spec: &LegSpec, q: OptPrice) -> Result<Priced, Reject> {
    let price = q.open_price(spec.side);
    if spec.side == Side::Sell && price <= ZERO {
        return Err(rej("no_price", format!("There is no bid for {} right now", spec.terms.series)));
    }
    if spec.side == Side::Buy && price <= ZERO {
        return Err(rej("no_price", format!("There is no offer for {} right now", spec.terms.series)));
    }
    let premium_usd = price * spec.contracts * spec.terms.contract_size * q.usd_per_quote;
    let premium = r2(premium_usd * acc.usd_factor());
    let cash = if spec.side == Side::Buy { -premium } else { premium };
    let commission = commission(env, snap, acc, &spec.terms.underlying, spec.contracts, premium_usd);
    Ok(Priced { spec: spec.clone(), quote: q, price, premium_usd, premium, cash, commission })
}

/// Options that gates do not stop from computing (the preview lists them all).
pub struct GateOpts {
    /// Suitability: None = not checked (pending fills; it was checked when the order was placed).
    pub eligible: Option<bool>,
    pub others: (D, D),
}

/// Resolves gates and fresh prices for opening `specs`. Fatal problems (no price) are `Err`; gate refusals are
/// collected in order.
pub fn price_open(env: &Env, st: &AccountState, snap: &OptSnapshot, specs: &[LegSpec], g: &GateOpts) -> Result<(Vec<Priced>, Vec<Reject>), Reject> {
    let mut reasons: Vec<Reject> = Vec::new();
    let mut push = |r: Result<(), Reject>| {
        if let Err(e) = r
            && !reasons.iter().any(|x| x.code == e.code)
        {
            reasons.push(e);
        }
    };
    let acc = &st.account;
    let first = &specs[0].terms;
    push(module_gate(env, st, snap, &first.underlying));
    // eligibility = the client accepted the options intro (gateway suitability); live and demo alike
    if let Some(ok) = g.eligible
        && !ok
    {
        push(Err(rej("not_eligible", NOT_ELIGIBLE)));
    }
    push(client_gate(env, st, snap, true));
    push(gate(env, st, &first.underlying, true, ZERO, None));
    if env.options.stale(env.now) {
        push(Err(rej("stale_prices", "Option prices are stale: you can only close positions until they are back")));
    }
    let mut out = Vec::with_capacity(specs.len());
    for s in specs {
        let q = env.options.price(&env.tenant.slug, &env.group.code, &s.terms, env.now).map_err(price_err)?;
        push(session_gate(env, snap, &s.terms, q.state, true, false));
        push(fresh_gate(env, &q));
        out.push(price_leg(env, snap, acc, s, q)?);
    }
    // per-client contract limits: client limit (Back Office) and the group's max per client
    let (mut long, mut short) = g.others;
    for p in st.positions.values().filter(|p| p.option.is_some()) {
        if p.side == Side::Buy { long += p.volume } else { short += p.volume }
    }
    let add_long: D = specs.iter().filter(|s| s.side == Side::Buy).map(|s| s.contracts).sum();
    let add_short: D = specs.iter().filter(|s| s.side == Side::Sell).map(|s| s.contracts).sum();
    let gs = snap.group(&env.tenant.slug, &env.group.code, &first.underlying);
    let cl = snap.client_limit(&env.tenant.slug, acc.user_id);
    let mut max_total = dec(gs.max_contracts_per_client).max(ZERO);
    if let Some(m) = cl.and_then(|l| l.max_contracts).map(dec).filter(|m| *m >= ZERO) {
        max_total = if max_total > ZERO { max_total.min(m) } else { m };
    }
    if (max_total > ZERO || cl.and_then(|l| l.max_contracts).is_some()) && long + short + add_long + add_short > max_total {
        push(Err(rej("limit_contracts", format!("Your options limit is {} open contracts (you hold {})", max_total.normalize(), (long + short).normalize()))));
    }
    if let Some(ms) = cl.and_then(|l| l.max_short_contracts).map(dec)
        && add_short > ZERO
        && short + add_short > ms
    {
        push(Err(rej("limit_contracts", format!("Your limit for sold options is {} contracts (you have sold {})", ms.normalize(), short.normalize()))));
    }
    Ok((out, reasons))
}

/// Cash still free for premiums: the balance not needed as margin, not reserved for working book orders and not
/// made of credit / bonus (`free_margin` already excludes the order reserve).
pub fn free_cash(m: &Metrics) -> D {
    (m.balance - m.order_reserve).min(m.free_margin - m.credit - m.bonus).max(ZERO)
}

/// Money checks of an opening trade from the metrics before and after its events.
pub fn funds_reasons(acc: &Account, before: &Metrics, after: &Metrics, legs: &[Priced]) -> Vec<Reject> {
    let mut out = Vec::new();
    let ccy = acc.ccy();
    let net: D = legs.iter().map(|l| l.cash).sum();
    let comm: D = legs.iter().map(|l| l.commission).sum();
    let debit = (-net).max(ZERO) + comm;
    let cash = free_cash(before);
    if debit > cash {
        out.push(rej("insufficient_cash", format!("Not enough cash: {} {ccy} needed for premium and commission, {} {ccy} available (credit and bonus can't pay premiums)", r2(debit).normalize(), r2(cash).normalize())));
    }
    if after.margin > before.margin {
        if after.equity - after.margin - after.order_reserve < ZERO {
            out.push(rej("insufficient_margin", format!("Not enough margin: {} {ccy} needed, free margin {} {ccy}", r2(after.margin - before.margin).normalize(), r2(before.free_margin).normalize())));
        } else if after.equity - after.credit - after.bonus - after.margin - after.order_reserve < ZERO {
            out.push(rej("insufficient_margin", "Sold options need margin from your own funds: credit and bonus can't cover it"));
        }
    }
    out
}

/* ------------------------------------------------------------------ */
/* Booking                                                             */
/* ------------------------------------------------------------------ */

pub struct FillMeta {
    pub source: Source,
    pub platform: String,
    pub comment: String,
    pub client_order_id: Option<String>,
    pub sl: Option<D>,
    pub tp: Option<D>,
    pub reason: DealReason,
    /// The pending order that filled (its ticket becomes the first leg's position ticket).
    pub order_ticket: Option<i64>,
    pub staff: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegFill {
    pub ticket: i64,
    pub deal: i64,
    pub series: String,
    pub side: Side,
    pub contracts: D,
    pub price: D,
    pub premium: D,
    pub commission: D,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Filled {
    pub combo_id: Option<i64>,
    pub legs: Vec<LegFill>,
}

/// Books priced legs: premium and commission postings, the positions and their entry deals.
fn book(tx: &mut Tx, env: &Env, legs: &[Priced], meta: &FillMeta) -> Filled {
    let combo_id = (legs.len() > 1).then(|| env.ids.ticket());
    let login = tx.st.account.login;
    let mut out = Vec::new();
    for (i, l) in legs.iter().enumerate() {
        let ticket = match meta.order_ticket {
            Some(t) if i == 0 && !tx.st.positions.contains_key(&t) => t,
            _ => env.ids.ticket(),
        };
        let deal_id = env.ids.deal();
        let t = &l.spec.terms;
        let txn = tx.post(env, TxnKind::OptionPremium, format!("deal:{deal_id}:premium"), "balance", HOUSE_PREMIUM, l.cash, Some(format!("deal:{deal_id}")), None, Some(format!("{} {} {}", l.spec.side.as_str(), l.spec.contracts.normalize(), t.series)));
        if !l.commission.is_zero() {
            tx.post(env, TxnKind::Commission, format!("deal:{deal_id}:commission"), "balance", "commission", -l.commission, Some(format!("deal:{deal_id}")), None, None);
        }
        let (sl, tp) = if legs.len() == 1 { (meta.sl, meta.tp) } else { (None, None) };
        let p = Position {
            ticket,
            login,
            symbol: t.series.clone(),
            side: l.spec.side,
            volume: l.spec.contracts,
            open_price: l.price,
            open_time: env.now,
            sl,
            tp,
            trailing: None,
            swap: ZERO,
            commission: l.commission,
            source: meta.source,
            platform: meta.platform.clone(),
            comment: meta.comment.clone(),
            book: Book::B,
            order_ticket: meta.order_ticket.unwrap_or(ticket),
            parent_ticket: None,
            child_tickets: vec![],
            book_since: env.now,
            book_price: l.price,
            book_carry_a: ZERO,
            book_carry_b: ZERO,
            route_history: vec![RouteEvent { at: env.now, kind: "open".into(), from: None, to: Book::B, volume: l.spec.contracts, price: l.price, staff: meta.staff.clone().unwrap_or_else(|| "Options desk".into()), reason: "Options are B-book".into(), related_ticket: None }],
            price_corrected: false,
            last_swap_day: None,
            client_order_id: meta.client_order_id.clone(),
            reversed_from: None,
            option: Some(t.clone()),
            combo_id,
            premium: l.cash,
            venue: None,
        };
        let deal = Deal {
            id: deal_id,
            login,
            position_ticket: ticket,
            order_ticket: Some(meta.order_ticket.unwrap_or(ticket)),
            symbol: t.series.clone(),
            side: l.spec.side,
            position_side: l.spec.side,
            entry: DealEntry::In,
            volume: l.spec.contracts,
            price: l.price,
            profit: ZERO,
            swap: ZERO,
            commission: l.commission,
            reason: meta.reason,
            book: Book::B,
            time: env.now,
            open_price: l.price,
            open_time: env.now,
            source: meta.source,
            comment: meta.comment.clone(),
            price_correction: false,
            ledger_txn: txn,
            staff: meta.staff.clone(),
            reason_code: None,
            snapshot: None,
            client_order_id: meta.client_order_id.clone(),
            partial: false,
            option: Some(DealOption { terms: t.clone(), cash: l.cash, usd_per_quote: l.quote.usd_per_quote, spot: Some(l.quote.spot), fixing: None, run: None, combo_id, charged: l.commission, fill: None, rebate: ZERO }),
        };
        tx.emit(Event::PositionOpened { position: p, deal: Some(deal) });
        out.push(LegFill { ticket, deal: deal_id, series: t.series.clone(), side: l.spec.side, contracts: l.spec.contracts, price: l.price, premium: l.premium, commission: l.commission });
    }
    let what: Vec<String> = out.iter().map(|l| format!("{} {} {} at {}", l.side.as_str(), l.contracts.normalize(), l.series, l.price.normalize())).collect();
    tx.note("fill", format!("Options filled: {}", what.join(", ")), json!({"comboId": combo_id, "tickets": out.iter().map(|l| l.ticket).collect::<Vec<_>>(), "options": true}));
    Filled { combo_id, legs: out }
}

/// Premium SL / TP of a position of `side` against the price it closes at now.
pub fn check_premium_sltp(side: Side, close_px: D, sl: Option<D>, tp: Option<D>) -> Result<(), Reject> {
    if let Some(sl) = sl {
        let bad = match side {
            Side::Buy => sl >= close_px,
            Side::Sell => sl <= close_px,
        };
        if bad || sl <= ZERO {
            return Err(rej("invalid_sl", format!("A premium stop loss must be {} {}", if side == Side::Buy { "below" } else { "above" }, close_px.normalize())));
        }
    }
    if let Some(tp) = tp {
        let bad = match side {
            Side::Buy => tp <= close_px,
            Side::Sell => tp >= close_px,
        };
        if bad || tp < ZERO {
            return Err(rej("invalid_tp", format!("A premium take profit must be {} {}", if side == Side::Buy { "above" } else { "below" }, close_px.normalize())));
        }
    }
    Ok(())
}

/// Fills `specs` now: gates, fresh prices, booking, then the money checks on the result. Any refusal returns Err
/// and the shard drops the whole transaction (all legs or none).
pub fn fill_now(tx: &mut Tx, env: &Env, specs: &[LegSpec], g: &GateOpts, meta: &FillMeta) -> Result<Filled, Reject> {
    let snap = snapshot(env)?;
    let (legs, reasons) = price_open(env, &tx.st, &snap, specs, g)?;
    if let Some(r) = reasons.into_iter().next() {
        return Err(r);
    }
    if specs.len() == 1 {
        check_premium_sltp(specs[0].side, legs[0].quote.close_price(specs[0].side), meta.sl, meta.tp)?;
    }
    let before = metrics(env, &tx.st);
    let filled = book(tx, env, &legs, meta);
    let after = metrics(env, &tx.st);
    if let Some(r) = funds_reasons(&tx.st.account, &before, &after, &legs).into_iter().next() {
        return Err(r);
    }
    Ok(filled)
}

/// Net debit per combo unit (legs scaled by the smallest leg): + = the client pays.
pub fn net_unit(legs: &[(Side, D, D)]) -> D {
    let min = legs.iter().map(|l| l.2).min().unwrap_or(ONE).max(D::new(1, 9));
    legs.iter().map(|(s, px, c)| if *s == Side::Buy { *px * *c / min } else { -*px * *c / min }).sum()
}

/// Has a premium limit been reached? A single leg buys at or below / sells at or above the limit; a multi-leg
/// order fills when its net debit per combo unit is at or below it (an all-sell order: its net credit at or
/// above it).
pub fn limit_met(legs: &[Priced], limit: D) -> bool {
    if legs.len() == 1 {
        let l = &legs[0];
        return if l.spec.side == Side::Buy { l.price <= limit } else { l.price >= limit };
    }
    let debit = net_unit(&legs.iter().map(|l| (l.spec.side, l.price, l.spec.contracts)).collect::<Vec<_>>());
    if legs.iter().any(|l| l.spec.side == Side::Buy) { debit <= limit } else { -debit >= limit }
}

/* ------------------------------------------------------------------ */
/* Place / preview                                                     */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug, PartialEq)]
pub enum PlaceOut {
    Filled(Filled),
    Pending { ticket: i64 },
    /// Same client order id was already processed: (ticket remembered, is it still a pending order).
    Duplicate { ticket: i64 },
}

fn check_request(req: &OptOrderReq) -> Result<(), Reject> {
    if req.legs.is_empty() {
        return Err(rej("invalid_order", "Add at least one leg"));
    }
    if req.legs.len() > MAX_LEGS {
        return Err(rej("invalid_order", format!("An order can have at most {MAX_LEGS} legs")));
    }
    if let Some(c) = &req.client_order_id
        && (c.is_empty() || c.len() > 64)
    {
        return Err(rej("invalid_client_order_id", "clientOrderId must be 1–64 characters"));
    }
    if req.legs.len() > 1 && (req.sl.is_some() || req.tp.is_some()) {
        return Err(rej("invalid_sl", "Premium stop loss / take profit is available for single-leg orders"));
    }
    if req.kind == OptKind::Limit {
        match req.limit_premium {
            None => return Err(rej("invalid_price", "Enter the limit premium")),
            Some(l) if req.legs.len() == 1 && l <= ZERO => return Err(rej("invalid_price", "The limit premium must be above 0")),
            _ => {}
        }
    }
    Ok(())
}

fn resolve_all(env: &Env, acc: &Account, snap: &OptSnapshot, req: &OptOrderReq) -> Result<Vec<LegSpec>, Reject> {
    let specs: Vec<LegSpec> = req.legs.iter().map(|l| resolve(env, acc, snap, l)).collect::<Result<_, _>>()?;
    if specs.iter().any(|s| s.terms.underlying != specs[0].terms.underlying) {
        return Err(rej("invalid_order", "All legs of a strategy must be on the same underlying"));
    }
    Ok(specs)
}

/// Places an option order: fills it now (market, or a limit already reached) or rests it as a pending order
/// (a limit not reached yet, or an underlying trigger).
pub fn place(tx: &mut Tx, env: &Env, req: OptOrderReq) -> Result<PlaceOut, Reject> {
    check_request(&req)?;
    if let Some(cid) = &req.client_order_id
        && let Some(t) = tx.st.client_ids.get(cid)
    {
        return Ok(PlaceOut::Duplicate { ticket: *t });
    }
    let snap = snapshot(env)?;
    let specs = resolve_all(env, &tx.st.account, &snap, &req)?;
    let g = GateOpts { eligible: Some(req.eligible), others: req.others };
    let meta = FillMeta { source: req.source, platform: req.platform.clone(), comment: req.comment.clone(), client_order_id: req.client_order_id.clone(), sl: req.sl, tp: req.tp, reason: DealReason::Client, order_ticket: None, staff: None };
    let pending = req.trigger.is_some() || req.kind == OptKind::Limit;
    if !pending {
        return fill_now(tx, env, &specs, &g, &meta).map(PlaceOut::Filled);
    }
    // gates now (a pending order must be allowed to open when placed), the money checks at the fill
    let (legs, reasons) = price_open(env, &tx.st, &snap, &specs, &g)?;
    if let Some(r) = reasons.into_iter().next() {
        return Err(r);
    }
    if req.trigger.is_none()
        && let Some(lim) = req.limit_premium
        && limit_met(&legs, lim)
    {
        return fill_now(tx, env, &specs, &g, &meta).map(PlaceOut::Filled);
    }
    if let Some(tr) = &req.trigger {
        if tr.price <= ZERO {
            return Err(rej("invalid_trigger", "Enter the trigger price"));
        }
        if spot_of(env, &tx.st.account, &tr.symbol).is_none() {
            return Err(rej("invalid_trigger", format!("No price for {}", tr.symbol)));
        }
    }
    if specs.len() == 1
        && let Some(lim) = req.limit_premium
    {
        // SL / TP around the price the position would open at
        check_premium_sltp(specs[0].side, lim, req.sl, req.tp)?;
    }
    let earliest_cut = specs.iter().map(|s| s.terms.expiry_at).min().unwrap();
    let no_open = snap.underlying(&specs[0].terms.underlying).map(|u| u.no_open_minutes.max(0)).unwrap_or(15) as i64;
    let last_open = earliest_cut - chrono::Duration::minutes(no_open);
    let tif_end = match req.tif {
        Expiry::Today => Some(crate::specs::end_of_server_day(env.now)),
        _ => None,
    };
    let expiry_at = Some(tif_end.map(|t| t.min(last_open)).unwrap_or(last_open));
    let ticket = env.ids.ticket();
    let first = &specs[0];
    let order = Order {
        ticket,
        login: tx.st.account.login,
        symbol: first.terms.series.clone(),
        side: first.side,
        kind: if req.kind == OptKind::Limit { OrderType::Limit } else { OrderType::Market },
        volume: first.contracts,
        price: req.limit_premium.unwrap_or(ZERO),
        stop_limit: None,
        sl: req.sl,
        tp: req.tp,
        trailing: None,
        expiry: if tif_end.is_some() { Expiry::Today } else { Expiry::Date },
        expiry_at,
        oco: None,
        source: req.source,
        platform: req.platform.clone(),
        comment: req.comment.clone(),
        book: Some(Book::B),
        placed_at: env.now,
        triggered: false,
        client_order_id: req.client_order_id.clone(),
        option: Some(OptionOrder { legs: specs.iter().map(|s| OptLeg { terms: s.terms.clone(), side: s.side, contracts: s.contracts }).collect(), limit_premium: req.limit_premium, book: None }),
        combo_id: None,
        trigger: req.trigger.clone(),
    };
    tx.emit(Event::OrderPlaced { order });
    tx.note("order_placed", format!("Options order #{ticket} placed"), json!({"ticket": ticket, "options": true}));
    Ok(PlaceOut::Pending { ticket })
}

/// What a client sees before placing: prices, money impact, payoff and Greeks. Never changes the account.
#[derive(Clone, Debug)]
pub struct Preview {
    pub reasons: Vec<Reject>,
    pub legs: Vec<Priced>,
    pub net_premium: D,
    pub commission: D,
    pub margin_before: D,
    pub margin_after: D,
    pub free_margin_after: D,
    pub cash_after: D,
    pub max_profit: Option<D>,
    pub max_loss: Option<D>,
    pub breakevens: Vec<D>,
    pub greeks: (f64, f64, f64, f64),
}

pub fn preview(env: &Env, st: &AccountState, req: &OptOrderReq) -> Preview {
    let m0 = metrics(env, st);
    let empty = |r: Reject| Preview {
        reasons: vec![r],
        legs: vec![],
        net_premium: ZERO,
        commission: ZERO,
        margin_before: m0.margin,
        margin_after: m0.margin,
        free_margin_after: m0.free_margin,
        cash_after: st.balance,
        max_profit: None,
        max_loss: None,
        breakevens: vec![],
        greeks: (0.0, 0.0, 0.0, 0.0),
    };
    if let Err(e) = check_request(req) {
        return empty(e);
    }
    let snap = match snapshot(env) {
        Ok(s) => s,
        Err(e) => return empty(e),
    };
    let specs = match resolve_all(env, &st.account, &snap, req) {
        Ok(s) => s,
        Err(e) => return empty(e),
    };
    let g = GateOpts { eligible: Some(req.eligible), others: req.others };
    let (legs, mut reasons) = match price_open(env, st, &snap, &specs, &g) {
        Ok(x) => x,
        Err(e) => return empty(e),
    };
    if specs.len() == 1
        && let Err(e) = check_premium_sltp(specs[0].side, legs[0].quote.close_price(specs[0].side), req.sl, req.tp)
    {
        reasons.push(e);
    }
    let mut tx = Tx::new(st);
    let meta = FillMeta { source: req.source, platform: req.platform.clone(), comment: String::new(), client_order_id: None, sl: None, tp: None, reason: DealReason::Client, order_ticket: None, staff: None };
    // a scratch transaction with its own ids: nothing of it is ever committed
    let ids = super::Ids::new(i64::MAX / 2, i64::MAX / 2, i64::MAX / 2);
    let scratch = Env { ids: &ids, ..*env };
    book(&mut tx, &scratch, &legs, &meta);
    let m1 = metrics(env, &tx.st);
    for r in funds_reasons(&st.account, &m0, &m1, &legs) {
        if !reasons.iter().any(|x| x.code == r.code) {
            reasons.push(r);
        }
    }
    let factor = st.account.usd_factor();
    let net_cash: D = legs.iter().map(|l| l.cash).sum();
    let commission: D = legs.iter().map(|l| l.commission).sum();
    // payoff at expiry (vanilla legs exactly; barrier legs are path-dependent and are left out of the extremes)
    let has_barrier = legs.iter().any(|l| l.spec.terms.barrier.is_some());
    let usdq = f(legs.first().map(|l| l.quote.usd_per_quote).unwrap_or(ONE));
    let pay: Vec<optmath::payoff::Leg> = legs
        .iter()
        .map(|l| {
            let t = &l.spec.terms;
            let qty = f(l.spec.contracts * t.contract_size * l.spec.side.sign());
            let kind = match t.right {
                OptRight::Call => optmath::payoff::LegKind::Call { strike: f(t.strike) },
                OptRight::Put => optmath::payoff::LegKind::Put { strike: f(t.strike) },
            };
            optmath::payoff::Leg { kind, qty, price: f(l.price) }
        })
        .collect();
    let (max_profit, max_loss, breakevens) = if has_barrier {
        let long_only = legs.iter().all(|l| l.spec.side == Side::Buy);
        (None, long_only.then(|| r2(-net_cash + commission)), vec![])
    } else {
        let s = optmath::payoff::summarize(&pay);
        let money = |x: f64| r2(dec(x * usdq) * factor);
        let comm = commission;
        (s.max_profit.map(|x| money(x) - comm), s.max_loss.map(|x| money(x) + comm), s.breakevens.iter().map(|b| dec(*b)).collect())
    };
    let mut greeks = (0.0, 0.0, 0.0, 0.0);
    for l in &legs {
        let n = f(l.spec.contracts) * f(l.spec.side.sign());
        greeks.0 += l.quote.delta * n;
        greeks.1 += l.quote.gamma * n;
        greeks.2 += l.quote.vega * n;
        greeks.3 += l.quote.theta * n;
    }
    Preview {
        reasons,
        net_premium: -net_cash,
        commission,
        margin_before: m0.margin,
        margin_after: m1.margin,
        free_margin_after: m1.free_margin,
        cash_after: tx.st.balance,
        max_profit,
        max_loss,
        breakevens,
        greeks,
        legs,
    }
}

/* ------------------------------------------------------------------ */
/* Close                                                               */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
pub struct OptClose {
    /// None = the whole position.
    pub volume: Option<D>,
    pub reason: DealReason,
    pub dealer: Option<DealerCtx>,
    /// Engine-initiated (stop-out, SL / TP): no client gates, runs on a halted series.
    pub system: bool,
    pub comment: String,
}

impl OptClose {
    pub fn client() -> Self {
        OptClose { volume: None, reason: DealReason::Client, dealer: None, system: false, comment: String::new() }
    }
}

/// Closes (part of) an option position at the fresh bid (long) / ask (short). Returns (deal id, realised P&L).
pub fn close(tx: &mut Tx, env: &Env, ticket: i64, c: OptClose) -> Result<(i64, D), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| rej("not_found", format!("Position #{ticket} not found")))?;
    let t = p.option.clone().ok_or_else(|| rej("not_option", format!("#{ticket} is not an option position")))?;
    if p.on_book() {
        // another account holds the other side: only the order book can close it (a reduce-only order)
        return Err(rej("book_venue", format!("#{ticket} trades on the options order book: close it with an order")));
    }
    let snap = snapshot(env)?;
    let force = c.dealer.as_ref().is_some_and(|d| d.force);
    if !c.system {
        gate(env, &tx.st, &t.underlying, false, ZERO, c.dealer.as_ref())?;
        if c.dealer.is_none() {
            client_gate(env, &tx.st, &snap, false)?;
        }
    }
    let volume = match c.volume {
        None => p.volume,
        Some(v) => {
            if v <= ZERO || v > p.volume {
                return Err(rej("invalid_volume", format!("Contracts must be between 0 and {}", p.volume.normalize())));
            }
            if v < p.volume
                && let Some(u) = snap.underlying(&t.underlying)
            {
                let step = dec(u.contract_step);
                if step > ZERO && !(v % step).is_zero() {
                    return Err(rej("invalid_volume", format!("Contracts must be a multiple of {}", step.normalize())));
                }
                let min = dec(u.min_contracts);
                if min > ZERO && p.volume - v < min {
                    return Err(rej("invalid_volume", format!("The remaining position would be below the minimum of {} contracts", min.normalize())));
                }
            }
            v
        }
    };
    let q = env.options.price(&env.tenant.slug, &env.group.code, &t, env.now).map_err(price_err)?;
    session_gate(env, &snap, &t, q.state, false, c.system || force)?;
    fresh_gate(env, &q)?;
    let px = q.close_price(p.side);
    let acc = tx.st.account.clone();
    let premium_usd = px * volume * t.contract_size * q.usd_per_quote;
    let amount = r2(premium_usd * acc.usd_factor());
    // a long sells (receives), a short buys back (pays)
    let cash = if p.side == Side::Buy { amount } else { -amount };
    let comm = commission(env, &snap, &acc, &t.underlying, volume, premium_usd);
    let reason = c.reason;
    exit(tx, env, &p, volume, px, cash, comm, q.usd_per_quote, Some(q.spot), None, None, reason, TxnKind::OptionPremium, Counter::House(HOUSE_PREMIUM), None, c.dealer.as_ref(), &c.comment)
}

/// Books an exit of `volume` contracts with `cash` on the balance (commission `comm` charged separately).
#[allow(clippy::too_many_arguments)]
fn exit(
    tx: &mut Tx,
    env: &Env,
    p: &Position,
    volume: D,
    px: D,
    cash: D,
    comm: D,
    usdq: D,
    spot: Option<D>,
    fixing: Option<D>,
    run: Option<i32>,
    reason: DealReason,
    kind: TxnKind,
    counter: Counter<'_>,
    key: Option<String>,
    dealer: Option<&DealerCtx>,
    comment: &str,
) -> Result<(i64, D), Reject> {
    let t = p.option.clone().ok_or_else(|| rej("not_option", "not an option position"))?;
    let full = volume >= p.volume;
    let frac = if full { ONE } else { volume / p.volume };
    let basis = if full { p.premium } else { r2(p.premium * frac) };
    let entry_comm = if full { p.commission } else { r2(p.commission * frac) };
    let deal_id = env.ids.deal();
    let cash = r2(cash);
    let key = key.unwrap_or_else(|| format!("deal:{deal_id}:premium"));
    let note = Some(format!("{} {} {}", reason.as_str(), volume.normalize(), t.series));
    let txn = if cash.is_zero() {
        None
    } else {
        match counter {
            Counter::House(house) => tx.post(env, kind, key, "balance", house, cash, Some(format!("deal:{deal_id}")), dealer.map(|d| d.reason_code.clone()), note),
            Counter::Clearing(terms) => post_clearing(tx, env, kind, key, terms, cash, Some(format!("deal:{deal_id}")), dealer.map(|d| d.reason_code.clone()), note),
        }
    };
    if !comm.is_zero() {
        tx.post(env, TxnKind::Commission, format!("deal:{deal_id}:commission"), "balance", "commission", -comm, Some(format!("deal:{deal_id}")), None, None);
    }
    let profit = cash + basis;
    let snapshot = Position { volume, premium: basis, commission: entry_comm, ..p.clone() };
    let deal = Deal {
        id: deal_id,
        login: p.login,
        position_ticket: p.ticket,
        order_ticket: None,
        symbol: p.symbol.clone(),
        side: p.side.opposite(),
        position_side: p.side,
        entry: DealEntry::Out,
        volume,
        price: px,
        profit,
        swap: ZERO,
        commission: entry_comm + comm,
        reason,
        book: p.book,
        time: env.now,
        open_price: p.open_price,
        open_time: p.open_time,
        source: if dealer.is_some() { Source::Dealer } else if matches!(reason, DealReason::Client) { Source::Manual } else { Source::System },
        comment: comment.to_string(),
        price_correction: false,
        ledger_txn: txn,
        staff: dealer.map(|d| d.staff.clone()),
        reason_code: dealer.map(|d| d.reason_code.clone()),
        snapshot: Some(Box::new(snapshot)),
        client_order_id: None,
        partial: !full,
        option: Some(DealOption { terms: t.clone(), cash, usd_per_quote: usdq, spot, fixing, run, combo_id: p.combo_id, charged: comm, fill: None, rebate: ZERO }),
    };
    let rest = if full { None } else { Some(Position { volume: p.volume - volume, premium: p.premium - basis, commission: p.commission - entry_comm, ..p.clone() }) };
    tx.emit(Event::PositionClosed { deal, position: rest });
    let what = match reason {
        DealReason::Expiry => "settled",
        DealReason::KnockOut => "knocked out",
        _ if full => "closed",
        _ => "partially closed",
    };
    tx.note(
        match reason {
            DealReason::Expiry => "settlement",
            DealReason::KnockOut => "knock_out",
            DealReason::Sl => "sl",
            DealReason::Tp => "tp",
            DealReason::StopOut => "stop_out",
            _ => "close",
        },
        format!("#{} {} {} {what} at {}", p.ticket, volume.normalize(), t.series, px.normalize()),
        json!({"ticket": p.ticket, "dealId": deal_id, "profit": num(profit), "cash": num(cash), "options": true}),
    );
    Ok((deal_id, profit))
}

/// Closes every leg of a combo in one transaction (all or nothing).
pub fn close_combo(tx: &mut Tx, env: &Env, combo_id: i64, dealer: Option<DealerCtx>) -> Result<Vec<(i64, i64, D)>, Reject> {
    let tickets: Vec<i64> = tx.st.positions.values().filter(|p| p.combo_id == Some(combo_id)).map(|p| p.ticket).collect();
    if tickets.is_empty() {
        return Err(rej("not_found", format!("Strategy #{combo_id} not found")));
    }
    let mut out = Vec::new();
    for t in tickets {
        let reason = if dealer.is_some() { DealReason::Dealer } else { DealReason::Client };
        let (deal, profit) = close(tx, env, t, OptClose { volume: None, reason, dealer: dealer.clone(), system: false, comment: format!("strategy #{combo_id}") })?;
        out.push((t, deal, profit));
    }
    apply_nbp(tx, env);
    Ok(out)
}

/* ------------------------------------------------------------------ */
/* Barriers                                                            */
/* ------------------------------------------------------------------ */

/// Knocks every live barrier on `underlying` reached by the raw mid: a knock-out closes at its rebate, a
/// knock-in becomes its vanilla. Each happens once (the position closes / the flag is set). Returns the tickets.
pub fn knocks(tx: &mut Tx, env: &Env, underlying: &str) -> Vec<i64> {
    let Some((s, _)) = env.options.spot(underlying) else { return vec![] };
    let spot = dec(s);
    if spot <= ZERO {
        return vec![];
    }
    let due: Vec<i64> = tx
        .st
        .positions
        .values()
        .filter(|p| p.option.as_ref().is_some_and(|t| t.underlying == underlying && env.now < t.expiry_at && t.alive_barrier().is_some_and(|b| b.kind.hit(spot, b.level))))
        .map(|p| p.ticket)
        .collect();
    let mut out = Vec::new();
    for ticket in due {
        let p = tx.st.positions[&ticket].clone();
        let mut t = p.option.clone().unwrap();
        let b = t.barrier.clone().unwrap();
        if b.kind.is_in() {
            t.barrier = Some(BarrierTerms { knocked_in: true, knocked_at: Some(env.now), knock_spot: Some(spot), ..b.clone() });
            let np = Position { option: Some(t.clone()), ..p.clone() };
            tx.emit(Event::PositionUpdated { position: np, change: "knocked_in".into(), deal: None });
            tx.note("knock_in", format!("#{ticket} {} knocked in at {} (barrier {})", t.series, spot.normalize(), b.level.normalize()), json!({"ticket": ticket, "spot": num(spot), "level": num(b.level), "options": true}));
            out.push(ticket);
        } else {
            let acc = tx.st.account.clone();
            // the rebate in USD at the knock (USDxxx: 1 / spot); without a rate a non-zero rebate waits for the next
            // tick instead of paying a quote-currency amount as if it were USD
            let Some(usdq) = usd_per_quote_at(env, &acc, &t, spot).or(b.rebate.is_zero().then_some(ONE)) else { continue };
            let cash = units(&p, &t) * b.rebate * usdq * acc.usd_factor();
            if exit(tx, env, &p, p.volume, b.rebate, cash, ZERO, usdq, Some(spot), None, None, DealReason::KnockOut, TxnKind::OptionSettlement, Counter::House(HOUSE_SETTLEMENT), Some(format!("knock:{ticket}")), None, &format!("knock-out at {}", spot.normalize())).is_ok() {
                out.push(ticket);
            }
        }
    }
    if !out.is_empty() {
        apply_nbp(tx, env);
    }
    out
}

/* ------------------------------------------------------------------ */
/* Settlement                                                          */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settled {
    pub tickets: Vec<i64>,
    /// Σ cash booked (account currency).
    pub cash: D,
}

/// Settles every position of expiry `key` (`SYMBOL:YYYY-MM-DD`) at `fixing`, run `run`: the payoff is paid (or
/// charged) in cash, the position closes with reason `expiry`. Idempotent: a settled position is gone, and the
/// ledger key `settle:{key}:{run}:{ticket}` can be booked once. `usdq` = the quote → USD rate when the
/// underlying's quote currency is neither USD nor its own (crosses).
pub fn settle(tx: &mut Tx, env: &Env, key: &str, fixing: D, run: i32, usdq: Option<D>) -> Settled {
    let mut out = Settled::default();
    if fixing <= ZERO {
        return out;
    }
    let due: Vec<i64> = tx.st.positions.values().filter(|p| p.option.as_ref().is_some_and(|t| t.expiry_key() == key && env.now >= t.expiry_at)).map(|p| p.ticket).collect();
    let acc = tx.st.account.clone();
    for ticket in due {
        let p = tx.st.positions[&ticket].clone();
        let t = p.option.clone().unwrap();
        let Some(q) = settle_usd_per_quote(env, &acc, &t, fixing, usdq) else { continue };
        let payout = settle_payoff(&t, fixing);
        // an order book position settles against its clearing account in USD, rounded once in USD (the cent
        // account gets exactly 100 ×, like a fill's premium), so a cent long and a USD short net to 0 per code
        let (cash, counter) = if p.on_book() { (r2(units(&p, &t) * payout * q) * acc.usd_factor(), Counter::Clearing(&t)) } else { (units(&p, &t) * payout * q * acc.usd_factor(), Counter::House(HOUSE_SETTLEMENT)) };
        let comment = format!("expiry fixing {} (run {run})", fixing.normalize());
        if let Ok(_) = exit(tx, env, &p, p.volume, payout, cash, ZERO, q, Some(fixing), Some(fixing), Some(run), DealReason::Expiry, TxnKind::OptionSettlement, counter, Some(format!("settle:{key}:{run}:{ticket}")), None, &comment) {
            out.tickets.push(ticket);
            out.cash += r2(cash);
        }
    }
    // pending orders on this expiry can never fill any more
    let stale: Vec<i64> = tx.st.orders.values().filter(|o| o.option.as_ref().is_some_and(|oo| oo.legs.iter().any(|l| l.terms.expiry_key() == key))).map(|o| o.ticket).collect();
    for t in stale {
        tx.emit(Event::OrderRemoved { ticket: t, status: OrderStatus::Expired, reason: "option expired".into(), at: env.now, fill_price: None, position_ticket: None });
    }
    if !out.tickets.is_empty() {
        apply_nbp(tx, env);
    }
    out
}

/// The payoff per unit at expiry. A barrier still alive whose level the FIXING itself has reached was touched in
/// the fixing window (the fixing is an average of the window's mids, so some mid reached the level), even if no
/// knock was seen (an engine outage, a gap): a knock-out pays its rebate, a knock-in pays as the vanilla it became.
pub fn settle_payoff(t: &OptionTerms, fixing: D) -> D {
    match &t.barrier {
        Some(b) if !b.kind.is_in() && b.kind.hit(fixing, b.level) => b.rebate,
        Some(b) if b.kind.is_in() && !b.knocked_in && b.kind.hit(fixing, b.level) => t.right.intrinsic(fixing, t.strike),
        _ => t.payoff(fixing),
    }
}

/// Re-runs a settlement at a corrected fixing: every earlier settlement deal of `key` is reversed (cash and deal),
/// its position reopened from the deal's snapshot and settled again at `fixing` / `run`. The client's balance moves
/// by exactly the difference.
pub fn rerun(tx: &mut Tx, env: &Env, key: &str, prev: &[Deal], fixing: D, run: i32, usdq: Option<D>) -> Result<Settled, Reject> {
    if fixing <= ZERO {
        return Err(rej("invalid_fixing", "The new fixing must be above 0"));
    }
    let mut reopened = 0;
    for d in prev {
        let Some(o) = &d.option else { continue };
        if d.reason != DealReason::Expiry || o.terms.expiry_key() != key || tx.st.reversed_deals.contains(&d.id) || d.login != tx.st.account.login {
            continue;
        }
        if o.run.is_some_and(|r| r >= run) {
            continue; // already at this run
        }
        let Some(snap) = d.snapshot.as_deref().cloned() else { continue };
        if tx.st.positions.contains_key(&snap.ticket) {
            continue;
        }
        if !o.cash.is_zero() {
            // reversed against the account it was settled against (the clearing account for a book position)
            let (idem, reference, note) = (format!("settle-rev:{key}:{}:{}", o.run.unwrap_or(0), d.position_ticket), Some(format!("deal:{}", d.id)), Some(format!("settlement re-run: fixing {} replaced", o.fixing.map(|x| x.normalize().to_string()).unwrap_or_default())));
            if snap.on_book() {
                post_clearing(tx, env, TxnKind::Reversal, idem, &o.terms, -o.cash, reference, Some("SETTLEMENT-RERUN".into()), note);
            } else {
                tx.post(env, TxnKind::Reversal, idem, "balance", HOUSE_SETTLEMENT, -o.cash, reference, Some("SETTLEMENT-RERUN".into()), note);
            }
        }
        tx.emit(Event::DealReversed { deal_id: d.id });
        tx.emit(Event::PositionUpdated { position: snap, change: format!("settlement re-run of deal {}", d.id), deal: None });
        reopened += 1;
    }
    if reopened == 0 {
        return Ok(Settled::default());
    }
    Ok(settle(tx, env, key, fixing, run, usdq))
}

/* ------------------------------------------------------------------ */
/* Void                                                                */
/* ------------------------------------------------------------------ */

/// Voids an erroneous option trade: every cash flow of the position (premium, proceeds, payouts) and every
/// commission it paid is reversed (shown as corrections), its deals are marked reversed and an open position is
/// removed. `deals` = the position's deals from the database.
pub fn void(tx: &mut Tx, env: &Env, ticket: i64, deals: &[Deal], dealer: &DealerCtx) -> Result<Value, Reject> {
    let mine: Vec<&Deal> = deals.iter().filter(|d| d.position_ticket == ticket && d.login == tx.st.account.login && d.option.is_some() && !tx.st.reversed_deals.contains(&d.id)).collect();
    let open = tx.st.positions.get(&ticket).cloned();
    if open.as_ref().is_some_and(|p| p.on_book()) || mine.iter().any(|d| d.option.as_ref().is_some_and(|o| o.fill.is_some())) {
        return Err(rej("book_venue", format!("#{ticket} traded on the options order book: bust the fill instead (both sides)")));
    }
    if open.as_ref().is_some_and(|p| p.option.is_none()) {
        return Err(rej("not_option", format!("#{ticket} is not an option position")));
    }
    if mine.is_empty() {
        return Err(rej("not_found", format!("Option trade #{ticket} not found (or already voided)")));
    }
    let mut cash_back = ZERO;
    let mut comm_back = ZERO;
    for d in &mine {
        let o = d.option.as_ref().unwrap();
        if !o.cash.is_zero() {
            let house = if matches!(d.reason, DealReason::Expiry | DealReason::KnockOut) { HOUSE_SETTLEMENT } else { HOUSE_PREMIUM };
            tx.post(env, TxnKind::Reversal, format!("void:{ticket}:{}", d.id), "balance", house, -o.cash, Some(format!("deal:{}", d.id)), Some(dealer.reason_code.clone()), Some(format!("void of option trade #{ticket}")));
            cash_back -= o.cash;
        }
        if !o.charged.is_zero() {
            tx.post(env, TxnKind::Reversal, format!("void:{ticket}:{}:commission", d.id), "balance", "commission", o.charged, Some(format!("deal:{}", d.id)), Some(dealer.reason_code.clone()), Some("void: commission refund".into()));
            comm_back += o.charged;
        }
        tx.emit(Event::DealReversed { deal_id: d.id });
    }
    if open.is_some() {
        tx.emit(Event::PositionRemoved { ticket, reason: format!("void by {}", dealer.staff) });
    }
    tx.note("void", format!("Option trade #{ticket} was voided by the dealing desk (correction)"), json!({"ticket": ticket, "cash": num(cash_back), "commission": num(comm_back), "options": true}));
    Ok(json!({"ticket": ticket, "deals": mine.iter().map(|d| d.id).collect::<Vec<_>>(), "cashReversed": num(cash_back), "commissionRefunded": num(comm_back), "wasOpen": open.is_some()}))
}

/* ------------------------------------------------------------------ */
/* Tick / timer evaluation                                             */
/* ------------------------------------------------------------------ */

/// Runs `f` on a copy of the transaction and keeps its events only if it succeeds.
fn attempt<T>(tx: &mut Tx, f: impl FnOnce(&mut Tx) -> Result<T, Reject>) -> Result<T, Reject> {
    let mut a = Tx::new(&tx.st);
    let out = f(&mut a)?;
    for e in a.events {
        tx.emit(e);
    }
    tx.notes.extend(a.notes);
    tx.audit.extend(a.audit);
    Ok(out)
}

/// Refusals that keep a pending order waiting (the market may allow it later).
fn transient(code: &str) -> bool {
    matches!(
        code,
        "market_closed" | "series_halted" | "stale_prices" | "no_price" | "options_disabled" | "close_only" | "trading_disabled" | "account_status" | "symbol_halted" | "symbol_close_only"
    )
}

/// Pending option order: trigger, then the premium limit; fills all legs at once.
fn evaluate_order(tx: &mut Tx, env: &Env, ticket: i64) {
    let Some(o) = tx.st.orders.get(&ticket).cloned() else { return };
    let Some(oo) = o.option.clone() else { return };
    if oo.book.is_some() {
        return; // an order-book stop: options_book::eval_stops
    }
    if o.expiry_at.is_some_and(|a| a <= env.now) {
        return; // expire_orders removes it
    }
    if let Some(tr) = &o.trigger
        && !o.triggered
    {
        let Some(px) = spot_of(env, &tx.st.account, &tr.symbol) else { return };
        if !tr.op.fired(px, tr.price) {
            return;
        }
        let armed = Order { triggered: true, ..o.clone() };
        tx.emit(Event::OrderUpdated { order: armed, change: "trigger_fired".into() });
        tx.note("order_triggered", format!("Order #{ticket}: {} is {} {}", tr.symbol, tr.op.as_str(), tr.price.normalize()), json!({"ticket": ticket, "options": true}));
    }
    let specs: Vec<LegSpec> = oo.legs.iter().map(|l| LegSpec { terms: l.terms.clone(), side: l.side, contracts: l.contracts }).collect();
    if o.kind == OrderType::Limit {
        let Some(lim) = oo.limit_premium else { return };
        let Ok(snap) = snapshot(env) else { return };
        let Ok((legs, _)) = price_open(env, &tx.st, &snap, &specs, &GateOpts { eligible: None, others: (ZERO, ZERO) }) else { return };
        if !limit_met(&legs, lim) {
            return;
        }
    }
    let meta = FillMeta { source: o.source, platform: o.platform.clone(), comment: o.comment.clone(), client_order_id: None, sl: o.sl, tp: o.tp, reason: DealReason::PendingFill, order_ticket: Some(ticket), staff: None };
    match attempt(tx, |t| fill_now(t, env, &specs, &GateOpts { eligible: None, others: (ZERO, ZERO) }, &meta)) {
        Ok(filled) => {
            let first = filled.legs.first().map(|l| (l.ticket, l.price));
            tx.emit(Event::OrderRemoved { ticket, status: OrderStatus::Filled, reason: "triggered".into(), at: env.now, fill_price: first.map(|x| x.1), position_ticket: first.map(|x| x.0) });
            tx.note("order_filled", format!("Options order #{ticket} filled"), json!({"ticket": ticket, "comboId": filled.combo_id, "positions": filled.legs.iter().map(|l| l.ticket).collect::<Vec<_>>(), "options": true}));
        }
        Err(e) if transient(e.code) => {}
        Err(e) => {
            tx.emit(Event::OrderRemoved { ticket, status: OrderStatus::Rejected, reason: e.message.clone(), at: env.now, fill_price: None, position_ticket: None });
            tx.note("order_rejected", format!("Options order #{ticket} could not be filled: {}", e.message), json!({"ticket": ticket, "code": e.code, "options": true}));
        }
    }
}

/// Premium SL / TP of the option positions on `underlying` (fresh prices).
fn premium_stops(tx: &mut Tx, env: &Env, underlying: &str) {
    let due: Vec<i64> = tx.st.positions.values().filter(|p| !p.on_book() && (p.sl.is_some() || p.tp.is_some()) && p.option.as_ref().is_some_and(|t| t.underlying == underlying)).map(|p| p.ticket).collect();
    // book positions: a reduce-only market order on the mark
    super::options_book::book_sltp(tx, env, underlying);
    for ticket in due {
        let p = tx.st.positions[&ticket].clone();
        let t = p.option.clone().unwrap();
        let Ok(q) = env.options.price(&env.tenant.slug, &env.group.code, &t, env.now) else { continue };
        let px = q.close_price(p.side);
        let hit = match p.side {
            Side::Buy => (p.sl.is_some_and(|s| px <= s), p.tp.is_some_and(|s| px >= s)),
            Side::Sell => (p.sl.is_some_and(|s| px >= s), p.tp.is_some_and(|s| px <= s)),
        };
        let reason = match hit {
            (true, _) => DealReason::Sl,
            (_, true) => DealReason::Tp,
            _ => continue,
        };
        let _ = attempt(tx, |x| close(x, env, ticket, OptClose { volume: None, reason, dealer: None, system: true, comment: format!("[{}]", reason.as_str()) }));
    }
}

/// Everything a raw tick on `underlying` can trigger: barrier knocks always; with `full` (at most every 250 ms per
/// account) also pending option orders, premium SL / TP and the margin check.
pub fn on_underlying(tx: &mut Tx, env: &Env, underlying: &str, full: bool) {
    knocks(tx, env, underlying);
    if !full {
        return;
    }
    super::risk::expire_orders(tx, env);
    let orders: Vec<i64> = tx
        .st
        .orders
        .values()
        .filter(|o| o.option.as_ref().is_some_and(|oo| oo.legs.iter().any(|l| l.terms.underlying == underlying)) || o.trigger.as_ref().is_some_and(|t| t.symbol == underlying))
        .map(|o| o.ticket)
        .collect();
    for t in orders {
        evaluate_order(tx, env, t);
    }
    super::options_book::eval_stops(tx, env, Some(underlying));
    premium_stops(tx, env, underlying);
    super::risk::check_margin(tx, env);
}

/// Periodic evaluation of an account with options (time decay changes marks and margin without a tick).
pub fn on_timer(tx: &mut Tx, env: &Env) {
    let keys = tx.st.option_keys();
    if keys.is_empty() {
        return;
    }
    for u in keys {
        knocks(tx, env, &u);
    }
    super::risk::expire_orders(tx, env);
    let orders: Vec<i64> = tx.st.orders.values().filter(|o| o.option.is_some() || o.trigger.is_some()).map(|o| o.ticket).collect();
    for t in orders {
        evaluate_order(tx, env, t);
    }
    super::options_book::eval_stops(tx, env, None);
    for u in tx.st.option_keys() {
        premium_stops(tx, env, &u);
    }
    super::risk::check_margin(tx, env);
}

/// Does a raw mid on `underlying` reach a live barrier of this account (a knock must not wait for the throttle)?
pub fn knock_due(st: &AccountState, underlying: &str, spot: D, now: DateTime<Utc>) -> bool {
    st.positions.values().any(|p| p.option.as_ref().is_some_and(|t| t.underlying == underlying && now < t.expiry_at && t.alive_barrier().is_some_and(|b| b.kind.hit(spot, b.level))))
}

/* ------------------------------------------------------------------ */
/* Stop-out by units                                                   */
/* ------------------------------------------------------------------ */

/// What stop-out closes together: a whole strategy (combo), one option position or one CFD position.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Unit {
    Combo(i64),
    Single(i64),
}

impl Unit {
    pub fn tickets(&self, st: &AccountState) -> Vec<i64> {
        match self {
            Unit::Combo(c) => st.positions.values().filter(|p| p.combo_id == Some(*c)).map(|p| p.ticket).collect(),
            Unit::Single(t) => vec![*t],
        }
    }
}

pub fn units_of(st: &AccountState) -> Vec<Unit> {
    let mut out: BTreeSet<Unit> = BTreeSet::new();
    for p in st.positions.values() {
        match p.combo_id {
            Some(c) if p.option.is_some() => out.insert(Unit::Combo(c)),
            _ => out.insert(Unit::Single(p.ticket)),
        };
    }
    out.into_iter().collect()
}

/// Closes a unit on a scratch copy of the transaction: Ok = its events (not yet applied) and what closed.
fn close_unit(tx: &Tx, env: &Env, unit: &Unit) -> Result<(Tx, Vec<Value>), Reject> {
    let tickets = unit.tickets(&tx.st);
    let acc = tx.st.account.clone();
    let mut x = Tx::new(&tx.st);
    let mut done = Vec::new();
    for t in tickets {
        let p = x.st.positions.get(&t).cloned().ok_or_else(|| rej("not_found", "gone"))?;
        if p.option.is_some() {
            let (deal, profit) = close(&mut x, env, t, OptClose { volume: None, reason: DealReason::StopOut, dealer: None, system: true, comment: "[so]".into() })?;
            done.push(json!({"ticket": t, "dealId": deal, "profit": num(profit)}));
        } else {
            let spec = env.spec(&p.symbol)?.clone();
            if !spec.is_open(env.now) {
                return Err(rej("market_closed", "market closed"));
            }
            let q = env.quote(&acc, &p.symbol).ok_or_else(|| rej("no_price", "no price"))?;
            let meta = super::trade::CloseMeta { comment: "[so]".into(), ..super::trade::CloseMeta::system(DealReason::StopOut) };
            let (deal, profit) = super::trade::close_part(&mut x, env, t, p.volume, q.close_price(p.side), meta)?;
            done.push(json!({"ticket": t, "dealId": deal, "profit": num(profit)}));
        }
    }
    Ok((x, done))
}

/// Stop-out of an account holding options: closes the unit that frees the most margin first (strategies close
/// all their legs together), until the margin level is above the stop-out level or nothing closable frees margin.
/// Nothing is recorded when nothing can be closed (a closed market, a series past its cut): the timer and the
/// ticks try again later.
pub fn stop_out(tx: &mut Tx, env: &Env) {
    let start = metrics(env, &tx.st).level;
    let mut closed: Vec<Value> = Vec::new();
    let mut failed: BTreeSet<Unit> = BTreeSet::new();
    for _ in 0..200 {
        let m = metrics(env, &tx.st);
        match m.level {
            Some(l) if l <= env.group.stop_out_pct => {}
            _ => break,
        }
        let acc = tx.st.account.clone();
        let mut best: Option<(D, D, Unit)> = None;
        for u in units_of(&tx.st).into_iter().filter(|u| !failed.contains(u)) {
            let tickets: BTreeSet<i64> = u.tickets(&tx.st).into_iter().collect();
            let freed = m.margin - super::margin_without(env, &tx.st, &tickets);
            if freed <= ZERO {
                continue;
            }
            let floating: D = tickets.iter().filter_map(|t| tx.st.positions.get(t)).filter_map(|p| super::position_floating(env, &acc, p)).sum();
            let better = match &best {
                None => true,
                Some((bf, bl, bu)) => freed > *bf || (freed == *bf && (floating < *bl || (floating == *bl && u < *bu))),
            };
            if better {
                best = Some((freed, floating, u));
            }
        }
        let Some((_, _, unit)) = best else { break };
        // order-book positions close on the book (book first within a band, then the Kalks backstop): the
        // liquidator does that once this transaction is committed (docs §8)
        if unit.tickets(&tx.st).iter().any(|t| tx.st.positions.get(t).is_some_and(|p| p.on_book())) {
            tx.liquidate = true;
            failed.insert(unit);
            continue;
        }
        match close_unit(tx, env, &unit) {
            Ok((x, v)) => {
                if closed.is_empty() {
                    tx.emit(Event::StopOut { level: start.map(r2) });
                }
                for e in x.events {
                    tx.emit(e);
                }
                tx.notes.extend(x.notes);
                closed.extend(v);
            }
            Err(_) => {
                failed.insert(unit);
            }
        }
    }
    if tx.liquidate && closed.is_empty() && !tx.st.margin_call {
        // the account is past its stop-out level with only order-book units left: flag it as in margin call so
        // the client sees it while the liquidator works
        let level = metrics(env, &tx.st).level;
        tx.emit(Event::MarginCall { entered: true, level: level.map(r2) });
    }
    if closed.is_empty() {
        return;
    }
    tx.note("stop_out", format!("Stop-out: {} position(s) closed at margin level {}%", closed.len(), start.map(|l| r2(l).normalize().to_string()).unwrap_or_default()), json!({"closed": closed}));
    apply_nbp(tx, env);
    if tx.st.margin_call && tx.st.positions.is_empty() {
        tx.emit(Event::MarginCall { entered: false, level: None });
    }
}

/* ------------------------------------------------------------------ */
/* Modify                                                              */
/* ------------------------------------------------------------------ */

/// Premium SL / TP of an option position (trailing stops are not available on options).
pub fn modify_position(tx: &mut Tx, env: &Env, ticket: i64, sl: Option<Option<D>>, tp: Option<Option<D>>, trailing: Option<Option<i64>>) -> Result<(Position, Position), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| rej("not_found", format!("Position #{ticket} not found")))?;
    let t = p.option.clone().ok_or_else(|| rej("not_option", "not an option position"))?;
    if trailing.is_some_and(|x| x.is_some()) {
        return Err(rej("invalid_trailing", "Trailing stops are not available on options"));
    }
    let nsl = sl.unwrap_or(p.sl).filter(|x| !x.is_zero());
    let ntp = tp.unwrap_or(p.tp).filter(|x| !x.is_zero());
    if nsl == p.sl && ntp == p.tp {
        return Err(rej("no_change", "Nothing changed"));
    }
    let q = env.options.price(&env.tenant.slug, &env.group.code, &t, env.now).map_err(price_err)?;
    check_premium_sltp(p.side, q.close_price(p.side), if nsl != p.sl { nsl } else { None }, if ntp != p.tp { ntp } else { None })?;
    let np = Position { sl: nsl, tp: ntp, ..p.clone() };
    tx.emit(Event::PositionUpdated { position: np.clone(), change: "modified".into(), deal: None });
    Ok((p, np))
}

/// A pending option order: the limit premium, SL / TP and expiry can change.
pub fn modify_order(tx: &mut Tx, env: &Env, ticket: i64, patch: &super::trade::OrderPatch) -> Result<(Order, Order), Reject> {
    let o = tx.st.orders.get(&ticket).cloned().ok_or_else(|| rej("not_found", format!("Order #{ticket} not found")))?;
    let mut oo = o.option.clone().ok_or_else(|| rej("not_option", "not an option order"))?;
    if patch.volume.is_some() || patch.stop_limit.is_some() || patch.trailing_points.is_some_and(|x| x.is_some()) {
        return Err(rej("invalid_order", "Only the limit premium, SL / TP and expiry of an options order can change"));
    }
    let mut n = o.clone();
    if let Some(p) = patch.price.filter(|p| *p > ZERO) {
        if o.kind != OrderType::Limit {
            return Err(rej("invalid_price", "Only limit orders have a limit premium"));
        }
        n.price = p;
        oo.limit_premium = Some(p);
    }
    if let Some(v) = patch.sl {
        n.sl = v;
    }
    if let Some(v) = patch.tp {
        n.tp = v;
    }
    if (n.sl.is_some() || n.tp.is_some()) && oo.legs.len() > 1 {
        return Err(rej("invalid_sl", "Premium stop loss / take profit is available for single-leg orders"));
    }
    if oo.legs.len() == 1 && o.kind == OrderType::Limit {
        check_premium_sltp(oo.legs[0].side, n.price, n.sl, n.tp)?;
    }
    if let Some(e) = patch.expiry {
        let last_open = n.expiry_at.unwrap_or(env.now);
        n.expiry = e;
        n.expiry_at = Some(match e {
            Expiry::Today => crate::specs::end_of_server_day(env.now).min(last_open),
            _ => last_open,
        });
    }
    n.option = Some(oo);
    if n == o {
        return Err(rej("no_change", "Nothing changed"));
    }
    tx.emit(Event::OrderUpdated { order: n.clone(), change: "modified".into() });
    Ok((o, n))
}

/* ------------------------------------------------------------------ */
/* Views                                                               */
/* ------------------------------------------------------------------ */

pub fn terms_json(t: &OptionTerms) -> Value {
    json!({
        "series": t.series,
        "underlying": t.underlying,
        "right": t.right.as_str(),
        "strike": num(t.strike),
        "expiry": t.expiry.to_string(),
        "expiryAt": t.expiry_at,
        "style": t.style(),
        "barrier": t.barrier.as_ref().map(|b| json!({"kind": b.kind.as_str(), "level": num(b.level), "rebate": num(b.rebate), "knockedIn": b.knocked_in, "knockedAt": b.knocked_at})),
        "contractSize": num(t.contract_size),
        "quoteCurrency": t.quote_ccy,
    })
}

pub fn greeks_json(g: Option<(f64, f64, f64, f64)>) -> Value {
    match g {
        Some((d, gm, v, th)) => json!({"delta": crate::options::pricing::round_to(d, 4), "gamma": crate::options::pricing::round_to(gm, 6), "vega": crate::options::pricing::round_to(v, 2), "theta": crate::options::pricing::round_to(th, 2)}),
        None => Value::Null,
    }
}

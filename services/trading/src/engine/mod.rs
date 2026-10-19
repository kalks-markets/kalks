//! The trading engine core: pure decision logic.
//!
//! Each operation takes the current `AccountState` and an `Env` (specs, tenant config, group, quotes,
//! clock, id allocator) and produces events through a `Tx`. `Tx::emit` applies every event to a working
//! copy immediately, so multi-step operations (stop-out closing several positions, netting reversal)
//! always see the state their own earlier events produced. Nothing here does IO: the shard persists the
//! events in one database transaction and only then swaps the working copy in.

pub mod corporate;
pub mod dealing;
pub mod funds;
pub mod options;
pub mod options_book;
pub mod risk;
pub mod trade;

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicI64, Ordering};

use crate::model::{Account, LedgerTxn, Posting, Side, TxnKind, acct_code, house_code};
use crate::money::{D, HUNDRED, ZERO, r2};
use crate::rules::{Group, TenantConfig};
use crate::specs::{Spec, Specs};
use crate::state::{AccountState, Event};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quote {
    pub bid: D,
    pub ask: D,
    /// provider time, ms
    pub t_ms: i64,
}

impl Quote {
    pub fn mid(&self) -> D {
        (self.bid + self.ask) / D::TWO
    }
    /// Price a position of `side` closes at now (buy closes at bid).
    pub fn close_price(&self, side: Side) -> D {
        match side {
            Side::Buy => self.bid,
            Side::Sell => self.ask,
        }
    }
    /// Price a new trade of `side` opens at now (buy opens at ask).
    pub fn open_price(&self, side: Side) -> D {
        match side {
            Side::Buy => self.ask,
            Side::Sell => self.bid,
        }
    }
}

/// Quotes for one spread group (market-data already applied the group markup).
pub trait Quotes: Send + Sync {
    fn get(&self, spread_group: &str, symbol: &str) -> Option<Quote>;
}

/// Global id sequences (single engine process; seeded from the database on start).
#[derive(Debug, Default)]
pub struct Ids {
    ticket: AtomicI64,
    deal: AtomicI64,
    txn: AtomicI64,
}

impl Ids {
    pub fn new(ticket: i64, deal: i64, txn: i64) -> Self {
        Self { ticket: AtomicI64::new(ticket), deal: AtomicI64::new(deal), txn: AtomicI64::new(txn) }
    }
    pub fn ticket(&self) -> i64 {
        self.ticket.fetch_add(1, Ordering::SeqCst) + 1
    }
    pub fn deal(&self) -> i64 {
        self.deal.fetch_add(1, Ordering::SeqCst) + 1
    }
    pub fn txn(&self) -> i64 {
        self.txn.fetch_add(1, Ordering::SeqCst) + 1
    }
}

/// Everything a decision may read besides the account itself.
#[derive(Clone, Copy)]
pub struct Env<'a> {
    pub specs: &'a Specs,
    pub tenant: &'a TenantConfig,
    pub group: &'a Group,
    pub quotes: &'a dyn Quotes,
    pub ids: &'a Ids,
    pub now: DateTime<Utc>,
    /// Quotes older than this are not tradable (stale feed, D116). 0 = no check.
    pub max_quote_age_ms: i64,
    /// Client restrictions (trading disabled, close-only) set in the Back Office; None = not checked.
    pub restrictions: Option<&'a crate::controls::Restrictions>,
    /// Kalks FX Options: snapshot, raw spots, prices, scenario margin (src/options).
    pub options: &'a dyn crate::options::OptionPricing,
    /// Corporate actions due but not yet applied everywhere (engine/corporate.rs); None = none.
    pub corp: Option<&'a corporate::CorpDue>,
    /// Risk desk (src/risk): exposure index, client scores, news windows, limit observations. None = no limits.
    pub risk: Option<&'a crate::risk::RiskCtx>,
}

impl Env<'_> {
    /// The corporate action on `symbol` this account still waits for (trading the symbol pauses meanwhile).
    pub fn corp_pending(&self, st: &AccountState, symbol: &str) -> Option<i64> {
        self.corp.and_then(|c| c.pending_for(st, symbol, self.now))
    }
}

impl Env<'_> {
    pub fn spec(&self, symbol: &str) -> Result<&Spec, Reject> {
        self.specs.get(symbol).ok_or_else(|| Reject::new("unknown_symbol", format!("Unknown symbol {symbol}")))
    }

    /// Group quote plus the account's dealer markup (pips, split half on each side).
    pub fn quote(&self, acc: &Account, symbol: &str) -> Option<Quote> {
        let q = self.quotes.get(&self.group.spread_group, symbol)?;
        if q.bid <= ZERO || q.ask <= ZERO {
            return None;
        }
        let m = acc.controls.markup_pips;
        if m.is_zero() {
            return Some(q);
        }
        let spec = self.specs.get(symbol)?;
        let half = spec.round_price(m * spec.pip_size / D::TWO);
        Some(Quote { bid: q.bid - half, ask: q.ask + half, ..q })
    }

    /// A tradable quote: present and fresh.
    pub fn live_quote(&self, acc: &Account, symbol: &str) -> Result<Quote, Reject> {
        let q = self.quote(acc, symbol).ok_or_else(|| Reject::new("no_price", format!("No price for {symbol}")))?;
        if self.max_quote_age_ms > 0 && self.now.timestamp_millis() - q.t_ms > self.max_quote_age_ms {
            return Err(Reject::new("stale_price", format!("The price feed for {symbol} is stale; trading is paused")));
        }
        Ok(q)
    }

    /// Converts `amount` in `ccy` to USD. `own` = (symbol, price) of the instrument being valued, used when it
    /// is itself the conversion pair (USDJPY P&L converts at its own closing price).
    pub fn to_usd(&self, acc: &Account, ccy: &str, amount: D, own: (&str, D)) -> Option<D> {
        if ccy == "USD" {
            return Some(amount);
        }
        let (pair, multiply) = self.specs.usd_pair(ccy)?;
        let rate = if pair == own.0 { own.1 } else { self.quote(acc, &pair)?.mid() };
        if rate <= ZERO {
            return None;
        }
        Some(if multiply { amount * rate } else { amount / rate })
    }

    pub fn commission_per_lot(&self, spec: &Spec) -> D {
        spec.commission_per_lot.unwrap_or(self.group.commission_per_lot)
    }
}

/// A rejected request.
#[derive(Clone, Debug, PartialEq)]
pub struct Reject {
    pub code: &'static str,
    pub message: String,
    /// Requote: the current bid/ask when the fill would exceed the client's max deviation (D106).
    pub requote: Option<(D, D)>,
}

impl Reject {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), requote: None }
    }
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

/// A notification for the terminal stream (fills, SL/TP hits, margin call, stop-out…). Not persisted
/// by itself — the underlying events are.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub kind: &'static str,
    pub message: String,
    pub data: Value,
}

/// A dealing audit draft; the shard stamps staff, reason and time and writes it with the events.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditDraft {
    pub action: &'static str,
    pub tickets: Vec<i64>,
    pub login: Option<i64>,
    pub symbol: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub flags: Vec<String>,
}

/// Working transaction over one account.
pub struct Tx {
    pub st: AccountState,
    pub events: Vec<Event>,
    pub notes: Vec<Note>,
    pub audit: Vec<AuditDraft>,
    /// The in-memory order-book state (`st.book`: working orders, reservations) changed: the shard swaps the
    /// state in even when there are no events.
    pub book_dirty: bool,
    /// Order-book commands to send once this transaction is committed (a stop that fired, an SL / TP).
    pub book_send: Vec<crate::book::Outgoing>,
    /// Stop-out could not bring the account back above the stop-out level by itself (order-book positions close
    /// on the book): the shard hands the account to the liquidator once this transaction is committed.
    pub liquidate: bool,
}

impl Tx {
    pub fn new(st: &AccountState) -> Self {
        Self { st: st.clone(), events: Vec::new(), notes: Vec::new(), audit: Vec::new(), book_dirty: false, book_send: Vec::new(), liquidate: false }
    }

    pub fn emit(&mut self, ev: Event) {
        self.st.apply(&ev);
        self.events.push(ev);
    }

    pub fn note(&mut self, kind: &'static str, message: impl Into<String>, data: Value) {
        self.notes.push(Note { kind, message: message.into(), data });
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Books a balanced ledger transaction between a client sub-ledger and a house account.
    /// Returns the txn id, or None when the amount rounds to 0.
    #[allow(clippy::too_many_arguments)]
    pub fn post(&mut self, env: &Env, kind: TxnKind, idem: String, sub: &str, house: &str, amount: D, reference: Option<String>, reason_code: Option<String>, note: Option<String>) -> Option<i64> {
        let amount = r2(amount);
        if amount.is_zero() {
            return None;
        }
        let ccy = self.st.account.ccy();
        let login = self.st.account.login;
        let txn = LedgerTxn {
            id: env.ids.txn(),
            tenant_id: self.st.account.tenant_id,
            idempotency_key: idem,
            kind,
            login,
            reference,
            reason_code,
            note,
            at: env.now,
            postings: vec![
                Posting { account: acct_code(login, sub), ccy: ccy.into(), amount },
                Posting { account: house_code(house, ccy), ccy: ccy.into(), amount: -amount },
            ],
        };
        debug_assert!(txn.is_balanced());
        let id = txn.id;
        self.emit(Event::Ledger { txn });
        Some(id)
    }

    /// Books a multi-leg transaction (legs: (ledger account, amount) in the account currency).
    pub fn post_legs(&mut self, env: &Env, kind: TxnKind, idem: String, legs: Vec<(String, D)>, reference: Option<String>) -> Option<i64> {
        let ccy = self.st.account.ccy();
        let postings: Vec<Posting> = legs.into_iter().map(|(a, v)| (a, r2(v))).filter(|(_, v)| !v.is_zero()).map(|(account, amount)| Posting { account, ccy: ccy.into(), amount }).collect();
        if postings.is_empty() {
            return None;
        }
        let txn = LedgerTxn { id: env.ids.txn(), tenant_id: self.st.account.tenant_id, idempotency_key: idem, kind, login: self.st.account.login, reference, reason_code: None, note: None, at: env.now, postings };
        assert!(txn.is_balanced(), "unbalanced ledger transaction");
        let id = txn.id;
        self.emit(Event::Ledger { txn });
        Some(id)
    }
}

/* ------------------------------------------------------------------ */
/* P&L and margin                                                      */
/* ------------------------------------------------------------------ */

/// Price P&L of `volume` lots in the account currency (unrounded).
pub fn pnl(env: &Env, acc: &Account, spec: &Spec, side: Side, volume: D, open: D, close: D) -> D {
    let q = (close - open) * side.sign() * volume * spec.contract_size;
    env.to_usd(acc, &spec.quote_ccy, q, (&spec.symbol, close)).unwrap_or(q) * acc.usd_factor()
}

/// Margin in account currency for `long` / `short` lots of one symbol at `price` (D14 hedged margin).
pub fn symbol_margin(env: &Env, acc: &Account, spec: &Spec, long: D, short: D, price: D) -> D {
    if long.is_zero() && short.is_zero() {
        return ZERO;
    }
    let lev = D::from(acc.leverage.min(spec.max_leverage).max(1));
    let per_lot_quote = spec.contract_size * price;
    let per_lot_usd = env.to_usd(acc, &spec.quote_ccy, per_lot_quote, (&spec.symbol, price)).unwrap_or(per_lot_quote);
    let hedged = long.min(short);
    let net = (long - short).abs();
    let lots = net + D::TWO * hedged * env.group.hedged_margin_pct / HUNDRED;
    per_lot_usd * spec.margin_pct / HUNDRED / lev * lots * acc.usd_factor()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub balance: D,
    pub credit: D,
    pub bonus: D,
    /// Floating P&L of open positions: CFD price P&L plus the unrealised P&L of options (mark vs premium).
    pub profit: D,
    /// Accrued swap of open positions.
    pub swap: D,
    /// Balance + credit + bonus + CFD price P&L + swap + `option_value`.
    pub equity: D,
    pub margin: D,
    pub free_margin: D,
    /// % (None when no margin is used).
    pub level: Option<D>,
    /// Signed market value of the option positions at their mark (long +, short −).
    pub option_value: D,
    /// Unrealised P&L of the option positions (part of `profit`).
    pub option_pnl: D,
    /// Scenario margin of the option positions (part of `margin`).
    pub option_margin: D,
    /// Option settlement proceeds still in the re-run hold (not withdrawable yet).
    pub held: D,
    /// CFD positions that could not be valued (no quote): their P&L is missing from `profit`.
    pub unpriced: usize,
    /// Reserved for working options order-book orders (docs/OPTIONS-EXCHANGE.md §3): part of neither margin nor
    /// equity, but subtracted from free margin, free cash and withdrawable. 0 when no book order works.
    pub order_reserve: D,
}

impl Metrics {
    /// What can be transferred out: the balance not needed as margin or reserved for working book orders and not
    /// made of credit/bonus, minus option settlement proceeds still in the re-run hold.
    pub fn withdrawable(&self) -> D {
        let free_own = self.free_margin - self.credit - self.bonus;
        ((self.balance - self.order_reserve).min(free_own) - self.held).max(ZERO)
    }
}

/// CFD symbol volumes per side (option positions and `exclude` left out), with an optional hypothetical trade.
fn exposure(st: &AccountState, extra: Option<(&str, Side, D)>, exclude: &BTreeSet<i64>) -> BTreeMap<String, (D, D)> {
    let mut m: BTreeMap<String, (D, D)> = Default::default();
    for p in st.positions.values().filter(|p| p.option.is_none() && !exclude.contains(&p.ticket)) {
        let e = m.entry(p.symbol.clone()).or_insert((ZERO, ZERO));
        match p.side {
            Side::Buy => e.0 += p.volume,
            Side::Sell => e.1 += p.volume,
        }
    }
    if let Some((sym, side, v)) = extra {
        let e = m.entry(sym.to_string()).or_insert((ZERO, ZERO));
        match side {
            Side::Buy => e.0 += v,
            Side::Sell => e.1 += v,
        }
        if st.account.mode == crate::model::Mode::Netting {
            let net = e.0 - e.1;
            *e = (net.max(ZERO), (-net).max(ZERO));
        }
    }
    m
}

fn cfd_margin(env: &Env, st: &AccountState, exp: &BTreeMap<String, (D, D)>) -> D {
    let acc = &st.account;
    let mut margin = ZERO;
    for (sym, (long, short)) in exp {
        let Some(spec) = env.specs.get(sym) else { continue };
        let price = match env.quote(acc, sym) {
            Some(q) => q.mid(),
            None => st.positions.values().find(|p| &p.symbol == sym).map(|p| p.open_price).unwrap_or(ZERO),
        };
        margin += symbol_margin(env, acc, spec, *long, *short, price);
    }
    margin
}

/// CFD margin plus option scenario margin (with same-underlying CFD offsets), with an optional hypothetical CFD trade.
pub fn total_margin(env: &Env, st: &AccountState, extra: Option<(&str, Side, D)>) -> D {
    let none = BTreeSet::new();
    let exp = exposure(st, extra, &none);
    cfd_margin(env, st, &exp) + options::margin(env, st, &exp, &[], &none)
}

/// Margin of the account without the positions in `exclude` (what closing them would free).
pub fn margin_without(env: &Env, st: &AccountState, exclude: &BTreeSet<i64>) -> D {
    let exp = exposure(st, None, exclude);
    cfd_margin(env, st, &exp) + options::margin(env, st, &exp, &[], exclude)
}

pub fn metrics(env: &Env, st: &AccountState) -> Metrics {
    let acc = &st.account;
    let mut profit = ZERO;
    let mut swap = ZERO;
    let mut option_value = ZERO;
    let mut option_pnl = ZERO;
    let mut unpriced = 0;
    for p in st.positions.values() {
        swap += p.swap;
        if let Some(t) = &p.option {
            // never dropped: position_value falls back to the fixing, the intrinsic value or the premium
            let v = options::position_value(env, acc, p, t);
            option_value += v;
            option_pnl += v + p.premium;
            continue;
        }
        let (Some(spec), Some(q)) = (env.specs.get(&p.symbol), env.quote(acc, &p.symbol)) else {
            unpriced += 1;
            continue;
        };
        profit += pnl(env, acc, spec, p.side, p.volume, p.open_price, q.close_price(p.side));
    }
    let none = BTreeSet::new();
    let exp = exposure(st, None, &none);
    let cfd = cfd_margin(env, st, &exp);
    let option_margin = options::margin(env, st, &exp, &[], &none);
    let margin = cfd + option_margin;
    let equity = st.balance + st.credit + st.bonus + profit + swap + option_value;
    let order_reserve = st.book.reserve();
    Metrics {
        balance: st.balance,
        credit: st.credit,
        bonus: st.bonus,
        profit: profit + option_pnl,
        swap,
        equity,
        margin,
        free_margin: equity - margin - order_reserve,
        // the margin level stays equity / position margin
        level: if margin > ZERO { Some(equity / margin * HUNDRED) } else { None },
        option_value,
        option_pnl,
        option_margin,
        held: st.held(env.now),
        unpriced,
        order_reserve,
    }
}

/// Floating P&L of one position including swap (account currency); options: mark vs premium.
pub fn position_floating(env: &Env, acc: &Account, p: &crate::model::Position) -> Option<D> {
    if let Some(t) = &p.option {
        return Some(options::position_value(env, acc, p, t) + p.premium);
    }
    let spec = env.specs.get(&p.symbol)?;
    let q = env.quote(acc, &p.symbol)?;
    Some(pnl(env, acc, spec, p.side, p.volume, p.open_price, q.close_price(p.side)) + p.swap)
}

#[cfg(test)]
pub mod testkit;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_options;

#[cfg(test)]
mod tests_book;

#[cfg(test)]
mod tests_catalogue;

#[cfg(test)]
mod tests_corporate;

#[cfg(test)]
mod tests_risk;

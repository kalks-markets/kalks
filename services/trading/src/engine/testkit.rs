//! Test fixtures: in-memory quotes, a tenant with groups, account factory, and an `Engine` harness that
//! applies transactions the way the shard does (minus the database).

use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::{Env, Ids, Quote, Quotes, Tx};
use crate::model::{Account, AccountKind, Book, Controls, DemoCfg, Mode, Status};
use crate::money::D;
use crate::rules::{Group, TenantConfig, TenantPolicy};
use crate::specs::Specs;
use crate::state::{AccountState, Event};

pub fn d(s: &str) -> D {
    D::from_str(s).unwrap()
}

pub fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[derive(Default)]
pub struct MapQuotes(pub Mutex<HashMap<String, Quote>>);

impl Quotes for MapQuotes {
    fn get(&self, _group: &str, symbol: &str) -> Option<Quote> {
        self.0.lock().unwrap().get(symbol).copied()
    }
}

pub fn group(code: &str, mode: Mode, cent: bool) -> Group {
    Group {
        tenant_id: 1,
        code: code.into(),
        name: code.into(),
        mode,
        cent,
        account_types: "both".into(),
        leverages: vec![50, 100, 200, 500],
        default_leverage: 100,
        margin_call_pct: d("100"),
        stop_out_pct: d("50"),
        hedged_margin_pct: d("50"),
        min_deposit: d("0"),
        swap_free: false,
        commission_per_lot: d("0"),
        route: Book::B,
        spread_group: "standard".into(),
        max_accounts_per_user: 5,
        demo_initial_balance: d("10000"),
        demo_refills_per_day: 2,
        demo_expiry_days: 10,
        enabled: true,
        product: crate::rules::Product::Cfd,
    }
}

/// An Options group (`product = options`): what option tests open their accounts in.
pub fn options_group(code: &str, mode: Mode, cent: bool) -> Group {
    Group { product: crate::rules::Product::Options, ..group(code, mode, cent) }
}

pub struct Kit {
    pub specs: Specs,
    pub tenant: TenantConfig,
    pub quotes: MapQuotes,
    pub ids: Ids,
    pub now: DateTime<Utc>,
    pub restrictions: crate::controls::Restrictions,
    pub options: FixedPricer,
    pub corp: super::corporate::CorpDue,
}

/* ------------------------------------------------------------------ */
/* Options: a pricer with fixed prices over a test snapshot            */
/* ------------------------------------------------------------------ */

use crate::model::OptionTerms;
use crate::options::{OptPrice, OptSnapshot, OptionPricing, PriceError, ScenLeg, ScenOut, compute_price, compute_scenario};

/// Options pricer for tests: a snapshot (`opt_snapshot`), raw spots set by the test, and optional fixed
/// bid / ask per series (the mark is their middle) so money assertions are exact. Without a fixed price a series
/// is priced by the real model; scenario margin always runs the real optmath grid.
#[derive(Default)]
pub struct FixedPricer {
    pub snap: Mutex<Option<Arc<OptSnapshot>>>,
    pub spots: Mutex<HashMap<String, (f64, i64)>>,
    pub prices: Mutex<HashMap<String, (D, D)>>,
    pub stale: AtomicBool,
    /// Order book top of book (tests publish into it to check the mark clamp).
    pub top: Arc<crate::book::md::Top>,
}

impl FixedPricer {
    pub fn set_snapshot(&self, v: serde_json::Value) {
        *self.snap.lock().unwrap() = Some(Arc::new(OptSnapshot::from_json(v).unwrap()));
    }
    pub fn spot(&self, symbol: &str, mid: &str, t: DateTime<Utc>) {
        self.spots.lock().unwrap().insert(symbol.into(), (d(mid).to_string().parse().unwrap(), t.timestamp_millis()));
    }
    /// Fixes the bid / ask of a series (every barrier variant of it too).
    pub fn fix(&self, series: &str, bid: &str, ask: &str) {
        self.prices.lock().unwrap().insert(series.into(), (d(bid), d(ask)));
    }
    pub fn unfix(&self, series: &str) {
        self.prices.lock().unwrap().remove(series);
    }
}

impl OptionPricing for FixedPricer {
    fn snapshot(&self) -> Option<Arc<OptSnapshot>> {
        self.snap.lock().unwrap().clone()
    }
    fn stale(&self, _now: DateTime<Utc>) -> bool {
        self.snapshot().is_none() || self.stale.load(Ordering::SeqCst)
    }
    fn spot(&self, symbol: &str) -> Option<(f64, i64)> {
        self.spots.lock().unwrap().get(symbol).copied()
    }
    fn price(&self, tenant: &str, group: &str, terms: &OptionTerms, now: DateTime<Utc>) -> Result<OptPrice, PriceError> {
        let snap = self.snapshot().ok_or(PriceError::NoVol)?;
        let fixed = self.prices.lock().unwrap().get(&terms.series).copied();
        let mut p = compute_price(&snap, &|s| OptionPricing::spot(self, s), &|c| self.usd_per(c), tenant, group, terms, now)?;
        if let Some((bid, ask)) = fixed {
            p.bid = bid;
            p.ask = ask;
            p.mark = (bid + ask) / D::TWO;
        }
        Ok(p)
    }
    fn scenario(&self, tenant: &str, underlying: &str, legs: &[ScenLeg], cfd_units: f64, now: DateTime<Utc>) -> Option<ScenOut> {
        let snap = self.snapshot()?;
        compute_scenario(&snap, &|s| OptionPricing::spot(self, s), &|c| self.usd_per(c), tenant, underlying, legs, cfd_units, now)
    }
    fn book_top(&self, tenant: &str, kind: crate::model::AccountKind, series: &str) -> Option<crate::book::md::TopQuote> {
        self.top.get(tenant, kind, series)
    }
}

/// Test snapshot: EURUSD (GK, 10 000 EUR per contract) and USDJPY (GK, quote JPY) with a flat-ish surface, both
/// tenants switches on, expiry 1 = Friday 2026-10-02 14:00 UTC, expiry 2 = Monday 2026-09-28 14:00 UTC (the kit's
/// "today"); strikes 1.1500–1.1700 (EURUSD) and 148–152 (USDJPY), calls and puts.
pub fn opt_snapshot() -> serde_json::Value {
    let mut series = Vec::new();
    for (id, date) in [(1, "20261002"), (2, "20260928")] {
        for k in ["1.1500", "1.1550", "1.1600", "1.1650", "1.1700"] {
            for (kind, c) in [("call", "C"), ("put", "P")] {
                series.push(serde_json::json!({"code": format!("EURUSD-{date}-{k}-{c}"), "symbol": "EURUSD", "expiryId": id, "strike": k.parse::<f64>().unwrap(), "kind": kind, "status": "active"}));
            }
        }
    }
    for k in ["148.00", "150.00", "152.00"] {
        for (kind, c) in [("call", "C"), ("put", "P")] {
            series.push(serde_json::json!({"code": format!("USDJPY-20261002-{k}-{c}"), "symbol": "USDJPY", "expiryId": 3, "strike": k.parse::<f64>().unwrap(), "kind": kind, "status": "active"}));
        }
    }
    let und = |sym: &str, base: &str, quote: &str, digits: i32| {
        serde_json::json!({"symbol": sym, "model": "gk", "baseCcy": base, "quoteCcy": quote, "calendarCodes": [base, quote], "contractSize": 10000, "digits": digits,
            "pipSize": 0.0001, "cutTime": "10:00", "cutZone": "America/New_York", "noOpenMinutes": 15, "closeOnlyMinutes": 1, "weekendVolWeight": 0.15,
            "holidayVolWeight": 0.5, "priceScan": 0.03, "volScan": 0.03, "extremeMultiple": 3, "extremeCover": 0.35, "minContracts": 1, "maxContracts": 100,
            "contractStep": 1, "barriersEnabled": true, "enabled": true, "deltaConvention": "spot"})
    };
    serde_json::json!({
        "version": 1, "staleAfterSecs": 300,
        "underlyings": [und("EURUSD", "EUR", "USD", 5), und("USDJPY", "USD", "JPY", 3)],
        "rates": [{"ccy": "USD", "rate": 0.04}, {"ccy": "EUR", "rate": 0.02}, {"ccy": "JPY", "rate": 0.005}],
        "holidays": {"USD": [], "EUR": [], "JPY": []},
        "surfaces": [
            {"symbol": "EURUSD", "version": 1, "blendWeight": 1.0, "pillars": [{"tenor": "1W", "days": 7, "atm": 0.07, "rr25": -0.002, "bf25": 0.002}, {"tenor": "1M", "days": 30, "atm": 0.075, "rr25": -0.003, "bf25": 0.0025}]},
            {"symbol": "USDJPY", "version": 1, "blendWeight": 1.0, "pillars": [{"tenor": "1W", "days": 7, "atm": 0.1, "rr25": -0.01, "bf25": 0.003}, {"tenor": "1M", "days": 30, "atm": 0.1, "rr25": -0.01, "bf25": 0.003}]}
        ],
        "expiries": [
            {"id": 1, "symbol": "EURUSD", "expiryDate": "2026-10-02", "cutAt": "2026-10-02T14:00:00Z", "status": "listed", "fixingRun": 0},
            {"id": 2, "symbol": "EURUSD", "expiryDate": "2026-09-28", "cutAt": "2026-09-28T14:00:00Z", "status": "listed", "fixingRun": 0},
            {"id": 3, "symbol": "USDJPY", "expiryDate": "2026-10-02", "cutAt": "2026-10-02T14:00:00Z", "status": "listed", "fixingRun": 0}
        ],
        "series": series,
        "tenants": [{"tenant": "kalks", "enabledDemo": true, "enabledLive": true}],
        "groups": [{"tenant": "kalks", "groupCode": "*", "symbol": "*", "volSpread": 0.004, "minSpreadUsd": 0.5, "commissionPerContract": 0.25,
                    "commissionCapPct": 10, "maxContractsPerClient": 200, "weekendMarginPct": 25, "enabled": true}],
        "controls": [],
        "clientLimits": []
    })
}

impl Kit {
    pub fn new() -> Self {
        let mut tenant = TenantConfig { tenant_id: 1, slug: "kalks".into(), policy: TenantPolicy::default(), ..Default::default() };
        for g in [group("hedge", Mode::Hedging, false), group("net", Mode::Netting, false), group("cent", Mode::Hedging, true)] {
            tenant.groups.insert(g.code.clone(), g);
        }
        let mut free = group("free", Mode::Hedging, false);
        free.swap_free = true;
        tenant.groups.insert("free".into(), free);
        let mut ecn = group("ecn", Mode::Hedging, false);
        ecn.commission_per_lot = d("7");
        tenant.groups.insert("ecn".into(), ecn);
        // Options accounts (CFD / Options account split): `opt` (USD) and `opt-cent` (USC)
        for g in [options_group("opt", Mode::Hedging, false), options_group("opt-cent", Mode::Hedging, true)] {
            tenant.groups.insert(g.code.clone(), g);
        }
        // Monday 2026-09-28 12:00 UTC: FX open
        let now = t("2026-09-28T12:00:00Z");
        let options = FixedPricer::default();
        options.set_snapshot(opt_snapshot());
        options.spot("EURUSD", "1.16", now);
        options.spot("USDJPY", "150", now);
        Self { specs: crate::specs::test_specs(), tenant, quotes: MapQuotes::default(), ids: Ids::new(1000, 5000, 9000), now, restrictions: Default::default(), options, corp: Default::default() }
    }

    pub fn quote(&self, symbol: &str, bid: &str, ask: &str) {
        self.quotes.0.lock().unwrap().insert(symbol.into(), Quote { bid: d(bid), ask: d(ask), t_ms: self.now.timestamp_millis() });
    }

    pub fn env<'a>(&'a self, st: &AccountState) -> Env<'a> {
        Env {
            specs: &self.specs,
            tenant: &self.tenant,
            group: &self.tenant.groups[&st.account.group],
            quotes: &self.quotes,
            ids: &self.ids,
            now: self.now,
            max_quote_age_ms: 0,
            restrictions: Some(&self.restrictions),
            options: &self.options,
            corp: Some(&self.corp),
        }
    }

    pub fn account(&self, login: i64, group: &str, kind: AccountKind) -> Account {
        let g = &self.tenant.groups[group];
        Account {
            tenant_id: 1,
            login,
            user_id: 7,
            kind,
            group: group.into(),
            mode: g.mode,
            cent: g.cent,
            leverage: 100,
            status: Status::Active,
            name: "Test".into(),
            route_override: None,
            controls: Controls::default(),
            demo: (kind == AccountKind::Demo).then(|| DemoCfg { initial_balance: g.demo_initial_balance, refills_per_day: g.demo_refills_per_day, expiry_days: 10 }),
            created_at: self.now,
            lifecycle: None,
        }
    }
}

/// An account plus its full event log, mutated only through committed transactions.
pub struct Harness {
    pub st: AccountState,
    pub log: Vec<Event>,
}

impl Harness {
    /// Live account funded with `balance` (USD) through a wallet transfer.
    pub fn live(kit: &Kit, group: &str, balance: &str) -> Self {
        let acc = kit.account(10_000_001, group, AccountKind::Live);
        let st0 = AccountState::new(acc.clone());
        let tx = super::funds::open_account(&kit.env(&st0), acc);
        let mut h = Harness { st: tx.st.clone(), log: tx.events.clone() };
        if balance != "0" {
            h.run(kit, |tx, env| super::funds::transfer(tx, env, super::funds::Direction::In, d(balance), "fund-1", None).map(|_| ())).unwrap();
        }
        h
    }

    pub fn demo(kit: &Kit, group: &str) -> Self {
        let acc = kit.account(50_000_001, group, AccountKind::Demo);
        let st0 = AccountState::new(acc.clone());
        let tx = super::funds::open_account(&kit.env(&st0), acc);
        Harness { st: tx.st.clone(), log: tx.events.clone() }
    }

    /// Runs `f` in a transaction and commits it on success.
    pub fn run<T>(&mut self, kit: &Kit, f: impl FnOnce(&mut Tx, &Env) -> Result<T, super::Reject>) -> Result<T, super::Reject> {
        let env = kit.env(&self.st);
        let mut tx = Tx::new(&self.st);
        let out = f(&mut tx, &env)?;
        self.commit(tx);
        Ok(out)
    }

    pub fn commit(&mut self, tx: Tx) {
        for e in &tx.events {
            if let Event::Ledger { txn } = e {
                assert!(txn.is_balanced(), "unbalanced txn {txn:?}");
            }
        }
        self.log.extend(tx.events);
        self.st = tx.st;
    }

    pub fn tick(&mut self, kit: &Kit, symbol: &str) {
        let env = kit.env(&self.st);
        let mut tx = Tx::new(&self.st);
        super::risk::on_tick(&mut tx, &env, symbol);
        self.commit(tx);
    }

    /// Replaying the log must give exactly the current state.
    pub fn assert_replay(&self) {
        let replayed = AccountState::replay(self.log.iter()).unwrap().unwrap();
        assert_eq!(replayed, self.st, "replay diverged");
    }

    /// Σ postings on the client balance ledger account == balance.
    pub fn assert_ledger(&self) {
        let login = self.st.account.login;
        let mut bal = D::ZERO;
        for e in &self.log {
            if let Event::Ledger { txn } = e {
                assert!(txn.is_balanced());
                bal += txn.effect(login, "balance");
            }
        }
        assert_eq!(bal, self.st.balance);
    }
}

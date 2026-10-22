//! Programme settings and IB levels (admin-configurable, stored per tenant), with the seeded defaults.

use crate::money::{D, HUNDRED, ZERO, dec};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SymbolGroup {
    pub key: String,
    pub name: String,
    /// Symbols of an asset class from config/instruments.json (`forex`, `metals`, …) fall into this group.
    #[serde(default)]
    pub asset_class: Option<String>,
    /// Explicit symbols; they win over the asset-class rule (e.g. forex majors).
    #[serde(default)]
    pub symbols: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CpaRules {
    pub enabled: bool,
    /// The client's first live deposit must be at least this much (USD).
    pub min_first_deposit: D,
    /// A first qualifying live trade is also required.
    pub require_first_trade: bool,
    /// CPA becomes payable this many days after it is earned.
    pub hold_days: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PayoutRules {
    /// `daily` | `weekly` | `monthly`
    pub schedule: String,
    /// Weekly: ISO weekday the period closes on (1 = Monday).
    pub weekday: u32,
    /// Monthly: day of month the period closes on (1–28).
    pub month_day: u32,
    /// Payees below this net amount are carried over to the next batch.
    pub min_amount: D,
    /// Create a batch for approval automatically when a period closes.
    pub auto_create: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SelfReferral {
    /// Each signal: `block` (no commission from that client until an admin clears the flag), `flag` (flag only), `off`.
    pub ip: String,
    pub device: String,
    pub identity: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WashRules {
    pub enabled: bool,
    /// Opposite trades on the same symbol opened and closed within this many seconds of each other.
    pub window_secs: i64,
    /// Volumes within this % of each other.
    pub volume_tolerance_pct: D,
    /// Short-trade pattern: at least this many sub-minimum trades in 24 h …
    pub short_trades_min: i64,
    /// … making up at least this % of the client's trades.
    pub short_trades_pct: D,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Share of the base per-lot rate paid at each tier (tier 1 = direct clients). N tiers = the list length.
    pub tiers: Vec<D>,
    /// Trades held for less than this don't qualify (D59).
    pub min_trade_seconds: i64,
    /// Cent accounts: 1 cent lot counts as this many standard lots.
    pub cent_lot_factor: D,
    /// Trading groups whose deals never earn commission (e.g. prop).
    pub excluded_groups: Vec<String>,
    pub symbol_groups: Vec<SymbolGroup>,
    pub cpa: CpaRules,
    pub payout: PayoutRules,
    /// Upper bounds for what an IB may give away (D60), in % of its own amount.
    pub max_rebate_pct: D,
    pub max_split_pct: D,
    /// `full`: IBs see client names, emails and trades (consent in the sign-up T&C); `masked`: initials and lots only (D61).
    pub client_visibility: String,
    pub self_referral: SelfReferral,
    pub wash: WashRules,
    /// Monthly evaluation may also move an IB down (off by default: promotions only).
    pub allow_demotion: bool,
    /// Public link base for referral links, e.g. `https://app.kalkstrade.com`.
    pub link_base: String,
    /// Four-eyes (USD): a payout batch whose total is at or above this needs a second staff member's approval
    /// before anything is paid. 0 = every batch needs two approvers.
    #[serde(default = "default_four_eyes_threshold")]
    pub four_eyes_threshold: D,
}

/// Default four-eyes threshold (USD) for a new programme or settings saved before the field existed.
pub const DEFAULT_FOUR_EYES_THRESHOLD: &str = "1000";

fn default_four_eyes_threshold() -> D {
    dec(DEFAULT_FOUR_EYES_THRESHOLD)
}

impl Default for Settings {
    fn default() -> Self {
        let g = |key: &str, name: &str, ac: Option<&str>, symbols: &[&str]| SymbolGroup {
            key: key.into(),
            name: name.into(),
            asset_class: ac.map(str::to_string),
            symbols: symbols.iter().map(|s| s.to_string()).collect(),
        };
        Settings {
            tiers: vec![dec("100"), dec("20"), dec("10")],
            min_trade_seconds: 120,
            cent_lot_factor: dec("0.01"),
            excluded_groups: vec!["prop".into()],
            symbol_groups: vec![
                g("fx-major", "Forex majors", None, &["EURUSD", "GBPUSD", "USDJPY", "AUDUSD", "USDCAD", "USDCHF", "NZDUSD"]),
                g("fx-minor", "Forex minors & crosses", Some("forex"), &[]),
                g("metals", "Metals", Some("metals"), &[]),
                g("indices", "Indices", Some("indices"), &[]),
                g("energies", "Energies", Some("energies"), &[]),
                g("crypto", "Crypto", Some("crypto"), &[]),
                g("stocks", "Stocks", Some("stocks"), &[]),
            ],
            cpa: CpaRules { enabled: true, min_first_deposit: dec("500"), require_first_trade: true, hold_days: 30 },
            payout: PayoutRules { schedule: "weekly".into(), weekday: 1, month_day: 1, min_amount: dec("10"), auto_create: true },
            max_rebate_pct: dec("50"),
            max_split_pct: dec("50"),
            client_visibility: "full".into(),
            self_referral: SelfReferral { ip: "block".into(), device: "block".into(), identity: "block".into() },
            wash: WashRules { enabled: true, window_secs: 60, volume_tolerance_pct: dec("10"), short_trades_min: 10, short_trades_pct: dec("50") },
            allow_demotion: false,
            link_base: "https://app.kalkstrade.com".into(),
            four_eyes_threshold: default_four_eyes_threshold(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), (&'static str, String)> {
        if self.tiers.is_empty() || self.tiers.len() > 10 {
            return Err(("tiers", "Set between 1 and 10 tiers.".into()));
        }
        if self.tiers.iter().any(|t| *t < ZERO || *t > HUNDRED) {
            return Err(("tiers", "Each tier share must be between 0 and 100%.".into()));
        }
        if !(0..=86_400).contains(&self.min_trade_seconds) {
            return Err(("minTradeSeconds", "Minimum trade duration must be 0–86400 seconds.".into()));
        }
        if self.cent_lot_factor <= ZERO || self.cent_lot_factor > dec("1") {
            return Err(("centLotFactor", "Cent lot factor must be above 0 and at most 1.".into()));
        }
        if self.symbol_groups.is_empty() {
            return Err(("symbolGroups", "Define at least one symbol group.".into()));
        }
        let mut keys = std::collections::HashSet::new();
        for g in &self.symbol_groups {
            if !valid_key(&g.key) || !keys.insert(g.key.clone()) {
                return Err(("symbolGroups", format!("Symbol group key '{}' is invalid or repeated.", g.key)));
            }
            if g.name.trim().is_empty() || g.name.len() > 60 {
                return Err(("symbolGroups", "Every symbol group needs a name (≤ 60 chars).".into()));
            }
        }
        if self.cpa.min_first_deposit < ZERO || !(0..=365).contains(&self.cpa.hold_days) {
            return Err(("cpa", "CPA minimum deposit must be ≥ 0 and the hold period 0–365 days.".into()));
        }
        if !matches!(self.payout.schedule.as_str(), "daily" | "weekly" | "monthly") {
            return Err(("payout.schedule", "Payout schedule must be daily, weekly or monthly.".into()));
        }
        if !(1..=7).contains(&self.payout.weekday) || !(1..=28).contains(&self.payout.month_day) {
            return Err(("payout", "Weekday must be 1–7 and the month day 1–28.".into()));
        }
        if self.payout.min_amount < ZERO {
            return Err(("payout.minAmount", "Minimum payout must be ≥ 0.".into()));
        }
        for (f, v) in [("maxRebatePct", self.max_rebate_pct), ("maxSplitPct", self.max_split_pct)] {
            if v < ZERO || v > HUNDRED {
                return Err((f, "Must be between 0 and 100%.".into()));
            }
        }
        if !matches!(self.client_visibility.as_str(), "full" | "masked") {
            return Err(("clientVisibility", "Client visibility must be full or masked.".into()));
        }
        for v in [&self.self_referral.ip, &self.self_referral.device, &self.self_referral.identity] {
            if !matches!(v.as_str(), "block" | "flag" | "off") {
                return Err(("selfReferral", "Each self-referral signal must be block, flag or off.".into()));
            }
        }
        if self.wash.window_secs < 1 || self.wash.volume_tolerance_pct < ZERO || self.wash.short_trades_min < 1 {
            return Err(("wash", "Wash-trading thresholds are out of range.".into()));
        }
        if self.four_eyes_threshold < ZERO || self.four_eyes_threshold > dec("1000000000") {
            return Err(("fourEyesThreshold", "Four-eyes threshold must be between 0 and 1,000,000,000 USD (0 = every batch).".into()));
        }
        if self.link_base.len() > 200 || !(self.link_base.starts_with("https://") || self.link_base.starts_with("http://")) {
            return Err(("linkBase", "Link base must be an http(s) URL.".into()));
        }
        Ok(())
    }
}

pub fn valid_key(k: &str) -> bool {
    (1..=32).contains(&k.len()) && k.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub key: String,
    pub name: String,
    /// 1 = entry level; every client starts here (D53).
    pub rank: i32,
    /// USD per standard lot, per symbol group key (D54).
    pub rates: BTreeMap<String, D>,
    /// One-time CPA per qualified client (D57).
    pub cpa_amount: D,
    /// Upgrade targets (D58): active clients this month and network lots this month.
    pub min_active_clients: i32,
    pub min_monthly_lots: D,
    #[serde(default)]
    pub perks: Vec<String>,
    #[serde(default = "default_icon")]
    pub icon: String,
    /// Kalks FX Options (O34): USD per option **contract** (round turn, paid on the closing deal), separate from
    /// the CFD per-lot `rates`. 0 until the broker sets it. `None` in a PUT = keep the level's current rate (a
    /// Back Office build that does not know the field yet can never reset it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options_rate: Option<D>,
}

fn default_icon() -> String {
    "coin".into()
}

impl Level {
    pub fn rate(&self, group: &str) -> D {
        self.rates.get(group).copied().unwrap_or(ZERO)
    }

    /// USD per option contract at this level (0 when not set).
    pub fn option_rate(&self) -> D {
        self.options_rate.unwrap_or(ZERO).max(ZERO)
    }
}

/// Symbol-group key recorded on option deals and their commission lines (option series never map to a CFD
/// symbol group, and their rate is the level's `optionsRate`, never a per-lot rate).
pub const OPTIONS_GROUP: &str = "options";

/// Engine option series code (`EURUSD-20261009-1.1650-C`): underlying, expiry date, strike, right. Used as a
/// fallback when a deal feed does not carry the `option` / `instrument` fields.
pub fn is_option_series(symbol: &str) -> bool {
    let p: Vec<&str> = symbol.split('-').collect();
    p.len() == 4
        && !p[0].is_empty()
        && p[1].len() == 8
        && p[1].bytes().all(|b| b.is_ascii_digit())
        && !p[2].is_empty()
        && p[2].bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && matches!(p[3], "C" | "P" | "c" | "p")
}

pub fn default_levels() -> Vec<Level> {
    let groups = ["fx-major", "fx-minor", "metals", "indices", "energies", "crypto", "stocks"];
    let table: [(&str, &str, i32, [&str; 7], &str, i32, &str, &[&str], &str); 5] = [
        ("bronze", "Bronze", 1, ["5", "6", "8", "2", "4", "6", "1"], "200", 0, "0", &["Base rate card", "Standard materials"], "coin"),
        ("silver", "Silver", 2, ["7", "8", "10", "3", "5", "8", "1.5"], "200", 10, "200", &["+$2/lot on majors", "Custom landing pages"], "crown"),
        ("gold", "Gold", 3, ["9", "10", "12", "4", "6", "10", "2"], "300", 50, "1000", &["+$2/lot on all groups", "CPA $300", "Dedicated partner manager"], "1st_place_medal"),
        ("platinum", "Platinum", 4, ["11", "12", "13.5", "5", "7", "12", "2.5"], "300", 150, "3000", &["Priority payouts", "Co-branded campaigns"], "trophy"),
        ("diamond", "Diamond", 5, ["13", "14", "15", "6", "8", "14", "3"], "300", 400, "8000", &["Top rate card", "Quarterly review"], "gem_stone"),
    ];
    table
        .iter()
        .map(|(key, name, rank, rates, cpa, clients, lots, perks, icon)| Level {
            key: key.to_string(),
            name: name.to_string(),
            rank: *rank,
            rates: groups.iter().zip(rates.iter()).map(|(g, r)| (g.to_string(), dec(r))).collect(),
            cpa_amount: dec(cpa),
            min_active_clients: *clients,
            min_monthly_lots: dec(lots),
            perks: perks.iter().map(|p| p.to_string()).collect(),
            icon: icon.to_string(),
            options_rate: Some(ZERO),
        })
        .collect()
}

pub fn validate_levels(levels: &[Level], settings: &Settings) -> Result<(), (&'static str, String)> {
    if levels.is_empty() || levels.len() > 20 {
        return Err(("levels", "Define between 1 and 20 levels.".into()));
    }
    let mut keys = std::collections::HashSet::new();
    let mut ranks = std::collections::HashSet::new();
    for l in levels {
        if !valid_key(&l.key) || !keys.insert(l.key.clone()) {
            return Err(("key", format!("Level key '{}' is invalid or repeated.", l.key)));
        }
        if !ranks.insert(l.rank) || l.rank < 1 {
            return Err(("rank", "Level ranks must be unique and start at 1.".into()));
        }
        if l.name.trim().is_empty() || l.name.len() > 40 {
            return Err(("name", "Every level needs a name (≤ 40 chars).".into()));
        }
        if l.cpa_amount < ZERO || l.min_active_clients < 0 || l.min_monthly_lots < ZERO {
            return Err(("targets", format!("{}: amounts and targets must be ≥ 0.", l.name)));
        }
        if l.options_rate.is_some_and(|r| r < ZERO || r > dec("1000")) {
            return Err(("optionsRate", format!("{}: the options rate must be 0–1000 USD per contract.", l.name)));
        }
        for (g, r) in &l.rates {
            if *r < ZERO || *r > dec("1000") {
                return Err(("rates", format!("{}: rate for {g} must be 0–1000 USD per lot.", l.name)));
            }
            if !settings.symbol_groups.iter().any(|s| &s.key == g) {
                return Err(("rates", format!("{}: unknown symbol group {g}.", l.name)));
            }
        }
    }
    if !ranks.contains(&1) {
        return Err(("rank", "One level must have rank 1 (the entry level).".into()));
    }
    Ok(())
}

/// Symbol → asset class from config/instruments.json (`asset_class`).
#[derive(Clone, Default, Debug)]
pub struct Instruments(pub HashMap<String, String>);

impl Instruments {
    pub fn load(path: &str) -> Self {
        #[derive(Deserialize)]
        struct I {
            symbol: String,
            asset_class: String,
        }
        match std::fs::read_to_string(path).map_err(anyhow::Error::from).and_then(|s| Ok(serde_json::from_str::<Vec<I>>(&s)?)) {
            Ok(v) => Instruments(v.into_iter().map(|i| (i.symbol.to_uppercase(), i.asset_class)).collect()),
            Err(e) => {
                tracing::warn!(error = %e, path, "instruments file not loaded; symbol groups use explicit lists only");
                Instruments::default()
            }
        }
    }
}

/// Which symbol group a symbol belongs to: an explicit symbol list wins, then the asset class.
pub fn symbol_group(symbol: &str, settings: &Settings, instruments: &Instruments) -> Option<String> {
    let sym = symbol.to_uppercase();
    if let Some(g) = settings.symbol_groups.iter().find(|g| g.symbols.iter().any(|s| s.eq_ignore_ascii_case(&sym))) {
        return Some(g.key.clone());
    }
    let ac = instruments.0.get(&sym)?;
    settings.symbol_groups.iter().find(|g| g.asset_class.as_deref() == Some(ac.as_str())).map(|g| g.key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let s = Settings::default();
        s.validate().unwrap();
        validate_levels(&default_levels(), &s).unwrap();
    }

    #[test]
    fn symbol_groups_resolve() {
        let s = Settings::default();
        let ins = Instruments::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/instruments.json"));
        assert_eq!(symbol_group("EURUSD", &s, &ins).as_deref(), Some("fx-major"));
        assert_eq!(symbol_group("gbpjpy", &s, &ins).as_deref(), Some("fx-minor"));
        assert_eq!(symbol_group("XAUUSD", &s, &ins).as_deref(), Some("metals"));
        assert_eq!(symbol_group("NAS100", &s, &ins).as_deref(), Some("indices"));
        assert_eq!(symbol_group("BTCUSD", &s, &ins).as_deref(), Some("crypto"));
        assert_eq!(symbol_group("NOPE", &s, &ins), None);
    }

    #[test]
    fn options_rate_defaults_to_zero_and_is_validated() {
        let s = Settings::default();
        let lv = default_levels();
        assert!(lv.iter().all(|l| l.option_rate() == ZERO), "options earn nothing until the broker sets a rate");
        // a PUT body without the field (older Back Office build) deserializes to None = keep the current rate
        let mut v = serde_json::to_value(&lv[0]).unwrap();
        v.as_object_mut().unwrap().remove("optionsRate");
        let l: Level = serde_json::from_value(v).unwrap();
        assert_eq!((l.options_rate, l.option_rate()), (None, ZERO));
        let l: Level = serde_json::from_value(serde_json::json!({"key": "x", "name": "X", "rank": 1, "rates": {}, "cpaAmount": "0", "minActiveClients": 0, "minMonthlyLots": "0", "optionsRate": 1.25})).unwrap();
        assert_eq!(l.option_rate(), dec("1.25"));
        let mut bad = default_levels();
        bad[0].options_rate = Some(dec("-1"));
        assert_eq!(validate_levels(&bad, &s).unwrap_err().0, "optionsRate");
        bad[0].options_rate = Some(dec("1000.01"));
        assert!(validate_levels(&bad, &s).is_err());
    }

    #[test]
    fn option_series_codes_are_recognised() {
        assert!(is_option_series("EURUSD-20261009-1.1650-C"));
        assert!(is_option_series("USDJPY-20261009-150.00-P"));
        assert!(is_option_series("XAUUSD-20261231-2650-C"));
        for s in ["EURUSD", "US30", "BTC-USD", "EURUSD-2026109-1.1-C", "EURUSD-20261009-1.1650-X", "EURUSD-20261009-abc-C"] {
            assert!(!is_option_series(s), "{s}");
        }
    }

    #[test]
    fn settings_validation_rejects_bad_values() {
        let mut s = Settings::default();
        s.tiers = vec![dec("120")];
        assert!(s.validate().is_err());
        let mut s = Settings::default();
        s.payout.schedule = "hourly".into();
        assert!(s.validate().is_err());
        let mut l = default_levels();
        l[1].rank = 1;
        assert!(validate_levels(&l, &Settings::default()).is_err());
    }
}

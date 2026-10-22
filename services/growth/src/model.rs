//! Programme configuration and row types, with their API (camelCase JSON) views.

use crate::money::{D, dec, num};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

pub fn opt_num(d: Option<D>) -> Value {
    d.map(num).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------- settings

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// USD value of one point (display, cost report).
    #[serde(with = "rust_decimal::serde::float")]
    pub point_value: D,
    /// Deals held shorter than this earn no points (anti-scalping).
    pub min_hold_seconds: i64,
    pub points_expiry_months: i64,
    /// Cashback accruals are paid once they are this old.
    pub cashback_hold_hours: i64,
    /// Demo accounts earn points when a rule says `accountType: "any"` or `"demo"`; this switch turns that off.
    pub demo_points: bool,
    /// Challenges (phase 4): contest prizes and mission rewards below this USD amount are paid automatically
    /// (finalize -> pay); the rest wait for staff approval (`marketing.approve`). 0 = every payout needs staff.
    #[serde(with = "rust_decimal::serde::float")]
    pub auto_pay_below: D,
    /// Four-eyes (maker-checker): a prize, mission reward, cashback payout or cashback redemption at or above this
    /// USD amount needs two different staff members to approve it; auto-pay never releases it. 0 = every payout.
    #[serde(with = "rust_decimal::serde::float")]
    pub four_eyes_threshold: D,
    /// Ended contests are finalized by the service once this long after the end, when no anti-cheat flag is open.
    pub auto_finalize: bool,
    pub auto_finalize_delay_minutes: i64,
    /// The public leaderboard API (`GET /v1/public/leaderboard`) answers for this broker.
    pub public_leaderboards: bool,
    /// Whether a client who never chose shows on public leaderboards (nickname only).
    pub nickname_public_default: bool,
    /// Missed days a streak forgives once a week.
    pub streak_grace_days: i64,
    /// Missions count a trade only from this size (lots) and hold time (anti-abuse).
    #[serde(with = "rust_decimal::serde::float")]
    pub mission_min_lots: D,
    pub mission_min_hold_seconds: i64,
    /// XP for joining a live contest, and for every 7 days of a trading streak.
    pub xp_contest_join: i64,
    pub xp_streak_week: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            point_value: dec("0.01"),
            min_hold_seconds: 60,
            points_expiry_months: 12,
            cashback_hold_hours: 24,
            demo_points: false,
            auto_pay_below: D::ZERO,
            four_eyes_threshold: D::from(1000),
            auto_finalize: false,
            auto_finalize_delay_minutes: 60,
            public_leaderboards: true,
            nickname_public_default: true,
            streak_grace_days: 1,
            mission_min_lots: dec("0.01"),
            mission_min_hold_seconds: 60,
            xp_contest_join: 25,
            xp_streak_week: 50,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), (&'static str, String)> {
        if self.point_value <= D::ZERO || self.point_value > D::ONE {
            return Err(("pointValue", "Point value must be between 0 and 1 USD.".into()));
        }
        if !(0..=86_400).contains(&self.min_hold_seconds) {
            return Err(("minHoldSeconds", "Minimum hold must be 0–86400 seconds.".into()));
        }
        if !(1..=120).contains(&self.points_expiry_months) {
            return Err(("pointsExpiryMonths", "Expiry must be 1–120 months.".into()));
        }
        if !(0..=24 * 90).contains(&self.cashback_hold_hours) {
            return Err(("cashbackHoldHours", "Hold must be 0–2160 hours.".into()));
        }
        if self.auto_pay_below < D::ZERO || self.auto_pay_below > dec("1000000") {
            return Err(("autoPayBelow", "Auto-pay threshold must be 0 – 1 000 000 USD.".into()));
        }
        if self.four_eyes_threshold < D::ZERO || self.four_eyes_threshold > dec("1000000") {
            return Err(("fourEyesThreshold", "Four-eyes threshold must be 0 – 1 000 000 USD.".into()));
        }
        if !(0..=60 * 24 * 30).contains(&self.auto_finalize_delay_minutes) {
            return Err(("autoFinalizeDelayMinutes", "Delay must be 0–43200 minutes.".into()));
        }
        if !(0..=3).contains(&self.streak_grace_days) {
            return Err(("streakGraceDays", "Grace must be 0–3 days.".into()));
        }
        if self.mission_min_lots < D::ZERO || self.mission_min_lots > dec("100") {
            return Err(("missionMinLots", "Minimum lots must be 0–100.".into()));
        }
        if !(0..=86_400).contains(&self.mission_min_hold_seconds) {
            return Err(("missionMinHoldSeconds", "Minimum hold must be 0–86400 seconds.".into()));
        }
        if !(0..=10_000).contains(&self.xp_contest_join) || !(0..=10_000).contains(&self.xp_streak_week) {
            return Err(("xpContestJoin", "XP awards must be 0–10000.".into()));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- loyalty

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Tier {
    pub key: String,
    pub name: String,
    pub rank: i32,
    pub min_points: i64,
    #[serde(with = "rust_decimal::serde::float")]
    pub multiplier: D,
    #[serde(default)]
    pub perks: Vec<String>,
}

pub fn default_tiers() -> Vec<Tier> {
    let t = |key: &str, name: &str, rank, min_points, m: &str, perks: &[&str]| Tier {
        key: key.into(),
        name: name.into(),
        rank,
        min_points,
        multiplier: dec(m),
        perks: perks.iter().map(|s| s.to_string()).collect(),
    };
    vec![
        t("bronze", "Bronze", 1, 0, "1", &["Points on every live lot", "Rewards catalogue"]),
        t("silver", "Silver", 2, 5_000, "1.1", &["1.1× points", "Priority support"]),
        t("gold", "Gold", 3, 15_000, "1.25", &["1.25× points", "Free prop retry once a quarter"]),
        t("platinum", "Platinum", 4, 50_000, "1.5", &["1.5× points", "Dedicated account manager"]),
    ]
}

pub fn validate_tiers(tiers: &[Tier]) -> Result<(), String> {
    if tiers.is_empty() || tiers.len() > 10 {
        return Err("Between 1 and 10 tiers.".into());
    }
    let mut sorted = tiers.to_vec();
    sorted.sort_by_key(|t| t.rank);
    if sorted[0].min_points != 0 {
        return Err("The first tier must start at 0 points.".into());
    }
    for (i, t) in sorted.iter().enumerate() {
        if t.rank != i as i32 + 1 {
            return Err("Ranks must be 1, 2, 3 … without gaps.".into());
        }
        if t.key.is_empty() || t.key.len() > 32 || !t.key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
            return Err(format!("Tier key '{}' must be lower-case letters, digits, - or _.", t.key));
        }
        if t.name.trim().is_empty() || t.name.len() > 40 {
            return Err("Every tier needs a name (≤ 40 chars).".into());
        }
        if t.multiplier < D::ONE || t.multiplier > dec("5") {
            return Err("Multipliers must be between 1 and 5.".into());
        }
        if i > 0 && t.min_points <= sorted[i - 1].min_points {
            return Err("Tier thresholds must increase with rank.".into());
        }
        if i > 0 && t.multiplier < sorted[i - 1].multiplier {
            return Err("Multipliers cannot decrease with rank.".into());
        }
    }
    let mut keys: Vec<&str> = tiers.iter().map(|t| t.key.as_str()).collect();
    keys.sort();
    keys.dedup();
    if keys.len() != tiers.len() {
        return Err("Tier keys must be unique.".into());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct EarnRule {
    pub id: i64,
    pub name: String,
    pub asset_class: Option<String>,
    pub symbols: Vec<String>,
    pub account_groups: Vec<String>,
    /// live | demo | any
    pub account_type: String,
    pub points_per_lot: D,
    pub priority: i32,
    pub active: bool,
}

impl EarnRule {
    pub fn json(&self) -> Value {
        json!({
            "id": self.id, "name": self.name, "assetClass": self.asset_class, "symbols": self.symbols,
            "accountGroups": self.account_groups, "accountType": self.account_type, "pointsPerLot": num(self.points_per_lot),
            "priority": self.priority, "active": self.active,
        })
    }
}

pub const ASSET_CLASSES: &[&str] = &["forex", "metals", "indices", "energies", "crypto", "stocks"];

pub fn default_rules() -> Vec<(&'static str, &'static str, &'static str)> {
    // (name, asset class, points per lot)
    vec![("Forex", "forex", "10"), ("Metals", "metals", "15"), ("Indices", "indices", "8"), ("Energies", "energies", "8"), ("Crypto", "crypto", "6"), ("Stocks", "stocks", "4")]
}

// ---------------------------------------------------------------- instruments

/// Symbol → asset class, from `config/instruments.json`.
#[derive(Default, Debug)]
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
                tracing::warn!(error = %e, path, "instruments file not loaded; rules match explicit symbol lists only");
                Instruments::default()
            }
        }
    }

    pub fn class_of(&self, symbol: &str) -> Option<String> {
        self.0.get(&symbol.to_uppercase()).cloned()
    }
}

// ---------------------------------------------------------------- contests

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Prize {
    pub rank_from: i32,
    pub rank_to: i32,
    #[serde(deserialize_with = "crate::money::de_dec", serialize_with = "rust_decimal::serde::float::serialize")]
    pub amount: D,
    /// wallet | credit
    #[serde(default = "wallet")]
    pub payout: String,
}

fn wallet() -> String {
    "wallet".into()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AntiCheat {
    pub min_hold_seconds: i64,
    #[serde(with = "rust_decimal::serde::float")]
    pub max_single_trade_pct: D,
    pub disqualify_on_balance_change: bool,
}

impl Default for AntiCheat {
    fn default() -> Self {
        AntiCheat { min_hold_seconds: 30, max_single_trade_pct: dec("80"), disqualify_on_balance_change: false }
    }
}

pub fn validate_prizes(p: &[Prize]) -> Result<(), String> {
    if p.len() > 50 {
        return Err("At most 50 prize rows.".into());
    }
    let mut sorted = p.to_vec();
    sorted.sort_by_key(|x| x.rank_from);
    let mut last = 0;
    for x in &sorted {
        if x.rank_from < 1 || x.rank_to < x.rank_from || x.rank_to > 1000 {
            return Err("Prize ranks must be 1–1000 with from ≤ to.".into());
        }
        if x.rank_from <= last {
            return Err("Prize rank ranges overlap.".into());
        }
        if x.amount <= D::ZERO || x.amount > dec("1000000") {
            return Err("Prize amounts must be positive.".into());
        }
        if x.payout != "wallet" && x.payout != "credit" {
            return Err("Prize payout must be wallet or credit.".into());
        }
        last = x.rank_to;
    }
    Ok(())
}

pub fn prize_pool(p: &[Prize]) -> D {
    p.iter().map(|x| x.amount * D::from(x.rank_to - x.rank_from + 1)).sum()
}

/// Effective status: stored draft / cancelled / finalized / paid win; otherwise it follows the clock.
pub fn contest_status(stored: &str, starts: DateTime<Utc>, ends: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match stored {
        "draft" | "cancelled" | "finalized" | "paid" => stored.into(),
        _ if now < starts => "scheduled".into(),
        _ if now < ends => "running".into(),
        _ => "ended".into(),
    }
}

/// "Arjun K." from a first and last name.
pub fn display_name(first: &str, last: &str) -> String {
    let f = first.trim();
    let l = last.trim().chars().next().map(|c| format!(" {}.", c.to_uppercase())).unwrap_or_default();
    let n = format!("{f}{l}").trim().to_string();
    if n.is_empty() { "Trader".into() } else { n.chars().take(40).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_validate() {
        Settings::default().validate().unwrap();
        validate_tiers(&default_tiers()).unwrap();
        let mut t = default_tiers();
        t[2].min_points = 1;
        assert!(validate_tiers(&t).is_err());
        let mut t = default_tiers();
        t[0].min_points = 10;
        assert!(validate_tiers(&t).is_err());
    }

    #[test]
    fn prizes_and_names() {
        let p = vec![
            Prize { rank_from: 1, rank_to: 1, amount: dec("500"), payout: "wallet".into() },
            Prize { rank_from: 2, rank_to: 5, amount: dec("100"), payout: "credit".into() },
        ];
        validate_prizes(&p).unwrap();
        assert_eq!(prize_pool(&p), dec("900"));
        let overlap = vec![p[0].clone(), Prize { rank_from: 1, rank_to: 3, amount: dec("1"), payout: "wallet".into() }];
        assert!(validate_prizes(&overlap).is_err());
        assert_eq!(display_name("arjun", "kumar"), "arjun K.");
        assert_eq!(display_name("", ""), "Trader");
    }

    #[test]
    fn contest_status_follows_clock() {
        let now = Utc::now();
        let h = chrono::Duration::hours(1);
        assert_eq!(contest_status("scheduled", now + h, now + h * 2, now), "scheduled");
        assert_eq!(contest_status("scheduled", now - h, now + h, now), "running");
        assert_eq!(contest_status("running", now - h * 2, now - h, now), "ended");
        assert_eq!(contest_status("finalized", now - h * 2, now - h, now), "finalized");
        assert_eq!(contest_status("draft", now - h, now + h, now), "draft");
    }
}

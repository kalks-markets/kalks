//! Tenant configuration the engine reads while deciding: groups, tenant dealing policy, symbol controls
//! and A/B routing rules. These are configuration (edited by staff, audited), not per-account events.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::model::{Book, Mode};
use crate::money::D;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    #[serde(default)]
    pub tenant_id: i64,
    /// Stable code, e.g. `standard`, `pro-netting`.
    pub code: String,
    pub name: String,
    pub mode: Mode,
    pub cent: bool,
    /// Which account types may use this group: `live`, `demo` or `both`.
    pub account_types: String,
    pub leverages: Vec<u32>,
    pub default_leverage: u32,
    #[serde(with = "rust_decimal::serde::float")]
    pub margin_call_pct: D,
    #[serde(with = "rust_decimal::serde::float")]
    pub stop_out_pct: D,
    /// Margin charged on hedged volume (0 = hedged legs are free, 100 = both legs in full).
    #[serde(with = "rust_decimal::serde::float")]
    pub hedged_margin_pct: D,
    #[serde(with = "rust_decimal::serde::float")]
    pub min_deposit: D,
    pub swap_free: bool,
    /// Round-turn commission per standard lot in USD, charged when exposure is opened.
    #[serde(with = "rust_decimal::serde::float")]
    pub commission_per_lot: D,
    pub route: Book,
    /// market-data spread group (`/v1/stream?group=`).
    pub spread_group: String,
    pub max_accounts_per_user: u32,
    #[serde(with = "rust_decimal::serde::float")]
    pub demo_initial_balance: D,
    pub demo_refills_per_day: u32,
    pub demo_expiry_days: u32,
    pub enabled: bool,
    /// What the group's accounts trade: CFDs or Kalks FX Options, never both (fixed once the group has accounts).
    /// Read fresh from the group on every operation, so it never rides on an event.
    #[serde(default)]
    pub product: Product,
}

impl Group {
    pub fn allows(&self, kind: &str) -> bool {
        self.account_types == "both" || self.account_types == kind
    }
}

/// The product an account trades, a property of its group: a CFD account opens CFD positions only, an Options
/// account option positions only (house and order book). Closing is never refused for the product.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Product {
    #[default]
    Cfd,
    Options,
}

impl Product {
    pub fn as_str(self) -> &'static str {
        match self {
            Product::Cfd => "cfd",
            Product::Options => "options",
        }
    }
    /// Anything but `options` is a CFD group (the database default).
    pub fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("options") { Product::Options } else { Product::Cfd }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantPolicy {
    /// D115: per-tenant switch; when off, per-account execution delays are ignored.
    pub exec_delay_enabled: bool,
    pub exec_delay_cap_ms: u32,
    /// Defaults for new groups.
    #[serde(with = "rust_decimal::serde::float")]
    pub margin_call_pct: D,
    #[serde(with = "rust_decimal::serde::float")]
    pub stop_out_pct: D,
}

impl Default for TenantPolicy {
    fn default() -> Self {
        Self { exec_delay_enabled: true, exec_delay_cap_ms: 500, margin_call_pct: D::from(100), stop_out_pct: D::from(50) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ControlMode {
    Halt,
    CloseOnly,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolControl {
    pub id: String,
    pub symbol: String,
    /// group code or "all"
    pub group: String,
    pub mode: ControlMode,
    pub reason_code: String,
    pub note: Option<String>,
    pub staff: String,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoutingCondition {
    pub field: String,
    pub op: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoutingAction {
    pub book: Book,
    pub pct: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lp: Option<String>,
}

/// Same shape as the Back Office `RoutingRule`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingRule {
    pub id: String,
    pub name: String,
    pub conditions: Vec<RoutingCondition>,
    pub join: String,
    pub action: RoutingAction,
    pub enabled: bool,
    #[serde(default)]
    pub hits24h: u64,
    #[serde(default)]
    pub lots24h: f64,
}

pub struct RouteCtx<'a> {
    pub login: i64,
    pub group: &'a str,
    pub symbol: &'a str,
    pub volume: D,
    /// Ticket the decision is for (the order, or the position for a routing suggestion): a rule's percentage split
    /// is a deterministic function of it, so the same ticket always lands in the same bucket.
    pub id: i64,
    /// What the risk desk knows about the client and the market (src/risk): None = no data, the condition never matches.
    pub facts: RouteFacts<'a>,
}

/// Risk-desk facts for the routing conditions beyond login / group / symbol / lot size.
#[derive(Clone, Copy, Debug, Default)]
pub struct RouteFacts<'a> {
    /// Client profitability score 0-100 (src/risk/score.rs).
    pub score: Option<f64>,
    /// `toxic` | `likely_profitable` | `neutral` | `likely_losing`.
    pub category: Option<&'a str>,
    /// Average holding time of the client's closed trades (seconds).
    pub avg_hold_secs: Option<f64>,
    /// Win rate of the closed trades, percent.
    pub win_rate: Option<f64>,
    /// ISO 3166 alpha-2, lower case.
    pub country: Option<&'a str>,
    /// Account equity in USD.
    pub equity_usd: Option<D>,
    /// Minutes between now and the nearest high-impact news event on the symbol's currencies (|Δ|, within 4 h).
    pub news_minutes: Option<i64>,
    /// The symbol is inside the tenant's configured news window right now.
    pub news_active: bool,
}

/// Deterministic bucket 0..10 000 of a ticket (splitmix64): `pct` % of tickets fall below `pct × 100`.
pub fn split_bucket(id: i64) -> u64 {
    let mut z = (id as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) % 10_000
}

/// The book a matching rule sends ticket `id` to: `pct` % of tickets (by `split_bucket`) to the rule's book, the
/// rest to the other book. 100 % (or more) always the rule's book, 0 % never.
pub fn split_book(action: &RoutingAction, id: i64) -> Book {
    let other = if action.book == Book::A { Book::B } else { Book::A };
    let pct = if action.pct.is_finite() { action.pct.clamp(0.0, 100.0) } else { 100.0 };
    if pct >= 100.0 {
        return action.book;
    }
    if (split_bucket(id) as f64) < pct * 100.0 { action.book } else { other }
}

/// The first enabled rule whose conditions match, and the book it sends this ticket to.
pub fn match_rule<'r>(rules: &'r [RoutingRule], ctx: &RouteCtx) -> Option<(&'r RoutingRule, Book)> {
    for r in rules {
        if !r.enabled || r.conditions.is_empty() {
            continue;
        }
        let m = |c: &RoutingCondition| cond_match(c, ctx);
        let ok = if r.join.eq_ignore_ascii_case("OR") { r.conditions.iter().any(m) } else { r.conditions.iter().all(m) };
        if ok {
            return Some((r, split_book(&r.action, ctx.id)));
        }
    }
    None
}

/// First enabled matching rule wins (D140). Conditions the engine has no data for (no score yet, no news feed,
/// unknown country) never match. A rule with `pct` < 100 splits deterministically by ticket (`split_book`).
pub fn resolve_route(rules: &[RoutingRule], ctx: &RouteCtx, fallback: Book) -> (Book, Option<String>) {
    match match_rule(rules, ctx) {
        Some((r, book)) => (book, Some(format!("Rule {} · {}", r.id, r.name))),
        None => (fallback, None),
    }
}

/// `60s`, `5m`, `2h`, `1d` or a bare number of seconds.
pub fn duration_secs(s: &str) -> Option<D> {
    let t = s.trim().to_lowercase();
    let digits: String = t.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let n: D = digits.parse().ok()?;
    let unit = t.chars().find(|c| c.is_ascii_alphabetic());
    Some(match unit {
        Some('m') => n * D::from(60),
        Some('h') => n * D::from(3600),
        Some('d') => n * D::from(86_400),
        _ => n,
    })
}

/// `±2m` / `+-5 min` in a News window value: the window in minutes it asks for.
fn news_minutes_of(value: &str) -> Option<i64> {
    let i = value.find('±').map(|i| i + '±'.len_utf8()).or_else(|| value.find("+-").map(|i| i + 2))?;
    let rest: String = value[i..].trim_start().chars().take_while(|c| c.is_ascii_digit()).collect();
    rest.parse().ok()
}

fn cond_match(c: &RoutingCondition, ctx: &RouteCtx) -> bool {
    let list: Vec<String> = c.value.split(',').map(|x| x.trim().to_lowercase()).collect();
    let num = |s: &str| -> Option<D> {
        let t: String = s.chars().filter(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '-').collect();
        t.parse().ok()
    };
    let cmp_to = |v: D, t: Option<D>| -> bool {
        let Some(t) = t else { return false };
        match c.op.as_str() {
            "≥" | ">=" => v >= t,
            "≤" | "<=" => v <= t,
            ">" => v > t,
            "<" => v < t,
            _ => v == t,
        }
    };
    let cmp = |v: D| cmp_to(v, num(&c.value));
    let f = &ctx.facts;
    let dec = |x: Option<f64>| x.and_then(crate::money::from_f64);
    match c.field.as_str() {
        "Login" => list.contains(&ctx.login.to_string()),
        "Group" => list.contains(&ctx.group.to_lowercase()),
        "Symbol" => list.contains(&ctx.symbol.to_lowercase()),
        "Lot size" => cmp(ctx.volume),
        "Risk score" | "Score" => dec(f.score).is_some_and(cmp),
        "Risk category" | "Category" => f.category.is_some_and(|cat| list.iter().any(|x| x.replace([' ', '-'], "_") == cat)),
        "Avg hold time" | "Hold time" => dec(f.avg_hold_secs).is_some_and(|v| cmp_to(v, duration_secs(&c.value))),
        "Win rate (30d)" | "Win rate" => dec(f.win_rate).is_some_and(cmp),
        "Equity" => f.equity_usd.is_some_and(cmp),
        "Country" => f.country.is_some_and(|cc| list.contains(&cc.to_lowercase())),
        "News window" => {
            let wanted = !matches!(c.value.trim().to_lowercase().as_str(), "no" | "false" | "off" | "outside");
            let inside = match news_minutes_of(&c.value) {
                Some(m) => f.news_minutes.is_some_and(|x| x <= m),
                None => f.news_active,
            };
            inside == wanted
        }
        _ => false,
    }
}

/// Per-tenant configuration snapshot shared by all shards (read-mostly).
#[derive(Clone, Debug, Default)]
pub struct TenantConfig {
    pub tenant_id: i64,
    pub slug: String,
    pub groups: HashMap<String, Group>,
    pub policy: TenantPolicy,
    pub symbol_controls: Vec<SymbolControl>,
    pub routing_rules: Vec<RoutingRule>,
    /// Risk desk: limits and settings (`risk_limits`, `risk_settings`; src/risk).
    pub risk: crate::risk::RiskConfig,
}

impl TenantConfig {
    /// Halt beats close-only; group-specific and "all" both apply.
    pub fn symbol_control(&self, symbol: &str, group: &str) -> Option<&SymbolControl> {
        let list: Vec<&SymbolControl> = self.symbol_controls.iter().filter(|c| c.symbol == symbol && (c.group == "all" || c.group == group)).collect();
        list.iter().find(|c| c.mode == ControlMode::Halt).or_else(|| list.iter().find(|c| c.mode == ControlMode::CloseOnly)).copied()
    }
}

/// All tenants' configuration.
#[derive(Clone, Default)]
pub struct Registry(Arc<RwLock<HashMap<i64, Arc<TenantConfig>>>>);

impl Registry {
    pub fn get(&self, tenant_id: i64) -> Option<Arc<TenantConfig>> {
        self.0.read().unwrap().get(&tenant_id).cloned()
    }
    pub fn by_slug(&self, slug: &str) -> Option<Arc<TenantConfig>> {
        self.0.read().unwrap().values().find(|t| t.slug == slug).cloned()
    }
    pub fn all(&self) -> Vec<Arc<TenantConfig>> {
        self.0.read().unwrap().values().cloned().collect()
    }
    pub fn put(&self, cfg: TenantConfig) {
        self.0.write().unwrap().insert(cfg.tenant_id, Arc::new(cfg));
    }
    /// Copy-on-write update of one tenant.
    pub fn update(&self, tenant_id: i64, f: impl FnOnce(&mut TenantConfig)) {
        let mut w = self.0.write().unwrap();
        let mut cfg = w.get(&tenant_id).map(|c| (**c).clone()).unwrap_or_default();
        f(&mut cfg);
        w.insert(tenant_id, Arc::new(cfg));
    }
    /// Every spread group in use (one feed connection each).
    pub fn spread_groups(&self) -> Vec<String> {
        let mut v: Vec<String> = self.0.read().unwrap().values().flat_map(|t| t.groups.values().map(|g| g.spread_group.clone()).collect::<Vec<_>>()).collect();
        v.sort();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, field: &str, op: &str, value: &str, book: Book, pct: f64) -> RoutingRule {
        RoutingRule { id: id.into(), name: id.into(), conditions: vec![RoutingCondition { field: field.into(), op: op.into(), value: value.into() }], join: "AND".into(), action: RoutingAction { book, pct, lp: None }, enabled: true, hits24h: 0, lots24h: 0.0 }
    }

    #[test]
    fn routing_first_match_wins() {
        let rules = vec![
            rule("RQ-L100", "Login", "=", "100", Book::A, 100.0),
            rule("RR-2", "Lot size", "≥", "20", Book::A, 100.0),
            rule("RR-X", "Risk score", "≥", "8", Book::A, 100.0),
            rule("RR-4", "Group", "is", "Prop", Book::B, 100.0),
        ];
        let ctx = |login, group, vol: i64| RouteCtx { login, group, symbol: "EURUSD", volume: D::from(vol), id: 1_000_001, facts: RouteFacts::default() };
        assert_eq!(resolve_route(&rules, &ctx(100, "standard", 1), Book::B).0, Book::A);
        assert_eq!(resolve_route(&rules, &ctx(7, "standard", 25), Book::B).0, Book::A);
        assert_eq!(resolve_route(&rules, &ctx(7, "standard", 1), Book::B), (Book::B, None));
        assert_eq!(resolve_route(&rules, &ctx(7, "prop", 1), Book::A).0, Book::B);
        // 0 % sends every ticket to the other book, 100 % none
        let r = vec![rule("R", "Symbol", "in", "EURUSD, GBPUSD", Book::A, 0.0)];
        assert_eq!(resolve_route(&r, &ctx(1, "x", 1), Book::A).0, Book::B);
        let r = vec![rule("R", "Symbol", "in", "EURUSD, GBPUSD", Book::A, 100.0)];
        assert_eq!(resolve_route(&r, &ctx(1, "x", 1), Book::B).0, Book::A);
    }

    fn ctx_with<'a>(id: i64, facts: RouteFacts<'a>) -> RouteCtx<'a> {
        RouteCtx { login: 10_000_001, group: "standard", symbol: "EURUSD", volume: D::from(1), id, facts }
    }

    #[test]
    fn percentage_split_is_deterministic_and_proportional() {
        let action = RoutingAction { book: Book::A, pct: 30.0, lp: None };
        // the same ticket always lands in the same book
        for id in 1_000_001..1_000_200 {
            assert_eq!(split_book(&action, id), split_book(&action, id));
        }
        // about 30 % of 10 000 consecutive tickets go to A
        let a = (1_000_001..1_010_001).filter(|id| split_book(&action, *id) == Book::A).count();
        assert!((2_800..=3_200).contains(&a), "{a} of 10 000 routed to A");
        // a rule with a 30 % split routes through resolve_route the same way
        let rules = vec![rule("S", "Symbol", "in", "EURUSD", Book::A, 30.0)];
        for id in [1_000_001_i64, 1_000_002, 1_000_003, 2_000_000] {
            assert_eq!(resolve_route(&rules, &ctx_with(id, RouteFacts::default()), Book::B).0, split_book(&action, id));
        }
        // 0 % and 100 % are not random
        assert!((1..500).all(|id| split_book(&RoutingAction { book: Book::A, pct: 100.0, lp: None }, id) == Book::A));
        assert!((1..500).all(|id| split_book(&RoutingAction { book: Book::A, pct: 0.0, lp: None }, id) == Book::B));
    }

    #[test]
    fn risk_conditions_match_on_facts() {
        let f = RouteFacts { score: Some(82.0), category: Some("toxic"), avg_hold_secs: Some(25.0), win_rate: Some(74.5), country: Some("ae"), equity_usd: Some(D::from(60_000)), news_minutes: Some(1), news_active: true };
        let m = |field: &str, op: &str, value: &str, facts: RouteFacts| resolve_route(&[rule("X", field, op, value, Book::A, 100.0)], &ctx_with(7, facts), Book::B).0 == Book::A;
        assert!(m("Risk score", "≥", "70", f));
        assert!(!m("Risk score", "≥", "90", f));
        assert!(m("Risk category", "in", "toxic, likely profitable", f));
        assert!(!m("Risk category", "in", "neutral", f));
        assert!(m("Avg hold time", "<", "60s", f));
        assert!(m("Avg hold time", "<", "1m", f));
        assert!(m("Avg hold time", "<", "1 min", f));
        assert!(!m("Avg hold time", ">", "2m", f));
        assert!(m("Win rate (30d)", "≥", "70%", f));
        assert!(!m("Win rate (30d)", "≥", "80%", f));
        assert!(m("Equity", "≥", "$50,000", f));
        assert!(!m("Equity", "≥", "$100,000", f));
        assert!(m("Country", "in", "AE, SA", f));
        assert!(!m("Country", "in", "GB", f));
        assert!(m("News window", "is", "High impact ±2m", f));
        assert!(!m("News window", "is", "High impact ±2m", RouteFacts { news_minutes: Some(5), ..f }));
        assert!(m("News window", "is", "active", f));
        assert!(!m("News window", "is", "active", RouteFacts { news_active: false, ..f }));
        // no data: never matches
        let none = RouteFacts::default();
        for (field, op, value) in [("Risk score", "≥", "0"), ("Risk category", "in", "neutral"), ("Avg hold time", ">", "0s"), ("Win rate (30d)", "≥", "0%"), ("Equity", "≥", "0"), ("Country", "in", "ae"), ("News window", "is", "±60m")] {
            assert!(!m(field, op, value, none), "{field} matched without data");
        }
    }
}

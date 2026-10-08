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
}

/// First enabled matching rule wins (D140). Conditions the engine has no data for yet (risk score, hold
/// time, win rate, news window, country, equity) never match. A rule sending < 50 % to its book routes the
/// ticket to the other book (the split itself is done by a book transfer).
pub fn resolve_route(rules: &[RoutingRule], ctx: &RouteCtx, fallback: Book) -> (Book, Option<String>) {
    for r in rules {
        if !r.enabled || r.conditions.is_empty() {
            continue;
        }
        let m = |c: &RoutingCondition| cond_match(c, ctx);
        let ok = if r.join.eq_ignore_ascii_case("OR") { r.conditions.iter().any(m) } else { r.conditions.iter().all(m) };
        if ok {
            let book = if r.action.pct >= 50.0 { r.action.book } else if r.action.book == Book::A { Book::B } else { Book::A };
            return (book, Some(format!("Rule {} · {}", r.id, r.name)));
        }
    }
    (fallback, None)
}

fn cond_match(c: &RoutingCondition, ctx: &RouteCtx) -> bool {
    let list: Vec<String> = c.value.split(',').map(|x| x.trim().to_lowercase()).collect();
    let num = |s: &str| -> Option<D> {
        let t: String = s.chars().filter(|ch| ch.is_ascii_digit() || *ch == '.' || *ch == '-').collect();
        t.parse().ok()
    };
    let cmp = |v: D| -> bool {
        let Some(t) = num(&c.value) else { return false };
        match c.op.as_str() {
            "≥" | ">=" => v >= t,
            "≤" | "<=" => v <= t,
            ">" => v > t,
            "<" => v < t,
            _ => v == t,
        }
    };
    match c.field.as_str() {
        "Login" => list.contains(&ctx.login.to_string()),
        "Group" => list.contains(&ctx.group.to_lowercase()),
        "Symbol" => list.contains(&ctx.symbol.to_lowercase()),
        "Lot size" => cmp(ctx.volume),
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
            rule("RR-2", "Lot size", "≥", "20", Book::A, 50.0),
            rule("RR-X", "Risk score", "≥", "8", Book::A, 100.0),
            rule("RR-4", "Group", "is", "Prop", Book::B, 100.0),
        ];
        let ctx = |login, group, vol: i64| RouteCtx { login, group, symbol: "EURUSD", volume: D::from(vol) };
        assert_eq!(resolve_route(&rules, &ctx(100, "standard", 1), Book::B).0, Book::A);
        assert_eq!(resolve_route(&rules, &ctx(7, "standard", 25), Book::B).0, Book::A);
        assert_eq!(resolve_route(&rules, &ctx(7, "standard", 1), Book::B), (Book::B, None));
        assert_eq!(resolve_route(&rules, &ctx(7, "prop", 1), Book::A).0, Book::B);
        // pct < 50 sends the ticket to the other book
        let r = vec![rule("R", "Symbol", "in", "EURUSD, GBPUSD", Book::A, 30.0)];
        assert_eq!(resolve_route(&r, &ctx(1, "x", 1), Book::A).0, Book::B);
    }
}

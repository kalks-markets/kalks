//! Risk limits and settings per tenant (`risk_limits`, `risk_settings`), edited in Back Office › Trading › Risk limits.
//!
//! Every limit has a mode: `alert` (default: the desk is told, nothing is refused) or `enforce` (new orders that would
//! break it are refused in `engine::trade::risk_gate`). Closing and reducing are never refused.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

use crate::money::{D, HUNDRED, ZERO};
use crate::rules::Group;
use crate::specs::Specs;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LimitMode {
    /// Raise an alert only (the default; the plan runs alert-only for the first two weeks).
    #[default]
    Alert,
    /// Refuse new orders that would break the limit.
    Enforce,
}

impl LimitMode {
    pub fn as_str(self) -> &'static str {
        match self {
            LimitMode::Alert => "alert",
            LimitMode::Enforce => "enforce",
        }
    }
}

fn yes() -> bool {
    true
}

fn eighty() -> D {
    D::from(80)
}

/// Net client exposure cap of one symbol (`*` = every symbol without its own row), in notional USD, on the B-book
/// (the exposure the house keeps).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureLimit {
    pub symbol: String,
    #[serde(with = "rust_decimal::serde::float")]
    pub cap_usd: D,
    /// Amber alert from this share of the cap (%); red at 100 %.
    #[serde(with = "rust_decimal::serde::float", default = "eighty")]
    pub warn_pct: D,
    #[serde(default)]
    pub mode: LimitMode,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// House daily loss threshold (B-book P&L since the server day started). Alert only.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyLossLimit {
    #[serde(with = "rust_decimal::serde::float")]
    pub threshold_usd: D,
    #[serde(with = "rust_decimal::serde::float", default = "eighty")]
    pub warn_pct: D,
    #[serde(default)]
    pub mode: LimitMode,
    #[serde(default = "yes")]
    pub enabled: bool,
}

/// Per-group defaults: max lot per order (an account's own max lot wins) and max open positions per client (across
/// the client's accounts in the tenant).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupLimit {
    pub group: String,
    #[serde(with = "rust_decimal::serde::float_option", default)]
    pub max_lot: Option<D>,
    #[serde(default)]
    pub max_positions: Option<u32>,
    #[serde(default)]
    pub mode: LimitMode,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn two() -> u32 {
    2
}

fn three() -> u8 {
    3
}

fn one() -> D {
    D::ONE
}

/// News windows: `minutes_before` / `minutes_after` a high-impact event (news calendar, impact ≥ `min_impact`) on a
/// currency of the symbol. Actions: a margin multiplier on new exposure and / or no new pending orders.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsLimit {
    #[serde(default = "two")]
    pub minutes_before: u32,
    #[serde(default = "two")]
    pub minutes_after: u32,
    #[serde(default = "three")]
    pub min_impact: u8,
    /// Margin required for new exposure is multiplied by this (1 = off).
    #[serde(with = "rust_decimal::serde::float", default = "one")]
    pub margin_multiplier: D,
    #[serde(default)]
    pub block_pending: bool,
    #[serde(default)]
    pub mode: LimitMode,
    #[serde(default = "yes")]
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskLimits {
    #[serde(default)]
    pub exposure: Vec<ExposureLimit>,
    #[serde(default)]
    pub daily_loss: Option<DailyLossLimit>,
    #[serde(default)]
    pub groups: Vec<GroupLimit>,
    #[serde(default)]
    pub news: Option<NewsLimit>,
}

impl RiskLimits {
    /// The enabled cap of `symbol`: its own row, else the `*` row.
    pub fn exposure_for(&self, symbol: &str) -> Option<&ExposureLimit> {
        self.exposure.iter().find(|l| l.enabled && l.symbol == symbol).or_else(|| self.exposure.iter().find(|l| l.enabled && l.symbol == "*"))
    }
    pub fn group(&self, code: &str) -> Option<&GroupLimit> {
        self.groups.iter().find(|g| g.enabled && g.group == code)
    }
    pub fn news(&self) -> Option<&NewsLimit> {
        self.news.as_ref().filter(|n| n.enabled)
    }
    pub fn daily_loss(&self) -> Option<&DailyLossLimit> {
        self.daily_loss.as_ref().filter(|d| d.enabled)
    }
    /// Every limit alert-only (the launch setting).
    pub fn alert_only(&self) -> bool {
        self.exposure.iter().all(|l| l.mode == LimitMode::Alert) && self.groups.iter().all(|g| g.mode == LimitMode::Alert) && self.news.as_ref().is_none_or(|n| n.mode == LimitMode::Alert)
    }

    /// Field and message of the first invalid value.
    pub fn validate(&self, groups: &HashMap<String, Group>, specs: &Specs) -> Result<(), (&'static str, String)> {
        let pct_ok = |p: D| p > ZERO && p <= HUNDRED;
        let mut seen = std::collections::HashSet::new();
        for l in &self.exposure {
            if l.symbol != "*" && specs.get(&l.symbol).is_none() {
                return Err(("exposure", format!("Unknown symbol {}", l.symbol)));
            }
            if !seen.insert(l.symbol.clone()) {
                return Err(("exposure", format!("{} is listed twice", l.symbol)));
            }
            if l.cap_usd <= ZERO {
                return Err(("exposure", format!("The cap of {} must be above 0", l.symbol)));
            }
            if !pct_ok(l.warn_pct) {
                return Err(("exposure", format!("The warning level of {} must be 1–100 %", l.symbol)));
            }
        }
        if let Some(d) = &self.daily_loss {
            if d.threshold_usd <= ZERO {
                return Err(("dailyLoss", "The daily loss threshold must be above 0".into()));
            }
            if !pct_ok(d.warn_pct) {
                return Err(("dailyLoss", "The warning level must be 1–100 %".into()));
            }
        }
        let mut seen = std::collections::HashSet::new();
        for g in &self.groups {
            if !groups.contains_key(&g.group) {
                return Err(("groups", format!("Unknown group {}", g.group)));
            }
            if !seen.insert(g.group.clone()) {
                return Err(("groups", format!("{} is listed twice", g.group)));
            }
            if g.max_lot.is_some_and(|m| m <= ZERO) {
                return Err(("groups", format!("The max lot of {} must be above 0", g.group)));
            }
            if g.max_positions == Some(0) {
                return Err(("groups", format!("The max open positions of {} must be at least 1", g.group)));
            }
        }
        if let Some(n) = &self.news {
            if n.minutes_before > 240 || n.minutes_after > 240 {
                return Err(("news", "News windows are at most 240 minutes before / after".into()));
            }
            if !(1..=3).contains(&n.min_impact) {
                return Err(("news", "Minimum impact is 1 (low) to 3 (high)".into()));
            }
            if n.margin_multiplier < D::ONE || n.margin_multiplier > D::from(10) {
                return Err(("news", "The margin multiplier must be 1–10".into()));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RoutingMode {
    /// Launch (100 % B-book): routing rules only label and recommend; trades keep the default book and the desk gets
    /// routing suggestions to approve.
    #[default]
    Recommend,
    /// Routing rules decide the book.
    Execute,
}

impl RoutingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            RoutingMode::Recommend => "recommend",
            RoutingMode::Execute => "execute",
        }
    }
}

fn six_hours() -> u32 {
    360
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskSettings {
    #[serde(default)]
    pub routing_mode: RoutingMode,
    /// Client scores are recomputed this often (minutes).
    #[serde(default = "six_hours")]
    pub score_interval_mins: u32,
}

impl Default for RiskSettings {
    fn default() -> Self {
        Self { routing_mode: RoutingMode::Recommend, score_interval_mins: 360 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RiskConfig {
    pub limits: RiskLimits,
    pub settings: RiskSettings,
    pub updated_at: Option<DateTime<Utc>>,
    pub updated_by: Option<String>,
}

impl RiskConfig {
    pub fn json(&self) -> Value {
        json!({"limits": self.limits, "settings": self.settings, "updatedAt": self.updated_at, "updatedBy": self.updated_by, "alertOnly": self.limits.alert_only()})
    }
}

/* ------------------------------------------------------------------ */
/* Storage                                                             */
/* ------------------------------------------------------------------ */

/// The tenant's limits and settings (defaults when nothing is stored).
pub async fn load(pool: &PgPool, tenant_id: i64) -> anyhow::Result<RiskConfig> {
    let mut cfg = RiskConfig::default();
    let rows = sqlx::query("SELECT kind, scope, params, updated_by, updated_at FROM risk_limits WHERE tenant_id = $1 ORDER BY kind, scope").bind(tenant_id).fetch_all(pool).await?;
    for r in rows {
        let kind: String = r.get("kind");
        let params: sqlx::types::Json<Value> = r.get("params");
        let at: DateTime<Utc> = r.get("updated_at");
        if cfg.updated_at.is_none_or(|u| at > u) {
            cfg.updated_at = Some(at);
            cfg.updated_by = Some(r.get("updated_by"));
        }
        let p = params.0;
        let bad = |e: serde_json::Error| tracing::error!(tenant_id, kind = %kind, error = %e, "risk limit row could not be read; skipped");
        match kind.as_str() {
            "exposure" => serde_json::from_value(p).map(|l| cfg.limits.exposure.push(l)).unwrap_or_else(bad),
            "daily_loss" => serde_json::from_value(p).map(|l| cfg.limits.daily_loss = Some(l)).unwrap_or_else(bad),
            "group" => serde_json::from_value(p).map(|l| cfg.limits.groups.push(l)).unwrap_or_else(bad),
            "news" => serde_json::from_value(p).map(|l| cfg.limits.news = Some(l)).unwrap_or_else(bad),
            _ => {}
        }
    }
    // `*` last so the specific rows read first in the list
    cfg.limits.exposure.sort_by(|a, b| (a.symbol == "*").cmp(&(b.symbol == "*")).then(a.symbol.cmp(&b.symbol)));
    if let Some(r) = sqlx::query("SELECT routing_mode, score_interval_mins, updated_by, updated_at FROM risk_settings WHERE tenant_id = $1").bind(tenant_id).fetch_optional(pool).await? {
        cfg.settings.routing_mode = if r.get::<String, _>("routing_mode") == "execute" { RoutingMode::Execute } else { RoutingMode::Recommend };
        cfg.settings.score_interval_mins = r.get::<i32, _>("score_interval_mins").clamp(15, 10_080) as u32;
    }
    Ok(cfg)
}

/// Replaces the tenant's limits (one transaction, with the audit row the caller writes in the same transaction).
pub async fn store(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: i64, limits: &RiskLimits, staff: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM risk_limits WHERE tenant_id = $1").bind(tenant_id).execute(&mut **tx).await?;
    let mut rows: Vec<(&str, String, Value, LimitMode, bool)> = Vec::new();
    for l in &limits.exposure {
        rows.push(("exposure", l.symbol.clone(), serde_json::to_value(l)?, l.mode, l.enabled));
    }
    if let Some(d) = &limits.daily_loss {
        rows.push(("daily_loss", String::new(), serde_json::to_value(d)?, d.mode, d.enabled));
    }
    for g in &limits.groups {
        rows.push(("group", g.group.clone(), serde_json::to_value(g)?, g.mode, g.enabled));
    }
    if let Some(n) = &limits.news {
        rows.push(("news", String::new(), serde_json::to_value(n)?, n.mode, n.enabled));
    }
    for (kind, scope, params, mode, enabled) in rows {
        sqlx::query("INSERT INTO risk_limits (tenant_id, kind, scope, params, mode, enabled, updated_by, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7, now())")
            .bind(tenant_id)
            .bind(kind)
            .bind(scope)
            .bind(sqlx::types::Json(params))
            .bind(mode.as_str())
            .bind(enabled)
            .bind(staff)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

pub async fn store_settings(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: i64, s: &RiskSettings, staff: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO risk_settings (tenant_id, routing_mode, score_interval_mins, updated_by, updated_at) VALUES ($1,$2,$3,$4, now())
         ON CONFLICT (tenant_id) DO UPDATE SET routing_mode = EXCLUDED.routing_mode, score_interval_mins = EXCLUDED.score_interval_mins, updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(tenant_id)
    .bind(s.routing_mode.as_str())
    .bind(s.score_interval_mins as i32)
    .bind(staff)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_parse_with_defaults_and_pick_the_symbol_row_first() {
        let l: RiskLimits = serde_json::from_value(json!({
            "exposure": [{"symbol": "*", "capUsd": 1_000_000}, {"symbol": "EURUSD", "capUsd": 2_000_000, "warnPct": 75, "mode": "enforce"}, {"symbol": "XAUUSD", "capUsd": 5, "enabled": false}],
            "groups": [{"group": "standard", "maxLot": 5, "maxPositions": 20}],
            "news": {"marginMultiplier": 2, "blockPending": true},
        }))
        .unwrap();
        assert_eq!(l.exposure_for("EURUSD").unwrap().cap_usd, D::from(2_000_000));
        assert_eq!(l.exposure_for("EURUSD").unwrap().mode, LimitMode::Enforce);
        // a disabled row falls back to `*`
        assert_eq!(l.exposure_for("XAUUSD").unwrap().symbol, "*");
        assert_eq!(l.exposure_for("GBPUSD").unwrap().warn_pct, D::from(80));
        assert_eq!(l.exposure_for("GBPUSD").unwrap().mode, LimitMode::Alert);
        let n = l.news().unwrap();
        assert_eq!((n.minutes_before, n.minutes_after, n.min_impact, n.mode), (2, 2, 3, LimitMode::Alert));
        assert_eq!(l.group("standard").unwrap().max_positions, Some(20));
        assert!(!l.alert_only());
        assert!(RiskLimits::default().alert_only());
        assert_eq!(RiskSettings::default().routing_mode, RoutingMode::Recommend);
    }
}

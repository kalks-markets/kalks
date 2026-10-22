//! Risk leverage caps (`leverage_caps`, docs/RISK-DESK.md › Leverage control): when the broker is at risk the desk caps
//! the leverage clients get, so they trade mostly on their own capital as margin.
//!
//! A cap has a scope (every account, an account group, an asset class or one account), a max leverage and what it
//! applies to:
//!
//! - `new_positions`: new exposure is margined at the capped leverage and the position keeps that leverage for its
//!   whole life (`Position::margin_leverage`); positions already open keep their margin.
//! - `all_positions`: every open position's margin is recomputed at the capped leverage at once (which can push
//!   accounts into margin call / stop-out: the Back Office previews the impact first).
//!
//! The leverage a position's margin uses is min(account leverage, symbol max leverage, every enabled `all_positions`
//! cap that matches, the position's own `margin_leverage`); new exposure also takes the `new_positions` caps. The most
//! restrictive cap wins. Caps are tenant configuration (`TenantConfig::risk.leverage_caps`): a change applies to the
//! next margin computation, with no restart.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sqlx::{PgPool, Row};

use crate::engine::{Env, metrics};
use crate::model::Account;
use crate::money::{D, HUNDRED, num, num_opt, r2};
use crate::specs::Spec;
use crate::state::AccountState;

/// The engine's asset classes (`config/instruments.json` `asset_class`).
pub const ASSET_CLASSES: [&str; 6] = ["forex", "metals", "indices", "energies", "crypto", "stocks"];

/// Highest leverage a cap may name (the group lists go up to 1:1000; a cap is meant to lower them).
pub const MAX_CAP: u32 = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapScope {
    /// Every account of the broker.
    All,
    /// One account group (`target` = group code).
    Group,
    /// One asset class (`target` = forex, metals, indices, energies, crypto, stocks): the symbols of that class only.
    AssetClass,
    /// One account (`target` = login): a dealer override.
    Account,
}

impl CapScope {
    pub fn as_str(self) -> &'static str {
        match self {
            CapScope::All => "all",
            CapScope::Group => "group",
            CapScope::AssetClass => "asset_class",
            CapScope::Account => "account",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "all" => CapScope::All,
            "group" => CapScope::Group,
            "asset_class" => CapScope::AssetClass,
            "account" => CapScope::Account,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppliesTo {
    /// New exposure only; open positions keep their margin.
    #[default]
    NewPositions,
    /// Open positions are re-margined at the cap at once, and new ones too.
    AllPositions,
}

impl AppliesTo {
    pub fn as_str(self) -> &'static str {
        match self {
            AppliesTo::NewPositions => "new_positions",
            AppliesTo::AllPositions => "all_positions",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeverageCap {
    pub id: i64,
    pub scope: CapScope,
    /// Group code, asset class or login ("" for `all`).
    #[serde(default)]
    pub target: String,
    pub max_leverage: u32,
    pub applies_to: AppliesTo,
    pub enabled: bool,
    pub reason_code: String,
    pub note: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

impl LeverageCap {
    /// Does this enabled cap apply to `acc` on a symbol of asset class `class` (None = the account as a whole: an
    /// asset-class cap never applies then)?
    pub fn matches(&self, acc: &Account, class: Option<&str>) -> bool {
        self.enabled && scope_matches(self.scope, &self.target, acc, class)
    }

    pub fn label(&self) -> String {
        scope_label(self.scope, &self.target)
    }
}

pub fn scope_matches(scope: CapScope, target: &str, acc: &Account, class: Option<&str>) -> bool {
    match scope {
        CapScope::All => true,
        CapScope::Group => target == acc.group,
        CapScope::Account => target == acc.login.to_string(),
        CapScope::AssetClass => class == Some(target),
    }
}

pub fn scope_label(scope: CapScope, target: &str) -> String {
    match scope {
        CapScope::All => "All accounts".into(),
        CapScope::Group => format!("Group {target}"),
        CapScope::AssetClass => format!("Asset class {target}"),
        CapScope::Account => format!("Account {target}"),
    }
}

/// The most restrictive enabled cap matching `acc` (and `class`); `new` = include `new_positions` caps.
pub fn tightest<'a>(caps: &'a [LeverageCap], acc: &Account, class: Option<&str>, new: bool) -> Option<&'a LeverageCap> {
    caps.iter().filter(|c| c.matches(acc, class) && (new || c.applies_to == AppliesTo::AllPositions)).min_by_key(|c| (c.max_leverage, c.id))
}

/// Leverage of one symbol for one account: `live` for the margin of open positions (account, symbol max and the
/// `all_positions` caps), `new` for new exposure (also the `new_positions` caps). Both ≥ 1, `new` ≤ `live`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymLev {
    pub live: u32,
    pub new: u32,
}

pub fn symbol_leverage(caps: &[LeverageCap], acc: &Account, spec: &Spec) -> SymLev {
    let base = acc.leverage.min(spec.max_leverage).max(1);
    if caps.is_empty() {
        return SymLev { live: base, new: base };
    }
    let class = Some(spec.asset_class.as_str());
    let live = tightest(caps, acc, class, false).map_or(base, |c| base.min(c.max_leverage)).max(1);
    let new = tightest(caps, acc, class, true).map_or(live, |c| live.min(c.max_leverage)).max(1);
    SymLev { live, new }
}

/// The account-wide cap (scopes all / group / account) the client's own leverage choice may not exceed.
pub fn account_cap<'a>(caps: &'a [LeverageCap], acc: &Account) -> Option<&'a LeverageCap> {
    tightest(caps, acc, None, true)
}

/// What clients see (account view): the effective leverage for new trades, the account-wide cap, and the effective
/// leverage per asset class where an asset-class cap makes it lower. A symbol's own max leverage still applies on top
/// (`/v1/symbols` `maxLeverage`): effective(symbol) = min(byClass[class] ?? effectiveLeverage, maxLeverage).
pub fn client_json(caps: &[LeverageCap], acc: &Account) -> Value {
    let cap = account_cap(caps, acc).map(|c| c.max_leverage);
    let eff = acc.leverage.min(cap.unwrap_or(u32::MAX)).max(1);
    let mut by = Map::new();
    for class in ASSET_CLASSES {
        if let Some(c) = tightest(caps, acc, Some(class), true)
            && c.max_leverage < eff
        {
            by.insert(class.into(), json!(c.max_leverage.max(1)));
        }
    }
    json!({"effectiveLeverage": eff, "leverageCap": cap, "effectiveLeverageByClass": by})
}

/* ------------------------------------------------------------------ */
/* Requests                                                            */
/* ------------------------------------------------------------------ */

/// A cap as the desk asks for it (preview and apply).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapReq {
    pub scope: String,
    /// Group code, asset class or login (string or number); ignored for `all`.
    #[serde(default)]
    pub target: Option<Value>,
    pub max_leverage: Value,
    #[serde(default)]
    pub applies_to: Option<String>,
}

/// A validated cap request.
#[derive(Clone, Debug, PartialEq)]
pub struct CapSpec {
    pub scope: CapScope,
    pub target: String,
    pub max_leverage: u32,
    pub applies_to: AppliesTo,
}

impl CapSpec {
    /// The cap as it would be stored (id 0 = not stored yet).
    pub fn draft(&self, staff: &str, now: DateTime<Utc>) -> LeverageCap {
        LeverageCap { id: 0, scope: self.scope, target: self.target.clone(), max_leverage: self.max_leverage, applies_to: self.applies_to, enabled: true, reason_code: String::new(), note: String::new(), created_by: staff.into(), created_at: now, updated_by: staff.into(), updated_at: now }
    }

    /// `caps` with this cap in place of the cap of the same scope (the result of applying it).
    pub fn applied_to(&self, caps: &[LeverageCap], staff: &str, now: DateTime<Utc>) -> Vec<LeverageCap> {
        let mut out: Vec<LeverageCap> = caps.iter().filter(|c| !(c.scope == self.scope && c.target == self.target)).cloned().collect();
        out.push(self.draft(staff, now));
        out
    }
}

/// Field and message of the first invalid value. `group_exists` checks a group code of the tenant; an account target
/// is only checked for its shape here (the caller checks it belongs to the tenant).
pub fn validate(r: &CapReq, group_exists: impl Fn(&str) -> bool) -> Result<CapSpec, (&'static str, String)> {
    let scope = CapScope::parse(r.scope.trim()).ok_or(("scope", "scope must be all, group, asset_class or account".to_string()))?;
    let max = match &r.max_leverage {
        Value::Number(n) => n.as_u64().filter(|v| *v >= 1 && *v <= MAX_CAP as u64),
        Value::String(s) => s.trim().parse::<u64>().ok().filter(|v| *v >= 1 && *v <= MAX_CAP as u64),
        _ => None,
    }
    .ok_or(("maxLeverage", format!("Max leverage must be a whole number from 1 to {MAX_CAP}")))? as u32;
    let applies_to = match r.applies_to.as_deref().map(str::trim) {
        None | Some("") | Some("new_positions") => AppliesTo::NewPositions,
        Some("all_positions") => AppliesTo::AllPositions,
        Some(_) => return Err(("appliesTo", "appliesTo must be new_positions or all_positions".into())),
    };
    let raw = match &r.target {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(_) => return Err(("target", "Invalid target".into())),
    };
    let target = match scope {
        CapScope::All => String::new(),
        CapScope::Group => {
            if !group_exists(&raw) {
                return Err(("target", format!("Unknown group {raw}")));
            }
            raw
        }
        CapScope::AssetClass => {
            let c = raw.to_lowercase();
            if !ASSET_CLASSES.contains(&c.as_str()) {
                return Err(("target", format!("Asset class must be one of {}", ASSET_CLASSES.join(", "))));
            }
            c
        }
        CapScope::Account => {
            let l: i64 = raw.parse().ok().filter(|l| *l > 0).ok_or(("target", "Enter the account login".to_string()))?;
            l.to_string()
        }
    };
    Ok(CapSpec { scope, target, max_leverage: max, applies_to })
}

/* ------------------------------------------------------------------ */
/* Preview                                                             */
/* ------------------------------------------------------------------ */

/// One account's impact of a cap: `now` is the engine as it is, `after` the same with the cap applied. None when the
/// account is outside the cap's scope. `in_scope` for an asset-class cap = the account holds a CFD position of the
/// class (any account could trade it, but only those are re-margined).
pub fn preview_account(now: &Env, after: &Env, st: &AccountState, spec: &CapSpec) -> Option<Value> {
    let acc = &st.account;
    let cfd: Vec<&crate::model::Position> = st.positions.values().filter(|p| p.option.is_none()).collect();
    let in_scope = match spec.scope {
        CapScope::AssetClass => cfd.iter().any(|p| now.specs.get(&p.symbol).is_some_and(|s| s.asset_class == spec.target)),
        s => scope_matches(s, &spec.target, acc, None),
    };
    if !in_scope {
        return None;
    }
    let positions = cfd.iter().filter(|p| now.specs.get(&p.symbol).is_some_and(|s| scope_matches(spec.scope, &spec.target, acc, Some(&s.asset_class)))).count();
    let before = metrics(now, st);
    let aft = metrics(after, st);
    let g = after.group;
    let status = match aft.level {
        Some(l) if l <= g.stop_out_pct => "stop_out",
        Some(l) if l <= g.margin_call_pct => "margin_call",
        _ => "ok",
    };
    let f = acc.usd_factor();
    Some(json!({
        "login": acc.login.to_string(),
        "userId": acc.user_id,
        "type": acc.kind.as_str(),
        "group": acc.group,
        "currency": acc.ccy(),
        "leverage": acc.leverage,
        "positions": positions,
        "equity": num(r2(before.equity)),
        "marginBefore": num(r2(before.margin)),
        "marginAfter": num(r2(aft.margin)),
        "freeMarginAfter": num(r2(aft.free_margin)),
        "marginLevelBefore": num_opt(before.level.map(r2)),
        "marginLevelAfter": num_opt(aft.level.map(r2)),
        "marginCallLevel": num(g.margin_call_pct),
        "stopOutLevel": num(g.stop_out_pct),
        "status": status,
        "changed": r2(before.margin) != r2(aft.margin),
        // USD for the totals (cent accounts are in USC)
        "equityUsd": num(r2(before.equity / f)),
        "marginBeforeUsd": num(r2(before.margin / f)),
        "marginAfterUsd": num(r2(aft.margin / f)),
    }))
}

/// Totals of the per-account rows of a preview (all accounts and live only), the accounts falling into margin call /
/// stop-out, and the affected accounts (lowest margin level first, at most `limit`).
pub fn preview_summary(spec: &CapSpec, rows: Vec<Value>, limit: usize) -> Value {
    let dec = |v: &Value| v.as_f64().and_then(crate::money::from_f64).unwrap_or_default();
    let tot = |rows: &[&Value]| {
        let before: D = rows.iter().map(|r| dec(&r["marginBeforeUsd"])).sum();
        let after: D = rows.iter().map(|r| dec(&r["marginAfterUsd"])).sum();
        json!({
            "accountsInScope": rows.len(),
            "accountsAffected": rows.iter().filter(|r| r["changed"] == true).count(),
            "positionsAffected": rows.iter().filter(|r| r["changed"] == true).map(|r| r["positions"].as_u64().unwrap_or(0)).sum::<u64>(),
            "marginBeforeUsd": num(r2(before)),
            "marginAfterUsd": num(r2(after)),
            "marginIncreaseUsd": num(r2(after - before)),
            "marginCall": rows.iter().filter(|r| r["status"] == "margin_call").count(),
            "stopOut": rows.iter().filter(|r| r["status"] == "stop_out").count(),
        })
    };
    let all: Vec<&Value> = rows.iter().collect();
    let live: Vec<&Value> = rows.iter().filter(|r| r["type"] == "live").collect();
    let mut affected: Vec<&Value> = rows.iter().filter(|r| r["changed"] == true || r["status"] != "ok").collect();
    let level = |r: &Value| r["marginLevelAfter"].as_f64().unwrap_or(f64::MAX);
    affected.sort_by(|a, b| level(a).total_cmp(&level(b)).then(a["login"].as_str().cmp(&b["login"].as_str())));
    let truncated = affected.len() > limit;
    let pick = |status: &str| -> Vec<Value> { rows.iter().filter(|r| r["status"] == status).take(limit).cloned().collect() };
    json!({
        "scope": spec.scope.as_str(),
        "target": spec.target,
        "label": scope_label(spec.scope, &spec.target),
        "maxLeverage": spec.max_leverage,
        "appliesTo": spec.applies_to.as_str(),
        "totals": tot(&all),
        "live": tot(&live),
        "marginCallAccounts": pick("margin_call"),
        "stopOutAccounts": pick("stop_out"),
        "accounts": affected.into_iter().take(limit).cloned().collect::<Vec<_>>(),
        "truncated": truncated,
        "generatedAt": Utc::now(),
    })
}

/// Margin level in % (for tests and callers that only have the two numbers).
pub fn level(equity: D, margin: D) -> Option<D> {
    (margin > D::ZERO).then(|| equity / margin * HUNDRED)
}

/* ------------------------------------------------------------------ */
/* Storage                                                             */
/* ------------------------------------------------------------------ */

fn from_row(r: &sqlx::postgres::PgRow) -> LeverageCap {
    LeverageCap {
        id: r.get("id"),
        scope: CapScope::parse(&r.get::<String, _>("scope")).unwrap_or(CapScope::All),
        target: r.get("target"),
        max_leverage: r.get::<i32, _>("max_leverage").max(1) as u32,
        applies_to: if r.get::<String, _>("applies_to") == "all_positions" { AppliesTo::AllPositions } else { AppliesTo::NewPositions },
        enabled: r.get("enabled"),
        reason_code: r.get("reason_code"),
        note: r.get("note"),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_by: r.get("updated_by"),
        updated_at: r.get("updated_at"),
    }
}

/// Every cap of the tenant (disabled ones too: the desk sees the history; the engine reads enabled ones only).
pub async fn load(pool: &PgPool, tenant_id: i64) -> anyhow::Result<Vec<LeverageCap>> {
    let rows = sqlx::query("SELECT * FROM leverage_caps WHERE tenant_id = $1 ORDER BY enabled DESC, max_leverage, id").bind(tenant_id).fetch_all(pool).await?;
    Ok(rows.iter().map(from_row).collect())
}

/// Creates the cap of a scope, or replaces it (re-enabled) when that scope already has one.
pub async fn upsert(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: i64, spec: &CapSpec, reason_code: &str, note: &str, staff: &str) -> anyhow::Result<LeverageCap> {
    let r = sqlx::query(
        "INSERT INTO leverage_caps (tenant_id, scope, target, max_leverage, applies_to, enabled, reason_code, note, created_by, updated_by)
         VALUES ($1,$2,$3,$4,$5, true, $6,$7,$8,$8)
         ON CONFLICT (tenant_id, scope, target) DO UPDATE SET max_leverage = EXCLUDED.max_leverage, applies_to = EXCLUDED.applies_to, enabled = true,
           reason_code = EXCLUDED.reason_code, note = EXCLUDED.note, updated_by = EXCLUDED.updated_by, updated_at = now()
         RETURNING *",
    )
    .bind(tenant_id)
    .bind(spec.scope.as_str())
    .bind(&spec.target)
    .bind(spec.max_leverage as i32)
    .bind(spec.applies_to.as_str())
    .bind(reason_code)
    .bind(note)
    .bind(staff)
    .fetch_one(&mut **tx)
    .await?;
    Ok(from_row(&r))
}

/// Disables cap `id` (the row stays for the history). None = no such cap of the tenant.
pub async fn disable(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, tenant_id: i64, id: i64, reason_code: &str, note: &str, staff: &str) -> anyhow::Result<Option<LeverageCap>> {
    let r = sqlx::query("UPDATE leverage_caps SET enabled = false, reason_code = $3, note = $4, updated_by = $5, updated_at = now() WHERE tenant_id = $1 AND id = $2 RETURNING *")
        .bind(tenant_id)
        .bind(id)
        .bind(reason_code)
        .bind(note)
        .bind(staff)
        .fetch_optional(&mut **tx)
        .await?;
    Ok(r.as_ref().map(from_row))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_validate() {
        let ok = |v: Value| validate(&serde_json::from_value(v).unwrap(), |g| g == "standard");
        let s = ok(json!({"scope": "asset_class", "target": "Crypto", "maxLeverage": 10, "appliesTo": "all_positions"})).unwrap();
        assert_eq!((s.scope, s.target.as_str(), s.max_leverage, s.applies_to), (CapScope::AssetClass, "crypto", 10, AppliesTo::AllPositions));
        let s = ok(json!({"scope": "all", "target": "ignored", "maxLeverage": "5"})).unwrap();
        assert_eq!((s.target.as_str(), s.applies_to), ("", AppliesTo::NewPositions));
        assert_eq!(ok(json!({"scope": "account", "target": 10000001, "maxLeverage": 2})).unwrap().target, "10000001");
        assert_eq!(ok(json!({"scope": "group", "target": "pro", "maxLeverage": 10})).unwrap_err().0, "target");
        assert_eq!(ok(json!({"scope": "asset_class", "target": "bonds", "maxLeverage": 10})).unwrap_err().0, "target");
        assert_eq!(ok(json!({"scope": "all", "maxLeverage": 0})).unwrap_err().0, "maxLeverage");
        assert_eq!(ok(json!({"scope": "all", "maxLeverage": 2.5})).unwrap_err().0, "maxLeverage");
        assert_eq!(ok(json!({"scope": "all", "maxLeverage": 10, "appliesTo": "some"})).unwrap_err().0, "appliesTo");
        assert_eq!(ok(json!({"scope": "everyone", "maxLeverage": 10})).unwrap_err().0, "scope");
    }
}

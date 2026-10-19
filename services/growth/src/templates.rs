//! Contest templates (phase 4): ready-made live contests the Back Office wizard starts from (pick a template,
//! set the dates and the prize pool, review and publish). Seeded per broker on first use and editable. A template
//! turns into an ordinary contest input (validated by the same code as a hand-made contest): the window comes from
//! its schedule, the prizes from the pool and the split.

use crate::challenge_calc::{self as cc, Band};
use crate::model::{AntiCheat, opt_num};
use crate::money::{D, dec, num};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;

/// The default templates: (key, name, description, scoring, schedule, days, min trades, asset classes, pool, split,
/// payout, min equity, rules).
#[allow(clippy::type_complexity)]
const DEFAULTS: &[(&str, &str, &str, &str, &str, i32, i32, &[&str], &str, &str, &str, Option<&str>, &str)] = &[
    (
        "weekly-return",
        "Weekly live contest – best % return",
        "A free week-long contest on your own live account. The best return wins.",
        "return_pct",
        "next_monday",
        7,
        5,
        &[],
        "1000",
        "top3",
        "wallet",
        Some("100"),
        "Free to enter. Verified clients only. Ranked by return % (realised + floating P&L ÷ starting equity). At least 5 closed trades to rank. Deposits, withdrawals or transfers on the entered account during the contest disqualify the entry. Prizes are paid to your wallet after the results are final.",
    ),
    (
        "monthly-lots",
        "Monthly lots race",
        "Trade the most lots this month on a live account and win trading credit.",
        "lots",
        "next_month",
        30,
        10,
        &[],
        "2500",
        "top10",
        "credit",
        None,
        "Free to enter. Verified clients only. Ranked by lots closed inside the month on the entered live account (trades held at least 60 seconds). At least 10 closed trades to rank. Prizes are paid as trading credit on the entered account after the results are final.",
    ),
    (
        "weekend-crypto",
        "Weekend crypto sprint",
        "Saturday and Sunday, crypto CFDs only. The best return over the weekend wins.",
        "return_pct",
        "next_weekend",
        2,
        3,
        &["crypto"],
        "500",
        "top5",
        "wallet",
        None,
        "Free to enter. Verified clients only. Only crypto CFD trades opened and closed during the weekend count. At least 3 closed trades to rank. Prizes are paid to your wallet after the results are final.",
    ),
];

/// Seeds the default templates once per broker.
pub async fn seed(pool: &sqlx::PgPool, tenant: &str) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))").bind(format!("growth-templates:{tenant}")).execute(&mut *tx).await?;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM contest_templates WHERE tenant = $1").bind(tenant).fetch_one(&mut *tx).await?;
    if n == 0 {
        let anti = AntiCheat { min_hold_seconds: 60, max_single_trade_pct: dec("80"), disqualify_on_balance_change: true };
        for (i, (key, name, desc, scoring, schedule, days, min_trades, classes, pool_usd, split, payout, min_equity, rules)) in DEFAULTS.iter().enumerate() {
            sqlx::query(
                "INSERT INTO contest_templates (tenant, key, name, description, kind, instrument, scoring, schedule, duration_days, min_trades, asset_classes, kyc_required,
                   min_equity, prize_pool, split, payout, rules, anti_cheat, sort, created_by)
                 VALUES ($1,$2,$3,$4,'live','cfd',$5,$6,$7,$8,$9,true,$10,$11,$12,$13,$14,$15,$16,'system') ON CONFLICT DO NOTHING",
            )
            .bind(tenant)
            .bind(key)
            .bind(name)
            .bind(desc)
            .bind(scoring)
            .bind(schedule)
            .bind(days)
            .bind(min_trades)
            .bind(classes.iter().map(|c| c.to_string()).collect::<Vec<_>>())
            .bind(min_equity.map(dec))
            .bind(dec(pool_usd))
            .bind(sqlx::types::Json(json!(split)))
            .bind(payout)
            .bind(rules)
            .bind(sqlx::types::Json(&anti))
            .bind((i as i32 + 1) * 10)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// A split: a preset key ("top3") or explicit bands.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Split {
    Preset(String),
    Bands(Vec<Band>),
}

impl Split {
    pub fn bands(&self) -> Result<Vec<Band>, String> {
        let b = match self {
            Split::Preset(k) => cc::preset(k).ok_or_else(|| format!("Unknown split preset '{k}'."))?,
            Split::Bands(b) => b.clone(),
        };
        cc::validate_bands(&b)?;
        Ok(b)
    }
}

pub fn split_of(r: &sqlx::postgres::PgRow) -> Split {
    serde_json::from_value(r.get::<sqlx::types::Json<Value>, _>("split").0).unwrap_or(Split::Preset("top3".into()))
}

pub fn template_json(r: &sqlx::postgres::PgRow) -> Value {
    let anti: AntiCheat = serde_json::from_value(r.get::<sqlx::types::Json<Value>, _>("anti_cheat").0).unwrap_or_default();
    json!({
        "id": r.get::<i64, _>("id"),
        "key": r.get::<String, _>("key"),
        "name": r.get::<String, _>("name"),
        "description": r.get::<String, _>("description"),
        "kind": r.get::<String, _>("kind"),
        "instrument": r.get::<String, _>("instrument"),
        "scoring": r.get::<String, _>("scoring"),
        "schedule": r.get::<String, _>("schedule"),
        "durationDays": r.get::<i32, _>("duration_days"),
        "minTrades": r.get::<i32, _>("min_trades"),
        "maxEntrants": r.get::<Option<i32>, _>("max_entrants"),
        "accountGroups": r.get::<Vec<String>, _>("account_groups"),
        "assetClasses": r.get::<Vec<String>, _>("asset_classes"),
        "kycRequired": r.get::<bool, _>("kyc_required"),
        "minEquity": opt_num(r.get("min_equity")),
        "prizePool": num(r.get("prize_pool")),
        "split": split_of(r),
        "payout": r.get::<String, _>("payout"),
        "rules": r.get::<String, _>("rules"),
        "antiCheat": anti,
        "active": r.get::<bool, _>("active"),
        "sort": r.get::<i32, _>("sort"),
        "updatedAt": r.get::<DateTime<Utc>, _>("updated_at"),
    })
}

/// What the wizard changes on a template (step 2): dates, pool, split, payout, name, publish state.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FromTemplate {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
    #[serde(default, deserialize_with = "crate::money::de_opt_dec")]
    pub prize_pool: Option<D>,
    #[serde(default)]
    pub split: Option<Split>,
    #[serde(default)]
    pub payout: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
}

/// The contest input (the body `POST /admin/contests` takes) for a template, with the wizard's choices.
pub fn contest_input(t: &Value, o: &FromTemplate, now: DateTime<Utc>) -> Result<Value, (&'static str, String)> {
    let (s, e) = cc::next_window(t["schedule"].as_str().unwrap_or("next_monday"), t["durationDays"].as_i64().unwrap_or(7), now);
    let starts = o.starts_at.clone().unwrap_or_else(|| s.to_rfc3339());
    let ends = o.ends_at.clone().unwrap_or_else(|| e.to_rfc3339());
    let pool = match o.prize_pool {
        Some(p) => p,
        None => crate::money::value_dec(&t["prizePool"]).unwrap_or_default(),
    };
    if pool < D::ZERO || pool > dec("1000000") {
        return Err(("prizePool", "The prize pool must be 0 – 1 000 000 USD.".into()));
    }
    let split: Split = match &o.split {
        Some(x) => x.clone(),
        None => serde_json::from_value(t["split"].clone()).unwrap_or(Split::Preset("top3".into())),
    };
    let bands = split.bands().map_err(|m| ("split", m))?;
    let payout = o.payout.clone().unwrap_or_else(|| t["payout"].as_str().unwrap_or("wallet").to_string());
    if payout != "wallet" && payout != "credit" {
        return Err(("payout", "wallet or credit.".into()));
    }
    let prizes = cc::split_pool(pool, &bands, &payout);
    Ok(json!({
        "name": o.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| t["name"].as_str().unwrap_or("").to_string()),
        "description": t["description"],
        "kind": t["kind"],
        "instrument": t["instrument"],
        "startsAt": starts,
        "endsAt": ends,
        "scoring": t["scoring"],
        "minTrades": t["minTrades"],
        "maxEntrants": t["maxEntrants"],
        "accountGroups": t["accountGroups"],
        "assetClasses": t["assetClasses"],
        "kycRequired": t["kycRequired"],
        "minEquity": t["minEquity"],
        "prizes": prizes,
        "rules": t["rules"],
        "antiCheat": t["antiCheat"],
        "status": o.status.clone().unwrap_or_else(|| "scheduled".into()),
        "templateId": t["id"],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_becomes_a_contest_input() {
        let t = json!({"id": 3, "name": "Weekly live contest – best % return", "description": "d", "kind": "live", "instrument": "cfd", "scoring": "return_pct",
                       "schedule": "next_monday", "durationDays": 7, "minTrades": 5, "maxEntrants": null, "accountGroups": [], "assetClasses": [],
                       "kycRequired": true, "minEquity": 100, "prizePool": 1000, "split": "top3", "payout": "wallet", "rules": "r", "antiCheat": {}});
        let now = DateTime::parse_from_rfc3339("2026-10-21T10:00:00Z").unwrap().with_timezone(&Utc);
        let v = contest_input(&t, &FromTemplate::default(), now).unwrap();
        assert_eq!(v["startsAt"], "2026-10-25T21:00:00+00:00");
        assert_eq!(v["prizes"].as_array().unwrap().len(), 3);
        assert_eq!(v["prizes"][0]["amount"], 500.0);
        assert_eq!(v["templateId"], 3);
        assert_eq!(v["status"], "scheduled");
        let o = FromTemplate { prize_pool: Some(dec("300")), split: Some(Split::Preset("winner".into())), payout: Some("credit".into()), name: Some("Halloween".into()), ..Default::default() };
        let v = contest_input(&t, &o, now).unwrap();
        assert_eq!(v["name"], "Halloween");
        assert_eq!(v["prizes"], json!([{"rankFrom": 1, "rankTo": 1, "amount": 300.0, "payout": "credit"}]));
        let bad = FromTemplate { split: Some(Split::Preset("nope".into())), ..Default::default() };
        assert_eq!(contest_input(&t, &bad, now).unwrap_err().0, "split");
        let bad = FromTemplate { payout: Some("cash".into()), ..Default::default() };
        assert_eq!(contest_input(&t, &bad, now).unwrap_err().0, "payout");
    }
}

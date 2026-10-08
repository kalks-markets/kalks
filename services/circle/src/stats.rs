//! Engine-verified trader stats (Q6, Q18, Q20): computed from the member's live accounts' closed trades over
//! the last 180 days (engine history), never typed in. Shown on the profile only when the member opts in
//! (`showStats`); the risk-adjusted return ranks the traders leaderboard and Explore.
//!
//! - trades, win rate: closing deals (net of swap and commission)
//! - monthly return: last-30-day net P&L over the balance 30 days ago (closed-trade P&L; deposits and
//!   withdrawals are not counted as performance)
//! - max drawdown: largest peak-to-trough fall of the closed-trade balance curve
//! - risk-adjusted: 90-day return ÷ max(max drawdown, 1 %)

use crate::state::AppState;
use crate::upstream;
use chrono::{DateTime, Datelike, Duration, Utc};
use serde_json::{Value, json};

fn f(v: &Value) -> f64 {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0)
}

fn round(x: f64, dp: i32) -> f64 {
    let m = 10f64.powi(dp);
    (x * m).round() / m
}

/// (close time, net USD) of each closing deal + the current balance (USD) → the stats.
pub fn compute(mut closes: Vec<(DateTime<Utc>, f64)>, balance_now: f64, now: DateTime<Utc>) -> Value {
    closes.sort_by_key(|c| c.0);
    let trades = closes.len();
    if trades == 0 {
        return json!({"trades": 0});
    }
    let wins = closes.iter().filter(|c| c.1 > 0.0).count();
    let total: f64 = closes.iter().map(|c| c.1).sum();
    let start = balance_now - total;
    // balance curve (closed trades only)
    let mut bal = start;
    let mut peak = start.max(0.0);
    let mut max_dd = 0.0f64;
    for (_, net) in &closes {
        bal += net;
        if bal > peak {
            peak = bal;
        }
        if peak > 0.0 {
            max_dd = max_dd.max((peak - bal) / peak * 100.0);
        }
    }
    let since = |d: i64| -> f64 { closes.iter().filter(|c| c.0 >= now - Duration::days(d)).map(|c| c.1).sum() };
    let ret = |d: i64| -> Option<f64> {
        let p = since(d);
        let base = balance_now - p;
        (base > 0.0).then(|| p / base * 100.0)
    };
    let r30 = ret(30);
    let r90 = ret(90);
    // last 6 calendar months
    let mut monthly = Vec::new();
    for back in (0..6).rev() {
        let (mut y, mut m) = (now.year(), now.month() as i32 - back);
        while m <= 0 {
            m += 12;
            y -= 1;
        }
        let in_month: Vec<&(DateTime<Utc>, f64)> = closes.iter().filter(|c| c.0.year() == y && c.0.month() as i32 == m).collect();
        let pnl: f64 = in_month.iter().map(|c| c.1).sum();
        // balance at the start of the month = now − P&L since then
        let after: f64 = closes.iter().filter(|c| (c.0.year(), c.0.month() as i32) >= (y, m)).map(|c| c.1).sum();
        let base = balance_now - after;
        monthly.push(json!({"month": format!("{y}-{m:02}"), "returnPct": if base > 0.0 { json!(round(pnl / base * 100.0, 2)) } else { Value::Null }, "trades": in_month.len()}));
    }
    let risk_adjusted = r90.map(|r| round(r / max_dd.max(1.0), 3));
    json!({
        "trades": trades,
        "winRate": round(wins as f64 / trades as f64 * 100.0, 1),
        "monthlyReturnPct": r30.map(|r| round(r, 2)),
        "returnPct90d": r90.map(|r| round(r, 2)),
        "maxDrawdownPct": round(max_dd, 2),
        "riskAdjusted": risk_adjusted,
        "monthly": monthly,
        "periodDays": 180,
    })
}

/// Fetches the member's live accounts and history and stores the stats.
pub async fn refresh(st: &AppState, user: i64) -> anyhow::Result<Value> {
    let tenant: String = sqlx::query_scalar("SELECT tenant FROM profiles WHERE user_id = $1").bind(user).fetch_one(&st.pool).await?;
    let accounts = upstream::accounts(st, &tenant, user).await?;
    let now = Utc::now();
    let mut closes = Vec::new();
    let mut balance = 0.0;
    for a in accounts.iter().filter(|a| a["type"].as_str() == Some("live")).take(5) {
        let Some(login) = a["login"].as_i64() else { continue };
        let cent = a["cent"].as_bool().unwrap_or(false) || a["currency"].as_str() == Some("USC");
        let usd = |x: f64| if cent { x / 100.0 } else { x };
        balance += usd(f(&a["balance"]));
        for d in upstream::history(st, &tenant, user, login, Some(now - Duration::days(180)), 1000).await?.unwrap_or_default() {
            if d["entry"].as_str() == Some("in") {
                continue;
            }
            let Some(t) = d["time"].as_str().and_then(|t| DateTime::parse_from_rfc3339(t).ok()) else { continue };
            closes.push((t.with_timezone(&Utc), usd(f(&d["profit"]) + f(&d["swap"]) - f(&d["commission"]).abs())));
        }
    }
    let stats = compute(closes, balance, now);
    let s = crate::profiles::settings(st).await;
    let risk = (stats["trades"].as_i64().unwrap_or(0) >= s.leaderboard_min_trades).then(|| stats["riskAdjusted"].as_f64()).flatten();
    sqlx::query("UPDATE profiles SET stats = $2, stats_at = now(), risk_score = $3 WHERE user_id = $1").bind(user).bind(sqlx::types::Json(&stats)).bind(risk).execute(&st.pool).await?;
    Ok(stats)
}

/// Background refresh of opted-in members (oldest first, a few per run).
pub async fn refresh_due(st: &AppState) -> anyhow::Result<usize> {
    let due: Vec<i64> = sqlx::query_scalar("SELECT user_id FROM profiles WHERE show_stats AND status = 'active' AND (stats_at IS NULL OR stats_at < now() - interval '6 hours') ORDER BY stats_at NULLS FIRST LIMIT 10")
        .fetch_all(&st.pool)
        .await?;
    for u in &due {
        if let Err(e) = refresh(st, *u).await {
            tracing::debug!(error = %e, user = u, "stats refresh failed");
            // try again later, not every minute
            sqlx::query("UPDATE profiles SET stats_at = now() - interval '5 hours' WHERE user_id = $1").bind(u).execute(&st.pool).await?;
        }
    }
    Ok(due.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn computes_verified_stats() {
        let now = Utc.with_ymd_and_hms(2026, 10, 9, 12, 0, 0).unwrap();
        let d = |days: i64, net: f64| (now - Duration::days(days), net);
        // start 1000: +100 (1100), -220 (880, dd 20 %), +120 (1000), +100 (1100)
        let s = compute(vec![d(100, 100.0), d(60, -220.0), d(20, 120.0), d(5, 100.0)], 1100.0, now);
        assert_eq!(s["trades"], 4);
        assert_eq!(s["winRate"], 75.0);
        assert_eq!(s["maxDrawdownPct"], 20.0);
        // 30 days: +220 on 880
        assert_eq!(s["monthlyReturnPct"], 25.0);
        // 90 days: 0 on 1100
        assert_eq!(s["returnPct90d"], 0.0);
        assert_eq!(s["riskAdjusted"], 0.0);
        assert_eq!(s["monthly"].as_array().unwrap().len(), 6);
        assert_eq!(s["monthly"][5]["month"], "2026-10");
        assert_eq!(compute(vec![], 500.0, now)["trades"], 0);
    }
}

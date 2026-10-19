//! Client profitability score (0-100) per trading account: how likely the client is to win against the house on the
//! B-book, with a category and a plain-language "why" list carrying the numbers behind every factor.
//!
//! The pure part ([`compute`]) reads one account's closed trades (90 days), a few cross-account facts the job queries
//! (identical opens on other accounts, hedges across the client's own accounts), money facts (deposits, bonus,
//! withdrawals), the client facts from the gateway (country, IB source) and two optional market facts (high-impact
//! news times, one-minute price jumps from market-data). Every optional source degrades gracefully: the factor says
//! what it could not check.
//!
//! Score = 50 + confidence × Σ factor points (confidence = n / (n + 20) closed trades), clamped to 0-100.
//! Categories: `toxic` (any toxic-flow signal: latency arbitrage, gap-only trading, copied trades / multi-accounting,
//! bonus abuse), `likely_profitable` (≥ 65), `likely_losing` (≤ 35), `neutral` otherwise.
//!
//! The job ([`run`]) scores every live account with recent trading on a schedule (`scoreIntervalMins`, default 6 h)
//! and on demand, keeps every result in `client_scores`, feeds the routing conditions (`RiskCtx::scores`) and raises
//! an alert when an account turns toxic.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::Ordering;

use super::ScoreLite;
use crate::api::AppState;
use crate::money::D;
use crate::specs::Specs;

/// Days of history a score reads.
pub const WINDOW_DAYS: i64 = 90;
/// A trade held less than this is a scalp (seconds).
pub const SCALP_SECS: f64 = 60.0;
/// Latency-arbitrage candidates: profitable trades held less than this (seconds).
pub const ARB_SECS: f64 = 30.0;

/// One closed trade (an exit deal), money in USD.
#[derive(Clone, Debug, PartialEq)]
pub struct Trade {
    pub symbol: String,
    /// `buy` | `sell` (the position's side)
    pub side: String,
    pub volume: f64,
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    /// Price P&L + swap − commission share, USD.
    pub net_usd: f64,
    /// Source of the opening order (`manual`, `api`, `copy`, …).
    pub open_source: String,
}

impl Trade {
    pub fn hold_secs(&self) -> f64 {
        (self.close_time - self.open_time).num_milliseconds() as f64 / 1000.0
    }
}

/// Everything besides the account's own trades.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub deposits_usd: f64,
    pub withdrawals_usd: f64,
    pub bonus_usd: f64,
    pub first_bonus_at: Option<DateTime<Utc>>,
    /// Withdrawals (transfer out) booked within 7 days after the first bonus.
    pub withdrawals_after_bonus: usize,
    pub account_created: Option<DateTime<Utc>>,
    pub country: Option<String>,
    pub ib_source: Option<String>,
    /// Entry deals of the account (90 days, the client's own opens: copy / PAMM / MAM / dealer excluded).
    pub opens: usize,
    /// Of those, opened within 2 s of an identical open (symbol, side, volume) on another login.
    pub identical_opens: usize,
    /// Distinct other logins those identical opens were on.
    pub identical_logins: usize,
    /// Opposite opens on the same symbol within 60 s on another account of the same client.
    pub self_hedges: usize,
    /// High-impact news times (None = news calendar unavailable).
    pub news: Option<Vec<DateTime<Utc>>>,
    /// Tickets → whether the minute the trade opened in (or the one before) was a price jump (None = not checked).
    pub jumps: Option<HashMap<usize, bool>>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Why {
    pub factor: String,
    pub label: String,
    pub value: String,
    pub points: f64,
    pub detail: String,
    /// `up` = in the client's favour (risk for the B-book), `down` = against, `warn` = toxic-flow signal.
    pub tone: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub score: f64,
    pub category: &'static str,
    pub trades: usize,
    pub win_rate: Option<f64>,
    pub profit_factor: Option<f64>,
    pub net_pnl_usd: f64,
    pub avg_hold_secs: Option<f64>,
    pub scalping_share: Option<f64>,
    pub toxic_flags: Vec<&'static str>,
    pub why: Vec<Why>,
    pub metrics: BTreeMap<&'static str, Value>,
}

fn round(v: f64, dp: i32) -> f64 {
    let m = 10f64.powi(dp);
    (v * m).round() / m
}

fn pct(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

fn usd(v: f64) -> String {
    let s = if v < 0.0 { "-" } else { "" };
    let a = v.abs();
    if a >= 1_000_000.0 {
        format!("{s}{:.2}M USD", a / 1_000_000.0)
    } else if a >= 10_000.0 {
        format!("{s}{:.1}k USD", a / 1000.0)
    } else {
        format!("{s}{a:.2} USD")
    }
}

fn secs(v: f64) -> String {
    if v < 120.0 {
        format!("{v:.0} s")
    } else if v < 7200.0 {
        format!("{:.0} min", v / 60.0)
    } else if v < 172_800.0 {
        format!("{:.1} h", v / 3600.0)
    } else {
        format!("{:.1} days", v / 86_400.0)
    }
}

fn clamp(v: f64, lim: f64) -> f64 {
    if v.is_finite() { v.clamp(-lim, lim) } else { 0.0 }
}

/// Was the trade opened shortly before a session close and closed soon after the reopen (held through a gap), or
/// opened in the first minutes after a reopen?
pub fn gap_trade(specs: &Specs, t: &Trade) -> bool {
    let Some(spec) = specs.get(&t.symbol) else { return false };
    let before_close = spec.is_open(t.open_time) && !spec.is_open(t.open_time + Duration::minutes(15));
    let after_reopen = |at: DateTime<Utc>, mins: i64| spec.is_open(at) && !spec.is_open(at - Duration::minutes(mins));
    (before_close && after_reopen(t.close_time, 30)) || after_reopen(t.open_time, 2)
}

/// The score of one account from its closed trades and facts (pure; see the module doc).
pub fn compute(trades: &[Trade], facts: &Facts, specs: &Specs, now: DateTime<Utc>) -> Outcome {
    let n = trades.len();
    let mut why: Vec<Why> = Vec::new();
    let mut flags: Vec<&'static str> = Vec::new();
    let mut metrics: BTreeMap<&'static str, Value> = BTreeMap::new();
    let add = |why: &mut Vec<Why>, factor: &str, label: &str, value: String, points: f64, detail: String| {
        let tone = if points > 0.5 { "up" } else if points < -0.5 { "down" } else { "neutral" };
        why.push(Why { factor: factor.into(), label: label.into(), value, points: round(points, 1), detail, tone });
    };
    let conf = n as f64 / (n as f64 + 20.0);
    add(&mut why, "sample", "Closed trades (90 days)", n.to_string(), 0.0, format!("{n} closed trades; the factors below count {:.0}% (more trades, more weight)", conf * 100.0));

    let wins: Vec<f64> = trades.iter().filter(|t| t.net_usd > 0.0).map(|t| t.net_usd).collect();
    let losses: Vec<f64> = trades.iter().filter(|t| t.net_usd < 0.0).map(|t| -t.net_usd).collect();
    let gross_win: f64 = wins.iter().sum();
    let gross_loss: f64 = losses.iter().sum();
    let net: f64 = trades.iter().map(|t| t.net_usd).sum();
    let win_rate = (n > 0).then(|| wins.len() as f64 / n as f64);
    let profit_factor = if gross_loss > 0.0 { Some(gross_win / gross_loss) } else if gross_win > 0.0 { Some(f64::INFINITY) } else { None };
    let holds: Vec<f64> = trades.iter().map(Trade::hold_secs).collect();
    let avg_hold = (n > 0).then(|| holds.iter().sum::<f64>() / n as f64);
    let scalps = trades.iter().filter(|t| t.hold_secs() < SCALP_SECS).count();
    let scalping_share = (n > 0).then(|| scalps as f64 / n as f64);
    let mut raw = 0.0;

    if let Some(wr) = win_rate {
        let p = clamp((wr - 0.5) * 30.0, 8.0);
        raw += p;
        add(&mut why, "win_rate", "Win rate", pct(wr), p, format!("{} of {n} trades closed in profit", wins.len()));
    }
    if let Some(pf) = profit_factor {
        let pfc = pf.min(3.0);
        let p = clamp((pfc - 1.0) * 15.0, 15.0);
        raw += p;
        let v = if pf.is_infinite() { "no losses".to_string() } else { format!("{pf:.2}") };
        add(&mut why, "profit_factor", "Profit factor", v, p, format!("Gross profit {} vs gross loss {}", usd(gross_win), usd(gross_loss)));
    }
    if !wins.is_empty() && !losses.is_empty() {
        let avg_w = gross_win / wins.len() as f64;
        let avg_l = gross_loss / losses.len() as f64;
        let r = avg_w / avg_l;
        let p = clamp((r - 1.0) * 5.0, 5.0);
        raw += p;
        add(&mut why, "payoff", "Average win / average loss", format!("{r:.2}"), p, format!("Average win {}, average loss {}", usd(avg_w), usd(avg_l)));
        metrics.insert("avgWinUsd", json!(round(avg_w, 2)));
        metrics.insert("avgLossUsd", json!(round(avg_l, 2)));
    }
    // consistency: Sharpe-like ratio of daily P&L
    let mut daily: BTreeMap<chrono::NaiveDate, f64> = BTreeMap::new();
    for t in trades {
        *daily.entry(t.close_time.date_naive()).or_default() += t.net_usd;
    }
    if daily.len() >= 5 {
        let v: Vec<f64> = daily.values().copied().collect();
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let sd = (v.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (v.len() as f64 - 1.0)).sqrt();
        let sharpe = if sd > 0.0 { mean / sd * (v.len() as f64).sqrt() } else if mean > 0.0 { 5.0 } else { 0.0 };
        let p = clamp(sharpe * 3.0, 10.0);
        raw += p;
        let up = v.iter().filter(|x| **x > 0.0).count();
        add(&mut why, "consistency", "Consistency (daily P&L)", format!("{sharpe:.2}"), p, format!("{up} of {} trading days positive; mean {} / day, spread {}", v.len(), usd(mean), usd(sd)));
        metrics.insert("sharpe", json!(round(sharpe, 2)));
    }
    // result against what the client put in
    if facts.deposits_usd > 0.0 && n > 0 {
        let ret = net / facts.deposits_usd;
        let p = clamp(ret * 25.0, 10.0);
        raw += p;
        add(&mut why, "return", "Net P&L vs deposits", pct(ret), p, format!("Net {} on {} deposited", usd(net), usd(facts.deposits_usd)));
    } else if n > 0 {
        let p = clamp(net / 500.0, 5.0);
        raw += p;
        add(&mut why, "return", "Net P&L", usd(net), p, "No deposits on record for this account".into());
    }
    // trend: the last 14 days against the 76 before
    let cut = now - Duration::days(14);
    let recent: f64 = trades.iter().filter(|t| t.close_time >= cut).map(|t| t.net_usd).sum();
    let earlier: f64 = trades.iter().filter(|t| t.close_time < cut).map(|t| t.net_usd).sum();
    if trades.iter().any(|t| t.close_time >= cut) && trades.iter().any(|t| t.close_time < cut) {
        let p = if recent > 0.0 && recent > earlier / 5.4 { 4.0 } else if recent < 0.0 && recent < earlier / 5.4 { -4.0 } else { 0.0 };
        raw += p;
        add(&mut why, "trend", "P&L trend", format!("{} last 14 days", usd(recent)), p, format!("{} in the 76 days before (per 14 days: {})", usd(earlier), usd(earlier / 76.0 * 14.0)));
    }
    // style
    if let (Some(h), Some(sc)) = (avg_hold, scalping_share) {
        let scalp_net: f64 = trades.iter().filter(|t| t.hold_secs() < SCALP_SECS).map(|t| t.net_usd).sum();
        let p = if sc >= 0.5 && scalp_net > 0.0 { 3.0 } else { 0.0 };
        raw += p;
        add(&mut why, "style", "Holding time", secs(h), p, format!("Average hold {}; {} of trades held under {:.0} s (net {})", secs(h), pct(sc), SCALP_SECS, usd(scalp_net)));
    }
    let lots: f64 = trades.iter().map(|t| t.volume).sum();
    if n > 0 {
        let mut by_sym: BTreeMap<&str, usize> = BTreeMap::new();
        for t in trades {
            *by_sym.entry(t.symbol.as_str()).or_default() += 1;
        }
        let mut top: Vec<(&str, usize)> = by_sym.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1));
        let list = top.iter().take(3).map(|(s, c)| format!("{s} {c}")).collect::<Vec<_>>().join(", ");
        add(&mut why, "instruments", "Instruments and size", format!("{} symbols", top.len()), 0.0, format!("Average {:.2} lots; most traded: {list}", lots / n as f64));
        metrics.insert("avgLots", json!(round(lots / n as f64, 2)));
        metrics.insert("symbols", json!(top.len()));
    }
    match &facts.news {
        Some(times) if n > 0 => {
            let near = trades.iter().filter(|t| times.iter().any(|e| (t.open_time - *e).num_seconds().abs() <= 300)).count();
            let share = near as f64 / n as f64;
            let near_net: f64 = trades.iter().filter(|t| times.iter().any(|e| (t.open_time - *e).num_seconds().abs() <= 300)).map(|t| t.net_usd).sum();
            let p = if share >= 0.3 && near_net > 0.0 { 3.0 } else { 0.0 };
            raw += p;
            add(&mut why, "news", "Trades around news", pct(share), p, format!("{near} trades opened within 5 min of a high-impact release (net {})", usd(near_net)));
            metrics.insert("newsShare", json!(round(share, 3)));
        }
        None if n > 0 => add(&mut why, "news", "Trades around news", "not checked".into(), 0.0, "The news calendar was not available for this run".into()),
        _ => {}
    }

    // toxic-flow signals
    #[allow(clippy::too_many_arguments)]
    fn warn(why: &mut Vec<Why>, flags: &mut Vec<&'static str>, raw: &mut f64, flag: &'static str, label: &str, value: String, detail: String) {
        flags.push(flag);
        why.push(Why { factor: flag.into(), label: label.into(), value, points: 10.0, detail, tone: "warn" });
        *raw += 10.0;
    }
    let quick: Vec<(usize, &Trade)> = trades.iter().enumerate().filter(|(_, t)| t.hold_secs() < ARB_SECS).collect();
    let quick_win = quick.iter().filter(|(_, t)| t.net_usd > 0.0).count();
    if quick.len() >= 10 && quick_win as f64 >= 0.7 * quick.len() as f64 && quick.len() as f64 >= 0.4 * n as f64 {
        match &facts.jumps {
            Some(j) => {
                let checked = quick.iter().filter(|(i, _)| j.contains_key(i)).count();
                let after_jump = quick.iter().filter(|(i, t)| t.net_usd > 0.0 && j.get(i).copied().unwrap_or(false)).count();
                if checked > 0 && after_jump as f64 >= 0.5 * checked as f64 {
                    warn(&mut why, &mut flags, &mut raw, "latency_arbitrage", "Latency arbitrage pattern", format!("{after_jump} of {checked}"), format!("{} trades held under {ARB_SECS:.0} s, {} profitable; {after_jump} of the {checked} checked opened right after a one-minute price jump", quick.len(), quick_win));
                } else {
                    add(&mut why, "short_trades", "Very short profitable trades", format!("{quick_win} of {}", quick.len()), 2.0, format!("Held under {ARB_SECS:.0} s; only {after_jump} of {checked} checked opened after a price jump (no arbitrage pattern)"));
                    raw += 2.0;
                }
            }
            None if quick_win as f64 >= 0.75 * quick.len() as f64 && quick.len() as f64 >= 0.5 * n as f64 => {
                warn(&mut why, &mut flags, &mut raw, "latency_arbitrage", "Latency arbitrage pattern", format!("{quick_win} of {}", quick.len()), format!("Most trades held under {ARB_SECS:.0} s and {} of them profitable (price-jump check unavailable)", pct(quick_win as f64 / quick.len() as f64)));
            }
            None => {}
        }
    }
    let gaps: Vec<&Trade> = trades.iter().filter(|t| gap_trade(specs, t)).collect();
    let gap_wins = gaps.iter().filter(|t| t.net_usd > 0.0).count();
    if gaps.len() >= 5 && gaps.len() as f64 >= 0.5 * n as f64 && gap_wins as f64 >= 0.6 * gaps.len() as f64 {
        let gap_net: f64 = gaps.iter().map(|t| t.net_usd).sum();
        warn(&mut why, &mut flags, &mut raw, "gap_trading", "Trades only on gaps", format!("{} of {n}", gaps.len()), format!("Opened just before a session close and closed after the reopen, or in the first minutes after it; {gap_wins} profitable, net {}", usd(gap_net)));
    }
    if facts.identical_opens >= 5 && facts.opens > 0 && facts.identical_opens as f64 >= 0.3 * facts.opens as f64 {
        warn(&mut why, &mut flags, &mut raw, "copied_trades", "Identical trades on other accounts", format!("{} of {} opens", facts.identical_opens, facts.opens), format!("Same symbol, side and volume within 2 s on {} other account(s): a shared EA or several accounts of one person (copy trading excluded)", facts.identical_logins));
    }
    if facts.bonus_usd > 0.0 {
        let quick_out = facts.withdrawals_after_bonus > 0 && n < 5;
        if quick_out || facts.self_hedges >= 3 {
            let what = if facts.self_hedges >= 3 { format!("{} opposite trades on the client's own accounts within 60 s", facts.self_hedges) } else { format!("{} withdrawal(s) within 7 days of the bonus with {n} trades", facts.withdrawals_after_bonus) };
            warn(&mut why, &mut flags, &mut raw, "bonus_abuse", "Bonus abuse pattern", usd(facts.bonus_usd), format!("Bonus received; {what}"));
        }
    }
    if let Some(c) = facts.account_created {
        let days = (now - c).num_days().max(0);
        add(&mut why, "account", "Account", format!("{days} days old"), 0.0, format!("Deposits {}, withdrawals {}, bonus {}; country {}; IB {}", usd(facts.deposits_usd), usd(facts.withdrawals_usd), usd(facts.bonus_usd), facts.country.clone().unwrap_or_else(|| "unknown".into()).to_uppercase(), facts.ib_source.clone().unwrap_or_else(|| "none / unknown".into())));
        metrics.insert("accountAgeDays", json!(days));
    }

    let mut score = (50.0 + conf * raw).clamp(0.0, 100.0);
    if !flags.is_empty() {
        score = score.max(75.0);
    }
    let category = if !flags.is_empty() {
        "toxic"
    } else if score >= 65.0 {
        "likely_profitable"
    } else if score <= 35.0 {
        "likely_losing"
    } else {
        "neutral"
    };
    metrics.insert("grossWinUsd", json!(round(gross_win, 2)));
    metrics.insert("grossLossUsd", json!(round(gross_loss, 2)));
    metrics.insert("lots", json!(round(lots, 2)));
    metrics.insert("gapTrades", json!(gaps.len()));
    metrics.insert("shortTrades", json!(quick.len()));
    Outcome {
        score: round(score, 1),
        category,
        trades: n,
        win_rate: win_rate.map(|w| round(w * 100.0, 1)),
        profit_factor: profit_factor.map(|p| if p.is_infinite() { 99.0 } else { round(p, 2) }),
        net_pnl_usd: round(net, 2),
        avg_hold_secs: avg_hold.map(|h| round(h, 1)),
        scalping_share: scalping_share.map(|s| round(s * 100.0, 1)),
        toxic_flags: flags,
        why,
        metrics,
    }
}

/* ------------------------------------------------------------------ */
/* Job                                                                 */
/* ------------------------------------------------------------------ */

/// One account to score.
#[derive(Clone, Debug)]
struct Acc {
    login: i64,
    user_id: i64,
    group: String,
    kind: String,
    created_at: DateTime<Utc>,
}

/// The view of a stored score (`client_scores.data`, the API shape).
pub fn score_view(a_login: i64, user_id: i64, group: &str, kind: &str, o: &Outcome, f: &Facts, at: DateTime<Utc>) -> Value {
    json!({
        "login": a_login.to_string(), "userId": user_id.to_string(), "group": group, "kind": kind,
        "score": o.score, "category": o.category, "trades": o.trades, "computedAt": at,
        "winRate": o.win_rate, "profitFactor": o.profit_factor, "netPnlUsd": o.net_pnl_usd, "avgHoldSecs": o.avg_hold_secs,
        "scalpingShare": o.scalping_share, "toxicFlags": o.toxic_flags, "why": o.why, "metrics": o.metrics,
        "facts": {
            "depositsUsd": (f.deposits_usd * 100.0).round() / 100.0, "withdrawalsUsd": (f.withdrawals_usd * 100.0).round() / 100.0,
            "accountAgeDays": f.account_created.map(|c| (at - c).num_days().max(0)).unwrap_or(0),
            "country": f.country, "ibSource": f.ib_source, "bonusUsd": (f.bonus_usd * 100.0).round() / 100.0,
        },
    })
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().timeout(std::time::Duration::from_secs(8)).build().unwrap_or_default()
}

/// Gateway client facts (country, IB source), cached for an hour; None when the gateway is not reachable.
async fn client_facts(st: &AppState, cache: &mut HashMap<i64, (std::time::Instant, Option<String>, Option<String>)>, slug: &str, user_id: i64) -> (Option<String>, Option<String>) {
    if let Some((at, c, ib)) = cache.get(&user_id)
        && at.elapsed() < std::time::Duration::from_secs(3600)
    {
        return (c.clone(), ib.clone());
    }
    let url = st.cfg.gateway_url.trim_end_matches('/');
    if url.is_empty() || user_id <= 0 {
        return (None, None);
    }
    let base = if url.starts_with("http") { url.to_string() } else { format!("http://{url}") };
    let r = http().get(format!("{base}/v1/internal/users/{user_id}")).header("x-kalks-internal", &st.cfg.gateway_token).header("x-kalks-tenant", slug).header("x-kalks-service", "trading").send().await;
    let (c, ib) = match r {
        Ok(res) if res.status().is_success() => {
            let v: Value = res.json().await.unwrap_or(Value::Null);
            let u = &v["user"];
            let c = u["country"].as_str().map(|s| s.trim().to_lowercase()).filter(|s| s.len() == 2);
            let ib = ["referred_by", "referredBy", "referral_source", "ib"].iter().find_map(|k| match &u[*k] {
                Value::String(s) if !s.is_empty() => Some(s.clone()),
                Value::Number(n) => Some(format!("referred by client {n}")),
                _ => None,
            });
            (c, ib)
        }
        _ => return (None, None),
    };
    cache.insert(user_id, (std::time::Instant::now(), c.clone(), ib.clone()));
    (c, ib)
}

/// High-impact news times of the window from the news service (None when it is not reachable).
async fn news_times(from: DateTime<Utc>, to: DateTime<Utc>) -> Option<Vec<DateTime<Utc>>> {
    let v = super::news::fetch(from, to, "3").await.ok()?;
    Some(super::news::parse_calendar(&v).into_iter().map(|e| e.starts_at).collect())
}

/// Was the one-minute bar of `at` (or the one before) a price jump: a range ≥ 3 × the median of the 30 bars before?
async fn jump_at(md: &str, cache: &mut HashMap<(String, i64), Option<bool>>, symbol: &str, at: DateTime<Utc>) -> Option<bool> {
    let minute = at.timestamp() / 60 * 60;
    if let Some(v) = cache.get(&(symbol.to_string(), minute)) {
        return *v;
    }
    let url = format!("{md}/v1/candles?symbol={symbol}&tf=M1&limit=32&to={minute}");
    let res = http().get(url).send().await.ok()?;
    let v: Value = res.json().await.ok()?;
    let bars = v["bars"].as_array()?;
    let ranges: Vec<(i64, f64)> = bars.iter().filter_map(|b| Some((b["t"].as_i64()?, b["h"].as_f64()? - b["l"].as_f64()?))).collect();
    let out = (|| {
        let (recent, before): (Vec<_>, Vec<_>) = ranges.iter().partition(|(t, _)| *t >= minute - 60);
        let mut base: Vec<f64> = before.iter().map(|(_, r)| *r).filter(|r| *r > 0.0).collect();
        if base.len() < 10 || recent.is_empty() {
            return None;
        }
        base.sort_by(|a, b| a.total_cmp(b));
        let median = base[base.len() / 2];
        Some(recent.iter().any(|(_, r)| *r >= 3.0 * median))
    })();
    cache.insert((symbol.to_string(), minute), out);
    out
}

/// Loads the latest stored score of every account into `RiskCtx` (routing conditions work right after a restart).
pub async fn load_latest(st: &AppState) -> anyhow::Result<usize> {
    let rows = sqlx::query("SELECT DISTINCT ON (login) login, user_id, score, category, data, computed_at FROM client_scores ORDER BY login, computed_at DESC, id DESC").fetch_all(&st.pool).await?;
    let n = rows.len();
    st.hub.shared.risk.set_scores(rows.iter().map(|r| (r.get::<i64, _>("login"), lite_of(r))));
    Ok(n)
}

fn lite_of(r: &sqlx::postgres::PgRow) -> ScoreLite {
    let data: sqlx::types::Json<Value> = r.get("data");
    let d = data.0;
    ScoreLite {
        user_id: r.get("user_id"),
        score: r.get::<D, _>("score").to_string().parse().unwrap_or(50.0),
        category: r.get("category"),
        win_rate: d["winRate"].as_f64(),
        avg_hold_secs: d["avgHoldSecs"].as_f64(),
        country: d["facts"]["country"].as_str().map(str::to_string),
        computed_at: r.get("computed_at"),
    }
}

/// Scores the tenant's live accounts with trading in the window (or just `only`), stores and publishes them.
/// Returns how many accounts were scored.
pub async fn score_tenant(st: &AppState, tenant_id: i64, only: Option<i64>, trigger: &str) -> anyhow::Result<usize> {
    let Some(cfg) = st.hub.shared.registry.get(tenant_id) else { return Ok(0) };
    let now = st.hub.shared.clock.now();
    let from = now - Duration::days(WINDOW_DAYS);
    let pool = &st.pool;
    let accounts: Vec<Acc> = sqlx::query(
        "SELECT a.login, a.user_id, a.group_code, a.kind, a.created_at FROM accounts a
         WHERE a.tenant_id = $1 AND ($2::bigint IS NOT NULL AND a.login = $2
            OR $2::bigint IS NULL AND a.kind = 'live' AND EXISTS (SELECT 1 FROM deals d WHERE d.login = a.login AND d.time >= $3))",
    )
    .bind(tenant_id)
    .bind(only)
    .bind(from)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|r| Acc { login: r.get("login"), user_id: r.get("user_id"), group: r.get("group_code"), kind: r.get("kind"), created_at: r.get("created_at") })
    .collect();
    if accounts.is_empty() {
        return Ok(0);
    }
    let logins: Vec<i64> = accounts.iter().map(|a| a.login).collect();
    // closed trades (exit deals), USD
    let mut trades: HashMap<i64, Vec<Trade>> = HashMap::new();
    for r in sqlx::query(
        "SELECT d.login, d.symbol, d.position_side, d.volume, d.open_time, d.time, d.profit, d.swap, d.commission, a.cent,
                COALESCE(d.data->'snapshot'->>'source', d.source) AS open_source
           FROM deals d JOIN accounts a ON a.login = d.login
          WHERE d.tenant_id = $1 AND d.login = ANY($2) AND d.entry <> 'in' AND NOT d.reversed AND d.time >= $3 AND d.data->'option' IS NULL
          ORDER BY d.time",
    )
    .bind(tenant_id)
    .bind(&logins)
    .bind(from)
    .fetch_all(pool)
    .await?
    {
        let f = |c: &str| r.get::<D, _>(c).to_string().parse::<f64>().unwrap_or(0.0);
        let div = if r.get::<bool, _>("cent") { 100.0 } else { 1.0 };
        trades.entry(r.get("login")).or_default().push(Trade {
            symbol: r.get("symbol"),
            side: r.get("position_side"),
            volume: f("volume"),
            open_time: r.get("open_time"),
            close_time: r.get("time"),
            net_usd: (f("profit") + f("swap") - f("commission")) / div,
            open_source: r.get("open_source"),
        });
    }
    // own opens, identical opens elsewhere (copy / PAMM / MAM / dealer / system excluded on both sides)
    const OWN: &str = "('copy','pamm','mam','dealer','system')";
    let mut opens: HashMap<i64, usize> = HashMap::new();
    for r in sqlx::query(sqlx::AssertSqlSafe(format!("SELECT login, count(*) AS n FROM deals WHERE tenant_id = $1 AND login = ANY($2) AND entry = 'in' AND time >= $3 AND source NOT IN {OWN} AND data->'option' IS NULL GROUP BY login")))
        .bind(tenant_id)
        .bind(&logins)
        .bind(from)
        .fetch_all(pool)
        .await?
    {
        opens.insert(r.get("login"), r.get::<i64, _>("n") as usize);
    }
    let mut identical: HashMap<i64, (usize, usize)> = HashMap::new();
    for r in sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT a.login, count(DISTINCT a.id) AS n, count(DISTINCT b.login) AS others
           FROM deals a JOIN deals b ON b.tenant_id = a.tenant_id AND b.symbol = a.symbol AND b.side = a.side AND b.volume = a.volume
                AND b.login <> a.login AND b.entry = 'in' AND b.time BETWEEN a.time - interval '2 seconds' AND a.time + interval '2 seconds'
                AND b.source NOT IN {OWN}
          WHERE a.tenant_id = $1 AND a.login = ANY($2) AND a.entry = 'in' AND a.time >= $3 AND a.source NOT IN {OWN}
          GROUP BY a.login"
    )))
    .bind(tenant_id)
    .bind(&logins)
    .bind(from)
    .fetch_all(pool)
    .await?
    {
        identical.insert(r.get("login"), (r.get::<i64, _>("n") as usize, r.get::<i64, _>("others") as usize));
    }
    let mut hedges: HashMap<i64, usize> = HashMap::new();
    for r in sqlx::query(
        "SELECT a.login, count(DISTINCT a.id) AS n
           FROM deals a JOIN accounts aa ON aa.login = a.login
           JOIN deals b ON b.tenant_id = a.tenant_id AND b.symbol = a.symbol AND b.side <> a.side AND b.entry = 'in' AND b.login <> a.login
                AND b.time BETWEEN a.time - interval '60 seconds' AND a.time + interval '60 seconds'
           JOIN accounts bb ON bb.login = b.login AND bb.user_id = aa.user_id
          WHERE a.tenant_id = $1 AND a.login = ANY($2) AND a.entry = 'in' AND a.time >= $3
          GROUP BY a.login",
    )
    .bind(tenant_id)
    .bind(&logins)
    .bind(from)
    .fetch_all(pool)
    .await?
    {
        hedges.insert(r.get("login"), r.get::<i64, _>("n") as usize);
    }
    // money facts from the ledger (USD; cent accounts / 100)
    let mut money: HashMap<i64, Facts> = HashMap::new();
    for r in sqlx::query(
        "SELECT t.login, t.kind, min(t.created_at) AS first_at, sum(p.amount / CASE WHEN p.currency = 'USC' THEN 100 ELSE 1 END) AS amount
           FROM ledger_txns t JOIN ledger_postings p ON p.txn_id = t.id AND p.account_code = 'acct:' || t.login || ':' || CASE WHEN t.kind = 'bonus' THEN 'bonus' ELSE 'balance' END
          WHERE t.tenant_id = $1 AND t.login = ANY($2) AND t.kind IN ('transfer_in', 'deposit', 'transfer_out', 'withdrawal', 'bonus')
          GROUP BY t.login, t.kind",
    )
    .bind(tenant_id)
    .bind(&logins)
    .fetch_all(pool)
    .await?
    {
        let f = money.entry(r.get("login")).or_default();
        let amt: f64 = r.get::<Option<D>, _>("amount").unwrap_or_default().to_string().parse().unwrap_or(0.0);
        match r.get::<String, _>("kind").as_str() {
            "transfer_in" | "deposit" => f.deposits_usd += amt,
            "transfer_out" | "withdrawal" => f.withdrawals_usd += -amt,
            "bonus" => {
                if amt > 0.0 {
                    f.bonus_usd += amt;
                }
                f.first_bonus_at = r.get("first_at");
            }
            _ => {}
        }
    }
    for (login, f) in money.iter_mut().filter(|(_, f)| f.first_bonus_at.is_some()) {
        let at = f.first_bonus_at.unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM ledger_txns WHERE tenant_id = $1 AND login = $2 AND kind IN ('transfer_out', 'withdrawal') AND created_at >= $3 AND created_at < $3 + interval '7 days'")
            .bind(tenant_id)
            .bind(*login)
            .bind(at)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
        f.withdrawals_after_bonus = n as usize;
    }
    let news = news_times(from, now).await;
    let md = st.cfg.market_data_url.trim_end_matches('/').to_string();
    let specs = st.hub.shared.specs.load();
    let mut gw_cache = HashMap::new();
    let mut jump_cache = HashMap::new();
    let mut out = 0;
    for a in &accounts {
        let ts = trades.remove(&a.login).unwrap_or_default();
        let mut f = money.remove(&a.login).unwrap_or_default();
        f.account_created = Some(a.created_at);
        f.opens = opens.get(&a.login).copied().unwrap_or(0);
        let (ido, idl) = identical.get(&a.login).copied().unwrap_or((0, 0));
        f.identical_opens = ido;
        f.identical_logins = idl;
        f.self_hedges = hedges.get(&a.login).copied().unwrap_or(0);
        f.news = news.clone();
        let (country, ib) = client_facts(st, &mut gw_cache, &cfg.slug, a.user_id).await;
        f.country = country;
        f.ib_source = ib;
        // the price-jump check only for accounts with many very short profitable trades (≤ 30 candle requests each)
        let quick: Vec<usize> = ts.iter().enumerate().filter(|(_, t)| t.hold_secs() < ARB_SECS && t.net_usd > 0.0).map(|(i, _)| i).collect();
        if quick.len() >= 7 && !md.is_empty() {
            let mut j = HashMap::new();
            for i in quick.iter().take(30) {
                if let Some(v) = jump_at(&md, &mut jump_cache, &ts[*i].symbol, ts[*i].open_time).await {
                    j.insert(*i, v);
                }
            }
            if !j.is_empty() {
                f.jumps = Some(j);
            }
        }
        let o = compute(&ts, &f, &specs, now);
        let view = score_view(a.login, a.user_id, &a.group, &a.kind, &o, &f, now);
        let score_d = crate::money::from_f64(o.score).unwrap_or(D::from(50));
        sqlx::query("INSERT INTO client_scores (tenant_id, login, user_id, computed_at, score, category, trades, data, trigger) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(tenant_id)
            .bind(a.login)
            .bind(a.user_id)
            .bind(now)
            .bind(score_d)
            .bind(o.category)
            .bind(o.trades as i32)
            .bind(sqlx::types::Json(&view))
            .bind(trigger)
            .execute(pool)
            .await?;
        let prev = st.hub.shared.risk.score(a.login);
        let lite = ScoreLite { user_id: a.user_id, score: o.score, category: o.category.to_string(), win_rate: o.win_rate, avg_hold_secs: o.avg_hold_secs, country: f.country.clone(), computed_at: now };
        st.hub.shared.risk.set_scores([(a.login, lite)]);
        if o.category == "toxic" && prev.is_none_or(|p| p.category != "toxic") {
            let flags = o.toxic_flags.join(", ").replace('_', " ");
            super::alerts::raise(
                st,
                tenant_id,
                super::alerts::AlertSpec {
                    key: format!("toxic:{}", a.login),
                    kind: "toxic_client",
                    level: super::alerts::Level::Amber,
                    title: format!("Toxic flow on {}", a.login),
                    message: format!("Account {} scored {:.0}/100: {flags}", a.login, o.score),
                    symbol: None,
                    login: Some(a.login),
                    data: json!({"score": o.score, "flags": o.toxic_flags}),
                },
            )
            .await;
        }
        out += 1;
    }
    Ok(out)
}

/// The scheduled and on-demand score job.
pub async fn run(st: AppState) {
    match load_latest(&st).await {
        Ok(n) => tracing::info!(accounts = n, "client scores loaded"),
        Err(e) => tracing::warn!(error = %e, "client scores could not be loaded"),
    }
    let risk = st.hub.shared.risk.clone();
    let mut last_run: HashMap<i64, DateTime<Utc>> = HashMap::new();
    if let Ok(rows) = sqlx::query("SELECT tenant_id, max(computed_at) AS at FROM client_scores WHERE trigger = 'schedule' GROUP BY tenant_id").fetch_all(&st.pool).await {
        for r in rows {
            last_run.insert(r.get("tenant_id"), r.get("at"));
        }
    }
    // the first scheduled run waits for the engine to settle
    let mut warm = true;
    loop {
        tokio::select! {
            _ = risk.score_wake.notified() => {}
            _ = tokio::time::sleep(std::time::Duration::from_secs(if warm { 90 } else { 60 })) => {}
        }
        warm = false;
        let mut reqs: Vec<(i64, Option<i64>)> = std::mem::take(&mut *risk.score_requests.lock().unwrap());
        let now = Utc::now();
        for t in st.hub.shared.registry.all() {
            let every = Duration::minutes(t.risk.settings.score_interval_mins.max(15) as i64);
            if last_run.get(&t.tenant_id).is_none_or(|at| now - *at >= every) {
                reqs.push((t.tenant_id, None));
                last_run.insert(t.tenant_id, now);
            }
        }
        let mut seen = HashSet::new();
        reqs.retain(|r| seen.insert(*r));
        if reqs.is_empty() {
            continue;
        }
        risk.score_running.store(true, Ordering::SeqCst);
        for (tenant, login) in reqs {
            let trigger = if login.is_some() { "manual" } else { "schedule" };
            match score_tenant(&st, tenant, login, trigger).await {
                Ok(n) => tracing::info!(tenant, ?login, accounts = n, "client scores computed"),
                Err(e) => tracing::error!(tenant, ?login, error = %e, "client score run failed"),
            }
        }
        risk.score_running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn trade(open: DateTime<Utc>, hold_secs: i64, net: f64) -> Trade {
        Trade { symbol: "EURUSD".into(), side: "buy".into(), volume: 1.0, open_time: open, close_time: open + Duration::seconds(hold_secs), net_usd: net, open_source: "manual".into() }
    }

    fn specs() -> Specs {
        crate::specs::test_specs()
    }

    fn facts() -> Facts {
        Facts { deposits_usd: 10_000.0, account_created: Some(t("2026-06-01T00:00:00Z")), opens: 0, ..Default::default() }
    }

    /// Weekday trading hours in September 2026 (FX open), one trade per hour slot.
    fn slots(n: usize) -> Vec<DateTime<Utc>> {
        let start = t("2026-09-01T08:00:00Z");
        let mut v = Vec::new();
        let mut d = 0;
        while v.len() < n {
            let day = start + Duration::days(d);
            d += 1;
            if matches!(chrono::Datelike::weekday(&day), chrono::Weekday::Sat | chrono::Weekday::Sun) {
                continue;
            }
            for h in 0..6 {
                if v.len() < n {
                    v.push(day + Duration::hours(h));
                }
            }
        }
        v
    }

    #[test]
    fn a_steady_winner_is_likely_profitable_with_the_numbers_behind_it() {
        let now = t("2026-10-01T12:00:00Z");
        let ts: Vec<Trade> = slots(60).into_iter().enumerate().map(|(i, at)| trade(at, 3600, if i % 4 == 3 { -40.0 } else { 90.0 })).collect();
        let o = compute(&ts, &facts(), &specs(), now);
        assert_eq!(o.category, "likely_profitable", "{o:?}");
        assert!(o.score >= 65.0, "{}", o.score);
        assert_eq!(o.trades, 60);
        assert_eq!(o.win_rate, Some(75.0));
        // gross win 45 × 90 = 4050, gross loss 15 × 40 = 600
        assert_eq!(o.profit_factor, Some(6.75));
        assert_eq!(o.net_pnl_usd, 3450.0);
        let wr = o.why.iter().find(|w| w.factor == "win_rate").unwrap();
        assert_eq!(wr.value, "75%");
        assert!(wr.points > 0.0 && wr.tone == "up");
        let pf = o.why.iter().find(|w| w.factor == "profit_factor").unwrap();
        assert!(pf.detail.contains("4050.00 USD") && pf.detail.contains("600.00 USD"), "{}", pf.detail);
        assert!(o.toxic_flags.is_empty());
    }

    #[test]
    fn a_steady_loser_is_likely_losing_and_few_trades_stay_near_neutral() {
        let now = t("2026-10-01T12:00:00Z");
        let ts: Vec<Trade> = slots(60).into_iter().enumerate().map(|(i, at)| trade(at, 5400, if i % 3 == 0 { 50.0 } else { -80.0 })).collect();
        let o = compute(&ts, &facts(), &specs(), now);
        assert_eq!(o.category, "likely_losing", "{o:?}");
        assert!(o.score <= 35.0);
        assert!(o.why.iter().find(|w| w.factor == "profit_factor").unwrap().tone == "down");
        // three winning trades: little weight, still neutral
        let few: Vec<Trade> = slots(3).into_iter().map(|at| trade(at, 3600, 100.0)).collect();
        let o = compute(&few, &facts(), &specs(), now);
        assert_eq!(o.category, "neutral", "{}", o.score);
        // no trades at all
        let o = compute(&[], &facts(), &specs(), now);
        assert_eq!((o.score, o.category, o.trades), (50.0, "neutral", 0));
    }

    #[test]
    fn latency_arbitrage_needs_short_profitable_trades_after_price_jumps() {
        let now = t("2026-10-01T12:00:00Z");
        let ts: Vec<Trade> = slots(30).into_iter().enumerate().map(|(i, at)| trade(at, 8, if i % 10 == 0 { -5.0 } else { 12.0 })).collect();
        // jumps confirmed for most of them
        let mut f = facts();
        f.jumps = Some((0..30).map(|i| (i, i % 5 != 0)).collect());
        let o = compute(&ts, &f, &specs(), now);
        assert_eq!(o.category, "toxic");
        assert_eq!(o.toxic_flags, vec!["latency_arbitrage"]);
        assert!(o.score >= 75.0);
        assert!(o.why.iter().any(|w| w.factor == "latency_arbitrage" && w.tone == "warn" && w.detail.contains("price jump")));
        // the same trades with no jumps behind them: a short-term trader, not arbitrage
        f.jumps = Some((0..30).map(|i| (i, false)).collect());
        let o = compute(&ts, &f, &specs(), now);
        assert!(o.toxic_flags.is_empty(), "{:?}", o.toxic_flags);
        assert!(o.why.iter().any(|w| w.factor == "short_trades"));
        // without the price-jump check the pattern alone flags it (and says the check was unavailable)
        f.jumps = None;
        let o = compute(&ts, &f, &specs(), now);
        assert_eq!(o.toxic_flags, vec!["latency_arbitrage"]);
        assert!(o.why.iter().any(|w| w.detail.contains("unavailable")));
    }

    #[test]
    fn gap_trading_copied_trades_and_bonus_abuse_are_flagged() {
        let now = t("2026-10-01T12:00:00Z");
        // EURUSD opens Friday 20:50 UTC (summer: the week closes 21:00 UTC), closes Sunday after the reopen (21:05 UTC)
        let fridays = ["2026-09-04", "2026-09-11", "2026-09-18", "2026-09-25", "2026-08-28", "2026-08-21"];
        let ts: Vec<Trade> = fridays
            .iter()
            .map(|d| {
                let open = t(&format!("{d}T20:50:00Z"));
                Trade { close_time: open + Duration::hours(48) + Duration::minutes(30), ..trade(open, 0, 150.0) }
            })
            .collect();
        assert!(ts.iter().all(|x| gap_trade(&specs(), x)), "fixture trades are gap trades");
        let o = compute(&ts, &facts(), &specs(), now);
        assert!(o.toxic_flags.contains(&"gap_trading"), "{:?}", o.why);
        // identical opens on other accounts (copy trading excluded by the job's query)
        let normal: Vec<Trade> = slots(20).into_iter().map(|at| trade(at, 3600, 10.0)).collect();
        let f = Facts { opens: 20, identical_opens: 12, identical_logins: 2, ..facts() };
        let o = compute(&normal, &f, &specs(), now);
        assert_eq!(o.toxic_flags, vec!["copied_trades"]);
        assert!(o.why.iter().any(|w| w.factor == "copied_trades" && w.value == "12 of 20 opens"));
        let f = Facts { opens: 20, identical_opens: 3, identical_logins: 1, ..facts() };
        assert!(compute(&normal, &f, &specs(), now).toxic_flags.is_empty());
        // bonus taken, withdrawn within a week, two trades
        let f = Facts { bonus_usd: 500.0, first_bonus_at: Some(t("2026-09-10T10:00:00Z")), withdrawals_after_bonus: 1, ..facts() };
        let o = compute(&normal[..2], &f, &specs(), now);
        assert_eq!(o.toxic_flags, vec!["bonus_abuse"]);
        // bonus with hedges across the client's own accounts
        let f = Facts { bonus_usd: 500.0, self_hedges: 4, ..facts() };
        assert_eq!(compute(&normal, &f, &specs(), now).toxic_flags, vec!["bonus_abuse"]);
        // a bonus alone is not abuse
        let f = Facts { bonus_usd: 500.0, ..facts() };
        assert!(compute(&normal, &f, &specs(), now).toxic_flags.is_empty());
    }

    #[test]
    fn style_news_and_trend_factors() {
        let now = t("2026-10-01T12:00:00Z");
        let at = slots(20);
        let ts: Vec<Trade> = at.iter().map(|a| trade(*a, 20, 15.0)).collect();
        let mut f = facts();
        f.news = Some(at.iter().take(10).map(|a| *a + Duration::minutes(2)).collect());
        let o = compute(&ts, &f, &specs(), now);
        let news = o.why.iter().find(|w| w.factor == "news").unwrap();
        assert_eq!(news.value, "50%");
        assert_eq!(o.scalping_share, Some(100.0));
        assert_eq!(o.avg_hold_secs, Some(20.0));
        assert!(o.why.iter().any(|w| w.factor == "style" && w.value == "20 s"));
        f.news = None;
        let o = compute(&ts, &f, &specs(), now);
        assert_eq!(o.why.iter().find(|w| w.factor == "news").unwrap().value, "not checked");
    }
}

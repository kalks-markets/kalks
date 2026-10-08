//! Client analytics (D91) and the monthly statement list, for the Client Area.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde_json::{Value, json};
use sqlx::Row;

use crate::error::{ApiError, ApiResult};
use crate::metrics::{self, Trade, round2};
use crate::state::App;
use crate::statement::{self, DealRow};
use crate::sync;
use crate::time;
use crate::upstream::{As, Target, num};

#[derive(Clone, Debug)]
pub struct Acct {
    pub login: i64,
    pub kind: String,
    pub group: String,
    pub group_name: String,
    /// `cfd` | `options` (CFD / Options account split)
    pub product: String,
    pub cent: bool,
    pub currency: String,
    pub equity: f64,
    pub balance: f64,
    pub created_at: DateTime<Utc>,
}

impl Acct {
    pub fn usd(&self) -> f64 {
        if self.cent { 0.01 } else { 1.0 }
    }
}

pub async fn user_accounts(app: &App, tenant: &str, user_id: i64) -> ApiResult<Vec<Acct>> {
    let rows = sqlx::query("SELECT login, kind, group_code, group_name, product, cent, currency, equity, balance, created_at FROM accounts WHERE tenant = $1 AND user_id = $2 ORDER BY kind, login")
        .bind(tenant)
        .bind(user_id)
        .fetch_all(&app.pool)
        .await?;
    Ok(rows
        .iter()
        .map(|r| Acct {
            login: r.get("login"),
            kind: r.get("kind"),
            group: r.get("group_code"),
            group_name: r.get("group_name"),
            product: r.get("product"),
            cent: r.get("cent"),
            currency: r.get("currency"),
            equity: r.get::<Decimal, _>("equity").to_f64().unwrap_or(0.0),
            balance: r.get::<Decimal, _>("balance").to_f64().unwrap_or(0.0),
            created_at: r.get("created_at"),
        })
        .collect())
}

/// Refreshes the user's accounts from the engine (best effort: stale data beats an error page).
pub async fn refresh(app: &App, tenant: &str, user_id: i64) {
    if let Err(e) = sync::refresh_user(app, tenant, user_id).await {
        tracing::warn!(error = %e, user_id, "on-demand refresh failed; serving mirrored data");
    }
}

/// Resolves `login=all|<login>`: all live accounts, or the one account (which must be the user's).
pub fn scope(accts: &[Acct], login: Option<i64>) -> ApiResult<Vec<Acct>> {
    match login {
        Some(l) => accts.iter().find(|a| a.login == l).cloned().map(|a| vec![a]).ok_or_else(|| ApiError::NotFound("Account not found.".into())),
        None => Ok(accts.iter().filter(|a| a.kind == "live").cloned().collect()),
    }
}

/// Balance after each posting on the balance sub-ledger, per login (for "balance before the trade").
async fn balance_paths(app: &App, tenant: &str, logins: &[i64]) -> ApiResult<HashMap<i64, Vec<(DateTime<Utc>, f64)>>> {
    let rows = sqlx::query("SELECT login, at, sum(amount) OVER (PARTITION BY login ORDER BY at, txn) AS bal FROM ledger WHERE tenant = $1 AND login = ANY($2) AND sub_ledger = 'balance' ORDER BY login, at, txn")
        .bind(tenant)
        .bind(logins)
        .fetch_all(&app.pool)
        .await?;
    let mut m: HashMap<i64, Vec<(DateTime<Utc>, f64)>> = HashMap::new();
    for r in rows {
        m.entry(r.get("login")).or_default().push((r.get("at"), r.get::<Decimal, _>("bal").to_f64().unwrap_or(0.0)));
    }
    Ok(m)
}

fn before(path: &[(DateTime<Utc>, f64)], t: DateTime<Utc>) -> f64 {
    // postings stamped at or after the deal (its own trade result) are excluded
    let i = path.partition_point(|(at, _)| *at < t - chrono::Duration::milliseconds(500));
    if i == 0 { 0.0 } else { path[i - 1].1 }
}

pub async fn trades_for(app: &App, tenant: &str, accts: &[Acct], from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> ApiResult<Vec<Trade>> {
    let logins: Vec<i64> = accts.iter().map(|a| a.login).collect();
    let paths = balance_paths(app, tenant, &logins).await?;
    let mut out = vec![];
    for a in accts {
        let deals: Vec<DealRow> = statement::load_deals(app, tenant, a.login, from, to).await?;
        let p = paths.get(&a.login).cloned().unwrap_or_default();
        out.extend(statement::to_trades(a.login, &deals, a.cent, |t| before(&p, t)));
    }
    out.sort_by_key(|t| (t.close_time, t.deal));
    Ok(out)
}

/// Daily curve rows (USD) summed over the accounts, gaps carried forward.
pub async fn curve_rows(app: &App, tenant: &str, accts: &[Acct], from: NaiveDate, to: NaiveDate) -> ApiResult<Vec<(NaiveDate, f64, f64, f64)>> {
    let mut total: BTreeMap<NaiveDate, (f64, f64, f64)> = BTreeMap::new();
    for a in accts {
        let rows = sqlx::query("SELECT day, balance, equity, flow FROM snapshots WHERE tenant = $1 AND login = $2 AND day <= $3 ORDER BY day")
            .bind(tenant)
            .bind(a.login)
            .bind(to)
            .fetch_all(&app.pool)
            .await?;
        let mut m = BTreeMap::new();
        for r in rows {
            let k = a.usd();
            m.insert(
                r.get::<NaiveDate, _>("day"),
                (r.get::<Decimal, _>("balance").to_f64().unwrap_or(0.0) * k, r.get::<Decimal, _>("equity").to_f64().unwrap_or(0.0) * k, r.get::<Decimal, _>("flow").to_f64().unwrap_or(0.0) * k),
            );
        }
        let start = from.max(time::server_day(a.created_at));
        if start > to {
            continue;
        }
        for (d, b, e, f) in metrics::fill_days(&m, start, to) {
            let x = total.entry(d).or_insert((0.0, 0.0, 0.0));
            x.0 += b;
            x.1 += e;
            x.2 += f;
        }
    }
    Ok(total.into_iter().map(|(d, (b, e, f))| (d, round2(b), round2(e), round2(f))).collect())
}

fn group_json(g: &[metrics::Group]) -> Value {
    json!(g.iter().map(|x| json!({"key": x.key, "trades": x.trades, "wins": x.wins, "winRate": round2(x.win_rate), "net": round2(x.net), "lots": round2(x.lots)})).collect::<Vec<_>>())
}

pub async fn analytics(app: &App, tenant: &str, user_id: i64, login: Option<i64>, from: DateTime<Utc>, to: DateTime<Utc>) -> ApiResult<Value> {
    refresh(app, tenant, user_id).await;
    let accts = user_accounts(app, tenant, user_id).await?;
    let sc = scope(&accts, login)?;
    let logins: Vec<i64> = sc.iter().map(|a| a.login).collect();

    // curves
    let to_day = time::server_day(to - chrono::Duration::seconds(1)).min(time::server_day(Utc::now()));
    let from_day = time::server_day(from);
    let rows = curve_rows(app, tenant, &sc, from_day, to_day).await?;
    let (points, cstats) = metrics::curve(&rows);

    // trades
    let trades = trades_for(app, tenant, &sc, Some(from), Some(to)).await?;
    let s = metrics::trade_stats(&trades);
    let by_symbol = {
        let mut g = metrics::group_by(&trades, |t| t.symbol.clone());
        g.sort_by(|a, b| b.net.total_cmp(&a.net));
        g
    };
    let by_weekday = metrics::group_by(&trades, |t| time::server_naive(t.close_time).weekday().num_days_from_monday().to_string());
    let by_session = metrics::group_by(&trades, |t| metrics::session_of(t.open_time).to_string());
    let longs: Vec<Trade> = trades.iter().filter(|t| t.side == "buy").cloned().collect();
    let shorts: Vec<Trade> = trades.iter().filter(|t| t.side != "buy").cloned().collect();
    let (ls, ss) = (metrics::trade_stats(&longs), metrics::trade_stats(&shorts));
    let behaviour = metrics::behaviour(&trades);

    // money flow + charges (USD), from the ledger in the period
    let mut flow: BTreeMap<&str, f64> = BTreeMap::new();
    let fl = sqlx::query("SELECT login, kind, sub_ledger, sum(amount) AS amt FROM ledger WHERE tenant = $1 AND login = ANY($2) AND at >= $3 AND at < $4 GROUP BY 1, 2, 3")
        .bind(tenant)
        .bind(&logins)
        .bind(from)
        .bind(to)
        .fetch_all(&app.pool)
        .await?;
    for r in fl {
        let login: i64 = r.get("login");
        let k = sc.iter().find(|a| a.login == login).map(Acct::usd).unwrap_or(1.0);
        let amt = r.get::<Decimal, _>("amt").to_f64().unwrap_or(0.0) * k;
        let kind: String = r.get("kind");
        let sub: String = r.get("sub_ledger");
        let key = if sub != "balance" {
            "bonus"
        } else {
            flow_key(&kind)
        };
        *flow.entry(key).or_default() += amt;
    }
    let spread: f64 = {
        let mut x = 0.0;
        for a in &sc {
            let sg = app.specs.spread_group(&a.group);
            for d in statement::load_deals(app, tenant, a.login, Some(from), Some(to)).await? {
                if !d.reversed {
                    x += app.specs.deal_spread_cost_usd(&sg, &d.symbol, d.volume.to_f64().unwrap_or(0.0), d.price.to_f64().unwrap_or(0.0));
                }
            }
        }
        x
    };
    let wallet_fees: Decimal = if login.is_none() {
        sqlx::query_scalar("SELECT COALESCE(sum(fee), 0) FROM wallet_withdrawals WHERE tenant = $1 AND user_id = $2 AND status = 'completed' AND completed_at >= $3 AND completed_at < $4")
            .bind(tenant)
            .bind(user_id)
            .bind(from)
            .bind(to)
            .fetch_one(&app.pool)
            .await?
    } else {
        Decimal::ZERO
    };
    let swap_paid: f64 = trades.iter().filter(|t| t.swap < 0.0).map(|t| -t.swap).sum();
    let swap_earned: f64 = trades.iter().filter(|t| t.swap > 0.0).map(|t| t.swap).sum();
    let earnings = if login.is_none() && app.up.configured(Target::Ib) {
        app.up.get(Target::Ib, tenant, As::User(user_id), "/v1/ib/me").await.ok().map(|v| num(&v["earnings"]["paid"]) + num(&v["earnings"]["approved"])).unwrap_or(0.0)
    } else {
        0.0
    };
    let g = |k: &str| round2(*flow.get(k).unwrap_or(&0.0));
    let equity_now: f64 = sc.iter().map(|a| a.equity * a.usd()).sum();

    let heat = metrics::hour_heatmap(&trades);
    let mut heat_n = vec![vec![0usize; 24]; 7];
    for t in &trades {
        let s = time::server_naive(t.close_time);
        heat_n[s.weekday().num_days_from_monday() as usize][chrono::Timelike::hour(&s) as usize] += 1;
    }
    let trade_json = |t: &Option<Trade>| t.as_ref().map(|t| json!({"deal": t.deal, "ticket": t.ticket, "login": t.login, "symbol": t.symbol, "side": t.side, "volume": t.volume, "net": round2(t.net), "closeTime": t.close_time}));
    let stats = |s: &metrics::TradeStats| {
        json!({
            "trades": s.trades, "wins": s.wins, "losses": s.losses, "winRate": round2(s.win_rate),
            "grossProfit": round2(s.gross_profit), "grossLoss": round2(s.gross_loss), "net": round2(s.net),
            "avgWin": round2(s.avg_win), "avgLoss": round2(s.avg_loss), "profitFactor": s.profit_factor.map(round2),
            "expectancy": round2(s.expectancy), "rewardRisk": s.reward_risk.map(round2),
            "avgHoldSecs": s.avg_hold_secs.round(), "avgHoldWinSecs": s.avg_hold_win_secs.round(), "avgHoldLossSecs": s.avg_hold_loss_secs.round(),
            "lots": round2(s.lots), "commission": round2(s.commission), "swap": round2(s.swap), "profit": round2(s.profit),
            "maxConsecWins": s.max_consec_wins, "maxConsecLosses": s.max_consec_losses,
            "best": trade_json(&s.best), "worst": trade_json(&s.worst),
        })
    };
    Ok(json!({
        "scope": if login.is_some() { "account" } else { "live" },
        "from": from, "to": to, "currency": "USD",
        "accounts": accts.iter().map(|a| json!({"login": a.login, "type": a.kind, "group": a.group, "groupName": a.group_name, "product": a.product, "currency": a.currency, "cent": a.cent, "equity": a.equity, "balance": a.balance})).collect::<Vec<_>>(),
        "curve": {
            "points": points.iter().map(|p| json!({"day": p.day, "balance": p.balance, "equity": p.equity, "flow": p.flow, "index": (p.index * 1e6).round() / 1e6, "drawdown": round2(p.drawdown)})).collect::<Vec<_>>(),
            "maxDrawdown": round2(cstats.max_drawdown), "currentDrawdown": round2(cstats.current_drawdown), "returnPct": round2(cstats.return_pct),
            "sharpe": cstats.sharpe.map(round2), "sortino": cstats.sortino.map(round2), "volatility": cstats.volatility.map(round2),
        },
        "stats": stats(&s),
        "long": stats(&ls),
        "short": stats(&ss),
        "bySymbol": group_json(&by_symbol),
        "byWeekday": group_json(&by_weekday),
        "byDay": group_json(&metrics::by_close_day(&trades)),
        "bySession": metrics::SESSIONS.iter().map(|(name, hours)| {
            let g = by_session.iter().find(|x| x.key == *name);
            json!({"session": name, "hours": hours, "trades": g.map(|x| x.trades).unwrap_or(0), "net": round2(g.map(|x| x.net).unwrap_or(0.0)), "winRate": round2(g.map(|x| x.win_rate).unwrap_or(0.0))})
        }).collect::<Vec<_>>(),
        "hourHeatmap": heat,
        "hourTrades": heat_n,
        "moneyFlow": {
            "deposits": g("deposits"), "withdrawals": g("withdrawals"), "tradingPnl": g("tradingPnl"), "commission": g("commission"),
            "performanceFees": g("performanceFees"), "bonus": g("bonus"), "adjustments": g("adjustments"), "earnings": round2(earnings),
            "optionPremiums": g("optionPremiums"), "optionSettlements": g("optionSettlements"),
            "equityNow": round2(equity_now),
        },
        "charges": {
            "commission": round2(-g("commission")), "swapPaid": round2(swap_paid), "swapEarned": round2(swap_earned),
            "performanceFees": round2(-g("performanceFees")), "walletFees": wallet_fees.to_f64().unwrap_or(0.0),
            "spreadEstimate": round2(spread),
        },
        "behaviour": behaviour,
    }))
}

/// Money-flow bucket of a balance ledger kind. Kalks FX Options premiums and settlements are trading flows with
/// their own buckets: never deposits, withdrawals or adjustments.
pub fn flow_key(kind: &str) -> &'static str {
    match kind {
        "transfer_in" | "deposit" | "demo_initial" | "demo_refill" => "deposits",
        "transfer_out" | "withdrawal" => "withdrawals",
        "trade_pnl" => "tradingPnl",
        "commission" => "commission",
        "perf_fee" => "performanceFees",
        "option_premium" => "optionPremiums",
        "option_settlement" => "optionSettlements",
        _ => "adjustments",
    }
}

/// Calendar months (server time) since the account opened, newest first, with the month's figures. `net` is the
/// realised result: CFD trade results + realised option P&L (from the option exit deals) + commission + fees.
pub async fn months(app: &App, tenant: &str, login: i64) -> ApiResult<Value> {
    let a = statement::account_info(app, tenant, login).await?;
    let rows = sqlx::query(
        "SELECT to_char(at AT TIME ZONE 'UTC' + interval '3 hours', 'YYYY-MM') AS m, kind, sum(amount) AS amt FROM ledger
         WHERE tenant = $1 AND login = $2 AND sub_ledger = 'balance' GROUP BY 1, 2",
    )
    .bind(tenant)
    .bind(login)
    .fetch_all(&app.pool)
    .await?;
    let trades = sqlx::query(
        "SELECT to_char(time AT TIME ZONE 'UTC' + interval '3 hours', 'YYYY-MM') AS m, count(*) AS n,
                COALESCE(sum(profit) FILTER (WHERE option IS NOT NULL), 0) AS opt_pnl
         FROM deals WHERE tenant = $1 AND login = $2 AND entry <> 'in' AND NOT reversed GROUP BY 1",
    )
        .bind(tenant)
        .bind(login)
        .fetch_all(&app.pool)
        .await?;
    let mut by: BTreeMap<String, (f64, f64, f64, i64)> = BTreeMap::new();
    for r in rows {
        let e = by.entry(r.get("m")).or_default();
        let amt = r.get::<Decimal, _>("amt").to_f64().unwrap_or(0.0);
        match r.get::<String, _>("kind").as_str() {
            "transfer_in" | "deposit" | "demo_initial" | "demo_refill" => e.1 += amt,
            "transfer_out" | "withdrawal" => e.2 += amt,
            "trade_pnl" | "commission" | "perf_fee" => e.0 += amt,
            _ => {}
        }
    }
    for r in trades {
        let e = by.entry(r.get("m")).or_default();
        e.3 = r.get("n");
        // option premiums / settlements are cash; the month's result counts the realised option P&L
        e.0 += r.get::<Decimal, _>("opt_pnl").to_f64().unwrap_or(0.0);
    }
    let mut out = vec![];
    let mut m = time::month_start(time::server_day(a.created_at));
    let last = time::month_start(time::server_day(Utc::now()));
    while m <= last {
        let key = m.format("%Y-%m").to_string();
        let v = by.get(&key).cloned().unwrap_or_default();
        let next = time::add_months(m, 1);
        out.push(json!({"month": key, "from": m, "to": next, "net": round2(v.0), "deposits": round2(v.1), "withdrawals": round2(v.2), "trades": v.3}));
        m = next;
    }
    out.reverse();
    Ok(json!({"login": login, "currency": a.currency, "months": out}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_flows_have_their_own_buckets() {
        assert_eq!(flow_key("option_premium"), "optionPremiums");
        assert_eq!(flow_key("option_settlement"), "optionSettlements");
        assert_eq!(flow_key("transfer_in"), "deposits");
        assert_eq!(flow_key("withdrawal"), "withdrawals");
        assert_eq!(flow_key("reversal"), "adjustments");
    }
}

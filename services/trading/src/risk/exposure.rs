//! Server-side exposure per tenant: net client volume and notional per symbol (long / short / net, per book), house
//! floating P&L (B-book), today's realised P&L (server day), the top exposed clients per symbol and per group.
//!
//! [`run`] scans the live accounts every second (the order gate's index, `RiskCtx::set_index`), and every 5 seconds
//! builds the full view, pushes `{type:"exposure"}` on the dealing stream and evaluates the exposure / daily-loss /
//! news / limit-refusal alerts. CFD positions only (options have their own desk); amounts in USD (cent accounts / 100).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use super::alerts::{self, AlertSpec, Level};
use super::{ExpIndex, RiskLimits};
use crate::api::AppState;
use crate::model::{AccountKind, Book, Side};
use crate::money::{D, ZERO, num, r2};
use crate::shard::ScanFn;

/// One open CFD position, valued now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PosLite {
    pub login: i64,
    pub user_id: i64,
    pub group: String,
    pub symbol: String,
    pub side: Side,
    pub volume: D,
    pub book: Book,
    /// Notional of the position in USD (always ≥ 0).
    pub notional_usd: D,
    /// Client floating P&L incl. swap, USD (None without a price).
    pub floating_usd: Option<D>,
}

/// The positions of the tenant's `kind` accounts (one scan of every shard).
pub async fn scan(st: &AppState, tenant_id: i64, kind: AccountKind) -> Vec<PosLite> {
    let f: ScanFn = Arc::new(move |a, env| {
        if a.account.kind != kind || a.positions.is_empty() {
            return vec![];
        }
        let acc = &a.account;
        a.positions
            .values()
            .filter(|p| p.option.is_none())
            .filter_map(|p| {
                let spec = env.specs.get(&p.symbol)?;
                let px = env.quote(acc, &p.symbol).map(|q| q.mid()).unwrap_or(p.open_price);
                let notional = env.to_usd(acc, &spec.quote_ccy, spec.contract_size * p.volume * px, (&p.symbol, px)).unwrap_or(ZERO);
                let floating = crate::engine::position_floating(env, acc, p).map(|v| v / acc.usd_factor());
                let x = PosLite { login: acc.login, user_id: acc.user_id, group: acc.group.clone(), symbol: p.symbol.clone(), side: p.side, volume: p.volume, book: p.book, notional_usd: notional, floating_usd: floating };
                serde_json::to_value(x).ok()
            })
            .collect()
    });
    st.hub.scan(tenant_id, f).await.into_iter().filter_map(|v| serde_json::from_value(v).ok()).collect()
}

/// The order gate's index from a scan.
pub fn index_of(positions: &[PosLite], started_ms: i64) -> ExpIndex {
    let mut idx = ExpIndex { started_ms, ..Default::default() };
    for p in positions {
        if p.book == Book::B {
            *idx.net_b_usd.entry(p.symbol.clone()).or_insert(ZERO) += p.side.sign() * p.notional_usd;
        }
        *idx.positions.entry(p.user_id).or_default().entry(p.login).or_insert(0) += 1;
    }
    idx
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SideBook {
    pub net_lots: D,
    pub net_usd: D,
    pub gross_usd: D,
    pub lots: D,
    pub positions: usize,
    pub floating: D,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SymbolRow {
    pub symbol: String,
    pub long_lots: D,
    pub short_lots: D,
    pub long_usd: D,
    pub short_usd: D,
    pub positions: usize,
    pub accounts: HashSet<i64>,
    pub floating: D,
    pub books: [SideBook; 2],
    /// login → (user, net lots, net USD, floating)
    pub clients: HashMap<i64, (i64, D, D, D)>,
}

impl SymbolRow {
    pub fn net_lots(&self) -> D {
        self.long_lots - self.short_lots
    }
    pub fn net_usd(&self) -> D {
        self.long_usd - self.short_usd
    }
    pub fn b(&self) -> &SideBook {
        &self.books[1]
    }
}

fn bi(b: Book) -> usize {
    if b == Book::A { 0 } else { 1 }
}

/// Aggregates positions per symbol (pure).
pub fn aggregate(positions: &[PosLite]) -> BTreeMap<String, SymbolRow> {
    let mut m: BTreeMap<String, SymbolRow> = BTreeMap::new();
    for p in positions {
        let r = m.entry(p.symbol.clone()).or_insert_with(|| SymbolRow { symbol: p.symbol.clone(), ..Default::default() });
        let s = p.side.sign();
        match p.side {
            Side::Buy => {
                r.long_lots += p.volume;
                r.long_usd += p.notional_usd;
            }
            Side::Sell => {
                r.short_lots += p.volume;
                r.short_usd += p.notional_usd;
            }
        }
        r.positions += 1;
        r.accounts.insert(p.login);
        let fl = p.floating_usd.unwrap_or(ZERO);
        r.floating += fl;
        let b = &mut r.books[bi(p.book)];
        b.net_lots += s * p.volume;
        b.net_usd += s * p.notional_usd;
        b.gross_usd += p.notional_usd;
        b.lots += p.volume;
        b.positions += 1;
        b.floating += fl;
        let c = r.clients.entry(p.login).or_insert((p.user_id, ZERO, ZERO, ZERO));
        c.1 += s * p.volume;
        c.2 += s * p.notional_usd;
        c.3 += fl;
    }
    m
}

/// Limit usage of a B-book net exposure: (used %, level).
pub fn limit_level(net_b: D, cap: D, warn_pct: D) -> (D, &'static str) {
    if cap <= ZERO {
        return (ZERO, "ok");
    }
    let used = net_b.abs() / cap * D::ONE_HUNDRED;
    let level = if used >= D::ONE_HUNDRED {
        "red"
    } else if used >= warn_pct {
        "amber"
    } else {
        "ok"
    };
    (r2(used), level)
}

/// Realised client P&L since `from` per book (exit deals, options excluded, reversed deals excluded), USD.
pub async fn realised_since(st: &AppState, tenant_id: i64, kind: AccountKind, from: DateTime<Utc>) -> anyhow::Result<(D, D)> {
    let rows = sqlx::query(
        "SELECT d.book, sum((d.profit + d.swap) / CASE WHEN a.cent THEN 100 ELSE 1 END) AS pnl
           FROM deals d JOIN accounts a ON a.login = d.login
          WHERE d.tenant_id = $1 AND a.kind = $2 AND d.entry <> 'in' AND NOT d.reversed AND d.time >= $3 AND d.data->'option' IS NULL
          GROUP BY d.book",
    )
    .bind(tenant_id)
    .bind(kind.as_str())
    .bind(from)
    .fetch_all(&st.pool)
    .await?;
    let (mut a, mut b) = (ZERO, ZERO);
    for r in rows {
        let v: D = r.get::<Option<D>, _>("pnl").unwrap_or_default();
        if r.get::<String, _>("book").trim() == "A" { a += v } else { b += v }
    }
    Ok((a, b))
}

/// Floating baseline of the server day (B-book client floating at the first scan of the day).
#[derive(Clone, Copy, Debug)]
pub struct Baseline {
    pub day: chrono::NaiveDate,
    pub floating_b: D,
    pub at: DateTime<Utc>,
}

/// The full exposure view (`GET /v1/dealing/exposure`).
pub fn view(positions: &[PosLite], limits: &RiskLimits, kind: AccountKind, now: DateTime<Utc>, day_start: DateTime<Utc>, realised: (D, D), baseline: Option<Baseline>) -> Value {
    let rows = aggregate(positions);
    let mut symbols: Vec<Value> = Vec::new();
    let (mut long, mut short, mut gross, mut floating) = (ZERO, ZERO, ZERO, ZERO);
    let mut books = [SideBook::default(), SideBook::default()];
    for r in rows.values() {
        long += r.long_usd;
        short += r.short_usd;
        gross += r.long_usd + r.short_usd;
        floating += r.floating;
        for i in 0..2 {
            let b = &mut books[i];
            b.net_usd += r.books[i].net_usd;
            b.gross_usd += r.books[i].gross_usd;
            b.lots += r.books[i].lots;
            b.positions += r.books[i].positions;
            b.floating += r.books[i].floating;
        }
        let limit = limits.exposure_for(&r.symbol).map(|l| {
            let (used, level) = limit_level(r.b().net_usd, l.cap_usd, l.warn_pct);
            json!({"capUsd": num(l.cap_usd), "warnPct": num(l.warn_pct), "usedPct": num(used), "level": level, "mode": l.mode.as_str()})
        });
        let mut top: Vec<(&i64, &(i64, D, D, D))> = r.clients.iter().collect();
        top.sort_by(|a, b| b.1.2.abs().cmp(&a.1.2.abs()).then(a.0.cmp(b.0)));
        let top: Vec<Value> = top.into_iter().take(5).map(|(l, (u, lots, usd, fl))| json!({"login": l.to_string(), "userId": u.to_string(), "netLots": num(*lots), "netUsd": num(r2(*usd)), "floatingUsd": num(r2(*fl))})).collect();
        symbols.push(json!({
            "symbol": r.symbol, "longLots": num(r.long_lots), "shortLots": num(r.short_lots), "netLots": num(r.net_lots()),
            "longUsd": num(r2(r.long_usd)), "shortUsd": num(r2(r.short_usd)), "netUsd": num(r2(r.net_usd())), "grossUsd": num(r2(r.long_usd + r.short_usd)),
            "positions": r.positions, "accounts": r.accounts.len(), "clientFloatingUsd": num(r2(r.floating)), "houseFloatingUsd": num(r2(-r.b().floating)),
            "books": {"A": {"netLots": num(r.books[0].net_lots), "netUsd": num(r2(r.books[0].net_usd))}, "B": {"netLots": num(r.books[1].net_lots), "netUsd": num(r2(r.books[1].net_usd))}},
            "limit": limit, "topClients": top,
        }));
    }
    symbols.sort_by(|a, b| {
        let x = |v: &Value| v["netUsd"].as_f64().unwrap_or(0.0).abs();
        x(b).total_cmp(&x(a))
    });
    let mut groups: BTreeMap<&str, (D, D, usize)> = BTreeMap::new();
    for p in positions {
        let g = groups.entry(p.group.as_str()).or_default();
        g.0 += p.side.sign() * p.notional_usd;
        g.1 += p.volume;
        g.2 += 1;
    }
    let mut groups: Vec<Value> = groups.into_iter().map(|(g, (n, l, c))| json!({"group": g, "netUsd": num(r2(n)), "lots": num(l), "positions": c})).collect();
    groups.sort_by(|a, b| b["netUsd"].as_f64().unwrap_or(0.0).abs().total_cmp(&a["netUsd"].as_f64().unwrap_or(0.0).abs()));
    let accounts: HashSet<i64> = positions.iter().map(|p| p.login).collect();
    let house_daily = baseline.map(|b| -realised.1 - (books[1].floating - b.floating_b)).unwrap_or(-realised.1 - books[1].floating);
    let book_json = |b: &SideBook| json!({"netUsd": num(r2(b.net_usd)), "grossUsd": num(r2(b.gross_usd)), "lots": num(b.lots), "positions": b.positions, "clientFloatingUsd": num(r2(b.floating))});
    json!({
        "generatedAt": now, "kind": kind.as_str(), "serverDayStart": day_start,
        "totals": {
            "grossUsd": num(r2(gross)), "netUsd": num(r2(long - short)), "longUsd": num(r2(long)), "shortUsd": num(r2(short)),
            "positions": positions.len(), "accounts": accounts.len(),
            "clientFloatingUsd": num(r2(floating)), "houseFloatingUsd": num(r2(-books[1].floating)),
            "realisedToday": {"clientUsd": num(r2(realised.0 + realised.1)), "houseUsd": num(r2(-realised.1)), "byBook": {"A": num(r2(realised.0)), "B": num(r2(realised.1))}},
            "houseDailyPnlUsd": num(r2(house_daily)), "baselineAt": baseline.map(|b| b.at),
        },
        "books": {"A": book_json(&books[0]), "B": book_json(&books[1])},
        "symbols": symbols,
        "groups": groups,
    })
}

/// Server day start (00:00 broker time) of `now`.
pub fn day_start(now: DateTime<Utc>) -> DateTime<Utc> {
    crate::specs::server_midnight(crate::specs::server_date(now))
}

/// Computes the view on demand (demo accounts, or before the loop's first pass).
pub async fn compute(st: &AppState, tenant_id: i64, kind: AccountKind) -> anyhow::Result<Value> {
    let Some(cfg) = st.hub.shared.registry.get(tenant_id) else { anyhow::bail!("unknown tenant") };
    let now = st.hub.shared.clock.now();
    let start = day_start(now);
    let pos = scan(st, tenant_id, kind).await;
    let realised = realised_since(st, tenant_id, kind, start).await?;
    Ok(view(&pos, &cfg.risk.limits, kind, now, start, realised, None))
}

/// Exposure / daily-loss / news / refusal alerts of one pass.
async fn evaluate(st: &AppState, tenant_id: i64, positions: &[PosLite], v: &Value, limits: &RiskLimits) {
    let mut active = HashSet::new();
    for s in v["symbols"].as_array().into_iter().flatten() {
        let Some(l) = s.get("limit").filter(|l| !l.is_null()) else { continue };
        let level = match l["level"].as_str() {
            Some("red") => Level::Red,
            Some("amber") => Level::Amber,
            _ => continue,
        };
        let sym = s["symbol"].as_str().unwrap_or_default().to_string();
        let net = s["books"]["B"]["netUsd"].as_f64().unwrap_or(0.0);
        let cap = l["capUsd"].as_f64().unwrap_or(0.0);
        let used = l["usedPct"].as_f64().unwrap_or(0.0);
        let side = if net >= 0.0 { "clients net long, house short" } else { "clients net short, house long" };
        let key = format!("exposure:{sym}");
        active.insert(key.clone());
        alerts::raise(
            st,
            tenant_id,
            AlertSpec {
                key,
                kind: "exposure",
                level,
                title: format!("{sym} net exposure at {used:.0}% of the limit"),
                message: format!("B-book net {:.0} USD ({side}); limit {:.0} USD ({}); {} positions on {} accounts", net.abs(), cap, if l["mode"] == "enforce" { "enforcing: increasing orders are refused" } else { "alert only" }, s["positions"], s["accounts"]),
                symbol: Some(sym),
                login: None,
                data: json!({"netUsd": net, "capUsd": cap, "usedPct": used, "mode": l["mode"], "topClients": s["topClients"]}),
            },
        )
        .await;
    }
    alerts::resolve_missing(st, tenant_id, &["exposure"], &active).await;

    let mut active = HashSet::new();
    if let Some(d) = limits.daily_loss() {
        let pnl = v["totals"]["houseDailyPnlUsd"].as_f64().unwrap_or(0.0);
        let thr = d.threshold_usd.to_string().parse::<f64>().unwrap_or(0.0);
        let warn = thr * d.warn_pct.to_string().parse::<f64>().unwrap_or(80.0) / 100.0;
        let loss = -pnl;
        let level = if loss >= thr { Some(Level::Red) } else if loss >= warn { Some(Level::Amber) } else { None };
        if let Some(level) = level {
            active.insert("daily_loss".to_string());
            alerts::raise(
                st,
                tenant_id,
                AlertSpec {
                    key: "daily_loss".into(),
                    kind: "daily_loss",
                    level,
                    title: format!("House daily loss {:.0} USD ({:.0}% of the threshold)", loss, if thr > 0.0 { loss / thr * 100.0 } else { 0.0 }),
                    message: format!("B-book P&L since the server day started: {pnl:.0} USD (realised {} USD, floating {} USD); threshold {thr:.0} USD", v["totals"]["realisedToday"]["houseUsd"], v["totals"]["houseFloatingUsd"]),
                    symbol: None,
                    login: None,
                    data: json!({"houseDailyPnlUsd": pnl, "thresholdUsd": thr}),
                },
            )
            .await;
        }
    }
    alerts::resolve_missing(st, tenant_id, &["daily_loss"], &active).await;

    // news windows that are open now on symbols the clients hold
    let mut active = HashSet::new();
    if let Some(n) = limits.news() {
        let now = st.hub.shared.clock.now();
        let specs = st.hub.shared.specs.load();
        let held: HashSet<&str> = positions.iter().map(|p| p.symbol.as_str()).collect();
        let mut seen = HashSet::new();
        for sym in held {
            let Some(spec) = specs.get(sym) else { continue };
            let Some(h) = st.hub.shared.risk.news_hit(spec, now, n.minutes_before as i64, n.minutes_after as i64, n.min_impact) else { continue };
            if !seen.insert(h.event.id) {
                continue;
            }
            let key = format!("news:{}", h.event.id);
            active.insert(key.clone());
            let actions = {
                let mut a = Vec::new();
                if n.margin_multiplier > D::ONE {
                    a.push(format!("margin × {} on new exposure", n.margin_multiplier.normalize()));
                }
                if n.block_pending {
                    a.push("no new pending orders".to_string());
                }
                if a.is_empty() { "no action configured".to_string() } else { format!("{} ({})", a.join(", "), if n.mode == super::LimitMode::Enforce { "enforcing" } else { "alert only" }) }
            };
            alerts::raise(
                st,
                tenant_id,
                AlertSpec {
                    key,
                    kind: "news",
                    level: Level::Amber,
                    title: format!("News window: {} {}", h.event.currency, h.event.title),
                    message: format!("{} UTC, impact {}; {actions}", h.event.starts_at.format("%H:%M"), h.event.impact),
                    symbol: None,
                    login: None,
                    data: json!({"eventId": h.event.id, "startsAt": h.event.starts_at, "currency": h.event.currency}),
                },
            )
            .await;
        }
    }
    alerts::resolve_missing(st, tenant_id, &["news"], &active).await;
}

/// Orders limits refused (or would have refused) since the last pass, grouped per code and symbol.
async fn refusals(st: &AppState, obs: Vec<super::Observation>) {
    let mut groups: BTreeMap<(i64, &'static str, String, bool), Vec<super::Observation>> = BTreeMap::new();
    for o in obs {
        groups.entry((o.tenant_id, o.code, o.symbol.clone(), o.enforced)).or_default().push(o);
    }
    for ((tenant, code, symbol, enforced), list) in groups {
        let what = match code {
            "exposure_limit" => "the exposure limit",
            "group_max_lot" => "the group max lot",
            "group_max_positions" => "the group max positions",
            "news_window" => "the news window",
            _ => "a risk limit",
        };
        let logins: Vec<String> = list.iter().map(|o| o.login.to_string()).collect::<std::collections::BTreeSet<_>>().into_iter().take(10).collect();
        let n = list.len();
        let last = list.last().map(|o| o.message.clone()).unwrap_or_default();
        alerts::raise(
            st,
            tenant,
            AlertSpec {
                key: format!("reject:{code}:{symbol}{}", if enforced { "" } else { ":alert" }),
                kind: "limit_reject",
                level: Level::Amber,
                title: if enforced { format!("{symbol}: {n} order(s) refused by {what}") } else { format!("{symbol}: {n} order(s) would have been refused by {what} (alert only)") },
                message: format!("{last} · accounts {}", logins.join(", ")),
                symbol: Some(symbol.clone()),
                login: list.last().map(|o| o.login),
                data: json!({"code": code, "orders": n, "logins": logins, "enforced": enforced}),
            },
        )
        .await;
    }
}

/// The exposure loop (see the module doc).
pub async fn run(st: AppState) {
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut n: u64 = 0;
    let mut baselines: HashMap<i64, Baseline> = HashMap::new();
    loop {
        tick.tick().await;
        n += 1;
        let risk = st.hub.shared.risk.clone();
        for t in st.hub.shared.registry.all() {
            let started = Utc::now().timestamp_millis();
            let pos = scan(&st, t.tenant_id, AccountKind::Live).await;
            risk.set_index(t.tenant_id, index_of(&pos, started));
            if n % 5 != 1 {
                continue;
            }
            let now = st.hub.shared.clock.now();
            let start = day_start(now);
            let realised = match realised_since(&st, t.tenant_id, AccountKind::Live, start).await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(tenant = t.tenant_id, error = %e, "realised P&L query failed");
                    continue;
                }
            };
            let day = crate::specs::server_date(now);
            let floating_b: D = pos.iter().filter(|p| p.book == Book::B).filter_map(|p| p.floating_usd).sum();
            let b = *baselines.entry(t.tenant_id).and_modify(|b| if b.day != day { *b = Baseline { day, floating_b, at: now } }).or_insert(Baseline { day, floating_b, at: now });
            let v = view(&pos, &t.risk.limits, AccountKind::Live, now, start, realised, Some(b));
            if let Some(s) = st.hub.shared.streams.dealing(t.tenant_id) {
                let _ = s.send(Arc::from(json!({"type": "exposure", "exposure": v}).to_string()));
            }
            evaluate(&st, t.tenant_id, &pos, &v, &t.risk.limits).await;
            risk.views.write().unwrap().insert(t.tenant_id, Arc::new(v));
            if n % 60 == 1 {
                alerts::resolve_quiet(&st, t.tenant_id, &["limit_reject", "routing"], 3600).await;
            }
        }
        let obs = risk.drain_observed();
        if !obs.is_empty() {
            refusals(&st, obs).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk::{ExposureLimit, LimitMode};

    fn d(s: &str) -> D {
        s.parse().unwrap()
    }

    fn p(login: i64, user: i64, symbol: &str, side: Side, vol: &str, book: Book, usd: &str, fl: &str) -> PosLite {
        PosLite { login, user_id: user, group: "standard".into(), symbol: symbol.into(), side, volume: d(vol), book, notional_usd: d(usd), floating_usd: Some(d(fl)) }
    }

    #[test]
    fn exposure_math_per_symbol_book_client_and_limit() {
        let pos = vec![
            p(1, 11, "EURUSD", Side::Buy, "2", Book::B, "232000", "150"),
            p(1, 11, "EURUSD", Side::Sell, "0.5", Book::B, "58000", "-20"),
            p(2, 12, "EURUSD", Side::Buy, "1", Book::A, "116000", "40"),
            p(3, 13, "EURUSD", Side::Sell, "3", Book::B, "348000", "-90"),
            p(3, 13, "XAUUSD", Side::Buy, "1", Book::B, "265000", "500"),
        ];
        let rows = aggregate(&pos);
        let e = &rows["EURUSD"];
        assert_eq!((e.long_lots, e.short_lots, e.net_lots()), (d("3"), d("3.5"), d("-0.5")));
        assert_eq!(e.net_usd(), d("-58000"));
        // B-book: +232 000 − 58 000 − 348 000
        assert_eq!(e.b().net_usd, d("-174000"));
        assert_eq!(e.books[0].net_usd, d("116000"));
        assert_eq!(e.floating, d("80"));
        assert_eq!(e.accounts.len(), 3);
        assert_eq!(e.clients[&1].2, d("174000"));
        let idx = index_of(&pos, 0);
        assert_eq!(idx.net_b_usd["EURUSD"], d("-174000"));
        assert_eq!(idx.net_b_usd["XAUUSD"], d("265000"));
        assert_eq!(idx.positions[&13][&3], 2);
        // usage against a 200k cap, amber from 80 %
        assert_eq!(limit_level(d("-174000"), d("200000"), d("80")), (d("87"), "amber"));
        assert_eq!(limit_level(d("265000"), d("200000"), d("80")), (d("132.5"), "red"));
        assert_eq!(limit_level(d("100"), d("200000"), d("80")).1, "ok");
        let limits = RiskLimits { exposure: vec![ExposureLimit { symbol: "*".into(), cap_usd: d("200000"), warn_pct: d("80"), mode: LimitMode::Alert, enabled: true }], ..Default::default() };
        let now = Utc::now();
        let v = view(&pos, &limits, AccountKind::Live, now, now, (d("10"), d("-300")), Some(Baseline { day: now.date_naive(), floating_b: d("100"), at: now }));
        // sorted by |net|: XAUUSD 265k first
        assert_eq!(v["symbols"][0]["symbol"], "XAUUSD");
        assert_eq!(v["symbols"][0]["limit"]["level"], "red");
        let eur = &v["symbols"][1];
        assert_eq!(eur["netUsd"], json!(-58000.0));
        assert_eq!(eur["books"]["B"]["netUsd"], json!(-174000.0));
        assert_eq!(eur["houseFloatingUsd"], json!(-40.0));
        assert_eq!(eur["topClients"][0]["login"], "3");
        assert_eq!(eur["topClients"][0]["netUsd"], json!(-348000.0));
        let t = &v["totals"];
        assert_eq!(t["grossUsd"], json!(1019000.0));
        assert_eq!(t["positions"], 5);
        assert_eq!(t["accounts"], 3);
        // B floating 150 − 20 − 90 + 500 = 540 → house −540
        assert_eq!(t["houseFloatingUsd"], json!(-540.0));
        assert_eq!(t["realisedToday"]["houseUsd"], json!(300.0));
        assert_eq!(t["realisedToday"]["clientUsd"], json!(-290.0));
        // house today: +300 realised − (540 − 100) floating change
        assert_eq!(t["houseDailyPnlUsd"], json!(-140.0));
        assert_eq!(v["books"]["B"]["positions"], 4);
    }
}

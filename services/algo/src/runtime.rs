//! 24/7 strategy runtime (D81, D84).
//!
//! The supervisor groups running deployments by (symbol, timeframe) and schedules a candle fetch from
//! market-data a few seconds after each bar closes. On a new closed bar every deployment of the group is
//! evaluated with the same evaluator as the backtester (so a live strategy fires where its backtest did).
//! Orders go to the trading engine through the terminal API on a session obtained with the one-time SSO
//! flow, tagged `source: "strategy"` with the deployment id in the comment and an idempotent
//! `clientOrderId`, so the engine applies every normal check (margin, sessions, dealer controls, max lot).
//! A management tick every few seconds books closes from the engine's deals, moves stops to breakeven and
//! enforces "close outside the trading window". Kill switches: per deployment, per user, and platform-wide.
//! The sandbox is the DSL itself (expressions only) plus an evaluation deadline per bar.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, Utc};
use reqwest::Method;
use serde_json::{Value, json};
use sqlx::Row;

use crate::dsl::Program;
use crate::dsl::eval::{Evaluator, Frame};
use crate::indicators::{self as ind, Bar};
use crate::specs::{server_date, server_time, tf_secs, ts};
use crate::state::{AppState, halted};
use crate::strategy::load_version;

#[derive(Clone, Debug)]
pub struct Dep {
    pub id: i64,
    pub tenant: String,
    pub user_id: i64,
    pub login: i64,
    pub status: String,
    pub name: String,
    pub program: Arc<Program>,
    pub risk: Value,
    pub last_bar_t: Option<i64>,
}

#[derive(Default)]
struct Group {
    last_closed: Option<i64>,
    next_check: i64,
}

type Locks = Arc<Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>>;

fn lock_for(locks: &Locks, id: i64) -> Arc<tokio::sync::Mutex<()>> {
    locks.lock().unwrap().entry(id).or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))).clone()
}

pub async fn log(st: &AppState, dep: i64, level: &str, kind: &str, msg: &str) {
    let _ = sqlx::query("INSERT INTO deployment_logs (tenant_id, deployment_id, level, kind, message) SELECT tenant_id, id, $2, $3, $4 FROM deployments WHERE id = $1")
        .bind(dep)
        .bind(level)
        .bind(kind)
        .bind(msg.chars().take(2000).collect::<String>())
        .execute(&st.pool)
        .await;
}

async fn load_deps(st: &AppState, cache: &mut HashMap<i64, Arc<Program>>) -> Vec<Dep> {
    let rows = match sqlx::query("SELECT d.id, d.tenant_id, d.user_id, d.login, d.status, d.version_id, d.risk, d.last_bar_t, s.name FROM deployments d JOIN strategies s ON s.id = d.strategy_id WHERE d.status IN ('running','paused')")
        .fetch_all(&st.pool)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "runtime: loading deployments failed");
            return vec![];
        }
    };
    let mut out = vec![];
    for r in rows {
        let vid: i64 = r.get("version_id");
        let id: i64 = r.get("id");
        let tenant: String = r.get("tenant_id");
        let prog = match cache.get(&vid) {
            Some(p) => p.clone(),
            None => match load_version(&st.pool, &tenant, vid).await.ok().flatten().map(|v| v.program(&st.specs)) {
                Some(Ok(p)) => {
                    let p = Arc::new(p);
                    cache.insert(vid, p.clone());
                    p
                }
                other => {
                    let msg = match other {
                        Some(Err(e)) => e,
                        _ => "strategy version not found".into(),
                    };
                    let _ = sqlx::query("UPDATE deployments SET status = 'error', error = $2, stopped_at = now() WHERE id = $1").bind(id).bind(&msg).execute(&st.pool).await;
                    log(st, id, "error", "error", &format!("Stopped: {msg}")).await;
                    continue;
                }
            },
        };
        out.push(Dep {
            id,
            tenant,
            user_id: r.get("user_id"),
            login: r.get("login"),
            status: r.get("status"),
            name: r.get("name"),
            program: prog,
            risk: r.get("risk"),
            last_bar_t: r.get("last_bar_t"),
        });
    }
    out
}

pub fn spawn(st: AppState) {
    let locks: Locks = Arc::new(Mutex::new(HashMap::new()));
    // bar scheduler
    {
        let st = st.clone();
        let locks = locks.clone();
        tokio::spawn(async move {
            let mut cache: HashMap<i64, Arc<Program>> = HashMap::new();
            let mut groups: HashMap<(String, String), Group> = HashMap::new();
            let mut deps: Vec<Dep> = vec![];
            let mut loaded_at = Instant::now() - Duration::from_secs(60);
            loop {
                let woke = tokio::time::timeout(Duration::from_secs(1), st.runtime_wake.notified()).await.is_ok();
                if woke || loaded_at.elapsed() > Duration::from_secs(10) {
                    deps = load_deps(&st, &mut cache).await;
                    loaded_at = Instant::now();
                    let live: HashSet<(String, String)> = deps.iter().map(|d| (d.program.spec.symbol.clone(), d.program.spec.timeframe.clone())).collect();
                    groups.retain(|k, _| live.contains(k));
                    if woke {
                        // a new deployment gets its warm-up evaluation right away
                        for g in groups.values_mut() {
                            g.next_check = 0;
                        }
                    }
                }
                let now = Utc::now().timestamp();
                let mut by_group: HashMap<(String, String), Vec<Dep>> = HashMap::new();
                for d in &deps {
                    by_group.entry((d.program.spec.symbol.clone(), d.program.spec.timeframe.clone())).or_default().push(d.clone());
                }
                for (key, members) in by_group {
                    let g = groups.entry(key.clone()).or_default();
                    if now < g.next_check {
                        continue;
                    }
                    let secs = tf_secs(&key.1).unwrap_or(60);
                    let need = members.iter().map(|d| d.program.lookback).max().unwrap_or(50) + 60;
                    let bars = match st.md.candles(&key.0, &key.1, need.clamp(300, 5000), None).await {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!(symbol = %key.0, tf = %key.1, error = %e, "runtime: candles");
                            g.next_check = now + 10;
                            continue;
                        }
                    };
                    let closed: Vec<Bar> = bars.iter().filter(|b| b.t + secs <= now).copied().collect();
                    let forming = bars.last().filter(|b| b.t + secs > now).map(|b| b.t);
                    g.next_check = match forming {
                        Some(t) => (t + secs + 3).max(now + 2),
                        None => now + secs.min(30),
                    };
                    let Some(last) = closed.last().map(|b| b.t) else { continue };
                    g.last_closed = Some(last);
                    let closed = Arc::new(closed);
                    for d in members {
                        if d.last_bar_t.is_some_and(|t| t >= last) {
                            continue;
                        }
                        let warmup = d.last_bar_t.is_none();
                        let (st, closed, lock) = (st.clone(), closed.clone(), lock_for(&locks, d.id));
                        // mark the bar as taken before evaluating so a slow evaluation is never repeated
                        let _ = sqlx::query("UPDATE deployments SET last_bar_t = $2, last_eval_at = now() WHERE id = $1").bind(d.id).bind(last).execute(&st.pool).await;
                        tokio::spawn(async move {
                            let _g = lock.lock().await;
                            if let Err(e) = on_bar(&st, &d, &closed, warmup).await {
                                log(&st, d.id, "error", "error", &format!("Evaluation failed: {e}")).await;
                            }
                        });
                    }
                    for d in deps.iter_mut() {
                        if d.program.spec.symbol == key.0 && d.program.spec.timeframe == key.1 {
                            d.last_bar_t = Some(last);
                        }
                    }
                }
            }
        });
    }
    // management tick
    {
        let st = st.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(3)).await;
                let rows = sqlx::query(
                    "SELECT DISTINCT d.id FROM deployments d WHERE d.status IN ('running','paused')
                       AND (EXISTS (SELECT 1 FROM deployment_positions p WHERE p.deployment_id = d.id AND p.closed_at IS NULL))",
                )
                .fetch_all(&st.pool)
                .await
                .unwrap_or_default();
                let mut cache = HashMap::new();
                let deps = load_deps(&st, &mut cache).await;
                let ids: HashSet<i64> = rows.iter().map(|r| r.get::<i64, _>("id")).collect();
                for d in deps.into_iter().filter(|d| ids.contains(&d.id) || d.program.spec.close_outside_session) {
                    let (st, lock) = (st.clone(), lock_for(&locks, d.id));
                    tokio::spawn(async move {
                        let Ok(_g) = lock.try_lock() else { return };
                        if let Err(e) = manage(&st, &d).await {
                            tracing::debug!(dep = d.id, error = %e, "runtime manage");
                        }
                    });
                }
            }
        });
    }
}

/* ------------------------------------------------------------------ */
/* Engine helpers                                                      */
/* ------------------------------------------------------------------ */

async fn state(st: &AppState, d: &Dep) -> anyhow::Result<Value> {
    let r = st.engine.terminal(&d.tenant, d.user_id, d.login, Method::GET, "/v1/terminal/state?historyLimit=1", None).await?;
    if !r.ok() {
        anyhow::bail!("account {}: {} ({})", d.login, r.message(), r.code());
    }
    Ok(r.body)
}

pub fn in_session(spec: &crate::spec::StrategySpec, t: i64) -> bool {
    let stt = server_time(ts(t));
    let wd = stt.weekday().num_days_from_sunday() as i64;
    if !spec.days.is_empty() && !spec.days.contains(&wd) {
        return false;
    }
    if spec.sessions.is_empty() {
        return true;
    }
    let mins = stt.timestamp().rem_euclid(86400) / 60;
    let m = |h: &str| h[..2].parse::<i64>().unwrap_or(0) * 60 + h[3..5].parse::<i64>().unwrap_or(0);
    spec.sessions.iter().any(|w| {
        let (a, b) = (m(&w.start), m(&w.end));
        if a <= b { mins >= a && mins < b } else { mins >= a || mins < b }
    })
}

/// Whether an engine deal / position JSON is a Kalks FX Options one (`option` object, `instrument`, or the
/// series code `EURUSD-20261009-1.1650-C`). Strategies only ever trade CFDs.
pub fn is_option(v: &Value) -> bool {
    if v.get("option").is_some_and(Value::is_object) || v.get("instrument").and_then(Value::as_str) == Some("option") {
        return true;
    }
    let sym = v.get("symbol").and_then(Value::as_str).unwrap_or("");
    let p: Vec<&str> = sym.split('-').collect();
    p.len() == 4 && p[1].len() == 8 && p[1].bytes().all(|b| b.is_ascii_digit()) && !p[2].is_empty() && p[2].bytes().all(|b| b.is_ascii_digit() || b == b'.') && matches!(p[3], "C" | "P")
}

/// The exit deals of a tracked position in an account history: CFD exits only. A deployment only tracks the
/// CFD tickets it opened itself, so an option deal (premium cash, contracts) can never be booked into its
/// realised P&L, daily loss or track record.
pub fn exits_of(deals: &[Value], ticket: i64) -> Vec<&Value> {
    deals
        .iter()
        .filter(|x| x.get("positionTicket").and_then(Value::as_i64) == Some(ticket) && x.get("entry").and_then(Value::as_str) != Some("in") && !is_option(x))
        .collect()
}

/// Books positions of this deployment that the engine has closed (SL/TP/trailing, manual, stop-out, kill).
pub async fn sync_closed(st: &AppState, d: &Dep, open_now: &HashSet<i64>) -> anyhow::Result<()> {
    let rows = sqlx::query("SELECT ticket, opened_at FROM deployment_positions WHERE deployment_id = $1 AND closed_at IS NULL").bind(d.id).fetch_all(&st.pool).await?;
    let gone: Vec<(i64, chrono::DateTime<Utc>)> = rows.iter().map(|r| (r.get("ticket"), r.get("opened_at"))).filter(|(t, _)| !open_now.contains(t)).collect();
    if gone.is_empty() {
        return Ok(());
    }
    let from = gone.iter().map(|g| g.1).min().unwrap() - chrono::Duration::days(1);
    let path = format!("/v1/accounts/{}/history?from={}&limit=1000", d.login, from.format("%Y-%m-%d"));
    let h = st.engine.user_call(Method::GET, &path, &d.tenant, d.user_id, None).await?;
    if !h.ok() {
        anyhow::bail!("history: {}", h.message());
    }
    let deals = h.body.get("deals").and_then(Value::as_array).cloned().unwrap_or_default();
    for (ticket, _) in gone {
        let exits = exits_of(&deals, ticket);
        if exits.is_empty() {
            continue; // not in the history yet
        }
        let num = |x: &Value, k: &str| x.get(k).and_then(Value::as_f64).unwrap_or(0.0);
        let net: f64 = exits.iter().map(|x| num(x, "profit") + num(x, "swap") - num(x, "commission").abs()).sum();
        let net = (net * 100.0).round() / 100.0;
        let last = exits.iter().max_by_key(|x| x.get("time").and_then(Value::as_str).unwrap_or("").to_string()).unwrap();
        let reason = last.get("reason").and_then(Value::as_str).unwrap_or("close").to_string();
        let closed_at = last.get("time").and_then(Value::as_str).and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&Utc)).unwrap_or_else(Utc::now);
        let r = sqlx::query("UPDATE deployment_positions SET closed_at = $2, close_price = $3, profit = $4, reason = $5 WHERE ticket = $1 AND closed_at IS NULL")
            .bind(ticket)
            .bind(closed_at)
            .bind(num(last, "price"))
            .bind(rust_decimal::Decimal::from_f64_retain(net).unwrap_or_default())
            .bind(&reason)
            .execute(&st.pool)
            .await?;
        if r.rows_affected() == 0 {
            continue;
        }
        let day = server_date(closed_at);
        sqlx::query(
            "INSERT INTO deployment_daily (deployment_id, tenant_id, day, realized, trades, wins) VALUES ($1,$2,$3,$4,1,$5)
             ON CONFLICT (deployment_id, day) DO UPDATE SET realized = deployment_daily.realized + EXCLUDED.realized, trades = deployment_daily.trades + 1, wins = deployment_daily.wins + EXCLUDED.wins",
        )
        .bind(d.id)
        .bind(&d.tenant)
        .bind(day)
        .bind(rust_decimal::Decimal::from_f64_retain(net).unwrap_or_default())
        .bind(if net > 0.0 { 1 } else { 0 })
        .execute(&st.pool)
        .await?;
        let why = match reason.as_str() {
            "sl" => "stop loss",
            "tp" => "take profit",
            "stop_out" => "stop-out",
            "client" => "close request",
            r => r,
        };
        log(st, d.id, "info", "close", &format!("#{ticket} closed by {why} at {}, P&L {}{net:.2}", num(last, "price"), if net >= 0.0 { "+" } else { "" })).await;
    }
    refresh_stats(st, d.id).await;
    Ok(())
}

pub async fn refresh_stats(st: &AppState, id: i64) {
    let _ = sqlx::query(
        "UPDATE deployments d SET stats = d.stats || jsonb_build_object(
            'trades', (SELECT count(*) FROM deployment_positions p WHERE p.deployment_id = d.id AND p.closed_at IS NOT NULL),
            'wins', (SELECT count(*) FROM deployment_positions p WHERE p.deployment_id = d.id AND p.closed_at IS NOT NULL AND p.profit > 0),
            'realized', (SELECT COALESCE(sum(profit), 0) FROM deployment_positions p WHERE p.deployment_id = d.id AND p.closed_at IS NOT NULL),
            'open', (SELECT count(*) FROM deployment_positions p WHERE p.deployment_id = d.id AND p.closed_at IS NULL))
         WHERE d.id = $1",
    )
    .bind(id)
    .execute(&st.pool)
    .await;
}

/// Closes this deployment's open positions (optionally one side). Returns (closed, failed).
pub async fn close_tracked(st: &AppState, d: &Dep, side: Option<&str>, why: &str) -> (usize, usize) {
    let rows = sqlx::query("SELECT ticket, side, symbol FROM deployment_positions WHERE deployment_id = $1 AND closed_at IS NULL").bind(d.id).fetch_all(&st.pool).await.unwrap_or_default();
    let (mut ok, mut failed) = (0, 0);
    for r in rows {
        let (ticket, s): (i64, String) = (r.get("ticket"), r.get("side"));
        if side.is_some_and(|x| x != s) {
            continue;
        }
        match st.engine.terminal(&d.tenant, d.user_id, d.login, Method::POST, &format!("/v1/terminal/positions/{ticket}/close"), Some(&json!({}))).await {
            Ok(r) if r.ok() => {
                ok += 1;
                log(st, d.id, "info", "close", &format!("{why}: closing #{ticket} {s} {}", r.body.get("profit").map(|p| format!("(P&L {p})")).unwrap_or_default())).await;
            }
            Ok(r) if r.status == reqwest::StatusCode::NOT_FOUND => {}
            Ok(r) => {
                failed += 1;
                log(st, d.id, "warn", "error", &format!("{why}: could not close #{ticket}: {} ({})", r.message(), r.code())).await;
            }
            Err(e) => {
                failed += 1;
                log(st, d.id, "warn", "error", &format!("{why}: could not close #{ticket}: {e}")).await;
            }
        }
    }
    if let Ok(s) = state(st, d).await {
        let open: HashSet<i64> = s.get("positions").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| p.get("ticket").and_then(Value::as_i64)).collect()).unwrap_or_default();
        let _ = sync_closed(st, d, &open).await;
    }
    (ok, failed)
}

/* ------------------------------------------------------------------ */
/* Bar close                                                           */
/* ------------------------------------------------------------------ */

fn fmt(v: f64, digits: u32) -> String {
    format!("{v:.*}", digits as usize)
}

async fn on_bar(st: &AppState, d: &Dep, closed: &[Bar], warmup: bool) -> anyhow::Result<()> {
    let p = &d.program;
    let spec = &p.spec;
    let sp = st.specs.get(&spec.symbol).cloned().ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;
    let secs = tf_secs(&spec.timeframe).unwrap_or(60);
    let bar = *closed.last().unwrap();
    let bar_close = bar.t + secs;
    // higher timeframes: only bars closed by this bar's close
    let mut others = HashMap::new();
    for tf in &p.timeframes {
        let hs = tf_secs(tf).unwrap_or(3600);
        let hb = st.md.candles(&spec.symbol, tf, (p.lookback + 60).clamp(300, 5000), None).await?;
        others.insert(tf.clone(), Frame::new(tf, hb.into_iter().filter(|b| b.t + hs <= bar_close).collect()));
    }
    let base = Frame::new(&spec.timeframe, closed.to_vec());
    let sig = Evaluator::new(&base, &others, Some(Instant::now() + Duration::from_secs(3))).run(p).map_err(|e| anyhow::anyhow!("{e}"))?;
    let i = closed.len() - 1;
    let [buy, sell, xb, xs] = sig.at(i);
    let val = |s: &Vec<f64>| s.get(i).map(|v| if v.is_nan() { "warming up".to_string() } else if *v != 0.0 { "YES".into() } else { "no".into() });
    let mut parts = vec![];
    for (k, s) in [("BUY", &sig.buy), ("SELL", &sig.sell), ("EXIT BUY", &sig.exit_buy), ("EXIT SELL", &sig.exit_sell)] {
        if let Some(v) = val(s) {
            parts.push(format!("{k} {v}"));
        }
    }
    let hhmm = server_time(ts(bar.t)).format("%H:%M");
    log(st, d.id, "info", "eval", &format!("{} bar {hhmm} closed at {} · {}{}", spec.timeframe, fmt(bar.c, sp.digits), parts.join(" · "), if warmup { " · warm-up: orders start from the next closed bar" } else { "" })).await;
    if warmup {
        return Ok(());
    }

    // current account state + book closes
    let stt = state(st, d).await?;
    let positions = stt.get("positions").and_then(Value::as_array).cloned().unwrap_or_default();
    let open_now: HashSet<i64> = positions.iter().filter_map(|p| p.get("ticket").and_then(Value::as_i64)).collect();
    sync_closed(st, d, &open_now).await?;

    let (halt, why) = halted(&st.pool, &d.tenant, d.user_id).await;
    if halt {
        log(st, d.id, "warn", "signal", &format!("Signals ignored: {why}")).await;
        return Ok(());
    }
    // the broker switched the strategy builder off: the deployment pauses (kept as it is) until it is back on
    if !crate::modules::on(st, &d.tenant, "algo").await {
        log(st, d.id, "warn", "signal", "Signals ignored: strategies are switched off for this broker").await;
        return Ok(());
    }
    // rule exits
    if xb {
        close_tracked(st, d, Some("buy"), "Exit rule").await;
    }
    if xs {
        close_tracked(st, d, Some("sell"), "Exit rule").await;
    }
    if !(buy || sell) {
        return Ok(());
    }
    let side = if buy { "buy" } else { "sell" };
    let skip = |why: String| async move {
        log(st, d.id, "info", "signal", &format!("{} signal skipped: {why}", side.to_uppercase())).await;
    };
    if buy && sell {
        return Ok(skip("buy and sell rules both true on the same bar".into()).await);
    }
    if d.status != "running" {
        return Ok(skip("deployment is paused".into()).await);
    }
    let now = Utc::now().timestamp();
    if !in_session(spec, now) {
        return Ok(skip("outside the trading window".into()).await);
    }
    if !sp.is_open(ts(now)) {
        return Ok(skip("market closed".into()).await);
    }
    // daily counters (server day)
    let today = server_date(ts(now));
    let day_row = sqlx::query("SELECT COALESCE(realized, 0)::float8 AS r FROM deployment_daily WHERE deployment_id = $1 AND day = $2").bind(d.id).bind(today).fetch_optional(&st.pool).await?;
    let day_realized: f64 = day_row.map(|r| r.get("r")).unwrap_or(0.0);
    let opened_today: i64 = sqlx::query_scalar("SELECT count(*) FROM deployment_positions WHERE deployment_id = $1 AND opened_at >= $2")
        .bind(d.id)
        .bind(crate::specs::ts(today.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp() - crate::specs::server_offset_secs(ts(now))))
        .fetch_one(&st.pool)
        .await?;
    let tracked: Vec<i64> = sqlx::query_scalar("SELECT ticket FROM deployment_positions WHERE deployment_id = $1 AND closed_at IS NULL").bind(d.id).fetch_all(&st.pool).await?;
    let floating: f64 = positions.iter().filter(|p| p.get("ticket").and_then(Value::as_i64).is_some_and(|t| tracked.contains(&t))).map(|p| p.get("profit").and_then(Value::as_f64).unwrap_or(0.0) + p.get("swap").and_then(Value::as_f64).unwrap_or(0.0)).sum();
    if spec.max_trades_per_day > 0 && opened_today >= spec.max_trades_per_day {
        return Ok(skip(format!("daily limit of {} trades reached", spec.max_trades_per_day)).await);
    }
    let max_loss = d.risk.get("maxDailyLoss").and_then(Value::as_f64).filter(|v| *v > 0.0).map(|v| if spec.max_daily_loss > 0.0 { v.min(spec.max_daily_loss) } else { v }).unwrap_or(spec.max_daily_loss);
    if max_loss > 0.0 && day_realized + floating <= -max_loss {
        return Ok(skip(format!("max daily loss reached ({:.2})", day_realized + floating)).await);
    }
    let max_open = d.risk.get("maxOpenPositions").and_then(Value::as_u64).map(|v| v as usize).unwrap_or(if spec.one_at_a_time { 1 } else { 5 });
    let open_count = tracked.len();
    if (spec.one_at_a_time && open_count > 0) || open_count >= max_open {
        return Ok(skip(format!("{open_count} position(s) already open (limit {})", if spec.one_at_a_time { 1 } else { max_open })).await);
    }
    let account = stt.get("account").cloned().unwrap_or(json!({}));
    if account.get("mode").and_then(Value::as_str) == Some("netting")
        && let Some(other) = positions.iter().find(|p| p.get("symbol").and_then(Value::as_str) == Some(&spec.symbol) && !p.get("ticket").and_then(Value::as_i64).is_some_and(|t| tracked.contains(&t)))
    {
        return Ok(skip(format!("netting account already holds #{} on {}", other.get("ticket").cloned().unwrap_or(Value::Null), spec.symbol)).await);
    }

    // price for stop distances: the account group's quote
    let groups = st.engine.groups(&d.tenant).await.unwrap_or_default();
    let group = account.get("group").and_then(Value::as_str).unwrap_or("standard");
    let spread_group = groups.iter().find(|g| g.get("code").and_then(Value::as_str) == Some(group)).and_then(|g| g.get("spreadGroup").and_then(Value::as_str)).unwrap_or("standard").to_string();
    let q = st.md.quotes(std::slice::from_ref(&spec.symbol), &spread_group).await?.get(&spec.symbol).copied().ok_or_else(|| anyhow::anyhow!("no quote for {}", spec.symbol))?;
    let entry = if buy { q.ask } else { q.bid };
    let atr_at = |n: i64| ind::atr(closed, n.max(1) as usize).last().copied().unwrap_or(f64::NAN);
    let dist = |m: &crate::spec::Distance, sl: Option<f64>| -> Option<f64> {
        match m.mode.as_str() {
            "points" => Some(m.value * sp.point),
            "pips" => Some(m.value * sp.pip_size),
            "price" => Some(m.value),
            "percent" => Some(entry * m.value / 100.0),
            "atr" => Some(m.value * atr_at(m.atr_period)),
            "level" => Some((entry - m.value).abs()),
            "rr" => sl.map(|s| s * m.value),
            _ => None,
        }
    };
    let sl_d = dist(&spec.sl, None);
    let tp_d = dist(&spec.tp, sl_d);
    if sl_d.is_some_and(|v| !(v > 0.0)) || tp_d.is_some_and(|v| !(v > 0.0)) {
        return Ok(skip("stop distance could not be computed (ATR warming up?)".into()).await);
    }
    let dir = if buy { 1.0 } else { -1.0 };
    let sl = sl_d.map(|x| sp.round_price(entry - dir * x));
    let tp = tp_d.map(|x| sp.round_price(entry + dir * x));
    let mut vol = spec.sizing.lots;
    if spec.sizing.mode == "risk" {
        let Some(sd) = sl_d else { return Ok(skip("risk sizing needs a stop loss".into()).await) };
        let bal = account.get("balance").and_then(Value::as_f64).unwrap_or(0.0);
        let conv = if sp.quote_ccy == "USD" { 1.0 } else if sp.symbol.starts_with("USD") { 1.0 / entry } else { 1.0 };
        vol = bal * spec.sizing.risk_pct / 100.0 / (sd * sp.contract_size * conv);
    }
    vol *= d.risk.get("lotMultiplier").and_then(Value::as_f64).filter(|v| *v > 0.0).unwrap_or(1.0);
    let cap = d.risk.get("maxLots").and_then(Value::as_f64).filter(|v| *v > 0.0).map(|v| v.min(spec.max_lots)).unwrap_or(spec.max_lots);
    if vol > cap {
        log(st, d.id, "warn", "signal", &format!("Volume {vol:.2} capped at {cap} lot")).await;
        vol = cap;
    }
    let Some(vol) = sp.floor_volume(vol) else { return Ok(skip(format!("volume {vol:.4} is below the minimum {} lot", sp.lot_min)).await) };
    let trailing_pts = match spec.trailing.mode.as_str() {
        "points" => Some(spec.trailing.value),
        "pips" => Some(spec.trailing.value * sp.pip_size / sp.point),
        "atr" => Some(spec.trailing.value * atr_at(spec.trailing.atr_period) / sp.point).filter(|v| v.is_finite()),
        _ => None,
    }
    .map(|v| v.round().max(1.0) as i64);
    let comment: String = format!("S{} {}", d.id, d.name).chars().take(64).collect();
    let mut body = json!({
        "symbol": spec.symbol, "side": side, "type": "market", "volume": vol, "source": "strategy", "platform": "API",
        "comment": comment, "clientOrderId": format!("algo-{}-{}-{side}", d.id, bar.t),
    });
    if let Some(v) = sl {
        body["sl"] = json!(v);
    }
    if let Some(v) = tp {
        body["tp"] = json!(v);
    }
    if let Some(v) = trailing_pts {
        body["trailingPoints"] = json!(v);
    }
    let desc = format!("{} {vol} {} ~{}{}{}{}", side.to_uppercase(), spec.symbol, fmt(entry, sp.digits), sl.map(|v| format!(" SL {}", fmt(v, sp.digits))).unwrap_or_default(), tp.map(|v| format!(" TP {}", fmt(v, sp.digits))).unwrap_or_default(), trailing_pts.map(|v| format!(" trailing {v} pts")).unwrap_or_default());
    log(st, d.id, "info", "signal", &format!("{} signal: sending market order {desc}", side.to_uppercase())).await;
    let r = st.engine.terminal(&d.tenant, d.user_id, d.login, Method::POST, "/v1/terminal/orders", Some(&body)).await?;
    if !r.ok() {
        log(st, d.id, "error", "error", &format!("Order rejected: {} ({})", r.message(), r.code())).await;
        return Ok(());
    }
    let status = r.body.get("status").and_then(Value::as_str).unwrap_or("");
    let ticket = r.body.get("positionTicket").or_else(|| r.body.get("ticket")).and_then(Value::as_i64);
    let price = r.body.get("price").and_then(Value::as_f64);
    if let Some(t) = ticket
        && status != "duplicate"
    {
        sqlx::query("INSERT INTO deployment_positions (ticket, tenant_id, deployment_id, symbol, side, volume, open_price) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (ticket) DO NOTHING")
            .bind(t)
            .bind(&d.tenant)
            .bind(d.id)
            .bind(&spec.symbol)
            .bind(side)
            .bind(rust_decimal::Decimal::from_f64_retain(vol).unwrap_or_default())
            .bind(price)
            .execute(&st.pool)
            .await?;
        let _ = sqlx::query("UPDATE deployments SET stats = stats || jsonb_build_object('lastOrderAt', now(), 'orders', COALESCE((stats->>'orders')::int, 0) + 1) WHERE id = $1").bind(d.id).execute(&st.pool).await;
        refresh_stats(st, d.id).await;
    }
    log(st, d.id, "info", "order", &format!("{} #{} {side} {vol} {} at {}", if status == "duplicate" { "Already sent" } else { "Filled" }, ticket.map(|t| t.to_string()).unwrap_or("?".into()), spec.symbol, price.map(|p| fmt(p, sp.digits)).unwrap_or("?".into()))).await;
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Management tick                                                     */
/* ------------------------------------------------------------------ */

async fn manage(st: &AppState, d: &Dep) -> anyhow::Result<()> {
    let spec = &d.program.spec;
    let stt = state(st, d).await?;
    let positions = stt.get("positions").and_then(Value::as_array).cloned().unwrap_or_default();
    let open_now: HashSet<i64> = positions.iter().filter_map(|p| p.get("ticket").and_then(Value::as_i64)).collect();
    sync_closed(st, d, &open_now).await?;
    let now = Utc::now().timestamp();
    if spec.close_outside_session && !in_session(spec, now) {
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM deployment_positions WHERE deployment_id = $1 AND closed_at IS NULL").bind(d.id).fetch_one(&st.pool).await?;
        if n > 0 {
            close_tracked(st, d, None, "Outside the trading window").await;
        }
        return Ok(());
    }
    let be = spec.trailing.breakeven_trigger;
    if be <= 0.0 {
        return Ok(());
    }
    let sp = st.specs.get(&spec.symbol).cloned().ok_or_else(|| anyhow::anyhow!("unknown symbol"))?;
    let rows = sqlx::query("SELECT ticket FROM deployment_positions WHERE deployment_id = $1 AND closed_at IS NULL AND NOT breakeven_done").bind(d.id).fetch_all(&st.pool).await?;
    for r in rows {
        let t: i64 = r.get("ticket");
        let Some(p) = positions.iter().find(|p| p.get("ticket").and_then(Value::as_i64) == Some(t)) else { continue };
        let (open, cur) = (p.get("openPrice").and_then(Value::as_f64).unwrap_or(0.0), p.get("currentPrice").and_then(Value::as_f64).unwrap_or(0.0));
        let dir = if p.get("side").and_then(Value::as_str) == Some("buy") { 1.0 } else { -1.0 };
        if (cur - open) * dir < be * sp.point {
            continue;
        }
        let cand = sp.round_price(open + dir * spec.trailing.breakeven_offset * sp.point);
        let sl = p.get("sl").and_then(Value::as_f64);
        sqlx::query("UPDATE deployment_positions SET breakeven_done = TRUE WHERE ticket = $1").bind(t).execute(&st.pool).await?;
        if sl.is_some_and(|s| if dir > 0.0 { s >= cand } else { s <= cand }) {
            continue;
        }
        let r = st.engine.terminal(&d.tenant, d.user_id, d.login, Method::PATCH, &format!("/v1/terminal/positions/{t}"), Some(&json!({"sl": cand}))).await?;
        if r.ok() {
            log(st, d.id, "info", "manage", &format!("Breakeven: #{t} stop moved to {}", fmt(cand, sp.digits))).await;
        } else {
            log(st, d.id, "warn", "error", &format!("Breakeven modify of #{t} rejected: {}", r.message())).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_deals_never_reach_a_deployment() {
        let deals = vec![
            json!({"id": 1, "positionTicket": 7, "entry": "in", "symbol": "EURUSD", "profit": 0}),
            json!({"id": 2, "positionTicket": 7, "entry": "out", "symbol": "EURUSD", "profit": 12, "option": null, "instrument": "cfd"}),
            // an option deal that (impossibly) shares the ticket: never booked
            json!({"id": 3, "positionTicket": 7, "entry": "out", "symbol": "EURUSD-20261009-1.1650-C", "profit": 500, "instrument": "option", "option": {"series": "EURUSD-20261009-1.1650-C"}}),
            json!({"id": 4, "positionTicket": 7, "entry": "out", "symbol": "EURUSD-20261009-1.1650-P", "profit": 900}),
        ];
        let ex = exits_of(&deals, 7);
        assert_eq!(ex.len(), 1);
        assert_eq!(ex[0]["id"], 2);
        assert!(!is_option(&json!({"symbol": "BTC-USD"})) && !is_option(&json!({"symbol": "US30"})));
    }
}

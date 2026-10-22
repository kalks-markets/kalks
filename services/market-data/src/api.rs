//! HTTP + WebSocket API.
//!
//! GET  /health                          service status (provider connections, tick rate, streaming vs plan limit)
//! GET  /v1/instruments?class=&tier=&symbols=&q=   instrument catalogue (core + provider catalogue), optional filters
//! GET  /v1/quotes?symbols=&group=       latest quotes with the group's spread applied (`"d":true` = delayed: the
//!                                       symbol is not streamed right now; a symbol without any price gets one fetched)
//! GET  /v1/candles?symbol=&tf=&limit=&to=   OHLC history (+ the forming bar), ascending; the first request of a
//!                                       catalogue symbol fetches its history from the provider (waits up to 8 s)
//! GET  /v1/streaming                    provider subscriptions: plan limits, streamed symbols, demand by tier
//! GET  /v1/history/status               stored bars per symbol/timeframe
//! GET  /v1/admin/spreads                spread markups           (Bearer MARKET_DATA_ADMIN_TOKEN)
//! PUT  /v1/admin/spreads                upsert a markup          (Bearer MARKET_DATA_ADMIN_TOKEN)
//! GET  /v1/admin/adjustment-factors?symbol=&from=&to=   provider adjustment factors of a stock (Bearer token)
//! GET  /v1/depth?symbol=&group=&levels= depth of market (feed levels, else indicative) with the group's spread
//! WS   /v1/stream?group=                {"op":"subscribe","symbols":[..],"passive"?:true} → {"type":"quote",...,"d"?:1}
//!                                       (+ {"type":"hb"} every 5s). A subscription asks the provider stream for the
//!                                       symbol (demand.rs) unless passive; `{"op":"hold","symbols":[..]}` (trading
//!                                       engine: positions / orders) replaces the connection's held set
//!                                       {"op":"bars","symbol":"XAUUSD","tf":"M15"} → {"type":"bar",...}
//!                                       {"op":"depth","symbols":[..],"levels"?:10,"src"?:"feed"} → {"type":"depth",...} on every
//!                                       quote change of those symbols ("undepth" to stop; src "feed" = provider depth only)

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use futures_util::SinkExt;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tower_http::cors::CorsLayer;

use crate::db::{self, Bar};
use crate::demand::{ClientDemand, Tier};
use crate::depth;
use crate::spreads::Markup;
use crate::state::{Event, HistoryJob, Market, Quote};
use crate::timeframes::Tf;

#[derive(Clone)]
pub struct AppState {
    pub market: Arc<Market>,
    pub admin_token: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/instruments", get(instruments))
        .route("/v1/quotes", get(quotes))
        .route("/v1/candles", get(candles))
        .route("/v1/history/status", get(history_status))
        .route("/v1/depth", get(depth_rest))
        .route("/v1/streaming", get(streaming))
        .route("/v1/admin/spreads", get(get_spreads).put(put_spread))
        .route("/v1/admin/adjustment-factors", get(adjustment_factors))
        .route("/v1/stream", get(stream))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

struct ApiError(anyhow::Error);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": self.0.to_string()}))).into_response()
    }
}
impl<E: Into<anyhow::Error>> From<E> for ApiError {
    fn from(e: E) -> Self {
        ApiError(e.into())
    }
}
fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"error": msg}))).into_response()
}

async fn health(State(s): State<AppState>) -> Json<Value> {
    let st = s.market.stats();
    let now = Utc::now().timestamp_millis();
    let streaming = s.market.streaming();
    // only streamed symbols can be stale: the rest are not supposed to tick
    let stale: Vec<&String> = st.last_tick_ms.iter().filter(|(k, t)| now - **t > 60_000 && streaming.contains(*k)).map(|(k, _)| k).collect();
    let limits = s.market.demand.lock().unwrap().limits.clone();
    Json(json!({
        // false when an expected provider business has been missing for more than 5 minutes
        "ok": st.healthy(Utc::now()),
        "provider_streams": st.connected_markets,
        "provider_streams_missing": st.missing(),
        "provider_missing_since": st.missing_since,
        "provider_refused_since": st.refused_since,
        "ticks_total": st.ticks_total,
        "symbols_ticking": st.last_tick_ms.len(),
        "stale_over_60s": stale,
        "instruments": s.market.cat.list.len(),
        "streaming": streaming.len(),
        "plan_limit_total": limits.total,
    }))
}

#[derive(Deserialize)]
struct InstrumentsQ {
    class: Option<String>,
    /// `core` or `catalogue`
    tier: Option<String>,
    symbols: Option<String>,
    /// substring of the symbol or name, case-insensitive
    q: Option<String>,
}

async fn instruments(State(s): State<AppState>, Query(q): Query<InstrumentsQ>) -> Json<Value> {
    let wanted: Option<HashSet<String>> = q.symbols.map(|x| x.split(',').map(|s| s.trim().to_string()).collect());
    let needle = q.q.map(|x| x.to_lowercase());
    let list: Vec<&crate::instruments::Instrument> = s
        .market
        .cat
        .list
        .iter()
        .filter(|i| q.class.as_deref().is_none_or(|c| c == i.asset_class))
        .filter(|i| match q.tier.as_deref() {
            Some("core") => i.is_core(),
            Some("catalogue") => !i.is_core(),
            _ => true,
        })
        .filter(|i| wanted.as_ref().is_none_or(|w| w.contains(&i.symbol)))
        .filter(|i| needle.as_ref().is_none_or(|n| i.symbol.to_lowercase().contains(n) || i.name.as_deref().is_some_and(|m| m.to_lowercase().contains(n))))
        .collect();
    Json(json!(list))
}

/// Provider subscriptions: plan limits, what streams per market, and the demand that did not fit.
async fn streaming(State(s): State<AppState>) -> Json<Value> {
    let now = std::time::Instant::now();
    let streaming = s.market.streaming();
    let (limits, wanted) = {
        let d = s.market.demand.lock().unwrap();
        (d.limits.clone(), d.wanted(now))
    };
    let mut by_market: std::collections::BTreeMap<String, Vec<&String>> = Default::default();
    for x in &streaming {
        if let Some(i) = s.market.cat.get(x) {
            by_market.entry(i.provider.market.clone()).or_default().push(x);
        }
    }
    let mut tiers: std::collections::BTreeMap<String, usize> = Default::default();
    for t in wanted.values() {
        *tiers.entry(format!("{t:?}").to_lowercase()).or_default() += 1;
    }
    let waiting: Vec<(&String, &Tier)> = wanted.iter().filter(|(k, t)| **t > Tier::Grace && !streaming.contains(*k)).collect();
    Json(json!({
        "limits": limits,
        "streaming": streaming.len(),
        "markets": by_market,
        "wanted_by_tier": tiers,
        "over_limit": waiting,
    }))
}

#[derive(Deserialize)]
struct QuotesQ {
    symbols: Option<String>,
    group: Option<String>,
}

async fn quotes(State(s): State<AppState>, Query(q): Query<QuotesQ>) -> Json<Value> {
    let group = q.group.unwrap_or_else(|| "raw".into());
    let wanted: Option<HashSet<String>> = q.symbols.map(|x| x.split(',').map(|s| s.trim().to_string()).collect());
    // asked-for symbols without any price yet (catalogue symbols nobody streamed): fetch one, wait briefly
    if let Some(w) = &wanted {
        let missing: Vec<String> = w.iter().filter(|x| s.market.cat.get(x).is_some() && s.market.quote(x).is_none()).take(500).cloned().collect();
        if !missing.is_empty() && s.market.request_history(HistoryJob::Snapshot { symbols: missing.clone() }) {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < until && missing.iter().any(|x| s.market.quote(x).is_none()) {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }
    let mut out = serde_json::Map::new();
    for inst in &s.market.cat.list {
        if wanted.as_ref().is_some_and(|w| !w.contains(&inst.symbol)) {
            continue;
        }
        if let Some(qt) = s.market.quote(&inst.symbol) {
            let q = s.market.spreads.apply(&group, inst, qt);
            // today's (server-day) open/high/low from the raw D1 bar, for % change and day range
            let day = s.market.day_stats(inst);
            let mut v = json!({"bid": q.bid, "ask": q.ask, "last": q.last, "t": q.t,
                       "o": day.map(|d| d.0), "h": day.map(|d| d.1), "l": day.map(|d| d.2)});
            if qt.delayed {
                v["d"] = json!(true);
            }
            out.insert(inst.symbol.clone(), v);
        }
    }
    Json(Value::Object(out))
}

#[derive(Deserialize)]
struct CandlesQ {
    symbol: String,
    tf: String,
    limit: Option<i64>,
    /// unix seconds (inclusive) — page back through history
    to: Option<i64>,
}

async fn candles(State(s): State<AppState>, Query(q): Query<CandlesQ>) -> Result<Response, ApiError> {
    let Some(tf) = Tf::parse(&q.tf) else { return Ok(bad("tf must be one of M1 M5 M15 M30 H1 H4 D1 W1 MN")) };
    let Some(inst) = s.market.cat.get(&q.symbol) else { return Ok(bad("unknown symbol")) };
    let limit = q.limit.unwrap_or(500).clamp(1, 5000);
    let to: Option<DateTime<Utc>> = q.to.and_then(|t| DateTime::from_timestamp(t, 0));
    let mut bars = db::load_bars(&s.market.pool, &q.symbol, tf.minutes(), to, None, limit).await?;
    // first chart of a catalogue symbol (or one whose history stopped): fetch from the provider, wait up to 8 s
    if to.is_none() && !inst.is_core() && bars.last().is_none_or(|b| b.t < tf.bucket(Utc::now()) - chrono::Duration::days(7).max(chrono::Duration::minutes(tf.minutes() as i64 * 50))) {
        if s.market.request_history(HistoryJob::Recent { symbol: q.symbol.clone(), first: Some(tf) }) && bars.is_empty() {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
            while std::time::Instant::now() < until {
                tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                bars = db::load_bars(&s.market.pool, &q.symbol, tf.minutes(), None, None, limit).await?;
                if !bars.is_empty() {
                    break;
                }
            }
        }
    }
    // merge the live forming bar (newest data) when serving the latest page
    if to.is_none() {
        if let Some(f) = s.market.forming(&q.symbol, tf) {
            match bars.last_mut() {
                Some(last) if last.t == f.t => {
                    last.h = last.h.max(f.h);
                    last.l = last.l.min(f.l);
                    last.c = f.c;
                    last.v = last.v.max(f.v);
                }
                Some(last) if last.t > f.t => {}
                _ => bars.push(f),
            }
        }
    }
    // provider history can carry more decimals than the symbol trades with
    let bars: Vec<_> = bars.into_iter().map(|b| inst.round_bar(b)).collect();
    Ok(Json(json!({"symbol": q.symbol, "tf": tf.name(), "digits": inst.digits, "bars": bars})).into_response())
}

async fn history_status(State(s): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = db::bar_counts(&s.market.pool).await?;
    let out: Vec<Value> = rows
        .into_iter()
        .map(|(sym, tf, n, first, last)| json!({"symbol": sym, "tf": Tf::from_minutes(tf).map(|t| t.name()).unwrap_or("?"), "bars": n, "from": first, "to": last}))
        .collect();
    Ok(Json(json!(out)))
}

fn authorized(s: &AppState, h: &HeaderMap) -> bool {
    !s.admin_token.is_empty() && h.get("authorization").and_then(|v| v.to_str().ok()) == Some(&format!("Bearer {}", s.admin_token))
}

async fn get_spreads(State(s): State<AppState>, h: HeaderMap) -> Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!(s.market.spreads.all())).into_response()
}

async fn put_spread(State(s): State<AppState>, h: HeaderMap, Json(m): Json<Markup>) -> Result<Response, ApiError> {
    if !authorized(&s, &h) {
        return Ok(StatusCode::UNAUTHORIZED.into_response());
    }
    if m.symbol != "*" && s.market.cat.get(&m.symbol).is_none() {
        return Ok(bad("unknown symbol"));
    }
    if let Err(e) = m.check() {
        return Ok(bad(e));
    }
    s.market.spreads.upsert(&s.market.pool, &m).await?;
    tracing::info!(group = %m.group_code, symbol = %m.symbol, markup = m.markup_points, "spread markup updated");
    Ok(Json(json!({"ok": true})).into_response())
}

#[derive(Deserialize)]
struct FactorsQ {
    symbol: String,
    /// YYYYMMDD
    from: String,
    to: String,
}

/// Provider adjustment factors of a stock (trading engine: corporate-action cross-check). Admin token; the provider
/// key stays here.
async fn adjustment_factors(State(s): State<AppState>, h: HeaderMap, Query(q): Query<FactorsQ>) -> Response {
    if !authorized(&s, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(inst) = s.market.cat.get(&q.symbol) else { return bad("unknown symbol") };
    let code = inst.provider.code.clone();
    let market = match code.rsplit('.').next() {
        Some("US") => "US",
        Some("HK") => "HK",
        Some("JP") => "JP",
        _ => return bad("not a stock"),
    };
    let ok = |d: &str| d.len() == 8 && d.chars().all(|c| c.is_ascii_digit());
    if !ok(&q.from) || !ok(&q.to) {
        return bad("from / to: YYYYMMDD");
    }
    let Some(pv) = s.market.provider.get() else {
        return (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "no provider connection (relay mode)"}))).into_response();
    };
    match pv.adjustment_factors(&code, market, &q.from, &q.to).await {
        Ok(f) => Json(json!({"symbol": q.symbol, "code": code, "factors": f.into_iter().map(|(d, x)| json!({"date": d, "factor": x})).collect::<Vec<_>>()})).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, Json(json!({"error": e.to_string()}))).into_response(),
    }
}

#[derive(Deserialize)]
struct DepthQ {
    symbol: String,
    group: Option<String>,
    levels: Option<usize>,
}

/// Depth of `symbol` for `group` (see `depth.rs`).
fn depth_for(s: &AppState, group: &str, symbol: &str, levels: usize) -> Option<depth::Depth> {
    let inst = s.market.cat.get(symbol)?;
    let raw = s.market.quote(symbol)?;
    let client = s.market.spreads.apply(group, inst, raw);
    Some(depth::build(inst, &raw, &client, s.market.feed_depth(symbol).as_ref(), levels))
}

async fn depth_rest(State(s): State<AppState>, Query(q): Query<DepthQ>) -> Response {
    let group = q.group.unwrap_or_else(|| "raw".into());
    match depth_for(&s, &group, &q.symbol, q.levels.unwrap_or(depth::DEFAULT_LEVELS)) {
        Some(d) => Json(json!({"symbol": q.symbol, "src": d.src, "t": d.t, "bids": d.bids, "asks": d.asks})).into_response(),
        None => bad("unknown symbol or no price yet"),
    }
}

#[derive(Deserialize)]
struct StreamQ {
    group: Option<String>,
}

async fn stream(State(s): State<AppState>, Query(q): Query<StreamQ>, ws: WebSocketUpgrade) -> Response {
    let group = q.group.unwrap_or_else(|| "raw".into());
    ws.on_upgrade(move |socket| client(socket, s, group))
}

/// One quote frame. `t` = provider event time, `r` = when this service received it (both ms); `"d":1` = delayed
/// (the symbol is not streamed: a last price to show, never to trade on).
fn quote_frame(symbol: &str, q: &Quote) -> String {
    if q.delayed {
        format!(r#"{{"type":"quote","s":"{symbol}","b":{},"a":{},"l":{},"t":{},"r":{},"d":1}}"#, q.bid, q.ask, q.last, q.t, q.recv)
    } else {
        format!(r#"{{"type":"quote","s":"{symbol}","b":{},"a":{},"l":{},"t":{},"r":{}}}"#, q.bid, q.ask, q.last, q.t, q.recv)
    }
}

fn bar_frame(symbol: &str, tf: Tf, b: &Bar) -> String {
    format!(r#"{{"type":"bar","s":"{symbol}","tf":"{}","t":{},"o":{},"h":{},"l":{},"c":{},"v":{}}}"#, tf.name(), b.t.timestamp(), b.o, b.h, b.l, b.c, b.v)
}

/// Current quotes of `symbols` for `group` (snapshot after subscribe or after falling behind).
fn snapshot(s: &AppState, group: &str, symbols: impl IntoIterator<Item = String>) -> Vec<String> {
    symbols
        .into_iter()
        .filter_map(|x| {
            let inst = s.market.cat.get(&x)?;
            let q = s.market.spreads.apply(group, inst, s.market.quote(&x)?);
            Some(quote_frame(&x, &q))
        })
        .collect()
}

/// Symbols wanted together with `symbol`: the pair converting its profit currency to USD (an order ticket on
/// XAUTHB needs USDTHB to price margin and P&L).
fn with_conversion(s: &AppState, symbol: &str) -> Vec<String> {
    let mut v = vec![symbol.to_string()];
    if let Some(p) = s.market.cat.get(symbol).and_then(|i| i.quote_ccy.as_deref()).filter(|c| *c != "USD").and_then(|c| s.market.cat.usd_pair(c)) {
        v.push(p);
    }
    v
}

async fn client(mut socket: WebSocket, s: AppState, group: String) {
    let mut rx = s.market.tx.subscribe();
    // this connection's references on the provider stream, released however the connection ends
    let mut demand = ClientDemand::new(s.market.demand.clone(), s.market.demand_changed.clone());
    let mut syms: HashSet<String> = HashSet::new();
    let mut bars: HashSet<(String, Tf)> = HashSet::new();
    // depth-of-market subscriptions: levels per client; `feed_only` = provider depth only (relays)
    let mut depths: HashSet<String> = HashSet::new();
    let mut depth_levels = depth::DEFAULT_LEVELS;
    let mut feed_only = false;
    // application heartbeat: lets the browser detect a dead connection (it cannot see WS pings)
    let mut hb = tokio::time::interval(std::time::Duration::from_secs(5));
    hb.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = hb.tick() => {
                let frame = format!(r#"{{"type":"hb","t":{}}}"#, Utc::now().timestamp_millis());
                if socket.send(Message::text(frame)).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                let Some(Ok(msg)) = incoming else { break };
                let Message::Text(text) = msg else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
                match v["op"].as_str() {
                    Some("subscribe") => {
                        let passive = v["passive"].as_bool().unwrap_or(false);
                        let mut added = Vec::new();
                        for x in v["symbols"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                            if s.market.cat.get(x).is_none() {
                                continue;
                            }
                            if syms.insert(x.to_string()) {
                                added.push(x.to_string());
                            }
                            // passive: deliver what streams, ask for nothing; re-subscribing switches the mode
                            let watched = demand.holds(x, Tier::Watch);
                            if !passive && !watched {
                                for y in with_conversion(&s, x) {
                                    demand.add(&y, Tier::Watch);
                                }
                            } else if passive && watched {
                                for y in with_conversion(&s, x) {
                                    demand.remove(&y, Tier::Watch);
                                }
                            }
                        }
                        // a symbol without any price yet gets a delayed one fetched (it streams if the plan has room)
                        if !passive {
                            let missing: Vec<String> = added.iter().filter(|x| s.market.quote(x).is_none()).cloned().collect();
                            if !missing.is_empty() {
                                s.market.request_history(HistoryJob::Snapshot { symbols: missing });
                            }
                        }
                        // snapshot so the client renders immediately (one flush for all of it)
                        if send_all(&mut socket, snapshot(&s, &group, added)).await.is_err() {
                            return;
                        }
                    }
                    Some("unsubscribe") => {
                        for x in v["symbols"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                            syms.remove(x);
                            if demand.holds(x, Tier::Watch) {
                                for y in with_conversion(&s, x) {
                                    demand.remove(&y, Tier::Watch);
                                }
                            }
                        }
                    }
                    Some("hold") => {
                        // the trading engine's positions and pending orders (+ conversion pairs): the whole set
                        let set: std::collections::BTreeSet<String> = v["symbols"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|x| s.market.cat.get(x).is_some()).map(str::to_string).collect();
                        demand.replace(Tier::Hold, &set);
                    }
                    Some("bars") => {
                        if let (Some(sym), Some(tf)) = (v["symbol"].as_str(), v["tf"].as_str().and_then(Tf::parse))
                            && s.market.cat.get(sym).is_some()
                            && bars.insert((sym.to_string(), tf))
                        {
                            // an open chart: the symbol streams with priority over watchlists
                            demand.add(sym, Tier::Focus);
                        }
                    }
                    Some("depth") => {
                        depth_levels = v["levels"].as_u64().map(|n| n as usize).unwrap_or(depth_levels).clamp(1, depth::MAX_LEVELS);
                        feed_only = v["src"] == "feed";
                        let mut added = Vec::new();
                        for x in v["symbols"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                            if s.market.cat.get(x).is_some() && depths.insert(x.to_string()) {
                                added.push(x.to_string());
                                // a depth ladder is a focus, except a relay's provider-depth mirror
                                if !feed_only {
                                    demand.add(x, Tier::Focus);
                                }
                            }
                        }
                        for x in added {
                            if let Some(d) = depth_for(&s, &group, &x, depth_levels).filter(|d| !feed_only || d.src == "feed")
                                && socket.send(Message::text(depth::frame(&x, &d))).await.is_err()
                            {
                                return;
                            }
                        }
                    }
                    Some("undepth") => {
                        for x in v["symbols"].as_array().into_iter().flatten().filter_map(Value::as_str) {
                            if depths.remove(x) {
                                demand.remove(x, Tier::Focus);
                            }
                        }
                    }
                    Some("unbars") => {
                        if let (Some(sym), Some(tf)) = (v["symbol"].as_str(), v["tf"].as_str().and_then(Tf::parse))
                            && bars.remove(&(sym.to_string(), tf))
                        {
                            demand.remove(sym, Tier::Focus);
                        }
                    }
                    _ => {}
                }
            }
            ev = rx.recv() => {
                // Every frame goes out at once: take this event plus whatever else is already queued (a burst, or a
                // client that fell behind) and write them with one flush instead of one syscall per frame. Nothing
                // waits for more events, so a single quote is never delayed.
                let sub = Subs { syms: &syms, bars: &bars, depths: &depths, depth_levels, feed_only };
                let mut out = Vec::new();
                let mut next = ev;
                for _ in 0..MAX_BATCH {
                    match next {
                        Ok(ev) => frames(&s, &group, &sub, ev, &mut out),
                        Err(RecvError::Lagged(n)) => {
                            // this client fell behind: skip the backlog and resend the current prices
                            tracing::debug!(skipped = n, "stream client lagged");
                            out.extend(snapshot(&s, &group, syms.iter().cloned()));
                        }
                        Err(RecvError::Closed) => return,
                    }
                    next = match rx.try_recv() {
                        Ok(ev) => Ok(ev),
                        Err(TryRecvError::Lagged(n)) => Err(RecvError::Lagged(n)),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Closed) => Err(RecvError::Closed),
                    };
                }
                if send_all(&mut socket, out).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// Upper bound of events written per flush (keeps heartbeats and client commands responsive under load).
const MAX_BATCH: usize = 256;

/// What one stream client is subscribed to.
struct Subs<'a> {
    syms: &'a HashSet<String>,
    bars: &'a HashSet<(String, Tf)>,
    depths: &'a HashSet<String>,
    depth_levels: usize,
    feed_only: bool,
}

/// The frames `ev` produces for a client (none when it isn't subscribed to it).
fn frames(s: &AppState, group: &str, sub: &Subs, ev: Event, out: &mut Vec<String>) {
    match ev {
        Event::Quote { symbol, quote } => {
            let Some(inst) = s.market.cat.get(&symbol) else { return };
            // the ladder follows every quote change of a symbol it is open for
            if sub.depths.contains(&*symbol) {
                let client = s.market.spreads.apply(group, inst, quote);
                let d = depth::build(inst, &quote, &client, s.market.feed_depth(&symbol).as_ref(), sub.depth_levels);
                if !sub.feed_only || d.src == "feed" {
                    out.push(depth::frame(&symbol, &d));
                }
            }
            if sub.syms.contains(&*symbol) {
                out.push(quote_frame(&symbol, &s.market.spreads.apply(group, inst, quote)));
            }
        }
        Event::Bars { symbol, bars } => {
            if sub.bars.is_empty() {
                return;
            }
            let Some(inst) = s.market.cat.get(&symbol) else { return };
            for (tf, bar) in bars.iter() {
                if sub.bars.iter().any(|(b, t)| t == tf && **b == *symbol) {
                    out.push(bar_frame(&symbol, *tf, &inst.round_bar(*bar)));
                }
            }
        }
    }
}

/// Writes `frames` and flushes once.
async fn send_all(socket: &mut WebSocket, frames: Vec<String>) -> Result<(), axum::Error> {
    if frames.is_empty() {
        return Ok(());
    }
    for f in frames {
        socket.feed(Message::text(f)).await?;
    }
    socket.flush().await
}

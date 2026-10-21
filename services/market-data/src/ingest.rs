//! Live provider ingest: one WebSocket per Infoway market (common / crypto / stock).
//! Subscribes to trades (10000 → pushes 10002) and depth (10003 → pushes 10005), sends the
//! 10010 heartbeat every 20s and reconnects with backoff (`Backoff`: reset only after a connection held 60 s).
//!
//! The provider counts connections per key, and keeps counting one that vanished without a close handshake until
//! it times out; reconnecting meanwhile is refused with HTTP 429. So: on SIGTERM / SIGINT every connection
//! unsubscribes and closes properly (`shutdown`, at most 3 s each) before the process exits, a 429 at connect backs
//! off 15 s → 300 s with jitter, alerting once per episode (`/health` `provider_refused_since`), and at start the
//! markets connect one at a time, `CONNECT_STAGGER` apart, never all at once.
//!
//! What each connection subscribes to follows the planner (demand.rs): the symbols someone needs, within the plan's
//! limit. A plan change is applied on the open connection (`apply_plan`); a market with nothing to stream does not
//! connect at all.

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};

use crate::config::Config;
use crate::state::{HistoryJob, Market};

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// Provider codes to subscribe per market (the planner's output).
pub type Plans = BTreeMap<String, watch::Receiver<Arc<BTreeSet<String>>>>;

/// Shutdown signal: true once the process is stopping.
pub type Stop = watch::Receiver<bool>;

/// Starts the planner and one connection task per provider market; the returned tasks finish once `stop` turns
/// true and every connection has unsubscribed and closed.
pub fn spawn_all(cfg: &Config, market: Arc<Market>, stop: Stop) -> Vec<tokio::task::JoinHandle<()>> {
    let plans = spawn_planner(market.clone());
    if !cfg.upstream.is_empty() {
        let (url, mk, stall) = (cfg.upstream.clone(), market.clone(), Duration::from_secs(cfg.upstream_stall_secs));
        let all = plans.values().cloned().collect::<Vec<_>>();
        return vec![tokio::spawn(async move { relay(url, mk, all, stall, stop).await })];
    }
    plans
        .into_iter()
        .enumerate()
        .map(|(i, (m, plan))| {
            let url = format!("{}?business={}&apikey={}", cfg.infoway_ws, m, cfg.infoway_key);
            let (mk, st) = (market.clone(), stop.clone());
            // gentle start: one market at a time
            let first = CONNECT_STAGGER * i as u32;
            tokio::spawn(async move { run(m, url, plan, mk, st, first).await })
        })
        .collect()
}

/// Delay between the first connections of the markets at start.
pub const CONNECT_STAGGER: Duration = Duration::from_secs(5);

/// Recomputes the plan whenever demand changes (and every 5 s for grace periods), publishes each market's
/// provider codes, and asks for history of catalogue symbols that start streaming (their bars have a gap).
fn spawn_planner(market: Arc<Market>) -> Plans {
    let mut txs: BTreeMap<String, watch::Sender<Arc<BTreeSet<String>>>> = BTreeMap::new();
    let mut rxs: Plans = BTreeMap::new();
    for m in market.cat.markets() {
        let (tx, rx) = watch::channel(Arc::new(BTreeSet::new()));
        txs.insert(m.clone(), tx);
        rxs.insert(m, rx);
    }
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(5));
        let mut last_dropped = 0usize;
        loop {
            tokio::select! {
                _ = market.demand_changed.notified() => {
                    // coalesce a burst of subscribe / unsubscribe messages into one plan change
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                _ = tick.tick() => {}
            }
            let before = market.streaming();
            let plan = market.demand.lock().unwrap().plan(&market.cat, &before, std::time::Instant::now());
            let after = plan.symbols();
            if after != before {
                let added: Vec<&String> = after.difference(&before).collect();
                let removed = before.difference(&after).count();
                tracing::info!(streaming = after.len(), added = added.len(), removed, "provider subscriptions changed");
                market.set_streaming(after.clone());
                for s in added {
                    if market.cat.get(s).is_some_and(|i| !i.is_core()) {
                        market.request_history(HistoryJob::Recent { symbol: s.clone(), first: None });
                    }
                }
            }
            if plan.dropped.len() != last_dropped {
                if !plan.dropped.is_empty() {
                    tracing::warn!(over_limit = plan.dropped.len(), first = ?plan.dropped.iter().take(5).collect::<Vec<_>>(), "demand exceeds the plan's symbol limit: lowest-priority symbols get delayed prices");
                }
                last_dropped = plan.dropped.len();
            }
            for (m, tx) in &txs {
                let codes: BTreeSet<String> = plan.markets.get(m).into_iter().flatten().filter_map(|s| market.cat.get(s)).map(|i| i.provider.code.clone()).collect();
                if **tx.borrow() != codes {
                    let _ = tx.send(Arc::new(codes));
                }
            }
        }
    });
    rxs
}

/// Provider frames for moving a connection's subscription from `current` to `want` (codes):
/// unsubscribe what goes first (11000 trades, 11001 depth), so the plan's symbol count is never exceeded even for
/// a moment, then subscribe the whole wanted list (10000 trades, 10003 depth). Re-sending codes already subscribed
/// is harmless whether the provider adds to the list or replaces it (its docs don't say; its SDK adds), so this is
/// correct either way. At most four frames per change: the provider allows 60 frames a minute per connection.
pub fn subscription_messages(business: &str, current: &BTreeSet<String>, want: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    if current == want {
        return out;
    }
    let removed: Vec<&str> = current.difference(want).map(String::as_str).collect();
    if !removed.is_empty() {
        let codes = removed.join(",");
        out.push(json!({"code": 11000, "trace": format!("kalks-{business}-untrade"), "data": {"codes": codes}}).to_string());
        out.push(json!({"code": 11001, "trace": format!("kalks-{business}-undepth"), "data": {"codes": codes}}).to_string());
    }
    if !want.is_empty() && want.difference(current).next().is_some() {
        let codes = want.iter().map(String::as_str).collect::<Vec<_>>().join(",");
        out.push(json!({"code": 10000, "trace": format!("kalks-{business}-trade"), "data": {"codes": codes}}).to_string());
        out.push(json!({"code": 10003, "trace": format!("kalks-{business}-depth"), "data": {"codes": codes}}).to_string());
    }
    out
}

/// Plan changes are applied to a connection at most this often (frames are rate-limited by the provider).
const PLAN_MIN_INTERVAL: Duration = Duration::from_secs(10);

/// A connection must hold this long before the reconnect backoff starts over.
const HEALTHY_SESSION: Duration = Duration::from_secs(60);

/// How a session ended (decides the reconnect delay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The provider refused the connection (HTTP 429: too many connections for the key).
    Refused,
    /// Any other error or a close.
    Dropped,
}

/// Reconnect delays. A 429 starts at 15 s and doubles to 300 s; other drops start at 1 s and double to 30 s (or follow
/// the 429 schedule while a refusal episode lasts). ±20 % jitter. Only a connection that held for 60 s starts over.
#[derive(Debug, Default)]
pub struct Backoff {
    attempt: u32,
    /// first refusal of the current episode
    pub refused_since: Option<chrono::DateTime<chrono::Utc>>,
}

impl Backoff {
    pub const REFUSED_BASE: Duration = Duration::from_secs(15);
    pub const REFUSED_CAP: Duration = Duration::from_secs(300);
    pub const ERROR_BASE: Duration = Duration::from_secs(1);
    pub const ERROR_CAP: Duration = Duration::from_secs(30);

    /// The delay before the next attempt after a session that ended with `outcome` having held `held`; `jitter` in
    /// [0, 1) (0.5 = no jitter). Returns the delay and whether a refusal episode just started (alert once).
    pub fn next(&mut self, outcome: Outcome, held: Duration, jitter: f64, now: chrono::DateTime<chrono::Utc>) -> (Duration, bool) {
        if held >= HEALTHY_SESSION {
            self.attempt = 0;
            self.refused_since = None;
        }
        self.attempt = self.attempt.saturating_add(1);
        let mut new_episode = false;
        if outcome == Outcome::Refused && self.refused_since.is_none() {
            self.refused_since = Some(now);
            new_episode = true;
        }
        let (base, cap) = if self.refused_since.is_some() { (Self::REFUSED_BASE, Self::REFUSED_CAP) } else { (Self::ERROR_BASE, Self::ERROR_CAP) };
        let raw = base.saturating_mul(1u32 << (self.attempt - 1).min(16)).min(cap);
        let factor = 0.8 + 0.4 * jitter.clamp(0.0, 1.0);
        (raw.mul_f64(factor), new_episode)
    }
}

/// Is `e` the provider refusing the connection (HTTP 429 at the WebSocket handshake)?
pub fn is_refused(e: &anyhow::Error) -> bool {
    use tokio_tungstenite::tungstenite::Error;
    matches!(e.downcast_ref::<Error>(), Some(Error::Http(r)) if r.status().as_u16() == 429)
}

/// A cheap jitter in [0, 1) (no RNG dependency).
fn jitter() -> f64 {
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    (n.wrapping_mul(2_654_435_761) % 1_000_000) as f64 / 1_000_000.0
}

/// Waits `d` unless the process stops first (true = stopping).
async fn sleep_or_stop(d: Duration, stop: &mut Stop) -> bool {
    if *stop.borrow() {
        return true;
    }
    tokio::select! {
        _ = tokio::time::sleep(d) => *stop.borrow(),
        _ = stop.changed() => true,
    }
}

async fn run(business: String, url: String, mut plan: watch::Receiver<Arc<BTreeSet<String>>>, market: Arc<Market>, mut stop: Stop, first: Duration) {
    let mut backoff = Backoff::default();
    if sleep_or_stop(first, &mut stop).await {
        return;
    }
    loop {
        // nothing to stream on this market: stay disconnected until there is
        while plan.borrow_and_update().is_empty() {
            market.set_expected(&business, false);
            tokio::select! {
                changed = plan.changed() => if changed.is_err() { return },
                _ = stop.changed() => return,
            }
        }
        if *stop.borrow() {
            return;
        }
        market.set_expected(&business, true);
        let started = Instant::now();
        // run each session in its own task so a panic inside it becomes a reconnect, never a silent stop
        let (b, u, p, m, st) = (business.clone(), url.clone(), plan.clone(), market.clone(), stop.clone());
        let joined = tokio::spawn(async move { session(&b, &u, p, &m, st).await }).await;
        let res = match joined {
            Ok(v) => v,
            Err(e) => Err(anyhow::anyhow!("provider session task crashed: {e}")),
        };
        market.set_connected(&business, false);
        if *stop.borrow() {
            market.set_expected(&business, false);
            return;
        }
        let outcome = match &res {
            Err(e) if is_refused(e) => Outcome::Refused,
            _ => Outcome::Dropped,
        };
        let was_refused = backoff.refused_since.is_some();
        let now = chrono::Utc::now();
        let (delay, new_episode) = backoff.next(outcome, started.elapsed(), jitter(), now);
        if was_refused && backoff.refused_since.is_none() {
            tracing::info!(%business, "provider connection accepted again after refusals");
        }
        market.set_refused(&business, backoff.refused_since);
        if new_episode {
            tracing::error!(%business, since = %now.to_rfc3339(), "ALERT provider connection refused (429) business={business} since={}", now.to_rfc3339());
        }
        let secs = delay.as_secs_f64().round();
        match res {
            Ok(()) => tracing::warn!(%business, "provider stream closed; reconnecting in {secs}s"),
            Err(e) => tracing::warn!(%business, error = %e, "provider stream error; reconnecting in {secs}s"),
        }
        if sleep_or_stop(delay, &mut stop).await {
            market.set_expected(&business, false);
            return;
        }
    }
}

/// Unsubscribes everything and closes the connection with a close handshake (the provider then frees the slot at
/// once instead of counting a dead connection until it times out). At most `CLOSE_WAIT`.
pub async fn shutdown<S, R>(business: &str, tx: &mut S, rx: &mut R, current: &BTreeSet<String>) -> anyhow::Result<()>
where
    S: futures_util::Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
    R: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let work = async {
        for m in subscription_messages(business, current, &BTreeSet::new()) {
            tx.send(Message::text(m)).await?;
        }
        tx.send(Message::Close(Some(CloseFrame { code: CloseCode::Normal, reason: "kalks market-data shutting down".into() }))).await?;
        // the provider answers with its own close frame (or just drops the socket)
        while let Some(m) = rx.next().await {
            if matches!(m, Ok(Message::Close(_)) | Err(_)) {
                break;
            }
        }
        anyhow::Ok(())
    };
    match tokio::time::timeout(CLOSE_WAIT, work).await {
        Ok(r) => r,
        Err(_) => anyhow::bail!("no close handshake within {}s", CLOSE_WAIT.as_secs()),
    }
}

/// How long a connection may take to close on shutdown.
pub const CLOSE_WAIT: Duration = Duration::from_secs(3);

/// No frame at all for this long while the market is open = dead connection.
const IDLE_TIMEOUT: Duration = Duration::from_secs(25);
/// Crypto trades around the clock: this long without a single trade means the stream has silently stalled.
const CRYPTO_TRADE_TIMEOUT: Duration = Duration::from_secs(15);

async fn session(business: &str, url: &str, mut plan: watch::Receiver<Arc<BTreeSet<String>>>, market: &Arc<Market>, mut stop: Stop) -> anyhow::Result<()> {
    let (ws, _) = tokio::time::timeout(Duration::from_secs(10), connect_async(url)).await.map_err(|_| anyhow::anyhow!("connect timeout"))??;
    if let tokio_tungstenite::MaybeTlsStream::Rustls(s) = ws.get_ref() {
        let _ = s.get_ref().0.set_nodelay(true);
    }
    let (mut tx, mut rx) = ws.split();
    let mut current: BTreeSet<String> = BTreeSet::new();
    let want = plan.borrow_and_update().clone();
    for m in subscription_messages(business, &current, &want) {
        tx.send(Message::text(m)).await?;
    }
    current = (*want).clone();
    tracing::info!(%business, symbols = current.len(), "subscribed to provider stream");
    market.set_connected(business, true);

    // the provider times a connection out after 90 s without a frame and counts every frame (60 a minute)
    let mut heartbeat = tokio::time::interval(Duration::from_secs(20));
    let mut last_plan = Instant::now();
    heartbeat.tick().await;
    let mut watchdog = tokio::time::interval(Duration::from_secs(1));
    let (mut last_frame, mut last_trade) = (Instant::now(), Instant::now());
    loop {
        tokio::select! {
            _ = stop.changed() => {
                let r = shutdown(business, &mut tx, &mut rx, &current).await;
                match &r {
                    Ok(()) => tracing::info!(%business, "provider stream unsubscribed and closed"),
                    Err(e) => tracing::warn!(%business, error = %e, "provider stream close incomplete"),
                }
                return Ok(());
            }
            _ = heartbeat.tick() => {
                tx.send(Message::text(json!({"code": 10010, "trace": format!("kalks-{business}-hb")}).to_string())).await?;
            }
            _ = watchdog.tick() => {
                // the provider does not answer heartbeats, so silence only means "dead" while a symbol of this
                // stream is in its trading session (FX/indices are legitimately silent over the weekend)
                let now = chrono::Utc::now();
                let open = current.iter().filter_map(|c| market.cat.from_provider(business, c)).any(|i| i.in_session(now));
                if open && last_frame.elapsed() > IDLE_TIMEOUT {
                    anyhow::bail!("no data for {}s", IDLE_TIMEOUT.as_secs());
                }
                if business == "crypto" && !current.is_empty() && last_trade.elapsed() > CRYPTO_TRADE_TIMEOUT {
                    anyhow::bail!("no trades for {}s (stalled stream)", CRYPTO_TRADE_TIMEOUT.as_secs());
                }
            }
            changed = plan.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                // at most one subscription change per PLAN_MIN_INTERVAL (later changes merge into it)
                let since = last_plan.elapsed();
                if since < PLAN_MIN_INTERVAL {
                    tokio::time::sleep(PLAN_MIN_INTERVAL - since).await;
                }
                last_plan = Instant::now();
                let want = plan.borrow_and_update().clone();
                for m in subscription_messages(business, &current, &want) {
                    tx.send(Message::text(m)).await?;
                }
                let (added, removed) = (want.difference(&current).count(), current.difference(&want).count());
                current = (*want).clone();
                // a newly streamed symbol counts as trading from now (the crypto stall check)
                last_trade = Instant::now();
                tracing::info!(%business, symbols = current.len(), added, removed, "provider subscription updated");
            }
            msg = rx.next() => {
                let Some(msg) = msg else { return Ok(()) };
                last_frame = Instant::now();
                match msg? {
                    Message::Text(t) => {
                        if handle(business, &t, market) {
                            last_trade = last_frame;
                        }
                    }
                    Message::Ping(p) => tx.send(Message::Pong(p)).await?,
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
    }
}

/// Applies one provider frame; true when it was a trade.
fn handle(business: &str, text: &str, market: &Arc<Market>) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        // e.g. "Subscribe fail: ..." (plan limit): never retried as is; the next plan change sends a new list
        if text.starts_with("Subscribe fail") {
            tracing::error!(%business, frame = %text.chars().take(300).collect::<String>(), "provider refused a subscription");
        }
        return false;
    };
    let code = v.get("code").and_then(Value::as_i64).unwrap_or(0);
    let d = &v["data"];
    match code {
        10002 => {
            let (Some(code), Some(p)) = (d["s"].as_str(), num(&d["p"])) else { return false };
            let Some(inst) = market.cat.from_provider(business, code) else { return false };
            let t = d["t"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            market.on_trade(&inst.symbol, p, num(&d["v"]).unwrap_or(0.0), t);
            return true;
        }
        10005 => {
            let Some(code) = d["s"].as_str() else { return false };
            let Some(inst) = market.cat.from_provider(business, code) else { return false };
            let (Some(b), Some(a)) = (num(&d["b"][0][0]), num(&d["a"][0][0])) else { return false };
            let t = d["t"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
            if b > 0.0 && a >= b {
                market.on_book(&inst.symbol, b, a, t);
                // more than one priced level: keep the book for the depth-of-market ladder (D97)
                let levels = |v: &Value| v.as_array().map(|l| l.iter().filter_map(|x| Some((num(&x[0])?, num(&x[1])?))).collect::<Vec<_>>()).unwrap_or_default();
                let (bl, al) = (levels(&d["b"]), levels(&d["a"]));
                if bl.len() > 1 && al.len() > 1 {
                    market.on_depth(&inst.symbol, bl, al, t);
                }
            }
        }
        10001 | 10004 | 10011 | 11010 | 200 => {}
        _ => {
            if v.get("msg").is_some() && code != 0 {
                tracing::debug!(%business, %text, "provider message");
            }
        }
    }
    false
}

/// Relay: WebSocket ping to the upstream this often; its pong (and its own 5 s heartbeat) keep a healthy connection
/// from ever going silent.
const RELAY_PING_EVERY: Duration = Duration::from_secs(5);

/// True once the upstream has sent no frame at all (quote, heartbeat or pong) for longer than `limit`: a
/// half-dead connection (e.g. after sleep / wake) that never delivers a close frame or an error.
fn stalled(last_frame: Instant, now: Instant, limit: Duration) -> bool {
    now.saturating_duration_since(last_frame) > limit
}

/// Relay mode: mirror quotes from another Kalks market-data stream (`{"type":"quote","s","b","a","l","t"}`).
/// Every catalogue symbol is subscribed passively (whatever the upstream streams arrives here); the symbols wanted
/// locally (this service's own plan) are subscribed actively, so the upstream streams them too. `stall`: silence
/// after which the connection counts as dead and is reconnected.
async fn relay(url: String, market: Arc<Market>, mut plans: Vec<watch::Receiver<Arc<BTreeSet<String>>>>, stall: Duration, mut stop: Stop) {
    let mut backoff = 1u64;
    market.set_expected("relay", true);
    loop {
        if *stop.borrow() {
            return;
        }
        let mut stop_in = stop.clone();
        let symbols: Vec<String> = market.cat.list.iter().map(|i| i.symbol.clone()).collect();
        let res: anyhow::Result<()> = async {
            let (ws, _) = tokio::time::timeout(Duration::from_secs(15), connect_async(&url)).await??;
            let (mut tx, mut rx) = ws.split();
            tx.send(Message::text(json!({"op": "subscribe", "symbols": symbols, "passive": true}).to_string())).await?;
            // provider depth only (never the upstream's indicative ladders); upstreams without depth ignore it
            tx.send(Message::text(json!({"op": "depth", "symbols": symbols, "src": "feed"}).to_string())).await?;
            let mut active: BTreeSet<String> = market.streaming();
            if !active.is_empty() {
                tx.send(Message::text(json!({"op": "subscribe", "symbols": active}).to_string())).await?;
            }
            market.set_connected("relay", true);
            tracing::info!(%url, active = active.len(), "relay connected");
            backoff = 1;
            let mut check = tokio::time::interval(Duration::from_secs(1));
            let mut ping = tokio::time::interval(RELAY_PING_EVERY);
            ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            ping.tick().await;
            // a read timeout around rx.next() inside this select would restart on every 1 s check tick and never
            // fire, so silence is measured from the last frame instead
            let mut last_frame = Instant::now();
            loop {
                let msg = tokio::select! {
                    _ = stop_in.changed() => {
                        let _ = tx.send(Message::Close(None)).await;
                        return Ok(());
                    }
                    m = rx.next() => m,
                    _ = ping.tick() => {
                        // a send into a dead socket may hang once its buffer is full
                        tokio::time::timeout(RELAY_PING_EVERY, tx.send(Message::Ping(Vec::new().into()))).await.map_err(|_| anyhow::anyhow!("ping not sent within {}s", RELAY_PING_EVERY.as_secs()))??;
                        continue;
                    }
                    _ = check.tick() => {
                        if stalled(last_frame, Instant::now(), stall) {
                            anyhow::bail!("no frame from the upstream for {}s (stalled connection)", stall.as_secs());
                        }
                        // the local plan changed: upstream demand follows it
                        if plans.iter_mut().any(|p| p.has_changed().unwrap_or(false)) {
                            for p in plans.iter_mut() {
                                p.borrow_and_update();
                            }
                        }
                        let want = market.streaming();
                        if want != active {
                            let added: Vec<&String> = want.difference(&active).collect();
                            let removed: Vec<&String> = active.difference(&want).collect();
                            if !added.is_empty() {
                                tx.send(Message::text(json!({"op": "subscribe", "symbols": added}).to_string())).await?;
                            }
                            if !removed.is_empty() {
                                tx.send(Message::text(json!({"op": "subscribe", "symbols": removed, "passive": true}).to_string())).await?;
                            }
                            active = want;
                        }
                        continue;
                    }
                };
                let Some(msg) = msg else { return Ok(()) };
                last_frame = Instant::now();
                match msg? {
                    Message::Text(t) => {
                        let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                        if v["type"] == "depth" && v["src"] == "feed" {
                            if let Some(s) = v["s"].as_str() {
                                let levels = |x: &Value| x.as_array().map(|l| l.iter().filter_map(|p| Some((num(&p[0])?, num(&p[1])?))).collect::<Vec<_>>()).unwrap_or_default();
                                let t = v["t"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                                market.on_depth(s, levels(&v["b"]), levels(&v["a"]), t);
                            }
                            continue;
                        }
                        if v["type"] != "quote" { continue }
                        let (Some(s), Some(b), Some(a)) = (v["s"].as_str(), num(&v["b"]), num(&v["a"])) else { continue };
                        let t = v["t"].as_i64().unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                        let last = num(&v["l"]).unwrap_or((b + a) / 2.0);
                        if v["d"].as_i64().unwrap_or(0) != 0 {
                            // the upstream's delayed snapshot: a price to show, never a live tick (no bars from it)
                            market.set_snapshot(s, last, None, t);
                            continue;
                        }
                        market.on_book(s, b, a, t);
                        market.on_trade(s, last, 0.0, t);
                    }
                    Message::Ping(p) => tx.send(Message::Pong(p)).await?,
                    Message::Close(_) => return Ok(()),
                    _ => {}
                }
            }
        }
        .await;
        market.set_connected("relay", false);
        if let Err(e) = res {
            tracing::warn!(error = %e, "relay stream error; reconnecting in {backoff}s");
        }
        if sleep_or_stop(Duration::from_secs(backoff), &mut stop).await {
            return;
        }
        backoff = (backoff * 2).min(15);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    fn codes(frames: &[String]) -> Vec<(i64, String)> {
        frames.iter().map(|f| serde_json::from_str::<Value>(f).unwrap()).map(|v| (v["code"].as_i64().unwrap(), v["data"]["codes"].as_str().unwrap().to_string())).collect()
    }

    #[test]
    fn subscription_changes_unsubscribe_first_then_send_the_full_list() {
        // first connect: everything
        let f = subscription_messages("common", &BTreeSet::new(), &set(&["EURUSD", "AUDCAD"]));
        assert_eq!(codes(&f), vec![(10000, "AUDCAD,EURUSD".into()), (10003, "AUDCAD,EURUSD".into())]);
        // one leaves, one joins: unsubscribe the leaver, then the whole wanted list
        let f = subscription_messages("common", &set(&["EURUSD", "AUDCAD"]), &set(&["EURUSD", "USDSGD"]));
        assert_eq!(codes(&f), vec![(11000, "AUDCAD".into()), (11001, "AUDCAD".into()), (10000, "EURUSD,USDSGD".into()), (10003, "EURUSD,USDSGD".into())]);
        // only removals: no subscribe frames
        let f = subscription_messages("common", &set(&["EURUSD", "USDSGD"]), &set(&["EURUSD"]));
        assert_eq!(codes(&f).iter().map(|c| c.0).collect::<Vec<_>>(), vec![11000, 11001]);
        // nothing changed: nothing sent (every frame counts against the provider's 60 a minute)
        assert!(subscription_messages("common", &set(&["EURUSD"]), &set(&["EURUSD"])).is_empty());
    }

    #[test]
    fn relay_counts_a_silent_upstream_as_stalled() {
        let limit = Duration::from_secs(20);
        let t0 = Instant::now();
        // frames (quotes, heartbeats, pongs) within the limit: alive
        assert!(!stalled(t0, t0, limit));
        assert!(!stalled(t0, t0 + Duration::from_secs(20), limit));
        // nothing for longer than the limit (e.g. after sleep / wake): dead, reconnect
        assert!(stalled(t0, t0 + Duration::from_secs(21), limit));
        assert!(stalled(t0, t0 + Duration::from_secs(6000), limit));
        // a frame stamped after "now" (clock race) never counts as silence
        assert!(!stalled(t0 + Duration::from_secs(1), t0, limit));
    }
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2026, 10, 8, 18, 44, 0).unwrap()
    }

    #[test]
    fn refused_backoff_schedule_caps_and_resets_only_after_a_held_connection() {
        let mut b = Backoff::default();
        let short = Duration::from_secs(2);
        // 429s: 15, 30, 60, 120, 240, 300, 300 s (no jitter), the alert only on the first
        let mut delays = Vec::new();
        let mut alerts = 0;
        for _ in 0..7 {
            let (d, alert) = b.next(Outcome::Refused, Duration::ZERO, 0.5, t0());
            delays.push(d.as_secs());
            alerts += alert as u32;
        }
        assert_eq!(delays, vec![15, 30, 60, 120, 240, 300, 300]);
        assert_eq!(alerts, 1, "one alert per episode");
        assert_eq!(b.refused_since, Some(t0()));
        // a connection that drops after 2 s does not end the episode or reset the schedule
        let (d, _) = b.next(Outcome::Dropped, short, 0.5, t0());
        assert_eq!(d.as_secs(), 300);
        assert!(b.refused_since.is_some());
        // one that held 60 s does: back to the 1 s error schedule, and a new 429 starts a new episode (alert again)
        let (d, _) = b.next(Outcome::Dropped, Duration::from_secs(60), 0.5, t0());
        assert_eq!(d, Duration::from_secs(1));
        assert!(b.refused_since.is_none());
        let (d, alert) = b.next(Outcome::Refused, short, 0.5, t0());
        assert!(alert);
        assert_eq!(d.as_secs(), 30, "the second attempt of the run, now on the 429 schedule");
        // plain drops: 1, 2, 4 … capped at 30 s
        let mut e = Backoff::default();
        let v: Vec<u64> = (0..7).map(|_| e.next(Outcome::Dropped, short, 0.5, t0()).0.as_secs()).collect();
        assert_eq!(v, vec![1, 2, 4, 8, 16, 30, 30]);
        // jitter stays within ±20 %
        let mut j = Backoff::default();
        assert_eq!(j.next(Outcome::Refused, short, 0.0, t0()).0, Duration::from_secs(12));
        let mut j = Backoff::default();
        assert_eq!(j.next(Outcome::Refused, short, 0.999_999, t0()).0.as_secs(), 17);
        let x = jitter();
        assert!((0.0..1.0).contains(&x));
    }

    /// A provider stand-in: accepts one WebSocket, records every frame until the client closes.
    async fn mock_provider() -> (String, tokio::task::JoinHandle<Vec<Message>>) {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws?business=crypto", l.local_addr().unwrap());
        let h = tokio::spawn(async move {
            let (tcp, _) = l.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let mut got = Vec::new();
            while let Some(Ok(m)) = ws.next().await {
                let close = matches!(m, Message::Close(_));
                got.push(m);
                if close {
                    break; // tungstenite answers the close handshake itself
                }
            }
            got
        });
        (url, h)
    }

    fn market() -> Arc<Market> {
        let cat = Catalogue::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/instruments.json"), concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/holidays")).unwrap();
        let pool = sqlx::postgres::PgPoolOptions::new().connect_lazy("postgres://nobody@127.0.0.1:1/none").unwrap();
        Market::new(cat, pool, crate::spreads::Spreads::default(), false, crate::demand::Demand::new(Vec::<String>::new(), Default::default()))
    }

    use crate::instruments::Catalogue;

    #[tokio::test]
    async fn stop_unsubscribes_and_closes_with_a_handshake() {
        let (url, server) = mock_provider().await;
        let mk = market();
        let (_plan_tx, plan) = watch::channel(Arc::new(["BTCUSDT".to_string(), "BNBUSDT".to_string()].into_iter().collect::<BTreeSet<_>>()));
        let (stop_tx, stop) = watch::channel(false);
        let (m2, u) = (mk.clone(), url.clone());
        let task = tokio::spawn(async move { session("crypto", &u, plan, &m2, stop).await });
        // connected and subscribed
        for _ in 0..50 {
            if mk.stats().connected_markets.contains("crypto") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(mk.stats().connected_markets.contains("crypto"));
        let t = Instant::now();
        stop_tx.send(true).unwrap();
        task.await.unwrap().unwrap();
        assert!(t.elapsed() < CLOSE_WAIT, "closed promptly ({:?})", t.elapsed());
        let frames = server.await.unwrap();
        let codes: Vec<i64> = frames.iter().filter_map(|m| if let Message::Text(t) = m { serde_json::from_str::<Value>(t).ok()?["code"].as_i64() } else { None }).collect();
        assert_eq!(codes, vec![10000, 10003, 11000, 11001], "subscribe, then unsubscribe both on stop");
        let unsub: Vec<String> = frames.iter().filter_map(|m| if let Message::Text(t) = m { let v: Value = serde_json::from_str(t).ok()?; (v["code"] == 11000).then(|| v["data"]["codes"].as_str().unwrap().to_string()) } else { None }).collect();
        assert_eq!(unsub, vec!["BNBUSDT,BTCUSDT".to_string()]);
        assert!(matches!(frames.last(), Some(Message::Close(Some(f))) if f.code == CloseCode::Normal), "ends with a close frame: {:?}", frames.last());
    }

    #[tokio::test]
    async fn a_429_at_connect_is_recognised_and_backs_off() {
        // a provider that refuses the WebSocket handshake with 429
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws?business=crypto", l.local_addr().unwrap());
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut tcp, _) = l.accept().await.unwrap();
            let mut buf = [0u8; 2048];
            let _ = tcp.read(&mut buf).await;
            let _ = tcp.write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n").await;
        });
        let mk = market();
        let (_plan_tx, plan) = watch::channel(Arc::new(["BTCUSDT".to_string()].into_iter().collect::<BTreeSet<_>>()));
        let (_stop_tx, stop) = watch::channel(false);
        let e = session("crypto", &url, plan, &mk, stop).await.unwrap_err();
        assert!(is_refused(&e), "{e}");
        assert!(!is_refused(&anyhow::anyhow!("connect timeout")));
        let mut b = Backoff::default();
        let (d, alert) = b.next(Outcome::Refused, Duration::ZERO, 0.5, t0());
        assert!(alert && d == Backoff::REFUSED_BASE);
    }

    #[tokio::test]
    async fn health_reports_missing_and_refused_markets() {
        let mk = market();
        mk.set_expected("common", true);
        mk.set_expected("crypto", true);
        mk.set_connected("common", true);
        mk.set_refused("crypto", Some(t0()));
        let st = mk.stats();
        assert_eq!(st.missing(), vec!["crypto".to_string()]);
        assert!(st.healthy(chrono::Utc::now()), "missing for less than 5 minutes");
        assert!(!st.healthy(chrono::Utc::now() + chrono::Duration::minutes(6)), "missing for more than 5 minutes");
        assert_eq!(st.refused_since.get("crypto"), Some(&t0()));
        mk.set_connected("crypto", true);
        assert!(mk.stats().missing().is_empty());
        assert!(mk.stats().healthy(chrono::Utc::now() + chrono::Duration::minutes(6)));
        // a market with nothing to stream is not expected
        mk.set_connected("crypto", false);
        mk.set_expected("crypto", false);
        assert!(mk.stats().missing().is_empty());
    }
}

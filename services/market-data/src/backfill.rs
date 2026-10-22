//! History from the provider, rate-limited (gate.rs) and resumable.
//!
//! **Core instruments** (eager, as before): phase 1 stores the latest 500 bars of M1, M5, M15, M30, H1 and D1 so
//! charts work immediately; phase 2 pages back to the configured depth per timeframe, remembering progress in
//! `backfill_state`.
//! **Catalogue instruments** (lazy): history is fetched the first time someone opens a chart (the timeframe they
//! wait for first), and again when a symbol starts streaming after a pause (the gap heals itself); deep history then
//! follows in the background. Symbols nobody streams get a delayed price snapshot (`Snapshot`) on request and from a
//! periodic sweep, so every catalogue symbol has a quote.
//! H4 and D1 are aggregated from our H1 bars at New York close (W1/MN from D1). Before our H1 coverage, provider
//! daily bars (UTC days) are used for D1, labelled with the matching server day.

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex as StdMutex};

use crate::config::Config;
use crate::db::{self, Bar, Source};
use crate::gate::{Gate, Prio};
use crate::instruments::Instrument;
use crate::state::{HistoryJob, Market};
use crate::timeframes::Tf;

pub struct Provider {
    http: reqwest::Client,
    base: String,
    key: String,
    pub gate: Gate,
    /// Codes per batch request.
    batch: usize,
}

impl Provider {
    /// Up to 500 bars ending at `before` (or the latest when None), ascending.
    async fn klines(&self, prio: Prio, inst: &Instrument, kline: i32, before: Option<DateTime<Utc>>) -> anyhow::Result<Vec<Bar>> {
        self.gate.acquire(prio).await;
        let mut body = json!({"klineType": kline, "klineNum": 500, "codes": inst.provider.code});
        if let Some(b) = before {
            body["timestamp"] = json!(b.timestamp());
        }
        let url = format!("{}/{}/v2/batch_kline", self.base, inst.provider.market);
        let v: Value = self.http.post(&url).header("apiKey", &self.key).json(&body).send().await?.json().await?;
        let ret = v["ret"].as_i64().unwrap_or(0);
        if ret != 200 {
            anyhow::bail!("provider ret {ret}: {}", v["msg"].as_str().unwrap_or(""));
        }
        let mut out = parse_bars(&v["data"][0]["respList"]);
        out.sort_by_key(|b| b.t);
        out.dedup_by_key(|b| b.t);
        Ok(out)
    }

    /// Provider forward adjustment factors of one stock per trading day (`/common/basic/symbols/adjustment_factors`):
    /// the corporate-action cross-check of the trading engine. `market` = US / HK / JP; days as YYYYMMDD.
    pub async fn adjustment_factors(&self, code: &str, market: &str, from: &str, to: &str) -> anyhow::Result<Vec<(String, f64)>> {
        self.gate.acquire(Prio::Interactive).await;
        let url = reqwest::Url::parse_with_params(&format!("{}/common/basic/symbols/adjustment_factors", self.base), &[("symbol", code), ("market", market), ("beginDay", from), ("endDay", to)])?;
        let v: Value = self.http.get(url).header("apiKey", &self.key).send().await.map_err(|e| anyhow::anyhow!(e.without_url().to_string()))?.json().await.map_err(|e| anyhow::anyhow!(e.without_url().to_string()))?;
        let ret = v["ret"].as_i64().unwrap_or(0);
        if ret != 200 {
            anyhow::bail!("provider ret {ret}: {}", v["msg"].as_str().unwrap_or(""));
        }
        Ok(v["data"].as_array().into_iter().flatten().filter_map(|r| Some((r["trade_date"].as_str()?.to_string(), r["forward_factor"].as_f64()?))).collect())
    }

    /// Latest `n` bars for the codes of one provider market, `batch` codes per request.
    async fn latest_batch(&self, prio: Prio, market: &str, codes: &[String], kline: i32, n: i32) -> anyhow::Result<Vec<(String, Vec<Bar>)>> {
        let mut out = Vec::new();
        for chunk in codes.chunks(self.batch.max(1)) {
            out.extend(self.latest_one(prio, market, chunk, kline, n).await?);
        }
        Ok(out)
    }

    async fn latest_one(&self, prio: Prio, market: &str, codes: &[String], kline: i32, n: i32) -> anyhow::Result<Vec<(String, Vec<Bar>)>> {
        self.gate.acquire(prio).await;
        let body = json!({"klineType": kline, "klineNum": n, "codes": codes.join(",")});
        let url = format!("{}/{}/v2/batch_kline", self.base, market);
        let v: Value = self.http.post(&url).header("apiKey", &self.key).json(&body).send().await?.json().await?;
        let ret = v["ret"].as_i64().unwrap_or(0);
        if ret != 200 {
            anyhow::bail!("provider ret {ret}: {}", v["msg"].as_str().unwrap_or(""));
        }
        Ok(v["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|d| Some((d["s"].as_str()?.to_string(), parse_bars(&d["respList"]))))
            .collect())
    }
}

fn parse_bars(list: &Value) -> Vec<Bar> {
    let p = |x: &Value| x.as_str().and_then(|s| s.parse::<f64>().ok()).or_else(|| x.as_f64()).unwrap_or(0.0);
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let t = r["t"].as_str().and_then(|s| s.parse::<i64>().ok()).or_else(|| r["t"].as_i64())?;
                    Some(Bar { t: Utc.timestamp_opt(t, 0).single()?, o: p(&r["o"]), h: p(&r["h"]), l: p(&r["l"]), c: p(&r["c"]), v: p(&r["v"]) })
                })
                .filter(|b| b.o > 0.0 && b.h >= b.l)
                .collect()
        })
        .unwrap_or_default()
}

pub fn spawn(cfg: &Config, market: Arc<Market>) {
    let provider = Arc::new(Provider {
        http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build().expect("http client"),
        base: cfg.infoway_rest.clone(),
        key: cfg.infoway_key.clone(),
        gate: Gate::start(std::time::Duration::from_secs_f64(1.0 / cfg.provider_rps.max(0.1))),
        batch: cfg.batch_codes,
    });
    let _ = market.provider.set(provider.clone());
    {
        let (pv, mk) = (provider.clone(), market.clone());
        tokio::spawn(async move { reconcile_loop(&pv, &mk).await });
    }
    // lazy history of catalogue instruments and price snapshots
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let _ = market.history.set(tx);
    {
        let (pv, mk, cfg) = (provider.clone(), market.clone(), cfg.clone());
        tokio::spawn(async move { worker(cfg, pv, mk, rx).await });
    }
    if cfg.snapshot_every_secs > 0 {
        let (mk, every) = (market.clone(), cfg.snapshot_every_secs);
        tokio::spawn(async move { snapshot_sweep(mk, every).await });
    }
    let cfg = cfg.clone();
    tokio::spawn(async move {
        if let Err(e) = run(&cfg, &provider, &market).await {
            tracing::error!(error = %e, "backfill stopped");
        }
    });
}

/// How many history jobs run at once (they share the gate, so this only lets one symbol's job overtake another's).
const WORKERS: usize = 4;
/// A symbol's recent history is not fetched again within this long.
const RECENT_TTL: std::time::Duration = std::time::Duration::from_secs(120);

async fn worker(cfg: Config, pv: Arc<Provider>, mk: Arc<Market>, mut rx: tokio::sync::mpsc::UnboundedReceiver<HistoryJob>) {
    let busy: Arc<StdMutex<HashSet<String>>> = Default::default();
    let fresh: Arc<StdMutex<HashMap<String, std::time::Instant>>> = Default::default();
    let deep_started: Arc<StdMutex<HashSet<String>>> = Default::default();
    let slots = Arc::new(tokio::sync::Semaphore::new(WORKERS));
    while let Some(job) = rx.recv().await {
        match job {
            HistoryJob::Recent { symbol, first } => {
                let Some(inst) = mk.cat.get(&symbol).cloned() else { continue };
                if fresh.lock().unwrap().get(&symbol).is_some_and(|t| t.elapsed() < RECENT_TTL) || !busy.lock().unwrap().insert(symbol.clone()) {
                    continue;
                }
                let permit = slots.clone().acquire_owned().await.expect("semaphore");
                let (pv, mk, cfg, busy, fresh, deep_started) = (pv.clone(), mk.clone(), cfg.clone(), busy.clone(), fresh.clone(), deep_started.clone());
                tokio::spawn(async move {
                    if let Err(e) = recent(&pv, &mk, &inst, first).await {
                        tracing::warn!(symbol = %inst.symbol, error = %e, "on-demand history failed");
                    }
                    fresh.lock().unwrap().insert(inst.symbol.clone(), std::time::Instant::now());
                    busy.lock().unwrap().remove(&inst.symbol);
                    drop(permit);
                    // deep history of a catalogue symbol, once per process, in the background
                    if !inst.is_core() && deep_started.lock().unwrap().insert(inst.symbol.clone()) {
                        if let Err(e) = deep_all(&cfg, &pv, &mk, &inst).await {
                            tracing::warn!(symbol = %inst.symbol, error = %e, "background history stopped");
                        }
                    }
                });
            }
            HistoryJob::Snapshot { symbols } => {
                let (pv, mk) = (pv.clone(), mk.clone());
                tokio::spawn(async move {
                    if let Err(e) = snapshot(&pv, &mk, &symbols, Prio::Interactive).await {
                        tracing::debug!(error = %e, "price snapshot failed");
                    }
                });
            }
        }
    }
}

/// Order in which the timeframes of a symbol are fetched: the one someone waits for first (its source, H1, for
/// the aggregated ones).
fn tf_order(first: Option<Tf>) -> Vec<Tf> {
    let lead = match first {
        Some(t) if DIRECT.contains(&t) => Some(t),
        Some(_) => Some(Tf::H1),
        None => None,
    };
    let mut v: Vec<Tf> = lead.into_iter().collect();
    v.extend(DIRECT.into_iter().filter(|t| Some(*t) != lead));
    v
}

/// The latest 500 bars of every timeframe of one symbol (first view of a catalogue instrument, or a gap after a
/// streaming pause), then the aggregated timeframes and provider daily history before our H1 coverage.
async fn recent(pv: &Provider, mk: &Arc<Market>, inst: &Instrument, first: Option<Tf>) -> anyhow::Result<()> {
    for tf in tf_order(first) {
        match pv.klines(Prio::Interactive, inst, tf.provider_kline().unwrap(), None).await {
            Ok(bars) if !bars.is_empty() => {
                store_final(mk, inst, tf, &bars).await?;
                seed_current(mk, inst, tf, &bars);
                if !mk.is_streaming(&inst.symbol)
                    && tf == Tf::M1
                    && let Some(b) = bars.last()
                {
                    mk.set_snapshot(&inst.symbol, b.c, None, b.t.timestamp_millis() + 59_999);
                }
            }
            Ok(_) => tracing::debug!(symbol = %inst.symbol, tf = tf.name(), "provider returned no history"),
            Err(e) => tracing::warn!(symbol = %inst.symbol, tf = tf.name(), error = %e, "history request failed"),
        }
        if tf == Tf::H1 {
            aggregate_higher(mk, inst, None).await?;
        }
    }
    daily_before_h1(pv, mk, inst, None, Prio::Interactive).await?;
    tracing::info!(symbol = %inst.symbol, "recent history stored");
    Ok(())
}

/// Background deep history of one symbol (configured depth per timeframe, then provider daily bars).
async fn deep_all(cfg: &Config, pv: &Provider, mk: &Arc<Market>, inst: &Instrument) -> anyhow::Result<()> {
    let now = Utc::now();
    for (minutes, days) in cfg.backfill_days {
        let tf = Tf::from_minutes(minutes).unwrap();
        deep(pv, mk, inst, tf, now - Duration::days(days)).await?;
    }
    let from = NaiveDate::parse_from_str(&cfg.backfill_daily_from, "%Y-%m-%d").unwrap_or(NaiveDate::from_ymd_opt(2012, 1, 1).unwrap());
    daily_before_h1(pv, mk, inst, Some(Utc.from_utc_datetime(&from.and_hms_opt(0, 0, 0).unwrap())), Prio::Background).await
}

/// Delayed prices of symbols that are not streamed: the provider's current daily bar (close = last price, open /
/// high / low = the provider's day), one request per market and batch.
async fn snapshot(pv: &Provider, mk: &Arc<Market>, symbols: &[String], prio: Prio) -> anyhow::Result<usize> {
    let mut by_market: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for s in symbols {
        if let Some(i) = mk.cat.get(s)
            && !mk.is_streaming(s)
        {
            by_market.entry(i.provider.market.clone()).or_default().push(i.provider.code.clone());
        }
    }
    let mut n = 0;
    for (market, codes) in by_market {
        for (code, bars) in pv.latest_batch(prio, &market, &codes, 8, 1).await? {
            let (Some(inst), Some(b)) = (mk.cat.from_provider(&market, &code), bars.last()) else { continue };
            mk.set_snapshot(&inst.symbol, b.c, Some((b.o, b.h, b.l)), Utc::now().timestamp_millis().min(b.t.timestamp_millis() + 86_399_999));
            n += 1;
        }
    }
    Ok(n)
}

/// Every `every` seconds: a delayed price for every catalogue symbol that is not streamed (background priority).
async fn snapshot_sweep(mk: Arc<Market>, every: u64) {
    // let the startup backfill of the core instruments go first
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    loop {
        let todo: Vec<String> = mk.cat.list.iter().filter(|i| !i.is_core() && !mk.is_streaming(&i.symbol)).map(|i| i.symbol.clone()).collect();
        if !todo.is_empty() {
            mk.request_history(HistoryJob::Snapshot { symbols: todo });
        }
        tokio::time::sleep(std::time::Duration::from_secs(every)).await;
    }
}

const DIRECT: [Tf; 5] = [Tf::M1, Tf::M5, Tf::M15, Tf::M30, Tf::H1];

async fn run(cfg: &Config, pv: &Provider, mk: &Arc<Market>) -> anyhow::Result<()> {
    // eager: the core instruments (the catalogue fills lazily, see `worker`)
    let list: Vec<Instrument> = mk.cat.list.iter().filter(|i| i.is_core()).cloned().collect();
    // ---- phase 1: latest bars everywhere ----
    for inst in &list {
        for tf in DIRECT {
            match pv.klines(Prio::Snapshot, inst, tf.provider_kline().unwrap(), None).await {
                Ok(bars) if !bars.is_empty() => {
                    store_final(mk, inst, tf, &bars).await?;
                    seed_current(mk, inst, tf, &bars);
                }
                Ok(_) => tracing::warn!(symbol = %inst.symbol, tf = tf.name(), "provider returned no history"),
                Err(e) => tracing::warn!(symbol = %inst.symbol, tf = tf.name(), error = %e, "history request failed"),
            }
        }
        aggregate_higher(mk, inst, None).await?;
        daily_before_h1(pv, mk, inst, None, Prio::Snapshot).await?;
        tracing::info!(symbol = %inst.symbol, "quick backfill done");
    }
    tracing::info!("phase 1 complete — charts available for all symbols");

    // ---- phase 2: deep history, resumable ----
    let now = Utc::now();
    for (minutes, days) in cfg.backfill_days {
        let tf = Tf::from_minutes(minutes).unwrap();
        for inst in &list {
            deep(pv, mk, inst, tf, now - Duration::days(days)).await?;
        }
    }
    let from = NaiveDate::parse_from_str(&cfg.backfill_daily_from, "%Y-%m-%d").unwrap_or(NaiveDate::from_ymd_opt(2012, 1, 1).unwrap());
    let from = Utc.from_utc_datetime(&from.and_hms_opt(0, 0, 0).unwrap());
    for inst in &list {
        daily_before_h1(pv, mk, inst, Some(from), Prio::Background).await?;
    }
    tracing::info!("phase 2 complete — full history stored");
    Ok(())
}

async fn store(mk: &Market, inst: &Instrument, tf: Tf, bars: &[Bar], src: Source) -> anyhow::Result<()> {
    let rows: Vec<(String, i32, Bar)> = bars.iter().map(|b| (inst.symbol.clone(), tf.minutes(), *b)).collect();
    db::upsert_bars(&mk.pool, &rows, src).await
}

/// Provider bars that have closed are final (Reconciled); only the one still forming merges with live data.
async fn store_final(mk: &Market, inst: &Instrument, tf: Tf, bars: &[Bar]) -> anyhow::Result<()> {
    let current = tf.bucket(Utc::now());
    let (closed, forming): (Vec<Bar>, Vec<Bar>) = bars.iter().partition(|b| b.t < current);
    store(mk, inst, tf, &closed, Source::Reconciled).await?;
    store(mk, inst, tf, &forming, Source::Provider).await
}

/// If the provider's newest bar is the one still forming, seed our live builder with its true open/high/low.
fn seed_current(mk: &Market, inst: &Instrument, tf: Tf, bars: &[Bar]) {
    if let Some(last) = bars.last() {
        if last.t == tf.bucket(Utc::now()) {
            mk.seed_forming(&inst.symbol, tf, *last);
        }
    }
}

async fn deep(pv: &Provider, mk: &Arc<Market>, inst: &Instrument, tf: Tf, target: DateTime<Utc>) -> anyhow::Result<()> {
    let key_tf = tf.minutes();
    let done: Option<bool> = sqlx::query_scalar("SELECT done FROM backfill_state WHERE symbol = $1 AND tf = $2 AND target_t <= $3")
        .bind(&inst.symbol)
        .bind(key_tf)
        .bind(target)
        .fetch_optional(&mk.pool)
        .await?;
    if done == Some(true) {
        return Ok(());
    }
    loop {
        let Some(oldest) = db::oldest_bar(&mk.pool, &inst.symbol, key_tf).await? else { break };
        if oldest <= target {
            break;
        }
        let bars = match pv.klines(Prio::Background, inst, tf.provider_kline().unwrap(), Some(oldest - Duration::seconds(1))).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(symbol = %inst.symbol, tf = tf.name(), error = %e, "deep history stopped (plan limit or no more data)");
                break;
            }
        };
        let older: Vec<Bar> = bars.into_iter().filter(|b| b.t < oldest).collect();
        if older.is_empty() {
            break;
        }
        store_final(mk, inst, tf, &older).await?;
        if tf == Tf::H1 {
            aggregate_higher(mk, inst, Some(older[0].t)).await?;
        }
    }
    sqlx::query(
        "INSERT INTO backfill_state (symbol, tf, oldest_t, target_t, done, updated_at) VALUES ($1,$2,(SELECT min(t) FROM candles WHERE symbol=$1 AND tf=$2),$3,TRUE,now())
         ON CONFLICT (symbol, tf) DO UPDATE SET oldest_t = EXCLUDED.oldest_t, target_t = EXCLUDED.target_t, done = TRUE, updated_at = now()",
    )
    .bind(&inst.symbol)
    .bind(key_tf)
    .bind(target)
    .execute(&mk.pool)
    .await?;
    tracing::info!(symbol = %inst.symbol, tf = tf.name(), "deep history complete");
    Ok(())
}

/// Aggregate `src` bars into `dst` buckets. Buckets starting before the oldest source bar are skipped (incomplete).
fn aggregate(src: &[Bar], dst: Tf, src_oldest: DateTime<Utc>) -> Vec<Bar> {
    let mut map: BTreeMap<DateTime<Utc>, Bar> = BTreeMap::new();
    for b in src {
        let k = dst.bucket(b.t);
        if k < src_oldest {
            continue;
        }
        map.entry(k)
            .and_modify(|a| {
                a.h = a.h.max(b.h);
                a.l = a.l.min(b.l);
                a.c = b.c;
                a.v += b.v;
            })
            .or_insert(Bar { t: k, ..*b });
    }
    map.into_values().collect()
}

/// H4 & D1 from H1, then W1 & MN from D1 — for bars at or after `from` (None = everything). Only buckets that start
/// inside the loaded window are rebuilt: one that started before it would be built from part of its bars.
async fn aggregate_higher(mk: &Market, inst: &Instrument, from: Option<DateTime<Utc>>) -> anyhow::Result<()> {
    let Some(h1_oldest) = db::oldest_bar(&mk.pool, &inst.symbol, Tf::H1.minutes()).await? else { return Ok(()) };
    let start = from.map(|f| Tf::W1.bucket(f)).unwrap_or(h1_oldest).max(h1_oldest);
    let h1_from = start - Duration::days(35);
    let h1 = db::load_bars(&mk.pool, &inst.symbol, Tf::H1.minutes(), None, Some(h1_from), 1_000_000).await?;
    for dst in [Tf::H4, Tf::D1] {
        let bars = aggregate(&h1, dst, h1_oldest.max(h1_from));
        store(mk, inst, dst, &bars, Source::Aggregated).await?;
        seed_current(mk, inst, dst, &bars);
    }
    let Some(d1_oldest) = db::oldest_bar(&mk.pool, &inst.symbol, Tf::D1.minutes()).await? else { return Ok(()) };
    // from the week that holds the first day of the month: every W1 and MN bucket in the window is whole
    let d1_from = Tf::W1.bucket(Tf::MN.bucket(start));
    let d1 = db::load_bars(&mk.pool, &inst.symbol, Tf::D1.minutes(), None, Some(d1_from), 1_000_000).await?;
    for dst in [Tf::W1, Tf::MN] {
        let bars = aggregate(&d1, dst, d1_oldest.max(d1_from));
        store(mk, inst, dst, &bars, Source::Aggregated).await?;
        seed_current(mk, inst, dst, &bars);
    }
    Ok(())
}

/// Provider daily history for days older than our H1 coverage (labelled with the matching server day).
async fn daily_before_h1(pv: &Provider, mk: &Market, inst: &Instrument, until: Option<DateTime<Utc>>, prio: Prio) -> anyhow::Result<()> {
    let Some(h1_oldest) = db::oldest_bar(&mk.pool, &inst.symbol, Tf::H1.minutes()).await? else { return Ok(()) };
    let first_full_day = Tf::D1.bucket(h1_oldest) + Duration::days(1);
    let mut before: Option<DateTime<Utc>> = None;
    loop {
        let bars = match pv.klines(prio, inst, 8, before).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(symbol = %inst.symbol, error = %e, "daily history stopped");
                break;
            }
        };
        if bars.is_empty() {
            break;
        }
        let oldest = bars[0].t;
        // UTC day D → server day D (bucket containing D 12:00 UTC)
        let shifted: Vec<Bar> = bars.iter().map(|b| Bar { t: Tf::D1.bucket(b.t + Duration::hours(12)), ..*b }).filter(|b| b.t < first_full_day).collect();
        store(mk, inst, Tf::D1, &shifted, Source::Provider).await?;
        match until {
            Some(u) if oldest > u && before != Some(oldest) => before = Some(oldest - Duration::seconds(1)),
            _ => break,
        }
    }
    // weekly/monthly over the whole daily range
    let Some(d1_oldest) = db::oldest_bar(&mk.pool, &inst.symbol, Tf::D1.minutes()).await? else { return Ok(()) };
    let d1 = db::load_bars(&mk.pool, &inst.symbol, Tf::D1.minutes(), None, None, 1_000_000).await?;
    for dst in [Tf::W1, Tf::MN] {
        let bars = aggregate(&d1, dst, d1_oldest);
        store(mk, inst, dst, &bars, Source::Aggregated).await?;
    }
    Ok(())
}

/// Every closed bar is replaced by the provider's final OHLC, so each candle is identical to the market
/// source (live ticks can arrive late or be missed). Runs 4s and again 40s after every minute boundary
/// (in case the provider finalises late): M1 each minute, M5/M15/M30/H1 when they close, then
/// H4/D1/W1/MN rebuilt from the corrected bars.
async fn reconcile_loop(pv: &Provider, mk: &Arc<Market>) {
    loop {
        let now = Utc::now();
        let minute = Tf::M1.bucket(now);
        let (boundary, at) = if now < minute + Duration::seconds(40) { (minute, minute + Duration::seconds(40)) } else { (minute + Duration::minutes(1), minute + Duration::seconds(64)) };
        let (boundary, at) = if now < minute + Duration::seconds(4) { (minute, minute + Duration::seconds(4)) } else { (boundary, at) };
        tokio::time::sleep((at - now).to_std().unwrap_or_default()).await;
        // only streamed symbols build live bars; the others get history when they are next viewed or streamed
        let mut by_market: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for s in mk.streaming() {
            if let Some(i) = mk.cat.get(&s) {
                by_market.entry(i.provider.market.clone()).or_default().push(i.provider.code.clone());
            }
        }
        let markets: Vec<(String, Vec<String>)> = by_market.into_iter().collect();
        for tf in DIRECT.into_iter().filter(|tf| tf.bucket(boundary) == boundary) {
            for (market, codes) in &markets {
                if codes.is_empty() {
                    continue;
                }
                match pv.latest_batch(Prio::Reconcile, market, codes, tf.provider_kline().unwrap(), 3).await {
                    Ok(list) => {
                        for (code, bars) in list {
                            let Some(inst) = mk.cat.from_provider(market, &code) else { continue };
                            let closed: Vec<Bar> = bars.into_iter().filter(|b| b.t < boundary).collect();
                            if let Err(e) = store(mk, inst, tf, &closed, Source::Reconciled).await {
                                tracing::warn!(error = %e, "reconcile store failed");
                            }
                            for b in &closed {
                                mk.absorb_closed(&inst.symbol, tf, *b);
                            }
                            if tf == Tf::H1 {
                                if let Err(e) = reconcile_higher(mk, inst, boundary).await {
                                    tracing::warn!(symbol = %inst.symbol, error = %e, "higher-timeframe reconcile failed");
                                }
                            }
                        }
                    }
                    Err(e) => tracing::warn!(%market, tf = tf.name(), error = %e, "reconcile request failed"),
                }
            }
        }
    }
}

/// After an H1 close at `end`: any H4/D1 bucket that just closed is rebuilt from the reconciled H1 bars,
/// and any W1/MN bucket that just closed from the D1 bars — all marked final.
async fn reconcile_higher(mk: &Market, inst: &Instrument, end: DateTime<Utc>) -> anyhow::Result<()> {
    for (dst, src) in [(Tf::H4, Tf::H1), (Tf::D1, Tf::H1), (Tf::W1, Tf::D1), (Tf::MN, Tf::D1)] {
        if dst.bucket(end) != end {
            continue;
        }
        let start = dst.bucket(end - Duration::seconds(1));
        let Some(src_oldest) = db::oldest_bar(&mk.pool, &inst.symbol, src.minutes()).await? else { continue };
        let bars = db::load_bars(&mk.pool, &inst.symbol, src.minutes(), Some(end - Duration::seconds(1)), Some(start), 10_000).await?;
        let agg: Vec<Bar> = aggregate(&bars, dst, src_oldest).into_iter().filter(|b| b.t == start).collect();
        store(mk, inst, dst, &agg, Source::Reconciled).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// W1 and MN are rebuilt from the D1 bars after an H1 page (deep history, a gap healed): the D1 window loaded for
    /// that starts inside an earlier week and month, and those buckets must not be rebuilt from the few days of them
    /// in the window (that overwrote last month's bar with its last day). Against PostgreSQL :5433.
    #[tokio::test]
    async fn rebuilding_higher_timeframes_leaves_buckets_before_the_window_alone() {
        use std::str::FromStr;
        let base = std::env::var("MARKET_DATA_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into());
        let name = format!("kalks_md_agg_{}", std::process::id());
        let server = sqlx::postgres::PgConnectOptions::from_str(&base).unwrap();
        let url = sqlx::ConnectOptions::to_url_lossy(&server.clone().database(&name)).to_string();
        let pool = db::connect(&url).await.expect("PostgreSQL :5433 must be running for this test");
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config");
        let cat = crate::instruments::Catalogue::load(&format!("{root}/instruments.json"), &format!("{root}/holidays")).unwrap();
        let mk = Market::new(cat, pool.clone(), crate::spreads::Spreads::default(), false, crate::demand::Demand::new(Vec::<String>::new(), Default::default()));
        let inst = mk.cat.get("BTCUSD").unwrap().clone();
        let day = |m: u32, d: u32| Tf::D1.bucket(Utc.with_ymd_and_hms(2026, m, d, 12, 0, 0).unwrap());
        // provider daily bars, 1 Aug - 20 Oct (server days), every one different
        let mut d1 = Vec::new();
        let mut d = day(8, 1);
        let mut i = 0.0;
        while d <= day(10, 20) {
            d1.push(Bar { t: d, o: 100.0 + i, h: 110.0 + i, l: 90.0 - i, c: 101.0 + i, v: 1.0 });
            d = Tf::D1.bucket(d + Duration::hours(36));
            i += 1.0;
        }
        store(&mk, &inst, Tf::D1, &d1, Source::Provider).await.unwrap();
        let oldest = d1[0].t;
        for dst in [Tf::W1, Tf::MN] {
            store(&mk, &inst, dst, &aggregate(&d1, dst, oldest), Source::Aggregated).await.unwrap();
        }
        let before_mn = db::load_bars(&pool, "BTCUSD", Tf::MN.minutes(), None, None, 10).await.unwrap();
        let before_w1 = db::load_bars(&pool, "BTCUSD", Tf::W1.minutes(), None, None, 50).await.unwrap();
        // our H1 coverage: 12-14 Oct, consistent with those days
        let mut h1 = Vec::new();
        for b in d1.iter().filter(|b| b.t >= day(10, 12) && b.t <= day(10, 14)) {
            for h in 0..24 {
                let (o, c) = if h == 0 { (b.o, b.c) } else { (b.c, b.c) };
                let (hi, lo) = if h == 5 { (b.h, b.c) } else if h == 9 { (b.c, b.l) } else { (o.max(c), o.min(c)) };
                h1.push(Bar { t: b.t + Duration::hours(h), o, h: hi, l: lo, c, v: 1.0 / 24.0 });
            }
        }
        store(&mk, &inst, Tf::H1, &h1, Source::Reconciled).await.unwrap();
        // an H1 page that starts on 13 Oct (deep history, or a gap healed)
        aggregate_higher(&mk, &inst, Some(day(10, 13))).await.unwrap();

        let after_mn = db::load_bars(&pool, "BTCUSD", Tf::MN.minutes(), None, None, 10).await.unwrap();
        let after_w1 = db::load_bars(&pool, "BTCUSD", Tf::W1.minutes(), None, None, 50).await.unwrap();
        let key = |v: &[Bar]| v.iter().map(|b| (b.t, b.o, b.h, b.l, b.c)).collect::<Vec<_>>();
        assert_eq!(key(&after_mn), key(&before_mn), "August, September and October keep their whole months");
        assert_eq!(key(&after_w1), key(&before_w1), "every week keeps its whole range");

        pool.close().await;
        let mut admin = sqlx::ConnectOptions::connect(&server.database("postgres")).await.unwrap();
        let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))).execute(&mut admin).await;
    }

    #[test]
    fn the_waited_for_timeframe_comes_first() {
        assert_eq!(tf_order(Some(Tf::M15)), vec![Tf::M15, Tf::M1, Tf::M5, Tf::M30, Tf::H1]);
        // aggregated timeframes need H1 first
        assert_eq!(tf_order(Some(Tf::D1))[0], Tf::H1);
        assert_eq!(tf_order(Some(Tf::W1))[0], Tf::H1);
        assert_eq!(tf_order(None), DIRECT.to_vec());
        assert_eq!(tf_order(Some(Tf::H4)).len(), 5);
    }
}

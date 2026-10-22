//! In-memory market state: latest quote per symbol, the forming bar of every timeframe,
//! a broadcast channel for subscribers, and a 1-second flusher that persists bars and ticks.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tokio::sync::{Notify, broadcast, mpsc};

use crate::db::{self, Bar, Source};
use crate::demand::Demand;
use crate::depth::FeedDepth;
use crate::instruments::{Catalogue, Instrument};
use crate::spreads::Spreads;
use crate::timeframes::Tf;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Quote {
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    /// provider event time, ms since epoch
    pub t: i64,
    /// last depth (top-of-book) update, ms — 0 if this symbol has no book stream
    #[serde(skip)]
    pub book_t: i64,
    /// when this service received the update (ms, local clock) — lets clients measure our added latency
    #[serde(skip)]
    pub recv: i64,
    /// Not a live price: the last price of a symbol that is not streamed (a provider bar, or the last stored bar
    /// of a catalogue instrument after a restart). Shown to people (`"d":1`), never traded on.
    #[serde(skip)]
    pub delayed: bool,
}

#[derive(Clone, Debug)]
pub enum Event {
    /// `symbol` is shared: the broadcast clones every event once per stream client, so no per-client allocation.
    Quote { symbol: Arc<str>, quote: Quote },
    /// Every timeframe a trade (or a correction) changed, in one event instead of one per timeframe.
    Bars { symbol: Arc<str>, bars: Arc<[(Tf, Bar)]> },
}

/// Bars and ticks waiting for the next 1-second flush. Bars are keyed by their open time too: a bar that closes
/// between two flushes (its last ticks, then the first tick of the next bar) is stored with its final values, not
/// replaced by the next bar before it reached the database.
#[derive(Default)]
struct Pending {
    bars: HashMap<(String, Tf, DateTime<Utc>), Bar>,
    ticks: Vec<(String, DateTime<Utc>, Option<f64>, Option<f64>, Option<f64>)>,
}

/// A request for provider history of one symbol (backfill.rs): the first chart of a catalogue instrument, a
/// symbol that started streaming again (gap), or a price for a symbol nobody streams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryJob {
    /// Latest bars of every timeframe (`first`: the timeframe someone is waiting for goes first).
    Recent { symbol: String, first: Option<Tf> },
    /// Latest price of symbols that are not streamed.
    Snapshot { symbols: Vec<String> },
}

pub struct Market {
    pub cat: Catalogue,
    pub pool: PgPool,
    pub spreads: Spreads,
    quotes: RwLock<HashMap<String, Quote>>,
    /// provider depth (several priced levels) per symbol, when the feed carries it
    depth: RwLock<HashMap<String, FeedDepth>>,
    forming: RwLock<HashMap<(String, Tf), Bar>>,
    pending: Mutex<Pending>,
    pub tx: broadcast::Sender<Event>,
    pub store_ticks: bool,
    stats: Mutex<Stats>,
    /// Who wants which symbol streamed (demand.rs), and a wake-up for the planner.
    pub demand: Arc<Mutex<Demand>>,
    pub demand_changed: Arc<Notify>,
    /// Symbols subscribed at the provider right now (the planner's last plan).
    streaming: RwLock<BTreeSet<String>>,
    /// Day open / high / low of delayed snapshots (symbols without live bars).
    snap_day: RwLock<HashMap<String, (f64, f64, f64)>>,
    /// History requests to the backfill worker (none in relay mode without an upstream history source).
    pub history: OnceLock<mpsc::UnboundedSender<HistoryJob>>,
    /// The provider's REST client (none in relay mode).
    pub provider: OnceLock<Arc<crate::backfill::Provider>>,
}

#[derive(Default, Clone, Serialize)]
pub struct Stats {
    pub ticks_total: u64,
    pub ticks_last_min: u64,
    pub connected_markets: HashSet<String>,
    pub last_tick_ms: HashMap<String, i64>,
    /// Provider businesses that should be connected (they have symbols to stream) and since when each has been
    /// missing (absent = connected or not expected).
    pub expected: HashSet<String>,
    pub missing_since: HashMap<String, DateTime<Utc>>,
    /// Businesses the provider refuses (HTTP 429 at connect), since the first refusal of the episode.
    pub refused_since: HashMap<String, DateTime<Utc>>,
}

/// A business missing for longer than this makes `/health` not ok.
pub const MISSING_ALERT_SECS: i64 = 300;

impl Stats {
    /// Expected businesses not connected now.
    pub fn missing(&self) -> Vec<String> {
        let mut v: Vec<String> = self.expected.iter().filter(|b| !self.connected_markets.contains(*b)).cloned().collect();
        v.sort();
        v
    }

    /// Healthy: something is connected and no expected business has been missing for more than 5 minutes.
    pub fn healthy(&self, now: DateTime<Utc>) -> bool {
        !self.connected_markets.is_empty() && self.missing().iter().all(|b| self.missing_since.get(b).is_none_or(|t| (now - *t).num_seconds() <= MISSING_ALERT_SECS))
    }
}

impl Market {
    pub fn new(cat: Catalogue, pool: PgPool, spreads: Spreads, store_ticks: bool, demand: Demand) -> Arc<Self> {
        let (tx, _) = broadcast::channel(8192);
        Arc::new(Self {
            demand: Arc::new(Mutex::new(demand)),
            demand_changed: Arc::new(Notify::new()),
            streaming: RwLock::new(BTreeSet::new()),
            snap_day: RwLock::new(HashMap::new()),
            history: OnceLock::new(),
            provider: OnceLock::new(),
            cat,
            pool,
            spreads,
            quotes: RwLock::new(HashMap::new()),
            depth: RwLock::new(HashMap::new()),
            forming: RwLock::new(HashMap::new()),
            pending: Mutex::new(Pending::default()),
            tx,
            store_ticks,
            stats: Mutex::new(Stats::default()),
        })
    }

    pub fn quote(&self, symbol: &str) -> Option<Quote> {
        self.quotes.read().unwrap().get(symbol).copied()
    }

    /// Is `symbol` subscribed at the provider right now?
    pub fn is_streaming(&self, symbol: &str) -> bool {
        self.streaming.read().unwrap().contains(symbol)
    }

    pub fn streaming(&self) -> BTreeSet<String> {
        self.streaming.read().unwrap().clone()
    }

    pub fn set_streaming(&self, s: BTreeSet<String>) {
        *self.streaming.write().unwrap() = s;
    }

    /// Asks the backfill worker for history (ignored when there is none, e.g. relay mode).
    pub fn request_history(&self, job: HistoryJob) -> bool {
        self.history.get().is_some_and(|tx| tx.send(job).is_ok())
    }

    /// Day open / high / low for `/v1/quotes`: our own D1 bar, else the delayed snapshot's.
    pub fn day_stats(&self, inst: &Instrument) -> Option<(f64, f64, f64)> {
        match self.forming(&inst.symbol, Tf::D1) {
            Some(d) => {
                let d = inst.round_bar(d);
                Some((d.o, d.h, d.l))
            }
            None => self.snap_day.read().unwrap().get(&inst.symbol).copied(),
        }
    }

    /// A delayed price for a symbol that is not streamed (from provider bars). Never replaces a live price that is
    /// still current; announced to subscribers with `"d":1`.
    pub fn set_snapshot(&self, symbol: &str, last: f64, day: Option<(f64, f64, f64)>, t: i64) {
        let Some(inst) = self.cat.get(symbol) else { return };
        if last <= 0.0 || !last.is_finite() {
            return;
        }
        let q = {
            let mut qs = self.quotes.write().unwrap();
            if let Some(cur) = qs.get(symbol)
                && (!cur.delayed && self.is_streaming(symbol) || cur.t >= t)
            {
                return;
            }
            let half = inst.base_spread / 2.0;
            let q = Quote { bid: inst.round(last - half), ask: inst.round(last + half), last: inst.round(last), t, book_t: 0, recv: Utc::now().timestamp_millis(), delayed: true };
            qs.insert(symbol.to_string(), q);
            q
        };
        if let Some((o, h, l)) = day {
            self.snap_day.write().unwrap().insert(symbol.to_string(), (inst.round(o), inst.round(h), inst.round(l)));
        }
        let _ = self.tx.send(Event::Quote { symbol: symbol.into(), quote: q });
    }

    pub fn feed_depth(&self, symbol: &str) -> Option<FeedDepth> {
        self.depth.read().unwrap().get(symbol).cloned()
    }

    /// Several priced levels from the provider's depth stream (raw prices, best first). The top of book still
    /// goes through `on_book`; this only keeps the ladder for the depth-of-market view.
    pub fn on_depth(&self, symbol: &str, bids: Vec<(f64, f64)>, asks: Vec<(f64, f64)>, t: i64) {
        let Some(inst) = self.cat.get(symbol) else { return };
        let clean = |v: Vec<(f64, f64)>| v.into_iter().filter(|(p, s)| *p > 0.0 && *s > 0.0 && p.is_finite() && s.is_finite()).map(|(p, s)| (inst.round(p), s)).take(crate::depth::MAX_LEVELS).collect::<Vec<_>>();
        let (bids, asks) = (clean(bids), clean(asks));
        if bids.len() < 2 || asks.len() < 2 {
            return;
        }
        self.depth.write().unwrap().insert(symbol.to_string(), FeedDepth { bids, asks, t });
    }

    pub fn forming(&self, symbol: &str, tf: Tf) -> Option<Bar> {
        self.forming.read().unwrap().get(&(symbol.to_string(), tf)).copied()
    }

    pub fn stats(&self) -> Stats {
        self.stats.lock().unwrap().clone()
    }

    pub fn set_connected(&self, market: &str, up: bool) {
        let mut s = self.stats.lock().unwrap();
        if up {
            s.connected_markets.insert(market.to_string());
            s.missing_since.remove(market);
        } else {
            s.connected_markets.remove(market);
            if s.expected.contains(market) {
                s.missing_since.entry(market.to_string()).or_insert_with(Utc::now);
            }
        }
    }

    /// A business has symbols to stream (it should be connected) or not.
    pub fn set_expected(&self, market: &str, expected: bool) {
        let mut s = self.stats.lock().unwrap();
        if expected {
            if s.expected.insert(market.to_string()) && !s.connected_markets.contains(market) {
                s.missing_since.entry(market.to_string()).or_insert_with(Utc::now);
            }
        } else {
            s.expected.remove(market);
            s.missing_since.remove(market);
        }
    }

    /// The provider refuses this business since `since` (None = no longer).
    pub fn set_refused(&self, market: &str, since: Option<DateTime<Utc>>) {
        let mut s = self.stats.lock().unwrap();
        match since {
            Some(t) => {
                s.refused_since.insert(market.to_string(), t);
            }
            None => {
                s.refused_since.remove(market);
            }
        }
    }

    /// Seed a forming bar from the database (e.g. after restart or provider backfill of the current bar).
    pub fn seed_forming(&self, symbol: &str, tf: Tf, bar: Bar) {
        let mut f = self.forming.write().unwrap();
        let key = (symbol.to_string(), tf);
        match f.get_mut(&key) {
            Some(cur) if cur.t == bar.t => {
                cur.o = bar.o;
                cur.h = cur.h.max(bar.h);
                cur.l = cur.l.min(bar.l);
            }
            Some(cur) if cur.t > bar.t => {}
            _ => {
                f.insert(key, bar);
            }
        }
    }

    /// After a restart: restore every timeframe's latest bar and the last price from the database, so quotes,
    /// day stats and charts are right immediately — including while the market is closed (weekend close).
    pub async fn restore(&self) -> anyhow::Result<()> {
        // one statement for every symbol and timeframe (an index probe each), not 9 queries per symbol
        let symbols: Vec<String> = self.cat.list.iter().map(|i| i.symbol.clone()).collect();
        for (symbol, tf, bar) in db::latest_bars(&self.pool, &symbols).await? {
            let (Some(inst), Some(tf)) = (self.cat.get(&symbol), Tf::from_minutes(tf)) else { continue };
            self.seed_forming(&inst.symbol, tf, bar);
            if tf == Tf::M1 {
                let half = inst.base_spread / 2.0;
                let t = bar.t.timestamp_millis() + 59_999;
                self.quotes.write().unwrap().entry(inst.symbol.clone()).or_insert(Quote {
                    bid: inst.round(bar.c - half),
                    ask: inst.round(bar.c + half),
                    last: inst.round(bar.c),
                    t,
                    book_t: 0,
                    recv: 0,
                    // a catalogue instrument may not stream again soon: its stored price is not a live one
                    delayed: !inst.is_core(),
                });
            }
        }
        Ok(())
    }

    /// Top-of-book update from the provider depth stream.
    pub fn on_book(&self, symbol: &str, bid: f64, ask: f64, t: i64) {
        let Some(inst) = self.cat.get(symbol) else { return };
        let (bid, ask) = (inst.round(bid), inst.round(ask));
        if !inst.in_session(DateTime::<Utc>::from_timestamp_millis(t).unwrap_or_else(Utc::now)) {
            return;
        }
        let recv = Utc::now().timestamp_millis();
        let q = {
            let mut qs = self.quotes.write().unwrap();
            let e = qs.entry(symbol.to_string()).or_insert(Quote { bid: 0.0, ask: 0.0, last: inst.round((bid + ask) / 2.0), t, book_t: t, recv, delayed: false });
            e.book_t = t;
            if e.bid == bid && e.ask == ask && !e.delayed {
                return; // same top of book (depth snapshots repeat it): nothing to push
            }
            e.delayed = false;
            e.bid = bid;
            e.ask = ask;
            e.t = e.t.max(t);
            e.recv = recv;
            *e
        };
        let _ = self.tx.send(Event::Quote { symbol: symbol.into(), quote: q });
    }

    /// Trade/price tick from the provider: updates the quote (if no book yet) and every timeframe's bar.
    pub fn on_trade(&self, symbol: &str, price: f64, volume: f64, t_ms: i64) {
        let Some(inst) = self.cat.get(symbol) else { return };
        let price = inst.round(price); // the provider can send more decimals than the symbol's digits
        let ts = DateTime::<Utc>::from_timestamp_millis(t_ms).unwrap_or_else(Utc::now);
        if !inst.in_session(ts) {
            return; // outside the trading session: quote stays at the last session price
        }
        let recv = Utc::now().timestamp_millis();
        let (q, quote_changed) = {
            let mut qs = self.quotes.write().unwrap();
            let half = inst.base_spread / 2.0;
            let e = qs.entry(symbol.to_string()).or_insert(Quote { bid: 0.0, ask: 0.0, last: 0.0, t: t_ms, book_t: 0, recv, delayed: false });
            let before = (e.bid, e.ask, e.last, e.delayed);
            e.delayed = false;
            e.last = price;
            // depth snapshots arrive far less often than trades: when the book is older than 1s, or a trade prints
            // outside it, re-centre bid/ask on the trade price keeping the book's spread width
            if t_ms - e.book_t > 1_000 || price < e.bid || price > e.ask {
                let half = if e.book_t > 0 && e.ask > e.bid { (e.ask - e.bid) / 2.0 } else { half };
                e.bid = inst.round(price - half);
                e.ask = inst.round(price + half);
            }
            e.t = e.t.max(t_ms);
            let changed = before != (e.bid, e.ask, e.last, e.delayed);
            if changed {
                e.recv = recv;
            }
            (*e, changed)
        };

        let mut changed = Vec::with_capacity(Tf::ALL.len());
        {
            let mut forming = self.forming.write().unwrap();
            let mut pending = self.pending.lock().unwrap();
            for tf in Tf::ALL {
                let start = tf.bucket(ts);
                let key = (symbol.to_string(), tf);
                let bar = match forming.get_mut(&key) {
                    Some(b) if b.t == start => {
                        b.h = b.h.max(price);
                        b.l = b.l.min(price);
                        b.c = price;
                        b.v += volume;
                        *b
                    }
                    Some(b) if b.t > start => continue, // late tick for an already-closed bar
                    _ => {
                        let nb = Bar { t: start, o: price, h: price, l: price, c: price, v: volume };
                        forming.insert(key.clone(), nb);
                        nb
                    }
                };
                pending.bars.insert((symbol.to_string(), tf, bar.t), bar);
                changed.push((tf, bar));
            }
            if self.store_ticks {
                pending.ticks.push((symbol.to_string(), ts, Some(q.bid), Some(q.ask), Some(price)));
            }
        }
        {
            let mut s = self.stats.lock().unwrap();
            s.ticks_total += 1;
            s.ticks_last_min += 1;
            s.last_tick_ms.insert(symbol.to_string(), t_ms);
        }
        // quotes go out immediately on every change (no batching); unchanged prices only move the bars' volume
        let sym: Arc<str> = symbol.into();
        if quote_changed {
            let _ = self.tx.send(Event::Quote { symbol: sym.clone(), quote: q });
        }
        if !changed.is_empty() {
            let _ = self.tx.send(Event::Bars { symbol: sym, bars: changed.into() });
        }
    }

    /// A closed `src` bar was corrected to the provider's final values: widen every higher timeframe's
    /// forming bar that contains it, so H1/H4/D1… keep the same extremes the provider has.
    pub fn absorb_closed(&self, symbol: &str, src: Tf, bar: Bar) {
        let mut changed = Vec::new();
        {
            let mut forming = self.forming.write().unwrap();
            let mut pending = self.pending.lock().unwrap();
            for tf in Tf::ALL.into_iter().filter(|t| *t > src) {
                let key = (symbol.to_string(), tf);
                let Some(b) = forming.get_mut(&key) else { continue };
                if b.t != tf.bucket(bar.t) || (bar.h <= b.h && bar.l >= b.l) {
                    continue;
                }
                b.h = b.h.max(bar.h);
                b.l = b.l.min(bar.l);
                pending.bars.insert((symbol.to_string(), tf, b.t), *b);
                changed.push((tf, *b));
            }
        }
        if !changed.is_empty() {
            let _ = self.tx.send(Event::Bars { symbol: symbol.into(), bars: changed.into() });
        }
    }

    /// Persist changed bars and archived ticks. Called every second.
    pub async fn flush(&self) -> anyhow::Result<()> {
        let (bars, ticks) = {
            let mut p = self.pending.lock().unwrap();
            (std::mem::take(&mut p.bars), std::mem::take(&mut p.ticks))
        };
        let rows: Vec<(String, i32, Bar)> = bars.into_iter().map(|((s, tf, _), b)| (s, tf.minutes(), b)).collect();
        db::upsert_bars(&self.pool, &rows, Source::Live).await?;
        db::insert_ticks(&self.pool, &ticks).await?;
        Ok(())
    }

    pub fn reset_minute_counter(&self) -> u64 {
        let mut s = self.stats.lock().unwrap();
        std::mem::take(&mut s.ticks_last_min)
    }
}

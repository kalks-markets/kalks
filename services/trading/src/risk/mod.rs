//! Dealer risk desk (docs/RISK-DESK.md): server-side exposure, risk limits (alert or enforce), client profitability
//! scores, A/B routing recommendations and the risk alert stream (Back Office + Telegram).
//!
//! Scope is standard broker risk management. Nothing here changes, delays or synthesises prices, quotes or candles:
//! the desk only observes the book, refuses new orders that break a limit the broker set (`enforce`), and tells staff.
//!
//! - [`RiskCtx`] (in `Shared`) is the live, in-memory side the pure engine reads through `Env::risk`: the latest
//!   exposure index per tenant, the latest score of every account, the news calendar, and a queue of observations
//!   (orders a limit refused or would have refused) for the alert engine.
//! - Configuration (limits, routing mode) is tenant config: `TenantConfig::risk` ([`RiskConfig`]).
//! - Background jobs ([`spawn`]): exposure (every second, pushed to the dealing stream every 5 s, alerts), news
//!   calendar (every 5 minutes), client scores (schedule + on demand), routing suggestions (every 30 s).

pub mod alerts;
pub mod exposure;
pub mod leverage;
pub mod limits;
pub mod news;
pub mod routing;
pub mod score;
pub mod telegram;

use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

pub use limits::{DailyLossLimit, ExposureLimit, GroupLimit, LimitMode, NewsLimit, RiskConfig, RiskLimits, RiskSettings, RoutingMode};

use crate::money::{D, ZERO};
use crate::specs::Spec;

/// What the order gate needs from the latest exposure computation of one tenant (live accounts).
#[derive(Clone, Debug, Default)]
pub struct ExpIndex {
    /// When the scan that built it started (ms): in-flight deltas older than this are already inside.
    pub started_ms: i64,
    /// B-book net client exposure per symbol, notional USD (+ = clients net long).
    pub net_b_usd: HashMap<String, D>,
    /// Open CFD positions per user and login.
    pub positions: HashMap<i64, HashMap<i64, usize>>,
}

/// The latest score of an account, as the routing conditions read it.
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreLite {
    pub user_id: i64,
    pub score: f64,
    pub category: String,
    pub win_rate: Option<f64>,
    pub avg_hold_secs: Option<f64>,
    pub country: Option<String>,
    pub computed_at: DateTime<Utc>,
}

/// A high-impact calendar event (news service `/v1/calendar`).
#[derive(Clone, Debug, PartialEq)]
pub struct NewsEvent {
    pub id: i64,
    pub title: String,
    pub currency: String,
    pub impact: u8,
    pub starts_at: DateTime<Utc>,
    pub symbols: Vec<String>,
}

impl NewsEvent {
    /// Does the event move `spec` (one of its currencies, or the symbol tagged on the event)?
    pub fn affects(&self, spec: &Spec) -> bool {
        let c = self.currency.to_uppercase();
        spec.quote_ccy == c || spec.base_ccy.as_deref() == Some(c.as_str()) || self.symbols.iter().any(|s| s.eq_ignore_ascii_case(&spec.symbol)) || (spec.base_ccy.is_none() && spec.symbol.starts_with(&c))
    }
}

/// A news window hit: the event and how far it is (minutes, + = still ahead).
#[derive(Clone, Debug, PartialEq)]
pub struct NewsHit {
    pub event: NewsEvent,
    pub minutes_to: i64,
}

#[derive(Default)]
pub struct NewsState {
    pub events: Vec<NewsEvent>,
    pub configured: bool,
    pub last_ok: Option<DateTime<Utc>>,
}

/// An order a limit refused (`enforced`) or would have refused (alert mode).
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub tenant_id: i64,
    pub login: i64,
    pub symbol: String,
    /// `exposure_limit` | `group_max_lot` | `group_max_positions` | `news_window`.
    pub code: &'static str,
    pub message: String,
    pub enforced: bool,
    pub at: DateTime<Utc>,
}

const MAX_OBSERVED: usize = 2_000;

#[derive(Default)]
pub struct RiskCtx {
    index: RwLock<HashMap<i64, Arc<ExpIndex>>>,
    /// (tenant, symbol) → B-book exposure added by orders the gate passed since the last scan: (ms, USD).
    inflight: Mutex<HashMap<(i64, String), Vec<(i64, D)>>>,
    scores: RwLock<HashMap<i64, ScoreLite>>,
    pub news: RwLock<NewsState>,
    observed: Mutex<VecDeque<Observation>>,
    /// The last full exposure view per tenant (the `GET /v1/dealing/exposure` body).
    pub views: RwLock<HashMap<i64, Arc<serde_json::Value>>>,
    /// Score runs asked for on demand: (tenant, login or every account).
    pub score_requests: Mutex<Vec<(i64, Option<i64>)>>,
    pub score_wake: tokio::sync::Notify,
    pub score_running: AtomicBool,
}

impl RiskCtx {
    pub fn set_index(&self, tenant_id: i64, idx: ExpIndex) {
        let started = idx.started_ms;
        self.index.write().unwrap().insert(tenant_id, Arc::new(idx));
        let mut inf = self.inflight.lock().unwrap();
        for v in inf.values_mut() {
            v.retain(|(ms, _)| *ms >= started);
        }
        inf.retain(|(t, _), v| *t != tenant_id || !v.is_empty());
    }

    pub fn index(&self, tenant_id: i64) -> Option<Arc<ExpIndex>> {
        self.index.read().unwrap().get(&tenant_id).cloned()
    }

    /// B-book net client exposure of `symbol` (USD): the last scan plus what the gate let through since.
    pub fn net_b_usd(&self, tenant_id: i64, symbol: &str) -> D {
        let base = self.index(tenant_id).and_then(|i| i.net_b_usd.get(symbol).copied()).unwrap_or(ZERO);
        let extra: D = self.inflight.lock().unwrap().get(&(tenant_id, symbol.to_string())).map(|v| v.iter().map(|(_, x)| *x).sum()).unwrap_or(ZERO);
        base + extra
    }

    pub fn add_inflight(&self, tenant_id: i64, symbol: &str, usd: D, now_ms: i64) {
        let mut inf = self.inflight.lock().unwrap();
        let v = inf.entry((tenant_id, symbol.to_string())).or_default();
        if v.len() < 10_000 {
            v.push((now_ms, usd));
        }
    }

    /// Open CFD positions of `user_id` on its other accounts (last scan).
    pub fn other_positions(&self, tenant_id: i64, user_id: i64, login: i64) -> usize {
        self.index(tenant_id).and_then(|i| i.positions.get(&user_id).map(|m| m.iter().filter(|(l, _)| **l != login).map(|(_, n)| *n).sum())).unwrap_or(0)
    }

    pub fn score(&self, login: i64) -> Option<ScoreLite> {
        self.scores.read().unwrap().get(&login).cloned()
    }

    pub fn set_scores(&self, items: impl IntoIterator<Item = (i64, ScoreLite)>) {
        let mut w = self.scores.write().unwrap();
        for (login, s) in items {
            w.insert(login, s);
        }
    }

    /// The nearest event of impact ≥ `min_impact` on `spec` whose window [start − before, start + after] holds `now`.
    pub fn news_hit(&self, spec: &Spec, now: DateTime<Utc>, before_min: i64, after_min: i64, min_impact: u8) -> Option<NewsHit> {
        let r = self.news.read().unwrap();
        r.events
            .iter()
            .filter(|e| e.impact >= min_impact && e.affects(spec))
            .filter(|e| now >= e.starts_at - chrono::Duration::minutes(before_min) && now <= e.starts_at + chrono::Duration::minutes(after_min))
            .map(|e| NewsHit { event: e.clone(), minutes_to: (e.starts_at - now).num_minutes() })
            .min_by_key(|h| h.minutes_to.abs())
    }

    /// Minutes to the nearest event of impact ≥ `min_impact` on `spec` (|Δ|, within 4 hours).
    pub fn news_minutes(&self, spec: &Spec, now: DateTime<Utc>, min_impact: u8) -> Option<i64> {
        self.news_hit(spec, now, 240, 240, min_impact).map(|h| h.minutes_to.abs())
    }

    pub fn set_news(&self, events: Vec<NewsEvent>, at: DateTime<Utc>) {
        let mut w = self.news.write().unwrap();
        w.events = events;
        w.configured = true;
        w.last_ok = Some(at);
    }

    pub fn observe(&self, o: Observation) {
        let mut q = self.observed.lock().unwrap();
        if q.len() >= MAX_OBSERVED {
            q.pop_front();
        }
        q.push_back(o);
    }

    pub fn drain_observed(&self) -> Vec<Observation> {
        self.observed.lock().unwrap().drain(..).collect()
    }

    /// Asks the score job to run now (one account, or every account of the tenant).
    pub fn request_scores(&self, tenant_id: i64, login: Option<i64>) {
        self.score_requests.lock().unwrap().push((tenant_id, login));
        self.score_wake.notify_one();
    }
}

/// Starts the risk desk jobs (exposure, alerts, news, scores, routing suggestions).
pub fn spawn(st: crate::api::AppState) {
    tokio::spawn(exposure::run(st.clone()));
    tokio::spawn(news::run(st.clone()));
    tokio::spawn(score::run(st.clone()));
    tokio::spawn(routing::run(st));
}

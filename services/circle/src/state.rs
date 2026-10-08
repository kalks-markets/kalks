//! Shared state: DB pool, config, HTTP client, the realtime hub (WebSocket fan-out), one-time stream tickets,
//! presence, rate limits and the wake-ups of the background workers.

use crate::config::Config;
use crate::storage::Storage;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, broadcast};

/// Who a realtime event is for.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    User(i64),
    Users(Arc<Vec<i64>>),
    /// Every connected member of a conversation (the socket tracks its memberships).
    Conv(i64),
    /// Back Office sockets with moderation rights.
    Staff,
}

#[derive(Clone, Debug)]
pub struct Event {
    pub to: Target,
    pub payload: Arc<Value>,
    /// Users who must not receive it (the sender of a typing event, blocked users).
    pub except: Option<i64>,
}

/// Identity bound to a stream ticket.
#[derive(Clone, Debug)]
pub enum Who {
    User { id: i64, tenant: String },
    Staff { id: String, tenant: String, global: bool },
}

struct Ticket {
    who: Who,
    expires: Instant,
}

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<Event>,
    tickets: Arc<Mutex<HashMap<String, Ticket>>>,
    online: Arc<Mutex<HashMap<i64, usize>>>,
}

impl Default for Hub {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(8192);
        Self { tx, tickets: Default::default(), online: Default::default() }
    }
}

pub const TICKET_TTL: Duration = Duration::from_secs(30);

impl Hub {
    pub fn send(&self, to: Target, payload: Value) {
        let _ = self.tx.send(Event { to, payload: Arc::new(payload), except: None });
    }

    pub fn send_except(&self, to: Target, payload: Value, except: i64) {
        let _ = self.tx.send(Event { to, payload: Arc::new(payload), except: Some(except) });
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    /// One-time ticket for `GET /v1/stream?ticket=` (30 s).
    pub fn issue(&self, who: Who) -> String {
        let t = crate::util::token(24);
        let mut m = self.tickets.lock().unwrap();
        let now = Instant::now();
        m.retain(|_, v| v.expires > now);
        m.insert(t.clone(), Ticket { who, expires: now + TICKET_TTL });
        t
    }

    pub fn redeem(&self, ticket: &str) -> Option<Who> {
        let mut m = self.tickets.lock().unwrap();
        let t = m.remove(ticket)?;
        (t.expires > Instant::now()).then_some(t.who)
    }

    pub fn connected(&self, user: i64, delta: i64) {
        let mut m = self.online.lock().unwrap();
        let e = m.entry(user).or_insert(0);
        *e = (*e as i64 + delta).max(0) as usize;
        if *e == 0 {
            m.remove(&user);
        }
    }

    pub fn is_online(&self, user: i64) -> bool {
        self.online.lock().unwrap().get(&user).copied().unwrap_or(0) > 0
    }

    pub fn online_of(&self, users: &[i64]) -> HashSet<i64> {
        let m = self.online.lock().unwrap();
        users.iter().copied().filter(|u| m.get(u).copied().unwrap_or(0) > 0).collect()
    }
}

/// Sliding-window limiter (uploads, AI calls, posting).
#[derive(Clone, Default)]
pub struct Limiter(Arc<Mutex<HashMap<String, Vec<Instant>>>>);

impl Limiter {
    pub fn hit(&self, key: &str, max: usize, window: Duration) -> bool {
        let mut m = self.0.lock().unwrap();
        let now = Instant::now();
        if m.len() > 50_000 {
            m.retain(|_, v| v.last().is_some_and(|t| now.duration_since(*t) < Duration::from_secs(3600)));
        }
        let v = m.entry(key.to_string()).or_default();
        v.retain(|t| now.duration_since(*t) < window);
        if v.len() >= max {
            return false;
        }
        v.push(now);
        true
    }
}

/// Wake-ups of the background workers (they also poll on a timer).
#[derive(Clone, Default)]
pub struct Wake {
    pub media: Arc<Notify>,
    pub moderation: Arc<Notify>,
    pub notify: Arc<Notify>,
}

/// Small TTL cache (module switches, quotes, FCM access token).
#[derive(Clone, Default)]
pub struct Cache(Arc<Mutex<HashMap<String, (Instant, Value)>>>);

impl Cache {
    pub fn get(&self, key: &str, ttl: Duration) -> Option<Value> {
        self.0.lock().unwrap().get(key).filter(|(at, _)| at.elapsed() < ttl).map(|(_, v)| v.clone())
    }
    pub fn stale(&self, key: &str) -> Option<Value> {
        self.0.lock().unwrap().get(key).map(|(_, v)| v.clone())
    }
    pub fn put(&self, key: &str, v: Value) {
        let mut m = self.0.lock().unwrap();
        if m.len() > 20_000 {
            m.clear();
        }
        m.insert(key.to_string(), (Instant::now(), v));
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub cfg: Arc<Config>,
    pub http: reqwest::Client,
    pub hub: Hub,
    pub limiter: Limiter,
    pub storage: Storage,
    pub wake: Wake,
    pub cache: Cache,
}

impl AppState {
    pub fn new(pool: PgPool, cfg: Config) -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(20)).build().expect("http client");
        let storage = Storage::from_config(&cfg, http.clone());
        Self { pool, cfg: Arc::new(cfg), http, hub: Hub::default(), limiter: Limiter::default(), storage, wake: Wake::default(), cache: Cache::default() }
    }

    /// Whether Claude is configured.
    pub fn ai(&self) -> bool {
        !self.cfg.anthropic_key.is_empty()
    }
}

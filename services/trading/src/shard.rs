//! Account shards: one tokio task per shard owns its accounts' state and is the only writer for them.
//!
//! Every request for an account is a closure sent to the account's shard (`login % shards`). The shard runs
//! it against a working copy (`engine::Tx`), commits the resulting events + projections + audit rows in
//! one PostgreSQL transaction, and only then swaps the new state in and publishes stream frames. A failed
//! commit leaves the in-memory state untouched. Price ticks arrive on a separate channel; commands are
//! always served first.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::engine::{AuditDraft, Env, Ids, Note, Quotes, Reject, Tx, risk};
use crate::feed::QuoteBook;
use crate::model::{Account, AccountKind, Book, Status};
use crate::persist::{self, AuditRow, Batch, CommitError};
use crate::rules::Registry;
use crate::specs::{Specs, SpecsCell};
use crate::state::{AccountState, Event};
use crate::views;

pub type Op = Box<dyn FnOnce(&mut Tx, &Env) -> Result<Value, Reject> + Send>;
pub type ReadFn = Box<dyn FnOnce(Option<(&AccountState, &Env)>) -> Value + Send>;
pub type ScanFn = Arc<dyn Fn(&AccountState, &Env) -> Vec<Value> + Send + Sync>;

#[derive(Clone, Debug)]
pub struct Staff {
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Debug)]
pub enum ExecError {
    Reject(Reject),
    NotFound,
    /// Ledger idempotency key already used.
    Duplicate(String),
    Internal(String),
}

#[derive(Debug)]
pub struct Done {
    pub value: Value,
    pub audit: Vec<Value>,
    pub notes: Vec<Note>,
}

pub struct Job {
    pub login: i64,
    pub actor: String,
    pub staff: Option<Staff>,
    pub reason_code: String,
    pub note: String,
    pub request: Option<Value>,
    pub op: Op,
    pub reply: oneshot::Sender<Result<Done, ExecError>>,
}

pub enum Cmd {
    Exec(Job),
    Open { account: Box<Account>, credentials: (String, String), actor: String, reply: oneshot::Sender<Result<Value, ExecError>> },
    Read { login: i64, f: ReadFn, reply: oneshot::Sender<Value> },
    Scan { tenant_id: i64, f: ScanFn, reply: oneshot::Sender<Vec<Value>> },
    Rollover { day: NaiveDate, at: DateTime<Utc>, reply: oneshot::Sender<usize> },
    /// Option expiry settlement of `key` (`SYMBOL:YYYY-MM-DD`) at `fixing`, fixing run `run`.
    Settle { key: String, fixing: crate::money::D, run: i32, usdq: Option<crate::money::D>, reply: oneshot::Sender<SettleReport> },
}

/// What one settlement pass did in one shard.
#[derive(Clone, Debug, Default)]
pub struct SettleReport {
    /// (tenant, login, positions settled, cash booked in USD)
    pub accounts: Vec<(i64, i64, usize, crate::money::D)>,
    pub failures: usize,
}

/// The engine clock. Production reads the system clock; tests pin it.
#[derive(Default)]
pub struct Clock(RwLock<Option<DateTime<Utc>>>);

impl Clock {
    pub fn now(&self) -> DateTime<Utc> {
        self.0.read().unwrap().unwrap_or_else(Utc::now)
    }
    /// Pins the clock (None = the system clock again).
    pub fn set(&self, t: Option<DateTime<Utc>>) {
        *self.0.write().unwrap() = t;
    }
}

pub struct TickMsg {
    pub group: Arc<str>,
    pub symbol: Arc<str>,
    pub recv_ms: i64,
}

/// Lightweight account directory shared by all handlers.
#[derive(Clone, Debug)]
pub struct AccountMeta {
    pub tenant_id: i64,
    pub user_id: i64,
    pub kind: AccountKind,
    pub group: String,
    pub status: Status,
    pub name: String,
    pub exec_delay_ms: u32,
}

#[derive(Default)]
pub struct Index {
    pub accounts: HashMap<i64, AccountMeta>,
    /// open position / pending order ticket → login
    pub tickets: HashMap<i64, i64>,
    /// Open option contracts per login (long, short): per-client limits across a client's accounts.
    pub option_contracts: HashMap<i64, (crate::money::D, crate::money::D)>,
}

impl Index {
    /// Open option contracts (long, short) of `user_id`'s other accounts in the tenant.
    pub fn other_contracts(&self, tenant_id: i64, user_id: i64, login: i64) -> (crate::money::D, crate::money::D) {
        let mut out = (crate::money::ZERO, crate::money::ZERO);
        for (l, (a, b)) in &self.option_contracts {
            if *l != login && self.accounts.get(l).is_some_and(|m| m.tenant_id == tenant_id && m.user_id == user_id) {
                out.0 += *a;
                out.1 += *b;
            }
        }
        out
    }
}

fn option_contracts(st: &AccountState) -> Option<(crate::money::D, crate::money::D)> {
    let mut out = (crate::money::ZERO, crate::money::ZERO);
    let mut any = false;
    for p in st.positions.values().filter(|p| p.option.is_some()) {
        any = true;
        if p.side == crate::model::Side::Buy { out.0 += p.volume } else { out.1 += p.volume }
    }
    any.then_some(out)
}

/// Events of one committed transaction, handed to the copy-trading tap (see `social::copier`).
#[derive(Clone, Debug)]
pub struct Committed {
    pub login: i64,
    pub tenant_id: i64,
    /// Stream version of `events[0]`.
    pub first_version: i64,
    pub at: DateTime<Utc>,
    pub events: Vec<Event>,
    /// Account equity (USD) right after the commit.
    pub equity_usd: crate::money::D,
}

/// Per-account and per-tenant (dealing) broadcast channels of JSON frames, plus the committed-event tap
/// for watched accounts (copy-trading masters and copy accounts).
#[derive(Clone, Default)]
pub struct Streams {
    accounts: Arc<Mutex<HashMap<i64, broadcast::Sender<Arc<str>>>>>,
    dealing: Arc<Mutex<HashMap<i64, broadcast::Sender<Arc<str>>>>>,
    tap: Arc<RwLock<Option<mpsc::UnboundedSender<Committed>>>>,
    watched: Arc<RwLock<HashSet<i64>>>,
}

impl Streams {
    /// Sends every committed transaction of a watched login to `tx`, in commit order.
    pub fn set_tap(&self, tx: mpsc::UnboundedSender<Committed>) {
        *self.tap.write().unwrap() = Some(tx);
    }
    pub fn watch(&self, login: i64) {
        self.watched.write().unwrap().insert(login);
    }
    pub fn unwatch(&self, login: i64) {
        self.watched.write().unwrap().remove(&login);
    }
    pub fn watching(&self, login: i64) -> bool {
        self.tap.read().unwrap().is_some() && self.watched.read().unwrap().contains(&login)
    }
    fn tap(&self, c: Committed) {
        if let Some(t) = self.tap.read().unwrap().as_ref() {
            let _ = t.send(c);
        }
    }
    pub fn subscribe_account(&self, login: i64) -> broadcast::Receiver<Arc<str>> {
        self.accounts.lock().unwrap().entry(login).or_insert_with(|| broadcast::channel(1024).0).subscribe()
    }
    pub fn subscribe_dealing(&self, tenant_id: i64) -> broadcast::Receiver<Arc<str>> {
        self.dealing.lock().unwrap().entry(tenant_id).or_insert_with(|| broadcast::channel(4096).0).subscribe()
    }
    fn account(&self, login: i64) -> Option<broadcast::Sender<Arc<str>>> {
        let mut m = self.accounts.lock().unwrap();
        match m.get(&login) {
            Some(tx) if tx.receiver_count() > 0 => Some(tx.clone()),
            Some(_) => {
                m.remove(&login);
                None
            }
            None => None,
        }
    }
    pub fn dealing(&self, tenant_id: i64) -> Option<broadcast::Sender<Arc<str>>> {
        let m = self.dealing.lock().unwrap();
        m.get(&tenant_id).filter(|tx| tx.receiver_count() > 0).cloned()
    }
}

#[derive(Default)]
pub struct Stats {
    pub ticks: AtomicU64,
    pub commits: AtomicU64,
    pub commit_errors: AtomicU64,
    /// feed receive → shard processed, last tick (ms)
    pub last_tick_lag_ms: AtomicI64,
    pub max_tick_lag_ms: AtomicI64,
}

/// A-book liquidity-provider bridge (D25). Launch is B-book only: the adapter is a stub that reports it is
/// not connected; A-routed trades are still executed internally and flagged A for risk attribution.
pub trait LpAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn connected(&self) -> bool;
    /// Called after an A-book fill is committed.
    fn hedge(&self, login: i64, ticket: i64, symbol: &str, side: &str, volume: &str) -> Result<(), String>;
}

pub struct NullLp;

impl LpAdapter for NullLp {
    fn name(&self) -> &str {
        "none"
    }
    fn connected(&self) -> bool {
        false
    }
    fn hedge(&self, login: i64, ticket: i64, symbol: &str, side: &str, volume: &str) -> Result<(), String> {
        tracing::warn!(login, ticket, symbol, side, volume, "A-book fill kept internal: no LP connected");
        Err("lp_not_connected".into())
    }
}

/// Symbols with positions or pending orders, per shard (see `Shard::publish_held`).
#[derive(Default)]
pub struct Held(RwLock<HashMap<usize, BTreeSet<String>>>);

impl Held {
    pub fn set(&self, shard: usize, symbols: BTreeSet<String>) {
        let mut w = self.0.write().unwrap();
        if w.get(&shard) != Some(&symbols) {
            w.insert(shard, symbols);
        }
    }
    /// Every shard's symbols together.
    pub fn all(&self) -> BTreeSet<String> {
        self.0.read().unwrap().values().flatten().cloned().collect()
    }
}

pub struct Shared {
    pub pool: PgPool,
    pub registry: Registry,
    /// Current contract specs (hot-swapped when the Back Office changes a template or the live switch).
    pub specs: SpecsCell,
    /// Symbols each shard holds positions or pending orders on (published every few seconds; the feed asks
    /// market-data to keep them streaming).
    pub held: Held,
    pub quotes: Arc<QuoteBook>,
    pub ids: Arc<Ids>,
    pub index: Arc<RwLock<Index>>,
    pub streams: Streams,
    pub stats: Arc<Stats>,
    pub lp: Arc<dyn LpAdapter>,
    pub max_quote_age_ms: i64,
    /// Client restrictions from the gateway (controls.rs), checked by `trade::gate` for client actions.
    pub restrictions: Arc<crate::controls::Restrictions>,
    /// Kalks FX Options: snapshot, raw spots, pricer (src/options).
    pub options: Arc<crate::options::OptionsCtx>,
    pub clock: Arc<Clock>,
    /// Kalks FX Options order book: actors, venues, outbox state (src/book).
    pub books: Arc<crate::book::Books>,
    /// Corporate actions due and not yet applied everywhere (src/corporate.rs).
    pub corp: crate::engine::corporate::CorpDue,
    /// Risk desk: exposure index, client scores, news windows, limit observations (src/risk).
    pub risk: Arc<crate::risk::RiskCtx>,
}

/* ------------------------------------------------------------------ */
/* Hub: the handle every API handler uses                              */
/* ------------------------------------------------------------------ */

#[derive(Clone)]
pub struct Hub {
    cmds: Vec<mpsc::Sender<Cmd>>,
    ticks: Vec<mpsc::Sender<Arc<TickMsg>>>,
    pub shared: Arc<Shared>,
}

impl Hub {
    /// Spawns the shards with the replayed states.
    pub fn start(shared: Arc<Shared>, n: usize, states: HashMap<i64, AccountState>) -> Self {
        let mut buckets: Vec<HashMap<i64, AccountState>> = (0..n).map(|_| HashMap::new()).collect();
        {
            let mut idx = shared.index.write().unwrap();
            for (login, st) in states {
                idx.accounts.insert(login, meta(&st));
                for t in st.positions.keys().chain(st.orders.keys()) {
                    idx.tickets.insert(*t, login);
                }
                if let Some(c) = option_contracts(&st) {
                    idx.option_contracts.insert(login, c);
                }
                buckets[(login as u64 % n as u64) as usize].insert(login, st);
            }
        }
        let mut cmds = Vec::new();
        let mut ticks = Vec::new();
        for (i, states) in buckets.into_iter().enumerate() {
            let (ctx, crx) = mpsc::channel(4096);
            let (ttx, trx) = mpsc::channel(8192);
            cmds.push(ctx);
            ticks.push(ttx);
            let mut shard = Shard { id: i, specs: shared.specs.load(), sh: shared.clone(), states, interest: HashMap::new(), keys: HashMap::new(), dirty: HashSet::new(), last_eval: HashMap::new(), pending: HashSet::new() };
            for login in shard.states.keys().copied().collect::<Vec<_>>() {
                shard.reindex(login);
            }
            tokio::spawn(shard.run(crx, trx));
        }
        let hub = Hub { cmds, ticks, shared };
        // the shards hand committed order-book commands (fired stops, SL / TP) to the book through the hub
        let _ = hub.shared.books.hub.set(hub.clone());
        hub
    }

    fn shard_of(&self, login: i64) -> usize {
        (login as u64 % self.cmds.len() as u64) as usize
    }

    async fn send(&self, login: i64, cmd: Cmd) -> Result<(), ExecError> {
        self.cmds[self.shard_of(login)].send(cmd).await.map_err(|_| ExecError::Internal("shard stopped".into()))
    }

    /// Runs `op` on `login` as the single writer; returns its value plus the committed audit entries.
    #[allow(clippy::too_many_arguments)]
    pub async fn exec(&self, login: i64, actor: &str, staff: Option<Staff>, reason_code: &str, note: &str, request: Option<Value>, op: Op) -> Result<Done, ExecError> {
        let (reply, rx) = oneshot::channel();
        let job = Job { login, actor: actor.to_string(), staff, reason_code: reason_code.to_string(), note: note.to_string(), request, op, reply };
        self.send(login, Cmd::Exec(job)).await?;
        rx.await.map_err(|_| ExecError::Internal("shard dropped the request".into()))?
    }

    pub async fn open(&self, account: Account, credentials: (String, String), actor: &str) -> Result<Value, ExecError> {
        let (reply, rx) = oneshot::channel();
        let login = account.login;
        self.send(login, Cmd::Open { account: Box::new(account), credentials, actor: actor.into(), reply }).await?;
        rx.await.map_err(|_| ExecError::Internal("shard dropped the request".into()))?
    }

    /// Reads one account inside its shard (consistent snapshot). `None` input = unknown login.
    pub async fn read(&self, login: i64, f: ReadFn) -> Value {
        let (reply, rx) = oneshot::channel();
        if self.send(login, Cmd::Read { login, f, reply }).await.is_err() {
            return Value::Null;
        }
        rx.await.unwrap_or(Value::Null)
    }

    /// Runs `f` over every account of a tenant (all shards) and concatenates the results.
    pub async fn scan(&self, tenant_id: i64, f: ScanFn) -> Vec<Value> {
        let mut rxs = Vec::new();
        for c in &self.cmds {
            let (reply, rx) = oneshot::channel();
            if c.send(Cmd::Scan { tenant_id, f: f.clone(), reply }).await.is_ok() {
                rxs.push(rx);
            }
        }
        let mut out = Vec::new();
        for rx in rxs {
            if let Ok(v) = rx.await {
                out.extend(v);
            }
        }
        out
    }

    /// Settles option expiry `key` in every shard; returns what each account booked.
    pub async fn settle(&self, key: &str, fixing: crate::money::D, run: i32, usdq: Option<crate::money::D>) -> SettleReport {
        let mut rxs = Vec::new();
        for c in &self.cmds {
            let (reply, rx) = oneshot::channel();
            if c.send(Cmd::Settle { key: key.to_string(), fixing, run, usdq, reply }).await.is_ok() {
                rxs.push(rx);
            }
        }
        let mut out = SettleReport::default();
        for rx in rxs {
            match rx.await {
                Ok(r) => {
                    out.accounts.extend(r.accounts);
                    out.failures += r.failures;
                }
                Err(_) => out.failures += 1,
            }
        }
        out
    }

    pub async fn rollover(&self, day: NaiveDate, at: DateTime<Utc>) -> usize {
        let mut rxs = Vec::new();
        for c in &self.cmds {
            let (reply, rx) = oneshot::channel();
            if c.send(Cmd::Rollover { day, at, reply }).await.is_ok() {
                rxs.push(rx);
            }
        }
        let mut n = 0;
        for rx in rxs {
            n += rx.await.unwrap_or(0);
        }
        n
    }

    /// Fan a quote out to every shard (each shard only evaluates accounts with interest in it).
    pub async fn tick(&self, msg: Arc<TickMsg>) {
        for t in &self.ticks {
            let _ = t.send(msg.clone()).await;
        }
    }

    pub fn login_of_ticket(&self, ticket: i64) -> Option<i64> {
        self.shared.index.read().unwrap().tickets.get(&ticket).copied()
    }

    pub fn meta(&self, login: i64) -> Option<AccountMeta> {
        self.shared.index.read().unwrap().accounts.get(&login).cloned()
    }
}

fn meta(st: &AccountState) -> AccountMeta {
    let a = &st.account;
    AccountMeta { tenant_id: a.tenant_id, user_id: a.user_id, kind: a.kind, group: a.group.clone(), status: a.status, name: a.name.clone(), exec_delay_ms: a.controls.exec_delay_ms }
}

/* ------------------------------------------------------------------ */
/* Shard                                                               */
/* ------------------------------------------------------------------ */

struct Shard {
    id: usize,
    sh: Arc<Shared>,
    /// Specs snapshot for the event being handled (refreshed from `sh.specs` before each one).
    specs: Arc<Specs>,
    states: HashMap<i64, AccountState>,
    /// (spread group, symbol) → logins with a position or pending order on it
    interest: HashMap<(String, String), BTreeSet<i64>>,
    keys: HashMap<i64, Vec<(String, String)>>,
    /// accounts whose equity changed since the last stream push
    dirty: HashSet<i64>,
    /// (login, underlying) → last full option evaluation on a raw tick (ms): at most one per 250 ms
    last_eval: HashMap<(i64, String), i64>,
    /// raw ticks skipped by the throttle, evaluated on the next timer tick
    pending: HashSet<(i64, String)>,
}

impl Shard {
    async fn run(mut self, mut cmds: mpsc::Receiver<Cmd>, mut ticks: mpsc::Receiver<Arc<TickMsg>>) {
        let mut timer = tokio::time::interval(std::time::Duration::from_millis(250));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut n: u64 = 0;
        tracing::info!(shard = self.id, accounts = self.states.len(), "shard started");
        loop {
            tokio::select! {
                biased;
                cmd = cmds.recv() => match cmd {
                    Some(c) => {
                        self.specs = self.sh.specs.load();
                        self.handle(c).await
                    }
                    None => break,
                },
                t = ticks.recv() => match t {
                    Some(t) => {
                        self.specs = self.sh.specs.load();
                        self.on_tick(t).await
                    }
                    None => break,
                },
                _ = timer.tick() => {
                    n += 1;
                    self.specs = self.sh.specs.load();
                    if n % 20 == 0 {
                        self.publish_held();
                    }
                    self.throttled().await;
                    self.push_equity();
                    if n % 4 == 0 {
                        self.expire().await;
                        self.push_dealing_pnl();
                    }
                    if n % 20 == 0 {
                        self.options_timer().await;
                    }
                }
            }
        }
    }

    async fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Exec(job) => {
                let Job { login, actor, staff, reason_code, note, request, op, reply } = job;
                let res = self.execute(login, &actor, staff.as_ref(), &reason_code, &note, request, None, op).await;
                let _ = reply.send(res);
            }
            Cmd::Open { account, credentials, actor, reply } => {
                let _ = reply.send(self.open(*account, credentials, &actor).await);
            }
            Cmd::Read { login, f, reply } => {
                let v = match self.states.get(&login) {
                    Some(st) => match self.sh.registry.get(st.account.tenant_id) {
                        Some(t) => match t.groups.get(&st.account.group) {
                            Some(g) => {
                                let env = self.env(&t, g);
                                f(Some((st, &env)))
                            }
                            None => f(None),
                        },
                        None => f(None),
                    },
                    None => f(None),
                };
                let _ = reply.send(v);
            }
            Cmd::Scan { tenant_id, f, reply } => {
                let mut out = Vec::new();
                if let Some(t) = self.sh.registry.get(tenant_id) {
                    for st in self.states.values().filter(|s| s.account.tenant_id == tenant_id) {
                        if let Some(g) = t.groups.get(&st.account.group) {
                            let env = self.env(&t, g);
                            out.extend(f(st, &env));
                        }
                    }
                }
                let _ = reply.send(out);
            }
            Cmd::Settle { key, fixing, run, usdq, reply } => {
                let logins: Vec<i64> = self.states.values().filter(|s| s.positions.values().any(|p| p.option.as_ref().is_some_and(|t| t.expiry_key() == key))).map(|s| s.login()).collect();
                let mut rep = SettleReport::default();
                for login in logins {
                    let k = key.clone();
                    let op: Op = Box::new(move |tx, env| {
                        let out = crate::engine::options::settle(tx, env, &k, fixing, run, usdq);
                        risk::check_margin(tx, env);
                        Ok(json!({"tickets": out.tickets, "cash": crate::money::num(out.cash)}))
                    });
                    match self.execute(login, "system", None, "", "", None, None, op).await {
                        Ok(d) => {
                            let st = &self.states[&login];
                            let n = d.value["tickets"].as_array().map(Vec::len).unwrap_or(0);
                            let cash = d.value["cash"].as_f64().and_then(crate::money::from_f64).unwrap_or_default() / st.account.usd_factor();
                            rep.accounts.push((st.account.tenant_id, login, n, cash));
                        }
                        Err(e) => {
                            rep.failures += 1;
                            tracing::error!(login, error = ?e, %key, "option settlement failed for account");
                        }
                    }
                }
                let _ = reply.send(rep);
            }
            Cmd::Rollover { day, at, reply } => {
                let logins: Vec<i64> = self.states.values().filter(|s| !s.positions.is_empty()).map(|s| s.login()).collect();
                let mut n = 0;
                for login in logins {
                    let op: Op = Box::new(move |tx, env| {
                        risk::rollover(tx, env, day, at);
                        risk::check_margin(tx, env);
                        Ok(Value::Null)
                    });
                    match self.execute(login, "system", None, "", "", None, None, op).await {
                        Ok(_) => n += 1,
                        Err(e) => tracing::error!(login, error = ?e, %day, "rollover failed for account"),
                    }
                }
                let _ = reply.send(n);
            }
        }
    }

    fn env<'a>(&'a self, t: &'a crate::rules::TenantConfig, g: &'a crate::rules::Group) -> Env<'a> {
        Env {
            specs: &self.specs,
            tenant: t,
            group: g,
            quotes: self.sh.quotes.as_ref(),
            ids: &self.sh.ids,
            now: self.sh.clock.now(),
            max_quote_age_ms: self.sh.max_quote_age_ms,
            restrictions: Some(&self.sh.restrictions),
            options: self.sh.options.as_ref(),
            corp: Some(&self.sh.corp),
            risk: Some(&self.sh.risk),
        }
    }

    async fn open(&mut self, account: Account, credentials: (String, String), actor: &str) -> Result<Value, ExecError> {
        let login = account.login;
        if self.states.contains_key(&login) {
            return Err(ExecError::Internal(format!("login {login} already exists")));
        }
        let t = self.sh.registry.get(account.tenant_id).ok_or_else(|| ExecError::Internal("unknown tenant".into()))?;
        let g = t.groups.get(&account.group).ok_or_else(|| ExecError::Internal("unknown group".into()))?;
        let env = self.env(&t, g);
        let tx = crate::engine::funds::open_account(&env, account);
        let batch = Batch { before_version: 0, actor, events: &tx.events, after: &tx.st, audit: &[], request: None, credentials: Some(credentials), at: env.now };
        persist::commit(&self.sh.pool, batch).await.map_err(|e| {
            self.sh.stats.commit_errors.fetch_add(1, Ordering::Relaxed);
            ExecError::Internal(e.to_string())
        })?;
        self.sh.stats.commits.fetch_add(1, Ordering::Relaxed);
        let v = views::account_json(&env, &tx.st);
        self.states.insert(login, tx.st);
        self.sh.index.write().unwrap().accounts.insert(login, meta(&self.states[&login]));
        self.reindex(login);
        Ok(v)
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute(&mut self, login: i64, actor: &str, staff: Option<&Staff>, reason_code: &str, note: &str, request: Option<Value>, credentials: Option<(String, String)>, op: Op) -> Result<Done, ExecError> {
        let st = self.states.get(&login).ok_or(ExecError::NotFound)?;
        let t = self.sh.registry.get(st.account.tenant_id).ok_or_else(|| ExecError::Internal("unknown tenant".into()))?;
        let g = t.groups.get(&st.account.group).ok_or_else(|| ExecError::Internal(format!("group {} is not configured", st.account.group)))?;
        let env = self.env(&t, g);
        let before_version = st.version;
        let old_tickets: Vec<i64> = st.positions.keys().chain(st.orders.keys()).copied().collect();
        let mut tx = Tx::new(st);
        let value = op(&mut tx, &env).map_err(ExecError::Reject)?;
        if tx.liquidate {
            // the liquidator re-reads the account after this transaction (it waits for its turn in this shard)
            self.sh.books.liquidate(login);
        }
        if tx.events.is_empty() && tx.audit.is_empty() {
            if tx.book_dirty {
                // only the in-memory order-book state changed (working orders, reservations): nothing to commit
                let Tx { st: new_st, notes, book_send, .. } = tx;
                self.push_book(&env, &new_st, &notes);
                self.send_book(book_send);
                self.states.insert(login, new_st);
                self.dirty.insert(login);
                return Ok(Done { value, audit: vec![], notes });
            }
            return Ok(Done { value, audit: vec![], notes: tx.notes });
        }
        let rows: Vec<AuditRow> = tx.audit.iter().map(|a| audit_row(&tx.st, a, staff, reason_code, note, env.now)).collect();
        let batch = Batch { before_version, actor, events: &tx.events, after: &tx.st, audit: &rows, request, credentials, at: env.now };
        let ids = match persist::commit(&self.sh.pool, batch).await {
            Ok(ids) => ids,
            Err(CommitError::DuplicateKey(k)) => return Err(ExecError::Duplicate(k)),
            Err(e) => {
                self.sh.stats.commit_errors.fetch_add(1, Ordering::Relaxed);
                tracing::error!(login, error = %e, events = tx.events.len(), "commit failed; state unchanged");
                return Err(ExecError::Internal(e.to_string()));
            }
        };
        self.sh.stats.commits.fetch_add(1, Ordering::Relaxed);
        let audit: Vec<Value> = ids.iter().zip(rows.iter()).map(|(id, r)| persist::audit_json(*id, r)).collect();
        // --- committed: publish, then swap the state in ---
        if self.sh.streams.watching(login) {
            let equity_usd = crate::engine::metrics(&env, &tx.st).equity / tx.st.account.usd_factor();
            self.sh.streams.tap(Committed { login, tenant_id: tx.st.account.tenant_id, first_version: before_version + 1, at: env.now, events: tx.events.clone(), equity_usd });
        }
        self.publish(&env, &tx, &audit);
        if tx.book_dirty
            && let Some(s) = self.sh.streams.account(login)
        {
            let _ = s.send(Arc::from(tx.st.book.frame().to_string()));
        }
        self.lp_hooks(&tx.events);
        for e in &tx.events {
            tracing::info!(login, version = before_version, kind = e.kind(), actor, "event");
        }
        let Tx { st: new_st, notes, book_send, .. } = tx;
        {
            let mut idx = self.sh.index.write().unwrap();
            for t in old_tickets {
                idx.tickets.remove(&t);
            }
            for t in new_st.positions.keys().chain(new_st.orders.keys()) {
                idx.tickets.insert(*t, login);
            }
            match option_contracts(&new_st) {
                Some(c) => {
                    idx.option_contracts.insert(login, c);
                }
                None => {
                    idx.option_contracts.remove(&login);
                }
            }
            idx.accounts.insert(login, meta(&new_st));
        }
        self.states.insert(login, new_st);
        self.reindex(login);
        self.dirty.insert(login);
        self.send_book(book_send);
        Ok(Done { value, audit, notes })
    }

    /// Order-book commands of a committed transaction go to their book actors (never blocks the shard).
    fn send_book(&self, out: Vec<crate::book::Outgoing>) {
        if out.is_empty() {
            return;
        }
        match self.sh.books.hub.get() {
            Some(hub) => crate::book::send_outgoing(hub, out),
            None => tracing::error!(n = out.len(), "order-book commands dropped: the hub is not registered"),
        }
    }

    /// A book-only change (reservation, removal) still updates the account on the terminal stream.
    fn push_book(&self, env: &Env, st: &AccountState, notes: &[Note]) {
        let Some(s) = self.sh.streams.account(st.login()) else { return };
        for n in notes {
            let _ = s.send(Arc::from(json!({"type": "notification", "kind": n.kind, "message": n.message, "data": n.data}).to_string()));
        }
        let _ = s.send(Arc::from(st.book.frame().to_string()));
        let _ = s.send(Arc::from(json!({"type": "account", "account": views::account_json(env, st)}).to_string()));
    }

    /// Publishes the symbols this shard's accounts hold positions or pending orders on (option underlyings
    /// included), so the feed keeps them streaming at market-data whatever else is watched.
    fn publish_held(&self) {
        self.sh.held.set(self.id, self.interest.keys().map(|(_, s)| s.clone()).collect());
    }

    fn reindex(&mut self, login: i64) {
        if let Some(old) = self.keys.remove(&login) {
            for k in old {
                if let Some(set) = self.interest.get_mut(&k) {
                    set.remove(&login);
                    if set.is_empty() {
                        self.interest.remove(&k);
                    }
                }
            }
        }
        let Some(st) = self.states.get(&login) else { return };
        let Some(t) = self.sh.registry.get(st.account.tenant_id) else { return };
        let Some(g) = t.groups.get(&st.account.group) else { return };
        let mut keys: Vec<(String, String)> = st.symbols().into_iter().map(|s| (g.spread_group.clone(), s)).collect();
        // options follow the raw mid of their underlying (and order triggers their symbol)
        keys.extend(st.option_keys().into_iter().map(|s| (crate::options::RAW.to_string(), s)));
        for k in &keys {
            self.interest.entry(k.clone()).or_default().insert(login);
        }
        self.keys.insert(login, keys);
    }

    async fn on_tick(&mut self, t: Arc<TickMsg>) {
        let key = (t.group.to_string(), t.symbol.to_string());
        let Some(logins) = self.interest.get(&key).cloned() else { return };
        let raw = &*t.group == crate::options::RAW;
        let now = self.sh.clock.now();
        let spot = if raw { self.sh.quotes.get(crate::options::RAW, &t.symbol).map(|q| q.mid()) } else { None };
        for login in logins {
            let sym = t.symbol.clone();
            let op: Op = if raw {
                // options: barrier knocks on every raw tick; orders, premium SL / TP and margin at most every 250 ms
                let k = (login, sym.to_string());
                let throttled = self.last_eval.get(&k).is_some_and(|l| now.timestamp_millis() - l < 250);
                let knock = spot.is_some_and(|s| self.states.get(&login).is_some_and(|st| crate::engine::options::knock_due(st, &sym, s, now)));
                if throttled && !knock {
                    self.pending.insert(k);
                    continue;
                }
                let full = !throttled;
                if full {
                    self.last_eval.insert(k.clone(), now.timestamp_millis());
                    self.pending.remove(&k);
                }
                Box::new(move |tx, env| {
                    crate::engine::options::on_underlying(tx, env, &sym, full);
                    Ok(Value::Null)
                })
            } else {
                Box::new(move |tx, env| {
                    risk::on_tick(tx, env, &sym);
                    Ok(Value::Null)
                })
            };
            if let Err(e) = self.execute(login, "system", None, "", "", None, None, op).await {
                tracing::error!(login, error = ?e, "tick processing failed");
            }
            self.dirty.insert(login);
        }
        let lag = Utc::now().timestamp_millis() - t.recv_ms;
        self.sh.stats.ticks.fetch_add(1, Ordering::Relaxed);
        self.sh.stats.last_tick_lag_ms.store(lag, Ordering::Relaxed);
        self.sh.stats.max_tick_lag_ms.fetch_max(lag, Ordering::Relaxed);
    }

    /// Raw ticks the throttle held back: one full option evaluation each.
    async fn throttled(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let now = self.sh.clock.now().timestamp_millis();
        let due: Vec<(i64, String)> = self.pending.drain().collect();
        for (login, sym) in due {
            self.last_eval.insert((login, sym.clone()), now);
            let op: Op = Box::new(move |tx, env| {
                crate::engine::options::on_underlying(tx, env, &sym, true);
                Ok(Value::Null)
            });
            if let Err(e) = self.execute(login, "system", None, "", "", None, None, op).await {
                tracing::error!(login, error = ?e, "option evaluation failed");
            }
            self.dirty.insert(login);
        }
    }

    /// Every 5 s: accounts with options are re-evaluated without a tick (time decay moves marks and margin,
    /// pending orders expire, triggers on quiet symbols).
    async fn options_timer(&mut self) {
        let logins: Vec<i64> = self.states.values().filter(|s| s.has_options()).map(|s| s.login()).collect();
        if logins.is_empty() {
            return;
        }
        let now = self.sh.clock.now().timestamp_millis();
        self.last_eval.retain(|_, t| now - *t < 60_000);
        for login in logins {
            let op: Op = Box::new(|tx, env| {
                crate::engine::options::on_timer(tx, env);
                Ok(Value::Null)
            });
            if let Err(e) = self.execute(login, "system", None, "", "", None, None, op).await {
                tracing::error!(login, error = ?e, "option timer evaluation failed");
            }
            self.dirty.insert(login);
        }
    }

    async fn expire(&mut self) {
        let now = self.sh.clock.now();
        let due: Vec<i64> = self.states.values().filter(|s| s.orders.values().any(|o| o.expiry_at.is_some_and(|a| a <= now))).map(|s| s.login()).collect();
        for login in due {
            let op: Op = Box::new(|tx, env| {
                risk::expire_orders(tx, env);
                Ok(Value::Null)
            });
            if let Err(e) = self.execute(login, "system", None, "", "", None, None, op).await {
                tracing::error!(login, error = ?e, "order expiry failed");
            }
        }
    }

    /* ---------------- streams ---------------- */

    fn publish(&self, env: &Env, tx: &Tx, audit: &[Value]) {
        let login = tx.st.login();
        let acct = self.sh.streams.account(login);
        let desk = self.sh.streams.dealing(tx.st.account.tenant_id);
        if acct.is_none() && desk.is_none() {
            return;
        }
        let mut frames: Vec<Value> = Vec::new();
        let mut desk_frames: Vec<Value> = Vec::new();
        for e in &tx.events {
            match e {
                Event::PositionOpened { position, deal } | Event::PositionUpdated { position, deal, .. } => {
                    let cur = tx.st.positions.get(&position.ticket).unwrap_or(position);
                    frames.push(json!({"type": "position", "op": "upsert", "position": views::position_json(env, &tx.st, cur)}));
                    desk_frames.push(json!({"type": "position", "op": "upsert", "position": views::desk_position_json(env, &tx.st, cur)}));
                    if let Some(d) = deal {
                        frames.push(json!({"type": "deal", "deal": views::deal_json(d)}));
                    }
                }
                Event::PositionClosed { deal, position } => {
                    frames.push(json!({"type": "deal", "deal": views::deal_json(deal)}));
                    desk_frames.push(json!({"type": "deal", "deal": views::desk_deal_json(deal, tx.st.account.user_id, false)}));
                    match position {
                        Some(p) => {
                            let cur = tx.st.positions.get(&p.ticket).unwrap_or(p);
                            frames.push(json!({"type": "position", "op": "upsert", "position": views::position_json(env, &tx.st, cur)}));
                            desk_frames.push(json!({"type": "position", "op": "upsert", "position": views::desk_position_json(env, &tx.st, cur)}));
                        }
                        None => {
                            frames.push(json!({"type": "position", "op": "remove", "ticket": deal.position_ticket}));
                            desk_frames.push(json!({"type": "position", "op": "remove", "ticket": deal.position_ticket.to_string()}));
                        }
                    }
                }
                Event::PositionRemoved { ticket, .. } => {
                    frames.push(json!({"type": "position", "op": "remove", "ticket": ticket}));
                    desk_frames.push(json!({"type": "position", "op": "remove", "ticket": ticket.to_string()}));
                }
                Event::OrderPlaced { order } | Event::OrderUpdated { order, .. } => {
                    if let Some(o) = tx.st.orders.get(&order.ticket) {
                        frames.push(json!({"type": "order", "op": "upsert", "order": views::order_json(o)}));
                        desk_frames.push(json!({"type": "order", "op": "upsert", "order": views::desk_order_json(&tx.st, o)}));
                    }
                }
                Event::OrderRemoved { ticket, status, reason, .. } => {
                    frames.push(json!({"type": "order", "op": "remove", "ticket": ticket, "status": status.as_str(), "reason": reason}));
                    desk_frames.push(json!({"type": "order", "op": "remove", "ticket": ticket.to_string(), "status": status.as_str()}));
                }
                Event::Ledger { txn } => {
                    frames.push(json!({"type": "ledger", "txn": {"id": txn.id, "kind": txn.kind.as_str(), "amount": crate::money::num(txn.effect(login, "balance") + txn.effect(login, "credit") + txn.effect(login, "bonus")), "at": txn.at}}));
                }
                _ => {}
            }
        }
        for n in &tx.notes {
            frames.push(json!({"type": "notification", "kind": n.kind, "message": n.message, "data": n.data}));
        }
        frames.push(json!({"type": "account", "account": views::account_json(env, &tx.st)}));
        if let Some(s) = acct {
            for f in frames {
                let _ = s.send(Arc::from(f.to_string()));
            }
        }
        if let Some(s) = desk {
            for a in audit {
                desk_frames.push(json!({"type": "audit", "entry": a}));
            }
            for f in desk_frames {
                let _ = s.send(Arc::from(f.to_string()));
            }
        }
    }

    /// Throttled (250 ms) equity + floating P&L frames for streamed accounts whose prices moved.
    fn push_equity(&mut self) {
        let dirty: Vec<i64> = self.dirty.drain().collect();
        for login in dirty {
            let Some(tx) = self.sh.streams.account(login) else { continue };
            let Some(st) = self.states.get(&login) else { continue };
            let Some(t) = self.sh.registry.get(st.account.tenant_id) else { continue };
            let Some(g) = t.groups.get(&st.account.group) else { continue };
            let env = self.env(&t, g);
            let m = crate::engine::metrics(&env, st);
            let items: Vec<Value> = st
                .positions
                .values()
                .map(|p| {
                    let (px, pr) = views::current(&env, st, p);
                    let mut v = json!({"ticket": p.ticket, "price": crate::money::num_opt(px), "profit": crate::money::num_opt(pr), "swap": crate::money::num(p.swap)});
                    if let Some(t) = &p.option {
                        let q = crate::engine::options::mark_of(&env, &st.account, t);
                        v["mark"] = crate::money::num_opt(q.map(|q| q.mark));
                        v["greeks"] = crate::engine::options::greeks_json(crate::engine::options::position_greeks(&env, p));
                    }
                    v
                })
                .collect();
            let mut f = views::metrics_json(&m);
            f["type"] = json!("equity");
            f["login"] = json!(login);
            f["positions"] = json!(items);
            let _ = tx.send(Arc::from(f.to_string()));
        }
    }

    fn push_dealing_pnl(&self) {
        let mut by_tenant: HashMap<i64, Vec<Value>> = HashMap::new();
        for st in self.states.values().filter(|s| !s.positions.is_empty()) {
            if self.sh.streams.dealing(st.account.tenant_id).is_none() {
                continue;
            }
            let Some(t) = self.sh.registry.get(st.account.tenant_id) else { continue };
            let Some(g) = t.groups.get(&st.account.group) else { continue };
            let env = self.env(&t, g);
            for p in st.positions.values() {
                // options included: views::current prices them from the mark cache
                let (Some(px), Some(pr)) = views::current(&env, st, p) else { continue };
                by_tenant.entry(st.account.tenant_id).or_default().push(json!({"ticket": p.ticket.to_string(), "price": crate::money::num(px), "profit": crate::money::num(pr)}));
            }
        }
        for (tenant, items) in by_tenant {
            if let Some(s) = self.sh.streams.dealing(tenant) {
                let _ = s.send(Arc::from(json!({"type": "pnl", "items": items}).to_string()));
            }
        }
    }

    /// A-book exposure (new A fills, splits and transfers to A) is offered to the LP adapter after commit.
    fn lp_hooks(&self, events: &[Event]) {
        for e in events {
            let (p, what) = match e {
                Event::PositionOpened { position, deal } if position.book == Book::A => (position, if deal.is_some() { "fill" } else { "split" }),
                Event::PositionUpdated { position, change, .. } if position.book == Book::A && change.starts_with("book ") => (position, "transfer"),
                _ => continue,
            };
            if let Err(err) = self.sh.lp.hedge(p.login, p.ticket, &p.symbol, p.side.as_str(), &p.volume.to_string()) {
                tracing::debug!(ticket = p.ticket, what, error = %err, "LP hedge not placed");
            }
        }
    }
}

fn audit_row(st: &AccountState, a: &AuditDraft, staff: Option<&Staff>, reason_code: &str, note: &str, at: DateTime<Utc>) -> AuditRow {
    let s = staff.cloned().unwrap_or(Staff { id: "system".into(), name: "System".into(), role: "system".into() });
    AuditRow {
        tenant_id: st.account.tenant_id,
        at,
        staff_id: s.id,
        staff_name: s.name,
        staff_role: s.role,
        action: a.action.to_string(),
        tickets: a.tickets.iter().map(|t| t.to_string()).collect(),
        login: a.login.or(Some(st.login())),
        symbol: a.symbol.clone(),
        before: a.before.clone(),
        after: a.after.clone(),
        reason_code: reason_code.to_string(),
        note: note.to_string(),
        flags: a.flags.clone(),
    }
}

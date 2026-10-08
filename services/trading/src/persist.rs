//! PostgreSQL: connection + migrations, loading configuration, replaying event streams on start, and
//! committing a batch of account events with all their projections (orders, positions, deals, ledger,
//! account row, audit rows) in ONE database transaction.

use chrono::{DateTime, Utc};
use futures_util::TryStreamExt;
use serde_json::{Value, json};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{ConnectOptions, PgPool, Postgres, Row, Transaction};
use std::collections::HashMap;
use std::str::FromStr;

use crate::model::{Book, Deal, LedgerTxn, Mode, Order, Position};
use crate::money::D;
use crate::rules::{Group, RoutingRule, SymbolControl, TenantConfig, TenantPolicy};
use crate::state::{AccountState, Event};

/// Connects, creating the database on first run, and applies migrations.
pub async fn connect(url: &str) -> anyhow::Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)?;
    let db = opts.get_database().unwrap_or("kalks_trading").to_string();
    let admin = opts.clone().database("postgres");
    let mut conn = admin.connect().await?;
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1").bind(&db).fetch_optional(&mut conn).await?;
    if exists.is_none() {
        sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{}\"", db.replace('"', "")))).execute(&mut conn).await?;
        tracing::info!(%db, "created database");
    }
    drop(conn);
    let pool = PgPoolOptions::new().max_connections(24).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

/* ------------------------------------------------------------------ */
/* Configuration                                                       */
/* ------------------------------------------------------------------ */

fn mode(s: &str) -> Mode {
    if s == "netting" { Mode::Netting } else { Mode::Hedging }
}

pub fn group_from_row(r: &sqlx::postgres::PgRow) -> Group {
    Group {
        tenant_id: r.get("tenant_id"),
        code: r.get("code"),
        name: r.get("name"),
        mode: mode(r.get::<String, _>("mode").as_str()),
        cent: r.get("cent"),
        account_types: r.get("account_types"),
        leverages: r.get::<Vec<i32>, _>("leverages").into_iter().map(|v| v.max(1) as u32).collect(),
        default_leverage: r.get::<i32, _>("default_leverage").max(1) as u32,
        margin_call_pct: r.get("margin_call_pct"),
        stop_out_pct: r.get("stop_out_pct"),
        hedged_margin_pct: r.get("hedged_margin_pct"),
        min_deposit: r.get("min_deposit"),
        swap_free: r.get("swap_free"),
        commission_per_lot: r.get("commission_per_lot"),
        route: if r.get::<String, _>("route") == "A" { Book::A } else { Book::B },
        spread_group: r.get("spread_group"),
        max_accounts_per_user: r.get::<i32, _>("max_accounts_per_user").max(0) as u32,
        demo_initial_balance: r.get("demo_initial_balance"),
        demo_refills_per_day: r.get::<i32, _>("demo_refills_per_day").max(0) as u32,
        demo_expiry_days: r.get::<i32, _>("demo_expiry_days").max(1) as u32,
        enabled: r.get("enabled"),
        product: crate::rules::Product::parse(&r.try_get::<String, _>("product").unwrap_or_default()),
    }
}

pub async fn load_tenant(pool: &PgPool, tenant_id: i64) -> anyhow::Result<TenantConfig> {
    let t = sqlx::query("SELECT id, slug FROM tenants WHERE id = $1").bind(tenant_id).fetch_one(pool).await?;
    let mut cfg = TenantConfig { tenant_id, slug: t.get("slug"), ..Default::default() };
    for r in sqlx::query("SELECT * FROM groups WHERE tenant_id = $1").bind(tenant_id).fetch_all(pool).await? {
        let g = group_from_row(&r);
        cfg.groups.insert(g.code.clone(), g);
    }
    if let Some(p) = sqlx::query("SELECT * FROM tenant_policies WHERE tenant_id = $1").bind(tenant_id).fetch_optional(pool).await? {
        cfg.policy = TenantPolicy {
            exec_delay_enabled: p.get("exec_delay_enabled"),
            exec_delay_cap_ms: p.get::<i32, _>("exec_delay_cap_ms").max(0) as u32,
            margin_call_pct: p.get("margin_call_pct"),
            stop_out_pct: p.get("stop_out_pct"),
        };
    }
    for r in sqlx::query("SELECT * FROM symbol_controls WHERE tenant_id = $1 ORDER BY at DESC").bind(tenant_id).fetch_all(pool).await? {
        let mode = if r.get::<String, _>("mode") == "halt" { crate::rules::ControlMode::Halt } else { crate::rules::ControlMode::CloseOnly };
        cfg.symbol_controls.push(SymbolControl { id: r.get("id"), symbol: r.get("symbol"), group: r.get("group_code"), mode, reason_code: r.get("reason_code"), note: r.get("note"), staff: r.get("staff"), at: r.get("at") });
    }
    if let Some(r) = sqlx::query("SELECT rules FROM routing_rules WHERE tenant_id = $1").bind(tenant_id).fetch_optional(pool).await? {
        let v: sqlx::types::Json<Vec<RoutingRule>> = r.get("rules");
        cfg.routing_rules = v.0;
    }
    Ok(cfg)
}

pub async fn load_registry(pool: &PgPool) -> anyhow::Result<Vec<TenantConfig>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM tenants ORDER BY id").fetch_all(pool).await?;
    let mut out = Vec::new();
    for id in ids {
        out.push(load_tenant(pool, id).await?);
    }
    Ok(out)
}

/* ------------------------------------------------------------------ */
/* Replay                                                              */
/* ------------------------------------------------------------------ */

/// Rebuilds every account by replaying its event stream (ordered by login, version). Versions must be
/// contiguous from 1; a gap or a stream not starting with account_opened aborts the start.
pub async fn replay_all(pool: &PgPool) -> anyhow::Result<HashMap<i64, AccountState>> {
    let mut out: HashMap<i64, AccountState> = HashMap::new();
    let mut rows = sqlx::query("SELECT login, version, payload FROM events ORDER BY login, version").fetch(pool);
    let mut cur: Option<AccountState> = None;
    while let Some(r) = rows.try_next().await? {
        let login: i64 = r.get("login");
        let version: i64 = r.get("version");
        let payload: sqlx::types::Json<Event> = r.get("payload");
        if cur.as_ref().is_some_and(|s| s.login() != login) {
            let s = cur.take().unwrap();
            out.insert(s.login(), s);
        }
        match (&mut cur, payload.0) {
            (None, Event::AccountOpened { account }) if version == 1 => cur = Some(AccountState::new(account)),
            (None, _) => anyhow::bail!("stream {login} does not start with account_opened at version 1"),
            (Some(s), ev) => {
                if version != s.version + 1 {
                    anyhow::bail!("stream {login}: version gap ({} → {version})", s.version);
                }
                s.apply(&ev);
            }
        }
    }
    if let Some(s) = cur {
        out.insert(s.login(), s);
    }
    Ok(out)
}

/// Highest ids in use: (ticket, deal, txn, live login, demo login).
pub async fn max_ids(pool: &PgPool) -> anyhow::Result<(i64, i64, i64, i64, i64)> {
    let r = sqlx::query(
        "SELECT GREATEST((SELECT max(ticket) FROM orders), (SELECT max(ticket) FROM positions), (SELECT max(id) FROM book_orders),
                         (SELECT max(GREATEST(maker_order, taker_order)) FROM book_fills), 1000000) AS ticket,
                GREATEST((SELECT max(id) FROM deals), 2000000) AS deal,
                COALESCE((SELECT max(id) FROM ledger_txns), 0) AS txn,
                GREATEST((SELECT max(login) FROM accounts WHERE kind = 'live'), 10000000) AS live,
                GREATEST((SELECT max(login) FROM accounts WHERE kind = 'demo'), 50000000) AS demo",
    )
    .fetch_one(pool)
    .await?;
    Ok((r.get("ticket"), r.get("deal"), r.get("txn"), r.get("live"), r.get("demo")))
}

/* ------------------------------------------------------------------ */
/* Commit                                                              */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
pub struct AuditRow {
    pub tenant_id: i64,
    pub at: DateTime<Utc>,
    pub staff_id: String,
    pub staff_name: String,
    pub staff_role: String,
    pub action: String,
    pub tickets: Vec<String>,
    pub login: Option<i64>,
    pub symbol: Option<String>,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub reason_code: String,
    pub note: String,
    pub flags: Vec<String>,
}

/// Why a commit failed.
#[derive(Debug)]
pub enum CommitError {
    /// The ledger idempotency key already exists (a concurrent duplicate request won).
    DuplicateKey(String),
    Db(anyhow::Error),
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CommitError::DuplicateKey(k) => write!(f, "duplicate idempotency key {k}"),
            CommitError::Db(e) => write!(f, "{e}"),
        }
    }
}

impl<E: Into<anyhow::Error>> From<E> for CommitError {
    fn from(e: E) -> Self {
        CommitError::Db(e.into())
    }
}

pub struct Batch<'a> {
    pub before_version: i64,
    pub actor: &'a str,
    pub events: &'a [Event],
    pub after: &'a AccountState,
    pub audit: &'a [AuditRow],
    /// Wallet transfer request fingerprint stored with its ledger txn (idempotent replays compare it).
    pub request: Option<Value>,
    /// New account: (trading hash, investor hash).
    pub credentials: Option<(String, String)>,
    pub at: DateTime<Utc>,
}

/// Returns the audit row ids.
pub async fn commit(pool: &PgPool, b: Batch<'_>) -> Result<Vec<i64>, CommitError> {
    let mut tx = pool.begin().await?;
    let st = b.after;
    let tenant = st.account.tenant_id;
    let login = st.login();
    for (i, ev) in b.events.iter().enumerate() {
        let version = b.before_version + 1 + i as i64;
        sqlx::query("INSERT INTO events (tenant_id, login, version, kind, actor, payload, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7)")
            .bind(tenant)
            .bind(login)
            .bind(version)
            .bind(ev.kind())
            .bind(b.actor)
            .bind(sqlx::types::Json(ev))
            .bind(b.at)
            .execute(&mut *tx)
            .await?;
        project(&mut tx, tenant, ev, b.request.as_ref()).await?;
    }
    upsert_account(&mut tx, st).await?;
    if let Some((trading, investor)) = &b.credentials {
        sqlx::query("INSERT INTO account_credentials (login, tenant_id, trading_hash, investor_hash) VALUES ($1,$2,$3,$4)")
            .bind(login)
            .bind(tenant)
            .bind(trading)
            .bind(investor)
            .execute(&mut *tx)
            .await?;
    }
    let mut ids = Vec::new();
    for a in b.audit {
        ids.push(insert_audit(&mut *tx, a).await?);
    }
    match tx.commit().await {
        Ok(()) => Ok(ids),
        Err(e) => Err(CommitError::Db(e.into())),
    }
}

pub async fn insert_audit<'e, E: sqlx::PgExecutor<'e>>(ex: E, a: &AuditRow) -> anyhow::Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO audit_log (tenant_id, at, staff_id, staff_name, staff_role, action, tickets, login, symbol, before, after, reason_code, note, flags)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) RETURNING id",
    )
    .bind(a.tenant_id)
    .bind(a.at)
    .bind(&a.staff_id)
    .bind(&a.staff_name)
    .bind(&a.staff_role)
    .bind(&a.action)
    .bind(&a.tickets)
    .bind(a.login)
    .bind(&a.symbol)
    .bind(a.before.clone().map(sqlx::types::Json))
    .bind(a.after.clone().map(sqlx::types::Json))
    .bind(&a.reason_code)
    .bind(&a.note)
    .bind(&a.flags)
    .fetch_one(ex)
    .await?;
    Ok(id)
}

async fn upsert_account(tx: &mut Transaction<'_, Postgres>, st: &AccountState) -> anyhow::Result<()> {
    let a = &st.account;
    sqlx::query(
        "INSERT INTO accounts (login, tenant_id, user_id, kind, group_code, mode, cent, currency, leverage, status, name, route_override, controls, demo,
                               balance, credit, bonus, margin_call, version, created_at, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20, now())
         ON CONFLICT (login) DO UPDATE SET user_id = EXCLUDED.user_id, group_code = EXCLUDED.group_code, mode = EXCLUDED.mode, leverage = EXCLUDED.leverage,
            status = EXCLUDED.status, name = EXCLUDED.name, route_override = EXCLUDED.route_override, controls = EXCLUDED.controls, demo = EXCLUDED.demo,
            balance = EXCLUDED.balance, credit = EXCLUDED.credit, bonus = EXCLUDED.bonus, margin_call = EXCLUDED.margin_call, version = EXCLUDED.version, updated_at = now()",
    )
    .bind(a.login)
    .bind(a.tenant_id)
    .bind(a.user_id)
    .bind(a.kind.as_str())
    .bind(&a.group)
    .bind(if a.mode == Mode::Netting { "netting" } else { "hedging" })
    .bind(a.cent)
    .bind(a.ccy())
    .bind(a.leverage as i32)
    .bind(a.status.as_str())
    .bind(&a.name)
    .bind(a.route_override.map(|b| b.as_str()))
    .bind(sqlx::types::Json(&a.controls))
    .bind(a.demo.as_ref().map(sqlx::types::Json))
    .bind(st.balance)
    .bind(st.credit)
    .bind(st.bonus)
    .bind(st.margin_call)
    .bind(st.version)
    .bind(a.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn project(tx: &mut Transaction<'_, Postgres>, tenant: i64, ev: &Event, request: Option<&Value>) -> Result<(), CommitError> {
    match ev {
        Event::AccountOpened { .. } | Event::AccountUpdated { .. } | Event::MarginCall { .. } | Event::StopOut { .. } | Event::RefillCounted { .. } | Event::CorporateAction { .. } => {}
        Event::Ledger { txn } => insert_txn(tx, txn, request).await?,
        Event::OrderPlaced { order } | Event::OrderUpdated { order, .. } => upsert_order(tx, tenant, order).await?,
        Event::OrderRemoved { ticket, status, reason, at, fill_price, position_ticket } => {
            sqlx::query("UPDATE orders SET status = $2, reason = $3, done_at = $4, fill_price = $5, position_ticket = $6, updated_at = now() WHERE ticket = $1")
                .bind(ticket)
                .bind(status.as_str())
                .bind(reason)
                .bind(at)
                .bind(fill_price)
                .bind(position_ticket)
                .execute(&mut **tx)
                .await?;
        }
        Event::PositionOpened { position, deal } | Event::PositionUpdated { position, deal, .. } => {
            upsert_position(tx, tenant, position, "open", None).await?;
            if let Some(d) = deal {
                insert_deal(tx, tenant, d).await?;
            }
            if let Event::PositionUpdated { change, .. } = ev
                && change == "knocked_in"
                && let Some(t) = &position.option
                && let Some(b) = &t.barrier
            {
                insert_knock(tx, tenant, position.ticket, position.login, t, b, b.knock_spot, "knock_in", None, b.knocked_at.unwrap_or_else(Utc::now)).await?;
            }
        }
        Event::PositionClosed { deal, position } => {
            insert_deal(tx, tenant, deal).await?;
            if deal.reason == crate::model::DealReason::KnockOut
                && let Some(o) = &deal.option
                && let Some(b) = &o.terms.barrier
            {
                insert_knock(tx, tenant, deal.position_ticket, deal.login, &o.terms, b, o.spot, "knock_out", Some(deal.id), deal.time).await?;
            }
            match position {
                Some(p) => upsert_position(tx, tenant, p, "open", None).await?,
                None => {
                    sqlx::query("UPDATE positions SET status = 'closed', closed_at = $2, updated_at = now() WHERE ticket = $1").bind(deal.position_ticket).bind(deal.time).execute(&mut **tx).await?;
                }
            }
        }
        Event::PositionRemoved { ticket, .. } => {
            sqlx::query("UPDATE positions SET status = 'voided', closed_at = now(), updated_at = now() WHERE ticket = $1").bind(ticket).execute(&mut **tx).await?;
        }
        Event::DealReversed { deal_id } => {
            sqlx::query("UPDATE deals SET reversed = true WHERE id = $1").bind(deal_id).execute(&mut **tx).await?;
        }
    }
    Ok(())
}

async fn insert_txn(tx: &mut Transaction<'_, Postgres>, t: &LedgerTxn, request: Option<&Value>) -> Result<(), CommitError> {
    if !t.is_balanced() {
        return Err(CommitError::Db(anyhow::anyhow!("unbalanced ledger txn {}", t.id)));
    }
    let res = sqlx::query(
        "INSERT INTO ledger_txns (id, tenant_id, idempotency_key, kind, login, reference, reason_code, note, request, created_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (tenant_id, idempotency_key) DO NOTHING",
    )
    .bind(t.id)
    .bind(t.tenant_id)
    .bind(&t.idempotency_key)
    .bind(t.kind.as_str())
    .bind(t.login)
    .bind(&t.reference)
    .bind(&t.reason_code)
    .bind(&t.note)
    .bind(request.map(sqlx::types::Json))
    .bind(t.at)
    .execute(&mut **tx)
    .await?;
    if res.rows_affected() == 0 {
        return Err(CommitError::DuplicateKey(t.idempotency_key.clone()));
    }
    for p in &t.postings {
        sqlx::query("INSERT INTO ledger_postings (tenant_id, txn_id, account_code, currency, amount) VALUES ($1,$2,$3,$4,$5)")
            .bind(t.tenant_id)
            .bind(t.id)
            .bind(&p.account)
            .bind(&p.ccy)
            .bind(p.amount)
            .execute(&mut **tx)
            .await?;
        sqlx::query(
            "INSERT INTO ledger_accounts (tenant_id, code, currency, balance) VALUES ($1,$2,$3,$4)
             ON CONFLICT (tenant_id, code) DO UPDATE SET balance = ledger_accounts.balance + EXCLUDED.balance, updated_at = now()",
        )
        .bind(t.tenant_id)
        .bind(&p.account)
        .bind(&p.ccy)
        .bind(p.amount)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// A barrier knock, recorded once per position (risk desk, audit).
#[allow(clippy::too_many_arguments)]
async fn insert_knock(tx: &mut Transaction<'_, Postgres>, tenant: i64, ticket: i64, login: i64, t: &crate::model::OptionTerms, b: &crate::model::BarrierTerms, spot: Option<D>, effect: &str, deal: Option<i64>, at: DateTime<Utc>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO option_knocks (ticket, tenant_id, login, series, underlying, kind, level, rebate, spot, effect, deal_id, knocked_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) ON CONFLICT (ticket) DO NOTHING",
    )
    .bind(ticket)
    .bind(tenant)
    .bind(login)
    .bind(&t.series)
    .bind(&t.underlying)
    .bind(b.kind.as_str())
    .bind(b.level)
    .bind(b.rebate)
    .bind(spot)
    .bind(effect)
    .bind(deal)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_order(tx: &mut Transaction<'_, Postgres>, tenant: i64, o: &Order) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO orders (ticket, tenant_id, login, symbol, side, kind, status, volume, price, stop_limit, sl, tp, triggered, expiry, expiry_at, oco,
                             source, platform, comment, book, client_order_id, placed_at, data, option, combo_id, trigger)
         VALUES ($1,$2,$3,$4,$5,$6,'pending',$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25)
         ON CONFLICT (ticket) DO UPDATE SET volume = EXCLUDED.volume, price = EXCLUDED.price, stop_limit = EXCLUDED.stop_limit, sl = EXCLUDED.sl, tp = EXCLUDED.tp,
            triggered = EXCLUDED.triggered, expiry = EXCLUDED.expiry, expiry_at = EXCLUDED.expiry_at, oco = EXCLUDED.oco, book = EXCLUDED.book, data = EXCLUDED.data,
            option = EXCLUDED.option, combo_id = EXCLUDED.combo_id, trigger = EXCLUDED.trigger, updated_at = now()",
    )
    .bind(o.ticket)
    .bind(tenant)
    .bind(o.login)
    .bind(&o.symbol)
    .bind(o.side.as_str())
    .bind(o.kind.as_str())
    .bind(o.volume)
    .bind(o.price)
    .bind(o.stop_limit)
    .bind(o.sl)
    .bind(o.tp)
    .bind(o.triggered)
    .bind(serde_json::to_value(o.expiry)?.as_str().unwrap_or("gtc").to_string())
    .bind(o.expiry_at)
    .bind(o.oco)
    .bind(o.source.as_str())
    .bind(&o.platform)
    .bind(&o.comment)
    .bind(o.book.map(|b| b.as_str()))
    .bind(&o.client_order_id)
    .bind(o.placed_at)
    .bind(sqlx::types::Json(o))
    .bind(o.option.as_ref().map(sqlx::types::Json))
    .bind(o.combo_id)
    .bind(o.trigger.as_ref().map(sqlx::types::Json))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_position(tx: &mut Transaction<'_, Postgres>, tenant: i64, p: &Position, status: &str, closed_at: Option<DateTime<Utc>>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO positions (ticket, tenant_id, login, symbol, side, volume, open_price, open_time, sl, tp, swap, commission, source, book, parent_ticket, status, closed_at, data, option, combo_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)
         ON CONFLICT (ticket) DO UPDATE SET volume = EXCLUDED.volume, open_price = EXCLUDED.open_price, sl = EXCLUDED.sl, tp = EXCLUDED.tp, swap = EXCLUDED.swap,
            commission = EXCLUDED.commission, book = EXCLUDED.book, status = EXCLUDED.status, closed_at = EXCLUDED.closed_at, data = EXCLUDED.data,
            option = EXCLUDED.option, combo_id = EXCLUDED.combo_id, updated_at = now()",
    )
    .bind(p.ticket)
    .bind(tenant)
    .bind(p.login)
    .bind(&p.symbol)
    .bind(p.side.as_str())
    .bind(p.volume)
    .bind(p.open_price)
    .bind(p.open_time)
    .bind(p.sl)
    .bind(p.tp)
    .bind(p.swap)
    .bind(p.commission)
    .bind(p.source.as_str())
    .bind(p.book.as_str())
    .bind(p.parent_ticket)
    .bind(status)
    .bind(closed_at)
    .bind(sqlx::types::Json(p))
    .bind(p.option.as_ref().map(sqlx::types::Json))
    .bind(p.combo_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_deal(tx: &mut Transaction<'_, Postgres>, tenant: i64, d: &Deal) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO deals (id, tenant_id, login, position_ticket, order_ticket, symbol, side, position_side, entry, volume, price, profit, swap, commission,
                            reason, book, time, open_price, open_time, source, comment, price_correction, ledger_txn, staff, reason_code, data, option)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27)",
    )
    .bind(d.id)
    .bind(tenant)
    .bind(d.login)
    .bind(d.position_ticket)
    .bind(d.order_ticket)
    .bind(&d.symbol)
    .bind(d.side.as_str())
    .bind(d.position_side.as_str())
    .bind(d.entry.as_str())
    .bind(d.volume)
    .bind(d.price)
    .bind(d.profit)
    .bind(d.swap)
    .bind(d.commission)
    .bind(d.reason.as_str())
    .bind(d.book.as_str())
    .bind(d.time)
    .bind(d.open_price)
    .bind(d.open_time)
    .bind(d.source.as_str())
    .bind(&d.comment)
    .bind(d.price_correction)
    .bind(d.ledger_txn)
    .bind(&d.staff)
    .bind(&d.reason_code)
    .bind(sqlx::types::Json(d))
    .bind(d.option.as_ref().map(sqlx::types::Json))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn load_deal(pool: &PgPool, id: i64) -> anyhow::Result<Option<Deal>> {
    let r = sqlx::query("SELECT data FROM deals WHERE id = $1").bind(id).fetch_optional(pool).await?;
    Ok(r.map(|r| r.get::<sqlx::types::Json<Deal>, _>("data").0))
}

/// Ledger consistency check after replay: every client sub-ledger balance in `ledger_accounts` must equal
/// the replayed state (and Σ postings). Returns the mismatches.
pub async fn verify_balances(pool: &PgPool, states: &HashMap<i64, AccountState>) -> anyhow::Result<Vec<String>> {
    let mut bad = Vec::new();
    let rows = sqlx::query("SELECT code, balance FROM ledger_accounts WHERE code LIKE 'acct:%'").fetch_all(pool).await?;
    let mut seen: HashMap<String, D> = HashMap::new();
    for r in rows {
        seen.insert(r.get("code"), r.get("balance"));
    }
    for st in states.values() {
        for (sub, v) in [("balance", st.balance), ("credit", st.credit), ("bonus", st.bonus)] {
            let code = crate::model::acct_code(st.login(), sub);
            let db = seen.get(&code).copied().unwrap_or_default();
            if db != v {
                bad.push(format!("{code}: ledger {db} vs replay {v}"));
            }
        }
    }
    let unbalanced: Vec<i64> = sqlx::query_scalar("SELECT txn_id FROM ledger_postings GROUP BY txn_id, currency HAVING sum(amount) <> 0 LIMIT 10").fetch_all(pool).await?;
    for t in unbalanced {
        bad.push(format!("txn {t} does not balance"));
    }
    Ok(bad)
}

pub fn audit_json(id: i64, a: &AuditRow) -> Value {
    json!({
        "id": format!("AUD-{id:06}"),
        "at": a.at,
        "staff": {"id": a.staff_id, "name": a.staff_name, "role": a.staff_role},
        "action": a.action,
        "tickets": a.tickets,
        "login": a.login.map(|l| l.to_string()),
        "symbol": a.symbol,
        "before": a.before,
        "after": a.after,
        "reasonCode": a.reason_code,
        "note": a.note,
        "flags": if a.flags.is_empty() { Value::Null } else { json!(a.flags) },
    })
}

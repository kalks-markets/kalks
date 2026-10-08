//! Mirrors the source services into kalks_reports.
//!
//! | Loop | Source | Period |
//! |---|---|---|
//! | clients | gateway `/v1/internal/referrals/users` (keyset `(changed_at, id)`) | every pass |
//! | accounts | engine `/v1/admin/accounts` (all pages): dimensions + live metrics; an account whose engine `version` changed gets its deals (`/history`) and ledger (`/ledger`) pulled incrementally | every pass |
//! | snapshots | today's end-of-day row per account is upserted from the live metrics until the server day ends; the first sync of an account backfills past days from its ledger | every pass |
//! | wallet | wallet `/v1/admin/deposits`, `/v1/admin/withdrawals` down to the oldest open item | every 5th pass |
//! | ib | IB `/v1/ib/admin/commissions` (35-day re-scan) | every 5th pass |
//! | specs | engine `/v1/admin/groups` (spread groups) + market-data `/v1/admin/spreads` (markups) | every 10th pass |
//!
//! Every write is an idempotent upsert keyed by the source id, so overlaps and retries are free.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::Row;

use crate::db;
use crate::state::App;
use crate::time;
use crate::upstream::{As, Target, dec, time as jtime};

/// Ledger kinds that move money in or out of an account (not trading results or charges). Kalks FX Options
/// premiums (`option_premium`) and settlements (`option_settlement`) are trading flows, never deposits or
/// withdrawals.
pub const FLOW_KINDS: &[&str] = &["transfer_in", "transfer_out", "deposit", "withdrawal", "demo_initial", "demo_refill"];

/// The `option` object of an engine deal JSON (`instrument: "option"`), or a minimal one for an option series
/// symbol when the feed does not carry it. None for CFD deals.
pub fn deal_option(d: &Value) -> Option<Value> {
    if d["option"].is_object() {
        return Some(d["option"].clone());
    }
    let sym = d["symbol"].as_str().unwrap_or("");
    (d["instrument"].as_str() == Some("option") || crate::statement::is_option_series(sym)).then(|| json!({"series": sym}))
}

pub async fn run(app: App) {
    let mut pass: u64 = 0;
    loop {
        if pass % 10 == 0 {
            for t in app.cfg.tenants.clone() {
                if let Err(e) = sync_specs(&app, &t).await {
                    tracing::warn!(error = %e, "specs sync failed");
                }
            }
        }
        if let Err(e) = sync_clients(&app).await {
            tracing::warn!(error = %e, "clients sync failed");
        }
        for t in app.cfg.tenants.clone() {
            if let Err(e) = sync_accounts(&app, &t).await {
                tracing::warn!(error = %e, tenant = %t, "accounts sync failed");
            }
            if pass % 5 == 0 {
                if let Err(e) = sync_wallet(&app, &t).await {
                    tracing::warn!(error = %e, tenant = %t, "wallet sync failed");
                }
                if let Err(e) = sync_ib(&app, &t).await {
                    tracing::debug!(error = %e, tenant = %t, "ib sync failed");
                }
            }
        }
        pass += 1;
        tokio::time::sleep(std::time::Duration::from_secs(app.cfg.sync_secs)).await;
    }
}

/* ------------------------------------------------------------------ */
/* Specs                                                               */
/* ------------------------------------------------------------------ */

pub async fn sync_specs(app: &App, tenant: &str) -> anyhow::Result<()> {
    let g = app.up.get(Target::Engine, tenant, As::Staff, "/v1/admin/groups").await?;
    app.specs.set_groups(g["groups"].as_array().map(Vec::as_slice).unwrap_or(&[]));
    if !app.cfg.market_data_admin_token.is_empty() {
        match app.up.get(Target::MarketData, tenant, As::None, "/v1/admin/spreads").await {
            Ok(v) => app.specs.set_markups(v.as_array().map(Vec::as_slice).unwrap_or(&[])),
            Err(e) => tracing::debug!(error = %e, "spread markups not available"),
        }
    }
    // USD rates for catalogue currencies (spread cost estimates of instruments quoted in them)
    if let Ok(v) = app.up.get(Target::MarketData, tenant, As::None, "/v1/quotes?group=raw").await
        && let Some(m) = v.as_object()
    {
        app.specs.set_rates(m);
    }
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Clients                                                             */
/* ------------------------------------------------------------------ */

pub async fn sync_clients(app: &App) -> anyhow::Result<usize> {
    let cur = db::get_cursor(&app.pool, "*", "clients").await.unwrap_or(json!({}));
    let mut since = cur["since"].as_str().unwrap_or("").to_string();
    let mut after = cur["after_id"].as_i64().unwrap_or(0);
    let mut n = 0;
    for _ in 0..200 {
        let q = if since.is_empty() { format!("/v1/internal/referrals/users?limit=1000&after_id={after}") } else { format!("/v1/internal/referrals/users?limit=1000&since={}&after_id={after}", enc(&since)) };
        let v = app.up.get(Target::Gateway, "kalks", As::None, &q).await?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            break;
        }
        for u in &items {
            let tenant = u["tenant"].as_str().unwrap_or("kalks");
            sqlx::query(
                "INSERT INTO clients (tenant, user_id, email, first_name, last_name, country, referral_code, referred_by, campaign, kyc_status, status, email_verified, created_at, changed_at, utm_source, utm_medium)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
                 ON CONFLICT (tenant, user_id) DO UPDATE SET email = EXCLUDED.email, first_name = EXCLUDED.first_name, last_name = EXCLUDED.last_name,
                   country = EXCLUDED.country, referral_code = EXCLUDED.referral_code, referred_by = EXCLUDED.referred_by, campaign = EXCLUDED.campaign,
                   utm_source = EXCLUDED.utm_source, utm_medium = EXCLUDED.utm_medium,
                   kyc_status = EXCLUDED.kyc_status, status = EXCLUDED.status, email_verified = EXCLUDED.email_verified, changed_at = EXCLUDED.changed_at",
            )
            .bind(tenant)
            .bind(u["id"].as_i64().unwrap_or(0))
            .bind(u["email"].as_str().unwrap_or(""))
            .bind(u["first_name"].as_str().unwrap_or(""))
            .bind(u["last_name"].as_str().unwrap_or(""))
            .bind(u["country"].as_str().unwrap_or(""))
            .bind(u["referral_code"].as_str())
            .bind(u["referred_by"].as_i64())
            // UTM campaign first (marketing attribution), else the IB partner campaign
            .bind(u["utm_campaign"].as_str().or(u["referral_campaign"].as_str()))
            .bind(u["kyc_status"].as_str().unwrap_or("unverified"))
            .bind(u["status"].as_str().unwrap_or("active"))
            .bind(u["email_verified"].as_bool().unwrap_or(false))
            .bind(jtime(&u["created_at"]).unwrap_or_else(Utc::now))
            .bind(jtime(&u["changed_at"]).unwrap_or_else(Utc::now))
            .bind(u["utm_source"].as_str())
            .bind(u["utm_medium"].as_str())
            .execute(&app.pool)
            .await?;
            n += 1;
        }
        let next = &v["next"];
        since = next["since"].as_str().unwrap_or(&since).to_string();
        after = next["after_id"].as_i64().unwrap_or(after);
        db::set_cursor(&app.pool, "*", "clients", &json!({"since": since, "after_id": after})).await?;
        if items.len() < 1000 {
            break;
        }
    }
    Ok(n)
}

fn enc(s: &str) -> String {
    s.replace('+', "%2B").replace(':', "%3A")
}

/* ------------------------------------------------------------------ */
/* Accounts, deals, ledger, snapshots                                  */
/* ------------------------------------------------------------------ */

/// Upserts an engine account view; returns (login, user_id, changed).
pub async fn upsert_account(app: &App, tenant: &str, a: &Value) -> anyhow::Result<(i64, i64, bool)> {
    let login = a["login"].as_i64().unwrap_or(0);
    let user_id = a["userId"].as_i64().unwrap_or(0);
    let version = a["version"].as_i64().unwrap_or(0);
    let row = sqlx::query(
        "INSERT INTO accounts (tenant, login, user_id, kind, group_code, group_name, mode, cent, currency, leverage, status, name, created_at,
                               balance, credit, bonus, equity, margin, profit, positions, version, product, seen_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22, now())
         ON CONFLICT (tenant, login) DO UPDATE SET user_id = EXCLUDED.user_id, kind = EXCLUDED.kind, group_code = EXCLUDED.group_code, product = EXCLUDED.product,
           group_name = EXCLUDED.group_name, mode = EXCLUDED.mode, cent = EXCLUDED.cent, currency = EXCLUDED.currency, leverage = EXCLUDED.leverage,
           status = EXCLUDED.status, name = EXCLUDED.name, balance = EXCLUDED.balance, credit = EXCLUDED.credit, bonus = EXCLUDED.bonus,
           equity = EXCLUDED.equity, margin = EXCLUDED.margin, profit = EXCLUDED.profit, positions = EXCLUDED.positions, version = EXCLUDED.version, seen_at = now()
         RETURNING synced_version",
    )
    .bind(tenant)
    .bind(login)
    .bind(user_id)
    .bind(a["type"].as_str().unwrap_or("live"))
    .bind(a["group"].as_str().unwrap_or(""))
    .bind(a["groupName"].as_str().unwrap_or(""))
    .bind(a["mode"].as_str().unwrap_or(""))
    .bind(a["cent"].as_bool().unwrap_or(false))
    .bind(a["currency"].as_str().unwrap_or("USD"))
    .bind(a["leverage"].as_i64().unwrap_or(0) as i32)
    .bind(a["status"].as_str().unwrap_or("active"))
    .bind(a["name"].as_str().unwrap_or(""))
    .bind(jtime(&a["createdAt"]).unwrap_or_else(Utc::now))
    .bind(dec(&a["balance"]))
    .bind(dec(&a["credit"]))
    .bind(dec(&a["bonus"]))
    .bind(dec(&a["equity"]))
    .bind(dec(&a["margin"]))
    .bind(dec(&a["profit"]))
    .bind(a["positions"].as_i64().unwrap_or(0) as i32)
    .bind(version)
    // CFD / Options account split (an engine without it: every account is a CFD account)
    .bind(if a["product"].as_str() == Some("options") { "options" } else { "cfd" })
    .fetch_one(&app.pool)
    .await?;
    let synced: i64 = row.get(0);
    Ok((login, user_id, synced != version))
}

pub async fn sync_accounts(app: &App, tenant: &str) -> anyhow::Result<()> {
    let mut page = 1;
    loop {
        let v = app.up.get(Target::Engine, tenant, As::Staff, &format!("/v1/admin/accounts?limit=500&page={page}")).await?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        for a in &items {
            let (login, user_id, changed) = upsert_account(app, tenant, a).await?;
            if changed
                && let Err(e) = sync_account_data(app, tenant, login, user_id).await
            {
                tracing::warn!(error = %e, login, "account data sync failed");
                continue;
            }
            if changed || a["positions"].as_i64().unwrap_or(0) > 0 {
                snapshot_today(app, tenant, login).await?;
            }
        }
        let total = v["total"].as_i64().unwrap_or(0);
        if items.len() < 500 || (page * 500) as i64 >= total {
            break;
        }
        page += 1;
    }
    // make sure every account has a row for today (quiet accounts carry their balance forward)
    let today = time::server_day(Utc::now());
    sqlx::query(
        "INSERT INTO snapshots (tenant, login, day, balance, credit, equity, margin, flow, source)
         SELECT a.tenant, a.login, $2, a.balance, a.credit + a.bonus, a.equity, a.margin, 0, 'live' FROM accounts a
         WHERE a.tenant = $1 AND a.synced_version >= 0
         ON CONFLICT (tenant, login, day) DO NOTHING",
    )
    .bind(tenant)
    .bind(today)
    .execute(&app.pool)
    .await?;
    Ok(())
}

/// Refreshes one client's accounts on demand (statements, analytics) so a trade made seconds ago is included.
pub async fn refresh_user(app: &App, tenant: &str, user_id: i64) -> anyhow::Result<()> {
    let v = app.up.get(Target::Engine, tenant, As::User(user_id), &format!("/v1/accounts?user_id={user_id}")).await?;
    for a in v["accounts"].as_array().cloned().unwrap_or_default() {
        let (login, uid, changed) = upsert_account(app, tenant, &a).await?;
        if changed {
            sync_account_data(app, tenant, login, uid).await?;
            snapshot_today(app, tenant, login).await?;
        }
    }
    Ok(())
}

/// Pulls new deals and ledger rows of one account (overlap of 10 minutes; a reversal forces a full deal re-read
/// so reopened deals get their `reversed` flag), then marks the engine version as mirrored.
pub async fn sync_account_data(app: &App, tenant: &str, login: i64, user_id: i64) -> anyhow::Result<()> {
    let lock = app.lock_for(tenant, login).await;
    let _g = lock.lock().await;
    let row = sqlx::query("SELECT version, synced_version, deals_until, ledger_until, backfilled FROM accounts WHERE tenant = $1 AND login = $2")
        .bind(tenant)
        .bind(login)
        .fetch_one(&app.pool)
        .await?;
    let version: i64 = row.get("version");
    if row.get::<i64, _>("synced_version") == version {
        return Ok(());
    }
    let deals_until: Option<DateTime<Utc>> = row.get("deals_until");
    let ledger_until: Option<DateTime<Utc>> = row.get("ledger_until");
    let backfilled: bool = row.get("backfilled");
    let overlap = Duration::minutes(10);

    let (new_ledger, ledger_max, saw_reversal) = pull_ledger(app, tenant, login, user_id, ledger_until.map(|t| t - overlap)).await?;
    let deals_from = if saw_reversal { None } else { deals_until.map(|t| t - overlap) };
    let deals_max = pull_deals(app, tenant, login, user_id, deals_from).await?;

    sqlx::query("UPDATE accounts SET synced_version = $3, deals_until = GREATEST(COALESCE(deals_until, $4), $4), ledger_until = GREATEST(COALESCE(ledger_until, $5), $5) WHERE tenant = $1 AND login = $2")
        .bind(tenant)
        .bind(login)
        .bind(version)
        .bind(deals_max.or(deals_until))
        .bind(ledger_max.or(ledger_until))
        .execute(&app.pool)
        .await?;
    if !backfilled || new_ledger > 0 {
        backfill_snapshots(app, tenant, login).await?;
    }
    Ok(())
}

async fn pull_deals(app: &App, tenant: &str, login: i64, user_id: i64, from: Option<DateTime<Utc>>) -> anyhow::Result<Option<DateTime<Utc>>> {
    let mut max: Option<DateTime<Utc>> = None;
    for page in 1..=500 {
        let mut q = format!("/v1/accounts/{login}/history?user_id={user_id}&limit=1000&page={page}");
        if let Some(f) = from {
            q.push_str(&format!("&from={}", enc(&f.to_rfc3339())));
        }
        let v = app.up.get(Target::Engine, tenant, As::User(user_id), &q).await?;
        let deals = v["deals"].as_array().cloned().unwrap_or_default();
        for d in &deals {
            let t = jtime(&d["time"]).unwrap_or_else(Utc::now);
            max = Some(max.map_or(t, |m| m.max(t)));
            upsert_deal(app, tenant, d).await?;
        }
        if deals.len() < 1000 {
            break;
        }
    }
    Ok(max)
}

pub async fn upsert_deal(app: &App, tenant: &str, d: &Value) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO deals (tenant, id, login, position_ticket, order_ticket, symbol, side, position_side, entry, volume, price, profit, swap, commission,
                            reason, book, time, open_price, open_time, source, comment, price_correction, reversed, option)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24)
         ON CONFLICT (tenant, id) DO UPDATE SET profit = EXCLUDED.profit, swap = EXCLUDED.swap, commission = EXCLUDED.commission, book = EXCLUDED.book,
           open_price = EXCLUDED.open_price, price_correction = EXCLUDED.price_correction, reversed = EXCLUDED.reversed,
           option = COALESCE(EXCLUDED.option, deals.option)",
    )
    .bind(tenant)
    .bind(d["id"].as_i64().unwrap_or(0))
    .bind(d["login"].as_i64().unwrap_or(0))
    .bind(d["positionTicket"].as_i64().unwrap_or(0))
    .bind(d["orderTicket"].as_i64())
    .bind(d["symbol"].as_str().unwrap_or(""))
    .bind(d["side"].as_str().unwrap_or("buy"))
    .bind(d["positionSide"].as_str().unwrap_or("buy"))
    .bind(d["entry"].as_str().unwrap_or("in"))
    .bind(dec(&d["volume"]))
    .bind(dec(&d["price"]))
    .bind(dec(&d["profit"]))
    .bind(dec(&d["swap"]))
    .bind(dec(&d["commission"]))
    .bind(d["reason"].as_str().unwrap_or("client"))
    .bind(d["book"].as_str().unwrap_or("B"))
    .bind(jtime(&d["time"]).unwrap_or_else(Utc::now))
    .bind(if d["openPrice"].is_null() { None } else { Some(dec(&d["openPrice"])) })
    .bind(jtime(&d["openTime"]))
    .bind(d["source"].as_str().unwrap_or("manual"))
    .bind(d["comment"].as_str().unwrap_or(""))
    .bind(d["priceCorrection"].as_bool().unwrap_or(false))
    .bind(d["reversed"].as_bool().unwrap_or(false))
    .bind(deal_option(d).map(sqlx::types::Json))
    .execute(&app.pool)
    .await?;
    Ok(())
}

/// Returns (rows written, newest time, a reversal was seen).
async fn pull_ledger(app: &App, tenant: &str, login: i64, user_id: i64, from: Option<DateTime<Utc>>) -> anyhow::Result<(usize, Option<DateTime<Utc>>, bool)> {
    let mut n = 0;
    let mut max: Option<DateTime<Utc>> = None;
    let mut reversal = false;
    for page in 1..=500 {
        let mut q = format!("/v1/accounts/{login}/ledger?user_id={user_id}&limit=1000&page={page}");
        if let Some(f) = from {
            q.push_str(&format!("&from={}", enc(&f.to_rfc3339())));
        }
        let v = app.up.get(Target::Engine, tenant, As::User(user_id), &q).await?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        for e in &items {
            let at = jtime(&e["at"]).unwrap_or_else(Utc::now);
            max = Some(max.map_or(at, |m| m.max(at)));
            let kind = e["kind"].as_str().unwrap_or("");
            let r = sqlx::query(
                "INSERT INTO ledger (tenant, login, txn, sub_ledger, kind, amount, currency, reference, reason_code, note, at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT DO NOTHING",
            )
            .bind(tenant)
            .bind(login)
            .bind(e["txn"].as_i64().unwrap_or(0))
            .bind(e["subLedger"].as_str().unwrap_or("balance"))
            .bind(kind)
            .bind(dec(&e["amount"]))
            .bind(e["currency"].as_str().unwrap_or("USD"))
            .bind(e["reference"].as_str())
            .bind(e["reasonCode"].as_str())
            .bind(e["note"].as_str())
            .bind(at)
            .execute(&app.pool)
            .await?;
            if r.rows_affected() > 0 {
                n += 1;
                if kind == "reversal" {
                    reversal = true;
                }
            }
        }
        if items.len() < 1000 {
            break;
        }
    }
    Ok((n, max, reversal))
}

/// Writes today's row from the account's live metrics (overwritten until the server day ends).
pub async fn snapshot_today(app: &App, tenant: &str, login: i64) -> anyhow::Result<()> {
    let today = time::server_day(Utc::now());
    let start = time::day_start(today);
    sqlx::query(
        "INSERT INTO snapshots (tenant, login, day, balance, credit, equity, margin, flow, source, taken_at)
         SELECT a.tenant, a.login, $3, a.balance, a.credit + a.bonus, a.equity, a.margin,
                COALESCE((SELECT sum(l.amount) FROM ledger l WHERE l.tenant = a.tenant AND l.login = a.login AND l.at >= $4 AND l.kind = ANY($5)), 0), 'live', now()
         FROM accounts a WHERE a.tenant = $1 AND a.login = $2
         ON CONFLICT (tenant, login, day) DO UPDATE SET balance = EXCLUDED.balance, credit = EXCLUDED.credit, equity = EXCLUDED.equity,
           margin = EXCLUDED.margin, flow = EXCLUDED.flow, source = 'live', taken_at = now()",
    )
    .bind(tenant)
    .bind(login)
    .bind(today)
    .bind(start)
    .bind(FLOW_KINDS)
    .execute(&app.pool)
    .await?;
    Ok(())
}

/// End-of-day balances rebuilt from the ledger for past days that have no live snapshot.
pub async fn backfill_snapshots(app: &App, tenant: &str, login: i64) -> anyhow::Result<()> {
    let rows = sqlx::query("SELECT sub_ledger, kind, amount, at FROM ledger WHERE tenant = $1 AND login = $2 ORDER BY at, txn")
        .bind(tenant)
        .bind(login)
        .fetch_all(&app.pool)
        .await?;
    let today = time::server_day(Utc::now());
    let mut days: BTreeMap<NaiveDate, (Decimal, Decimal, Decimal)> = BTreeMap::new();
    let (mut bal, mut other) = (Decimal::ZERO, Decimal::ZERO);
    for r in &rows {
        let at: DateTime<Utc> = r.get("at");
        let amt: Decimal = r.get("amount");
        let sub: String = r.get("sub_ledger");
        let kind: String = r.get("kind");
        if sub == "balance" {
            bal += amt;
        } else {
            other += amt;
        }
        let d = time::server_day(at);
        let e = days.entry(d).or_insert((Decimal::ZERO, Decimal::ZERO, Decimal::ZERO));
        e.0 = bal;
        e.1 = other;
        if FLOW_KINDS.contains(&kind.as_str()) || sub != "balance" {
            e.2 += amt;
        }
    }
    for (d, (b, o, flow)) in days {
        if d >= today {
            continue;
        }
        sqlx::query(
            "INSERT INTO snapshots (tenant, login, day, balance, credit, equity, margin, flow, source) VALUES ($1,$2,$3,$4,$5,$6,0,$7,'backfill')
             ON CONFLICT (tenant, login, day) DO UPDATE SET balance = EXCLUDED.balance, credit = EXCLUDED.credit, equity = EXCLUDED.equity, flow = EXCLUDED.flow
             WHERE snapshots.source = 'backfill'",
        )
        .bind(tenant)
        .bind(login)
        .bind(d)
        .bind(b)
        .bind(o)
        .bind(b + o)
        .bind(flow)
        .execute(&app.pool)
        .await?;
    }
    sqlx::query("UPDATE accounts SET backfilled = true WHERE tenant = $1 AND login = $2").bind(tenant).bind(login).execute(&app.pool).await?;
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Wallet                                                              */
/* ------------------------------------------------------------------ */

pub async fn sync_wallet(app: &App, tenant: &str) -> anyhow::Result<()> {
    // deposits: read newest first until below the oldest id that can still change (or everything once)
    let open: Option<i64> = sqlx::query_scalar("SELECT min(id) FROM wallet_deposits WHERE tenant = $1 AND status NOT IN ('credited','failed','rejected')")
        .bind(tenant)
        .fetch_one(&app.pool)
        .await?;
    let max: Option<i64> = sqlx::query_scalar("SELECT max(id) FROM wallet_deposits WHERE tenant = $1").bind(tenant).fetch_one(&app.pool).await?;
    let floor = open.or(max).unwrap_or(0);
    'dep: for page in 1..=200 {
        let v = app.up.get(Target::Wallet, tenant, As::Staff, &format!("/v1/admin/deposits?limit=200&page={page}")).await?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        for d in &items {
            let id = d["id"].as_i64().unwrap_or(0);
            if id < floor {
                break 'dep;
            }
            sqlx::query(
                "INSERT INTO wallet_deposits (tenant, id, user_id, chain, currency, amount, status, credited_at, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)
                 ON CONFLICT (tenant, id) DO UPDATE SET user_id = EXCLUDED.user_id, amount = EXCLUDED.amount, status = EXCLUDED.status, credited_at = EXCLUDED.credited_at",
            )
            .bind(tenant)
            .bind(id)
            .bind(d["user_id"].as_i64())
            .bind(d["chain"].as_str().unwrap_or(""))
            .bind(d["currency"].as_str().unwrap_or("USDT"))
            .bind(dec(&d["amount"]))
            .bind(d["status"].as_str().unwrap_or("pending"))
            .bind(jtime(&d["credited_at"]))
            .bind(jtime(&d["created_at"]).unwrap_or_else(Utc::now))
            .execute(&app.pool)
            .await?;
        }
        if items.len() < 200 {
            break;
        }
    }
    let open: Option<i64> = sqlx::query_scalar("SELECT min(id) FROM wallet_withdrawals WHERE tenant = $1 AND status NOT IN ('completed','rejected','cancelled')")
        .bind(tenant)
        .fetch_one(&app.pool)
        .await?;
    let max: Option<i64> = sqlx::query_scalar("SELECT max(id) FROM wallet_withdrawals WHERE tenant = $1").bind(tenant).fetch_one(&app.pool).await?;
    let floor = open.or(max).unwrap_or(0);
    'wd: for page in 1..=200 {
        let v = app.up.get(Target::Wallet, tenant, As::Staff, &format!("/v1/admin/withdrawals?limit=200&page={page}")).await?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        for w in &items {
            let id = w["id"].as_i64().unwrap_or(0);
            if id < floor {
                break 'wd;
            }
            sqlx::query(
                "INSERT INTO wallet_withdrawals (tenant, id, user_id, chain, currency, amount, fee, net_amount, status, completed_at, created_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT (tenant, id) DO UPDATE SET status = EXCLUDED.status, completed_at = EXCLUDED.completed_at, fee = EXCLUDED.fee, net_amount = EXCLUDED.net_amount",
            )
            .bind(tenant)
            .bind(id)
            .bind(w["user_id"].as_i64().unwrap_or(0))
            .bind(w["chain"].as_str().unwrap_or(""))
            .bind(w["currency"].as_str().unwrap_or("USDT"))
            .bind(dec(&w["amount"]))
            .bind(dec(&w["fee"]))
            .bind(dec(&w["net_amount"]))
            .bind(w["status"].as_str().unwrap_or("requested"))
            .bind(jtime(&w["completed_at"]))
            .bind(jtime(&w["created_at"]).unwrap_or_else(Utc::now))
            .execute(&app.pool)
            .await?;
        }
        if items.len() < 200 {
            break;
        }
    }
    sync_wallet_manual(app, tenant).await
}

/// Manual wallet deposits / withdrawals booked by staff as real money (Back Office "Balance & credit", reasons
/// "Deposit (external payment received)" and "Withdrawal (paid externally)"). Rare, and an approval can apply an
/// older request later, so every applied one is re-read (idempotent upsert).
pub async fn sync_wallet_manual(app: &App, tenant: &str) -> anyhow::Result<()> {
    for (category, kind) in [("deposit", "deposit"), ("withdrawal", "withdrawal")] {
        for page in 1..=50 {
            let v = app.up.get(Target::Wallet, tenant, As::Staff, &format!("/v1/admin/adjustments?target=wallet&status=applied&category={category}&limit=2000&page={page}")).await?;
            let items = v["items"].as_array().cloned().unwrap_or_default();
            for a in &items {
                let Some(at) = jtime(&a["applied_at"]) else { continue };
                sqlx::query(
                    "INSERT INTO wallet_manual (tenant, id, user_id, kind, amount, currency, applied_at) VALUES ($1,$2,$3,$4,$5,$6,$7)
                     ON CONFLICT (tenant, id) DO UPDATE SET amount = EXCLUDED.amount, applied_at = EXCLUDED.applied_at",
                )
                .bind(tenant)
                .bind(a["id"].as_i64().unwrap_or(0))
                .bind(a["user_id"].as_i64().unwrap_or(0))
                .bind(kind)
                .bind(dec(&a["amount_usd"]))
                .bind(a["currency"].as_str().unwrap_or("USDT"))
                .bind(at)
                .execute(&app.pool)
                .await?;
            }
            if items.len() < 2000 {
                break;
            }
        }
    }
    Ok(())
}

/* ------------------------------------------------------------------ */
/* IB commissions                                                      */
/* ------------------------------------------------------------------ */

pub async fn sync_ib(app: &App, tenant: &str) -> anyhow::Result<()> {
    let full = db::get_cursor(&app.pool, tenant, "ib_full").await.is_none();
    let from = if full { None } else { Some((Utc::now() - Duration::days(35)).to_rfc3339()) };
    for page in 1..=400 {
        let mut q = format!("/v1/ib/admin/commissions?limit=500&page={page}");
        if let Some(f) = &from {
            q.push_str(&format!("&from={}", enc(f)));
        }
        let v = app.up.get(Target::Ib, tenant, As::Staff, &q).await?;
        let items = v["items"].as_array().cloned().unwrap_or_default();
        for c in &items {
            sqlx::query(
                "INSERT INTO ib_commissions (tenant, id, kind, status, amount, beneficiary_id, client_id, login, symbol, lots, created_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                 ON CONFLICT (tenant, id) DO UPDATE SET status = EXCLUDED.status, amount = EXCLUDED.amount",
            )
            .bind(tenant)
            .bind(c["id"].as_i64().unwrap_or(0))
            .bind(c["kind"].as_str().unwrap_or("lot"))
            .bind(c["status"].as_str().unwrap_or("pending"))
            .bind(dec(&c["amount"]))
            .bind(c["beneficiary"]["id"].as_i64().unwrap_or(0))
            .bind(c["client"]["id"].as_i64())
            .bind(c["login"].as_i64())
            .bind(c["symbol"].as_str())
            .bind(dec(&c["lots"]))
            .bind(jtime(&c["createdAt"]).unwrap_or_else(Utc::now))
            .execute(&app.pool)
            .await?;
        }
        if items.len() < 500 {
            break;
        }
    }
    if full {
        db::set_cursor(&app.pool, tenant, "ib_full", &json!({"done": Utc::now()})).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_flows_are_never_deposits_or_withdrawals() {
        for k in ["option_premium", "option_settlement", "trade_pnl", "commission", "reversal"] {
            assert!(!FLOW_KINDS.contains(&k), "{k}");
        }
    }

    #[test]
    fn option_deals_are_recognised_in_the_feed() {
        let o = deal_option(&json!({"symbol": "EURUSD-20261009-1.1650-C", "instrument": "option", "option": {"series": "EURUSD-20261009-1.1650-C", "cash": -104}})).unwrap();
        assert_eq!(o["cash"], -104);
        assert_eq!(deal_option(&json!({"symbol": "EURUSD-20261009-1.1650-C"})).unwrap()["series"], "EURUSD-20261009-1.1650-C");
        assert!(deal_option(&json!({"symbol": "EURUSD", "option": null, "instrument": "cfd"})).is_none());
    }
}

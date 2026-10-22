//! Enabling the order book for a tenant and account kind (docs/OPTIONS-EXCHANGE.md §11). Nothing is enabled
//! automatically: the founder (staff with `options.settle`, four-eyes) switches it on in the Back Office.
//!
//! 1. **Halt house opens**: the venue is switched on in memory first, so `/v1/terminal/options/orders` refuses
//!    listed legs from now on (`book_venue`); barrier-only orders stay Kalks-quoted.
//! 2. **Cancel legacy pending option orders** (house-priced limit / trigger orders on listed series); clients are
//!    notified.
//! 3. **Start the actors and the market maker** and wait for its quote coverage (a closed market is a warning).
//! 4. **Novate** every open house-priced vanilla position: per series a `Seed` journal entry (client steps and the
//!    MM's opposite, Σ = 0), the market maker's mirror position (deal reason `novation`) with the premium the
//!    house took for those positions (`house:options_premium → MM balance`, key `novate:{tenant}:{kind}:{series}`),
//!    and the client's position moved to `venue = book` (it keeps its position and P&L). Barriers stay house.
//! 5. **Write the `option_book_venues` row** (forward-only; the kill switches are halt / cancel-only and MM pause).
//!
//! Every step is idempotent: a crashed or refused enable is completed by running it again.

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;

use super::types::*;
use super::{BookKey, entry};
use crate::api::AppState;
use crate::engine::options_book as ob;
use crate::model::{AccountKind, Book, Deal, DealEntry, DealOption, DealReason, LedgerTxn, Position, Posting, RouteEvent, Side, Source, TxnKind, Venue, acct_code, house_code};
use crate::money::{D, ONE, ZERO, num, r2, rdp};
use crate::options::OptionPricing;
use crate::shard::Op;
use crate::state::Event;

/// Quote coverage the market maker should reach before novation (percent).
pub const REQUIRED_COVERAGE: f64 = 90.0;

/// One open house-priced vanilla position that novation moves to the book.
#[derive(Clone, Debug)]
pub struct Novated {
    pub login: i64,
    pub ticket: i64,
    pub series: String,
    pub contracts: D,
    pub side: Side,
    pub open_price: D,
    /// Premium basis (signed cash, account currency) and the USD factor of the account.
    pub premium: D,
    pub factor: D,
}

async fn house_positions(st: &AppState, tenant_id: i64, kind: AccountKind) -> (Vec<Novated>, usize, usize) {
    let mm: Vec<i64> = st.hub.shared.books.mm.logins.lock().unwrap().values().copied().collect();
    let rows = st
        .hub
        .scan(
            tenant_id,
            Arc::new(move |a, _| {
                if a.account.kind != kind || mm.contains(&a.login()) {
                    return vec![];
                }
                let mut v: Vec<Value> = a
                    .positions
                    .values()
                    .filter(|p| !p.on_book())
                    .filter_map(|p| {
                        let t = p.option.as_ref()?;
                        if t.barrier.is_some() {
                            return Some(json!({"barrier": true}));
                        }
                        Some(json!({"login": p.login, "ticket": p.ticket, "series": p.symbol, "contracts": p.volume.to_string(), "side": p.side.as_str(), "open": p.open_price.to_string(), "premium": p.premium.to_string(), "factor": a.account.usd_factor().to_string()}))
                    })
                    .collect();
                let pending = a.orders.values().filter(|o| o.option.as_ref().is_some_and(|oo| oo.book.is_none() && oo.legs.iter().any(|l| l.terms.barrier.is_none()))).count();
                if pending > 0 {
                    v.push(json!({"pending": pending}));
                }
                v
            }),
        )
        .await;
    let (mut out, mut barriers, mut pending) = (Vec::new(), 0usize, 0usize);
    for r in rows {
        if r["barrier"] == true {
            barriers += 1;
            continue;
        }
        if let Some(n) = r["pending"].as_u64() {
            pending += n as usize;
            continue;
        }
        let d = |k: &str| r[k].as_str().and_then(|s| s.parse::<D>().ok()).unwrap_or(ZERO);
        out.push(Novated {
            login: r["login"].as_i64().unwrap_or(0),
            ticket: r["ticket"].as_i64().unwrap_or(0),
            series: r["series"].as_str().unwrap_or("").to_string(),
            contracts: d("contracts"),
            side: if r["side"] == "buy" { Side::Buy } else { Side::Sell },
            open_price: d("open"),
            premium: d("premium"),
            factor: d("factor").max(ONE),
        });
    }
    out.sort_by_key(|n| (n.series.clone(), n.ticket));
    (out, barriers, pending)
}

/// `GET /v1/admin/options/book/enable/plan?kind=` — the dry run.
pub async fn plan(st: &AppState, tenant_id: i64, kind: AccountKind) -> Value {
    let books = &st.hub.shared.books;
    let enabled = books.venue_enabled(tenant_id, kind);
    let enabled_at: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT enabled_at FROM option_book_venues WHERE tenant_id = $1 AND kind = $2").bind(tenant_id).bind(kind.as_str()).fetch_optional(&st.pool).await.ok().flatten();
    let (pos, barriers, pending) = house_positions(st, tenant_id, kind).await;
    let clients: std::collections::BTreeSet<i64> = pos.iter().map(|p| p.login).collect();
    let contracts: D = pos.iter().map(|p| p.contracts).sum();
    let premium_usd: D = pos.iter().map(|p| (p.premium / p.factor).abs()).sum();
    let mut warnings = Vec::new();
    let mut blockers = Vec::new();
    let opts = &st.hub.shared.options;
    let now = st.hub.shared.clock.now();
    match opts.snapshot() {
        None => blockers.push("No options snapshot: the options service has not been reached.".to_string()),
        Some(s) => {
            let slug = st.hub.shared.registry.get(tenant_id).map(|t| t.slug.clone()).unwrap_or_default();
            if !s.enabled(&slug, kind == AccountKind::Live) {
                blockers.push(format!("Kalks FX Options are switched off for {} accounts of this broker.", kind.as_str()));
            }
            if opts.stale(now) {
                blockers.push("The options snapshot is stale.".into());
            }
            let open = s.underlyings.values().filter(|u| u.enabled).any(|u| st.hub.shared.specs.load().get(&u.symbol).is_some_and(|sp| sp.is_open(now)));
            if !open {
                warnings.push("Every options market is closed now: the market maker starts quoting at the next session open.".into());
            }
        }
    }
    if st.cfg.options_mm_user <= 0 && books.mm.login(tenant_id, kind).is_none() {
        blockers.push("OPTIONS_MM_USER_ID is not set: the Kalks market maker has no user.".into());
    }
    if barriers > 0 {
        warnings.push(format!("{barriers} barrier position(s) stay Kalks-quoted (house venue)."));
    }
    let not_whole = pos.iter().filter(|p| p.contracts.fract() != ZERO).count();
    if not_whole > 0 {
        warnings.push(format!("{not_whole} position(s) are not a whole number of contract steps and stay house-priced."));
    }
    let pending_row = sqlx::query_as::<_, (i64, String, DateTime<Utc>, String)>("SELECT id, requested_by, requested_at, reason FROM option_approvals WHERE tenant_id = $1 AND action = 'book_enable' AND kind = $2 AND status = 'pending' ORDER BY id DESC LIMIT 1")
        .bind(tenant_id)
        .bind(kind.as_str())
        .fetch_optional(&st.pool)
        .await
        .ok()
        .flatten();
    json!({
        "kind": kind.as_str(), "enabled": enabled, "enabledAt": enabled_at,
        "mmCoverage": {"pct": super::mm::coverage(&st.hub, tenant_id, kind), "required": REQUIRED_COVERAGE},
        "steps": [
            {"key": "halt_house_opens", "label": "Halt house opens", "detail": "New option opens against the house stop; closing keeps working."},
            {"key": "cancel_legacy_orders", "label": "Cancel legacy pending option orders", "detail": "Clients are notified.", "count": pending},
            {"key": "start_actors", "label": "Start the book actors and the market maker", "detail": "Wait for MM quote coverage."},
            {"key": "novate", "label": "Novate open positions to the book", "detail": "Clients keep their positions and P&L; the house's opposite moves into the MM account.", "count": pos.len()},
            {"key": "write_venue", "label": "Write the venue row", "detail": "From then on the book is the venue for these accounts."},
        ],
        "legacyPendingOrders": pending,
        "novation": {"positions": pos.len(), "clients": clients.len(), "contracts": num(contracts), "premiumUsd": num(r2(premium_usd))},
        "barriersStayHouse": barriers,
        "warnings": warnings, "blockers": blockers,
        "pending": pending_row.map(|(id, by, at, reason)| json!({"id": id, "requestedBy": by, "requestedAt": at, "reason": reason})),
    })
}

/// Runs the enable (idempotent). Returns the report.
pub async fn enable(st: &AppState, tenant_id: i64, kind: AccountKind, by: &str, reason: &str) -> anyhow::Result<Value> {
    let hub = &st.hub;
    let books = &hub.shared.books;
    let slug = hub.shared.registry.get(tenant_id).map(|t| t.slug.clone()).ok_or_else(|| anyhow::anyhow!("unknown tenant"))?;
    let snap = hub.shared.options.snapshot().ok_or_else(|| anyhow::anyhow!("no options snapshot"))?;
    // 1. halt house opens: the book is the venue from now on
    books.set_venue(tenant_id, kind, true);
    // 2. cancel legacy pending option orders
    let tickets = hub
        .scan(
            tenant_id,
            Arc::new(move |a, _| {
                if a.account.kind != kind {
                    return vec![];
                }
                a.orders.values().filter(|o| o.option.as_ref().is_some_and(|oo| oo.book.is_none() && oo.legs.iter().any(|l| l.terms.barrier.is_none()))).map(|o| json!([o.login, o.ticket])).collect()
            }),
        )
        .await;
    let mut cancelled = 0;
    for t in tickets {
        let (Some(login), Some(ticket)) = (t[0].as_i64(), t[1].as_i64()) else { continue };
        let op: Op = Box::new(move |tx, env| {
            let o = crate::engine::trade::cancel_order(tx, env, ticket, "The options order book is now live: house-priced orders were cancelled")?;
            tx.note("order_cancelled", format!("Option order #{ticket} was cancelled: options now trade on the order book. Place it again from the order ticket."), json!({"ticket": ticket, "options": true, "book": true}));
            Ok(json!({"ticket": o.ticket}))
        });
        if hub.exec(login, "system:book-enable", None, "", "", None, op).await.is_ok() {
            cancelled += 1;
        }
    }
    // 3. the market maker: its account and a few quoting passes
    let mm = super::mm::account(st, tenant_id, kind).await?;
    let mm_user = hub.meta(mm).map(|m| m.user_id).unwrap_or(0);
    let mut coverage = 0.0;
    for _ in 0..40 {
        let _ = super::mm::pass(st, tenant_id, kind).await;
        coverage = super::mm::coverage(hub, tenant_id, kind);
        if coverage >= REQUIRED_COVERAGE {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
    // 4. novation, per series
    let (pos, _, _) = house_positions(st, tenant_id, kind).await;
    let mut by_series: BTreeMap<String, Vec<Novated>> = BTreeMap::new();
    for p in pos {
        by_series.entry(p.series.clone()).or_default().push(p);
    }
    let mut novated = Vec::new();
    for (series, list) in by_series {
        let Ok((terms, u)) = ob::terms_of(&snap, &series) else { continue };
        let (tick, step) = ob::units(&u);
        let spec = SeriesSpec { terms: terms.clone(), tick, step };
        let list: Vec<Novated> = list.into_iter().filter(|p| ob::whole(p.contracts, step).is_some()).collect();
        if list.is_empty() {
            continue;
        }
        let mut entries: Vec<(i64, Steps)> = Vec::new();
        for p in &list {
            let s = ob::whole(p.contracts, step).unwrap_or(0) * if p.side == Side::Buy { 1 } else { -1 };
            entries.push((p.login, s));
        }
        let net: Steps = entries.iter().map(|e| e.1).sum();
        if net != 0 {
            entries.push((mm, -net));
        }
        let key = BookKey::new(tenant_id, kind, &terms.underlying);
        // a. the book's positions (once per series: the journal says whether it was seeded before)
        let seeded: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM book_journal WHERE tenant_id = $1 AND kind = $2 AND underlying = $3 AND cmd_kind = 'seed' AND cmd->>'series' = $4 AND out->>'ok' = 'true')")
            .bind(tenant_id)
            .bind(kind.as_str())
            .bind(&terms.underlying)
            .bind(&series)
            .fetch_one(&st.pool)
            .await
            .unwrap_or(false);
        if !seeded && entries.len() > 1 {
            let at = hub.shared.clock.now().timestamp_millis();
            let (out, _, _) = entry::call(hub, mm, &key, Cmd::Seed { series: series.clone(), spec: spec.clone(), entries: entries.clone(), at }).await.map_err(|e| anyhow::anyhow!("seed {series}: {e:?}"))?;
            anyhow::ensure!(out.ok, "seed {series}: {:?}", out.message);
        }
        // b. the market maker's mirror position and the premium the house took for it
        let usd_cash: D = list.iter().map(|p| p.premium / p.factor).sum();
        let mm_contracts = D::from(-net) * step;
        let vwap = {
            let tot: D = list.iter().map(|p| p.contracts).sum();
            if tot > ZERO { rdp(list.iter().map(|p| p.open_price * p.contracts).sum::<D>() / tot, 12) } else { ZERO }
        };
        let idem = format!("novate:{slug}:{}:{series}", kind.as_str());
        let (t2, s2) = (terms.clone(), series.clone());
        let op: Op = Box::new(move |tx, env| novate_mm(tx, env, &idem, &t2, &s2, mm_contracts, vwap, -usd_cash));
        match hub.exec(mm, "system:book-enable", None, "", "", None, op).await {
            Ok(_) | Err(crate::shard::ExecError::Duplicate(_)) => {}
            Err(e) => anyhow::bail!("novation of {series} on the market maker: {e:?}"),
        }
        if books.hooks.crash_in_novation.swap(false, std::sync::atomic::Ordering::SeqCst) {
            anyhow::bail!("simulated crash during the novation of {series}");
        }
        // c. the clients' positions move to the book venue
        for p in &list {
            let ticket = p.ticket;
            let op: Op = Box::new(move |tx, env| novate_client(tx, env, ticket));
            hub.exec(p.login, "system:book-enable", None, "", "", None, op).await.map_err(|e| anyhow::anyhow!("novation of #{ticket}: {e:?}"))?;
        }
        novated.push(json!({"series": series, "positions": list.len(), "mmContracts": num(mm_contracts), "premiumUsd": num(r2(-usd_cash))}));
    }
    // a crashed earlier run left its underlyings cancel-only (the reconcile at start found book and accounts apart):
    // now that the novation is complete, a clean reconcile lifts those system halts
    let mut healed = Vec::new();
    for h in books.handles().into_iter().filter(|h| h.key.tenant_id == tenant_id && h.key.kind == kind) {
        let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM book_halts WHERE tenant_id = $1 AND kind = $2 AND underlying = $3 AND lifted_at IS NULL AND staff = 'system' AND reason LIKE 'reconcile:%'")
            .bind(tenant_id)
            .bind(kind.as_str())
            .bind(&h.key.underlying)
            .fetch_all(&st.pool)
            .await
            .unwrap_or_default();
        if ids.is_empty() || super::reconcile(hub, &h, &[]).await.is_some() {
            continue;
        }
        let at = hub.shared.clock.now().timestamp_millis();
        if h.call(Cmd::Halt { scope: HaltScope::All, mode: HaltMode::Resume, reason: "novation completed: reconcile clean".into(), at }).await.is_ok() {
            let _ = sqlx::query("UPDATE book_halts SET lifted_at = now(), lifted_by = 'book-enable' WHERE id = ANY($1)").bind(&ids).execute(&st.pool).await;
            healed.push(h.key.underlying.clone());
        }
    }
    // 5. the venue row
    books.enable_venue(&st.pool, tenant_id, kind, by, reason).await?;
    let _ = sqlx::query("UPDATE option_book_venues SET data = $3 WHERE tenant_id = $1 AND kind = $2")
        .bind(tenant_id)
        .bind(kind.as_str())
        .bind(sqlx::types::Json(json!({"cancelledOrders": cancelled, "novated": novated, "mmLogin": mm, "mmUser": mm_user, "coverage": coverage})))
        .execute(&st.pool)
        .await;
    tracing::warn!(tenant = tenant_id, kind = kind.as_str(), by, cancelled, series = novated.len(), coverage, "options order book ENABLED");
    Ok(json!({"cancelledOrders": cancelled, "novated": novated, "mmLogin": mm, "coverage": coverage, "healed": healed}))
}

/// The market maker's side of a novated series: a book-venue position opposite to the clients' (deal reason
/// `novation`) and the premium the house took for those positions moved from `house:options_premium`.
#[allow(clippy::too_many_arguments)]
fn novate_mm(tx: &mut crate::engine::Tx, env: &crate::engine::Env, idem: &str, terms: &crate::model::OptionTerms, series: &str, contracts: D, price: D, cash_usd: D) -> Result<Value, crate::engine::Reject> {
    let acc = tx.st.account.clone();
    let login = acc.login;
    let cash = r2(cash_usd * acc.usd_factor());
    let txn_id = if cash.is_zero() {
        None
    } else {
        let txn = LedgerTxn {
            id: env.ids.txn(),
            tenant_id: acc.tenant_id,
            idempotency_key: format!("{idem}:cash"),
            kind: TxnKind::OptionPremium,
            login,
            reference: Some(idem.to_string()),
            reason_code: None,
            note: Some(format!("Novation of {series}: the premium of the house's side moves to the market maker")),
            at: env.now,
            postings: vec![Posting { account: acct_code(login, "balance"), ccy: "USD".into(), amount: r2(cash_usd) }, Posting { account: house_code(crate::engine::options::HOUSE_PREMIUM, "USD"), ccy: "USD".into(), amount: -r2(cash_usd) }],
        };
        assert!(txn.is_balanced());
        let id = txn.id;
        tx.emit(Event::Ledger { txn });
        Some(id)
    };
    if contracts.is_zero() {
        return Ok(json!({"series": series, "contracts": 0}));
    }
    let side = if contracts > ZERO { Side::Buy } else { Side::Sell };
    let vol = contracts.abs();
    let ticket = env.ids.ticket();
    let deal_id = env.ids.deal();
    let pos = Position {
        ticket,
        login,
        symbol: series.to_string(),
        side,
        volume: vol,
        open_price: price,
        open_time: env.now,
        sl: None,
        tp: None,
        trailing: None,
        swap: ZERO,
        commission: ZERO,
        source: Source::System,
        platform: "Order book".into(),
        comment: "novation".into(),
        book: Book::B,
        order_ticket: ticket,
        parent_ticket: None,
        child_tickets: vec![],
        book_since: env.now,
        book_price: price,
        book_carry_a: ZERO,
        book_carry_b: ZERO,
        route_history: vec![RouteEvent { at: env.now, kind: "open".into(), from: None, to: Book::B, volume: vol, price, staff: "Options order book".into(), reason: "Novation: the house's side of client positions moved to the market maker".into(), related_ticket: None }],
        price_corrected: false,
        last_swap_day: None,
        client_order_id: None,
        reversed_from: None,
        option: Some(terms.clone()),
        combo_id: None,
        premium: cash,
        venue: Some(Venue::Book),
        margin_leverage: None,
    };
    let deal = Deal {
        id: deal_id,
        login,
        position_ticket: ticket,
        order_ticket: None,
        symbol: series.to_string(),
        side,
        position_side: side,
        entry: DealEntry::In,
        volume: vol,
        price,
        profit: ZERO,
        swap: ZERO,
        commission: ZERO,
        reason: DealReason::Novation,
        book: Book::B,
        time: env.now,
        open_price: price,
        open_time: env.now,
        source: Source::System,
        comment: format!("novation {idem}"),
        price_correction: false,
        ledger_txn: txn_id,
        staff: None,
        reason_code: None,
        snapshot: None,
        client_order_id: None,
        partial: false,
        option: Some(DealOption { terms: terms.clone(), cash, usd_per_quote: ONE, spot: None, fixing: None, run: None, combo_id: None, charged: ZERO, fill: None, rebate: ZERO }),
    };
    tx.emit(Event::PositionOpened { position: pos, deal: Some(deal) });
    Ok(json!({"series": series, "ticket": ticket, "contracts": num(contracts)}))
}

/// The client's side: the same position, now on the order book (it keeps its price, premium and P&L).
fn novate_client(tx: &mut crate::engine::Tx, env: &crate::engine::Env, ticket: i64) -> Result<Value, crate::engine::Reject> {
    let Some(p) = tx.st.positions.get(&ticket).cloned() else { return Ok(json!({"ticket": ticket, "gone": true})) };
    if p.on_book() {
        return Ok(json!({"ticket": ticket, "already": true}));
    }
    let mut np = p.clone();
    np.venue = Some(Venue::Book);
    np.route_history.push(RouteEvent { at: env.now, kind: "novation".into(), from: Some(p.book), to: p.book, volume: p.volume, price: p.open_price, staff: "Options order book".into(), reason: "Novated to the options order book: close it with an order on the book".into(), related_ticket: None });
    tx.emit(Event::PositionUpdated { position: np, change: "novation".into(), deal: None });
    tx.note("position_novated", format!("Your option position #{ticket} ({}) now trades on the options order book. Your position and P&L are unchanged.", p.symbol), json!({"ticket": ticket, "series": p.symbol, "options": true, "book": true}));
    Ok(json!({"ticket": ticket, "novated": true}))
}

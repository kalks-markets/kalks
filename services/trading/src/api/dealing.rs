//! Back Office dealing desk — the REST contract documented at the top of
//! `apps/admin/lib/trading-desk/store.ts` (D117).
//!
//! Every write carries `{ reasonCode, note }` and the staff identity headers, and returns
//! `{ data, audit[] }` (or `{ error, audit? }`). The engine writes each audit entry in the same database
//! transaction as the change it describes; rejected attempts are audited as `trade.rejected`.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::sync::Arc;

use super::terminal::{OrderPatchBody, order_patch, parse_expiry, parse_kind, parse_side, parse_time};
use super::{ApiError, ApiResult, AppState, Body, ROLES_CONFIG, ROLES_DEALING, StaffCtx, parse_ticket};
use crate::engine::dealing::{self, BookMove};
use crate::engine::trade::{self, CloseReq, DealerCtx, OrderReq, PlaceResult, PositionPatch};
use crate::engine::{AuditDraft, funds};
use crate::model::{Book, Controls, Deal, OrderType, Side, Source};
use crate::money::{D, de_dec, de_opt_dec, num, num_opt};
use crate::persist::{self, AuditRow};
use crate::rules::{ControlMode, RoutingAction, RoutingCondition, RoutingRule, SymbolControl};
use crate::shard::{Done, ExecError, Op, ScanFn};
use crate::views;

/* ------------------------------------------------------------------ */
/* Reasons, audit, execution helpers                                   */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Reason {
    #[serde(default)]
    pub reason_code: String,
    #[serde(default)]
    pub note: String,
}

pub fn check_reason(r: &Reason) -> ApiResult<()> {
    if r.reason_code.trim().is_empty() {
        return Err(ApiError::Validation { field: "reasonCode", message: "Select a reason code".into() });
    }
    if r.reason_code.starts_with("DLR-99") && r.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "A note is required for reason “Other”".into() });
    }
    Ok(())
}

pub(crate) fn dealer(s: &StaffCtx, r: &Reason, force: bool) -> DealerCtx {
    DealerCtx { staff: s.staff.name.clone(), reason_code: r.reason_code.clone(), force }
}

pub(crate) fn row(s: &StaffCtx, r: &Reason, action: &str, tickets: Vec<String>, login: Option<i64>, symbol: Option<String>, before: Option<Value>, after: Option<Value>, flags: Vec<String>) -> AuditRow {
    AuditRow {
        tenant_id: s.ctx.tenant.tenant_id,
        at: Utc::now(),
        staff_id: s.staff.id.clone(),
        staff_name: s.staff.name.clone(),
        staff_role: s.staff.role.clone(),
        action: action.into(),
        tickets,
        login,
        symbol,
        before,
        after,
        reason_code: r.reason_code.clone(),
        note: r.note.trim().to_string(),
        flags,
    }
}

/// Writes a standalone audit entry (tenant-level changes, rejections) and returns it as JSON.
pub async fn audit_now(st: &AppState, a: AuditRow) -> ApiResult<Value> {
    let id = persist::insert_audit(&st.pool, &a).await?;
    let v = persist::audit_json(id, &a);
    if let Some(tx) = dealing_stream(st, a.tenant_id) {
        let _ = tx.send(Arc::from(json!({"type": "audit", "entry": v}).to_string()));
    }
    Ok(v)
}

fn dealing_stream(st: &AppState, tenant: i64) -> Option<tokio::sync::broadcast::Sender<Arc<str>>> {
    st.hub.shared.streams.dealing(tenant)
}

/// Runs a dealer operation on one account; a rejection is audited as `trade.rejected`.
async fn dexec(st: &AppState, s: &StaffCtx, login: i64, r: &Reason, attempted: &str, tickets: Vec<String>, symbol: Option<String>, op: Op) -> ApiResult<Done> {
    let meta = st.hub.meta(login).filter(|m| m.tenant_id == s.ctx.tenant.tenant_id).ok_or_else(|| ApiError::NotFound(format!("Unknown account {login}")))?;
    let _ = meta;
    match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &r.reason_code, r.note.trim(), None, op).await {
        Ok(d) => Ok(d),
        Err(ExecError::Reject(rej)) => {
            let entry = audit_now(st, row(s, r, "trade.rejected", tickets, Some(login), symbol, None, Some(json!({"attempted": attempted, "error": rej.message})), vec!["rejected".into()])).await?;
            Err(ApiError::Reject { reject: rej, audit: vec![entry] })
        }
        Err(e) => Err(e.into()),
    }
}

pub(crate) fn reply(data: Value, audit: Vec<Value>) -> Json<Value> {
    Json(json!({"data": data, "audit": audit}))
}

pub(crate) fn login_of(st: &AppState, s: &StaffCtx, ticket: i64) -> ApiResult<i64> {
    let login = st.hub.login_of_ticket(ticket).ok_or_else(|| ApiError::NotFound(format!("Ticket #{ticket} not found")))?;
    match st.hub.meta(login) {
        Some(m) if m.tenant_id == s.ctx.tenant.tenant_id => Ok(login),
        _ => Err(ApiError::NotFound(format!("Ticket #{ticket} not found"))),
    }
}

pub(crate) fn draft(action: &'static str, tickets: Vec<i64>, symbol: Option<String>, before: Option<Value>, after: Option<Value>, flags: Vec<&str>) -> AuditDraft {
    AuditDraft { action, tickets, login: None, symbol, before, after, flags: flags.into_iter().map(String::from).collect() }
}

/* ------------------------------------------------------------------ */
/* Reads                                                               */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Default, Clone)]
pub struct PosQ {
    book: Option<String>,
    group: Option<String>,
    symbol: Option<String>,
    source: Option<String>,
    login: Option<i64>,
}

fn positions_scan(q: PosQ) -> ScanFn {
    Arc::new(move |a, env| {
        if q.login.is_some_and(|l| l != a.login()) || q.group.as_ref().is_some_and(|g| g != &a.account.group) {
            return vec![];
        }
        a.positions
            .values()
            .filter(|p| q.book.as_deref().is_none_or(|b| p.book.as_str() == b))
            .filter(|p| q.symbol.as_deref().is_none_or(|s| p.symbol == s))
            .filter(|p| q.source.as_deref().is_none_or(|s| p.source.as_str() == s))
            .map(|p| views::desk_position_json(env, a, p))
            .collect()
    })
}

fn orders_scan(q: PosQ) -> ScanFn {
    Arc::new(move |a, _| {
        if q.login.is_some_and(|l| l != a.login()) || q.group.as_ref().is_some_and(|g| g != &a.account.group) {
            return vec![];
        }
        a.orders.values().filter(|o| q.symbol.as_deref().is_none_or(|s| o.symbol == s)).filter(|o| q.source.as_deref().is_none_or(|s| o.source.as_str() == s)).map(|o| views::desk_order_json(a, o)).collect()
    })
}

fn by_time(mut v: Vec<Value>, key: &str) -> Vec<Value> {
    v.sort_by(|a, b| b[key].as_str().unwrap_or("").cmp(a[key].as_str().unwrap_or("")));
    v
}

pub async fn positions(State(st): State<AppState>, s: StaffCtx, Query(q): Query<PosQ>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(by_time(st.hub.scan(s.ctx.tenant.tenant_id, positions_scan(q)).await, "openTime"))))
}

pub async fn orders(State(st): State<AppState>, s: StaffCtx, Query(q): Query<PosQ>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(by_time(st.hub.scan(s.ctx.tenant.tenant_id, orders_scan(q)).await, "placed"))))
}

#[derive(Deserialize, Default)]
pub struct DealsQ {
    login: Option<i64>,
    symbol: Option<String>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
}

async fn closing_deals(st: &AppState, tenant: i64, q: &DealsQ) -> ApiResult<Vec<Value>> {
    let rows = sqlx::query(
        "SELECT d.data, d.reversed, a.user_id FROM deals d JOIN accounts a ON a.login = d.login
         WHERE d.tenant_id = $1 AND d.entry <> 'in' AND ($2::bigint IS NULL OR d.login = $2) AND ($3::text IS NULL OR d.symbol = $3)
           AND ($4::timestamptz IS NULL OR d.time >= $4) AND ($5::timestamptz IS NULL OR d.time < $5)
         ORDER BY d.time DESC, d.id DESC LIMIT $6",
    )
    .bind(tenant)
    .bind(q.login)
    .bind(&q.symbol)
    .bind(parse_time(&q.from)?)
    .bind(parse_time(&q.to)?)
    .bind(q.limit.unwrap_or(200).clamp(1, 2000))
    .fetch_all(&st.pool)
    .await?;
    Ok(rows
        .iter()
        .map(|r| {
            let d: sqlx::types::Json<Deal> = r.get("data");
            views::desk_deal_json(&d.0, r.get("user_id"), r.get("reversed"))
        })
        .collect())
}

/// Closing deals (DeskDeal) — the statement rows a dealer can reopen.
pub async fn deals(State(st): State<AppState>, s: StaffCtx, Query(q): Query<DealsQ>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(closing_deals(&st, s.ctx.tenant.tenant_id, &q).await?)))
}

async fn account_controls(st: &AppState, tenant: i64) -> Vec<Value> {
    st.hub
        .scan(
            tenant,
            Arc::new(|a, _| {
                let c = &a.account.controls;
                if *c == Controls::default() {
                    return vec![];
                }
                vec![json!({
                    "login": a.login().to_string(), "clientId": a.account.user_id.to_string(), "group": a.account.group,
                    "tradingDisabled": c.trading_disabled, "closeOnly": c.close_only, "maxLot": num_opt(c.max_lot),
                    "execDelayMs": c.exec_delay_ms, "markupPips": num(c.markup_pips), "reason": c.reason, "setBy": c.set_by, "updated": c.updated,
                })]
            }),
        )
        .await
}

/// Everything the desk needs to hydrate (DeskState without the audit log).
pub async fn state(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let t = &s.ctx.tenant;
    let positions = by_time(st.hub.scan(t.tenant_id, positions_scan(PosQ::default())).await, "openTime");
    let orders = by_time(st.hub.scan(t.tenant_id, orders_scan(PosQ::default())).await, "placed");
    let deals = closing_deals(&st, t.tenant_id, &DealsQ::default()).await?;
    Ok(Json(json!({
        "positions": positions,
        "orders": orders,
        "deals": deals,
        "symbolControls": t.symbol_controls,
        "accountControls": account_controls(&st, t.tenant_id).await,
        "routingRules": t.routing_rules,
        "tenant": t.policy,
        "groups": t.groups.values().map(|g| json!({"code": g.code, "name": g.name})).collect::<Vec<_>>(),
    })))
}

pub async fn controls(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let t = &s.ctx.tenant;
    Ok(Json(json!({"symbolControls": t.symbol_controls, "accountControls": account_controls(&st, t.tenant_id).await, "tenant": t.policy})))
}

/* ------------------------------------------------------------------ */
/* Trades                                                              */
/* ------------------------------------------------------------------ */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTrade {
    login: String,
    symbol: String,
    side: String,
    #[serde(rename = "type", default)]
    kind: Option<String>,
    #[serde(deserialize_with = "de_dec")]
    volume: D,
    #[serde(default, deserialize_with = "de_opt_dec")]
    price: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    stop_limit: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    sl: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    tp: Option<D>,
    #[serde(default)]
    book: Option<String>,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    expiry: Option<String>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn create_trade(State(st): State<AppState>, s: StaffCtx, Body(b): Body<CreateTrade>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let login: i64 = b.login.trim().parse().map_err(|_| ApiError::BadRequest(format!("Unknown account {}", b.login)))?;
    let kind = parse_kind(b.kind.as_deref())?;
    let side = parse_side(&b.side)?;
    let manual = kind == OrderType::Market && b.price.is_some_and(|p| p > D::ZERO);
    if manual && b.reason.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "A manual price needs a note explaining where the price comes from".into() });
    }
    let book = match b.book.as_deref() {
        None | Some("") => None,
        Some(x) => Some(Book::parse(x).ok_or(ApiError::Validation { field: "book", message: "book must be A or B".into() })?),
    };
    let (expiry, expiry_at) = parse_expiry(b.expiry.as_deref(), None)?;
    let delay = if kind == OrderType::Market && !manual { super::terminal::exec_delay(&st, &s.ctx.tenant, login).await } else { 0 };
    let symbol = b.symbol.trim().to_uppercase();
    let req = OrderReq {
        symbol: symbol.clone(),
        side,
        kind,
        volume: b.volume,
        price: b.price,
        stop_limit: b.stop_limit,
        sl: b.sl,
        tp: b.tp,
        trailing_points: None,
        expiry,
        expiry_at,
        deviation_points: None,
        requested_price: None,
        oco_with: None,
        source: Source::Dealer,
        platform: "Back Office".into(),
        comment: b.comment.clone().unwrap_or_default().chars().take(128).collect(),
        client_order_id: None,
        book,
        dealer: Some(dealer(&s, &b.reason, false)),
    };
    let sym = symbol.clone();
    let op: Op = Box::new(move |tx, env| {
        let market = env.quote(&tx.st.account, &sym).map(|q| q.open_price(side));
        let r = trade::place_order(tx, env, req)?;
        match &r {
            PlaceResult::Filled { order_ticket, position_ticket, price, book, .. } => {
                let t = position_ticket.unwrap_or(*order_ticket);
                let p = tx.st.positions.get(&t);
                let mut flags = vec!["dealer"];
                if manual {
                    flags.push("manual price");
                }
                let delay_flag = format!("execution delay {delay} ms");
                let mut d = draft(
                    "position.open",
                    vec![t],
                    Some(sym.clone()),
                    None,
                    Some(json!({"side": side.as_str(), "volume": num(b.volume), "openPrice": num(*price), "market": num_opt(market), "sl": num_opt(p.and_then(|p| p.sl)), "tp": num_opt(p.and_then(|p| p.tp)), "book": book.as_str(), "margin": num(crate::money::r2(crate::engine::total_margin(env, &tx.st, None))), "comment": b.comment, "source": "dealer"})),
                    flags,
                );
                if delay > 0 {
                    d.flags.push(delay_flag);
                }
                tx.audit.push(d);
                Ok(json!({"ticket": t.to_string(), "kind": "position", "price": num(*price), "book": book.as_str(), "delayMs": delay}))
            }
            PlaceResult::Pending { ticket, price, book } => {
                let o = tx.st.orders[ticket].clone();
                let rb = book.unwrap_or_else(|| trade::route_for(env, &tx.st, &sym, b.volume, None, *ticket).0);
                tx.audit.push(draft(
                    "order.place",
                    vec![*ticket],
                    Some(sym.clone()),
                    None,
                    Some(json!({"type": views::desk_order_type(&o), "volume": num(o.volume), "price": num(o.price), "stopLimit": num_opt(o.stop_limit), "sl": num_opt(o.sl), "tp": num_opt(o.tp), "book": rb.as_str(), "expiry": views::expiry_str(o.expiry), "source": "dealer"})),
                    vec![],
                ));
                Ok(json!({"ticket": ticket.to_string(), "kind": "order", "price": num(*price), "book": rb.as_str(), "delayMs": 0}))
            }
            PlaceResult::Duplicate { ticket } => Ok(json!({"ticket": ticket.to_string(), "kind": "duplicate"})),
        }
    });
    let done = dexec(&st, &s, login, &b.reason, "create trade", vec![], Some(symbol), op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyBody {
    #[serde(default, deserialize_with = "crate::money::patch_dec")]
    sl: Option<Option<D>>,
    #[serde(default, deserialize_with = "crate::money::patch_dec")]
    tp: Option<Option<D>>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn modify_position(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<ModifyBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let d = dealer(&s, &b.reason, false);
    let patch = PositionPatch { sl: b.sl, tp: b.tp, trailing_points: None };
    let op: Op = Box::new(move |tx, env| {
        let (p0, p1) = trade::modify_position(tx, env, ticket, patch, Some(&d))?;
        tx.audit.push(draft("position.modify", vec![ticket], Some(p0.symbol.clone()), Some(json!({"sl": num_opt(p0.sl), "tp": num_opt(p0.tp)})), Some(json!({"sl": num_opt(p1.sl), "tp": num_opt(p1.tp)})), vec![]));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &b.reason, "modify", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseBody {
    #[serde(default, deserialize_with = "de_opt_dec")]
    volume: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    price: Option<D>,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    stop_out: bool,
    #[serde(flatten)]
    reason: Reason,
}

fn is_error_correction(r: &Reason) -> bool {
    r.reason_code.starts_with("DLR-02")
}

pub async fn close(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<CloseBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let ticket = parse_ticket(&ticket)?;
    let corrected = b.price.is_some_and(|p| p > D::ZERO);
    if corrected && !is_error_correction(&b.reason) {
        return Err(ApiError::Validation { field: "reasonCode", message: "Closing at a specified price is a price correction — use reason DLR-02 · Error correction".into() });
    }
    if corrected && b.reason.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "A price correction needs a note (source of the correct price)".into() });
    }
    let login = login_of(&st, &s, ticket)?;
    let force = b.force || b.stop_out;
    let d = dealer(&s, &b.reason, force);
    let (vol, price, stop_out) = (b.volume, b.price, b.stop_out);
    let op: Op = Box::new(move |tx, env| {
        let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| crate::engine::Reject::new("not_found", format!("Position #{ticket} not found")))?;
        if vol.is_some_and(|v| v >= p.volume) && vol != Some(p.volume) {
            return Err(crate::engine::Reject::new("invalid_volume", format!("Partial close must be below {} lots — use Close for the full volume", p.volume.normalize())));
        }
        let market = env.quote(&tx.st.account, &p.symbol).map(|q| q.close_price(p.side));
        let (deal, profit) = trade::close_position(tx, env, ticket, CloseReq { volume: vol, price, dealer: Some(d.clone()), stop_out, ..Default::default() })?;
        let closed = vol.unwrap_or(p.volume);
        let partial = closed < p.volume;
        let action = if partial { "position.partial_close" } else if stop_out { "position.stop_out" } else if d.force { "position.force_close" } else { "position.close" };
        let mut flags = vec![];
        if corrected {
            flags.push("price correction");
        }
        if d.force {
            flags.push("forced");
        }
        let close_px = price.map(|x| env.specs.get(&p.symbol).map(|s| s.round_price(x)).unwrap_or(x)).or(market);
        tx.audit.push(draft(
            action,
            vec![ticket],
            Some(p.symbol.clone()),
            Some(json!({"volume": num(p.volume), "book": p.book.as_str()})),
            Some(json!({"closedVolume": num(closed), "remaining": num(p.volume - closed), "closePrice": num_opt(close_px), "market": num_opt(market), "profit": num(profit), "deal": deal.to_string()})),
            flags,
        ));
        Ok(json!({"dealId": deal.to_string(), "profit": num(profit)}))
    });
    let done = dexec(&st, &s, login, &b.reason, "close", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
pub struct AddBody {
    #[serde(deserialize_with = "de_dec")]
    volume: D,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn add_volume(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<AddBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let d = dealer(&s, &b.reason, false);
    let op: Op = Box::new(move |tx, env| {
        let sym = tx.st.positions.get(&ticket).map(|p| p.symbol.clone());
        let (before, after) = dealing::add_volume(tx, env, ticket, b.volume, &d)?;
        tx.audit.push(draft("position.add_volume", vec![ticket], sym, Some(before), Some(after), vec![]));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &b.reason, "add volume", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorrectionBody {
    #[serde(deserialize_with = "de_dec")]
    open_price: D,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn price_correction(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<CorrectionBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    if !is_error_correction(&b.reason) {
        return Err(ApiError::Validation { field: "reasonCode", message: "Price edits are restricted to error correction (DLR-02)".into() });
    }
    if b.reason.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "A price correction needs a note (source of the correct price)".into() });
    }
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let op: Op = Box::new(move |tx, env| {
        let sym = tx.st.positions.get(&ticket).map(|p| p.symbol.clone());
        let (before, after) = dealing::price_correction(tx, env, ticket, b.open_price)?;
        tx.audit.push(draft("position.price_correction", vec![ticket], sym, Some(before), Some(after), vec!["price correction", "client statement"]));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &b.reason, "price correction", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
pub struct ChargesBody {
    #[serde(default, deserialize_with = "de_opt_dec")]
    swap: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    commission: Option<D>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn charges(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<ChargesBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let d = dealer(&s, &b.reason, false);
    let op: Op = Box::new(move |tx, env| {
        let sym = tx.st.positions.get(&ticket).map(|p| p.symbol.clone());
        let (before, after) = dealing::adjust_charges(tx, env, ticket, b.swap, b.commission, &d)?;
        tx.audit.push(draft("position.adjust_charges", vec![ticket], sym, Some(before), Some(after), vec!["charges adjustment"]));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &b.reason, "adjust charges", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

pub async fn void(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(r): Body<Reason>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&r)?;
    if !(r.reason_code.starts_with("DLR-02") || r.reason_code.starts_with("DLR-06")) {
        return Err(ApiError::Validation { field: "reasonCode", message: "Voiding a trade is limited to error correction or technical issue".into() });
    }
    if r.note.trim().is_empty() {
        return Err(ApiError::Validation { field: "note", message: "Voiding a trade needs a note".into() });
    }
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let d = dealer(&s, &r, false);
    let op: Op = Box::new(move |tx, env| {
        let sym = tx.st.positions.get(&ticket).map(|p| p.symbol.clone());
        let before = dealing::void_position(tx, env, ticket, &d)?;
        tx.audit.push(draft("position.void", vec![ticket], sym, Some(before), Some(json!({"status": "void — no P&L booked"})), vec!["void"]));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &r, "void", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

pub async fn reopen(State(st): State<AppState>, s: StaffCtx, Path(id): Path<String>, Body(r): Body<Reason>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&r)?;
    let id: i64 = id.trim().trim_start_matches('D').parse().map_err(|_| ApiError::BadRequest(format!("Invalid deal id {id}")))?;
    let deal = persist::load_deal(&st.pool, id).await?.ok_or_else(|| ApiError::NotFound(format!("Deal {id} not found")))?;
    match st.hub.meta(deal.login) {
        Some(m) if m.tenant_id == s.ctx.tenant.tenant_id => {}
        _ => return Err(ApiError::NotFound(format!("Deal {id} not found"))),
    }
    let d = dealer(&s, &r, false);
    let login = deal.login;
    let op: Op = Box::new(move |tx, env| {
        let (before, after) = dealing::reopen_deal(tx, env, &deal, &d)?;
        tx.audit.push(draft("deal.reopen", vec![deal.position_ticket], Some(deal.symbol.clone()), Some(before), Some(after), vec!["reopen"]));
        Ok(json!({"ticket": deal.position_ticket.to_string()}))
    });
    let done = dexec(&st, &s, login, &r, "reopen deal", vec![id.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
pub struct BookBody {
    tickets: Vec<String>,
    to: String,
    #[serde(default, deserialize_with = "de_opt_dec")]
    volume: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pct: Option<D>,
    #[serde(flatten)]
    reason: Reason,
}

/// A/B book transfer: full, or partial (split into a linked child ticket).
pub async fn book_transfers(State(st): State<AppState>, s: StaffCtx, Body(b): Body<BookBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let to = Book::parse(&b.to).ok_or(ApiError::Validation { field: "to", message: "to must be A or B".into() })?;
    if b.pct.is_some_and(|p| p <= D::ZERO || p > D::ONE_HUNDRED) {
        return Err(ApiError::Validation { field: "pct", message: "pct must be 1–100".into() });
    }
    let many = b.tickets.len() > 1 || b.pct.is_some();
    let (mut done, mut failed, mut created, mut audit) = (vec![], vec![], vec![], vec![]);
    for t in &b.tickets {
        let Ok(ticket) = parse_ticket(t) else {
            failed.push(json!({"ticket": t, "error": "invalid ticket"}));
            continue;
        };
        let Ok(login) = login_of(&st, &s, ticket) else {
            failed.push(json!({"ticket": t, "error": "not found"}));
            continue;
        };
        let d = dealer(&s, &b.reason, false);
        let mv = match (b.volume, b.pct) {
            (Some(v), _) => BookMove::Volume(v),
            (None, Some(p)) => BookMove::Pct(p),
            _ => BookMove::Full,
        };
        let op: Op = Box::new(move |tx, env| {
            let sym = tx.st.positions.get(&ticket).map(|p| p.symbol.clone());
            let (child, before, after, split) = dealing::transfer_book(tx, env, ticket, to, mv, &d)?;
            let mut flags = vec![];
            if many {
                flags.push("bulk");
            }
            if split {
                flags.push("partial transfer");
            }
            let tickets = match child {
                Some(c) => vec![ticket, c],
                None => vec![ticket],
            };
            tx.audit.push(draft(if split { "book.split" } else { "book.transfer" }, tickets, sym, Some(before), Some(after), flags));
            Ok(json!(child))
        });
        match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &b.reason.reason_code, b.reason.note.trim(), None, op).await {
            Ok(r) => {
                done.push(json!(t));
                if let Some(c) = r.value.as_i64() {
                    created.push(json!(c.to_string()));
                }
                audit.extend(r.audit);
            }
            Err(ExecError::Reject(e)) => failed.push(json!({"ticket": t, "error": e.message})),
            Err(e) => failed.push(json!({"ticket": t, "error": format!("{e:?}")})),
        }
    }
    Ok(reply(json!({"done": done, "failed": failed, "created": created}), audit))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkBody {
    tickets: Vec<String>,
    op: String,
    #[serde(default)]
    force: bool,
    #[serde(default, deserialize_with = "de_opt_dec")]
    sl_pct: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    tp_pct: Option<D>,
    #[serde(default)]
    clear: Option<String>,
    #[serde(flatten)]
    reason: Reason,
}

/// Bulk close / bulk SL-TP by % of the current price.
pub async fn bulk(State(st): State<AppState>, s: StaffCtx, Body(b): Body<BulkBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    if b.op != "close" && b.op != "modify" {
        return Err(ApiError::Validation { field: "op", message: "op must be close or modify".into() });
    }
    let (mut done, mut failed, mut audit, mut profit) = (vec![], vec![], vec![], D::ZERO);
    for t in &b.tickets {
        let Ok(ticket) = parse_ticket(t) else {
            failed.push(json!({"ticket": t, "error": "invalid ticket"}));
            continue;
        };
        let Ok(login) = login_of(&st, &s, ticket) else {
            failed.push(json!({"ticket": t, "error": "not found"}));
            continue;
        };
        let d = dealer(&s, &b.reason, b.force);
        let (op_kind, slp, tpp, clear) = (b.op.clone(), b.sl_pct, b.tp_pct, b.clear.clone());
        let op: Op = Box::new(move |tx, env| {
            let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| crate::engine::Reject::new("not_found", "not found"))?;
            if op_kind == "close" {
                let (deal, pr) = trade::close_position(tx, env, ticket, CloseReq { dealer: Some(d.clone()), ..Default::default() })?;
                let px = tx.st.positions.get(&ticket).map(|_| D::ZERO);
                let _ = px;
                let mut flags = vec!["bulk"];
                if d.force {
                    flags.push("forced");
                }
                tx.audit.push(draft(if d.force { "position.force_close" } else { "position.close" }, vec![ticket], Some(p.symbol.clone()), Some(json!({"volume": num(p.volume), "book": p.book.as_str()})), Some(json!({"closedVolume": num(p.volume), "profit": num(pr), "deal": deal.to_string()})), flags));
                return Ok(json!(pr.to_string()));
            }
            if p.option.is_some() {
                return Err(crate::engine::Reject::new("not_supported", "SL / TP in % of the price is for CFD positions; set an option's premium SL / TP on the position"));
            }
            let q = env.live_quote(&tx.st.account, &p.symbol)?;
            let spec = env.spec(&p.symbol)?.clone();
            let refp = q.close_price(p.side);
            let dir = if p.side == Side::Buy { D::ONE } else { -D::ONE };
            let mut sl = p.sl;
            let mut tp = p.tp;
            if matches!(clear.as_deref(), Some("sl") | Some("both")) {
                sl = None;
            }
            if matches!(clear.as_deref(), Some("tp") | Some("both")) {
                tp = None;
            }
            if let Some(x) = slp {
                sl = Some(spec.round_price(refp * (D::ONE - dir * x / D::ONE_HUNDRED)));
            }
            if let Some(x) = tpp {
                tp = Some(spec.round_price(refp * (D::ONE + dir * x / D::ONE_HUNDRED)));
            }
            let (p0, p1) = trade::modify_position(tx, env, ticket, PositionPatch { sl: Some(sl), tp: Some(tp), trailing_points: None }, Some(&d))?;
            tx.audit.push(draft("position.modify", vec![ticket], Some(p0.symbol.clone()), Some(json!({"sl": num_opt(p0.sl), "tp": num_opt(p0.tp)})), Some(json!({"sl": num_opt(p1.sl), "tp": num_opt(p1.tp)})), vec!["bulk"]));
            Ok(Value::Null)
        });
        match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &b.reason.reason_code, b.reason.note.trim(), None, op).await {
            Ok(r) => {
                done.push(json!(t));
                if let Some(p) = r.value.as_str().and_then(|v| v.parse::<D>().ok()) {
                    profit += p;
                }
                audit.extend(r.audit);
            }
            Err(ExecError::Reject(e)) => failed.push(json!({"ticket": t, "error": e.message})),
            Err(e) => failed.push(json!({"ticket": t, "error": format!("{e:?}")})),
        }
    }
    let mut data = json!({"done": done, "failed": failed});
    if b.op == "close" {
        data["profit"] = num(profit);
    }
    Ok(reply(data, audit))
}

/* ------------------------------------------------------------------ */
/* Pending orders                                                      */
/* ------------------------------------------------------------------ */

#[derive(Deserialize)]
pub struct OrderModifyBody {
    #[serde(flatten)]
    patch: OrderPatchBody,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn modify_order(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<OrderModifyBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let patch = order_patch(&b.patch)?;
    let d = dealer(&s, &b.reason, false);
    let op: Op = Box::new(move |tx, env| {
        let (o0, o1) = trade::modify_order(tx, env, ticket, patch, Some(&d))?;
        let pick = |o: &crate::model::Order| json!({"price": num(o.price), "volume": num(o.volume), "sl": num_opt(o.sl), "tp": num_opt(o.tp), "expiry": views::expiry_str(o.expiry)});
        tx.audit.push(draft("order.modify", vec![ticket], Some(o0.symbol.clone()), Some(pick(&o0)), Some(pick(&o1)), vec![]));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &b.reason, "modify order", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
pub struct CancelBody {
    tickets: Vec<String>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn cancel_orders(State(st): State<AppState>, s: StaffCtx, Body(b): Body<CancelBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let many = b.tickets.len() > 1;
    let (mut done, mut failed, mut audit) = (vec![], vec![], vec![]);
    for t in &b.tickets {
        let Some((ticket, login)) = parse_ticket(t).ok().and_then(|k| login_of(&st, &s, k).ok().map(|l| (k, l))) else {
            failed.push(json!({"ticket": t, "error": "not found"}));
            continue;
        };
        let op: Op = Box::new(move |tx, env| {
            let o = trade::cancel_order(tx, env, ticket, "cancelled by dealer")?;
            tx.audit.push(draft("order.cancel", vec![ticket], Some(o.symbol.clone()), Some(json!({"type": views::desk_order_type(&o), "volume": num(o.volume), "price": num(o.price)})), Some(json!({"status": "cancelled"})), if many { vec!["bulk"] } else { vec![] }));
            Ok(Value::Null)
        });
        match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &b.reason.reason_code, b.reason.note.trim(), None, op).await {
            Ok(r) => {
                done.push(json!(t));
                audit.extend(r.audit);
            }
            Err(ExecError::Reject(e)) => failed.push(json!({"ticket": t, "error": e.message})),
            Err(e) => failed.push(json!({"ticket": t, "error": format!("{e:?}")})),
        }
    }
    Ok(reply(json!({"done": done, "failed": failed}), audit))
}

pub async fn fill_order(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(r): Body<Reason>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&r)?;
    let ticket = parse_ticket(&ticket)?;
    let login = login_of(&st, &s, ticket)?;
    let d = dealer(&s, &r, false);
    let op: Op = Box::new(move |tx, env| {
        let o = tx.st.orders.get(&ticket).cloned().ok_or_else(|| crate::engine::Reject::new("not_found", format!("Order #{ticket} not found")))?;
        let (pos, price, book) = dealing::fill_order(tx, env, ticket, &d)?;
        let pt = pos.unwrap_or(ticket);
        tx.audit.push(draft("position.open", vec![pt], Some(o.symbol.clone()), None, Some(json!({"side": o.side.as_str(), "volume": num(o.volume), "openPrice": num(price), "book": book.as_str(), "source": "dealer"})), vec!["dealer"]));
        tx.audit.push(draft("order.fill", vec![ticket, pt], Some(o.symbol.clone()), Some(json!({"order": ticket.to_string(), "price": num(o.price)})), Some(json!({"position": pt.to_string(), "fillPrice": num(price)})), vec!["dealer fill"]));
        Ok(json!({"ticket": pt.to_string()}))
    });
    let done = dexec(&st, &s, login, &r, "fill order", vec![ticket.to_string()], None, op).await?;
    Ok(reply(done.value, done.audit))
}

/* ------------------------------------------------------------------ */
/* Controls                                                            */
/* ------------------------------------------------------------------ */

#[derive(Deserialize)]
pub struct SymbolControlBody {
    #[serde(default = "all")]
    group: String,
    mode: Option<String>,
    #[serde(flatten)]
    reason: Reason,
}

fn all() -> String {
    "all".into()
}

pub async fn symbol_control(State(st): State<AppState>, s: StaffCtx, Path(symbol): Path<String>, Body(b): Body<SymbolControlBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let symbol = symbol.to_uppercase();
    if st.hub.shared.specs.load().get(&symbol).is_none() {
        return Err(ApiError::NotFound(format!("Unknown symbol {symbol}")));
    }
    let t = s.ctx.tenant.clone();
    if b.group != "all" && !t.groups.contains_key(&b.group) {
        return Err(ApiError::Validation { field: "group", message: "Unknown group".into() });
    }
    let mode = match b.mode.as_deref() {
        None | Some("") | Some("trading") => None,
        Some("halt") => Some(ControlMode::Halt),
        Some("close-only") | Some("close_only") => Some(ControlMode::CloseOnly),
        _ => return Err(ApiError::Validation { field: "mode", message: "mode must be halt, close-only or null".into() }),
    };
    let prev = t.symbol_controls.iter().find(|c| c.symbol == symbol && c.group == b.group).cloned();
    let mut tx = st.pool.begin().await?;
    sqlx::query("DELETE FROM symbol_controls WHERE tenant_id = $1 AND symbol = $2 AND group_code = $3").bind(t.tenant_id).bind(&symbol).bind(&b.group).execute(&mut *tx).await?;
    let new = match mode {
        Some(m) => {
            let c = SymbolControl { id: format!("SC-{}", crate::auth::random_token(6)), symbol: symbol.clone(), group: b.group.clone(), mode: m, reason_code: b.reason.reason_code.clone(), note: Some(b.reason.note.clone()).filter(|n| !n.is_empty()), staff: s.staff.name.clone(), at: Utc::now() };
            sqlx::query("INSERT INTO symbol_controls (id, tenant_id, symbol, group_code, mode, reason_code, note, staff, at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(&c.id)
                .bind(t.tenant_id)
                .bind(&c.symbol)
                .bind(&c.group)
                .bind(if m == ControlMode::Halt { "halt" } else { "close-only" })
                .bind(&c.reason_code)
                .bind(&c.note)
                .bind(&c.staff)
                .bind(c.at)
                .execute(&mut *tx)
                .await?;
            Some(c)
        }
        None => None,
    };
    let mode_str = |m: Option<ControlMode>| match m {
        Some(ControlMode::Halt) => "halt",
        Some(ControlMode::CloseOnly) => "close-only",
        None => "trading",
    };
    let a = row(&s, &b.reason, "control.symbol", vec![], None, Some(symbol.clone()), Some(json!({"scope": b.group, "mode": mode_str(prev.map(|p| p.mode))})), Some(json!({"scope": b.group, "mode": mode_str(mode)})), vec![]);
    let id = persist::insert_audit(&mut *tx, &a).await?;
    tx.commit().await?;
    st.hub.shared.registry.update(t.tenant_id, |cfg| {
        cfg.symbol_controls.retain(|c| !(c.symbol == symbol && c.group == b.group));
        if let Some(c) = new {
            cfg.symbol_controls.insert(0, c);
        }
    });
    Ok(reply(Value::Null, vec![persist::audit_json(id, &a)]))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountControlBody {
    trading_disabled: Option<bool>,
    close_only: Option<bool>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    max_lot: Option<D>,
    exec_delay_ms: Option<u32>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    markup_pips: Option<D>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn account_control(State(st): State<AppState>, s: StaffCtx, Path(login): Path<String>, Body(b): Body<AccountControlBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let login: i64 = login.trim().parse().map_err(|_| ApiError::NotFound(format!("Unknown account {login}")))?;
    let cap = s.ctx.tenant.policy.exec_delay_cap_ms.min(500);
    if b.exec_delay_ms.is_some_and(|d| d > cap) {
        return Err(ApiError::Validation { field: "execDelayMs", message: format!("Execution delay must be 0–{cap} ms") });
    }
    if b.max_lot.is_some_and(|m| m <= D::ZERO) {
        return Err(ApiError::Validation { field: "maxLot", message: "Max lot must be above 0".into() });
    }
    let (staff, reason_code) = (s.staff.name.clone(), b.reason.reason_code.clone());
    let op: Op = Box::new(move |tx, _env| {
        let c0 = tx.st.account.controls.clone();
        let mut c = c0.clone();
        if let Some(v) = b.trading_disabled {
            c.trading_disabled = v;
        }
        if let Some(v) = b.close_only {
            c.close_only = v;
        }
        if b.max_lot.is_some() {
            c.max_lot = b.max_lot;
        }
        if let Some(v) = b.exec_delay_ms {
            c.exec_delay_ms = v;
        }
        if let Some(v) = b.markup_pips {
            c.markup_pips = v;
        }
        c.reason = reason_code;
        c.set_by = staff;
        c.updated = Some(Utc::now());
        funds::set_controls(tx, c.clone())?;
        let pick = |c: &Controls| json!({"tradingDisabled": c.trading_disabled, "closeOnly": c.close_only, "maxLot": num_opt(c.max_lot), "execDelayMs": c.exec_delay_ms, "markupPips": num(c.markup_pips)});
        let flags = if b.exec_delay_ms.is_some_and(|d| d > 0) { vec!["execution delay"] } else { vec![] };
        tx.audit.push(draft("control.account", vec![], None, Some(pick(&c0)), Some(pick(&c)), flags));
        Ok(Value::Null)
    });
    let done = dexec(&st, &s, login, &b.reason, "account control", vec![], None, op).await?;
    Ok(reply(done.value, done.audit))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TenantBody {
    exec_delay_enabled: Option<bool>,
    exec_delay_cap_ms: Option<u32>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    margin_call_pct: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    stop_out_pct: Option<D>,
    #[serde(flatten)]
    reason: Reason,
}

pub async fn tenant_policy(State(st): State<AppState>, s: StaffCtx, Body(b): Body<TenantBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_CONFIG)?;
    check_reason(&b.reason)?;
    let t = s.ctx.tenant.clone();
    let mut p = t.policy.clone();
    if let Some(v) = b.exec_delay_enabled {
        p.exec_delay_enabled = v;
    }
    if let Some(v) = b.exec_delay_cap_ms {
        if v > 500 {
            return Err(ApiError::Validation { field: "execDelayCapMs", message: "Execution delay cap is at most 500 ms".into() });
        }
        p.exec_delay_cap_ms = v;
    }
    if let Some(v) = b.margin_call_pct {
        p.margin_call_pct = v;
    }
    if let Some(v) = b.stop_out_pct {
        p.stop_out_pct = v;
    }
    if p.stop_out_pct >= p.margin_call_pct || p.stop_out_pct < D::ZERO {
        return Err(ApiError::Validation { field: "stopOutPct", message: "Stop-out level must be below the margin call level".into() });
    }
    let mut tx = st.pool.begin().await?;
    sqlx::query("UPDATE tenant_policies SET exec_delay_enabled = $2, exec_delay_cap_ms = $3, margin_call_pct = $4, stop_out_pct = $5, updated_at = now() WHERE tenant_id = $1")
        .bind(t.tenant_id)
        .bind(p.exec_delay_enabled)
        .bind(p.exec_delay_cap_ms as i32)
        .bind(p.margin_call_pct)
        .bind(p.stop_out_pct)
        .execute(&mut *tx)
        .await?;
    let a = row(&s, &b.reason, "control.tenant", vec![], None, None, Some(json!(t.policy)), Some(json!(p)), vec![]);
    let id = persist::insert_audit(&mut *tx, &a).await?;
    tx.commit().await?;
    st.hub.shared.registry.update(t.tenant_id, |cfg| cfg.policy = p);
    Ok(reply(Value::Null, vec![persist::audit_json(id, &a)]))
}

/* ------------------------------------------------------------------ */
/* Routing (D2/D140)                                                   */
/* ------------------------------------------------------------------ */

pub async fn routing_rules(s: StaffCtx) -> ApiResult<Json<Value>> {
    Ok(Json(json!(s.ctx.tenant.routing_rules)))
}

pub(crate) async fn store_rules(st: &AppState, s: &StaffCtx, rules: Vec<RoutingRule>, a: AuditRow) -> ApiResult<Json<Value>> {
    let t = s.ctx.tenant.tenant_id;
    let mut tx = st.pool.begin().await?;
    sqlx::query("INSERT INTO routing_rules (tenant_id, rules, updated_at) VALUES ($1,$2, now()) ON CONFLICT (tenant_id) DO UPDATE SET rules = EXCLUDED.rules, updated_at = now()")
        .bind(t)
        .bind(sqlx::types::Json(&rules))
        .execute(&mut *tx)
        .await?;
    let id = persist::insert_audit(&mut *tx, &a).await?;
    tx.commit().await?;
    st.hub.shared.registry.update(t, |cfg| cfg.routing_rules = rules);
    Ok(reply(Value::Null, vec![persist::audit_json(id, &a)]))
}

#[derive(Deserialize)]
pub struct RulesBody {
    rules: Vec<RoutingRule>,
    #[serde(default)]
    summary: String,
    #[serde(flatten)]
    reason: Reason,
}

fn rule_list(r: &[RoutingRule]) -> String {
    r.iter().map(|x| format!("{}{}", x.id, if x.enabled { "" } else { " (off)" })).collect::<Vec<_>>().join(", ")
}

pub async fn save_routing_rules(State(st): State<AppState>, s: StaffCtx, Body(b): Body<RulesBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let a = row(&s, &b.reason, "routing.rule", vec![], None, None, Some(json!({"rules": rule_list(&s.ctx.tenant.routing_rules)})), Some(json!({"rules": rule_list(&b.rules), "change": b.summary})), vec![]);
    store_rules(&st, &s, b.rules, a).await
}

#[derive(Deserialize)]
pub struct QuickBody {
    login: Option<String>,
    group: Option<String>,
    book: Option<String>,
    #[serde(flatten)]
    reason: Reason,
}

/// Quick route: an account or group override placed ahead of the rule set (new trades only).
pub async fn quick_route(State(st): State<AppState>, s: StaffCtx, Body(b): Body<QuickBody>) -> ApiResult<Json<Value>> {
    s.require(ROLES_DEALING)?;
    check_reason(&b.reason)?;
    let (is_login, key) = match (&b.login, &b.group) {
        (Some(l), _) => (true, l.clone()),
        (None, Some(g)) => (false, g.clone()),
        _ => return Err(ApiError::Validation { field: "login", message: "login or group is required".into() }),
    };
    let book = match b.book.as_deref() {
        None | Some("") => None,
        Some(x) => Some(Book::parse(x).ok_or(ApiError::Validation { field: "book", message: "book must be A, B or null".into() })?),
    };
    let id = if is_login { format!("RQ-L{key}") } else { format!("RQ-G{key}") };
    let mut rules = s.ctx.tenant.routing_rules.clone();
    let prev = rules.iter().find(|r| r.id == id).map(|r| r.action.book);
    rules.retain(|r| r.id != id);
    if let Some(bk) = book {
        let rule = RoutingRule {
            id: id.clone(),
            name: if is_login { format!("Account {key} → {}-book", bk.as_str()) } else { format!("Group {key} → {}-book", bk.as_str()) },
            conditions: vec![if is_login { RoutingCondition { field: "Login".into(), op: "=".into(), value: key.clone() } } else { RoutingCondition { field: "Group".into(), op: "is".into(), value: key.clone() } }],
            join: "AND".into(),
            action: RoutingAction { book: bk, pct: 100.0, lp: (bk == Book::A).then(|| "Primary LP".into()) },
            enabled: true,
            hits24h: 0,
            lots24h: 0.0,
        };
        let at = if is_login { rules.iter().position(|r| !r.id.starts_with("RQ-L")) } else { rules.iter().position(|r| !r.id.starts_with("RQ-")) }.unwrap_or(rules.len());
        rules.insert(at, rule);
    }
    let scope = if is_login { format!("login {key}") } else { format!("group {key}") };
    let a = row(
        &s,
        &b.reason,
        "routing.rule",
        vec![],
        if is_login { key.parse().ok() } else { None },
        None,
        Some(json!({"scope": scope, "route": prev.map(|b| format!("{}-book", b.as_str())).unwrap_or_else(|| "rules".into())})),
        Some(json!({"scope": scope, "route": book.map(|b| format!("{}-book", b.as_str())).unwrap_or_else(|| "rules".into())})),
        vec!["new trades only".into()],
    );
    store_rules(&st, &s, rules, a).await
}

/* ------------------------------------------------------------------ */
/* Audit                                                               */
/* ------------------------------------------------------------------ */

#[derive(Deserialize)]
pub struct AuditQ {
    staff: Option<String>,
    action: Option<String>,
    ticket: Option<String>,
    login: Option<i64>,
    from: Option<String>,
    to: Option<String>,
    limit: Option<i64>,
    /// id cursor: entries older than this id
    before: Option<i64>,
}

pub async fn audit(State(st): State<AppState>, s: StaffCtx, Query(q): Query<AuditQ>) -> ApiResult<Json<Value>> {
    let rows = sqlx::query(
        "SELECT * FROM audit_log WHERE tenant_id = $1
           AND ($2::text IS NULL OR staff_id = $2 OR staff_name ILIKE $2)
           AND ($3::text IS NULL OR action = $3 OR action LIKE $3 || '.%')
           AND ($4::text IS NULL OR $4 = ANY(tickets))
           AND ($5::bigint IS NULL OR login = $5)
           AND ($6::timestamptz IS NULL OR at >= $6) AND ($7::timestamptz IS NULL OR at < $7)
           AND ($8::bigint IS NULL OR id < $8)
         ORDER BY id DESC LIMIT $9",
    )
    .bind(s.ctx.tenant.tenant_id)
    .bind(&q.staff)
    .bind(&q.action)
    .bind(&q.ticket)
    .bind(q.login)
    .bind(parse_time(&q.from)?)
    .bind(parse_time(&q.to)?)
    .bind(q.before)
    .bind(q.limit.unwrap_or(200).clamp(1, 1000))
    .fetch_all(&st.pool)
    .await?;
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            let a = AuditRow {
                tenant_id: r.get("tenant_id"),
                at: r.get("at"),
                staff_id: r.get("staff_id"),
                staff_name: r.get("staff_name"),
                staff_role: r.get("staff_role"),
                action: r.get("action"),
                tickets: r.get("tickets"),
                login: r.get("login"),
                symbol: r.get("symbol"),
                before: r.get::<Option<sqlx::types::Json<Value>>, _>("before").map(|j| j.0),
                after: r.get::<Option<sqlx::types::Json<Value>>, _>("after").map(|j| j.0),
                reason_code: r.get("reason_code"),
                note: r.get("note"),
                flags: r.get("flags"),
            };
            persist::audit_json(r.get("id"), &a)
        })
        .collect();
    Ok(Json(json!(out)))
}

pub async fn stream_ticket(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    let t = st.tickets.issue(&st.keys, crate::auth::StreamGrant::Dealing { tenant_id: s.ctx.tenant.tenant_id, staff: s.staff.name.clone() });
    Ok(Json(json!({"ticket": t, "expiresIn": 30})))
}

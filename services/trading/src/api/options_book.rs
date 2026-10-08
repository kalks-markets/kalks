//! Options order book, terminal API (docs/OPTIONS-EXCHANGE.md §12; Kalks Trader session, `Authorization: Bearer`).
//!
//! * `POST   /v1/terminal/options/book/orders` — limit / market / stop_market / stop_limit.
//! * `PATCH  /v1/terminal/options/book/orders/{id}` `{price?, qty?}` — amend.
//! * `DELETE /v1/terminal/options/book/orders/{id}` — cancel; `DELETE …/book/orders?series=&underlying=` — cancel many.
//! * `GET    /v1/terminal/options/book/orders?status=open|history&series=` — working orders and stops / history.
//! * `GET    /v1/terminal/options/book/fills?from&to` — the account's fills.
//! * `POST   /v1/terminal/options/book/preview` — reserve, estimated average price from the depth, fee.
//! * `POST   /v1/terminal/options/book/deadman {timeoutMs}` — cancel everything without a heartbeat.
//! * `POST   /v1/terminal/options/book/mass-quote` — market-maker programme accounts only.
//! * `POST   /v1/terminal/positions/{ticket}/close` on a book position becomes a reduce-only market IOC
//!   (`book_close`, called by the generic close route).
//!
//! Every route answers 422 `book_disabled` while the order book is not enabled for the account's tenant and kind.

use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::BTreeMap;
use std::time::Duration;

use super::terminal::{self, parse_side, parse_time};
use super::{ApiError, ApiResult, AppState, Body, Ctx, parse_ticket};
use crate::book::entry::{self, SubmitError, Submitted};
use crate::book::types::*;
use crate::book::{BookKey, matching};
use crate::engine::options_book::{self as ob, BookReq, StopReq};
use crate::model::{AccountKind, Side, Source, StopSource, TriggerOp};
use crate::money::{D, ZERO, de_dec, de_opt_dec, num, num_opt, r2};
use crate::shard::Op;

/// New book orders per login and second.
pub const RATE_PER_SEC: u32 = 20;

fn validation(field: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::Validation { field, message: message.into() }
}

fn status(code: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::Status { status: 422, code, message: message.into() }
}

impl From<SubmitError> for ApiError {
    fn from(e: SubmitError) -> Self {
        match e {
            SubmitError::Exec(x) => x.into(),
            SubmitError::Book(m) => ApiError::Status { status: 503, code: "book_unavailable", message: m },
        }
    }
}

/// The session's account kind, and the venue check.
fn venue(st: &AppState, s: &terminal::Session) -> ApiResult<AccountKind> {
    let kind = st.hub.meta(s.login).map(|m| m.kind).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    if !st.hub.shared.books.venue_enabled(s.tenant_id, kind) {
        return Err(status("book_disabled", format!("The options order book is not enabled for {} accounts yet", kind.as_str())));
    }
    Ok(kind)
}

fn settling_gate(st: &AppState, login: i64) -> ApiResult<()> {
    if st.hub.shared.books.is_settling(login, chrono::Utc::now().timestamp_millis()) {
        return Err(status("settling", "Your last fills are still being booked: try again in a moment"));
    }
    Ok(())
}

/* ------------------------------------------------------------------ */
/* Bodies                                                              */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TriggerBody {
    pub source: String,
    pub op: String,
    #[serde(deserialize_with = "de_dec")]
    pub price: D,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderBody {
    pub series: String,
    pub side: String,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(deserialize_with = "de_dec")]
    pub qty: D,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub price: Option<D>,
    #[serde(default)]
    pub tif: Option<String>,
    #[serde(default)]
    pub expire_at: Option<String>,
    #[serde(default)]
    pub post_only: Option<bool>,
    #[serde(default)]
    pub reduce_only: Option<bool>,
    #[serde(default)]
    pub trigger: Option<TriggerBody>,
    #[serde(default)]
    pub client_order_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

/// Parsed body: a book order, or a stop.
pub enum Parsed {
    Order(BookReq),
    Stop(StopReq),
}

pub fn parse(b: &OrderBody) -> ApiResult<Parsed> {
    let side = parse_side(&b.side)?;
    let kind = b.kind.as_deref().unwrap_or("limit").trim().to_ascii_lowercase().replace('-', "_");
    let tif = match b.tif.as_deref() {
        None => None,
        Some(t) => Some(Tif::parse(t).ok_or_else(|| validation("tif", "tif must be gtc, gtd, ioc or fok"))?),
    };
    let expire_at = parse_time(&b.expire_at)?;
    let source = match b.source.as_deref() {
        None | Some("") => Source::Manual,
        Some(s) => Source::parse_client(s).ok_or_else(|| validation("source", "unknown source"))?,
    };
    if b.qty <= ZERO {
        return Err(validation("qty", "qty must be above 0"));
    }
    let mut req = match kind.as_str() {
        "limit" | "stop_limit" => {
            let p = b.price.ok_or_else(|| validation("price", "price is required for a limit order"))?;
            let mut r = BookReq::limit(b.series.trim(), side, b.qty, p);
            r.tif = tif.unwrap_or(Tif::Gtc);
            r
        }
        "market" | "stop_market" => {
            if tif.is_some_and(|t| t != Tif::Ioc) {
                return Err(validation("tif", "a market order is IOC"));
            }
            BookReq::market(b.series.trim(), side, b.qty)
        }
        _ => return Err(validation("type", "type must be limit, market, stop_market or stop_limit")),
    };
    if req.tif == Tif::Gtd {
        req.expire_at = Some(expire_at.ok_or_else(|| validation("expireAt", "expireAt is required for a GTD order"))?);
    }
    req.post_only = b.post_only.unwrap_or(false);
    req.reduce_only = b.reduce_only.unwrap_or(false);
    req.client_order_id = b.client_order_id.as_deref().map(str::trim).filter(|c| !c.is_empty()).map(str::to_string);
    req.source = source;
    if req.post_only && !req.tif.rests() {
        return Err(validation("postOnly", "post-only orders must be GTC or GTD limit orders"));
    }
    if kind.starts_with("stop_") {
        let t = b.trigger.as_ref().ok_or_else(|| validation("trigger", "a stop order needs trigger {source, op, price}"))?;
        let source = match t.source.trim().to_ascii_lowercase().as_str() {
            "mark" => StopSource::Mark,
            "underlying" => StopSource::Underlying,
            _ => return Err(validation("trigger.source", "trigger source must be mark or underlying")),
        };
        let op = match t.op.trim().to_ascii_lowercase().as_str() {
            "above" | ">=" | "gte" => TriggerOp::Above,
            "below" | "<=" | "lte" => TriggerOp::Below,
            _ => return Err(validation("trigger.op", "trigger op must be above or below")),
        };
        req.origin = "stop".into();
        return Ok(Parsed::Stop(StopReq { book: req, source, op, trigger: t.price }));
    }
    if b.trigger.is_some() {
        return Err(validation("trigger", "only stop_market / stop_limit orders take a trigger"));
    }
    Ok(Parsed::Order(req))
}

/* ------------------------------------------------------------------ */
/* Answers                                                             */
/* ------------------------------------------------------------------ */

/// `{status, order, fills, reason?}` for an order the book just processed.
fn answer(sub: &Submitted, cmd_order: Option<&Resting>, spec: Option<&SeriesSpec>, reserved: D) -> Value {
    let id = sub.id;
    let Some(out) = &sub.out else {
        return json!({"status": "duplicate", "duplicate": true, "order": {"id": id}, "fills": []});
    };
    let applied: BTreeMap<(String, String), &Value> = sub.applied.iter().filter_map(|a| Some(((a.value["fillId"].as_str()?.to_string(), a.value["role"].as_str()?.to_string()), &a.value))).collect();
    let mut fills = Vec::new();
    let (mut filled, mut notional) = (0i64, 0i64);
    for f in &out.fills {
        let role = if f.taker.order.as_ref().is_some_and(|o| o.id == id) {
            Role::Taker
        } else if f.maker.order.as_ref().is_some_and(|o| o.id == id) {
            Role::Maker
        } else {
            continue;
        };
        filled += f.qty;
        notional += f.px * f.qty;
        let a = applied.get(&(f.id.clone(), role.as_str().to_string()));
        fills.push(json!({
            "fillId": f.id, "price": num(f.spec.price(f.px)), "qty": num(f.spec.contracts(f.qty)), "role": role.as_str(),
            "fee": a.map(|v| v["fee"].clone()).unwrap_or(Value::Null), "rebate": a.map(|v| v["rebate"].clone()).unwrap_or(Value::Null),
            "positionTicket": a.map(|v| v["positionTicket"].clone()).unwrap_or(Value::Null),
        }));
    }
    let done = out.done.iter().find(|d| d.id == id);
    let rested = out.rested.contains(&id);
    let (st, reason) = match (done, rested) {
        (_, true) if filled > 0 => ("partially_filled", None),
        (_, true) => ("working", None),
        (Some(d), _) => match d.status {
            DoneStatus::Filled => ("filled", None),
            DoneStatus::Rejected => ("rejected", Some(d.reason.clone())),
            _ if d.order.filled > 0 => ("partially_filled", Some(d.reason.clone())),
            _ => ("cancelled", Some(d.reason.clone())),
        },
        (None, false) => ("rejected", out.code.clone()),
    };
    let base = done.map(|d| &d.order).or(cmd_order);
    let order = match (base, spec) {
        (Some(o), Some(spec)) => {
            let qty = o.qty;
            let left = if rested { qty - filled } else { 0 };
            json!({
                "id": id, "series": spec.terms.series, "side": o.side.as_str(), "qty": num(spec.contracts(qty)), "filled": num(spec.contracts(filled)),
                "left": num(spec.contracts(left)), "avgPrice": if filled > 0 { num(spec.tick * D::from(notional) / D::from(filled)) } else { Value::Null },
                "price": num(spec.price(o.px)), "tif": o.tif.as_str(), "flags": flag_names(o.flags), "reserved": num(r2(reserved)),
                "createdAt": chrono::DateTime::from_timestamp_millis(o.ext.created_ms), "type": o.ext.kind,
            })
        }
        _ => json!({"id": id}),
    };
    let mut v = json!({"status": st, "order": order, "fills": fills});
    if let Some(r) = reason {
        v["reason"] = json!(r);
    }
    if out.code.is_some() && !out.ok {
        v["message"] = json!(out.message);
    }
    if !sub.settled {
        v["settling"] = json!(true);
    }
    v
}

async fn reserved_of(st: &AppState, login: i64, id: i64) -> D {
    let v = st.hub.read(login, Box::new(move |x| x.and_then(|(a, _)| a.book.orders.get(&id).map(|w| json!(w.reserved().to_string()))).unwrap_or(Value::Null))).await;
    v.as_str().and_then(|s| s.parse().ok()).unwrap_or(ZERO)
}

fn working_json(w: &crate::book::reserve::Working) -> Value {
    w.json()
}

fn stop_json(o: &crate::model::Order) -> Value {
    let bs = o.option.as_ref().and_then(|x| x.book.as_ref());
    json!({
        "id": o.ticket, "series": o.symbol, "side": o.side.as_str(), "type": if o.kind == crate::model::OrderType::StopLimit { "stop_limit" } else { "stop_market" },
        "qty": num(o.volume), "filled": 0, "left": num(o.volume), "price": num_opt(o.stop_limit), "tif": bs.map(|b| b.tif.clone()),
        "flags": bs.map(|b| { let mut f = vec![]; if b.post_only { f.push("post_only") } if b.reduce_only { f.push("reduce_only") } f }).unwrap_or_default(),
        "trigger": bs.map(|b| json!({"source": b.source.as_str(), "op": b.op.as_str(), "price": num(b.trigger)})),
        "reserved": 0, "expireAt": o.expiry_at, "createdAt": o.placed_at, "clientOrderId": o.client_order_id, "status": "working",
    })
}

/* ------------------------------------------------------------------ */
/* Orders                                                              */
/* ------------------------------------------------------------------ */

/// `POST /v1/terminal/options/book/orders`
pub async fn place(State(st): State<AppState>, ctx: Ctx, Body(b): Body<OrderBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    terminal::copy_guard(&st, &s, !b.reduce_only.unwrap_or(false))?;
    if !b.reduce_only.unwrap_or(false) {
        crate::modules::require(&st, &ctx.tenant.slug, "options").await?;
    }
    venue(&st, &s)?;
    st.limiter.hit(&format!("book:{}", s.login), RATE_PER_SEC, Duration::from_secs(1)).map_err(ApiError::RateLimited)?;
    settling_gate(&st, s.login)?;
    if b.client_order_id.as_deref().is_none_or(|c| c.trim().is_empty()) {
        return Err(validation("clientOrderId", "clientOrderId is required (it makes a repeated submit harmless)"));
    }
    let eligible = super::options::eligible(&st, &s).await;
    let others = st.hub.shared.index.read().unwrap().other_contracts(s.tenant_id, s.user_id, s.login);
    let lp = st.hub.meta(s.login).is_some_and(|m| st.hub.shared.books.is_lp(m.user_id, &m.group));
    let actor = terminal::actor_of(&s);
    match parse(&b)? {
        Parsed::Stop(mut r) => {
            r.book.eligible = eligible;
            r.book.others = others;
            let op: Op = Box::new(move |tx, env| ob::place_stop(tx, env, r).map(|o| json!({"status": "working", "order": stop_json(&o), "fills": []})));
            let done = st.hub.exec(s.login, &actor, None, "", "", None, op).await?;
            Ok(Json(terminal::with_notes(done.value, &done.notes)))
        }
        Parsed::Order(mut r) => {
            r.eligible = eligible;
            r.others = others;
            r.lp = lp;
            submit_answer(&st, s.login, &actor, r).await.map(Json)
        }
    }
}

/// Submits and builds the `{status, order, fills}` answer.
pub async fn submit_answer(st: &AppState, login: i64, actor: &str, r: BookReq) -> ApiResult<Value> {
    let cid = r.client_order_id.clone();
    let sub = entry::submit(&st.hub, login, actor, r).await?;
    if sub.out.is_none() {
        // the same clientOrderId again: the order as it is now
        let id = sub.id;
        let w = st.hub.read(login, Box::new(move |x| x.and_then(|(a, _)| a.book.orders.get(&id).map(working_json)).unwrap_or(Value::Null))).await;
        let order = if w.is_null() { history_row(st, login, id).await?.unwrap_or(json!({"id": id})) } else { w };
        return Ok(json!({"status": order["status"].as_str().unwrap_or("working"), "duplicate": true, "clientOrderId": cid, "order": order, "fills": []}));
    }
    let reserved = reserved_of(st, login, sub.id).await;
    Ok(answer(&sub, sub.order.as_ref(), sub.spec.as_ref(), reserved))
}

async fn history_row(st: &AppState, login: i64, id: i64) -> ApiResult<Option<Value>> {
    let r = sqlx::query("SELECT data, series, price, status, reason, created_at, done_at, (data->'ext'->>'kind') AS kind FROM book_orders WHERE id = $1 AND login = $2").bind(id).bind(login).fetch_optional(&st.pool).await?;
    Ok(r.map(|r| history_json(&r)))
}

fn history_json(r: &sqlx::postgres::PgRow) -> Value {
    let o: sqlx::types::Json<Resting> = r.get("data");
    let o = o.0;
    let price: D = r.get("price");
    let tick = if o.px > 0 { price / D::from(o.px) } else { ZERO };
    let step_of = |q: i64| o.ext.contracts(q);
    json!({
        "id": o.id, "series": r.get::<String, _>("series"), "side": o.side.as_str(), "type": r.get::<Option<String>, _>("kind"),
        "qtySteps": o.qty, "filledSteps": o.filled, "qty": num(step_of(o.qty)), "filled": num(step_of(o.filled)), "left": num(step_of(o.left)),
        "avgPrice": if o.filled > 0 { num(tick * D::from(o.notional) / D::from(o.filled)) } else { Value::Null },
        "price": num(price), "tif": o.tif.as_str(), "flags": flag_names(o.flags), "status": r.get::<String, _>("status"), "reason": r.get::<Option<String>, _>("reason"),
        "createdAt": r.get::<chrono::DateTime<chrono::Utc>, _>("created_at"), "doneAt": r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("done_at"), "clientOrderId": o.ext.client_order_id,
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AmendBody {
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub price: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub qty: Option<D>,
}

/// `PATCH /v1/terminal/options/book/orders/{id}`
pub async fn amend(State(st): State<AppState>, ctx: Ctx, Path(id): Path<String>, Body(b): Body<AmendBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    venue(&st, &s)?;
    st.limiter.hit(&format!("book:{}", s.login), RATE_PER_SEC, Duration::from_secs(1)).map_err(ApiError::RateLimited)?;
    let id = parse_ticket(&id)?;
    if b.price.is_none() && b.qty.is_none() {
        return Err(validation("price", "give a new price and / or qty"));
    }
    let token = rand_token();
    let (price, qty) = (b.price, b.qty);
    let (key, cmd) = entry::exec_typed(&st.hub, s.login, &terminal::actor_of(&s), move |tx, env| ob::prepare_amend(tx, env, id, price, qty, token)).await?;
    let (out, applied, settled) = entry::call(&st.hub, s.login, &key, cmd).await?;
    if !out.ok {
        // the hold is released by the actor's Release item; answer with the book's reason
        return Err(ApiError::Reject { reject: crate::engine::Reject::new(static_code(out.code.as_deref()), out.message.clone().unwrap_or_default()), audit: vec![] });
    }
    let sub = Submitted { key: Some(key), id, out: Some(out.clone()), applied, settled, order: None, spec: None };
    let w = st.hub.read(s.login, Box::new(move |x| x.and_then(|(a, _)| a.book.orders.get(&id).map(working_json)).unwrap_or(Value::Null))).await;
    let mut v = answer(&sub, None, out.fills.first().map(|f| &f.spec), ZERO);
    if !w.is_null() {
        v["order"] = w;
    } else if let Some(h) = history_row(&st, s.login, id).await? {
        v["order"] = h;
    }
    v["status"] = json!(match v["order"]["status"].as_str() {
        Some("working") => "working",
        Some("partially_filled") => "partially_filled",
        Some("filled") => "filled",
        Some(x) => x,
        None => "working",
    });
    Ok(Json(v))
}

fn static_code(c: Option<&str>) -> &'static str {
    match c.unwrap_or("rejected") {
        "not_found" => "not_found",
        "invalid_price" => "invalid_price",
        "invalid_qty" => "invalid_volume",
        "series_cancel_only" => "series_cancel_only",
        "would_take" => "would_take",
        "no_change" => "no_change",
        "series_closed" => "series_closed",
        _ => "rejected",
    }
}

fn rand_token() -> u64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    u64::from_le_bytes(b) | 1
}

/// The book key of a working order of this account.
async fn working_key(st: &AppState, login: i64, kind: AccountKind, tenant: i64, id: i64) -> ApiResult<(BookKey, String)> {
    let v = st.hub.read(login, Box::new(move |x| x.and_then(|(a, _)| a.book.orders.get(&id).map(|w| json!([w.underlying, w.series]))).unwrap_or(Value::Null))).await;
    match (v[0].as_str(), v[1].as_str()) {
        (Some(u), Some(s)) => Ok((BookKey::new(tenant, kind, u), s.to_string())),
        _ => Err(ApiError::NotFound(format!("Order #{id} is not working"))),
    }
}

/// `DELETE /v1/terminal/options/book/orders/{id}` (also cancels a book stop that has not fired).
pub async fn cancel(State(st): State<AppState>, ctx: Ctx, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    let kind = venue(&st, &s)?;
    let id = parse_ticket(&id)?;
    // a stop is an account order
    let is_stop = st.hub.read(s.login, Box::new(move |x| json!(x.is_some_and(|(a, _)| a.orders.get(&id).is_some_and(|o| o.option.as_ref().is_some_and(|oo| oo.book.is_some())))))).await;
    if is_stop.as_bool() == Some(true) {
        let op: Op = Box::new(move |tx, env| crate::engine::trade::cancel_order(tx, env, id, "cancelled by client").map(|o| json!({"status": "cancelled", "order": stop_json(&o)})));
        let done = st.hub.exec(s.login, &terminal::actor_of(&s), None, "", "", None, op).await?;
        return Ok(Json(done.value));
    }
    let (key, series) = working_key(&st, s.login, kind, s.tenant_id, id).await?;
    let at = st.hub.shared.clock.now().timestamp_millis();
    let (out, applied, settled) = entry::call(&st.hub, s.login, &key, Cmd::Cancel { series, id, login: s.login, reason: "cancelled".into(), at }).await?;
    if !out.ok {
        return Err(ApiError::NotFound(out.message.unwrap_or_else(|| format!("Order #{id} is not working"))));
    }
    let _ = applied;
    let o = out.done.iter().find(|d| d.id == id).map(|d| d.order.clone());
    let v = history_row(&st, s.login, id).await?.unwrap_or_else(|| json!({"id": id, "filled": o.as_ref().map(|o| o.filled)}));
    Ok(Json(json!({"status": "cancelled", "order": v, "settling": !settled})))
}

#[derive(Deserialize, Default)]
pub struct CancelManyQ {
    pub series: Option<String>,
    pub underlying: Option<String>,
}

/// `DELETE /v1/terminal/options/book/orders?series=&underlying=` — every working order (and book stop) matching.
pub async fn cancel_many(State(st): State<AppState>, ctx: Ctx, Query(q): Query<CancelManyQ>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    let kind = venue(&st, &s)?;
    let (fs, fu) = (q.series.clone(), q.underlying.clone().map(|u| u.to_ascii_uppercase()));
    let (fs2, fu2) = (fs.clone(), fu.clone());
    let v = st
        .hub
        .read(
            s.login,
            Box::new(move |x| {
                let Some((a, _)) = x else { return Value::Null };
                let mut by_u: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for w in a.book.orders.values().filter(|w| w.flags & EPHEMERAL == 0) {
                    if fs2.as_ref().is_some_and(|s| *s != w.series) || fu2.as_ref().is_some_and(|u| *u != w.underlying) {
                        continue;
                    }
                    let e = by_u.entry(w.underlying.clone()).or_default();
                    if !e.contains(&w.series) {
                        e.push(w.series.clone());
                    }
                }
                let stops: Vec<i64> = a
                    .orders
                    .values()
                    .filter(|o| o.option.as_ref().is_some_and(|oo| oo.book.is_some() && oo.legs.first().is_some_and(|l| fs2.as_ref().is_none_or(|s| *s == l.terms.series) && fu2.as_ref().is_none_or(|u| *u == l.terms.underlying))))
                    .map(|o| o.ticket)
                    .collect();
                json!({"books": by_u, "stops": stops})
            }),
        )
        .await;
    let mut cancelled = Vec::new();
    let at = st.hub.shared.clock.now().timestamp_millis();
    if let Some(books) = v["books"].as_object() {
        for (u, series) in books {
            let key = BookKey::new(s.tenant_id, kind, u);
            let list: Vec<Option<String>> = if fs.is_some() { series.as_array().unwrap().iter().map(|x| x.as_str().map(str::to_string)).collect() } else { vec![None] };
            for sname in list {
                let (out, _, _) = entry::call(&st.hub, s.login, &key, Cmd::CancelAll { login: s.login, series: sname, expiry: None, ephemeral_only: false, reason: "cancelled".into(), at }).await?;
                cancelled.extend(out.done.iter().filter(|d| d.order.flags & EPHEMERAL == 0).map(|d| d.id));
            }
        }
    }
    for t in v["stops"].as_array().cloned().unwrap_or_default().iter().filter_map(Value::as_i64) {
        let op: Op = Box::new(move |tx, env| crate::engine::trade::cancel_order(tx, env, t, "cancelled by client").map(|_| Value::Null));
        if st.hub.exec(s.login, &terminal::actor_of(&s), None, "", "", None, op).await.is_ok() {
            cancelled.push(t);
        }
    }
    Ok(Json(json!({"status": "cancelled", "cancelled": cancelled})))
}

#[derive(Deserialize, Default)]
pub struct OrdersQ {
    pub status: Option<String>,
    pub series: Option<String>,
    pub limit: Option<i64>,
}

/// `GET /v1/terminal/options/book/orders?status=open|history&series=`
pub async fn orders(State(st): State<AppState>, ctx: Ctx, Query(q): Query<OrdersQ>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let series = q.series.clone();
    if q.status.as_deref().unwrap_or("open") == "history" {
        let rows = sqlx::query(
            "SELECT data, series, price, status, reason, created_at, done_at, (data->'ext'->>'kind') AS kind FROM book_orders
             WHERE login = $1 AND status <> 'open' AND ($2::text IS NULL OR series = $2) ORDER BY created_at DESC, id DESC LIMIT $3",
        )
        .bind(s.login)
        .bind(&series)
        .bind(q.limit.unwrap_or(200).clamp(1, 1000))
        .fetch_all(&st.pool)
        .await?;
        return Ok(Json(json!({"orders": rows.iter().map(history_json).collect::<Vec<_>>()})));
    }
    let v = st
        .hub
        .read(
            s.login,
            Box::new(move |x| {
                let Some((a, _)) = x else { return Value::Null };
                let mut out: Vec<Value> = a.book.orders.values().filter(|w| w.flags & EPHEMERAL == 0 && series.as_ref().is_none_or(|s| *s == w.series)).map(working_json).collect();
                out.extend(a.orders.values().filter(|o| o.option.as_ref().is_some_and(|oo| oo.book.is_some()) && series.as_ref().is_none_or(|s| *s == o.symbol)).map(stop_json));
                json!({"orders": out, "reserved": num(r2(a.book.reserve()))})
            }),
        )
        .await;
    if v.is_null() {
        return Err(ApiError::NotFound("Account not found".into()));
    }
    Ok(Json(v))
}

#[derive(Deserialize, Default)]
pub struct FillsQ {
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
}

/// `GET /v1/terminal/options/book/fills?from&to`
pub async fn fills(State(st): State<AppState>, ctx: Ctx, Query(q): Query<FillsQ>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let rows = sqlx::query(
        "SELECT fill_id, series, price, contracts, maker_login, taker_login, maker_order, taker_order, data, at, fill_kind, premium_usd FROM book_fills
         WHERE (maker_login = $1 OR taker_login = $1) AND ($2::timestamptz IS NULL OR at >= $2) AND ($3::timestamptz IS NULL OR at < $3)
         ORDER BY at DESC, seq DESC LIMIT $4",
    )
    .bind(s.login)
    .bind(parse_time(&q.from)?)
    .bind(parse_time(&q.to)?)
    .bind(q.limit.unwrap_or(200).clamp(1, 1000))
    .fetch_all(&st.pool)
    .await?;
    let ids: Vec<String> = rows.iter().map(|r| r.get::<String, _>("fill_id")).collect();
    // fees and the position each fill went to, from this account's deals
    let deals = sqlx::query("SELECT data FROM deals WHERE login = $1 AND option->'fill'->>'id' = ANY($2)").bind(s.login).bind(&ids).fetch_all(&st.pool).await?;
    let mut per: BTreeMap<String, (D, D, Option<i64>)> = BTreeMap::new();
    for d in deals {
        let d: sqlx::types::Json<crate::model::Deal> = d.get("data");
        let Some(o) = d.0.option.as_ref() else { continue };
        let Some(f) = o.fill.as_ref() else { continue };
        let e = per.entry(f.id.clone()).or_default();
        e.0 += o.charged;
        e.1 += o.rebate;
        e.2 = Some(d.0.position_ticket);
    }
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id: String = r.get("fill_id");
            let maker: i64 = r.get("maker_login");
            let f: sqlx::types::Json<Fill> = r.get("data");
            let role = if maker == s.login { Role::Maker } else { Role::Taker };
            let side = f.0.party(role).side;
            let x = per.get(&id).cloned().unwrap_or_default();
            json!({
                "fillId": id, "series": r.get::<String, _>("series"), "side": side.as_str(), "role": role.as_str(), "price": num(r.get::<D, _>("price")),
                "qty": num(r.get::<D, _>("contracts")), "premiumUsd": num(r.get::<D, _>("premium_usd")), "fee": num(x.0), "rebate": num(x.1), "positionTicket": x.2,
                "orderId": if role == Role::Maker { r.get::<Option<i64>, _>("maker_order") } else { r.get::<Option<i64>, _>("taker_order") },
                "kind": r.get::<String, _>("fill_kind"), "at": r.get::<chrono::DateTime<chrono::Utc>, _>("at"),
            })
        })
        .collect();
    Ok(Json(json!({"fills": items})))
}

/// `POST /v1/terminal/options/book/preview` — the order body; never changes anything.
pub async fn preview(State(st): State<AppState>, ctx: Ctx, Body(b): Body<OrderBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let kind = venue(&st, &s)?;
    let mut r = match parse(&b)? {
        Parsed::Order(r) => r,
        Parsed::Stop(s) => s.book,
    };
    r.eligible = super::options::eligible(&st, &s).await;
    r.others = st.hub.shared.index.read().unwrap().other_contracts(s.tenant_id, s.user_id, s.login);
    r.lp = st.hub.meta(s.login).is_some_and(|m| st.hub.shared.books.is_lp(m.user_id, &m.group));
    r.client_order_id = None;
    let v = st
        .hub
        .read(
            s.login,
            Box::new(move |x| {
                let Some((a, env)) = x else { return Value::Null };
                let ids = crate::engine::Ids::new(i64::MAX / 2, i64::MAX / 2, i64::MAX / 2);
                let scratch = crate::engine::Env { ids: &ids, ..*env };
                let mut tx = crate::engine::Tx::new(a);
                let before = a.book.reserve();
                match ob::enter(&mut tx, &scratch, r) {
                    Ok(ob::Entered::New { cmd: Cmd::New { spec, order, .. }, .. }) => {
                        let after = tx.st.book.reserve();
                        let m = crate::engine::metrics(env, &tx.st);
                        json!({
                            "ok": true, "reasons": [], "reserve": num(r2(after - before)), "orderReserveAfter": num(r2(after)),
                            "freeMarginAfter": num(r2(m.free_margin)), "price": num(spec.price(order.px)), "px": order.px, "steps": order.qty,
                            "tick": num(spec.tick), "step": num(spec.step), "side": order.side.as_str(), "stp": order.stp, "series": spec.terms.series,
                            "underlying": spec.terms.underlying, "feeTaker": num(order.ext.fee_taker), "feeMaker": num(order.ext.fee_maker), "feeCapPct": num(order.ext.fee_cap_pct),
                            "contractSize": num(spec.terms.contract_size), "currency": a.account.ccy(), "mark": crate::engine::options::mark_of(env, &a.account, &spec.terms).map(|q| num(q.mark)),
                        })
                    }
                    Ok(_) => json!({"ok": false, "reasons": [{"code": "duplicate", "message": "duplicate"}]}),
                    Err(e) => json!({"ok": false, "reasons": [{"code": e.code, "message": e.message}]}),
                }
            }),
        )
        .await;
    if v.is_null() {
        return Err(ApiError::NotFound("Account not found".into()));
    }
    let mut v = v;
    if v["ok"] == true {
        // estimated fill from the book's depth (own orders stop it: self-trade prevention)
        let key = BookKey::new(s.tenant_id, kind, v["underlying"].as_str().unwrap_or_default());
        let (series, side, steps, px, stp) = (v["series"].as_str().unwrap_or_default().to_string(), if v["side"] == "buy" { Side::Buy } else { Side::Sell }, v["steps"].as_i64().unwrap_or(0), v["px"].as_i64().unwrap_or(0), v["stp"].as_i64().unwrap_or(0));
        let est = match st.hub.shared.books.handle(&key) {
            Some(h) => h.read(Box::new(move |b| match b.book(&series) {
                Some(sb) => {
                    let (q, n) = matching::estimate(sb, side, steps, px, stp);
                    json!([q, n])
                }
                None => json!([0, 0]),
            }))
            .await
            .unwrap_or(json!([0, 0])),
            None => json!([0, 0]),
        };
        let (q, n) = (est[0].as_i64().unwrap_or(0), est[1].as_i64().unwrap_or(0));
        let step: D = v["step"].as_f64().and_then(crate::money::from_f64).unwrap_or(D::ONE);
        let tick: D = v["tick"].as_f64().and_then(crate::money::from_f64).unwrap_or(ZERO);
        let size: D = v["contractSize"].as_f64().and_then(crate::money::from_f64).unwrap_or(D::ONE);
        let contracts = D::from(q) * step;
        v["estFilled"] = num(contracts);
        v["estAvgPrice"] = if q > 0 { num(tick * D::from(n) / D::from(q)) } else { Value::Null };
        let rate: D = v["feeTaker"].as_f64().and_then(crate::money::from_f64).unwrap_or(ZERO);
        let cap: D = v["feeCapPct"].as_f64().and_then(crate::money::from_f64).unwrap_or(ZERO);
        let prem = tick * D::from(n) * step * size;
        v["fee"] = num(r2((rate.abs() * contracts).min(cap / crate::money::HUNDRED * prem)));
        for k in ["px", "steps", "stp", "feeCapPct"] {
            if let Value::Object(o) = &mut v {
                o.remove(k);
            }
        }
    }
    Ok(Json(v))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeadmanBody {
    pub timeout_ms: i64,
}

/// `POST /v1/terminal/options/book/deadman {timeoutMs}` — call again before the timeout (heartbeat); 0 switches it off.
pub async fn deadman(State(st): State<AppState>, ctx: Ctx, Body(b): Body<DeadmanBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    let kind = venue(&st, &s)?;
    if b.timeout_ms != 0 && !(1000..=600_000).contains(&b.timeout_ms) {
        return Err(validation("timeoutMs", "timeoutMs must be 0 (off) or between 1000 and 600000"));
    }
    let now = chrono::Utc::now().timestamp_millis();
    st.hub.shared.books.heartbeat(s.login, s.tenant_id, kind, b.timeout_ms, now);
    Ok(Json(json!({"timeoutMs": b.timeout_ms, "expiresAt": (b.timeout_ms > 0).then(|| chrono::DateTime::from_timestamp_millis(now + b.timeout_ms))})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuoteSideBody {
    #[serde(deserialize_with = "de_dec")]
    pub price: D,
    #[serde(deserialize_with = "de_dec")]
    pub qty: D,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct QuoteBody {
    pub series: String,
    #[serde(default)]
    pub bid: Option<QuoteSideBody>,
    #[serde(default)]
    pub ask: Option<QuoteSideBody>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MassQuoteBody {
    pub quotes: Vec<QuoteBody>,
}

/// `POST /v1/terminal/options/book/mass-quote` — market-maker programme accounts only. Replaces the account's
/// quotes in every listed series (a series without bid / ask pulls that side). Quotes are post-only and ephemeral.
pub async fn mass_quote(State(st): State<AppState>, ctx: Ctx, Body(b): Body<MassQuoteBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    venue(&st, &s)?;
    let meta = st.hub.meta(s.login).ok_or_else(|| ApiError::NotFound("Account not found".into()))?;
    if !st.hub.shared.books.is_lp(meta.user_id, &meta.group) {
        return Err(ApiError::Forbidden("Mass quotes are for market-maker programme accounts".into()));
    }
    st.limiter.hit(&format!("bookmq:{}", s.login), 10, Duration::from_secs(1)).map_err(ApiError::RateLimited)?;
    if b.quotes.is_empty() || b.quotes.len() > 2000 {
        return Err(validation("quotes", "1 to 2000 series per mass quote"));
    }
    let res = crate::book::entry::mass_quote(&st.hub, s.login, &terminal::actor_of(&s), b.quotes.iter().map(|q| (q.series.clone(), q.bid.as_ref().map(|x| (x.price, x.qty)), q.ask.as_ref().map(|x| (x.price, x.qty)))).collect()).await?;
    Ok(Json(res))
}

/// `POST /v1/terminal/positions/{ticket}/close` on a book position: a reduce-only market IOC.
/// Answers `{status: filled|partial, filled, avgPrice, left}`.
pub async fn book_close(st: &AppState, s: &terminal::Session, ticket: i64, volume: Option<D>) -> ApiResult<Value> {
    venue(st, s)?;
    settling_gate(st, s.login)?;
    let req = entry::exec_typed(&st.hub, s.login, "client", move |tx, _| ob::close_request(&tx.st, ticket, volume)).await?;
    let want = req.qty;
    let sub = entry::submit(&st.hub, s.login, &terminal::actor_of(s), req).await?;
    let out = sub.out.clone().unwrap_or_default();
    let (mut filled, mut notional, mut spec) = (0i64, 0i64, None);
    for f in &out.fills {
        if f.taker.order.as_ref().is_some_and(|o| o.id == sub.id) {
            filled += f.qty;
            notional += f.px * f.qty;
            spec = Some(f.spec.clone());
        }
    }
    let Some(spec) = spec else {
        let why = out.done.iter().find(|d| d.id == sub.id).map(|d| d.reason.clone()).unwrap_or_else(|| "no_liquidity".into());
        return Err(status("no_liquidity", format!("Nothing could be closed on the order book right now ({why})")));
    };
    let c = spec.contracts(filled);
    let left = want - c;
    Ok(json!({
        "status": if left <= ZERO { "filled" } else { "partial" }, "filled": num(c), "avgPrice": num(spec.tick * D::from(notional) / D::from(filled)),
        "left": num(left.max(ZERO)), "orderId": sub.id,
        "fills": out.fills.iter().filter(|f| f.taker.order.as_ref().is_some_and(|o| o.id == sub.id)).map(|f| json!({"fillId": f.id, "price": num(f.spec.price(f.px)), "qty": num(f.spec.contracts(f.qty))})).collect::<Vec<_>>(),
    }))
}

/* ------------------------------------------------------------------ */
/* Combo RFQ (docs §5)                                                 */
/* ------------------------------------------------------------------ */

impl From<crate::book::rfq::RfqError> for ApiError {
    fn from(e: crate::book::rfq::RfqError) -> Self {
        match e {
            crate::book::rfq::RfqError::Exec(x) => x.into(),
            crate::book::rfq::RfqError::Code("validation", m) => ApiError::Validation { field: "legs", message: m },
            crate::book::rfq::RfqError::Code(code, m) => ApiError::Status { status: 422, code, message: m },
            crate::book::rfq::RfqError::NotFound => ApiError::NotFound("Request not found".into()),
        }
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RfqLegBody {
    pub series: String,
    pub side: String,
    pub ratio: i64,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RfqBody {
    pub legs: Vec<RfqLegBody>,
    #[serde(deserialize_with = "de_dec")]
    pub qty: D,
    #[serde(default)]
    pub reduce_only: Option<bool>,
}

fn rfq_id(id: &str) -> ApiResult<i64> {
    id.trim().parse::<i64>().map_err(|_| ApiError::NotFound("Request not found".into()))
}

/// `POST /v1/terminal/options/rfq {legs[{series, side, ratio}], qty, reduceOnly?}` → `{rfq: {id, expiresAt, legs, qty,
/// status}, quotes: [...]}`. The Kalks market maker answers at once.
pub async fn rfq_open(State(st): State<AppState>, ctx: Ctx, Body(b): Body<RfqBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    let reduce = b.reduce_only.unwrap_or(false);
    terminal::copy_guard(&st, &s, !reduce)?;
    if !reduce {
        crate::modules::require(&st, &ctx.tenant.slug, "options").await?;
    }
    let kind = venue(&st, &s)?;
    st.limiter.hit(&format!("book:{}", s.login), RATE_PER_SEC, Duration::from_secs(1)).map_err(ApiError::RateLimited)?;
    if b.qty <= ZERO {
        return Err(validation("qty", "qty must be above 0"));
    }
    let mut legs = Vec::new();
    for l in &b.legs {
        legs.push((l.series.trim().to_string(), parse_side(&l.side)?, l.ratio));
    }
    let r = crate::book::rfq::open(&st.hub, &st.pool, s.tenant_id, kind, s.login, s.user_id, legs, b.qty, reduce).await?;
    let now_ms = st.hub.shared.clock.now().timestamp_millis();
    Ok(Json(json!({"rfq": r.json(), "quotes": r.quotes_json(now_ms), "note": r.note})))
}

/// `GET /v1/terminal/options/rfq/{id}` → `{rfq, quotes: [{quoteId, responder, bid, ask, qty, validUntil}]}` (a lapsed
/// quote is replaced by a new firm one while the request is open).
pub async fn rfq_get(State(st): State<AppState>, ctx: Ctx, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let id = rfq_id(&id)?;
    let r = st.hub.shared.books.rfqs.get(id).filter(|r| r.login == s.login).ok_or_else(|| ApiError::NotFound("Request not found".into()))?;
    let r = if r.status == "open" { crate::book::rfq::refresh(&st.hub, r.id).await.unwrap_or(r) } else { r };
    let now_ms = st.hub.shared.clock.now().timestamp_millis();
    Ok(Json(json!({"rfq": r.json(), "quotes": r.quotes_json(now_ms), "note": r.note})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RfqAcceptBody {
    pub quote_id: String,
    pub side: String,
    #[serde(deserialize_with = "de_dec")]
    pub limit_net: D,
}

/// `POST /v1/terminal/options/rfq/{id}/accept {quoteId, side, limitNet}` → `{status: "filled", comboId, fills, net}`:
/// every leg fills at once or nothing does (422 with the reason: `quote_expired`, `price_moved`, …).
pub async fn rfq_accept(State(st): State<AppState>, ctx: Ctx, Path(id): Path<String>, Body(b): Body<RfqAcceptBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    venue(&st, &s)?;
    st.limiter.hit(&format!("book:{}", s.login), RATE_PER_SEC, Duration::from_secs(1)).map_err(ApiError::RateLimited)?;
    settling_gate(&st, s.login)?;
    let id = rfq_id(&id)?;
    let quote = b.quote_id.trim().parse::<i64>().map_err(|_| status("quote_expired", "The quote has expired. Accept the new one."))?;
    let side = parse_side(&b.side)?;
    let r = st.hub.shared.books.rfqs.get(id).filter(|r| r.login == s.login).ok_or_else(|| ApiError::NotFound("Request not found".into()))?;
    terminal::copy_guard(&st, &s, !r.reduce_only)?;
    let mut base = BookReq::limit("", side, ZERO, ZERO);
    base.eligible = super::options::eligible(&st, &s).await;
    base.others = st.hub.shared.index.read().unwrap().other_contracts(s.tenant_id, s.user_id, s.login);
    base.lp = st.hub.meta(s.login).is_some_and(|m| st.hub.shared.books.is_lp(m.user_id, &m.group));
    let a = crate::book::rfq::accept(&st.hub, id, s.login, &terminal::actor_of(&s), quote, side, b.limit_net, base).await?;
    let tick = a.rfq.tick();
    if !a.out.ok {
        let code: &'static str = match a.out.code.as_deref() {
            Some("quote_expired") => "quote_expired",
            Some("price_moved") => "price_moved",
            Some("reduce_only") => "reduce_only",
            Some("self_trade") => "self_trade",
            Some("series_cancel_only") => "series_cancel_only",
            Some("series_closed") => "series_closed",
            Some("no_price") => "no_price",
            _ => "rejected",
        };
        crate::book::rfq::record(&st.pool, &a.rfq, json!({"rejected": a.out.code})).await;
        return Err(ApiError::Status { status: 422, code, message: a.out.message.clone().unwrap_or_else(|| "The combo was not filled".into()) });
    }
    // this account's legs as booked (one outbox item for all of them)
    let booked: BTreeMap<String, Value> = a.applied.iter().flat_map(|x| x.value["legs"].as_array().cloned().unwrap_or_default()).filter_map(|l| Some((l["fillId"].as_str()?.to_string(), l))).collect();
    let fills: Vec<Value> = a
        .out
        .fills
        .iter()
        .map(|f| {
            let b = booked.get(&f.id);
            json!({
                "fillId": f.id, "series": f.series, "side": f.taker.side.as_str(), "role": "taker", "price": num(f.spec.price(f.px)), "qty": num(f.spec.contracts(f.qty)),
                "fee": b.map(|v| v["fee"].clone()).unwrap_or(json!(0)), "rebate": b.map(|v| v["rebate"].clone()).unwrap_or(json!(0)),
                "positionTicket": b.map(|v| v["positionTicket"].clone()).unwrap_or(Value::Null), "kind": "rfq", "comboId": f.combo.map(|c| c.to_string()), "at": chrono::DateTime::from_timestamp_millis(f.at),
            })
        })
        .collect();
    crate::book::rfq::record(&st.pool, &a.rfq, json!({"filled": {"quote": quote, "side": side.as_str(), "net": num(D::from(a.net) * tick), "fills": fills.len()}})).await;
    let mut v = json!({"status": "filled", "comboId": a.rfq.id.to_string(), "net": num(D::from(a.net) * tick), "fills": fills});
    if !a.settled {
        v["settling"] = json!(true);
    }
    Ok(Json(v))
}

/// `DELETE /v1/terminal/options/rfq/{id}` → `{status: "cancelled"}`.
pub async fn rfq_cancel(State(st): State<AppState>, ctx: Ctx, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let id = rfq_id(&id)?;
    let r = crate::book::rfq::cancel(&st.hub, id, s.login)?;
    crate::book::rfq::record(&st.pool, &r, json!({})).await;
    Ok(Json(json!({"status": r.status, "rfq": r.json()})))
}

/// `POST /v1/terminal/options/combos/{comboId}/close` on a strategy held on the order book (docs §5): one
/// reduce-only combo RFQ to the market maker, accepted at its firm quote — every leg closes at once or none does.
/// None when the strategy is not on the book (the house close handles it).
pub async fn book_close_combo(st: &AppState, s: &terminal::Session, combo: i64) -> ApiResult<Option<Value>> {
    let v = st
        .hub
        .read(
            s.login,
            Box::new(move |x| {
                let Some((a, _)) = x else { return Value::Null };
                json!(a.positions.values().filter(|p| p.combo_id == Some(combo) && p.option.is_some()).map(|p| json!({"ticket": p.ticket, "series": p.symbol, "side": p.side.as_str(), "contracts": p.volume.to_string(), "book": p.on_book()})).collect::<Vec<_>>())
            }),
        )
        .await;
    let legs = v.as_array().cloned().unwrap_or_default();
    if legs.is_empty() || !legs.iter().any(|l| l["book"] == true) {
        return Ok(None);
    }
    if legs.iter().any(|l| l["book"] != true) {
        return Err(status("mixed_venue", "This strategy has legs on the order book and Kalks-quoted legs: close them one by one"));
    }
    let kind = venue(st, s)?;
    settling_gate(st, s.login)?;
    // per series: the contracts to close (a strategy holds one position per series and side)
    let mut per: BTreeMap<String, D> = BTreeMap::new();
    for l in &legs {
        let c: D = l["contracts"].as_str().and_then(|x| x.parse().ok()).unwrap_or(ZERO);
        let sign = if l["side"] == "buy" { D::ONE } else { -D::ONE };
        *per.entry(l["series"].as_str().unwrap_or_default().to_string()).or_default() += c * sign;
    }
    per.retain(|_, c| !c.is_zero());
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a.abs() } else { gcd(b, a % b) }
    }
    let whole: Vec<(String, Side, i64)> = per.iter().map(|(s, c)| (s.clone(), if *c > ZERO { Side::Sell } else { Side::Buy }, rust_decimal::prelude::ToPrimitive::to_i64(&c.abs().trunc()).unwrap_or(0))).collect();
    if whole.iter().any(|w| w.2 <= 0) || per.values().any(|c| !c.fract().is_zero()) {
        return Err(status("invalid_volume", "Only whole contracts close as a strategy: close the legs one by one"));
    }
    let g = whole.iter().fold(0, |a, w| gcd(a, w.2)).max(1);
    let legs_in: Vec<(String, Side, i64)> = whole.iter().map(|(s, side, n)| (s.clone(), *side, n / g)).collect();
    let r = crate::book::rfq::open(&st.hub, &st.pool, s.tenant_id, kind, s.login, s.user_id, legs_in, D::from(g), true).await?;
    let q = r.quote.clone().ok_or_else(|| status("no_liquidity", r.note.clone().unwrap_or_else(|| "The market maker cannot price this strategy right now".into())))?;
    let ask = q.ask.ok_or_else(|| status("no_liquidity", "The market maker has no price to close this strategy"))?;
    let mut base = BookReq::limit("", Side::Buy, ZERO, ZERO);
    base.reduce_only = true;
    base.origin = format!("close-combo:{combo}");
    let a = crate::book::rfq::accept(&st.hub, r.id, s.login, &terminal::actor_of(s), q.id, Side::Buy, D::from(ask) * r.tick(), base).await?;
    if !a.out.ok {
        let code: &'static str = match a.out.code.as_deref() {
            Some("quote_expired") => "quote_expired",
            Some("price_moved") => "price_moved",
            Some("reduce_only") => "reduce_only",
            _ => "rejected",
        };
        return Err(ApiError::Status { status: 422, code, message: a.out.message.clone().unwrap_or_else(|| "The strategy was not closed".into()) });
    }
    crate::book::rfq::record(&st.pool, &a.rfq, json!({"filled": {"quote": q.id, "side": "buy", "net": num(D::from(a.net) * r.tick()), "fills": a.out.fills.len(), "closeCombo": combo}})).await;
    // the deals each leg booked (profit of the closed share)
    let mut profit = ZERO;
    let mut out_legs = Vec::new();
    for x in &a.applied {
        for l in x.value["legs"].as_array().cloned().unwrap_or_default() {
            for d in l["deals"].as_array().cloned().unwrap_or_default() {
                if d["entry"] == "out" {
                    profit += d["profit"].as_f64().and_then(crate::money::from_f64).unwrap_or(ZERO);
                    out_legs.push(json!({"ticket": d["ticket"], "dealId": d["dealId"], "profit": d["profit"], "fillId": l["fillId"], "series": l["series"], "price": l["price"], "qty": l["qty"]}));
                }
            }
        }
    }
    Ok(Some(json!({"status": "closed", "comboId": combo, "legs": out_legs, "profit": num(r2(profit)), "net": num(D::from(a.net) * r.tick()), "rfq": r.id.to_string(), "venue": "book", "settling": !a.settled})))
}

/// Is `ticket` an order-book position of the session's account?
pub async fn is_book_position(st: &AppState, login: i64, ticket: i64) -> bool {
    st.hub.read(login, Box::new(move |x| json!(x.is_some_and(|(a, _)| a.positions.get(&ticket).is_some_and(|p| p.on_book()))))).await.as_bool() == Some(true)
}


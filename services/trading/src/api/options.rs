//! Kalks FX Options HTTP API (README "Kalks FX Options").
//!
//! Terminal (Kalks Trader session, `Authorization: Bearer`):
//! * `POST /v1/terminal/options/preview` — prices, money impact, payoff, Greeks; never changes the account.
//! * `POST /v1/terminal/options/orders` — market, limit (premium) and underlying-trigger orders, 1–8 legs filled
//!   all or nothing; `clientOrderId` makes a repeat harmless.
//! * `POST /v1/terminal/options/combos/{comboId}/close` — closes every leg of a strategy at once.
//! * `GET /v1/terminal/options/settlements?from&to` — the account's expiry settlements.
//! * Closing one position (also partially): the existing `POST /v1/terminal/positions/{ticket}/close`.
//!
//! Back Office (staff headers): `GET /v1/admin/options/book`, `GET /v1/admin/options/settlements`,
//! `POST /v1/admin/options/settlements/{expiry}/rerun`, `POST /v1/admin/options/trades/{ticket}/void`,
//! `GET /v1/admin/options/status`.

use axum::Json;
use axum::extract::{Path, Query, State};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::terminal::{self, PageQ, parse_side, parse_time};
use super::{ApiError, ApiResult, AppState, Body, Ctx, ROLES_CONFIG, ROLES_DEALING, StaffCtx, parse_ticket};
use crate::engine::options::{self as eopt, BarrierReq, LegReq, OptKind, OptOrderReq, PlaceOut};
use crate::engine::trade::DealerCtx;
use crate::engine::{AuditDraft, Reject};
use crate::model::{BarrierKind, Deal, Expiry, Source, Trigger, TriggerOp};
use crate::money::{D, ZERO, de_dec, de_opt_dec, num, num_opt, r2};
use crate::options::OptionPricing;
use crate::options::settle::{self, RERUN_WINDOW_SECS};
use crate::shard::{ExecError, Op};
use crate::views;

/* ------------------------------------------------------------------ */
/* Bodies                                                              */
/* ------------------------------------------------------------------ */

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BarrierBody {
    pub kind: String,
    #[serde(deserialize_with = "de_dec")]
    pub level: D,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub rebate: Option<D>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LegBody {
    pub series: String,
    pub side: String,
    #[serde(deserialize_with = "de_dec")]
    pub contracts: D,
    #[serde(default)]
    pub barrier: Option<BarrierBody>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TriggerBody {
    pub symbol: String,
    pub op: String,
    #[serde(deserialize_with = "de_dec")]
    pub price: D,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OrderBody {
    pub legs: Vec<LegBody>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub limit_premium: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub sl: Option<D>,
    #[serde(default, deserialize_with = "de_opt_dec")]
    pub tp: Option<D>,
    #[serde(default)]
    pub trigger: Option<TriggerBody>,
    #[serde(default)]
    pub tif: Option<String>,
    #[serde(default)]
    pub client_order_id: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub comment: Option<String>,
}

fn validation(field: &'static str, message: impl Into<String>) -> ApiError {
    ApiError::Validation { field, message: message.into() }
}

/// Body → engine request (suitability and the other accounts' contracts are filled in by the caller).
pub fn order_req(b: &OrderBody) -> ApiResult<OptOrderReq> {
    let mut legs = Vec::with_capacity(b.legs.len());
    for l in &b.legs {
        let barrier = match &l.barrier {
            None => None,
            Some(x) => Some(BarrierReq {
                kind: BarrierKind::parse(&x.kind).ok_or_else(|| validation("legs.barrier.kind", "barrier kind must be UO, DO, UI or DI"))?,
                level: x.level,
                rebate: x.rebate.unwrap_or(ZERO),
            }),
        };
        legs.push(LegReq { series: l.series.trim().to_string(), side: parse_side(&l.side)?, contracts: l.contracts, barrier });
    }
    let kind = match b.kind.as_deref().unwrap_or("market").to_ascii_lowercase().as_str() {
        "market" => OptKind::Market,
        "limit" => OptKind::Limit,
        _ => return Err(validation("type", "type must be market or limit")),
    };
    let trigger = match &b.trigger {
        None => None,
        Some(t) => Some(Trigger {
            symbol: t.symbol.trim().to_ascii_uppercase(),
            op: match t.op.to_ascii_lowercase().as_str() {
                "above" => TriggerOp::Above,
                "below" => TriggerOp::Below,
                _ => return Err(validation("trigger.op", "trigger op must be above or below")),
            },
            price: t.price,
        }),
    };
    let tif = match b.tif.as_deref().unwrap_or("gtc").to_ascii_lowercase().as_str() {
        "gtc" | "" => Expiry::Gtc,
        "day" | "today" => Expiry::Today,
        _ => return Err(validation("tif", "tif must be gtc or day")),
    };
    let source = match b.source.as_deref() {
        None | Some("") => Source::Manual,
        Some(s) => Source::parse_client(s).ok_or_else(|| validation("source", "unknown source"))?,
    };
    Ok(OptOrderReq {
        legs,
        kind,
        limit_premium: b.limit_premium,
        sl: b.sl.filter(|x| !x.is_zero()),
        tp: b.tp.filter(|x| !x.is_zero()),
        trigger,
        tif,
        client_order_id: b.client_order_id.clone().map(|c| c.trim().to_string()),
        source,
        platform: b.platform.clone().unwrap_or_else(|| "Web".into()).chars().take(32).collect(),
        comment: b.comment.clone().unwrap_or_default().chars().take(128).collect(),
        eligible: false,
        others: (ZERO, ZERO),
    })
}

/// Options eligibility (gateway suitability, cached 60 s): the client accepted the options intro. Live and demo
/// accounts alike.
pub async fn eligible(st: &AppState, s: &terminal::Session) -> bool {
    st.hub.meta(s.login).is_some() && st.hub.shared.options.suitability(&st.gateway, s.user_id).await.eligible
}

fn others(st: &AppState, s: &terminal::Session) -> (D, D) {
    st.hub.shared.index.read().unwrap().other_contracts(s.tenant_id, s.user_id, s.login)
}

fn reasons_json(r: &[Reject]) -> Value {
    json!(r.iter().map(|x| json!({"code": x.code, "message": x.message})).collect::<Vec<_>>())
}

fn greeks(g: (f64, f64, f64, f64)) -> Value {
    eopt::greeks_json(Some(g))
}

fn preview_json(p: &eopt::Preview, ccy: &str) -> Value {
    json!({
        "ok": p.reasons.is_empty(),
        "reasons": reasons_json(&p.reasons),
        "legs": p.legs.iter().map(|l| json!({
            "series": l.spec.terms.series,
            "side": l.spec.side.as_str(),
            "contracts": num(l.spec.contracts),
            "price": num(l.price),
            "premium": num(l.premium),
            "commission": num(l.commission),
            "bid": num(l.quote.bid),
            "ask": num(l.quote.ask),
            "mark": num(l.quote.mark),
            "iv": crate::options::pricing::round_to(l.quote.iv, 5),
            "state": l.quote.state.as_str(),
            "option": eopt::terms_json(&l.spec.terms),
        })).collect::<Vec<_>>(),
        "netPremium": num(p.net_premium),
        "commission": num(p.commission),
        "marginBefore": num(r2(p.margin_before)),
        "marginAfter": num(r2(p.margin_after)),
        "freeMarginAfter": num(r2(p.free_margin_after)),
        "cashAfter": num(r2(p.cash_after)),
        "maxProfit": num_opt(p.max_profit),
        "maxLoss": num_opt(p.max_loss),
        "breakevens": p.breakevens.iter().map(|b| num(*b)).collect::<Vec<_>>(),
        "greeks": greeks(p.greeks),
        "currency": ccy,
    })
}

/* ------------------------------------------------------------------ */
/* Terminal                                                            */
/* ------------------------------------------------------------------ */

/// `POST /v1/terminal/options/preview`
pub async fn preview(State(st): State<AppState>, ctx: Ctx, Body(b): Body<OrderBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let mut req = order_req(&b)?;
    req.eligible = eligible(&st, &s).await;
    req.others = others(&st, &s);
    let mut v = st
        .hub
        .read(
            s.login,
            Box::new(move |x| match x {
                Some((a, env)) => preview_json(&eopt::preview(env, a, &req), a.account.ccy()),
                None => Value::Null,
            }),
        )
        .await;
    if v.is_null() {
        return Err(ApiError::NotFound("Account not found".into()));
    }
    if let Err(e) = terminal::copy_guard(&st, &s, true) {
        // a copy account can't trade: say so in the preview too
        if let ApiError::Status { code, message, .. } = e {
            v["ok"] = json!(false);
            v["reasons"].as_array_mut().map(|a| a.insert(0, json!({"code": code, "message": message})));
        }
    }
    if s.read_only {
        v["ok"] = json!(false);
        v["reasons"].as_array_mut().map(|a| a.insert(0, json!({"code": "read_only", "message": "This is a view-only session"})));
    }
    Ok(Json(v))
}

fn place_json(env: &crate::engine::Env, st: &crate::state::AccountState, out: &PlaceOut, cid: Option<&str>) -> Value {
    match out {
        PlaceOut::Filled(f) => json!({
            "status": "filled",
            "comboId": f.combo_id,
            "positions": f.legs.iter().filter_map(|l| st.positions.get(&l.ticket)).map(|p| views::position_json(env, st, p)).collect::<Vec<_>>(),
            "fills": f.legs.iter().map(|l| json!({"ticket": l.ticket, "dealId": l.deal, "series": l.series, "side": l.side.as_str(), "contracts": num(l.contracts), "price": num(l.price), "premium": num(l.premium), "commission": num(l.commission)})).collect::<Vec<_>>(),
        }),
        PlaceOut::Pending { ticket } => json!({"status": "pending", "order": st.orders.get(ticket).map(views::order_json)}),
        PlaceOut::Duplicate { ticket } => match st.orders.get(ticket) {
            Some(o) => json!({"status": "pending", "duplicate": true, "order": views::order_json(o)}),
            None => {
                let positions: Vec<Value> = st.positions.values().filter(|p| p.option.is_some() && (p.client_order_id.as_deref() == cid || p.ticket == *ticket)).map(|p| views::position_json(env, st, p)).collect();
                json!({"status": "filled", "duplicate": true, "positions": positions})
            }
        },
    }
}

/// `POST /v1/terminal/options/orders`
pub async fn place(State(st): State<AppState>, ctx: Ctx, Body(b): Body<OrderBody>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    crate::modules::require(&st, &ctx.tenant.slug, "options").await?;
    terminal::copy_guard(&st, &s, true)?;
    let mut req = order_req(&b)?;
    if req.client_order_id.as_deref().is_none_or(str::is_empty) {
        return Err(validation("clientOrderId", "clientOrderId is required (it makes a repeated submit harmless)"));
    }
    // once the order book is live for this account kind, listed (vanilla) options trade there only; barriers stay
    // Kalks-quoted (docs/OPTIONS-EXCHANGE.md §11: house opens halted)
    if let Some(m) = st.hub.meta(s.login)
        && st.hub.shared.books.venue_enabled(s.tenant_id, m.kind)
        && req.legs.iter().any(|l| l.barrier.is_none())
    {
        return Err(ApiError::Status { status: 422, code: "book_venue", message: "Listed options trade on the options order book: place a book order".into() });
    }
    if st.social.is_fund(s.login) {
        req.source = Source::Pamm;
    }
    req.eligible = eligible(&st, &s).await;
    req.others = others(&st, &s);
    let delay = if req.kind == OptKind::Market && req.trigger.is_none() { terminal::exec_delay(&st, &ctx.tenant, s.login).await } else { 0 };
    let cid = req.client_order_id.clone();
    let op: Op = Box::new(move |tx, env| {
        let out = eopt::place(tx, env, req)?;
        Ok(place_json(env, &tx.st, &out, cid.as_deref()))
    });
    let mut v = terminal::run(&st, &s, op).await?;
    if delay > 0 {
        v["delayMs"] = json!(delay);
    }
    Ok(Json(v))
}

/// `POST /v1/terminal/options/combos/{comboId}/close`
pub async fn close_combo(State(st): State<AppState>, ctx: Ctx, Path(combo): Path<String>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    s.writable()?;
    terminal::copy_guard(&st, &s, false)?;
    let combo = parse_ticket(&combo)?;
    // a strategy on the order book closes by a reduce-only combo RFQ to the market maker (docs §5)
    if let Some(v) = super::options_book::book_close_combo(&st, &s, combo).await? {
        return Ok(Json(v));
    }
    let delay = terminal::exec_delay(&st, &ctx.tenant, s.login).await;
    let op: Op = Box::new(move |tx, env| {
        let legs = eopt::close_combo(tx, env, combo, None)?;
        let profit: D = legs.iter().map(|l| l.2).sum();
        Ok(json!({"status": "closed", "comboId": combo, "legs": legs.iter().map(|(t, d, p)| json!({"ticket": t, "dealId": d, "profit": num(*p)})).collect::<Vec<_>>(), "profit": num(profit)}))
    });
    let mut v = terminal::run(&st, &s, op).await?;
    if delay > 0 {
        v["delayMs"] = json!(delay);
    }
    Ok(Json(v))
}

fn settlement_item(d: &Deal, reversed: bool) -> Option<Value> {
    let o = d.option.as_ref()?;
    Some(json!({
        "ticket": d.position_ticket,
        "dealId": d.id,
        "series": o.terms.series,
        "underlying": o.terms.underlying,
        "expiry": o.terms.expiry.to_string(),
        "side": d.position_side.as_str(),
        "contracts": num(d.volume),
        "fixing": num_opt(o.fixing),
        "payout": num(o.cash),
        "profit": num(d.profit),
        "at": d.time,
        "run": o.run,
        "reversed": reversed,
    }))
}

/// `GET /v1/terminal/options/settlements?from&to`
pub async fn settlements(State(st): State<AppState>, ctx: Ctx, Query(q): Query<PageQ>) -> ApiResult<Json<Value>> {
    let s = terminal::session(&st, &ctx).await?;
    let rows = sqlx::query(
        "SELECT data, reversed FROM deals WHERE login = $1 AND reason = 'expiry' AND option IS NOT NULL
           AND ($2::timestamptz IS NULL OR time >= $2) AND ($3::timestamptz IS NULL OR time < $3) ORDER BY time DESC, id DESC LIMIT $4",
    )
    .bind(s.login)
    .bind(parse_time(&q.from)?)
    .bind(parse_time(&q.to)?)
    .bind(q.limit.unwrap_or(200).clamp(1, 1000))
    .fetch_all(&st.pool)
    .await?;
    let items: Vec<Value> = rows.iter().filter_map(|r| settlement_item(&r.get::<sqlx::types::Json<Deal>, _>("data").0, r.get("reversed"))).collect();
    Ok(Json(json!({"items": items})))
}

/* ------------------------------------------------------------------ */
/* Back Office                                                         */
/* ------------------------------------------------------------------ */

const ROLES_SETTLE: &[&str] = &["platform_owner", "super_admin", "admin", "risk_manager"];

#[derive(Deserialize, Default)]
pub struct BookQ {
    /// live (default) | demo | all
    kind: Option<String>,
}

/// `GET /v1/admin/options/book` — the house's options exposure per underlying (house = minus the clients),
/// the clients with the largest open P&L, recent settlement runs.
pub async fn book(State(st): State<AppState>, s: StaffCtx, Query(q): Query<BookQ>) -> ApiResult<Json<Value>> {
    s.require_perm("options.read", ROLES_DEALING)?;
    let tenant = s.ctx.tenant.tenant_id;
    let kind = q.kind.unwrap_or_else(|| "live".into());
    let hedge_login: Option<i64> = sqlx::query_scalar("SELECT login FROM option_hedge_accounts WHERE tenant_id = $1").bind(tenant).fetch_optional(&st.pool).await?;
    let k2 = kind.clone();
    let rows = st
        .hub
        .scan(
            tenant,
            Arc::new(move |a, env| {
                if Some(a.login()) == hedge_login || (k2 != "all" && a.account.kind.as_str() != k2) {
                    return vec![];
                }
                a.positions
                    .values()
                    // the house's exposure: order-book positions face another account, not the house
                    .filter(|p| !p.on_book())
                    .filter_map(|p| {
                        let t = p.option.as_ref()?;
                        let g = eopt::position_greeks(env, p).unwrap_or((0.0, 0.0, 0.0, 0.0));
                        let pnl = eopt::position_value(env, &a.account, p, t) + p.premium;
                        Some(json!({
                            "u": t.underlying, "user": a.account.user_id, "login": a.login(), "side": p.side.as_str(), "contracts": num(p.volume),
                            "size": num(t.contract_size), "delta": g.0, "gamma": g.1, "vega": g.2, "theta": g.3, "pnl": num(pnl / a.account.usd_factor()),
                        }))
                    })
                    .collect()
            }),
        )
        .await;
    #[derive(Default)]
    struct U {
        delta: f64,
        units: f64,
        gamma: f64,
        vega: f64,
        theta: f64,
        long: D,
        short: D,
        clients: BTreeSet<i64>,
        size: D,
    }
    let mut by_u: BTreeMap<String, U> = BTreeMap::new();
    let mut by_user: BTreeMap<i64, (D, D, BTreeMap<i64, D>)> = BTreeMap::new();
    for r in &rows {
        let dec = |k: &str| r[k].as_f64().and_then(crate::money::from_f64).unwrap_or(ZERO);
        let (Some(u), Some(user), Some(login)) = (r["u"].as_str(), r["user"].as_i64(), r["login"].as_i64()) else { continue };
        let e = by_u.entry(u.to_string()).or_default();
        let c = dec("contracts");
        let d = r["delta"].as_f64().unwrap_or(0.0);
        e.delta += d;
        e.units += d * r["size"].as_f64().unwrap_or(0.0);
        e.gamma += r["gamma"].as_f64().unwrap_or(0.0);
        e.vega += r["vega"].as_f64().unwrap_or(0.0);
        e.theta += r["theta"].as_f64().unwrap_or(0.0);
        if r["side"] == "buy" { e.long += c } else { e.short += c }
        e.clients.insert(user);
        e.size = dec("size");
        let x = by_user.entry(user).or_default();
        x.0 += dec("pnl");
        x.1 += c;
        *x.2.entry(login).or_default() += c;
    }
    let hedge = match hedge_login {
        Some(l) => crate::options::hedger::hedge_units(&st, l).await,
        None => Default::default(),
    };
    let underlyings: Vec<Value> = by_u
        .iter()
        .map(|(u, x)| {
            let h = hedge.get(u).copied().unwrap_or(0.0);
            let size = crate::options::f(x.size).max(1e-12);
            json!({
                "symbol": u,
                "netDelta": crate::options::pricing::round_to(-x.delta, 4),
                "netDeltaUnits": crate::options::pricing::round_to(-x.units, 2),
                "clientDelta": crate::options::pricing::round_to(x.delta, 4),
                "gamma": crate::options::pricing::round_to(-x.gamma, 6),
                "vega": crate::options::pricing::round_to(-x.vega, 2),
                "theta": crate::options::pricing::round_to(-x.theta, 2),
                "longContracts": num(x.long),
                "shortContracts": num(x.short),
                "clients": x.clients.len(),
                "hedgeContracts": crate::options::pricing::round_to(h / size, 4),
                "hedgeUnits": crate::options::pricing::round_to(h, 2),
                "deltaAfterHedgeUnits": crate::options::pricing::round_to(-x.units + h, 2),
            })
        })
        .collect();
    // realised option P&L today (server day) per login
    let today = crate::specs::server_midnight(crate::specs::server_date(Utc::now()));
    let realised: BTreeMap<i64, D> = sqlx::query("SELECT login, sum(profit) AS p FROM deals WHERE tenant_id = $1 AND option IS NOT NULL AND entry <> 'in' AND NOT reversed AND time >= $2 GROUP BY login")
        .bind(tenant)
        .bind(today)
        .fetch_all(&st.pool)
        .await?
        .iter()
        .map(|r| (r.get::<i64, _>("login"), r.get::<Option<D>, _>("p").unwrap_or(ZERO)))
        .collect();
    let mut top: Vec<Value> = by_user
        .iter()
        .map(|(user, (pnl, contracts, logins))| {
            let login = logins.iter().max_by_key(|(_, c)| **c).map(|(l, _)| *l).unwrap_or(0);
            let today_pnl: D = logins.keys().filter_map(|l| realised.get(l)).sum();
            json!({"userId": user, "login": login, "pnl": num(r2(*pnl)), "contracts": num(*contracts), "todayPnl": num(r2(today_pnl))})
        })
        .collect();
    top.sort_by(|a, b| b["pnl"].as_f64().unwrap_or(0.0).abs().total_cmp(&a["pnl"].as_f64().unwrap_or(0.0).abs()));
    top.truncate(20);
    let runs = settlement_runs(&st, tenant, 20).await?;
    let opts = &st.hub.shared.options;
    Ok(Json(json!({
        "kind": kind,
        "underlyings": underlyings,
        "topClients": top,
        "settlements": runs,
        "snapshot": {"version": opts.version(), "stale": opts.stale(st.hub.shared.clock.now()), "lastOkAt": chrono::DateTime::from_timestamp_millis(opts.last_ok_ms())},
        "hedgeAccount": hedge_login,
    })))
}

async fn settlement_runs(st: &AppState, tenant: i64, limit: i64) -> ApiResult<Vec<Value>> {
    let rows = sqlx::query("SELECT * FROM option_settlement_runs WHERE tenant_id = $1 ORDER BY started_at DESC, id DESC LIMIT $2").bind(tenant).bind(limit).fetch_all(&st.pool).await?;
    Ok(rows
        .iter()
        .map(|r| {
            json!({
                "id": r.get::<i64, _>("id"), "expiry": r.get::<String, _>("expiry_key"), "symbol": r.get::<String, _>("symbol"),
                "date": r.get::<chrono::NaiveDate, _>("expiry_date"), "run": r.get::<i32, _>("run"), "fixing": num(r.get::<D, _>("fixing")),
                "source": r.get::<Option<String>, _>("source"), "status": r.get::<String, _>("status"), "kind": r.get::<String, _>("kind"),
                "positions": r.get::<i32, _>("positions"), "accounts": r.get::<i32, _>("accounts"), "payoutUsd": num(r.get::<D, _>("payout_usd")),
                "failures": r.get::<i32, _>("failures"), "reason": r.get::<String, _>("reason"), "createdBy": r.get::<String, _>("created_by"),
                "startedAt": r.get::<chrono::DateTime<Utc>, _>("started_at"), "finishedAt": r.get::<Option<chrono::DateTime<Utc>>, _>("finished_at"),
            })
        })
        .collect())
}

#[derive(Deserialize, Default)]
pub struct RunsQ {
    limit: Option<i64>,
}

/// `GET /v1/admin/options/settlements` — recent settlement runs of the tenant.
pub async fn settlement_list(State(st): State<AppState>, s: StaffCtx, Query(q): Query<RunsQ>) -> ApiResult<Json<Value>> {
    s.require_perm("options.read", ROLES_DEALING)?;
    Ok(Json(json!({"items": settlement_runs(&st, s.ctx.tenant.tenant_id, q.limit.unwrap_or(50).clamp(1, 500)).await?})))
}

#[derive(Deserialize)]
pub struct RerunBody {
    #[serde(default)]
    reason: String,
}

/// `POST /v1/admin/options/settlements/{expiry}/rerun {reason}` — after the options service re-fixed an expiry
/// (within 1 h of the first settlement): every settlement deal of the expiry in this tenant is reversed and settled
/// again at the new fixing. Audited; clients are notified.
pub async fn rerun(State(st): State<AppState>, s: StaffCtx, Path(expiry): Path<String>, Body(b): Body<RerunBody>) -> ApiResult<Json<Value>> {
    s.require_perm("options.settle", ROLES_SETTLE)?;
    let reason = b.reason.trim().to_string();
    if reason.is_empty() {
        return Err(validation("reason", "A reason is required"));
    }
    let (symbol, date) = settle::parse_key(&expiry).ok_or_else(|| validation("expiry", "expiry must look like EURUSD:2026-10-09"))?;
    let key = settle::key(&symbol, date);
    let tenant = s.ctx.tenant.tenant_id;
    let opts = &st.hub.shared.options;
    // the current fixing: the options service first (freshest), then the snapshot
    let (fixing, run, source) = match opts.fixing(&symbol, date).await {
        Ok(Some(f)) if matches!(f.status.as_str(), "fixed" | "settled") && f.price.is_some() => (f.price.unwrap(), f.run.max(1), f.source),
        _ => settle::fixing_for(&st, &symbol, date).await.ok_or_else(|| ApiError::Conflict { code: "not_fixed", message: format!("{key} has no published fixing") })?,
    };
    let rows = sqlx::query(
        "SELECT data FROM deals WHERE tenant_id = $1 AND reason = 'expiry' AND NOT reversed AND option->'terms'->>'underlying' = $2 AND option->'terms'->>'expiry' = $3 ORDER BY id",
    )
    .bind(tenant)
    .bind(&symbol)
    .bind(date.to_string())
    .fetch_all(&st.pool)
    .await?;
    let deals: Vec<Deal> = rows.iter().map(|r| r.get::<sqlx::types::Json<Deal>, _>("data").0).collect();
    if deals.is_empty() {
        return Err(ApiError::Conflict { code: "nothing_to_rerun", message: format!("No settled positions of {key} in this tenant") });
    }
    let todo: Vec<&Deal> = deals.iter().filter(|d| d.option.as_ref().and_then(|o| o.run).is_some_and(|r| r < run)).collect();
    if todo.is_empty() {
        return Err(ApiError::Conflict { code: "nothing_to_rerun", message: format!("{key} is already settled at fixing run {run}: re-fix it in the options service first") });
    }
    let first: Option<chrono::DateTime<Utc>> = sqlx::query_scalar("SELECT min(finished_at) FROM option_settlement_runs WHERE tenant_id = $1 AND expiry_key = $2 AND kind = 'settle'").bind(tenant).bind(&key).fetch_one(&st.pool).await?;
    let first = first.or_else(|| deals.iter().map(|d| d.time).min()).unwrap_or_else(Utc::now);
    let now = st.hub.shared.clock.now();
    if (now - first).num_seconds() > RERUN_WINDOW_SECS {
        return Err(ApiError::Conflict { code: "rerun_window_closed", message: format!("{key} was settled at {}: the re-run window is {} minutes", first.format("%H:%M UTC"), RERUN_WINDOW_SECS / 60) });
    }
    let mut by_login: BTreeMap<i64, Vec<Deal>> = BTreeMap::new();
    for d in todo {
        by_login.entry(d.login).or_default().push(d.clone());
    }
    // crosses: one conversion rate for every account of the re-run (settle.rs)
    let usdq = settle::conversion(&st, &symbol, date).await;
    let mut rep = crate::shard::SettleReport::default();
    let mut audit = Vec::new();
    let mut failed = Vec::new();
    for (login, ds) in by_login {
        let k = key.clone();
        let staff_reason = reason.clone();
        let op: Op = Box::new(move |tx, env| {
            let before = tx.st.balance;
            let out = eopt::rerun(tx, env, &k, &ds, fixing, run, usdq)?;
            crate::engine::risk::check_margin(tx, env);
            tx.audit.push(AuditDraft {
                action: "options.settlement_rerun",
                tickets: out.tickets.clone(),
                login: None,
                symbol: Some(k.clone()),
                before: Some(json!({"deals": ds.iter().map(|d| d.id).collect::<Vec<_>>(), "fixing": ds.iter().find_map(|d| d.option.as_ref().and_then(|o| o.fixing)).map(num), "balance": num(before)})),
                after: Some(json!({"fixing": num(fixing), "run": run, "balance": num(tx.st.balance), "change": num(tx.st.balance - before), "reason": staff_reason})),
                flags: vec!["settlement re-run".into(), "client statement".into()],
            });
            Ok(json!({"tickets": out.tickets, "cashUsd": num((tx.st.balance - before) / tx.st.account.usd_factor())}))
        });
        match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), "SETTLEMENT-RERUN", &reason, None, op).await {
            Ok(d) => {
                let n = d.value["tickets"].as_array().map(Vec::len).unwrap_or(0);
                let cash = d.value["cashUsd"].as_f64().and_then(crate::money::from_f64).unwrap_or(ZERO);
                rep.accounts.push((tenant, login, n, cash));
                audit.extend(d.audit);
            }
            Err(e) => {
                rep.failures += 1;
                failed.push(json!({"login": login, "error": format!("{e:?}")}));
            }
        }
    }
    // the rounding the re-run left in the expiry's clearing account (book positions)
    if rep.failures == 0 {
        settle::sweep_clearing(&st.hub, &symbol, date, run).await;
    }
    settle::record(&st, &rep, &key, &symbol, date, run, fixing, source.as_deref(), "rerun", &reason, &s.staff.name).await;
    settle::notify(&st, &rep, &key, run, fixing, true).await;
    let positions: usize = rep.accounts.iter().map(|a| a.2).sum();
    let change: D = rep.accounts.iter().map(|a| a.3).sum();
    tracing::info!(%key, run, fixing = %fixing, positions, staff = %s.staff.name, "options settlement re-run");
    Ok(Json(json!({"expiry": key, "run": run, "fixing": num(fixing), "accounts": rep.accounts.len(), "positions": positions, "cashChange": num(r2(change)), "failed": failed, "audit": audit})))
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VoidBody {
    #[serde(default)]
    reason_code: String,
    #[serde(default)]
    note: String,
}

/// `POST /v1/admin/options/trades/{ticket}/void {reasonCode, note}` — an erroneous option trade is reversed
/// completely (premium, proceeds, payouts and commissions), shown to the client as corrections.
pub async fn void(State(st): State<AppState>, s: StaffCtx, Path(ticket): Path<String>, Body(b): Body<VoidBody>) -> ApiResult<Json<Value>> {
    s.require_perm("options.dealing", ROLES_DEALING)?;
    if b.reason_code.trim().is_empty() {
        return Err(validation("reasonCode", "Select a reason code"));
    }
    if b.note.trim().is_empty() {
        return Err(validation("note", "Voiding a trade needs a note"));
    }
    let ticket = parse_ticket(&ticket)?;
    let tenant = s.ctx.tenant.tenant_id;
    let rows = sqlx::query("SELECT data FROM deals WHERE tenant_id = $1 AND position_ticket = $2 ORDER BY id").bind(tenant).bind(ticket).fetch_all(&st.pool).await?;
    let deals: Vec<Deal> = rows.iter().map(|r| r.get::<sqlx::types::Json<Deal>, _>("data").0).collect();
    let login = deals.first().map(|d| d.login).or_else(|| st.hub.login_of_ticket(ticket)).ok_or_else(|| ApiError::NotFound(format!("Option trade #{ticket} not found")))?;
    match st.hub.meta(login) {
        Some(m) if m.tenant_id == tenant => {}
        _ => return Err(ApiError::NotFound(format!("Option trade #{ticket} not found"))),
    }
    let dealer = DealerCtx { staff: s.staff.name.clone(), reason_code: b.reason_code.trim().to_string(), force: false };
    let note = b.note.trim().to_string();
    let op: Op = Box::new(move |tx, env| {
        let before = tx.st.balance;
        let v = eopt::void(tx, env, ticket, &deals, &dealer)?;
        tx.audit.push(AuditDraft {
            action: "options.void",
            tickets: vec![ticket],
            login: None,
            symbol: deals.first().map(|d| d.symbol.clone()),
            before: Some(json!({"balance": num(before), "deals": v["deals"].clone()})),
            after: Some(json!({"balance": num(tx.st.balance), "status": "void — premium and commission reversed"})),
            flags: vec!["void".into(), "correction".into(), "client statement".into()],
        });
        Ok(v)
    });
    match st.hub.exec(login, &format!("staff:{}", s.staff.id), Some(s.staff.clone()), &b.reason_code, &note, None, op).await {
        Ok(d) => Ok(Json(json!({"data": d.value, "audit": d.audit}))),
        Err(ExecError::Reject(r)) => Err(ApiError::reject(r)),
        Err(e) => Err(e.into()),
    }
}

/// `GET /v1/admin/options/status` — the engine's view of the options service.
pub async fn status(State(st): State<AppState>, s: StaffCtx) -> ApiResult<Json<Value>> {
    s.require(ROLES_CONFIG.iter().chain(ROLES_DEALING).copied().collect::<Vec<_>>().as_slice())?;
    let opts = &st.hub.shared.options;
    let now = st.hub.shared.clock.now();
    let snap = opts.snapshot();
    Ok(Json(json!({
        "configured": opts.configured(),
        "version": snap.as_ref().map(|x| x.version),
        "stale": opts.stale(now),
        "staleAfterSecs": snap.as_ref().map(|x| x.stale_after_secs),
        "lastOkAt": chrono::DateTime::from_timestamp_millis(opts.last_ok_ms()),
        "enabled": snap.as_ref().map(|x| json!({"live": x.enabled(&s.ctx.tenant.slug, true), "demo": x.enabled(&s.ctx.tenant.slug, false)})),
        "underlyings": snap.as_ref().map(|x| x.underlyings.values().filter(|u| u.enabled).map(|u| json!({"symbol": u.symbol, "spot": opts.spot(&u.symbol).map(|s| s.0)})).collect::<Vec<_>>()),
    })))
}

//! JSON views for the HTTP API and the streams. Money, prices and volumes are JSON numbers
//! (exact decimal text of the engine's decimals); timestamps are RFC 3339 UTC; tickets are numbers in
//! the client/terminal API and strings in the dealing API (the Back Office contract uses string tickets).

use serde_json::{Value, json};

use crate::engine::{Env, Metrics, metrics, options, pnl};
use crate::model::{Deal, DealReason, Expiry, Order, OrderType, Position, Side};
use crate::money::{D, num, num_opt, r2};
use crate::state::AccountState;

pub fn metrics_json(m: &Metrics) -> Value {
    json!({
        "balance": num(r2(m.balance)),
        "credit": num(r2(m.credit)),
        "bonus": num(r2(m.bonus)),
        "profit": num(r2(m.profit)),
        "swap": num(r2(m.swap)),
        "equity": num(r2(m.equity)),
        "margin": num(r2(m.margin)),
        "freeMargin": num(r2(m.free_margin)),
        "marginLevel": num_opt(m.level.map(r2)),
        "withdrawable": num(r2(m.withdrawable())),
        "optionValue": num(r2(m.option_value)),
        "optionPnl": num(r2(m.option_pnl)),
        "optionMargin": num(r2(m.option_margin)),
        "settlementHold": num(r2(m.held)),
        "orderReserve": num(r2(m.order_reserve)),
    })
}

pub fn account_json(env: &Env, st: &AccountState) -> Value {
    let a = &st.account;
    let m = metrics(env, st);
    let mut v = json!({
        "login": a.login,
        "userId": a.user_id,
        "type": a.kind.as_str(),
        "group": a.group,
        "groupName": env.group.name,
        // CFD / Options account split: what the account trades (its group's product)
        "product": env.group.product.as_str(),
        "mode": if a.mode == crate::model::Mode::Netting { "netting" } else { "hedging" },
        "cent": a.cent,
        "currency": a.ccy(),
        "baseCurrency": "USD",
        "leverage": a.leverage,
        "leverages": env.group.leverages,
        "status": a.status.as_str(),
        "name": a.name,
        "route": a.route_override.map(|b| b.as_str()).unwrap_or(env.group.route.as_str()),
        "marginCall": st.margin_call,
        "marginCallLevel": num(env.group.margin_call_pct),
        "stopOutLevel": num(env.group.stop_out_pct),
        "positions": st.positions.len(),
        "orders": st.orders.len(),
        "controls": {
            "tradingDisabled": a.controls.trading_disabled,
            "closeOnly": a.controls.close_only,
            "maxLot": num_opt(a.controls.max_lot),
            "execDelayMs": a.controls.exec_delay_ms,
            "markupPips": num(a.controls.markup_pips),
        },
        "createdAt": a.created_at,
        "version": st.version,
    });
    if let Some(d) = &a.demo {
        let today = crate::specs::server_date(env.now);
        v["demo"] = json!({
            "initialBalance": num(d.initial_balance * a.usd_factor()),
            "refillsPerDay": d.refills_per_day,
            "refillsUsedToday": if st.refill_day == Some(today) { st.refills } else { 0 },
            "expiryDays": d.expiry_days,
        });
    }
    if let (Value::Object(o), Value::Object(mj)) = (&mut v, metrics_json(&m)) {
        o.extend(mj);
    }
    v
}

/// (price the position closes at now, floating P&L) — options from the mark cache.
pub fn current(env: &Env, st: &AccountState, p: &Position) -> (Option<D>, Option<D>) {
    if p.option.is_some() {
        return options::position_now(env, &st.account, p);
    }
    let Some(q) = env.quote(&st.account, &p.symbol) else { return (None, None) };
    let px = q.close_price(p.side);
    let profit = env.specs.get(&p.symbol).map(|s| r2(pnl(env, &st.account, s, p.side, p.volume, p.open_price, px)));
    (Some(px), profit)
}

/// Kalks FX Options fields of a position (`option`, `mark`, `greeks`, `comboId`; null for CFD positions).
pub fn option_fields(env: &Env, st: &AccountState, p: &Position) -> Value {
    match &p.option {
        None => json!({"option": null, "mark": null, "greeks": null, "comboId": null}),
        Some(t) => {
            let q = options::mark_of(env, &st.account, t);
            json!({
                "option": options::terms_json(t),
                "venue": p.venue.unwrap_or(crate::model::Venue::House).as_str(),
                "mark": num_opt(q.map(|q| q.mark)),
                "markValue": num(r2(options::position_value(env, &st.account, p, t))),
                "premium": num(p.premium),
                "greeks": options::greeks_json(options::position_greeks(env, p)),
                "comboId": p.combo_id,
                "iv": q.map(|q| crate::options::pricing::round_to(q.iv, 5)),
                "underlyingPrice": num_opt(q.map(|q| q.spot)),
                "state": q.map(|q| q.state.as_str()),
            })
        }
    }
}

fn merge(mut v: Value, extra: Value) -> Value {
    if let (Value::Object(o), Value::Object(e)) = (&mut v, extra) {
        o.extend(e);
    }
    v
}

pub fn position_json(env: &Env, st: &AccountState, p: &Position) -> Value {
    let (px, profit) = current(env, st, p);
    let v = json!({
        "ticket": p.ticket,
        "login": p.login,
        "symbol": p.symbol,
        "side": p.side.as_str(),
        "volume": num(p.volume),
        "openPrice": num(p.open_price),
        "openTime": p.open_time,
        "sl": num_opt(p.sl),
        "tp": num_opt(p.tp),
        "trailingPoints": p.trailing.as_ref().map(|t| t.distance_points),
        "swap": num(p.swap),
        "commission": num(p.commission),
        "currentPrice": num_opt(px),
        "profit": num_opt(profit),
        "source": p.source.as_str(),
        "platform": p.platform,
        "comment": p.comment,
        "book": p.book.as_str(),
        "parentTicket": p.parent_ticket,
        "childTickets": p.child_tickets,
        "priceCorrected": p.price_corrected,
        "reversedFrom": p.reversed_from,
    });
    merge(v, option_fields(env, st, p))
}

pub fn expiry_str(e: Expiry) -> &'static str {
    match e {
        Expiry::Gtc => "GTC",
        Expiry::Today => "Today",
        Expiry::Date => "Date",
    }
}

pub fn order_option_json(o: &Order) -> Value {
    json!({
        "option": o.option.as_ref().map(|oo| json!({
            "legs": oo.legs.iter().map(|l| json!({"series": l.terms.series, "side": l.side.as_str(), "contracts": num(l.contracts), "option": options::terms_json(&l.terms)})).collect::<Vec<_>>(),
            "limitPremium": num_opt(oo.limit_premium),
        })),
        "trigger": o.trigger.as_ref().map(|t| json!({"symbol": t.symbol, "op": t.op.as_str(), "price": num(t.price)})),
        "comboId": o.combo_id,
    })
}

pub fn order_json(o: &Order) -> Value {
    let v = json!({
        "ticket": o.ticket,
        "login": o.login,
        "symbol": o.symbol,
        "side": o.side.as_str(),
        "type": o.kind.as_str(),
        "volume": num(o.volume),
        "price": num(o.price),
        "stopLimit": num_opt(o.stop_limit),
        "triggered": o.triggered,
        "sl": num_opt(o.sl),
        "tp": num_opt(o.tp),
        "trailingPoints": o.trailing.as_ref().map(|t| t.distance_points),
        "expiry": expiry_str(o.expiry),
        "expiryAt": o.expiry_at,
        "oco": o.oco,
        "source": o.source.as_str(),
        "platform": o.platform,
        "comment": o.comment,
        "book": o.book.map(|b| b.as_str()),
        "placedAt": o.placed_at,
        "clientOrderId": o.client_order_id,
    });
    merge(v, order_option_json(o))
}

/// `option` (null on CFD deals) plus `instrument: "option" | "cfd"`, so downstream consumers (IB, growth,
/// reports, prop, algo) can tell option deals (volume = contracts) from CFD deals (volume = lots).
pub fn deal_option_json(d: &Deal) -> Value {
    match &d.option {
        None => json!({"option": null, "instrument": "cfd"}),
        Some(o) => json!({"instrument": "option", "option": {
            "series": o.terms.series, "underlying": o.terms.underlying, "right": o.terms.right.as_str(), "strike": num(o.terms.strike),
            "expiry": o.terms.expiry.to_string(), "style": o.terms.style(), "cash": num(o.cash), "usdPerQuote": num(o.usd_per_quote),
            "spot": num_opt(o.spot), "fixing": num_opt(o.fixing), "run": o.run, "comboId": o.combo_id, "commissionCharged": num(o.charged),
        }}),
    }
}

pub fn deal_json(d: &Deal) -> Value {
    let v = json!({
        "id": d.id,
        "login": d.login,
        "positionTicket": d.position_ticket,
        "orderTicket": d.order_ticket,
        "symbol": d.symbol,
        "side": d.side.as_str(),
        "positionSide": d.position_side.as_str(),
        "entry": d.entry.as_str(),
        "volume": num(d.volume),
        "price": num(d.price),
        "profit": num(d.profit),
        "swap": num(d.swap),
        "commission": num(d.commission),
        "reason": d.reason.as_str(),
        "book": d.book.as_str(),
        "time": d.time,
        "openPrice": num(d.open_price),
        "openTime": d.open_time,
        "source": d.source.as_str(),
        "comment": d.comment,
        "priceCorrection": d.price_correction,
        "ledgerTxn": d.ledger_txn,
    });
    merge(v, deal_option_json(d))
}

/* ------------------------------------------------------------------ */
/* Dealing (Back Office contract: apps/admin/lib/trading-desk/types.ts) */
/* ------------------------------------------------------------------ */

pub fn desk_position_json(env: &Env, st: &AccountState, p: &Position) -> Value {
    let (px, profit) = current(env, st, p);
    let v = json!({
        "ticket": p.ticket.to_string(),
        "login": p.login.to_string(),
        "clientId": st.account.user_id.to_string(),
        "symbol": p.symbol,
        "side": p.side.as_str(),
        "volume": num(p.volume),
        "openPrice": num(p.open_price),
        "sl": num_opt(p.sl),
        "tp": num_opt(p.tp),
        "swap": num(p.swap),
        "commission": num(p.commission),
        "openTime": p.open_time,
        "source": p.source.as_str(),
        "platform": if p.source == crate::model::Source::Dealer { "Back Office".to_string() } else { p.platform.clone() },
        "group": st.account.group,
        "groupName": env.group.name,
        "route": p.book.as_str(),
        "parentTicket": p.parent_ticket.map(|t| t.to_string()),
        "comment": p.comment,
        "bookSince": p.book_since,
        "bookPrice": num(p.book_price),
        "bookCarry": {"A": num(r2(p.book_carry_a)), "B": num(r2(p.book_carry_b))},
        "routeHistory": p.route_history.iter().map(|e| json!({
            "at": e.at, "kind": e.kind, "from": e.from.map(|b| b.as_str()), "to": e.to.as_str(), "volume": num(e.volume),
            "price": num(e.price), "staff": e.staff, "reason": e.reason, "relatedTicket": e.related_ticket.map(|t| t.to_string()),
        })).collect::<Vec<_>>(),
        "childTickets": if p.child_tickets.is_empty() { Value::Null } else { json!(p.child_tickets.iter().map(|t| t.to_string()).collect::<Vec<_>>()) },
        "priceCorrected": p.price_corrected,
        "currentPrice": num_opt(px),
        "profit": num_opt(profit),
        "trailingPoints": p.trailing.as_ref().map(|t| t.distance_points),
        "currency": st.account.ccy(),
    });
    merge(v, option_fields(env, st, p))
}

pub fn desk_order_type(o: &Order) -> &'static str {
    match (o.side, o.kind) {
        (Side::Buy, OrderType::Limit) => "Buy Limit",
        (Side::Sell, OrderType::Limit) => "Sell Limit",
        (Side::Buy, OrderType::Stop) => "Buy Stop",
        (Side::Sell, OrderType::Stop) => "Sell Stop",
        (Side::Buy, OrderType::StopLimit) => "Buy Stop Limit",
        (Side::Sell, OrderType::StopLimit) => "Sell Stop Limit",
        (Side::Buy, OrderType::Market) => "Buy",
        (Side::Sell, OrderType::Market) => "Sell",
    }
}

pub fn desk_order_json(st: &AccountState, o: &Order) -> Value {
    let v = json!({
        "ticket": o.ticket.to_string(),
        "login": o.login.to_string(),
        "clientId": st.account.user_id.to_string(),
        "symbol": o.symbol,
        "type": desk_order_type(o),
        "volume": num(o.volume),
        "price": num(o.price),
        "stopLimit": num_opt(o.stop_limit),
        "sl": num_opt(o.sl),
        "tp": num_opt(o.tp),
        "placed": o.placed_at,
        "expiry": match o.expiry { Expiry::Gtc => json!("GTC"), Expiry::Today => json!("Today"), Expiry::Date => json!(o.expiry_at) },
        "group": st.account.group,
        "source": o.source.as_str(),
        "comment": o.comment,
        "book": o.book.map(|b| b.as_str()),
        "triggered": o.triggered,
        "oco": o.oco.map(|t| t.to_string()),
    });
    merge(v, order_option_json(o))
}

/// DeskDeal: `profit` is the realised P&L incl. the swap and commission share of the closed volume.
pub fn desk_deal_json(d: &Deal, client_id: i64, reversed: bool) -> Value {
    let kind = match d.reason {
        DealReason::PriceCorrection => "price-correction",
        DealReason::StopOut => "stop-out",
        DealReason::Force => "force",
        DealReason::Expiry => "expiry",
        DealReason::KnockOut => "knock-out",
        _ if d.partial => "partial",
        _ => "close",
    };
    let v = json!({
        "id": d.id.to_string(),
        "ticket": d.position_ticket.to_string(),
        "login": d.login.to_string(),
        "clientId": client_id.to_string(),
        "symbol": d.symbol,
        "side": d.position_side.as_str(),
        "volume": num(d.volume),
        "openPrice": num(d.open_price),
        "closePrice": num(d.price),
        "openTime": d.open_time,
        "closeTime": d.time,
        "profit": num(d.profit + d.swap - d.commission),
        "priceProfit": num(d.profit),
        "swap": num(d.swap),
        "commission": num(d.commission),
        "book": d.book.as_str(),
        "kind": kind,
        "reason": d.reason.as_str(),
        "priceCorrection": d.price_correction,
        "reversed": reversed,
        "staff": d.staff.clone().unwrap_or_else(|| "system".into()),
        "reasonCode": d.reason_code.clone().unwrap_or_default(),
    });
    merge(v, deal_option_json(d))
}

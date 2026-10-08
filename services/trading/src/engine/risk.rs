//! Tick-driven and scheduled logic: pending-order triggers, SL/TP, trailing stops, order expiry,
//! margin call, stop-out (largest loser first), negative balance protection, swap rollover.

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::json;

use super::trade::{CloseMeta, Fill, apply_nbp, close_part, fill, gate};
use super::{Env, Quote, Tx, metrics, position_floating};
use crate::model::{DealReason, OrderStatus, OrderType, Position, Side};
use crate::money::{D, ZERO, num, r2};
use crate::state::Event;

/// Has pending order `o` reached its price on quote `q`?
fn reached(kind: OrderType, side: Side, price: D, q: &Quote) -> bool {
    match (kind, side) {
        (OrderType::Limit, Side::Buy) => q.ask <= price,
        (OrderType::Limit, Side::Sell) => q.bid >= price,
        (OrderType::Stop | OrderType::StopLimit, Side::Buy) => q.ask >= price,
        (OrderType::Stop | OrderType::StopLimit, Side::Sell) => q.bid <= price,
        (OrderType::Market, _) => true,
    }
}

/// Everything a new quote on `symbol` can trigger for this account.
pub fn on_tick(tx: &mut Tx, env: &Env, symbol: &str) {
    // a corporate action waits for this account: no triggers, SL / TP or margin on old sizes with new prices
    if env.corp_pending(&tx.st, symbol).is_some() {
        return;
    }
    let acc = tx.st.account.clone();
    let Some(spec) = env.specs.get(symbol) else { return };
    let Some(q) = env.quote(&acc, symbol) else { return };
    if !spec.is_open(env.now) {
        return;
    }
    expire_orders(tx, env);

    // 1. pending orders
    let tickets: Vec<i64> = tx.st.orders.values().filter(|o| o.symbol == symbol && o.option.is_none()).map(|o| o.ticket).collect();
    for t in tickets {
        let Some(mut o) = tx.st.orders.get(&t).cloned() else { continue }; // OCO partner may be gone
        if o.kind == OrderType::StopLimit && !o.triggered && reached(OrderType::Stop, o.side, o.price, &q) {
            o.triggered = true;
            tx.emit(Event::OrderUpdated { order: o.clone(), change: "stop_limit_triggered".into() });
            tx.note("order_triggered", format!("Stop-limit #{t} triggered: now a limit order at {}", o.active_price().normalize()), json!({"ticket": t}));
        }
        if o.kind == OrderType::StopLimit && !o.triggered {
            continue;
        }
        if reached(o.active_kind(), o.side, o.active_price(), &q) {
            trigger_fill(tx, env, o.ticket, &q);
        }
    }

    // 2. positions: trailing stop, then SL / TP
    let tickets: Vec<i64> = tx.st.positions.values().filter(|p| p.symbol == symbol && p.option.is_none()).map(|p| p.ticket).collect();
    let mut closed_any = false;
    for t in tickets {
        let Some(p) = tx.st.positions.get(&t).cloned() else { continue };
        let p = trail(tx, env, p, &q);
        let px = q.close_price(p.side);
        let hit = match p.side {
            Side::Buy => (p.sl.is_some_and(|s| px <= s), p.tp.is_some_and(|s| px >= s)),
            Side::Sell => (p.sl.is_some_and(|s| px >= s), p.tp.is_some_and(|s| px <= s)),
        };
        let reason = match hit {
            (true, _) => DealReason::Sl,
            (_, true) => DealReason::Tp,
            _ => continue,
        };
        let meta = CloseMeta { comment: format!("[{}]", if reason == DealReason::Sl { "sl" } else { "tp" }), ..CloseMeta::system(reason) };
        if let Ok((deal, profit)) = close_part(tx, env, t, p.volume, px, meta) {
            closed_any = true;
            let what = if reason == DealReason::Sl { "Stop loss" } else { "Take profit" };
            tx.note(if reason == DealReason::Sl { "sl" } else { "tp" }, format!("{what} hit on #{t} {} {} at {}", p.side.as_str(), p.symbol, px.normalize()), json!({"ticket": t, "dealId": deal, "profit": num(profit)}));
        }
    }
    // NBP only for losses realised by trading (not for a staff adjustment that made the balance negative)
    if closed_any {
        apply_nbp(tx, env);
    }

    // 3. margin
    check_margin(tx, env);
}

fn trigger_fill(tx: &mut Tx, env: &Env, ticket: i64, q: &Quote) {
    let Some(o) = tx.st.orders.get(&ticket).cloned() else { return };
    let price = q.open_price(o.side);
    // halted symbol, disabled / close-only account, a CFD order on what is now an Options account (CFD / Options
    // account split): the order stays pending until trading is allowed again (or staff cancel it)
    let opening = super::trade::is_opening(&tx.st, &o.symbol, o.side, o.volume);
    if super::trade::cfd_product_gate(env, opening).is_err() || gate(env, &tx.st, &o.symbol, opening, o.volume, None).is_err() {
        return;
    }
    let res = {
        fill(
            tx,
            env,
            Fill {
                symbol: o.symbol.clone(),
                side: o.side,
                volume: o.volume,
                price,
                sl: o.sl,
                tp: o.tp,
                trailing: o.trailing.clone(),
                source: o.source,
                platform: o.platform.clone(),
                comment: o.comment.clone(),
                order_ticket: o.ticket,
                book: o.book,
                client_order_id: None,
                reason: DealReason::PendingFill,
                staff: None,
                reason_code: None,
                check_margin: true,
            },
        )
    };
    match res {
        Ok(out) => {
            tx.emit(Event::OrderRemoved { ticket, status: OrderStatus::Filled, reason: "triggered".into(), at: env.now, fill_price: Some(price), position_ticket: out.position_ticket });
            tx.note("order_filled", format!("Order #{ticket} {} {} {} filled at {}", o.side.as_str(), o.volume.normalize(), o.symbol, price.normalize()), json!({"ticket": ticket, "positionTicket": out.position_ticket, "price": num(price)}));
            if let Some(partner) = o.oco
                && tx.st.orders.contains_key(&partner)
            {
                tx.emit(Event::OrderRemoved { ticket: partner, status: OrderStatus::Cancelled, reason: format!("OCO: #{ticket} filled"), at: env.now, fill_price: None, position_ticket: None });
                tx.note("order_cancelled", format!("Order #{partner} cancelled (OCO with #{ticket})"), json!({"ticket": partner}));
            }
        }
        Err(e) => {
            tx.emit(Event::OrderRemoved { ticket, status: OrderStatus::Rejected, reason: e.message.clone(), at: env.now, fill_price: None, position_ticket: None });
            tx.note("order_rejected", format!("Order #{ticket} could not be filled: {}", e.message), json!({"ticket": ticket, "code": e.code}));
        }
    }
}

/// Server-side trailing stop: once the position is `distance` in profit, the SL follows the price at
/// `distance`, moving only forward and only by at least `step`.
fn trail(tx: &mut Tx, env: &Env, p: Position, q: &Quote) -> Position {
    let Some(tr) = p.trailing.clone() else { return p };
    let Some(spec) = env.specs.get(&p.symbol) else { return p };
    let d = D::from(tr.distance_points) * spec.point;
    let step = D::from(tr.step_points.max(1)) * spec.point;
    let px = q.close_price(p.side);
    let cand = match p.side {
        Side::Buy if px - p.open_price >= d => spec.round_price(px - d),
        Side::Sell if p.open_price - px >= d => spec.round_price(px + d),
        _ => return p,
    };
    let better = match (p.side, p.sl) {
        (_, None) => true,
        (Side::Buy, Some(s)) => cand >= s + step,
        (Side::Sell, Some(s)) => cand <= s - step,
    };
    if !better {
        return p;
    }
    let np = Position { sl: Some(cand), ..p };
    tx.emit(Event::PositionUpdated { position: np.clone(), change: "trailing".into(), deal: None });
    np
}

/// GTC / Today / Date expiry.
pub fn expire_orders(tx: &mut Tx, env: &Env) {
    let due: Vec<i64> = tx.st.orders.values().filter(|o| o.expiry_at.is_some_and(|at| at <= env.now)).map(|o| o.ticket).collect();
    for t in due {
        tx.emit(Event::OrderRemoved { ticket: t, status: OrderStatus::Expired, reason: "expired".into(), at: env.now, fill_price: None, position_ticket: None });
        tx.note("order_expired", format!("Order #{t} expired"), json!({"ticket": t}));
    }
}

/// Margin call notification (with 5-point hysteresis) and stop-out (D16).
pub fn check_margin(tx: &mut Tx, env: &Env) {
    if env.corp.is_some_and(|c| c.any_pending(&tx.st, env.now)) {
        return;
    }
    let m = metrics(env, &tx.st);
    let g = env.group;
    match m.level {
        Some(l) if l <= g.stop_out_pct => stop_out(tx, env),
        Some(l) if l <= g.margin_call_pct => {
            if !tx.st.margin_call {
                tx.emit(Event::MarginCall { entered: true, level: Some(r2(l)) });
                tx.note("margin_call", format!("Margin call: margin level {}% (call at {}%, stop-out at {}%)", r2(l).normalize(), g.margin_call_pct.normalize(), g.stop_out_pct.normalize()), json!({"level": num(r2(l))}));
            }
        }
        l => {
            if tx.st.margin_call && l.is_none_or(|l| l > g.margin_call_pct + D::from(5)) {
                tx.emit(Event::MarginCall { entered: false, level: l.map(r2) });
            }
        }
    }
}

/// Closes the largest losing position first until the margin level is back above the stop-out level. An account
/// holding options closes by units instead: the unit (strategy, option or CFD position) that frees the most
/// margin first, strategies with all their legs together (engine::options::stop_out).
pub fn stop_out(tx: &mut Tx, env: &Env) {
    if tx.st.positions.values().any(|p| p.option.is_some()) {
        return super::options::stop_out(tx, env);
    }
    let acc = tx.st.account.clone();
    let start = metrics(env, &tx.st).level;
    tx.emit(Event::StopOut { level: start.map(r2) });
    let mut closed = Vec::new();
    for _ in 0..1000 {
        let m = metrics(env, &tx.st);
        match m.level {
            Some(l) if l <= env.group.stop_out_pct => {}
            _ => break,
        }
        let pick = tx
            .st
            .positions
            .values()
            .filter(|p| env.specs.get(&p.symbol).is_some_and(|s| s.is_open(env.now)))
            .filter_map(|p| position_floating(env, &acc, p).map(|f| (f, p.volume, p.ticket)))
            .min_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2)));
        let Some((_, _, ticket)) = pick else { break };
        let p = tx.st.positions[&ticket].clone();
        let q = env.quote(&acc, &p.symbol).unwrap();
        let meta = CloseMeta { comment: "[so]".into(), ..CloseMeta::system(DealReason::StopOut) };
        match close_part(tx, env, ticket, p.volume, q.close_price(p.side), meta) {
            Ok((deal, profit)) => closed.push(json!({"ticket": ticket, "dealId": deal, "profit": num(profit)})),
            Err(_) => break,
        }
    }
    tx.note("stop_out", format!("Stop-out: {} position(s) closed at margin level {}%", closed.len(), start.map(|l| r2(l).normalize().to_string()).unwrap_or_default()), json!({"closed": closed}));
    apply_nbp(tx, env);
    if tx.st.margin_call && tx.st.positions.is_empty() {
        tx.emit(Event::MarginCall { entered: false, level: None });
    }
}

/// Swap rollover for the server day `day` ending at `at` (D19). Idempotent per position via `last_swap_day`.
pub fn rollover(tx: &mut Tx, env: &Env, day: NaiveDate, at: DateTime<Utc>) {
    if env.group.swap_free {
        return;
    }
    let acc = tx.st.account.clone();
    let tickets: Vec<i64> = tx.st.positions.keys().copied().collect();
    let mut total = ZERO;
    for t in tickets {
        let p = tx.st.positions[&t].clone();
        // options pay no swap: the premium is paid in full
        if p.option.is_some() || p.open_time >= at || p.last_swap_day.is_some_and(|d| d >= day) {
            continue;
        }
        let Some(spec) = env.specs.get(&p.symbol) else { continue };
        let mult = spec.swap_multiplier(day);
        if mult == 0 {
            continue;
        }
        let own = env.quote(&acc, &p.symbol).map(|q| q.mid()).unwrap_or(p.open_price);
        // points per lot (core instruments) or a yearly percentage of the position's value at tonight's price
        let amount_quote = spec.swap_per_night(p.side == Side::Buy, p.volume, own) * D::from(mult);
        let Some(usd) = env.to_usd(&acc, &spec.quote_ccy, amount_quote, (&p.symbol, own)) else { continue };
        let amt = r2(usd * acc.usd_factor());
        if amt.is_zero() {
            continue;
        }
        total += amt;
        let np = Position { swap: p.swap + amt, last_swap_day: Some(day), ..p };
        tx.emit(Event::PositionUpdated { position: np, change: format!("swap {day} x{mult}"), deal: None });
    }
    if !total.is_zero() {
        tx.note("swap", format!("Rollover {day}: swap {} {}", total.normalize(), acc.ccy()), json!({"day": day.to_string(), "amount": num(total)}));
    }
}


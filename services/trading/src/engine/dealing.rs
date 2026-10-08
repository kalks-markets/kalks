//! Dealer-only position operations (D117): add volume, price correction, charges, void, reopen, A/B book transfer.

use serde_json::{Value, json};

use super::trade::{DealerCtx, gate, market_open};
use super::{Env, Reject, Tx, metrics, pnl, total_margin};
use crate::model::{Book, Deal, DealEntry, DealReason, Position, RouteEvent, TxnKind, acct_code, house_code};
use crate::money::{D, ZERO, num, r2, rdp};
use crate::state::Event;

/// Dealer position tools that only make sense for CFDs (options: the options desk routes).
fn not_option(p: &Position) -> Result<(), Reject> {
    if p.option.is_some() {
        return Err(Reject::new("not_supported", format!("#{} is an option position: this tool is for CFD positions", p.ticket)));
    }
    Ok(())
}

pub fn snap(p: &Position) -> Value {
    json!({"volume": num(p.volume), "openPrice": num(p.open_price), "sl": p.sl.map(num), "tp": p.tp.map(num), "book": p.book.as_str(), "swap": num(p.swap), "commission": num(p.commission)})
}

pub fn add_volume(tx: &mut Tx, env: &Env, ticket: i64, volume: D, dealer: &DealerCtx) -> Result<(Value, Value), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    not_option(&p)?;
    let spec = env.spec(&p.symbol)?.clone();
    if let Some(e) = spec.volume_error(volume).or_else(|| spec.volume_error(p.volume + volume)) {
        return Err(Reject::new("invalid_volume", e));
    }
    super::trade::cfd_product_gate(env, true)?;
    gate(env, &tx.st, &p.symbol, true, p.volume + volume, Some(dealer))?;
    market_open(env, &spec)?;
    let q = env.live_quote(&tx.st.account, &p.symbol)?;
    let px = q.open_price(p.side);
    let comm = r2(env.commission_per_lot(&spec) * volume * tx.st.account.usd_factor());
    let before_m = total_margin(env, &tx.st, None);
    let after_m = total_margin(env, &tx.st, Some((&p.symbol, p.side, volume)));
    let m = metrics(env, &tx.st);
    if after_m > before_m && m.equity - comm - after_m < ZERO {
        return Err(Reject::new("no_money", format!("Insufficient free margin: needs {}, free {}", r2(after_m - before_m).normalize(), r2(m.free_margin).normalize())));
    }
    let total = p.volume + volume;
    let avg = rdp((p.open_price * p.volume + px * volume) / total, spec.digits + 3);
    let book_avg = rdp((p.book_price * p.volume + px * volume) / total, spec.digits + 3);
    let deal_id = env.ids.deal();
    let txn = if comm.is_zero() { None } else { tx.post(env, TxnKind::Commission, format!("deal:{deal_id}:commission"), "balance", "commission", -comm, Some(format!("deal:{deal_id}")), Some(dealer.reason_code.clone()), None) };
    let np = Position { volume: total, open_price: avg, book_price: book_avg, commission: p.commission + comm, ..p.clone() };
    let deal = Deal {
        id: deal_id,
        login: p.login,
        position_ticket: ticket,
        order_ticket: None,
        symbol: p.symbol.clone(),
        side: p.side,
        position_side: p.side,
        entry: DealEntry::In,
        volume,
        price: px,
        profit: ZERO,
        swap: ZERO,
        commission: comm,
        reason: DealReason::Dealer,
        book: p.book,
        time: env.now,
        open_price: px,
        open_time: env.now,
        source: crate::model::Source::Dealer,
        comment: "add volume".into(),
        price_correction: false,
        ledger_txn: txn,
        staff: Some(dealer.staff.clone()),
        reason_code: Some(dealer.reason_code.clone()),
        snapshot: None,
        client_order_id: None,
        partial: false,
        option: None,
    };
    tx.emit(Event::PositionUpdated { position: np, change: "volume_added".into(), deal: Some(deal) });
    Ok((json!({"volume": num(p.volume), "openPrice": num(p.open_price)}), json!({"volume": num(total), "openPrice": num(avg), "addedAt": num(px), "added": num(volume)})))
}

/// Error correction of the open price (flagged "price correction" on the client statement).
pub fn price_correction(tx: &mut Tx, env: &Env, ticket: i64, open_price: D) -> Result<(Value, Value), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    not_option(&p)?;
    let spec = env.spec(&p.symbol)?;
    if open_price <= ZERO {
        return Err(Reject::new("invalid_price", "Enter the corrected open price"));
    }
    let px = spec.round_price(open_price);
    if px == p.open_price {
        return Err(Reject::new("no_change", "Price is unchanged"));
    }
    if (px / p.open_price - D::ONE).abs() > D::new(5, 2) {
        return Err(Reject::new("off_market", format!("Corrected price {} is more than 5% away from the recorded open price {} — check the value", px.normalize(), p.open_price.normalize())));
    }
    let never_moved = p.book_price == p.open_price;
    let np = Position { open_price: px, book_price: if never_moved { px } else { p.book_price }, price_corrected: true, ..p.clone() };
    tx.emit(Event::PositionUpdated { position: np, change: "price_correction".into(), deal: None });
    Ok((json!({"openPrice": num(p.open_price)}), json!({"openPrice": num(px)})))
}

/// Sets the accrued swap and/or the commission of an open position. Commission is already on the balance,
/// so a commission change books the difference; swap is still unrealised and just changes.
pub fn adjust_charges(tx: &mut Tx, env: &Env, ticket: i64, swap: Option<D>, commission: Option<D>, dealer: &DealerCtx) -> Result<(Value, Value), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    not_option(&p)?;
    let sw = r2(swap.unwrap_or(p.swap));
    let cm = r2(commission.unwrap_or(p.commission));
    if cm < ZERO {
        return Err(Reject::new("invalid_amount", "Commission cannot be negative"));
    }
    if sw == p.swap && cm == p.commission {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    let diff = cm - p.commission;
    if !diff.is_zero() {
        let key = format!("charges:{ticket}:{}", tx.st.version);
        tx.post(env, TxnKind::Commission, key, "balance", "commission", -diff, Some(format!("position:{ticket}")), Some(dealer.reason_code.clone()), Some("commission adjustment".into()));
    }
    let np = Position { swap: sw, commission: cm, ..p.clone() };
    tx.emit(Event::PositionUpdated { position: np, change: "charges".into(), deal: None });
    Ok((json!({"swap": num(p.swap), "commission": num(p.commission)}), json!({"swap": num(sw), "commission": num(cm)})))
}

/// Removes a position as if it never existed: no P&L is booked and the entry commission is refunded.
pub fn void_position(tx: &mut Tx, env: &Env, ticket: i64, dealer: &DealerCtx) -> Result<Value, Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    if p.option.is_some() {
        return Err(Reject::new("not_supported", "Void option trades through the options desk (POST /v1/admin/options/trades/{ticket}/void): it reverses the premium too"));
    }
    if !p.commission.is_zero() {
        tx.post(env, TxnKind::Reversal, format!("void:{ticket}"), "balance", "commission", p.commission, Some(format!("position:{ticket}")), Some(dealer.reason_code.clone()), Some("void: commission refund".into()));
    }
    tx.emit(Event::PositionRemoved { ticket, reason: format!("void by {}", dealer.staff) });
    Ok(snap(&p))
}

/// Reverses a closing deal: the booked P&L is taken back and the closed volume is open again.
pub fn reopen_deal(tx: &mut Tx, env: &Env, deal: &Deal, dealer: &DealerCtx) -> Result<(Value, Value), Reject> {
    if deal.option.is_some() {
        return Err(Reject::new("not_supported", "Option deals can't be reopened; void the trade or re-run the settlement instead"));
    }
    if tx.st.reversed_deals.contains(&deal.id) {
        return Err(Reject::new("already_reversed", format!("Deal {} was already reversed", deal.id)));
    }
    if deal.entry == DealEntry::In {
        return Err(Reject::new("invalid_deal", "Only closing deals can be reopened"));
    }
    let snapshot = deal.snapshot.as_deref().cloned().ok_or_else(|| Reject::new("invalid_deal", "This deal has no position snapshot"))?;
    let booked = deal.profit + deal.swap;
    if !booked.is_zero() {
        let ccy = tx.st.account.ccy();
        let login = tx.st.account.login;
        tx.post_legs(env, TxnKind::Reversal, format!("reopen:deal:{}", deal.id), vec![(acct_code(login, "balance"), -booked), (house_code("trading_pnl", ccy), deal.profit), (house_code("swap", ccy), deal.swap)], Some(format!("deal:{}", deal.id)));
    }
    let merged = match tx.st.positions.get(&deal.position_ticket).cloned() {
        Some(open) => {
            let total = open.volume + snapshot.volume;
            let spec = env.spec(&open.symbol)?;
            let avg = rdp((open.open_price * open.volume + snapshot.open_price * snapshot.volume) / total, spec.digits + 3);
            Position { volume: total, open_price: avg, swap: open.swap + snapshot.swap, commission: open.commission + snapshot.commission, book_carry_a: open.book_carry_a + snapshot.book_carry_a, book_carry_b: open.book_carry_b + snapshot.book_carry_b, ..open }
        }
        None => snapshot,
    };
    tx.emit(Event::DealReversed { deal_id: deal.id });
    tx.emit(Event::PositionUpdated { position: merged, change: format!("reopened deal {}", deal.id), deal: None });
    let _ = dealer;
    Ok((json!({"deal": deal.id, "closedVolume": num(deal.volume), "closePrice": num(deal.price), "profit": num(booked)}), json!({"reopenedVolume": num(deal.volume), "balanceReversal": num(-booked)})))
}

pub enum BookMove {
    Full,
    Volume(D),
    Pct(D),
}

/// A/B book transfer (D2/D140): full (the ticket changes book) or partial (the moved volume is split off into
/// a child ticket linked to its parent). Client P&L is unaffected; the book attribution restarts at the
/// transfer price.
pub fn transfer_book(tx: &mut Tx, env: &Env, ticket: i64, to: Book, mv: BookMove, dealer: &DealerCtx) -> Result<(Option<i64>, Value, Value, bool), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    not_option(&p)?;
    if p.book == to {
        return Err(Reject::new("same_book", format!("already on {}-book", to.as_str())));
    }
    let spec = env.spec(&p.symbol)?.clone();
    let q = env.live_quote(&tx.st.account, &p.symbol)?;
    let steps = |v: D| (v / spec.lot_step).floor() * spec.lot_step;
    let mut mv_vol = match mv {
        BookMove::Full => p.volume,
        BookMove::Volume(v) => steps(v),
        BookMove::Pct(pc) => steps(p.volume * pc / D::ONE_HUNDRED),
    }
    .min(p.volume);
    if mv_vol < spec.lot_min {
        return Err(Reject::new("invalid_volume", format!("volume to move below minimum {}", spec.lot_min.normalize())));
    }
    let rest = p.volume - mv_vol;
    if rest > ZERO && rest < spec.lot_min {
        mv_vol = p.volume; // remainder too small to keep: move everything
    }
    let px = q.close_price(p.side);
    let from = p.book;
    let acc = tx.st.account.clone();
    let staff = dealer.staff.clone();
    let carry = |c: &mut Position, vol: D, from: Book| {
        let seg = pnl(env, &acc, &spec, c.side, vol, c.book_price, px);
        match from {
            Book::A => c.book_carry_a += seg,
            Book::B => c.book_carry_b += seg,
        }
        seg
    };
    if mv_vol == p.volume {
        let mut np = p.clone();
        let seg = carry(&mut np, p.volume, from);
        np.book = to;
        np.book_since = env.now;
        np.book_price = px;
        np.route_history.push(RouteEvent { at: env.now, kind: "transfer".into(), from: Some(from), to, volume: p.volume, price: px, staff, reason: dealer.reason_code.clone(), related_ticket: None });
        tx.emit(Event::PositionUpdated { position: np, change: format!("book {} → {}", from.as_str(), to.as_str()), deal: None });
        return Ok((None, json!({"book": from.as_str(), "volume": num(p.volume)}), json!({"book": to.as_str(), "volume": num(p.volume), "transferPrice": num(px), "pnlOnPreviousBook": num(r2(seg))}), false));
    }
    let frac = mv_vol / p.volume;
    let keep = D::ONE - frac;
    let child_ticket = env.ids.ticket();
    let c_swap = r2(p.swap * frac);
    let c_comm = r2(p.commission * frac);
    let mut child = Position {
        ticket: child_ticket,
        volume: mv_vol,
        swap: c_swap,
        commission: c_comm,
        book: to,
        parent_ticket: Some(ticket),
        child_tickets: vec![],
        book_since: env.now,
        book_carry_a: p.book_carry_a * frac,
        book_carry_b: p.book_carry_b * frac,
        ..p.clone()
    };
    // the moved volume's P&L so far belongs to the old book
    let tmp_price = child.book_price;
    carry(&mut child, mv_vol, from);
    child.book_price = px;
    let _ = tmp_price;
    child.route_history.push(RouteEvent { at: env.now, kind: "split-in".into(), from: Some(from), to, volume: mv_vol, price: px, staff: staff.clone(), reason: dealer.reason_code.clone(), related_ticket: Some(ticket) });
    let mut parent = Position { volume: rest, swap: p.swap - c_swap, commission: p.commission - c_comm, book_carry_a: p.book_carry_a * keep, book_carry_b: p.book_carry_b * keep, ..p.clone() };
    parent.child_tickets.push(child_ticket);
    parent.route_history.push(RouteEvent { at: env.now, kind: "split-out".into(), from: Some(from), to, volume: mv_vol, price: px, staff, reason: dealer.reason_code.clone(), related_ticket: Some(child_ticket) });
    tx.emit(Event::PositionUpdated { position: parent, change: format!("split {} lots to #{child_ticket}", mv_vol.normalize()), deal: None });
    tx.emit(Event::PositionOpened { position: child, deal: None });
    let mut after = serde_json::Map::new();
    after.insert(ticket.to_string(), json!(format!("{} lots on {}-book", rest.normalize(), from.as_str())));
    after.insert(child_ticket.to_string(), json!(format!("{} lots on {}-book", mv_vol.normalize(), to.as_str())));
    after.insert("transferPrice".into(), num(px));
    after.insert("openPrice".into(), num(p.open_price));
    Ok((Some(child_ticket), json!({"ticket": ticket.to_string(), "book": from.as_str(), "volume": num(p.volume)}), Value::Object(after), true))
}

/// Dealer fill of a pending order at the current market price.
pub fn fill_order(tx: &mut Tx, env: &Env, ticket: i64, dealer: &DealerCtx) -> Result<(Option<i64>, D, Book), Reject> {
    use super::trade::{Fill, fill, is_opening};
    let o = tx.st.orders.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Order #{ticket} not found")))?;
    if o.option.is_some() {
        return Err(Reject::new("not_supported", "Option orders fill at the model price when their limit or trigger is reached"));
    }
    let spec = env.spec(&o.symbol)?.clone();
    super::trade::cfd_product_gate(env, is_opening(&tx.st, &o.symbol, o.side, o.volume))?;
    gate(env, &tx.st, &o.symbol, is_opening(&tx.st, &o.symbol, o.side, o.volume), o.volume, Some(dealer))?;
    market_open(env, &spec)?;
    let q = env.live_quote(&tx.st.account, &o.symbol)?;
    let price = q.open_price(o.side);
    let out = fill(
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
            comment: format!("Filled from order #{ticket}"),
            order_ticket: o.ticket,
            book: o.book,
            client_order_id: None,
            reason: DealReason::Dealer,
            staff: Some(dealer.staff.clone()),
            reason_code: Some(dealer.reason_code.clone()),
            check_margin: true,
        },
    )?;
    tx.emit(Event::OrderRemoved { ticket, status: crate::model::OrderStatus::Filled, reason: format!("dealer fill by {}", dealer.staff), at: env.now, fill_price: Some(price), position_ticket: out.position_ticket });
    if let Some(partner) = o.oco
        && tx.st.orders.contains_key(&partner)
    {
        tx.emit(Event::OrderRemoved { ticket: partner, status: crate::model::OrderStatus::Cancelled, reason: format!("OCO: #{ticket} filled"), at: env.now, fill_price: None, position_ticket: None });
    }
    Ok((out.position_ticket, price, out.book))
}

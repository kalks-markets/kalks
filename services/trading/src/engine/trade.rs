//! Orders and positions: placing, filling (netting / hedging), closing, modifying, Close By, bulk close.

use chrono::{DateTime, Utc};
use serde_json::json;

use super::{Env, Reject, Tx, pnl, total_margin};
use crate::model::{AccountKind, Book, Deal, DealEntry, DealReason, Expiry, Mode, Order, OrderStatus, OrderType, Position, RouteEvent, Side, Source, Status, Trailing, TxnKind, acct_code, house_code};
use crate::money::{D, ZERO, r2, rdp};
use crate::rules::{ControlMode, Product, RouteCtx, resolve_route};
use crate::specs::{Spec, end_of_server_day};
use crate::state::{AccountState, Event};

/// Who is acting. Dealers bypass client-only checks (market-closed with manual price, account status on close).
#[derive(Clone, Debug, Default)]
pub struct DealerCtx {
    pub staff: String,
    pub reason_code: String,
    pub force: bool,
}

#[derive(Clone, Debug)]
pub struct OrderReq {
    pub symbol: String,
    pub side: Side,
    pub kind: OrderType,
    pub volume: D,
    /// Pending: order / stop price. Market + dealer: manual fill price.
    pub price: Option<D>,
    pub stop_limit: Option<D>,
    pub sl: Option<D>,
    pub tp: Option<D>,
    pub trailing_points: Option<i64>,
    pub expiry: Expiry,
    pub expiry_at: Option<DateTime<Utc>>,
    /// D106: max slippage in points against `requested_price`; exceeded → requote.
    pub deviation_points: Option<i64>,
    pub requested_price: Option<D>,
    pub oco_with: Option<i64>,
    pub source: Source,
    pub platform: String,
    pub comment: String,
    pub client_order_id: Option<String>,
    /// Dealer override of the routing decision.
    pub book: Option<Book>,
    pub dealer: Option<DealerCtx>,
}

impl OrderReq {
    pub fn market(symbol: &str, side: Side, volume: D) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            kind: OrderType::Market,
            volume,
            price: None,
            stop_limit: None,
            sl: None,
            tp: None,
            trailing_points: None,
            expiry: Expiry::Gtc,
            expiry_at: None,
            deviation_points: None,
            requested_price: None,
            oco_with: None,
            source: Source::Manual,
            platform: "Web".into(),
            comment: String::new(),
            client_order_id: None,
            book: None,
            dealer: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlaceResult {
    Filled { order_ticket: i64, position_ticket: Option<i64>, price: D, book: Book, deals: Vec<i64> },
    Pending { ticket: i64, price: D, book: Option<Book> },
    /// Same `client_order_id` was already processed: nothing new was done.
    Duplicate { ticket: i64 },
}

/* ------------------------------------------------------------------ */
/* Gates                                                               */
/* ------------------------------------------------------------------ */

/// Would a trade of `side`/`volume` on `symbol` add exposure? (Netting: reducing an opposite position doesn't.)
pub fn is_opening(st: &AccountState, symbol: &str, side: Side, volume: D) -> bool {
    if st.account.mode == Mode::Hedging {
        return true;
    }
    match st.position_for(symbol) {
        Some(p) => p.side == side || volume > p.volume,
        None => true,
    }
}

/// Refusal of CFD trading on an Options account (CFD / Options account split).
pub const OPTIONS_ACCOUNT: &str = "This is an Options account: CFDs trade in a CFD account";

/// CFD / Options account split: an account trades the product of its group. New CFD exposure on an Options account
/// is refused for everyone, dealers included (`product_mismatch`); closes, SL / TP and stop-out are never refused.
pub fn cfd_product_gate(env: &Env, opening: bool) -> Result<(), Reject> {
    if opening && env.group.product == Product::Options {
        return Err(Reject::new("product_mismatch", OPTIONS_ACCOUNT));
    }
    Ok(())
}

/// Account / dealer-control / symbol-control checks (D115, D140).
pub fn gate(env: &Env, st: &AccountState, symbol: &str, opening: bool, volume: D, dealer: Option<&DealerCtx>) -> Result<(), Reject> {
    let acc = &st.account;
    let c = &acc.controls;
    let login = acc.login;
    // client restrictions set in the Back Office (controls.rs): the client can't trade / open; dealers still can
    if dealer.is_none()
        && let Some(r) = env.restrictions
        && let Some(rej) = crate::controls::trading_reject(r, acc.user_id, opening, env.now)
    {
        return Err(rej);
    }
    if opening {
        match acc.status {
            Status::Active => {}
            Status::CloseOnly => return Err(Reject::new("close_only", format!("Account {login} is close-only — new positions are rejected"))),
            s => return Err(Reject::new("account_status", format!("Account {login} is {} — trading not allowed", s.as_str()))),
        }
        if c.trading_disabled {
            return Err(Reject::new("trading_disabled", format!("Trading is disabled on {login} (dealer control)")));
        }
        if c.close_only {
            return Err(Reject::new("close_only", format!("{login} is close-only — new positions are rejected")));
        }
        if let Some(max) = c.max_lot
            && max > ZERO
            && volume > max
        {
            return Err(Reject::new("max_lot", format!("Volume {} exceeds the account max lot {}", volume.normalize(), max.normalize())));
        }
    } else if dealer.is_none() {
        match acc.status {
            Status::Active | Status::CloseOnly => {}
            s => return Err(Reject::new("account_status", format!("Account {login} is {} — trading not allowed", s.as_str()))),
        }
        if c.trading_disabled {
            return Err(Reject::new("trading_disabled", format!("Trading is disabled on {login} (dealer control)")));
        }
    }
    if let Some(sc) = env.tenant.symbol_control(symbol, &acc.group) {
        let scope = if sc.group == "all" { String::new() } else { format!(" for {}", sc.group) };
        match sc.mode {
            ControlMode::Halt if !dealer.is_some_and(|d| d.force) => {
                return Err(Reject::new("symbol_halted", format!("{symbol} is halted{scope} — trading suspended")));
            }
            ControlMode::CloseOnly if opening => {
                return Err(Reject::new("symbol_close_only", format!("{symbol} is close-only{scope} — new positions are rejected")));
            }
            _ => {}
        }
    }
    // a split / dividend due on this symbol and not yet applied to the account: nothing trades meanwhile (prices are
    // already post-action), dealers included
    if let Some(id) = env.corp_pending(st, symbol) {
        return Err(Reject::new("corporate_action", format!("{symbol}: a corporate action (#{id}) is being applied; trading resumes in a moment")));
    }
    if opening && let Some(spec) = env.specs.get(symbol) {
        // catalogue instruments open on live accounts only once the platform switched live trading on for them
        // (specs.rs `Spec::live`; the Back Office symbol settings). Closing is always allowed; no dealer override.
        if acc.kind != AccountKind::Demo && !spec.live {
            return Err(Reject::new("symbol_demo_only", format!("{symbol} is available on demo accounts only for now: live trading on it is not enabled yet")));
        }
        // a catalogue instrument's profit currency must convert to USD with a live price (margin and P&L), never
        // be taken as USD for want of one
        if !spec.core && spec.quote_ccy != "USD" {
            let own = env.quote(acc, symbol).map(|q| q.mid()).unwrap_or(ZERO);
            if env.to_usd(acc, &spec.quote_ccy, D::ONE, (symbol, own)).is_none() {
                return Err(Reject::new("no_conversion", format!("No {}/USD price yet to value {symbol}; try again in a moment", spec.quote_ccy)));
            }
        }
    }
    Ok(())
}

pub fn market_open(env: &Env, spec: &Spec) -> Result<(), Reject> {
    if spec.is_open(env.now) { Ok(()) } else { Err(Reject::new("market_closed", format!("Market is closed for {}", spec.symbol))) }
}

/// SL/TP relative to `reference` (the price the position would close at, or the pending entry price).
pub fn check_sltp(spec: &Spec, side: Side, reference: D, sl: Option<D>, tp: Option<D>) -> Result<(), Reject> {
    let gap = D::from(spec.stops_level_points) * spec.point;
    let r = spec.round_price(reference);
    if let Some(sl) = sl {
        let bad = match side {
            Side::Buy => sl > r - gap || sl >= r,
            Side::Sell => sl < r + gap || sl <= r,
        };
        if bad || sl <= ZERO {
            return Err(Reject::new("invalid_sl", format!("Stop loss must be {} {}", if side == Side::Buy { "below" } else { "above" }, r.normalize())));
        }
    }
    if let Some(tp) = tp {
        let bad = match side {
            Side::Buy => tp < r + gap || tp <= r,
            Side::Sell => tp > r - gap || tp >= r,
        };
        if bad || tp <= ZERO {
            return Err(Reject::new("invalid_tp", format!("Take profit must be {} {}", if side == Side::Buy { "above" } else { "below" }, r.normalize())));
        }
    }
    Ok(())
}

pub fn trailing(spec: &Spec, points: Option<i64>) -> Result<Option<Trailing>, Reject> {
    match points {
        None | Some(0) => Ok(None),
        Some(p) if p < 0 || p < spec.stops_level_points || p > 1_000_000 => Err(Reject::new("invalid_trailing", "Trailing stop distance is out of range")),
        Some(p) => Ok(Some(Trailing { distance_points: p, step_points: (p / 10).max(1) })),
    }
}

fn round_opt(spec: &Spec, v: Option<D>) -> Option<D> {
    v.filter(|x| !x.is_zero()).map(|x| spec.round_price(x))
}

/// Routing decision (D2/D140): dealer override → rules → account default → group route.
pub fn route_for(env: &Env, st: &AccountState, symbol: &str, volume: D, over: Option<Book>) -> (Book, String) {
    if let Some(b) = over {
        return (b, "Dealer override".into());
    }
    let fallback = st.account.route_override.unwrap_or(env.group.route);
    let ctx = RouteCtx { login: st.account.login, group: &st.account.group, symbol, volume };
    let (book, rule) = resolve_route(&env.tenant.routing_rules, &ctx, fallback);
    (book, rule.unwrap_or_else(|| if st.account.route_override.is_some() { "Account route".into() } else { "Default route".into() }))
}

/* ------------------------------------------------------------------ */
/* Place                                                               */
/* ------------------------------------------------------------------ */

pub fn place_order(tx: &mut Tx, env: &Env, req: OrderReq) -> Result<PlaceResult, Reject> {
    if let Some(cid) = &req.client_order_id {
        if cid.is_empty() || cid.len() > 64 {
            return Err(Reject::new("invalid_client_order_id", "clientOrderId must be 1–64 characters"));
        }
        if let Some(t) = tx.st.client_ids.get(cid) {
            return Ok(PlaceResult::Duplicate { ticket: *t });
        }
    }
    let spec = env.spec(&req.symbol)?.clone();
    if let Some(e) = spec.volume_error(req.volume) {
        return Err(Reject::new("invalid_volume", e));
    }
    let dealer = req.dealer.as_ref();
    let manual = dealer.is_some() && req.kind == OrderType::Market && req.price.is_some_and(|p| p > ZERO);
    let opening = req.kind != OrderType::Market || is_opening(&tx.st, &req.symbol, req.side, req.volume);
    cfd_product_gate(env, opening)?;
    gate(env, &tx.st, &req.symbol, opening, req.volume, dealer)?;
    if !manual {
        market_open(env, &spec)?;
    }
    let acc = tx.st.account.clone();
    let q = match env.live_quote(&acc, &req.symbol) {
        Ok(q) => Some(q),
        Err(e) if !manual => return Err(e),
        Err(_) => None,
    };

    if req.kind == OrderType::Market {
        let q = q.as_ref();
        let market_px = q.map(|q| q.open_price(req.side));
        let price = if manual {
            let px = spec.round_price(req.price.unwrap_or_default());
            if let Some(m) = market_px
                && (px / m - D::ONE).abs() > D::new(5, 2)
            {
                return Err(Reject::new("off_market", format!("Manual price {} is more than 5% away from the market ({})", px.normalize(), m.normalize())));
            }
            px
        } else {
            market_px.ok_or_else(|| Reject::new("no_price", format!("No price for {}", req.symbol)))?
        };
        if let (Some(dev), Some(want)) = (req.deviation_points, req.requested_price)
            && !manual
            && (price - want).abs() > D::from(dev.max(0)) * spec.point
        {
            let q = q.unwrap();
            return Err(Reject { code: "requote", message: format!("Price changed to {} (max deviation {} points)", price.normalize(), dev), requote: Some((q.bid, q.ask)) });
        }
        let sl = round_opt(&spec, req.sl);
        let tp = round_opt(&spec, req.tp);
        // the resulting position closes at the bid (buy) / ask (sell)
        let reference = q.map(|q| q.close_price(req.side)).unwrap_or(price);
        let resulting_side = match (tx.st.account.mode, tx.st.position_for(&req.symbol)) {
            (Mode::Netting, Some(p)) if p.side != req.side && req.volume <= p.volume => None, // pure reduction: SL/TP stay
            _ => Some(req.side),
        };
        if let Some(side) = resulting_side {
            check_sltp(&spec, side, reference, sl, tp)?;
        }
        let tr = trailing(&spec, req.trailing_points)?;
        let order_ticket = env.ids.ticket();
        let out = fill(
            tx,
            env,
            Fill {
                symbol: req.symbol.clone(),
                side: req.side,
                volume: req.volume,
                price,
                sl,
                tp,
                trailing: tr,
                source: req.source,
                platform: req.platform.clone(),
                comment: req.comment.clone(),
                order_ticket,
                book: req.book,
                client_order_id: req.client_order_id.clone(),
                reason: if dealer.is_some() { DealReason::Dealer } else { DealReason::Client },
                staff: dealer.map(|d| d.staff.clone()),
                reason_code: dealer.map(|d| d.reason_code.clone()),
                check_margin: true,
            },
        )?;
        tx.note("fill", format!("{} {} {} filled at {}", req.side.as_str(), req.volume.normalize(), req.symbol, price.normalize()), json!({"orderTicket": order_ticket, "positionTicket": out.position_ticket, "symbol": req.symbol, "side": req.side.as_str(), "price": crate::money::num(price)}));
        return Ok(PlaceResult::Filled { order_ticket, position_ticket: out.position_ticket, price, book: out.book, deals: out.deals });
    }

    // ---------- pending ----------
    let q = q.ok_or_else(|| Reject::new("no_price", format!("No price for {}", req.symbol)))?;
    let price = spec.round_price(req.price.filter(|p| *p > ZERO).ok_or_else(|| Reject::new("invalid_price", "Enter an order price"))?);
    let mkt = q.open_price(req.side);
    let gap = D::from(spec.stops_level_points) * spec.point;
    let buy = req.side == Side::Buy;
    match req.kind {
        OrderType::Limit => {
            if if buy { price >= mkt - gap } else { price <= mkt + gap } {
                return Err(Reject::new("invalid_price", format!("{} limit must be {} the market ({})", if buy { "Buy" } else { "Sell" }, if buy { "below" } else { "above" }, mkt.normalize())));
            }
        }
        OrderType::Stop | OrderType::StopLimit => {
            if if buy { price <= mkt + gap } else { price >= mkt - gap } {
                return Err(Reject::new("invalid_price", format!("{} stop must be {} the market ({})", if buy { "Buy" } else { "Sell" }, if buy { "above" } else { "below" }, mkt.normalize())));
            }
        }
        OrderType::Market => unreachable!(),
    }
    let stop_limit = if req.kind == OrderType::StopLimit {
        let l = spec.round_price(req.stop_limit.filter(|p| *p > ZERO).ok_or_else(|| Reject::new("invalid_price", "Enter the limit price of the stop-limit order"))?);
        if if buy { l > price } else { l < price } {
            return Err(Reject::new("invalid_price", format!("Limit price must be {} the stop price", if buy { "at or below" } else { "at or above" })));
        }
        Some(l)
    } else {
        None
    };
    let entry = stop_limit.unwrap_or(price);
    let sl = round_opt(&spec, req.sl);
    let tp = round_opt(&spec, req.tp);
    check_sltp(&spec, req.side, entry, sl, tp)?;
    let tr = trailing(&spec, req.trailing_points)?;
    let expiry_at = match req.expiry {
        Expiry::Gtc => None,
        Expiry::Today => Some(end_of_server_day(env.now)),
        Expiry::Date => {
            let at = req.expiry_at.ok_or_else(|| Reject::new("invalid_expiry", "Enter the expiry date"))?;
            if at <= env.now {
                return Err(Reject::new("invalid_expiry", "Expiry must be in the future"));
            }
            Some(at)
        }
    };
    if let Some(o) = req.oco_with {
        let other = tx.st.orders.get(&o).ok_or_else(|| Reject::new("not_found", format!("Order #{o} not found")))?;
        if other.oco.is_some() {
            return Err(Reject::new("invalid_oco", format!("Order #{o} is already part of an OCO pair")));
        }
    }
    let ticket = env.ids.ticket();
    let order = Order {
        ticket,
        login: tx.st.account.login,
        symbol: req.symbol.clone(),
        side: req.side,
        kind: req.kind,
        volume: req.volume,
        price,
        stop_limit,
        sl,
        tp,
        trailing: tr,
        expiry: req.expiry,
        expiry_at,
        oco: req.oco_with,
        source: req.source,
        platform: req.platform.clone(),
        comment: req.comment.clone(),
        book: req.book,
        placed_at: env.now,
        triggered: false,
        client_order_id: req.client_order_id.clone(),
        option: None,
        combo_id: None,
        trigger: None,
    };
    tx.emit(Event::OrderPlaced { order });
    if let Some(o) = req.oco_with {
        let mut other = tx.st.orders[&o].clone();
        other.oco = Some(ticket);
        tx.emit(Event::OrderUpdated { order: other, change: "oco_linked".into() });
    }
    Ok(PlaceResult::Pending { ticket, price, book: req.book })
}

/* ------------------------------------------------------------------ */
/* Fill                                                                */
/* ------------------------------------------------------------------ */

pub struct Fill {
    pub symbol: String,
    pub side: Side,
    pub volume: D,
    pub price: D,
    pub sl: Option<D>,
    pub tp: Option<D>,
    pub trailing: Option<Trailing>,
    pub source: Source,
    pub platform: String,
    pub comment: String,
    pub order_ticket: i64,
    pub book: Option<Book>,
    pub client_order_id: Option<String>,
    pub reason: DealReason,
    pub staff: Option<String>,
    pub reason_code: Option<String>,
    pub check_margin: bool,
}

#[derive(Debug)]
pub struct FillOut {
    pub position_ticket: Option<i64>,
    pub book: Book,
    pub deals: Vec<i64>,
}

/// Commission (account currency, rounded) for opening `volume` lots.
fn commission(env: &Env, st: &AccountState, spec: &Spec, volume: D) -> D {
    r2(env.commission_per_lot(spec) * volume * st.account.usd_factor())
}

/// Executes a trade at `f.price`: hedging opens a new position; netting opens, adds to, reduces, closes or
/// reverses (D18) the single position on the symbol.
pub fn fill(tx: &mut Tx, env: &Env, f: Fill) -> Result<FillOut, Reject> {
    let spec = env.spec(&f.symbol)?.clone();
    let existing = if tx.st.account.mode == Mode::Netting { tx.st.position_for(&f.symbol).cloned() } else { None };
    let opens = match &existing {
        Some(p) if p.side != f.side => (f.volume - p.volume).max(ZERO),
        _ => f.volume,
    };
    let comm = commission(env, &tx.st, &spec, opens);
    if f.check_margin && opens > ZERO {
        let before = total_margin(env, &tx.st, None);
        let after = total_margin(env, &tx.st, Some((&f.symbol, f.side, f.volume)));
        let m = super::metrics(env, &tx.st);
        if after > before && m.equity - comm - after - m.order_reserve < ZERO {
            return Err(Reject::new("no_money", format!("Not enough money: margin {} {} needed, free margin {} {}", r2(after - before).normalize(), tx.st.account.ccy(), r2(m.free_margin).normalize(), tx.st.account.ccy())));
        }
    }
    let (book, route_reason) = route_for(env, &tx.st, &f.symbol, f.volume, f.book);
    let mut deals = Vec::new();

    match existing {
        Some(p) if p.side != f.side => {
            // reduce / close / reverse
            let close_vol = f.volume.min(p.volume);
            let meta = CloseMeta { reason: f.reason, entry: DealEntry::Out, source: f.source, comment: f.comment.clone(), staff: f.staff.clone(), reason_code: f.reason_code.clone(), price_correction: false, order_ticket: Some(f.order_ticket), client_order_id: f.client_order_id.clone() };
            let (deal_id, _) = close_part(tx, env, p.ticket, close_vol, f.price, meta)?;
            deals.push(deal_id);
            if opens > ZERO {
                let (ticket, d) = open_position(tx, env, &spec, &f, opens, comm, book, &route_reason, Some(p.ticket));
                deals.push(d);
                apply_nbp(tx, env);
                return Ok(FillOut { position_ticket: Some(ticket), book, deals });
            }
            apply_nbp(tx, env);
            let still = tx.st.positions.get(&p.ticket).map(|x| x.ticket);
            Ok(FillOut { position_ticket: still, book: p.book, deals })
        }
        Some(p) => {
            // add to the netting position (volume-weighted average price)
            let total = p.volume + f.volume;
            let avg = rdp((p.open_price * p.volume + f.price * f.volume) / total, spec.digits + 3);
            let book_avg = rdp((p.book_price * p.volume + f.price * f.volume) / total, spec.digits + 3);
            let deal_id = env.ids.deal();
            let txn = charge_commission(tx, env, comm, deal_id);
            let mut np = p.clone();
            np.volume = total;
            np.open_price = avg;
            np.book_price = book_avg;
            np.commission += comm;
            if f.sl.is_some() {
                np.sl = f.sl;
            }
            if f.tp.is_some() {
                np.tp = f.tp;
            }
            if f.trailing.is_some() {
                np.trailing = f.trailing.clone();
            }
            let deal = entry_deal(env, &tx.st, deal_id, &np, &f, f.volume, comm, txn);
            tx.emit(Event::PositionUpdated { position: np, change: "volume_added".into(), deal: Some(deal) });
            deals.push(deal_id);
            Ok(FillOut { position_ticket: Some(p.ticket), book: p.book, deals })
        }
        None => {
            let (ticket, d) = open_position(tx, env, &spec, &f, f.volume, comm, book, &route_reason, None);
            deals.push(d);
            Ok(FillOut { position_ticket: Some(ticket), book, deals })
        }
    }
}

fn charge_commission(tx: &mut Tx, env: &Env, comm: D, deal_id: i64) -> Option<i64> {
    if comm.is_zero() {
        return None;
    }
    tx.post(env, TxnKind::Commission, format!("deal:{deal_id}:commission"), "balance", "commission", -comm, Some(format!("deal:{deal_id}")), None, None)
}

#[allow(clippy::too_many_arguments)]
fn entry_deal(env: &Env, st: &AccountState, id: i64, p: &Position, f: &Fill, volume: D, comm: D, txn: Option<i64>) -> Deal {
    Deal {
        id,
        login: st.account.login,
        position_ticket: p.ticket,
        order_ticket: Some(f.order_ticket),
        symbol: p.symbol.clone(),
        side: f.side,
        position_side: p.side,
        entry: DealEntry::In,
        volume,
        price: f.price,
        profit: ZERO,
        swap: ZERO,
        commission: comm,
        reason: f.reason,
        book: p.book,
        time: env.now,
        open_price: f.price,
        open_time: env.now,
        source: f.source,
        comment: f.comment.clone(),
        price_correction: false,
        ledger_txn: txn,
        staff: f.staff.clone(),
        reason_code: f.reason_code.clone(),
        snapshot: None,
        client_order_id: f.client_order_id.clone(),
        partial: false,
        option: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn open_position(tx: &mut Tx, env: &Env, _spec: &Spec, f: &Fill, volume: D, comm: D, book: Book, route_reason: &str, reversed_from: Option<i64>) -> (i64, i64) {
    // MT5 convention: the position takes the ticket of the order that opened it
    let ticket = if tx.st.positions.contains_key(&f.order_ticket) { env.ids.ticket() } else { f.order_ticket };
    let deal_id = env.ids.deal();
    let txn = charge_commission(tx, env, comm, deal_id);
    let p = Position {
        ticket,
        login: tx.st.account.login,
        symbol: f.symbol.clone(),
        side: f.side,
        volume,
        open_price: f.price,
        open_time: env.now,
        sl: f.sl,
        tp: f.tp,
        trailing: f.trailing.clone(),
        swap: ZERO,
        commission: comm,
        source: f.source,
        platform: f.platform.clone(),
        comment: f.comment.clone(),
        book,
        order_ticket: f.order_ticket,
        parent_ticket: None,
        child_tickets: vec![],
        book_since: env.now,
        book_price: f.price,
        book_carry_a: ZERO,
        book_carry_b: ZERO,
        route_history: vec![RouteEvent { at: env.now, kind: "open".into(), from: None, to: book, volume, price: f.price, staff: f.staff.clone().unwrap_or_else(|| "Routing engine".into()), reason: route_reason.to_string(), related_ticket: None }],
        price_corrected: false,
        last_swap_day: None,
        client_order_id: f.client_order_id.clone(),
        reversed_from,
        option: None,
        combo_id: None,
        premium: ZERO,
        venue: None,
    };
    let deal = entry_deal(env, &tx.st, deal_id, &p, f, volume, comm, txn);
    tx.emit(Event::PositionOpened { position: p, deal: Some(deal) });
    (ticket, deal_id)
}

/* ------------------------------------------------------------------ */
/* Close                                                               */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
pub struct CloseMeta {
    pub reason: DealReason,
    pub entry: DealEntry,
    pub source: Source,
    pub comment: String,
    pub staff: Option<String>,
    pub reason_code: Option<String>,
    pub price_correction: bool,
    pub order_ticket: Option<i64>,
    pub client_order_id: Option<String>,
}

impl CloseMeta {
    pub fn client(reason: DealReason) -> Self {
        Self { reason, entry: DealEntry::Out, source: Source::Manual, comment: String::new(), staff: None, reason_code: None, price_correction: false, order_ticket: None, client_order_id: None }
    }
    pub fn system(reason: DealReason) -> Self {
        Self { source: Source::System, ..Self::client(reason) }
    }
}

/// Closes `volume` of position `ticket` at `price`. Books price P&L + the closed share of the accrued swap
/// on the balance (one ledger transaction), records the exit deal. Returns (deal id, profit).
pub fn close_part(tx: &mut Tx, env: &Env, ticket: i64, volume: D, price: D, meta: CloseMeta) -> Result<(i64, D), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    if p.option.is_some() {
        // option exits are priced by the options pricer (engine::options::close), never at a CFD price
        return Err(Reject::new("not_supported", format!("#{ticket} is an option position")));
    }
    let spec = env.spec(&p.symbol)?.clone();
    let acc = tx.st.account.clone();
    let volume = volume.min(p.volume);
    let full = volume == p.volume;
    let frac = volume / p.volume;
    let profit = r2(pnl(env, &acc, &spec, p.side, volume, p.open_price, price));
    let swap = if full { p.swap } else { r2(p.swap * frac) };
    let comm = if full { p.commission } else { r2(p.commission * frac) };
    let deal_id = env.ids.deal();
    let ccy = acc.ccy();
    let txn = tx.post_legs(
        env,
        TxnKind::TradePnl,
        format!("deal:{deal_id}"),
        vec![(acct_code(acc.login, "balance"), profit + swap), (house_code("trading_pnl", ccy), -profit), (house_code("swap", ccy), -swap)],
        Some(format!("deal:{deal_id}")),
    );
    let snapshot = Position { volume, swap, commission: comm, book_carry_a: p.book_carry_a * frac, book_carry_b: p.book_carry_b * frac, ..p.clone() };
    let deal = Deal {
        id: deal_id,
        login: acc.login,
        position_ticket: ticket,
        order_ticket: meta.order_ticket,
        symbol: p.symbol.clone(),
        side: p.side.opposite(),
        position_side: p.side,
        entry: meta.entry,
        volume,
        price,
        profit,
        swap,
        commission: comm,
        reason: meta.reason,
        book: p.book,
        time: env.now,
        open_price: p.open_price,
        open_time: p.open_time,
        source: meta.source,
        comment: meta.comment,
        price_correction: meta.price_correction,
        ledger_txn: txn,
        staff: meta.staff,
        reason_code: meta.reason_code,
        snapshot: Some(Box::new(snapshot)),
        client_order_id: meta.client_order_id,
        partial: !full,
        option: None,
    };
    let rest = if full {
        None
    } else {
        let keep = D::ONE - frac;
        Some(Position { volume: p.volume - volume, swap: p.swap - swap, commission: p.commission - comm, book_carry_a: p.book_carry_a * keep, book_carry_b: p.book_carry_b * keep, ..p })
    };
    tx.emit(Event::PositionClosed { deal, position: rest });
    Ok((deal_id, profit + swap))
}

/// D16: a negative balance with no open positions is reset to 0 (house absorbs the loss).
pub fn apply_nbp(tx: &mut Tx, env: &Env) {
    if tx.st.positions.is_empty() && tx.st.balance < ZERO {
        let amt = -tx.st.balance;
        let key = format!("nbp:{}:{}", tx.st.account.login, tx.st.version);
        tx.post(env, TxnKind::Nbp, key, "balance", "nbp", amt, None, Some("NBP".into()), Some("Negative balance protection".into()));
        tx.note("nbp", format!("Negative balance of {} {} was reset to 0", amt.normalize(), tx.st.account.ccy()), json!({"amount": crate::money::num(amt)}));
    }
}

#[derive(Clone, Debug, Default)]
pub struct CloseReq {
    /// None = full volume.
    pub volume: Option<D>,
    pub deviation_points: Option<i64>,
    pub requested_price: Option<D>,
    /// Dealer: close at this price (error correction, flagged on the statement).
    pub price: Option<D>,
    pub dealer: Option<DealerCtx>,
    pub stop_out: bool,
}

pub fn close_position(tx: &mut Tx, env: &Env, ticket: i64, req: CloseReq) -> Result<(i64, D), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    if p.option.is_some() {
        if req.price.is_some_and(|x| x > ZERO) {
            return Err(Reject::new("not_supported", "Options are always closed at the model price; void an erroneous option trade instead"));
        }
        let reason = if req.stop_out {
            DealReason::StopOut
        } else if req.dealer.as_ref().is_some_and(|d| d.force) {
            DealReason::Force
        } else if req.dealer.is_some() {
            DealReason::Dealer
        } else {
            DealReason::Client
        };
        let c = super::options::OptClose { volume: req.volume, reason, dealer: req.dealer.clone(), system: req.stop_out, comment: String::new() };
        let out = super::options::close(tx, env, ticket, c)?;
        apply_nbp(tx, env);
        return Ok(out);
    }
    let spec = env.spec(&p.symbol)?.clone();
    let dealer = req.dealer.as_ref();
    gate(env, &tx.st, &p.symbol, false, ZERO, dealer)?;
    let volume = match req.volume {
        None => p.volume,
        Some(v) => {
            if v <= ZERO || v > p.volume {
                return Err(Reject::new("invalid_volume", format!("Volume must be between {} and {}", spec.lot_min.normalize(), p.volume.normalize())));
            }
            if v < p.volume {
                if let Some(e) = spec.volume_error(v) {
                    return Err(Reject::new("invalid_volume", e));
                }
                if p.volume - v < spec.lot_min {
                    return Err(Reject::new("invalid_volume", format!("Remaining volume would be below the minimum {}", spec.lot_min.normalize())));
                }
            }
            v
        }
    };
    let corrected = req.price.is_some_and(|x| x > ZERO) && dealer.is_some();
    let price = if corrected {
        let px = spec.round_price(req.price.unwrap());
        if let Some(q) = env.quote(&tx.st.account, &p.symbol) {
            let m = q.close_price(p.side);
            if (px / m - D::ONE).abs() > D::new(5, 2) {
                return Err(Reject::new("off_market", format!("Close price {} is more than 5% away from the market ({}) — check the value", px.normalize(), m.normalize())));
            }
        }
        px
    } else {
        market_open(env, &spec)?;
        let q = env.live_quote(&tx.st.account, &p.symbol)?;
        let px = q.close_price(p.side);
        if let (Some(dev), Some(want)) = (req.deviation_points, req.requested_price)
            && (px - want).abs() > D::from(dev.max(0)) * spec.point
        {
            return Err(Reject { code: "requote", message: format!("Price changed to {} (max deviation {} points)", px.normalize(), dev), requote: Some((q.bid, q.ask)) });
        }
        px
    };
    let reason = if corrected {
        DealReason::PriceCorrection
    } else if req.stop_out {
        DealReason::StopOut
    } else if dealer.is_some_and(|d| d.force) {
        DealReason::Force
    } else if dealer.is_some() {
        DealReason::Dealer
    } else {
        DealReason::Client
    };
    let meta = CloseMeta {
        reason,
        entry: DealEntry::Out,
        source: if dealer.is_some() { Source::Dealer } else { Source::Manual },
        comment: String::new(),
        staff: dealer.map(|d| d.staff.clone()),
        reason_code: dealer.map(|d| d.reason_code.clone()),
        price_correction: corrected,
        order_ticket: None,
        client_order_id: None,
    };
    let out = close_part(tx, env, ticket, volume, price, meta)?;
    let what = if volume < p.volume { "partially closed" } else { "closed" };
    tx.note("close", format!("#{ticket} {} {} {} {what} at {}", p.side.as_str(), volume.normalize(), p.symbol, price.normalize()), json!({"ticket": ticket, "dealId": out.0, "profit": crate::money::num(out.1)}));
    apply_nbp(tx, env);
    Ok(out)
}

/// Close By (hedging): closes the overlapping volume of two opposite positions against each other at the
/// open price of `by` — no spread is paid on the overlap.
pub fn close_by(tx: &mut Tx, env: &Env, ticket: i64, by: i64) -> Result<Vec<i64>, Reject> {
    if tx.st.account.mode != Mode::Hedging {
        return Err(Reject::new("not_hedging", "Close By is only available on hedging accounts"));
    }
    let a = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    let b = tx.st.positions.get(&by).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{by} not found")))?;
    if a.option.is_some() || b.option.is_some() {
        return Err(Reject::new("not_supported", "Close By is not available for options"));
    }
    if a.symbol != b.symbol || a.side == b.side || ticket == by {
        return Err(Reject::new("invalid_close_by", "Close By needs two opposite positions on the same symbol"));
    }
    let spec = env.spec(&a.symbol)?.clone();
    gate(env, &tx.st, &a.symbol, false, ZERO, None)?;
    market_open(env, &spec)?;
    let v = a.volume.min(b.volume);
    let px = b.open_price;
    let mk = |t: i64| CloseMeta { entry: DealEntry::OutBy, comment: format!("close by #{t}"), ..CloseMeta::client(DealReason::CloseBy) };
    let (d1, _) = close_part(tx, env, ticket, v, px, mk(by))?;
    let (d2, _) = close_part(tx, env, by, v, px, mk(ticket))?;
    tx.note("close_by", format!("#{ticket} closed by #{by}: {} {}", v.normalize(), a.symbol), json!({"ticket": ticket, "by": by, "deals": [d1, d2]}));
    apply_nbp(tx, env);
    Ok(vec![d1, d2])
}

/* ------------------------------------------------------------------ */
/* Modify / cancel                                                     */
/* ------------------------------------------------------------------ */

/// `Some(None)` clears a field, `None` keeps it.
#[derive(Clone, Debug, Default)]
pub struct PositionPatch {
    pub sl: Option<Option<D>>,
    pub tp: Option<Option<D>>,
    pub trailing_points: Option<Option<i64>>,
}

pub fn modify_position(tx: &mut Tx, env: &Env, ticket: i64, patch: PositionPatch, dealer: Option<&DealerCtx>) -> Result<(Position, Position), Reject> {
    let p = tx.st.positions.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Position #{ticket} not found")))?;
    if p.option.is_some() {
        if dealer.is_none() {
            match tx.st.account.status {
                Status::Active | Status::CloseOnly => {}
                s => return Err(Reject::new("account_status", format!("Account {} is {} — trading not allowed", tx.st.account.login, s.as_str()))),
            }
            if let Some(r) = env.restrictions
                && let Some(rej) = crate::controls::trading_reject(r, tx.st.account.user_id, false, env.now)
            {
                return Err(rej);
            }
        }
        return super::options::modify_position(tx, env, ticket, patch.sl, patch.tp, patch.trailing_points);
    }
    let spec = env.spec(&p.symbol)?.clone();
    if dealer.is_none() {
        match tx.st.account.status {
            Status::Active | Status::CloseOnly => {}
            s => return Err(Reject::new("account_status", format!("Account {} is {} — trading not allowed", tx.st.account.login, s.as_str()))),
        }
        // trading disabled in the Back Office: SL / TP can't change either (close-only still may)
        if let Some(r) = env.restrictions
            && let Some(rej) = crate::controls::trading_reject(r, tx.st.account.user_id, false, env.now)
        {
            return Err(rej);
        }
    }
    let q = env.live_quote(&tx.st.account, &p.symbol)?;
    let sl = match patch.sl {
        None => p.sl,
        Some(v) => round_opt(&spec, v),
    };
    let tp = match patch.tp {
        None => p.tp,
        Some(v) => round_opt(&spec, v),
    };
    // only validate levels that changed (a trailing SL already behind the price is fine)
    check_sltp(&spec, p.side, q.close_price(p.side), if sl != p.sl { sl } else { None }, if tp != p.tp { tp } else { None })?;
    let tr = match patch.trailing_points {
        None => p.trailing.clone(),
        Some(v) => trailing(&spec, v)?,
    };
    if sl == p.sl && tp == p.tp && tr == p.trailing {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    let np = Position { sl, tp, trailing: tr, ..p.clone() };
    tx.emit(Event::PositionUpdated { position: np.clone(), change: "modified".into(), deal: None });
    Ok((p, np))
}

#[derive(Clone, Debug, Default)]
pub struct OrderPatch {
    pub price: Option<D>,
    pub stop_limit: Option<D>,
    pub volume: Option<D>,
    pub sl: Option<Option<D>>,
    pub tp: Option<Option<D>>,
    pub trailing_points: Option<Option<i64>>,
    pub expiry: Option<Expiry>,
    pub expiry_at: Option<DateTime<Utc>>,
}

pub fn modify_order(tx: &mut Tx, env: &Env, ticket: i64, patch: OrderPatch, dealer: Option<&DealerCtx>) -> Result<(Order, Order), Reject> {
    let o = tx.st.orders.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Order #{ticket} not found")))?;
    if o.option.is_some() {
        if dealer.is_none()
            && let Some(r) = env.restrictions
            && let Some(rej) = crate::controls::trading_reject(r, tx.st.account.user_id, true, env.now)
        {
            return Err(rej);
        }
        return super::options::modify_order(tx, env, ticket, &patch);
    }
    let spec = env.spec(&o.symbol)?.clone();
    cfd_product_gate(env, patch.volume.is_some_and(|v| v > o.volume))?;
    if dealer.is_none() {
        gate(env, &tx.st, &o.symbol, true, patch.volume.unwrap_or(o.volume), None)?;
    }
    let q = env.live_quote(&tx.st.account, &o.symbol)?;
    let mut n = o.clone();
    if let Some(v) = patch.volume {
        if let Some(e) = spec.volume_error(v) {
            return Err(Reject::new("invalid_volume", e));
        }
        n.volume = v;
    }
    if let Some(p) = patch.price.filter(|p| *p > ZERO) {
        n.price = spec.round_price(p);
    }
    if let Some(p) = patch.stop_limit.filter(|p| *p > ZERO) {
        if o.kind != OrderType::StopLimit {
            return Err(Reject::new("invalid_price", "Only stop-limit orders have a limit price"));
        }
        n.stop_limit = Some(spec.round_price(p));
    }
    let mkt = q.open_price(o.side);
    let buy = o.side == Side::Buy;
    let px = n.active_price();
    let ok = match n.active_kind() {
        OrderType::Limit => if buy { px < mkt } else { px > mkt },
        OrderType::Stop | OrderType::StopLimit => if buy { px > mkt } else { px < mkt },
        OrderType::Market => true,
    };
    if !ok && (patch.price.is_some() || patch.stop_limit.is_some()) {
        return Err(Reject::new("invalid_price", format!("Order price is on the wrong side of the market ({})", mkt.normalize())));
    }
    if n.kind == OrderType::StopLimit && !n.triggered
        && let Some(l) = n.stop_limit
        && (if buy { l > n.price } else { l < n.price })
    {
        return Err(Reject::new("invalid_price", format!("Limit price must be {} the stop price", if buy { "at or below" } else { "at or above" })));
    }
    if let Some(v) = patch.sl {
        n.sl = round_opt(&spec, v);
    }
    if let Some(v) = patch.tp {
        n.tp = round_opt(&spec, v);
    }
    check_sltp(&spec, o.side, n.stop_limit.unwrap_or(n.price), n.sl, n.tp)?;
    if let Some(v) = patch.trailing_points {
        n.trailing = trailing(&spec, v)?;
    }
    if let Some(e) = patch.expiry {
        n.expiry = e;
        n.expiry_at = match e {
            Expiry::Gtc => None,
            Expiry::Today => Some(end_of_server_day(env.now)),
            Expiry::Date => {
                let at = patch.expiry_at.ok_or_else(|| Reject::new("invalid_expiry", "Enter the expiry date"))?;
                if at <= env.now {
                    return Err(Reject::new("invalid_expiry", "Expiry must be in the future"));
                }
                Some(at)
            }
        };
    }
    if n == o {
        return Err(Reject::new("no_change", "Nothing changed"));
    }
    tx.emit(Event::OrderUpdated { order: n.clone(), change: "modified".into() });
    Ok((o, n))
}

pub fn cancel_order(tx: &mut Tx, env: &Env, ticket: i64, reason: &str) -> Result<Order, Reject> {
    let o = tx.st.orders.get(&ticket).cloned().ok_or_else(|| Reject::new("not_found", format!("Order #{ticket} not found")))?;
    tx.emit(Event::OrderRemoved { ticket, status: OrderStatus::Cancelled, reason: reason.into(), at: env.now, fill_price: None, position_ticket: None });
    Ok(o)
}

/* ------------------------------------------------------------------ */
/* Bulk close (D100)                                                   */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkFilter {
    All,
    Profitable,
    Losing,
    Pending,
    Buys,
    Sells,
}

#[derive(Debug, Default)]
pub struct BulkOutcome {
    pub done: Vec<i64>,
    pub failed: Vec<(i64, String)>,
    pub profit: D,
}

pub fn bulk_close(tx: &mut Tx, env: &Env, filter: BulkFilter, symbol: Option<&str>) -> BulkOutcome {
    let mut out = BulkOutcome::default();
    if filter == BulkFilter::Pending {
        let tickets: Vec<i64> = tx.st.orders.values().filter(|o| symbol.is_none_or(|s| o.symbol == s)).map(|o| o.ticket).collect();
        for t in tickets {
            match cancel_order(tx, env, t, "bulk cancel") {
                Ok(_) => out.done.push(t),
                Err(e) => out.failed.push((t, e.message)),
            }
        }
        return out;
    }
    let acc = tx.st.account.clone();
    let tickets: Vec<i64> = tx
        .st
        .positions
        .values()
        .filter(|p| symbol.is_none_or(|s| p.symbol == s))
        .filter(|p| match filter {
            BulkFilter::All => true,
            BulkFilter::Buys => p.side == Side::Buy,
            BulkFilter::Sells => p.side == Side::Sell,
            BulkFilter::Profitable => super::position_floating(env, &acc, p).is_some_and(|v| v > ZERO),
            BulkFilter::Losing => super::position_floating(env, &acc, p).is_some_and(|v| v < ZERO),
            BulkFilter::Pending => false,
        })
        .map(|p| p.ticket)
        .collect();
    for t in tickets {
        let mut attempt = Tx::new(&tx.st);
        match close_position(&mut attempt, env, t, CloseReq::default()) {
            Ok((_, profit)) => {
                for ev in attempt.events {
                    tx.emit(ev);
                }
                tx.notes.extend(attempt.notes);
                out.done.push(t);
                out.profit += profit;
            }
            Err(e) => out.failed.push((t, e.message)),
        }
    }
    out
}

//! Risk desk in the engine (docs/RISK-DESK.md): exposure caps (alert vs enforce, increase refused, reduce and close
//! allowed), per-group max lot / max positions, news windows (no new pending orders, margin multiplier), pending
//! orders refused at fill time, routing in recommend vs execute mode with the new conditions, and negative balance
//! protection on every stop-out / gap path.

use std::collections::HashMap;

use super::testkit::{Harness, Kit, d, t};
use super::trade::{self, CloseReq, OrderReq, PlaceResult};
use crate::model::{AccountKind, Book, DealReason, OrderType, Side, TxnKind};
use crate::money::D;
use crate::risk::{ExpIndex, ExposureLimit, GroupLimit, LimitMode, NewsEvent, NewsLimit, RoutingMode, ScoreLite};
use crate::rules::{RoutingAction, RoutingCondition, RoutingRule};
use crate::state::{AccountState, Event};

fn buy(sym: &str, v: &str) -> OrderReq {
    OrderReq::market(sym, Side::Buy, d(v))
}
fn sell(sym: &str, v: &str) -> OrderReq {
    OrderReq::market(sym, Side::Sell, d(v))
}

fn try_place(h: &mut Harness, kit: &Kit, req: OrderReq) -> Result<PlaceResult, super::Reject> {
    h.run(kit, |tx, env| trade::place_order(tx, env, req))
}

fn pos_ticket(r: &PlaceResult) -> i64 {
    match r {
        PlaceResult::Filled { position_ticket: Some(t), .. } => *t,
        other => panic!("not filled: {other:?}"),
    }
}

/// The exposure index as the exposure loop would set it: B-book net per symbol (USD), scanned after `kit.now`.
fn index(kit: &Kit, net: &[(&str, &str)]) {
    let idx = ExpIndex { started_ms: kit.now.timestamp_millis() + 1, net_b_usd: net.iter().map(|(s, v)| (s.to_string(), d(v))).collect(), positions: HashMap::new() };
    kit.risk.set_index(1, idx);
}

fn cap(kit: &mut Kit, symbol: &str, usd: &str, mode: LimitMode) {
    kit.tenant.risk.limits.exposure = vec![ExposureLimit { symbol: symbol.into(), cap_usd: d(usd), warn_pct: d("80"), mode, enabled: true }];
}

#[test]
fn exposure_cap_alerts_first_then_refuses_increases_but_never_reductions() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    // 1 lot EURUSD = 110 000 USD; the house book is 900 000 USD net long clients, the cap 1 000 000
    cap(&mut kit, "EURUSD", "1000000", LimitMode::Alert);
    index(&kit, &[("EURUSD", "900000")]);
    let mut h = Harness::live(&kit, "hedge", "100000");
    // alert mode: the order passes and the desk is told what would have been refused
    let r = try_place(&mut h, &kit, buy("EURUSD", "2")).unwrap();
    let long = pos_ticket(&r);
    let obs = kit.risk.drain_observed();
    assert_eq!(obs.len(), 1);
    assert_eq!((obs[0].code, obs[0].enforced, obs[0].login), ("exposure_limit", false, h.st.account.login));
    // the passed order counts as in-flight exposure until the next scan
    assert_eq!(kit.risk.net_b_usd(1, "EURUSD"), d("1120000"));

    // enforce: an increase past the cap is refused with a clear code
    cap(&mut kit, "EURUSD", "1000000", LimitMode::Enforce);
    index(&kit, &[("EURUSD", "900000")]);
    let e = try_place(&mut h, &kit, buy("EURUSD", "1")).unwrap_err();
    assert_eq!(e.code, "exposure_limit");
    assert!(e.message.contains("1010000") && e.message.contains("1000000"), "{}", e.message);
    assert_eq!(h.st.positions.len(), 1);
    assert!(kit.risk.drain_observed().iter().any(|o| o.enforced && o.code == "exposure_limit"));
    // within the cap: fine
    assert!(try_place(&mut h, &kit, buy("EURUSD", "0.5")).is_ok());
    // a reducing trade always passes, even far past the cap
    index(&kit, &[("EURUSD", "1500000")]);
    assert!(try_place(&mut h, &kit, sell("EURUSD", "1")).is_ok());
    assert_eq!(try_place(&mut h, &kit, buy("EURUSD", "0.01")).unwrap_err().code, "exposure_limit");
    // closing a position is never checked
    index(&kit, &[("EURUSD", "-1500000")]);
    h.run(&kit, |tx, env| trade::close_position(tx, env, long, CloseReq::default())).unwrap();
    // an order through zero to the other side past the cap is an increase too
    index(&kit, &[("EURUSD", "900000")]);
    assert_eq!(try_place(&mut h, &kit, sell("EURUSD", "18")).unwrap_err().code, "exposure_limit");
    assert!(try_place(&mut h, &kit, sell("EURUSD", "17")).is_ok(), "−970 000 is inside the cap");
    // the `*` row covers symbols without their own
    kit.quote("GBPUSD", "1.30000", "1.30000");
    cap(&mut kit, "*", "100000", LimitMode::Enforce);
    index(&kit, &[]);
    assert_eq!(try_place(&mut h, &kit, buy("GBPUSD", "1")).unwrap_err().code, "exposure_limit");
    // demo accounts and A-book routed accounts are not house exposure
    let mut demo = Harness::demo(&kit, "hedge");
    assert!(try_place(&mut demo, &kit, buy("GBPUSD", "1")).is_ok());
    let mut a = Harness::live(&kit, "hedge", "100000");
    a.st.account.route_override = Some(Book::A);
    assert!(try_place(&mut a, &kit, buy("GBPUSD", "1")).is_ok());
    // a dealer's forced trade is not checked; a normal dealer trade is
    let dealer = |force| trade::DealerCtx { staff: "Julia".into(), reason_code: "DLR-03".into(), force };
    assert_eq!(try_place(&mut h, &kit, OrderReq { dealer: Some(dealer(false)), source: crate::model::Source::Dealer, ..buy("GBPUSD", "1") }).unwrap_err().code, "exposure_limit");
    assert!(try_place(&mut h, &kit, OrderReq { dealer: Some(dealer(true)), source: crate::model::Source::Dealer, ..buy("GBPUSD", "1") }).is_ok());
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn a_pending_order_past_the_cap_is_rejected_when_it_triggers() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    let mut h = Harness::live(&kit, "hedge", "100000");
    let PlaceResult::Pending { ticket, .. } = try_place(&mut h, &kit, OrderReq { kind: OrderType::Limit, price: Some(d("1.09900")), ..buy("EURUSD", "5") }).unwrap() else { panic!() };
    cap(&mut kit, "EURUSD", "1000000", LimitMode::Enforce);
    index(&kit, &[("EURUSD", "800000")]);
    kit.quote("EURUSD", "1.09880", "1.09890");
    h.tick(&kit, "EURUSD");
    assert!(h.st.orders.is_empty() && h.st.positions.is_empty());
    assert!(h.log.iter().any(|e| matches!(e, Event::OrderRemoved { ticket: t, status: crate::model::OrderStatus::Rejected, reason, .. } if *t == ticket && reason.contains("exposure"))));
    h.assert_replay();
}

#[test]
fn group_max_lot_and_max_positions_per_client() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    kit.quote("GBPUSD", "1.30000", "1.30000");
    kit.tenant.risk.limits.groups = vec![GroupLimit { group: "hedge".into(), max_lot: Some(d("1")), max_positions: Some(3), mode: LimitMode::Enforce, enabled: true }];
    let mut h = Harness::live(&kit, "hedge", "100000");
    assert_eq!(try_place(&mut h, &kit, buy("EURUSD", "2")).unwrap_err().code, "group_max_lot");
    // pending orders too
    let lim = OrderReq { kind: OrderType::Limit, price: Some(d("1.05")), ..buy("EURUSD", "2") };
    assert_eq!(try_place(&mut h, &kit, lim).unwrap_err().code, "group_max_lot");
    // the account's own max lot wins over the group default
    h.st.account.controls.max_lot = Some(d("5"));
    assert!(try_place(&mut h, &kit, buy("EURUSD", "2")).is_ok());
    h.st.account.controls.max_lot = None;
    assert!(try_place(&mut h, &kit, buy("GBPUSD", "1")).is_ok());
    // two positions here + one on the client's other account (last scan) = the max of 3
    let mut idx = ExpIndex { started_ms: kit.now.timestamp_millis() + 1, ..Default::default() };
    idx.positions.insert(h.st.account.user_id, HashMap::from([(10_000_999, 1)]));
    kit.risk.set_index(1, idx);
    assert_eq!(try_place(&mut h, &kit, buy("EURUSD", "1")).unwrap_err().code, "group_max_positions");
    // alert mode only reports
    kit.tenant.risk.limits.groups[0].mode = LimitMode::Alert;
    assert!(try_place(&mut h, &kit, buy("EURUSD", "1")).is_ok());
    assert!(kit.risk.drain_observed().iter().any(|o| o.code == "group_max_positions" && !o.enforced));
    // netting: adding to the one position on the symbol creates no new position
    kit.tenant.risk.limits.groups = vec![GroupLimit { group: "net".into(), max_lot: None, max_positions: Some(1), mode: LimitMode::Enforce, enabled: true }];
    kit.risk.set_index(1, ExpIndex::default());
    let mut n = Harness::live(&kit, "net", "100000");
    assert!(try_place(&mut n, &kit, buy("EURUSD", "1")).is_ok());
    assert!(try_place(&mut n, &kit, buy("EURUSD", "1")).is_ok());
    assert!(try_place(&mut n, &kit, sell("EURUSD", "0.5")).is_ok());
    assert_eq!(try_place(&mut n, &kit, buy("GBPUSD", "1")).unwrap_err().code, "group_max_positions");
}

fn nfp(kit: &Kit, minutes_ahead: i64) {
    let ev = NewsEvent { id: 77, title: "Non-Farm Employment Change".into(), currency: "USD".into(), impact: 3, starts_at: kit.now + chrono::Duration::minutes(minutes_ahead), symbols: vec![] };
    kit.risk.set_news(vec![ev], kit.now);
}

#[test]
fn news_window_blocks_new_pending_orders_and_raises_margin_in_enforce_mode() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    kit.quote("EURGBP", "0.85000", "0.85000");
    kit.quote("GBPUSD", "1.30000", "1.30000");
    kit.tenant.risk.limits.news = Some(NewsLimit { minutes_before: 5, minutes_after: 5, min_impact: 3, margin_multiplier: d("3"), block_pending: true, mode: LimitMode::Enforce, enabled: true });
    nfp(&kit, 2);
    let mut h = Harness::live(&kit, "hedge", "2000");
    let lim = |sym: &str, px: &str| OrderReq { kind: OrderType::Limit, price: Some(d(px)), ..buy(sym, "0.1") };
    let e = try_place(&mut h, &kit, lim("EURUSD", "1.05")).unwrap_err();
    assert_eq!(e.code, "news_window");
    assert!(e.message.contains("Non-Farm"), "{}", e.message);
    // EURGBP has no USD leg: not in the window
    assert!(try_place(&mut h, &kit, lim("EURGBP", "0.80")).is_ok());
    // margin × 3 on new exposure: 1 lot needs 3 300 instead of 1 100 USD
    let e = try_place(&mut h, &kit, buy("EURUSD", "1")).unwrap_err();
    assert_eq!(e.code, "no_money");
    assert!(e.message.contains("news window"), "{}", e.message);
    assert!(try_place(&mut h, &kit, buy("EURUSD", "0.5")).is_ok(), "1 650 USD fits");
    // the window has passed: back to normal
    kit.now += chrono::Duration::minutes(10);
    kit.quote("EURUSD", "1.10000", "1.10000");
    assert!(try_place(&mut h, &kit, lim("EURUSD", "1.05")).is_ok());
    // alert mode: nothing is refused, the would-be refusal is reported
    nfp(&kit, 1);
    kit.tenant.risk.limits.news.as_mut().unwrap().mode = LimitMode::Alert;
    let _ = kit.risk.drain_observed();
    assert!(try_place(&mut h, &kit, lim("EURUSD", "1.04")).is_ok());
    assert!(kit.risk.drain_observed().iter().any(|o| o.code == "news_window" && !o.enforced));
    assert_eq!(trade::news_margin_multiplier(&kit.env(&h.st), "EURUSD"), D::ONE);
}

fn rule(id: &str, field: &str, op: &str, value: &str, book: Book, pct: f64) -> RoutingRule {
    RoutingRule { id: id.into(), name: format!("{field} rule"), conditions: vec![RoutingCondition { field: field.into(), op: op.into(), value: value.into() }], join: "AND".into(), action: RoutingAction { book, pct, lp: None }, enabled: true, hits24h: 0, lots24h: 0.0 }
}

fn book_of(h: &Harness, ticket: i64) -> (Book, String) {
    let p = &h.st.positions[&ticket];
    (p.book, p.route_history[0].reason.clone())
}

#[test]
fn routing_recommends_in_recommend_mode_and_routes_in_execute_mode() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    kit.tenant.routing_rules = vec![rule("RR-1", "Risk score", "≥", "70", Book::A, 100.0)];
    let mut h = Harness::live(&kit, "hedge", "100000");
    // no score yet: the condition can't match
    let t0 = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.1")).unwrap());
    assert_eq!(book_of(&h, t0), (Book::B, "Default route".into()));
    kit.risk.set_scores([(h.st.account.login, ScoreLite { user_id: 7, score: 81.0, category: "likely_profitable".into(), win_rate: Some(71.0), avg_hold_secs: Some(40.0), country: Some("ae".into()), computed_at: kit.now })]);
    // recommend (the default): labelled, still on the default book
    assert_eq!(kit.tenant.risk.settings.routing_mode, RoutingMode::Recommend);
    let t1 = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.1")).unwrap());
    let (b, why) = book_of(&h, t1);
    assert_eq!(b, Book::B);
    assert!(why.contains("RR-1 recommends A-book (recommend only)"), "{why}");
    // a dealer quick route always applies
    kit.tenant.routing_rules.insert(0, rule(&format!("RQ-L{}", h.st.account.login), "Login", "=", &h.st.account.login.to_string(), Book::A, 100.0));
    let t2 = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.1")).unwrap());
    assert_eq!(book_of(&h, t2).0, Book::A);
    kit.tenant.routing_rules.remove(0);
    // execute: the rule routes
    kit.tenant.risk.settings.routing_mode = RoutingMode::Execute;
    let t3 = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.1")).unwrap());
    assert_eq!(book_of(&h, t3), (Book::A, "Rule RR-1 · Risk score rule".into()));
    // country, hold time, category and equity conditions read the same facts
    for (field, op, value) in [("Country", "in", "AE, SA"), ("Avg hold time", "<", "60s"), ("Risk category", "in", "likely_profitable"), ("Equity", "≥", "$50,000"), ("Win rate (30d)", "≥", "70%")] {
        kit.tenant.routing_rules = vec![rule("RR-2", field, op, value, Book::A, 100.0)];
        let t = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.1")).unwrap());
        assert_eq!(book_of(&h, t).0, Book::A, "{field}");
    }
    // a 50 % split is deterministic by ticket
    kit.tenant.routing_rules = vec![rule("RR-3", "Symbol", "in", "EURUSD", Book::A, 50.0)];
    for _ in 0..6 {
        let t = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.01")).unwrap());
        let want = crate::rules::split_book(&RoutingAction { book: Book::A, pct: 50.0, lp: None }, h.st.positions[&t].order_ticket);
        assert_eq!(book_of(&h, t).0, want);
    }
    // news window condition
    nfp(&kit, 1);
    kit.tenant.routing_rules = vec![rule("RR-4", "News window", "is", "High impact ±2m", Book::A, 100.0)];
    let t = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.1")).unwrap());
    assert_eq!(book_of(&h, t).0, Book::A);
    h.assert_replay();
}

/* ------------------------------------------------------------------ */
/* Negative balance protection (D16): every stop-out and gap path      */
/* ------------------------------------------------------------------ */

fn nbp_booked(h: &Harness) -> D {
    h.log.iter().filter_map(|e| if let Event::Ledger { txn } = e && txn.kind == TxnKind::Nbp { Some(txn.postings[0].amount) } else { None }).sum()
}

fn flat_balance_ok(h: &Harness) {
    if h.st.positions.is_empty() {
        assert!(h.st.balance >= D::ZERO, "flat account left at {}", h.st.balance);
    }
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn nbp_after_a_stop_loss_gap() {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut h = Harness::live(&kit, "hedge", "1200");
    let r = try_place(&mut h, &kit, OrderReq { sl: Some(d("1.09900")), ..buy("EURUSD", "1") }).unwrap();
    let p = pos_ticket(&r);
    // the price gaps 300 pips through the stop: the SL fills at the gap price, −3 000 on a 1 200 balance
    kit.quote("EURUSD", "1.07000", "1.07000");
    h.tick(&kit, "EURUSD");
    assert!(!h.st.positions.contains_key(&p));
    assert!(h.log.iter().any(|e| matches!(e, Event::PositionClosed { deal, .. } if deal.reason == DealReason::Sl && deal.profit == d("-3000.00"))));
    assert_eq!(h.st.balance, D::ZERO);
    assert_eq!(nbp_booked(&h), d("1800.00"));
    flat_balance_ok(&h);
}

#[test]
fn nbp_after_a_stop_out_gap_on_netting_cent_and_hedging_accounts() {
    for group in ["hedge", "net", "cent"] {
        let kit = Kit::new();
        kit.quote("EURUSD", "1.10000", "1.10000");
        kit.quote("GBPUSD", "1.30000", "1.30000");
        let mut h = Harness::live(&kit, group, "2000");
        try_place(&mut h, &kit, buy("EURUSD", "1")).unwrap();
        if group != "net" {
            try_place(&mut h, &kit, sell("GBPUSD", "0.2")).unwrap();
        }
        // a weekend-style gap far past the stop-out level
        kit.quote("EURUSD", "1.05000", "1.05000");
        h.tick(&kit, "EURUSD");
        assert!(h.log.iter().any(|e| matches!(e, Event::StopOut { .. })), "{group}: stop-out");
        // the stop-out closed what it had to; once flat, the balance is 0, never below
        if !h.st.positions.is_empty() {
            kit.quote("GBPUSD", "1.40000", "1.40000");
            h.tick(&kit, "GBPUSD");
        }
        assert!(h.st.positions.is_empty(), "{group}: still open {:?}", h.st.positions.keys());
        assert_eq!(h.st.balance, D::ZERO, "{group}");
        assert!(nbp_booked(&h) > D::ZERO, "{group}");
        flat_balance_ok(&h);
    }
}

#[test]
fn nbp_on_client_close_close_by_dealer_close_and_netting_reduction() {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    // client close after a gap (the stop-out would have fired on a tick; here the client closes first)
    let mut h = Harness::live(&kit, "hedge", "500");
    let p = pos_ticket(&try_place(&mut h, &kit, buy("EURUSD", "0.4")).unwrap());
    kit.quote("EURUSD", "1.08000", "1.08000");
    h.run(&kit, |tx, env| trade::close_position(tx, env, p, CloseReq::default())).unwrap();
    assert_eq!(h.st.balance, D::ZERO);
    flat_balance_ok(&h);
    // dealer close
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut g = Harness::live(&kit, "hedge", "500");
    let p = pos_ticket(&try_place(&mut g, &kit, buy("EURUSD", "0.4")).unwrap());
    kit.quote("EURUSD", "1.08000", "1.08000");
    let dealer = trade::DealerCtx { staff: "Julia".into(), reason_code: "DLR-03".into(), force: false };
    g.run(&kit, |tx, env| trade::close_position(tx, env, p, CloseReq { dealer: Some(dealer), ..Default::default() })).unwrap();
    assert_eq!(g.st.balance, D::ZERO);
    flat_balance_ok(&g);
    // netting: an opposite order that closes the position
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut n = Harness::live(&kit, "net", "500");
    try_place(&mut n, &kit, buy("EURUSD", "0.4")).unwrap();
    kit.quote("EURUSD", "1.08000", "1.08000");
    try_place(&mut n, &kit, sell("EURUSD", "0.4")).unwrap();
    assert_eq!(n.st.balance, D::ZERO);
    flat_balance_ok(&n);
    // close by: two opposite positions, the loss realised when the second goes
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut c = Harness::live(&kit, "hedge", "500");
    let a = pos_ticket(&try_place(&mut c, &kit, buy("EURUSD", "0.4")).unwrap());
    kit.quote("EURUSD", "1.08500", "1.08500");
    let b = pos_ticket(&try_place(&mut c, &kit, sell("EURUSD", "0.4")).unwrap());
    c.run(&kit, |tx, env| trade::close_by(tx, env, a, b)).unwrap();
    assert!(c.st.positions.is_empty());
    assert_eq!(c.st.balance, D::ZERO);
    flat_balance_ok(&c);
}

#[test]
fn balance_never_stays_negative_once_flat_across_gap_sizes() {
    for gap in ["0.0050", "0.0200", "0.0500", "0.1000", "0.2500"] {
        for group in ["hedge", "net", "cent"] {
            let kit = Kit::new();
            kit.quote("EURUSD", "1.10000", "1.10000");
            let mut h = Harness::live(&kit, group, "1500");
            try_place(&mut h, &kit, OrderReq { sl: Some(d("1.09500")), ..buy("EURUSD", "1") }).unwrap();
            let px = d("1.10000") - d(gap);
            kit.quote("EURUSD", &px.to_string(), &px.to_string());
            h.tick(&kit, "EURUSD");
            assert!(h.st.positions.is_empty(), "{group} {gap}");
            assert!(h.st.balance >= D::ZERO, "{group} {gap}: {}", h.st.balance);
            flat_balance_ok(&h);
        }
    }
}

#[test]
fn risk_gate_does_nothing_without_the_risk_desk() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    cap(&mut kit, "EURUSD", "1", LimitMode::Enforce);
    let h = Harness::live(&kit, "hedge", "100000");
    let env = super::Env { risk: None, ..kit.env(&h.st) };
    let st: AccountState = h.st.clone();
    assert!(trade::risk_gate(&env, &st, "EURUSD", Side::Buy, d("10"), trade::RiskStage::Market, None, None).is_ok());
    let _ = (AccountKind::Live, t("2026-09-28T12:00:00Z"));
}

//! Kalks FX Options in the engine core (on Options accounts): premium cash flows, commission, cash-only, scenario
//! margin (no CFD offsets on an Options account), the CFD / Options product gates, combos (all or nothing), closes and cut-offs, gates, pending limit / trigger orders, premium SL / TP,
//! barrier knocks (once), settlement (idempotent, re-run nets, hold), void, stop-out by units, metrics, replay.

use super::options::{self, BarrierReq, LegReq, OptClose, OptKind, OptOrderReq, PlaceOut};
use super::testkit::{Harness, Kit, d, group, options_group, opt_snapshot, t};
use super::{Reject, funds, metrics, risk, trade};
use crate::model::{AccountKind, BarrierKind, Deal, DealReason, Expiry, Side, Trigger, TriggerOp};
use crate::money::{D, ZERO, r2};
use crate::state::Event;

const C116: &str = "EURUSD-20261002-1.1600-C";
const C117: &str = "EURUSD-20261002-1.1700-C";
const P115: &str = "EURUSD-20261002-1.1500-P";
const KEY: &str = "EURUSD:2026-10-02";

fn leg(series: &str, side: Side, c: &str) -> LegReq {
    LegReq { series: series.into(), side, contracts: d(c), barrier: None }
}

fn req(legs: Vec<LegReq>, cid: &str) -> OptOrderReq {
    OptOrderReq { client_order_id: Some(cid.into()), ..OptOrderReq::market(legs) }
}

fn place(h: &mut Harness, kit: &Kit, r: OptOrderReq) -> Result<PlaceOut, Reject> {
    h.run(kit, |tx, env| options::place(tx, env, r))
}

fn filled(o: PlaceOut) -> options::Filled {
    match o {
        PlaceOut::Filled(f) => f,
        other => panic!("not filled: {other:?}"),
    }
}

fn kit() -> Kit {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.15990", "1.16010");
    kit.quote("USDJPY", "149.990", "150.010");
    kit.options.fix(C116, "0.0050", "0.0052");
    kit
}

fn deals_of(h: &Harness, ticket: i64) -> Vec<Deal> {
    h.log
        .iter()
        .filter_map(|e| match e {
            Event::PositionOpened { deal: Some(d), .. } | Event::PositionUpdated { deal: Some(d), .. } | Event::PositionClosed { deal: d, .. } => Some(d.clone()),
            _ => None,
        })
        .filter(|d| d.position_ticket == ticket)
        .collect()
}

#[test]
fn buy_then_close_books_premium_commission_and_realised_pnl() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let f = filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "o1")).unwrap());
    let t = f.legs[0].ticket;
    // premium 0.0052 × 2 × 10 000 = 104; commission min(0.25 × 2, 10 % × 104) = 0.50
    assert_eq!((f.legs[0].price, f.legs[0].premium, f.legs[0].commission), (d("0.0052"), d("104.00"), d("0.50")));
    assert_eq!(h.st.balance, d("9895.50"));
    let p = &h.st.positions[&t];
    assert_eq!((p.symbol.as_str(), p.volume, p.premium, p.option.as_ref().unwrap().right), (C116, d("2"), d("-104.00"), crate::model::OptRight::Call));
    let m = metrics(&kit.env(&h.st), &h.st);
    // marked at mid 0.0051: value 102, P&L -2, no margin for a long
    assert_eq!((r2(m.option_value), r2(m.option_pnl), m.margin), (d("102.00"), d("-2.00"), ZERO));
    assert_eq!(r2(m.equity), d("9997.50"));
    assert_eq!(m.equity, m.balance + m.credit + m.bonus + (m.profit - m.option_pnl) + m.swap + m.option_value, "equity identity");
    assert_eq!(r2(m.withdrawable()), d("9895.50"), "the option's value is not cash");
    // the same client order id again: nothing new
    assert!(matches!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "o1")).unwrap(), PlaceOut::Duplicate { .. }));
    assert_eq!(h.st.positions.len(), 1);

    // the premium rises: sell one back at the bid (partial), then the rest through the generic close
    kit.options.fix(C116, "0.0060", "0.0062");
    let (_, pnl) = h.run(&kit, |tx, env| options::close(tx, env, t, OptClose { volume: Some(d("1")), ..OptClose::client() })).unwrap();
    assert_eq!(pnl, d("8.00")); // 60 - 52
    assert_eq!(h.st.balance, d("9955.25")); // + 60 - 0.25
    assert_eq!((h.st.positions[&t].volume, h.st.positions[&t].premium, h.st.positions[&t].commission), (d("1"), d("-52.00"), d("0.25")));
    let (_, pnl) = h.run(&kit, |tx, env| trade::close_position(tx, env, t, trade::CloseReq::default())).unwrap();
    assert_eq!(pnl, d("8.00"));
    assert_eq!(h.st.balance, d("10015.00"));
    assert!(h.st.positions.is_empty());
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn a_short_receives_premium_and_pays_scenario_margin() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Sell, "1")], "s1")).unwrap());
    assert_eq!(h.st.balance, d("10049.75")); // + 50 - 0.25
    let m = metrics(&kit.env(&h.st), &h.st);
    assert!(m.option_margin > d("50") && m.option_margin == m.margin, "{m:?}");
    assert_eq!(r2(m.option_value), d("-51.00"));
    // the premium received is not withdrawable beyond the free margin
    assert!(m.withdrawable() < h.st.balance);
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn premiums_are_paid_from_cash_never_from_credit() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "50");
    h.run(&kit, |tx, env| funds::adjust(tx, env, funds::AdjustKind::Credit, d("1000"), "cr-1", "BON", "credit")).unwrap();
    let e = place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "c1")).unwrap_err();
    assert_eq!(e.code, "insufficient_cash", "{e}");
    assert_eq!(h.st.balance, d("50"));
    h.run(&kit, |tx, env| funds::transfer(tx, env, funds::Direction::In, d("10"), "fund-2", None).map(|_| ())).unwrap();
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "c2")).unwrap());
    assert_eq!(h.st.balance, d("7.75")); // 60 - 52 - 0.25
    h.assert_ledger();
}

#[test]
fn short_margin_must_come_from_own_funds() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "100");
    h.run(&kit, |tx, env| funds::adjust(tx, env, funds::AdjustKind::Credit, d("100000"), "cr-1", "BON", "credit")).unwrap();
    let e = place(&mut h, &kit, req(vec![leg(C116, Side::Sell, "10")], "s1")).unwrap_err();
    assert_eq!(e.code, "insufficient_margin", "{e}");
    assert!(h.st.positions.is_empty());
}

#[test]
fn long_options_carry_no_margin_and_an_options_account_takes_no_cfd_offsets() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "50000");
    filled(place(&mut h, &kit, req(vec![leg(C117, Side::Buy, "5")], "l1")).unwrap());
    assert_eq!(metrics(&kit.env(&h.st), &h.st).option_margin, ZERO, "a long option is paid in full");
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Sell, "10")], "s1")).unwrap());
    let base = metrics(&kit.env(&h.st), &h.st).option_margin;
    assert!(base > ZERO);
    // an Options account opens no CFD exposure (CFD / Options account split), dealers included
    let e = h.run(&kit, |tx, env| trade::place_order(tx, env, trade::OrderReq::market("EURUSD", Side::Buy, d("1"))).map(|_| ())).unwrap_err();
    assert_eq!(e.code, "product_mismatch", "{e}");
    let none = std::collections::BTreeSet::new();
    let covered: std::collections::BTreeMap<String, (D, D)> = [("EURUSD".to_string(), (d("1"), ZERO))].into();
    // and its option margin never takes a same-underlying CFD offset: a covered call is margined like a naked one
    let env = kit.env(&h.st);
    assert_eq!(options::margin(&env, &h.st, &covered, &[], &none), base);
    // a pre-split account in a CFD group keeps the old cross-margin rule: offsets can only reduce the option margin
    // (every CFD exposure on the underlying, long or short, any size, leaves it at or below `base`)
    let legacy = super::Env { group: &kit.tenant.groups["hedge"], ..kit.env(&h.st) };
    for (long, short) in [("0.1", "0"), ("0.5", "0"), ("1", "0"), ("0", "0.3"), ("0", "1.5"), ("3", "0")] {
        let exp: std::collections::BTreeMap<String, (D, D)> = [("EURUSD".to_string(), (d(long), d(short)))].into();
        let m = options::margin(&legacy, &h.st, &exp, &[], &none);
        assert!(m <= base, "{long}/{short}: {m} > {base}");
    }
    assert!(options::margin(&legacy, &h.st, &covered, &[], &none) < base, "the covered call's offset");
}

/// CFD / Options account split, both ways: options (market, pending, preview) are refused on a CFD account and CFDs
/// (market, pending, dealer trades, volume added by a dealer) on an Options account, with `product_mismatch`;
/// closing is never refused for the product, and a refusal books nothing.
#[test]
fn each_account_trades_only_its_own_product() {
    let kit = kit();
    // options on a CFD account
    let mut c = Harness::live(&kit, "hedge", "10000");
    let e = place(&mut c, &kit, req(vec![leg(C116, Side::Buy, "1")], "x1")).unwrap_err();
    assert_eq!((e.code, e.message.as_str()), ("product_mismatch", options::CFD_ACCOUNT));
    let lim = OptOrderReq { kind: OptKind::Limit, limit_premium: Some(d("0.0010")), ..req(vec![leg(C116, Side::Buy, "1")], "x2") };
    assert_eq!(place(&mut c, &kit, lim).unwrap_err().code, "product_mismatch");
    let pv = options::preview(&kit.env(&c.st), &c.st, &req(vec![leg(C116, Side::Sell, "1")], "pv"));
    assert!(pv.reasons.iter().any(|r| r.code == "product_mismatch"), "{:?}", pv.reasons);
    assert!(c.st.positions.is_empty() && c.st.orders.is_empty() && c.st.balance == d("10000"));
    // CFDs still trade there
    c.run(&kit, |tx, env| trade::place_order(tx, env, trade::OrderReq::market("EURUSD", Side::Buy, d("0.1"))).map(|_| ())).unwrap();
    // CFDs on an Options account: market, pending, a dealer's trade, a dealer adding volume
    let mut o = Harness::live(&kit, "opt", "10000");
    let e = o.run(&kit, |tx, env| trade::place_order(tx, env, trade::OrderReq::market("EURUSD", Side::Buy, d("0.1"))).map(|_| ())).unwrap_err();
    assert_eq!((e.code, e.message.as_str()), ("product_mismatch", trade::OPTIONS_ACCOUNT));
    let pending = trade::OrderReq { kind: crate::model::OrderType::Limit, price: Some(d("1.1000")), ..trade::OrderReq::market("EURUSD", Side::Buy, d("0.1")) };
    assert_eq!(o.run(&kit, |tx, env| trade::place_order(tx, env, pending).map(|_| ())).unwrap_err().code, "product_mismatch");
    let dealer = trade::DealerCtx { force: true, ..Default::default() };
    let by_dealer = trade::OrderReq { dealer: Some(dealer.clone()), ..trade::OrderReq::market("EURUSD", Side::Buy, d("0.1")) };
    assert_eq!(o.run(&kit, |tx, env| trade::place_order(tx, env, by_dealer).map(|_| ())).unwrap_err().code, "product_mismatch");
    // options trade there
    let t = filled(place(&mut o, &kit, req(vec![leg(C116, Side::Buy, "1")], "o1")).unwrap()).legs[0].ticket;
    // a pre-split CFD position on what is now an Options group: no more volume (dealers neither), but it closes
    let cfd = *c.st.positions.keys().next().unwrap();
    let mut flipped = Kit::new();
    flipped.tenant.groups.get_mut("hedge").unwrap().product = crate::rules::Product::Options;
    flipped.quote("EURUSD", "1.15990", "1.16010");
    assert_eq!(c.run(&flipped, |tx, env| super::dealing::add_volume(tx, env, cfd, d("0.1"), &dealer).map(|_| ())).unwrap_err().code, "product_mismatch");
    c.run(&flipped, |tx, env| trade::close_position(tx, env, cfd, trade::CloseReq::default())).unwrap();
    assert!(c.st.positions.is_empty());
    o.run(&kit, |tx, env| options::close(tx, env, t, OptClose::client())).unwrap();
    c.assert_ledger();
    c.assert_replay();
    o.assert_ledger();
    o.assert_replay();
}

/// A group move never crosses products (CFD / Options account split), whoever asks; within a product it works.
#[test]
fn group_moves_stay_within_the_product() {
    let kit = kit();
    let mut c = Harness::live(&kit, "hedge", "1000");
    let e = c.run(&kit, |tx, env| funds::change_group(tx, env, &kit.tenant.groups["opt"]).map(|_| ())).unwrap_err();
    assert_eq!(e.code, "product_mismatch", "{e}");
    assert!(e.message.starts_with("A CFD account can't move to opt, an Options group"), "{e}");
    c.run(&kit, |tx, env| funds::change_group(tx, env, &kit.tenant.groups["ecn"]).map(|_| ())).unwrap();
    assert_eq!(c.st.account.group, "ecn");
    let mut o = Harness::live(&kit, "opt", "1000");
    assert_eq!(o.run(&kit, |tx, env| funds::change_group(tx, env, &kit.tenant.groups["hedge"]).map(|_| ())).unwrap_err().code, "product_mismatch");
    let mut kit2 = Kit::new();
    kit2.tenant.groups.insert("opt-pro".into(), options_group("opt-pro", crate::model::Mode::Hedging, false));
    o.run(&kit2, |tx, env| funds::change_group(tx, env, &kit2.tenant.groups["opt-pro"]).map(|_| ())).unwrap();
    assert_eq!(o.st.account.group, "opt-pro");
    c.assert_replay();
    o.assert_replay();
}

#[test]
fn combos_fill_and_close_all_or_nothing() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let v0 = h.st.version;
    // second leg above the 100-contract maximum: nothing is booked
    let e = place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1"), leg(P115, Side::Sell, "101")], "x1")).unwrap_err();
    assert_eq!(e.code, "invalid_volume");
    // second leg needs more margin than the account has: nothing is booked either
    let e = place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1"), leg(P115, Side::Sell, "100")], "x2")).unwrap_err();
    assert_eq!(e.code, "insufficient_margin", "{e}");
    assert_eq!((h.st.version, h.st.positions.len(), h.st.balance), (v0, 0, d("10000")));
    // legs on two underlyings are refused
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1"), leg("USDJPY-20261002-150.00-C", Side::Buy, "1")], "x3")).unwrap_err().code, "invalid_order");

    // a bull call spread fills as one strategy
    let f = filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2"), leg(C117, Side::Sell, "2")], "x4")).unwrap());
    let combo = f.combo_id.expect("a combo id");
    assert!(h.st.positions.values().all(|p| p.combo_id == Some(combo)));
    assert_eq!(h.st.positions.len(), 2);
    // one leg's series halted: the whole strategy stays open
    let mut snap = opt_snapshot();
    snap["controls"] = serde_json::json!([{"id": 1, "tenant": "*", "scope": "series", "target": C117, "mode": "halt", "reason": "bad quote"}]);
    kit.options.set_snapshot(snap);
    let e = h.run(&kit, |tx, env| options::close_combo(tx, env, combo, None)).unwrap_err();
    assert_eq!(e.code, "series_halted");
    assert_eq!(h.st.positions.len(), 2);
    kit.options.set_snapshot(opt_snapshot());
    let out = h.run(&kit, |tx, env| options::close_combo(tx, env, combo, None)).unwrap();
    assert_eq!(out.len(), 2);
    assert!(h.st.positions.is_empty());
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn no_opens_in_the_last_15_minutes_and_closes_until_one_minute_before_the_cut() {
    let mut kit = kit();
    let today = "EURUSD-20260928-1.1600-C";
    kit.options.fix(today, "0.0020", "0.0022");
    let mut h = Harness::live(&kit, "opt", "10000");
    let f = filled(place(&mut h, &kit, req(vec![leg(today, Side::Buy, "2")], "a")).unwrap());
    kit.now = t("2026-09-28T13:50:00Z");
    assert_eq!(place(&mut h, &kit, req(vec![leg(today, Side::Buy, "1")], "b")).unwrap_err().code, "cutoff");
    h.run(&kit, |tx, env| options::close(tx, env, f.legs[0].ticket, OptClose { volume: Some(d("1")), ..OptClose::client() })).unwrap();
    kit.now = t("2026-09-28T13:59:30Z");
    assert_eq!(h.run(&kit, |tx, env| options::close(tx, env, f.legs[0].ticket, OptClose::client())).unwrap_err().code, "cutoff");
}

#[test]
fn gates_switches_suitability_limits_controls_staleness_and_sessions() {
    let mut kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    // live switched off for the tenant
    let mut s = opt_snapshot();
    s["tenants"] = serde_json::json!([{"tenant": "kalks", "enabledDemo": true, "enabledLive": false}]);
    kit.options.set_snapshot(s);
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g1")).unwrap_err().code, "options_disabled");
    kit.options.set_snapshot(opt_snapshot());
    // eligibility (the options intro accepted): live and demo accounts alike
    let r = OptOrderReq { eligible: false, ..req(vec![leg(C116, Side::Buy, "1")], "g2") };
    assert_eq!(place(&mut h, &kit, r.clone()).unwrap_err().code, "not_eligible");
    let mut demo = Harness::demo(&kit, "opt");
    assert_eq!(demo.st.account.kind, AccountKind::Demo);
    let e = place(&mut demo, &kit, r.clone()).unwrap_err();
    assert_eq!(e.code, "not_eligible");
    assert_eq!(e.message, options::NOT_ELIGIBLE);
    assert!(place(&mut demo, &kit, OptOrderReq { eligible: true, ..r }).is_ok());
    // client limits from the Back Office
    let mut s = opt_snapshot();
    s["clientLimits"] = serde_json::json!([{"tenant": "kalks", "userId": 7, "blocked": true, "reason": "review"}]);
    kit.options.set_snapshot(s);
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g3")).unwrap_err().code, "not_eligible");
    let mut s = opt_snapshot();
    s["clientLimits"] = serde_json::json!([{"tenant": "kalks", "userId": 7, "maxContracts": 3, "maxShortContracts": 1}]);
    kit.options.set_snapshot(s);
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "g4")).unwrap());
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "g5")).unwrap_err().code, "limit_contracts");
    assert_eq!(place(&mut h, &kit, OptOrderReq { others: (d("1"), ZERO), ..req(vec![leg(C116, Side::Buy, "1")], "g6") }).unwrap_err().code, "limit_contracts", "other accounts count");
    assert_eq!(place(&mut h, &kit, req(vec![leg(C117, Side::Sell, "2")], "g7")).unwrap_err().code, "limit_contracts");
    kit.options.set_snapshot(opt_snapshot());
    let open = *h.st.positions.keys().next().unwrap();
    // a halted series: no opens, no client closes; the engine (stop-out, SL / TP) still closes
    let mut s = opt_snapshot();
    s["controls"] = serde_json::json!([{"id": 1, "tenant": "*", "scope": "underlying", "target": "EURUSD", "mode": "halt"}]);
    kit.options.set_snapshot(s);
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g8")).unwrap_err().code, "series_halted");
    assert_eq!(h.run(&kit, |tx, env| options::close(tx, env, open, OptClose::client())).unwrap_err().code, "series_halted");
    // close-only control: closes allowed, opens refused
    let mut s = opt_snapshot();
    s["controls"] = serde_json::json!([{"id": 2, "tenant": "kalks", "scope": "expiry", "target": KEY, "mode": "close_only"}]);
    kit.options.set_snapshot(s);
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g9")).unwrap_err().code, "close_only");
    h.run(&kit, |tx, env| options::close(tx, env, open, OptClose { volume: Some(d("1")), ..OptClose::client() })).unwrap();
    kit.options.set_snapshot(opt_snapshot());
    // stale snapshot: close-only
    kit.options.stale.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g10")).unwrap_err().code, "stale_prices");
    h.run(&kit, |tx, env| options::close(tx, env, open, OptClose::client())).unwrap();
    kit.options.stale.store(false, std::sync::atomic::Ordering::SeqCst);
    // the weekend: the FX market is closed
    kit.now = t("2026-09-26T12:00:00Z");
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g11")).unwrap_err().code, "market_closed");
    // dealer account controls apply as for CFDs
    kit.now = t("2026-09-28T12:00:00Z");
    h.run(&kit, |tx, _| funds::set_status(tx, crate::model::Status::CloseOnly)).unwrap();
    assert_eq!(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "g12")).unwrap_err().code, "close_only");
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn pending_limit_and_underlying_trigger_orders() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let lim = OptOrderReq { kind: OptKind::Limit, limit_premium: Some(d("0.0045")), ..req(vec![leg(C116, Side::Buy, "1")], "p1") };
    let PlaceOut::Pending { ticket } = place(&mut h, &kit, lim).unwrap() else { panic!("pending") };
    let o = &h.st.orders[&ticket];
    assert!(o.option.is_some() && o.expiry_at == Some(t("2026-10-02T13:45:00Z")), "expires when opening ends");
    h.run(&kit, |tx, env| {
        options::on_underlying(tx, env, "EURUSD", true);
        Ok(())
    })
    .unwrap();
    assert!(h.st.orders.contains_key(&ticket), "not reached yet");
    kit.options.fix(C116, "0.0042", "0.0044");
    h.run(&kit, |tx, env| {
        options::on_underlying(tx, env, "EURUSD", true);
        Ok(())
    })
    .unwrap();
    assert!(h.st.orders.is_empty());
    let p = h.st.positions.values().next().unwrap();
    assert_eq!((p.open_price, p.ticket), (d("0.0044"), ticket), "filled at the market, position takes the order ticket");

    // a market order armed by the underlying
    let trig = OptOrderReq { trigger: Some(Trigger { symbol: "EURUSD".into(), op: TriggerOp::Above, price: d("1.17") }), tif: Expiry::Today, ..req(vec![leg(C117, Side::Buy, "1")], "p2") };
    let PlaceOut::Pending { ticket } = place(&mut h, &kit, trig).unwrap() else { panic!("pending") };
    kit.options.fix(C117, "0.0010", "0.0012");
    kit.options.spot("EURUSD", "1.165", kit.now);
    h.run(&kit, |tx, env| {
        options::on_underlying(tx, env, "EURUSD", true);
        Ok(())
    })
    .unwrap();
    assert!(h.st.orders.contains_key(&ticket));
    kit.options.spot("EURUSD", "1.1712", kit.now);
    h.run(&kit, |tx, env| {
        options::on_underlying(tx, env, "EURUSD", true);
        Ok(())
    })
    .unwrap();
    assert!(!h.st.orders.contains_key(&ticket));
    assert_eq!(h.st.positions.len(), 2);
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn premium_take_profit_and_stop_loss() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let r = OptOrderReq { tp: Some(d("0.0070")), sl: Some(d("0.0030")), ..req(vec![leg(C116, Side::Buy, "1")], "t1") };
    // a TP below the bid is refused
    assert_eq!(place(&mut h, &kit, OptOrderReq { tp: Some(d("0.0040")), ..r.clone() }).unwrap_err().code, "invalid_tp");
    let f = filled(place(&mut h, &kit, r).unwrap());
    kit.options.fix(C116, "0.0071", "0.0073");
    h.run(&kit, |tx, env| {
        options::on_underlying(tx, env, "EURUSD", true);
        Ok(())
    })
    .unwrap();
    assert!(!h.st.positions.contains_key(&f.legs[0].ticket));
    let last = deals_of(&h, f.legs[0].ticket).pop().unwrap();
    assert_eq!((last.reason, last.price, last.profit), (DealReason::Tp, d("0.0071"), d("19.00")));
    h.assert_replay();
}

#[test]
fn barrier_knock_out_pays_the_rebate_once_and_knock_in_flips_once() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let uo = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::UO, level: d("1.18"), rebate: d("0.0005") }), ..leg(C116, Side::Buy, "2") };
    let di = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::DI, level: d("1.14"), rebate: ZERO }), ..leg(P115, Side::Buy, "1") };
    // a barrier already reached is refused; an up-and-out call below its strike can never pay
    let bad = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::UO, level: d("1.155"), rebate: ZERO }), ..leg(C116, Side::Buy, "1") };
    assert_eq!(place(&mut h, &kit, req(vec![bad], "b0")).unwrap_err().code, "invalid_barrier");
    let ko = filled(place(&mut h, &kit, req(vec![uo], "b1")).unwrap()).legs[0].ticket;
    let ki = filled(place(&mut h, &kit, req(vec![di], "b2")).unwrap()).legs[0].ticket;
    assert_eq!(h.st.positions[&ko].option.as_ref().unwrap().style(), "barrier");
    let bal = h.st.balance;

    kit.options.spot("EURUSD", "1.1801", kit.now);
    let knocked = h.run(&kit, |tx, env| Ok(options::knocks(tx, env, "EURUSD"))).unwrap();
    assert_eq!(knocked, vec![ko]);
    assert!(!h.st.positions.contains_key(&ko));
    // rebate 0.0005 × 2 × 10 000 = 10
    assert_eq!(h.st.balance - bal, d("10.00"));
    let d0 = deals_of(&h, ko).pop().unwrap();
    assert_eq!(d0.reason, DealReason::KnockOut);
    let v = h.st.version;
    assert!(h.run(&kit, |tx, env| Ok(options::knocks(tx, env, "EURUSD"))).unwrap().is_empty(), "once");
    assert_eq!(h.st.version, v);

    kit.options.spot("EURUSD", "1.1399", kit.now);
    assert_eq!(h.run(&kit, |tx, env| Ok(options::knocks(tx, env, "EURUSD"))).unwrap(), vec![ki]);
    let b = h.st.positions[&ki].option.as_ref().unwrap().barrier.clone().unwrap();
    assert!(b.knocked_in && b.knocked_at == Some(kit.now));
    let v = h.st.version;
    assert!(h.run(&kit, |tx, env| Ok(options::knocks(tx, env, "EURUSD"))).unwrap().is_empty(), "once");
    assert_eq!(h.st.version, v);
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn settlement_is_idempotent_reruns_net_and_holds_the_proceeds_for_an_hour() {
    let mut kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let long = filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "e1")).unwrap()).legs[0].ticket;
    let di = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::DI, level: d("1.10"), rebate: d("0.0002") }), ..leg(P115, Side::Buy, "1") };
    filled(place(&mut h, &kit, req(vec![di], "e2")).unwrap());
    let bal = h.st.balance;
    // before the cut nothing settles
    let s = h.run(&kit, |tx, env| Ok(options::settle(tx, env, KEY, d("1.17"), 1, None))).unwrap();
    assert!(s.tickets.is_empty());
    kit.now = t("2026-10-02T14:05:00Z");
    let s = h.run(&kit, |tx, env| Ok(options::settle(tx, env, KEY, d("1.17"), 1, None))).unwrap();
    assert_eq!(s.tickets.len(), 2);
    // long call: (1.17 - 1.16) × 2 × 10 000 = 200; the knock-in put never knocked in: rebate 0.0002 × 10 000 = 2
    assert_eq!(h.st.balance - bal, d("202.00"));
    assert!(h.st.positions.is_empty());
    let set = deals_of(&h, long).pop().unwrap();
    assert_eq!((set.reason, set.price, set.profit), (DealReason::Expiry, d("0.01"), d("96.00")));
    assert_eq!(set.option.as_ref().unwrap().run, Some(1));
    let m = metrics(&kit.env(&h.st), &h.st);
    assert_eq!(m.held, d("202.00"));
    assert_eq!(m.withdrawable(), h.st.balance - d("202.00"));
    // settling again (crash catch-up, scheduler repeat) books nothing
    let v = h.st.version;
    assert!(h.run(&kit, |tx, env| Ok(options::settle(tx, env, KEY, d("1.17"), 1, None))).unwrap().tickets.is_empty());
    assert_eq!(h.st.version, v);

    // re-run at a corrected fixing: the difference only
    let before = h.st.balance;
    let prev: Vec<Deal> = h.log.iter().filter_map(|e| if let Event::PositionClosed { deal, .. } = e { (deal.reason == DealReason::Expiry).then(|| deal.clone()) } else { None }).collect();
    kit.now = t("2026-10-02T14:30:00Z");
    let s = h.run(&kit, |tx, env| options::rerun(tx, env, KEY, &prev, d("1.168"), 2, None)).unwrap();
    assert_eq!(s.tickets.len(), 2);
    assert_eq!(h.st.balance - before, d("-40.00")); // 160 instead of 200, the rebate unchanged
    assert!(prev.iter().all(|d| h.st.reversed_deals.contains(&d.id)));
    assert_eq!(metrics(&kit.env(&h.st), &h.st).held, d("162.00"));
    // a second re-run with the same deals does nothing (already reversed)
    let v = h.st.version;
    h.run(&kit, |tx, env| options::rerun(tx, env, KEY, &prev, d("1.168"), 2, None)).unwrap();
    assert_eq!(h.st.version, v);
    // the hold ends an hour after the re-run
    kit.now = t("2026-10-02T15:31:00Z");
    assert_eq!(metrics(&kit.env(&h.st), &h.st).held, ZERO);
    h.assert_ledger();
    h.assert_replay();
}

/// A barrier whose level the fixing itself reached was touched in the fixing window even when no knock was seen
/// (the engine was down, a gap): the knock-out settles at its rebate, the knock-in as the vanilla.
#[test]
fn a_barrier_the_fixing_reached_settles_as_touched() {
    let mut kit2 = kit();
    let mut kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let uo = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::UO, level: d("1.18"), rebate: d("0.0005") }), ..leg(C116, Side::Buy, "2") };
    let di = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::DI, level: d("1.14"), rebate: d("0.0002") }), ..leg(P115, Side::Buy, "1") };
    filled(place(&mut h, &kit, req(vec![uo], "t1")).unwrap());
    filled(place(&mut h, &kit, req(vec![di], "t2")).unwrap());
    kit.now = t("2026-10-02T14:05:00Z");
    // fixing 1.185 ≥ the UO level 1.18: rebate 0.0005 × 2 × 10 000 = 10 (not the 1.185 − 1.16 intrinsic)
    let bal = h.st.balance;
    let s = h.run(&kit, |tx, env| Ok(options::settle(tx, env, KEY, d("1.185"), 1, None))).unwrap();
    assert_eq!(s.tickets.len(), 2);
    // + the DI put that never knocked in (1.185 > 1.14): its rebate 0.0002 × 10 000 = 2
    assert_eq!(h.st.balance - bal, d("12.00"));
    h.assert_ledger();

    let mut h2 = Harness::live(&kit2, "opt", "10000");
    let di = LegReq { barrier: Some(BarrierReq { kind: BarrierKind::DI, level: d("1.14"), rebate: d("0.0002") }), ..leg(P115, Side::Buy, "1") };
    filled(place(&mut h2, &kit2, req(vec![di], "t3")).unwrap());
    kit2.now = t("2026-10-02T14:05:00Z");
    // fixing 1.135 ≤ the DI level 1.14, never knocked in: it is the vanilla put, (1.15 − 1.135) × 10 000 = 150
    let bal = h2.st.balance;
    h2.run(&kit2, |tx, env| Ok(options::settle(tx, env, KEY, d("1.135"), 1, None))).unwrap();
    assert_eq!(h2.st.balance - bal, d("150.00"));
    h2.assert_ledger();
    h2.assert_replay();
}

#[test]
fn a_short_itm_settlement_charges_the_client_and_nbp_covers_a_deficit() {
    let mut kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Sell, "1")], "n1")).unwrap());
    let bal = h.st.balance;
    kit.now = t("2026-10-02T14:05:00Z");
    h.run(&kit, |tx, env| Ok(options::settle(tx, env, KEY, d("1.19"), 1, None))).unwrap();
    assert_eq!(h.st.balance - bal, d("-300.00"));
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn void_reverses_premium_proceeds_and_commission_as_corrections() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let t0 = filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "v1")).unwrap()).legs[0].ticket;
    kit.options.fix(C116, "0.0060", "0.0062");
    h.run(&kit, |tx, env| options::close(tx, env, t0, OptClose { volume: Some(d("1")), ..OptClose::client() })).unwrap();
    let deals = deals_of(&h, t0);
    let dealer = trade::DealerCtx { staff: "Dealer".into(), reason_code: "DLR-02".into(), force: false };
    h.run(&kit, |tx, env| options::void(tx, env, t0, &deals, &dealer)).unwrap();
    assert_eq!(h.st.balance, d("10000.00"));
    assert!(h.st.positions.is_empty());
    assert!(deals.iter().all(|d| h.st.reversed_deals.contains(&d.id)));
    assert_eq!(h.run(&kit, |tx, env| options::void(tx, env, t0, &deals, &dealer)).unwrap_err().code, "not_found", "only once");
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn quote_currency_premiums_convert_to_usd() {
    let kit = kit();
    let s = "USDJPY-20261002-150.00-C";
    kit.options.fix(s, "1.00", "1.02");
    let mut h = Harness::live(&kit, "opt", "10000");
    let f = filled(place(&mut h, &kit, req(vec![leg(s, Side::Buy, "1")], "j1")).unwrap());
    // 1.02 JPY × 10 000 / 150 = 68.00 USD
    assert_eq!((f.legs[0].premium, f.legs[0].commission), (d("68.00"), d("0.25")));
    assert_eq!(h.st.balance, d("9931.75"));
    h.assert_ledger();
}

#[test]
fn preview_prices_like_the_fill_and_changes_nothing() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let r = req(vec![leg(C116, Side::Buy, "2"), leg(C117, Side::Sell, "2")], "pv");
    kit.options.fix(C117, "0.0010", "0.0012");
    let p = options::preview(&kit.env(&h.st), &h.st, &r);
    assert!(p.reasons.is_empty(), "{:?}", p.reasons);
    // debit 104 - 20 = 84; commission 0.50 + min(0.50, 10 % × 20 = 2) = 1.00
    assert_eq!((p.net_premium, p.commission, p.cash_after), (d("84.00"), d("1.00"), d("9915.00")));
    // a bull call spread: max loss = debit + commission, max profit = width - debit - commission
    assert_eq!((p.max_loss, p.max_profit), (Some(d("85.00")), Some(d("115.00"))));
    assert_eq!(p.breakevens, vec![d("1.1642")]);
    assert_eq!(h.st.balance, d("10000"));
    let f = filled(place(&mut h, &kit, r).unwrap());
    assert_eq!(f.legs.iter().map(|l| l.premium).collect::<Vec<_>>(), vec![d("104.00"), d("20.00")]);
    assert_eq!(h.st.balance, p.cash_after);
    // a refused order lists its reasons
    let small = Harness::live(&kit, "opt", "100");
    let p = options::preview(&kit.env(&small.st), &small.st, &req(vec![leg(C116, Side::Buy, "10")], "pv2"));
    assert_eq!(p.reasons.iter().map(|r| r.code).collect::<Vec<_>>(), vec!["insufficient_cash"]);
    let p = options::preview(&kit.env(&small.st), &small.st, &req(vec![leg(C116, Side::Sell, "10")], "pv3"));
    assert_eq!(p.reasons.iter().map(|r| r.code).collect::<Vec<_>>(), vec!["insufficient_margin"]);
    assert!(p.margin_after > p.margin_before);
}

#[test]
fn stop_out_closes_by_units_keeps_strategies_together_and_terminates() {
    let kit = Kit::new(); // real model prices: values follow the spot
    let mut h = Harness::live(&kit, "opt", "3000");
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Sell, "4")], "so1")).unwrap());
    let combo = filled(place(&mut h, &kit, req(vec![leg("EURUSD-20261002-1.1650-C", Side::Sell, "2"), leg(C117, Side::Buy, "2")], "so2")).unwrap()).combo_id.unwrap();
    let level0 = metrics(&kit.env(&h.st), &h.st).level.unwrap();
    assert!(level0 > d("100"), "{level0}");
    kit.options.spot("EURUSD", "1.235", kit.now);
    h.run(&kit, |tx, env| {
        risk::check_margin(tx, env);
        Ok(())
    })
    .unwrap();
    assert!(h.log.iter().any(|e| matches!(e, Event::StopOut { .. })), "stop-out ran");
    let legs = h.st.positions.values().filter(|p| p.combo_id == Some(combo)).count();
    assert!(legs == 0 || legs == 2, "a strategy closes with all its legs");
    let m = metrics(&kit.env(&h.st), &h.st);
    assert!(m.level.is_none_or(|l| l > d("50")) || h.st.positions.is_empty(), "{m:?}");
    // nothing closable (the weekend): no stop-out is recorded, however often the margin is checked
    let mut kit = kit;
    let mut w = Harness::live(&kit, "opt", "3000");
    kit.options.spot("EURUSD", "1.16", kit.now);
    filled(place(&mut w, &kit, req(vec![leg(C116, Side::Sell, "4")], "w1")).unwrap());
    kit.options.spot("EURUSD", "1.235", kit.now);
    kit.now = t("2026-09-26T12:00:00Z");
    assert!(metrics(&kit.env(&w.st), &w.st).level.is_some_and(|l| l <= d("50")));
    let v = w.st.version;
    for _ in 0..3 {
        w.run(&kit, |tx, env| {
            options::on_timer(tx, env);
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(w.st.version, v, "no events while nothing can be closed");
    h.assert_ledger();
    h.assert_replay();
}

#[test]
fn metrics_count_every_position_cfd_and_option() {
    // an account from before the CFD / Options account split may hold both: its group was a CFD group then
    let mut kit = kit();
    kit.tenant.groups.get_mut("opt").unwrap().product = crate::rules::Product::Cfd;
    let mut h = Harness::live(&kit, "opt", "10000");
    h.run(&kit, |tx, env| trade::place_order(tx, env, trade::OrderReq::market("EURUSD", Side::Buy, d("0.1"))).map(|_| ())).unwrap();
    kit.tenant.groups.get_mut("opt").unwrap().product = crate::rules::Product::Options;
    filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], "m1")).unwrap());
    filled(place(&mut h, &kit, req(vec![leg(C117, Side::Sell, "1")], "m2")).unwrap());
    let env = kit.env(&h.st);
    let m = metrics(&env, &h.st);
    assert_eq!(m.unpriced, 0);
    let options: D = h.st.positions.values().filter(|p| p.option.is_some()).map(|p| options::position_value(&env, &h.st.account, p, p.option.as_ref().unwrap())).sum();
    assert_eq!(m.option_value, options);
    assert_eq!(h.st.positions.values().filter(|p| p.option.is_some()).count(), 2);
    // without a snapshot the options are still valued: the long at its intrinsic value (0 here), the short at
    // least at the premium it received; short options keep a (fallback) margin
    let saved = kit.options.snap.lock().unwrap().take();
    kit.options.spots.lock().unwrap().clear();
    let m2 = metrics(&kit.env(&h.st), &h.st);
    let short = h.st.positions.values().find(|p| p.option.is_some() && p.side == Side::Sell).unwrap();
    assert_eq!(r2(m2.option_value), r2(-short.open_price * d("10000")), "{m2:?}");
    assert!(m2.option_margin > ZERO);
    // and without any price at all, both at the premium they were opened at
    kit.quotes.0.lock().unwrap().clear();
    let m3 = metrics(&kit.env(&h.st), &h.st);
    let at_open: D = h.st.positions.values().filter(|p| p.option.is_some()).map(|p| p.open_price * p.volume * d("10000") * p.side.sign()).sum();
    assert_eq!(r2(m3.option_value), r2(at_open));
    assert_eq!(m3.unpriced, 1, "the CFD without a quote is reported");
    *kit.options.snap.lock().unwrap() = saved;
}

#[test]
fn copy_pamm_mam_and_prop_accounts_never_trade_options() {
    let mut kit = kit();
    for code in ["prop", "prop-50k", "PROP_Funded", "copy", "copy-netting", "copy-demo", "pamm", "mam"] {
        kit.tenant.groups.insert(code.into(), group(code, crate::model::Mode::Hedging, false));
    }
    for code in ["prop", "prop-50k", "PROP_Funded", "copy", "copy-netting", "copy-demo", "pamm", "mam"] {
        assert!(options::system_group(code), "{code}");
        let mut h = Harness::live(&kit, code, "10000");
        let e = place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "1")], &format!("sys-{code}"))).unwrap_err();
        assert_eq!(e.code, "options_disabled", "{code}: {e}");
        // a pending limit order is refused at placement too, and the preview lists the refusal
        let pending = OptOrderReq { kind: OptKind::Limit, limit_premium: Some(d("0.0010")), ..req(vec![leg(C116, Side::Buy, "1")], &format!("sys-lim-{code}")) };
        assert_eq!(place(&mut h, &kit, pending).unwrap_err().code, "options_disabled", "{code}");
        let pv = options::preview(&kit.env(&h.st), &h.st, &req(vec![leg(C116, Side::Sell, "1")], "pv"));
        assert!(pv.reasons.iter().any(|r| r.code == "options_disabled"), "{code}: {:?}", pv.reasons);
        assert!(h.st.positions.is_empty() && h.st.orders.is_empty());
        assert_eq!(h.st.balance, d("10000"), "{code}: no premium, no commission");
        h.assert_ledger();
    }
    // ordinary groups (even ones whose names merely contain those words) still trade
    for code in ["standard-copytrader", "hedge"] {
        assert!(!options::system_group(code), "{code}");
    }
    kit.tenant.groups.insert("standard-copytrader".into(), options_group("standard-copytrader", crate::model::Mode::Hedging, false));
    let mut ok = Harness::live(&kit, "standard-copytrader", "10000");
    filled(place(&mut ok, &kit, req(vec![leg(C116, Side::Buy, "1")], "ok-1")).unwrap());
}

#[test]
fn deal_feeds_flag_option_deals_for_downstream_consumers() {
    let kit = kit();
    let mut h = Harness::live(&kit, "opt", "10000");
    let f = filled(place(&mut h, &kit, req(vec![leg(C116, Side::Buy, "2")], "feed-1")).unwrap());
    let t = f.legs[0].ticket;
    h.run(&kit, |tx, env| options::close(tx, env, t, OptClose::client())).unwrap();
    // CFDs trade on a CFD account
    let mut c = Harness::live(&kit, "hedge", "10000");
    c.run(&kit, |tx, env| trade::place_order(tx, env, trade::OrderReq::market("EURUSD", Side::Buy, d("0.1"))).map(|_| ())).unwrap();
    let cfd = *c.st.positions.keys().next().unwrap();
    c.run(&kit, |tx, env| trade::close_position(tx, env, cfd, trade::CloseReq::default())).unwrap();
    let opt_deals = deals_of(&h, t);
    assert_eq!(opt_deals.len(), 2);
    for dl in &opt_deals {
        // the dealing feed (IB, growth, prop) and the client history (reports, algo) both carry the flag
        for v in [crate::views::desk_deal_json(dl, 7, false), crate::views::deal_json(dl)] {
            assert_eq!(v["instrument"], "option");
            assert_eq!(v["option"]["series"], C116);
            assert_eq!(v["volume"].as_f64(), Some(2.0), "volume = contracts");
        }
    }
    let exit = opt_deals.iter().find(|x| x.entry != crate::model::DealEntry::In).unwrap();
    assert_eq!(crate::views::desk_deal_json(exit, 7, false)["option"]["commissionCharged"].as_f64(), Some(0.5), "the exit's own commission");
    for dl in deals_of(&c, cfd) {
        for v in [crate::views::desk_deal_json(&dl, 7, false), crate::views::deal_json(&dl)] {
            assert_eq!(v["instrument"], "cfd");
            assert!(v["option"].is_null());
        }
    }
}

#[test]
fn old_events_without_option_fields_still_read_and_write_the_same() {
    let kit = kit();
    let mut h = Harness::live(&kit, "hedge", "10000");
    h.run(&kit, |tx, env| trade::place_order(tx, env, trade::OrderReq::market("EURUSD", Side::Buy, d("0.1"))).map(|_| ())).unwrap();
    for e in &h.log {
        let v = serde_json::to_value(e).unwrap();
        let s = v.to_string();
        assert!(!s.contains("\"option\"") && !s.contains("combo_id") && !s.contains("\"premium\"") && !s.contains("holds"), "CFD events are unchanged: {s}");
        // the CFD / Options account split lives in the group, never in an event
        assert!(!s.contains("product"), "no product in events: {s}");
        assert_eq!(&serde_json::from_value::<Event>(v).unwrap(), e);
    }
    assert!(!serde_json::to_string(&h.st).unwrap().contains("holds"));
}

mod property {
    use super::*;
    use proptest::prelude::*;

    #[derive(Debug, Clone)]
    enum Op {
        Open(usize, bool, u8),
        Combo(u8),
        Close(usize, bool),
        Spot(i32),
        Knocks,
        Tick,
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0usize..5, any::<bool>(), 1u8..4).prop_map(|(s, b, c)| Op::Open(s, b, c)),
            (1u8..3).prop_map(Op::Combo),
            (0usize..8, any::<bool>()).prop_map(|(i, p)| Op::Close(i, p)),
            (-150i32..150).prop_map(Op::Spot),
            Just(Op::Knocks),
            Just(Op::Tick),
        ]
    }

    const SERIES: [&str; 5] = ["EURUSD-20261002-1.1500-C", "EURUSD-20261002-1.1600-C", "EURUSD-20261002-1.1700-C", "EURUSD-20261002-1.1500-P", "EURUSD-20261002-1.1650-P"];

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        /// Any sequence of option trades, combos, closes, spot moves, knocks and ticks keeps every ledger
        /// transaction balanced, the equity identity, combos whole after stop-outs, and replays identically.
        #[test]
        fn ledger_equity_and_replay_hold(ops in proptest::collection::vec(op(), 1..18)) {
            let kit = Kit::new();
            let mut h = Harness::live(&kit, "opt", "20000");
            let mut n = 0;
            for o in ops {
                n += 1;
                let cid = format!("p{n}");
                match o {
                    Op::Open(s, buy, c) => {
                        let mut l = leg(SERIES[s], if buy { Side::Buy } else { Side::Sell }, &c.to_string());
                        if s == 1 && buy {
                            l.barrier = Some(BarrierReq { kind: BarrierKind::UO, level: d("1.175"), rebate: d("0.0001") });
                        }
                        let _ = place(&mut h, &kit, req(vec![l], &cid));
                    }
                    Op::Combo(c) => {
                        let _ = place(&mut h, &kit, req(vec![leg(SERIES[1], Side::Buy, &c.to_string()), leg(SERIES[2], Side::Sell, &c.to_string())], &cid));
                    }
                    Op::Close(i, part) => {
                        let tickets: Vec<i64> = h.st.positions.keys().copied().collect();
                        if !tickets.is_empty() {
                            let t = tickets[i % tickets.len()];
                            let v = if part && h.st.positions[&t].volume > d("1") { Some(d("1")) } else { None };
                            let _ = h.run(&kit, |tx, env| options::close(tx, env, t, OptClose { volume: v, ..OptClose::client() }));
                        }
                    }
                    Op::Spot(bp) => kit.options.spot("EURUSD", &(d("1.16") + D::from(bp) / D::from(10_000)).to_string(), kit.now),
                    Op::Knocks => { let _ = h.run(&kit, |tx, env| Ok(options::knocks(tx, env, "EURUSD"))); }
                    Op::Tick => { let _ = h.run(&kit, |tx, env| { options::on_underlying(tx, env, "EURUSD", true); Ok(()) }); }
                }
                let m = metrics(&kit.env(&h.st), &h.st);
                prop_assert_eq!(m.equity, m.balance + m.credit + m.bonus + (m.profit - m.option_pnl) + m.swap + m.option_value);
                let mut combos: std::collections::BTreeMap<i64, usize> = Default::default();
                for p in h.st.positions.values() {
                    if let Some(c) = p.combo_id { *combos.entry(c).or_default() += 1; }
                }
                prop_assert!(combos.values().all(|n| *n <= 2));
            }
            h.assert_ledger();
            h.assert_replay();
        }
    }
}

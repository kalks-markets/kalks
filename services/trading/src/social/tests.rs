//! Copy-trading engine tests: a master and a follower account run through the real engine (testkit
//! harness); every master transaction's events are mirrored onto the follower exactly as the copier does.
//! Covers sizing, SL/TP, partial and full closes, pending orders and their fills, netting add / reversal,
//! exclusions, pause, idempotency (a master event mirrored twice executes once), close-all, the equity stop /
//! drawdown rule, and that the follower's own event log replays to the same state.

use super::math::{Sizing, SizingMode};
use super::mirror::{self, LogEntry, MirrorCfg};
use crate::engine::funds::{self, Direction};
use crate::engine::testkit::{Harness, Kit, d};
use crate::engine::trade::{self, CloseReq, OrderReq, PlaceResult, PositionPatch};
use crate::engine::{metrics, risk};
use crate::model::{AccountKind, OrderType, Side, Source};
use crate::money::D;
use crate::state::{AccountState, Event};

fn live(kit: &Kit, login: i64, group: &str, balance: &str) -> Harness {
    let acc = kit.account(login, group, AccountKind::Live);
    let st0 = AccountState::new(acc.clone());
    let tx = funds::open_account(&kit.env(&st0), acc);
    let mut h = Harness { st: tx.st.clone(), log: tx.events.clone() };
    let key = format!("fund-{login}");
    h.run(kit, |tx, env| funds::transfer(tx, env, Direction::In, d(balance), &key, None).map(|_| ())).unwrap();
    h
}

struct Pair {
    m: Harness,
    f: Harness,
    seen: usize,
    cfg: MirrorCfg,
}

impl Pair {
    fn new(kit: &Kit, group: &str, master: &str, follower: &str, sizing: Sizing) -> Self {
        let m = live(kit, 10_000_001, group, master);
        let f = live(kit, 10_000_002, group, follower);
        let seen = m.log.len();
        let cfg = MirrorCfg { sub_id: 7, sizing, max_lot: None, excluded: vec![], master: "Gold Swing".into(), opens: true, catch_up: false, master_equity_usd: D::ZERO, mam: None, auto_sl_pips: None };
        Pair { m, f, seen, cfg }
    }

    fn master<T>(&mut self, kit: &Kit, op: impl FnOnce(&mut crate::engine::Tx, &crate::engine::Env) -> Result<T, crate::engine::Reject>) -> T {
        self.m.run(kit, op).unwrap()
    }

    /// Master events not mirrored yet, with their stream versions.
    fn pending(&mut self) -> Vec<(i64, Event)> {
        let evs = self.m.log[self.seen..].iter().enumerate().map(|(i, e)| ((self.seen + i + 1) as i64, e.clone())).collect();
        self.seen = self.m.log.len();
        evs
    }

    fn mirror_events(&mut self, kit: &Kit, evs: &[(i64, Event)]) -> Vec<LogEntry> {
        let eq = metrics(&kit.env(&self.m.st), &self.m.st).equity;
        let cfg = MirrorCfg { master_equity_usd: eq, ..self.cfg.clone() };
        self.f
            .run(kit, |tx, env| {
                let mut out = Vec::new();
                for (v, e) in evs {
                    out.extend(mirror::mirror(tx, env, &cfg, *v, kit.now, e));
                }
                Ok(out)
            })
            .unwrap()
    }

    fn sync(&mut self, kit: &Kit) -> Vec<LogEntry> {
        let evs = self.pending();
        self.mirror_events(kit, &evs)
    }

    fn fpos(&self) -> Vec<&crate::model::Position> {
        self.f.st.positions.values().collect()
    }
}

fn filled(r: &PlaceResult) -> i64 {
    match r {
        PlaceResult::Filled { position_ticket: Some(t), .. } => *t,
        PlaceResult::Pending { ticket, .. } => *ticket,
        other => panic!("unexpected {other:?}"),
    }
}

fn equity() -> Sizing {
    Sizing { mode: SizingMode::Equity, value: D::ONE }
}

#[test]
fn mirrors_open_sltp_partial_and_full_close_idempotently() {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    let mut p = Pair::new(&kit, "hedge", "10000", "2500", equity());

    // master buys 1.00 lot with SL/TP → follower 0.25 (2 500 / 10 000), same levels, source copy
    let mt = filled(&p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq { sl: Some(d("1.09")), tp: Some(d("1.12")), ..OrderReq::market("EURUSD", Side::Buy, d("1")) })));
    let open_evs = p.pending();
    let log = p.mirror_events(&kit, &open_evs);
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!((log[0].action, log[0].status), ("open", "done"));
    let fp = p.fpos()[0].clone();
    assert_eq!((fp.volume, fp.side, fp.sl, fp.tp, fp.source), (d("0.25"), Side::Buy, Some(d("1.09")), Some(d("1.12")), Source::Copy));
    assert_eq!(fp.comment, format!("copy #{mt}"));
    assert_eq!(mirror::linked_position(&p.f.st, 7, mt), Some(fp.ticket));

    // the same master event mirrored again (restart, duplicate delivery) does nothing
    let again = p.mirror_events(&kit, &open_evs);
    assert!(again.is_empty() || again.iter().all(|e| e.status == "skipped"), "{again:?}");
    assert_eq!(p.f.st.positions.len(), 1);

    // SL change
    p.master(&kit, |tx, env| trade::modify_position(tx, env, mt, PositionPatch { sl: Some(Some(d("1.095"))), ..Default::default() }, None));
    let log = p.sync(&kit);
    assert_eq!((log[0].action, log[0].status), ("modify", "done"));
    assert_eq!(p.fpos()[0].sl, Some(d("1.095")));

    // master closes 0.40 of 1.00 → follower closes 0.10 of 0.25
    kit.quote("EURUSD", "1.10100", "1.10110");
    p.master(&kit, |tx, env| trade::close_position(tx, env, mt, CloseReq { volume: Some(d("0.4")), ..Default::default() }));
    let part_evs = p.pending();
    let log = p.mirror_events(&kit, &part_evs);
    assert_eq!((log[0].action, log[0].status, log[0].volume), ("partial_close", "done", Some(d("0.1"))));
    assert_eq!(p.fpos()[0].volume, d("0.15"));
    // replaying the partial close does not close again
    p.mirror_events(&kit, &part_evs);
    assert_eq!(p.fpos()[0].volume, d("0.15"));

    // full close
    p.master(&kit, |tx, env| trade::close_position(tx, env, mt, CloseReq::default()));
    let log = p.sync(&kit);
    assert_eq!((log[0].action, log[0].status), ("close", "done"));
    assert!(p.f.st.positions.is_empty());
    // follower P&L: 0.25 lot, +0.9 pips on 0.1 and +0.9 pips on 0.15 after the spread → positive
    assert!(p.f.st.balance > d("2500"));
    let exits: Vec<_> = p.f.log.iter().filter_map(|e| if let Event::PositionClosed { deal, .. } = e { Some(deal.clone()) } else { None }).collect();
    assert_eq!(exits.len(), 2);
    assert!(exits.iter().all(|x| x.source == Source::Copy && x.client_order_id.as_deref().is_some_and(|k| k.starts_with("cx7:"))));
    p.f.assert_replay();
    p.f.assert_ledger();
}

#[test]
fn mirrors_pending_orders_modify_cancel_and_fills() {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    let mut p = Pair::new(&kit, "hedge", "10000", "5000", equity());

    // placed → modified → cancelled
    let o1 = filled(&p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq { kind: OrderType::Limit, price: Some(d("1.09")), ..OrderReq::market("EURUSD", Side::Buy, d("1")) })));
    let log = p.sync(&kit);
    assert_eq!((log[0].action, log[0].status), ("order", "done"));
    let fo = p.f.st.orders.values().next().unwrap().clone();
    assert_eq!((fo.volume, fo.price, fo.kind, fo.source), (d("0.5"), d("1.09"), OrderType::Limit, Source::Copy));
    p.master(&kit, |tx, env| trade::modify_order(tx, env, o1, trade::OrderPatch { price: Some(d("1.095")), ..Default::default() }, None));
    p.sync(&kit);
    assert_eq!(p.f.st.orders[&fo.ticket].price, d("1.095"));
    p.master(&kit, |tx, env| trade::cancel_order(tx, env, o1, "client"));
    let log = p.sync(&kit);
    assert_eq!((log[0].action, log[0].status), ("cancel", "done"));
    assert!(p.f.st.orders.is_empty());

    // a limit both fill on their own tick: the master's fill does not open a second follower position
    let o2 = filled(&p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq { kind: OrderType::Limit, price: Some(d("1.099")), ..OrderReq::market("EURUSD", Side::Buy, d("1")) })));
    p.sync(&kit);
    kit.quote("EURUSD", "1.09880", "1.09890");
    p.m.tick(&kit, "EURUSD");
    p.f.tick(&kit, "EURUSD");
    assert_eq!(p.f.st.positions.len(), 1);
    let log = p.sync(&kit);
    assert!(log.iter().any(|e| e.message.contains("already filled")), "{log:?}");
    assert_eq!(p.f.st.positions.len(), 1);
    // and the master's close closes it (linked through the order key)
    p.master(&kit, |tx, env| trade::close_position(tx, env, o2, CloseReq::default()));
    p.sync(&kit);
    assert!(p.f.st.positions.is_empty());

    // the master's order fills but the follower's copy has not: the copy is replaced by a market fill
    kit.quote("EURUSD", "1.10000", "1.10010");
    let o3 = filled(&p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq { kind: OrderType::Stop, price: Some(d("1.1010")), ..OrderReq::market("EURUSD", Side::Buy, d("1")) })));
    p.sync(&kit);
    kit.quote("EURUSD", "1.10100", "1.10110");
    p.m.tick(&kit, "EURUSD"); // only the master sees the tick yet
    let log = p.sync(&kit);
    assert_eq!(log.iter().map(|e| (e.action, e.status)).collect::<Vec<_>>(), vec![("cancel", "done"), ("open", "done")]);
    assert!(p.f.st.orders.is_empty());
    assert_eq!(mirror::linked_position(&p.f.st, 7, o3).map(|t| p.f.st.positions[&t].volume), Some(d("0.5")));
    p.f.assert_replay();
    p.f.assert_ledger();
}

#[test]
fn netting_add_and_reversal_follow_the_master() {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    let mut p = Pair::new(&kit, "net", "10000", "5000", equity());
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("1"))));
    p.sync(&kit);
    assert_eq!(p.fpos()[0].volume, d("0.5"));
    // add 0.5 → follower adds 0.25
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("0.5"))));
    let log = p.sync(&kit);
    assert_eq!((log[0].action, log[0].status), ("add", "done"), "{log:?}");
    assert_eq!(p.fpos()[0].volume, d("0.75"));
    // sell 3 → master reverses to short 1.5; follower closes 0.75 and opens short 0.75
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Sell, d("3"))));
    let log = p.sync(&kit);
    assert_eq!(log.iter().map(|e| e.action).collect::<Vec<_>>(), vec!["close", "open"], "{log:?}");
    let fp = p.fpos();
    assert_eq!(fp.len(), 1);
    assert_eq!((fp[0].side, fp[0].volume), (Side::Sell, d("0.75")));
    // partial reduction by an opposite order: buy 0.5 of the 1.5 short → follower buys back 0.25
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("0.5"))));
    p.sync(&kit);
    assert_eq!(p.fpos()[0].volume, d("0.5"));
    p.f.assert_replay();
    p.f.assert_ledger();
}

#[test]
fn sizing_modes_exclusions_and_pause() {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    kit.quote("BTCUSD", "80000", "80020");
    let mut p = Pair::new(&kit, "hedge", "10000", "3000", Sizing { mode: SizingMode::FixedLot, value: d("0.1") });
    p.cfg.excluded = vec!["BTCUSD".into()];
    let t = filled(&p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("2")))));
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("BTCUSD", Side::Buy, d("0.1"))));
    let log = p.sync(&kit);
    assert_eq!(log.iter().map(|e| (e.action, e.status)).collect::<Vec<_>>(), vec![("open", "done"), ("open", "skipped")]);
    assert_eq!(p.fpos().len(), 1);
    assert_eq!(p.fpos()[0].volume, d("0.1"));
    // paused: new opens are skipped but exits still follow
    p.cfg.opens = false;
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Sell, d("1"))));
    let log = p.sync(&kit);
    assert_eq!((log[0].status, log[0].message.as_str()), ("skipped", "copying is paused"));
    // fixed lot: master closes half of 2.00 → follower 0.05 of 0.10
    p.master(&kit, |tx, env| trade::close_position(tx, env, t, CloseReq { volume: Some(d("1")), ..Default::default() }));
    p.sync(&kit);
    assert_eq!(p.fpos()[0].volume, d("0.05"));
    // multiplier with a max lot per trade
    p.cfg.opens = true;
    p.cfg.sizing = Sizing { mode: SizingMode::Multiplier, value: d("2") };
    p.cfg.max_lot = Some(d("0.3"));
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("0.5"))));
    p.sync(&kit);
    assert!(p.fpos().iter().any(|x| x.volume == d("0.3")));
    p.f.assert_replay();
}

#[test]
fn equity_stop_drawdown_and_close_all() {
    assert_eq!(mirror::breach(d("900"), d("1000"), Some(d("950")), None), Some("equity_stop"));
    assert_eq!(mirror::breach(d("960"), d("1000"), Some(d("950")), None), None);
    assert_eq!(mirror::breach(d("790"), d("1000"), None, Some(d("20"))), Some("max_dd"));
    assert_eq!(mirror::breach(d("810"), d("1000"), None, Some(d("20"))), None);
    assert_eq!(mirror::breach(d("0"), d("0"), None, Some(d("20"))), None);

    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    let mut p = Pair::new(&kit, "hedge", "10000", "1000", equity());
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("5"))));
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq { kind: OrderType::Limit, price: Some(d("1.09")), ..OrderReq::market("EURUSD", Side::Buy, d("1")) }));
    p.sync(&kit);
    assert_eq!((p.f.st.positions.len(), p.f.st.orders.len()), (1, 1));
    // price falls 61 pips: the follower (0.5 lot) loses about 305 → equity ≈ 695; a 3 % drawdown limit trips
    kit.quote("EURUSD", "1.09400", "1.09410");
    let eq = metrics(&kit.env(&p.f.st), &p.f.st).equity;
    assert_eq!(mirror::breach(eq, d("1000"), None, Some(d("3"))), Some("max_dd"));
    let (done, failed) = p.f.run(&kit, |tx, env| Ok(mirror::close_all(tx, env, 7, "max_dd"))).unwrap();
    assert!(failed.is_empty());
    assert_eq!(done.len(), 2);
    assert!(p.f.st.positions.is_empty() && p.f.st.orders.is_empty());
    // the master's later close has nothing to mirror
    let log = p.sync(&kit);
    assert!(log.is_empty(), "{log:?}");
    p.f.assert_replay();
    p.f.assert_ledger();
    let _ = risk::check_margin;
}

#[test]
fn deal_origin_is_the_source_that_opened_the_position() {
    // IB ledger (services/ib): a copied position closed by its own SL is a `system` close, but the trade is a copy
    let kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10010");
    let mut p = Pair::new(&kit, "hedge", "10000", "2500", equity());
    p.master(&kit, |tx, env| trade::place_order(tx, env, OrderReq { sl: Some(d("1.09")), ..OrderReq::market("EURUSD", Side::Buy, d("1")) }));
    p.sync(&kit);
    assert_eq!(p.fpos()[0].source, Source::Copy);
    kit.quote("EURUSD", "1.08900", "1.08910");
    p.f.tick(&kit, "EURUSD");
    assert!(p.f.st.positions.is_empty(), "the copied position hit its SL");
    let exit = p.f.log.iter().rev().find_map(|e| if let Event::PositionClosed { deal, .. } = e { Some(deal.clone()) } else { None }).unwrap();
    assert_eq!((exit.reason, exit.source, exit.origin()), (crate::model::DealReason::Sl, Source::System, Source::Copy));
    let v = crate::views::desk_deal_json(&exit, 1, false);
    assert_eq!((v["source"].as_str(), v["closeSource"].as_str()), (Some("copy"), Some("system")));
    // the master's own trade (closed by the same SL) is the client's: `manual`
    p.m.tick(&kit, "EURUSD");
    let own = p.m.log.iter().rev().find_map(|e| if let Event::PositionClosed { deal, .. } = e { Some(deal.clone()) } else { None }).unwrap();
    assert_eq!(crate::views::desk_deal_json(&own, 1, false)["source"], "manual");
}

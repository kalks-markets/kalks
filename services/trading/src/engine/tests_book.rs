//! The options order book on the account side (pure: the engine harness, the matching function, the outbox items
//! applied in order — no database, no services): entry gates and reservations, fills through the clearing
//! account (USD and cent accounts), maker rebate / taker fee, FIFO netting into one book position, idempotent
//! re-application, pro-rata release and reserve = 0 when idle, the account replay, and a proptest over random
//! order flows between three accounts (every transaction balances, clearing nets to 0 per fill, Σ long = Σ short
//! = the book's positions, reserve 0 once idle, replay identical).

use proptest::prelude::*;
use std::collections::BTreeMap;

use super::options_book::{self as ob, BookReq, Entered};
use super::testkit::{Harness, Kit, d, opt_snapshot};
use super::{Tx, metrics};
use crate::book::matching::apply;
use crate::book::outbox::{Item, items_of};
use crate::book::types::*;
use crate::model::{AccountKind, Side};
use crate::money::{D, ZERO};
use crate::state::{AccountState, Event};

const C116: &str = "EURUSD-20261002-1.1600-C";
const CLEARING: &str = "house:options_clearing.EURUSD.20261002:USD";

fn kit() -> Kit {
    let kit = Kit::new();
    kit.quote("EURUSD", "1.15990", "1.16010");
    kit.quote("USDJPY", "149.990", "150.010");
    kit.options.fix(C116, "0.0050", "0.0052");
    // book fees: maker rebate 0.05, taker 0.30 per contract, cap 10 % of the premium
    let mut s = opt_snapshot();
    s["groups"][0]["makerFeePerContract"] = serde_json::json!(-0.05);
    s["groups"][0]["takerFeePerContract"] = serde_json::json!(0.30);
    kit.options.set_snapshot(s);
    kit
}

fn account(kit: &Kit, login: i64, user: i64, group: &str, usd: &str) -> Harness {
    let mut acc = kit.account(login, group, AccountKind::Live);
    acc.user_id = user;
    let st0 = AccountState::new(acc.clone());
    let tx = super::funds::open_account(&kit.env(&st0), acc);
    let mut h = Harness { st: tx.st.clone(), log: tx.events.clone() };
    h.run(kit, |tx, env| super::funds::transfer(tx, env, super::funds::Direction::In, d(usd), &format!("fund-{login}"), None).map(|_| ())).unwrap();
    h
}

struct World {
    kit: Kit,
    accts: BTreeMap<i64, Harness>,
    book: UnderlyingBooks,
}

impl World {
    fn new(kit: Kit, accts: Vec<Harness>) -> Self {
        World { kit, accts: accts.into_iter().map(|h| (h.st.account.login, h)).collect(), book: UnderlyingBooks::new(BookKey::new(1, AccountKind::Live, "EURUSD")) }
    }

    /// Enters an order on `login` and runs it through the book; applies every outbox item in order.
    fn order(&mut self, login: i64, req: BookReq) -> Result<Out, crate::engine::Reject> {
        let h = self.accts.get_mut(&login).unwrap();
        let e = h.run(&self.kit, |tx, env| ob::enter(tx, env, req))?;
        let Entered::New { cmd, .. } = e else { panic!("duplicate") };
        Ok(self.send(cmd))
    }

    fn send(&mut self, cmd: Cmd) -> Out {
        let out = apply(&mut self.book, &cmd);
        for (login, item, _) in items_of(&cmd, &out) {
            self.item(login, item);
        }
        out
    }

    fn item(&mut self, login: i64, item: Item) {
        let h = self.accts.get_mut(&login).unwrap();
        h.run(&self.kit, |tx, env| crate::book::outbox::op_for(item)(tx, env)).unwrap();
    }

    fn st(&self, login: i64) -> &AccountState {
        &self.accts[&login].st
    }

    /// Σ of every posting on an account code across all accounts' ledger events.
    fn postings_to(&self, code: &str) -> Vec<crate::model::Posting> {
        self.accts.values().flat_map(|h| h.log.iter()).filter_map(|e| if let Event::Ledger { txn } = e { Some(txn) } else { None }).flat_map(|t| t.postings.iter()).filter(|p| p.account == code).cloned().collect()
    }

    fn ledger_sum(&self, code: &str) -> D {
        self.accts.values().flat_map(|h| h.log.iter()).filter_map(|e| if let Event::Ledger { txn } = e { Some(txn) } else { None }).flat_map(|t| t.postings.iter()).filter(|p| p.account == code).map(|p| p.amount).sum()
    }

    /// Every account's book positions per series = the book's positions (steps = contracts here).
    fn assert_positions_match(&self) {
        let mut acct: BTreeMap<(String, i64), D> = BTreeMap::new();
        for h in self.accts.values() {
            for p in h.st.positions.values().filter(|p| p.on_book()) {
                *acct.entry((p.symbol.clone(), p.login)).or_default() += p.volume * p.side.sign();
            }
        }
        acct.retain(|_, v| !v.is_zero());
        let mut book: BTreeMap<(String, i64), D> = BTreeMap::new();
        for (s, sb) in &self.book.series {
            for (l, q) in &sb.pos {
                book.insert((s.clone(), *l), D::from(*q));
            }
        }
        assert_eq!(acct, book, "account book positions vs the book");
    }

    fn assert_idle_reserve(&self) {
        for h in self.accts.values() {
            assert!(h.st.book.orders.is_empty(), "working orders left: {:?}", h.st.book.orders.keys());
            assert_eq!(h.st.book.reserve(), ZERO);
            assert_eq!(metrics(&self.kit.env(&h.st), &h.st).order_reserve, ZERO);
        }
    }
}

fn limit(side: Side, qty: &str, px: &str) -> BookReq {
    BookReq::limit(C116, side, d(qty), d(px))
}

/// CFD / Options account split: a CFD account never enters the book (its stops neither), whatever the order;
/// the market maker's own account quotes whatever its group (`lp`); nothing is reserved by a refusal.
#[test]
fn book_orders_need_an_options_account() {
    let kit = kit();
    let (a, c) = (10_000_001, 10_000_009);
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "10000"));
    w.accts.insert(c, account(&w.kit, c, 9, "hedge", "10000"));
    for r in [limit(Side::Buy, "1", "0.0050"), limit(Side::Sell, "1", "0.0052"), BookReq { post_only: true, ..limit(Side::Sell, "2", "0.0060") }] {
        let e = w.order(c, r).unwrap_err();
        assert_eq!(e.code, "product_mismatch", "{e}");
    }
    assert!(w.st(c).book.orders.is_empty() && w.st(c).book.reserve() == ZERO && w.st(c).positions.is_empty());
    // the same order from the Options account works
    assert_eq!(w.order(a, limit(Side::Buy, "1", "0.0050")).unwrap().rested.len(), 1);
    // a liquidity-provider account (the Kalks market maker) is exempt from the product rule
    let lp = BookReq { lp: true, ..limit(Side::Sell, "1", "0.0055") };
    assert_eq!(w.order(c, lp).unwrap().rested.len(), 1);
}

#[test]
fn a_trade_between_two_clients_books_premium_through_clearing_fees_and_releases_reserves() {
    let kit = kit();
    let (a, b) = (10_000_001, 10_000_002);
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "10000"));
    w.accts.insert(b, account(&w.kit, b, 8, "opt", "10000"));
    // A rests a sell of 2 at 0.0051: the fee and the opening margin are reserved, free margin drops by it
    let out = w.order(a, limit(Side::Sell, "2", "0.0051")).unwrap();
    assert_eq!(out.rested.len(), 1);
    let ra = w.st(a).book.reserve();
    assert!(ra > d("0.60"), "fee 2 × 0.30 + margin: {ra}");
    let m = metrics(&w.kit.env(w.st(a)), w.st(a));
    assert_eq!((m.order_reserve, m.free_margin), (ra, m.equity - m.margin - ra));
    assert_eq!(m.withdrawable(), (m.balance - ra).min(m.free_margin - m.credit - m.bonus).max(ZERO));
    // B buys 3 at 0.0052: 2 trade at the resting 0.0051, 1 rests
    let out = w.order(b, limit(Side::Buy, "3", "0.0052")).unwrap();
    assert_eq!((out.fills.len(), out.fills[0].px, out.fills[0].qty, out.fills[0].premium_usd), (1, 510, 2, d("102.00")));
    let f = &out.fills[0];
    // A: + premium 102, + maker rebate min(0.05 × 2, 10 % × 102) = 0.10; B: − 102, − taker fee 0.60
    assert_eq!(w.st(a).balance, d("10102.10"));
    assert_eq!(w.st(b).balance, d("9897.40"));
    // clearing nets to zero once both sides are booked
    assert_eq!(w.ledger_sum(CLEARING), ZERO);
    let pa = w.st(a).positions.values().next().unwrap().clone();
    let pb = w.st(b).positions.values().next().unwrap().clone();
    assert_eq!((pa.side, pa.volume, pa.premium, pa.on_book()), (Side::Sell, d("2"), d("102.00"), true));
    assert_eq!((pb.side, pb.volume, pb.premium, pb.commission, pb.on_book()), (Side::Buy, d("2"), d("-102.00"), d("0.60"), true));
    // the deals carry option.fill
    let deal_fill = w.accts[&b].log.iter().find_map(|e| match e {
        Event::PositionOpened { deal: Some(dl), .. } => dl.option.as_ref().and_then(|o| o.fill.clone()),
        _ => None,
    });
    assert_eq!(deal_fill.map(|x| (x.id, x.role)), Some((f.id.clone(), "taker".to_string())));
    // A's order is done (nothing reserved), B still reserves for the resting 1 only
    assert!(w.st(a).book.orders.is_empty() && w.st(a).book.reserve() == ZERO);
    assert_eq!(w.st(b).book.orders.values().next().unwrap().left, 1);
    // the same fill again: recognised, nothing booked twice
    let v0 = w.st(b).version;
    w.item(b, Item::Fill { fill: f.clone(), role: Role::Taker });
    assert_eq!(w.st(b).version, v0);
    w.assert_positions_match();
    // B cancels the rest: reserve 0 everywhere
    let id = *w.st(b).book.orders.keys().next().unwrap();
    w.send(Cmd::Cancel { series: C116.into(), id, login: b, reason: "cancelled".into(), at: 0 });
    w.assert_idle_reserve();
    // A buys 1 back from B's new offer: FIFO netting closes 1 of A's short, realised P&L on the deal
    w.order(b, limit(Side::Sell, "1", "0.0050")).unwrap();
    let out = w.order(a, BookReq::market(C116, Side::Buy, d("1"))).unwrap();
    assert_eq!(out.fills[0].px, 500);
    let short = w.st(a).positions.values().next().unwrap();
    assert_eq!((short.volume, short.premium), (d("1"), d("51.00")));
    let close = w.accts[&a].log.iter().rev().find_map(|e| if let Event::PositionClosed { deal, .. } = e { Some(deal.clone()) } else { None }).unwrap();
    assert_eq!((close.profit, close.volume), (d("1.00"), d("1"))); // sold at 51, bought back at 50
    let lb = w.st(b).positions.values().next().unwrap();
    assert_eq!((lb.side, lb.volume), (Side::Buy, d("1")), "B sold 1 of its long");
    assert_eq!(w.ledger_sum(CLEARING), ZERO);
    w.assert_positions_match();
    w.assert_idle_reserve();
    for h in w.accts.values() {
        h.assert_ledger();
        h.assert_replay();
    }
}

#[test]
fn cent_accounts_use_the_four_leg_clearing_form() {
    let kit = kit();
    let (a, c) = (10_000_001, 10_000_003);
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "10000"));
    w.accts.insert(c, account(&w.kit, c, 9, "opt-cent", "1000"));
    assert_eq!(w.st(c).balance, d("100000"));
    w.order(a, limit(Side::Sell, "1", "0.0051")).unwrap();
    w.order(c, limit(Side::Buy, "1", "0.0051")).unwrap();
    // 51 USD = 5100 USC; taker fee 0.30 USD = 30 USC
    assert_eq!(w.st(c).balance, d("100000") - d("5100") - d("30"));
    let txn = w.accts[&c].log.iter().find_map(|e| match e {
        Event::Ledger { txn } if txn.idempotency_key.ends_with(":prem") => Some(txn.clone()),
        _ => None,
    });
    let txn = txn.unwrap();
    assert_eq!(txn.postings.len(), 4, "{txn:?}");
    assert!(txn.is_balanced());
    assert_eq!(w.ledger_sum(CLEARING), ZERO);
    assert_eq!(w.ledger_sum("house:fx:USD") + w.ledger_sum(CLEARING), w.ledger_sum("house:fx:USD"));
}

#[test]
fn entry_gates_band_tick_reduce_only_limits_and_funds() {
    let kit = kit();
    let a = 10_000_001;
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "60"));
    let run = |w: &mut World, r: BookReq| w.accts.get_mut(&a).unwrap().run(&w.kit, |tx, env| ob::enter(tx, env, r)).map(|_| ());
    // not a tick multiple, out of band (mark 0.0051: a buy at most 0.0051 × 1.5 + 5 ticks)
    assert_eq!(run(&mut w, limit(Side::Buy, "1", "0.005105")).unwrap_err().code, "invalid_price");
    assert_eq!(run(&mut w, limit(Side::Buy, "1", "0.00771")).unwrap_err().code, "price_out_of_band");
    // the passive side may be any price ≥ 1 tick
    assert!(run(&mut w, limit(Side::Buy, "1", "0.00001")).is_ok());
    // reduce-only without a position
    let mut r = limit(Side::Sell, "1", "0.0051");
    r.reduce_only = true;
    assert_eq!(run(&mut w, r).unwrap_err().code, "reduce_only");
    // not eligible (options intro), opening only
    let mut r = limit(Side::Buy, "1", "0.0040");
    r.eligible = false;
    assert_eq!(run(&mut w, r).unwrap_err().code, "not_eligible");
    // cash: 60 USD cannot reserve 2 × 52 + fees
    assert_eq!(run(&mut w, limit(Side::Buy, "2", "0.0052")).unwrap_err().code, "insufficient_cash");
    // a post-only market order makes no sense
    let mut r = BookReq::market(C116, Side::Buy, d("1"));
    r.post_only = true;
    assert_eq!(run(&mut w, r).unwrap_err().code, "invalid_order");
    // nothing above stays reserved except the 1-tick bid
    assert_eq!(w.st(a).book.orders.len(), 1);
}

#[test]
fn market_orders_become_ioc_at_the_band_and_stops_fire_on_the_mark() {
    let kit = kit();
    let (a, b) = (10_000_001, 10_000_002);
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "10000"));
    w.accts.insert(b, account(&w.kit, b, 8, "opt", "10000"));
    // a market buy at mark 0.0051 becomes an IOC limit at max(0.0051 × 1.1, 0.0051 + 5 ticks) = 0.00561
    let e = w.accts.get_mut(&a).unwrap().run(&w.kit, |tx, env| ob::enter(tx, env, BookReq::market(C116, Side::Buy, d("1")))).unwrap();
    let Entered::New { cmd, .. } = e else { panic!() };
    let Cmd::New { order, .. } = &cmd else { panic!() };
    assert_eq!((order.px, order.tif), (561, Tif::Ioc));
    // an empty book: the market order is cancelled, nothing stays reserved
    let out = w.send(cmd);
    assert_eq!(out.done[0].reason, "ioc_remainder");
    assert!(w.st(a).book.orders.is_empty());
    // a stop_market buy triggered by the mark above 0.0055: placed, nothing reserved; fires when the mark gets there
    let sr = ob::StopReq { book: BookReq::market(C116, Side::Buy, d("1")), source: crate::model::StopSource::Mark, op: crate::model::TriggerOp::Above, trigger: d("0.0055") };
    let stop = w.accts.get_mut(&a).unwrap().run(&w.kit, |tx, env| ob::place_stop(tx, env, sr)).unwrap();
    assert_eq!(w.st(a).book.reserve(), ZERO);
    let mut tx = Tx::new(w.st(a));
    ob::eval_stops(&mut tx, &w.kit.env(w.st(a)), None);
    assert!(tx.book_send.is_empty(), "mark 0.0051 is below the trigger");
    w.kit.options.fix(C116, "0.0055", "0.0057");
    let env = w.kit.env(w.st(a));
    let mut tx = Tx::new(w.st(a));
    ob::eval_stops(&mut tx, &env, None);
    assert_eq!(tx.book_send.len(), 1);
    let Cmd::New { order, .. } = &tx.book_send[0].cmd else { panic!() };
    assert_eq!(order.id, stop.ticket, "the fired stop keeps its ticket (the book dedupes resubmissions)");
    assert!(tx.events.iter().any(|e| matches!(e, Event::OrderRemoved { reason, .. } if reason == "book_stop_fired")));
    assert!(tx.st.book.reserve() > ZERO, "reserved when it fires");
}

/* ------------------------------------------------------------------ */
/* Random flows                                                        */
/* ------------------------------------------------------------------ */

#[derive(Clone, Debug)]
enum Flow {
    Limit { who: usize, buy: bool, qty: u8, px: i64, reduce: bool },
    Market { who: usize, buy: bool, qty: u8 },
    CancelAll { who: usize },
}

fn flow() -> impl Strategy<Value = Flow> {
    prop_oneof![
        5 => (0usize..3, any::<bool>(), 1u8..4, 480i64..=540, prop::bool::weighted(0.2)).prop_map(|(who, buy, qty, px, reduce)| Flow::Limit { who, buy, qty, px, reduce }),
        2 => (0usize..3, any::<bool>(), 1u8..3).prop_map(|(who, buy, qty)| Flow::Market { who, buy, qty }),
        1 => (0usize..3).prop_map(|who| Flow::CancelAll { who }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

    #[test]
    fn random_flows_keep_money_and_positions_consistent(flows in prop::collection::vec(flow(), 1..30)) {
        let kit = kit();
        let logins = [10_000_001i64, 10_000_002, 10_000_003];
        let mut w = World::new(kit, vec![]);
        for (i, l) in logins.iter().enumerate() {
            let group = if i == 2 { "opt-cent" } else { "opt" };
            w.accts.insert(*l, account(&w.kit, *l, 7 + i as i64, group, "20000"));
        }
        for f in &flows {
            match f {
                Flow::Limit { who, buy, qty, px, reduce } => {
                    let mut r = BookReq::limit(C116, if *buy { Side::Buy } else { Side::Sell }, D::from(*qty), D::from(*px) * d("0.00001"));
                    r.reduce_only = *reduce;
                    let _ = w.order(logins[*who], r);
                }
                Flow::Market { who, buy, qty } => {
                    let _ = w.order(logins[*who], BookReq::market(C116, if *buy { Side::Buy } else { Side::Sell }, D::from(*qty)));
                }
                Flow::CancelAll { who } => {
                    w.send(Cmd::CancelAll { login: logins[*who], series: None, expiry: None, ephemeral_only: false, reason: "test".into(), at: 0 });
                }
            }
            // after every command (all items applied): clearing nets to 0, positions agree, Σ long = Σ short
            prop_assert_eq!(w.ledger_sum(CLEARING), ZERO);
            w.assert_positions_match();
            let net: D = w.accts.values().flat_map(|h| h.st.positions.values()).filter(|p| p.on_book()).map(|p| p.volume * p.side.sign()).sum();
            prop_assert_eq!(net, ZERO);
            // the shard's working orders are the book's
            for l in logins {
                let mine: Vec<(i64, i64)> = w.st(l).book.orders.values().map(|o| (o.id, o.left)).collect();
                let theirs: Vec<(i64, i64)> = w.book.series.values().flat_map(|sb| sb.orders.values().filter(|o| o.login == l).map(|o| (o.id, o.left))).collect();
                prop_assert_eq!(mine, theirs);
            }
        }
        for l in logins {
            w.send(Cmd::CancelAll { login: l, series: None, expiry: None, ephemeral_only: false, reason: "end".into(), at: 0 });
        }
        w.assert_idle_reserve();
        for h in w.accts.values() {
            h.assert_ledger();
            h.assert_replay();
        }
    }
}

#[test]
fn the_mark_clamps_inside_the_published_book_and_values_positions() {
    let kit = kit();
    let (a, b) = (10_000_001, 10_000_002);
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "10000"));
    w.accts.insert(b, account(&w.kit, b, 8, "opt", "10000"));
    // model 0.0050 / 0.0052 → mid 0.0051; B buys 1 from A at 0.0051
    w.order(a, limit(Side::Sell, "2", "0.0051")).unwrap();
    w.order(b, limit(Side::Buy, "1", "0.0051")).unwrap();
    let terms = w.st(b).positions.values().next().unwrap().option.clone().unwrap();
    let model = |w: &World| super::options::mark_of(&w.kit.env(w.st(b)), &w.st(b).account, &terms).unwrap().mark;
    assert_eq!(model(&w), d("0.0051"), "no book published: the model mid");
    // A's remaining offer at 0.0051 is published; then A improves it to 0.0049 (below the model mid)
    let top = w.kit.options.top.clone();
    top.publish("kalks", &w.book, None);
    assert_eq!(model(&w), d("0.0051"), "min(model, ask) with the ask at the model");
    w.order(a, limit(Side::Sell, "1", "0.0049")).unwrap();
    top.publish("kalks", &w.book, None);
    assert_eq!(model(&w), d("0.0049"), "one qualifying side below the model: the mark is the ask");
    // the long is valued at the clamped mark (49 instead of 51)
    let m = metrics(&w.kit.env(w.st(b)), w.st(b));
    assert_eq!(crate::money::r2(m.option_value), d("49.00"));
    // a bid above the model mid on the other side: two-sided, spread 0 ≤ 3 × model spread → clamp(model, bid, ask)
    w.order(b, limit(Side::Buy, "1", "0.0048")).unwrap();
    top.publish("kalks", &w.book, None);
    assert_eq!(model(&w), d("0.0049"), "clamp(0.0051, 0.0048, 0.0049) = 0.0049");
    // a demo account of the same tenant does not see the live book
    let st = w.st(b).clone();
    let mut demo = st.account.clone();
    demo.kind = AccountKind::Demo;
    assert_eq!(super::options::mark_of(&w.kit.env(&st), &demo, &terms).unwrap().mark, d("0.0051"));
}

/// Book positions settle at the fixing like house ones (docs §9): at cut − closeOnlyMinutes `Expire` cancels the
/// expiry's working orders (reserves released), each side is paid / charged its payoff against the expiry's
/// clearing account (not `house:options_settlement`), which nets to 0 because every long has a short, settling
/// again books nothing, and the purge drops the series from the book.
#[test]
fn book_positions_settle_at_the_fixing_and_both_sides_net_to_zero() {
    let kit = kit();
    let (a, b, c) = (10_000_001, 10_000_002, 10_000_004);
    let mut w = World::new(kit, vec![]);
    for (l, u) in [(a, 7), (b, 8), (c, 9)] {
        w.accts.insert(l, account(&w.kit, l, u, "opt", "10000"));
    }
    // A sells 3: B buys 2, C buys 1, all at 0.0051; B also rests a buy of 1 at 0.0040
    w.order(a, limit(Side::Sell, "3", "0.0051")).unwrap();
    assert_eq!(w.order(b, limit(Side::Buy, "2", "0.0051")).unwrap().fills.len(), 1);
    assert_eq!(w.order(c, limit(Side::Buy, "1", "0.0051")).unwrap().fills.len(), 1);
    assert_eq!(w.order(b, limit(Side::Buy, "1", "0.0040")).unwrap().rested.len(), 1);
    assert_eq!(w.ledger_sum(CLEARING), ZERO);
    w.assert_positions_match();
    // close-only time: the expiry's orders are cancelled and their reserves released
    let expiry = chrono::NaiveDate::from_ymd_opt(2026, 10, 2).unwrap();
    let e = w.send(Cmd::Expire { expiry, purge: false, at: 0 });
    assert_eq!(e.done.iter().map(|x| (x.status, x.login)).collect::<Vec<_>>(), vec![(DoneStatus::Expired, b)]);
    w.assert_idle_reserve();

    let settle = |w: &mut World, login: i64| w.accts.get_mut(&login).unwrap().run(&w.kit, |tx, env| Ok(super::options::settle(tx, env, "EURUSD:2026-10-02", d("1.17"), 1, None))).unwrap();
    // before the cut nothing settles
    assert!(settle(&mut w, a).tickets.is_empty());
    w.kit.now = chrono::DateTime::parse_from_rfc3339("2026-10-02T14:05:00Z").unwrap().with_timezone(&chrono::Utc);
    let before: BTreeMap<i64, D> = [a, b, c].into_iter().map(|l| (l, w.st(l).balance)).collect();
    // call 1.16 at a 1.17 fixing: 0.01 × 10 000 = 100 USD per contract
    for (l, cash) in [(a, "-300.00"), (b, "200.00"), (c, "100.00")] {
        let s = settle(&mut w, l);
        assert_eq!((s.tickets.len(), s.cash), (1, d(cash)), "login {l}");
        assert_eq!(w.st(l).balance - before[&l], d(cash), "login {l}");
        assert!(w.st(l).positions.is_empty(), "login {l}");
        let deal = w.accts[&l].log.iter().rev().find_map(|e| if let Event::PositionClosed { deal, .. } = e { Some(deal.clone()) } else { None }).unwrap();
        assert_eq!((deal.reason, deal.price), (crate::model::DealReason::Expiry, d("0.01")));
    }
    assert_eq!(w.ledger_sum(CLEARING), ZERO, "long payouts = short charges, through the clearing account");
    assert!(w.postings_to("house:options_settlement:USD").is_empty(), "book positions do not settle against the house");
    assert_eq!(w.postings_to(CLEARING).len(), 2 * 2 + 3, "2 fills (both sides each) and 3 settlements");
    // a repeat (crash catch-up, scheduler pass) books nothing
    for l in [a, b, c] {
        let v = w.st(l).version;
        assert!(settle(&mut w, l).tickets.is_empty());
        assert_eq!(w.st(l).version, v);
    }
    // after the settlement pass the book drops the expiry: no book positions left on either side
    w.send(Cmd::Expire { expiry, purge: true, at: 0 });
    assert!(w.book.book(C116).is_none());
    w.assert_positions_match();
    w.assert_idle_reserve();
    for h in w.accts.values() {
        h.assert_ledger();
        h.assert_replay();
    }
}

/// The cross-currency case of docs §9: a USD account short and a cent account long the same book series. Each
/// settles against the expiry's clearing account in USD (the cent side in the 4-leg form through `house:fx`, like
/// its fill), so the clearing account nets to 0 per ledger code; nothing touches `house:options_settlement`; a
/// re-run at a corrected fixing reverses both sides through the same accounts and the clearing still nets to 0;
/// the payout is held for the re-run window; every account replays to its live state.
#[test]
fn usd_and_cent_book_positions_settle_through_clearing_and_net_to_zero() {
    let kit = kit();
    let (a, c, b) = (10_000_001, 10_000_003, 10_000_002);
    let mut w = World::new(kit, vec![]);
    w.accts.insert(a, account(&w.kit, a, 7, "opt", "10000"));
    w.accts.insert(c, account(&w.kit, c, 9, "opt-cent", "1000"));
    w.accts.insert(b, account(&w.kit, b, 8, "opt-cent", "1000"));
    // A (USD) sells 3: C (cent) buys 2, B (cent) buys 1
    w.order(a, limit(Side::Sell, "3", "0.0051")).unwrap();
    w.order(c, limit(Side::Buy, "2", "0.0051")).unwrap();
    w.order(b, limit(Side::Buy, "1", "0.0051")).unwrap();
    assert_eq!(w.ledger_sum(CLEARING), ZERO);
    w.kit.now = chrono::DateTime::parse_from_rfc3339("2026-10-02T14:05:00Z").unwrap().with_timezone(&chrono::Utc);
    let settle = |w: &mut World, login: i64, fixing: &str, run: i32| w.accts.get_mut(&login).unwrap().run(&w.kit, |tx, env| Ok(super::options::settle(tx, env, "EURUSD:2026-10-02", d(fixing), run, None))).unwrap();
    let before: BTreeMap<i64, D> = [a, c, b].into_iter().map(|l| (l, w.st(l).balance)).collect();
    // 1.16 call at 1.16733: 0.00733 × 10 000 = 73.30 USD per contract (cent: 7 330 USC)
    for (l, cash) in [(a, "-219.90"), (c, "14660.00"), (b, "7330.00")] {
        let s = settle(&mut w, l, "1.16733", 1);
        assert_eq!((s.tickets.len(), s.cash), (1, d(cash)), "login {l}");
        assert_eq!(w.st(l).balance - before[&l], d(cash), "login {l}");
    }
    assert_eq!(w.ledger_sum(CLEARING), ZERO, "USD short = cent longs, per ledger code");
    assert!(w.postings_to("house:options_settlement:USD").is_empty() && w.postings_to("house:options_settlement:USC").is_empty());
    let cent_settle = w.accts[&c].log.iter().find_map(|e| match e {
        Event::Ledger { txn } if txn.idempotency_key.starts_with("settle:") => Some(txn.clone()),
        _ => None,
    });
    let cent_settle = cent_settle.unwrap();
    assert_eq!(cent_settle.postings.len(), 4, "4-leg form via house:fx: {cent_settle:?}");
    assert!(cent_settle.is_balanced());
    assert_eq!(cent_settle.postings.iter().find(|p| p.account == CLEARING).unwrap().amount, d("-146.60"));
    // house:fx carries the conversion: USC −, USD + by the same value
    assert_eq!(w.ledger_sum("house:fx:USC") / D::from(100) + w.ledger_sum("house:fx:USD"), ZERO);
    // the payout is held for the re-run window
    assert_eq!(metrics(&w.kit.env(w.st(c)), w.st(c)).held, d("14660.00"));
    // re-run at a corrected fixing 1.16700 (run 2): both sides are reversed through the clearing account
    let deals = |w: &World, l: i64| -> Vec<crate::model::Deal> { w.accts[&l].log.iter().filter_map(|e| if let Event::PositionClosed { deal, .. } = e { Some(deal.clone()) } else { None }).filter(|d| d.reason == crate::model::DealReason::Expiry).collect() };
    let mid: BTreeMap<i64, D> = [a, c, b].into_iter().map(|l| (l, w.st(l).balance)).collect();
    for (l, diff) in [(a, "9.90"), (c, "-660.00"), (b, "-330.00")] {
        let ds = deals(&w, l);
        let s = w.accts.get_mut(&l).unwrap().run(&w.kit, |tx, env| super::options::rerun(tx, env, "EURUSD:2026-10-02", &ds, d("1.16700"), 2, None)).unwrap();
        assert_eq!(s.tickets.len(), 1, "login {l}");
        assert_eq!(w.st(l).balance - mid[&l], d(diff), "login {l}: the re-run moves the balance by exactly the difference");
    }
    assert_eq!(w.ledger_sum(CLEARING), ZERO, "still nets to 0 after the re-run");
    assert!(w.postings_to("house:options_settlement:USD").is_empty() && w.postings_to("house:options_settlement:USC").is_empty());
    let rev = w.accts[&c].log.iter().find_map(|e| match e {
        Event::Ledger { txn } if txn.idempotency_key.starts_with("settle-rev:") => Some(txn.clone()),
        _ => None,
    });
    assert_eq!(rev.unwrap().postings.len(), 4, "the cent reversal mirrors the 4-leg settlement");
    // the hold follows the corrected payout (14 660 − 14 660 + 14 000)
    assert_eq!(metrics(&w.kit.env(w.st(c)), w.st(c)).held, d("14000.00"));
    w.kit.now += chrono::Duration::seconds(crate::state::SETTLEMENT_HOLD_SECS + 1);
    assert_eq!(metrics(&w.kit.env(w.st(c)), w.st(c)).held, ZERO, "released after the re-run window");
    for h in w.accts.values() {
        h.assert_ledger();
        h.assert_replay();
    }
}

/// Rounding: a payoff that is not a whole cent per position (XAU-like decimals) leaves at most 0.005 USD per settled
/// position in the clearing account; the sweep (settle.rs) moves exactly that and refuses anything larger.
#[test]
fn per_position_rounding_stays_within_the_sweep_limit() {
    let kit = kit();
    let (a, b, c) = (10_000_001, 10_000_002, 10_000_004);
    let mut w = World::new(kit, vec![]);
    for (l, u) in [(a, 7), (b, 8), (c, 9)] {
        w.accts.insert(l, account(&w.kit, l, u, "opt", "10000"));
    }
    w.order(a, limit(Side::Sell, "3", "0.0051")).unwrap();
    w.order(b, limit(Side::Buy, "1", "0.0051")).unwrap();
    w.order(c, limit(Side::Buy, "2", "0.0051")).unwrap();
    w.kit.now = chrono::DateTime::parse_from_rfc3339("2026-10-02T14:05:00Z").unwrap().with_timezone(&chrono::Utc);
    // a fixing with 7 decimals: 0.0073333 × 10 000 = 73.333 per contract → 3 × = 219.999 → −220.00; 73.33; 146.67
    for l in [a, b, c] {
        w.accts.get_mut(&l).unwrap().run(&w.kit, |tx, env| Ok(super::options::settle(tx, env, "EURUSD:2026-10-02", d("1.1673333"), 1, None))).unwrap();
    }
    let net = w.ledger_sum(CLEARING);
    assert_eq!(net, d("0.00"), "220.00 − 73.33 − 146.67");
    // the general bound: |net| ≤ 0.005 × settled positions
    for (n, settled, ok) in [(d("0.01"), 3, true), (d("0.015"), 3, true), (d("0.02"), 3, false)] {
        assert_eq!(crate::options::settle::sweep_amount(n, settled).is_some(), ok, "{n} over {settled}");
    }
}

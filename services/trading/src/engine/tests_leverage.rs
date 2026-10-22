//! Risk leverage caps (src/risk/leverage.rs, docs/RISK-DESK.md › Leverage control): margin of new orders at the capped
//! leverage, `all_positions` re-margining open positions (margin call), scopes resolving to the most restrictive cap,
//! client leverage changes above the cap refused, the preview equal to the applied result, disabling a cap restoring
//! the normal leverage for new positions while positions opened under it keep theirs, and replay.

use super::testkit::{Harness, Kit, d};
use super::trade::{self, OrderReq, PlaceResult};
use super::{Env, funds, metrics, risk, symbol_leverage};
use crate::model::Side;
use crate::money::r2;
use crate::risk::leverage::{self, AppliesTo, CapScope, CapSpec, LeverageCap};

fn cap(id: i64, scope: CapScope, target: &str, max: u32, applies_to: AppliesTo) -> LeverageCap {
    let mut c = CapSpec { scope, target: target.into(), max_leverage: max, applies_to }.draft("Julia Novak", super::testkit::t("2026-09-28T12:00:00Z"));
    c.id = id;
    c
}

fn place(h: &mut Harness, kit: &Kit, sym: &str, side: Side, v: &str) -> PlaceResult {
    h.run(kit, |tx, env| trade::place_order(tx, env, OrderReq::market(sym, side, d(v)))).unwrap()
}

fn ticket(r: &PlaceResult) -> i64 {
    match r {
        PlaceResult::Filled { position_ticket: Some(t), .. } => *t,
        other => panic!("not filled: {other:?}"),
    }
}

fn margin(kit: &Kit, h: &Harness) -> crate::money::D {
    r2(metrics(&kit.env(&h.st), &h.st).margin)
}

#[test]
fn new_order_margin_uses_the_capped_leverage() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    kit.tenant.risk.leverage_caps = vec![cap(1, CapScope::All, "", 10, AppliesTo::NewPositions)];
    // 1 lot = 110 000 USD notional: 11 000 at 1:10 instead of 1 100 at the account's 1:100
    let mut poor = Harness::live(&kit, "hedge", "5000");
    let e = poor.run(&kit, |tx, env| trade::place_order(tx, env, OrderReq::market("EURUSD", Side::Buy, d("1")))).unwrap_err();
    assert_eq!(e.code, "no_money");
    let mut h = Harness::live(&kit, "hedge", "20000");
    let t = ticket(&place(&mut h, &kit, "EURUSD", Side::Buy, "1"));
    assert_eq!(h.st.positions[&t].margin_leverage, Some(10));
    assert_eq!(margin(&kit, &h), d("11000.00"));
    h.assert_replay();
}

#[test]
fn new_positions_cap_keeps_open_margin_and_disabling_restores_normal_leverage() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut h = Harness::live(&kit, "hedge", "50000");
    let a = ticket(&place(&mut h, &kit, "EURUSD", Side::Buy, "1"));
    assert_eq!(margin(&kit, &h), d("1100.00"));
    kit.tenant.risk.leverage_caps = vec![cap(1, CapScope::All, "", 10, AppliesTo::NewPositions)];
    // the open position keeps its margin
    assert_eq!(margin(&kit, &h), d("1100.00"));
    assert_eq!(h.st.positions[&a].margin_leverage, None);
    let b = ticket(&place(&mut h, &kit, "EURUSD", Side::Buy, "1"));
    assert_eq!(margin(&kit, &h), d("12100.00"));
    // disabled: new positions at 1:100 again, the one opened under the cap keeps 1:10
    kit.tenant.risk.leverage_caps[0].enabled = false;
    assert_eq!(margin(&kit, &h), d("12100.00"));
    let c = ticket(&place(&mut h, &kit, "EURUSD", Side::Buy, "1"));
    assert_eq!(h.st.positions[&c].margin_leverage, None);
    assert_eq!(h.st.positions[&b].margin_leverage, Some(10));
    assert_eq!(margin(&kit, &h), d("13200.00"));
    // a hedged pair with mixed leverages: 1 lot short at 1:100 against the three longs (hedged at 50 % per leg)
    let s = ticket(&place(&mut h, &kit, "EURUSD", Side::Sell, "1"));
    assert_eq!(h.st.positions[&s].margin_leverage, None);
    // net 2 lots at the longs' mean 1/leverage ((1/100 + 1/10 + 1/100) / 3 = 0.04) + 1 hedged lot × 50 % × (0.04 + 0.01)
    // = (2 × 0.04 + 0.5 × 0.05) × 110 000 = 11 550
    assert_eq!(margin(&kit, &h), d("11550.00"));
    h.assert_replay();
}

#[test]
fn all_positions_cap_remargins_open_positions_into_margin_call() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut h = Harness::live(&kit, "hedge", "10000");
    place(&mut h, &kit, "EURUSD", Side::Buy, "1");
    assert_eq!(margin(&kit, &h), d("1100.00"));
    kit.tenant.risk.leverage_caps = vec![cap(1, CapScope::Group, "hedge", 10, AppliesTo::AllPositions)];
    // equity 10 000 / margin 11 000 = 90.9 % ≤ the 100 % margin call
    assert_eq!(margin(&kit, &h), d("11000.00"));
    h.run(&kit, |tx, env| {
        risk::check_margin(tx, env);
        Ok(())
    })
    .unwrap();
    assert!(h.st.margin_call);
    // another group is not in scope
    let mut n = Harness::demo(&kit, "net");
    n.run(&kit, |tx, env| funds::demo_refill(tx, env).map(|_| ())).ok();
    let env = kit.env(&n.st);
    let spec = env.specs.get("EURUSD").unwrap();
    assert_eq!(symbol_leverage(&env, &n.st.account, spec).live, 100);
    h.assert_replay();
}

#[test]
fn scopes_resolve_to_the_most_restrictive_cap() {
    let mut kit = Kit::new();
    kit.tenant.risk.leverage_caps = vec![
        cap(1, CapScope::All, "", 50, AppliesTo::AllPositions),
        cap(2, CapScope::Group, "hedge", 20, AppliesTo::AllPositions),
        cap(3, CapScope::AssetClass, "crypto", 5, AppliesTo::AllPositions),
        cap(4, CapScope::Account, "10000001", 30, AppliesTo::AllPositions),
        cap(5, CapScope::Account, "50000001", 2, AppliesTo::NewPositions),
        cap(6, CapScope::Group, "net", 10, AppliesTo::NewPositions),
    ];
    let live = Harness::live(&kit, "hedge", "0"); // login 10 000 001, group hedge
    let env = kit.env(&live.st);
    let lev = |h: &Harness, sym: &str| {
        let env = kit.env(&h.st);
        symbol_leverage(&env, &h.st.account, env.specs.get(sym).unwrap())
    };
    // group 20 beats all 50 and account 30
    assert_eq!(lev(&live, "EURUSD").live, 20);
    assert_eq!(lev(&live, "EURUSD").new, 20);
    // crypto 5 beats everything (and BTCUSD's own max 20)
    assert_eq!(lev(&live, "BTCUSD").live, 5);
    assert_eq!(leverage::account_cap(&env.tenant.risk.leverage_caps, &live.st.account).unwrap().max_leverage, 20);
    // demo 50 000 001 in `net`: all 50 for open positions, its own new-positions cap 2 for new exposure
    let demo = Harness::demo(&kit, "net");
    assert_eq!(lev(&demo, "EURUSD").live, 50);
    assert_eq!(lev(&demo, "EURUSD").new, 2);
    // the account view: effective leverage, the cap and the lower asset classes
    let v = leverage::client_json(&kit.tenant.risk.leverage_caps, &live.st.account);
    assert_eq!(v["effectiveLeverage"], 20);
    assert_eq!(v["leverageCap"], 20);
    assert_eq!(v["effectiveLeverageByClass"]["crypto"], 5);
    assert!(v["effectiveLeverageByClass"].get("forex").is_none());
    // no caps: the account / symbol leverage as before
    let plain = Kit::new();
    let env = plain.env(&live.st);
    assert_eq!(symbol_leverage(&env, &live.st.account, env.specs.get("EURUSD").unwrap()).live, 100);
    assert_eq!(symbol_leverage(&env, &live.st.account, env.specs.get("BTCUSD").unwrap()).live, 20);
}

#[test]
fn client_leverage_change_above_the_cap_is_refused() {
    let mut kit = Kit::new();
    kit.tenant.risk.leverage_caps = vec![cap(1, CapScope::All, "", 50, AppliesTo::NewPositions), cap(2, CapScope::AssetClass, "crypto", 5, AppliesTo::AllPositions)];
    let mut h = Harness::live(&kit, "hedge", "1000");
    let e = h.run(&kit, |tx, env| funds::change_leverage(tx, env, 200, false)).unwrap_err();
    assert_eq!(e.code, "leverage_capped");
    // at the cap: fine (an asset-class cap does not limit the account leverage)
    h.run(&kit, |tx, env| funds::change_leverage(tx, env, 50, false)).unwrap();
    // staff may still set any group leverage (margin stays capped)
    h.run(&kit, |tx, env| funds::change_leverage(tx, env, 500, true)).unwrap();
    assert_eq!(h.st.account.leverage, 500);
    let env = kit.env(&h.st);
    assert_eq!(symbol_leverage(&env, &h.st.account, env.specs.get("EURUSD").unwrap()).new, 50);
}

#[test]
fn preview_matches_the_applied_result() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    kit.quote("BTCUSD", "80000", "80000");
    let mut h = Harness::live(&kit, "hedge", "10000");
    place(&mut h, &kit, "EURUSD", Side::Buy, "1");
    place(&mut h, &kit, "BTCUSD", Side::Sell, "0.5");
    let spec = CapSpec { scope: CapScope::AssetClass, target: "crypto".into(), max_leverage: 4, applies_to: AppliesTo::AllPositions };
    let after_cfg = {
        let mut t = kit.tenant.clone();
        t.risk.leverage_caps = spec.applied_to(&t.risk.leverage_caps, "Julia Novak", kit.now);
        t
    };
    let now = kit.env(&h.st);
    let after = Env { tenant: &after_cfg, ..now };
    let row = leverage::preview_account(&now, &after, &h.st, &spec).expect("in scope: holds crypto");
    // BTCUSD 40 000 notional: 2 000 at 1:20 → 10 000 at 1:4; EURUSD unchanged 1 100 (equity ~10 000 / 11 100 = 90 %)
    assert_eq!(row["marginBefore"].as_f64(), Some(3100.0));
    assert_eq!(row["marginAfter"].as_f64(), Some(11100.0));
    assert_eq!(row["positions"], 1);
    assert_eq!(row["status"], "margin_call");
    let summary = leverage::preview_summary(&spec, vec![row.clone()], 10);
    assert_eq!(summary["totals"]["accountsAffected"], 1);
    assert_eq!(summary["totals"]["marginCall"], 1);
    assert_eq!(summary["marginCallAccounts"][0]["login"], "10000001");
    // apply it: the engine's numbers are the preview's
    kit.tenant.risk.leverage_caps = after_cfg.risk.leverage_caps.clone();
    let m = metrics(&kit.env(&h.st), &h.st);
    assert_eq!(r2(m.margin).to_string(), row["marginAfter"].as_f64().unwrap().to_string() + ".00");
    assert_eq!(Some(r2(m.level.unwrap())), crate::money::from_f64(row["marginLevelAfter"].as_f64().unwrap()));
    h.run(&kit, |tx, env| {
        risk::check_margin(tx, env);
        Ok(())
    })
    .unwrap();
    assert!(h.st.margin_call);
    // an account without crypto is not in an asset-class scope
    let other = Harness::demo(&kit, "hedge");
    assert!(leverage::preview_account(&kit.env(&other.st), &Env { tenant: &after_cfg, ..kit.env(&other.st) }, &other.st, &spec).is_none());
}

#[test]
fn netting_add_under_a_cap_keeps_each_part_margined_at_its_leverage() {
    let mut kit = Kit::new();
    kit.quote("EURUSD", "1.10000", "1.10000");
    let mut h = Harness::live(&kit, "net", "50000");
    let t = ticket(&place(&mut h, &kit, "EURUSD", Side::Buy, "1"));
    kit.tenant.risk.leverage_caps = vec![cap(1, CapScope::All, "", 10, AppliesTo::NewPositions)];
    // the free-margin check charges only the new lot at 1:10
    place(&mut h, &kit, "EURUSD", Side::Buy, "1");
    // 2 lots at the harmonic mean of 1:100 and 1:10: 2 / (1/100 + 1/10) = 18.18 → 1:18
    assert_eq!(h.st.positions[&t].margin_leverage, Some(18));
    assert_eq!(margin(&kit, &h), d("12222.22"));
    // a reduction keeps it; a reversal opens at today's new-exposure leverage
    place(&mut h, &kit, "EURUSD", Side::Sell, "1");
    assert_eq!(h.st.positions[&t].margin_leverage, Some(18));
    let r = ticket(&place(&mut h, &kit, "EURUSD", Side::Sell, "2"));
    assert_eq!(h.st.positions[&r].margin_leverage, Some(10));
    assert_eq!(margin(&kit, &h), d("11000.00"));
    h.assert_replay();
    h.assert_ledger();
}

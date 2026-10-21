//! Pure maths for copy trading and PAMM: copy sizing (D69), high-water-mark fees (D66), NAV / units and the
//! rollover plan (D65, D67), and the statistics behind the leaderboard and the risk score (D72).
//! No IO; every function here is covered by the tests at the bottom.

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::money::{D, HUNDRED, ONE, ZERO, r2, rdp};
use crate::specs::Spec;

/// NAV and units are kept to 8 decimals.
pub const NAV_DP: u32 = 8;

/* ------------------------------------------------------------------ */
/* Copy sizing                                                         */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizingMode {
    /// Follower equity ÷ master equity.
    Equity,
    /// Fixed allocation (USD) ÷ master equity.
    Allocation,
    /// Master volume × value.
    Multiplier,
    /// Every open is `value` lots.
    FixedLot,
}

impl SizingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            SizingMode::Equity => "equity",
            SizingMode::Allocation => "allocation",
            SizingMode::Multiplier => "multiplier",
            SizingMode::FixedLot => "fixed_lot",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "equity" => SizingMode::Equity,
            "allocation" => SizingMode::Allocation,
            "multiplier" => SizingMode::Multiplier,
            "fixed_lot" | "fixed-lot" | "fixed" => SizingMode::FixedLot,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizing {
    pub mode: SizingMode,
    pub value: D,
}

fn floor_step(v: D, step: D) -> D {
    if step <= ZERO {
        return v;
    }
    ((v / step).floor() * step).normalize()
}

/// Caps `raw` at the follower's max lot and the symbol's max lot and rounds it down to the lot step.
/// None when the result is below the symbol's minimum lot (the copy is skipped).
pub fn clamp_volume(raw: D, spec: &Spec, max_lot: Option<D>) -> Option<D> {
    let mut v = raw;
    if let Some(m) = max_lot.filter(|m| *m > ZERO) {
        v = v.min(m);
    }
    v = floor_step(v.min(spec.lot_max), spec.lot_step);
    if v < spec.lot_min || v <= ZERO { None } else { Some(v) }
}

/// Unclamped follower volume for a master trade of `master_vol` lots.
pub fn raw_open_volume(s: &Sizing, master_vol: D, master_eq: D, follower_eq: D) -> D {
    match s.mode {
        SizingMode::Equity => {
            if master_eq <= ZERO || follower_eq <= ZERO {
                ZERO
            } else {
                master_vol * follower_eq / master_eq
            }
        }
        SizingMode::Allocation => {
            if master_eq <= ZERO {
                ZERO
            } else {
                master_vol * s.value / master_eq
            }
        }
        SizingMode::Multiplier => master_vol * s.value,
        SizingMode::FixedLot => s.value,
    }
}

/// Follower volume for a copied open (market fill or pending order).
pub fn open_volume(s: &Sizing, master_vol: D, master_eq: D, follower_eq: D, spec: &Spec, max_lot: Option<D>) -> Option<D> {
    clamp_volume(raw_open_volume(s, master_vol, master_eq, follower_eq), spec, max_lot)
}

/// Follower volume to add when the master adds `master_add` lots to a position of `master_before` lots that
/// the follower copies with `follower_before` lots. Fixed-lot copies stay proportional to the follower's
/// own position; the other modes size the added volume like a new trade.
#[allow(clippy::too_many_arguments)]
pub fn add_volume(s: &Sizing, master_add: D, master_before: D, follower_before: D, master_eq: D, follower_eq: D, spec: &Spec, max_lot: Option<D>) -> Option<D> {
    match s.mode {
        SizingMode::FixedLot => {
            if master_before <= ZERO {
                return None;
            }
            clamp_volume(follower_before * master_add / master_before, spec, max_lot)
        }
        _ => open_volume(s, master_add, master_eq, follower_eq, spec, max_lot),
    }
}

/// Follower volume to close when the master closed `closed` lots and kept `remaining` lots.
/// Closes the same fraction, rounded down to the lot step; the whole position when the remainder would
/// fall below the minimum lot; None when the fraction rounds to 0 (nothing to do yet).
pub fn close_volume(follower_vol: D, closed: D, remaining: D, spec: &Spec) -> Option<D> {
    if follower_vol <= ZERO {
        return None;
    }
    if remaining <= ZERO || closed + remaining <= ZERO {
        return Some(follower_vol);
    }
    let v = floor_step(follower_vol * closed / (closed + remaining), spec.lot_step);
    if v <= ZERO {
        return None;
    }
    if follower_vol - v < spec.lot_min { Some(follower_vol) } else { Some(v) }
}

/* ------------------------------------------------------------------ */
/* Copy performance fee (D66)                                          */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CopyFee {
    pub fee: D,
    /// High-water mark after deposits / withdrawals of the period.
    pub hwm_before: D,
    pub hwm_after: D,
}

/// `hwm' = hwm + net flows`, `fee = pct × max(0, equity − hwm')`, new HWM = equity − fee (never lowered).
pub fn copy_fee(pct: D, hwm: D, net_flow: D, equity: D) -> CopyFee {
    let adj = (hwm + net_flow).max(ZERO);
    let gain = equity - adj;
    let fee = if gain > ZERO && pct > ZERO { r2(gain * pct / HUNDRED) } else { ZERO };
    let after = if fee > ZERO { equity - fee } else { adj };
    CopyFee { fee, hwm_before: adj, hwm_after: r2(after) }
}

/// Platform cut and the master's share of a fee.
pub fn split_fee(fee: D, platform_cut_pct: D) -> (D, D) {
    let cut = r2(fee * platform_cut_pct / HUNDRED);
    (cut, fee - cut)
}

/* ------------------------------------------------------------------ */
/* PAMM: NAV, units, rollover (D65, D67)                               */
/* ------------------------------------------------------------------ */

pub fn nav(equity: D, units: D) -> D {
    if units <= ZERO { ONE } else { rdp(equity / units, NAV_DP) }
}

pub fn units_for(amount: D, nav: D) -> D {
    if nav <= ZERO { ZERO } else { rdp(amount / nav, NAV_DP) }
}

/// Unit-weighted blend of an investor's HWM with the NAV new units were bought at.
pub fn blended_hwm(old_units: D, old_hwm: D, new_units: D, nav: D) -> D {
    let total = old_units + new_units;
    if total <= ZERO { nav } else { rdp((old_units * old_hwm + new_units * nav) / total, NAV_DP) }
}

/// Fee of one investor at a rollover: (fee money, units taken).
pub fn investor_fee(pct: D, nav: D, hwm: D, units: D) -> (D, D) {
    if pct <= ZERO || units <= ZERO || nav <= hwm || nav <= ZERO {
        return (ZERO, ZERO);
    }
    let fee = r2((nav - hwm) * units * pct / HUNDRED);
    if fee <= ZERO {
        return (ZERO, ZERO);
    }
    (fee, rdp(fee / nav, NAV_DP).min(units))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Holding {
    pub investor: i64,
    pub units: D,
    pub hwm: D,
    pub is_master: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlanReq {
    Invest { id: i64, investor: i64, amount: D },
    /// `units` None = everything the investor holds.
    Redeem { id: i64, investor: i64, units: Option<D> },
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RolloverPlan {
    pub nav: D,
    pub equity_before: D,
    pub units_before: D,
    /// (investor, fee money, units taken, hwm after)
    pub fees: Vec<(i64, D, D, D)>,
    /// (request, investor, units, amount)
    pub redemptions: Vec<(i64, i64, D, D)>,
    /// (request, investor, units, hwm after)
    pub investments: Vec<(i64, i64, D, D)>,
    /// Requests refused for good (refund invests).
    pub rejected: Vec<(i64, &'static str)>,
    /// Requests left pending (not enough free margin yet).
    pub deferred: Vec<(i64, &'static str)>,
    pub fees_total: D,
    pub redeemed_total: D,
    pub invested_total: D,
    pub equity_after: D,
    pub units_after: D,
}

/// Plans a rollover: fees at NAV (taken as units, NAV unchanged), then redemptions, then investments,
/// all at the same NAV. `free_cash` is what can leave the account now (free margin); `min_own_pct` is the
/// share of units the master must keep (D68).
pub fn plan_rollover(equity: D, holdings: &[Holding], reqs: &[PlanReq], fee_pct: D, min_own_pct: D, free_cash: D) -> RolloverPlan {
    let units_before: D = holdings.iter().map(|h| h.units).sum();
    let nav = nav(equity, units_before);
    let mut plan = RolloverPlan { nav, equity_before: equity, units_before, ..Default::default() };
    let mut units: std::collections::BTreeMap<i64, D> = holdings.iter().map(|h| (h.investor, h.units)).collect();
    let mut hwm: std::collections::BTreeMap<i64, D> = holdings.iter().map(|h| (h.investor, h.hwm)).collect();
    let master: Option<i64> = holdings.iter().find(|h| h.is_master).map(|h| h.investor);
    let mut total = units_before;

    // 1. fees (the master pays none on its own capital)
    for h in holdings.iter().filter(|h| !h.is_master) {
        let (fee, fu) = investor_fee(fee_pct, nav, h.hwm, h.units);
        if fee > ZERO {
            *units.get_mut(&h.investor).unwrap() -= fu;
            total -= fu;
            hwm.insert(h.investor, nav);
            plan.fees.push((h.investor, fee, fu, nav));
            plan.fees_total += fee;
        }
    }
    let master_ok = |units: &std::collections::BTreeMap<i64, D>, total: D| -> bool {
        match master {
            None => true,
            Some(m) => total <= ZERO || min_own_pct <= ZERO || units.get(&m).copied().unwrap_or(ZERO) * HUNDRED >= min_own_pct * total,
        }
    };
    let mut cash = free_cash - plan.fees_total;

    // 2. redemptions
    for r in reqs {
        let PlanReq::Redeem { id, investor, units: want } = r else { continue };
        let have = units.get(investor).copied().unwrap_or(ZERO);
        let u = want.unwrap_or(have).min(have);
        if u <= ZERO {
            plan.rejected.push((*id, "insufficient_units"));
            continue;
        }
        let amount = r2(u * nav);
        if Some(*investor) == master && total - u > ZERO {
            let mut probe = units.clone();
            *probe.get_mut(investor).unwrap() -= u;
            if !master_ok(&probe, total - u) {
                plan.rejected.push((*id, "master_share"));
                continue;
            }
        }
        if amount > cash {
            plan.deferred.push((*id, "insufficient_free_margin"));
            continue;
        }
        cash -= amount;
        *units.get_mut(investor).unwrap() -= u;
        total -= u;
        plan.redemptions.push((*id, *investor, u, amount));
        plan.redeemed_total += amount;
    }

    // 3. investments
    for r in reqs {
        let PlanReq::Invest { id, investor, amount } = r else { continue };
        let u = units_for(*amount, nav);
        if u <= ZERO {
            plan.rejected.push((*id, "invalid_amount"));
            continue;
        }
        if Some(*investor) != master {
            let mut probe = units.clone();
            *probe.entry(*investor).or_insert(ZERO) += u;
            if !master_ok(&probe, total + u) {
                plan.rejected.push((*id, "master_share"));
                continue;
            }
        }
        let old = units.get(investor).copied().unwrap_or(ZERO);
        let h = blended_hwm(old, hwm.get(investor).copied().unwrap_or(nav), u, nav);
        units.insert(*investor, old + u);
        hwm.insert(*investor, h);
        total += u;
        plan.investments.push((*id, *investor, u, h));
        plan.invested_total += *amount;
    }
    plan.units_after = total;
    plan.equity_after = equity - plan.fees_total - plan.redeemed_total + plan.invested_total;
    plan
}

/// Lots of a closed fund deal pushed to the IB service are kept to 4 decimals.
pub const IB_LOTS_DP: u32 = 4;

/// Splits a closed fund deal's volume across the fund's holders by units (IB lots, D64), largest remainder:
/// every share is rounded down to 4 decimals, then the leftover 0.0001 steps go to the largest remainders
/// (ties: more units, then the lower user id), so the shares always add up to the deal volume (to 4 dp).
/// `holdings` = (user, units); holders without units and zero shares are left out.
pub fn split_lots(volume: D, holdings: &[(i64, D)]) -> Vec<(i64, D)> {
    let held: Vec<(i64, D)> = holdings.iter().copied().filter(|h| h.1 > ZERO).collect();
    let total: D = held.iter().map(|h| h.1).sum();
    if total <= ZERO || volume <= ZERO {
        return vec![];
    }
    let step = D::new(1, IB_LOTS_DP);
    let target = rdp(volume, IB_LOTS_DP);
    let mut rows: Vec<(i64, D, D, D)> = held
        .iter()
        .map(|&(user, units)| {
            let raw = volume * units / total;
            let floor = raw.round_dp_with_strategy(IB_LOTS_DP, rust_decimal::RoundingStrategy::ToZero);
            (user, units, floor, raw - floor)
        })
        .collect();
    let mut left = ((target - rows.iter().map(|r| r.2).sum::<D>()) / step).round();
    let mut order: Vec<usize> = (0..rows.len()).collect();
    order.sort_by(|&a, &b| rows[b].3.cmp(&rows[a].3).then(rows[b].1.cmp(&rows[a].1)).then(rows[a].0.cmp(&rows[b].0)));
    for i in order {
        if left <= ZERO {
            break;
        }
        rows[i].2 += step;
        left -= ONE;
    }
    rows.into_iter().filter(|r| r.2 > ZERO).map(|r| (r.0, r.2.normalize())).collect()
}

/* ------------------------------------------------------------------ */
/* Statistics (D72)                                                    */
/* ------------------------------------------------------------------ */

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DayPoint {
    pub day: NaiveDate,
    /// End-of-day equity (USD).
    pub equity: f64,
    /// Net external flow during the day (deposits − withdrawals, USD).
    pub flow: f64,
}

/// Time-weighted return index: I₀ = 1, I_t = I_{t−1} × (E_t − F_t) ÷ E_{t−1}. Flows do not move it.
pub fn index_series(points: &[DayPoint]) -> Vec<(NaiveDate, f64)> {
    let mut out = Vec::with_capacity(points.len());
    let mut idx = 1.0f64;
    let mut prev: Option<f64> = None;
    for p in points {
        if let Some(pe) = prev
            && pe > 0.0
        {
            idx *= ((p.equity - p.flow) / pe).max(0.0);
        }
        out.push((p.day, idx));
        prev = Some(p.equity);
    }
    out
}

/// Return (%) from the last point at or before `from` to the end.
pub fn period_return(series: &[(NaiveDate, f64)], from: Option<NaiveDate>) -> f64 {
    let Some(last) = series.last() else { return 0.0 };
    let base = match from {
        None => 1.0,
        Some(f) => series.iter().rev().find(|(d, _)| *d <= f).map(|(_, v)| *v).unwrap_or(1.0),
    };
    if base <= 0.0 { 0.0 } else { (last.1 / base - 1.0) * 100.0 }
}

/// (max drawdown, current drawdown) as fractions.
pub fn drawdowns(series: &[(NaiveDate, f64)]) -> (f64, f64) {
    let mut peak = 0.0f64;
    let mut max = 0.0f64;
    let mut cur = 0.0f64;
    for (_, v) in series {
        peak = peak.max(*v);
        cur = if peak > 0.0 { 1.0 - v / peak } else { 0.0 };
        max = max.max(cur);
    }
    (max, cur)
}

/// Annualised volatility (fraction) of daily index returns.
pub fn volatility(series: &[(NaiveDate, f64)]) -> f64 {
    let rets: Vec<f64> = series.windows(2).filter(|w| w[0].1 > 0.0).map(|w| w[1].1 / w[0].1 - 1.0).collect();
    if rets.len() < 2 {
        return 0.0;
    }
    let mean = rets.iter().sum::<f64>() / rets.len() as f64;
    let var = rets.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (rets.len() - 1) as f64;
    var.sqrt() * 252f64.sqrt()
}

/// System risk score 1–10: `raw = 0.6 × min(maxDD ÷ 50%, 1) + 0.4 × min(volatility ÷ 100%, 1)`,
/// `score = clamp(1 + round(9 × raw), 1, 10)`.
pub fn risk_score(max_dd: f64, vol: f64) -> u8 {
    let raw = 0.6 * (max_dd / 0.5).clamp(0.0, 1.0) + 0.4 * (vol / 1.0).clamp(0.0, 1.0);
    (1.0 + (9.0 * raw).round()).clamp(1.0, 10.0) as u8
}

/// Risk score straight from daily points.
pub fn risk_score_from(points: &[DayPoint]) -> u8 {
    let s = index_series(points);
    risk_score(drawdowns(&s).0, volatility(&s))
}

/// Month-by-month returns (%), chaining the index at month ends.
pub fn monthly_returns(series: &[(NaiveDate, f64)]) -> Vec<(String, f64)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    let mut base = 1.0f64;
    let mut i = 0;
    while i < series.len() {
        let (y, m) = (series[i].0.year(), series[i].0.month());
        let mut j = i;
        while j + 1 < series.len() && series[j + 1].0.year() == y && series[j + 1].0.month() == m {
            j += 1;
        }
        let end = series[j].1;
        out.push((format!("{y:04}-{m:02}"), if base > 0.0 { (end / base - 1.0) * 100.0 } else { 0.0 }));
        base = end;
        i = j + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testkit::d;

    fn spec(sym: &str) -> Spec {
        crate::specs::test_specs().get(sym).unwrap().clone()
    }

    #[test]
    fn sizing_modes() {
        let eu = spec("EURUSD"); // min 0.01, step 0.01
        let eq = Sizing { mode: SizingMode::Equity, value: ONE };
        // follower 2 500 vs master 10 000 → a quarter of 1.00 lot
        assert_eq!(open_volume(&eq, d("1"), d("10000"), d("2500"), &eu, None), Some(d("0.25")));
        // rounds down to the step: 0.33 × 1/3 = 0.11
        assert_eq!(open_volume(&eq, d("0.33"), d("3000"), d("1000"), &eu, None), Some(d("0.11")));
        // below the minimum lot → skipped
        assert_eq!(open_volume(&eq, d("0.01"), d("10000"), d("500"), &eu, None), None);
        let alloc = Sizing { mode: SizingMode::Allocation, value: d("5000") };
        assert_eq!(open_volume(&alloc, d("2"), d("20000"), d("1"), &eu, None), Some(d("0.5")));
        let mult = Sizing { mode: SizingMode::Multiplier, value: d("1.5") };
        assert_eq!(open_volume(&mult, d("0.3"), d("1"), d("1"), &eu, None), Some(d("0.45")));
        // max lot per trade caps it
        assert_eq!(open_volume(&mult, d("0.3"), d("1"), d("1"), &eu, Some(d("0.2"))), Some(d("0.2")));
        let fixed = Sizing { mode: SizingMode::FixedLot, value: d("0.1") };
        assert_eq!(open_volume(&fixed, d("7"), d("1"), d("1"), &eu, None), Some(d("0.1")));
        // symbol max lot
        assert_eq!(open_volume(&mult, d("80"), d("1"), d("1"), &eu, None), Some(eu.lot_max));
        // no equity yet → nothing copied
        assert_eq!(open_volume(&eq, d("1"), d("10000"), d("0"), &eu, None), None);
    }

    #[test]
    fn adds_and_partial_closes_are_proportional() {
        let eu = spec("EURUSD");
        let fixed = Sizing { mode: SizingMode::FixedLot, value: d("0.1") };
        // master 1.0 → adds 0.5; the fixed-lot follower holds 0.1 → adds 0.05
        assert_eq!(add_volume(&fixed, d("0.5"), d("1"), d("0.1"), d("1"), d("1"), &eu, None), Some(d("0.05")));
        let eq = Sizing { mode: SizingMode::Equity, value: ONE };
        assert_eq!(add_volume(&eq, d("0.5"), d("1"), d("0.1"), d("10000"), d("2000"), &eu, None), Some(d("0.1")));
        // master closes 0.4 of 1.0 (0.6 left): follower 0.25 → 0.1
        assert_eq!(close_volume(d("0.25"), d("0.4"), d("0.6"), &eu), Some(d("0.1")));
        // half of 0.03 → 0.01 (rounded down)
        assert_eq!(close_volume(d("0.03"), d("1"), d("1"), &eu), Some(d("0.01")));
        // a tenth of 0.05 rounds to 0 → nothing now
        assert_eq!(close_volume(d("0.05"), d("0.1"), d("0.9"), &eu), None);
        // remainder below the minimum lot → close everything (symbol with min 0.10, step 0.01)
        let big = Spec { lot_min: d("0.1"), ..eu.clone() };
        assert_eq!(close_volume(d("0.15"), d("1"), d("1"), &big), Some(d("0.15")));
        // master closed fully
        assert_eq!(close_volume(d("0.37"), d("1"), d("0"), &eu), Some(d("0.37")));
    }

    #[test]
    fn copy_fee_high_water_mark() {
        // start 1 000, grew to 1 200 → 20 % of 200 = 40; HWM 1 160
        let f = copy_fee(d("20"), d("1000"), ZERO, d("1200"));
        assert_eq!((f.fee, f.hwm_before, f.hwm_after), (d("40.00"), d("1000"), d("1160.00")));
        // next period: drops to 1 100 → no fee, HWM stays 1 160
        let g = copy_fee(d("20"), f.hwm_after, ZERO, d("1100"));
        assert_eq!((g.fee, g.hwm_after), (ZERO, d("1160.00")));
        // recovers to 1 210 → fee only on the 50 above the HWM
        let h = copy_fee(d("20"), g.hwm_after, ZERO, d("1210"));
        assert_eq!(h.fee, d("10.00"));
        // a 500 deposit is not profit: HWM 1 200 + 500, equity 1 750 → fee on 50
        let k = copy_fee(d("20"), d("1200"), d("500"), d("1750"));
        assert_eq!((k.fee, k.hwm_before), (d("10.00"), d("1700")));
        // a withdrawal lowers the mark
        let w = copy_fee(d("20"), d("1200"), d("-200"), d("1100"));
        assert_eq!((w.fee, w.hwm_before, w.hwm_after), (d("20.00"), d("1000"), d("1080.00")));
        assert_eq!(split_fee(d("40.00"), d("20")), (d("8.00"), d("32.00")));
    }

    #[test]
    fn ib_lots_split_adds_up_to_the_deal_volume() {
        let sum = |v: &[(i64, D)]| v.iter().map(|x| x.1).sum::<D>();
        // three equal holders of 1 lot: 0.3333 each would lose 0.0001; the largest remainder (tie → lower id) takes it
        let s = split_lots(d("1"), &[(3, d("100")), (1, d("100")), (2, d("100"))]);
        assert_eq!(s, vec![(3, d("0.3333")), (1, d("0.3334")), (2, d("0.3333"))]);
        assert_eq!(sum(&s), d("1"));
        // master 1 000 units, investors 4 000 and 2 000: 0.07 lots
        let s = split_lots(d("0.07"), &[(1, d("1000")), (2, d("4000")), (3, d("2000"))]);
        assert_eq!(s, vec![(1, d("0.01")), (2, d("0.04")), (3, d("0.02"))]);
        // rounding every share independently (half up) would pay 0.0002 + 0.0002 + 0.0002 for a 0.0005 deal
        let s = split_lots(d("0.0005"), &[(1, d("1")), (2, d("1")), (3, d("1"))]);
        assert_eq!(sum(&s), d("0.0005"));
        assert_eq!(s, vec![(1, d("0.0002")), (2, d("0.0002")), (3, d("0.0001"))]);
        // a tiny holder whose share rounds to 0 is left out; holders without units never get anything
        let s = split_lots(d("0.01"), &[(1, d("99999")), (2, d("1")), (3, ZERO)]);
        assert_eq!(s, vec![(1, d("0.01"))]);
        assert!(split_lots(d("1"), &[(1, ZERO)]).is_empty());
        assert!(split_lots(ZERO, &[(1, d("5"))]).is_empty());
        // many holders, odd volume: always exact
        let many: Vec<(i64, D)> = (1..=37).map(|i| (i, D::from(i * 13 % 7 + 1))).collect();
        assert_eq!(sum(&split_lots(d("2.37"), &many)), d("2.37"));
    }

    #[test]
    fn nav_units_and_rollover_plan() {
        // master seeds 1 000 at NAV 1 → 1 000 units
        assert_eq!(nav(d("1000"), ZERO), ONE);
        assert_eq!(units_for(d("1000"), ONE), d("1000"));
        // investor A invests 4 000 at NAV 1 at the first rollover
        let p1 = plan_rollover(d("1000"), &[Holding { investor: 1, units: d("1000"), hwm: ONE, is_master: true }], &[PlanReq::Invest { id: 10, investor: 2, amount: d("4000") }], d("20"), d("10"), d("1000"));
        assert_eq!(p1.nav, ONE);
        assert_eq!(p1.investments, vec![(10, 2, d("4000"), ONE)]);
        assert_eq!((p1.units_after, p1.equity_after), (d("5000"), d("5000")));
        // the fund makes 10 %: equity 5 500, NAV 1.1. A pays 20 % of 0.1 × 4 000 = 80 → 72.72727273 units
        let hold = [Holding { investor: 1, units: d("1000"), hwm: ONE, is_master: true }, Holding { investor: 2, units: d("4000"), hwm: ONE, is_master: false }];
        let p2 = plan_rollover(d("5500"), &hold, &[PlanReq::Redeem { id: 11, investor: 2, units: None }], d("20"), d("10"), d("5500"));
        assert_eq!(p2.nav, d("1.1"));
        assert_eq!(p2.fees, vec![(2, d("80.00"), d("72.72727273"), d("1.1"))]);
        // A redeems what is left: 3 927.27272727 units × 1.1 = 4 320.00
        assert_eq!(p2.redemptions, vec![(11, 2, d("3927.27272727"), d("4320.00"))]);
        assert_eq!(p2.equity_after, d("1100.00"));
        assert_eq!(p2.units_after, d("1000"));
        // NAV is unchanged by the fee and the redemption: 1 100 / 1 000 = 1.1
        assert_eq!(nav(p2.equity_after, p2.units_after), d("1.1"));
        // investor value: 4 000 × 1.1 = 4 400 = 4 320 paid out + 80 fee
        assert_eq!(p2.redeemed_total + p2.fees_total, d("4400.00"));
    }

    #[test]
    fn rollover_blends_hwm_and_protects_master_share() {
        let hold = [Holding { investor: 1, units: d("100"), hwm: ONE, is_master: true }, Holding { investor: 2, units: d("900"), hwm: d("1.2"), is_master: false }];
        // NAV 1.1 below A's HWM 1.2: no fee; A adds 1 100 → 1 000 units, HWM blends to 1.147
        let p = plan_rollover(d("1100"), &hold, &[PlanReq::Invest { id: 1, investor: 2, amount: d("1100") }], d("20"), d("5"), d("1100"));
        assert!(p.fees.is_empty());
        assert_eq!(p.investments, vec![(1, 2, d("1000"), d("1.14736842"))]);
        // the master may not drop below 5 %: 100 of 1 000 units, redeeming 60 would leave 40 of 940 (4.3 %)
        let q = plan_rollover(d("1000"), &[Holding { investor: 1, units: d("100"), hwm: ONE, is_master: true }, Holding { investor: 2, units: d("900"), hwm: ONE, is_master: false }], &[PlanReq::Redeem { id: 2, investor: 1, units: Some(d("60")) }], d("20"), d("5"), d("1000"));
        assert_eq!(q.rejected, vec![(2, "master_share")]);
        // an investment that dilutes the master below 5 % is refused
        let r = plan_rollover(d("1000"), &[Holding { investor: 1, units: d("100"), hwm: ONE, is_master: true }, Holding { investor: 2, units: d("900"), hwm: ONE, is_master: false }], &[PlanReq::Invest { id: 3, investor: 3, amount: d("1500") }], d("20"), d("5"), d("1000"));
        assert_eq!(r.rejected, vec![(3, "master_share")]);
        // not enough free margin: the redemption waits
        let s = plan_rollover(d("1000"), &[Holding { investor: 1, units: d("100"), hwm: ONE, is_master: true }, Holding { investor: 2, units: d("900"), hwm: ONE, is_master: false }], &[PlanReq::Redeem { id: 4, investor: 2, units: None }], d("20"), d("5"), d("300"));
        assert_eq!(s.deferred, vec![(4, "insufficient_free_margin")]);
        assert_eq!(s.units_after, d("1000"));
    }

    #[test]
    fn statistics_and_risk_score() {
        let day = |i: u32| NaiveDate::from_ymd_opt(2026, 8, 30).unwrap() + chrono::Duration::days(i as i64);
        // 1 000 → 1 100 (+10 %) → deposit 1 000 same day equity 2 100 (flow ignored) → 1 890 (−10 %)
        let pts = [
            DayPoint { day: day(0), equity: 1000.0, flow: 1000.0 },
            DayPoint { day: day(1), equity: 1100.0, flow: 0.0 },
            DayPoint { day: day(2), equity: 2100.0, flow: 1000.0 },
            DayPoint { day: day(3), equity: 1890.0, flow: 0.0 },
        ];
        let s = index_series(&pts);
        let v: Vec<f64> = s.iter().map(|x| (x.1 * 10000.0).round() / 10000.0).collect();
        assert_eq!(v, vec![1.0, 1.1, 1.1, 0.99]);
        assert!((period_return(&s, None) - (-1.0)).abs() < 1e-9);
        assert!((period_return(&s, Some(day(1))) - (-10.0)).abs() < 1e-9);
        let (max, cur) = drawdowns(&s);
        assert!((max - 0.1).abs() < 1e-9 && (cur - 0.1).abs() < 1e-9);
        let m = monthly_returns(&s);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].0, "2026-08");
        assert!((m[0].1 - 10.0).abs() < 1e-9);
        assert!((m[1].1 - (-10.0)).abs() < 1e-9);
        // risk score anchors
        assert_eq!(risk_score(0.0, 0.0), 1);
        assert_eq!(risk_score(0.10, 0.20), 3);
        assert_eq!(risk_score(0.40, 0.80), 8);
        assert_eq!(risk_score(0.50, 1.00), 10);
        assert_eq!(risk_score(0.9, 3.0), 10);
        assert!(volatility(&s) > 0.0);
    }
}

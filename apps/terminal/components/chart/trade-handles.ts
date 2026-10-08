/**
 * The S / T handles on a position line (Kalks Trader web chart, chart-view.tsx; the app's chart page
 * apps/mobile/assets/chart/chart.html follows the same rules). A position without a stop loss shows "S", one without
 * a take profit shows "T": drag a handle to place that stop, or tap it for the order tickets' starting distance.
 * Pure functions (no imports), so `node --test apps/terminal/tests` runs them as they are.
 */

export type StopKind = "sl" | "tp";
export type Side = "buy" | "sell";
export interface StopQuote {
  bid: number;
  ask: number;
}

/** "st", "s", "t" or "": the handles of a position line (none when the account is read-only). */
export function lineHandles(p: { sl?: number | null; tp?: number | null }, readOnly: boolean): string {
  if (readOnly) return "";
  return `${p.sl == null ? "s" : ""}${p.tp == null ? "t" : ""}`;
}

const round = (v: number, digits: number) => +v.toFixed(digits);

/**
 * Holds a stop on the side of the close price that the trade server accepts (services/trading check_sltp): a buy
 * closes at the bid, so its stop loss stays at least the stops level (and one point) below it and its take profit
 * as far above; a sell closes at the ask, mirrored. `gap` = the stops level in price units.
 */
export function clampStop(kind: StopKind, side: Side, price: number, q: StopQuote, gap: number, digits: number): number {
  const point = 1 / 10 ** digits;
  const lim = Math.max(gap, point);
  const ref = side === "buy" ? q.bid : q.ask;
  // a buy's stop loss and a sell's take profit sit below the close price
  const below = (side === "buy") === (kind === "sl");
  return round(below ? Math.min(price, ref - lim) : Math.max(price, ref + lim), digits);
}

/**
 * Where a tapped handle puts the stop: the order tickets' starting distance (order-ticket.tsx toggleStop) — twice the
 * spread, at least 10 pips, for the stop loss and twice that for the take profit — measured from the price the
 * position closes at now, then held on the valid side.
 */
export function defaultStop(kind: StopKind, side: Side, q: StopQuote, pip: number, gap: number, digits: number): number {
  const spreadPips = Math.max(1, (q.ask - q.bid) / pip);
  const base = Math.max(10, Math.round(spreadPips * 2));
  const dist = (kind === "sl" ? base : base * 2) * pip;
  const ref = side === "buy" ? q.bid : q.ask;
  const up = (side === "buy") === (kind === "tp");
  return clampStop(kind, side, ref + (up ? dist : -dist), q, gap, digits);
}

// The S / T handles on a chart position line (components/chart/trade-handles.ts): `node --test apps/terminal/tests`.
// What it guards: a handle shows only for a missing stop and never on a read-only account; a tapped handle puts the
// stop at the order tickets' starting distance (max(10 pips, 2 x spread), take profit twice that) from the price the
// position closes at; a dragged stop is held on the side the trade server accepts (stops level, at least one point).
import { test } from "node:test";
import assert from "node:assert/strict";
import { clampStop, defaultStop, lineHandles } from "../components/chart/trade-handles.ts";

test("a position line shows S without a stop loss and T without a take profit, none when read-only", () => {
  assert.equal(lineHandles({}, false), "st");
  assert.equal(lineHandles({ sl: 1.08 }, false), "t");
  assert.equal(lineHandles({ tp: 1.09 }, false), "s");
  assert.equal(lineHandles({ sl: 1.08, tp: 1.09 }, false), "");
  assert.equal(lineHandles({ sl: null, tp: null }, false), "st");
  assert.equal(lineHandles({}, true), "");
});

test("tap: 10 pips (tight spread) for the stop loss, 20 for the take profit, from the close price", () => {
  const q = { bid: 1.085, ask: 1.0851 }; // EURUSD, 1 pip spread
  assert.equal(defaultStop("sl", "buy", q, 0.0001, 0, 5), 1.084);
  assert.equal(defaultStop("tp", "buy", q, 0.0001, 0, 5), 1.087);
  assert.equal(defaultStop("sl", "sell", q, 0.0001, 0, 5), 1.0861);
  assert.equal(defaultStop("tp", "sell", q, 0.0001, 0, 5), 1.0831);
});

test("tap: a wide spread widens the distance to twice the spread", () => {
  const q = { bid: 2650.0, ask: 2650.8 }; // XAUUSD, 8 pips of 0.1
  // 2 x 8 = 16 pips = 1.6; take profit 32 pips = 3.2
  assert.equal(defaultStop("sl", "buy", q, 0.1, 0, 2), 2648.4);
  assert.equal(defaultStop("tp", "buy", q, 0.1, 0, 2), 2653.2);
  assert.equal(defaultStop("sl", "sell", q, 0.1, 0, 2), 2652.4);
  assert.equal(defaultStop("tp", "sell", q, 0.1, 0, 2), 2647.6);
});

test("drag: a stop is held on the valid side of the close price, the stops level away", () => {
  const q = { bid: 1.085, ask: 1.0851 };
  const gap = 0.0001; // 10 points
  // buy closes at the bid: SL at most bid - gap, TP at least bid + gap
  assert.equal(clampStop("sl", "buy", 1.0899, q, gap, 5), 1.0849);
  assert.equal(clampStop("sl", "buy", 1.08, q, gap, 5), 1.08);
  assert.equal(clampStop("tp", "buy", 1.08, q, gap, 5), 1.0851);
  assert.equal(clampStop("tp", "buy", 1.09, q, gap, 5), 1.09);
  // sell closes at the ask: SL at least ask + gap, TP at most ask - gap
  assert.equal(clampStop("sl", "sell", 1.08, q, gap, 5), 1.0852);
  assert.equal(clampStop("tp", "sell", 1.09, q, gap, 5), 1.085);
  // no stops level: still one point away (the server refuses a stop at the price itself)
  assert.equal(clampStop("sl", "buy", 1.085, q, 0, 5), 1.08499);
  assert.equal(clampStop("tp", "sell", 1.0851, q, 0, 5), 1.08509);
});

test("tap: the default distance never lands inside a large stops level", () => {
  const q = { bid: 100.0, ask: 100.02 };
  // 10 pips of 0.01 = 0.10, but the stops level is 0.50
  assert.equal(defaultStop("sl", "buy", q, 0.01, 0.5, 2), 99.5);
  assert.equal(defaultStop("tp", "sell", q, 0.01, 0.5, 2), 99.52);
});

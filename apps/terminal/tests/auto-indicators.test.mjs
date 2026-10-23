// No automatic indicators on a chart (lib/indicators.ts isAutoIndicatorPair / stripAutoIndicators, used by
// lib/store.tsx readWorkspace at uiVersion 4): `node --test apps/terminal/tests`.
// What it guards: a saved workspace from before uiVersion 4 loses the EMA 50 + SMA 20 every chart started with, but only
// while that pair is untouched (same params, order, visibility, styles, levels); anything the client changed stays.
// The app does the same (apps/mobile indicators.dart isAutoIndicatorPair, test/terminal/chart_menu_test.dart).
import { test } from "node:test";
import assert from "node:assert/strict";
import { isAutoIndicatorPair, makeInstance, migrateIndicators, stripAutoIndicators } from "../lib/indicators.ts";

// what the old defaultWorkspace() put on the first chart, after a save + load round trip
const pair = () => migrateIndicators(JSON.parse(JSON.stringify(migrateIndicators(["ema50", "sma20"]))));

test("the old default pair is recognised as it was saved", () => {
  assert.equal(isAutoIndicatorPair(pair()), true);
  assert.equal(isAutoIndicatorPair([makeInstance("ema"), makeInstance("sma")]), true);
  // an explicit default style is still the default
  const [ema, sma] = pair();
  assert.equal(isAutoIndicatorPair([{ ...ema, style: { ma: {} } }, sma]), true);
});

test("anything the client changed is not the default pair", () => {
  const [ema, sma] = pair();
  const cases = {
    period: [ema, { ...sma, params: { ...sma.params, period: 21 } }],
    source: [ema, { ...sma, params: { ...sma.params, source: "hl2" } }],
    hidden: [{ ...ema, visible: false }, sma],
    colour: [{ ...ema, style: { ma: { color: "info" } } }, sma],
    width: [ema, { ...sma, style: { ma: { width: 3 } } }],
    levels: [{ ...ema, levels: [1] }, sma],
    added: [ema, sma, makeInstance("rsi")],
    removed: [ema],
    order: [sma, ema],
    empty: [],
  };
  for (const [name, list] of Object.entries(cases)) assert.equal(isAutoIndicatorPair(list), false, name);
});

test("the migration empties only untouched charts and moves the workspace to uiVersion 4", () => {
  const custom = [makeInstance("ema", [], { period: 50 }), makeInstance("sma", [], { period: 30 })];
  const ws = { uiVersion: 3, tabs: [{ id: "a", type: "area", indicators: pair() }, { id: "b", type: "candles", indicators: custom }, { id: "c", type: "candles", indicators: [makeInstance("bb")] }] };
  const out = stripAutoIndicators(ws);
  assert.equal(out.uiVersion, 4);
  assert.deepEqual(out.tabs[0], { id: "a", type: "area", indicators: [] });
  assert.equal(out.tabs[1].indicators, custom);
  assert.equal(out.tabs[2].indicators.length, 1);
  // the input is not changed
  assert.equal(ws.tabs[0].indicators.length, 2);
});

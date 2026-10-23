// No automatic indicators on a chart (lib/indicators.ts isAutoIndicatorPair / stripAutoIndicators, used by
// lib/store.tsx readWorkspace at uiVersion 4): `node --test apps/terminal/tests`.
// What it guards: a saved workspace from before uiVersion 4 loses the indicators the old default workspace added (EMA 50 +
// SMA 20 on the first chart; a lone Bollinger Bands on EURUSD, RSI on NAS100, MACD on BTCUSD), but only while they are
// untouched (same params, order, visibility, styles, levels); anything the client changed stays. The app does the same
// for the pair, the only default it ever had (apps/mobile indicators.dart isAutoIndicatorPair, chart_menu_test.dart).
import { test } from "node:test";
import assert from "node:assert/strict";
import { isAutoIndicatorPair, isAutoIndicatorSingle, makeInstance, migrateIndicators, stripAutoIndicators } from "../lib/indicators.ts";

// what the old defaultWorkspace() put on the first chart, after a save + load round trip
const saved = (ids) => migrateIndicators(JSON.parse(JSON.stringify(migrateIndicators(ids))));
const pair = () => saved(["ema50", "sma20"]);

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
  const ws = {
    uiVersion: 3,
    tabs: [
      { id: "a", symbol: "XAUUSD", type: "area", indicators: pair() },
      { id: "b", symbol: "XAUUSD", type: "candles", indicators: custom },
      { id: "c", symbol: "GBPUSD", type: "candles", indicators: [makeInstance("bb")] },
      { id: "d", symbol: "EURUSD", type: "candles", indicators: saved(["bb"]) },
      { id: "e", symbol: "NAS100", type: "candles", indicators: saved(["rsi"]) },
      { id: "f", symbol: "BTCUSD", type: "candles", indicators: saved(["macd"]) },
      { id: "g", symbol: "BTCUSD", type: "candles", indicators: [makeInstance("macd", [], { fast: 8 })] },
    ],
  };
  const out = stripAutoIndicators(ws);
  assert.equal(out.uiVersion, 4);
  assert.deepEqual(out.tabs[0], { id: "a", symbol: "XAUUSD", type: "area", indicators: [] });
  assert.equal(out.tabs[1].indicators, custom);
  assert.equal(out.tabs[2].indicators.length, 1); // BB was never automatic on GBPUSD
  assert.deepEqual(out.tabs.slice(3, 6).map((t) => t.indicators), [[], [], []]);
  assert.equal(out.tabs[6].indicators.length, 1); // a MACD with other inputs stays
  // the input is not changed
  assert.equal(ws.tabs[0].indicators.length, 2);
});

test("the other default charts: a lone untouched BB on EURUSD, RSI on NAS100, MACD on BTCUSD", () => {
  assert.equal(isAutoIndicatorSingle("EURUSD", saved(["bb"])), true);
  assert.equal(isAutoIndicatorSingle("NAS100", saved(["rsi"])), true);
  assert.equal(isAutoIndicatorSingle("BTCUSD", saved(["macd"])), true);
  assert.equal(isAutoIndicatorSingle("NAS100", [makeInstance("rsi")]), true);
  // only on the symbol that got it by default
  assert.equal(isAutoIndicatorSingle("XAUUSD", saved(["bb"])), false);
  assert.equal(isAutoIndicatorSingle("EURUSD", saved(["rsi"])), false);
  assert.equal(isAutoIndicatorSingle("GBPUSD", saved(["macd"])), false);
});

test("a default chart's indicator the client changed is kept", () => {
  const [bb] = saved(["bb"]);
  const [rsi] = saved(["rsi"]);
  const [macd] = saved(["macd"]);
  const cases = {
    params: ["EURUSD", [{ ...bb, params: { ...bb.params, period: 21 } }]],
    source: ["NAS100", [{ ...rsi, params: { ...rsi.params, source: "hlc3" } }]],
    hidden: ["BTCUSD", [{ ...macd, visible: false }]],
    colour: ["EURUSD", [{ ...bb, style: { upper: { color: "info" } } }]],
    levels: ["NAS100", [{ ...rsi, levels: [80, 20] }]],
    added: ["BTCUSD", [macd, makeInstance("ema")]],
    another: ["NAS100", [rsi, makeInstance("rsi", [rsi])]],
    empty: ["EURUSD", []],
  };
  for (const [name, [symbol, list]] of Object.entries(cases)) assert.equal(isAutoIndicatorSingle(symbol, list), false, name);
  // the pair rule still does not depend on the symbol, and a lone default is not the pair
  assert.equal(isAutoIndicatorPair(saved(["bb"])), false);
});

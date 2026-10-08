// Module switches in Kalks Trader (lib/modules.ts): `node --test apps/terminal/tests`.
// What it guards: a module the gateway doesn't list (or an unknown config) counts as on; "a+b" needs every module
// (options explain = Options and AI), "a|b" any one; the toolbox drops the News, Calendar, AI Trader and MAM tabs of
// switched-off modules and keeps every other tab.
import { test } from "node:test";
import assert from "node:assert/strict";
import { TAB_MODULES, isModuleOn, tabOn } from "../lib/modules.ts";

test("missing keys and unknown configs are on; false is off", () => {
  assert.equal(isModuleOn(null, "options"), true);
  assert.equal(isModuleOn(undefined, "news"), true);
  assert.equal(isModuleOn({}, "news"), true);
  assert.equal(isModuleOn({ news: true }, "news"), true);
  assert.equal(isModuleOn({ news: false }, "news"), false);
  assert.equal(isModuleOn({ news: false }, "calendar"), true);
});

test("'+' needs every module, '|' any one of them", () => {
  assert.equal(isModuleOn({ options: true, ai: true }, "options+ai"), true);
  assert.equal(isModuleOn({ ai: false }, "options+ai"), false);
  assert.equal(isModuleOn({ options: false }, "options+ai"), false);
  assert.equal(isModuleOn({ algo: false }, "algo|api"), true);
  assert.equal(isModuleOn({ algo: false, api: false }, "algo|api"), false);
});

test("the toolbox drops only the tabs of switched-off modules", () => {
  assert.deepEqual(TAB_MODULES, { news: "news", calendar: "calendar", ai: "ai", mam: "mam" });
  const off = { news: false, calendar: false, ai: false, mam: false, options: false };
  for (const tab of ["news", "calendar", "ai", "mam"]) {
    assert.equal(tabOn(off, tab), false, tab);
    assert.equal(tabOn({}, tab), true, tab);
  }
  for (const tab of ["positions", "pending", "history", "alerts", "exposure", "journal", "options", "orders", "closed", "settlements"]) assert.equal(tabOn(off, tab), true, tab);
});

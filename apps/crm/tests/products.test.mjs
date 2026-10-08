// CFD / Options account split (lib/products.ts): `node --test apps/crm/tests`.
// What it guards: the open-account wizard offers a product's own groups only (never prop / copy / PAMM / MAM or the
// market maker's), counts the account limit per (live / demo, product) like the engine, the account lists split by
// product (CFD first), and an older engine without `product` reads as CFD everywhere.
import { test } from "node:test";
import assert from "node:assert/strict";
import { byProduct, offeredGroups, optionsModuleOn, parseProduct, productOf, productOrder, usedFor } from "../lib/products.ts";

const g = (code, product, accountTypes = "both", enabled = true) => ({ code, product, accountTypes, enabled });
const GROUPS = [
  g("standard", "cfd"),
  g("pro", undefined), // an older engine sends no product: CFD
  g("ecn", "cfd", "live"),
  g("prop-50k", "cfd"),
  g("copy", "cfd", "live", false),
  g("options-mm", "options", "both", false),
  g("options-standard", "options"),
  g("options-pro", "options", "both", false),
  g("options-live", "options", "live"),
];

test("the wizard offers the chosen product's groups for the chosen account type", () => {
  assert.deepEqual(offeredGroups(GROUPS, "live", "cfd").map((x) => x.code), ["standard", "pro", "ecn"]);
  assert.deepEqual(offeredGroups(GROUPS, "demo", "cfd").map((x) => x.code), ["standard", "pro"]);
  assert.deepEqual(offeredGroups(GROUPS, "live", "options").map((x) => x.code), ["options-standard", "options-live"]);
  assert.deepEqual(offeredGroups(GROUPS, "demo", "options").map((x) => x.code), ["options-standard"]);
  // the market maker's group is never offered, even if a broker enabled it
  assert.deepEqual(offeredGroups([g("options-mm", "options")], "live", "options"), []);
});

test("the account limit counts per (live / demo, product), never archived, closed or platform accounts", () => {
  const acc = (type, product, status = "active", group = "standard") => ({ type, product, status, group });
  const accounts = [
    acc("live", "cfd"),
    acc("live", undefined, "active", "pro"),
    acc("live", "cfd", "archived"),
    acc("live", "cfd", "closed"),
    acc("live", "cfd", "active", "copy"),
    acc("live", "cfd", "active", "prop-50k"),
    acc("demo", "cfd"),
    acc("live", "options", "active", "options-standard"),
  ];
  assert.equal(usedFor(accounts, "live", "cfd"), 2);
  assert.equal(usedFor(accounts, "demo", "cfd"), 1);
  assert.equal(usedFor(accounts, "live", "options"), 1);
  assert.equal(usedFor(accounts, "demo", "options"), 0);
});

test("lists split by product, CFD first, order kept; no product = CFD", () => {
  const list = [{ login: 1, product: "options" }, { login: 2 }, { login: 3, product: "cfd" }, { login: 4, product: "options" }];
  const s = byProduct(list);
  assert.deepEqual(s.cfd.map((a) => a.login), [2, 3]);
  assert.deepEqual(s.options.map((a) => a.login), [1, 4]);
  assert.deepEqual(productOrder(list).map((a) => a.login), [2, 3, 1, 4]);
  assert.equal(productOf(null), "cfd");
  assert.equal(productOf({ product: "OPTIONS" }), "cfd", "only the engine's exact value");
});

test("links and the Options module switch", () => {
  assert.equal(parseProduct("options"), "options");
  assert.equal(parseProduct("cfd"), "cfd");
  assert.equal(parseProduct("forex"), null);
  assert.equal(parseProduct(null), null);
  assert.equal(optionsModuleOn(undefined), true, "unknown config = on");
  assert.equal(optionsModuleOn({}), true, "a key the gateway doesn't send = on");
  assert.equal(optionsModuleOn({ options: true }), true);
  assert.equal(optionsModuleOn({ options: false }), false);
});

// CFD / Options account split in Kalks Trader (lib/options/product.ts): `node --test apps/terminal/tests`.
// What it guards: the active account's product decides the workspace (the in-memory switch only without an engine
// account); `?mode=options` opens the client's Options account on start (never over the SSO account); the CFD |
// Options control switches to an account of that product or offers to open one; an older engine without `product`
// reads as CFD.
import { test } from "node:test";
import assert from "node:assert/strict";
import { groupAccounts, openAccountPath, parseMode, pickSession, productOf, switchTarget, workspaceOf } from "../lib/options/product.ts";

const s = (login, product) => ({ login, account: product === null ? null : { product } });

test("the workspace follows the account's product, the switch only without an engine account", () => {
  assert.equal(workspaceOf("options", "cfd"), "options");
  assert.equal(workspaceOf("cfd", "options"), "cfd", "a CFD account never shows the options workspace");
  assert.equal(workspaceOf(null, "options"), "options", "guest / demo build: the switch decides");
  assert.equal(workspaceOf(null, "cfd"), "cfd");
  assert.equal(productOf({}), "cfd", "older engine: CFD");
  assert.equal(productOf(undefined), "cfd");
  assert.equal(productOf({ product: "options" }), "options");
});

test("?mode= reads cfd / options in any case, nothing else", () => {
  assert.equal(parseMode("options"), "options");
  assert.equal(parseMode(" OPTIONS "), "options");
  assert.equal(parseMode("cfd"), "cfd");
  assert.equal(parseMode("forex"), null);
  assert.equal(parseMode(null), null);
});

test("start: the SSO account first, then an account of the asked product, then the last shown, then the newest", () => {
  const list = [s("50000003", "cfd"), s("50000002", "options"), s("10000001", "cfd"), s("10000009", "options")];
  assert.equal(pickSession(list, { prefer: "10000001", want: "options" })?.login, "10000001", "the SSO account wins (its product decides)");
  assert.equal(pickSession(list, { active: "50000003", want: "options" })?.login, "50000002", "first Options account");
  assert.equal(pickSession(list, { active: "10000009", want: "options" })?.login, "10000009", "the last shown one when it trades options");
  assert.equal(pickSession(list, { active: "50000002", want: "cfd" })?.login, "50000003");
  assert.equal(pickSession(list, { active: "10000001" })?.login, "10000001", "no product asked: the last shown");
  assert.equal(pickSession(list, {})?.login, "50000003", "else the newest");
  // no account of that product here: the usual pick (the terminal then offers to open one)
  assert.equal(pickSession([s("1", "cfd"), s("2", null)], { active: "2", want: "options" })?.login, "2");
  assert.equal(pickSession([], { want: "options" }), undefined);
});

test("the CFD | Options control opens the current, the last shown, else the first account of the product", () => {
  const accts = [{ login: "1", product: "cfd" }, { login: "2", product: "options" }, { login: "3" }, { login: "4", product: "options" }];
  assert.equal(switchTarget(accts, "2", "options"), "2", "already there");
  assert.equal(switchTarget(accts, "1", "options", ["3", "4", "2"]), "4", "the last shown Options account");
  assert.equal(switchTarget(accts, "1", "options"), "2", "else the first listed");
  assert.equal(switchTarget(accts, "2", "cfd", ["3"]), "3", "no product = CFD");
  assert.equal(switchTarget([{ login: "1", product: "cfd" }], "1", "options"), null, "none: offer to open one");
  assert.equal(openAccountPath("options"), "/accounts/new?product=options");
});

test("the account switcher groups by product, CFD first, order kept", () => {
  const g = groupAccounts([{ login: "2", product: "options" }, { login: "1" }, { login: "3", product: "cfd" }]);
  assert.deepEqual(g.map((x) => [x.product, x.accounts.map((a) => a.login)]), [["cfd", ["1", "3"]], ["options", ["2"]]]);
  assert.deepEqual(groupAccounts([{ login: "1" }]).map((x) => x.product), ["cfd"]);
});

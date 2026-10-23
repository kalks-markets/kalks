// NOWPayments IPN route (/api/payments/nowpayments/ipn, lib/nowpayments-ipn.ts): `node --test apps/crm/tests`.
// Against a stub wallet on a loopback port; no real service or secret is involved.
//
// What it guards: the route is public (no session, open in maintenance mode), the body reaches the wallet byte for
// byte with the signature header and the internal token, the wallet's answer is passed back (so a bad signature is a
// 401 to NOWPayments), and the limits (JSON only, 64 KB, per-IP budget, POST only).

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import "./helpers/ts-hooks.mjs";

const calls = [];
let walletSvc;
let gateway;

before(async () => {
  walletSvc = await new Promise((resolve) => {
    const s = createServer(async (req, res) => {
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const raw = Buffer.concat(chunks);
      calls.push({ path: req.url, headers: req.headers, raw });
      const ok = req.headers["x-nowpayments-sig"] === "a".repeat(128);
      res.writeHead(ok ? 200 : 401, { "content-type": "application/json" });
      res.end(JSON.stringify(ok ? { status: "ok" } : { error: { code: "invalid_signature", message: "Invalid signature" } }));
    });
    s.listen(0, "127.0.0.1", () => resolve(s));
  });
  // a broker in maintenance: the IPN must still go through
  gateway = await new Promise((resolve) => {
    const s = createServer((req, res) => {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify({ tenant: { slug: "kalks", name: "Kalks", status: "active", brand: {} }, maintenance: { active: true, enabled: true, message: "", until: null }, modules: {}, flags: {} }));
    });
    s.listen(0, "127.0.0.1", () => resolve(s));
  });
  process.env.WALLET_URL = `http://127.0.0.1:${walletSvc.address().port}`;
  process.env.WALLET_INTERNAL_TOKEN = "test-wallet-token";
  process.env.GATEWAY_URL = `http://127.0.0.1:${gateway.address().port}`;
});

after(() => {
  walletSvc?.close();
  gateway?.close();
});

const load = async () => ({
  ...(await import("next/server")),
  proxy: (await import("../proxy.ts")).proxy,
  route: await import("../app/api/payments/nowpayments/ipn/route.ts"),
  lib: await import("../lib/nowpayments-ipn.ts"),
});

const URL_ = "https://app.kalkstrade.com/api/payments/nowpayments/ipn";
// keys out of order, unicode and spacing: the wallet must see exactly these bytes to check the signature
const BODY = '{"payment_status":"finished", "payment_id":5745459419,"order_description":"Dépôt 💰","actually_paid":1e-7}';

test("the proxy lets the IPN through without a session, even in maintenance mode", async () => {
  const m = await load();
  const res = await m.proxy(new m.NextRequest(URL_, { method: "POST", headers: { "content-type": "application/json" }, body: BODY }));
  assert.equal(res.status, 200);
  assert.ok(res.headers.get("x-middleware-next"), "passed to the route");
  // an ordinary API call of the same broker is held by maintenance mode
  const held = await m.proxy(new m.NextRequest("https://app.kalkstrade.com/api/wallet/overview"));
  assert.equal(held.status, 503);
});

test("the raw body and the signature reach the wallet unchanged; its answer goes back", async () => {
  const m = await load();
  const sig = "a".repeat(128);
  const res = await m.route.POST(new m.NextRequest(URL_, { method: "POST", headers: { "content-type": "application/json", "x-nowpayments-sig": sig, "x-forwarded-for": "203.0.113.5" }, body: BODY }));
  assert.equal(res.status, 200);
  assert.deepEqual(await res.json(), { status: "ok" });
  const c = calls.at(-1);
  assert.equal(c.path, "/v1/nowpayments/ipn");
  assert.equal(c.raw.toString("utf8"), BODY);
  assert.equal(c.headers["x-nowpayments-sig"], sig);
  assert.equal(c.headers["x-kalks-internal"], "test-wallet-token");
  assert.equal(c.headers["x-forwarded-for"], "203.0.113.5");
  assert.equal(res.headers.get("cache-control"), "no-store");
  // a bad signature is the wallet's 401
  const bad = await m.route.POST(new m.NextRequest(URL_, { method: "POST", headers: { "content-type": "application/json", "x-nowpayments-sig": "b".repeat(128) }, body: BODY }));
  assert.equal(bad.status, 401);
  assert.equal((await bad.json()).error.code, "invalid_signature");
});

test("limits: JSON only, 64 KB, POST only, per-IP budget", async () => {
  const m = await load();
  const before = calls.length;
  const form = await m.route.POST(new m.NextRequest(URL_, { method: "POST", headers: { "content-type": "text/plain", "x-forwarded-for": "198.51.100.1" }, body: "x" }));
  assert.equal(form.status, 415);
  const big = await m.route.POST(new m.NextRequest(URL_, { method: "POST", headers: { "content-type": "application/json", "x-forwarded-for": "198.51.100.1" }, body: `{"a":"${"x".repeat(70 * 1024)}"}` }));
  assert.equal(big.status, 413);
  const empty = await m.route.POST(new m.NextRequest(URL_, { method: "POST", headers: { "content-type": "application/json", "x-forwarded-for": "198.51.100.1" }, body: "" }));
  assert.equal(empty.status, 400);
  assert.equal(calls.length, before, "nothing reached the wallet");
  assert.equal(m.route.GET().status, 405);
  // the budget is per IP and per minute
  const t0 = 1_000_000;
  for (let i = 0; i < m.lib.IPN_PER_MINUTE; i++) assert.equal(m.lib.ipnAllowed("192.0.2.9", t0), true);
  assert.equal(m.lib.ipnAllowed("192.0.2.9", t0 + 1000), false);
  assert.equal(m.lib.ipnAllowed("192.0.2.10", t0 + 1000), true);
  assert.equal(m.lib.ipnAllowed("192.0.2.9", t0 + 61_000), true);
});

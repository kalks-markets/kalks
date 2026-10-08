// Kalks Circle BFF (/api/circle/*, /api/mobile/circle/*): `node --test apps/crm/tests`. The proxy and the route
// handler run as they are against a stub gateway and a stub Circle service on loopback ports.
//
// What it guards: the session (never browser-sent ids) becomes the X-Kalks-* identity of the service call; the path
// allow-list; same-origin JSON for mutations; upload chunks passed raw with Upload-Offset; the stream ticket; the
// module switch `circle`; the app's bearer rewrite onto the same handler.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import "./helpers/ts-hooks.mjs";

const TOKEN = "u".repeat(43);
const USER = { id: 42, email: "arjun@example.com", first_name: "Arjun", last_name: "Mehta", name: "Arjun Mehta", country: "IN", kyc_status: "verified", referral_code: "ARJ42", tenant: { slug: "acme", name: "Acme FX" } };
const TENANTS = {
  "app.kalkstrade.com": { modules: {} },
  "off.broker.test": { modules: { circle: false } },
};
const calls = [];
let gateway, circleSvc;

function stub(handle) {
  const server = createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const raw = Buffer.concat(chunks);
    calls.push({ method: req.method, path: req.url, headers: req.headers, raw });
    const [status, data] = handle(req, raw);
    res.writeHead(status, { "content-type": "application/json" });
    res.end(JSON.stringify(data));
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

before(async () => {
  gateway = await stub((req) => {
    const url = new URL(req.url, "http://x");
    if (url.pathname === "/v1/public/tenant-config") return [200, { maintenance: { active: false }, flags: {}, ...(TENANTS[req.headers["x-kalks-host"]] ?? { modules: {} }) }];
    if (url.pathname === "/v1/auth/me") return (req.headers.authorization ?? "") === `Bearer ${TOKEN}` ? [200, { user: USER, viewer: null }] : [401, { error: { code: "unauthorized", message: "Please sign in." } }];
    return [404, { error: { code: "not_found", message: "stub" } }];
  });
  circleSvc = await stub((req) => {
    const url = new URL(req.url, "http://x");
    if (url.pathname === "/v1/stream/ticket") return [200, { ticket: "circle-ticket-1", expiresIn: 30 }];
    if (url.pathname === "/v1/circle/uploads" && req.method === "POST") return [200, { upload: { id: 7, token: "t".repeat(48), chunkSize: 8388608, directUrl: "/circle/upload/7" } }];
    return [200, { ok: true, path: url.pathname, query: url.search }];
  });
  process.env.GATEWAY_URL = `http://127.0.0.1:${gateway.address().port}`;
  process.env.CIRCLE_URL = `http://127.0.0.1:${circleSvc.address().port}`;
  process.env.CIRCLE_INTERNAL_TOKEN = "circle-secret";
});

after(() => {
  gateway?.close();
  circleSvc?.close();
});

const load = async () => ({
  ...(await import("next/server")),
  cfg: await import("../lib/tenant-config.ts"),
  mobile: await import("../lib/mobile.ts"),
  circleLib: await import("../lib/circle.ts"),
  proxy: (await import("../proxy.ts")).proxy,
  route: await import("../app/api/circle/[...path]/route.ts"),
});

const BASE = "https://app.kalkstrade.com";
const headersOf = (res) => Object.fromEntries(res.headers.entries());
const browser = { host: "app.kalkstrade.com", "x-forwarded-proto": "https", origin: BASE, cookie: `kalks_session=${TOKEN}; kalks_locale=hi`, "cf-ipcountry": "IN" };

/** Proxy first (module switch, mobile rewrite), then the handler it lets through, like Next does. */
async function call(m, url, init = {}, base = BASE) {
  const req = new m.NextRequest(`${base}${url}`, init);
  const res = await m.proxy(req);
  const h = headersOf(res);
  if (!h["x-middleware-rewrite"] && !h["x-middleware-next"]) return { res, forwarded: false };
  const overridden = (h["x-middleware-override-headers"] ?? "").split(",").filter(Boolean);
  const fwd = new Headers(overridden.length ? undefined : init.headers);
  for (const k of overridden) fwd.set(k, h[`x-middleware-request-${k}`]);
  const target = new URL(h["x-middleware-rewrite"] ?? `${base}${url}`);
  const inner = new m.NextRequest(target, { method: init.method ?? "GET", headers: fwd, body: init.body, duplex: "half" });
  const method = init.method ?? "GET";
  const segments = target.pathname.replace(/^\/api\/circle\//, "").split("/").filter(Boolean).map(decodeURIComponent);
  return { res: await m.route[method](inner, { params: Promise.resolve({ path: segments }) }), forwarded: true };
}

const last = () => calls[calls.length - 1];

test("Circle paths follow module `circle`; the app reaches the BFF through a rewrite", async () => {
  const { cfg, mobile, circleLib } = await load();
  assert.equal(cfg.moduleFor("/circle"), "circle");
  assert.equal(cfg.moduleFor("/circle/@ana"), "circle");
  assert.equal(cfg.moduleFor("/api/circle/feed/for-you"), "circle");
  assert.equal(cfg.moduleFor("/api/circle/stream-ticket"), "circle");
  assert.deepEqual(mobile.mobileRoute("/api/mobile/circle/posts/12/react"), { kind: "rewrite", target: "/api/circle/posts/12/react", policyPath: "/api/circle/posts/12/react" });
  assert.equal(circleLib.circlePath(["posts", "12", "react"]), "/v1/circle/posts/12/react");
  assert.equal(circleLib.circlePath(["profiles", "@ana.fx"]), "/v1/circle/profiles/%40ana.fx");
  assert.equal(circleLib.circlePath(["cashtags", "$XAUUSD"]), "/v1/circle/cashtags/%24XAUUSD");
  for (const bad of [[], ["admin", "queue"], ["posts", ".."], ["posts", "a/b"], ["internal"], ["me", "x".repeat(65)], Array(9).fill("me")]) {
    assert.equal(circleLib.circlePath(bad), null, JSON.stringify(bad));
  }
});

test("reads forward the session's identity, never the browser's", async () => {
  const m = await load();
  const { res } = await call(m, "/api/circle/feed/following?cursor=abc&limit=5", { headers: { ...browser, "x-kalks-user-id": "999", "x-kalks-tenant": "evil" } });
  assert.equal(res.status, 200);
  const c = last();
  assert.equal(c.path, "/v1/circle/feed/following?cursor=abc&limit=5");
  assert.equal(c.headers["x-kalks-internal"], "circle-secret");
  assert.equal(c.headers["x-kalks-user-id"], "42");
  assert.equal(c.headers["x-kalks-tenant"], "acme");
  assert.equal(c.headers["x-kalks-user-name"], "Arjun%20Mehta");
  assert.equal(c.headers["x-kalks-country"], "IN");
  assert.equal(c.headers["x-kalks-kyc"], "verified");
  assert.equal(c.headers["x-kalks-referral-code"], "ARJ42");
  assert.equal(c.headers["x-kalks-locale"], "hi");
  assert.equal(c.headers["x-kalks-ip-country"], "in");
  assert.equal(res.headers.get("cache-control"), "no-store");
  // no session
  const { res: anon } = await call(m, "/api/circle/me", { headers: { host: "app.kalkstrade.com" } });
  assert.equal(anon.status, 401);
  // unknown families are refused before the service is called
  const n = calls.length;
  const { res: nf } = await call(m, "/api/circle/admin/queue", { headers: browser });
  assert.equal(nf.status, 404);
  assert.equal(calls.length, n);
});

test("mutations: same-origin JSON; upload chunks raw with Upload-Offset; the stream ticket", async () => {
  const m = await load();
  const json = { ...browser, "content-type": "application/json" };
  const { res } = await call(m, "/api/circle/posts", { method: "POST", headers: json, body: JSON.stringify({ body: "Gold $XAUUSD" }) });
  assert.equal(res.status, 200);
  assert.equal(last().path, "/v1/circle/posts");
  assert.deepEqual(JSON.parse(last().raw.toString()), { body: "Gold $XAUUSD" });
  const { res: cross } = await call(m, "/api/circle/posts", { method: "POST", headers: { ...json, origin: "https://evil.example" }, body: "{}" });
  assert.equal(cross.status, 403);
  const { res: notJson } = await call(m, "/api/circle/posts", { method: "POST", headers: { ...browser, "content-type": "text/plain" }, body: "hi" });
  assert.equal(notJson.status, 415);
  // DELETE with and without a body
  const { res: del } = await call(m, "/api/circle/posts/5/react?kind=like", { method: "DELETE", headers: browser });
  assert.equal(del.status, 200);
  assert.equal(last().method, "DELETE");
  assert.equal(last().path, "/v1/circle/posts/5/react?kind=like");
  // a new upload gets this host's direct upload URL (the service in development)
  const { res: up } = await call(m, "/api/circle/uploads", { method: "POST", headers: json, body: JSON.stringify({ kind: "photo", purpose: "post", mime: "image/png", size: 10 }) });
  const u = await up.json();
  assert.match(u.upload.directUrl, /\/v1\/upload\/7$/);
  // chunk: raw bytes and the offset reach the service
  const { res: chunk } = await call(m, "/api/circle/uploads/7", { method: "PUT", headers: { ...browser, "upload-offset": "0", "content-type": "application/octet-stream" }, body: new Uint8Array([1, 2, 3, 4]) });
  assert.equal(chunk.status, 200);
  assert.equal(last().path, "/v1/circle/uploads/7");
  assert.equal(last().headers["upload-offset"], "0");
  assert.deepEqual([...last().raw], [1, 2, 3, 4]);
  const { res: noOffset } = await call(m, "/api/circle/uploads/7", { method: "PUT", headers: browser, body: new Uint8Array([1]) });
  assert.equal(noOffset.status, 422);
  // stream ticket: the service's ticket and the stream URL
  const { res: t } = await call(m, "/api/circle/stream-ticket", { method: "POST", headers: json, body: "{}" });
  const tv = await t.json();
  assert.equal(tv.ticket, "circle-ticket-1");
  assert.match(tv.url, /^ws:\/\/127\.0\.0\.1:\d+\/v1\/stream$/);
  assert.equal(last().path, "/v1/stream/ticket");
});

test("module `circle` off: every Circle path answers 403 module_disabled", async () => {
  const m = await load();
  for (const p of ["/api/circle/me", "/api/circle/stream-ticket", "/circle"]) {
    const res = await m.proxy(new m.NextRequest(`https://off.broker.test${p}`, { headers: { host: "off.broker.test", cookie: `kalks_session=${TOKEN}` } }));
    if (p.startsWith("/api/")) {
      assert.equal(res.status, 403, p);
      assert.equal((await res.json()).error.code, "module_disabled", p);
    } else {
      assert.notEqual(res.headers.get("x-middleware-next"), "1", "the page is not served");
    }
  }
});

test("the app: /api/mobile/circle/* with the bearer token runs the same handler", async () => {
  const m = await load();
  const { res, forwarded } = await call(m, "/api/mobile/circle/me", { headers: { host: "app.kalkstrade.com", "x-forwarded-proto": "https", authorization: `Bearer ${TOKEN}`, "x-kalks-locale": "ar" } });
  assert.equal(forwarded, true);
  assert.equal(res.status, 200);
  assert.equal(last().path, "/v1/circle/me");
  assert.equal(last().headers["x-kalks-user-id"], "42");
  assert.equal(last().headers["x-kalks-locale"], "ar");
});

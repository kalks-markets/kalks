// Mobile app API (/api/mobile/*, docs/MOBILE-API.md): `node --test apps/crm/tests`.
// The proxy and the route handlers run as they are, against stub gateway / wallet / support servers on loopback
// ports; no real service or secret is involved.
//
// What it guards: the bearer rewrite onto the cookie routes (every cookie-route policy reused), browser cookies never
// authenticating /api/mobile/*, a bearer token never accepted together with cookies, the same-origin bypass only for
// bearer requests without cookies, the auth routes returning the session in JSON, uploads passing the rewrite, and
// the shape of the public config.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import "./helpers/ts-hooks.mjs";

/* ------------------------------------------------------------------ */
/* Stub upstreams                                                      */
/* ------------------------------------------------------------------ */

const TOKENS = {
  user: "u".repeat(43),
  viewer: `v.${"w".repeat(43)}`,
  staffRead: `i.${"r".repeat(43)}`,
  staffFull: `s.${"f".repeat(43)}`,
  blocked: "b".repeat(43),
};
const USER = { id: 42, email: "arjun@example.com", first_name: "Arjun", last_name: "Mehta", name: "Arjun Mehta", kyc_status: "verified", tenant: { slug: "kalks", name: "Kalks" } };
const VIEWER = { id: 7, label: "Accountant", username: "acc", accounts: ["50000001"], sections: ["accounts", "wallet"], expires_at: null, status: "active", last_login_at: null, created_at: "2026-09-01T00:00:00Z" };
const SESSION = { token: TOKENS.user, expires_at: "2026-10-15T00:00:00Z" };

const TENANTS = {
  "app.kalkstrade.com": { tenant: { slug: "kalks", name: "Kalks", status: "active", brand: {} }, maintenance: { active: false, enabled: false, message: "", until: null }, modules: { wallet: true, prop: true }, flags: { demo_accounts: true }, branding: { slug: "kalks", name: "Kalks", default: true, urls: { trade: "https://trade.kalkstrade.com" } } },
  "app.broker.test": { tenant: { slug: "acme", name: "Acme FX", status: "active", brand: {} }, maintenance: { active: false, enabled: false, message: "", until: null }, modules: { prop: false }, flags: {}, branding: { slug: "acme", name: "Acme FX", default: false, logo_url: "https://cdn.acme.test/logo.png", primary: "#12AB34", accent: "red", support_email: "help@acme.test", urls: { trade: "https://trade.broker.test", website: "https://acme.test" } } },
  "down.broker.test": { tenant: { slug: "down", name: "Down", status: "active", brand: {} }, maintenance: { active: true, enabled: true, message: "Back at 10:00", until: "2026-10-08T10:00:00Z" }, modules: {}, flags: {} },
};

const calls = []; // every upstream request: {svc, method, path, headers, body, raw}
let gateway, walletSvc, supportSvc;

function stub(name, handle) {
  const server = createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const raw = Buffer.concat(chunks);
    let body;
    try {
      body = raw.length && (req.headers["content-type"] ?? "").includes("json") ? JSON.parse(raw.toString("utf8")) : undefined;
    } catch {
      body = undefined;
    }
    calls.push({ svc: name, method: req.method, path: req.url, headers: req.headers, body, raw });
    const [status, data] = await handle(req, body);
    res.writeHead(status, { "content-type": "application/json" });
    res.end(JSON.stringify(data));
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

const bearer = (req) => (req.headers.authorization ?? "").replace(/^Bearer /, "");

before(async () => {
  gateway = await stub("gateway", (req, body) => {
    const url = new URL(req.url, "http://x");
    const p = url.pathname;
    if (p === "/v1/public/tenant-config") return [200, TENANTS[req.headers["x-kalks-host"]] ?? TENANTS["app.kalkstrade.com"]];
    if (p === "/v1/auth/login") {
      if (body?.email === "blocked@example.com") return [403, { error: { code: "account_suspended", message: "This account is suspended." } }];
      if (body?.email === "newdevice@example.com") return [200, { status: "otp_required", challenge: "c-1", purpose: "login", email_masked: "n•••@example.com", expires_in: 600, resend_in: 30 }];
      return [200, { status: "ok", user: USER, session: SESSION }];
    }
    if (p === "/v1/auth/register") return [200, { status: "otp_required", challenge: "c-reg", purpose: "register" }];
    if (p === "/v1/auth/verify-email") return [200, { status: "ok", user: USER, session: SESSION }];
    if (p === "/v1/auth/logout") return [200, { status: "ok" }];
    if (p === "/v1/auth/heartbeat") return [200, { status: "ok", restrictions: [] }];
    if (p === "/v1/auth/stepup") return [200, { challenge: "st-c", expires_in: 600 }];
    if (p === "/v1/auth/stepup/verify") return [200, { stepup_token: "st-1" }];
    if (p === "/v1/auth/impersonation/event") return [200, { status: "ok" }];
    if (p === "/v1/kyc/documents") return [200, { document: { id: 5, kind: url.searchParams.get("kind") } }];
    if (p === "/v1/auth/me") {
      const t = bearer(req);
      if (t === TOKENS.user || t === TOKENS.staffRead || t === TOKENS.staffFull) return [200, { user: USER, viewer: null, session: { id: 1, idle_minutes: 60, expires_at: SESSION.expires_at } }];
      if (t === TOKENS.viewer) return [200, { user: { ...USER }, viewer: VIEWER }];
      return [401, { error: { code: "unauthorized", message: "Please sign in." } }];
    }
    return [404, { error: { code: "not_found", message: "stub" } }];
  });
  walletSvc = await stub("wallet", (req) => {
    const url = new URL(req.url, "http://x");
    if (url.pathname === "/v1/withdrawals/quote") return [200, { quote: { amount: "10", fee: "1", net_amount: "9" } }];
    if (url.pathname === "/v1/withdrawals") return [200, { withdrawal: { id: 1, status: "pending" } }];
    if (url.pathname.endsWith("/overview")) return [200, { balances: [{ currency: "USDT", available: "25" }] }];
    return [404, { error: { code: "not_found", message: "stub" } }];
  });
  supportSvc = await stub("support", (req) => {
    const url = new URL(req.url, "http://x");
    if (url.pathname === "/v1/stream/ticket") return [200, { ticket: "sup-ticket-1" }];
    if (url.pathname === "/v1/support/me/attachments") return [200, { attachment: { id: 9, name: decodeURIComponent(req.headers["x-file-name"] ?? "") } }];
    return [404, { error: { code: "not_found", message: "stub" } }];
  });
  process.env.GATEWAY_URL = `http://127.0.0.1:${gateway.address().port}`;
  process.env.WALLET_URL = `http://127.0.0.1:${walletSvc.address().port}`;
  process.env.SUPPORT_URL = `http://127.0.0.1:${supportSvc.address().port}`;
  process.env.NEXT_PUBLIC_TERMINAL_URL = "https://trade.kalkstrade.com";
  process.env.NEXT_PUBLIC_MARKET_DATA_URL = "https://api.kalkstrade.com";
});

after(() => {
  gateway?.close();
  walletSvc?.close();
  supportSvc?.close();
});

// modules read their upstream URLs at import time: import them after the stubs are listening
const load = async () => ({
  ...(await import("next/server")),
  mobile: await import("../lib/mobile.ts"),
  proxy: (await import("../proxy.ts")).proxy,
  auth: await import("../app/api/mobile/auth/[action]/route.ts"),
  config: await import("../app/api/mobile/config/route.ts"),
  wallet: await import("../app/api/wallet/[...path]/route.ts"),
  kyc: await import("../app/api/kyc/[[...path]]/route.ts"),
  support: await import("../app/api/support/[...path]/route.ts"),
  heartbeat: await import("../app/api/auth/heartbeat/route.ts"),
});

const BASE = "https://app.kalkstrade.com";
const headersOf = (res) => Object.fromEntries(res.headers.entries());
const json = { "content-type": "application/json" };
const auth = (t) => ({ authorization: `Bearer ${t}` });

/** Runs a request through the proxy, then (like Next) through the handler it rewrites / passes to. */
async function viaProxy(m, url, init, handler, params, base = BASE) {
  const req = new m.NextRequest(`${base}${url}`, init);
  const res = await m.proxy(req);
  const h = headersOf(res);
  if (!h["x-middleware-rewrite"] && !h["x-middleware-next"]) return { res, forwarded: null, proxied: res };
  const overridden = (h["x-middleware-override-headers"] ?? "").split(",").filter(Boolean);
  const fwd = new Headers();
  for (const k of overridden) fwd.set(k, h[`x-middleware-request-${k}`]);
  const target = h["x-middleware-rewrite"] ?? `${base}${url}`;
  const inner = new m.NextRequest(target, { method: init?.method ?? "GET", headers: fwd, body: init?.body, duplex: "half" });
  return { res: await handler(inner, { params: Promise.resolve(params) }), forwarded: fwd, target, proxied: res };
}

/* ------------------------------------------------------------------ */
/* Routing and header rules                                            */
/* ------------------------------------------------------------------ */

test("mobile paths resolve to rewrites of the cookie routes or to native routes; anything else is refused", async () => {
  const { mobile } = await load();
  for (const family of mobile.REWRITE_FAMILIES) {
    assert.deepEqual(mobile.mobileRoute(`/api/mobile/${family}/x/1`), { kind: "rewrite", target: `/api/${family}/x/1`, policyPath: `/api/${family}/x/1` }, family);
  }
  assert.equal(mobile.REWRITE_FAMILIES.length, 16);
  assert.deepEqual(mobile.mobileRoute("/api/mobile/notifications"), { kind: "rewrite", target: "/api/notifications", policyPath: "/api/notifications" });
  assert.deepEqual(mobile.mobileRoute("/api/mobile/suitability/options/quiz"), { kind: "rewrite", target: "/api/suitability/options/quiz", policyPath: "/api/suitability/options/quiz" });
  assert.deepEqual(mobile.mobileRoute("/api/mobile/auth/heartbeat"), { kind: "rewrite", target: "/api/auth/heartbeat", policyPath: "/api/auth/heartbeat" });
  for (const action of ["login", "verify-email", "resend", "register", "forgot", "reset", "logout", "me", "stepup", "stepup-verify", "stepup-resend", "password"]) {
    assert.deepEqual(mobile.mobileRoute(`/api/mobile/auth/${action}`), { kind: "native", target: `/api/mobile/auth/${action}`, policyPath: `/api/auth/${action}` }, action);
  }
  assert.deepEqual(mobile.mobileRoute("/api/mobile/config"), { kind: "native", target: "/api/mobile/config", policyPath: "/api/status" });
  assert.deepEqual(mobile.mobileRoute("/api/mobile/trade/state"), { kind: "native", target: "/api/mobile/trade/state", policyPath: "/api/mobile/trade/state" });
  assert.equal(mobile.mobileRoute("/api/mobile/trade/symbols").policyPath, "/api/status", "contract specs are public");
  for (const bad of ["/api/mobile/", "/api/mobile/admin/x", "/api/mobile/../auth/login", "/api/mobile//wallet", "/api/wallet/overview", "/api/mobile/config/x", "/api/mobile/auth/google/start", "/api/mobile/auth/expired", "/api/mobile/auth", "/api/mobile/trade", "/api/mobile/wallet/%2e%2e/admin", "/api/mobile/unsubscribe"]) {
    assert.equal(mobile.mobileRoute(bad), null, bad);
  }
});

test("bearer tokens are parsed strictly", async () => {
  const { mobile } = await load();
  const h = (v) => new Headers(v ? { authorization: v } : {});
  assert.equal(mobile.bearerOf(h(`Bearer ${TOKENS.user}`)), TOKENS.user);
  assert.equal(mobile.bearerOf(h(`bearer ${TOKENS.viewer}`)), TOKENS.viewer);
  assert.equal(mobile.bearerOf(h(`Bearer ${TOKENS.staffRead}`)), TOKENS.staffRead);
  for (const bad of [undefined, "Basic abc", "Bearer short", `Bearer ${TOKENS.user};x`, `Bearer ${TOKENS.user} extra`, "Bearer a=b; kalks_session=x"]) assert.equal(mobile.bearerOf(h(bad)), null, String(bad));
});

test("rewritten requests: the bearer becomes the session cookie, browser cookies are dropped, same-origin only for bearer without cookies", async () => {
  const { mobile } = await load();
  const route = mobile.mobileRoute("/api/mobile/wallet/withdrawals");
  const common = { host: "app.kalkstrade.com", "x-forwarded-proto": "https", origin: "https://evil.example", "x-kalks-locale": "ar", "x-kalks-device": "D".repeat(32), "x-kalks-mobile": "spoofed", "x-kalks-platform": "android", "x-kalks-app-version": "1.0.0+1" };
  // bearer, no cookies: authenticated and same-origin
  const ok = mobile.mobileRequestHeaders(new Headers({ ...common, authorization: `Bearer ${TOKENS.user}` }), TOKENS.user, route);
  assert.equal(ok.get("cookie"), `kalks_locale=ar; kalks_did=${"D".repeat(32)}; kalks_session=${TOKENS.user}`);
  assert.equal(ok.get("origin"), "https://app.kalkstrade.com");
  assert.equal(ok.get("sec-fetch-site"), "same-origin");
  assert.equal(ok.get("authorization"), null);
  assert.equal(ok.get("x-kalks-mobile"), "1");
  assert.equal(ok.get("x-kalks-platform"), "android");
  assert.equal(ok.get("x-kalks-app-version"), "1.0.0+1");
  // bearer WITH cookies: never authenticated, never same-origin (the proxy refuses it before this anyway)
  const mixed = mobile.mobileRequestHeaders(new Headers({ ...common, cookie: "kalks_session=victim", authorization: `Bearer ${TOKENS.user}` }), TOKENS.user, route);
  assert.ok(!(mixed.get("cookie") ?? "").includes("kalks_session"));
  assert.equal(mixed.get("origin"), "https://evil.example");
  assert.equal(mixed.get("sec-fetch-site"), null);
  // no bearer: the browser's cookie is dropped and the (cross-site) origin is left as it came
  const anon = mobile.mobileRequestHeaders(new Headers({ ...common, cookie: `kalks_session=${TOKENS.user}` }), null, route);
  assert.equal(anon.get("cookie"), `kalks_locale=ar; kalks_did=${"D".repeat(32)}`);
  assert.equal(anon.get("origin"), "https://evil.example");
  assert.equal(anon.get("sec-fetch-site"), null);
  // native routes keep the Authorization header and get no session cookie
  const native = mobile.mobileRequestHeaders(new Headers({ ...common, authorization: `Bearer ${TOKENS.user}` }), TOKENS.user, mobile.mobileRoute("/api/mobile/trade/state"));
  assert.equal(native.get("authorization"), `Bearer ${TOKENS.user}`);
  assert.equal(native.get("cookie"), "kalks_locale=ar");
});

test("a bearer token together with cookies is refused before any handler or upstream", async () => {
  const m = await load();
  const before = calls.length;
  for (const path of ["/api/mobile/wallet/overview", "/api/mobile/auth/me", "/api/mobile/trade/state"]) {
    const res = await m.proxy(new m.NextRequest(`${BASE}${path}`, { headers: { ...auth(TOKENS.user), cookie: "kalks_locale=en" } }));
    assert.equal(res.status, 400, path);
    assert.equal((await res.json()).error.code, "bearer_with_cookies");
  }
  assert.equal(calls.length, before, "nothing reached a service");
  // a malformed Authorization header is not silently ignored
  const bad = await m.proxy(new m.NextRequest(`${BASE}/api/mobile/wallet/overview`, { headers: { authorization: "Bearer nope" } }));
  assert.equal(bad.status, 401);
});

/* ------------------------------------------------------------------ */
/* Bearer access to the cookie BFFs                                    */
/* ------------------------------------------------------------------ */

test("bearer requests are rewritten onto the cookie routes and pass their same-origin check; identity comes from the gateway", async () => {
  const m = await load();
  const { res, target } = await viaProxy(m, "/api/mobile/wallet/withdrawals/quote", { method: "POST", headers: { ...json, ...auth(TOKENS.user) }, body: JSON.stringify({ amount: "10", chain: "tron", to_address: "TU7PHUS22Hw632YsnAyjxNh4gu3u8PzcHZ", user_id: 999 }) }, m.wallet.POST, { path: ["withdrawals", "quote"] });
  assert.equal(target, `${BASE}/api/wallet/withdrawals/quote`);
  assert.equal(res.status, 200);
  assert.equal(calls.findLast((c) => c.path === "/v1/withdrawals/quote").body.user_id, 42);
  const read = await viaProxy(m, "/api/mobile/wallet/overview?x=1", { headers: auth(TOKENS.user) }, m.wallet.GET, { path: ["overview"] });
  assert.equal(read.target, `${BASE}/api/wallet/overview?x=1`, "the query string is kept");
  assert.equal(read.res.status, 200);
  assert.ok(calls.findLast((c) => c.svc === "wallet").path.startsWith("/v1/wallets/42/"));
});

test("browser cookies never authenticate a mobile request, and a cookie-borne cross-site write is refused", async () => {
  const m = await load();
  const { res, forwarded } = await viaProxy(m, "/api/mobile/wallet/overview", { headers: { cookie: `kalks_session=${TOKENS.user}` } }, m.wallet.GET, { path: ["overview"] });
  assert.equal(res.status, 401);
  assert.ok(!(forwarded.get("cookie") ?? "").includes("kalks_session"));
  const before = calls.filter((c) => c.path === "/v1/withdrawals").length;
  const post = await viaProxy(m, "/api/mobile/wallet/withdrawals", { method: "POST", headers: { ...json, cookie: `kalks_session=${TOKENS.user}`, origin: "https://evil.example" }, body: JSON.stringify({ amount: "10", chain: "tron", to_address: "T1", idempotency_key: "k-12345678" }) }, m.wallet.POST, { path: ["withdrawals"] });
  assert.equal(post.res.status, 403, "no bearer: the cookie route's same-origin check still refuses");
  assert.equal(calls.filter((c) => c.path === "/v1/withdrawals").length, before);
});

test("cookie routes keep their same-origin check (unchanged by the mobile path)", async () => {
  const m = await load();
  const req = new m.NextRequest(`${BASE}/api/wallet/withdrawals/quote`, { method: "POST", headers: { host: "app.kalkstrade.com", ...json, cookie: `kalks_session=${TOKENS.user}`, origin: "https://evil.example" }, body: JSON.stringify({ amount: "10", chain: "tron", to_address: "T1" }) });
  const res = await m.wallet.POST(req, { params: Promise.resolve({ path: ["withdrawals", "quote"] }) });
  assert.equal(res.status, 403);
  assert.equal((await res.json()).error.code, "forbidden");
  // and a cookie request to a cookie route is not touched by the mobile branch
  const p = await m.proxy(new m.NextRequest(`${BASE}/api/wallet/overview`, { headers: { cookie: `kalks_session=${TOKENS.user}` } }));
  assert.equal(p.headers.get("x-middleware-next"), "1");
  assert.equal(p.headers.get("x-middleware-request-x-kalks-mobile"), null);
});

test("uploads pass the rewrite: KYC multipart documents and support attachments (raw body + X-File-Name)", async () => {
  const m = await load();
  const form = new FormData();
  form.set("file", new File([Buffer.from("%PDF-1.4 fake")], "passport.pdf", { type: "application/pdf" }));
  form.set("kind", "passport");
  form.set("side", "front");
  const kyc = await viaProxy(m, "/api/mobile/kyc/documents", { method: "POST", headers: auth(TOKENS.user), body: form }, m.kyc.POST, { path: ["documents"] });
  assert.equal(kyc.target, `${BASE}/api/kyc/documents`);
  assert.equal(kyc.res.status, 200, JSON.stringify(await kyc.res.clone().json()));
  const sent = calls.findLast((c) => c.path.startsWith("/v1/kyc/documents"));
  assert.match(sent.path, /kind=passport/);
  assert.equal(sent.headers.authorization, `Bearer ${TOKENS.user}`);
  assert.equal(sent.raw.toString(), "%PDF-1.4 fake");

  const bytes = Buffer.from([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);
  const att = await viaProxy(m, "/api/mobile/support/attachments", { method: "POST", headers: { ...auth(TOKENS.user), "content-type": "image/png", "x-file-name": encodeURIComponent("screen shot.png") }, body: bytes }, m.support.POST, { path: ["attachments"] });
  assert.equal(att.res.status, 200);
  assert.equal((await att.res.json()).attachment.name, "screen shot.png");
  const up = calls.findLast((c) => c.path === "/v1/support/me/attachments");
  assert.deepEqual([...up.raw], [...bytes]);
  assert.equal(up.headers["x-kalks-user-id"], "42");
});

test("the support / notifications stream ticket works through the rewrite; config names the stream", async () => {
  const m = await load();
  const { res, target } = await viaProxy(m, "/api/mobile/support/stream-ticket", { method: "POST", headers: { ...json, ...auth(TOKENS.user) }, body: "{}" }, m.support.POST, { path: ["stream-ticket"] });
  assert.equal(target, `${BASE}/api/support/stream-ticket`);
  assert.equal(res.status, 200);
  assert.equal((await res.json()).ticket, "sup-ticket-1");
  // a read-only staff session may open the receive-only stream, as on the web
  const staff = await viaProxy(m, "/api/mobile/support/stream-ticket", { method: "POST", headers: { ...json, ...auth(TOKENS.staffRead) }, body: "{}" }, m.support.POST, { path: ["stream-ticket"] });
  assert.equal(staff.res.status, 200);
});

test("the heartbeat cookie route works with the bearer (presence + restrictions)", async () => {
  const m = await load();
  const { res, target } = await viaProxy(m, "/api/mobile/auth/heartbeat", { method: "POST", headers: { ...json, ...auth(TOKENS.user) }, body: "{}" }, m.heartbeat.POST, {});
  assert.equal(target, `${BASE}/api/auth/heartbeat`);
  assert.equal(res.status, 200);
  assert.equal(calls.findLast((c) => c.path === "/v1/auth/heartbeat").headers.authorization, `Bearer ${TOKENS.user}`);
});

/* ------------------------------------------------------------------ */
/* Policies reused unchanged                                           */
/* ------------------------------------------------------------------ */

test("view-only logins stay read-only and in scope", async () => {
  const m = await load();
  const write = await viaProxy(m, "/api/mobile/wallet/withdrawals/quote", { method: "POST", headers: { ...json, ...auth(TOKENS.viewer) }, body: "{}" }, m.wallet.POST, { path: ["withdrawals", "quote"] });
  assert.equal(write.res.status, 403);
  assert.equal((await write.res.json()).error.code, "viewer_read_only");
  const outOfScope = await viaProxy(m, "/api/mobile/notifications", { headers: auth(TOKENS.viewer) }, () => new Response("unreachable"), {});
  assert.equal(outOfScope.res.status, 403);
  assert.equal((await outOfScope.res.json()).error.code, "viewer_scope");
  const read = await viaProxy(m, "/api/mobile/wallet/overview", { headers: auth(TOKENS.viewer) }, m.wallet.GET, { path: ["overview"] });
  assert.equal(read.res.status, 200);
  const stepup = await viaProxy(m, "/api/mobile/auth/stepup", { method: "POST", headers: { ...json, ...auth(TOKENS.viewer) }, body: "{}" }, m.auth.POST, { action: "stepup" });
  assert.equal(stepup.res.status, 403);
  const me = await viaProxy(m, "/api/mobile/auth/me", { headers: auth(TOKENS.viewer) }, m.auth.GET, { action: "me" });
  assert.equal(me.res.status, 200);
  assert.equal((await me.res.json()).viewer.username, "acc");
  const out = await viaProxy(m, "/api/mobile/auth/logout", { method: "POST", headers: auth(TOKENS.viewer) }, m.auth.POST, { action: "logout" });
  assert.equal(out.res.status, 200, "viewers can sign out");
});

test("read-only staff sessions can't change anything and are audited under the cookie-route path", async () => {
  const m = await load();
  const n = calls.filter((c) => c.path === "/v1/auth/impersonation/event").length;
  const { res } = await viaProxy(m, "/api/mobile/wallet/withdrawals", { method: "POST", headers: { ...json, ...auth(TOKENS.staffRead) }, body: "{}" }, m.wallet.POST, { path: ["withdrawals"] });
  assert.equal(res.status, 403);
  assert.equal((await res.json()).error.code, "staff_read_only");
  const ev = calls.filter((c) => c.path === "/v1/auth/impersonation/event");
  assert.equal(ev.length, n + 1);
  assert.equal(ev.at(-1).body.kind, "write_refused");
  assert.equal(ev.at(-1).body.path, "/api/wallet/withdrawals");
  const read = await viaProxy(m, "/api/mobile/wallet/overview", { headers: auth(TOKENS.staffRead) }, m.wallet.GET, { path: ["overview"] });
  assert.equal(read.res.status, 200);
  await viaProxy(m, "/api/mobile/wallet/withdrawals/quote", { method: "POST", headers: { ...json, ...auth(TOKENS.staffFull) }, body: JSON.stringify({ amount: "10", chain: "tron", to_address: "T1" }) }, m.wallet.POST, { path: ["withdrawals", "quote"] });
  assert.equal(calls.filter((c) => c.path === "/v1/auth/impersonation/event").at(-1).body.kind, "action");
});

test("module switches and maintenance apply to the mobile paths; the config stays open", async () => {
  const m = await load();
  const off = await m.proxy(new m.NextRequest("https://app.broker.test/api/mobile/prop/plans", { headers: { host: "app.broker.test", ...auth(TOKENS.user) } }));
  assert.equal(off.status, 403);
  assert.equal((await off.json()).error.code, "module_disabled");
  const down = await m.proxy(new m.NextRequest("https://down.broker.test/api/mobile/auth/login", { method: "POST", headers: { host: "down.broker.test", ...json }, body: "{}" }));
  assert.equal(down.status, 503);
  assert.equal((await down.json()).error.code, "maintenance");
  const cfg = await m.proxy(new m.NextRequest("https://down.broker.test/api/mobile/config", { headers: { host: "down.broker.test" } }));
  assert.equal(cfg.headers.get("x-middleware-next"), "1");
  const logout = await m.proxy(new m.NextRequest("https://down.broker.test/api/mobile/auth/logout", { method: "POST", headers: { host: "down.broker.test", ...auth(TOKENS.user) } }));
  assert.equal(logout.headers.get("x-middleware-next"), "1", "signing out works during maintenance");
});

test("unknown mobile paths answer 404 without reaching a handler", async () => {
  const m = await load();
  const res = await m.proxy(new m.NextRequest(`${BASE}/api/mobile/admin/users`, { headers: auth(TOKENS.user) }));
  assert.equal(res.status, 404);
});

/* ------------------------------------------------------------------ */
/* Auth: the session in JSON                                           */
/* ------------------------------------------------------------------ */

test("sign-in returns the session in the JSON body (no cookie) and mints a device id", async () => {
  const m = await load();
  const { res } = await viaProxy(m, "/api/mobile/auth/login", { method: "POST", headers: { ...json, cookie: "kalks_session=browser", "user-agent": "KalksApp/1.0.0 (Android 15)" }, body: JSON.stringify({ email: "arjun@example.com", password: "x" }) }, m.auth.POST, { action: "login" });
  assert.equal(res.status, 200);
  const data = await res.json();
  assert.deepEqual(data.session, SESSION);
  assert.equal(data.status, "ok");
  assert.equal(data.user.id, 42);
  assert.match(data.device, /^[A-Za-z0-9_-]{32}$/);
  assert.equal(res.headers.get("set-cookie"), null);
  const sent = calls.findLast((c) => c.path === "/v1/auth/login");
  assert.equal(sent.headers["x-kalks-device"], data.device);
  assert.equal(sent.headers.authorization, undefined, "sign-in never forwards a session");
  assert.equal(sent.headers["user-agent"], "KalksApp/1.0.0 (Android 15)");
});

test("a new device gets the email-code challenge; the code returns the session; a known device id is kept", async () => {
  const m = await load();
  const device = "D".repeat(32);
  const { res } = await viaProxy(m, "/api/mobile/auth/login", { method: "POST", headers: { ...json, "x-kalks-device": device }, body: JSON.stringify({ email: "newdevice@example.com", password: "x" }) }, m.auth.POST, { action: "login" });
  const data = await res.json();
  assert.equal(data.status, "otp_required");
  assert.equal(data.challenge, "c-1");
  assert.equal(data.session, undefined);
  assert.equal(data.device, undefined);
  assert.equal(calls.findLast((c) => c.path === "/v1/auth/login").headers["x-kalks-device"], device);
  const v = await viaProxy(m, "/api/mobile/auth/verify-email", { method: "POST", headers: { ...json, "x-kalks-device": device }, body: JSON.stringify({ challenge: "c-1", code: "123456" }) }, m.auth.POST, { action: "verify-email" });
  assert.equal(v.res.status, 200);
  assert.equal((await v.res.json()).session.token, TOKENS.user);
  assert.equal(calls.findLast((c) => c.path === "/v1/auth/verify-email").headers["x-kalks-device"], device);
});

test("blocked users are refused at sign-in and their sessions are dead", async () => {
  const m = await load();
  const { res } = await viaProxy(m, "/api/mobile/auth/login", { method: "POST", headers: json, body: JSON.stringify({ email: "blocked@example.com", password: "x" }) }, m.auth.POST, { action: "login" });
  assert.equal(res.status, 403);
  const data = await res.json();
  assert.equal(data.error.code, "account_suspended");
  assert.equal(data.session, undefined);
  const me = await viaProxy(m, "/api/mobile/auth/me", { headers: auth(TOKENS.blocked) }, m.auth.GET, { action: "me" });
  assert.equal(me.res.status, 401);
  const w = await viaProxy(m, "/api/mobile/wallet/overview", { headers: auth(TOKENS.blocked) }, m.wallet.GET, { path: ["overview"] });
  assert.equal(w.res.status, 401);
});

test("session actions need the bearer token; me answers for it; step-up returns its token in JSON", async () => {
  const m = await load();
  const none = await viaProxy(m, "/api/mobile/auth/logout", { method: "POST", headers: json, body: "{}" }, m.auth.POST, { action: "logout" });
  assert.equal(none.res.status, 401);
  const st = await viaProxy(m, "/api/mobile/auth/stepup-verify", { method: "POST", headers: { ...json, ...auth(TOKENS.user) }, body: JSON.stringify({ challenge: "st-c", code: "111111", action: "withdrawal", target: "" }) }, m.auth.POST, { action: "stepup-verify" });
  assert.equal(st.res.status, 200);
  assert.equal((await st.res.json()).stepup_token, "st-1");
  assert.equal(calls.findLast((c) => c.path === "/v1/auth/stepup/verify").headers.authorization, `Bearer ${TOKENS.user}`);
  const me = await viaProxy(m, "/api/mobile/auth/me", { headers: auth(TOKENS.user) }, m.auth.GET, { action: "me" });
  assert.equal(me.res.status, 200);
  assert.equal((await me.res.json()).user.id, 42);
  const out = await viaProxy(m, "/api/mobile/auth/logout", { method: "POST", headers: auth(TOKENS.user) }, m.auth.POST, { action: "logout" });
  assert.equal(out.res.status, 200);
  assert.equal((await out.res.json()).device, undefined, "session actions never mint a device id");
  assert.equal(calls.findLast((c) => c.path === "/v1/auth/logout").headers.authorization, `Bearer ${TOKENS.user}`);
});

test("register: the referral comes from the body, attribution never does (the app is recorded as the source)", async () => {
  const m = await load();
  const { res } = await viaProxy(m, "/api/mobile/auth/register", { method: "POST", headers: { ...json, "x-kalks-platform": "android" }, body: JSON.stringify({ first_name: "A", last_name: "B", email: "new@example.com", password: "Secret123", country: "IN", referral_code: "abc123", referral_campaign: "summer", attribution: { utm_source: "forged" } }) }, m.auth.POST, { action: "register" });
  assert.equal(res.status, 200);
  assert.equal((await res.json()).challenge, "c-reg");
  const sent = calls.findLast((c) => c.path === "/v1/auth/register").body;
  assert.equal(sent.referral_code, "abc123");
  assert.equal(sent.referral_campaign, "summer");
  assert.deepEqual(sent.attribution, { utm_source: "kalks_app", utm_medium: "android" });
  assert.equal(sent.marketing_consent, true);
});

test("auth input is checked: JSON only, unknown actions 404", async () => {
  const m = await load();
  const form = await viaProxy(m, "/api/mobile/auth/login", { method: "POST", headers: { "content-type": "application/x-www-form-urlencoded" }, body: "email=a" }, m.auth.POST, { action: "login" });
  assert.equal(form.res.status, 415);
  const arr = await viaProxy(m, "/api/mobile/auth/login", { method: "POST", headers: json, body: "[]" }, m.auth.POST, { action: "login" });
  assert.equal(arr.res.status, 400);
  const res = await m.auth.POST(new m.NextRequest(`${BASE}/api/mobile/auth/nope`, { method: "POST", headers: json, body: "{}" }), { params: Promise.resolve({ action: "nope" }) });
  assert.equal(res.status, 404);
  // defence in depth: should a request reach the handler with a session cookie and a bearer, it is refused
  const mixed = await m.auth.GET(new m.NextRequest(`${BASE}/api/mobile/auth/me`, { headers: { ...auth(TOKENS.user), cookie: `kalks_session=${TOKENS.user}` } }), { params: Promise.resolve({ action: "me" }) });
  assert.equal(mixed.status, 400);
});

/* ------------------------------------------------------------------ */
/* Config                                                              */
/* ------------------------------------------------------------------ */

test("config: service URLs, streams and the broker's branding", async () => {
  const m = await load();
  const { res } = await viaProxy(m, "/api/mobile/config", { headers: { host: "app.kalkstrade.com", "x-forwarded-proto": "https" } }, m.config.GET, {});
  assert.equal(res.status, 200);
  const c = await res.json();
  assert.equal(c.apiVersion, 1);
  assert.deepEqual(c.urls, {
    app: "https://app.kalkstrade.com",
    terminal: "https://trade.kalkstrade.com",
    marketData: { http: "https://api.kalkstrade.com", ws: "wss://api.kalkstrade.com/v1/stream" },
    streams: { engine: "wss://trade.kalkstrade.com/engine/stream", options: "wss://trade.kalkstrade.com/options/stream", support: "wss://app.kalkstrade.com/support/stream" },
  });
  assert.equal(c.tenant.slug, "kalks");
  assert.equal(c.tenant.default, true);
  assert.deepEqual(c.modules, { wallet: true, prop: true });
  assert.deepEqual(c.flags, { demo_accounts: true });
  assert.deepEqual(c.maintenance, { active: false, message: "", until: null });
  assert.equal(c.minAppVersion, null);
  assert.equal(JSON.stringify(c).includes("TOKEN"), false);

  // a white-label broker: its own trade domain and colours (hex only)
  const b = await (await viaProxy(m, "/api/mobile/config", { headers: { host: "app.broker.test", "x-forwarded-proto": "https" } }, m.config.GET, {}, "https://app.broker.test")).res.json();
  assert.equal(b.urls.app, "https://app.broker.test");
  assert.equal(b.urls.terminal, "https://trade.broker.test");
  assert.equal(b.urls.streams.engine, "wss://trade.broker.test/engine/stream");
  assert.equal(b.urls.streams.options, "wss://trade.broker.test/options/stream");
  assert.equal(b.urls.streams.support, "wss://app.broker.test/support/stream");
  assert.deepEqual(b.tenant, { slug: "acme", name: "Acme FX", default: false, logoUrl: "https://cdn.acme.test/logo.png", primary: "#12ab34", accent: null, supportEmail: "help@acme.test", website: "https://acme.test" });
  assert.deepEqual(b.modules, { prop: false });

  // maintenance is reported, not refused
  const d = await (await viaProxy(m, "/api/mobile/config", { headers: { host: "down.broker.test" } }, m.config.GET, {}, "https://down.broker.test")).res.json();
  assert.deepEqual(d.maintenance, { active: true, message: "Back at 10:00", until: "2026-10-08T10:00:00Z" });
});

test("service URLs on a local stack point at the services' own ports", async () => {
  const { mobile } = await load();
  const saved = { t: process.env.NEXT_PUBLIC_TERMINAL_URL, md: process.env.NEXT_PUBLIC_MARKET_DATA_URL };
  process.env.NEXT_PUBLIC_TERMINAL_URL = "http://localhost:3002";
  process.env.NEXT_PUBLIC_MARKET_DATA_URL = "http://127.0.0.1:8081";
  try {
    const u = mobile.serviceUrls(new URL("http://localhost:3000"));
    assert.equal(u.app, "http://localhost:3000");
    assert.deepEqual(u.marketData, { http: "http://127.0.0.1:8081", ws: "ws://127.0.0.1:8081/v1/stream" });
    assert.match(u.streams.engine, /^ws:\/\/127\.0\.0\.1:\d+\/v1\/terminal\/stream$/);
    assert.match(u.streams.options, /^ws:\/\/127\.0\.0\.1:\d+\/v1\/options\/stream$/);
    assert.match(u.streams.support, /^ws:\/\/127\.0\.0\.1:\d+\/v1\/stream$/);
  } finally {
    process.env.NEXT_PUBLIC_TERMINAL_URL = saved.t;
    process.env.NEXT_PUBLIC_MARKET_DATA_URL = saved.md;
  }
});

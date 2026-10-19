// Challenges routes on the growth BFF (app/api/growth/[[...path]], phase 4): `node --test apps/crm/tests`.
// Stub gateway (session) and growth service on loopback ports; the route handler runs as it is.
//
// What it guards: the hub, missions, XP, nickname and leaderboard routes reach /v1/growth/me/* with the session's
// user id (never one from the browser), only the whitelisted query keys pass, the nickname write is same-origin
// JSON, unknown paths stay 404, and the app reaches the same routes through /api/mobile/growth/*.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import "./helpers/ts-hooks.mjs";

const TOKEN = "u".repeat(43);
const USER = { id: 42, email: "arjun@example.com", first_name: "Arjun", last_name: "Mehta", kyc_status: "verified", country: "IN", tenant: { slug: "kalks", name: "Kalks" } };
const calls = [];
let gateway, growthSvc;

function stub(name, handle) {
  const server = createServer(async (req, res) => {
    const chunks = [];
    for await (const c of req) chunks.push(c);
    const raw = Buffer.concat(chunks).toString("utf8");
    const body = raw ? JSON.parse(raw) : undefined;
    calls.push({ svc: name, method: req.method, path: req.url, headers: req.headers, body });
    const [status, data] = await handle(req, body);
    res.writeHead(status, { "content-type": "application/json" });
    res.end(JSON.stringify(data));
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

before(async () => {
  gateway = await stub("gateway", (req) => {
    const p = new URL(req.url, "http://x").pathname;
    if (p === "/v1/auth/me") return (req.headers.authorization ?? "").endsWith(TOKEN) ? [200, { user: USER, viewer: null, session: { id: 1 } }] : [401, { error: { code: "unauthorized" } }];
    return [404, { error: { code: "not_found" } }];
  });
  growthSvc = await stub("growth", (req, body) => {
    const p = new URL(req.url, "http://x").pathname;
    if (p === "/v1/growth/me/hub") return [200, { level: { level: 2, name: "Trader", xp: 120 }, missions: [], contests: [], recent: [], streaks: {}, nickname: { nickname: "SwiftFalcon482" } }];
    if (p === "/v1/growth/me/nickname" && req.method === "POST") return body?.nickname === "taken" ? [409, { error: { code: "nickname_taken", message: "That nickname is taken." } }] : [200, { nickname: body?.nickname ?? "SwiftFalcon482", public: body?.public ?? true }];
    if (p.startsWith("/v1/growth/me/")) return [200, { items: [], me: null, ok: p }];
    return [404, { error: { code: "not_found" } }];
  });
  process.env.GATEWAY_URL = `http://127.0.0.1:${gateway.address().port}`;
  process.env.GROWTH_URL = `http://127.0.0.1:${growthSvc.address().port}`;
  process.env.GROWTH_INTERNAL_TOKEN = "growth-test-token";
});

after(() => {
  gateway?.close();
  growthSvc?.close();
});

const BASE = "https://app.kalkstrade.com";
const load = async () => ({ ...(await import("next/server")), route: await import("../app/api/growth/[[...path]]/route.ts"), mobile: await import("../lib/mobile.ts") });
const ctx = (path) => ({ params: Promise.resolve({ path: path.split("/") }) });
const cookie = { cookie: `kalks_session=${TOKEN}` };

async function get(m, path, query = "", headers = cookie) {
  const req = new m.NextRequest(`${BASE}/api/growth/${path}${query}`, { headers });
  return m.route.GET(req, ctx(path));
}

async function post(m, path, body, headers = {}) {
  const req = new m.NextRequest(`${BASE}/api/growth/${path}`, { method: "POST", headers: { ...cookie, origin: BASE, host: "app.kalkstrade.com", "content-type": "application/json", ...headers }, body: JSON.stringify(body) });
  return m.route.POST(req, ctx(path));
}

test("hub, missions, xp, nickname and leaderboard reach the service as the session's client", async () => {
  const m = await load();
  for (const path of ["hub", "missions", "xp", "nickname", "leaderboard"]) {
    calls.length = 0;
    const res = await get(m, path, path === "leaderboard" ? "?period=week&contest=12&user_id=7&secret=x" : "?user_id=7");
    assert.equal(res.status, 200, path);
    assert.equal(res.headers.get("cache-control"), "no-store");
    const g = calls.find((c) => c.svc === "growth");
    assert.ok(g, path);
    assert.equal(g.headers["x-kalks-user-id"], "42", "the session's user, never the browser's");
    assert.equal(g.headers["x-kalks-internal"], "growth-test-token");
    assert.equal(g.headers["x-kalks-tenant"], "kalks");
    if (path === "leaderboard") assert.equal(g.path, "/v1/growth/me/leaderboard?contest=12&period=week", "only whitelisted query keys");
    else assert.equal(g.path, `/v1/growth/me/${path}`);
  }
  const hub = await (await get(m, "hub")).json();
  assert.equal(hub.level.name, "Trader");
});

test("signed out, unknown paths and cross-site writes are refused", async () => {
  const m = await load();
  calls.length = 0;
  assert.equal((await get(m, "hub", "", {})).status, 401);
  assert.equal((await get(m, "missions/1/claim")).status, 404);
  assert.equal((await get(m, "rank")).status, 404, "the Trader rank widget is served by Kalks Trader's own BFF");
  const cross = await post(m, "nickname", { nickname: "PipHunter" }, { origin: "https://evil.example" });
  assert.equal(cross.status, 403);
  assert.equal(calls.filter((c) => c.svc === "growth").length, 0, "nothing reached the service");
});

test("nickname writes pass through, errors keep their code", async () => {
  const m = await load();
  calls.length = 0;
  const ok = await post(m, "nickname", { nickname: "PipHunter", public: false });
  assert.equal(ok.status, 200);
  assert.deepEqual(await ok.json(), { nickname: "PipHunter", public: false });
  const g = calls.find((c) => c.svc === "growth");
  assert.equal(g.method, "POST");
  assert.deepEqual(g.body, { nickname: "PipHunter", public: false });
  const taken = await post(m, "nickname", { nickname: "taken" });
  assert.equal(taken.status, 409);
  assert.equal((await taken.json()).error.code, "nickname_taken");
});

test("the app reaches the challenges routes through /api/mobile/growth/*", async () => {
  const m = await load();
  for (const p of ["hub", "missions", "xp", "nickname", "leaderboard"]) {
    const r = m.mobile.mobileRoute(`/api/mobile/growth/${p}`);
    assert.ok(r, p);
    assert.deepEqual(r, { kind: "rewrite", target: `/api/growth/${p}`, policyPath: `/api/growth/${p}` });
  }
});

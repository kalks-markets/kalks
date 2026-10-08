// Broker module switches (lib/tenant-config.ts, applied by proxy.ts): `node --test apps/crm/tests`. The proxy runs
// as it is against a stub gateway on a loopback port that answers /v1/public/tenant-config per host.
//
// What it guards: a page or BFF path follows its module; the strategy catalogue (/api/algo/meta) and the account
// picker (/api/algo/accounts) serve both the strategy builder and AI Trader (Algo) and API keys and webhooks (API), so
// they stay open while EITHER module is on and close only when both are off.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import "./helpers/ts-hooks.mjs";

const MODULES = {
  "algo-off.broker.test": { algo: false },
  "api-off.broker.test": { api: false },
  "both-off.broker.test": { algo: false, api: false },
  "all-on.broker.test": {},
  "new-off.broker.test": { options: false, news: false, calendar: false, markets: false, mam: false, support_chat: false },
  "ai-off.broker.test": { ai: false },
  "chat-ai-off.broker.test": { support_chat: false, ai: false },
};
let gateway;

before(async () => {
  gateway = createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    res.writeHead(url.pathname === "/v1/public/tenant-config" ? 200 : 404, { "content-type": "application/json" });
    if (url.pathname !== "/v1/public/tenant-config") return res.end(JSON.stringify({ error: { code: "not_found", message: "stub" } }));
    res.end(JSON.stringify({ maintenance: { active: false }, modules: MODULES[req.headers["x-kalks-host"]] ?? {}, flags: {} }));
  });
  await new Promise((resolve) => gateway.listen(0, "127.0.0.1", resolve));
  process.env.GATEWAY_URL = `http://127.0.0.1:${gateway.address().port}`;
});

after(() => gateway?.close());

// modules read their upstream URLs at import time: import them after the stub is listening
const load = async () => ({ ...(await import("next/server")), cfg: await import("../lib/tenant-config.ts"), proxy: (await import("../proxy.ts")).proxy });

/** The proxy's answer for a GET of `path` on `host`: "open", or the module_disabled refusal. */
async function gate(m, host, path) {
  const res = await m.proxy(new m.NextRequest(`https://${host}${path}`, { headers: { host } }));
  if (res.status === 403) {
    const body = await res.json();
    assert.equal(body.error.code, "module_disabled", `${host}${path}`);
    return "off";
  }
  return "open";
}

test("shared algo paths belong to both modules; the others to one", async () => {
  const { cfg } = await load();
  assert.equal(cfg.moduleFor("/api/algo/meta"), "algo|api");
  assert.equal(cfg.moduleFor("/api/algo/accounts"), "algo|api");
  assert.equal(cfg.moduleFor("/api/algo/keys/9/revoke"), "api");
  assert.equal(cfg.moduleFor("/api/algo/strategies/4"), "algo");
  assert.equal(cfg.moduleFor("/api/algo/controls/kill"), "algo");
  assert.equal(cfg.moduleFor("/api/algo/metadata"), "api", "a prefix match is by path segment");
  assert.equal(cfg.moduleFor("/wallet"), "wallet");
  assert.equal(cfg.moduleFor("/api/trading/accounts"), null);
  // closed only when every module of the path is off
  assert.equal(cfg.moduleOff({ algo: false }, "/api/algo/meta"), false);
  assert.equal(cfg.moduleOff({ api: false }, "/api/algo/accounts"), false);
  assert.equal(cfg.moduleOff({ algo: false, api: false }, "/api/algo/meta"), true);
  assert.equal(cfg.moduleOff({ api: false }, "/api/algo/keys"), true);
  assert.equal(cfg.moduleOff({ algo: false }, "/api/algo/keys"), false);
  assert.equal(cfg.moduleOff({}, "/api/algo/strategies"), false);
  assert.equal(cfg.moduleOff({ algo: true }, "/api/algo/strategies"), false);
  assert.equal(cfg.moduleOff({ wallet: false }, "/api/trading/accounts"), false);
});

test("the strategy catalogue and account picker stay open while Algo or API is on", async () => {
  const m = await load();
  for (const path of ["/api/algo/meta", "/api/algo/accounts"]) {
    assert.equal(await gate(m, "all-on.broker.test", path), "open", path);
    // API off, Algo on: the strategy builder and AI Trader still load their catalogue and accounts
    assert.equal(await gate(m, "api-off.broker.test", path), "open", path);
    // Algo off, API on: API keys and webhooks still pick an account
    assert.equal(await gate(m, "algo-off.broker.test", path), "open", path);
    assert.equal(await gate(m, "both-off.broker.test", path), "off", path);
  }
  // single-module paths are unchanged
  assert.equal(await gate(m, "api-off.broker.test", "/api/algo/keys"), "off");
  assert.equal(await gate(m, "api-off.broker.test", "/api/algo/webhooks"), "off");
  assert.equal(await gate(m, "api-off.broker.test", "/api/algo/strategies"), "open");
  assert.equal(await gate(m, "algo-off.broker.test", "/api/algo/strategies"), "off");
  assert.equal(await gate(m, "algo-off.broker.test", "/api/algo/ai/strategy"), "off");
  assert.equal(await gate(m, "algo-off.broker.test", "/api/algo/keys"), "open");
});

test("the new module keys own their pages and BFF paths", async () => {
  const { cfg } = await load();
  const want = {
    "/options": "options",
    "/api/suitability/options": "options",
    "/markets": "markets",
    "/news": "news",
    "/api/news/feed": "news",
    "/api/news/map": "news",
    "/api/news/brief": "news",
    "/calendar": "calendar",
    "/api/news/calendar/next": "calendar",
    "/api/news/me/calendar/reminders": "calendar",
    "/social/mam": "mam",
    "/social/managed": "mam",
    "/api/social/mam/links": "mam",
    "/social/copy": "copy_trading",
    "/social/pamm": "pamm",
    "/api/support/messages": "support_chat|ai",
    "/api/support/conversations/4/resolve": "support_chat|ai",
    "/api/mobile/trade/options/orders": "options",
    "/api/mobile/trade/options/explain": "options+ai",
    "/api/mobile/trade/ai-trader": "ai",
    "/api/mobile/trade/mam": "mam",
  };
  for (const [path, mod] of Object.entries(want)) assert.equal(cfg.moduleFor(path), mod, path);
  // the bell's stream ticket and the Support page itself (email) stay open with the chat off
  assert.equal(cfg.moduleFor("/api/support/stream-ticket"), null);
  assert.equal(cfg.moduleFor("/support"), null);
  assert.equal(cfg.moduleFor("/api/mobile/trade/orders"), null, "CFD trading is not a module");
  // "+" needs every module, "|" any one of them
  assert.equal(cfg.moduleOff({ ai: false }, "/api/mobile/trade/options/explain"), true);
  assert.equal(cfg.moduleOff({ options: false }, "/api/mobile/trade/options/explain"), true);
  assert.equal(cfg.moduleOff({}, "/api/mobile/trade/options/explain"), false);
  assert.equal(cfg.offModule({ ai: false }, "/api/mobile/trade/options/explain"), "ai");
  assert.equal(cfg.offModule({ options: false }, "/options"), "options");
  assert.equal(cfg.offModule({}, "/options"), null);
});

test("pages of a switched-off module show the not-available page named after it; their APIs refuse", async () => {
  const m = await load();
  for (const [path, mod] of [["/options", "options"], ["/news", "news"], ["/calendar", "calendar"], ["/markets", "markets"], ["/social/mam", "mam"], ["/social/managed", "mam"]]) {
    const res = await m.proxy(new m.NextRequest(`https://new-off.broker.test${path}`, { headers: { host: "new-off.broker.test" } }));
    const to = new URL(res.headers.get("x-middleware-rewrite"));
    assert.equal(to.pathname, "/unavailable", path);
    assert.equal(to.searchParams.get("m"), mod, path);
  }
  for (const path of ["/api/news/feed", "/api/news/calendar", "/api/social/mam/links", "/api/suitability/options", "/api/support/messages"]) {
    assert.equal(await gate(m, "new-off.broker.test", path), path === "/api/support/messages" ? "open" : "off", path);
  }
  // the rest of social, the Support page and the bell stay
  assert.equal(await gate(m, "new-off.broker.test", "/api/social/funds"), "open");
  assert.equal(await gate(m, "new-off.broker.test", "/api/support/stream-ticket"), "open");
  // the chat API serves the chat and Ask Kalks AI: closed only when both are off
  assert.equal(await gate(m, "ai-off.broker.test", "/api/support/messages"), "open");
  assert.equal(await gate(m, "chat-ai-off.broker.test", "/api/support/messages"), "off");
  assert.equal(await gate(m, "chat-ai-off.broker.test", "/api/support/stream-ticket"), "open");
  // the app's native trade routes
  assert.equal(await gate(m, "new-off.broker.test", "/api/mobile/trade/options/orders"), "off");
  assert.equal(await gate(m, "ai-off.broker.test", "/api/mobile/trade/ai-trader"), "off");
  assert.equal(await gate(m, "ai-off.broker.test", "/api/mobile/trade/options/explain"), "off");
  assert.equal(await gate(m, "new-off.broker.test", "/api/mobile/trade/mam"), "off");
  assert.equal(await gate(m, "all-on.broker.test", "/api/mobile/trade/options/orders"), "open");
});

test("navigation and commands drop the new modules' pages", async () => {
  const { navForFeatures, pageModule } = await import("../components/tenant-config.tsx");
  const nav = [
    { key: "dashboard", label: "Dashboard", href: "/", sub: [{ href: "/", label: "Overview" }, { href: "/markets", label: "Markets" }, { href: "/news", label: "News" }, { href: "/calendar", label: "Calendar" }] },
    { key: "options", label: "Options", href: "/options" },
    { key: "social", label: "Social", href: "/social", sub: [{ href: "/social", label: "Discover" }, { href: "/social/mam", label: "MAM manager" }, { href: "/social/managed", label: "Managed" }] },
  ];
  const out = navForFeatures(nav, { modules: { markets: false, news: false, calendar: false, options: false, mam: false }, flags: {} });
  assert.deepEqual(out.map((m) => m.key), ["dashboard", "social"]);
  assert.deepEqual(out[0].sub.map((s) => s.href), ["/"]);
  assert.deepEqual(out[1].sub.map((s) => s.href), ["/social"]);
  assert.equal(pageModule("/social/managed"), "mam");
  assert.equal(pageModule("/options"), "options");
  // unknown config: everything stays
  assert.equal(navForFeatures(nav, null).length, 3);
});

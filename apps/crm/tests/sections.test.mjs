// Client Area sections (docs/SECTIONS.md, packages/ui/src/lib/sections.ts, lib/features.ts, proxy.ts):
// `node --test apps/crm/tests`. The proxy and the route handler run as they are against stub gateway and trading
// servers on a loopback port.
//
// What it guards: every nav page and every dashboard block has a section key and every key is used; a hidden page
// sends the client to the dashboard and its single-page APIs answer 403 section_disabled (cookie and mobile paths
// alike); the client's own switches (gateway /v1/me/features) win over the broker's switches for everyone; account
// groups are read from the engine only when the broker has a group rule; a switched-off module hides its sections;
// the nav and the palette drop hidden pages.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import "./helpers/ts-hooks.mjs";

const T = { priya: "p".repeat(43), omar: "o".repeat(43), dead: "d".repeat(43), pro: "g".repeat(43) };
// the broker's switches for everyone, per host
const TENANTS = {
  "app.kalkstrade.com": { sections: { "page.news": false, "page.profile.viewers": false, "dashboard.banners": false }, modules: {} },
  "app.wide-open.test": { sections: {}, modules: {} },
};
// the client's own (after country / group / client rules); `pro` only differs once its account groups are sent
const MINE = {
  [T.priya]: { user_id: 1, group_rules: false, sections: { "page.news": true, "page.profile.viewers": false, "dashboard.banners": false } },
  [T.omar]: { user_id: 2, group_rules: false, sections: { "page.news": false, "page.profile.viewers": true, "dashboard.banners": false } },
  [T.pro]: { user_id: 3, group_rules: true, sections: { "page.news": true } },
};
const calls = [];
let server;

before(async () => {
  server = createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    calls.push({ path: url.pathname, search: url.search, headers: req.headers });
    const send = (status, data) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(data));
    };
    const host = req.headers["x-kalks-host"];
    if (url.pathname === "/v1/public/tenant-config") {
      const t = TENANTS[host] ?? TENANTS["app.kalkstrade.com"];
      return send(200, { maintenance: { active: false }, modules: t.modules, flags: {}, sections: t.sections, features_version: 1, tenant: { slug: "kalks" } });
    }
    if (url.pathname === "/v1/me/features") {
      const tok = (req.headers.authorization ?? "").replace(/^Bearer /, "");
      const m = MINE[tok];
      if (!m) return send(401, { error: { code: "unauthorized", message: "Please sign in." } });
      const groups = url.searchParams.get("groups");
      const sections = tok === T.pro && groups === "pro,std" ? { "page.news": false } : m.sections;
      return send(200, { ...m, modules: {}, flags: {}, sections, groups: groups ? groups.split(",") : [], version: 1 });
    }
    // the trading engine: the client's accounts (their groups)
    if (url.pathname === "/v1/accounts") return send(200, { accounts: [{ login: 1, group: "std" }, { login: 2, group: "PRO" }, { login: 3, group: "pro" }] });
    send(404, { error: { code: "not_found", message: "stub" } });
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  process.env.GATEWAY_URL = `http://127.0.0.1:${server.address().port}`;
  process.env.TRADING_URL = process.env.GATEWAY_URL;
});

after(() => server?.close());

// modules read their upstream URLs at import time: import them after the stub is listening
const load = async () => ({ ...(await import("next/server")), proxy: (await import("../proxy.ts")).proxy, sections: await import("@kalks/ui/sections") });

/** The proxy's answer for a GET of `path` on `host` (with a session cookie, or a bearer token for /api/mobile). */
async function visit(m, path, { host = "app.kalkstrade.com", token, bearer } = {}) {
  const headers = { host };
  if (token) headers.cookie = `kalks_session=${token}`;
  if (bearer) headers.authorization = `Bearer ${bearer}`;
  const res = await m.proxy(new m.NextRequest(`https://${host}${path}`, { headers }));
  if (res.status === 403) return (await res.json()).error.code;
  const to = res.headers.get("location");
  if (to) return `redirect ${new URL(to).pathname}${new URL(to).search}`;
  return "open";
}

test("every Client Area page and dashboard block has a section, and every section is used", async () => {
  const { SECTIONS, sectionFor } = await import("@kalks/ui/sections");
  const { CRM_NAV } = await import("../lib/nav.ts");
  const pages = new Set(SECTIONS.filter((s) => s.kind === "page").map((s) => s.path));
  const navHrefs = CRM_NAV.flatMap((m) => (m.sub?.length ? m.sub.map((s) => s.href) : [m.href]));
  for (const href of navHrefs) {
    if (href === "/") continue; // the dashboard itself: its blocks have the switches
    assert.ok(pages.has(href), `nav entry ${href} has a section`);
    assert.equal(sectionFor(href), `page.${href.slice(1).replaceAll("/", ".")}`, href);
  }
  for (const p of pages) assert.ok(navHrefs.includes(p), `section page ${p} is in the nav`);
  assert.equal(sectionFor("/"), null);
  // detail pages follow their page; unknown APIs have no section
  assert.equal(sectionFor("/wallet/deposit/usdt"), "page.wallet.deposit");
  assert.equal(sectionFor("/social/masters/42"), "page.social");
  assert.equal(sectionFor("/api/wallet/withdrawals"), null, "shared APIs stay open");
  assert.equal(sectionFor("/api/security/viewers/7/revoke"), "page.profile.viewers");
  assert.equal(sectionFor("/api/security/viewer-activity"), null, "by path segment");
  // the dashboard renders exactly the dashboard.* blocks
  const src = readFileSync(new URL("../components/dashboard/live-dashboard.tsx", import.meta.url), "utf8");
  const used = new Set([...src.matchAll(/"(dashboard\.[a-z_]+)"/g)].map((x) => x[1]));
  const blocks = new Set(SECTIONS.filter((s) => s.kind === "block").map((s) => s.key));
  assert.deepEqual([...used].sort(), [...blocks].sort());
});

test("a section follows its own switch and its module's", async () => {
  const { sectionOn, pathOn } = await import("@kalks/ui/sections");
  assert.equal(sectionOn(null, "page.news"), true, "unknown config: on");
  assert.equal(sectionOn({ modules: {}, sections: {} }, "page.news"), true, "missing key: on");
  assert.equal(sectionOn({ modules: {}, sections: { "page.news": false } }, "page.news"), false);
  // a switched-off module hides its sections whatever they say
  assert.equal(sectionOn({ modules: { wallet: false }, sections: { "dashboard.wallet_card": true } }, "dashboard.wallet_card"), false);
  assert.equal(sectionOn({ modules: { wallet: false }, sections: {} }, "dashboard.balance_total"), true);
  assert.equal(pathOn({ modules: { news: false }, sections: { "page.news": true } }, "/news"), false);
  assert.equal(pathOn({ modules: {}, sections: { "page.wallet.deposit": false } }, "/wallet/deposit"), false);
  assert.equal(pathOn({ modules: {}, sections: { "page.wallet.deposit": false } }, "/wallet"), true);
});

test("hidden pages send the client to the dashboard; their single-page APIs refuse", async () => {
  const m = await load();
  // signed out: the broker's switches for everyone (then the sign-in gate as usual)
  assert.equal(await visit(m, "/news"), "redirect /");
  assert.equal(await visit(m, "/calendar"), "redirect /login?next=%2Fcalendar");
  assert.equal(await visit(m, "/news", { host: "app.wide-open.test" }), "redirect /login?next=%2Fnews");
  // Priya's own rule shows News; Omar's doesn't, but shows him view-only access
  assert.equal(await visit(m, "/news", { token: T.priya }), "open");
  assert.equal(await visit(m, "/news", { token: T.omar }), "redirect /");
  assert.equal(await visit(m, "/profile/viewers", { token: T.priya }), "redirect /");
  assert.equal(await visit(m, "/profile/viewers", { token: T.omar }), "open");
  assert.equal(await visit(m, "/api/security/viewers", { token: T.priya }), "section_disabled");
  assert.equal(await visit(m, "/api/security/viewers", { token: T.omar }), "open");
  // shared APIs and the dashboard are never held by a section
  assert.equal(await visit(m, "/api/security/sessions", { token: T.priya }), "open");
  assert.equal(await visit(m, "/", { token: T.omar }), "open");
  // a dead session falls back to the switches for everyone
  assert.equal(await visit(m, "/api/security/viewers", { token: T.dead }), "section_disabled");
  assert.equal(await visit(m, "/api/security/viewers", { token: T.dead, host: "app.wide-open.test" }), "open");
});

test("the mobile app's paths follow the same sections", async () => {
  const m = await load();
  assert.equal(await visit(m, "/api/mobile/security/viewers", { bearer: T.priya }), "section_disabled");
  assert.equal(await visit(m, "/api/mobile/security/viewers", { bearer: T.omar }), "open");
  assert.equal(await visit(m, "/api/mobile/security/sessions", { bearer: T.priya }), "open");
  // the client's own switches for the app
  const { GET } = await import("../app/api/me/features/route.ts");
  const res = await GET(new m.NextRequest("https://app.kalkstrade.com/api/me/features", { headers: { host: "app.kalkstrade.com", cookie: `kalks_session=${T.omar}` } }));
  const v = await res.json();
  assert.equal(v.personal, true);
  assert.deepEqual(v.sections, MINE[T.omar].sections);
  const anon = await (await GET(new m.NextRequest("https://app.kalkstrade.com/api/me/features", { headers: { host: "app.kalkstrade.com" } }))).json();
  assert.equal(anon.personal, false);
  assert.deepEqual(anon.sections, TENANTS["app.kalkstrade.com"].sections);
});

test("account groups are read from the engine only when the broker has a group rule", async () => {
  const m = await load();
  const before = calls.filter((c) => c.path === "/v1/accounts").length;
  assert.equal(await visit(m, "/news", { token: T.priya, host: "app.wide-open.test" }), "open");
  assert.equal(calls.filter((c) => c.path === "/v1/accounts").length, before, "no group rule: no engine call");
  // the pro client: their accounts' groups (std, pro) go to the gateway, whose group rule hides News
  assert.equal(await visit(m, "/news", { token: T.pro }), "redirect /");
  const acc = calls.filter((c) => c.path === "/v1/accounts").at(-1);
  assert.equal(acc.headers["x-kalks-user-id"], "3");
  const me = calls.filter((c) => c.path === "/v1/me/features").at(-1);
  assert.equal(new URLSearchParams(me.search).get("groups"), "pro,std");
  // cached per session: the next page load asks nobody
  const n = calls.length;
  assert.equal(await visit(m, "/news", { token: T.pro }), "redirect /");
  assert.deepEqual(calls.slice(n).filter((c) => c.path !== "/v1/public/tenant-config"), []);
});

test("navigation and the palette drop hidden pages; a nav entry goes when none of its pages is left", async () => {
  const { navForFeatures } = await import("../components/tenant-config.tsx");
  const nav = [
    { key: "dashboard", label: "Dashboard", href: "/", sub: [{ href: "/", label: "Overview" }, { href: "/news", label: "News" }] },
    { key: "wallet", label: "Wallet", href: "/wallet", sub: [{ href: "/wallet", label: "Overview" }, { href: "/wallet/deposit", label: "Deposit" }, { href: "/wallet/history", label: "History" }] },
    { key: "profile", label: "Profile", href: "/profile", sub: [{ href: "/profile/viewers", label: "Viewers" }] },
    { key: "support", label: "Support", href: "/support" },
  ];
  const out = navForFeatures(nav, { modules: {}, flags: {}, sections: { "page.news": false, "page.wallet": false, "page.wallet.deposit": false, "page.profile.viewers": false, "page.support": false } });
  assert.deepEqual(out.map((x) => x.key), ["dashboard", "wallet"]);
  assert.deepEqual(out[0].sub.map((s) => s.href), ["/"]);
  // the wallet entry now opens on its first page left
  assert.equal(out[1].href, "/wallet/history");
  assert.deepEqual(out[1].sub.map((s) => s.href), ["/wallet/history"]);
  // a module off hides its pages even when their sections are on
  const off = navForFeatures(nav, { modules: { wallet: false }, flags: {}, sections: { "page.wallet": true } });
  assert.deepEqual(off.map((x) => x.key), ["dashboard", "profile", "support"]);
});

import type { NextRequest } from "next/server";
import { csrf, error, reply, soft } from "@/lib/engine/server";
import { actingAccount, options, optionsStreamUrl, publicBook, publicChain, tenantOf } from "@/lib/options/server";
import { moduleGate } from "@/lib/tenant-brand";

// Kalks FX Options BFF (read side). Browser -> /api/options/<route> (same origin; `X-Kalks-Login` names the acting
// account, whose engine session is in the HttpOnly cookie) -> services/options /v1/options/… with the internal
// token, the broker (X-Kalks-Tenant) and the account's kind (X-Kalks-Account-Kind) + group (pricing). Trading
// (preview, orders, closes, settlements) goes through the engine BFF: /api/engine/options/*.
//
//   GET  underlyings                      {underlyings[], version}
//   GET  expiries?u=                      {underlying, expiries[], version}
//   GET  chain?u=&expiry=                 chain header + rows[] (the account group's spreads)
//   GET  series/{code}                    {series, expiry, quote, …}
//   GET  candles?series=&tf=&limit=&to=   premium candles of one series, USD per contract (tf minutes: 1 5 15 30 60 240 1440;
//                                         + barrier=&level=&rebate=&knockedAt= for a barrier position)
//   GET  smile?u=&expiry=                 {points[], pillars[], termStructure[], atmVol}
//   POST stream-ticket                    {ticket, expiresIn, url}: one-time WebSocket ticket for the chain stream
//   GET  public/chain/{u}?expiry=         guest chain (no session; 404 options_disabled until the public chain is on)
//   GET  public/stream-url                {url}: guest chain stream (no ticket)
//   GET  public/book/{series}             order book depth, 10 levels each side (docs/OPTIONS-EXCHANGE.md §10)
//   GET  public/trades/{series}?limit=    trade tape of a series
//   GET  public/stats/{u}                 open interest and volume per strike of an underlying
//
// The service answers 404 `options_disabled` while the module is off for this broker / account kind: the terminal
// shows "Options launching soon".

type Ctx = { params: Promise<{ path: string[] }> };

const U_RE = /^[A-Z]{3,8}$/;
const DATE_RE = /^\d{4}-\d{2}-\d{2}$/;
const SERIES_RE = /^[A-Z0-9]{3,12}-\d{8}-[0-9.]{1,16}-[CP](-[A-Z0-9._]{1,24})?$/;
const CANDLE_TFS = new Set(["1", "5", "15", "30", "60", "240", "1440"]);

const NUM_RE = /^\d{1,12}(\.\d{1,10})?$/;

/**
 * `candles?series=&tf=&limit=&to=` (+ `barrier=UO|DO|UI|DI&level=&rebate=&knockedAt=` for a barrier position) → the
 * service's query (validated), or null.
 */
function candlesQuery(req: NextRequest): string | null {
  const sp = req.nextUrl.searchParams;
  const series = sp.get("series") ?? "";
  const tf = sp.get("tf") ?? "";
  const limit = sp.get("limit");
  const to = sp.get("to");
  if (!SERIES_RE.test(series) || !CANDLE_TFS.has(tf)) return null;
  if (limit !== null && !/^\d{1,4}$/.test(limit)) return null;
  if (to !== null && !/^\d{9,11}$/.test(to)) return null;
  const n = limit === null ? null : Math.min(1500, Math.max(1, Number(limit)));
  let q = `series=${encodeURIComponent(series)}&tf=${tf}${n ? `&limit=${n}` : ""}${to ? `&to=${to}` : ""}`;
  const barrier = sp.get("barrier");
  if (barrier !== null) {
    const level = sp.get("level") ?? "";
    const rebate = sp.get("rebate");
    const knockedAt = sp.get("knockedAt");
    if (!/^(UO|DO|UI|DI)$/.test(barrier) || !NUM_RE.test(level) || (rebate !== null && !NUM_RE.test(rebate)) || (knockedAt !== null && !/^\d{9,11}$/.test(knockedAt))) return null;
    q += `&barrier=${barrier}&level=${level}${rebate !== null ? `&rebate=${rebate}` : ""}${knockedAt !== null ? `&knockedAt=${knockedAt}` : ""}`;
  }
  return q;
}

function q(req: NextRequest, needU: boolean): { u?: string; expiry?: string } | null {
  const sp = req.nextUrl.searchParams;
  const u = sp.get("u")?.toUpperCase() ?? undefined;
  const expiry = sp.get("expiry") ?? undefined;
  if ((needU && !u) || (u && !U_RE.test(u)) || (expiry && !DATE_RE.test(expiry))) return null;
  return { u, expiry };
}

const relay = (r: { status: number; data: unknown }) => reply(r.status, r.data);

async function handle(req: NextRequest, { params }: Ctx, method: "GET" | "POST") {
  const path = (await params).path;
  const [a, b, c] = path;

  // ---- public (guests, the SEO chain page)
  if (a === "public") {
    if (method === "GET" && b === "chain" && path.length === 3 && U_RE.test((c ?? "").toUpperCase())) {
      const expiry = req.nextUrl.searchParams.get("expiry");
      if (expiry && !DATE_RE.test(expiry)) return error(400, "bad_request", "Invalid expiry.");
      const r = await publicChain(c!.toUpperCase(), expiry);
      return reply(r.status, r.data, { headers: r.status === 200 ? { "cache-control": "public, max-age=1" } : {} });
    }
    if (method === "GET" && b === "stream-url" && path.length === 2) return reply(200, { url: optionsStreamUrl(req) });
    // order book market data: the stream's polling fallback (depth / tape of the selected series) and guests
    if (method === "GET" && (b === "book" || b === "trades") && path.length === 3 && SERIES_RE.test(c ?? "")) {
      let q = "";
      if (b === "trades") {
        const limit = req.nextUrl.searchParams.get("limit");
        if (limit !== null && !/^\d{1,3}$/.test(limit)) return error(400, "bad_request", "Invalid limit.");
        if (limit) q = `?limit=${Math.min(200, Math.max(1, Number(limit)))}`;
      }
      const r = await publicBook(`${b}/${encodeURIComponent(c!)}${q}`);
      return reply(r.status, r.data, { headers: r.status === 200 ? { "cache-control": "public, max-age=1" } : {} });
    }
    if (method === "GET" && b === "stats" && path.length === 3 && U_RE.test((c ?? "").toUpperCase())) {
      const r = await publicBook(`stats/${c!.toUpperCase()}`);
      return reply(r.status, r.data, { headers: r.status === 200 ? { "cache-control": "public, max-age=1" } : {} });
    }
    return error(404, "not_found", "Not found.");
  }

  // ---- signed-in reads: the acting account picks the module switch (live/demo) and the group's pricing
  if (method === "POST") {
    const blocked = csrf(req);
    if (blocked) return blocked;
  }
  const who = await actingAccount(req);
  if (!who.ok) return error(who.status, who.code, who.message);
  const { kind, group } = who.account;
  const tenant = await tenantOf(req);
  const call = (p: string, body?: unknown) => options(p, { tenant, kind, body, method: body !== undefined ? "POST" : "GET" });
  const g = encodeURIComponent(group || "*");

  if (method === "GET" && a === "underlyings" && path.length === 1) return relay(await call("/v1/options/underlyings"));
  if (method === "GET" && a === "expiries" && path.length === 1) {
    const x = q(req, true);
    if (!x) return error(400, "bad_request", "Invalid underlying.");
    return relay(await call(`/v1/options/expiries?u=${x.u}`));
  }
  if (method === "GET" && a === "chain" && path.length === 1) {
    const x = q(req, true);
    if (!x) return error(400, "bad_request", "Invalid underlying or expiry.");
    return relay(await call(`/v1/options/chain?u=${x.u}${x.expiry ? `&expiry=${x.expiry}` : ""}&group=${g}`));
  }
  if (method === "GET" && a === "smile" && path.length === 1) {
    const x = q(req, true);
    if (!x) return error(400, "bad_request", "Invalid underlying or expiry.");
    return relay(await call(`/v1/options/smile?u=${x.u}${x.expiry ? `&expiry=${x.expiry}` : ""}`));
  }
  if (method === "GET" && a === "series" && path.length === 2 && SERIES_RE.test(b ?? "")) return relay(await call(`/v1/options/series/${encodeURIComponent(b!)}?group=${g}`));
  if (method === "GET" && a === "candles" && path.length === 1) {
    const cq = candlesQuery(req);
    if (!cq) return error(400, "bad_request", "Invalid series, timeframe, limit or time.");
    return relay(await call(`/v1/options/candles?${cq}`));
  }
  if (method === "POST" && a === "stream-ticket" && path.length === 1) {
    const r = await call("/v1/options/stream/ticket", { group: group || "*" });
    if (r.status !== 200) return relay(r);
    return reply(200, { ticket: r.data.ticket, expiresIn: r.data.expiresIn, url: optionsStreamUrl(req) });
  }
  return error(404, "not_found", "Not found.");
}

export const dynamic = "force-dynamic";
// the broker switched Options off (module switches): every options call answers 403 module_disabled
export const GET = async (req: NextRequest, ctx: Ctx) => soft(req, (await moduleGate(req.headers, "options")) ?? (await handle(req, ctx, "GET")));
export const POST = async (req: NextRequest, ctx: Ctx) => soft(req, (await moduleGate(req.headers, "options")) ?? (await handle(req, ctx, "POST")));

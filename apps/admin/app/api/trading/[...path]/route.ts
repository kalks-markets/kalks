import { NextResponse, type NextRequest } from "next/server";
import { clientIp, gateway } from "@/lib/gateway";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { TRADING_STREAM_URL, engine, tradingConfigured } from "@/lib/trading";
import { tradingAllows, type TradingPerm } from "@/lib/trading-perms";
import { optionsAllows, type OptionsPerm } from "@/lib/options-perms";

// Trading BFF: browser -> /api/trading/<path> (same origin, staff cookie) -> trading engine.
// The staff session is verified with the gateway on every call; the permission for the route is checked here
// (lib/trading-perms.ts), then the engine is called with TRADING_INTERNAL_TOKEN and the staff identity headers
// built from that verified session. The engine checks the role again and writes the audit log.

type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
/** `perm`: the permission the route needs (any one of a list). */
type Route = { method: Method; re: RegExp; perm: TradingPerm | readonly TradingPerm[]; to?: (m: RegExpMatchArray) => string };

const T = "(\\d{1,18})"; // ticket / deal id / login
const ROUTES: Route[] = [
  // dealing desk reads
  { method: "GET", re: /^dealing\/(state|positions|orders|deals|controls|audit)$/, perm: "dealing.read" },
  { method: "GET", re: /^dealing\/routing\/rules$/, perm: "dealing.read" },
  // risk desk (exposure with limit usage, risk limits, alerts, client scores, news windows, routing suggestions)
  { method: "GET", re: /^dealing\/exposure$/, perm: "dealing.read" },
  { method: "GET", re: /^dealing\/risk\/(limits|alerts|scores|news)$/, perm: "dealing.read" },
  { method: "GET", re: new RegExp(`^dealing/risk/scores/${T}$`), perm: "dealing.read" },
  { method: "GET", re: /^dealing\/routing\/suggestions$/, perm: "dealing.read" },
  { method: "GET", re: /^symbols$/, perm: "dealing.read", to: () => "/v1/symbols" },
  // instrument catalogue (Config › Symbols): live-trading switch and spec templates. Changes need dealing.policy here;
  // the engine then allows them only to the platform owner / super admin of the Kalks platform tenant.
  { method: "GET", re: /^admin\/symbols\/catalogue(\/audit)?$/, perm: "dealing.read" },
  { method: "PUT", re: /^admin\/symbols\/live$/, perm: "dealing.policy" },
  { method: "PUT", re: /^admin\/symbols\/templates\/[a-z0-9-]{1,40}$/, perm: "dealing.policy" },
  // stock corporate actions (Trading › Corporate actions): propose / edit with dealing.write, approve / reject with
  // dealing.policy; the engine allows changes to platform staff only and enforces four-eyes on splits and large dividends
  { method: "GET", re: /^admin\/corporate-actions$/, perm: "dealing.read" },
  { method: "GET", re: new RegExp(`^admin/corporate-actions/${T}$`), perm: "dealing.read" },
  { method: "POST", re: /^admin\/corporate-actions$/, perm: "dealing.write" },
  { method: "POST", re: /^admin\/corporate-actions\/import$/, perm: "dealing.write" },
  { method: "PATCH", re: new RegExp(`^admin/corporate-actions/${T}$`), perm: "dealing.write" },
  { method: "POST", re: new RegExp(`^admin/corporate-actions/${T}/(approve|reject)$`), perm: "dealing.policy" },
  { method: "POST", re: new RegExp(`^admin/corporate-actions/${T}/check$`), perm: "dealing.read" },
  // dealing desk writes
  { method: "POST", re: /^dealing\/trades$/, perm: "dealing.write" },
  { method: "POST", re: /^dealing\/positions\/bulk$/, perm: "dealing.write" },
  { method: "PATCH", re: new RegExp(`^dealing/positions/${T}$`), perm: "dealing.write" },
  { method: "POST", re: new RegExp(`^dealing/positions/${T}/(close|add|price-correction|charges|void)$`), perm: "dealing.write" },
  { method: "POST", re: new RegExp(`^dealing/deals/${T}/reopen$`), perm: "dealing.write" },
  { method: "POST", re: /^dealing\/book-transfers$/, perm: "dealing.write" },
  { method: "PATCH", re: new RegExp(`^dealing/orders/${T}$`), perm: "dealing.write" },
  { method: "POST", re: /^dealing\/orders\/cancel$/, perm: "dealing.write" },
  { method: "POST", re: new RegExp(`^dealing/orders/${T}/fill$`), perm: "dealing.write" },
  { method: "PUT", re: /^dealing\/controls\/symbols\/[A-Z0-9._-]{1,20}$/, perm: "dealing.write" },
  { method: "PUT", re: new RegExp(`^dealing/controls/accounts/${T}$`), perm: "dealing.write" },
  { method: "PUT", re: /^dealing\/controls\/tenant$/, perm: "dealing.policy" },
  { method: "PUT", re: /^dealing\/routing\/(rules|quick)$/, perm: "dealing.write" },
  { method: "PUT", re: /^dealing\/risk\/limits$/, perm: "dealing.write" },
  { method: "PUT", re: /^dealing\/risk\/settings$/, perm: "dealing.policy" },
  { method: "POST", re: new RegExp(`^dealing/risk/alerts/${T}/ack$`), perm: "dealing.write" },
  { method: "POST", re: /^dealing\/risk\/scores\/run$/, perm: "dealing.write" },
  { method: "POST", re: new RegExp(`^dealing/routing/suggestions/${T}/(approve|reject)$`), perm: "dealing.write" },
  // trading accounts, groups, ledger
  { method: "GET", re: /^admin\/accounts$/, perm: "accounts.read" },
  { method: "GET", re: new RegExp(`^admin/accounts/${T}$`), perm: "accounts.read" },
  { method: "POST", re: new RegExp(`^admin/accounts/${T}/balance$`), perm: "finance.adjust" },
  { method: "POST", re: new RegExp(`^admin/accounts/${T}/(status|group|leverage)$`), perm: "accounts.write" },
  // lifecycle: archive (optionally emptied first: trades closed, balance to wallet) and restore; reason-coded, audited
  { method: "POST", re: new RegExp(`^admin/accounts/${T}/(archive|restore)$`), perm: "accounts.write" },
  // close permanently (B12, C2, C3, C5, C11, C12): staff requests, the closure queue, reopen (Super Admin, checked by
  // the engine), the exit-reasons report; account policy; bulk archive (C4)
  { method: "GET", re: new RegExp(`^admin/accounts/${T}/closure-check$`), perm: ["accounts.close", "accounts.close.approve"] },
  { method: "POST", re: new RegExp(`^admin/accounts/${T}/closure$`), perm: "accounts.close" },
  { method: "POST", re: new RegExp(`^admin/accounts/${T}/reopen$`), perm: "accounts.close.approve" },
  { method: "GET", re: /^admin\/closures(\/report)?$/, perm: ["accounts.close", "accounts.close.approve"] },
  { method: "GET", re: new RegExp(`^admin/closures/${T}$`), perm: ["accounts.close", "accounts.close.approve"] },
  { method: "POST", re: new RegExp(`^admin/closures/${T}/(approve|reject)$`), perm: "accounts.close.approve" },
  { method: "GET", re: /^admin\/account-policy$/, perm: "accounts.read" },
  { method: "PUT", re: /^admin\/account-policy$/, perm: "dealing.policy" },
  { method: "POST", re: /^admin\/accounts\/bulk$/, perm: "accounts.write" },
  { method: "GET", re: /^admin\/groups$/, perm: "accounts.read" },
  { method: "POST", re: /^admin\/groups$/, perm: "groups.write" },
  { method: "PUT", re: /^admin\/groups\/[a-z0-9-]{1,40}$/, perm: "groups.write" },
  { method: "GET", re: /^admin\/ledger\/accounts$/, perm: "finance.adjust" },
];

/* ---------------- FX Options on the engine ---------------- */

/** Query parameters an option route may forward, each with its validator (anything else is dropped; a listed
 *  parameter with a bad value is refused, never passed through raw). */
const isDay = (v: string) => /^\d{4}-\d{2}-\d{2}$/.test(v) && Number.isFinite(Date.parse(v));
const isDayOrIso = (v: string) => isDay(v) || (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d{1,9})?)?(Z|[+-]\d{2}:?\d{2})$/.test(v) && Number.isFinite(Date.parse(v)));
const OPT_QUERY = {
  kind: (v: string) => /^(live|demo|all)$/.test(v),
  from: isDayOrIso,
  to: isDayOrIso,
  login: (v: string) => /^\d{1,18}$/.test(v),
  limit: (v: string) => /^\d{1,4}$/.test(v) && Number(v) >= 1 && Number(v) <= 1000,
  expiry: isDay,
  u: (v: string) => /^[A-Z0-9._-]{1,20}$/.test(v),
  status: (v: string) => /^(pending|approved|rejected|executed|expired|all)$/.test(v),
} as const;
type OptQuery = keyof typeof OPT_QUERY;

/** Option series code: SYMBOL-YYYYMMDD-STRIKE-C|P, plus an optional suffix (barrier / variant). */
const SERIES = "[A-Z0-9]{3,12}-\\d{8}-[0-9.]{1,16}-[CP](?:-[A-Z0-9._]{1,24})?";
const ID = "\\d{1,18}";
/** Order book fill id: {UNDERLYING}.{L|D}{seq}.{n} (e.g. EURUSD.L1042.0). */
const FILL = "[A-Z0-9]{2,12}\\.[LD]\\d{1,18}\\.\\d{1,4}";

/** Body checks on top of the reason (the engine validates again). */
const kindIs = (b: Record<string, unknown>, all = false) => (typeof b.kind === "string" && (all ? /^(live|demo|all)$/ : /^(live|demo)$/).test(b.kind) ? null : `kind must be live or demo${all ? " (or all)" : ""}.`);
const approvalOk = (b: Record<string, unknown>) => (b.approvalId === undefined || b.approvalId === null || (typeof b.approvalId === "number" && Number.isInteger(b.approvalId) && b.approvalId > 0) || (typeof b.approvalId === "string" && /^[A-Za-z0-9_-]{1,64}$/.test(b.approvalId)) ? null : "approvalId is not valid.");
const scopeIs = (b: Record<string, unknown>, scopes: readonly string[]) =>
  typeof b.scope !== "string" || !scopes.includes(b.scope) ? `scope must be ${scopes.join(", ")}.` : b.scope !== "all" && (typeof b.target !== "string" || !/^[A-Za-z0-9:._-]{1,64}$/.test(b.target)) ? "target is required for this scope." : null;

type OptRoute = { method: Method; re: RegExp; perm: OptionsPerm; query?: readonly OptQuery[]; check?: (b: Record<string, unknown>) => string | null };

/** FX Options on the engine (the option book, order books, market maker, clearing, settlement re-runs, voids,
 *  busts, rollout); checked with lib/options-perms.ts. The options reference data (series, surfaces, controls,
 *  fixings, MM settings) lives in the options service: /api/options. */
const OPTION_ROUTES: OptRoute[] = [
  { method: "GET", re: /^admin\/options\/book$/, perm: "options.read", query: ["kind"] },
  // {expiry}: the expiry key SYMBOL:YYYY-MM-DD
  { method: "POST", re: /^admin\/options\/settlements\/[A-Za-z0-9:._-]{1,64}\/rerun$/, perm: "options.settle" },
  { method: "POST", re: new RegExp(`^admin/options/trades/${T}/void$`), perm: "options.dealing" },
  // market maker: status, pause / resume
  { method: "GET", re: /^admin\/options\/mm$/, perm: "options.read", query: ["kind"] },
  { method: "POST", re: /^admin\/options\/mm\/(pause|resume)$/, perm: "options.dealing", check: (b) => kindIs(b) ?? scopeIs(b, ["all", "underlying", "expiry"]) },
  // order books: monitor, depth with owners (the engine audits the view), halts
  { method: "GET", re: /^admin\/options\/books$/, perm: "options.read", query: ["kind"] },
  { method: "POST", re: /^admin\/options\/books\/halt$/, perm: "options.dealing", check: (b) => kindIs(b) ?? scopeIs(b, ["all", "underlying", "expiry", "series"]) ?? (b.mode === "halt" || b.mode === "cancel_only" ? null : "mode must be halt or cancel_only.") },
  { method: "DELETE", re: new RegExp(`^admin/options/books/halt/${ID}$`), perm: "options.dealing" },
  { method: "GET", re: new RegExp(`^admin/options/books/${SERIES}$`), perm: "options.dealing", query: ["kind"] },
  // liquidation log, clearing accounts
  { method: "GET", re: /^admin\/options\/liquidations$/, perm: "options.read", query: ["kind", "from", "to", "login", "limit"] },
  { method: "GET", re: /^admin\/options\/clearing$/, perm: "options.read", query: ["kind", "expiry", "u"] },
  // four-eyes: bust a fill, enable the book; pending approvals
  { method: "POST", re: new RegExp(`^admin/options/fills/${FILL}/bust$`), perm: "options.settle", check: approvalOk },
  { method: "GET", re: /^admin\/options\/approvals$/, perm: "options.read", query: ["status", "kind"] },
  // combo RFQs: open requests with the market maker's live quote, and the recent ones
  { method: "GET", re: /^admin\/options\/rfqs$/, perm: "options.read", query: ["kind"] },
  { method: "GET", re: /^admin\/options\/book\/enable\/plan$/, perm: "options.read", query: ["kind"] },
  { method: "POST", re: /^admin\/options\/book\/enable$/, perm: "options.settle", check: (b) => kindIs(b) ?? approvalOk(b) },
];

const json = (data: unknown, status = 200) => NextResponse.json(data, { status, headers: { "cache-control": "no-store" } });

async function handle(req: NextRequest, parts: string[], method: Method) {
  const path = parts.map((p) => decodeURIComponent(p)).join("/");
  if (!tradingConfigured()) return apiError(503, "not_configured", "The trading engine is not configured for the Back Office.");

  // special routes first
  if (method === "POST" && path === "dealing/stream-ticket") return streamTicket(req);
  if (method === "GET" && path === "summary") return summary(req);
  if (method === "GET" && path === "clients") return clientNames(req);
  const hist = method === "GET" ? path.match(new RegExp(`^accounts/${T}/(history|ledger)$`)) : null;
  if (hist) return accountStatement(req, hist[1]!, hist[2]!);

  const optRoute = OPTION_ROUTES.find((r) => r.method === method && r.re.test(path));
  if (optRoute) return optionsRoute(req, path, method, optRoute);

  const route = ROUTES.find((r) => r.method === method && r.re.test(path));
  if (!route) return apiError(404, "not_found", "Not found.");

  let body: unknown;
  if (method !== "GET") {
    const blocked = mutationAllowed(req);
    if (blocked) return blocked;
    body = await req.json().catch(() => null);
    if (body === null || typeof body !== "object" || Array.isArray(body)) return apiError(400, "bad_request", "Invalid request body.");
  }
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  const perms: readonly TradingPerm[] = typeof route.perm === "string" ? [route.perm] : route.perm;
  if (!perms.some((p) => tradingAllows(who.staff, p))) return apiError(403, "forbidden", "Your role doesn't allow this.");
  // balance and credit changes go through "Balance & credit" (wallet service: limits, four-eyes, client
  // notification); the direct engine route stays for bonus only
  if (/^admin\/accounts\/\d+\/balance$/.test(path) && (body as { type?: unknown }).type !== "bonus")
    return apiError(422, "use_adjustments", "Balance and credit changes are made with Balance & credit (four-eyes, limits and client notice apply).");

  const target = route.to ? route.to(path.match(route.re)!) : `/v1/${path}${method === "GET" ? req.nextUrl.search : ""}`;
  const r = await engine(target, { method, body, staff: who.staff, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
  // risk desk routes on an engine without them answer its generic 404: say so instead of "Not found"
  if (r.status === 404 && RISK_PATH.test(path)) {
    const err = (r.data as { error?: { code?: string; message?: string } } | null)?.error;
    if (!err || (err.code === "not_found" && err.message === "Not found.")) return apiError(404, "engine_pending", "Available after the engine update.");
  }
  return json(r.data, r.status);
}

/** Risk desk paths (exposure, risk limits / alerts / scores / news, routing suggestions). */
const RISK_PATH = /^dealing\/(exposure|risk\/|routing\/suggestions)/;

/**
 * Option routes on the engine. Writes carry a reason (`reason`, or `reasonCode` + `note` for a void); DELETE takes
 * it from the JSON body (CSRF: JSON only) and passes it on both in the body and as `?reason=`. GETs forward only the
 * route's allow-listed query parameters, validated.
 */
async function optionsRoute(req: NextRequest, path: string, method: Method, route: OptRoute) {
  let body: unknown;
  const q = new URLSearchParams();
  if (method !== "GET") {
    const blocked = mutationAllowed(req);
    if (blocked) return blocked;
    body = await req.json().catch(() => null);
    if (body === null || typeof body !== "object" || Array.isArray(body)) return apiError(400, "bad_request", "Invalid request body.");
    const b = body as Record<string, unknown>;
    const reason = typeof b.reason === "string" ? b.reason.trim() : typeof b.reasonCode === "string" ? b.reasonCode.trim() : "";
    if (reason.length < 3) return apiError(422, "validation", "Add a reason for the audit log.");
    if (reason.length > 500) return apiError(422, "validation", "Keep the reason under 500 characters.");
    const problem = route.check?.(b);
    if (problem) return apiError(422, "validation", problem);
    if (method === "DELETE") q.set("reason", reason);
  } else {
    for (const k of route.query ?? []) {
      const v = req.nextUrl.searchParams.get(k);
      if (v === null || v === "") continue;
      if (!OPT_QUERY[k](v)) return apiError(400, "bad_request", `Invalid ${k}.`);
      q.set(k, v);
    }
  }
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!optionsAllows(who.staff, route.perm)) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const qs = q.toString() ? `?${q}` : "";
  const r = await engine(`/v1/${path}${qs}`, { method, body, staff: who.staff, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
  // an engine without the option routes answers its generic 404 (or an empty one): say so instead of "Not found"
  const err = (r.data as { error?: { code?: string; message?: string } } | null)?.error;
  if (r.status === 404 && (!err || (err.code === "not_found" && err.message === "Not found.")))
    return apiError(404, "engine_pending", "Available after the engine update.");
  return json(r.data, r.status);
}

/** One-time ticket for the dealing WebSocket; the browser connects to the engine directly with it. */
async function streamTicket(req: NextRequest) {
  const blocked = mutationAllowed(req);
  if (blocked) return blocked;
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!tradingAllows(who.staff, "dealing.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const r = await engine<{ ticket?: string; expiresIn?: number }>("/v1/dealing/stream-ticket", { method: "POST", body: {}, staff: who.staff, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
  if (r.status !== 200 || !r.data?.ticket) return json(r.data, r.status === 200 ? 502 : r.status);
  return json({ ticket: r.data.ticket, expiresIn: r.data.expiresIn ?? 30, url: TRADING_STREAM_URL || null });
}

/** Client statement (closing deals) and ledger of one account. The engine scopes /v1/accounts/{login}/* to the owner, so the owner's user id is looked up first. */
async function accountStatement(req: NextRequest, login: string, kind: string) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!tradingAllows(who.staff, "accounts.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const fwd = { staff: who.staff, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") };
  const acc = await engine<{ account?: { userId: number } }>(`/v1/admin/accounts/${login}`, fwd);
  if (acc.status !== 200 || !acc.data?.account) return json(acc.data, acc.status);
  const r = await engine(`/v1/accounts/${login}/${kind}${req.nextUrl.search}`, { ...fwd, userId: acc.data.account.userId });
  return json(r.data, r.status);
}

/* ---------------- client names (gateway) ---------------- */

const NAME_TTL = 5 * 60_000;
const names = new Map<string, { name: string; email: string; at: number }>();

/** `?ids=1,2,3` → `{ names: { "1": { name, email } } }` from the gateway (needs clients.read). */
async function clientNames(req: NextRequest) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!tradingAllows(who.staff, "accounts.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const ids = Array.from(new Set((req.nextUrl.searchParams.get("ids") ?? "").split(",").filter((x) => /^\d{1,18}$/.test(x)))).slice(0, 60);
  const out: Record<string, { name: string; email: string }> = {};
  if (!who.staff.permissions?.includes("clients.read")) return json({ names: out });
  const now = Date.now();
  await Promise.all(
    ids.map(async (id) => {
      const hit = names.get(id);
      if (hit && now - hit.at < NAME_TTL) {
        out[id] = { name: hit.name, email: hit.email };
        return;
      }
      const r = await gateway<{ user?: { name: string; email: string } }>(`/v1/admin/users/${id}`, { token: who.token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
      if (r.status === 200 && r.data.user) {
        names.set(id, { name: r.data.user.name, email: r.data.user.email, at: now });
        out[id] = { name: r.data.user.name, email: r.data.user.email };
      }
    }),
  );
  if (names.size > 5000) names.clear();
  return json({ names: out });
}

/* ---------------- Command Center KPIs ---------------- */

type SumPos = { route: "A" | "B"; volume: number; profit: number | null; swap: number; commission: number; currency?: string; login: string };
type SumDeal = { profit: number; book: "A" | "B"; currency?: string; login: string };

/** Server day start (00:00 broker time: GMT+3 while US DST is active, else GMT+2) as RFC 3339 UTC. */
function serverDayStart(now = Date.now()) {
  const y = new Date(now).getUTCFullYear();
  const nthSunday = (month: number, n: number) => {
    const first = new Date(Date.UTC(y, month, 1));
    return Date.UTC(y, month, 1 + ((7 - first.getUTCDay()) % 7) + (n - 1) * 7);
  };
  const dst = now >= nthSunday(2, 2) + 7 * 3600_000 && now < nthSunday(10, 1) + 6 * 3600_000;
  const off = (dst ? 3 : 2) * 3600_000;
  const local = new Date(now + off);
  return new Date(Date.UTC(local.getUTCFullYear(), local.getUTCMonth(), local.getUTCDate()) - off).toISOString();
}

async function summary(req: NextRequest) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!tradingAllows(who.staff, "dealing.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const fwd = { staff: who.staff, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") };
  const from = serverDayStart();
  const [live, demo, positions, deals, accounts] = await Promise.all([
    engine<{ total: number }>("/v1/admin/accounts?type=live&limit=1", fwd),
    engine<{ total: number }>("/v1/admin/accounts?type=demo&limit=1", fwd),
    engine<SumPos[]>("/v1/dealing/positions", fwd),
    engine<SumDeal[]>(`/v1/dealing/deals?from=${encodeURIComponent(from)}&limit=2000`, fwd),
    engine<{ items: { login: number; type: string }[] }>("/v1/admin/accounts?limit=500", fwd),
  ]);
  if (positions.status !== 200 || !Array.isArray(positions.data)) return json(positions.data ?? { error: { code: "unavailable", message: "The trading engine is unavailable." } }, positions.status === 200 ? 502 : positions.status);
  const kind = new Map((accounts.data?.items ?? []).map((a) => [String(a.login), a.type]));
  const usd = (v: number, ccy?: string) => (ccy === "USC" ? v / 100 : v);
  const book = { A: { positions: 0, lots: 0, floating: 0 }, B: { positions: 0, lots: 0, floating: 0 } };
  let demoPositions = 0;
  for (const p of positions.data) {
    if (kind.get(p.login) === "demo") demoPositions++;
    const b = book[p.route] ?? book.B;
    b.positions++;
    b.lots += p.volume;
    b.floating += usd((p.profit ?? 0) + p.swap - p.commission, p.currency);
  }
  const dl = Array.isArray(deals.data) ? deals.data : [];
  const realised = dl.reduce((s, d) => s + usd(d.profit, d.currency), 0);
  const brokerB = -dl.filter((d) => d.book === "B").reduce((s, d) => s + usd(d.profit, d.currency), 0);
  return json({
    accounts: { live: live.data?.total ?? 0, demo: demo.data?.total ?? 0 },
    positions: { total: positions.data.length, demo: demoPositions, A: book.A, B: book.B },
    deals: { today: dl.length, clientRealised: +realised.toFixed(2), brokerBRealised: +brokerB.toFixed(2), since: from },
    generatedAt: new Date().toISOString(),
  });
}

type Ctx = { params: Promise<{ path: string[] }> };
export async function GET(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "GET");
}
export async function POST(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "POST");
}
export async function PUT(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "PUT");
}
export async function PATCH(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "PATCH");
}
export async function DELETE(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "DELETE");
}

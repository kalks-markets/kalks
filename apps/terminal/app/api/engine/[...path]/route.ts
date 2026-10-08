import type { NextRequest, NextResponse } from "next/server";
import { clientAccount, csrf, engine, error, readSessions, reply, sessionFor, soft, streamUrl, writeSessions, type EngineSession, type Obj } from "@/lib/engine/server";
import { moduleGate } from "@/lib/tenant-brand";

// Kalks Trader trading BFF. Browser -> /api/engine/<route> (same origin, `X-Kalks-Login: <login>` picks which
// of this browser's sessions acts) -> engine /v1/terminal/<route> with that session's bearer token + the
// internal token. Neither token reaches the browser. CSRF: the cookie is SameSite=Lax and every write must
// be a same-origin JSON request. Investor sessions are read-only in the engine (403 read_only).
//
//   GET    state?historyLimit=              account, positions, orders, recent deals
//   GET    history?from&to&page&limit       closed deals + done pending orders
//   POST   orders                           market / pending order (fields below)
//   PATCH  orders/{ticket}                  {price?, stopLimit?, volume?, sl?, tp?, trailingPoints?, expiry?, expiryAt?}
//   DELETE orders/{ticket}
//   POST   positions/{ticket}/close         {volume?, deviationPoints?, requestedPrice?}
//   PATCH  positions/{ticket}               {sl?, tp?, trailingPoints?}  (null clears)
//   POST   positions/close-by               {ticket, by}
//   POST   bulk-close                       {filter: all|profitable|losing|pending|buys|sells, symbol?}
//   POST   stream-ticket                    {ticket, expiresIn, url}: one-time WebSocket ticket (30 s)
//   POST   demo-refill                      demo accounts: top the balance back up (Client Area API, owner resolved here)
//   GET    mam?symbol&volume                MAM role of the account + allocation summary (manager) / managing programme (client)
//
// Kalks FX Options (same account, same session; investor sessions are read-only):
//   POST   options/preview                  {legs:[{series, side, contracts}], type, limitPremium?} -> margin / P&L preview
//   POST   options/orders                   {legs, type, limitPremium?, sl?, tp?, trigger?:{symbol, op, price}, tif?, clientOrderId}
//   POST   options/combos/{comboId}/close   close every leg of a strategy at once (all-or-nothing)
//   GET    options/settlements?limit        expiry settlements (fixing, payout, run)
//   (closing one option position, also partially: positions/{ticket}/close {volume}; on a book-venue position the
//   engine closes it reduce-only at market through the book: {status: filled|partial, filled, avgPrice, left})
//
// Kalks FX Options order book (docs/OPTIONS-EXCHANGE.md §2, §5, §12; prices per unit in the quote currency):
//   POST   options/book/preview             same body as an order (clientOrderId optional) -> reserve, est. avg price, fee
//   POST   options/book/orders              {series, side, type: limit|market|stop_market|stop_limit, qty, price?,
//                                            tif: gtc|ioc|fok|gtd, expireAt?, postOnly?, reduceOnly?,
//                                            trigger?: {source: mark|underlying, op: above|below, price}, clientOrderId}
//   GET    options/book/orders?status=open|history&series=
//   PATCH  options/book/orders/{id}         {price?, qty?}
//   DELETE options/book/orders/{id}
//   DELETE options/book/orders?series=&underlying=   cancel all of a series or an underlying
//   GET    options/book/fills?from&to
//   POST   options/rfq                      {legs:[{series, side, ratio}], qty, reduceOnly?} -> {rfq}
//   GET    options/rfq/{id}                 {rfq, quotes:[{quoteId, responder, bid, ask, qty, validUntil}]}
//   POST   options/rfq/{id}/accept          {quoteId, side, limitNet}
//   DELETE options/rfq/{id}
//   (deadman and mass-quote are for market-maker programme accounts over the API, not the browser: not forwarded)

type Ctx = { params: Promise<{ path: string[] }> };

const TICKET_RE = /^\d{1,12}$/;
const DATE_RE = /^\d{4}-\d{2}-\d{2}(T[\d:.]+(Z|[+-]\d{2}:\d{2})?)?$/;
const SYMBOL_RE = /^[A-Z0-9._]{2,20}$/;
/** Option series code, e.g. EURUSD-20261009-1.1650-C (barrier series may carry a suffix). */
const SERIES_RE = /^[A-Z0-9]{3,12}-\d{8}-[0-9.]{1,16}-[CP](-[A-Z0-9._]{1,24})?$/;
const COMBO_RE = /^[A-Za-z0-9_-]{1,64}$/;
const MAX_LEGS = 8;
/** order book order id (= engine ticket) and RFQ id */
const ORDER_ID_RE = /^\d{1,18}$/;
const RFQ_ID_RE = /^[A-Za-z0-9_.:-]{1,64}$/;
const UNDERLYING_RE = /^[A-Z0-9]{3,12}$/;
const CLIENT_ORDER_RE = /^[A-Za-z0-9_-]{8,64}$/;
const BOOK_TYPES = ["limit", "market", "stop_market", "stop_limit"];
const BOOK_TIFS = ["gtc", "ioc", "fok", "gtd"];
const MAX_QTY = 100_000;

/** Dealing details never reach the browser (book, routing, ledger ids, dealer controls). */
const HIDDEN = new Set(["book", "route", "userId", "ledgerTxn", "parentTicket", "childTickets", "priceCorrected", "version", "tenantId"]);
function scrub(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(scrub);
  if (!v || typeof v !== "object") return v;
  const out: Obj = {};
  for (const [k, x] of Object.entries(v as Obj)) {
    if (HIDDEN.has(k)) continue;
    if (k === "controls" && x && typeof x === "object") {
      const c = x as Obj;
      out[k] = { tradingDisabled: !!c.tradingDisabled, closeOnly: !!c.closeOnly, maxLot: c.maxLot ?? null };
    } else out[k] = scrub(x);
  }
  return out;
}

const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) ? v : typeof v === "string" && v.trim() !== "" && Number.isFinite(Number(v)) ? Number(v) : undefined);
/** number, or null (explicit clear), or undefined (not sent) */
const numOrNull = (v: unknown) => (v === null ? null : num(v));

function pick(src: Obj, spec: Record<string, "num" | "numOrNull" | "str" | "int">): Obj | string {
  const out: Obj = {};
  for (const [k, kind] of Object.entries(spec)) {
    if (!(k in src) || src[k] === undefined) continue;
    const v = src[k];
    if (kind === "str") {
      if (typeof v !== "string" || v.length > 64) return k;
      out[k] = v;
    } else if (kind === "numOrNull") {
      const n = numOrNull(v);
      if (n === undefined) return k;
      out[k] = n;
    } else {
      const n = num(v);
      if (n === undefined || (kind === "int" && !Number.isInteger(n))) return k;
      out[k] = n;
    }
  }
  return out;
}

function orderBody(b: Obj): Obj | NextResponse {
  const o = pick(b, { symbol: "str", side: "str", type: "str", volume: "num", price: "num", stopLimit: "num", sl: "num", tp: "num", trailingPoints: "int", expiry: "str", expiryAt: "str", requestedPrice: "num", deviationPoints: "int", ocoWith: "int", comment: "str", clientOrderId: "str", source: "str" });
  if (typeof o === "string") return error(422, "validation", `Invalid ${o}.`);
  if (!SYMBOL_RE.test(String(o.symbol ?? ""))) return error(422, "validation", "Invalid symbol.");
  if (o.side !== "buy" && o.side !== "sell") return error(422, "validation", "Invalid side.");
  if (!["market", "limit", "stop", "stop_limit"].includes(String(o.type))) return error(422, "validation", "Invalid order type.");
  // a browser may tag its own orders as manual or AI Trader only (never api / copy / dealer …)
  o.source = o.source === "ai" ? "ai" : "manual";
  o.platform = "Web";
  if (typeof o.comment === "string") o.comment = o.comment.slice(0, 31);
  return o;
}

/** Option legs + order type of a preview / order (the engine re-checks everything; this keeps junk out). */
function optionBody(b: Obj, order: boolean): Obj | NextResponse {
  if (!Array.isArray(b.legs) || b.legs.length < 1 || b.legs.length > MAX_LEGS) return error(422, "validation", `Give 1 to ${MAX_LEGS} legs.`);
  const legs: Obj[] = [];
  for (const raw of b.legs as unknown[]) {
    const l = (raw ?? {}) as Obj;
    const contracts = num(l.contracts);
    if (typeof l.series !== "string" || !SERIES_RE.test(l.series)) return error(422, "validation", "Invalid series.");
    if (l.side !== "buy" && l.side !== "sell") return error(422, "validation", "Invalid side.");
    if (contracts === undefined || contracts <= 0 || contracts > 100_000) return error(422, "validation", "Invalid contracts.");
    const leg: Obj = { series: l.series, side: l.side, contracts };
    // a barrier leg (Kalks-quoted, house ticket): {kind: UO|DO|UI|DI, level, rebate?}
    if (l.barrier !== undefined && l.barrier !== null) {
      const x = l.barrier as Obj;
      const level = num(x.level);
      const rebate = x.rebate === undefined || x.rebate === null ? undefined : num(x.rebate);
      if (!["UO", "DO", "UI", "DI"].includes(String(x.kind)) || level === undefined || level <= 0 || level > 1e9 || (x.rebate !== undefined && x.rebate !== null && (rebate === undefined || rebate < 0))) return error(422, "validation", "Invalid barrier.");
      leg.barrier = { kind: x.kind, level, ...(rebate !== undefined ? { rebate } : {}) };
    }
    legs.push(leg);
  }
  if (new Set(legs.map((l) => l.series)).size !== legs.length) return error(422, "validation", "Each series may appear once.");
  const type = b.type === "limit" ? "limit" : b.type === "market" || b.type === undefined ? "market" : null;
  if (!type) return error(422, "validation", "Invalid order type.");
  const out: Obj = { legs, type };
  if (type === "limit") {
    const lp = num(b.limitPremium);
    if (lp === undefined || lp < 0) return error(422, "validation", "Invalid limit premium.");
    out.limitPremium = lp;
  }
  if (!order) return out;
  for (const k of ["sl", "tp"] as const) {
    if (b[k] === undefined || b[k] === null) continue;
    const v = num(b[k]);
    if (v === undefined || v < 0) return error(422, "validation", `Invalid ${k}.`);
    out[k] = v;
  }
  if (b.trigger !== undefined && b.trigger !== null) {
    const t = b.trigger as Obj;
    const price = num(t.price);
    if (typeof t.symbol !== "string" || !SYMBOL_RE.test(t.symbol) || (t.op !== "above" && t.op !== "below") || price === undefined || price <= 0) return error(422, "validation", "Invalid trigger.");
    out.trigger = { symbol: t.symbol, op: t.op, price };
  }
  if (b.tif !== undefined) {
    if (b.tif !== "gtc" && b.tif !== "day") return error(422, "validation", "Invalid time in force.");
    out.tif = b.tif;
  }
  if (typeof b.clientOrderId !== "string" || !/^[A-Za-z0-9_-]{8,64}$/.test(b.clientOrderId)) return error(422, "validation", "Invalid clientOrderId.");
  out.clientOrderId = b.clientOrderId;
  return out;
}

/** A book order / preview (§2): only the documented fields, each checked; the engine re-checks everything. */
function bookOrderBody(b: Obj, order: boolean): Obj | NextResponse {
  const bad = (m: string) => error(422, "validation", m);
  if (typeof b.series !== "string" || !SERIES_RE.test(b.series)) return bad("Invalid series.");
  if (b.side !== "buy" && b.side !== "sell") return bad("Invalid side.");
  const type = String(b.type ?? "");
  if (!BOOK_TYPES.includes(type)) return bad("Invalid order type.");
  const qty = num(b.qty);
  if (qty === undefined || qty <= 0 || qty > MAX_QTY) return bad("Invalid quantity.");
  const out: Obj = { series: b.series, side: b.side, type, qty };
  const priced = type === "limit" || type === "stop_limit";
  if (priced) {
    const price = num(b.price);
    if (price === undefined || price <= 0 || price > 1e9) return bad("Invalid price.");
    out.price = price;
  }
  // market orders are IOC at the band (the engine stamps the band); a stop-market fires as one
  let tif = String(b.tif ?? (priced ? "gtc" : "ioc"));
  if (!BOOK_TIFS.includes(tif)) return bad("Invalid time in force.");
  if (type === "market" || type === "stop_market") tif = "ioc";
  out.tif = tif;
  if (tif === "gtd") {
    if (typeof b.expireAt !== "string" || !DATE_RE.test(b.expireAt) || !(Date.parse(b.expireAt) > Date.now())) return bad("Invalid expiry time.");
    out.expireAt = b.expireAt;
  }
  if (b.postOnly !== undefined && b.postOnly !== null) {
    if (typeof b.postOnly !== "boolean") return bad("Invalid postOnly.");
    if (b.postOnly && (type !== "limit" || tif !== "gtc")) return bad("Post-only needs a GTC limit order.");
    if (b.postOnly) out.postOnly = true;
  }
  if (b.reduceOnly !== undefined && b.reduceOnly !== null) {
    if (typeof b.reduceOnly !== "boolean") return bad("Invalid reduceOnly.");
    if (b.reduceOnly) out.reduceOnly = true;
  }
  if (type === "stop_market" || type === "stop_limit") {
    const t = (b.trigger ?? {}) as Obj;
    const price = num(t.price);
    if ((t.source !== "mark" && t.source !== "underlying") || (t.op !== "above" && t.op !== "below") || price === undefined || price <= 0 || price > 1e9) return bad("Invalid trigger.");
    out.trigger = { source: t.source, op: t.op, price };
  }
  if (order || b.clientOrderId !== undefined) {
    if (typeof b.clientOrderId !== "string" || !CLIENT_ORDER_RE.test(b.clientOrderId)) return bad("Invalid clientOrderId.");
    out.clientOrderId = b.clientOrderId;
  }
  return out;
}

/** A combo RFQ (§5): 1–8 legs, one per series, whole ratios, a positive size. */
function rfqBody(b: Obj): Obj | NextResponse {
  const bad = (m: string) => error(422, "validation", m);
  if (!Array.isArray(b.legs) || b.legs.length < 1 || b.legs.length > MAX_LEGS) return bad(`Give 1 to ${MAX_LEGS} legs.`);
  const legs: Obj[] = [];
  for (const raw of b.legs as unknown[]) {
    const l = (raw ?? {}) as Obj;
    const ratio = num(l.ratio);
    if (typeof l.series !== "string" || !SERIES_RE.test(l.series)) return bad("Invalid series.");
    if (l.side !== "buy" && l.side !== "sell") return bad("Invalid side.");
    if (ratio === undefined || !Number.isInteger(ratio) || ratio < 1 || ratio > 100) return bad("Invalid ratio.");
    legs.push({ series: l.series, side: l.side, ratio });
  }
  if (new Set(legs.map((l) => l.series)).size !== legs.length) return bad("Each series may appear once.");
  const qty = num(b.qty);
  if (qty === undefined || qty <= 0 || qty > MAX_QTY) return bad("Invalid quantity.");
  return { legs, qty, ...(b.reduceOnly === true ? { reduceOnly: true } : {}) };
}

function pageQuery(req: NextRequest): string | NextResponse {
  const sp = req.nextUrl.searchParams;
  const out = new URLSearchParams();
  for (const k of ["from", "to"] as const) {
    const v = sp.get(k);
    if (!v) continue;
    if (!DATE_RE.test(v)) return error(400, "bad_request", `Invalid ${k} date.`);
    out.set(k, v);
  }
  for (const [k, max] of [["page", 100000], ["limit", 500]] as const) {
    const v = sp.get(k);
    if (!v) continue;
    const n = Number(v);
    if (!Number.isInteger(n) || n < 1 || n > max) return error(400, "bad_request", `Invalid ${k}.`);
    out.set(k, String(n));
  }
  const s = out.toString();
  return s ? `?${s}` : "";
}

/** Resolve the acting session; a session the engine no longer knows is dropped from the cookie. */
async function forward(req: NextRequest, s: EngineSession, path: string, init: { method?: "GET" | "POST" | "PATCH" | "DELETE"; body?: unknown } = {}): Promise<{ status: number; data: Obj; expired?: NextResponse }> {
  const r = await engine<Obj>(path, { ...init, bearer: s.t, req });
  if (r.status === 401) {
    const res = reply(401, { error: { code: "session_expired", message: "Your trading session has expired. Log in again." } });
    return { ...r, expired: writeSessions(req, res, readSessions(req).filter((x) => x.l !== s.l)) };
  }
  return r;
}

async function handle(req: NextRequest, { params }: Ctx, method: "GET" | "POST" | "PATCH" | "DELETE") {
  const path = (await params).path;
  if (method !== "GET") {
    const blocked = csrf(req, method !== "DELETE");
    if (blocked) return blocked;
  }
  const s = sessionFor(req);
  if (!s) return error(401, "unauthorized", "Log in to your trading account.");
  const [a, b, c] = path;
  const body = method === "POST" || method === "PATCH" ? (((await req.json().catch(() => null)) ?? {}) as Obj) : {};
  if (typeof body !== "object" || Array.isArray(body)) return error(400, "bad_request", "Invalid request body.");

  const done = async (r: { status: number; data: Obj; expired?: NextResponse }, map?: (d: Obj) => Promise<Obj> | Obj) => {
    if (r.expired) return r.expired;
    return reply(r.status, r.status < 300 && map ? await map(r.data) : scrub(r.data));
  };

  // ---- reads
  if (method === "GET" && a === "state" && path.length === 1) {
    const n = Number(req.nextUrl.searchParams.get("historyLimit") ?? 100);
    const limit = Number.isInteger(n) && n >= 0 && n <= 500 ? n : 100;
    return done(await forward(req, s, `/v1/terminal/state?historyLimit=${limit}`), async (d) => ({ ...(scrub(d) as Obj), account: await clientAccount(d.account) }));
  }
  if (method === "GET" && a === "mam" && path.length === 1) {
    const off = await moduleGate(req.headers, "mam");
    if (off) return off;
    const sp = req.nextUrl.searchParams;
    const symbol = sp.get("symbol");
    const volume = sp.get("volume") ?? "1";
    if (symbol !== null && !SYMBOL_RE.test(symbol)) return error(400, "bad_request", "Invalid symbol.");
    if (!/^\d{1,4}(\.\d{1,2})?$/.test(volume) || Number(volume) <= 0) return error(400, "bad_request", "Invalid volume.");
    return done(await forward(req, s, `/v1/terminal/mam${symbol ? `?symbol=${symbol}&volume=${volume}` : ""}`));
  }
  if (method === "GET" && a === "history" && path.length === 1) {
    const q = pageQuery(req);
    if (typeof q !== "string") return q;
    return done(await forward(req, s, `/v1/terminal/history${q}`));
  }

  // ---- writes
  if (a === "orders" && path.length === 1 && method === "POST") {
    const o = orderBody(body);
    if (o instanceof Response) return o;
    return done(await forward(req, s, "/v1/terminal/orders", { method: "POST", body: o }));
  }
  if (a === "orders" && path.length === 2 && TICKET_RE.test(b ?? "")) {
    if (method === "DELETE") return done(await forward(req, s, `/v1/terminal/orders/${b}`, { method: "DELETE" }));
    if (method === "PATCH") {
      const o = pick(body, { price: "num", stopLimit: "num", volume: "num", sl: "numOrNull", tp: "numOrNull", trailingPoints: "numOrNull", expiry: "str", expiryAt: "str" });
      if (typeof o === "string") return error(422, "validation", `Invalid ${o}.`);
      return done(await forward(req, s, `/v1/terminal/orders/${b}`, { method: "PATCH", body: o }));
    }
  }
  if (a === "positions" && path.length === 2 && b === "close-by" && method === "POST") {
    const o = pick(body, { ticket: "int", by: "int" });
    if (typeof o === "string" || o.ticket === undefined || o.by === undefined) return error(422, "validation", "Choose two positions.");
    return done(await forward(req, s, "/v1/terminal/positions/close-by", { method: "POST", body: o }));
  }
  if (a === "positions" && TICKET_RE.test(b ?? "")) {
    if (path.length === 3 && c === "close" && method === "POST") {
      const o = pick(body, { volume: "num", deviationPoints: "int", requestedPrice: "num" });
      if (typeof o === "string") return error(422, "validation", `Invalid ${o}.`);
      return done(await forward(req, s, `/v1/terminal/positions/${b}/close`, { method: "POST", body: o }));
    }
    if (path.length === 2 && method === "PATCH") {
      const o = pick(body, { sl: "numOrNull", tp: "numOrNull", trailingPoints: "numOrNull" });
      if (typeof o === "string") return error(422, "validation", `Invalid ${o}.`);
      return done(await forward(req, s, `/v1/terminal/positions/${b}`, { method: "PATCH", body: o }));
    }
  }
  if (a === "bulk-close" && path.length === 1 && method === "POST") {
    const filter = String(body.filter ?? "");
    if (!["all", "profitable", "losing", "pending", "buys", "sells"].includes(filter)) return error(422, "validation", "Invalid filter.");
    const symbol = body.symbol === undefined ? undefined : String(body.symbol);
    if (symbol !== undefined && !SYMBOL_RE.test(symbol)) return error(422, "validation", "Invalid symbol.");
    return done(await forward(req, s, "/v1/terminal/bulk-close", { method: "POST", body: { filter, ...(symbol ? { symbol } : {}) } }));
  }
  if (a === "stream-ticket" && path.length === 1 && method === "POST") {
    return done(await forward(req, s, "/v1/terminal/stream-ticket", { method: "POST" }), (d) => ({ ticket: d.ticket, expiresIn: d.expiresIn, url: streamUrl(req) }));
  }
  if (a === "demo-refill" && path.length === 1 && method === "POST") {
    if (s.r) return error(403, "read_only", "Investor (read-only) sessions can't refill the balance.");
    // the owner comes from the engine's own view of this session, never from the browser
    const st = await forward(req, s, "/v1/terminal/state?historyLimit=0");
    if (st.expired) return st.expired;
    if (st.status !== 200) return reply(st.status, scrub(st.data));
    if (st.data.readOnly) return error(403, "read_only", "Investor (read-only) sessions can't refill the balance.");
    const acc = (st.data.account ?? {}) as Obj;
    const r = await engine<Obj>(`/v1/accounts/${s.l}/demo-refill`, { method: "POST", req, headers: { "x-kalks-user-id": String(acc.userId ?? "") } });
    return reply(r.status, scrub(r.data));
  }
  // ---- Kalks FX Options
  if (a === "options") {
    if (method === "POST" && b === "preview" && path.length === 2) {
      const o = optionBody(body, false);
      if (o instanceof Response) return o;
      return done(await forward(req, s, "/v1/terminal/options/preview", { method: "POST", body: o }));
    }
    if (method === "POST" && b === "orders" && path.length === 2) {
      if (s.r) return error(403, "read_only", "Trading is disabled with the investor password.");
      const o = optionBody(body, true);
      if (o instanceof Response) return o;
      return done(await forward(req, s, "/v1/terminal/options/orders", { method: "POST", body: o }));
    }
    if (method === "POST" && b === "combos" && path.length === 4 && COMBO_RE.test(c ?? "") && path[3] === "close") {
      if (s.r) return error(403, "read_only", "Trading is disabled with the investor password.");
      return done(await forward(req, s, `/v1/terminal/options/combos/${encodeURIComponent(c!)}/close`, { method: "POST" }));
    }
    if (method === "GET" && b === "settlements" && path.length === 2) {
      const n = Number(req.nextUrl.searchParams.get("limit") ?? 100);
      const limit = Number.isInteger(n) && n >= 1 && n <= 500 ? n : 100;
      return done(await forward(req, s, `/v1/terminal/options/settlements?limit=${limit}`));
    }
    // ---- order book
    const readOnly = () => error(403, "read_only", "Trading is disabled with the investor password.");
    if (b === "book") {
      const d = path[3];
      if (method === "POST" && c === "preview" && path.length === 3) {
        const o = bookOrderBody(body, false);
        if (o instanceof Response) return o;
        return done(await forward(req, s, "/v1/terminal/options/book/preview", { method: "POST", body: o }));
      }
      if (c === "orders" && path.length === 3) {
        if (method === "POST") {
          if (s.r) return readOnly();
          const o = bookOrderBody(body, true);
          if (o instanceof Response) return o;
          return done(await forward(req, s, "/v1/terminal/options/book/orders", { method: "POST", body: o }));
        }
        const sp = req.nextUrl.searchParams;
        const series = sp.get("series");
        if (series !== null && !SERIES_RE.test(series)) return error(400, "bad_request", "Invalid series.");
        if (method === "GET") {
          const status = sp.get("status") ?? "open";
          if (status !== "open" && status !== "history") return error(400, "bad_request", "Invalid status.");
          return done(await forward(req, s, `/v1/terminal/options/book/orders?status=${status}${series ? `&series=${encodeURIComponent(series)}` : ""}`));
        }
        if (method === "DELETE") {
          if (s.r) return readOnly();
          const underlying = sp.get("underlying");
          if (underlying !== null && !UNDERLYING_RE.test(underlying)) return error(400, "bad_request", "Invalid underlying.");
          if (!series && !underlying) return error(400, "bad_request", "Choose a series or an underlying.");
          const q = new URLSearchParams();
          if (series) q.set("series", series);
          if (underlying) q.set("underlying", underlying);
          return done(await forward(req, s, `/v1/terminal/options/book/orders?${q}`, { method: "DELETE" }));
        }
      }
      if (c === "orders" && path.length === 4 && ORDER_ID_RE.test(d ?? "")) {
        if (s.r) return readOnly();
        if (method === "DELETE") return done(await forward(req, s, `/v1/terminal/options/book/orders/${d}`, { method: "DELETE" }));
        if (method === "PATCH") {
          const price = body.price === undefined ? undefined : num(body.price);
          const qty = body.qty === undefined ? undefined : num(body.qty);
          if ((body.price !== undefined && (price === undefined || price <= 0)) || (body.qty !== undefined && (qty === undefined || qty <= 0 || qty > MAX_QTY))) return error(422, "validation", "Invalid price or quantity.");
          if (price === undefined && qty === undefined) return error(422, "validation", "Change the price or the quantity.");
          return done(await forward(req, s, `/v1/terminal/options/book/orders/${d}`, { method: "PATCH", body: { ...(price !== undefined ? { price } : {}), ...(qty !== undefined ? { qty } : {}) } }));
        }
      }
      if (method === "GET" && c === "fills" && path.length === 3) {
        const q = new URLSearchParams();
        for (const k of ["from", "to"] as const) {
          const v = req.nextUrl.searchParams.get(k);
          if (!v) continue;
          if (!DATE_RE.test(v)) return error(400, "bad_request", `Invalid ${k} date.`);
          q.set(k, v);
        }
        return done(await forward(req, s, `/v1/terminal/options/book/fills${q.toString() ? `?${q}` : ""}`));
      }
    }
    // ---- combo RFQ
    if (b === "rfq") {
      if (method === "POST" && path.length === 2) {
        if (s.r) return readOnly();
        const o = rfqBody(body);
        if (o instanceof Response) return o;
        return done(await forward(req, s, "/v1/terminal/options/rfq", { method: "POST", body: o }));
      }
      if (path.length >= 3 && RFQ_ID_RE.test(c ?? "")) {
        const id = encodeURIComponent(c!);
        if (method === "GET" && path.length === 3) return done(await forward(req, s, `/v1/terminal/options/rfq/${id}`));
        if (method === "DELETE" && path.length === 3) {
          if (s.r) return readOnly();
          return done(await forward(req, s, `/v1/terminal/options/rfq/${id}`, { method: "DELETE" }));
        }
        if (method === "POST" && path.length === 4 && path[3] === "accept") {
          if (s.r) return readOnly();
          const limitNet = num(body.limitNet);
          if (typeof body.quoteId !== "string" || !RFQ_ID_RE.test(body.quoteId) || (body.side !== "buy" && body.side !== "sell") || limitNet === undefined || Math.abs(limitNet) > 1e9) return error(422, "validation", "Invalid quote, side or limit.");
          return done(await forward(req, s, `/v1/terminal/options/rfq/${id}/accept`, { method: "POST", body: { quoteId: body.quoteId, side: body.side, limitNet } }));
        }
      }
    }
  }
  return error(404, "not_found", "Not found.");
}

export const dynamic = "force-dynamic";
export const GET = async (req: NextRequest, ctx: Ctx) => soft(req, await handle(req, ctx, "GET"));
export const POST = async (req: NextRequest, ctx: Ctx) => soft(req, await handle(req, ctx, "POST"));
export const PATCH = async (req: NextRequest, ctx: Ctx) => soft(req, await handle(req, ctx, "PATCH"));
export const DELETE = async (req: NextRequest, ctx: Ctx) => soft(req, await handle(req, ctx, "DELETE"));

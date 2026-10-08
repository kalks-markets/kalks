// Server-only helpers for the Kalks trading engine (services/trading, 127.0.0.1:8090).
// The browser never talks to the engine's HTTP API and never sees the internal token or a terminal
// session token: route handlers under /api/engine/* hold the engine session tokens in an HttpOnly cookie
// and forward each call with `Authorization: Bearer <session>` + `X-Kalks-Internal`.
// Contract: services/trading/README.md ("Terminal API", "Streams").

import { NextResponse, type NextRequest } from "next/server";
import { clientIp } from "@/lib/gateway";
import { tenantBrand } from "@/lib/tenant-brand";
import { hostOf } from "@/lib/tenant-host";

const TRADING_URL = (process.env.TRADING_URL ?? "http://127.0.0.1:8090").replace(/\/+$/, "");
const TRADING_TOKEN = process.env.TRADING_INTERNAL_TOKEN ?? "";

export type Obj = Record<string, unknown>;
export type EngineResult<T = Obj> = { status: number; data: T };

export const NO_STORE = { "cache-control": "no-store" };

export function reply(status: number, data: unknown, init?: { headers?: Record<string, string> }) {
  return NextResponse.json(data, { status, headers: { ...NO_STORE, ...init?.headers } });
}
export function error(status: number, code: string, message: string) {
  return reply(status, { error: { code, message } });
}

/** The broker of the visitor's host (gateway tenant_domains, as for the brand, cached 30 s); Kalks when unknown or
 *  when the gateway is unreachable. A terminal session belongs to its broker, so every engine call names it. */
export async function engineTenant(req?: NextRequest): Promise<string> {
  const b = await tenantBrand(req ? hostOf(req.headers) : undefined).catch(() => null);
  const slug = b?.slug?.toLowerCase();
  return slug && /^[a-z0-9-]{1,64}$/.test(slug) ? slug : "kalks";
}

/** One call to the engine. `bearer` = terminal session token (terminal routes). */
export async function engine<T = Obj>(
  path: string,
  init: { method?: "GET" | "POST" | "PATCH" | "DELETE"; body?: unknown; bearer?: string; req?: NextRequest; headers?: Record<string, string> } = {},
): Promise<EngineResult<T>> {
  const headers: Record<string, string> = { "x-kalks-internal": TRADING_TOKEN, "x-kalks-tenant": await engineTenant(init.req), ...init.headers };
  if (init.bearer) headers.authorization = `Bearer ${init.bearer}`;
  if (init.req) {
    headers["x-forwarded-for"] = clientIp(init.req.headers);
    const ua = init.req.headers.get("user-agent");
    if (ua) headers["user-agent"] = ua.slice(0, 400);
  }
  const method = init.method ?? (init.body !== undefined ? "POST" : "GET");
  // axum rejects `content-type: application/json` with an empty body, so writes always carry a JSON body
  const body = init.body !== undefined ? init.body : method === "GET" || method === "DELETE" ? undefined : {};
  if (body !== undefined) headers["content-type"] = "application/json";
  try {
    const res = await fetch(`${TRADING_URL}${path}`, { method, headers, body: body !== undefined ? JSON.stringify(body) : undefined, cache: "no-store" });
    const text = await res.text();
    let data: unknown = {};
    try {
      data = text ? JSON.parse(text) : {};
    } catch {
      data = { error: { code: res.status === 400 ? "bad_request" : "engine_error", message: text.slice(0, 200) || `Trade server error ${res.status}` } };
    }
    return { status: res.status, data: data as T };
  } catch {
    return { status: 503, data: { error: { code: "unavailable", message: "The trade server is unavailable. Please try again shortly." } } as T };
  }
}

/**
 * The terminal's own fetches send `X-Kalks-Errors: body`: a rejection (market closed, invalid stops,
 * wrong password…) is an expected answer, so it comes back as HTTP 200 with `error.status` instead of a
 * 4xx that browsers log as a console error. Other callers get the plain status codes.
 */
export async function soft(req: NextRequest, res: NextResponse): Promise<NextResponse> {
  if (res.status < 400 || req.headers.get("x-kalks-errors") !== "body") return res;
  const body = (await res.clone().json().catch(() => ({}))) as Obj;
  const err = (body.error ?? {}) as Obj;
  const out = NextResponse.json({ ...body, error: { ...err, status: res.status } }, { status: 200, headers: NO_STORE });
  for (const c of res.cookies.getAll()) out.cookies.set(c);
  return out;
}

/* ------------------------------------------------------------------ */
/* Session cookie: every terminal login on this browser (multi-account) */
/* ------------------------------------------------------------------ */

export const SESSION_COOKIE = "kalks_trade";
export const MAX_SESSIONS = 8;
export const LOGIN_RE = /^\d{8}$/;

export interface EngineSession {
  /** trading account login */
  l: string;
  /** engine terminal session token (opaque) */
  t: string;
  /** investor (read-only) session */
  r: boolean;
  /** expiry, unix ms */
  e: number;
}

export function readSessions(req: NextRequest): EngineSession[] {
  const raw = req.cookies.get(SESSION_COOKIE)?.value;
  if (!raw) return [];
  try {
    const list = JSON.parse(Buffer.from(raw, "base64url").toString("utf8")) as EngineSession[];
    if (!Array.isArray(list)) return [];
    const now = Date.now();
    return list.filter((s) => s && typeof s.t === "string" && LOGIN_RE.test(String(s.l)) && typeof s.e === "number" && s.e > now).slice(0, MAX_SESSIONS);
  } catch {
    return [];
  }
}

/** Write the session list on `res` (or clear the cookie when it is empty). */
export function writeSessions(req: NextRequest, res: NextResponse, list: EngineSession[]) {
  const secure = req.nextUrl.protocol === "https:" || req.headers.get("x-forwarded-proto") === "https";
  if (!list.length) {
    res.cookies.set(SESSION_COOKIE, "", { httpOnly: true, sameSite: "lax", secure, path: "/", maxAge: 0 });
    return res;
  }
  const maxExp = Math.max(...list.map((s) => s.e));
  res.cookies.set(SESSION_COOKIE, Buffer.from(JSON.stringify(list.slice(0, MAX_SESSIONS)), "utf8").toString("base64url"), {
    httpOnly: true,
    sameSite: "lax",
    secure,
    path: "/",
    maxAge: Math.max(60, Math.round((maxExp - Date.now()) / 1000)),
  });
  return res;
}

/** Add (or replace) a login's session; the newest login goes first. */
export function withSession(list: EngineSession[], s: EngineSession): EngineSession[] {
  return [s, ...list.filter((x) => x.l !== s.l)].slice(0, MAX_SESSIONS);
}

/** The session for the login named in `x-kalks-login` (the account the terminal is acting on). */
export function sessionFor(req: NextRequest, list = readSessions(req)): EngineSession | null {
  const login = req.headers.get("x-kalks-login") ?? req.nextUrl.searchParams.get("login");
  if (!login || !LOGIN_RE.test(login)) return null;
  return list.find((s) => s.l === login) ?? null;
}

/** Same-origin check for state-changing requests (cookie is SameSite=Lax; writes must be JSON too). */
export function sameOrigin(req: NextRequest): boolean {
  const origin = req.headers.get("origin");
  if (!origin) return req.headers.get("sec-fetch-site") === "same-origin";
  const host = req.headers.get("x-forwarded-host") ?? req.headers.get("host");
  try {
    return new URL(origin).host === host;
  } catch {
    return false;
  }
}

export function csrf(req: NextRequest, needsJson = true): NextResponse | null {
  if (!sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
  if (needsJson && !req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");
  return null;
}

/* ------------------------------------------------------------------ */
/* Client-safe views: dealing details never reach the browser           */
/* ------------------------------------------------------------------ */

let groupsCache: { at: number; spread: Record<string, string> } | null = null;

/** group code → market-data spread group (the quotes the account trades at), cached for a minute. */
async function spreadGroups(): Promise<Record<string, string>> {
  if (groupsCache && Date.now() - groupsCache.at < 60_000) return groupsCache.spread;
  const r = await engine<{ groups?: { code: string; spreadGroup?: string }[] }>("/v1/groups");
  if (r.status !== 200) return groupsCache?.spread ?? {};
  const spread: Record<string, string> = {};
  for (const g of r.data.groups ?? []) spread[g.code] = g.spreadGroup || g.code;
  groupsCache = { at: Date.now(), spread };
  return spread;
}

export async function clientAccount(a: unknown): Promise<Obj | null> {
  if (!a || typeof a !== "object") return null;
  const { route: _route, userId: _userId, version: _version, controls, ...rest } = a as Obj;
  const c = (controls ?? {}) as Obj;
  const spread = await spreadGroups();
  const group = String(rest.group ?? "");
  return { ...rest, spreadGroup: spread[group] ?? group, controls: { tradingDisabled: !!c.tradingDisabled, closeOnly: !!c.closeOnly, maxLot: c.maxLot ?? null } };
}

export function clientPosition(p: unknown): Obj {
  const { book: _book, parentTicket: _p, childTickets: _c, priceCorrected: _pc, ...rest } = (p ?? {}) as Obj;
  return rest;
}
export function clientOrder(o: unknown): Obj {
  const { book: _book, ...rest } = (o ?? {}) as Obj;
  return rest;
}
export function clientDeal(d: unknown): Obj {
  const { book: _book, ledgerTxn: _l, ...rest } = (d ?? {}) as Obj;
  return rest;
}

/** Public WebSocket URL of the engine's account stream for this request's host. */
export function streamUrl(req: NextRequest): string {
  const env = process.env.ENGINE_STREAM_URL;
  if (env) return env;
  const host = req.headers.get("x-forwarded-host") ?? req.headers.get("host") ?? "";
  const hostname = host.replace(/:\d+$/, "").replace(/^\[|\]$/g, "");
  // local dev: straight to the engine port (the engine checks only the one-time ticket)
  if (hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1") return `${TRADING_URL.replace(/^http/, "ws")}/v1/terminal/stream`;
  // production: Caddy maps trade.<domain>/engine/stream → engine /v1/terminal/stream
  const secure = req.nextUrl.protocol === "https:" || req.headers.get("x-forwarded-proto") !== "http";
  return `${secure ? "wss" : "ws"}://${host}/engine/stream`;
}

import { NextResponse, type NextRequest } from "next/server";
import { LOCALE_COOKIE, isLocale } from "@kalks/i18n/locales";
import { CIRCLE_STREAM_URL, CIRCLE_UPLOAD_URL, circle, circlePath } from "@/lib/circle";
import { edgeCountry } from "@/lib/gateway";
import { sameOrigin, sessionUser } from "@/lib/trading";

// Kalks Circle BFF. Browser -> /api/circle/<route> (same origin) -> services/circle /v1/circle/<route>; the app uses
// /api/mobile/circle/<route> (rewritten here by the proxy with its bearer token). The client is resolved from the
// gateway session; ids sent by the browser are never trusted for identity. Every route of docs/CIRCLE-API.md §3–§9
// passes through as-is (method, path, query, JSON body), plus:
//
//   POST stream-ticket            {ticket, url, expiresIn}: one-time ticket for wss://<host>/circle/stream
//   PUT  uploads/{id}             one chunk of a resumable upload (raw body, Upload-Offset header, ≤ 8 MB)
//
// Mutations must be same-origin; JSON bodies only (except upload chunks). Module `circle` off -> the proxy answers
// 403 module_disabled before this handler runs (lib/tenant-config.ts).

type Ctx = { params: Promise<{ path: string[] }> };
const NO_STORE = { "cache-control": "no-store" };
const MAX_JSON = 256 * 1024;
const MAX_CHUNK = 8 * 1024 * 1024 + 64 * 1024;

function error(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status, headers: NO_STORE });
}

async function handle(req: NextRequest, ctx: Ctx, method: "GET" | "POST" | "PATCH" | "PUT" | "DELETE") {
  const segments = (await ctx.params).path ?? [];
  if (method !== "GET" && !sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
  const user = await sessionUser(req);
  if (user === "unavailable") return error(503, "unavailable", "Sign-in service is unavailable. Please try again shortly.");
  if (!user) return error(401, "unauthorized", "Please sign in.");
  const cookieLocale = req.cookies.get(LOCALE_COOKIE)?.value;
  const locale = cookieLocale && isLocale(cookieLocale) ? cookieLocale : null;
  const ipCountry = edgeCountry(req.headers)?.toLowerCase() ?? null;

  if (segments.length === 1 && segments[0] === "stream-ticket") {
    if (method !== "POST") return error(405, "method_not_allowed", "Use POST.");
    const r = await circle<{ ticket?: string; expiresIn?: number }>("/v1/stream/ticket", { method: "POST", user, locale, ipCountry });
    return NextResponse.json(r.status === 200 ? { ticket: r.data.ticket, url: CIRCLE_STREAM_URL || null, expiresIn: r.data.expiresIn } : r.data, { status: r.status, headers: NO_STORE });
  }
  const path = circlePath(segments);
  if (!path) return error(404, "not_found", "Not found.");
  const query = req.nextUrl.search.length <= 2048 ? req.nextUrl.search : "";

  // one chunk of a resumable upload: raw bytes + Upload-Offset
  if (method === "PUT" && segments[0] === "uploads" && segments.length === 2) {
    const offset = req.headers.get("upload-offset") ?? "";
    if (!/^\d{1,12}$/.test(offset)) return error(422, "validation", "Send the byte offset of this chunk in Upload-Offset.");
    const len = Number(req.headers.get("content-length") ?? "0");
    if (len > MAX_CHUNK) return error(413, "too_large", "Chunks can be up to 8 MB.");
    const bytes = await req.arrayBuffer();
    if (bytes.byteLength > MAX_CHUNK) return error(413, "too_large", "Chunks can be up to 8 MB.");
    const r = await circle(path, { method: "PUT", user, locale, ipCountry, raw: { bytes, type: "application/octet-stream", offset }, timeoutMs: 120_000 });
    return NextResponse.json(r.data, { status: r.status, headers: NO_STORE });
  }

  let body: unknown;
  if (method !== "GET") {
    const text = await req.text();
    if (text.length > MAX_JSON) return error(413, "too_large", "Request body too large.");
    if (text.trim()) {
      if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");
      try {
        body = JSON.parse(text);
      } catch {
        return error(400, "bad_request", "Invalid request body.");
      }
      if (!body || typeof body !== "object" || Array.isArray(body)) return error(400, "bad_request", "Invalid request body.");
    } else if (method !== "DELETE") {
      body = {};
    }
  }
  const r = await circle<Record<string, unknown>>(`${path}${query}`, { method, body, user, locale, ipCountry, timeoutMs: 30_000 });
  // the direct upload URL of a new upload: this host in production, the service in development
  const upload = r.data?.upload as { id?: number; directUrl?: string } | undefined;
  if (r.status === 200 && upload?.id !== undefined) upload.directUrl = `${CIRCLE_UPLOAD_URL}/${upload.id}`;
  return NextResponse.json(r.data, { status: r.status, headers: NO_STORE });
}

export const GET = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "GET");
export const POST = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "POST");
export const PATCH = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "PATCH");
export const PUT = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "PUT");
export const DELETE = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "DELETE");

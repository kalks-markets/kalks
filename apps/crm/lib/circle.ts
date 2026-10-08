// Server-only helpers for Kalks Circle (services/circle, 127.0.0.1:8105, docs/CIRCLE-API.md). The browser and the
// app never see the service or CIRCLE_INTERNAL_TOKEN: /api/circle/* resolves the signed-in client from the gateway
// session (cookie, or the app's bearer through the /api/mobile/circle/* rewrite) and forwards the gateway user id,
// broker, name, country, KYC status, language and referral code as X-Kalks-* headers.

import type { GatewayUser } from "@/lib/gateway";

const CIRCLE_URL = (process.env.CIRCLE_URL ?? "http://127.0.0.1:8105").replace(/\/+$/, "");
const CIRCLE_TOKEN = process.env.CIRCLE_INTERNAL_TOKEN ?? "";

/**
 * Browser URL of the realtime stream. Unset: `wss://<this host>/circle/stream` in production (Caddy -> :8105) and
 * the service directly in development.
 */
export const CIRCLE_STREAM_URL = process.env.CIRCLE_STREAM_URL ?? (process.env.NODE_ENV === "production" ? "" : `${CIRCLE_URL.replace(/^http/, "ws")}/v1/stream`);

/**
 * Base of the direct resumable upload route (`PUT <base>/<id>?token=`). Production: `/circle/upload` on this host
 * (Caddy -> :8105, no Next.js in the way for large videos); development: the service directly.
 */
export const CIRCLE_UPLOAD_URL = process.env.CIRCLE_UPLOAD_URL ?? (process.env.NODE_ENV === "production" ? "/circle/upload" : `${CIRCLE_URL}/v1/upload`);

export type CircleResult<T = Record<string, unknown>> = { status: number; data: T };

type Init = {
  method?: "GET" | "POST" | "PATCH" | "PUT" | "DELETE";
  body?: unknown;
  raw?: { bytes: ArrayBuffer; type: string; offset: string };
  user?: GatewayUser;
  locale?: string | null;
  ipCountry?: string | null;
  timeoutMs?: number;
};

/** Identity headers for the service (name and referral code percent-encoded / validated by the service). */
export function circleHeaders(user: GatewayUser, extra: { locale?: string | null; ipCountry?: string | null } = {}): Record<string, string> {
  const h: Record<string, string> = {
    "x-kalks-tenant": user.tenant?.slug || "kalks",
    "x-kalks-user-id": String(user.id),
    "x-kalks-user-name": encodeURIComponent(user.name || `${user.first_name ?? ""} ${user.last_name ?? ""}`.trim()),
  };
  if (user.country) h["x-kalks-country"] = user.country;
  if (user.kyc_status) h["x-kalks-kyc"] = user.kyc_status;
  if (user.referral_code) h["x-kalks-referral-code"] = user.referral_code;
  if (extra.locale) h["x-kalks-locale"] = extra.locale;
  if (extra.ipCountry) h["x-kalks-ip-country"] = extra.ipCountry;
  return h;
}

export async function circle<T = Record<string, unknown>>(path: string, init: Init = {}): Promise<CircleResult<T>> {
  const headers: Record<string, string> = { "x-kalks-internal": CIRCLE_TOKEN };
  if (init.user) Object.assign(headers, circleHeaders(init.user, { locale: init.locale, ipCountry: init.ipCountry }));
  let body: BodyInit | undefined;
  if (init.raw) {
    headers["content-type"] = init.raw.type || "application/octet-stream";
    headers["upload-offset"] = init.raw.offset;
    body = init.raw.bytes;
  } else if (init.body !== undefined) {
    headers["content-type"] = "application/json";
    body = JSON.stringify(init.body);
  }
  try {
    const res = await fetch(`${CIRCLE_URL}${path}`, {
      method: init.method ?? (body !== undefined ? "POST" : "GET"),
      headers,
      body,
      cache: "no-store",
      signal: AbortSignal.timeout(init.timeoutMs ?? 20_000),
    });
    const data = (await res.json().catch(() => ({}))) as T;
    return { status: res.status, data };
  } catch {
    return { status: 503, data: { error: { code: "unavailable", message: "Kalks Circle is unavailable right now. Please try again shortly." } } as T };
  }
}

const SEGMENT = /^[A-Za-z0-9_.@:$-]{1,64}$/;
/** First path segments the service serves to clients (docs/CIRCLE-API.md). */
const FAMILIES = new Set([
  "me", "handles", "profiles", "posts", "comments", "trade-cards", "feed", "tags", "cashtags", "explore", "search", "topics", "leaderboards", "prices",
  "announcements", "academy", "ai", "rules", "stories", "highlights", "chat", "uploads", "media", "reports",
]);

/** The service path of a BFF path (`["posts", "12", "react"]` -> `/v1/circle/posts/12/react`), or null when refused. */
export function circlePath(segments: string[]): string | null {
  if (!segments.length || segments.length > 8) return null;
  if (segments.some((s) => !SEGMENT.test(s) || s === "." || s === "..")) return null;
  if (!FAMILIES.has(segments[0]!)) return null;
  return `/v1/circle/${segments.map(encodeURIComponent).join("/")}`;
}

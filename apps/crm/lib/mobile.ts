// The Kalks mobile app (Flutter, apps/mobile) talks to this Client Area BFF under /api/mobile/*. Contract for the
// app: docs/MOBILE-API.md.
//
// The app has no cookies: it keeps the gateway session token in the phone's secure store (Android Keystore) and
// sends it as `Authorization: Bearer <token>`. Identity is still derived server-side exactly like the cookie routes:
//
// - Most mobile paths are REWRITES of the existing cookie BFF routes (/api/mobile/wallet/... -> /api/wallet/...).
//   The proxy (proxy.ts) moves the bearer token into the rewritten request's session cookie and marks it as
//   same-origin, so the very same handlers run: validation, step-up codes, viewer scopes, staff read-only sessions,
//   module switches and maintenance mode.
// - A few paths are NATIVE mobile routes: auth flows that must return the session token in the body (auth/*),
//   the public app config (config) and the trading-engine terminal API (trade/*, lib/mobile-trade.ts).
//
// CSRF: a browser never adds an Authorization header on its own (a cross-site page needs a CORS preflight for it,
// which this app never grants), a request carrying both a bearer token and cookies is refused, and every cookie a
// browser attaches to /api/mobile/* is dropped. So only the holder of the token can act with it, and the cookie
// routes keep their same-origin checks unchanged for browsers.

import { isLocale, LOCALE_COOKIE } from "@kalks/i18n/locales";
import { tenantConfig, type TenantConfig } from "@/lib/tenant-config";
import { hostOf } from "@/lib/tenant-host";

export const MOBILE_PREFIX = "/api/mobile/";

/** Gateway session tokens: base64url, optionally prefixed (`v.` view-only, `i.` / `s.` staff sessions). */
const TOKEN_RE = /^(?:[a-z]\.)?[A-Za-z0-9_-]{32,128}$/;
/** Device ids (base64url of random bytes): minted by /api/mobile/auth/* or the app itself. The gateway takes 16–128. */
const DEVICE_RE = /^[A-Za-z0-9_-]{16,128}$/;
/** `1.4.2`, `1.4.2+37`, `1.4.2-beta.1`. */
const VERSION_RE = /^\d{1,4}\.\d{1,4}\.\d{1,4}(?:[-+][0-9A-Za-z.+-]{1,32})?$/;

/** Existing cookie BFF families the app uses through a rewrite (first path segment under /api). */
export const REWRITE_FAMILIES = ["trading", "wallet", "news", "notifications", "kyc", "security", "support", "status", "growth", "partner", "social", "prop", "academy", "reports", "algo", "suitability", "circle"] as const;
const REWRITES = new Set<string>(REWRITE_FAMILIES);
/** Cookie auth routes that answer without a session in the body: served through a rewrite as well. */
const AUTH_REWRITES = new Set(["heartbeat", "impersonation", "marketing"]);
/** Native auth actions (app/api/mobile/auth/[action]). */
export const AUTH_ACTIONS = ["login", "verify-email", "resend", "register", "forgot", "reset", "logout", "me", "stepup", "stepup-verify", "stepup-resend", "password"] as const;
const AUTH_NATIVE = new Set<string>(AUTH_ACTIONS);
/** Policy path of routes anyone may call (no maintenance gate, no viewer or staff rules): the proxy's ALWAYS_OPEN. */
const OPEN_POLICY = "/api/status";

export type MobileRoute = {
  /** rewrite: served by the cookie route at `target`; native: served by app/api/mobile/... itself. */
  kind: "rewrite" | "native";
  /** Where the request is served (the rewrite target, or the mobile path itself). */
  target: string;
  /** The equivalent cookie-route path, used for the maintenance / module / viewer / staff policies. */
  policyPath: string;
};

/** Resolves a /api/mobile/* path; null for anything the app may not call. */
export function mobileRoute(pathname: string): MobileRoute | null {
  if (!pathname.startsWith(MOBILE_PREFIX)) return null;
  const rest = pathname.slice(MOBILE_PREFIX.length).replace(/\/+$/, "");
  if (!rest || rest.includes("..") || rest.includes("//") || rest.includes("\\") || /%(2e|2f|5c|00)/i.test(rest)) return null;
  const parts = rest.split("/");
  const [family, second] = parts;
  if (family === "config") return parts.length === 1 ? { kind: "native", target: pathname, policyPath: OPEN_POLICY } : null;
  if (family === "auth") {
    if (parts.length !== 2 || !second) return null;
    if (AUTH_REWRITES.has(second)) return { kind: "rewrite", target: `/api/${rest}`, policyPath: `/api/${rest}` };
    return AUTH_NATIVE.has(second) ? { kind: "native", target: pathname, policyPath: `/api/auth/${second}` } : null;
  }
  if (family === "trade") {
    if (parts.length < 2) return null;
    // contract specs and the live-trading switches are public product information (Kalks Trader serves them to guests)
    if (rest === "trade/symbols") return { kind: "native", target: pathname, policyPath: OPEN_POLICY };
    return { kind: "native", target: pathname, policyPath: `/api/mobile/${rest}` };
  }
  if (family && REWRITES.has(family)) return { kind: "rewrite", target: `/api/${rest}`, policyPath: `/api/${rest}` };
  return null;
}

/** The bearer session token of a mobile request (null when absent or malformed). */
export function bearerOf(h: Headers): string | null {
  const v = h.get("authorization");
  if (!v) return null;
  const m = /^Bearer\s+(\S+)$/i.exec(v.trim());
  return m && TOKEN_RE.test(m[1]!) ? m[1]! : null;
}

/** The app's device id (`X-Kalks-Device`), when well-formed. */
export function deviceOf(h: Headers): string | null {
  const v = h.get("x-kalks-device")?.trim();
  return v && DEVICE_RE.test(v) ? v : null;
}

/** The reader's language sent by the app (`X-Kalks-Locale`), when supported. */
export function localeOf(h: Headers): string | null {
  const v = h.get("x-kalks-locale")?.trim().toLowerCase();
  return v && isLocale(v) ? v : null;
}

/** The platform recorded on orders (`X-Kalks-Platform: android | ios`). The app is Android-first: missing = Android. */
export function platformOf(h: Headers): "Android" | "iOS" {
  return h.get("x-kalks-platform")?.trim().toLowerCase() === "ios" ? "iOS" : "Android";
}

/** The app's version (`X-Kalks-App-Version`), when well-formed. */
export function appVersionOf(h: Headers): string | null {
  const v = h.get("x-kalks-app-version")?.trim();
  return v && VERSION_RE.test(v) ? v : null;
}

/** Request headers of a mobile call as the proxy received them: a cookie header of any kind counts. */
export const hasCookies = (h: Headers) => !!h.get("cookie")?.trim();

/**
 * Request headers for the handler that serves a mobile request.
 * - Any cookie the client sent is dropped (a browser's Client Area cookie never authenticates /api/mobile/*).
 * - Rewrites with a bearer token (and no cookies): the token becomes the session cookie and the request is marked
 *   same-origin, so the cookie route's CSRF check passes. The token itself proves the caller holds the session, see
 *   the note at the top. Without a token nothing is marked, so a cookie route's writes stay refused.
 * - The app's language and device id become the locale / device cookies the cookie routes read, so gateway emails
 *   and errors follow the app's language.
 */
export function mobileRequestHeaders(incoming: Headers, token: string | null, route: MobileRoute): Headers {
  const h = new Headers(incoming);
  const cookieFree = !hasCookies(incoming);
  h.delete("cookie");
  h.delete("x-kalks-mobile");
  const cookies: string[] = [];
  const locale = localeOf(incoming);
  if (locale) cookies.push(`${LOCALE_COOKIE}=${locale}`);
  if (route.kind === "rewrite") {
    const device = deviceOf(incoming);
    if (device) cookies.push(`kalks_did=${device}`);
    h.delete("authorization");
    if (token && cookieFree) {
      cookies.push(`kalks_session=${token}`);
      const host = (h.get("x-forwarded-host") ?? h.get("host") ?? "").split(",")[0]!.trim();
      const proto = h.get("x-forwarded-proto") === "https" ? "https" : "http";
      if (host) h.set("origin", `${proto}://${host}`);
      h.set("sec-fetch-site", "same-origin");
    }
  }
  if (cookies.length) h.set("cookie", cookies.join("; "));
  h.set("x-kalks-mobile", "1");
  return h;
}

/* ------------------------------------------------------------------ */
/* Public service URLs for the app                                     */
/* ------------------------------------------------------------------ */

/** Loopback hosts (local development). */
export function isLoopbackHost(host: string): boolean {
  const name = host.replace(/:\d+$/, "").replace(/^\[|\]$/g, "").toLowerCase();
  return name === "localhost" || name === "127.0.0.1" || name === "::1" || name.endsWith(".localhost");
}

export const toWs = (url: string) => url.replace(/^http/, "ws");
const trim = (url: string) => url.replace(/\/+$/, "");

/** The origin the app used to reach this BFF (Caddy forwards the public host and the scheme). */
export function requestOrigin(h: Headers, fallback: URL): URL {
  const host = (h.get("x-forwarded-host") ?? h.get("host") ?? fallback.host).split(",")[0]!.trim();
  const proto = h.get("x-forwarded-proto") ?? fallback.protocol.replace(":", "");
  return new URL(`${proto === "https" ? "https" : "http"}://${host}`);
}

export type ServiceUrls = {
  /** This Client Area (the app's API base, minus /api/mobile). */
  app: string;
  /** Kalks Trader on the web (the broker's trade domain). */
  terminal: string;
  /** market-data REST (candles, quotes, symbols) and its quote stream (no auth; `?group=<spreadGroup>`). */
  marketData: { http: string; ws: string };
  /** WebSocket streams that need a one-time ticket: `<url>?ticket=<ticket>`. */
  streams: { engine: string; options: string; support: string; circle: string };
  /** Direct resumable uploads (Kalks Circle videos): `PUT <url>/<id>?token=` with `Upload-Offset`. */
  uploads: { circle: string };
};

/**
 * Where the app finds the public services of this broker. Production: market-data at its public edge
 * (NEXT_PUBLIC_MARKET_DATA_URL), the trading-engine and options streams at the broker's trade domain
 * (trade.<domain>/engine/stream, /options/stream: Caddy), the support stream at this host (/support/stream).
 * Locally (loopback URLs) the services' own ports: on a USB phone, `adb reverse tcp:<port> tcp:<port>` makes them
 * reachable as localhost. Every URL can be pinned with an env var (MOBILE_*).
 */
export function serviceUrls(origin: URL, opts: { tradeUrl?: string | null } = {}): ServiceUrls {
  const env = process.env;
  const terminal = trim(opts.tradeUrl || env.NEXT_PUBLIC_TERMINAL_URL || "http://localhost:3002");
  const app = origin.origin;
  const md = trim(env.MOBILE_MARKET_DATA_URL || env.NEXT_PUBLIC_MARKET_DATA_URL || "http://127.0.0.1:8081");
  const tradingUrl = trim(env.TRADING_URL || "http://127.0.0.1:8090");
  const optionsUrl = trim(env.OPTIONS_URL || "http://127.0.0.1:8104");
  const supportUrl = trim(env.SUPPORT_URL || "http://127.0.0.1:8100");
  const circleUrl = trim(env.CIRCLE_URL || "http://127.0.0.1:8105");
  const appLocal = isLoopbackHost(new URL(app).host);
  const termHost = new URL(terminal).host;
  const local = isLoopbackHost(termHost);
  return {
    app,
    terminal,
    marketData: { http: md, ws: `${toWs(md)}/v1/stream` },
    streams: {
      engine: env.MOBILE_ENGINE_STREAM_URL || (local ? `${toWs(tradingUrl)}/v1/terminal/stream` : `${toWs(new URL(terminal).origin)}/engine/stream`),
      options: env.MOBILE_OPTIONS_STREAM_URL || (local ? `${toWs(optionsUrl)}/v1/options/stream` : `${toWs(new URL(terminal).origin)}/options/stream`),
      support: env.MOBILE_SUPPORT_STREAM_URL || env.SUPPORT_STREAM_URL || (appLocal ? `${toWs(supportUrl)}/v1/stream` : `${toWs(new URL(app).origin)}/support/stream`),
      circle: env.MOBILE_CIRCLE_STREAM_URL || env.CIRCLE_STREAM_URL || (appLocal ? `${toWs(circleUrl)}/v1/stream` : `${toWs(new URL(app).origin)}/circle/stream`),
    },
    uploads: { circle: env.MOBILE_CIRCLE_UPLOAD_URL || (appLocal ? `${circleUrl}/v1/upload` : `${new URL(app).origin}/circle/upload`) },
  };
}

/** The service URLs for a request: the broker of this host (gateway tenant config) may have its own trade domain. */
export async function brokerServiceUrls(h: Headers, fallback: URL, cfg?: TenantConfig | null): Promise<ServiceUrls> {
  const origin = requestOrigin(h, fallback);
  const brand = (cfg === undefined ? await tenantConfig(hostOf(h) ?? "") : cfg)?.branding ?? null;
  // a white-label broker's own trade domain; never on a local stack, whose services run on this machine
  const tradeUrl = brand && !brand.default && !isLoopbackHost(origin.host) ? (brand.urls?.trade ?? null) : null;
  return serviceUrls(origin, { tradeUrl });
}

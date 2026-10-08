import { NextResponse, type NextRequest } from "next/server";
import { IS_DEMO } from "@kalks/mock/mode";
import { SESSION_COOKIE, clientIp, gateway, safeNext } from "@/lib/gateway";
import { REF_COOKIE, cleanRef, trackClick } from "@/lib/ib";
import { captureAttribution } from "@/lib/attribution";
import { moduleOff, offModule, tenantConfig } from "@/lib/tenant-config";
import { hostOf } from "@/lib/tenant-host";
import { VIEWER_OUT_OF_SCOPE, VIEWER_READ_ONLY, isViewerToken, viewerApiAllowed, viewerHome, viewerPageAllowed, type ViewerScope } from "@/lib/viewer";
import { MOBILE_PREFIX, bearerOf, hasCookies, mobileRequestHeaders, mobileRoute } from "@/lib/mobile";

// Route protection for the Client Area.
// - Signed-out visitors on any app page -> /login?next=<page>
// - Signed-in users on /login, /register, /forgot -> ?next or the dashboard
// The (app) layout re-validates the session with the gateway on every full render.
// Demo builds (NEXT_PUBLIC_KALKS_MODE=demo) have no sign-in gate: everything is open, and the
// login / register pages offer "Enter demo".

const AUTH_PAGES = ["/login", "/register", "/forgot"];
/** Public pages (no sign-in): Academy and Prop certificate verification, the end page of a staff session. */
const PUBLIC_PAGES = ["/certificate", "/verify", "/s", "/staff-session"];

// Partner links (IB programme, services/ib): /r/CODE[/campaign] or any page with ?ref=CODE[&c=campaign].
// The click is recorded with the IB service and the referral kept in a first-party cookie (kalks_ref), so
// sign-up (email or Google) attributes the partner and campaign. Signed-out visitors land on /register.
const REF_SEEN = "kalks_ref_seen";

/** Pages and APIs that stay reachable in maintenance mode and without a session. */
const ALWAYS_OPEN = ["/status", "/maintenance", "/unavailable", "/api/status", "/unsubscribe", "/api/unsubscribe"];

// Broker runtime config (gateway tenant config): maintenance mode holds clients on /maintenance (their API
// calls answer 503); a module the Platform Owner switched off (D112) is hidden from the nav and its pages and
// BFF routes answer "not available". Staff (Back Office) are never affected.
export async function proxy(req: NextRequest) {
  if (IS_DEMO) return routes(req);
  const { pathname } = req.nextUrl;
  // the mobile app: bearer session instead of cookies (lib/mobile.ts, docs/MOBILE-API.md)
  if (pathname.startsWith(MOBILE_PREFIX)) return mobileApi(req);
  const open = ALWAYS_OPEN.some((p) => pathname === p || pathname.startsWith(`${p}/`));
  const api = pathname.startsWith("/api/");
  if (!open) {
    const held = await brokerGate(req, pathname, api);
    if (held) return held;
  }
  if (api && !open) {
    const token = req.cookies.get(SESSION_COOKIE)?.value;
    const held = (await viewerApi(req, token, pathname)) ?? (await staffApi(req, token, pathname));
    if (held) return held;
  }
  if (api || open) return NextResponse.next();
  // first-touch UTM / referrer attribution for sign-up (lib/attribution.ts)
  return captureAttribution(req, await routes(req));
}

/** Maintenance mode and modules switched off for the broker; null = go ahead. */
async function brokerGate(req: NextRequest, pathname: string, api: boolean): Promise<NextResponse | null> {
  const cfg = await tenantConfig(hostOf(req.headers) ?? "");
  if (cfg?.maintenance.active && pathname !== "/api/auth/logout") {
    if (api) return NextResponse.json({ error: { code: "maintenance", message: "The Client Area is under maintenance. Please try again shortly." } }, { status: 503, headers: { "retry-after": "60" } });
    return NextResponse.rewrite(new URL("/maintenance", req.url));
  }
  if (cfg && moduleOff(cfg.modules, pathname)) {
    if (api) return NextResponse.json({ error: { code: "module_disabled", message: "This feature isn't available on your account." } }, { status: 403 });
    const to = new URL("/unavailable", req.url);
    to.search = "";
    to.searchParams.set("m", offModule(cfg.modules, pathname) ?? "");
    return NextResponse.rewrite(to);
  }
  return null;
}

const mobileError = (status: number, code: string, message: string) => NextResponse.json({ error: { code, message } }, { status, headers: { "cache-control": "no-store" } });

/**
 * The mobile app's API (/api/mobile/*, lib/mobile.ts). The same broker, viewer and staff policies as the cookie
 * routes apply, judged on the equivalent cookie-route path; then the request is either rewritten to that cookie
 * route (bearer token as its session cookie) or served by the native mobile route. Browser cookies never count
 * here, and a bearer token is never accepted together with cookies.
 */
async function mobileApi(req: NextRequest): Promise<NextResponse> {
  const route = mobileRoute(req.nextUrl.pathname);
  if (!route) return mobileError(404, "not_found", "Not found.");
  const token = bearerOf(req.headers);
  if (req.headers.has("authorization") && !token) return mobileError(401, "unauthorized", "Please sign in.");
  if (token && hasCookies(req.headers)) return mobileError(400, "bearer_with_cookies", "Send the session token without cookies.");
  const path = route.policyPath;
  const open = ALWAYS_OPEN.some((p) => path === p || path.startsWith(`${p}/`));
  if (!open) {
    const held = await brokerGate(req, path, true);
    if (held) return held;
  }
  if (token && !open) {
    const held = (await viewerApi(req, token, path)) ?? (await staffApi(req, token, path));
    if (held) return held;
  }
  const headers = mobileRequestHeaders(req.headers, token, route);
  if (route.kind === "rewrite") return NextResponse.rewrite(new URL(route.target + req.nextUrl.search, req.url), { request: { headers } });
  return NextResponse.next({ request: { headers } });
}

/** The scope of a view-only session (D90), or null for a normal session / no session. */
async function viewerScope(req: NextRequest, token = req.cookies.get(SESSION_COOKIE)?.value): Promise<ViewerScope | null> {
  if (!isViewerToken(token)) return null;
  const r = await gateway<{ viewer?: ViewerScope | null }>("/v1/auth/me", { token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
  return r.status === 200 ? (r.data.viewer ?? null) : null;
}

/**
 * View-only sessions are held to read requests of their sections before any BFF runs: every change is refused
 * here (and again by the BFFs and the gateway). A dead viewer session falls through to the normal 401 handling.
 */
async function viewerApi(req: NextRequest, token: string | undefined | null, pathname: string): Promise<NextResponse | null> {
  if (!isViewerToken(token)) return null;
  const scope = await viewerScope(req, token ?? undefined);
  if (!scope) return null;
  if (viewerApiAllowed(scope, req.method, pathname)) return null;
  const readOnly = req.method !== "GET" && req.method !== "HEAD";
  return NextResponse.json({ error: readOnly ? VIEWER_READ_ONLY : VIEWER_OUT_OF_SCOPE }, { status: 403, headers: { "cache-control": "no-store" } });
}

// Staff sessions (Back Office "Log in as client", gateway client_controls.rs): read-only ones are held to read
// requests before any BFF runs (the gateway refuses their changes too); every change a staff session makes or is
// refused is reported to the gateway, which audits it with the staff id.
const STAFF_READ_ONLY_PREFIX = "i.";
const STAFF_FULL_PREFIX = "s.";
/** Changes a read-only staff session may still make: sign out / end the staff session, presence heartbeat, and the
 *  ticket for the receive-only realtime stream (bell + chat updates; the socket accepts no commands). */
const STAFF_ALWAYS_POST = ["/api/auth/logout", "/api/auth/heartbeat", "/api/auth/impersonation", "/api/support/stream-ticket"];

function staffToken(req: NextRequest, token = req.cookies.get(SESSION_COOKIE)?.value): { token: string; readOnly: boolean } | null {
  if (!token) return null;
  if (token.startsWith(STAFF_READ_ONLY_PREFIX)) return { token, readOnly: true };
  if (token.startsWith(STAFF_FULL_PREFIX)) return { token, readOnly: false };
  return null;
}

async function staffEvent(req: NextRequest, token: string, kind: "action" | "write_refused" | "page_view", status?: number, path = req.nextUrl.pathname) {
  await gateway("/v1/auth/impersonation/event", {
    body: { kind, method: req.method, path, status },
    token,
    ip: clientIp(req.headers),
    userAgent: req.headers.get("user-agent"),
  }).catch(() => null);
}

/** `pathname`: the cookie-route path the request is judged (and audited) as; for the mobile app, its policy path. */
async function staffApi(req: NextRequest, token: string | undefined | null, pathname: string): Promise<NextResponse | null> {
  const s = staffToken(req, token ?? undefined);
  if (!s || req.method === "GET" || req.method === "HEAD") return null;
  if (STAFF_ALWAYS_POST.includes(pathname) || pathname === "/api/security/viewer-activity") return null;
  if (s.readOnly) {
    await staffEvent(req, s.token, "write_refused", 403, pathname);
    return NextResponse.json({ error: { code: "staff_read_only", message: "This is a read-only staff session. Changes are not allowed." } }, { status: 403, headers: { "cache-control": "no-store" } });
  }
  await staffEvent(req, s.token, "action", undefined, pathname);
  return null;
}

async function routes(req: NextRequest) {
  if (IS_DEMO) return gate(req);
  const { pathname, searchParams } = req.nextUrl;
  const short = pathname.match(/^\/r\/([^/]+)(?:\/([^/]+))?\/?$/);
  const ref = short ? cleanRef(decodeURIComponent(short[1]!), short[2] ? decodeURIComponent(short[2]) : null) : cleanRef(searchParams.get("ref"), searchParams.get("c"));
  if (short && !ref) return NextResponse.redirect(new URL("/register", req.url));
  if (!ref) return gate(req);

  const signedIn = !!req.cookies.get(SESSION_COOKIE)?.value;
  const onAuthPage = AUTH_PAGES.some((p) => pathname === p || pathname.startsWith(`${p}/`));
  let res: NextResponse;
  if (short || (!signedIn && !onAuthPage)) {
    const url = new URL("/register", req.url);
    url.searchParams.set("ref", ref.code);
    if (ref.campaign) url.searchParams.set("c", ref.campaign);
    res = NextResponse.redirect(url);
  } else {
    res = await gate(req);
  }
  // one click per visitor and link per 30 minutes from this browser (reloads and the /r redirect don't count twice)
  const value = ref.campaign ? `${ref.code}:${ref.campaign}` : ref.code;
  if (req.cookies.get(REF_SEEN)?.value !== value) {
    await trackClick(req, res, ref, clientIp(req.headers));
    res.cookies.set(REF_SEEN, value, { httpOnly: true, secure: process.env.NODE_ENV === "production", sameSite: "lax", path: "/", maxAge: 1800 });
  } else if (!req.cookies.get(REF_COOKIE)) {
    res.cookies.set(REF_COOKIE, value, { httpOnly: false, secure: process.env.NODE_ENV === "production", sameSite: "lax", path: "/", maxAge: 90 * 24 * 3600 });
  }
  return res;
}

async function gate(req: NextRequest): Promise<NextResponse> {
  if (IS_DEMO) return NextResponse.next();
  const { pathname, search } = req.nextUrl;
  const token = req.cookies.get(SESSION_COOKIE)?.value;
  const isAuthPage = AUTH_PAGES.some((p) => pathname === p || pathname.startsWith(`${p}/`));
  if (PUBLIC_PAGES.some((p) => pathname === p || pathname.startsWith(`${p}/`))) return NextResponse.next();

  if (isAuthPage) {
    if (!token) return NextResponse.next();
    const r = await gateway("/v1/auth/me", { token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent"), host: hostOf(req.headers) });
    if (r.status === 200) return NextResponse.redirect(new URL(safeNext(req.nextUrl.searchParams.get("next")), req.url));
    const res = NextResponse.next();
    if (r.status === 401) res.cookies.delete(SESSION_COOKIE);
    return res;
  }

  if (!token) {
    const url = new URL("/login", req.url);
    if (pathname !== "/") url.searchParams.set("next", pathname + search);
    return NextResponse.redirect(url);
  }
  // pages a staff member opens as the client are audited (at most once per page every 5 minutes). Only full page
  // loads count here: Next strips the prefetch headers before the proxy, so an in-app request could be a link
  // prefetch of a page nobody opened. In-app navigations are reported by the staff banner (account-notices.tsx).
  const staff = staffToken(req);
  const dest = req.headers.get("sec-fetch-dest");
  const socket = req.headers.get("upgrade")?.toLowerCase() === "websocket";
  if (staff && !socket && (!dest || dest === "document")) await staffEvent(req, staff.token, "page_view");
  // view-only sessions only see the sections they were given
  if (isViewerToken(token)) {
    const scope = await viewerScope(req);
    if (scope && !viewerPageAllowed(scope, pathname)) {
      const home = viewerHome(scope);
      if (home !== pathname) return NextResponse.redirect(new URL(home, req.url));
    }
  }
  // let the layout know which page was requested (for ?next= if the session turns out to be dead)
  const headers = new Headers(req.headers);
  headers.set("x-kalks-path", pathname + search);
  return NextResponse.next({ request: { headers } });
}

export const config = {
  // everything except API routes, Next internals and static files
  // (/trade is redirected to Kalks Trader by next.config before the proxy runs)
  // (API routes only pass the maintenance / module gate above)
  matcher: ["/((?!_next/|assets/|favicon\\.ico|.*\\.[a-zA-Z0-9]+$).*)"],
};

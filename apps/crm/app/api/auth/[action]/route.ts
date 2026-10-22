import { NextResponse, type NextRequest } from "next/server";
import { publicUrl } from "@kalks/ui/public-url";
import { DEVICE_COOKIE, SESSION_COOKIE, clientIp, edgeCountry, forgetSession, gateway, newDeviceId, safeNext, sameOrigin, setDeviceCookie, setSessionCookie } from "@/lib/gateway";
import { withCampaign } from "@/lib/ib";
import { withAttribution } from "@/lib/attribution";

// Client Area auth BFF. Browser -> /api/auth/<action> (same origin) -> gateway /v1/auth/<action>.
// CSRF: cookies are SameSite=Lax, POSTs must be JSON and carry a same-origin Origin header.

const POST_ACTIONS: Record<string, string> = {
  register: "/v1/auth/register",
  login: "/v1/auth/login",
  "verify-email": "/v1/auth/verify-email",
  resend: "/v1/auth/resend",
  forgot: "/v1/auth/forgot",
  reset: "/v1/auth/reset",
  logout: "/v1/auth/logout",
  // signed-in only: step-up confirmation (D20) and the Client Area password change
  stepup: "/v1/auth/stepup",
  "stepup-resend": "/v1/auth/stepup/resend",
  "stepup-verify": "/v1/auth/stepup/verify",
  password: "/v1/auth/password",
};

/** Actions that act on the current session, so the session token is forwarded. */
const SESSION_ACTIONS = new Set(["logout", "stepup", "stepup-resend", "stepup-verify", "password"]);

type Session = { token: string; expires_at: string };

function error(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status });
}

export async function POST(req: NextRequest, { params }: { params: Promise<{ action: string }> }) {
  const { action } = await params;
  const path = POST_ACTIONS[action];
  if (!path) return error(404, "not_found", "Not found.");
  if (!sameOrigin(req.headers)) return error(403, "forbidden", "Cross-site request blocked.");
  if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");

  const body = action === "logout" ? {} : await req.json().catch(() => null);
  if (body === null || typeof body !== "object") return error(400, "bad_request", "Invalid request body.");
  // partner campaign (IB programme) from the referral link the visitor arrived with
  if (action === "register") withCampaign(body as Record<string, unknown>, req);
  // first-touch UTM attribution (cookie) and the marketing-email choice from the form
  if (action === "register") withAttribution(body as Record<string, unknown>, req);

  const token = req.cookies.get(SESSION_COOKIE)?.value;
  let device = req.cookies.get(DEVICE_COOKIE)?.value;
  const mintDevice = !device;
  if (!device) device = newDeviceId();

  const r = await gateway<Record<string, unknown> & { session?: Session }>(path, {
    body,
    ip: clientIp(req.headers),
    userAgent: req.headers.get("user-agent"),
    device,
    token: SESSION_ACTIONS.has(action) ? token : undefined,
    country: edgeCountry(req.headers),
  });

  const { session, ...data } = r.data;
  const res = NextResponse.json(data, { status: r.status, headers: { "cache-control": "no-store" } });
  if (mintDevice) setDeviceCookie(res, device);
  if (session?.token) setSessionCookie(res, session);
  if (action === "logout" || action === "password") await forgetSession(token);
  if (action === "logout" || action === "reset") res.cookies.delete(SESSION_COOKIE);
  return res;
}

export async function GET(req: NextRequest, { params }: { params: Promise<{ action: string }> }) {
  const { action } = await params;
  if (action === "me") {
    const token = req.cookies.get(SESSION_COOKIE)?.value;
    if (!token) return error(401, "unauthorized", "Please sign in.");
    const r = await gateway("/v1/auth/me", { token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
    const res = NextResponse.json(r.data, { status: r.status, headers: { "cache-control": "no-store" } });
    if (r.status === 401) res.cookies.delete(SESSION_COOKIE);
    return res;
  }
  if (action === "expired") {
    // Stale or revoked session: drop the cookie and go to sign-in (keeps the page the user wanted).
    const next = safeNext(req.nextUrl.searchParams.get("next"), "");
    const token = req.cookies.get(SESSION_COOKIE)?.value;
    if (token) {
      // only clear a cookie that really is dead, so a cross-site link can't sign someone out
      const r = await gateway("/v1/auth/me", { token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
      if (r.status !== 401) return NextResponse.redirect(publicUrl(req, next || "/"));
    }
    const url = publicUrl(req, "/login");
    if (next) url.searchParams.set("next", next);
    const res = NextResponse.redirect(url);
    res.cookies.delete(SESSION_COOKIE);
    return res;
  }
  return error(404, "not_found", "Not found.");
}

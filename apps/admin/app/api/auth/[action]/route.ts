import { NextResponse, type NextRequest } from "next/server";
import { publicUrl } from "@kalks/ui/public-url";
import { STAFF_COOKIE, STAFF_DEVICE_COOKIE, clientIp, forgetStaff, gateway, safeNext } from "@/lib/gateway";

// Back Office staff auth BFF. Browser -> /api/auth/<action> (same origin) -> gateway /v1/admin/auth/<action>.
// CSRF: cookies are SameSite=Lax, POSTs must be JSON and carry a same-origin Origin header.

const POST_ACTIONS: Record<string, string> = {
  login: "/v1/admin/auth/login",
  "verify-otp": "/v1/admin/auth/verify-otp",
  resend: "/v1/admin/auth/resend",
  logout: "/v1/admin/auth/logout",
  // staff invite: set a password, then the emailed sign-in code (verify-otp) issues the session
  "invite-accept": "/v1/admin/auth/invite/accept",
};

const PROD = process.env.NODE_ENV === "production";

type Session = { token: string; expires_at: string };

function error(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status });
}

function sameOrigin(req: NextRequest): boolean {
  const origin = req.headers.get("origin");
  if (!origin) return req.headers.get("sec-fetch-site") === "same-origin";
  const host = req.headers.get("x-forwarded-host") ?? req.headers.get("host");
  try {
    return new URL(origin).host === host;
  } catch {
    return false;
  }
}

function newDeviceId(): string {
  const b = new Uint8Array(24);
  crypto.getRandomValues(b);
  return Buffer.from(b).toString("base64url");
}

export async function POST(req: NextRequest, { params }: { params: Promise<{ action: string }> }) {
  const { action } = await params;
  const path = POST_ACTIONS[action];
  if (!path) return error(404, "not_found", "Not found.");
  if (!sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
  if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");

  const body = action === "logout" ? {} : await req.json().catch(() => null);
  if (body === null || typeof body !== "object") return error(400, "bad_request", "Invalid request body.");

  let device = req.cookies.get(STAFF_DEVICE_COOKIE)?.value;
  const mintDevice = !device;
  if (!device) device = newDeviceId();

  const r = await gateway<Record<string, unknown> & { session?: Session }>(path, {
    body,
    ip: clientIp(req.headers),
    userAgent: req.headers.get("user-agent"),
    device,
    token: action === "logout" ? req.cookies.get(STAFF_COOKIE)?.value : undefined,
  });

  const { session, ...data } = r.data;
  const res = NextResponse.json(data, { status: r.status, headers: { "cache-control": "no-store" } });
  if (mintDevice) res.cookies.set(STAFF_DEVICE_COOKIE, device, { httpOnly: true, secure: PROD, sameSite: "lax", path: "/", maxAge: 400 * 24 * 3600 });
  if (session?.token) {
    const maxAge = Math.max(60, Math.floor((new Date(session.expires_at).getTime() - Date.now()) / 1000));
    res.cookies.set(STAFF_COOKIE, session.token, { httpOnly: true, secure: PROD, sameSite: "lax", path: "/", maxAge });
  }
  if (action === "logout") await forgetStaff(req.cookies.get(STAFF_COOKIE)?.value);
  if (action === "logout") res.cookies.delete(STAFF_COOKIE);
  return res;
}

export async function GET(req: NextRequest, { params }: { params: Promise<{ action: string }> }) {
  const { action } = await params;
  const token = req.cookies.get(STAFF_COOKIE)?.value;
  if (action === "invite") {
    const t = req.nextUrl.searchParams.get("token") ?? "";
    if (!/^[A-Za-z0-9_-]{20,128}$/.test(t)) return error(410, "invite_expired", "This invite link is invalid or has expired.");
    const r = await gateway(`/v1/admin/auth/invite?token=${encodeURIComponent(t)}`, { ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
    return NextResponse.json(r.data, { status: r.status, headers: { "cache-control": "no-store" } });
  }
  if (action === "me") {
    if (!token) return error(401, "unauthorized", "Please sign in.");
    const r = await gateway("/v1/admin/auth/me", { token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
    const res = NextResponse.json(r.data, { status: r.status, headers: { "cache-control": "no-store" } });
    if (r.status === 401) res.cookies.delete(STAFF_COOKIE);
    return res;
  }
  if (action === "expired") {
    const next = safeNext(req.nextUrl.searchParams.get("next"), "");
    if (token) {
      // only clear a cookie that really is dead, so a cross-site link can't sign someone out
      const r = await gateway("/v1/admin/auth/me", { token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
      // 403 = blocked session (IP allow-list, suspended broker): it's dead for this browser too
      if (r.status !== 401 && r.status !== 403) return NextResponse.redirect(publicUrl(req, next || "/"));
    }
    const url = publicUrl(req, "/login");
    if (next) url.searchParams.set("next", next);
    const res = NextResponse.redirect(url);
    res.cookies.delete(STAFF_COOKIE);
    return res;
  }
  return error(404, "not_found", "Not found.");
}

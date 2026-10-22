import { NextResponse, type NextRequest } from "next/server";
import { publicUrl } from "@kalks/ui/public-url";
import { clientIp, edgeCountry, gateway, setSessionCookie } from "@/lib/gateway";

// GET /api/auth/impersonate?ticket=<one-time, 60 s>: a staff member opens the Client Area as a client from the Back
// Office ("Log in as client", gateway client_controls.rs). The ticket becomes a 30-minute staff session in this
// browser's session cookie; the staff member's own Back Office session is separate and unaffected. The client's
// password is never involved. Read-only staff sessions are held to reads by proxy.ts and refused by the gateway.
export const dynamic = "force-dynamic";

const TICKET_RE = /^[A-Za-z0-9_-]{16,128}$/;

export async function GET(req: NextRequest) {
  const ticket = req.nextUrl.searchParams.get("ticket") ?? "";
  const fail = () => {
    const res = NextResponse.redirect(publicUrl(req, "/staff-session?state=expired"), 303);
    res.headers.set("referrer-policy", "no-referrer");
    return res;
  };
  if (!TICKET_RE.test(ticket)) return fail();
  const r = await gateway<{ session?: { token: string; expires_at: string } }>("/v1/auth/impersonate/redeem", {
    body: { ticket },
    ip: clientIp(req.headers),
    userAgent: req.headers.get("user-agent"),
    country: edgeCountry(req.headers),
  });
  if (r.status !== 200 || !r.data.session?.token) return fail();
  // a session this browser had before (e.g. the staff member's own test client) is replaced, not signed out
  const res = NextResponse.redirect(publicUrl(req, "/"), 303);
  setSessionCookie(res, r.data.session);
  res.headers.set("cache-control", "no-store");
  res.headers.set("referrer-policy", "no-referrer");
  return res;
}

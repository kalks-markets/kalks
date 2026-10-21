import { NextResponse, type NextRequest } from "next/server";
import { STAFF_COOKIE, clientIp, gateway } from "@/lib/gateway";
import { apiError, mutationAllowed, withInviteUrl } from "@/lib/bff";

// Back Office admin BFF: browser -> /api/admin/<path> (same origin, staff cookie) -> gateway /v1/admin/<path>.
// The gateway checks the staff session, the IP allow-list and the role's permission on every call; this layer
// only forwards allow-listed paths (CSRF-checked for mutations).

type Method = "GET" | "POST" | "PATCH" | "PUT" | "DELETE";
const ID = "\\d{1,18}";
/** A section key: `page.wallet.deposit`, `dashboard.wallet_card`. */
const SECTION = "[a-z][a-z0-9_]*(?:\\.[a-z0-9_]+){1,4}";
const PATHS: Record<Method, RegExp[]> = {
  GET: [
    /^stats$/, /^users$/, new RegExp(`^users/${ID}$`), /^audit$/, /^staff$/, new RegExp(`^staff/${ID}$`), /^sessions$/,
    /^roles$/, /^permissions$/, /^security\/ip$/, /^settings\/maintenance$/, /^settings\/features$/,
    new RegExp(`^users/${ID}/security$`), /^requests$/, /^settings\/sessions$/,
    // Client Area layout: sections and their targeting rules (gateway sections.rs)
    /^settings\/sections$/, /^settings\/sections\/preview$/,
  ],
  POST: [
    new RegExp(`^sessions/${ID}/revoke$`), /^staff\/invite$/, new RegExp(`^staff/${ID}/(resend-invite|disable|enable|reset-2fa|sign-out)$`),
    /^roles$/, new RegExp(`^roles/${ID}/reset$`), /^security\/ip$/,
    new RegExp(`^users/${ID}/sessions/revoke-all$`), new RegExp(`^users/${ID}/viewers/${ID}/revoke$`), new RegExp(`^requests/${ID}$`),
    new RegExp(`^settings/sections/${SECTION}/rules$`),
    // erase a closed client's personal data (gateway client_erase.rs, clients.erase)
    new RegExp(`^users/${ID}/erase$`),
  ],
  PATCH: [new RegExp(`^staff/${ID}$`), new RegExp(`^roles/${ID}$`)],
  PUT: [/^security\/ip\/settings$/, /^settings\/maintenance$/, /^settings\/features\/[a-z][a-z0-9_]{1,47}$/, /^settings\/sessions$/, new RegExp(`^settings/sections/${SECTION}$`)],
  DELETE: [new RegExp(`^roles/${ID}$`), new RegExp(`^security/ip/${ID}$`), new RegExp(`^settings/sections/rules/${ID}$`)],
};

async function forward(req: NextRequest, parts: string[], method: Method) {
  const path = parts.join("/");
  if (!PATHS[method].some((re) => re.test(path))) return apiError(404, "not_found", "Not found.");
  const token = req.cookies.get(STAFF_COOKIE)?.value;
  if (!token) return apiError(401, "unauthorized", "Please sign in.");
  let body: unknown;
  if (method !== "GET") {
    const blocked = mutationAllowed(req);
    if (blocked) return blocked;
    body = await req.json().catch(() => ({}));
    if (body === null || typeof body !== "object") return apiError(400, "bad_request", "Invalid request body.");
  }
  const qs = method === "GET" ? req.nextUrl.search : "";
  const r = await gateway(`/v1/admin/${path}${qs}`, { method, body, token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
  return NextResponse.json(withInviteUrl(req, r.data), { status: r.status, headers: { "cache-control": "no-store" } });
}

type Ctx = { params: Promise<{ path: string[] }> };
export const GET = async (req: NextRequest, { params }: Ctx) => forward(req, (await params).path, "GET");
export const POST = async (req: NextRequest, { params }: Ctx) => forward(req, (await params).path, "POST");
export const PATCH = async (req: NextRequest, { params }: Ctx) => forward(req, (await params).path, "PATCH");
export const PUT = async (req: NextRequest, { params }: Ctx) => forward(req, (await params).path, "PUT");
export const DELETE = async (req: NextRequest, { params }: Ctx) => forward(req, (await params).path, "DELETE");

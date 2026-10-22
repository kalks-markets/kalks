import { NextResponse, type NextRequest } from "next/server";
import { clientIp, gateway } from "@/lib/gateway";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { prop, propConfigured, propRaw, type PropMethod } from "@/lib/prop";
import { propAllows, type PropPerm } from "@/lib/prop-perms";

// Prop BFF: browser -> /api/prop/<path> (same origin, staff cookie) -> prop service /v1/admin/<path>.
// The staff session is verified with the gateway on every call; the permission for the route is checked here
// (lib/prop-perms.ts), then the service is called with PROP_INTERNAL_TOKEN and the staff identity headers built
// from that verified session. The service checks the role again and writes its audit log.

type Route = { method: PropMethod; re: RegExp; perm: PropPerm };

const ID = "(\\d{1,18})";
const PLAN = "[a-z0-9-]{2,48}";
const CODE = "[A-Za-z0-9_-]{4,40}";
const ROUTES: Route[] = [
  // reads
  { method: "GET", re: /^(overview|plans|engine-groups|challenges|events|payouts|certificates|flags|news|audit|settings)$/, perm: "prop.read" },
  { method: "GET", re: new RegExp(`^challenges/${ID}$`), perm: "prop.read" },
  // plan builder, overrides, scaling, flag review, news calendar, certificates
  { method: "POST", re: /^plans$/, perm: "prop.write" },
  { method: "PUT", re: new RegExp(`^plans/${PLAN}$`), perm: "prop.write" },
  { method: "POST", re: new RegExp(`^plans/${PLAN}/status$`), perm: "prop.write" },
  { method: "POST", re: new RegExp(`^challenges/${ID}/override$`), perm: "prop.write" },
  { method: "POST", re: new RegExp(`^accounts/${ID}/scale$`), perm: "prop.write" },
  { method: "POST", re: new RegExp(`^flags/${ID}/review$`), perm: "prop.write" },
  { method: "POST", re: /^news$/, perm: "prop.write" },
  { method: "DELETE", re: new RegExp(`^news/${ID}$`), perm: "prop.write" },
  { method: "POST", re: new RegExp(`^certificates/${CODE}/revoke$`), perm: "prop.write" },
  // prop settings (four-eyes threshold; audited by the service)
  { method: "PUT", re: /^settings$/, perm: "prop.write" },
  // payouts (four-eyes: the service enforces the second approver)
  { method: "POST", re: new RegExp(`^payouts/${ID}/(approve|reject)$`), perm: "prop.approve" },
];

const json = (data: unknown, status = 200) => NextResponse.json(data, { status, headers: { "cache-control": "no-store" } });
const fwd = (req: NextRequest) => ({ ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });

async function handle(req: NextRequest, parts: string[], method: PropMethod) {
  const path = parts.map((p) => decodeURIComponent(p)).join("/");
  if (!propConfigured()) return apiError(503, "not_configured", "The prop service is not configured for the Back Office.");

  // special routes first
  if (method === "GET" && path === "clients") return clientNames(req);
  const svg = method === "GET" ? path.match(new RegExp(`^public/certificates/(${CODE})/image\\.svg$`)) : null;
  if (svg) return certificateSvg(req, svg[1]!);
  const series = method === "GET" ? path.match(new RegExp(`^challenges/${ID}/(equity|trades)$`)) : null;
  if (series) return challengeSeries(req, series[1]!, series[2]!);

  const route = ROUTES.find((r) => r.method === method && r.re.test(path));
  if (!route) return apiError(404, "not_found", "Not found.");

  let body: Record<string, unknown> | undefined;
  if (method !== "GET") {
    const blocked = mutationAllowed(req);
    if (blocked) return blocked;
    const raw = await req.json().catch(() => null);
    if (raw === null || typeof raw !== "object" || Array.isArray(raw)) return apiError(400, "bad_request", "Invalid request body.");
    body = raw as Record<string, unknown>;
  }
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!propAllows(who.staff, route.perm)) return apiError(403, "forbidden", "Your role doesn't allow this.");

  // Payout approval: forward the client's current KYC status from the gateway (the service also checks its own
  // gateway DB connection when it has one). Never taken from the browser.
  const approve = method === "POST" ? path.match(new RegExp(`^payouts/${ID}/approve$`)) : null;
  if (approve && body) {
    delete body.kycStatus;
    const kyc = await payoutKyc(req, who, approve[1]!);
    if (kyc) body.kycStatus = kyc;
  }

  const target = `/v1/admin/${path}${method === "GET" ? req.nextUrl.search : ""}`;
  const r = await prop(target, { method, body: method === "DELETE" ? undefined : body, staff: who.staff, ...fwd(req) });
  return json(r.data, r.status);
}

type Who = Exclude<Awaited<ReturnType<typeof requireStaff>>, NextResponse>;

/** KYC status of the payout's client, from the gateway admin user endpoint (needs clients.read). */
async function payoutKyc(req: NextRequest, who: Who, payoutId: string): Promise<string | null> {
  const list = await prop<{ payouts?: { id: number; userId: number }[] }>("/v1/admin/payouts", { staff: who.staff, ...fwd(req) });
  const p = list.data?.payouts?.find((x) => String(x.id) === payoutId);
  if (!p) return null;
  const u = await gateway<{ user?: { kyc_status?: string } }>(`/v1/admin/users/${p.userId}`, { token: who.token, ...fwd(req) });
  return u.status === 200 ? (u.data.user?.kyc_status ?? null) : null;
}

/** Equity curve / trade list of a challenge. The client routes are scoped to the owner, so the owner is looked up first. */
async function challengeSeries(req: NextRequest, id: string, kind: string) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!propAllows(who.staff, "prop.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const c = await prop<{ userId?: number }>(`/v1/admin/challenges/${id}`, { staff: who.staff, ...fwd(req) });
  if (c.status !== 200 || !c.data?.userId) return json(c.data, c.status === 200 ? 502 : c.status);
  const r = await prop(`/v1/challenges/${id}/${kind}${req.nextUrl.search}`, { userId: c.data.userId, ...fwd(req) });
  return json(r.data, r.status);
}

/** Branded certificate image (SVG) — the Back Office has no public verify page, so it is proxied here. */
async function certificateSvg(req: NextRequest, code: string) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!propAllows(who.staff, "prop.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const r = await propRaw(`/v1/public/certificates/${encodeURIComponent(code)}/image.svg`, { staff: who.staff, ...fwd(req) });
  if (!r) return apiError(503, "unavailable", "The prop service is unavailable. Please try again shortly.");
  if (r.status !== 200) return apiError(r.status === 404 ? 404 : 502, r.status === 404 ? "not_found" : "bad_gateway", "Certificate image not available.");
  return new NextResponse(r.body, { status: 200, headers: { "content-type": "image/svg+xml; charset=utf-8", "cache-control": "private, no-store", "content-security-policy": "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src data:", "x-content-type-options": "nosniff" } });
}

/* ---------------- client names (gateway) ---------------- */

const NAME_TTL = 5 * 60_000;
const names = new Map<string, { name: string; email: string; kyc: string; at: number }>();

/** `?ids=1,2,3` → `{ names: { "1": { name, email, kyc } } }` from the gateway (needs clients.read). */
async function clientNames(req: NextRequest) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!propAllows(who.staff, "prop.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const ids = Array.from(new Set((req.nextUrl.searchParams.get("ids") ?? "").split(",").filter((x) => /^\d{1,18}$/.test(x)))).slice(0, 60);
  const out: Record<string, { name: string; email: string; kyc: string }> = {};
  if (!who.staff.permissions?.includes("clients.read")) return json({ names: out });
  const now = Date.now();
  await Promise.all(
    ids.map(async (id) => {
      const hit = names.get(id);
      if (hit && now - hit.at < NAME_TTL) {
        out[id] = { name: hit.name, email: hit.email, kyc: hit.kyc };
        return;
      }
      const r = await gateway<{ user?: { name: string; email: string; kyc_status?: string } }>(`/v1/admin/users/${id}`, { token: who.token, ...fwd(req) });
      if (r.status === 200 && r.data.user) {
        const v = { name: r.data.user.name, email: r.data.user.email, kyc: r.data.user.kyc_status ?? "unverified" };
        names.set(id, { ...v, at: now });
        out[id] = v;
      }
    }),
  );
  if (names.size > 5000) names.clear();
  return json({ names: out });
}

type Ctx = { params: Promise<{ path: string[] }> };
export async function GET(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "GET");
}
export async function POST(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "POST");
}
export async function PUT(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "PUT");
}
export async function DELETE(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "DELETE");
}

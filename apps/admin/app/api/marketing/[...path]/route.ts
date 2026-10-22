import { NextResponse, type NextRequest } from "next/server";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { growth, growthConfigured } from "@/lib/growth";
import { marketingAllow, type MarketingPerm } from "@/lib/marketing-perms";
import { reportsFetch } from "@/lib/reports";

// Marketing BFF: browser -> /api/marketing/<path> (same origin, staff cookie) -> growth service /v1/growth/admin/<path>.
// The staff session is verified with the gateway on every call and the route's permission is checked here
// (lib/marketing-perms.ts); the growth service checks the role again and writes the audit log.

type Method = "GET" | "POST" | "PUT" | "PATCH";
type Route = { method: Method; re: RegExp; perm: MarketingPerm };

const ID = "(\\d{1,18})";
const R = (s: string) => new RegExp(`^${s}$`);
const ROUTES: Route[] = [
  // reads
  { method: "GET", re: /^(overview|settings|tiers|rules|catalogue|redemptions|members|reports|audit)$/, perm: "marketing.read" },
  { method: "GET", re: /^cashback\/(programmes|accruals|payouts)$/, perm: "marketing.read" },
  { method: "GET", re: /^bonuses\/(campaigns|grants)$/, perm: "marketing.read" },
  { method: "GET", re: /^promos(\/redemptions)?$/, perm: "marketing.read" },
  { method: "GET", re: /^banners(\/preview)?$/, perm: "marketing.read" },
  { method: "GET", re: /^contests$/, perm: "marketing.read" },
  // challenges (phase 4): templates, missions, levels, payout approvals
  { method: "GET", re: /^(contest-templates|missions|levels|payouts)$/, perm: "marketing.read" },
  { method: "GET", re: R(`contests/${ID}`), perm: "marketing.read" },
  { method: "GET", re: /^journeys(\/meta)?$/, perm: "marketing.read" },
  { method: "GET", re: R(`journeys/${ID}(/enrollments|/events)?`), perm: "marketing.read" },
  // configuration writes
  { method: "PUT", re: /^(settings|tiers|levels)$/, perm: "marketing.write" },
  { method: "POST", re: /^(contest-templates|missions|xp\/adjust)$/, perm: "marketing.write" },
  { method: "PATCH", re: R(`(contest-templates|missions)/${ID}`), perm: "marketing.write" },
  { method: "POST", re: R(`contest-templates/${ID}/contest`), perm: "marketing.write" },
  { method: "POST", re: R(`contests/${ID}/(duplicate|save-template)`), perm: "marketing.write" },
  { method: "POST", re: /^(rules|catalogue|cashback\/programmes|bonuses\/campaigns|promos|banners|contests)$/, perm: "marketing.write" },
  { method: "PATCH", re: R(`(rules|catalogue|cashback/programmes|bonuses/campaigns|promos|banners|contests)/${ID}`), perm: "marketing.write" },
  { method: "POST", re: R(`contests/${ID}/(cancel|refresh|finalize)`), perm: "marketing.write" },
  { method: "POST", re: R(`contests/${ID}/entries/${ID}/(disqualify|reinstate)`), perm: "marketing.write" },
  { method: "POST", re: R(`contests/${ID}/flags/${ID}/resolve`), perm: "marketing.write" },
  { method: "POST", re: /^run\/(profiles|deals|bonus|contests|payouts|reversals|expiry|journeys|missions)$/, perm: "marketing.write" },
  { method: "POST", re: /^journeys$/, perm: "marketing.write" },
  { method: "PATCH", re: R(`journeys/${ID}`), perm: "marketing.write" },
  { method: "POST", re: R(`journeys/${ID}/(status|test)`), perm: "marketing.write" },
  // money out
  { method: "POST", re: R(`contests/${ID}/pay`), perm: "marketing.approve" },
  { method: "POST", re: /^bonuses\/grants$/, perm: "marketing.approve" },
  { method: "POST", re: R(`bonuses/grants/${ID}/cancel`), perm: "marketing.approve" },
  { method: "POST", re: /^points\/adjust$/, perm: "marketing.approve" },
  { method: "POST", re: /^cashback\/payouts\/run$/, perm: "marketing.approve" },
  { method: "POST", re: R(`redemptions/${ID}/(retry|approve|reject)`), perm: "marketing.approve" },
  { method: "POST", re: R(`missions/rewards/${ID}/(approve|reject)`), perm: "marketing.approve" },
  // four-eyes: one prize, a held cashback payout
  { method: "POST", re: R(`contests/${ID}/prizes/${ID}/(approve|reject)`), perm: "marketing.approve" },
  { method: "POST", re: R(`cashback/payouts/${ID}/(approve|reject)`), perm: "marketing.approve" },
];

const QUERY_KEYS = ["status", "campaign", "user", "promo", "programme", "page", "limit", "q", "from", "to", "placement", "country", "kyc", "accountType", "signupDays", "enrollment"];

async function handle(req: NextRequest, parts: string[], method: Method) {
  const path = parts.map((p) => decodeURIComponent(p)).join("/");
  if (!growthConfigured()) return apiError(503, "not_configured", "The marketing service is not configured for the Back Office.");
  if (method === "GET" && path === "me") return perms(req);
  if (method === "GET" && path === "campaigns") return campaigns(req);
  const route = ROUTES.find((r) => r.method === method && r.re.test(path));
  if (!route) return apiError(404, "not_found", "Not found.");

  let body: unknown;
  if (method !== "GET") {
    const blocked = mutationAllowed(req);
    if (blocked) return blocked;
    body = await req.json().catch(() => null);
    if (body === null || typeof body !== "object" || Array.isArray(body)) return apiError(400, "bad_request", "Invalid request body.");
  }
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!marketingAllow(who.staff, route.perm)) return apiError(403, "forbidden", "Your role doesn't allow this.");

  const q = new URLSearchParams();
  for (const k of QUERY_KEYS) {
    const v = req.nextUrl.searchParams.get(k);
    if (v && v.length <= 80) q.set(k, v);
  }
  // test sends go to the signed-in staff member's own address (never an address from the browser)
  if (method === "POST" && /^journeys\/\d+\/test$/.test(path)) body = { ...(body as object), to: who.staff.email };
  const target = `/v1/growth/admin/${path}${method === "GET" && q.size ? `?${q}` : ""}`;
  const r = await growth(target, { method, body, staff: who.staff });
  return NextResponse.json(r.data, { status: r.status, headers: { "cache-control": "no-store" } });
}

/** UTM campaign attribution from the reports service (sign-ups, FTDs and deposits by utm source / medium / campaign). */
async function campaigns(req: NextRequest) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!marketingAllow(who.staff, "marketing.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const q = new URLSearchParams();
  for (const k of ["from", "to"]) {
    const v = req.nextUrl.searchParams.get(k);
    if (v && /^[0-9TZ:.+-]{8,40}$/.test(v)) q.set(k, v);
  }
  const res = await reportsFetch(`/v1/admin/campaigns${q.size ? `?${q}` : ""}`, { staff: who.staff, perms: ["marketing.read"], timeoutMs: 60_000 });
  if (!res) return apiError(503, "unavailable", "The reports service is unavailable. Please try again shortly.");
  const data = await res.json().catch(() => ({ error: { code: "bad_gateway", message: "The reports service returned an unexpected response." } }));
  return NextResponse.json(data, { status: res.status, headers: { "cache-control": "no-store" } });
}

/** The caller's Marketing permissions, so the UI can hide what the role can't use. */
async function perms(req: NextRequest) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  return NextResponse.json(
    { read: marketingAllow(who.staff, "marketing.read"), write: marketingAllow(who.staff, "marketing.write"), approve: marketingAllow(who.staff, "marketing.approve") },
    { headers: { "cache-control": "no-store" } },
  );
}

type Ctx = { params: Promise<{ path: string[] }> };
export const GET = async (req: NextRequest, { params }: Ctx) => handle(req, (await params).path, "GET");
export const POST = async (req: NextRequest, { params }: Ctx) => handle(req, (await params).path, "POST");
export const PUT = async (req: NextRequest, { params }: Ctx) => handle(req, (await params).path, "PUT");
export const PATCH = async (req: NextRequest, { params }: Ctx) => handle(req, (await params).path, "PATCH");

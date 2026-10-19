import { NextResponse, type NextRequest } from "next/server";
import { growth } from "@/lib/growth";
import { sameOrigin, sessionUser } from "@/lib/trading";

// Rewards & marketing BFF. Browser -> /api/growth/<route> (same origin) -> growth service /v1/growth/me/…
// The client is resolved from the HttpOnly gateway session cookie; the service gets that user id in
// X-Kalks-User-Id plus the segment headers (country, KYC, sign-up date, name, referral code). A user id sent by
// the browser is never used. Mutations must be same-origin JSON. Routes (services/growth/README.md):
//
//   GET   rewards                         points, tier, tiers, earn rules, catalogue, recent, series
//   GET   points?kind=&page=&limit=       points history
//   POST  redeem                          {itemId, login?}
//   GET   redemptions · vouchers
//   GET   cashback                        programmes, totals, accruals, payouts, series
//   POST  cashback/{programmeId}/enrol    {}
//   GET   promotions                      campaigns, grants, promo history
//   POST  bonuses/{campaignId}/claim      {login?}
//   POST  promo                           {code, login?}
//   GET   contests · contests/{id}
//   POST  contests/{id}/join              {login?} (demo: credentials returned once, passed through)
//   GET   banners?placement=
//   POST  banners/{id}/events             {kind}
//   GET   shares · POST shares            {kind, login, dealId | from/to, showAmounts}
//   GET   hub                             level, XP, streaks, missions, active contests, recent rewards, nickname (phase 4)
//   GET   missions · xp                   missions for the current periods · level, streaks, XP history
//   GET   nickname · POST nickname        {nickname?, public?} the leaderboard nickname and the public-website choice
//   GET   leaderboard?contest=|period=   nicknames, ranks, scores, level badges; the client's own row as `me`

type Ctx = { params: Promise<{ path?: string[] }> };
const NO_STORE = { "cache-control": "no-store" };

function error(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status, headers: NO_STORE });
}

const ID = String.raw`[A-Za-z0-9_-]{1,64}`;
const GET_ROUTES = [/^rewards$/, /^points$/, /^redemptions$/, /^vouchers$/, /^cashback$/, /^promotions$/, /^contests$/, new RegExp(`^contests/${ID}$`), /^banners$/, /^shares$/, /^hub$/, /^missions$/, /^xp$/, /^nickname$/, /^leaderboard$/];
const POST_ROUTES = [
  /^redeem$/,
  new RegExp(`^cashback/${ID}/enrol$`),
  new RegExp(`^bonuses/${ID}/claim$`),
  /^promo$/,
  new RegExp(`^contests/${ID}/join$`),
  new RegExp(`^banners/${ID}/events$`),
  /^shares$/,
  /^nickname$/,
];
const QUERY_KEYS = ["placement", "kind", "page", "limit", "contest", "period"];

async function handle(req: NextRequest, ctx: Ctx, method: "GET" | "POST") {
  const path = ((await ctx.params).path ?? []).join("/");
  const allowed = (method === "GET" ? GET_ROUTES : POST_ROUTES).some((r) => r.test(path));
  if (!allowed) return error(404, "not_found", "Not found.");
  let body: unknown;
  if (method !== "GET") {
    if (!sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
    if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");
    body = await req.json().catch(() => null);
    if (!body || typeof body !== "object" || Array.isArray(body)) return error(400, "bad_request", "Invalid request body.");
  }
  const user = await sessionUser(req);
  if (user === "unavailable") return error(503, "unavailable", "Sign-in service is unavailable. Please try again shortly.");
  if (!user) return error(401, "unauthorized", "Please sign in.");

  const q = new URLSearchParams();
  for (const k of QUERY_KEYS) {
    const v = req.nextUrl.searchParams.get(k);
    if (v && v.length <= 60) q.set(k, v);
  }
  const target = `/v1/growth/me/${path}${method === "GET" && q.size ? `?${q}` : ""}`;
  const r = await growth<Record<string, unknown>>(target, { method, body, user });
  return NextResponse.json(r.data, { status: r.status, headers: NO_STORE });
}

export const GET = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "GET");
export const POST = (req: NextRequest, ctx: Ctx) => handle(req, ctx, "POST");

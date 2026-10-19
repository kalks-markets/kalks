import { NextResponse, type NextRequest } from "next/server";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { contentAllows } from "@/lib/academy";
import { PREVIEW_BASES, contentAdmin, contentPerms, contentUpload } from "@/lib/content-server";
import { pagePath } from "@/lib/content-catalogue";
import { UPLOAD_MAX, isPage, matchRoute, type Method, type Perm } from "@/lib/content-routes";

// Back Office site content BFF: browser -> /api/content/<path> (same origin, staff cookie) -> services/content.
// The route table, its permissions and parameter checks live in lib/content-routes.ts; this file authenticates,
// guards mutations (same origin; JSON bodies or raw images within size limits) and forwards.

type Ctx = { params: Promise<{ path: string[] }> };
const NO_STORE = { "cache-control": "no-store" };
const reply = (r: { status: number; data: unknown }) => NextResponse.json(r.data, { status: r.status, headers: NO_STORE });

async function gate(req: NextRequest, perm: Perm | null) {
  const s = await requireStaff(req);
  if (s instanceof NextResponse) return s;
  if (perm && !contentAllows(s.staff, perm)) return apiError(403, "forbidden", "Your role doesn't allow this.");
  return s.staff;
}

/** Same-origin check for raw uploads (mutationAllowed wants a JSON body). */
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

async function jsonBody(req: NextRequest, max: number): Promise<Record<string, unknown> | NextResponse> {
  const blocked = mutationAllowed(req);
  if (blocked) return blocked;
  if (Number(req.headers.get("content-length") ?? "0") > max) return apiError(413, "too_large", "Request is too large.");
  const text = await req.text().catch(() => "");
  if (new TextEncoder().encode(text).byteLength > max) return apiError(413, "too_large", "Request is too large.");
  let b: unknown;
  try {
    b = JSON.parse(text);
  } catch {
    b = null;
  }
  if (!b || typeof b !== "object" || Array.isArray(b)) return apiError(400, "bad_request", "Invalid request body.");
  return b as Record<string, unknown>;
}

const IMAGE_TYPES = ["image/png", "image/jpeg", "image/webp", "image/svg+xml", "application/octet-stream", ""];

async function handle(req: NextRequest, method: Method, ctx: Ctx) {
  const path = (await ctx.params).path;
  const route = matchRoute(method, path, req.nextUrl.searchParams);
  if (route === null) return apiError(404, "not_found", "Not found.");
  if (route === "bad") return apiError(422, "validation", "Invalid parameters.");

  if (route.kind === "me") {
    const staff = await gate(req, null);
    if (staff instanceof NextResponse) return staff;
    const p = contentPerms(staff);
    return NextResponse.json({ read: p.includes("content.read"), write: p.includes("content.write"), publish: p.includes("content.publish"), sites: PREVIEW_BASES }, { headers: NO_STORE });
  }

  if (route.kind === "preview") {
    const b = await jsonBody(req, 4096);
    if (b instanceof NextResponse) return b;
    const staff = await gate(req, route.perm);
    if (staff instanceof NextResponse) return staff;
    const surface = b.surface;
    const page = typeof b.page === "string" && isPage(b.page) ? b.page : null;
    if (surface !== "website" && surface !== "app" && surface !== "trader") return apiError(422, "validation", "Previews are available for the website, Client Area and Kalks Trader.");
    const r = await contentAdmin("/v1/admin/content/preview-token", { method: "POST", body: { surface }, staff });
    const token = (r.data as { token?: string })?.token;
    if (r.status !== 200 || !token) return reply(r);
    const url = `${PREVIEW_BASES[surface]}/api/content-preview?token=${encodeURIComponent(token)}&path=${encodeURIComponent(page ? pagePath(surface, page) : "/")}`;
    return NextResponse.json({ url, expires_at: (r.data as { expires_at?: string }).expires_at ?? null }, { headers: NO_STORE });
  }

  // proxy
  if (route.body === "upload") {
    if (!sameOrigin(req)) return apiError(403, "forbidden", "Cross-site request blocked.");
    const type = (req.headers.get("content-type") ?? "").split(";")[0]!.trim().toLowerCase();
    if (!IMAGE_TYPES.includes(type)) return apiError(415, "unsupported_type", "Upload a PNG, JPEG, WebP or SVG image.");
    const max = route.maxBytes ?? UPLOAD_MAX;
    if (Number(req.headers.get("content-length") ?? "0") > max + 1024) return apiError(413, "too_large", "Images can be up to 10 MB.");
    const staff = await gate(req, route.perm);
    if (staff instanceof NextResponse) return staff;
    const bytes = await req.arrayBuffer().catch(() => null);
    if (!bytes || bytes.byteLength === 0) return apiError(400, "bad_request", "The file is empty.");
    if (bytes.byteLength > max) return apiError(413, "too_large", "Images can be up to 10 MB.");
    let name = "image";
    try {
      name = decodeURIComponent(req.headers.get("x-file-name") ?? "image").slice(0, 200) || "image";
    } catch {
      name = "image";
    }
    return reply(await contentUpload(route.target, { bytes, name, type, staff }));
  }

  if (route.body === "json") {
    const b = await jsonBody(req, route.maxBytes ?? 300 * 1024);
    if (b instanceof NextResponse) return b;
    const perm: Perm = route.publishIf && b[route.publishIf] === true ? "content.publish" : route.perm;
    const staff = await gate(req, perm);
    if (staff instanceof NextResponse) return staff;
    return reply(await contentAdmin(route.target, { method, body: b, staff, timeoutMs: route.target.endsWith("/import") ? 60_000 : 15_000 }));
  }

  if (method !== "GET") {
    // bodiless mutation (DELETE): same-origin only
    if (!sameOrigin(req)) return apiError(403, "forbidden", "Cross-site request blocked.");
  }
  const staff = await gate(req, route.perm);
  if (staff instanceof NextResponse) return staff;
  return reply(await contentAdmin(route.target, { method, staff }));
}

export async function GET(req: NextRequest, ctx: Ctx) {
  return handle(req, "GET", ctx);
}
export async function PUT(req: NextRequest, ctx: Ctx) {
  return handle(req, "PUT", ctx);
}
export async function POST(req: NextRequest, ctx: Ctx) {
  return handle(req, "POST", ctx);
}
export async function DELETE(req: NextRequest, ctx: Ctx) {
  return handle(req, "DELETE", ctx);
}

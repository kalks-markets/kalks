import { NextResponse, type NextRequest } from "next/server";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { contentAllows, type ContentPerm } from "@/lib/academy";
import { newsAdmin } from "@/lib/news-server";

// Back Office news + calendar BFF: browser -> /api/news/<path> (same origin, staff cookie) -> services/news /v1/admin/…
// content.read for reads, content.write for every change. Feed sources, the calendar and the daily brief are
// platform-wide: only the platform tenant may change, fetch / refresh or regenerate them (the service enforces it).
//
//   GET    news?status&source&q&limit&offset   stories with this tenant's pin / hide / retag state
//   PUT    news/{id}                           {pinned?, hidden?, symbols?, countries?, importance?}
//   DELETE news/{id}                           back to automatic tags, unpinned, visible
//   GET    sources · PUT sources/{id} {enabled} · POST sources/{id}/refresh
//   GET    calendar?from&to · PUT calendar/{id} {actual?, impact?} · POST calendar/refresh
//   POST   brief                                regenerate today's AI market brief
//   GET    stats · audit · brief (the current brief, same as clients see)

type Ctx = { params: Promise<{ path: string[] }> };
const ID = /^\d{1,15}$/;
const SLUG = /^[a-z0-9_-]{1,64}$/;

async function gate(req: NextRequest, perm: ContentPerm) {
  const s = await requireStaff(req);
  if (s instanceof NextResponse) return s;
  if (!contentAllows(s.staff, perm)) return apiError(403, "forbidden", "Your role doesn't allow this.");
  return s.staff;
}

const reply = (r: { status: number; data: unknown }) => NextResponse.json(r.data, { status: r.status, headers: { "cache-control": "no-store" } });

function query(req: NextRequest, keys: string[]) {
  const out = new URLSearchParams();
  for (const k of keys) {
    const v = req.nextUrl.searchParams.get(k);
    if (v) out.set(k, v.slice(0, 80));
  }
  const s = out.toString();
  return s ? `?${s}` : "";
}

export async function GET(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const staff = await gate(req, "content.read");
  if (staff instanceof NextResponse) return staff;
  const [a] = path;
  if (path.length !== 1) return apiError(404, "not_found", "Not found.");
  if (a === "news") return reply(await newsAdmin(`/v1/admin/news${query(req, ["status", "source", "q", "limit", "offset"])}`, { staff }));
  if (a === "sources" || a === "stats" || a === "audit") return reply(await newsAdmin(`/v1/admin/${a}`, { staff }));
  if (a === "calendar") return reply(await newsAdmin(`/v1/admin/calendar${query(req, ["from", "to"])}`, { staff }));
  if (a === "brief") return reply(await newsAdmin("/v1/brief", { staff }));
  return apiError(404, "not_found", "Not found.");
}

async function body(req: NextRequest) {
  const blocked = mutationAllowed(req);
  if (blocked) return blocked;
  const b = await req.json().catch(() => null);
  if (!b || typeof b !== "object" || Array.isArray(b)) return apiError(400, "bad_request", "Invalid request body.");
  if (JSON.stringify(b).length > 10_000) return apiError(413, "bad_request", "Request is too large.");
  return b as Record<string, unknown>;
}

export async function PUT(req: NextRequest, { params }: Ctx) {
  const [a, id, ...rest] = (await params).path;
  if (rest.length || !id) return apiError(404, "not_found", "Not found.");
  const target = a === "news" && ID.test(id) ? `/v1/admin/news/${id}` : a === "sources" && SLUG.test(id) ? `/v1/admin/sources/${id}` : a === "calendar" && ID.test(id) ? `/v1/admin/calendar/${id}` : null;
  if (!target) return apiError(404, "not_found", "Not found.");
  const b = await body(req);
  if (b instanceof NextResponse) return b;
  const staff = await gate(req, "content.write");
  if (staff instanceof NextResponse) return staff;
  return reply(await newsAdmin(target, { method: "PUT", body: b, staff }));
}

export async function POST(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const key = path.join("/");
  const target =
    key === "calendar/refresh" ? "/v1/admin/calendar/refresh" : key === "brief" ? "/v1/admin/brief" : path.length === 3 && path[0] === "sources" && SLUG.test(path[1]!) && path[2] === "refresh" ? `/v1/admin/sources/${path[1]}/refresh` : null;
  if (!target) return apiError(404, "not_found", "Not found.");
  const b = await body(req);
  if (b instanceof NextResponse) return b;
  const staff = await gate(req, "content.write");
  if (staff instanceof NextResponse) return staff;
  return reply(await newsAdmin(target, { method: "POST", body: b, staff, timeoutMs: key === "brief" ? 280_000 : 30_000 }));
}

export async function DELETE(req: NextRequest, { params }: Ctx) {
  const [a, id, ...rest] = (await params).path;
  if (a !== "news" || !id || !ID.test(id) || rest.length) return apiError(404, "not_found", "Not found.");
  const blocked = mutationAllowed(req);
  if (blocked) return blocked;
  const staff = await gate(req, "content.write");
  if (staff instanceof NextResponse) return staff;
  return reply(await newsAdmin(`/v1/admin/news/${id}`, { method: "DELETE", staff }));
}

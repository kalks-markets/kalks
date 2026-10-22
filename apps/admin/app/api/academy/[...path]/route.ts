import { NextResponse, type NextRequest } from "next/server";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { contentAllows, type ContentPerm } from "@/lib/academy";
import { academyAdmin } from "@/lib/academy-server";

// Back Office Academy CMS BFF: browser -> /api/academy/<path> (same origin, staff cookie) -> services/academy /v1/admin/…
// Permissions (lib/academy.ts): content.read for reads, content.write for every change.
//
//   GET    tree?lang                         phases > sections > chapters (+ publish state, source, learner counts)
//   GET    nodes/{kind}/{slug}?lang          full item incl. quiz answers; `default` = platform version when overridden
//   PUT    nodes/{kind}/{slug}               {lang, data?, published?, order?, parent?}
//   DELETE nodes/{kind}/{slug}?lang          reset to the platform default (drops this tenant's override); deletes a custom chapter
//   POST   chapters                          {lang, section, title, summary?, body?}  new draft chapter
//   POST   reorder                           {lang, kind, parent, slugs[]}
//   GET    stats?lang · audit · glossary?lang

type Ctx = { params: Promise<{ path: string[] }> };
const KINDS = new Set(["phase", "section", "chapter", "exam", "term"]);
const SLUG = /^[a-z0-9-]{1,96}$/;
const LANG = /^[a-z-]{2,5}$/;

function langQs(req: NextRequest) {
  const l = req.nextUrl.searchParams.get("lang");
  return l && LANG.test(l) ? `?lang=${l}` : "";
}

async function gate(req: NextRequest, perm: ContentPerm) {
  const s = await requireStaff(req);
  if (s instanceof NextResponse) return s;
  if (!contentAllows(s.staff, perm)) return apiError(403, "forbidden", "Your role doesn't allow this.");
  return s.staff;
}

const reply = (r: { status: number; data: unknown }) => NextResponse.json(r.data, { status: r.status, headers: { "cache-control": "no-store" } });

function nodePath(path: string[]) {
  return path.length === 3 && path[0] === "nodes" && KINDS.has(path[1]!) && SLUG.test(path[2]!) ? `/v1/admin/nodes/${path[1]}/${path[2]}` : null;
}

export async function GET(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const staff = await gate(req, "content.read");
  if (staff instanceof NextResponse) return staff;
  const one = path.length === 1 ? path[0] : null;
  if (one === "tree" || one === "stats" || one === "glossary") return reply(await academyAdmin(`/v1/admin/${one}${langQs(req)}`, { staff }));
  if (one === "audit") return reply(await academyAdmin("/v1/admin/audit", { staff }));
  const np = nodePath(path);
  if (np) return reply(await academyAdmin(`${np}${langQs(req)}`, { staff }));
  return apiError(404, "not_found", "Not found.");
}

async function body(req: NextRequest) {
  const blocked = mutationAllowed(req);
  if (blocked) return blocked;
  const b = await req.json().catch(() => null);
  if (!b || typeof b !== "object" || Array.isArray(b)) return apiError(400, "bad_request", "Invalid request body.");
  if (JSON.stringify(b).length > 200_000) return apiError(413, "bad_request", "Content is too large.");
  return b as Record<string, unknown>;
}

export async function PUT(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const np = nodePath(path);
  if (!np) return apiError(404, "not_found", "Not found.");
  const b = await body(req);
  if (b instanceof NextResponse) return b;
  const staff = await gate(req, "content.write");
  if (staff instanceof NextResponse) return staff;
  return reply(await academyAdmin(np, { method: "PUT", body: b, staff }));
}

export async function POST(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const one = path.length === 1 ? path[0] : null;
  if (one !== "chapters" && one !== "reorder") return apiError(404, "not_found", "Not found.");
  const b = await body(req);
  if (b instanceof NextResponse) return b;
  const staff = await gate(req, "content.write");
  if (staff instanceof NextResponse) return staff;
  return reply(await academyAdmin(`/v1/admin/${one}`, { method: "POST", body: b, staff }));
}

export async function DELETE(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  const np = nodePath(path);
  if (!np) return apiError(404, "not_found", "Not found.");
  const blocked = mutationAllowed(req);
  if (blocked) return blocked; // the client sends content-type: application/json with an empty body
  const staff = await gate(req, "content.write");
  if (staff instanceof NextResponse) return staff;
  return reply(await academyAdmin(`${np}${langQs(req)}`, { method: "DELETE", staff }));
}

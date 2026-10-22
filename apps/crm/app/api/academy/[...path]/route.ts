import { NextResponse, type NextRequest } from "next/server";
import { CERT_CODE as CODE, academy, certificateImage } from "@/lib/academy";
import { sameOrigin, sessionUser } from "@/lib/trading";

// Client Area Academy BFF. Browser -> /api/academy/<route> (same origin) -> services/academy /v1/…
// The learner is resolved from the HttpOnly gateway session cookie; a user id from the browser is never used.
// Quizzes and exams are graded by the service (the browser never receives answers before submitting).
//
//   GET  catalog?lang                        phases > sections > chapters with my progress, exam state, certificates
//   GET  chapters/{slug}?lang                chapter (markdown body, takeaways, quiz without answers), prev / next
//   POST chapters/{slug}/progress            {read_pct}
//   POST chapters/{slug}/quiz                {answers: (number|null)[]}  instant feedback; recorded when all answered
//   GET  exams/{phase}                       final exam (no answers) + unlocked + attempts + certificate
//   POST exams/{phase}                       {answers: number[]}  pass issues the phase certificate
//   GET  me/certificates
//   GET  glossary?q&category&lang
//   GET  certificates/{code}                 public verification (no sign-in)
//   GET  certificates/{code}/image           public certificate image (SVG)

type Ctx = { params: Promise<{ path: string[] }> };

const NO_STORE = { "cache-control": "no-store" };
const SLUG = /^[a-z0-9-]{1,96}$/;

function error(status: number, code: string, message: string) {
  return NextResponse.json({ error: { code, message } }, { status, headers: NO_STORE });
}

function reply(r: { status: number; data: unknown }) {
  return NextResponse.json(r.data, { status: r.status, headers: NO_STORE });
}

function query(req: NextRequest, keys: string[]) {
  const out = new URLSearchParams();
  for (const k of keys) {
    const v = req.nextUrl.searchParams.get(k);
    if (v) out.set(k, v.slice(0, 80));
  }
  const s = out.toString();
  return s ? `?${s}` : "";
}

async function auth(req: NextRequest) {
  const user = await sessionUser(req);
  if (user === "unavailable") return error(503, "unavailable", "Sign-in service is unavailable. Please try again shortly.");
  if (!user) return error(401, "unauthorized", "Please sign in.");
  return user;
}

export async function GET(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;

  // public certificate verification (linked from the certificate itself)
  if (path[0] === "certificates" && path.length >= 2 && path.length <= 3) {
    const code = path[1]!;
    if (!CODE.test(code)) return error(404, "not_found", "Certificate not found.");
    if (path.length === 2) return reply(await academy(`/v1/public/certificates/${code}`));
    if (path[2] !== "image") return error(404, "not_found", "Not found.");
    const img = await certificateImage(code, req.nextUrl.searchParams.get("download") === "1");
    if (img.status !== 200) return error(img.status, img.status === 404 ? "not_found" : "unavailable", img.status === 404 ? "Certificate not found." : "The Academy is unavailable.");
    return img;
  }

  const user = await auth(req);
  if (user instanceof NextResponse) return user;

  if (path.length === 1 && path[0] === "catalog") return reply(await academy(`/v1/catalog${query(req, ["lang"])}`, { user }));
  if (path.length === 1 && path[0] === "glossary") return reply(await academy(`/v1/glossary${query(req, ["lang", "q", "category"])}`, { user }));
  if (path.length === 2 && path[0] === "me" && path[1] === "certificates") return reply(await academy("/v1/me/certificates", { user }));
  if (path.length === 2 && path[0] === "chapters" && SLUG.test(path[1]!)) return reply(await academy(`/v1/chapters/${path[1]}${query(req, ["lang"])}`, { user }));
  if (path.length === 2 && path[0] === "exams" && SLUG.test(path[1]!)) return reply(await academy(`/v1/exams/${path[1]}${query(req, ["lang"])}`, { user }));
  return error(404, "not_found", "Not found.");
}

export async function POST(req: NextRequest, { params }: Ctx) {
  const path = (await params).path;
  if (!sameOrigin(req)) return error(403, "forbidden", "Cross-site request blocked.");
  if (!req.headers.get("content-type")?.includes("application/json")) return error(415, "bad_request", "Expected JSON.");
  const user = await auth(req);
  if (user instanceof NextResponse) return user;
  const body = (await req.json().catch(() => null)) as Record<string, unknown> | null;
  if (!body || typeof body !== "object") return error(400, "bad_request", "Invalid JSON body.");

  const answers = (max: number, nullable: boolean) => {
    const a = body.answers;
    if (!Array.isArray(a) || a.length > max) return null;
    if (!a.every((x) => (nullable && x === null) || (Number.isInteger(x) && (x as number) >= 0 && (x as number) < 10))) return null;
    return a as (number | null)[];
  };

  if (path.length === 3 && path[0] === "chapters" && SLUG.test(path[1]!)) {
    if (path[2] === "progress") {
      const pct = Number(body.read_pct);
      if (!Number.isFinite(pct)) return error(400, "bad_request", "Invalid read_pct.");
      return reply(await academy(`/v1/chapters/${path[1]}/progress${query(req, ["lang"])}`, { user, body: { read_pct: Math.max(0, Math.min(100, Math.round(pct))) } }));
    }
    if (path[2] === "quiz") {
      const a = answers(10, true);
      if (!a) return error(400, "bad_request", "Invalid answers.");
      return reply(await academy(`/v1/chapters/${path[1]}/quiz${query(req, ["lang"])}`, { user, body: { answers: a } }));
    }
  }
  if (path.length === 2 && path[0] === "exams" && SLUG.test(path[1]!)) {
    const a = answers(60, false);
    if (!a) return error(400, "bad_request", "Invalid answers.");
    return reply(await academy(`/v1/exams/${path[1]}${query(req, ["lang"])}`, { user, body: { answers: a } }));
  }
  return error(404, "not_found", "Not found.");
}

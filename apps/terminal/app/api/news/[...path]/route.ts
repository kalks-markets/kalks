import { NextResponse, type NextRequest } from "next/server";
import { Memo } from "@/lib/memo";
import { moduleGate } from "@/lib/tenant-brand";

// Kalks Trader news + economic calendar BFF (read-only). Browser -> /api/news/<route> (same origin) ->
// services/news /v1/… with the internal token (never sent to the browser). Headlines and the calendar are
// public reads: guests see them too.
//
//   GET feed?symbol&currency&country&category&q&before&limit&minImportance · feed/{id}
//   GET calendar?from&to&currency&impact · calendar/next?impact · calendar/{id}

type Ctx = { params: Promise<{ path: string[] }> };

const NEWS_URL = (process.env.NEWS_URL ?? "http://127.0.0.1:8103").replace(/\/+$/, "");
const NEWS_TOKEN = process.env.NEWS_INTERNAL_TOKEN ?? "";
const ID = /^\d{1,15}$/;

function query(req: NextRequest, keys: string[]) {
  const out = new URLSearchParams();
  for (const k of keys) {
    const v = req.nextUrl.searchParams.get(k);
    if (v) out.set(k, v.slice(0, 80));
  }
  const s = out.toString();
  return s ? `?${s}` : "";
}

// Public reads: every trader shares one upstream call per path for a short while (failures are never cached).
const reads = new Memo<{ status: number; data: unknown }>(30_000, 2_000);

async function forward(path: string) {
  const r = await reads.get(
    path,
    async () => {
      try {
        const res = await fetch(`${NEWS_URL}${path}`, { headers: { "x-kalks-internal": NEWS_TOKEN, "x-kalks-tenant": "kalks" }, cache: "no-store", signal: AbortSignal.timeout(10_000) });
        return { status: res.status, data: await res.json().catch(() => ({})) };
      } catch {
        return { status: 503, data: { error: { code: "unavailable", message: "News is unavailable right now." } } };
      }
    },
    (v) => v.status === 200,
  );
  return NextResponse.json(r.data, { status: r.status, headers: { "cache-control": r.status >= 200 && r.status < 300 ? "private, max-age=30" : "no-store" } });
}

export async function GET(req: NextRequest, { params }: Ctx) {
  const [a, b, ...rest] = (await params).path;
  // module switches: the calendar and the news are switched separately
  const off = await moduleGate(req.headers, a === "calendar" ? "calendar" : "news");
  if (off) return off;
  if (rest.length) return NextResponse.json({ error: { code: "not_found", message: "Not found." } }, { status: 404 });
  if (a === "feed" && !b) return forward(`/v1/news${query(req, ["symbol", "currency", "country", "category", "q", "before", "limit", "minImportance"])}`);
  if (a === "feed" && b && ID.test(b)) return forward(`/v1/news/${b}`);
  if (a === "calendar" && !b) return forward(`/v1/calendar${query(req, ["from", "to", "currency", "impact"])}`);
  if (a === "calendar" && b === "next") return forward(`/v1/calendar/next${query(req, ["impact"])}`);
  if (a === "calendar" && b && ID.test(b)) return forward(`/v1/calendar/${b}`);
  return NextResponse.json({ error: { code: "not_found", message: "Not found." } }, { status: 404 });
}

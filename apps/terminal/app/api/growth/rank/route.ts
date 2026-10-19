/**
 * GET /api/growth/rank — the contest rank widget of the acting account (x-kalks-login): the client's running
 * contests entered with that account (rank, ranked entrants, score, trades) and their level, through the growth
 * service (services/growth, GET /v1/growth/me/rank?login=). Phase 4 challenges.
 *
 * Who: the owner of the engine session in this browser's cookie, also when a staff member opened it as the client.
 * Investor (read-only) logins get nothing: the rank and level are the owner's, not the account's.
 * The rewards module switched off for the broker = the service answers 403 module_disabled and the widget hides.
 */
import type { NextRequest } from "next/server";
import { engine, error, readSessions, reply, sessionFor } from "@/lib/engine/server";
import { rankView } from "@/lib/rank";
import { tenantBrand } from "@/lib/tenant-brand";
import { hostOf } from "@/lib/tenant-host";

export const dynamic = "force-dynamic";

const GROWTH_URL = (process.env.GROWTH_URL ?? "http://127.0.0.1:8101").replace(/\/+$/, "");
const GROWTH_TOKEN = process.env.GROWTH_INTERNAL_TOKEN ?? "";

async function tenantOf(req: NextRequest): Promise<string> {
  const b = await tenantBrand(hostOf(req.headers)).catch(() => null);
  const slug = b?.slug?.toLowerCase();
  return slug && /^[a-z0-9_-]{1,64}$/.test(slug) ? slug : "kalks";
}

export async function GET(req: NextRequest) {
  const s = sessionFor(req, readSessions(req));
  if (!s) return error(401, "unauthorized", "Log in to your trading account.");
  if (s.r) return error(403, "investor", "Investor (read-only) logins don't show the owner's contest rank.");
  const st = await engine<{ account?: { userId?: number }; readOnly?: boolean }>("/v1/terminal/state?historyLimit=0", { bearer: s.t, req });
  if (st.status === 401 || st.status === 403 || st.status === 404) return error(401, "session_expired", "Your trading session has expired. Log in again.");
  if (st.status !== 200) return error(503, "unavailable", "The trade server is unavailable. Try again shortly.");
  if (st.data.readOnly) return error(403, "investor", "Investor (read-only) logins don't show the owner's contest rank.");
  const userId = Number(st.data.account?.userId);
  if (!Number.isSafeInteger(userId) || userId <= 0) return error(404, "no_owner", "This account has no client profile.");
  let res: Response;
  try {
    res = await fetch(`${GROWTH_URL}/v1/growth/me/rank?login=${encodeURIComponent(s.l)}`, {
      headers: { "x-kalks-internal": GROWTH_TOKEN, "x-kalks-tenant": await tenantOf(req), "x-kalks-user-id": String(userId) },
      cache: "no-store",
      signal: AbortSignal.timeout(8_000),
    });
  } catch {
    return error(503, "unavailable", "Rewards are unavailable right now.");
  }
  const data = (await res.json().catch(() => ({}))) as { error?: { code?: string; message?: string } };
  if (!res.ok) return error(res.status, data.error?.code ?? "unavailable", data.error?.message ?? "Rewards are unavailable right now.");
  return reply(200, rankView(data));
}

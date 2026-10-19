import { NextResponse, type NextRequest } from "next/server";
import { SESSION_COOKIE } from "@/lib/gateway";
import { forEveryone, userFeatures } from "@/lib/features";
import { tenantConfig } from "@/lib/tenant-config";
import { hostOf } from "@/lib/tenant-host";

// GET /api/me/features (and /api/mobile/me/features for the app): the signed-in client's modules, flags and sections
// after the broker's targeting rules (lib/features.ts, docs/SECTIONS.md). `false` = hidden; a key that isn't listed
// counts as on. Without a session: the broker's switches for everyone.

export const dynamic = "force-dynamic";

export async function GET(req: NextRequest) {
  const host = hostOf(req.headers) ?? "";
  const token = req.cookies.get(SESSION_COOKIE)?.value;
  const mine = token ? await userFeatures(token, req.headers, host) : null;
  const f = mine ?? forEveryone(await tenantConfig(host));
  return NextResponse.json({ modules: f?.modules ?? {}, flags: f?.flags ?? {}, sections: f?.sections ?? {}, personal: !!mine }, { headers: { "cache-control": "no-store" } });
}

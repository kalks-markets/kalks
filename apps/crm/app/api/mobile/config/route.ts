import { NextResponse, type NextRequest } from "next/server";
import { tenantConfig } from "@/lib/tenant-config";
import { hostOf } from "@/lib/tenant-host";
import { brokerServiceUrls } from "@/lib/mobile";

// GET /api/mobile/config (public, no session): what the app needs before anything else (docs/MOBILE-API.md).
// The app only knows one base URL (this Client Area); quotes, candles and the trade / options / support streams live
// on other hosts in production. Also the broker's branding (the same source as the web's brandCss: gateway
// /v1/public/tenant-config of this host), its module switches and maintenance state. No secrets; the app may cache it
// for a few minutes and refreshes it on start.

export const dynamic = "force-dynamic";

const API_VERSION = 1;
const hex = (v: string | null | undefined) => (v && /^#[0-9a-f]{6}$/i.test(v) ? v.toLowerCase() : null);

export async function GET(req: NextRequest) {
  const cfg = await tenantConfig(hostOf(req.headers) ?? "");
  const brand = cfg?.branding ?? null;
  const urls = await brokerServiceUrls(req.headers, req.nextUrl, cfg);
  return NextResponse.json(
    {
      apiVersion: API_VERSION,
      minAppVersion: process.env.MOBILE_MIN_APP_VERSION || null,
      // where the app sends the user to update (the Android APK page, /download); MOBILE_UPDATE_URL pins another
      updateUrl: process.env.MOBILE_UPDATE_URL || `${urls.app}/download`,
      urls,
      tenant: {
        slug: brand?.slug ?? cfg?.tenant.slug ?? "kalks",
        name: brand?.name ?? cfg?.tenant.name ?? "Kalks",
        default: brand ? !!brand.default : true,
        logoUrl: brand?.logo_url ?? null,
        // hex colours only (#rrggbb), like the web's brandCss
        primary: hex(brand?.primary),
        accent: hex(brand?.accent),
        supportEmail: brand?.support_email ?? null,
        website: brand?.urls?.website ?? null,
      },
      // module switches (D112): false = hidden in the app, its API answers 403 module_disabled
      modules: cfg?.modules ?? {},
      flags: cfg?.flags ?? {},
      // Client Area pages and dashboard blocks for everyone; signed in, the client's own: /api/mobile/me/features
      sections: cfg?.sections ?? {},
      maintenance: cfg?.maintenance ? { active: cfg.maintenance.active, message: cfg.maintenance.message, until: cfg.maintenance.until } : { active: false, message: "", until: null },
    },
    { headers: { "cache-control": "no-store" } },
  );
}

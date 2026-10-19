// Server-only client for services/content (127.0.0.1:8105): site content slots and the media library.
// The browser never sees the service or its token; it talks to app/api/content/[...path]/route.ts.
//
// Every call carries the staff member's tenant, email and content permissions (x-kalks-staff-perms): the BFF
// checks them first and the service checks them again (a wrongly wired route can't widen access).

import type { GatewayStaff } from "@/lib/gateway";
import { contentAllows, type ContentPerm } from "@/lib/academy";

const CONTENT_URL = (process.env.CONTENT_URL ?? "http://127.0.0.1:8105").replace(/\/+$/, "");
const CONTENT_TOKEN = process.env.CONTENT_INTERNAL_TOKEN ?? "";

export const CONTENT_SURFACES = ["website", "app", "trader", "email"] as const;
export type ContentSurface = (typeof CONTENT_SURFACES)[number];

/** Where each surface is served, for previews (`<base>/api/content-preview?token&path`) and built-in default images. */
export const PREVIEW_BASES: Record<Exclude<ContentSurface, "email">, string> = {
  website: (process.env.CONTENT_PREVIEW_WEBSITE_URL ?? "http://localhost:3010").replace(/\/+$/, ""),
  app: (process.env.CONTENT_PREVIEW_APP_URL ?? "http://localhost:3000").replace(/\/+$/, ""),
  trader: (process.env.CONTENT_PREVIEW_TRADER_URL ?? "http://localhost:3002").replace(/\/+$/, ""),
};

/** The content permissions a staff member holds, as the service expects them (comma list). */
export function contentPerms(staff: GatewayStaff): ContentPerm[] {
  return (["content.read", "content.write", "content.publish"] as const).filter((p) => contentAllows(staff, p));
}

function headers(staff: GatewayStaff): Record<string, string> {
  return {
    "x-kalks-internal": CONTENT_TOKEN,
    "x-kalks-tenant": staff.tenant?.slug || "kalks",
    "x-kalks-staff": staff.email,
    "x-kalks-staff-perms": contentPerms(staff).join(","),
  };
}

const UNAVAILABLE = { status: 503, data: { error: { code: "unavailable", message: "The content service is unavailable. Please try again shortly." } } };

export async function contentAdmin(path: string, init: { method?: "GET" | "POST" | "PUT" | "DELETE"; body?: unknown; staff: GatewayStaff; timeoutMs?: number }): Promise<{ status: number; data: unknown }> {
  const h = headers(init.staff);
  if (init.body !== undefined) h["content-type"] = "application/json";
  try {
    const res = await fetch(`${CONTENT_URL}${path}`, {
      method: init.method ?? (init.body !== undefined ? "POST" : "GET"),
      headers: h,
      body: init.body !== undefined ? JSON.stringify(init.body) : undefined,
      cache: "no-store",
      signal: AbortSignal.timeout(init.timeoutMs ?? 15_000),
    });
    return { status: res.status, data: await res.json().catch(() => ({})) };
  } catch {
    return UNAVAILABLE;
  }
}

/** Raw image upload (new library entry or a light / dark file): the bytes, file name and type are forwarded. */
export async function contentUpload(path: string, init: { bytes: ArrayBuffer; name: string; type: string; staff: GatewayStaff }): Promise<{ status: number; data: unknown }> {
  const h = headers(init.staff);
  h["content-type"] = init.type || "application/octet-stream";
  h["x-file-name"] = encodeURIComponent(init.name);
  try {
    const res = await fetch(`${CONTENT_URL}${path}`, { method: "POST", headers: h, body: init.bytes, cache: "no-store", signal: AbortSignal.timeout(60_000) });
    return { status: res.status, data: await res.json().catch(() => ({})) };
  } catch {
    return UNAVAILABLE;
  }
}

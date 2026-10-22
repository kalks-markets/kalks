// Server-only client for the prop firm service (services/prop, 127.0.0.1:8097).
// PROP_INTERNAL_TOKEN never leaves the server. The staff identity headers are built here from the staff session
// the gateway verified (lib/bff.ts requireStaff) — never from anything the browser sends.

import type { GatewayStaff } from "@/lib/gateway";
import { propPerms } from "@/lib/prop-perms";

const PROP_URL = (process.env.PROP_URL ?? "http://127.0.0.1:8097").replace(/\/$/, "");
const INTERNAL_TOKEN = process.env.PROP_INTERNAL_TOKEN ?? "";

export const propConfigured = () => INTERNAL_TOKEN.length > 0;

export type PropMethod = "GET" | "POST" | "PUT" | "DELETE";

type Init = { method?: PropMethod; body?: unknown; staff?: GatewayStaff; userId?: string | number; ip?: string | null; userAgent?: string | null; timeoutMs?: number };

function headersFor(init: Init): Record<string, string> {
  const headers: Record<string, string> = { "x-kalks-internal": INTERNAL_TOKEN, "x-kalks-tenant": init.staff?.tenant.slug || "kalks" };
  if (init.body !== undefined) headers["content-type"] = "application/json";
  if (init.staff) {
    headers["x-kalks-staff-id"] = String(init.staff.id);
    headers["x-kalks-staff-name"] = encodeURIComponent(init.staff.name || init.staff.email);
    headers["x-kalks-staff-role"] = init.staff.role;
    // exact prop.* keys, checked by the service ("-" when none: an empty header would fall back to the role check)
    headers["x-kalks-staff-perms"] = propPerms(init.staff).join(",") || "-";
  }
  if (init.userId !== undefined) headers["x-kalks-user-id"] = String(init.userId);
  if (init.ip) headers["x-forwarded-for"] = init.ip;
  if (init.userAgent) headers["user-agent"] = init.userAgent;
  return headers;
}

/** JSON call to the prop service. Network failures come back as 503 `unavailable`. */
export async function prop<T = unknown>(path: string, init: Init = {}): Promise<{ status: number; data: T }> {
  try {
    const res = await fetch(`${PROP_URL}${path}`, {
      method: init.method ?? (init.body !== undefined ? "POST" : "GET"),
      headers: headersFor(init),
      body: init.body !== undefined ? JSON.stringify(init.body) : undefined,
      cache: "no-store",
      signal: AbortSignal.timeout(init.timeoutMs ?? 15_000),
    });
    const text = await res.text();
    let data: unknown = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch {
      data = { error: { code: "bad_gateway", message: "The prop service returned an unexpected response." } };
    }
    return { status: res.status, data: data as T };
  } catch {
    return { status: 503, data: { error: { code: "unavailable", message: "The prop service is unavailable. Please try again shortly." } } as T };
  }
}

/** Raw (non-JSON) GET, e.g. the certificate SVG. */
export async function propRaw(path: string, init: Init = {}): Promise<{ status: number; body: string; contentType: string } | null> {
  try {
    const res = await fetch(`${PROP_URL}${path}`, { headers: headersFor(init), cache: "no-store", signal: AbortSignal.timeout(init.timeoutMs ?? 15_000) });
    return { status: res.status, body: await res.text(), contentType: res.headers.get("content-type") ?? "application/octet-stream" };
  } catch {
    return null;
  }
}

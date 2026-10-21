// Server-only helpers for the IB / referral service (services/ib, 127.0.0.1:8096).
// The browser never sees the service or its internal token: /api/partner/* resolves the signed-in client from
// the gateway session cookie and forwards the gateway user id as X-Kalks-User-Id. Link clicks are recorded
// from the proxy (proxy.ts) before the visitor reaches /register.

import type { NextRequest, NextResponse } from "next/server";
import type { GatewayUser } from "@/lib/gateway";

const IB_URL = (process.env.IB_URL ?? "http://127.0.0.1:8096").replace(/\/+$/, "");
const IB_TOKEN = process.env.IB_INTERNAL_TOKEN ?? "";

/** First-party cookie carrying the referral a visitor arrived with: `CODE` or `CODE:campaign`. */
export const REF_COOKIE = "kalks_ref";
const REF_MAX_AGE = 90 * 24 * 3600;

export type IbResult<T = Record<string, unknown>> = { status: number; data: T };

export async function ib<T = Record<string, unknown>>(
  path: string,
  init: { method?: "GET" | "POST" | "PUT" | "PATCH"; body?: unknown; userId?: number; tenant?: string; timeoutMs?: number } = {},
): Promise<IbResult<T>> {
  const headers: Record<string, string> = { "x-kalks-internal": IB_TOKEN, "x-kalks-tenant": init.tenant || "kalks" };
  if (init.userId !== undefined) headers["x-kalks-user-id"] = String(init.userId);
  if (init.body !== undefined) headers["content-type"] = "application/json";
  try {
    const res = await fetch(`${IB_URL}${path}`, {
      method: init.method ?? (init.body !== undefined ? "POST" : "GET"),
      headers,
      body: init.body !== undefined ? JSON.stringify(init.body) : undefined,
      cache: "no-store",
      signal: AbortSignal.timeout(init.timeoutMs ?? 15_000),
    });
    const data = (await res.json().catch(() => ({}))) as T;
    return { status: res.status, data };
  } catch {
    return { status: 503, data: { error: { code: "unavailable", message: "The partner service is unavailable. Please try again shortly." } } as T };
  }
}

export const ibFor = (user: GatewayUser) => ({ userId: user.id, tenant: user.tenant?.slug || "kalks" });

const CODE_RE = /^[A-Za-z0-9]{3,24}$/;
const SLUG_RE = /^[A-Za-z0-9_-]{1,40}$/;

export function cleanRef(code: string | null | undefined, campaign: string | null | undefined): { code: string; campaign: string | null } | null {
  const c = (code ?? "").trim();
  if (!CODE_RE.test(c)) return null;
  const k = (campaign ?? "").trim();
  return { code: c.toUpperCase(), campaign: SLUG_RE.test(k) ? k.toLowerCase() : null };
}

/** `CODE[:campaign]` from the referral cookie. */
export function readRefCookie(v: string | undefined): { code: string; campaign: string | null } | null {
  if (!v) return null;
  const [code, campaign] = decodeURIComponent(v).split(":");
  return cleanRef(code, campaign);
}

/**
 * Records a link click with the IB service and remembers the referral in a first-party cookie, so the
 * sign-up form (email or Google) attributes the campaign even if the visitor browses around first.
 * Best effort: a slow or unavailable IB service never blocks the page (1.5 s budget).
 */
export async function trackClick(req: NextRequest, res: NextResponse, ref: { code: string; campaign: string | null }, ip: string) {
  const r = await ib<{ valid?: boolean; code?: string; campaign?: string | null }>("/v1/ib/clicks", {
    body: { code: ref.code, campaign: ref.campaign, ip, userAgent: req.headers.get("user-agent") ?? "", referer: req.headers.get("referer"), landing: req.nextUrl.pathname },
    timeoutMs: 1500,
  });
  // unknown codes are still kept: the sign-up form shows the code pre-filled and the gateway refuses it on
  // the referral field, so the visitor can fix or clear it; a paused / unknown campaign is dropped
  const campaign = r.status === 200 && r.data.valid ? (r.data.campaign ?? null) : ref.campaign;
  const value = campaign ? `${ref.code}:${campaign}` : ref.code;
  res.cookies.set(REF_COOKIE, value, { httpOnly: false, secure: process.env.NODE_ENV === "production", sameSite: "lax", path: "/", maxAge: REF_MAX_AGE });
}

/**
 * Sign-up (email or Google): adds the partner campaign from the referral cookie when the referral code on the
 * form is the one the visitor arrived with. A campaign typed by the browser is only kept if it is well-formed.
 */
export function withCampaign(body: Record<string, unknown>, req: NextRequest) {
  const fromCookie = readRefCookie(req.cookies.get(REF_COOKIE)?.value);
  const code = typeof body.referral_code === "string" ? body.referral_code.trim().toUpperCase() : "";
  const sent = typeof body.referral_campaign === "string" && SLUG_RE.test(body.referral_campaign) ? body.referral_campaign.toLowerCase() : null;
  delete body.referral_campaign;
  if (!code) return;
  if (fromCookie && fromCookie.code === code && fromCookie.campaign) body.referral_campaign = fromCookie.campaign;
  else if (sent) body.referral_campaign = sent;
}

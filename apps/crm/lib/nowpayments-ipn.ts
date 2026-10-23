// NOWPayments IPN (Instant Payment Notification) forwarder, served at https://<client area>/api/payments/nowpayments/ipn
// (app/api/payments/nowpayments/ipn/route.ts).
//
// Public by design: NOWPayments calls it, not a signed-in client (proxy.ts lists it in ALWAYS_OPEN, so maintenance
// mode, sessions and module switches never block it). It only forwards: the raw body, byte for byte, and the
// `x-nowpayments-sig` header go to the wallet service (`POST /v1/nowpayments/ipn`, internal token), which checks the
// HMAC-SHA512 signature with the broker's IPN secret, re-reads the payment from the NOWPayments API and credits it once
// (services/wallet/README.md#nowpayments). Nothing here parses, trusts or logs the body.
//
// Limits: JSON only, 64 KB (Caddy caps it too), and a per-IP budget so the endpoint can't be used to hammer the wallet.

import { NextResponse, type NextRequest } from "next/server";
import { clientIp } from "@/lib/gateway";

const WALLET_URL = (process.env.WALLET_URL ?? "http://127.0.0.1:8095").replace(/\/+$/, "");
const WALLET_TOKEN = process.env.WALLET_INTERNAL_TOKEN ?? "";
export const IPN_MAX_BYTES = 64 * 1024;
/** Requests per IP per minute. NOWPayments sends one per status change (plus its retries), far below this. */
export const IPN_PER_MINUTE = 120;
const NO_STORE = { "cache-control": "no-store" };

const hits = new Map<string, { at: number; n: number }>();

/** Fixed one-minute window per IP (in memory, per Client Area process). */
export function ipnAllowed(ip: string, now = Date.now()): boolean {
  const h = hits.get(ip);
  if (!h || now - h.at >= 60_000) {
    if (hits.size > 10_000) hits.clear();
    hits.set(ip, { at: now, n: 1 });
    return true;
  }
  h.n += 1;
  return h.n <= IPN_PER_MINUTE;
}

export const ipnError = (status: number, code: string, message: string) => NextResponse.json({ error: { code, message } }, { status, headers: NO_STORE });

export async function forwardIpn(req: NextRequest): Promise<NextResponse> {
  if (!ipnAllowed(clientIp(req.headers))) return ipnError(429, "rate_limited", "Too many requests.");
  if (!(req.headers.get("content-type") ?? "").toLowerCase().includes("json")) return ipnError(415, "bad_request", "Expected JSON.");
  if (Number(req.headers.get("content-length") ?? "0") > IPN_MAX_BYTES) return ipnError(413, "too_large", "Body too large.");
  const raw = new Uint8Array(await req.arrayBuffer());
  if (raw.byteLength === 0) return ipnError(400, "bad_request", "Empty body.");
  if (raw.byteLength > IPN_MAX_BYTES) return ipnError(413, "too_large", "Body too large.");
  const headers: Record<string, string> = {
    "content-type": "application/json",
    "x-kalks-internal": WALLET_TOKEN,
    "x-kalks-service": "crm",
    "x-forwarded-for": clientIp(req.headers),
  };
  const sig = req.headers.get("x-nowpayments-sig");
  if (sig) headers["x-nowpayments-sig"] = sig.slice(0, 256);
  try {
    const res = await fetch(`${WALLET_URL}/v1/nowpayments/ipn`, { method: "POST", headers, body: raw, cache: "no-store", signal: AbortSignal.timeout(25_000) });
    const data = await res.json().catch(() => ({}));
    return NextResponse.json(data, { status: res.status, headers: NO_STORE });
  } catch {
    // NOWPayments retries the notification; the wallet's poller also re-checks open payments every minute
    return ipnError(503, "unavailable", "Temporarily unavailable.");
  }
}

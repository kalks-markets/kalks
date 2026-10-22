/**
 * GET /sso?token=<one-time token>: the README's Trade-button link form. Redeemed server-side, the session
 * cookie is set and the browser lands on `/` with the new account active (the token never stays in the URL).
 */
import { NextResponse, type NextRequest } from "next/server";
import { publicUrl } from "@kalks/ui/public-url";
import { redeemSso } from "@/lib/engine/sso";

export const dynamic = "force-dynamic";

export async function GET(req: NextRequest) {
  const token = req.nextUrl.searchParams.get("token") ?? "";
  const r = await redeemSso(req, token);
  const data = (await r.clone().json().catch(() => ({}))) as { login?: string; error?: { code?: string } };
  const to = publicUrl(req, "/");
  if (r.status === 200 && data.login) to.searchParams.set("account", data.login);
  else to.pathname = "/login", to.searchParams.set("error", data.error?.code ?? "sso_failed");
  const res = NextResponse.redirect(to, 303);
  for (const c of r.cookies.getAll()) res.cookies.set(c);
  res.headers.set("cache-control", "no-store");
  res.headers.set("referrer-policy", "no-referrer");
  return res;
}

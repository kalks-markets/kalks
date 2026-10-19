import { NextResponse, type NextRequest } from "next/server";
import { CONTENT_PREVIEW_COOKIE, isPreviewToken, safePreviewPath } from "@kalks/ui/content/server";

// Back Office preview links (Content → Pages → Preview): /api/content-preview?token=<signed token>&path=/login stores
// the short-lived token in a cookie and opens the page, which then shows drafts (lib/content.ts). The content
// service checks the token (tenant, surface, expiry); this route only carries it. ?exit=1 leaves the preview.

export function GET(req: NextRequest) {
  const q = req.nextUrl.searchParams;
  const res = NextResponse.redirect(new URL(safePreviewPath(q.get("path")), req.url));
  res.headers.set("cache-control", "no-store");
  if (q.get("exit")) {
    res.cookies.delete(CONTENT_PREVIEW_COOKIE);
    return res;
  }
  const token = q.get("token");
  if (!isPreviewToken(token)) return NextResponse.json({ error: { code: "bad_request", message: "Invalid preview link." } }, { status: 400 });
  res.cookies.set(CONTENT_PREVIEW_COOKIE, token, { httpOnly: true, sameSite: "lax", secure: process.env.NODE_ENV === "production", path: "/", maxAge: 30 * 60 });
  return res;
}

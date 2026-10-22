import type { NextRequest } from "next/server";
import { certificateImage } from "@/lib/academy";

// Public Academy certificate image for the verification page. It sits next to the page rather than under
// /api/academy, which proxy.ts refuses while the broker has the Academy module off: certificate checks stay public
// then (services/academy/src/modules.rs). No sign-in: /certificate/** is let through by proxy.ts.

type Ctx = { params: Promise<{ code: string }> };

export async function GET(req: NextRequest, { params }: Ctx) {
  const img = await certificateImage((await params).code, req.nextUrl.searchParams.get("download") === "1");
  if (img.status === 200) return img;
  return new Response(img.status === 404 ? "Certificate not found" : "The Academy is unavailable", { status: img.status, headers: { "cache-control": "no-store" } });
}

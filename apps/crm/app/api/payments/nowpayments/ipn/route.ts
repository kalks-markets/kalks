import type { NextRequest } from "next/server";
import { forwardIpn, ipnError } from "@/lib/nowpayments-ipn";

// NOWPayments IPN: public (no session), forwarded raw to the wallet service, which verifies the signature.
// See lib/nowpayments-ipn.ts.

export const dynamic = "force-dynamic";

export function POST(req: NextRequest) {
  return forwardIpn(req);
}

export function GET() {
  return ipnError(405, "method_not_allowed", "POST only.");
}

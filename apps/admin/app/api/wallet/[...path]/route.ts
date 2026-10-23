import { NextResponse, type NextRequest } from "next/server";
import { clientIp, gateway } from "@/lib/gateway";
import { apiError, mutationAllowed, requireStaff } from "@/lib/bff";
import { walletConfigured, walletService } from "@/lib/wallet";
import { walletAllows, type WalletPerm } from "@/lib/wallet-perms";
import { tradingAllows } from "@/lib/trading-perms";

// Finance BFF: browser -> /api/wallet/<path> (same origin, staff cookie) -> wallet service /v1/admin/<path>.
// The staff session is verified with the gateway on every call and the permission for the route is checked
// here (lib/wallet-perms.ts); the wallet service checks the forwarded finance.* key again and writes its audit log.
//   GET names?ids=1,2  -> client names / emails from the gateway (needs clients.read in the gateway)
// Balance & credit (adjustments/*): a route lists the permissions of which any one is enough here; the wallet
// service then enforces the exact key (finance.adjust for add / deduct, finance.credit for credit,
// finance.adjust_force to force, finance.adjust_approve to approve) from the forwarded x-kalks-staff-perms.

type Method = "GET" | "POST" | "PUT";
type Perm = WalletPerm | "finance.adjust";
type Route = { method: Method; re: RegExp; perm: Perm | Perm[] };

const ADJ_READ: Perm[] = ["finance.read", "finance.adjust", "finance.credit", "finance.adjust_approve"];
const ADJ_WRITE: Perm[] = ["finance.adjust", "finance.credit"];
const allows = (staff: Parameters<typeof walletAllows>[0], p: Perm) => (p === "finance.adjust" ? tradingAllows(staff, p) : walletAllows(staff, p));

const N = "\\d{1,18}";
const ROUTES: Route[] = [
  { method: "GET", re: /^(summary|deposits|withdrawals|wallets|settings|reconciliation|audit)$/, perm: "finance.read" },
  { method: "GET", re: new RegExp(`^(deposits|withdrawals|wallets)/${N}$`), perm: "finance.read" },
  { method: "POST", re: new RegExp(`^deposits/${N}/(assign|reject|recheck)$`), perm: "finance.write" },
  { method: "POST", re: new RegExp(`^withdrawals/${N}/(approve|reject)$`), perm: "finance.approve" },
  { method: "POST", re: new RegExp(`^withdrawals/${N}/paid$`), perm: "finance.write" },
  { method: "GET", re: /^adjustments$/, perm: ADJ_READ },
  { method: "GET", re: new RegExp(`^adjustments/(${N}|settings|targets/${N})$`), perm: ADJ_READ },
  { method: "POST", re: /^adjustments(\/preview)?$/, perm: ADJ_WRITE },
  { method: "POST", re: new RegExp(`^adjustments/${N}/(approve|reject)$`), perm: "finance.adjust_approve" },
  { method: "POST", re: new RegExp(`^adjustments/${N}/cancel$`), perm: ADJ_WRITE },
  { method: "PUT", re: /^adjustments\/settings$/, perm: "finance.settings" },
  { method: "PUT", re: /^settings$/, perm: "finance.settings" },
  // NOWPayments crypto deposits: settings (keys are write-only, the service never returns them), payments, review.
  // Approve: finance.write proposes / credits, finance.adjust_approve gives the second approval above the four-eyes
  // threshold; the service decides which applies to the payment's state.
  { method: "GET", re: /^nowpayments\/(settings|payments)$/, perm: "finance.read" },
  { method: "GET", re: new RegExp(`^nowpayments/payments/${N}$`), perm: "finance.read" },
  { method: "PUT", re: /^nowpayments\/settings$/, perm: "finance.settings" },
  { method: "POST", re: /^nowpayments\/test$/, perm: "finance.settings" },
  { method: "POST", re: new RegExp(`^nowpayments/payments/${N}/(approve|reject)$`), perm: ["finance.write", "finance.adjust_approve"] },
  { method: "POST", re: new RegExp(`^nowpayments/payments/${N}/refresh$`), perm: "finance.write" },
];

const json = (data: unknown, status = 200) => NextResponse.json(data, { status, headers: { "cache-control": "no-store" } });

async function handle(req: NextRequest, parts: string[], method: Method) {
  const path = parts.map((p) => decodeURIComponent(p)).join("/");
  if (!walletConfigured()) return apiError(503, "not_configured", "The wallet service is not configured for the Back Office.");
  if (method === "GET" && path === "names") return names(req);
  const route = ROUTES.find((r) => r.method === method && r.re.test(path));
  if (!route) return apiError(404, "not_found", "Not found.");
  let body: unknown;
  if (method !== "GET") {
    const blocked = mutationAllowed(req);
    if (blocked) return blocked;
    body = await req.json().catch(() => null);
    if (body === null || typeof body !== "object" || Array.isArray(body)) return apiError(400, "bad_request", "Invalid request body.");
  }
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  const perms = Array.isArray(route.perm) ? route.perm : [route.perm];
  if (!perms.some((p) => allows(who.staff, p))) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const r = await walletService(`/v1/admin/${path}${method === "GET" ? req.nextUrl.search : ""}`, { method, body, staff: who.staff, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
  return json(r.data, r.status);
}

const NAME_TTL = 5 * 60_000;
const cache = new Map<string, { name: string; email: string; kyc: string; at: number }>();

/** `?ids=1,2,3` → `{ names: { "1": { name, email, kyc_status } } }` (gateway, needs clients.read). */
async function names(req: NextRequest) {
  const who = await requireStaff(req);
  if (who instanceof NextResponse) return who;
  if (!walletAllows(who.staff, "finance.read")) return apiError(403, "forbidden", "Your role doesn't allow this.");
  const ids = Array.from(new Set((req.nextUrl.searchParams.get("ids") ?? "").split(",").filter((x) => /^\d{1,18}$/.test(x)))).slice(0, 100);
  const out: Record<string, { name: string; email: string; kyc_status: string }> = {};
  if (!who.staff.permissions?.includes("clients.read")) return json({ names: out });
  const now = Date.now();
  await Promise.all(
    ids.map(async (id) => {
      const hit = cache.get(id);
      if (hit && now - hit.at < NAME_TTL) {
        out[id] = { name: hit.name, email: hit.email, kyc_status: hit.kyc };
        return;
      }
      const r = await gateway<{ user?: { name: string; email: string; kyc_status: string } }>(`/v1/admin/users/${id}`, { token: who.token, ip: clientIp(req.headers), userAgent: req.headers.get("user-agent") });
      if (r.status === 200 && r.data.user) {
        cache.set(id, { name: r.data.user.name, email: r.data.user.email, kyc: r.data.user.kyc_status, at: now });
        out[id] = { name: r.data.user.name, email: r.data.user.email, kyc_status: r.data.user.kyc_status };
      }
    }),
  );
  if (cache.size > 5000) cache.clear();
  return json({ names: out });
}

type Ctx = { params: Promise<{ path: string[] }> };
export async function GET(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "GET");
}
export async function POST(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "POST");
}
export async function PUT(req: NextRequest, { params }: Ctx) {
  return handle(req, (await params).path, "PUT");
}

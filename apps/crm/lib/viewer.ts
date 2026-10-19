// View-only logins (D90, D93): what a viewer session may open in the Client Area.
// Used by the proxy (edge) to hold viewer sessions to their sections, by the shell to build their navigation,
// and by the BFFs to keep viewers to the trading accounts they were given. The gateway refuses every change
// made with a viewer session on its side too (403 viewer_read_only).

export type ViewerSection = "dashboard" | "accounts" | "history" | "wallet" | "partner";

export type ViewerScope = {
  id: number;
  label: string;
  username: string;
  accounts: string[];
  sections: ViewerSection[];
  expires_at: string | null;
  revoked_at?: string | null;
  status: "active" | "expired" | "revoked";
  last_login_at: string | null;
  created_at: string;
};

/** Viewer session tokens start with this (gateway `identity::VIEWER_TOKEN_PREFIX`). */
export const VIEWER_TOKEN_PREFIX = "v.";

export const isViewerToken = (token: string | undefined | null) => !!token && token.startsWith(VIEWER_TOKEN_PREFIX);

type SectionDef = { label: string; hint: string; home: string; pages: string[]; api: string[] };

/** Pages: path prefixes ("/" = the dashboard page only). APIs: GET prefixes the section needs. */
export const VIEWER_SECTIONS: Record<ViewerSection, SectionDef> = {
  dashboard: { label: "Dashboard", hint: "Overview, markets, news and calendar", home: "/", pages: ["/", "/markets", "/news", "/calendar"], api: ["/api/trading/accounts", "/api/news", "/api/growth/banners"] },
  accounts: { label: "Accounts & positions", hint: "Balances, open positions and orders", home: "/accounts", pages: ["/accounts"], api: ["/api/trading/accounts", "/api/trading/groups"] },
  history: { label: "Trade history & statements", hint: "Closed trades, ledger and statements", home: "/portfolio/history", pages: ["/portfolio"], api: ["/api/trading/accounts", "/api/reports"] },
  wallet: { label: "Wallet balances", hint: "USDT balance and wallet history", home: "/wallet", pages: ["/wallet", "/wallet/history"], api: ["/api/wallet/overview", "/api/wallet/activity", "/api/wallet/config", "/api/wallet/ledger"] },
  partner: { label: "Partner dashboard", hint: "IB clients and commissions", home: "/partner", pages: ["/partner"], api: ["/api/partner"] },
};

export const VIEWER_SECTION_KEYS = Object.keys(VIEWER_SECTIONS) as ViewerSection[];

/** Pages under an allowed prefix that are still write-only flows, never shown to viewers. */
const VIEWER_BLOCKED_PAGES = ["/accounts/new", "/wallet/deposit", "/wallet/withdraw", "/wallet/transfer", "/partner/payouts", "/partner/links"];

/** APIs every viewer session may read / call. */
const VIEWER_ALWAYS_GET = ["/api/auth/me", "/api/status", "/api/me/features"];
const VIEWER_ALWAYS_POST = ["/api/auth/logout", "/api/security/viewer-activity"];

const under = (path: string, prefix: string) => (prefix === "/" ? path === "/" : path === prefix || path.startsWith(`${prefix}/`));

export function viewerPageAllowed(scope: Pick<ViewerScope, "sections">, pathname: string): boolean {
  if (VIEWER_BLOCKED_PAGES.some((p) => under(pathname, p))) return false;
  return scope.sections.some((s) => VIEWER_SECTIONS[s]?.pages.some((p) => under(pathname, p)));
}

/** Read-only requests of a viewer session; every other method is refused before it reaches a BFF. */
export function viewerApiAllowed(scope: Pick<ViewerScope, "sections">, method: string, pathname: string): boolean {
  if (method === "POST" && VIEWER_ALWAYS_POST.includes(pathname)) return true;
  if (method !== "GET" && method !== "HEAD") return false;
  if (VIEWER_ALWAYS_GET.some((p) => under(pathname, p))) return true;
  return scope.sections.some((s) => VIEWER_SECTIONS[s]?.api.some((p) => under(pathname, p)));
}

/** Where a viewer lands: their first section's home page. */
export function viewerHome(scope: Pick<ViewerScope, "sections">): string {
  const first = VIEWER_SECTION_KEYS.find((k) => scope.sections.includes(k));
  return first ? VIEWER_SECTIONS[first].home : "/";
}

/** A trading account the viewer was given (logins are compared as strings). */
export function viewerHasAccount(scope: Pick<ViewerScope, "accounts">, login: string | number): boolean {
  return scope.accounts.includes(String(login));
}

export const VIEWER_READ_ONLY = { code: "viewer_read_only", message: "This is a view-only login. Viewers can't make changes." } as const;
export const VIEWER_OUT_OF_SCOPE = { code: "viewer_scope", message: "This isn't shared with your view-only login." } as const;

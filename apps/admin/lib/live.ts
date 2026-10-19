import type { NavModule } from "@kalks/ui";
import { IS_DEMO, pathAllowed } from "@kalks/mock/mode";
import { ADMIN_NAV } from "@/lib/nav";

/**
 * Live builds (NEXT_PUBLIC_KALKS_MODE unset / "live") show only pages backed by real services.
 * Demo builds show the full mock showcase. See packages/mock/src/mode.ts.
 */

/** Pages (path prefixes) that run on real data in live builds. */
export const LIVE_PAGES = ["/", "/clients", "/security", "/org", "/trading", "/config", "/social", "/partners", "/content/academy", "/content/news", "/content/calendar", "/prop", "/finance", "/algo", "/marketing", "/brokers", "/settings/maintenance", "/settings/features", "/settings/layout", "/support", "/content/notifications", "/analytics", "/options", "/command/risk"] as const;

/** Sub-pages under a live prefix that are not live yet. */
export const LIVE_EXCLUDED = [
  "/clients/leads",
  "/clients/aml",
  "/clients/duplicates",
  "/clients/segments",
  "/security/users",
  "/org/desks",
  "/org/kpis",
  // trading: everything runs on the trading engine except rollovers (corporate actions are live)
  "/trading/rollovers",
  // config: account groups (/config, /config/groups), spreads and symbols (catalogue, live switch, templates) are live
  "/config/charges",
  "/config/swaps",
  "/config/margin",
  "/config/sessions",
  "/config/demo",
  // social: copy trading, PAMM, fee payouts and the social audit are live; the algo marketplace and API keys are not
  "/social/marketplace",
  "/social/api-keys",
  "/partners/sub-brokers",
  // owner panel: everything but the global symbol master
  "/brokers/symbols",
  // finance: deposits, withdrawals, wallets, adjustments (Balance & credit), reconciliation and wallet settings
  // run on the wallet service
  "/finance/transactions",
  "/finance/payouts",
  "/finance/conversion",
  // marketing: everything runs on the growth service (journeys included); UTM campaigns on the reports service
] as const;

/** Next milestone: shown in the live nav with a "Soon" chip and a Coming soon page. */
export const SOON_PAGES: Record<string, { title: string; text: string }> = {
};

export function isLivePath(pathname: string): boolean {
  return pathAllowed(pathname, LIVE_PAGES) && !pathAllowed(pathname, LIVE_EXCLUDED);
}

/** Coming-soon copy for a path (longest matching Soon prefix), if it's an upcoming module. */
export function soonFor(pathname: string): { title: string; text: string } | null {
  const key = Object.keys(SOON_PAGES)
    .filter((p) => pathname === p || (p !== "/finance" && pathname.startsWith(p + "/")))
    .sort((a, b) => b.length - a.length)[0];
  return key ? SOON_PAGES[key]! : null;
}


/**
 * Navigation for live builds: only pages backed by real services (a module goes when none of its pages is live),
 * without the demo's mock count badges. Pages that aren't live yet stay reachable by URL and say so.
 */
export const LIVE_NAV: NavModule[] = ADMIN_NAV.flatMap((m) => {
  const sub = m.sub?.filter((s) => isLivePath(s.href)).map((s) => ({ ...s, badge: undefined })); // demo badges are mock counts
  if (m.sub ? !sub?.length : !isLivePath(m.href)) return [];
  // the rail lights the module on any of its pages (its landing page may move to the first live one)
  const match = m.match ?? [...new Set([m.href, ...(sub ?? []).map((s) => s.href)])];
  return [{ ...m, badge: undefined, match, href: sub?.length ? sub[0]!.href : m.href, sub }];
});

export const NAV = IS_DEMO ? ADMIN_NAV : LIVE_NAV;

export const NAV_COMMANDS = NAV.flatMap((m) => (m.sub ?? [{ href: m.href, label: m.label }]).map((s) => ({ group: m.label, label: s.label, href: s.href, Icon: m.icon })));

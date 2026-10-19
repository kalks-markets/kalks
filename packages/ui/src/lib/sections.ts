// One map of what belongs to which switch, shared by the Client Area (proxy, nav, dashboard), the mobile API paths,
// Kalks Trader and the Back Office's Client Area layout page (docs/SECTIONS.md).
//
// * Modules (gateway tenancy.rs, kind `module`): the Platform Owner switches them per broker. PATH_MODULES says which
//   module a Client Area page or BFF path belongs to.
// * Sections (kind `section`): every Client Area page / menu entry and every dashboard block. The broker's staff
//   (`settings.write`) switch them, for everyone or by country, account group or client (gateway sections.rs).
//   A section inside a switched-off module stays off. SECTIONS is the catalogue; the gateway's BUILTIN_SECTIONS lists
//   the same keys (a gateway test reads this file and fails when they drift apart).
//
// Pure data and helpers, no imports: plain `node --test` and the apps' bundlers load it as it is.

/** Switches as the apps receive them (`key -> on`). A key that isn't listed counts as on. */
export type Switches = Record<string, boolean>;

/**
 * Which module a Client Area page or BFF path belongs to (longest prefix wins, by path segment). A path shared by
 * two modules ("algo|api") stays open while either of them is on; a path that needs both ("options+ai") closes when
 * either is off. The mobile app's native trade routes are judged on their own path (apps/crm/lib/mobile.ts), its
 * rewrites on the cookie route.
 */
export const PATH_MODULES: readonly (readonly [string, string])[] = [
  // MAM was part of copy trading's /social until it got its own switch
  ["/social/mam", "mam"],
  ["/social/managed", "mam"],
  ["/api/social/mam", "mam"],
  ["/social/pamm", "pamm"],
  ["/social/investments", "pamm"],
  ["/api/social/funds", "pamm"],
  // PAMM investments and their invest / redeem requests (cancel) belong to PAMM, not copy trading
  ["/api/social/investments", "pamm"],
  ["/api/social/requests", "pamm"],
  ["/social", "copy_trading"],
  ["/api/social", "copy_trading"],
  ["/prop", "prop"],
  ["/api/prop", "prop"],
  ["/partner", "ib"],
  ["/api/partner", "ib"],
  ["/developer/strategies", "algo"],
  ["/developer/deployments", "algo"],
  ["/developer/backtests", "algo"],
  ["/developer/marketplace", "algo"],
  ["/api/algo/strategies", "algo"],
  ["/api/algo/deployments", "algo"],
  ["/api/algo/backtests", "algo"],
  ["/api/algo/market", "algo"],
  ["/api/algo/ai", "algo"],
  ["/api/algo/validate", "algo"],
  // the kill switch stops running strategies: it follows Algo, where its page lives
  ["/api/algo/controls", "algo"],
  // the strategy catalogue and the account picker serve the strategy builder and AI Trader (Algo) as well as API keys
  // and webhooks (API)
  ["/api/algo/meta", "algo|api"],
  ["/api/algo/accounts", "algo|api"],
  ["/developer", "api"],
  ["/api/algo", "api"],
  ["/academy", "academy"],
  ["/api/academy", "academy"],
  ["/wallet", "wallet"],
  ["/api/wallet", "wallet"],
  ["/rewards", "rewards"],
  // growth BFF: rewards features follow the module; banners and share cards stay on
  ...["rewards", "points", "redeem", "redemptions", "vouchers", "cashback", "promotions", "bonuses", "promo", "contests"].map((p) => [`/api/growth/${p}`, "rewards"] as const),
  // Kalks FX Options: the Options page and its terms (suitability is options-only)
  ["/options", "options"],
  ["/api/suitability", "options"],
  // market pages; the news BFF serves news and the economic calendar
  ["/markets", "markets"],
  ["/news", "news"],
  ["/calendar", "calendar"],
  ...["feed", "map", "sources", "brief"].map((p) => [`/api/news/${p}`, "news"] as const),
  ["/api/news/calendar", "calendar"],
  ["/api/news/me/calendar", "calendar"],
  // the support chat (Ask Kalks AI asks through it too); the stream ticket also serves the bell and stays open, and so
  // does the Support page (email)
  ...["me", "conversations", "messages", "handover", "read", "typing", "attachments"].map((p) => [`/api/support/${p}`, "support_chat|ai"] as const),
  // the mobile app's native trade routes (Kalks Trader in the app)
  ["/api/mobile/trade/options", "options"],
  ["/api/mobile/trade/options/explain", "options+ai"],
  ["/api/mobile/trade/ai-trader", "ai"],
  ["/api/mobile/trade/mam", "mam"],
];

const under = (path: string, prefix: string) => path === prefix || path.startsWith(prefix + "/");

/** The longest prefix of `path` in `table` (by path segment) and its value, or null. */
function longest(table: readonly (readonly [string, string])[], path: string): string | null {
  let best: readonly [string, string] | null = null;
  for (const row of table) if (under(path, row[0]) && (!best || row[0].length > best[0].length)) best = row;
  return best ? best[1] : null;
}

/** Whether `key` is on: an unknown config (gateway unreachable) or a key the gateway doesn't send counts as on.
 *  "a+b" needs every one, "a|b" any one of them. Works for module and section keys alike. */
export function isOn(switches: Switches | null | undefined, key: string): boolean {
  if (key.includes("+")) return key.split("+").every((k) => switches?.[k] !== false);
  if (key.includes("|")) return key.split("|").some((k) => switches?.[k] !== false);
  return switches?.[key] !== false;
}

/** The module(s) a Client Area page or BFF path belongs to, or null. */
export function moduleFor(pathname: string): string | null {
  return longest(PATH_MODULES, pathname);
}

/** True when the broker switched off the module(s) of a page or BFF path. A module the gateway doesn't list is on. */
export function moduleOff(modules: Switches, pathname: string): boolean {
  const mod = moduleFor(pathname);
  return !!mod && !isOn(modules, mod);
}

/** The first switched-off module of a path (for the "not available" page), or null. */
export function offModule(modules: Switches, pathname: string): string | null {
  if (!moduleOff(modules, pathname)) return null;
  return moduleFor(pathname)!.split(/[|+]/).find((m) => modules[m] === false) ?? null;
}

/* ------------------------------------------------------------------ */
/* Sections: Client Area pages and dashboard blocks                    */
/* ------------------------------------------------------------------ */

export type SectionDef = {
  key: string;
  kind: "page" | "block";
  /** The Client Area nav entry it sits under (CRM_NAV keys; "dashboard" for the dashboard blocks). */
  group: string;
  name: string;
  /** Pages: the path (it and everything below it, unless a longer section path claims it). */
  path?: string;
  /** The module it belongs to: off while that module is off, whatever its own switch says. */
  module?: string;
};

/** Nav groups in Client Area order, with the label the Back Office shows. */
export const SECTION_GROUPS: readonly { key: string; name: string }[] = [
  { key: "dashboard", name: "Dashboard" },
  { key: "accounts", name: "Accounts" },
  { key: "wallet", name: "Wallet" },
  { key: "portfolio", name: "Portfolio" },
  { key: "options", name: "Options" },
  { key: "partner", name: "Partner (IB)" },
  { key: "social", name: "Copy & PAMM" },
  { key: "prop", name: "Prop Challenges" },
  { key: "rewards", name: "Contests & Rewards" },
  { key: "developer", name: "API & Algo" },
  { key: "academy", name: "Academy" },
  { key: "profile", name: "Profile & Security" },
  { key: "support", name: "Support" },
];

const page = (path: string, group: string, name: string, module?: string): SectionDef => ({ key: `page.${path.slice(1).replace(/\//g, ".")}`, kind: "page", group, name, path, ...(module ? { module } : {}) });
const block = (key: string, name: string, module?: string): SectionDef => ({ key: `dashboard.${key}`, kind: "block", group: "dashboard", name, ...(module ? { module } : {}) });

/**
 * Every switchable Client Area page and dashboard block. The dashboard itself ("/") is the landing page and has no
 * switch of its own: its blocks do. Keys: `page.<path with dots>` and `dashboard.<block>`.
 */
export const SECTIONS: readonly SectionDef[] = [
  // the dashboard's blocks (apps/crm/components/dashboard/live-dashboard.tsx), top to bottom
  block("ask_ai", "Ask Kalks AI bar", "ai"),
  block("balance_total", "Total balance"),
  block("balance_equity", "Equity"),
  block("wallet_card", "Wallet balance", "wallet"),
  block("today_pnl", "Today's P&L"),
  block("open_positions", "Open positions"),
  block("rewards_card", "Reward points", "rewards"),
  block("banners", "Promotion banners"),
  block("shortcuts", "Shortcuts"),
  block("accounts", "Your accounts"),
  page("/markets", "dashboard", "Markets", "markets"),
  page("/news", "dashboard", "News", "news"),
  page("/calendar", "dashboard", "Calendar", "calendar"),
  page("/accounts", "accounts", "My accounts"),
  page("/accounts/new", "accounts", "Open account"),
  page("/wallet", "wallet", "Wallet overview", "wallet"),
  page("/wallet/deposit", "wallet", "Deposit", "wallet"),
  page("/wallet/withdraw", "wallet", "Withdraw", "wallet"),
  page("/wallet/transfer", "wallet", "Transfer", "wallet"),
  page("/wallet/history", "wallet", "Wallet history", "wallet"),
  page("/portfolio", "portfolio", "Portfolio overview"),
  page("/portfolio/analytics", "portfolio", "Analytics"),
  page("/portfolio/history", "portfolio", "Trade history"),
  page("/portfolio/ledger", "portfolio", "Ledger"),
  page("/portfolio/statements", "portfolio", "Statements"),
  page("/options", "options", "Options", "options"),
  page("/partner", "partner", "Partner dashboard", "ib"),
  page("/partner/clients", "partner", "Partner clients", "ib"),
  page("/partner/network", "partner", "Network", "ib"),
  page("/partner/commissions", "partner", "Commissions", "ib"),
  page("/partner/links", "partner", "Links & materials", "ib"),
  page("/partner/payouts", "partner", "Partner payouts", "ib"),
  page("/social", "social", "Discover", "copy_trading"),
  page("/social/copy", "social", "Copy trading", "copy_trading"),
  page("/social/pamm", "social", "PAMM funds", "pamm"),
  page("/social/investments", "social", "My investments", "pamm"),
  page("/social/managed", "social", "Managed accounts", "mam"),
  page("/social/master", "social", "Become a master", "copy_trading"),
  page("/social/mam", "social", "MAM manager", "mam"),
  page("/prop", "prop", "Challenges", "prop"),
  page("/prop/mine", "prop", "My challenges", "prop"),
  page("/prop/payouts", "prop", "Prop payouts", "prop"),
  page("/prop/certificates", "prop", "Certificates", "prop"),
  page("/rewards", "rewards", "Contests", "rewards"),
  page("/rewards/loyalty", "rewards", "Loyalty", "rewards"),
  page("/rewards/cashback", "rewards", "Cashback", "rewards"),
  page("/rewards/promotions", "rewards", "Promotions", "rewards"),
  page("/developer", "developer", "API keys", "api"),
  page("/developer/webhooks", "developer", "Webhooks", "api"),
  page("/developer/strategies", "developer", "Strategy builder", "algo"),
  page("/developer/deployments", "developer", "Running strategies", "algo"),
  page("/developer/backtests", "developer", "Backtests", "algo"),
  page("/developer/marketplace", "developer", "Marketplace", "algo"),
  page("/developer/docs", "developer", "API docs", "api"),
  page("/academy", "academy", "Courses", "academy"),
  page("/academy/glossary", "academy", "Glossary", "academy"),
  page("/academy/progress", "academy", "My progress", "academy"),
  page("/academy/coach", "academy", "AI Coach", "academy"),
  page("/profile", "profile", "Profile"),
  page("/profile/security", "profile", "Security"),
  page("/profile/verification", "profile", "Verification"),
  page("/profile/viewers", "profile", "View-only access"),
  page("/profile/notifications", "profile", "Notification settings"),
  page("/profile/preferences", "profile", "Preferences"),
  page("/support", "support", "Support"),
];

/** Pages: path → section key. */
const PAGE_SECTIONS: readonly (readonly [string, string])[] = SECTIONS.filter((s) => s.path).map((s) => [s.path!, s.key] as const);

/**
 * BFF paths that serve one page only: they answer 403 `section_disabled` while that page is off for the client (the
 * mobile app's rewrites are judged on the same cookie-route path). APIs shared by several pages or the dashboard (the
 * wallet, accounts, cashback, …) stay open: a hidden page never takes a feature away from another one.
 */
export const API_SECTIONS: readonly (readonly [string, string])[] = [
  ["/api/security/viewers", "page.profile.viewers"],
  ["/api/algo/keys", "page.developer"],
  ["/api/algo/webhooks", "page.developer.webhooks"],
  ["/api/growth/contests", "page.rewards"],
  ["/api/growth/promotions", "page.rewards.promotions"],
  ["/api/growth/vouchers", "page.rewards.loyalty"],
  ["/api/growth/redemptions", "page.rewards.loyalty"],
];

/** The section of a Client Area page or a single-page BFF path, or null (the dashboard itself, shared APIs). */
export function sectionFor(pathname: string): string | null {
  return longest(pathname.startsWith("/api/") ? API_SECTIONS : PAGE_SECTIONS, pathname);
}

/** Whether a page, link or BFF path is shown: its module(s) and its section are on. */
export function pathOn(f: { modules?: Switches | null; sections?: Switches | null } | null | undefined, pathname: string): boolean {
  if (!f) return true;
  const mod = moduleFor(pathname);
  if (mod && !isOn(f.modules, mod)) return false;
  const sec = sectionFor(pathname);
  return !sec || isOn(f.sections, sec);
}

/** Whether a section is on: its own switch and its module's. */
export function sectionOn(f: { modules?: Switches | null; sections?: Switches | null } | null | undefined, key: string): boolean {
  if (!f) return true;
  const def = SECTIONS.find((s) => s.key === key);
  if (def?.module && !isOn(f.modules, def.module)) return false;
  return isOn(f.sections, key);
}

/* ------------------------------------------------------------------ */
/* Kalks Trader                                                        */
/* ------------------------------------------------------------------ */

/**
 * Kalks Trader's toolbox tabs that follow a switch: their module, and the Client Area page of the same content where
 * there is one (`a+b` needs both). Kalks Trader has no client session with the gateway, so it applies the broker's
 * tenant-wide section switches (rules for everyone), not country / group / client rules.
 */
export const TRADER_TABS: Readonly<Record<string, string>> = { news: "news+page.news", calendar: "calendar+page.calendar", ai: "ai", mam: "mam+page.social.mam" };

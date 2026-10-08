import type { NavModule } from "@kalks/ui";

/**
 * Which permission opens which Back Office page (longest matching prefix wins; any listed key is enough).
 * Keys come from the gateway (services/gateway/src/rbac.rs), which is the single source of truth: the shell hides
 * nav items the staff member can't open, LiveGate shows "No access" for them, and every BFF re-checks the key.
 */
export const PAGE_PERMS: Record<string, readonly string[]> = {
  "/": ["stats.read"],
  "/command": ["stats.read"],
  "/clients": ["clients.read"],
  "/clients/kyc": ["kyc.read"],
  "/trading": ["dealing.read"],
  "/trading/accounts": ["accounts.read"],
  "/trading/closures": ["accounts.close", "accounts.close.approve"],
  "/config": ["accounts.read", "spreads.read"],
  "/config/spreads": ["spreads.read"],
  "/options": ["options.read"],
  "/finance": ["finance.read"],
  "/finance/adjustments": ["finance.adjust", "finance.credit", "finance.adjust_approve", "finance.read"],
  "/partners": ["partners.read"],
  "/social": ["social.read"],
  "/social/marketplace": ["algo.read"],
  "/social/api-keys": ["algo.read"],
  "/prop": ["prop.read"],
  "/algo": ["algo.read"],
  "/marketing": ["marketing.read"],
  "/support": ["support.read"],
  "/content": ["content.read"],
  "/analytics": ["reports.read"],
  "/security": ["audit.read"],
  "/security/users": ["audit.read"],
  "/security/sessions": ["sessions.read"],
  "/security/ip": ["security.read"],
  // /org (own access card) is open to everyone; the staff list itself needs staff.read
  "/org/roles": ["staff.read"],
  "/org/desks": ["staff.read"],
  "/org/kpis": ["staff.read"],
  "/settings": ["settings.read"],
  "/brokers": ["owner.tenants", "owner.system", "owner.billing"],
  "/brokers/billing": ["owner.billing"],
  "/brokers/system": ["owner.system"],
  "/brokers/symbols": ["owner.system"],
};

/**
 * Back Office pages that run a client module the Platform Owner can switch off per broker (gateway tenancy.rs, longest
 * prefix wins). "a|b": hidden only when every listed module is off; "" exempts a sub-page from its parent's module.
 * The Platform Owner always sees every page (the options desk, MM and the owner panel serve every broker).
 */
export const PAGE_MODULES: Record<string, string> = {
  "/options": "options",
  "/partners": "ib",
  // masters, applications, fee payouts and caps serve copy trading and PAMM
  "/social": "copy_trading|pamm",
  "/social/followers": "copy_trading",
  "/social/house": "copy_trading",
  "/social/pamm": "pamm",
  "/social/mam": "mam",
  "/social/audit": "copy_trading|pamm|mam",
  "/social/marketplace": "algo",
  "/social/api-keys": "api",
  "/algo": "algo|api",
  "/algo/deployments": "algo",
  "/algo/marketplace": "algo",
  "/algo/keys": "api",
  "/algo/webhooks": "api",
  "/prop": "prop",
  // marketing: bonuses, contests, loyalty rewards, promo codes and cashback are the Rewards module
  "/marketing": "rewards",
  "/marketing/banners": "",
  "/marketing/automation": "",
  "/marketing/campaigns": "",
  "/marketing/reports": "",
  "/content/news": "news",
  "/content/calendar": "calendar",
  "/content/academy": "academy",
};

type Who = { permissions?: string[]; is_owner?: boolean; tenant?: { modules?: Record<string, boolean> } };

/** The module(s) a Back Office page belongs to, or null. */
export function pageModule(pathname: string): string | null {
  const key = Object.keys(PAGE_MODULES)
    .filter((p) => pathname === p || pathname.startsWith(p + "/"))
    .sort((a, b) => b.length - a.length)[0];
  return key ? PAGE_MODULES[key]! || null : null;
}

/** True when the staff member's broker has the page's module(s) switched off (never for the Platform Owner). */
export function moduleHidden(staff: Who, pathname: string): boolean {
  const mods = staff.tenant?.modules;
  const mod = pageModule(pathname);
  if (staff.is_owner || !mods || !mod) return false;
  return mod.split("|").every((m) => mods[m] === false);
}

/** Required permissions for a path (longest prefix), or null when the page is open to every staff member. */
export function pagePerms(pathname: string): readonly string[] | null {
  const key = Object.keys(PAGE_PERMS)
    .filter((p) => (p === "/" ? pathname === "/" : pathname === p || pathname.startsWith(p + "/")))
    .sort((a, b) => b.length - a.length)[0];
  return key ? PAGE_PERMS[key]! : null;
}

export function canOpen(staff: Who, pathname: string): boolean {
  const need = pagePerms(pathname);
  return (!need || need.some((p) => staff.permissions?.includes(p))) && !moduleHidden(staff, pathname);
}

/** Navigation without the pages the staff member can't open, by role or because the broker's module is off (a
 *  module disappears when none of its pages is left). */
export function navFor(nav: NavModule[], staff: Who): NavModule[] {
  return nav.flatMap((m) => {
    if (!m.sub?.length) return canOpen(staff, m.href) ? [m] : [];
    const sub = m.sub.filter((s) => canOpen(staff, s.href));
    if (!sub.length) return [];
    return [{ ...m, href: sub[0]!.href, sub }];
  });
}

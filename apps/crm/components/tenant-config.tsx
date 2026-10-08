"use client";

import * as React from "react";
import type { NavModule } from "@/components/kit";

/** Modules and flags switched per broker (gateway tenant config); everything is on when unknown. */
export type ClientFeatures = { modules: Record<string, boolean>; flags: Record<string, boolean> };

const Ctx = React.createContext<ClientFeatures | null>(null);

export function FeaturesProvider({ value, children }: { value: ClientFeatures | null; children: React.ReactNode }) {
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useFeatures(): ClientFeatures | null {
  return React.useContext(Ctx);
}

/** `on(key)`: whether a module is on for this broker (unknown config or a key the gateway doesn't send = on). */
export function useModules(): (key: string) => boolean {
  const f = React.useContext(Ctx);
  return React.useCallback((key: string) => f?.modules[key] !== false, [f]);
}

/** Nav path → module key (mirrors lib/tenant-config.ts MODULE_PATHS for pages). */
const PAGE_MODULES: [string, string][] = [
  ["/social/mam", "mam"],
  ["/social/managed", "mam"],
  ["/social/pamm", "pamm"],
  ["/social/investments", "pamm"],
  ["/social", "copy_trading"],
  ["/prop", "prop"],
  ["/partner", "ib"],
  ["/developer/strategies", "algo"],
  ["/developer/deployments", "algo"],
  ["/developer/backtests", "algo"],
  ["/developer/marketplace", "algo"],
  ["/developer", "api"],
  ["/academy", "academy"],
  ["/wallet", "wallet"],
  ["/rewards", "rewards"],
  ["/options", "options"],
  ["/markets", "markets"],
  ["/news", "news"],
  ["/calendar", "calendar"],
];

export function pageModule(href: string): string | null {
  const hit = PAGE_MODULES.filter(([p]) => href === p || href.startsWith(p + "/")).sort((a, b) => b[0].length - a[0].length)[0];
  return hit ? hit[1] : null;
}

/** Navigation without the modules this broker has switched off. */
export function navForFeatures(nav: NavModule[], f: ClientFeatures | null): NavModule[] {
  if (!f) return nav;
  const on = (href: string) => {
    const m = pageModule(href);
    return !m || f.modules[m] !== false;
  };
  return nav.flatMap((m) => {
    if (!m.sub?.length) return on(m.href) ? [m] : [];
    const sub = m.sub.filter((s) => on(s.href));
    return sub.length ? [{ ...m, href: sub[0]!.href, sub }] : [];
  });
}

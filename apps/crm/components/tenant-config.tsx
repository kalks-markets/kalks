"use client";

import * as React from "react";
import type { NavModule } from "@/components/kit";
import { isOn, moduleFor, pathOn, sectionOn } from "@kalks/ui/sections";

/** Modules and flags switched per broker and the client's own sections (Client Area pages and dashboard blocks, after
 *  the broker's targeting rules: lib/features.ts); everything is on when unknown. */
export type ClientFeatures = { modules: Record<string, boolean>; flags: Record<string, boolean>; sections?: Record<string, boolean> };

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
  return React.useCallback((key: string) => isOn(f?.modules, key), [f]);
}

/** `show(key)`: whether a section (`dashboard.wallet_card`, `page.news`, …) is shown to this client: its own switch
 *  and its module's. A switched-off section is simply left out. */
export function useSections(): (key: string) => boolean {
  const f = React.useContext(Ctx);
  return React.useCallback((key: string) => sectionOn(f, key), [f]);
}

/** `open(href)`: whether a link to a Client Area page is offered (its module and its section are on). */
export function usePageOn(): (href: string) => boolean {
  const f = React.useContext(Ctx);
  return React.useCallback((href: string) => pathOn(f, href), [f]);
}

/** The module(s) of a nav path (the shared map, packages/ui/src/lib/sections.ts). */
export function pageModule(href: string): string | null {
  return moduleFor(href);
}

/** Navigation without the modules this broker has switched off and the pages hidden for this client (a nav entry
 *  disappears when none of its pages is left). */
export function navForFeatures(nav: NavModule[], f: ClientFeatures | null): NavModule[] {
  if (!f) return nav;
  const on = (href: string) => pathOn(f, href);
  return nav.flatMap((m) => {
    if (!m.sub?.length) return on(m.href) ? [m] : [];
    const sub = m.sub.filter((s) => on(s.href));
    return sub.length ? [{ ...m, href: sub[0]!.href, sub }] : [];
  });
}

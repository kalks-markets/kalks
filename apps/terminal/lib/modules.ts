// Module switches (gateway services/gateway/src/tenancy.rs) as Kalks Trader sees them: the Platform Owner turns a
// module off per broker; its tabs and links disappear here and its BFF routes answer 403 `module_disabled`. The map
// also carries the broker's section switches for everyone (`page.news`, …, docs/SECTIONS.md): a toolbox tab whose
// Client Area page the broker hid is left out too.
// Pure helpers over the shared map (packages/ui/src/lib/sections.ts): the server side is lib/tenant-brand.ts, the
// client side components/modules.tsx.

import { TRADER_TABS, isOn } from "@kalks/ui/sections";

export type Modules = Record<string, boolean>;

/** Whether `key` is on: an unknown config (gateway unreachable) or a key the gateway doesn't send counts as on.
 *  "a+b" needs every module, "a|b" any one of them. */
export function isModuleOn(modules: Modules | null | undefined, key: string): boolean {
  return isOn(modules, key);
}

/** The toolbox tabs that follow a switch (the others are always offered): their module and, where there is one, the
 *  Client Area page of the same content. */
export const TAB_MODULES: Partial<Record<string, string>> = TRADER_TABS;

/** Whether a toolbox tab is offered with these modules. */
export function tabOn(modules: Modules | null | undefined, tab: string): boolean {
  const m = TAB_MODULES[tab];
  return !m || isModuleOn(modules, m);
}

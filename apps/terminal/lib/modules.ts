// Module switches (gateway services/gateway/src/tenancy.rs) as Kalks Trader sees them: the Platform Owner turns a
// module off per broker; its tabs and links disappear here and its BFF routes answer 403 `module_disabled`.
// Pure helpers (no imports): the server side is lib/tenant-brand.ts, the client side components/modules.tsx.

export type Modules = Record<string, boolean>;

/** Whether `key` is on: an unknown config (gateway unreachable) or a key the gateway doesn't send counts as on.
 *  "a+b" needs every module, "a|b" any one of them. */
export function isModuleOn(modules: Modules | null | undefined, key: string): boolean {
  if (key.includes("+")) return key.split("+").every((k) => modules?.[k] !== false);
  if (key.includes("|")) return key.split("|").some((k) => modules?.[k] !== false);
  return modules?.[key] !== false;
}

/** The toolbox tabs that belong to a module (the others are always offered). */
export const TAB_MODULES: Partial<Record<string, string>> = { news: "news", calendar: "calendar", ai: "ai", mam: "mam" };

/** Whether a toolbox tab is offered with these modules. */
export function tabOn(modules: Modules | null | undefined, tab: string): boolean {
  const m = TAB_MODULES[tab];
  return !m || isModuleOn(modules, m);
}

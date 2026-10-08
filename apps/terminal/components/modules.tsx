"use client";

// The broker's module switches in the browser (from the layout: lib/tenant-brand.ts `tenantModules`). Tabs, links and
// panels of a switched-off module are left out; `useModuleOn()(key)` answers like the server's `moduleOn`.
import * as React from "react";
import { isModuleOn, type Modules } from "@/lib/modules";

const Ctx = React.createContext<Modules | null>(null);

export function ModulesProvider({ modules, children }: { modules: Modules | null; children: React.ReactNode }) {
  return <Ctx.Provider value={modules}>{children}</Ctx.Provider>;
}

/** The raw map (null: unknown, everything on). */
export function useModules(): Modules | null {
  return React.useContext(Ctx);
}

/** `on(key)`: whether a module is on for this broker ("a+b": every one). */
export function useModuleOn(): (key: string) => boolean {
  const m = React.useContext(Ctx);
  return React.useCallback((key: string) => isModuleOn(m, key), [m]);
}

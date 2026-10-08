// Server-only: the broker brand and module switches of the visitor's host (gateway /v1/public/tenant-config →
// `branding`, `modules`; the gateway resolves the tenant from tenant_domains). Cached 30 s per host per server
// process, so a module the Platform Owner switches off disappears here within that window.

import { NextResponse } from "next/server";
import type { TenantBrand } from "@kalks/ui";
import { gateway } from "@/lib/gateway";
import { hostOf, requestHost } from "@/lib/tenant-host";
import { isModuleOn, type Modules } from "@/lib/modules";

const TTL_MS = 30_000;
const MAX_HOSTS = 500;
type Entry = { brand: TenantBrand | null; modules: Modules | null };
const cache = new Map<string, { at: number } & Entry>();
/** Refreshes in flight per host: metadata and the layout of one render share one gateway call. */
const inflight = new Map<string, Promise<Entry>>();

async function load(host?: string): Promise<Entry> {
  const key = host ?? (await requestHost()) ?? "";
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < TTL_MS) return hit;
  const pending = inflight.get(key);
  if (pending) return pending;
  const p = (async () => {
    const r = await gateway<{ branding?: TenantBrand; modules?: Modules }>("/v1/public/tenant-config", { host: key || null });
    const ok = r.status === 200 && !!r.data?.branding?.slug;
    // the gateway unreachable: keep what we had (else the stock Kalks look with every module on)
    const e: Entry = ok ? { brand: r.data.branding!, modules: r.data.modules ?? null } : { brand: hit?.brand ?? null, modules: hit?.modules ?? null };
    if (cache.size >= MAX_HOSTS) cache.clear();
    cache.set(key, { at: Date.now(), ...e });
    return e;
  })().finally(() => inflight.delete(key));
  inflight.set(key, p);
  return p;
}

/** Null when the gateway is unreachable: the app then shows the stock Kalks look. */
export async function tenantBrand(host?: string): Promise<TenantBrand | null> {
  return (await load(host)).brand;
}

/** The broker's module switches (null: unknown, everything on). */
export async function tenantModules(host?: string): Promise<Modules | null> {
  return (await load(host)).modules;
}

/** Whether module `key` ("a+b": every one) is on for the visitor's broker. */
export async function moduleOn(key: string, host?: string): Promise<boolean> {
  return isModuleOn(await tenantModules(host), key);
}

/** BFF guard: 403 `module_disabled` (the Client Area's shape) while module `key` is off for the request's broker. */
export async function moduleGate(headers: Headers, key: string): Promise<NextResponse | null> {
  if (await moduleOn(key, hostOf(headers))) return null;
  return NextResponse.json({ error: { code: "module_disabled", message: "This feature isn't available on your account." } }, { status: 403, headers: { "cache-control": "no-store" } });
}

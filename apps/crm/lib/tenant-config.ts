// Server-only: the broker's runtime config from the gateway (/v1/public/tenant-config): maintenance mode,
// modules (D112), feature flags (D146), sections for everyone (docs/SECTIONS.md) and branding (D1). The broker is the one of the visitor's host
// (tenant_domains, resolved by the gateway). Cached for a few seconds per host per server process so the
// proxy can check it on every request; the Platform Owner's switches take effect within that window.

import type { TenantBrand } from "@kalks/ui";
import { gateway } from "@/lib/gateway";
import { requestHost } from "@/lib/tenant-host";

export type TenantConfig = {
  tenant: { slug: string; name: string; status: string; brand: Record<string, string> };
  maintenance: { enabled: boolean; active: boolean; message: string; until: string | null; since: string | null };
  modules: Record<string, boolean>;
  flags: Record<string, boolean>;
  /** Client Area pages and dashboard blocks for everyone (a signed-in client's own: lib/features.ts). */
  sections?: Record<string, boolean>;
  /** Moves on every switch: per-client caches (lib/features.ts) reload when it does. */
  features_version?: number;
  branding?: TenantBrand;
  resolved_by?: "host" | "header" | "default";
};

const TTL_MS = 5_000;
const MAX_HOSTS = 500;
const cache = new Map<string, { at: number; cfg: TenantConfig | null }>();
/** Refreshes in flight per host: the proxy, metadata and layouts of one page load share one gateway call. */
const inflight = new Map<string, Promise<TenantConfig | null>>();

/** Null when the gateway is unreachable (the Client Area then behaves as if everything is on).
 *  `host`: the visitor's host (the proxy passes it; elsewhere it's read from the request). */
export async function tenantConfig(host?: string): Promise<TenantConfig | null> {
  const key = host ?? (await requestHost()) ?? "";
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.cfg;
  const pending = inflight.get(key);
  if (pending) return pending;
  const load = (async () => {
    const r = await gateway<TenantConfig>("/v1/public/tenant-config", { host: key || null });
    const cfg = r.status === 200 && r.data?.maintenance ? r.data : null;
    if (cache.size >= MAX_HOSTS) cache.clear();
    cache.set(key, { at: Date.now(), cfg: cfg ?? hit?.cfg ?? null });
    return cache.get(key)!.cfg;
  })().finally(() => inflight.delete(key));
  inflight.set(key, load);
  return load;
}

/** The broker brand of the current request (null: gateway unreachable → the stock Kalks look). */
export async function tenantBrand(host?: string): Promise<TenantBrand | null> {
  return (await tenantConfig(host))?.branding ?? null;
}

// Which module or section a page or BFF path belongs to: one map for the Client Area, the mobile API, Kalks Trader and
// the Back Office (packages/ui/src/lib/sections.ts, docs/SECTIONS.md).
export { moduleFor, moduleOff, offModule, sectionFor } from "@kalks/ui/sections";

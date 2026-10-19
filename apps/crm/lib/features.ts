// Server-only: the signed-in client's own switches (gateway GET /v1/me/features, docs/SECTIONS.md): the broker's
// modules and flags, and the sections (Client Area pages and dashboard blocks) after the broker's targeting rules for
// the client's country, account groups and the client themself.
//
// Account groups live in the trading engine, which the gateway doesn't call: when the broker has a group rule (the
// gateway says so), the client's accounts are read here and their group codes sent along. The browser never sends
// groups.
//
// Cached per session for 30 s, and dropped as soon as the broker changes a switch: the key carries the tenant
// config's `features_version` (lib/tenant-config.ts, itself refreshed every 5 s).

import { gateway, clientIp } from "@/lib/gateway";
import { Memo, secretKey } from "@/lib/memo";
import { tenantConfig, type TenantConfig } from "@/lib/tenant-config";

export type Switches = Record<string, boolean>;
export type ClientSwitches = { modules: Switches; flags: Switches; sections: Switches };

type MeFeatures = ClientSwitches & { user_id: number; group_rules: boolean; groups: string[]; version: number };

const TRADING_URL = process.env.TRADING_URL ?? "http://127.0.0.1:8090";
const TRADING_TOKEN = process.env.TRADING_INTERNAL_TOKEN ?? "";
const TTL_MS = 30_000;
const cache = new Memo<ClientSwitches | null>(TTL_MS, 5_000);

/** Group codes of the client's trading accounts (empty when the engine is unreachable). */
async function accountGroups(userId: number, tenant: string, h: Headers): Promise<string[]> {
  try {
    const res = await fetch(`${TRADING_URL}/v1/accounts`, {
      headers: { "x-kalks-internal": TRADING_TOKEN, "x-kalks-tenant": tenant, "x-kalks-user-id": String(userId), "x-forwarded-for": clientIp(h) },
      cache: "no-store",
    });
    if (!res.ok) return [];
    const data = (await res.json().catch(() => ({}))) as { accounts?: { group?: unknown }[] };
    const groups = (data.accounts ?? []).map((a) => (typeof a.group === "string" ? a.group.toLowerCase() : "")).filter((g) => /^[a-z0-9-]{1,40}$/.test(g));
    return [...new Set(groups)].sort();
  } catch {
    return [];
  }
}

/** The switches of the client signed in with `token`; null when the session is dead or the gateway unreachable
 *  (callers then fall back to the broker's switches for everyone: `forEveryone`). */
export async function userFeatures(token: string, h: Headers, host?: string): Promise<ClientSwitches | null> {
  const cfg = await tenantConfig(host);
  const key = `${await secretKey(token)}|${host ?? ""}|${cfg?.features_version ?? "?"}`;
  return cache.get(
    key,
    async () => {
      const call = (qs = "") => gateway<MeFeatures>(`/v1/me/features${qs}`, { token, host: host ?? null, ip: clientIp(h), userAgent: h.get("user-agent") });
      let r = await call();
      if (r.status !== 200 || !r.data?.sections) return null;
      if (r.data.group_rules) {
        const groups = await accountGroups(r.data.user_id, cfg?.tenant.slug || "kalks", h);
        if (groups.length) {
          const again = await call(`?groups=${encodeURIComponent(groups.join(","))}`);
          if (again.status === 200 && again.data?.sections) r = again;
        }
      }
      return { modules: r.data.modules ?? {}, flags: r.data.flags ?? {}, sections: r.data.sections };
    },
    (v) => v !== null,
  );
}

/** The broker's switches for everyone (anonymous pages, or the gateway unreachable for the client's own). */
export function forEveryone(cfg: TenantConfig | null): ClientSwitches | null {
  return cfg ? { modules: cfg.modules, flags: cfg.flags, sections: cfg.sections ?? {} } : null;
}

// Server-only: public service status for /status (no internal details: names, state and a coarse latency only).

import { tenantConfig } from "@/lib/tenant-config";

const u = (env: string | undefined, fallback: string) => (env || fallback).replace(/\/$/, "");

// `module`: hidden (and not probed) while the broker has that module switched off
const COMPONENTS: { key: string; module?: string; name: string; url: string }[] = [
  { key: "signin", name: "Client Area & sign-in", url: u(process.env.GATEWAY_URL, "http://127.0.0.1:8080") },
  { key: "prices", name: "Live prices & charts", url: u(process.env.MARKET_DATA_URL, "http://127.0.0.1:8081") },
  { key: "trading", name: "Trading", url: u(process.env.TRADING_URL, "http://127.0.0.1:8090") },
  { key: "wallet", module: "wallet", name: "Deposits & withdrawals", url: u(process.env.WALLET_URL, "http://127.0.0.1:8095") },
  { key: "partners", module: "ib", name: "Partner programme", url: u(process.env.IB_URL, "http://127.0.0.1:8096") },
  { key: "prop", module: "prop", name: "Prop challenges", url: u(process.env.PROP_URL, "http://127.0.0.1:8097") },
  { key: "academy", module: "academy", name: "Academy", url: u(process.env.ACADEMY_URL, "http://127.0.0.1:8098") },
  { key: "algo", module: "algo", name: "Algo trading & API", url: u(process.env.ALGO_URL, "http://127.0.0.1:8099") },
];

export type PublicStatus = {
  status: "operational" | "degraded" | "outage" | "maintenance";
  checked_at: string;
  maintenance: { active: boolean; message: string; until: string | null } | null;
  components: { key: string; name: string; status: "operational" | "degraded" | "outage" }[];
};

async function check(url: string): Promise<"operational" | "degraded" | "outage"> {
  try {
    const r = await fetch(`${url}/health`, { cache: "no-store", signal: AbortSignal.timeout(3000) });
    const b = (await r.json().catch(() => null)) as Record<string, unknown> | null;
    if (r.ok && b && (b.status === "ok" || b.ok === true) && b.db !== false && b.feedConnected !== false) return "operational";
    return r.ok || b ? "degraded" : "outage";
  } catch {
    return "outage";
  }
}

let cache: { at: number; v: PublicStatus } | null = null;

export async function publicStatus(): Promise<PublicStatus> {
  if (cache && Date.now() - cache.at < 15_000) return cache.v;
  const cfg = await tenantConfig();
  const shown = COMPONENTS.filter((c) => !c.module || cfg?.modules[c.module] !== false);
  const components = await Promise.all(shown.map(async (c) => ({ key: c.key, name: c.name, status: await check(c.url) })));
  const m = cfg?.maintenance.active ? { active: true, message: cfg.maintenance.message, until: cfg.maintenance.until } : null;
  const worst = components.some((c) => c.status === "outage") ? "outage" : components.some((c) => c.status === "degraded") ? "degraded" : "operational";
  const v: PublicStatus = { status: m ? "maintenance" : worst, checked_at: new Date().toISOString(), maintenance: m, components };
  cache = { at: Date.now(), v };
  return v;
}

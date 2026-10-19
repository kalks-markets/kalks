// Server-only: probes every Kalks service's /health (Back Office → Brokers → System ops, owner.system).
// Each service listens on loopback; URLs come from the same env vars the BFFs use, with the local defaults.

export type ServiceDef = { key: string; name: string; detail: string; url: string; optional?: boolean };

const u = (env: string | undefined, fallback: string) => (env || fallback).replace(/\/$/, "");

export const SERVICES: ServiceDef[] = [
  { key: "gateway", name: "Gateway", detail: "Sign-in, sessions, RBAC, audit log · :8080", url: u(process.env.GATEWAY_URL, "http://127.0.0.1:8080") },
  { key: "market-data", name: "Market data", detail: "Prices, candles, spreads · :8081", url: u(process.env.MARKET_DATA_URL, "http://127.0.0.1:8081") },
  { key: "trading", name: "Trading engine", detail: "Accounts, orders, margin, ledger, dealing · :8090", url: u(process.env.TRADING_URL, "http://127.0.0.1:8090") },
  { key: "wallet", name: "Wallet", detail: "USDT deposits, withdrawals, transfers · :8095", url: u(process.env.WALLET_URL, "http://127.0.0.1:8095") },
  { key: "ib", name: "IB programme", detail: "Referral tree, commissions, payouts · :8096", url: u(process.env.IB_URL, "http://127.0.0.1:8096") },
  { key: "prop", name: "Prop firm", detail: "Challenges, funded accounts, payouts · :8097", url: u(process.env.PROP_URL, "http://127.0.0.1:8097"), optional: true },
  { key: "academy", name: "Academy", detail: "Courses, exams, certificates · :8098", url: u(process.env.ACADEMY_URL, "http://127.0.0.1:8098") },
  { key: "algo", name: "Algo", detail: "Strategies, backtests, runtime, API keys · :8099", url: u(process.env.ALGO_URL, "http://127.0.0.1:8099") },
  { key: "support", name: "Support", detail: "Tickets and notifications · :8100", url: u(process.env.SUPPORT_URL, "http://127.0.0.1:8100"), optional: true },
  { key: "growth", name: "Growth", detail: "Bonuses, contests, rewards · :8101", url: u(process.env.GROWTH_URL, "http://127.0.0.1:8101"), optional: true },
  { key: "content", name: "Content", detail: "Editable pages, media library · :8105", url: u(process.env.CONTENT_URL, "http://127.0.0.1:8105"), optional: true },
];

export type Probe = {
  key: string;
  name: string;
  detail: string;
  optional: boolean;
  status: "up" | "degraded" | "down";
  http: number | null;
  latency_ms: number;
  facts: Record<string, string>;
};

function flat(v: unknown, prefix = "", out: Record<string, string> = {}): Record<string, string> {
  if (v && typeof v === "object" && !Array.isArray(v)) {
    for (const [k, x] of Object.entries(v)) {
      if (Object.keys(out).length >= 14) break;
      flat(x, prefix ? `${prefix}.${k}` : k, out);
    }
  } else if (prefix) {
    out[prefix] = Array.isArray(v) ? v.join(", ") || "none" : String(v);
  }
  return out;
}

export async function probe(s: ServiceDef): Promise<Probe> {
  const t0 = Date.now();
  try {
    const r = await fetch(`${s.url}/health`, { cache: "no-store", signal: AbortSignal.timeout(3500) });
    const body = (await r.json().catch(() => null)) as Record<string, unknown> | null;
    const ok = r.ok && body !== null && (body.status === "ok" || body.ok === true);
    const dbDown = body?.db === false || body?.feedConnected === false;
    return {
      key: s.key,
      name: s.name,
      detail: s.detail,
      optional: !!s.optional,
      status: ok && !dbDown ? "up" : r.ok || body ? "degraded" : "down",
      http: r.status,
      latency_ms: Date.now() - t0,
      facts: body ? flat(body) : {},
    };
  } catch {
    return { key: s.key, name: s.name, detail: s.detail, optional: !!s.optional, status: "down", http: null, latency_ms: Date.now() - t0, facts: {} };
  }
}

export async function probeAll(): Promise<Probe[]> {
  return Promise.all(SERVICES.map(probe));
}

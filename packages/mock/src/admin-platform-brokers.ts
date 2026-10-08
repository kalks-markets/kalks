/**
 * Back Office — Brokers (Platform Owner) mock data.
 * Tenants, billing, module/plan matrix, global symbol master, system ops and feature flags.
 * Every export is prefixed BRK_ to avoid collisions in the admin-platform barrel.
 */
import { seeded } from "./rng";
import { PEOPLE, type Person } from "./people";
import { INSTRUMENTS, type AssetClass } from "./symbols";

/* ------------------------------------------------------------------ */
/* Plans                                                               */
/* ------------------------------------------------------------------ */

export type BrkPlan = "starter" | "growth" | "enterprise" | "owner";
export type BrkTenantStatus = "active" | "trial" | "suspended" | "onboarding";

export interface BrkPlanDef {
  key: Exclude<BrkPlan, "owner">;
  name: string;
  licence: number; // monthly USD
  setupFee: number;
  revShare: number; // % of broker net revenue
  maxClients: number;
  maxStaff: number;
  maxSymbols: number;
  blurb: string;
}

export const BRK_PLANS: BrkPlanDef[] = [
  { key: "starter", name: "Starter", licence: 2900, setupFee: 15000, revShare: 25, maxClients: 2500, maxStaff: 10, maxSymbols: 120, blurb: "Launch a regional brand fast" },
  { key: "growth", name: "Growth", licence: 7900, setupFee: 35000, revShare: 20, maxClients: 15000, maxStaff: 40, maxSymbols: 400, blurb: "Scale IB networks and social trading" },
  { key: "enterprise", name: "Enterprise", licence: 16000, setupFee: 75000, revShare: 15, maxClients: 100000, maxStaff: 250, maxSymbols: 2000, blurb: "Full stack, dedicated infra, SLA 99.95%" },
];

export const BRK_PLAN_LABEL: Record<BrkPlan, string> = { starter: "Starter", growth: "Growth", enterprise: "Enterprise", owner: "Owner" };

/* ------------------------------------------------------------------ */
/* Modules                                                             */
/* ------------------------------------------------------------------ */

/** The gateway's module keys (services/gateway/src/tenancy.rs) plus the showcase's plan-only items (trading, stocks,
 *  mobile, kyc_auto), so the demo Brokers pages read like the live ones. */
export type BrkModuleKey =
  | "trading"
  | "wallet"
  | "ib"
  | "copy_trading"
  | "pamm"
  | "mam"
  | "prop"
  | "rewards"
  | "academy"
  | "ai"
  | "api"
  | "algo"
  | "options"
  | "news"
  | "calendar"
  | "markets"
  | "support_chat"
  | "stocks"
  | "mobile"
  | "kyc_auto";

export interface BrkModule {
  key: BrkModuleKey;
  name: string;
  description: string;
  addOn: number; // monthly add-on price when not included in plan
  plans: Record<"starter" | "growth" | "enterprise", boolean>;
  core?: boolean;
  tenants: number; // tenants currently with module on
}

export const BRK_MODULES: BrkModule[] = [
  { key: "trading", name: "Trading", description: "Web terminal, MT-style accounts, Live/Demo, hedging & netting", addOn: 0, plans: { starter: true, growth: true, enterprise: true }, core: true, tenants: 10 },
  { key: "wallet", name: "Wallet", description: "USDT TRC20 deposits & withdrawals, internal transfers", addOn: 0, plans: { starter: true, growth: true, enterprise: true }, core: true, tenants: 10 },
  { key: "ib", name: "Partner / IB", description: "Multi-tier commissions, CPA, sub-IB trees, payouts", addOn: 900, plans: { starter: false, growth: true, enterprise: true }, tenants: 8 },
  { key: "copy_trading", name: "Copy trading", description: "Strategy providers, followers, performance fees", addOn: 1200, plans: { starter: false, growth: true, enterprise: true }, tenants: 6 },
  { key: "pamm", name: "PAMM", description: "Managed accounts, allocation by equity, fee high-water mark", addOn: 1400, plans: { starter: false, growth: false, enterprise: true }, tenants: 4 },
  { key: "mam", name: "MAM", description: "Multi-account managers, allocation methods, managed accounts", addOn: 900, plans: { starter: false, growth: false, enterprise: true }, tenants: 3 },
  { key: "prop", name: "Prop challenges", description: "Evaluation phases, rules engine, funded accounts, payouts", addOn: 1800, plans: { starter: false, growth: false, enterprise: true }, tenants: 4 },
  { key: "rewards", name: "Contests & rewards", description: "Demo/live contests, leaderboards, loyalty points", addOn: 600, plans: { starter: false, growth: true, enterprise: true }, tenants: 6 },
  { key: "academy", name: "Academy", description: "Courses, lessons, quizzes and certificates", addOn: 400, plans: { starter: true, growth: true, enterprise: true }, tenants: 9 },
  { key: "ai", name: "AI Coach", description: "Trade journal insights, market Q&A, risk nudges", addOn: 1500, plans: { starter: false, growth: false, enterprise: true }, tenants: 3 },
  { key: "api", name: "API & Algo", description: "REST/WebSocket API keys, FIX bridge, rate limits", addOn: 1100, plans: { starter: false, growth: true, enterprise: true }, tenants: 5 },
  { key: "algo", name: "Strategy builder", description: "No-code bots, backtesting, paper deployment", addOn: 800, plans: { starter: false, growth: false, enterprise: true }, tenants: 3 },
  { key: "options", name: "Options", description: "FX options: chain, strategies, order book and RFQs", addOn: 1600, plans: { starter: false, growth: false, enterprise: true }, tenants: 2 },
  { key: "news", name: "News", description: "Market news, news map and the daily brief", addOn: 0, plans: { starter: true, growth: true, enterprise: true }, core: true, tenants: 10 },
  { key: "calendar", name: "Economic calendar", description: "Economic events, reminders and alerts", addOn: 0, plans: { starter: true, growth: true, enterprise: true }, core: true, tenants: 10 },
  { key: "markets", name: "Markets", description: "Market overview: quotes, movers and heat map", addOn: 0, plans: { starter: true, growth: true, enterprise: true }, core: true, tenants: 10 },
  { key: "support_chat", name: "Support chat", description: "AI bot and live agents in the Client Area", addOn: 300, plans: { starter: true, growth: true, enterprise: true }, tenants: 9 },
  { key: "stocks", name: "Stock CFDs", description: "US/EU equities with corporate actions & dividends", addOn: 700, plans: { starter: false, growth: true, enterprise: true }, tenants: 7 },
  { key: "mobile", name: "Branded mobile app", description: "iOS & Android builds under tenant brand, push", addOn: 2200, plans: { starter: false, growth: false, enterprise: true }, tenants: 4 },
  { key: "kyc_auto", name: "KYC automation", description: "Sumsub liveness, document OCR, AML screening", addOn: 500, plans: { starter: true, growth: true, enterprise: true }, tenants: 10 },
];

/* ------------------------------------------------------------------ */
/* Tenants                                                             */
/* ------------------------------------------------------------------ */

export interface BrkTenant {
  id: string;
  name: string;
  legalName: string;
  regNo: string;
  mark: string;
  color: string;
  accent: string;
  domain: string;
  country: string;
  countryName: string;
  regulator: string;
  plan: BrkPlan;
  status: BrkTenantStatus;
  clients: number;
  activeTraders: number;
  staff: number;
  symbols: number;
  volume30d: number; // USD notional
  netRevenue30d: number;
  revSharePct: number;
  revShare30d: number;
  mrr: number;
  setupFee: number;
  maxClients: number;
  maxStaff: number;
  maxSymbols: number;
  billingEmail: string;
  contact: Person;
  createdAt: string;
  modules: BrkModuleKey[];
  trend: number[];
  onboardingStep?: number; // 0..5 for onboarding tenants
}

type Seed = [
  name: string,
  legal: string,
  regNo: string,
  mark: string,
  color: string,
  accent: string,
  domain: string,
  country: string,
  countryName: string,
  regulator: string,
  plan: BrkPlan,
  status: BrkTenantStatus,
  clients: number,
  staff: number,
  symbols: number,
  volume30d: number,
  netRevenue30d: number,
  createdAt: string,
  contactIdx: number,
  modules: BrkModuleKey[],
];

const ALL_MODS: BrkModuleKey[] = BRK_MODULES.map((m) => m.key);
const GROWTH_MODS: BrkModuleKey[] = ["trading", "wallet", "ib", "copy_trading", "rewards", "academy", "api", "stocks", "kyc_auto"];
const STARTER_MODS: BrkModuleKey[] = ["trading", "wallet", "academy", "kyc_auto"];

const SEEDS: Seed[] = [
  ["Kalks Markets", "Kalks Markets Ltd", "SC-8421096", "K", "#ff5a1f", "#e9b949", "kalks.com", "sc", "Seychelles", "FSA Seychelles SD142", "owner", "active", 48210, 184, 1240, 18_420_000_000, 3_842_000, "2024-02-01T09:00:00Z", 4, ALL_MODS],
  ["Aurum FX", "Aurum Financial Services LLC", "DIFC-CL4471", "A", "#e9b949", "#ff8a3d", "aurumfx.com", "ae", "United Arab Emirates", "DFSA Cat. 3A", "enterprise", "active", 12480, 62, 860, 4_210_000_000, 1_064_000, "2024-09-14T08:00:00Z", 1, ["trading", "wallet", "ib", "copy_trading", "pamm", "prop", "rewards", "academy", "ai", "api", "stocks", "mobile", "kyc_auto"]],
  ["NovaTrade Asia", "NovaTrade Asia Pte. Ltd.", "UEN 202318842K", "N", "#38bdf8", "#22c55e", "novatrade.asia", "sg", "Singapore", "MAS CMS100942", "growth", "active", 8960, 38, 380, 2_730_000_000, 612_000, "2025-01-20T08:00:00Z", 8, GROWTH_MODS],
  ["Dunes Capital", "Dunes Capital Markets LLC", "CN-3902215", "D", "#22c55e", "#e9b949", "dunescapital.ae", "ae", "United Arab Emirates", "SCA 20200000154", "growth", "active", 5340, 24, 310, 1_620_000_000, 388_000, "2025-03-02T08:00:00Z", 5, ["trading", "wallet", "ib", "copy_trading", "academy", "stocks", "kyc_auto"]],
  ["Meridian Prime", "Meridian Prime Markets Ltd", "CY-HE412887", "M", "#14b8a6", "#f5f5f7", "meridianprime.eu", "cy", "Cyprus", "CySEC 418/22", "enterprise", "active", 9120, 71, 1120, 3_140_000_000, 804_000, "2024-11-05T08:00:00Z", 12, ["trading", "wallet", "ib", "copy_trading", "pamm", "prop", "rewards", "academy", "api", "algo", "stocks", "mobile", "kyc_auto"]],
  ["Kestrel Markets", "Kestrel Markets (Pty) Ltd", "2025/482113/07", "K", "#f04438", "#f5f5f7", "kestrelmarkets.co.za", "za", "South Africa", "FSCA FSP 53921", "starter", "trial", 1240, 7, 96, 214_000_000, 46_200, "2026-08-28T08:00:00Z", 23, STARTER_MODS],
  ["Solace Trade", "Solace Trade Serviços Ltda", "CNPJ 48.221.905/0001-12", "S", "#ff8a3d", "#38bdf8", "solacetrade.com.br", "br", "Brazil", "FSC Mauritius GB24203118", "growth", "suspended", 3870, 19, 240, 942_000_000, 201_000, "2025-05-18T08:00:00Z", 16, ["trading", "wallet", "ib", "rewards", "academy", "stocks", "kyc_auto"]],
  ["Lotus Pacific FX", "Lotus Pacific Securities JSC", "VN-0316892271", "L", "#a3e635", "#e9b949", "lotuspacific.vn", "vn", "Vietnam", "VFSC 14912", "starter", "onboarding", 0, 3, 0, 0, 0, "2026-09-16T08:00:00Z", 3, STARTER_MODS],
  ["Atlas Brokerage", "Atlas Brokerage S.A. de C.V.", "MX-AB2019-0442", "A", "#fb7185", "#e9b949", "atlasbrokerage.mx", "mx", "Mexico", "FSA Seychelles SD188", "starter", "active", 2110, 9, 118, 391_000_000, 84_600, "2025-08-11T08:00:00Z", 11, ["trading", "wallet", "academy", "kyc_auto", "ib"]],
  ["Cedar Global", "Cedar Global Markets Ltd", "RC-1873302", "C", "#94a3b8", "#22c55e", "cedarglobal.ng", "ng", "Nigeria", "SEC Nigeria SEC/BD/211", "growth", "active", 4420, 21, 292, 1_108_000_000, 262_000, "2025-06-30T08:00:00Z", 7, ["trading", "wallet", "ib", "copy_trading", "rewards", "academy", "kyc_auto"]],
];

export const BRK_TENANTS: BrkTenant[] = SEEDS.map((s, i) => {
  const [name, legalName, regNo, mark, color, accent, domain, country, countryName, regulator, plan, status, clients, staff, symbols, volume30d, netRevenue30d, createdAt, contactIdx, modules] = s;
  const def = BRK_PLANS.find((p) => p.key === plan);
  const r = seeded(700 + i * 13);
  const revSharePct = plan === "owner" ? 0 : def!.revShare;
  const base = volume30d / 1e8 || 1;
  const trend = Array.from({ length: 24 }, (_, k) => base * (0.72 + k * 0.012 + r.range(-0.06, 0.06)));
  return {
    id: `tnt_${String(i + 1).padStart(3, "0")}`,
    name,
    legalName,
    regNo,
    mark,
    color,
    accent,
    domain,
    country,
    countryName,
    regulator,
    plan,
    status,
    clients,
    activeTraders: Math.round(clients * r.range(0.22, 0.34)),
    staff,
    symbols,
    volume30d,
    netRevenue30d,
    revSharePct,
    revShare30d: Math.round((netRevenue30d * revSharePct) / 100),
    mrr: plan === "owner" || status !== "active" ? 0 : def!.licence,
    setupFee: plan === "owner" ? 0 : def!.setupFee,
    maxClients: plan === "owner" ? 250000 : def!.maxClients,
    maxStaff: plan === "owner" ? 500 : def!.maxStaff,
    maxSymbols: plan === "owner" ? 5000 : def!.maxSymbols,
    billingEmail: `billing@${domain}`,
    contact: PEOPLE[contactIdx]!,
    createdAt,
    modules,
    trend,
    onboardingStep: status === "onboarding" ? 2 : status === "trial" ? 4 : undefined,
  };
});

export function brkTenant(id: string): BrkTenant {
  return BRK_TENANTS.find((t) => t.id === id) ?? BRK_TENANTS[0]!;
}

/** Monthly recurring revenue: licence + trailing 30d rev-share (excluding owner). */
export const BRK_TOTALS = (() => {
  const paying = BRK_TENANTS.filter((t) => t.plan !== "owner");
  const licence = paying.reduce((s, t) => s + t.mrr, 0);
  const revShare = paying.filter((t) => t.status !== "suspended").reduce((s, t) => s + t.revShare30d, 0);
  return {
    tenants: BRK_TENANTS.length,
    active: BRK_TENANTS.filter((t) => t.status === "active").length,
    trial: BRK_TENANTS.filter((t) => t.status === "trial").length,
    onboarding: BRK_TENANTS.filter((t) => t.status === "onboarding").length,
    suspended: BRK_TENANTS.filter((t) => t.status === "suspended").length,
    clients: BRK_TENANTS.reduce((s, t) => s + t.clients, 0),
    whiteLabelClients: paying.reduce((s, t) => s + t.clients, 0),
    volume30d: BRK_TENANTS.reduce((s, t) => s + t.volume30d, 0),
    licence,
    revShare,
    mrr: licence + revShare,
  };
})();

/* ------------------------------------------------------------------ */
/* MRR history (12 months)                                             */
/* ------------------------------------------------------------------ */

export const BRK_MRR_HISTORY: { month: string; licence: number; revShare: number; tenants: number }[] = [
  { month: "Oct 25", licence: 34700, revShare: 318000, tenants: 5 },
  { month: "Nov 25", licence: 34700, revShare: 341000, tenants: 5 },
  { month: "Dec 25", licence: 34700, revShare: 329000, tenants: 5 },
  { month: "Jan 26", licence: 37600, revShare: 362000, tenants: 6 },
  { month: "Feb 26", licence: 45500, revShare: 384000, tenants: 7 },
  { month: "Mar 26", licence: 45500, revShare: 401000, tenants: 7 },
  { month: "Apr 26", licence: 53400, revShare: 437000, tenants: 8 },
  { month: "May 26", licence: 53400, revShare: 462000, tenants: 8 },
  { month: "Jun 26", licence: 61300, revShare: 471000, tenants: 8 },
  { month: "Jul 26", licence: 61300, revShare: 506000, tenants: 8 },
  { month: "Aug 26", licence: 66500, revShare: 531000, tenants: 9 },
  { month: "Sep 26", licence: 0, revShare: 0, tenants: 10 },
];
// current month computed from live tenant data
BRK_MRR_HISTORY[11]!.licence = BRK_TOTALS.licence;
BRK_MRR_HISTORY[11]!.revShare = BRK_TOTALS.revShare;

/* ------------------------------------------------------------------ */
/* Invoices                                                            */
/* ------------------------------------------------------------------ */

export type BrkInvoiceStatus = "paid" | "pending" | "overdue";
export interface BrkInvoice {
  number: string;
  tenantId: string;
  period: string;
  issued: string;
  due: string;
  setup: number;
  licence: number;
  revShare: number;
  total: number;
  status: BrkInvoiceStatus;
  paidAt?: string;
  daysOverdue?: number;
  txHash?: string;
}

const PERIODS = [
  { label: "Jun 2026", issued: "2026-07-01T06:00:00Z", due: "2026-07-15T20:59:00Z", k: 6 },
  { label: "Jul 2026", issued: "2026-08-01T06:00:00Z", due: "2026-08-15T20:59:00Z", k: 7 },
  { label: "Aug 2026", issued: "2026-09-01T06:00:00Z", due: "2026-09-15T20:59:00Z", k: 8 },
];

export const BRK_INVOICES: BrkInvoice[] = (() => {
  const out: BrkInvoice[] = [];
  let n = 118;
  const r = seeded(4411);
  for (const p of PERIODS) {
    for (const t of BRK_TENANTS) {
      if (t.plan === "owner" || t.status === "onboarding") continue;
      if (t.id === "tnt_006" && p.k < 8) continue; // Kestrel trial started late Aug
      const def = BRK_PLANS.find((x) => x.key === t.plan)!;
      const setup = t.createdAt.startsWith(`2026-0${p.k}`) ? def.setupFee : 0;
      const licence = t.status === "trial" ? 0 : def.licence;
      const revShare = Math.round(t.revShare30d * r.range(0.86, 1.04));
      const total = setup + licence + revShare;
      let status: BrkInvoiceStatus = "paid";
      let daysOverdue: number | undefined;
      if (p.k === 8) {
        if (t.id === "tnt_007") {
          status = "overdue";
          daysOverdue = 9;
        } else if (t.id === "tnt_009") {
          status = "overdue";
          daysOverdue = 9;
        } else if (t.id === "tnt_006" || t.id === "tnt_010" || t.id === "tnt_003") status = "pending";
      }
      if (p.k === 7 && t.id === "tnt_007") {
        status = "overdue";
        daysOverdue = 40;
      }
      n += 1;
      out.push({
        number: `INV-2026-${String(p.k + 1).padStart(2, "0")}-${String(n).padStart(4, "0")}`,
        tenantId: t.id,
        period: p.label,
        issued: p.issued,
        due: p.due,
        setup,
        licence,
        revShare,
        total,
        status,
        daysOverdue,
        paidAt: status === "paid" ? p.due.replace(/-(1\d)T/, (_m, d) => `-${String(Number(d) - r.int(2, 9)).padStart(2, "0")}T`) : undefined,
        txHash: status === "paid" ? `${r.int(1e7, 9e7).toString(16)}${r.int(1e7, 9e7).toString(16)}a4c1${r.int(1e7, 9e7).toString(16)}e9` : undefined,
      });
    }
  }
  // Setup fee for Kestrel trial (Aug) and Lotus Pacific onboarding (Sep, issued now)
  out.push({ number: "INV-2026-09-0151", tenantId: "tnt_008", period: "Setup", issued: "2026-09-16T06:00:00Z", due: "2026-09-30T20:59:00Z", setup: 15000, licence: 0, revShare: 0, total: 15000, status: "pending" });
  return out.reverse();
})();

/* ------------------------------------------------------------------ */
/* Global symbol master                                                */
/* ------------------------------------------------------------------ */

export type BrkSymbolStatus = "active" | "close-only" | "halted";
export interface BrkSymbol {
  symbol: string;
  name: string;
  assetClass: AssetClass;
  digits: number;
  contractSize: number;
  feedSymbol: string; // Infoways primary
  lpSymbol: string; // liquidity provider
  backupFeed: string;
  status: BrkSymbolStatus;
  tenantsEnabled: number;
  lastSync: string;
  sessions: string;
  minLot: number;
  maxLot: number;
}

function lpName(sym: string, cls: AssetClass) {
  if (cls === "forex" || cls === "metals" || cls === "crypto") return `${sym.slice(0, 3)}/${sym.slice(3)}`;
  if (cls === "stocks") return `${sym}.OQ`;
  if (cls === "energies") return sym === "USOIL" ? "WTI.c" : "BRENT.c";
  const m: Record<string, string> = { US30: "DJ30.c", NAS100: "NDX100.c", SPX500: "SPX500.c", GER40: "DAX40.c", UK100: "FTSE100.c", JP225: "NIK225.c" };
  return m[sym] ?? `${sym}.c`;
}
function feedName(sym: string, cls: AssetClass) {
  if (cls === "indices") return ({ US30: "DJI", NAS100: "NDX", SPX500: "SPX", GER40: "DAX", UK100: "UKX", JP225: "N225" } as Record<string, string>)[sym] ?? sym;
  if (cls === "energies") return sym === "USOIL" ? "CL1!" : "BZ1!";
  return sym;
}

export const BRK_SYMBOLS: BrkSymbol[] = INSTRUMENTS.map((i, k) => {
  const r = seeded(9100 + k);
  return {
    symbol: i.symbol,
    name: i.name,
    assetClass: i.assetClass,
    digits: i.digits,
    contractSize: i.contractSize,
    feedSymbol: feedName(i.symbol, i.assetClass),
    lpSymbol: lpName(i.symbol, i.assetClass),
    backupFeed: i.assetClass === "crypto" ? "Binance WS" : i.assetClass === "stocks" ? "Polygon.io" : "Finalto FIX",
    status: i.symbol === "USDINR" ? "close-only" : i.symbol === "NFLX" ? "halted" : "active",
    tenantsEnabled: i.assetClass === "stocks" ? r.int(4, 7) : i.symbol === "USDINR" ? 2 : r.int(7, 10),
    lastSync: `2026-09-${String(r.int(18, 23)).padStart(2, "0")}T${String(r.int(6, 20)).padStart(2, "0")}:${String(r.int(10, 59))}:00Z`,
    sessions: i.assetClass === "crypto" ? "24/7" : i.assetClass === "stocks" ? "Mon–Fri 16:30–23:00" : i.assetClass === "indices" ? "Mon–Fri 01:05–23:55" : "Mon 00:05 – Fri 23:55",
    minLot: 0.01,
    maxLot: i.assetClass === "stocks" ? 500 : i.assetClass === "crypto" ? 20 : 100,
  };
});

export interface BrkFeed {
  name: string;
  role: string;
  protocol: string;
  latency: number;
  uptime: number;
  ticks: number; // per second
  symbols: number;
  status: "live" | "degraded" | "standby";
  series: number[];
}

export const BRK_FEEDS: BrkFeed[] = [
  { name: "Infoways", role: "Primary", protocol: "WebSocket", latency: 38, uptime: 99.98, ticks: 1240, symbols: 1180, status: "live", series: [41, 39, 38, 44, 40, 37, 36, 39, 42, 38, 37, 40, 38, 36, 39, 41, 38, 37, 38, 38] },
  { name: "Finalto FIX", role: "LP & backup", protocol: "FIX 4.4", latency: 12, uptime: 99.99, ticks: 2210, symbols: 640, status: "live", series: [13, 12, 12, 14, 12, 11, 12, 13, 12, 12, 11, 12, 13, 12, 12, 11, 12, 12, 13, 12] },
  { name: "Binance WS", role: "Crypto backup", protocol: "WebSocket", latency: 64, uptime: 99.91, ticks: 880, symbols: 42, status: "live", series: [58, 61, 66, 70, 64, 62, 60, 68, 72, 65, 63, 61, 66, 64, 62, 67, 64, 63, 65, 64] },
  { name: "Polygon.io", role: "Equities backup", protocol: "REST + WS", latency: 214, uptime: 99.42, ticks: 96, symbols: 310, status: "degraded", series: [140, 150, 138, 162, 180, 176, 190, 205, 198, 222, 240, 231, 210, 218, 226, 212, 204, 219, 214, 214] },
];

/* ------------------------------------------------------------------ */
/* Module overrides per tenant                                         */
/* ------------------------------------------------------------------ */

export interface BrkOverride {
  tenantId: string;
  module: BrkModuleKey;
  state: "on" | "off";
  reason: string;
  by: Person;
  at: string;
  price: number;
}

export const BRK_OVERRIDES: BrkOverride[] = [
  { tenantId: "tnt_003", module: "prop", state: "on", reason: "Paid add-on — Q4 prop launch in SEA", by: PEOPLE[9]!, at: "2026-09-12T10:22:00Z", price: 1800 },
  { tenantId: "tnt_004", module: "api", state: "off", reason: "SCA pending approval for algo access", by: PEOPLE[4]!, at: "2026-08-28T14:05:00Z", price: 0 },
  { tenantId: "tnt_004", module: "rewards", state: "off", reason: "Local promo rules — contests disabled", by: PEOPLE[1]!, at: "2026-07-19T09:40:00Z", price: 0 },
  { tenantId: "tnt_009", module: "ib", state: "on", reason: "Paid add-on — LATAM IB network", by: PEOPLE[11]!, at: "2026-06-03T16:12:00Z", price: 900 },
  { tenantId: "tnt_002", module: "algo", state: "off", reason: "DFSA review of automated strategies", by: PEOPLE[4]!, at: "2026-09-02T08:30:00Z", price: 0 },
  { tenantId: "tnt_010", module: "api", state: "off", reason: "Not requested by tenant", by: PEOPLE[20]!, at: "2026-07-01T12:00:00Z", price: 0 },
  { tenantId: "tnt_010", module: "stocks", state: "off", reason: "SEC Nigeria equity CFD restriction", by: PEOPLE[4]!, at: "2026-07-01T12:00:00Z", price: 0 },
];

/* ------------------------------------------------------------------ */
/* System ops                                                          */
/* ------------------------------------------------------------------ */

export type BrkSvcStatus = "operational" | "degraded" | "down" | "maintenance";
export interface BrkService {
  key: string;
  name: string;
  host: string;
  status: BrkSvcStatus;
  latency: number;
  unit: string;
  uptime: number;
  version: string;
  instances: string;
  series: number[];
  bars: number[]; // last 30 days, 0 ok 1 degraded 2 outage
  metric: string;
}

function svcSeries(seed: number, base: number, jitter: number) {
  const r = seeded(seed);
  return Array.from({ length: 28 }, () => Math.max(1, base + r.normal() * jitter));
}
function svcBars(_seed: number, bad: number[] = [], worse: number[] = []) {
  return Array.from({ length: 30 }, (_, i) => (worse.includes(i) ? 2 : bad.includes(i) ? 1 : 0));
}

export const BRK_SERVICES: BrkService[] = [
  { key: "api", name: "Public API", host: "api.kalks.com · eu-central-1", status: "operational", latency: 42, unit: "ms p95", uptime: 99.99, version: "v4.18.2", instances: "12 / 12 pods", series: svcSeries(1, 42, 4), bars: svcBars(1, [11]), metric: "4.8k req/s" },
  { key: "ws", name: "WebSocket gateway", host: "stream.kalks.com · 3 regions", status: "operational", latency: 18, unit: "ms p95", uptime: 99.98, version: "v2.9.0", instances: "9 / 9 pods", series: svcSeries(2, 18, 2), bars: svcBars(2, [22]), metric: "61.2k conns" },
  { key: "feed", name: "Price feed", host: "Infoways → aggregator", status: "operational", latency: 38, unit: "ms tick", uptime: 99.98, version: "v3.4.1", instances: "4 / 4 pods", series: svcSeries(3, 38, 3), bars: svcBars(3, [5]), metric: "1,240 ticks/s" },
  { key: "matching", name: "Matching engine", host: "me-01/02 · bare metal LD4", status: "operational", latency: 0.8, unit: "ms exec", uptime: 100, version: "v7.2.0", instances: "2 / 2 active-active", series: svcSeries(4, 0.8, 0.08), bars: svcBars(4), metric: "312 orders/s" },
  { key: "tron", name: "Wallet watcher TRON", host: "tron-watch · TronGrid + full node", status: "degraded", latency: 2.4, unit: "s block lag", uptime: 99.71, version: "v1.12.4", instances: "2 / 3 pods", series: svcSeries(5, 2.4, 0.5).map((v, i) => (i > 20 ? v + 1.8 : v)), bars: svcBars(5, [3, 17, 29]), metric: "Block 66,412,908" },
  { key: "workers", name: "Workers", host: "jobs · BullMQ", status: "operational", latency: 140, unit: "ms avg job", uptime: 99.97, version: "v5.3.3", instances: "24 / 24 workers", series: svcSeries(6, 140, 18), bars: svcBars(6, [9]), metric: "18.4k jobs/h" },
  { key: "postgres", name: "Postgres", host: "pg-primary + 2 replicas", status: "operational", latency: 3.1, unit: "ms query p95", uptime: 99.99, version: "16.4", instances: "Replica lag 0.2s", series: svcSeries(7, 3.1, 0.3), bars: svcBars(7), metric: "1.84 TB · 62% used" },
  { key: "redis", name: "Redis", host: "redis-cluster · 6 shards", status: "operational", latency: 0.4, unit: "ms p99", uptime: 100, version: "7.2.5", instances: "Hit rate 97.8%", series: svcSeries(8, 0.4, 0.05), bars: svcBars(8), metric: "38.2 GB mem" },
  { key: "storage", name: "Object storage", host: "S3 · kyc-docs, statements", status: "operational", latency: 64, unit: "ms GET p95", uptime: 99.99, version: "S3 API", instances: "Versioning on", series: svcSeries(9, 64, 7), bars: svcBars(9, [14]), metric: "4.1 TB · 12.8M objects" },
];

export interface BrkQueue {
  name: string;
  depth: number;
  rate: number;
  consumers: number;
  oldest: string;
  status: "healthy" | "backlog" | "stalled";
  capacity: number;
}
export const BRK_QUEUES: BrkQueue[] = [
  { name: "deposits.tron.confirm", depth: 214, rate: 38, consumers: 4, oldest: "2m 14s", status: "backlog", capacity: 400 },
  { name: "withdrawals.payout", depth: 12, rate: 6, consumers: 2, oldest: "41s", status: "healthy", capacity: 200 },
  { name: "kyc.sumsub.webhooks", depth: 3, rate: 11, consumers: 3, oldest: "4s", status: "healthy", capacity: 200 },
  { name: "ib.commission.calc", depth: 1840, rate: 420, consumers: 8, oldest: "12s", status: "healthy", capacity: 5000 },
  { name: "email.transactional", depth: 66, rate: 54, consumers: 4, oldest: "3s", status: "healthy", capacity: 1000 },
  { name: "reports.statements", depth: 0, rate: 0, consumers: 2, oldest: "—", status: "healthy", capacity: 500 },
];

export interface BrkCron {
  name: string;
  schedule: string;
  human: string;
  lastRun: string;
  duration: string;
  next: string;
  status: "success" | "failed" | "running";
}
export const BRK_CRONS: BrkCron[] = [
  { name: "swap.rollover", schedule: "0 0 * * 1-5", human: "Daily 00:00 server", lastRun: "2026-09-23T21:00:00Z", duration: "48s", next: "2026-09-24T21:00:00Z", status: "success" },
  { name: "billing.invoices.generate", schedule: "0 6 1 * *", human: "1st of month 09:00", lastRun: "2026-09-01T06:00:00Z", duration: "2m 11s", next: "2026-10-01T06:00:00Z", status: "success" },
  { name: "billing.overdue.check", schedule: "0 7 * * *", human: "Daily 10:00", lastRun: "2026-09-24T07:00:00Z", duration: "6s", next: "2026-09-25T07:00:00Z", status: "success" },
  { name: "ib.commission.settle", schedule: "30 0 * * *", human: "Daily 03:30", lastRun: "2026-09-24T00:30:00Z", duration: "4m 02s", next: "2026-09-25T00:30:00Z", status: "success" },
  { name: "statements.daily.email", schedule: "15 21 * * 1-5", human: "Weekdays 00:15", lastRun: "2026-09-23T21:15:00Z", duration: "11m 48s", next: "2026-09-24T21:15:00Z", status: "failed" },
  { name: "symbols.sync.tenants", schedule: "*/30 * * * *", human: "Every 30 min", lastRun: "2026-09-24T08:30:00Z", duration: "14s", next: "2026-09-24T09:00:00Z", status: "running" },
  { name: "backup.postgres.snapshot", schedule: "0 */6 * * *", human: "Every 6 hours", lastRun: "2026-09-24T06:00:00Z", duration: "7m 36s", next: "2026-09-24T12:00:00Z", status: "success" },
];

export interface BrkIncident {
  id: string;
  title: string;
  severity: "minor" | "major" | "critical" | "maintenance";
  status: "investigating" | "identified" | "monitoring" | "resolved" | "scheduled";
  components: string[];
  startedAt: string;
  resolvedAt?: string;
  updates: { at: string; text: string; status: string }[];
  author: Person;
}

export const BRK_INCIDENTS: BrkIncident[] = [
  {
    id: "inc_2091",
    title: "Delayed USDT TRC20 deposit confirmations",
    severity: "minor",
    status: "monitoring",
    components: ["Deposits (USDT TRC20)"],
    startedAt: "2026-09-24T05:42:00Z",
    author: PEOPLE[9]!,
    updates: [
      { at: "2026-09-24T07:10:00Z", status: "Monitoring", text: "Third watcher pod restored. Backlog draining at ~38 tx/s; new deposits credit within 3 minutes." },
      { at: "2026-09-24T06:05:00Z", status: "Identified", text: "TronGrid rate limiting one watcher pod. Failing over to our own full node." },
      { at: "2026-09-24T05:42:00Z", status: "Investigating", text: "Some deposits are taking longer than usual to credit. Funds are safe." },
    ],
  },
  {
    id: "inc_2090",
    title: "Scheduled maintenance — Postgres minor upgrade",
    severity: "maintenance",
    status: "scheduled",
    components: ["Client Area", "Back Office"],
    startedAt: "2026-09-27T23:00:00Z",
    author: PEOPLE[13]!,
    updates: [{ at: "2026-09-22T12:00:00Z", status: "Scheduled", text: "Sunday 28 Sep 02:00–02:30 server time. Trading is unaffected; Client Area read-only for up to 10 minutes." }],
  },
  {
    id: "inc_2084",
    title: "Equities quotes stale on Polygon backup",
    severity: "minor",
    status: "resolved",
    components: ["Price feed"],
    startedAt: "2026-09-19T14:31:00Z",
    resolvedAt: "2026-09-19T15:02:00Z",
    author: PEOPLE[4]!,
    updates: [{ at: "2026-09-19T15:02:00Z", status: "Resolved", text: "Backup feed reconnected. Primary Infoways feed was unaffected throughout." }],
  },
  {
    id: "inc_2077",
    title: "Elevated API latency in eu-central-1",
    severity: "major",
    status: "resolved",
    components: ["Public API", "Client Area"],
    startedAt: "2026-09-13T09:12:00Z",
    resolvedAt: "2026-09-13T09:48:00Z",
    author: PEOPLE[9]!,
    updates: [{ at: "2026-09-13T09:48:00Z", status: "Resolved", text: "Autoscaler misconfiguration fixed; p95 back under 50ms." }],
  },
];

export const BRK_STATUS_COMPONENTS: { name: string; status: BrkSvcStatus; uptime: number; bars: number[] }[] = [
  { name: "Trading & execution", status: "operational", uptime: 100, bars: svcBars(11) },
  { name: "Price feed", status: "operational", uptime: 99.98, bars: svcBars(12, [24]) },
  { name: "Client Area", status: "operational", uptime: 99.97, bars: svcBars(13, [18]) },
  { name: "Deposits (USDT TRC20)", status: "degraded", uptime: 99.71, bars: svcBars(14, [3, 17], [29]) },
  { name: "Withdrawals", status: "operational", uptime: 99.99, bars: svcBars(15) },
  { name: "Public API", status: "operational", uptime: 99.96, bars: svcBars(16, [18]) },
  { name: "Back Office", status: "operational", uptime: 99.99, bars: svcBars(17) },
];

/* ------------------------------------------------------------------ */
/* Feature flags                                                       */
/* ------------------------------------------------------------------ */

export type BrkFlagType = "release" | "ops" | "experiment";
export interface BrkFlag {
  key: string;
  description: string;
  type: BrkFlagType;
  enabled: boolean;
  rollout: number;
  overrides: Record<string, boolean>;
  changedBy: Person;
  changedAt: string;
  createdAt: string;
  stale?: boolean;
}

export const BRK_FLAGS: BrkFlag[] = [
  { key: "terminal.v2_orderbook", description: "New depth-of-market panel in the web terminal", type: "release", enabled: true, rollout: 60, overrides: { tnt_002: true, tnt_005: true, tnt_007: false }, changedBy: PEOPLE[9]!, changedAt: "2026-09-23T16:40:00Z", createdAt: "2026-08-02T10:00:00Z" },
  { key: "wallet.tron_fullnode_fallback", description: "Fail over TRC20 watcher to self-hosted full node when TronGrid throttles", type: "ops", enabled: true, rollout: 100, overrides: {}, changedBy: PEOPLE[4]!, changedAt: "2026-09-24T06:05:00Z", createdAt: "2026-05-14T10:00:00Z" },
  { key: "ai_coach.trade_review", description: "Post-trade AI review cards in the journal", type: "experiment", enabled: true, rollout: 25, overrides: { tnt_002: true, tnt_003: false }, changedBy: PEOPLE[21]!, changedAt: "2026-09-21T11:05:00Z", createdAt: "2026-09-01T10:00:00Z" },
  { key: "onboarding.one_page_kyc", description: "Single-page KYC flow with inline liveness check", type: "experiment", enabled: true, rollout: 50, overrides: { tnt_004: true }, changedBy: PEOPLE[6]!, changedAt: "2026-09-18T09:22:00Z", createdAt: "2026-08-20T10:00:00Z" },
  { key: "ops.withdrawals_kill_switch", description: "Pause all outgoing withdrawals platform-wide (emergency)", type: "ops", enabled: false, rollout: 100, overrides: {}, changedBy: PEOPLE[4]!, changedAt: "2026-07-02T03:14:00Z", createdAt: "2025-11-10T10:00:00Z" },
  { key: "copy.equity_protector", description: "Auto-stop copying when follower drawdown exceeds threshold", type: "release", enabled: true, rollout: 100, overrides: { tnt_010: false }, changedBy: PEOPLE[13]!, changedAt: "2026-09-10T15:30:00Z", createdAt: "2026-06-11T10:00:00Z" },
  { key: "prop.instant_funding", description: "Instant funded accounts without evaluation phase", type: "release", enabled: false, rollout: 0, overrides: { tnt_005: true }, changedBy: PEOPLE[1]!, changedAt: "2026-09-15T12:18:00Z", createdAt: "2026-09-05T10:00:00Z" },
  { key: "pricing.cent_accounts", description: "Cent account type (USC) in the open-account wizard", type: "release", enabled: true, rollout: 100, overrides: { tnt_002: false }, changedBy: PEOPLE[9]!, changedAt: "2026-04-02T10:00:00Z", createdAt: "2026-02-10T10:00:00Z", stale: true },
  { key: "ops.feed_failover_auto", description: "Automatic failover to backup feed when primary stalls > 3s", type: "ops", enabled: true, rollout: 100, overrides: {}, changedBy: PEOPLE[4]!, changedAt: "2026-09-19T14:40:00Z", createdAt: "2025-10-01T10:00:00Z" },
  { key: "growth.referral_leaderboard", description: "Public referral leaderboard on the partner page", type: "experiment", enabled: false, rollout: 10, overrides: {}, changedBy: PEOPLE[16]!, changedAt: "2026-08-30T08:12:00Z", createdAt: "2026-08-12T10:00:00Z" },
  { key: "clientarea.dark_light_auto", description: "Follow OS theme by default in the Client Area", type: "release", enabled: true, rollout: 100, overrides: {}, changedBy: PEOPLE[6]!, changedAt: "2026-03-18T10:00:00Z", createdAt: "2026-01-11T10:00:00Z", stale: true },
];

export interface BrkFlagEvent {
  flag: string;
  action: string;
  by: Person;
  at: string;
  tone: "up" | "down" | "ember" | "gold" | "neutral";
}
export const BRK_FLAG_EVENTS: BrkFlagEvent[] = [
  { flag: "wallet.tron_fullnode_fallback", action: "Enabled globally", by: PEOPLE[4]!, at: "2026-09-24T06:05:00Z", tone: "up" },
  { flag: "terminal.v2_orderbook", action: "Rollout 40% → 60%", by: PEOPLE[9]!, at: "2026-09-23T16:40:00Z", tone: "ember" },
  { flag: "ai_coach.trade_review", action: "Override NovaTrade Asia → off", by: PEOPLE[21]!, at: "2026-09-21T11:05:00Z", tone: "down" },
  { flag: "ops.feed_failover_auto", action: "Threshold 5s → 3s", by: PEOPLE[4]!, at: "2026-09-19T14:40:00Z", tone: "gold" },
  { flag: "onboarding.one_page_kyc", action: "Rollout 25% → 50%", by: PEOPLE[6]!, at: "2026-09-18T09:22:00Z", tone: "ember" },
  { flag: "prop.instant_funding", action: "Override Meridian Prime → on", by: PEOPLE[1]!, at: "2026-09-15T12:18:00Z", tone: "up" },
];

/* ------------------------------------------------------------------ */
/* Platform activity (tenants page)                                    */
/* ------------------------------------------------------------------ */

export const BRK_ACTIVITY: { text: string; tenantId: string; by: Person; at: string; tone: "up" | "down" | "ember" | "gold" | "info" | "warn" }[] = [
  { text: "DNS verified for trade.lotuspacific.vn", tenantId: "tnt_008", by: PEOPLE[13]!, at: "2026-09-24T07:48:00Z", tone: "up" },
  { text: "Invoice INV-2026-09-0145 is 9 days overdue", tenantId: "tnt_007", by: PEOPLE[4]!, at: "2026-09-24T07:00:00Z", tone: "down" },
  { text: "Prop challenges add-on enabled", tenantId: "tnt_003", by: PEOPLE[9]!, at: "2026-09-12T10:22:00Z", tone: "ember" },
  { text: "Trial extended by 14 days", tenantId: "tnt_006", by: PEOPLE[1]!, at: "2026-09-11T13:10:00Z", tone: "gold" },
  { text: "Tenant suspended — non-payment (Jul invoice)", tenantId: "tnt_007", by: PEOPLE[4]!, at: "2026-09-08T09:00:00Z", tone: "warn" },
  { text: "Upgraded Growth → Enterprise", tenantId: "tnt_005", by: PEOPLE[9]!, at: "2026-08-31T11:40:00Z", tone: "info" },
];

export const BRK_REGULATORS = ["FSA Seychelles", "FSC Mauritius", "CySEC", "DFSA", "SCA", "MAS", "FSCA", "VFSC", "ASIC", "FCA", "Unregulated (offshore)"];
export const BRK_COUNTRIES: [string, string][] = [
  ["ae", "United Arab Emirates"],
  ["sg", "Singapore"],
  ["cy", "Cyprus"],
  ["gb", "United Kingdom"],
  ["za", "South Africa"],
  ["sc", "Seychelles"],
  ["mu", "Mauritius"],
  ["vn", "Vietnam"],
  ["my", "Malaysia"],
  ["in", "India"],
  ["br", "Brazil"],
  ["mx", "Mexico"],
  ["ng", "Nigeria"],
];

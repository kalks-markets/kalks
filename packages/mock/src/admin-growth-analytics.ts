/* Back Office · Analytics mock data (prefix ANL_). Deterministic — safe for SSR. */
import { seeded } from "./rng";
import { PEOPLE, person, type Person } from "./people";

const DAY = 86_400_000;
/** "Today" in the admin: 24 Sep 2026 (server GMT+3). */
export const ANL_TODAY = Date.UTC(2026, 8, 24);

const r2 = (v: number) => Math.round(v * 100) / 100;

/* ------------------------------------------------------------------ */
/* Broker P&L                                                          */
/* ------------------------------------------------------------------ */

export interface AnlPnlDay {
  date: string; // YYYY-MM-DD
  time: number; // unix seconds
  spread: number;
  commission: number;
  swap: number;
  bbook: number; // broker's B-book P&L (can be negative)
  ibCost: number; // positive number = cost
  net: number;
  lots: number;
}

export const ANL_PNL_SOURCES = [
  { key: "spread", label: "Spread", color: "var(--k-ember)" },
  { key: "commission", label: "Commission", color: "var(--k-gold)" },
  { key: "swap", label: "Swap", color: "var(--k-info)" },
  { key: "bbook", label: "B-book P&L", color: "var(--k-up)" },
] as const;

export const ANL_PNL_DAILY: AnlPnlDay[] = (() => {
  const r = seeded(9101);
  const N = 540;
  const out: AnlPnlDay[] = [];
  for (let i = N - 1; i >= 0; i--) {
    const t = ANL_TODAY - i * DAY;
    const d = new Date(t);
    const wd = d.getUTCDay();
    const weekend = wd === 0 || wd === 6;
    const growth = 0.68 + 0.34 * ((N - i) / N);
    const f = growth * (weekend ? 0.14 : 1) * (1 + r.normal() * 0.09);
    const spread = 36_400 * f * (1 + r.normal() * 0.08);
    const commission = 13_800 * f * (1 + r.normal() * 0.1);
    const swap = (wd === 3 ? 3 : 1) * 4_900 * f * (1 + r.normal() * 0.12);
    const bbook = weekend ? 1_800 * r.normal() : 19_500 * f + 41_000 * r.normal() * growth;
    const ibCost = (spread + commission) * (0.27 + r.next() * 0.03);
    out.push({
      date: d.toISOString().slice(0, 10),
      time: Math.floor(t / 1000),
      spread: r2(spread),
      commission: r2(commission),
      swap: r2(swap),
      bbook: r2(bbook),
      ibCost: r2(ibCost),
      net: r2(spread + commission + swap + bbook - ibCost),
      lots: Math.round(f * 21_400),
    });
  }
  return out;
})();

export interface AnlSymbolPnl {
  symbol: string;
  lots: number;
  spread: number;
  commission: number;
  swap: number;
  bbook: number;
  net: number;
  bookA: number; // % of volume A-booked
}

/** Base = last 30 days. Pages scale these by range. */
export const ANL_PNL_SYMBOLS: AnlSymbolPnl[] = (
  [
    ["XAUUSD", 118_400, 402_800, 88_200, 46_100, 214_600, 38],
    ["EURUSD", 142_900, 186_300, 102_400, 21_800, -38_200, 71],
    ["NAS100", 61_200, 148_900, 22_600, 18_300, 96_400, 22],
    ["GBPUSD", 74_800, 104_600, 52_900, 13_200, 41_800, 64],
    ["BTCUSD", 22_600, 131_200, 18_700, 29_400, 58_300, 18],
    ["USDJPY", 66_300, 82_100, 44_800, 17_600, -12_400, 67],
    ["US30", 38_100, 76_400, 13_900, 7_800, 33_100, 25],
    ["GBPJPY", 29_700, 61_300, 19_800, 9_900, 27_600, 34],
    ["USOIL", 24_900, 44_800, 11_700, 6_200, -9_800, 41],
    ["XAGUSD", 18_300, 38_600, 9_400, 4_100, 16_900, 30],
    ["ETHUSD", 12_800, 42_700, 7_100, 8_800, 19_400, 15],
    ["GER40", 16_400, 29_800, 6_300, 2_900, 11_200, 28],
  ] as const
).map(([symbol, lots, spread, commission, swap, bbook, bookA]) => ({ symbol, lots, spread, commission, swap, bbook, net: spread + commission + swap + bbook, bookA }));

export interface AnlGroupPnl {
  group: "Standard" | "Pro" | "Cent" | "ECN" | "Prop";
  accounts: number;
  lots: number;
  revenue: number;
  bbook: number;
  ibCost: number;
  bookA: number;
  note: string;
}

export const ANL_PNL_GROUPS: AnlGroupPnl[] = [
  { group: "Standard", accounts: 18_420, lots: 238_600, revenue: 684_300, bbook: 312_800, ibCost: 214_900, bookA: 24, note: "Spread-only · 1:1000" },
  { group: "Pro", accounts: 6_210, lots: 214_900, revenue: 402_100, bbook: 88_400, ibCost: 118_600, bookA: 58, note: "Raw spread + $3.5/lot" },
  { group: "ECN", accounts: 1_380, lots: 162_300, revenue: 218_700, bbook: 0, ibCost: 64_200, bookA: 100, note: "STP · LP routed" },
  { group: "Cent", accounts: 9_860, lots: 41_200, revenue: 96_400, bbook: 71_300, ibCost: 22_800, bookA: 0, note: "USC · 1:2000" },
  { group: "Prop", accounts: 2_940, lots: 69_800, revenue: 188_200, bbook: 126_900, ibCost: 18_100, bookA: 6, note: "Challenge fees incl." },
];

export const ANL_BOOK_SPLIT = {
  aBook: { volumePct: 43.6, revenue: 612_400, lps: ["LMAX Digital", "Finalto", "B2C2"], markup: 0.3 },
  bBook: { volumePct: 56.4, revenue: 1_077_300, clientsHedged: 1_184, internalised: 92.4 },
};

export interface AnlClientPnl {
  person: Person;
  login: string;
  group: AnlGroupPnl["group"];
  lots: number;
  trades: number;
  brokerPnl: number; // + = broker earned
  book: "A" | "B";
  toxicity: number; // 0-100
}

export const ANL_CLIENT_PNL: AnlClientPnl[] = (() => {
  const r = seeded(7702);
  const groups: AnlGroupPnl["group"][] = ["Standard", "Pro", "ECN", "Standard", "Cent", "Prop", "Pro"];
  return PEOPLE.slice(0, 22).map((p, i) => {
    const winner = i % 2 === 0;
    const brokerPnl = winner ? r.range(18_000, 142_000) : -r.range(14_000, 96_000);
    return {
      person: p,
      login: String(80_400_000 + r.int(1_000, 99_999)),
      group: groups[i % groups.length]!,
      lots: r2(r.range(120, 4_800)),
      trades: r.int(140, 6_200),
      brokerPnl: r2(brokerPnl),
      book: (brokerPnl < -85_000 ? "A" : r.bool(0.8) ? "B" : "A") as "A" | "B",
      toxicity: Math.round(brokerPnl < 0 ? r.range(55, 96) : r.range(4, 38)),
    };
  });
})();

/* ------------------------------------------------------------------ */
/* Acquisition funnel                                                  */
/* ------------------------------------------------------------------ */

export const ANL_FUNNEL_STAGES = [
  { key: "visit", label: "Visits", hint: "Unique site sessions" },
  { key: "signup", label: "Sign-ups", hint: "Email + phone verified" },
  { key: "kyc", label: "KYC approved", hint: "POI + POA passed" },
  { key: "ftd", label: "First deposit", hint: "FTD ≥ $50" },
  { key: "active", label: "Active traders", hint: "≥ 5 trades in 30d" },
] as const;

export type AnlFunnelKey = (typeof ANL_FUNNEL_STAGES)[number]["key"];

/** 30-day totals. Other ranges are scaled in the page. */
export const ANL_FUNNEL_30D: Record<AnlFunnelKey, number> = { visit: 412_880, signup: 18_642, kyc: 9_128, ftd: 3_284, active: 2_176 };
export const ANL_FUNNEL_PREV: Record<AnlFunnelKey, number> = { visit: 389_140, signup: 16_910, kyc: 8_402, ftd: 2_958, active: 1_990 };

export interface AnlFunnelRow {
  key: string;
  label: string;
  country?: string;
  visit: number;
  signup: number;
  kyc: number;
  ftd: number;
  active: number;
  spend?: number;
}

const mkFunnel = (key: string, label: string, visit: number, s: number, k: number, f: number, a: number, extra: Partial<AnlFunnelRow> = {}): AnlFunnelRow => {
  const signup = Math.round(visit * s);
  const kyc = Math.round(signup * k);
  const ftd = Math.round(kyc * f);
  return { key, label, visit, signup, kyc, ftd, active: Math.round(ftd * a), ...extra };
};

export const ANL_FUNNEL_COUNTRIES: AnlFunnelRow[] = [
  mkFunnel("in", "India", 96_400, 0.052, 0.46, 0.33, 0.62, { country: "in" }),
  mkFunnel("ng", "Nigeria", 54_800, 0.061, 0.41, 0.3, 0.58, { country: "ng" }),
  mkFunnel("vn", "Vietnam", 41_200, 0.047, 0.55, 0.42, 0.71, { country: "vn" }),
  mkFunnel("ae", "UAE", 22_900, 0.038, 0.68, 0.51, 0.74, { country: "ae" }),
  mkFunnel("my", "Malaysia", 28_300, 0.044, 0.52, 0.38, 0.69, { country: "my" }),
  mkFunnel("br", "Brazil", 36_700, 0.042, 0.44, 0.31, 0.63, { country: "br" }),
  mkFunnel("pk", "Pakistan", 31_600, 0.05, 0.39, 0.28, 0.6, { country: "pk" }),
  mkFunnel("za", "South Africa", 19_800, 0.049, 0.5, 0.37, 0.67, { country: "za" }),
  mkFunnel("eg", "Egypt", 17_400, 0.043, 0.42, 0.29, 0.57, { country: "eg" }),
  mkFunnel("ph", "Philippines", 21_100, 0.045, 0.47, 0.32, 0.64, { country: "ph" }),
  mkFunnel("tr", "Türkiye", 14_600, 0.036, 0.49, 0.35, 0.66, { country: "tr" }),
  mkFunnel("mx", "Mexico", 12_900, 0.034, 0.45, 0.3, 0.61, { country: "mx" }),
];

export const ANL_FUNNEL_SOURCES: AnlFunnelRow[] = [
  mkFunnel("google", "Google Ads", 128_400, 0.041, 0.5, 0.34, 0.63, { spend: 186_400 }),
  mkFunnel("meta", "Meta Ads", 96_200, 0.052, 0.4, 0.27, 0.55, { spend: 142_900 }),
  mkFunnel("ib", "IB referrals", 38_600, 0.118, 0.61, 0.49, 0.78, { spend: 0 }),
  mkFunnel("organic", "Organic / SEO", 84_900, 0.031, 0.53, 0.36, 0.7, { spend: 18_200 }),
  mkFunnel("affiliates", "Affiliates (CPA)", 41_700, 0.058, 0.44, 0.33, 0.52, { spend: 96_300 }),
  mkFunnel("telegram", "Telegram & YouTube", 23_100, 0.049, 0.47, 0.31, 0.6, { spend: 22_600 }),
];

export const ANL_TIME_TO_FTD = [
  { bucket: "< 1h", count: 412 },
  { bucket: "1–6h", count: 638 },
  { bucket: "6–24h", count: 721 },
  { bucket: "1–3d", count: 584 },
  { bucket: "3–7d", count: 402 },
  { bucket: "7–14d", count: 268 },
  { bucket: "14–30d", count: 171 },
  { bucket: "30d+", count: 88 },
];

/** Daily sign-ups vs FTDs, last 30 days. */
export const ANL_FUNNEL_DAILY = (() => {
  const r = seeded(4410);
  return Array.from({ length: 30 }, (_, i) => {
    const t = ANL_TODAY - (29 - i) * DAY;
    const wd = new Date(t).getUTCDay();
    const f = (wd === 0 || wd === 6 ? 0.72 : 1) * (0.9 + i * 0.006);
    const signup = Math.round(640 * f * (1 + r.normal() * 0.1));
    return { date: new Date(t).toISOString().slice(0, 10), signup, ftd: Math.round(signup * (0.165 + r.normal() * 0.015)) };
  });
})();

/* ------------------------------------------------------------------ */
/* Cohorts & LTV                                                       */
/* ------------------------------------------------------------------ */

export interface AnlCohort {
  month: string; // "Oct 2025"
  key: string; // 2025-10
  ftds: number;
  cac: number;
  retention: number[]; // % active, index = months since FTD (0..)
  ltv: number[]; // cumulative net revenue per FTD ($)
}

export const ANL_COHORTS: AnlCohort[] = (() => {
  const r = seeded(3303);
  const names = ["Oct 2025", "Nov 2025", "Dec 2025", "Jan 2026", "Feb 2026", "Mar 2026", "Apr 2026", "May 2026", "Jun 2026", "Jul 2026", "Aug 2026", "Sep 2026"];
  const keys = ["2025-10", "2025-11", "2025-12", "2026-01", "2026-02", "2026-03", "2026-04", "2026-05", "2026-06", "2026-07", "2026-08", "2026-09"];
  return names.map((month, c) => {
    const len = 12 - c;
    const quality = 1 + (c - 5) * 0.012 + r.normal() * 0.03; // later cohorts slightly better
    const retention: number[] = [100];
    for (let m = 1; m < len; m++) {
      const base = 62 * Math.pow(m, -0.42) * quality;
      retention.push(Math.max(8, Math.min(99, Math.round((base + r.normal() * 2) * 10) / 10)));
    }
    const ltv: number[] = [];
    let acc = 0;
    for (let m = 0; m < len; m++) {
      acc += (m === 0 ? 95 : 190) * (retention[m]! / 100) * quality * (1 + r.normal() * 0.05);
      ltv.push(Math.round(acc));
    }
    return { month, key: keys[c]!, ftds: Math.round((2_380 + c * 92) * (1 + r.normal() * 0.06)), cac: Math.round(312 + r.normal() * 16 - c * 2.5), retention, ltv };
  });
})();

/* ------------------------------------------------------------------ */
/* Deposits                                                            */
/* ------------------------------------------------------------------ */

export const ANL_DEP_DAILY = (() => {
  const r = seeded(5511);
  return Array.from({ length: 90 }, (_, i) => {
    const t = ANL_TODAY - (89 - i) * DAY;
    const wd = new Date(t).getUTCDay();
    const f = (wd === 0 || wd === 6 ? 0.55 : 1) * (0.86 + i * 0.0022);
    const deposits = 684_000 * f * (1 + r.normal() * 0.14);
    const withdrawals = 412_000 * f * (1 + r.normal() * 0.18);
    return { date: new Date(t).toISOString().slice(0, 10), deposits: r2(deposits), withdrawals: r2(withdrawals), ftds: Math.round(112 * f * (1 + r.normal() * 0.12)) };
  });
})();

export interface AnlDepCountry {
  country: string;
  name: string;
  ftds: number;
  deposits: number;
  withdrawals: number;
  avgFtd: number;
}

export const ANL_DEP_COUNTRIES: AnlDepCountry[] = [
  { country: "in", name: "India", ftds: 612, deposits: 3_184_200, withdrawals: 1_942_100, avgFtd: 318 },
  { country: "ae", name: "UAE", ftds: 284, deposits: 2_962_800, withdrawals: 1_604_300, avgFtd: 1_240 },
  { country: "vn", name: "Vietnam", ftds: 402, deposits: 2_118_900, withdrawals: 1_288_400, avgFtd: 412 },
  { country: "ng", name: "Nigeria", ftds: 438, deposits: 1_486_300, withdrawals: 922_700, avgFtd: 204 },
  { country: "my", name: "Malaysia", ftds: 246, deposits: 1_392_600, withdrawals: 804_200, avgFtd: 486 },
  { country: "br", name: "Brazil", ftds: 218, deposits: 1_064_800, withdrawals: 698_500, avgFtd: 362 },
  { country: "za", name: "South Africa", ftds: 186, deposits: 948_300, withdrawals: 571_900, avgFtd: 394 },
  { country: "sa", name: "Saudi Arabia", ftds: 94, deposits: 1_208_400, withdrawals: 612_800, avgFtd: 1_580 },
  { country: "pk", name: "Pakistan", ftds: 172, deposits: 486_200, withdrawals: 301_400, avgFtd: 186 },
  { country: "ph", name: "Philippines", ftds: 158, deposits: 512_700, withdrawals: 322_600, avgFtd: 228 },
  { country: "eg", name: "Egypt", ftds: 118, deposits: 402_900, withdrawals: 244_100, avgFtd: 242 },
  { country: "tr", name: "Türkiye", ftds: 104, deposits: 618_300, withdrawals: 402_800, avgFtd: 512 },
  { country: "gb", name: "United Kingdom", ftds: 62, deposits: 584_100, withdrawals: 361_900, avgFtd: 1_120 },
];

export interface AnlDepIb {
  person: Person;
  code: string;
  ftds: number;
  deposits: number;
  net: number;
  trend: number[];
}

export const ANL_DEP_IBS: AnlDepIb[] = (() => {
  const r = seeded(8120);
  const idx = [5, 1, 7, 13, 3, 10, 21, 15];
  return idx.map((pi, i) => {
    const ftds = Math.round(248 / (1 + i * 0.38) + r.range(-6, 6));
    const deposits = ftds * r.range(620, 1_480);
    return {
      person: person(pi),
      code: `IB-${(21_400 + pi * 137).toString()}`,
      ftds,
      deposits: r2(deposits),
      net: r2(deposits * r.range(0.32, 0.52)),
      trend: Array.from({ length: 14 }, (_, k) => Math.round(ftds / 14 * (0.6 + k * 0.05 + r.next() * 0.5))),
    };
  });
})();

export interface AnlCampaign {
  id: string;
  name: string;
  channel: "Google" | "Meta" | "Affiliate" | "Telegram" | "Email" | "IB";
  ftds: number;
  deposits: number;
  spend: number;
  status: "running" | "paused" | "completed";
}

export const ANL_DEP_CAMPAIGNS: AnlCampaign[] = [
  { id: "CMP-2291", name: "Gold rush · XAUUSD 0-spread week", channel: "Google", ftds: 486, deposits: 1_284_600, spend: 92_400, status: "running" },
  { id: "CMP-2287", name: "50% deposit bonus · SEA", channel: "Meta", ftds: 412, deposits: 862_300, spend: 71_800, status: "running" },
  { id: "CMP-2280", name: "Prop challenge launch · $10k", channel: "Meta", ftds: 318, deposits: 402_900, spend: 48_200, status: "completed" },
  { id: "CMP-2276", name: "IB Diwali contest", channel: "IB", ftds: 294, deposits: 948_100, spend: 36_500, status: "running" },
  { id: "CMP-2271", name: "Affiliate CPA · LATAM", channel: "Affiliate", ftds: 226, deposits: 518_700, spend: 67_800, status: "running" },
  { id: "CMP-2266", name: "Copy trading masters week", channel: "Telegram", ftds: 164, deposits: 322_400, spend: 14_900, status: "paused" },
  { id: "CMP-2259", name: "Win-back · dormant FTDs", channel: "Email", ftds: 138, deposits: 296_800, spend: 3_200, status: "completed" },
  { id: "CMP-2252", name: "NAS100 earnings season", channel: "Google", ftds: 121, deposits: 384_600, spend: 41_300, status: "completed" },
];

export const ANL_DEP_METHODS = [
  { label: "USDT · TRC20", value: 58.4, coin: "usdt", amount: 10_512_000 },
  { label: "Bank wire", value: 13.2, coin: "", amount: 2_376_000 },
  { label: "Cards (Visa/MC)", value: 9.6, coin: "", amount: 1_728_000 },
  { label: "Local (UPI/PIX)", value: 8.9, coin: "", amount: 1_602_000 },
  { label: "USDT · ERC20", value: 5.1, coin: "eth", amount: 918_000 },
  { label: "BTC", value: 3.2, coin: "btc", amount: 576_000 },
  { label: "Skrill / Neteller", value: 1.6, coin: "", amount: 288_000 },
];

/* ------------------------------------------------------------------ */
/* Partners                                                            */
/* ------------------------------------------------------------------ */

export type AnlTier = "Elite" | "Platinum" | "Gold" | "Silver" | "Starter";

export interface AnlPartner {
  person: Person;
  code: string;
  tier: AnlTier;
  clients: number;
  ftds: number;
  lots: number;
  commission: number;
  revenue: number; // net revenue to broker (after commission)
  roi: number;
  trend: number[];
  subIbs: number;
}

export const ANL_PARTNERS: AnlPartner[] = (() => {
  const r = seeded(6620);
  const idx = [5, 1, 7, 13, 3, 10, 21, 15, 2, 17, 11, 19, 8, 23, 16, 0];
  const tiers: AnlTier[] = ["Elite", "Elite", "Platinum", "Platinum", "Platinum", "Gold", "Gold", "Gold", "Gold", "Silver", "Silver", "Silver", "Silver", "Starter", "Starter", "Starter"];
  return idx.map((pi, i) => {
    const lots = Math.round(38_000 / Math.pow(1 + i, 0.72) * r.range(0.85, 1.15));
    const rate = { Elite: 9, Platinum: 8, Gold: 7, Silver: 6, Starter: 5 }[tiers[i]!];
    const commission = lots * rate * r.range(0.9, 1.05);
    const gross = commission * r.range(1.55, 3.1);
    const revenue = gross - commission;
    const clients = Math.round(lots / r.range(28, 46));
    return {
      person: person(pi),
      code: `IB-${21_400 + pi * 137}`,
      tier: tiers[i]!,
      clients,
      ftds: Math.round(clients * r.range(0.18, 0.32)),
      lots,
      commission: r2(commission),
      revenue: r2(revenue),
      roi: Math.round((revenue / commission) * 100),
      trend: Array.from({ length: 12 }, (_, k) => Math.round(lots / 12 * (0.7 + k * 0.04 + r.next() * 0.3))),
      subIbs: tiers[i] === "Elite" ? r.int(14, 38) : tiers[i] === "Platinum" ? r.int(6, 16) : r.int(0, 6),
    };
  });
})();

export const ANL_IB_MONTHLY = (() => {
  const r = seeded(6630);
  const months = ["Oct", "Nov", "Dec", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep"];
  return months.map((m, i) => {
    const f = 0.72 + i * 0.03 + r.normal() * 0.04;
    const commission = 312_000 * f;
    return { month: m, commission: r2(commission), revenue: r2(commission * (1.9 + r.normal() * 0.12)), cpa: r2(38_000 * f * (1 + r.normal() * 0.1)) };
  });
})();

export const ANL_IB_TIERS: { tier: AnlTier; partners: number; share: number; rate: string; color: string }[] = [
  { tier: "Elite", partners: 14, share: 34.8, rate: "$9 / lot + 20% RS", color: "var(--k-ember)" },
  { tier: "Platinum", partners: 42, share: 27.1, rate: "$8 / lot", color: "var(--k-gold)" },
  { tier: "Gold", partners: 128, share: 21.4, rate: "$7 / lot", color: "var(--k-up)" },
  { tier: "Silver", partners: 316, share: 11.2, rate: "$6 / lot", color: "var(--k-info)" },
  { tier: "Starter", partners: 1_184, share: 5.5, rate: "$5 / lot", color: "var(--k-fg-3)" },
];

/* ------------------------------------------------------------------ */
/* Regulatory exports                                                  */
/* ------------------------------------------------------------------ */

export type AnlFormat = "CSV" | "XML" | "XLSX";

export interface AnlRegReport {
  id: string;
  name: string;
  description: string;
  regulator: string;
  icon: string; // Icon3D
  formats: AnlFormat[];
  period: "daily" | "monthly" | "quarterly" | "on demand";
  lastGenerated: string;
  rows: number;
  due?: string;
  critical?: boolean;
}

export const ANL_REG_REPORTS: AnlRegReport[] = [
  { id: "trx", name: "Transaction report", description: "All executed trades with timestamps, prices, LP route and book flag (EMIR/MiFIR-style fields).", regulator: "FSA Seychelles", icon: "receipt", formats: ["XML", "CSV"], period: "daily", lastGenerated: "2026-09-24T06:00:00Z", rows: 184_216, due: "2026-09-25T06:00:00Z" },
  { id: "clients", name: "Client list", description: "Active client register — identity, country, risk class, KYC level, account groups.", regulator: "FSC Mauritius", icon: "busts_in_silhouette", formats: ["CSV", "XLSX"], period: "monthly", lastGenerated: "2026-09-01T07:12:00Z", rows: 38_810, due: "2026-10-01T09:00:00Z" },
  { id: "aml", name: "AML flags & STRs", description: "Suspicious transaction reports, open AML cases, sanctions hits and decisions.", regulator: "FIU", icon: "shield", formats: ["XML", "XLSX"], period: "on demand", lastGenerated: "2026-09-19T14:40:00Z", rows: 27, critical: true },
  { id: "large", name: "Large transactions", description: "Deposits & withdrawals ≥ $10,000 equivalent, with source-of-funds status.", regulator: "FIU", icon: "money_bag", formats: ["CSV", "XLSX"], period: "daily", lastGenerated: "2026-09-24T06:05:00Z", rows: 214 },
  { id: "positions", name: "Positions snapshot", description: "End-of-day open positions, exposure per symbol and net B-book risk.", regulator: "FSA Seychelles", icon: "bar_chart", formats: ["CSV", "XML", "XLSX"], period: "daily", lastGenerated: "2026-09-23T21:00:00Z", rows: 12_904 },
  { id: "complaints", name: "Complaints register", description: "Client complaints, category, handling time, resolution and ombudsman escalations.", regulator: "FSC Mauritius", icon: "speech_balloon", formats: ["XLSX", "CSV"], period: "quarterly", lastGenerated: "2026-07-01T08:30:00Z", rows: 142, due: "2026-10-15T09:00:00Z" },
];

export interface AnlRegHistory {
  id: string;
  report: string;
  period: string;
  format: AnlFormat;
  by: Person;
  at: string;
  size: string;
  rows: number;
  hash: string;
  status: "completed" | "failed" | "processing";
}

const HEX = "0123456789abcdef";
const hash = (seed: number) => {
  const r = seeded(seed);
  return Array.from({ length: 64 }, () => HEX[r.int(0, 15)]).join("");
};

export function anlHash(seed: number) {
  return hash(seed);
}

const REG_BASE: Record<string, [number, number]> = {
  "Transaction report": [184_000, 236],
  "Large transactions": [214, 410],
  "Positions snapshot": [12_900, 180],
  "AML flags & STRs": [27, 5_200],
  "Client list": [38_810, 320],
  "Complaints register": [142, 1_100],
};
const fmtSize = (bytes: number) => (bytes > 1_048_576 ? `${(bytes / 1_048_576).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1024))} KB`);

export const ANL_REG_HISTORY: AnlRegHistory[] = (() => {
  const r = seeded(2020);
  const items: [string, string, AnlFormat, string][] = [
    ["Transaction report", "23 Sep 2026", "XML", "2026-09-24T06:00:00Z"],
    ["Large transactions", "23 Sep 2026", "CSV", "2026-09-24T06:05:00Z"],
    ["Positions snapshot", "23 Sep 2026", "CSV", "2026-09-23T21:00:00Z"],
    ["Transaction report", "22 Sep 2026", "XML", "2026-09-23T06:00:00Z"],
    ["AML flags & STRs", "Q3 2026 (to date)", "XML", "2026-09-19T14:40:00Z"],
    ["Large transactions", "22 Sep 2026", "CSV", "2026-09-23T06:05:00Z"],
    ["Positions snapshot", "22 Sep 2026", "XLSX", "2026-09-22T21:00:00Z"],
    ["Transaction report", "21 Sep 2026", "XML", "2026-09-22T06:00:00Z"],
    ["Client list", "Aug 2026", "XLSX", "2026-09-01T07:12:00Z"],
    ["Transaction report", "20 Sep 2026", "XML", "2026-09-21T06:00:00Z"],
    ["Complaints register", "Q2 2026", "XLSX", "2026-07-01T08:30:00Z"],
    ["Client list", "Jul 2026", "CSV", "2026-08-01T07:05:00Z"],
  ];
  const staff = [PEOPLE[4]!, PEOPLE[12]!, PEOPLE[9]!, PEOPLE[20]!];
  return items.map(([report, period, format, at], i) => ({
    id: `EXP-${48_210 - i * 7}`,
    report,
    period,
    format,
    by: i % 3 === 0 ? PEOPLE[4]! : staff[r.int(0, staff.length - 1)]!,
    at,
    rows: Math.round((REG_BASE[report]?.[0] ?? 1000) * r.range(0.86, 1.04)),
    size: fmtSize((REG_BASE[report]?.[0] ?? 1000) * (REG_BASE[report]?.[1] ?? 200) * r.range(0.86, 1.04)),
    hash: hash(900 + i),
    status: i === 6 ? "failed" : "completed",
  }));
})();

/* ------------------------------------------------------------------ */
/* Scheduled reports                                                   */
/* ------------------------------------------------------------------ */

export type AnlFreq = "daily" | "weekly" | "monthly";

export interface AnlSchedule {
  id: string;
  name: string;
  report: string;
  frequency: AnlFreq;
  day?: string; // "Mon" or "1st"
  time: string; // HH:mm GMT+3
  nextRun: string;
  recipients: string[];
  format: AnlFormat | "PDF";
  enabled: boolean;
  lastStatus: "delivered" | "failed" | "never";
  owner: Person;
}

export const ANL_REPORT_TYPES = [
  "Broker P&L summary",
  "Revenue by symbol",
  "Acquisition funnel",
  "Cohort retention",
  "Deposits & withdrawals",
  "IB leaderboard",
  "Transaction report",
  "Large transactions",
  "Positions snapshot",
  "AML flags & STRs",
];

/** Staff mailbox for a person, e.g. priya.nair@kalkstrade.com */
export function anlStaffEmail(p: Person) {
  return `${p.name.toLowerCase().normalize("NFD").replace(/[\u0300-\u036f]/g, "").replace(/[^a-z]+/g, ".").replace(/^\.|\.$/g, "")}@kalkstrade.com`;
}

export const ANL_SCHEDULES: AnlSchedule[] = [
  { id: "SCH-104", name: "Daily P&L flash", report: "Broker P&L summary", frequency: "daily", time: "08:00", nextRun: "2026-09-25T05:00:00Z", recipients: [anlStaffEmail(PEOPLE[4]!), anlStaffEmail(PEOPLE[9]!), "ceo@kalkstrade.com", "finance@kalkstrade.com"], format: "PDF", enabled: true, lastStatus: "delivered", owner: PEOPLE[4]! },
  { id: "SCH-108", name: "Regulator · transaction feed", report: "Transaction report", frequency: "daily", time: "09:00", nextRun: "2026-09-25T06:00:00Z", recipients: ["reporting@kalkstrade.com", "compliance@kalkstrade.com"], format: "XML", enabled: true, lastStatus: "delivered", owner: PEOPLE[12]! },
  { id: "SCH-111", name: "Weekly growth review", report: "Acquisition funnel", frequency: "weekly", day: "Mon", time: "10:00", nextRun: "2026-09-28T07:00:00Z", recipients: [anlStaffEmail(PEOPLE[1]!), anlStaffEmail(PEOPLE[8]!), "growth@kalkstrade.com"], format: "PDF", enabled: true, lastStatus: "delivered", owner: PEOPLE[1]! },
  { id: "SCH-115", name: "IB payouts leaderboard", report: "IB leaderboard", frequency: "weekly", day: "Fri", time: "17:30", nextRun: "2026-09-25T14:30:00Z", recipients: [anlStaffEmail(PEOPLE[5]!), "partners@kalkstrade.com"], format: "XLSX", enabled: true, lastStatus: "failed", owner: PEOPLE[5]! },
  { id: "SCH-119", name: "Treasury cash-flow", report: "Deposits & withdrawals", frequency: "daily", time: "23:55", nextRun: "2026-09-24T20:55:00Z", recipients: [anlStaffEmail(PEOPLE[9]!), "treasury@kalkstrade.com", "cfo@kalkstrade.com"], format: "XLSX", enabled: true, lastStatus: "delivered", owner: PEOPLE[9]! },
  { id: "SCH-122", name: "Board pack · monthly", report: "Cohort retention", frequency: "monthly", day: "1st", time: "07:00", nextRun: "2026-10-01T04:00:00Z", recipients: ["board@kalkstrade.com", "ceo@kalkstrade.com", anlStaffEmail(PEOPLE[4]!), anlStaffEmail(PEOPLE[20]!), anlStaffEmail(PEOPLE[13]!)], format: "PDF", enabled: true, lastStatus: "delivered", owner: PEOPLE[4]! },
  { id: "SCH-127", name: "Large transactions → FIU", report: "Large transactions", frequency: "daily", time: "06:30", nextRun: "2026-09-25T03:30:00Z", recipients: ["mlro@kalkstrade.com"], format: "CSV", enabled: true, lastStatus: "delivered", owner: PEOPLE[12]! },
  { id: "SCH-131", name: "Symbol revenue deep-dive", report: "Revenue by symbol", frequency: "weekly", day: "Wed", time: "12:00", nextRun: "2026-09-30T09:00:00Z", recipients: [anlStaffEmail(PEOPLE[15]!), "dealing@kalkstrade.com"], format: "XLSX", enabled: false, lastStatus: "never", owner: PEOPLE[15]! },
  { id: "SCH-134", name: "EOD positions snapshot", report: "Positions snapshot", frequency: "daily", time: "00:00", nextRun: "2026-09-24T21:00:00Z", recipients: ["risk@kalkstrade.com", anlStaffEmail(PEOPLE[4]!)], format: "CSV", enabled: true, lastStatus: "delivered", owner: PEOPLE[4]! },
];

export interface AnlDelivery {
  id: string;
  schedule: string;
  at: string;
  recipients: number;
  status: "delivered" | "failed" | "bounced" | "running";
  size: string;
  channel: "Email" | "SFTP" | "Slack";
  note?: string;
}

export const ANL_DELIVERY_LOG: AnlDelivery[] = [
  { id: "DLV-99812", schedule: "Daily P&L flash", at: "2026-09-24T05:00:12Z", recipients: 4, status: "delivered", size: "1.8 MB", channel: "Email" },
  { id: "DLV-99811", schedule: "Large transactions → FIU", at: "2026-09-24T03:30:08Z", recipients: 1, status: "delivered", size: "214 KB", channel: "SFTP" },
  { id: "DLV-99809", schedule: "Regulator · transaction feed", at: "2026-09-24T06:00:41Z", recipients: 2, status: "delivered", size: "42.6 MB", channel: "SFTP" },
  { id: "DLV-99806", schedule: "EOD positions snapshot", at: "2026-09-23T21:00:05Z", recipients: 2, status: "delivered", size: "3.4 MB", channel: "Email" },
  { id: "DLV-99804", schedule: "Treasury cash-flow", at: "2026-09-23T20:55:18Z", recipients: 3, status: "bounced", size: "2.1 MB", channel: "Email", note: "cfo@kalkstrade.com mailbox full" },
  { id: "DLV-99801", schedule: "IB payouts leaderboard", at: "2026-09-19T14:30:02Z", recipients: 2, status: "failed", size: "—", channel: "Slack", note: "Slack webhook 410 Gone" },
  { id: "DLV-99797", schedule: "Daily P&L flash", at: "2026-09-23T05:00:09Z", recipients: 4, status: "delivered", size: "1.7 MB", channel: "Email" },
  { id: "DLV-99794", schedule: "Weekly growth review", at: "2026-09-21T07:00:22Z", recipients: 3, status: "delivered", size: "5.2 MB", channel: "Email" },
  { id: "DLV-99790", schedule: "Regulator · transaction feed", at: "2026-09-23T06:00:37Z", recipients: 2, status: "delivered", size: "41.9 MB", channel: "SFTP" },
  { id: "DLV-99786", schedule: "Board pack · monthly", at: "2026-09-01T04:00:30Z", recipients: 5, status: "delivered", size: "8.9 MB", channel: "Email" },
];

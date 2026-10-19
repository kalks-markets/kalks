/**
 * Back Office · Marketing mock data (bonuses, banners, contests, rewards,
 * automation journeys, promo codes, UTM campaigns).
 * Every export is prefixed MKT_ (types Mkt…) so the admin-growth barrel never collides.
 * Deterministic: seeded() only, fixed ISO dates (today = 2026-09-24, server GMT+3).
 */
import { seeded } from "./rng";
import { PEOPLE, person, type Person } from "./people";

const login = (r: ReturnType<typeof seeded>) => String(r.int(80100000, 80999999));

/* ------------------------------------------------------------------ */
/* Shared targeting                                                     */
/* ------------------------------------------------------------------ */

export const MKT_GROUPS = ["Standard", "Pro", "Cent", "ECN", "Islamic", "VIP", "Crypto", "Zero"] as const;
export type MktGroup = (typeof MKT_GROUPS)[number];

export const MKT_COUNTRIES: { code: string; name: string }[] = [
  { code: "in", name: "India" },
  { code: "ae", name: "UAE" },
  { code: "sa", name: "Saudi Arabia" },
  { code: "br", name: "Brazil" },
  { code: "mx", name: "Mexico" },
  { code: "vn", name: "Vietnam" },
  { code: "my", name: "Malaysia" },
  { code: "th", name: "Thailand" },
  { code: "id", name: "Indonesia" },
  { code: "ph", name: "Philippines" },
  { code: "ng", name: "Nigeria" },
  { code: "za", name: "South Africa" },
  { code: "eg", name: "Egypt" },
  { code: "tr", name: "Türkiye" },
  { code: "pk", name: "Pakistan" },
  { code: "ke", name: "Kenya" },
];

export const MKT_SEGMENTS = [
  "All clients",
  "New sign-ups (7d)",
  "Funded, no trade",
  "KYC pending",
  "VIP",
  "Dormant 30d",
  "IB-referred",
  "Crypto traders",
  "Demo only",
  "GCC",
  "LATAM",
  "South-East Asia",
] as const;
export type MktSegment = (typeof MKT_SEGMENTS)[number];

/* ------------------------------------------------------------------ */
/* Bonuses                                                              */
/* ------------------------------------------------------------------ */

export type MktBonusKind = "deposit" | "no-deposit" | "reload" | "crypto";
export type MktBonusStatus = "active" | "paused" | "scheduled" | "expired" | "draft";

export interface MktBonusCampaign {
  id: string;
  name: string;
  tagline: string;
  kind: MktBonusKind;
  pct: number; // % of deposit credited (0 for fixed)
  fixed: number; // fixed credit for no-deposit
  max: number; // cap per client
  minDeposit: number;
  releasePerLot: number; // $ released to balance per 1 lot traded
  expiryDays: number;
  groups: MktGroup[];
  countries: string[];
  schedule: string;
  claimed: number;
  creditIssued: number;
  released: number;
  conversionPct: number; // claimers who became active traders
  status: MktBonusStatus;
  enabled: boolean;
  illustration: string;
  start: string;
  end?: string;
  trend: number[];
}

export const MKT_BONUS_CAMPAIGNS: MktBonusCampaign[] = [
  {
    id: "bn_welcome30",
    name: "30% welcome credit",
    tagline: "First deposit bonus for new live accounts",
    kind: "deposit",
    pct: 30,
    fixed: 0,
    max: 3000,
    minDeposit: 100,
    releasePerLot: 2,
    expiryDays: 90,
    groups: ["Standard", "Pro", "Cent"],
    countries: ["in", "ae", "vn", "my", "id", "ng", "za", "br"],
    schedule: "Always on",
    claimed: 4812,
    creditIssued: 1284360,
    released: 468220,
    conversionPct: 62.4,
    status: "active",
    enabled: true,
    illustration: "wrapped_gift",
    start: "2026-01-05T00:00:00+03:00",
    trend: [22, 28, 25, 31, 36, 34, 41, 38, 44, 49, 46, 52],
  },
  {
    id: "bn_crypto100",
    name: "100% crypto deposit boost",
    tagline: "USDT TRC20 deposits double into credit",
    kind: "crypto",
    pct: 100,
    fixed: 0,
    max: 5000,
    minDeposit: 250,
    releasePerLot: 4,
    expiryDays: 60,
    groups: ["Crypto", "Pro", "ECN"],
    countries: ["vn", "th", "ph", "tr", "ng", "br", "mx"],
    schedule: "Until 31 Oct 2026",
    claimed: 1396,
    creditIssued: 1862440,
    released: 391110,
    conversionPct: 71.8,
    status: "active",
    enabled: true,
    illustration: "coin",
    start: "2026-08-15T00:00:00+03:00",
    end: "2026-10-31T23:59:00+03:00",
    trend: [8, 14, 19, 22, 30, 33, 29, 38, 42, 47, 55, 61],
  },
  {
    id: "bn_nodep30",
    name: "No-deposit $30 demo-to-live",
    tagline: "Graduating demo traders get $30 live credit",
    kind: "no-deposit",
    pct: 0,
    fixed: 30,
    max: 30,
    minDeposit: 0,
    releasePerLot: 1,
    expiryDays: 30,
    groups: ["Standard", "Cent"],
    countries: ["in", "pk", "eg", "ke", "ng", "id", "ph"],
    schedule: "Always on · KYC required",
    claimed: 9240,
    creditIssued: 277200,
    released: 49896,
    conversionPct: 18.6,
    status: "active",
    enabled: true,
    illustration: "rocket",
    start: "2026-03-01T00:00:00+03:00",
    trend: [40, 42, 39, 44, 47, 45, 43, 48, 50, 46, 49, 51],
  },
  {
    id: "bn_reload20",
    name: "Reload 20% Fridays",
    tagline: "Every Friday 00:00–23:59 server time",
    kind: "reload",
    pct: 20,
    fixed: 0,
    max: 1000,
    minDeposit: 200,
    releasePerLot: 2,
    expiryDays: 45,
    groups: ["Standard", "Pro", "ECN", "VIP"],
    countries: ["ae", "sa", "in", "my", "za", "tr"],
    schedule: "Fridays only",
    claimed: 2218,
    creditIssued: 386540,
    released: 201000,
    conversionPct: 84.2,
    status: "paused",
    enabled: false,
    illustration: "calendar",
    start: "2026-05-01T00:00:00+03:00",
    trend: [18, 20, 24, 19, 26, 27, 22, 29, 24, 21, 17, 12],
  },
  {
    id: "bn_islamic15",
    name: "Ramadan 15% Islamic credit",
    tagline: "Swap-free accounts · GCC and South-East Asia",
    kind: "deposit",
    pct: 15,
    fixed: 0,
    max: 2000,
    minDeposit: 300,
    releasePerLot: 3,
    expiryDays: 30,
    groups: ["Islamic"],
    countries: ["ae", "sa", "my", "id", "eg", "pk"],
    schedule: "18 Feb – 20 Mar 2027",
    claimed: 0,
    creditIssued: 0,
    released: 0,
    conversionPct: 0,
    status: "scheduled",
    enabled: true,
    illustration: "crown",
    start: "2027-02-18T00:00:00+03:00",
    end: "2027-03-20T23:59:00+03:00",
    trend: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
  },
  {
    id: "bn_vip50",
    name: "VIP 50% top-up",
    tagline: "Invite-only for $25k+ net-deposit clients",
    kind: "deposit",
    pct: 50,
    fixed: 0,
    max: 25000,
    minDeposit: 10000,
    releasePerLot: 5,
    expiryDays: 120,
    groups: ["VIP", "ECN"],
    countries: ["ae", "sa", "in", "za"],
    schedule: "Manual approval",
    claimed: 64,
    creditIssued: 842000,
    released: 510300,
    conversionPct: 96.9,
    status: "active",
    enabled: true,
    illustration: "gem_stone",
    start: "2026-06-10T00:00:00+03:00",
    trend: [2, 3, 5, 4, 6, 5, 7, 6, 8, 7, 9, 8],
  },
];

export const MKT_BONUS_KPIS = {
  activeCampaigns: MKT_BONUS_CAMPAIGNS.filter((c) => c.status === "active").length,
  creditIssued: MKT_BONUS_CAMPAIGNS.reduce((s, c) => s + c.creditIssued, 0),
  released: MKT_BONUS_CAMPAIGNS.reduce((s, c) => s + c.released, 0),
  conversionPct: 58.3,
  creditOutstanding: 2_981_340,
  forfeited30d: 184_220,
};

/** Daily credit issued vs released, last 30 days (USD). */
export const MKT_BONUS_FLOW = (() => {
  const r = seeded(8812);
  return Array.from({ length: 30 }, (_, i) => {
    const d = new Date(Date.UTC(2026, 7, 26 + i));
    const issued = 38000 + r.range(-9000, 16000) + i * 520;
    return {
      label: `${String(d.getUTCDate()).padStart(2, "0")} ${["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][d.getUTCMonth()]}`,
      issued: Math.round(issued),
      released: Math.round(issued * r.range(0.24, 0.42)),
    };
  });
})();

export interface MktBonusGrant {
  id: string;
  person: Person;
  login: string;
  campaignId: string;
  deposit: number;
  credit: number;
  lotsTraded: number;
  lotsRequired: number;
  releasedPct: number;
  grantedAt: string;
  expiresAt: string;
  status: "active" | "completed" | "expired" | "forfeited";
}

export const MKT_BONUS_GRANTS: MktBonusGrant[] = (() => {
  const r = seeded(4411);
  const out: MktBonusGrant[] = [];
  const active = MKT_BONUS_CAMPAIGNS.filter((c) => c.claimed > 0);
  for (let i = 0; i < 28; i++) {
    const c = active[i % active.length]!;
    const p = person(i * 5 + 3);
    const deposit = c.kind === "no-deposit" ? 0 : Math.round(Math.max(c.minDeposit, r.range(c.minDeposit, c.minDeposit * 12)) / 10) * 10;
    const credit = c.kind === "no-deposit" ? c.fixed : Math.min(c.max, Math.round((deposit * c.pct) / 100));
    const lotsRequired = Math.round((credit / c.releasePerLot) * 10) / 10;
    const pct = i % 9 === 4 ? 100 : i % 11 === 7 ? r.range(4, 20) : r.range(6, 96);
    const hour = 23 - (i % 20);
    const day = 24 - Math.floor(i / 3);
    const granted = `2026-09-${String(day).padStart(2, "0")}T${String(hour).padStart(2, "0")}:${String((i * 17) % 60).padStart(2, "0")}:00+03:00`;
    const exp = new Date(Date.UTC(2026, 8, day + c.expiryDays));
    out.push({
      id: `GR-${(51820 - i).toString()}`,
      person: p,
      login: login(r),
      campaignId: c.id,
      deposit,
      credit,
      lotsRequired,
      lotsTraded: Math.round(lotsRequired * pct) / 100,
      releasedPct: Math.round(pct * 10) / 10,
      grantedAt: granted,
      expiresAt: exp.toISOString(),
      status: pct >= 100 ? "completed" : i % 13 === 12 ? "forfeited" : "active",
    });
  }
  return out;
})();

/* ------------------------------------------------------------------ */
/* Banners                                                              */
/* ------------------------------------------------------------------ */

export type MktPlacement = "dashboard" | "wallet" | "login";

export interface MktBanner {
  id: string;
  headline: string;
  sub: string;
  cta: string;
  eyebrow: string;
  photo: string;
  placement: MktPlacement;
  segments: MktSegment[];
  locales: string[];
  start: string;
  end: string;
  impressions: number;
  clicks: number;
  status: "active" | "scheduled" | "paused" | "expired" | "draft";
  priority: number;
  link: string;
}

export const MKT_BANNERS: MktBanner[] = [
  { id: "bnr_01", eyebrow: "Welcome offer", headline: "Get 30% credit on your first deposit", sub: "Up to $3,000 trading credit. Released $2 per lot.", cta: "Claim bonus", photo: "/assets/photos/trading-screen.jpg", placement: "dashboard", segments: ["New sign-ups (7d)", "KYC pending"], locales: ["EN", "HI", "VI"], start: "2026-09-01T00:00:00+03:00", end: "2026-12-31T23:59:00+03:00", impressions: 412880, clicks: 19420, status: "active", priority: 1, link: "/wallet/deposit?promo=WELCOME30" },
  { id: "bnr_02", eyebrow: "Crypto week", headline: "Double your USDT deposit", sub: "100% credit on USDT TRC20 deposits above $250.", cta: "Deposit USDT", photo: "/assets/photos/crypto-coins.jpg", placement: "wallet", segments: ["Crypto traders", "South-East Asia"], locales: ["EN", "VI", "TH"], start: "2026-09-15T00:00:00+03:00", end: "2026-10-31T23:59:00+03:00", impressions: 188410, clicks: 12840, status: "active", priority: 1, link: "/wallet/deposit?method=usdt" },
  { id: "bnr_03", eyebrow: "Contest", headline: "Gold Rush Championship — $50,000 pool", sub: "Trade XAUUSD on a live account. Top 20 win.", cta: "Join contest", photo: "/assets/photos/gold.jpg", placement: "dashboard", segments: ["All clients"], locales: ["EN", "AR", "PT"], start: "2026-09-20T00:00:00+03:00", end: "2026-10-20T23:59:00+03:00", impressions: 96240, clicks: 6120, status: "active", priority: 2, link: "/contests/gold-rush" },
  { id: "bnr_04", eyebrow: "Trade the world", headline: "Welcome back. Markets never sleep.", sub: "Sign in to 250+ instruments with spreads from 0.0 pips.", cta: "Open terminal", photo: "/assets/photos/skyline.jpg", placement: "login", segments: ["All clients"], locales: ["EN"], start: "2026-07-01T00:00:00+03:00", end: "2026-12-31T23:59:00+03:00", impressions: 1204330, clicks: 21460, status: "active", priority: 1, link: "/trade" },
  { id: "bnr_05", eyebrow: "We miss you", headline: "Your $25 comeback credit is waiting", sub: "Place one trade within 7 days to activate.", cta: "Reactivate", photo: "/assets/photos/london.jpg", placement: "dashboard", segments: ["Dormant 30d"], locales: ["EN", "ES"], start: "2026-09-10T00:00:00+03:00", end: "2026-10-10T23:59:00+03:00", impressions: 38120, clicks: 2980, status: "active", priority: 3, link: "/promo/COMEBACK25" },
  { id: "bnr_06", eyebrow: "Fast withdrawals", headline: "Withdraw to USDT in under 10 minutes", sub: "Zero fees on TRC20 withdrawals this month.", cta: "Learn more", photo: "/assets/photos/bitcoin.jpg", placement: "wallet", segments: ["All clients"], locales: ["EN", "AR"], start: "2026-09-01T00:00:00+03:00", end: "2026-09-30T23:59:00+03:00", impressions: 241800, clicks: 5210, status: "active", priority: 2, link: "/wallet" },
  { id: "bnr_07", eyebrow: "Dubai", headline: "Meet Kalks at Forex Expo Dubai", sub: "Booth 42 · 6–7 October · VIP lounge access.", cta: "Book a meeting", photo: "/assets/photos/dubai.jpg", placement: "login", segments: ["GCC", "VIP"], locales: ["EN", "AR"], start: "2026-10-01T00:00:00+03:00", end: "2026-10-07T23:59:00+03:00", impressions: 0, clicks: 0, status: "scheduled", priority: 2, link: "/events/dubai" },
  { id: "bnr_08", eyebrow: "Go live", headline: "Ready for real markets? Get $30 free", sub: "Verify your identity and switch from demo to live.", cta: "Go live now", photo: "/assets/photos/charts.jpg", placement: "dashboard", segments: ["Demo only"], locales: ["EN", "HI", "ID"], start: "2026-08-01T00:00:00+03:00", end: "2026-12-31T23:59:00+03:00", impressions: 154990, clicks: 11870, status: "active", priority: 1, link: "/accounts/new?type=live" },
  { id: "bnr_09", eyebrow: "Summer", headline: "Summer reload: 20% every Friday", sub: "Paused while finance reviews credit exposure.", cta: "Deposit", photo: "/assets/photos/singapore.jpg", placement: "wallet", segments: ["Funded, no trade", "LATAM"], locales: ["EN", "ES", "PT"], start: "2026-06-01T00:00:00+03:00", end: "2026-09-30T23:59:00+03:00", impressions: 86400, clicks: 3120, status: "paused", priority: 3, link: "/wallet/deposit" },
];

export const MKT_BANNER_PHOTOS = ["trading-screen", "stock-market", "crypto", "bitcoin", "money", "analytics", "skyscrapers", "skyline", "dubai", "singapore", "london", "nyc", "gold", "crypto-coins", "trader", "charts", "finance"] as const;

/* ------------------------------------------------------------------ */
/* Contests                                                             */
/* ------------------------------------------------------------------ */

export interface MktContest {
  id: string;
  name: string;
  type: "demo" | "live";
  metric: string;
  entrants: number;
  capacity: number;
  prizePool: number;
  minDeposit: number;
  status: "running" | "scheduled" | "completed" | "draft";
  start: string;
  end: string;
  photo: string;
  instruments: string;
  prizes: { rank: string; prize: number; label?: string }[];
}

export const MKT_CONTESTS: MktContest[] = [
  { id: "ct_gold", name: "Gold Rush Championship", type: "live", metric: "Gain %", entrants: 1842, capacity: 3000, prizePool: 50000, minDeposit: 500, status: "running", start: "2026-09-20T00:00:00+03:00", end: "2026-10-20T23:59:00+03:00", photo: "/assets/photos/gold.jpg", instruments: "XAUUSD, XAGUSD", prizes: [{ rank: "1st", prize: 20000 }, { rank: "2nd", prize: 10000 }, { rank: "3rd", prize: 5000 }, { rank: "4th – 10th", prize: 1500, label: "each" }, { rank: "11th – 20th", prize: 450, label: "each" }] },
  { id: "ct_demo_weekly", name: "Weekly Demo Sprint #38", type: "demo", metric: "Gain %", entrants: 6214, capacity: 10000, prizePool: 3000, minDeposit: 0, status: "running", start: "2026-09-21T00:00:00+03:00", end: "2026-09-27T23:59:00+03:00", photo: "/assets/photos/trading-screen.jpg", instruments: "All instruments", prizes: [{ rank: "1st", prize: 1000, label: "live credit" }, { rank: "2nd", prize: 600, label: "live credit" }, { rank: "3rd", prize: 400, label: "live credit" }, { rank: "4th – 10th", prize: 100, label: "each" }] },
  { id: "ct_crypto", name: "Crypto Titans Cup", type: "live", metric: "Net profit", entrants: 918, capacity: 2000, prizePool: 25000, minDeposit: 250, status: "running", start: "2026-09-01T00:00:00+03:00", end: "2026-09-30T23:59:00+03:00", photo: "/assets/photos/crypto.jpg", instruments: "BTCUSD, ETHUSD, SOLUSD", prizes: [{ rank: "1st", prize: 10000 }, { rank: "2nd", prize: 6000 }, { rank: "3rd", prize: 4000 }, { rank: "4th – 10th", prize: 715, label: "each" }] },
  { id: "ct_nas", name: "NAS100 Q4 Invitational", type: "live", metric: "Gain %", entrants: 0, capacity: 500, prizePool: 30000, minDeposit: 2500, status: "scheduled", start: "2026-10-05T00:00:00+03:00", end: "2026-12-18T23:59:00+03:00", photo: "/assets/photos/nyc.jpg", instruments: "NAS100, US500", prizes: [{ rank: "1st", prize: 15000 }, { rank: "2nd", prize: 8000 }, { rank: "3rd", prize: 4000 }, { rank: "4th – 5th", prize: 1500, label: "each" }] },
  { id: "ct_student", name: "Campus Trading League", type: "demo", metric: "Sharpe ratio", entrants: 2470, capacity: 5000, prizePool: 12000, minDeposit: 0, status: "completed", start: "2026-07-01T00:00:00+03:00", end: "2026-08-31T23:59:00+03:00", photo: "/assets/photos/analytics.jpg", instruments: "Forex majors", prizes: [{ rank: "1st", prize: 5000, label: "scholarship" }, { rank: "2nd", prize: 3000 }, { rank: "3rd", prize: 2000 }, { rank: "4th – 10th", prize: 285, label: "each" }] },
  { id: "ct_ramadan", name: "Ramadan Swap-free Classic", type: "live", metric: "Gain %", entrants: 0, capacity: 1500, prizePool: 20000, minDeposit: 300, status: "draft", start: "2027-02-18T00:00:00+03:00", end: "2027-03-20T23:59:00+03:00", photo: "/assets/photos/dubai.jpg", instruments: "Islamic group", prizes: [{ rank: "1st", prize: 8000 }, { rank: "2nd", prize: 5000 }, { rank: "3rd", prize: 3000 }, { rank: "4th – 10th", prize: 570, label: "each" }] },
];

export interface MktLeader {
  rank: number;
  person: Person;
  login: string;
  gainPct: number;
  equity: number;
  trades: number;
  winRate: number;
  spark: number[];
}

export const MKT_LEADERBOARD: MktLeader[] = (() => {
  const r = seeded(9921);
  const order = [9, 1, 14, 5, 2, 20, 0, 11, 16, 7, 22, 18];
  let gain = 184.6;
  return order.map((pi, i) => {
    gain = i === 0 ? gain : gain * r.range(0.78, 0.94);
    let v = 100;
    const spark = Array.from({ length: 18 }, (_, k) => (v = v * (1 + (gain / 100 / 18) * r.range(0.2, 1.8) + (k % 5 === 0 ? -0.02 : 0))));
    return {
      rank: i + 1,
      person: PEOPLE[pi]!,
      login: login(r),
      gainPct: Math.round(gain * 100) / 100,
      equity: Math.round(r.range(2400, 48000) * (1 + gain / 100)),
      trades: r.int(28, 412),
      winRate: Math.round(r.range(48, 81) * 10) / 10,
      spark,
    };
  });
})();

/* ------------------------------------------------------------------ */
/* Rewards / loyalty                                                    */
/* ------------------------------------------------------------------ */

export interface MktPointRule {
  id: string;
  group: "Trading" | "Funding" | "Engagement";
  label: string;
  hint: string;
  points: number;
  unit: string;
  icon: string;
  enabled: boolean;
}

export const MKT_POINT_RULES: MktPointRule[] = [
  { id: "pr_fx", group: "Trading", label: "Forex", hint: "Majors, minors and exotics", points: 10, unit: "per lot", icon: "globe_with_meridians", enabled: true },
  { id: "pr_metals", group: "Trading", label: "Metals", hint: "XAUUSD, XAGUSD", points: 14, unit: "per lot", icon: "coin", enabled: true },
  { id: "pr_idx", group: "Trading", label: "Indices", hint: "NAS100, US30, GER40…", points: 6, unit: "per lot", icon: "bar_chart", enabled: true },
  { id: "pr_energy", group: "Trading", label: "Energies", hint: "USOIL, UKOIL, NGAS", points: 8, unit: "per lot", icon: "fire", enabled: true },
  { id: "pr_crypto", group: "Trading", label: "Crypto CFDs", hint: "BTC, ETH, SOL, XRP", points: 18, unit: "per lot", icon: "gem_stone", enabled: true },
  { id: "pr_stocks", group: "Trading", label: "Stocks", hint: "US and EU share CFDs", points: 4, unit: "per 100 shares", icon: "chart_increasing", enabled: false },
  { id: "pr_dep", group: "Funding", label: "Net deposit", hint: "Credited after 7 days if not withdrawn", points: 1, unit: "per $10", icon: "money_bag", enabled: true },
  { id: "pr_ftd", group: "Funding", label: "First deposit", hint: "One-off, min. $100", points: 750, unit: "one-off", icon: "dollar_banknote", enabled: true },
  { id: "pr_kyc", group: "Engagement", label: "KYC completed", hint: "Full verification incl. proof of address", points: 500, unit: "one-off", icon: "identification_card", enabled: true },
  { id: "pr_ref", group: "Engagement", label: "Referral funded", hint: "Referred friend deposits $200+", points: 1000, unit: "per referral", icon: "handshake", enabled: true },
  { id: "pr_streak", group: "Engagement", label: "5-day trading streak", hint: "Trade at least 0.1 lot on 5 consecutive days", points: 250, unit: "per streak", icon: "fire", enabled: true },
  { id: "pr_academy", group: "Engagement", label: "Academy course passed", hint: "Final quiz score 80%+", points: 150, unit: "per course", icon: "graduation_cap", enabled: true },
];

export interface MktTier {
  name: "Bronze" | "Silver" | "Gold" | "Platinum";
  threshold: number;
  multiplier: number;
  members: number;
  perks: string[];
  color: string;
}

export const MKT_TIERS: MktTier[] = [
  { name: "Bronze", threshold: 0, multiplier: 1, members: 48210, perks: ["Points on every lot", "Academy access"], color: "#c07a45" },
  { name: "Silver", threshold: 5000, multiplier: 1.15, members: 9842, perks: ["1.15x points", "Priority chat support", "Free VPS 1 month"], color: "#b9bec7" },
  { name: "Gold", threshold: 25000, multiplier: 1.35, members: 2136, perks: ["1.35x points", "Personal manager", "Swap discount 10%"], color: "#e9b949" },
  { name: "Platinum", threshold: 100000, multiplier: 1.6, members: 284, perks: ["1.6x points", "Same-hour withdrawals", "Event invitations", "Custom leverage review"], color: "#e5e4e2" },
];

export interface MktRewardItem {
  id: string;
  name: string;
  category: "Trading" | "Cash" | "Gadgets" | "Education" | "Experiences";
  icon: string;
  cost: number;
  stock: number | null; // null = unlimited
  redemptions: number;
  tier: MktTier["name"];
  active: boolean;
}

export const MKT_CATALOGUE: MktRewardItem[] = [
  { id: "rw_credit50", name: "$50 trading credit", category: "Cash", icon: "money_bag", cost: 5000, stock: null, redemptions: 3184, tier: "Bronze", active: true },
  { id: "rw_cash100", name: "$100 withdrawable cash", category: "Cash", icon: "dollar_banknote", cost: 12000, stock: null, redemptions: 942, tier: "Silver", active: true },
  { id: "rw_vps", name: "Free VPS · 3 months", category: "Trading", icon: "satellite_antenna", cost: 3500, stock: null, redemptions: 1210, tier: "Bronze", active: true },
  { id: "rw_spread", name: "Spread rebate 20% · 30 days", category: "Trading", icon: "chart_decreasing", cost: 8000, stock: null, redemptions: 618, tier: "Silver", active: true },
  { id: "rw_course", name: "Pro price-action masterclass", category: "Education", icon: "graduation_cap", cost: 2500, stock: null, redemptions: 2041, tier: "Bronze", active: true },
  { id: "rw_books", name: "Trader's library bundle", category: "Education", icon: "books", cost: 1800, stock: 420, redemptions: 380, tier: "Bronze", active: true },
  { id: "rw_laptop", name: "MacBook Pro 14\"", category: "Gadgets", icon: "laptop", cost: 240000, stock: 6, redemptions: 4, tier: "Platinum", active: true },
  { id: "rw_phone", name: "iPhone 17 Pro", category: "Gadgets", icon: "mobile_phone", cost: 150000, stock: 12, redemptions: 9, tier: "Gold", active: true },
  { id: "rw_gift", name: "Kalks merch box", category: "Experiences", icon: "wrapped_gift", cost: 4000, stock: 800, redemptions: 512, tier: "Bronze", active: true },
  { id: "rw_gem", name: "Platinum tier fast-track", category: "Experiences", icon: "gem_stone", cost: 60000, stock: null, redemptions: 38, tier: "Gold", active: true },
  { id: "rw_crown", name: "Dubai VIP trading retreat", category: "Experiences", icon: "crown", cost: 400000, stock: 2, redemptions: 1, tier: "Platinum", active: true },
  { id: "rw_contest", name: "Contest entry ticket", category: "Trading", icon: "trophy", cost: 1500, stock: null, redemptions: 866, tier: "Bronze", active: false },
];

export interface MktRedemption {
  id: string;
  person: Person;
  login: string;
  itemId: string;
  points: number;
  at: string;
  status: "completed" | "pending" | "processing" | "rejected";
  tier: MktTier["name"];
}

export const MKT_REDEMPTIONS: MktRedemption[] = (() => {
  const r = seeded(5512);
  const items = MKT_CATALOGUE.filter((c) => c.cost < 200000);
  return Array.from({ length: 22 }, (_, i) => {
    const it = items[r.int(0, items.length - 1)]!;
    const st = i < 3 ? "pending" : i === 5 ? "processing" : i === 9 ? "rejected" : "completed";
    return {
      id: `RD-${30911 - i}`,
      person: person(i * 7 + 2),
      login: login(r),
      itemId: it.id,
      points: it.cost,
      at: `2026-09-${String(24 - Math.floor(i / 3)).padStart(2, "0")}T${String(21 - (i % 12)).padStart(2, "0")}:${String((i * 13) % 60).padStart(2, "0")}:00+03:00`,
      status: st,
      tier: it.cost > 50000 ? "Gold" : it.cost > 6000 ? "Silver" : "Bronze",
    } as MktRedemption;
  });
})();

export const MKT_REWARDS_KPIS = {
  members: MKT_TIERS.reduce((s, t) => s + t.members, 0),
  pointsIssued30d: 48_210_400,
  pointsRedeemed30d: 21_884_900,
  liability: 186_420, // USD value of outstanding points
};

/* ------------------------------------------------------------------ */
/* Automation journeys                                                  */
/* ------------------------------------------------------------------ */

export type MktNodeKind = "trigger" | "wait" | "condition" | "email" | "inapp" | "sms" | "push" | "bonus" | "tag" | "goal" | "exit";

export interface MktNode {
  id: string;
  kind: MktNodeKind;
  title: string;
  detail: string;
  x: number;
  y: number;
  entered: number;
  passed: number;
  config: [string, string][];
}

export interface MktEdge {
  from: string;
  to: string;
  label?: "Yes" | "No";
}

export interface MktJourney {
  id: string;
  name: string;
  trigger: string;
  goal: string;
  enrolled: number;
  converted: number;
  active: number;
  enabled: boolean;
  updated: string;
  owner: Person;
  nodes: MktNode[];
  edges: MktEdge[];
}

// canvas layout grid
const CX = [24, 254, 484, 714, 944];
const CY = [24, 160, 296, 432];

function journey(
  base: Omit<MktJourney, "nodes" | "edges">,
  spec: {
    trigger: [string, string, [string, string][]];
    wait: [string, string];
    cond: [string, string];
    yes: [MktNodeKind, string, string, [string, string][]][];
    no: [MktNodeKind, string, string];
    goal: [string, string];
  },
): MktJourney {
  const e = base.enrolled;
  const afterWait = Math.round(e * 0.93);
  const yesN = Math.round(afterWait * 0.71);
  const noN = afterWait - yesN;
  const ch: MktNode[] = spec.yes.map(([kind, title, detail, config], i) => ({
    id: `n_ch${i}`,
    kind,
    title,
    detail,
    x: CX[3]!,
    y: CY[i]!,
    entered: kind === "sms" ? Math.round(yesN * 0.42) : yesN,
    passed: Math.round((kind === "sms" ? yesN * 0.42 : yesN) * (kind === "email" ? 0.97 : kind === "inapp" ? 0.88 : 0.99)),
    config,
  }));
  const nodes: MktNode[] = [
    { id: "n_trig", kind: "trigger", title: spec.trigger[0], detail: spec.trigger[1], x: CX[0]!, y: CY[1]! + 68, entered: e, passed: e, config: spec.trigger[2] },
    { id: "n_wait", kind: "wait", title: spec.wait[0], detail: spec.wait[1], x: CX[1]!, y: CY[1]! + 68, entered: e, passed: afterWait, config: [["Delay", spec.wait[0].replace("Wait ", "")], ["Quiet hours", "22:00 – 08:00 local"], ["Business days only", "No"]] },
    { id: "n_cond", kind: "condition", title: spec.cond[0], detail: spec.cond[1], x: CX[2]!, y: CY[1]! + 68, entered: afterWait, passed: yesN, config: [["Rule", spec.cond[1]], ["Yes branch", `${yesN.toLocaleString("en-US")} clients`], ["No branch", `${noN.toLocaleString("en-US")} clients`]] },
    ...ch,
    { id: "n_exit", kind: spec.no[0], title: spec.no[1], detail: spec.no[2], x: CX[3]!, y: CY[3]!, entered: noN, passed: noN, config: [["Action", spec.no[2]], ["Re-entry", "After 30 days"]] },
    { id: "n_goal", kind: "goal", title: spec.goal[0], detail: spec.goal[1], x: CX[4]!, y: CY[1]!, entered: yesN, passed: base.converted, config: [["Goal event", spec.goal[1]], ["Attribution window", "7 days"], ["Exit on goal", "Yes"]] },
  ];
  const edges: MktEdge[] = [
    { from: "n_trig", to: "n_wait" },
    { from: "n_wait", to: "n_cond" },
    ...ch.map((c) => ({ from: "n_cond", to: c.id, label: "Yes" as const })),
    { from: "n_cond", to: "n_exit", label: "No" },
    ...ch.map((c) => ({ from: c.id, to: "n_goal" })),
  ];
  return { ...base, nodes, edges };
}

export const MKT_JOURNEYS: MktJourney[] = [
  journey(
    { id: "jr_ftd_notrade", name: "Deposited, no trade in 48h", trigger: "First deposit ≥ $100", goal: "First trade", enrolled: 3842, converted: 1596, active: 412, enabled: true, updated: "2026-09-22T14:10:00+03:00", owner: PEOPLE[4]! },
    {
      trigger: ["First deposit", "Deposit ≥ $100 on a live account", [["Event", "deposit.completed (first)"], ["Filter", "amount ≥ $100 · live"], ["Entry limit", "Once per client"]]],
      wait: ["Wait 48 hours", "Give the client time to trade"],
      cond: ["No trade?", "trades.count = 0 since deposit"],
      yes: [
        ["email", "Email · First trade in 3 steps", "Template: onboarding-first-trade-v4", [["Template", "onboarding-first-trade-v4"], ["Subject", "Your $500 is ready. Here's your first trade."], ["Open rate", "48.2%"], ["Click rate", "11.6%"]]],
        ["inapp", "In-app · Trade idea card", "Dashboard modal with XAUUSD idea", [["Placement", "Dashboard modal"], ["Content", "Guided XAUUSD 0.01 lot trade"], ["Dismiss rate", "22%"]]],
        ["sms", "SMS · Account manager call", "Only for deposits ≥ $1,000", [["Sender", "KALKS"], ["Message", "Hi {first_name}, your manager can place a guided first trade with you. Reply CALL."], ["Filter", "deposit ≥ $1,000"]]],
      ],
      no: ["exit", "Exit · Already trading", "Tag client as activated"],
      goal: ["Goal · First trade", "position.opened within 7 days"],
    },
  ),
  journey(
    { id: "jr_kyc", name: "KYC started, not finished", trigger: "KYC step 1 submitted", goal: "KYC approved", enrolled: 5218, converted: 2874, active: 690, enabled: true, updated: "2026-09-21T09:42:00+03:00", owner: PEOPLE[12]! },
    {
      trigger: ["KYC started", "Identity step submitted", [["Event", "kyc.step_submitted"], ["Filter", "step = identity"], ["Entry limit", "Once per client"]]],
      wait: ["Wait 24 hours", "Allow time to finish"],
      cond: ["Still incomplete?", "kyc.status ≠ approved"],
      yes: [
        ["email", "Email · Finish in 2 minutes", "Template: kyc-reminder-v2", [["Template", "kyc-reminder-v2"], ["Subject", "One step left to unlock withdrawals"], ["Open rate", "52.9%"]]],
        ["push", "Push · Upload proof of address", "Mobile app deep link", [["Deep link", "kalks://profile/verification"], ["CTR", "14.1%"]]],
        ["sms", "SMS · Help from support", "Link to live chat", [["Sender", "KALKS"], ["Message", "Need help verifying? Chat with us: kalkstrade.com/help"]]],
      ],
      no: ["exit", "Exit · KYC approved", "Remove from journey"],
      goal: ["Goal · KYC approved", "kyc.approved within 14 days"],
    },
  ),
  journey(
    { id: "jr_dormant", name: "Dormant 30 days win-back", trigger: "No login for 30 days", goal: "Trade placed", enrolled: 11420, converted: 1644, active: 2210, enabled: true, updated: "2026-09-18T18:05:00+03:00", owner: PEOPLE[9]! },
    {
      trigger: ["Dormant 30 days", "No login and no open positions", [["Event", "client.dormant"], ["Filter", "balance ≥ $10"], ["Entry limit", "Every 90 days"]]],
      wait: ["Wait 2 days", "Stagger sends"],
      cond: ["Balance ≥ $50?", "account.balance ≥ 50"],
      yes: [
        ["email", "Email · What you missed", "Weekly market recap", [["Template", "winback-recap-v3"], ["Open rate", "21.4%"]]],
        ["bonus", "Bonus · $25 comeback credit", "Auto-grant COMEBACK25", [["Code", "COMEBACK25"], ["Credit", "$25 · release $1/lot"], ["Expiry", "7 days"]]],
        ["inapp", "In-app · Welcome back banner", "Dashboard hero", [["Placement", "Dashboard hero"], ["Banner", "bnr_05"]]],
      ],
      no: ["email", "Email · Deposit reminder", "Low balance nurture"],
      goal: ["Goal · Trade placed", "position.opened within 14 days"],
    },
  ),
  journey(
    { id: "jr_margin", name: "Margin call education", trigger: "Margin level < 120%", goal: "Academy lesson done", enrolled: 2104, converted: 988, active: 164, enabled: true, updated: "2026-09-12T11:30:00+03:00", owner: PEOPLE[4]! },
    {
      trigger: ["Margin call", "Margin level fell below 120%", [["Event", "risk.margin_call"], ["Filter", "first time in 30 days"], ["Entry limit", "Once per 30 days"]]],
      wait: ["Wait 6 hours", "After the market settles"],
      cond: ["Stopped out?", "stop_out event within 6h"],
      yes: [
        ["email", "Email · Managing leverage", "Risk education series #1", [["Template", "risk-leverage-101"], ["Open rate", "44.0%"]]],
        ["inapp", "In-app · Position sizing tool", "Calculator walkthrough", [["Placement", "Terminal side panel"]]],
        ["tag", "Tag · Risk-education cohort", "Sync to CRM segment", [["Segment", "risk-education-2026Q3"]]],
      ],
      no: ["inapp", "In-app · Margin tips", "Soft reminder only"],
      goal: ["Goal · Lesson completed", "academy.lesson_completed"],
    },
  ),
  journey(
    { id: "jr_demo_live", name: "Demo → live upgrade", trigger: "Demo profit ≥ 10%", goal: "Live account funded", enrolled: 7630, converted: 1241, active: 1320, enabled: false, updated: "2026-09-02T16:20:00+03:00", owner: PEOPLE[21]! },
    {
      trigger: ["Demo milestone", "Demo return ≥ 10% over 14 days", [["Event", "demo.milestone"], ["Filter", "return ≥ 10% · 20+ trades"], ["Entry limit", "Once per client"]]],
      wait: ["Wait 1 day", "Let the win sink in"],
      cond: ["No live account?", "accounts.live.count = 0"],
      yes: [
        ["email", "Email · You're ready for live", "Template: demo-graduate-v5", [["Template", "demo-graduate-v5"], ["Open rate", "39.8%"]]],
        ["bonus", "Bonus · $30 no-deposit credit", "Campaign bn_nodep30", [["Campaign", "No-deposit $30 demo-to-live"], ["KYC required", "Yes"]]],
        ["push", "Push · Open live in 60 seconds", "Mobile deep link", [["Deep link", "kalks://accounts/new?type=live"]]],
      ],
      no: ["exit", "Exit · Already live", "Suppress for 60 days"],
      goal: ["Goal · Live funded", "deposit.completed on live"],
    },
  ),
];

/* ------------------------------------------------------------------ */
/* Promo codes                                                          */
/* ------------------------------------------------------------------ */

export type MktPromoType = "deposit-bonus" | "fee-discount" | "free-vps" | "contest-entry";

export interface MktPromoCode {
  code: string;
  type: MktPromoType;
  value: string;
  uses: number;
  limit: number;
  expires: string;
  owner: { kind: "IB" | "Campaign"; name: string; person?: Person };
  status: "active" | "paused" | "expired" | "scheduled";
  created: string;
  revenue: number;
}

export const MKT_PROMO_CODES: MktPromoCode[] = [
  { code: "WELCOME30", type: "deposit-bonus", value: "30% up to $3,000", uses: 4812, limit: 10000, expires: "2026-12-31T23:59:00+03:00", owner: { kind: "Campaign", name: "30% welcome credit" }, status: "active", created: "2026-01-05T10:00:00+03:00", revenue: 1_842_000 },
  { code: "USDT100", type: "deposit-bonus", value: "100% up to $5,000", uses: 1396, limit: 2500, expires: "2026-10-31T23:59:00+03:00", owner: { kind: "Campaign", name: "100% crypto deposit boost" }, status: "active", created: "2026-08-15T09:00:00+03:00", revenue: 986_400 },
  { code: "ARJUN-FX", type: "deposit-bonus", value: "20% up to $1,000", uses: 342, limit: 500, expires: "2026-11-30T23:59:00+03:00", owner: { kind: "IB", name: "Arjun Mehta", person: PEOPLE[0] }, status: "active", created: "2026-06-11T12:00:00+03:00", revenue: 214_900 },
  { code: "FATIMA-GCC", type: "fee-discount", value: "-25% commission", uses: 188, limit: 300, expires: "2026-12-15T23:59:00+03:00", owner: { kind: "IB", name: "Fatima Al-Sayed", person: PEOPLE[1] }, status: "active", created: "2026-07-02T08:30:00+03:00", revenue: 142_300 },
  { code: "VPS3FREE", type: "free-vps", value: "3 months VPS", uses: 928, limit: 1000, expires: "2026-09-30T23:59:00+03:00", owner: { kind: "Campaign", name: "Algo traders Q3" }, status: "active", created: "2026-07-01T09:00:00+03:00", revenue: 318_700 },
  { code: "GOLDRUSH", type: "contest-entry", value: "Free entry", uses: 1842, limit: 3000, expires: "2026-10-20T23:59:00+03:00", owner: { kind: "Campaign", name: "Gold Rush Championship" }, status: "active", created: "2026-09-15T09:00:00+03:00", revenue: 402_100 },
  { code: "LUCAS-BR", type: "deposit-bonus", value: "15% up to $750", uses: 611, limit: 600, expires: "2026-09-15T23:59:00+03:00", owner: { kind: "IB", name: "Lucas Ferreira", person: PEOPLE[2] }, status: "expired", created: "2026-04-20T10:15:00+03:00", revenue: 176_500 },
  { code: "ZEROFEE-SEA", type: "fee-discount", value: "Zero commission 14d", uses: 207, limit: 1000, expires: "2026-11-15T23:59:00+03:00", owner: { kind: "Campaign", name: "South-East Asia launch" }, status: "active", created: "2026-09-10T09:00:00+03:00", revenue: 58_200 },
  { code: "COMEBACK25", type: "deposit-bonus", value: "$25 credit", uses: 1644, limit: 5000, expires: "2026-12-31T23:59:00+03:00", owner: { kind: "Campaign", name: "Dormant 30 days win-back" }, status: "active", created: "2026-05-18T09:00:00+03:00", revenue: 96_800 },
  { code: "MEI-SG", type: "free-vps", value: "6 months VPS", uses: 44, limit: 100, expires: "2026-12-31T23:59:00+03:00", owner: { kind: "IB", name: "Mei Lin", person: PEOPLE[8] }, status: "paused", created: "2026-08-02T14:00:00+03:00", revenue: 31_900 },
  { code: "NAS100Q4", type: "contest-entry", value: "Free entry + $50", uses: 0, limit: 500, expires: "2026-12-18T23:59:00+03:00", owner: { kind: "Campaign", name: "NAS100 Q4 Invitational" }, status: "scheduled", created: "2026-09-23T17:40:00+03:00", revenue: 0 },
  { code: "OMAR-VIP", type: "fee-discount", value: "-40% commission", uses: 58, limit: 75, expires: "2027-03-31T23:59:00+03:00", owner: { kind: "IB", name: "Omar Haddad", person: PEOPLE[5] }, status: "active", created: "2026-06-30T10:00:00+03:00", revenue: 612_400 },
  { code: "DANIEL-NG", type: "deposit-bonus", value: "25% up to $500", uses: 402, limit: 1000, expires: "2026-12-31T23:59:00+03:00", owner: { kind: "IB", name: "Daniel Okafor", person: PEOPLE[7] }, status: "active", created: "2026-05-05T11:00:00+03:00", revenue: 88_300 },
  { code: "SPRINT38", type: "contest-entry", value: "Demo sprint entry", uses: 6214, limit: 10000, expires: "2026-09-27T23:59:00+03:00", owner: { kind: "Campaign", name: "Weekly Demo Sprint #38" }, status: "active", created: "2026-09-19T09:00:00+03:00", revenue: 22_600 },
];

export const MKT_PROMO_KPIS = {
  active: MKT_PROMO_CODES.filter((c) => c.status === "active").length,
  redemptions30d: 9_412,
  bonusValue30d: 612_840,
  attributedDeposits: MKT_PROMO_CODES.reduce((s, c) => s + c.revenue, 0),
  daily: [220, 260, 240, 310, 290, 330, 360, 342, 318, 390, 410, 380, 402, 455],
};

/* ------------------------------------------------------------------ */
/* UTM campaigns                                                        */
/* ------------------------------------------------------------------ */

export interface MktCampaign {
  id: string;
  source: string;
  medium: string;
  campaign: string;
  spend: number;
  clicks: number;
  signups: number;
  kyc: number;
  ftds: number;
  ftdAmount: number;
  active: number;
  revenue: number;
  status: "running" | "paused" | "completed";
}

const CAMPAIGN_RAW: [string, string, string, number, number, number, number][] = [
  // source, medium, campaign, spend, clicks, signup rate, revenue multiple
  ["google", "cpc", "gold_brand_search_in", 48200, 96400, 0.084, 3.4],
  ["meta", "paid_social", "usdt_boost_sea", 62100, 214800, 0.041, 2.1],
  ["tiktok", "paid_social", "demo_challenge_latam", 28400, 188200, 0.032, 0.72],
  ["youtube", "video", "masterclass_gcc", 19800, 41200, 0.061, 2.8],
  ["ib_network", "referral", "ib_q3_push", 0, 38400, 0.142, 999],
  ["telegram", "community", "signals_vn", 6400, 22800, 0.098, 4.6],
  ["google", "display", "retarget_dormant", 12600, 51200, 0.038, 1.35],
  ["x", "paid_social", "nas100_invitational", 8900, 19600, 0.044, 0.86],
  ["affiliate", "cpa", "forexcompare_uk", 34000, 28700, 0.121, 1.9],
  ["newsletter", "email", "sept_market_outlook", 900, 12400, 0.052, 11.2],
  ["bing", "cpc", "broker_generic_ae", 7200, 9800, 0.071, 1.1],
  ["meta", "paid_social", "welcome30_africa", 21400, 132500, 0.029, 0.64],
];

export const MKT_CAMPAIGNS: MktCampaign[] = CAMPAIGN_RAW.map(([source, medium, campaign, spend, clicks, sr, mult], i) => {
  const r = seeded(600 + i);
  const signups = Math.round(clicks * sr);
  const kyc = Math.round(signups * r.range(0.46, 0.68));
  const ftds = Math.round(kyc * r.range(0.36, 0.58));
  const ftdAmount = Math.round(ftds * r.range(380, 1650));
  const active = Math.round(ftds * r.range(0.52, 0.78));
  const revenue = mult === 999 ? Math.round(ftdAmount * 0.21) : Math.round(spend * mult);
  return { id: `cmp_${i + 1}`, source, medium, campaign, spend, clicks, signups, kyc, ftds, ftdAmount, active, revenue, status: i === 7 ? "paused" : i === 9 ? "completed" : "running" };
});

/** Weekly spend vs attributed revenue (USD), last 12 weeks. */
export const MKT_SPEND_REVENUE = (() => {
  const r = seeded(3131);
  return Array.from({ length: 12 }, (_, i) => {
    const d = new Date(Date.UTC(2026, 6, 6 + i * 7));
    const spend = Math.round(15000 + i * 1200 + r.range(-3000, 4000));
    return {
      label: `${String(d.getUTCDate()).padStart(2, "0")} ${["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][d.getUTCMonth()]}`,
      spend,
      revenue: Math.round(spend * (1.3 + i * 0.09 + r.range(-0.25, 0.35))),
    };
  });
})();

export const MKT_UTM_SOURCES = ["google", "meta", "tiktok", "youtube", "telegram", "x", "affiliate", "newsletter", "bing", "ib_network"];
export const MKT_UTM_MEDIUMS = ["cpc", "paid_social", "display", "video", "email", "referral", "cpa", "community"];


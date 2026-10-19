// Challenges (phase 4) shapes for the Back Office: contest templates, prize-pool splits, missions, payout approvals
// and levels. Mirrors services/growth/README.md "Challenges"; money in USD.

import type { AntiCheat, ContestInstrument, Prize, Scoring } from "./api";

export type Band = { rankFrom: number; rankTo: number; pct: number };
export type SplitPreset = { key: string; label: string; bands: Band[] };
export type Split = string | Band[];

export type ContestTemplate = {
  id: number;
  key: string;
  name: string;
  description: string;
  kind: "live" | "demo";
  instrument: ContestInstrument;
  scoring: Scoring;
  schedule: "next_monday" | "next_month" | "next_weekend" | "tomorrow";
  durationDays: number;
  minTrades: number;
  maxEntrants: number | null;
  accountGroups: string[];
  assetClasses: string[];
  kycRequired: boolean;
  minEquity: number | null;
  prizePool: number;
  split: Split;
  payout: "wallet" | "credit";
  rules: string;
  antiCheat: AntiCheat;
  active: boolean;
  sort: number;
  updatedAt: string;
};

export type TemplatesResp = { items: ContestTemplate[]; presets: SplitPreset[]; autoPayBelow: number; assetClasses: string[] };

export type MissionKind = "kyc" | "first_deposit" | "deposit" | "lots" | "trading_days" | "hold" | "join_contest" | "referral";

export type Mission = {
  id: number;
  name: string;
  description: string;
  kind: MissionKind;
  schedule: "daily" | "weekly" | "once";
  target: number;
  params: { minLots?: number; minHoldSeconds?: number; assetClasses?: string[] };
  xp: number;
  rewardAmount: number | null;
  rewardPayout: "wallet" | "credit" | null;
  kycRequired: boolean;
  active: boolean;
  startsAt: string;
  endsAt: string | null;
  sort: number;
  updatedAt: string;
  completions: number;
  completions7d: number;
  participants: number;
};

export type MissionsResp = { items: Mission[]; kinds: MissionKind[]; defaults: { minLots: number; minHoldSeconds: number; autoPayBelow: number } };

export type PayoutRow = {
  source: "mission" | "contest";
  id: number;
  contestId: number | null;
  userId: number;
  name: string;
  amount: number;
  payout: "wallet" | "credit" | null;
  /** approval | pending | failed | paid | kyc | rejected */
  status: string;
  title: string;
  error: string | null;
  at: string | null;
};

export type PayoutsResp = { items: PayoutRow[]; total: number; autoPayBelow: number };

export type Level = { level: number; name: string; minXp: number; badge: string; clients?: number };
export type LevelsResp = { levels: Level[]; badges: string[] };

export const SPLIT_FALLBACK: SplitPreset[] = [
  { key: "winner", label: "Winner takes all", bands: [{ rankFrom: 1, rankTo: 1, pct: 100 }] },
  { key: "top3", label: "Top 3 · 50 / 30 / 20", bands: [{ rankFrom: 1, rankTo: 1, pct: 50 }, { rankFrom: 2, rankTo: 2, pct: 30 }, { rankFrom: 3, rankTo: 3, pct: 20 }] },
];

/** Bands of a split (a preset key or explicit bands). */
export function bandsOf(split: Split, presets: SplitPreset[]): Band[] {
  if (Array.isArray(split)) return split;
  return (presets.find((p) => p.key === split) ?? SPLIT_FALLBACK.find((p) => p.key === split) ?? SPLIT_FALLBACK[1]!).bands;
}

const cents = (v: number) => Math.round(v * 100);

/**
 * Prize rows from a pool and bands, exactly like the service (challenge_calc::split_pool): each rank gets
 * pool × pct ÷ 100 ÷ ranks in its band, rounded down to the cent; the cents lost go to rank 1.
 */
export function splitPool(pool: number, bands: Band[], payout: "wallet" | "credit"): Prize[] {
  if (!(pool > 0)) return [];
  const poolC = cents(pool);
  const rows: Prize[] = bands
    .map((b) => {
      const n = b.rankTo - b.rankFrom + 1;
      const each = Math.floor((poolC * b.pct) / 100 / n);
      return { rankFrom: b.rankFrom, rankTo: b.rankTo, amount: each / 100, payout };
    })
    .filter((p) => p.amount > 0);
  const total = rows.reduce((s, p) => s + cents(p.amount) * (p.rankTo - p.rankFrom + 1), 0);
  const rest = poolC - total;
  if (rest > 0 && rows[0]) {
    const first = rows[0];
    if (first.rankFrom === first.rankTo) first.amount = (cents(first.amount) + rest) / 100;
    else {
      rows.unshift({ rankFrom: first.rankFrom, rankTo: first.rankFrom, amount: (cents(first.amount) + rest) / 100, payout });
      first.rankFrom += 1;
    }
  }
  return rows;
}

/** Shares must cover ranks in order and add up to 100 %. Returns an error text or null. */
export function bandsError(b: Band[]): string | null {
  if (!b.length) return "Add at least one band.";
  let last = 0;
  let total = 0;
  for (const x of b) {
    if (!(x.rankFrom > last) || !(x.rankTo >= x.rankFrom)) return "Bands must go down the table in order without overlapping.";
    if (!(x.pct > 0)) return "Every band needs a share above 0%.";
    total += x.pct;
    last = x.rankTo;
  }
  return Math.abs(total - 100) > 0.01 ? `The shares add up to ${+total.toFixed(2)}%, not 100%.` : null;
}

export const prizeTotal = (p: Prize[]) => p.reduce((s, x) => s + x.amount * (x.rankTo - x.rankFrom + 1), 0);

/** Window of a template schedule from now, in server time (GMT+3), like the service's next_window. */
export function nextWindow(schedule: ContestTemplate["schedule"], days: number, now = new Date()): { start: Date; end: Date } {
  const OFF = 3 * 3600_000;
  const server = new Date(now.getTime() + OFF);
  const day = (y: number, m: number, d: number) => new Date(Date.UTC(y, m, d) - OFF);
  const y = server.getUTCFullYear();
  const mo = server.getUTCMonth();
  const d = server.getUTCDate();
  const dow = server.getUTCDay(); // 0 = Sunday
  const n = Math.max(1, Math.min(366, Math.round(days)));
  if (schedule === "next_month") return { start: day(y, mo + 1, 1), end: day(y, mo + 2, 1) };
  if (schedule === "next_weekend") {
    const add = ((6 - dow + 7) % 7) || 7;
    return { start: day(y, mo, d + add), end: day(y, mo, d + add + 2) };
  }
  if (schedule === "tomorrow") return { start: day(y, mo, d + 1), end: day(y, mo, d + 1 + n) };
  const add = ((1 - dow + 7) % 7) || 7;
  return { start: day(y, mo, d + add), end: day(y, mo, d + add + n) };
}

export const SCHEDULE_LABEL: Record<ContestTemplate["schedule"], string> = { next_monday: "Next week (from Monday)", next_month: "Next calendar month", next_weekend: "Next weekend (Sat–Sun)", tomorrow: "From tomorrow" };

export const MISSION_KIND: Record<MissionKind, { label: string; target: string; unit: string; once?: boolean }> = {
  kyc: { label: "Complete KYC", target: "", unit: "", once: true },
  first_deposit: { label: "First deposit", target: "", unit: "", once: true },
  deposit: { label: "Deposit at least", target: "Amount", unit: "USD" },
  lots: { label: "Trade lots", target: "Lots", unit: "lots" },
  trading_days: { label: "Trade on different days", target: "Days", unit: "days" },
  hold: { label: "Hold a position", target: "Hours", unit: "hours" },
  join_contest: { label: "Join a live contest", target: "Contests", unit: "contests" },
  referral: { label: "Refer a friend who deposits", target: "Friends", unit: "friends" },
};

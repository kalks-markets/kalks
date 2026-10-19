"use client";

// Browser side of the growth BFF (app/api/growth/[[...path]]/route.ts -> services/growth /v1/growth/me/*).
// Live builds only: demo builds keep the mock rewards data from @kalks/mock/rewards.
// Shapes mirror services/growth/README.md. Money is USD, sent as JSON numbers; times are RFC 3339.

import * as React from "react";
import { toast } from "sonner";
import type { MessageKey } from "@kalks/i18n";
import { intlTag } from "@kalks/i18n/locales";
import { tr } from "@kalks/i18n/react";
import { readCached, writeCached } from "@kalks/ui/swr-cache";

/* ------------------------------------------------------------------ */
/* Shapes                                                              */
/* ------------------------------------------------------------------ */

export type Id = number;

export interface Tier {
  key: string;
  name: string;
  rank: number;
  minPoints: number;
  multiplier: number;
  perks: string[];
}

export interface EarnRule {
  id: Id;
  name: string;
  assetClass: string | null;
  symbols: string[];
  accountGroups: string[];
  accountType: string;
  pointsPerLot: number;
}

export type CatalogueKind = "cashback" | "bonus_credit" | "fee_discount" | string;

export interface CatalogueItem {
  id: Id;
  name: string;
  description: string;
  kind: CatalogueKind;
  costPoints: number;
  value: number;
  minTier: string | null;
  stock: number | null;
  active: boolean;
  params: Record<string, unknown>;
}

export type PointsKind = "earn" | "redeem" | "bonus" | "promo" | "expire" | "adjust" | "reversal";

export interface PointsTx {
  id: Id;
  kind: PointsKind | string;
  points: number;
  description: string;
  login: number | null;
  dealId: number | null;
  createdAt: string;
}

export interface Rewards {
  points: {
    balance: number;
    lifetime: number;
    earnedThisMonth: number;
    lotsThisMonth: number;
    earned12m: number;
    expiringSoon: { points: number; at: string } | null;
  };
  tier: { key: string; name: string; rank: number; multiplier: number; minPoints: number; perks: string[] };
  nextTier: { key: string; name: string; minPoints: number; pointsToGo: number } | null;
  tiers: Tier[];
  rules: EarnRule[];
  pointValue: number;
  minHoldSeconds: number;
  pointsExpiryMonths: number;
  catalogue: CatalogueItem[];
  recent: PointsTx[];
  series: { day: string; points: number }[];
}

export interface PointsPage {
  items: PointsTx[];
  page: number;
  limit: number;
  total: number;
}

export interface Redemption {
  id: Id;
  itemId: Id;
  itemName: string;
  kind: CatalogueKind;
  points: number;
  value: number;
  status: "pending" | "completed" | "failed" | string;
  login: number | null;
  voucherCode: string | null;
  grantId: Id | null;
  error: string | null;
  createdAt: string;
  completedAt: string | null;
}

export interface Voucher {
  id: Id;
  code: string;
  kind: "fee_discount" | string;
  pct: number;
  appliesTo: "any" | "prop" | "commission" | string;
  status: "active" | "used" | "expired" | string;
  expiresAt: string;
  usedAt: string | null;
  source: string;
}

export interface CashbackProgramme {
  id: Id;
  name: string;
  description: string;
  assetClasses: string[];
  symbols: string[];
  accountGroups: string[];
  usdPerLot: number;
  maxPerMonth: number | null;
  optIn: boolean;
  enrolled: boolean;
  startsAt: string;
  endsAt: string | null;
  lotsMonth: number;
  earnedMonth: number;
}

export interface CashbackAccrual {
  id: Id;
  programmeId: Id;
  programme: string;
  dealId: number;
  login: number;
  symbol: string;
  lots: number;
  amount: number;
  status: string;
  createdAt: string;
}

export interface CashbackPayout {
  id: Id;
  amount: number;
  status: "pending" | "paid" | "failed" | string;
  createdAt: string;
  paidAt: string | null;
}

export interface CashbackMe {
  programmes: CashbackProgramme[];
  totals: { accrued: number; paid: number; month: number; lifetime: number };
  accruals: CashbackAccrual[];
  payouts: CashbackPayout[];
  series: { day: string; amount: number }[];
}

export interface CampaignPublic {
  id: Id;
  name: string;
  description: string;
  terms: string;
  kind: "deposit" | "fixed";
  pct: number;
  cap: number;
  fixedAmount: number;
  minDeposit: number;
  releasePerLot: number;
  expiryDays: number;
  forfeitOnWithdrawal: boolean;
  claimWindowDays: number;
  startsAt: string;
  endsAt: string | null;
  eligible: boolean;
  reason: string | null;
  claimed: boolean;
}

export type GrantStatus = "awaiting_deposit" | "pending" | "active" | "completed" | "forfeited" | "expired" | "cancelled" | "failed";

export interface Grant {
  id: Id;
  campaignId: Id;
  campaign: string;
  status: GrantStatus | string;
  source: string;
  login: number | null;
  depositAmount: number | null;
  amount: number;
  released: number;
  remaining: number;
  lotsTraded: number;
  lotsRequired: number;
  releasePerLot: number;
  progressPct: number;
  claimedAt: string;
  grantedAt: string | null;
  expiresAt: string | null;
  endedAt: string | null;
  endReason: string | null;
  claimDeadline: string | null;
}

export interface PromoUse {
  id: Id;
  code: string;
  kind: string;
  status: "applied" | "blocked" | string;
  reason: string | null;
  createdAt: string;
}

export interface Promotions {
  campaigns: CampaignPublic[];
  grants: Grant[];
  promoHistory: PromoUse[];
}

export interface PromoResult {
  result: { kind: string; message: string; grant?: Grant; points?: number; voucher?: Voucher };
}

export type ContestStatus = "draft" | "scheduled" | "running" | "ended" | "finalized" | "paid" | "cancelled";

export interface Prize {
  rankFrom: number;
  rankTo: number;
  amount: number;
  payout: "wallet" | "credit" | string;
}

export interface Contest {
  id: Id;
  slug: string;
  name: string;
  description: string;
  kind: "demo" | "live";
  /** What is traded: CFDs (lots) or Kalks FX Options (contracts, O36). Older services omit it (= cfd). */
  instrument?: "cfd" | "options" | string;
  /** Options contests: minimum opening premium per trade (USD) for a trade to add volume and count; null = none. */
  minPremium?: number | null;
  status: ContestStatus | string;
  startsAt: string;
  endsAt: string;
  scoring: "return_pct" | "profit" | "lots" | "contracts" | string;
  minTrades: number;
  maxEntrants: number | null;
  entrants: number;
  startingBalance: number | null;
  demoGroup: string | null;
  accountGroups: string[];
  kycRequired: boolean;
  minEquity: number | null;
  prizes: Prize[];
  prizePool: number;
  rules: string;
  antiCheat: { minHoldSeconds: number; maxSingleTradePct: number; disqualifyOnBalanceChange: boolean };
  updatedAt: string;
}

export interface Standing {
  entryId: Id;
  userId: number | null;
  name: string;
  country: string | null;
  login: number | null;
  rank: number | null;
  score: number;
  returnPct: number;
  profit: number;
  lots: number;
  /** Options contests: contracts of the trades that count. */
  contracts?: number;
  trades: number;
  /** Options contests, own entry only: trades left out as self-trades, and below the minimum premium. */
  selfTrades?: number | null;
  smallTrades?: number | null;
  qualified: boolean;
  status: "active" | "disqualified" | string;
  prize: number | null;
  prizeStatus: string | null;
  me: boolean;
  updatedAt: string;
  /** Client views (phase 4): the entrant's level badge; `name` is their leaderboard nickname. */
  level?: { level: number; name: string; badge: string };
}

export type ContestCard = Contest & { myEntry: Standing | null };

export interface ContestsResp {
  items: ContestCard[];
  stats: { entered: number; prizesWon: number; prizeFinishes: number; bestRank: number | null; active: number };
}

export interface ContestDetail {
  contest: Contest;
  leaderboard: Standing[];
  myEntry: Standing | null;
  entrants: number;
}

export interface JoinResult {
  entry: Standing;
  credentials?: { login: number; password: string; investorPassword: string };
}

export interface BannerView {
  id: Id;
  title: string;
  body: string;
  ctaLabel: string | null;
  ctaUrl: string | null;
  imageUrl: string | null;
  tone: "ember" | "gold" | "neutral" | "up" | string;
  placement: string;
  dismissible: boolean;
}

/** Options share card (services/growth shares.rs option_card): premiums in USD per contract. */
export interface ShareOption {
  series: string;
  underlying: string;
  right: "call" | "put";
  strike: number | null;
  expiry: string | null;
  style: string;
  side: "buy" | "sell" | string;
  contracts: number;
  entryPremium: number | null;
  exitPremium: number | null;
  pnlPct: number | null;
  reason: "closed" | "expired" | "knocked_out" | "stop_out" | "sl" | "tp" | string;
  breakeven: number | null;
  settle: number | null;
}

export interface ShareData {
  name: string;
  symbol: string | null;
  side: string | null;
  openPrice: number | null;
  closePrice: number | null;
  openTime: string | null;
  closeTime: string | null;
  movePct: number | null;
  returnPct: number | null;
  trades: number | null;
  winRate: number | null;
  lots: number | null;
  profit: number | null;
  currency: string;
  from: string | null;
  to: string | null;
  referralCode: string | null;
  brand: string;
  /** "option" on an options share card (then `contracts` and `option` are set, `lots` is null). */
  instrument?: "option" | string;
  contracts?: number | null;
  option?: ShareOption;
}

export interface Share {
  code: string;
  kind: "trade" | "period";
  login: number;
  showAmounts: boolean;
  createdAt: string;
  views: number;
  url: string;
  data: ShareData;
}

/* ------------------------------------------------------------------ */
/* Fetch                                                               */
/* ------------------------------------------------------------------ */

export class GrowthApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public field?: string,
  ) {
    super(message);
  }
}

const FRIENDLY: Record<string, MessageKey> = {
  insufficient_points: "rewards.error.insufficientPoints",
  out_of_stock: "rewards.error.outOfStock",
  already_joined: "rewards.error.alreadyJoined",
  already_claimed: "rewards.error.alreadyClaimed",
  contest_closed: "rewards.error.contestClosed",
  limit_reached: "rewards.error.limitReached",
  options_intro_required: "rewards.error.optionsIntro",
};
/** Codes whose translated text is shown instead of the service's English message. */
const FRIENDLY_FIRST = new Set(["options_intro_required"]);

export async function growthApi<T>(path: string, init?: { method?: "GET" | "POST"; body?: unknown; signal?: AbortSignal }): Promise<T> {
  const method = init?.method ?? (init?.body !== undefined ? "POST" : "GET");
  const write = method !== "GET";
  let res: Response;
  try {
    res = await fetch(`/api/growth/${path}`, {
      method,
      headers: write ? { "content-type": "application/json" } : undefined,
      body: write ? JSON.stringify(init?.body ?? {}) : undefined,
      cache: "no-store",
      signal: init?.signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    throw new GrowthApiError(0, "network", tr("common.networkError"));
  }
  const data = (await res.json().catch(() => ({}))) as { error?: { code?: string; message?: string; field?: string } };
  if (!res.ok) {
    if (res.status === 401 && typeof window !== "undefined") {
      window.location.assign(`/api/auth/expired?next=${encodeURIComponent(window.location.pathname + window.location.search)}`);
    }
    const code = data.error?.code ?? "error";
    const msg =
      code === "unavailable" || res.status >= 500
        ? tr("rewards.error.unavailable")
        : FRIENDLY_FIRST.has(code) && FRIENDLY[code]
          ? tr(FRIENDLY[code])
          : (data.error?.message ?? (FRIENDLY[code] ? tr(FRIENDLY[code]) : tr("common.errorRetry")));
    throw new GrowthApiError(res.status, code, msg, data.error?.field);
  }
  return data as T;
}

export function errorToast(title: string, e: unknown) {
  toast.error(title, { description: e instanceof Error ? e.message : tr("common.errorRetry") });
}

/** Loads `path` once (and again on `reload()`); refreshes quietly every `ms` while the tab is visible.
 *  Opened again, a page starts from this tab's last answer while it refetches (@kalks/ui/swr-cache). */
export function useGrowth<T>(path: string | null, ms = 0) {
  const [data, setData] = React.useState<T | null>(() => (path !== null ? (readCached<T>(`growth:${path}`) ?? null) : null));
  const [error, setError] = React.useState<GrowthApiError | null>(null);
  const [tick, setTick] = React.useState(0);
  const reload = React.useCallback(() => setTick((t) => t + 1), []);

  React.useEffect(() => {
    if (path === null) return;
    const cached = readCached<T>(`growth:${path}`);
    if (cached !== undefined) setData(cached);
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const ctl = new AbortController();
    const run = async () => {
      if (stop) return;
      if (typeof document === "undefined" || document.visibilityState === "visible") {
        try {
          const d = await growthApi<T>(path, { signal: ctl.signal });
          if (stop) return;
          setData(d);
          writeCached(`growth:${path}`, d);
          setError(null);
        } catch (e) {
          if (stop || (e as Error).name === "AbortError") return;
          setError(e instanceof GrowthApiError ? e : new GrowthApiError(0, "error", tr("common.errorRetry")));
        }
      }
      if (!stop && ms > 0) timer = setTimeout(run, ms);
    };
    run();
    return () => {
      stop = true;
      ctl.abort();
      if (timer) clearTimeout(timer);
    };
  }, [path, ms, tick]);

  return { data, error, loading: data === null && error === null, reload, setData };
}

/* ------------------------------------------------------------------ */
/* Formatting                                                          */
/* ------------------------------------------------------------------ */

/** Intl tag for the reader's language (Latin digits). These helpers only run in the browser on loaded data. */
const tag = () => intlTag(tr.locale);
/** Short month name in the reader's language, e.g. "Sep". */
const mon = (m: number) => new Intl.DateTimeFormat(tag(), { month: "short", timeZone: "UTC" }).format(Date.UTC(2000, m, 1));

/** "28 Sep 2026" (local time). */
export function fmtDate(iso: string | null | undefined, withYear = true) {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return `${String(d.getDate()).padStart(2, "0")} ${mon(d.getMonth())}${withYear ? ` ${d.getFullYear()}` : ""}`;
}

/** "28 Sep, 14:03" (local time). */
export function fmtDateTime(iso: string | null | undefined) {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  return `${fmtDate(iso, false)}, ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** "28 Sep" for a "YYYY-MM-DD" day key. */
export function fmtDay(day: string) {
  const [, m, d] = day.split("-").map(Number);
  return m && d ? `${d} ${mon(m - 1)}` : day;
}

export const fmtPoints = (v: number) => Math.round(v).toLocaleString(tag());

/** Whole count with grouping in the reader's language, e.g. "1,234". */
export const fmtCount = (v: number) => v.toLocaleString(tag());

export function fmtUsd(v: number, digits = 2) {
  const s = Math.abs(v).toLocaleString(tag(), { minimumFractionDigits: digits, maximumFractionDigits: digits });
  return `${v < 0 ? "-" : ""}$${s}`;
}

/** 12.5 -> "12.5%", 12 -> "12%". */
export function fmtPct(v: number, signed = false) {
  const n = Number.isInteger(v) ? String(v) : String(+v.toFixed(2));
  return `${signed && v > 0 ? "+" : ""}${n}%`;
}

export const fmtLots = (v: number) => v.toLocaleString(tag(), { minimumFractionDigits: 2, maximumFractionDigits: 2 });

/** Option contracts: whole numbers without decimals, fractional steps as needed (an option contract is never a lot). */
export const fmtContracts = (v: number) => v.toLocaleString(tag(), { maximumFractionDigits: 4 });

/** The strike of an options share card as its series code writes it ("1.1650"), else the number. */
export function optionStrikeLabel(o: Pick<ShareOption, "series" | "strike">): string {
  const m = /^[A-Za-z0-9]+-\d{8}-(\d+(?:\.\d+)?)-[CPcp]$/.exec(o.series ?? "");
  if (m && o.strike !== null && Math.abs(Number(m[1]) - o.strike) < 1e-9) return m[1]!;
  if (o.strike === null || !Number.isFinite(o.strike)) return "";
  return String(+o.strike.toFixed(6));
}

/** A Kalks FX Options contest (O36): only option trades count, volume in contracts. */
export const isOptionsContest = (c: Pick<Contest, "instrument">) => c.instrument === "options";

/** Groups that never trade options (copy-trading followers, PAMM, MAM, prop), same rule as the engine. */
export function optionsSystemGroup(code: string) {
  const g = code.trim().toLowerCase();
  return g.startsWith("prop") || g === "copy" || g.startsWith("copy-") || g === "pamm" || g.startsWith("pamm-") || g === "mam" || g.startsWith("mam-");
}

export const titleCase = (s: string) => s.replace(/_/g, " ").replace(/^\w/, (c) => c.toUpperCase());

/** "#1", "#2–5" for a prize band. */
export const bandLabel = (p: Pick<Prize, "rankFrom" | "rankTo">) => (p.rankFrom === p.rankTo ? `#${p.rankFrom}` : `#${p.rankFrom}–${p.rankTo}`);

/** Prize for a rank from the contest's bands (null outside the prize zone). */
export function prizeFor(c: Pick<Contest, "prizes">, rank: number | null) {
  if (!rank) return null;
  return c.prizes.find((p) => rank >= p.rankFrom && rank <= p.rankTo)?.amount ?? null;
}

/** Last rank that wins a prize. */
export const prizeZone = (c: Pick<Contest, "prizes">) => c.prizes.reduce((m, p) => Math.max(m, p.rankTo), 0);

/**
 * The prize a standing wins or is on track for: the service's own once the contest is finalized, else its rank's
 * band, but only for a qualified entry that isn't disqualified (services/growth allocate_prizes: an entry under the
 * minimum trades, or disqualified, wins nothing, and its rank pays nobody).
 */
export function projectedPrize(c: Pick<Contest, "prizes">, s: Pick<Standing, "prize" | "rank" | "qualified" | "status">): number | null {
  if (s.prize !== null && s.prize !== undefined) return s.prize;
  if (!s.qualified || s.status === "disqualified") return null;
  return prizeFor(c, s.rank);
}

export const SCORING_LABEL: Record<string, MessageKey> = { return_pct: "rewards.scoring.returnPct", profit: "rewards.scoring.profit", lots: "rewards.scoring.lots", contracts: "rewards.scoring.contracts" };
export const scoringLabel = (s: string) => (SCORING_LABEL[s] ? tr(SCORING_LABEL[s]) : titleCase(s));

/** Public origin for share links: NEXT_PUBLIC_APP_URL, else this page's origin. */
export function linkBase(): string {
  const env = process.env.NEXT_PUBLIC_APP_URL;
  if (env) return env.replace(/\/+$/, "");
  return typeof window === "undefined" ? "" : window.location.origin;
}

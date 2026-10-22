/**
 * Growth service (services/growth) — types and fetch wrappers for the live Marketing pages.
 * Browser → /api/marketing/<path> (BFF, app/api/marketing/[...path]) → growth service /v1/growth/admin/<path>.
 * Shapes follow services/growth/README.md (camelCase JSON, money in USD as numbers, RFC 3339 times).
 */
import type { ApiErr } from "@/components/live/kit";

export const M = (path: string) => `/api/marketing/${path}`;

export type Perms = { read: boolean; write: boolean; approve: boolean };
export type Paged<T> = { items: T[]; total: number; page?: number; limit?: number };

/* ------------------------------------------------------------------ */
/* Overview                                                             */
/* ------------------------------------------------------------------ */

export type Overview = {
  loyalty: { members: number; pointsIssued30d: number; pointsRedeemed30d: number; liabilityPoints: number; liabilityUsd: number };
  cashback: { accrued: number; paid30d: number };
  bonus: { activeCampaigns: number; activeGrants: number; issued30d: number; released30d: number; forfeited30d: number; outstanding: number };
  contests: { running: number; scheduled: number; entrants: number };
  promos: { active: number; redemptions30d: number; blocked30d: number };
  banners: { active: number; impressions30d: number; clicks30d: number };
};

/* ------------------------------------------------------------------ */
/* Bonuses                                                              */
/* ------------------------------------------------------------------ */

export type CampaignKind = "deposit" | "fixed";
export type CampaignStatus = "draft" | "active" | "paused" | "ended";

export type CampaignInput = {
  name: string;
  description: string;
  terms: string;
  kind: CampaignKind;
  pct: number;
  cap: number;
  fixedAmount: number;
  minDeposit: number;
  releasePerLot: number;
  expiryDays: number;
  forfeitOnWithdrawal: boolean;
  claimWindowDays: number;
  accountGroups: string[];
  maxClaims: number | null;
  perUserLimit: number;
  newUsersDays: number | null;
  kycRequired: boolean;
  visibility: "public" | "code_only";
  status: CampaignStatus;
  startsAt?: string;
  endsAt?: string | null;
};

export type Campaign = CampaignInput & {
  id: number;
  startsAt: string;
  endsAt: string | null;
  claims: number;
  active: number;
  issued: number;
  released: number;
  forfeited: number;
  outstanding: number;
};

export type GrantStatus = "awaiting_deposit" | "pending" | "active" | "completed" | "forfeited" | "expired" | "cancelled" | "failed";

export type Grant = {
  id: number;
  campaignId: number;
  campaign: string;
  status: GrantStatus;
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
  userId: number;
  name: string;
};

/* ------------------------------------------------------------------ */
/* Promo codes                                                          */
/* ------------------------------------------------------------------ */

export type PromoKind = "bonus" | "points" | "discount";
export type AppliesTo = "any" | "prop" | "commission";

export type PromoInput = {
  code: string;
  description: string;
  kind: PromoKind;
  campaignId: number | null;
  points: number | null;
  discountPct: number | null;
  discountAppliesTo: AppliesTo;
  maxUses: number | null;
  perUserLimit: number;
  newUsersDays: number | null;
  countries: string[];
  kycRequired: boolean;
  active: boolean;
  startsAt?: string;
  endsAt?: string | null;
};

export type Promo = PromoInput & { id: number; startsAt: string; endsAt: string | null; uses: number; blocked: number; lastUsedAt: string | null };

export type PromoUse = { id: number; code: string; kind: string; status: "applied" | "blocked"; reason: string | null; createdAt: string; userId: number; name: string; promoId: number | null };

/* ------------------------------------------------------------------ */
/* Banners                                                              */
/* ------------------------------------------------------------------ */

export type Placement = "dashboard" | "wallet" | "rewards" | "terminal";
export type Tone = "ember" | "gold" | "neutral" | "up";
export type Kyc = "unverified" | "pending" | "verified" | "rejected";
export type AccountType = "live" | "demo" | "none";

export type BannerInput = {
  title: string;
  body: string;
  ctaLabel: string | null;
  ctaUrl: string | null;
  imageUrl: string | null;
  tone: Tone;
  placement: Placement;
  countries: string[];
  kyc: Kyc[];
  accountTypes: AccountType[];
  newUsersDays: number | null;
  priority: number;
  dismissible: boolean;
  active: boolean;
  startsAt?: string;
  endsAt?: string | null;
};

export type Banner = BannerInput & { id: number; startsAt: string; endsAt: string | null; impressions: number; clicks: number; dismissals: number; ctr: number };

export type BannerView = { id: number; title: string; body: string; ctaLabel: string | null; ctaUrl: string | null; imageUrl: string | null; tone: Tone; placement: Placement; dismissible: boolean };

/* ------------------------------------------------------------------ */
/* Contests                                                             */
/* ------------------------------------------------------------------ */

export type ContestStatus = "draft" | "scheduled" | "running" | "ended" | "finalized" | "paid" | "cancelled";
/** Volume scoring is `lots` in a CFD contest and `contracts` in an options contest (O36). */
export type Scoring = "return_pct" | "profit" | "lots" | "contracts";
/** What the contest is traded on: CFDs (default) or Kalks FX Options. */
export type ContestInstrument = "cfd" | "options";
export type Prize = { rankFrom: number; rankTo: number; amount: number; payout: "wallet" | "credit" };
export type AntiCheat = { minHoldSeconds: number; maxSingleTradePct: number; disqualifyOnBalanceChange: boolean };

export type ContestInput = {
  slug?: string;
  name: string;
  description: string;
  kind: "demo" | "live";
  instrument: ContestInstrument;
  startsAt: string;
  endsAt: string;
  scoring: Scoring;
  minTrades: number;
  maxEntrants: number | null;
  startingBalance: number | null;
  demoGroup: string | null;
  accountGroups: string[];
  kycRequired: boolean;
  minEquity: number | null;
  prizes: Prize[];
  rules: string;
  antiCheat: AntiCheat;
  /** Options contests: minimum opening premium per trade (USD) for volume and trade count; null = none. */
  minPremium: number | null;
  status: "draft" | "scheduled";
  /** Phase 4: CFD asset classes that count (empty = all), the template it came from, the contest it was cloned from. */
  assetClasses?: string[];
  templateId?: number | null;
  clonedFrom?: number | null;
};

export type Contest = Omit<ContestInput, "status" | "slug"> & { id: number; slug: string; status: ContestStatus; entrants: number; prizePool: number; updatedAt: string };

/** Four-eyes (maker-checker) state of a payout: at or above `threshold` two different staff members approve it. */
export type FourEyes = {
  required: boolean;
  threshold: number | null;
  /** Staff id (as `useStaff().id`) of the first approver. */
  firstApprovedById: string | null;
  firstApprovedBy: string | null;
  firstApprovedAt: string | null;
  firstApprovalNote: string | null;
};

export type Standing = {
  entryId: number;
  userId: number | null;
  name: string;
  country: string;
  login: number | null;
  rank: number | null;
  score: number;
  returnPct: number;
  profit: number;
  lots: number;
  /** Options contests: contracts of the trades that count. */
  contracts?: number;
  trades: number;
  /** Options contests: trades left out as self-trades (no P&L, volume or count) and below the minimum premium (P&L only). */
  selfTrades?: number | null;
  smallTrades?: number | null;
  qualified: boolean;
  status: "active" | "disqualified";
  prize: number | null;
  prizePayout?: "wallet" | "credit" | null;
  /** none | pending | approval | awaiting_second_approval | paid | failed | rejected */
  prizeStatus: string;
  /** Staff views only. */
  fourEyes?: FourEyes | null;
  updatedAt: string;
  flags?: string[];
  disqualifyReason?: string | null;
};

/** Anti-cheat flag (contest_flags). Name / login may be absent; the leaderboard row is looked up by entryId. */
export type ContestFlag = {
  id: number;
  entryId: number;
  kind: "balance_change" | "single_trade" | "short_holds" | "self_trade" | string;
  severity?: string;
  details?: Record<string, unknown>;
  status: "open" | "cleared" | "disqualified";
  createdAt: string;
  resolvedBy?: string | null;
  resolvedAt?: string | null;
  note?: string | null;
  name?: string;
  login?: number | null;
};

export type ContestDetail = { contest: Contest; leaderboard: Standing[]; flags: ContestFlag[]; fourEyesThreshold?: number };
/** POST contests/{id}/pay */
export type PayResult = { paid: number; failed: number; amount: number; firstApproved?: number; skippedSameStaff?: number; awaitingSecond?: number; fourEyesThreshold?: number };

/* ------------------------------------------------------------------ */
/* Loyalty                                                              */
/* ------------------------------------------------------------------ */

export type Settings = {
  pointValue: number;
  minHoldSeconds: number;
  pointsExpiryMonths: number;
  cashbackHoldHours: number;
  demoPoints?: boolean;
  // challenges (phase 4)
  autoPayBelow?: number;
  /** Four-eyes: payouts at or above this USD amount need two different staff members (0 = every payout). */
  fourEyesThreshold?: number;
  autoFinalize?: boolean;
  autoFinalizeDelayMinutes?: number;
  publicLeaderboards?: boolean;
  nicknamePublicDefault?: boolean;
  streakGraceDays?: number;
  missionMinLots?: number;
  missionMinHoldSeconds?: number;
  xpContestJoin?: number;
  xpStreakWeek?: number;
};
export type Tier = { key: string; name: string; rank: number; minPoints: number; multiplier: number; perks: string[] };
export type EarnRule = { id: number; name: string; assetClass: string | null; symbols: string[]; accountGroups: string[]; accountType: "live" | "demo" | "any"; pointsPerLot: number; active: boolean; priority: number };
export type CatalogueKind = "cashback" | "bonus_credit" | "fee_discount";
export type CatalogueItem = {
  id: number;
  name: string;
  description: string;
  kind: CatalogueKind;
  costPoints: number;
  value: number;
  minTier: string | null;
  stock: number | null;
  active: boolean;
  params: { releasePerLot?: number; expiryDays?: number; appliesTo?: AppliesTo; validDays?: number } & Record<string, unknown>;
};
export type Redemption = {
  id: number;
  itemId: number;
  itemName: string;
  kind: CatalogueKind;
  points: number;
  value: number;
  /** approval / awaiting_second_approval: a cashback redemption held for staff (four-eyes); rejected: points returned */
  status: "approval" | "awaiting_second_approval" | "pending" | "completed" | "failed" | "rejected";
  login: number | null;
  voucherCode: string | null;
  decidedBy?: string | null;
  fourEyes?: FourEyes;
  grantId: number | null;
  error: string | null;
  createdAt: string;
  completedAt: string | null;
  userId: number;
  name: string;
};
export type Member = { userId: number; name: string; email: string; country: string; balance: number; lifetime: number; earned12m: number; tier: string | { key: string; name: string } | null; lastEarnAt: string | null };

/* ------------------------------------------------------------------ */
/* Cashback                                                             */
/* ------------------------------------------------------------------ */

export type ProgrammeInput = {
  name: string;
  description: string;
  assetClasses: string[];
  symbols: string[];
  accountGroups: string[];
  usdPerLot: number;
  maxPerMonth: number | null;
  optIn: boolean;
  active: boolean;
  startsAt?: string;
  endsAt?: string | null;
};
export type Programme = ProgrammeInput & { id: number; startsAt: string; endsAt: string | null; enrolled: number; accrued: number; paid: number; lots30d: number };
export type Accrual = { id: number; programmeId: number; programme?: string; userId?: number; name?: string; dealId: number; login: number; symbol: string; lots: number; amount: number; status: "accrued" | "paid" | "void"; createdAt: string };
export type CashbackPayout = {
  id: number;
  userId: number;
  name: string;
  amount: number;
  /** approval / awaiting_second_approval: held at or above the four-eyes threshold; rejected: accruals rejected too */
  status: "approval" | "awaiting_second_approval" | "pending" | "paid" | "failed" | "rejected";
  attempts: number;
  error: string | null;
  createdAt: string;
  paidAt: string | null;
  decidedBy?: string | null;
  decidedAt?: string | null;
  rejectReason?: string | null;
  fourEyes?: FourEyes;
};

/* ------------------------------------------------------------------ */
/* Reports                                                              */
/* ------------------------------------------------------------------ */

export type Report = {
  from: string;
  to: string;
  totals: {
    bonusIssued: number;
    bonusReleased: number;
    bonusForfeited: number;
    cashbackPaid: number;
    cashbackAccrued: number;
    prizesPaid: number;
    pointsRedeemedUsd: number;
    redemptionCashPaid: number;
    promoRedemptions: number;
    total: number;
  };
  byCampaign: { id: number; name: string; issued: number; released: number; forfeited: number; claims: number }[];
  byProgramme: { id: number; name: string; accrued: number; paid: number; lots: number }[];
  byContest: { id: number; name: string; prizes: number; entrants: number }[];
  series: { day: string; bonus: number; cashback: number; prizes: number; points: number }[];
};

/* ------------------------------------------------------------------ */
/* Writes                                                               */
/* ------------------------------------------------------------------ */

export type WriteResult<T> = { ok: true; data: T } | { ok: false; error: ApiErr };

function expired() {
  const next = window.location.pathname + window.location.search;
  window.location.assign(`/api/auth/expired?next=${encodeURIComponent(next)}`);
}

/** POST / PUT / PATCH JSON to /api/marketing/<path>. Service validation errors come back as {code, message, field}. */
export async function mkSend<T = unknown>(path: string, body: object, method: "POST" | "PUT" | "PATCH" = "POST"): Promise<WriteResult<T>> {
  try {
    const r = await fetch(M(path), { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body), credentials: "same-origin" });
    const data = await r.json().catch(() => ({}));
    if (r.status === 401) {
      expired();
      return { ok: false, error: { code: "unauthorized", message: "Your session has ended." } };
    }
    if (r.ok) return { ok: true, data: data as T };
    const e = (data as { error?: ApiErr })?.error;
    return { ok: false, error: e && e.message ? e : { code: String(r.status), message: r.status === 503 ? "The marketing service isn't reachable right now." : "Something went wrong." } };
  } catch {
    return { ok: false, error: { code: "network", message: "Can't reach the Back Office server." } };
  }
}

/** Human label for a service validation field. */
export const FIELD_LABEL: Record<string, string> = {
  name: "Name",
  code: "Code",
  pct: "Percent",
  cap: "Cap",
  fixedAmount: "Fixed amount",
  minDeposit: "Minimum deposit",
  releasePerLot: "Release per lot",
  expiryDays: "Expiry days",
  claimWindowDays: "Claim window",
  maxClaims: "Max claims",
  perUserLimit: "Per-user limit",
  newUsersDays: "New users within",
  campaignId: "Campaign",
  points: "Points",
  discountPct: "Discount",
  maxUses: "Max uses",
  countries: "Countries",
  title: "Title",
  body: "Body",
  placement: "Placement",
  priority: "Priority",
  ctaUrl: "CTA link",
  imageUrl: "Image URL",
  startsAt: "Starts",
  endsAt: "Ends",
  startingBalance: "Starting balance",
  minTrades: "Min trades",
  maxEntrants: "Max entrants",
  prizes: "Prizes",
  demoGroup: "Demo group",
  minEquity: "Minimum equity",
  instrument: "Instrument",
  scoring: "Scoring",
  minPremium: "Minimum premium",
  accountGroups: "Account groups",
  tiers: "Tiers",
  pointsPerLot: "Points per lot",
  costPoints: "Cost",
  value: "Value",
  usdPerLot: "USD per lot",
  maxPerMonth: "Monthly cap",
  pointValue: "Point value",
  minHoldSeconds: "Minimum hold",
  pointsExpiryMonths: "Points expiry",
  cashbackHoldHours: "Cashback hold",
  fourEyesThreshold: "Four-eyes threshold",
  autoPayBelow: "Auto-pay threshold",
  userId: "Client ID",
  login: "Account",
  amount: "Amount",
  note: "Note",
  reason: "Reason",
};

export function errText(e: ApiErr) {
  const label = e.field ? FIELD_LABEL[e.field] ?? e.field : undefined;
  return label && !e.message.toLowerCase().includes(label.toLowerCase()) ? `${label}: ${e.message}` : e.message;
}

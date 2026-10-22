/**
 * IB programme (services/ib) — types and fetch wrappers for the live Back Office pages.
 * Browser → /api/partners/<path> (BFF, app/api/partners/[...path]) → IB service /v1/ib/admin/<path>.
 * Shapes follow services/ib/src/api/admin.rs and services/ib/src/model.rs (camelCase JSON).
 */
import type { ApiErr } from "@/components/live/kit";

export const P = (path: string) => `/api/partners/${path}`;

/* ------------------------------------------------------------------ */
/* Permissions                                                          */
/* ------------------------------------------------------------------ */

export type Perms = { read: boolean; write: boolean; approve: boolean };

/* ------------------------------------------------------------------ */
/* Settings & levels                                                    */
/* ------------------------------------------------------------------ */

export type SelfRefAction = "block" | "flag" | "off";
export type Schedule = "daily" | "weekly" | "monthly";

export type SymbolGroup = { key: string; name: string; assetClass: string | null; symbols: string[] };

/** Programme settings with every decimal as a number (the service sends decimals as strings). */
export type Settings = {
  tiers: number[];
  minTradeSeconds: number;
  centLotFactor: number;
  excludedGroups: string[];
  symbolGroups: SymbolGroup[];
  cpa: { enabled: boolean; minFirstDeposit: number; requireFirstTrade: boolean; holdDays: number };
  payout: { schedule: Schedule; weekday: number; monthDay: number; minAmount: number; autoCreate: boolean };
  maxRebatePct: number;
  maxSplitPct: number;
  clientVisibility: "full" | "masked";
  selfReferral: { ip: SelfRefAction; device: SelfRefAction; identity: SelfRefAction };
  wash: { enabled: boolean; windowSecs: number; volumeTolerancePct: number; shortTradesMin: number; shortTradesPct: number };
  allowDemotion: boolean;
  linkBase: string;
  /** Four-eyes (USD): batches at or above this need a second approver. 0 = every batch. */
  fourEyesThreshold: number;
};

export type SettingsDoc = { settings: Settings; version: number; updatedAt: string; updatedBy: string | null; nextPayoutClose: string };

const n = (v: unknown) => (v === null || v === undefined || v === "" ? 0 : Number(v));

/** Normalises the service's settings JSON (decimal strings → numbers). */
export function toSettings(raw: Record<string, unknown>): Settings {
  const r = raw as unknown as Settings & Record<string, unknown>;
  return {
    ...r,
    tiers: ((raw.tiers as unknown[]) ?? []).map(n),
    minTradeSeconds: n(r.minTradeSeconds),
    centLotFactor: n(r.centLotFactor),
    excludedGroups: r.excludedGroups ?? [],
    symbolGroups: (r.symbolGroups ?? []).map((g) => ({ key: g.key, name: g.name, assetClass: g.assetClass ?? null, symbols: g.symbols ?? [] })),
    cpa: { enabled: !!r.cpa?.enabled, minFirstDeposit: n(r.cpa?.minFirstDeposit), requireFirstTrade: !!r.cpa?.requireFirstTrade, holdDays: n(r.cpa?.holdDays) },
    payout: { schedule: r.payout?.schedule ?? "weekly", weekday: n(r.payout?.weekday) || 1, monthDay: n(r.payout?.monthDay) || 1, minAmount: n(r.payout?.minAmount), autoCreate: !!r.payout?.autoCreate },
    maxRebatePct: n(r.maxRebatePct),
    maxSplitPct: n(r.maxSplitPct),
    clientVisibility: r.clientVisibility === "masked" ? "masked" : "full",
    selfReferral: { ip: r.selfReferral?.ip ?? "block", device: r.selfReferral?.device ?? "block", identity: r.selfReferral?.identity ?? "block" },
    wash: {
      enabled: !!r.wash?.enabled,
      windowSecs: n(r.wash?.windowSecs),
      volumeTolerancePct: n(r.wash?.volumeTolerancePct),
      shortTradesMin: n(r.wash?.shortTradesMin),
      shortTradesPct: n(r.wash?.shortTradesPct),
    },
    allowDemotion: !!r.allowDemotion,
    linkBase: r.linkBase ?? "",
    fourEyesThreshold: r.fourEyesThreshold === undefined || r.fourEyesThreshold === null ? 1000 : n(r.fourEyesThreshold),
  };
}

export type Level = {
  key: string;
  name: string;
  rank: number;
  icon: string;
  perks: string[];
  minActiveClients: number;
  minMonthlyLots: number;
  cpaAmount: number;
  rates: Record<string, number>;
  /** Kalks FX Options: USD per option contract (round turn, paid on the closing deal). Absent from a service that predates it. */
  optionsRate?: number;
  members?: number;
};

/** Highest options rate the service accepts (USD per contract). */
export const OPTIONS_RATE_MAX = 1000;

export type LevelsDoc = { levels: Level[]; symbolGroups: SymbolGroup[] };

/** Level as the service accepts it on PUT /levels (no member count). A level without `optionsRate` leaves it out,
 *  so the service keeps the level's current options rate. */
export function levelBody(l: Level) {
  return {
    key: l.key,
    name: l.name.trim(),
    rank: l.rank,
    icon: l.icon,
    perks: l.perks.map((p) => p.trim()).filter(Boolean),
    minActiveClients: Math.round(l.minActiveClients),
    minMonthlyLots: l.minMonthlyLots,
    cpaAmount: l.cpaAmount,
    rates: l.rates,
    ...(l.optionsRate === undefined ? {} : { optionsRate: l.optionsRate }),
  };
}

/* ------------------------------------------------------------------ */
/* Overview                                                             */
/* ------------------------------------------------------------------ */

export type MonthRow = { month: string; tier1: number; tier2: number; tier3: number; split: number; rebate: number; cpa: number; clawback: number; lots: number };

export type Overview = {
  kpis: {
    members: number;
    referredClients: number;
    ibsWithClients: number;
    earningIbs: number;
    commissionMonth: number;
    commissionPrevMonth: number;
    pending: number;
    approved: number;
    paid: number;
    lotsMonth: number;
    openFlags: number;
    pendingBatches: number;
    transfersInFlight: number;
  };
  months: MonthRow[];
  funnel: { clicks: number; signups: number; kyc: number; ftds: number; traders: number };
  levels: { key: string; name: string; rank: number; members: number }[];
  top: { id: number; name: string; country: string; level: string; commissionMonth: number; clients: number }[];
};

/* ------------------------------------------------------------------ */
/* Partners                                                             */
/* ------------------------------------------------------------------ */

export type PartnerRow = {
  id: number;
  name: string;
  email: string;
  country: string;
  code: string;
  level: string;
  levelLocked: boolean;
  status: "active" | "suspended";
  parentId: number | null;
  parentName: string | null;
  parentSource: "signup" | "admin";
  clients: number;
  activeClients: number;
  commissionMonth: number;
  pending: number;
  paid: number;
  openFlags: number;
  rebatePct: number;
  splitPct: number;
  selfReferral: string | null;
  joinedAt: string;
  kycStatus: string;
  firstDepositAt: string | null;
  firstDepositAmount: number | null;
  // detail only
  lotsMonth?: number;
  lotsPrevMonth?: number;
  campaign?: string | null;
};

export type Paged<T> = { items: T[]; page: number; limit: number; total: number };

export type TreeNode = { id: number; parentId: number | null; tier: number; name: string; country: string; level: string; joinedAt: string; funded: boolean; selfReferral: string | null; lotsMonth: number };

export type FlagMin = { id: number; kind: FlagKind; severity: "low" | "medium" | "high"; status: "open" | "confirmed" | "dismissed"; clientId: number | null; ibId: number | null; details: Record<string, unknown>; createdAt: string };

export type PartnerDetail = {
  partner: PartnerRow;
  upline: { id: number; name: string; level: string }[];
  tree: TreeNode[];
  commissions: { id: number; kind: CommKind; status: CommStatus; amount: number; tier: number; symbol: string | null; lots: number; contracts?: number; symbolGroup?: string | null; dealId: number | null; clientId: number; clientName: string; createdAt: string }[];
  payouts: { id: number; batchId: number; amount: number; status: PayoutStatus; paidAt: string | null; createdAt: string; lastError: string | null }[];
  flags: FlagMin[];
  reassignments: { fromParent: number | null; toParent: number | null; fromName?: string | null; toName?: string | null; staff: string; reason: string; at: string }[];
  levelHistory: { from: string | null; to: string; reason: string; month: string | null; activeClients: number | null; lots: number | null; at: string }[];
};

/* ------------------------------------------------------------------ */
/* Commissions                                                          */
/* ------------------------------------------------------------------ */

export type CommKind = "lot" | "split" | "rebate" | "cpa" | "clawback" | "adjustment";
export type CommStatus = "pending" | "approved" | "paid" | "rejected" | "void";

export type Commission = {
  id: number;
  kind: CommKind;
  status: CommStatus;
  amount: number;
  tier: number;
  rate: number;
  sharePct: number;
  /** 0 on option lines (options are paid per contract and never count as lots) */
  lots: number;
  /** option contracts on an option line (0 on CFD lines); `rate` is then USD per contract */
  contracts?: number;
  symbol: string | null;
  /** "options" on option lines */
  symbolGroup: string | null;
  levelKey: string | null;
  dealId: number | null;
  source: string | null;
  login: number | null;
  beneficiary: { id: number; name: string };
  client: { id: number; name: string };
  batchId: number | null;
  note: string | null;
  createdAt: string;
  availableAt: string;
};

export type CommissionsDoc = Paged<Commission> & { sum: number };

/* ------------------------------------------------------------------ */
/* Batches & payouts                                                    */
/* ------------------------------------------------------------------ */

export type BatchStatus = "pending_approval" | "awaiting_second_approval" | "approved" | "paid" | "partially_paid" | "rejected";

/** Four-eyes state of a batch: whether it needs two approvers and who gave the first approval. */
export type FourEyes = {
  required: boolean;
  threshold: number;
  /** Staff id (as the gateway sends it) of the first approver. */
  firstApprovedById: string | null;
  firstApprovedBy: string | null;
  firstApprovedAt: string | null;
  firstApprovalNote: string | null;
};
export type PayoutStatus = "pending_approval" | "transfer_pending" | "paid" | "rejected" | "failed";

export type Batch = {
  id: number;
  schedule: string;
  periodStart: string | null;
  periodEnd: string;
  status: BatchStatus;
  total: number;
  lines: number;
  payees: number;
  createdBy: string;
  createdAt: string;
  decidedBy: string | null;
  decidedAt: string | null;
  decisionNote: string | null;
  completedAt: string | null;
  decidedById?: string | null;
  fourEyes?: FourEyes;
  transfers?: { paid: number; pending: number; failed: number };
};

export type BatchesDoc = Paged<Batch> & { unbatched: { amount: number; payees: number }; schedule: Schedule; minAmount: number; nextClose: string; fourEyesThreshold?: number };

export type Payout = {
  id: number;
  userId: number;
  name: string;
  email: string | null;
  level: string | null;
  amount: number;
  lines: number;
  status: PayoutStatus;
  attempts: number;
  lastError: string | null;
  nextAttemptAt: string | null;
  walletTxn: string | null;
  paidAt: string | null;
  openFlags: number;
  breakdown: { lots: number; cpa: number; rebates: number; adjustments: number };
};

export type BatchDetail = { batch: Batch; payouts: Payout[] };

/* ------------------------------------------------------------------ */
/* Fraud flags                                                          */
/* ------------------------------------------------------------------ */

export type FlagKind = "self_referral_ip" | "self_referral_device" | "self_referral_identity" | "wash_trading" | "short_trades";

export type Flag = FlagMin & {
  client: { id: number | null; name: string; email: string | null };
  ib: { id: number | null; name: string; code: string | null };
  resolvedBy: string | null;
  resolvedAt: string | null;
  note: string | null;
};

export type FlagsDoc = Paged<Flag> & { counts: Partial<Record<"open" | "confirmed" | "dismissed", number>> };

/* ------------------------------------------------------------------ */
/* Writes                                                               */
/* ------------------------------------------------------------------ */

export type WriteResult<T> = { ok: true; data: T } | { ok: false; error: ApiErr };

function expired() {
  const next = window.location.pathname + window.location.search;
  window.location.assign(`/api/auth/expired?next=${encodeURIComponent(next)}`);
}

/** POST / PUT / PATCH JSON to /api/partners/<path>. Service validation errors come back as {code, message, field}. */
export async function ibSend<T = unknown>(path: string, body: Record<string, unknown>, method: "POST" | "PUT" | "PATCH" = "POST"): Promise<WriteResult<T>> {
  try {
    const r = await fetch(P(path), { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body), credentials: "same-origin" });
    const data = await r.json().catch(() => ({}));
    if (r.status === 401) {
      expired();
      return { ok: false, error: { code: "unauthorized", message: "Your session has ended." } };
    }
    if (r.ok) return { ok: true, data: data as T };
    const e = (data as { error?: ApiErr })?.error;
    return { ok: false, error: e && e.message ? e : { code: String(r.status), message: r.status === 503 ? "The partner service isn't reachable right now." : "Something went wrong." } };
  } catch {
    return { ok: false, error: { code: "network", message: "Can't reach the Back Office server." } };
  }
}

/** Human label for a service validation field. */
export const FIELD_LABEL: Record<string, string> = {
  reason: "Reason",
  tiers: "Tiers",
  minTradeSeconds: "Minimum trade duration",
  centLotFactor: "Cent lot factor",
  symbolGroups: "Symbol groups",
  cpa: "CPA rules",
  payout: "Payout schedule",
  "payout.schedule": "Payout schedule",
  "payout.minAmount": "Minimum payout",
  maxRebatePct: "Max client rebate",
  maxSplitPct: "Max sub-IB split",
  clientVisibility: "Client visibility",
  selfReferral: "Self-referral",
  wash: "Wash trading",
  linkBase: "Link base",
  fourEyesThreshold: "Four-eyes threshold",
  levels: "Levels",
  key: "Level key",
  rank: "Rank",
  name: "Name",
  targets: "Targets",
  rates: "Rates",
  optionsRate: "Options rate",
  level: "Level",
  status: "Status",
  rebatePct: "Client rebate",
  splitPct: "Sub-IB split",
  parentId: "New upline",
  action: "Action",
};

export function errText(e: ApiErr) {
  const label = e.field ? FIELD_LABEL[e.field] : undefined;
  return label && !e.message.toLowerCase().includes(label.toLowerCase()) ? `${label}: ${e.message}` : e.message;
}

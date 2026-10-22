"use client";

/**
 * Live-build building blocks for the Prop Firm pages. Everything goes through the Back Office BFF /api/prop/*
 * (app/api/prop/[...path]) to the prop service's staff API (/v1/admin/*). Types follow the service's JSON
 * (services/prop/README.md, "API").
 */
import * as React from "react";
import { AlertTriangle, ShieldCheck } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, Chip, Dialog, DialogClose, cn, formatNumber, type ChipTone } from "@kalks/ui";
import { IS_DEMO } from "@kalks/mock/mode";
import { ErrorState, ago, when, type ApiErr } from "@/components/live/kit";
import { MiniField, Select } from "@/components/config/kit";
import { useStaff } from "@/components/staff-session";
import { propAllows, type PropPerm } from "@/lib/prop-perms";

/* ------------------------------------------------------------------ */
/* Service shapes                                                       */
/* ------------------------------------------------------------------ */

export type PlanType = "1-step" | "2-step" | "instant";
export type PlanStatus = "draft" | "active" | "paused" | "archived";
export type PayoutFreq = "weekly" | "bi-weekly" | "monthly" | "on-demand";
export type PlanSize = { size: number; fee: number; leverage: number; enabled: boolean };
export type PlanPhase = { name: string; target: number; minDays: number; timeLimit: number };

export type Plan = {
  id: string;
  name: string;
  type: PlanType;
  status: PlanStatus;
  version: number;
  group: string;
  sizes: PlanSize[];
  phases: PlanPhase[];
  dailyLoss: number;
  dailyBasis: "balance" | "equity";
  maxDD: number;
  ddType: "static" | "trailing";
  trailingLock: boolean;
  consistency: number;
  newsTrading: boolean;
  newsWindow: number;
  newsBreachFails: boolean;
  weekendHolding: boolean;
  eaAllowed: boolean;
  banned: string[];
  split: number;
  splitMax: number;
  scalingEvery: number;
  scalingIncrease: number;
  scalingProfit: number;
  scalingCap: number;
  refundFee: boolean;
  payoutFreq: PayoutFreq;
  firstPayoutDays: number;
  minPayout: number;
  updatedAt?: string | null;
  updatedBy?: string | null;
};
export type PlanStats = { active: number; sold30d: number; revenue30d: number; passRate: number | null };
export type PlanRow = Plan & { stats?: PlanStats };

export type EngineGroup = { code: string; name: string; leverages: number[]; enabled: boolean; maxAccountsPerUser: number };

export type Verdict = { kind: "ok" | "pass" | "breach"; rule?: string; message?: string; threshold?: number };
export type LiveRules = {
  day: string;
  dailyLimit: number;
  dailyRef: number;
  dailyFloor: number;
  dailyUsed: number;
  ddLimit: number;
  ddFloor: number;
  ddUsed: number;
  hwm: number;
  profit: number;
  targetAmount: number | null;
  targetReached: boolean;
  tradingDays: number;
  minDays: number;
  daysOk: boolean;
  bestDay: number | null;
  consistencyLimit: number | null;
  consistencyOk: boolean;
  deadline: string | null;
  verdict: Verdict;
  warn: number | string | null;
  nextReset: string;
  weekendWindow: boolean;
  equity: number;
  balance: number;
  at: string;
};
export type PhaseStats = {
  trades: number;
  open: number;
  wins: number;
  losses: number;
  winRate: number | null;
  avgWin: number;
  avgLoss: number;
  profitFactor: number | null;
  lots: number;
  bestDay: { day: string; profit: number } | null;
  days: string[];
  dayProfits: { day: string; profit: number }[];
};
export type PhaseAccount = {
  id: number;
  challengeId: number;
  phaseIndex: number;
  phase: string;
  funded: boolean;
  login: number | null;
  status: "provisioning" | "active" | "passed" | "failed" | "closed";
  initialBalance: number;
  targetPct: number | null;
  minDays: number;
  timeLimitDays: number;
  startedAt: string | null;
  endedAt: string | null;
  endReason: string | null;
  balance: number | null;
  equity: number | null;
  openPositions: number | null;
  tradingDays: number;
  lastEvalAt: string | null;
  lastPayoutAt: string | null;
  scaledAt: string | null;
  rules: Partial<LiveRules> | null;
  stats: Partial<PhaseStats> | null;
};
export type ChallengeStatus = "pending_payment" | "provisioning" | "active" | "funded" | "failed" | "closed" | "payment_failed";
export type Challenge = {
  id: number;
  userId: number;
  traderName: string;
  planId: string;
  planName: string;
  type: PlanType;
  size: number;
  fee: number;
  leverage: number;
  group: string;
  status: ChallengeStatus;
  phaseIndex: number;
  feeRefunded: boolean;
  split: number;
  failureReason: string | null;
  createdAt: string;
  plan?: Plan;
  phases?: PhaseAccount[];
  current?: PhaseAccount | null;
  flags?: number;
};
export type PayoutQuote = {
  eligibleFrom: string | null;
  profit: number;
  split: number;
  traderAmount: number;
  firmAmount: number;
  feeRefund: number;
  total: number;
  minPayout: number;
  blockers: string[];
  eligible: boolean;
};
export type PayoutStatus = "pending" | "awaiting_second_approval" | "approved" | "paid" | "rejected" | "failed";
/** Four-eyes state of a payout: `required` = it needs (or needed) two approvals; ids are raw staff ids. */
export type PayoutFourEyes = {
  required: boolean;
  threshold: number | null;
  firstApprovedById: string | null;
  firstApprovedBy: string | null;
  firstApprovedAt: string | null;
  firstApprovalNote: string | null;
};
export type Payout = {
  id: number;
  challengeId: number;
  accountId: number;
  userId: number;
  login: number;
  profit: number;
  split: number;
  traderAmount: number;
  firmAmount: number;
  feeRefund: number;
  total: number;
  status: PayoutStatus;
  kycStatus: string | null;
  requestedAt: string;
  decidedAt: string | null;
  decidedBy: string | null;
  note: string | null;
  error: string | null;
  planName: string;
  size: number;
  traderName: string;
  fourEyes?: PayoutFourEyes;
};
export type PropSettings = { fourEyesThreshold: number; updatedAt: string | null; updatedBy: string | null };
export type RuleEvent = {
  id: number;
  accountId: number;
  login: number;
  rule: string;
  severity: "breach" | "violation" | "warning" | "info";
  at: string;
  equity: number | null;
  balance: number | null;
  threshold: number | null;
  message: string;
  details: unknown;
  challengeId?: number;
  userId?: number;
  traderName?: string;
  planName?: string;
  size?: number;
  phase?: string;
};
export type Flag = {
  id: number;
  accountId: number;
  challengeId: number;
  userId: number;
  login: number;
  kind: string;
  score: number | null;
  summary: string;
  evidence: unknown;
  relatedLogin: number | null;
  status: "open" | "cleared" | "confirmed";
  createdAt: string;
  updatedAt: string | null;
  reviewedBy: string | null;
  reviewedAt: string | null;
  reviewNote: string | null;
  traderName?: string;
  planName?: string;
  phase?: string;
  accountStatus?: string;
};
export type AuditItem = { id: number; at: string; actor: string; actorName: string; actorRole: string; action: string; entity: string; entityId: string; before: unknown; after: unknown; reason: string | null; note: string | null };
export type ChallengeDetail = Challenge & { events: RuleEvent[]; flags: Flag[]; payouts: Payout[]; audit: AuditItem[]; payout?: PayoutQuote | null };
export type Certificate = { code: string; kind: "pass" | "funded" | "payout"; title: string; traderName: string; planName: string; size: number; amount: number | null; phase: string | null; issuedAt: string; revoked: boolean; challengeId: number; userId: number; verifyUrl: string };
export type NewsEvent = { id: number; at: string; title: string; currency: string; impact: string | null; symbols: string[]; createdBy?: string | null };
export type Overview = {
  activeChallenges: number;
  funded: number;
  failed: number;
  passRate: number | null;
  fees30d: number;
  sold30d: number;
  paid30d: number;
  payoutsPending: number;
  payoutsPendingAmount: number;
  flagsOpen: number;
  breaches24h: number;
  fundedCapital: number;
  breachReasons: { rule: string; count: number }[];
};

/* ------------------------------------------------------------------ */
/* Permissions & writes                                                 */
/* ------------------------------------------------------------------ */

/** Whether the signed-in staff member may use a prop permission (demo builds: everything). */
export function usePropCan(perm: PropPerm): boolean {
  const s = useStaff();
  if (IS_DEMO) return true;
  return propAllows(s, perm);
}

export type WriteResult<T> = { ok: true; data: T } | { ok: false; error: ApiErr };

function expired() {
  const next = window.location.pathname + window.location.search;
  window.location.assign(`/api/auth/expired?next=${encodeURIComponent(next)}`);
}

/** POST / PUT / DELETE to /api/prop/<path>; keeps the service's error (code, message, field). */
export async function propWrite<T = unknown>(path: string, body: Record<string, unknown> = {}, method: "POST" | "PUT" | "DELETE" = "POST"): Promise<WriteResult<T>> {
  try {
    const r = await fetch(`/api/prop/${path}`, { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body), credentials: "same-origin" });
    const data = await r.json().catch(() => ({}));
    if (r.status === 401) {
      expired();
      return { ok: false, error: { code: "unauthorized", message: "Your session has ended." } };
    }
    if (r.ok) return { ok: true, data: data as T };
    return { ok: false, error: data?.error ?? { code: "unknown", message: "Something went wrong." } };
  } catch {
    return { ok: false, error: { code: "network", message: "Can't reach the Back Office server." } };
  }
}

/* ------------------------------------------------------------------ */
/* Action dialog: reason code + note, async, shows the service error    */
/* ------------------------------------------------------------------ */

export type PropAction = {
  title: React.ReactNode;
  description?: React.ReactNode;
  confirmLabel: string;
  confirmVariant?: "ember" | "sell" | "buy" | "surface";
  /** reason codes; the chosen code is passed to run() */
  reasons?: string[];
  /** note field: "required" (default), "optional" or "none" */
  note?: "required" | "optional" | "none";
  noteLabel?: string;
  notePlaceholder?: string;
  body?: React.ReactNode;
  disabled?: boolean | string;
  run: (v: { reason: string; note: string }) => Promise<WriteResult<unknown>>;
  success: string;
  onDone?: (data: unknown) => void;
};

export function ActionDialog({ action, open, onOpenChange }: { action: PropAction | null; open: boolean; onOpenChange: (o: boolean) => void }) {
  const [reason, setReason] = React.useState("");
  const [note, setNote] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (open) {
      setReason(action?.reasons?.[0] ?? "");
      setNote("");
      setError(null);
      setBusy(false);
    }
  }, [open, action]);
  if (!action) return null;
  const mode = action.note ?? "required";
  const blocked = typeof action.disabled === "string" ? action.disabled : null;
  const missing = mode === "required" && !note.trim();
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={action.title}
      description={action.description}
      width={540}
      footer={
        <>
          <span className="mr-auto text-[11.5px] text-fg-3">{blocked}</span>
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant={action.confirmVariant ?? "ember"}
            size="sm"
            disabled={!!action.disabled || missing || busy}
            onClick={async () => {
              setBusy(true);
              setError(null);
              const res = await action.run({ reason, note: note.trim() });
              setBusy(false);
              if (!res.ok) {
                setError(res.error.message);
                toast.error("Not saved", { description: res.error.message });
                return;
              }
              toast.success(action.success, { description: "Recorded in the prop audit log" });
              action.onDone?.(res.data);
              onOpenChange(false);
            }}
          >
            {busy ? "Working…" : action.confirmLabel}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {action.body}
        {action.reasons && action.reasons.length > 0 && (
          <MiniField label="Reason">
            <Select value={reason} onChange={setReason} options={action.reasons} />
          </MiniField>
        )}
        {mode !== "none" && (
          <label className="block">
            <span className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
              {action.noteLabel ?? "Note"}
              <span className={cn("text-[11px] font-normal", mode === "required" ? "text-down" : "text-fg-3")}>{mode === "required" ? "Required" : "Optional"}</span>
            </span>
            <textarea
              value={note}
              onChange={(e) => setNote(e.target.value)}
              rows={3}
              aria-label={action.noteLabel ?? "Note"}
              placeholder={action.notePlaceholder ?? "Context for the audit log"}
              className="w-full resize-none rounded-[14px] border border-line bg-surface-2 px-3.5 py-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-ember/50 focus:ring-4 focus:ring-ember/10"
            />
          </label>
        )}
        {error && (
          <div role="alert" className="flex items-start gap-2.5 rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12.5px] text-fg">
            <AlertTriangle className="mt-0.5 size-3.5 shrink-0 text-down" />
            {error}
          </div>
        )}
        <div className="flex items-start gap-2.5 rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] text-fg-3">
          <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-up" />
          Written to the prop audit log with your staff ID, role, the reason and the before / after values.
        </div>
      </div>
    </Dialog>
  );
}

/** const act = useAction(); act.ask({...}); {act.node} */
export function useAction() {
  const [action, setAction] = React.useState<PropAction | null>(null);
  const [open, setOpen] = React.useState(false);
  const ask = React.useCallback((a: PropAction) => {
    setAction(a);
    setOpen(true);
  }, []);
  return { ask, node: <ActionDialog action={action} open={open} onOpenChange={setOpen} /> };
}

/** Reason + note joined for services that take a single `reason` string. */
export const reasonText = (v: { reason: string; note: string }) => (v.note ? `${v.reason}: ${v.note}` : v.reason);

/* ------------------------------------------------------------------ */
/* States                                                               */
/* ------------------------------------------------------------------ */

const DOWN = ["unavailable", "not_configured", "bad_gateway", "network"];

export function PropError({ error, onRetry, className }: { error: ApiErr; onRetry?: () => void; className?: string }) {
  const friendly: ApiErr = DOWN.includes(error.code) ? { ...error, message: "Prop firm data comes from the prop service, which isn't reachable right now. Try again in a moment." } : error;
  return (
    <Card className={className}>
      <ErrorState error={friendly} onRetry={onRetry} />
    </Card>
  );
}

export function ReadOnlyNote({ what }: { what: string }) {
  return <Chip tone="neutral">View only · your role can&apos;t {what}</Chip>;
}

/* ------------------------------------------------------------------ */
/* Client names                                                          */
/* ------------------------------------------------------------------ */

export type ClientName = { name: string; email: string; kyc: string };

/** Names / emails / KYC of client ids from the gateway (empty when the role has no clients.read). */
export function useClientNames(ids: (number | string | null | undefined)[]) {
  const key = Array.from(new Set(ids.filter((x): x is number | string => x !== null && x !== undefined).map(String)))
    .sort()
    .slice(0, 60)
    .join(",");
  const [names, setNames] = React.useState<Record<string, ClientName>>({});
  React.useEffect(() => {
    if (!key) return;
    let alive = true;
    fetch(`/api/prop/clients?ids=${key}`, { cache: "no-store", credentials: "same-origin" })
      .then((r) => (r.ok ? r.json() : null))
      .then((d: { names?: Record<string, ClientName> } | null) => {
        if (alive && d?.names) setNames((n) => ({ ...n, ...d.names }));
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [key]);
  return names;
}

/** Trader cell: name (service) with the gateway email under it. */
export function TraderCell({ name, userId, names, sub }: { name?: string | null; userId: number; names: Record<string, ClientName>; sub?: React.ReactNode }) {
  const g = names[String(userId)];
  return (
    <span className="block min-w-0">
      <span className="block truncate text-[13px] font-medium text-fg">{g?.name || name || `Client #${userId}`}</span>
      <span className="block truncate text-[11.5px] text-fg-3">{sub ?? g?.email ?? `#${userId}`}</span>
    </span>
  );
}

/* ------------------------------------------------------------------ */
/* Labels, chips and cells                                              */
/* ------------------------------------------------------------------ */

export const BANNED_LABEL: Record<string, string> = {
  hft: "HFT",
  latency_arbitrage: "Latency arbitrage",
  tick_scalping: "Tick scalping",
  cross_account_copying: "Copy-trading between accounts",
  cross_account_hedging: "Hedging across accounts",
  martingale: "Martingale",
  grid_trading: "Grid trading",
  reverse_arbitrage: "Reverse arbitrage",
  account_management: "Account management / pass service",
};
/** Strategies the service detects (src/heuristics.rs); the others are shown to traders only. */
export const DETECTED = ["hft", "latency_arbitrage", "tick_scalping", "cross_account_copying", "cross_account_hedging"];
export const bannedLabel = (code: string) => BANNED_LABEL[code] ?? code.replace(/_/g, " ");

export const RULE_LABEL: Record<string, string> = {
  daily_loss: "Daily loss",
  max_drawdown: "Max drawdown",
  time_limit: "Time limit",
  weekend_holding: "Weekend holding",
  news_window: "News window",
  banned_strategy: "Banned strategy",
  profit_target: "Profit target",
  consistency: "Consistency",
  manual: "Manual decision",
};
export const ruleLabel = (r: string | null | undefined) => (r ? (RULE_LABEL[r] ?? r.replace(/_/g, " ")) : "—");

const SEVERITY: Record<string, { tone: ChipTone; label: string }> = {
  breach: { tone: "down", label: "Breach" },
  violation: { tone: "ember", label: "Violation" },
  warning: { tone: "warn", label: "Warning" },
  info: { tone: "neutral", label: "Info" },
};
export function SeverityChip({ severity }: { severity: string }) {
  const s = SEVERITY[severity] ?? { tone: "neutral" as ChipTone, label: severity };
  return (
    <Chip size="sm" tone={s.tone} dot>
      {s.label}
    </Chip>
  );
}

const STATUS: Record<string, { tone: ChipTone; label: string }> = {
  // challenges
  pending_payment: { tone: "warn", label: "Awaiting payment" },
  provisioning: { tone: "info", label: "Provisioning" },
  active: { tone: "up", label: "Active" },
  funded: { tone: "gold", label: "Funded" },
  failed: { tone: "down", label: "Failed" },
  closed: { tone: "neutral", label: "Closed" },
  payment_failed: { tone: "down", label: "Payment failed" },
  passed: { tone: "up", label: "Passed" },
  // plans
  draft: { tone: "neutral", label: "Draft" },
  paused: { tone: "warn", label: "Paused" },
  archived: { tone: "neutral", label: "Archived" },
  // payouts
  pending: { tone: "warn", label: "Pending" },
  awaiting_second_approval: { tone: "info", label: "Awaiting 2nd approval" },
  approved: { tone: "info", label: "Approved" },
  paid: { tone: "up", label: "Paid" },
  rejected: { tone: "down", label: "Rejected" },
  // flags
  open: { tone: "warn", label: "Open" },
  cleared: { tone: "up", label: "Cleared" },
  confirmed: { tone: "down", label: "Confirmed" },
};
export function PropStatus({ status, label }: { status: string; label?: string }) {
  const s = STATUS[status] ?? { tone: "neutral" as ChipTone, label: status };
  return (
    <Chip size="sm" dot tone={s.tone}>
      {label ?? s.label}
    </Chip>
  );
}

export function PlanTypeChip({ type }: { type: PlanType }) {
  return (
    <Chip size="sm" tone={type === "instant" ? "gold" : type === "1-step" ? "ember" : "neutral"}>
      {type === "instant" ? "Instant" : type === "1-step" ? "1-Step" : "2-Step"}
    </Chip>
  );
}

const KYC: Record<string, { tone: ChipTone; label: string }> = {
  verified: { tone: "up", label: "KYC verified" },
  pending: { tone: "warn", label: "KYC pending" },
  rejected: { tone: "down", label: "KYC rejected" },
  unverified: { tone: "neutral", label: "No KYC" },
};
export function KycStatus({ status }: { status: string | null | undefined }) {
  if (!status) return <span className="text-fg-3">—</span>;
  const s = KYC[status] ?? { tone: "neutral" as ChipTone, label: status };
  return (
    <Chip size="sm" tone={s.tone} dot>
      {s.label}
    </Chip>
  );
}

/* Four-eyes on payouts */

export const SECOND_APPROVER_NEEDED = "You gave the first approval. Another staff member must give the second approval (four-eyes).";

/** Whether `staffId` gave the first (four-eyes) approval of the payout. */
export const isFirstApprover = (p: Payout, staffId: string | number) => p.fourEyes?.firstApprovedById != null && String(p.fourEyes.firstApprovedById) === String(staffId);

/** "First approval by <name> · <when>" (+ note) for a payout waiting for its second approval. */
export function FirstApproval({ p, now, className }: { p: Payout; now: number; className?: string }) {
  const f = p.fourEyes;
  if (p.status !== "awaiting_second_approval" || !f?.firstApprovedBy) return null;
  return (
    <span className={className ?? "block max-w-56 text-[10.5px] leading-snug text-fg-3"}>
      First approval by {f.firstApprovedBy}
      {f.firstApprovedAt && <span title={when(f.firstApprovedAt)}> · {ago(f.firstApprovedAt, now)}</span>}
      {f.firstApprovalNote && <span className="block truncate" title={f.firstApprovalNote}>“{f.firstApprovalNote}”</span>}
    </span>
  );
}

export const BLOCKER_LABEL: Record<string, string> = {
  not_yet_eligible: "Not yet eligible",
  below_minimum: "Below minimum",
  positions_open: "Positions open",
  payout_pending: "Payout in review",
  consistency: "Consistency not met",
};

/** Used / limit bar (daily loss, drawdown). */
export function UsageBar({ used, limit, className }: { used: number | null | undefined; limit: number | null | undefined; className?: string }) {
  if (used === null || used === undefined || !limit) return <span className="text-fg-3">—</span>;
  const pct = Math.max(0, Math.min(100, (used / limit) * 100));
  const tone = pct >= 90 ? "bg-down" : pct >= 50 ? "bg-warn" : "bg-up";
  const text = pct >= 90 ? "text-down" : pct >= 50 ? "text-warn" : "text-fg-2";
  return (
    <span className={cn("block w-28", className)}>
      <span className="flex items-baseline justify-between text-[11.5px]">
        <span className={cn("k-num font-medium", text)}>{usd(Math.max(0, used), 0)}</span>
        <span className="k-num text-fg-3">/ {usd(limit, 0)}</span>
      </span>
      <span className="mt-1 block h-1.5 overflow-hidden rounded-full bg-surface-3">
        <span className={cn("block h-full rounded-full", tone)} style={{ width: `${pct}%` }} />
      </span>
    </span>
  );
}

/** Profit vs target bar. */
export function TargetBar({ profit, target, className }: { profit: number | null | undefined; target: number | null | undefined; className?: string }) {
  if (profit === null || profit === undefined) return <span className="text-fg-3">—</span>;
  if (!target) return <span className={cn("k-num text-[12.5px] font-medium", profit >= 0 ? "text-up" : "text-down")}>{signedUsd(profit)}</span>;
  const pos = Math.max(0, Math.min(100, (profit / target) * 100));
  const neg = profit < 0 ? Math.min(100, (-profit / target) * 100) : 0;
  return (
    <span className={cn("block w-32", className)}>
      <span className="flex items-baseline justify-between text-[11.5px]">
        <span className={cn("k-num font-medium", profit >= 0 ? "text-up" : "text-down")}>{signedUsd(profit, 0)}</span>
        <span className="k-num text-fg-3">/ {usd(target, 0)}</span>
      </span>
      <span className="mt-1 block h-1.5 overflow-hidden rounded-full bg-surface-3">
        {profit >= 0 ? <span className={cn("block h-full rounded-full", pos >= 100 ? "bg-up" : "bg-ember")} style={{ width: `${pos}%` }} /> : <span className="block h-full rounded-full bg-down/70" style={{ width: `${neg}%` }} />}
      </span>
    </span>
  );
}

export const usd = (v: number | null | undefined, d = 2) => (v === null || v === undefined || !Number.isFinite(v) ? "—" : `${v < 0 ? "−" : ""}$${formatNumber(Math.abs(v), d)}`);
export const signedUsd = (v: number, d = 2) => `${v > 0 ? "+" : ""}${usd(v, d)}`;
export const usdK = (v: number | null | undefined) => {
  if (v === null || v === undefined) return "—";
  const a = Math.abs(v);
  const s = a >= 1_000_000 ? `$${(a / 1_000_000).toFixed(a % 1_000_000 ? 1 : 0)}M` : a >= 1000 ? `$${formatNumber(a / 1000, a % 1000 ? 1 : 0)}K` : `$${formatNumber(a, 0)}`;
  return v < 0 ? `−${s}` : s;
};
export const pct = (v: number | null | undefined, d = 1) => (v === null || v === undefined ? "—" : `${formatNumber(v, d)}%`);
export const int = (v: number | null | undefined) => (v === null || v === undefined ? "—" : v.toLocaleString("en-US", { maximumFractionDigits: 0 }));

/** Readable rendering of a JSON value (evidence, audit before/after). */
export function JsonView({ value, className }: { value: unknown; className?: string }) {
  if (value === null || value === undefined) return <span className="text-fg-3">—</span>;
  if (typeof value !== "object") return <span className="k-num text-fg">{String(value)}</span>;
  if (Array.isArray(value)) {
    if (value.length === 0) return <span className="text-fg-3">none</span>;
    if (value.every((v) => v === null || typeof v !== "object")) return <span className="k-num text-fg">{value.map(String).join(", ")}</span>;
    return (
      <div className={cn("space-y-1.5", className)}>
        {value.slice(0, 50).map((v, i) => (
          <div key={i} className="rounded-[10px] border border-line bg-surface-2/60 px-2.5 py-1.5">
            <JsonView value={v} />
          </div>
        ))}
        {value.length > 50 && <div className="text-[11.5px] text-fg-3">+{value.length - 50} more</div>}
      </div>
    );
  }
  const entries = Object.entries(value as Record<string, unknown>);
  if (!entries.length) return <span className="text-fg-3">—</span>;
  return (
    <dl className={cn("grid grid-cols-[minmax(90px,auto)_1fr] gap-x-3 gap-y-1 text-[12px]", className)}>
      {entries.map(([k, v]) => (
        <React.Fragment key={k}>
          <dt className="text-fg-3">{k.replace(/([a-z])([A-Z])/g, "$1 $2").replace(/_/g, " ")}</dt>
          <dd className="min-w-0 break-words text-fg-2">
            <JsonView value={v} />
          </dd>
        </React.Fragment>
      ))}
    </dl>
  );
}

"use client";

/**
 * Shared building blocks for the live IB programme pages (components/partners-live/*).
 */
import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import { Button, Card, Chip, Dialog, DialogClose, IconGlyph, cn, formatNumber, type ChipTone } from "@kalks/ui";
import { ErrorState, useApi, type ApiErr } from "@/components/live/kit";
import { ShieldCheck } from "lucide-react";
import { ErrorBanner } from "@/components/trading-desk/kit";
import { P, errText, type BatchStatus, type CommKind, type CommStatus, type FlagKind, type Level, type Perms, type PayoutStatus, type WriteResult } from "./api";

/* ------------------------------------------------------------------ */
/* Permissions & shared data                                            */
/* ------------------------------------------------------------------ */

/** The signed-in staff member's IB permissions (GET /api/partners/me). Nothing is allowed until it loads. */
export function usePerms(): Perms & { loaded: boolean } {
  const { data } = useApi<Perms>(P("me"));
  return { read: data?.read ?? true, write: !!data?.write, approve: !!data?.approve, loaded: !!data };
}

/** Level table (names, ranks, icons) for chips and pickers. */
export function useLevels() {
  const r = useApi<{ levels: Level[] }>(P("levels"));
  const levels = React.useMemo(() => [...(r.data?.levels ?? [])].sort((a, b) => a.rank - b.rank), [r.data]);
  return { levels, loading: r.loading && !r.data, error: r.error, reload: r.reload };
}

/* ------------------------------------------------------------------ */
/* Formatting                                                           */
/* ------------------------------------------------------------------ */

export const usd = (v: number | null | undefined, d = 2) => (v === null || v === undefined || !Number.isFinite(v) ? "—" : `${v < 0 ? "−" : ""}$${formatNumber(Math.abs(v), d)}`);
export const usdK = (v: number) => {
  const a = Math.abs(v);
  const s = a >= 1_000_000 ? `$${(a / 1_000_000).toFixed(2)}M` : a >= 10_000 ? `$${(a / 1000).toFixed(a >= 100_000 ? 0 : 1)}k` : `$${formatNumber(a, a >= 100 ? 0 : 2)}`;
  return v < 0 ? `−${s}` : s;
};
export const int = (v: number | null | undefined) => (v === null || v === undefined ? "—" : v.toLocaleString("en-US", { maximumFractionDigits: 0 }));
export const lots = (v: number | null | undefined, d = 2) => (v === null || v === undefined ? "—" : v.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: Math.max(d, 4) }));
/* ------------------------------------------------------------------ */
/* Options (Kalks FX Options): paid per contract, never per lot        */
/* ------------------------------------------------------------------ */

/** Engine option series code, e.g. EURUSD-20261009-1.1650-C (same rule as services/ib `is_option_series`). */
export const isOptionSeries = (symbol: string | null | undefined) => !!symbol && /^[^-]+-\d{8}-[\d.]+-[CPcp]$/.test(symbol);

/** An IB line or deal on an option: the service's "options" symbol group, contracts, or an option series symbol (clawbacks). */
export const isOptionLine = (c: { symbolGroup?: string | null; contracts?: number | null; instrument?: string | null; symbol?: string | null }) =>
  c.instrument === "option" || c.symbolGroup === "options" || (c.contracts ?? 0) > 0 || isOptionSeries(c.symbol);

/** "Options · 3 contracts" ("Options" when the line carries no contract count, e.g. a clawback). */
export const optionsLabel = (contracts: number | null | undefined) => {
  const n = Math.abs(contracts ?? 0);
  if (!n) return "Options";
  return `Options · ${n.toLocaleString("en-US", { maximumFractionDigits: 2 })} contract${n === 1 ? "" : "s"}`;
};

/** Kind label of a line: "Per contract" for the per-lot kind on an option deal (options are paid per contract). */
export const kindLabelOf = (c: { kind: CommKind; symbolGroup?: string | null; contracts?: number | null; symbol?: string | null }) =>
  c.kind === "lot" && isOptionLine(c) ? "Per contract" : (COMM_KIND[c.kind]?.label ?? c.kind);

/** Units of a line: "Options · 3 contracts" on options, else "1.50 lots" ("" when it has none, e.g. a CFD clawback). */
export const unitsText = (c: { symbolGroup?: string | null; contracts?: number | null; symbol?: string | null; lots: number }) => (isOptionLine(c) ? optionsLabel(c.contracts) : c.lots ? `${lots(c.lots)} lots` : "");

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
/** "2026-09" → "Sep" (or "Sep 26" with year). */
export const monthLabel = (m: string, withYear = false) => {
  const [y, mm] = m.split("-");
  const l = MONTHS[Number(mm) - 1] ?? m;
  return withYear ? `${l} ${y?.slice(2)}` : l;
};
export const WEEKDAYS = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/* ------------------------------------------------------------------ */
/* Levels                                                               */
/* ------------------------------------------------------------------ */

const LEVEL_PALETTE: { color: string; tone: ChipTone }[] = [
  { color: "color-mix(in oklab, var(--k-ember) 55%, var(--k-fg-3))", tone: "neutral" },
  { color: "var(--k-fg-2)", tone: "neutral" },
  { color: "var(--k-gold)", tone: "gold" },
  { color: "var(--k-info)", tone: "info" },
  { color: "var(--k-ember)", tone: "ember" },
  { color: "var(--k-up)", tone: "up" },
];

/** Colour + chip tone for a level, by its position in the ladder. */
export function levelStyle(key: string, levels: Level[] | { key: string; rank: number }[]) {
  const sorted = [...levels].sort((a, b) => a.rank - b.rank);
  const i = Math.max(0, sorted.findIndex((l) => l.key === key));
  return LEVEL_PALETTE[Math.min(i, LEVEL_PALETTE.length - 1)]!;
}

export function LevelChip({ level, levels, size = "sm" }: { level: string | null | undefined; levels: Level[]; size?: "sm" | "md" }) {
  if (!level) return <span className="text-fg-3">—</span>;
  const l = levels.find((x) => x.key === level);
  const s = levelStyle(level, levels);
  return (
    <Chip size={size} tone={s.tone}>
      {l && <IconGlyph name={l.icon} className={size === "sm" ? "size-3" : "size-3.5"} />}
      {l?.name ?? level}
    </Chip>
  );
}

/* ------------------------------------------------------------------ */
/* Status chips                                                         */
/* ------------------------------------------------------------------ */

const S = (label: string, tone: ChipTone) => ({ label, tone });

export const COMM_STATUS: Record<CommStatus, { label: string; tone: ChipTone }> = {
  pending: S("Pending", "warn"),
  approved: S("Approved", "info"),
  paid: S("Paid", "up"),
  rejected: S("Rejected", "down"),
  void: S("Void", "neutral"),
};

export const COMM_KIND: Record<CommKind, { label: string; tone: ChipTone }> = {
  lot: S("Per lot", "neutral"),
  split: S("Sub-IB split", "info"),
  rebate: S("Client rebate", "gold"),
  cpa: S("CPA", "ember"),
  clawback: S("Clawback", "down"),
  adjustment: S("Adjustment", "neutral"),
};

export const BATCH_STATUS: Record<BatchStatus, { label: string; tone: ChipTone }> = {
  pending_approval: S("Awaiting approval", "warn"),
  awaiting_second_approval: S("Awaiting 2nd approval", "warn"),
  approved: S("Approved · transferring", "info"),
  paid: S("Paid", "up"),
  partially_paid: S("Partly paid", "down"),
  rejected: S("Rejected", "neutral"),
};

export const PAYOUT_STATUS: Record<PayoutStatus, { label: string; tone: ChipTone }> = {
  pending_approval: S("Awaiting approval", "warn"),
  transfer_pending: S("Pending transfer", "info"),
  paid: S("Credited", "up"),
  rejected: S("Rejected", "neutral"),
  failed: S("Transfer failed", "down"),
};

export const FLAG_KIND: Record<FlagKind, { label: string; short: string; desc: string }> = {
  self_referral_ip: { label: "Self-referral · IP", short: "Same IP", desc: "The client signed up or traded from the IB's IP address." },
  self_referral_device: { label: "Self-referral · device", short: "Same device", desc: "The client and the IB share a device fingerprint." },
  self_referral_identity: { label: "Self-referral · identity", short: "Same identity", desc: "The client's identity details match the IB's." },
  wash_trading: { label: "Wash trading", short: "Wash trading", desc: "Opposite trades on the same symbol and size, opened and closed close together." },
  short_trades: { label: "Short-trade pattern", short: "Short trades", desc: "Many trades held below the minimum duration within 24 hours." },
};

export const FLAG_STATUS: Record<string, { label: string; tone: ChipTone }> = {
  open: S("Open", "warn"),
  confirmed: S("Confirmed", "down"),
  dismissed: S("Dismissed", "neutral"),
};

export const SEV_TONE: Record<string, ChipTone> = { high: "down", medium: "warn", low: "neutral" };

export function StatusPill({ map, status }: { map: Record<string, { label: string; tone: ChipTone }>; status: string }) {
  const s = map[status] ?? { label: status, tone: "neutral" as ChipTone };
  return (
    <Chip size="sm" dot tone={s.tone}>
      {s.label}
    </Chip>
  );
}

/* ------------------------------------------------------------------ */
/* Cells                                                                */
/* ------------------------------------------------------------------ */

/** Member name + id, linking to the partner detail. */
export function MemberLink({ id, name, sub, className }: { id: number | null | undefined; name?: string | null; sub?: React.ReactNode; className?: string }) {
  if (id === null || id === undefined) return <span className="text-fg-3">—</span>;
  return (
    <span className={cn("block min-w-0", className)}>
      <Link href={`/partners/list?partner=${id}`} onClick={(e) => e.stopPropagation()} className="block truncate text-[13px] font-medium text-fg hover:text-ember">
        {name?.trim() || `Client #${id}`}
      </Link>
      <span className="block truncate font-mono text-[11px] text-fg-3">
        #{id}
        {sub ? <> · {sub}</> : null}
      </span>
    </span>
  );
}

/* ------------------------------------------------------------------ */
/* States                                                               */
/* ------------------------------------------------------------------ */

const DOWN = ["unavailable", "not_configured", "bad_gateway", "network", "internal"];

export function PartnersError({ error, onRetry, className }: { error: ApiErr; onRetry?: () => void; className?: string }) {
  const friendly: ApiErr = DOWN.includes(error.code) ? { ...error, message: `The partner service isn't reachable right now. ${error.code === "not_configured" ? "It isn't configured for this Back Office." : "Try again in a moment."}` } : error;
  return (
    <Card className={className}>
      <ErrorState error={friendly} onRetry={onRetry} />
    </Card>
  );
}

/** Quiet empty panel used inside cards. */
export function EmptyNote({ title, text, className, action }: { title: string; text?: React.ReactNode; className?: string; action?: React.ReactNode }) {
  return (
    <div className={cn("flex flex-col items-center justify-center rounded-[16px] border border-dashed border-line px-6 py-10 text-center", className)}>
      <div className="text-[13.5px] font-medium text-fg-2">{title}</div>
      {text && <div className="mt-1 max-w-sm text-[12.5px] text-fg-3">{text}</div>}
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}

export function ReadOnlyNote({ what }: { what: string }) {
  return <Chip tone="neutral">View only · your role can't {what}</Chip>;
}

/* ------------------------------------------------------------------ */
/* URL param (no router re-render; shareable links)                     */
/* ------------------------------------------------------------------ */

export function useUrlParam(name: string): [string | null, (v: string | null) => void] {
  const [v, setV] = React.useState<string | null>(null);
  React.useEffect(() => {
    const read = () => setV(new URLSearchParams(window.location.search).get(name));
    read();
    window.addEventListener("popstate", read);
    return () => window.removeEventListener("popstate", read);
  }, [name]);
  const set = React.useCallback(
    (next: string | null) => {
      const u = new URL(window.location.href);
      if (next === null) u.searchParams.delete(name);
      else u.searchParams.set(name, next);
      window.history.replaceState(window.history.state, "", u.pathname + u.search);
      setV(next);
    },
    [name],
  );
  return [v, set];
}

/* ------------------------------------------------------------------ */
/* Action dialog: reason (3–500 chars) or optional note                 */
/* ------------------------------------------------------------------ */

export type ReasonAction<T = unknown> = {
  title: React.ReactNode;
  description?: React.ReactNode;
  body?: React.ReactNode;
  confirmLabel: string;
  confirmVariant?: "ember" | "buy" | "sell" | "surface";
  /** "required" (default): 3–500 chars. "optional": a note that may be empty. "none": no text field. */
  reason?: "required" | "optional" | "none";
  reasonLabel?: string;
  placeholder?: string;
  /** blocks the confirm button; a string is shown next to it */
  disabled?: boolean | string;
  run: (reason: string) => Promise<WriteResult<T>>;
  success: string | ((data: T) => string);
  successDetail?: string | ((data: T) => string | undefined);
  onDone?: (data: T) => void;
};

export function ReasonDialog({ action, open, onOpenChange }: { action: ReasonAction<any> | null; open: boolean; onOpenChange: (o: boolean) => void }) {
  const [text, setText] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (open) {
      setText("");
      setError(null);
      setBusy(false);
    }
  }, [open, action]);
  if (!action) return null;
  const mode = action.reason ?? "required";
  const len = text.trim().length;
  const short = mode === "required" && len < 3;
  const long = len > 500;
  const blocked = typeof action.disabled === "string" ? action.disabled : null;
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      title={action.title}
      description={action.description}
      width={560}
      footer={
        <>
          <span className="mr-auto text-[11.5px] text-fg-3">{blocked}</span>
          <DialogClose asChild>
            <Button variant="ghost" size="sm" disabled={busy}>
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant={action.confirmVariant ?? "ember"}
            size="sm"
            disabled={!!action.disabled || short || long || busy}
            onClick={async () => {
              setBusy(true);
              setError(null);
              const res = await action.run(text.trim());
              setBusy(false);
              if (!res.ok) {
                const msg = errText(res.error);
                setError(msg);
                toast.error("Not saved", { description: msg });
                return;
              }
              const title = typeof action.success === "function" ? action.success(res.data) : action.success;
              const detail = typeof action.successDetail === "function" ? action.successDetail(res.data) : action.successDetail;
              toast.success(title, { description: detail ?? "Recorded in the partner audit log" });
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
        {mode !== "none" && (
          <label className="block">
            <span className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
              {action.reasonLabel ?? (mode === "optional" ? "Note" : "Reason")}
              <span className={cn("text-[11px] font-normal", mode === "required" ? (short ? "text-down" : "text-fg-3") : "text-fg-3")}>
                {mode === "required" ? (short ? "Required · at least 3 characters" : `${len}/500`) : "Optional"}
              </span>
            </span>
            <textarea
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={3}
              maxLength={600}
              aria-label={action.reasonLabel ?? "Reason"}
              placeholder={action.placeholder ?? "Why is this being done? Kept in the audit log."}
              className="w-full resize-none rounded-[14px] border border-line bg-surface-2 px-3.5 py-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-ember/50 focus:ring-4 focus:ring-ember/10"
            />
            {long && <span className="mt-1 block text-[11.5px] text-down">Keep it under 500 characters.</span>}
          </label>
        )}
        <ErrorBanner error={error} />
        <AuditNotice />
      </div>
    </Dialog>
  );
}

function AuditNotice() {
  return (
    <div className="flex items-start gap-2.5 rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] text-fg-3">
      <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-up" />
      Written to the partner programme audit log with your staff ID, the time and the before / after values.
    </div>
  );
}

/** const act = useReasonAction(); act.ask({...}); {act.node} */
export function useReasonAction() {
  const [action, setAction] = React.useState<ReasonAction<any> | null>(null);
  const [open, setOpen] = React.useState(false);
  const ask = React.useCallback(<T,>(a: ReasonAction<T>) => {
    setAction(a as ReasonAction<any>);
    setOpen(true);
  }, []);
  return { ask, node: <ReasonDialog action={action} open={open} onOpenChange={setOpen} /> };
}

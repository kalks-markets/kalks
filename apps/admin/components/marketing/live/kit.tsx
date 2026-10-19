"use client";

/**
 * Shared building blocks for the live Marketing pages (components/marketing/live/*): permissions, labelled form
 * fields (real <label for> so assistive tech and getByLabel work), status chips, states and the action dialog.
 */
import * as React from "react";
import { toast } from "sonner";
import { ShieldCheck } from "lucide-react";
import { Button, Card, Chip, Dialog, DialogClose, Toggle, cn, formatNumber, type ChipTone } from "@kalks/ui";
import { ErrorState, useApi, type ApiErr } from "@/components/live/kit";
import { ErrorBanner } from "@/components/trading-desk/kit";
import { M, errText, type Perms, type WriteResult } from "./api";

/* ------------------------------------------------------------------ */
/* Permissions                                                          */
/* ------------------------------------------------------------------ */

/** The signed-in staff member's Marketing permissions (GET /api/marketing/me). Nothing is allowed until it loads. */
export function usePerms(): Perms & { loaded: boolean } {
  const { data } = useApi<Perms>(M("me"));
  return { read: data?.read ?? true, write: !!data?.write, approve: !!data?.approve, loaded: !!data };
}

/* ------------------------------------------------------------------ */
/* Formatting                                                           */
/* ------------------------------------------------------------------ */

const fin = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
export const usd = (v: number | null | undefined, d = 2) => (!fin(v) ? "—" : `${v < 0 ? "−" : ""}$${formatNumber(Math.abs(v), d)}`);
export const usdK = (v: number | null | undefined) => {
  if (!fin(v)) return "—";
  const a = Math.abs(v);
  const s = a >= 1_000_000 ? `$${(a / 1_000_000).toFixed(2)}M` : a >= 10_000 ? `$${(a / 1000).toFixed(a >= 100_000 ? 0 : 1)}k` : `$${formatNumber(a, a >= 100 ? 0 : 2)}`;
  return v < 0 ? `−${s}` : s;
};
export const int = (v: number | null | undefined) => (!fin(v) ? "—" : v.toLocaleString("en-US", { maximumFractionDigits: 0 }));
export const num = (v: number | null | undefined, d = 2) => (!fin(v) ? "—" : v.toLocaleString("en-US", { minimumFractionDigits: 0, maximumFractionDigits: d }));
export const pct = (v: number | null | undefined, d = 1) => (!fin(v) ? "—" : `${v.toFixed(d)}%`);

/** ISO → value for <input type="datetime-local"> in the browser's time zone. */
export function toLocalInput(iso: string | null | undefined) {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}:${p(d.getMinutes())}`;
}
/** datetime-local value → RFC 3339 (UTC), or null when empty / invalid. */
export function fromLocalInput(v: string) {
  if (!v) return null;
  const d = new Date(v);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}
/** "" → null, else the number (NaN → null). */
export function numOrNull(v: string) {
  if (v.trim() === "") return null;
  const n = Number(v);
  return Number.isFinite(n) ? n : null;
}
export const numStr = (v: number | null | undefined) => (v === null || v === undefined ? "" : String(v));
/** "IN, ae  my" → ["IN","AE","MY"] */
export const splitList = (v: string, upper = false) =>
  v
    .split(/[\s,;]+/)
    .map((s) => (upper ? s.trim().toUpperCase() : s.trim()))
    .filter(Boolean);

/* ------------------------------------------------------------------ */
/* Labelled fields                                                      */
/* ------------------------------------------------------------------ */

const box = "flex h-10 items-center gap-1.5 rounded-[12px] border border-line bg-surface-2 px-3 text-[13.5px] transition-colors focus-within:border-ember/50 focus-within:ring-4 focus-within:ring-ember/10";
const inner = "h-full min-w-0 flex-1 bg-transparent text-fg outline-none placeholder:text-fg-3";

function Wrap({ id, label, hint, error, className, children }: { id: string; label: string; hint?: React.ReactNode; error?: string | null; className?: string; children: React.ReactNode }) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1.5", className)}>
      <div className="flex items-center justify-between gap-2 text-[12.5px] font-medium text-fg-2">
        <label htmlFor={id}>{label}</label>
        {hint && <span className="truncate text-[11.5px] font-normal text-fg-3">{hint}</span>}
      </div>
      {children}
      {error && <span className="text-[11.5px] text-down">{error}</span>}
    </div>
  );
}

export function TextF({ label, value, onChange, hint, error, placeholder, mono, className, maxLength, disabled, prefix }: { label: string; value: string; onChange: (v: string) => void; hint?: React.ReactNode; error?: string | null; placeholder?: string; mono?: boolean; className?: string; maxLength?: number; disabled?: boolean; prefix?: React.ReactNode }) {
  const id = React.useId();
  return (
    <Wrap id={id} label={label} hint={hint} error={error} className={className}>
      <div className={cn(box, disabled && "opacity-60")}>
        {prefix && <span className="shrink-0 text-fg-3 [&_svg]:size-4">{prefix}</span>}
        <input id={id} value={value} disabled={disabled} maxLength={maxLength} placeholder={placeholder} onChange={(e) => onChange(e.target.value)} className={cn(inner, mono && "font-mono tracking-wide")} />
      </div>
    </Wrap>
  );
}

export function NumF({ label, value, onChange, hint, error, prefix, suffix, step = "any", className, placeholder, disabled }: { label: string; value: string; onChange: (v: string) => void; hint?: React.ReactNode; error?: string | null; prefix?: string; suffix?: string; step?: string | number; className?: string; placeholder?: string; disabled?: boolean }) {
  const id = React.useId();
  return (
    <Wrap id={id} label={label} hint={hint} error={error} className={className}>
      <div className={cn(box, disabled && "opacity-60")}>
        {prefix && <span className="text-fg-3">{prefix}</span>}
        <input
          id={id}
          type="number"
          inputMode="decimal"
          step={step}
          value={value}
          disabled={disabled}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
          className={cn(inner, "k-num [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none")}
        />
        {suffix && <span className="whitespace-nowrap text-[12px] text-fg-3">{suffix}</span>}
      </div>
    </Wrap>
  );
}

export function AreaF({ label, value, onChange, hint, rows = 3, placeholder, className }: { label: string; value: string; onChange: (v: string) => void; hint?: React.ReactNode; rows?: number; placeholder?: string; className?: string }) {
  const id = React.useId();
  return (
    <Wrap id={id} label={label} hint={hint} className={className}>
      <textarea
        id={id}
        rows={rows}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="w-full resize-y rounded-[12px] border border-line bg-surface-2 px-3 py-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-ember/50 focus:ring-4 focus:ring-ember/10"
      />
    </Wrap>
  );
}

export function SelectF<T extends string>({ label, value, onChange, options, hint, className }: { label: string; value: T; onChange: (v: T) => void; options: { value: T; label: string }[]; hint?: React.ReactNode; className?: string }) {
  const id = React.useId();
  return (
    <Wrap id={id} label={label} hint={hint} className={className}>
      <div className={box}>
        <select id={id} value={value} onChange={(e) => onChange(e.target.value as T)} className={cn(inner, "cursor-pointer [&>option]:bg-surface")}>
          {options.map((o) => (
            <option key={o.value} value={o.value}>
              {o.label}
            </option>
          ))}
        </select>
      </div>
    </Wrap>
  );
}

export function DateTimeF({ label, value, onChange, hint, error, className }: { label: string; value: string; onChange: (v: string) => void; hint?: React.ReactNode; error?: string | null; className?: string }) {
  const id = React.useId();
  return (
    <Wrap id={id} label={label} hint={hint} error={error} className={className}>
      <div className={box}>
        <input id={id} type="datetime-local" value={value} onChange={(e) => onChange(e.target.value)} className={cn(inner, "k-num text-[13px] [color-scheme:dark]")} />
      </div>
    </Wrap>
  );
}

/** Label + hint on the left, a switch on the right. The switch's accessible name is the label. */
export function ToggleRow({ label, hint, checked, onChange, disabled }: { label: string; hint?: React.ReactNode; checked: boolean; onChange: (v: boolean) => void; disabled?: boolean }) {
  return (
    <div className={cn("flex items-center justify-between gap-4 py-3", disabled && "pointer-events-none opacity-50")}>
      <div className="min-w-0">
        <div className="text-[13.5px] font-medium">{label}</div>
        {hint && <div className="text-[12px] text-fg-3">{hint}</div>}
      </div>
      <Toggle checked={checked} onChange={onChange} label={label} />
    </div>
  );
}

/** Pill multi-select with an accessible group name. */
export function PillPicker<T extends string>({ label, options, value, onChange, className }: { label: string; options: { value: T; label: string }[]; value: T[]; onChange: (v: T[]) => void; className?: string }) {
  return (
    <div className={className}>
      <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">{label}</div>
      <div role="group" aria-label={label} className="flex flex-wrap gap-1.5">
        {options.map((o) => {
          const on = value.includes(o.value);
          return (
            <button
              key={o.value}
              type="button"
              aria-pressed={on}
              onClick={() => onChange(on ? value.filter((x) => x !== o.value) : [...value, o.value])}
              className={cn("inline-flex h-8 items-center rounded-full border px-3 text-[12.5px] font-medium transition-colors", on ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}
            >
              {o.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

export function FormSection({ title, hint, children, className }: { title: string; hint?: React.ReactNode; children: React.ReactNode; className?: string }) {
  return (
    <section className={cn("space-y-3", className)}>
      <div>
        <div className="k-label">{title}</div>
        {hint && <div className="mt-0.5 text-[12px] text-fg-3">{hint}</div>}
      </div>
      {children}
    </section>
  );
}

/* ------------------------------------------------------------------ */
/* Status chips                                                         */
/* ------------------------------------------------------------------ */

const S = (label: string, tone: ChipTone) => ({ label, tone });

export const CAMPAIGN_STATUS: Record<string, { label: string; tone: ChipTone }> = { draft: S("Draft", "neutral"), active: S("Active", "up"), paused: S("Paused", "warn"), ended: S("Ended", "neutral") };
export const GRANT_STATUS: Record<string, { label: string; tone: ChipTone }> = {
  awaiting_deposit: S("Awaiting deposit", "info"),
  pending: S("Posting", "warn"),
  active: S("Releasing", "gold"),
  completed: S("Released", "up"),
  forfeited: S("Forfeited", "down"),
  expired: S("Expired", "neutral"),
  cancelled: S("Cancelled", "neutral"),
  failed: S("Failed", "down"),
};
export const CONTEST_STATUS: Record<string, { label: string; tone: ChipTone }> = {
  draft: S("Draft", "neutral"),
  scheduled: S("Scheduled", "info"),
  running: S("Running", "ember"),
  ended: S("Ended", "warn"),
  finalized: S("Finalized", "gold"),
  paid: S("Paid", "up"),
  cancelled: S("Cancelled", "neutral"),
};
export const PAY_STATUS: Record<string, { label: string; tone: ChipTone }> = {
  pending: S("Pending", "warn"),
  paid: S("Paid", "up"),
  completed: S("Completed", "up"),
  failed: S("Failed", "down"),
  accrued: S("Accrued", "gold"),
  void: S("Void", "neutral"),
  none: S("—", "neutral"),
  applied: S("Applied", "up"),
  blocked: S("Blocked", "down"),
  open: S("Open", "warn"),
  cleared: S("Cleared", "neutral"),
  disqualified: S("Disqualified", "down"),
  approval: S("Needs approval", "gold"),
  kyc: S("Waiting for KYC", "neutral"),
  rejected: S("Rejected", "neutral"),
};

export function StatusPill({ map, status }: { map: Record<string, { label: string; tone: ChipTone }>; status: string }) {
  const s = map[status] ?? { label: status, tone: "neutral" as ChipTone };
  return (
    <Chip size="sm" dot tone={s.tone}>
      {s.label}
    </Chip>
  );
}

/** Active / inactive chip from a boolean plus the validity window. */
export function windowState(active: boolean, startsAt: string | null | undefined, endsAt: string | null | undefined, now = Date.now()): { key: "active" | "scheduled" | "ended" | "paused"; label: string; tone: ChipTone } {
  if (endsAt && Date.parse(endsAt) < now) return { key: "ended", label: "Ended", tone: "neutral" };
  if (!active) return { key: "paused", label: "Inactive", tone: "warn" };
  if (startsAt && Date.parse(startsAt) > now) return { key: "scheduled", label: "Scheduled", tone: "info" };
  return { key: "active", label: "Active", tone: "up" };
}

/* ------------------------------------------------------------------ */
/* States                                                               */
/* ------------------------------------------------------------------ */

const DOWN = ["unavailable", "not_configured", "bad_gateway", "network", "internal"];

export function MkError({ error, onRetry, className }: { error: ApiErr; onRetry?: () => void; className?: string }) {
  const friendly: ApiErr = DOWN.includes(error.code) ? { ...error, message: `The marketing service isn't reachable right now. ${error.code === "not_configured" ? "It isn't configured for this Back Office." : "Try again in a moment."}` } : error;
  return (
    <Card className={className}>
      <ErrorState error={friendly} onRetry={onRetry} />
    </Card>
  );
}

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

export function AuditNotice({ text }: { text?: string }) {
  return (
    <div className="flex items-start gap-2.5 rounded-[14px] border border-line bg-surface-2/60 px-3.5 py-2.5 text-[12px] text-fg-3">
      <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-up" />
      {text ?? "Written to the marketing audit log with your staff ID, the time and the before / after values."}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Form dialog: footer with Cancel + submit, error banner, busy state   */
/* ------------------------------------------------------------------ */

export function FormDialog<T>({
  open,
  onOpenChange,
  title,
  description,
  width = 720,
  side,
  submitLabel,
  submitTestId,
  submit,
  success,
  onDone,
  disabled,
  children,
  footerNote,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  title: React.ReactNode;
  description?: React.ReactNode;
  width?: number;
  side?: "right";
  submitLabel: string;
  submitTestId?: string;
  /** Returns a result, or a string (client-side validation error). */
  submit: () => Promise<WriteResult<T>> | string;
  success: string | ((d: T) => string);
  onDone?: (d: T) => void;
  disabled?: boolean;
  children: React.ReactNode;
  footerNote?: React.ReactNode;
}) {
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  React.useEffect(() => {
    if (open) {
      setError(null);
      setBusy(false);
    }
  }, [open]);
  const go = async () => {
    setError(null);
    const r = submit();
    if (typeof r === "string") {
      setError(r);
      return;
    }
    setBusy(true);
    const res = await r;
    setBusy(false);
    if (!res.ok) {
      const msg = errText(res.error);
      setError(msg);
      toast.error("Not saved", { description: msg });
      return;
    }
    toast.success(typeof success === "function" ? success(res.data) : success, { description: "Recorded in the marketing audit log" });
    onDone?.(res.data);
    onOpenChange(false);
  };
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      title={title}
      description={description}
      width={width}
      side={side}
      footer={
        <>
          {footerNote && <span className="mr-auto hidden text-[12px] text-fg-3 sm:block">{footerNote}</span>}
          <DialogClose asChild>
            <Button variant="ghost" size="sm" disabled={busy}>
              Cancel
            </Button>
          </DialogClose>
          <Button variant="ember" size="sm" disabled={busy || disabled} onClick={go} data-testid={submitTestId}>
            {busy ? "Saving…" : submitLabel}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        {children}
        <ErrorBanner error={error} />
      </div>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Action dialog: confirmation with an optional / required note          */
/* ------------------------------------------------------------------ */

export type Action<T = unknown> = {
  title: React.ReactNode;
  description?: React.ReactNode;
  body?: React.ReactNode;
  confirmLabel: string;
  confirmVariant?: "ember" | "buy" | "sell" | "surface";
  confirmTestId?: string;
  /** "required": 3–500 chars. "optional" (default): may be empty. "none": no text field. */
  note?: "required" | "optional" | "none";
  noteLabel?: string;
  placeholder?: string;
  run: (note: string) => Promise<WriteResult<T>>;
  success: string | ((d: T) => string);
  onDone?: (d: T) => void;
};

function ActionDialog({ action, open, onOpenChange }: { action: Action<any> | null; open: boolean; onOpenChange: (o: boolean) => void }) {
  const [text, setText] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const id = React.useId();
  React.useEffect(() => {
    if (open) {
      setText("");
      setError(null);
      setBusy(false);
    }
  }, [open, action]);
  if (!action) return null;
  const mode = action.note ?? "optional";
  const len = text.trim().length;
  const short = mode === "required" && len < 3;
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !busy && onOpenChange(o)}
      title={action.title}
      description={action.description}
      width={560}
      footer={
        <>
          <DialogClose asChild>
            <Button variant="ghost" size="sm" disabled={busy}>
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant={action.confirmVariant ?? "ember"}
            size="sm"
            data-testid={action.confirmTestId}
            disabled={short || len > 500 || busy}
            onClick={async () => {
              setBusy(true);
              setError(null);
              const res = await action.run(text.trim());
              setBusy(false);
              if (!res.ok) {
                const msg = errText(res.error);
                setError(msg);
                toast.error("Not done", { description: msg });
                return;
              }
              toast.success(typeof action.success === "function" ? action.success(res.data) : action.success, { description: "Recorded in the marketing audit log" });
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
          <div>
            <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
              <label htmlFor={id}>{action.noteLabel ?? (mode === "required" ? "Reason" : "Note")}</label>
              <span className={cn("text-[11px] font-normal", short ? "text-down" : "text-fg-3")}>{mode === "required" ? (short ? "Required · at least 3 characters" : `${len}/500`) : "Optional"}</span>
            </div>
            <textarea
              id={id}
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={3}
              maxLength={600}
              placeholder={action.placeholder ?? "Kept in the audit log."}
              className="w-full resize-none rounded-[14px] border border-line bg-surface-2 px-3.5 py-2.5 text-[13px] text-fg outline-none placeholder:text-fg-3 focus:border-ember/50 focus:ring-4 focus:ring-ember/10"
            />
          </div>
        )}
        <ErrorBanner error={error} />
        <AuditNotice />
      </div>
    </Dialog>
  );
}

/** const act = useAction(); act.ask({...}); {act.node} */
export function useAction() {
  const [action, setAction] = React.useState<Action<any> | null>(null);
  const [open, setOpen] = React.useState(false);
  const ask = React.useCallback(<T,>(a: Action<T>) => {
    setAction(a as Action<any>);
    setOpen(true);
  }, []);
  return { ask, node: <ActionDialog action={action} open={open} onOpenChange={setOpen} /> };
}

/** Small uppercase label / value tile. */
export function Tile({ label, value, sub, tone, className }: { label: React.ReactNode; value: React.ReactNode; sub?: React.ReactNode; tone?: "up" | "down" | "warn" | "gold" | "ember"; className?: string }) {
  const t = tone ? { up: "text-up", down: "text-down", warn: "text-warn", gold: "text-gold", ember: "text-ember" }[tone] : "text-fg";
  return (
    <div className={cn("k-row min-w-0 px-3.5 py-2.5", className)}>
      <div className="truncate text-[10.5px] font-medium uppercase tracking-[0.06em] text-fg-3">{label}</div>
      <div className={cn("k-num mt-1 truncate text-[14.5px] font-medium", t)}>{value}</div>
      {sub && <div className="mt-0.5 truncate text-[11px] text-fg-3">{sub}</div>}
    </div>
  );
}

/** Client name + id cell. */
export function ClientCell({ id, name, sub }: { id: number | null | undefined; name?: string | null; sub?: React.ReactNode }) {
  if (id === null || id === undefined) return <span className="text-fg-3">—</span>;
  return (
    <span className="block min-w-0 max-w-52">
      <span className="block truncate text-[13px] font-medium text-fg">{name?.trim() || `Client #${id}`}</span>
      <span className="block truncate font-mono text-[11px] text-fg-3">
        #{id}
        {sub ? <> · {sub}</> : null}
      </span>
    </span>
  );
}

"use client";

// Kalks 2 primitives (docs/design/KALKS2.md §5–§6). Looks live in styles.css (.k-btn, .k-chip, .k-field …); these
// components add the markup, the accessibility and the old prop names the pages still pass.

import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "../lib/cn";

/* ------------------------------------------------------------------ */
/* Button — NeoPOP block                                               */
/* ------------------------------------------------------------------ */

const BTN_VARIANT = {
  /** the one main action of a panel: Open account, Confirm, Start a challenge */
  primary: "k-btn-primary",
  /** Deposit, Ask AI, "new" offers */
  highlight: "k-btn-highlight",
  /** strong secondary: Trade, Close position */
  ink: "k-btn-ink",
  /** on photos only (hero CTAs) */
  white: "k-btn-white",
  /** Cancel, Try the demo, secondary links */
  ghost: "k-btn-ghost",
  buy: "k-btn-buy",
  sell: "k-btn-sell",
  /** quiet raised block (steppers, toolbars, table actions) */
  neutral: "k-btn-neutral",
  /** flat red tint */
  soft: "k-btn-soft",
  // names used before Kalks 2
  ember: "k-btn-primary",
  gold: "k-btn-highlight",
  surface: "k-btn-neutral",
  outline: "k-btn-ghost",
  "up-outline": "k-btn-soft-up",
  "down-outline": "k-btn-soft-down",
} as const;

const BTN_SIZE = {
  28: "k-btn-28",
  32: "k-btn-32",
  40: "",
  48: "k-btn-48",
  56: "k-btn-56",
  // names used before Kalks 2, mapped to the nearest size that keeps the founder's rule (no oversized buttons:
  // the old 42–44 px "lg" / "xl" become 40 + edge = 44, the old 52 px "xl" of the Back Office 48)
  xs: "k-btn-28",
  sm: "k-btn-32",
  md: "",
  lg: "",
  xl: "k-btn-48",
} as const;

export type ButtonVariant = keyof typeof BTN_VARIANT;
export type ButtonSize = keyof typeof BTN_SIZE;

/** Class list of a button (for links styled as buttons). */
export function buttonVariants({ variant, size, block, round, className }: { variant?: ButtonVariant | null; size?: ButtonSize | null; block?: boolean; round?: boolean; className?: string } = {}) {
  return cn("k-btn", BTN_VARIANT[variant ?? "neutral"], BTN_SIZE[size ?? 40], block && "k-btn-block", round && "k-btn-round", className);
}

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant | null;
  /** 32 · 40 · 48 · 56 (28 for dense tables); the old names xs / sm / md / lg / xl map to 28 / 32 / 40 / 40 / 48 */
  size?: ButtonSize | null;
  /** Busy: label hidden, three dots, same width, not pressable. */
  loading?: boolean;
  /** Leading icon (an <svg>); `iconEnd` trails the label. */
  icon?: React.ReactNode;
  iconEnd?: React.ReactNode;
  /** Full width (keeps the edge inside the row). */
  block?: boolean;
  /** Accepted for compatibility; renders nothing (the UI has no decorative looping motion). */
  shimmer?: boolean;
}

export const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant, size, loading, icon, iconEnd, block, shimmer: _shimmer, children, type = "button", disabled, ...props },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      disabled={disabled}
      aria-busy={loading || undefined}
      data-loading={loading ? "" : undefined}
      className={buttonVariants({ variant, size, block, className })}
      {...props}
    >
      {icon}
      {children}
      {iconEnd}
    </button>
  );
});

/* ------------------------------------------------------------------ */
/* Icon button                                                         */
/* ------------------------------------------------------------------ */

type IconButtonVariant = "default" | "ghost" | "glass" | "outline" | "primary" | "highlight" | "ink" | "neutral" | "buy" | "sell" | "white";
const ICB_SIZE = { sm: { s: 32, r: 10, ic: 16, btn: 32 }, md: { s: 40, r: 12, ic: 18, btn: 40 }, lg: { s: 44, r: 13, ic: 18, btn: 40 } } as const;

export type IconButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  size?: "sm" | "md" | "lg";
  /** Soft squares (default, ghost, glass, outline) or a round NeoPOP block (primary, highlight, ink, …). */
  variant?: IconButtonVariant;
  /** Selected tool: pastel yellow with a yellow hairline. */
  active?: boolean;
  /** Unread marker: a neon-red dot. */
  dot?: boolean;
};

/** Icon-only button. Always give it an `aria-label`. */
export const IconButton = React.forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { className, size = "md", variant = "default", active, dot, children, type = "button", style, ...props },
  ref,
) {
  const s = ICB_SIZE[size];
  if (variant !== "default" && variant !== "ghost" && variant !== "glass" && variant !== "outline") {
    return (
      <button ref={ref} type={type} className={cn(buttonVariants({ variant, size: s.btn as ButtonSize, round: true }), dot && "k-dot", className)} style={style} {...props}>
        {children}
      </button>
    );
  }
  return (
    <button
      ref={ref}
      type={type}
      data-variant={variant === "default" ? undefined : variant}
      data-active={active ? "" : undefined}
      aria-pressed={active === undefined ? undefined : active}
      className={cn("k-icb", dot && "k-dot", className)}
      style={{ "--s": `${s.s}px`, "--r": `${s.r}px`, "--ic": `${s.ic}px`, ...style } as React.CSSProperties}
      {...props}
    >
      {children}
    </button>
  );
});

/* ------------------------------------------------------------------ */
/* Chips, status chips, tags                                           */
/* ------------------------------------------------------------------ */

const chipTone = {
  neutral: "bg-surface-3 text-fg-2",
  up: "bg-up-soft text-up",
  down: "bg-down-soft text-down",
  red: "bg-red-soft text-red",
  yellow: "bg-yellow-soft text-yellow",
  ok: "bg-ok-soft text-ok",
  warn: "bg-warn-soft text-warn",
  info: "bg-up-soft text-up",
  solid: "bg-fg text-bg",
  ink: "bg-ink text-ink-fg",
  // names used before Kalks 2
  ember: "bg-red-soft text-red",
  gold: "bg-yellow-soft text-yellow",
} as const;
export type ChipTone = keyof typeof chipTone;

/**
 * A pill. Static (status, count, label) by default: 24 px, or 20 px at `size="sm"`. With `onClick` it is a filter
 * chip (30 px, `selected` = ink). `dot` adds the 6 px status dot.
 */
export function Chip({
  tone = "neutral",
  dot,
  className,
  children,
  size,
  selected,
  onClick,
  variant,
  title,
}: {
  tone?: ChipTone;
  dot?: boolean;
  className?: string;
  children: React.ReactNode;
  size?: "sm" | "md" | "lg";
  selected?: boolean;
  onClick?: () => void;
  variant?: "outline";
  title?: string;
}) {
  if (onClick) {
    return (
      <button type="button" aria-pressed={!!selected} onClick={onClick} title={title} data-variant={variant} className={cn("k-chip", size === "md" && "h-7 px-3 text-[12.5px]", className)}>
        {dot && <span className="size-1.5 rounded-full bg-current" />}
        {children}
      </button>
    );
  }
  if (size === "lg") {
    return (
      <span title={title} data-variant={variant} className={cn("k-chip", selected && "!bg-fg !text-bg", className)}>
        {children}
      </span>
    );
  }
  return (
    <span title={title} data-size={size === "sm" ? "sm" : undefined} className={cn("k-status k-num", dot && "k-status-dot", chipTone[tone], className)}>
      {children}
    </span>
  );
}

const STATUS: Record<string, { tone: ChipTone; label: string }> = {
  completed: { tone: "ok", label: "Completed" },
  approved: { tone: "ok", label: "Approved" },
  verified: { tone: "ok", label: "Verified" },
  active: { tone: "ok", label: "Active" },
  passed: { tone: "ok", label: "Passed" },
  paid: { tone: "ok", label: "Paid" },
  resolved: { tone: "ok", label: "Resolved" },
  pending: { tone: "warn", label: "Pending" },
  review: { tone: "warn", label: "In review" },
  open: { tone: "warn", label: "Open" },
  processing: { tone: "info", label: "Processing" },
  scheduled: { tone: "info", label: "Scheduled" },
  running: { tone: "yellow", label: "Running" },
  accruing: { tone: "yellow", label: "Accruing" },
  rejected: { tone: "down", label: "Rejected" },
  failed: { tone: "down", label: "Failed" },
  stopped: { tone: "down", label: "Stopped" },
  suspended: { tone: "down", label: "Suspended" },
  expired: { tone: "neutral", label: "Expired" },
  draft: { tone: "neutral", label: "Draft" },
  paused: { tone: "neutral", label: "Paused" },
};

/** Status chip: green only for done states, yellow pending, red rejected, blue in progress. */
export function StatusChip({ status, label }: { status: string; label?: string }) {
  const s = STATUS[status] ?? { tone: "neutral" as ChipTone, label: status };
  return (
    <Chip tone={s.tone} dot>
      {label ?? s.label}
    </Chip>
  );
}

export type TagTone = "neutral" | "live" | "demo" | "cfd" | "options" | "new";

/** 20 px caps tag (the Badge): LIVE outlined ink, DEMO grey, CFD ink, OPTIONS yellow, NEW red. */
export function Tag({ tone = "neutral", className, children }: { tone?: TagTone; className?: string; children: React.ReactNode }) {
  return (
    <span data-tone={tone === "neutral" || tone === "demo" ? undefined : tone} className={cn("k-tag", className)}>
      {children}
    </span>
  );
}
/** Alias of Tag. */
export const Badge = Tag;

/* ------------------------------------------------------------------ */
/* Cards                                                               */
/* ------------------------------------------------------------------ */

export type CardProps = React.HTMLAttributes<HTMLDivElement> & {
  /** solid (default): s1, radius 24, e1 · glass: frosted, only over a photo or the blooms · photo: an image card */
  variant?: "solid" | "glass" | "photo";
  /** photo card: the image and its focal point */
  image?: string;
  imagePosition?: string;
  /** Before Kalks 2: the ember "hot" card; now a red-tinted card. */
  hot?: boolean;
};

export function Card({ className, hot, variant = "solid", image, imagePosition = "50% 30%", style, children, ...props }: CardProps) {
  if (variant === "photo") {
    return (
      <div
        className={cn("relative isolate overflow-hidden rounded-[var(--radius-card)] bg-k-black text-white", className)}
        style={{ backgroundImage: image ? `url(${image})` : undefined, backgroundSize: "cover", backgroundPosition: imagePosition, ...style }}
        {...props}
      >
        <div aria-hidden className="absolute inset-0 -z-10 bg-[linear-gradient(180deg,rgba(12,3,4,0)_30%,rgba(12,3,4,.72)_100%)]" />
        {children}
      </div>
    );
  }
  return (
    <div className={cn(hot ? "k-hot-card rounded-[var(--radius-card)]" : variant === "glass" ? "k-glass relative rounded-[var(--radius-card)]" : "k-card", className)} style={style} {...props}>
      {children}
    </div>
  );
}

export function CardHeader({
  title,
  subtitle,
  icon,
  action,
  className,
}: {
  title: React.ReactNode;
  subtitle?: React.ReactNode;
  icon?: React.ReactNode;
  action?: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-wrap items-start justify-between gap-x-4 gap-y-3 px-6 pt-5", className)}>
      <div className="flex min-w-0 items-center gap-3">
        {icon && <span className="grid size-[38px] shrink-0 place-items-center rounded-[12px] bg-surface-3 text-fg-2 [&_svg]:size-[18px]">{icon}</span>}
        <div className="min-w-0">
          <h3 className="k-title truncate text-[19px] text-fg">{title}</h3>
          {subtitle && <p className="mt-1 line-clamp-2 text-[13px] leading-snug text-fg-3">{subtitle}</p>}
        </div>
      </div>
      {action && <div className="flex flex-wrap items-center gap-2">{action}</div>}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Form controls                                                       */
/* ------------------------------------------------------------------ */

type FieldBoxProps = { leading?: React.ReactNode; trailing?: React.ReactNode; invalid?: boolean; fieldSize?: "sm" | "md" };

export const Input = React.forwardRef<HTMLInputElement, React.InputHTMLAttributes<HTMLInputElement> & FieldBoxProps & { inputClassName?: string }>(function Input(
  { className, leading, trailing, inputClassName, invalid, fieldSize, disabled, ...props },
  ref,
) {
  return (
    <div className={cn("k-field", className)} data-invalid={invalid ? "" : undefined} data-disabled={disabled ? "" : undefined} data-size={fieldSize === "sm" ? "sm" : undefined}>
      {leading && <span className="k-field-affix">{leading}</span>}
      <input ref={ref} disabled={disabled} aria-invalid={invalid || undefined} className={inputClassName} {...props} />
      {trailing && <span className="k-field-affix">{trailing}</span>}
    </div>
  );
});

/** Native select in the field look (keyboard and screen readers get the platform's own list). */
export const Select = React.forwardRef<
  HTMLSelectElement,
  React.SelectHTMLAttributes<HTMLSelectElement> & FieldBoxProps & { options?: readonly { value: string; label: React.ReactNode; disabled?: boolean }[]; selectClassName?: string }
>(function Select({ className, leading, invalid, fieldSize, options, children, selectClassName, disabled, ...props }, ref) {
  return (
    <div className={cn("k-field relative", className)} data-invalid={invalid ? "" : undefined} data-disabled={disabled ? "" : undefined} data-size={fieldSize === "sm" ? "sm" : undefined}>
      {leading && <span className="k-field-affix">{leading}</span>}
      <select ref={ref} disabled={disabled} aria-invalid={invalid || undefined} className={selectClassName} {...props}>
        {options?.map((o) => (
          <option key={o.value} value={o.value} disabled={o.disabled}>
            {o.label as string}
          </option>
        ))}
        {children}
      </select>
      <ChevronDown aria-hidden className="pointer-events-none absolute end-3 size-4 text-fg-3" />
    </div>
  );
});

export const Textarea = React.forwardRef<HTMLTextAreaElement, React.TextareaHTMLAttributes<HTMLTextAreaElement> & { invalid?: boolean; textareaClassName?: string }>(function Textarea(
  { className, invalid, textareaClassName, disabled, ...props },
  ref,
) {
  return (
    <div className={cn("k-field k-field-textarea", className)} data-invalid={invalid ? "" : undefined} data-disabled={disabled ? "" : undefined}>
      <textarea ref={ref} disabled={disabled} aria-invalid={invalid || undefined} className={textareaClassName} {...props} />
    </div>
  );
});

/**
 * Label above (12.5 / 600), the control, then a hint or the error (which says what to do). Pass `htmlFor` with the
 * control's id to tie the label to it; without it the group is labelled for assistive tech.
 */
export function Field({ label, hint, error, children, className, htmlFor, aside }: { label: React.ReactNode; hint?: React.ReactNode; error?: string; children: React.ReactNode; className?: string; htmlFor?: string; aside?: React.ReactNode }) {
  const id = React.useId();
  const Label = htmlFor ? "label" : "span";
  return (
    <div role={htmlFor ? undefined : "group"} aria-labelledby={htmlFor ? undefined : `${id}-l`} className={cn("flex flex-col gap-[7px]", className)}>
      <span className="flex items-center justify-between gap-3">
        <Label id={`${id}-l`} htmlFor={htmlFor} className="text-[12.5px] font-semibold leading-tight text-fg-2">
          {label}
        </Label>
        {aside && <span className="text-[12.5px] text-fg-3">{aside}</span>}
      </span>
      {children}
      {error ? (
        <span role="alert" className="text-[12.5px] leading-snug text-red">
          {error}
        </span>
      ) : (
        hint && <span className="text-[12.5px] leading-snug text-fg-3">{hint}</span>
      )}
    </div>
  );
}

export function Kbd({ children }: { children: React.ReactNode }) {
  return <kbd className="k-kbd">{children}</kbd>;
}

export function Skeleton({ className }: { className?: string }) {
  return <div className={cn("animate-pulse rounded-lg bg-surface-3", className)} />;
}

export function Divider({ className, vertical }: { className?: string; vertical?: boolean }) {
  return <div role="separator" aria-orientation={vertical ? "vertical" : "horizontal"} className={cn(vertical ? "w-px self-stretch bg-line" : "h-px w-full bg-line", className)} />;
}

const PROGRESS_FILL = { accent: "bg-[var(--k-tog-on)]", up: "bg-up", down: "bg-down", ok: "bg-ok", warn: "bg-warn", red: "bg-red", yellow: "bg-yellow", ember: "bg-[var(--k-tog-on)]", gold: "bg-yellow" } as const;

/** Meter: sunken track, fill (ink in light / yellow in dark by default; `up` = blue for Prop), optional ink target tick. */
export function Progress({ value, tone = "accent", size = "sm", target, label, className }: { value: number; tone?: keyof typeof PROGRESS_FILL; size?: "sm" | "md"; target?: number; label?: string; className?: string }) {
  const v = Math.min(100, Math.max(0, value));
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(v)}
      className={cn("relative w-full rounded-full bg-surface-4 shadow-[inset_0_1px_2px_rgba(0,0,0,.18)]", size === "md" ? "h-2.5" : "h-1.5", className)}
    >
      <div className={cn("h-full rounded-full transition-[width] duration-300", PROGRESS_FILL[tone])} style={{ width: `${v}%` }} />
      {target !== undefined && <i aria-hidden className="absolute -top-1 h-[calc(100%+8px)] w-[3px] -translate-x-1/2 rounded-sm bg-fg" style={{ left: `${Math.min(100, Math.max(0, target))}%` }} />}
    </div>
  );
}

/** Switch: 46 × 28 sunken track (sm 40 × 24), white puck on a 2 px edge; on = ink (light) / yellow (dark). */
export function Toggle({ checked, onChange, label, disabled, size, id, className }: { checked: boolean; onChange: (v: boolean) => void; label?: string; disabled?: boolean; size?: "sm" | "md"; id?: string; className?: string }) {
  return (
    <button
      id={id}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      data-size={size === "sm" ? "sm" : undefined}
      onClick={() => onChange(!checked)}
      className={cn("k-tog", className)}
    />
  );
}

/**
 * Slider: 6 px sunken track, fill, 24 px puck on a 3 px edge and an ink value bubble. A native range input sits on
 * top, so arrows / Page / Home / End, pointer drag and screen readers all work as usual.
 */
export function Slider({
  value,
  onChange,
  min = 0,
  max = 100,
  step = 1,
  label,
  format = (v) => String(v),
  bubble = true,
  ticks,
  disabled,
  className,
}: {
  value: number;
  onChange: (v: number) => void;
  min?: number;
  max?: number;
  step?: number;
  /** Accessible name. */
  label: string;
  format?: (v: number) => string;
  bubble?: boolean;
  ticks?: readonly React.ReactNode[];
  disabled?: boolean;
  className?: string;
}) {
  const pct = max > min ? ((Math.min(max, Math.max(min, value)) - min) / (max - min)) * 100 : 0;
  return (
    <div className={cn(bubble && "pt-8", className)}>
      {/* numbers run left to right in every language (like the prices), so the track does too */}
      <div dir="ltr" className={cn("k-slider", disabled && "opacity-50")}>
        <span className="k-slider-track" />
        <span className="k-slider-fill" style={{ width: `${pct}%` }} />
        <input type="range" min={min} max={max} step={step} value={value} disabled={disabled} aria-label={label} aria-valuetext={format(value)} onChange={(e) => onChange(Number(e.target.value))} />
        <span className="k-slider-puck" style={{ left: `${pct}%` }} />
        {bubble && (
          <span className="k-slider-bubble" style={{ left: `${pct}%` }} aria-hidden>
            {format(value)}
          </span>
        )}
      </div>
      {ticks && (
        <div dir="ltr" className="k-slider-ticks" aria-hidden>
          {ticks.map((t, i) => (
            <span key={i}>{t}</span>
          ))}
        </div>
      )}
    </div>
  );
}

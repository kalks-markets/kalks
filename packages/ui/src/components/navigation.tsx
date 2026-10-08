"use client";

import * as React from "react";
import { motion, useReducedMotion } from "motion/react";
import { Check } from "lucide-react";
import { cn } from "../lib/cn";

const THUMB = { type: "tween", duration: 0.18, ease: [0.2, 0.8, 0.2, 1] } as const;

/** Arrow / Home / End navigation for a row of items (left-to-right and right-to-left). */
function useRovingKeys(count: number, index: number, select: (i: number) => void) {
  return (e: React.KeyboardEvent<HTMLElement>) => {
    const rtl = getComputedStyle(e.currentTarget).direction === "rtl";
    const next = { ArrowRight: rtl ? -1 : 1, ArrowLeft: rtl ? 1 : -1, ArrowDown: 1, ArrowUp: -1 }[e.key];
    let to = -1;
    if (next !== undefined) to = (index + next + count) % count;
    else if (e.key === "Home") to = 0;
    else if (e.key === "End") to = count - 1;
    if (to < 0) return;
    e.preventDefault();
    select(to);
    const items = e.currentTarget.querySelectorAll<HTMLElement>("[data-roving]");
    items[to]?.focus();
  };
}

/* ------------------------------------------------------------------ */
/* Segmented control                                                   */
/* ------------------------------------------------------------------ */

type SegSize = 26 | 30 | 36 | "xs" | "sm" | "md";
const SEG_SIZE: Record<string, "sm" | undefined | "lg"> = { 26: "sm", 30: undefined, 36: "lg", xs: "sm", sm: undefined, md: "lg" };

/**
 * Market | Limit | Stop, timeframes, All | Live | Demo, the theme switch: a sunken track whose selected item is
 * raised on a 2 px NeoPOP edge. A radio group: arrows move the selection.
 */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  size = 30,
  block,
  className,
  "aria-label": ariaLabel,
}: {
  options: readonly (T | { value: T; label: React.ReactNode; title?: string })[];
  value: T;
  onChange: (v: T) => void;
  /** item height 26 · 30 · 36 (xs / sm / md are the old names) */
  size?: SegSize;
  block?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  const id = React.useId();
  const reduce = useReducedMotion();
  const items = options.map((o) => (typeof o === "string" ? { value: o, label: o as React.ReactNode, title: undefined } : o));
  const index = Math.max(0, items.findIndex((o) => o.value === value));
  const onKeyDown = useRovingKeys(items.length, index, (i) => onChange(items[i]!.value));
  return (
    <div role="radiogroup" aria-label={ariaLabel} data-size={SEG_SIZE[size]} data-block={block ? "" : undefined} onKeyDown={onKeyDown} className={cn("k-seg", className)}>
      {items.map((o) => {
        const on = o.value === value;
        return (
          <button key={o.value} type="button" role="radio" aria-checked={on} tabIndex={on ? 0 : -1} data-roving="" title={o.title} onClick={() => onChange(o.value)} className="k-seg-item">
            {on && <motion.span layoutId={`seg-${id}`} className="k-seg-thumb" transition={reduce ? { duration: 0 } : THUMB} />}
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Tabs                                                                */
/* ------------------------------------------------------------------ */

/**
 * underline: in-card sections (ink / yellow underline) · pill: panel tabs with count badges (Trader positions,
 * activity) · glass: pill tabs inside a photo hero. A tablist with automatic activation; the page renders the panel.
 */
export function Tabs<T extends string>({
  tabs,
  value,
  onChange,
  variant = "underline",
  className,
  "aria-label": ariaLabel,
  idPrefix,
}: {
  tabs: readonly { value: T; label: React.ReactNode; count?: number }[];
  value: T;
  onChange: (v: T) => void;
  variant?: "underline" | "pill" | "glass";
  className?: string;
  "aria-label"?: string;
  /** When set, tab i controls the element with id `${idPrefix}-panel-${value}`. */
  idPrefix?: string;
}) {
  const id = React.useId();
  const reduce = useReducedMotion();
  const index = Math.max(0, tabs.findIndex((t) => t.value === value));
  const onKeyDown = useRovingKeys(tabs.length, index, (i) => onChange(tabs[i]!.value));
  const wrap =
    variant === "underline"
      ? "flex items-center gap-6 border-b border-line"
      : variant === "pill"
        ? "flex items-center gap-1"
        : "inline-flex gap-0.5 rounded-[14px] bg-[rgba(14,6,7,.42)] p-1 shadow-[inset_0_0_0_1px_rgba(255,255,255,.12)] backdrop-blur-[18px]";
  return (
    <div role="tablist" aria-label={ariaLabel} onKeyDown={onKeyDown} className={cn(wrap, className)}>
      {tabs.map((t) => {
        const on = t.value === value;
        const item =
          variant === "underline"
            ? cn("relative -mb-px flex items-center gap-2 pb-3 text-[14px] font-semibold transition-colors", on ? "text-fg" : "text-fg-3 hover:text-fg-2")
            : variant === "pill"
              ? cn("flex h-7 items-center rounded-[8px] px-2.5 text-[13px] font-semibold transition-colors", on ? "bg-surface-3 text-fg" : "text-fg-2 hover:text-fg")
              : cn("h-[30px] rounded-[10px] px-[13px] text-[13px] font-semibold transition-colors", on ? "bg-white/95 text-k-black" : "text-white/75 hover:text-white");
        return (
          <button
            key={t.value}
            type="button"
            role="tab"
            aria-selected={on}
            aria-controls={idPrefix ? `${idPrefix}-panel-${t.value}` : undefined}
            id={idPrefix ? `${idPrefix}-tab-${t.value}` : undefined}
            tabIndex={on ? 0 : -1}
            data-roving=""
            onClick={() => onChange(t.value)}
            className={item}
          >
            {t.label}
            {t.count !== undefined && (
              <span className={cn("k-num ms-1.5 rounded-[5px] px-[5px] py-0.5 font-mono text-[11px] font-semibold leading-none", variant === "underline" && on ? "bg-yellow-soft text-fg" : "bg-surface-4 text-fg-2")}>{t.count}</span>
            )}
            {variant === "underline" && on && <motion.span layoutId={`tab-${id}`} className="absolute inset-x-0 -bottom-px h-0.5 rounded-full bg-[var(--k-accent-line)]" transition={reduce ? { duration: 0 } : THUMB} />}
          </button>
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Stepper (wizards: open account, KYC, withdrawal)                    */
/* ------------------------------------------------------------------ */

export function Stepper({ steps, current, className }: { steps: string[]; current: number; className?: string }) {
  return (
    <ol className={cn("flex items-center gap-3", className)}>
      {steps.map((s, i) => {
        const done = i < current;
        const on = i === current;
        return (
          <li key={s} aria-current={on ? "step" : undefined} className="flex flex-1 items-center gap-3">
            <span
              className={cn(
                "grid size-7 shrink-0 place-items-center rounded-full text-xs font-bold k-num transition-colors",
                done && "bg-fg text-bg",
                on && "bg-yellow text-on-yellow shadow-[1px_1px_0_var(--k-yellow-edge),2px_2px_0_var(--k-yellow-edge)]",
                !done && !on && "text-fg-3 shadow-[inset_0_0_0_1.5px_var(--k-border-2)]",
              )}
            >
              {done ? <Check className="size-3.5" strokeWidth={2.75} /> : i + 1}
            </span>
            <span className={cn("hidden text-[13px] font-semibold md:inline", on ? "text-fg" : "text-fg-3")}>{s}</span>
            {i < steps.length - 1 && <span className={cn("h-px flex-1", done ? "bg-fg/40" : "bg-line")} />}
          </li>
        );
      })}
    </ol>
  );
}

/* ------------------------------------------------------------------ */
/* Choice cards (open-account wizard: CFD account | Options account)  */
/* ------------------------------------------------------------------ */

/**
 * Big radio cards: s2 cards with a radio dot; the selected one is s1 with a 1.5 px ink ring, a 2 px ink edge and a
 * filled dot. A radio group: arrows move the selection.
 */
export function ChoiceCards<T extends string>({
  options,
  value,
  onChange,
  columns = 2,
  className,
  "aria-label": ariaLabel,
}: {
  options: readonly { value: T; title: React.ReactNode; text?: React.ReactNode; icon?: React.ReactNode; disabled?: boolean }[];
  value: T | null;
  onChange: (v: T) => void;
  columns?: 1 | 2 | 3;
  className?: string;
  "aria-label"?: string;
}) {
  const index = Math.max(0, options.findIndex((o) => o.value === value));
  const onKeyDown = useRovingKeys(options.length, index, (i) => !options[i]!.disabled && onChange(options[i]!.value));
  return (
    <div role="radiogroup" aria-label={ariaLabel} onKeyDown={onKeyDown} className={cn("grid gap-2.5", columns === 1 ? "grid-cols-1" : columns === 3 ? "grid-cols-1 sm:grid-cols-3" : "grid-cols-1 sm:grid-cols-2", className)}>
      {options.map((o, i) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            disabled={o.disabled}
            tabIndex={on || (value === null && i === 0) ? 0 : -1}
            data-roving=""
            onClick={() => onChange(o.value)}
            className={cn(
              "flex flex-col items-start gap-1 rounded-[16px] p-3.5 text-start transition-[background-color,box-shadow] duration-150 disabled:cursor-not-allowed disabled:opacity-50",
              on ? "bg-surface shadow-[inset_0_0_0_1.5px_var(--k-fg),2px_2px_0_var(--k-fg)]" : "bg-surface-2 shadow-[inset_0_0_0_1.5px_var(--k-border-2)] hover:bg-surface-3",
            )}
          >
            <span className="mb-2 flex w-full items-center justify-between">
              <span aria-hidden className={cn("size-[18px] rounded-full transition-shadow", on ? "shadow-[inset_0_0_0_5.5px_var(--k-fg)]" : "shadow-[inset_0_0_0_1.5px_var(--k-border-2)]")} />
              {o.icon && <span className="text-fg-2 [&_svg]:size-[18px]">{o.icon}</span>}
            </span>
            <b className="text-[14px] font-bold leading-tight text-fg">{o.title}</b>
            {o.text && <span className="text-[12.5px] leading-snug text-fg-3">{o.text}</span>}
          </button>
        );
      })}
    </div>
  );
}

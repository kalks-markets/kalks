"use client";

// Page header and KPI card of the Client Area: a big friendly display title with a grey line under it, and KPI
// cards with a thin coloured accent bar, a large bold value and a small change chip (the pastel dashboard look).
// Same props as the @kalks/ui versions.

import * as React from "react";
import Link from "next/link";
import { ArrowDownRight, ArrowUpRight, MoreHorizontal } from "lucide-react";
import { cn } from "@kalks/ui";
import type { ChipTone } from "./primitives";

/** Page title block: title + subtitle + actions. */
export function PageHeader({ title, subtitle, actions, className }: { title: React.ReactNode; subtitle?: React.ReactNode; actions?: React.ReactNode; className?: string }) {
  return (
    <div style={{ "--k-reveal-y": "10px", "--k-reveal-ms": "500ms" } as React.CSSProperties} className={cn("k-reveal mb-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between", className)}>
      <div className="min-w-0">
        <h1 className="k-display text-[26px] font-semibold leading-[1.15] tracking-[-0.025em] text-fg sm:text-[32px]">{title}</h1>
        {subtitle && <div className="mt-1.5 text-[14.5px] leading-relaxed text-fg-2">{subtitle}</div>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

const BAR: Partial<Record<ChipTone, string>> = { down: "var(--k-down)", gold: "var(--k-gold)", warn: "var(--k-warn)", info: "var(--k-info)" };
const TILE: Partial<Record<ChipTone, string>> = { down: "coral", gold: "amber", warn: "amber", info: "sky", up: "mint" };

const SOFT: Record<ChipTone, string> = {
  neutral: "bg-surface-3 text-fg-3",
  up: "bg-up-soft text-up",
  down: "bg-down-soft text-down",
  ember: "bg-ember-soft text-ember",
  gold: "bg-gold-soft text-gold",
  warn: "bg-warn-soft text-warn",
  info: "bg-info-soft text-info",
  solid: "bg-[var(--k-ink)] text-[var(--k-ink-fg)]",
};

/** Change chip in the reference style: a small tinted circle with an arrow, then the figure. */
export function ChangeChip({ tone = "neutral", children, className }: { tone?: ChipTone; children: React.ReactNode; className?: string }) {
  const down = tone === "down";
  const Icon = down ? ArrowDownRight : ArrowUpRight;
  const soft = SOFT[tone];
  return (
    <span className={cn("k-num inline-flex min-w-0 items-center gap-1.5 text-[12.5px] font-semibold text-fg-2", className)}>
      <span className={cn("grid size-5 shrink-0 place-items-center rounded-full", soft)}>
        {tone === "neutral" ? <span className="size-1.5 rounded-full bg-current" /> : <Icon className="size-3" strokeWidth={2.5} />}
      </span>
      <span className="truncate">{children}</span>
    </span>
  );
}

export function KpiCard({
  label,
  value,
  icon,
  chip,
  chipTone = "neutral",
  href,
  hot,
  footer,
  className,
  delay = 0,
  accent,
}: {
  label: string;
  value: React.ReactNode;
  icon?: React.ReactNode;
  chip?: React.ReactNode;
  chipTone?: ChipTone;
  href?: string;
  /** Accepted for compatibility (unused). */
  illustration?: string;
  hot?: boolean;
  footer?: React.ReactNode;
  className?: string;
  delay?: number;
  /** Colour of the accent bar (defaults from the chip tone; the brand colour otherwise). */
  accent?: string;
}) {
  // founder 2026-10-10: every page's figures look like the Home balance strip — a label, a big thin number, a small
  // note; a row of these joins into one glass strip with hairline dividers (app/kx-orange.css, .k-kpi)
  void accent;
  void TILE;
  void BAR;
  return (
    <div className={cn("k-kpi k-reveal min-w-0", className)} style={{ "--k-reveal-y": "14px", "--k-reveal-ms": "500ms", ...(delay ? { "--k-reveal-delay": `${delay}s` } : {}) } as React.CSSProperties}>
      <div className={cn("k-card relative flex h-full flex-col px-5 pb-5 pt-5 [container-type:inline-size] sm:px-6", hot && "k-card-hot")}>
        <div className="flex min-h-6 items-center justify-between gap-3">
          <span className="flex min-w-0 items-center gap-2 text-[11px] font-semibold uppercase tracking-[0.1em] text-fg-3">
            {icon && <span className="shrink-0 text-ember [&_svg]:size-3.5">{icon}</span>}
            <span className="truncate">{label}</span>
          </span>
          {href && (
            <Link href={href} className="-me-1.5 grid size-7 shrink-0 place-items-center rounded-full text-fg-3 transition-colors hover:bg-surface-3 hover:text-fg" aria-label={label}>
              <ArrowUpRight className="size-4" />
            </Link>
          )}
        </div>
        <div className="k-num mt-3 min-w-0 whitespace-nowrap text-[clamp(18px,12cqw,28px)] font-light leading-none tracking-[-0.03em] text-fg">{value}</div>
        {(footer || chip) && <div className="mt-3 flex min-w-0 flex-wrap items-center gap-2 text-[12px] text-fg-3">{footer ?? <ChangeChip tone={chipTone}>{chip}</ChangeChip>}</div>}
      </div>
    </div>
  );
}

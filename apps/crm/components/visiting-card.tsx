"use client";

// Visiting cards (founder 2026-10-10: "MAM, copy trades, partners: visiting-card style"): each master, fund, money
// manager and the partner's own card is a business card (3.5 × 2 in), with a monogram, the name in caps, a title line,
// a short accent rule and up to four figures along the bottom. Finishes: black (copy masters), ivory (PAMM funds),
// graphite (MAM managers), orange (partner).

import * as React from "react";
import { LogoMark, cn } from "@/components/kit";

export type VcFinish = "black" | "ivory" | "graphite" | "orange";
export type VcStat = { label: React.ReactNode; value: React.ReactNode; tone?: "up" | "down" };

const FINISH: Record<VcFinish, { card: string; muted: string; rule: string; up: string; down: string; mono: string }> = {
  black: {
    card: "bg-[linear-gradient(150deg,#1d1d21,#0c0c0e_55%,#000)] text-white",
    muted: "text-white/55",
    rule: "bg-[#f2600c]",
    up: "text-[#4ade80]",
    down: "text-[#ff6b5b]",
    mono: "border-white/25 text-white",
  },
  ivory: {
    card: "bg-[linear-gradient(150deg,#fbf8f1,#f1eadb_60%,#e6dcc7)] text-[#1b1712]",
    muted: "text-[#1b1712]/55",
    rule: "bg-[#f2600c]",
    up: "text-[#137a3d]",
    down: "text-[#b3261e]",
    mono: "border-[#1b1712]/25 text-[#1b1712]",
  },
  graphite: {
    card: "bg-[linear-gradient(150deg,#55555d,#2a2a2f_50%,#141416)] text-white",
    muted: "text-white/60",
    rule: "bg-[#ffa040]",
    up: "text-[#4ade80]",
    down: "text-[#ff8a7a]",
    mono: "border-white/30 text-white",
  },
  orange: {
    card: "bg-[linear-gradient(150deg,#ff8a3d,#f2600c_50%,#a63f04)] text-white",
    muted: "text-white/75",
    rule: "bg-white",
    up: "text-white",
    down: "text-white",
    mono: "border-white/40 text-white",
  },
};

export function initials(name: string) {
  const parts = name.replace(/[^\p{L}\p{N} ]/gu, " ").trim().split(/\s+/).filter(Boolean);
  if (!parts.length) return "K";
  return (parts.length === 1 ? parts[0]!.slice(0, 2) : parts[0]![0]! + parts[1]![0]!).toUpperCase();
}

export function VisitingCard({
  finish = "black",
  name,
  title,
  kicker,
  corner,
  badges,
  stats,
  footer,
  className,
}: {
  finish?: VcFinish;
  name: string;
  /** the line under the name (strategy, fund manager, partner level) */
  title?: React.ReactNode;
  /** small caps line at the top (COPY TRADING MASTER, PAMM FUND…) */
  kicker?: React.ReactNode;
  /** top-right corner (a tick to compare, a status) */
  corner?: React.ReactNode;
  badges?: React.ReactNode;
  stats?: VcStat[];
  /** replaces the figures row (the partner's referral link) */
  footer?: React.ReactNode;
  className?: string;
}) {
  const f = FINISH[finish];
  return (
    <div className={cn("relative aspect-[1.75/1] w-full overflow-hidden rounded-[14px] shadow-[inset_0_1px_0_rgba(255,255,255,.14),0_22px_44px_-26px_rgba(0,0,0,.9)] [container-type:inline-size]", f.card, className)}>
      {/* card stock: a soft light from the top and a fine grain */}
      <div aria-hidden className="pointer-events-none absolute inset-0 bg-[radial-gradient(110%_80%_at_15%_0%,rgba(255,255,255,.14),transparent_55%)]" />
      <div aria-hidden className="pointer-events-none absolute inset-0 opacity-[.06] [background-image:repeating-linear-gradient(0deg,currentColor_0_1px,transparent_1px_3px)]" />
      {/* the brand mark, embossed in the corner */}
      <LogoMark size={120} className="pointer-events-none absolute -bottom-[6cqw] -end-[5cqw] size-[34cqw] opacity-[.07]" />

      <div className="relative flex h-full flex-col p-[5.5cqw]">
        <div className="flex items-start gap-[3.5cqw]">
          <span className={cn("grid size-[13cqw] shrink-0 place-items-center rounded-full border-[1.5px] font-semibold tracking-[0.04em] text-[clamp(12px,4.6cqw,20px)]", f.mono)}>{initials(name)}</span>
          <div className="min-w-0 flex-1 pt-[0.6cqw]">
            {kicker && <div className={cn("truncate text-[clamp(8px,2.5cqw,10.5px)] font-bold uppercase tracking-[0.2em]", f.muted)}>{kicker}</div>}
            <div className="k-display mt-[0.6cqw] truncate text-[clamp(15px,6cqw,24px)] font-bold uppercase leading-[1.05] tracking-[0.03em]">{name}</div>
            {title && <div className={cn("mt-[0.8cqw] truncate text-[clamp(10.5px,3.3cqw,13.5px)]", f.muted)}>{title}</div>}
          </div>
          {corner && <div className="shrink-0">{corner}</div>}
        </div>
        <div className={cn("ms-[16.5cqw] mt-[2.6cqw] h-[2px] w-[9cqw] rounded-full", f.rule)} />
        {badges && <div className="ms-[16.5cqw] mt-[2.4cqw] flex min-w-0 flex-wrap gap-1">{badges}</div>}
        <div className="mt-auto">
          {footer ??
            (stats && stats.length > 0 && (
              <div className="grid gap-[3cqw]" style={{ gridTemplateColumns: `repeat(${stats.length}, minmax(0, 1fr))` }}>
                {stats.map((s, i) => (
                  <div key={i} className="min-w-0">
                    <div className={cn("k-num truncate text-[clamp(12px,4.4cqw,18px)] font-semibold leading-tight", s.tone === "up" ? f.up : s.tone === "down" ? f.down : "")}>{s.value}</div>
                    <div className={cn("mt-[0.6cqw] truncate text-[clamp(7.5px,2.3cqw,9.5px)] font-semibold uppercase tracking-[0.14em]", f.muted)}>{s.label}</div>
                  </div>
                ))}
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}

/** a small pill on a visiting card (house master, PAMM, closed) */
export function VcBadge({ children, finish = "black" }: { children: React.ReactNode; finish?: VcFinish }) {
  return (
    <span className={cn("rounded-full px-2 py-0.5 text-[clamp(8px,2.3cqw,10px)] font-bold uppercase tracking-[0.08em]", finish === "ivory" ? "bg-[#1b1712]/8 text-[#1b1712]/75" : "bg-white/14 text-white/85")}>
      {children}
    </span>
  );
}

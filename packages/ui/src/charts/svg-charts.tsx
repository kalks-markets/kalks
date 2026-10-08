"use client";

import * as React from "react";
import { motion } from "motion/react";
import { cn } from "../lib/cn";

/* ------------------------------------------------------------------ */
/* Sparkline                                                           */
/* ------------------------------------------------------------------ */

export function Sparkline({
  data,
  width = 96,
  height = 32,
  tone,
  className,
  fill = true,
}: {
  data: number[];
  width?: number;
  height?: number;
  /** up = blue, down = red (default: by direction); yellow / accent for neutral series */
  tone?: "up" | "down" | "gold" | "ember" | "yellow" | "accent";
  className?: string;
  fill?: boolean;
}) {
  const id = React.useId();
  if (data.length < 2) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const span = max - min || 1;
  const pts = data.map((v, i) => [(i / (data.length - 1)) * width, height - 2 - ((v - min) / span) * (height - 4)] as const);
  const d = pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const t = tone ?? (data[data.length - 1]! >= data[0]! ? "up" : "down");
  const color = t === "accent" ? "var(--k-accent-line)" : t === "yellow" ? "var(--k-yellow)" : `var(--k-${t})`;
  // the equity look (KALKS2 §6): ink line over a neon-yellow fill in light, yellow over yellow in dark
  const fillColor = t === "accent" ? "var(--k-accent-fill)" : color;
  return (
    <svg viewBox={`0 0 ${width} ${height}`} width={width} height={height} preserveAspectRatio="none" aria-hidden className={cn("overflow-visible", className)}>
      <defs>
        <linearGradient id={id} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0" stopColor={fillColor} stopOpacity={t === "accent" ? 0.55 : 0.22} />
          <stop offset="1" stopColor={fillColor} stopOpacity="0" />
        </linearGradient>
      </defs>
      {fill && <path d={`${d} L${width},${height} L0,${height} Z`} fill={`url(#${id})`} />}
      <path d={d} fill="none" stroke={color} strokeWidth={1.5} strokeLinejoin="round" strokeLinecap="round" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

/* ------------------------------------------------------------------ */
/* Gauge — red → yellow ring (margin level / sentiment / risk)         */
/* ------------------------------------------------------------------ */

export function Gauge({
  value,
  max = 100,
  label,
  sublabel,
  display,
  size = 240,
  className,
}: {
  value: number;
  max?: number;
  label?: string;
  sublabel?: React.ReactNode;
  display?: React.ReactNode;
  size?: number;
  className?: string;
}) {
  const id = React.useId().replace(/:/g, "");
  const stroke = size * 0.085;
  const r = (size - stroke) / 2 - 8;
  const c = 2 * Math.PI * r;
  const arc = 0.78; // 280° sweep
  const pct = Math.min(1, Math.max(0, value / max));
  return (
    <div className={cn("relative grid place-items-center", className)} style={{ width: size, height: size }}>
      <svg viewBox={`0 0 ${size} ${size}`} className="absolute inset-0 rotate-[129.6deg]">
        <defs>
          <linearGradient id={`g${id}`} x1="0" x2="1" y1="0" y2="1">
            <stop offset="0" stopColor="var(--k-red)" />
            <stop offset="1" stopColor="var(--k-yellow)" />
          </linearGradient>
        </defs>
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--k-surface-3)" strokeWidth={stroke} strokeDasharray={`${c * arc} ${c}`} strokeLinecap="round" />
        {/* no value arc at 0: a round cap on an empty dash would still paint a dot */}
        {pct > 0 && <motion.circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={`url(#g${id})`}
          strokeWidth={stroke}
          strokeLinecap="round"
          initial={{ strokeDasharray: `0 ${c}` }}
          animate={{ strokeDasharray: `${c * arc * pct} ${c}` }}
          transition={{ duration: 1.1, ease: [0.16, 1, 0.3, 1] }}
        />}
      </svg>
      <div className="absolute rounded-full bg-surface-2 shadow-[inset_0_0_0_1px_var(--k-border)]" style={{ inset: stroke + 18 }} />
      <div className="relative flex flex-col items-center text-center">
        <div className="k-money leading-none text-fg" style={{ fontSize: size * 0.18 }}>
          {display ?? Math.round(value)}
        </div>
        {label && <div className="mt-2 text-sm text-fg-2">{label}</div>}
        {sublabel && <div className="mt-1 text-xs">{sublabel}</div>}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Barcode bars — profit/loss distribution                             */
/* ------------------------------------------------------------------ */

export function BarcodeBars({ lossPct, height = 96, bars = 90, className }: { lossPct: number; height?: number; bars?: number; className?: string }) {
  const cut = Math.round((lossPct / 100) * bars);
  return (
    <div className={cn("flex items-end gap-[2px]", className)} style={{ height }}>
      {Array.from({ length: bars }, (_, i) => {
        const isLoss = i < cut;
        const edge = Math.abs(i - cut) < 10;
        const h = 30 + ((Math.sin(i * 1.7) + 1) / 2) * 60 + (edge ? 10 : 0);
        return (
          <motion.span
            key={i}
            className={cn("w-[3px] flex-1 rounded-[1px]", isLoss ? "bg-down" : "bg-up")}
            style={{ opacity: isLoss ? (i > cut - 22 ? 0.85 : 0.25) : i < cut + 26 ? 0.85 : 0.2 }}
            initial={{ height: 0 }}
            animate={{ height: `${h}%` }}
            transition={{ duration: 0.6, delay: i * 0.004 }}
          />
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Capsule bars — weekly balance (Xpertfunding reference)              */
/* ------------------------------------------------------------------ */

export function CapsuleBars({
  data,
  active,
  height = 220,
  format = (v: number) => v.toFixed(2),
  className,
}: {
  data: { label: string; value: number }[];
  active?: number;
  height?: number;
  format?: (v: number) => string;
  className?: string;
}) {
  const [hover, setHover] = React.useState<number | null>(null);
  const max = Math.max(...data.map((d) => d.value));
  const sel = hover ?? active ?? data.length - 1;
  return (
    <div className={cn("flex items-end justify-between gap-3", className)} style={{ height }}>
      {data.map((d, i) => {
        const on = i === sel;
        return (
          <div key={d.label} className="relative flex h-full flex-1 flex-col items-center justify-end" onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover(null)}>
            {on && (
              <motion.div layoutId="capsule-tip" className="absolute z-10 -translate-y-full rounded-[8px] bg-fg px-2 py-1 font-mono text-xs font-semibold text-bg" style={{ bottom: `${(d.value / max) * 82 + 6}%` }}>
                {format(d.value)}
              </motion.div>
            )}
            <motion.div
              className={cn(
                "w-full max-w-14 rounded-[14px]",
                on ? "bg-yellow shadow-[1px_1px_0_var(--k-yellow-edge),2px_2px_0_var(--k-yellow-edge),3px_3px_0_var(--k-yellow-edge)]" : "bg-surface-3",
              )}
              initial={{ height: 0 }}
              animate={{ height: `${(d.value / max) * 82}%` }}
              transition={{ duration: 0.8, delay: i * 0.05, ease: [0.16, 1, 0.3, 1] }}
            />
            <span className={cn("mt-2 whitespace-nowrap text-[11px]", on ? "text-fg" : "text-fg-3")}>{d.label}</span>
          </div>
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Donut                                                               */
/* ------------------------------------------------------------------ */

/** Categorical series in the Kalks 2 palette (no green: green is reserved for "Completed"). */
export const CHART_COLORS = ["#D4112A", "#FFD21F", "#2F7BFF", "#928380", "#FF8A8A", "#B58C00", "#8DB7FF", "#7A0C18"];

export function Donut({
  data,
  size = 180,
  thickness = 22,
  center,
  className,
}: {
  data: { label: string; value: number; color?: string }[];
  size?: number;
  thickness?: number;
  center?: React.ReactNode;
  className?: string;
}) {
  const total = data.reduce((s, d) => s + d.value, 0) || 1;
  const r = (size - thickness) / 2;
  const c = 2 * Math.PI * r;
  let acc = 0;
  return (
    <div className={cn("relative", className)} style={{ width: size, height: size }}>
      <svg viewBox={`0 0 ${size} ${size}`} className="-rotate-90">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--k-surface-3)" strokeWidth={thickness} />
        {data.map((d, i) => {
          const len = (d.value / total) * c;
          const gap = data.length > 1 ? 3 : 0;
          const el = (
            <motion.circle
              key={d.label}
              cx={size / 2}
              cy={size / 2}
              r={r}
              fill="none"
              stroke={d.color ?? CHART_COLORS[i % CHART_COLORS.length]}
              strokeWidth={thickness}
              strokeDashoffset={-acc}
              initial={{ strokeDasharray: `0 ${c}` }}
              animate={{ strokeDasharray: `${Math.max(0, len - gap)} ${c}` }}
              transition={{ duration: 0.9, delay: i * 0.06 }}
            />
          );
          acc += len;
          return el;
        })}
      </svg>
      {center && <div className="absolute inset-0 grid place-items-center text-center">{center}</div>}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Diverging bar — net long / short exposure                           */
/* ------------------------------------------------------------------ */

export function DivergingBar({ value, max, className }: { value: number; max: number; className?: string }) {
  const pct = Math.min(1, Math.abs(value) / (max || 1)) * 50;
  const long = value >= 0;
  return (
    <div className={cn("relative h-2 w-full rounded-full bg-surface-3", className)}>
      <span className="absolute left-1/2 top-[-3px] h-[14px] w-px bg-fg-3" />
      <motion.span
        className={cn("absolute top-0 h-full rounded-full", long ? "bg-gradient-to-r from-up/50 to-up" : "bg-gradient-to-l from-down/50 to-down")}
        style={long ? { left: "50%" } : { right: "50%" }}
        initial={{ width: 0 }}
        animate={{ width: `${pct}%` }}
        transition={{ duration: 0.8, ease: "easeOut" }}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Mini bars — small activity histogram inside KPI cards               */
/* ------------------------------------------------------------------ */

export function MiniBars({ data, className, highlightLast = true }: { data: number[]; className?: string; highlightLast?: boolean }) {
  const max = Math.max(...data, 1);
  return (
    <div className={cn("flex h-8 items-end gap-[3px]", className)}>
      {data.map((v, i) => (
        <span
          key={i}
          className={cn("w-[3px] rounded-full", highlightLast && i === data.length - 1 ? "bg-gold" : "bg-fg-3/50")}
          style={{ height: `${(v / max) * 100}%` }}
        />
      ))}
    </div>
  );
}

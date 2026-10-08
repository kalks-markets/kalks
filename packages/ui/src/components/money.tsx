"use client";

import * as React from "react";
import { cn } from "../lib/cn";
import { splitNumber, formatPct } from "../lib/format";

/**
 * A money figure with dimmed decimals ($54,208.<dim>11</dim>). `display` sets it as Kalks 2 money typography:
 * Archivo at wdth 108 / 700 with tabular figures and the cents smaller in the secondary text colour. Values never
 * roll (KALKS2 §7); `countUp` is accepted for compatibility and ignored.
 */
export function Money({
  value,
  currency = "$",
  decimals = 2,
  signed = false,
  tone,
  display,
  className,
  decClassName,
}: {
  value: number;
  currency?: string;
  decimals?: number;
  /** @deprecated no rolling digits in Kalks 2 */
  countUp?: boolean;
  signed?: boolean;
  tone?: "up" | "down" | "auto";
  /** Display face (balances, hero KPIs); off = the surrounding font. */
  display?: boolean;
  className?: string;
  decClassName?: string;
}) {
  const { sign, int, dec } = splitNumber(value, decimals);
  const t = tone === "auto" ? (value > 0 ? "up" : value < 0 ? "down" : undefined) : tone;
  // minus is U+2212 (KALKS2 §3)
  const s = signed && value > 0 ? "+" : sign === "-" ? "−" : sign;
  return (
    <span className={cn("k-num whitespace-nowrap", display && "k-money", t === "up" && "text-up", t === "down" && "text-down", className)}>
      {s}
      {currency}
      {int}
      {decimals > 0 && <span className={cn(display ? "k-cents" : "opacity-50", decClassName)}>.{dec}</span>}
    </span>
  );
}

/** Change: blue up, red down; `chip` = mono on up-soft / down-soft. */
export function Delta({ value, suffix = "%", decimals = 2, className, chip }: { value: number; suffix?: string; decimals?: number; className?: string; chip?: boolean }) {
  const up = value >= 0;
  const text = suffix === "%" ? formatPct(value, decimals) : `${up ? "+" : "−"}${Math.abs(value).toFixed(decimals)}${suffix}`;
  return (
    <span className={cn("k-num inline-flex items-center gap-1 font-semibold", up ? "text-up" : "text-down", chip && cn("k-chg", up ? "bg-up-soft" : "bg-down-soft"), className)}>
      <svg viewBox="0 0 10 10" aria-hidden className={cn("size-2 fill-current", !up && "rotate-180")}>
        <path d="M5 1l4 7H1z" />
      </svg>
      <span className="sr-only">{up ? "up" : "down"} </span>
      {text.replace(/^[+\-−]/, "")}
    </span>
  );
}

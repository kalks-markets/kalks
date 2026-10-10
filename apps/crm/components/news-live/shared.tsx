"use client";

import * as React from "react";
import Link from "next/link";
import { Minus, TrendingDown, TrendingUp } from "lucide-react";
import { Chip, SymbolAvatar, cn } from "@/components/kit";
import { INSTRUMENT_MAP } from "@kalks/mock";
import type { CalEvent, NewsItem, Sentiment } from "./api";
import type { T } from "@kalks/i18n";
import { tr, useT } from "@kalks/i18n/react";
import { TERMINAL_URL } from "@/lib/live";

export const SENT: Record<Sentiment, { tone: "up" | "down" | "neutral"; icon: React.ReactNode; labelKey: "news.sentiment.bullish" | "news.sentiment.bearish" | "news.sentiment.neutral" }> = {
  bullish: { tone: "up", icon: <TrendingUp className="size-3" />, labelKey: "news.sentiment.bullish" },
  bearish: { tone: "down", icon: <TrendingDown className="size-3" />, labelKey: "news.sentiment.bearish" },
  neutral: { tone: "neutral", icon: <Minus className="size-3" />, labelKey: "news.sentiment.neutral" },
};

export const COUNTRY_NAME: Record<string, string> = {
  us: "United States", eu: "Euro area", de: "Germany", fr: "France", it: "Italy", es: "Spain", gb: "United Kingdom", jp: "Japan", au: "Australia",
  ca: "Canada", ch: "Switzerland", nz: "New Zealand", cn: "China", in: "India", sa: "Saudi Arabia", ru: "Russia", ir: "Iran", br: "Brazil",
  mx: "Mexico", kr: "South Korea", sg: "Singapore",
};

export const CATEGORY_LABEL: Record<string, string> = { macro: "Central banks & macro", forex: "Forex", metals: "Metals", indices: "Indices", energies: "Energy", crypto: "Crypto", stocks: "Stocks", markets: "Markets" };

/** Country name in the reader's language (English fallback, then the code). Pass the component's
 * useT() translator when it renders during server rendering, so server and browser print the same text. */
export function countryName(c: string, t: T = tr) {
  return t.dyn(`news.country.${c}`, COUNTRY_NAME[c] ?? c.toUpperCase());
}
/** News category label in the reader's language; unknown categories as served. */
export function categoryLabel(c: string, t: T = tr) {
  return t.dyn(`news.category.${c}`, CATEGORY_LABEL[c] ?? c);
}

/** A cover per category from the founder's own solid-colour photos (2026-10-10: the old stock library is retired;
 *  publisher images are never copied). */
const COVER: Record<string, string> = {
  macro: "/heroes/news.jpg",
  forex: "/heroes/markets.jpg",
  metals: "/heroes/rewards.jpg",
  indices: "/heroes/portfolio.jpg",
  energies: "/heroes/profile.jpg",
  crypto: "/heroes/copy.jpg",
  stocks: "/heroes/accounts.jpg",
  markets: "/heroes/wallet.jpg",
};
export function coverFor(n: Pick<NewsItem, "id" | "category">) {
  return COVER[n.category] ?? COVER.markets!;
}

export function ago(iso: string, now = Date.now()) {
  const m = Math.max(0, Math.round((now - new Date(iso).getTime()) / 60000));
  if (m < 1) return tr("news.ago.justNow");
  if (m < 60) return tr("news.ago.minutes", { m });
  const h = Math.floor(m / 60);
  if (h < 24) return m % 60 ? tr("news.ago.hoursMinutes", { h, m: m % 60 }) : tr("news.ago.hours", { h });
  const d = Math.floor(h / 24);
  return d === 1 ? tr("news.ago.yesterday") : tr("news.ago.days", { count: d });
}

export function Flag({ country, className }: { country: string; className?: string }) {
  if (!country) return null;
  return <span className={cn(`fi fis fi-${country} shrink-0 rounded-full`, className ?? "size-3.5")} aria-hidden />;
}

export function SymbolPill({ s, href }: { s: string; href?: string }) {
  if (!INSTRUMENT_MAP[s]) return <span className="rounded-full border border-line bg-surface-3 px-2 py-0.5 font-mono text-[10.5px] text-fg-2">{s}</span>;
  return (
    <Link target="_blank" rel="noopener" href={href ?? `${TERMINAL_URL}/?symbol=${s}`} onClick={(e) => e.stopPropagation()} className="flex items-center gap-1 rounded-full border border-line bg-surface-3 py-0.5 ps-0.5 pe-2 font-mono text-[10.5px] text-fg-2 transition-colors hover:border-ember/40 hover:text-ember">
      <SymbolAvatar symbol={s} size={14} />
      {s}
    </Link>
  );
}

export function SentimentChip({ s, className }: { s: Sentiment; className?: string }) {
  const t = useT();
  return (
    <Chip size="sm" tone={SENT[s].tone} className={className}>
      {SENT[s].icon}
      {t(SENT[s].labelKey)}
    </Chip>
  );
}

export function ImpactBars({ impact, className }: { impact: number; className?: string }) {
  const color = impact === 3 ? "bg-down" : impact === 2 ? "bg-warn" : "bg-fg-2";
  const t = useT();
  return (
    <span className={cn("inline-flex items-end gap-[3px]", className)} aria-label={impact ? t("news.impact.aria", { n: impact }) : t("news.impact.holiday")}>
      {[1, 2, 3].map((i) => (
        <span key={i} className={cn("w-[4px] rounded-full", i <= impact ? color : "bg-surface-3")} style={{ height: 6 + i * 3 }} />
      ))}
    </span>
  );
}

export function ActualValue({ e }: { e: CalEvent }) {
  if (!e.actual) return <span className="text-fg-3">—</span>;
  return <span className={cn("font-semibold", e.surprise === 1 ? "text-up" : e.surprise === -1 ? "text-down" : "text-fg")}>{e.actual}</span>;
}

/** Server-time offset label, e.g. "GMT+3". */
export const gmt = (offset: number) => `GMT${offset >= 0 ? "+" : ""}${offset}`;

export function localTime(iso: string) {
  return new Date(iso).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** Ticking "now" (every `ms`), null until mounted so SSR and the first client render agree. */
export function useNow(ms = 1000) {
  const [now, setNow] = React.useState<number | null>(null);
  React.useEffect(() => {
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), ms);
    return () => clearInterval(t);
  }, [ms]);
  return now;
}

export function countdown(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const d = Math.floor(s / 86400);
  const hh = String(Math.floor((s % 86400) / 3600)).padStart(2, "0");
  const mm = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const ss = String(s % 60).padStart(2, "0");
  return d > 0 ? tr("news.countdown.days", { d, time: `${hh}:${mm}:${ss}` }) : `${hh}:${mm}:${ss}`;
}

/** Heat for the world map: tone per country (−1..1), damped for countries with few stories. */
export function heatOf(countries: { country: string; count: number; sentiment: number }[]) {
  const out: Record<string, number> = {};
  for (const c of countries) if (c.count > 0 && c.sentiment !== 0) out[c.country] = Math.max(-1, Math.min(1, c.sentiment * Math.min(1, c.count / 3)));
  return out;
}

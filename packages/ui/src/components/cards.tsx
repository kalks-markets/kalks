"use client";

// Kalks 2 composite cards (docs/design/KALKS2.md §6): the photo hero with its glass stat strip, small stat tiles
// and the trading-account card with its product tag (CFD / OPTIONS).

import * as React from "react";
import { cn } from "../lib/cn";
import { Tag } from "./primitives";
import { Tabs } from "./navigation";
import { Money } from "./money";
import { Sparkline } from "../charts/svg-charts";

/* ------------------------------------------------------------------ */
/* Photo hero + glass stat strip                                       */
/* ------------------------------------------------------------------ */

export type HeroStat = {
  label: React.ReactNode;
  value: React.ReactNode;
  /** colour key before the label (a CSS colour, e.g. var(--k-up)) */
  color?: string;
  /** optional 0–100 bar under the label */
  bar?: number;
};

/**
 * One striking image (solid-colour art: one subject on one flat backdrop), a left scrim for the copy, optional glass
 * pill tabs top-right, kicker + Display M headline + one sentence + a white 48 CTA, and the frosted KPI strip 12 px
 * inside the bottom edge. Radius 28, 440 px tall (phone 560, the strip goes 2 × 2).
 */
export function PhotoHero<T extends string = string>({
  image,
  imageAlt = "",
  imagePosition = "60% 22%",
  phoneImagePosition = "62% 0%",
  kicker,
  title,
  text,
  cta,
  link,
  tabs,
  stats,
  statsAction,
  className,
}: {
  image: string;
  imageAlt?: string;
  imagePosition?: string;
  phoneImagePosition?: string;
  kicker?: React.ReactNode;
  title: React.ReactNode;
  text?: React.ReactNode;
  /** usually <Button variant="white" size={48}> */
  cta?: React.ReactNode;
  /** a text link next to the CTA */
  link?: React.ReactNode;
  tabs?: { items: readonly { value: T; label: React.ReactNode }[]; value: T; onChange: (v: T) => void; "aria-label"?: string };
  stats?: readonly HeroStat[];
  /** round ink "more" button at the end of the strip (hidden on phones) */
  statsAction?: React.ReactNode;
  className?: string;
}) {
  return (
    <section className={cn("relative isolate h-[560px] overflow-hidden rounded-[var(--radius-hero)] bg-[#140405] text-white md:h-[440px]", className)}>
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={image}
        alt={imageAlt}
        className="absolute inset-0 -z-20 size-full object-cover [object-position:var(--pos-m)] md:[object-position:var(--pos)]"
        style={{ "--pos": imagePosition, "--pos-m": phoneImagePosition } as React.CSSProperties}
      />
      <div
        aria-hidden
        className="absolute inset-0 -z-10 bg-[linear-gradient(180deg,rgba(12,3,4,.75)_0%,rgba(12,3,4,.3)_45%,rgba(12,3,4,0)_60%)] md:bg-[linear-gradient(90deg,rgba(12,3,4,.78)_0%,rgba(12,3,4,.5)_32%,rgba(12,3,4,0)_58%)] rtl:md:bg-[linear-gradient(270deg,rgba(12,3,4,.78)_0%,rgba(12,3,4,.5)_32%,rgba(12,3,4,0)_58%)]"
      />
      {tabs && (
        <div className="absolute end-[18px] top-[18px] hidden md:block">
          <Tabs variant="glass" tabs={tabs.items} value={tabs.value} onChange={tabs.onChange} aria-label={tabs["aria-label"]} />
        </div>
      )}
      <div className="absolute inset-x-5 top-[22px] max-w-[430px] md:start-[34px] md:end-auto md:top-[34px]">
        {kicker && <div className="k-kicker inline-flex items-center gap-2 text-k-yellow">{kicker}</div>}
        <h2 className="k-d mt-3.5 text-[34px] text-white md:text-[46px]">{title}</h2>
        {text && <p className="mt-3.5 max-w-[36ch] text-[14.5px] font-medium leading-[1.45] text-white/80 md:text-[15.5px]">{text}</p>}
        {(cta || link) && (
          <div className="mt-[22px] flex items-center gap-4 [&_.k-btn-white]:[--k-white-edge:#9E8C88]">
            {cta}
            {link && <span className="inline-flex items-center gap-1.5 text-[14px] font-semibold text-white/90">{link}</span>}
          </div>
        )}
      </div>
      {stats && stats.length > 0 && (
        <div className="k-glass-strong absolute inset-x-3 bottom-3 grid grid-cols-2 items-center rounded-[var(--radius-strip)] text-fg md:flex">
          {stats.map((s, i) => (
            <div
              key={i}
              className={cn(
                "min-w-0 px-[15px] py-[13px] md:flex-1 md:px-[22px] md:pb-[17px] md:pt-4",
                i % 2 === 1 && "border-s border-line",
                i >= 2 && "border-t border-line md:border-t-0",
                i >= 1 && "md:border-s md:border-line",
              )}
            >
              <div className="k-money whitespace-nowrap text-[21px] leading-none md:text-[27px]">{s.value}</div>
              <div className="mt-2 flex items-center gap-[7px] text-[12.5px] font-medium leading-none text-fg-2">
                {s.color && <i aria-hidden className="size-2 shrink-0 rounded-[3px]" style={{ background: s.color }} />}
                {s.label}
              </div>
              {s.bar !== undefined && (
                <div className="mt-[11px] h-[3px] overflow-hidden rounded-full bg-line-2">
                  <span className="block h-full rounded-full" style={{ width: `${Math.min(100, Math.max(0, s.bar))}%`, background: s.color ?? "var(--k-fg)" }} />
                </div>
              )}
            </div>
          ))}
          {statsAction && <div className="me-[18px] hidden md:block">{statsAction}</div>}
        </div>
      )}
    </section>
  );
}

/* ------------------------------------------------------------------ */
/* Stat tile: one big number                                           */
/* ------------------------------------------------------------------ */

export function StatTile({
  label,
  value,
  sub,
  icon,
  spark,
  sparkTone,
  className,
}: {
  label: React.ReactNode;
  /** the number (use <Money display />) */
  value: React.ReactNode;
  /** one line under it: a change, a caption */
  sub?: React.ReactNode;
  icon?: React.ReactNode;
  spark?: readonly number[];
  sparkTone?: "up" | "down" | "yellow" | "accent";
  className?: string;
}) {
  return (
    <div className={cn("k-card flex flex-col p-5", className)}>
      <div className="flex items-center justify-between gap-3">
        <span className="text-[13px] font-semibold text-fg-2">{label}</span>
        {icon && <span className="grid size-[34px] place-items-center rounded-[11px] bg-surface-3 text-fg-2 [&_svg]:size-[17px]">{icon}</span>}
      </div>
      <div className="k-money mt-4 text-[34px] leading-none text-fg">{value}</div>
      {sub && <div className="mt-2 text-[13px] font-medium text-fg-2">{sub}</div>}
      {spark && spark.length > 1 && <Sparkline data={[...spark]} tone={sparkTone} width={240} height={44} className="mt-4 h-11 w-full" />}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Trading account card                                                */
/* ------------------------------------------------------------------ */

export type AccountProduct = "cfd" | "options";

/**
 * A trading account: product tag (CFD ink / OPTIONS yellow), LIVE / DEMO, the login, the group name, the balance in
 * money type, two-column facts (mono) and its actions (usually Deposit highlight 32 + Trade ink 32 + a more button).
 */
export function AccountCard({
  product,
  kind,
  login,
  name,
  balance,
  currency = "$",
  facts,
  actions,
  labels,
  onClick,
  className,
}: {
  product: AccountProduct;
  kind: "live" | "demo";
  login: React.ReactNode;
  name: React.ReactNode;
  balance: number;
  currency?: string;
  facts?: readonly { label: React.ReactNode; value: React.ReactNode }[];
  actions?: React.ReactNode;
  /** tag texts (translated by the page); default CFD / OPTIONS / LIVE / DEMO */
  labels?: Partial<Record<AccountProduct | "live" | "demo", string>>;
  onClick?: () => void;
  className?: string;
}) {
  return (
    <article className={cn("k-card flex flex-col gap-3.5 px-5 pb-[18px] pt-5", onClick && "cursor-pointer", className)} onClick={onClick}>
      <div className="flex items-center gap-1.5">
        <Tag tone={product}>{labels?.[product] ?? (product === "cfd" ? "CFD" : "Options")}</Tag>
        <Tag tone={kind === "live" ? "live" : "demo"}>{labels?.[kind] ?? (kind === "live" ? "Live" : "Demo")}</Tag>
        <span className="ms-auto font-mono text-[12.5px] font-medium text-fg-3">{login}</span>
      </div>
      <div className="text-[13.5px] font-semibold text-fg-2">{name}</div>
      <Money value={balance} currency={currency} display className="-mt-1 text-[30px] leading-none text-fg" />
      {facts && facts.length > 0 && (
        <dl className="grid grid-cols-2 gap-x-3.5 gap-y-2.5 border-t border-line pt-[13px]">
          {facts.map((f, i) => (
            <div key={i} className="min-w-0">
              <dt className="mb-1.5 text-[12px] font-medium leading-none text-fg-3">{f.label}</dt>
              <dd className="k-num truncate font-mono text-[13.5px] font-semibold leading-none">{f.value}</dd>
            </div>
          ))}
        </dl>
      )}
      {actions && <div className="flex items-center gap-2 [&>*:last-child]:ms-auto">{actions}</div>}
    </article>
  );
}

/** The empty slot next to the account cards: "Open an Options account" with one line and an action. */
export function AddAccountCard({ title, text, action, className }: { title: React.ReactNode; text?: React.ReactNode; action?: React.ReactNode; className?: string }) {
  return (
    <div className={cn("flex flex-col items-start justify-between gap-4 rounded-[var(--radius-card)] p-5 shadow-[inset_0_0_0_1.5px_var(--k-border-2)]", className)}>
      <div>
        <div className="k-title text-[18px] leading-tight">{title}</div>
        {text && <p className="mt-2 max-w-[28ch] text-[13.5px] text-fg-2">{text}</p>}
      </div>
      {action}
    </div>
  );
}

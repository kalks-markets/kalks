"use client";

// Page heroes for the Client Area's main pages (founder's reference 2026-10-10: a photo across the top with a big
// headline, the page pills and buttons over the photo, a glass card on the right, and light cards with big thin
// numbers sitting on the photo's lower edge). One photo per main page: public/heroes/<page>.jpg, made from the
// founder's portraits by widening their own backdrop.
//
// The hero slides up under the sticky top bar; while it is under the bar, <html data-hero="on"> turns the bar's
// tokens white-on-glass (app/kx-orange.css).

import * as React from "react";
import Link from "next/link";
import { cn } from "@/components/kit";

export type HeroPage = "dashboard" | "accounts" | "wallet" | "portfolio" | "copy" | "prop" | "markets" | "options" | "partner" | "rewards" | "academy" | "profile" | "support" | "news";

/** where the subject sits in each photo (object-position), so phones crop to the face */
const FOCUS: Record<HeroPage, string> = {
  dashboard: "50% 36%",
  accounts: "70% 30%",
  wallet: "64% 30%",
  portfolio: "64% 25%",
  copy: "58% 30%",
  prop: "62% 25%",
  markets: "64% 25%",
  options: "62% 30%",
  partner: "64% 30%",
  rewards: "70% 30%",
  academy: "64% 35%",
  profile: "60% 25%",
  support: "58% 28%",
  news: "60% 30%",
};

/** each photo's backdrop colour (measured from the image) */
const PHOTO_RGB: Record<HeroPage, string> = {
  dashboard: "150 10 8",
  accounts: "214 2 25",
  wallet: "202 0 16",
  portfolio: "0 1 188",
  copy: "9 80 133",
  prop: "157 137 228",
  markets: "229 44 1",
  options: "1 37 118",
  partner: "2 1 153",
  rewards: "203 5 1",
  academy: "1 60 147",
  profile: "221 64 39",
  support: "227 39 45",
  news: "176 34 29",
};

/** the section a URL belongs to, for its photo: every page of a section shares it, the rest use the dashboard's */
export function photoForPath(path: string): HeroPage {
  const seg = path.split("/")[1] ?? "";
  if (seg === "accounts") return "accounts";
  if (seg === "wallet") return "wallet";
  if (seg === "portfolio") return "portfolio";
  if (seg === "social") return "copy";
  if (seg === "prop") return "prop";
  if (seg === "markets") return "markets";
  if (seg === "options") return "options";
  if (seg === "partner") return "partner";
  if (seg === "rewards") return "rewards";
  if (seg === "academy") return "academy";
  if (seg === "profile") return "profile";
  if (seg === "support") return "support";
  if (seg === "news" || seg === "calendar") return "news";
  if (seg === "developer") return "options";
  return "dashboard";
}

/** Shell-level: the section's photo, blurred, behind EVERY page (cards and the rail turn to glass over it, its
 *  colour tints the page), so moving between pages never drops to plain black. */
export function usePagePhoto(path: string) {
  const page = photoForPath(path);
  React.useEffect(() => {
    const root = document.documentElement;
    root.style.setProperty("--k-page-photo", `url("/heroes/${page}.jpg")`);
    root.style.setProperty("--k-photo-rgb", PHOTO_RGB[page]);
    root.dataset.photo = page;
  }, [page]);
}

function useHeroHeader(ref: React.RefObject<HTMLElement | null>) {
  React.useEffect(() => {
    const root = document.documentElement;
    const header = document.querySelector<HTMLElement>("[data-shell-header]");
    const setH = () => root.style.setProperty("--k-hdr", `${header?.offsetHeight ?? 76}px`);
    setH();
    const ro = header ? new ResizeObserver(setH) : null;
    if (header) ro!.observe(header);
    const el = ref.current;
    let raf = 0;
    const check = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        if (!el) return;
        const bottom = el.getBoundingClientRect().bottom;
        const hdr = header?.getBoundingClientRect().bottom ?? 76;
        root.dataset.hero = bottom > hdr ? "on" : "off";
      });
    };
    check();
    window.addEventListener("scroll", check, { passive: true });
    window.addEventListener("resize", check);
    return () => {
      ro?.disconnect();
      window.removeEventListener("scroll", check);
      window.removeEventListener("resize", check);
      cancelAnimationFrame(raf);
      delete root.dataset.hero;
    };
  }, [ref]);
}

export function PageHero({
  page,
  eyebrow,
  title,
  lead,
  actions,
  aside,
  children,
  compact,
  overlap,
  center,
  body,
  tall,
}: {
  page: HeroPage;
  eyebrow?: React.ReactNode;
  /** optional: Home shows only the AI bar */
  title?: React.ReactNode;
  lead?: React.ReactNode;
  actions?: React.ReactNode;
  /** a glass card on the right (desktop) / under the title (phones) */
  aside?: React.ReactNode;
  /** cards sitting on the lower edge of the photo */
  children?: React.ReactNode;
  /** shorter hero for inner main pages */
  compact?: boolean;
  /** accepted for compatibility: every hero now has the Home layout and nothing rises onto the photo */
  overlap?: boolean;
  /** centred title + body (the dashboard: greeting and the AI bar, like the Claude / ChatGPT home) */
  center?: boolean;
  /** content under the title (full width, up to 780 px) */
  body?: React.ReactNode;
  /** the photo fills the screen (Support: the page is only the photo and the chat) */
  tall?: boolean;
}) {
  const ref = React.useRef<HTMLElement>(null);
  useHeroHeader(ref);
  return (
    <section
      ref={ref}
      // phones: edge to edge from the very top; larger screens: a rounded panel just under the window edge
      className="k-page-hero relative isolate -mx-4 mb-6 overflow-hidden rounded-b-[28px] bg-[#2a0f08] text-white [--k-hero-gap:12px] sm:mx-0 sm:rounded-[28px] sm:[--k-hero-gap:4px]"
      style={{ marginTop: "calc(-1 * (var(--k-hdr, 76px) + var(--k-hero-gap)))" }}
    >
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src={`/heroes/${page}.jpg`} alt="" aria-hidden className="absolute inset-0 -z-10 h-full w-full object-cover" style={{ objectPosition: FOCUS[page] }} />
      {/* legibility: the bar on top, the headline on the left, the cards at the bottom */}
      <div aria-hidden className="absolute inset-x-0 top-0 -z-10 h-56 bg-gradient-to-b from-black/55 via-black/25 to-transparent lg:h-40 lg:from-black/45 lg:via-transparent" />
      <div aria-hidden className="absolute inset-0 -z-10 bg-[radial-gradient(70%_60%_at_50%_80%,rgba(0,0,0,.32),transparent_70%)]" />
      {children && <div aria-hidden className="absolute inset-x-0 bottom-0 -z-10 h-1/2 bg-gradient-to-t from-black/30 to-transparent" />}
      <div aria-hidden className="absolute inset-x-0 bottom-0 -z-10 h-2/3 bg-gradient-to-t from-black/60 via-black/25 to-transparent" />

      <div style={{ paddingTop: "calc(var(--k-hdr, 76px) + 28px)" }} className="px-5 sm:px-8 lg:px-10">
        {/* every page has the same hero as Home: same height, the title centred at the same size, then the short line,
            the buttons and (Home) the AI bar, all centred. The page's cards follow under the photo. */}
        <div
          className={cn("flex flex-col items-center justify-end pb-10 text-center lg:pb-14", tall ? "min-h-[calc(100dvh-var(--k-hdr,76px)-60px)]" : "min-h-[340px] lg:min-h-[480px]")}
        >
          {eyebrow && <div className="mb-3 text-[12.5px] font-semibold uppercase tracking-[0.14em] text-white/80">{eyebrow}</div>}
          {title && <h1 className="k-display text-[38px] font-bold leading-[1] tracking-[-0.035em] text-white [text-shadow:0_2px_30px_rgba(0,0,0,.35)] sm:text-[56px] lg:text-[68px]">{title}</h1>}
          {lead && <p className="mt-4 max-w-[560px] text-[14.5px] leading-relaxed text-white/88 [text-shadow:0_1px_12px_rgba(0,0,0,.35)] sm:text-[15.5px]">{lead}</p>}
          {actions && <div className="mt-7 flex flex-wrap items-center justify-center gap-2.5">{actions}</div>}
          {body && <div className={cn("w-full", (title || lead || actions) && "mt-7 sm:mt-9")}>{body}</div>}
          {aside && <div className="mt-7 w-full max-w-[420px]">{aside}</div>}
        </div>
      </div>
      {children && <div className="relative px-3 pb-3 sm:px-4 sm:pb-4">{children}</div>}
    </section>
  );
}

/** the frosted card on the photo (the reference's run card) */
export function HeroGlass({ className, children }: { className?: string; children: React.ReactNode }) {
  return (
    <div
      className={cn(
        "rounded-[26px] border border-white/20 bg-white/12 p-5 text-white shadow-[0_20px_60px_-30px_rgba(0,0,0,.6)] backdrop-blur-2xl backdrop-saturate-150 sm:p-6",
        className,
      )}
    >
      {children}
    </div>
  );
}

/** white glass button for the hero */
export function HeroButton({ href, external, children, primary }: { href: string; external?: boolean; children: React.ReactNode; primary?: boolean }) {
  const cls = cn(
    "inline-flex h-11 items-center gap-2 rounded-full px-5 text-[14px] font-semibold transition-[filter,background-color] [&_svg]:size-4",
    primary ? "bg-red text-on-red shadow-[0_10px_30px_-12px_rgba(242,96,12,.9)] hover:brightness-110" : "border border-white/25 bg-white/14 text-white backdrop-blur-xl hover:bg-white/22",
  );
  return external ? (
    <a href={href} target="_blank" rel="noopener" className={cls}>
      {children}
    </a>
  ) : (
    <Link href={href} className={cls}>
      {children}
    </Link>
  );
}

/* ------------------------------------------------------------------------------------------------------------
   Stat tiles: light cards with a big thin number, a small label and a thin chart (the reference's lower cards)
   ------------------------------------------------------------------------------------------------------------ */

export function StatTile({
  label,
  value,
  unit,
  meta,
  chart,
  footer,
  href,
  className,
}: {
  label: React.ReactNode;
  value: React.ReactNode;
  unit?: React.ReactNode;
  meta?: React.ReactNode;
  chart?: React.ReactNode;
  footer?: React.ReactNode;
  href?: string;
  className?: string;
}) {
  const body = (
    <div
      className={cn(
        "k-hero-tile flex h-full min-h-[188px] flex-col rounded-[24px] p-5 text-fg transition-transform [container-type:inline-size] sm:p-6",
        href && "hover:-translate-y-0.5",
        className,
      )}
    >
      <div className="flex items-start justify-between gap-3">
        <span className="shrink-0 whitespace-nowrap text-[14px] text-fg-2">{label}</span>
        {meta && <span className="min-w-0 truncate text-end text-[11px] font-medium uppercase tracking-[0.08em] text-fg-3">{meta}</span>}
      </div>
      <div className="mt-2 flex min-w-0 items-baseline gap-1.5">
        {/* the number scales with the card's own width, so it never clips when the rail is open */}
        <span className="k-num whitespace-nowrap text-[clamp(26px,14cqw,46px)] font-light leading-none tracking-[-0.035em]">{value}</span>
        {unit && <span className="shrink-0 text-[11px] font-medium uppercase tracking-[0.08em] text-fg-3">{unit}</span>}
      </div>
      {chart && <div className="mt-auto pt-4">{chart}</div>}
      {footer && <div className="mt-auto pt-3">{footer}</div>}
    </div>
  );
  return href ? (
    <Link href={href} className="block h-full min-w-0">
      {body}
    </Link>
  ) : (
    <div className="h-full min-w-0">{body}</div>
  );
}

/** thin bars, the last one orange (e.g. daily P&L of the last days) */
export function MiniBars({ values, labels, height = 54 }: { values: number[]; labels?: string[]; height?: number }) {
  const max = Math.max(1e-9, ...values.map((v) => Math.abs(v)));
  return (
    <div>
      <div className="flex items-end gap-[5px]" style={{ height }}>
        {values.map((v, i) => {
          const last = i === values.length - 1;
          const h = Math.max(3, (Math.abs(v) / max) * height);
          return <span key={i} className={cn("flex-1 rounded-full", last ? "bg-red" : v < 0 ? "bg-fg-3/40" : "bg-fg/70")} style={{ height: h }} title={String(v)} />;
        })}
      </div>
      {labels && (
        <div className="mt-2 flex gap-[5px] text-[10px] uppercase tracking-[0.06em] text-fg-3">
          {labels.map((l, i) => (
            <span key={i} className={cn("flex-1 text-center", i === labels.length - 1 && "font-semibold text-fg")}>
              {l}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

/** a thin grey line with the last stretch in orange and an end dot */
export function MiniLine({ values, height = 56 }: { values: number[]; height?: number }) {
  if (values.length < 2) return <div style={{ height }} className="flex items-end"><span className="h-px w-full bg-line" /></div>;
  const W = 300;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const x = (i: number) => (i / (values.length - 1)) * W;
  const y = (v: number) => 4 + (1 - (v - min) / (max - min || 1)) * (height - 8);
  const pts = values.map((v, i) => `${x(i).toFixed(1)},${y(v).toFixed(1)}`);
  const cut = Math.max(1, Math.floor(values.length * 0.7));
  return (
    <svg viewBox={`0 0 ${W} ${height}`} width="100%" height={height} preserveAspectRatio="none" aria-hidden>
      <polyline points={pts.slice(0, cut + 1).join(" ")} fill="none" stroke="var(--k-fg-3)" strokeOpacity=".55" strokeWidth="1.6" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
      <polyline points={pts.slice(cut).join(" ")} fill="none" stroke="var(--k-red)" strokeWidth="2" vectorEffect="non-scaling-stroke" strokeLinejoin="round" />
      <circle cx={x(values.length - 1)} cy={y(values[values.length - 1]!)} r="3" fill="var(--k-red)" />
    </svg>
  );
}

/** a half-circle gauge, filled in orange up to `value` (0…1) */
export function ArcGauge({ value, caption }: { value: number; caption?: React.ReactNode }) {
  const v = Math.max(0, Math.min(1, value));
  const R = 80;
  const len = Math.PI * R;
  return (
    <div className="relative">
      <svg viewBox="0 0 200 104" width="100%" height={84} aria-hidden>
        <path d="M20 100 A80 80 0 0 1 180 100" fill="none" stroke="var(--k-surface-4)" strokeWidth="6" strokeLinecap="round" />
        <path d="M20 100 A80 80 0 0 1 180 100" fill="none" stroke="var(--k-red)" strokeWidth="6" strokeLinecap="round" strokeDasharray={`${len * v} ${len}`} />
      </svg>
      {caption && <div className="absolute inset-x-0 bottom-0 text-center text-[10.5px] font-medium uppercase tracking-[0.08em] text-fg-3">{caption}</div>}
    </div>
  );
}

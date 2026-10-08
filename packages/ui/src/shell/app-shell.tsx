"use client";

import * as React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { motion } from "motion/react";
import type { LucideIcon } from "lucide-react";
import { LogOut, MoreHorizontal } from "lucide-react";
import { cn } from "../lib/cn";
import { Logo, LogoMark } from "./logo";
import { Tooltip, TooltipProvider, Menu } from "../components/overlays";
import { useLocale, useT } from "@kalks/i18n/react";

export interface SubNavItem {
  href: string;
  label: string;
  icon?: LucideIcon;
  badge?: number | string;
}

export interface NavModule {
  key: string;
  label: string;
  icon: LucideIcon;
  href: string;
  /** path prefixes that belong to this module */
  match?: string[];
  sub?: SubNavItem[];
  badge?: number | string;
  section?: string;
  external?: boolean;
}

function isActive(pathname: string, m: NavModule) {
  const prefixes = m.match ?? [m.href];
  return prefixes.some((p) => (p === "/" ? pathname === "/" : pathname === p || pathname.startsWith(p + "/")));
}

function subActive(pathname: string, s: SubNavItem, all: SubNavItem[]) {
  // longest matching prefix wins so /wallet and /wallet/deposit don't both light up
  const matches = all.filter((x) => pathname === x.href || pathname.startsWith(x.href + "/"));
  const best = matches.sort((a, b) => b.href.length - a.href.length)[0];
  return best?.href === s.href;
}

/* ------------------------------------------------------------------ */
/* Icon rail                                                           */
/* ------------------------------------------------------------------ */

/** Tooltips point away from the rail: right in left-to-right layouts, left in right-to-left ones. */
function useRailSide(): "left" | "right" {
  return useLocale().dir === "rtl" ? "left" : "right";
}

function IconRail({ modules, footer, side = "right" }: { modules: NavModule[]; footer?: React.ReactNode; side?: "left" | "right" }) {
  const railSide = side;
  const pathname = usePathname();
  // long module lists (the Back Office) get a denser rail; whatever still doesn't fit scrolls, with the cut edge
  // faded so the hidden modules read as "more below" rather than a clipped icon
  const dense = modules.length > 12;
  const navRef = React.useRef<HTMLElement>(null);
  const [edges, setEdges] = React.useState({ top: false, bottom: false });
  React.useEffect(() => {
    const el = navRef.current;
    if (!el) return;
    const update = () => setEdges({ top: el.scrollTop > 2, bottom: el.scrollTop + el.clientHeight < el.scrollHeight - 2 });
    update();
    el.querySelector<HTMLElement>("[data-rail-active]")?.scrollIntoView({ block: "nearest" });
    el.addEventListener("scroll", update, { passive: true });
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", update);
      ro.disconnect();
    };
  }, [modules.length]);
  const fade = edges.top || edges.bottom ? `linear-gradient(to bottom, ${edges.top ? "transparent, #000 28px" : "#000"}, ${edges.bottom ? "#000 calc(100% - 28px), transparent" : "#000"})` : undefined;
  let lastSection: string | undefined;
  return (
    <aside className="fixed inset-y-3 start-3 z-40 hidden w-[76px] flex-col items-center rounded-[24px] border border-line bg-surface/85 py-4 shadow-[inset_0_1px_0_var(--k-border-top)] backdrop-blur-xl lg:flex">
      <Link href="/" className="grid size-11 place-items-center rounded-2xl border border-line bg-surface-3 shadow-[inset_0_1px_0_var(--k-border-top)]">
        <LogoMark size={20} className="text-fg" />
      </Link>
      {/* navigation links prefetch their route in full (prefetch={true}): the Client Area and Back Office routes are
          dynamic (session cookie), which Next otherwise never prefetches, so every click waited a server round trip.
          Their pages are client components with no server data, so the prefetched payload stays valid. */}
      <nav ref={navRef} className={cn("mt-5 flex min-h-0 w-full flex-1 flex-col items-center overflow-y-auto px-2 [scrollbar-width:none]", dense ? "gap-1" : "gap-1.5")} style={fade ? { maskImage: fade, WebkitMaskImage: fade } : undefined}>
        {modules.map((m) => {
          const active = isActive(pathname, m);
          const Icon = m.icon;
          const sep = m.section && m.section !== lastSection && lastSection !== undefined;
          lastSection = m.section ?? lastSection;
          return (
            <React.Fragment key={m.key}>
              {sep && <span className={cn("h-px w-8 shrink-0 bg-line", dense ? "my-1" : "my-1.5")} />}
              <Tooltip content={m.label} side={railSide}>
                <Link
                  href={m.href}
                  prefetch={m.external ? false : true}
                  target={m.external ? "_blank" : undefined}
                  data-rail-active={active || undefined}
                  className={cn("relative grid shrink-0 place-items-center rounded-full transition-colors", dense ? "size-10" : "size-11", active ? "text-fg" : "text-fg-3 hover:bg-surface-3 hover:text-fg")}
                >
                  {active && (
                    <>
                      <motion.span layoutId="rail-active" className="absolute inset-0 rounded-[14px] bg-yellow-soft shadow-[inset_0_0_0_1px_color-mix(in_oklab,var(--k-yellow)_55%,transparent)]" transition={{ type: "tween", duration: 0.18, ease: [0.2, 0.8, 0.2, 1] }} />
                      <motion.span layoutId="rail-bar" className="absolute -start-2 top-1/2 h-5 w-[3px] -translate-y-1/2 rounded-full bg-yellow" />
                    </>
                  )}
                  <Icon className="relative size-[19px]" strokeWidth={1.7} />
                  {m.badge !== undefined && <span className="absolute end-0.5 top-0.5 grid h-4 min-w-4 place-items-center rounded-full bg-red px-1 text-[9px] font-bold text-on-red ring-2 ring-surface">{m.badge}</span>}
                </Link>
              </Tooltip>
            </React.Fragment>
          );
        })}
      </nav>
      <div className="mt-3 flex flex-col items-center gap-2">{footer}</div>
    </aside>
  );
}

/* ------------------------------------------------------------------ */
/* Pill sub-navigation                                                 */
/* ------------------------------------------------------------------ */

function PillNav({ items, variant, fit = false }: { items: SubNavItem[]; variant: "icons" | "text"; fit?: boolean }) {
  const t = useT();
  const pathname = usePathname();
  const MAX = variant === "text" ? 7 : 8;
  const activeIdx = items.findIndex((s) => subActive(pathname, s, items));
  const groupRef = React.useRef<HTMLDivElement>(null);
  const measureRef = React.useRef<HTMLDivElement>(null);
  // null = not measured yet (first paint shows the static MAX split)
  const [fitCount, setFitCount] = React.useState<number | null>(null);

  // When `fit` is on, show as many pills as the space the header gives us allows and move the rest into "More",
  // so the pill group can never push the right-hand controls off-screen.
  React.useLayoutEffect(() => {
    if (!fit) return;
    const wrap = groupRef.current?.parentElement;
    const meas = measureRef.current;
    if (!wrap || !meas) return;
    const compute = () => {
      const avail = wrap.clientWidth;
      const kids = Array.from(meas.children) as HTMLElement[];
      const widths = items.map((_, i) => kids[i]?.offsetWidth ?? 0);
      const moreW = kids[items.length]?.offsetWidth ?? 0;
      const GAP = 4;
      const CHROME = 14; // p-1.5 on both sides + 1px border each side
      const total = CHROME + widths.reduce((a, w) => a + w, 0) + GAP * Math.max(0, items.length - 1);
      if (total <= avail) return setFitCount(items.length);
      // reserve the active pill (it always stays visible) and the More button, then fill in order
      let used = CHROME + moreW + (activeIdx >= 0 ? widths[activeIdx]! + GAP : 0);
      let n = activeIdx >= 0 ? 1 : 0;
      for (let i = 0; i < items.length; i++) {
        if (i === activeIdx) continue;
        if (used + widths[i]! + GAP > avail) break;
        used += widths[i]! + GAP;
        n++;
      }
      setFitCount(n);
    };
    compute();
    const ro = new ResizeObserver(compute);
    ro.observe(wrap);
    return () => ro.disconnect();
  }, [fit, items, activeIdx]);

  const limit = fit && fitCount !== null ? fitCount : MAX;
  let visible: SubNavItem[];
  let overflow: SubNavItem[];
  if (limit >= items.length) {
    visible = items;
    overflow = [];
  } else {
    // keep the active pill visible: take the first (limit - 1) others plus the active one, in nav order
    const others = items.filter((_, i) => i !== activeIdx);
    const keep = new Set(others.slice(0, Math.max(0, activeIdx >= 0 ? limit - 1 : limit)));
    if (activeIdx >= 0 && limit > 0) keep.add(items[activeIdx]!);
    visible = items.filter((s) => keep.has(s));
    overflow = items.filter((s) => !keep.has(s));
  }

  const itemClass = (iconOnly: boolean, active: boolean) =>
    cn("relative flex h-10 shrink-0 items-center gap-2 rounded-full text-[13.5px] font-medium transition-colors", iconOnly ? "w-10 justify-center" : "px-4", active ? "text-fg" : "text-fg-3 hover:text-fg");
  const itemBody = (s: SubNavItem, iconOnly: boolean) => {
    const Icon = s.icon;
    return (
      <>
        {Icon && <Icon className="relative size-[17px]" strokeWidth={1.8} />}
        {!iconOnly && <span className="relative whitespace-nowrap">{s.label}</span>}
        {s.badge !== undefined && <span className="relative grid h-[18px] min-w-[18px] place-items-center rounded-full bg-red-soft px-1 text-[10px] font-semibold text-red">{s.badge}</span>}
      </>
    );
  };
  const moreBtnClass = "flex h-10 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-full px-3.5 text-[13.5px] font-medium text-fg-3 hover:text-fg";

  return (
    <TooltipProvider>
      <div ref={groupRef} className={cn("k-pill-group relative flex items-center gap-1 p-1.5", fit && "max-w-full")}>
        {fit && (
          // invisible measuring row: every pill at its natural width, plus the More button
          <div ref={measureRef} aria-hidden className="pointer-events-none invisible absolute left-0 top-0 flex items-center gap-1 whitespace-nowrap">
            {items.map((s, i) => {
              const iconOnly = variant === "icons" && i !== activeIdx && !!s.icon;
              return (
                <span key={s.href} className={itemClass(iconOnly, i === activeIdx)}>
                  {itemBody(s, iconOnly)}
                </span>
              );
            })}
            <span className={moreBtnClass}>
              {t("shell.more")} <MoreHorizontal className="size-4" />
            </span>
          </div>
        )}
        {visible.map((s) => {
          const active = s === items[activeIdx];
          const iconOnly = variant === "icons" && !active && !!s.icon;
          const link = (
            <Link key={s.href} href={s.href} prefetch className={itemClass(iconOnly, active)}>
              {active && (
                <motion.span
                  layoutId="pill-active"
                  className="absolute inset-0 rounded-full border border-[var(--k-border-top)] bg-surface-3 shadow-[inset_0_1px_0_var(--k-border-top),0_6px_20px_-8px_rgba(0,0,0,0.6)]"
                  transition={{ type: "spring", bounce: 0.18, duration: 0.5 }}
                />
              )}
              {itemBody(s, iconOnly)}
            </Link>
          );
          return iconOnly ? (
            <Tooltip key={s.href} content={s.label} side="bottom">
              {link}
            </Tooltip>
          ) : (
            link
          );
        })}
        {overflow.length > 0 && (
          <Menu
            trigger={
              <button className={moreBtnClass}>
                {t("shell.more")} <MoreHorizontal className="size-4" />
              </button>
            }
            items={overflow.map((o) => ({ label: o.label, href: o.href, icon: o.icon ? <o.icon /> : undefined, hint: o.badge !== undefined ? <span className="k-num text-[11px] text-red">{o.badge}</span> : undefined }))}
          />
        )}
      </div>
    </TooltipProvider>
  );
}

/* ------------------------------------------------------------------ */
/* Mobile bottom bar                                                   */
/* ------------------------------------------------------------------ */

function MobileBar({ modules }: { modules: NavModule[] }) {
  const t = useT();
  const pathname = usePathname();
  const primary = modules.slice(0, 4);
  const rest = modules.slice(4);
  return (
    <nav className="fixed inset-x-3 bottom-3 z-40 flex items-center justify-around rounded-[22px] border border-line bg-surface/90 p-1.5 shadow-[inset_0_1px_0_var(--k-border-top)] backdrop-blur-xl lg:hidden">
      {primary.map((m) => {
        const active = isActive(pathname, m);
        return (
          <Link key={m.key} href={m.href} prefetch={m.external ? false : true} className={cn("flex flex-1 flex-col items-center gap-1 rounded-2xl py-2 text-[10.5px] font-medium", active ? "bg-yellow-soft text-fg" : "text-fg-3")}>
            <m.icon className="size-5" strokeWidth={1.7} />
            {m.label.split(" ")[0]}
          </Link>
        );
      })}
      <Menu
        align="end"
        trigger={
          <button className="flex flex-1 flex-col items-center gap-1 py-2 text-[10.5px] font-medium text-fg-3">
            <MoreHorizontal className="size-5" />
            {t("shell.more")}
          </button>
        }
        items={rest.map((m) => ({ label: m.label, href: m.href, icon: <m.icon /> }))}
      />
    </nav>
  );
}

/* ------------------------------------------------------------------ */
/* Shell                                                               */
/* ------------------------------------------------------------------ */

export function AppShell({
  modules,
  topRight,
  railFooter,
  brandSuffix,
  pillVariant = "icons",
  children,
}: {
  modules: NavModule[];
  topRight?: React.ReactNode;
  railFooter?: React.ReactNode;
  brandSuffix?: React.ReactNode;
  pillVariant?: "icons" | "text";
  children: React.ReactNode;
}) {
  const t = useT();
  const pathname = usePathname();
  const current = modules.find((m) => isActive(pathname, m));
  const railSide = useRailSide();
  return (
    <TooltipProvider>
      <div className="relative min-h-dvh overflow-x-clip">
        <div className="k-noise" />
        <IconRail
          modules={modules}
          side={railSide}
          footer={
            <>
              {railFooter}
              <Tooltip content={t("shell.logOut")} side={railSide}>
                <Link href="/login" className="grid size-10 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg">
                  <LogOut className="size-[18px]" strokeWidth={1.7} />
                </Link>
              </Tooltip>
            </>
          }
        />
        <div className="relative lg:ps-[100px]">
          <header className="sticky top-0 z-30 px-4 pt-3 sm:px-6 lg:px-8">
            <div className="flex h-16 items-center gap-4">
              <Link href="/" className="flex shrink-0 items-center gap-3">
                <Logo height={20} />
                {brandSuffix}
              </Link>
              <div className="hidden min-w-0 flex-1 justify-center xl:flex">{current?.sub && current.sub.length > 1 && <PillNav items={current.sub} variant={pillVariant} fit />}</div>
              <div className="ms-auto flex shrink-0 items-center gap-2 xl:ms-0">{topRight}</div>
            </div>
            {current?.sub && current.sub.length > 1 && (
              <div className="-mx-4 overflow-x-auto px-4 pb-2 [scrollbar-width:none] xl:hidden">
                <div className="inline-flex">
                  <PillNav items={current.sub} variant="text" />
                </div>
              </div>
            )}
          </header>
          <main className="relative mx-auto max-w-[1600px] px-4 pb-28 pt-4 sm:px-6 lg:px-8 lg:pb-12">{children}</main>
        </div>
        <MobileBar modules={modules} />
      </div>
    </TooltipProvider>
  );
}

/** Page title block: greeting/title + subtitle + actions. */
export function PageHeader({ title, subtitle, actions, className }: { title: React.ReactNode; subtitle?: React.ReactNode; actions?: React.ReactNode; className?: string }) {
  return (
    // CSS entrance (styles.css .k-reveal): the title paints with the server HTML instead of after hydration
    <div
      style={{ "--k-reveal-y": "10px", "--k-reveal-ms": "500ms" } as React.CSSProperties}
      className={cn("k-reveal mb-6 flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between", className)}
    >
      <div className="min-w-0">
        <h1 className="text-[26px] font-medium leading-tight tracking-[-0.02em] text-fg sm:text-[30px]">{title}</h1>
        {subtitle && <p className="mt-1.5 text-[14.5px] text-fg-2">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

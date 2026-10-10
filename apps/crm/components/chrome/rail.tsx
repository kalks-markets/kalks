"use client";

// Slim left rail of the Client Area (right in RTL): the broker's logo avatar, icon-only navigation with tooltips
// (the active module marked by an accent notch on the outer edge), settings and log out at the bottom. A toggle
// expands it into a labelled sidebar; the choice is remembered in this browser.

import * as React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { motion } from "motion/react";
import { ChevronsLeft, ChevronsRight, LogOut, Settings } from "lucide-react";
import { Logo, LogoMark, Tooltip, cn, useBrand, type NavModule } from "@/components/kit";
import { useLocale, useT } from "@kalks/i18n/react";
import { isActive } from "./nav-utils";

const KEY = "kalks.crm.rail";

/** Expanded (labelled) or slim rail, remembered per browser. */
export function useRailExpanded(): [boolean, (v: boolean) => void] {
  const [open, setOpen] = React.useState(false);
  React.useEffect(() => {
    try {
      setOpen(window.localStorage.getItem(KEY) === "open");
    } catch {
      /* storage blocked: stay slim */
    }
  }, []);
  const set = React.useCallback((v: boolean) => {
    setOpen(v);
    try {
      window.localStorage.setItem(KEY, v ? "open" : "slim");
    } catch {
      /* ignore */
    }
  }, []);
  return [open, set];
}

/** The broker's mark on a brand-coloured disc (Kalks: the K glyph; a broker: its logo on white, or its initial). */
export function BrandAvatar({ size = 46 }: { size?: number }) {
  const brand = useBrand();
  if (brand?.logo_url)
    return (
      <span className="grid shrink-0 place-items-center overflow-hidden rounded-full bg-white shadow-[0_8px_22px_-10px_var(--k-ember)] ring-1 ring-line" style={{ width: size, height: size }}>
        <LogoMark size={Math.round(size * 0.56)} />
      </span>
    );
  return (
    <span className="k-brand-disc grid shrink-0 place-items-center rounded-full text-white" style={{ width: size, height: size }}>
      {brand ? <span className="text-[19px] font-bold leading-none">{brand.name.trim().charAt(0).toUpperCase()}</span> : <LogoMark size={Math.round(size * 0.42)} className="text-white" />}
    </span>
  );
}

export function Rail({ modules, expanded, onToggle, onSignOut }: { modules: NavModule[]; expanded: boolean; onToggle: () => void; onSignOut: () => void }) {
  const t = useT();
  const pathname = usePathname();
  const { dir } = useLocale();
  const side = dir === "rtl" ? "left" : "right";
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
  const fade = edges.top || edges.bottom ? `linear-gradient(to bottom, ${edges.top ? "transparent, #000 24px" : "#000"}, ${edges.bottom ? "#000 calc(100% - 24px), transparent" : "#000"})` : undefined;
  let last: string | undefined;

  const item = (key: string, label: string, body: React.ReactNode) =>
    expanded ? (
      <React.Fragment key={key}>{body}</React.Fragment>
    ) : (
      <Tooltip key={key} content={label} side={side}>
        {body}
      </Tooltip>
    );
  const rowCls = (active: boolean) =>
    cn(
      "group/rail relative flex h-10 shrink-0 items-center rounded-[13px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ember/40",
      expanded ? "w-full gap-3 px-3" : "w-10 justify-center",
      active ? "bg-ember-soft text-ember" : "text-fg-3 hover:bg-surface-3/70 hover:text-fg",
    );

  return (
    <aside className={cn("k-rail fixed inset-y-0 start-0 z-40 hidden flex-col py-4 transition-[width] duration-300 lg:flex", expanded ? "w-[248px] px-4" : "w-[84px] items-center px-0")}>
      <Link href="/" className={cn("flex h-[46px] shrink-0 items-center gap-3", expanded ? "px-1" : "")} aria-label={t("shell.nav.dashboard")}>
        {/* open rail: the full Kalks wordmark (a broker: its logo or name); slim rail: the round K */}
        {expanded ? <Logo height={26} className="text-white" /> : <BrandAvatar size={46} />}
      </Link>
      <span className={cn("mt-4 block h-px shrink-0 bg-line", expanded ? "mx-1" : "w-10")} />
      <nav ref={navRef} className={cn("mt-3 flex min-h-0 flex-1 flex-col gap-[3px] overflow-y-auto [scrollbar-width:none]", expanded ? "-mx-4 px-4" : "w-full items-center")} style={fade ? { maskImage: fade, WebkitMaskImage: fade } : undefined}>
        {modules.map((m) => {
          const active = isActive(pathname, m);
          const Icon = m.icon;
          const sep = m.section && last !== undefined && m.section !== last;
          last = m.section ?? last;
          return (
            <React.Fragment key={m.key}>
              {sep && <span className={cn("my-1 block h-px shrink-0 bg-line", expanded ? "mx-3" : "w-6 self-center")} />}
              {item(
                m.key,
                m.label,
                <Link href={m.href} prefetch={m.external ? false : true} target={m.external ? "_blank" : undefined} data-rail-active={active || undefined} aria-current={active ? "page" : undefined} className={rowCls(active)}>
                  {active && <motion.span layoutId="crm-rail-notch" aria-hidden className="k-rail-notch" transition={{ type: "spring", bounce: 0.2, duration: 0.5 }} />}
                  <Icon className="relative size-[19px] shrink-0" strokeWidth={active ? 2 : 1.75} />
                  {expanded && <span className="relative min-w-0 flex-1 truncate text-[13.5px] font-semibold">{m.label}</span>}
                  {m.badge !== undefined && (
                    <span className={cn("grid h-4 min-w-4 place-items-center rounded-full bg-ember px-1 text-[9px] font-bold text-[var(--k-on-ember)] ring-2 ring-[var(--k-rail-bg-solid)]", expanded ? "relative" : "absolute end-0.5 top-0.5")}>{m.badge}</span>
                  )}
                </Link>,
              )}
            </React.Fragment>
          );
        })}
      </nav>
      <div className={cn("mt-2 flex shrink-0 flex-col gap-[3px]", expanded ? "" : "items-center")}>
        {item(
          "settings",
          t("shell.preferences"),
          <Link href="/profile/preferences" className={rowCls(pathname === "/profile/preferences")}>
            <Settings className="size-[20px] shrink-0" strokeWidth={1.75} />
            {expanded && <span className="flex-1 truncate text-[13.5px] font-semibold">{t("shell.preferences")}</span>}
          </Link>,
        )}
        {item(
          "logout",
          t("shell.logOut"),
          <button type="button" onClick={onSignOut} className={rowCls(false)}>
            <LogOut className="size-[20px] shrink-0 rtl:-scale-x-100" strokeWidth={1.75} />
            {expanded && <span className="flex-1 truncate text-start text-[13.5px] font-semibold">{t("shell.logOut")}</span>}
          </button>,
        )}
      </div>
      {/* collapse / expand, sitting on the rail's outer edge */}
      <Tooltip content={expanded ? t("dashboard.chrome.collapse") : t("dashboard.chrome.expand")} side={side}>
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={expanded}
          aria-label={expanded ? t("dashboard.chrome.collapse") : t("dashboard.chrome.expand")}
          className="k-surface-btn absolute -end-3.5 top-[86px] grid size-7 place-items-center rounded-full text-fg-2 hover:text-fg"
        >
          {expanded ? <ChevronsLeft className="size-3.5 rtl:-scale-x-100" /> : <ChevronsRight className="size-3.5 rtl:-scale-x-100" />}
        </button>
      </Tooltip>
    </aside>
  );
}

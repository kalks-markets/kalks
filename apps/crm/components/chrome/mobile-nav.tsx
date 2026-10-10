"use client";

// Phones and tablets: a frosted bottom bar with the four main modules and "More", which opens a drawer listing every
// module (with its pages), plus language, theme, settings and log out. Touch targets are at least 44px.

import * as React from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { ChevronRight, LayoutGrid, LogOut, Moon, Settings, Sun } from "lucide-react";
import { Dialog, LanguageMenu, cn, type NavModule } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { activeSub, isActive } from "./nav-utils";

export function MobileBar({ modules, onSignOut }: { modules: NavModule[]; onSignOut: () => void }) {
  const t = useT();
  const pathname = usePathname();
  const [open, setOpen] = React.useState(false);
  const primary = modules.slice(0, 4);
  const moreActive = !primary.some((m) => isActive(pathname, m));
  React.useEffect(() => setOpen(false), [pathname]);
  return (
    <>
      <nav aria-label={t("dashboard.chrome.menu")} className="k-mobilebar fixed inset-x-3 bottom-[max(12px,env(safe-area-inset-bottom))] z-40 flex items-stretch justify-around rounded-[24px] p-1.5 lg:hidden">
        {primary.map((m) => {
          const active = isActive(pathname, m);
          return (
            <Link key={m.key} href={m.href} prefetch={m.external ? false : true} aria-current={active ? "page" : undefined} className={cn("relative flex min-h-14 min-w-0 flex-1 flex-col items-center justify-center gap-1 rounded-[18px] px-1 text-[10.5px] font-semibold", active ? "text-ember" : "text-fg-3")}>
              {active && <span aria-hidden className="absolute top-0 h-[3px] w-6 rounded-b-full bg-ember" />}
              <m.icon className="size-[21px]" strokeWidth={active ? 2 : 1.75} />
              <span className="max-w-full truncate">{m.label}</span>
            </Link>
          );
        })}
        <button type="button" onClick={() => setOpen(true)} aria-haspopup="dialog" className={cn("relative flex min-h-14 min-w-0 flex-1 flex-col items-center justify-center gap-1 rounded-[18px] px-1 text-[10.5px] font-semibold", moreActive ? "text-ember" : "text-fg-3")}>
          {moreActive && <span aria-hidden className="absolute top-0 h-[3px] w-6 rounded-b-full bg-ember" />}
          <LayoutGrid className="size-[21px]" strokeWidth={moreActive ? 2 : 1.75} />
          <span>{t("shell.more")}</span>
        </button>
      </nav>
      <Dialog open={open} onOpenChange={setOpen} side="right" title={t("dashboard.chrome.menu")}>
        <Drawer modules={modules} pathname={pathname} onSignOut={onSignOut} />
      </Dialog>
    </>
  );
}

function Drawer({ modules, pathname, onSignOut }: { modules: NavModule[]; pathname: string; onSignOut: () => void }) {
  const t = useT();
  const [openKey, setOpenKey] = React.useState<string | null>(() => modules.find((m) => isActive(pathname, m))?.key ?? null);
  return (
    <div className="-mx-2 space-y-1 pb-2">
      {modules.map((m) => {
        const active = isActive(pathname, m);
        const subs = m.sub && m.sub.length > 1 ? m.sub : null;
        const expanded = openKey === m.key;
        const cur = subs ? activeSub(pathname, subs) : undefined;
        return (
          <div key={m.key} className={cn("rounded-[18px]", expanded && subs ? "bg-surface-2" : "")}>
            <div className="flex items-center">
              <Link href={m.href} className={cn("flex min-h-12 min-w-0 flex-1 items-center gap-3 rounded-[16px] px-2.5 text-[14.5px] font-semibold", active ? "text-ember" : "text-fg")}>
                <span className={cn("k-tile size-10 shrink-0 rounded-[12px] [&_svg]:size-[19px]", active ? "k-tile-accent" : "k-tile-neutral")}>
                  <m.icon strokeWidth={1.8} />
                </span>
                <span className="truncate">{m.label}</span>
              </Link>
              {subs && (
                <button type="button" onClick={() => setOpenKey(expanded ? null : m.key)} aria-expanded={expanded} aria-label={m.label} className="grid size-12 shrink-0 place-items-center rounded-full text-fg-3">
                  <ChevronRight className={cn("size-4 transition-transform rtl:-scale-x-100", expanded && "rotate-90 rtl:-rotate-90")} />
                </button>
              )}
            </div>
            {subs && expanded && (
              <div className="grid grid-cols-2 gap-1 px-2 pb-2">
                {subs.map((s) => (
                  <Link key={s.href} href={s.href} className={cn("flex min-h-11 items-center gap-2 rounded-[12px] px-3 text-[13px] font-medium", s === cur ? "bg-surface text-ember shadow-[var(--k-shadow-card)]" : "text-fg-2")}>
                    {s.icon && <s.icon className="size-4 shrink-0" />}
                    <span className="truncate">{s.label}</span>
                  </Link>
                ))}
              </div>
            )}
          </div>
        );
      })}
      <div className="mt-3 flex items-center gap-2 border-t border-line px-2 pt-4">
        <LanguageMenu />
        <Link href="/profile/preferences" aria-label={t("shell.preferences")} className="k-surface-btn grid size-11 place-items-center rounded-full text-fg-2">
          <Settings className="size-[18px]" />
        </Link>
        <button type="button" onClick={onSignOut} className="ms-auto flex h-11 items-center gap-2 rounded-full px-3 text-[13px] font-semibold text-down">
          <LogOut className="size-4 rtl:-scale-x-100" /> {t("shell.logOut")}
        </button>
      </div>
    </div>
  );
}

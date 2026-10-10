"use client";

import * as React from "react";
import { useRouter } from "next/navigation";
import { useTheme } from "next-themes";
import { Command } from "cmdk";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { Bell, Check, Moon, Search, Sun, CornerDownLeft } from "lucide-react";
import { IconButton, Kbd } from "../components/primitives";
import { Popover } from "../components/overlays";
import { Flag } from "../components/avatars";
import { cn } from "../lib/cn";
import { LOCALES } from "@kalks/i18n/locales";
import { useLocale, useT } from "@kalks/i18n/react";

/* ------------------------------------------------------------------ */
/* Theme                                                               */
/* ------------------------------------------------------------------ */

export function ThemeToggle() {
  const t = useT();
  const { resolvedTheme, setTheme } = useTheme();
  const [mounted, setMounted] = React.useState(false);
  React.useEffect(() => setMounted(true), []);
  const dark = !mounted || resolvedTheme !== "light";
  return (
    <IconButton aria-label={t("shell.toggleTheme")} onClick={() => setTheme(dark ? "light" : "dark")}>
      {dark ? <Moon /> : <Sun />}
    </IconButton>
  );
}

/* ------------------------------------------------------------------ */
/* Language (all major languages; RTL switches document direction)    */
/* ------------------------------------------------------------------ */

/** Supported interface languages (from @kalks/i18n; `rtl` marks right-to-left scripts). */
export const LANGUAGES = LOCALES;

export function LanguageMenu() {
  const t = useT();
  const { locale: lang, setLocale } = useLocale();
  const [q, setQ] = React.useState("");
  const current = LANGUAGES.find((l) => l.code === lang) ?? LANGUAGES[0];
  const query = q.trim().toLowerCase();
  const list = LANGUAGES.filter((l) => !query || l.name.toLowerCase().includes(query) || l.english.toLowerCase().includes(query) || l.code.includes(query));
  return (
    <Popover
      width={280}
      trigger={
        <IconButton aria-label={t("shell.language")}>
          <Flag country={current.flag} className="size-[18px]" />
        </IconButton>
      }
    >
      <div className="p-2">
        <div className="flex items-center gap-2 rounded-xl border border-line bg-surface-2 px-3">
          <Search className="size-4 text-fg-3" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("shell.searchLanguage")} className="h-9 flex-1 bg-transparent text-sm outline-none placeholder:text-fg-3" />
        </div>
        <div className="mt-2 max-h-72 overflow-y-auto">
          {list.map((l) => (
            <button key={l.code} lang={l.code} onClick={() => void setLocale(l.code)} className={cn("flex w-full items-center gap-3 rounded-xl px-3 py-2 text-start text-sm hover:bg-surface-3", l.code === lang ? "text-fg" : "text-fg-2")}>
              <Flag country={l.flag} className="size-[18px]" />
              <span className="flex-1">{l.name}</span>
              {"rtl" in l && l.rtl && <span className="rounded bg-surface-3 px-1.5 text-[10px] text-fg-3">RTL</span>}
              {l.code === lang && <Check className="size-4 text-fg" />}
            </button>
          ))}
        </div>
      </div>
    </Popover>
  );
}

/* ------------------------------------------------------------------ */
/* Notifications                                                       */
/* ------------------------------------------------------------------ */

export function NotificationsPopover({ items }: { items: { id: string; title: string; time: string; unread: boolean; icon?: React.ReactNode }[] }) {
  const t = useT();
  const [list, setList] = React.useState(items);
  const unread = list.filter((i) => i.unread).length;
  return (
    <Popover
      width={380}
      trigger={
        <IconButton aria-label={t("shell.notifications")} dot={unread > 0}>
          <Bell />
        </IconButton>
      }
    >
      <div className="flex items-center justify-between border-b border-line px-4 py-3">
        <div className="text-sm font-medium">
          {t("shell.notifications")} {unread > 0 && <span className="ms-1 rounded-full bg-red px-1.5 text-[11px] font-semibold text-on-red">{unread}</span>}
        </div>
        <button onClick={() => setList((l) => l.map((i) => ({ ...i, unread: false })))} className="text-xs text-fg-3 hover:text-fg">
          {t("shell.markAllRead")}
        </button>
      </div>
      <div className="max-h-96 overflow-y-auto p-1.5">
        {list.map((n) => (
          <div key={n.id} className="flex gap-3 rounded-xl px-3 py-2.5 hover:bg-surface-3">
            <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-surface-3 text-fg-2 [&_svg]:size-4">{n.icon}</span>
            <div className="min-w-0 flex-1">
              <p className={cn("text-[13px] leading-snug", n.unread ? "text-fg" : "text-fg-2")}>{n.title}</p>
              <p className="mt-0.5 text-[11.5px] text-fg-3">{t("shell.timeAgo", { time: n.time })}</p>
            </div>
            {n.unread && <span className="mt-2 size-2 shrink-0 rounded-full bg-neon-red" />}
          </div>
        ))}
      </div>
    </Popover>
  );
}

/* ------------------------------------------------------------------ */
/* ⌘K command palette                                                  */
/* ------------------------------------------------------------------ */

export interface CommandItem {
  group: string;
  label: string;
  href: string;
  icon?: React.ReactNode;
  keywords?: string;
}

export function CommandPalette({ items, placeholder, compact }: { items: CommandItem[]; placeholder?: string; compact?: boolean }) {
  const t = useT();
  const [open, setOpen] = React.useState(false);
  const router = useRouter();
  React.useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen((o) => !o);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const groups = Array.from(new Set(items.map((i) => i.group)));
  return (
    <>
      <button onClick={() => setOpen(true)} className={cn("hidden h-10 items-center gap-2 rounded-full border border-line bg-surface/70 pl-3.5 pr-2 text-sm text-fg-3 shadow-[inset_0_1px_0_var(--k-border-top)] hover:text-fg-2", !compact && "md:flex")}>
        <Search className="size-4" />
        <span className="w-28 text-start">{t("shell.search")}</span>
        <Kbd>⌘K</Kbd>
      </button>
      <IconButton className={compact ? undefined : "md:hidden"} aria-label={t("shell.search")} title={t("shell.search")} onClick={() => setOpen(true)}>
        <Search />
      </IconButton>
      <DialogPrimitive.Root open={open} onOpenChange={setOpen}>
        <DialogPrimitive.Portal>
          <DialogPrimitive.Overlay className="fixed inset-0 z-50 bg-black/60 backdrop-blur-sm" />
          <DialogPrimitive.Content className="k-card fixed left-1/2 top-[14vh] z-50 w-[calc(100vw-24px)] max-w-[620px] -translate-x-1/2 overflow-hidden rounded-[22px] bg-surface outline-none">
            <DialogPrimitive.Title className="sr-only">{t("shell.search")}</DialogPrimitive.Title>
            <Command loop>
              <div className="flex items-center gap-3 border-b border-line px-5">
                <Search className="size-4 text-fg-3" />
                <Command.Input autoFocus placeholder={placeholder ?? t("shell.searchPalette")} className="h-14 flex-1 bg-transparent text-[15px] outline-none placeholder:text-fg-3" />
                <Kbd>ESC</Kbd>
              </div>
              <Command.List className="max-h-[420px] overflow-y-auto p-2">
                <Command.Empty className="px-4 py-10 text-center text-sm text-fg-3">{t("shell.noResults")}</Command.Empty>
                {groups.map((g) => (
                  <Command.Group key={g} heading={g} className="[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-3 [&_[cmdk-group-heading]]:text-[11px] [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-wider [&_[cmdk-group-heading]]:text-fg-3">
                    {items
                      .filter((i) => i.group === g)
                      .map((i) => (
                        <Command.Item
                          key={i.href + i.label}
                          value={`${i.label} ${i.keywords ?? ""}`}
                          onSelect={() => {
                            setOpen(false);
                            router.push(i.href);
                          }}
                          className="group flex cursor-pointer items-center gap-3 rounded-xl px-3 py-2.5 text-sm text-fg-2 data-[selected=true]:bg-surface-3 data-[selected=true]:text-fg"
                        >
                          <span className="grid size-8 place-items-center rounded-lg border border-line bg-surface-2 [&_svg]:size-4">{i.icon}</span>
                          <span className="flex-1">{i.label}</span>
                          <CornerDownLeft className="size-3.5 opacity-0 group-data-[selected=true]:opacity-100" />
                        </Command.Item>
                      ))}
                  </Command.Group>
                ))}
              </Command.List>
            </Command>
          </DialogPrimitive.Content>
        </DialogPrimitive.Portal>
      </DialogPrimitive.Root>
    </>
  );
}

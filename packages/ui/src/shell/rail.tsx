"use client";

// Kalks 2 shell pieces (docs/design/KALKS2.md §6): the Client Area's black icon rail (the ink tab bar on phones) and
// the top bar (date kicker, greeting title, search ⌘K, account switcher pill, one yellow Deposit).

import * as React from "react";
import Link from "next/link";
import { ChevronDown, Search } from "lucide-react";
import { cn } from "../lib/cn";
import { Tag, type TagTone } from "../components/primitives";
import { Tooltip } from "../components/overlays";
import { LogoMark } from "./logo";

export type RailItem = {
  key: string;
  label: string;
  icon: React.ReactNode;
  href?: string;
  onClick?: () => void;
  active?: boolean;
  /** neon-red unread dot */
  dot?: boolean;
};

// forwards ref + props: it is the trigger of a Tooltip (asChild)
const RailButton = React.forwardRef<HTMLElement, { item: RailItem; className: string } & React.HTMLAttributes<HTMLElement>>(function RailButton({ item, className, ...rest }, ref) {
  const common = {
    ...rest,
    "aria-label": item.label,
    "aria-current": item.active ? ("page" as const) : undefined,
    className: cn(className, item.dot && "k-dot"),
  };
  return item.href ? (
    <Link ref={ref as React.Ref<HTMLAnchorElement>} href={item.href} {...common}>
      {item.icon}
    </Link>
  ) : (
    <button
      ref={ref as React.Ref<HTMLButtonElement>}
      type="button"
      {...common}
      onClick={(e) => {
        rest.onClick?.(e);
        item.onClick?.();
      }}
    >
      {item.icon}
    </button>
  );
});

/**
 * The black rail (both themes): the K on top, 44 px square buttons (radius 14, icon 20); the active one is a yellow
 * square on a 2 px edge. `bottom` items (alerts, settings) sit at the foot, then `footer` (the avatar).
 */
export function IconRail({ items, bottom, footer, logo, label = "Main", className }: { items: readonly RailItem[]; bottom?: readonly RailItem[]; footer?: React.ReactNode; logo?: React.ReactNode; label?: string; className?: string }) {
  return (
    <nav aria-label={label} className={cn("k-irail", className)}>
      <div className="mb-3.5 grid size-11 place-items-center">{logo ?? <LogoMark size={28} />}</div>
      {items.map((it) => (
        <Tooltip key={it.key} content={it.label} side="right">
          <RailButton item={it} className="k-irail-btn" />
        </Tooltip>
      ))}
      <span className="flex-1" />
      {bottom?.map((it) => (
        <Tooltip key={it.key} content={it.label} side="right">
          <RailButton item={it} className="k-irail-btn" />
        </Tooltip>
      ))}
      {footer && <div className="mt-1.5">{footer}</div>}
    </nav>
  );
}

/** The rail on phones: an ink bar (64 px, radius 24), the active icon in a disc (white in light, yellow in dark). */
export function TabBar({ items, label = "Main", className }: { items: readonly RailItem[]; label?: string; className?: string }) {
  return (
    <nav aria-label={label} className={cn("k-tabbar", className)}>
      {items.map((it) => (
        <RailButton key={it.key} item={it} className="k-tabbar-btn relative" />
      ))}
    </nav>
  );
}

/** Round avatar with initials on the dark-red gradient. */
export function RailAvatar({ initials, src, label }: { initials: string; src?: string; label?: string }) {
  return (
    <span role="img" aria-label={label ?? initials} className="grid size-9 shrink-0 place-items-center overflow-hidden rounded-full bg-[linear-gradient(135deg,#3A0A12,#D4112A)] text-[13px] font-bold text-white">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      {src ? <img src={src} alt="" className="size-full object-cover" /> : initials}
    </span>
  );
}

/**
 * Page top bar: mono kicker (the date), the title ("Good evening, <b>Name</b>": Archivo 34, the name 800), then
 * the actions on the end side (search, account pill, one yellow Deposit — nothing else).
 */
export function TopBar({ kicker, title, children, className }: { kicker?: React.ReactNode; title: React.ReactNode; children?: React.ReactNode; className?: string }) {
  return (
    <header className={cn("flex flex-wrap items-center gap-3 md:flex-nowrap md:gap-4", className)}>
      <div className="min-w-0">
        {kicker && <div className="mb-[9px] font-mono text-[12.5px] font-medium leading-none text-fg-3">{kicker}</div>}
        <h1 className="truncate font-display text-[28px] font-normal leading-none tracking-[-0.03em] wdth-112 md:text-[34px] [&_b]:font-extrabold">{title}</h1>
      </div>
      {children && <div className="ms-auto flex items-center gap-2.5">{children}</div>}
    </header>
  );
}

/** Glass search field that opens the command palette (shows the ⌘K key cap). */
export function SearchButton({ placeholder, onClick, shortcut = "⌘K", className }: { placeholder: string; onClick?: () => void; shortcut?: string; className?: string }) {
  return (
    <button type="button" onClick={onClick} className={cn("k-glass flex h-[42px] w-[250px] items-center gap-[9px] rounded-[13px] pe-2 ps-3.5 text-[14px] font-medium text-fg-3", className)}>
      <Search className="size-[18px] shrink-0" />
      <span className="truncate">{placeholder}</span>
      {shortcut && <kbd className="k-kbd ms-auto">{shortcut}</kbd>}
    </button>
  );
}

/** Account switcher pill: product tag, the login, the balance (mono), a chevron. */
export function AccountPill({ tag, tagTone = "cfd", login, balance, onClick, className, "aria-label": ariaLabel }: { tag: string; tagTone?: TagTone; login: React.ReactNode; balance: React.ReactNode; onClick?: () => void; className?: string; "aria-label"?: string }) {
  return (
    <button type="button" onClick={onClick} aria-label={ariaLabel} aria-haspopup="menu" className={cn("k-glass flex h-[42px] items-center gap-2.5 rounded-[13px] pe-3 ps-2 text-[13.5px] font-semibold", className)}>
      <Tag tone={tagTone} className="h-[22px]">
        {tag}
      </Tag>
      <span className="text-fg-2">{login}</span>
      <span className="k-num font-mono">{balance}</span>
      <ChevronDown className="size-4 text-fg-3" />
    </button>
  );
}

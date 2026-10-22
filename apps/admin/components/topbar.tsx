"use client";

import * as React from "react";
import Link from "next/link";
import { Check, ChevronDown, LogOut, ShieldCheck, UserRound, KeyRound } from "lucide-react";
import { Avatar, Chip, CommandPalette, Menu, ThemeToggle, cn } from "@kalks/ui";
import { NotificationBell } from "@/components/notifications";
import { IS_DEMO } from "@kalks/mock/mode";
import { NAV, navCommands } from "@/lib/live";
import { navFor } from "@/lib/access";
import { signOut, useStaff } from "@/components/staff-session";

const TENANTS = [
  { id: "kalks", name: "Kalks Markets", plan: "Owner", color: "#ff5a1f" },
  { id: "aurum", name: "Aurum FX", plan: "Enterprise", color: "#e9b949" },
  { id: "nova", name: "NovaTrade Asia", plan: "Growth", color: "#38bdf8" },
  { id: "dunes", name: "Dunes Capital", plan: "Growth", color: "#22c55e" },
];

function TenantSwitcher() {
  const [t, setT] = React.useState(TENANTS[0]!);
  return (
    <Menu
      align="start"
      width={260}
      header={<div className="text-[11px] uppercase tracking-wider text-fg-3">Switch broker (platform owner)</div>}
      items={TENANTS.map((x) => ({
        label: (
          <span className="flex items-center justify-between gap-2">
            {x.name}
            <span className="text-[11px] text-fg-3">{x.plan}</span>
          </span>
        ),
        icon: <span className="grid size-5 place-items-center rounded-md text-[10px] font-bold text-black" style={{ background: x.color }}>{x.name[0]}</span>,
        onSelect: () => setT(x),
        hint: x.id === t.id ? <Check className="size-3.5 text-ember" /> : undefined,
      }))}
      trigger={
        <button className="hidden h-10 items-center gap-2 whitespace-nowrap rounded-full border border-line bg-surface/70 pl-1.5 pr-3 text-[13px] font-medium shadow-[inset_0_1px_0_var(--k-border-top)] hover:bg-surface-3 min-[1760px]:flex">
          <span className="grid size-7 place-items-center rounded-full text-[11px] font-bold text-black" style={{ background: t.color }}>
            {t.name[0]}
          </span>
          {t.name}
          <ChevronDown className="size-3.5 text-fg-3" />
        </button>
      }
    />
  );
}

function ServerClock() {
  const [now, setNow] = React.useState<string>("--:--:--");
  React.useEffect(() => {
    const f = () => {
      const d = new Date();
      setNow(`${String((d.getUTCHours() + 3) % 24).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}:${String(d.getUTCSeconds()).padStart(2, "0")}`);
    };
    f();
    const t = setInterval(f, 1000);
    return () => clearInterval(t);
  }, []);
  return (
    <span className="hidden h-10 items-center whitespace-nowrap rounded-full border border-line bg-surface/70 px-3.5 font-mono text-[12.5px] text-fg-2 shadow-[inset_0_1px_0_var(--k-border-top)] min-[1900px]:flex">
      <span className="mr-1.5 text-fg-3">GMT+3</span>
      {now}
    </span>
  );
}

function FeedPill() {
  const [ms, setMs] = React.useState(38);
  React.useEffect(() => {
    const t = setInterval(() => setMs(32 + Math.round(Math.random() * 14)), 2000);
    return () => clearInterval(t);
  }, []);
  return (
    <Link href="/command/system" className="hidden h-10 items-center gap-2 whitespace-nowrap rounded-full border border-line bg-surface/70 px-3.5 text-[12.5px] text-fg-2 shadow-[inset_0_1px_0_var(--k-border-top)] hover:text-fg 2xl:flex">
      <span className="relative size-2 rounded-full bg-up text-up">
        <span className="absolute inset-0 animate-pulse-dot rounded-full" />
      </span>
      Infoways · live · <span className="k-num font-mono text-fg">{ms}ms</span>
    </Link>
  );
}

type HealthService = { key: string; name: string; ok: boolean; latency_ms: number };

/** Live builds: real market-data status from /api/admin/health (polled every 30s). */
function LiveFeedPill() {
  const [md, setMd] = React.useState<HealthService | null>(null);
  React.useEffect(() => {
    let alive = true;
    const load = () =>
      fetch("/api/admin/health", { cache: "no-store" })
        .then((r) => (r.ok ? r.json() : null))
        .then((d: { services?: HealthService[] } | null) => alive && setMd(d?.services?.find((s) => s.key === "market-data") ?? null))
        .catch(() => alive && setMd(null));
    void load();
    const t = setInterval(load, 30_000);
    return () => {
      alive = false;
      clearInterval(t);
    };
  }, []);
  const ok = md?.ok ?? false;
  return (
    <Link href="/" className="hidden h-10 items-center gap-2 whitespace-nowrap rounded-full border border-line bg-surface/70 px-3.5 text-[12.5px] text-fg-2 shadow-[inset_0_1px_0_var(--k-border-top)] hover:text-fg 2xl:flex">
      <span className={cn("size-2 rounded-full", md ? (ok ? "bg-up" : "bg-down") : "bg-fg-3")} />
      Market data · {md ? (ok ? "live" : "degraded") : "checking"}
      {md && <span className="k-num font-mono text-fg">{md.latency_ms}ms</span>}
    </Link>
  );
}

function QueueChips() {
  const q = [
    { label: "KYC", n: 12, href: "/clients/kyc" },
    { label: "WDL", n: 7, href: "/finance/withdrawals" },
    { label: "PAY", n: 3, href: "/finance/payouts" },
  ];
  return (
    <div className="hidden items-center gap-1.5 min-[1400px]:flex">
      {q.map((x) => (
        <Link key={x.label} href={x.href} className={cn("flex h-10 items-center gap-1.5 rounded-full border border-ember/25 bg-ember-soft px-3 text-[12px] font-semibold text-ember hover:border-ember/50")}>
          {x.label}
          <span className="k-num rounded-full bg-ember px-1.5 text-[10.5px] text-white">{x.n}</span>
        </Link>
      ))}
    </div>
  );
}

export function AdminTopRight() {
  const staff = useStaff();
  // same filter as the rail: no pages the role can't open or whose module is off for the broker
  const commands = React.useMemo(() => navCommands(IS_DEMO ? NAV : navFor(NAV, staff)), [staff]);
  return (
    <>
      {IS_DEMO && <TenantSwitcher />}
      {IS_DEMO ? <FeedPill /> : <LiveFeedPill />}
      <ServerClock />
      {IS_DEMO && <QueueChips />}
      <CommandPalette placeholder={IS_DEMO ? "Search user, account #, tx hash, page…" : "Jump to a page…"} items={commands.map((c) => ({ group: c.group, label: c.label, href: c.href, icon: <c.Icon /> }))} />
      <ThemeToggle />
      <NotificationBell />
      <Menu
        width={250}
        header={
          <div className="flex items-center gap-3">
            <Avatar name={staff.name} size={40} />
            <div className="min-w-0">
              <div className="truncate text-sm font-medium">{staff.name}</div>
              <div className="truncate text-xs text-fg-3">{staff.role_label}</div>
              <Chip size="sm" tone="up" className="mt-1.5" dot>
                {staff.tenant.name}
              </Chip>
            </div>
          </div>
        }
        items={[
          { label: "My profile", icon: <UserRound />, href: "/org" },
          ...(IS_DEMO ? [{ label: "Change password", icon: <KeyRound /> }] : []),
          { label: "My audit trail", icon: <ShieldCheck />, href: IS_DEMO ? "/security" : `/security?actor=staff:${staff.id}` },
          "sep",
          { label: "Sign out", icon: <LogOut />, danger: true, onSelect: () => void signOut() },
        ]}
        trigger={
          <button aria-label="Staff menu" className="rounded-full">
            <Avatar name={staff.name} size={40} online />
          </button>
        }
      />
    </>
  );
}

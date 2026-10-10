"use client";

// Home's money summary under the hero (founder 2026-10-10: the hero shows only the name and the AI bar; balances move
// down into one stylish strip). Six cells on one glass card: total balance, equity, wallet, today's P&L with the last
// 7 days, open positions, rewards.

import * as React from "react";
import Link from "next/link";
import { cn } from "@/components/kit";
import { HideMoneyButton, MASK, useHideMoney } from "@/components/hide-money";

export type StripCell = {
  key: string;
  label: React.ReactNode;
  value: React.ReactNode;
  unit?: React.ReactNode;
  sub?: React.ReactNode;
  extra?: React.ReactNode;
  href?: string;
  big?: boolean;
  /** an amount: masked when the client hides amounts */
  secret?: boolean;
};

export function BalanceStrip({ cells }: { cells: StripCell[] }) {
  const [hidden] = useHideMoney();
  return (
    <div className="k-card mt-6 grid grid-cols-2 overflow-hidden rounded-[26px] sm:grid-cols-3 xl:grid-cols-[1.35fr_repeat(5,minmax(0,1fr))]">
      {cells.map((c, i) => {
        const masked = hidden && c.secret;
        const inner = (
          <>
            <div className={cn("text-[11px] font-semibold uppercase tracking-[0.1em] text-fg-3", i === 0 && "pe-8")}>{c.label}</div>
            <div className="mt-2 flex min-w-0 items-baseline gap-1.5">
              <span className={cn("k-num whitespace-nowrap font-light leading-none tracking-[-0.03em]", c.big ? "text-[clamp(22px,12cqw,34px)]" : "text-[clamp(16px,11.5cqw,26px)]")}>{masked ? MASK : c.value}</span>
              {c.unit && !masked && <span className="shrink-0 text-[10.5px] font-semibold uppercase tracking-[0.08em] text-fg-3">{c.unit}</span>}
            </div>
            {c.sub && <div className="mt-2 truncate text-[12px] text-fg-3">{c.sub}</div>}
            {c.extra && !masked && <div className="mt-3">{c.extra}</div>}
          </>
        );
        const cls = cn("block h-full p-5 transition-colors sm:p-6", c.href && "hover:bg-surface-3");
        return (
          // hairlines between cells on wide screens
          <div key={c.key} className={cn("relative min-w-0 border-line [container-type:inline-size]", i > 0 && "xl:border-s")}>
            {c.href ? (
              <Link href={c.href} className={cls}>
                {inner}
              </Link>
            ) : (
              <div className={cls}>{inner}</div>
            )}
            {/* the eye beside the total: hides every amount on every page at once */}
            {i === 0 && <HideMoneyButton className="absolute end-3 top-3.5 sm:end-4 sm:top-4" />}
          </div>
        );
      })}
    </div>
  );
}

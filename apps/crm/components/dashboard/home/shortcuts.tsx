"use client";

// Home's shortcut buttons (founder 2026-10-10: "instead of the extra cards, give shortcut buttons"): one glass tile per
// destination, icon in an orange-tinted square + a short label. Modules the broker switched off are left out by the
// caller.

import * as React from "react";
import Link from "next/link";
import { cn } from "@/components/kit";

export type Shortcut = { key: string; label: string; href: string; icon: React.ReactNode; external?: boolean };

export function Shortcuts({ items }: { items: Shortcut[] }) {
  return (
    <div className="grid grid-cols-3 gap-3 sm:grid-cols-4 md:grid-cols-6 xl:grid-cols-4 2xl:grid-cols-5">
      {items.map((s) => {
        const body = (
          <>
            <span className="grid size-11 place-items-center rounded-[14px] bg-red-soft text-red transition-colors group-hover:bg-red group-hover:text-on-red [&_svg]:size-5">
              {s.icon}
            </span>
            <span className="line-clamp-2 text-center text-[12.5px] font-semibold leading-tight text-fg">{s.label}</span>
          </>
        );
        const cls = cn(
          "k-card group flex min-h-[112px] flex-col items-center justify-center gap-2.5 rounded-[22px] px-2 py-4 transition-[transform,border-color] hover:-translate-y-0.5 hover:border-[color-mix(in_oklab,var(--k-red)_45%,transparent)]",
        );
        return s.external ? (
          <a key={s.key} href={s.href} target="_blank" rel="noopener" className={cls}>
            {body}
          </a>
        ) : (
          <Link key={s.key} href={s.href} className={cls}>
            {body}
          </Link>
        );
      })}
    </div>
  );
}

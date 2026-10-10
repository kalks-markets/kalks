"use client";

// "Hide amounts" (founder 2026-10-10): one eye button hides the balance, equity, wallet, P&L and every account card at
// once, on every page, and stays hidden on this device until shown again.

import * as React from "react";
import { Eye, EyeOff } from "lucide-react";
import { useT } from "@kalks/i18n/react";
import { cn } from "@/components/kit";

const KEY = "kalks.crm.hideMoney";
const listeners = new Set<() => void>();
let hidden: boolean | null = null;

function read() {
  if (hidden === null) {
    try {
      hidden = localStorage.getItem(KEY) === "1";
    } catch {
      hidden = false;
    }
  }
  return hidden;
}

function set(v: boolean) {
  hidden = v;
  try {
    localStorage.setItem(KEY, v ? "1" : "0");
  } catch {
    /* private window: kept for this visit only */
  }
  listeners.forEach((l) => l());
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => listeners.delete(l);
}

/** [hidden, toggle] — shared by every page */
export function useHideMoney(): [boolean, () => void] {
  const h = React.useSyncExternalStore(subscribe, read, () => false);
  return [h, React.useCallback(() => set(!read()), [])];
}

export const MASK = "••••••";

/** the eye button */
export function HideMoneyButton({ className }: { className?: string }) {
  const t = useT();
  const [h, toggle] = useHideMoney();
  return (
    <button
      type="button"
      onClick={toggle}
      aria-pressed={h}
      aria-label={h ? t("dashboard.home.showBalances") : t("dashboard.home.hideBalances")}
      title={h ? t("dashboard.home.showBalances") : t("dashboard.home.hideBalances")}
      className={cn("grid size-8 shrink-0 place-items-center rounded-full text-fg-3 transition-colors hover:bg-surface-3 hover:text-fg [&_svg]:size-[17px]", className)}
    >
      {h ? <EyeOff /> : <Eye />}
    </button>
  );
}

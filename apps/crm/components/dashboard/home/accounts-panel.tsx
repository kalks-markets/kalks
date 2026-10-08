"use client";

// "Your accounts" on the dashboard: each trading account as a glossy card (brand gradient for live, ink for demo) in a
// carousel with arrows and dots, then its details as label/value pairs (hideable), then the account's actions.

import * as React from "react";
import Link from "next/link";
import { AnimatePresence, motion } from "motion/react";
import { ChevronLeft, ChevronRight, Eye, EyeOff, Plus } from "lucide-react";
import { Button, CopyButton, LogoMark, Skeleton, cn, formatMoney } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { productOrder } from "@/lib/products";

export type CardAccount = {
  login: string;
  type: "live" | "demo";
  prop?: boolean;
  /** Group · mode, e.g. "Pro · Hedging". */
  title: string;
  name?: string | null;
  /** "$" or "USC " */
  currency: string;
  cent: boolean;
  balance: number;
  equity: number;
  freeMargin: number;
  marginLevel: number | null;
  leverage: number;
  server: string;
  positions?: number;
  /** CFD / Options account split: an Options account shows an OPTIONS badge (absent = CFD). */
  product?: "cfd" | "options";
};

function money(v: number, cur: string) {
  return cur === "$" ? formatMoney(v) : `${cur}${v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
}

function spacedLogin(login: string) {
  return login.replace(/(\d{4})(?=\d)/g, "$1 ");
}

/** The glossy ribbon drawn over the account card (white strokes, purely decorative). */
function Ribbon() {
  return (
    <svg viewBox="0 0 400 250" preserveAspectRatio="none" className="pointer-events-none absolute inset-0 size-full" aria-hidden>
      <defs>
        <linearGradient id="kr1" x1="0" x2="1" y1="0" y2="0">
          <stop offset="0" stopColor="#fff" stopOpacity="0" />
          <stop offset="0.45" stopColor="#fff" stopOpacity="0.55" />
          <stop offset="1" stopColor="#fff" stopOpacity="0.05" />
        </linearGradient>
        <linearGradient id="kr2" x1="0" x2="1" y1="1" y2="0">
          <stop offset="0" stopColor="#fff" stopOpacity="0.05" />
          <stop offset="0.6" stopColor="#fff" stopOpacity="0.35" />
          <stop offset="1" stopColor="#fff" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d="M-20 170 C 60 120, 110 210, 180 150 S 290 40, 330 120 S 400 190, 430 90" fill="none" stroke="url(#kr1)" strokeWidth="26" strokeLinecap="round" />
      <path d="M-30 200 C 50 160, 120 240, 200 180 S 300 80, 350 150 S 410 200, 440 130" fill="none" stroke="url(#kr2)" strokeWidth="12" strokeLinecap="round" />
      <path d="M150 -20 C 170 60, 120 90, 160 140 S 240 170, 230 260" fill="none" stroke="url(#kr2)" strokeWidth="40" strokeLinecap="round" opacity="0.5" />
    </svg>
  );
}

export function AccountVisual({ a, hidden }: { a: CardAccount; hidden?: boolean }) {
  const t = useT();
  const live = a.type === "live" && !a.prop;
  return (
    <div className={cn("k-acct-card relative aspect-[1.6/1] w-full overflow-hidden rounded-[24px] p-5 text-white sm:p-6", live ? "k-acct-live" : "k-acct-demo")}>
      <Ribbon />
      <div className="relative flex h-full flex-col">
        <div className="flex items-start justify-between gap-3">
          <LogoMark size={26} className="text-white" />
          <span className="flex items-center gap-1.5">
            {a.product === "options" && <span className="rounded-full bg-white/20 px-2.5 py-1 text-[10.5px] font-bold tracking-[0.08em] backdrop-blur-sm">{t("accounts.product.chipOptions")}</span>}
            <span className="rounded-full bg-white/20 px-2.5 py-1 text-[10.5px] font-bold tracking-[0.08em] backdrop-blur-sm">{a.prop ? t("accounts.badge.prop") : a.type === "live" ? t("accounts.badge.live") : t("accounts.badge.demo")}</span>
          </span>
        </div>
        <div dir="ltr" className="k-num mt-auto text-start font-mono text-[19px] font-semibold tracking-[0.14em] [text-shadow:0_1px_8px_rgba(0,0,0,0.18)] sm:text-[22px]">
          {spacedLogin(a.login)}
        </div>
        <div className="mt-3 flex items-end justify-between gap-3">
          <div className="min-w-0">
            <div className="truncate text-[10.5px] font-semibold uppercase tracking-[0.1em] text-white/75">{a.name || t("dashboard.home.tradingAccount")}</div>
            <div className="truncate text-[13px] font-semibold">{a.title}</div>
          </div>
          <div className="shrink-0 text-end">
            <div className="text-[10.5px] font-semibold uppercase tracking-[0.1em] text-white/75">{t("common.balance")}</div>
            <div dir="ltr" className="k-num text-[15px] font-bold">{hidden ? "••••••" : money(a.balance, a.currency)}</div>
          </div>
        </div>
      </div>
    </div>
  );
}

export function AccountsPanel({
  accounts,
  loading,
  failed,
  onRetry,
  actions,
  allHref = "/accounts",
  extraCount = 0,
}: {
  accounts: CardAccount[] | null;
  loading?: boolean;
  failed?: boolean;
  onRetry?: () => void;
  /** Actions for the selected account (trade, fund, menu). */
  actions?: (a: CardAccount) => React.ReactNode;
  allHref?: string;
  extraCount?: number;
}) {
  const t = useT();
  const [i, setI] = React.useState(0);
  const [dir, setDir] = React.useState(1);
  const [hidden, setHidden] = React.useState(false);
  // CFD accounts first, then Options accounts (CFD / Options account split)
  const list = React.useMemo(() => productOrder(accounts ?? []), [accounts]);
  const idx = Math.min(i, Math.max(0, list.length - 1));
  const a = list[idx];
  const go = (d: number) => {
    if (list.length < 2) return;
    setDir(d);
    setI((idx + d + list.length) % list.length);
  };
  const tx = React.useRef<number | null>(null);
  const rows: [string, React.ReactNode][] = a
    ? [
        [t("dashboard.home.accountName"), <span key="n" className="truncate">{a.name || a.title}</span>],
        [
          t("accounts.label.login"),
          <span key="l" dir="ltr" className="inline-flex items-center gap-1 font-mono">
            {hidden ? `•••• ${a.login.slice(-4)}` : a.login}
            {!hidden && <CopyButton value={a.login} label={t("accounts.label.login")} />}
          </span>,
        ],
        [t("common.equity"), <span key="e" dir="ltr" className="k-num">{hidden ? "••••" : money(a.equity, a.currency)}</span>],
        ...(a.product === "options" ? [] : ([[t("dashboard.home.leverage"), <span key="lv" dir="ltr" className="k-num">1:{a.leverage.toLocaleString("en-US")}</span>]] as [string, React.ReactNode][])),
        [t("accounts.label.freeMargin"), <span key="f" dir="ltr" className="k-num">{hidden ? "••••" : money(a.freeMargin, a.currency)}</span>],
        [t("accounts.label.marginLevel"), <span key="m" dir="ltr" className="k-num">{a.marginLevel === null || !Number.isFinite(a.marginLevel) ? "—" : `${Math.round(a.marginLevel).toLocaleString("en-US")}%`}</span>],
      ]
    : [];

  return (
    <section className="min-w-0">
      <div className="flex items-center justify-between gap-3">
        <h2 className="k-display text-[20px] font-semibold tracking-[-0.015em] sm:text-[22px]">{t("dashboard.home.yourAccounts")}</h2>
        <Link href="/accounts/new">
          <Button size="sm" variant="surface">
            {t("dashboard.accounts.open")} <Plus />
          </Button>
        </Link>
      </div>

      <div className="group/acct relative mt-5 px-1">
        {loading && !accounts ? (
          <Skeleton className="aspect-[1.6/1] w-full rounded-[24px]" />
        ) : failed && !accounts ? (
          <div className="k-card grid aspect-[1.6/1] place-items-center p-6 text-center text-[13px] text-fg-2">
            <div>
              <p>{t("dashboard.accounts.unavailable")}</p>
              {onRetry && (
                <Button size="sm" className="mt-3" onClick={onRetry}>
                  {t("common.retry")}
                </Button>
              )}
            </div>
          </div>
        ) : !a ? (
          <Link href="/accounts/new" className="k-acct-empty grid aspect-[1.6/1] w-full place-items-center rounded-[24px] p-6 text-center">
            <span>
              <span className="mx-auto grid size-12 place-items-center rounded-full bg-ember text-[var(--k-on-ember)] shadow-[0_10px_24px_-10px_var(--k-ember)]">
                <Plus className="size-5" />
              </span>
              <span className="mt-3 block text-[15px] font-bold text-fg">{t("dashboard.accounts.openLive.title")}</span>
              <span className="mt-1 block text-[12.5px] text-fg-3">{t("dashboard.accounts.openLive.text")}</span>
            </span>
          </Link>
        ) : (
          <>
            {/* the next card peeking out behind, like a stack */}
            {list.length > 1 && <div aria-hidden className={cn("k-acct-card absolute inset-x-6 -bottom-3 top-4 rotate-[3deg] rounded-[24px] opacity-70 rtl:-rotate-[3deg]", list[(idx + 1) % list.length]!.type === "live" ? "k-acct-live" : "k-acct-demo")} />}
            <div
              className="relative"
              onTouchStart={(e) => (tx.current = e.touches[0]!.clientX)}
              onTouchEnd={(e) => {
                if (tx.current === null) return;
                const dx = e.changedTouches[0]!.clientX - tx.current;
                tx.current = null;
                if (Math.abs(dx) > 40) go(dx < 0 ? 1 : -1);
              }}
            >
              <AnimatePresence mode="popLayout" initial={false} custom={dir}>
                <motion.div
                  key={a.login}
                  custom={dir}
                  initial={{ opacity: 0, x: dir * 40, scale: 0.98 }}
                  animate={{ opacity: 1, x: 0, scale: 1 }}
                  exit={{ opacity: 0, x: dir * -40, scale: 0.98 }}
                  transition={{ duration: 0.32, ease: [0.16, 1, 0.3, 1] }}
                >
                  <Link href={`/accounts/${a.login}`} aria-label={`${a.title} #${a.login}`} className="block rounded-[24px] outline-none focus-visible:ring-4 focus-visible:ring-ember/30">
                    <AccountVisual a={a} hidden={hidden} />
                  </Link>
                </motion.div>
              </AnimatePresence>
            </div>
            {list.length > 1 && (
              <>
                <button type="button" onClick={() => go(-1)} aria-label={t("dashboard.home.previous")} className="k-surface-btn absolute -start-4 top-1/2 z-10 grid size-10 -translate-y-1/2 place-items-center rounded-full text-fg opacity-0 transition-opacity focus-visible:opacity-100 group-hover/acct:opacity-100 max-lg:hidden">
                  <ChevronLeft className="size-[18px] rtl:-scale-x-100" />
                </button>
                <button type="button" onClick={() => go(1)} aria-label={t("dashboard.home.next")} className="k-surface-btn absolute -end-5 top-1/2 z-10 grid size-10 -translate-y-1/2 place-items-center rounded-full text-fg">
                  <ChevronRight className="size-[18px] rtl:-scale-x-100" />
                </button>
              </>
            )}
          </>
        )}
      </div>
      {list.length > 1 && (
        <div className="mt-6 flex items-center justify-center gap-1.5">
          {list.map((x, k) => (
            <button
              key={x.login}
              type="button"
              aria-label={`#${x.login}`}
              aria-current={k === idx}
              onClick={() => {
                setDir(k > idx ? 1 : -1);
                setI(k);
              }}
              className="grid h-6 place-items-center px-0.5"
            >
              <span className={cn("block h-1.5 rounded-full transition-all", k === idx ? "w-6 bg-fg" : "w-1.5 bg-fg-3/45")} />
            </button>
          ))}
          {extraCount > 0 && (
            <Link href={allHref} className="ms-2 text-[12px] font-semibold text-fg-3 hover:text-ember">
              {t("dashboard.accounts.more", { count: extraCount })}
            </Link>
          )}
        </div>
      )}

      {a && (
        <>
          <div className="mt-7 flex items-center justify-between gap-3">
            <h3 className="k-display text-[17px] font-semibold tracking-[-0.01em]">{t("dashboard.home.accountInfo")}</h3>
            <button type="button" onClick={() => setHidden((h) => !h)} aria-pressed={hidden} aria-label={hidden ? t("dashboard.home.showBalances") : t("dashboard.home.hideBalances")} className="grid size-10 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg">
              {hidden ? <Eye className="size-[18px]" /> : <EyeOff className="size-[18px]" />}
            </button>
          </div>
          <div className="k-card mt-3 grid grid-cols-2 gap-x-5 gap-y-4 p-5">
            {rows.map(([k, v]) => (
              <div key={k} className="min-w-0">
                <div className="truncate text-[12px] text-fg-3">{k}</div>
                <div className="mt-1 flex min-w-0 items-center truncate text-[14.5px] font-semibold text-fg">{v}</div>
              </div>
            ))}
          </div>
          {actions && <div className="mt-3 flex flex-wrap items-center gap-2">{actions(a)}</div>}
        </>
      )}
    </section>
  );
}

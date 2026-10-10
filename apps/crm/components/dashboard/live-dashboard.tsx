"use client";

import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import {
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUpFromLine,
  ArrowUpRight,
  Award,
  CandlestickChart,
  ChartSpline,
  Coins,
  Copy,
  FileText,
  Gift,
  Globe,
  GraduationCap,
  History,
  IdCard,
  Layers,
  LifeBuoy,
  Mail,
  Trophy,
  UserRound,
  Wallet,
} from "lucide-react";
import { Button, Card, CardHeader, Chip, CoinIcon, MarketSessions, Money, Reveal, cn, formatMoney, useQuotes } from "@/components/kit";
import { INSTRUMENTS, isMarketOpen } from "@kalks/mock";
import { KYC_CHIP, useReadOnly, useSession, type SessionUser } from "@/components/session";
import { FeedGuard } from "@/components/feed-guard";
import { SUPPORT_EMAIL, TERMINAL_URL } from "@/lib/live";
import { curOf, fmtLevel, isArchived, levelTone, serverOf, useAccounts, type EngineAccount } from "@/components/trading/api";
import { liveTotals } from "@/components/trading/accounts-page";
import { AccountActions, FundButton, RefillButton, TradeButton, isPropAccount } from "@/components/trading/ui";
import { useWalletFunded, walletStep } from "@/components/wallet-live/onboarding";
import { fmt, useWallet, type ActivityItem, type Page, type WalletConfig } from "@/components/wallet-live/api";
import { KIND_LABEL } from "@/components/wallet-live/ui";
import { useGrowth, type Rewards } from "@/components/growth/api";
import { BannerSlot } from "@/components/growth/banner-slot";
import { LiveCalendarCard, LiveNewsCard, LiveWorldCard } from "@/components/news-live/dashboard";
import { Trans, useFormat, useT } from "@kalks/i18n/react";
import { AccountsPanel, type CardAccount } from "@/components/dashboard/home/accounts-panel";
import { Shortcuts, type Shortcut } from "@/components/dashboard/home/shortcuts";
import { BalanceStrip } from "@/components/dashboard/home/balance-strip";
import { ActivityTabs, ChecklistCard, type ListRowItem } from "@/components/dashboard/home/list-cards";
import { NotificationsPanel, type Prompt } from "@/components/dashboard/home/notifications-panel";
import { OverviewLayout, SectionTitle } from "@/components/dashboard/home/overview";
import { RANGE_DAYS, StatisticCard, type StatMode, type StatRange } from "@/components/dashboard/home/statistic-card";
import type { TrendPoint } from "@/components/dashboard/home/trend-chart";
import { AiFacts, AiLink, AskAi, type AiChip } from "@/components/ai/ask-ai";
import { ArcGauge, HeroButton, HeroGlass, MiniBars, MiniLine, PageHero, StatTile } from "@/components/page-hero";
import { useModules } from "@/components/tenant-config";

/** the last 7 calendar days ending today, with no P&L yet (accounts without history) */
function lastWeekDays() {
  return Array.from({ length: 7 }, (_, i) => {
    const d = new Date();
    d.setDate(d.getDate() - 6 + i);
    return { day: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`, v: 0 };
  });
}

export function clientId(id: number) {
  return `KL-${String(id).padStart(6, "0")}`;
}

type T = ReturnType<typeof useT>;
type F = ReturnType<typeof useFormat>;

function fmtDate(iso: string, f: F) {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "—" : f.date(d);
}

/* ------------------------------------------------------------------ */
/* Getting started: every step is driven by the real client record     */
/* ------------------------------------------------------------------ */

type StepState = "done" | "todo" | "review" | "rejected" | "soon";
type Step = { key: string; icon: React.ReactNode; title: string; text: string; state: StepState; href?: string };

/** The "Verify your identity" step from the real KYC status (users.kyc_status + the latest case). */
function kycStep(me: SessionUser, t: T): { state: StepState; text: string } {
  if (me.kyc_status === "verified") return { state: "done", text: t("dashboard.steps.kyc.verified") };
  switch (me.kyc_case_status) {
    case "more_info":
      return { state: "todo", text: t("dashboard.steps.kyc.moreInfo") };
    case "submitted":
    case "in_review":
      return { state: "review", text: t("dashboard.steps.kyc.review") };
    case "draft":
      return { state: "todo", text: t("dashboard.steps.kyc.draft") };
    case "rejected":
      return { state: "rejected", text: t("dashboard.steps.kyc.rejected") };
  }
  if (me.kyc_status === "pending") return { state: "review", text: t("dashboard.steps.kyc.review") };
  if (me.kyc_status === "rejected") return { state: "rejected", text: t("dashboard.steps.kyc.rejected") };
  return { state: "todo", text: t("dashboard.steps.kyc.todo") };
}

function steps(me: SessionUser, accounts: EngineAccount[] | null, t: T, f: F): Step[] {
  const live = accounts?.filter((a) => a.type === "live").length ?? 0;
  const demo = accounts?.filter((a) => a.type === "demo").length ?? 0;
  const opened = live + demo > 0;
  return [
    { key: "account", icon: <UserRound />, title: t("dashboard.steps.account.title"), text: t("dashboard.steps.account.text", { date: fmtDate(me.created_at, f) }), state: "done" },
    {
      key: "email",
      icon: <Mail />,
      title: t("dashboard.steps.email.title"),
      text: me.email_verified ? t("dashboard.steps.email.verified", { email: me.email }) : t("dashboard.steps.email.confirm", { email: me.email }),
      state: me.email_verified ? "done" : "todo",
    },
    { key: "kyc", icon: <IdCard />, title: t("dashboard.steps.kyc.title"), ...kycStep(me, t), href: "/profile/verification" },
    {
      key: "account-open",
      icon: <Layers />,
      title: t("dashboard.steps.accountOpen.title"),
      text: opened ? t("dashboard.steps.accountOpen.opened", { live, demo, count: live + demo }) : t("dashboard.steps.accountOpen.todo"),
      state: opened ? "done" : "todo",
      href: opened ? "/accounts" : "/accounts/new",
    },
    { key: "wallet", icon: <Wallet />, title: t("dashboard.steps.wallet.title"), text: t("dashboard.steps.wallet.text"), state: "soon", href: "/wallet" },
  ];
}

const STATE_CHIP: Record<StepState, { tone: "up" | "warn" | "down" | "neutral" | "ember"; label: string }> = {
  done: { tone: "up", label: "Done" },
  todo: { tone: "ember", label: "To do" },
  review: { tone: "warn", label: "In review" },
  rejected: { tone: "down", label: "Rejected" },
  soon: { tone: "neutral", label: "Not started" },
};

const STEP_TONE: Record<string, ListRowItem["tone"]> = { account: "accent", email: "sky", kyc: "amber", "account-open": "lavender", wallet: "mint" };

/* ------------------------------------------------------------------ */
/* Statistics: equity curve from the reports service                   */
/* ------------------------------------------------------------------ */

type Curve = { day: string; balance: number; equity: number; flow: number }[];

function isoDay(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const curveCache = new Map<number, Promise<Curve | null>>();

/** Daily equity / balance / net deposits of the live accounts over the last `days` days (cached per tab). */
function loadCurve(days: number): Promise<Curve | null> {
  let p = curveCache.get(days);
  if (!p) {
    const to = new Date();
    to.setDate(to.getDate() + 1);
    const from = new Date();
    from.setDate(from.getDate() - days + 1);
    p = fetch(`/api/reports/analytics?login=all&from=${isoDay(from)}&to=${isoDay(to)}`, { cache: "no-store" })
      .then(async (r) => (r.ok ? (((await r.json()) as { curve?: { points?: Curve } }).curve?.points ?? []) : null))
      .catch(() => null);
    curveCache.set(days, p);
    // a failed or stale answer is fetched again next time
    void p.then((v) => setTimeout(() => curveCache.delete(days), v ? 60_000 : 0));
  }
  return p;
}

function useCurve(days: number) {
  const [v, setV] = React.useState<{ days: number; curve: Curve | null } | null>(null);
  React.useEffect(() => {
    let alive = true;
    void loadCurve(days).then((curve) => alive && setV({ days, curve }));
    return () => {
      alive = false;
    };
  }, [days]);
  return v && v.days === days ? { curve: v.curve, loading: false } : { curve: null, loading: true };
}

/** This period and the one before it; P&L is cumulative from the start of each period, net of deposits / withdrawals. */
function toSeries(curve: Curve, n: number, mode: StatMode, range: StatRange) {
  const cut = new Date();
  cut.setDate(cut.getDate() - n + 1);
  const from = isoDay(cut);
  const cur = curve.filter((p) => p.day >= from);
  const prev = curve.filter((p) => p.day < from).slice(-n);
  const val = (xs: Curve) => {
    if (mode === "equity") return xs.map((p) => p.equity);
    let acc = 0;
    return xs.map((p, i) => (i === 0 ? 0 : ((acc += p.equity - xs[i - 1]!.equity - p.flow), acc)));
  };
  const step = range === "year" ? 7 : 1;
  const keep = (_: unknown, i: number, xs: unknown[]) => i % step === 0 || i === xs.length - 1;
  const cv = val(cur);
  const points: TrendPoint[] = cur.map((p, i) => ({ t: new Date(`${p.day}T12:00:00`).getTime(), v: cv[i]! })).filter(keep);
  const compare = prev.length > 1 ? val(prev).filter(keep) : null;
  return { points, compare };
}

/* ------------------------------------------------------------------ */
/* Cards below the overview                                            */
/* ------------------------------------------------------------------ */

function AccountCard() {
  const me = useSession();
  const t = useT();
  const f = useFormat();
  const kyc = KYC_CHIP[me.kyc_status];
  const rows: [string, React.ReactNode][] = [
    [t("dashboard.account.clientId"), <span key="id" className="font-mono" dir="ltr">{clientId(me.id)}</span>],
    [t("common.email"), <span key="e" className="truncate">{me.email}</span>],
    [t("dashboard.account.emailStatus"), me.email_verified ? <Chip key="ev" size="sm" tone="up">{t("common.verified")}</Chip> : <Chip key="ev" size="sm" tone="warn">{t("dashboard.account.notVerified")}</Chip>],
    [t("dashboard.account.identity"), <Chip key="k" size="sm" tone={kyc.tone} dot>{t.dyn(`shell.kyc.${me.kyc_status}`, kyc.label)}</Chip>],
    [t("dashboard.account.memberSince"), <span key="m" className="k-num">{fmtDate(me.created_at, f)}</span>],
  ];
  return (
    <Card className="h-full">
      <CardHeader
        title={t("dashboard.account.title")}
        icon={<UserRound />}
        action={
          <Link href="/profile">
            <Button size="sm" variant="surface">
              {t("dashboard.account.profile")}
            </Button>
          </Link>
        }
      />
      <div className="mt-2 divide-y divide-line px-5 pb-3 sm:px-6">
        {rows.map(([k, v]) => (
          <div key={k} className="flex items-center justify-between gap-4 py-3 text-[13px]">
            <span className="shrink-0 text-fg-3">{k}</span>
            <span className="min-w-0 truncate text-end font-semibold text-fg">{v}</span>
          </div>
        ))}
      </div>
    </Card>
  );
}

function TraderBanner() {
  const t = useT();
  return (
    <Card className="k-card-hot relative h-full overflow-hidden">
      <div className="relative flex h-full flex-col justify-between gap-5 p-6 sm:p-7">
        <div className="max-w-xl">
          <Chip tone="ember" className="mb-3">
            <CandlestickChart className="size-3.5" /> {t("dashboard.trader.chip")}
          </Chip>
          <h3 className="k-display text-[22px] font-bold tracking-[-0.02em] sm:text-2xl">Kalks Trader</h3>
          <p className="mt-2 text-sm text-fg-2">{t("dashboard.trader.text", { count: INSTRUMENTS.length })}</p>
        </div>
        <a href={TERMINAL_URL} target="_blank" rel="noopener" className="shrink-0 self-start">
          <Button variant="ink" size="lg">
            {t("dashboard.launchTrader")} <ArrowUpRight className="rtl:-scale-x-100" />
          </Button>
        </a>
      </div>
    </Card>
  );
}

function SessionsCard() {
  const t = useT();
  const open = INSTRUMENTS.filter((i) => isMarketOpen(i.symbol)).length;
  return (
    <Card className="h-full">
      <CardHeader title={t("dashboard.sessions.title")} action={<Chip size="sm">{t("dashboard.sessions.open", { open, total: INSTRUMENTS.length })}</Chip>} />
      <div className="px-5 pb-6 pt-4 sm:px-6">
        <MarketSessions />
      </div>
    </Card>
  );
}

function HeatmapCard() {
  const t = useT();
  const qs = useQuotes(INSTRUMENTS.map((i) => i.symbol));
  const sorted = [...INSTRUMENTS].sort((a, b) => (qs[b.symbol]?.change ?? 0) - (qs[a.symbol]?.change ?? 0));
  const up = INSTRUMENTS.filter((i) => (qs[i.symbol]?.change ?? 0) >= 0).length;
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("dashboard.heatmap.title")}
        subtitle={t("dashboard.heatmap.subtitle")}
        action={
          <>
            <Chip tone="up" className="hidden sm:inline-flex">
              {t("dashboard.heatmap.up", { count: up })}
            </Chip>
            <Chip tone="down" className="hidden sm:inline-flex">
              {t("dashboard.heatmap.down", { count: INSTRUMENTS.length - up })}
            </Chip>
            <Link href="/markets">
              <Button size="sm" variant="surface">
                {t("dashboard.heatmap.allMarkets")}
              </Button>
            </Link>
          </>
        }
      />
      <div className="grid grid-cols-3 content-start gap-2 px-4 pb-5 pt-4 sm:grid-cols-5 sm:px-6 md:grid-cols-6 lg:grid-cols-9 xl:grid-cols-5 2xl:grid-cols-6">
        {sorted.map((i) => {
          const ch = qs[i.symbol]?.change ?? 0;
          const a = Math.min(1, Math.abs(ch) / 3);
          const open = isMarketOpen(i.symbol);
          return (
            <Link
              key={i.symbol}
              href="/markets"
              title={open ? t("dashboard.heatmap.tipOpen", { symbol: i.symbol }) : t("dashboard.heatmap.tipClosed", { symbol: i.symbol })}
              className="rounded-[14px] px-3 py-2.5 transition-transform hover:-translate-y-0.5"
              style={{ background: `color-mix(in oklab, ${ch >= 0 ? "var(--k-up)" : "var(--k-down)"} ${Math.round(8 + a * 48)}%, var(--k-surface-2))` }}
            >
              <div className="flex items-center gap-1.5">
                <span className="truncate text-[12.5px] font-bold text-fg">{i.symbol}</span>
                <span className={cn("size-1.5 shrink-0 rounded-full", open ? "bg-up" : "border border-fg-3")} />
              </div>
              <div className={cn("k-num mt-0.5 text-[12px] font-semibold", a > 0.55 ? "text-fg" : ch >= 0 ? "text-up" : "text-down")}>
                <span dir="ltr">
                  {ch >= 0 ? "+" : ""}
                  {ch.toFixed(2)}%
                </span>
              </div>
            </Link>
          );
        })}
      </div>
    </Card>
  );
}

function SupportCard() {
  const t = useT();
  const copy = () => {
    navigator.clipboard?.writeText(SUPPORT_EMAIL).then(
      () => toast.success(t("dashboard.support.copied")),
      () => toast.error(t("dashboard.support.copyFailed")),
    );
  };
  return (
    <Card className="flex h-full flex-col gap-4 px-5 py-5 sm:px-6 md:flex-row md:items-center">
      <span className="k-tile k-tile-lavender size-12 shrink-0 rounded-[15px] [&_svg]:size-5">
        <LifeBuoy />
      </span>
      <div className="min-w-0 flex-1">
        <div className="k-display text-[16px] font-bold">{t("dashboard.support.title")}</div>
        <div className="mt-0.5 text-[13px] text-fg-2">
          <Trans k="dashboard.support.text" vars={{ email: SUPPORT_EMAIL }} tags={{ mail: (c) => <span className="font-mono text-fg" dir="ltr">{c}</span> }} />
        </div>
      </div>
      <div className="flex gap-2">
        <Button size="sm" variant="surface" onClick={copy}>
          <Copy /> {t("common.copy")}
        </Button>
        <a href={`mailto:${SUPPORT_EMAIL}`}>
          <Button size="sm" variant="ember">
            <Mail /> {t("dashboard.support.emailSupport")}
          </Button>
        </a>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function toCard(a: EngineAccount, t: T): CardAccount {
  return {
    login: String(a.login),
    type: a.type,
    prop: isPropAccount(a),
    title: `${a.groupName} · ${t.dyn(`accounts.mode.${a.mode}`, a.mode)}`,
    name: a.name || null,
    currency: curOf(a),
    cent: a.cent,
    balance: a.balance,
    equity: a.equity,
    freeMargin: a.freeMargin,
    marginLevel: a.marginLevel,
    leverage: a.leverage,
    server: serverOf(a),
    positions: a.positions,
    product: a.product === "options" ? "options" : "cfd",
  };
}

const ACTIVITY_ICON: Record<string, { icon: React.ReactNode; tone: ListRowItem["tone"] }> = {
  deposit: { icon: <ArrowDownToLine />, tone: "mint" },
  withdrawal: { icon: <ArrowUpFromLine />, tone: "coral" },
  transfer: { icon: <ArrowLeftRight className="rtl:-scale-x-100" />, tone: "lavender" },
  other: { icon: <Coins />, tone: "amber" },
};

/** Live builds: only data that is real for this client — their record, accounts, wallet, live prices, real links. */
export function LiveDashboard({ movers }: { movers: React.ReactNode }) {
  const me = useSession();
  const t = useT();
  const f = useFormat();
  const readOnly = useReadOnly();
  // modules the broker switched off (gateway): their cards, KPIs, links and quick actions are left out
  const on = useModules();
  const acc = useAccounts(10000);
  // Archived / closed accounts live on the Accounts page's Archived tab only.
  const accounts = React.useMemo(() => acc.data?.accounts.filter((a) => !isArchived(a)) ?? null, [acc.data]);
  const totals = liveTotals(accounts ?? []);
  const wallet = useWalletFunded();
  const usdt = wallet?.balances.find((b) => b.currency === "USDT");
  const walletTotal = usdt ? Number(usdt.available) + Number(usdt.locked) : wallet ? 0 : null;
  const rewards = useGrowth<Rewards>("rewards");

  // Statistics card
  const [mode, setMode] = React.useState<StatMode>("equity");
  const [range, setRange] = React.useState<StatRange>("month");
  const n = RANGE_DAYS[range];
  const week = useCurve(14);

  // today's P&L: today's equity move net of deposits / withdrawals (reports), else the floating P&L of the live accounts
  const wk = week.curve;
  const today = wk && wk.length > 1 ? wk[wk.length - 1]!.equity - wk[wk.length - 2]!.equity - wk[wk.length - 1]!.flow : null;
  const todayBase = wk && wk.length > 1 ? wk[wk.length - 2]!.equity : 0;
  const todayPct = today !== null && todayBase > 0 ? (today / todayBase) * 100 : null;
  const hasLive = totals.live.length > 0;

  // carousel: live first, then demo, then prop
  const ordered = React.useMemo(() => [...totals.live, ...totals.demo, ...(accounts ?? []).filter((a) => isPropAccount(a))], [accounts, totals.live, totals.demo]);
  const cards = React.useMemo(() => (accounts ? ordered.slice(0, 8).map((a) => toCard(a, t)) : null), [accounts, ordered, t]);

  const list = steps(me, accounts, t, f)
    .filter((s) => s.key !== "wallet" || on("wallet"))
    .map((s) => (s.key === "wallet" ? walletStep(wallet, t) : s));
  const done = list.filter((s) => s.state === "done").length;
  const checklist: ListRowItem[] = list.map((s) => {
    const chip = STATE_CHIP[s.state];
    return {
      key: s.key,
      icon: s.icon,
      tone: STEP_TONE[s.key] ?? "accent",
      title: s.title,
      sub: s.text,
      done: s.state === "done",
      href: s.href,
      status: s.state === "todo" && s.href ? undefined : { label: t.dyn(`dashboard.steps.state.${s.state}`, chip.label), tone: chip.tone },
      action: s.state === "todo" && s.href && !readOnly ? { label: t("common.continue"), href: s.href } : undefined,
    };
  });

  const kyc = kycStep(me, t);
  const prompts: Prompt[] = [];
  if (!readOnly && kyc.state !== "done")
    prompts.push({ id: `kyc-${kyc.state}`, title: t("dashboard.steps.kyc.title"), text: kyc.text, icon: <IdCard />, tone: kyc.state === "rejected" ? "coral" : "amber", action: { label: kyc.state === "review" ? t("common.details") : t("dashboard.home.verifyNow"), href: "/profile/verification" } });
  if (!readOnly && on("wallet") && wallet && walletTotal === 0 && !wallet.pending_deposits.length)
    prompts.push({ id: "fund", title: t("dashboard.home.fundTitle"), text: t("dashboard.home.fundText"), icon: <Wallet />, tone: "mint", action: { label: t("dashboard.home.depositNow"), href: "/wallet/deposit" } });

  const r = rewards.data;
  const linkedRows: ListRowItem[] = [
    { key: "trader", icon: <CandlestickChart />, tone: "accent", title: "Kalks Trader", sub: t("dashboard.trader.chip"), action: { label: t("common.open"), href: TERMINAL_URL, external: true } },
    ...(on("copy_trading") ? [{ key: "copy", icon: <Copy />, tone: "pink" as const, title: t("shell.nav.copyTrading"), sub: t("shell.nav.social"), action: { label: t("common.open"), href: "/social" } }] : []),
    ...(on("ib") ? [{ key: "ib", icon: <Award />, tone: "amber" as const, title: t("shell.nav.partner"), sub: t("dashboard.partner.chip"), action: { label: t("common.open"), href: "/partner" } }] : []),
    ...(on("rewards")
      ? [
          {
            key: "loyalty",
            icon: <Gift />,
            tone: "lavender" as const,
            title: t("shell.nav.loyalty"),
            sub: r ? t("dashboard.home.points", { points: r.points.balance.toLocaleString("en-US") }) : t("shell.nav.rewards"),
            ...(r ? { status: { label: r.tier.name, tone: "ember" as const } } : { action: { label: t("common.open"), href: "/rewards/loyalty" } }),
          },
        ]
      : []),
  ];

  // Ask Kalks AI: suggestions answered by the real support bot; account questions also show the client's own figures
  const liveAccts = totals.live;
  const money2 = (a: EngineAccount, v: number) => `${curOf(a)}${v.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
  const aiChips: AiChip[] = [
    ...(on("wallet") ? [{ key: "deposit", label: t("dashboard.ai.chip.deposit"), extra: <AiLink href="/wallet/deposit">{t("common.deposit")}</AiLink> }] : []),
    {
      key: "freeMargin",
      label: t("dashboard.ai.chip.freeMargin"),
      extra: <AiFacts title={t("dashboard.ai.yourAccounts")} rows={liveAccts.map((a) => ({ label: `#${a.login} · ${a.groupName}`, value: money2(a, a.freeMargin) }))} />,
    },
    {
      key: "marginLevel",
      label: t("dashboard.ai.chip.marginLevel"),
      extra: <AiFacts title={t("dashboard.ai.yourAccounts")} rows={liveAccts.map((a) => ({ label: `#${a.login} · ${a.groupName}`, value: fmtLevel(a.marginLevel), tone: levelTone(a.marginLevel) }))} />,
    },
    { key: "openAccount", label: t("dashboard.ai.chip.openAccount"), question: t("dashboard.ai.q.openAccount"), extra: <AiLink href="/accounts/new">{t("dashboard.accounts.open")}</AiLink> },
  ];

  // the hero: daily P&L of the last 7 days (equity move net of deposits / withdrawals), the equity line, total balance
  const last8 = wk && wk.length > 1 ? wk.slice(-8) : null;
  const daily = last8 ? last8.slice(1).map((p, i) => ({ day: p.day, v: p.equity - last8[i]!.equity - p.flow })) : null;
  const weekday = (day: string) => new Date(`${day}T12:00:00`).toLocaleDateString(t.locale, { weekday: "narrow" });
  const equityLine = wk ? wk.map((p) => p.equity) : [];
  const total = accounts || walletTotal !== null ? totals.equity + (walletTotal ?? 0) : null;
  const tierProgress = r ? (r.nextTier ? Math.max(0, Math.min(1, (r.points.lifetime - r.tier.minPoints) / Math.max(1, r.nextTier.minPoints - r.tier.minPoints))) : 1) : 0;
  const pnlNow = today ?? totals.profit;
  const lead = hasLive
    ? t.dyn(
        totals.live.length === 1 ? "dashboard.hero.leadLiveOne" : "dashboard.hero.leadLive",
        totals.live.length === 1 ? "{positions} open positions on your live account. Every number below is live." : "{positions} open positions across {live} live accounts. Every number below is live.",
        { positions: totals.positions, live: totals.live.length },
      ).replace(/^1 open positions/, "1 open position")
    : t.dyn("dashboard.hero.leadNew", "Open a live account, fund it in USDT and trade forex, gold, oil and crypto from one place.");

  const shortcuts: Shortcut[] = [
    ...(on("wallet") && !readOnly
      ? [
          { key: "deposit", label: t("common.deposit"), href: "/wallet/deposit", icon: <ArrowDownToLine /> },
          { key: "withdraw", label: t("common.withdraw"), href: "/wallet/withdraw", icon: <ArrowUpFromLine /> },
          { key: "transfer", label: t("common.transfer"), href: "/wallet/transfer", icon: <ArrowLeftRight className="rtl:-scale-x-100" /> },
        ]
      : []),
    ...(!readOnly ? [{ key: "open", label: t("dashboard.accounts.open"), href: "/accounts/new", icon: <Layers /> }] : []),
    { key: "trader", label: "Kalks Trader", href: TERMINAL_URL, icon: <CandlestickChart />, external: true },
    ...(on("markets") ? [{ key: "markets", label: t.dyn("shell.nav.markets", "Markets"), href: "/markets", icon: <Globe /> }] : []),
    ...(on("options") ? [{ key: "options", label: t.dyn("shell.nav.options", "Options"), href: "/options", icon: <ChartSpline /> }] : []),
    ...(on("copy_trading") || on("pamm") ? [{ key: "copy", label: t.dyn("shell.nav.copyPamm", "Copy & PAMM"), href: "/social", icon: <Copy /> }] : []),
    ...(on("prop") ? [{ key: "prop", label: t.dyn("shell.nav.prop", "Prop challenges"), href: "/prop", icon: <Trophy /> }] : []),
    ...(on("ib") ? [{ key: "ib", label: t("shell.nav.partner"), href: "/partner", icon: <Award /> }] : []),
    ...(on("rewards") ? [{ key: "rewards", label: t("shell.nav.rewards"), href: "/rewards", icon: <Gift /> }] : []),
    ...(on("academy") ? [{ key: "academy", label: t.dyn("shell.nav.academy", "Academy"), href: "/academy", icon: <GraduationCap /> }] : []),
    ...(on("wallet") ? [{ key: "history", label: t("dashboard.home.history"), href: "/wallet/history", icon: <History /> }] : []),
    { key: "statements", label: t.dyn("portfolio.st.title", "Statements"), href: "/portfolio/statements", icon: <FileText /> },
    { key: "kyc", label: t.dyn("dashboard.account.identity", "Verification"), href: "/profile/verification", icon: <IdCard /> },
    { key: "support", label: t("shell.nav.support"), href: "/support", icon: <LifeBuoy /> },
  ];

  return (
    <div className="pb-16">
      {/* the hero: only the AI bar on the photo (founder 2026-10-10: no greeting, the name is already in the top bar) */}
      <PageHero
        page="dashboard"
        center
        body={!readOnly && on("ai") ? <AskAi hero chips={aiChips} chat={on("support_chat")} /> : undefined}
      />

      {/* the money summary, moved down from the hero */}
      <BalanceStrip
        cells={[
          {
            key: "total", secret: true,
            big: true,
            label: t("dashboard.home.totalBalance"),
            value: total === null ? "—" : <Money value={total} countUp={false} />,
            sub: t.dyn("dashboard.hero.totalSub", "Live accounts + wallet"),
            href: "/wallet",
          },
          {
            key: "equity", secret: true,
            label: t("dashboard.equity.title"),
            value: accounts ? <Money value={totals.equity} countUp={false} /> : "—",
            sub: hasLive ? t("dashboard.home.accountsChip", { live: totals.live.length, positions: totals.positions }) : t("dashboard.accounts.openLive.title"),
            href: "/accounts",
          },
          ...(on("wallet")
            ? [
                {
                  key: "wallet", secret: true,
                  label: t("dashboard.home.walletBalance"),
                  value: walletTotal !== null ? <Money value={walletTotal} countUp={false} /> : "—",
                  unit: "USDT",
                  sub: "TRC20 · BEP20",
                  href: "/wallet",
                },
              ]
            : []),
          {
            key: "pnl", secret: true,
            label: today !== null ? t("dashboard.home.todayPnl") : t("dashboard.home.floating"),
            value: accounts ? <Money value={pnlNow} signed tone="auto" countUp={false} /> : "—",
            extra: <MiniBars values={(daily ?? lastWeekDays()).map((d) => d.v)} labels={(daily ?? lastWeekDays()).map((d) => weekday(d.day))} height={22} />,
            href: "/portfolio/analytics",
          },
          {
            key: "positions",
            label: t.dyn("dashboard.hero.openPositions", "Open positions"),
            value: accounts ? String(totals.positions) : "—",
            sub: hasLive
              ? totals.live.length === 1
                ? t.dyn("dashboard.hero.onLiveOne", "on your live account")
                : t.dyn("dashboard.hero.acrossLive", "across {count} live accounts", { count: totals.live.length })
              : undefined,
            href: "/portfolio",
          },
          ...(on("rewards")
            ? [
                {
                  key: "rewards",
                  label: t("dashboard.home.rewards"),
                  value: r ? r.points.balance.toLocaleString("en-US") : "—",
                  unit: t.dyn("dashboard.hero.points", "pts"),
                  sub: r ? (r.nextTier ? `${r.tier.name} · ${t.dyn("dashboard.hero.toNext", "{points} pts to {tier}", { points: r.nextTier.pointsToGo.toLocaleString("en-US"), tier: r.nextTier.name })}` : r.tier.name) : undefined,
                  href: "/rewards/loyalty",
                },
              ]
            : []),
        ]}
      />

      <BannerSlot placement="dashboard" />

      {/* Home, kept short (founder 2026-10-10): Ask Kalks AI with the shortcuts under it, your accounts beside them */}
      <div className="mt-8 grid grid-cols-1 items-start gap-6 xl:grid-cols-12">
        <div className="flex min-w-0 flex-col gap-6 xl:col-span-7">
          <section>
            <h2 className="k-display mb-3 text-[18px] font-semibold tracking-[-0.015em]">{t.dyn("dashboard.home.shortcuts", "Shortcuts")}</h2>
            <Shortcuts items={shortcuts} />
          </section>
        </div>
        <div className="min-w-0 xl:col-span-5">
          <AccountsPanel
            accounts={cards}
            loading={!acc.data && !acc.error}
            failed={!!acc.error}
            onRetry={acc.reload}
            extraCount={Math.max(0, ordered.length - 8)}
            actions={
              readOnly
                ? undefined
                : (c) => {
                    const a = accounts?.find((x) => String(x.login) === c.login);
                    if (!a) return null;
                    return (
                      <>
                        <TradeButton a={a} size="md" className="flex-1" />
                        {a.type === "live" ? !isPropAccount(a) && <FundButton a={a} size="md" /> : <RefillButton a={a} onDone={acc.reload} size="md" />}
                        <AccountActions a={a} onChanged={acc.reload} />
                      </>
                    );
                  }
            }
          />
        </div>
      </div>
    </div>
  );
}

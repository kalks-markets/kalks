"use client";

import * as React from "react";
import Link from "next/link";
import {
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUpFromLine,
  ArrowUpRight,
  Award,
  BadgeCheck,
  CandlestickChart,
  Coins,
  Copy,
  Gift,
  IdCard,
  LifeBuoy,
  LineChart,
  Mail,
  Phone,
  Repeat,
  TrendingUp,
  Users,
  Wallet,
} from "lucide-react";
import {
  BarcodeBars,
  Button,
  Card,
  CardHeader,
  Chip,
  CoinIcon,
  Gauge,
  Icon3D,
  KpiCard,
  ListRow,
  MarketSessions,
  Money,
  PageHeader,
  PriceText,
  Reveal,
  SymbolAvatar,
  WorldMap,
  cn,
  formatMoney,
  useQuotes,
} from "@/components/kit";
import { ACCOUNTS, CALENDAR, DASHBOARD, ME, NEWS, ONBOARDING, POSITIONS, WALLET, WALLET_TXS, equitySeries, freeMargin, marginLevel, positionProfit, type TradingAccount, type WalletTx } from "@kalks/mock";
import { COPY_SUBSCRIPTIONS } from "@kalks/mock/portfolio-extra";
import { LOYALTY } from "@kalks/mock/rewards";
import { AccountMenu, accountTitle } from "@/components/account-row";
import { MoversCard } from "@/components/dashboard/movers";
import { useSession } from "@/components/session";
import { TERMINAL_URL } from "@/lib/live";
import { Trans, useFormat, useT } from "@kalks/i18n/react";
import { AccountsPanel, type CardAccount } from "@/components/dashboard/home/accounts-panel";
import { BalancePanel, QuickActions } from "@/components/dashboard/home/balance-panel";
import { ActivityTabs, ChecklistCard, type ListRowItem } from "@/components/dashboard/home/list-cards";
import { NotificationsPanel, type Prompt } from "@/components/dashboard/home/notifications-panel";
import { OverviewLayout, SectionTitle } from "@/components/dashboard/home/overview";
import { RANGE_DAYS, StatisticCard, type StatMode, type StatRange } from "@/components/dashboard/home/statistic-card";
import type { TrendPoint } from "@/components/dashboard/home/trend-chart";
import { AiFacts, AiLink, AskAi, type AiChip } from "@/components/ai/ask-ai";
import { botAnswer } from "@kalks/mock/support-extra";

/** Demo answers for Ask Kalks AI (sample data; live builds ask the real support bot). */
function demoAnswer(q: string, chip?: string): string {
  const live = ACCOUNTS.filter((a) => a.type === "live" && !a.cent);
  const free = live.reduce((s, a) => s + freeMargin(a), 0);
  const s = q.toLowerCase();
  // typed questions on the same topics get the same answers as the suggestions
  const topic = chip ?? (/free margin/.test(s) ? "freeMargin" : /margin level|stop.?out|margin call/.test(s) ? "marginLevel" : /open.*account|new account/.test(s) ? "openAccount" : /deposit|fund/.test(s) ? "deposit" : undefined);
  switch (topic) {
    case "deposit":
      return "Open **Wallet → Deposit**, choose USDT on TRON (TRC20) or BNB Chain, and send from any wallet or exchange. Deposits are credited automatically after the network confirmations, usually within a minute; then move the funds to any live account with **Transfer**, instantly and free.";
    case "freeMargin":
      return `Free margin is the part of your equity you can still use for new positions: **equity − used margin**. Across your USD live accounts you have about **${formatMoney(free)}** free right now. Your figures per account are below.`;
    case "marginLevel":
      return "Margin level is **equity ÷ used margin × 100%**. Above 100% you can open new trades. At the **margin call** level (100%) we warn you, and at the **stop-out** level (50%) positions start closing automatically, the biggest loss first. All your accounts are healthy.";
    case "openAccount":
      return "Go to **Accounts → Open account**, pick Live or Demo and an account type (Standard, Pro, ECN or Cent), then choose your leverage. Your login is issued instantly; fund a live account from your wallet.";
  }
  if (s.includes("gold")) return "Gold (XAUUSD) is up 0.84% today at 2,654.30. Traders are pricing deeper Fed rate cuts after softer US data, and a weaker dollar is adding support. Key resistance sits near 2,670; support around 2,628, close to the stop on your 0.50 lot buy in account #80412337.";
  if (s.includes("week")) return `This week you closed 38 trades with a 66% win rate and +${formatMoney(2184.4)} net profit. Your best trade was XAUUSD (+$612.40). Most losses came from EURUSD shorts opened during the London open.`;
  return botAnswer(q).text;
}

function greeting() {
  const h = new Date().getHours();
  return h < 12 ? "morning" : h < 18 ? "afternoon" : "evening";
}

/* ------------------------------------------------------------------ */
/* Overview blocks on sample data                                      */
/* ------------------------------------------------------------------ */

function toCard(a: TradingAccount, t: ReturnType<typeof useT>): CardAccount {
  return {
    login: a.login,
    type: a.type,
    title: accountTitle(a, t),
    name: a.nickname ?? null,
    currency: a.cent ? "USC " : "$",
    cent: a.cent,
    balance: a.balance,
    equity: a.equity,
    freeMargin: freeMargin(a),
    marginLevel: Number.isFinite(marginLevel(a)) ? marginLevel(a) : null,
    leverage: a.leverage,
    server: a.server,
  };
}

/** Sample equity / P&L for the Statistics card: this period and the one before it (P&L is cumulative from the start of each period). */
function useDemoSeries(mode: StatMode, range: StatRange) {
  const all = React.useMemo(() => equitySeries(740, DASHBOARD.totalEquity), []);
  return React.useMemo(() => {
    const n = RANGE_DAYS[range];
    const cur = all.slice(-n);
    const prev = all.slice(-2 * n, -n);
    const step = range === "year" ? 7 : 1;
    const thin = <T,>(xs: T[]) => xs.filter((_, i) => i % step === 0 || i === xs.length - 1);
    const base = (xs: typeof all) => (mode === "pnl" ? xs[0]!.value : 0);
    const points: TrendPoint[] = thin(cur).map((p) => ({ t: p.time * 1000, v: p.value - base(cur) }));
    const compare = thin(prev).map((p) => p.value - base(prev));
    return { points, compare };
  }, [all, mode, range]);
}

const TX_ICON: Record<WalletTx["type"], { icon: React.ReactNode; tone: ListRowItem["tone"] }> = {
  deposit: { icon: <ArrowDownToLine />, tone: "mint" },
  withdrawal: { icon: <ArrowUpFromLine />, tone: "coral" },
  transfer: { icon: <ArrowLeftRight />, tone: "lavender" },
  "ib-payout": { icon: <Coins />, tone: "amber" },
  "copy-fee": { icon: <Users />, tone: "pink" },
  bonus: { icon: <Gift />, tone: "pink" },
  conversion: { icon: <Repeat />, tone: "sky" },
};

function DemoOverview() {
  const me = useSession();
  const t = useT();
  const f = useFormat();
  const [hour, setHour] = React.useState<string>("evening");
  React.useEffect(() => setHour(greeting()), []);
  const [mode, setMode] = React.useState<StatMode>("equity");
  const [range, setRange] = React.useState<StatRange>("month");
  const series = useDemoSeries(mode, range);
  const ib = DASHBOARD.earnings;
  const cards = React.useMemo(() => ACCOUNTS.map((a) => toCard(a, t)), [t]);
  const done = ONBOARDING.filter((s) => s.done).length;

  const STEP_ICON: Record<string, React.ReactNode> = { email: <Mail />, phone: <Phone />, deposit: <Wallet />, kyc: <IdCard /> };
  const checklist: ListRowItem[] = ONBOARDING.map((s) => ({
    key: s.key,
    icon: STEP_ICON[s.key] ?? <BadgeCheck />,
    tone: s.key === "kyc" ? "amber" : "accent",
    title: s.label,
    sub: s.done ? undefined : t("dashboard.onboarding.text"),
    done: s.done,
    status: s.done ? { label: t("dashboard.steps.state.done"), tone: "up" as const } : undefined,
    action: s.done ? undefined : { label: t("common.continue"), href: "/profile/verification" },
  }));

  const txRows: ListRowItem[] = WALLET_TXS.slice(0, 5).map((x) => {
    const out = x.type === "withdrawal" || x.type === "copy-fee";
    return {
      key: x.id,
      icon: TX_ICON[x.type].icon,
      tone: TX_ICON[x.type].tone,
      title: t.dyn(`wallet.txType.${x.type}`, x.type),
      sub: `${f.date(x.createdAt, { day: "numeric", month: "short" })} · ${x.asset}`,
      value: (
        <span dir="ltr" className={out ? "text-fg" : "text-up"}>
          {out ? "-" : "+"}
          {formatMoney(x.amount)}
        </span>
      ),
      href: "/wallet/history",
    };
  });
  const fundingRows: ListRowItem[] = [
    ...WALLET.assets.map((a) => ({
      key: a.asset,
      icon: <CoinIcon coin={a.icon} size={28} />,
      tone: "neutral" as const,
      title: `${a.asset} · ${a.network}`,
      sub: `${a.balance.toLocaleString("en-US")} ${a.asset} · ${formatMoney(a.usd)}`,
      status: a.asset === "USDT" ? { label: t("dashboard.home.connected"), tone: "ember" as const } : undefined,
      action: a.asset === "USDT" ? undefined : { label: t("common.deposit"), href: "/wallet/deposit" },
    })),
  ];
  const linkedRows: ListRowItem[] = [
    { key: "trader", icon: <CandlestickChart />, tone: "accent", title: "Kalks Trader", sub: t("dashboard.trader.chip"), action: { label: t("common.open"), href: TERMINAL_URL, external: true } },
    { key: "ib", icon: <Award />, tone: "amber", title: t("shell.nav.partner"), sub: ME.ibLevelName, status: { label: t("common.active"), tone: "ember" } },
    { key: "copy", icon: <Copy />, tone: "pink", title: t("shell.nav.copyTrading"), sub: t("dashboard.home.subscriptions", { count: COPY_SUBSCRIPTIONS.length }), status: { label: t("common.active"), tone: "ember" } },
    { key: "loyalty", icon: <Gift />, tone: "lavender", title: t("shell.nav.loyalty"), sub: t("dashboard.home.points", { points: LOYALTY.balance.toLocaleString("en-US") }), action: { label: t("dashboard.home.redeem"), href: "/rewards/loyalty" } },
  ];

  const prompts: Prompt[] =
    me.kyc_status === "verified"
      ? []
      : [
          {
            id: "kyc",
            title: t("dashboard.steps.kyc.title"),
            text: me.kyc_status === "pending" ? t("dashboard.steps.kyc.review") : t("dashboard.steps.kyc.todo"),
            icon: <IdCard />,
            tone: "amber",
            action: { label: t("dashboard.home.verifyNow"), href: "/profile/verification" },
          },
        ];

  const lvl = (a: TradingAccount) => marginLevel(a);
  const liveAccts = ACCOUNTS.filter((a) => a.type === "live");
  const aiChips: AiChip[] = [
    { key: "deposit", label: t("dashboard.ai.chip.deposit"), extra: <AiLink href="/wallet/deposit">{t("common.deposit")}</AiLink> },
    {
      key: "freeMargin",
      label: t("dashboard.ai.chip.freeMargin"),
      extra: <AiFacts title={t("dashboard.ai.yourAccounts")} rows={liveAccts.map((a) => ({ label: `#${a.login} · ${a.group}`, value: `${a.cent ? "USC " : "$"}${freeMargin(a).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}` }))} />,
    },
    {
      key: "marginLevel",
      label: t("dashboard.ai.chip.marginLevel"),
      extra: <AiFacts title={t("dashboard.ai.yourAccounts")} rows={liveAccts.map((a) => ({ label: `#${a.login} · ${a.group}`, value: Number.isFinite(lvl(a)) ? `${Math.round(lvl(a)).toLocaleString("en-US")}%` : "—", tone: lvl(a) > 500 ? ("up" as const) : lvl(a) > 200 ? ("warn" as const) : ("down" as const) }))} />,
    },
    { key: "openAccount", label: t("dashboard.ai.chip.openAccount"), question: t("dashboard.ai.q.openAccount"), extra: <AiLink href="/accounts/new">{t("dashboard.accounts.open")}</AiLink> },
  ];

  return (
    <OverviewLayout
      ai={<AskAi chips={aiChips} demoAnswer={demoAnswer} />}
      header={<PageHeader className="mb-0" title={t("shell.nav.overview")} subtitle={t.dyn(`dashboard.greeting.${hour}`, undefined, { name: me.first_name })} />}
      kpis={
        <div className="-mx-4 flex snap-x snap-mandatory scroll-px-4 gap-4 overflow-x-auto px-4 pb-1 [scrollbar-width:none] sm:mx-0 sm:grid sm:grid-cols-2 sm:overflow-visible sm:px-0 sm:pb-0 [&>*]:w-[78%] [&>*]:shrink-0 [&>*]:snap-start sm:[&>*]:w-auto">
          <KpiCard label={t("dashboard.equity.title")} icon={<TrendingUp />} value={<Money value={DASHBOARD.totalEquity} />} chip={t("dashboard.kpi.today", { pct: DASHBOARD.equityChangeTodayPct })} chipTone="up" href="/portfolio" />
          <KpiCard label={t("dashboard.home.todayPnl")} icon={<LineChart />} value={<Money value={DASHBOARD.equityChangeToday} signed tone="up" />} chip={t("dashboard.kpi.vsLastMonth", { pct: DASHBOARD.monthPnlPct })} chipTone="up" href="/portfolio/analytics" accent="var(--k-up)" delay={0.05} />
          <KpiCard
            label={t("dashboard.home.walletBalance")}
            icon={<Wallet />}
            value={<Money value={DASHBOARD.wallet} />}
            accent="var(--k-info)"
            footer={
              <div className="flex items-center gap-2">
                <div className="flex -space-x-1.5 rtl:space-x-reverse">
                  {["usdt", "trx", "btc"].map((c) => (
                    // eslint-disable-next-line @next/next/no-img-element
                    <img key={c} src={`/assets/coins/${c}.svg`} alt={c} className="size-5 rounded-full ring-2 ring-surface" />
                  ))}
                </div>
                <span className="text-[12px] font-semibold text-fg-3">USDT · TRC20</span>
              </div>
            }
            href="/wallet"
            delay={0.1}
          />
          <KpiCard
            label={t("dashboard.home.rewardsEarnings")}
            icon={<Award />}
            value={<Money value={ib.total} />}
            chipTone="gold"
            accent="var(--k-gold)"
            footer={
              <div className="flex flex-wrap items-center gap-1.5">
                <Chip size="sm" tone="gold">IB ${ib.ib.toFixed(0)}</Chip>
                <Chip size="sm">{t("dashboard.kpi.copy", { amount: `$${ib.copy.toFixed(0)}` })}</Chip>
                <Chip size="sm">PAMM ${ib.pamm.toFixed(0)}</Chip>
              </div>
            }
            href="/partner"
            delay={0.15}
          />
        </div>
      }
      statistic={<StatisticCard mode={mode} onMode={setMode} range={range} onRange={setRange} points={series.points} compare={series.compare} />}
      checklist={<ChecklistCard title={t("dashboard.onboarding.title")} subtitle={t("dashboard.onboarding.text")} rows={checklist} done={done} total={ONBOARDING.length} />}
      accounts={
        <AccountsPanel
          accounts={cards}
          actions={(c) => {
            const a = ACCOUNTS.find((x) => x.login === c.login)!;
            return (
              <>
                <a href={`${TERMINAL_URL}/?account=${a.login}`} target="_blank" rel="noopener" className="flex-1">
                  <Button variant="ember" className="w-full">
                    <CandlestickChart /> {t("dashboard.home.trade")}
                  </Button>
                </a>
                <Link href={`/wallet/transfer?to=${a.login}`} className="flex-1">
                  <Button variant="surface" className="w-full">
                    <ArrowDownToLine /> {t("common.deposit")}
                  </Button>
                </Link>
                <AccountMenu a={a} />
              </>
            );
          }}
        />
      }
      activity={
        <ActivityTabs
          tabs={[
            { key: "history", label: t("dashboard.home.history"), rows: txRows, empty: t("wallet.recent.emptyText"), more: { label: t("common.viewAll"), href: "/wallet/history" } },
            { key: "funding", label: t("dashboard.home.funding"), rows: fundingRows, empty: t("wallet.recent.emptyText") },
            { key: "linked", label: t("dashboard.home.linked"), rows: linkedRows, empty: "" },
          ]}
        />
      }
      balance={<BalancePanel total={DASHBOARD.totalEquity + DASHBOARD.wallet} chip={<span dir="ltr">+{DASHBOARD.equityChangeTodayPct}%</span>} chipTone="up" sub={t("dashboard.home.totalBalanceSub")} />}
      quick={
        <QuickActions
          title={t("dashboard.home.quickActions")}
          items={[
            { key: "transfer", label: t("common.transfer"), href: "/wallet/transfer", icon: <ArrowLeftRight className="rtl:-scale-x-100" />, tone: "lavender" },
            { key: "trader", label: "Kalks Trader", href: TERMINAL_URL, icon: <CandlestickChart />, tone: "accent", external: true },
            { key: "copy", label: t("shell.nav.copyTrading"), href: "/social", icon: <Copy />, tone: "pink" },
            { key: "support", label: t("shell.nav.support"), href: "/support", icon: <LifeBuoy />, tone: "amber" },
          ]}
        />
      }
      notifications={<NotificationsPanel prompts={prompts} />}
    />
  );
}

/* ------------------------------------------------------------------ */
/* More cards below the overview                                       */
/* ------------------------------------------------------------------ */

function MarginHealth() {
  const t = useT();
  const live = ACCOUNTS.filter((a) => a.type === "live" && !a.cent);
  const equity = live.reduce((s, a) => s + a.equity, 0);
  const margin = live.reduce((s, a) => s + a.margin, 0);
  const level = (equity / margin) * 100;
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title={t("dashboard.margin.title")} subtitle={t("dashboard.margin.subtitle")} action={<Chip tone="up" dot>{t("dashboard.margin.healthy")}</Chip>} />
      <div className="flex flex-1 items-center justify-center py-4">
        <Gauge value={Math.min(level, 2000)} max={2000} display={`${Math.round(level).toLocaleString()}%`} label={t("dashboard.margin.level")} size={220} />
      </div>
      <div className="grid grid-cols-2 gap-3 px-5 pb-5 sm:px-6 sm:pb-6">
        <div className="k-row px-4 py-3">
          <div className="text-[12px] text-fg-3">{t("dashboard.margin.used")}</div>
          <Money value={margin} className="mt-1 block text-[15px] font-bold" />
        </div>
        <div className="k-row px-4 py-3">
          <div className="text-[12px] text-fg-3">{t("dashboard.margin.free")}</div>
          <Money value={equity - margin} className="mt-1 block text-[15px] font-bold" />
        </div>
      </div>
    </Card>
  );
}

function ProfitLossCard() {
  const t = useT();
  return (
    <Card className="flex h-full flex-col">
      <div className="px-5 pt-5 sm:px-6 sm:pt-6">
        <div className="flex items-start justify-between">
          <div>
            <div className="k-label">{t("dashboard.pnl.title")}</div>
            <Money value={DASHBOARD.monthPnl} signed className="k-display mt-2 block text-[28px] font-bold text-up" />
          </div>
          <Chip tone="up" dot>
            {t("dashboard.pnl.lowRisk")}
          </Chip>
        </div>
        <div className="mt-6 flex items-end justify-between text-xs">
          <div>
            <div className="text-fg-3">{t("common.loss")}</div>
            <div className="k-num text-base font-bold text-down">{DASHBOARD.profitShare.lossPct}%</div>
          </div>
          <div className="text-end">
            <div className="text-fg-3">{t("common.profit")}</div>
            <div className="k-num text-base font-bold text-up">{DASHBOARD.profitShare.profitPct}%</div>
          </div>
        </div>
        <BarcodeBars lossPct={DASHBOARD.profitShare.lossPct} className="mt-2" height={80} />
      </div>
      <div className="mt-4 flex-1 divide-y divide-line px-5 pb-4 sm:px-6">
        {[
          [t("dashboard.pnl.winRate"), "64.2%"],
          [t("dashboard.pnl.trades"), "148"],
          [t("dashboard.pnl.avgWin"), "$212.40"],
          [t("dashboard.pnl.avgLoss"), "-$118.06"],
          [t("dashboard.pnl.charges"), "$96.30"],
        ].map(([k, v]) => (
          <div key={k} className="flex items-center justify-between py-2.5 text-[13px]">
            <span className="text-fg-3">{k}</span>
            <span dir="ltr" className={cn("k-num font-semibold", v.startsWith("-") ? "text-down" : "text-fg")}>
              {v}
            </span>
          </div>
        ))}
      </div>
    </Card>
  );
}

function CalendarCard() {
  const t = useT();
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("dashboard.calendar.title")}
        subtitle={t("dashboard.calendar.subtitle")}
        action={
          <Link href="/calendar">
            <Button size="sm" variant="surface">
              {t("common.viewAll")}
            </Button>
          </Link>
        }
      />
      <div className="k-fade-bottom mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {CALENDAR.slice(0, 5).map((e) => (
          <div key={e.id} className="k-row relative flex items-center gap-3 overflow-hidden py-3 pe-4 ps-5">
            <span className={cn("absolute inset-y-2 start-0 w-[3px] rounded-e-full", e.impact === 3 ? "bg-down" : e.impact === 2 ? "bg-warn" : "bg-fg-3")} />
            <div className="w-11 shrink-0 font-mono text-[12px] text-fg-3">{e.time}</div>
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13.5px] font-semibold">{e.title}</div>
              <div className="k-num mt-0.5 text-[11.5px] text-fg-3">
                {e.actual ? <span className="text-fg-2">{t("dashboard.calendar.actual", { value: e.actual })}</span> : null}
                {t("dashboard.calendar.forecastPrevious", { forecast: e.forecast, previous: e.previous })}
              </div>
            </div>
            <Chip size="sm" tone={e.impact === 3 ? "down" : "neutral"}>
              <span className={`fi fis fi-${e.country} size-3 rounded-full`} />
              {e.currency}
            </Chip>
          </div>
        ))}
      </div>
    </Card>
  );
}

function NewsCard() {
  const t = useT();
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("dashboard.news.title")}
        action={
          <Link href="/news">
            <Button size="sm" variant="surface">
              {t("dashboard.news.all")}
            </Button>
          </Link>
        }
      />
      <div className="k-fade-bottom mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {NEWS.slice(0, 4).map((n) => (
          <ListRow key={n.id} href="/news" className="items-start gap-3.5 py-3">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src={n.image} alt="" className="size-14 shrink-0 rounded-xl object-cover" />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2 text-[11.5px] text-fg-3">
                {n.pinned && (
                  <Chip size="sm" tone="ember">
                    {t("dashboard.news.pinned")}
                  </Chip>
                )}
                <span>{n.source}</span>·<span>{t("dashboard.time.minutesAgo", { count: n.minutesAgo })}</span>
              </div>
              <div className="mt-1 line-clamp-2 text-[13.5px] font-semibold leading-snug">{n.title}</div>
              <div className="mt-1.5 flex gap-1.5">
                {n.symbols.map((s) => (
                  <span key={s} className="rounded-md bg-surface-3 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-2">
                    {s}
                  </span>
                ))}
              </div>
            </div>
          </ListRow>
        ))}
      </div>
    </Card>
  );
}

function WorldCard() {
  const pins = [
    { country: "us", count: 18, label: "United States" },
    { country: "gb", count: 7, label: "United Kingdom" },
    { country: "de", count: 5, label: "Germany" },
    { country: "jp", count: 6, label: "Japan" },
    { country: "in", count: 4, label: "India" },
    { country: "sa", count: 3, label: "Saudi Arabia" },
    { country: "cn", count: 5, label: "China" },
    { country: "au", count: 2, label: "Australia" },
    { country: "br", count: 2, label: "Brazil" },
    { country: "sg", count: 2, label: "Singapore" },
  ];
  const t = useT();
  const heat = { us: 0.8, gb: 0.4, de: -0.6, jp: -0.3, au: 0.5, ch: -0.2, ca: -0.4, cn: 0.3 };
  return (
    <Card className="h-full">
      <CardHeader title={t("dashboard.world.title")} subtitle={t("dashboard.world.subtitle")} action={<Chip tone="ember" dot>{t("dashboard.world.stories", { count: 54 })}</Chip>} />
      <div className="px-4 pt-2 sm:px-6">
        <WorldMap pins={pins} heat={heat} />
      </div>
      <div className="px-5 pb-6 pt-2 sm:px-6">
        <MarketSessions />
      </div>
    </Card>
  );
}

function PositionsCard() {
  const t = useT();
  const qs = useQuotes(POSITIONS.map((p) => p.symbol));
  const total = POSITIONS.reduce((s, p) => s + positionProfit(p, qs[p.symbol]!.bid, qs[p.symbol]!.ask), 0);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title={t("dashboard.positions.title")}
        subtitle={
          <span>
            {t("dashboard.positions.summary", { count: POSITIONS.length })}{" "}
            <span className={cn("k-num font-semibold", total >= 0 ? "text-up" : "text-down")}>
              {total >= 0 ? "+" : "-"}
              {formatMoney(Math.abs(total))}
            </span>
          </span>
        }
        action={
          <Link target="_blank" rel="noopener" href={TERMINAL_URL}>
            <Button size="sm" variant="surface">
              <CandlestickChart /> {t("dashboard.positions.terminal")}
            </Button>
          </Link>
        }
      />
      <div className="mt-4 flex-1 space-y-2 px-4 pb-5 sm:px-6">
        {POSITIONS.map((p) => {
          const q = qs[p.symbol]!;
          const pnl = positionProfit(p, q.bid, q.ask);
          return (
            <div key={p.ticket} className="k-row flex items-center gap-3 px-4 py-2.5">
              <SymbolAvatar symbol={p.symbol} size={26} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2 text-[13.5px] font-semibold">
                  {p.symbol}
                  <Chip size="sm" tone={p.side === "buy" ? "up" : "down"}>
                    {t(p.side === "buy" ? "common.buy" : "common.sell").toUpperCase()} {p.volume}
                  </Chip>
                </div>
                <div className="k-num mt-0.5 font-mono text-[11px] text-fg-3">
                  {p.openPrice} → <PriceText symbol={p.symbol} value={p.side === "buy" ? q.bid : q.ask} dir={q.dir} className="text-[11px]" />
                </div>
              </div>
              <div dir="ltr" className={cn("k-num text-end text-[14px] font-bold", pnl >= 0 ? "text-up" : "text-down")}>
                {pnl >= 0 ? "+" : "-"}
                {formatMoney(Math.abs(pnl))}
              </div>
            </div>
          );
        })}
      </div>
    </Card>
  );
}

function PartnerBanner() {
  const me = useSession();
  const t = useT();
  return (
    <Card className="k-card-hot relative overflow-hidden">
      <div className="relative flex flex-col gap-5 p-6 sm:p-7 md:flex-row md:items-center md:justify-between">
        <div className="max-w-xl">
          <Chip tone="gold" className="mb-3">
            <BadgeCheck className="size-3.5" /> {t("dashboard.partner.chip")}
          </Chip>
          <h3 className="k-display text-[22px] font-bold tracking-[-0.02em] sm:text-2xl">{t("dashboard.partner.title")}</h3>
          <p className="mt-2 text-sm text-fg-2">
            <Trans k="dashboard.partner.text" vars={{ url: `app.kalkstrade.com/r/${me.referral_code}` }} tags={{ link: (c) => <span className="font-mono text-fg" dir="ltr">{c}</span> }} />
          </p>
        </div>
        <div className="flex items-center gap-4">
          <Icon3D name="handshake" size={72} className="hidden md:inline-grid" />
          <Link href="/partner">
            <Button variant="ink" size="lg">
              {t("dashboard.partner.open")} <ArrowUpRight className="rtl:-scale-x-100" />
            </Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

export default function DemoDashboard() {
  const t = useT();
  return (
    <div className="pb-40">
      <DemoOverview />

      <SectionTitle
        action={
          <Link target="_blank" rel="noopener" href={TERMINAL_URL}>
            <Button variant="ink">
              {t("dashboard.openTerminal")} <ArrowUpRight className="rtl:-scale-x-100" />
            </Button>
          </Link>
        }
      >
        {t("dashboard.home.tradingTitle")}
      </SectionTitle>
      <div className="grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3">
        <Reveal>
          <PositionsCard />
        </Reveal>
        <Reveal delay={0.05}>
          <MarginHealth />
        </Reveal>
        <Reveal delay={0.1} className="md:col-span-2 xl:col-span-1">
          <ProfitLossCard />
        </Reveal>
      </div>

      <SectionTitle>{t("dashboard.home.marketsTitle")}</SectionTitle>
      <div className="grid grid-cols-1 gap-5 md:grid-cols-2 xl:grid-cols-3">
        <Reveal delay={0.05}>
          <CalendarCard />
        </Reveal>
        <Reveal delay={0.1}>
          <MoversCard />
        </Reveal>
        <Reveal delay={0.15} className="md:col-span-2 xl:col-span-1">
          <NewsCard />
        </Reveal>
      </div>

      <div className="mt-5 grid grid-cols-1 gap-5 xl:grid-cols-12">
        <Reveal className="xl:col-span-7">
          <WorldCard />
        </Reveal>
        <Reveal delay={0.05} className="xl:col-span-5">
          <PartnerBanner />
        </Reveal>
      </div>
    </div>
  );
}

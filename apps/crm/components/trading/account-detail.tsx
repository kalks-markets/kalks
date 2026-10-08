"use client";

import * as React from "react";
import { useReadOnly } from "@/components/session";
import Link from "next/link";
import { useParams, useRouter, useSearchParams } from "next/navigation";
import { AnimatePresence, motion } from "motion/react";
import { ArrowLeft, RotateCw, Server } from "lucide-react";
import { Button, Card, CardHeader, Chip, CopyButton, EmptyState, Gauge, KeyValue, Money, Reveal, Skeleton, Tabs, cn } from "@/components/kit";
import { STATUS_LABEL, curOf, fmtAmount, isArchived, fmtDate, fmtLevel, fmtPrice, levelTone, modeLabel, serverOf, serverTime, usePoll, type AccountDetail, type EngineAccount, type EnginePosition, type EngineOrder, type HistoryPage } from "./api";
import { DealsTable, HistoryPanel, LedgerPanel } from "./activity";
import { CredentialsPanel, SettingsPanel } from "./manage";
import { AccountActions, FundButton, KindBadge, ProductChip, RefillButton, StatusBadge, TradeButton, isPropAccount } from "./ui";
import { productOf } from "@/lib/products";
import { FlavorChip, RestoreButton, accountFlavor, copyingName } from "./archive";
import { ClosureBanner } from "./closure";
import { DefaultStar, HealthCard } from "./extras";
import { AccountAnalyticsPanel } from "@/components/reports/live-analytics";
import { Trans, useT } from "@kalks/i18n/react";
import { OptionPremium, OptionTag, TradeSymbolAvatar, fmtContracts, symbolLabel } from "./instrument";
import { isOptionTrade, optionTerms, positionPremiumsUsd, usdFactorOf } from "./option-deal";

const TAB_KEYS = ["overview", "positions", "history", "ledger", "analytics", "credentials", "settings"] as const;
type TabKey = (typeof TAB_KEYS)[number];

function StatTile({ label, children, tone }: { label: string; children: React.ReactNode; tone?: "up" | "down" | "warn" }) {
  return (
    <div className="k-row min-w-0 px-4 py-3">
      <div className="text-[12px] text-fg-3">{label}</div>
      <div className={cn("k-num mt-1 truncate text-[15px] font-semibold sm:text-[16px]", tone === "up" && "text-up", tone === "down" && "text-down", tone === "warn" && "text-warn")}>{children}</div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Positions / orders                                                  */
/* ------------------------------------------------------------------ */

const ORDER_TYPE = { market: "accountDetail.orderType.market", limit: "accountDetail.orderType.limit", stop: "accountDetail.orderType.stop", stop_limit: "accountDetail.orderType.stopLimit" } as const satisfies Record<EngineOrder["type"], string>;

function PositionsTable({ positions, cur, usdFactor }: { positions: EnginePosition[]; cur: string; usdFactor: number }) {
  const t = useT();
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[820px] border-separate border-spacing-y-2 text-[13.5px]">
        <thead>
          <tr className="text-[12px] text-fg-3">
            <th className="px-4 text-start font-medium">{t("accountDetail.col.symbol")}</th>
            <th className="px-3 text-start font-medium">{t("accountDetail.col.ticket")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.volume")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.open")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.current")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.slTp")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.swap")}</th>
            <th className="px-4 text-end font-medium">{t("accountDetail.col.pnl")}</th>
          </tr>
        </thead>
        <tbody>
          {positions.map((p) => {
            const d = p.openPrice >= 1000 ? 2 : p.openPrice >= 50 ? 3 : 5;
            const isOpt = isOptionTrade(p);
            const opt = isOpt ? optionTerms(p.symbol, p.option) : null;
            const prem = isOpt ? positionPremiumsUsd(p, usdFactor) : null;
            return (
              <tr key={p.ticket} className="bg-surface-2">
                <td className="rounded-s-[14px] border-y border-s border-line px-4 py-3">
                  <div className="flex items-center gap-3">
                    <TradeSymbolAvatar symbol={opt?.series ?? p.symbol} size={26} />
                    <div>
                      <div className="flex flex-wrap items-center gap-2 font-medium">
                        <span className="whitespace-nowrap" title={isOpt ? p.symbol : undefined}>
                          {symbolLabel(t, p.symbol, p.option)}
                        </span>
                        <Chip size="sm" tone={p.side === "buy" ? "up" : "down"}>
                          {p.side === "buy" ? t("accountDetail.side.buy") : t("accountDetail.side.sell")}
                        </Chip>
                        {isOpt && <OptionTag />}
                      </div>
                      <div className="text-[11px] text-fg-3">{serverTime(p.openTime)}</div>
                    </div>
                  </div>
                </td>
                <td className="border-y border-line px-3 font-mono text-[12px] text-fg-3">#{p.ticket}</td>
                <td className="k-num whitespace-nowrap border-y border-line px-3 text-end">{isOpt ? t("accounts.opt.contracts", { count: fmtContracts(p.volume) }) : p.volume.toFixed(2)}</td>
                <td className="k-num border-y border-line px-3 text-end font-mono text-fg-2">{prem ? <OptionPremium usd={prem.open} unit={p.openPrice} currency={opt?.quoteCurrency} /> : fmtPrice(p.openPrice, d)}</td>
                <td className="k-num border-y border-line px-3 text-end font-mono">{prem ? <OptionPremium usd={prem.now} unit={p.currentPrice} currency={opt?.quoteCurrency} /> : fmtPrice(p.currentPrice, d)}</td>
                <td className="k-num border-y border-line px-3 text-end font-mono text-[12px] text-fg-3">
                  {isOpt ? (
                    "—"
                  ) : (
                    <>
                      <span className="text-down/80">{p.sl ? fmtPrice(p.sl, d) : "—"}</span> / <span className="text-up/80">{p.tp ? fmtPrice(p.tp, d) : "—"}</span>
                    </>
                  )}
                </td>
                <td className={cn("k-num border-y border-line px-3 text-end text-[12.5px]", p.swap < 0 ? "text-down" : "text-fg-2")}>{isOpt && !p.swap ? "—" : fmtAmount(p.swap, "")}</td>
                <td className={cn("k-num rounded-e-[14px] border-y border-e border-line px-4 text-end text-[14px] font-semibold", p.profit > 0 ? "text-up" : p.profit < 0 ? "text-down" : "")}>{fmtAmount(p.profit, cur, true)}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function OrdersTable({ orders }: { orders: EngineOrder[] }) {
  const t = useT();
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[640px] border-separate border-spacing-y-2 text-[13.5px]">
        <thead>
          <tr className="text-[12px] text-fg-3">
            <th className="px-4 text-start font-medium">{t("accountDetail.col.symbol")}</th>
            <th className="px-3 text-start font-medium">{t("accountDetail.col.ticket")}</th>
            <th className="px-3 text-start font-medium">{t("common.type")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.volume")}</th>
            <th className="px-3 text-end font-medium">{t("accountDetail.col.price")}</th>
            <th className="px-4 text-end font-medium">{t("accountDetail.col.placed")}</th>
          </tr>
        </thead>
        <tbody>
          {orders.map((o) => {
            const legs = o.option?.legs ?? [];
            const isOpt = isOptionTrade(o);
            const first = legs[0];
            const opt = isOpt ? optionTerms(first?.series ?? o.symbol, first?.option) : null;
            return (
            <tr key={o.ticket} className="bg-surface-2">
              <td className="rounded-s-[14px] border-y border-s border-line px-4 py-3">
                <div className="flex items-center gap-3">
                  <TradeSymbolAvatar symbol={opt?.series ?? o.symbol} size={24} />
                  <span className="flex flex-wrap items-center gap-2 font-medium">
                    <span className="whitespace-nowrap" title={isOpt ? o.symbol : undefined}>
                      {symbolLabel(t, first?.series ?? o.symbol, first?.option)}
                    </span>
                    {legs.length > 1 && <span className="text-[11.5px] font-normal text-fg-3">{t("accounts.opt.moreLegs", { count: legs.length - 1 })}</span>}
                    {isOpt && <OptionTag />}
                  </span>
                </div>
              </td>
              <td className="border-y border-line px-3 font-mono text-[12px] text-fg-3">#{o.ticket}</td>
              <td className="border-y border-line px-3">
                <Chip size="sm" tone={o.side === "buy" ? "up" : "down"}>
                  {t("accountDetail.order.label", { side: o.side === "buy" ? t("common.buy") : t("common.sell"), type: t(ORDER_TYPE[o.type]) })}
                </Chip>
              </td>
              <td className="k-num whitespace-nowrap border-y border-line px-3 text-end">{isOpt ? t("accounts.opt.contracts", { count: fmtContracts(first?.contracts ?? o.volume) }) : o.volume.toFixed(2)}</td>
              <td className="k-num border-y border-line px-3 text-end font-mono text-fg-2">
                {isOpt ? (o.option?.limitPremium != null ? <OptionPremium usd={null} unit={o.option.limitPremium} currency={opt?.quoteCurrency} /> : t(ORDER_TYPE.market)) : fmtPrice(o.price)}
              </td>
              <td className="k-num rounded-e-[14px] border-y border-e border-line px-4 text-end text-[12.5px] text-fg-3">{serverTime(o.placedAt)}</td>
            </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function PositionsPanel({ a, positions, orders }: { a: EngineAccount; positions: EnginePosition[]; orders: EngineOrder[] }) {
  const t = useT();
  const cur = curOf(a);
  if (positions.length === 0 && orders.length === 0)
    return (
      <Card>
        <EmptyState
          art="emptyPosition"
          title={t("accountDetail.positions.emptyTitle")}
          text={t("accountDetail.positions.emptyText")}
          action={<TradeButton a={a} size="md" label={t("accountDetail.positions.openTrader")} />}
        />
      </Card>
    );
  return (
    <div className="space-y-4">
      <Reveal>
        <Card>
          <CardHeader
            title={t("accountDetail.overview.openPositions")}
            subtitle={
              <span>
                <Trans
                  k="accountDetail.positions.subtitleTrader"
                  vars={{ count: positions.length, amount: fmtAmount(a.profit, cur, true) }}
                  tags={{ pnl: (c) => <span className={cn("k-num font-medium", a.profit > 0 ? "text-up" : a.profit < 0 ? "text-down" : "")}>{c}</span> }}
                />
              </span>
            }
            action={<TradeButton a={a} label={t("accountDetail.positions.manageInTrader")} />}
          />
          <div className="mt-2 px-4 pb-5 sm:px-6">
            {positions.length ? <PositionsTable positions={positions} cur={cur} usdFactor={usdFactorOf(a)} /> : <div className="py-6 text-center text-[13px] text-fg-3">{t("accountDetail.positions.none")}</div>}
            {positions.some(isOptionTrade) && <div className="mt-1 text-[11.5px] text-fg-3">{t("accounts.opt.premiumHint")}</div>}
          </div>
        </Card>
      </Reveal>
      {orders.length > 0 && (
        <Reveal delay={0.05}>
          <Card>
            <CardHeader title={t("accountDetail.orders.title")} subtitle={t("accountDetail.orders.subtitle", { count: orders.length })} />
            <div className="mt-2 px-4 pb-5 sm:px-6">
              <OrdersTable orders={orders} />
            </div>
          </Card>
        </Reveal>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Overview                                                            */
/* ------------------------------------------------------------------ */

function OverviewPanel({ a, positions, onTab }: { a: EngineAccount; positions: EnginePosition[]; onTab: (t: TabKey) => void }) {
  const t = useT();
  const cur = curOf(a);
  const recent = usePoll<HistoryPage>(`accounts/${a.login}/history?limit=6`, 15000);
  const ml = a.marginLevel ?? 0;
  const tone = levelTone(a.marginLevel);
  const st = STATUS_LABEL[a.status];
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="xl:col-span-5">
          <Card className="h-full">
            <CardHeader
              title={t("accountDetail.stat.margin")}
              subtitle={t("accountDetail.margin.subtitle", { call: a.marginCallLevel, stopOut: a.stopOutLevel })}
              action={
                a.margin > 0 ? (
                  <Chip tone={a.marginCall ? "down" : tone === "up" ? "up" : tone === "warn" ? "warn" : "down"} dot>
                    {a.marginCall ? t("accountDetail.margin.call") : tone === "up" ? t("accountDetail.health.healthy") : tone === "warn" ? t("accountDetail.health.watch") : t("accountDetail.health.atRisk")}
                  </Chip>
                ) : (
                  <Chip>{t("accountDetail.margin.noneUsed")}</Chip>
                )
              }
            />
            <div className="flex justify-center py-2">
              <Gauge value={a.margin > 0 ? Math.min(ml, 3000) : 0} max={3000} size={180} display={fmtLevel(a.margin > 0 ? a.marginLevel : null)} label={t("accountDetail.stat.marginLevel")} />
            </div>
            <div className="grid grid-cols-2 gap-2 px-4 pb-5 sm:px-6">
              <StatTile label={t("common.balance")}>
                <Money value={a.balance} currency={cur} countUp={false} />
              </StatTile>
              <StatTile label={t("common.equity")}>
                <Money value={a.equity} currency={cur} countUp={false} />
              </StatTile>
              <StatTile label={t("accountDetail.stat.margin")}>
                <Money value={a.margin} currency={cur} countUp={false} />
              </StatTile>
              <StatTile label={t("accountDetail.stat.freeMargin")}>
                <Money value={a.freeMargin} currency={cur} countUp={false} />
              </StatTile>
              <StatTile label={t("accountDetail.stat.credit")}>
                <Money value={a.credit + a.bonus} currency={cur} countUp={false} />
              </StatTile>
              <StatTile label={t("accountDetail.stat.floatingPnl")} tone={a.profit > 0 ? "up" : a.profit < 0 ? "down" : undefined}>
                {fmtAmount(a.profit, cur, true)}
              </StatTile>
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.05} className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader
              title={t("accountDetail.overview.openPositions")}
              subtitle={t("accountDetail.overview.positionsSubtitle", { open: a.positions, pending: a.orders })}
              action={
                <Button size="sm" variant="surface" onClick={() => onTab("positions")}>
                  {t("accountDetail.overview.allPositions")}
                </Button>
              }
            />
            <div className="mt-4 space-y-2 px-4 pb-5 sm:px-6">
              {positions.slice(0, 5).map((p) => {
                const isOpt = isOptionTrade(p);
                const prem = isOpt ? positionPremiumsUsd(p, usdFactorOf(a)) : null;
                return (
                <div key={p.ticket} className="k-row flex items-center gap-3 px-4 py-2.5">
                  <TradeSymbolAvatar symbol={p.symbol} size={24} />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2 text-[13.5px] font-medium">
                      <span className="truncate">{symbolLabel(t, p.symbol, p.option)}</span>
                      <Chip size="sm" tone={p.side === "buy" ? "up" : "down"}>
                        {p.side === "buy" ? t("accountDetail.side.buy") : t("accountDetail.side.sell")} {isOpt ? t("accounts.opt.contracts", { count: fmtContracts(p.volume) }) : p.volume}
                      </Chip>
                      {isOpt && <OptionTag />}
                    </div>
                    <div className="k-num mt-0.5 truncate font-mono text-[11px] text-fg-3">
                      {prem && prem.open !== null && prem.now !== null ? `${fmtAmount(prem.open, "$")} → ${fmtAmount(prem.now, "$")} ${t("accounts.opt.perContract")}` : `${fmtPrice(p.openPrice)} → ${fmtPrice(p.currentPrice)}`} · #{p.ticket}
                    </div>
                  </div>
                  <span className={cn("k-num text-[14px] font-semibold", p.profit > 0 ? "text-up" : p.profit < 0 ? "text-down" : "")}>{fmtAmount(p.profit, cur, true)}</span>
                </div>
                );
              })}
              {positions.length === 0 && <div className="py-8 text-center text-[13px] text-fg-3">{t("accountDetail.overview.noPositions")}</div>}
            </div>
          </Card>
        </Reveal>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.08} className="xl:col-span-7">
          <Card className="h-full">
            <CardHeader
              title={t("accountDetail.overview.recentDeals")}
              subtitle={recent.data ? t("accountDetail.overview.dealsCount", { count: recent.data.total }) : t("accountDetail.overview.recentDealsSubtitle")}
              action={
                <Button size="sm" variant="surface" onClick={() => onTab("history")}>
                  {t("accountDetail.overview.fullHistory")}
                </Button>
              }
            />
            <div className="px-4 pb-5 pt-3 sm:px-6">
              {recent.loading && <Skeleton className="h-40 w-full rounded-[14px]" />}
              {recent.data && recent.data.deals.length > 0 && <DealsTable deals={recent.data.deals} cur={cur} usdFactor={usdFactorOf(a)} />}
              {recent.data && recent.data.deals.length === 0 && <div className="py-8 text-center text-[13px] text-fg-3">{t("accountDetail.overview.noDeals")}</div>}
              {recent.error && !recent.data && <div className="py-8 text-center text-[13px] text-fg-3">{recent.error.message}</div>}
            </div>
          </Card>
        </Reveal>
        <Reveal delay={0.12} className="xl:col-span-5">
          <Card className="h-full">
            <CardHeader title={t("accountDetail.info.title")} />
            <div className="px-6 pb-4 pt-1">
              <KeyValue
                rows={[
                  [t("common.type"), `${a.type === "live" ? t("common.live") : t("common.demo")} · ${a.groupName}`],
                  [t("accounts.wizard.step.product"), t(productOf(a) === "options" ? "accounts.product.options" : "accounts.product.cfd")],
                  // position mode and leverage are CFD terms (CFD / Options account split)
                  ...(productOf(a) === "options"
                    ? []
                    : ([
                        [t("accountDetail.info.positionMode"), modeLabel(a.mode)],
                      ] as [string, React.ReactNode][])),
                  [t("common.currency"), a.cent ? t("accountDetail.info.uscCents") : a.currency],
                  ...(productOf(a) === "options" ? [] : ([[t("accountDetail.info.leverage"), `1:${a.leverage.toLocaleString("en-US")}`]] as [string, React.ReactNode][])),
                  [t("accountDetail.info.server"), <span key="sv" className="font-mono">{serverOf(a)}</span>],
                  [t("common.status"), <Chip key="st" size="sm" tone={st.tone}>{st.label}</Chip>],
                  [t("accountDetail.info.opened"), fmtDate(a.createdAt)],
                ]}
              />
            </div>
          </Card>
        </Reveal>
      </div>
      {!isArchived(a) && (
        <Reveal delay={0.15}>
          <HealthCard a={a} />
        </Reveal>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */

function Detail() {
  const { login } = useParams<{ login: string }>();
  const sp = useSearchParams();
  const router = useRouter();
  const t = useT();
  const readOnly = useReadOnly();
  const valid = /^\d{8}$/.test(login ?? "");
  const { data, error, loading, reload } = usePoll<AccountDetail>(valid ? `accounts/${login}` : null, 3000);
  const initial = (TAB_KEYS as readonly string[]).includes(sp.get("tab") ?? "") ? (sp.get("tab") as TabKey) : "overview";
  const [tab, setTabState] = React.useState<TabKey>(initial);
  const setTab = (k: TabKey) => {
    setTabState(k);
    router.replace(`/accounts/${login}${k === "overview" ? "" : `?tab=${k}`}`, { scroll: false });
  };

  if (!valid || (error && error.status === 404))
    return (
      <div className="pb-16">
        <Card className="mt-10">
          <EmptyState
            art="market"
            title={t("accountDetail.notFound.title", { login })}
            text={t("accountDetail.notFound.text")}
            action={
              <div className="flex gap-2">
                <Link href="/accounts">
                  <Button variant="surface">
                    <ArrowLeft className="rtl:-scale-x-100" /> {t("accountDetail.notFound.myAccounts")}
                  </Button>
                </Link>
                <Link href="/accounts/new">
                  <Button variant="ember">{t("accountDetail.notFound.openAccount")}</Button>
                </Link>
              </div>
            }
          />
        </Card>
      </div>
    );

  if (error && !data)
    return (
      <Card className="mt-10">
        <EmptyState
          art="connectionLost"
          title={t("accountDetail.unavailable.title")}
          text={error.message}
          action={
            <Button variant="surface" onClick={reload}>
              <RotateCw /> {t("common.retry")}
            </Button>
          }
        />
      </Card>
    );

  if (loading || !data)
    return (
      <div className="pb-16">
        <Skeleton className="mb-4 h-5 w-48" />
        <Skeleton className="h-[220px] w-full rounded-[20px]" />
        <Skeleton className="mt-6 h-10 w-full max-w-xl rounded-full" />
        <Skeleton className="mt-5 h-[320px] w-full rounded-[20px]" />
      </div>
    );

  const a = data.account;
  const cur = curOf(a);
  const lt = a.margin > 0 ? levelTone(a.marginLevel) : undefined;
  const archived = isArchived(a);

  return (
    <div className="pb-16">
      <div className="mb-4 flex items-center gap-2 text-[13px] text-fg-3">
        <Link href="/accounts" className="inline-flex items-center gap-1.5 hover:text-fg">
          <ArrowLeft className="size-3.5 rtl:-scale-x-100" /> {t("accountDetail.breadcrumb.accounts")}
        </Link>
        <span>/</span>
        <span className="font-mono text-fg-2">#{a.login}</span>
      </div>

      <motion.div initial={{ opacity: 0, y: 10 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}>
        <Card className="overflow-hidden">
          <div className="relative flex flex-col gap-6 p-6 lg:flex-row lg:items-end lg:justify-between">
            <div className="min-w-0">
              <div className="flex flex-wrap items-center gap-2">
                <KindBadge type={a.type} prop={isPropAccount(a)} />
                <FlavorChip a={a} />
                <ProductChip a={a} />
                <h1 className="text-[20px] font-medium tracking-tight">
                  {productOf(a) === "options" ? a.groupName : `${a.groupName} · ${modeLabel(a.mode)}`}
                </h1>
                {a.name && <span className="text-[14px] text-fg-3">“{a.name}”</span>}
                <DefaultStar a={a} />
                <StatusBadge a={a} />
              </div>
              <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1.5 text-[13px] text-fg-2">
                <span className="inline-flex items-center gap-1 font-mono">
                  #{a.login}
                  <CopyButton value={String(a.login)} label={t("accountDetail.info.login")} />
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <Server className="size-3.5 text-fg-3" />
                  <span className="font-mono">{serverOf(a)}</span>
                </span>
                {productOf(a) !== "options" && <Chip size="sm">1:{a.leverage.toLocaleString("en-US")}</Chip>}
                <span className="text-fg-3">{a.cent ? t("accountDetail.header.centCurrency") : a.currency}</span>
                {accountFlavor(a) === "copy" && copyingName(a) && <span className="font-medium text-fg">{t("accounts.copy.copying", { name: copyingName(a)! })}</span>}
                {archived && (a.archivedAt ?? a.closedAt) && <span className="text-fg-3">{t(a.status === "closed" ? "accounts.archived.closedOn" : "accounts.archived.on", { date: fmtDate(a.archivedAt ?? a.closedAt) })}</span>}
              </div>
              <div className="k-label mt-5">{t("common.equity")}</div>
              <div className="mt-1 flex flex-wrap items-baseline gap-3">
                <Money value={a.equity} currency={cur} countUp={false} className="text-[40px] font-semibold leading-none tracking-[-0.02em] sm:text-[46px]" />
                {a.type === "live" && !isPropAccount(a) && a.balance === 0 && a.equity === 0 && <Chip tone="warn">{t("accountDetail.header.notFunded")}</Chip>}
              </div>
              <div className="mt-4 flex flex-wrap gap-x-6 gap-y-2 text-[13px]">
                <span className="text-fg-3">
                  {t("common.balance")} <Money value={a.balance} currency={cur} countUp={false} className="ms-1 font-medium text-fg" />
                </span>
                <span className="text-fg-3">
                  {t("accountDetail.stat.freeMargin")} <Money value={a.freeMargin} currency={cur} countUp={false} className="ms-1 font-medium text-fg" />
                </span>
                <span className="text-fg-3">
                  {t("accountDetail.stat.marginLevel")} <span className={cn("k-num ms-1 font-medium", lt === "up" && "text-up", lt === "warn" && "text-warn", lt === "down" && "text-down")}>{fmtLevel(a.margin > 0 ? a.marginLevel : null)}</span>
                </span>
                <span className="text-fg-3">
                  {t("accountDetail.stat.floatingPnl")} <span className={cn("k-num ms-1 font-medium", a.profit > 0 ? "text-up" : a.profit < 0 ? "text-down" : "text-fg")}>{fmtAmount(a.profit, cur, true)}</span>
                </span>
              </div>
            </div>
            {!readOnly && archived && a.status === "archived" && (
              <div className="flex flex-wrap items-center gap-2">
                <RestoreButton a={a} onDone={reload} />
              </div>
            )}
            {!readOnly && !archived && (
              <div className="flex flex-wrap items-center gap-2">
                <AccountActions a={a} onChanged={reload} />
                {a.type === "live" ? !isPropAccount(a) && <FundButton a={a} size="md" /> : <RefillButton a={a} onDone={reload} size="md" />}
                {accountFlavor(a) === "copy" ? (
                  <>
                    <Link href="/social/copy">
                      <Button size="md" variant="surface">
                        {t("accounts.copy.manage")}
                      </Button>
                    </Link>
                    <TradeButton a={a} size="lg" variant="surface" label={t("accounts.copy.watchPnl")} />
                  </>
                ) : (
                  <TradeButton a={a} size="lg" />
                )}
              </div>
            )}
          </div>
        </Card>
        <ClosureBanner a={a} onChanged={reload} />
      </motion.div>

      <div className="-mx-4 mt-6 overflow-x-auto px-4 sm:mx-0 sm:px-0">
        <Tabs
          className="min-w-max"
          value={tab}
          onChange={setTab}
          tabs={[
            { value: "overview", label: t("accountDetail.tab.overview") },
            { value: "positions", label: t("accountDetail.tab.positions"), count: a.positions + a.orders },
            { value: "history", label: t("accountDetail.tab.history") },
            { value: "ledger", label: t("accountDetail.tab.ledger") },
            { value: "analytics", label: t("accountDetail.tab.analytics") },
            // a view-only login (D90) never sees credentials or settings
            ...(readOnly || archived ? [] : [{ value: "credentials" as const, label: t("accountDetail.tab.credentials") }, { value: "settings" as const, label: t("accountDetail.tab.settings") }]),
          ]}
        />
      </div>

      <div className="mt-5">
        <AnimatePresence mode="wait">
          <motion.div key={tab} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -4 }} transition={{ duration: 0.2 }}>
            {tab === "overview" && <OverviewPanel a={a} positions={data.positions} onTab={setTab} />}
            {tab === "positions" && <PositionsPanel a={a} positions={data.positions} orders={data.orders} />}
            {tab === "history" && <HistoryPanel a={a} />}
            {tab === "ledger" && <LedgerPanel a={a} />}
            {tab === "analytics" && <AccountAnalyticsPanel login={a.login} />}
            {tab === "credentials" && !readOnly && !archived && <CredentialsPanel a={a} />}
            {tab === "settings" && !readOnly && !archived && <SettingsPanel a={a} onChanged={reload} />}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

export function LiveAccountDetail() {
  return (
    <React.Suspense fallback={null}>
      <Detail />
    </React.Suspense>
  );
}

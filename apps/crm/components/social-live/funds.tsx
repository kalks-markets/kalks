"use client";

import * as React from "react";
import Link from "next/link";
import { ArrowUpRight, CalendarClock, Coins, Crown, LayoutGrid, Lock, Rows3, ShieldAlert, Snowflake, TrendingUp, Users, Wallet } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, Dialog, EmptyState, EquityChart, KeyValue, KpiCard, PageHeader, Segmented, cn, type Column, type SeriesPoint } from "@/components/kit";
import { useFormat, useT } from "@kalks/i18n/react";
import { fmtDate, serverTime } from "@/components/trading/api";
import { PERIOD_LABEL, compactUsd, nav4, pct, usd, useSocial, type FeePeriod, type FundDetail, type FundView } from "./api";
import { BlockSkeleton, InfoBox, MasterIdentity, SocialError, Tile } from "./bits";
import { InvestDialog } from "./invest-dialog";
import { VisitingCard } from "@/components/visiting-card";

type Rollover = FundDetail["rollovers"][number];

export function FundStatusChip({ status }: { status: FundView["status"] }) {
  const t = useT();
  if (status === "active") return <Chip size="sm" tone="up" dot>{t("social.fundStatus.active")}</Chip>;
  if (status === "frozen")
    return (
      <Chip size="sm" tone="down">
        <Snowflake className="size-3" /> {t("social.fundStatus.frozen")}
      </Chip>
    );
  return <Chip size="sm">{t("social.fundStatus.closed")}</Chip>;
}

/* ------------------------------------------------------------------ */
/* Fund detail drawer (NAV history + rollovers)                        */
/* ------------------------------------------------------------------ */

export function FundDetailDrawer({ fundId, onClose, onInvest }: { fundId: number | null; onClose: () => void; onInvest?: (id: number) => void }) {
  const t = useT();
  const fmt = useFormat();
  const { data, error } = useSocial<FundDetail>(fundId ? `funds/${fundId}` : null, 30000);
  const d = data && data.fund.id === fundId ? data : null;
  const series = React.useMemo<SeriesPoint[]>(() => {
    if (!d) return [];
    return d.navHistory
      .map((p) => ({ time: Math.floor(Date.parse(p.at) / 1000), value: p.nav }))
      .filter((p) => Number.isFinite(p.time) && Number.isFinite(p.value))
      .sort((a, b) => a.time - b.time)
      .filter((p, i, arr) => i === 0 || p.time > arr[i - 1]!.time);
  }, [d]);
  const rollCols: Column<Rollover>[] = [
    { key: "at", header: t("social.rollover"), cell: (r) => <span className="k-num whitespace-nowrap text-fg-2">{serverTime(r.at)}</span>, sort: (r) => r.at },
    { key: "nav", header: "NAV", align: "right", cell: (r) => <span className="k-num font-medium">{nav4(r.nav)}</span> },
    { key: "in", header: t("social.funds.col.in"), align: "right", cell: (r) => <span className="k-num text-up">{usd(r.invested, 0)}</span>, hideOn: "sm" },
    { key: "out", header: t("social.funds.col.out"), align: "right", cell: (r) => <span className="k-num text-down">{usd(r.redeemed, 0)}</span>, hideOn: "sm" },
    { key: "fees", header: t("social.fees"), align: "right", cell: (r) => <span className="k-num text-fg-2">{usd(r.fees)}</span> },
  ];
  const f = d?.fund;
  return (
    <Dialog open={fundId !== null} onOpenChange={(o) => !o && onClose()} side="right" title={f ? f.name : t("social.funds.pammFund")} description={f ? t("social.funds.managedBy", { name: f.master.nickname }) : undefined}>
      {!d || !f ? (
        error ? <InfoBox tone="down">{error.message}</InfoBox> : <BlockSkeleton n={3} h={100} />
      ) : (
        <div className="space-y-5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <FundStatusChip status={f.status} />
            <div className="flex gap-2">
              <Link href={`/social/masters/${f.masterId}`}>
                <Button size="sm" variant="surface">
                  {t("social.masterProfile")} <ArrowUpRight className="rtl:-scale-x-100" />
                </Button>
              </Link>
              {onInvest && f.status === "active" && (
                <Button size="sm" variant="ember" onClick={() => onInvest(f.id)}>
                  <Wallet /> {t("social.invest")}
                </Button>
              )}
            </div>
          </div>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            <Tile label={t("social.navPerUnit")}>{nav4(f.nav)}</Tile>
            <Tile label={t("social.returnAll")}>
              <span className={f.returnAll >= 0 ? "text-up" : "text-down"}>{pct(f.returnAll)}</span>
            </Tile>
            <Tile label={t("social.return1m")}>
              <span className={f.return1m >= 0 ? "text-up" : "text-down"}>{pct(f.return1m)}</span>
            </Tile>
            <Tile label={t("social.aum")}>{compactUsd(f.aum)}</Tile>
            <Tile label={t("social.investors")}>{fmt.number(f.investors, 0)}</Tile>
            <Tile label={t("social.drawdown")}>{f.drawdownPct > 0 ? `-${f.drawdownPct.toFixed(1)}%` : "0.0%"}</Tile>
          </div>
          <div>
            <div className="mb-1 text-[12.5px] font-medium text-fg-2">{t("social.navPerUnit")}</div>
            {series.length > 1 ? (
              <EquityChart data={series} height={240} showVolume={false} />
            ) : (
              <div className="k-row px-4 py-8 text-center text-[13px] text-fg-3">{t("social.funds.navEmpty")}</div>
            )}
          </div>
          <KeyValue
            rows={[
              [t("social.rollover"), t("social.funds.rolloverNext", { period: PERIOD_LABEL[f.period], next: serverTime(f.nextRolloverAt) })],
              [t("social.funds.lastRollover"), serverTime(f.lastRolloverAt)],
              [t("social.performanceFee"), t("social.invest.feeAboveHwm", { fee: f.perfFeePct })],
              [t("social.lockIn"), f.lockInDays ? t("social.invest.lockDays", { count: f.lockInDays }) : t("common.none")],
              [t("social.funds.minInvestment"), usd(f.minInvestment, 0)],
              [t("social.drawdownFreeze"), t("social.funds.freezeValue", { dd: f.maxDdPct, peak: nav4(f.navPeak) })],
              [t("social.funds.masterShare"), t("social.funds.masterShareValue", { pct: f.masterSharePct.toFixed(1), min: f.minOwnPct })],
              [t("social.created"), fmtDate(f.createdAt)],
            ]}
          />
          <div>
            <div className="mb-2 text-[12.5px] font-medium text-fg-2">{t("social.funds.rollovers")}</div>
            {d.rollovers.length ? (
              <DataTable columns={rollCols} rows={[...d.rollovers].sort((a, b) => b.at.localeCompare(a.at))} dense pageSize={10} rowKey={(r) => r.at} />
            ) : (
              <div className="k-row px-4 py-6 text-center text-[13px] text-fg-3">{t("social.funds.noRollovers")}</div>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */

const EXPLAIN = [
  { icon: <Coins />, t: "social.funds.explain.navT", s: "social.funds.explain.navS" },
  { icon: <CalendarClock />, t: "social.funds.explain.queueT", s: "social.funds.explain.queueS" },
  { icon: <TrendingUp />, t: "social.funds.explain.hwmT", s: "social.funds.explain.hwmS" },
  { icon: <ShieldAlert />, t: "social.invest.sl", s: "social.funds.explain.slS" },
  { icon: <Snowflake />, t: "social.drawdownFreeze", s: "social.funds.explain.freezeS" },
] as const;

/** a fund as a visiting card (ivory, founder 2026-10-10), its terms underneath */
function FundCard({ f, onInvest, onOpen }: { f: FundView; onInvest: () => void; onOpen: () => void }) {
  const t = useT();
  const fmt = useFormat();
  return (
    <div className="flex h-full flex-col">
      <button type="button" onClick={onOpen} className="block text-start transition-transform hover:-translate-y-0.5">
        <VisitingCard
          finish="ivory"
          kicker="PAMM"
          name={f.name}
          title={t("social.byName", { name: f.master.nickname })}
          stats={[
            { label: t("social.navPerUnit"), value: nav4(f.nav) },
            { label: "1M", value: pct(f.return1m, 1), tone: f.return1m > 0 ? "up" : f.return1m < 0 ? "down" : undefined },
            { label: t("social.aum"), value: compactUsd(f.aum) },
            { label: t("social.drawdown"), value: f.drawdownPct > 0 ? `-${f.drawdownPct.toFixed(1)}%` : "0.0%", tone: f.drawdownPct > 0 ? "down" : undefined },
          ]}
        />
      </button>
      <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 px-1 text-[12px] text-fg-3">
        <FundStatusChip status={f.status} />
        <span>
          {t("social.investors")} <span className="k-num font-medium text-fg">{fmt.number(f.investors, 0)}</span>
        </span>
        <span>
          {t("social.returnAll")} <span className={cn("k-num font-medium", f.returnAll >= 0 ? "text-up" : "text-down")}>{pct(f.returnAll, 1)}</span>
        </span>
        <span>
          {t("social.funds.perfFeeShort")} <span className="k-num font-medium text-fg">{f.perfFeePct}% HWM</span>
        </span>
        <span>
          {t("social.min")} <span className="k-num font-medium text-fg">{usd(f.minInvestment, 0)}</span>
        </span>
      </div>
      <div className="mt-2.5 flex flex-wrap gap-1.5 px-1">
        <Chip size="sm" tone="ember">
          <CalendarClock className="size-3" /> {PERIOD_LABEL[f.period]} · {serverTime(f.nextRolloverAt, false)}
        </Chip>
        {f.lockInDays > 0 ? (
          <Chip size="sm" tone="warn">
            <Lock className="size-3" /> {t("social.funds.lockInDays", { d: f.lockInDays })}
          </Chip>
        ) : (
          <Chip size="sm">{t("social.funds.noLockIn")}</Chip>
        )}
        <Chip size="sm">
          <Snowflake className="size-3" /> {t("social.funds.freezeChip", { dd: f.maxDdPct })}
        </Chip>
      </div>
      <div className="mt-auto grid grid-cols-2 gap-2 pt-4">
        <Button size="sm" variant="surface" className="w-full" onClick={onOpen}>
          {t("common.details")} <ArrowUpRight className="rtl:-scale-x-100" />
        </Button>
        <Button size="sm" variant="ember" onClick={onInvest} disabled={f.status !== "active"}>
          <Wallet /> {t("social.invest")}
        </Button>
      </div>
    </div>
  );
}

export function LivePammPage() {
  const t = useT();
  const fmt = useFormat();
  const { data, error, loading, reload } = useSocial<{ items: FundView[] }>("funds", 30000);
  const [view, setView] = React.useState<"cards" | "table">("cards");
  const [roll, setRoll] = React.useState<"all" | FeePeriod>("all");
  const [invest, setInvest] = React.useState<number | null>(null);
  const [open, setOpen] = React.useState<number | null>(null);

  const all = data?.items ?? [];
  const funds = all.filter((f) => roll === "all" || f.period === roll).sort((a, b) => b.aum - a.aum);
  const aum = all.reduce((s, f) => s + f.aum, 0);
  const investors = all.reduce((s, f) => s + f.investors, 0);
  const nextRoll = all
    .filter((f) => f.status === "active" && f.nextRolloverAt)
    .map((f) => f.nextRolloverAt!)
    .sort()[0];

  const columns: Column<FundView>[] = [
    { key: "fund", header: t("social.funds.col.fund"), cell: (f) => <MasterIdentity nickname={f.master.nickname} size={34} sub={f.name} />, width: "240px" },
    { key: "nav", header: t("social.funds.col.navUnit"), align: "right", cell: (f) => <span className="k-num font-medium">{nav4(f.nav)}</span>, sort: (f) => f.nav },
    { key: "ret", header: t("social.returnAll"), align: "right", cell: (f) => <span className={cn("k-num font-semibold", f.returnAll >= 0 ? "text-up" : "text-down")}>{pct(f.returnAll, 1)}</span>, sort: (f) => f.returnAll },
    { key: "aum", header: t("social.aum"), align: "right", cell: (f) => <span className="k-num">{compactUsd(f.aum)}</span>, sort: (f) => f.aum },
    { key: "inv", header: t("social.investors"), align: "right", cell: (f) => <span className="k-num text-fg-2">{f.investors}</span>, sort: (f) => f.investors, hideOn: "md" },
    { key: "roll", header: t("social.rollover"), cell: (f) => <span className="block"><span>{PERIOD_LABEL[f.period]}</span><span className="block text-[11px] text-fg-3">{serverTime(f.nextRolloverAt, false)}</span></span>, hideOn: "lg" },
    { key: "fee", header: t("social.fee"), align: "right", cell: (f) => <span className="k-num">{f.perfFeePct}%</span>, sort: (f) => f.perfFeePct },
    { key: "min", header: t("social.funds.col.minLock"), align: "right", cell: (f) => <span className="k-num text-fg-2">{usd(f.minInvestment, 0)} · {f.lockInDays ? t("social.age.days", { d: f.lockInDays }) : t("social.funds.none")}</span>, hideOn: "md" },
    { key: "st", header: t("common.status"), align: "center", cell: (f) => <FundStatusChip status={f.status} /> },
    {
      key: "act",
      header: "",
      align: "right",
      cell: (f) => (
        <Button
          size="xs"
          variant="ember"
          disabled={f.status !== "active"}
          onClick={(e) => {
            e.stopPropagation();
            setInvest(f.id);
          }}
        >
          {t("social.invest")}
        </Button>
      ),
    },
  ];

  return (
    <div className="pb-24">
      <PageHeader
        title={t("social.funds.title")}
        subtitle={t("social.funds.subtitle")}
        actions={
          <Link href="/social/investments">
            <Button variant="surface" size="lg">
              {t("social.myInvestments")} <ArrowUpRight className="rtl:-scale-x-100" />
            </Button>
          </Link>
        }
      />

      {error && !data ? (
        <SocialError onRetry={reload} message={error.message} />
      ) : (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <KpiCard label={t("social.funds.kpi.funds")} icon={<LayoutGrid />} value={<span className="k-num">{loading ? "—" : all.length}</span>} chip={t("social.funds.kpi.openCount", { count: all.filter((f) => f.status === "active").length })} />
            <KpiCard label={t("social.funds.kpi.totalAum")} icon={<Wallet />} value={<span className="k-num">{loading ? "—" : compactUsd(aum)}</span>} chip={t("social.funds.kpi.investorCapital")} delay={0.04} />
            <KpiCard label={t("social.investors")} icon={<Users />} value={<span className="k-num">{loading ? "—" : fmt.number(investors, 0)}</span>} chip={t("social.funds.kpi.acrossAll")} delay={0.08} />
            <KpiCard label={t("social.nextRollover")} icon={<CalendarClock />} value={<span className="text-[20px]">{nextRoll ? serverTime(nextRoll, false) : "—"}</span>} chip={t("social.serverTime")} chipTone="ember" delay={0.12} />
          </div>

          <Card className="mt-4 grid grid-cols-1 divide-y divide-line sm:grid-cols-2 sm:divide-y-0 lg:grid-cols-5 lg:divide-x">
            {EXPLAIN.map((x) => (
              <div key={x.t} className="flex gap-3 px-5 py-4">
                <span className="grid size-8 shrink-0 place-items-center rounded-full border border-gold/30 bg-gold-soft text-gold [&_svg]:size-4">{x.icon}</span>
                <div>
                  <div className="text-[13px] font-medium">{t(x.t)}</div>
                  <div className="mt-0.5 text-[11.5px] leading-snug text-fg-3">{t(x.s)}</div>
                </div>
              </div>
            ))}
          </Card>

          <div className="mt-6">
            <div className="mb-4 flex flex-wrap items-center gap-2">
              <h2 className="me-auto text-[18px] font-medium tracking-tight">
                {t("social.funds.count", { count: funds.length })} <span className="text-fg-3">{t("social.funds.sortedByAum")}</span>
              </h2>
              <Segmented
                size="xs"
                value={roll}
                onChange={setRoll}
                options={[
                  { value: "all", label: t("social.funds.anyRollover") },
                  { value: "daily", label: t("social.period.daily") },
                  { value: "weekly", label: t("social.period.weekly") },
                  { value: "monthly", label: t("social.period.monthly") },
                ]}
              />
              <Segmented
                size="xs"
                value={view}
                onChange={setView}
                options={[
                  { value: "cards", label: <LayoutGrid className="size-3.5" /> },
                  { value: "table", label: <Rows3 className="size-3.5" /> },
                ]}
              />
            </div>
            {loading ? (
              <BlockSkeleton n={2} h={200} />
            ) : funds.length === 0 ? (
              <Card>
                <EmptyState
                  illustration="bank"
                  art={all.length ? undefined : "pammFunds"}
                  title={all.length ? t("social.funds.empty.filteredTitle") : t("social.funds.empty.title")}
                  text={all.length ? t("social.funds.empty.filteredText") : t("social.funds.empty.text")}
                  action={
                    !all.length ? (
                      <Link href="/social/master">
                        <Button variant="ember">
                          <Crown /> {t("social.becomeMaster")}
                        </Button>
                      </Link>
                    ) : undefined
                  }
                />
              </Card>
            ) : view === "cards" ? (
              <div className="grid grid-cols-1 gap-x-5 gap-y-8 md:grid-cols-2 xl:grid-cols-3">
                {funds.map((f) => (
                  <FundCard key={f.id} f={f} onInvest={() => setInvest(f.id)} onOpen={() => setOpen(f.id)} />
                ))}
              </div>
            ) : (
              <Card>
                <CardHeader title={t("social.funds.allFunds")} subtitle={t("social.funds.allFundsSub")} />
                <div className="px-4 pb-5 pt-4 sm:px-6">
                  <DataTable columns={columns} rows={funds} rowKey={(f) => String(f.id)} onRowClick={(f) => setOpen(f.id)} search={(f) => `${f.name} ${f.master.nickname}`} />
                </div>
              </Card>
            )}
          </div>
        </>
      )}

      <p className="mt-6 text-[12px] leading-relaxed text-fg-3">
        {t("social.funds.disclaimer")}
      </p>

      <FundDetailDrawer
        fundId={open}
        onClose={() => setOpen(null)}
        onInvest={(id) => {
          setOpen(null);
          setInvest(id);
        }}
      />
      <InvestDialog fundId={invest} open={invest !== null} onOpenChange={(o) => !o && setInvest(null)} onDone={reload} />
    </div>
  );
}

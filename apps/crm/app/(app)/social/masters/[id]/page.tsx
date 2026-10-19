"use client";

import * as React from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { ArrowLeft, BadgeCheck, Bot, Clock, Copy as CopyIcon, Landmark, Lock, Share2, Star, Users } from "lucide-react";
import { toast } from "sonner";
import {
  Avatar,
  Button,
  CapsuleBars,
  Card,
  CardHeader,
  Chip,
  DataTable,
  Donut,
  EmptyState,
  EquityChart,
  Flag,
  Gauge,
  IconButton,
  KeyValue,
  Money,
  Reveal,
  Segmented,
  SymbolAvatar,
  SymbolCell,
  Tooltip,
  cn,
  formatCompact,
  type Column,
} from "@/components/kit";
import type { SeriesPoint } from "@/components/kit";
import {
  PAMM_FUNDS,
  SOCIAL_POLICY,
  masterById,
  masterEquity,
  masterGrowth,
  masterMonthly,
  masterTrades,
  riskLabel,
  type Master,
  type MasterTrade,
} from "@kalks/mock/social";
import { ProgramTags, RiskBadge, formatAge } from "@/components/social/master-bits";
import { CopyDialog } from "@/components/social/copy-dialog";
import { InvestDialog } from "@/components/social/invest-dialog";
import { IS_DEMO as DEMO_BUILD } from "@kalks/mock/mode";
import { LiveMasterProfilePage } from "@/components/social-live/master-profile";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const DONUT_COLORS = ["var(--k-ember)", "#e9b949", "#22c55e", "#38bdf8", "#a1a1aa"];
const RANGES = { "1M": 30, "3M": 90, "6M": 180, "1Y": 365, ALL: 99999 } as const;

function fmtD(iso: string) {
  const d = new Date(Date.parse(iso) + 3 * 3600000);
  return `${String(d.getUTCDate()).padStart(2, "0")} ${MONTHS[d.getUTCMonth()]} ${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
}

/* ------------------------------------------------------------------ */

function EquityCard({ m }: { m: Master }) {
  const all = React.useMemo(() => masterEquity(m), [m]);
  const [range, setRange] = React.useState<keyof typeof RANGES>("1Y");
  const data = React.useMemo(() => all.slice(-RANGES[range]), [all, range]);
  const [hover, setHover] = React.useState<SeriesPoint | null>(null);
  const onHover = React.useCallback((p: SeriesPoint | null) => setHover(p), []);
  const first = data[0]!.value;
  const shown = hover?.value ?? data[data.length - 1]!.value;
  const g = (shown / first - 1) * 100;
  return (
    <Card className="h-full">
      <div className="flex flex-col gap-4 px-6 pt-6 sm:flex-row sm:items-start sm:justify-between">
        <div>
          <div className="k-label">Growth of $10,000</div>
          <div className="mt-2 flex flex-wrap items-baseline gap-3">
            <Money value={shown} countUp={!hover} className="text-[32px] font-semibold tracking-tight" />
            <Chip tone={g >= 0 ? "up" : "down"}>
              {g >= 0 ? "+" : ""}
              {g.toFixed(2)}% · {range}
            </Chip>
          </div>
          <div className="mt-1 text-xs text-fg-3">{hover ? new Date(hover.time * 1000).toLocaleDateString("en-GB", { day: "2-digit", month: "short", year: "numeric" }) : "Net of commissions and swaps · bars show trade activity"}</div>
        </div>
        <Segmented size="xs" value={range} onChange={setRange} options={Object.keys(RANGES) as (keyof typeof RANGES)[]} />
      </div>
      <div className="px-3 pb-4 pt-2">
        <EquityChart data={data} height={480} onHover={onHover} />
      </div>
    </Card>
  );
}

function StatsCard({ m }: { m: Master }) {
  const stats: [string, React.ReactNode, string?][] = [
    ["Win rate", `${m.winRate.toFixed(1)}%`],
    ["Profit factor", m.profitFactor.toFixed(2)],
    ["Max drawdown", <span key="dd" className="text-down">-{m.maxDD.toFixed(1)}%</span>],
    ["Current DD", <span key="cdd" className={m.currentDD > 10 ? "text-down" : "text-fg"}>-{m.currentDD.toFixed(1)}%</span>],
    ["Avg. hold", m.avgHold],
    ["Trades / week", m.tradesPerWeek.toString()],
    ["Sharpe ratio", m.sharpe.toFixed(2)],
    ["Leverage", `1:${m.leverage}`],
  ];
  return (
    <Card className="flex h-full flex-col">
      <CardHeader title="Risk & statistics" subtitle="System score from volatility, DD and leverage" action={<RiskBadge risk={m.risk} showLabel />} />
      <div className="flex justify-center py-3">
        <Gauge value={m.risk} max={10} display={`${m.risk}/10`} label={`${riskLabel(m.risk)} risk`} size={190} />
      </div>
      <div className="grid grid-cols-2 gap-2 px-4 pb-5 sm:px-6">
        {stats.map(([k, v]) => (
          <div key={k} className="k-row px-3.5 py-2.5">
            <div className="text-[12px] text-fg-3">{k}</div>
            <div className="k-num mt-0.5 text-[14.5px] font-medium">{v}</div>
          </div>
        ))}
      </div>
    </Card>
  );
}

function MonthlyCard({ m }: { m: Master }) {
  const monthly = React.useMemo(() => masterMonthly(m), [m]);
  const years = Array.from(new Set(monthly.map((x) => x.year))).sort((a, b) => b - a);
  const cell = (y: number, mo: number) => monthly.find((x) => x.year === y && x.month === mo);
  return (
    <Card className="h-full">
      <CardHeader title="Monthly returns" subtitle="Compounded, in % · green profit, red loss" />
      <div className="overflow-x-auto px-4 pb-5 pt-4 sm:px-6">
        <table className="w-full min-w-[720px] border-separate border-spacing-1 text-[12px]">
          <thead>
            <tr>
              <th className="w-12 text-left text-[11px] font-medium text-fg-3" />
              {MONTHS.map((mo) => (
                <th key={mo} className="text-center text-[11px] font-medium text-fg-3">
                  {mo}
                </th>
              ))}
              <th className="text-right text-[11px] font-medium text-fg-3">Year</th>
            </tr>
          </thead>
          <tbody>
            {years.map((y) => {
              const yr = monthly.filter((x) => x.year === y).reduce((acc, x) => acc * (1 + x.ret / 100), 1);
              const ytd = (yr - 1) * 100;
              return (
                <tr key={y}>
                  <td className="k-num font-mono text-[12px] text-fg-2">{y}</td>
                  {MONTHS.map((_, mo) => {
                    const c = cell(y, mo);
                    if (!c) return <td key={mo} className="h-9 rounded-lg bg-surface-2/50" />;
                    const a = Math.min(1, Math.abs(c.ret) / 8);
                    const tone = c.ret >= 0 ? "var(--k-up)" : "var(--k-down)";
                    return (
                      <td key={mo} className="k-num h-9 rounded-lg text-center font-medium" style={{ background: `color-mix(in oklab, ${tone} ${Math.round(10 + a * 55)}%, transparent)`, color: a > 0.55 ? "#fff" : tone }}>
                        <Tooltip content={`${MONTHS[mo]} ${y}: ${c.ret >= 0 ? "+" : ""}${c.ret.toFixed(2)}%`}>
                          <span className="block">{c.ret >= 0 ? "+" : ""}{c.ret.toFixed(1)}</span>
                        </Tooltip>
                      </td>
                    );
                  })}
                  <td className={cn("k-num pl-2 text-right text-[13px] font-semibold", ytd >= 0 ? "text-up" : "text-down")}>
                    {ytd >= 0 ? "+" : ""}
                    {ytd.toFixed(1)}%
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <div className="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-4">
          {(() => {
            const rets = monthly.map((x) => x.ret);
            const best = monthly.reduce((a, b) => (b.ret > a.ret ? b : a));
            const worst = monthly.reduce((a, b) => (b.ret < a.ret ? b : a));
            return [
              ["Best month", `+${best.ret.toFixed(1)}%`, `${MONTHS[best.month]} ${best.year}`, "text-up"],
              ["Worst month", `${worst.ret.toFixed(1)}%`, `${MONTHS[worst.month]} ${worst.year}`, "text-down"],
              ["Positive months", `${Math.round((rets.filter((r) => r > 0).length / rets.length) * 100)}%`, `${rets.filter((r) => r > 0).length} of ${rets.length}`, "text-fg"],
              ["Avg. month", `${(rets.reduce((a, b) => a + b, 0) / rets.length).toFixed(2)}%`, "arithmetic mean", "text-fg"],
            ].map(([k, v, sub, cls]) => (
              <div key={k} className="k-row px-3.5 py-2.5">
                <div className="text-[12px] text-fg-3">{k}</div>
                <div className={cn("k-num mt-0.5 text-[16px] font-semibold", cls)}>{v}</div>
                <div className="text-[11px] text-fg-3">{sub}</div>
              </div>
            ));
          })()}
        </div>
      </div>
    </Card>
  );
}

function InstrumentsCard({ m }: { m: Master }) {
  return (
    <Card className="h-full">
      <CardHeader title="Instruments" subtitle="Share of traded volume · 90 days" />
      <div className="flex flex-col items-center gap-5 px-6 pb-6 pt-4">
        <Donut
          data={m.instruments.map((i, k) => ({ label: i.label, value: i.value, color: DONUT_COLORS[k % DONUT_COLORS.length] }))}
          size={170}
          thickness={20}
          center={
            <div>
              <div className="k-num text-[22px] font-semibold">{m.instruments.length}</div>
              <div className="text-[11px] text-fg-3">symbols</div>
            </div>
          }
        />
        <div className="w-full space-y-2">
          {m.instruments.map((i, k) => (
            <div key={i.label} className="k-row flex items-center gap-3 px-3.5 py-2">
              <span className="size-2.5 rounded-full" style={{ background: DONUT_COLORS[k % DONUT_COLORS.length] }} />
              <SymbolAvatar symbol={i.label} size={20} />
              <span className="flex-1 text-[13px] font-medium">{i.label}</span>
              <span className="k-num text-[13px] text-fg-2">{i.value}%</span>
            </div>
          ))}
        </div>
      </div>
    </Card>
  );
}

function FeesCard({ m }: { m: Master }) {
  const fund = PAMM_FUNDS.find((f) => f.masterId === m.id);
  return (
    <Card className="h-full">
      <CardHeader title="Fee terms" subtitle="Charged only on new profits" />
      <div className="px-6 pt-3">
        <div className="flex items-baseline gap-2">
          <span className="k-num text-[34px] font-semibold text-gold">{m.perfFee}%</span>
          <span className="text-[13px] text-fg-2">performance fee</span>
        </div>
      </div>
      <div className="px-6 pb-2">
        <KeyValue
          rows={[
            ["High-water mark", <Chip key="h" size="sm" tone="up">Yes</Chip>],
            ["Fee settlement", <span key="r" className="capitalize">{m.rollover}</span>],
            ["Minimum investment", `$${m.minInvestment.toLocaleString()}`],
            ["Lock-in (PAMM)", m.program === "copy" ? "—" : m.lockInDays ? <span key="l" className="inline-flex items-center gap-1"><Lock className="size-3.5 text-warn" />{m.lockInDays} days</span> : "None"],
            ["PAMM rollover", fund ? <span key="ro" className="capitalize">{fund.rollover} · {fund.nextRolloverLabel}</span> : "—"],
            ["Fund DD freeze", fund ? `-${fund.ddFreeze}%` : "—"],
            ["Master's own capital", `${m.ownCapitalPct}%`],
          ]}
        />
      </div>
      <p className="px-6 pb-6 text-[12px] leading-relaxed text-fg-3">{SOCIAL_POLICY.feeSettlement}</p>
    </Card>
  );
}

function GrowthCard({ m }: { m: Master }) {
  const data = React.useMemo(() => masterGrowth(m).slice(-12), [m]);
  const [k, setK] = React.useState<"aum" | "followers">("aum");
  return (
    <Card className="h-full">
      <CardHeader
        title={k === "aum" ? "Assets under management" : "Followers & investors"}
        subtitle="Last 12 weeks"
        action={
          <Segmented
            size="xs"
            value={k}
            onChange={setK}
            options={[
              { value: "aum", label: "AUM" },
              { value: "followers", label: "Followers" },
            ]}
          />
        }
      />
      <div className="px-6 pt-3">
        <div className="k-num text-[24px] font-semibold">{k === "aum" ? `$${formatCompact(m.aum)}` : m.followers.toLocaleString()}</div>
        <div className="text-[12px] text-up">
          +{(((data[data.length - 1]![k] - data[0]![k]) / data[0]![k]) * 100).toFixed(1)}% in 12 weeks
        </div>
      </div>
      <div className="px-4 pb-5 pt-6 sm:px-6">
        <CapsuleBars data={data.map((d) => ({ label: d.label.slice(0, 2), value: d[k] }))} height={180} format={(v) => (k === "aum" ? `$${formatCompact(v)}` : Math.round(v).toLocaleString())} className="gap-1.5" />
      </div>
    </Card>
  );
}

function TradesCard({ m }: { m: Master }) {
  const trades = React.useMemo(() => masterTrades(m, 30), [m]);
  const columns: Column<MasterTrade>[] = [
    { key: "sym", header: "Symbol", cell: (t) => <SymbolCell symbol={t.symbol} size={24} sub={<span className="font-mono">#{t.ticket}</span>} /> },
    { key: "side", header: "Side", cell: (t) => <Chip size="sm" tone={t.side === "buy" ? "up" : "down"}>{t.side.toUpperCase()}</Chip> },
    { key: "lots", header: "Lots", align: "right", cell: (t) => <span className="k-num">{t.lots.toFixed(2)}</span>, hideOn: "sm" },
    { key: "px", header: "Open → close", align: "right", cell: (t) => <span className="k-num font-mono text-[12px] text-fg-2">{t.openPrice} → {t.closePrice}</span>, hideOn: "md" },
    { key: "time", header: "Closed", align: "right", cell: (t) => <span className="k-num text-fg-2">{fmtD(t.closeTime)}</span>, sort: (t) => t.closeTime },
    { key: "pips", header: "Pips", align: "right", cell: (t) => <span className={cn("k-num", t.pips >= 0 ? "text-up" : "text-down")}>{t.pips >= 0 ? "+" : ""}{t.pips.toFixed(1)}</span>, sort: (t) => t.pips },
    { key: "pct", header: "Result", align: "right", cell: (t) => <span className={cn("k-num font-semibold", t.profitPct >= 0 ? "text-up" : "text-down")}>{t.profitPct >= 0 ? "+" : ""}{t.profitPct.toFixed(2)}%</span>, sort: (t) => t.profitPct },
  ];
  return (
    <Card className="h-full">
      <CardHeader
        title="Trade history"
        subtitle="Closed trades, results as % of equity"
        action={
          <Chip tone="warn">
            <Clock className="size-3" /> {SOCIAL_POLICY.tradeDelayHours}h delay
          </Chip>
        }
      />
      <div className="px-4 pb-5 pt-4 sm:px-6">
        <DataTable columns={columns} rows={trades} pageSize={12} dense rowKey={(t) => t.ticket} />
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function DemoMasterProfilePage() {
  const { id } = useParams<{ id: string }>();
  const m = masterById(id);
  const [copy, setCopy] = React.useState(false);
  const [invest, setInvest] = React.useState(false);
  const [fav, setFav] = React.useState(false);
  if (!m)
    return (
      <Card className="mt-10">
        <EmptyState
          illustration="magnifying_glass_tilted_left"
          title="Master not found"
          text="This profile may have been unpublished or the link is incorrect."
          action={
            <Link href="/social">
              <Button variant="ember">Back to Discover</Button>
            </Link>
          }
        />
      </Card>
    );
  const fund = PAMM_FUNDS.find((f) => f.masterId === m.id) ?? null;
  const sinceD = new Date(Date.parse("2026-09-24") - m.ageDays * 86400000);
  const since = `${MONTHS[sinceD.getUTCMonth()]} ${sinceD.getUTCFullYear()}`;

  return (
    <div className="pb-24">
      <Link href="/social" className="mb-4 inline-flex items-center gap-1.5 text-[13px] text-fg-2 hover:text-fg">
        <ArrowLeft className="size-4" /> Discover
      </Link>

      <Reveal>
        <Card className="relative overflow-hidden">
          <div className="absolute -left-24 -top-24 size-72 rounded-full bg-ember/15 blur-3xl" />
          <div className="relative flex flex-col gap-6 p-6 sm:p-7 lg:flex-row lg:items-start">
            <div className="relative shrink-0 self-start">
              <Avatar src={m.person.photo} name={m.person.name} size={104} className="[&_img]:ring-2 [&_img]:ring-ember/40" />
              {m.verified && (
                <span className="absolute bottom-1 right-1 grid size-7 place-items-center rounded-full bg-surface ring-2 ring-surface">
                  <BadgeCheck className="size-6 text-ember" />
                </span>
              )}
            </div>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2.5">
                <h1 className="text-[28px] font-medium leading-tight tracking-tight">{m.person.name}</h1>
                <span className="flex items-center gap-1.5 text-[13px] text-fg-2">
                  <Flag country={m.person.country} className="size-4" /> {m.person.countryName}
                </span>
                {m.verified && (
                  <Chip tone="up" size="sm">
                    <BadgeCheck className="size-3" /> Verified master
                  </Chip>
                )}
              </div>
              <div className="mt-1 text-[16px] text-fg">{m.strategy}</div>
              <p className="mt-3 max-w-3xl text-[14px] leading-relaxed text-fg-2">{m.description}</p>
              <div className="mt-4 flex flex-wrap items-center gap-2">
                <ProgramTags program={m.program} size="md" />
                {m.api && (
                  <Chip>
                    <Bot className="size-3.5" /> API / algo
                  </Chip>
                )}
                {m.tags.map((t) => (
                  <Chip key={t}>{t}</Chip>
                ))}
              </div>
              <div className="mt-4 flex flex-wrap gap-x-5 gap-y-1 text-[12.5px] text-fg-3">
                <span>Trading since {since} · {formatAge(m.ageDays)}</span>
                <span>
                  Account <span className="font-mono text-fg-2">#{m.account}</span> · Kalks-Live01
                </span>
                <span className="flex items-center gap-1">
                  <Users className="size-3.5" /> {m.followers.toLocaleString()} followers
                </span>
              </div>
            </div>
            <div className="flex w-full shrink-0 flex-col gap-4 lg:w-[300px]">
              <div className="grid grid-cols-3 gap-2">
                {[
                  ["1Y", m.return1y],
                  ["3M", m.return3m],
                  ["All", m.returnAll],
                ].map(([k, v]) => (
                  <div key={k as string} className="k-row px-3 py-2.5 text-center">
                    <div className="text-[11px] text-fg-3">Return {k}</div>
                    <div className={cn("k-num text-[16px] font-semibold", (v as number) >= 0 ? "text-up" : "text-down")}>
                      {(v as number) >= 0 ? "+" : ""}
                      {(v as number).toFixed(1)}%
                    </div>
                  </div>
                ))}
              </div>
              <div className="flex gap-2">
                {m.program !== "pamm" && (
                  <Button variant="ember" size="lg" className="flex-1" shimmer onClick={() => setCopy(true)}>
                    <CopyIcon /> Copy
                  </Button>
                )}
                {fund && (
                  <Button variant="gold" size="lg" className="flex-1" onClick={() => setInvest(true)}>
                    <Landmark /> Invest (PAMM)
                  </Button>
                )}
              </div>
              <div className="flex items-center justify-between text-[12px] text-fg-3">
                <span>AUM ${formatCompact(m.aum)} · min ${m.minInvestment}</span>
                <div className="flex gap-1.5">
                  <IconButton
                    size="sm"
                    aria-label="Share profile"
                    onClick={() => {
                      navigator.clipboard?.writeText(`https://app.kalkstrade.com/social/masters/${m.id}`).catch(() => {});
                      toast.success("Profile link copied");
                    }}
                  >
                    <Share2 />
                  </IconButton>
                  <IconButton
                    size="sm"
                    active={fav}
                    aria-label="Watchlist"
                    onClick={() => {
                      setFav(!fav);
                      toast.success(fav ? "Removed from watchlist" : "Added to watchlist", { description: fav ? undefined : "You'll be notified about big drawdowns and fee changes." });
                    }}
                  >
                    <Star className={cn(fav && "fill-current")} />
                  </IconButton>
                </div>
              </div>
            </div>
          </div>
        </Card>
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <EquityCard m={m} />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-4">
          <StatsCard m={m} />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <MonthlyCard m={m} />
        </Reveal>
        <Reveal delay={0.1} className="xl:col-span-4">
          <InstrumentsCard m={m} />
        </Reveal>
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-8">
          <TradesCard m={m} />
        </Reveal>
        <div className="flex flex-col gap-4 xl:col-span-4">
          <Reveal delay={0.1}>
            <FeesCard m={m} />
          </Reveal>
          <Reveal delay={0.15}>
            <GrowthCard m={m} />
          </Reveal>
        </div>
      </div>

      <CopyDialog master={m} open={copy} onOpenChange={setCopy} />
      <InvestDialog fund={fund} open={invest} onOpenChange={setInvest} />
    </div>
  );
}

export default function MasterProfilePage() {
  return DEMO_BUILD ? <DemoMasterProfilePage /> : <LiveMasterProfilePage />;
}

"use client";

import * as React from "react";
import Link from "next/link";
import { CalendarClock, Coins, Gauge as GaugeIcon, Layers, Scale, SlidersHorizontal, Users } from "lucide-react";
import { buttonVariants, Card, CardHeader, Chip, DataTable, EmptyState, KpiCard, PageHeader, Reveal, Segmented, Skeleton, SymbolCell, cn, formatNumber, type Column } from "@kalks/ui";
import { PnlText, usdCompact } from "@/components/command/kit";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { ago, useNow } from "@/components/live/kit";
import { groupLabel, useExposure, useLiveDirectory, type ExposureSymbol } from "@/lib/trading-desk";
import { LevelChip, LoginCell, RiskError, UsageBar } from "./risk-kit";

type BookView = "all" | "A" | "B";

/** Client net of a symbol for the chosen book (+ = clients net long). */
const netOf = (r: ExposureSymbol, book: BookView, view: "usd" | "lots") => (book === "all" ? (view === "usd" ? r.netUsd : r.netLots) : view === "usd" ? r.books[book].netUsd : r.books[book].netLots);

/** Live exposure: the engine's server-side exposure per symbol with limit usage, house P&L and top clients
 *  (dealing stream `exposure` frame every 5 s, REST poll while the stream is offline). */
export function LiveExposurePage() {
  const { exposure: e, error, loading } = useExposure();
  useLiveDirectory();
  const now = useNow(5000);
  const [book, setBook] = React.useState<BookView>("B");
  const [view, setView] = React.useState<"usd" | "lots">("usd");
  const [sel, setSel] = React.useState<string | null>(null);

  const rows = React.useMemo(() => (e?.symbols ?? []).filter((r) => book === "all" || r.books[book].netLots !== 0 || r.books[book].netUsd !== 0), [e, book]);
  const max = Math.max(1, ...rows.map((r) => Math.abs(netOf(r, book, view))));
  const selected = rows.find((r) => r.symbol === sel) ?? rows[0] ?? null;
  const groups = e?.groups ?? [];
  const gmax = Math.max(1, ...groups.map((g) => Math.abs(g.netUsd)));
  const breached = rows.filter((r) => r.limit && r.limit.level !== "ok");

  const cols: Column<ExposureSymbol>[] = [
    { key: "s", header: "Symbol", cell: (r) => <SymbolCell symbol={r.symbol} size={24} sub={<span className="text-[11px] text-fg-3">{r.positions} pos · {r.accounts} accounts</span>} />, sort: (r) => r.symbol },
    { key: "b", header: "Long lots", align: "right", hideOn: "md", cell: (r) => <span className="k-num font-mono text-[12px] text-up">{formatNumber(r.longLots, 2)}</span>, sort: (r) => r.longLots },
    { key: "se", header: "Short lots", align: "right", hideOn: "md", cell: (r) => <span className="k-num font-mono text-[12px] text-down">{formatNumber(r.shortLots, 2)}</span>, sort: (r) => r.shortLots },
    {
      key: "n",
      header: view === "usd" ? "Client net (USD)" : "Client net (lots)",
      cell: (r) => {
        const v = netOf(r, book, view);
        const w = (Math.abs(v) / max) * 50;
        return (
          <span className="flex items-center gap-2">
            <span className="relative h-2 w-28 rounded-full bg-surface-3 sm:w-36">
              <span className="absolute inset-y-0 left-1/2 w-px bg-line" />
              <span className={cn("absolute inset-y-0 rounded-full", v >= 0 ? "left-1/2 bg-up" : "right-1/2 bg-down")} style={{ width: `${w}%` }} />
            </span>
            <span className={cn("k-num w-20 text-right font-mono text-[12px]", v >= 0 ? "text-up" : "text-down")}>{view === "usd" ? usdCompact(v, 1) : formatNumber(v, 2)}</span>
          </span>
        );
      },
      sort: (r) => netOf(r, book, view),
      csv: (r) => (view === "usd" ? netOf(r, book, view).toFixed(0) : netOf(r, book, view).toFixed(2)),
    },
    { key: "bb", header: "House B-book net", align: "right", cell: (r) => <span className="k-num font-mono text-[12px] text-fg-2">{usdCompact(-r.books.B.netUsd, 1)}</span>, sort: (r) => -r.books.B.netUsd, csv: (r) => (-r.books.B.netUsd).toFixed(0) },
    {
      key: "lim",
      header: "Limit usage",
      cell: (r) =>
        r.limit ? (
          <span className="flex items-center gap-2 whitespace-nowrap">
            <UsageBar usedPct={r.limit.usedPct} level={r.limit.level} />
            <span className="text-[11px] text-fg-3">of {usdCompact(r.limit.capUsd, 1)}</span>
            {r.limit.level !== "ok" && <LevelChip level={r.limit.level} usedPct={r.limit.usedPct} />}
            {r.limit.mode === "enforce" && <Chip size="sm" tone="down">Enforce</Chip>}
          </span>
        ) : (
          <span className="text-[11.5px] text-fg-3">No cap</span>
        ),
      sort: (r) => r.limit?.usedPct ?? -1,
      csv: (r) => (r.limit ? `${r.limit.usedPct.toFixed(0)}% of ${r.limit.capUsd}` : ""),
    },
    { key: "aa", header: "A-book net", align: "right", hideOn: "xl", cell: (r) => <span className="k-num font-mono text-[12px] text-info">{usdCompact(r.books.A.netUsd, 1)}</span>, sort: (r) => r.books.A.netUsd },
    { key: "cp", header: "Client floating", align: "right", hideOn: "lg", cell: (r) => <PnlText value={r.clientFloatingUsd} className="font-mono text-[12px]" />, sort: (r) => r.clientFloatingUsd, csv: (r) => r.clientFloatingUsd.toFixed(2) },
    { key: "bp", header: "House floating", align: "right", cell: (r) => <PnlText value={r.houseFloatingUsd} className="font-mono text-[12px]" />, sort: (r) => r.houseFloatingUsd, csv: (r) => r.houseFloatingUsd.toFixed(2) },
  ];

  const t = e?.totals;
  return (
    <div className="pb-10">
      <PageHeader
        title="Exposure"
        subtitle={
          <span className="inline-flex flex-wrap items-center gap-2">
            Net client exposure per symbol, computed by the trading engine with limit usage. The house holds the opposite side of the B-book. <DeskStatusChip />
            {e && <Chip size="sm" tone="neutral">Updated {ago(e.generatedAt, now)}</Chip>}
          </span>
        }
        actions={
          <div className="flex flex-wrap gap-2">
            <Segmented size="md" value={book} onChange={setBook} options={[{ value: "B", label: "B-book" }, { value: "A", label: "A-book" }, { value: "all", label: "A + B" }]} />
            <Segmented size="md" value={view} onChange={setView} options={[{ value: "usd", label: "Net USD" }, { value: "lots", label: "Net lots" }]} />
            <Link href="/trading/risk-limits" className={buttonVariants({ variant: "surface", size: "md" })}>
              <SlidersHorizontal /> Risk limits
            </Link>
          </div>
        }
      />
      {error ? (
        <Card>
          <RiskError error={error} />
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
            <KpiCard
              label="Gross exposure"
              icon={<Scale />}
              value={t ? <span className="k-num">{usdCompact(t.grossUsd, 1)}</span> : <Skeleton className="h-8 w-24" />}
              chip={t ? `Long ${usdCompact(t.longUsd, 1)} · short ${usdCompact(t.shortUsd, 1)}` : "Loading…"}
            />
            <KpiCard label="House floating (B)" icon={<GaugeIcon />} value={t ? <PnlText value={t.houseFloatingUsd} /> : <Skeleton className="h-8 w-24" />} chip={t ? `Client floating ${usdCompact(t.clientFloatingUsd, 1)}` : "B-book, house view"} chipTone="ember" delay={0.04} />
            <KpiCard
              label="Realised today (house)"
              icon={<Coins />}
              value={t ? <PnlText value={t.realisedToday.houseUsd} /> : <Skeleton className="h-8 w-24" />}
              chip={t ? `Clients ${usdCompact(t.realisedToday.clientUsd, 1)} · A ${usdCompact(t.realisedToday.byBook.A, 1)} · B ${usdCompact(t.realisedToday.byBook.B, 1)}` : "Since server day start"}
              delay={0.08}
            />
            <KpiCard
              label="House daily P&L"
              icon={<CalendarClock />}
              value={t ? <PnlText value={t.houseDailyPnlUsd} /> : <Skeleton className="h-8 w-24" />}
              hot
              chip={t ? (t.baselineAt ? `Baseline ${ago(t.baselineAt, now)}` : "Since server day start") : "B-book"}
              chipTone="ember"
              delay={0.12}
            />
          </div>
          {breached.length > 0 && (
            <Reveal delay={0.04}>
              <div className={cn("mt-4 flex flex-wrap items-center gap-2 rounded-[16px] border px-4 py-3 text-[12.5px]", breached.some((r) => r.limit!.level === "red") ? "border-down/30 bg-down-soft" : "border-warn/30 bg-warn-soft")}>
                <span className="font-medium">Limit usage:</span>
                {breached.map((r) => (
                  <button key={r.symbol} type="button" onClick={() => setSel(r.symbol)} className="inline-flex items-center gap-1.5">
                    <span className="font-mono">{r.symbol}</span> <LevelChip level={r.limit!.level} usedPct={r.limit!.usedPct} />
                  </button>
                ))}
                <Link href="/trading/risk-limits" className="ml-auto text-fg-2 hover:text-fg">
                  Edit limits
                </Link>
              </div>
            </Reveal>
          )}
          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Reveal delay={0.06} className="xl:col-span-8">
              <Card className="px-4 py-5 sm:px-6">
                {loading ? (
                  <div className="space-y-2">
                    {Array.from({ length: 6 }).map((_, i) => (
                      <Skeleton key={i} className="h-12 w-full" />
                    ))}
                  </div>
                ) : (
                  <DataTable
                    columns={cols}
                    rows={rows}
                    dense
                    pageSize={20}
                    rowKey={(r) => r.symbol}
                    exportName="exposure"
                    onRowClick={(r) => setSel(r.symbol)}
                    empty={<EmptyState title="No open exposure" text="Net exposure appears as soon as positions are open on this book." illustration="chart_increasing" />}
                  />
                )}
              </Card>
            </Reveal>
            <div className="space-y-4 xl:col-span-4">
              <Reveal delay={0.08}>
                <Card>
                  <CardHeader title={selected ? `Top clients · ${selected.symbol}` : "Top clients"} subtitle="Largest client net per symbol · click a symbol row" icon={<Users />} />
                  <div className="space-y-1.5 px-4 pb-5 pt-4 sm:px-6">
                    {!selected && <div className="text-[12.5px] text-fg-3">No open positions.</div>}
                    {selected?.topClients.map((c) => (
                      <Link key={c.login} href={`/trading/accounts?login=${c.login}`} className="k-row flex items-center gap-3 px-3 py-2 hover:border-fg-3">
                        <span className="min-w-0 flex-1">
                          <LoginCell login={c.login} userId={c.userId} sub={`${c.netLots >= 0 ? "+" : ""}${formatNumber(c.netLots, 2)} lots`} />
                        </span>
                        <span className="text-right">
                          <span className={cn("k-num block font-mono text-[12px]", c.netUsd >= 0 ? "text-up" : "text-down")}>{usdCompact(c.netUsd, 1)}</span>
                          <PnlText value={c.floatingUsd} className="block font-mono text-[11px]" />
                        </span>
                      </Link>
                    ))}
                    {selected && !selected.topClients.length && <div className="text-[12.5px] text-fg-3">No client positions on this symbol.</div>}
                  </div>
                </Card>
              </Reveal>
              <Reveal delay={0.1}>
                <Card>
                  <CardHeader title="By group" subtitle="Client net USD per account group" icon={<Layers />} />
                  <div className="space-y-2.5 px-6 pb-6 pt-4">
                    {groups.length === 0 && <div className="text-[12.5px] text-fg-3">No open positions.</div>}
                    {groups.map((g) => (
                      <div key={g.group} className="text-[12.5px]">
                        <div className="flex items-center justify-between">
                          <span>{groupLabel(g.group)}</span>
                          <span className={cn("k-num font-mono", g.netUsd >= 0 ? "text-up" : "text-down")}>{usdCompact(g.netUsd, 1)}</span>
                        </div>
                        <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-3">
                          <div className={cn("h-full rounded-full", g.netUsd >= 0 ? "bg-up" : "bg-down")} style={{ width: `${(Math.abs(g.netUsd) / gmax) * 100}%` }} />
                        </div>
                        <div className="mt-0.5 text-[11px] text-fg-3">
                          {g.positions} positions · {formatNumber(g.lots, 2)} lots
                        </div>
                      </div>
                    ))}
                    <div className="pt-2">
                      <Link href="/trading/risk-limits" className="inline-flex items-center gap-1.5 text-[12.5px] text-fg-2 hover:text-ember">
                        <SlidersHorizontal className="size-3.5" /> Exposure caps and alerts: Risk limits
                      </Link>
                    </div>
                  </div>
                </Card>
              </Reveal>
            </div>
          </div>
        </>
      )}
    </div>
  );
}

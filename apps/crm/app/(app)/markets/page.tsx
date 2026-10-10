"use client";

import * as React from "react";
import Link from "next/link";
import { toast } from "sonner";
import { ArrowUpRight, CandlestickChart, Search, Star, X } from "lucide-react";
import { Button, Card, CardHeader, Chip, DataTable, Delta, EmptyState, PageHeader, PriceText, Reveal, Segmented, Sparkline, SymbolCell, Tooltip, cn, formatNumber, useFeedMode, useQuotes, type Column } from "@/components/kit";
import { ASSET_CLASS_LABEL, INSTRUMENTS, IS_DEMO, fetchCandles, isMarketOpen, sparkline, type AssetClass, type Instrument } from "@kalks/mock";
import { CONTRACT_SPECS, DEFAULT_FAVOURITES } from "@kalks/mock/markets-extra";
import { InstrumentDrawer } from "@/components/markets/instrument-drawer";
import { FeedGuard } from "@/components/feed-guard";
import { useT } from "@kalks/i18n/react";
import { TERMINAL_URL } from "@/lib/live";
import { PageHero } from "@/components/page-hero";

type Tab = "all" | "fav" | AssetClass;
const CLASSES: AssetClass[] = ["forex", "metals", "indices", "energies", "crypto", "stocks"];

function pipSize(i: Instrument) {
  return i.digits === 5 || i.digits === 3 ? Math.pow(10, -(i.digits - 1)) : Math.pow(10, -i.digits);
}

/** FX spreads in pips; everything else in price units. */
function spreadText(i: Instrument, bid: number, ask: number) {
  return i.assetClass === "forex" ? `${((ask - bid) / pipSize(i)).toFixed(1)}` : formatNumber(ask - bid, i.digits);
}

/** Seeded 7-day series whose direction agrees with today's change. */
function weekLine(i: Instrument) {
  const d = sparkline(i.symbol + "7d", 42, (i.change / 100 / 42) * 3);
  const up = d[d.length - 1]! >= d[0]!;
  if (up === i.change >= 0) return d;
  const m = (Math.max(...d) + Math.min(...d)) / 2;
  return d.map((v) => 2 * m - v);
}

/**
 * Closes of the last 7 calendar days (H1 bars) from the market-data service. A fixed bar count would
 * cover ~9 days of FX and a month of US stocks, so the window is cut by time instead.
 */
const weekCache = new Map<string, Promise<number[] | null>>();
function useWeekCloses(symbol: string): number[] | null {
  const mode = useFeedMode();
  const [v, setV] = React.useState<{ symbol: string; closes: number[] | null } | null>(null);
  React.useEffect(() => {
    if (mode !== "live") return;
    let alive = true;
    let p = weekCache.get(symbol);
    if (!p) {
      p = fetchCandles(symbol, "H1", 180).then((bars) => {
        if (!bars) return null;
        const from = Date.now() / 1000 - 7 * 86400;
        const week = bars.filter((b) => b.time >= from).map((b) => b.close);
        return week.length > 1 ? week : null;
      });
      weekCache.set(symbol, p);
      void p.then((c) => {
        if (!c) weekCache.delete(symbol); // don't remember a failure
      });
    }
    void p.then((closes) => alive && setV({ symbol, closes }));
    return () => {
      alive = false;
    };
  }, [symbol, mode]);
  return v && v.symbol === symbol ? v.closes : null;
}

/** 7-day trend: real H1 closes of the last 7 days (the seeded line only when the service is offline). */
function WeekLine({ inst }: { inst: Instrument }) {
  const real = useWeekCloses(inst.symbol);
  const mode = useFeedMode();
  const data = React.useMemo(() => real ?? (mode === "live" ? null : weekLine(inst)), [real, inst, mode]);
  return data ? <Sparkline data={data} width={96} height={28} /> : <div style={{ width: 96, height: 28 }} />;
}

/* ------------------------------------------------------------------ */

function HeatTile({ inst, change, onClick }: { inst: Instrument; change: number; onClick: () => void }) {
  const a = Math.min(1, Math.abs(change) / 3);
  const tone = change >= 0 ? "var(--k-up)" : "var(--k-down)";
  const open = isMarketOpen(inst.symbol);
  const t = useT();
  return (
    <button
      onClick={onClick}
      title={open ? t("news.markets.tile.open", { symbol: inst.symbol }) : t("news.markets.tile.closed", { symbol: inst.symbol })}
      className="group relative overflow-hidden rounded-[14px] border border-line px-3 py-2.5 text-start transition-transform hover:-translate-y-0.5 hover:border-[var(--k-border-top)]"
      style={{ background: `color-mix(in oklab, ${tone} ${Math.round(8 + a * 52)}%, var(--k-surface-2))` }}
    >
      <div className="flex items-center gap-1.5">
        <span className="truncate text-[12.5px] font-semibold text-fg">{inst.symbol}</span>
        <span aria-label={open ? t("news.markets.marketOpen") : t("news.markets.marketClosed")} className={cn("size-1.5 shrink-0 rounded-full", open ? "bg-up" : "border border-fg-3")} />
      </div>
      <div className={cn("k-num mt-0.5 text-[12px] font-medium", a > 0.55 ? "text-fg" : change >= 0 ? "text-up" : "text-down")}>
        {change >= 0 ? "+" : ""}
        {change.toFixed(2)}%
      </div>
    </button>
  );
}

export default function MarketsPage() {
  // symbols without a live price (no provider) are dropped from INSTRUMENTS once the feed is live
  const qs = useQuotes(INSTRUMENTS.map((i) => i.symbol));
  // INSTRUMENTS shrinks when the feed goes live (symbols without a price are dropped): recompute lists then
  const mode = useFeedMode();
  const [tab, setTab] = React.useState<Tab>("all");
  const [q, setQ] = React.useState("");
  const [favs, setFavs] = React.useState<string[]>(DEFAULT_FAVOURITES);
  const [open, setOpen] = React.useState<string | null>(null);
  const t = useT();

  const toggleFav = (s: string) => {
    const on = favs.includes(s);
    toast.success(on ? t("news.markets.favRemoved", { symbol: s }) : t("news.markets.favAdded", { symbol: s }));
    setFavs((f) => (on ? f.filter((x) => x !== s) : [...f, s]));
  };

  const rows = React.useMemo(() => {
    const qq = q.trim().toLowerCase();
    return INSTRUMENTS.filter(
      (i) => (tab === "all" || (tab === "fav" ? favs.includes(i.symbol) : i.assetClass === tab)) && (!qq || i.symbol.toLowerCase().includes(qq) || i.name.toLowerCase().includes(qq)),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab, q, favs, mode]);

  // heatmap: today's live move, best to worst
  const sorted = [...INSTRUMENTS].sort((a, b) => (qs[b.symbol]?.change ?? 0) - (qs[a.symbol]?.change ?? 0));
  const breadth = INSTRUMENTS.filter((i) => (qs[i.symbol]?.change ?? 0) >= 0).length;

  const cols: Column<Instrument>[] = [
    {
      key: "fav",
      header: "",
      width: "44px",
      cell: (i) => (
        <button
          onClick={(e) => {
            e.stopPropagation();
            toggleFav(i.symbol);
          }}
          aria-label={t("news.markets.favourite")}
          className={cn("grid size-7 place-items-center rounded-full transition-colors hover:bg-surface-3", favs.includes(i.symbol) ? "text-gold" : "text-fg-3")}
        >
          <Star className={cn("size-4", favs.includes(i.symbol) && "fill-current")} />
        </button>
      ),
    },
    { key: "symbol", header: t("news.markets.col.instrument"), cell: (i) => <SymbolCell symbol={i.symbol} size={28} />, sort: (i) => i.symbol },
    { key: "bid", header: t("news.markets.col.bid"), align: "right", cell: (i) => <PriceText symbol={i.symbol} value={qs[i.symbol]!.bid} dir={qs[i.symbol]!.dir} className="text-[13.5px]" /> },
    { key: "ask", header: t("news.markets.col.ask"), align: "right", cell: (i) => <PriceText symbol={i.symbol} value={qs[i.symbol]!.ask} dir={qs[i.symbol]!.dir} className="text-[13.5px]" /> },
    {
      key: "spread",
      header: t("news.markets.col.spread"),
      align: "right",
      hideOn: "md",
      cell: (i) => <span className="k-num font-mono text-[12.5px] text-fg-2">{spreadText(i, qs[i.symbol]!.bid, qs[i.symbol]!.ask)}</span>,
    },
    { key: "chg", header: t("news.markets.col.dailyChange"), align: "right", cell: (i) => <Delta value={qs[i.symbol]!.change} chip />, sort: (i) => qs[i.symbol]!.change },
    {
      key: "spark",
      header: t("news.markets.col.week"),
      align: "center",
      hideOn: "sm",
      cell: (i) => (
        <div className="flex justify-center">
          <WeekLine inst={i} />
        </div>
      ),
    },
    {
      key: "hours",
      header: t("news.markets.col.hours"),
      hideOn: "lg",
      cell: (i) => (
        <div className="flex items-center gap-2 text-[12.5px]">
          <span className={cn("size-1.5 rounded-full", isMarketOpen(i.symbol) ? "bg-up" : "bg-fg-3")} title={isMarketOpen(i.symbol) ? t("news.markets.marketOpen") : t("news.markets.marketClosedLastPrice")} />
          <span className="text-fg-2">{t.dyn(`news.hoursShort.${i.assetClass}`, CONTRACT_SPECS[i.symbol]!.hoursShort)}</span>
          <span dir="ltr" className="font-mono text-[11px] text-fg-3">{CONTRACT_SPECS[i.symbol]!.hours[0]!.sessions}</span>
        </div>
      ),
    },
    {
      key: "trade",
      header: "",
      align: "right",
      cell: (i) => (
        <Link target="_blank" rel="noopener" href={`${TERMINAL_URL}/?symbol=${i.symbol}`} onClick={(e) => e.stopPropagation()}>
          <Button size="xs" variant="surface" className="group-hover:border-ember/40 group-hover:text-ember">
            {t("news.markets.trade")} <ArrowUpRight className="rtl:-scale-x-100" />
          </Button>
        </Link>
      ),
    },
  ];

  return (
    <div className="pb-24">
      <PageHero page="markets" overlap
        title={t("news.markets.title")}
        lead={t("news.markets.subtitle", { count: INSTRUMENTS.length })}
        actions={
          <Link target="_blank" rel="noopener" href={TERMINAL_URL}>
            <Button variant="ember" shimmer>
              <CandlestickChart /> {t("news.markets.openTerminal")}
            </Button>
          </Link>
        }
      />

      <Reveal>
        <Card>
          <CardHeader
            title={t("news.markets.heatmap.title")}
            subtitle={t("news.markets.heatmap.subtitle")}
            action={
              IS_DEMO || mode === "live" ? (
                <div className="hidden items-center gap-2 sm:flex">
                  <Chip tone="up">{t("news.markets.heatmap.up", { count: breadth })}</Chip>
                  <Chip tone="down">{t("news.markets.heatmap.down", { count: INSTRUMENTS.length - breadth })}</Chip>
                </div>
              ) : undefined
            }
          />
          <FeedGuard minHeight={160}>
            <div className="grid grid-cols-3 gap-2 px-4 pb-5 pt-4 sm:grid-cols-5 sm:px-6 md:grid-cols-7 xl:grid-cols-14">
              {sorted.map((i) => (
                <HeatTile key={i.symbol} inst={i} change={qs[i.symbol]!.change} onClick={() => setOpen(i.symbol)} />
              ))}
            </div>
          </FeedGuard>
        </Card>
      </Reveal>

      <Reveal delay={0.08} className="mt-4">
        <Card className="px-4 pb-5 pt-5 sm:px-6">
          <div className="mb-4 flex flex-wrap items-center gap-2">
            <div className="max-w-full overflow-x-auto">
              <Segmented
                value={tab}
                onChange={setTab}
                options={[
                  { value: "all", label: t("common.all") },
                  {
                    value: "fav",
                    label: (
                      <>
                        <Star className="size-3" /> {favs.length}
                      </>
                    ),
                  },
                  ...CLASSES.map((c) => ({ value: c, label: t.dyn(`news.assetClass.${c}`, ASSET_CLASS_LABEL[c]) })),
                ]}
              />
            </div>
            <div className="ms-auto flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
              <Search className="size-3.5 text-fg-3" />
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder={t("news.markets.searchPlaceholder")} className="w-40 bg-transparent text-[13px] outline-none placeholder:text-fg-3 sm:w-52" />
              {q && (
                <button onClick={() => setQ("")} aria-label={t("news.markets.clearSearch")} className="text-fg-3 hover:text-fg">
                  <X className="size-3.5" />
                </button>
              )}
            </div>
          </div>
          <FeedGuard minHeight={320}>
            <DataTable
              columns={cols}
              rows={rows}
              pageSize={30}
              rowKey={(i) => i.symbol}
              onRowClick={(i) => setOpen(i.symbol)}
              empty={
                tab === "fav" && !q ? (
                  <EmptyState art="emptyWatchlist" title={t("market.empty.favouritesTitle")} action={<Button variant="surface" onClick={() => setTab("all")}>{t("common.viewAll")}</Button>} />
                ) : (
                  <EmptyState illustration="magnifying_glass_tilted_left" title={t("market.empty.noMatch")} />
                )
              }
            />
          </FeedGuard>
          <div className="mt-3 flex items-center gap-2 text-[11.5px] text-fg-3">
            <Tooltip content={t("news.markets.liveTooltip")}>
              <span className="flex items-center gap-1.5">
                <span className="size-1.5 rounded-full bg-up" /> {t("common.live")}
              </span>
            </Tooltip>
            · {t("news.markets.footnote")}
          </div>
        </Card>
      </Reveal>

      <InstrumentDrawer symbol={open} onClose={() => setOpen(null)} fav={!!open && favs.includes(open)} onFav={() => open && toggleFav(open)} />
    </div>
  );
}

"use client";

/** Command Center › Live risk on the trading engine (live builds): server exposure, house P&L, risk alerts, top and
 *  toxic clients, news windows. The demo build keeps its mock page (app/(app)/command/risk/page.tsx). */
import * as React from "react";
import Link from "next/link";
import { AlertOctagon, BellRing, CalendarClock, Download, Gauge as GaugeIcon, Layers, Newspaper, ShieldAlert, SlidersHorizontal, Users } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, EmptyState, KpiCard, PageHeader, Reveal, Segmented, Skeleton, SymbolAvatar, Tooltip, buttonVariants, cn, formatNumber } from "@kalks/ui";
import { TradingDeskProvider } from "@/lib/trading-desk/context";
import { ago, downloadCsv, useApi, useNow } from "@/components/live/kit";
import { useCan } from "@/components/staff-session";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { RiskMonitor } from "@/components/trading-desk/risk-monitor";
import { CategoryChip, LoginCell, RiskError, ScoreBar, ScoreDrawer, UsageBar } from "@/components/trading-live/risk-kit";
import { flagLabel, groupLabel, serverStamp, useExposure, useLiveDirectory, useRiskAlerts, type Exposure, type NewsDoc, type ScoreList } from "@/lib/trading-desk";
import { PnlText, usdCompact } from "./kit";

function heatStyle(v: number, max: number): React.CSSProperties {
  const k = Math.min(1, Math.abs(v) / (max || 1));
  const pct = Math.round(8 + k * 52);
  const c = v >= 0 ? "var(--k-up)" : "var(--k-down)";
  return { background: `color-mix(in oklab, ${c} ${pct}%, transparent)`, borderColor: `color-mix(in oklab, ${c} ${Math.round(pct * 0.8)}%, transparent)` };
}

export function LiveRiskCommand() {
  return (
    <TradingDeskProvider>
      <LiveRisk />
    </TradingDeskProvider>
  );
}

function LiveRisk() {
  const can = useCan("dealing.read");
  const { exposure: e, error } = useExposure();
  const { alerts } = useRiskAlerts();
  useLiveDirectory();
  const now = useNow(5000);
  const open = alerts.filter((a) => a.status === "open");
  const red = open.filter((a) => a.level === "red").length;
  const t = e?.totals;
  const worst = [...(e?.symbols ?? [])].filter((s) => s.limit).sort((a, b) => b.limit!.usedPct - a.limit!.usedPct)[0];

  if (!can)
    return (
      <div className="pb-10">
        <PageHeader title="Live risk" subtitle="B-book exposure, house P&L, risk alerts and toxic flow." />
        <Card>
          <EmptyState illustration="locked" title="Not available for your role" text="Live risk needs the dealing read permission." />
        </Card>
      </div>
    );

  return (
    <div className="pb-10">
      <PageHeader
        title="Live risk"
        subtitle={
          <span className="inline-flex flex-wrap items-center gap-2">
            B-book exposure, house P&L, risk alerts and toxic flow from the trading engine. <DeskStatusChip />
            {e && <Chip size="sm" tone="neutral">Updated {ago(e.generatedAt, now)}</Chip>}
          </span>
        }
        actions={
          <>
            <Button
              variant="surface"
              size="lg"
              disabled={!e}
              onClick={() => {
                if (!e) return;
                downloadCsv(
                  "risk-snapshot",
                  ["symbol", "client_net_usd", "a_net_usd", "b_net_usd", "house_floating_usd", "limit_cap_usd", "limit_used_pct", "positions"],
                  e.symbols.map((s) => [s.symbol, s.netUsd.toFixed(0), s.books.A.netUsd.toFixed(0), s.books.B.netUsd.toFixed(0), s.houseFloatingUsd.toFixed(2), s.limit?.capUsd ?? "", s.limit ? s.limit.usedPct.toFixed(1) : "", s.positions]),
                );
                toast.success("risk-snapshot.csv exported", { description: `${e.symbols.length} symbols` });
              }}
            >
              <Download /> Export
            </Button>
            <Link href="/trading/risk-limits" className={buttonVariants({ variant: "ember", size: "lg" })}>
              <SlidersHorizontal /> Limits & alerts
            </Link>
          </>
        }
      />

      {error ? (
        <Card>
          <RiskError error={error} />
        </Card>
      ) : (
        <>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <KpiCard label="House floating (B)" icon={<GaugeIcon />} value={t ? <PnlText value={t.houseFloatingUsd} /> : <Skeleton className="h-8 w-24" />} chip={t ? `${t.positions} positions · ${t.accounts} accounts` : "Loading…"} chipTone="ember" href="/trading/exposure" />
            <KpiCard
              label="House daily P&L"
              icon={<CalendarClock />}
              value={t ? <PnlText value={t.houseDailyPnlUsd} /> : <Skeleton className="h-8 w-24" />}
              chip={t ? `Realised ${usdCompact(t.realisedToday.houseUsd, 1)} today` : "Since server day start"}
              chipTone={t && t.houseDailyPnlUsd < 0 ? "down" : "up"}
              hot
              delay={0.05}
            />
            <KpiCard label="Open risk alerts" icon={<BellRing />} value={<span className={cn("k-num", red && "text-down")}>{open.length}</span>} chip={`${red} red · ${open.length - red} amber / info`} chipTone={red ? "down" : open.length ? "warn" : "neutral"} href="/trading/dealer" delay={0.1} />
            <KpiCard
              label="Largest limit usage"
              icon={<AlertOctagon />}
              value={<span className="k-num">{worst ? worst.symbol : "—"}</span>}
              chip={worst ? `${formatNumber(worst.limit!.usedPct, 0)}% of ${usdCompact(worst.limit!.capUsd, 1)} · ${worst.limit!.mode === "enforce" ? "enforcing" : "alert only"}` : "No caps set"}
              chipTone={worst?.limit?.level === "red" ? "down" : worst?.limit?.level === "amber" ? "warn" : "neutral"}
              href="/trading/risk-limits"
              delay={0.15}
            />
          </div>

          <Reveal delay={0.08} className="mt-4">
            <BookHeat e={e} />
          </Reveal>

          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Reveal delay={0.1} className="xl:col-span-7">
              <RiskMonitor className="h-full" max={10} />
            </Reveal>
            <Reveal delay={0.12} className="xl:col-span-5">
              <TopClients e={e} />
            </Reveal>
          </div>

          <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
            <Reveal delay={0.1} className="xl:col-span-5">
              <ToxicClients />
            </Reveal>
            <Reveal delay={0.12} className="xl:col-span-4">
              <NewsWindows now={now} />
            </Reveal>
            <Reveal delay={0.14} className="xl:col-span-3">
              <Groups e={e} />
            </Reveal>
          </div>
        </>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */

function BookHeat({ e }: { e: Exposure | null }) {
  const [mode, setMode] = React.useState<"usd" | "lots">("usd");
  const rows = (e?.symbols ?? []).slice(0, 14);
  const val = (v: { netUsd: number; netLots: number }) => (mode === "usd" ? v.netUsd : v.netLots);
  const max = Math.max(1, ...rows.flatMap((r) => [Math.abs(val(r.books.A)), Math.abs(val(r.books.B)), Math.abs(mode === "usd" ? r.netUsd : r.netLots)]));
  return (
    <Card>
      <CardHeader
        title="Exposure heat grid"
        subtitle="Client net by symbol × book · green = clients net long, red = net short · with limit usage on the B-book net"
        action={<Segmented size="xs" value={mode} onChange={setMode} options={[{ value: "usd", label: "Net USD" }, { value: "lots", label: "Net lots" }]} />}
      />
      <div className="mt-4 overflow-x-auto px-4 pb-5 sm:px-6">
        {!e ? (
          <Skeleton className="h-56 w-full" />
        ) : !rows.length ? (
          <EmptyState title="No open exposure" text="The grid fills as soon as client positions are open." illustration="chart_increasing" />
        ) : (
          <div className="grid min-w-[640px] gap-1.5" style={{ gridTemplateColumns: "150px repeat(3, minmax(0,1fr)) 150px" }}>
            <div />
            {["A-book", "B-book", "Total"].map((h) => (
              <div key={h} className="pb-1 text-center text-[11px] font-medium uppercase tracking-wider text-fg-3">
                {h}
              </div>
            ))}
            <div className="pb-1 text-right text-[11px] font-medium uppercase tracking-wider text-fg-3">Limit usage</div>
            {rows.map((r) => {
              const cells = [val(r.books.A), val(r.books.B), mode === "usd" ? r.netUsd : r.netLots];
              const fmt = (v: number) => (mode === "usd" ? usdCompact(v, 1) : formatNumber(v, 2));
              return (
                <React.Fragment key={r.symbol}>
                  <Link href="/trading/exposure" className="flex items-center gap-2.5 pr-2 hover:text-ember">
                    <SymbolAvatar symbol={r.symbol} size={22} />
                    <span>
                      <span className="block text-[13px] font-medium">{r.symbol}</span>
                      <span className="block text-[10.5px] text-fg-3">{r.positions} pos</span>
                    </span>
                  </Link>
                  {cells.map((v, k) => (
                    <Tooltip key={k} content={`${r.symbol} · ${["A-book", "B-book", "Total"][k]} · ${fmt(v)}`}>
                      <div className="k-num grid h-11 place-items-center rounded-[10px] border font-mono text-[12px] font-medium text-fg" style={heatStyle(v, max)}>
                        {fmt(v)}
                      </div>
                    </Tooltip>
                  ))}
                  <div className="flex items-center justify-end">{r.limit ? <UsageBar usedPct={r.limit.usedPct} level={r.limit.level} /> : <span className="text-[11px] text-fg-3">No cap</span>}</div>
                </React.Fragment>
              );
            })}
          </div>
        )}
      </div>
    </Card>
  );
}

function TopClients({ e }: { e: Exposure | null }) {
  const [tab, setTab] = React.useState<"winning" | "exposed">("winning");
  const list = React.useMemo(() => {
    const m = new Map<string, { login: string; userId: string; floating: number; net: number; symbols: Set<string> }>();
    for (const s of e?.symbols ?? [])
      for (const c of s.topClients) {
        const r = m.get(c.login) ?? { login: c.login, userId: c.userId, floating: 0, net: 0, symbols: new Set<string>() };
        r.floating += c.floatingUsd;
        r.net += Math.abs(c.netUsd);
        r.symbols.add(s.symbol);
        m.set(c.login, r);
      }
    const xs = [...m.values()];
    return (tab === "winning" ? xs.sort((a, b) => b.floating - a.floating) : xs.sort((a, b) => b.net - a.net)).slice(0, 8);
  }, [e, tab]);
  const max = Math.max(1, ...list.map((r) => Math.abs(tab === "winning" ? r.floating : r.net)));
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title="Top clients"
        subtitle={tab === "winning" ? "Largest client floating profit = house loss on B-book" : "Largest gross client net across symbols"}
        icon={<Users />}
        action={<Segmented size="xs" value={tab} onChange={setTab} options={[{ value: "winning", label: "Winning" }, { value: "exposed", label: "Exposed" }]} />}
      />
      <div className="mt-4 flex-1 space-y-1.5 px-4 pb-5 sm:px-6">
        {!e && <Skeleton className="h-40 w-full" />}
        {e && !list.length && <div className="text-[12.5px] text-fg-3">No open client positions.</div>}
        {list.map((r) => {
          const v = tab === "winning" ? r.floating : r.net;
          return (
            <Link key={r.login} href={`/trading/accounts?login=${r.login}`} className="k-row block px-3 py-2 hover:border-fg-3">
              <span className="flex items-center gap-3">
                <span className="min-w-0 flex-1">
                  <LoginCell login={r.login} userId={r.userId} sub={[...r.symbols].slice(0, 3).join(", ")} />
                </span>
                {tab === "winning" ? <PnlText value={r.floating} className="font-mono text-[12.5px]" /> : <span className="k-num font-mono text-[12.5px]">{usdCompact(r.net, 1)}</span>}
              </span>
              <span className="mt-1.5 block h-1 overflow-hidden rounded-full bg-surface-3">
                <span className={cn("block h-full rounded-full", tab === "winning" ? (v >= 0 ? "bg-down" : "bg-up") : "bg-warn")} style={{ width: `${(Math.abs(v) / max) * 100}%` }} />
              </span>
            </Link>
          );
        })}
      </div>
    </Card>
  );
}

function ToxicClients() {
  const { data, error, reload } = useApi<ScoreList>("/api/trading/dealing/risk/scores?category=toxic&limit=20", { refreshMs: 60_000 });
  const [open, setOpen] = React.useState<string | null>(null);
  const items = (data?.items ?? []).filter((s) => s.category === "toxic").slice(0, 8);
  return (
    <Card className="h-full">
      <CardHeader
        title="Toxic clients"
        subtitle="Engine client scores flagged toxic"
        icon={<ShieldAlert />}
        action={
          <Link href="/trading/client-scores" className="text-[12.5px] text-fg-2 hover:text-ember">
            All scores
          </Link>
        }
      />
      <div className="mt-4 space-y-1.5 px-4 pb-5 sm:px-6">
        {error ? (
          <RiskError error={error} compact onRetry={reload} />
        ) : !data ? (
          <Skeleton className="h-32 w-full" />
        ) : !items.length ? (
          <div className="text-[12.5px] text-fg-3">No toxic clients in the last scoring run.</div>
        ) : (
          items.map((s) => (
            <button key={s.login} type="button" onClick={() => setOpen(s.login)} className="k-row flex w-full items-center gap-3 px-3 py-2 text-left hover:border-fg-3">
              <span className="min-w-0 flex-1">
                <LoginCell login={s.login} userId={s.userId} sub={s.toxicFlags.map(flagLabel).join(", ") || undefined} />
              </span>
              <span className="flex flex-col items-end gap-1">
                <CategoryChip category={s.category} />
                <ScoreBar score={s.score} category={s.category} />
              </span>
            </button>
          ))
        )}
      </div>
      <ScoreDrawer login={open} onClose={() => setOpen(null)} />
    </Card>
  );
}

function NewsWindows({ now }: { now: number }) {
  const { data, error, reload } = useApi<NewsDoc>("/api/trading/dealing/risk/news", { refreshMs: 60_000 });
  const list = (data?.windows ?? []).slice(0, 6);
  return (
    <Card className="h-full">
      <CardHeader title="News windows" subtitle="High-impact events and their symbols" icon={<Newspaper />} action={data && !data.configured ? <Chip size="sm" tone="warn">Calendar off</Chip> : undefined} />
      <div className="mt-4 space-y-1.5 px-4 pb-5 sm:px-6">
        {error ? (
          <RiskError error={error} compact onRetry={reload} />
        ) : !data ? (
          <Skeleton className="h-32 w-full" />
        ) : !list.length ? (
          <div className="text-[12.5px] text-fg-3">No upcoming news windows.</div>
        ) : (
          list.map((w) => (
            <div key={w.id} className={cn("k-row flex items-center gap-3 px-3 py-2", w.active && "border-warn/40 bg-warn-soft")}>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12.5px]">{w.title}</span>
                <span className="block text-[11px] text-fg-3">
                  <span className="font-mono">{w.currency}</span> · {w.symbols.slice(0, 4).join(", ")}
                  {w.symbols.length > 4 ? ` +${w.symbols.length - 4}` : ""}
                </span>
              </span>
              <span className="text-right text-[11px]">
                {w.active ? (
                  <Chip size="sm" tone="warn" dot>
                    Active
                  </Chip>
                ) : (
                  <span className="block font-mono text-fg-2">{serverStamp(w.startsAt).slice(0, 12)}</span>
                )}
                <span className="block text-fg-3">{ago(w.startsAt, now)}</span>
              </span>
            </div>
          ))
        )}
      </div>
    </Card>
  );
}

function Groups({ e }: { e: Exposure | null }) {
  const groups = e?.groups ?? [];
  const max = Math.max(1, ...groups.map((g) => Math.abs(g.netUsd)));
  return (
    <Card className="h-full">
      <CardHeader title="By group" subtitle="Client net USD" icon={<Layers />} />
      <div className="space-y-2.5 px-6 pb-6 pt-4">
        {!e && <Skeleton className="h-32 w-full" />}
        {e && !groups.length && <div className="text-[12.5px] text-fg-3">No open positions.</div>}
        {groups.map((g) => (
          <div key={g.group} className="text-[12.5px]">
            <div className="flex items-center justify-between gap-2">
              <span className="truncate">{groupLabel(g.group)}</span>
              <span className={cn("k-num font-mono", g.netUsd >= 0 ? "text-up" : "text-down")}>{usdCompact(g.netUsd, 1)}</span>
            </div>
            <div className="mt-1 h-1.5 overflow-hidden rounded-full bg-surface-3">
              <div className={cn("h-full rounded-full", g.netUsd >= 0 ? "bg-up" : "bg-down")} style={{ width: `${(Math.abs(g.netUsd) / max) * 100}%` }} />
            </div>
            <div className="mt-0.5 text-[11px] text-fg-3">
              {g.positions} positions · {formatNumber(g.lots, 2)} lots
            </div>
          </div>
        ))}
      </div>
    </Card>
  );
}

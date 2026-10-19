"use client";

import * as React from "react";
import Link from "next/link";
import { AlertOctagon, Download, Gauge as GaugeIcon, ShieldAlert, SlidersHorizontal, TrendingDown } from "lucide-react";
import { toast } from "sonner";
import {
  Avatar,
  Button,
  Card,
  CardHeader,
  Chip,
  DataTable,
  Flag,
  KpiCard,
  PageHeader,
  Progress,
  Reveal,
  Segmented,
  SymbolAvatar,
  SymbolCell,
  Tooltip,
  cn,
  formatMoney,
  formatNumber,
  useQuotes,
  type Column,
} from "@kalks/ui";
import { getClient, REASON_CODES, serverTime } from "@kalks/mock/admin-clients";
import { EXPOSURE_GRID, MARGIN_CALLS, RISK_GROUPS, STOPOUT_LOG, TOP_CLIENTS, type MarginCallRow } from "@kalks/mock/admin-ops";
import { PnlText, ReasonDialog, usdCompact } from "@/components/command/kit";
import { useLiveExposure } from "@/components/command/overview";
import { LiveRiskCommand } from "@/components/command/live-risk";
import { IS_DEMO } from "@kalks/mock/mode";

/* ------------------------------------------------------------------ */

function heatStyle(v: number, max: number): React.CSSProperties {
  const k = Math.min(1, Math.abs(v) / max);
  const pct = Math.round(8 + k * 52);
  const c = v >= 0 ? "var(--k-up)" : "var(--k-down)";
  return { background: `color-mix(in oklab, ${c} ${pct}%, transparent)`, borderColor: `color-mix(in oklab, ${c} ${Math.round(pct * 0.8)}%, transparent)` };
}

function HeatGrid() {
  const [mode, setMode] = React.useState<"usd" | "share">("usd");
  const max = Math.max(...EXPOSURE_GRID.flatMap((r) => r.cells.map(Math.abs)));
  const colTotals = RISK_GROUPS.map((_, k) => EXPOSURE_GRID.reduce((s, r) => s + r.cells[k]!, 0));
  return (
    <Card>
      <CardHeader
        title="Exposure heat grid"
        subtitle="Net client exposure by symbol × group · green = clients net long, red = net short"
        action={<Segmented size="xs" value={mode} onChange={setMode} options={[{ value: "usd", label: "Net USD" }, { value: "share", label: "% of symbol" }]} />}
      />
      <div className="mt-4 overflow-x-auto px-4 pb-5 sm:px-6">
        <div className="grid min-w-[820px] gap-1.5" style={{ gridTemplateColumns: `150px repeat(${RISK_GROUPS.length}, minmax(0,1fr)) 120px` }}>
          <div />
          {RISK_GROUPS.map((g) => (
            <div key={g} className="pb-1 text-center text-[11px] font-medium uppercase tracking-wider text-fg-3">
              {g}
            </div>
          ))}
          <div className="pb-1 text-right text-[11px] font-medium uppercase tracking-wider text-fg-3">Net total</div>
          {EXPOSURE_GRID.map((row) => {
            const total = row.cells.reduce((a, b) => a + b, 0);
            const gross = row.cells.reduce((a, b) => a + Math.abs(b), 0) || 1;
            return (
              <React.Fragment key={row.symbol}>
                <div className="flex items-center gap-2.5 pr-2">
                  <SymbolAvatar symbol={row.symbol} size={22} />
                  <span className="text-[13px] font-medium">{row.symbol}</span>
                </div>
                {row.cells.map((v, k) => (
                  <Tooltip key={k} content={`${row.symbol} · ${RISK_GROUPS[k]} · ${usdCompact(v)}`}>
                    <button
                      onClick={() => toast(`${row.symbol} · ${RISK_GROUPS[k]}`, { description: `Net ${usdCompact(v)} · open positions filtered in Trading → Positions` })}
                      className="k-num grid h-11 place-items-center rounded-[10px] border font-mono text-[12px] font-medium text-fg transition-transform hover:scale-[1.03]"
                      style={heatStyle(v, max)}
                    >
                      {mode === "usd" ? usdCompact(v, 1) : `${Math.round((v / gross) * 100)}%`}
                    </button>
                  </Tooltip>
                ))}
                <div className={cn("k-num flex items-center justify-end font-mono text-[12.5px] font-semibold", total >= 0 ? "text-up" : "text-down")}>{usdCompact(total, 1)}</div>
              </React.Fragment>
            );
          })}
          <div className="pt-2 text-[11px] uppercase tracking-wider text-fg-3">Group net</div>
          {colTotals.map((t, k) => (
            <div key={k} className={cn("k-num pt-2 text-center font-mono text-[12px] font-medium", t >= 0 ? "text-up" : "text-down")}>
              {usdCompact(t, 1)}
            </div>
          ))}
          <div />
        </div>
        <div className="mt-4 flex items-center justify-end gap-2 text-[11px] text-fg-3">
          Net short
          <span className="h-2 w-40 rounded-full bg-[linear-gradient(90deg,var(--k-down),transparent_50%,var(--k-up))]" />
          Net long
        </div>
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function TopClients() {
  const [tab, setTab] = React.useState<"winners" | "losers">("winners");
  const list = TOP_CLIENTS[tab];
  const max = Math.max(...list.map((r) => Math.abs(r.pnlToday)));
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title="Top clients today"
        subtitle={tab === "winners" ? "Client winners = book losers on B-book" : "Largest client losses today"}
        action={<Segmented size="xs" value={tab} onChange={setTab} options={[{ value: "winners", label: "Winning" }, { value: "losers", label: "Losing" }]} />}
      />
      <div className="mt-4 flex-1 space-y-1.5 px-4 pb-5 sm:px-6">
        {list.map((r, i) => {
          const c = getClient(r.clientId);
          return (
            <Link key={r.clientId} href={`/clients/${c.id}`} className="k-row flex items-center gap-3 px-3.5 py-2.5 transition-colors hover:bg-surface-3/60">
              <span className="k-num w-4 text-[11px] text-fg-3">{i + 1}</span>
              <Avatar src={c.photo} name={c.name} size={30} />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1.5 text-[13px] font-medium">
                  <span className="truncate">{c.name}</span>
                  <Flag country={c.country} className="size-3.5" />
                </div>
                <div className="font-mono text-[11px] text-fg-3">
                  #{r.login} · {r.trades} trades · {r.topSymbol}
                </div>
              </div>
              <div className="w-28 text-right">
                <PnlText value={r.pnlToday} className="text-[13.5px]" />
                <div className="mt-1 h-1 overflow-hidden rounded-full bg-surface-3">
                  <div className={cn("ml-auto h-full rounded-full", r.pnlToday >= 0 ? "bg-up" : "bg-down")} style={{ width: `${(Math.abs(r.pnlToday) / max) * 100}%` }} />
                </div>
              </div>
            </Link>
          );
        })}
      </div>
    </Card>
  );
}

/* ------------------------------------------------------------------ */

function useLiveMarginCalls() {
  const syms = Array.from(new Set(MARGIN_CALLS.map((m) => m.symbol)));
  const qs = useQuotes(syms);
  const ref = React.useRef<Record<string, number> | null>(null);
  if (!ref.current) ref.current = Object.fromEntries(syms.map((s) => [s, (qs[s]!.bid + qs[s]!.ask) / 2]));
  return MARGIN_CALLS.map((m) => {
    const q = qs[m.symbol]!;
    const mid = (q.bid + q.ask) / 2;
    const move = (mid / ref.current![m.symbol]! - 1) * (m.side === "buy" ? 1 : -1);
    const ml = Math.max(20, Math.min(160, (m.equity / m.margin) * 100 + move * 2500));
    return { ...m, liveEquity: (ml / 100) * m.margin, ml, dir: q.dir };
  }).sort((a, b) => a.ml - b.ml);
}

function MarginCalls() {
  const rows = useLiveMarginCalls();
  const [target, setTarget] = React.useState<(MarginCallRow & { ml: number }) | null>(null);
  return (
    <Card className="flex h-full flex-col">
      <CardHeader
        title="Margin calls"
        subtitle={`${rows.length} accounts below 100% ML · stop-out at 50%`}
        action={
          <Button size="sm" variant="surface" onClick={() => toast.success(`Margin-call notice sent to ${rows.length} clients`, { description: "Email + push + in-app banner" })}>
            Notify all
          </Button>
        }
      />
      <div className="mt-4 overflow-x-auto px-4 pb-5 sm:px-6">
        <table className="w-full min-w-[660px] border-separate border-spacing-0 text-[13px]">
          <thead>
            <tr className="text-[11px] uppercase tracking-[0.05em] text-fg-3">
              {["Client", "Exposure", "Equity / margin", "Margin level", ""].map((h, i) => (
                <th key={i} className={cn("border-y border-line bg-surface-2 px-3 py-2.5 font-medium", i === 0 && "rounded-l-[14px] border-l pl-4 text-left", i === 1 && "text-left", i === 2 && "text-right", i === 3 && "w-[170px] text-left", i === 4 && "rounded-r-[14px] border-r")}>
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const c = getClient(r.clientId);
              const tone = r.ml < 60 ? "down" : r.ml < 80 ? "warn" : "gold";
              return (
                <tr key={r.clientId} className="group">
                  <td className="border-b border-line py-2 pl-4 pr-3 group-hover:bg-surface-2/60">
                    <Link href={`/clients/${c.id}`} className="flex items-center gap-2.5">
                      <Avatar src={c.photo} name={c.name} size={28} />
                      <span className="min-w-0">
                        <span className="block truncate text-[13px] font-medium hover:text-ember">{c.name}</span>
                        <span className="block font-mono text-[11px] text-fg-3">
                          #{r.login} · {r.group}
                        </span>
                      </span>
                    </Link>
                  </td>
                  <td className="border-b border-line px-3 group-hover:bg-surface-2/60">
                    <span className="flex items-center gap-2">
                      <SymbolAvatar symbol={r.symbol} size={20} />
                      <span className="text-[12.5px]">{r.symbol}</span>
                      <Chip size="sm" tone={r.side === "buy" ? "up" : "down"}>
                        {r.side.toUpperCase()} {r.lots}
                      </Chip>
                    </span>
                  </td>
                  <td className="k-num border-b border-line px-3 text-right font-mono text-[12.5px] group-hover:bg-surface-2/60">
                    {formatMoney(r.liveEquity)}
                    <div className="text-[11px] text-fg-3">{formatMoney(r.margin)}</div>
                  </td>
                  <td className="border-b border-line px-3 group-hover:bg-surface-2/60">
                    <div className="flex items-center gap-2.5">
                      <div className="relative flex-1">
                        <Progress value={r.ml} tone={tone} />
                        <span className="absolute -top-0.5 left-1/2 h-2.5 w-px bg-down" title="Stop-out 50%" />
                      </div>
                      <span className={cn("k-num w-14 text-right font-mono text-[12.5px] font-semibold", tone === "down" ? "text-down" : tone === "warn" ? "text-warn" : "text-gold", r.dir === 1 && "flash-up", r.dir === -1 && "flash-down")}>{r.ml.toFixed(1)}%</span>
                    </div>
                  </td>
                  <td className="border-b border-line py-2 pl-3 pr-2 text-right group-hover:bg-surface-2/60">
                    <div className="flex justify-end gap-1.5">
                      <Button size="xs" variant="surface" onClick={() => toast.success(`Margin call sent to ${c.name}`, { description: `ML ${r.ml.toFixed(1)}% · email + push` })}>
                        Notify
                      </Button>
                      <Button size="xs" variant="down-outline" onClick={() => setTarget(r)}>
                        Liquidate
                      </Button>
                    </div>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
      {target && (
        <ReasonDialog
          open
          onOpenChange={(o) => !o && setTarget(null)}
          title="Manual liquidation"
          description={`${getClient(target.clientId).name} · #${target.login} · ML ${target.ml.toFixed(1)}%`}
          codes={REASON_CODES.trade}
          confirmLabel="Close all positions"
          confirmVariant="sell"
          successMessage={`All positions on #${target.login} closed at market`}
        >
          <div className="k-row flex items-center justify-between px-4 py-3 text-[13px]">
            <SymbolCell symbol={target.symbol} size={24} sub={`${target.side.toUpperCase()} ${target.lots} lots`} />
            <span className="text-right text-[12px] text-fg-3">
              Closes at market
              <br />
              <span className="font-mono">slippage tolerance 0.5 pips</span>
            </span>
          </div>
        </ReasonDialog>
      )}
    </Card>
  );
}

/* ------------------------------------------------------------------ */

type StopOut = (typeof STOPOUT_LOG)[number];

const stopCols: Column<StopOut>[] = [
  { key: "time", header: "Time", cell: (r) => <span className="font-mono text-[12px] text-fg-2">{serverTime(r.time)}</span>, sort: (r) => r.time },
  { key: "id", header: "Event", cell: (r) => <span className="font-mono text-[12px]">{r.id}</span> },
  {
    key: "client",
    header: "Client",
    cell: (r) => {
      const c = getClient(r.clientId);
      return (
        <Link href={`/clients/${c.id}`} className="flex items-center gap-2 hover:text-ember">
          <Avatar src={c.photo} name={c.name} size={24} />
          <span className="text-[13px]">{c.name}</span>
          <span className="font-mono text-[11px] text-fg-3">#{r.login}</span>
        </Link>
      );
    },
  },
  { key: "symbol", header: "Symbol", cell: (r) => <SymbolCell symbol={r.symbol} size={20} sub={`${r.positions} positions`} /> },
  { key: "lots", header: "Lots", align: "right", cell: (r) => <span className="k-num">{r.lots.toFixed(2)}</span>, sort: (r) => r.lots },
  { key: "ml", header: "ML at close", align: "right", cell: (r) => <span className="k-num font-mono text-down">{r.ml}%</span> },
  { key: "slip", header: "Slippage", align: "right", cell: (r) => <span className={cn("k-num font-mono", r.slippage > 1 ? "text-warn" : "text-fg-2")}>{r.slippage} pips</span>, sort: (r) => r.slippage },
  { key: "loss", header: "Realised", align: "right", cell: (r) => <PnlText value={r.loss} />, sort: (r) => r.loss },
  { key: "neg", header: "Neg. balance", align: "center", cell: (r) => (r.loss < -8000 ? <Chip size="sm" tone="warn">Protected</Chip> : <span className="text-fg-3">—</span>) },
];

/* ------------------------------------------------------------------ */

export default function LiveRiskPage() {
  return IS_DEMO ? <DemoLiveRiskPage /> : <LiveRiskCommand />;
}

function DemoLiveRiskPage() {
  const exp = useLiveExposure();
  const floating = exp.reduce((s, r) => s + r.bookPnl, 0);
  const top = [...exp].sort((a, b) => b.usage - a.usage)[0]!;
  const soLoss = STOPOUT_LOG.reduce((s, r) => s + r.loss, 0);
  return (
    <div className="pb-10">
      <PageHeader
        title="Live risk"
        subtitle="B-book exposure, client P&L concentration, margin calls and stop-outs — streaming."
        actions={
          <>
            <Button variant="surface" size="lg" onClick={() => toast.success("risk-snapshot.csv exported", { description: "Heat grid, margin calls, stop-outs" })}>
              <Download /> Export
            </Button>
            <Link href="/trading/exposure">
              <Button variant="ember" size="lg">
                <SlidersHorizontal /> Limits & alerts
              </Button>
            </Link>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Book floating P&L" icon={<GaugeIcon />} value={<span className={cn("k-num", floating >= 0 ? "text-up" : "text-down")}>{floating >= 0 ? "+" : "-"}{formatMoney(Math.abs(floating)).replace("-", "")}</span>} chip="Live · 8 symbols" chipTone="ember" />
        <KpiCard label="Margin calls" icon={<ShieldAlert />} value={<span className="k-num">{MARGIN_CALLS.length}</span>} chip={`${MARGIN_CALLS.filter((m) => m.equity / m.margin < 0.6).length} within 10% of stop-out`} chipTone="warn" delay={0.05} />
        <KpiCard label="Stop-outs today" icon={<TrendingDown />} value={<span className="k-num">{STOPOUT_LOG.length}</span>} chip={`${formatMoney(soLoss)} realised`} chipTone="down" delay={0.1} />
        <KpiCard label="Largest concentration" icon={<AlertOctagon />} value={<span className="k-num">{top.symbol}</span>} chip={`${Math.round(top.usage)}% of limit · ${usdCompact(top.netUsd)}`} chipTone={top.usage >= 85 ? "down" : "warn"} href="/trading/exposure" delay={0.15} />
      </div>

      <Reveal delay={0.1} className="mt-4">
        <HeatGrid />
      </Reveal>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.1} className="xl:col-span-8">
          <MarginCalls />
        </Reveal>
        <Reveal delay={0.15} className="xl:col-span-4">
          <TopClients />
        </Reveal>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader title="Stop-out log" subtitle="Today · negative balance protection applied automatically" />
          <div className="mt-4 px-4 pb-5 sm:px-6">
            <DataTable columns={stopCols} rows={STOPOUT_LOG} dense pageSize={8} exportName="stop-outs" search={(r) => `${r.id} ${r.login} ${r.symbol} ${getClient(r.clientId).name}`} rowKey={(r) => r.id} />
          </div>
        </Card>
      </Reveal>
      <p className="mt-3 text-[11.5px] text-fg-3">Margin levels recalculate on every tick from the price feed · stop-out 50% · margin call 100%.</p>
    </div>
  );
}

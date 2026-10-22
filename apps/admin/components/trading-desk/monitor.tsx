"use client";

import * as React from "react";
import Link from "next/link";
import { ExternalLink, TrendingDown } from "lucide-react";
import { Button, Chip, DataTable, Segmented, SymbolAvatar, cn, formatNumber, useQuotes, type Column } from "@kalks/ui";
import { priceFeed } from "@kalks/mock";
import { MiniClient } from "@/components/trading/shared";
import { REASON_STOP_OUT, accountMetrics, clientName, getAccount, groupLabel, positionPnl, useDesk, type AccountMetrics, type DeskPosition } from "@/lib/trading-desk";
import { DeskDialog, MetaTile, money, signedMoney } from "./kit";
import { useCan } from "@/components/staff-session";
import { NeedsPerm } from "@/components/needs-perm";

type Row = AccountMetrics & { clientId: string; group: string; leverage: number; loser: DeskPosition | null; loserPnl: number };

export function MarginMonitor() {
  const { state, api } = useDesk();
  const canDeal = useCan("dealing.write");
  const [view, setView] = React.useState<"risk" | "all">("risk");
  const [target, setTarget] = React.useState<Row | null>(null);
  const symbols = React.useMemo(() => Array.from(new Set(state.positions.map((p) => p.symbol))).sort(), [state.positions]);
  const qs = useQuotes(symbols);
  const quote = React.useCallback((s: string) => qs[s] ?? priceFeed().quote(s), [qs]);
  const { marginCallPct, stopOutPct } = state.tenant;

  const logins = React.useMemo(() => Array.from(new Set(state.positions.map((p) => p.login))), [state.positions]);
  const all: Row[] = logins
    .map((l) => {
      const m = accountMetrics(state, l, quote);
      const a = getAccount(l);
      if (!m || !a) return null;
      let loser: DeskPosition | null = null;
      let loserPnl = 0;
      for (const p of m.positions) {
        const v = positionPnl(p, quote(p.symbol));
        if (v < loserPnl) {
          loserPnl = v;
          loser = p;
        }
      }
      return { ...m, clientId: a.clientId, group: a.group, leverage: a.leverage, loser, loserPnl };
    })
    .filter((x): x is Row => !!x)
    .sort((a, b) => a.level - b.level);
  const rows = view === "risk" ? all.filter((r) => r.level < 300) : all;
  const stopOut = all.filter((r) => r.level < stopOutPct).length;
  const call = all.filter((r) => r.level >= stopOutPct && r.level < marginCallPct).length;

  const status = (l: number) => (l < stopOutPct ? { t: "Stop-out", tone: "down" as const } : l < marginCallPct ? { t: "Margin call", tone: "warn" as const } : l < 200 ? { t: "Watch", tone: "gold" as const } : { t: "Healthy", tone: "up" as const });

  const cols: Column<Row>[] = [
    { key: "c", header: "Account", cell: (r) => <MiniClient clientId={r.clientId} login={`${r.login} · ${groupLabel(r.group)} 1:${r.leverage}`} />, csv: (r) => `${clientName(r.clientId, r.login)} (${r.login}, ${r.group})` },
    { key: "b", header: "Balance", align: "right", cell: (r) => <span className="k-num font-mono text-[12px] text-fg-2">{money(r.balance, 0)}</span>, sort: (r) => r.balance },
    { key: "e", header: "Equity", align: "right", cell: (r) => <span className="k-num font-mono text-[12.5px]">{money(r.equity, 0)}</span>, sort: (r) => r.equity },
    { key: "f", header: "Floating", align: "right", cell: (r) => <span className={cn("k-num font-mono text-[12px]", r.floating >= 0 ? "text-up" : "text-down")}>{signedMoney(r.floating, 0)}</span>, sort: (r) => r.floating },
    { key: "m", header: "Margin", align: "right", cell: (r) => <span className="k-num font-mono text-[12px] text-fg-2">{money(r.margin, 0)}</span>, sort: (r) => r.margin },
    { key: "fm", header: "Free", align: "right", hideOn: "xl", cell: (r) => <span className={cn("k-num font-mono text-[12px]", r.freeMargin < 0 ? "text-down" : "text-fg-2")}>{money(r.freeMargin, 0)}</span>, sort: (r) => r.freeMargin },
    {
      key: "l",
      header: "Margin level",
      align: "right",
      sort: (r) => (r.level === Infinity ? 1e9 : r.level),
      csv: (r) => (r.level === Infinity ? "" : r.level.toFixed(1)),
      cell: (r) => {
        const s = status(r.level);
        return (
          <span className="inline-flex items-center gap-2">
            <span className="h-1.5 w-14 overflow-hidden rounded-full bg-surface-3">
              <span className={cn("block h-full rounded-full", { down: "bg-down", warn: "bg-warn", gold: "bg-gold", up: "bg-up" }[s.tone])} style={{ width: `${Math.max(6, Math.min(100, r.level / 5))}%` }} />
            </span>
            <span className={cn("k-num w-14 text-right font-mono text-[12px]", { down: "text-down", warn: "text-warn", gold: "text-gold", up: "text-fg-2" }[s.tone])}>{r.level === Infinity ? "—" : `${formatNumber(r.level, 0)}%`}</span>
          </span>
        );
      },
    },
    { key: "s", header: "Status", cell: (r) => { const s = status(r.level); return <Chip size="sm" tone={s.tone} dot>{s.t}</Chip>; }, csv: (r) => status(r.level).t },
    {
      key: "lo",
      header: "Largest loser",
      cell: (r) =>
        r.loser ? (
          <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-[12px]">
            <SymbolAvatar symbol={r.loser.symbol} size={16} />
            <span className="font-mono text-fg-2">#{r.loser.ticket}</span>
            <span className="k-num font-mono text-down">{signedMoney(r.loserPnl, 0)}</span>
          </span>
        ) : (
          <span className="text-[12px] text-fg-3">—</span>
        ),
      sort: (r) => r.loserPnl,
    },
    {
      key: "a",
      header: "",
      align: "right",
      cell: (r) => (
        <span className="inline-flex items-center gap-1.5" onClick={(e) => e.stopPropagation()}>
          <NeedsPerm ok={canDeal} perm="dealing.write">
            <Button size="xs" variant="down-outline" disabled={!r.loser} onClick={() => setTarget(r)} aria-label={`Close largest loser on ${r.login}`}>
              <TrendingDown /> Close loser
            </Button>
          </NeedsPerm>
          <Link href={`/trading?login=${r.login}`} className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label={`Positions of ${r.login}`}>
            <ExternalLink className="size-3.5" />
          </Link>
        </span>
      ),
    },
  ];

  const lp = target?.loser ? state.positions.find((p) => p.ticket === target.loser!.ticket) : null;
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-2 lg:grid-cols-4">
        <MetaTile label={`Below stop-out (${stopOutPct}%)`} value={stopOut} tone={stopOut ? "down" : undefined} />
        <MetaTile label={`Margin call (${marginCallPct}%)`} value={call} tone={call ? "warn" : undefined} />
        <MetaTile label="Accounts with positions" value={all.length} />
        <MetaTile label="Equity at risk (< 200%)" value={money(all.filter((r) => r.level < 200).reduce((s, r) => s + r.equity, 0), 0)} />
      </div>
      <DataTable
        columns={cols}
        rows={rows}
        dense
        pageSize={12}
        rowKey={(r) => r.login}
        exportName="margin-monitor"
        search={(r) => `${r.login} ${clientName(r.clientId, r.login)}`}
        searchPlaceholder="Login, client…"
        toolbar={<Segmented size="sm" value={view} onChange={setView} options={[{ value: "risk", label: "Level < 300%" }, { value: "all", label: "All accounts" }]} />}
      />
      <DeskDialog
        open={!!target && !!lp}
        onOpenChange={(o) => !o && setTarget(null)}
        title={lp ? `Close largest loser on ${target!.login}` : ""}
        description="Closes the position with the biggest floating loss at market (stop-out procedure, forced)."
        defaultCode={REASON_STOP_OUT}
        confirmLabel={lp ? `Close #${lp.ticket}` : "Close"}
        confirmVariant="sell"
        onConfirm={(r) => api.closePosition(lp!.ticket, r, { stopOut: true })}
        success={(d) => `#${lp?.ticket} closed · ${signedMoney(d.profit)}`}
      >
        {lp && target && (
          <div className="grid grid-cols-3 gap-2">
            <MetaTile label="Position" value={`${lp.symbol} ${lp.side.toUpperCase()} ${lp.volume}`} />
            <MetaTile label="Floating" value={signedMoney(positionPnl(lp, quote(lp.symbol)))} tone="down" />
            <MetaTile label="Margin level" value={target.level === Infinity ? "—" : `${formatNumber(target.level, 0)}%`} tone={target.level < marginCallPct ? "down" : undefined} />
          </div>
        )}
      </DeskDialog>
    </div>
  );
}

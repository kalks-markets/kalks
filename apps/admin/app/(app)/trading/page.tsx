"use client";

import * as React from "react";
import Link from "next/link";
import { Activity, ArrowLeftRight, ChevronDown, Coins, Crosshair, GitBranch, Layers, MoreHorizontal, Plus, Scale, ScrollText, Scissors, TrendingUp, XCircle } from "lucide-react";
import { Button, Card, Chip, DataTable, Input, KpiCard, Menu, PageHeader, PriceText, Reveal, Segmented, SymbolCell, cn, formatNumber, useQuotes, type Column } from "@kalks/ui";
import { priceFeed } from "@kalks/mock";
import { PnlText, usdCompact } from "@/components/command/kit";
import { MiniClient, SOURCE_LABEL, SideChip, SourceTag, fmtPrice } from "@/components/trading/shared";
import { ago, bookAttribution, clientName, currentPriceOf, groupLabel, groupOptions, notionalUsd, positionPnl, serverStamp, useDesk, useLiveDirectory, type DeskPosition, type OrderSource } from "@/lib/trading-desk";
import { DeskStatusChip } from "@/components/trading-desk/status";
import { useCan } from "@/components/staff-session";
import { BookChip, Checkbox } from "@/components/trading-desk/kit";
import { BulkBar } from "@/components/trading-desk/bulk";
import { CreateTradeDrawer } from "@/components/trading-desk/create-trade";
import { PositionDrawer, type PositionDrawerView } from "@/components/trading-desk/position-drawer";

export default function PositionsPage() {
  const { state } = useDesk();
  useLiveDirectory();
  const canDeal = useCan("dealing.write");
  const positions = state.positions;
  const [symbol, setSymbol] = React.useState<string>("all");
  const [group, setGroup] = React.useState<string>("all");
  const [book, setBook] = React.useState<"all" | "A" | "B">("all");
  const [source, setSource] = React.useState<"all" | OrderSource>("all");
  const [login, setLogin] = React.useState("");
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [open, setOpen] = React.useState<string | null>(null);
  const [view, setView] = React.useState<PositionDrawerView | null>(null);
  const openAt = (ticket: string | null, at: PositionDrawerView | null = null) => {
    setView(at);
    setOpen(ticket);
  };
  const [create, setCreate] = React.useState(false);

  React.useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const t = params.get("ticket");
    if (t) setOpen(t);
    const l = params.get("login");
    if (l) setLogin(l);
    if (params.get("new") === "1") setCreate(true);
  }, []);

  const symbols = React.useMemo(() => Array.from(new Set(positions.map((p) => p.symbol))).sort(), [positions]);
  const sources = React.useMemo(() => Array.from(new Set(positions.map((p) => p.source))).sort(), [positions]);
  const qs = useQuotes(symbols);
  const quote = React.useCallback((s: string) => qs[s] ?? priceFeed().quote(s), [qs]);
  const pnlOf = (p: DeskPosition) => positionPnl(p, quote(p.symbol));

  const rows = positions.filter(
    (p) =>
      (symbol === "all" || p.symbol === symbol) &&
      (group === "all" || p.group === group) &&
      (book === "all" || p.route === book) &&
      (source === "all" || p.source === source) &&
      (!login.trim() || p.login.includes(login.trim())),
  );

  // prune selection when positions close
  React.useEffect(() => {
    setSelected((s) => {
      const alive = new Set(positions.map((p) => p.ticket));
      const next = new Set([...s].filter((t) => alive.has(t)));
      return next.size === s.size ? s : next;
    });
  }, [positions]);

  const sel = rows.filter((r) => selected.has(r.ticket));
  const allOn = rows.length > 0 && sel.length === rows.length;
  const toggle = (t: string, v: boolean) =>
    setSelected((s) => {
      const n = new Set(s);
      if (v) n.add(t);
      else n.delete(t);
      return n;
    });

  const byBook = (b: "A" | "B") => {
    const list = rows.filter((p) => p.route === b);
    let pnl = 0;
    let net = 0;
    let lots = 0;
    for (const p of list) {
      const q = quote(p.symbol);
      pnl += positionPnl(p, q);
      lots += p.volume;
      net += (p.side === "buy" ? 1 : -1) * notionalUsd(p.symbol, p.volume, (q.bid + q.ask) / 2 || p.openPrice, quote);
    }
    return { count: list.length, pnl, net, lots };
  };
  const A = byBook("A");
  const B = byBook("B");
  const brokerB = -rows.reduce((s, p) => s + bookAttribution(p, quote(p.symbol)).B, 0);
  const lots = A.lots + B.lots;

  const cols: Column<DeskPosition>[] = [
    {
      key: "sel",
      header: <Checkbox checked={allOn} indeterminate={sel.length > 0 && !allOn} onChange={(v) => setSelected(v ? new Set([...selected, ...rows.map((r) => r.ticket)]) : new Set([...selected].filter((t) => !rows.some((r) => r.ticket === t))))} label="Select all filtered positions" />,
      cell: (r) => <Checkbox checked={selected.has(r.ticket)} onChange={(v) => toggle(r.ticket, v)} label={`Select #${r.ticket}`} />,
      width: "36px",
    },
    {
      key: "t",
      header: "Ticket",
      cell: (r) => (
        <span className="inline-flex items-center gap-1 font-mono text-[12px]">
          {r.ticket}
          {r.parentTicket && (
            <span title={`Split from #${r.parentTicket}`}>
              <GitBranch className="size-3 text-info" />
            </span>
          )}
          {r.priceCorrected && <Chip size="sm" tone="gold">PC</Chip>}
        </span>
      ),
      sort: (r) => r.ticket,
    },
    { key: "c", header: "Client", cell: (r) => <MiniClient clientId={r.clientId} login={r.login} />, csv: (r) => `${clientName(r.clientId, r.login)} (${r.login})` },
    { key: "s", header: "Symbol", cell: (r) => <SymbolCell symbol={r.symbol} size={22} sub={<SideChip side={r.side} />} />, sort: (r) => r.symbol, csv: (r) => `${r.symbol} ${r.side}` },
    { key: "v", header: "Volume", align: "right", cell: (r) => <span className="k-num font-mono">{formatNumber(r.volume, 2)}</span>, sort: (r) => r.volume },
    { key: "o", header: "Open", align: "right", cell: (r) => <span className="k-num font-mono text-[12px] text-fg-2">{fmtPrice(r.symbol, r.openPrice)}</span>, csv: (r) => r.openPrice },
    {
      key: "cur",
      header: "Current",
      align: "right",
      cell: (r) => {
        const q = quote(r.symbol);
        return <PriceText symbol={r.symbol} value={currentPriceOf(r, q)} dir={q.dir} className="text-[12px]" />;
      },
      csv: (r) => currentPriceOf(r, quote(r.symbol)),
    },
    { key: "sltp", header: "S/L · T/P", align: "right", hideOn: "lg", cell: (r) => <span className="k-num whitespace-nowrap font-mono text-[11px] text-fg-3">{r.sl ? fmtPrice(r.symbol, r.sl) : "—"} · {r.tp ? fmtPrice(r.symbol, r.tp) : "—"}</span>, csv: (r) => `${r.sl ?? ""} / ${r.tp ?? ""}` },
    { key: "pnl", header: "P&L", align: "right", cell: (r) => <PnlText value={pnlOf(r)} className="font-mono text-[12.5px]" />, sort: (r) => pnlOf(r), csv: (r) => pnlOf(r).toFixed(2) },
    { key: "src", header: "Source", cell: (r) => <SourceTag source={r.source} platform={r.platform} />, csv: (r) => SOURCE_LABEL[r.source] },
    { key: "g", header: "Group", hideOn: "xl", cell: (r) => <Chip size="sm" tone={/^vip$/i.test(r.group) ? "gold" : /^prop$/i.test(r.group) ? "ember" : "neutral"}>{groupLabel(r.group)}</Chip>, csv: (r) => r.group },
    {
      key: "r",
      header: "Book",
      align: "center",
      cell: (r) => (
        <span className="inline-flex items-center gap-1">
          <BookChip book={r.route} />
          {r.routeHistory.length > 1 && <span className="k-num font-mono text-[10px] text-fg-3" title={`${r.routeHistory.length - 1} routing change(s)`}>{r.routeHistory.length - 1}×</span>}
        </span>
      ),
      sort: (r) => r.route,
    },
    { key: "ot", header: "Opened", align: "right", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={serverStamp(r.openTime)}>{ago(r.openTime)}</span>, sort: (r) => r.openTime },
    {
      key: "a",
      header: "",
      align: "right",
      cell: (r) => (
        <span onClick={(e) => e.stopPropagation()}>
          <Menu
            width={220}
            items={[
              { label: "Open ticket", icon: <Crosshair />, onSelect: () => openAt(r.ticket) },
              { label: "Modify SL / TP", icon: <Crosshair />, onSelect: () => openAt(r.ticket, { tab: "modify" }) },
              { label: "Partial close", icon: <Scissors />, onSelect: () => openAt(r.ticket, { tab: "close", closeMode: "partial" }) },
              { label: `Move to ${r.route === "A" ? "B" : "A"}-book`, icon: <ArrowLeftRight />, onSelect: () => openAt(r.ticket, { tab: "book" }) },
              "sep",
              { label: "Close / force close", icon: <XCircle />, danger: true, onSelect: () => openAt(r.ticket, { tab: "close", closeMode: "full" }) },
            ]}
            trigger={
              <button className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label={`Actions for #${r.ticket}`}>
                <MoreHorizontal className="size-4" />
              </button>
            }
          />
        </span>
      ),
    },
  ];

  const filterMenu = <T extends string>(label: string, value: T | "all", set: (v: T | "all") => void, opts: { value: T; label: string; hint?: string }[]) => (
    <Menu
      align="start"
      width={200}
      items={[{ label: `All ${label}`, onSelect: () => set("all") }, "sep", ...opts.map((o) => ({ label: o.label, hint: o.hint, onSelect: () => set(o.value) }))]}
      trigger={
        <Button size="sm" variant="surface" className={cn(value !== "all" && "border-ember/40")}>
          {value === "all" ? `All ${label}` : opts.find((o) => o.value === value)?.label ?? value} <ChevronDown />
        </Button>
      }
    />
  );

  return (
    <div className="pb-10">
      <PageHeader
        title="Open positions"
        subtitle={<span className="inline-flex flex-wrap items-center gap-2">All client positions — live P&L, A/B book and dealer actions, every change reason-coded and audited. <DeskStatusChip /></span>}
        actions={
          <>
            <Link href="/trading/dealer?tab=audit">
              <Button variant="surface" size="lg">
                <ScrollText /> Audit trail
              </Button>
            </Link>
            {canDeal && (
              <Button variant="ember" size="lg" onClick={() => setCreate(true)}>
                <Plus /> Create trade
              </Button>
            )}
          </>
        }
      />
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-3 2xl:grid-cols-6">
        <KpiCard label="Positions" icon={<Layers />} value={<span className="k-num">{rows.length}</span>} chip={`${new Set(rows.map((r) => r.clientId)).size} clients`} />
        <KpiCard label="Volume" icon={<Activity />} value={<span className="k-num">{formatNumber(lots, 1)}</span>} chip={`${formatNumber(A.lots, 1)} A · ${formatNumber(B.lots, 1)} B`} delay={0.03} />
        <KpiCard label="Client floating" icon={<TrendingUp />} value={<PnlText value={A.pnl + B.pnl} />} chip="Live" chipTone="ember" delay={0.06} />
        <KpiCard label="A-book floating" icon={<Scale />} value={<PnlText value={A.pnl} />} chip={`${A.count} pos · net ${usdCompact(A.net, 1)}`} chipTone="info" delay={0.09} />
        <KpiCard label="B-book floating" icon={<Scale />} value={<PnlText value={B.pnl} />} chip={`${B.count} pos · net ${usdCompact(B.net, 1)}`} delay={0.12} />
        <KpiCard label="Broker B-book P&L" icon={<Coins />} value={<PnlText value={brokerB} />} hot chip="attributed since routing" chipTone="ember" delay={0.15} />
      </div>
      <Reveal delay={0.1} className="mt-4">
        <Card className="px-4 py-5 sm:px-6">
          <BulkBar selected={sel} quote={quote} onClear={() => setSelected(new Set())} />
          <DataTable
            columns={cols}
            rows={rows}
            dense
            pageSize={14}
            rowKey={(r) => r.ticket}
            onRowClick={(r) => openAt(r.ticket)}
            exportName="open-positions"
            search={(r) => `${r.ticket} ${r.login} ${r.symbol} ${clientName(r.clientId, r.login)} ${r.parentTicket ?? ""}`}
            searchPlaceholder="Ticket, client…"
            toolbar={
              <div className="flex flex-wrap items-center gap-2">
                <Segmented size="sm" value={book} onChange={setBook} options={[{ value: "all", label: "A + B" }, { value: "A", label: "A-book" }, { value: "B", label: "B-book" }]} />
                {filterMenu("symbols", symbol, setSymbol, symbols.map((s) => ({ value: s, label: s, hint: String(positions.filter((x) => x.symbol === s).length) })))}
                {filterMenu("groups", group, setGroup, groupOptions())}
                {filterMenu("sources", source, setSource, sources.map((s) => ({ value: s, label: SOURCE_LABEL[s], hint: String(positions.filter((x) => x.source === s).length) })))}
                <Input value={login} onChange={(e) => setLogin(e.target.value.replace(/\D/g, ""))} placeholder="Login" aria-label="Filter by login" className="h-8 w-28 rounded-full font-mono text-[12.5px]" />
              </div>
            }
          />
        </Card>
      </Reveal>

      <PositionDrawer ticket={open} view={view} onOpenChange={(o) => !o && openAt(null)} onSelectTicket={(t) => openAt(t)} />
      <CreateTradeDrawer open={create} onOpenChange={setCreate} />
    </div>
  );
}

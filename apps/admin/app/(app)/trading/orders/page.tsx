"use client";

import * as React from "react";
import { Clock, Crosshair, ListOrdered, MoreHorizontal, Pencil, Plus, Trash2, XCircle, Zap } from "lucide-react";
import { Button, Card, Chip, DataTable, Field, Input, KpiCard, Menu, PageHeader, PriceText, Reveal, Segmented, SymbolCell, cn, formatNumber, useQuotes, type Column } from "@kalks/ui";
import { getInstrument, priceFeed } from "@kalks/mock";
import { MiniClient, SourceTag, fmtPrice } from "@/components/trading/shared";
import { ago, clientName, digitsOf, groupLabel, groupOptions, serverStamp, useDesk, useLiveDirectory, type DeskOrder } from "@/lib/trading-desk";
import { BookChip, Checkbox, DeskDialog, MetaTile, parseNum } from "@/components/trading-desk/kit";
import { CreateTradeDrawer } from "@/components/trading-desk/create-trade";
import { DeskStatusChip } from "@/components/trading-desk/status";

const tone = (t: DeskOrder["type"]) => (t.startsWith("Buy") ? "up" : "down");

/** Bulk cancel acts only on orders the dealer ticked (like the Positions bulk bar), never on a whole filter. */
type Act = { k: "cancel" | "modify" | "fill"; o: DeskOrder } | { k: "cancelMany"; orders: DeskOrder[] } | null;

export default function OrdersPage() {
  const { state, api } = useDesk();
  useLiveDirectory();
  const orders = state.orders;
  const [kind, setKind] = React.useState<"all" | "limit" | "stop">("all");
  const [group, setGroup] = React.useState<string>("all");
  const [act, setAct] = React.useState<Act>(null);
  const [create, setCreate] = React.useState(false);
  const [selected, setSelected] = React.useState<Set<string>>(new Set());
  const [edit, setEdit] = React.useState({ price: "", stopLimit: "", volume: "", sl: "", tp: "", expiry: "GTC" });
  const symbols = React.useMemo(() => Array.from(new Set(orders.map((o) => o.symbol))), [orders]);
  const qs = useQuotes(symbols);
  const rows = orders.filter((o) => (kind === "all" || o.type.toLowerCase().includes(kind)) && (group === "all" || o.group === group));

  React.useEffect(() => {
    setSelected((s) => {
      const alive = new Set(orders.map((o) => o.ticket));
      const next = new Set([...s].filter((t) => alive.has(t)));
      return next.size === s.size ? s : next;
    });
  }, [orders]);

  const dist = (o: DeskOrder) => {
    const q = qs[o.symbol] ?? priceFeed().quote(o.symbol);
    const mkt = o.type.startsWith("Buy") ? q.ask : q.bid;
    const digits = digitsOf(o.symbol);
    const pip = digits >= 4 ? 0.0001 : digits === 3 ? 0.01 : digits === 2 ? 0.1 : 1;
    return { pct: mkt ? ((o.price - mkt) / mkt) * 100 : 0, pips: (o.price - mkt) / pip, mkt, dir: q.dir };
  };

  const openModify = (o: DeskOrder) => {
    const d = digitsOf(o.symbol);
    setEdit({ price: o.price.toFixed(d), stopLimit: o.stopLimit ? o.stopLimit.toFixed(d) : "", volume: String(o.volume), sl: o.sl ? o.sl.toFixed(d) : "", tp: o.tp ? o.tp.toFixed(d) : "", expiry: o.expiry });
    setAct({ k: "modify", o });
  };

  const sel = rows.filter((r) => selected.has(r.ticket));
  const allOn = rows.length > 0 && sel.length === rows.length;

  const cols: Column<DeskOrder>[] = [
    {
      key: "sel",
      header: <Checkbox checked={allOn} indeterminate={sel.length > 0 && !allOn} onChange={(v) => setSelected(v ? new Set(rows.map((r) => r.ticket)) : new Set())} label="Select all filtered orders" />,
      cell: (r) => (
        <Checkbox
          checked={selected.has(r.ticket)}
          onChange={(v) =>
            setSelected((s) => {
              const n = new Set(s);
              if (v) n.add(r.ticket);
              else n.delete(r.ticket);
              return n;
            })
          }
          label={`Select order #${r.ticket}`}
        />
      ),
      width: "36px",
    },
    { key: "t", header: "Ticket", cell: (r) => <span className="font-mono text-[12px]">{r.ticket}</span>, sort: (r) => r.ticket },
    { key: "c", header: "Client", cell: (r) => <MiniClient clientId={r.clientId} login={r.login} />, csv: (r) => `${clientName(r.clientId, r.login)} (${r.login})` },
    { key: "s", header: "Symbol", cell: (r) => <SymbolCell symbol={r.symbol} size={22} sub={groupLabel(r.group)} />, sort: (r) => r.symbol },
    { key: "ty", header: "Type", cell: (r) => <Chip size="sm" tone={tone(r.type)}>{r.type}</Chip>, csv: (r) => r.type },
    { key: "v", header: "Volume", align: "right", cell: (r) => <span className="k-num font-mono">{formatNumber(r.volume, 2)}</span>, sort: (r) => r.volume },
    {
      key: "p",
      header: "Order price",
      align: "right",
      cell: (r) => (
        <span className="k-num whitespace-nowrap font-mono text-[12.5px]">
          {fmtPrice(r.symbol, r.price)}
          {r.stopLimit ? <span className="block text-[10.5px] text-fg-3">limit {fmtPrice(r.symbol, r.stopLimit)}</span> : null}
        </span>
      ),
      csv: (r) => r.price,
    },
    { key: "m", header: "Market", align: "right", cell: (r) => { const d = dist(r); return <PriceText symbol={r.symbol} value={d.mkt} dir={d.dir} className="text-[12px]" />; }, csv: (r) => dist(r).mkt },
    {
      key: "d",
      header: "Distance",
      align: "right",
      sort: (r) => Math.abs(dist(r).pct),
      cell: (r) => {
        const d = dist(r);
        const near = Math.abs(d.pct) < 0.3;
        const cls = getInstrument(r.symbol).assetClass;
        return (
          <span className={cn("k-num whitespace-nowrap font-mono text-[12px]", near ? "text-warn" : "text-fg-2")}>
            {d.pips >= 0 ? "+" : ""}
            {formatNumber(d.pips, 1)} {cls === "forex" || cls === "metals" ? "pips" : "pts"} <span className="text-fg-3">({d.pct >= 0 ? "+" : ""}{d.pct.toFixed(2)}%)</span>
          </span>
        );
      },
    },
    { key: "sltp", header: "S/L · T/P", align: "right", hideOn: "lg", cell: (r) => <span className="whitespace-nowrap font-mono text-[11px] text-fg-3">{r.sl ? fmtPrice(r.symbol, r.sl) : "—"} · {r.tp ? fmtPrice(r.symbol, r.tp) : "—"}</span>, csv: (r) => `${r.sl ?? ""} / ${r.tp ?? ""}` },
    { key: "src", header: "Source", hideOn: "xl", cell: (r) => <span className="inline-flex items-center gap-1.5"><SourceTag source={r.source ?? "manual"} />{r.book && <BookChip book={r.book} />}</span>, csv: (r) => r.source ?? "manual" },
    { key: "e", header: "Expiry", cell: (r) => <span className="whitespace-nowrap text-[12px] text-fg-2">{r.expiry.startsWith("20") ? new Date(r.expiry.length > 10 ? r.expiry : `${r.expiry}T12:00:00Z`).toLocaleDateString("en-GB", { day: "2-digit", month: "short" }) : r.expiry}</span>, csv: (r) => r.expiry },
    { key: "pl", header: "Placed", align: "right", cell: (r) => <span className="whitespace-nowrap text-[11.5px] text-fg-3" title={serverStamp(r.placed)}>{ago(r.placed)}</span>, sort: (r) => r.placed },
    {
      key: "a",
      header: "",
      align: "right",
      cell: (r) => (
        <span onClick={(e) => e.stopPropagation()}>
          <Menu
            width={200}
            items={[
              { label: "Modify order", icon: <Pencil />, onSelect: () => openModify(r) },
              { label: "Fill at market", icon: <Zap />, onSelect: () => setAct({ k: "fill", o: r }) },
              "sep",
              { label: "Cancel order", icon: <Trash2 />, danger: true, onSelect: () => setAct({ k: "cancel", o: r }) },
            ]}
            trigger={
              <button className="grid size-7 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label={`Actions for order #${r.ticket}`}>
                <MoreHorizontal className="size-4" />
              </button>
            }
          />
        </span>
      ),
    },
  ];

  const o = act && act.k !== "cancelMany" ? act.o : null;
  const many = act?.k === "cancelMany" ? act.orders : [];
  const near = rows.filter((r) => Math.abs(dist(r).pct) < 0.3).length;
  const close = (v: boolean) => !v && setAct(null);
  return (
    <div className="pb-10">
      <PageHeader
        title="Pending orders"
        subtitle={<span className="inline-flex flex-wrap items-center gap-2">Limit, stop and stop-limit orders with live distance to market. <DeskStatusChip /></span>}
        actions={
          <>
            <Button variant="down-outline" size="lg" disabled={!sel.length} title={sel.length ? undefined : "Tick the orders to cancel first"} onClick={() => setAct({ k: "cancelMany", orders: sel })}>
              <XCircle /> Cancel selected ({sel.length})
            </Button>
            <Button variant="ember" size="lg" onClick={() => setCreate(true)}>
              <Plus /> New order
            </Button>
          </>
        }
      />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        <KpiCard label="Pending orders" icon={<ListOrdered />} value={<span className="k-num">{rows.length}</span>} chip={`${new Set(rows.map((r) => r.clientId)).size} clients`} />
        <KpiCard label="Volume" icon={<Crosshair />} value={<span className="k-num">{formatNumber(rows.reduce((s, r) => s + r.volume, 0), 1)}</span>} chip="lots" delay={0.04} />
        <KpiCard label="Near trigger" icon={<Crosshair />} value={<span className="k-num text-warn">{near}</span>} chip="within 0.3% of market" chipTone="warn" delay={0.08} />
        <KpiCard label="Expiring today" icon={<Clock />} value={<span className="k-num">{rows.filter((r) => r.expiry === "Today").length}</span>} chip="23:59 GMT+3" delay={0.12} />
      </div>
      <Reveal delay={0.1} className="mt-4">
        <Card className="px-4 py-5 sm:px-6">
          <DataTable
            columns={cols}
            rows={rows}
            dense
            pageSize={14}
            rowKey={(r) => r.ticket}
            onRowClick={openModify}
            exportName="pending-orders"
            search={(r) => `${r.ticket} ${r.login} ${r.symbol} ${clientName(r.clientId, r.login)}`}
            toolbar={
              <div className="flex flex-wrap gap-2">
                <Segmented size="sm" value={kind} onChange={setKind} options={[{ value: "all", label: "All types" }, { value: "limit", label: "Limit" }, { value: "stop", label: "Stop" }]} />
                <Segmented size="sm" value={group} onChange={setGroup} options={[{ value: "all", label: "All groups" }, ...groupOptions()]} />
              </div>
            }
          />
        </Card>
      </Reveal>

      <CreateTradeDrawer open={create} onOpenChange={setCreate} initialType="limit" />

      {o && (
        <>
          <DeskDialog
            open={act?.k === "cancel"}
            onOpenChange={close}
            title={`Cancel order #${o.ticket}`}
            description={`${o.type} ${o.volume} ${o.symbol} @ ${fmtPrice(o.symbol, o.price)} · ${clientName(o.clientId, o.login)}`}
            confirmLabel="Cancel order"
            confirmVariant="sell"
            onConfirm={(r) => api.cancelOrders([o.ticket], r)}
            success={`Order #${o.ticket} cancelled`}
          />
          <DeskDialog
            open={act?.k === "fill"}
            onOpenChange={close}
            title={`Fill order #${o.ticket} at market`}
            description={`${o.type} ${o.volume} ${o.symbol} · ${clientName(o.clientId, o.login)}. Opens a position at the live price; account, symbol and margin controls apply.`}
            confirmLabel="Fill now"
            confirmVariant={o.type.startsWith("Buy") ? "buy" : "sell"}
            onConfirm={(r) => api.fillOrder(o.ticket, r)}
            success={(d) => `Order #${o.ticket} filled → position #${d.ticket}`}
          >
            <div className="grid grid-cols-2 gap-2">
              <MetaTile label="Order price" value={fmtPrice(o.symbol, o.price)} />
              <MetaTile label="Market" value={<PriceText symbol={o.symbol} value={dist(o).mkt} dir={dist(o).dir} className="text-[13px]" />} />
            </div>
          </DeskDialog>
          <DeskDialog
            open={act?.k === "modify"}
            onOpenChange={close}
            title={`Modify order #${o.ticket}`}
            description={`${o.type} ${o.symbol} · ${clientName(o.clientId, o.login)} · ${o.login}`}
            confirmLabel="Apply changes"
            onConfirm={(r) =>
              api.modifyOrder(
                o.ticket,
                { price: parseNum(edit.price), volume: Number(edit.volume) || undefined, stopLimit: o.type.includes("Stop Limit") ? parseNum(edit.stopLimit) : undefined, sl: parseNum(edit.sl) ?? null, tp: parseNum(edit.tp) ?? null, expiry: edit.expiry },
                r,
              )
            }
            success={`Order #${o.ticket} modified`}
          >
            <div className="grid grid-cols-2 gap-3">
              <Field label={o.type.includes("Stop") ? "Stop price" : "Limit price"} hint={`mkt ${fmtPrice(o.symbol, dist(o).mkt)}`}>
                <Input value={edit.price} onChange={(e) => setEdit({ ...edit, price: e.target.value })} aria-label="Order price" className="font-mono" />
              </Field>
              <Field label="Volume">
                <Input value={edit.volume} onChange={(e) => setEdit({ ...edit, volume: e.target.value })} aria-label="Volume" className="font-mono" trailing="lots" />
              </Field>
              {o.type.includes("Stop Limit") && (
                <Field label="Limit price">
                  <Input value={edit.stopLimit} onChange={(e) => setEdit({ ...edit, stopLimit: e.target.value })} aria-label="Stop-limit price" className="font-mono" />
                </Field>
              )}
              <Field label="Stop loss">
                <Input value={edit.sl} onChange={(e) => setEdit({ ...edit, sl: e.target.value })} placeholder="None" aria-label="Stop loss" className="font-mono" />
              </Field>
              <Field label="Take profit">
                <Input value={edit.tp} onChange={(e) => setEdit({ ...edit, tp: e.target.value })} placeholder="None" aria-label="Take profit" className="font-mono" />
              </Field>
            </div>
            <div className="flex items-center gap-3">
              <span className="text-[12.5px] font-medium text-fg-2">Expiry</span>
              <Segmented size="xs" value={edit.expiry === "GTC" || edit.expiry === "Today" ? edit.expiry : "date"} onChange={(v) => v !== "date" && setEdit({ ...edit, expiry: v })} options={[{ value: "GTC", label: "GTC" }, { value: "Today", label: "Today" }, ...(edit.expiry.startsWith("20") ? [{ value: "date", label: edit.expiry }] : [])]} />
            </div>
          </DeskDialog>
        </>
      )}
      <DeskDialog
        open={act?.k === "cancelMany"}
        onOpenChange={close}
        title={`Cancel ${many.length} selected order${many.length === 1 ? "" : "s"}`}
        description="Only the orders you ticked are cancelled; each one gets its own audit entry. Orders you did not select stay in place."
        confirmLabel={`Cancel ${many.length} order${many.length === 1 ? "" : "s"}`}
        confirmVariant="sell"
        disabled={!many.length && "Nothing selected"}
        onConfirm={(r) => api.cancelOrders(many.map((x) => x.ticket), r)}
        success={(d) => `${d.done.length} orders cancelled${d.failed.length ? ` · ${d.failed.length} skipped` : ""}`}
      >
        <div className="grid grid-cols-3 gap-2">
          <MetaTile label="Orders" value={many.length} />
          <MetaTile label="Clients" value={new Set(many.map((x) => x.clientId)).size} />
          <MetaTile label="Symbols" value={new Set(many.map((x) => x.symbol)).size} />
        </div>
      </DeskDialog>
    </div>
  );
}

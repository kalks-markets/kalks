"use client";

import * as React from "react";
import { ArrowRight, GitBranch, History, Undo2 } from "lucide-react";
import { Button, Chip, Dialog, DialogClose, Field, Input, PriceText, Segmented, SymbolCell, Tabs, Toggle, cn, formatNumber, useQuote } from "@kalks/ui";
import { getInstrument } from "@kalks/mock";
import {
  REASON_ERROR_CORRECTION,
  ago,
  bookAttribution,
  clientName,
  currentPriceOf,
  groupLabel,
  positionPnl,
  roundVol,
  serverStamp,
  symbolSpec,
  useDesk,
  type AuditEntry,
  type DeskPosition,
  type DeskResult,
} from "@/lib/trading-desk";
import { SideChip, SourceTag } from "@/components/trading/shared";
import { AuditNotice, BookChip, ErrorBanner, MetaTile, ReasonFields, Stepper, parseNum, reportResult, signedMoney, useReason } from "./kit";
import { actionText } from "./labels";

type Tab = "modify" | "close" | "book" | "adjust" | "timeline";
type AdjustMode = "add" | "charges" | "price" | "void";
/** Where a row action opens the drawer (e.g. "Partial close" → Close tab, partial). Omitted: the SL / TP tab. */
export type PositionDrawerView = { tab: Tab; closeMode?: "partial" | "full" };

export function PositionDrawer({
  ticket,
  view,
  onOpenChange,
  onSelectTicket,
}: {
  ticket: string | null;
  view?: PositionDrawerView | null;
  onOpenChange: (o: boolean) => void;
  onSelectTicket?: (t: string) => void;
}) {
  const { state, api } = useDesk();
  const live = ticket ? state.positions.find((p) => p.ticket === ticket) : undefined;
  const last = React.useRef<DeskPosition | null>(null);
  if (live) last.current = live;
  const p = live ?? (ticket && last.current?.ticket === ticket ? last.current : null);
  const closed = !!ticket && !live;

  const [tab, setTab] = React.useState<Tab>("modify");
  const r = useReason();
  const [error, setError] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  // form state
  const [sl, setSl] = React.useState("");
  const [tp, setTp] = React.useState("");
  const [closeMode, setCloseMode] = React.useState<"partial" | "full">("partial");
  const [closeVol, setCloseVol] = React.useState("");
  const [atPrice, setAtPrice] = React.useState(false);
  const [closePx, setClosePx] = React.useState("");
  const [force, setForce] = React.useState(false);
  const [moveMode, setMoveMode] = React.useState<"full" | "partial">("full");
  const [moveVol, setMoveVol] = React.useState("");
  const [adjust, setAdjust] = React.useState<AdjustMode>("add");
  const [addVol, setAddVol] = React.useState("");
  const [swap, setSwap] = React.useState("");
  const [comm, setComm] = React.useState("");
  const [openPx, setOpenPx] = React.useState("");

  const { reset } = r;
  const init = React.useCallback(
    (x: DeskPosition) => {
      const d = getInstrument(x.symbol).digits;
      const spec = symbolSpec(x.symbol);
      const half = roundVol(x.symbol, Math.max(spec.min, x.volume / 2));
      const fmt = (v: number) => v.toFixed(spec.step >= 1 ? 0 : spec.step >= 0.1 ? 1 : 2);
      setSl(x.sl ? x.sl.toFixed(d) : "");
      setTp(x.tp ? x.tp.toFixed(d) : "");
      setCloseVol(fmt(half));
      setMoveVol(fmt(half));
      setAddVol(fmt(spec.min));
      setSwap(x.swap.toFixed(2));
      setComm(x.commission.toFixed(2));
      setOpenPx(x.openPrice.toFixed(d));
      setClosePx("");
      setAtPrice(false);
      setForce(false);
    },
    [],
  );
  // re-init when another ticket opens, or after an action changes the position
  const sig = p ? `${p.ticket}|${p.volume}|${p.sl}|${p.tp}|${p.openPrice}|${p.swap}|${p.commission}|${p.route}` : "";
  React.useEffect(() => {
    if (p) init(p);
    setError(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sig]);
  React.useEffect(() => {
    // a different ticket starts from the default sub-modes
    setCloseMode("partial");
    setMoveMode("full");
    setAdjust("add");
    if (!ticket) {
      setTab("modify");
      reset();
    }
  }, [ticket, reset]);
  React.useEffect(() => {
    // after the reset above: open on the tab the row action named
    if (!ticket || !view) return;
    setTab(view.tab);
    if (view.closeMode) setCloseMode(view.closeMode);
  }, [ticket, view]);
  const priceCorrectionPath = (tab === "close" && atPrice) || (tab === "adjust" && adjust === "price");
  React.useEffect(() => {
    if (priceCorrectionPath) r.setCode(REASON_ERROR_CORRECTION);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [priceCorrectionPath]);

  const q = useQuote(p?.symbol ?? "EURUSD");
  if (!p) return null;
  const inst = getInstrument(p.symbol);
  const client = { name: clientName(p.clientId, p.login) };
  const pnl = positionPnl(p, q);
  const attr = bookAttribution(p, q);
  const cur = currentPriceOf(p, q);
  const other = p.route === "A" ? "B" : "A";
  const fmt = (v: number) => formatNumber(v, inst.digits);
  // price corrections (close at price, open-price correction) and voids need a note (engine rule)
  const noteMissing = (priceCorrectionPath || (tab === "adjust" && adjust === "void")) && !r.note.trim();

  const run = async <T,>(fn: () => Promise<DeskResult<T>>, msg: string | ((d: T) => string)) => {
    setBusy(true);
    setError(null);
    const res = await fn();
    setBusy(false);
    if (!res.ok) {
      setError(res.error);
      reportResult(res, "");
      return false;
    }
    reportResult(res, msg);
    reset();
    return true;
  };

  const action = (() => {
    if (closed) return null;
    switch (tab) {
      case "modify":
        return { label: "Apply SL / TP", variant: "ember" as const, go: () => run(() => api.modifyPosition(p.ticket, { sl: parseNum(sl) ?? null, tp: parseNum(tp) ?? null }, r.reason), `SL/TP updated on #${p.ticket}`) };
      case "close": {
        const px = atPrice ? parseNum(closePx) : undefined;
        if (closeMode === "partial") {
          const v = Number(closeVol) || 0;
          return { label: `Close ${closeVol} lots`, variant: "sell" as const, go: () => run(() => api.partialClose(p.ticket, v, r.reason, { price: px }), (d) => `${v} lots closed on #${p.ticket} · ${signedMoney(d.profit)}`) };
        }
        return { label: force ? "Force close" : "Close position", variant: "sell" as const, go: () => run(() => api.closePosition(p.ticket, r.reason, { price: px, force }), (d) => `#${p.ticket} closed · ${signedMoney(d.profit)}`) };
      }
      case "book": {
        const v = moveMode === "full" ? p.volume : Number(moveVol) || 0;
        return {
          label: `Move ${moveMode === "full" ? p.volume : moveVol} lots to ${other}-book`,
          variant: "ember" as const,
          go: () =>
            run(
              () => api.transferBook([p.ticket], other, r.reason, moveMode === "full" ? {} : { volume: v }),
              (d) => (d.created.length ? `Split: #${d.created[0]} (${v} lots) now on ${other}-book` : d.failed.length ? d.failed[0]!.error : `#${p.ticket} moved to ${other}-book`),
            ),
        };
      }
      case "adjust":
        if (adjust === "add") return { label: `Add ${addVol} lots at market`, variant: p.side === "buy" ? ("buy" as const) : ("sell" as const), go: () => run(() => api.addVolume(p.ticket, Number(addVol) || 0, r.reason), `${addVol} lots added to #${p.ticket}`) };
        if (adjust === "charges") return { label: "Apply adjustment", variant: "ember" as const, go: () => run(() => api.adjustCharges(p.ticket, { swap: parseNum(swap), commission: parseNum(comm) }, r.reason), `Charges adjusted on #${p.ticket}`) };
        if (adjust === "price") return { label: "Apply price correction", variant: "gold" as const, go: () => run(() => api.priceCorrection(p.ticket, parseNum(openPx) ?? 0, r.reason), `Open price corrected on #${p.ticket}`) };
        return { label: "Void trade", variant: "sell" as const, go: () => run(() => api.voidPosition(p.ticket, r.reason), `#${p.ticket} voided`) };
      default:
        return null;
    }
  })();

  return (
    <Dialog
      open={!!ticket}
      onOpenChange={onOpenChange}
      side="right"
      title={
        <span className="flex items-center gap-2">
          Position <span className="font-mono">#{p.ticket}</span> <BookChip book={p.route} size="md" />
          {closed && <Chip tone="neutral">Closed</Chip>}
          {p.priceCorrected && <Chip tone="gold" size="sm">Price corrected</Chip>}
        </span>
      }
      description={`${client.name} · ${p.login} · ${groupLabel(p.group)}`}
      footer={
        <>
          <span className="mr-auto max-w-[220px] truncate text-[11.5px] text-fg-3">{action && (r.error ?? (noteMissing ? "A note is required" : null))}</span>
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Done
            </Button>
          </DialogClose>
          {action && (
            <Button variant={action.variant} size="sm" disabled={!!r.error || noteMissing || busy} onClick={() => void action.go()}>
              {busy ? "Working…" : action.label}
            </Button>
          )}
        </>
      }
    >
      <div className="space-y-5">
        <div className="k-row flex items-center justify-between gap-3 px-4 py-3">
          <SymbolCell symbol={p.symbol} size={30} sub={<span className="flex items-center gap-1.5"><SideChip side={p.side} volume={p.volume} /> <SourceTag source={p.source} platform={p.platform} /></span>} />
          <div className="text-right">
            <div className="text-[11px] text-fg-3">{closed ? "Last P&L" : "Live P&L"}</div>
            <div className={cn("k-num font-mono text-[18px] font-medium", pnl >= 0 ? "text-up" : "text-down")}>{signedMoney(pnl)}</div>
          </div>
        </div>
        <div className="grid grid-cols-4 gap-2">
          <MetaTile label="Open" value={fmt(p.openPrice)} />
          <MetaTile label="Current" value={<PriceText symbol={p.symbol} value={cur} dir={q.dir} className="text-[13px]" />} />
          <MetaTile label="S/L" value={p.sl ? fmt(p.sl) : "—"} />
          <MetaTile label="T/P" value={p.tp ? fmt(p.tp) : "—"} />
          <MetaTile label="Swap" value={signedMoney(p.swap)} />
          <MetaTile label="Commission" value={signedMoney(-p.commission)} />
          <MetaTile label="Opened" value={ago(p.openTime)} />
          <MetaTile label={`${p.route}-book since`} value={ago(p.bookSince)} />
        </div>
        {(p.parentTicket || p.childTickets?.length || p.comment) && (
          <div className="flex flex-wrap items-center gap-2 text-[12px] text-fg-3">
            {p.parentTicket && (
              <button type="button" onClick={() => onSelectTicket?.(p.parentTicket!)} className="inline-flex items-center gap-1 rounded-full border border-line bg-surface-2 px-2.5 py-1 hover:text-fg">
                <GitBranch className="size-3" /> split from <span className="font-mono">#{p.parentTicket}</span>
              </button>
            )}
            {p.childTickets?.map((c) => (
              <button key={c} type="button" onClick={() => onSelectTicket?.(c)} className="inline-flex items-center gap-1 rounded-full border border-line bg-surface-2 px-2.5 py-1 hover:text-fg">
                <GitBranch className="size-3" /> child <span className="font-mono">#{c}</span>
              </button>
            ))}
            {p.comment && <span className="truncate">“{p.comment}”</span>}
          </div>
        )}

        <Tabs
          value={tab}
          onChange={(t) => {
            setTab(t);
            setError(null);
          }}
          tabs={[
            { value: "modify", label: "SL / TP" },
            { value: "close", label: "Close" },
            { value: "book", label: "Book" },
            { value: "adjust", label: "Adjust" },
            { value: "timeline", label: "Timeline" },
          ]}
        />

        {closed && tab !== "timeline" ? (
          <div className="rounded-[14px] border border-line bg-surface-2 px-4 py-3 text-[13px] text-fg-2">This position is closed. Open the Timeline tab to review its deals or reopen it.</div>
        ) : tab === "modify" ? (
          <div className="grid grid-cols-2 gap-3">
            <Field label="Stop loss" hint={p.side === "buy" ? "below bid" : "above ask"}>
              <Input value={sl} onChange={(e) => setSl(e.target.value)} placeholder="None" aria-label="Stop loss" className="font-mono" />
            </Field>
            <Field label="Take profit" hint={p.side === "buy" ? "above bid" : "below ask"}>
              <Input value={tp} onChange={(e) => setTp(e.target.value)} placeholder="None" aria-label="Take profit" className="font-mono" />
            </Field>
            <div className="col-span-2 flex gap-2">
              <Button size="xs" variant="surface" onClick={() => setSl("")}>Remove SL</Button>
              <Button size="xs" variant="surface" onClick={() => setTp("")}>Remove TP</Button>
              <Button size="xs" variant="surface" onClick={() => setSl(p.openPrice.toFixed(inst.digits))}>SL to breakeven</Button>
            </div>
          </div>
        ) : tab === "close" ? (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-3">
              <Segmented size="sm" value={closeMode} onChange={setCloseMode} options={[{ value: "partial", label: "Partial" }, { value: "full", label: `Full ${p.volume}` }]} />
              <Segmented size="sm" value={atPrice ? "price" : "market"} onChange={(v) => setAtPrice(v === "price")} options={[{ value: "market", label: "At market" }, { value: "price", label: "At price (correction)" }]} />
            </div>
            <div className="grid grid-cols-2 gap-3">
              {closeMode === "partial" && (
                <div>
                  <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Volume to close <span className="font-normal text-fg-3">of {p.volume}</span></div>
                  <Stepper value={closeVol} onChange={setCloseVol} step={symbolSpec(p.symbol).step} min={symbolSpec(p.symbol).min} digits={symbolSpec(p.symbol).step >= 1 ? 0 : symbolSpec(p.symbol).step >= 0.1 ? 1 : 2} suffix="lots" ariaLabel="Volume to close" />
                </div>
              )}
              {atPrice ? (
                <Field label="Close price" hint={`mkt ${fmt(cur)}`}>
                  <Input value={closePx} onChange={(e) => setClosePx(e.target.value)} placeholder={fmt(cur)} aria-label="Close price" className="border-gold/40 font-mono" />
                </Field>
              ) : (
                <MetaTile label="Closes at market" value={<PriceText symbol={p.symbol} value={cur} dir={q.dir} className="text-[14px]" />} className="py-2.5" />
              )}
            </div>
            {closeMode === "full" && (
              <label className="flex items-center justify-between rounded-[14px] border border-line bg-surface-2 px-3.5 py-2.5 text-[12.5px]">
                <span>
                  <span className="font-medium text-fg">Force close</span> <span className="text-fg-3">— ignores a symbol halt; flagged “forced”</span>
                </span>
                <Toggle checked={force} onChange={setForce} label="Force close" />
              </label>
            )}
            {atPrice && <div className="rounded-[14px] border border-gold/30 bg-gold-soft px-3.5 py-2.5 text-[12px] text-fg-2">Closing at a specified price is an error correction: reason DLR-02, note required, shown as “price correction” on the client statement.</div>}
          </div>
        ) : tab === "book" ? (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2 text-[13px]">
              <BookChip book={p.route} size="md" /> <ArrowRight className="size-3.5 text-fg-3" /> <BookChip book={other} size="md" />
              <Segmented size="sm" className="ml-2" value={moveMode} onChange={setMoveMode} options={[{ value: "full", label: "Full volume" }, { value: "partial", label: "Partial (split ticket)" }]} />
            </div>
            {moveMode === "partial" && (
              <div className="grid grid-cols-2 items-end gap-3">
                <div>
                  <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Volume to move <span className="font-normal text-fg-3">of {p.volume}</span></div>
                  <Stepper value={moveVol} onChange={setMoveVol} step={symbolSpec(p.symbol).step} min={symbolSpec(p.symbol).min} digits={symbolSpec(p.symbol).step >= 1 ? 0 : symbolSpec(p.symbol).step >= 0.1 ? 1 : 2} suffix="lots" ariaLabel="Volume to move" />
                </div>
                <div className="flex gap-1.5 pb-1.5">
                  {[25, 50, 75].map((pct) => (
                    <Button key={pct} size="xs" variant="surface" onClick={() => setMoveVol(String(roundVol(p.symbol, (p.volume * pct) / 100)))}>
                      {pct}%
                    </Button>
                  ))}
                </div>
              </div>
            )}
            <div className="text-[12px] leading-relaxed text-fg-3">
              {moveMode === "partial"
                ? `Splits #${p.ticket}: it keeps the remaining volume on the ${p.route}-book; a linked child ticket carries the moved volume on the ${other}-book. Both keep the open price and time.`
                : `The whole ticket moves to the ${other}-book.`}{" "}
              Book P&L is attributed from the transfer at the current market ({fmt(cur)}).
            </div>
            <div className="grid grid-cols-3 gap-2">
              <MetaTile label="Client P&L on A" value={signedMoney(attr.A)} tone={attr.A >= 0 ? "up" : "down"} />
              <MetaTile label="Client P&L on B" value={signedMoney(attr.B)} tone={attr.B >= 0 ? "up" : "down"} />
              <MetaTile label="Broker B-book P&L" value={signedMoney(-attr.B)} tone={-attr.B >= 0 ? "up" : "down"} />
            </div>
            <RouteHistory p={p} />
          </div>
        ) : tab === "adjust" ? (
          <div className="space-y-4">
            <Segmented size="sm" value={adjust} onChange={setAdjust} options={[{ value: "add", label: "Add volume" }, { value: "charges", label: "Swap / commission" }, { value: "price", label: "Price correction" }, { value: "void", label: "Void" }]} />
            {adjust === "add" ? (
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Add volume</div>
                  <Stepper value={addVol} onChange={setAddVol} step={symbolSpec(p.symbol).step} min={symbolSpec(p.symbol).min} digits={symbolSpec(p.symbol).step >= 1 ? 0 : symbolSpec(p.symbol).step >= 0.1 ? 1 : 2} suffix="lots" ariaLabel="Add volume" />
                </div>
                <div className="text-[12px] leading-relaxed text-fg-3">Fills at market and averages the open price. Margin, account and symbol controls are checked.</div>
              </div>
            ) : adjust === "charges" ? (
              <div className="grid grid-cols-2 gap-3">
                <Field label="Swap (USD)" hint={`now ${signedMoney(p.swap)}`}>
                  <Input value={swap} onChange={(e) => setSwap(e.target.value)} aria-label="Swap" className="font-mono" />
                </Field>
                <Field label="Commission (USD)" hint={`now ${formatNumber(p.commission, 2)}`}>
                  <Input value={comm} onChange={(e) => setComm(e.target.value)} aria-label="Commission" className="font-mono" />
                </Field>
              </div>
            ) : adjust === "price" ? (
              <div className="space-y-3">
                <Field label="Corrected open price" hint={`was ${fmt(p.openPrice)}`}>
                  <Input value={openPx} onChange={(e) => setOpenPx(e.target.value)} aria-label="Corrected open price" className="border-gold/40 font-mono" />
                </Field>
                <div className="rounded-[14px] border border-gold/30 bg-gold-soft px-3.5 py-2.5 text-[12px] text-fg-2">Restricted to error correction (DLR-02) with a note. The client statement shows the ticket as “price correction”.</div>
              </div>
            ) : (
              <div className="rounded-[14px] border border-down/30 bg-down-soft px-3.5 py-2.5 text-[12px] text-fg-2">Voiding removes the ticket as if it never existed — no P&L is booked. Allowed for error correction (DLR-02) or technical issue (DLR-06) with a note.</div>
            )}
          </div>
        ) : (
          <Timeline ticket={p.ticket} parent={p.parentTicket} audit={state.audit} p={p} onReopen={(dealId) => run(() => api.reopenDeal(dealId, r.reason), (d) => `#${d.ticket} reopened`)} reasonReady={!r.error} busy={busy} deals={state.deals.filter((d) => d.ticket === p.ticket)} />
        )}

        {(tab !== "timeline" || state.deals.some((d) => d.ticket === p.ticket && !d.reversed)) && !(closed && tab !== "timeline") && (
          <ReasonFields r={r} codes={priceCorrectionPath ? [REASON_ERROR_CORRECTION] : undefined} noteRequiredHint={priceCorrectionPath || (tab === "adjust" && adjust === "void")} />
        )}
        <ErrorBanner error={error} />
        {tab !== "timeline" && !closed && <AuditNotice />}
      </div>
    </Dialog>
  );
}

function RouteHistory({ p }: { p: DeskPosition }) {
  const d = getInstrument(p.symbol).digits;
  return (
    <div>
      <div className="mb-2 text-[11px] uppercase tracking-wider text-fg-3">Routing history</div>
      <div className="space-y-1.5">
        {[...p.routeHistory].reverse().map((e, i) => (
          <div key={i} className="k-row flex items-center gap-2.5 px-3 py-2 text-[12px]">
            {e.from ? <BookChip book={e.from} /> : <span className="w-5 text-center text-fg-3">·</span>}
            <ArrowRight className="size-3 text-fg-3" />
            <BookChip book={e.to} />
            <span className="flex-1 truncate text-fg-2">
              {e.kind === "open" ? "Opened" : e.kind === "transfer" ? "Transferred" : e.kind === "split-out" ? `Split out → #${e.relatedTicket}` : `Split in ← #${e.relatedTicket}`} · <span className="k-num font-mono">{e.volume}</span> lots @ <span className="k-num font-mono">{formatNumber(e.price, d)}</span>
            </span>
            <span className="whitespace-nowrap text-[11px] text-fg-3">
              {e.staff} · {ago(e.at)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

function Timeline({ ticket, parent, audit, p, deals, onReopen, reasonReady, busy }: { ticket: string; parent?: string; audit: AuditEntry[]; p: DeskPosition; deals: { id: string; volume: number; closePrice: number; profit: number; reversed?: boolean; closeTime: string; kind: string }[]; onReopen: (dealId: string) => void; reasonReady: boolean; busy: boolean }) {
  const splitAt = p.routeHistory.find((e) => e.kind === "split-in")?.at;
  const items = audit.filter((a) => a.tickets.includes(ticket) || (parent && splitAt && a.tickets.includes(parent) && a.at < splitAt));
  const d = getInstrument(p.symbol).digits;
  const opened = p.routeHistory[0];
  return (
    <div className="space-y-4">
      {deals.length > 0 && (
        <div>
          <div className="mb-2 text-[11px] uppercase tracking-wider text-fg-3">Closing deals</div>
          <div className="space-y-1.5">
            {deals.map((x) => (
              <div key={x.id} className="k-row flex items-center gap-3 px-3 py-2 text-[12px]">
                <span className="font-mono text-fg-2">{x.id}</span>
                <span className="flex-1 text-fg-3">
                  {x.kind} · <span className="k-num font-mono text-fg-2">{x.volume}</span> @ <span className="k-num font-mono text-fg-2">{formatNumber(x.closePrice, d)}</span> · {ago(x.closeTime)}
                </span>
                <span className={cn("k-num font-mono", x.profit >= 0 ? "text-up" : "text-down")}>{signedMoney(x.profit)}</span>
                {x.reversed ? (
                  <Chip size="sm">Reopened</Chip>
                ) : (
                  <Button size="xs" variant="surface" disabled={!reasonReady || busy} onClick={() => onReopen(x.id)}>
                    <Undo2 /> Reopen
                  </Button>
                )}
              </div>
            ))}
          </div>
        </div>
      )}
      <div>
        <div className="mb-2 flex items-center gap-1.5 text-[11px] uppercase tracking-wider text-fg-3">
          <History className="size-3" /> Ticket timeline
        </div>
        <ol className="relative space-y-3 border-l border-line pl-4">
          {items.map((a) => (
            <li key={a.id} className="relative">
              <span className={cn("absolute -left-[21px] top-1.5 size-2.5 rounded-full border-2 border-surface", a.flags?.includes("price correction") ? "bg-gold" : a.action === "trade.rejected" ? "bg-down" : a.action.startsWith("book") ? "bg-info" : "bg-ember")} />
              <div className="flex flex-wrap items-center gap-x-2 text-[12.5px]">
                <span className="font-medium text-fg">{actionText(a.action)}</span>
                {a.tickets.length > 1 && <span className="font-mono text-[11px] text-fg-3">{a.tickets.map((t) => `#${t}`).join(" · ")}</span>}
                {a.flags?.map((f) => (
                  <Chip key={f} size="sm" tone={f === "price correction" ? "gold" : f === "rejected" ? "down" : "neutral"}>
                    {f}
                  </Chip>
                ))}
              </div>
              <div className="mt-0.5 text-[11.5px] text-fg-3">
                {a.staff.name} · {serverStamp(a.at)} · {a.reasonCode}
                {a.note && <> · “{a.note}”</>}
              </div>
              <ChangeLine before={a.before} after={a.after} />
            </li>
          ))}
          <li className="relative">
            <span className="absolute -left-[21px] top-1.5 size-2.5 rounded-full border-2 border-surface bg-fg-3" />
            <div className="text-[12.5px] font-medium">Opened</div>
            <div className="mt-0.5 text-[11.5px] text-fg-3">
              {serverStamp(p.openTime)} · {p.side.toUpperCase()} {opened?.volume ?? p.volume} @ {formatNumber(opened?.price ?? p.openPrice, d)} · {opened?.to ?? p.route}-book · {opened?.reason}
            </div>
          </li>
        </ol>
      </div>
    </div>
  );
}

export function ChangeLine({ before, after }: { before?: Readonly<Record<string, unknown>> | null; after?: Readonly<Record<string, unknown>> | null }) {
  const same = (k: string) => !!before && !!after && k in before && k in after && JSON.stringify(before[k]) === JSON.stringify(after[k]);
  // full-object snapshots (e.g. a group update) list only what changed
  const keys = Array.from(new Set([...Object.keys(before ?? {}), ...Object.keys(after ?? {})])).filter((k) => !same(k));
  if (!keys.length) return null;
  const f = (v: unknown) => (v === null || v === undefined ? "—" : typeof v === "number" ? String(+v.toFixed(5)) : typeof v === "object" ? JSON.stringify(v).slice(0, 60) : String(v));
  return (
    <div className="mt-1 flex flex-wrap gap-1">
      {keys.slice(0, 6).map((k) => {
        const b = before?.[k];
        const a = after?.[k];
        return (
          <span key={k} className="rounded-md border border-line bg-surface-2 px-1.5 py-0.5 font-mono text-[10.5px] text-fg-3">
            {k}: {before && k in before ? <span className="text-fg-3 line-through decoration-fg-3/50">{f(b)}</span> : null}
            {before && k in before && after && k in after ? " → " : null}
            {after && k in after ? <span className="text-fg-2">{f(a)}</span> : null}
          </span>
        );
      })}
    </div>
  );
}

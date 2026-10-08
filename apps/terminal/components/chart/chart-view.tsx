"use client";

import * as React from "react";
import { useTheme } from "next-themes";
import { LineStyle, type IPriceLine } from "lightweight-charts";
import { toast } from "@/lib/notify";
import { ArrowDownRight, ArrowUpRight, Bell, Camera, CandlestickChart, ChevronUp, Crosshair, GripVertical, Layers, Minus, Plus, ShoppingCart, SlidersHorizontal, X, Zap } from "lucide-react";
import { getInstrument, isMarketOpen, priceFeed } from "@kalks/mock";
import { PriceText, cn, useQuote } from "@kalks/ui";
import { usePositionProfit, useTerminal, type Anchor, type ChartTab, type Drawing } from "@/lib/store";
import { CHART_TYPES, TIMEFRAMES, accMoney, contractSpec, fmtPrice, fmtVol, pipSize, profitAt, roundPrice, type TPosition } from "@/lib/trading";
import { useContextMenu, type MenuItem } from "@/components/ui/menu";
import { INDICATOR_CATEGORIES, INDICATOR_LIST } from "@/lib/indicators";
import { chartRegistry, useChartEngine, type LegendData } from "./engine";
import { IndicatorLegendRow } from "./indicators/legend";
import { clampStop, defaultStop, lineHandles, type StopKind } from "./trade-handles";
import { addIndicator, openIndicatorList, openIndicatorSettings, removeIndicator, toggleIndicator } from "./indicators/state";
import { useMarketOpen } from "@/lib/market-hours";
import { openRegister } from "@/lib/guest";
import { useT } from "@kalks/i18n/react";
import type { MessageKey } from "@kalks/i18n";

/* ------------------------------------------------------------------ */
/* Trade lines                                                         */
/* ------------------------------------------------------------------ */

export type LineKind = "pos" | "sl" | "tp" | "pending" | "alert";
export interface TLine {
  id: string;
  kind: LineKind;
  price: number;
  ref: string;
  side?: "buy" | "sell";
  label: string;
  draggable: boolean;
  closable?: boolean;
  /** a position line's S / T handles: "s" without a stop loss, "t" without a take profit (trade-handles.ts) */
  handles?: string;
}

/** An S / T handle pulled out of a position line: the stop it makes and where that stop may go. */
interface HandleDrag {
  kind: StopKind;
  ref: string;
  side: "buy" | "sell";
  /** pointer y at the press: a handle is a tap until the pointer moves a few pixels */
  y0: number;
  pip: number;
  /** the stops level in price units */
  gap: number;
}

const uid = () => Math.random().toString(36).slice(2, 9);

/** Position / SL / TP / pending / alert lines for one symbol. */
export function useTradeLines(symbol: string): TLine[] {
  const T = useTerminal();
  const positions = T.positions.filter((p) => p.symbol === symbol);
  const pendings = T.pendings.filter((p) => p.symbol === symbol);
  const alerts = T.alerts.filter((a) => a.symbol === symbol && a.active);
  const ro = T.readOnly;
  const t = useT();
  return React.useMemo(() => {
    const out: TLine[] = [];
    for (const p of positions) {
      out.push({ id: `pos:${p.ticket}`, kind: "pos", price: p.openPrice, ref: p.ticket, side: p.side, label: t(p.side === "buy" ? "chart.line.buy" : "chart.line.sell", { lot: fmtVol(p.volume) }), draggable: !ro, closable: !ro, handles: lineHandles(p, ro) });
      if (p.sl !== undefined) out.push({ id: `sl:${p.ticket}`, kind: "sl", price: p.sl, ref: p.ticket, side: p.side, label: "SL", draggable: !ro, closable: !ro });
      if (p.tp !== undefined) out.push({ id: `tp:${p.ticket}`, kind: "tp", price: p.tp, ref: p.ticket, side: p.side, label: "TP", draggable: !ro, closable: !ro });
    }
    for (const o of pendings) {
      out.push({ id: `pnd:${o.ticket}`, kind: "pending", price: o.price, ref: o.ticket, side: o.side, label: t(PENDING_LABEL[`${o.side}:${o.type}`] ?? "chart.line.buyLimit", { lot: fmtVol(o.volume) }), draggable: !ro, closable: !ro });
    }
    for (const a of alerts) out.push({ id: `alr:${a.id}`, kind: "alert", price: a.price, ref: a.id, label: t("chart.line.alert"), draggable: true, closable: true });
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [JSON.stringify(positions.map((p) => [p.ticket, p.openPrice, p.sl, p.tp, p.volume, p.side])), JSON.stringify(pendings.map((p) => [p.ticket, p.price, p.volume])), JSON.stringify(alerts.map((a) => [a.id, a.price])), ro, t]);
}

const PENDING_LABEL: Record<string, MessageKey> = {
  "buy:limit": "chart.line.buyLimit",
  "sell:limit": "chart.line.sellLimit",
  "buy:stop": "chart.line.buyStop",
  "sell:stop": "chart.line.sellStop",
  "buy:stop-limit": "chart.line.buyStopLimit",
  "sell:stop-limit": "chart.line.sellStopLimit",
};

/** Commit a dragged trade line to the store (SL/TP/pending/alert; dragging a position sets SL or TP by direction). */
export function commitLineDrag(T: ReturnType<typeof useTerminal>, symbol: string, line: TLine, price: number) {
  const inst = getInstrument(symbol);
  if (Math.abs(price - line.price) < 1 / 10 ** inst.digits) return;
  if (line.kind === "sl") T.modifyPosition(line.ref, { sl: price });
  else if (line.kind === "tp") T.modifyPosition(line.ref, { tp: price });
  else if (line.kind === "pending") T.modifyPending(line.ref, { price });
  else if (line.kind === "alert") T.updateAlert(line.ref, { price });
  else {
    const p = T.positions.find((x) => x.ticket === line.ref);
    const q = priceFeed().snapshot(symbol);
    if (!p || !q) return;
    const cur = p.side === "buy" ? q.bid : q.ask;
    const isSl = p.side === "buy" ? price < cur : price > cur;
    T.modifyPosition(line.ref, isSl ? { sl: price } : { tp: price });
  }
}

/** Close / remove action behind the × on a trade line chip. */
export function removeLine(T: ReturnType<typeof useTerminal>, l: TLine) {
  if (l.kind === "pos") T.closePosition(l.ref);
  else if (l.kind === "sl") T.modifyPosition(l.ref, { sl: null });
  else if (l.kind === "tp") T.modifyPosition(l.ref, { tp: null });
  else if (l.kind === "pending") T.cancelPending(l.ref);
  else if (l.kind === "alert") T.removeAlert(l.ref);
}

export interface ChartViewProps {
  tab: ChartTab;
  active: boolean;
  onActivate: () => void;
  compact?: boolean;
  hideOneClick?: boolean;
  /** draw the accent frame of the active chart (default: when active); off with a single chart on screen */
  highlight?: boolean;
}

export function ChartView({ tab, active, onActivate, compact, hideOneClick, highlight = active }: ChartViewProps) {
  const T = useTerminal();
  const t = useT();
  const { resolvedTheme } = useTheme();
  const wrap = React.useRef<HTMLDivElement>(null);
  const el = React.useRef<HTMLDivElement>(null);
  // legend values change on every tick: they live in a small store read by the legend leaves; ChartView only
  // keeps the legend's shape (which indicator rows exist), which changes when indicators do
  const legendStore = React.useMemo(createLegendStore, []);
  const [shape, setShape] = React.useState<LegendShape>(EMPTY_SHAPE);
  const onLegend = React.useCallback(
    (l: LegendData) => {
      legendStore.set(l);
      setShape((s) => sameShape(s, l) ? s : shapeOf(l));
    },
    [legendStore],
  );
  const [legendOpen, setLegendOpen] = React.useState(false);
  // No quote subscription here: a tick must not re-render the chart overlays. Leaf components (QuoteTag,
  // OneClickPanel, PositionChipPnl) subscribe themselves; handlers read the current quote when they run.
  const quoteNow = () => priceFeed().quote(tab.symbol);
  const inst = getInstrument(tab.symbol);
  const acc = T.account;
  const tool = active ? T.drawTool : "cursor";
  const engine = useChartEngine(el, {
    symbol: tab.symbol,
    tf: tab.tf,
    type: tab.type,
    indicators: tab.indicators,
    theme: resolvedTheme,
    crosshair: tool === "crosshair" || tool === "hline" || tool === "trend" || tool === "rect" || tool === "fib",
    onLegend,
  });
  const cm = useContextMenu(236);

  /* ---------------- registry for toolbar actions ---------------- */
  React.useEffect(() => {
    if (!engine) return;
    chartRegistry.set(tab.id, {
      zoom: (d) => {
        if (!engine.alive.current) return;
        const ts = engine.chart.timeScale();
        const cur = ts.options().barSpacing;
        ts.applyOptions({ barSpacing: Math.max(2, Math.min(40, d > 0 ? cur * 1.3 : cur / 1.3)) });
      },
      fit: () => {
        if (!engine.alive.current) return;
        engine.chart.timeScale().applyOptions({ barSpacing: 7, rightOffset: 12 });
        engine.chart.timeScale().scrollToRealTime();
        engine.chart.priceScale("right").applyOptions({ autoScale: true });
      },
      screenshot: () => {
        const canvas = engine.chart.takeScreenshot(true, false);
        canvas.toBlob((b) => {
          if (!b) return;
          const a = document.createElement("a");
          a.href = URL.createObjectURL(b);
          a.download = `${tab.symbol}_${tab.tf}_${new Date().toISOString().slice(0, 19).replace(/[:T]/g, "")}.png`;
          a.click();
          setTimeout(() => URL.revokeObjectURL(a.href), 2000);
        });
        toast.success(t("chart.screenshot.saved"), { description: `${tab.symbol}, ${tab.tf} · ${canvas.width}×${canvas.height} PNG` });
      },
    });
    return () => {
      chartRegistry.delete(tab.id);
    };
  }, [engine, tab.id, tab.symbol, tab.tf, t]);

  /* ---------------- lines from store ---------------- */
  const ro = T.readOnly;
  const tradeLines = useTradeLines(tab.symbol);
  // a new SL / TP (a position line dropped past the price, an S / T handle dragged or tapped) shows at once, not when
  // the trade server answers; it goes once the position carries it or the server says no
  const [fresh, setFresh] = React.useState<TLine[]>([]);
  const lines = React.useMemo(() => {
    const add = fresh.filter((f) => !tradeLines.some((l) => l.id === f.id));
    return add.length ? [...tradeLines, ...add] : tradeLines;
  }, [tradeLines, fresh]);
  const addFresh = (l: TLine) => setFresh((f) => [...f.filter((x) => x.id !== l.id), l]);
  const dropFresh = (id: string) => setFresh((f) => (f.some((x) => x.id === id) ? f.filter((x) => x.id !== id) : f));

  const [drag, setDrag] = React.useState<{ id: string; price: number; handle?: HandleDrag } | null>(null);
  const dragRef = React.useRef(drag);
  dragRef.current = drag;
  // a dropped SL / TP / pending line stays where it was dropped until the trade server has answered
  const [hold, setHold] = React.useState<{ id: string; price: number } | null>(null);
  const holdRef = React.useRef(hold);
  holdRef.current = hold;
  const settle = (id: string, price: number, p: Promise<boolean>) => {
    setHold({ id, price });
    void p.finally(() => setHold((h) => (h?.id === id && h.price === price ? null : h)));
  };
  const priceLines = React.useRef<{ owner: unknown; map: Map<string, IPriceLine> }>({ owner: null, map: new Map() });

  // sync price lines (axis labels + lines drawn by the chart)
  React.useEffect(() => {
    if (!engine || !engine.alive.current) return;
    if (priceLines.current.owner !== engine.chart) priceLines.current = { owner: engine.chart, map: new Map() }; // same chart survives theme recolours
    const c = engine.palette;
    const map = priceLines.current.map;
    const colorOf = (l: TLine) => (l.kind === "sl" ? c.down : l.kind === "tp" ? c.up : l.kind === "pending" ? c.gold : l.kind === "alert" ? c.warn : l.side === "buy" ? c.up : c.down);
    const want = new Map<string, { price: number; color: string; style: LineStyle; width: 1 | 2; title: string }>();
    for (const l of lines) want.set(l.id, { price: l.price, color: colorOf(l), style: l.kind === "pos" ? LineStyle.Solid : l.kind === "alert" ? LineStyle.SparseDotted : LineStyle.Dashed, width: 1, title: "" });
    for (const d of tab.drawings) if (d.kind === "hline") want.set(`hl:${d.id}`, { price: d.price, color: T.selectedDrawing === d.id ? c.ember : c.fg2, style: LineStyle.Solid, width: 1, title: "" });
    for (const [id, pl] of map) {
      if (!want.has(id)) {
        try {
          engine.main.removePriceLine(pl);
        } catch {
          /* chart rebuilt */
        }
        map.delete(id);
      }
    }
    for (const [id, w] of want) {
      const price = dragRef.current?.id === id ? dragRef.current.price : holdRef.current?.id === id ? holdRef.current.price : w.price;
      const opts = { price, color: w.color, lineWidth: w.width, lineStyle: w.style, axisLabelVisible: true, title: w.title, axisLabelColor: w.color, axisLabelTextColor: id.startsWith("hl:") ? (c.dark ? "#0a0a0d" : "#fff") : "#fff" };
      const ex = map.get(id);
      if (ex) ex.applyOptions(opts);
      else map.set(id, engine.main.createPriceLine(opts));
    }
  }, [engine, lines, tab.drawings, T.selectedDrawing, drag, hold]);

  /* ---------------- geometry loop for HTML overlays ---------------- */
  const [geo, setGeo] = React.useState<{ ys: Record<string, number | null>; psw: number; w: number; h: number; dr: Record<string, number[] | null>; pt: number[] }>({ ys: {}, psw: 68, w: 0, h: 0, dr: {}, pt: [] });
  const linesRef = React.useRef(lines);
  linesRef.current = lines;
  const drawingsRef = React.useRef(tab.drawings);
  drawingsRef.current = tab.drawings;
  const [draft, setDraft] = React.useState<{ kind: "trend" | "rect" | "fib"; a: Anchor; b: Anchor; clickMode?: boolean } | null>(null);
  const draftRef = React.useRef(draft);
  draftRef.current = draft;
  React.useEffect(() => {
    if (!engine) return;
    let raf = 0;
    // flat list of every number the overlays depend on: a frame where none changed costs no React render
    let prev: (number | string | null)[] = [];
    const sig: (number | string | null)[] = [];
    // DOM sizes are read when they change (ResizeObserver), never per frame: reading layout in the frame loop
    // forced a synchronous layout per chart per frame while ticks were mutating the page
    let w = el.current?.clientWidth ?? 0;
    let pt: number[] = [0];
    let observed: HTMLElement[] = [];
    const measure = () => {
      w = el.current?.clientWidth ?? 0;
      const panes = engine.chart.panes();
      if (panes.length > 1) {
        // pane tops (px from the chart's top) for the oscillator sub-window legends
        const top0 = el.current?.getBoundingClientRect().top ?? 0;
        pt = panes.map((p) => Math.round((p.getHTMLElement()?.getBoundingClientRect().top ?? top0) - top0));
      } else pt = [0];
    };
    const ro = new ResizeObserver(measure);
    const observe = () => {
      ro.disconnect();
      observed = engine.chart.panes().map((p) => p.getHTMLElement()).filter((x): x is HTMLElement => !!x);
      if (el.current) ro.observe(el.current);
      observed.forEach((x) => ro.observe(x));
      measure();
    };
    observe();
    const loop = () => {
      if (!engine.alive.current) return;
      const ys: Record<string, number | null> = {};
      const d = dragRef.current;
      const hd = holdRef.current;
      for (const l of linesRef.current) ys[l.id] = engine.main.priceToCoordinate(d?.id === l.id ? d.price : hd?.id === l.id ? hd.price : l.price);
      const ts = engine.chart.timeScale();
      const dr: Record<string, number[] | null> = {};
      const all: (Drawing | { id: string; kind: "trend" | "rect" | "fib"; a: Anchor; b: Anchor })[] = [...drawingsRef.current];
      if (draftRef.current) all.push({ id: "__draft", ...draftRef.current });
      for (const x of all) {
        if (x.kind === "hline") {
          const y = engine.main.priceToCoordinate(d?.id === `hl:${x.id}` ? d.price : x.price);
          dr[x.id] = y === null ? null : [y];
        } else {
          const x1 = ts.logicalToCoordinate(x.a.l as never);
          const x2 = ts.logicalToCoordinate(x.b.l as never);
          const y1 = engine.main.priceToCoordinate(x.a.p);
          const y2 = engine.main.priceToCoordinate(x.b.p);
          dr[x.id] = x1 === null || x2 === null || y1 === null || y2 === null ? null : [x1, y1, x2, y2];
        }
      }
      const psw = engine.chart.priceScale("right").width();
      const pane0 = engine.chart.panes()[0];
      const h = pane0 ? pane0.getHeight() : 0;
      // oscillator panes added or removed: watch the new pane elements (re-measures)
      if (engine.chart.panes().length !== observed.length) observe();
      sig.length = 0;
      sig.push(psw, w, h, pt.length, ...pt);
      for (const k in ys) sig.push(k, ys[k]!);
      for (const k in dr) {
        sig.push(k);
        const g = dr[k];
        if (g) sig.push(...g);
        else sig.push(null);
      }
      let same = sig.length === prev.length;
      for (let i = 0; same && i < sig.length; i++) same = sig[i] === prev[i];
      if (!same) {
        prev = sig.slice();
        setGeo({ ys, psw, w, h, dr, pt });
      }
      raf = requestAnimationFrame(loop);
    };
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
    };
  }, [engine]);

  /* ---------------- helpers ---------------- */
  const localY = (clientY: number) => clientY - (el.current?.getBoundingClientRect().top ?? 0);
  const localX = (clientX: number) => clientX - (el.current?.getBoundingClientRect().left ?? 0);
  const priceAt = (clientY: number) => {
    if (!engine) return null;
    const p = engine.main.coordinateToPrice(localY(clientY));
    return p === null ? null : roundPrice(tab.symbol, p);
  };
  const anchorAt = (clientX: number, clientY: number): Anchor | null => {
    if (!engine) return null;
    const l = engine.chart.timeScale().coordinateToLogical(localX(clientX));
    const p = engine.main.coordinateToPrice(localY(clientY));
    return l === null || p === null ? null : { l, p };
  };

  const hitLine = (clientY: number) => {
    const y = localY(clientY);
    let best: { id: string; d: number } | null = null;
    for (const l of linesRef.current) {
      if (!l.draggable) continue;
      const ly = geo.ys[l.id];
      if (ly == null) continue;
      const d = Math.abs(ly - y);
      if (d < 5 && (!best || d < best.d)) best = { id: l.id, d };
    }
    for (const d of tab.drawings) {
      if (d.kind !== "hline") continue;
      const ly = geo.dr[d.id]?.[0];
      if (ly == null) continue;
      const dd = Math.abs(ly - y);
      if (dd < 5 && (!best || dd < best.d)) best = { id: `hl:${d.id}`, d: dd };
    }
    return best?.id ?? null;
  };

  const [hover, setHover] = React.useState(false);

  const startDrag = (id: string) => {
    const l = linesRef.current.find((x) => x.id === id);
    const hl = id.startsWith("hl:") ? tab.drawings.find((d) => `hl:${d.id}` === id) : null;
    const price = l?.price ?? (hl && hl.kind === "hline" ? hl.price : null);
    if (price === null || price === undefined || !engine) return;
    engine.chart.applyOptions({ handleScroll: false, handleScale: false });
    setDrag({ id, price });
    if (hl) T.selectDrawing(hl.id);
  };

  /** Pulls an S / T handle out of a position line: the new stop follows the pointer once it moves (startHandle). */
  const handleOf = (l: TLine, kind: StopKind, y0: number): HandleDrag => ({ kind, ref: l.ref, side: l.side ?? "buy", y0, pip: pipSize(inst), gap: contractSpec(tab.symbol).stopsLevel / 10 ** inst.digits });
  const startHandle = (l: TLine, kind: StopKind, clientY: number) => {
    if (!engine) return;
    engine.chart.applyOptions({ handleScroll: false, handleScale: false });
    setDrag({ id: `${kind}:${l.ref}`, price: l.price, handle: handleOf(l, kind, clientY) });
  };

  React.useEffect(() => {
    if (!drag || !engine) return;
    const id = drag.id;
    const h = drag.handle;
    // a handle stays a tap until the pointer has moved a few pixels; then its new line appears and follows it
    let moved = !h;
    let shown = false;
    let last = drag.price;
    let done = false;
    const move = (e: PointerEvent) => {
      if (!moved) {
        if (Math.abs(e.clientY - h!.y0) <= 4) return;
        moved = true;
      }
      const p = engine.main.coordinateToPrice(localY(e.clientY));
      if (p === null) return;
      let price = roundPrice(tab.symbol, p);
      if (h) {
        // held on the side of the price the trade server accepts
        price = clampStop(h.kind, h.side, price, quoteNow(), h.gap, inst.digits);
        if (!shown) {
          shown = true;
          addFresh(stopLine(id, h, price));
        }
      }
      last = price;
      setDrag((d) => (d ? { ...d, price } : d));
    };
    const up = () => {
      if (done) return;
      done = true;
      engine.chart.applyOptions({ handleScroll: true, handleScale: true });
      setDrag(null);
      if (!dragRef.current) return;
      if (h) commitHandle(id, h, moved ? last : null);
      else commitDrag(id, last);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up, { once: true });
    window.addEventListener("pointercancel", up, { once: true });
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [drag?.id, engine]);

  const stopLine = (id: string, h: HandleDrag, price: number): TLine => ({ id, kind: h.kind, price, ref: h.ref, side: h.side, label: h.kind === "sl" ? "SL" : "TP", draggable: false, closable: false });
  /** A handle let go: the stop where it was dropped, or (a tap) at the order tickets' starting distance. */
  const commitHandle = (id: string, h: HandleDrag, dropped: number | null) => {
    const price = dropped ?? defaultStop(h.kind, h.side, quoteNow(), h.pip, h.gap, inst.digits);
    addFresh(stopLine(id, h, price));
    settle(id, price, T.modifyPosition(h.ref, h.kind === "sl" ? { sl: price } : { tp: price }).finally(() => dropFresh(id)));
  };

  const commitDrag = (id: string, price: number) => {
    const [kind, ref] = id.split(":") as [string, string];
    if (kind === "hl") {
      T.updateTab(tab.id, (t) => ({ drawings: t.drawings.map((d) => (d.id === ref && d.kind === "hline" ? { ...d, price } : d)) }));
      return;
    }
    const l = linesRef.current.find((x) => x.id === id);
    if (!l) return;
    if (Math.abs(price - l.price) < 1 / 10 ** inst.digits) return;
    if (kind === "sl") settle(id, price, T.modifyPosition(ref, { sl: price }));
    else if (kind === "tp") settle(id, price, T.modifyPosition(ref, { tp: price }));
    else if (kind === "pnd") settle(id, price, T.modifyPending(ref, { price }));
    else if (kind === "alr") T.updateAlert(ref, { price });
    else if (kind === "pos") {
      const p = T.positions.find((x) => x.ticket === ref);
      if (!p) return;
      const q = quoteNow();
      const cur = p.side === "buy" ? q.bid : q.ask;
      const isSl = p.side === "buy" ? price < cur : price > cur;
      const id = `${isSl ? "sl" : "tp"}:${ref}`;
      addFresh({ id, kind: isSl ? "sl" : "tp", price, ref, side: p.side, label: isSl ? "SL" : "TP", draggable: false, closable: false });
      settle(id, price, T.modifyPosition(ref, isSl ? { sl: price } : { tp: price }).finally(() => dropFresh(id)));
    }
  };

  /* ---------------- drawing tools ---------------- */
  const drawing = tool === "hline" || tool === "trend" || tool === "rect" || tool === "fib";
  const onDrawDown = (e: React.PointerEvent) => {
    if (!drawing) return;
    e.stopPropagation();
    e.preventDefault();
    const a = anchorAt(e.clientX, e.clientY);
    if (!a) return;
    if (tool === "hline") {
      const id = uid();
      T.updateTab(tab.id, (t) => ({ drawings: [...t.drawings, { id, kind: "hline", price: roundPrice(tab.symbol, a.p) }] }));
      T.setDrawTool("cursor");
      T.selectDrawing(id);
      toast.success(t("chart.draw.hlineAdded"), { description: t("chart.draw.hlineAddedText", { symbol: tab.symbol, price: fmtPrice(tab.symbol, a.p) }) });
      return;
    }
    if (draft?.clickMode) {
      commitDraft({ ...draft, b: a });
      return;
    }
    setDraft({ kind: tool as "trend" | "rect" | "fib", a, b: a });
  };
  const onDrawMove = (e: React.PointerEvent) => {
    if (!draft) return;
    const b = anchorAt(e.clientX, e.clientY);
    if (b) setDraft((d) => (d ? { ...d, b } : d));
  };
  const onDrawUp = () => {
    const d = draftRef.current;
    if (!d || d.clickMode) return;
    const g = geo.dr["__draft"];
    const moved = g ? Math.hypot(g[2]! - g[0]!, g[3]! - g[1]!) > 6 : false;
    if (moved) commitDraft(d);
    else setDraft({ ...d, clickMode: true });
  };
  const commitDraft = (d: { kind: "trend" | "rect" | "fib"; a: Anchor; b: Anchor }) => {
    const id = uid();
    T.updateTab(tab.id, (t) => ({ drawings: [...t.drawings, { id, kind: d.kind, a: d.a, b: d.b }] }));
    setDraft(null);
    T.setDrawTool("cursor");
    T.selectDrawing(id);
  };
  React.useEffect(() => {
    if (!drawing) setDraft(null);
  }, [drawing]);

  /* ---------------- context menu ---------------- */
  const onContext = (e: React.MouseEvent) => {
    e.preventDefault();
    onActivate();
    const q = quoteNow();
    const price = priceAt(e.clientY);
    const p = price ?? q.bid;
    const ps = fmtPrice(tab.symbol, p);
    const below = p < q.bid;
    const place = (side: "buy" | "sell", type: "limit" | "stop") => {
      if (T.ws.oneClick) T.placeOrder({ symbol: tab.symbol, side, type, volume: T.ws.lot, price: p });
      else T.openNewOrder({ symbol: tab.symbol, side, type, price: p });
    };
    const trade: MenuItem[] = T.guest
      ? [
          { header: t("trader.guest.title") },
          { label: t("chart.menu.openAccount"), icon: <ShoppingCart />, onSelect: openRegister },
        ]
      : ro
      ? [{ header: t("chart.menu.readOnly") }]
      : [
          { label: t("chart.menu.buyLimitAt", { lot: fmtVol(T.ws.lot), price: ps }), icon: <ArrowUpRight />, tone: "up", disabled: !below, onSelect: () => place("buy", "limit") },
          { label: t("chart.menu.sellLimitAt", { lot: fmtVol(T.ws.lot), price: ps }), icon: <ArrowDownRight />, tone: "down", disabled: below, onSelect: () => place("sell", "limit") },
          { label: t("chart.menu.buyStopAt", { lot: fmtVol(T.ws.lot), price: ps }), icon: <ArrowUpRight />, tone: "up", disabled: below, onSelect: () => place("buy", "stop") },
          { label: t("chart.menu.sellStopAt", { lot: fmtVol(T.ws.lot), price: ps }), icon: <ArrowDownRight />, tone: "down", disabled: !below, onSelect: () => place("sell", "stop") },
          "sep",
          { label: t("chart.menu.newOrder"), icon: <ShoppingCart />, hint: "F9", onSelect: () => T.openNewOrder({ symbol: tab.symbol }) },
        ];
    cm.open(
      e,
      [
        ...trade,
        "sep",
        { label: t("chart.menu.alertAt", { price: ps }), icon: <Bell />, onSelect: () => T.addAlert({ symbol: tab.symbol, cond: p >= q.bid ? "above" : "below", price: p }) },
        {
          label: t("chart.menu.addHline"),
          icon: <Minus />,
          onSelect: () => {
            const id = uid();
            T.updateTab(tab.id, (t) => ({ drawings: [...t.drawings, { id, kind: "hline", price: p }] }));
            T.selectDrawing(id);
          },
        },
        "sep",
        { label: t("chart.menu.timeframes"), icon: <CandlestickChart />, items: TIMEFRAMES.map((tf) => ({ label: tf, checked: tab.tf === tf, onSelect: () => T.updateTab(tab.id, { tf, drawings: [] }) })) },
        { label: t("chart.menu.chartType"), icon: <Layers />, items: CHART_TYPES.map((ct) => ({ label: t(`trader.chartType.${ct}`), checked: tab.type === ct, onSelect: () => T.updateTab(tab.id, { type: ct }) })) },
        {
          label: t("chart.menu.indicators"),
          icon: <SlidersHorizontal />,
          items: [
            { label: t("chart.menu.indicatorsList"), hint: "Ctrl+I", onSelect: () => openIndicatorList(tab.id) },
            "sep",
            ...INDICATOR_CATEGORIES.map((cat) => ({
              label: t.dyn(`market.nav.category.${cat.replace(/\s+/g, "").replace(/^./, (c) => c.toLowerCase())}`, cat),
              items: INDICATOR_LIST.filter((d) => d.category === cat).map((d) => ({ label: d.name, onSelect: () => addIndicator(T, tab.id, d.type) })),
            })),
            "sep",
            { label: t("chart.menu.removeAllIndicators"), danger: true, disabled: !tab.indicators.length, onSelect: () => T.updateTab(tab.id, { indicators: [] }) },
          ],
        },
        { label: t("chart.menu.crosshair"), icon: <Crosshair />, hint: "Ctrl+F", onSelect: () => T.setDrawTool(T.drawTool === "crosshair" ? "cursor" : "crosshair") },
        "sep",
        { label: t("chart.menu.zoomIn"), icon: <Plus />, hint: "+", onSelect: () => chartRegistry.get(tab.id)?.zoom(1) },
        { label: t("chart.menu.zoomOut"), icon: <Minus />, hint: "−", onSelect: () => chartRegistry.get(tab.id)?.zoom(-1) },
        { label: t("chart.menu.saveAsPicture"), icon: <Camera />, onSelect: () => chartRegistry.get(tab.id)?.screenshot() },
        { label: t("chart.menu.deleteAllObjects"), danger: true, icon: <X />, disabled: !tab.drawings.length, onSelect: () => T.updateTab(tab.id, { drawings: [] }) },
      ],
      <span>
        {tab.symbol}, {tab.tf} · <span className="text-fg-2">{ps}</span>
      </span>,
    );
  };

  /* ---------------- render ---------------- */
  const dragLine = drag ? lines.find((l) => l.id === drag.id) : null;
  const dragPos = dragLine && (dragLine.kind === "pos" || dragLine.kind === "sl" || dragLine.kind === "tp") ? T.positions.find((p) => p.ticket === dragLine.ref) : null;
  const chipRight = geo.psw + 6;
  const allMainRows = shape.main;
  // keep the overlay legend inside the main pane: extra rows fold into "+N more"
  const maxRows = Math.max(1, Math.floor(((geo.h || 400) - (ro || hideOneClick ? 40 : 96)) / 16));
  const folded = !legendOpen && allMainRows.length > maxRows;
  const mainRows = folded ? allMainRows.slice(0, maxRows - 1) : allMainRows;
  const paneRows = shape.panes;
  const indAction = (a: "toggle", uid: string) => {
    const x = tab.indicators.find((i) => i.uid === uid);
    if (x && a === "toggle") toggleIndicator(T, tab.id, x);
  };

  return (
    <div
      ref={wrap}
      onPointerDown={onActivate}
      onContextMenu={onContext}
      className={cn("relative h-full min-h-0 w-full select-none overflow-hidden rounded-[8px] border bg-[var(--t-chart-bg)]", highlight ? "border-ember/70 shadow-[0_0_0_1px_rgba(255,90,31,0.25)]" : "border-line", (hover || drag) && "cursor-ns-resize")}
      onPointerDownCapture={(e) => {
        if (drawing || e.button !== 0) return;
        // the S / T handles and the × on a chip take their own presses
        if ((e.target as Element).closest("[data-chip-action]")) return;
        const id = hitLine(e.clientY);
        if (!id) {
          if (T.selectedDrawing && !(e.target as Element).closest("[data-drawing]")) T.selectDrawing(null);
          return;
        }
        e.stopPropagation();
        e.preventDefault();
        onActivate();
        startDrag(id);
      }}
      onPointerMove={(e) => !drag && !drawing && setHover(!!hitLine(e.clientY))}
      onPointerLeave={() => setHover(false)}
      data-chart={tab.id}
    >
      <div ref={el} className="absolute inset-0" />

      {/* drawings */}
      <svg className="pointer-events-none absolute inset-0 z-[2] h-full w-full overflow-hidden" style={{ width: geo.w, height: geo.h || "100%" }}>
        {Object.entries(geo.dr).map(([id, g]) => {
          if (!g) return null;
          const d = id === "__draft" ? (draft ? { id, kind: draft.kind } : null) : tab.drawings.find((x) => x.id === id);
          if (!d) return null;
          const sel = T.selectedDrawing === id;
          const col = sel ? "var(--k-ember)" : "var(--k-gold)";
          const selectProps = {
            "data-drawing": id,
            style: { pointerEvents: drawing ? "none" : "stroke", cursor: "pointer" } as React.CSSProperties,
            onPointerDown: (e: React.PointerEvent) => {
              e.stopPropagation();
              T.selectDrawing(id);
            },
          };
          if (d.kind === "hline") {
            return <line key={id} x1={0} x2={geo.w - geo.psw} y1={g[0]} y2={g[0]} stroke="transparent" strokeWidth={10} {...selectProps} />;
          }
          const [x1, y1, x2, y2] = g as [number, number, number, number];
          if (d.kind === "trend")
            return (
              <g key={id}>
                <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={col} strokeWidth={sel ? 2 : 1.5} />
                <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="transparent" strokeWidth={10} {...selectProps} />
                {sel && [[x1, y1], [x2, y2]].map(([a, b], i) => <rect key={i} x={a! - 3.5} y={b! - 3.5} width={7} height={7} fill="var(--t-chart-bg)" stroke={col} />)}
              </g>
            );
          if (d.kind === "rect")
            return (
              <g key={id}>
                <rect x={Math.min(x1, x2)} y={Math.min(y1, y2)} width={Math.abs(x2 - x1)} height={Math.abs(y2 - y1)} fill={sel ? "rgba(255,90,31,0.10)" : "rgba(233,185,73,0.08)"} stroke={col} strokeWidth={1} />
                <rect x={Math.min(x1, x2)} y={Math.min(y1, y2)} width={Math.abs(x2 - x1)} height={Math.abs(y2 - y1)} fill="transparent" stroke="transparent" strokeWidth={8} {...selectProps} style={{ ...selectProps.style, pointerEvents: drawing ? "none" : "all" }} />
              </g>
            );
          // fibonacci retracement
          const levels = [0, 0.236, 0.382, 0.5, 0.618, 0.786, 1];
          const xa = Math.min(x1, x2);
          const xb = Math.max(x1, x2, xa + 40);
          const pa = id === "__draft" ? draft!.a.p : (d as unknown as { a: Anchor }).a.p;
          const pb = id === "__draft" ? draft!.b.p : (d as unknown as { b: Anchor }).b.p;
          return (
            <g key={id}>
              <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={col} strokeDasharray="3 3" strokeWidth={1} />
              {levels.map((lv) => {
                const y = y2 + (y1 - y2) * lv;
                return (
                  <g key={lv}>
                    <line x1={xa} x2={xb} y1={y} y2={y} stroke={col} strokeOpacity={lv === 0 || lv === 1 ? 0.9 : 0.55} strokeWidth={1} />
                    <text x={xa + 3} y={y - 3} fontSize={9.5} fill={col} fontFamily="var(--font-geist-mono)">
                      {(lv * 100).toFixed(1)}% · {fmtPrice(tab.symbol, pb + (pa - pb) * lv)}
                    </text>
                  </g>
                );
              })}
              <rect x={xa} y={Math.min(y1, y2)} width={xb - xa} height={Math.abs(y2 - y1)} fill="transparent" {...selectProps} style={{ ...selectProps.style, pointerEvents: drawing ? "none" : "all" }} />
            </g>
          );
        })}
      </svg>

      {/* drawing capture layer */}
      {drawing && active && (
        <div className="absolute inset-0 z-[3] cursor-crosshair" style={{ right: geo.psw }} onPointerDown={onDrawDown} onPointerMove={onDrawMove} onPointerUp={onDrawUp} />
      )}

      {/* trade line chips */}
      <div className="pointer-events-none absolute inset-0 z-[4]">
        {chipRows(lines, geo.ys, geo.h).map((row) => (
          // chips closer than a chip's height (two positions opened at the same price, SL next to a pending
          // order…) share one row, side by side from the price scale leftwards, instead of covering each other
          <div key={row.lines[0]!.id} className="absolute flex -translate-y-1/2 flex-row-reverse items-center gap-1" style={{ top: row.y, right: chipRight }}>
        {row.lines.map((l) => {
          const isDrag = drag?.id === l.id;
          const price = isDrag ? drag.price : hold?.id === l.id ? hold.price : l.price;
          let pnlText = "";
          let pnl = 0;
          const livePos = l.kind === "pos" ? T.positions.find((x) => x.ticket === l.ref) : undefined;
          // S / T: only while the position has no such stop (a new one being placed counts)
          const handles = l.kind === "pos" && l.handles ? (["sl", "tp"] as const).filter((k) => l.handles!.includes(k[0]!) && !lines.some((x) => x.id === `${k}:${l.ref}`)) : [];
          if (l.kind === "sl" || l.kind === "tp") {
            const p = T.positions.find((x) => x.ticket === l.ref);
            if (p) {
              pnl = profitAt(p, price) + p.swap - p.commission;
              pnlText = accMoney(acc, pnl, { signed: true });
            }
          }
          const tone =
            l.kind === "sl" ? "border-down/60 bg-down text-white" : l.kind === "tp" ? "border-up/60 bg-up text-white" : l.kind === "pending" ? "border-gold/60 bg-[color-mix(in_oklab,var(--k-gold)_88%,black)] text-[#1a1204]" : l.kind === "alert" ? "border-warn/60 bg-warn text-[#1a1204]" : l.side === "buy" ? "border-up/60 bg-panel-2 text-up" : "border-down/60 bg-panel-2 text-down";
          return (
            <div
              key={l.id}
              className={cn("pointer-events-auto flex h-[18px] shrink-0 items-center overflow-hidden rounded-[4px] border font-mono text-[10.5px] font-medium leading-none shadow-[0_2px_8px_rgba(0,0,0,0.35)]", tone, l.draggable && "cursor-ns-resize")}
              style={row.lines.length > 1 ? { transform: `translateY(${(geo.ys[l.id] ?? row.y) - row.y}px)` } : undefined}
              onPointerDown={(e) => {
                if (!l.draggable || e.button !== 0) return;
                e.stopPropagation();
                e.preventDefault();
                onActivate();
                startDrag(l.id);
              }}
              onDoubleClick={() => l.kind === "pos" && T.setUi({ positionDialog: l.ref })}
              title={l.kind === "pos" ? t("chart.line.posTitle") : l.draggable ? t("chart.line.dragTitle") : undefined}
            >
              {l.draggable && <GripVertical className="ms-0.5 size-3 shrink-0 opacity-70" aria-hidden />}
              <span className={l.draggable ? "pe-1.5 ps-0.5" : "px-1.5"}>
                {l.label}
                {isDrag && l.kind !== "pos" && <span className="ml-1 opacity-80">{fmtPrice(tab.symbol, price)}</span>}
              </span>
              {livePos && <PositionChipPnl p={livePos} />}
              {handles.length > 0 && (
                <span className="flex h-full items-center gap-[3px] border-l border-line px-[3px]">
                  {handles.map((k) => (
                    <button
                      key={k}
                      type="button"
                      data-chip-action
                      data-handle={k}
                      aria-label={t(k === "sl" ? "chart.line.slHandleTitle" : "chart.line.tpHandleTitle")}
                      title={t(k === "sl" ? "chart.line.slHandleTitle" : "chart.line.tpHandleTitle")}
                      onPointerDown={(e) => {
                        if (e.button !== 0) return;
                        e.stopPropagation();
                        e.preventDefault();
                        onActivate();
                        startHandle(l, k, e.clientY);
                      }}
                      onClick={(e) => {
                        e.stopPropagation();
                        // keyboard (Enter / Space): the starting distance, as a tap
                        if (e.detail === 0) commitHandle(`${k}:${l.ref}`, handleOf(l, k, 0), null);
                      }}
                      onDoubleClick={(e) => e.stopPropagation()}
                      className={cn(
                        "grid size-[14px] shrink-0 cursor-ns-resize place-items-center rounded-[3px] border font-sans text-[9px] font-bold leading-none transition-colors",
                        k === "sl" ? "border-down text-down hover:bg-down/15" : "border-up text-up hover:bg-up/15",
                      )}
                    >
                      {t(k === "sl" ? "chart.line.slHandle" : "chart.line.tpHandle")}
                    </button>
                  ))}
                </span>
              )}
              {pnlText && <span className="k-num border-l border-white/25 px-1.5">{pnlText}</span>}
              {l.closable && (
                <button
                  data-chip-action
                  aria-label={t("chart.line.remove", { label: l.label })}
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation();
                    if (l.kind === "pos") T.closePosition(l.ref);
                    else if (l.kind === "sl") T.modifyPosition(l.ref, { sl: null });
                    else if (l.kind === "tp") T.modifyPosition(l.ref, { tp: null });
                    else if (l.kind === "pending") T.cancelPending(l.ref);
                    else if (l.kind === "alert") T.removeAlert(l.ref);
                  }}
                  className="grid h-full w-4 place-items-center border-l border-current/25 hover:bg-black/20"
                >
                  <X className="size-2.5" />
                </button>
              )}
            </div>
          );
        })}
          </div>
        ))}
        {/* ghost while dragging a position line → projected SL/TP */}
        {drag && dragLine?.kind === "pos" && dragPos && geo.ys[drag.id] != null && (
          (() => {
            const q = quoteNow();
            const cur = dragPos.side === "buy" ? q.bid : q.ask;
            const isSl = dragPos.side === "buy" ? drag.price < cur : drag.price > cur;
            const pr = profitAt(dragPos, drag.price);
            return (
              <div className={cn("absolute left-3 flex h-[18px] -translate-y-1/2 items-center gap-1.5 rounded-[4px] px-1.5 font-mono text-[10.5px] text-white", isSl ? "bg-down" : "bg-up")} style={{ top: geo.ys[drag.id]! }}>
                {isSl ? "SL" : "TP"} {fmtPrice(tab.symbol, drag.price)} · {accMoney(acc, pr, { signed: true })}
              </div>
            );
          })()
        )}
      </div>
      {drag && dragLine?.kind === "pos" && engine && (
        <div className="pointer-events-none absolute inset-x-0 z-[3] border-t border-dashed border-fg-2/70" style={{ top: engine.main.priceToCoordinate(drag.price) ?? -10, right: geo.psw }} />
      )}

      {/* legend */}
      <div className="pointer-events-none absolute left-2 top-1.5 z-[5] max-w-[calc(100%-90px)]">
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5 font-mono text-[10.5px] leading-4 text-fg-3">
          <span className="font-sans text-[11.5px] font-semibold text-fg">
            {tab.symbol}, {tab.tf}
          </span>
          {!compact && <span className="font-sans text-fg-3">{inst.name}</span>}
          <LegendOhlc store={legendStore} digits={inst.digits} compact={compact} />
        </div>
        {mainRows.length > 0 && (
          <div className="mt-0.5 flex flex-col items-start">
            {mainRows.map((uid) => (
              <LiveLegendRow key={uid} store={legendStore} uid={uid} onToggle={() => indAction("toggle", uid)} onSettings={() => openIndicatorSettings(tab.id, uid)} onRemove={() => removeIndicator(T, tab.id, uid)} />
            ))}
            {(folded || legendOpen) && allMainRows.length > maxRows && (
              <button
                onPointerDown={(e) => e.stopPropagation()}
                onClick={() => setLegendOpen((v) => !v)}
                className="pointer-events-auto h-4 rounded-[3px] px-0.5 font-mono text-[10px] leading-4 text-fg-3 hover:text-fg"
                aria-expanded={legendOpen}
              >
                {legendOpen ? t("chart.legend.showLess") : t("chart.legend.more", { count: allMainRows.length - mainRows.length })}
              </button>
            )}
          </div>
        )}
      </div>

      {/* sub-window legends (one per oscillator pane) */}
      {paneRows.map((r) =>
        geo.pt[r.pane] === undefined ? null : (
          <div key={r.uid} className="pointer-events-none absolute left-2 z-[5] max-w-[calc(100%-90px)]" style={{ top: geo.pt[r.pane]! + 3 }}>
            <LiveLegendRow store={legendStore} uid={r.uid} onToggle={() => indAction("toggle", r.uid)} onSettings={() => openIndicatorSettings(tab.id, r.uid)} onRemove={() => removeIndicator(T, tab.id, r.uid)} />
          </div>
        ),
      )}

      {/* one-click trading panel */}
      {!ro && !hideOneClick && (
        <OneClickPanel symbol={tab.symbol} compact={compact} top={24 + (mainRows.length ? (mainRows.length + (allMainRows.length > maxRows ? 1 : 0)) * 16 + 2 : 0)} />
      )}

      {/* bid/ask tag */}

      {drawing && active && (
        <div className="pointer-events-none absolute left-1/2 top-2 z-[6] -translate-x-1/2 rounded-[5px] border border-ember/40 bg-panel-2/95 px-2 py-0.5 text-[10.5px] text-fg-2">
          {tool === "hline" ? t("chart.draw.hline") : draft?.clickMode ? t("chart.draw.secondPoint") : t(tool === "fib" ? "chart.draw.fib" : tool === "rect" ? "chart.draw.rect" : "chart.draw.trend")}
        </div>
      )}
      {cm.node}
    </div>
  );
}

/** Visible trade-line chips grouped into rows: lines less than one chip height apart share a row. */
function chipRows(lines: TLine[], ys: Record<string, number | null>, h: number) {
  const vis = lines.filter((l) => {
    const y = ys[l.id];
    return y != null && y >= 4 && y <= h - 4;
  });
  vis.sort((a, b) => ys[a.id]! - ys[b.id]!);
  const rows: { y: number; lines: TLine[] }[] = [];
  for (const l of vis) {
    const y = ys[l.id]!;
    const last = rows[rows.length - 1];
    if (last && y - last.y < 19) last.lines.push(l);
    else rows.push({ y, lines: [l] });
  }
  return rows;
}

/* ------------------------------------------------------------------ */
/* Per-tick leaves (the only parts of a chart that re-render on a tick) */
/* ------------------------------------------------------------------ */

/** Latest legend values of one chart (the bar under the crosshair, else the forming bar), outside React state. */
function createLegendStore() {
  let v: LegendData | null = null;
  const subs = new Set<() => void>();
  return {
    get: () => v,
    set: (l: LegendData) => {
      v = l;
      subs.forEach((f) => f());
    },
    subscribe: (f: () => void) => {
      subs.add(f);
      return () => void subs.delete(f);
    },
  };
}
type LegendStore = ReturnType<typeof createLegendStore>;
interface LegendShape {
  main: string[];
  panes: { uid: string; pane: number }[];
}
const EMPTY_SHAPE: LegendShape = { main: [], panes: [] };
const shapeOf = (l: LegendData): LegendShape => ({ main: l.ind.filter((r) => r.pane <= 0).map((r) => r.uid), panes: l.ind.filter((r) => r.pane > 0).map((r) => ({ uid: r.uid, pane: r.pane })) });
function sameShape(s: LegendShape, l: LegendData) {
  let mi = 0;
  let pi = 0;
  for (const r of l.ind) {
    if (r.pane <= 0) {
      if (s.main[mi++] !== r.uid) return false;
    } else {
      const p = s.panes[pi++];
      if (!p || p.uid !== r.uid || p.pane !== r.pane) return false;
    }
  }
  return mi === s.main.length && pi === s.panes.length;
}

function LegendOhlc({ store, digits, compact }: { store: LegendStore; digits: number; compact?: boolean }) {
  const legend = React.useSyncExternalStore(store.subscribe, store.get, () => null);
  if (!legend) return null;
  const up = legend.c >= legend.o;
  return (
    <>
      {(["o", "h", "l", "c"] as const).map((k) => (
        <span key={k} className="k-num">
          {k.toUpperCase()}
          <span className={cn("ml-1", up ? "text-up" : "text-down")}>{legend[k].toFixed(digits)}</span>
        </span>
      ))}
      {!compact && <span className={cn("k-num", legend.chg >= 0 ? "text-up" : "text-down")}>{legend.chg >= 0 ? "+" : ""}{legend.chg.toFixed(2)}%</span>}
    </>
  );
}

function LiveLegendRow({ store, uid, ...actions }: { store: LegendStore; uid: string; onToggle: () => void; onSettings: () => void; onRemove: () => void }) {
  const row = React.useSyncExternalStore(store.subscribe, () => store.get()?.ind.find((r) => r.uid === uid), () => undefined);
  return row ? <IndicatorLegendRow row={row} {...actions} /> : null;
}

/** Floating P&L on a position line chip: the engine's value (equity frames) or computed from the quote. */
function PositionChipPnl({ p }: { p: TPosition }) {
  const T = useTerminal();
  const pnl = usePositionProfit(p);
  return <span className={cn("k-num border-l border-line px-1.5", pnl >= 0 ? "bg-up/15 text-up" : "bg-down/15 text-down")}>{accMoney(T.account, pnl, { signed: true })}</span>;
}

/* ------------------------------------------------------------------ */

export function OneClickPanel({ symbol, compact, top, left }: { symbol: string; compact?: boolean; top: number; left?: number }) {
  const T = useTerminal();
  const t = useT();
  const { bid, ask, dir, delayed } = useQuote(symbol);
  const spread = Math.round((ask - bid) * 10 ** getInstrument(symbol).digits);
  // a delayed snapshot (not streaming yet): prices shown, Sell / Buy off with the reason
  const blocked = delayed ? t("desk.side.delayedTip") : null;
  const [lot, setLot] = React.useState(String(T.ws.lot.toFixed(2)));
  React.useEffect(() => {
    setLot(T.ws.lot.toFixed(2));
  }, [T.ws.lot]);
  const vol = Math.max(0.01, parseFloat(lot) || 0.01);
  const open = useMarketOpen(symbol);
  const go = (side: "buy" | "sell") => {
    if (!open || blocked) return;
    if (T.guest) return void T.quickTrade(symbol, side, vol); // explains: no trading account yet
    if (T.ws.oneClick) T.quickTrade(symbol, side, vol);
    else T.openNewOrder({ symbol, side, type: "market" });
  };
  const commitLot = () => {
    const v = Math.max(0.01, Math.round(vol * 100) / 100);
    T.setWs({ lot: v });
    setLot(v.toFixed(2));
  };
  const step = (d: number) => {
    const inc = vol >= 10 ? 1 : vol >= 1 ? 0.1 : 0.01;
    const v = Math.max(0.01, +(vol + d * inc).toFixed(2));
    setLot(v.toFixed(2));
    T.setWs({ lot: v });
  };
  const [collapsed, setCollapsed] = React.useState(false);
  if (collapsed)
    return (
      <button
        className="absolute left-2 z-[6] flex h-8 items-center gap-1.5 rounded-[8px] border border-line-top bg-panel-2/95 px-2.5 text-[12px] font-semibold text-fg-2 shadow-[0_6px_20px_-8px_rgba(0,0,0,0.6)] hover:text-fg"
        style={{ top, left }}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={() => setCollapsed(false)}
        aria-label={t("chart.oneClick.show")}
      >
        <span className="size-1.5 rounded-full bg-up" />
        <span className="size-1.5 rounded-full bg-down" />
        {t("chart.oneClick.collapsed")}
      </button>
    );
  return (
    <div data-tour="oneclick" className="absolute left-2 z-[6] flex items-stretch overflow-hidden rounded-[9px] border border-line-top bg-panel-2 shadow-[0_6px_20px_-8px_rgba(0,0,0,0.6)]" style={{ top, left }} onPointerDown={(e) => e.stopPropagation()} onContextMenu={(e) => e.stopPropagation()}>
      <button onClick={() => go("sell")} disabled={!open || !!blocked} title={blocked ?? (open ? (T.guest ? t("trader.guest.title") : undefined) : t("chart.oneClick.marketClosed"))} className={cn("group flex flex-col items-start bg-down/12 px-2 py-1 text-left transition-colors hover:bg-down/25 disabled:cursor-not-allowed disabled:bg-surface-2 disabled:opacity-60", compact ? "min-w-[74px]" : "min-w-[92px]")} aria-label={t(open ? "chart.oneClick.sellAria" : "chart.oneClick.sellClosedAria", { symbol })}>
        <span className="flex items-center gap-1 text-[11px] font-semibold text-down">{T.ws.oneClick && !T.guest && <Zap className="size-3 fill-current" aria-hidden />}{t("common.sell")}</span>
        <PriceText symbol={symbol} value={bid} dir={dir} className={compact ? "text-[12px]" : "text-[14px]"} />
      </button>
      <div className="flex w-[84px] flex-col items-center justify-center border-x border-line bg-panel px-0.5">
        <div className="flex w-full items-center">
        <button onClick={() => step(-1)} className="grid size-5 shrink-0 place-items-center rounded text-[13px] leading-none text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label={t("chart.oneClick.decrease")}>−</button>
        <input
          aria-label={t("chart.oneClick.lot")}
          value={lot}
          onChange={(e) => setLot(e.target.value.replace(/[^0-9.]/g, ""))}
          onBlur={commitLot}
          onKeyDown={(e) => e.key === "Enter" && (e.currentTarget as HTMLInputElement).blur()}
          onWheel={(e) => {
            const v = Math.max(0.01, +(vol + (e.deltaY < 0 ? 0.01 : -0.01)).toFixed(2));
            setLot(v.toFixed(2));
            T.setWs({ lot: v });
          }}
          className="k-num w-full min-w-0 bg-transparent text-center font-mono text-[12px] font-medium text-fg outline-none"
        />
        <button onClick={() => step(1)} className="grid size-5 shrink-0 place-items-center rounded text-[13px] leading-none text-fg-3 hover:bg-surface-3 hover:text-fg" aria-label={t("chart.oneClick.increase")}>+</button>
        </div>
        {!open ? <span className="whitespace-nowrap text-[10px] font-semibold text-warn">{t("chart.oneClick.marketClosed")}</span> : delayed ? <span className="whitespace-nowrap text-[10px] font-semibold text-warn">{t("desk.side.delayed")}</span> : <span className="font-mono text-[10.5px] text-fg-3">{spread}</span>}
      </div>
      <button onClick={() => go("buy")} disabled={!open || !!blocked} title={blocked ?? (open ? (T.guest ? t("trader.guest.title") : undefined) : t("chart.oneClick.marketClosed"))} className={cn("flex flex-col items-end bg-up/12 px-2 py-1 text-right transition-colors hover:bg-up/25 disabled:cursor-not-allowed disabled:bg-surface-2 disabled:opacity-60", compact ? "min-w-[74px]" : "min-w-[92px]")} aria-label={t(open ? "chart.oneClick.buyAria" : "chart.oneClick.buyClosedAria", { symbol })}>
        <span className="flex items-center gap-1 text-[11px] font-semibold text-up">{t("common.buy")}{T.ws.oneClick && !T.guest && <Zap className="size-3 fill-current" aria-hidden />}</span>
        <PriceText symbol={symbol} value={ask} dir={dir} className={cn("justify-end", compact ? "text-[12px]" : "text-[14px]")} />
      </button>
      <button onClick={() => setCollapsed(true)} className="grid w-6 place-items-center border-l border-line bg-panel text-fg-3 hover:text-fg" aria-label={t("chart.oneClick.hide")} title={t("chart.oneClick.hideShort")}>
        <ChevronUp className="size-3.5" />
      </button>
    </div>
  );
}


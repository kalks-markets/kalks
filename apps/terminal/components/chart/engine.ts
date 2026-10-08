"use client";

import * as React from "react";
import {
  AreaSeries,
  BarSeries,
  CandlestickSeries,
  ColorType,
  CrosshairMode,
  HistogramSeries,
  LineSeries,
  LineStyle,
  createChart,
  type IChartApi,
  type IPriceLine,
  type ISeriesApi,
  type SeriesType,
  type UTCTimestamp,
} from "lightweight-charts";
import { candles, fetchCandles, getInstrument, priceFeed, serverOffset, type Candle, type LiveBar } from "@kalks/mock";
import type { IndicatorInstance } from "@/lib/indicators";
import { TF_SECONDS, TIMEFRAMES, type ChartType, type Timeframe } from "@/lib/trading";
import { createIndicatorLayer, type IndLegendRow, type IndicatorLayer } from "./indicators/layer";
import { BrandWatermark } from "./brand-watermark";

/**
 * Charts are drawn in broker server time like MT5: GMT+3 while US daylight saving is active, GMT+2 otherwise
 * (so the day starts at New York close). Always convert with these DST-aware helpers, never a fixed offset.
 */
/** UTC unix seconds → chart time (server time, New York close). */
export const toChartTime = (utc: number) => utc + serverOffset(utc);

/** Chart time → UTC unix seconds (inverse of toChartTime, DST-aware). */
export function fromChartTime(chart: number): number {
  const summer = chart - 3 * 3600;
  return summer + serverOffset(summer) === chart ? summer : chart - 2 * 3600;
}

/* ---- real history from the market-data service, cached per symbol/timeframe ---- */
type History = Candle[] | "sim";
const historyCache = new Map<string, History>();
/** when each real history entry was last brought up to date (ms) */
const historyAt = new Map<string, number>();
const historyWait = new Map<string, Promise<History>>();

// the service came up after a simulator period: simulated histories must never be shown next to live prices
if (typeof window !== "undefined") {
  priceFeed().onMode(() => {
    if (priceFeed().mode !== "live") return;
    for (const [k, h] of historyCache) if (h === "sim") historyCache.delete(k);
  });
}

function loadHistory(symbol: string, tf: Timeframe): Promise<History> {
  const key = `${symbol}|${tf}`;
  const hit = historyCache.get(key);
  if (hit) return Promise.resolve(hit);
  let p = historyWait.get(key);
  if (!p) {
    p = (async (): Promise<History> => {
      // the request goes out at once, in parallel with the feed start-up (it doesn't depend on the quotes)
      const fetching = fetchCandles(symbol, tf, 1500);
      await priceFeed().ready; // resolves once ("sim" stays resolved): read the current mode instead
      if (priceFeed().mode !== "live") return "sim";
      const bars = await fetching;
      if (!bars || bars.length === 0) return "sim";
      return bars.map((b) => ({ ...b, time: toChartTime(b.time) }));
    })().then((h) => {
      // a failed fetch while live is not remembered: the next mount retries
      if (h !== "sim" || priceFeed().mode !== "live") historyCache.set(key, h);
      if (h !== "sim") historyAt.set(key, Date.now());
      historyWait.delete(key);
      return h;
    });
    historyWait.set(key, p);
  }
  return p;
}

/** Starts loading a chart's history before the chart mounts (Kalks Trader start-up: the saved layout's charts are
 *  requested while the session check is still in flight); the chart then picks up the same request. */
export function prefetchHistory(symbol: string, tf: Timeframe) {
  void loadHistory(symbol, tf);
}

/**
 * Candle history for a chart: real bars from our database (or the simulator when the service is offline).
 * A cached history is returned synchronously, so switching symbol/timeframe draws instantly; the engine then
 * refreshes its tail.
 */
function useHistory(symbol: string, tf: Timeframe): History | null {
  const key = `${symbol}|${tf}`;
  const [state, setState] = React.useState<{ key: string; h: History } | null>(() => {
    const hit = historyCache.get(key);
    return hit ? { key, h: hit } : null;
  });
  React.useEffect(() => {
    let alive = true;
    const load = () => void loadHistory(symbol, tf).then((h) => alive && setState((s) => (s && s.key === key && s.h === h ? s : { key, h })));
    load();
    // simulator → live: reload so the chart is rebuilt from real history
    const unmode = priceFeed().onMode(() => priceFeed().mode === "live" && load());
    return () => {
      alive = false;
      unmode();
    };
  }, [key, symbol, tf]);
  if (state && state.key === key && (state.h !== "sim" || historyCache.get(key) === "sim")) return state.h;
  return historyCache.get(key) ?? null;
}

export interface Palette {
  up: string;
  down: string;
  gold: string;
  ember: string;
  warn: string;
  fg: string;
  fg2: string;
  fg3: string;
  grid: string;
  bg: string;
  label: string;
  mono: string;
  dark: boolean;
}

function readVar(el: Element, name: string, fallback: string) {
  const v = getComputedStyle(el).getPropertyValue(name).trim();
  return v || fallback;
}

export function readPalette(el: Element): Palette {
  const dark = !document.documentElement.classList.contains("light");
  return {
    up: readVar(el, "--k-up", "#2f7bff"),
    down: readVar(el, "--k-down", "#f04438"),
    gold: readVar(el, "--k-gold", "#e9b949"),
    ember: readVar(el, "--k-ember", "#ff5a1f"),
    warn: readVar(el, "--k-warn", "#f59e0b"),
    fg: readVar(el, "--k-fg", "#f5f5f7"),
    fg2: readVar(el, "--k-fg-2", "#a1a1aa"),
    fg3: readVar(el, "--k-fg-3", "#63636e"),
    grid: readVar(el, "--t-grid", "rgba(255,255,255,0.035)"),
    bg: readVar(el, "--t-chart-bg", "#0a0a0d"),
    label: dark ? "#26262e" : "#55555f",
    mono: readVar(document.body, "--font-geist-mono", "ui-monospace").replace(/"/g, "'") + ", ui-monospace, monospace",
    dark,
  };
}

/** Seeded history re-timed to "now" and scaled to a timeframe-appropriate volatility. */
export function buildHistory(symbol: string, tf: Timeframe): Candle[] {
  const step = TF_SECONDS[tf];
  const idx = TIMEFRAMES.indexOf(tf);
  const count = tf === "MN" ? 120 : tf === "W1" ? 200 : tf === "D1" ? 300 : 360 + idx;
  const raw = candles(symbol, count, step);
  const k = Math.min(2.6, 0.24 * Math.pow(step / 60, 0.2));
  const last = raw[raw.length - 1]!.close;
  const bid = priceFeed().snapshot(symbol)?.bid ?? last;
  const f = (p: number) => bid * Math.exp(k * Math.log(p / last));
  const now = toChartTime(Math.floor(Date.now() / 1000));
  const lastT = Math.floor(now / step) * step;
  return raw.map((c, i) => {
    const o = f(c.open);
    const cl = f(c.close);
    return { time: lastT - (raw.length - 1 - i) * step, open: o, high: Math.max(f(c.high), o, cl), low: Math.min(f(c.low), o, cl), close: cl, volume: c.volume };
  });
}

export interface LegendData {
  o: number;
  h: number;
  l: number;
  c: number;
  v: number;
  chg: number;
  ind: IndLegendRow[];
}

export interface Engine {
  chart: IChartApi;
  main: ISeriesApi<SeriesType>;
  bars: React.RefObject<Candle[]>;
  askLine: IPriceLine;
  palette: Palette;
  alive: { current: boolean };
  indicators: IndicatorLayer;
}

const t = (x: number) => x as UTCTimestamp;

/**
 * Builds a lightweight-charts instance for one chart window: main series by chart type,
 * volume, registry-driven indicators (overlays on pane 0, oscillators in their own panes); streams live ticks.
 */
export function useChartEngine(
  el: React.RefObject<HTMLDivElement | null>,
  opts: { symbol: string; tf: Timeframe; type: ChartType; indicators: IndicatorInstance[]; theme: string | undefined; crosshair: boolean; onLegend: (l: LegendData) => void },
) {
  const [engine, setEngine] = React.useState<Engine | null>(null);
  const legendRef = React.useRef(opts.onLegend);
  legendRef.current = opts.onLegend;
  const indRef = React.useRef(opts.indicators);
  indRef.current = opts.indicators;
  const indKey = JSON.stringify(opts.indicators);
  const history = useHistory(opts.symbol, opts.tf);

  React.useEffect(() => {
    const host = el.current;
    if (!host || !history) return;
    const { symbol, tf, type } = opts;
    const inst = getInstrument(symbol);
    let c = readPalette(host);
    const step = TF_SECONDS[tf];
    const chart = createChart(host, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: c.bg },
        textColor: c.fg3,
        fontFamily: c.mono,
        fontSize: 10.5,
        attributionLogo: false,
        panes: { separatorColor: c.dark ? "rgba(255,255,255,0.07)" : "rgba(15,15,20,0.1)", separatorHoverColor: "rgba(255,90,31,0.35)", enableResize: true },
      },
      grid: { vertLines: { color: c.grid }, horzLines: { color: c.grid } },
      rightPriceScale: { borderVisible: true, borderColor: c.dark ? "rgba(255,255,255,0.07)" : "rgba(15,15,20,0.1)", scaleMargins: { top: 0.12, bottom: 0.14 }, minimumWidth: 68 },
      timeScale: { borderVisible: true, borderColor: c.dark ? "rgba(255,255,255,0.07)" : "rgba(15,15,20,0.1)", timeVisible: step < 86400, secondsVisible: false, rightOffset: 12, barSpacing: 7, minBarSpacing: 1.5 },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: c.fg3, width: 1, style: LineStyle.Dashed, labelBackgroundColor: c.label },
        horzLine: { color: c.fg3, width: 1, style: LineStyle.Dashed, labelBackgroundColor: c.label },
      },
      localization: { priceFormatter: (p: number) => p.toFixed(inst.digits) },
    });

    const pf = { type: "price" as const, precision: inst.digits, minMove: 1 / 10 ** inst.digits };
    let main: ISeriesApi<SeriesType>;
    if (type === "candles") main = chart.addSeries(CandlestickSeries, { upColor: c.up, downColor: c.down, borderVisible: false, wickUpColor: c.up, wickDownColor: c.down, priceFormat: pf });
    else if (type === "bars") main = chart.addSeries(BarSeries, { upColor: c.up, downColor: c.down, thinBars: false, priceFormat: pf });
    else if (type === "line") main = chart.addSeries(LineSeries, { color: c.gold, lineWidth: 2, priceFormat: pf });
    else main = chart.addSeries(AreaSeries, { lineColor: c.gold, topColor: "rgba(233,185,73,0.28)", bottomColor: "rgba(233,185,73,0.0)", lineWidth: 2, priceFormat: pf });
    // candles are built from the raw last trade price (the market's price, same for every account); the axis
    // shows the account's executable Bid and Ask instead of the last close, exactly as in the market watch
    main.applyOptions({ priceLineVisible: false, lastValueVisible: false, title: "" });

    const vol = chart.addSeries(HistogramSeries, { priceScaleId: "vol", priceLineVisible: false, lastValueVisible: false, priceFormat: { type: "volume" } });
    chart.priceScale("vol").applyOptions({ scaleMargins: { top: 0.86, bottom: 0 } });

    // Kalks mark + "SYMBOL, TF" + instrument name: one faint broker watermark, drawn on the canvas (in screenshots too)
    const watermark = new BrandWatermark({ symbol, tf, name: inst.name, dark: c.dark, font: readVar(document.body, "--font-geist-sans", "system-ui").replace(/"/g, "'") + ", system-ui, sans-serif" });
    chart.panes()[0]!.attachPrimitive(watermark);

    const live = history !== "sim";
    const data = history === "sim" ? buildHistory(symbol, tf) : history.map((d) => ({ ...d }));
    const bars = { current: data } as React.RefObject<Candle[]>;
    const alive = { current: true };
    const volColor = (d: Candle) => (d.close >= d.open ? `${c.up}38` : `${c.down}38`); // reads the live palette `c`
    const mainPoint = (d: Candle) => (type === "line" || type === "area" ? { time: t(d.time), value: d.close } : { time: t(d.time), open: d.open, high: d.high, low: d.low, close: d.close });
    main.setData(data.map(mainPoint));
    vol.setData(data.map((d) => ({ time: t(d.time), value: d.volume, color: volColor(d) })));

    /* ---- indicators: registry-driven layer (./indicators/layer.ts), incremental on ticks ---- */
    let indReady = false;
    const indicators = createIndicatorLayer({ chart, bars: () => data, step, digits: inst.digits, palette: c, alive: () => alive.current, onChange:() => indReady && legendRef.current(legendAt(hovering >= 0 ? hovering : data.length - 1)) });
    indicators.sync(indRef.current);

    // viewport: default bar spacing anchored at the latest bar (re-applied once the container is measured)
    chart.timeScale().scrollToRealTime();
    const raf = requestAnimationFrame(() => {
      chart.timeScale().applyOptions({ barSpacing: 7, rightOffset: 12 });
      chart.timeScale().scrollToRealTime();
    });

    /* ---- bid / ask lines (account group's spread): the ask is the buy price, in the buy colour ---- */
    const snap = priceFeed().quote(symbol);
    const bidLine = main.createPriceLine({ price: snap.bid, color: c.fg2, lineWidth: 1, lineStyle: LineStyle.Dotted, axisLabelVisible: true, title: "", axisLabelColor: c.fg2, axisLabelTextColor: c.dark ? "#0a0a0d" : "#fff" });
    const askLine = main.createPriceLine({ price: snap.ask, color: c.up, lineWidth: 1, lineStyle: LineStyle.Dotted, axisLabelVisible: true, title: "", axisLabelColor: c.up, axisLabelTextColor: "#fff" });

    /* ---- legend ---- */
    // today's open from the service (rolls at New York close); the simulator falls back to ~1 day of bars back
    const dayOpenOf = () => priceFeed().day(symbol)?.open ?? data[Math.max(0, data.length - Math.max(1, Math.round(86400 / step)))]!.open;
    const legendAt = (i: number): LegendData => {
      const d = data[i]!;
      const dayOpen = dayOpenOf();
      return {
        o: d.open,
        h: d.high,
        l: d.low,
        c: d.close,
        v: d.volume,
        chg: ((d.close - dayOpen) / dayOpen) * 100,
        ind: indicators.legend(i),
      };
    };
    legendRef.current(legendAt(data.length - 1));
    let hovering = -1;
    indReady = true;
    chart.subscribeCrosshairMove((p) => {
      if (p.logical === undefined || p.logical === null || !p.time) {
        hovering = -1;
        return legendRef.current(legendAt(data.length - 1));
      }
      const i = Math.max(0, Math.min(data.length - 1, Math.round(p.logical)));
      hovering = i;
      legendRef.current(legendAt(i));
    });

    const redraw = (last: Candle) => {
      main.update(mainPoint(last));
      vol.update({ time: t(last.time), value: last.volume, color: volColor(last) });
      indicators.update();
      if (hovering < 0) legendRef.current(legendAt(data.length - 1));
    };

    /* ---- live: the service pushes the exact forming bar (same candle as stored); quotes move the ask line ---- */
    let unsubBars = () => {};
    let lastLive: Candle | null = null;
    if (live) {
      unsubBars = priceFeed().subscribeBars(symbol, tf, (b: LiveBar) => {
        const bar: Candle = { time: toChartTime(b.t), open: b.o, high: b.h, low: b.l, close: b.c, volume: b.v };
        lastLive = bar;
        const last = data[data.length - 1]!;
        if (bar.time < last.time) return;
        if (bar.time > last.time) data.push(bar);
        else data[data.length - 1] = bar;
        redraw(bar);
      });
    }
    const unsubQuotes = priceFeed().subscribe([symbol], (q) => {
      bidLine.applyOptions({ price: q.bid });
      askLine.applyOptions({ price: q.ask });
      if (live) return;
      // simulator: build the forming bar from bid ticks (bucketed in DST-aware server time)
      const tt = Math.floor(toChartTime(Math.floor(q.time / 1000)) / step) * step;
      let last = data[data.length - 1]!;
      if (tt > last.time) {
        last = { time: tt, open: last.close, high: Math.max(last.close, q.bid), low: Math.min(last.close, q.bid), close: q.bid, volume: 1 };
        data.push(last);
      } else {
        last = { ...last, high: Math.max(last.high, q.bid), low: Math.min(last.low, q.bid), close: q.bid, volume: last.volume + Math.round(1 + Math.random() * 6) };
        data[data.length - 1] = last;
      }
      redraw(last);
    });
    const unsub = () => {
      unsubBars();
      unsubQuotes();
    };

    /* ---- scroll back: load older bars from the database when the left edge comes into view ---- */
    let loadingOlder = false;
    let exhausted = !live;
    const onRange = (r: { from: number; to: number } | null) => {
      if (!r || exhausted || loadingOlder || r.from > 30) return;
      loadingOlder = true;
      const first = data[0]!.time;
      void fetchCandles(symbol, tf, 1500, fromChartTime(first) - 1).then((older) => {
        loadingOlder = false;
        if (!alive.current) return;
        const add = (older ?? []).map((b) => ({ ...b, time: toChartTime(b.time) })).filter((b) => b.time < first);
        if (add.length === 0) {
          exhausted = true;
          return;
        }
        data.unshift(...add);
        historyCache.set(`${symbol}|${tf}`, data.map((d) => ({ ...d })));
        main.setData(data.map(mainPoint));
        vol.setData(data.map((d) => ({ time: t(d.time), value: d.volume, color: volColor(d) })));
        indicators.reset();
      });
    };
    chart.timeScale().subscribeVisibleLogicalRangeChange(onRange);

    /* ---- tail refresh: a cached history is behind after a remount, a reconnect or a hidden tab — merge the latest bars ---- */
    const key = `${symbol}|${tf}`;
    let refreshing = false;
    const refreshTail = () => {
      if (!live || refreshing) return;
      refreshing = true;
      const lastT = data[data.length - 1]!.time;
      const limit = Math.min(1500, Math.ceil((toChartTime(Math.floor(Date.now() / 1000)) - lastT) / step) + 3);
      void fetchCandles(symbol, tf, limit).then((fresh) => {
        refreshing = false;
        if (!alive.current || !fresh || fresh.length === 0) return;
        const tail = fresh.map((b) => ({ ...b, time: toChartTime(b.time) }));
        const from = tail[0]!.time;
        if (tail.length >= limit && from > lastT) {
          data.splice(0, data.length, ...tail); // the gap is longer than one page: start over from the fresh page
          exhausted = false;
        } else {
          let i = data.length;
          while (i > 0 && data[i - 1]!.time >= from) i--;
          data.splice(i, data.length - i, ...tail);
        }
        // a stream bar that arrived while the request was in flight is newer than the response
        const lb = lastLive as Candle | null;
        if (lb && lb.time >= data[data.length - 1]!.time) {
          if (lb.time > data[data.length - 1]!.time) data.push(lb);
          else data[data.length - 1] = lb;
        }
        historyCache.set(key, data.map((d) => ({ ...d })));
        historyAt.set(key, Date.now());
        main.setData(data.map(mainPoint));
        vol.setData(data.map((d) => ({ time: t(d.time), value: d.volume, color: volColor(d) })));
        indicators.reset();
        if (hovering < 0) legendRef.current(legendAt(data.length - 1));
      });
    };
    // switching back to a cached symbol/timeframe drew instantly from the cache: now bring it up to date
    if (live && Date.now() - (historyAt.get(key) ?? 0) > 2000) refreshTail();
    const unresync = priceFeed().onResync(refreshTail);

    /* ---- theme: recolour in place (no rebuild, keeps zoom/scroll) ---- */
    const applyTheme = () => {
      c = readPalette(host);
      const line = c.dark ? "rgba(255,255,255,0.07)" : "rgba(15,15,20,0.1)";
      chart.applyOptions({
        layout: { background: { type: ColorType.Solid, color: c.bg }, textColor: c.fg3, panes: { separatorColor: line } },
        grid: { vertLines: { color: c.grid }, horzLines: { color: c.grid } },
        rightPriceScale: { borderColor: line },
        timeScale: { borderColor: line },
        crosshair: { vertLine: { color: c.fg3, labelBackgroundColor: c.label }, horzLine: { color: c.fg3, labelBackgroundColor: c.label } },
      });
      if (type === "candles") main.applyOptions({ upColor: c.up, downColor: c.down, wickUpColor: c.up, wickDownColor: c.down } as never);
      else if (type === "bars") main.applyOptions({ upColor: c.up, downColor: c.down } as never);
      else if (type === "line") main.applyOptions({ color: c.gold } as never);
      else main.applyOptions({ lineColor: c.gold } as never);
      main.applyOptions({ priceLineColor: c.fg2 });
      vol.setData(data.map((d) => ({ time: t(d.time), value: d.volume, color: volColor(d) })));
      indicators.setPalette(c);
      askLine.applyOptions({ color: c.up, axisLabelColor: c.up });
      bidLine.applyOptions({ color: c.fg2, axisLabelColor: c.fg2, axisLabelTextColor: c.dark ? "#0a0a0d" : "#fff" });
      watermark.applyOptions({ dark: c.dark });
      legendRef.current(legendAt(hovering >= 0 ? hovering : data.length - 1));
      setEngine((e) => (e && e.chart === chart ? { ...e, palette: c } : e));
    };
    let themeRaf = 0;
    const themeObs = new MutationObserver(() => {
      cancelAnimationFrame(themeRaf);
      themeRaf = requestAnimationFrame(applyTheme); // wait one frame so the new CSS variables are computed
    });
    themeObs.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "style"] });

    setEngine({ chart, main, bars, askLine, palette: c, alive, indicators });
    return () => {
      alive.current = false;
      themeObs.disconnect();
      cancelAnimationFrame(themeRaf);
      cancelAnimationFrame(raf);
      unsub();
      unresync();
      chart.timeScale().unsubscribeVisibleLogicalRangeChange(onRange);
      setEngine(null);
      chart.remove();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [opts.symbol, opts.tf, opts.type, history]);

  // crosshair visibility follows the drawing tool (cursor = labels only, crosshair = full lines)
  React.useEffect(() => {
    if (!engine) return;
    engine.chart.applyOptions({
      crosshair: {
        vertLine: { visible: opts.crosshair, labelVisible: true },
        horzLine: { visible: opts.crosshair, labelVisible: true },
      },
    });
  }, [engine, opts.crosshair]);

  // indicator list edits apply in place (no chart rebuild: zoom, scroll and pane sizes survive)
  React.useEffect(() => {
    if (!engine || !engine.alive.current) return;
    engine.indicators.sync(indRef.current);
  }, [engine, indKey]);

  return engine;
}

/** Imperative handles to each mounted chart, used by the toolbar (zoom, screenshot…). */
export interface ChartHandle {
  zoom: (dir: 1 | -1) => void;
  fit: () => void;
  screenshot: () => void;
}
export const chartRegistry = new Map<string, ChartHandle>();

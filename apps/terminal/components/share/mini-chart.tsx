"use client";

import * as React from "react";
import { CandlestickSeries, ColorType, CrosshairMode, LineStyle, createChart, createSeriesMarkers, type SeriesMarker, type Time, type UTCTimestamp } from "lightweight-charts";
import { fetchCandles, getInstrument, priceFeed, serverOffset } from "@kalks/mock";
import type { ShareTrade } from "@/lib/share";

const TFS = [
  ["M1", 60],
  ["M5", 300],
  ["M15", 900],
  ["H1", 3600],
  ["H4", 14400],
  ["D1", 86400],
] as const;

/** Chart time = server (broker) time, like the terminal chart. */
const toChart = (utc: number) => (utc + serverOffset(utc)) as UTCTimestamp;

function pickTf(spanSec: number) {
  for (const [tf, sec] of TFS) if (spanSec / sec <= 90) return { tf, sec };
  return { tf: "D1", sec: 86400 };
}

function css(host: HTMLElement, name: string, fallback: string) {
  return getComputedStyle(host).getPropertyValue(name).trim() || fallback;
}

/**
 * Small candle chart of the trade's symbol with entry / exit markers and SL, TP, entry lines.
 * Real candles from the market-data service; loads when scrolled into view; open trades keep the last bar live.
 */
export function TradeMiniChart({ trade, height = 128 }: { trade: ShareTrade; height?: number }) {
  const host = React.useRef<HTMLDivElement>(null);
  const [visible, setVisible] = React.useState(false);
  const [state, setState] = React.useState<"loading" | "ready" | "empty">("loading");

  React.useEffect(() => {
    const el = host.current;
    if (!el) return;
    const io = new IntersectionObserver((e) => e.some((x) => x.isIntersecting) && (setVisible(true), io.disconnect()), { rootMargin: "200px" });
    io.observe(el);
    return () => io.disconnect();
  }, []);

  const key = `${trade.ticket}|${trade.status}|${trade.closeTime ?? ""}|${trade.sl ?? ""}|${trade.tp ?? ""}`;
  React.useEffect(() => {
    const el = host.current;
    if (!visible || !el) return;
    let alive = true;
    let cleanup = () => {};
    const openSec = Math.floor(Date.parse(trade.openTime) / 1000);
    const endSec = trade.closeTime ? Math.floor(Date.parse(trade.closeTime) / 1000) : Math.floor(Date.now() / 1000);
    const span = Math.max(600, endSec - openSec);
    const { tf, sec } = pickTf(span);
    const pad = Math.max(span * 0.35, sec * 12);
    const to = trade.closeTime ? Math.floor(endSec + pad * 0.5) : undefined;
    const limit = Math.min(600, Math.ceil((span + pad * 1.5) / sec) + 10);
    void fetchCandles(trade.symbol, tf, limit, to).then((bars) => {
      if (!alive) return;
      if (!bars || bars.length < 3) return setState("empty");
      const inst = getInstrument(trade.symbol);
      const c = {
        up: css(el, "--k-up", "#2f7bff"),
        down: css(el, "--k-down", "#f04438"),
        gold: css(el, "--k-gold", "#e9b949"),
        fg: css(el, "--k-fg-2", "#a1a1aa"),
        fg3: css(el, "--k-fg-3", "#63636e"),
      };
      const chart = createChart(el, {
        autoSize: true,
        layout: { background: { type: ColorType.Solid, color: "transparent" }, textColor: c.fg3, fontSize: 10, attributionLogo: false },
        grid: { vertLines: { visible: false }, horzLines: { color: "rgba(255,255,255,0.035)" } },
        rightPriceScale: { borderVisible: false, scaleMargins: { top: 0.2, bottom: 0.2 } },
        timeScale: { borderVisible: false, visible: false, rightOffset: 4 },
        crosshair: { mode: CrosshairMode.Hidden },
        handleScroll: false,
        handleScale: false,
        localization: { priceFormatter: (p: number) => p.toFixed(inst.digits) },
      });
      const series = chart.addSeries(CandlestickSeries, {
        upColor: c.up,
        downColor: c.down,
        borderVisible: false,
        wickUpColor: c.up,
        wickDownColor: c.down,
        priceLineVisible: false,
        lastValueVisible: !trade.closeTime,
        priceFormat: { type: "price", precision: inst.digits, minMove: 1 / 10 ** inst.digits },
      });
      const data = bars.map((b) => ({ time: toChart(b.time), open: b.open, high: b.high, low: b.low, close: b.close }));
      series.setData(data);
      const bucket = (utc: number) => toChart(Math.floor(utc / sec) * sec);
      // markers must sit on a bar that exists (history can be sparse); a marker on a missing bar breaks autoscale
      const snap = (utc: number): UTCTimestamp | null => {
        const t = bucket(utc) as number;
        let hit: number | null = null;
        for (const d of data) if ((d.time as number) <= t) hit = d.time as number;
        return hit !== null && t - hit <= sec * 3 ? (hit as UTCTimestamp) : null;
      };
      const buy = trade.side === "buy";
      const markers: SeriesMarker<Time>[] = [];
      const entryAt = snap(openSec);
      if (entryAt !== null && trade.status !== "pending" && trade.status !== "cancelled") {
        markers.push({ time: entryAt, position: buy ? "belowBar" : "aboveBar", shape: buy ? "arrowUp" : "arrowDown", color: buy ? c.up : c.down, size: 0.7 });
      }
      const exitAt = trade.status === "closed" ? snap(endSec) : null;
      if (exitAt !== null) markers.push({ time: exitAt, position: buy ? "aboveBar" : "belowBar", shape: "circle", color: c.gold, size: 0.6 });
      // markers must not drive autoscale: on a short chart their pixel padding blows the price range up
      createSeriesMarkers(series, markers, { autoScale: false });
      series.createPriceLine({ price: trade.openPrice, color: c.gold, lineWidth: 1, lineStyle: LineStyle.Solid, axisLabelVisible: false, title: trade.status === "pending" ? "Order" : "Entry" });
      if (trade.sl) series.createPriceLine({ price: trade.sl, color: c.down, lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: false, title: "SL" });
      if (trade.tp) series.createPriceLine({ price: trade.tp, color: c.up, lineWidth: 1, lineStyle: LineStyle.Dashed, axisLabelVisible: false, title: "TP" });
      if (trade.closePrice) series.createPriceLine({ price: trade.closePrice, color: c.fg, lineWidth: 1, lineStyle: LineStyle.Dotted, axisLabelVisible: false, title: "Exit" });
      const from = toChart(Math.max(bars[0]!.time, openSec - pad));
      chart.timeScale().setVisibleRange({ from, to: data[data.length - 1]!.time });
      setState("ready");

      // live last bar for trades still running
      let unsub = () => {};
      if (trade.status === "open" || trade.status === "pending") {
        let last = { ...data[data.length - 1]! };
        unsub = priceFeed().subscribe([trade.symbol], (q) => {
          const px = q.last || (q.bid + q.ask) / 2;
          const t = bucket(Math.floor((q.time || Date.now()) / 1000));
          if ((t as number) > (last.time as number)) last = { time: t, open: last.close, high: px, low: px, close: px };
          else last = { ...last, high: Math.max(last.high, px), low: Math.min(last.low, px), close: px };
          series.update(last);
        });
      }
      cleanup = () => {
        unsub();
        chart.remove();
      };
    });
    return () => {
      alive = false;
      cleanup();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visible, key]);

  return (
    <div className="relative" style={{ height }}>
      <div ref={host} className="absolute inset-0" />
      {state !== "ready" && (
        <div className="absolute inset-0 grid place-items-center text-[11px] text-fg-3">{state === "loading" ? "Loading chart…" : "Chart unavailable"}</div>
      )}
    </div>
  );
}

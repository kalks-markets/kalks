/**
 * Kalks indicator library — pure maths, no DOM / chart dependencies (safe to import from workers, the AI trader, tests).
 *
 * Every indicator is a registry entry ({@link INDICATOR_DEFS}) with typed params, outputs and a `calc(ctx, from)` kernel that
 * fills output arrays for bar indices `[from, n)`. Arrays are aligned with the input bars and hold NaN until warm.
 * Kernels only read values `< from` that they wrote earlier, so the same code serves both a full pass (`from = 0`) and an
 * incremental tick (`from = n - 1`) — see {@link IndicatorComputation}.
 *
 * Stable public API:
 *   calcIndicator(type, bars, params?)       → { [output]: number[] } (full pass)
 *   new IndicatorComputation(type, params)   → .run(bars) incremental re-use across ticks
 *   INDICATOR_DEFS / getIndicatorDef / normalizeParams / defaultParams / makeInstance / migrateIndicators
 *   sma / ema / bollinger / rsi / macd       (legacy close-array helpers)
 */

/* ------------------------------------------------------------------ */
/* Legacy close-array helpers (kept for existing importers)            */
/* ------------------------------------------------------------------ */

export function sma(v: number[], n: number): number[] {
  const out = new Array<number>(v.length).fill(NaN);
  let s = 0;
  for (let i = 0; i < v.length; i++) {
    s += v[i]!;
    if (i >= n) s -= v[i - n]!;
    if (i >= n - 1) out[i] = s / n;
  }
  return out;
}

export function ema(v: number[], n: number): number[] {
  const out = new Array<number>(v.length).fill(NaN);
  const k = 2 / (n + 1);
  let prev = NaN;
  for (let i = 0; i < v.length; i++) {
    if (i === n - 1) {
      let s = 0;
      for (let j = 0; j < n; j++) s += v[j]!;
      prev = s / n;
    } else if (i >= n) prev = v[i]! * k + prev * (1 - k);
    if (i >= n - 1) out[i] = prev;
  }
  return out;
}

export function bollinger(v: number[], n = 20, mult = 2) {
  const mid = sma(v, n);
  const up = new Array<number>(v.length).fill(NaN);
  const lo = new Array<number>(v.length).fill(NaN);
  for (let i = n - 1; i < v.length; i++) {
    let s = 0;
    for (let j = i - n + 1; j <= i; j++) s += (v[j]! - mid[i]!) ** 2;
    const sd = Math.sqrt(s / n);
    up[i] = mid[i]! + mult * sd;
    lo[i] = mid[i]! - mult * sd;
  }
  return { mid, up, lo };
}

export function rsi(v: number[], n = 14): number[] {
  const out = new Array<number>(v.length).fill(NaN);
  let g = 0;
  let l = 0;
  for (let i = 1; i < v.length; i++) {
    const d = v[i]! - v[i - 1]!;
    const up = Math.max(d, 0);
    const dn = Math.max(-d, 0);
    if (i <= n) {
      g += up;
      l += dn;
      if (i === n) {
        g /= n;
        l /= n;
        out[i] = l === 0 ? 100 : 100 - 100 / (1 + g / l);
      }
    } else {
      g = (g * (n - 1) + up) / n;
      l = (l * (n - 1) + dn) / n;
      out[i] = l === 0 ? 100 : 100 - 100 / (1 + g / l);
    }
  }
  return out;
}

export function macd(v: number[], fast = 12, slow = 26, signal = 9) {
  const f = ema(v, fast);
  const s = ema(v, slow);
  const line = v.map((_, i) => f[i]! - s[i]!);
  const start = line.findIndex((x) => !Number.isNaN(x));
  const sigPart = ema(line.slice(start), signal);
  const sig = line.map((_, i) => (i < start ? NaN : sigPart[i - start]!));
  const hist = line.map((x, i) => x - sig[i]!);
  return { line, signal: sig, hist };
}

/* ------------------------------------------------------------------ */
/* Types                                                               */
/* ------------------------------------------------------------------ */

/** OHLCV bar. `time` is chart time in seconds (server time); day boundaries use floor(time / 86400). */
export interface Bar {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

export const INDICATOR_CATEGORIES = ["Trend", "Oscillators", "Volatility", "Volume", "Bill Williams"] as const;
export type IndicatorCategory = (typeof INDICATOR_CATEGORIES)[number];

export const SOURCES = ["close", "open", "high", "low", "hl2", "hlc3", "ohlc4"] as const;
export type Source = (typeof SOURCES)[number];
export const SOURCE_LABEL: Record<Source, string> = { close: "Close", open: "Open", high: "High", low: "Low", hl2: "Median (HL/2)", hlc3: "Typical (HLC/3)", ohlc4: "Weighted (OHLC/4)" };

export type MaMethod = "sma" | "ema" | "smma" | "wma";
const MA_OPTIONS = [
  { value: "sma", label: "Simple" },
  { value: "ema", label: "Exponential" },
  { value: "smma", label: "Smoothed" },
  { value: "wma", label: "Linear weighted" },
];

export type ParamValue = number | string;
export type Params = Record<string, ParamValue>;

export type ParamDef =
  | { key: string; label: string; kind: "int"; def: number; min: number; max: number }
  | { key: string; label: string; kind: "float"; def: number; min: number; max: number; step: number }
  | { key: string; label: string; kind: "source"; def: Source }
  | { key: string; label: string; kind: "select"; def: string; options: { value: string; label: string }[] };

/** Theme colour tokens (resolved against the live palette so dark/light both work) or any CSS colour. */
export type ColorToken = "ember" | "gold" | "up" | "down" | "warn" | "info" | "fg" | "fg2" | "fg3";
export const COLOR_TOKENS: ColorToken[] = ["ember", "gold", "up", "down", "warn", "info", "fg2", "fg3"];

/** How a value is formatted: symbol price digits, price digits + 1, a fixed number of decimals, or volume. */
export type ValueFormat = "price" | "price+" | "vol" | number;

export interface OutputDef {
  key: string;
  label: string;
  kind: "line" | "hist" | "dots" | "markers";
  color: ColorToken | string;
  width?: 1 | 2 | 3 | 4;
  dashed?: boolean;
  stepped?: boolean;
  /** Bars to shift the plot by (+ = into the future, − = into the past), from params. */
  shift?: (p: Params) => number;
  /** hist: colour by sign (≥0 up) or by direction vs previous bar (rising up). */
  histColor?: "sign" | "trend";
  /** markers: arrow above (up) or below (down) the value. */
  marker?: "up" | "down";
  fmt?: ValueFormat;
  /** false = never widens the price scale (pivot levels far from price). */
  autoscale?: boolean;
}

export interface FillDef {
  upper: string;
  lower: string;
  color: ColorToken;
  /** When set, segments where lower > upper use this colour (Ichimoku cloud). */
  downColor?: ColorToken;
  alpha: number;
}

export interface IndicatorDef {
  type: IndicatorType;
  name: string;
  short: string;
  category: IndicatorCategory;
  pane: "overlay" | "separate";
  description: string;
  params: ParamDef[];
  outputs: OutputDef[];
  fills?: FillDef[];
  levels?: (p: Params) => number[];
  /** Fixed value range for the pane's scale (bounded oscillators). */
  range?: [number, number];
  /** Bars that must follow a bar before its value is final (Fractals = 2). */
  lookahead?: number;
  calc: (c: CalcCtx, from: number) => void;
}

export interface CalcCtx {
  b: Bar[];
  p: Params;
  o: Record<string, number[]>;
}

/* ------------------------------------------------------------------ */
/* Kernels (index-wise; NaN propagates through every window)           */
/* ------------------------------------------------------------------ */

/** Output/internal array `name`, sized to the bar count (new slots NaN). */
function A(c: CalcCtx, name: string): number[] {
  let a = c.o[name];
  if (!a) a = c.o[name] = [];
  const n = c.b.length;
  while (a.length < n) a.push(NaN);
  if (a.length > n) a.length = n;
  return a;
}

function srcOf(b: Bar, s: ParamValue): number {
  switch (s) {
    case "open":
      return b.open;
    case "high":
      return b.high;
    case "low":
      return b.low;
    case "hl2":
      return (b.high + b.low) / 2;
    case "hlc3":
      return (b.high + b.low + b.close) / 3;
    case "ohlc4":
      return (b.open + b.high + b.low + b.close) / 4;
    default:
      return b.close;
  }
}

function fillSrc(c: CalcCtx, name: string, s: ParamValue, from: number): number[] {
  const a = A(c, name);
  for (let i = from; i < c.b.length; i++) a[i] = srcOf(c.b[i]!, s);
  return a;
}

export function smaAt(x: number[], i: number, n: number): number {
  if (i < n - 1) return NaN;
  let s = 0;
  for (let j = i - n + 1; j <= i; j++) s += x[j]!;
  return s / n;
}

export function wmaAt(x: number[], i: number, n: number): number {
  if (i < n - 1) return NaN;
  let s = 0;
  let w = 0;
  for (let k = 0; k < n; k++) {
    const wt = n - k;
    s += x[i - k]! * wt;
    w += wt;
  }
  return s / w;
}

/** Recursive average seeded with the SMA of the first full window (EMA: alpha 2/(n+1), Wilder/SMMA: 1/n). */
export function emaAt(x: number[], out: number[], i: number, n: number, alpha = 2 / (n + 1)): number {
  const prev = i > 0 ? out[i - 1]! : NaN;
  if (Number.isNaN(prev)) return smaAt(x, i, n);
  return prev + alpha * (x[i]! - prev);
}

export const rmaAt = (x: number[], out: number[], i: number, n: number) => emaAt(x, out, i, n, 1 / n);

function maAt(method: ParamValue, x: number[], out: number[], i: number, n: number): number {
  if (method === "ema") return emaAt(x, out, i, n);
  if (method === "smma" || method === "rma") return rmaAt(x, out, i, n);
  if (method === "wma") return wmaAt(x, i, n);
  return smaAt(x, i, n);
}

export function stdevAt(x: number[], i: number, n: number, mean = smaAt(x, i, n)): number {
  if (Number.isNaN(mean)) return NaN;
  let s = 0;
  for (let j = i - n + 1; j <= i; j++) s += (x[j]! - mean) ** 2;
  return Math.sqrt(s / n);
}

function maxAt(x: number[], i: number, n: number): number {
  if (i < n - 1) return NaN;
  let m = -Infinity;
  for (let j = i - n + 1; j <= i; j++) {
    const v = x[j]!;
    if (Number.isNaN(v)) return NaN;
    if (v > m) m = v;
  }
  return m;
}
function minAt(x: number[], i: number, n: number): number {
  if (i < n - 1) return NaN;
  let m = Infinity;
  for (let j = i - n + 1; j <= i; j++) {
    const v = x[j]!;
    if (Number.isNaN(v)) return NaN;
    if (v < m) m = v;
  }
  return m;
}
function hhAt(b: Bar[], i: number, n: number): number {
  if (i < n - 1) return NaN;
  let m = -Infinity;
  for (let j = i - n + 1; j <= i; j++) if (b[j]!.high > m) m = b[j]!.high;
  return m;
}
function llAt(b: Bar[], i: number, n: number): number {
  if (i < n - 1) return NaN;
  let m = Infinity;
  for (let j = i - n + 1; j <= i; j++) if (b[j]!.low < m) m = b[j]!.low;
  return m;
}
function trueRange(b: Bar[], i: number): number {
  const x = b[i]!;
  if (i === 0) return x.high - x.low;
  const pc = b[i - 1]!.close;
  return Math.max(x.high - x.low, Math.abs(x.high - pc), Math.abs(x.low - pc));
}
const num = (v: ParamValue) => (typeof v === "number" ? v : parseFloat(v));
const int = (v: ParamValue) => Math.round(num(v));
const day = (t: number) => Math.floor(t / 86400);

/* ------------------------------------------------------------------ */
/* Registry                                                            */
/* ------------------------------------------------------------------ */

const P = {
  period: (def: number, label = "Period", max = 500): ParamDef => ({ key: "period", label, kind: "int", def, min: 1, max }),
  int: (key: string, label: string, def: number, min = 1, max = 500): ParamDef => ({ key, label, kind: "int", def, min, max }),
  float: (key: string, label: string, def: number, min: number, max: number, step: number): ParamDef => ({ key, label, kind: "float", def, min, max, step }),
  source: (def: Source = "close"): ParamDef => ({ key: "source", label: "Source", kind: "source", def }),
  method: (def: MaMethod = "sma", key = "method", label = "MA method"): ParamDef => ({ key, label, kind: "select", def, options: MA_OPTIONS }),
};

type MaKind = "sma" | "ema" | "wma" | "smma" | "dema" | "tema" | "hma";

function maDef(type: MaKind, name: string, short: string, color: ColorToken, description: string): IndicatorDef {
  return {
    type,
    name,
    short,
    category: "Trend",
    pane: "overlay",
    description,
    params: [P.period(type === "sma" ? 20 : type === "ema" ? 50 : 20), P.source()],
    outputs: [{ key: "ma", label: short, kind: "line", color, width: 2, fmt: "price" }],
    calc(c, from) {
      const n = int(c.p.period);
      const s = fillSrc(c, "_s", c.p.source, from);
      const ma = A(c, "ma");
      if (type === "sma") for (let i = from; i < s.length; i++) ma[i] = smaAt(s, i, n);
      else if (type === "ema") for (let i = from; i < s.length; i++) ma[i] = emaAt(s, ma, i, n);
      else if (type === "smma") for (let i = from; i < s.length; i++) ma[i] = rmaAt(s, ma, i, n);
      else if (type === "wma") for (let i = from; i < s.length; i++) ma[i] = wmaAt(s, i, n);
      else if (type === "dema" || type === "tema") {
        const e1 = A(c, "_e1");
        const e2 = A(c, "_e2");
        const e3 = A(c, "_e3");
        for (let i = from; i < s.length; i++) {
          e1[i] = emaAt(s, e1, i, n);
          e2[i] = emaAt(e1, e2, i, n);
          if (type === "dema") ma[i] = 2 * e1[i]! - e2[i]!;
          else {
            e3[i] = emaAt(e2, e3, i, n);
            ma[i] = 3 * e1[i]! - 3 * e2[i]! + e3[i]!;
          }
        }
      } else {
        // Hull: WMA(2·WMA(n/2) − WMA(n), √n)
        const half = Math.max(1, Math.floor(n / 2));
        const sq = Math.max(1, Math.floor(Math.sqrt(n)));
        const raw = A(c, "_raw");
        for (let i = from; i < s.length; i++) {
          raw[i] = 2 * wmaAt(s, i, half) - wmaAt(s, i, n);
          ma[i] = wmaAt(raw, i, sq);
        }
      }
    },
  };
}

const DEFS_LIST: IndicatorDef[] = [
  /* ---------------- Trend ---------------- */
  maDef("sma", "Moving Average (Simple)", "SMA", "ember", "Arithmetic mean of the last N prices."),
  maDef("ema", "Moving Average (Exponential)", "EMA", "gold", "Weights recent prices exponentially more, seeded with the SMA."),
  maDef("wma", "Moving Average (Linear Weighted)", "WMA", "info", "Linearly weighted average, newest price weight N."),
  maDef("smma", "Moving Average (Smoothed)", "SMMA", "up", "Wilder's smoothed average (RMA), alpha 1/N."),
  maDef("dema", "Double Exponential MA", "DEMA", "warn", "2·EMA − EMA(EMA): less lag than an EMA."),
  maDef("tema", "Triple Exponential MA", "TEMA", "down", "3·EMA − 3·EMA² + EMA³."),
  maDef("hma", "Hull Moving Average", "HMA", "fg2", "WMA(2·WMA(N/2) − WMA(N), √N): smooth and responsive."),
  {
    type: "vwap",
    name: "VWAP (Session)",
    short: "VWAP",
    category: "Trend",
    pane: "overlay",
    description: "Volume-weighted average of the typical price, reset at each server-day open.",
    params: [P.source("hlc3")],
    outputs: [{ key: "vwap", label: "VWAP", kind: "line", color: "info", width: 2, fmt: "price" }],
    calc(c, from) {
      const { b } = c;
      const pv = A(c, "_pv");
      const vv = A(c, "_vv");
      const out = A(c, "vwap");
      for (let i = from; i < b.length; i++) {
        const x = b[i]!;
        const tp = srcOf(x, c.p.source);
        const reset = i === 0 || day(x.time) !== day(b[i - 1]!.time);
        pv[i] = (reset ? 0 : pv[i - 1]!) + tp * x.volume;
        vv[i] = (reset ? 0 : vv[i - 1]!) + x.volume;
        out[i] = vv[i]! > 0 ? pv[i]! / vv[i]! : tp;
      }
    },
  },
  {
    type: "ichimoku",
    name: "Ichimoku Kinko Hyo",
    short: "Ichimoku",
    category: "Trend",
    pane: "overlay",
    description: "Tenkan, Kijun, a cloud of Senkou spans projected forward and the lagging Chikou span.",
    params: [P.int("tenkan", "Tenkan-sen", 9), P.int("kijun", "Kijun-sen", 26), P.int("senkou", "Senkou Span B", 52), P.int("disp", "Displacement", 26, 1, 200)],
    outputs: [
      { key: "tenkan", label: "Tenkan", kind: "line", color: "down", fmt: "price" },
      { key: "kijun", label: "Kijun", kind: "line", color: "info", fmt: "price" },
      { key: "spanA", label: "Span A", kind: "line", color: "up", shift: (p) => int(p.disp), fmt: "price" },
      { key: "spanB", label: "Span B", kind: "line", color: "down", shift: (p) => int(p.disp), fmt: "price" },
      { key: "chikou", label: "Chikou", kind: "line", color: "fg3", shift: (p) => -int(p.disp), fmt: "price" },
    ],
    fills: [{ upper: "spanA", lower: "spanB", color: "up", downColor: "down", alpha: 0.12 }],
    calc(c, from) {
      const { b, p } = c;
      const t = A(c, "tenkan");
      const k = A(c, "kijun");
      const sa = A(c, "spanA");
      const sb = A(c, "spanB");
      const ch = A(c, "chikou");
      for (let i = from; i < b.length; i++) {
        t[i] = (hhAt(b, i, int(p.tenkan)) + llAt(b, i, int(p.tenkan))) / 2;
        k[i] = (hhAt(b, i, int(p.kijun)) + llAt(b, i, int(p.kijun))) / 2;
        sa[i] = (t[i]! + k[i]!) / 2;
        sb[i] = (hhAt(b, i, int(p.senkou)) + llAt(b, i, int(p.senkou))) / 2;
        ch[i] = b[i]!.close;
      }
    },
  },
  {
    type: "psar",
    name: "Parabolic SAR",
    short: "SAR",
    category: "Trend",
    pane: "overlay",
    description: "Wilder's stop-and-reverse: trailing dots that flip when price crosses them.",
    params: [P.float("step", "Step", 0.02, 0.001, 1, 0.01), P.float("max", "Maximum", 0.2, 0.01, 1, 0.01)],
    outputs: [{ key: "sar", label: "SAR", kind: "dots", color: "gold", fmt: "price" }],
    calc(c, from) {
      const { b } = c;
      const st = num(c.p.step);
      const mx = num(c.p.max);
      const sar = A(c, "sar");
      const dir = A(c, "_dir");
      const ep = A(c, "_ep");
      const af = A(c, "_af");
      for (let i = from; i < b.length; i++) {
        const x = b[i]!;
        if (i === 0) {
          sar[0] = dir[0] = ep[0] = af[0] = NaN;
          continue;
        }
        if (i === 1) {
          const up = x.close >= b[0]!.close;
          dir[1] = up ? 1 : -1;
          sar[1] = up ? Math.min(b[0]!.low, x.low) : Math.max(b[0]!.high, x.high);
          ep[1] = up ? Math.max(b[0]!.high, x.high) : Math.min(b[0]!.low, x.low);
          af[1] = st;
          continue;
        }
        const pd = dir[i - 1]!;
        const pe = ep[i - 1]!;
        const pa = af[i - 1]!;
        let s = sar[i - 1]! + pa * (pe - sar[i - 1]!);
        if (pd === 1) {
          s = Math.min(s, b[i - 1]!.low, b[i - 2]!.low);
          if (x.low < s) {
            dir[i] = -1;
            s = Math.max(pe, x.high);
            ep[i] = x.low;
            af[i] = st;
          } else {
            dir[i] = 1;
            ep[i] = x.high > pe ? x.high : pe;
            af[i] = x.high > pe ? Math.min(pa + st, mx) : pa;
          }
        } else {
          s = Math.max(s, b[i - 1]!.high, b[i - 2]!.high);
          if (x.high > s) {
            dir[i] = 1;
            s = Math.min(pe, x.low);
            ep[i] = x.high;
            af[i] = st;
          } else {
            dir[i] = -1;
            ep[i] = x.low < pe ? x.low : pe;
            af[i] = x.low < pe ? Math.min(pa + st, mx) : pa;
          }
        }
        sar[i] = s;
      }
    },
  },
  {
    type: "supertrend",
    name: "SuperTrend",
    short: "SuperTrend",
    category: "Trend",
    pane: "overlay",
    description: "ATR bands around the median price that ratchet with the trend and flip on a close through them.",
    params: [P.int("period", "ATR period", 10), P.float("mult", "Multiplier", 3, 0.1, 20, 0.1)],
    outputs: [
      { key: "up", label: "Up", kind: "line", color: "up", width: 2, fmt: "price" },
      { key: "dn", label: "Down", kind: "line", color: "down", width: 2, fmt: "price" },
    ],
    calc(c, from) {
      const { b } = c;
      const n = int(c.p.period);
      const m = num(c.p.mult);
      const tr = A(c, "_tr");
      const atr = A(c, "_atr");
      const fl = A(c, "_fl");
      const fu = A(c, "_fu");
      const dir = A(c, "_dir");
      const up = A(c, "up");
      const dn = A(c, "dn");
      for (let i = from; i < b.length; i++) {
        const x = b[i]!;
        tr[i] = trueRange(b, i);
        atr[i] = rmaAt(tr, atr, i, n);
        const hl2 = (x.high + x.low) / 2;
        const bl = hl2 - m * atr[i]!;
        const bu = hl2 + m * atr[i]!;
        const pl = i > 0 ? fl[i - 1]! : NaN;
        const pu = i > 0 ? fu[i - 1]! : NaN;
        const pc = i > 0 ? b[i - 1]!.close : NaN;
        fl[i] = Number.isNaN(pl) || bl > pl || pc < pl ? bl : pl;
        fu[i] = Number.isNaN(pu) || bu < pu || pc > pu ? bu : pu;
        const pd = i > 0 ? dir[i - 1]! : NaN;
        if (Number.isNaN(atr[i]!)) dir[i] = NaN;
        else if (Number.isNaN(pd)) dir[i] = 1;
        else if (pd === -1 && x.close > pu) dir[i] = 1;
        else if (pd === 1 && x.close < pl) dir[i] = -1;
        else dir[i] = pd;
        up[i] = dir[i] === 1 ? fl[i]! : NaN;
        dn[i] = dir[i] === -1 ? fu[i]! : NaN;
      }
    },
  },
  {
    type: "envelopes",
    name: "Envelopes",
    short: "Env",
    category: "Trend",
    pane: "overlay",
    description: "A moving average with bands a fixed percentage above and below.",
    params: [P.period(14), P.float("dev", "Deviation %", 0.1, 0.01, 50, 0.05), P.method("sma"), P.source()],
    outputs: [
      { key: "up", label: "Upper", kind: "line", color: "info", fmt: "price" },
      { key: "lo", label: "Lower", kind: "line", color: "info", fmt: "price" },
    ],
    calc(c, from) {
      const n = int(c.p.period);
      const d = num(c.p.dev) / 100;
      const s = fillSrc(c, "_s", c.p.source, from);
      const ma = A(c, "_ma");
      const up = A(c, "up");
      const lo = A(c, "lo");
      for (let i = from; i < s.length; i++) {
        ma[i] = maAt(c.p.method, s, ma, i, n);
        up[i] = ma[i]! * (1 + d);
        lo[i] = ma[i]! * (1 - d);
      }
    },
  },
  {
    type: "pivots",
    name: "Pivot Points (Classic, Daily)",
    short: "Pivots",
    category: "Trend",
    pane: "overlay",
    description: "Floor-trader pivots from the previous server day's high, low and close.",
    params: [],
    outputs: [
      { key: "r3", label: "R3", kind: "line", color: "down", stepped: true, dashed: true, fmt: "price", autoscale: false },
      { key: "r2", label: "R2", kind: "line", color: "down", stepped: true, dashed: true, fmt: "price", autoscale: false },
      { key: "r1", label: "R1", kind: "line", color: "down", stepped: true, fmt: "price", autoscale: false },
      { key: "p", label: "P", kind: "line", color: "gold", stepped: true, width: 2, fmt: "price", autoscale: false },
      { key: "s1", label: "S1", kind: "line", color: "up", stepped: true, fmt: "price", autoscale: false },
      { key: "s2", label: "S2", kind: "line", color: "up", stepped: true, dashed: true, fmt: "price", autoscale: false },
      { key: "s3", label: "S3", kind: "line", color: "up", stepped: true, dashed: true, fmt: "price", autoscale: false },
    ],
    calc(c, from) {
      const { b } = c;
      const dh = A(c, "_dh");
      const dl = A(c, "_dl");
      const ph = A(c, "_ph");
      const pl = A(c, "_pl");
      const pc = A(c, "_pc");
      const o = { p: A(c, "p"), r1: A(c, "r1"), r2: A(c, "r2"), r3: A(c, "r3"), s1: A(c, "s1"), s2: A(c, "s2"), s3: A(c, "s3") };
      for (let i = from; i < b.length; i++) {
        const x = b[i]!;
        const newDay = i === 0 || day(x.time) !== day(b[i - 1]!.time);
        if (newDay) {
          dh[i] = x.high;
          dl[i] = x.low;
          ph[i] = i === 0 ? NaN : dh[i - 1]!;
          pl[i] = i === 0 ? NaN : dl[i - 1]!;
          pc[i] = i === 0 ? NaN : b[i - 1]!.close;
        } else {
          dh[i] = Math.max(dh[i - 1]!, x.high);
          dl[i] = Math.min(dl[i - 1]!, x.low);
          ph[i] = ph[i - 1]!;
          pl[i] = pl[i - 1]!;
          pc[i] = pc[i - 1]!;
        }
        const H = ph[i]!;
        const L = pl[i]!;
        const pp = (H + L + pc[i]!) / 3;
        o.p[i] = pp;
        o.r1[i] = 2 * pp - L;
        o.s1[i] = 2 * pp - H;
        o.r2[i] = pp + (H - L);
        o.s2[i] = pp - (H - L);
        o.r3[i] = H + 2 * (pp - L);
        o.s3[i] = L - 2 * (H - pp);
      }
    },
  },

  /* ---------------- Volatility ---------------- */
  {
    type: "bb",
    name: "Bollinger Bands",
    short: "BB",
    category: "Volatility",
    pane: "overlay",
    description: "SMA with bands k population standard deviations away.",
    params: [P.period(20), P.float("mult", "Deviations", 2, 0.1, 10, 0.1), P.source()],
    outputs: [
      { key: "up", label: "Upper", kind: "line", color: "fg2", fmt: "price" },
      { key: "mid", label: "Basis", kind: "line", color: "fg2", dashed: true, fmt: "price" },
      { key: "lo", label: "Lower", kind: "line", color: "fg2", fmt: "price" },
    ],
    fills: [{ upper: "up", lower: "lo", color: "fg2", alpha: 0.05 }],
    calc(c, from) {
      const n = int(c.p.period);
      const k = num(c.p.mult);
      const s = fillSrc(c, "_s", c.p.source, from);
      const mid = A(c, "mid");
      const up = A(c, "up");
      const lo = A(c, "lo");
      for (let i = from; i < s.length; i++) {
        mid[i] = smaAt(s, i, n);
        const sd = stdevAt(s, i, n, mid[i]!);
        up[i] = mid[i]! + k * sd;
        lo[i] = mid[i]! - k * sd;
      }
    },
  },
  {
    type: "keltner",
    name: "Keltner Channels",
    short: "KC",
    category: "Volatility",
    pane: "overlay",
    description: "EMA basis with bands a multiple of ATR away.",
    params: [P.period(20, "EMA period"), P.int("atr", "ATR period", 10), P.float("mult", "Multiplier", 2, 0.1, 10, 0.1)],
    outputs: [
      { key: "up", label: "Upper", kind: "line", color: "info", fmt: "price" },
      { key: "mid", label: "Basis", kind: "line", color: "info", dashed: true, fmt: "price" },
      { key: "lo", label: "Lower", kind: "line", color: "info", fmt: "price" },
    ],
    fills: [{ upper: "up", lower: "lo", color: "info", alpha: 0.05 }],
    calc(c, from) {
      const { b } = c;
      const s = fillSrc(c, "_s", "close", from);
      const tr = A(c, "_tr");
      const atr = A(c, "_atr");
      const mid = A(c, "mid");
      const up = A(c, "up");
      const lo = A(c, "lo");
      const m = num(c.p.mult);
      for (let i = from; i < b.length; i++) {
        tr[i] = trueRange(b, i);
        atr[i] = rmaAt(tr, atr, i, int(c.p.atr));
        mid[i] = emaAt(s, mid, i, int(c.p.period));
        up[i] = mid[i]! + m * atr[i]!;
        lo[i] = mid[i]! - m * atr[i]!;
      }
    },
  },
  {
    type: "donchian",
    name: "Donchian Channels",
    short: "DC",
    category: "Volatility",
    pane: "overlay",
    description: "Highest high and lowest low of the last N bars, and their midpoint.",
    params: [P.period(20)],
    outputs: [
      { key: "up", label: "Upper", kind: "line", color: "ember", fmt: "price" },
      { key: "mid", label: "Basis", kind: "line", color: "ember", dashed: true, fmt: "price" },
      { key: "lo", label: "Lower", kind: "line", color: "ember", fmt: "price" },
    ],
    fills: [{ upper: "up", lower: "lo", color: "ember", alpha: 0.04 }],
    calc(c, from) {
      const { b } = c;
      const n = int(c.p.period);
      const up = A(c, "up");
      const lo = A(c, "lo");
      const mid = A(c, "mid");
      for (let i = from; i < b.length; i++) {
        up[i] = hhAt(b, i, n);
        lo[i] = llAt(b, i, n);
        mid[i] = (up[i]! + lo[i]!) / 2;
      }
    },
  },
  {
    type: "atr",
    name: "Average True Range",
    short: "ATR",
    category: "Volatility",
    pane: "separate",
    description: "Average of the true range; Wilder smoothing by default (MT5 uses Simple).",
    params: [P.period(14), { key: "smoothing", label: "Smoothing", kind: "select", def: "rma", options: [{ value: "rma", label: "Wilder (RMA)" }, { value: "sma", label: "Simple" }, { value: "ema", label: "Exponential" }] }],
    outputs: [{ key: "atr", label: "ATR", kind: "line", color: "info", fmt: "price+" }],
    calc(c, from) {
      const { b } = c;
      const tr = A(c, "_tr");
      const out = A(c, "atr");
      for (let i = from; i < b.length; i++) {
        tr[i] = trueRange(b, i);
        out[i] = maAt(c.p.smoothing, tr, out, i, int(c.p.period));
      }
    },
  },
  {
    type: "stddev",
    name: "Standard Deviation",
    short: "StdDev",
    category: "Volatility",
    pane: "separate",
    description: "Population standard deviation of price around its SMA.",
    params: [P.period(20), P.source()],
    outputs: [{ key: "sd", label: "StdDev", kind: "line", color: "gold", fmt: "price+" }],
    calc(c, from) {
      const n = int(c.p.period);
      const s = fillSrc(c, "_s", c.p.source, from);
      const out = A(c, "sd");
      for (let i = from; i < s.length; i++) out[i] = stdevAt(s, i, n);
    },
  },

  /* ---------------- Oscillators ---------------- */
  {
    type: "rsi",
    name: "Relative Strength Index",
    short: "RSI",
    category: "Oscillators",
    pane: "separate",
    description: "Wilder's RSI: ratio of smoothed gains to losses, 0–100.",
    params: [P.period(14), P.source()],
    outputs: [{ key: "rsi", label: "RSI", kind: "line", color: "gold", fmt: 2 }],
    levels: () => [70, 30],
    range: [0, 100],
    calc(c, from) {
      const n = int(c.p.period);
      const s = fillSrc(c, "_s", c.p.source, from);
      const g = A(c, "_g");
      const l = A(c, "_l");
      const ag = A(c, "_ag");
      const al = A(c, "_al");
      const out = A(c, "rsi");
      for (let i = from; i < s.length; i++) {
        const d = i === 0 ? NaN : s[i]! - s[i - 1]!;
        g[i] = d > 0 ? d : Number.isNaN(d) ? NaN : 0;
        l[i] = d < 0 ? -d : Number.isNaN(d) ? NaN : 0;
        ag[i] = rmaAt(g, ag, i, n);
        al[i] = rmaAt(l, al, i, n);
        out[i] = Number.isNaN(ag[i]!) ? NaN : al[i] === 0 ? (ag[i] === 0 ? 50 : 100) : 100 - 100 / (1 + ag[i]! / al[i]!);
      }
    },
  },
  {
    type: "stoch",
    name: "Stochastic Oscillator",
    short: "Stoch",
    category: "Oscillators",
    pane: "separate",
    description: "Close relative to the N-bar range; %K with slowing, %D its moving average.",
    params: [P.int("k", "%K period", 14), P.int("d", "%D period", 3), P.int("slow", "Slowing", 3)],
    outputs: [
      { key: "k", label: "%K", kind: "line", color: "info", fmt: 2 },
      { key: "d", label: "%D", kind: "line", color: "ember", dashed: true, fmt: 2 },
    ],
    levels: () => [80, 20],
    range: [0, 100],
    calc(c, from) {
      const { b } = c;
      const nk = int(c.p.k);
      const sl = int(c.p.slow);
      const nu = A(c, "_num");
      const de = A(c, "_den");
      const k = A(c, "k");
      const d = A(c, "d");
      for (let i = from; i < b.length; i++) {
        const ll = llAt(b, i, nk);
        nu[i] = b[i]!.close - ll;
        de[i] = hhAt(b, i, nk) - ll;
        const sn = smaAt(nu, i, sl);
        const sd = smaAt(de, i, sl);
        k[i] = Number.isNaN(sn) ? NaN : sd === 0 ? (i > 0 && !Number.isNaN(k[i - 1]!) ? k[i - 1]! : 50) : (100 * sn) / sd;
        d[i] = smaAt(k, i, int(c.p.d));
      }
    },
  },
  {
    type: "stochrsi",
    name: "Stochastic RSI",
    short: "StochRSI",
    category: "Oscillators",
    pane: "separate",
    description: "Stochastic formula applied to RSI values, smoothed %K and %D.",
    params: [P.int("rsi", "RSI period", 14), P.int("stoch", "Stochastic period", 14), P.int("k", "%K smoothing", 3), P.int("d", "%D smoothing", 3), P.source()],
    outputs: [
      { key: "k", label: "%K", kind: "line", color: "info", fmt: 2 },
      { key: "d", label: "%D", kind: "line", color: "ember", dashed: true, fmt: 2 },
    ],
    levels: () => [80, 20],
    range: [0, 100],
    calc(c, from) {
      const n = int(c.p.rsi);
      const ns = int(c.p.stoch);
      const s = fillSrc(c, "_s", c.p.source, from);
      const g = A(c, "_g");
      const l = A(c, "_l");
      const ag = A(c, "_ag");
      const al = A(c, "_al");
      const r = A(c, "_rsi");
      const st = A(c, "_st");
      const k = A(c, "k");
      const d = A(c, "d");
      for (let i = from; i < s.length; i++) {
        const dd = i === 0 ? NaN : s[i]! - s[i - 1]!;
        g[i] = Number.isNaN(dd) ? NaN : Math.max(dd, 0);
        l[i] = Number.isNaN(dd) ? NaN : Math.max(-dd, 0);
        ag[i] = rmaAt(g, ag, i, n);
        al[i] = rmaAt(l, al, i, n);
        r[i] = Number.isNaN(ag[i]!) ? NaN : al[i] === 0 ? (ag[i] === 0 ? 50 : 100) : 100 - 100 / (1 + ag[i]! / al[i]!);
        const hi = maxAt(r, i, ns);
        const lo = minAt(r, i, ns);
        st[i] = Number.isNaN(hi) ? NaN : hi === lo ? 50 : (100 * (r[i]! - lo)) / (hi - lo);
        k[i] = smaAt(st, i, int(c.p.k));
        d[i] = smaAt(k, i, int(c.p.d));
      }
    },
  },
  {
    type: "macd",
    name: "MACD",
    short: "MACD",
    category: "Oscillators",
    pane: "separate",
    description: "Fast EMA − slow EMA, its EMA signal line and the difference histogram.",
    params: [P.int("fast", "Fast EMA", 12), P.int("slow", "Slow EMA", 26), P.int("signal", "Signal", 9), P.source()],
    outputs: [
      { key: "hist", label: "Histogram", kind: "hist", color: "fg3", histColor: "sign", fmt: "price+" },
      { key: "macd", label: "MACD", kind: "line", color: "ember", fmt: "price+" },
      { key: "signal", label: "Signal", kind: "line", color: "gold", fmt: "price+" },
    ],
    levels: () => [0],
    calc(c, from) {
      const s = fillSrc(c, "_s", c.p.source, from);
      const f = A(c, "_f");
      const sl = A(c, "_sl");
      const m = A(c, "macd");
      const sg = A(c, "signal");
      const h = A(c, "hist");
      for (let i = from; i < s.length; i++) {
        f[i] = emaAt(s, f, i, int(c.p.fast));
        sl[i] = emaAt(s, sl, i, int(c.p.slow));
        m[i] = f[i]! - sl[i]!;
        sg[i] = emaAt(m, sg, i, int(c.p.signal));
        h[i] = m[i]! - sg[i]!;
      }
    },
  },
  {
    type: "cci",
    name: "Commodity Channel Index",
    short: "CCI",
    category: "Oscillators",
    pane: "separate",
    description: "(Typical price − SMA) / (0.015 · mean absolute deviation).",
    params: [P.period(20), P.source("hlc3")],
    outputs: [{ key: "cci", label: "CCI", kind: "line", color: "info", fmt: 2 }],
    levels: () => [100, -100],
    calc(c, from) {
      const n = int(c.p.period);
      const s = fillSrc(c, "_s", c.p.source, from);
      const out = A(c, "cci");
      for (let i = from; i < s.length; i++) {
        const ma = smaAt(s, i, n);
        if (Number.isNaN(ma)) {
          out[i] = NaN;
          continue;
        }
        let md = 0;
        for (let j = i - n + 1; j <= i; j++) md += Math.abs(s[j]! - ma);
        md /= n;
        out[i] = md === 0 ? 0 : (s[i]! - ma) / (0.015 * md);
      }
    },
  },
  {
    type: "willr",
    name: "Williams' Percent Range",
    short: "%R",
    category: "Oscillators",
    pane: "separate",
    description: "−100 · (highest high − close) / (highest high − lowest low).",
    params: [P.period(14)],
    outputs: [{ key: "wr", label: "%R", kind: "line", color: "info", fmt: 2 }],
    levels: () => [-20, -80],
    range: [-100, 0],
    calc(c, from) {
      const { b } = c;
      const n = int(c.p.period);
      const out = A(c, "wr");
      for (let i = from; i < b.length; i++) {
        const hh = hhAt(b, i, n);
        const ll = llAt(b, i, n);
        out[i] = Number.isNaN(hh) ? NaN : hh === ll ? -50 : (-100 * (hh - b[i]!.close)) / (hh - ll);
      }
    },
  },
  {
    type: "momentum",
    name: "Momentum",
    short: "Momentum",
    category: "Oscillators",
    pane: "separate",
    description: "Price relative to N bars ago, ×100 (MT5 definition, centred on 100).",
    params: [P.period(14), P.source()],
    outputs: [{ key: "mom", label: "Momentum", kind: "line", color: "info", fmt: 2 }],
    levels: () => [100],
    calc(c, from) {
      const n = int(c.p.period);
      const s = fillSrc(c, "_s", c.p.source, from);
      const out = A(c, "mom");
      for (let i = from; i < s.length; i++) out[i] = i < n ? NaN : (s[i]! / s[i - n]!) * 100;
    },
  },
  {
    type: "roc",
    name: "Rate of Change",
    short: "ROC",
    category: "Oscillators",
    pane: "separate",
    description: "Percentage change over N bars.",
    params: [P.period(14), P.source()],
    outputs: [{ key: "roc", label: "ROC", kind: "line", color: "gold", fmt: 2 }],
    levels: () => [0],
    calc(c, from) {
      const n = int(c.p.period);
      const s = fillSrc(c, "_s", c.p.source, from);
      const out = A(c, "roc");
      for (let i = from; i < s.length; i++) out[i] = i < n ? NaN : ((s[i]! - s[i - n]!) / s[i - n]!) * 100;
    },
  },
  {
    type: "adx",
    name: "Average Directional Index",
    short: "ADX",
    category: "Oscillators",
    pane: "separate",
    description: "Wilder's ADX with +DI / −DI directional lines.",
    params: [P.period(14, "DI period"), P.int("smooth", "ADX smoothing", 14)],
    outputs: [
      { key: "adx", label: "ADX", kind: "line", color: "gold", width: 2, fmt: 2 },
      { key: "pdi", label: "+DI", kind: "line", color: "up", fmt: 2 },
      { key: "mdi", label: "−DI", kind: "line", color: "down", fmt: 2 },
    ],
    levels: () => [25],
    range: [0, 100],
    calc(c, from) {
      const { b } = c;
      const n = int(c.p.period);
      const tr = A(c, "_tr");
      const pdm = A(c, "_pdm");
      const mdm = A(c, "_mdm");
      const str = A(c, "_str");
      const sp = A(c, "_sp");
      const sm = A(c, "_sm");
      const dx = A(c, "_dx");
      const pdi = A(c, "pdi");
      const mdi = A(c, "mdi");
      const adx = A(c, "adx");
      for (let i = from; i < b.length; i++) {
        if (i === 0) {
          tr[0] = pdm[0] = mdm[0] = str[0] = sp[0] = sm[0] = dx[0] = pdi[0] = mdi[0] = adx[0] = NaN;
          continue;
        }
        const up = b[i]!.high - b[i - 1]!.high;
        const dn = b[i - 1]!.low - b[i]!.low;
        pdm[i] = up > dn && up > 0 ? up : 0;
        mdm[i] = dn > up && dn > 0 ? dn : 0;
        tr[i] = trueRange(b, i);
        str[i] = rmaAt(tr, str, i, n);
        sp[i] = rmaAt(pdm, sp, i, n);
        sm[i] = rmaAt(mdm, sm, i, n);
        pdi[i] = str[i] ? (100 * sp[i]!) / str[i]! : NaN;
        mdi[i] = str[i] ? (100 * sm[i]!) / str[i]! : NaN;
        const sum = pdi[i]! + mdi[i]!;
        dx[i] = Number.isNaN(sum) ? NaN : sum === 0 ? 0 : (100 * Math.abs(pdi[i]! - mdi[i]!)) / sum;
        adx[i] = rmaAt(dx, adx, i, int(c.p.smooth));
      }
    },
  },
  {
    type: "aroon",
    name: "Aroon",
    short: "Aroon",
    category: "Oscillators",
    pane: "separate",
    description: "Bars since the N-bar high / low, scaled 0–100.",
    params: [P.period(14)],
    outputs: [
      { key: "up", label: "Up", kind: "line", color: "up", fmt: 2 },
      { key: "dn", label: "Down", kind: "line", color: "down", fmt: 2 },
    ],
    levels: () => [70, 30],
    range: [0, 100],
    calc(c, from) {
      const { b } = c;
      const n = int(c.p.period);
      const up = A(c, "up");
      const dn = A(c, "dn");
      for (let i = from; i < b.length; i++) {
        if (i < n) {
          up[i] = dn[i] = NaN;
          continue;
        }
        let hi = i;
        let lo = i;
        for (let j = i - n; j <= i; j++) {
          if (b[j]!.high >= b[hi]!.high) hi = j;
          if (b[j]!.low <= b[lo]!.low) lo = j;
        }
        up[i] = (100 * (n - (i - hi))) / n;
        dn[i] = (100 * (n - (i - lo))) / n;
      }
    },
  },

  /* ---------------- Bill Williams ---------------- */
  {
    type: "ao",
    name: "Awesome Oscillator",
    short: "AO",
    category: "Bill Williams",
    pane: "separate",
    description: "SMA(5) − SMA(34) of the median price; bars green when rising.",
    params: [P.int("fast", "Fast", 5), P.int("slow", "Slow", 34)],
    outputs: [{ key: "ao", label: "AO", kind: "hist", color: "fg3", histColor: "trend", fmt: "price+" }],
    calc(c, from) {
      const s = fillSrc(c, "_s", "hl2", from);
      const out = A(c, "ao");
      for (let i = from; i < s.length; i++) out[i] = smaAt(s, i, int(c.p.fast)) - smaAt(s, i, int(c.p.slow));
    },
  },
  {
    type: "ac",
    name: "Accelerator Oscillator",
    short: "AC",
    category: "Bill Williams",
    pane: "separate",
    description: "AO − SMA(5) of AO; bars green when rising.",
    params: [P.int("fast", "Fast", 5), P.int("slow", "Slow", 34), P.int("signal", "Signal", 5)],
    outputs: [{ key: "ac", label: "AC", kind: "hist", color: "fg3", histColor: "trend", fmt: "price+" }],
    calc(c, from) {
      const s = fillSrc(c, "_s", "hl2", from);
      const ao = A(c, "_ao");
      const out = A(c, "ac");
      for (let i = from; i < s.length; i++) {
        ao[i] = smaAt(s, i, int(c.p.fast)) - smaAt(s, i, int(c.p.slow));
        out[i] = ao[i]! - smaAt(ao, i, int(c.p.signal));
      }
    },
  },
  {
    type: "alligator",
    name: "Alligator",
    short: "Alligator",
    category: "Bill Williams",
    pane: "overlay",
    description: "Three smoothed MAs of the median price, shifted forward: jaw, teeth and lips.",
    params: [P.int("jaw", "Jaw period", 13), P.int("jawShift", "Jaw shift", 8, 0, 100), P.int("teeth", "Teeth period", 8), P.int("teethShift", "Teeth shift", 5, 0, 100), P.int("lips", "Lips period", 5), P.int("lipsShift", "Lips shift", 3, 0, 100)],
    outputs: [
      { key: "jaw", label: "Jaw", kind: "line", color: "info", shift: (p) => int(p.jawShift), fmt: "price" },
      { key: "teeth", label: "Teeth", kind: "line", color: "down", shift: (p) => int(p.teethShift), fmt: "price" },
      { key: "lips", label: "Lips", kind: "line", color: "up", shift: (p) => int(p.lipsShift), fmt: "price" },
    ],
    calc(c, from) {
      const s = fillSrc(c, "_s", "hl2", from);
      const j = A(c, "jaw");
      const t = A(c, "teeth");
      const l = A(c, "lips");
      for (let i = from; i < s.length; i++) {
        j[i] = rmaAt(s, j, i, int(c.p.jaw));
        t[i] = rmaAt(s, t, i, int(c.p.teeth));
        l[i] = rmaAt(s, l, i, int(c.p.lips));
      }
    },
  },
  {
    type: "fractals",
    name: "Fractals",
    short: "Fractals",
    category: "Bill Williams",
    pane: "overlay",
    description: "A high (low) with two lower highs (higher lows) on each side. Confirmed two bars later.",
    params: [],
    outputs: [
      { key: "up", label: "Up", kind: "markers", marker: "up", color: "up", fmt: "price" },
      { key: "dn", label: "Down", kind: "markers", marker: "down", color: "down", fmt: "price" },
    ],
    lookahead: 2,
    calc(c, from) {
      const { b } = c;
      const up = A(c, "up");
      const dn = A(c, "dn");
      for (let i = from; i < b.length; i++) {
        if (i < 2 || i > b.length - 3) {
          up[i] = dn[i] = NaN;
          continue;
        }
        const h = b[i]!.high;
        const l = b[i]!.low;
        up[i] = h > b[i - 1]!.high && h > b[i - 2]!.high && h > b[i + 1]!.high && h > b[i + 2]!.high ? h : NaN;
        dn[i] = l < b[i - 1]!.low && l < b[i - 2]!.low && l < b[i + 1]!.low && l < b[i + 2]!.low ? l : NaN;
      }
    },
  },

  /* ---------------- Volume ---------------- */
  {
    type: "volumes",
    name: "Volumes",
    short: "Volumes",
    category: "Volume",
    pane: "separate",
    description: "Tick volume, green when higher than the previous bar.",
    params: [],
    outputs: [{ key: "v", label: "Volume", kind: "hist", color: "fg3", histColor: "trend", fmt: "vol" }],
    calc(c, from) {
      const out = A(c, "v");
      for (let i = from; i < c.b.length; i++) out[i] = c.b[i]!.volume;
    },
  },
  {
    type: "obv",
    name: "On Balance Volume",
    short: "OBV",
    category: "Volume",
    pane: "separate",
    description: "Cumulative volume, added on up closes and subtracted on down closes.",
    params: [],
    outputs: [{ key: "obv", label: "OBV", kind: "line", color: "info", fmt: "vol" }],
    calc(c, from) {
      const { b } = c;
      const out = A(c, "obv");
      for (let i = from; i < b.length; i++) {
        if (i === 0) {
          out[0] = 0;
          continue;
        }
        const d = b[i]!.close - b[i - 1]!.close;
        out[i] = out[i - 1]! + (d > 0 ? b[i]!.volume : d < 0 ? -b[i]!.volume : 0);
      }
    },
  },
  {
    type: "mfi",
    name: "Money Flow Index",
    short: "MFI",
    category: "Volume",
    pane: "separate",
    description: "Volume-weighted RSI of the typical price, 0–100.",
    params: [P.period(14)],
    outputs: [{ key: "mfi", label: "MFI", kind: "line", color: "gold", fmt: 2 }],
    levels: () => [80, 20],
    range: [0, 100],
    calc(c, from) {
      const { b } = c;
      const n = int(c.p.period);
      const tp = fillSrc(c, "_tp", "hlc3", from);
      const pos = A(c, "_pos");
      const neg = A(c, "_neg");
      const out = A(c, "mfi");
      for (let i = from; i < b.length; i++) {
        const mf = tp[i]! * b[i]!.volume;
        pos[i] = i === 0 ? NaN : tp[i]! > tp[i - 1]! ? mf : 0;
        neg[i] = i === 0 ? NaN : tp[i]! < tp[i - 1]! ? mf : 0;
        const sp = smaAt(pos, i, n);
        const sn = smaAt(neg, i, n);
        out[i] = Number.isNaN(sp) ? NaN : sn === 0 ? (sp === 0 ? 50 : 100) : 100 - 100 / (1 + sp / sn);
      }
    },
  },
];

export const INDICATOR_TYPES = [
  "sma", "ema", "wma", "smma", "dema", "tema", "hma", "vwap", "ichimoku", "psar", "supertrend", "envelopes", "pivots",
  "bb", "keltner", "donchian", "atr", "stddev",
  "rsi", "stoch", "stochrsi", "macd", "cci", "willr", "momentum", "roc", "adx", "aroon",
  "ao", "ac", "alligator", "fractals",
  "volumes", "obv", "mfi",
] as const;
export type IndicatorType = (typeof INDICATOR_TYPES)[number];

export const INDICATOR_DEFS = Object.fromEntries(DEFS_LIST.map((d) => [d.type, d])) as Record<IndicatorType, IndicatorDef>;
export const INDICATOR_LIST: IndicatorDef[] = INDICATOR_TYPES.map((t) => INDICATOR_DEFS[t]);

export const isIndicatorType = (t: unknown): t is IndicatorType => typeof t === "string" && t in INDICATOR_DEFS;
export const getIndicatorDef = (t: IndicatorType): IndicatorDef => INDICATOR_DEFS[t];

/* ------------------------------------------------------------------ */
/* Params                                                              */
/* ------------------------------------------------------------------ */

export function defaultParams(type: IndicatorType): Params {
  const out: Params = {};
  for (const p of INDICATOR_DEFS[type].params) out[p.key] = p.def;
  return out;
}

/** Validate one value; returns an error message or null. */
export function validateParam(p: ParamDef, v: unknown): string | null {
  if (p.kind === "int" || p.kind === "float") {
    const n = typeof v === "number" ? v : typeof v === "string" && v.trim() !== "" ? Number(v) : NaN;
    if (!Number.isFinite(n)) return `${p.label} must be a number`;
    if (p.kind === "int" && !Number.isInteger(n)) return `${p.label} must be a whole number`;
    if (n < p.min || n > p.max) return `${p.label} must be between ${p.min} and ${p.max}`;
    return null;
  }
  if (p.kind === "source") return (SOURCES as readonly string[]).includes(String(v)) ? null : `Unknown source`;
  return p.options.some((o) => o.value === v) ? null : `Unknown ${p.label.toLowerCase()}`;
}

/** Clamp/repair params to the definition (unknown keys dropped, invalid values → defaults). */
export function normalizeParams(type: IndicatorType, raw?: Partial<Record<string, unknown>> | null): Params {
  const out: Params = {};
  for (const p of INDICATOR_DEFS[type].params) {
    const v = raw?.[p.key];
    if (p.kind === "int" || p.kind === "float") {
      const n = typeof v === "number" ? v : typeof v === "string" ? Number(v) : NaN;
      if (!Number.isFinite(n)) out[p.key] = p.def;
      else {
        const c = Math.min(p.max, Math.max(p.min, n));
        out[p.key] = p.kind === "int" ? Math.round(c) : c;
      }
    } else out[p.key] = validateParam(p, v) === null ? (v as string) : p.def;
  }
  return out;
}

/** "SMA 20", "BB 20, 2", "RSI 14 hlc3" … */
export function indicatorLabel(type: IndicatorType, params: Params): string {
  const def = INDICATOR_DEFS[type];
  const parts: string[] = [];
  let src = "";
  for (const p of def.params) {
    const v = params[p.key] ?? p.def;
    if (p.kind === "source") {
      if (v !== p.def) src = String(v);
    } else if (p.kind === "select") {
      if (v !== p.def) parts.push(String(v).toUpperCase());
    } else parts.push(String(v));
  }
  return [def.short, parts.join(", "), src].filter(Boolean).join(" ");
}

/* ------------------------------------------------------------------ */
/* Computation                                                         */
/* ------------------------------------------------------------------ */

/**
 * Incremental evaluator. `run(bars)` recomputes only what changed since the last call:
 *  - same history, forming bar updated → just the last bar (plus lookahead)
 *  - new bar(s) appended → from the previous last bar
 *  - history replaced / prepended (scroll-back) / shrunk → full pass
 */
export class IndicatorComputation {
  readonly type: IndicatorType;
  readonly params: Params;
  readonly def: IndicatorDef;
  outs: Record<string, number[]> = {};
  private len = 0;
  private first = NaN;
  private last = NaN;

  constructor(type: IndicatorType, params: Params) {
    this.type = type;
    this.params = params;
    this.def = INDICATOR_DEFS[type];
  }

  run(bars: readonly Bar[]): { from: number; full: boolean } {
    const n = bars.length;
    const full = this.len === 0 || n < this.len || bars[0]?.time !== this.first || (n === this.len && bars[n - 1]?.time !== this.last && n > 0);
    if (full) this.outs = {};
    const from = full ? 0 : Math.max(0, this.len - 1 - (this.def.lookahead ?? 0));
    const ctx: CalcCtx = { b: bars as Bar[], p: this.params, o: this.outs };
    for (const o of this.def.outputs) A(ctx, o.key);
    if (n > 0) this.def.calc(ctx, from);
    this.len = n;
    this.first = n ? bars[0]!.time : NaN;
    this.last = n ? bars[n - 1]!.time : NaN;
    return { from, full };
  }

  reset() {
    this.len = 0;
  }
}

/** Full calculation. Returns the public outputs only (internal buffers stripped). */
export function calcIndicator(type: IndicatorType, bars: readonly Bar[], params?: Partial<Record<string, unknown>>): Record<string, number[]> {
  const c = new IndicatorComputation(type, normalizeParams(type, params));
  c.run(bars);
  const out: Record<string, number[]> = {};
  for (const o of c.def.outputs) out[o.key] = c.outs[o.key]!;
  return out;
}

/* ------------------------------------------------------------------ */
/* Instances (per chart tab, persisted in the workspace)               */
/* ------------------------------------------------------------------ */

export interface OutputStyle {
  color?: string;
  width?: number;
  visible?: boolean;
}

export interface IndicatorInstance {
  uid: string;
  type: IndicatorType;
  params: Params;
  visible: boolean;
  /** Per-output overrides (colour: token name or CSS colour). */
  style?: Record<string, OutputStyle>;
  /** Horizontal levels override (oscillators). */
  levels?: number[];
}

const uid = () => Math.random().toString(36).slice(2, 10);

const LINE_CYCLE: ColorToken[] = ["ember", "gold", "info", "up", "down", "warn", "fg2"];

/** New instance with default params; single-line overlays get a colour not yet used on the chart. */
export function makeInstance(type: IndicatorType, existing: IndicatorInstance[] = [], params?: Partial<Record<string, unknown>>): IndicatorInstance {
  const inst: IndicatorInstance = { uid: uid(), type, params: normalizeParams(type, params), visible: true };
  const def = INDICATOR_DEFS[type];
  if (def.outputs.length === 1 && def.outputs[0]!.kind === "line" && existing.some((e) => e.type === type)) {
    const used = new Set(existing.flatMap((e) => (INDICATOR_DEFS[e.type].outputs.length === 1 ? [e.style?.[INDICATOR_DEFS[e.type].outputs[0]!.key]?.color ?? INDICATOR_DEFS[e.type].outputs[0]!.color] : [])));
    const col = LINE_CYCLE.find((c) => !used.has(c));
    if (col) inst.style = { [def.outputs[0]!.key]: { color: col } };
  }
  return inst;
}

const LEGACY: Record<string, [IndicatorType, Params]> = {
  sma20: ["sma", { period: 20 }],
  ema50: ["ema", { period: 50 }],
  bb: ["bb", {}],
  rsi: ["rsi", {}],
  macd: ["macd", {}],
};

/**
 * Up to workspace uiVersion 3 a new workspace's first chart started with EMA 50 and SMA 20. True when `list` is still
 * exactly that pair as it was added (EMA 50 close, then SMA 20 close, shown, default styles and levels): the one-time
 * migration removes it (app: indicators.dart isAutoIndicatorPair). Anything the client changed stays.
 */
export function isAutoIndicatorPair(list: readonly IndicatorInstance[]): boolean {
  const plainStyle = (s: IndicatorInstance["style"]) => !s || Object.values(s).every((o) => !o || (o.color === undefined && o.width === undefined && o.visible !== false));
  const untouched = (i: IndicatorInstance | undefined, type: IndicatorType, period: number) =>
    !!i &&
    i.type === type &&
    i.visible !== false &&
    i.levels === undefined &&
    plainStyle(i.style) &&
    Object.keys(i.params ?? {}).length === 2 &&
    i.params.period === period &&
    i.params.source === "close";
  return list.length === 2 && untouched(list[0], "ema", 50) && untouched(list[1], "sma", 20);
}

/** Workspace uiVersion 3 -> 4: a chart still showing exactly the automatic EMA 50 + SMA 20 loses them (app: Workspace.fromJson). */
export function stripAutoIndicators<W extends { tabs: { indicators: IndicatorInstance[] }[]; uiVersion?: number }>(w: W): W {
  return { ...w, tabs: w.tabs.map((t) => (isAutoIndicatorPair(t.indicators) ? { ...t, indicators: [] } : t)), uiVersion: 4 };
}

/** Accepts the old `IndicatorId[]` workspace format and the current instance list; drops anything unknown. */
export function migrateIndicators(raw: unknown): IndicatorInstance[] {
  if (!Array.isArray(raw)) return [];
  const out: IndicatorInstance[] = [];
  const seen = new Set<string>();
  for (const x of raw) {
    let inst: IndicatorInstance | null = null;
    if (typeof x === "string" && LEGACY[x]) inst = { uid: uid(), type: LEGACY[x][0], params: normalizeParams(LEGACY[x][0], LEGACY[x][1]), visible: true };
    else if (x && typeof x === "object" && isIndicatorType((x as IndicatorInstance).type)) {
      const o = x as Partial<IndicatorInstance> & { type: IndicatorType };
      inst = {
        uid: typeof o.uid === "string" && o.uid && !seen.has(o.uid) ? o.uid : uid(),
        type: o.type,
        params: normalizeParams(o.type, o.params as Record<string, unknown>),
        visible: o.visible !== false,
        ...(o.style && typeof o.style === "object" ? { style: o.style } : {}),
        ...(Array.isArray(o.levels) ? { levels: o.levels.filter((v) => typeof v === "number" && Number.isFinite(v)) } : {}),
      };
    }
    if (inst) {
      seen.add(inst.uid);
      out.push(inst);
    }
  }
  return out;
}

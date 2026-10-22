import { INSTRUMENTS, getInstrument, instrumentSpec, priceFeed, type Instrument, type Position, type TradingAccount } from "@kalks/mock";
import type { T as Translate } from "@kalks/i18n";

/* ------------------------------------------------------------------ */
/* Timeframes                                                          */
/* ------------------------------------------------------------------ */

export const TIMEFRAMES = ["M1", "M5", "M15", "M30", "H1", "H4", "D1", "W1", "MN"] as const;
export type Timeframe = (typeof TIMEFRAMES)[number];
export const TF_SECONDS: Record<Timeframe, number> = {
  M1: 60,
  M5: 300,
  M15: 900,
  M30: 1800,
  H1: 3600,
  H4: 14400,
  D1: 86400,
  W1: 604800,
  MN: 2592000,
};

export const CHART_TYPES = ["candles", "bars", "line", "area"] as const;
export type ChartType = (typeof CHART_TYPES)[number];

/** @deprecated Legacy preset ids, kept only so old saved workspaces can be migrated. Use the registry in `@/lib/indicators`. */
export const INDICATORS = [
  { id: "sma20", label: "Moving Average (SMA 20)", short: "SMA 20", pane: "main" },
  { id: "ema50", label: "Exponential MA (EMA 50)", short: "EMA 50", pane: "main" },
  { id: "bb", label: "Bollinger Bands (20, 2)", short: "BB 20,2", pane: "main" },
  { id: "rsi", label: "Relative Strength Index (14)", short: "RSI 14", pane: "sub" },
  { id: "macd", label: "MACD (12, 26, 9)", short: "MACD 12,26,9", pane: "sub" },
] as const;
export type IndicatorId = (typeof INDICATORS)[number]["id"];

/* ------------------------------------------------------------------ */
/* Contract maths                                                      */
/* ------------------------------------------------------------------ */

/** Size of one pip in price units, per instrument. */
export function pipSize(inst: Instrument): number {
  switch (inst.assetClass) {
    case "forex":
      return inst.digits === 3 || inst.digits === 4 ? 0.01 : 0.0001;
    case "metals":
      return inst.symbol === "XAUUSD" ? 0.1 : 0.01;
    case "indices":
      return 1;
    case "energies":
      return 0.01;
    case "crypto":
      return inst.digits >= 4 ? 0.0001 : inst.digits === 3 ? 0.01 : 1;
    case "stocks":
      return 0.01;
  }
}

/** Smallest price increment (a "point"). */
export const pointSize = (symbol: string) => 1 / 10 ** getInstrument(symbol).digits;

export function splitSymbol(symbol: string): { base: string; quote: string } {
  const inst = getInstrument(symbol);
  if (inst.assetClass === "forex") return { base: symbol.slice(0, 3), quote: symbol.slice(3) };
  if (inst.assetClass === "metals") return { base: symbol.slice(0, 3), quote: "USD" };
  return { base: symbol, quote: "USD" };
}

/** Multiplier that converts an amount in the symbol's quote currency to USD. */
export function quoteToUsd(symbol: string, price: number): number {
  const { base, quote } = splitSymbol(symbol);
  if (quote === "USD") return 1;
  if (base === "USD") return 1 / price;
  const cross = priceFeed().snapshot(`USD${quote}`);
  return cross ? 1 / cross.bid : 1 / price;
}

/** USD value of one pip for 1.00 lot. */
export function pipValuePerLot(symbol: string, price: number): number {
  const inst = getInstrument(symbol);
  return pipSize(inst) * inst.contractSize * quoteToUsd(symbol, price);
}

export function effectiveLeverage(symbol: string, leverage: number) {
  const inst = getInstrument(symbol);
  if (inst.assetClass === "forex" || inst.assetClass === "metals") return leverage;
  if (inst.assetClass === "crypto") return Math.min(leverage, 20);
  if (inst.assetClass === "stocks") return 5;
  return Math.min(leverage, 100);
}

/** Leverage the engine extras carry (lib/engine/map.ts) for the broker's risk leverage caps. */
type LeverageExtras = { engine?: { effectiveLeverage?: number; leverageByClass?: Record<string, number> } };

/**
 * The leverage a new trade on `symbol` gets on this account: the engine's effective leverage after the broker's risk
 * leverage caps (per asset class where a class cap is lower), else the account leverage. `marginRequired` then applies
 * the symbol's own maximum.
 */
export function accountLeverage(acc: TradingAccount & LeverageExtras, symbol: string): number {
  const e = acc.engine;
  if (!e?.effectiveLeverage) return acc.leverage;
  const cls = getInstrument(symbol).assetClass;
  return Math.min(e.leverageByClass?.[cls] ?? e.effectiveLeverage, e.effectiveLeverage);
}

/** "1:500", or "1:10 (capped from 1:500)" when the broker's risk leverage cap is lower than the account leverage. */
export function leverageLabel(acc: TradingAccount & LeverageExtras): string {
  const eff = acc.engine?.effectiveLeverage;
  return eff && eff < acc.leverage ? `1:${eff} (capped from 1:${acc.leverage})` : `1:${acc.leverage}`;
}

/** USD margin required for `lots` at `leverage`. */
export function marginRequired(symbol: string, lots: number, price: number, leverage: number): number {
  const inst = getInstrument(symbol);
  const { base } = splitSymbol(symbol);
  const notionalUsd = base === "USD" ? lots * inst.contractSize : lots * inst.contractSize * price * quoteToUsd(symbol, price);
  return notionalUsd / effectiveLeverage(symbol, leverage);
}

/** Floating P&L in USD, including swap and commission. */
export function profitUsd(p: Pick<Position, "symbol" | "side" | "volume" | "openPrice" | "swap" | "commission">, bid: number, ask: number) {
  const inst = getInstrument(p.symbol);
  const close = p.side === "buy" ? bid : ask;
  const diff = p.side === "buy" ? close - p.openPrice : p.openPrice - close;
  return diff * p.volume * inst.contractSize * quoteToUsd(p.symbol, close) + p.swap - p.commission;
}

/** Gross P&L (no swap/commission) if the position were closed at `price`. */
export function profitAt(p: Pick<Position, "symbol" | "side" | "volume" | "openPrice">, price: number) {
  const inst = getInstrument(p.symbol);
  const diff = p.side === "buy" ? price - p.openPrice : p.openPrice - price;
  return diff * p.volume * inst.contractSize * quoteToUsd(p.symbol, price);
}

export function fmtPrice(symbol: string, v: number) {
  const d = getInstrument(symbol).digits;
  return v.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d, useGrouping: false });
}
export const roundPrice = (symbol: string, v: number) => +v.toFixed(getInstrument(symbol).digits);

export function fmtVol(v: number) {
  return v.toFixed(2);
}

/** Account money: USD internally, shown in USC on cent accounts. */
export function accMoney(a: Pick<TradingAccount, "cent">, usd: number, opts: { signed?: boolean; decimals?: number } = {}) {
  const v = a.cent ? usd * 100 : usd;
  const d = opts.decimals ?? 2;
  const s = Math.abs(v).toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
  const sign = v < 0 ? "-" : opts.signed && v > 0 ? "+" : "";
  return `${sign}${s}`;
}
export const accCcy = (a: Pick<TradingAccount, "cent">) => (a.cent ? "USC" : "USD");

/**
 * Margin level state against the account group's levels (engine accounts carry them; 100 % / 50 %
 * otherwise): "stopout" at or below stop out, "call" at or below margin call, "low" below twice the call.
 */
export function marginState(level: number, a: TradingAccount): "ok" | "low" | "call" | "stopout" {
  if (!Number.isFinite(level)) return "ok";
  const e = (a as TradingAccount & { engine?: { marginCallLevel?: number; stopOutLevel?: number } }).engine;
  const call = e?.marginCallLevel ?? 100;
  const out = e?.stopOutLevel ?? 50;
  return level <= out ? "stopout" : level <= call ? "call" : level < call * 2 ? "low" : "ok";
}

/* ------------------------------------------------------------------ */
/* Orders                                                              */
/* ------------------------------------------------------------------ */

export type OrderType = "market" | "limit" | "stop" | "stop-limit";
export type Expiry = "GTC" | "Today" | "Date";

export interface PendingOrder {
  ticket: string;
  login: string;
  symbol: string;
  side: "buy" | "sell";
  type: Exclude<OrderType, "market">;
  volume: number;
  price: number;
  stopLimit?: number;
  sl?: number;
  tp?: number;
  trailing?: number;
  expiry: Expiry;
  expiryDate?: string;
  placed: string;
  source: TradeSource;
  comment?: string;
  oco?: string;
}

/** Who opened a trade. "ai" = the terminal's AI Trader. */
export type TradeSource = Position["source"] | "ai" | "pamm" | "mam";

export interface TPosition extends Omit<Position, "source"> {
  source: TradeSource;
  trailing?: number; // distance in price units
  comment?: string;
}

export interface TClosed extends TPosition {
  closePrice: number;
  closeTime: string;
  profit: number;
  reason?: string;
}

export const PENDING_LABEL = (o: Pick<PendingOrder, "side" | "type">) => `${o.side} ${o.type === "stop-limit" ? "stop limit" : o.type}`;
/** Translation key of the pending label ("buy limit"…) for UI text; PENDING_LABEL stays English for the journal. */
export const pendingLabelKey = (o: Pick<PendingOrder, "side" | "type">) => `order.pending.${o.side}.${o.type}` as const;

export const SEED_PENDING: PendingOrder[] = [
  { ticket: "49434302", login: "80412337", symbol: "XAUUSD", side: "buy", type: "limit", volume: 0.3, price: 2628.5, sl: 2612, tp: 2672, expiry: "GTC", placed: "2026-09-24T10:22:00Z", source: "manual" },
  { ticket: "49434318", login: "80412337", symbol: "GBPUSD", side: "sell", type: "stop", volume: 1, price: 1.2742, sl: 1.2791, tp: 1.2655, expiry: "Today", placed: "2026-09-24T12:40:00Z", source: "api" },
  { ticket: "49434355", login: "80412512", symbol: "BTCUSD", side: "buy", type: "stop-limit", volume: 0.05, price: 64200, stopLimit: 64150, sl: 62800, expiry: "Date", expiryDate: "2026-09-27", placed: "2026-09-24T14:05:00Z", source: "strategy" },
  { ticket: "49434371", login: "90022871", symbol: "EURUSD", side: "buy", type: "limit", volume: 2, price: 1.0812, sl: 1.0786, tp: 1.0875, expiry: "GTC", placed: "2026-09-24T09:10:00Z", source: "manual" },
];

/** Extra seed positions for accounts that have none in the shared mock. */
export const SEED_POSITIONS_EXTRA: TPosition[] = [
  { ticket: "49434410", login: "90022871", symbol: "XAUUSD", side: "buy", volume: 1, openPrice: 2646.72, sl: 2630, tp: 2690, swap: -6.4, commission: 0, openTime: "2026-09-24T08:44:12Z", source: "manual" },
  { ticket: "49434418", login: "90022871", symbol: "BTCUSD", side: "sell", volume: 0.25, openPrice: 63620, sl: 64600, swap: -2.1, commission: 0, openTime: "2026-09-24T12:05:31Z", source: "strategy" },
  { ticket: "49434421", login: "90022871", symbol: "GBPUSD", side: "buy", volume: 1.5, openPrice: 1.27702, tp: 1.2845, swap: 0.8, commission: 0, openTime: "2026-09-23T16:22:08Z", source: "manual" },
  { ticket: "49434433", login: "80413001", symbol: "EURUSD", side: "buy", volume: 0.2, openPrice: 1.08391, sl: 1.0812, swap: -0.3, commission: 0, openTime: "2026-09-24T10:12:48Z", source: "manual" },
  { ticket: "49434440", login: "90022904", symbol: "USDJPY", side: "buy", volume: 0.5, openPrice: 149.118, sl: 148.4, tp: 150.2, swap: 1.2, commission: 3.5, openTime: "2026-09-24T06:31:02Z", source: "api" },
];

export const SOURCE_LABEL: Record<TradeSource, string> = { manual: "Manual", copy: "Copy", api: "API", strategy: "Strategy", ai: "AI", pamm: "PAMM", mam: "MAM" };

export const SERVERS = ["Kalks-Live01", "Kalks-Live02", "Kalks-Demo", "Kalks-Prop01"] as const;

export const DEFAULT_SYMBOLS = ["XAUUSD", "EURUSD", "NAS100", "BTCUSD"];
export const ALL_SYMBOLS = INSTRUMENTS.map((i) => i.symbol);

/**
 * Contract specification (sessions, volumes, swaps): the effective trading spec of config/trading-specs.json (packages/mock
 * instrumentSpec, layered like the engine), with the instrument's own data for anything else.
 */
export function contractSpec(symbol: string) {
  const inst = getInstrument(symbol);
  const spec = instrumentSpec(symbol);
  const pip = pipSize(inst);
  const sessions: Record<Instrument["assetClass"], string> = {
    forex: "Mon 00:05 – Fri 23:55",
    metals: "Mon–Fri 01:05 – 23:55 (break 00:00–01:05)",
    indices: "Mon–Fri 01:05 – 23:15",
    energies: "Mon–Fri 01:05 – 23:55",
    crypto: "24/7",
    stocks: "Mon–Fri 16:35 – 22:55",
  };
  const session = inst.session === "hk_equity" ? "Mon–Fri 09:30 – 12:00, 13:00 – 16:00 HKT" : inst.session === "jp_equity" ? "Mon–Fri 09:00 – 11:30, 12:30 – 15:30 JST" : inst.session === "24x7" ? "24/7" : sessions[inst.assetClass];
  return {
    contractSize: spec?.contractSize ?? inst.contractSize,
    digits: inst.digits,
    pip,
    tickSize: 1 / 10 ** inst.digits,
    minVolume: spec?.lotMin ?? 0.01,
    maxVolume: spec?.lotMax ?? (inst.assetClass === "crypto" ? 20 : 100),
    step: spec?.lotStep ?? 0.01,
    marginCcy: inst.baseCcy ?? splitSymbol(symbol).base,
    profitCcy: inst.quoteCcy ?? splitSymbol(symbol).quote,
    swapLong: spec?.swapLong ?? 0,
    swapShort: spec?.swapShort ?? 0,
    /** "points" per lot per night (core instruments) or "percent_per_year" of the position value */
    swapUnit: spec?.swapUnit ?? "points",
    /** weekday charged three nights; "" = every night alike (crypto) */
    tripleSwap: spec ? spec.tripleSwapDay : inst.assetClass === "crypto" ? "" : "Wednesday",
    swapEveryNight: spec?.swapDays === "all",
    stopsLevel: spec?.stopsLevelPoints ?? (inst.assetClass === "forex" ? 10 : 20),
    sessions: session,
    execution: "Market",
    gtc: "Good till cancelled",
  };
}

/** A swap rate as shown to traders: "−20.00% / year" or "−7.20 pts" (a real minus sign). */
export function swapRateText(t: Translate, v: number, unit: "points" | "percent_per_year"): string {
  const n = `${v < 0 ? "−" : v > 0 ? "+" : ""}${Math.abs(v).toFixed(2)}`;
  return unit === "percent_per_year" ? t("desk.sw.pctYear", { n }) : t("order.unit.pts", { n });
}

/** "Long −20.00% / year · Short −20.00% / year (charged nightly)" (order popup, positions tooltips). */
export function swapSummary(t: Translate, symbol: string): string {
  const s = contractSpec(symbol);
  const when = s.swapEveryNight || !s.tripleSwap ? t("desk.sw.nightly") : t("desk.sw.triple", { day: t.dyn(`order.info.tripleSwapDay.${s.tripleSwap}`, s.tripleSwap) });
  return t("desk.sw.summary", { long: swapRateText(t, s.swapLong, s.swapUnit), short: swapRateText(t, s.swapShort, s.swapUnit), when });
}

/** Server time is GMT+3. */
export function serverTime(d = new Date()) {
  const t = new Date(d.getTime() + 3 * 3600 * 1000);
  const p = (n: number, l = 2) => String(n).padStart(l, "0");
  return {
    date: `${t.getUTCFullYear()}.${p(t.getUTCMonth() + 1)}.${p(t.getUTCDate())}`,
    time: `${p(t.getUTCHours())}:${p(t.getUTCMinutes())}:${p(t.getUTCSeconds())}`,
    ms: p(t.getUTCMilliseconds(), 3),
  };
}
export function fmtServer(iso: string | number, withSeconds = true) {
  const s = serverTime(new Date(iso));
  return `${s.date} ${withSeconds ? s.time : s.time.slice(0, 5)}`;
}

let seq = 49435000;
/** Strictly increasing ticket numbers with small random gaps (like a real server), never duplicated. */
export const nextTicket = () => String((seq += 1 + Math.floor(Math.random() * 7)));

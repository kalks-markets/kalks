/**
 * Back Office — dealing desk mocks: positions, orders, accounts, routing,
 * dealer overrides, exposure limits, rollovers and corporate actions.
 */
import { seeded, hashString } from "./rng";
import { getInstrument, INSTRUMENTS, type AssetClass } from "./symbols";
import { ADMIN_NOW, CLIENTS, clientAccounts, type AdminClient } from "./admin-clients";

const MIN = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;
const iso = (t: number) => new Date(t).toISOString();

export const TRADING_GROUPS = ["Standard", "Pro", "ECN", "Cent", "VIP", "Prop"] as const;
export type TradingGroup = (typeof TRADING_GROUPS)[number];

/* ------------------------------------------------------------------ */
/* Open positions across all clients                                   */
/* ------------------------------------------------------------------ */

/** D84: every order carries a source tag. "dealer" = created by staff in the Back Office. */
export type OrderSource = "manual" | "api" | "fix" | "webhook" | "strategy" | "copy" | "pamm" | "ai" | "dealer";

export interface AdminPosition {
  ticket: string;
  login: string;
  clientId: string;
  symbol: string;
  side: "buy" | "sell";
  volume: number;
  openPrice: number;
  sl?: number;
  tp?: number;
  swap: number;
  commission: number;
  openTime: string;
  source: OrderSource;
  platform: "Web" | "iOS" | "Android" | "API" | "Copy" | "Back Office";
  group: TradingGroup;
  route: "A" | "B";
  /** set when this ticket was split off another one (partial book transfer) */
  parentTicket?: string;
  comment?: string;
}

const POS_SYMBOLS = ["XAUUSD", "XAUUSD", "XAUUSD", "EURUSD", "EURUSD", "GBPUSD", "USDJPY", "NAS100", "NAS100", "US30", "BTCUSD", "BTCUSD", "ETHUSD", "USOIL", "GBPJPY", "GER40", "SOLUSD", "AAPL", "TSLA", "NVDA", "XAGUSD", "AUDUSD"];

export const ADMIN_POSITIONS: AdminPosition[] = (() => {
  const r = seeded(70411);
  const funded = CLIENTS.filter((c) => c.funded && c.logins.length);
  return Array.from({ length: 148 }, (_, i) => {
    const c = funded[(i * 13 + r.int(0, 5)) % funded.length]!;
    const symbol = r.pick(POS_SYMBOLS);
    const inst = getInstrument(symbol);
    const side = r.bool(symbol === "XAUUSD" || symbol === "NAS100" ? 0.68 : 0.5) ? "buy" : "sell";
    const big = c.risk >= 8 || c.group === "VIP";
    const volume = +(inst.assetClass === "indices" ? r.range(0.5, big ? 40 : 8) : inst.assetClass === "stocks" ? r.int(5, 200) : inst.assetClass === "crypto" ? r.range(0.05, big ? 6 : 1.5) : r.range(0.01, big ? 18 : 3)).toFixed(2);
    const openPrice = +(inst.price * (1 + r.normal() * (inst.assetClass === "forex" ? 0.0018 : 0.005))).toFixed(inst.digits);
    const dist = inst.price * 0.008;
    const group = (c.group as TradingGroup) ?? "Standard";
    return {
      ticket: String(49440000 - i * 29 - r.int(0, 20)),
      login: c.logins[0]!,
      clientId: c.id,
      symbol,
      side,
      volume,
      openPrice,
      sl: r.bool(0.6) ? +(side === "buy" ? openPrice - dist : openPrice + dist).toFixed(inst.digits) : undefined,
      tp: r.bool(0.45) ? +(side === "buy" ? openPrice + dist * 1.6 : openPrice - dist * 1.6).toFixed(inst.digits) : undefined,
      swap: +(r.normal() * 4 - 1).toFixed(2),
      commission: group === "ECN" ? +(volume * 7).toFixed(2) : 0,
      openTime: iso(ADMIN_NOW - r.int(1, 60 * 72) * MIN),
      source: r.pick(["manual", "manual", "manual", "copy", "api", "strategy"] as const),
      platform: r.pick(["Web", "Web", "iOS", "Android", "API", "Copy"] as const),
      group: i % 23 === 0 ? "Prop" : group,
      route: c.risk >= 8 || (volume >= 20 && r.bool(0.5)) ? "A" : "B",
    };
  });
})();

/** Client P&L for an admin position at the current quote. */
export function adminPositionPnl(p: AdminPosition, bid: number, ask: number) {
  const inst = getInstrument(p.symbol);
  const close = p.side === "buy" ? bid : ask;
  const diff = p.side === "buy" ? close - p.openPrice : p.openPrice - close;
  let pnl = diff * p.volume * inst.contractSize;
  if (p.symbol.endsWith("JPY")) pnl = pnl / close;
  return pnl + p.swap - p.commission;
}

/* ------------------------------------------------------------------ */
/* Pending orders                                                      */
/* ------------------------------------------------------------------ */

export interface AdminOrder {
  ticket: string;
  login: string;
  clientId: string;
  symbol: string;
  type: "Buy Limit" | "Sell Limit" | "Buy Stop" | "Sell Stop" | "Buy Stop Limit" | "Sell Stop Limit";
  volume: number;
  price: number;
  /** limit price of a stop-limit order */
  stopLimit?: number;
  sl?: number;
  tp?: number;
  placed: string;
  expiry: "GTC" | "Today" | string;
  group: TradingGroup;
  source?: OrderSource;
  comment?: string;
}

export const ADMIN_ORDERS: AdminOrder[] = (() => {
  const r = seeded(33120);
  const funded = CLIENTS.filter((c) => c.funded && c.logins.length);
  return Array.from({ length: 64 }, (_, i) => {
    const c = funded[(i * 7 + 3) % funded.length]!;
    const symbol = r.pick(POS_SYMBOLS);
    const inst = getInstrument(symbol);
    const type = r.pick(["Buy Limit", "Sell Limit", "Buy Stop", "Sell Stop", "Buy Limit", "Sell Limit", "Buy Stop Limit"] as const);
    const below = type === "Buy Limit" || type === "Sell Stop";
    const price = +(inst.price * (1 + (below ? -1 : 1) * r.range(0.001, 0.012))).toFixed(inst.digits);
    return {
      ticket: String(51220000 - i * 17),
      login: c.logins[0]!,
      clientId: c.id,
      symbol,
      type,
      volume: +(inst.assetClass === "stocks" ? r.int(5, 100) : r.range(0.01, 5)).toFixed(2),
      price,
      sl: r.bool(0.6) ? +(price * (type.startsWith("Buy") ? 0.994 : 1.006)).toFixed(inst.digits) : undefined,
      tp: r.bool(0.5) ? +(price * (type.startsWith("Buy") ? 1.01 : 0.99)).toFixed(inst.digits) : undefined,
      placed: iso(ADMIN_NOW - r.int(2, 60 * 96) * MIN),
      expiry: r.pick(["GTC", "GTC", "Today", "2026-09-30"]),
      group: c.group as TradingGroup,
    };
  });
})();

/* ------------------------------------------------------------------ */
/* All trading accounts                                                */
/* ------------------------------------------------------------------ */

export interface AdminAccountRow {
  login: string;
  clientId: string;
  group: string;
  type: "live" | "demo";
  leverage: number;
  currency: "USD" | "USC";
  balance: number;
  equity: number;
  credit: number;
  margin: number;
  route: "A" | "B";
  server: string;
  openPositions: number;
  created: string;
  status: "active" | "disabled" | "read-only";
}

export const ADMIN_ACCOUNTS: AdminAccountRow[] = CLIENTS.filter((c) => c.logins.length).flatMap((c: AdminClient) =>
  clientAccounts(c)
    .filter((a) => a.type === "live")
    .map((a) => ({
      login: a.login,
      clientId: c.id,
      group: a.group,
      type: a.type,
      leverage: a.leverage,
      currency: a.currency,
      balance: a.balance,
      equity: a.equity,
      credit: a.credit,
      margin: hashString(a.login) % 7 === 0 ? +(a.equity / (0.6 + (hashString(a.login) % 90) / 100)).toFixed(2) : a.margin,
      route: a.route,
      server: a.server,
      openPositions: a.openPositions,
      created: a.created,
      status: c.tradingDisabled ? "disabled" : c.status === "blocked" ? "read-only" : "active",
    })),
);

/* ------------------------------------------------------------------ */
/* A/B routing                                                         */
/* ------------------------------------------------------------------ */

export interface RoutingCondition {
  /** Risk score: engine client score 0-100; Risk category: toxic, likely_profitable, neutral, likely_losing */
  field: "Risk score" | "Risk category" | "Avg hold time" | "Lot size" | "Symbol" | "Group" | "Win rate (30d)" | "Equity" | "Country" | "News window" | "Login";
  op: "≥" | "≤" | "<" | ">" | "=" | "in" | "is";
  value: string;
}

export interface RoutingRule {
  id: string;
  name: string;
  conditions: RoutingCondition[];
  join: "AND" | "OR";
  action: { book: "A" | "B"; pct: number; lp?: string };
  enabled: boolean;
  hits24h: number;
  lots24h: number;
}

export const ROUTING_RULES: RoutingRule[] = [
  { id: "RR-1", name: "Toxic flow → A-book", conditions: [{ field: "Risk score", op: "≥", value: "8" }, { field: "Avg hold time", op: "<", value: "60s" }], join: "AND", action: { book: "A", pct: 100, lp: "Primary LP" }, enabled: true, hits24h: 1204, lots24h: 312.4 },
  { id: "RR-2", name: "Large tickets partial hedge", conditions: [{ field: "Lot size", op: "≥", value: "20" }], join: "AND", action: { book: "A", pct: 50, lp: "Primary LP" }, enabled: true, hits24h: 38, lots24h: 100.2 },
  { id: "RR-3", name: "News window on majors", conditions: [{ field: "News window", op: "is", value: "High impact ±2m" }, { field: "Symbol", op: "in", value: "EURUSD, GBPUSD, XAUUSD" }], join: "AND", action: { book: "A", pct: 30, lp: "Primary LP" }, enabled: false, hits24h: 0, lots24h: 0 },
  { id: "RR-4", name: "Prop funded accounts", conditions: [{ field: "Group", op: "is", value: "Prop" }], join: "AND", action: { book: "B", pct: 100 }, enabled: true, hits24h: 842, lots24h: 214.0 },
];

export const ROUTING_DEFAULT = { book: "B" as const, pct: 100, hits24h: 41_280, lots24h: 6_158.1 };

export const LP_CONNECTIONS: { name: string; protocol: string; status: "not_connected" | "connected" | "testing"; note: string }[] = [
  { name: "Primary LP", protocol: "FIX 4.4", status: "not_connected", note: "Credentials not configured" },
  { name: "Crypto LP", protocol: "FIX 4.4 / REST", status: "not_connected", note: "Planned — Phase 2" },
];

/* ------------------------------------------------------------------ */
/* Dealer overrides                                                    */
/* ------------------------------------------------------------------ */

export interface DealerOverride {
  clientId: string;
  login: string;
  group: string;
  markupPips: number;
  maxLot: number;
  execDelayMs: number;
  tradingDisabled: boolean;
  closeOnly: boolean;
  reason: string;
  setBy: string;
  updated: string;
}

export const DEALER_OVERRIDES: DealerOverride[] = (() => {
  const r = seeded(6021);
  const pool = CLIENTS.filter((c) => c.funded && c.logins.length).slice(4, 60);
  const reasons = ["RSK-01 · Latency arbitrage pattern", "RSK-02 · Scalping on thin symbols", "VIP-01 · Negotiated pricing", "RSK-03 · Exposure concentration", "CMP-01 · AML hold", "VIP-02 · Increased max lot"];
  return Array.from({ length: 14 }, (_, i) => {
    const c = pool[(i * 5) % pool.length]!;
    const reason = reasons[i % reasons.length]!;
    const vip = reason.startsWith("VIP");
    return {
      clientId: c.id,
      login: c.logins[0]!,
      group: c.group,
      markupPips: vip ? -0.2 : +r.range(0, 1.2).toFixed(1),
      maxLot: vip ? 100 : r.pick([2, 5, 10, 20, 50]),
      execDelayMs: vip || i % 3 === 1 ? 0 : r.pick([0, 50, 120, 250, 500]),
      tradingDisabled: reason.startsWith("CMP"),
      closeOnly: i === 7,
      reason,
      setBy: r.pick(["Julia Novak", "Omar Haddad", "Head of Dealing"]),
      updated: iso(ADMIN_NOW - r.int(1, 400) * HOUR),
    };
  });
})();

/* ------------------------------------------------------------------ */
/* Exposure limits                                                     */
/* ------------------------------------------------------------------ */

export interface ExposureLimit {
  symbol: string;
  maxNetLots: number;
  maxNotional: number;
  warnPct: number;
  highPct: number;
  autoHedgePct: number; // 0 = off
  enabled: boolean;
}

export const EXPOSURE_LIMITS: ExposureLimit[] = ["XAUUSD", "EURUSD", "BTCUSD", "NAS100", "GBPUSD", "USOIL", "USDJPY", "ETHUSD", "US30", "GER40", "XAGUSD", "SOLUSD"].map((s, i) => {
  const r = seeded(hashString("lim" + s));
  const inst = getInstrument(s);
  const maxNetLots = { XAUUSD: 280, EURUSD: 280, BTCUSD: 60, NAS100: 2000, GBPUSD: 90, USOIL: 110, USDJPY: 120, ETHUSD: 560 }[s] ?? r.int(40, 400);
  return {
    symbol: s,
    maxNetLots,
    maxNotional: Math.max(250_000, Math.round((maxNetLots * inst.contractSize * (s.startsWith("USD") ? 1 : inst.price)) / 100_000) * 100_000),
    warnPct: 70,
    highPct: 85,
    autoHedgePct: i < 2 ? 50 : 0,
    enabled: true,
  };
});

export const ASSET_CLASS_EXPOSURE: { cls: AssetClass; netUsd: number; grossUsd: number; pnl: number }[] = [
  { cls: "metals", netUsd: 63_800_000, grossUsd: 112_400_000, pnl: -21_480 },
  { cls: "forex", netUsd: -18_200_000, grossUsd: 146_900_000, pnl: 12_902 },
  { cls: "indices", netUsd: 24_600_000, grossUsd: 71_800_000, pnl: -8_144 },
  { cls: "crypto", netUsd: 2_620_000, grossUsd: 5_810_000, pnl: 3_990 },
  { cls: "energies", netUsd: -4_240_000, grossUsd: 9_800_000, pnl: 5_380 },
  { cls: "stocks", netUsd: 1_120_000, grossUsd: 2_340_000, pnl: -610 },
];

/* ------------------------------------------------------------------ */
/* Futures-CFD rollovers                                               */
/* ------------------------------------------------------------------ */

export interface Rollover {
  symbol: string;
  underlying: string;
  current: string;
  next: string;
  rollDate: string;
  currentPrice: number;
  nextPrice: number;
  method: "Price adjustment" | "Swap adjustment" | "Close & reopen";
  openPositions: number;
  netLots: number;
  status: "scheduled" | "due" | "applied";
}

export const ROLLOVERS: Rollover[] = [
  { symbol: "USOIL", underlying: "NYMEX WTI Crude", current: "CLX6 (Nov 26)", next: "CLZ6 (Dec 26)", rollDate: "2026-09-25", currentPrice: 71.84, nextPrice: 71.42, method: "Swap adjustment", openPositions: 214, netLots: -58.7, status: "due" },
  { symbol: "UKOIL", underlying: "ICE Brent Crude", current: "COX6 (Nov 26)", next: "COZ6 (Dec 26)", rollDate: "2026-09-29", currentPrice: 75.12, nextPrice: 74.81, method: "Swap adjustment", openPositions: 88, netLots: -12.4, status: "scheduled" },
  { symbol: "GER40", underlying: "Eurex DAX", current: "FDXZ6 (Dec 26)", next: "FDXH7 (Mar 27)", rollDate: "2026-12-14", currentPrice: 18994.2, nextPrice: 19086.9, method: "Price adjustment", openPositions: 142, netLots: 38.0, status: "scheduled" },
  { symbol: "JP225", underlying: "OSE Nikkei 225", current: "NKZ6 (Dec 26)", next: "NKH7 (Mar 27)", rollDate: "2026-12-08", currentPrice: 38742, nextPrice: 38620, method: "Price adjustment", openPositions: 61, netLots: 14.2, status: "scheduled" },
  { symbol: "UK100", underlying: "ICE FTSE 100", current: "Z6 (Dec 26)", next: "H7 (Mar 27)", rollDate: "2026-12-15", currentPrice: 8321.6, nextPrice: 8288.4, method: "Swap adjustment", openPositions: 34, netLots: -6.1, status: "scheduled" },
  { symbol: "XAGUSD", underlying: "COMEX Silver", current: "SIZ6 (Dec 26)", next: "SIH7 (Mar 27)", rollDate: "2026-11-24", currentPrice: 31.184, nextPrice: 31.402, method: "Swap adjustment", openPositions: 76, netLots: 22.8, status: "scheduled" },
  { symbol: "USOIL", underlying: "NYMEX WTI Crude", current: "CLV6 (Oct 26)", next: "CLX6 (Nov 26)", rollDate: "2026-08-26", currentPrice: 73.02, nextPrice: 72.61, method: "Swap adjustment", openPositions: 198, netLots: -41.2, status: "applied" },
];

/* ------------------------------------------------------------------ */
/* Corporate actions (stock CFDs)                                      */
/* ------------------------------------------------------------------ */

export interface CorporateAction {
  id: string;
  symbol: string;
  type: "Dividend" | "Split" | "Special dividend";
  exDate: string;
  payDate?: string;
  amount?: number; // per share, USD
  ratio?: string; // e.g. "10:1"
  longPositions: number;
  shortPositions: number;
  longQty: number;
  shortQty: number;
  status: "upcoming" | "due" | "applied";
}

export const CORPORATE_ACTIONS: CorporateAction[] = [
  { id: "CA-3101", symbol: "AAPL", type: "Dividend", exDate: "2026-09-26", payDate: "2026-10-02", amount: 0.26, longPositions: 184, shortPositions: 41, longQty: 12_480, shortQty: 2_110, status: "due" },
  { id: "CA-3102", symbol: "NVDA", type: "Split", exDate: "2026-10-08", ratio: "4:1", longPositions: 312, shortPositions: 58, longQty: 21_400, shortQty: 3_860, status: "upcoming" },
  { id: "CA-3103", symbol: "META", type: "Dividend", exDate: "2026-09-29", payDate: "2026-10-15", amount: 0.525, longPositions: 96, shortPositions: 22, longQty: 3_940, shortQty: 710, status: "upcoming" },
  { id: "CA-3104", symbol: "TSLA", type: "Split", exDate: "2026-11-12", ratio: "3:1", longPositions: 241, shortPositions: 133, longQty: 8_820, shortQty: 5_140, status: "upcoming" },
  { id: "CA-3105", symbol: "NFLX", type: "Special dividend", exDate: "2026-10-21", payDate: "2026-10-30", amount: 1.5, longPositions: 38, shortPositions: 12, longQty: 1_120, shortQty: 260, status: "upcoming" },
  { id: "CA-3099", symbol: "AAPL", type: "Dividend", exDate: "2026-08-11", payDate: "2026-08-14", amount: 0.25, longPositions: 170, shortPositions: 36, longQty: 11_900, shortQty: 1_980, status: "applied" },
];

export const TRADING_INSTRUMENTS = INSTRUMENTS;
export { ADMIN_NOW };
export const _DAY = DAY;

import { seeded } from "./rng";
import { PEOPLE } from "./people";
import { ACCOUNTS, HISTORY, accountUsd, type ClosedTrade } from "./client";

/* ------------------------------------------------------------------ */
/* Server clock                                                        */
/* ------------------------------------------------------------------ */

/** Fixed "now" for mock data (server time GMT+3). */
export const PORTFOLIO_NOW = Date.parse("2026-09-24T18:00:00Z");
export const SERVER_OFFSET_H = 3;

/** Hour (0–23) and weekday (0=Sun) of an ISO time in server time (GMT+3). */
export function serverParts(iso: string) {
  const d = new Date(Date.parse(iso) + SERVER_OFFSET_H * 3600_000);
  return { hour: d.getUTCHours(), dow: d.getUTCDay(), date: d.toISOString().slice(0, 10) };
}

/* ------------------------------------------------------------------ */
/* Sources of the overall portfolio                                    */
/* ------------------------------------------------------------------ */

export interface PammInvestment {
  id: string;
  fund: string;
  manager: (typeof PEOPLE)[number];
  invested: number;
  value: number;
  sharePct: number;
  since: string;
  monthPct: number;
}

export const PAMM_INVESTMENTS: PammInvestment[] = [
  { id: "PM-2041", fund: "Aurum Macro", manager: PEOPLE[5]!, invested: 8000, value: 9412.36, sharePct: 1.84, since: "2025-11-02", monthPct: 3.41 },
  { id: "PM-2107", fund: "Nordic FX Carry", manager: PEOPLE[12]!, invested: 4000, value: 4288.9, sharePct: 0.62, since: "2026-03-18", monthPct: 1.12 },
];

export interface CopySubscription {
  id: string;
  master: (typeof PEOPLE)[number];
  strategy: string;
  allocated: number;
  equity: number;
  feePct: number;
  since: string;
  monthPct: number;
}

export const COPY_SUBSCRIPTIONS: CopySubscription[] = [
  { id: "CP-88120", master: PEOPLE[2]!, strategy: "Rio Gold Momentum", allocated: 3000, equity: 3624.12, feePct: 20, since: "2026-01-09", monthPct: 5.82 },
  { id: "CP-88341", master: PEOPLE[14]!, strategy: "Tokyo Session Scalper", allocated: 2500, equity: 2391.4, feePct: 25, since: "2026-06-22", monthPct: -1.94 },
  { id: "CP-88502", master: PEOPLE[9]!, strategy: "Index Swing Pro", allocated: 1500, equity: 1718.05, feePct: 15, since: "2026-08-04", monthPct: 2.47 },
];

export const IB_EARNINGS = { available: 1488.2, pending: 212.4, lifetime: 18640.35, thisMonth: 1488.2, lastMonth: 1302.9 };

export const LIVE_ACCOUNTS = ACCOUNTS.filter((a) => a.type === "live");
export const DEMO_ACCOUNTS = ACCOUNTS.filter((a) => a.type === "demo");

export const PORTFOLIO_TOTALS = (() => {
  const live = LIVE_ACCOUNTS.reduce((s, a) => s + accountUsd(a, "equity"), 0);
  const pamm = PAMM_INVESTMENTS.reduce((s, p) => s + p.value, 0);
  const copy = COPY_SUBSCRIPTIONS.reduce((s, c) => s + c.equity, 0);
  const ib = IB_EARNINGS.available + IB_EARNINGS.pending;
  const demo = DEMO_ACCOUNTS.reduce((s, a) => s + a.equity, 0);
  const total = live + pamm + copy + ib;
  return { live, pamm, copy, ib, demo, total, changeToday: 1382.64, changeTodayPct: 1.72, changeMonth: 7614.3, changeMonthPct: 10.21 };
})();

export const ASSET_ALLOCATION = [
  { label: "Forex", value: 0.31 },
  { label: "Metals", value: 0.24 },
  { label: "Indices", value: 0.18 },
  { label: "Crypto", value: 0.12 },
  { label: "Energies", value: 0.05 },
  { label: "Stocks", value: 0.04 },
  { label: "Cash / free margin", value: 0.06 },
];

/** Net exposure by symbol across live accounts, PAMM and copy (USD notional). */
export const EXPOSURE = [
  { symbol: "XAUUSD", long: 184_200, short: 22_400 },
  { symbol: "NAS100", long: 96_400, short: 18_100 },
  { symbol: "EURUSD", long: 41_200, short: 150_300 },
  { symbol: "BTCUSD", long: 63_400, short: 0 },
  { symbol: "GBPJPY", long: 12_600, short: 57_300 },
  { symbol: "USOIL", long: 8_200, short: 57_500 },
  { symbol: "US30", long: 42_300, short: 12_900 },
  { symbol: "USDJPY", long: 30_100, short: 44_800 },
];

/* ------------------------------------------------------------------ */
/* Analytics                                                           */
/* ------------------------------------------------------------------ */

export interface TradeStats {
  trades: number;
  wins: number;
  losses: number;
  winRate: number;
  grossProfit: number;
  grossLoss: number;
  net: number;
  profitFactor: number;
  avgWin: number;
  avgLoss: number;
  rr: number;
  best: ClosedTrade;
  worst: ClosedTrade;
  avgHoldMin: number;
  lots: number;
  commission: number;
  swap: number;
  expectancy: number;
}

export function tradeStats(list: ClosedTrade[]): TradeStats {
  const wins = list.filter((t) => t.profit > 0);
  const losses = list.filter((t) => t.profit <= 0);
  const grossProfit = wins.reduce((s, t) => s + t.profit, 0);
  const grossLoss = Math.abs(losses.reduce((s, t) => s + t.profit, 0));
  const avgWin = wins.length ? grossProfit / wins.length : 0;
  const avgLoss = losses.length ? grossLoss / losses.length : 0;
  const hold = list.reduce((s, t) => s + (Date.parse(t.closeTime) - Date.parse(t.openTime)) / 60000, 0);
  const sorted = [...list].sort((a, b) => b.profit - a.profit);
  return {
    trades: list.length,
    wins: wins.length,
    losses: losses.length,
    winRate: list.length ? (wins.length / list.length) * 100 : 0,
    grossProfit,
    grossLoss,
    net: grossProfit - grossLoss,
    profitFactor: grossLoss ? grossProfit / grossLoss : 0,
    avgWin,
    avgLoss,
    rr: avgLoss ? avgWin / avgLoss : 0,
    best: sorted[0]!,
    worst: sorted[sorted.length - 1]!,
    avgHoldMin: list.length ? hold / list.length : 0,
    lots: list.reduce((s, t) => s + t.volume, 0),
    commission: list.reduce((s, t) => s + t.commission, 0),
    swap: list.reduce((s, t) => s + t.swap, 0),
    expectancy: list.length ? (grossProfit - grossLoss) / list.length : 0,
  };
}

export function groupPnl<K extends string | number>(list: ClosedTrade[], key: (t: ClosedTrade) => K) {
  const m = new Map<K, { key: K; pnl: number; trades: number; wins: number }>();
  for (const t of list) {
    const k = key(t);
    const e = m.get(k) ?? { key: k, pnl: 0, trades: 0, wins: 0 };
    e.pnl += t.profit;
    e.trades += 1;
    if (t.profit > 0) e.wins += 1;
    m.set(k, e);
  }
  return [...m.values()];
}

/** Weekday × hour P&L grid (server time) used for the heatmap — seeded with realistic session bias. */
export const HOUR_HEATMAP: { day: string; cells: { pnl: number; trades: number }[] }[] = (() => {
  const r = seeded(8812);
  const days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  return days.map((day, di) => ({
    day,
    cells: Array.from({ length: 24 }, (_, h) => {
      const weekend = di >= 5;
      const london = h >= 10 && h < 13;
      const overlap = h >= 15 && h < 19;
      const asia = h >= 2 && h < 8;
      const late = h >= 22 || h < 2;
      const base = weekend ? (r.bool(0.25) ? 1 : 0) : london ? r.int(4, 12) : overlap ? r.int(5, 14) : asia ? r.int(0, 4) : late ? r.int(0, 2) : r.int(1, 6);
      const trades = base;
      const bias = london ? 62 : overlap ? 34 : asia ? -22 : late ? -48 : 6;
      const pnl = trades === 0 ? 0 : +(trades * (bias + r.normal() * 55) + (di === 4 && overlap ? -180 : 0)).toFixed(2);
      return { pnl, trades };
    }),
  }));
})();

export const SESSIONS_PERF = [
  { session: "London open", hours: "10:00–13:00", pnl: 6412.8, winRate: 71.4, trades: 612 },
  { session: "NY overlap", hours: "15:00–19:00", pnl: 4120.35, winRate: 63.2, trades: 704 },
  { session: "Asia", hours: "02:00–08:00", pnl: -842.1, winRate: 48.9, trades: 186 },
  { session: "Late NY", hours: "22:00–02:00", pnl: -1310.44, winRate: 41.6, trades: 94 },
];

/** Lifetime money flow (USD) for the waterfall. */
export const MONEY_FLOW = {
  deposits: 42_500,
  tradingPnl: 21_864.3,
  earnings: 4_112.85,
  charges: -2_218.62,
  withdrawals: -9_700,
};

export const CHARGES_BREAKDOWN = [
  { label: "Commission", value: 1_164.2, note: "ECN & Pro raw pricing" },
  { label: "Swap", value: 712.8, note: "Overnight financing (net)" },
  { label: "Fees", value: 341.62, note: "Copy / PAMM performance fees, withdrawal fees" },
];
export const SPREAD_COST_INFO = 3_482.5;

export const INSIGHTS = [
  {
    id: "overtrade",
    tone: "down" as const,
    icon: "fire",
    title: "You overtrade after losses",
    stat: "42%",
    text: "42% of your trades were opened within 10 min of a losing trade. Their win rate is 38% vs 67% for the rest.",
    tip: "Add a 15-minute cool-down after two losses in a row.",
  },
  {
    id: "session",
    tone: "up" as const,
    icon: "trophy",
    title: "Best session: London open",
    stat: "+$6.4K",
    text: "10:00–13:00 server time delivers 51% of your net profit with a 71% win rate.",
    tip: "Concentrate size in this window and trade lighter in Asia.",
  },
  {
    id: "risk",
    tone: "warn" as const,
    icon: "warning",
    title: "Risk per trade is creeping up",
    stat: "2.8%",
    text: "Your average risk per trade rose from 1.1% to 2.8% of equity over the last 30 days, above your 2% target.",
    tip: "Use the position-size calculator or set a max-lot rule on account 80412337.",
  },
  {
    id: "revenge",
    tone: "down" as const,
    icon: "chart_decreasing",
    title: "Revenge trades cost you $1,284",
    stat: "-$1.3K",
    text: "19 trades doubled volume right after a loss. Together they lost $1,284.60 this quarter.",
    tip: "Keep lot size fixed for the rest of the session after a losing trade.",
  },
  {
    id: "hold",
    tone: "gold" as const,
    icon: "hourglass_not_done",
    title: "Winners are cut too early",
    stat: "1.3×",
    text: "You hold losing trades 1.3× longer than winners. Letting XAUUSD winners run to TP would have added +$2,140.",
    tip: "Try a trailing stop instead of closing manually.",
  },
  {
    id: "focus",
    tone: "up" as const,
    icon: "gem_stone",
    title: "Gold is your edge",
    stat: "PF 2.4",
    text: "XAUUSD has a profit factor of 2.4 across 58 trades, the best of all the symbols you trade.",
    tip: "Consider an alert on the London open for XAUUSD breakouts.",
  },
];

/* ------------------------------------------------------------------ */
/* Unified ledger                                                      */
/* ------------------------------------------------------------------ */

export type LedgerType = "deposit" | "withdrawal" | "transfer-in" | "transfer-out" | "trade" | "commission" | "swap" | "bonus" | "ib-payout" | "fee";

export interface LedgerEntry {
  id: string;
  time: string;
  login: string;
  type: LedgerType;
  amount: number;
  ref: string;
  note: string;
  balance: number;
}

export const LEDGER_TYPE_LABEL: Record<LedgerType, string> = {
  deposit: "Deposit",
  withdrawal: "Withdrawal",
  "transfer-in": "Transfer in",
  "transfer-out": "Transfer out",
  trade: "Trade P&L",
  commission: "Commission",
  swap: "Swap",
  bonus: "Bonus",
  "ib-payout": "IB payout",
  fee: "Fee",
};

export const LEDGER: LedgerEntry[] = (() => {
  const r = seeded(5150);
  type Raw = Omit<LedgerEntry, "balance" | "id">;
  const raw: Raw[] = [];
  for (const t of HISTORY) {
    const gross = +(t.profit - t.swap + t.commission).toFixed(2);
    raw.push({ time: t.closeTime, login: t.login, type: "trade", amount: gross, ref: `#${t.ticket}`, note: `${t.side === "buy" ? "Buy" : "Sell"} ${t.volume} ${t.symbol} @ ${t.closePrice}` });
    if (t.commission) raw.push({ time: t.closeTime, login: t.login, type: "commission", amount: -t.commission, ref: `#${t.ticket}`, note: `Commission ${t.volume} lots ${t.symbol}` });
    if (t.swap) raw.push({ time: t.closeTime, login: t.login, type: "swap", amount: t.swap, ref: `#${t.ticket}`, note: `Overnight swap ${t.symbol}` });
  }
  const t0 = Date.parse(HISTORY[HISTORY.length - 1]!.closeTime);
  const span = PORTFOLIO_NOW - t0;
  const logins = ["80412337", "80412512", "80413001"];
  for (const login of logins) raw.push({ time: new Date(t0 - 86400_000 * 3).toISOString(), login, type: "deposit", amount: login === "80413001" ? 5000 : 15000, ref: `TX${r.int(700000, 799999)}`, note: "USDT TRC20 deposit · 20/20 confirmations" });
  const extra: [LedgerType, number, number][] = [
    ["transfer-in", 6, 3500], ["transfer-in", 4, 2000], ["transfer-out", 3, -1500], ["withdrawal", 3, -2500], ["deposit", 3, 4000],
    ["bonus", 1, 250], ["ib-payout", 4, 412.8], ["fee", 3, -1],
  ];
  for (const [type, n, amt] of extra) {
    for (let i = 0; i < n; i++) {
      const login = r.pick(logins);
      const amount = type === "ib-payout" ? +r.range(180, 520).toFixed(2) : type === "fee" ? -1 : Math.round((amt * r.range(0.6, 1.4)) / 50) * 50;
      raw.push({
        time: new Date(t0 + r.next() * span).toISOString(),
        login,
        type,
        amount,
        ref: `TX${r.int(800000, 904412)}`,
        note:
          type === "transfer-in" ? "From Wallet (USDT)" :
          type === "transfer-out" ? `To account ${logins.find((l) => l !== login)}` :
          type === "withdrawal" ? "To Wallet → TRC20 TN4b…u8Qa" :
          type === "deposit" ? "USDT TRC20 deposit · 20/20 confirmations" :
          type === "bonus" ? "Welcome bonus (credit)" :
          type === "ib-payout" ? "Partner commission payout" :
          "Withdrawal network fee",
      });
    }
  }
  raw.sort((a, b) => Date.parse(a.time) - Date.parse(b.time));
  const bal: Record<string, number> = {};
  const out: LedgerEntry[] = raw.map((e, i) => {
    bal[e.login] = +((bal[e.login] ?? 0) + e.amount).toFixed(2);
    return { ...e, id: `L${(100000 + i).toString()}`, balance: bal[e.login]! };
  });
  return out.reverse();
})();

/* ------------------------------------------------------------------ */
/* Statements                                                          */
/* ------------------------------------------------------------------ */

export interface MonthlyStatement {
  id: string;
  month: string; // YYYY-MM
  label: string;
  login: string;
  trades: number;
  net: number;
  deposits: number;
  withdrawals: number;
  sizeKb: number;
  generatedAt: string;
}

export const MONTHLY_STATEMENTS: MonthlyStatement[] = (() => {
  const r = seeded(3131);
  const out: MonthlyStatement[] = [];
  for (let i = 0; i < 12; i++) {
    const d = new Date(Date.UTC(2026, 8 - i - 1, 1));
    const month = d.toISOString().slice(0, 7);
    const label = d.toLocaleDateString("en-GB", { month: "long", year: "numeric", timeZone: "UTC" });
    for (const login of ["80412337", "80412512"]) {
      if (login === "80412512" && i > 3 && r.bool(0.3)) continue;
      out.push({
        id: `ST-${month.replace("-", "")}-${login.slice(-4)}`,
        month,
        label,
        login,
        trades: r.int(24, 160),
        net: +(r.normal() * 1400 + 900).toFixed(2),
        deposits: r.bool(0.4) ? Math.round(r.range(10, 60)) * 100 : 0,
        withdrawals: r.bool(0.25) ? -Math.round(r.range(5, 30)) * 100 : 0,
        sizeKb: r.int(84, 420),
        generatedAt: new Date(Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 1, 0, 5)).toISOString(),
      });
    }
  }
  return out;
})();

export const BROKER_INFO = {
  legal: "Kalks Markets Ltd",
  address: "Suite 305, Griffith Corporate Centre, Kingstown, St. Vincent and the Grenadines",
  licence: "Registration No. 27114 BC 2023",
  support: "support@kalkstrade.com",
};

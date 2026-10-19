import { seeded, hashString } from "./rng";
import { INSTRUMENTS, getInstrument } from "./symbols";
import { PEOPLE } from "./people";

/* ------------------------------------------------------------------ */
/* Current signed-in client                                            */
/* ------------------------------------------------------------------ */

export const ME = {
  ...PEOPLE[0]!,
  firstName: "Arjun",
  phone: "+91 98201 44721",
  dob: "1991-06-14",
  kycStatus: "pending" as "unverified" | "pending" | "verified" | "rejected",
  kycLevel: 1,
  memberSince: "2024-02-11",
  ibLevel: 2,
  ibLevelName: "Silver Partner",
  referralCode: "ARJUN24",
  referralLink: "https://app.kalkstrade.com/r/ARJUN24",
  serverTimezone: "GMT+3",
};

export const ONBOARDING = [
  { key: "email", label: "Verify email", done: true },
  { key: "phone", label: "Verify phone", done: true },
  { key: "deposit", label: "First deposit", done: true },
  { key: "kyc", label: "Complete KYC", done: false },
];

/* ------------------------------------------------------------------ */
/* Trading accounts                                                    */
/* ------------------------------------------------------------------ */

export type AccountMode = "hedging" | "netting";
export interface TradingAccount {
  login: string;
  type: "live" | "demo";
  group: string; // Standard / Pro / ECN / Cent
  mode: AccountMode;
  cent: boolean;
  server: string;
  leverage: number;
  currency: "USD" | "USC";
  balance: number;
  equity: number;
  credit: number;
  margin: number;
  nickname?: string;
  createdAt: string;
  expiresAt?: string; // demo only
  refillsLeft?: number; // demo only
  swapFree: boolean;
}

export const ACCOUNTS: TradingAccount[] = [
  { login: "80412337", type: "live", group: "Pro", mode: "hedging", cent: false, server: "Kalks-Live01", leverage: 500, currency: "USD", balance: 25000, equity: 26204.18, credit: 0, margin: 2041.1, nickname: "Main swing", createdAt: "2024-02-12", swapFree: false },
  { login: "80412512", type: "live", group: "Standard", mode: "netting", cent: false, server: "Kalks-Live01", leverage: 200, currency: "USD", balance: 18450.2, equity: 18200.45, credit: 250, margin: 612.4, nickname: "Gold scalps", createdAt: "2024-05-03", swapFree: true },
  { login: "80413001", type: "live", group: "Cent", mode: "hedging", cent: true, server: "Kalks-Live02", leverage: 1000, currency: "USC", balance: 1240500, equity: 1251097, credit: 0, margin: 18420, createdAt: "2024-08-19", swapFree: false },
  { login: "90022871", type: "demo", group: "Pro", mode: "hedging", cent: false, server: "Kalks-Demo", leverage: 500, currency: "USD", balance: 100000, equity: 102418.62, credit: 0, margin: 3120, createdAt: "2026-09-18", expiresAt: "2026-09-28", refillsLeft: 2, swapFree: false },
  { login: "90022904", type: "demo", group: "ECN", mode: "netting", cent: false, server: "Kalks-Demo", leverage: 100, currency: "USD", balance: 10000, equity: 9612.4, credit: 0, margin: 402.5, createdAt: "2026-09-21", expiresAt: "2026-10-01", refillsLeft: 3, swapFree: false },
];

export function accountUsd(a: TradingAccount, field: "balance" | "equity" | "margin" | "credit") {
  return a.cent ? a[field] / 100 : a[field];
}
export const freeMargin = (a: TradingAccount) => a.equity - a.margin;
export const marginLevel = (a: TradingAccount) => (a.margin > 0 ? (a.equity / a.margin) * 100 : Infinity);

export const ACCOUNT_GROUPS = [
  { id: "standard", name: "Standard", tagline: "Zero commission, all-in spreads", minDeposit: 10, spreadFrom: "1.0", commission: "None", leverage: [50, 100, 200, 500, 1000], modes: ["hedging", "netting"], cent: false, photo: "/assets/photos/skyline.jpg", popular: false },
  { id: "pro", name: "Pro", tagline: "Tight raw-feel spreads for active traders", minDeposit: 200, spreadFrom: "0.3", commission: "None", leverage: [50, 100, 200, 500], modes: ["hedging", "netting"], cent: false, photo: "/assets/photos/london.jpg", popular: true },
  { id: "ecn", name: "ECN", tagline: "Raw spreads from 0.0 + $3.5/lot/side", minDeposit: 500, spreadFrom: "0.0", commission: "$3.5 / lot / side", leverage: [50, 100, 200, 500], modes: ["netting", "hedging"], cent: false, photo: "/assets/photos/nyc.jpg", popular: false },
  { id: "cent", name: "Cent", tagline: "Trade in cents — ideal to test strategies live", minDeposit: 10, spreadFrom: "1.0", commission: "None", leverage: [100, 500, 1000, 2000], modes: ["hedging"], cent: true, photo: "/assets/photos/singapore.jpg", popular: false },
];

/* ------------------------------------------------------------------ */
/* Wallet                                                              */
/* ------------------------------------------------------------------ */

export const WALLET = {
  address: "TQ7xK2mB9vYwP4hLrE8nJ3cZa1sD6fG9Kf",
  network: "TRON (TRC20)",
  confirmationsRequired: 20,
  assets: [
    { asset: "USDT", network: "TRC20", balance: 12480.22, usd: 12480.22, icon: "usdt", change: 0 },
    { asset: "TRX", network: "TRON", balance: 1840.5, usd: 296.32, icon: "trx", change: 1.8 },
    { asset: "BTC", network: "Bitcoin", balance: 0.0142, usd: 900.45, icon: "btc", change: 2.84 },
  ],
  limits: { minDeposit: 10, minWithdraw: 20, maxDailyWithdraw: 50000, withdrawFee: 1 },
};

export type TxType = "deposit" | "withdrawal" | "transfer" | "ib-payout" | "copy-fee" | "bonus" | "conversion";
export type TxStatus = "completed" | "pending" | "processing" | "rejected";
export interface WalletTx {
  id: string;
  type: TxType;
  status: TxStatus;
  amount: number;
  asset: string;
  from: string;
  to: string;
  hash?: string;
  confirmations?: number;
  fee: number;
  createdAt: string; // ISO
}

export const WALLET_TXS: WalletTx[] = (() => {
  const r = seeded(77);
  const out: WalletTx[] = [];
  const types: TxType[] = ["deposit", "deposit", "transfer", "transfer", "withdrawal", "ib-payout", "copy-fee", "conversion", "bonus"];
  let t = Date.parse("2026-09-24T18:40:00Z");
  for (let i = 0; i < 36; i++) {
    const type = r.pick(types);
    const amount = +(type === "bonus" ? 250 : r.range(40, 6000)).toFixed(2);
    const status: TxStatus = i === 0 ? "processing" : i === 2 ? "pending" : r.bool(0.05) ? "rejected" : "completed";
    const acc = r.pick(ACCOUNTS.filter((a) => a.type === "live"));
    out.push({
      id: `TX${(904412 - i * 37).toString()}`,
      type,
      status,
      amount,
      asset: "USDT",
      from: type === "deposit" ? "TRC20 · external" : type === "transfer" ? "Wallet" : type === "withdrawal" ? "Wallet" : type === "ib-payout" ? "Partner program" : type === "copy-fee" ? `Account ${acc.login}` : "Wallet",
      to: type === "deposit" ? "Wallet" : type === "transfer" ? `Account ${acc.login}` : type === "withdrawal" ? "TRC20 · TN4b…u8Qa" : "Wallet",
      hash: type === "deposit" || type === "withdrawal" ? `${hashString("tx" + i).toString(16)}a91f${(i * 7919).toString(16)}e0c4b2` : undefined,
      confirmations: type === "deposit" ? (status === "processing" || status === "pending" ? 12 : 20) : undefined,
      fee: type === "withdrawal" ? 1 : 0,
      createdAt: new Date(t).toISOString(),
    });
    t -= r.int(2, 40) * 3600 * 1000;
  }
  return out;
})();

/* ------------------------------------------------------------------ */
/* Positions & history                                                 */
/* ------------------------------------------------------------------ */

export interface Position {
  ticket: string;
  login: string;
  symbol: string;
  side: "buy" | "sell";
  volume: number;
  openPrice: number;
  sl?: number;
  tp?: number;
  swap: number;
  commission: number;
  openTime: string;
  source: "manual" | "copy" | "api" | "strategy";
}

export const POSITIONS: Position[] = [
  { ticket: "49433784", login: "80412337", symbol: "XAUUSD", side: "buy", volume: 0.5, openPrice: 2641.18, sl: 2628, tp: 2680, swap: -3.2, commission: 0, openTime: "2026-09-24T09:12:44Z", source: "manual" },
  { ticket: "49433812", login: "80412337", symbol: "EURUSD", side: "sell", volume: 1.2, openPrice: 1.08612, sl: 1.0892, tp: 1.0801, swap: -1.84, commission: 0, openTime: "2026-09-23T14:03:10Z", source: "manual" },
  { ticket: "49434011", login: "80412337", symbol: "NAS100", side: "buy", volume: 2, openPrice: 20021.6, sl: 19880, tp: 20300, swap: -6.1, commission: 0, openTime: "2026-09-24T13:31:02Z", source: "strategy" },
  { ticket: "49434120", login: "80412512", symbol: "BTCUSD", side: "buy", volume: 0.1, openPrice: 62890, sl: 61500, swap: -4.4, commission: 0, openTime: "2026-09-22T21:44:19Z", source: "copy" },
  { ticket: "49434188", login: "80412512", symbol: "GBPJPY", side: "sell", volume: 0.3, openPrice: 191.412, tp: 189.9, swap: 0.62, commission: 0, openTime: "2026-09-24T07:18:55Z", source: "api" },
  { ticket: "49434201", login: "80413001", symbol: "USOIL", side: "sell", volume: 0.8, openPrice: 72.41, sl: 73.4, tp: 70.2, swap: -0.9, commission: 0, openTime: "2026-09-24T11:02:37Z", source: "manual" },
];

export function positionProfit(p: Position, bid: number, ask: number) {
  const inst = getInstrument(p.symbol);
  const close = p.side === "buy" ? bid : ask;
  const diff = p.side === "buy" ? close - p.openPrice : p.openPrice - close;
  let pnl = diff * p.volume * inst.contractSize;
  if (p.symbol.endsWith("JPY")) pnl = pnl / close; // quote currency → USD
  return pnl + p.swap - p.commission;
}

export interface ClosedTrade extends Position {
  closePrice: number;
  closeTime: string;
  profit: number;
}

export const HISTORY: ClosedTrade[] = (() => {
  const r = seeded(4242);
  const syms = ["XAUUSD", "EURUSD", "GBPUSD", "NAS100", "BTCUSD", "USDJPY", "US30", "USOIL", "ETHUSD", "TSLA", "GBPJPY", "AUDUSD"];
  const out: ClosedTrade[] = [];
  let t = Date.parse("2026-09-24T16:00:00Z");
  for (let i = 0; i < 420; i++) {
    const symbol = r.pick(syms);
    const inst = getInstrument(symbol);
    const side = r.bool(0.55) ? "buy" : "sell";
    const volume = +r.pick([0.01, 0.05, 0.1, 0.2, 0.5, 1, 1.5, 2]).toFixed(2);
    const openPrice = inst.price * (1 + r.normal() * 0.01);
    const win = r.bool(0.58);
    const move = Math.abs(r.normal()) * inst.price * (inst.assetClass === "forex" ? 0.0022 : 0.006);
    const closePrice = side === "buy" ? openPrice + (win ? move : -move * 1.05) : openPrice - (win ? move : -move * 1.05);
    let profit = (side === "buy" ? closePrice - openPrice : openPrice - closePrice) * volume * inst.contractSize;
    if (symbol.endsWith("JPY")) profit /= closePrice;
    const swap = -+(r.range(0, 4) * volume).toFixed(2);
    const commission = +(volume * 7 * (r.bool(0.3) ? 1 : 0)).toFixed(2);
    const closeTime = new Date(t).toISOString();
    const openTime = new Date(t - r.int(3, 60 * 30) * 60 * 1000).toISOString();
    out.push({
      ticket: String(49433000 - i * 13),
      login: r.pick(["80412337", "80412337", "80412512", "80413001"]),
      symbol, side, volume,
      openPrice: +openPrice.toFixed(inst.digits),
      closePrice: +closePrice.toFixed(inst.digits),
      swap, commission, openTime, closeTime,
      profit: +(profit + swap - commission).toFixed(2),
      source: r.pick(["manual", "manual", "manual", "copy", "api", "strategy"] as const),
    });
    t -= r.int(1, 14) * 3600 * 1000;
  }
  return out;
})();

/* ------------------------------------------------------------------ */
/* Equity series (daily)                                               */
/* ------------------------------------------------------------------ */

export function equitySeries(days = 365, end = 48915.6, seed = 9): { time: number; value: number; volume: number }[] {
  const r = seeded(seed);
  const out: { time: number; value: number; volume: number }[] = [];
  let v = end * 0.52;
  const now = Math.floor(Date.parse("2026-09-24T00:00:00Z") / 1000);
  for (let i = days - 1; i >= 0; i--) {
    v *= 1 + 0.0019 + r.normal() * 0.012;
    out.push({ time: now - i * 86400, value: v, volume: Math.round(r.range(4, 60)) });
  }
  const k = end / out[out.length - 1]!.value;
  return out.map((p) => ({ ...p, value: +(p.value * k).toFixed(2) }));
}

/** Daily realised P&L for the calendar heatmap. */
export function dailyPnl(year: number, month: number, seed = 3): { date: string; pnl: number; trades: number }[] {
  const r = seeded(seed + year * 12 + month);
  const days = new Date(Date.UTC(year, month + 1, 0)).getUTCDate();
  const out = [];
  for (let d = 1; d <= days; d++) {
    const dow = new Date(Date.UTC(year, month, d)).getUTCDay();
    const weekend = dow === 0 || dow === 6;
    const trades = weekend ? (r.bool(0.2) ? r.int(1, 3) : 0) : r.int(0, 14);
    const pnl = trades === 0 ? 0 : +(r.normal() * 420 + 120).toFixed(2);
    out.push({ date: `${year}-${String(month + 1).padStart(2, "0")}-${String(d).padStart(2, "0")}`, pnl, trades });
  }
  return out;
}

/* ------------------------------------------------------------------ */
/* News & calendar                                                     */
/* ------------------------------------------------------------------ */

export interface NewsItem {
  id: string;
  title: string;
  source: string;
  minutesAgo: number;
  symbols: string[];
  country: string;
  image: string;
  pinned?: boolean;
  sentiment: "bullish" | "bearish" | "neutral";
}

export const NEWS: NewsItem[] = [
  { id: "n1", title: "Gold holds near record as traders price in deeper Fed cuts", source: "Reuters", minutesAgo: 12, symbols: ["XAUUSD"], country: "us", image: "/assets/photos/gold.jpg", pinned: true, sentiment: "bullish" },
  { id: "n2", title: "ECB signals readiness to ease again as eurozone inflation cools", source: "Bloomberg", minutesAgo: 34, symbols: ["EURUSD", "GER40"], country: "de", image: "/assets/photos/finance.jpg", sentiment: "bearish" },
  { id: "n3", title: "Bitcoin reclaims $63K as ETF inflows hit a three-week high", source: "CoinDesk", minutesAgo: 51, symbols: ["BTCUSD", "ETHUSD"], country: "us", image: "/assets/photos/bitcoin.jpg", sentiment: "bullish" },
  { id: "n4", title: "Oil slides on demand worries despite Middle East supply risk", source: "Financial Times", minutesAgo: 78, symbols: ["USOIL", "UKOIL"], country: "sa", image: "/assets/photos/dubai.jpg", sentiment: "bearish" },
  { id: "n5", title: "Bank of Japan keeps rates steady, yen weakens past 149", source: "Nikkei Asia", minutesAgo: 102, symbols: ["USDJPY", "JP225"], country: "jp", image: "/assets/photos/skyline.jpg", sentiment: "neutral" },
  { id: "n6", title: "Nasdaq futures climb as chipmakers extend AI-driven rally", source: "CNBC", minutesAgo: 131, symbols: ["NAS100", "NVDA"], country: "us", image: "/assets/photos/nyc.jpg", sentiment: "bullish" },
  { id: "n7", title: "UK retail sales beat forecasts, sterling firms against dollar", source: "The Guardian", minutesAgo: 164, symbols: ["GBPUSD"], country: "gb", image: "/assets/photos/london.jpg", sentiment: "bullish" },
  { id: "n8", title: "RBI holds repo rate; rupee steady near 83.5 per dollar", source: "Mint", minutesAgo: 190, symbols: ["USDINR"], country: "in", image: "/assets/photos/money.jpg", sentiment: "neutral" },
  { id: "n9", title: "Singapore dollar edges up after MAS keeps policy unchanged", source: "Straits Times", minutesAgo: 222, symbols: ["USDSGD"], country: "sg", image: "/assets/photos/singapore.jpg", sentiment: "neutral" },
  { id: "n10", title: "Brazil real rallies as central bank hikes Selic by 25bp", source: "Valor", minutesAgo: 260, symbols: ["USDBRL"], country: "br", image: "/assets/photos/charts.jpg", sentiment: "bullish" },
];

export interface CalendarEvent {
  id: string;
  time: string; // HH:mm server time
  country: string;
  currency: string;
  title: string;
  impact: 1 | 2 | 3;
  actual?: string;
  forecast: string;
  previous: string;
}

export const CALENDAR: CalendarEvent[] = [
  { id: "c1", time: "11:00", country: "de", currency: "EUR", title: "Ifo Business Climate", impact: 2, actual: "85.4", forecast: "86.0", previous: "86.6" },
  { id: "c2", time: "15:30", country: "us", currency: "USD", title: "Non-Farm Payrolls (NFP)", impact: 3, forecast: "165K", previous: "142K" },
  { id: "c3", time: "15:30", country: "us", currency: "USD", title: "Unemployment Rate", impact: 3, forecast: "4.2%", previous: "4.2%" },
  { id: "c4", time: "17:00", country: "us", currency: "USD", title: "ISM Services PMI", impact: 2, forecast: "51.7", previous: "51.5" },
  { id: "c5", time: "17:30", country: "us", currency: "USD", title: "Crude Oil Inventories", impact: 2, forecast: "-1.4M", previous: "-1.6M" },
  { id: "c6", time: "21:00", country: "us", currency: "USD", title: "FOMC Member Speaks", impact: 1, forecast: "—", previous: "—" },
  { id: "c7", time: "02:30", country: "au", currency: "AUD", title: "CPI y/y", impact: 3, forecast: "2.7%", previous: "3.5%" },
];

/* ------------------------------------------------------------------ */
/* Dashboard aggregates                                                */
/* ------------------------------------------------------------------ */

export const DASHBOARD = {
  totalEquity: 48915.6,
  equityChangeToday: 1204.18,
  equityChangeTodayPct: 2.52,
  wallet: 13676.99,
  monthPnl: 6382.4,
  monthPnlPct: 14.9,
  earnings: { total: 2140.75, ib: 1488.2, copy: 402.55, pamm: 250 },
  profitShare: { lossPct: 31, profitPct: 69 },
};

export const NOTIFICATIONS = [
  { id: "no1", kind: "fill", title: "Buy 0.50 XAUUSD filled at 2,641.18", time: "2m", unread: true },
  { id: "no2", kind: "deposit", title: "Deposit 1,500.00 USDT confirmed (20/20)", time: "1h", unread: true },
  { id: "no3", kind: "kyc", title: "Your proof of address is under review", time: "3h", unread: true },
  { id: "no4", kind: "ib", title: "IB payout of $412.80 approved", time: "1d", unread: false },
  { id: "no5", kind: "margin", title: "Demo 90022904 margin level fell below 300%", time: "2d", unread: false },
];

export function topMovers(dir: "gainers" | "losers") {
  const sorted = [...INSTRUMENTS].sort((a, b) => b.change - a.change);
  return (dir === "gainers" ? sorted : sorted.reverse()).slice(0, 7);
}

/**
 * Mock data for the API & Algo module: API keys, usage, webhooks,
 * webhook deliveries, strategy marketplace and API reference.
 */
import { seeded, hashString } from "./rng";
import { PEOPLE, type Person } from "./people";
import { getInstrument, type AssetClass } from "./symbols";

/* ------------------------------------------------------------------ */
/* API keys                                                            */
/* ------------------------------------------------------------------ */

export type ApiScope = "read" | "trade";
export type ApiKeyStatus = "active" | "paused" | "expired";

export interface ApiKey {
  id: string;
  name: string;
  login: string;
  prefix: string; // public key id shown in UI
  scopes: ApiScope[];
  ips: string[];
  createdAt: string;
  lastUsed: string | null;
  lastIp?: string;
  expiresAt: string | null;
  status: ApiKeyStatus;
  rateLimit: number; // req/s
  requests24h: number;
  errors24h: number;
}

export const API_KEYS: ApiKey[] = [
  { id: "key_01", name: "Gold scalper bot", login: "80412512", prefix: "kk_live_EXAMPLE1", scopes: ["read", "trade"], ips: ["185.212.44.19", "185.212.44.20"], createdAt: "2026-06-14T09:12:00Z", lastUsed: "2026-09-24T14:58:12Z", lastIp: "185.212.44.19", expiresAt: "2027-06-14", status: "active", rateLimit: 20, requests24h: 84210, errors24h: 131 },
  { id: "key_02", name: "Portfolio dashboard (read-only)", login: "80412337", prefix: "kk_live_EXAMPLE2", scopes: ["read"], ips: [], createdAt: "2026-03-02T11:40:00Z", lastUsed: "2026-09-24T14:41:03Z", lastIp: "92.38.141.7", expiresAt: null, status: "active", rateLimit: 10, requests24h: 12884, errors24h: 4 },
  { id: "key_03", name: "NAS100 momentum — VPS Frankfurt", login: "80412337", prefix: "kk_live_EXAMPLE3", scopes: ["read", "trade"], ips: ["45.87.212.101"], createdAt: "2026-08-01T07:05:00Z", lastUsed: "2026-09-24T13:22:47Z", lastIp: "45.87.212.101", expiresAt: "2026-12-31", status: "active", rateLimit: 50, requests24h: 41276, errors24h: 212 },
  { id: "key_04", name: "Python research notebook", login: "90022871", prefix: "kk_demo_Z1ka3v", scopes: ["read", "trade"], ips: [], createdAt: "2026-09-19T16:30:00Z", lastUsed: "2026-09-23T21:10:55Z", lastIp: "103.21.58.4", expiresAt: "2026-10-19", status: "active", rateLimit: 10, requests24h: 2310, errors24h: 38 },
  { id: "key_05", name: "Cent grid EA bridge", login: "80413001", prefix: "kk_live_EXAMPLE4", scopes: ["read", "trade"], ips: ["172.105.9.66"], createdAt: "2026-02-20T10:00:00Z", lastUsed: "2026-09-12T08:02:19Z", lastIp: "172.105.9.66", expiresAt: "2026-12-01", status: "paused", rateLimit: 20, requests24h: 0, errors24h: 0 },
  { id: "key_06", name: "Old MT bridge (2025)", login: "80412512", prefix: "kk_live_EXAMPLE5", scopes: ["read"], ips: ["51.15.201.33"], createdAt: "2025-06-01T12:00:00Z", lastUsed: "2026-05-30T23:59:01Z", lastIp: "51.15.201.33", expiresAt: "2026-06-01", status: "expired", rateLimit: 10, requests24h: 0, errors24h: 0 },
];

export const API_STATS = {
  requests24h: 140680,
  requestsChangePct: 12.4,
  errorRate: 0.27,
  p50: 18,
  p99: 84,
  ordersViaApi24h: 1284,
  wsConnections: 7,
  rateLimited24h: 42,
};

/** Hourly (last 48h) or daily (last 30d) API request volume. */
export function apiUsage(mode: "hour" | "day"): { time: number; value: number; volume: number }[] {
  const r = seeded(mode === "hour" ? 811 : 977);
  const end = Date.parse("2026-09-24T15:00:00Z") / 1000;
  const n = mode === "hour" ? 48 : 30;
  const step = mode === "hour" ? 3600 : 86400;
  const out: { time: number; value: number; volume: number }[] = [];
  for (let i = n - 1; i >= 0; i--) {
    const t = end - i * step;
    let v: number;
    if (mode === "hour") {
      const h = new Date(t * 1000).getUTCHours();
      const session = h >= 6 && h <= 20 ? 1 : 0.45; // London/NY sessions busier
      v = Math.round((4200 + r.range(-600, 900)) * session + (h === 12 || h === 13 ? 1800 : 0));
    } else {
      const d = new Date(t * 1000).getUTCDay();
      const weekend = d === 0 || d === 6 ? 0.18 : 1;
      v = Math.round((118000 + (n - i) * 900 + r.range(-12000, 14000)) * weekend);
    }
    out.push({ time: t, value: v, volume: Math.round(v * r.range(0.004, 0.012)) });
  }
  return out;
}

export const ENDPOINT_USAGE = [
  { path: "GET /positions", share: 34, p50: 12 },
  { path: "GET /quotes/stream", share: 22, p50: 4 },
  { path: "POST /orders", share: 18, p50: 26 },
  { path: "GET /accounts/{login}", share: 14, p50: 9 },
  { path: "DELETE /orders/{id}", share: 7, p50: 21 },
  { path: "GET /history/deals", share: 5, p50: 48 },
];

export type OrderSource = "manual" | "api" | "webhook" | "strategy" | "copy";
export const ORDER_SOURCES: { source: OrderSource; label: string; count: number }[] = [
  { source: "manual", label: "Manual", count: 412 },
  { source: "api", label: "API", count: 1284 },
  { source: "webhook", label: "Webhook", count: 638 },
  { source: "strategy", label: "Strategy", count: 391 },
  { source: "copy", label: "Copy", count: 176 },
];

/* ------------------------------------------------------------------ */
/* Webhooks                                                            */
/* ------------------------------------------------------------------ */

export type SizingMode = "fixed" | "multiplier" | "risk";
export interface FanoutTarget {
  login: string;
  mode: SizingMode;
  value: number; // lots | x | % risk
  enabled: boolean;
}

export interface SignalWebhook {
  id: string;
  name: string;
  source: "TradingView" | "Custom";
  symbols: string[];
  url: string;
  secret: string;
  enabled: boolean;
  createdAt: string;
  lastSignal: string;
  signals24h: number;
  successRate: number;
  avgLatency: number;
  targets: FanoutTarget[];
}

export const WEBHOOKS: SignalWebhook[] = [
  {
    id: "wh_9f3a1c",
    name: "TradingView Gold breakout",
    source: "TradingView",
    symbols: ["XAUUSD"],
    url: "https://api.kalkstrade.com/algo/hooks/wh_9f3a1c7e2b",
    secret: "whsec_EXAMPLE000000000000001",
    enabled: true,
    createdAt: "2026-07-02T10:00:00Z",
    lastSignal: "2026-09-24T14:52:08Z",
    signals24h: 14,
    successRate: 99.1,
    avgLatency: 42,
    targets: [
      { login: "80412512", mode: "fixed", value: 0.5, enabled: true },
      { login: "80412337", mode: "risk", value: 1, enabled: true },
      { login: "90022871", mode: "multiplier", value: 2, enabled: false },
    ],
  },
  {
    id: "wh_2b77e0",
    name: "NAS100 momentum",
    source: "TradingView",
    symbols: ["NAS100", "US30"],
    url: "https://api.kalkstrade.com/algo/hooks/wh_2b77e04d91",
    secret: "whsec_EXAMPLE000000000000002",
    enabled: true,
    createdAt: "2026-08-11T08:30:00Z",
    lastSignal: "2026-09-24T13:31:44Z",
    signals24h: 6,
    successRate: 96.4,
    avgLatency: 51,
    targets: [
      { login: "80412337", mode: "multiplier", value: 1.5, enabled: true },
      { login: "80413001", mode: "fixed", value: 0.1, enabled: true },
    ],
  },
  {
    id: "wh_c410d8",
    name: "EURUSD mean-revert",
    source: "Custom",
    symbols: ["EURUSD", "GBPUSD"],
    url: "https://api.kalkstrade.com/algo/hooks/wh_c410d85a3f",
    secret: "whsec_EXAMPLE000000000000003",
    enabled: false,
    createdAt: "2026-05-19T12:10:00Z",
    lastSignal: "2026-09-19T09:04:12Z",
    signals24h: 0,
    successRate: 92.8,
    avgLatency: 47,
    targets: [{ login: "90022904", mode: "risk", value: 0.5, enabled: true }],
  },
];

export interface WebhookDelivery {
  id: string;
  webhookId: string;
  time: string;
  action: "buy" | "sell" | "close";
  symbol: string;
  status: 200 | 422 | 401 | 429;
  latency: number;
  filled: number;
  total: number;
  message: string;
}

export const WEBHOOK_DELIVERIES: WebhookDelivery[] = (() => {
  const r = seeded(4401);
  const out: WebhookDelivery[] = [];
  let t = Date.parse("2026-09-24T14:52:08Z");
  const errs: [WebhookDelivery["status"], string][] = [
    [422, "Invalid volume: 0.001 below min lot 0.01"],
    [422, "Market closed for symbol"],
    [401, "Signature mismatch (X-Kalks-Signature)"],
    [429, "Rate limit: 5 signals / 10s"],
  ];
  for (let i = 0; i < 42; i++) {
    const wh = WEBHOOKS[i % 7 === 3 ? 1 : i % 5 === 4 ? 2 : 0]!;
    const symbol = r.pick(wh.symbols);
    const total = wh.targets.filter((x) => x.enabled).length || 1;
    const bad = r.bool(0.1);
    const [status, message] = bad ? r.pick(errs) : ([200, "Accepted · orders routed"] as const);
    out.push({
      id: `dlv_${(hashString(wh.id + i) % 0xffffff).toString(16).padStart(6, "0")}`,
      webhookId: wh.id,
      time: new Date(t).toISOString(),
      action: r.pick(["buy", "sell", "buy", "sell", "close"] as const),
      symbol,
      status,
      latency: Math.round(bad ? r.range(8, 22) : r.range(28, 74)),
      filled: status === 200 ? (r.bool(0.9) ? total : Math.max(0, total - 1)) : 0,
      total,
      message,
    });
    t -= r.int(9, 140) * 60 * 1000;
  }
  return out;
})();

/* ------------------------------------------------------------------ */
/* Strategy marketplace                                                */
/* ------------------------------------------------------------------ */

export interface MarketStrategy {
  id: string;
  name: string;
  tagline: string;
  author: Person;
  symbols: string[];
  assetClass: AssetClass;
  returnPct: number; // 12m
  maxDD: number;
  winRate: number;
  subscribers: number;
  rating: number;
  reviews: number;
  price: number; // USD / month; 0 = free
  verified: boolean;
  months: number;
  tradesPerWeek: number;
  minDeposit: number;
  spark: number[];
  featured?: boolean;
  risk: "Low" | "Medium" | "High";
}

function curve(seed: number, ret: number, dd: number, n = 40) {
  const r = seeded(seed);
  const out: number[] = [];
  let v = 100;
  const drift = ret / 100 / n;
  for (let i = 0; i < n; i++) {
    v *= 1 + drift + r.normal() * (dd / 100) * 0.18;
    out.push(+v.toFixed(2));
  }
  return out;
}

const RAW_STRATS: Omit<MarketStrategy, "spark" | "author" | "id">[] = [
  { name: "Aurum Breakout Pro", tagline: "London-open gold breakouts with ATR trailing stops", symbols: ["XAUUSD"], assetClass: "metals", returnPct: 68.4, maxDD: 11.2, winRate: 58, subscribers: 2418, rating: 4.9, reviews: 312, price: 49, verified: true, months: 26, tradesPerWeek: 9, minDeposit: 1000, featured: true, risk: "Medium" },
  { name: "Nasdaq Momentum Rider", tagline: "Trend-following on US tech indices, flat by NY close", symbols: ["NAS100", "SPX500"], assetClass: "indices", returnPct: 42.1, maxDD: 9.8, winRate: 54, subscribers: 1382, rating: 4.7, reviews: 188, price: 29, verified: true, months: 19, tradesPerWeek: 12, minDeposit: 500, risk: "Medium" },
  { name: "EURUSD Mean Reversion", tagline: "Asian-range fades with tight 12-pip stops", symbols: ["EURUSD", "GBPUSD"], assetClass: "forex", returnPct: 18.6, maxDD: 4.1, winRate: 71, subscribers: 3904, rating: 4.6, reviews: 521, price: 0, verified: true, months: 31, tradesPerWeek: 22, minDeposit: 200, risk: "Low" },
  { name: "Crypto Swing Alpha", tagline: "4H swing structure on BTC and ETH with funding filter", symbols: ["BTCUSD", "ETHUSD"], assetClass: "crypto", returnPct: 91.3, maxDD: 24.6, winRate: 47, subscribers: 986, rating: 4.4, reviews: 97, price: 39, verified: false, months: 14, tradesPerWeek: 4, minDeposit: 1000, risk: "High" },
  { name: "Yen Carry Guard", tagline: "Carry harvesting with volatility-based hedges", symbols: ["USDJPY", "GBPJPY", "EURJPY"], assetClass: "forex", returnPct: 22.9, maxDD: 6.3, winRate: 63, subscribers: 1127, rating: 4.5, reviews: 140, price: 19, verified: true, months: 22, tradesPerWeek: 6, minDeposit: 500, risk: "Low" },
  { name: "Oil Inventory Scalper", tagline: "EIA-day volatility scalps on WTI and Brent", symbols: ["USOIL", "UKOIL"], assetClass: "energies", returnPct: 33.7, maxDD: 13.9, winRate: 52, subscribers: 402, rating: 4.2, reviews: 51, price: 0, verified: false, months: 11, tradesPerWeek: 5, minDeposit: 300, risk: "High" },
  { name: "Magnificent 7 Rotation", tagline: "Weekly relative-strength rotation across US mega caps", symbols: ["NVDA", "AAPL", "META", "TSLA"], assetClass: "stocks", returnPct: 38.2, maxDD: 12.4, winRate: 56, subscribers: 758, rating: 4.6, reviews: 83, price: 29, verified: true, months: 17, tradesPerWeek: 3, minDeposit: 2000, risk: "Medium" },
  { name: "Silver Grid Lite", tagline: "Low-frequency grid with hard equity stop", symbols: ["XAGUSD"], assetClass: "metals", returnPct: 15.4, maxDD: 5.2, winRate: 76, subscribers: 1560, rating: 4.3, reviews: 176, price: 0, verified: true, months: 20, tradesPerWeek: 15, minDeposit: 200, risk: "Low" },
  { name: "DAX Opening Drive", tagline: "First-hour Frankfurt momentum with fixed 1R targets", symbols: ["GER40", "UK100"], assetClass: "indices", returnPct: 27.5, maxDD: 8.7, winRate: 59, subscribers: 611, rating: 4.4, reviews: 64, price: 19, verified: true, months: 15, tradesPerWeek: 8, minDeposit: 500, risk: "Medium" },
];

const AUTHOR_IDX = [9, 2, 8, 17, 14, 5, 19, 12, 1];

export const MARKET_STRATEGIES: MarketStrategy[] = RAW_STRATS.map((s, i) => ({
  ...s,
  id: `st_${(hashString(s.name) % 0xfffff).toString(16)}`,
  author: PEOPLE[AUTHOR_IDX[i]!]!,
  spark: curve(hashString(s.name), s.returnPct, s.maxDD),
}));

export const MARKETPLACE_TERMS = { authorShare: 70, platformShare: 30, payoutDay: "1st of each month", minSubscribersForPaid: 0 };

export const MY_PUBLISHING = { published: 0, drafts: 1, draftName: "XAU London Range v2", estMonthly: 1860 };

/* ------------------------------------------------------------------ */
/* API reference                                                       */
/* ------------------------------------------------------------------ */

export type HttpMethod = "GET" | "POST" | "PUT" | "DELETE";
export interface ApiParam {
  name: string;
  in: "path" | "query" | "body" | "header";
  type: string;
  required: boolean;
  desc: string;
}
export interface ApiEndpoint {
  id: string;
  method: HttpMethod;
  path: string;
  title: string;
  desc: string;
  scope: ApiScope;
  params: ApiParam[];
  body?: Record<string, unknown>;
  response: unknown;
}

export const API_BASE = "https://api.kalkstrade.com/v1";

export const API_ENDPOINTS: Record<"accounts" | "orders" | "positions", ApiEndpoint[]> = {
  accounts: [
    {
      id: "list-accounts",
      method: "GET",
      path: "/accounts",
      title: "List trading accounts",
      desc: "Returns every trading account the key can access, with live balance, equity and margin.",
      scope: "read",
      params: [{ name: "type", in: "query", type: "string", required: false, desc: "Filter by live or demo" }],
      response: { data: [{ login: "80412337", type: "live", group: "Pro", currency: "USD", balance: 25000.0, equity: 26204.18, margin: 2041.1, leverage: 500 }] },
    },
    {
      id: "get-account",
      method: "GET",
      path: "/accounts/{login}",
      title: "Get account",
      desc: "Detailed snapshot of one account, including free margin, margin level and server.",
      scope: "read",
      params: [{ name: "login", in: "path", type: "string", required: true, desc: "Trading account login, e.g. 80412337" }],
      response: { login: "80412337", server: "Kalks-Live01", equity: 26204.18, free_margin: 24163.08, margin_level: 1283.8 },
    },
  ],
  orders: [
    {
      id: "create-order",
      method: "POST",
      path: "/orders",
      title: "Place an order",
      desc: "Places a market or pending order. Every order is tagged with source=api and your key id for reporting.",
      scope: "trade",
      params: [
        { name: "login", in: "body", type: "string", required: true, desc: "Target trading account" },
        { name: "symbol", in: "body", type: "string", required: true, desc: "Instrument, e.g. XAUUSD" },
        { name: "side", in: "body", type: "enum", required: true, desc: "buy | sell" },
        { name: "type", in: "body", type: "enum", required: true, desc: "market | limit | stop" },
        { name: "volume", in: "body", type: "number", required: true, desc: "Lots, min 0.01, step 0.01" },
        { name: "price", in: "body", type: "number", required: false, desc: "Required for limit / stop" },
        { name: "sl", in: "body", type: "number", required: false, desc: "Stop loss price" },
        { name: "tp", in: "body", type: "number", required: false, desc: "Take profit price" },
        { name: "client_id", in: "body", type: "string", required: false, desc: "Idempotency key, max 64 chars" },
      ],
      body: { login: "80412337", symbol: "XAUUSD", side: "buy", type: "market", volume: 0.5, sl: 2638.0, tp: 2690.0, client_id: "gold-bo-0924-01" },
      response: { id: "ord_8f21c0", ticket: 51298844, status: "filled", price: 2654.48, volume: 0.5, source: "api", filled_at: "2026-09-24T14:58:12.184Z" },
    },
    {
      id: "cancel-order",
      method: "DELETE",
      path: "/orders/{id}",
      title: "Cancel a pending order",
      desc: "Cancels a pending limit or stop order. Filled orders cannot be cancelled — close the position instead.",
      scope: "trade",
      params: [{ name: "id", in: "path", type: "string", required: true, desc: "Order id returned by POST /orders" }],
      response: { id: "ord_8f21c0", status: "cancelled" },
    },
  ],
  positions: [
    {
      id: "list-positions",
      method: "GET",
      path: "/positions",
      title: "List open positions",
      desc: "Open positions with floating P/L, updated on every tick. Filter by account or symbol.",
      scope: "read",
      params: [
        { name: "login", in: "query", type: "string", required: false, desc: "Account login" },
        { name: "symbol", in: "query", type: "string", required: false, desc: "Instrument filter" },
      ],
      response: { data: [{ ticket: 51298844, symbol: "XAUUSD", side: "buy", volume: 0.5, open_price: 2654.48, profit: 142.6, source: "api" }] },
    },
    {
      id: "close-position",
      method: "DELETE",
      path: "/positions/{ticket}",
      title: "Close a position",
      desc: "Closes a position fully, or partially when volume is passed.",
      scope: "trade",
      params: [
        { name: "ticket", in: "path", type: "integer", required: true, desc: "Position ticket" },
        { name: "volume", in: "query", type: "number", required: false, desc: "Partial close volume" },
      ],
      response: { ticket: 51298844, status: "closed", close_price: 2657.33, profit: 142.5 },
    },
  ],
};

export const RATE_LIMITS = [
  { scope: "REST · read", limit: "20 req/s per key", burst: "40", window: "1s sliding" },
  { scope: "REST · trade", limit: "10 orders/s per account", burst: "20", window: "1s sliding" },
  { scope: "WebSocket", limit: "5 connections per key", burst: "—", window: "—" },
  { scope: "WS subscriptions", limit: "200 symbols per connection", burst: "—", window: "—" },
  { scope: "Webhooks", limit: "5 signals / 10s per webhook", burst: "10", window: "10s fixed" },
  { scope: "FIX 4.4", limit: "50 msg/s per session", burst: "100", window: "1s sliding" },
];

export const API_ERRORS = [
  { code: 400, name: "bad_request", desc: "Malformed JSON or missing required field." },
  { code: 401, name: "unauthorized", desc: "Missing, expired or revoked API key, or bad signature." },
  { code: 403, name: "forbidden", desc: "Key lacks the scope (e.g. read-only key calling POST /orders) or IP not whitelisted." },
  { code: 404, name: "not_found", desc: "Account, order or position does not exist or is not visible to this key." },
  { code: 409, name: "duplicate_client_id", desc: "An order with this client_id was already accepted." },
  { code: 422, name: "invalid_order", desc: "Volume, price, SL/TP or market-hours validation failed." },
  { code: 423, name: "kill_switch_active", desc: "Trading disabled by the key or account kill switch." },
  { code: 429, name: "rate_limited", desc: "Too many requests. Respect the Retry-After header." },
  { code: 503, name: "market_closed", desc: "Trading session closed for the symbol." },
];

export const FIX_SESSION = {
  host: "fix.kalkstrade.com",
  port: 9880,
  senderCompId: "KLK_80412337",
  targetCompId: "KALKS",
  heartbeat: 30,
  version: "FIX.4.4",
  tls: "TLS 1.3 required",
  resetOnLogon: "Y",
  msgTypes: [
    ["A", "Logon"],
    ["0", "Heartbeat"],
    ["1", "Test Request"],
    ["5", "Logout"],
    ["D", "New Order Single"],
    ["F", "Order Cancel Request"],
    ["G", "Order Cancel/Replace"],
    ["8", "Execution Report"],
    ["V", "Market Data Request"],
    ["W", "MD Snapshot / Full Refresh"],
    ["AN", "Request For Positions"],
    ["AP", "Position Report"],
  ] as [string, string][],
};

export function instrumentDigits(symbol: string) {
  return getInstrument(symbol).digits;
}

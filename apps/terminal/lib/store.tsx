"use client";

import * as React from "react";
import { toast } from "@/lib/notify";
import { tr } from "@kalks/i18n/react";
import { ACCOUNTS, HISTORY, INSTRUMENTS, INSTRUMENT_MAP, IS_LIVE, POSITIONS, getInstrument, isMarketOpen, liveTradable, priceFeed, rebaseTrades, type Quote, type TradingAccount } from "@kalks/mock";
import { useQuotes } from "@kalks/ui";
import { startLiveFlags, syncRestricted, useMarketScope, visibleSymbol } from "@/lib/scope";
import {
  DEFAULT_SYMBOLS,
  PENDING_LABEL,
  pendingLabelKey,
  SEED_PENDING,
  SEED_POSITIONS_EXTRA,
  accCcy,
  accMoney,
  fmtPrice,
  fmtVol,
  marginRequired,
  nextTicket,
  pointSize,
  profitAt,
  profitUsd,
  roundPrice,
  serverTime,
  type ChartType,
  type Expiry,
  type OrderType,
  type PendingOrder,
  type TClosed,
  type TPosition,
  type Timeframe,
} from "./trading";
import { beep } from "./sound";
import { aiTrader } from "./ai-trader/runtime";
import { migrateIndicators, type IndicatorInstance } from "./indicators";
import { GUEST_ACCOUNT, GUEST_LOGIN, guestNotice } from "./guest";
import { engineApi } from "./engine/client";
import { engineActions } from "./engine/actions";
import { AccountStream } from "./engine/stream";
import { liveStore, useLiveEquity, useLivePosition } from "./engine/live";
import { mapAccount, mapHistory, mapOrder, mapPosition, rejectReason, serverName, type EngineTradingAccount } from "./engine/map";
import type { EngAccount, EngDeal, EngState, SessionInfo, StreamFrame } from "./engine/types";
import { routeOptionFrame, splitOptionState } from "./options/book";
import { AccountProductContext } from "./options/mode";
import { productOf } from "./options/product";
import { useModuleOn } from "@/components/modules";

/* ------------------------------------------------------------------ */
/* Types                                                               */
/* ------------------------------------------------------------------ */

export interface Session {
  login: string;
  investor: boolean;
  server: string;
  via: "sso" | "login" | "guest";
  at: number;
  /** Live builds: no trading account (real market data only, trade actions explain + link to sign-up). */
  guest?: boolean;
  /** Live builds: a trading-engine session (real account, orders go to the engine). */
  engine?: boolean;
}

/** The trading account the terminal last showed (live builds, several logins on one browser). */
export const ACTIVE_KEY = "kalks.terminal.active";
export function readActive(): string | null {
  try {
    return localStorage.getItem(ACTIVE_KEY);
  } catch {
    return null;
  }
}
export function writeActive(login: string | null) {
  try {
    if (login) localStorage.setItem(ACTIVE_KEY, login);
    else localStorage.removeItem(ACTIVE_KEY);
  } catch {
    /* storage blocked */
  }
}

/** Session for an engine login (live builds). */
export function engineSession(s: { login: string; readOnly: boolean; account: EngAccount | null }, via: Session["via"] = "login"): Session {
  return { login: s.login, investor: s.readOnly, server: serverName(s.account?.type ?? (s.login.startsWith("5") ? "demo" : "live")), via, at: Date.now(), engine: true };
}

/** The session a live build always starts with until the trading engine serves real accounts. */
export function guestSession(): Session {
  return { login: GUEST_LOGIN, investor: false, server: GUEST_ACCOUNT.server, via: "guest", at: Date.now(), guest: true };
}

const accountOf = (login: string): TradingAccount => ACCOUNTS.find((a) => a.login === login) ?? GUEST_ACCOUNT;

export interface Anchor {
  l: number; // logical bar index
  p: number; // price
}
export type Drawing =
  | { id: string; kind: "hline"; price: number }
  | { id: string; kind: "trend" | "rect" | "fib"; a: Anchor; b: Anchor };
export type DrawTool = "cursor" | "crosshair" | "hline" | "trend" | "fib" | "rect" | "text" | "ruler";

export interface ChartTab {
  id: string;
  symbol: string;
  tf: Timeframe;
  type: ChartType;
  /** Indicator instances (type + params + style). Old `IndicatorId[]` workspaces are migrated on load. */
  indicators: IndicatorInstance[];
  drawings: Drawing[];
}

export type Layout = "1" | "2h" | "2v" | "4";
export const LAYOUT_COUNT: Record<Layout, number> = { "1": 1, "2h": 2, "2v": 2, "4": 4 };

/** Activity-panel tabs. "trade" is the old combined positions + orders tab (saved workspaces), shown as "positions". */
export type ToolboxTab = "positions" | "pending" | "trade" | "history" | "exposure" | "news" | "calendar" | "alerts" | "journal" | "ai" | "mam" | "options" | "orders" | "closed" | "settlements";
export type RightTab = "order" | "depth" | "info";
export type MwTab = "symbols" | "details" | "favourites";
/** The right-hand column of the desktop terminal (docs/TERMINAL-DESIGN.md §2.2): instruments, the order book, or the Navigator. */
export type SideTab = "instruments" | "book" | "ticks" | "navigator";
/** Instrument list filter: an asset class, everything, or favourites (Market Watch, symbol search). */
export type Segment = "all" | "forex" | "metals" | "indices" | "energies" | "crypto" | "stocks" | "favourites";
/** Which chart engine renders chart tiles. "kalks" = the original lightweight-charts engine. */

export interface Workspace {
  layout: Layout;
  tabs: ChartTab[];
  slots: string[];
  activeId: string;
  /**
   * watch = the right-hand column (Instruments | Order book) is open. right / book / bookOpt / navigator are kept for
   * older saves and the phone layout; the desktop shows the order form as a popup and the book as a column tab.
   */
  panels: { watch: boolean; right: boolean; toolbox: boolean; navigator: boolean; book: boolean; bookOpt: boolean };
  /** which tab of the right-hand column shows */
  side: SideTab;
  /** Options mode: the options panel (chain, analytics, book) is open under the chart */
  optPanel: boolean;
  /** where positions / orders / history live: below the chart, reached by scrolling the page ("page"), or in a
   *  resizable panel under the chart on the first screen ("split") */
  posLayout: "page" | "split";
  rightTab: RightTab;
  toolboxTab: ToolboxTab;
  mwTab: MwTab;
  /** Market Watch / mobile watchlist segment */
  mwSegment: Segment;
  /** symbol search (Ctrl+K) segment */
  searchSegment: Segment;
  favourites: string[];
  hidden: string[];
  oneClick: boolean;
  sound: boolean;
  /** max slippage (points) from the price on screen when the order is sent; null = any price (market execution) */
  maxDeviation: number | null;
  lot: number; // default one-click lot
  profile: string;
  /** Workspace format: 3 = the chart-first shell (docs/TERMINAL-DESIGN.md). Older saves are migrated once on load. */
  uiVersion?: number;
}

export interface PriceAlert {
  id: string;
  symbol: string;
  cond: "above" | "below";
  price: number;
  note?: string;
  active: boolean;
  created: string;
  triggeredAt?: string;
}

export interface JournalLine {
  id: number;
  ts: number;
  src: "Trade" | "Terminal" | "Network" | "Alerts" | "Account" | "Experts";
  text: string;
  level?: "info" | "warn" | "error";
}

export interface OrderRequest {
  symbol: string;
  side: "buy" | "sell";
  type: OrderType;
  volume: number;
  price?: number;
  stopLimit?: number;
  sl?: number;
  tp?: number;
  trailing?: number; // price distance
  expiry?: Expiry;
  expiryDate?: string;
  comment?: string;
  ocoPrice?: number; // place an opposite-side twin at this price, linked OCO
  source?: TPosition["source"];
}

export interface NewOrderPrefill {
  symbol: string;
  side?: "buy" | "sell";
  type?: OrderType;
  price?: number;
}

interface Core {
  positions: TPosition[];
  pendings: PendingOrder[];
  history: TClosed[];
  balances: Record<string, number>; // USD
  refills: Record<string, number>;
  alerts: PriceAlert[];
  journal: JournalLine[];
}

/* ------------------------------------------------------------------ */
/* Defaults & persistence                                              */
/* ------------------------------------------------------------------ */

const WS_KEY = "kalks.terminal.workspace";
export const SESSION_KEY = "kalks.terminal.session";
export const SAVED_KEY = "kalks.terminal.saved";

const uid = () => Math.random().toString(36).slice(2, 9);

export function makeTab(symbol: string, tf: Timeframe = "H1", indicators: IndicatorInstance[] = []): ChartTab {
  return { id: uid(), symbol, tf, type: "candles", indicators, drawings: [] };
}

export function defaultWorkspace(): Workspace {
  const tabs = [
    makeTab("XAUUSD", "M15", migrateIndicators(["ema50", "sma20"])),
    makeTab("EURUSD", "H1", migrateIndicators(["bb"])),
    makeTab("NAS100", "M5", migrateIndicators(["rsi"])),
    makeTab("BTCUSD", "H4", migrateIndicators(["macd"])),
  ];
  return {
    layout: "1",
    tabs,
    slots: [tabs[0]!.id],
    activeId: tabs[0]!.id,
    // the Navigator duplicates the account switcher, the indicator list and the Close positions menu: off by default
    panels: { watch: true, right: true, toolbox: true, navigator: false, book: false, bookOpt: true },
    side: "instruments",
    optPanel: false,
    posLayout: "page",
    rightTab: "order",
    toolboxTab: "positions",
    mwTab: "symbols",
    mwSegment: "all",
    searchSegment: "all",
    favourites: ["XAUUSD", "EURUSD", "NAS100", "BTCUSD", "GBPUSD"],
    hidden: [],
    // new traders confirm every order; one-click (instant Sell / Buy) is a setting they switch on
    oneClick: false,
    sound: true,
    maxDeviation: null,
    lot: 0.5,
    profile: "Default",
    uiVersion: 3,
  };
}

/** Guest mode has no synthetic depth ladder: fall back to real panels. */
function guestSafe(w: Workspace, guest: boolean): Workspace {
  // news and calendar are live in every build (services/news); guests have no order panel for the DOM
  return { ...w, rightTab: guest && w.rightTab === "depth" ? "info" : w.rightTab };
}

function loadWorkspace(guest: boolean): Workspace {
  return guestSafe(readWorkspace(), guest);
}

/** The charts the saved layout shows (symbol + timeframe), for warming their history at start-up. */
export function savedCharts(): { symbol: string; tf: Timeframe }[] {
  const w = readWorkspace();
  return w.slots.map((id) => w.tabs.find((t) => t.id === id)).filter((t): t is ChartTab => !!t).map((t) => ({ symbol: t.symbol, tf: t.tf }));
}

function readWorkspace(): Workspace {
  const d = defaultWorkspace();
  try {
    const raw = localStorage.getItem(WS_KEY);
    if (!raw) return d;
    const w = { ...d, ...(JSON.parse(raw) as Partial<Workspace>) };
    w.panels = { ...d.panels, ...w.panels };
    if (!Array.isArray(w.tabs) || w.tabs.length === 0) return d;
    w.tabs = w.tabs.filter((t) => !!INSTRUMENT_MAP[t.symbol]).map((t) => ({ ...t, drawings: t.drawings ?? [], indicators: migrateIndicators(t.indicators) }));
    if (!w.tabs.length) return d;
    w.slots = (w.slots ?? []).filter((s) => w.tabs.some((t) => t.id === s));
    if (!w.slots.length) w.slots = [w.tabs[0]!.id];
    if (!w.tabs.some((t) => t.id === w.activeId)) w.activeId = w.slots[0]!;
    if ((w.uiVersion ?? 1) < 2) {
      // redesigned shell: positions and orders are separate tabs, favourites are a chip, the Navigator is optional
      if (w.toolboxTab === "trade") w.toolboxTab = "positions";
      if (w.mwTab === "favourites") {
        w.mwTab = "symbols";
        w.mwSegment = "favourites";
      }
      w.panels = { ...w.panels, navigator: false };
      w.uiVersion = 2;
    }
    if ((w.uiVersion ?? 1) < 3) {
      // chart-first shell: one right-hand column (Instruments | Order book), the order form is a popup
      w.panels = { ...w.panels, watch: true, toolbox: true };
      w.side = w.panels.navigator ? "navigator" : "instruments";
      w.optPanel = false;
      w.uiVersion = 3;
    }
    if (!["instruments", "book", "ticks", "navigator"].includes(w.side)) w.side = "instruments";
    if (w.posLayout !== "split") w.posLayout = "page";
    if (w.toolboxTab === "trade") w.toolboxTab = "positions";
    return fitSlots(w);
  } catch {
    return d;
  }
}

/** Make sure `slots` holds exactly as many charts as the layout shows. */
function fitSlots(w: Workspace): Workspace {
  const n = LAYOUT_COUNT[w.layout];
  let slots = w.slots.filter((s) => w.tabs.some((t) => t.id === s));
  let tabs = w.tabs;
  if (slots.length > n) {
    const keep = slots.includes(w.activeId) ? [w.activeId, ...slots.filter((s) => s !== w.activeId)] : slots;
    slots = keep.slice(0, n).sort((a, b) => w.slots.indexOf(a) - w.slots.indexOf(b));
  }
  while (slots.length < n) {
    const free = tabs.find((t) => !slots.includes(t.id));
    if (free) slots = [...slots, free.id];
    else {
      const used = new Set(tabs.map((t) => t.symbol));
      const sym = DEFAULT_SYMBOLS.find((s) => !used.has(s)) ?? "GBPUSD";
      const t = makeTab(sym, "H1");
      tabs = [...tabs, t];
      slots = [...slots, t.id];
    }
  }
  const activeId = slots.includes(w.activeId) ? w.activeId : slots[0]!;
  return { ...w, tabs, slots, activeId };
}

function emptyCore(): Core {
  return { positions: [], pendings: [], history: [], balances: {}, refills: {}, alerts: [], journal: [] };
}

function initialCore(): Core {
  const balances: Record<string, number> = {};
  const refills: Record<string, number> = {};
  for (const a of ACCOUNTS) {
    balances[a.login] = a.cent ? a.balance / 100 : a.balance;
    refills[a.login] = a.refillsLeft ?? 0;
  }
  const now = Date.now();
  return {
    positions: [...POSITIONS, ...rebaseTrades(SEED_POSITIONS_EXTRA)].map((p) => ({ ...p })),
    pendings: rebaseTrades(SEED_PENDING).map((p) => ({ ...p })),
    history: HISTORY.map((h) => ({ ...h })) as TClosed[],
    balances,
    refills,
    alerts: rebaseTrades([
      { id: uid(), symbol: "XAUUSD", cond: "above", price: 2670, note: "Retest of ATH", active: true, created: new Date(now - 3600e3 * 5).toISOString() },
      { id: uid(), symbol: "EURUSD", cond: "below", price: 1.08, note: "ECB week support", active: true, created: new Date(now - 3600e3 * 26).toISOString() },
      { id: uid(), symbol: "BTCUSD", cond: "above", price: 65000, active: false, created: new Date(now - 3600e3 * 50).toISOString(), triggeredAt: new Date(now - 3600e3 * 30).toISOString() },
    ] as PriceAlert[]),
    journal: [],
  };
}

export function readSession(): Session | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    return raw ? (JSON.parse(raw) as Session) : null;
  } catch {
    return null;
  }
}
export function writeSession(s: Session | null) {
  try {
    if (s) localStorage.setItem(SESSION_KEY, JSON.stringify(s));
    else localStorage.removeItem(SESSION_KEY);
  } catch {
    /* storage blocked */
  }
}

/* ------------------------------------------------------------------ */
/* Account metrics                                                     */
/* ------------------------------------------------------------------ */

export interface Metrics {
  balance: number;
  credit: number;
  equity: number;
  margin: number;
  free: number;
  level: number;
  floating: number;
}

export function computeMetrics(acc: TradingAccount, balance: number, positions: TPosition[], quotes: Record<string, Quote>): Metrics {
  let floating = 0;
  let margin = 0;
  for (const p of positions) {
    const q = quotes[p.symbol] ?? priceFeed().snapshot(p.symbol)!;
    floating += profitUsd(p, q.bid, q.ask);
    margin += marginRequired(p.symbol, p.volume, p.openPrice, acc.leverage);
  }
  const credit = acc.cent ? acc.credit / 100 : acc.credit;
  const equity = balance + credit + floating;
  return { balance, credit, equity, margin, free: equity - margin, level: margin > 0 ? (equity / margin) * 100 : Infinity, floating };
}

/* ------------------------------------------------------------------ */
/* Context                                                             */
/* ------------------------------------------------------------------ */

interface UiState {
  newOrder: NewOrderPrefill | null;
  positionDialog: string | null;
  pendingDialog: string | null;
  search: boolean;
  shortcuts: boolean;
  spec: string | null;
  about: boolean;
  /** Tools > Options */
  options: boolean;
  alertDialog: { symbol: string; price?: number } | null;
  /** live builds: log in to another trading account (kept in the account switcher) */
  loginDialog: boolean;
  /** first-run tour on screen (desktop) */
  tour: boolean;
  /** desktop "Full chart": the chart covers the window (top bar and side column hidden) */
  fullChart: boolean;
  /** Help › Trading terms explained */
  glossary: boolean;
}

interface Ctx {
  session: Session;
  /** Live build without a trading account: no positions/orders/history/balances; trade actions explain. */
  guest: boolean;
  /** Live build (real services only: no sample news, calendar or synthetic depth). */
  live: boolean;
  /** Connected to the trading engine: account, orders and positions are real. */
  engine: boolean;
  /** Engine mode: the account state has been loaded at least once. */
  synced: boolean;
  account: TradingAccount;
  accounts: TradingAccount[];
  readOnly: boolean;
  positions: TPosition[]; // current account
  pendings: PendingOrder[];
  history: TClosed[];
  allPositions: TPosition[];
  balances: Record<string, number>;
  refillsLeft: number;
  alerts: PriceAlert[];
  journal: JournalLine[];
  ws: Workspace;
  ui: UiState;
  drawTool: DrawTool;
  selectedDrawing: string | null;
  activeTab: ChartTab;
  activeSymbol: string;
  // workspace
  setWs: (patch: Partial<Workspace> | ((w: Workspace) => Partial<Workspace>)) => void;
  setLayout: (l: Layout) => void;
  addTab: (symbol?: string, tf?: Timeframe) => void;
  closeTab: (id: string) => void;
  activateTab: (id: string) => void;
  updateTab: (id: string, patch: Partial<ChartTab> | ((t: ChartTab) => Partial<ChartTab>)) => void;
  openSymbol: (symbol: string, newTab?: boolean) => void;
  togglePanel: (k: keyof Workspace["panels"], v?: boolean) => void;
  setDrawTool: (t: DrawTool) => void;
  selectDrawing: (id: string | null) => void;
  deleteSelectedDrawing: () => void;
  resetWorkspace: () => void;
  // ui
  setUi: (patch: Partial<UiState>) => void;
  openNewOrder: (p?: Partial<NewOrderPrefill>) => void;
  // trading (live builds: every action is a request to the trading engine; resolves when it answered)
  placeOrder: (o: OrderRequest) => Promise<boolean>;
  quickTrade: (symbol: string, side: "buy" | "sell", volume?: number) => void;
  closePosition: (ticket: string, volume?: number, reason?: string) => Promise<boolean>;
  modifyPosition: (ticket: string, patch: { sl?: number | null; tp?: number | null; trailing?: number | null }) => Promise<boolean>;
  closeBy: (a: string, b: string) => void;
  cancelPending: (ticket: string) => void;
  modifyPending: (ticket: string, patch: { price?: number; sl?: number | null; tp?: number | null }) => Promise<boolean>;
  bulkClose: (kind: "all" | "profit" | "loss" | "symbol" | "buys" | "sells", symbol?: string) => void;
  cancelAllPendings: () => void;
  // alerts & journal
  addAlert: (a: Omit<PriceAlert, "id" | "created" | "active">) => void;
  updateAlert: (id: string, patch: Partial<PriceAlert>) => void;
  removeAlert: (id: string) => void;
  log: (src: JournalLine["src"], text: string, level?: JournalLine["level"]) => void;
  clearJournal: () => void;
  // account
  switchAccount: (login: string) => void;
  refillDemo: () => void;
  logout: () => void;
  /** live builds: open the "log in to another account" dialog */
  openLogin: () => void;
  /** live builds: a login just succeeded in this browser (dialog): add it to the switcher and show it */
  accountAdded: (login: string) => Promise<void>;
}

const TerminalCtx = React.createContext<Ctx | null>(null);

export function useTerminal() {
  const c = React.useContext(TerminalCtx);
  if (!c) throw new Error("useTerminal outside TerminalProvider");
  return c;
}

/** Live metrics for the current (or given) account. */
export function useMetrics(login?: string): Metrics & { account: TradingAccount } {
  const t = useTerminal();
  const l = login ?? t.account.login;
  const live = useLiveEquity(t.engine ? l : null);
  const pos = t.allPositions.filter((p) => p.login === l);
  // engine accounts: the numbers come from equity frames; quotes are only needed to compute them locally
  const qs = useQuotes(!t.engine && pos.length ? [...new Set(pos.map((p) => p.symbol))] : []);
  if (t.engine) {
    // the engine's numbers (equity frames ≤ 4/s, else the last account view); cent accounts USC → USD
    const acc = (t.accounts.find((a) => a.login === l) ?? t.account) as EngineTradingAccount;
    if (live) return { balance: live.balance, credit: live.credit, equity: live.equity, margin: live.margin, free: live.freeMargin, level: live.marginLevel ?? Infinity, floating: live.profit + live.swap, account: acc };
    const k = acc.cent ? 100 : 1;
    const e = acc.engine;
    return { balance: acc.balance / k, credit: acc.credit / k, equity: acc.equity / k, margin: acc.margin / k, free: (e?.freeMargin ?? acc.equity - acc.margin) / k, level: e?.marginLevel ?? Infinity, floating: ((e?.profit ?? 0) + (e?.swap ?? 0)) / k, account: acc };
  }
  const acc = accountOf(l);
  return { ...computeMetrics(acc, t.balances[l] ?? 0, pos, qs), account: acc };
}

/** Floating profit (USD) of an open position: the engine's value in live builds, else computed from quotes. */
export function usePositionProfit(p: TPosition): number {
  const t = useTerminal();
  const q = useQuotes(t.engine ? [] : [p.symbol])[p.symbol];
  const live = useLivePosition(t.engine ? p.login : null, p.ticket);
  if (live) return live.profit;
  if (t.engine) return (p as TPosition & { profit?: number }).profit ?? 0;
  return q ? profitUsd(p, q.bid, q.ask) : 0;
}

/* ------------------------------------------------------------------ */
/* Provider                                                            */
/* ------------------------------------------------------------------ */

export function TerminalProvider({ initialSession, engineSessions, children, onLogout }: { initialSession: Session; engineSessions?: SessionInfo[]; children: React.ReactNode; onLogout: (to?: string) => void }) {
  const [session, setSession] = React.useState(initialSession);
  const guest = !!initialSession.guest;
  const engine = !!initialSession.engine;
  const [core, setCore] = React.useState<Core>(() => (guest || engine ? emptyCore() : initialCore()));
  // engine mode: every login held by this browser (account switcher) and its last account view
  const [engSessions, setEngSessions] = React.useState<{ login: string; readOnly: boolean }[]>(() => (engineSessions ?? []).map((x) => ({ login: x.login, readOnly: x.readOnly })));
  const [engAccounts, setEngAccounts] = React.useState<Record<string, EngineTradingAccount>>(() => Object.fromEntries((engineSessions ?? []).flatMap((x) => (x.account ? [[x.login, mapAccount(x.account)]] : []))));
  const engSessionsRef = React.useRef(engSessions);
  engSessionsRef.current = engSessions;
  const engAccRef = React.useRef(engAccounts);
  engAccRef.current = engAccounts;
  const [synced, setSynced] = React.useState(!engine);
  const streamRef = React.useRef<AccountStream | null>(null);
  const coreRef = React.useRef(core);
  const [ws, setWsState] = React.useState<Workspace>(() => loadWorkspace(guest));
  const wsRef = React.useRef(ws);
  wsRef.current = ws;
  const sessionRef = React.useRef(session);
  sessionRef.current = session;
  const [ui, setUiState] = React.useState<UiState>({ newOrder: null, positionDialog: null, pendingDialog: null, search: false, shortcuts: false, spec: null, about: false, options: false, alertDialog: null, loginDialog: false, tour: false, glossary: false, fullChart: false });
  const [drawTool, setDrawTool] = React.useState<DrawTool>("cursor");
  const [selectedDrawing, selectDrawing] = React.useState<string | null>(null);
  const jid = React.useRef(0);
  const guestBooted = React.useRef(false);

  const commit = React.useCallback((fn: (c: Core) => Core) => {
    const next = fn(coreRef.current);
    coreRef.current = next;
    setCore(next);
  }, []);

  const log = React.useCallback(
    (src: JournalLine["src"], text: string, level?: JournalLine["level"]) => {
      const line: JournalLine = { id: ++jid.current, ts: Date.now(), src, text, level };
      commit((c) => ({ ...c, journal: [...c.journal.slice(-499), line] }));
    },
    [commit],
  );

  const accountTypeRef = React.useRef<string>("demo");
  const account: TradingAccount = engine ? (engAccounts[session.login] ?? { ...GUEST_ACCOUNT, login: session.login, server: session.server, type: session.server === "Kalks-Demo" ? "demo" : "live" }) : accountOf(session.login);
  accountTypeRef.current = account.type;
  // live accounts and guests only see markets that trade live (lib/scope.ts); demo accounts see everything
  syncRestricted(guest || account.type === "live");
  const readOnly = session.investor;

  const notify = React.useCallback((kind: "fill" | "close" | "alert" | "error") => {
    if (wsRef.current.sound) beep(kind);
  }, []);

  // persist workspace
  React.useEffect(() => {
    try {
      localStorage.setItem(WS_KEY, JSON.stringify(ws));
    } catch {
      /* ignore */
    }
  }, [ws]);

  // boot journal
  React.useEffect(() => {
    if (guest) {
      // guest: only what really happened in this terminal (startup + market-data connection)
      const feed = priceFeed();
      let last = feed.mode;
      const report = () => {
        if (feed.mode === "live") log("Network", "market data: connected to the Kalks market-data stream");
        else if (feed.mode === "sim") log("Network", "market data: service unreachable, showing reference prices until it reconnects", "warn");
      };
      const off = feed.onMode(() => {
        if (feed.mode !== last) report();
        last = feed.mode;
      });
      if (guestBooted.current) return () => void off();
      guestBooted.current = true;
      log("Terminal", `Kalks Trader started · ${navigator.platform || "Web"}, ${INSTRUMENTS.length - feed.unavailable.size} symbols`);
      log("Terminal", "guest mode: charts and quotes only, no trading account connected");
      report();
      return () => void off();
    }
    if (engine) {
      log("Terminal", `Kalks Trader started · ${navigator.platform || "Web"}, ${INSTRUMENTS.length - priceFeed().unavailable.size} symbols, GMT+3 server time`);
      return;
    }
    const a = accountOf(initialSession.login);
    const n = coreRef.current.positions.filter((p) => p.login === a.login).length;
    const o = coreRef.current.pendings.filter((p) => p.login === a.login).length;
    log("Terminal", "Kalks Trader x64 build 5120 started for Kalks Global Markets Ltd");
    log("Terminal", `${navigator.platform || "Web"}, ${navigator.hardwareConcurrency ?? 8} cores, ${INSTRUMENTS.length} symbols, GMT+3 server time`);
    log("Network", `'${a.login}': authorized on ${a.server} through Access Point EU Frankfurt (ping 38.2 ms)${initialSession.investor ? ", investor mode (read-only)" : ""}`);
    log("Network", `'${a.login}': terminal synchronized with Kalks Global: ${n} positions, ${o} orders, ${INSTRUMENTS.length} symbols, 0 spreads`);
    log("Trade", `'${a.login}': ${a.mode} account, leverage 1:${a.leverage}, ${accCcy(a)}`);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /* ------------------------------ fills ------------------------------ */

  const addHistory = (c: Core, p: TPosition, volume: number, price: number, reason: string): Core => {
    const share = volume / p.volume;
    const profit = profitAt({ ...p, volume }, price) + p.swap * share - p.commission * share;
    const closed: TClosed = {
      ...p,
      volume,
      swap: +(p.swap * share).toFixed(2),
      commission: +(p.commission * share).toFixed(2),
      closePrice: price,
      closeTime: new Date().toISOString(),
      profit: +profit.toFixed(2),
      reason,
    };
    return { ...c, history: [closed, ...c.history], balances: { ...c.balances, [p.login]: (c.balances[p.login] ?? 0) + profit } };
  };

  /** Execute a fill for `login`, honouring hedging/netting. Returns a journal-friendly description. */
  const fill = (c: Core, login: string, r: { symbol: string; side: "buy" | "sell"; volume: number; price: number; sl?: number; tp?: number; trailing?: number; comment?: string; source: TPosition["source"] }): { core: Core; ticket: string } => {
    const acc = accountOf(login);
    const commission = acc.group === "ECN" ? +(3.5 * r.volume).toFixed(2) : 0;
    const make = (volume: number): TPosition => ({
      ticket: nextTicket(),
      login,
      symbol: r.symbol,
      side: r.side,
      volume: +volume.toFixed(2),
      openPrice: r.price,
      sl: r.sl,
      tp: r.tp,
      trailing: r.trailing,
      swap: 0,
      commission,
      openTime: new Date().toISOString(),
      source: r.source,
      comment: r.comment,
    });
    if (acc.mode === "hedging") {
      const p = make(r.volume);
      return { core: { ...c, positions: [...c.positions, p] }, ticket: p.ticket };
    }
    // netting: one position per symbol
    const ex = c.positions.find((p) => p.login === login && p.symbol === r.symbol);
    if (!ex) {
      const p = make(r.volume);
      return { core: { ...c, positions: [...c.positions, p] }, ticket: p.ticket };
    }
    if (ex.side === r.side) {
      const vol = ex.volume + r.volume;
      const merged: TPosition = { ...ex, volume: +vol.toFixed(2), openPrice: roundPrice(r.symbol, (ex.openPrice * ex.volume + r.price * r.volume) / vol), sl: r.sl ?? ex.sl, tp: r.tp ?? ex.tp, commission: ex.commission + commission };
      return { core: { ...c, positions: c.positions.map((p) => (p.ticket === ex.ticket ? merged : p)) }, ticket: ex.ticket };
    }
    const closeVol = Math.min(ex.volume, r.volume);
    let next = addHistory(c, ex, closeVol, r.price, "netting");
    const remaining = +(ex.volume - r.volume).toFixed(2);
    if (remaining > 0) {
      const share = remaining / ex.volume;
      next = { ...next, positions: next.positions.map((p) => (p.ticket === ex.ticket ? { ...ex, volume: remaining, swap: ex.swap * share, commission: ex.commission * share } : p)) };
      return { core: next, ticket: ex.ticket };
    }
    next = { ...next, positions: next.positions.filter((p) => p.ticket !== ex.ticket) };
    if (remaining < 0) {
      const p = make(-remaining);
      next = { ...next, positions: [...next.positions, p] };
      return { core: next, ticket: p.ticket };
    }
    return { core: next, ticket: ex.ticket };
  };

  const validStops = (side: "buy" | "sell", ref: number, sl?: number, tp?: number) => {
    if (side === "buy") return (sl === undefined || sl < ref) && (tp === undefined || tp > ref);
    return (sl === undefined || sl > ref) && (tp === undefined || tp < ref);
  };

  // client-side reject reasons -> translation keys (the journal keeps the English reason)
  const REJECT_KEY: Record<string, string> = { "Invalid volume": "invalid_volume", "Not enough money": "no_money", "Invalid stops": "invalid_sl", "Invalid price": "invalid_price", "Invalid stop-limit price": "invalid_stop_limit", "Invalid OCO price": "invalid_oco_price" };
  const fail = (text: string, reason: string) => {
    log("Trade", `${text} failed [${reason}]`, "error");
    toast.error(REJECT_KEY[reason] ? tr.dyn(`order.reject.${REJECT_KEY[reason]}`, reason) : reason, { description: text });
    notify("error");
    return false;
  };

  /** MT5 behaviour: no new orders, modifications or closes while the symbol's session is closed. */
  const marketClosed = (text: string, quiet = false) => {
    log("Trade", `${text} failed [Market closed]`, "error");
    if (!quiet) {
      toast.error(tr("order.toast.marketIsClosed"), { description: text });
      notify("error");
    }
    return false;
  };

  const placeOrderMock = React.useCallback(
    (o: OrderRequest): boolean => {
      const s = sessionRef.current;
      if (s.guest) {
        if (o.source !== "ai") guestNotice(o.type === "market" ? tr(o.side === "buy" ? "order.guest.buying" : "order.guest.selling", { symbol: o.symbol }) : tr("order.guest.pendingOrder", { label: tr(pendingLabelKey({ side: o.side, type: o.type as PendingOrder["type"] })) }));
        return false;
      }
      if (s.investor) {
        toast.error(tr("order.toast.tradingDisabled"), { description: tr("order.toast.investorPassword") });
        return false;
      }
      const acc = accountOf(s.login);
      const q = priceFeed().snapshot(o.symbol)!;
      const inst = getInstrument(o.symbol);
      const vol = +o.volume.toFixed(2);
      const desc = o.type === "market" ? `market ${o.side} ${fmtVol(vol)} ${o.symbol}` : `${PENDING_LABEL({ side: o.side, type: o.type as PendingOrder["type"] })} ${fmtVol(vol)} ${o.symbol} at ${fmtPrice(o.symbol, o.price ?? 0)}`;
      if (!(vol >= 0.01)) return fail(desc, "Invalid volume");
      if (!isMarketOpen(o.symbol)) return marketClosed(`'${acc.login}': ${desc}`);

      // margin check
      const c0 = coreRef.current;
      const m = computeMetrics(acc, c0.balances[acc.login] ?? 0, c0.positions.filter((p) => p.login === acc.login), {});
      const need = marginRequired(o.symbol, vol, o.side === "buy" ? q.ask : q.bid, acc.leverage);
      if (o.type === "market" && need > m.free) return fail(desc, "Not enough money");

      if (o.type === "market") {
        const dev = wsRef.current.maxDeviation ?? 3;
        const slipPts = Math.floor(Math.random() * Math.min(dev, 3));
        const px = roundPrice(o.symbol, (o.side === "buy" ? q.ask : q.bid) + (o.side === "buy" ? 1 : -1) * slipPts * pointSize(o.symbol) * (Math.random() < 0.5 ? 1 : -1));
        if (!validStops(o.side, px, o.sl, o.tp)) return fail(desc, "Invalid stops");
        const { core, ticket } = fill(c0, acc.login, { symbol: o.symbol, side: o.side, volume: vol, price: px, sl: o.sl, tp: o.tp, trailing: o.trailing, comment: o.comment, source: o.source ?? "manual" });
        commit(() => core);
        log("Trade", `'${acc.login}': market ${o.side} ${fmtVol(vol)} ${o.symbol}${o.sl ? ` sl: ${fmtPrice(o.symbol, o.sl)}` : ""}${o.tp ? ` tp: ${fmtPrice(o.symbol, o.tp)}` : ""} (deviation ${dev})`);
        log("Trade", `'${acc.login}': deal #${ticket} ${o.side} ${fmtVol(vol)} ${o.symbol} at ${fmtPrice(o.symbol, px)} done (based on order #${ticket})`);
        toast.success(tr(o.side === "buy" ? "order.toast.buyFilled" : "order.toast.sellFilled", { volume: fmtVol(vol), symbol: o.symbol }), { description: `${tr("order.toast.filledDesc", { ticket, price: fmtPrice(o.symbol, px), mode: tr.dyn(`order.mode.${acc.mode}`, acc.mode) })}${slipPts ? tr("order.toast.slippage", { n: slipPts }) : ""}` });
        notify("fill");
        return true;
      }

      const price = roundPrice(o.symbol, o.price ?? (o.side === "buy" ? q.ask : q.bid));
      const ref = o.side === "buy" ? q.ask : q.bid;
      const typeOk =
        o.type === "limit" ? (o.side === "buy" ? price < ref : price > ref) : o.side === "buy" ? price > ref : price < ref;
      if (!typeOk) return fail(desc, "Invalid price");
      if (o.type === "stop-limit" && o.stopLimit !== undefined && (o.side === "buy" ? o.stopLimit > price : o.stopLimit < price)) return fail(desc, "Invalid stop-limit price");
      if (!validStops(o.side, o.type === "stop-limit" ? (o.stopLimit ?? price) : price, o.sl, o.tp)) return fail(desc, "Invalid stops");
      const oco = o.ocoPrice !== undefined ? uid() : undefined;
      const base: PendingOrder = {
        ticket: nextTicket(),
        login: acc.login,
        symbol: o.symbol,
        side: o.side,
        type: o.type as PendingOrder["type"],
        volume: vol,
        price,
        stopLimit: o.type === "stop-limit" ? roundPrice(o.symbol, o.stopLimit ?? price) : undefined,
        sl: o.sl,
        tp: o.tp,
        trailing: o.trailing,
        expiry: o.expiry ?? "GTC",
        expiryDate: o.expiryDate,
        placed: new Date().toISOString(),
        source: o.source ?? "manual",
        comment: o.comment,
        oco,
      };
      const list = [base];
      if (o.ocoPrice !== undefined) {
        const twinSide = o.side === "buy" ? "sell" : "buy";
        const twinPrice = roundPrice(o.symbol, o.ocoPrice);
        const tref = twinSide === "buy" ? q.ask : q.bid;
        const ok = base.type === "limit" ? (twinSide === "buy" ? twinPrice < tref : twinPrice > tref) : twinSide === "buy" ? twinPrice > tref : twinPrice < tref;
        if (!ok) return fail(`OCO ${twinSide} ${base.type} at ${fmtPrice(o.symbol, twinPrice)}`, "Invalid OCO price");
        list.push({ ...base, ticket: nextTicket(), side: twinSide, price: twinPrice, sl: undefined, tp: undefined, stopLimit: undefined });
      }
      commit((c) => ({ ...c, pendings: [...c.pendings, ...list] }));
      for (const p of list) log("Trade", `'${acc.login}': accepted ${PENDING_LABEL(p)} ${fmtVol(p.volume)} ${p.symbol} at ${fmtPrice(p.symbol, p.price)}${p.oco ? " [OCO]" : ""} #${p.ticket}`);
      const placedLabel = tr(pendingLabelKey(base));
      toast.success(tr("order.toast.pendingPlaced", { label: placedLabel.charAt(0).toLocaleUpperCase() + placedLabel.slice(1) }), {
        description: tr("order.toast.pendingPlacedDesc", {
          volume: fmtVol(vol),
          symbol: o.symbol,
          price: fmtPrice(o.symbol, price),
          oco: list.length > 1 ? tr("order.toast.ocoTwin", { side: tr(`order.side.${list[1]!.side}`), price: fmtPrice(o.symbol, list[1]!.price) }) : "",
          expiry: base.expiry === "Date" ? base.expiryDate : base.expiry === "Today" ? tr("order.expiry.today") : tr("order.expiry.gtc"),
        }),
      });
      notify("fill");
      void inst;
      return true;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [commit, log, notify],
  );

  const closeInternal = (ticket: string, volume: number | undefined, reason: string, at?: number, silent = false) => {
    const c = coreRef.current;
    const p = c.positions.find((x) => x.ticket === ticket);
    if (!p) return;
    const q = priceFeed().snapshot(p.symbol)!;
    const price = at ?? (p.side === "buy" ? q.bid : q.ask);
    const vol = Math.min(p.volume, +(volume ?? p.volume).toFixed(2));
    let next = addHistory(c, p, vol, price, reason);
    const remaining = +(p.volume - vol).toFixed(2);
    if (remaining > 0) {
      const share = remaining / p.volume;
      next = { ...next, positions: next.positions.map((x) => (x.ticket === ticket ? { ...p, volume: remaining, swap: p.swap * share, commission: p.commission * share } : x)) };
    } else next = { ...next, positions: next.positions.filter((x) => x.ticket !== ticket) };
    commit(() => next);
    const profit = next.history[0]!.profit;
    const acc = accountOf(p.login);
    log("Trade", `'${p.login}': ${reason === "manual" ? "" : `${reason} triggered, `}deal #${nextTicket()} ${p.side === "buy" ? "sell" : "buy"} ${fmtVol(vol)} ${p.symbol} at ${fmtPrice(p.symbol, price)} done (close #${ticket}${remaining > 0 ? `, partial ${fmtVol(vol)} of ${fmtVol(p.volume)}` : ""}), profit ${accMoney(acc, profit, { signed: true })}`);
    if (!silent && p.login === sessionRef.current.login) {
      const title = reason === "sl" ? tr("order.toast.slHit", { symbol: p.symbol }) : reason === "tp" ? tr("order.toast.tpHit", { symbol: p.symbol }) : remaining > 0 ? tr("order.toast.closedPartial", { ticket, volume: fmtVol(vol) }) : tr("order.toast.closed", { ticket });
      (profit >= 0 ? toast.success : toast.error)(title, { description: tr("order.toast.closedDesc", { side: tr(`order.side.${p.side}`).toLocaleUpperCase(), volume: fmtVol(vol), symbol: p.symbol, price: fmtPrice(p.symbol, price), profit: `${accMoney(acc, profit, { signed: true })} ${accCcy(acc)}` }) });
      notify("close");
    }
  };

  const closePositionMock = React.useCallback(
    (ticket: string, volume?: number, reason = "manual") => {
      if (sessionRef.current.guest) return;
      if (sessionRef.current.investor) return void toast.error(tr("order.toast.readOnly"));
      const p = coreRef.current.positions.find((x) => x.ticket === ticket);
      if (p && !isMarketOpen(p.symbol)) return void marketClosed(`'${p.login}': close #${ticket} ${p.side} ${fmtVol(volume ?? p.volume)} ${p.symbol}`);
      closeInternal(ticket, volume, reason);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const modifyPositionMock = React.useCallback(
    (ticket: string, patch: { sl?: number | null; tp?: number | null; trailing?: number | null }) => {
      if (sessionRef.current.investor) return false;
      const p = coreRef.current.positions.find((x) => x.ticket === ticket);
      if (!p) return false;
      const q = priceFeed().snapshot(p.symbol)!;
      const sl = patch.sl === undefined ? p.sl : patch.sl === null ? undefined : roundPrice(p.symbol, patch.sl);
      const tp = patch.tp === undefined ? p.tp : patch.tp === null ? undefined : roundPrice(p.symbol, patch.tp);
      const trailing = patch.trailing === undefined ? p.trailing : patch.trailing === null ? undefined : patch.trailing;
      const text = `modify #${ticket} ${p.side} ${fmtVol(p.volume)} ${p.symbol} sl: ${sl ? fmtPrice(p.symbol, sl) : "0"}, tp: ${tp ? fmtPrice(p.symbol, tp) : "0"}`;
      if (!isMarketOpen(p.symbol)) return marketClosed(`'${p.login}': ${text}`);
      if (!validStops(p.side, p.side === "buy" ? q.bid : q.ask, sl, tp)) return fail(text, "Invalid stops");
      commit((c) => ({ ...c, positions: c.positions.map((x) => (x.ticket === ticket ? { ...x, sl, tp, trailing } : x)) }));
      log("Trade", `'${p.login}': ${text}${trailing ? `, trailing ${Math.round(trailing / pointSize(p.symbol))} pts` : ""} done`);
      toast.success(tr("order.toast.positionModified", { ticket }), { description: `${tr("order.toast.slTp", { sl: sl ? fmtPrice(p.symbol, sl) : "—", tp: tp ? fmtPrice(p.symbol, tp) : "—" })}${trailing ? tr("order.toast.trailingPts", { n: Math.round(trailing / pointSize(p.symbol)) }) : ""}` });
      return true;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [commit, log],
  );

  const closeByMock = React.useCallback(
    (a: string, b: string) => {
      const c = coreRef.current;
      const pa = c.positions.find((x) => x.ticket === a);
      const pb = c.positions.find((x) => x.ticket === b);
      if (!pa || !pb || pa.symbol !== pb.symbol || pa.side === pb.side) return void toast.error(tr("order.toast.closeByNeedsOpposite"));
      if (!isMarketOpen(pa.symbol)) return void marketClosed(`'${pa.login}': close position #${a} by position #${b} ${pa.symbol}`);
      const vol = Math.min(pa.volume, pb.volume);
      // pa closes at pb's open price, pb closes at its own open price (zero gross), saving one spread
      closeInternal(a, vol, "close by", pb.openPrice, true);
      closeInternal(b, vol, "close by", pb.openPrice, true);
      const acc = accountOf(pa.login);
      const gross = profitAt({ ...pa, volume: vol }, pb.openPrice);
      log("Trade", `'${pa.login}': close position #${a} ${pa.side} ${fmtVol(vol)} ${pa.symbol} by position #${b} ${pb.side} ${fmtVol(vol)} ${pb.symbol} done`);
      toast.success(tr("order.toast.closedBy", { a, b }), { description: tr("order.toast.closedByProfitDesc", { volume: fmtVol(vol), symbol: pa.symbol, profit: `${accMoney(acc, gross, { signed: true })} ${accCcy(acc)}` }) });
      notify("close");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [log, notify],
  );

  const cancelPendingMock = React.useCallback(
    (ticket: string) => {
      if (sessionRef.current.investor) return;
      const o = coreRef.current.pendings.find((x) => x.ticket === ticket);
      if (!o) return;
      commit((c) => ({ ...c, pendings: c.pendings.filter((x) => x.ticket !== ticket) }));
      log("Trade", `'${o.login}': cancel order #${ticket} ${PENDING_LABEL(o)} ${fmtVol(o.volume)} ${o.symbol} at ${fmtPrice(o.symbol, o.price)} done`);
      toast(tr("order.toast.orderCancelled", { ticket }), { description: tr("order.toast.orderDesc", { label: tr(pendingLabelKey(o)), volume: fmtVol(o.volume), symbol: o.symbol }) });
    },
    [commit, log],
  );

  const modifyPendingMock = React.useCallback(
    (ticket: string, patch: { price?: number; sl?: number | null; tp?: number | null }) => {
      if (sessionRef.current.investor) return false;
      const o = coreRef.current.pendings.find((x) => x.ticket === ticket);
      if (!o) return false;
      const q = priceFeed().snapshot(o.symbol)!;
      const price = patch.price !== undefined ? roundPrice(o.symbol, patch.price) : o.price;
      const sl = patch.sl === undefined ? o.sl : patch.sl === null ? undefined : roundPrice(o.symbol, patch.sl);
      const tp = patch.tp === undefined ? o.tp : patch.tp === null ? undefined : roundPrice(o.symbol, patch.tp);
      const ref = o.side === "buy" ? q.ask : q.bid;
      const text = `modify order #${ticket} ${PENDING_LABEL(o)} ${fmtVol(o.volume)} ${o.symbol} at ${fmtPrice(o.symbol, price)}`;
      if (!isMarketOpen(o.symbol)) return marketClosed(`'${o.login}': ${text}`);
      const typeOk = o.type === "limit" ? (o.side === "buy" ? price < ref : price > ref) : o.side === "buy" ? price > ref : price < ref;
      if (!typeOk) return fail(text, "Invalid price");
      if (!validStops(o.side, price, sl, tp)) return fail(text, "Invalid stops");
      commit((c) => ({ ...c, pendings: c.pendings.map((x) => (x.ticket === ticket ? { ...x, price, sl, tp } : x)) }));
      log("Trade", `'${o.login}': ${text} done`);
      toast.success(tr("order.toast.orderModified", { ticket }), { description: tr("order.toast.orderAtDesc", { label: tr(pendingLabelKey(o)), price: fmtPrice(o.symbol, price) }) });
      return true;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [commit, log],
  );

  const bulkCloseMock = React.useCallback(
    (kind: "all" | "profit" | "loss" | "symbol" | "buys" | "sells", symbol?: string) => {
      if (sessionRef.current.investor) return;
      const login = sessionRef.current.login;
      const list = coreRef.current.positions.filter((p) => {
        if (p.login !== login) return false;
        const q = priceFeed().snapshot(p.symbol)!;
        const pr = profitUsd(p, q.bid, q.ask);
        if (kind === "profit") return pr > 0;
        if (kind === "loss") return pr < 0;
        if (kind === "symbol") return p.symbol === symbol;
        if (kind === "buys") return p.side === "buy";
        if (kind === "sells") return p.side === "sell";
        return true;
      });
      if (!list.length) return void toast(tr("order.toast.nothingToClose"), { description: tr("order.toast.noPositionsMatch") });
      // positions on closed markets stay open (MT5 rejects them with "Market closed")
      const open = list.filter((p) => isMarketOpen(p.symbol));
      const blocked = list.filter((p) => !isMarketOpen(p.symbol));
      for (const p of blocked) marketClosed(`'${login}': close #${p.ticket} ${p.side} ${fmtVol(p.volume)} ${p.symbol}`, true);
      const blockedSyms = [...new Set(blocked.map((p) => p.symbol))].join(", ");
      if (!open.length) {
        toast.error(tr("order.toast.marketIsClosed"), { description: tr("order.toast.marketClosedPositions", { count: blocked.length, symbols: blockedSyms }) });
        return void notify("error");
      }
      const before = coreRef.current.balances[login] ?? 0;
      for (const p of open) closeInternal(p.ticket, undefined, "manual", undefined, true);
      const realised = (coreRef.current.balances[login] ?? 0) - before;
      const acc = accountOf(login);
      log("Trade", `'${login}': bulk close (${kind}${symbol ? ` ${symbol}` : ""}): ${open.length} positions, profit ${accMoney(acc, realised, { signed: true })}${blocked.length ? `, ${blocked.length} skipped (market closed)` : ""}`);
      (realised >= 0 ? toast.success : toast.error)(tr("order.toast.closedCount", { count: open.length }), {
        description: `${tr("order.toast.realised", { amount: `${accMoney(acc, realised, { signed: true })} ${accCcy(acc)}` })}${blocked.length ? tr("order.toast.leftOpen", { count: blocked.length, symbols: blockedSyms }) : ""}`,
      });
      notify("close");
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [log, notify],
  );

  const cancelAllPendingsMock = React.useCallback(() => {
    if (sessionRef.current.investor) return;
    const login = sessionRef.current.login;
    const n = coreRef.current.pendings.filter((p) => p.login === login).length;
    if (!n) return void toast(tr("order.toast.noPendingOrders"));
    commit((c) => ({ ...c, pendings: c.pendings.filter((p) => p.login !== login) }));
    log("Trade", `'${login}': ${n} pending orders cancelled`);
    toast(tr("order.toast.cancelledPending", { count: n }));
  }, [commit, log]);

  /* ------------------------------ trading engine (live builds) ------------------------------ */

  const expiredRef = React.useRef<(login: string) => void>(() => {});

  /** Replace the active account's book with an engine state (initial load, reconnect, stream `resync`). */
  const applyState = React.useCallback(
    (login: string, full: EngState) => {
      const st = splitOptionState(login, full); // option positions/orders live in lib/options/book.ts
      const acc = mapAccount(st.account);
      setEngAccounts((m) => ({ ...m, [login]: acc }));
      liveStore.setAccount(login, st.account);
      commit((c) => ({
        ...c,
        positions: st.positions.map((p) => mapPosition(p, acc.cent)),
        pendings: st.orders.map(mapOrder),
        history: mapHistory(st.history.deals, acc.cent),
      }));
      if (sessionRef.current.login === login && sessionRef.current.investor !== st.readOnly) setSession((s) => ({ ...s, investor: st.readOnly }));
    },
    [commit],
  );

  const loadState = React.useCallback(
    async (login: string) => {
      const r = await engineApi.state(login, 300);
      if (sessionRef.current.login !== login) return false;
      if (!r.ok) {
        if (r.err.status === 401) expiredRef.current(login);
        else log("Network", `'${login}': synchronization with ${sessionRef.current.server} failed [${rejectReason(r.err)}]`, "error");
        return false;
      }
      applyState(login, r.data);
      return r.data;
    },
    [applyState, log],
  );

  /** Stream notifications: server-side events (SL/TP, pending fills, margin call, stop out…) become toasts. */
  const onNotice = React.useCallback(
    (login: string, kind: string, message: string, data?: Record<string, unknown>) => {
      const who = `'${login}': `;
      switch (kind) {
        case "correction": {
          log("Trade", `${who}${message}`, "warn");
          // an options order-book fill the dealing desk cancelled (bust): the trade was reversed, the fee refunded
          if (data?.options !== true && data?.bust !== true) return;
          const m = /^([A-Z0-9]{3,12})-\d{8}-([0-9.]+)-([CP])/.exec(String(data?.series ?? ""));
          const what = m ? `${m[1]} ${m[2]} ${m[3] === "C" ? tr("trader.opt.call") : tr("trader.opt.put")}` : String(data?.series ?? "");
          toast.warning(tr("trader.opt.bust.title"), { description: tr("trader.opt.bust.text", { what, n: String(data?.contracts ?? "") }), duration: 15_000 });
          return notify("alert");
        }
        case "fill":
        case "close":
        case "close_by":
          return; // this terminal's own requests are journaled by the action; deals arrive as frames
        case "sl":
        case "tp":
          log("Trade", `${who}${message}`);
          (kind === "tp" ? toast.success : toast.error)(kind === "sl" ? tr("order.toast.slTriggered") : tr("order.toast.tpTriggered"), { description: message });
          return notify("close");
        case "order_triggered":
        case "order_filled":
          log("Trade", `${who}${message}`);
          toast.success(kind === "order_filled" ? tr("order.toast.pendingFilled") : tr("order.toast.pendingTriggered"), { description: message });
          return notify("fill");
        case "order_rejected":
        case "order_expired":
          log("Trade", `${who}${message}`, "warn");
          toast.warning(kind === "order_expired" ? tr("order.toast.pendingExpired") : tr("order.toast.pendingRejected"), { description: message });
          return notify("error");
        case "margin_call":
          log("Account", `${who}${message}`, "warn");
          toast.warning(tr("order.toast.marginCall"), { description: message, duration: 12_000 });
          return notify("alert");
        case "stop_out":
          log("Account", `${who}${message}`, "error");
          toast.error(tr("order.toast.stopOut"), { description: message, duration: 15_000 });
          return notify("error");
        case "balance":
          log("Account", `${who}${message}`);
          return void toast(tr("order.toast.balanceOperation"), { description: message });
        default:
          log(kind === "order_cancelled" ? "Trade" : "Account", `${who}${message}`);
      }
    },
    [log, notify],
  );

  const onFrame = React.useCallback(
    (login: string, frame: StreamFrame, reconnected: boolean) => {
      if (sessionRef.current.login !== login) return;
      const cent = engAccRef.current[login]?.cent ?? false;
      const f = routeOptionFrame(login, frame, cent); // option entries go to the option book
      if (!f) return;
      switch (f.type) {
        case "snapshot": {
          const acc = mapAccount(f.account);
          setEngAccounts((m) => ({ ...m, [login]: acc }));
          liveStore.setAccount(login, f.account);
          commit((c) => ({ ...c, positions: f.positions.map((p) => mapPosition(p, acc.cent)), pendings: f.orders.map(mapOrder) }));
          if (sessionRef.current.investor !== f.readOnly) setSession((s) => ({ ...s, investor: f.readOnly }));
          // deals closed while the socket was down only come with the full state
          if (reconnected) void loadState(login);
          return;
        }
        case "position": {
          if (f.op === "remove") return commit((c) => ({ ...c, positions: c.positions.filter((p) => p.ticket !== String(f.ticket)) }));
          const p = mapPosition(f.position, cent);
          return commit((c) => (c.positions.some((x) => x.ticket === p.ticket) ? { ...c, positions: c.positions.map((x) => (x.ticket === p.ticket ? p : x)) } : { ...c, positions: [...c.positions, p] }));
        }
        case "order": {
          if (f.op === "remove") return commit((c) => ({ ...c, pendings: c.pendings.filter((o) => o.ticket !== String(f.ticket)) }));
          const o = mapOrder(f.order);
          return commit((c) => (c.pendings.some((x) => x.ticket === o.ticket) ? { ...c, pendings: c.pendings.map((x) => (x.ticket === o.ticket ? o : x)) } : { ...c, pendings: [...c.pendings, o] }));
        }
        case "deal": {
          const d = f.deal;
          if (d.entry === "in") return;
          // the position (still at its pre-close volume) carries the entry commission for the share
          const pos = coreRef.current.positions.find((p) => p.ticket === String(d.positionTicket));
          const entry: EngDeal[] = pos ? [{ ...d, id: -1, entry: "in", volume: pos.volume, commission: pos.commission * (cent ? 100 : 1), swap: 0, profit: 0 }] : [];
          const rows = mapHistory([d, ...entry], cent);
          return commit((c) => ({ ...c, history: [...rows, ...c.history.filter((h) => (h as { deal?: string }).deal !== String(d.id))] }));
        }
        case "account": {
          const acc = mapAccount(f.account);
          setEngAccounts((m) => ({ ...m, [login]: acc }));
          return liveStore.setAccount(login, f.account);
        }
        case "equity":
          return liveStore.setEquity(login, f, cent);
        case "notification":
          return onNotice(login, f.kind, f.message, f.data);
        case "ledger":
          if (!["trade_pnl", "commission", "swap"].includes(f.txn.kind)) log("Account", `'${login}': ${f.txn.kind.replace(/_/g, " ")} ${f.txn.amount >= 0 ? "+" : ""}${(cent ? f.txn.amount / 100 : f.txn.amount).toFixed(2)}`);
          return;
        case "resync":
          log("Network", `'${login}': terminal fell behind the account stream, resynchronizing`, "warn");
          void loadState(login);
          return streamRef.current?.reconnect();
      }
    },
    [commit, loadState, log, onNotice],
  );

  // active login: load the state, then keep it live over the account stream
  React.useEffect(() => {
    if (!engine) return;
    const login = session.login;
    setSynced(false);
    commit((c) => ({ ...c, positions: [], pendings: [], history: [] }));
    const g = engAccRef.current[login]?.engine.spreadGroup;
    if (g) priceFeed().setGroup(g); // quotes carry this account group's spread (the prices the engine fills at)
    let snaps = 0;
    let lost = false;
    void loadState(login).then((st) => {
      if (!st) return;
      setSynced(true);
      const a = mapAccount(st.account);
      priceFeed().setGroup(a.engine.spreadGroup);
      log("Network", `'${login}': authorized on ${a.server}${st.readOnly ? ", investor mode (read-only)" : ""}`);
      log("Network", `'${login}': terminal synchronized with Kalks: ${st.positions.length} positions, ${st.orders.length} orders, ${st.history.deals.length} recent deals`);
      log("Trade", `'${login}': ${a.mode} account, ${a.group}, leverage 1:${a.leverage}, ${a.currency}`);
    });
    const stream = new AccountStream(login, {
      onFrame: (f) => {
        if (f.type === "snapshot") snaps++;
        onFrame(login, f, f.type === "snapshot" && snaps > 1);
      },
      onStatus: (st, info) => {
        liveStore.setStatus(st, info?.attempt ?? 0, info?.delayMs);
        if (st === "open") {
          log("Network", `'${login}': ${lost ? "connection to the trade server restored" : `account stream connected (${sessionRef.current.server})`}`);
          lost = false;
        } else if (st === "reconnecting" && !lost) {
          lost = true;
          log("Network", `'${login}': connection to the trade server lost${info?.reason ? ` (${info.reason})` : ""}, reconnecting`, "warn");
        }
      },
      onUnauthorized: () => expiredRef.current(login),
    });
    streamRef.current = stream;
    // dev-only handle for the E2E reconnect test (window.__kalksStream.kill())
    if (process.env.NODE_ENV !== "production") (window as unknown as { __kalksStream?: AccountStream }).__kalksStream = stream;
    return () => {
      stream.stop();
      if (streamRef.current === stream) streamRef.current = null;
      liveStore.clear(login);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, session.login]);

  const eng = React.useMemo(
    () =>
      engineActions({
        login: () => sessionRef.current.login,
        account: () => engAccRef.current[sessionRef.current.login],
        positions: () => coreRef.current.positions,
        pendings: () => coreRef.current.pendings,
        log,
        sound: notify,
        refreshIfStale: () => {
          if (streamRef.current?.status !== "open") void loadState(sessionRef.current.login);
        },
        deviation: () => wsRef.current.maxDeviation,
        expired: (login) => expiredRef.current(login),
      }),
    [log, notify, loadState],
  );

  /** Guest / investor gate shared by every trade action. */
  const blocked = (what: string, source?: string) => {
    const s = sessionRef.current;
    if (s.guest) {
      if (source !== "ai") guestNotice(what);
      return true;
    }
    if (s.investor) {
      if (source !== "ai") toast.error(tr("order.toast.tradingDisabled"), { description: tr("order.toast.investorPassword") });
      return true;
    }
    return false;
  };

  const placeOrder = React.useCallback(
    async (o: OrderRequest): Promise<boolean> => {
      // a market hidden from this account (live trading off) can't be opened here; the engine refuses it too
      // (symbol_demo_only). Closing and modifying stay allowed.
      if (accountTypeRef.current === "live" && !liveTradable(o.symbol)) {
        toast(tr("desk.trade.unavailable"));
        return false;
      }
      if (!engine) return placeOrderMock(o);
      if (blocked(o.type === "market" ? tr(o.side === "buy" ? "order.guest.buying" : "order.guest.selling", { symbol: o.symbol }) : tr("order.guest.pendingOrder", { label: tr(pendingLabelKey({ side: o.side, type: o.type as PendingOrder["type"] })) }), o.source)) return false;
      return (await eng.placeOrder(o)).ok;
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, placeOrderMock],
  );
  const quickTrade = React.useCallback(
    (symbol: string, side: "buy" | "sell", volume?: number) => void placeOrder({ symbol, side, type: "market", volume: volume ?? wsRef.current.lot }),
    [placeOrder],
  );
  const closePosition = React.useCallback(
    async (ticket: string, volume?: number, reason = "manual"): Promise<boolean> => {
      if (!engine) return (closePositionMock(ticket, volume, reason), true);
      if (blocked(tr("order.guest.closingPosition"))) return false;
      return eng.closePosition(ticket, volume, reason);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, closePositionMock],
  );
  const modifyPosition = React.useCallback(
    async (ticket: string, patch: { sl?: number | null; tp?: number | null; trailing?: number | null }): Promise<boolean> => {
      if (!engine) return modifyPositionMock(ticket, patch);
      if (blocked(tr("order.guest.modifyingPosition"))) return false;
      return eng.modifyPosition(ticket, patch);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, modifyPositionMock],
  );
  const closeBy = React.useCallback(
    (a: string, b: string) => {
      if (!engine) return closeByMock(a, b);
      if (!blocked(tr("order.guest.closeBy"))) void eng.closeBy(a, b);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, closeByMock],
  );
  const cancelPending = React.useCallback(
    (ticket: string) => {
      if (!engine) return cancelPendingMock(ticket);
      if (!blocked(tr("order.guest.deletingOrder"))) void eng.cancelPending(ticket);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, cancelPendingMock],
  );
  const modifyPending = React.useCallback(
    async (ticket: string, patch: { price?: number; sl?: number | null; tp?: number | null }): Promise<boolean> => {
      if (!engine) return modifyPendingMock(ticket, patch);
      if (blocked(tr("order.guest.modifyingOrder"))) return false;
      return eng.modifyPending(ticket, patch);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, modifyPendingMock],
  );
  const bulkClose = React.useCallback(
    (kind: "all" | "profit" | "loss" | "symbol" | "buys" | "sells", symbol?: string) => {
      if (!engine) return bulkCloseMock(kind, symbol);
      if (!blocked(tr("order.guest.closingPositions"))) void eng.bulkClose(kind, symbol);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [eng, bulkCloseMock],
  );
  const cancelAllPendings = React.useCallback(() => {
    if (!engine) return cancelAllPendingsMock();
    if (!blocked(tr("order.guest.deletingOrders"))) void eng.cancelAllPendings();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [eng, cancelAllPendingsMock]);

  /* ------------------------------ tick engine ------------------------------ */

  // streaming demand beyond the rows on screen (packages/mock prices.ts): favourites, the charts on screen, the
  // positions and orders, the order form's market
  const demandKey = React.useMemo(() => {
    const out = new Set<string>(ws.favourites);
    for (const t of ws.tabs) if (ws.slots.includes(t.id)) out.add(t.symbol);
    for (const x of [...core.positions, ...core.pendings]) out.add(x.symbol);
    if (ui.newOrder) out.add(ui.newOrder.symbol);
    return [...out].sort().join(",");
  }, [ws.favourites, ws.tabs, ws.slots, core.positions, core.pendings, ui.newOrder]);
  React.useEffect(() => {
    priceFeed().want("terminal", demandKey ? demandKey.split(",") : []);
  }, [demandKey]);

  // the core instruments always; catalogue markets while a position, order or alert needs their ticks
  const tickSymbols = React.useMemo(() => {
    const out = new Set(INSTRUMENTS.map((i) => i.symbol));
    for (const x of [...core.positions, ...core.pendings, ...core.alerts]) out.add(x.symbol);
    return [...out].join(",");
  }, [core.positions, core.pendings, core.alerts]);
  React.useEffect(() => {
    const feed = priceFeed();
    return feed.subscribe(
      tickSymbols.split(","),
      (q) => {
        const c = coreRef.current;
        // SL / TP / trailing / pending fills run in this tab only for the demo build's sample accounts;
        // live builds leave all of it to the trading engine (server-side)
        for (const p of engine ? [] : c.positions) {
          if (p.symbol !== q.symbol) continue;
          const px = p.side === "buy" ? q.bid : q.ask;
          if (p.sl !== undefined && (p.side === "buy" ? px <= p.sl : px >= p.sl)) {
            closeInternal(p.ticket, undefined, "sl", p.sl);
            continue;
          }
          if (p.tp !== undefined && (p.side === "buy" ? px >= p.tp : px <= p.tp)) {
            closeInternal(p.ticket, undefined, "tp", p.tp);
            continue;
          }
          if (p.trailing) {
            const cand = roundPrice(p.symbol, p.side === "buy" ? px - p.trailing : px + p.trailing);
            const inProfit = p.side === "buy" ? px - p.openPrice >= p.trailing : p.openPrice - px >= p.trailing;
            const better = p.sl === undefined || (p.side === "buy" ? cand > p.sl : cand < p.sl);
            if (inProfit && better) {
              commit((cc) => ({ ...cc, positions: cc.positions.map((x) => (x.ticket === p.ticket ? { ...x, sl: cand } : x)) }));
              log("Trade", `'${p.login}': trailing stop #${p.ticket} ${p.symbol} moved to ${fmtPrice(p.symbol, cand)}`);
            }
          }
        }
        // pending orders
        for (const o of engine ? [] : coreRef.current.pendings) {
          if (o.symbol !== q.symbol) continue;
          const ref = o.side === "buy" ? q.ask : q.bid;
          const hit = o.type === "limit" ? (o.side === "buy" ? ref <= o.price : ref >= o.price) : o.side === "buy" ? ref >= o.price : ref <= o.price;
          if (!hit) continue;
          if (o.type === "stop-limit") {
            const lim: PendingOrder = { ...o, type: "limit", price: o.stopLimit ?? o.price, stopLimit: undefined };
            commit((cc) => ({ ...cc, pendings: cc.pendings.map((x) => (x.ticket === o.ticket ? lim : x)) }));
            log("Trade", `'${o.login}': order #${o.ticket} ${o.side} stop limit ${o.symbol} activated, ${o.side} limit at ${fmtPrice(o.symbol, lim.price)} placed`);
            continue;
          }
          const px = o.type === "limit" ? o.price : ref;
          let cc = coreRef.current;
          cc = { ...cc, pendings: cc.pendings.filter((x) => x.ticket !== o.ticket && (!o.oco || x.oco !== o.oco)) };
          const { core, ticket } = fill(cc, o.login, { symbol: o.symbol, side: o.side, volume: o.volume, price: px, sl: o.sl, tp: o.tp, trailing: o.trailing, comment: o.comment, source: o.source });
          commit(() => core);
          log("Trade", `'${o.login}': order #${o.ticket} ${PENDING_LABEL(o)} ${fmtVol(o.volume)} ${o.symbol} at ${fmtPrice(o.symbol, o.price)} triggered, deal #${ticket} at ${fmtPrice(o.symbol, px)} done`);
          if (o.oco) log("Trade", `'${o.login}': OCO sibling of #${o.ticket} cancelled`);
          if (o.login === sessionRef.current.login) {
            toast.success(tr("order.toast.orderFilled", { ticket: o.ticket }), { description: `${tr("order.toast.orderFilledDesc", { label: tr(pendingLabelKey(o)), volume: fmtVol(o.volume), symbol: o.symbol, price: fmtPrice(o.symbol, px) })}${o.oco ? tr("order.toast.ocoSiblingCancelled") : ""}` });
            notify("fill");
          }
        }
        // alerts
        for (const a of coreRef.current.alerts) {
          if (!a.active || a.symbol !== q.symbol) continue;
          if (a.cond === "above" ? q.bid >= a.price : q.bid <= a.price) {
            commit((cc) => ({ ...cc, alerts: cc.alerts.map((x) => (x.id === a.id ? { ...x, active: false, triggeredAt: new Date().toISOString() } : x)) }));
            log("Alerts", `${a.symbol} bid ${a.cond === "above" ? ">=" : "<="} ${fmtPrice(a.symbol, a.price)} (bid ${fmtPrice(a.symbol, q.bid)})${a.note ? ` · ${a.note}` : ""}`, "warn");
            toast.warning(tr("order.toast.alert", { symbol: a.symbol, cond: tr(`order.toast.alertCond.${a.cond}`), price: fmtPrice(a.symbol, a.price) }), { description: `${tr("order.toast.alertBid", { price: fmtPrice(a.symbol, q.bid) })}${a.note ? ` · ${a.note}` : ""}` });
            notify("alert");
          }
        }
      },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tickSymbols]);

  /* ------------------------------ AI Trader ------------------------------ */

  // Runs this account's AI strategies; orders take the same path as manual ones (placeOrder, source "ai").
  // Guest: drafts can be composed and previewed (stored under "guest"), nothing can trade.
  // The broker switched AI off (module switches): the strategies are kept but don't run until it is back on.
  const aiOn = useModuleOn()("ai");
  React.useEffect(() => {
    if (!aiOn) return;
    if (guest)
      return aiTrader.attach({
        login: GUEST_LOGIN,
        accountType: "live",
        accountMode: "hedging",
        investor: true,
        positions: () => [],
        history: () => [],
        balance: () => 0,
        placeOrder: async () => ({ ok: false }),
        modifyPosition: async () => false,
        closePosition: async () => false,
        log,
      });
    if (engine) {
      // live: AI orders go to the engine like manual ones, tagged source "ai"
      const login = session.login;
      const a = engAccRef.current[login] ?? account;
      return aiTrader.attach({
        login,
        accountType: a.type,
        accountMode: a.mode,
        investor: session.investor,
        positions: () => coreRef.current.positions,
        history: () => coreRef.current.history,
        balance: () => {
          const x = engAccRef.current[login];
          return x ? x.balance / (x.cent ? 100 : 1) : 0;
        },
        placeOrder: (o) => (sessionRef.current.investor ? Promise.resolve({ ok: false }) : eng.placeOrder(o)),
        modifyPosition: (t, p) => eng.modifyPosition(t, p),
        closePosition: (t, reason) => eng.closePosition(t, undefined, reason),
        log,
      });
    }
    const a = accountOf(session.login);
    return aiTrader.attach({
      login: a.login,
      accountType: a.type,
      accountMode: a.mode,
      investor: session.investor,
      positions: () => coreRef.current.positions.filter((p) => p.login === a.login),
      history: () => coreRef.current.history.filter((p) => p.login === a.login),
      balance: () => coreRef.current.balances[a.login] ?? 0,
      placeOrder: async (o) => ({ ok: await placeOrder(o) }),
      modifyPosition: (t, p) => modifyPosition(t, p),
      closePosition: async (ticket, reason) => {
        const p = coreRef.current.positions.find((x) => x.ticket === ticket);
        if (!p) return false;
        if (!isMarketOpen(p.symbol)) return marketClosed(`'${p.login}': ${reason}: close #${ticket} ${p.side} ${fmtVol(p.volume)} ${p.symbol}`, true);
        closeInternal(ticket, undefined, reason);
        return true;
      },
      log,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [session.login, session.investor, !!engAccounts[session.login], aiOn]);

  /* ------------------------------ workspace ------------------------------ */

  const setWs = React.useCallback((patch: Partial<Workspace> | ((w: Workspace) => Partial<Workspace>)) => {
    setWsState((w) => ({ ...w, ...(typeof patch === "function" ? patch(w) : patch) }));
  }, []);

  const setLayout = React.useCallback((l: Layout) => setWsState((w) => fitSlots({ ...w, layout: l })), []);

  // the engine's live switch (live builds), and charts on markets this account may not see go to EURUSD
  React.useEffect(() => startLiveFlags(), []);
  const scope = useMarketScope();
  React.useEffect(() => {
    const hidden = wsRef.current.tabs.filter((t) => !scope.visible(t.symbol));
    if (!hidden.length) return;
    setWsState((w) => ({ ...w, tabs: w.tabs.map((t) => (scope.visible(t.symbol) ? t : { ...t, symbol: "EURUSD", drawings: [] })) }));
    toast(tr("desk.trade.unavailable"), { description: tr("desk.trade.unavailableText"), id: "market-unavailable" });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope.restricted, scope.list]);

  /** A market this account may not see (live trading off): a neutral note, and EURUSD instead. */
  const allowedSymbol = (symbol: string) => {
    if (visibleSymbol(symbol)) return symbol;
    toast(tr("desk.trade.unavailable"), { description: tr("desk.trade.unavailableText"), id: "market-unavailable" });
    return "EURUSD";
  };
  const addTab = React.useCallback((symbol?: string, tf: Timeframe = "H1") => {
    if (symbol) symbol = allowedSymbol(symbol);
    setWsState((w) => {
      const t = makeTab(symbol ?? w.tabs.find((x) => x.id === w.activeId)?.symbol ?? "EURUSD", tf);
      const slots = w.slots.map((s) => (s === w.activeId ? t.id : s));
      return { ...w, tabs: [...w.tabs, t], slots, activeId: t.id };
    });
  }, []);

  const closeTab = React.useCallback((id: string) => {
    setWsState((w) => {
      if (w.tabs.length <= 1) {
        toast(tr("order.toast.lastChart"));
        return w;
      }
      const tabs = w.tabs.filter((t) => t.id !== id);
      let slots = w.slots.filter((s) => s !== id);
      let layout = w.layout;
      const free = tabs.find((t) => !slots.includes(t.id));
      if (slots.length < LAYOUT_COUNT[layout]) {
        if (free) slots = [...w.slots.map((s) => (s === id ? free.id : s))];
        else layout = slots.length >= 2 ? (layout === "4" ? "2v" : layout) : "1";
      }
      if (LAYOUT_COUNT[layout] === 2 && slots.length > 2) slots = slots.slice(0, 2);
      const activeId = w.activeId === id ? slots[0]! : w.activeId;
      return fitSlots({ ...w, tabs, slots, layout, activeId });
    });
  }, []);

  const activateTab = React.useCallback((id: string) => {
    setWsState((w) => {
      if (w.slots.includes(id)) return { ...w, activeId: id };
      return { ...w, slots: w.slots.map((s) => (s === w.activeId ? id : s)), activeId: id };
    });
  }, []);

  const updateTab = React.useCallback((id: string, patch: Partial<ChartTab> | ((t: ChartTab) => Partial<ChartTab>)) => {
    setWsState((w) => ({ ...w, tabs: w.tabs.map((t) => (t.id === id ? { ...t, ...(typeof patch === "function" ? patch(t) : patch) } : t)) }));
  }, []);

  const openSymbol = React.useCallback((symbol: string, newTab = false) => {
    if (newTab) return addTab(symbol);
    symbol = allowedSymbol(symbol);
    setWsState((w) => ({ ...w, tabs: w.tabs.map((t) => (t.id === w.activeId ? { ...t, symbol, drawings: t.symbol === symbol ? t.drawings : [] } : t)) }));
  }, [addTab]);

  const togglePanel = React.useCallback((k: keyof Workspace["panels"], v?: boolean) => {
    setWsState((w) => ({ ...w, panels: { ...w.panels, [k]: v ?? !w.panels[k] } }));
  }, []);

  const deleteSelectedDrawing = React.useCallback(() => {
    if (!selectedDrawing) return;
    setWsState((w) => ({ ...w, tabs: w.tabs.map((t) => ({ ...t, drawings: t.drawings.filter((d) => d.id !== selectedDrawing) })) }));
    selectDrawing(null);
    toast(tr("order.toast.objectDeleted"));
  }, [selectedDrawing]);

  const resetWorkspace = React.useCallback(() => {
    setWsState(() => defaultWorkspace());
    try {
      for (const k of Object.keys(localStorage)) if (k.startsWith("react-resizable-panels:kalks")) localStorage.removeItem(k);
    } catch {
      /* ignore */
    }
    toast.success(tr("order.toast.workspaceReset"));
  }, []);

  const setUi = React.useCallback((patch: Partial<UiState>) => setUiState((u) => ({ ...u, ...patch })), []);
  const openNewOrder = React.useCallback(
    (p?: Partial<NewOrderPrefill>) => {
      if (sessionRef.current.guest) return void guestNotice(tr("order.guest.placingOrder"));
      if (sessionRef.current.investor) return void toast.error(tr("order.toast.readOnly"), { description: tr("order.toast.readOnlyDesc") });
      const w = wsRef.current;
      const sym = p?.symbol ?? w.tabs.find((t) => t.id === w.activeId)?.symbol ?? "EURUSD";
      setUiState((u) => ({ ...u, newOrder: { symbol: sym, side: p?.side, type: p?.type, price: p?.price } }));
    },
    [],
  );

  /* ------------------------------ alerts ------------------------------ */

  const addAlert = React.useCallback(
    (a: Omit<PriceAlert, "id" | "created" | "active">) => {
      commit((c) => ({ ...c, alerts: [{ ...a, id: uid(), active: true, created: new Date().toISOString() }, ...c.alerts] }));
      log("Alerts", `alert created: ${a.symbol} bid ${a.cond === "above" ? ">=" : "<="} ${fmtPrice(a.symbol, a.price)}`);
      toast.success(tr("order.toast.alertCreated"), { description: `${a.symbol} ${tr(`order.toast.alertCond.${a.cond}`)} ${fmtPrice(a.symbol, a.price)}` });
    },
    [commit, log],
  );
  const updateAlert = React.useCallback((id: string, patch: Partial<PriceAlert>) => commit((c) => ({ ...c, alerts: c.alerts.map((a) => (a.id === id ? { ...a, ...patch } : a)) })), [commit]);
  const removeAlert = React.useCallback(
    (id: string) => {
      commit((c) => ({ ...c, alerts: c.alerts.filter((a) => a.id !== id) }));
      toast(tr("order.toast.alertDeleted"));
    },
    [commit],
  );
  const clearJournal = React.useCallback(() => commit((c) => ({ ...c, journal: [] })), [commit]);

  /* ------------------------------ account ------------------------------ */

  const switchAccountMock = React.useCallback(
    (login: string) => {
      if (sessionRef.current.guest) return;
      const a = ACCOUNTS.find((x) => x.login === login);
      if (!a || login === sessionRef.current.login) return;
      const s: Session = { login, investor: false, server: a.server, via: sessionRef.current.via, at: Date.now() };
      priceFeed().setGroup(a.group); // quotes carry this account group's spread
      setSession(s);
      writeSession(s);
      log("Network", `'${login}': authorized on ${a.server} through Access Point EU Frankfurt (ping ${(30 + Math.random() * 14).toFixed(1)} ms)`);
      log("Network", `'${login}': terminal synchronized with Kalks Global`);
      toast.success(tr(a.type === "demo" ? "order.toast.switchedDemo" : "order.toast.switchedLive", { login }), { description: `${a.group} · ${tr.dyn(`order.mode.${a.mode}`, a.mode)} · ${a.server}` });
    },
    [log],
  );

  const refillDemoMock = React.useCallback(() => {
    if (sessionRef.current.guest) return;
    const a = accountOf(sessionRef.current.login);
    if (a.type !== "demo") return void toast.error(tr("order.toast.refillDemoOnly"));
    const left = coreRef.current.refills[a.login] ?? 0;
    if (left <= 0) return void toast.error(tr("order.toast.noRefillsLeft"), { description: tr("order.toast.noRefillsLeftDesc") });
    const target = a.balance;
    commit((c) => ({ ...c, balances: { ...c.balances, [a.login]: target }, refills: { ...c.refills, [a.login]: left - 1 } }));
    log("Account", `'${a.login}': demo balance refilled to ${accMoney(a, target)} ${accCcy(a)} (${left - 1} refills left)`);
    toast.success(tr("order.toast.demoRefilled"), { description: tr("order.toast.refillsLeft", { amount: `${accMoney(a, target)} ${accCcy(a)}`, count: left - 1 }) });
  }, [commit, log]);

  const logoutMock = React.useCallback(() => {
    writeSession(null);
    onLogout();
  }, [onLogout]);

  // ---- engine accounts (live builds): several logins, each with its own engine session

  const showAccount = React.useCallback(
    (login: string, quiet = false) => {
      const e = engSessionsRef.current.find((x) => x.login === login);
      if (!e || login === sessionRef.current.login) return;
      const a = engAccRef.current[login];
      const s: Session = { login, investor: e.readOnly, server: a?.server ?? serverName(login.startsWith("5") ? "demo" : "live"), via: sessionRef.current.via, at: Date.now(), engine: true };
      if (a) priceFeed().setGroup(a.engine.spreadGroup);
      liveStore.clear(sessionRef.current.login);
      sessionRef.current = s;
      setSession(s);
      writeActive(login);
      if (!quiet) toast.success(tr(a?.type === "demo" ? "order.toast.switchedDemo" : "order.toast.switchedLive", { login }), { description: a ? `${a.group} · ${tr.dyn(`order.mode.${a.mode}`, a.mode)} · ${a.server}${e.readOnly ? tr("order.toast.readOnlySuffix") : ""}` : undefined });
    },
    [],
  );

  const dropSession = React.useCallback(
    (login: string, to?: string) => {
      const rest = engSessionsRef.current.filter((x) => x.login !== login);
      engSessionsRef.current = rest;
      setEngSessions(rest);
      setEngAccounts((m) => {
        const { [login]: _gone, ...keep } = m;
        return keep;
      });
      if (sessionRef.current.login !== login) return;
      if (rest[0]) return showAccount(rest[0].login, true);
      writeActive(null);
      writeSession(null);
      onLogout(to);
    },
    [onLogout, showAccount],
  );

  expiredRef.current = (login: string) => {
    if (!engSessionsRef.current.some((x) => x.login === login)) return;
    log("Network", `'${login}': session expired, log in again`, "error");
    toast.error(tr("order.toast.sessionExpired"), { id: `expired-${login}`, description: tr("order.toast.sessionExpiredDesc", { login }) });
    dropSession(login, `/login?expired=1&login=${login}`);
  };

  const switchAccount = React.useCallback((login: string) => (engine ? showAccount(login) : switchAccountMock(login)), [engine, showAccount, switchAccountMock]);

  const refillDemo = React.useCallback(() => {
    if (!engine) return refillDemoMock();
    if (blocked(tr("order.guest.refillingDemo"))) return;
    if (engAccRef.current[sessionRef.current.login]?.type !== "demo") return void toast.error(tr("order.toast.refillDemoOnly"));
    void eng.refillDemo();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engine, eng, refillDemoMock]);

  const logout = React.useCallback(() => {
    if (!engine) return logoutMock();
    const login = sessionRef.current.login;
    void engineApi.logout(login).then(() => {
      log("Network", `'${login}': logged out`);
      if (engSessionsRef.current.length > 1) toast(tr("order.toast.loggedOut", { login }));
      dropSession(login, "/login?logout=1");
    });
  }, [engine, logoutMock, log, dropSession]);

  const openLogin = React.useCallback(() => setUiState((u) => ({ ...u, loginDialog: true })), []);

  const accountAdded = React.useCallback(
    async (login: string) => {
      const r = await engineApi.sessions();
      if (r.ok) {
        const list = r.data.sessions.map((x) => ({ login: x.login, readOnly: x.readOnly }));
        engSessionsRef.current = list;
        setEngSessions(list);
        const accs: Record<string, EngineTradingAccount> = {};
        for (const x of r.data.sessions) if (x.account) accs[x.login] = mapAccount(x.account);
        engAccRef.current = { ...engAccRef.current, ...accs };
        setEngAccounts((m) => ({ ...m, ...accs }));
      }
      if (login === sessionRef.current.login) {
        // same account again (e.g. investor → master password): reload with the new session
        const e = engSessionsRef.current.find((x) => x.login === login);
        if (e) setSession((s) => ({ ...s, investor: e.readOnly }));
        streamRef.current?.reconnect();
        void loadState(login);
        return;
      }
      showAccount(login);
    },
    [loadState, showAccount],
  );

  /* ------------------------------ derived ------------------------------ */

  const activeTab = ws.tabs.find((t) => t.id === ws.activeId) ?? ws.tabs[0]!;
  const engList = React.useMemo(() => engSessions.map((x) => engAccRef.current[x.login] ?? engAccounts[x.login]).filter(Boolean) as TradingAccount[], [engSessions, engAccounts]);
  const engBalances = React.useMemo(() => Object.fromEntries(Object.values(engAccounts).map((a) => [a.login, a.balance / (a.cent ? 100 : 1)])), [engAccounts]);
  const value: Ctx = {
    session,
    guest,
    live: IS_LIVE,
    engine,
    synced,
    account,
    accounts: guest ? [] : engine ? engList : ACCOUNTS,
    readOnly,
    positions: engine ? core.positions : core.positions.filter((p) => p.login === session.login),
    pendings: engine ? core.pendings : core.pendings.filter((p) => p.login === session.login),
    history: engine ? core.history : core.history.filter((p) => p.login === session.login),
    allPositions: core.positions,
    balances: engine ? engBalances : core.balances,
    refillsLeft: engine ? (account.refillsLeft ?? 0) : (core.refills[session.login] ?? 0),
    alerts: core.alerts,
    journal: core.journal,
    ws,
    ui,
    drawTool,
    selectedDrawing,
    activeTab,
    activeSymbol: activeTab.symbol,
    setWs,
    setLayout,
    addTab,
    closeTab,
    activateTab,
    updateTab,
    openSymbol,
    togglePanel,
    setDrawTool,
    selectDrawing,
    deleteSelectedDrawing,
    resetWorkspace,
    setUi,
    openNewOrder,
    placeOrder,
    quickTrade,
    closePosition,
    modifyPosition,
    closeBy,
    cancelPending,
    modifyPending,
    bulkClose,
    cancelAllPendings,
    addAlert,
    updateAlert,
    removeAlert,
    log,
    clearJournal,
    switchAccount,
    refillDemo,
    logout,
    openLogin,
    accountAdded,
  };
  // CFD / Options account split: the active engine account's product decides the workspace (lib/options/mode.ts)
  const accountProduct = engine && !guest && engAccounts[session.login] ? productOf(engAccounts[session.login]!.engine) : null;
  return (
    <TerminalCtx.Provider value={value}>
      <AccountProductContext.Provider value={accountProduct}>{children}</AccountProductContext.Provider>
    </TerminalCtx.Provider>
  );
}

/** Journal timestamp "2026.09.24 14:32:11.482" in server time. */
export function journalTime(ts: number) {
  const s = serverTime(new Date(ts));
  return `${s.date} ${s.time}.${s.ms}`;
}

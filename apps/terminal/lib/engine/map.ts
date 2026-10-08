// Engine JSON → terminal shapes. Pure (no React, no browser APIs), so the share BFF can use it too.
// Money convention of the terminal store: USD internally; cent accounts report USC from the engine (× 100),
// so their amounts are divided by 100 here (the UI multiplies back through accMoney()).
import type { TradingAccount } from "@kalks/mock";
import type { PendingOrder, TClosed, TPosition, TradeSource } from "../trading";
import { pointSize } from "../trading";
import type { EngAccount, EngDeal, EngOrder, EngPosition } from "./types";

export const serverName = (type: "live" | "demo") => (type === "demo" ? "Kalks-Demo" : "Kalks-Live");

const SOURCES: Record<string, TradeSource> = { manual: "manual", ai: "ai", api: "api", fix: "api", webhook: "api", strategy: "strategy", copy: "copy", pamm: "pamm", mam: "mam" };
export const mapSource = (s: string): TradeSource => SOURCES[s] ?? "manual";

const opt = (v: number | null | undefined) => (v === null || v === undefined ? undefined : v);

export interface EngineAccountExtra {
  status: string;
  spreadGroup: string;
  groupCode: string;
  leverages: number[];
  controls?: EngAccount["controls"];
  marginCallLevel?: number;
  stopOutLevel?: number;
  demoInitial?: number;
  /** live numbers in account currency (USC on cent accounts) */
  profit: number;
  swap: number;
  freeMargin: number;
  marginLevel: number | null;
  /** copy trading: the account is a follower's copy account (group `copy` / `copy-netting`, or the engine's `role`) */
  copy?: boolean;
  /** CFD / Options account split: the account's product decides the workspace (lib/options/mode.ts). */
  product: "cfd" | "options";
}

/** Copy accounts are named "Copy · <master>" by the engine (social/copier.rs). */
const COPY_NAME = /^Copy\s*·\s*/;

/** True for a copy-trading follower account: system group `copy` / `copy-*`, or `role` copy/follower when the engine sends it. */
export function isCopyGroup(group: string | undefined, role?: string | null) {
  const g = (group ?? "").toLowerCase();
  return g === "copy" || g.startsWith("copy-") || role === "copy" || role === "follower";
}

/** The master's nickname from a copy account's name ("Copy · Alpha" -> "Alpha"); null when unknown. */
export function copyMasterName(name: string | null | undefined) {
  return name && COPY_NAME.test(name) ? name.replace(COPY_NAME, "").trim() || null : null;
}

/** Fired in the browser when the engine rejects a request with `copy_account` (the account is managed by copy trading). */
export const COPY_ACCOUNT_EVENT = "kalks:copy-account";

export type EngineTradingAccount = TradingAccount & { engine: EngineAccountExtra };

export function mapAccount(a: EngAccount): EngineTradingAccount {
  const refillsLeft = a.demo ? Math.max(0, a.demo.refillsPerDay - a.demo.refillsUsedToday) : undefined;
  return {
    login: String(a.login),
    type: a.type,
    group: a.groupName || a.group,
    mode: a.mode,
    cent: a.cent,
    server: serverName(a.type),
    leverage: a.leverage,
    currency: a.currency,
    balance: a.balance,
    equity: a.equity,
    credit: a.credit + (a.bonus ?? 0),
    margin: a.margin,
    nickname: a.name || undefined,
    createdAt: a.createdAt,
    refillsLeft,
    swapFree: false,
    engine: {
      status: a.status,
      spreadGroup: a.spreadGroup || a.group,
      groupCode: a.group,
      leverages: a.leverages ?? [],
      controls: a.controls,
      marginCallLevel: a.marginCallLevel,
      stopOutLevel: a.stopOutLevel,
      demoInitial: a.demo?.initialBalance,
      profit: a.profit,
      swap: a.swap,
      freeMargin: a.freeMargin,
      marginLevel: a.marginLevel,
      copy: isCopyGroup(a.group, (a as { role?: string | null }).role),
      product: a.product === "options" ? "options" : "cfd",
    },
  };
}

export function mapPosition(p: EngPosition, cent: boolean): TPosition & { profit?: number; currentPrice?: number } {
  const k = cent ? 100 : 1;
  return {
    ticket: String(p.ticket),
    login: String(p.login),
    symbol: p.symbol,
    side: p.side,
    volume: p.volume,
    openPrice: p.openPrice,
    sl: opt(p.sl),
    tp: opt(p.tp),
    trailing: p.trailingPoints ? p.trailingPoints * pointSize(p.symbol) : undefined,
    swap: p.swap / k,
    commission: p.commission / k,
    openTime: p.openTime,
    source: mapSource(p.source),
    comment: p.comment || undefined,
    profit: p.profit !== undefined ? p.profit / k : undefined,
    currentPrice: p.currentPrice,
  };
}

export function mapOrder(o: EngOrder): PendingOrder {
  const triggered = o.type === "stop_limit" && o.triggered;
  const expiry = o.expiry === "GTC" || o.expiry === "Today" ? o.expiry : "Date";
  return {
    ticket: String(o.ticket),
    login: String(o.login),
    symbol: o.symbol,
    side: o.side,
    // a triggered stop-limit is a limit order at its stop-limit price (MT5)
    type: triggered ? "limit" : o.type === "stop_limit" ? "stop-limit" : o.type,
    volume: o.volume,
    price: triggered && o.stopLimit !== null ? o.stopLimit : o.price,
    stopLimit: !triggered && o.stopLimit !== null ? o.stopLimit : undefined,
    sl: opt(o.sl),
    tp: opt(o.tp),
    trailing: o.trailingPoints ? o.trailingPoints * pointSize(o.symbol) : undefined,
    expiry,
    expiryDate: expiry === "Date" ? (o.expiryAt ?? o.expiry).slice(0, 10) : undefined,
    placed: o.placedAt,
    source: mapSource(o.source),
    comment: o.comment || undefined,
    oco: o.oco ? String(Math.min(o.oco, o.ticket)) : undefined,
  };
}

const REASONS: Record<string, string> = { client: "manual", sl: "sl", tp: "tp", stop_out: "stop out", close_by: "close by", dealer: "dealer", force: "dealer", price_correction: "correction", pending_fill: "manual" };

/** Closing deals → closed-trade rows (one per exit deal, newest first). Entry deals give the commission share. */
export function mapHistory(deals: EngDeal[], cent: boolean): (TClosed & { deal: string })[] {
  const k = cent ? 100 : 1;
  const entries = new Map<number, EngDeal>();
  for (const d of deals) if (d.entry === "in" && !entries.has(d.positionTicket)) entries.set(d.positionTicket, d);
  return deals
    .filter((d) => (d.entry === "out" || d.entry === "out_by") && !d.reversed)
    .map((d) => {
      const e = entries.get(d.positionTicket);
      const entryCommission = e && e.volume > 0 ? (e.commission * Math.min(1, d.volume / e.volume)) : 0;
      const commission = (d.commission + entryCommission) / k;
      const swap = d.swap / k;
      const gross = d.profit / k;
      return {
        deal: String(d.id),
        ticket: String(d.positionTicket),
        login: String(d.login),
        symbol: d.symbol,
        side: d.positionSide,
        volume: d.volume,
        openPrice: d.openPrice,
        openTime: d.openTime,
        swap: +swap.toFixed(2),
        commission: +commission.toFixed(2),
        source: mapSource(d.source),
        comment: d.comment || undefined,
        closePrice: d.price,
        closeTime: d.time,
        profit: +(gross + swap - commission).toFixed(2),
        reason: REASONS[d.reason] ?? d.reason,
      };
    })
    .sort((a, b) => Date.parse(b.closeTime) - Date.parse(a.closeTime));
}

/* ------------------------------------------------------------------ */
/* Server rejections → MT5-style reasons                               */
/* ------------------------------------------------------------------ */

const REJECT: Record<string, string> = {
  market_closed: "Market closed",
  no_price: "No prices",
  stale_price: "No prices",
  no_money: "Not enough money",
  insufficient_funds: "Not enough money",
  invalid_volume: "Invalid volume",
  invalid_price: "Invalid price",
  invalid_sl: "Invalid stops",
  invalid_tp: "Invalid stops",
  invalid_expiry: "Invalid expiration",
  invalid_oco: "Invalid OCO order",
  max_lot: "Volume limit reached",
  close_only: "Only position closing is allowed",
  trading_disabled: "Trading is disabled",
  account_status: "Account is disabled",
  symbol_halted: "Trading is halted on this symbol",
  symbol_close_only: "Only position closing is allowed on this symbol",
  not_hedging: "Close By needs a hedging account",
  invalid_close_by: "Invalid Close By",
  requote: "Requote",
  read_only: "Trading is disabled (investor password)",
  forbidden: "Not allowed",
  positions_open: "Positions are open",
  refill_limit: "No refills left today",
  refill_not_needed: "Balance is already at the initial amount",
  demo_account: "Demo accounts only",
  rate_limited: "Too many requests",
  unavailable: "No connection with the trade server",
  engine_error: "Trade server error",
  session_expired: "Session expired",
  unauthorized: "Not logged in",
  not_found: "Not found",
  no_change: "No changes",
  validation: "Invalid request",
  bad_request: "Invalid request",
  duplicate: "Duplicate request",
  // copy trading (D70): copied trades close with the master; the follower stops copying in the Client Area
  copy_managed: "Copied position",
  copy_account: "Copy account",
  pamm_account: "PAMM fund account",
};

export interface EngineErr {
  status: number;
  code: string;
  message: string;
  bid?: number;
  ask?: number;
  field?: string;
}

/** Short MT5-style reason for a rejected request ("Market closed", "Not enough money", "Requote"…). */
export function rejectReason(e: EngineErr): string {
  if (e.code === "copy_account" && typeof window !== "undefined") window.dispatchEvent(new CustomEvent(COPY_ACCOUNT_EVENT));
  return REJECT[e.code] ?? (e.message || e.code.replace(/_/g, " "));
}

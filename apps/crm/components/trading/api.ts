"use client";

// Browser side of the trading BFF (app/api/trading/[...path]/route.ts). Live builds only: demo builds keep
// the mock accounts from @kalks/mock.

import * as React from "react";
import { toast } from "sonner";
import { tr } from "@kalks/i18n/react";
import { intlTag } from "@kalks/i18n/locales";
import type { MessageKey } from "@kalks/i18n";
import { readCached, writeCached } from "@kalks/ui/swr-cache";
import type { DealOption, InstrumentFilter, PositionOption } from "./option-deal";

/* ------------------------------------------------------------------ */
/* Engine shapes (services/trading/README.md, client-safe subset)       */
/* ------------------------------------------------------------------ */

export type AccountKind = "live" | "demo";

export interface EngineAccount {
  login: number;
  type: AccountKind;
  group: string;
  groupName: string;
  mode: "hedging" | "netting";
  cent: boolean;
  currency: string;
  baseCurrency: string;
  leverage: number;
  leverages: number[];
  status: "active" | "close_only" | "read_only" | "disabled" | "expired" | "archived" | "closed";
  name: string;
  /** When the account was archived / closed (engine; absent on older engines). */
  archivedAt?: string | null;
  closedAt?: string | null;
  updatedAt?: string | null;
  /** The client's starred default account (B9). */
  isDefault?: boolean;
  /** The latest close-permanently request (B12). */
  closureRequest?: { id: number; status: "pending" | "approved" | "rejected" | "cancelled" } | null;
  /** Flagged dormant (no activity for the broker's dormancy period, B11). */
  dormantSince?: string | null;
  lastActivityAt?: string | null;
  marginCall: boolean;
  marginCallLevel: number;
  stopOutLevel: number;
  positions: number;
  orders: number;
  controls: { tradingDisabled: boolean; closeOnly: boolean; maxLot: number | null };
  balance: number;
  credit: number;
  bonus: number;
  profit: number;
  swap: number;
  equity: number;
  margin: number;
  freeMargin: number;
  marginLevel: number | null;
  withdrawable: number;
  demo?: { initialBalance: number; refillsPerDay: number; refillsUsedToday: number; expiryDays: number } | null;
  createdAt: string;
  /** What the account trades: its group's product (CFD / Options account split; absent on older engines = CFD). */
  product?: "cfd" | "options";
}

export interface EngineGroup {
  code: string;
  name: string;
  mode: "hedging" | "netting";
  cent: boolean;
  accountTypes: "live" | "demo" | "both";
  leverages: number[];
  defaultLeverage: number;
  marginCallPct: number;
  stopOutPct: number;
  hedgedMarginPct: number;
  minDeposit: number;
  swapFree: boolean;
  commissionPerLot: number;
  spreadGroup: string;
  maxAccountsPerUser: number;
  demoInitialBalance: number;
  demoRefillsPerDay: number;
  demoExpiryDays: number;
  enabled: boolean;
  /** CFD group or Options group (absent on older engines = CFD). */
  product?: "cfd" | "options";
}

export interface EnginePosition {
  ticket: number;
  login: number;
  symbol: string;
  side: "buy" | "sell";
  volume: number;
  openPrice: number;
  openTime: string;
  sl: number | null;
  tp: number | null;
  swap: number;
  commission: number;
  currentPrice: number | null;
  profit: number;
  source: string;
  platform: string;
  comment: string;
  /** Kalks FX Options position: `symbol` is the series code, `volume` the contracts, prices are premiums per unit
   *  of the underlying (null on CFD positions). */
  option?: PositionOption | null;
  /** Options: value now and signed premium booked at open, both in the account currency. */
  markValue?: number | null;
  premium?: number | null;
  mark?: number | null;
}

export interface EngineOrder {
  ticket: number;
  symbol: string;
  side: "buy" | "sell";
  type: "market" | "limit" | "stop" | "stop_limit";
  volume: number;
  price: number | null;
  stopLimit: number | null;
  sl: number | null;
  tp: number | null;
  expiry: string;
  placedAt: string;
  status?: string;
  doneAt?: string | null;
  fillPrice?: number | null;
  reason?: string | null;
  /** Kalks FX Options pending order (`symbol` = the first leg's series code). */
  option?: { legs?: { series: string; side: "buy" | "sell"; contracts: number; option?: PositionOption }[]; limitPremium?: number | null } | null;
}

export interface EngineDeal {
  id: number;
  login: number;
  positionTicket: number;
  orderTicket: number | null;
  symbol: string;
  side: "buy" | "sell";
  positionSide: "buy" | "sell";
  entry: "in" | "out" | "out_by";
  volume: number;
  price: number;
  profit: number;
  swap: number;
  commission: number;
  reason: string;
  time: string;
  openPrice: number | null;
  openTime: string | null;
  source: string;
  comment: string;
  reversed?: boolean;
  /** "option" for Kalks FX Options deals (volume = contracts, price = premium per unit), "cfd" otherwise. */
  instrument?: "option" | "cfd" | null;
  option?: DealOption | null;
}

export interface HistoryPage {
  deals: EngineDeal[];
  orders: EngineOrder[];
  page: number;
  limit: number;
  total: number;
  totals: { profit: number; swap: number; commission: number };
  /** Filtered views (?instrument=option|cfd) only: the BFF scanned the newest deals of the period, not all of them. */
  truncated?: boolean;
}

export interface LedgerItem {
  txn: number;
  kind: string;
  subLedger: "balance" | "credit" | "bonus" | string;
  amount: number;
  currency: string;
  reference: string | null;
  reasonCode: string | null;
  note: string | null;
  at: string;
}

export interface LedgerPage {
  items: LedgerItem[];
  page: number;
  limit: number;
  total: number;
}

export interface AccountDetail {
  account: EngineAccount;
  positions: EnginePosition[];
  orders: EngineOrder[];
}

export interface OpenResult {
  account: EngineAccount;
  credentials: { login: number; password?: string; investorPassword?: string };
}

/* ------------------------------------------------------------------ */
/* Fetch                                                               */
/* ------------------------------------------------------------------ */

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public field?: string,
  ) {
    super(message);
  }
}

const FRIENDLY: Record<string, MessageKey> = {
  positions_open: "accounts.error.positions_open",
  refill_limit: "accounts.error.refill_limit",
  refill_not_needed: "accounts.error.refill_not_needed",
  account_limit: "accounts.error.account_limit",
  invalid_leverage: "accounts.error.invalid_leverage",
  unavailable: "accounts.error.unavailable",
};

export async function tradingApi<T>(path: string, init?: { method?: "GET" | "POST" | "PATCH"; body?: unknown; signal?: AbortSignal }): Promise<T> {
  const method = init?.method ?? (init?.body !== undefined ? "POST" : "GET");
  let res: Response;
  try {
    res = await fetch(`/api/trading/${path}`, {
      method,
      headers: method !== "GET" ? { "content-type": "application/json" } : undefined,
      body: method !== "GET" ? JSON.stringify(init?.body ?? {}) : undefined,
      cache: "no-store",
      signal: init?.signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    throw new ApiError(0, "network", tr("common.networkError"));
  }
  const data = (await res.json().catch(() => ({}))) as { error?: { code?: string; message?: string; field?: string } };
  if (!res.ok) {
    if (res.status === 401 && typeof window !== "undefined") {
      window.location.assign(`/api/auth/expired?next=${encodeURIComponent(window.location.pathname + window.location.search)}`);
    }
    const code = data.error?.code ?? "error";
    throw new ApiError(res.status, code, (FRIENDLY[code] ? tr(FRIENDLY[code]) : undefined) ?? data.error?.message ?? tr("common.errorRetry"), data.error?.field);
  }
  return data as T;
}

export function errorToast(title: string, e: unknown) {
  toast.error(title, { description: e instanceof Error ? e.message : tr("common.errorRetry") });
}

/** Polls `path` every `ms` while the tab is visible. `reload()` refetches at once.
 *  A page opened again starts from this tab's last answer while it refetches (@kalks/ui/swr-cache: per session,
 *  cleared by any write, so nothing from before a change the client just made is shown). */
export function usePoll<T>(path: string | null, ms: number) {
  const [data, setData] = React.useState<T | null>(() => (path ? (readCached<T>(`trading:${path}`) ?? null) : null));
  const [error, setError] = React.useState<ApiError | null>(null);
  const [tick, setTick] = React.useState(0);
  const reload = React.useCallback(() => setTick((t) => t + 1), []);

  React.useEffect(() => {
    if (!path) return;
    const cached = readCached<T>(`trading:${path}`);
    if (cached !== undefined) setData(cached);
    let stop = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const ctl = new AbortController();
    const run = async () => {
      if (stop) return;
      if (typeof document === "undefined" || document.visibilityState === "visible") {
        try {
          const d = await tradingApi<T>(path, { signal: ctl.signal });
          if (stop) return;
          setData(d);
          setError(null);
          writeCached(`trading:${path}`, d);
        } catch (e) {
          if (stop || (e as Error).name === "AbortError") return;
          setError(e instanceof ApiError ? e : new ApiError(0, "error", tr("common.error")));
        }
      }
      if (!stop && ms > 0) timer = setTimeout(run, ms);
    };
    run();
    return () => {
      stop = true;
      ctl.abort();
      if (timer) clearTimeout(timer);
    };
  }, [path, ms, tick]);

  return { data, error, loading: data === null && error === null, reload };
}

export const useAccounts = (ms = 5000) => usePoll<{ accounts: EngineAccount[] }>("accounts", ms);
export const useGroups = () => usePoll<{ groups: EngineGroup[] }>("groups", 0);

/**
 * Trade button: asks for a one-time SSO token and opens Kalks Trader at `/?sso=<token>`.
 * The tab is opened synchronously (inside the click) so popup blockers let it through.
 */
export async function openTerminal(login: number) {
  const w = window.open("about:blank", "_blank");
  try {
    const r = await tradingApi<{ url: string }>(`accounts/${login}/sso`, { body: {} });
    if (w && !w.closed) {
      w.opener = null;
      w.location.replace(r.url);
    } else {
      window.location.assign(r.url);
    }
  } catch (e) {
    w?.close();
    errorToast(tr("accounts.toast.openTraderFailed"), e);
  }
}

/* ------------------------------------------------------------------ */
/* Formatting                                                          */
/* ------------------------------------------------------------------ */

/** Money prefix: "$" for USD, "USC " for cent accounts (D30). */
export const curOf = (a: Pick<EngineAccount, "cent" | "currency">) => (a.cent || a.currency === "USC" ? "USC " : a.currency === "USD" ? "$" : `${a.currency} `);

/** USD value of an amount in the account currency. */
export const toUsd = (a: Pick<EngineAccount, "cent" | "currency">, v: number) => (a.cent || a.currency === "USC" ? v / 100 : v);

export const modeLabel = (m: string) => (m === "netting" ? "Netting" : "Hedging");

export const accountTitle = (a: Pick<EngineAccount, "groupName" | "mode">) => `${a.groupName} · ${modeLabel(a.mode)}`;

/** MT5-style server name shown next to the login (D4). */
export const serverOf = (a: { type: AccountKind }) => (a.type === "live" ? "Kalks-Live" : "Kalks-Demo");

export function levelTone(ml: number | null | undefined): "up" | "warn" | "down" | undefined {
  if (ml === null || ml === undefined || !Number.isFinite(ml)) return undefined;
  return ml > 500 ? "up" : ml > 200 ? "warn" : "down";
}

export function fmtLevel(ml: number | null | undefined) {
  return ml === null || ml === undefined || !Number.isFinite(ml) || ml <= 0 ? "—" : `${Math.round(ml).toLocaleString("en-US")}%`;
}

export const STATUS_LABEL: Record<EngineAccount["status"], { label: string; tone: "up" | "warn" | "down" | "neutral" }> = {
  active: { label: "Active", tone: "up" },
  close_only: { label: "Close only", tone: "warn" },
  read_only: { label: "Read only", tone: "warn" },
  disabled: { label: "Disabled", tone: "down" },
  expired: { label: "Expired", tone: "neutral" },
  archived: { label: "Archived", tone: "neutral" },
  closed: { label: "Closed", tone: "neutral" },
};

/** Archived / closed accounts sit in the Archived tab: out of the Live / Demo lists and the live totals. */
export const isArchived = (a: Pick<EngineAccount, "status">) => a.status === "archived" || a.status === "closed";

/** Server time offset in hours: GMT+3 while US DST is on, GMT+2 otherwise (same rule as the engine). */
export function serverOffset(t: Date) {
  const y = t.getUTCFullYear();
  const nthSunday = (month: number, n: number) => {
    const first = new Date(Date.UTC(y, month, 1)).getUTCDay();
    return 1 + ((7 - first) % 7) + (n - 1) * 7;
  };
  const start = Date.UTC(y, 2, nthSunday(2, 2), 7); // 2nd Sunday of March, 02:00 New York
  const end = Date.UTC(y, 10, nthSunday(10, 1), 6); // 1st Sunday of November, 02:00 New York
  return t.getTime() >= start && t.getTime() < end ? 3 : 2;
}

/** "24 Sep 2026, 14:03" in server time. */
export function serverTime(iso: string | null | undefined, withYear = true) {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const shifted = new Date(d.getTime() + serverOffset(d) * 3600_000);
  return new Intl.DateTimeFormat(intlTag(tr.locale), {
    day: "2-digit",
    month: "short",
    ...(withYear ? { year: "numeric" } : {}),
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
  }).format(shifted);
}

export function fmtDate(iso: string | null | undefined) {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleDateString(intlTag(tr.locale), { day: "2-digit", month: "short", year: "numeric" });
}

/** Digits for price display when no instrument spec is at hand. */
export function priceDigits(v: number) {
  if (!Number.isFinite(v)) return 2;
  if (v >= 1000) return 2;
  if (v >= 50) return 3;
  return 5;
}

export function fmtPrice(v: number | null | undefined, digits?: number) {
  if (v === null || v === undefined || !Number.isFinite(v)) return "—";
  const d = digits ?? priceDigits(v);
  return v.toLocaleString("en-US", { minimumFractionDigits: d, maximumFractionDigits: d });
}

export function fmtAmount(v: number, cur: string, signed = false) {
  const s = Math.abs(v).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return `${signed ? (v > 0 ? "+" : v < 0 ? "-" : "") : v < 0 ? "-" : ""}${cur}${s}`;
}

/** YYYY-MM-DD in local time. */
export function isoDay(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export const LEDGER_KIND: Record<string, { label: string; tone: "up" | "down" | "ember" | "gold" | "info" | "neutral" }> = {
  transfer_in: { label: "Deposit from wallet", tone: "up" },
  transfer_out: { label: "Withdrawal to wallet", tone: "down" },
  trade_pnl: { label: "Trade result", tone: "ember" },
  commission: { label: "Commission", tone: "neutral" },
  deposit: { label: "Deposit", tone: "up" },
  withdrawal: { label: "Withdrawal", tone: "down" },
  adjustment: { label: "Adjustment", tone: "info" },
  credit: { label: "Credit", tone: "gold" },
  bonus: { label: "Bonus", tone: "gold" },
  nbp: { label: "Negative balance protection", tone: "info" },
  demo_funding: { label: "Demo funds", tone: "gold" },
  demo_initial: { label: "Demo funds", tone: "gold" },
  demo_refill: { label: "Demo refill", tone: "gold" },
  reversal: { label: "Reversal", tone: "neutral" },
  charges: { label: "Charges", tone: "neutral" },
  swap: { label: "Swap", tone: "neutral" },
  perf_fee: { label: "Performance fee", tone: "neutral" },
  option_premium: { label: "Option premium", tone: "ember" },
  option_settlement: { label: "Option settlement", tone: "ember" },
  option_rebate: { label: "Option rebate", tone: "up" },
};

export const ledgerKind = (k: string) => LEDGER_KIND[k] ?? { label: k.replace(/_/g, " "), tone: "neutral" as const };

export const REASON_LABEL: Record<string, string> = {
  client: "Client",
  dealer: "Dealer",
  sl: "Stop loss",
  tp: "Take profit",
  stop_out: "Stop out",
  close_by: "Close by",
  pending_fill: "Pending order",
  force: "Dealer",
  price_correction: "Price correction",
};

/** Browser download of a server-built CSV (the BFF sets Content-Disposition). `instrument` narrows a trades export
 *  to CFD or option deals. */
export function downloadExport(login: number, kind: "history" | "ledger", from?: string, to?: string, instrument: InstrumentFilter = "all") {
  const q = new URLSearchParams({ kind });
  if (from) q.set("from", from);
  if (to) q.set("to", to);
  if (kind === "history" && instrument !== "all") q.set("instrument", instrument);
  const description = tr("accounts.toast.exportDesc", { login, kind: tr(kind === "history" ? "accounts.export.trades" : "accounts.export.ledger") });
  // fetched first: a service error becomes a message instead of replacing the page with raw JSON
  void (async () => {
    try {
      const r = await fetch(`/api/trading/accounts/${login}/export?${q}`, { credentials: "same-origin" });
      if (!r.ok) {
        const body = (await r.json().catch(() => null)) as { error?: { message?: string } } | null;
        throw new Error(body?.error?.message || tr("common.errorRetry"));
      }
      const href = URL.createObjectURL(await r.blob());
      const a = document.createElement("a");
      a.href = href;
      a.download = /filename="?([^";]+)"?/.exec(r.headers.get("content-disposition") ?? "")?.[1] ?? `${login}-${kind}.csv`;
      document.body.appendChild(a);
      a.click();
      a.remove();
      setTimeout(() => URL.revokeObjectURL(href), 10_000);
      toast.success(tr("accounts.toast.exportStarted"), { description });
    } catch (e) {
      toast.error(tr("accounts.toast.exportFailed"), { description: e instanceof Error ? e.message : description });
    }
  })();
}

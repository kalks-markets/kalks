"use client";

/**
 * Live desk directory: trading accounts, groups, contract specs and client names from the trading engine
 * (through /api/trading/*). Filled by RestTradingDesk in live builds; empty in demo builds, where every helper
 * in calc.ts falls back to the mock data. Components read it with useLiveDirectory().
 */
import * as React from "react";
import type { Book } from "./types";

export interface LiveControls {
  tradingDisabled: boolean;
  closeOnly: boolean;
  maxLot: number | null;
  execDelayMs: number;
  markupPips: number;
}

/** Engine account view (GET /v1/admin/accounts), logins and user ids as strings. */
export interface LiveAccount {
  login: string;
  userId: string;
  type: "live" | "demo";
  group: string;
  groupName: string;
  mode: "hedging" | "netting";
  cent: boolean;
  currency: "USD" | "USC";
  leverage: number;
  leverages: number[];
  status: "active" | "close_only" | "read_only" | "disabled" | "expired" | "archived" | "closed";
  name: string;
  route: Book;
  marginCall: boolean;
  marginCallLevel: number;
  stopOutLevel: number;
  positions: number;
  orders: number;
  controls: LiveControls;
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
  createdAt: string;
  demo?: { initialBalance: number; refillsPerDay: number; refillsUsedToday: number; expiryDays: number };
  /** What the account trades: its group's product (CFD / Options account split). Absent on older engines = CFD. */
  product?: Product;
}

/** CFD / Options account split: a group's accounts trade CFDs or Kalks FX Options, never both. */
export type Product = "cfd" | "options";

/** The product of a group or account (anything but "options" is a CFD one, as older engines send none). */
export const productOf = (x: { product?: string | null } | null | undefined): Product => (x?.product === "options" ? "options" : "cfd");

/** Engine group (GET /v1/admin/groups). */
export interface LiveGroup {
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
  route: Book;
  spreadGroup: string;
  maxAccountsPerUser: number;
  demoInitialBalance: number;
  demoRefillsPerDay: number;
  demoExpiryDays: number;
  enabled: boolean;
  /** Fixed once the group has accounts (the engine refuses the change). */
  product?: Product;
  accounts?: number;
}

/** Engine contract spec (GET /v1/symbols). */
export interface LiveSymbol {
  symbol: string;
  assetClass: string;
  digits: number;
  point: number;
  pipSize: number;
  contractSize: number;
  profitCurrency: string;
  lotMin: number;
  lotMax: number;
  lotStep: number;
  marginPct: number;
  maxLeverage: number;
  open: boolean;
  session: string;
  swapLong: number;
  swapShort: number;
  tripleSwapDay: string;
}

type Snapshot = { version: number; loaded: boolean; accounts: Map<string, LiveAccount>; groups: LiveGroup[]; symbols: Map<string, LiveSymbol>; names: Map<string, { name: string; email: string }> };

let snap: Snapshot = { version: 0, loaded: false, accounts: new Map(), groups: [], symbols: new Map(), names: new Map() };
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const SERVER: Snapshot = snap;

function set(p: Partial<Snapshot>) {
  snap = { ...snap, ...p, version: snap.version + 1 };
  emit();
}

export const liveDirectory = () => snap;
export const liveAccount = (login: string) => snap.accounts.get(login);
export const liveSymbol = (symbol: string) => snap.symbols.get(symbol);
export const liveGroups = () => snap.groups;
export const liveGroup = (code: string) => snap.groups.find((g) => g.code === code);

/** Display name of a client (gateway user) — falls back to the account name, then the id. */
export function liveClientName(userId: string, login?: string) {
  const n = snap.names.get(userId)?.name;
  if (n) return n;
  const a = login ? snap.accounts.get(login) : undefined;
  if (a?.name) return a.name;
  return `Client ${userId}`;
}
export const liveClientEmail = (userId: string) => snap.names.get(userId)?.email ?? "";

export function useLiveDirectory() {
  return React.useSyncExternalStore(
    (fn) => {
      listeners.add(fn);
      return () => void listeners.delete(fn);
    },
    () => snap,
    () => SERVER,
  );
}

/* ---------------- loading ---------------- */

type RawAccount = Omit<LiveAccount, "login" | "userId"> & { login: number | string; userId: number | string };
export const normAccount = (a: RawAccount): LiveAccount => ({ ...a, login: String(a.login), userId: String(a.userId) });

async function get<T>(url: string): Promise<T | null> {
  try {
    const r = await fetch(url, { cache: "no-store", credentials: "same-origin" });
    if (!r.ok) return null;
    return (await r.json()) as T;
  } catch {
    return null;
  }
}

/** Upserts accounts (e.g. from an account detail fetch) without a full reload. */
export function upsertAccounts(list: LiveAccount[]) {
  if (!list.length) return;
  const accounts = new Map(snap.accounts);
  for (const a of list) accounts.set(a.login, a);
  set({ accounts });
  void loadNames(list.map((a) => a.userId));
}

let namesInFlight = new Set<string>();
async function loadNames(ids: string[]) {
  const missing = Array.from(new Set(ids)).filter((id) => !snap.names.has(id) && !namesInFlight.has(id));
  if (!missing.length) return;
  missing.forEach((id) => namesInFlight.add(id));
  for (let i = 0; i < missing.length; i += 60) {
    const chunk = missing.slice(i, i + 60);
    const r = await get<{ names: Record<string, { name: string; email: string }> }>(`/api/trading/clients?ids=${chunk.join(",")}`);
    const names = new Map(snap.names);
    for (const id of chunk) names.set(id, r?.names[id] ?? { name: "", email: "" });
    set({ names });
  }
  namesInFlight = new Set([...namesInFlight].filter((id) => !missing.includes(id)));
}

export async function loadAccounts() {
  const r = await get<{ items: RawAccount[] }>("/api/trading/admin/accounts?limit=500");
  if (!r) return;
  const accounts = new Map(r.items.map((a) => [String(a.login), normAccount(a)]));
  set({ accounts, loaded: true });
  void loadNames([...accounts.values()].map((a) => a.userId));
}

export async function loadGroups() {
  const r = await get<{ groups: LiveGroup[] }>("/api/trading/admin/groups");
  if (r) set({ groups: r.groups });
}

export async function loadDirectory() {
  const [, , sym] = await Promise.all([loadAccounts(), loadGroups(), get<{ symbols: LiveSymbol[] }>("/api/trading/symbols")]);
  if (sym) set({ symbols: new Map(sym.symbols.map((s) => [s.symbol, s])) });
}

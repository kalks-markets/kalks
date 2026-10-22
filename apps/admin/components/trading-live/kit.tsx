"use client";

/** Live-build helpers for the trading pages that talk to the engine outside the dealing desk (accounts, groups). */
import { sendJson } from "@/components/live/kit";
import type { AuditEntry, DeskResult, Reason, RestTradingDesk } from "@/lib/trading-desk";

export const ACC_REASONS = ["ACC-01 · Client request", "ACC-02 · Compliance / KYC", "ACC-03 · Risk management", "ACC-04 · Error correction", "ACC-05 · Inactivity", "ACC-99 · Other"] as const;
export const FIN_REASONS = ["FIN-01 · Manual deposit", "FIN-02 · Manual withdrawal", "FIN-03 · Compensation", "FIN-04 · Error correction", "FIN-05 · Bonus / promotion", "FIN-06 · Credit line", "FIN-99 · Other"] as const;
export const GRP_REASONS = ["GRP-01 · New offering", "GRP-02 · Pricing change", "GRP-03 · Risk policy", "GRP-04 · Regulatory", "GRP-99 · Other"] as const;

/** Writes to /api/trading/<path> with the reason; returns a DeskResult so DeskDialog can report it. */
export async function tradingWrite<T>(path: string, body: Record<string, unknown>, reason: Reason, desk?: RestTradingDesk | null, method: "POST" | "PUT" | "DELETE" = "POST"): Promise<DeskResult<T>> {
  const r = await sendJson<{ data: T; audit?: AuditEntry[] }>(`/api/trading/${path}`, { ...body, reasonCode: reason.code, note: reason.note?.trim() ?? "" }, method);
  if (!r.ok) return { ok: false, error: r.error.message };
  const audit = r.data.audit ?? [];
  desk?.ingestAudit(audit);
  return { ok: true, data: r.data.data, audit };
}

export const ACC_ARCHIVE_REASONS = ["ARC-01 · Client request", "ARC-02 · Dormant / inactive", "ARC-03 · Duplicate account", "ARC-04 · Compliance", "ARC-05 · Expired demo cleanup", "ARC-99 · Other"] as const;
export const ACC_RESTORE_REASONS = ["RST-01 · Client request", "RST-02 · Archived in error", "RST-03 · Compliance cleared", "RST-99 · Other"] as const;

export const STATUS_LABEL: Record<string, string> = { active: "Active", close_only: "Close-only", read_only: "Read-only", disabled: "Disabled", expired: "Expired", archived: "Archived", closed: "Closed" };
export const STATUS_TONE: Record<string, "up" | "warn" | "down" | "neutral"> = { active: "up", close_only: "warn", read_only: "warn", disabled: "down", expired: "neutral", archived: "neutral", closed: "down" };
/** Lifecycle states: reached only through Archive / Restore (and Close permanently), never the generic status selector. */
export const LIFECYCLE_STATUSES: readonly string[] = ["archived", "closed"];
/** Statuses the generic "Change status" selector may set. */
export const SETTABLE_STATUS = Object.entries(STATUS_LABEL)
  .filter(([s]) => !LIFECYCLE_STATUSES.includes(s))
  .map(([value, label]) => ({ value, label }));

/** Product kind of a trading account, derived from its group code (copy / PAMM / MAM / prop / regular). */
export type AccountKind = "copy" | "pamm" | "mam" | "prop" | "regular";
export function accountKind(group: string): AccountKind {
  const g = group.toLowerCase();
  if (g === "copy" || g === "copy-netting") return "copy";
  if (g === "pamm") return "pamm";
  if (g === "mam") return "mam";
  if (g.startsWith("prop")) return "prop";
  return "regular";
}
export const KIND_LABEL: Record<AccountKind, string> = { copy: "Copy", pamm: "PAMM", mam: "MAM", prop: "Prop", regular: "Regular" };
export const KIND_TONE: Record<Exclude<AccountKind, "regular">, "info" | "gold" | "ember" | "warn"> = { copy: "info", pamm: "gold", mam: "gold", prop: "ember" };

/** Terminal deal view (GET /v1/accounts/{login}/history). */
export type HistDeal = {
  id: number;
  positionTicket: number;
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
  book: "A" | "B";
  time: string;
  comment: string;
  reversed: boolean;
  priceCorrection: boolean;
};
export type History = { deals: HistDeal[]; page: number; limit: number; total: number; totals: { profit: number; swap: number; commission: number } };
export type LedgerItem = { txn: number | string; kind: string; subLedger: string; amount: number; currency: string; reference: string | null; reasonCode: string | null; note: string | null; at: string };
export type Ledger = { items: LedgerItem[]; page: number; limit: number; total: number };

export const LEDGER_KIND: Record<string, string> = {
  transfer_in: "Wallet → account",
  transfer_out: "Account → wallet",
  trade_pnl: "Trade P&L",
  commission: "Commission",
  deposit: "Deposit (staff)",
  withdrawal: "Withdrawal (staff)",
  adjustment: "Adjustment",
  credit: "Credit",
  bonus: "Bonus",
  nbp: "Negative balance protection",
  demo_funding: "Demo funding",
  demo_initial: "Demo opening balance",
  demo_refill: "Demo refill",
  reversal: "Reversal",
  swap: "Swap",
};

export const money2 = (v: number, ccy = "USD") => `${v < 0 ? "−" : ""}${ccy === "USC" ? "" : "$"}${Math.abs(v).toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}${ccy === "USC" ? " USC" : ""}`;
export const signed2 = (v: number, ccy = "USD") => `${v > 0 ? "+" : ""}${money2(v, ccy)}`;

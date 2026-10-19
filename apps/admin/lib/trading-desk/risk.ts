"use client";

/**
 * Risk desk (trading engine, live builds): server exposure with limit usage, risk limits, alerts, client scores,
 * routing suggestions and news windows. The REST calls go through the trading BFF (/api/trading/dealing/...);
 * the dealing stream frames `exposure` and `risk_alert` are fed in by RestTradingDesk (rest.ts) and kept in a
 * small module store read with useExposure() / useRiskAlerts(). Demo builds never call the engine: the hooks
 * return empty data and the pages show their demo / "connect the engine" state.
 */
import * as React from "react";
import { IS_DEMO } from "@kalks/mock/mode";
import type { AuditEntry, Book } from "./types";

/* ---------------- contract types (engine /v1/dealing/...) ---------------- */

export type RiskMode = "alert" | "enforce";
export type LimitLevel = "ok" | "amber" | "red";

export interface ExposureSymbol {
  symbol: string;
  longLots: number;
  shortLots: number;
  netLots: number;
  longUsd: number;
  shortUsd: number;
  netUsd: number;
  grossUsd: number;
  positions: number;
  accounts: number;
  clientFloatingUsd: number;
  houseFloatingUsd: number;
  books: Record<Book, { netLots: number; netUsd: number }>;
  limit: { capUsd: number; warnPct: number; usedPct: number; level: LimitLevel; mode: RiskMode } | null;
  topClients: { login: string; userId: string; netLots: number; netUsd: number; floatingUsd: number }[];
}

export interface Exposure {
  generatedAt: string;
  kind: "live" | "demo";
  serverDayStart: string;
  totals: {
    grossUsd: number;
    netUsd: number;
    longUsd: number;
    shortUsd: number;
    positions: number;
    accounts: number;
    clientFloatingUsd: number;
    houseFloatingUsd: number;
    realisedToday: { clientUsd: number; houseUsd: number; byBook: Record<Book, number> };
    houseDailyPnlUsd: number;
    baselineAt: string | null;
  };
  books: Record<Book, { netUsd: number; grossUsd: number; lots: number; positions: number; clientFloatingUsd: number }>;
  symbols: ExposureSymbol[];
  groups: { group: string; netUsd: number; lots: number; positions: number }[];
}

export interface ExposureLimitRow {
  symbol: string;
  capUsd: number;
  warnPct: number;
  mode: RiskMode;
  enabled: boolean;
}
export interface GroupLimitRow {
  group: string;
  maxLot: number | null;
  maxPositions: number | null;
  mode: RiskMode;
  enabled: boolean;
}
export interface DailyLossLimit {
  thresholdUsd: number;
  warnPct: number;
  mode: RiskMode;
  enabled: boolean;
}
export interface NewsLimit {
  minutesBefore: number;
  minutesAfter: number;
  minImpact: number;
  marginMultiplier: number;
  blockPending: boolean;
  mode: RiskMode;
  enabled: boolean;
}
export interface RiskLimits {
  exposure: ExposureLimitRow[];
  dailyLoss: DailyLossLimit | null;
  groups: GroupLimitRow[];
  news: NewsLimit | null;
}
export type RoutingMode = "recommend" | "execute";
export interface RiskSettings {
  routingMode: RoutingMode;
  scoreIntervalMins: number;
}
export interface RiskLimitsDoc {
  limits: RiskLimits;
  settings: RiskSettings;
  updatedAt: string | null;
  updatedBy: string | null;
}

export type AlertKind = "exposure" | "daily_loss" | "news" | "limit_reject" | "toxic_client" | "routing";
export interface RiskAlert {
  id: string;
  key: string;
  kind: AlertKind;
  level: "red" | "amber" | "info";
  title: string;
  message: string;
  symbol: string | null;
  login: string | null;
  data: Record<string, unknown>;
  count: number;
  createdAt: string;
  updatedAt: string;
  ackedAt: string | null;
  ackedBy: string | null;
  resolvedAt: string | null;
  status: "open" | "acked" | "resolved";
}

export type ScoreCategory = "likely_profitable" | "neutral" | "likely_losing" | "toxic";
export interface ScoreWhy {
  factor: string;
  label: string;
  value: string;
  points: number;
  detail: string;
  tone: "up" | "down" | "warn" | "neutral";
}
export interface ClientScore {
  login: string;
  userId: string;
  group: string;
  kind: "live" | "demo";
  score: number;
  category: ScoreCategory;
  trades: number;
  computedAt: string;
  winRate: number | null;
  profitFactor: number | null;
  netPnlUsd: number;
  avgHoldSecs: number | null;
  scalpingShare: number | null;
  toxicFlags: string[];
  why: ScoreWhy[];
  metrics: Record<string, number | string | null>;
  facts: { depositsUsd: number; accountAgeDays: number; country: string | null; ibSource: string | null; bonusUsd: number };
}
export interface ScoreList {
  items: ClientScore[];
  computedAt: string | null;
  running: boolean;
}
export interface ScoreDetail {
  current: ClientScore | null;
  history: { computedAt: string; score: number; category: string }[];
}

export interface RoutingSuggestion {
  id: string;
  kind: "position" | "account";
  login: string;
  userId: string;
  ticket: string | null;
  symbol: string | null;
  volume: number | null;
  from: Book;
  to: Book;
  reasons: { label: string; detail: string }[];
  rule: string | null;
  score: number | null;
  category: string | null;
  status: "pending" | "approved" | "rejected" | "expired";
  createdAt: string;
  decidedAt: string | null;
  decidedBy: string | null;
  note: string | null;
  result: { done: string[]; failed: { ticket: string; error: string }[] } | null;
}

export interface NewsWindow {
  id: string;
  title: string;
  currency: string;
  impact: number;
  startsAt: string;
  from: string;
  to: string;
  active: boolean;
  symbols: string[];
}
export interface NewsDoc {
  windows: NewsWindow[];
  configured: boolean;
  lastOkAt: string | null;
}

/* ---------------- labels ---------------- */

export const CATEGORY_LABEL: Record<ScoreCategory, string> = { toxic: "Toxic", likely_profitable: "Likely profitable", neutral: "Neutral", likely_losing: "Likely losing" };
export const CATEGORY_TONE: Record<ScoreCategory, "down" | "warn" | "neutral" | "up"> = { toxic: "down", likely_profitable: "warn", neutral: "neutral", likely_losing: "up" };
export const categoryLabel = (c: string | null | undefined) => (c ? ((CATEGORY_LABEL as Record<string, string>)[c] ?? c.replace(/_/g, " ")) : "—");
export const categoryTone = (c: string | null | undefined) => (c ? ((CATEGORY_TONE as Record<string, "down" | "warn" | "neutral" | "up">)[c] ?? "neutral") : "neutral");

export const FLAG_LABEL: Record<string, string> = { latency_arbitrage: "Latency arbitrage", gap_trading: "Gap trading", copied_trades: "Copied trades", bonus_abuse: "Bonus abuse", scalping: "Scalping", news_trading: "News trading" };
export const flagLabel = (f: string) => FLAG_LABEL[f] ?? f.replace(/_/g, " ");

export const ALERT_KIND_LABEL: Record<AlertKind, string> = { exposure: "Exposure", daily_loss: "Daily loss", news: "News window", limit_reject: "Limit reject", toxic_client: "Toxic client", routing: "Routing" };

export const REJECT_LABEL: Record<string, string> = { exposure_limit: "Exposure limit", group_max_lot: "Group max lot", group_max_positions: "Group max positions", news_window: "News window" };

/** "45s" / "12m" / "3.2h" for a hold time in seconds. */
export function holdText(secs: number | null | undefined) {
  if (secs === null || secs === undefined) return "—";
  if (secs < 90) return `${Math.round(secs)}s`;
  if (secs < 5400) return `${Math.round(secs / 60)}m`;
  if (secs < 172_800) return `${(secs / 3600).toFixed(1)}h`;
  return `${(secs / 86400).toFixed(1)}d`;
}

/* ---------------- REST ---------------- */

export type RiskErr = { code: string; message: string; status: number };
export type RiskRes<T> = { ok: true; data: T; audit: AuditEntry[] } | { ok: false; error: RiskErr; audit: AuditEntry[] };

/** Engine without the risk desk routes (BFF: engine_pending) or not configured. */
export const isPending = (e: { code: string } | null | undefined) => !!e && (e.code === "engine_pending" || e.code === "not_configured" || e.code === "not_found");

export async function riskCall<T>(method: "GET" | "POST" | "PUT", path: string, body?: unknown): Promise<RiskRes<T>> {
  try {
    const r = await fetch(`/api/trading/${path}`, {
      method,
      headers: body !== undefined ? { "content-type": "application/json" } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined,
      cache: "no-store",
      credentials: "same-origin",
    });
    const j = (await r.json().catch(() => ({}))) as { data?: T; audit?: AuditEntry[]; error?: { code?: string; message?: string } };
    if (r.status === 401 && typeof window !== "undefined") {
      const next = window.location.pathname + window.location.search;
      window.location.assign(`/api/auth/expired?next=${encodeURIComponent(next)}`);
    }
    const audit = j.audit ?? [];
    if (!r.ok) return { ok: false, error: { code: j.error?.code ?? (r.status === 503 ? "not_configured" : "unknown"), message: j.error?.message ?? "Request failed", status: r.status }, audit };
    return { ok: true, data: (method === "GET" ? j : j.data) as T, audit };
  } catch {
    return { ok: false, error: { code: "network", message: "Can't reach the Back Office server.", status: 0 }, audit: [] };
  }
}

/* ---------------- stream-fed store ---------------- */

type RiskSnap = {
  version: number;
  exposure: Exposure | null;
  /** when the last exposure arrived (stream frame or REST) */
  exposureAt: number;
  exposureError: RiskErr | null;
  alerts: RiskAlert[];
  alertsLoaded: boolean;
  alertsError: RiskErr | null;
};

let snap: RiskSnap = { version: 0, exposure: null, exposureAt: 0, exposureError: null, alerts: [], alertsLoaded: false, alertsError: null };
const SERVER: RiskSnap = snap;
const listeners = new Set<() => void>();
const redListeners = new Set<(a: RiskAlert) => void>();

function set(p: Partial<RiskSnap>) {
  snap = { ...snap, ...p, version: snap.version + 1 };
  listeners.forEach((l) => l());
}
const subscribe = (fn: () => void) => {
  listeners.add(fn);
  return () => void listeners.delete(fn);
};

export const riskSnapshot = () => snap;

/** Stream frame `{type:"exposure"}` (RestTradingDesk). */
export function ingestExposure(e: Exposure) {
  if (!e || !Array.isArray(e.symbols)) return;
  set({ exposure: e, exposureAt: Date.now(), exposureError: null });
}

const sortAlerts = (xs: RiskAlert[]) => [...xs].sort((a, b) => (a.level === "red" ? 0 : a.level === "amber" ? 1 : 2) - (b.level === "red" ? 0 : b.level === "amber" ? 1 : 2) || Date.parse(b.updatedAt) - Date.parse(a.updatedAt));

/** Stream frame `{type:"risk_alert"}` and ack results: upsert; resolved ones leave the open list. Fires the "new red" listeners for an unseen red alert (or one escalated to red). */
export function ingestAlert(raw: RiskAlert) {
  if (!raw?.id) return;
  const a = { ...raw, id: String(raw.id), login: raw.login === null || raw.login === undefined ? null : String(raw.login) };
  const prev = snap.alerts.find((x) => x.id === a.id);
  const rest = snap.alerts.filter((x) => x.id !== a.id);
  set({ alerts: sortAlerts(a.status === "resolved" ? rest : [a, ...rest]) });
  if (a.status === "open" && a.level === "red" && (!prev || prev.level !== "red")) redListeners.forEach((l) => l(a));
}

export function onNewRedAlert(fn: (a: RiskAlert) => void) {
  redListeners.add(fn);
  return () => void redListeners.delete(fn);
}

let exposureInFlight = false;
export async function loadExposure() {
  if (IS_DEMO || exposureInFlight) return;
  exposureInFlight = true;
  const r = await riskCall<Exposure>("GET", "dealing/exposure?kind=live");
  exposureInFlight = false;
  if (r.ok) set({ exposure: r.data, exposureAt: Date.now(), exposureError: null });
  else set({ exposureError: r.error });
}

let alertsInFlight = false;
export async function loadAlerts() {
  if (IS_DEMO || alertsInFlight) return;
  alertsInFlight = true;
  const [open, acked] = await Promise.all([riskCall<RiskAlert[]>("GET", "dealing/risk/alerts?status=open&limit=200"), riskCall<RiskAlert[]>("GET", "dealing/risk/alerts?status=acked&limit=50")]);
  alertsInFlight = false;
  if (!open.ok) return set({ alertsLoaded: true, alertsError: open.error });
  const list = [...(Array.isArray(open.data) ? open.data : []), ...(acked.ok && Array.isArray(acked.data) ? acked.data : [])];
  set({ alerts: sortAlerts(list.map((a) => ({ ...a, id: String(a.id), login: a.login === null || a.login === undefined ? null : String(a.login) }))), alertsLoaded: true, alertsError: null });
}

export async function ackAlert(id: string, note?: string): Promise<RiskRes<RiskAlert>> {
  const r = await riskCall<RiskAlert>("POST", `dealing/risk/alerts/${id}/ack`, { note: note?.trim() || undefined });
  if (r.ok && r.data) ingestAlert(r.data);
  return r;
}

/**
 * Server exposure, live: the `exposure` stream frame every 5 s; when no frame arrived for 7 s (stream offline,
 * page outside the dealing desk) the REST endpoint is polled every 5 s.
 */
export function useExposure() {
  const s = React.useSyncExternalStore(subscribe, riskSnapshot, () => SERVER);
  React.useEffect(() => {
    if (IS_DEMO) return;
    if (Date.now() - riskSnapshot().exposureAt > 7000) void loadExposure();
    const t = setInterval(() => {
      if (document.visibilityState === "visible" && Date.now() - riskSnapshot().exposureAt > 7000) void loadExposure();
    }, 5000);
    return () => clearInterval(t);
  }, []);
  return { exposure: s.exposure, error: s.exposure ? null : s.exposureError, at: s.exposureAt, loading: !IS_DEMO && !s.exposure && !s.exposureError };
}

/** Open and acknowledged (not resolved) alerts: REST on mount + every 30 s, live through the `risk_alert` frames. */
export function useRiskAlerts() {
  const s = React.useSyncExternalStore(subscribe, riskSnapshot, () => SERVER);
  React.useEffect(() => {
    if (IS_DEMO) return;
    void loadAlerts();
    const t = setInterval(() => {
      if (document.visibilityState === "visible") void loadAlerts();
    }, 30_000);
    return () => clearInterval(t);
  }, []);
  return { alerts: s.alerts, loaded: IS_DEMO || s.alertsLoaded, error: s.alertsError, reload: loadAlerts };
}

/* ---------------- defaults (demo builds, empty engine) ---------------- */

export const DEFAULT_LIMITS: RiskLimits = {
  exposure: [
    { symbol: "*", capUsd: 2_000_000, warnPct: 80, mode: "alert", enabled: true },
    { symbol: "XAUUSD", capUsd: 5_000_000, warnPct: 80, mode: "alert", enabled: true },
    { symbol: "EURUSD", capUsd: 8_000_000, warnPct: 80, mode: "alert", enabled: true },
    { symbol: "NAS100", capUsd: 3_000_000, warnPct: 75, mode: "alert", enabled: true },
    { symbol: "BTCUSD", capUsd: 1_500_000, warnPct: 70, mode: "alert", enabled: true },
  ],
  dailyLoss: { thresholdUsd: 150_000, warnPct: 70, mode: "alert", enabled: true },
  groups: [],
  news: { minutesBefore: 2, minutesAfter: 2, minImpact: 3, marginMultiplier: 1, blockPending: false, mode: "alert", enabled: true },
};
export const DEFAULT_SETTINGS: RiskSettings = { routingMode: "recommend", scoreIntervalMins: 60 };

"use client";

/**
 * Dealing desk service layer — trading-engine implementation (live builds).
 *
 * Implements TradingDeskApi against the Back Office BFF (/api/trading/*, see app/api/trading/[...path]),
 * which calls the engine's dealing API (services/trading/README.md, "Dealing desk API") with the internal token
 * and the signed-in staff member's identity. The engine executes, validates and writes the audit log; this
 * class only mirrors state:
 *   - hydrate: GET dealing/state + dealing/audit, the account/group/symbol directory
 *   - live: the dealing WebSocket (one-time ticket) — position / order / deal / audit deltas and a P&L frame
 *     every second; on `resync` or reconnect the state is reloaded over REST
 *   - writes: POST/PATCH/PUT with { reasonCode, note }; the returned audit entries are merged immediately.
 *   - risk desk frames (`exposure`, `risk_alert`) go to the risk store (risk.ts), read with useExposure / useRiskAlerts.
 */
import type { RoutingRule, TradingGroup } from "@kalks/mock/admin-trading";
import { liveAccount, loadAccounts, loadDirectory } from "./directory";
import { ingestAlert, ingestExposure, type Exposure, type RiskAlert } from "./risk";
import type {
  AccountControl,
  AuditEntry,
  Book,
  BulkOutcome,
  ControlMode,
  CreateTradeInput,
  DeskDeal,
  DeskOrder,
  DeskPosition,
  DeskResult,
  DeskState,
  OrderPatch,
  Reason,
  StaffRef,
  SymbolControl,
  TenantPolicy,
  TradingDeskApi,
} from "./types";

const EMPTY: DeskState = {
  version: 1,
  positions: [],
  orders: [],
  deals: [],
  audit: [],
  symbolControls: [],
  accountControls: [],
  routingRules: [],
  balanceAdj: {},
  tenant: { execDelayEnabled: false, execDelayCapMs: 500, marginCallPct: 100, stopOutPct: 50 },
  seq: { position: 0, order: 0, deal: 0, audit: 0, control: 0 },
};

type Raw = Record<string, unknown>;
const nn = <T,>(v: T | null | undefined): T | undefined => (v === null ? undefined : v);

function normPosition(p: Raw): DeskPosition {
  const x = p as unknown as DeskPosition & { sl: number | null; tp: number | null; parentTicket: string | null; childTickets: string[] | null; comment: string | null };
  return { ...x, sl: nn(x.sl), tp: nn(x.tp), parentTicket: nn(x.parentTicket), childTickets: nn(x.childTickets), comment: x.comment || undefined, routeHistory: x.routeHistory ?? [], bookCarry: x.bookCarry ?? { A: 0, B: 0 } };
}

function normOrder(o: Raw): DeskOrder {
  const x = o as unknown as DeskOrder & { stopLimit: number | null; sl: number | null; tp: number | null; book: Book | null; comment: string | null };
  return { ...x, stopLimit: nn(x.stopLimit), sl: nn(x.sl), tp: nn(x.tp), book: nn(x.book), comment: x.comment || undefined, expiry: x.expiry ?? "GTC" };
}

function normDeal(d: Raw): DeskDeal {
  const x = d as unknown as DeskDeal;
  return { ...x, currency: liveAccount(x.login)?.currency ?? "USD", snapshot: undefined as unknown as DeskPosition };
}

function normAudit(a: Raw): AuditEntry {
  const x = a as unknown as AuditEntry & { login?: string | number | null; symbol?: string | null; tickets?: string[]; flags?: string[] | null };
  return { ...x, login: x.login === null || x.login === undefined ? undefined : String(x.login), symbol: nn(x.symbol), tickets: x.tickets ?? [], note: x.note ?? "", flags: x.flags?.length ? x.flags : undefined };
}

function normControl(c: Raw): AccountControl {
  const x = c as unknown as AccountControl & { maxLot: number | null };
  return { ...x, maxLot: x.maxLot ?? 0, reason: x.reason ?? "", setBy: x.setBy ?? "", updated: x.updated ?? "" };
}

type Api<T> = { ok: true; status: number; data: T; audit: AuditEntry[] } | { ok: false; status: number; error: string; code: string; audit: AuditEntry[] };

async function call<T>(method: "GET" | "POST" | "PUT" | "PATCH", path: string, body?: unknown, wrapped = method !== "GET"): Promise<Api<T>> {
  try {
    const r = await fetch(`/api/trading/${path}`, {
      method,
      headers: body !== undefined ? { "content-type": "application/json" } : undefined,
      body: body !== undefined ? JSON.stringify(body) : undefined,
      cache: "no-store",
      credentials: "same-origin",
    });
    const j = (await r.json().catch(() => ({}))) as { data?: T; audit?: Raw[]; error?: { code?: string; message?: string } };
    if (r.status === 401 && typeof window !== "undefined") {
      const next = window.location.pathname + window.location.search;
      window.location.assign(`/api/auth/expired?next=${encodeURIComponent(next)}`);
    }
    const audit = (j.audit ?? []).map(normAudit);
    if (!r.ok) return { ok: false, status: r.status, error: j.error?.message ?? "Request failed", code: j.error?.code ?? "unknown", audit };
    // reads return the payload itself; writes wrap it in { data, audit }
    return { ok: true, status: r.status, data: (wrapped ? j.data : j) as T, audit };
  } catch {
    return { ok: false, status: 0, error: "Can't reach the Back Office server.", code: "network", audit: [] };
  }
}

/** "AUD-000123" → 123 (the engine's audit id, also the `before` cursor). */
const auditSeq = (a: AuditEntry) => Number(a.id.replace(/\D/g, "")) || 0;

const reasonBody = (r: Reason) => ({ reasonCode: r.code, note: r.note?.trim() ?? "" });

export class RestTradingDesk implements TradingDeskApi {
  private state: DeskState = EMPTY;
  private listeners = new Set<() => void>();
  private hydrated = false;
  private ws: WebSocket | null = null;
  private retry = 0;
  private closed = false;
  private poll: ReturnType<typeof setInterval> | null = null;
  /** true once the first REST load finished (pages show skeletons before) */
  ready = false;
  /** last load / stream error for the connection chip */
  status: "connecting" | "live" | "offline" = "connecting";

  static readonly serverState: DeskState = EMPTY;

  getState = () => this.state;
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => void this.listeners.delete(fn);
  };
  setActor(_staff: StaffRef) {
    /* the BFF derives the staff identity from the verified session */
  }

  private set(p: Partial<DeskState>) {
    this.state = { ...this.state, ...p };
    this.listeners.forEach((l) => l());
  }

  /* ---------------- loading + stream ---------------- */

  hydrate() {
    if (this.hydrated || typeof window === "undefined") return;
    this.hydrated = true;
    void loadDirectory();
    void this.reload().then(() => this.connect());
    this.poll = setInterval(() => {
      if (document.visibilityState === "visible") void loadAccounts();
    }, 5000);
  }

  async reload() {
    const [st, audit] = await Promise.all([
      call<{ positions: Raw[]; orders: Raw[]; deals: Raw[]; symbolControls: SymbolControl[]; accountControls: Raw[]; routingRules: RoutingRule[]; tenant: TenantPolicy }>("GET", "dealing/state"),
      call<Raw[]>("GET", "dealing/audit?limit=500"),
    ]);
    if (!st.ok) {
      this.status = "offline";
      this.ready = true;
      this.set({});
      return;
    }
    const d = st.data;
    this.ready = true;
    this.set({
      positions: d.positions.map(normPosition),
      orders: d.orders.map(normOrder),
      deals: d.deals.map(normDeal),
      symbolControls: d.symbolControls ?? [],
      accountControls: (d.accountControls ?? []).map(normControl),
      routingRules: d.routingRules ?? [],
      tenant: d.tenant ?? EMPTY.tenant,
      audit: audit.ok ? audit.data.map(normAudit) : this.state.audit,
    });
  }

  private async reloadControls() {
    const [c, rules] = await Promise.all([call<{ symbolControls: SymbolControl[]; accountControls: Raw[]; tenant: TenantPolicy }>("GET", "dealing/controls"), call<RoutingRule[]>("GET", "dealing/routing/rules")]);
    const p: Partial<DeskState> = {};
    if (c.ok) Object.assign(p, { symbolControls: c.data.symbolControls, accountControls: c.data.accountControls.map(normControl), tenant: c.data.tenant });
    if (rules.ok) p.routingRules = rules.data;
    this.set(p);
    void loadAccounts();
  }

  private async connect() {
    if (this.closed) return;
    const t = await call<{ ticket: string; url: string | null }>("POST", "dealing/stream-ticket", {}, false);
    if (!t.ok) return this.reconnectLater();
    const base = t.data.url || `${window.location.protocol === "https:" ? "wss" : "ws"}://${window.location.host}/engine/stream`;
    let ws: WebSocket;
    try {
      ws = new WebSocket(`${base}?ticket=${encodeURIComponent(t.data.ticket)}`);
    } catch {
      return this.reconnectLater();
    }
    this.ws = ws;
    ws.onopen = () => {
      this.retry = 0;
      this.status = "live";
      this.set({});
    };
    ws.onmessage = (e) => {
      try {
        this.frame(JSON.parse(String(e.data)) as Raw);
      } catch {
        /* ignore malformed frame */
      }
    };
    ws.onclose = () => {
      if (this.ws !== ws) return;
      this.ws = null;
      this.status = "offline";
      this.set({});
      this.reconnectLater();
    };
    ws.onerror = () => ws.close();
  }

  private reconnectLater() {
    if (this.closed) return;
    const wait = Math.min(15_000, 1000 * 2 ** this.retry++);
    setTimeout(() => {
      void this.reload().then(() => this.connect());
    }, wait);
  }

  private frame(f: Raw) {
    const s = this.state;
    switch (f.type) {
      case "snapshot": {
        this.set({ positions: (f.positions as Raw[]).map(normPosition), orders: (f.orders as Raw[]).map(normOrder) });
        return;
      }
      case "position": {
        if (f.op === "remove") return this.set({ positions: s.positions.filter((p) => p.ticket !== String(f.ticket)) });
        const p = normPosition(f.position as Raw);
        const i = s.positions.findIndex((x) => x.ticket === p.ticket);
        return this.set({ positions: i < 0 ? [p, ...s.positions] : s.positions.map((x, k) => (k === i ? p : x)) });
      }
      case "order": {
        if (f.op === "remove") return this.set({ orders: s.orders.filter((o) => o.ticket !== String(f.ticket)) });
        const o = normOrder(f.order as Raw);
        const i = s.orders.findIndex((x) => x.ticket === o.ticket);
        return this.set({ orders: i < 0 ? [o, ...s.orders] : s.orders.map((x, k) => (k === i ? o : x)) });
      }
      case "deal": {
        const d = normDeal(f.deal as Raw);
        const i = s.deals.findIndex((x) => x.id === d.id);
        return this.set({ deals: i < 0 ? [d, ...s.deals] : s.deals.map((x, k) => (k === i ? d : x)) });
      }
      case "audit": {
        const a = normAudit(f.entry as Raw);
        this.mergeAudit([a]);
        if (a.action.startsWith("control.") || a.action === "routing.rule") void this.reloadControls();
        if (a.action === "deal.reopen" || a.action === "position.void") void this.reloadDeals();
        return;
      }
      case "pnl": {
        const items = f.items as { ticket: string | number; price: number | null; profit: number | null }[];
        if (!items?.length) return;
        const m = new Map(items.map((x) => [String(x.ticket), x]));
        let changed = false;
        const positions = s.positions.map((p) => {
          const u = m.get(p.ticket);
          if (!u || (u.price === p.currentPrice && u.profit === p.profit)) return p;
          changed = true;
          return { ...p, currentPrice: u.price, profit: u.profit };
        });
        if (changed) this.set({ positions });
        return;
      }
      // risk desk: server exposure every 5 s (live accounts) and risk alerts (new / escalated / acked / resolved)
      case "exposure":
        ingestExposure(f.exposure as Exposure);
        return;
      case "risk_alert":
        ingestAlert(f.alert as RiskAlert);
        return;
      case "resync":
        void this.reload();
        return;
      default:
        return;
    }
  }

  private async reloadDeals() {
    const r = await call<Raw[]>("GET", "dealing/deals?limit=200");
    if (r.ok) this.set({ deals: r.data.map(normDeal) });
  }

  /** Adds audit entries returned by a write made outside the desk (account / group operations). */
  ingestAudit(entries: AuditEntry[]) {
    this.mergeAudit(entries.map((a) => normAudit(a as unknown as Raw)));
  }

  private mergeAudit(entries: AuditEntry[]) {
    if (!entries.length) return;
    const seen = new Set(this.state.audit.map((a) => a.id));
    const fresh = entries.filter((a) => !seen.has(a.id));
    if (fresh.length) this.set({ audit: [...fresh, ...this.state.audit].sort((a, b) => auditSeq(b) - auditSeq(a)) });
  }

  /** Runs one write; merges the audit entries it returns (also on a rejection). */
  private async write<T>(method: "POST" | "PUT" | "PATCH", path: string, body: Raw, after?: () => void | Promise<void>): Promise<DeskResult<T>> {
    const r = await call<T>(method, path, body);
    this.mergeAudit(r.audit);
    if (!r.ok) return { ok: false, error: r.error, audit: r.audit };
    if (after) await after();
    return { ok: true, data: (r.data ?? null) as T, audit: r.audit };
  }

  /* ---------------- TradingDeskApi ---------------- */

  createTrade(input: CreateTradeInput, reason: Reason) {
    return this.write<{ ticket: string; kind: "position" | "order"; price: number; book: Book; delayMs: number }>("POST", "dealing/trades", { ...input, ...reasonBody(reason) });
  }

  modifyPosition(ticket: string, patch: { sl?: number | null; tp?: number | null }, reason: Reason) {
    return this.write<null>("PATCH", `dealing/positions/${ticket}`, { ...patch, ...reasonBody(reason) });
  }

  modifyMany(tickets: string[], rule: { slPct?: number | null; tpPct?: number | null; clear?: "sl" | "tp" | "both" }, reason: Reason) {
    return this.write<BulkOutcome>("POST", "dealing/positions/bulk", { tickets, op: "modify", ...rule, ...reasonBody(reason) });
  }

  partialClose(ticket: string, volume: number, reason: Reason, opts?: { price?: number }) {
    return this.write<{ dealId: string; profit: number }>("POST", `dealing/positions/${ticket}/close`, { volume, price: opts?.price, ...reasonBody(reason) });
  }

  closePosition(ticket: string, reason: Reason, opts?: { price?: number; force?: boolean; stopOut?: boolean }) {
    return this.write<{ dealId: string; profit: number }>("POST", `dealing/positions/${ticket}/close`, { price: opts?.price, force: opts?.force, stopOut: opts?.stopOut, ...reasonBody(reason) });
  }

  async closeMany(tickets: string[], reason: Reason, opts?: { force?: boolean }) {
    const r = await this.write<BulkOutcome & { profit?: number }>("POST", "dealing/positions/bulk", { tickets, op: "close", force: opts?.force, ...reasonBody(reason) });
    return r.ok ? { ...r, data: { ...r.data, profit: r.data.profit ?? 0 } } : r;
  }

  addVolume(ticket: string, volume: number, reason: Reason) {
    return this.write<null>("POST", `dealing/positions/${ticket}/add`, { volume, ...reasonBody(reason) });
  }

  priceCorrection(ticket: string, openPrice: number, reason: Reason) {
    return this.write<null>("POST", `dealing/positions/${ticket}/price-correction`, { openPrice, ...reasonBody(reason) });
  }

  adjustCharges(ticket: string, patch: { swap?: number; commission?: number }, reason: Reason) {
    return this.write<null>("POST", `dealing/positions/${ticket}/charges`, { ...patch, ...reasonBody(reason) });
  }

  voidPosition(ticket: string, reason: Reason) {
    return this.write<null>("POST", `dealing/positions/${ticket}/void`, { ...reasonBody(reason) }, () => this.reloadDeals());
  }

  reopenDeal(dealId: string, reason: Reason) {
    return this.write<{ ticket: string }>("POST", `dealing/deals/${dealId}/reopen`, { ...reasonBody(reason) }, () => this.reloadDeals());
  }

  transferBook(tickets: string[], to: Book, reason: Reason, opts?: { volume?: number; pct?: number }) {
    return this.write<BulkOutcome & { created: string[] }>("POST", "dealing/book-transfers", { tickets, to, volume: opts?.volume, pct: opts?.pct, ...reasonBody(reason) });
  }

  modifyOrder(ticket: string, patch: OrderPatch, reason: Reason) {
    return this.write<null>("PATCH", `dealing/orders/${ticket}`, { ...patch, ...reasonBody(reason) });
  }

  cancelOrders(tickets: string[], reason: Reason) {
    return this.write<BulkOutcome>("POST", "dealing/orders/cancel", { tickets, ...reasonBody(reason) });
  }

  fillOrder(ticket: string, reason: Reason) {
    return this.write<{ ticket: string }>("POST", `dealing/orders/${ticket}/fill`, { ...reasonBody(reason) });
  }

  setSymbolControl(symbol: string, group: TradingGroup | "all", mode: ControlMode | null, reason: Reason) {
    return this.write<null>("PUT", `dealing/controls/symbols/${encodeURIComponent(symbol)}`, { group, mode, ...reasonBody(reason) }, () => this.reloadControls());
  }

  setAccountControl(login: string, patch: Partial<Omit<AccountControl, "login" | "clientId" | "group" | "setBy" | "updated" | "reason">>, reason: Reason) {
    return this.write<null>("PUT", `dealing/controls/accounts/${login}`, { ...patch, ...reasonBody(reason) }, () => this.reloadControls());
  }

  setTenantPolicy(patch: Partial<TenantPolicy>, reason: Reason) {
    return this.write<null>("PUT", "dealing/controls/tenant", { ...patch, ...reasonBody(reason) }, () => this.reloadControls());
  }

  saveRoutingRules(rules: RoutingRule[], reason: Reason, summary: string) {
    return this.write<null>("PUT", "dealing/routing/rules", { rules, summary, ...reasonBody(reason) }, () => this.reloadControls());
  }

  quickRoute(scope: { login: string } | { group: string }, book: Book | null, reason: Reason) {
    return this.write<null>("PUT", "dealing/routing/quick", { ...scope, book, ...reasonBody(reason) }, () => this.reloadControls());
  }

  async reset(): Promise<DeskResult> {
    return { ok: false, error: "Resetting is only available in the demo." };
  }

  /** Older audit entries (id cursor). */
  async loadOlderAudit() {
    const last = this.state.audit[this.state.audit.length - 1];
    if (!last) return 0;
    const r = await call<Raw[]>("GET", `dealing/audit?limit=500&before=${auditSeq(last)}`);
    if (!r.ok) return 0;
    const older = r.data.map(normAudit);
    this.mergeAudit(older);
    return older.length;
  }
}

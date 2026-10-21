import type { AdminOrder, AdminPosition, OrderSource, RoutingRule, TradingGroup } from "@kalks/mock/admin-trading";

export type Book = "A" | "B";
export type Side = "buy" | "sell";

/** Who performed an action (the signed-in staff member). */
export interface StaffRef {
  id: number | string;
  name: string;
  role: string;
}

/** Mandatory reason on every dealer action (D117). `code` is "DLR-02 · Error correction". */
export interface Reason {
  code: string;
  note?: string;
}

/** One routing change on a ticket: opened on a book, moved, split in/out. */
export interface RouteEvent {
  at: string;
  kind: "open" | "transfer" | "split-out" | "split-in";
  from: Book | null;
  to: Book;
  volume: number;
  /** market price at the moment of the change; P&L is attributed to the new book from here */
  price: number;
  staff: string;
  reason: string;
  relatedTicket?: string;
}

export interface DeskPosition extends AdminPosition {
  /** when the current book took over the risk, and at what price */
  bookSince: string;
  bookPrice: number;
  /** client price-P&L realised on earlier book segments (before the last transfer), per book */
  bookCarry: { A: number; B: number };
  routeHistory: RouteEvent[];
  childTickets?: string[];
  /** open price was changed by an error correction (shown as "price correction" on the client statement) */
  priceCorrected?: boolean;
  /** seeded demo row still at mock reference levels — re-based onto live prices once the feed is live */
  refLevel?: boolean;
  /* Live desk (trading engine) only: the engine's own close price and price P&L (account currency),
     refreshed every second by the dealing stream. Demo positions leave them unset. */
  currentPrice?: number | null;
  profit?: number | null;
  trailingPoints?: number | null;
  currency?: "USD" | "USC";
  groupName?: string;
}

export type DealKind = "close" | "partial" | "force" | "stop-out" | "price-correction";

/** A closing deal (full or partial), kept for the client statement and for reopen. */
export interface DeskDeal {
  id: string;
  ticket: string;
  login: string;
  clientId: string;
  symbol: string;
  side: Side;
  volume: number;
  openPrice: number;
  closePrice: number;
  openTime: string;
  closeTime: string;
  /** realised P&L incl. the swap and commission share of the closed volume */
  profit: number;
  book: Book;
  kind: DealKind;
  priceCorrection?: boolean;
  reversed?: boolean;
  staff: string;
  reasonCode: string;
  /* live desk only: order source of the position's open (manual / api / copy / mam / pamm / dealer / system …) and of the close */
  source?: string;
  closeSource?: string;
  /* live desk only: price P&L, swap and commission share of the deal, engine close reason, account currency */
  priceProfit?: number;
  swap?: number;
  commission?: number;
  reason?: string;
  currency?: "USD" | "USC";
  /** the position as it was (with the closed volume) — used to reopen */
  snapshot: DeskPosition;
}

export interface DeskOrder extends AdminOrder {
  book?: Book;
  refLevel?: boolean;
}

export type ControlMode = "halt" | "close-only";

export interface SymbolControl {
  id: string;
  symbol: string;
  group: TradingGroup | "all";
  mode: ControlMode;
  reasonCode: string;
  note?: string;
  staff: string;
  at: string;
}

export interface AccountControl {
  login: string;
  clientId: string;
  group: string;
  tradingDisabled: boolean;
  closeOnly: boolean;
  maxLot: number;
  execDelayMs: number;
  markupPips: number;
  reason: string;
  setBy: string;
  updated: string;
}

export interface TenantPolicy {
  /** D115 — per-tenant switch; when off, per-account execution delays are ignored */
  execDelayEnabled: boolean;
  execDelayCapMs: number;
  marginCallPct: number;
  stopOutPct: number;
}

export type AuditAction =
  | "position.open"
  | "position.modify"
  | "position.partial_close"
  | "position.close"
  | "position.force_close"
  | "position.stop_out"
  | "position.add_volume"
  | "position.price_correction"
  | "position.adjust_charges"
  | "position.void"
  | "deal.reopen"
  | "book.transfer"
  | "book.split"
  | "order.place"
  | "order.modify"
  | "order.cancel"
  | "order.fill"
  | "control.symbol"
  | "control.account"
  | "control.tenant"
  | "routing.rule"
  | "trade.rejected"
  | "desk.reset"
  // trading engine account ops (live desk)
  | "account.balance"
  | "account.credit"
  | "account.status"
  | "account.group"
  | "account.leverage"
  | "account.rejected"
  | "group.create"
  | "group.update"
  // risk desk (trading engine)
  | "risk.limits"
  | "risk.settings";

export interface AuditEntry {
  readonly id: string;
  readonly at: string;
  readonly staff: StaffRef;
  readonly action: AuditAction;
  readonly tickets: readonly string[];
  readonly login?: string;
  readonly symbol?: string;
  readonly before?: Readonly<Record<string, unknown>> | null;
  readonly after?: Readonly<Record<string, unknown>> | null;
  readonly reasonCode: string;
  readonly note: string;
  /** e.g. "price correction", "manual price", "forced", "execution delay 250 ms" */
  readonly flags?: readonly string[];
}

export interface DeskState {
  version: number;
  positions: DeskPosition[];
  orders: DeskOrder[];
  deals: DeskDeal[];
  audit: AuditEntry[];
  symbolControls: SymbolControl[];
  accountControls: AccountControl[];
  routingRules: RoutingRule[];
  /** realised P&L and charge adjustments per login since the seed (added to balance) */
  balanceAdj: Record<string, number>;
  tenant: TenantPolicy;
  seq: { position: number; order: number; deal: number; audit: number; control: number };
}

export type DeskResult<T = null> = { ok: true; data: T; audit: AuditEntry[] } | { ok: false; error: string; audit?: AuditEntry[] };

export type TradeType = "market" | "limit" | "stop" | "stop-limit";

export interface CreateTradeInput {
  login: string;
  symbol: string;
  side: Side;
  type: TradeType;
  volume: number;
  /** market: manual fill price (needs a note); pending: order / stop price */
  price?: number;
  /** stop-limit: the limit price */
  stopLimit?: number;
  sl?: number;
  tp?: number;
  /** defaults to the routing rules */
  book?: Book;
  comment?: string;
  expiry?: "GTC" | "Today" | string;
}

export interface OrderPatch {
  price?: number;
  stopLimit?: number;
  volume?: number;
  sl?: number | null;
  tp?: number | null;
  expiry?: string;
}

export interface BulkOutcome {
  done: string[];
  failed: { ticket: string; error: string }[];
}

/**
 * The dealing-desk contract. The Back Office talks to this interface only; today it is implemented
 * client-side (LocalTradingDesk, localStorage), later by the trading-engine REST API (see store.ts).
 */
export interface TradingDeskApi {
  getState(): DeskState;
  subscribe(fn: () => void): () => void;
  setActor(staff: StaffRef): void;

  createTrade(input: CreateTradeInput, reason: Reason): Promise<DeskResult<{ ticket: string; kind: "position" | "order"; price: number; book: Book; delayMs: number }>>;
  modifyPosition(ticket: string, patch: { sl?: number | null; tp?: number | null }, reason: Reason): Promise<DeskResult>;
  modifyMany(tickets: string[], rule: { slPct?: number | null; tpPct?: number | null; clear?: "sl" | "tp" | "both" }, reason: Reason): Promise<DeskResult<BulkOutcome>>;
  partialClose(ticket: string, volume: number, reason: Reason, opts?: { price?: number }): Promise<DeskResult<{ dealId: string; profit: number }>>;
  closePosition(ticket: string, reason: Reason, opts?: { price?: number; force?: boolean; stopOut?: boolean }): Promise<DeskResult<{ dealId: string; profit: number }>>;
  closeMany(tickets: string[], reason: Reason, opts?: { force?: boolean }): Promise<DeskResult<BulkOutcome & { profit: number }>>;
  addVolume(ticket: string, volume: number, reason: Reason): Promise<DeskResult>;
  priceCorrection(ticket: string, openPrice: number, reason: Reason): Promise<DeskResult>;
  adjustCharges(ticket: string, patch: { swap?: number; commission?: number }, reason: Reason): Promise<DeskResult>;
  voidPosition(ticket: string, reason: Reason): Promise<DeskResult>;
  reopenDeal(dealId: string, reason: Reason): Promise<DeskResult<{ ticket: string }>>;
  transferBook(tickets: string[], to: Book, reason: Reason, opts?: { volume?: number; pct?: number }): Promise<DeskResult<BulkOutcome & { created: string[] }>>;

  modifyOrder(ticket: string, patch: OrderPatch, reason: Reason): Promise<DeskResult>;
  cancelOrders(tickets: string[], reason: Reason): Promise<DeskResult<BulkOutcome>>;
  fillOrder(ticket: string, reason: Reason): Promise<DeskResult<{ ticket: string }>>;

  setSymbolControl(symbol: string, group: TradingGroup | "all", mode: ControlMode | null, reason: Reason): Promise<DeskResult>;
  setAccountControl(login: string, patch: Partial<Omit<AccountControl, "login" | "clientId" | "group" | "setBy" | "updated" | "reason">>, reason: Reason): Promise<DeskResult>;
  setTenantPolicy(patch: Partial<TenantPolicy>, reason: Reason): Promise<DeskResult>;
  saveRoutingRules(rules: RoutingRule[], reason: Reason, summary: string): Promise<DeskResult>;
  quickRoute(scope: { login: string } | { group: string }, book: Book | null, reason: Reason): Promise<DeskResult>;

  reset(reason: Reason): Promise<DeskResult>;
}

export type { OrderSource };

// Engine JSON shapes as the terminal BFF returns them (services/trading/README.md, dealing fields removed).

export interface EngAccount {
  login: number;
  type: "live" | "demo";
  group: string;
  groupName: string;
  spreadGroup?: string;
  mode: "hedging" | "netting";
  cent: boolean;
  currency: "USD" | "USC";
  leverage: number;
  leverages?: number[];
  status: string;
  name?: string;
  marginCall?: boolean;
  marginCallLevel?: number;
  stopOutLevel?: number;
  controls?: { tradingDisabled: boolean; closeOnly: boolean; maxLot: number | null };
  balance: number;
  credit: number;
  bonus: number;
  profit: number;
  swap: number;
  equity: number;
  margin: number;
  freeMargin: number;
  marginLevel: number | null;
  withdrawable?: number;
  demo?: { initialBalance: number; refillsPerDay: number; refillsUsedToday: number; expiryDays: number } | null;
  createdAt: string;
  /** What the account trades: its group's product (CFD / Options account split; absent on older engines = CFD). */
  product?: "cfd" | "options";
}

export interface EngPosition {
  ticket: number;
  login: number;
  symbol: string;
  side: "buy" | "sell";
  volume: number;
  openPrice: number;
  openTime: string;
  sl: number | null;
  tp: number | null;
  trailingPoints: number | null;
  swap: number;
  commission: number;
  currentPrice?: number;
  profit?: number;
  source: string;
  platform?: string;
  comment?: string;
  reversedFrom?: number | null;
}

export interface EngOrder {
  ticket: number;
  login: number;
  symbol: string;
  side: "buy" | "sell";
  type: "limit" | "stop" | "stop_limit";
  volume: number;
  price: number;
  stopLimit: number | null;
  triggered: boolean;
  sl: number | null;
  tp: number | null;
  trailingPoints: number | null;
  expiry: string;
  expiryAt: string | null;
  oco: number | null;
  source: string;
  platform?: string;
  comment?: string;
  placedAt: string;
  clientOrderId?: string | null;
}

export interface EngDeal {
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
  openPrice: number;
  openTime: string;
  source: string;
  comment?: string;
  reversed?: boolean;
}

export interface EngState {
  account: EngAccount;
  positions: EngPosition[];
  orders: EngOrder[];
  history: { deals: EngDeal[] };
  readOnly: boolean;
  serverTime: string;
}

export interface EngNotification {
  kind: string;
  message: string;
  data?: Record<string, unknown>;
}

export interface EngEquity {
  login: number;
  balance: number;
  credit: number;
  bonus: number;
  profit: number;
  swap: number;
  equity: number;
  margin: number;
  freeMargin: number;
  marginLevel: number | null;
  withdrawable?: number;
  /** options also carry the account's mark (premium per unit) and the position's Greeks (delta / gamma in contracts,
   *  vega / theta in USD) */
  positions: { ticket: number; price: number; profit: number; swap: number; mark?: number | null; greeks?: { delta?: number; gamma?: number; vega?: number; theta?: number } | null }[];
}

export type StreamFrame =
  | { type: "snapshot"; readOnly: boolean; account: EngAccount; positions: EngPosition[]; orders: EngOrder[] }
  | { type: "position"; op: "upsert"; position: EngPosition }
  | { type: "position"; op: "remove"; ticket: number }
  | { type: "order"; op: "upsert"; order: EngOrder }
  | { type: "order"; op: "remove"; ticket: number; status: string; reason?: string }
  | { type: "deal"; deal: EngDeal }
  | { type: "ledger"; txn: { id: number; kind: string; amount: number; at: string } }
  | { type: "account"; account: EngAccount }
  | ({ type: "notification" } & EngNotification)
  | ({ type: "equity" } & EngEquity)
  | { type: "hb"; t: number }
  | { type: "resync"; skipped?: number };

export interface SessionInfo {
  login: string;
  readOnly: boolean;
  expiresAt: string;
  account: EngAccount | null;
}

//! Domain types. These are also the event payloads (JSON in `events.payload`), so decimals serialise as
//! strings here (exact); the HTTP API renders its own views with numbers (see `api::views`).

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::money::{D, ONE, ZERO};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn sign(self) -> D {
        match self {
            Side::Buy => ONE,
            Side::Sell => -ONE,
        }
    }
    pub fn opposite(self) -> Side {
        match self {
            Side::Buy => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Side::Buy => "buy",
            Side::Sell => "sell",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    Market,
    Limit,
    Stop,
    StopLimit,
}

impl OrderType {
    pub fn as_str(self) -> &'static str {
        match self {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Stop => "stop",
            OrderType::StopLimit => "stop_limit",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expiry {
    Gtc,
    /// Until the end of the current server day (next rollover).
    Today,
    /// Until `expiry_at`.
    Date,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Book {
    A,
    B,
}

impl Book {
    pub fn as_str(self) -> &'static str {
        match self {
            Book::A => "A",
            Book::B => "B",
        }
    }
    pub fn parse(s: &str) -> Option<Book> {
        match s {
            "A" | "a" => Some(Book::A),
            "B" | "b" => Some(Book::B),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Netting,
    Hedging,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    Live,
    Demo,
}

impl AccountKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountKind::Live => "live",
            AccountKind::Demo => "demo",
        }
    }
    pub fn parse(s: &str) -> Option<AccountKind> {
        match s.trim().to_ascii_lowercase().as_str() {
            "live" => Some(AccountKind::Live),
            "demo" => Some(AccountKind::Demo),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Active,
    /// No trading at all (login still works, read-only view).
    Disabled,
    /// Only closing / reducing trades.
    CloseOnly,
    /// View only; every trade request is rejected.
    ReadOnly,
    /// Demo account past its expiry.
    Expired,
    /// Hidden from the client's lists, restorable (client or staff). History and ledger are kept.
    Archived,
    /// Closed permanently: final for the client; the login is never reused.
    Closed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Active => "active",
            Status::Disabled => "disabled",
            Status::CloseOnly => "close_only",
            Status::ReadOnly => "read_only",
            Status::Expired => "expired",
            Status::Archived => "archived",
            Status::Closed => "closed",
        }
    }
    /// Archived or closed: no login, no money in, no trading, not counted towards the account limit.
    pub fn is_retired(self) -> bool {
        matches!(self, Status::Archived | Status::Closed)
    }
    pub fn parse(s: &str) -> Option<Status> {
        Some(match s {
            "active" => Status::Active,
            "disabled" => Status::Disabled,
            "close_only" | "close-only" => Status::CloseOnly,
            "read_only" | "read-only" => Status::ReadOnly,
            "expired" => Status::Expired,
            "archived" => Status::Archived,
            "closed" => Status::Closed,
            _ => return None,
        })
    }
}

/// D84: every order carries a source tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Manual,
    Api,
    Fix,
    Webhook,
    Strategy,
    Copy,
    Pamm,
    /// Allocated by a MAM manager's block trade onto a linked client account.
    Mam,
    Ai,
    Dealer,
    /// Engine-initiated (SL/TP, stop-out, expiry, rollover).
    System,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Manual => "manual",
            Source::Api => "api",
            Source::Fix => "fix",
            Source::Webhook => "webhook",
            Source::Strategy => "strategy",
            Source::Copy => "copy",
            Source::Pamm => "pamm",
            Source::Mam => "mam",
            Source::Ai => "ai",
            Source::Dealer => "dealer",
            Source::System => "system",
        }
    }
    /// Sources a client-side caller (terminal, public API, algo) may claim.
    pub fn parse_client(s: &str) -> Option<Source> {
        Some(match s {
            "manual" => Source::Manual,
            "api" => Source::Api,
            "fix" => Source::Fix,
            "webhook" => Source::Webhook,
            "strategy" => Source::Strategy,
            "copy" => Source::Copy,
            "pamm" => Source::Pamm,
            "ai" => Source::Ai,
            _ => return None,
        })
    }
}

/// Dealer controls on one account (D115).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Controls {
    pub trading_disabled: bool,
    pub close_only: bool,
    pub max_lot: Option<D>,
    pub exec_delay_ms: u32,
    /// Extra spread in pips on top of the group spread (split half on bid, half on ask).
    pub markup_pips: D,
    pub reason: String,
    pub set_by: String,
    pub updated: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DemoCfg {
    pub initial_balance: D,
    pub refills_per_day: u32,
    pub expiry_days: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub tenant_id: i64,
    pub login: i64,
    /// Gateway client id (users.id).
    pub user_id: i64,
    pub kind: AccountKind,
    pub group: String,
    pub mode: Mode,
    /// Cent account: the ledger currency is USC (= USD × 100); lot sizes are unchanged.
    pub cent: bool,
    pub leverage: u32,
    pub status: Status,
    pub name: String,
    pub route_override: Option<Book>,
    pub controls: Controls,
    pub demo: Option<DemoCfg>,
    pub created_at: DateTime<Utc>,
    /// Archive / close bookkeeping (absent on accounts that were never retired; old events lack it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle: Option<Lifecycle>,
}

/// Why and by whom an account was archived, and what to restore it to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lifecycle {
    pub prior_status: Status,
    pub archived_at: DateTime<Utc>,
    #[serde(default)]
    pub reason_code: String,
    /// `user:{id}` or `staff:{id}`.
    #[serde(default)]
    pub by: String,
    /// The client may restore it from the Client Area (false when staff archived it, unless they allowed it).
    #[serde(default)]
    pub client_restorable: bool,
    /// Set when the account was closed permanently (status `closed`); `archived_at` is then when it was retired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<DateTime<Utc>>,
}

impl Account {
    /// Ledger / display currency.
    pub fn ccy(&self) -> &'static str {
        if self.cent { "USC" } else { "USD" }
    }
    /// USD → account currency factor.
    pub fn usd_factor(&self) -> D {
        if self.cent { D::ONE_HUNDRED } else { ONE }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trailing {
    /// Distance from the market in points.
    pub distance_points: i64,
    /// Minimum SL move in points (throttles updates).
    pub step_points: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RouteEvent {
    pub at: DateTime<Utc>,
    /// open | transfer | split-out | split-in
    pub kind: String,
    pub from: Option<Book>,
    pub to: Book,
    pub volume: D,
    pub price: D,
    pub staff: String,
    pub reason: String,
    pub related_ticket: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub ticket: i64,
    pub login: i64,
    pub symbol: String,
    pub side: Side,
    pub volume: D,
    pub open_price: D,
    pub open_time: DateTime<Utc>,
    pub sl: Option<D>,
    pub tp: Option<D>,
    pub trailing: Option<Trailing>,
    /// Accrued swap (account currency, unrealised until close).
    pub swap: D,
    /// Commission charged at open for the remaining volume (informational; already on the balance).
    pub commission: D,
    pub source: Source,
    pub platform: String,
    pub comment: String,
    pub book: Book,
    pub order_ticket: i64,
    pub parent_ticket: Option<i64>,
    pub child_tickets: Vec<i64>,
    pub book_since: DateTime<Utc>,
    pub book_price: D,
    /// Client price P&L realised on earlier book segments, per book (account currency).
    pub book_carry_a: D,
    pub book_carry_b: D,
    pub route_history: Vec<RouteEvent>,
    pub price_corrected: bool,
    pub last_swap_day: Option<NaiveDate>,
    pub client_order_id: Option<String>,
    /// Set when this position was opened by a netting reversal of `ticket`.
    pub reversed_from: Option<i64>,
    /// Kalks FX Options: the contract this position holds (None = a CFD position). `symbol` is then the series
    /// code, `volume` the contracts and `open_price` the premium per unit of the underlying (quote currency).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<OptionTerms>,
    /// Options: the multi-leg order (strategy) this leg was opened with; legs of one combo close together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combo_id: Option<i64>,
    /// Options: signed premium cash booked at open for the remaining contracts (account currency: paid −,
    /// received +). Realised P&L = cash at close / expiry + this basis.
    #[serde(default, skip_serializing_if = "D::is_zero")]
    pub premium: D,
    /// Options: where the contract trades. None / `house` = Kalks-priced (B-book, the house is the counterparty);
    /// `book` = the options order book (another account is the counterparty, cash through the expiry's clearing
    /// account). Absent on every event written before the order book existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub venue: Option<Venue>,
}

impl Position {
    pub fn is_option(&self) -> bool {
        self.option.is_some()
    }
    /// An option position held on the options order book (docs/OPTIONS-EXCHANGE.md).
    pub fn on_book(&self) -> bool {
        self.venue == Some(Venue::Book)
    }
}

/// Where an option position trades.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Venue {
    House,
    Book,
}

impl Venue {
    pub fn as_str(self) -> &'static str {
        match self {
            Venue::House => "house",
            Venue::Book => "book",
        }
    }
}

/// Call or put.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptRight {
    Call,
    Put,
}

impl OptRight {
    pub fn as_str(self) -> &'static str {
        match self {
            OptRight::Call => "call",
            OptRight::Put => "put",
        }
    }
    pub fn parse(s: &str) -> Option<OptRight> {
        match s.to_ascii_lowercase().as_str() {
            "call" | "c" => Some(OptRight::Call),
            "put" | "p" => Some(OptRight::Put),
            _ => None,
        }
    }
    /// Intrinsic value per unit at `s`.
    pub fn intrinsic(self, s: D, strike: D) -> D {
        match self {
            OptRight::Call => (s - strike).max(ZERO),
            OptRight::Put => (strike - s).max(ZERO),
        }
    }
}

/// Barrier type: up/down and out/in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BarrierKind {
    UO,
    DO,
    UI,
    DI,
}

impl BarrierKind {
    pub fn as_str(self) -> &'static str {
        match self {
            BarrierKind::UO => "UO",
            BarrierKind::DO => "DO",
            BarrierKind::UI => "UI",
            BarrierKind::DI => "DI",
        }
    }
    pub fn parse(s: &str) -> Option<BarrierKind> {
        match s.to_ascii_uppercase().as_str() {
            "UO" | "UP_OUT" | "UP-AND-OUT" => Some(BarrierKind::UO),
            "DO" | "DOWN_OUT" | "DOWN-AND-OUT" => Some(BarrierKind::DO),
            "UI" | "UP_IN" | "UP-AND-IN" => Some(BarrierKind::UI),
            "DI" | "DOWN_IN" | "DOWN-AND-IN" => Some(BarrierKind::DI),
            _ => None,
        }
    }
    pub fn is_up(self) -> bool {
        matches!(self, BarrierKind::UO | BarrierKind::UI)
    }
    pub fn is_in(self) -> bool {
        matches!(self, BarrierKind::UI | BarrierKind::DI)
    }
    /// Has the underlying reached the barrier?
    pub fn hit(self, spot: D, level: D) -> bool {
        if self.is_up() { spot >= level } else { spot <= level }
    }
}

/// Knock-in / knock-out terms of a barrier option (continuous monitoring on the raw mid).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BarrierTerms {
    pub kind: BarrierKind,
    pub level: D,
    /// Cash rebate per unit of the underlying (quote currency): a knock-out pays it at the hit, a knock-in that
    /// never knocked in pays it at expiry.
    #[serde(default)]
    pub rebate: D,
    #[serde(default)]
    pub knocked_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knocked_at: Option<DateTime<Utc>>,
    /// Raw mid that knocked it in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knock_spot: Option<D>,
}

/// The contract of an option position (copied from the options service snapshot when the trade is made, so a
/// position stays self-describing after its series leaves the snapshot).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionTerms {
    /// Vanilla series code, e.g. `EURUSD-20261009-1.1650-C`.
    pub series: String,
    pub underlying: String,
    pub right: OptRight,
    pub strike: D,
    /// Expiry date and the cut instant (European, cash-settled at the fixing).
    pub expiry: NaiveDate,
    pub expiry_at: DateTime<Utc>,
    /// Units of the underlying per contract.
    pub contract_size: D,
    /// Premium currency (the underlying's quote currency).
    pub quote_ccy: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub barrier: Option<BarrierTerms>,
}

impl OptionTerms {
    /// `SYMBOL:YYYY-MM-DD` (the options service's expiry key).
    pub fn expiry_key(&self) -> String {
        format!("{}:{}", self.underlying, self.expiry)
    }
    pub fn style(&self) -> &'static str {
        if self.barrier.is_some() { "barrier" } else { "vanilla" }
    }
    /// A barrier that has not knocked in / out yet (it still prices as a barrier).
    pub fn alive_barrier(&self) -> Option<&BarrierTerms> {
        self.barrier.as_ref().filter(|b| !(b.kind.is_in() && b.knocked_in))
    }
    /// Payoff per unit at expiry for fixing `f` (a knock-in that never knocked in pays its rebate).
    pub fn payoff(&self, f: D) -> D {
        match &self.barrier {
            Some(b) if b.kind.is_in() && !b.knocked_in => b.rebate,
            _ => self.right.intrinsic(f, self.strike),
        }
    }
}

/// An underlying price condition that arms a pending order (raw mid).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    pub symbol: String,
    pub op: TriggerOp,
    pub price: D,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerOp {
    Above,
    Below,
}

impl TriggerOp {
    pub fn as_str(self) -> &'static str {
        match self {
            TriggerOp::Above => "above",
            TriggerOp::Below => "below",
        }
    }
    pub fn fired(self, px: D, level: D) -> bool {
        match self {
            TriggerOp::Above => px >= level,
            TriggerOp::Below => px <= level,
        }
    }
}

/// One leg of a pending option order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptLeg {
    pub terms: OptionTerms,
    pub side: Side,
    pub contracts: D,
}

/// A pending option order: its legs fill together (all or nothing).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionOrder {
    pub legs: Vec<OptLeg>,
    /// Limit on the premium: a single leg buys at or below it (sells at or above it); a multi-leg order fills
    /// when its net debit per combo unit is at or below it (a credit order: its net credit at or above it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_premium: Option<D>,
    /// Order-book stop order (`stop_market` / `stop_limit`, docs/OPTIONS-EXCHANGE.md §2): when its trigger fires it
    /// becomes a book order (market IOC, or a limit at `limit_premium`). Never filled at the house price.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book: Option<BookStop>,
}

/// What an order-book stop watches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopSource {
    /// The series' mark (model mid clamped inside the book), never the last trade.
    Mark,
    /// The underlying's raw mid.
    Underlying,
}

impl StopSource {
    pub fn as_str(self) -> &'static str {
        match self {
            StopSource::Mark => "mark",
            StopSource::Underlying => "underlying",
        }
    }
}

/// Terms of an order-book stop order (kept on the account `Order`, so it survives restarts in the event stream).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BookStop {
    pub source: StopSource,
    pub op: TriggerOp,
    /// Trigger level: a premium per unit (source mark) or an underlying price.
    pub trigger: D,
    /// Time in force of the order it becomes (`ioc` for stop_market; gtc / gtd / ioc / fok for stop_limit).
    pub tif: String,
    #[serde(default)]
    pub post_only: bool,
    #[serde(default)]
    pub reduce_only: bool,
    /// Good-till-date of the resulting limit order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_at: Option<DateTime<Utc>>,
}

/// Option facts of a deal.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DealOption {
    pub terms: OptionTerms,
    /// Signed cash booked on the balance by this deal, commission excluded (account currency): premium paid −,
    /// premium received +, sale / buy-back proceeds, expiry payout, knock-out rebate.
    pub cash: D,
    /// Quote currency → USD rate used for the cash.
    pub usd_per_quote: D,
    /// Raw mid of the underlying at the fill (fixing for an expiry).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spot: Option<D>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixing: Option<D>,
    /// Fixing run of an expiry settlement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combo_id: Option<i64>,
    /// Commission charged by this deal (an exit deal's `commission` also carries the closed share of the entry
    /// commission, like CFD exits).
    #[serde(default)]
    pub charged: D,
    /// Order-book fill this deal comes from (absent on house-priced deals).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<FillRef>,
    /// Maker rebate credited with this deal (account currency, ≥ 0; order book only).
    #[serde(default, skip_serializing_if = "D::is_zero")]
    pub rebate: D,
}

/// The order-book fill behind a deal: `option.fill = {id, role, kind, combo}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FillRef {
    pub id: String,
    /// `maker` | `taker`.
    pub role: String,
    /// `book` | `rfq` | `liquidation` | `backstop` | `novation`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combo: Option<i64>,
    /// The book order (engine ticket) that traded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Order {
    pub ticket: i64,
    pub login: i64,
    pub symbol: String,
    pub side: Side,
    pub kind: OrderType,
    pub volume: D,
    /// Limit / stop price (for stop-limit: the stop trigger price).
    pub price: D,
    /// Stop-limit: the limit price used after the stop triggers.
    pub stop_limit: Option<D>,
    pub sl: Option<D>,
    pub tp: Option<D>,
    pub trailing: Option<Trailing>,
    pub expiry: Expiry,
    pub expiry_at: Option<DateTime<Utc>>,
    /// One-cancels-other partner ticket.
    pub oco: Option<i64>,
    pub source: Source,
    pub platform: String,
    pub comment: String,
    /// Dealer-chosen book (None = routing rules at fill time).
    pub book: Option<Book>,
    pub placed_at: DateTime<Utc>,
    /// Stop-limit whose stop has triggered: now a limit order at `stop_limit`.
    pub triggered: bool,
    pub client_order_id: Option<String>,
    /// Kalks FX Options pending order (legs, premium limit). `symbol` is then the first leg's series code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<OptionOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combo_id: Option<i64>,
    /// Arms the order when the underlying's raw mid crosses a level (`triggered` turns true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<Trigger>,
}

impl Order {
    pub fn is_option(&self) -> bool {
        self.option.is_some()
    }
    /// The price the order currently waits for.
    pub fn active_price(&self) -> D {
        if self.kind == OrderType::StopLimit && self.triggered { self.stop_limit.unwrap_or(self.price) } else { self.price }
    }
    /// Effective behaviour now (a triggered stop-limit behaves as a limit).
    pub fn active_kind(&self) -> OrderType {
        if self.kind == OrderType::StopLimit && self.triggered { OrderType::Limit } else { self.kind }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Filled,
    Cancelled,
    Expired,
    /// Rejected at trigger time (e.g. not enough margin).
    Rejected,
}

impl OrderStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            OrderStatus::Filled => "filled",
            OrderStatus::Cancelled => "cancelled",
            OrderStatus::Expired => "expired",
            OrderStatus::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealEntry {
    In,
    Out,
    /// Closed by an opposite position (Close By).
    OutBy,
}

impl DealEntry {
    pub fn as_str(self) -> &'static str {
        match self {
            DealEntry::In => "in",
            DealEntry::Out => "out",
            DealEntry::OutBy => "out_by",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealReason {
    Client,
    Dealer,
    Sl,
    Tp,
    StopOut,
    CloseBy,
    Reversal,
    Force,
    PriceCorrection,
    PendingFill,
    /// Option settled at expiry against the fixing.
    Expiry,
    /// Barrier option knocked out (closed at its rebate).
    KnockOut,
    /// A house-priced option moved to the order book (the client keeps the position and its P&L).
    Novation,
}

impl DealReason {
    pub fn as_str(self) -> &'static str {
        match self {
            DealReason::Client => "client",
            DealReason::Dealer => "dealer",
            DealReason::Sl => "sl",
            DealReason::Tp => "tp",
            DealReason::StopOut => "stop_out",
            DealReason::CloseBy => "close_by",
            DealReason::Reversal => "reversal",
            DealReason::Force => "force",
            DealReason::PriceCorrection => "price_correction",
            DealReason::PendingFill => "pending_fill",
            DealReason::Expiry => "expiry",
            DealReason::KnockOut => "knock_out",
            DealReason::Novation => "novation",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Deal {
    pub id: i64,
    pub login: i64,
    pub position_ticket: i64,
    pub order_ticket: Option<i64>,
    pub symbol: String,
    /// Direction of the deal itself (closing a buy is a sell deal).
    pub side: Side,
    /// Direction of the position it belongs to.
    pub position_side: Side,
    pub entry: DealEntry,
    pub volume: D,
    pub price: D,
    /// Price P&L (account currency), 0 for entry deals.
    pub profit: D,
    /// Swap realised with this deal.
    pub swap: D,
    /// Commission charged (entry) or the closed share of the entry commission (exit, informational).
    pub commission: D,
    pub reason: DealReason,
    pub book: Book,
    pub time: DateTime<Utc>,
    pub open_price: D,
    pub open_time: DateTime<Utc>,
    pub source: Source,
    pub comment: String,
    pub price_correction: bool,
    pub ledger_txn: Option<i64>,
    pub staff: Option<String>,
    pub reason_code: Option<String>,
    /// The position as it was for the closed volume (exit deals) — used by "reopen deal".
    pub snapshot: Option<Box<Position>>,
    /// Client-supplied id of the order that produced this deal (duplicate-submission guard).
    pub client_order_id: Option<String>,
    /// Exit deal that left part of the position open.
    #[serde(default)]
    pub partial: bool,
    /// Kalks FX Options deal: terms, the cash booked, fixing / run of a settlement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<DealOption>,
}

impl Deal {
    /// Where the trade came from: the source of the order that OPENED the position (the snapshot an exit
    /// deal keeps), so a copied position closed by its SL or a MAM trade the manager closed is still `copy` /
    /// `mam`, not `system` / `manual`. Entry deals (and the few exits without a snapshot) carry their own source.
    pub fn origin(&self) -> Source {
        self.snapshot.as_ref().map_or(self.source, |p| p.source)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxnKind {
    /// Initial demo balance.
    DemoInitial,
    DemoRefill,
    /// Wallet → trading account.
    TransferIn,
    /// Trading account → wallet.
    TransferOut,
    /// Staff deposit / withdrawal booked directly on the account.
    Deposit,
    Withdrawal,
    TradePnl,
    Commission,
    Swap,
    Adjustment,
    Credit,
    Bonus,
    /// Negative balance protection write-off.
    Nbp,
    /// Reversal of an earlier transaction (reopened deal, void).
    Reversal,
    /// Copy / PAMM performance fee crystallised at the period end (D66): balance → `house:perf_fees`.
    #[serde(rename = "perf_fee")]
    PerformanceFee,
    /// Platform capital booked on (or withdrawn from) a house account (services/algo "House accounts"):
    /// `house:house_capital` ↔ balance. Not a client deposit: deposit / FTD reports never count it.
    HouseCapital,
    /// Kalks FX Options premium paid / received (opens and closes): balance ↔ `house:options_premium`.
    OptionPremium,
    /// Kalks FX Options expiry payout or knock-out rebate: balance ↔ `house:options_settlement`.
    OptionSettlement,
    /// Options order book maker rebate: `house:options_rebates` → balance.
    OptionRebate,
    /// Stock dividend adjustment on an ex-date (corporate action): `house:dividends` ↔ balance (long credited the
    /// net dividend, short debited the gross).
    Dividend,
}

impl TxnKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TxnKind::DemoInitial => "demo_initial",
            TxnKind::DemoRefill => "demo_refill",
            TxnKind::TransferIn => "transfer_in",
            TxnKind::TransferOut => "transfer_out",
            TxnKind::Deposit => "deposit",
            TxnKind::Withdrawal => "withdrawal",
            TxnKind::TradePnl => "trade_pnl",
            TxnKind::Commission => "commission",
            TxnKind::Swap => "swap",
            TxnKind::Adjustment => "adjustment",
            TxnKind::Credit => "credit",
            TxnKind::Bonus => "bonus",
            TxnKind::Nbp => "nbp",
            TxnKind::Reversal => "reversal",
            TxnKind::PerformanceFee => "perf_fee",
            TxnKind::HouseCapital => "house_capital",
            TxnKind::OptionPremium => "option_premium",
            TxnKind::OptionSettlement => "option_settlement",
            TxnKind::OptionRebate => "option_rebate",
            TxnKind::Dividend => "dividend",
        }
    }
}

/// One leg of a double-entry transaction. Positive = the ledger account's balance goes up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Posting {
    pub account: String,
    pub ccy: String,
    pub amount: D,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LedgerTxn {
    pub id: i64,
    pub tenant_id: i64,
    pub idempotency_key: String,
    pub kind: TxnKind,
    pub login: i64,
    pub reference: Option<String>,
    pub reason_code: Option<String>,
    pub note: Option<String>,
    pub at: DateTime<Utc>,
    pub postings: Vec<Posting>,
}

impl LedgerTxn {
    /// Σ postings per currency must be 0.
    pub fn is_balanced(&self) -> bool {
        let mut by: std::collections::BTreeMap<&str, D> = Default::default();
        for p in &self.postings {
            *by.entry(p.ccy.as_str()).or_insert(ZERO) += p.amount;
        }
        !self.postings.is_empty() && by.values().all(|v| v.is_zero()) && self.postings.iter().all(|p| !p.amount.is_zero())
    }
    /// Net effect on one client sub-ledger (`balance`, `credit`, `bonus`).
    pub fn effect(&self, login: i64, sub: &str) -> D {
        let code = acct_code(login, sub);
        self.postings.iter().filter(|p| p.account == code).map(|p| p.amount).sum()
    }
}

pub fn acct_code(login: i64, sub: &str) -> String {
    format!("acct:{login}:{sub}")
}

pub fn house_code(name: &str, ccy: &str) -> String {
    format!("house:{name}:{ccy}")
}

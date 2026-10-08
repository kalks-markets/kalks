// The trading engine's shapes beyond the account itself (port of apps/crm/components/trading/api.ts and
// option-deal.ts, client-safe subset): groups, positions, pending orders, deals, history and ledger pages, the
// account detail and the open-account result, plus the web's formatting helpers for them. Served by the trading BFF
// under `/api/mobile/trading/...` (the same answers as the web's `/api/trading/...`).
import '../format/format.dart';
import 'account.dart';

double _d(Object? v) => v is num ? v.toDouble() : double.tryParse('${v ?? ''}') ?? 0;
double? _dn(Object? v) => v is num ? v.toDouble() : (v is String ? double.tryParse(v) : null);
int _i(Object? v) => v is num ? v.toInt() : int.tryParse('${v ?? ''}') ?? 0;
int? _in(Object? v) => v is num ? v.toInt() : (v is String ? int.tryParse(v) : null);
String _s(Object? v, [String fallback = '']) => v == null ? fallback : '$v';
Map<String, dynamic>? _map(Object? v) => v is Map ? v.cast<String, dynamic>() : null;
List<Map<String, dynamic>> _maps(Object? v) => [
  for (final x in (v is List ? v : const []))
    if (x is Map) x.cast<String, dynamic>(),
];

/// An account group (`GET trading/groups` -> {groups: [...]}): what the Open account wizard offers.
class EngineGroup {
  const EngineGroup({
    required this.code,
    required this.name,
    required this.mode,
    required this.cent,
    required this.accountTypes,
    required this.leverages,
    required this.defaultLeverage,
    required this.marginCallPct,
    required this.stopOutPct,
    required this.minDeposit,
    required this.swapFree,
    required this.commissionPerLot,
    required this.maxAccountsPerUser,
    required this.demoInitialBalance,
    required this.demoRefillsPerDay,
    required this.demoExpiryDays,
    required this.enabled,
    this.hedgedMarginPct = 0,
    this.spreadGroup = '',
    this.product = 'cfd',
    this.raw = const {},
  });

  final String code, name;

  /// hedging | netting
  final String mode;
  final bool cent;

  /// live | demo | both
  final String accountTypes;
  final List<int> leverages;
  final int defaultLeverage;
  final double marginCallPct, stopOutPct, hedgedMarginPct, minDeposit, commissionPerLot, demoInitialBalance;
  final bool swapFree, enabled;
  final String spreadGroup;
  final int maxAccountsPerUser, demoRefillsPerDay, demoExpiryDays;

  /// cfd | options: what accounts of this group trade.
  final String product;
  final Map<String, dynamic> raw;

  bool get isOptions => product == 'options';

  bool offers(AccountKind kind) => accountTypes == 'both' || accountTypes == kind.name;

  static EngineGroup fromJson(Map<String, dynamic> j) => EngineGroup(
    code: _s(j['code']),
    name: _s(j['name'], _s(j['code'])),
    mode: _s(j['mode'], 'hedging'),
    cent: j['cent'] == true,
    accountTypes: _s(j['accountTypes'], 'both'),
    leverages: [for (final l in (j['leverages'] as List? ?? const [])) _i(l)],
    defaultLeverage: _i(j['defaultLeverage']),
    marginCallPct: _d(j['marginCallPct']),
    stopOutPct: _d(j['stopOutPct']),
    hedgedMarginPct: _d(j['hedgedMarginPct']),
    minDeposit: _d(j['minDeposit']),
    swapFree: j['swapFree'] == true,
    commissionPerLot: _d(j['commissionPerLot']),
    spreadGroup: _s(j['spreadGroup']),
    maxAccountsPerUser: _i(j['maxAccountsPerUser']),
    demoInitialBalance: _d(j['demoInitialBalance']),
    demoRefillsPerDay: _i(j['demoRefillsPerDay']),
    demoExpiryDays: _i(j['demoExpiryDays']),
    enabled: j['enabled'] != false,
    product: productOf(j['product']),
    raw: j,
  );
}

/// An open position (`GET trading/accounts/{login}` -> positions).
class EnginePosition {
  const EnginePosition({
    required this.ticket,
    required this.login,
    required this.symbol,
    required this.side,
    required this.volume,
    required this.openPrice,
    required this.openTime,
    required this.sl,
    required this.tp,
    required this.swap,
    required this.commission,
    required this.currentPrice,
    required this.profit,
    this.source = '',
    this.platform = '',
    this.comment = '',
    this.option,
    this.markValue,
    this.premium,
    this.mark,
  });

  final int ticket, login;
  final String symbol;

  /// buy | sell
  final String side;
  final double volume, openPrice, swap, commission, profit;
  final DateTime? openTime;
  final double? sl, tp, currentPrice;
  final String source, platform, comment;

  /// Kalks FX Options position terms (`symbol` is then the series code, `volume` the contracts).
  final Map<String, dynamic>? option;
  final double? markValue, premium, mark;

  bool get isOption => option != null || isOptionSymbol(symbol);

  static EnginePosition fromJson(Map<String, dynamic> j) => EnginePosition(
    ticket: _i(j['ticket']),
    login: _i(j['login']),
    symbol: _s(j['symbol']),
    side: _s(j['side'], 'buy'),
    volume: _d(j['volume']),
    openPrice: _d(j['openPrice']),
    openTime: DateTime.tryParse(_s(j['openTime'])),
    sl: _dn(j['sl']),
    tp: _dn(j['tp']),
    swap: _d(j['swap']),
    commission: _d(j['commission']),
    currentPrice: _dn(j['currentPrice']),
    profit: _d(j['profit']),
    source: _s(j['source']),
    platform: _s(j['platform']),
    comment: _s(j['comment']),
    option: _map(j['option']),
    markValue: _dn(j['markValue']),
    premium: _dn(j['premium']),
    mark: _dn(j['mark']),
  );
}

/// A pending (or done) order.
class EngineOrder {
  const EngineOrder({
    required this.ticket,
    required this.symbol,
    required this.side,
    required this.type,
    required this.volume,
    required this.price,
    required this.stopLimit,
    required this.sl,
    required this.tp,
    required this.expiry,
    required this.placedAt,
    this.status,
    this.doneAt,
    this.fillPrice,
    this.reason,
    this.option,
  });

  final int ticket;
  final String symbol, side;

  /// market | limit | stop | stop_limit
  final String type;
  final double volume;
  final double? price, stopLimit, sl, tp, fillPrice;

  /// gtc | day | specified …
  final String expiry;
  final DateTime? placedAt, doneAt;
  final String? status, reason;
  final Map<String, dynamic>? option;

  static EngineOrder fromJson(Map<String, dynamic> j) => EngineOrder(
    ticket: _i(j['ticket']),
    symbol: _s(j['symbol']),
    side: _s(j['side'], 'buy'),
    type: _s(j['type'], 'market'),
    volume: _d(j['volume']),
    price: _dn(j['price']),
    stopLimit: _dn(j['stopLimit']),
    sl: _dn(j['sl']),
    tp: _dn(j['tp']),
    expiry: _s(j['expiry']),
    placedAt: DateTime.tryParse(_s(j['placedAt'])),
    status: j['status'] as String?,
    doneAt: DateTime.tryParse(_s(j['doneAt'])),
    fillPrice: _dn(j['fillPrice']),
    reason: j['reason'] as String?,
    option: _map(j['option']),
  );
}

/// A deal (one side of a trade: entry `in`, exit `out` / `out_by`).
class EngineDeal {
  const EngineDeal({
    required this.id,
    required this.login,
    required this.positionTicket,
    required this.orderTicket,
    required this.symbol,
    required this.side,
    required this.positionSide,
    required this.entry,
    required this.volume,
    required this.price,
    required this.profit,
    required this.swap,
    required this.commission,
    required this.reason,
    required this.time,
    required this.openPrice,
    required this.openTime,
    this.source = '',
    this.comment = '',
    this.reversed = false,
    this.instrument,
    this.option,
  });

  final int id, login, positionTicket;
  final int? orderTicket;
  final String symbol, side, positionSide;

  /// in | out | out_by
  final String entry;
  final double volume, price, profit, swap, commission;

  /// client | dealer | sl | tp | stop_out | close_by | pending_fill | force | price_correction | expiry | …
  final String reason;
  final DateTime? time, openTime;
  final double? openPrice;
  final String source, comment;
  final bool reversed;

  /// option | cfd
  final String? instrument;
  final Map<String, dynamic>? option;

  bool get isOption => instrument == 'option' || option != null || isOptionSymbol(symbol);

  static EngineDeal fromJson(Map<String, dynamic> j) => EngineDeal(
    id: _i(j['id']),
    login: _i(j['login']),
    positionTicket: _i(j['positionTicket']),
    orderTicket: _in(j['orderTicket']),
    symbol: _s(j['symbol']),
    side: _s(j['side'], 'buy'),
    positionSide: _s(j['positionSide'], _s(j['side'], 'buy')),
    entry: _s(j['entry'], 'in'),
    volume: _d(j['volume']),
    price: _d(j['price']),
    profit: _d(j['profit']),
    swap: _d(j['swap']),
    commission: _d(j['commission']),
    reason: _s(j['reason'], 'client'),
    time: DateTime.tryParse(_s(j['time'])),
    openPrice: _dn(j['openPrice']),
    openTime: DateTime.tryParse(_s(j['openTime'])),
    source: _s(j['source']),
    comment: _s(j['comment']),
    reversed: j['reversed'] == true,
    instrument: j['instrument'] as String?,
    option: _map(j['option']),
  );
}

/// `GET trading/accounts/{login}/history?from&to&page&limit&instrument`.
class HistoryPage {
  const HistoryPage({
    required this.deals,
    required this.orders,
    required this.page,
    required this.limit,
    required this.total,
    required this.profit,
    required this.swap,
    required this.commission,
    this.truncated = false,
  });
  final List<EngineDeal> deals;
  final List<EngineOrder> orders;
  final int page, limit, total;

  /// The period's totals.
  final double profit, swap, commission;
  final bool truncated;

  static HistoryPage fromJson(Map<String, dynamic> j) {
    final totals = _map(j['totals']) ?? const {};
    return HistoryPage(
      deals: [for (final d in _maps(j['deals'])) EngineDeal.fromJson(d)],
      orders: [for (final o in _maps(j['orders'])) EngineOrder.fromJson(o)],
      page: _i(j['page'] ?? 1),
      limit: _i(j['limit'] ?? 50),
      total: _i(j['total']),
      profit: _d(totals['profit']),
      swap: _d(totals['swap']),
      commission: _d(totals['commission']),
      truncated: j['truncated'] == true,
    );
  }
}

/// A balance movement (`GET trading/accounts/{login}/ledger?from&to&page&limit`).
class LedgerItem {
  const LedgerItem({
    required this.txn,
    required this.kind,
    required this.subLedger,
    required this.amount,
    required this.currency,
    required this.at,
    this.reference,
    this.reasonCode,
    this.note,
  });
  final int txn;

  /// transfer_in | transfer_out | trade_pnl | commission | deposit | withdrawal | adjustment | credit | bonus | …
  final String kind;

  /// balance | credit | bonus
  final String subLedger;
  final double amount;
  final String currency;
  final String? reference, reasonCode, note;
  final DateTime? at;

  static LedgerItem fromJson(Map<String, dynamic> j) => LedgerItem(
    txn: _i(j['txn']),
    kind: _s(j['kind']),
    subLedger: _s(j['subLedger'], 'balance'),
    amount: _d(j['amount']),
    currency: _s(j['currency'], 'USD'),
    reference: j['reference'] as String?,
    reasonCode: j['reasonCode'] as String?,
    note: j['note'] as String?,
    at: DateTime.tryParse(_s(j['at'])),
  );
}

class LedgerPage {
  const LedgerPage({required this.items, required this.page, required this.limit, required this.total});
  final List<LedgerItem> items;
  final int page, limit, total;

  static LedgerPage fromJson(Map<String, dynamic> j) => LedgerPage(
    items: [for (final x in _maps(j['items'])) LedgerItem.fromJson(x)],
    page: _i(j['page'] ?? 1),
    limit: _i(j['limit'] ?? 50),
    total: _i(j['total']),
  );
}

/// `GET trading/accounts/{login}` -> {account, positions, orders}.
class AccountDetail {
  const AccountDetail({required this.account, required this.positions, required this.orders});
  final EngineAccount account;
  final List<EnginePosition> positions;
  final List<EngineOrder> orders;

  static AccountDetail fromJson(Map<String, dynamic> j) => AccountDetail(
    account: EngineAccount.fromJson(_map(j['account']) ?? const {}),
    positions: [for (final p in _maps(j['positions'])) EnginePosition.fromJson(p)],
    orders: [for (final o in _maps(j['orders'])) EngineOrder.fromJson(o)],
  );
}

/// `POST trading/accounts` (open) -> {account, credentials: {login, password?, investorPassword?}}.
class OpenResult {
  const OpenResult({required this.account, required this.login, this.password, this.investorPassword});
  final EngineAccount account;
  final int login;
  final String? password, investorPassword;

  static OpenResult fromJson(Map<String, dynamic> j) {
    final c = _map(j['credentials']) ?? const {};
    final account = EngineAccount.fromJson(_map(j['account']) ?? const {});
    return OpenResult(
      account: account,
      login: _in(c['login']) ?? account.login,
      password: c['password'] as String?,
      investorPassword: c['investorPassword'] as String?,
    );
  }
}

/* ------------------------------------------------------------------ */
/* The web's helpers (components/trading/api.ts)                       */
/* ------------------------------------------------------------------ */

/// Archived / closed accounts sit in the Archived tab: out of the Live / Demo lists and the live totals.
bool isArchivedAccount(EngineAccount a) => a.archived;

/// "Pro · Hedging" (web accountTitle; the mode through `accounts.mode.<mode>`).
String modeLabel(String mode) => mode == 'netting' ? 'Netting' : 'Hedging';

/// Margin-level tone: > 500 % up, > 200 % warn, else down (null: no margin in use).
String? levelTone(num? ml) {
  if (ml == null || !ml.isFinite) return null;
  return ml > 500 ? 'up' : (ml > 200 ? 'warn' : 'down');
}

/// "1,234%" or "—" (web fmtLevel: also "—" for 0 and below).
String fmtLevel(num? ml) => ml == null || !ml.isFinite || ml <= 0 ? '—' : '${Fmt.number(ml.round(), 0)}%';

/// Status label key and tone (web STATUS_LABEL; the label via `accounts.status.<status>` where it exists).
const Map<String, ({String label, String tone})> kAccountStatus = {
  'active': (label: 'Active', tone: 'up'),
  'close_only': (label: 'Close only', tone: 'warn'),
  'read_only': (label: 'Read only', tone: 'warn'),
  'disabled': (label: 'Disabled', tone: 'down'),
  'expired': (label: 'Expired', tone: 'neutral'),
  'archived': (label: 'Archived', tone: 'neutral'),
  'closed': (label: 'Closed', tone: 'neutral'),
};

/// Price digits when no instrument spec is at hand (web priceDigits).
int priceDigits(num v) {
  if (!v.isFinite) return 2;
  if (v >= 1000) return 2;
  if (v >= 50) return 3;
  return 5;
}

/// "1.08412" / "—" (web fmtPrice, en-US grouping).
String fmtPrice(num? v, [int? digits]) => v == null || !v.isFinite ? '—' : Fmt.number(v, digits ?? priceDigits(v));

/// "-$12.30" / "+USC 1,200.00" (web fmtAmount: `cur` is the account prefix, e.g. EngineAccount.currencyPrefix).
String fmtAmount(num v, String cur, {bool signed = false}) {
  final s = Fmt.number(v.abs());
  final sign = signed ? (v > 0 ? '+' : (v < 0 ? '-' : '')) : (v < 0 ? '-' : '');
  return '$sign$cur$s';
}

/// YYYY-MM-DD in local time (web isoDay).
String isoDay(DateTime d) => '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';

/// The trading server's offset from UTC: GMT+3 while US daylight time is on, GMT+2 otherwise (web serverOffset,
/// the engine's rule). Statements, history and ledger times are shown in this server time.
int tradingServerOffset(DateTime t) {
  final u = t.toUtc();
  final y = u.year;
  int nthSunday(int month, int n) {
    final first = DateTime.utc(y, month).weekday % 7; // Sunday = 0
    return 1 + ((7 - first) % 7) + (n - 1) * 7;
  }

  final start = DateTime.utc(y, 3, nthSunday(3, 2), 7);
  final end = DateTime.utc(y, 11, nthSunday(11, 1), 6);
  return !u.isBefore(start) && u.isBefore(end) ? 3 : 2;
}

/// A time in trading server time (shifted, as UTC fields), for formatting with LocaleFormat-free patterns.
DateTime toTradingServerTime(DateTime t) => t.toUtc().add(Duration(hours: tradingServerOffset(t)));

/// Ledger kinds: the web's English label (fallback for `accounts.ledgerKind.<kind>`) and chip tone.
const Map<String, ({String label, String tone})> kLedgerKind = {
  'transfer_in': (label: 'Deposit from wallet', tone: 'up'),
  'transfer_out': (label: 'Withdrawal to wallet', tone: 'down'),
  'trade_pnl': (label: 'Trade result', tone: 'ember'),
  'commission': (label: 'Commission', tone: 'neutral'),
  'deposit': (label: 'Deposit', tone: 'up'),
  'withdrawal': (label: 'Withdrawal', tone: 'down'),
  'adjustment': (label: 'Adjustment', tone: 'info'),
  'credit': (label: 'Credit', tone: 'gold'),
  'bonus': (label: 'Bonus', tone: 'gold'),
  'nbp': (label: 'Negative balance protection', tone: 'info'),
  'demo_funding': (label: 'Demo funds', tone: 'gold'),
  'demo_initial': (label: 'Demo funds', tone: 'gold'),
  'demo_refill': (label: 'Demo refill', tone: 'gold'),
  'reversal': (label: 'Reversal', tone: 'neutral'),
  'charges': (label: 'Charges', tone: 'neutral'),
  'swap': (label: 'Swap', tone: 'neutral'),
  'perf_fee': (label: 'Performance fee', tone: 'neutral'),
  'option_premium': (label: 'Option premium', tone: 'ember'),
  'option_settlement': (label: 'Option settlement', tone: 'ember'),
  'option_rebate': (label: 'Option rebate', tone: 'up'),
};

({String label, String tone}) ledgerKind(String k) => kLedgerKind[k] ?? (label: k.replaceAll('_', ' '), tone: 'neutral');

/// Deal reasons (web REASON_LABEL; the label via `accounts.reason.<reason>` where it exists).
const Map<String, String> kReasonLabel = {
  'client': 'Client',
  'dealer': 'Dealer',
  'sl': 'Stop loss',
  'tp': 'Take profit',
  'stop_out': 'Stop out',
  'close_by': 'Close by',
  'pending_fill': 'Pending order',
  'force': 'Dealer',
  'price_correction': 'Price correction',
};

/* ------------------------------------------------------------------ */
/* Kalks FX Options in trade lists (option-deal.ts)                    */
/* ------------------------------------------------------------------ */

final RegExp _seriesRe = RegExp(r'^([A-Za-z0-9]{2,16})-(\d{4})(\d{2})(\d{2})-(\d+(?:\.\d+)?)-([CPcp])$');

/// A series code's parts: `EURUSD-20261002-1.1000-C` -> (EURUSD, call, 1.1000, 2026-10-02).
({String series, String underlying, String right, double strike, String strikeLabel, String expiry})? parseSeries(String? code) {
  final m = _seriesRe.firstMatch(code ?? '');
  if (m == null) return null;
  return (
    series: code!,
    underlying: m[1]!.toUpperCase(),
    expiry: '${m[2]}-${m[3]}-${m[4]}',
    strike: double.parse(m[5]!),
    strikeLabel: m[5]!,
    right: m[6]!.toUpperCase() == 'C' ? 'call' : 'put',
  );
}

bool isOptionSymbol(String? symbol) => _seriesRe.hasMatch(symbol ?? '');

/// Account-currency units per USD (cent accounts keep money in US cents).
int usdFactorOf(EngineAccount? a) => a != null && (a.cent || a.currency == 'USC') ? 100 : 1;

// A trading account as the engine shows it to its client (apps/crm/components/trading/api.ts EngineAccount; the BFF
// strips dealer fields). Served by `GET trading/accounts` -> {accounts: [...]}.
import '../format/format.dart';

enum AccountKind { live, demo }

/// What an account (its group) trades: `cfd` or `options` (engine `product`). Anything but "options" reads as CFD,
/// so an engine without the field keeps every account a CFD account.
String productOf(Object? v) => v == 'options' ? 'options' : 'cfd';

class EngineAccount {
  const EngineAccount({
    required this.login,
    required this.type,
    required this.group,
    required this.groupName,
    required this.mode,
    required this.cent,
    required this.currency,
    required this.leverage,
    required this.status,
    required this.name,
    required this.positions,
    required this.orders,
    required this.balance,
    required this.credit,
    required this.bonus,
    required this.profit,
    required this.equity,
    required this.margin,
    required this.freeMargin,
    required this.marginLevel,
    required this.createdAt,
    this.isDefault = false,
    this.demo,
    this.product = 'cfd',
    this.raw = const {},
  });

  final int login;
  final AccountKind type;
  final String group;
  final String groupName;

  /// hedging | netting
  final String mode;
  final bool cent;
  final String currency;
  final int leverage;

  /// active | close_only | read_only | disabled | expired | archived | closed
  final String status;
  final String name;
  final int positions;
  final int orders;
  final double balance, credit, bonus, profit, equity, margin, freeMargin;
  final double? marginLevel;
  final DateTime createdAt;
  final bool isDefault;

  /// Demo accounts: {initialBalance, refillsPerDay, refillsUsedToday, expiryDays}.
  final Map<String, dynamic>? demo;

  /// cfd | options: a CFD account trades CFDs only, an Options account options only (the group's product).
  final String product;
  final Map<String, dynamic> raw;

  bool get isOptions => product == 'options';

  bool get live => type == AccountKind.live;
  bool get archived => status == 'archived' || status == 'closed';

  /// Prop challenge accounts (group "prop…").
  bool get prop => group.toLowerCase().startsWith('prop');

  /// "Kalks-Live" / "Kalks-Demo".
  String get server => live ? 'Kalks-Live' : 'Kalks-Demo';

  /// "$", "USC ", "EUR " (web curOf).
  String get currencyPrefix => Fmt.accountPrefix(currency: currency, cent: cent);

  /// Trade buttons are off for these (web TradeButton `blocked`).
  bool get tradeBlocked => status == 'disabled' || status == 'expired' || archived;

  String money(num v) => Fmt.accountMoney(v, currency: currency, cent: cent);
  double usd(num v) => Fmt.toUsd(v, cent: cent).toDouble();

  static double _d(Object? v) => v is num ? v.toDouble() : double.tryParse('$v') ?? 0;

  static EngineAccount fromJson(Map<String, dynamic> j) => EngineAccount(
    login: (j['login'] as num?)?.toInt() ?? 0,
    type: j['type'] == 'demo' ? AccountKind.demo : AccountKind.live,
    group: '${j['group'] ?? ''}',
    groupName: '${j['groupName'] ?? j['group'] ?? ''}',
    mode: '${j['mode'] ?? 'hedging'}',
    cent: j['cent'] == true,
    currency: '${j['currency'] ?? 'USD'}',
    leverage: (j['leverage'] as num?)?.toInt() ?? 0,
    status: '${j['status'] ?? 'active'}',
    name: '${j['name'] ?? ''}',
    positions: (j['positions'] as num?)?.toInt() ?? 0,
    orders: (j['orders'] as num?)?.toInt() ?? 0,
    balance: _d(j['balance']),
    credit: _d(j['credit']),
    bonus: _d(j['bonus']),
    profit: _d(j['profit']),
    equity: _d(j['equity']),
    margin: _d(j['margin']),
    freeMargin: _d(j['freeMargin']),
    marginLevel: j['marginLevel'] is num ? (j['marginLevel'] as num).toDouble() : null,
    createdAt: DateTime.tryParse('${j['createdAt']}') ?? DateTime.now(),
    isDefault: j['isDefault'] == true,
    demo: j['demo'] is Map ? (j['demo'] as Map).cast<String, dynamic>() : null,
    product: productOf(j['product']),
    raw: j,
  );
}

/// Totals over the live accounts in USD (web liveTotals): prop accounts and archived ones don't count.
class AccountTotals {
  AccountTotals(List<EngineAccount> all)
    : live = all.where((a) => !a.archived && a.live && !a.prop).toList(),
      demo = all.where((a) => !a.archived && a.type == AccountKind.demo).toList(),
      archived = all.where((a) => a.archived).toList(),
      positions = all.where((a) => !a.archived).fold(0, (s, a) => s + a.positions);

  final List<EngineAccount> live;
  final List<EngineAccount> demo;
  final List<EngineAccount> archived;
  final int positions;

  double get equity => live.fold(0, (s, a) => s + a.usd(a.equity));
  double get balance => live.fold(0, (s, a) => s + a.usd(a.balance));
  double get freeMargin => live.fold(0, (s, a) => s + a.usd(a.freeMargin));
  double get profit => live.fold(0, (s, a) => s + a.usd(a.profit));
}

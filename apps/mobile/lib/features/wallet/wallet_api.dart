// The USDT wallet's data: a port of apps/crm/components/wallet-live/api.ts plus the pure helpers of ui.tsx and
// pay.ts. Models of the wallet BFF's answers (apps/crm/app/api/wallet/[...path]/route.ts, services/wallet/README.md),
// one provider per web `useWallet(path, ms)` with the same poll interval, and the rules the pages share (amount
// cleaning, address checks, status maps, activity titles, the wallet apps' deep links, error texts).
// Amounts stay strings end to end, as on the web; only display and limit checks parse them.
import 'dart:convert';
import 'dart:math';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../core/api/api_providers.dart';
import '../../core/format/format.dart';
import '../../core/lifecycle.dart';
import '../../core/models/account.dart';
import '../../core/models/user.dart';
import '../../core/models/wallet.dart';
import '../../data/client_data.dart';
import '../../i18n/t.dart';
import '../../ui/tokens.dart';

/* ------------------------------------------------------------------ parsing */

double _num(Object? v) => v is num ? v.toDouble() : double.tryParse('${v ?? ''}') ?? 0;
int _int(Object? v) => v is num ? v.toInt() : int.tryParse('${v ?? ''}') ?? 0;
String? _s(Object? v) => v == null ? null : '$v';
DateTime _date(Object? v) => DateTime.tryParse('${v ?? ''}') ?? DateTime.now();
DateTime? _dateOrNull(Object? v) => v == null ? null : DateTime.tryParse('$v');
Map<String, dynamic> _map(Object? v) => v is Map ? v.cast<String, dynamic>() : const {};
List<Map<String, dynamic>> _list(Object? v) => [
  for (final x in (v is List ? v : const <Object?>[]))
    if (x is Map) x.cast<String, dynamic>(),
];

/* ------------------------------------------------------------------ networks */

/// Display names of the two networks (web CHAIN_LABEL).
typedef ChainLabel = ({String name, String short, String wallet});

const Map<String, ChainLabel> kChainLabel = {
  'bsc': (name: 'BNB Smart Chain', short: 'BEP20', wallet: 'MetaMask'),
  'tron': (name: 'TRON', short: 'TRC20', wallet: 'TronLink'),
};

/// The label of `chain` (an unknown chain shows its own code).
ChainLabel chainLabel(String? chain) => kChainLabel[chain] ?? (name: chain ?? '', short: (chain ?? '').toUpperCase(), wallet: '');

/// A network of `GET wallet/config`.
class WalletChain {
  const WalletChain({
    required this.chain,
    required this.network,
    required this.token,
    required this.tokenContract,
    required this.decimals,
    required this.evmChainId,
    required this.confirmations,
    required this.depositsEnabled,
    required this.withdrawalsEnabled,
    required this.minDeposit,
    required this.withdrawFee,
  });

  /// bsc | tron
  final String chain;
  final String network, token, tokenContract;
  final int decimals;
  final int? evmChainId;
  final int confirmations;
  final bool depositsEnabled, withdrawalsEnabled;
  final String minDeposit, withdrawFee;

  static WalletChain fromJson(Map<String, dynamic> j) => WalletChain(
    chain: '${j['chain'] ?? ''}',
    network: '${j['network'] ?? ''}',
    token: '${j['token'] ?? 'USDT'}',
    tokenContract: '${j['token_contract'] ?? ''}',
    decimals: _int(j['decimals']),
    evmChainId: j['evm_chain_id'] == null ? null : _int(j['evm_chain_id']),
    confirmations: _int(j['confirmations']),
    depositsEnabled: j['deposits_enabled'] == true,
    withdrawalsEnabled: j['withdrawals_enabled'] == true,
    minDeposit: '${j['min_deposit'] ?? '0'}',
    withdrawFee: '${j['withdraw_fee'] ?? '0'}',
  );
}

/// Withdrawal limits and fees of `GET wallet/config`.
class WalletLimits {
  const WalletLimits({
    required this.withdrawMin,
    required this.withdrawMax,
    required this.withdrawDailyMax,
    required this.withdrawFeeFlat,
    required this.withdrawFeePct,
    required this.depositCooldownHours,
    required this.intentTtlMinutes,
  });

  final String withdrawMin, withdrawMax, withdrawDailyMax, withdrawFeeFlat, withdrawFeePct;
  final int depositCooldownHours, intentTtlMinutes;

  static WalletLimits fromJson(Map<String, dynamic> j) => WalletLimits(
    withdrawMin: '${j['withdraw_min'] ?? '0'}',
    withdrawMax: '${j['withdraw_max'] ?? '0'}',
    withdrawDailyMax: '${j['withdraw_daily_max'] ?? '0'}',
    withdrawFeeFlat: '${j['withdraw_fee_flat'] ?? '0'}',
    withdrawFeePct: '${j['withdraw_fee_pct'] ?? '0'}',
    depositCooldownHours: _int(j['deposit_cooldown_hours']),
    intentTtlMinutes: _int(j['intent_ttl_minutes']),
  );
}

/// `GET wallet/config`: networks, limits, fees.
class WalletConfig {
  const WalletConfig({required this.chains, required this.limits});
  final List<WalletChain> chains;
  final WalletLimits limits;

  WalletChain? chain(String c) => chains.where((x) => x.chain == c).firstOrNull;

  static WalletConfig fromJson(Map<String, dynamic> j) =>
      WalletConfig(chains: [for (final c in _list(j['chains'])) WalletChain.fromJson(c)], limits: WalletLimits.fromJson(_map(j['limits'])));
}

/* ------------------------------------------------------------------ deposits */

/// A deposit request (`POST wallet/deposits/intents` -> {intent}): the company address, the amount, the expiry.
class DepositIntent {
  const DepositIntent({
    required this.id,
    required this.chain,
    required this.network,
    required this.currency,
    required this.amount,
    required this.address,
    required this.tokenContract,
    required this.decimals,
    required this.evmChainId,
    required this.status,
    required this.expiresAt,
    required this.createdAt,
  });

  /// dep_<24 hex>
  final String id;
  final String chain, network, currency, amount, address, tokenContract;
  final int decimals;
  final int? evmChainId;

  /// open | submitted | completed | expired
  final String status;
  final DateTime expiresAt, createdAt;

  bool expiredAt(DateTime now) => expiresAt.isBefore(now);

  static DepositIntent fromJson(Map<String, dynamic> j) => DepositIntent(
    id: '${j['id'] ?? ''}',
    chain: '${j['chain'] ?? ''}',
    network: '${j['network'] ?? ''}',
    currency: '${j['currency'] ?? 'USDT'}',
    amount: '${j['amount'] ?? '0'}',
    address: '${j['address'] ?? ''}',
    tokenContract: '${j['token_contract'] ?? ''}',
    decimals: _int(j['decimals']),
    evmChainId: j['evm_chain_id'] == null ? null : _int(j['evm_chain_id']),
    status: '${j['status'] ?? 'open'}',
    expiresAt: _date(j['expires_at']),
    createdAt: _date(j['created_at']),
  );
}

/// An on-chain deposit (pending → confirming → credited; failed / review / unmatched / rejected).
class Deposit {
  const Deposit({
    required this.id,
    required this.intentId,
    required this.chain,
    required this.network,
    required this.txHash,
    required this.explorerUrl,
    required this.fromAddress,
    required this.amount,
    required this.expectedAmount,
    required this.confirmations,
    required this.requiredConfirmations,
    required this.status,
    required this.reviewReason,
    required this.failureReason,
    required this.creditedAt,
    required this.createdAt,
  });

  final int id;
  final String? intentId;
  final String chain, network, txHash;
  final String? explorerUrl, fromAddress, amount, expectedAmount;
  final int confirmations, requiredConfirmations;

  /// pending | confirming | credited | failed | review | unmatched | rejected
  final String status;
  final String? reviewReason, failureReason;
  final DateTime? creditedAt;
  final DateTime createdAt;

  static Deposit fromJson(Map<String, dynamic> j) => Deposit(
    id: _int(j['id']),
    intentId: _s(j['intent_id']),
    chain: '${j['chain'] ?? ''}',
    network: '${j['network'] ?? ''}',
    txHash: '${j['tx_hash'] ?? ''}',
    explorerUrl: _s(j['explorer_url']),
    fromAddress: _s(j['from_address']),
    amount: _s(j['amount']),
    expectedAmount: _s(j['expected_amount']),
    confirmations: _int(j['confirmations']),
    requiredConfirmations: _int(j['required_confirmations']),
    status: '${j['status'] ?? 'pending'}',
    reviewReason: _s(j['review_reason']),
    failureReason: _s(j['failure_reason']),
    creditedAt: _dateOrNull(j['credited_at']),
    createdAt: _date(j['created_at']),
  );
}

/// `GET wallet/deposits/intents/{id}` -> {intent, deposit}.
class IntentView {
  const IntentView({required this.intent, required this.deposit});
  final DepositIntent intent;
  final Deposit? deposit;

  static IntentView fromJson(Map<String, dynamic> j) =>
      IntentView(intent: DepositIntent.fromJson(_map(j['intent'])), deposit: j['deposit'] is Map ? Deposit.fromJson(_map(j['deposit'])) : null);
}

/* ------------------------------------------------------------------ crypto deposits (NOWPayments) */

/// A coin clients can pay with (`GET wallet/nowpayments/config` coins, a payment's `coin`).
class NpCoin {
  const NpCoin({
    required this.code,
    required this.ticker,
    required this.name,
    required this.network,
    required this.icon,
    required this.chainIcon,
    required this.needsExtraId,
    required this.minUsd,
  });

  /// NOWPayments' ticker (usdttrc20, btc, …)
  final String code;
  final String ticker, name, network, icon;
  final String? chainIcon;
  final bool needsExtraId;
  final String minUsd;

  /// "USDT · TRON (TRC20)", or the ticker alone.
  String get label => network.isEmpty ? ticker : '$ticker · $network';

  static NpCoin fromJson(Map<String, dynamic> j) => NpCoin(
    code: '${j['code'] ?? ''}',
    ticker: '${j['ticker'] ?? (j['code'] ?? '').toString().toUpperCase()}',
    name: '${j['name'] ?? ''}',
    network: '${j['network'] ?? ''}',
    icon: '${j['icon'] ?? ''}',
    chainIcon: _s(j['chain_icon']),
    needsExtraId: j['needs_extra_id'] == true,
    minUsd: '${j['min_usd'] ?? '0'}',
  );
}

/// `GET wallet/nowpayments/config`: off, or the coins and the USD limits.
class NpConfig {
  const NpConfig({required this.enabled, required this.sandbox, required this.minUsd, required this.maxUsd, required this.feePaidByUser, required this.coins});
  final bool enabled, sandbox, feePaidByUser;
  final String minUsd, maxUsd;
  final List<NpCoin> coins;

  /// Whether the deposit page offers the crypto method.
  bool get offered => enabled && coins.isNotEmpty;

  NpCoin? coin(String code) => coins.where((c) => c.code == code).firstOrNull;

  static NpConfig fromJson(Map<String, dynamic> j) => NpConfig(
    enabled: j['enabled'] == true,
    sandbox: j['sandbox'] == true,
    minUsd: '${j['min_deposit_usd'] ?? '0'}',
    maxUsd: '${j['max_deposit_usd'] ?? '0'}',
    feePaidByUser: j['fee_paid_by_user'] == true,
    coins: [for (final c in _list(j['coins'])) NpCoin.fromJson(c)],
  );
}

/// A crypto payment (`POST wallet/nowpayments/payments` / `GET wallet/nowpayments/payments/{id}` -> {payment}).
class NpPayment {
  const NpPayment({
    required this.id,
    required this.status,
    required this.priceAmount,
    required this.coin,
    required this.payAmount,
    required this.payAddress,
    required this.payinExtraId,
    required this.actuallyPaid,
    required this.expiresAt,
    required this.creditAmount,
    required this.sandbox,
  });

  final int id;

  /// creating | waiting | confirming | credited | review | expired | failed | refunded | rejected
  final String status;
  final String priceAmount;
  final NpCoin coin;
  final String? payAmount, payAddress, payinExtraId, actuallyPaid, creditAmount;
  final DateTime? expiresAt;
  final bool sandbox;

  /// Still waiting for money or for confirmations (the page keeps polling and shows no "new deposit").
  bool get open => status == 'waiting' || status == 'creating' || status == 'confirming';

  static NpPayment fromJson(Map<String, dynamic> j) => NpPayment(
    id: _int(j['id']),
    status: '${j['status'] ?? 'waiting'}',
    priceAmount: '${j['price_amount'] ?? '0'}',
    coin: NpCoin.fromJson(_map(j['coin'])),
    payAmount: _s(j['pay_amount']),
    payAddress: _s(j['pay_address']),
    payinExtraId: _s(j['payin_extra_id']),
    actuallyPaid: _s(j['actually_paid']),
    expiresAt: _dateOrNull(j['expires_at']),
    creditAmount: _s(j['credit_amount']),
    sandbox: j['sandbox'] == true,
  );
}

/// A NOWPayments coin code as people read it ("usdttrc20" -> "USDT TRC20", "bnbbsc" -> "BNB"; web npCoinName).
String npCoinName(String? code) {
  final c = (code ?? '').toLowerCase();
  const nets = {'trc20': 'TRC20', 'bsc': 'BEP20', 'erc20': 'ERC20', 'sol': 'SOL', 'matic': 'Polygon', 'ton': 'TON'};
  for (final base in const ['usdt', 'usdc']) {
    if (c.startsWith(base) && c.length > base.length) {
      final net = c.substring(base.length);
      return '${base.toUpperCase()} ${nets[net] ?? net.toUpperCase()}';
    }
  }
  if (c == 'bnbbsc' || c == 'bnbmainnet') return 'BNB';
  return c.toUpperCase();
}

/// A USD deposit amount (2 decimals) within the coin's minimum and the broker's maximum.
bool npAmountOk(String amount, String? minUsd, String? maxUsd) {
  final a = amount.trim();
  final n = double.tryParse(a) ?? 0;
  final max = _num(maxUsd);
  return _amount2.hasMatch(a) && n > 0 && n >= _num(minUsd) && (max <= 0 || n <= max);
}

/// The payment status chips (web NP_STATUS).
const Map<String, WalletStatus> kNpStatus = {
  'creating': (tone: KChipTone.warn, label: 'wallet.nowpayments.status.waiting'),
  'waiting': (tone: KChipTone.warn, label: 'wallet.nowpayments.status.waiting'),
  'confirming': (tone: KChipTone.info, label: 'wallet.nowpayments.status.confirming'),
  'credited': (tone: KChipTone.up, label: 'wallet.nowpayments.status.credited'),
  'review': (tone: KChipTone.warn, label: 'wallet.nowpayments.status.review'),
  'expired': (tone: KChipTone.neutral, label: 'wallet.nowpayments.status.expired'),
  'failed': (tone: KChipTone.down, label: 'wallet.nowpayments.status.failed'),
  'refunded': (tone: KChipTone.neutral, label: 'wallet.nowpayments.status.refunded'),
  'rejected': (tone: KChipTone.down, label: 'wallet.nowpayments.status.rejected'),
};

/// The logos in assets/coins (other coins get a letter disc).
const Set<String> kCoinIcons = {'ada', 'avax', 'bnb', 'btc', 'doge', 'dot', 'eth', 'link', 'ltc', 'matic', 'sol', 'trx', 'usdt', 'xrp'};

/* ------------------------------------------------------------------ withdrawals, transfers */

/// A withdrawal: requested → approved / rejected → paid → completed (cancelled while requested).
class Withdrawal {
  const Withdrawal({
    required this.id,
    required this.chain,
    required this.network,
    required this.toAddress,
    required this.amount,
    required this.fee,
    required this.netAmount,
    required this.status,
    required this.reason,
    required this.payoutTxHash,
    required this.explorerUrl,
    required this.createdAt,
  });

  final int id;
  final String chain, network, toAddress, amount, fee, netAmount;

  /// requested | approved | rejected | cancelled | paid | completed
  final String status;
  final String? reason, payoutTxHash, explorerUrl;
  final DateTime createdAt;

  static Withdrawal fromJson(Map<String, dynamic> j) => Withdrawal(
    id: _int(j['id']),
    chain: '${j['chain'] ?? ''}',
    network: '${j['network'] ?? ''}',
    toAddress: '${j['to_address'] ?? ''}',
    amount: '${j['amount'] ?? '0'}',
    fee: '${j['fee'] ?? '0'}',
    netAmount: '${j['net_amount'] ?? '0'}',
    status: '${j['status'] ?? 'requested'}',
    reason: _s(j['reason']),
    payoutTxHash: _s(j['payout_tx_hash']),
    explorerUrl: _s(j['explorer_url']),
    createdAt: _date(j['created_at']),
  );
}

/// `POST wallet/withdrawals/quote` -> {quote}: every check of a request, nothing locked.
class WithdrawQuote {
  const WithdrawQuote({
    required this.amount,
    required this.fee,
    required this.netAmount,
    required this.usedToday,
    required this.dailyMax,
    required this.available,
  });
  final String amount, fee, netAmount, usedToday, dailyMax, available;

  static WithdrawQuote fromJson(Map<String, dynamic> j) => WithdrawQuote(
    amount: '${j['amount'] ?? '0'}',
    fee: '${j['fee'] ?? '0'}',
    netAmount: '${j['net_amount'] ?? '0'}',
    usedToday: '${j['used_today'] ?? '0'}',
    dailyMax: '${j['daily_max'] ?? '0'}',
    available: '${j['available'] ?? '0'}',
  );
}

/// A wallet ↔ trading account transfer (`GET wallet/transfers`, `POST wallet/transfers/to-trading|from-trading`).
class TradingTransfer {
  const TradingTransfer({
    required this.id,
    required this.login,
    required this.direction,
    required this.amount,
    required this.status,
    required this.errorMessage,
    required this.createdAt,
  });
  final int id;
  final int login;

  /// to_trading | from_trading
  final String direction;
  final String amount;

  /// pending | completed | failed
  final String status;
  final String? errorMessage;
  final DateTime createdAt;

  static TradingTransfer fromJson(Map<String, dynamic> j) => TradingTransfer(
    id: _int(j['id']),
    login: _int(j['login']),
    direction: '${j['direction'] ?? 'to_trading'}',
    amount: '${j['amount'] ?? '0'}',
    status: '${j['status'] ?? 'pending'}',
    errorMessage: _s(j['error_message']),
    createdAt: _date(j['created_at']),
  );
}

/* ------------------------------------------------------------------ overview, activity, notifications */

/// `GET wallet/overview`: balances, deposits being confirmed, open withdrawals, today's limit use.
class WalletOverviewData {
  const WalletOverviewData({
    required this.balances,
    required this.pendingDeposits,
    required this.openWithdrawals,
    required this.usedToday,
    required this.remainingToday,
    required this.dailyMax,
    required this.cooldownUntil,
    required this.notificationsUnread,
  });

  final List<WalletBalance> balances;
  final List<Deposit> pendingDeposits;
  final List<Withdrawal> openWithdrawals;
  final String usedToday, remainingToday, dailyMax;
  final DateTime? cooldownUntil;
  final int notificationsUnread;

  /// The USDT balance (web usdtAvailable): zeros when the wallet has never held funds.
  WalletBalance get usdt => balances.where((b) => b.currency == 'USDT').firstOrNull ?? const WalletBalance(currency: 'USDT', available: '0', locked: '0');

  static WalletOverviewData fromJson(Map<String, dynamic> j) {
    final l = _map(j['limits']);
    return WalletOverviewData(
      balances: [for (final b in _list(j['balances'])) WalletBalance.fromJson(b)],
      pendingDeposits: [for (final d in _list(j['pending_deposits'])) Deposit.fromJson(d)],
      openWithdrawals: [for (final w in _list(j['open_withdrawals'])) Withdrawal.fromJson(w)],
      usedToday: '${l['used_today'] ?? '0'}',
      remainingToday: '${l['remaining_today'] ?? '0'}',
      dailyMax: '${l['daily_max'] ?? '0'}',
      cooldownUntil: _dateOrNull(l['cooldown_until']),
      notificationsUnread: _int(j['notifications_unread']),
    );
  }
}

/// A row of `GET wallet/activity` (deposits, withdrawals, transfers and other credits / debits).
class ActivityItem {
  const ActivityItem({
    required this.type,
    required this.id,
    required this.status,
    required this.amount,
    required this.currency,
    required this.chain,
    required this.txHash,
    required this.explorerUrl,
    required this.login,
    required this.direction,
    required this.kind,
    required this.note,
    required this.address,
    required this.confirmations,
    required this.requiredConfirmations,
    required this.createdAt,
  });

  /// deposit | withdrawal | transfer | other
  final String type;
  final String id, status;
  final String? amount;
  final String currency;
  final String? chain, txHash, explorerUrl;
  final int? login;

  /// in | out
  final String direction;
  final String? kind, note, address;
  final int? confirmations, requiredConfirmations;
  final DateTime createdAt;

  bool get inbound => direction == 'in';

  /// Failed, rejected and cancelled rows are dimmed and struck through.
  bool get dim => const {'failed', 'rejected', 'cancelled'}.contains(status);

  static ActivityItem fromJson(Map<String, dynamic> j) => ActivityItem(
    type: '${j['type'] ?? 'other'}',
    id: '${j['id'] ?? ''}',
    status: '${j['status'] ?? ''}',
    amount: _s(j['amount']),
    currency: '${j['currency'] ?? 'USDT'}',
    chain: _s(j['chain']),
    txHash: _s(j['tx_hash']),
    explorerUrl: _s(j['explorer_url']),
    login: j['login'] == null ? null : _int(j['login']),
    direction: '${j['direction'] ?? 'in'}',
    kind: _s(j['kind']),
    note: _s(j['note']),
    address: _s(j['address']),
    confirmations: j['confirmations'] == null ? null : _int(j['confirmations']),
    requiredConfirmations: j['required_confirmations'] == null ? null : _int(j['required_confirmations']),
    createdAt: _date(j['created_at']),
  );
}

/// A page of a wallet list (`{items, page, limit, total}`).
class WalletPage<V> {
  const WalletPage({required this.items, required this.page, required this.limit, required this.total});
  final List<V> items;
  final int page, limit, total;

  static WalletPage<V> fromJson<V>(Map<String, dynamic> j, V Function(Map<String, dynamic>) item) {
    final items = [for (final x in _list(j['items'])) item(x)];
    return WalletPage(
      items: items,
      page: _int(j['page'] ?? 1),
      limit: _int(j['limit'] ?? items.length),
      total: j['total'] == null ? items.length : _int(j['total']),
    );
  }
}

/// A wallet notice (`GET wallet/notifications` -> {items, unread}).
class WalletNotice {
  const WalletNotice({required this.id, required this.title, required this.body, required this.read, required this.createdAt});
  final int id;
  final String title, body;
  final bool read;
  final DateTime createdAt;

  static WalletNotice fromJson(Map<String, dynamic> j) =>
      WalletNotice(id: _int(j['id']), title: '${j['title'] ?? ''}', body: '${j['body'] ?? ''}', read: j['read'] == true, createdAt: _date(j['created_at']));
}

class WalletNotices {
  const WalletNotices({required this.items, required this.unread});
  final List<WalletNotice> items;
  final int unread;

  static WalletNotices fromJson(Map<String, dynamic> j) =>
      WalletNotices(items: [for (final n in _list(j['items'])) WalletNotice.fromJson(n)], unread: _int(j['unread']));
}

/* ------------------------------------------------------------------ providers (one per web useWallet) */

/// `useWallet("config")`: once per page.
final walletCfgProvider = FutureProvider.autoDispose<WalletConfig>((ref) async {
  return WalletConfig.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/config'));
});

/// `useWallet("overview", ms)`: the wallet page polls every 10 s, the withdraw page 15 s, the transfer page 10 s.
final walletOverviewEveryProvider = FutureProvider.autoDispose.family<WalletOverviewData, int>((ref, ms) async {
  ref.pollEvery(Duration(milliseconds: ms));
  return WalletOverviewData.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/overview'));
});

/// The query of `useWallet("activity?…", ms)`: `type` / `page` are sent only when set (as the web's path).
typedef ActivityQuery = ({String? type, int? page, int limit, int ms});

/// The wallet page's recent activity: `activity?limit=8` every 15 s.
const ActivityQuery kRecentActivity = (type: null, page: null, limit: 8, ms: 15000);

/// Rows per history page (web PER).
const int kHistoryPer = 25;

final walletActivityPageProvider = FutureProvider.autoDispose.family<WalletPage<ActivityItem>, ActivityQuery>((ref, q) async {
  ref.pollEvery(Duration(milliseconds: q.ms));
  final j = await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/activity', query: {'type': q.type, 'page': q.page, 'limit': q.limit});
  return WalletPage.fromJson(j, ActivityItem.fromJson);
});

/// `useWallet("notifications", 30000)`.
final walletNoticesProvider = FutureProvider.autoDispose<WalletNotices>((ref) async {
  ref.pollEvery(const Duration(seconds: 30));
  return WalletNotices.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/notifications'));
});

/// `useWallet("deposits/intents/<id>", 5000)`: the open deposit, polled every 5 s while it is shown.
final depositIntentProvider = FutureProvider.autoDispose.family<IntentView, String>((ref, id) async {
  ref.pollEvery(const Duration(seconds: 5));
  return IntentView.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/deposits/intents/$id'));
});

/// `useWallet("nowpayments/config")`: crypto deposits through NOWPayments (off unless the broker switched them on).
final npConfigProvider = FutureProvider.autoDispose<NpConfig>((ref) async {
  return NpConfig.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/nowpayments/config'));
});

/// `useWallet("nowpayments/payments/<id>", 5000)`: the crypto payment, polled every 5 s while it is shown.
final npPaymentProvider = FutureProvider.autoDispose.family<NpPayment, int>((ref, id) async {
  ref.pollEvery(const Duration(seconds: 5));
  return NpPayment.fromJson(_map((await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/nowpayments/payments/$id'))['payment']));
});

/// `useWallet("withdrawals?limit=20", 15000)`.
final withdrawalsProvider = FutureProvider.autoDispose<WalletPage<Withdrawal>>((ref) async {
  ref.pollEvery(const Duration(seconds: 15));
  return WalletPage.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/withdrawals', query: {'limit': 20}), Withdrawal.fromJson);
});

/// `useWallet("transfers?limit=15", 10000)`.
final tradingTransfersProvider = FutureProvider.autoDispose<WalletPage<TradingTransfer>>((ref) async {
  ref.pollEvery(const Duration(seconds: 10));
  return WalletPage.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('wallet/transfers', query: {'limit': 15}), TradingTransfer.fromJson);
});

/// `useAccounts(ms)` of the wallet pages (15 s on the wallet page, 10 s on the transfer page).
final walletAccountsProvider = FutureProvider.autoDispose.family<List<EngineAccount>, int>((ref, ms) async {
  ref.pollEvery(Duration(milliseconds: ms));
  final j = await ref.watch(apiProvider).get<Map<String, dynamic>>('trading/accounts');
  return [for (final a in _list(j['accounts'])) EngineAccount.fromJson(a)];
});

/// After a money change: every wallet answer on screen, and the dashboard's shared copies, load again.
void refreshWalletData(WidgetRef ref) {
  ref
    ..invalidate(walletOverviewEveryProvider)
    ..invalidate(walletActivityPageProvider)
    ..invalidate(withdrawalsProvider)
    ..invalidate(tradingTransfersProvider)
    ..invalidate(walletAccountsProvider)
    ..invalidate(walletOverviewProvider)
    ..invalidate(walletActivityProvider)
    ..invalidate(accountsProvider);
}

/* ------------------------------------------------------------------ amounts */

/// "1234.5" -> "1,234.50" (web fmt: at least `dp` decimals, up to 6 when the value has them).
String fmt(Object? v, [int dp = 2]) => Fmt.amount(v, dp);

/// Amount fields (web cleanAmount): a comma becomes the decimal point, anything that isn't a digit goes, one point
/// and 2 decimals are kept.
String cleanAmount(String raw) {
  final v = raw.replaceAll(',', '.').replaceAll(RegExp(r'[^\d.]'), '');
  final dot = v.indexOf('.');
  if (dot < 0) return v;
  final rest = v.substring(dot + 1).replaceAll('.', '');
  return v.substring(0, dot + 1) + (rest.length > 2 ? rest.substring(0, 2) : rest);
}

final RegExp _amount2 = RegExp(r'^\d{1,12}(\.\d{1,2})?$');
final RegExp _amount6 = RegExp(r'^\d{1,12}(\.\d{1,6})?$');

/// A withdrawal / transfer amount: up to 2 decimals and above zero.
bool amountOk(String amount) {
  final a = amount.trim();
  return _amount2.hasMatch(a) && (double.tryParse(a) ?? 0) > 0;
}

/// A deposit amount: up to 6 decimals, above zero and at least the network's minimum.
bool depositAmountOk(String amount, String? minDeposit) {
  final a = amount.trim();
  final n = double.tryParse(a) ?? 0;
  return _amount6.hasMatch(a) && n >= _num(minDeposit) && n > 0;
}

/// A transfer amount within `max` (web: Number(amt) <= max + 1e-9).
bool transferAmountOk(String amount, double max) => amountOk(amount) && double.parse(amount.trim()) <= max + 1e-9;

/// The Max button (web `String(Math.floor(v * 100) / 100)`): cents rounded down, written like JavaScript ("3250.4").
String maxAmount(Object? v) {
  final n = (_num(v) * 100).floorToDouble() / 100;
  if (!n.isFinite || n <= 0) return '0';
  return n == n.truncateToDouble() ? n.toInt().toString() : n.toString();
}

/// "1.00 USDT" or "1.00 USDT + 0.5%" (the withdrawal fee tile).
String feeLabel(WalletLimits l) => '${fmt(l.withdrawFeeFlat)} USDT${_num(l.withdrawFeePct) > 0 ? ' + ${l.withdrawFeePct}%' : ''}';

/// Share of today's withdrawal limit used (0..1).
double limitUsed(String used, String max) {
  final m = _num(max);
  return m > 0 ? (_num(used) / m).clamp(0.0, 1.0) : 0;
}

/// A fresh request id per submitted form (web requestId: 12 random bytes as hex); the service makes the request
/// idempotent on it, so a double tap books once.
String requestId([Random? random]) {
  final r = random ?? Random.secure();
  return List.generate(12, (_) => r.nextInt(256).toRadixString(16).padLeft(2, '0')).join();
}

/* ------------------------------------------------------------------ addresses, hashes, links */

final RegExp _bscAddress = RegExp(r'^0x[0-9a-fA-F]{40}$');
final RegExp _tronAddress = RegExp(r'^T[1-9A-HJ-NP-Za-km-z]{33}$');

/// Address format check (the service validates checksums too).
bool addressLooksValid(String chain, String address) {
  final v = address.trim();
  return chain == 'bsc' ? _bscAddress.hasMatch(v) : _tronAddress.hasMatch(v);
}

/// A transaction hash as the deposit form accepts it (64 hex characters, optionally 0x).
bool txHashLooksValid(String hash) => RegExp(r'^(0x)?[0-9a-fA-F]{64}$').hasMatch(hash.trim());

/// A deposit request id (dep_ + 24 hex).
bool intentIdValid(String? id) => id != null && RegExp(r'^dep_[0-9a-f]{24}$').hasMatch(id);

/// "0x9c1f…e2a4" (web shortHash).
String shortHash(String h, [int head = 6, int tail = 4]) => h.length <= head + tail + 1 ? h : '${h.substring(0, head)}…${h.substring(h.length - tail)}';

/// The Client Area's host for the wallet apps' in-app browsers (web window.location.host).
String appHost(String appUrl) {
  final u = Uri.tryParse(appUrl);
  if (u == null || u.host.isEmpty) return 'app.kalkstrade.com';
  return u.hasPort ? '${u.host}:${u.port}' : u.host;
}

/// Opens this deposit inside the MetaMask app, whose browser injects the wallet (web metamaskDeepLink).
String metamaskDeepLink(String host, String intentId) => 'https://metamask.app.link/dapp/$host/wallet/deposit?intent=${Uri.encodeComponent(intentId)}';

/// Opens this deposit inside the TronLink app's DApp browser (TronLink "open DApp" deep link).
String tronlinkDeepLink(String host, String intentId) {
  final param = jsonEncode({
    'url': 'https://$host/wallet/deposit?intent=${Uri.encodeComponent(intentId)}',
    'action': 'open',
    'protocol': 'tronlink',
    'version': '1.0',
  });
  return 'tronlinkoutside://pull.activity?param=${Uri.encodeComponent(param)}';
}

/// The wallet app's deep link for this deposit's network.
String walletAppLink(String chain, String host, String intentId) => chain == 'bsc' ? metamaskDeepLink(host, intentId) : tronlinkDeepLink(host, intentId);

/* ------------------------------------------------------------------ statuses, titles */

/// A status chip: tone and message key (web StatusTag props).
typedef WalletStatus = ({KChipTone tone, String label});

const Map<String, WalletStatus> kDepositStatus = {
  'pending': (tone: KChipTone.warn, label: 'wallet.status.deposit.pending'),
  'confirming': (tone: KChipTone.info, label: 'wallet.status.deposit.confirming'),
  'credited': (tone: KChipTone.up, label: 'wallet.status.deposit.credited'),
  'failed': (tone: KChipTone.down, label: 'common.failed'),
  'review': (tone: KChipTone.warn, label: 'wallet.status.deposit.review'),
  'unmatched': (tone: KChipTone.warn, label: 'wallet.status.deposit.review'),
  'rejected': (tone: KChipTone.down, label: 'wallet.status.deposit.rejected'),
};

const Map<String, WalletStatus> kWithdrawalStatus = {
  'requested': (tone: KChipTone.warn, label: 'wallet.status.withdrawal.requested'),
  'approved': (tone: KChipTone.info, label: 'common.approved'),
  'paid': (tone: KChipTone.info, label: 'wallet.status.withdrawal.paid'),
  'completed': (tone: KChipTone.up, label: 'common.completed'),
  'rejected': (tone: KChipTone.down, label: 'common.rejected'),
  'cancelled': (tone: KChipTone.neutral, label: 'common.cancelled'),
};

const Map<String, WalletStatus> kTransferStatus = {
  'pending': (tone: KChipTone.warn, label: 'common.processing'),
  'completed': (tone: KChipTone.up, label: 'common.completed'),
  'failed': (tone: KChipTone.down, label: 'common.failed'),
};

/// Other credits / debits by kind (web KIND_LABEL).
const Map<String, String> kKindLabel = {
  'commission': 'wallet.kind.commission',
  'ib_payout': 'wallet.kind.ibPayout',
  'prop_purchase': 'wallet.kind.propPurchase',
  'prop_payout': 'wallet.kind.propPayout',
  'pamm_invest': 'wallet.kind.pammInvest',
  'pamm_redeem': 'wallet.kind.pammRedeem',
  'copy_fee': 'wallet.kind.copyFee',
  'mam_fee': 'wallet.kind.mamFee',
  'adjustment': 'wallet.kind.adjustment',
  'adjustment_in': 'wallet.kind.adjustment',
  'adjustment_out': 'wallet.kind.adjustment',
  'manual_deposit': 'wallet.txType.deposit',
  'manual_withdrawal': 'wallet.txType.withdrawal',
  'refund': 'wallet.kind.refund',
};

/// The status chip of an activity row (null for other credits).
WalletStatus? activityStatus(ActivityItem a) => switch (a.type) {
  'deposit' => kDepositStatus[a.status],
  'withdrawal' => kWithdrawalStatus[a.status],
  'transfer' => kTransferStatus[a.status],
  _ => null,
};

/// The title of an activity row (web activityTitle).
String activityTitle(ActivityItem a, T t) {
  switch (a.type) {
    case 'deposit':
      if (a.kind == 'nowpayments') return t('wallet.nowpayments.activityLine', {'coin': npCoinName(a.note)});
      return t('wallet.depositLine', {'network': a.chain == null ? '' : chainLabel(a.chain).short}).trim();
    case 'withdrawal':
      return t('wallet.withdrawalLine', {'network': a.chain == null ? '' : chainLabel(a.chain).short}).trim();
    case 'transfer':
      return a.direction == 'out' ? t('wallet.activity.toTrading', {'login': a.login}) : t('wallet.activity.fromTrading', {'login': a.login});
  }
  final kind = kKindLabel[a.kind ?? ''];
  return kind != null ? t(kind) : t('wallet.activity.walletTx');
}

/// The detail after the date: confirmations of a confirming deposit, a withdrawal's address, or the statement note.
String activitySub(ActivityItem a, T t) {
  final req = a.requiredConfirmations ?? 0;
  if (a.type == 'deposit' && a.status == 'confirming' && req > 0) {
    return t('wallet.activity.confirmations', {'done': min(a.confirmations ?? 0, req), 'required': req});
  }
  if (a.type == 'withdrawal' && a.address != null && a.address!.isNotEmpty) return t('wallet.activity.to', {'address': shortHash(a.address!)});
  // a NOWPayments deposit's note is its coin, already in the title
  if (a.kind == 'nowpayments') return '';
  return a.note ?? '';
}

/// The currency shown after an activity amount (money back from a trading account is USD).
String activityCurrency(ActivityItem a) => a.type == 'transfer' && a.inbound ? 'USD' : 'USDT';

/// The KYC notice of the wallet pages: null when verified, else its title key.
String? kycNoticeTitle(KycStatus status) => switch (status) {
  KycStatus.verified => null,
  KycStatus.pending => 'wallet.kyc.inReview',
  KycStatus.rejected => 'wallet.kyc.needsAttention',
  KycStatus.unverified => 'wallet.kyc.verifyToWithdraw',
};

/* ------------------------------------------------------------------ accounts */

/// Live accounts the wallet can move money to and from (prop challenges hold simulated capital; disabled and
/// expired accounts take no transfers).
bool isTransferable(EngineAccount a) => a.live && !a.prop && a.status != 'disabled' && a.status != 'expired';

/// Live accounts counted for "Move to another account" (two or more).
bool isBetweenEligible(EngineAccount a) => a.live && !a.prop && a.status != 'archived' && a.status != 'closed';

/// What can leave a trading account, in USD (web toUsd(a, a.withdrawable)).
double withdrawableUsd(EngineAccount a) => Fmt.toUsd(_num(a.raw['withdrawable'] ?? a.balance), cent: a.cent || a.currency == 'USC').toDouble();

/// A trading account's balance in USD (cent accounts / 100).
double balanceUsd(EngineAccount a) => Fmt.toUsd(a.balance, cent: a.cent || a.currency == 'USC').toDouble();

/* ------------------------------------------------------------------ errors */

/// The text of a wallet error (web walletApi + FRIENDLY): outages of the wallet service get its own wording,
/// everything else the server's message (in the reader's language when the code is known: localizeError).
String walletErrorText(Object? e, T t, {String? fallback}) {
  if (e is! ApiException) return fallback ?? t('wallet.error.generic');
  if (e.isNetwork) return t('common.networkError');
  const friendly = {'unavailable': 'wallet.error.unavailable', 'insufficient_funds': 'wallet.error.insufficientFunds'};
  final key = friendly[e.code];
  if (key != null && e.status >= 500) return t(key);
  return localizeError(e, t);
}

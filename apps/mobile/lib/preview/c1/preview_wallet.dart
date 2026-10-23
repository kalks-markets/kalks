// Sample answers for the Wallet pages (development previews and widget tests only; never in a shipped build).
// Shapes are the real API's (the web's /api/wallet/... routes, apps/crm/app/api/wallet/[...path]/route.ts and
// services/wallet/README.md); values are made up. The wallet keeps a little state so the flows work end to end:
// a deposit request advances pending -> confirming -> credited across its 5 s polls, withdrawals lock funds and can be
// cancelled, transfers move the balance, and every write is logged in [previewWalletCalls] for the tests.
// `wallet/overview` stays compatible with the dashboard (USDT total 3,250.40 before any change).
// Return null for paths this file doesn't answer.
import 'dart:math' as math;

import '../preview_data.dart';

/// Every write the wallet pages sent (method, path, body), oldest first.
final List<({String method, String path, Map<String, dynamic> body})> previewWalletCalls = [];

/// Every read the wallet pages sent (the path), oldest first: the tests count the polls.
final List<String> previewWalletReads = [];

/// Back to the starting sample (tests call it in setUp).
void resetPreviewWallet() {
  previewWalletCalls.clear();
  previewWalletReads.clear();
  _s = _WalletState();
  previewNowPayments = false;
}

/// Crypto deposits through NOWPayments offered (off by default, like a broker that never switched them on).
bool previewNowPayments = false;

/// The sample NOWPayments coins (`wallet/nowpayments/config`).
const List<Map<String, Object?>> _npCoins = [
  {
    'code': 'usdttrc20',
    'ticker': 'USDT',
    'name': 'USDT',
    'network': 'TRON (TRC20)',
    'icon': 'usdt',
    'chain_icon': 'trx',
    'needs_extra_id': false,
    'min_usd': '10',
  },
  {'code': 'btc', 'ticker': 'BTC', 'name': 'Bitcoin', 'network': 'Bitcoin', 'icon': 'btc', 'chain_icon': null, 'needs_extra_id': false, 'min_usd': '25.01'},
  {'code': 'xrp', 'ticker': 'XRP', 'name': 'XRP', 'network': 'XRP Ledger', 'icon': 'xrp', 'chain_icon': null, 'needs_extra_id': true, 'min_usd': '10'},
];

/// A crypto payment: waiting for 2 polls, confirming for 2, then credited.
Map<String, Object?> _npPoll(Map<String, Object?> p) {
  final polls = (p['_polls'] as int? ?? 0) + 1;
  p['_polls'] = polls;
  final status = polls <= 2 ? 'waiting' : (polls <= 4 ? 'confirming' : 'credited');
  if (status != 'waiting') p['actually_paid'] = p['pay_amount'];
  if (status == 'credited' && p['status'] != 'credited') {
    p['credit_amount'] = p['price_amount'];
    p['credited_at'] = _iso(DateTime.now());
    _s.available += _d(p['price_amount']);
  }
  p['status'] = status;
  return {
    for (final e in p.entries)
      if (!e.key.startsWith('_')) e.key: e.value,
  };
}

_WalletState _s = _WalletState();

const String _bscAddress = '0x7a1F3c9B2e4D5f60718293aBcDeF0123456789aB';
const String _tronAddress = 'TQn9Y2khEsLJW1ChVWFMSMeRDow5KcbLSE';
const List<int> _bscSteps = [0, 4, 9, 15];
const List<int> _tronSteps = [0, 6, 13, 20];

String _money(double v) => v.toStringAsFixed(2);
double _d(Object? v) => v is num ? v.toDouble() : double.tryParse('${v ?? ''}') ?? 0;
String _iso(DateTime t) => t.toUtc().toIso8601String();
String _hex(int seed, int length) {
  final r = math.Random(seed);
  return List.generate(length, (_) => '0123456789abcdef'[r.nextInt(16)]).join();
}

String _explorer(String chain, String hash) => chain == 'bsc' ? 'https://bscscan.com/tx/$hash' : 'https://tronscan.org/#/transaction/$hash';

Map<String, dynamic> _err(String code, String message, [String? field]) => {
  'error': {'code': code, 'message': message, 'field': ?field},
};

/// The wallet's networks and limits (a superset of preview_data's, which the dashboard reads).
Map<String, dynamic> get previewWalletConfigFull => {
  'chains': [
    {
      'chain': 'bsc',
      'network': 'BEP20',
      'token': 'USDT',
      'token_contract': '0x55d398326f99059fF775485246999027B3197955',
      'decimals': 18,
      'evm_chain_id': 56,
      'confirmations': 15,
      'deposits_enabled': true,
      'withdrawals_enabled': true,
      'min_deposit': '10',
      'withdraw_fee': '0.5',
    },
    {
      'chain': 'tron',
      'network': 'TRC20',
      'token': 'USDT',
      'token_contract': 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',
      'decimals': 6,
      'evm_chain_id': null,
      'confirmations': 20,
      'deposits_enabled': true,
      'withdrawals_enabled': true,
      'min_deposit': '10',
      'withdraw_fee': '1',
    },
  ],
  'limits': {
    'withdraw_min': '20',
    'withdraw_max': '10000',
    'withdraw_daily_max': '50000',
    'withdraw_fee_flat': '1',
    'withdraw_fee_pct': '0',
    'deposit_cooldown_hours': 24,
    'intent_ttl_minutes': 60,
  },
};

class _Intent {
  _Intent(this.id, this.chain, this.amount, this.createdAt, {this.hash, this.polls = 0, this.depositId = 0, this.submittedAt});
  final String id, chain, amount;
  final DateTime createdAt;
  String? hash;
  int polls;
  int depositId;
  DateTime? submittedAt;
  bool credited = false;

  List<int> get steps => chain == 'bsc' ? _bscSteps : _tronSteps;
  int get step => math.min(polls, steps.length - 1);
  String get status => hash == null ? 'open' : (step == steps.length - 1 ? 'completed' : 'submitted');
  String get depositStatus => step == 0 ? 'pending' : (step == steps.length - 1 ? 'credited' : 'confirming');

  Map<String, dynamic> intentJson() => {
    'id': id,
    'chain': chain,
    'network': chain == 'bsc' ? 'BEP20' : 'TRC20',
    'currency': 'USDT',
    'amount': amount,
    'address': chain == 'bsc' ? _bscAddress : _tronAddress,
    'token_contract': chain == 'bsc' ? '0x55d398326f99059fF775485246999027B3197955' : 'TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t',
    'decimals': chain == 'bsc' ? 18 : 6,
    'evm_chain_id': chain == 'bsc' ? 56 : null,
    'status': status,
    'expires_at': _iso(createdAt.add(const Duration(minutes: 60))),
    'created_at': _iso(createdAt),
  };

  Map<String, dynamic>? depositJson() {
    final h = hash;
    if (h == null) return null;
    final st = depositStatus;
    return {
      'id': depositId,
      'intent_id': id,
      'chain': chain,
      'network': chain == 'bsc' ? 'BEP20' : 'TRC20',
      'tx_hash': h,
      'explorer_url': _explorer(chain, h),
      'from_address': chain == 'bsc' ? '0x3f5CE5FBFe3E9af3971dD833D26bA9b5C936f0bE' : 'TJDENsfBJs4RFETt1X1W8wMDc8M5XnJhCe',
      'amount': st == 'pending' ? null : amount,
      'expected_amount': amount,
      'confirmations': steps[step],
      'required_confirmations': steps.last,
      'status': st,
      'review_reason': null,
      'failure_reason': null,
      'credited_at': st == 'credited' ? _iso(DateTime.now()) : null,
      'created_at': _iso(submittedAt ?? createdAt),
    };
  }
}

class _WalletState {
  /// NOWPayments crypto payments by id (polled: waiting -> confirming -> credited).
  final Map<int, Map<String, Object?>> npPayments = {};

  _WalletState() {
    final now = DateTime.now();
    // a deposit being confirmed (the overview's "In progress"), resumable at /wallet/deposit?intent=…
    intents['dep_4c1d9e2f7a8b6c5d3e2f1a0b'] = _Intent(
      'dep_4c1d9e2f7a8b6c5d3e2f1a0b',
      'bsc',
      '500.00',
      now.subtract(const Duration(minutes: 9)),
      hash: '0x${_hex(7, 64)}',
      polls: 2,
      depositId: 912,
      submittedAt: now.subtract(const Duration(minutes: 6)),
    );
    withdrawals.addAll([
      _withdrawal(318, 'tron', 'TXLAQ63Xg1NAzckPwKHvzw7CSEmLMEqcdj', 250, 'requested', now.subtract(const Duration(hours: 2))),
      _withdrawal(301, 'bsc', '0x8894E0a0c962CB723c1976a4421c95949bE2D4E3', 400, 'completed', now.subtract(const Duration(days: 9)), paid: true),
      _withdrawal(
        287,
        'tron',
        'TNXoiAJ3dct8Fjg4M9fkLFh9S2v9TXc32G',
        1200,
        'rejected',
        now.subtract(const Duration(days: 20)),
        reason: 'The destination is an exchange deposit address on another network.',
      ),
      _withdrawal(260, 'bsc', '0x8894E0a0c962CB723c1976a4421c95949bE2D4E3', 100, 'cancelled', now.subtract(const Duration(days: 31))),
    ]);
    transfers.addAll([
      _transfer(412, 10042817, 'to_trading', 1000, 'completed', now.subtract(const Duration(days: 1))),
      _transfer(405, 10042817, 'from_trading', 300, 'completed', now.subtract(const Duration(days: 3))),
      _transfer(398, 10051123, 'to_trading', 50, 'completed', now.subtract(const Duration(days: 5))),
      _transfer(377, 10042817, 'to_trading', 200, 'failed', now.subtract(const Duration(days: 12)), error: 'The account is in close-only mode.'),
    ]);
    notices.addAll([
      {
        'id': 77,
        'kind': 'deposit.credited',
        'title': 'Deposit credited',
        'body': '1,500.00 USDT arrived in your wallet (TRC20).',
        'read': false,
        'created_at': _iso(now.subtract(const Duration(hours: 5))),
      },
      {
        'id': 76,
        'kind': 'withdrawal.requested',
        'title': 'Withdrawal requested',
        'body': '250.00 USDT to TXLAQ6…cdj is waiting for review.',
        'read': false,
        'created_at': _iso(now.subtract(const Duration(hours: 2))),
      },
      {
        'id': 71,
        'kind': 'withdrawal.completed',
        'title': 'Withdrawal sent',
        'body': '399.50 USDT reached your BEP20 address.',
        'read': true,
        'created_at': _iso(now.subtract(const Duration(days: 8))),
      },
    ]);
  }

  // 3,250.40 in total, as the dashboard's sample: 250 of it locked by the open withdrawal
  double available = 3000.40;
  double locked = 250;
  double usedToday = 250;
  final Map<String, _Intent> intents = {};
  final List<Map<String, dynamic>> withdrawals = [];
  final List<Map<String, dynamic>> transfers = [];
  final List<Map<String, dynamic>> notices = [];

  /// Rows made in this session (deposits credited, withdrawals, transfers), newest first.
  final List<Map<String, dynamic>> extraActivity = [];
  final Map<String, Map<String, dynamic>> byKey = {};
  int nextIntent = 1;
  int nextId = 1000;
}

Map<String, dynamic> _withdrawal(int id, String chain, String to, double amount, String status, DateTime at, {bool paid = false, String? reason}) {
  final fee = 1.0 + (chain == 'bsc' ? 0.5 : 1.0);
  final hash = paid ? '0x${_hex(id, 64)}' : null;
  return {
    'id': id,
    'chain': chain,
    'network': chain == 'bsc' ? 'BEP20' : 'TRC20',
    'to_address': to,
    'amount': _money(amount),
    'fee': _money(fee),
    'net_amount': _money(amount - fee),
    'status': status,
    'reason': reason,
    'payout_tx_hash': hash,
    'explorer_url': hash == null ? null : _explorer(chain, hash),
    'payout_confirmations': paid ? 15 : 0,
    'created_at': _iso(at),
    'updated_at': _iso(at),
    'completed_at': status == 'completed' ? _iso(at.add(const Duration(hours: 3))) : null,
  };
}

Map<String, dynamic> _transfer(int id, int login, String direction, double amount, String status, DateTime at, {String? error}) => {
  'id': id,
  'login': login,
  'direction': direction,
  'amount': _money(amount),
  'status': status,
  'error_message': error,
  'engine_amount': login == 10051123 ? _money(amount * 100) : _money(amount),
  'engine_currency': login == 10051123 ? 'USC' : 'USD',
  'created_at': _iso(at),
};

/// The wallet's activity: this session's rows, the live deposit / withdrawals / transfers, then older history.
List<Map<String, dynamic>> _activity() {
  final now = DateTime.now();
  final rows = <Map<String, dynamic>>[..._s.extraActivity];
  for (final i in _s.intents.values) {
    final d = i.depositJson();
    if (d == null || i.credited) continue;
    rows.add({
      'type': 'deposit',
      'id': '${i.depositId}',
      'status': d['status'],
      'amount': i.amount,
      'currency': 'USDT',
      'chain': i.chain,
      'network': d['network'],
      'tx_hash': i.hash,
      'explorer_url': d['explorer_url'],
      'direction': 'in',
      'confirmations': d['confirmations'],
      'required_confirmations': d['required_confirmations'],
      'created_at': d['created_at'],
      'updated_at': d['created_at'],
    });
  }
  for (final w in _s.withdrawals) {
    rows.add({
      'type': 'withdrawal',
      'id': '${w['id']}',
      'status': w['status'],
      'amount': w['amount'],
      'currency': 'USDT',
      'chain': w['chain'],
      'network': w['network'],
      'tx_hash': w['payout_tx_hash'],
      'explorer_url': w['explorer_url'],
      'direction': 'out',
      'fee': w['fee'],
      'net_amount': w['net_amount'],
      'address': w['to_address'],
      'created_at': w['created_at'],
      'updated_at': w['updated_at'],
    });
  }
  for (final x in _s.transfers) {
    rows.add({
      'type': 'transfer',
      'id': '${x['id']}',
      'status': x['status'],
      'amount': x['amount'],
      'currency': 'USDT',
      'login': x['login'],
      'direction': x['direction'] == 'to_trading' ? 'out' : 'in',
      'created_at': x['created_at'],
      'updated_at': x['created_at'],
    });
  }
  // older history: credited deposits and partner / prop credits, so the history pages have a second page
  const kinds = ['commission', 'ib_payout', 'adjustment_in', 'prop_payout'];
  const notes = ['IB commission · September', 'Partner payout · week 38', 'Welcome bonus correction', 'Prop payout · challenge #4471'];
  for (var i = 0; i < 26; i++) {
    final at = now.subtract(Duration(days: 2 + i * 2, hours: i * 3 % 11));
    if (i % 3 == 2) {
      final k = i ~/ 3 % kinds.length;
      rows.add({
        'type': 'other',
        'id': 'o$i',
        'status': 'completed',
        'amount': _money(20 + i * 7.5),
        'currency': 'USDT',
        'direction': 'in',
        'kind': kinds[k],
        'note': notes[k],
        'created_at': _iso(at),
        'updated_at': _iso(at),
      });
    } else {
      final chain = i.isEven ? 'tron' : 'bsc';
      final hash = '${chain == 'bsc' ? '0x' : ''}${_hex(100 + i, 64)}';
      rows.add({
        'type': 'deposit',
        'id': '${800 - i}',
        'status': i == 7 ? 'rejected' : 'credited',
        'amount': _money(i == 0 ? 1500 : 100 + i * 25),
        'currency': 'USDT',
        'chain': chain,
        'network': chain == 'bsc' ? 'BEP20' : 'TRC20',
        'tx_hash': hash,
        'explorer_url': _explorer(chain, hash),
        'direction': 'in',
        'created_at': _iso(at),
        'updated_at': _iso(at),
      });
    }
  }
  rows.sort((a, b) => '${b['created_at']}'.compareTo('${a['created_at']}'));
  return rows;
}

Map<String, dynamic> _overview() {
  final pending = [
    for (final i in _s.intents.values)
      if (i.hash != null && !i.credited) i.depositJson()!,
  ];
  return {
    'balances': [
      {'currency': 'USDT', 'available': _money(_s.available), 'locked': _money(_s.locked)},
    ],
    'pending_deposits': pending,
    'open_withdrawals': [
      for (final w in _s.withdrawals)
        if (w['status'] == 'requested' || w['status'] == 'approved' || w['status'] == 'paid') w,
    ],
    'limits': {'used_today': _money(_s.usedToday), 'remaining_today': _money(50000 - _s.usedToday), 'daily_max': '50000', 'cooldown_until': null},
    'notifications_unread': _s.notices.where((n) => n['read'] != true).length,
  };
}

/// A poll of a deposit request: the deposit moves one step on, and is credited at the last one.
Map<String, dynamic> _pollIntent(_Intent i) {
  final view = {'intent': i.intentJson(), 'deposit': i.depositJson()};
  if (i.hash != null) {
    if (i.depositStatus == 'credited' && !i.credited) {
      i.credited = true;
      _s.available += _d(i.amount);
      _s.extraActivity.insert(0, {
        'type': 'deposit',
        'id': '${i.depositId}',
        'status': 'credited',
        'amount': i.amount,
        'currency': 'USDT',
        'chain': i.chain,
        'network': i.chain == 'bsc' ? 'BEP20' : 'TRC20',
        'tx_hash': i.hash,
        'explorer_url': _explorer(i.chain, i.hash!),
        'direction': 'in',
        'created_at': _iso(i.submittedAt ?? i.createdAt),
        'updated_at': _iso(DateTime.now()),
      });
    }
    i.polls++;
  }
  return view;
}

(int, Object) _quote(String chain, String amount, String to) {
  final cfg = previewWalletConfigFull;
  final limits = cfg['limits'] as Map<String, dynamic>;
  final a = _d(amount);
  final valid = chain == 'bsc' ? RegExp(r'^0x[0-9a-fA-F]{40}$').hasMatch(to) : RegExp(r'^T[1-9A-HJ-NP-Za-km-z]{33}$').hasMatch(to);
  if (!valid) return (422, _err('validation', 'Enter a valid ${chain == 'bsc' ? 'BEP20' : 'TRC20'} address.', 'to_address'));
  if (a < _d(limits['withdraw_min'])) return (422, _err('below_minimum', 'The minimum withdrawal is ${limits['withdraw_min']} USDT.', 'amount'));
  if (a > _d(limits['withdraw_max'])) return (422, _err('above_maximum', 'The maximum withdrawal is ${limits['withdraw_max']} USDT.', 'amount'));
  if (_s.usedToday + a > 50000) return (422, _err('daily_limit', 'This is over your daily withdrawal limit.', 'amount'));
  if (a > _s.available) return (422, _err('insufficient_funds', 'Not enough available balance.', 'amount'));
  final fee = _d(limits['withdraw_fee_flat']) + a * _d(limits['withdraw_fee_pct']) / 100 + (chain == 'bsc' ? 0.5 : 1);
  return (
    200,
    {
      'quote': {
        'amount': _money(a),
        'fee': _money(fee),
        'net_amount': _money(a - fee),
        'used_today': _money(_s.usedToday),
        'daily_max': '50000',
        'available': _money(_s.available),
      },
    },
  );
}

Map<String, dynamic> _page(List<Map<String, dynamic>> all, Map<String, String> query, {int fallbackLimit = 50}) {
  final limit = int.tryParse(query['limit'] ?? '') ?? fallbackLimit;
  final page = int.tryParse(query['page'] ?? '') ?? 1;
  final from = math.min((page - 1) * limit, all.length);
  return {'items': all.sublist(from, math.min(from + limit, all.length)), 'page': page, 'limit': limit, 'total': all.length};
}

(int, Object)? previewWallet(String method, String path, Map<String, dynamic> body, Map<String, String> query) {
  if (!path.startsWith('wallet/')) return null;
  final route = path.substring('wallet/'.length);
  if (method != 'GET') previewWalletCalls.add((method: method, path: path, body: Map.of(body)));
  if (method == 'GET') previewWalletReads.add(path);

  if (method == 'GET') {
    switch (route) {
      case 'config':
        return (200, previewWalletConfigFull);
      case 'overview':
        return (200, _overview());
      case 'notifications':
        return (200, {'items': _s.notices, 'unread': _s.notices.where((n) => n['read'] != true).length});
      case 'activity':
        final type = query['type'];
        final rows = _activity().where((r) => type == null || type == 'all' || r['type'] == type).toList();
        return (200, _page(rows, query));
      case 'withdrawals':
        final list = [..._s.withdrawals]..sort((a, b) => '${b['created_at']}'.compareTo('${a['created_at']}'));
        return (200, _page(list, query));
      case 'transfers':
        final list = [..._s.transfers]..sort((a, b) => '${b['created_at']}'.compareTo('${a['created_at']}'));
        return (200, _page(list, query));
      case 'ledger':
        return (200, {'items': <Object>[], 'page': 1, 'limit': 50, 'total': 0});
      case 'nowpayments/config':
        if (!previewNowPayments) return (200, {'enabled': false, 'coins': <Object>[]});
        return (200, {'enabled': true, 'sandbox': false, 'min_deposit_usd': '10', 'max_deposit_usd': '50000', 'fee_paid_by_user': true, 'coins': _npCoins});
    }
    final np = RegExp(r'^nowpayments/payments/(\d{1,18})$').firstMatch(route);
    if (np != null) {
      final p = _s.npPayments[int.parse(np.group(1)!)];
      if (p == null) return (404, _err('not_found', 'Payment not found'));
      return (200, {'payment': _npPoll(p)});
    }
    final intent = RegExp(r'^deposits/intents/(dep_[0-9a-f]{24})$').firstMatch(route);
    if (intent != null) {
      final i = _s.intents[intent.group(1)];
      if (i == null) return (404, _err('not_found', 'Not found.'));
      return (200, _pollIntent(i));
    }
    return (404, _err('not_found', 'Not found.'));
  }

  // writes
  switch (route) {
    case 'nowpayments/payments':
      if (!previewNowPayments) return (422, _err('nowpayments_disabled', 'Crypto deposits are not available at the moment'));
      final amount = '${body['amount'] ?? ''}'.trim();
      if (!RegExp(r'^\d{1,12}(\.\d{1,2})?$').hasMatch(amount) || _d(amount) <= 0) {
        return (422, _err('validation', 'Enter an amount in USD with up to 2 decimals.', 'amount'));
      }
      final coin = _npCoins.where((c) => c['code'] == body['pay_currency']).firstOrNull;
      if (coin == null) return (422, _err('validation', 'Choose one of the listed coins', 'pay_currency'));
      if (_d(amount) < _d(coin['min_usd'])) return (422, _err('below_minimum', 'The minimum for this coin is ${coin['min_usd']} USD'));
      final id = _s.nextId++;
      final p = <String, Object?>{
        'id': id,
        'order_id': 'np_${_hex(id, 24)}',
        'status': 'waiting',
        'np_status': 'waiting',
        'price_amount': amount,
        'price_currency': 'USD',
        'pay_currency': coin['code'],
        'coin': coin,
        'pay_amount': (_d(amount) * (coin['code'] == 'btc' ? 0.0000152 : 1.005)).toStringAsFixed(coin['code'] == 'btc' ? 8 : 6),
        'pay_address': coin['code'] == 'xrp'
            ? 'rLNaPoKeeBjZe2qs6x52yVPZpZ8td4dc6w'
            : (coin['code'] == 'btc' ? 'bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh' : _tronAddress),
        'payin_extra_id': coin['needs_extra_id'] == true ? '771234' : null,
        'expires_at': _iso(DateTime.now().add(const Duration(minutes: 20))),
        'actually_paid': null,
        'credit_amount': null,
        'credited_at': null,
        'sandbox': false,
        'created_at': _iso(DateTime.now()),
        '_polls': 0,
      };
      _s.npPayments[id] = p;
      return (
        200,
        {
          'payment': {
            for (final e in p.entries)
              if (!e.key.startsWith('_')) e.key: e.value,
          },
        },
      );
    case 'notifications/read':
      for (final n in _s.notices) {
        n['read'] = true;
      }
      return (200, {'ok': true});
    case 'deposits/intents':
      final chain = body['chain'];
      if (chain != 'bsc' && chain != 'tron') return (422, _err('validation', 'Choose a network.', 'chain'));
      final amount = '${body['amount'] ?? ''}'.trim();
      if (!RegExp(r'^\d{1,12}(\.\d{1,6})?$').hasMatch(amount) || _d(amount) <= 0) {
        return (422, _err('validation', 'Enter an amount with up to 6 decimals.', 'amount'));
      }
      if (_d(amount) < 10) return (422, _err('below_minimum', 'The minimum deposit is 10 USDT.', 'amount'));
      final id = 'dep_${_hex(9000 + _s.nextIntent++, 24)}';
      final i = _Intent(id, '$chain', amount, DateTime.now());
      _s.intents[id] = i;
      return (200, {'intent': i.intentJson()});
    case 'deposits/submit':
      final i = _s.intents['${body['intent_id']}'];
      if (i == null) return (422, _err('validation', 'Unknown deposit request.', 'intent_id'));
      final hash = '${body['tx_hash'] ?? ''}'.trim();
      if (!RegExp(r'^(0x)?[0-9a-fA-F]{64}$').hasMatch(hash)) {
        return (422, _err('validation', 'Enter the transaction hash (64 hexadecimal characters).', 'tx_hash'));
      }
      if (_s.intents.values.any((x) => x.hash?.toLowerCase() == hash.toLowerCase() && x != i)) {
        return (409, _err('tx_already_used', 'This transaction was already submitted.'));
      }
      i
        ..hash = hash
        ..submittedAt = DateTime.now()
        ..depositId = _s.nextId++
        ..polls = 0;
      return (200, {'deposit': i.depositJson()});
    case 'withdrawals/quote':
      return _quote('${body['chain']}', '${body['amount'] ?? ''}'.trim(), '${body['to_address'] ?? ''}'.trim());
    case 'withdrawals':
      final chain = '${body['chain']}';
      final amount = '${body['amount'] ?? ''}'.trim();
      final to = '${body['to_address'] ?? ''}'.trim();
      final key = '${body['idempotency_key'] ?? ''}';
      if (!RegExp(r'^[A-Za-z0-9:_-]{8,128}$').hasMatch(key)) return (422, _err('validation', 'Missing request id.', 'idempotency_key'));
      // every check first, then the emailed code (as the BFF)
      final q = _quote(chain, amount, to);
      if (q.$1 != 200) return q;
      if (body['stepup_token'] == null || '${body['stepup_token']}'.isEmpty) {
        return (403, _err('stepup_required', 'Confirm this withdrawal with the code we email you.'));
      }
      final replay = _s.byKey['w:$key'];
      if (replay != null) return (200, {'withdrawal': replay});
      final quote = (q.$2 as Map)['quote'] as Map;
      final a = _d(amount);
      final w = _withdrawal(_s.nextId++, chain, to, a, 'requested', DateTime.now())
        ..['fee'] = quote['fee']
        ..['net_amount'] = quote['net_amount'];
      _s
        ..available -= a
        ..locked += a
        ..usedToday += a;
      _s.withdrawals.insert(0, w);
      _s.byKey['w:$key'] = w;
      return (200, {'withdrawal': w});
    case 'transfers/to-trading':
    case 'transfers/from-trading':
      final login = body['login'];
      if (login is! int || !RegExp(r'^\d{8}$').hasMatch('$login')) return (422, _err('validation', 'Choose a trading account.', 'login'));
      final amount = '${body['amount'] ?? ''}'.trim();
      if (!RegExp(r'^\d{1,12}(\.\d{1,2})?$').hasMatch(amount) || _d(amount) <= 0) {
        return (422, _err('validation', 'Enter an amount with up to 2 decimals.', 'amount'));
      }
      final key = '${body['idempotency_key'] ?? ''}';
      if (!RegExp(r'^[A-Za-z0-9:_-]{8,128}$').hasMatch(key)) return (422, _err('validation', 'Missing request id.', 'idempotency_key'));
      final replay = _s.byKey['t:$key'];
      if (replay != null) return (200, {'transfer': replay});
      final account = (previewAccounts['accounts'] as List).cast<Map<String, dynamic>>().where((a) => a['login'] == login).firstOrNull;
      if (account == null || account['type'] != 'live') return (422, _err('not_own_account', 'Choose one of your own live accounts.', 'login'));
      final a = _d(amount);
      final toTrading = route.endsWith('to-trading');
      if (toTrading && a > _s.available) return (422, _err('insufficient_funds', 'Not enough available balance.', 'amount'));
      final cent = account['cent'] == true;
      if (!toTrading && a > _d(account['withdrawable']) / (cent ? 100 : 1)) return (422, _err('no_money', 'Not enough free margin on the account.', 'amount'));
      _s.available += toTrading ? -a : a;
      final x = _transfer(_s.nextId++, login, toTrading ? 'to_trading' : 'from_trading', a, 'completed', DateTime.now());
      _s.transfers.insert(0, x);
      _s.byKey['t:$key'] = x;
      return (200, {'transfer': x});
  }
  final cancel = RegExp(r'^withdrawals/(\d+)/cancel$').firstMatch(route);
  if (cancel != null) {
    final w = _s.withdrawals.where((x) => '${x['id']}' == cancel.group(1)).firstOrNull;
    if (w == null) return (404, _err('not_found', 'Not found.'));
    if (w['status'] != 'requested') return (409, _err('invalid_state', 'Only a withdrawal waiting for review can be cancelled.'));
    final a = _d(w['amount']);
    w['status'] = 'cancelled';
    _s
      ..available += a
      ..locked = math.max(0, _s.locked - a);
    return (200, {'withdrawal': w});
  }
  return (404, _err('not_found', 'Not found.'));
}

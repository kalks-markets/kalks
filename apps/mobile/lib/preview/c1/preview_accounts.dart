// Sample answers for the Accounts pages (development previews and widget tests only; never in a shipped build).
// Shapes are the real API's (the web's /api/trading/... routes: apps/crm/app/api/trading/[...path]/route.ts and the
// engine's account views); values are made up. Return null for paths this file doesn't answer.
//
// `trading/accounts` is a superset of the shared sample (lib/preview/preview_data.dart): the same three accounts and
// figures, plus the default star, demo terms, one pending order, an archived account and a demo Options account
// (the Options Standard group: options trade in their own account; the groups carry their product). Changes made in the preview
// (rename, default, leverage, archive / restore, closure requests, new accounts) are kept until the app restarts
// (`resetPreviewAccounts()` in tests). Step-up protected writes answer 403 stepup_required without `stepup_token`.
import '../../features/terminal/preview/preview_server.dart';
import '../preview_data.dart' as shared;

Map<String, dynamic> _error(String code, String message) => {
  'error': {'code': code, 'message': message},
};

(int, Object) _stepupRequired() => (403, _error('stepup_required', 'Confirm this change with the code we emailed you.'));

const List<String> _reasons = ['costs', 'platform', 'performance', 'other_broker', 'stop_trading', 'too_many_accounts', 'service', 'other'];

/* ------------------------------------------------------------------ preview state */

final Map<int, String> _names = {};
int? _defaultLogin = 10042817;
final Map<int, int> _leverage = {};
final Map<int, String> _status = {};
final Map<int, Map<String, dynamic>> _closure = {};
final List<Map<String, dynamic>> _opened = [];

/// Every write the Accounts pages sent (method, path, body), for the tests.
final List<({String method, String path, Map<String, dynamic> body})> previewAccountCalls = [];

/// Forget the preview's changes (tests).
void resetPreviewAccounts() {
  previewAccountCalls.clear();
  _names.clear();
  _defaultLogin = 10042817;
  _leverage.clear();
  _status.clear();
  _closure.clear();
  _opened.clear();
}

String _ago(Duration d) => DateTime.now().toUtc().subtract(d).toIso8601String();

/// The sample Options account (demo, Options Standard): the options workspace of Kalks Trader opens on it.
const int previewOptionsLogin = 20019001;

Map<String, dynamic> get _optionsAccount => {
  ...(shared.previewAccounts['accounts'] as List).first as Map<String, dynamic>,
  'login': previewOptionsLogin,
  'type': 'demo',
  'group': 'options-standard',
  'groupName': 'Options Standard',
  'product': 'options',
  'name': '',
  'leverage': 100,
  'leverages': [100],
  'positions': 0,
  'orders': 0,
  'balance': 10000,
  'profit': 0,
  'equity': 10000,
  'margin': 0,
  'freeMargin': 10000,
  'marginLevel': null,
  'withdrawable': 10000,
  'demo': {'initialBalance': 10000, 'refillsPerDay': 3, 'refillsUsedToday': 0, 'expiryDays': 30},
  'createdAt': '2026-09-21T08:00:00Z',
};

/// The archived sample account (Archived tab).
Map<String, dynamic> get _archivedAccount => {
  ...(shared.previewAccounts['accounts'] as List).first as Map<String, dynamic>,
  'login': 10038890,
  'group': 'standard',
  'groupName': 'Standard',
  'name': 'Old scalping',
  'status': 'archived',
  'leverage': 500,
  'positions': 0,
  'orders': 0,
  'balance': 0,
  'profit': 0,
  'equity': 0,
  'margin': 0,
  'freeMargin': 0,
  'marginLevel': null,
  'withdrawable': 0,
  'archivedAt': '2026-08-14T09:30:00Z',
  'createdAt': '2026-04-02T08:00:00Z',
};

Map<String, dynamic> _withState(Map<String, dynamic> a) {
  final login = a['login'] as int;
  final out = {...a};
  if (_names.containsKey(login)) out['name'] = _names[login];
  if (_leverage.containsKey(login)) out['leverage'] = _leverage[login];
  if (_status.containsKey(login)) {
    out['status'] = _status[login];
    if (_status[login] == 'archived') out['archivedAt'] ??= DateTime.now().toUtc().toIso8601String();
  }
  out['isDefault'] = login == _defaultLogin;
  final c = _closure[login];
  out['closureRequest'] = c == null ? null : {'id': c['id'], 'status': c['status']};
  return out;
}

/// Every account of the sample client, with the preview's changes.
List<Map<String, dynamic>> _accounts() {
  final base = [
    for (final a in (shared.previewAccounts['accounts'] as List).cast<Map<String, dynamic>>())
      {
        ...a,
        if (a['login'] == 10042817) ...{
          'orders': 1,
          'leverages': [50, 100, 200, 500],
        },
        if (a['type'] == 'demo') 'demo': {'initialBalance': 25000, 'refillsPerDay': 3, 'refillsUsedToday': 1, 'expiryDays': 30},
      },
    _optionsAccount,
    _archivedAccount,
    ..._opened,
  ];
  return [for (final a in base) _withState(a)];
}

Map<String, dynamic>? _account(int login) {
  for (final a in _accounts()) {
    if (a['login'] == login) return a;
  }
  return null;
}

/* ------------------------------------------------------------------ positions, orders, deals */

Map<String, dynamic> _position(
  int ticket,
  int login,
  String symbol,
  String side,
  double volume,
  double open,
  double current,
  double profit, {
  double? sl,
  double? tp,
  double swap = 0,
  Map<String, dynamic>? option,
  Duration ago = const Duration(hours: 5),
}) => {
  'ticket': ticket,
  'login': login,
  'symbol': symbol,
  'side': side,
  'volume': volume,
  'openPrice': open,
  'openTime': _ago(ago),
  'sl': sl,
  'tp': tp,
  'swap': swap,
  'commission': 0,
  'currentPrice': current,
  'profit': profit,
  'source': 'manual',
  'platform': 'Web',
  'comment': '',
  'option': ?option,
  if (option != null) ...{'premium': -60.0, 'markValue': 85.0, 'mark': 0.0017},
};

List<Map<String, dynamic>> _positions(int login) => switch (login) {
  10042817 => [
    _position(500912, login, 'EURUSD', 'buy', 1, 1.08143, 1.08411, 268.4, sl: 1.0772, tp: 1.0905, swap: -3.1, ago: const Duration(hours: 26)),
    _position(500977, login, 'XAUUSD', 'sell', 0.5, 2662.18, 2659.79, 119.45, sl: 2681.0, ago: const Duration(hours: 7)),
    _position(
      501020,
      login,
      'EURUSD-20261016-1.1000-C',
      'buy',
      5,
      0.0012,
      0.0017,
      25,
      option: {
        'series': 'EURUSD-20261016-1.1000-C',
        'underlying': 'EURUSD',
        'right': 'call',
        'strike': 1.1,
        'expiry': '2026-10-16',
        'contractSize': 10000,
        'quoteCurrency': 'USD',
      },
      ago: const Duration(hours: 3),
    ),
  ],
  10051123 => [_position(600311, login, 'GBPUSD', 'sell', 0.3, 1.33912, 1.34685, -2319.8, sl: 1.3550)],
  20017734 => [
    _position(700101, login, 'BTCUSD', 'buy', 0.1, 64120.5, 67023, 290.25, ago: const Duration(days: 2)),
    _position(700144, login, 'USDJPY', 'buy', 0.5, 149.214, 149.319, 52.5, tp: 150.5),
  ],
  _ => const [],
};

List<Map<String, dynamic>> _orders(int login) => login == 10042817
    ? [
        {
          'ticket': 501101,
          'symbol': 'GBPJPY',
          'side': 'buy',
          'type': 'limit',
          'volume': 0.4,
          'price': 197.25,
          'stopLimit': null,
          'sl': 196.4,
          'tp': 199,
          'expiry': 'gtc',
          'placedAt': _ago(const Duration(hours: 2)),
        },
      ]
    : const [];

Map<String, dynamic> _deal(
  int id,
  int login,
  String symbol,
  String side,
  String entry,
  double volume,
  double price,
  double profit,
  String reason,
  Duration ago,
) => {
  'id': id,
  'login': login,
  'positionTicket': id - 7,
  'orderTicket': id - 3,
  'symbol': symbol,
  'side': side,
  'positionSide': entry == 'in' ? side : (side == 'buy' ? 'sell' : 'buy'),
  'entry': entry,
  'volume': volume,
  'price': price,
  'profit': profit,
  'swap': entry == 'in' ? 0 : -1.2,
  'commission': entry == 'in' ? 3.5 : 0,
  'reason': reason,
  'time': _ago(ago),
  'openPrice': null,
  'openTime': null,
  'source': 'manual',
  'comment': '',
  'instrument': 'cfd',
};

Map<String, dynamic> _recentDeals(int login) {
  final deals = login == 10038890 || _opened.any((a) => a['login'] == login)
      ? <Map<String, dynamic>>[]
      : [
          _deal(910455, login, 'EURUSD', 'sell', 'out', 0.5, 1.08392, 120.5, 'client', const Duration(hours: 3)),
          _deal(910402, login, 'XAUUSD', 'sell', 'out', 0.2, 2655.4, -45.2, 'sl', const Duration(hours: 9)),
          _deal(910388, login, 'GBPUSD', 'buy', 'out', 0.3, 1.3421, 96, 'tp', const Duration(days: 1)),
          _deal(910350, login, 'EURUSD', 'buy', 'in', 1, 1.08143, 0, 'client', const Duration(days: 1, hours: 2)),
          _deal(910301, login, 'USDJPY', 'sell', 'out', 0.5, 149.02, 38.75, 'client', const Duration(days: 2)),
          _deal(910298, login, 'XAUUSD', 'buy', 'in', 0.2, 2652.1, 0, 'client', const Duration(days: 2, hours: 3)),
        ];
  return {
    'deals': deals,
    'orders': <Object>[],
    'page': 1,
    'limit': 6,
    'total': deals.isEmpty ? 0 : 48,
    'totals': {'profit': 210.05, 'swap': -3.6, 'commission': 10.5},
  };
}

Map<String, dynamic> _health(int login) => {
  'login': login,
  'score': login == 10051123 ? 41 : 82,
  'lastTradeAt': _ago(const Duration(hours: 3)),
  'items': login == 10051123
      ? [
          {
            'key': 'margin_level',
            'status': 'warn',
            'value': {'level': 2099, 'marginCall': 100},
          },
          {
            'key': 'stop_loss',
            'status': 'good',
            'value': {'withSl': 1, 'positions': 1},
          },
          {
            'key': 'margin_use',
            'status': 'warn',
            'value': {'pct': 4.8},
          },
          {
            'key': 'floating',
            'status': 'bad',
            'value': {'pctOfBalance': 0.9},
          },
          {
            'key': 'results_30d',
            'status': 'warn',
            'value': {'trades': 12, 'winRate': 42},
          },
        ]
      : [
          {
            'key': 'margin_level',
            'status': 'good',
            'value': {'level': 700.7, 'marginCall': 100},
          },
          {
            'key': 'stop_loss',
            'status': 'warn',
            'value': {'withSl': 2, 'positions': 3},
          },
          {
            'key': 'margin_use',
            'status': 'good',
            'value': {'pct': 14.3},
          },
          {
            'key': 'floating',
            'status': 'good',
            'value': {'pctOfBalance': 3.3},
          },
          {
            'key': 'results_30d',
            'status': 'good',
            'value': {'trades': 48, 'winRate': 63},
          },
        ],
};

/* ------------------------------------------------------------------ groups */

Map<String, dynamic> _group(
  String code,
  String name, {
  String mode = 'hedging',
  bool cent = false,
  String types = 'both',
  required List<int> leverages,
  required int defaultLeverage,
  double marginCall = 50,
  double stopOut = 20,
  double minDeposit = 0,
  double commission = 0,
  int max = 5,
  bool swapFree = false,
  String product = 'cfd',
}) => {
  'code': code,
  'name': name,
  'product': product,
  'mode': mode,
  'cent': cent,
  'accountTypes': types,
  'leverages': leverages,
  'defaultLeverage': defaultLeverage,
  'marginCallPct': marginCall,
  'stopOutPct': stopOut,
  'hedgedMarginPct': 50,
  'minDeposit': minDeposit,
  'swapFree': swapFree,
  'commissionPerLot': commission,
  'spreadGroup': code,
  'maxAccountsPerUser': max,
  'demoInitialBalance': 10000,
  'demoRefillsPerDay': 3,
  'demoExpiryDays': 30,
  'enabled': true,
};

final List<Map<String, dynamic>> _groups = [
  _group('standard', 'Standard', leverages: [50, 100, 200, 500, 1000], defaultLeverage: 500),
  _group('pro', 'Pro', leverages: [50, 100, 200, 500], defaultLeverage: 200, marginCall: 100, stopOut: 50, minDeposit: 500, commission: 3.5, max: 3),
  _group('cent', 'Cent', cent: true, types: 'live', leverages: [100, 200, 500, 1000], defaultLeverage: 500, minDeposit: 10, max: 2),
  _group(
    'ecn',
    'ECN',
    mode: 'netting',
    types: 'live',
    leverages: [50, 100, 200],
    defaultLeverage: 100,
    marginCall: 100,
    stopOut: 50,
    minDeposit: 2000,
    commission: 3,
    max: 2,
  ),
  // the Options Standard group (leverage doesn't apply to options: one fixed value)
  _group('options-standard', 'Options Standard', leverages: [100], defaultLeverage: 100, marginCall: 100, stopOut: 50, product: 'options'),
];

/* ------------------------------------------------------------------ answers */

(int, Object)? previewAccounts(String method, String path, Map<String, dynamic> body, Map<String, String> query) {
  if (!path.startsWith('trading/')) return null;
  if (method != 'GET') previewAccountCalls.add((method: method, path: path, body: Map.of(body)));
  final rest = path.substring('trading/'.length).split('/');

  if (rest.length == 1 && rest[0] == 'groups' && method == 'GET') return (200, {'groups': _groups});
  if (rest.length == 1 && rest[0] == 'prefs') {
    if (method == 'POST') {
      final v = body['defaultLogin'];
      _defaultLogin = v is num ? v.toInt() : null;
    }
    return (200, {'ok': true, 'defaultLogin': _defaultLogin});
  }
  if (rest.length == 2 && rest[0] == 'transfers' && rest[1] == 'between' && method == 'POST') return _transferBetween(body);
  if (rest.isEmpty || rest[0] != 'accounts') return null;

  if (rest.length == 1) {
    if (method == 'GET') return (200, {'accounts': _accounts()});
    if (method == 'POST') return _open(body);
    return null;
  }
  final login = int.tryParse(rest[1]);
  if (login == null || rest[1].length != 8) return (404, _error('not_found', 'Not found.'));
  final a = _account(login);
  if (a == null) return (404, _error('not_found', 'Account not found.'));
  final live = a['type'] == 'live';

  if (rest.length == 2) {
    if (method == 'GET') return (200, {'account': a, 'positions': _positions(login), 'orders': _orders(login)});
    if (method == 'PATCH') {
      final name = '${body['name'] ?? ''}'.trim();
      if (name.runes.length > 32) return (422, _error('validation', 'Use up to 32 characters.'));
      _names[login] = name;
      return (200, {'account': _account(login)});
    }
    return null;
  }

  final action = rest.sublist(2).join('/');
  switch ((method, action)) {
    // the overview's recent deals only (the History tab's paging belongs to the Portfolio preview answers)
    case ('GET', 'history') when query['limit'] == '6':
      return (200, _recentDeals(login));
    case ('GET', 'health'):
      return (200, _health(login));
    case ('GET', 'export'):
      return (200, 'Time (UTC),Deal,Symbol,Side,Volume,Price,Profit\n2026-10-07 11:02:14,910455,EURUSD,sell,0.50,1.08392,120.50\n');
    case ('GET', 'history-zip'):
      return (200, 'PK');
    case ('POST', 'sso'):
      return (200, {'url': 'https://trade.kalkstrade.com/?sso=preview', 'expiresAt': _ago(const Duration(minutes: -1))});
    case ('POST', 'demo-refill'):
      if (live) return (409, _error('demo_only', 'Only demo accounts can be refilled.'));
      return (200, {'amount': 15000, 'balance': 25000});
    case ('POST', 'demo-balance'):
      final amount = body['amount'];
      if (amount is! num || amount < 100 || amount > 1000000) return (422, _error('validation', 'Choose an amount between 100 and 1,000,000.'));
      return (200, {'ok': true, 'balance': amount});
    case ('GET', 'group-options'):
      return (200, {'groups': _groupOptions(a)});
    case ('POST', 'group'):
      return (200, {'ok': true, 'group': body['group']});
    case ('POST', 'passwords'):
      if (body['stepup_token'] == null) return _stepupRequired();
      final pw = '${body['password'] ?? ''}';
      if (pw.length < 8 || pw.length > 64 || !RegExp(r'\d').hasMatch(pw) || !RegExp(r'[A-Za-z]').hasMatch(pw)) {
        return (422, _error('validation', 'Use 8 to 64 characters with letters and digits.'));
      }
      return (200, {'ok': true, 'sessionsRevoked': body['kind'] == 'trading' ? 1 : 0});
    case ('POST', 'leverage'):
      final lev = body['leverage'];
      if (lev is! int) return (422, _error('validation', 'Invalid leverage.'));
      if ((a['positions'] as int? ?? 0) > 0) return (409, _error('positions_open', 'Close all open positions before changing leverage.'));
      if (body['stepup_token'] == null) return _stepupRequired();
      final from = a['leverage'];
      _leverage[login] = lev;
      return (200, {'from': from, 'leverage': lev});
    case ('GET', 'archive-check'):
      return (200, _archiveCheck(a));
    case ('POST', 'archive'):
      final chk = _archiveCheck(a);
      if (chk['needsEmpty'] == true && body['empty'] != true) return (409, _error('needs_empty', 'Close the open trades and move the money out first.'));
      if (live && body['stepup_token'] == null) return _stepupRequired();
      _status[login] = 'archived';
      return (
        200,
        {
          'ok': true,
          'status': 'archived',
          'steps': [
            if ((chk['positions'] as int) + (chk['orders'] as int) > 0) {'step': 'close_positions', 'ok': true},
            if (live && (chk['balance'] as num) > 0) {'step': 'return_balance', 'ok': true, 'detail': '${chk['balance']} moved to your wallet'},
            {'step': 'archive', 'ok': true},
          ],
        },
      );
    case ('POST', 'restore'):
      _status[login] = 'active';
      return (200, {'ok': true, 'status': 'active'});
    case ('GET', 'closure'):
      return (200, _closureStatus(a));
    case ('POST', 'closure'):
      if (!live) return (409, _error('demo_account', 'Demo accounts are deleted (archived) instead of closed.'));
      if (!_reasons.contains(body['reasonCode'])) return (422, _error('validation', 'Choose a reason.'));
      if (body['stepup_token'] == null) return _stepupRequired();
      final req = {
        'id': 300 + login % 97,
        'status': 'pending',
        'reasonCode': body['reasonCode'],
        'createdAt': DateTime.now().toUtc().toIso8601String(),
        'decidedAt': null,
        'message': null,
        'source': 'client',
      };
      _closure[login] = req;
      return (
        200,
        {
          'ok': true,
          'steps': [
            if (_positions(login).isNotEmpty) {'step': 'close_positions', 'ok': true},
            if ((a['balance'] as num) > 0) {'step': 'return_balance', 'ok': true},
            {'step': 'request', 'ok': true},
          ],
          'request': req,
        },
      );
    case ('POST', 'closure/cancel'):
      _closure.remove(login);
      return (200, {'ok': true});
  }
  return null;
}

Map<String, dynamic> _archiveCheck(Map<String, dynamic> a) {
  final positions = a['positions'] as int? ?? 0, orders = a['orders'] as int? ?? 0;
  final balance = (a['balance'] as num?)?.toDouble() ?? 0;
  final live = a['type'] == 'live';
  final archived = a['status'] == 'archived' || a['status'] == 'closed';
  return {
    'login': a['login'],
    'kind': a['type'],
    'status': a['status'],
    'positions': positions,
    'orders': orders,
    'balance': balance,
    'credit': a['credit'] ?? 0,
    'bonus': a['bonus'] ?? 0,
    'canArchive': !archived,
    'needsEmpty': positions + orders > 0 || (live && balance > 0),
    'blockers': [
      if (archived) {'code': 'already_archived', 'message': 'This account is already archived.'},
    ],
  };
}

Map<String, dynamic> _closureStatus(Map<String, dynamic> a) {
  final login = a['login'] as int;
  final positions = a['positions'] as int? ?? 0, orders = a['orders'] as int? ?? 0;
  final balance = (a['balance'] as num?)?.toDouble() ?? 0;
  final req = _closure[login];
  return {
    'login': login,
    'kind': a['type'],
    'status': a['status'],
    'positions': positions,
    'orders': orders,
    'balance': balance,
    'credit': a['credit'] ?? 0,
    'bonus': a['bonus'] ?? 0,
    'needsEmpty': positions + orders > 0 || balance > 0,
    'canRequest': req?['status'] != 'pending',
    'blockers': [
      if (a['type'] == 'demo') {'code': 'demo_account', 'message': 'Demo accounts are deleted (archived) instead of closed.'},
    ],
    'surveyReasons': _reasons,
    'request': req,
  };
}

List<Map<String, dynamic>> _groupOptions(Map<String, dynamic> a) {
  final busy = (a['positions'] as int? ?? 0) + (a['orders'] as int? ?? 0) > 0;
  final cent = a['cent'] == true;
  return [
    for (final g in _groups)
      if (g['code'] != a['group'] &&
          (g['accountTypes'] == 'both' || g['accountTypes'] == a['type']) &&
          g['product'] == (a['product'] == 'options' ? 'options' : 'cfd'))
        () {
          Map<String, String>? blocker;
          if (g['cent'] != cent) {
            blocker = {'code': 'cent_mismatch', 'message': "Cent and standard accounts can't switch between each other."};
          } else if (busy) {
            blocker = {'code': 'positions_open', 'message': 'Close all trades and orders first.'};
          } else if ((g['minDeposit'] as num) > ((a['balance'] as num?) ?? 0)) {
            blocker = {'code': 'min_deposit', 'message': "The balance is below this type's minimum deposit."};
          }
          return {
            'code': g['code'],
            'name': g['name'],
            'mode': g['mode'],
            'cent': g['cent'],
            'minDeposit': g['minDeposit'],
            'leverages': g['leverages'],
            'commissionPerLot': g['commissionPerLot'],
            'swapFree': g['swapFree'],
            'allowed': blocker == null,
            'blocker': blocker,
          };
        }(),
  ];
}

(int, Object) _open(Map<String, dynamic> body) {
  final type = body['type'];
  if (type != 'live' && type != 'demo') return (422, _error('validation', 'Choose a live or demo account.'));
  final g = _groups.where((x) => x['code'] == body['group']).firstOrNull;
  if (g == null) return (422, _error('validation', 'Choose an account type.'));
  final pw = body['password'];
  if (pw is String && (pw.length < 8 || !RegExp(r'\d').hasMatch(pw))) {
    return (
      422,
      {
        'error': {'code': 'validation', 'message': 'Use 8 to 64 characters with letters and digits.', 'field': 'password'},
      },
    );
  }
  final demo = type == 'demo';
  final n = _opened.length + 1;
  final login = demo ? 20020000 + n : 10060000 + n;
  final balance = demo ? ((body['initialBalance'] as num?)?.toDouble() ?? 10000) * (g['cent'] == true ? 100 : 1) : 0.0;
  final account = <String, dynamic>{
    ...(shared.previewAccounts['accounts'] as List).first as Map<String, dynamic>,
    'login': login,
    'type': type,
    'group': g['code'],
    'groupName': g['name'],
    'product': g['product'],
    'mode': g['mode'],
    'cent': g['cent'],
    'currency': g['cent'] == true ? 'USC' : 'USD',
    'leverage': body['leverage'] ?? g['defaultLeverage'],
    'leverages': g['leverages'],
    'name': '${body['name'] ?? ''}',
    'marginCallLevel': g['marginCallPct'],
    'stopOutLevel': g['stopOutPct'],
    'positions': 0,
    'orders': 0,
    'balance': balance,
    'profit': 0,
    'equity': balance,
    'margin': 0,
    'freeMargin': balance,
    'marginLevel': null,
    'withdrawable': balance,
    'createdAt': DateTime.now().toUtc().toIso8601String(),
    if (demo) 'demo': {'initialBalance': balance, 'refillsPerDay': 3, 'refillsUsedToday': 0, 'expiryDays': 30},
  };
  _opened.add(account);
  // the preview trade server opens it in Kalks Trader too
  PreviewServer.instance.addAccount(
    login: login,
    type: '$type',
    group: '${g['code']}',
    groupName: '${g['name']}',
    balance: balance,
    cent: g['cent'] == true,
    leverage: (body['leverage'] as num?)?.toInt() ?? (g['defaultLeverage'] as int),
    product: '${g['product'] ?? 'cfd'}',
  );
  return (
    200,
    {
      'account': _withState(account),
      'credentials': {'login': login, if (pw is! String) 'password': 'Tq7mR4xV2pLs', 'investorPassword': 'Rd8nW3kZ5hJe'},
    },
  );
}

(int, Object) _transferBetween(Map<String, dynamic> body) {
  final from = body['fromLogin'], to = body['toLogin'];
  if (from is! int || to is! int) return (422, _error('validation', 'Choose both accounts.'));
  if (from == to) return (422, _error('validation', 'Choose two different accounts.'));
  final amount = '${body['amount'] ?? ''}';
  if (!RegExp(r'^\d{1,12}(\.\d{1,2})?$').hasMatch(amount) || double.parse(amount) <= 0) {
    return (422, _error('validation', 'Enter an amount with up to 2 decimals.'));
  }
  final key = body['idempotency_key'];
  if (key is! String || !RegExp(r'^[A-Za-z0-9_-]{8,64}$').hasMatch(key)) return (422, _error('validation', 'Missing request id.'));
  final src = _account(from);
  if (src == null || _account(to) == null) return (404, _error('not_found', 'Account not found.'));
  final usd = ((src['withdrawable'] as num?) ?? 0) / (src['currency'] == 'USC' ? 100 : 1);
  if (double.parse(amount) > usd + 1e-9) {
    return (422, _error('insufficient_funds', 'Not enough free funds on #$from: ${usd.toStringAsFixed(2)} USD available.'));
  }
  if (body['stepup_token'] == null) return _stepupRequired();
  return (200, {'status': 'completed', 'id': 'tt_${key.substring(0, 8)}', 'amount': amount, 'from_login': from, 'to_login': to});
}

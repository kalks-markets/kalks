// Kalks Trader in previews (`--dart-define=KALKS_PREVIEW=true`) and widget tests: a small in-memory trade server that
// answers the `trade/*` calls (lib/preview/preview_adapter.dart hands them over), and plays the market-data and engine
// WebSockets (fake channels given to MarketStream / EngineStream as their connector). Quotes walk randomly, orders
// fill, pending orders trigger, SL / TP close positions, and the stream pushes the same frames as the real engine, so
// every screen can be tried without a server. Sample values only; never used in a build that ships.
import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:web_socket_channel/web_socket_channel.dart';

import '../options/options_preview.dart';

class _Sym {
  _Sym(
    this.symbol,
    this.name,
    this.cls,
    this.digits,
    this.price,
    this.spread,
    this.change,
    this.contract, {
    this.core = true,
    this.live = true,
    this.session = 'fx',
    this.quote = 'USD',
    this.base,
  });
  final String symbol, name, cls;
  final int digits;
  double price;
  final double spread, change, contract;
  final bool core, live;
  final String session, quote;
  final String? base;
}

final List<_Sym> _symbols = [
  _Sym('EURUSD', 'Euro vs US Dollar', 'forex', 5, 1.08456, 0.00008, 0.21, 100000, base: 'EUR'),
  _Sym('GBPUSD', 'Pound vs US Dollar', 'forex', 5, 1.27881, 0.0001, -0.14, 100000, base: 'GBP'),
  _Sym('USDJPY', 'US Dollar vs Yen', 'forex', 3, 149.382, 0.012, 0.36, 100000, quote: 'JPY', base: 'USD'),
  _Sym('AUDUSD', 'Aussie vs US Dollar', 'forex', 5, 0.66214, 0.0001, 0.52, 100000, base: 'AUD'),
  _Sym('USDCAD', 'US Dollar vs Loonie', 'forex', 5, 1.35722, 0.00012, -0.08, 100000, quote: 'CAD', base: 'USD'),
  _Sym('USDCHF', 'US Dollar vs Franc', 'forex', 5, 0.84917, 0.00012, -0.22, 100000, quote: 'CHF', base: 'USD'),
  _Sym('GBPJPY', 'Pound vs Yen', 'forex', 3, 191.024, 0.02, 1.12, 100000, quote: 'JPY', base: 'GBP'),
  _Sym('EURJPY', 'Euro vs Yen', 'forex', 3, 162.011, 0.018, 0.58, 100000, quote: 'JPY', base: 'EUR'),
  _Sym('USDINR', 'US Dollar vs Rupee', 'forex', 4, 83.5125, 0.0035, 0.04, 100000, quote: 'INR', base: 'USD'),
  _Sym('XAUUSD', 'Gold vs US Dollar', 'metals', 2, 2654.3, 0.18, 0.84, 100),
  _Sym('XAGUSD', 'Silver vs US Dollar', 'metals', 3, 31.184, 0.02, 1.46, 5000),
  _Sym('US30', 'Dow Jones 30', 'indices', 1, 42318.5, 1.8, 0.31, 1),
  _Sym('NAS100', 'Nasdaq 100', 'indices', 1, 20118.4, 1.2, 1.24, 1),
  _Sym('SPX500', 'S&P 500', 'indices', 1, 5762.8, 0.5, 0.44, 1),
  _Sym('GER40', 'Germany 40', 'indices', 1, 18994.2, 1.4, -0.27, 1, quote: 'EUR'),
  _Sym('UK100', 'FTSE 100', 'indices', 1, 8321.6, 1.1, -0.12, 1, quote: 'GBP'),
  _Sym('JP225', 'Nikkei 225', 'indices', 0, 38742, 8, 0.92, 1, quote: 'JPY'),
  _Sym('USOIL', 'WTI Crude Oil', 'energies', 2, 71.84, 0.03, -1.38, 1000),
  _Sym('UKOIL', 'Brent Crude Oil', 'energies', 2, 75.12, 0.03, -1.11, 1000),
  _Sym('BTCUSD', 'Bitcoin', 'crypto', 2, 63412, 18, 2.84, 1, session: '24x7', base: 'BTC'),
  _Sym('ETHUSD', 'Ethereum', 'crypto', 2, 2618.44, 1.6, 1.92, 1, session: '24x7', base: 'ETH'),
  _Sym('SOLUSD', 'Solana', 'crypto', 3, 148.212, 0.12, 4.61, 1, session: '24x7', base: 'SOL'),
  _Sym('XRPUSD', 'Ripple', 'crypto', 4, 0.5874, 0.0012, -2.14, 1, session: '24x7', base: 'XRP'),
  _Sym('AAPL', 'Apple Inc.', 'stocks', 2, 228.14, 0.06, 0.72, 1, session: 'us_equity'),
  _Sym('TSLA', 'Tesla Inc.', 'stocks', 2, 254.28, 0.08, -3.18, 1, session: 'us_equity'),
  _Sym('NVDA', 'NVIDIA Corp.', 'stocks', 2, 121.44, 0.05, 2.26, 1, session: 'us_equity'),
  _Sym('META', 'Meta Platforms', 'stocks', 2, 568.31, 0.12, 1.08, 1, session: 'us_equity'),
  _Sym('NFLX', 'Netflix Inc.', 'stocks', 2, 709.52, 0.2, -0.64, 1, session: 'us_equity'),
  // catalogue (a sample of the 1,300+): some live, some demo only
  _Sym('NZDUSD', 'Kiwi vs US Dollar', 'forex', 5, 0.60812, 0.00014, 0.33, 100000, core: false, base: 'NZD'),
  _Sym('EURGBP', 'Euro vs Pound', 'forex', 5, 0.84811, 0.00011, 0.05, 100000, core: false, quote: 'GBP', base: 'EUR'),
  _Sym('AUDJPY', 'Aussie vs Yen', 'forex', 3, 98.912, 0.02, 0.71, 100000, core: false, quote: 'JPY', base: 'AUD'),
  _Sym('EURCHF', 'Euro vs Franc', 'forex', 5, 0.92114, 0.00015, -0.11, 100000, core: false, quote: 'CHF', base: 'EUR'),
  _Sym('USDSGD', 'US Dollar vs Singapore Dollar', 'forex', 5, 1.30412, 0.0002, 0.02, 100000, core: false, live: false, quote: 'SGD', base: 'USD'),
  _Sym('USDZAR', 'US Dollar vs Rand', 'forex', 4, 17.6211, 0.008, 0.44, 100000, core: false, live: false, quote: 'ZAR', base: 'USD'),
  _Sym('XPTUSD', 'Platinum vs US Dollar', 'metals', 2, 981.4, 1.2, 0.62, 100, core: false),
  _Sym('FRA40', 'France 40', 'indices', 1, 7612.4, 1.2, -0.31, 1, core: false, quote: 'EUR'),
  _Sym('AUS200', 'Australia 200', 'indices', 1, 8211.7, 1.4, 0.12, 1, core: false, live: false, quote: 'AUD'),
  _Sym('NGAS', 'Natural Gas', 'energies', 3, 2.684, 0.006, 2.4, 10000, core: false),
  _Sym('ADAUSD', 'Cardano', 'crypto', 4, 0.3512, 0.0008, 3.1, 1, core: false, session: '24x7', base: 'ADA'),
  _Sym('DOGEUSD', 'Dogecoin', 'crypto', 5, 0.10814, 0.0002, 5.2, 1, core: false, session: '24x7', base: 'DOGE'),
  _Sym('LTCUSD', 'Litecoin', 'crypto', 2, 66.42, 0.08, -0.8, 1, core: false, session: '24x7', base: 'LTC'),
  _Sym('BNBUSD', 'BNB', 'crypto', 2, 562.8, 0.6, 1.1, 1, core: false, live: false, session: '24x7', base: 'BNB'),
  _Sym('GOOGL', 'Alphabet Inc.', 'stocks', 2, 163.82, 0.05, 0.38, 1, core: false, live: false, session: 'us_equity'),
];

double _pip(_Sym s) => switch (s.cls) {
  'forex' => s.digits == 3 || s.digits == 2 ? 0.01 : 0.0001,
  'metals' => s.symbol.startsWith('XAU') ? 0.1 : 0.01,
  'indices' => 1,
  'energies' => 0.01,
  'crypto' => s.digits >= 4 ? 0.0001 : (s.digits == 3 ? 0.01 : 1),
  _ => 0.01,
};

double _round(double v, int d) => double.parse(v.toStringAsFixed(d));

class _Account {
  _Account(this.login, this.type, this.group, this.groupName, this.balance, {this.cent = false, this.leverage = 200, this.product = 'cfd'});
  final int login;
  final String type, group, groupName;
  final bool cent;
  final int leverage;

  /// cfd | options: an Options account refuses CFD orders (product_mismatch).
  final String product;

  /// Account currency (USC on cent accounts: x 100 USD).
  double balance;
  int refillsUsed = 0;
  final List<Map<String, dynamic>> positions = [];
  final List<Map<String, dynamic>> orders = [];
  final List<Map<String, dynamic>> deals = [];
  bool readOnly = false;
}

class PreviewServer {
  PreviewServer._() {
    for (final s in _symbols) {
      _mid[s.symbol] = s.price;
      _open[s.symbol] = s.price / (1 + s.change / 100);
    }
    _seed();
  }

  static final PreviewServer instance = PreviewServer._();

  final math.Random _rng = math.Random(7);
  final Map<String, double> _mid = {};
  final Map<String, double> _open = {};
  final Map<int, _Account> _accounts = {};
  final Map<String, int> _tokens = {};
  final Set<_MarketChannel> _markets = {};
  final Map<int, Set<_EngineChannel>> _engines = {};
  int _ticket = 49435120;
  int _deal = 88120400;
  Timer? _timer;
  int _tick = 0;

  /* ---------------- data ---------------- */

  _Sym? _sym(String s) => _symbols.where((x) => x.symbol == s).firstOrNull;

  double _spread(String symbol, String group) {
    final s = _sym(symbol);
    if (s == null) return 0;
    return s.spread * (group == 'raw' || group == 'pro' ? 0.6 : 1);
  }

  ({double bid, double ask}) _quote(String symbol, [String group = 'standard']) {
    final s = _sym(symbol);
    final mid = _mid[symbol] ?? 0;
    if (s == null) return (bid: mid, ask: mid);
    final half = _spread(symbol, group) / 2;
    return (bid: _round(mid - half, s.digits), ask: _round(mid + half, s.digits));
  }

  double _usdPerQuote(_Sym s, double price) {
    if (s.quote == 'USD') return 1;
    if (s.symbol == 'USD${s.quote}') return 1 / price;
    final cross = _mid['USD${s.quote}'];
    if (cross != null && cross > 0) return 1 / cross;
    final inv = _mid['${s.quote}USD'];
    return inv != null && inv > 0 ? inv : 1;
  }

  void _seed() {
    final pro = _Account(10042817, 'live', 'pro', 'Pro', 12480.55);
    final cent = _Account(10051123, 'live', 'cent', 'Cent', 254300, cent: true);
    final demo = _Account(20017734, 'demo', 'standard', 'Standard', 10000, leverage: 500);
    // the Options account of the sample client (lib/preview/c1/preview_accounts.dart)
    final options = _Account(20019001, 'demo', 'options-standard', 'Options Standard', 10000, leverage: 100, product: 'options');
    for (final a in [pro, cent, demo, options]) {
      _accounts[a.login] = a;
    }
    final now = DateTime.now().toUtc();
    void pos(_Account a, String symbol, String side, double vol, double offsetPips, {double? sl, double? tp, int hours = 3}) {
      final s = _sym(symbol)!;
      final open = _round((_mid[symbol] ?? s.price) - (side == 'buy' ? 1 : -1) * offsetPips * _pip(s), s.digits);
      a.positions.add({
        'ticket': _ticket += 7,
        'login': a.login,
        'symbol': symbol,
        'side': side,
        'volume': vol,
        'openPrice': open,
        'openTime': now.subtract(Duration(hours: hours)).toIso8601String(),
        'sl': sl,
        'tp': tp,
        'trailingPoints': null,
        'swap': side == 'buy' ? -1.2 * vol * (a.cent ? 100 : 1) : 0.4,
        'commission': 0,
        'source': 'manual',
        'platform': 'Web',
      });
    }

    pos(pro, 'XAUUSD', 'buy', 0.5, 42, sl: 2630, tp: 2690);
    pos(pro, 'EURUSD', 'sell', 1.2, -8, tp: 1.0790);
    pos(pro, 'BTCUSD', 'buy', 0.1, 95, sl: 62200, hours: 20);
    pos(cent, 'GBPJPY', 'buy', 0.3, 18, hours: 40);
    pos(demo, 'NAS100', 'buy', 2, 35, tp: 20250);
    pos(demo, 'USDJPY', 'sell', 1, 12, sl: 150.2);
    pro.orders.add({
      'ticket': _ticket += 5,
      'login': pro.login,
      'symbol': 'XAUUSD',
      'side': 'buy',
      'type': 'limit',
      'volume': 0.3,
      'price': 2628.5,
      'stopLimit': null,
      'triggered': false,
      'sl': 2612,
      'tp': 2672,
      'trailingPoints': null,
      'expiry': 'GTC',
      'expiryAt': null,
      'oco': null,
      'source': 'manual',
      'placedAt': now.subtract(const Duration(hours: 2)).toIso8601String(),
    });
    demo.orders.add({
      'ticket': _ticket += 5,
      'login': demo.login,
      'symbol': 'GBPUSD',
      'side': 'sell',
      'type': 'stop',
      'volume': 1,
      'price': 1.2742,
      'stopLimit': null,
      'triggered': false,
      'sl': 1.2791,
      'tp': 1.2655,
      'trailingPoints': null,
      'expiry': 'Today',
      'expiryAt': null,
      'oco': null,
      'source': 'manual',
      'placedAt': now.subtract(const Duration(hours: 1)).toIso8601String(),
    });
    // closed trades
    final hist = [
      ('EURUSD', 'buy', 1.0, 1.08112, 1.08391, 27.9, 30),
      ('XAUUSD', 'sell', 0.2, 2662.4, 2648.1, 286.0, 50),
      ('NAS100', 'buy', 1.0, 20190.2, 20141.7, -48.5, 70),
      ('GBPUSD', 'sell', 0.5, 1.27712, 1.27801, -44.5, 90),
      ('BTCUSD', 'buy', 0.05, 61980, 63120, 57.0, 140),
      ('USDJPY', 'buy', 1.0, 148.812, 149.204, 262.4, 200),
      // older trades, so the History periods (week / month / 3 months / all) differ
      ('EURUSD', 'sell', 0.5, 1.09120, 1.09265, -72.5, 24 * 12),
      ('XAUUSD', 'buy', 0.1, 2588.4, 2611.9, 235.0, 24 * 45),
      ('BTCUSD', 'sell', 0.02, 64210, 63380, 16.6, 24 * 160),
    ];
    for (final a in [pro, cent, demo]) {
      for (var i = 0; i < hist.length; i++) {
        final h = hist[i];
        final t = now.subtract(Duration(hours: h.$7 + i));
        final k = a.cent ? 100 : 1;
        final ticket = _ticket += 3;
        a.deals.add({
          'id': _deal += 2,
          'login': a.login,
          'positionTicket': ticket,
          'orderTicket': null,
          'symbol': h.$1,
          'side': h.$2,
          'positionSide': h.$2,
          'entry': 'in',
          'volume': h.$3,
          'price': h.$4,
          'profit': 0,
          'swap': 0,
          'commission': 0,
          'reason': 'client',
          'time': t.subtract(const Duration(hours: 4)).toIso8601String(),
          'openPrice': h.$4,
          'openTime': t.subtract(const Duration(hours: 4)).toIso8601String(),
          'source': 'manual',
        });
        a.deals.add({
          'id': _deal += 2,
          'login': a.login,
          'positionTicket': ticket,
          'orderTicket': null,
          'symbol': h.$1,
          'side': h.$2 == 'buy' ? 'sell' : 'buy',
          'positionSide': h.$2,
          'entry': 'out',
          'volume': h.$3,
          'price': h.$5,
          'profit': h.$6 * k,
          'swap': -0.6 * k,
          'commission': 0,
          'reason': i == 1 ? 'tp' : (i == 2 ? 'sl' : 'client'),
          'time': t.toIso8601String(),
          'openPrice': h.$4,
          'openTime': t.subtract(const Duration(hours: 4)).toIso8601String(),
          'source': 'manual',
        });
      }
    }
  }

  Map<String, dynamic> _specJson(_Sym s) => {
    'symbol': s.symbol,
    'name': s.name,
    'assetClass': s.cls,
    'digits': s.digits,
    'point': 1 / math.pow(10, s.digits),
    'pipSize': _pip(s),
    'contractSize': s.contract,
    'profitCurrency': s.quote,
    'baseCurrency': s.base ?? s.symbol,
    'lotMin': 0.01,
    'lotMax': s.cls == 'crypto' ? 20 : 100,
    'lotStep': 0.01,
    'marginPct': s.cls == 'stocks' ? 2000 : (s.cls == 'crypto' ? 500 : 100),
    'maxLeverage': s.cls == 'forex' || s.cls == 'metals' ? 1000 : 100,
    'swapLong': s.cls == 'crypto' ? -20.0 : -7.2,
    'swapShort': s.cls == 'crypto' ? -20.0 : 1.4,
    'swapUnit': s.cls == 'crypto' || s.cls == 'stocks' ? 'percent_per_year' : 'points',
    'tripleSwapDay': s.cls == 'crypto' ? null : 'Wednesday',
    'session': s.session,
    'open': true,
    'stopsLevelPoints': 10,
    'core': s.core,
    'liveTrading': s.live,
    'liveOff': null,
  };

  /// `GET /v1/quotes?group=` of market data: every symbol's price with today's open / high / low.
  Map<String, dynamic> quotesSnapshot(String group) => {
    for (final s in _symbols)
      s.symbol: () {
        final q = _quote(s.symbol, group);
        final open = _open[s.symbol]!;
        return {
          'bid': q.bid,
          'ask': q.ask,
          'last': _mid[s.symbol],
          'o': open,
          'h': math.max(open, _mid[s.symbol]!) * 1.002,
          'l': math.min(open, _mid[s.symbol]!) * 0.998,
          't': DateTime.now().millisecondsSinceEpoch,
          'd': s.core ? 0 : 1,
        };
      }(),
  };

  /// Candle history ending at the current price (deterministic per symbol / timeframe).
  List<({int t, double o, double h, double l, double c, double v})> _bars(String symbol, String tf, int limit, int? to) {
    final s = _sym(symbol);
    if (s == null) return const [];
    const secs = {'M1': 60, 'M5': 300, 'M15': 900, 'M30': 1800, 'H1': 3600, 'H4': 14400, 'D1': 86400, 'W1': 604800, 'MN': 2592000};
    final step = secs[tf] ?? 3600;
    final nowSec = DateTime.now().millisecondsSinceEpoch ~/ 1000;
    final last = (to ?? nowSec) ~/ step * step;
    final n = limit.clamp(1, 600);
    final r = math.Random(symbol.codeUnits.fold<int>(0, (a, c) => a * 31 + c) ^ step);
    final vol = 0.0012 * math.pow(step / 60, 0.45) * (s.cls == 'crypto' ? 2.2 : (s.cls == 'forex' ? 0.5 : 1));
    final closes = List<double>.filled(n, 0);
    var c = _mid[symbol] ?? s.price;
    for (var i = n - 1; i >= 0; i--) {
      closes[i] = c;
      c = c * (1 - (r.nextDouble() - 0.5) * vol * 2 - 0.0001);
    }
    final out = <({int t, double o, double h, double l, double c, double v})>[];
    for (var i = 0; i < n; i++) {
      final cl = closes[i];
      final op = i == 0 ? cl * (1 + (r.nextDouble() - 0.5) * vol) : closes[i - 1];
      final hi = math.max(op, cl) * (1 + r.nextDouble() * vol * 0.6);
      final lo = math.min(op, cl) * (1 - r.nextDouble() * vol * 0.6);
      out.add((
        t: last - (n - 1 - i) * step,
        o: _round(op, s.digits),
        h: _round(hi, s.digits),
        l: _round(lo, s.digits),
        c: _round(cl, s.digits),
        v: (200 + r.nextInt(1800)).toDouble(),
      ));
    }
    return out;
  }

  /// An account opened in the preview (the Open account wizard), so Kalks Trader can open it.
  void addAccount({
    required int login,
    required String type,
    required String group,
    required String groupName,
    double balance = 0,
    bool cent = false,
    int leverage = 100,
    String product = 'cfd',
  }) => _accounts[login] ??= _Account(login, type, group, groupName, balance, cent: cent, leverage: leverage, product: product);

  /// `GET /v1/candles` of market data, as the chart reads it.
  List<T> candlesAs<T>(String symbol, String tf, T Function(int t, double o, double h, double l, double c, double v) make, {int limit = 1000, int? to}) => [
    for (final b in _bars(symbol, tf, limit, to)) make(b.t, b.o, b.h, b.l, b.c, b.v),
  ];

  /* ---------------- REST: the trade routes ---------------- */

  _Account? _byToken(String? token) {
    if (token == null) return null;
    final login = _tokens[token];
    return login == null ? null : _accounts[login];
  }

  String _mint(int login, {bool investor = false, String kind = 's'}) {
    final t = 'kt1.$kind.preview${login}x${_rng.nextInt(1 << 30).toRadixString(36).padLeft(8, '0')}.${'A' * 43}';
    _tokens[t] = login;
    if (investor) _accounts[login]!.readOnly = true;
    return t;
  }

  static Map<String, dynamic> _err(String code, String message) => {
    'error': {'code': code, 'message': message},
  };

  Map<String, dynamic> _accountJson(_Account a) {
    final m = _metrics(a);
    return {
      'login': a.login,
      'type': a.type,
      'group': a.group,
      'groupName': a.groupName,
      'product': a.product,
      'spreadGroup': _spreadGroup(a),
      'mode': 'hedging',
      'cent': a.cent,
      'currency': a.cent ? 'USC' : 'USD',
      'leverage': a.leverage,
      'leverages': [50, 100, 200, 500],
      'status': 'active',
      'name': a.login == 10042817 ? 'Main' : '',
      'marginCall': false,
      'marginCallLevel': 100,
      'stopOutLevel': 50,
      'controls': {'tradingDisabled': false, 'closeOnly': false, 'maxLot': null},
      'balance': a.balance,
      'credit': 0,
      'bonus': 0,
      'profit': m.profit,
      'swap': m.swap,
      'equity': m.equity,
      'margin': m.margin,
      'freeMargin': m.equity - m.margin,
      'marginLevel': m.margin > 0 ? m.equity / m.margin * 100 : null,
      'demo': a.type == 'demo' ? {'initialBalance': 10000, 'refillsPerDay': 3, 'refillsUsedToday': a.refillsUsed, 'expiryDays': 30} : null,
      'createdAt': '2026-03-04T10:00:00Z',
    };
  }

  String _spreadGroup(_Account a) => a.group == 'cent' || a.product == 'options' ? 'standard' : a.group;

  /// Floating numbers in the account currency.
  ({double profit, double swap, double equity, double margin, Map<int, double> each}) _metrics(_Account a) {
    final k = a.cent ? 100.0 : 1.0;
    var profit = 0.0, swap = 0.0, margin = 0.0;
    final each = <int, double>{};
    for (final p in a.positions) {
      final s = _sym(p['symbol'] as String);
      if (s == null) continue;
      final q = _quote(s.symbol, _spreadGroup(a));
      final buy = p['side'] == 'buy';
      final close = buy ? q.bid : q.ask;
      final diff = buy ? close - (p['openPrice'] as num) : (p['openPrice'] as num) - close;
      final vol = (p['volume'] as num).toDouble();
      final pr = diff * vol * s.contract * _usdPerQuote(s, close) * k;
      each[p['ticket'] as int] = pr;
      profit += pr;
      swap += (p['swap'] as num).toDouble();
      final spec = _specJson(s);
      final lev = math.min(a.leverage, spec['maxLeverage'] as int);
      margin += s.contract * close * _usdPerQuote(s, close) * (spec['marginPct'] as num) / 100 / lev * vol * k;
    }
    return (profit: profit, swap: swap, equity: a.balance + profit + swap, margin: margin, each: each);
  }

  /// `trade/history?from&to&page&limit`: the account's deals in the range, newest first, paged like the API.
  Map<String, dynamic> _history(_Account a, Map<String, String> query) {
    final from = DateTime.tryParse(query['from'] ?? '');
    final to = DateTime.tryParse(query['to'] ?? '');
    final page = (int.tryParse(query['page'] ?? '') ?? 1).clamp(1, 100000);
    final limit = (int.tryParse(query['limit'] ?? '') ?? 100).clamp(1, 500);
    final all = a.deals.reversed.where((d) {
      final t = DateTime.tryParse('${d['time']}');
      if (t == null) return true;
      return (from == null || !t.isBefore(from)) && (to == null || !t.isAfter(to));
    }).toList();
    final start = (page - 1) * limit;
    final deals = start >= all.length ? const <Map<String, dynamic>>[] : all.sublist(start, (start + limit).clamp(0, all.length));
    return {'deals': deals, 'orders': <Object>[], 'page': page, 'limit': limit, 'total': all.length};
  }

  Map<String, dynamic> _state(_Account a, {int historyLimit = 200}) => {
    'account': _accountJson(a),
    'positions': a.positions,
    'orders': a.orders,
    'history': {'deals': a.deals.reversed.take(historyLimit).toList()},
    'readOnly': a.readOnly,
    'restrictions': <String>[],
    'staff': null,
    'serverTime': DateTime.now().toUtc().toIso8601String(),
    'expiresAt': DateTime.now().add(const Duration(hours: 12)).toUtc().toIso8601String(),
  };

  /// Answers one `trade/*` call: (status, body), or null when the path isn't the trade server's.
  (int, Object)? answer(String method, String path, Map<String, String> query, Map<String, dynamic> body, String? token) {
    if (!path.startsWith('trade/')) return null;
    if (path.startsWith('trade/options') || path == 'trade/ai-trader') {
      return previewOptionsAnswer(method, path, query, body, token == null ? null : _tokens[token]?.toString());
    }
    final p = path.substring(6);
    switch (p) {
      case 'symbols':
        final list = _symbols.map(_specJson).toList();
        return (
          200,
          {
            'symbols': list,
            'live': [
              for (final s in _symbols)
                if (!s.core && s.live) s.symbol,
            ],
            'off': [
              for (final s in _symbols)
                if (!s.core && !s.live) s.symbol,
            ],
          },
        );
      case 'sessions':
        final login = int.tryParse('${body['login']}');
        final a = login == null ? null : _accounts[login];
        if (a == null) return (404, _err('not_found', 'Account not found.'));
        a.readOnly = false;
        final tok = _mint(a.login);
        return (
          200,
          {
            'token': tok,
            'expiresAt': DateTime.now().add(const Duration(hours: 12)).toUtc().toIso8601String(),
            'readOnly': false,
            'login': '${a.login}',
            'account': _accountJson(a),
          },
        );
      case 'sessions/check':
        final out = [
          for (final t in (body['tokens'] as List? ?? const []))
            () {
              final a = _byToken('$t');
              return a == null ? {'alive': false} : {'alive': true, 'login': '${a.login}', 'readOnly': a.readOnly, 'account': _accountJson(a)};
            }(),
        ];
        return (200, {'sessions': out});
      case 'login':
        final login = int.tryParse('${body['login']}');
        final a = login == null ? null : _accounts[login];
        final server = '${body['server'] ?? ''}';
        if (a == null) return (401, _err('invalid_credentials', 'Invalid account or password.'));
        if (server == 'Kalks-Prop') return (409, _err('wrong_server', 'Account $login is not on $server.'));
        final pw = '${body['password'] ?? ''}';
        if (pw == 'wrong') return (401, _err('invalid_credentials', 'Invalid account or password.'));
        final investor = pw.toLowerCase().startsWith('investor');
        final tok = _mint(a.login, investor: investor, kind: 'p');
        return (
          200,
          {
            'token': tok,
            'expiresAt': DateTime.now().add(const Duration(hours: 12)).toUtc().toIso8601String(),
            'readOnly': investor,
            'login': '${a.login}',
            'account': _accountJson(a),
          },
        );
      case 'notifications':
        return (200, {'items': <Object>[], 'unread': 0});
      case 'notifications/read':
        return (200, {'ok': true, 'unread': 0});
    }
    final a = _byToken(token);
    if (a == null) return (401, _err('trade_session_required', 'Open the account first.'));
    if (method != 'GET' && a.readOnly && p != 'stream-ticket' && p != 'logout') {
      return (403, _err('read_only', 'Trading is disabled with the investor password.'));
    }
    switch (p) {
      case 'state':
        return (200, _state(a, historyLimit: int.tryParse(query['historyLimit'] ?? '') ?? 200));
      case 'history':
        return (200, _history(a, query));
      case 'controls':
        return (200, {'tradingDisabled': false, 'closeOnly': false, 'maxLot': null});
      case 'stream-ticket':
        return (200, {'ticket': 'preview-${a.login}', 'expiresIn': 30, 'url': 'wss://preview.local/engine/stream'});
      case 'logout':
        _tokens.remove(token);
        return (200, {'status': 'ok'});
      case 'demo-refill':
        if (a.type != 'demo') return (403, _err('demo_account', 'Demo accounts only'));
        if (a.refillsUsed >= 3) return (409, _err('refill_limit', 'No refills left today'));
        a.refillsUsed++;
        a.balance = 10000;
        _pushAccount(a);
        return (200, {'status': 'ok', 'amount': 10000, 'balance': a.balance});
      case 'orders':
        return _placeOrder(a, body);
      case 'bulk-close':
        return _bulk(a, '${body['filter'] ?? 'all'}');
      case 'positions/close-by':
        return _closeBy(a, int.tryParse('${body['ticket']}') ?? 0, int.tryParse('${body['by']}') ?? 0);
    }
    final pos = RegExp(r'^positions/(\d+)(/close)?$').firstMatch(p);
    if (pos != null) {
      final ticket = int.parse(pos[1]!);
      if (pos[2] != null) return _close(a, ticket, body['volume'] is num ? (body['volume'] as num).toDouble() : null, 'client');
      if (method == 'PATCH') return _modifyPosition(a, ticket, body);
    }
    final ord = RegExp(r'^orders/(\d+)$').firstMatch(p);
    if (ord != null) {
      final ticket = int.parse(ord[1]!);
      final o = a.orders.where((x) => x['ticket'] == ticket).firstOrNull;
      if (o == null) return (404, _err('not_found', 'Order not found.'));
      if (method == 'DELETE') {
        a.orders.remove(o);
        _push(a, {'type': 'order', 'op': 'remove', 'ticket': ticket, 'status': 'cancelled'});
        return (200, {'status': 'cancelled', 'ticket': ticket});
      }
      for (final k in ['price', 'sl', 'tp', 'stopLimit', 'volume']) {
        if (body.containsKey(k)) o[k] = body[k];
      }
      _push(a, {'type': 'order', 'op': 'upsert', 'order': o});
      return (200, {'order': o});
    }
    return (404, _err('not_found', 'Not in the preview trade server: $method $path'));
  }

  (int, Object) _placeOrder(_Account a, Map<String, dynamic> b) {
    // the engine: an Options account trades options only
    if (a.product == 'options') return (422, _err('product_mismatch', 'This is an Options account: CFDs trade in a CFD account.'));
    final symbol = '${b['symbol']}';
    final s = _sym(symbol);
    if (s == null) return (404, _err('not_found', 'Unknown symbol.'));
    if (a.type == 'live' && !s.core && !s.live) return (403, _err('symbol_demo_only', 'Live trading for $symbol is not enabled yet.'));
    final vol = (b['volume'] as num?)?.toDouble() ?? 0;
    if (vol < 0.01) return (422, _err('invalid_volume', 'Invalid volume.'));
    final side = '${b['side']}';
    final type = '${b['type']}';
    final q = _quote(symbol, _spreadGroup(a));
    if (type == 'market') {
      final price = side == 'buy' ? q.ask : q.bid;
      final sl = (b['sl'] as num?)?.toDouble();
      final tp = (b['tp'] as num?)?.toDouble();
      if (sl != null && (side == 'buy' ? sl >= price : sl <= price)) {
        return (422, _err('invalid_sl', 'Stop loss must be ${side == 'buy' ? 'below' : 'above'} ${price.toStringAsFixed(s.digits)}.'));
      }
      if (tp != null && (side == 'buy' ? tp <= price : tp >= price)) {
        return (422, _err('invalid_tp', 'Take profit must be ${side == 'buy' ? 'above' : 'below'} ${price.toStringAsFixed(s.digits)}.'));
      }
      final m = _metrics(a);
      final spec = _specJson(s);
      final need =
          s.contract *
          price *
          _usdPerQuote(s, price) *
          (spec['marginPct'] as num) /
          100 /
          math.min(a.leverage, spec['maxLeverage'] as int) *
          vol *
          (a.cent ? 100 : 1);
      if (need > m.equity - m.margin) return (422, _err('no_money', 'Not enough money.'));
      final p = _openPosition(a, symbol, side, vol, price, sl: sl, tp: tp, trailing: b['trailingPoints'] as int?, comment: b['comment'] as String?);
      return (
        200,
        {
          'status': 'filled',
          'orderTicket': (p['ticket'] as int) - 1,
          'positionTicket': p['ticket'],
          'price': price,
          'deals': [_deal],
          'delayMs': 38,
        },
      );
    }
    final price = (b['price'] as num?)?.toDouble();
    if (price == null || price <= 0) return (422, _err('invalid_price', 'Enter a price for the pending order.'));
    final o = {
      'ticket': _ticket += 3,
      'login': a.login,
      'symbol': symbol,
      'side': side,
      'type': type,
      'volume': vol,
      'price': price,
      'stopLimit': b['stopLimit'],
      'triggered': false,
      'sl': b['sl'],
      'tp': b['tp'],
      'trailingPoints': b['trailingPoints'],
      'expiry': b['expiry'] ?? 'GTC',
      'expiryAt': null,
      'oco': b['ocoWith'],
      'source': b['source'] ?? 'manual',
      'comment': b['comment'],
      'placedAt': DateTime.now().toUtc().toIso8601String(),
    };
    if (b['ocoWith'] != null) {
      final twin = a.orders.where((x) => x['ticket'] == b['ocoWith']).firstOrNull;
      if (twin != null) {
        twin['oco'] = o['ticket'];
        _push(a, {'type': 'order', 'op': 'upsert', 'order': twin});
      }
    }
    a.orders.add(o);
    _push(a, {'type': 'order', 'op': 'upsert', 'order': o});
    return (200, {'status': 'placed', 'ticket': o['ticket'], 'price': price});
  }

  Map<String, dynamic> _openPosition(
    _Account a,
    String symbol,
    String side,
    double vol,
    double price, {
    double? sl,
    double? tp,
    int? trailing,
    String? comment,
  }) {
    final ticket = _ticket += 4;
    final now = DateTime.now().toUtc().toIso8601String();
    final p = {
      'ticket': ticket,
      'login': a.login,
      'symbol': symbol,
      'side': side,
      'volume': vol,
      'openPrice': price,
      'openTime': now,
      'sl': sl,
      'tp': tp,
      'trailingPoints': trailing,
      'swap': 0,
      'commission': 0,
      'source': 'manual',
      'platform': 'Android',
      'comment': comment,
    };
    a.positions.add(p);
    final deal = {
      'id': _deal += 2,
      'login': a.login,
      'positionTicket': ticket,
      'orderTicket': ticket - 1,
      'symbol': symbol,
      'side': side,
      'positionSide': side,
      'entry': 'in',
      'volume': vol,
      'price': price,
      'profit': 0,
      'swap': 0,
      'commission': 0,
      'reason': 'client',
      'time': now,
      'openPrice': price,
      'openTime': now,
      'source': 'manual',
    };
    a.deals.add(deal);
    _push(a, {'type': 'position', 'op': 'upsert', 'position': p});
    _push(a, {'type': 'deal', 'deal': deal});
    return p;
  }

  (int, Object) _close(_Account a, int ticket, double? volume, String reason) {
    final p = a.positions.where((x) => x['ticket'] == ticket).firstOrNull;
    if (p == null) return (404, _err('not_found', 'Position not found.'));
    final s = _sym(p['symbol'] as String)!;
    final q = _quote(s.symbol, _spreadGroup(a));
    final buy = p['side'] == 'buy';
    final close = buy ? q.bid : q.ask;
    final full = (p['volume'] as num).toDouble();
    final vol = volume == null ? full : math.min(full, volume);
    final diff = buy ? close - (p['openPrice'] as num) : (p['openPrice'] as num) - close;
    final profit = double.parse((diff * vol * s.contract * _usdPerQuote(s, close) * (a.cent ? 100 : 1)).toStringAsFixed(2));
    final swap = (p['swap'] as num).toDouble() * vol / full;
    a.balance += profit + swap;
    final now = DateTime.now().toUtc().toIso8601String();
    final deal = {
      'id': _deal += 2,
      'login': a.login,
      'positionTicket': ticket,
      'orderTicket': null,
      'symbol': s.symbol,
      'side': buy ? 'sell' : 'buy',
      'positionSide': p['side'],
      'entry': 'out',
      'volume': vol,
      'price': close,
      'profit': profit,
      'swap': swap,
      'commission': 0,
      'reason': reason,
      'time': now,
      'openPrice': p['openPrice'],
      'openTime': p['openTime'],
      'source': 'manual',
    };
    a.deals.add(deal);
    _push(a, {'type': 'deal', 'deal': deal});
    if (vol >= full - 1e-9) {
      a.positions.remove(p);
      _push(a, {'type': 'position', 'op': 'remove', 'ticket': ticket});
    } else {
      p['volume'] = double.parse((full - vol).toStringAsFixed(2));
      p['swap'] = (p['swap'] as num) - swap;
      _push(a, {'type': 'position', 'op': 'upsert', 'position': p});
    }
    _pushAccount(a);
    return (200, {'status': 'closed', 'dealId': deal['id'], 'profit': profit});
  }

  (int, Object) _modifyPosition(_Account a, int ticket, Map<String, dynamic> b) {
    final p = a.positions.where((x) => x['ticket'] == ticket).firstOrNull;
    if (p == null) return (404, _err('not_found', 'Position not found.'));
    final q = _quote(p['symbol'] as String, _spreadGroup(a));
    final buy = p['side'] == 'buy';
    final cur = buy ? q.bid : q.ask;
    if (b.containsKey('sl') && b['sl'] is num && (buy ? (b['sl'] as num) >= cur : (b['sl'] as num) <= cur)) {
      return (422, _err('invalid_sl', 'Stop loss must be ${buy ? 'below' : 'above'} $cur.'));
    }
    if (b.containsKey('tp') && b['tp'] is num && (buy ? (b['tp'] as num) <= cur : (b['tp'] as num) >= cur)) {
      return (422, _err('invalid_tp', 'Take profit must be ${buy ? 'above' : 'below'} $cur.'));
    }
    for (final k in ['sl', 'tp', 'trailingPoints']) {
      if (b.containsKey(k)) p[k] = b[k];
    }
    _push(a, {'type': 'position', 'op': 'upsert', 'position': p});
    return (200, {'position': p});
  }

  (int, Object) _closeBy(_Account a, int t1, int t2) {
    final p1 = a.positions.where((x) => x['ticket'] == t1).firstOrNull;
    final p2 = a.positions.where((x) => x['ticket'] == t2).firstOrNull;
    if (p1 == null || p2 == null || p1['symbol'] != p2['symbol'] || p1['side'] == p2['side']) return (422, _err('invalid_close_by', 'Invalid Close By'));
    final vol = math.min((p1['volume'] as num).toDouble(), (p2['volume'] as num).toDouble());
    _close(a, t1, vol, 'close_by');
    _close(a, t2, vol, 'close_by');
    return (
      200,
      {
        'status': 'closed',
        'deals': [_deal - 2, _deal],
      },
    );
  }

  (int, Object) _bulk(_Account a, String filter) {
    if (filter == 'pending') {
      final done = [for (final o in a.orders) o['ticket']];
      for (final t in done) {
        _push(a, {'type': 'order', 'op': 'remove', 'ticket': t, 'status': 'cancelled'});
      }
      a.orders.clear();
      return (200, {'done': done, 'failed': <Object>[], 'profit': 0});
    }
    final m = _metrics(a);
    final pick = a.positions.where((p) {
      final pr = m.each[p['ticket']] ?? 0;
      return switch (filter) {
        'profitable' => pr > 0,
        'losing' => pr < 0,
        'buys' => p['side'] == 'buy',
        'sells' => p['side'] == 'sell',
        _ => true,
      };
    }).toList();
    var profit = 0.0;
    final done = <Object>[];
    for (final p in pick) {
      final r = _close(a, p['ticket'] as int, null, 'client');
      if (r.$1 == 200) {
        profit += ((r.$2 as Map)['profit'] as num).toDouble();
        done.add(p['ticket'] as Object);
      }
    }
    return (200, {'done': done, 'failed': <Object>[], 'profit': profit});
  }

  /* ---------------- the market: ticks, triggers ---------------- */

  /// The market clock runs while a market-data or engine socket is open (the terminal is on screen) and stops with
  /// the last one, so tests and closed terminals leave no timer behind.
  void _ensureTimer() {
    _timer ??= Timer.periodic(const Duration(milliseconds: 450), (_) => _step());
  }

  void _maybeStop() {
    if (_markets.isEmpty && _engines.values.every((s) => s.isEmpty)) stop();
  }

  /// Stops the market clock now (tests); the next socket starts it again.
  void stop() {
    _timer?.cancel();
    _timer = null;
  }

  void _step() {
    _tick++;
    final active = <String>{for (final m in _markets) ...m.active};
    for (final s in _symbols) {
      if (!active.contains(s.symbol) && _tick % 6 != 0) continue;
      final vol = s.cls == 'crypto' ? 0.00025 : (s.cls == 'forex' ? 0.00004 : 0.0001);
      final mid = _mid[s.symbol]!;
      _mid[s.symbol] = mid * (1 + (_rng.nextDouble() - 0.5) * 2 * vol);
    }
    for (final m in _markets) {
      m.tick(this);
    }
    for (final a in _accounts.values) {
      _triggers(a);
    }
    if (_tick % 2 == 0) {
      for (final e in _engines.entries) {
        final a = _accounts[e.key];
        if (a == null || e.value.isEmpty) continue;
        _push(a, _equity(a));
      }
    }
  }

  void _triggers(_Account a) {
    final g = _spreadGroup(a);
    for (final o in [...a.orders]) {
      final q = _quote(o['symbol'] as String, g);
      final buy = o['side'] == 'buy';
      final price = (o['price'] as num).toDouble();
      final hit = switch (o['type']) {
        'limit' => buy ? q.ask <= price : q.bid >= price,
        'stop' || 'stop_limit' => buy ? q.ask >= price : q.bid <= price,
        _ => false,
      };
      if (!hit) continue;
      a.orders.remove(o);
      _push(a, {'type': 'order', 'op': 'remove', 'ticket': o['ticket'], 'status': 'filled'});
      final fill = buy ? q.ask : q.bid;
      _openPosition(
        a,
        o['symbol'] as String,
        o['side'] as String,
        (o['volume'] as num).toDouble(),
        fill,
        sl: (o['sl'] as num?)?.toDouble(),
        tp: (o['tp'] as num?)?.toDouble(),
      );
      _push(a, {
        'type': 'notification',
        'kind': 'order_filled',
        'message': '#${o['ticket']} ${o['side']} ${o['type']} ${o['volume']} ${o['symbol']} filled at $fill',
      });
      if (o['oco'] != null) {
        final twin = a.orders.where((x) => x['ticket'] == o['oco']).firstOrNull;
        if (twin != null) {
          a.orders.remove(twin);
          _push(a, {'type': 'order', 'op': 'remove', 'ticket': twin['ticket'], 'status': 'cancelled'});
        }
      }
    }
    for (final p in [...a.positions]) {
      final q = _quote(p['symbol'] as String, g);
      final buy = p['side'] == 'buy';
      final cur = buy ? q.bid : q.ask;
      final sl = (p['sl'] as num?)?.toDouble();
      final tp = (p['tp'] as num?)?.toDouble();
      final slHit = sl != null && (buy ? cur <= sl : cur >= sl);
      final tpHit = tp != null && (buy ? cur >= tp : cur <= tp);
      if (!slHit && !tpHit) continue;
      _close(a, p['ticket'] as int, null, slHit ? 'sl' : 'tp');
      _push(a, {'type': 'notification', 'kind': slHit ? 'sl' : 'tp', 'message': '#${p['ticket']} ${p['side']} ${p['volume']} ${p['symbol']} closed at $cur'});
    }
  }

  Map<String, dynamic> _equity(_Account a) {
    final m = _metrics(a);
    final g = _spreadGroup(a);
    return {
      'type': 'equity',
      'login': a.login,
      'balance': a.balance,
      'credit': 0,
      'bonus': 0,
      'profit': m.profit,
      'swap': m.swap,
      'equity': m.equity,
      'margin': m.margin,
      'freeMargin': m.equity - m.margin,
      'marginLevel': m.margin > 0 ? m.equity / m.margin * 100 : null,
      'positions': [
        for (final p in a.positions)
          {
            'ticket': p['ticket'],
            'price': p['side'] == 'buy' ? _quote(p['symbol'] as String, g).bid : _quote(p['symbol'] as String, g).ask,
            'profit': m.each[p['ticket']] ?? 0,
            'swap': p['swap'],
          },
      ],
    };
  }

  void _push(_Account a, Map<String, dynamic> frame) {
    for (final e in [...?_engines[a.login]]) {
      e.send(frame);
    }
  }

  void _pushAccount(_Account a) => _push(a, {'type': 'account', 'account': _accountJson(a)});

  /* ---------------- fake sockets ---------------- */

  /// MarketStream's connector in previews.
  WebSocketChannel marketConnector(Uri url) {
    _ensureTimer();
    final ch = _MarketChannel(url.queryParameters['group'] ?? 'standard');
    _markets.add(ch);
    ch.onClose = () {
      _markets.remove(ch);
      _maybeStop();
    };
    return ch;
  }

  /// EngineStream's connector in previews: the snapshot first, then live frames.
  WebSocketChannel engineConnector(Uri url) {
    _ensureTimer();
    final login = int.tryParse((url.queryParameters['ticket'] ?? '').replaceFirst('preview-', '')) ?? 0;
    final ch = _EngineChannel();
    final set = _engines[login] ??= {};
    set.add(ch);
    ch.onClose = () {
      set.remove(ch);
      _maybeStop();
    };
    final a = _accounts[login];
    if (a != null) {
      scheduleMicrotask(() {
        ch.send({'type': 'snapshot', 'readOnly': a.readOnly, 'account': _accountJson(a), 'positions': a.positions, 'orders': a.orders});
        ch.send(_equity(a));
      });
    }
    return ch;
  }

  /// Quote frame of one symbol for a group.
  Map<String, dynamic>? quoteFrame(String symbol, String group, {bool delayed = false}) {
    final s = _sym(symbol);
    if (s == null) return null;
    final q = _quote(symbol, group);
    return {
      'type': 'quote',
      's': symbol,
      'b': q.bid,
      'a': q.ask,
      'l': _round(_mid[symbol]!, s.digits),
      't': DateTime.now().millisecondsSinceEpoch,
      'd': delayed ? 1 : 0,
    };
  }

  Map<String, dynamic>? barFrame(String symbol, String tf) {
    const secs = {'M1': 60, 'M5': 300, 'M15': 900, 'M30': 1800, 'H1': 3600, 'H4': 14400, 'D1': 86400, 'W1': 604800, 'MN': 2592000};
    final s = _sym(symbol);
    if (s == null) return null;
    final step = secs[tf] ?? 3600;
    final now = DateTime.now().millisecondsSinceEpoch ~/ 1000 ~/ step * step;
    final bars = _bars(symbol, tf, 2, null);
    final last = bars.last;
    final c = _round(_mid[symbol]!, s.digits);
    return {'type': 'bar', 's': symbol, 'tf': tf, 't': now, 'o': last.o, 'h': math.max(last.h, c), 'l': math.min(last.l, c), 'c': c, 'v': last.v};
  }

  Map<String, dynamic>? depthFrame(String symbol, String group) {
    final s = _sym(symbol);
    if (s == null) return null;
    final q = _quote(symbol, group);
    final step = _pip(s) / 2;
    final r = math.Random(_tick);
    return {
      'type': 'depth',
      's': symbol,
      'src': 'indicative',
      't': DateTime.now().millisecondsSinceEpoch,
      'b': [
        for (var i = 0; i < 10; i++) [_round(q.bid - i * step, s.digits), _round(0.5 + r.nextDouble() * 6, 2)],
      ],
      'a': [
        for (var i = 0; i < 10; i++) [_round(q.ask + i * step, s.digits), _round(0.5 + r.nextDouble() * 6, 2)],
      ],
    };
  }
}

/// A WebSocket played by the preview server.
abstract class _FakeChannel implements WebSocketChannel {
  _FakeChannel() {
    sink = _FakeSink(this);
  }

  // closed by close() (the sink's close or the socket under test)
  // ignore: close_sinks
  final StreamController<Object?> _in = StreamController<Object?>();
  void Function()? onClose;
  bool _closed = false;

  @override
  // closed by the socket that uses this channel
  // ignore: close_sinks
  late final WebSocketSink sink;

  @override
  Stream<Object?> get stream => _in.stream;

  @override
  Future<void> get ready => Future<void>.value();

  @override
  String? get protocol => null;

  @override
  int? get closeCode => null;

  @override
  String? get closeReason => null;

  void send(Map<String, dynamic> frame) {
    if (!_closed) _in.add(jsonEncode(frame));
  }

  void received(Map<String, dynamic> m);

  void close() {
    if (_closed) return;
    _closed = true;
    onClose?.call();
    unawaited(_in.close());
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _FakeSink implements WebSocketSink {
  _FakeSink(this.ch);
  final _FakeChannel ch;

  @override
  void add(Object? data) {
    try {
      final m = jsonDecode('$data');
      if (m is Map) ch.received(m.cast<String, dynamic>());
    } catch (_) {}
  }

  @override
  void addError(Object error, [StackTrace? stackTrace]) {}

  @override
  Future<void> addStream(Stream<Object?> stream) async {}

  @override
  Future<void> close([int? closeCode, String? closeReason]) async => ch.close();

  @override
  Future<void> get done => Future<void>.value();
}

class _MarketChannel extends _FakeChannel {
  _MarketChannel(this.group);
  final String group;
  final Set<String> active = {};
  final Set<String> passive = {};
  final Set<String> bars = {};
  final Set<String> depth = {};

  @override
  void received(Map<String, dynamic> m) {
    final symbols = [for (final s in (m['symbols'] as List? ?? const [])) '$s'];
    switch (m['op']) {
      case 'subscribe':
        if (m['passive'] == true) {
          active.removeAll(symbols);
          passive.addAll(symbols);
          // passive: one (delayed for the catalogue) snapshot each
          for (final s in symbols) {
            final cat = !_symbols.any((x) => x.symbol == s && x.core);
            final f = PreviewServer.instance.quoteFrame(s, group, delayed: cat);
            if (f != null) send(f);
          }
        } else {
          active.addAll(symbols);
        }
      case 'bars':
        bars.add('${m['symbol']}|${m['tf']}');
      case 'unbars':
        bars.remove('${m['symbol']}|${m['tf']}');
      case 'depth':
        depth.addAll(symbols);
      case 'undepth':
        depth.removeAll(symbols);
    }
  }

  void tick(PreviewServer s) {
    for (final sym in active) {
      final f = s.quoteFrame(sym, group);
      if (f != null) send(f);
    }
    for (final key in bars) {
      final p = key.split('|');
      final f = s.barFrame(p[0], p[1]);
      if (f != null) send(f);
    }
    for (final sym in depth) {
      final f = s.depthFrame(sym, group);
      if (f != null) send(f);
    }
    if (s._tick % 10 == 0) send({'type': 'hb', 't': DateTime.now().millisecondsSinceEpoch});
  }
}

class _EngineChannel extends _FakeChannel {
  @override
  void received(Map<String, dynamic> m) {}
}

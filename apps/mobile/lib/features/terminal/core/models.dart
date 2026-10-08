// Kalks Trader data shapes, parsed from the engine JSON the mobile API returns (docs/MOBILE-API.md §6; web:
// apps/terminal/lib/engine/types.ts + map.ts). Money convention (same as the web store): every amount is kept in USD;
// cent accounts report USC from the engine (x 100) and are divided by 100 here, the UI multiplies back (accMoney).
import 'package:flutter/foundation.dart';

double _d(Object? v, [double fallback = 0]) => v is num ? v.toDouble() : (double.tryParse('${v ?? ''}') ?? fallback);
double? _dn(Object? v) => v == null ? null : (v is num ? v.toDouble() : double.tryParse('$v'));
int _i(Object? v) => v is num ? v.toInt() : (int.tryParse('${v ?? ''}') ?? 0);
Map<String, dynamic> _map(Object? v) => v is Map ? v.cast<String, dynamic>() : const {};

/// Contract specification of one market (`GET trade/symbols`, the engine's /v1/symbols).
@immutable
class SymbolSpec {
  const SymbolSpec({
    required this.symbol,
    required this.name,
    required this.assetClass,
    required this.digits,
    required this.point,
    required this.pipSize,
    required this.contractSize,
    required this.profitCurrency,
    required this.baseCurrency,
    required this.lotMin,
    required this.lotMax,
    required this.lotStep,
    required this.marginPct,
    required this.maxLeverage,
    required this.swapLong,
    required this.swapShort,
    required this.swapUnit,
    required this.tripleSwapDay,
    required this.session,
    required this.open,
    required this.stopsLevelPoints,
    required this.core,
    required this.liveTrading,
  });

  final String symbol, name, assetClass;
  final int digits;
  final double point, pipSize, contractSize;

  /// Quote currency (P&L currency), e.g. JPY for USDJPY.
  final String profitCurrency;
  final String baseCurrency;
  final double lotMin, lotMax, lotStep;

  /// 100 = notional / leverage.
  final double marginPct;
  final int maxLeverage;
  final double swapLong, swapShort;

  /// `points` (per lot per night) or `percent_per_year` (of the position value).
  final String swapUnit;

  /// "Wednesday", or null when every night is charged alike (crypto).
  final String? tripleSwapDay;

  /// fx | 24x7 | us_equity | hk_equity | jp_equity
  final String session;

  /// The session was open when the list was fetched (holidays included).
  final bool open;
  final int stopsLevelPoints;

  /// The 28 core markets (always live-tradable); the rest is the catalogue.
  final bool core;
  final bool liveTrading;

  /// Live accounts and guests see core markets and live-enabled catalogue markets; demo accounts see everything.
  bool get liveTradable => core || liveTrading;

  static SymbolSpec fromJson(Map<String, dynamic> j) {
    final digits = _i(j['digits']);
    final point = _dn(j['point']) ?? _pow10(-digits);
    final cls = '${j['assetClass'] ?? 'forex'}';
    final symbol = '${j['symbol'] ?? ''}';
    return SymbolSpec(
      symbol: symbol,
      name: '${j['name'] ?? symbol}',
      assetClass: cls,
      digits: digits,
      point: point,
      pipSize: _dn(j['pipSize']) ?? defaultPip(cls, symbol, digits),
      contractSize: _d(j['contractSize'], 1),
      profitCurrency: '${j['profitCurrency'] ?? j['quoteCcy'] ?? 'USD'}',
      baseCurrency: '${j['baseCurrency'] ?? j['baseCcy'] ?? (cls == 'forex' && symbol.length >= 6 ? symbol.substring(0, 3) : symbol)}',
      lotMin: _d(j['lotMin'], 0.01),
      lotMax: _d(j['lotMax'], 100),
      lotStep: _d(j['lotStep'], 0.01),
      marginPct: _d(j['marginPct'], 100),
      maxLeverage: _i(j['maxLeverage']) == 0 ? 1000 : _i(j['maxLeverage']),
      swapLong: _d(j['swapLong']),
      swapShort: _d(j['swapShort']),
      swapUnit: j['swapUnit'] == 'percent_per_year' ? 'percent_per_year' : 'points',
      tripleSwapDay: j['tripleSwapDay'] is String && (j['tripleSwapDay'] as String).isNotEmpty ? j['tripleSwapDay'] as String : null,
      session: '${j['session'] ?? 'fx'}',
      open: j['open'] != false,
      stopsLevelPoints: _i(j['stopsLevelPoints']),
      core: j['core'] == true,
      liveTrading: j['liveTrading'] == true,
    );
  }

  /// Web pipSize(inst) for when the server sends none.
  static double defaultPip(String cls, String symbol, int digits) => switch (cls) {
    'forex' => digits == 3 || digits == 2 ? 0.01 : 0.0001,
    'metals' => symbol.startsWith('XAU') ? 0.1 : 0.01,
    'indices' => 1,
    'energies' => 0.01,
    'crypto' => digits >= 4 ? 0.0001 : (digits == 3 ? 0.01 : 1),
    _ => 0.01,
  };

  static double _pow10(int n) {
    var f = 1.0;
    for (var i = 0; i < n.abs(); i++) {
      f *= 10;
    }
    return n < 0 ? 1 / f : f;
  }
}

/// A trading account as the terminal shows it (engine account view, web mapAccount). Money in USD.
@immutable
class TAccount {
  const TAccount({
    required this.login,
    required this.type,
    required this.group,
    required this.groupCode,
    required this.spreadGroup,
    required this.mode,
    required this.cent,
    required this.currency,
    required this.leverage,
    required this.status,
    required this.name,
    required this.balance,
    required this.credit,
    required this.equity,
    required this.margin,
    required this.freeMargin,
    required this.profit,
    required this.swap,
    required this.marginLevel,
    required this.marginCallLevel,
    required this.stopOutLevel,
    required this.tradingDisabled,
    required this.closeOnly,
    required this.maxLot,
    required this.refillsLeft,
    required this.demoInitial,
    this.product = 'cfd',
  });

  final String login;

  /// live | demo
  final String type;

  /// Display name of the group ("Pro"), and its code ("pro").
  final String group, groupCode;

  /// The market-data `?group=` whose spread the quotes carry.
  final String spreadGroup;

  /// hedging | netting
  final String mode;
  final bool cent;
  final String currency;
  final int leverage;
  final String status, name;
  final double balance, credit, equity, margin, freeMargin, profit, swap;
  final double? marginLevel;
  final double marginCallLevel, stopOutLevel;
  final bool tradingDisabled, closeOnly;
  final double? maxLot;
  final int? refillsLeft;
  final double? demoInitial;

  /// cfd | options (the group's product; missing = cfd): an Options account opens the options workspace.
  final String product;

  bool get live => type == 'live';
  bool get isOptions => product == 'options';
  bool get demo => type == 'demo';
  bool get hedging => mode == 'hedging';

  /// "Kalks-Live" / "Kalks-Demo" (web serverName).
  String get server => demo ? 'Kalks-Demo' : 'Kalks-Live';

  /// USC on cent accounts, else the account currency.
  String get ccy => cent ? 'USC' : currency;

  static TAccount fromJson(Map<String, dynamic> a) {
    final cent = a['cent'] == true;
    final k = cent ? 100.0 : 1.0;
    final controls = _map(a['controls']);
    final demo = a['demo'] is Map ? _map(a['demo']) : null;
    return TAccount(
      login: '${a['login'] ?? ''}',
      type: a['type'] == 'demo' ? 'demo' : 'live',
      group: '${(a['groupName'] is String && (a['groupName'] as String).isNotEmpty) ? a['groupName'] : a['group'] ?? ''}',
      groupCode: '${a['group'] ?? ''}',
      spreadGroup: '${(a['spreadGroup'] is String && (a['spreadGroup'] as String).isNotEmpty) ? a['spreadGroup'] : a['group'] ?? 'standard'}',
      mode: a['mode'] == 'netting' ? 'netting' : 'hedging',
      cent: cent,
      currency: '${a['currency'] ?? (cent ? 'USC' : 'USD')}' == 'USC' ? 'USD' : '${a['currency'] ?? 'USD'}',
      leverage: _i(a['leverage']) == 0 ? 100 : _i(a['leverage']),
      status: '${a['status'] ?? 'active'}',
      name: '${a['name'] ?? ''}',
      balance: _d(a['balance']) / k,
      credit: (_d(a['credit']) + _d(a['bonus'])) / k,
      equity: _d(a['equity']) / k,
      margin: _d(a['margin']) / k,
      freeMargin: _d(a['freeMargin'], _d(a['equity']) - _d(a['margin'])) / k,
      profit: _d(a['profit']) / k,
      swap: _d(a['swap']) / k,
      marginLevel: _dn(a['marginLevel']),
      marginCallLevel: _d(a['marginCallLevel'], 100),
      stopOutLevel: _d(a['stopOutLevel'], 50),
      tradingDisabled: controls['tradingDisabled'] == true,
      closeOnly: controls['closeOnly'] == true || a['status'] == 'close_only',
      maxLot: _dn(controls['maxLot']),
      refillsLeft: demo == null ? null : (_i(demo['refillsPerDay']) - _i(demo['refillsUsedToday'])).clamp(0, 1 << 20),
      demoInitial: demo == null ? null : _dn(demo['initialBalance']),
      product: a['product'] == 'options' ? 'options' : 'cfd',
    );
  }
}

/// An open position (engine position, web mapPosition). Money in USD.
@immutable
class TPosition {
  const TPosition({
    required this.ticket,
    required this.symbol,
    required this.side,
    required this.volume,
    required this.openPrice,
    required this.openTime,
    required this.sl,
    required this.tp,
    required this.trailingPoints,
    required this.swap,
    required this.commission,
    required this.profit,
    required this.currentPrice,
    required this.source,
    required this.comment,
  });

  final String ticket, symbol;

  /// buy | sell
  final String side;
  final double volume, openPrice;
  final DateTime openTime;
  final double? sl, tp;
  final int? trailingPoints;
  final double swap, commission;

  /// The engine's last floating profit (USD) and price, when it sent them.
  final double? profit, currentPrice;
  final String source;
  final String? comment;

  bool get buy => side == 'buy';

  TPosition copyWith({double? volume, double? sl, double? tp, bool clearSl = false, bool clearTp = false, double? profit}) => TPosition(
    ticket: ticket,
    symbol: symbol,
    side: side,
    volume: volume ?? this.volume,
    openPrice: openPrice,
    openTime: openTime,
    sl: clearSl ? null : (sl ?? this.sl),
    tp: clearTp ? null : (tp ?? this.tp),
    trailingPoints: trailingPoints,
    swap: swap,
    commission: commission,
    profit: profit ?? this.profit,
    currentPrice: currentPrice,
    source: source,
    comment: comment,
  );

  static TPosition fromJson(Map<String, dynamic> p, {required bool cent}) {
    final k = cent ? 100.0 : 1.0;
    return TPosition(
      ticket: '${p['ticket']}',
      symbol: '${p['symbol'] ?? ''}',
      side: p['side'] == 'sell' ? 'sell' : 'buy',
      volume: _d(p['volume']),
      openPrice: _d(p['openPrice']),
      openTime: DateTime.tryParse('${p['openTime']}') ?? DateTime.now(),
      sl: _positive(p['sl']),
      tp: _positive(p['tp']),
      trailingPoints: _i(p['trailingPoints']) > 0 ? _i(p['trailingPoints']) : null,
      swap: _d(p['swap']) / k,
      commission: _d(p['commission']) / k,
      profit: p['profit'] == null ? null : _d(p['profit']) / k,
      currentPrice: _dn(p['currentPrice']),
      source: mapSource('${p['source'] ?? 'manual'}'),
      comment: p['comment'] is String && (p['comment'] as String).isNotEmpty ? p['comment'] as String : null,
    );
  }
}

double? _positive(Object? v) {
  final x = _dn(v);
  return x == null || x <= 0 ? null : x;
}

const Map<String, String> _sources = {
  'manual': 'manual',
  'ai': 'ai',
  'api': 'api',
  'fix': 'api',
  'webhook': 'api',
  'strategy': 'strategy',
  'copy': 'copy',
  'pamm': 'pamm',
  'mam': 'mam',
};

/// Engine source -> terminal source (web mapSource).
String mapSource(String s) => _sources[s] ?? 'manual';

/// A pending order (web mapOrder): `type` limit | stop | stop-limit; a triggered stop-limit is a limit at its
/// stop-limit price (MT5).
@immutable
class TOrder {
  const TOrder({
    required this.ticket,
    required this.symbol,
    required this.side,
    required this.type,
    required this.volume,
    required this.price,
    required this.stopLimit,
    required this.sl,
    required this.tp,
    required this.trailingPoints,
    required this.expiry,
    required this.expiryDate,
    required this.placed,
    required this.source,
    required this.comment,
    required this.oco,
  });

  final String ticket, symbol, side, type;
  final double volume, price;
  final double? stopLimit, sl, tp;
  final int? trailingPoints;

  /// GTC | Today | Date
  final String expiry;
  final String? expiryDate;
  final DateTime placed;
  final String source;
  final String? comment;

  /// The OCO pair id (the lower ticket of the two).
  final String? oco;

  bool get buy => side == 'buy';

  /// Translation key of "buy limit" etc. (web pendingLabelKey).
  String get labelKey => 'order.pending.$side.$type';

  /// English label for the journal (web PENDING_LABEL).
  String get label => '$side ${type == 'stop-limit' ? 'stop limit' : type}';

  static TOrder fromJson(Map<String, dynamic> o) {
    final rawType = '${o['type'] ?? 'limit'}';
    final triggered = rawType == 'stop_limit' && o['triggered'] == true;
    final exp = '${o['expiry'] ?? 'GTC'}';
    final expiry = exp == 'GTC' || exp == 'Today' ? exp : 'Date';
    final stopLimit = _dn(o['stopLimit']);
    final ticket = _i(o['ticket']);
    final oco = _i(o['oco']);
    return TOrder(
      ticket: '${o['ticket']}',
      symbol: '${o['symbol'] ?? ''}',
      side: o['side'] == 'sell' ? 'sell' : 'buy',
      type: triggered ? 'limit' : (rawType == 'stop_limit' ? 'stop-limit' : rawType),
      volume: _d(o['volume']),
      price: triggered && stopLimit != null ? stopLimit : _d(o['price']),
      stopLimit: !triggered ? stopLimit : null,
      sl: _positive(o['sl']),
      tp: _positive(o['tp']),
      trailingPoints: _i(o['trailingPoints']) > 0 ? _i(o['trailingPoints']) : null,
      expiry: expiry,
      expiryDate: expiry == 'Date' ? '${o['expiryAt'] ?? o['expiry']}'.substring(0, 10.clamp(0, '${o['expiryAt'] ?? o['expiry']}'.length)) : null,
      placed: DateTime.tryParse('${o['placedAt']}') ?? DateTime.now(),
      source: mapSource('${o['source'] ?? 'manual'}'),
      comment: o['comment'] is String && (o['comment'] as String).isNotEmpty ? o['comment'] as String : null,
      oco: oco > 0 ? '${oco < ticket ? oco : ticket}' : null,
    );
  }
}

/// A closed trade row (one per exit deal, web mapHistory). Money in USD; `profit` includes swap and commission.
@immutable
class TClosed {
  const TClosed({
    required this.deal,
    required this.ticket,
    required this.symbol,
    required this.side,
    required this.volume,
    required this.openPrice,
    required this.openTime,
    required this.closePrice,
    required this.closeTime,
    required this.swap,
    required this.commission,
    required this.profit,
    required this.reason,
    required this.source,
  });

  final String deal, ticket, symbol, side;
  final double volume, openPrice, closePrice, swap, commission, profit;
  final DateTime openTime, closeTime;
  final String reason, source;
}

const Map<String, String> _reasons = {
  'client': 'manual',
  'sl': 'sl',
  'tp': 'tp',
  'stop_out': 'stop out',
  'close_by': 'close by',
  'dealer': 'dealer',
  'force': 'dealer',
  'price_correction': 'correction',
  'pending_fill': 'manual',
};

/// Closing deals -> closed-trade rows, newest first. Entry deals give the commission share (web mapHistory).
List<TClosed> mapHistory(List<Map<String, dynamic>> deals, {required bool cent}) {
  final k = cent ? 100.0 : 1.0;
  final entries = <int, Map<String, dynamic>>{};
  for (final d in deals) {
    if (d['entry'] == 'in') entries.putIfAbsent(_i(d['positionTicket']), () => d);
  }
  double r2(double v) => (v * 100).roundToDouble() / 100;
  final out = <TClosed>[];
  for (final d in deals) {
    final entry = d['entry'];
    if ((entry != 'out' && entry != 'out_by') || d['reversed'] == true) continue;
    final e = entries[_i(d['positionTicket'])];
    final vol = _d(d['volume']);
    final entryVol = e == null ? 0.0 : _d(e['volume']);
    final entryCommission = e != null && entryVol > 0 ? _d(e['commission']) * (vol / entryVol).clamp(0, 1) : 0.0;
    final commission = (_d(d['commission']) + entryCommission) / k;
    final swap = _d(d['swap']) / k;
    final gross = _d(d['profit']) / k;
    out.add(
      TClosed(
        deal: '${d['id']}',
        ticket: '${d['positionTicket']}',
        symbol: '${d['symbol'] ?? ''}',
        side: d['positionSide'] == 'sell' ? 'sell' : 'buy',
        volume: vol,
        openPrice: _d(d['openPrice']),
        openTime: DateTime.tryParse('${d['openTime']}') ?? DateTime.now(),
        closePrice: _d(d['price']),
        closeTime: DateTime.tryParse('${d['time']}') ?? DateTime.now(),
        swap: r2(swap),
        commission: r2(commission),
        profit: r2(gross + swap - commission),
        reason: _reasons['${d['reason']}'] ?? '${d['reason'] ?? ''}',
        source: mapSource('${d['source'] ?? 'manual'}'),
      ),
    );
  }
  out.sort((a, b) => b.closeTime.compareTo(a.closeTime));
  return out;
}

/// Engine numbers of one position from an `equity` frame (USD; options also their mark and Greeks).
@immutable
class LivePos {
  const LivePos({required this.price, required this.profit, required this.swap, this.mark, this.greeks});
  final double price, profit, swap;
  final double? mark;
  final Map<String, double>? greeks;
}

/// Live account numbers from `equity` frames (<= 4 per second). USD.
@immutable
class LiveEquity {
  const LiveEquity({
    required this.balance,
    required this.credit,
    required this.profit,
    required this.swap,
    required this.equity,
    required this.margin,
    required this.freeMargin,
    required this.marginLevel,
    required this.positions,
  });

  final double balance, credit, profit, swap, equity, margin, freeMargin;
  final double? marginLevel;
  final Map<String, LivePos> positions;

  static LiveEquity fromFrame(Map<String, dynamic> f, {required bool cent}) {
    final k = cent ? 100.0 : 1.0;
    final pos = <String, LivePos>{};
    for (final raw in (f['positions'] as List? ?? const [])) {
      final p = _map(raw);
      final g = p['greeks'] is Map ? _map(p['greeks']).map((key, v) => MapEntry(key, _d(v))) : null;
      pos['${p['ticket']}'] = LivePos(price: _d(p['price']), profit: _d(p['profit']) / k, swap: _d(p['swap']) / k, mark: _dn(p['mark']), greeks: g);
    }
    return LiveEquity(
      balance: _d(f['balance']) / k,
      credit: (_d(f['credit']) + _d(f['bonus'])) / k,
      profit: _d(f['profit']) / k,
      swap: _d(f['swap']) / k,
      equity: _d(f['equity']) / k,
      margin: _d(f['margin']) / k,
      freeMargin: _d(f['freeMargin']) / k,
      marginLevel: _dn(f['marginLevel']),
      positions: pos,
    );
  }

  /// An account view (after a balance change, close, deposit…): numbers now, per-position values kept.
  static LiveEquity fromAccount(TAccount a, LiveEquity? prev) => LiveEquity(
    balance: a.balance,
    credit: a.credit,
    profit: a.profit,
    swap: a.swap,
    equity: a.equity,
    margin: a.margin,
    freeMargin: a.freeMargin,
    marginLevel: a.marginLevel,
    positions: prev?.positions ?? const {},
  );
}

/// The account numbers a screen shows (web useMetrics). USD; `level` is +infinity without margin in use.
@immutable
class Metrics {
  const Metrics({
    required this.balance,
    required this.credit,
    required this.equity,
    required this.margin,
    required this.free,
    required this.level,
    required this.floating,
  });
  final double balance, credit, equity, margin, free, level, floating;

  static const Metrics zero = Metrics(balance: 0, credit: 0, equity: 0, margin: 0, free: 0, level: double.infinity, floating: 0);

  static Metrics of(TAccount a, LiveEquity? live) {
    if (live != null) {
      return Metrics(
        balance: live.balance,
        credit: live.credit,
        equity: live.equity,
        margin: live.margin,
        free: live.freeMargin,
        level: live.marginLevel ?? double.infinity,
        floating: live.profit + live.swap,
      );
    }
    return Metrics(
      balance: a.balance,
      credit: a.credit,
      equity: a.equity,
      margin: a.margin,
      free: a.freeMargin,
      level: a.marginLevel ?? double.infinity,
      floating: a.profit + a.swap,
    );
  }
}

/// True for an engine position / order / deal of Kalks FX Options (they carry `option`), which the options mode shows.
bool isOptionEntry(Map<String, dynamic> j) =>
    j['option'] is Map || (j['symbol'] is String && RegExp(r'^[A-Z0-9]{3,12}-\d{8}-').hasMatch(j['symbol'] as String));

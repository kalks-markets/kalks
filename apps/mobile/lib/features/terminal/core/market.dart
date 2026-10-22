// Markets of Kalks Trader: the contract specs (`GET trade/symbols`, refreshed every 5 minutes like the web's live
// flags), the live quotes (market-data stream with the web's passive / active demand, lib/core/realtime/market_stream
// .dart), today's open for the change % (market-data `GET /v1/quotes?group=`), candle history (`GET /v1/candles`),
// forming bars and depth. Previews answer everything from lib/features/terminal/preview.
import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/config/app_config.dart';
import '../../../core/realtime/market_stream.dart';
import '../../../core/realtime/socket.dart';
import '../../../env.dart';
import '../preview/preview_server.dart';
import 'models.dart';

/// The markets the trade server knows, with the live switch (web scope.ts).
@immutable
class SymbolBook {
  const SymbolBook(this.all, {this.loadedAt});
  final Map<String, SymbolSpec> all;
  final DateTime? loadedAt;

  static const SymbolBook empty = SymbolBook({});

  SymbolSpec? operator [](String symbol) => all[symbol];

  /// The 28 core markets in the server's order (browsing; the search looks through everything visible).
  List<SymbolSpec> get core => all.values.where((s) => s.core).toList();

  /// What an account may see: live accounts (and guests) only core and live-enabled markets, demo everything.
  List<SymbolSpec> visible({required bool live}) => live ? all.values.where((s) => s.liveTradable).toList() : all.values.toList();

  bool isVisible(String symbol, {required bool live}) {
    final s = all[symbol];
    return s != null && (!live || s.liveTradable);
  }

  static SymbolBook fromJson(Map<String, dynamic> j) {
    final out = <String, SymbolSpec>{};
    for (final raw in (j['symbols'] as List? ?? const [])) {
      if (raw is! Map) continue;
      final s = SymbolSpec.fromJson(raw.cast<String, dynamic>());
      if (s.symbol.isNotEmpty) out[s.symbol] = s;
    }
    // the live / off lists win over the per-symbol flag (the BFF builds them from the same source)
    final live = {for (final s in (j['live'] as List? ?? const [])) '$s'};
    final off = {for (final s in (j['off'] as List? ?? const [])) '$s'};
    if (live.isNotEmpty || off.isNotEmpty) {
      for (final e in out.entries.toList()) {
        final s = e.value;
        if (s.core) continue;
        final on = live.contains(s.symbol) ? true : (off.contains(s.symbol) ? false : s.liveTrading);
        if (on != s.liveTrading) {
          out[e.key] = SymbolSpec.fromJson({..._specJson(s), 'liveTrading': on});
        }
      }
    }
    return SymbolBook(out, loadedAt: DateTime.now());
  }

  static Map<String, dynamic> _specJson(SymbolSpec s) => {
    'symbol': s.symbol,
    'name': s.name,
    'assetClass': s.assetClass,
    'digits': s.digits,
    'point': s.point,
    'pipSize': s.pipSize,
    'contractSize': s.contractSize,
    'profitCurrency': s.profitCurrency,
    'baseCurrency': s.baseCurrency,
    'lotMin': s.lotMin,
    'lotMax': s.lotMax,
    'lotStep': s.lotStep,
    'marginPct': s.marginPct,
    'maxLeverage': s.maxLeverage,
    'swapLong': s.swapLong,
    'swapShort': s.swapShort,
    'swapUnit': s.swapUnit,
    'tripleSwapDay': s.tripleSwapDay,
    'session': s.session,
    'open': s.open,
    'stopsLevelPoints': s.stopsLevelPoints,
    'core': s.core,
    'liveTrading': s.liveTrading,
  };
}

/// `GET trade/symbols` (public, cached 60 s by the server), reloaded every 5 minutes while the terminal is open.
class SymbolsController extends AsyncNotifier<SymbolBook> {
  Timer? _timer;

  @override
  Future<SymbolBook> build() async {
    ref.onDispose(() => _timer?.cancel());
    _timer = Timer.periodic(const Duration(minutes: 5), (_) => unawaited(_reload()));
    return _load();
  }

  Future<SymbolBook> _load() async => SymbolBook.fromJson(await ref.read(apiProvider).get<Map<String, dynamic>>('trade/symbols'));

  Future<void> _reload() async {
    try {
      state = AsyncData(await _load());
    } catch (_) {
      // keep the last list
    }
  }
}

final symbolsProvider = AsyncNotifierProvider<SymbolsController, SymbolBook>(SymbolsController.new);

/// The loaded book (empty until the first answer).
final symbolBookProvider = Provider<SymbolBook>((ref) => ref.watch(symbolsProvider).value ?? SymbolBook.empty);

/// One price of a market, with today's change.
@immutable
class TQuote {
  const TQuote({
    required this.symbol,
    required this.bid,
    required this.ask,
    required this.last,
    required this.change,
    required this.dir,
    required this.delayed,
    required this.time,
  });
  final String symbol;
  final double bid, ask, last;

  /// % against today's open (New York close).
  final double change;

  /// 1 up / -1 down / 0 (tick flash).
  final int dir;

  /// A delayed snapshot, not streaming: Buy / Sell stay disabled.
  final bool delayed;
  final int time;

  bool get valid => bid > 0 && ask > 0;
  double get mid => (bid + ask) / 2;
}

/// One candle (unix seconds, UTC).
@immutable
class Candle {
  const Candle(this.t, this.o, this.h, this.l, this.c, this.v);
  final int t;
  final double o, h, l, c, v;
}

typedef TQuoteListener = void Function(TQuote q);

/// Quotes, bars and depth for the terminal: the market-data stream plus today's opens, in the account's spread group.
class MarketFeed {
  MarketFeed({
    required this.wsUrl,
    required this.httpUrl,
    required List<String> symbols,
    String group = 'standard',
    SocketConnector? connector,
    Dio? http,
    bool sample = false,
  }) : _sample = Env.preview || sample,
       _http = http ?? Dio(BaseOptions(connectTimeout: const Duration(seconds: 10), receiveTimeout: const Duration(seconds: 20))) {
    _stream = MarketStream(wsUrl: wsUrl, allSymbols: symbols, group: group, connector: connector);
    _group = group;
  }

  final String wsUrl, httpUrl;

  /// Day snapshot and candles from the preview trade server (previews, the in-app demo) instead of the REST service.
  final bool _sample;
  final Dio _http;
  late final MarketStream _stream;
  late String _group;
  final Map<String, double> _open = {};
  final Map<String, ({double open, double high, double low})> _day = {};
  final Map<String, TQuote> _snap = {};
  final Map<String, Set<TQuoteListener>> _fns = {};
  bool _started = false;

  SocketStatus get status => _stream.status;
  String get group => _group;

  void start() {
    if (_started) return;
    _started = true;
    _stream.start();
    unawaited(_loadDay());
  }

  void stop() => _stream.stop();
  void resume() => _stream.resume();

  /// Every instrument the app knows (the passive subscription after the next (re)connect).
  set symbols(List<String> list) => _stream.allSymbols = list;

  /// The account's spread group: quotes carry its spread (the prices the engine fills at).
  void setGroup(String g) {
    final next = g.toLowerCase();
    if (next == _group) return;
    _group = next;
    _snap.clear();
    _stream.setGroup(next);
    unawaited(_loadDay());
  }

  Future<void> _loadDay() async {
    final g = _group;
    try {
      final Map<String, dynamic> data;
      if (_sample) {
        data = PreviewServer.instance.quotesSnapshot(g);
      } else {
        final r = await _http.get<Object?>('$httpUrl/v1/quotes', queryParameters: {'group': g});
        if (r.data is! Map) return;
        data = (r.data! as Map).cast<String, dynamic>();
      }
      if (g != _group) return;
      for (final e in data.entries) {
        if (e.value is! Map) continue;
        final q = (e.value as Map).cast<String, dynamic>();
        double n(Object? v) => v is num ? v.toDouble() : 0;
        final bid = n(q['bid']), ask = n(q['ask']);
        final mid = n(q['last']) > 0 ? n(q['last']) : (bid + ask) / 2;
        final o = n(q['o']);
        _open[e.key] = o > 0 ? o : (_open[e.key] ?? mid);
        if (o > 0) _day[e.key] = (open: o, high: n(q['h']) > 0 ? n(q['h']) : mid, low: n(q['l']) > 0 ? n(q['l']) : mid);
        final prev = _stream.quote(e.key);
        if (prev == null && bid > 0 && ask > 0) {
          _snap[e.key] = TQuote(
            symbol: e.key,
            bid: bid,
            ask: ask,
            last: mid,
            change: _change(e.key, mid),
            dir: 0,
            delayed: q['d'] == 1 || q['d'] == true,
            time: (q['t'] as num?)?.toInt() ?? 0,
          );
          for (final fn in [...?_fns[e.key]]) {
            fn(_snap[e.key]!);
          }
        }
      }
    } catch (_) {
      // the stream still brings prices; the change % starts at 0
    }
  }

  double _change(String symbol, double last) {
    final o = _open[symbol];
    if (o == null || o <= 0) {
      _open[symbol] = last;
      return 0;
    }
    return (last - o) / o * 100;
  }

  TQuote _wrap(Quote q) =>
      TQuote(symbol: q.symbol, bid: q.bid, ask: q.ask, last: q.last, change: _change(q.symbol, q.last), dir: q.dir, delayed: q.delayed, time: q.time);

  /// The last price of `symbol` (stream, else the REST snapshot), or null.
  TQuote? quote(String symbol) {
    final q = _stream.quote(symbol);
    return q != null ? _wrap(q) : _snap[symbol];
  }

  /// The bid of a symbol (currency conversions).
  double? bidOf(String symbol) => quote(symbol)?.bid;

  /// Today's open / high / low.
  ({double open, double high, double low})? day(String symbol) => _day[symbol];

  /// Live quotes of `symbols` while subscribed (a listener counts as active demand: only rows on screen).
  void Function() subscribe(Iterable<String> symbols, TQuoteListener fn) {
    final list = symbols.toList();
    for (final s in list) {
      (_fns[s] ??= {}).add(fn);
    }
    final un = _stream.subscribe(list, (q) => fn(_wrap(q)));
    for (final s in list) {
      final snap = _stream.quote(s) == null ? _snap[s] : null;
      if (snap != null) fn(snap);
    }
    return () {
      un();
      for (final s in list) {
        _fns[s]?.remove(fn);
      }
    };
  }

  /// Symbols an owner wants live (favourites, positions and orders, the chart).
  void want(String owner, Iterable<String> symbols) => _stream.want(owner, symbols);

  void Function() subscribeBars(String symbol, String tf, BarListener fn) => _stream.subscribeBars(symbol, tf, fn);

  /// After every reconnect of the stream (bars sent meanwhile were missed). Returns the unsubscribe.
  void Function() onReconnect(void Function() fn) => _stream.onReconnect(fn);
  void Function() subscribeDepth(String symbol, DepthListener fn) => _stream.subscribeDepth(symbol, fn);

  /// Candle history (oldest first), `to` = unix seconds inclusive. Empty when unavailable.
  Future<List<Candle>> candles(String symbol, String tf, {int limit = 1000, int? to}) async {
    if (_sample) return PreviewServer.instance.candlesAs(symbol, tf, Candle.new, limit: limit, to: to);
    try {
      final r = await _http.get<Object?>(
        '$httpUrl/v1/candles',
        queryParameters: {'symbol': symbol, 'tf': tf, 'limit': '$limit', 'to': ?(to == null ? null : '$to')},
      );
      final bars = r.data is Map ? (r.data! as Map)['bars'] : null;
      if (bars is! List) return const [];
      double n(Object? v) => v is num ? v.toDouble() : 0;
      return [
        for (final b in bars)
          if (b is Map) Candle((b['t'] as num?)?.toInt() ?? 0, n(b['o']), n(b['h']), n(b['l']), n(b['c']), n(b['v'])),
      ];
    } catch (_) {
      return const [];
    }
  }
}

/// How the market-data socket connects (previews and the in-app demo: the preview server; tests override it).
final marketConnectorProvider = Provider<SocketConnector?>((ref) => Env.preview || ref.watch(demoModeProvider) ? PreviewServer.instance.marketConnector : null);

/// The terminal's market feed (one per open terminal; closed with it).
final marketFeedProvider = Provider.autoDispose<MarketFeed>((ref) {
  final cfg = ref.watch(configProvider);
  final feed = MarketFeed(
    wsUrl: cfg.marketDataWs,
    httpUrl: cfg.marketDataHttp,
    symbols: const [],
    connector: ref.watch(marketConnectorProvider),
    sample: ref.watch(demoModeProvider),
  );
  ref.onDispose(feed.stop);
  return feed;
});

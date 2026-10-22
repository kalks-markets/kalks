// Live quotes from the market-data service (config.urls.marketData.ws + ?group=<account spread group>), with the
// web's demand rules (packages/mock/src/prices.ts) so the app adds no extra provider load:
// - on every (re)connect: subscribe to EVERY instrument PASSIVELY (prices when they stream anyway, delayed snapshots
//   otherwise), then send the active demand;
// - ACTIVE only for what is wanted: symbols with a live listener (rows on screen, the open ticket) plus explicit
//   wants (favourites, positions and orders, the chart) — `want(owner, symbols)`;
// - demand changes are debounced (700 ms quiet, at most every 3 s) and sent as diffs: `subscribe` for new symbols,
//   `subscribe` + `passive: true` for dropped ones;
// - charts and depth ask through `bars` / `unbars` and `depth` / `undepth`; after a reconnect a chart fetches the bars
//   it missed (`onReconnect`).
// Frames: quote {s, b, a, l?, t, d?}, bar {s, tf, t, o, h, l, c, v}, depth {s, src, t, b, a}.
import 'dart:async';

import 'backoff.dart';
import 'socket.dart';

class Quote {
  const Quote({required this.symbol, required this.bid, required this.ask, required this.last, required this.time, this.delayed = false, this.dir = 0});
  final String symbol;
  final double bid, ask, last;

  /// Unix ms of the price.
  final int time;

  /// A delayed snapshot, not streaming: Buy / Sell stay disabled (terminal rule).
  final bool delayed;

  /// 1 up / -1 down / 0 versus the previous quote (tick flash).
  final int dir;
}

class Bar {
  const Bar(this.t, this.o, this.h, this.l, this.c, this.v);
  final int t;
  final double o, h, l, c, v;
}

typedef QuoteListener = void Function(Quote q);
typedef BarListener = void Function(Bar b);
typedef DepthListener = void Function(Map<String, dynamic> depth);

class MarketStream {
  MarketStream({
    required this.wsUrl,
    required this.allSymbols,
    this.group = 'standard',
    SocketConnector? connector,
    this.debounce = const Duration(milliseconds: 700),
    this.maxWait = const Duration(seconds: 3),
  }) {
    _socket = ReconnectingSocket(
      name: 'market-data',
      resolveUrl: () async => Uri.parse('$wsUrl?group=${Uri.encodeQueryComponent(group)}'),
      onFrame: _onFrame,
      onOpen: _onOpen,
      backoff: Backoff(first: const Duration(milliseconds: 250), max: const Duration(seconds: 2), jitter: 0.2),
      silentAfter: const Duration(seconds: 12),
      watchEvery: const Duration(seconds: 2),
      connector: connector,
    );
  }

  /// config.urls.marketData.ws
  final String wsUrl;

  /// Every instrument the app knows (from `trade/symbols`): the passive subscription.
  List<String> allSymbols;

  /// The account group whose spread the quotes carry (raw / standard / pro / ecn / cent).
  String group;

  /// Demand changes go out after this quiet time …
  final Duration debounce;

  /// … and at most this long after the first change.
  final Duration maxWait;
  late final ReconnectingSocket _socket;

  final Map<String, Quote> _quotes = {};
  final Map<String, Set<QuoteListener>> _listeners = {};
  final Map<String, Set<String>> _wants = {};
  final Map<String, Set<BarListener>> _bars = {};
  final Map<String, Set<DepthListener>> _depth = {};
  final Set<void Function()> _reconnects = {};
  bool _opened = false;
  Set<String> _activeSent = {};
  Timer? _demandTimer;
  DateTime? _demandSince;

  /// What was last sent to the service as active (tests, diagnostics).
  Set<String> get activeSent => Set.unmodifiable(_activeSent);
  SocketStatus get status => _socket.status;

  void start() => _socket.start();
  void stop() {
    _demandTimer?.cancel();
    _socket.stop();
  }

  void resume() => _socket.resume();

  Quote? quote(String symbol) => _quotes[symbol];

  /// Switches the spread group: a new socket (its quotes carry that group's spread).
  void setGroup(String g) {
    final next = g.toLowerCase();
    if (next == group) return;
    group = next;
    _socket.reconnect();
  }

  /// Live quotes for `symbols` while subscribed; a listener counts as active demand. Returns the unsubscribe.
  void Function() subscribe(Iterable<String> symbols, QuoteListener fn) {
    final list = symbols.toList();
    for (final s in list) {
      (_listeners[s] ??= {}).add(fn);
      final q = _quotes[s];
      if (q != null) fn(q);
    }
    _scheduleDemand();
    return () {
      for (final s in list) {
        _listeners[s]?.remove(fn);
        if (_listeners[s]?.isEmpty ?? false) _listeners.remove(s);
      }
      _scheduleDemand();
    };
  }

  /// Symbols `owner` wants streamed live (replaces its previous set; empty clears it): favourites, the account's
  /// positions and orders, the chart.
  void want(String owner, Iterable<String> symbols) {
    final set = symbols.toSet();
    if (set.isEmpty) {
      _wants.remove(owner);
    } else {
      _wants[owner] = set;
    }
    _scheduleDemand();
  }

  /// The forming bar of `symbol` / `tf` (M1 … MN).
  void Function() subscribeBars(String symbol, String tf, BarListener fn) {
    final key = '$symbol|$tf';
    final set = _bars[key] ??= {};
    if (set.isEmpty) _socket.send({'op': 'bars', 'symbol': symbol, 'tf': tf});
    set.add(fn);
    return () {
      set.remove(fn);
      if (set.isEmpty) {
        _bars.remove(key);
        _socket.send({'op': 'unbars', 'symbol': symbol, 'tf': tf});
      }
    };
  }

  /// Called after every reconnect (not the first connect): bar updates sent while the socket was down are lost, so
  /// a chart fetches the bars it missed. Returns the unsubscribe.
  void Function() onReconnect(void Function() fn) {
    _reconnects.add(fn);
    return () => _reconnects.remove(fn);
  }

  /// The depth-of-market ladder of `symbol` (every quote change pushes a fresh book).
  void Function() subscribeDepth(String symbol, DepthListener fn) {
    final set = _depth[symbol] ??= {};
    if (set.isEmpty) {
      _socket.send({
        'op': 'depth',
        'symbols': [symbol],
      });
    }
    set.add(fn);
    return () {
      set.remove(fn);
      if (set.isEmpty) {
        _depth.remove(symbol);
        _socket.send({
          'op': 'undepth',
          'symbols': [symbol],
        });
      }
    };
  }

  void _onOpen() {
    // every instrument passively, then the active demand from scratch
    _socket.send({'op': 'subscribe', 'symbols': allSymbols, 'passive': true});
    _activeSent = {};
    _flushDemand();
    for (final key in _bars.keys) {
      final p = key.split('|');
      _socket.send({'op': 'bars', 'symbol': p[0], 'tf': p[1]});
    }
    if (_depth.isNotEmpty) _socket.send({'op': 'depth', 'symbols': _depth.keys.toList()});
    if (_opened) {
      for (final fn in [..._reconnects]) {
        fn();
      }
    }
    _opened = true;
  }

  void _scheduleDemand() {
    final now = DateTime.now();
    if (_demandTimer == null) {
      _demandSince = now;
    } else {
      _demandTimer!.cancel();
    }
    final sinceStart = now.difference(_demandSince ?? now);
    var wait = maxWait - sinceStart;
    if (wait > debounce) wait = debounce;
    if (wait.isNegative) wait = Duration.zero;
    _demandTimer = Timer(wait, () {
      _demandTimer = null;
      _flushDemand();
    });
  }

  /// Sends the demand diff now (only while connected; a reconnect sends it from scratch).
  void _flushDemand() {
    if (!_socket.isOpen) return;
    final want = <String>{
      for (final e in _listeners.entries)
        if (e.value.isNotEmpty) e.key,
      for (final set in _wants.values) ...set,
    };
    final on = want.difference(_activeSent).toList();
    final off = _activeSent.difference(want).toList();
    if (on.isNotEmpty) _socket.send({'op': 'subscribe', 'symbols': on});
    if (off.isNotEmpty) _socket.send({'op': 'subscribe', 'symbols': off, 'passive': true});
    _activeSent = want;
  }

  static double _d(Object? v) => v is num ? v.toDouble() : double.tryParse('$v') ?? 0;

  void _onFrame(Map<String, dynamic> m) {
    switch (m['type']) {
      case 'quote':
        final s = m['s'] as String?;
        final bid = _d(m['b']), ask = _d(m['a']);
        if (s == null || bid <= 0 || ask <= 0) return;
        final prev = _quotes[s];
        final last = _d(m['l']) > 0 ? _d(m['l']) : (bid + ask) / 2;
        final q = Quote(
          symbol: s,
          bid: bid,
          ask: ask,
          last: last,
          time: (m['t'] as num?)?.toInt() ?? 0,
          delayed: m['d'] == 1 || m['d'] == true,
          dir: prev == null ? 0 : (bid > prev.bid ? 1 : (bid < prev.bid ? -1 : 0)),
        );
        _quotes[s] = q;
        for (final fn in [...?_listeners[s]]) {
          fn(q);
        }
      case 'bar':
        final b = Bar((m['t'] as num?)?.toInt() ?? 0, _d(m['o']), _d(m['h']), _d(m['l']), _d(m['c']), _d(m['v']));
        for (final fn in [...?_bars['${m['s']}|${m['tf']}']]) {
          fn(b);
        }
      case 'depth':
        for (final fn in [...?_depth['${m['s']}']]) {
          fn(m);
        }
    }
  }
}

// Candle history kept for the session per market and timeframe (MT5-style instant timeframe switching): a chart
// shows what it has at once and refreshes it in the background; the neighbouring timeframes of the one on screen are
// fetched ahead; the same request is never in flight twice. The live forming bar keeps the cached copy current.
import 'dart:async';

import 'package:flutter/foundation.dart';

import '../core/market.dart' show Candle;

class ChartHistoryCache {
  ChartHistoryCache({this.capacity = 32});

  /// The app's one cache (it outlives a chart: switching back to a timeframe or a market draws at once).
  static final ChartHistoryCache instance = ChartHistoryCache();

  /// The most series kept (least recently used go first; 1,000-1,500 candles each).
  final int capacity;

  final Map<String, List<Candle>> _bars = {};
  final Map<String, Future<List<Candle>>> _inflight = {};

  static String keyOf(String source, String tf) => '$source|$tf';

  /// The cached candles (oldest first), or null.
  List<Candle>? get(String key) {
    final v = _bars.remove(key);
    if (v == null) return null;
    _bars[key] = v;
    return v;
  }

  bool has(String key) => _bars[key]?.isNotEmpty ?? false;

  void put(String key, List<Candle> bars) {
    if (bars.isEmpty) return;
    _bars.remove(key);
    _bars[key] = [...bars];
    while (_bars.length > capacity) {
      _bars.remove(_bars.keys.first);
    }
  }

  /// The forming bar (new or updated) into the cached series, so the next switch back starts from it.
  void upsert(String key, Candle bar) {
    final list = _bars[key];
    if (list == null || list.isEmpty) return;
    final last = list.last;
    if (bar.t < last.t) return;
    if (bar.t > last.t) {
      list.add(bar);
    } else {
      list[list.length - 1] = bar;
    }
  }

  /// One request per key at a time: a chart that opens a timeframe being fetched ahead waits for that answer.
  Future<List<Candle>> fetch(String key, Future<List<Candle>> Function() load) {
    final running = _inflight[key];
    if (running != null) return running;
    // (a block body: returning the removed future from whenComplete would make it wait for itself)
    final f = load().whenComplete(() {
      _inflight.remove(key);
    });
    _inflight[key] = f;
    return f;
  }

  /// Fetches `key` ahead unless it is cached or already on its way.
  Future<void> prefetch(String key, Future<List<Candle>> Function() load) async {
    if (has(key) || _inflight.containsKey(key)) return;
    try {
      final bars = await fetch(key, load);
      // a chart may have stored a fresher copy meanwhile
      if (!has(key)) put(key, bars);
    } catch (_) {
      // ahead-of-time only: the chart asks again when it opens
    }
  }

  @visibleForTesting
  void clear() {
    _bars.clear();
    _inflight.clear();
  }
}

/// The timeframes next to `tf` in `order` (the strip's neighbours: the likely next tap).
List<String> neighbourTimeframes(String tf, List<String> order) {
  final i = order.indexOf(tf);
  if (i < 0) return const [];
  return [if (i + 1 < order.length) order[i + 1], if (i > 0) order[i - 1]];
}

/// A fresh history page merged into the candles a chart shows (older pages it scrolled back to and a forming bar
/// newer than the page are kept). When the shown candles already line up with the page up to their last bar, only
/// `tail` (that last bar and the newer ones, at most `maxTail`) needs to reach the chart: an update in place, the
/// view stays where the reader put it. Otherwise `tail` is null and the chart redraws `bars`.
({List<Candle> bars, List<Candle>? tail}) mergeHistory(List<Candle> shown, List<Candle> fresh, {int maxTail = 64}) {
  if (fresh.isEmpty) return (bars: shown, tail: shown.isEmpty ? null : const <Candle>[]);
  if (shown.isEmpty) return (bars: [...fresh], tail: null);
  final first = fresh.first.t, last = fresh.last.t;
  final bars = <Candle>[
    for (final c in shown)
      if (c.t < first) c,
    ...fresh,
    for (final c in shown)
      if (c.t > last) c,
  ];
  final edge = shown.last.t;
  // the same candle times before the shown last bar: nothing in the middle moved
  var i = 0;
  while (i < shown.length && shown[i].t < edge) {
    if (i >= bars.length || bars[i].t != shown[i].t) return (bars: bars, tail: null);
    i++;
  }
  if (i < bars.length && bars[i].t < edge) return (bars: bars, tail: null);
  final tail = bars.sublist(i);
  return (bars: bars, tail: tail.length <= maxTail ? tail : null);
}

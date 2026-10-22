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

bool _same(Candle a, Candle b) => a.t == b.t && a.o == b.o && a.h == b.h && a.l == b.l && a.c == b.c && a.v == b.v;

/// The newest bars from the server merged into the candles a chart shows: the latest page after the live stream
/// reconnected (`page`; one that starts after the shown bars, a gap longer than the page, replaces them), or the last
/// few bars after a bar closed (their stored, final values). For the latter the shown forming bar keeps its close
/// (live updates went on while the answer was on its way) and takes in the answer's range. Null when nothing
/// changed; `tail` = the changed bars when they are all at or after the shown last bar (an update in place), else
/// null (the chart redraws them all, the view stays).
({List<Candle> bars, List<Candle>? tail})? refreshBars(List<Candle> shown, List<Candle> fresh, {bool page = false}) {
  if (fresh.isEmpty) return null;
  if (shown.isEmpty || (page && fresh.first.t > shown.last.t)) return (bars: [...fresh], tail: null);
  final bars = mergeHistory(shown, fresh).bars;
  final live = shown.last;
  if (!page && fresh.last.t == live.t) {
    final i = bars.lastIndexWhere((c) => c.t == live.t);
    final f = bars[i];
    bars[i] = Candle(live.t, f.o, f.h > live.h ? f.h : live.h, f.l < live.l ? f.l : live.l, live.c, f.v > live.v ? f.v : live.v);
  }
  var i = 0;
  while (i < shown.length && i < bars.length && _same(shown[i], bars[i])) {
    i++;
  }
  if (i == shown.length && i == bars.length) return null;
  final tail = i < bars.length && i >= shown.length - 1 && bars[i].t >= live.t ? bars.sublist(i) : null;
  return (bars: bars, tail: tail);
}

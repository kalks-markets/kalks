// The quotes' 24 h sparklines: the last 24 hourly closes of a market from the market-data candles (`GET /v1/candles`,
// the same history the chart draws; previews answer from the preview trade server). Fetched lazily, only for the rows
// on screen (a row asks when it is built), one request per market at a time, and kept for about five minutes. No
// candles (the service is down, a market without history): no sparkline, never a made-up line.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'market.dart';

/// Hourly bars of the last day.
const String kSparkTf = 'H1';
const int kSparkBars = 24;

/// How long a fetched line stays fresh.
const Duration kSparkTtl = Duration(minutes: 5);

class SparkCache {
  SparkCache(this._load, {DateTime Function()? now}) : _now = now ?? DateTime.now;

  /// Candles of a market (oldest first).
  final Future<List<Candle>> Function(String symbol) _load;
  final DateTime Function() _now;
  final Map<String, ({DateTime at, List<double> closes})> _lines = {};
  final Map<String, Future<List<double>>> _inflight = {};

  /// The cached closes of `symbol` while fresh, else null.
  List<double>? peek(String symbol) {
    final e = _lines[symbol];
    if (e == null || _now().difference(e.at) > kSparkTtl) return null;
    return e.closes;
  }

  /// The closes of `symbol`: the cached line while fresh, else one fetch (shared by every row asking meanwhile).
  Future<List<double>> get(String symbol) {
    final hit = peek(symbol);
    if (hit != null) return SynchronousFuture(hit);
    return _inflight[symbol] ??= _fetch(symbol);
  }

  Future<List<double>> _fetch(String symbol) async {
    try {
      final bars = await _load(symbol);
      final closes = [
        for (final b in bars)
          if (b.c.isFinite && b.c > 0) b.c,
      ];
      final last = closes.length > kSparkBars ? closes.sublist(closes.length - kSparkBars) : closes;
      _lines[symbol] = (at: _now(), closes: last);
      return last;
    } catch (_) {
      return const [];
    } finally {
      unawaited(_inflight.remove(symbol));
    }
  }
}

/// One cache per open terminal (it follows the market feed).
final sparkCacheProvider = Provider.autoDispose<SparkCache>((ref) {
  final feed = ref.watch(marketFeedProvider);
  return SparkCache((s) => feed.candles(s, kSparkTf, limit: kSparkBars));
});

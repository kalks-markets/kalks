// One chart window (web: components/chart/engine.ts + chart-view.tsx): candle history from market data (cached for
// the session per symbol / timeframe so switching draws at once and ticks at once, then refreshed in the background
// in place; the neighbouring timeframes are fetched ahead: history_cache.dart), the forming bar from the stream, the
// account's bid / ask lines, and the trade lines (positions, SL / TP, pending orders, alerts) with drag / tap
// callbacks. Charts are drawn in broker server time like MT5 (GMT+3 in US summer time, GMT+2 otherwise).
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/realtime/market_stream.dart' show Bar;
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/trade_math.dart';
import '../core/workspace.dart';
import 'chart_bridge.dart';
import 'chart_surface.dart';
import 'history_cache.dart';
import 'indicators.dart';

/// The US autumn switch of `year` (first Sunday of November, 06:00 UTC): server time goes back from GMT+3 to GMT+2.
int _autumnSwitch(int year) {
  final first = DateTime.utc(year, 11);
  final sunday = 1 + (7 - first.weekday % 7) % 7;
  return DateTime.utc(year, 11, sunday, 6).millisecondsSinceEpoch ~/ 1000;
}

/// UTC unix seconds -> chart time (server time). Strictly increasing, which lightweight-charts needs: when server time
/// goes back an hour (autumn switch) the two UTC hours around the switch share the one server hour 08:00-09:00, at
/// half speed (crypto's M1-H4 bars there are 30 s of chart time apart), instead of the second hour repeating the
/// first one's times (an out-of-order series the chart refuses to draw).
int toChartTime(int utc) {
  final sw = _autumnSwitch(DateTime.fromMillisecondsSinceEpoch(utc * 1000, isUtc: true).year);
  if (utc >= sw - 3600 && utc < sw + 3600) return sw + 2 * 3600 + (utc - (sw - 3600)) ~/ 2;
  return utc + serverOffsetSeconds(DateTime.fromMillisecondsSinceEpoch(utc * 1000, isUtc: true));
}

/// Chart time -> UTC unix seconds (DST-aware inverse of [toChartTime]).
int fromChartTime(int chart) {
  final sw = _autumnSwitch(DateTime.fromMillisecondsSinceEpoch(chart * 1000, isUtc: true).year);
  if (chart >= sw + 2 * 3600 && chart < sw + 3 * 3600) return sw - 3600 + (chart - (sw + 2 * 3600)) * 2;
  final summer = chart - 3 * 3600;
  return toChartTime(summer) == chart ? summer : chart - 2 * 3600;
}

/// Where a chart's candles come from (market data by default; the options mode can give its own premium candles).
abstract class ChartSource {
  /// Cache key (symbol / series).
  String get key;
  int get digits;

  /// History oldest first, UTC seconds; `to` = inclusive end (older pages).
  Future<List<Candle>> history(String tf, {int? to});

  /// The newest `count` bars (oldest first, the forming one last): a look at the bars that just closed.
  Future<List<Candle>> recent(String tf, int count) => history(tf);

  /// The forming bar; returns the unsubscribe (null: no live bars).
  void Function()? bars(String tf, void Function(Candle bar) onBar) => null;

  /// Called after the live bars reconnected (bar updates sent while they were down were lost); returns the
  /// unsubscribe (null: never).
  void Function()? resyncs(void Function() onResync) => null;

  /// Bid / ask lines; returns the unsubscribe (null: none).
  void Function()? quotes(void Function(double bid, double ask) onQuote) => null;

  /// Whether the neighbouring timeframes may be fetched ahead (cheap: one market-data request each).
  bool get prefetch => false;
}

/// A market of the market-data service.
class MarketChartSource extends ChartSource {
  MarketChartSource(this.feed, this.symbol, this.digits);
  final MarketFeed feed;
  final String symbol;
  @override
  final int digits;

  @override
  String get key => symbol;

  @override
  Future<List<Candle>> history(String tf, {int? to}) => feed.candles(symbol, tf, limit: to == null ? 1000 : 1500, to: to);

  @override
  Future<List<Candle>> recent(String tf, int count) => feed.candles(symbol, tf, limit: count);

  @override
  void Function()? bars(String tf, void Function(Candle bar) onBar) => feed.subscribeBars(symbol, tf, (Bar b) => onBar(Candle(b.t, b.o, b.h, b.l, b.c, b.v)));

  @override
  void Function()? resyncs(void Function() onResync) => feed.onReconnect(onResync);

  @override
  void Function()? quotes(void Function(double bid, double ask) onQuote) => feed.subscribe([symbol], (q) => onQuote(q.bid, q.ask));

  @override
  bool get prefetch => true;
}

class TerminalChart extends ConsumerStatefulWidget {
  const TerminalChart({
    super.key,
    required this.symbol,
    required this.tf,
    this.source,
    this.lines = const [],
    this.onLineDragged,
    this.onLineTapped,
    this.onLineClosed,
    this.onLongPress,
    this.chartType = 'candles',
    this.indicators = const [],
    this.onIndicatorTapped,
    this.topInset = 0,
    this.legendTitle = true,
  });

  /// The market (also the cache key when no source is given).
  final String symbol;

  /// M1 … MN
  final String tf;
  final ChartSource? source;
  final List<ChartLine> lines;

  /// A line dropped at `price`. Until a returned future completes the line stays at the drop price on the chart
  /// (then the chart shows the lines it is given again: the new price once the server confirmed it, else the old one).
  final FutureOr<void> Function(ChartLine line, double price)? onLineDragged;
  final void Function(ChartLine line)? onLineTapped;

  /// The × on a line's chip.
  final void Function(ChartLine line)? onLineClosed;
  final void Function(double price)? onLongPress;

  /// candles | bars | line | area
  final String chartType;

  /// Indicator instances drawn by the chart page (the web's own indicator code).
  final List<IndInstance> indicators;

  /// An indicator's legend row was tapped.
  final void Function(String uid)? onIndicatorTapped;

  /// Room an overlay of the app takes at the top of the plot (the trade panel): the legend starts below it.
  final double topInset;

  /// The legend names the market and timeframe (false: the app shows them over the chart; the legend has the OHLC).
  final bool legendTitle;

  /// Widget tests and the web preview draw the native chart (no WebView there).
  static bool forceNative = false;

  /// How long after a bar closed the chart takes its stored values (the server replaces a closed bar with the
  /// provider's final OHLC 4 s after it closes): what is drawn then matches the history a reload shows.
  static Duration settleDelay = const Duration(seconds: 8);

  @override
  ConsumerState<TerminalChart> createState() => _TerminalChartState();
}

class _TerminalChartState extends ConsumerState<TerminalChart> {
  ChartHistoryCache get _cache => ChartHistoryCache.instance;
  final ChartSurfaceController _c = ChartSurfaceController();
  final List<void Function()> _unsubs = [];
  List<Candle> _data = [];
  int _gen = 0;
  bool _dark = true;
  ChartPalette? _palette;

  /// A dropped line kept at its drop price until the app's action finished: (id, price).
  (String, double)? _held;
  Timer? _holdTimer;

  /// The look at the bar that just closed ([TerminalChart.settleDelay]).
  Timer? _settle;

  /// A stop made from a position line's S / T handle (lineForHandle): not among the given lines until the position
  /// carries it, so it is added to what the chart shows while it is held.
  ChartLine? _fresh;

  ChartSource get _source {
    final s = widget.source;
    if (s != null) return s;
    final spec = ref.read(symbolBookProvider)[widget.symbol];
    return MarketChartSource(ref.read(marketFeedProvider), widget.symbol, spec?.digits ?? 5);
  }

  @override
  void initState() {
    super.initState();
    _c.onEvent = _onEvent;
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    final k = context.k;
    final p = chartPalette(k);
    if (_palette == null) {
      _palette = p;
      _dark = k.dark;
      _load();
    } else if (_dark != k.dark || p.up != _palette!.up) {
      _palette = p;
      _dark = k.dark;
      _c.send(ChartCmd.palette(p));
    }
  }

  @override
  void didUpdateWidget(TerminalChart old) {
    super.didUpdateWidget(old);
    final sourceChanged = (old.source?.key ?? old.symbol) != (widget.source?.key ?? widget.symbol);
    if (sourceChanged || old.tf != widget.tf) {
      _load();
      return;
    }
    if (!listEquals(old.lines, widget.lines)) _sendLines();
    if (old.chartType != widget.chartType) _c.send(ChartCmd.chartType(widget.chartType));
    if (!listEquals(old.indicators, widget.indicators)) _c.send(ChartCmd.indicators(_indicatorsJson));
    if (old.topInset != widget.topInset) _c.send(ChartCmd.inset(widget.topInset));
  }

  List<Map<String, Object?>> get _indicatorsJson => [for (final i in widget.indicators) i.toJson()];

  /// The lines to draw: the given ones, with a just-dropped line kept at its drop price (and a stop just made from
  /// an S / T handle added until the position carries it).
  void _sendLines() {
    var h = _held;
    // the confirmed line is at the drop price: nothing to hold any more
    if (h != null && widget.lines.any((l) => l.id == h!.$1 && (l.price - h.$2).abs() < 1e-9)) {
      _held = h = null;
      _fresh = null;
      _holdTimer?.cancel();
      _holdTimer = null;
    }
    final f = _fresh;
    final add = f != null && h != null && h.$1 == f.id && !widget.lines.any((l) => l.id == f.id);
    final lines = h == null ? widget.lines : [for (final l in widget.lines) l.id == h.$1 ? l.at(h.$2) : l, if (add) f.at(h.$2)];
    _c.send(ChartCmd.lines(lines));
  }

  void _release() {
    _holdTimer?.cancel();
    _holdTimer = null;
    _fresh = null;
    if (_held == null) return;
    _held = null;
    if (mounted) _sendLines();
  }

  @override
  void dispose() {
    for (final u in _unsubs) {
      u();
    }
    _unsubs.clear();
    _holdTimer?.cancel();
    _settle?.cancel();
    _c.dispose();
    super.dispose();
  }

  static ChartBar _bar(Candle c) => (t: toChartTime(c.t), o: c.o, h: c.h, l: c.l, c: c.c, v: c.v);

  Future<void> _load() async {
    final gen = ++_gen;
    // a fresh chart (another market, a reloaded page) has no drag in progress
    _c.holdLines = false;
    _settle?.cancel();
    for (final u in _unsubs) {
      u();
    }
    _unsubs.clear();
    final src = _source;
    final tf = widget.tf;
    final key = ChartHistoryCache.keyOf(src.key, tf);
    final t = context.t;
    _c.send(
      ChartCmd.init(
        digits: src.digits,
        intraday: (kTfSeconds[tf] ?? 3600) < 86400,
        palette: _palette!,
        symbol: widget.symbol,
        tf: tf,
        step: kTfSeconds[tf] ?? 60,
        chartType: widget.chartType,
        indicators: _indicatorsJson,
        title: widget.legendTitle,
        texts: {
          'more': t('chart.legend.more', {'count': '{count}'}),
          'less': t('chart.legend.showLess'),
          // a position line's S / T handles
          'sl': t('chart.line.slHandle'),
          'tp': t('chart.line.tpHandle'),
          'slTitle': t('chart.line.slHandleTitle'),
          'tpTitle': t('chart.line.tpHandleTitle'),
        },
      ),
    );
    if (widget.topInset > 0) _c.send(ChartCmd.inset(widget.topInset));
    _sendLines();
    // the session's copy first (switching back to a timeframe or a market draws at once) …
    final cached = _cache.get(key);
    _data = cached == null ? [] : [...cached];
    if (_data.isNotEmpty) _c.send(ChartCmd.bars(_data.map(_bar).toList()));
    final q = src.quotes((bid, ask) {
      if (gen == _gen) _c.send(ChartCmd.quote(bid, ask));
    });
    if (q != null) _unsubs.add(q);
    // … ticking at once (every bar from the stream reaches the chart on the next frame) …
    var live = false;
    if (_data.isNotEmpty) {
      _liveBars(src, tf, key, gen);
      live = true;
    }
    // … then refreshed in the background (the same request as a fetch-ahead of this timeframe, if one is running)
    final fresh = await _cache.fetch(key, () => src.history(tf));
    if (!mounted || gen != _gen) return;
    if (fresh.isNotEmpty) {
      // a forming bar that arrived meanwhile and the older pages scrolled back to are kept
      final m = mergeHistory(_data, fresh);
      final tail = m.tail;
      _data = m.bars;
      _cache.put(key, _data);
      if (tail == null) {
        _c.send(ChartCmd.bars(_data.map(_bar).toList()));
      } else if (tail.isNotEmpty) {
        // the cache lined up: only its last bar and the newer ones, in place (the view stays)
        _c.send(ChartCmd.tail(tail.map(_bar).toList()));
      }
    }
    if (!live) _liveBars(src, tf, key, gen);
    // the strip's neighbours of this timeframe, so the likely next tap draws at once
    if (src.prefetch) {
      for (final n in neighbourTimeframes(tf, kTimeframes)) {
        unawaited(_cache.prefetch(ChartHistoryCache.keyOf(src.key, n), () => src.history(n)));
      }
    }
  }

  /// The forming bar from the stream: every update goes to the chart (coalesced to one call per frame by the
  /// surface) and into the session cache. When a bar closes, its stored values are taken a few seconds later; after
  /// the stream reconnected, the bars it missed.
  void _liveBars(ChartSource src, String tf, String key, int gen) {
    final b = src.bars(tf, (bar) {
      if (gen != _gen || _data.isEmpty) return;
      if (bar.t < _data.last.t) return;
      if (bar.t > _data.last.t) {
        _data.add(bar);
        _settle?.cancel();
        _settle = Timer(TerminalChart.settleDelay, () => unawaited(_refresh(src, tf, key, gen, count: 3)));
      } else {
        _data[_data.length - 1] = bar;
      }
      _cache.upsert(key, bar);
      _c.send(ChartCmd.bar(_bar(bar)));
    });
    if (b != null) _unsubs.add(b);
    final r = src.resyncs(() {
      if (gen == _gen) unawaited(_refresh(src, tf, key, gen));
    });
    if (r != null) _unsubs.add(r);
  }

  /// Brings what is drawn up to date with the server: the latest page after a reconnect (the bars missed while the
  /// stream was down, a stale last bar), the newest `count` bars after a bar closed (its final values).
  Future<void> _refresh(ChartSource src, String tf, String key, int gen, {int? count}) async {
    final List<Candle> fresh;
    try {
      fresh = count == null ? await src.history(tf) : await src.recent(tf, count);
    } catch (_) {
      return;
    }
    if (!mounted || gen != _gen) return;
    final next = refreshBars(_data, fresh, page: count == null);
    if (next == null) return;
    _data = next.bars;
    _cache.put(key, _data);
    final tail = next.tail;
    _c.send(tail != null ? ChartCmd.tail(tail.map(_bar).toList()) : ChartCmd.redraw(_data.map(_bar).toList()));
  }

  void _onEvent(ChartEvent e) {
    switch (e) {
      case ChartNeedsOlder(:final before):
        unawaited(_older(before));
      case ChartDragStarted():
        _c.holdLines = true;
      case ChartLineDragged(:final id, :final price):
        _c.holdLines = false;
        // an S / T handle dragged out (or tapped) of a position line: the stop line it makes is not given yet
        final t = context.t;
        final made = widget.lines.any((x) => x.id == id) ? null : lineForHandle(widget.lines, id, sl: t('chart.line.slHandle'), tp: t('chart.line.tpHandle'));
        final l = widget.lines.where((x) => x.id == id).firstOrNull ?? made;
        if (l == null) {
          _sendLines();
          break;
        }
        _holdTimer?.cancel();
        _fresh = made;
        // a position line does not move (its drag sets the SL or TP): only stops, orders and alerts stay put
        _held = l.kind == 'pos' ? null : (id, price);
        _sendLines();
        final r = widget.onLineDragged?.call(l, price);
        if (_held == null) break;
        // a lost answer never pins the line: the given lines win after a while at the latest
        _holdTimer = Timer(const Duration(seconds: 12), _release);
        if (r is Future) {
          // refused: back at once; done: until the confirmed line arrives (or briefly, if it already did)
          r.then((ok) {
            if (!mounted || _held?.$1 != id) return;
            _holdTimer?.cancel();
            _holdTimer = ok == false ? null : Timer(const Duration(milliseconds: 1500), _release);
            if (ok == false) _release();
          }, onError: (Object _) => _release()).ignore();
        } else {
          WidgetsBinding.instance.addPostFrameCallback((_) => _release());
        }
      case ChartLineTapped(:final id):
        _c.holdLines = false;
        final l = widget.lines.where((x) => x.id == id).firstOrNull;
        if (l != null) widget.onLineTapped?.call(l);
      case ChartLineClosed(:final id):
        _c.holdLines = false;
        final l = widget.lines.where((x) => x.id == id).firstOrNull;
        if (l != null) widget.onLineClosed?.call(l);
      case ChartLongPress(:final price):
        widget.onLongPress?.call(price);
      case ChartIndicatorTapped(:final uid):
        widget.onIndicatorTapped?.call(uid);
      case ChartReady():
        // a reloaded page: draw everything again
        unawaited(_load());
    }
  }

  Future<void> _older(int beforeChart) async {
    final gen = _gen;
    final src = _source;
    final older = await src.history(widget.tf, to: fromChartTime(beforeChart) - 1);
    if (!mounted || gen != _gen) return;
    final first = _data.isEmpty ? 0 : _data.first.t;
    final add = older.where((c) => c.t < first).toList();
    if (add.isNotEmpty) {
      _data = [...add, ..._data];
      _cache.put(ChartHistoryCache.keyOf(src.key, widget.tf), _data);
    }
    _c.send(ChartCmd.older(add.map(_bar).toList(), exhausted: add.isEmpty));
  }

  @override
  Widget build(BuildContext context) => chartSurface(_c, native: TerminalChart.forceNative);
}

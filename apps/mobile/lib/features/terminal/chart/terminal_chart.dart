// One chart window (web: components/chart/engine.ts + chart-view.tsx): candle history from market data (cached per
// symbol / timeframe so switching draws at once, then the tail is refreshed), the forming bar from the stream, the
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
import 'indicators.dart';

/// UTC unix seconds -> chart time (server time).
int toChartTime(int utc) => utc + serverOffsetSeconds(DateTime.fromMillisecondsSinceEpoch(utc * 1000, isUtc: true));

/// Chart time -> UTC unix seconds (DST-aware inverse).
int fromChartTime(int chart) {
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

  /// The forming bar; returns the unsubscribe (null: no live bars).
  void Function()? bars(String tf, void Function(Candle bar) onBar) => null;

  /// Bid / ask lines; returns the unsubscribe (null: none).
  void Function()? quotes(void Function(double bid, double ask) onQuote) => null;
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
  void Function()? bars(String tf, void Function(Candle bar) onBar) => feed.subscribeBars(symbol, tf, (Bar b) => onBar(Candle(b.t, b.o, b.h, b.l, b.c, b.v)));

  @override
  void Function()? quotes(void Function(double bid, double ask) onQuote) => feed.subscribe([symbol], (q) => onQuote(q.bid, q.ask));
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

  /// Widget tests and the web preview draw the native chart (no WebView there).
  static bool forceNative = false;

  @override
  ConsumerState<TerminalChart> createState() => _TerminalChartState();
}

class _TerminalChartState extends ConsumerState<TerminalChart> {
  static final Map<String, List<Candle>> _cache = {};
  final ChartSurfaceController _c = ChartSurfaceController();
  final List<void Function()> _unsubs = [];
  List<Candle> _data = [];
  int _gen = 0;
  bool _dark = true;
  ChartPalette? _palette;

  /// A dropped line kept at its drop price until the app's action finished: (id, price).
  (String, double)? _held;
  Timer? _holdTimer;

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
    final lines = h == null
        ? widget.lines
        : [
            for (final l in widget.lines) l.id == h.$1 ? l.at(h.$2) : l,
            if (add) f.at(h.$2),
          ];
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
    _c.dispose();
    super.dispose();
  }

  static ChartBar _bar(Candle c) => (t: toChartTime(c.t), o: c.o, h: c.h, l: c.l, c: c.c, v: c.v);

  Future<void> _load() async {
    final gen = ++_gen;
    // a fresh chart (another market, a reloaded page) has no drag in progress
    _c.holdLines = false;
    for (final u in _unsubs) {
      u();
    }
    _unsubs.clear();
    final src = _source;
    final tf = widget.tf;
    final key = '${src.key}|$tf';
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
    _sendLines();
    final cached = _cache[key];
    if (cached != null && cached.isNotEmpty) {
      _data = [...cached];
      _c.send(ChartCmd.bars(_data.map(_bar).toList()));
    }
    final q = src.quotes((bid, ask) {
      if (gen == _gen) _c.send(ChartCmd.quote(bid, ask));
    });
    if (q != null) _unsubs.add(q);
    final fresh = await src.history(tf);
    if (!mounted || gen != _gen) return;
    if (fresh.isNotEmpty) {
      // keep a forming bar that arrived while the request was in flight
      final last = _data.isNotEmpty ? _data.last : null;
      _data = [...fresh];
      if (last != null && last.t > _data.last.t) _data.add(last);
      _cache[key] = [..._data];
      _c.send(ChartCmd.bars(_data.map(_bar).toList()));
    }
    final b = src.bars(tf, (bar) {
      if (gen != _gen || _data.isEmpty) return;
      if (bar.t < _data.last.t) return;
      if (bar.t > _data.last.t) {
        _data.add(bar);
      } else {
        _data[_data.length - 1] = bar;
      }
      _c.send(ChartCmd.bar(_bar(bar)));
    });
    if (b != null) _unsubs.add(b);
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
        final made = widget.lines.any((x) => x.id == id) ? null : lineForHandle(widget.lines, id);
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
      _cache['${src.key}|${widget.tf}'] = [..._data];
    }
    _c.send(ChartCmd.older(add.map(_bar).toList(), exhausted: add.isEmpty));
  }

  @override
  Widget build(BuildContext context) => chartSurface(_c, native: TerminalChart.forceNative);
}

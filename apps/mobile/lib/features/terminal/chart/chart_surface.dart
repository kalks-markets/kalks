// Where the chart is drawn. On Android: the bundled lightweight-charts page in a WebView (chart_webview.dart). On the
// web preview and in widget tests: a small native painter that understands the same commands (chart_native.dart),
// because webview_flutter has no web implementation. Both take ChartCmd JSON and report ChartEvents.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import '../../../ui/ui.dart';
import 'chart_bridge.dart';
import 'chart_native.dart';
import 'chart_webview_stub.dart' if (dart.library.io) 'chart_webview.dart';

/// Sends commands to a surface and receives its events. Commands sent before the surface is ready are queued.
///
/// Live updates (quotes, the forming bar, trade lines) are delivered on the next animation frame: every tick that
/// arrives before that frame is coalesced (the last quote, the last version of each bar and the last set of lines
/// win), so the page gets at most one call per frame and a price never waits longer than one frame. Other commands
/// (init, history, indicators, chart type, colours) go out at once. While a trade line is dragged ([holdLines]) line
/// updates wait and the rest goes out every [dragInterval], so the dragged line stays under the finger.
class ChartSurfaceController {
  ChartSurfaceController({this.dragInterval = const Duration(milliseconds: 180)});

  /// How often live updates go out while a trade line is dragged (the finger's moves come first).
  final Duration dragInterval;

  final List<_Cmd> _queue = [];
  void Function(List<String> batch)? _sink;
  Timer? _timer;
  int? _frameId;
  bool _soon = false;
  bool _holdLines = false;
  bool _disposed = false;

  /// Gestures and requests from the chart.
  void Function(ChartEvent e)? onEvent;

  /// The last `init`, replayed to a surface that (re)attaches.
  String? lastInit;

  static final _typeRe = RegExp(r'^\{"type":"(\w+)"');
  static final _barRe = RegExp(r'^\{"type":"bar","bar":\[(-?\d+)');
  static const _live = {'quote', 'bar', 'lines'};
  static const _latestOnly = {'quote', 'lines', 'indicators', 'chartType', 'palette', 'inset'};

  void send(String json) {
    if (_disposed) return;
    final type = _typeRe.firstMatch(json)?.group(1) ?? '';
    int? key;
    if (type == 'init') {
      lastInit = json;
      _queue.clear();
    } else if (_latestOnly.contains(type)) {
      _queue.removeWhere((m) => m.type == type);
    } else if (type == 'bar') {
      key = int.tryParse(_barRe.firstMatch(json)?.group(1) ?? '');
      _queue.removeWhere((m) => m.type == 'bar' && m.key == key);
    }
    _queue.add(_Cmd(type, json, key));
    _schedule(now: !_live.contains(type));
  }

  /// Holds trade-line updates (a chip is being dragged); releasing delivers the latest lines.
  bool get holdLines => _holdLines;
  set holdLines(bool v) {
    if (_holdLines == v) return;
    _holdLines = v;
    if (!v && _queue.any((m) => m.type == 'lines')) _schedule(now: true);
  }

  /// Commands waiting for the next delivery (tests).
  @visibleForTesting
  List<String> get pending => [for (final m in _queue) m.json];

  /// Bridge calls made so far (tests, measurements).
  @visibleForTesting
  int deliveries = 0;

  void _schedule({required bool now}) {
    if (_sink == null) return;
    if (now) {
      if (_soon) return;
      _soon = true;
      scheduleMicrotask(_flush);
    } else if (_holdLines) {
      // while a chip is dragged the page gets fewer calls (the finger's moves come first)
      _timer ??= Timer(dragInterval, _flush);
    } else {
      // the next frame (scheduleFrameCallback also asks for one when the app is idle)
      _frameId ??= SchedulerBinding.instance.scheduleFrameCallback((_) {
        _frameId = null;
        _flush();
      });
    }
  }

  void _cancelFrame() {
    final id = _frameId;
    if (id == null) return;
    _frameId = null;
    SchedulerBinding.instance.cancelFrameCallbackWithId(id);
  }

  void _flush() {
    _soon = false;
    _timer?.cancel();
    _timer = null;
    _cancelFrame();
    final s = _sink;
    if (s == null || _queue.isEmpty) return;
    final out = <String>[];
    final keep = <_Cmd>[];
    for (final m in _queue) {
      if (_holdLines && m.type == 'lines') {
        keep.add(m);
      } else {
        out.add(m.json);
      }
    }
    _queue
      ..clear()
      ..addAll(keep);
    if (out.isNotEmpty) {
      deliveries++;
      s(out);
    }
  }

  /// Called by the surface when it can draw: the last init (if it is not queued) and everything queued, at once.
  void attach(void Function(List<String> batch) sink) {
    _sink = sink;
    _timer?.cancel();
    _timer = null;
    _cancelFrame();
    final out = [if (lastInit != null && (_queue.isEmpty || _queue.first.json != lastInit)) lastInit!, for (final m in _queue) m.json];
    _queue.clear();
    if (out.isNotEmpty) {
      deliveries++;
      sink(out);
    }
  }

  void detach() {
    _sink = null;
    _timer?.cancel();
    _timer = null;
    _cancelFrame();
  }

  /// The chart widget is gone: no more deliveries.
  void dispose() {
    detach();
    _disposed = true;
    _queue.clear();
  }

  void emit(ChartEvent e) => onEvent?.call(e);
}

class _Cmd {
  const _Cmd(this.type, this.json, this.key);
  final String type, json;
  final int? key;
}

/// The surface widget for this platform.
Widget chartSurface(ChartSurfaceController c, {bool native = false}) => native || kIsWeb ? NativeChartSurface(controller: c) : webViewChartSurface(c);

String _hex(Color c) {
  final v = c.toARGB32();
  return '#${(v & 0xFFFFFF).toRadixString(16).padLeft(6, '0')}';
}

String _rgba(Color c) => 'rgba(${(c.r * 255).round()},${(c.g * 255).round()},${(c.b * 255).round()},${c.a.toStringAsFixed(3)})';

/// The chart colours of the terminal theme (web --t-chart-bg, --t-grid, --k-*).
ChartPalette chartPalette(KTokens k) => ChartPalette(
  dark: k.dark,
  bg: k.dark ? _hex(k.bg) : '#ffffff',
  grid: k.dark ? 'rgba(255,255,255,0.035)' : 'rgba(15,15,20,0.055)',
  line: k.dark ? 'rgba(255,255,255,0.07)' : 'rgba(15,15,20,0.1)',
  up: _hex(k.up),
  down: _hex(k.down),
  gold: _hex(k.gold),
  warn: _hex(k.warn),
  ember: _hex(k.ember),
  fg: _hex(k.fg),
  fg2: _hex(k.fg2),
  fg3: _hex(k.fg3),
  label: k.dark ? '#26262e' : '#55555f',
  panel: _rgba(k.surface2.withValues(alpha: 1)),
  info: _hex(k.info),
);

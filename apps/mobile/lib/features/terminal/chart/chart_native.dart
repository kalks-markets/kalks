// A small native candle chart that understands the chart page's commands (chart_bridge.dart): used by the web
// preview (webview_flutter has no web implementation) and widget tests. Candles / bars / line / area + volume, the
// OHLC legend with a row per indicator (tap: the indicator menu; only moving averages are drawn here), the bid / ask
// lines, trade lines with their chips at the price scale (tap, ×,
// vertical drag, the P&L on them, a position's S / T handles), the Kalks K in the corner, horizontal pan, long press. The product chart on Android
// is the lightweight-charts page (chart_webview.dart).
import 'dart:convert';
import 'dart:math' as math;

import 'package:flutter/material.dart';

import '../../../ui/ui.dart';
import 'chart_bridge.dart';
import 'chart_surface.dart';

class NativeChartSurface extends StatefulWidget {
  const NativeChartSurface({super.key, required this.controller});
  final ChartSurfaceController controller;

  @override
  State<NativeChartSurface> createState() => _NativeChartSurfaceState();
}

class _Line {
  _Line(this.id, this.kind, this.price, this.label, this.side, this.drag, this.note, this.tone, this.close, {this.handles = '', this.pip = 0, this.gap = 0});
  final String id, kind, label;
  final double price;
  final String? side, note, tone;
  final bool drag, close;

  /// A position line's S / T handles (chart_bridge ChartLine.handles), with the pip size and stops level they use.
  final String handles;
  final double pip, gap;

  String get ref => id.substring(id.indexOf(':') + 1);
}

class _NativeChartSurfaceState extends State<NativeChartSurface> {
  List<List<double>> _bars = [];
  List<_Line> _lines = [];
  List<Map<String, dynamic>> _inds = [];
  String _type = 'candles';
  String _symbol = '', _tf = '';
  double _bid = 0, _ask = 0;
  int _digits = 2;
  Map<String, dynamic> _pal = const {};
  double _scroll = 0; // bars scrolled back from the latest
  bool _exhausted = false, _loadingOlder = false;
  String? _dragId;
  double? _dragPrice;
  _Geo? _geo;
  Map<String, dynamic> _texts = const {};

  /// An S / T handle being dragged out: its position line, the stop line under the finger (drawn with the given
  /// lines; the handle stays mounted until the finger lifts) and the finger's unclamped y.
  _Line? _hPos, _hLine;
  double _hY = 0;

  /// The lines drawn: the given ones and a stop being dragged out of a handle.
  List<_Line> get _shown => _hLine == null ? _lines : [..._lines, _hLine!];

  @override
  void initState() {
    super.initState();
    widget.controller.attach(_recvAll);
  }

  @override
  void didUpdateWidget(NativeChartSurface old) {
    super.didUpdateWidget(old);
    if (old.controller != widget.controller) {
      old.controller.detach();
      widget.controller.attach(_recvAll);
    }
  }

  @override
  void dispose() {
    widget.controller.detach();
    super.dispose();
  }

  static List<double> _row(Object? b) => [for (final x in (b is List ? b : const [])) (x as num).toDouble()];

  void _recvAll(List<String> batch) {
    for (final json in batch) {
      _recv((jsonDecode(json) as Map).cast<String, dynamic>());
    }
    if (mounted) setState(() {});
  }

  void _recv(Map<String, dynamic> m) {
    switch (m['type']) {
      case 'init':
        _digits = (m['digits'] as num?)?.toInt() ?? 2;
        _pal = (m['palette'] as Map?)?.cast<String, dynamic>() ?? const {};
        _symbol = '${m['symbol'] ?? ''}';
        _tf = '${m['tf'] ?? ''}';
        _inds = _instances(m['indicators']);
        _type = '${m['chartType'] ?? 'candles'}';
        _texts = (m['texts'] as Map?)?.cast<String, dynamic>() ?? const {};
        _bars = [];
        _lines = [];
        _scroll = 0;
      case 'bars':
        _bars = [for (final b in (m['bars'] as List? ?? const [])) _row(b)];
        _scroll = 0;
        _exhausted = false;
        _loadingOlder = false;
      case 'older':
        _loadingOlder = false;
        _exhausted = m['exhausted'] == true;
        final add = [for (final b in (m['bars'] as List? ?? const [])) _row(b)];
        if (_bars.isNotEmpty) _bars = [...add.where((b) => b[0] < _bars.first[0]), ..._bars];
      case 'bar':
        final b = _row(m['bar']);
        if (b.length < 6 || _bars.isEmpty) break;
        if (b[0] > _bars.last[0]) {
          _bars.add(b);
        } else if (b[0] == _bars.last[0]) {
          _bars[_bars.length - 1] = b;
        }
      case 'quote':
        _bid = (m['bid'] as num?)?.toDouble() ?? _bid;
        _ask = (m['ask'] as num?)?.toDouble() ?? _ask;
      case 'lines':
        _lines = [
          for (final l in (m['lines'] as List? ?? const []))
            if (l is Map)
              _Line(
                '${l['id']}',
                '${l['kind']}',
                (l['price'] as num).toDouble(),
                '${l['label']}',
                l['side'] as String?,
                l['drag'] == true,
                l['note'] as String?,
                l['tone'] as String?,
                l['close'] == true,
                handles: '${l['handles'] ?? ''}',
                pip: (l['pip'] as num?)?.toDouble() ?? 0,
                gap: (l['gap'] as num?)?.toDouble() ?? 0,
              ),
        ];
      case 'palette':
        _pal = (m['palette'] as Map?)?.cast<String, dynamic>() ?? _pal;
      case 'indicators':
        _inds = _instances(m['indicators']);
      case 'chartType':
        _type = '${m['chartType'] ?? 'candles'}';
    }
  }

  static List<Map<String, dynamic>> _instances(Object? list) => [
    for (final i in (list is List ? list : const []))
      if (i is Map) i.cast<String, dynamic>(),
  ];

  Color _c(String key, Color fallback) {
    final v = _pal[key];
    if (v is! String) return fallback;
    if (v.startsWith('#') && v.length == 7) return Color(0xFF000000 | int.parse(v.substring(1), radix: 16));
    final m = RegExp(r'rgba\((\d+),(\d+),(\d+),([\d.]+)\)').firstMatch(v.replaceAll(' ', ''));
    if (m != null) return Color.fromRGBO(int.parse(m[1]!), int.parse(m[2]!), int.parse(m[3]!), double.parse(m[4]!));
    return fallback;
  }

  Color _lineColor(_Line l, KTokens k) => switch (l.kind) {
    'sl' => _c('down', k.down),
    'tp' => _c('up', k.up),
    'pending' => _c('gold', k.gold),
    'alert' || 'barrier' => _c('warn', k.warn),
    'strike' => _c('ember', k.ember),
    'breakeven' => _c('fg2', k.fg2),
    _ => l.side == 'buy' ? _c('up', k.up) : _c('down', k.down),
  };

  /// Simple / exponential moving average of the closes (null until enough bars). The stand-in draws only the
  /// moving averages; the chart page draws every indicator with the web's code.
  List<double?> _ma(Map<String, dynamic> inst) {
    final ema = inst['type'] == 'ema';
    final p = inst['params'];
    final n = (p is Map && p['period'] is num ? (p['period'] as num).toInt() : 20).clamp(1, 500);
    final out = List<double?>.filled(_bars.length, null);
    if (!ema) {
      var sum = 0.0;
      for (var i = 0; i < _bars.length; i++) {
        sum += _bars[i][4];
        if (i >= n) sum -= _bars[i - n][4];
        if (i >= n - 1) out[i] = sum / n;
      }
    } else {
      final k = 2 / (n + 1);
      double? prev;
      var sum = 0.0;
      for (var i = 0; i < _bars.length; i++) {
        final c = _bars[i][4];
        if (prev == null) {
          sum += c;
          if (i == n - 1) prev = sum / n;
          out[i] = prev;
        } else {
          prev = c * k + prev * (1 - k);
          out[i] = prev;
        }
      }
    }
    return out;
  }

  String _uid(Map<String, dynamic> i) => '${i['uid']}';

  bool _isMa(Map<String, dynamic> i) => i['visible'] != false && (i['type'] == 'sma' || i['type'] == 'ema');

  /// "EMA 50": the type and the numeric params (the app's menus show the registry's labels).
  String _label(Map<String, dynamic> i) {
    final p = i['params'];
    final nums = [
      if (p is Map)
        for (final v in p.values)
          if (v is num) v == v.roundToDouble() ? '${v.toInt()}' : '$v',
    ];
    return ['${i['type']}'.toUpperCase(), nums.join(', ')].where((x) => x.isNotEmpty).join(' ');
  }

  Color _indColor(Map<String, dynamic> i, KTokens k) {
    final st = i['style'];
    final tok = st is Map && st['ma'] is Map ? (st['ma'] as Map)['color'] : null;
    final t = tok is String ? tok : (i['type'] == 'ema' ? 'gold' : 'ember');
    return switch (t) {
      'gold' => _c('gold', k.gold),
      'up' => _c('up', k.up),
      'down' => _c('down', k.down),
      'warn' => _c('warn', k.warn),
      'info' => _c('info', k.info),
      'fg2' => _c('fg2', k.fg2),
      'fg3' => _c('fg3', k.fg3),
      _ => _c('ember', k.ember),
    };
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final mas = {for (final i in _inds.where(_isMa)) _uid(i): (_ma(i), _indColor(i, k))};
    return LayoutBuilder(
      builder: (context, c) {
        final geo = _Geo.of(this, c.biggest);
        _geo = geo;
        return GestureDetector(
          behavior: HitTestBehavior.opaque,
          onHorizontalDragUpdate: (d) => setState(() {
            _scroll = (_scroll + d.delta.dx / _Geo.spacing).clamp(0, math.max(0, _bars.length - 10).toDouble());
            if (!_exhausted && !_loadingOlder && _bars.isNotEmpty && _bars.length - _scroll - geo.visible < 30) {
              _loadingOlder = true;
              widget.controller.emit(ChartNeedsOlder(_bars.first[0].toInt()));
            }
          }),
          onLongPressStart: (d) {
            final p = geo.priceAt(d.localPosition.dy);
            if (p != null) widget.controller.emit(ChartLongPress(double.parse(p.toStringAsFixed(_digits))));
          },
          child: Stack(
            children: [
              Positioned.fill(child: CustomPaint(painter: _Painter(this, geo, k, mas))),
              Positioned(left: 10, bottom: 30, child: KLogoMark(size: 21, color: (k.dark ? Colors.white : Colors.black).withValues(alpha: 0.27))),
              Positioned(left: 8, top: 6, right: 80, child: _legend(k, mas)),
              ..._chips(geo, k),
            ],
          ),
        );
      },
    );
  }

  Widget _legend(KTokens k, Map<String, (List<double?>, Color)> mas) {
    if (_bars.isEmpty) return const SizedBox.shrink();
    final b = _bars.last;
    final col = b[4] >= b[1] ? _c('up', k.up) : _c('down', k.down);
    final fg3 = _c('fg3', k.fg3);
    TextStyle st(Color c, {FontWeight w = FontWeight.w500}) => TextStyle(fontFamily: KFonts.mono, fontSize: 11, height: 1.45, fontWeight: w, color: c);
    InlineSpan v(String key, double x) => TextSpan(
      children: [
        TextSpan(text: '$key ', style: st(fg3)),
        TextSpan(text: '${x.toStringAsFixed(_digits)}  ', style: st(col)),
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text.rich(
          TextSpan(
            children: [
              TextSpan(
                text: '$_symbol, $_tf  ',
                style: st(_c('fg', k.fg), w: FontWeight.w600),
              ),
              v('O', b[1]),
              v('H', b[2]),
              v('L', b[3]),
              v('C', b[4]),
            ],
          ),
        ),
        for (final i in _inds)
          GestureDetector(
            key: ValueKey('chart-legend-${_uid(i)}'),
            behavior: HitTestBehavior.opaque,
            onTap: () => widget.controller.emit(ChartIndicatorTapped(_uid(i))),
            child: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: '${_label(i)}  ',
                    style: st(i['visible'] == false ? fg3.withValues(alpha: 0.6) : _c('fg2', k.fg2))
                        .copyWith(decoration: i['visible'] == false ? TextDecoration.lineThrough : null),
                  ),
                  if (mas[_uid(i)] case final m?) TextSpan(text: m.$1.lastOrNull?.toStringAsFixed(_digits) ?? '—', style: st(m.$2)),
                ],
              ),
            ),
          ),
      ],
    );
  }

  /// The chips at the price scale; chips of nearby prices line up side by side to the left (like the chart page).
  List<Widget> _chips(_Geo geo, KTokens k) {
    final placed = <(_Line, double)>[];
    for (final l in _shown) {
      final y = geo.y(_dragId == l.id ? (_dragPrice ?? l.price) : l.price);
      if (y != null && y >= 8 && y <= geo.plotH - 4) placed.add((l, y));
    }
    placed.sort((a, b) => a.$2.compareTo(b.$2));
    final out = <Widget>[];
    var rowY = -1000.0;
    var right = geo.plotW - 6;
    for (final (l, y) in placed) {
      if (y - rowY >= 21) {
        rowY = y;
        right = geo.plotW - 6;
      }
      final w = _chipWidth(l);
      final left = math.max(4.0, right - w);
      out.add(_chip(l, y, left, k));
      right = left - 4;
    }
    return out;
  }

  double _chipWidth(_Line l) {
    final dragging = _dragId == l.id && l.kind != 'pos';
    var chars = l.label.length + (dragging ? _digits + 6 : 0);
    var w = chars * 6.4 + 12;
    if (l.drag) w += 14;
    if (l.note != null) {
      chars = l.note!.length;
      w += chars * 6.4 + 11;
    }
    final hs = _handlesOf(l);
    if (hs.isNotEmpty) w += 1 + 3 + hs.length * 17;
    if (l.close) w += 21;
    return w;
  }

  /// The S / T handles a position chip shows now: "sl" / "tp" while that stop is missing (a stop being made counts).
  List<String> _handlesOf(_Line l) => l.kind != 'pos' || l.handles.isEmpty
      ? const []
      : [
          for (final k in const ['sl', 'tp'])
            if (l.handles.contains(k[0]) && !_lines.any((x) => x.id == '$k:${l.ref}')) k,
        ];

  _Line _stopLine(_Line pos, String kind, double price) => _Line('$kind:${pos.ref}', kind, price, kind.toUpperCase(), pos.side, false, null, null, false);

  /// A handle tapped: the stop at the order tickets' starting distance (chart_bridge defaultStop), shown at once.
  void _tapHandle(_Line pos, String kind) {
    if (_bid <= 0 || _ask <= 0) return;
    final p = defaultStop(kind, pos.side ?? 'buy', bid: _bid, ask: _ask, pip: pos.pip, gap: pos.gap, digits: _digits);
    setState(() => _lines = [..._lines, _stopLine(pos, kind, p)]);
    widget.controller.emit(ChartLineDragged('$kind:${pos.ref}', p));
  }

  void _startHandle(_Line pos, String kind) {
    final g = _geo;
    if (g == null) return;
    final id = '$kind:${pos.ref}';
    _hPos = pos;
    _hY = g.y(pos.price) ?? 0;
    setState(() {
      _hLine = _stopLine(pos, kind, pos.price);
      _dragId = id;
      _dragPrice = pos.price;
    });
    widget.controller.emit(ChartDragStarted(id));
  }

  void _moveHandle(double dy) {
    final g = _geo, pos = _hPos, id = _dragId;
    if (g == null || pos == null || id == null) return;
    _hY += dy;
    final p = g.priceAt(_hY);
    if (p == null) return;
    final kind = id.substring(0, 2);
    setState(() => _dragPrice = clampStop(kind, pos.side ?? 'buy', p, bid: _bid, ask: _ask, gap: pos.gap, digits: _digits));
  }

  void _endHandle() {
    final id = _dragId, p = _dragPrice, pos = _hPos;
    _hPos = null;
    if (id == null || p == null || pos == null) return;
    setState(() {
      _hLine = null;
      _dragId = null;
      _dragPrice = null;
      // the new stop stays at the drop price until the app sends its lines
      _lines = [..._lines, _stopLine(pos, id.substring(0, 2), p)];
    });
    widget.controller.emit(ChartLineDragged(id, p));
  }

  Widget _chip(_Line l, double y, double left, KTokens k) {
    final price = _dragId == l.id ? (_dragPrice ?? l.price) : l.price;
    final col = _lineColor(l, k);
    final filled = l.kind != 'pos' && l.kind != 'breakeven';
    final warm = l.kind == 'pending' || l.kind == 'alert' || l.kind == 'barrier';
    final fg = warm ? (k.dark ? const Color(0xFF1A1204) : Colors.white) : (filled ? Colors.white : col);
    final noteColor = filled ? fg : (l.tone == 'up' ? _c('up', k.up) : (l.tone == 'down' ? _c('down', k.down) : _c('fg2', k.fg2)));
    final sep = Container(width: 1, height: 20, color: fg.withValues(alpha: 0.3));
    TextStyle st(Color c) => TextStyle(fontFamily: KFonts.mono, fontSize: 10.5, fontWeight: FontWeight.w500, color: c, height: 1);
    final hs = _handlesOf(l);
    Widget handle(String kind) {
      final c = kind == 'sl' ? _c('down', k.down) : _c('up', k.up);
      final pulled = _hLine?.id == '$kind:${l.ref}';
      return GestureDetector(
        key: ValueKey('chart-handle-$kind:${l.ref}'),
        behavior: HitTestBehavior.opaque,
        onTap: () => _tapHandle(l, kind),
        onVerticalDragStart: (_) => _startHandle(l, kind),
        onVerticalDragUpdate: (d) => _moveHandle(d.delta.dy),
        onVerticalDragEnd: (_) => _endHandle(),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 1.5, vertical: 3),
          child: Opacity(
            opacity: pulled ? 0.35 : 1,
            child: Container(
              width: 14,
              height: 14,
              alignment: Alignment.center,
              decoration: BoxDecoration(
                border: Border.all(color: c),
                borderRadius: BorderRadius.circular(3),
              ),
              child: Text('${_texts[kind] ?? kind[0].toUpperCase()}', style: st(c).copyWith(fontSize: 9, fontWeight: FontWeight.w700, fontFamily: KFonts.sans)),
            ),
          ),
        ),
      );
    }

    return Positioned(
      // keyed: chips reorder as lines move past each other (or a stop is pulled out of a handle) mid-gesture
      key: ValueKey('chart-chip-${l.id}'),
      left: left,
      top: y - 10,
      child: GestureDetector(
        onTap: () => widget.controller.emit(ChartLineTapped(l.id)),
        onVerticalDragStart: l.drag
            ? (_) {
                setState(() => _dragId = l.id);
                widget.controller.emit(ChartDragStarted(l.id));
              }
            : null,
        onVerticalDragUpdate: l.drag
            ? (d) {
                final g = _geo;
                if (g == null) return;
                final p = g.priceAt(g.y(_dragPrice ?? l.price)! + d.delta.dy);
                if (p != null) setState(() => _dragPrice = double.parse(p.toStringAsFixed(_digits)));
              }
            : null,
        onVerticalDragEnd: l.drag
            ? (_) {
                final p = _dragPrice;
                setState(() {
                  _dragId = null;
                  _dragPrice = null;
                });
                if (p != null) widget.controller.emit(ChartLineDragged(l.id, p));
              }
            : null,
        child: Container(
          height: 20,
          decoration: BoxDecoration(
            color: filled ? col : k.surface2,
            borderRadius: BorderRadius.circular(5),
            border: Border.all(color: col),
            boxShadow: const [BoxShadow(color: Color(0x59000000), blurRadius: 8, offset: Offset(0, 2))],
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (l.drag)
                Padding(
                  padding: const EdgeInsets.only(left: 4),
                  child: Text('⋮⋮', style: st(fg.withValues(alpha: 0.7))),
                ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 5),
                child: Text('${l.label}${_dragId == l.id && l.kind != 'pos' ? ' ${price.toStringAsFixed(_digits)}' : ''}', style: st(fg)),
              ),
              if (l.note != null) ...[
                sep,
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 5),
                  child: Text(l.note!, style: st(noteColor)),
                ),
              ],
              if (hs.isNotEmpty) ...[
                sep,
                Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 1.5),
                  child: Row(mainAxisSize: MainAxisSize.min, children: [for (final h in hs) handle(h)]),
                ),
              ],
              if (l.close) ...[
                sep,
                GestureDetector(
                  behavior: HitTestBehavior.opaque,
                  onTap: () => widget.controller.emit(ChartLineClosed(l.id)),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 6),
                    child: Text('×', style: st(fg).copyWith(fontSize: 12)),
                  ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

/// Layout of the visible window: price range, plot size, bar spacing.
class _Geo {
  _Geo(this.size, this.from, this.to, this.lo, this.hi);

  static const double spacing = 7, axisW = 60, timeH = 22, rightOffset = 8;
  final Size size;
  final int from, to;
  final double lo, hi;

  double get plotW => size.width - axisW;
  double get plotH => size.height - timeH;
  int get visible => to - from;

  double? y(double price) => hi <= lo ? null : plotH * (0.12 + 0.74 * (1 - (price - lo) / (hi - lo)));

  double? priceAt(double y) => hi <= lo ? null : lo + (1 - (y / plotH - 0.12) / 0.74) * (hi - lo);

  static _Geo of(_NativeChartSurfaceState s, Size size) {
    final n = s._bars.length;
    final count = ((size.width - axisW) / spacing - rightOffset).floor().clamp(5, 2000);
    final to = (n - s._scroll.round()).clamp(0, n);
    final from = math.max(0, to - count);
    var lo = double.infinity, hi = -double.infinity;
    for (var i = from; i < to; i++) {
      lo = math.min(lo, s._bars[i][3]);
      hi = math.max(hi, s._bars[i][2]);
    }
    if (s._bid > 0) {
      lo = math.min(lo, s._bid);
      hi = math.max(hi, s._ask);
    }
    if (!lo.isFinite || !hi.isFinite) {
      lo = 0;
      hi = 1;
    }
    if (hi - lo < 1e-9) {
      hi += 1;
      lo -= 1;
    }
    return _Geo(size, from, to, lo, hi);
  }
}

class _Painter extends CustomPainter {
  _Painter(this.s, this.g, this.k, this.mas);
  final _NativeChartSurfaceState s;
  final _Geo g;
  final KTokens k;
  final Map<String, (List<double?>, Color)> mas;

  @override
  void paint(Canvas canvas, Size size) {
    final bg = s._c('bg', k.dark ? const Color(0xFF0A0A0D) : Colors.white);
    final grid = s._c('grid', k.line);
    final line = s._c('line', k.line);
    final up = s._c('up', k.up), down = s._c('down', k.down), fg2 = s._c('fg2', k.fg2), fg3 = s._c('fg3', k.fg3);
    canvas.drawRect(Offset.zero & size, Paint()..color = bg);
    final gp = Paint()
      ..color = grid
      ..strokeWidth = 1;
    for (var i = 1; i < 6; i++) {
      final y = g.plotH * i / 6;
      canvas.drawLine(Offset(0, y), Offset(g.plotW, y), gp);
    }
    for (var i = 1; i < 5; i++) {
      final x = g.plotW * i / 5;
      canvas.drawLine(Offset(x, 0), Offset(x, g.plotH), gp);
    }
    final border = Paint()..color = line;
    canvas.drawLine(Offset(g.plotW, 0), Offset(g.plotW, size.height), border);
    canvas.drawLine(Offset(0, g.plotH), Offset(size.width, g.plotH), border);

    // volume, then candles, then the overlays
    var vmax = 1.0;
    for (var i = g.from; i < g.to; i++) {
      vmax = math.max(vmax, s._bars[i][5]);
    }
    final count = g.to - g.from;
    final xEnd = g.plotW - _Geo.rightOffset * _Geo.spacing;
    double xOf(int i) => xEnd - (count - (i - g.from) - 0.5) * _Geo.spacing;
    for (var i = g.from; i < g.to; i++) {
      final b = s._bars[i];
      final x = xOf(i);
      final isUp = b[4] >= b[1];
      final col = isUp ? up : down;
      final vh = b[5] / vmax * g.plotH * 0.14;
      canvas.drawRect(Rect.fromLTWH(x - 2.5, g.plotH - vh, 5, vh), Paint()..color = col.withValues(alpha: 0.22));
      if (s._type == 'line' || s._type == 'area') continue;
      final yo = g.y(b[1])!, yc = g.y(b[4])!, yh = g.y(b[2])!, yl = g.y(b[3])!;
      final p = Paint()
        ..color = col
        ..strokeWidth = 1;
      canvas.drawLine(Offset(x, yh), Offset(x, yl), p);
      if (s._type == 'bars') {
        canvas.drawLine(Offset(x - 2.5, yo), Offset(x, yo), p);
        canvas.drawLine(Offset(x, yc), Offset(x + 2.5, yc), p);
      } else {
        canvas.drawRect(Rect.fromLTRB(x - 2.5, math.min(yo, yc), x + 2.5, math.max(math.max(yo, yc), math.min(yo, yc) + 1)), p);
      }
    }
    if ((s._type == 'line' || s._type == 'area') && g.to > g.from) {
      final path = Path();
      for (var i = g.from; i < g.to; i++) {
        final pt = Offset(xOf(i), g.y(s._bars[i][4])!);
        if (i == g.from) {
          path.moveTo(pt.dx, pt.dy);
        } else {
          path.lineTo(pt.dx, pt.dy);
        }
      }
      final gold = s._c('gold', k.gold);
      if (s._type == 'area') {
        final fill = Path.from(path)
          ..lineTo(xOf(g.to - 1), g.plotH)
          ..lineTo(xOf(g.from), g.plotH)
          ..close();
        canvas.drawPath(fill, Paint()..color = gold.withValues(alpha: 0.16));
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = gold
          ..strokeWidth = 2
          ..style = PaintingStyle.stroke,
      );
    }
    for (final e in mas.entries) {
      final path = Path();
      var started = false;
      for (var i = g.from; i < g.to; i++) {
        final v = e.value.$1[i];
        if (v == null) continue;
        final pt = Offset(xOf(i), g.y(v)!);
        if (!started) {
          path.moveTo(pt.dx, pt.dy);
          started = true;
        } else {
          path.lineTo(pt.dx, pt.dy);
        }
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = e.value.$2
          ..strokeWidth = 2
          ..style = PaintingStyle.stroke,
      );
    }

    void hline(double price, Color c, {bool dashed = true, bool label = true, Color text = Colors.white}) {
      final y = g.y(price);
      if (y == null || y < 0 || y > g.plotH) return;
      final paint = Paint()
        ..color = c
        ..strokeWidth = 1;
      if (dashed) {
        for (var x = 0.0; x < g.plotW; x += 6) {
          canvas.drawLine(Offset(x, y), Offset(math.min(x + 3, g.plotW), y), paint);
        }
      } else {
        canvas.drawLine(Offset(0, y), Offset(g.plotW, y), paint);
      }
      if (!label) return;
      final tp = TextPainter(
        text: TextSpan(
          text: price.toStringAsFixed(s._digits),
          style: TextStyle(fontFamily: KFonts.mono, fontSize: 10, color: text),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      canvas.drawRect(Rect.fromLTWH(g.plotW, y - 8, _Geo.axisW, 16), Paint()..color = c);
      tp.paint(canvas, Offset(g.plotW + 4, y - tp.height / 2));
    }

    // price axis labels
    for (var i = 1; i < 6; i++) {
      final y = g.plotH * i / 6;
      final p = g.priceAt(y);
      if (p == null) continue;
      final tp = TextPainter(
        text: TextSpan(
          text: p.toStringAsFixed(s._digits),
          style: TextStyle(fontFamily: KFonts.mono, fontSize: 10, color: fg3),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, Offset(g.plotW + 4, y - tp.height / 2));
    }
    for (final l in s._shown) {
      final price = s._dragId == l.id ? (s._dragPrice ?? l.price) : l.price;
      final c = s._lineColor(l, k);
      final warm = l.kind == 'pending' || l.kind == 'alert' || l.kind == 'barrier';
      hline(price, c, dashed: l.kind != 'pos', text: warm && k.dark ? const Color(0xFF1A1204) : Colors.white);
    }
    if (s._bid > 0) {
      hline(s._bid, fg2, text: k.dark ? const Color(0xFF0A0A0D) : Colors.white);
      // the ask is the buy price: the buy colour
      hline(s._ask, up);
    }
  }

  @override
  bool shouldRepaint(_Painter old) => true;
}

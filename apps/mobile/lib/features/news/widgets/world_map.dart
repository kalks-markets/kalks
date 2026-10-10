// The dotted world map with glowing news pins (packages/ui world-map.tsx WorldMap) and the market sessions bars
// (MarketSessions, server time GMT+3).
import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import 'world_dots.dart';

/// Country centroids (lon, lat) of the pins the web can show.
const Map<String, (double, double)> _centroids = {
  'us': (-98, 39), 'eu': (7.75, 48.6), 'gb': (-2, 54), 'de': (10, 51), 'jp': (138, 36), 'in': (79, 22), 'sa': (45, 24), 'ae': (54, 24), //
  'sg': (103.8, 1.35), 'br': (-51, -10), 'cn': (104, 35), 'au': (134, -25), 'ca': (-106, 56), 'ch': (8, 47), 'za': (24, -29), 'ru': (90, 60),
  'tr': (35, 39), 'ng': (8, 9), 'mx': (-102, 23), 'eg': (30, 26), 'vn': (106, 16), 'my': (102, 4), 'id': (118, -2), 'ph': (122, 12),
  'pk': (70, 30), 'ir': (53, 32), 'iq': (44, 33), 'sy': (38, 35), 'kp': (127, 40), 'kr': (128, 36), 'cu': (-79, 21.5), 'mm': (96, 21),
  'af': (66, 34), 'be': (4.5, 50.8), 'nl': (5.3, 52.1), 'fr': (2.3, 46.5), 'es': (-3.7, 40.2), 'it': (12.5, 42.8), 'pt': (-8, 39.6),
  'pl': (19, 52), 'ua': (31, 49), 'by': (28, 53.5), 'se': (15, 62), 'no': (9, 61), 'fi': (26, 64), 'dk': (9.5, 56), 'ie': (-8, 53.2),
  'at': (14.5, 47.5), 'gr': (22, 39), 'cy': (33, 35), 'il': (35, 31.4), 'jo': (36, 31), 'lb': (35.8, 33.9), 'kw': (47.6, 29.3),
  'qa': (51.2, 25.3), 'bh': (50.6, 26), 'om': (57, 21), 'ye': (48, 15.5), 'th': (101, 15), 'kh': (105, 12.5), 'la': (103, 18),
  'bd': (90, 24), 'lk': (80.7, 7.8), 'np': (84, 28), 'kz': (67, 48), 'uz': (64, 41), 'hk': (114.2, 22.3), 'tw': (121, 23.7),
  'nz': (174, -41), 'ar': (-64, -34), 'cl': (-71, -33), 'co': (-73, 4), 'pe': (-75, -9), 've': (-66, 7), 'ke': (38, 0.2), 'gh': (-1, 7.9),
  'et': (39, 8.6), 'tz': (35, -6), 'ma': (-6, 32), 'dz': (3, 28), 'tn': (9, 34), 'ci': (-5.5, 7.5), 'sn': (-14.5, 14.5), 'cm': (12, 5.7),
  'ug': (32.3, 1.4), 'zw': (29.8, -19), 'sd': (30, 15), 'ly': (17, 27), 'so': (46, 6), 'ss': (30, 7.5), 'cf': (21, 6.6), 'cd': (23, -3),
  'ro': (25, 46), 'hu': (19.5, 47.2), 'cz': (15.5, 49.8),
};

/// d3 geoEqualEarth with the map's scale and translate.
Offset projectEqualEarth(double lon, double lat) {
  const a1 = 1.340264, a2 = -0.081106, a3 = 0.000893, a4 = 0.003796;
  final m = math.sqrt(3) / 2;
  final lambda = lon * math.pi / 180, phi = lat * math.pi / 180;
  final l = math.asin(m * math.sin(phi)), l2 = l * l, l6 = l2 * l2 * l2;
  final x = lambda * math.cos(l) / (m * (a1 + 3 * a2 * l2 + l6 * (7 * a3 + 9 * a4 * l2)));
  final y = l * (a1 + a2 * l2 + l6 * (a3 + a4 * l2));
  return Offset(kWorldTx + kWorldScale * x, kWorldTy - kWorldScale * y);
}

List<Offset>? _dots;
List<Offset> _landDots() => _dots ??= () {
  final out = <Offset>[];
  for (var j = 0; j < kWorldRows.length; j++) {
    final y = kWorldStep / 2 + j * kWorldStep;
    var i = 0;
    var land = false;
    for (final run in kWorldRows[j].split('.')) {
      final n = int.parse(run, radix: 36);
      if (land) {
        for (var k = 0; k < n; k++) {
          out.add(Offset(kWorldStep / 2 + (i + k) * kWorldStep, y));
        }
      }
      i += n;
      land = !land;
    }
  }
  return out;
}();

class MapPin {
  const MapPin({required this.country, required this.count, required this.label, this.tone = 'ember'});
  final String country;
  final int count;
  final String label;

  /// ember | up | down
  final String tone;
}

class WorldMap extends StatefulWidget {
  const WorldMap({super.key, required this.pins, this.heat = const {}, this.onPin});
  final List<MapPin> pins;

  /// Country -> tone (-1..1).
  final Map<String, double> heat;
  final ValueChanged<MapPin>? onPin;

  @override
  State<WorldMap> createState() => _WorldMapState();
}

class _WorldMapState extends State<WorldMap> with SingleTickerProviderStateMixin {
  late final AnimationController _pulse = AnimationController(vsync: this, duration: const Duration(seconds: 3))..repeat();

  @override
  void dispose() {
    _pulse.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final pins = [
      for (final p in widget.pins)
        if (_centroids[p.country] != null) (pin: p, at: projectEqualEarth(_centroids[p.country]!.$1, _centroids[p.country]!.$2)),
    ];
    return AspectRatio(
      aspectRatio: kWorldW / kWorldH,
      child: LayoutBuilder(
        builder: (context, c) {
          final scale = c.maxWidth / kWorldW;
          return GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapUp: widget.onPin == null
                ? null
                : (d) {
                    final p = d.localPosition / scale;
                    ({MapPin pin, Offset at})? best;
                    var bestD = 30.0;
                    for (final x in pins) {
                      final dist = (x.at - p).distance;
                      if (dist < bestD) {
                        bestD = dist;
                        best = x;
                      }
                    }
                    if (best != null) widget.onPin!(best.pin);
                  },
            child: Directionality(
              textDirection: TextDirection.ltr,
              child: AnimatedBuilder(
                animation: _pulse,
                builder: (context, _) => CustomPaint(
                  size: Size(c.maxWidth, c.maxWidth * kWorldH / kWorldW),
                  painter: _MapPainter(
                    scale: scale,
                    dots: k.fg3.withValues(alpha: 0.55),
                    up: k.up,
                    down: k.down,
                    ember: k.ember,
                    surface: k.surface,
                    fg: k.fg,
                    heat: {
                      for (final e in widget.heat.entries)
                        if (_centroids[e.key] != null) projectEqualEarth(_centroids[e.key]!.$1, _centroids[e.key]!.$2): e.value,
                    },
                    pins: pins,
                    pulse: _pulse.value,
                    textStyle: context.text.mono(10, weight: FontWeight.w700, color: k.fg),
                  ),
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}

class _MapPainter extends CustomPainter {
  _MapPainter({
    required this.scale,
    required this.dots,
    required this.up,
    required this.down,
    required this.ember,
    required this.surface,
    required this.fg,
    required this.heat,
    required this.pins,
    required this.pulse,
    required this.textStyle,
  });
  final double scale;
  final Color dots, up, down, ember, surface, fg;
  final Map<Offset, double> heat;
  final List<({MapPin pin, Offset at})> pins;
  final double pulse;
  final TextStyle textStyle;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(scale);
    for (final e in heat.entries) {
      final c = e.value >= 0 ? up : down;
      final r = 40 + e.value.abs() * 18;
      canvas.drawCircle(e.key, r, Paint()..shader = ui.Gradient.radial(e.key, r, [c.withValues(alpha: 0.35), c.withValues(alpha: 0)]));
    }
    canvas.drawPoints(
      ui.PointMode.points,
      _landDots(),
      Paint()
        ..color = dots
        ..strokeWidth = 3.1
        ..strokeCap = StrokeCap.round,
    );
    // the glow breathes 18 -> 30 -> 18 over 3 s
    final glow = 18 + 12 * (1 - (2 * pulse - 1).abs());
    for (final p in pins) {
      canvas.drawCircle(p.at, glow, Paint()..shader = ui.Gradient.radial(p.at, glow, [kEmber.withValues(alpha: 0.55), kEmber.withValues(alpha: 0)]));
      canvas.drawCircle(p.at, 11, Paint()..color = surface);
      canvas.drawCircle(
        p.at,
        11,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5
          ..color = p.pin.tone == 'up' ? up : (p.pin.tone == 'down' ? down : ember),
      );
      final tp = TextPainter(
        text: TextSpan(text: '${p.pin.count}', style: textStyle),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, p.at - Offset(tp.width / 2, tp.height / 2));
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_MapPainter old) => true;
}

/* ------------------------------------------------------------------ market sessions */

/// Sydney · Tokyo · London · New York on a 24 h bar in server time (GMT+3), with "Open · 3h 12m left" / "Opens in …".
class MarketSessions extends StatefulWidget {
  const MarketSessions({super.key});

  @override
  State<MarketSessions> createState() => _MarketSessionsState();
}

class _MarketSessionsState extends State<MarketSessions> {
  Timer? _timer;
  static const _sessions = [
    (key: 'shell.sessions.sydney', open: 0, close: 9, flag: 'au'),
    (key: 'shell.sessions.tokyo', open: 3, close: 12, flag: 'jp'),
    (key: 'shell.sessions.london', open: 10, close: 19, flag: 'gb'),
    (key: 'shell.sessions.newYork', open: 15, close: 24, flag: 'us'),
  ];

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 1), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final now = DateTime.now().toUtc();
    final hh = (now.hour + 3) % 24;
    final h = hh + now.minute / 60 + now.second / 3600;
    String two(int n) => n.toString().padLeft(2, '0');
    final clock = '${two(hh)}:${two(now.minute)}:${two(now.second)}';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(
                t('shell.sessions.title'),
                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
              ),
            ),
            Text(
              'GMT+3 · $clock',
              textDirection: TextDirection.ltr,
              style: context.text.mono(11.5, color: k.fg2),
            ),
          ],
        ),
        const SizedBox(height: 10),
        for (final s in _sessions) ...[
          Builder(
            builder: (context) {
              final open = h >= s.open && h < s.close;
              final until = open ? s.close - h : (s.open - h + 24) % 24;
              final uh = until.floor();
              final um = ((until - uh) * 60).floor();
              return Padding(
                padding: const EdgeInsets.symmetric(vertical: 4),
                child: Row(
                  children: [
                    KFlag(s.flag, size: 16),
                    const SizedBox(width: 10),
                    SizedBox(
                      width: 68,
                      child: Text(
                        t(s.key),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.footnote.copyWith(color: k.fg2),
                      ),
                    ),
                    Expanded(
                      child: Directionality(
                        textDirection: TextDirection.ltr,
                        child: LayoutBuilder(
                          builder: (context, c) => Container(
                            height: 8,
                            decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(4)),
                            child: Stack(
                              children: [
                                Positioned(
                                  left: c.maxWidth * s.open / 24,
                                  width: c.maxWidth * (s.close - s.open) / 24,
                                  top: 0,
                                  bottom: 0,
                                  child: Container(
                                    decoration: BoxDecoration(
                                      borderRadius: BorderRadius.circular(4),
                                      color: open ? null : k.fg3.withValues(alpha: 0.3),
                                      gradient: open ? LinearGradient(colors: [k.ember.withValues(alpha: 0.6), k.ember]) : null,
                                      boxShadow: open ? [BoxShadow(color: k.ember.withValues(alpha: 0.5), blurRadius: 12)] : null,
                                    ),
                                  ),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                    const SizedBox(width: 10),
                    SizedBox(
                      width: 104,
                      child: Text(
                        open ? '● ${t('shell.sessions.openLeft', {'h': uh, 'm': um})}' : t('shell.sessions.opensIn', {'h': uh, 'm': um}),
                        textAlign: TextAlign.end,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.caption.copyWith(color: open ? k.up : k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                      ),
                    ),
                  ],
                ),
              );
            },
          ),
        ],
      ],
    );
  }
}

import 'package:flutter/material.dart';

import '../tokens.dart';
import 'page_hero.dart';

/// The ground behind every Client Area page. Dark (the Client Area since 2026-10-10): with a section [photo], that
/// photo blurred under a black wash (web html[data-photo] .k-backdrop: blur 70 px, saturate 1.35, 62 % black), so the
/// glass cards take its colour; without one, Vantablack with a soft orange bloom (web --k-bloom-a / -b). The light
/// theme keeps its pastel washes; the terminal gets its subtle ember glow (.t-backdrop).
class KBackdrop extends StatelessWidget {
  const KBackdrop({super.key, this.photo});

  /// The section's photo (KHeroPhoto), drawn from its tiny pre-blurred copy.
  final KHeroPhoto? photo;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final p = photo;
    if (p != null && k.dark && !k.trader) {
      return RepaintBoundary(
        child: Stack(
          fit: StackFit.expand,
          children: [
            const ColoredBox(color: Colors.black),
            Image.asset(p.blurred, fit: BoxFit.cover, excludeFromSemantics: true, gaplessPlayback: true),
            const ColoredBox(color: Color(0xA8000000)),
          ],
        ),
      );
    }
    return RepaintBoundary(
      child: CustomPaint(painter: _WashPainter(k), size: Size.infinite),
    );
  }
}

class _WashPainter extends CustomPainter {
  _WashPainter(this.k);
  final KTokens k;

  /// `radial-gradient(<rx>vw <ry>vh at <x>% <y>%, colour, transparent <stop>%)`
  void _wash(Canvas canvas, Size s, double rx, double ry, double x, double y, Color c, double stop) {
    final center = Offset(s.width * x, s.height * y);
    final r = Size(s.width * rx, s.height * ry);
    canvas.save();
    canvas.translate(center.dx, center.dy);
    canvas.scale(1, r.height / r.width);
    final paint = Paint()
      ..shader = RadialGradient(colors: [c, c.withValues(alpha: 0)], stops: [0, stop]).createShader(Rect.fromCircle(center: Offset.zero, radius: r.width));
    canvas.drawCircle(Offset.zero, r.width, paint);
    canvas.restore();
  }

  @override
  void paint(Canvas canvas, Size size) {
    canvas.drawRect(Offset.zero & size, Paint()..color = k.bg);
    if (k.trader) {
      _wash(canvas, size, 1.4, 0.6, 0.12, -0.12, k.wash1, 0.62);
      _wash(canvas, size, 1.0, 0.7, 1.0, 0.0, k.wash2, 0.6);
      return;
    }
    if (k.dark) {
      // Vantablack with the orange bloom at the top end and a faint amber one at the bottom start
      _wash(canvas, size, 0.9, 0.5, 0.95, -0.1, k.wash1, 0.7);
      _wash(canvas, size, 0.8, 0.45, -0.05, 1.05, k.wash2, 0.7);
      return;
    }
    _wash(canvas, size, 0.55, 0.48, 0.06, -0.06, k.wash1, 0.70);
    _wash(canvas, size, 0.48, 0.52, 1.00, 0.04, k.wash3, 0.72);
    _wash(canvas, size, 0.60, 0.56, 0.72, 1.04, k.wash2, 0.70);
    _wash(canvas, size, 0.42, 0.48, -0.04, 0.92, k.wash3, 0.72);
    _wash(canvas, size, 0.36, 0.36, 0.44, 0.46, k.wash1.withValues(alpha: k.wash1.a * 0.45), 0.75);
  }

  @override
  bool shouldRepaint(_WashPainter old) => old.k != k;
}

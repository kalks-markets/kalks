// The Client Area's card objects (founder 2026-10-10: "like the debit cards for accounts, visiting cards for masters
// and partners, education as books and certificates"), drawn after the web's components:
// - KDebitCard: a trading account as a debit card (web components/dashboard/home/accounts-panel.tsx AccountVisual):
//   orange live, black demo, gold prop; logo, badges, chip, contactless mark, the spaced login, equity / free margin /
//   leverage, the name and the balance;
// - KCardFace: an account type / live-demo / product choice as a card face (web trading/group-card.tsx CardFace);
// - KVisitingCard: copy masters, PAMM funds, MAM managers and the partner's own card (web visiting-card.tsx);
// - KBookCover and KCertificateCard: an Academy phase as a hardcover book, a certificate as the paper one (web
//   academy/live/book.tsx).
// Text sizes follow the card's own width (the web's container units), so a card reads the same at any size.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../tokens.dart';
import '../typography.dart';
import 'brand.dart';

/// Card finishes (web .k-acct-live / -demo / -prop and .k-face-*).
enum KCardFinish { live, demo, prop, standard, cent, pro, graphite, ecn, vip, options }

/// The finish of an account type's card (web faceOf).
KCardFinish finishOfGroup(String code, {bool options = false}) {
  if (options) return KCardFinish.options;
  if (code == 'cent') return KCardFinish.cent;
  if (code == 'pro') return KCardFinish.pro;
  if (code == 'pro-netting' || code.endsWith('netting')) return KCardFinish.graphite;
  if (code == 'ecn') return KCardFinish.ecn;
  if (code == 'vip' || code.startsWith('prop')) return KCardFinish.vip;
  return KCardFinish.standard;
}

/// CSS `linear-gradient(150deg, a, b p, c)` on a card.
LinearGradient _g150(List<Color> c, double mid) =>
    LinearGradient(begin: const Alignment(-0.62, -1), end: const Alignment(0.62, 1), colors: c, stops: [0, mid, 1]);

/// The finish's fill. Live and standard follow the broker's colour (Kalks orange for Kalks).
LinearGradient cardFinishGradient(KCardFinish f, Color ember) {
  final stock = ember == kEmber;
  final orange = stock
      ? const [Color(0xFFFF8A3D), Color(0xFFF2600C), Color(0xFFA63F04)]
      : [Color.lerp(ember, Colors.white, 0.18)!, ember, Color.lerp(ember, Colors.black, 0.42)!];
  return switch (f) {
    KCardFinish.live || KCardFinish.standard => _g150(orange, 0.47),
    KCardFinish.demo => _g150(const [Color(0xFF2A2A2E), Color(0xFF101012), Color(0xFF000000)], 0.55),
    KCardFinish.prop => _g150(const [Color(0xFFF3C870), Color(0xFFC8892B), Color(0xFF7A4A12)], 0.5),
    KCardFinish.vip => _g150(const [Color(0xFFF3C870), Color(0xFFC8892B), Color(0xFF6E420F)], 0.5),
    KCardFinish.cent => _g150(const [Color(0xFFE09A6A), Color(0xFFA8582C), Color(0xFF5A2A12)], 0.5),
    KCardFinish.pro => _g150(const [Color(0xFF2C2C31), Color(0xFF111113), Color(0xFF000000)], 0.55),
    KCardFinish.graphite => _g150(const [Color(0xFF5A5A62), Color(0xFF2C2C31), Color(0xFF131315)], 0.5),
    KCardFinish.ecn => _g150(const [Color(0xFFFF6A45), Color(0xFFC8321B), Color(0xFF5C1206)], 0.5),
    KCardFinish.options => _g150(const [Color(0xFF7A46B0), Color(0xFF3B1D62), Color(0xFF14081F)], 0.52),
  };
}

bool _darkFinish(KCardFinish f) => f == KCardFinish.demo || f == KCardFinish.pro || f == KCardFinish.graphite;

/// The finish's drop shadow (web .k-face / .k-acct-* box-shadow).
List<BoxShadow> _finishShadow(KCardFinish f, Color ember) => [
  BoxShadow(
    color: switch (f) {
      KCardFinish.live || KCardFinish.standard => ember.withValues(alpha: 0.42),
      KCardFinish.prop || KCardFinish.vip => const Color(0xE67A4A12),
      _ => const Color(0xE6000000),
    },
    offset: const Offset(0, 22),
    blurRadius: 44,
    spreadRadius: -26,
  ),
];

/// "10042817" -> "1004 2817" (web spacedLogin).
String spacedLogin(String login) => login.replaceAllMapped(RegExp(r'(\d{4})(?=\d)'), (m) => '${m[1]} ');

/// One card face: the finish, the ribbon, an inner hairline and the content, rounded.
class _Face extends StatelessWidget {
  const _Face({required this.finish, required this.radius, required this.child, this.ribbon = 1});
  final KCardFinish finish;
  final double radius;
  final Widget child;
  final double ribbon;

  @override
  Widget build(BuildContext context) {
    final e = context.k.ember;
    final r = BorderRadius.circular(radius);
    return DecoratedBox(
      decoration: BoxDecoration(borderRadius: r, boxShadow: _finishShadow(finish, e)),
      child: ClipRRect(
        borderRadius: r,
        child: Stack(
          fit: StackFit.expand,
          children: [
            DecoratedBox(decoration: BoxDecoration(gradient: cardFinishGradient(finish, e))),
            if (ribbon > 0)
              Opacity(
                opacity: ribbon,
                child: const CustomPaint(painter: KCardRibbon()),
              ),
            DecoratedBox(
              decoration: BoxDecoration(
                borderRadius: r,
                border: Border.all(color: Colors.white.withValues(alpha: _darkFinish(finish) ? 0.09 : 0.16), width: 0.8),
              ),
            ),
            child,
          ],
        ),
      ),
    );
  }
}

/// A trading account as a debit card (web AccountVisual).
class KDebitCard extends StatelessWidget {
  const KDebitCard({
    super.key,
    required this.login,
    required this.finish,
    required this.badges,
    required this.figures,
    required this.holder,
    required this.title,
    required this.balanceLabel,
    required this.balance,
    this.hidden = false,
  });

  final String login;
  final KCardFinish finish;

  /// The pills at the top end (OPTIONS, LIVE / DEMO / PROP).
  final List<String> badges;

  /// Three label / value pairs along the middle (equity, free margin, leverage).
  final List<(String, String)> figures;

  /// The small caps line over the title (the account's name, or "Trading account").
  final String holder;
  final String title;
  final String balanceLabel;
  final String balance;

  /// Amounts as dots (the "hide amounts" eye). The login and the leverage stay.
  final bool hidden;

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: 1.586,
      child: LayoutBuilder(
        builder: (context, c) {
          final cq = c.maxWidth / 100;
          double clamp(double lo, double v, double hi) => v.clamp(lo, hi).toDouble();
          const white = Colors.white;
          final caps = context.text.micro.copyWith(
            color: white.withValues(alpha: 0.68),
            fontSize: 9,
            fontWeight: FontWeight.w600,
            letterSpacing: 1.1,
            height: 1.2,
          );
          return _Face(
            finish: finish,
            radius: 22,
            child: Padding(
              padding: EdgeInsets.all(clamp(14, 5.5 * cq, 20)),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const KLogoMark(color: white),
                      const Spacer(),
                      for (final b in badges) ...[const SizedBox(width: 6), _Pill(b)],
                    ],
                  ),
                  SizedBox(height: clamp(8, 3.6 * cq, 14)),
                  Row(
                    children: [
                      KCardChip(width: clamp(28, 9.4 * cq, 34)),
                      const SizedBox(width: 10),
                      CustomPaint(
                        size: const Size(20, 20),
                        painter: KContactless(color: white.withValues(alpha: 0.78)),
                      ),
                    ],
                  ),
                  const Spacer(),
                  Text(
                    spacedLogin(login),
                    textDirection: TextDirection.ltr,
                    maxLines: 1,
                    style: context.text
                        .mono(clamp(15, 6 * cq, 22), weight: FontWeight.w600, color: white)
                        .copyWith(
                          letterSpacing: clamp(15, 6 * cq, 22) * 0.14,
                          height: 1.1,
                          shadows: const [Shadow(color: Color(0x2E000000), blurRadius: 8, offset: Offset(0, 1))],
                        ),
                  ),
                  SizedBox(height: clamp(6, 2.5 * cq, 10)),
                  Row(
                    children: [
                      for (final (i, f) in figures.indexed) ...[
                        if (i > 0) const SizedBox(width: 8),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(f.$1.toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: caps),
                              const SizedBox(height: 1),
                              Text(
                                f.$2,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                textDirection: TextDirection.ltr,
                                style: context.text.label.copyWith(
                                  color: white,
                                  fontSize: clamp(11, 3.6 * cq, 13.5),
                                  fontWeight: FontWeight.w600,
                                  fontFeatures: kTabular,
                                  height: 1.2,
                                ),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ],
                  ),
                  SizedBox(height: clamp(6, 2.5 * cq, 10)),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              holder.toUpperCase(),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: caps.copyWith(fontSize: 9.5, color: white.withValues(alpha: 0.72)),
                            ),
                            const SizedBox(height: 1),
                            Text(
                              title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.label.copyWith(color: white, fontSize: clamp(11.5, 3.8 * cq, 13.5), fontWeight: FontWeight.w600, height: 1.2),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 12),
                      Column(
                        crossAxisAlignment: CrossAxisAlignment.end,
                        children: [
                          Text(balanceLabel.toUpperCase(), style: caps.copyWith(fontSize: 9.5, color: white.withValues(alpha: 0.72))),
                          const SizedBox(height: 1),
                          Text(
                            hidden ? '••••••' : balance,
                            textDirection: TextDirection.ltr,
                            style: context.text.figure.copyWith(color: white, fontSize: clamp(13, 4.6 * cq, 17), fontWeight: FontWeight.w700, height: 1.15),
                          ),
                        ],
                      ),
                    ],
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

/// An account type, live / demo or product choice as a card face (web CardFace): logo, a badge, the chip, the name in
/// caps and two figures at the bottom; a white tick while selected.
class KCardFace extends StatelessWidget {
  const KCardFace({super.key, required this.finish, required this.name, this.badge, this.left, this.right, this.selected = false});
  final KCardFinish finish;
  final String name;
  final String? badge;
  final (String, String)? left;
  final (String, String)? right;
  final bool selected;

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: 1.586,
      child: LayoutBuilder(
        builder: (context, c) {
          final cq = c.maxWidth / 100;
          double clamp(double lo, double v, double hi) => v.clamp(lo, hi).toDouble();
          const white = Colors.white;
          final caps = context.text.micro.copyWith(
            color: white.withValues(alpha: 0.72),
            fontSize: 9,
            fontWeight: FontWeight.w600,
            letterSpacing: 1.1,
            height: 1.2,
          );
          final value = context.text.label.copyWith(
            color: white,
            fontSize: clamp(11.5, 4 * cq, 14),
            fontWeight: FontWeight.w600,
            fontFeatures: kTabular,
            height: 1.2,
          );
          return _Face(
            finish: finish,
            radius: 18,
            ribbon: 0.6,
            child: Padding(
              padding: EdgeInsets.all(clamp(14, 5 * cq, 20)),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const KLogoMark(size: 22, color: white),
                      const Spacer(),
                      if (badge != null) _Pill(badge!.toUpperCase()),
                      if (selected) ...[
                        const SizedBox(width: 6),
                        Container(
                          width: 24,
                          height: 24,
                          decoration: const BoxDecoration(color: white, shape: BoxShape.circle),
                          child: const Icon(LucideIcons.check, size: 14, color: Color(0xFF141416)),
                        ),
                      ],
                    ],
                  ),
                  SizedBox(height: clamp(8, 4 * cq, 14)),
                  Align(
                    alignment: AlignmentDirectional.centerStart,
                    child: KCardChip(width: clamp(26, 8.5 * cq, 30)),
                  ),
                  const Spacer(),
                  Text(
                    name.toUpperCase(),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.title1.copyWith(
                      color: white,
                      fontSize: clamp(17, 7.5 * cq, 26),
                      fontWeight: FontWeight.w600,
                      letterSpacing: clamp(17, 7.5 * cq, 26) * 0.06,
                      height: 1.1,
                      shadows: const [Shadow(color: Color(0x33000000), blurRadius: 10)],
                    ),
                  ),
                  if (left != null || right != null) ...[
                    SizedBox(height: clamp(6, 2.5 * cq, 10)),
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        Expanded(
                          child: left == null
                              ? const SizedBox.shrink()
                              : Column(
                                  crossAxisAlignment: CrossAxisAlignment.start,
                                  children: [
                                    Text(left!.$1.toUpperCase(), maxLines: 1, overflow: TextOverflow.ellipsis, style: caps),
                                    Text(left!.$2, maxLines: 1, overflow: TextOverflow.ellipsis, style: value),
                                  ],
                                ),
                        ),
                        if (right != null) ...[
                          const SizedBox(width: 12),
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.end,
                            children: [
                              Text(right!.$1.toUpperCase(), style: caps),
                              Text(right!.$2, textDirection: TextDirection.ltr, style: value),
                            ],
                          ),
                        ],
                      ],
                    ),
                  ],
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

/// A small pill on a card (LIVE, OPTIONS, a status).
class _Pill extends StatelessWidget {
  const _Pill(this.label);
  final String label;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 4),
    decoration: BoxDecoration(color: Colors.white.withValues(alpha: 0.2), borderRadius: BorderRadius.circular(12)),
    child: Text(
      label,
      maxLines: 1,
      style: context.text.micro.copyWith(color: Colors.white, fontSize: 10, fontWeight: FontWeight.w700, letterSpacing: 0.8, height: 1.1),
    ),
  );
}

/// The payment chip printed on a card (decorative).
class KCardChip extends StatelessWidget {
  const KCardChip({super.key, this.width = 34});
  final double width;

  @override
  Widget build(BuildContext context) => CustomPaint(size: Size(width, width * 0.76), painter: const _ChipPainter());
}

class _ChipPainter extends CustomPainter {
  const _ChipPainter();

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 40, size.height / 30);
    final rr = RRect.fromRectAndRadius(const Rect.fromLTWH(0.5, 0.5, 39, 29), const Radius.circular(6));
    canvas.drawRRect(
      rr.shift(const Offset(0, 1)),
      Paint()
        ..color = const Color(0x40000000)
        ..maskFilter = const MaskFilter.blur(BlurStyle.normal, 1),
    );
    canvas.drawRRect(
      rr,
      Paint()
        ..shader = const LinearGradient(
          begin: Alignment.topLeft,
          end: Alignment.bottomRight,
          colors: [Color(0xFFF6E3A8), Color(0xFFD4B062), Color(0xFFA8823C)],
        ).createShader(const Rect.fromLTWH(0, 0, 40, 30)),
    );
    canvas.drawRRect(
      rr,
      Paint()
        ..style = PaintingStyle.stroke
        ..strokeWidth = 1
        ..color = const Color(0x2E000000),
    );
    final line = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1
      ..color = const Color(0x7350370F);
    final p = Path()
      ..moveTo(0, 10)
      ..lineTo(13, 10)
      ..moveTo(0, 20)
      ..lineTo(13, 20)
      ..moveTo(27, 10)
      ..lineTo(40, 10)
      ..moveTo(27, 20)
      ..lineTo(40, 20)
      ..moveTo(13, 0)
      ..lineTo(13, 30)
      ..moveTo(27, 0)
      ..lineTo(27, 30)
      ..moveTo(13, 15)
      ..lineTo(27, 15);
    canvas.drawPath(p, line);
    canvas.restore();
  }

  @override
  bool shouldRepaint(_ChipPainter old) => false;
}

/// The contactless mark (three arcs), drawn in a 24-unit box like the web's SVG.
class KContactless extends CustomPainter {
  const KContactless({required this.color});
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 24, size.height / 24);
    final paint = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.8
      ..strokeCap = StrokeCap.round
      ..color = color;
    // M8.5 7.5a6 6 0 0 1 0 9   M12 5a9.5 9.5 0 0 1 0 14   M5 10a2.5 2.5 0 0 1 0 4
    canvas.drawArc(Rect.fromCircle(center: const Offset(4.5, 12), radius: 5.4), -0.98, 1.96, false, paint);
    canvas.drawArc(Rect.fromCircle(center: const Offset(4.5, 12), radius: 9.3), -0.86, 1.72, false, paint);
    canvas.drawArc(Rect.fromCircle(center: const Offset(3.5, 12), radius: 2.5), -0.93, 1.86, false, paint);
    canvas.restore();
  }

  @override
  bool shouldRepaint(KContactless old) => old.color != color;
}

/// The glossy white ribbon strokes over a card (web Ribbon SVG, viewBox 400 x 250).
class KCardRibbon extends CustomPainter {
  const KCardRibbon();

  @override
  void paint(Canvas canvas, Size size) {
    canvas.save();
    canvas.scale(size.width / 400, size.height / 250);
    Paint stroke(double w, List<Color> colors, List<double> stops, {Alignment begin = Alignment.centerLeft, Alignment end = Alignment.centerRight}) => Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = w
      ..strokeCap = StrokeCap.round
      ..shader = LinearGradient(begin: begin, end: end, colors: colors, stops: stops).createShader(const Rect.fromLTWH(0, 0, 400, 250));
    const white = Colors.white;
    final p1 = Path()
      ..moveTo(-20, 170)
      ..cubicTo(60, 120, 110, 210, 180, 150)
      ..cubicTo(250, 90, 290, 40, 330, 120)
      ..cubicTo(370, 200, 400, 190, 430, 90);
    canvas.drawPath(p1, stroke(26, [white.withValues(alpha: 0), white.withValues(alpha: 0.32), white.withValues(alpha: 0.04)], const [0, 0.45, 1]));
    final p2 = Path()
      ..moveTo(-30, 200)
      ..cubicTo(50, 160, 120, 240, 200, 180)
      ..cubicTo(280, 120, 300, 80, 350, 150)
      ..cubicTo(400, 220, 410, 200, 440, 130);
    canvas.drawPath(
      p2,
      stroke(
        12,
        [white.withValues(alpha: 0.04), white.withValues(alpha: 0.22), white.withValues(alpha: 0)],
        const [0, 0.6, 1],
        begin: Alignment.bottomLeft,
        end: Alignment.topRight,
      ),
    );
    final p3 = Path()
      ..moveTo(150, -20)
      ..cubicTo(170, 60, 120, 90, 160, 140)
      ..cubicTo(200, 190, 240, 170, 230, 260);
    canvas.drawPath(
      p3,
      stroke(
        40,
        [white.withValues(alpha: 0.02), white.withValues(alpha: 0.1), white.withValues(alpha: 0)],
        const [0, 0.6, 1],
        begin: Alignment.bottomLeft,
        end: Alignment.topRight,
      ),
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(KCardRibbon old) => false;
}

/* ------------------------------------------------------------------------------------------------ visiting cards */

/// Visiting-card stocks (web VcFinish): black (copy masters), ivory (PAMM funds), graphite (MAM managers), orange (the
/// partner).
enum KVisitingFinish { black, ivory, graphite, orange }

/// One figure along the bottom of a visiting card; `tone` up / down colours the value.
typedef KVcStat = ({String label, String value, String? tone});

/// The initials of a name for a monogram (web initials).
String monogram(String name) {
  final parts = name.replaceAll(RegExp(r'[^\p{L}\p{N} ]', unicode: true), ' ').trim().split(RegExp(r'\s+')).where((p) => p.isNotEmpty).toList();
  if (parts.isEmpty) return 'K';
  if (parts.length == 1) return parts.first.characters.take(2).toString().toUpperCase();
  return '${parts[0].characters.first}${parts[1].characters.first}'.toUpperCase();
}

/// A business card (3.5 x 2 in, web VisitingCard): a monogram, the kicker, the name in caps, a title line, a short
/// accent rule, badges, and up to four figures (or a [footer]) along the bottom, the K embossed in the corner.
class KVisitingCard extends StatelessWidget {
  const KVisitingCard({
    super.key,
    this.finish = KVisitingFinish.black,
    required this.name,
    this.title,
    this.kicker,
    this.corner,
    this.badges = const [],
    this.stats = const [],
    this.footer,
  });

  final KVisitingFinish finish;
  final String name;
  final String? title;
  final String? kicker;
  final Widget? corner;
  final List<String> badges;
  final List<KVcStat> stats;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final (List<Color> fill, Color fg, Color muted, Color rule, Color up, Color down) = switch (finish) {
      KVisitingFinish.black => (
        const [Color(0xFF1D1D21), Color(0xFF0C0C0E), Color(0xFF000000)],
        Colors.white,
        Colors.white.withValues(alpha: 0.55),
        kEmber,
        const Color(0xFF6EA8FF),
        const Color(0xFFFF6B5B),
      ),
      KVisitingFinish.ivory => (
        const [Color(0xFFFBF8F1), Color(0xFFF1EADB), Color(0xFFE6DCC7)],
        const Color(0xFF1B1712),
        const Color(0xFF1B1712).withValues(alpha: 0.55),
        kEmber,
        const Color(0xFF1A56D6),
        const Color(0xFFB3261E),
      ),
      KVisitingFinish.graphite => (
        const [Color(0xFF55555D), Color(0xFF2A2A2F), Color(0xFF141416)],
        Colors.white,
        Colors.white.withValues(alpha: 0.6),
        kGold,
        const Color(0xFF8CB8FF),
        const Color(0xFFFF8A7A),
      ),
      KVisitingFinish.orange => (
        const [Color(0xFFFF8A3D), Color(0xFFF2600C), Color(0xFFA63F04)],
        Colors.white,
        Colors.white.withValues(alpha: 0.75),
        Colors.white,
        Colors.white,
        Colors.white,
      ),
    };
    return AspectRatio(
      aspectRatio: 1.75,
      child: LayoutBuilder(
        builder: (context, c) {
          final cq = c.maxWidth / 100;
          double clamp(double lo, double v, double hi) => v.clamp(lo, hi).toDouble();
          final r = BorderRadius.circular(14);
          final ivory = finish == KVisitingFinish.ivory;
          return DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: r,
              boxShadow: const [BoxShadow(color: Color(0xE6000000), offset: Offset(0, 22), blurRadius: 44, spreadRadius: -26)],
            ),
            child: ClipRRect(
              borderRadius: r,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  DecoratedBox(decoration: BoxDecoration(gradient: _g150(fill, 0.55))),
                  // card stock: a soft light from the top
                  const DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: RadialGradient(center: Alignment(-0.7, -1), radius: 1.1, colors: [Color(0x24FFFFFF), Color(0x00FFFFFF)], stops: [0, 0.55]),
                    ),
                  ),
                  // the brand mark, embossed in the corner
                  PositionedDirectional(
                    end: -5 * cq,
                    bottom: -6 * cq,
                    child: Opacity(
                      opacity: 0.07,
                      child: KLogoMark(size: 34 * cq, color: fg),
                    ),
                  ),
                  DecoratedBox(
                    decoration: BoxDecoration(
                      borderRadius: r,
                      border: Border.all(color: (ivory ? Colors.black : Colors.white).withValues(alpha: 0.1), width: 0.8),
                    ),
                  ),
                  Padding(
                    padding: EdgeInsets.all(5.5 * cq),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Container(
                              width: 13 * cq,
                              height: 13 * cq,
                              alignment: Alignment.center,
                              decoration: BoxDecoration(
                                shape: BoxShape.circle,
                                border: Border.all(color: fg.withValues(alpha: finish == KVisitingFinish.orange ? 0.4 : 0.28), width: 1.5),
                              ),
                              child: Text(
                                monogram(name),
                                style: context.text.headline.copyWith(
                                  color: fg,
                                  fontSize: clamp(12, 4.6 * cq, 20),
                                  fontWeight: FontWeight.w600,
                                  letterSpacing: 0.6,
                                  height: 1,
                                ),
                              ),
                            ),
                            SizedBox(width: 3.5 * cq),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  if (kicker != null)
                                    Text(
                                      kicker!.toUpperCase(),
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: context.text.micro.copyWith(
                                        color: muted,
                                        fontSize: clamp(8, 2.5 * cq, 10.5),
                                        fontWeight: FontWeight.w700,
                                        letterSpacing: 2,
                                        height: 1.2,
                                      ),
                                    ),
                                  SizedBox(height: 0.6 * cq),
                                  Text(
                                    name.toUpperCase(),
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: context.text.title1.copyWith(
                                      color: fg,
                                      fontSize: clamp(15, 6 * cq, 24),
                                      fontWeight: FontWeight.w700,
                                      letterSpacing: 0.7,
                                      height: 1.05,
                                    ),
                                  ),
                                  if (title != null) ...[
                                    SizedBox(height: 0.8 * cq),
                                    Text(
                                      title!,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: context.text.footnote.copyWith(color: muted, fontSize: clamp(10.5, 3.3 * cq, 13.5)),
                                    ),
                                  ],
                                ],
                              ),
                            ),
                            if (corner != null) ...[const SizedBox(width: 8), corner!],
                          ],
                        ),
                        Padding(
                          padding: EdgeInsetsDirectional.only(start: 16.5 * cq, top: 2.6 * cq),
                          child: Container(
                            width: 9 * cq,
                            height: 2,
                            decoration: BoxDecoration(color: rule, borderRadius: BorderRadius.circular(1)),
                          ),
                        ),
                        if (badges.isNotEmpty)
                          Padding(
                            padding: EdgeInsetsDirectional.only(start: 16.5 * cq, top: 2.4 * cq),
                            child: Wrap(
                              spacing: 4,
                              runSpacing: 4,
                              children: [
                                for (final b in badges)
                                  Container(
                                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 2),
                                    decoration: BoxDecoration(
                                      color: ivory ? const Color(0x141B1712) : Colors.white.withValues(alpha: 0.14),
                                      borderRadius: BorderRadius.circular(10),
                                    ),
                                    child: Text(
                                      b.toUpperCase(),
                                      style: context.text.micro.copyWith(
                                        color: ivory ? const Color(0xBF1B1712) : Colors.white.withValues(alpha: 0.85),
                                        fontSize: clamp(8, 2.3 * cq, 10),
                                        fontWeight: FontWeight.w700,
                                        letterSpacing: 0.8,
                                      ),
                                    ),
                                  ),
                              ],
                            ),
                          ),
                        const Spacer(),
                        if (footer != null)
                          footer!
                        else if (stats.isNotEmpty)
                          Row(
                            crossAxisAlignment: CrossAxisAlignment.end,
                            children: [
                              for (final (i, s) in stats.indexed) ...[
                                if (i > 0) SizedBox(width: 3 * cq),
                                Expanded(
                                  child: Column(
                                    crossAxisAlignment: CrossAxisAlignment.start,
                                    children: [
                                      Text(
                                        s.value,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        textDirection: TextDirection.ltr,
                                        style: context.text.figure.copyWith(
                                          color: s.tone == 'up' ? up : (s.tone == 'down' ? down : fg),
                                          fontSize: clamp(12, 4.4 * cq, 18),
                                          fontWeight: FontWeight.w600,
                                          height: 1.15,
                                        ),
                                      ),
                                      SizedBox(height: 0.6 * cq),
                                      Text(
                                        s.label.toUpperCase(),
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: context.text.micro.copyWith(
                                          color: muted,
                                          fontSize: clamp(7.5, 2.3 * cq, 9.5),
                                          fontWeight: FontWeight.w600,
                                          letterSpacing: 1.2,
                                        ),
                                      ),
                                    ],
                                  ),
                                ),
                              ],
                            ],
                          ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

/* --------------------------------------------------------------------------------------------- books, certificates */

/// A phase of the Academy as a hardcover book (web BookCover + CoverFace): the page block at the fore-edge, the cloth
/// finish per level, the spine, a foil frame, the phase number, the title, the level and the minutes, a ribbon while
/// reading and a gold seal once certified.
class KBookCover extends StatelessWidget {
  const KBookCover({
    super.key,
    required this.finish,
    required this.series,
    required this.kicker,
    required this.number,
    required this.title,
    required this.level,
    required this.minutes,
    this.reading = false,
    this.done = false,
  });

  final KCardFinish finish;

  /// "Kalks Academy"
  final String series;

  /// "Phase 3" / "Elective"
  final String kicker;
  final int number;
  final String title;
  final String level;
  final String minutes;
  final bool reading;
  final bool done;

  @override
  Widget build(BuildContext context) {
    return AspectRatio(
      aspectRatio: 3 / 4 * 1.0,
      child: LayoutBuilder(
        builder: (context, c) {
          final cq = (c.maxWidth - 6) / 100;
          double clamp(double lo, double v, double hi) => v.clamp(lo, hi).toDouble();
          const white = Colors.white;
          const r = BorderRadiusDirectional.only(
            topStart: Radius.circular(4),
            bottomStart: Radius.circular(4),
            topEnd: Radius.circular(12),
            bottomEnd: Radius.circular(12),
          );
          final rr = r.resolve(Directionality.of(context));
          final cover = DecoratedBox(
            decoration: BoxDecoration(
              borderRadius: rr,
              boxShadow: const [BoxShadow(color: Color(0xD9000000), offset: Offset(0, 22), blurRadius: 44, spreadRadius: -26)],
            ),
            child: ClipRRect(
              borderRadius: rr,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  DecoratedBox(decoration: BoxDecoration(gradient: cardFinishGradient(finish, context.k.ember))),
                  // cloth light from the top
                  const DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: RadialGradient(center: Alignment(-0.4, -1), radius: 1.2, colors: [Color(0x38FFFFFF), Color(0x00FFFFFF)], stops: [0, 0.6]),
                    ),
                  ),
                  // spine and hinge
                  PositionedDirectional(
                    top: 0,
                    bottom: 0,
                    start: 0,
                    width: 8 * cq,
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        gradient: LinearGradient(
                          begin: AlignmentDirectional.centerStart.resolve(Directionality.of(context)),
                          end: AlignmentDirectional.centerEnd.resolve(Directionality.of(context)),
                          colors: const [Color(0x8C000000), Color(0x33000000), Color(0x00000000)],
                        ),
                      ),
                    ),
                  ),
                  PositionedDirectional(
                    top: 0,
                    bottom: 0,
                    start: 8 * cq,
                    width: 1,
                    child: const ColoredBox(color: Color(0x40FFFFFF)),
                  ),
                  // foil frame
                  PositionedDirectional(
                    top: 6 * cq,
                    bottom: 6 * cq,
                    start: 13 * cq,
                    end: 6 * cq,
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(5),
                        border: Border.all(color: white.withValues(alpha: 0.3)),
                      ),
                    ),
                  ),
                  // reading ribbon
                  if (reading)
                    PositionedDirectional(
                      top: 0,
                      end: 14 * cq,
                      width: 7 * cq,
                      height: 24 * cq,
                      child: ClipPath(
                        clipper: _RibbonClip(),
                        child: const ColoredBox(color: Color(0xE6FFFFFF)),
                      ),
                    ),
                  Padding(
                    padding: EdgeInsetsDirectional.fromSTEB(18 * cq, 11 * cq, 11 * cq, 11 * cq),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            KLogoMark(size: clamp(6, 4.4 * cq, 14), color: white),
                            SizedBox(width: 2 * cq),
                            Expanded(
                              child: Text(
                                series.toUpperCase(),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: context.text.micro.copyWith(
                                  color: white.withValues(alpha: 0.8),
                                  fontSize: clamp(4.5, 3.4 * cq, 11),
                                  fontWeight: FontWeight.w700,
                                  letterSpacing: 0.18 * clamp(4.5, 3.4 * cq, 11),
                                  height: 1.1,
                                ),
                              ),
                            ),
                          ],
                        ),
                        SizedBox(height: 7 * cq),
                        Text(
                          kicker.toUpperCase(),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.micro.copyWith(
                            color: white.withValues(alpha: 0.72),
                            fontSize: clamp(5, 3.6 * cq, 12),
                            fontWeight: FontWeight.w600,
                            letterSpacing: 0.14 * clamp(5, 3.6 * cq, 12),
                            height: 1.1,
                          ),
                        ),
                        Text(
                          number.toString().padLeft(2, '0'),
                          style: context.text.largeTitle.copyWith(
                            color: white.withValues(alpha: 0.95),
                            fontSize: clamp(18, 28 * cq, 92),
                            fontWeight: FontWeight.w300,
                            height: 0.95,
                            letterSpacing: -1.2 * cq,
                            fontFeatures: kTabular,
                          ),
                        ),
                        const Spacer(),
                        Flexible(
                          flex: 3,
                          child: Text(
                            title,
                            maxLines: 3,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.title1.copyWith(
                              color: white,
                              fontSize: clamp(7, 7.6 * cq, 24),
                              fontWeight: FontWeight.w600,
                              height: 1.1,
                              letterSpacing: -0.2,
                            ),
                          ),
                        ),
                        SizedBox(height: 3 * cq),
                        Container(
                          width: 12 * cq,
                          height: math.max(1.5, 0.8 * cq),
                          decoration: BoxDecoration(color: white.withValues(alpha: 0.7), borderRadius: BorderRadius.circular(2)),
                        ),
                        SizedBox(height: 3 * cq),
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                level.toUpperCase(),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: context.text.micro.copyWith(
                                  color: white.withValues(alpha: 0.78),
                                  fontSize: clamp(5, 3.5 * cq, 12),
                                  fontWeight: FontWeight.w600,
                                  letterSpacing: 0.1 * clamp(5, 3.5 * cq, 12),
                                  height: 1.1,
                                ),
                              ),
                            ),
                            Text(
                              minutes,
                              style: context.text.micro.copyWith(
                                color: white.withValues(alpha: 0.78),
                                fontSize: clamp(5, 3.5 * cq, 12),
                                fontWeight: FontWeight.w600,
                                fontFeatures: kTabular,
                                height: 1.1,
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                  // certified: a gold seal on the cover
                  if (done)
                    PositionedDirectional(
                      end: 9 * cq,
                      top: 34 * cq,
                      child: Container(
                        width: 17 * cq,
                        height: 17 * cq,
                        decoration: const BoxDecoration(
                          shape: BoxShape.circle,
                          gradient: RadialGradient(
                            center: Alignment(-0.3, -0.4),
                            colors: [Color(0xFFFFE7A3), Color(0xFFD7A23E), Color(0xFF8A5A12)],
                            stops: [0, 0.55, 1],
                          ),
                          boxShadow: [BoxShadow(color: Color(0x73000000), offset: Offset(0, 6), blurRadius: 16)],
                        ),
                        child: Icon(LucideIcons.badgeCheck, size: 9 * cq, color: const Color(0xFF4A3005)),
                      ),
                    ),
                ],
              ),
            ),
          );
          return Stack(
            children: [
              // the page block at the fore-edge and the foot
              PositionedDirectional(
                top: 6,
                bottom: 0,
                start: 6,
                end: 0,
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    borderRadius: rr,
                    gradient: const LinearGradient(colors: [Color(0xFFD8CFBD), Color(0xFFF4EFE5), Color(0xFFE7DFCF)], stops: [0, 0.35, 1]),
                  ),
                ),
              ),
              PositionedDirectional(top: 0, start: 0, end: 6, bottom: 6, child: cover),
            ],
          );
        },
      ),
    );
  }
}

class _RibbonClip extends CustomClipper<Path> {
  @override
  Path getClip(Size s) => Path()
    ..moveTo(0, 0)
    ..lineTo(s.width, 0)
    ..lineTo(s.width, s.height)
    ..lineTo(s.width / 2, s.height * 0.8)
    ..lineTo(0, s.height)
    ..close();

  @override
  bool shouldReclip(_RibbonClip old) => false;
}

/// A certificate as the paper one (web CertificateCard): cream sheet, gold double frame and corner diamonds, the
/// series, the title, the learner's name, what they completed, the date, the seal and the certificate ID.
class KCertificateCard extends StatelessWidget {
  const KCertificateCard({
    super.key,
    required this.series,
    required this.heading,
    required this.certifies,
    required this.name,
    required this.text,
    required this.date,
    required this.dateLabel,
    required this.code,
    required this.codeLabel,
  });

  final String series, heading, certifies, name, text, date, dateLabel, code, codeLabel;

  @override
  Widget build(BuildContext context) {
    const ink = Color(0xFF2B2216);
    const gold = Color(0xFFB8893A);
    return AspectRatio(
      aspectRatio: 1.414,
      child: LayoutBuilder(
        builder: (context, c) {
          final cq = c.maxWidth / 100;
          double clamp(double lo, double v, double hi) => v.clamp(lo, hi).toDouble();
          // the phone's serif (Noto Serif on Android); the app's fonts where there is none
          const serif = 'serif';
          final small = context.text.micro.copyWith(
            color: const Color(0xFF6B5A40),
            fontSize: clamp(6.5, 1.6 * cq, 9.5),
            letterSpacing: 1.4,
            fontWeight: FontWeight.w500,
          );
          return DecoratedBox(
            decoration: BoxDecoration(
              color: const Color(0xFFF6F1E6),
              borderRadius: BorderRadius.circular(8),
              boxShadow: const [BoxShadow(color: Color(0xF2000000), offset: Offset(0, 24), blurRadius: 50, spreadRadius: -28)],
            ),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(8),
              child: Stack(
                fit: StackFit.expand,
                children: [
                  const DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: RadialGradient(radius: 0.8, colors: [Color(0xFFFFFAF0), Color(0x00FFFAF0)], stops: [0, 0.7]),
                    ),
                  ),
                  Positioned.fill(
                    child: Padding(
                      padding: EdgeInsets.all(2.6 * cq),
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          border: Border.all(color: gold, width: math.max(1.5, 0.5 * cq)),
                          borderRadius: BorderRadius.circular(4),
                        ),
                      ),
                    ),
                  ),
                  Positioned.fill(
                    child: Padding(
                      padding: EdgeInsets.all(3.8 * cq),
                      child: DecoratedBox(
                        decoration: BoxDecoration(
                          border: Border.all(color: gold.withValues(alpha: 0.55)),
                          borderRadius: BorderRadius.circular(2),
                        ),
                      ),
                    ),
                  ),
                  for (final a in const [Alignment.topLeft, Alignment.topRight, Alignment.bottomLeft, Alignment.bottomRight])
                    Align(
                      alignment: a,
                      child: Padding(
                        padding: EdgeInsets.all(2.2 * cq),
                        child: Transform.rotate(
                          angle: math.pi / 4,
                          child: Container(
                            width: 3 * cq,
                            height: 3 * cq,
                            decoration: BoxDecoration(
                              color: const Color(0xFFF6F1E6),
                              border: Border.all(color: gold, width: math.max(1, 0.4 * cq)),
                            ),
                          ),
                        ),
                      ),
                    ),
                  Padding(
                    padding: EdgeInsets.fromLTRB(10 * cq, 8 * cq, 10 * cq, 7.5 * cq),
                    child: Column(
                      children: [
                        Row(
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            KLogoMark(size: clamp(9, 2.8 * cq, 12), color: ink),
                            SizedBox(width: 1.4 * cq),
                            Text(
                              series.toUpperCase(),
                              style: context.text.micro.copyWith(
                                color: const Color(0xFF7A5A22),
                                fontSize: clamp(7, 1.9 * cq, 11),
                                fontWeight: FontWeight.w700,
                                letterSpacing: 2.6,
                              ),
                            ),
                          ],
                        ),
                        SizedBox(height: 2.4 * cq),
                        Text(
                          heading,
                          textAlign: TextAlign.center,
                          maxLines: 1,
                          style: TextStyle(fontFamily: serif, fontFamilyFallback: KFonts.fallback, color: ink, fontSize: clamp(14, 5.2 * cq, 30), height: 1),
                        ),
                        SizedBox(height: 2.6 * cq),
                        Text(
                          certifies.toUpperCase(),
                          textAlign: TextAlign.center,
                          style: small.copyWith(fontSize: clamp(7, 1.8 * cq, 11)),
                        ),
                        SizedBox(height: 1.2 * cq),
                        Container(
                          padding: EdgeInsets.fromLTRB(3 * cq, 0, 3 * cq, 0.8 * cq),
                          decoration: BoxDecoration(
                            border: Border(bottom: BorderSide(color: gold.withValues(alpha: 0.6))),
                          ),
                          child: Text(
                            name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(
                              fontFamily: serif,
                              fontFamilyFallback: KFonts.fallback,
                              fontStyle: FontStyle.italic,
                              color: ink,
                              fontSize: clamp(15, 6 * cq, 34),
                              height: 1.15,
                            ),
                          ),
                        ),
                        SizedBox(height: 2 * cq),
                        Text(
                          text,
                          textAlign: TextAlign.center,
                          maxLines: 2,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.footnote.copyWith(color: const Color(0xFF4A3D2A), fontSize: clamp(8, 2.1 * cq, 12.5), height: 1.3),
                        ),
                        const Spacer(),
                        Row(
                          crossAxisAlignment: CrossAxisAlignment.end,
                          children: [
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Container(
                                    padding: EdgeInsets.only(bottom: 0.6 * cq),
                                    decoration: const BoxDecoration(
                                      border: Border(bottom: BorderSide(color: Color(0x662B2216))),
                                    ),
                                    child: Text(
                                      date,
                                      style: context.text.label.copyWith(
                                        color: ink,
                                        fontSize: clamp(8, 2.1 * cq, 12.5),
                                        fontWeight: FontWeight.w600,
                                        fontFeatures: kTabular,
                                      ),
                                    ),
                                  ),
                                  SizedBox(height: 0.6 * cq),
                                  Text(dateLabel.toUpperCase(), style: small),
                                ],
                              ),
                            ),
                            SizedBox(width: 2 * cq),
                            Container(
                              width: 13 * cq,
                              height: 13 * cq,
                              alignment: Alignment.center,
                              decoration: const BoxDecoration(
                                shape: BoxShape.circle,
                                gradient: RadialGradient(
                                  center: Alignment(-0.3, -0.4),
                                  colors: [Color(0xFFFFE7A3), Color(0xFFD7A23E), Color(0xFF8A5A12)],
                                  stops: [0, 0.55, 1],
                                ),
                                boxShadow: [BoxShadow(color: Color(0x735A3C0A), offset: Offset(0, 3), blurRadius: 10)],
                              ),
                              child: KLogoMark(size: 6 * cq, color: const Color(0xFF3D2705)),
                            ),
                            SizedBox(width: 2 * cq),
                            Expanded(
                              child: Column(
                                crossAxisAlignment: CrossAxisAlignment.end,
                                children: [
                                  Container(
                                    padding: EdgeInsets.only(bottom: 0.6 * cq),
                                    decoration: const BoxDecoration(
                                      border: Border(bottom: BorderSide(color: Color(0x662B2216))),
                                    ),
                                    child: Text(
                                      code,
                                      maxLines: 1,
                                      overflow: TextOverflow.ellipsis,
                                      style: context.text.label.copyWith(
                                        color: ink,
                                        fontSize: clamp(8, 2.1 * cq, 12.5),
                                        fontWeight: FontWeight.w600,
                                        fontFeatures: kTabular,
                                      ),
                                    ),
                                  ),
                                  SizedBox(height: 0.6 * cq),
                                  Text(codeLabel.toUpperCase(), style: small),
                                ],
                              ),
                            ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

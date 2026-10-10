// The growth pages' shared pieces (port of apps/crm/components/growth/ui.tsx, banner-slot.tsx and
// components/rewards/countdown.tsx): load fallback, quiet empty box, section title, status chips, rank badge, tier
// orb, day bars, the live account picker, the marketing banner slot, a boxed countdown, a paged row list and the CSV
// export of a table.
import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../core/files.dart';
import '../../../core/models/account.dart';
import '../../../data/client_data.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../rewards_api.dart';

/* ------------------------------------------------------------------ layout helpers */

/// Gap between page blocks.
const double kBlockGap = 16;

/// The web's .k-row: a rounded surface-2 tile.
class RowBox extends StatelessWidget {
  const RowBox({super.key, required this.child, this.padding = const EdgeInsets.symmetric(horizontal: 14, vertical: 12), this.color, this.border});
  final Widget child;
  final EdgeInsetsGeometry padding;
  final Color? color;
  final Color? border;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: padding,
      decoration: BoxDecoration(
        color: color ?? k.surface2,
        borderRadius: BorderRadius.circular(k.rowRadius),
        border: Border.all(color: border ?? Colors.transparent),
      ),
      child: child,
    );
  }
}

/// A small label over a figure inside a RowBox (web "k-row px-3 py-2.5" stat tiles).
class StatTile extends StatelessWidget {
  const StatTile({super.key, required this.label, required this.value, this.color, this.size = 18});
  final String label;
  final String value;
  final Color? color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return RowBox(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 11),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 5),
          FittedBox(
            fit: BoxFit.scaleDown,
            alignment: AlignmentDirectional.centerStart,
            child: Text(
              value,
              maxLines: 1,
              textDirection: TextDirection.ltr,
              style: context.text.figure.copyWith(fontSize: size, height: 1, color: color ?? k.fg),
            ),
          ),
        ],
      ),
    );
  }
}

/// A Row of equal tiles with gaps.
class TileRow extends StatelessWidget {
  const TileRow({super.key, required this.children, this.gap = 8});
  final List<Widget> children;
  final double gap;

  @override
  Widget build(BuildContext context) => IntrinsicHeight(
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (var i = 0; i < children.length; i++) ...[if (i > 0) SizedBox(width: gap), Expanded(child: children[i])],
      ],
    ),
  );
}

/// Children in a grid of `columns` (rows of equal-height tiles).
class TileGrid extends StatelessWidget {
  const TileGrid({super.key, required this.children, this.columns = 2, this.gap = 8});
  final List<Widget> children;
  final int columns;
  final double gap;

  @override
  Widget build(BuildContext context) {
    final rows = <Widget>[];
    for (var i = 0; i < children.length; i += columns) {
      if (i > 0) rows.add(SizedBox(height: gap));
      rows.add(
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (var j = 0; j < columns; j++) ...[
                if (j > 0) SizedBox(width: gap),
                Expanded(child: i + j < children.length ? children[i + j] : const SizedBox.shrink()),
              ],
            ],
          ),
        ),
      );
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: rows);
  }
}

/// The web's SectionTitle (h2 + text above a grid of cards).
class GrowthSectionTitle extends StatelessWidget {
  const GrowthSectionTitle({super.key, required this.title, this.text});
  final String title;
  final String? text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(title, style: context.text.title1.copyWith(fontSize: 19, fontWeight: FontWeight.w500)),
        if (text != null) ...[const SizedBox(height: 2), Text(text!, style: context.text.footnote.copyWith(color: context.k.fg3, fontSize: 13))],
      ],
    ),
  );
}

/// A rounded box with a dashed border (web "rounded-[16px] border border-dashed border-line").
class DashedBox extends StatelessWidget {
  const DashedBox({super.key, required this.child, this.padding = const EdgeInsets.fromLTRB(14, 12, 14, 14)});
  final Widget child;
  final EdgeInsetsGeometry padding;

  @override
  Widget build(BuildContext context) => CustomPaint(
    painter: _DashedBorder(color: context.k.line, radius: 16),
    child: Padding(padding: padding, child: child),
  );
}

/// Quiet inline empty state for a card body (dashed box, web CardEmpty).
class CardEmpty extends StatelessWidget {
  const CardEmpty({super.key, required this.title, this.text, this.child, this.minHeight});
  final String title;
  final String? text;
  final Widget? child;
  final double? minHeight;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return DashedBox(
      padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 26),
      child: ConstrainedBox(
        constraints: BoxConstraints(minHeight: minHeight == null ? 0 : minHeight! - 52, minWidth: double.infinity),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              title,
              textAlign: TextAlign.center,
              style: context.text.callout.copyWith(fontWeight: FontWeight.w600, color: k.fg2),
            ),
            if (text != null) ...[
              const SizedBox(height: 4),
              Text(
                text!,
                textAlign: TextAlign.center,
                style: context.text.footnote.copyWith(color: k.fg3),
              ),
            ],
            if (child != null) ...[const SizedBox(height: 14), child!],
          ],
        ),
      ),
    );
  }
}

class _DashedBorder extends CustomPainter {
  _DashedBorder({required this.color, required this.radius});
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final rrect = RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius)).deflate(0.5);
    final path = Path()..addRRect(rrect);
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    for (final m in path.computeMetrics()) {
      for (double d = 0; d < m.length; d += 9) {
        canvas.drawPath(m.extractPath(d, math.min(d + 5, m.length)), paint);
      }
    }
  }

  @override
  bool shouldRepaint(_DashedBorder old) => old.color != color;
}

/// A tinted callout row (web "rounded-[12px] border border-warn/25 bg-warn-soft px-3 py-2" lines).
class ToneLine extends StatelessWidget {
  const ToneLine({super.key, required this.text, required this.tone, this.icon});
  final String text;
  final KChipTone tone;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final (bg, fg, border) = context.k.chip(tone);
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: border),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(text, style: context.text.footnote.copyWith(color: tone == KChipTone.neutral ? context.k.fg2 : fg)),
          ),
          if (icon != null) ...[const SizedBox(width: 8), Icon(icon, size: 14, color: fg)],
        ],
      ),
    );
  }
}

/// A disabled "done" state button (web Button variant up-outline, disabled at full opacity): "Joined", "Claimed".
class DonePill extends StatelessWidget {
  const DonePill({super.key, required this.label, this.icon = LucideIcons.check, this.expand = false, this.height = 40});
  final String label;
  final IconData icon;
  final bool expand;
  final double height;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      height: height,
      width: expand ? double.infinity : null,
      padding: const EdgeInsets.symmetric(horizontal: 16),
      decoration: BoxDecoration(
        color: k.upSoft,
        borderRadius: BorderRadius.circular(height / 2),
        border: Border.all(color: k.up.withValues(alpha: 0.35)),
      ),
      child: Row(
        mainAxisSize: expand ? MainAxisSize.max : MainAxisSize.min,
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          Icon(icon, size: 16, color: k.up),
          const SizedBox(width: 7),
          Flexible(
            child: Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.headline.copyWith(fontSize: 14, color: k.up),
            ),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ chips */

const Map<String, (String, KChipTone)> _status = {
  'pending': ('rewards.status.pending', KChipTone.warn),
  'completed': ('rewards.status.completed', KChipTone.up),
  'failed': ('rewards.status.failed', KChipTone.down),
  'paid': ('rewards.status.paid', KChipTone.up),
  'accrued': ('rewards.status.accrued', KChipTone.info),
  'void': ('rewards.status.void', KChipTone.neutral),
  'voided': ('rewards.status.void', KChipTone.neutral),
  'active': ('rewards.status.active', KChipTone.up),
  'used': ('rewards.status.used', KChipTone.neutral),
  'expired': ('rewards.status.expired', KChipTone.neutral),
  'awaiting_deposit': ('rewards.status.awaitingDeposit', KChipTone.warn),
  'forfeited': ('rewards.status.forfeited', KChipTone.down),
  'cancelled': ('rewards.status.cancelled', KChipTone.neutral),
  'applied': ('rewards.status.applied', KChipTone.up),
  'blocked': ('rewards.status.blocked', KChipTone.down),
  'scheduled': ('rewards.status.upcoming', KChipTone.info),
  'running': ('rewards.status.live', KChipTone.ember),
  'ended': ('rewards.status.ended', KChipTone.neutral),
  'finalized': ('rewards.status.finalized', KChipTone.gold),
  'disqualified': ('rewards.status.disqualified', KChipTone.down),
};

/// The status chip of a growth record (web GrowthStatus).
class GrowthStatus extends StatelessWidget {
  const GrowthStatus(this.status, {super.key, this.dot = true});
  final String status;
  final bool dot;

  @override
  Widget build(BuildContext context) {
    final s = _status[status];
    return KChip(label: s != null ? context.t(s.$1) : titleCase(status), tone: s?.$2 ?? KChipTone.neutral, dot: dot, small: true);
  }
}

/// LIVE / DEMO (and OPTIONS) badges of a contest (web kindChip).
List<Widget> contestKindChips(BuildContext context, Contest c) {
  final t = context.t;
  return [
    if (c.live)
      KChip(label: t('rewards.contest.badge.live'), tone: KChipTone.ember, small: true)
    else
      KChip(label: t('rewards.contest.badge.demo'), tone: KChipTone.gold, small: true),
    if (c.options) KChip(label: t('rewards.contest.badge.options'), tone: KChipTone.info, small: true),
  ];
}

/* ------------------------------------------------------------------ ranks, tiers */

/// A round rank badge: gold, silver and bronze for the podium (web RankBadge).
class RankBadge extends StatelessWidget {
  const RankBadge({super.key, required this.rank, this.size = 30});
  final int? rank;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final r = rank;
    final (Color bg, Color border, Color fg) = switch (r) {
      null || 0 => (k.surface3, k.line, k.fg3),
      1 => (k.goldSoft, k.gold.withValues(alpha: 0.5), k.gold),
      2 => (
        k.dark ? Colors.white.withValues(alpha: 0.1) : const Color(0xFFEFF1F4),
        k.dark ? Colors.white.withValues(alpha: 0.25) : const Color(0xFFC7CCD4),
        k.fg,
      ),
      3 => (
        const Color(0xFFD98B4A).withValues(alpha: 0.15),
        const Color(0xFFD98B4A).withValues(alpha: 0.45),
        k.dark ? const Color(0xFFE8A06A) : const Color(0xFFB8682C),
      ),
      _ => (k.surface3, k.line, k.fg2),
    };
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: bg,
        shape: BoxShape.circle,
        border: Border.all(color: border),
      ),
      child: Text(
        r == null || r == 0 ? '—' : '$r',
        textDirection: TextDirection.ltr,
        style: context.text.caption.copyWith(
          fontSize: math.max(10, size * 0.4),
          fontWeight: r == null ? FontWeight.w400 : FontWeight.w700,
          color: fg,
          height: 1,
        ),
      ),
    );
  }
}

/// A static tier orb (bronze, silver, gold, platinum, diamond; other tiers in the brand colour).
class TierOrb extends StatelessWidget {
  const TierOrb({super.key, required this.tier, this.name, this.size = 40});
  final String tier;
  final String? name;
  final double size;

  static const Map<String, List<Color>> _styles = {
    'bronze': [Color(0xFFFFD9B8), Color(0xFFD98B4A), Color(0xFF8A4B1F)],
    'silver': [Color(0xFFFFFFFF), Color(0xFFC7CCD4), Color(0xFF7A818C)],
    'gold': [Color(0xFFFFF3C4), Color(0xFFE9B949), Color(0xFF9C6F14)],
    'platinum': [Color(0xFFFFFFFF), Color(0xFFB9C6CF), Color(0xFF55636E)],
    'diamond': [Color(0xFFFFFFFF), Color(0xFF9FE3F5), Color(0xFF2B7F97)],
  };

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final colors = _styles[tier.toLowerCase()] ?? [Color.lerp(k.ember, Colors.white, 0.75)!, k.ember2, Color.lerp(k.ember, Colors.black, 0.22)!];
    final letter = (name ?? tier).isEmpty ? '' : (name ?? tier).characters.first.toUpperCase();
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        gradient: RadialGradient(center: const Alignment(-0.4, -0.5), radius: 0.9, colors: colors, stops: const [0, 0.45, 1]),
      ),
      child: size < 18
          ? null
          : Text(
              letter,
              style: TextStyle(fontSize: size * 0.3, fontWeight: FontWeight.w800, color: Colors.black.withValues(alpha: 0.7), height: 1),
            ),
    );
  }
}

/* ------------------------------------------------------------------ charts */

/// The 30-day column chart: one bar per day, the touched (or last) day highlighted with its value (web DayBars).
class DayBars extends StatefulWidget {
  const DayBars({super.key, required this.labels, required this.values, required this.format, this.height = 200});
  final List<String> labels;
  final List<double> values;
  final String Function(double v) format;
  final double height;

  @override
  State<DayBars> createState() => _DayBarsState();
}

class _DayBarsState extends State<DayBars> {
  int? _hover;

  void _at(double dx, double w) {
    final n = widget.values.length;
    if (n == 0) return;
    var i = (dx / w * n).floor().clamp(0, n - 1);
    if (Directionality.of(context) == TextDirection.rtl) i = n - 1 - i;
    if (i != _hover) {
      KHaptics.selection();
      setState(() => _hover = i);
    }
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final v = widget.values;
    final maxV = v.fold<double>(0, math.max);
    final sel = _hover ?? v.length - 1;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          children: [
            Expanded(
              child: Text(sel >= 0 && sel < widget.labels.length ? widget.labels[sel] : '', style: context.text.footnote.copyWith(color: k.fg3)),
            ),
            Text(
              sel >= 0 && sel < v.length ? widget.format(v[sel]) : '',
              textDirection: TextDirection.ltr,
              style: context.text.footnote.copyWith(fontWeight: FontWeight.w600, color: k.fg, fontFeatures: kTabular),
            ),
          ],
        ),
        const SizedBox(height: 10),
        LayoutBuilder(
          builder: (context, c) => GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTapDown: (d) => _at(d.localPosition.dx, c.maxWidth),
            onHorizontalDragUpdate: (d) => _at(d.localPosition.dx, c.maxWidth),
            onHorizontalDragEnd: (_) => setState(() => _hover = null),
            child: SizedBox(
              height: widget.height,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  for (var i = 0; i < v.length; i++) ...[
                    if (i > 0) const SizedBox(width: 3),
                    Expanded(
                      child: FractionallySizedBox(
                        heightFactor: maxV > 0 ? math.max(v[i] > 0 ? 0.04 : 0.015, v[i] / maxV) : 0.015,
                        child: Container(
                          decoration: BoxDecoration(
                            color: i == sel ? k.ember : (v[i] > 0 ? k.surface3 : k.surface3.withValues(alpha: 0.6)),
                            borderRadius: const BorderRadius.vertical(top: Radius.circular(4)),
                          ),
                        ),
                      ),
                    ),
                  ],
                ],
              ),
            ),
          ),
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            Text(
              widget.labels.isEmpty ? '' : widget.labels.first,
              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
            const Spacer(),
            Text(
              widget.labels.isEmpty ? '' : widget.labels.last,
              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
          ],
        ),
      ],
    );
  }
}

/// Tiny bars of the last days, the last one highlighted (web MiniBars).
class MiniBars extends StatelessWidget {
  const MiniBars(this.values, {super.key, this.height = 24});
  final List<double> values;
  final double height;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final maxV = values.fold<double>(0, math.max);
    return SizedBox(
      height: height,
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          for (var i = 0; i < values.length; i++) ...[
            if (i > 0) const SizedBox(width: 3),
            SizedBox(
              width: 4,
              child: FractionallySizedBox(
                heightFactor: maxV > 0 ? math.max(0.12, values[i] / maxV) : 0.12,
                child: Container(
                  decoration: BoxDecoration(color: i == values.length - 1 ? k.gold : k.fg3.withValues(alpha: 0.35), borderRadius: BorderRadius.circular(2)),
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ countdown */

({int d, int h, int m, int s}) _parts(Duration left) {
  final x = left.isNegative ? 0 : left.inSeconds;
  return (d: x ~/ 86400, h: (x % 86400) ~/ 3600, m: (x % 3600) ~/ 60, s: x % 60);
}

String _two(int n) => n.toString().padLeft(2, '0');

/// The ticking Days / Hrs / Min / Sec boxes of a contest (web Countdown).
class CountdownBoxes extends StatefulWidget {
  const CountdownBoxes({super.key, required this.until});
  final DateTime until;

  @override
  State<CountdownBoxes> createState() => _CountdownBoxesState();
}

class _CountdownBoxesState extends State<CountdownBoxes> {
  Timer? _timer;

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
    final p = _parts(widget.until.difference(DateTime.now()));
    final cells = [
      (t('rewards.countdown.days'), p.d),
      (t('rewards.countdown.hours'), p.h),
      (t('rewards.countdown.minutes'), p.m),
      (t('rewards.countdown.seconds'), p.s),
    ];
    return Directionality(
      textDirection: TextDirection.ltr,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (var i = 0; i < cells.length; i++) ...[
            if (i > 0)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 4),
                child: Text(':', style: context.text.mono(15, color: k.fg3)),
              ),
            Container(
              constraints: const BoxConstraints(minWidth: 50),
              padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
              decoration: BoxDecoration(
                color: k.dark ? Colors.black.withValues(alpha: 0.35) : Colors.white.withValues(alpha: 0.7),
                borderRadius: BorderRadius.circular(14),
                border: Border.all(color: k.dark ? Colors.white.withValues(alpha: 0.1) : k.line),
              ),
              child: Column(
                children: [
                  Text(
                    _two(cells[i].$2),
                    style: context.text.mono(21, weight: FontWeight.w600, color: k.fg).copyWith(height: 1, fontFeatures: kTabular),
                  ),
                  const SizedBox(height: 4),
                  Text(cells[i].$1.toUpperCase(), style: context.text.micro.copyWith(fontSize: 9, color: k.fg3, letterSpacing: 0.6)),
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// The compact countdown "3d 04:12:33" (web Countdown compact).
class CompactCountdown extends StatefulWidget {
  const CompactCountdown({super.key, required this.until, this.style});
  final DateTime until;
  final TextStyle? style;

  @override
  State<CompactCountdown> createState() => _CompactCountdownState();
}

class _CompactCountdownState extends State<CompactCountdown> {
  Timer? _timer;

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
    final p = _parts(widget.until.difference(DateTime.now()));
    return Text(
      '${p.d}${context.t('rewards.countdown.dayShort')} ${_two(p.h)}:${_two(p.m)}:${_two(p.s)}',
      textDirection: TextDirection.ltr,
      style: (widget.style ?? context.text.mono(11.5, color: context.k.fg2)).copyWith(fontFeatures: kTabular),
    );
  }
}

/* ------------------------------------------------------------------ page shells */

/// Page header + banner + skeletons while the first load runs, or the "Rewards are unavailable" card (web PageFallback).
class GrowthFallback extends StatelessWidget {
  const GrowthFallback({
    super.key,
    required this.title,
    required this.subtitle,
    this.error,
    required this.onRetry,
    this.heights = const [300, 150, 150],
    this.banner = true,
    this.leading,
    this.hero,
  });
  final String title, subtitle;
  final Widget? leading;

  /// The page's photo hero (the section's first page): the title is on the photo, not in the page.
  final KPageHero? hero;
  final Object? error;
  final VoidCallback onRetry;
  final List<double> heights;
  final bool banner;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return KPageScroll(
      onRefresh: () async => onRetry(),
      hero: hero,
      children: [
        ?leading,
        if (hero == null) ...[KPageHeader(title: title, subtitle: Text(subtitle)), const SizedBox(height: 20)] else const SizedBox(height: 6),
        if (banner) const BannerSlot(placement: 'rewards'),
        if (error != null)
          KCard(
            child: KEmptyState(
              compact: true,
              art: KIllustrationName.connectionLost,
              title: t('rewards.load.unavailableTitle'),
              text: error == null ? t('rewards.load.unavailableText') : growthError(error!, t),
              action: KButton(label: t('common.retry'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onRetry),
            ),
          )
        else
          for (final h in heights) ...[KSkeleton(height: h, radius: 20), const SizedBox(height: kBlockGap)],
      ],
    );
  }
}

/// Scrolls the page until the widget with `key` is built, then brings it to the top (the web's #anchor links).
Future<void> scrollToKey(ScrollController c, GlobalKey key) async {
  for (var i = 0; i < 24 && key.currentContext == null && c.hasClients; i++) {
    final p = c.position;
    if (p.pixels >= p.maxScrollExtent) break;
    c.jumpTo(math.min(p.pixels + p.viewportDimension * 0.8, p.maxScrollExtent));
    await WidgetsBinding.instance.endOfFrame;
  }
  final ctx = key.currentContext;
  if (ctx != null && ctx.mounted) await Scrollable.ensureVisible(ctx, duration: const Duration(milliseconds: 380), curve: Curves.easeOutCubic, alignment: 0.02);
}

/* ------------------------------------------------------------------ paged rows */

/// A list of rows paged like the web's DataTable (`pageSize` per page, "1–10 / 34" with arrows when there is more).
class PagedRows<V> extends StatefulWidget {
  const PagedRows({super.key, required this.rows, required this.itemBuilder, this.pageSize = 10, this.empty, this.divided = true});
  final List<V> rows;
  final Widget Function(BuildContext context, V row) itemBuilder;
  final int pageSize;
  final Widget? empty;
  final bool divided;

  @override
  State<PagedRows<V>> createState() => _PagedRowsState<V>();
}

class _PagedRowsState<V> extends State<PagedRows<V>> {
  int _page = 0;

  @override
  void didUpdateWidget(PagedRows<V> old) {
    super.didUpdateWidget(old);
    final pages = math.max(1, (widget.rows.length / widget.pageSize).ceil());
    if (_page >= pages) _page = pages - 1;
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final rows = widget.rows;
    if (rows.isEmpty) return widget.empty ?? const SizedBox.shrink();
    final pages = math.max(1, (rows.length / widget.pageSize).ceil());
    final from = _page * widget.pageSize;
    final view = rows.sublist(from, math.min(rows.length, from + widget.pageSize));
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 0; i < view.length; i++) ...[if (i > 0 && widget.divided) const KDivider(), widget.itemBuilder(context, view[i])],
        if (pages > 1) ...[
          const SizedBox(height: 10),
          Row(
            children: [
              Expanded(
                child: Text(
                  '${from + 1}–${math.min(rows.length, from + widget.pageSize)} / ${rows.length}',
                  textDirection: TextDirection.ltr,
                  textAlign: rtl ? TextAlign.end : TextAlign.start,
                  style: context.text.footnote.copyWith(color: k.fg3, fontFeatures: kTabular),
                ),
              ),
              KIconButton(
                icon: rtl ? LucideIcons.chevronRight : LucideIcons.chevronLeft,
                size: 34,
                semanticLabel: '$_page',
                onPressed: _page == 0 ? null : () => setState(() => _page--),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 6),
                child: Text(
                  '${_page + 1} / $pages',
                  textDirection: TextDirection.ltr,
                  style: context.text.footnote.copyWith(color: k.fg3, fontFeatures: kTabular),
                ),
              ),
              KIconButton(
                icon: rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight,
                size: 34,
                semanticLabel: '${_page + 2}',
                onPressed: _page >= pages - 1 ? null : () => setState(() => _page++),
              ),
            ],
          ),
        ],
      ],
    );
  }
}

/// The DataTable's "CSV" export: the rows as a CSV file in the share sheet.
Future<void> shareCsv(BuildContext context, String name, List<String> header, List<List<Object?>> rows) async {
  String cell(Object? v) {
    final s = v == null ? '' : '$v';
    return RegExp(r'[",\n]').hasMatch(s) ? '"${s.replaceAll('"', '""')}"' : s;
  }

  final csv = [header.map(cell).join(','), for (final r in rows) r.map(cell).join(',')].join('\n');
  final bytes = Uint8List.fromList(utf8.encode(csv));
  try {
    await shareFile((bytes: bytes, fileName: '$name.csv', contentType: 'text/csv'), fallbackName: '$name.csv');
  } catch (_) {
    if (context.mounted) await kCopy(context, csv);
  }
}

/// The small "CSV" button of a table toolbar.
class CsvButton extends StatelessWidget {
  const CsvButton({super.key, required this.onPressed});
  final VoidCallback? onPressed;

  @override
  Widget build(BuildContext context) =>
      KButton(label: 'CSV', icon: LucideIcons.download, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onPressed);
}

/* ------------------------------------------------------------------ live account picker */

/// Choose one of the client's live accounts (contest entry, bonus claim, bonus-credit reward). Picks the only one by
/// itself; none -> "Open live account" (web LiveAccountPicker).
class LiveAccountPicker extends ConsumerStatefulWidget {
  const LiveAccountPicker({super.key, required this.value, required this.onChanged, this.filter, this.hint});
  final int? value;
  final ValueChanged<int> onChanged;
  final bool Function(EngineAccount a)? filter;
  final String? hint;

  @override
  ConsumerState<LiveAccountPicker> createState() => _LiveAccountPickerState();
}

class _LiveAccountPickerState extends ConsumerState<LiveAccountPicker> {
  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final acc = ref.watch(accountsProvider);
    if (!acc.hasValue && !acc.hasError) return const KSkeleton(height: 62, radius: 14);
    if (acc.hasError && !acc.hasValue) {
      return CardEmpty(
        title: t('rewards.picker.accountsUnavailable'),
        text: growthError(acc.error!, t),
        child: KButton(
          label: t('common.retry'),
          icon: LucideIcons.rotateCw,
          variant: KButtonVariant.surface,
          size: KButtonSize.sm,
          onPressed: () => ref.invalidate(accountsProvider),
        ),
      );
    }
    final live = [
      for (final a in acc.requireValue)
        if (a.live && !a.archived && (widget.filter == null || widget.filter!(a))) a,
    ];
    if (widget.value == null && live.length == 1) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && widget.value == null) widget.onChanged(live.first.login);
      });
    }
    if (live.isEmpty) {
      return CardEmpty(
        title: t('rewards.picker.noEligible'),
        text: widget.hint ?? t('rewards.picker.openFirst'),
        child: KButton(
          label: t('rewards.picker.openLive'),
          icon: LucideIcons.plus,
          variant: KButtonVariant.surface,
          size: KButtonSize.sm,
          onPressed: () {
            Navigator.of(context).maybePop();
            GoRouter.of(context).go('/accounts/new');
          },
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (final a in live) ...[
          if (a != live.first) const SizedBox(height: 6),
          Semantics(
            selected: a.login == widget.value,
            child: KPressable(
              pressedScale: 0.99,
              onTap: () {
                KHaptics.selection();
                widget.onChanged(a.login);
              },
              child: RowBox(
                color: a.login == widget.value ? k.emberSoft : null,
                border: a.login == widget.value ? k.ember.withValues(alpha: 0.5) : null,
                padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 10),
                child: Row(
                  children: [
                    KChip(label: t('accounts.badge.live'), tone: KChipTone.ember, small: true),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            '#${a.login}',
                            textDirection: TextDirection.ltr,
                            style: context.text.mono(13, weight: FontWeight.w600, color: k.fg),
                          ),
                          Text(
                            '${a.groupName} · ${t.dyn('accounts.mode.${a.mode}', fallback: a.mode)}',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                          ),
                        ],
                      ),
                    ),
                    Text(
                      a.money(a.equity),
                      textDirection: TextDirection.ltr,
                      style: context.text.footnote.copyWith(color: k.fg2, fontFeatures: kTabular),
                    ),
                    const SizedBox(width: 10),
                    Container(
                      width: 16,
                      height: 16,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        border: Border.all(color: a.login == widget.value ? k.ember : k.line),
                      ),
                      child: a.login == widget.value
                          ? Container(
                              width: 8,
                              height: 8,
                              decoration: BoxDecoration(color: k.ember, shape: BoxShape.circle),
                            )
                          : null,
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ],
    );
  }
}

/* ------------------------------------------------------------------ banners */

/// Targeted marketing banners (D121): nothing when the service is unavailable or no banner targets the client.
/// Impressions are counted once per banner while the slot lives (web BannerSlot).
class BannerSlot extends ConsumerStatefulWidget {
  const BannerSlot({super.key, required this.placement, this.max = 1});
  final String placement;
  final int max;

  @override
  ConsumerState<BannerSlot> createState() => _BannerSlotState();
}

class _BannerSlotState extends ConsumerState<BannerSlot> {
  final Set<int> _hidden = {};
  final Set<int> _seen = {};

  void _track(int id, String kind) {
    unawaited(growthPost(ref, 'banners/$id/events', {'kind': kind}).then((_) {}, onError: (Object _) {}));
  }

  void _open(BannerView b) {
    _track(b.id, 'click');
    final url = b.ctaUrl!;
    if (RegExp(r'^https?://', caseSensitive: false).hasMatch(url)) {
      final u = Uri.tryParse(url);
      if (u != null) unawaited(launchUrl(u, mode: LaunchMode.externalApplication));
    } else {
      context.go(url);
    }
  }

  @override
  Widget build(BuildContext context) {
    final items = (ref.watch(bannersProvider(widget.placement)).value ?? const <BannerView>[]).where((b) => !_hidden.contains(b.id)).take(widget.max).toList();
    if (items.isEmpty) return const SizedBox.shrink();
    for (final b in items) {
      if (_seen.add(b.id)) _track(b.id, 'impression');
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: kBlockGap),
      child: Column(
        children: [
          for (final b in items) ...[if (b != items.first) const SizedBox(height: 12), _banner(context, b)],
        ],
      ),
    );
  }

  Widget _banner(BuildContext context, BannerView b) {
    final k = context.k;
    final t = context.t;
    final card = KCard(
      padding: EdgeInsets.zero,
      child: IntrinsicHeight(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(
              width: 4,
              color: switch (b.tone) {
                'ember' => k.ember,
                'gold' => k.gold,
                'up' => k.up,
                _ => k.fg3,
              },
            ),
            Expanded(
              child: Padding(
                padding: const EdgeInsetsDirectional.fromSTEB(18, 14, 40, 16),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(b.title, style: context.text.headline),
                    if (b.body.isNotEmpty) ...[const SizedBox(height: 2), Text(b.body, style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13))],
                    if (b.ctaUrl != null && b.ctaLabel != null) ...[
                      const SizedBox(height: 12),
                      KButton(
                        label: b.ctaLabel!,
                        trailingIcon: LucideIcons.arrowUpRight,
                        size: KButtonSize.sm,
                        variant: b.tone == 'ember' ? KButtonVariant.ember : KButtonVariant.surface,
                        onPressed: () => _open(b),
                      ),
                    ],
                  ],
                ),
              ),
            ),
          ],
        ),
      ),
    );
    if (!b.dismissible) return card;
    return Stack(
      children: [
        card,
        PositionedDirectional(
          top: 0,
          end: 0,
          child: KPressable(
            semanticLabel: t('rewards.banner.dismiss'),
            onTap: () {
              _track(b.id, 'dismiss');
              setState(() => _hidden.add(b.id));
            },
            child: Icon(LucideIcons.x, size: 15, color: k.fg3),
          ),
        ),
      ],
    );
  }
}

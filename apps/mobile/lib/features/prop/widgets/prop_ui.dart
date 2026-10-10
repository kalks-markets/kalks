// Shared pieces of the prop pages (port of apps/crm/components/prop-live/ui.tsx + equity-chart.tsx): load error card,
// inline error with its next step, rule tiles, credentials, the Trade button, tiles, banners, the drawdown gauge, the
// equity chart with its rule levels, and the certificate picture.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/format/format.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../prop_api.dart';

/* ------------------------------------------------------------------ errors */

/// The web's LoadError: connection art, the prop texts and Try again.
class PropLoadError extends StatelessWidget {
  const PropLoadError({super.key, required this.error, required this.onRetry, this.title});
  final Object? error;
  final VoidCallback onRetry;
  final String? title;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final e = error;
    final text = e is ApiException && (e.status == 0 || e.status >= 500) ? t('prop.loadError.text') : propErrorText(e, t);
    return KCard(
      child: KEmptyState(
        compact: true,
        art: KIllustrationName.connectionLost,
        title: title ?? t('prop.loadError.title'),
        text: text,
        action: KButton(label: t('prop.tryAgain'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onRetry),
      ),
    );
  }
}

/// Inline error with the next step (deposit, verify) when there is one (web ErrorNote).
class PropErrorNote extends StatelessWidget {
  const PropErrorNote({super.key, required this.error, this.onNavigate});
  final Object? error;

  /// Opens a link of the note (default: context.go); sheets close themselves first.
  final void Function(String href)? onNavigate;

  @override
  Widget build(BuildContext context) {
    if (error == null) return const SizedBox.shrink();
    final t = context.t;
    final code = propErrorCode(error);
    final link = kPropErrorLink[code];
    final soft = code == 'provisioning' || code == 'payment_pending' || code == 'wallet_pending';
    return KNotice(
      tone: soft ? KChipTone.info : KChipTone.down,
      icon: LucideIcons.triangleAlert,
      text: propErrorText(error, t),
      action: link == null
          ? null
          : KButton(
              label: t(link.$2),
              trailingIcon: Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight,
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => onNavigate != null ? onNavigate!(link.$1) : context.go(link.$1),
            ),
    );
  }
}

/* ------------------------------------------------------------------ small bits */

/// The web's .k-row: a soft rounded box on the card.
class PropRow extends StatelessWidget {
  const PropRow({super.key, required this.child, this.padding = const EdgeInsets.symmetric(horizontal: 14, vertical: 12), this.color, this.border});
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
        color: color ?? k.surface2.withValues(alpha: k.dark ? 0.7 : 0.75),
        borderRadius: BorderRadius.circular(k.rowRadius),
        border: Border.all(color: border ?? k.line),
      ),
      child: child,
    );
  }
}

/// The web's .k-label: a small grey heading above a block.
class PropLabel extends StatelessWidget {
  const PropLabel(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Text(
    text,
    style: context.text.caption.copyWith(color: context.k.fg3, fontWeight: FontWeight.w600),
  );
}

/// A label / value tile (web Tile).
class PropTile extends StatelessWidget {
  const PropTile({super.key, required this.label, this.value, this.valueWidget, this.sub, this.tone});
  final String label;
  final String? value;
  final Widget? valueWidget;
  final String? sub;
  final Color? tone;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return PropRow(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.caption.copyWith(color: k.fg3),
          ),
          const SizedBox(height: 4),
          DefaultTextStyle.merge(
            style: context.text.figure.copyWith(fontSize: 16, color: tone ?? k.fg),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            child: valueWidget ?? Text(value ?? '—', textDirection: TextDirection.ltr),
          ),
          if (sub != null) ...[
            const SizedBox(height: 2),
            Text(
              sub!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.caption.copyWith(color: k.fg3, fontSize: 11),
            ),
          ],
        ],
      ),
    );
  }
}

/// Two widgets side by side with a gap (the web's grid-cols-2 rows).
class PropPair extends StatelessWidget {
  const PropPair(this.a, this.b, {super.key, this.gap = 8});
  final Widget a, b;
  final double gap;

  @override
  Widget build(BuildContext context) => IntrinsicHeight(
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Expanded(child: a),
        SizedBox(width: gap),
        Expanded(child: b),
      ],
    ),
  );
}

/// Items two per row (grid-cols-2).
class PropGrid extends StatelessWidget {
  const PropGrid({super.key, required this.children, this.gap = 8});
  final List<Widget> children;
  final double gap;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.stretch,
    children: [
      for (var i = 0; i < children.length; i += 2) ...[
        if (i > 0) SizedBox(height: gap),
        PropPair(children[i], i + 1 < children.length ? children[i + 1] : const SizedBox.shrink(), gap: gap),
      ],
    ],
  );
}

/// The header's action button (surface pill) with an optional count chip (web: My challenges + count).
class PropHeaderButton extends StatelessWidget {
  const PropHeaderButton({super.key, required this.label, required this.icon, required this.onTap, this.count = 0});
  final String label;
  final IconData icon;
  final VoidCallback onTap;
  final int count;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KPressable(
      onTap: onTap,
      semanticLabel: label,
      child: Container(
        height: KSize.buttonMd,
        padding: const EdgeInsets.symmetric(horizontal: 16),
        decoration: BoxDecoration(
          color: k.surface,
          borderRadius: BorderRadius.circular(KSize.buttonMd / 2),
          border: Border.all(color: k.line),
          boxShadow: const [BoxShadow(color: Color(0x0A301C40), offset: Offset(0, 1), blurRadius: 2)],
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(icon, size: 17, color: k.fg),
            const SizedBox(width: 7),
            Text(label, style: context.text.headline.copyWith(fontSize: 14, height: 1.1)),
            if (count > 0) ...[const SizedBox(width: 8), KChip(label: '$count', tone: KChipTone.ember, small: true)],
          ],
        ),
      ),
    );
  }
}

IconData arrowEnd(BuildContext context) => Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight;

/* ------------------------------------------------------------------ banners */

/// A tinted status banner (mine.tsx Banner): icon, title, text and an action under it.
class PropBanner extends StatelessWidget {
  const PropBanner({super.key, required this.tone, required this.icon, required this.title, this.text, this.action});
  final KChipTone tone;
  final IconData icon;
  final String title;
  final String? text;
  final Widget? action;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final (bg, fg, border) = k.chip(tone);
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: border),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 1),
            child: Icon(icon, size: 18, color: fg),
          ),
          const SizedBox(width: 12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  title,
                  style: context.text.label.copyWith(fontWeight: FontWeight.w600, color: fg),
                ),
                if (text != null) ...[const SizedBox(height: 2), Text(text!, style: context.text.footnote.copyWith(color: k.fg2, height: 1.4))],
                if (action != null) ...[const SizedBox(height: 10), action!],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ rule tiles */

enum RuleState { ok, passed, failed, waiting, off }

class RuleStateChip extends StatelessWidget {
  const RuleStateChip(this.state, {super.key});
  final RuleState state;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final (String label, KChipTone tone) = switch (state) {
      RuleState.ok => (t('prop.ruleState.ok'), KChipTone.ember),
      RuleState.passed => (t('prop.ruleState.passed'), KChipTone.up),
      RuleState.failed => (t('prop.ruleState.failed'), KChipTone.down),
      RuleState.waiting => (t('prop.ruleState.waiting'), KChipTone.neutral),
      RuleState.off => (t('prop.ruleState.off'), KChipTone.neutral),
    };
    return KChip(label: label, tone: tone, small: true);
  }
}

/// One row of a [RuleTile]: label, value and an optional tone.
typedef RuleRow = (String label, String value, Color? tone);

/// A rule card (web RuleTile): icon, title, state chip, a progress bar with its read-outs, the detail rows.
class RuleTile extends StatelessWidget {
  const RuleTile({
    super.key,
    required this.icon,
    required this.title,
    required this.state,
    required this.rows,
    this.progress,
    this.tone,
    this.left,
    this.right,
    this.footer,
  });
  final IconData icon;
  final String title;
  final RuleState state;
  final List<RuleRow> rows;

  /// 0..100; null hides the bar.
  final double? progress;
  final Color? tone;
  final Widget? left;
  final String? right;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Container(
                width: 32,
                height: 32,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  color: k.surface2,
                  border: Border.all(color: k.line),
                ),
                child: Icon(icon, size: 15, color: k.fg2),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  title,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.headline.copyWith(fontSize: 14.5, fontWeight: FontWeight.w500),
                ),
              ),
              const SizedBox(width: 8),
              RuleStateChip(state),
            ],
          ),
          if (progress != null) ...[
            const SizedBox(height: 14),
            Row(
              children: [
                Expanded(
                  child: DefaultTextStyle.merge(
                    style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    child: left ?? const SizedBox.shrink(),
                  ),
                ),
                if (right != null) ...[
                  const SizedBox(width: 8),
                  Text(
                    right!,
                    textDirection: TextDirection.ltr,
                    style: context.text.caption.copyWith(color: k.fg3, fontFeatures: kTabular),
                  ),
                ],
              ],
            ),
            const SizedBox(height: 6),
            KProgressBar(value: progress! / 100, color: tone ?? k.ember, height: 8),
          ],
          const SizedBox(height: 14),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            decoration: BoxDecoration(
              color: k.surface2.withValues(alpha: 0.5),
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: k.line),
            ),
            child: Column(
              children: [
                for (var i = 0; i < rows.length; i++) ...[
                  if (i > 0) const KDivider(),
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(
                          child: Text(rows[i].$1, style: context.text.footnote.copyWith(color: k.fg3)),
                        ),
                        const SizedBox(width: 10),
                        Flexible(
                          child: Text(
                            rows[i].$2,
                            textAlign: TextAlign.end,
                            style: context.text.footnote.copyWith(fontWeight: FontWeight.w600, color: rows[i].$3 ?? k.fg, fontFeatures: kTabular),
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
          ),
          if (footer != null) ...[const SizedBox(height: 12), footer!],
        ],
      ),
    );
  }
}

/// The web's Trans (`Used <v>{v}</v> of {max}`): the figure in the text colour.
class PropTrans extends StatelessWidget {
  const PropTrans(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => KRichText(
    text,
    style: context.text.caption.copyWith(color: context.k.fg3, fontWeight: FontWeight.w400),
    tags: {
      'v': KTag(
        style: TextStyle(color: context.k.fg, fontFeatures: kTabular),
      ),
    },
  );
}

/* ------------------------------------------------------------------ credentials, Trade */

/// A login / password field with reveal and copy (web CredentialField).
class CredentialField extends StatefulWidget {
  const CredentialField({super.key, required this.label, required this.value, this.secret = false, this.mono = true, this.copy = true});
  final String label;
  final String value;
  final bool secret;
  final bool mono;
  final bool copy;

  @override
  State<CredentialField> createState() => _CredentialFieldState();
}

class _CredentialFieldState extends State<CredentialField> {
  late bool _show = !widget.secret;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final t = context.t;
    final style = widget.mono ? context.text.mono(13) : context.text.callout;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(
          widget.label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
        ),
        const SizedBox(height: 6),
        Container(
          height: 40,
          padding: const EdgeInsetsDirectional.only(start: 12, end: 2),
          decoration: BoxDecoration(
            color: k.surface2,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: k.line),
          ),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  _show ? widget.value : '••••••••',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textDirection: widget.mono ? TextDirection.ltr : null,
                  textAlign: TextAlign.start,
                  style: style.copyWith(color: k.fg, letterSpacing: _show ? null : 2.6),
                ),
              ),
              if (widget.secret)
                KIconButton(
                  icon: _show ? LucideIcons.eyeOff : LucideIcons.eye,
                  size: 32,
                  color: k.fg3,
                  semanticLabel: _show ? t('prop.hide') : t('prop.show'),
                  onPressed: () => setState(() => _show = !_show),
                ),
              if (widget.copy)
                KIconButton(
                  icon: LucideIcons.copy,
                  size: 32,
                  color: k.fg3,
                  semanticLabel: '${t('common.copy')} ${widget.label}',
                  onPressed: () => kCopy(context, widget.value),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Opens Kalks Trader on a prop account (web PropTradeButton: one-time SSO; the app's trader opens the session).
class PropTradeButton extends StatelessWidget {
  const PropTradeButton({
    super.key,
    required this.login,
    this.disabled = false,
    this.label,
    this.variant = KButtonVariant.ember,
    this.size = KButtonSize.sm,
    this.onBeforeOpen,
  });
  final int? login;
  final bool disabled;
  final String? label;
  final KButtonVariant variant;
  final KButtonSize size;

  /// Runs before opening (a sheet closes itself).
  final VoidCallback? onBeforeOpen;

  @override
  Widget build(BuildContext context) {
    final blocked = disabled || login == null;
    return KButton(
      label: label ?? context.t('prop.trade.button'),
      icon: LucideIcons.candlestickChart,
      variant: variant,
      size: size,
      onPressed: blocked
          ? null
          : () {
              final router = GoRouter.of(context);
              onBeforeOpen?.call();
              router.push('/trader?login=$login');
            },
    );
  }
}

/* ------------------------------------------------------------------ daily reset countdown */

/// "HH:MM:SS" to the next reset (web useCountdown: the service's nextReset, else the next 17:00 New York).
class ResetCountdown extends StatefulWidget {
  const ResetCountdown({super.key, required this.target, this.style});
  final DateTime? target;
  final TextStyle? style;

  @override
  State<ResetCountdown> createState() => _ResetCountdownState();
}

class _ResetCountdownState extends State<ResetCountdown> {
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
    final now = DateTime.now();
    var to = widget.target;
    if (to == null || !to.isAfter(now)) to = nextNyClose(now);
    return Text(hms(to.difference(now)), textDirection: TextDirection.ltr, style: widget.style);
  }
}

/* ------------------------------------------------------------------ drawdown gauge */

/// A half-circle gauge (web Gauge): the value arc, the figure in the middle, a label and a sub-label.
class PropGauge extends StatelessWidget {
  const PropGauge({super.key, required this.value, required this.display, required this.label, this.sublabel, this.color, this.size = 210});

  /// 0..100.
  final double value;
  final String display;
  final String label;
  final String? sublabel;
  final Color? color;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return SizedBox(
      width: size,
      height: size * 0.62,
      child: CustomPaint(
        painter: _GaugePainter(value.clamp(0, 100) / 100, color ?? k.ember, k.surface3),
        child: Padding(
          padding: EdgeInsets.only(top: size * 0.2),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(display, textDirection: TextDirection.ltr, style: context.text.moneyL.copyWith(fontSize: 26)),
              const SizedBox(height: 2),
              Text(
                label,
                textAlign: TextAlign.center,
                style: context.text.caption.copyWith(color: k.fg2),
              ),
              if (sublabel != null)
                Text(
                  sublabel!,
                  textAlign: TextAlign.center,
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

class _GaugePainter extends CustomPainter {
  _GaugePainter(this.value, this.color, this.track);
  final double value;
  final Color color, track;

  @override
  void paint(Canvas canvas, Size size) {
    const stroke = 14.0;
    final r = size.width / 2 - stroke / 2;
    final rect = Rect.fromCircle(center: Offset(size.width / 2, r + stroke / 2), radius: r);
    final base = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = stroke
      ..strokeCap = StrokeCap.round
      ..color = track;
    canvas.drawArc(rect, math.pi, math.pi, false, base);
    if (value > 0) canvas.drawArc(rect, math.pi, math.pi * value, false, base..color = color);
  }

  @override
  bool shouldRepaint(_GaugePainter old) => old.value != value || old.color != color || old.track != track;
}

/* ------------------------------------------------------------------ equity chart */

/// A labelled rule level on the equity chart (drawdown floor, start balance, daily floor, profit target).
class PropChartLine {
  const PropChartLine(this.value, this.label, this.color);
  final double value;
  final String label;
  final Color color;
}

/// The equity line with its rule levels (web PropEquityLine): KLineChart with dashed guides and their labels under it.
class PropEquityChart extends StatelessWidget {
  const PropEquityChart({super.key, required this.points, required this.lines, this.height = 220});
  final List<({DateTime at, double v})> points;
  final List<PropChartLine> lines;
  final double height;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final t = context.t;
    if (points.length < 2) {
      return Container(
        height: height,
        alignment: Alignment.center,
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: k.line),
        ),
        child: Text(
          t('prop.chart.empty'),
          textAlign: TextAlign.center,
          style: context.text.footnote.copyWith(color: k.fg3),
        ),
      );
    }
    final f = LocaleFormat(t.locale);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KLineChart(
          values: [for (final p in points) p.v],
          labels: [for (final p in points) f.dateTime(p.at)],
          height: height,
          color: k.ember,
          guides: [for (final l in lines) KChartGuide(l.value, color: l.color, label: l.label)],
          format: usd,
        ),
        const SizedBox(height: 10),
        Wrap(
          spacing: 14,
          runSpacing: 6,
          children: [
            for (final l in lines)
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  CustomPaint(size: const Size(16, 2), painter: _DashPainter(l.color)),
                  const SizedBox(width: 6),
                  Text(
                    l.label,
                    style: context.text.caption.copyWith(color: l.color, fontWeight: FontWeight.w500),
                  ),
                ],
              ),
          ],
        ),
      ],
    );
  }
}

class _DashPainter extends CustomPainter {
  _DashPainter(this.color);
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final p = Paint()
      ..color = color
      ..strokeWidth = 1.6;
    for (double x = 0; x < size.width; x += 6) {
      canvas.drawLine(Offset(x, size.height / 2), Offset(math.min(x + 3.5, size.width), size.height / 2), p);
    }
  }

  @override
  bool shouldRepaint(_DashPainter old) => old.color != color;
}

/* ------------------------------------------------------------------ certificate picture */

const List<String> _months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

String _certMoney(double v) => '\$${v.toStringAsFixed(v % 1 == 0 ? 0 : 2).replaceAllMapped(RegExp(r'\B(?=(\d{3})+(?!\d))'), (_) => ',')}';

/// The certificate picture, drawn from the certificate's fields in the same 1200 × 675 layout as the server's PNG
/// (apps/crm/lib/prop-cert-image.tsx; English, like the image itself), scaled to the width.
class CertificateArt extends StatelessWidget {
  const CertificateArt({super.key, required this.cert, required this.host});
  final Certificate cert;

  /// "app.kalkstrade.com" (the verify line).
  final String host;

  @override
  Widget build(BuildContext context) {
    final c = cert;
    final headline = c.kind == 'payout' ? 'Certificate of Payout' : (c.kind == 'funded' ? 'Funded Trader' : 'Certificate of Achievement');
    final big = c.kind == 'payout' && c.amount != null ? _certMoney(c.amount!) : _certMoney(c.size);
    final sub = switch (c.kind) {
      'payout' => 'paid out on a ${_certMoney(c.size)} ${c.planName} account',
      'funded' => '${_certMoney(c.size)} funded account · ${c.planName}',
      _ => 'passed ${c.phase ?? 'evaluation'} · ${_certMoney(c.size)} account · ${c.planName}',
    };
    final d = c.issuedAt?.toUtc();
    final date = d == null ? '' : '${d.day.toString().padLeft(2, '0')} ${_months[d.month - 1]} ${d.year}';
    const fg = Color(0xFFF5F5F6), muted = Color(0xFF9A9AA3), line = Color(0xFF2A2A30);
    TextStyle s(double size, {FontWeight w = FontWeight.w400, Color color = fg, double spacing = 0}) =>
        TextStyle(fontSize: size, fontWeight: w, color: color, letterSpacing: spacing, height: 1.1, fontFamily: context.text.body.fontFamily);
    return AspectRatio(
      aspectRatio: 1200 / 675,
      child: FittedBox(
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Container(
            width: 1200,
            height: 675,
            color: const Color(0xFF0B0B0D),
            padding: const EdgeInsets.all(24),
            child: Container(
              decoration: BoxDecoration(
                color: const Color(0xFF121215),
                border: Border.all(color: line, width: 2),
                borderRadius: BorderRadius.circular(28),
              ),
              clipBehavior: Clip.antiAlias,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Container(height: 6, color: kEmber),
                  Expanded(
                    child: Padding(
                      padding: const EdgeInsets.symmetric(horizontal: 56),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          const SizedBox(height: 52),
                          Row(
                            crossAxisAlignment: CrossAxisAlignment.end,
                            children: [
                              Text('KALKS', style: s(28, w: FontWeight.w700, spacing: 1)),
                              const SizedBox(width: 14),
                              Text('PROP', style: s(18, color: muted)),
                              const Spacer(),
                              Text('No. ${c.code}', style: s(16, color: muted)),
                            ],
                          ),
                          const SizedBox(height: 62),
                          Text(headline.toUpperCase(), style: s(22, color: const Color(0xFFFF8A3D), spacing: 3)),
                          const SizedBox(height: 18),
                          Text(
                            c.traderName,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: s(56, w: FontWeight.w600),
                          ),
                          const SizedBox(height: 20),
                          Text(big, style: s(92, w: FontWeight.w700)),
                          const SizedBox(height: 22),
                          Text(
                            sub,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: s(24, color: const Color(0xFFC4C4CC)),
                          ),
                          const Spacer(),
                          Container(height: 1, color: line),
                          const SizedBox(height: 30),
                          Row(
                            crossAxisAlignment: CrossAxisAlignment.end,
                            children: [
                              Column(
                                crossAxisAlignment: CrossAxisAlignment.start,
                                children: [
                                  Text('Issued', style: s(16, color: muted)),
                                  const SizedBox(height: 8),
                                  Text(date, style: s(22)),
                                ],
                              ),
                              const Spacer(),
                              if (c.revoked)
                                Text(
                                  'REVOKED',
                                  style: s(20, w: FontWeight.w600, color: const Color(0xFFF04438)),
                                ),
                              const Spacer(),
                              Column(
                                crossAxisAlignment: CrossAxisAlignment.end,
                                children: [
                                  Text('Verify', style: s(16, color: muted)),
                                  const SizedBox(height: 8),
                                  Text('$host/verify/${c.code}', style: s(20)),
                                ],
                              ),
                            ],
                          ),
                          const SizedBox(height: 38),
                        ],
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

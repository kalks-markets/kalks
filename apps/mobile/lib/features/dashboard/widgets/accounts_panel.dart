// "Your accounts" (web components/dashboard/home/accounts-panel.tsx): each trading account as a glossy card (brand
// gradient for live, ink for demo) in a swipeable carousel with dots, then its details (hideable), then its actions.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/format/format.dart';
import '../../../core/models/account.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';

class AccountsPanel extends StatefulWidget {
  const AccountsPanel({
    super.key,
    required this.accounts,
    required this.loading,
    required this.failed,
    required this.onRetry,
    required this.hidden,
    required this.onToggleHidden,
    this.extraCount = 0,
    this.readOnly = false,
    this.actions,
  });

  /// Live first, then demo, then prop; at most 8 (web).
  final List<EngineAccount>? accounts;
  final bool loading;
  final bool failed;
  final VoidCallback onRetry;
  final bool hidden;
  final VoidCallback onToggleHidden;
  final int extraCount;
  final bool readOnly;

  /// The selected account's actions under its details (web `actions`: Trade, Fund / Refill, ⋯); null hides them.
  final Widget Function(EngineAccount a)? actions;

  @override
  State<AccountsPanel> createState() => _AccountsPanelState();
}

class _AccountsPanelState extends State<AccountsPanel> {
  final _page = PageController();
  int _i = 0;

  @override
  void dispose() {
    _page.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final list = widget.accounts ?? const [];
    final idx = list.isEmpty ? 0 : _i.clamp(0, list.length - 1);
    final a = list.isEmpty ? null : list[idx];

    Widget cardArea;
    if (widget.loading && widget.accounts == null) {
      cardArea = const AspectRatio(aspectRatio: 1.6, child: KSkeleton(radius: 24));
    } else if (widget.failed && widget.accounts == null) {
      cardArea = AspectRatio(
        aspectRatio: 1.6,
        child: KCard(
          child: Center(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  t('dashboard.accounts.unavailable'),
                  textAlign: TextAlign.center,
                  style: context.text.callout.copyWith(color: k.fg2),
                ),
                const SizedBox(height: 12),
                KButton(label: t('common.retry'), size: KButtonSize.sm, onPressed: widget.onRetry),
              ],
            ),
          ),
        ),
      );
    } else if (a == null) {
      cardArea = KPressable(
        onTap: () => context.go('/accounts/new'),
        pressedScale: 0.99,
        child: AspectRatio(
          aspectRatio: 1.6,
          child: CustomPaint(
            painter: _DashedRRect(color: Color.lerp(k.line, k.ember, 0.35)!, radius: 24),
            child: Container(
              decoration: BoxDecoration(color: k.surface.withValues(alpha: 0.6), borderRadius: BorderRadius.circular(24)),
              child: Center(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Container(
                      width: 48,
                      height: 48,
                      decoration: BoxDecoration(
                        color: k.ember,
                        shape: BoxShape.circle,
                        boxShadow: [BoxShadow(color: k.ember.withValues(alpha: 0.5), offset: const Offset(0, 10), blurRadius: 24, spreadRadius: -10)],
                      ),
                      child: Icon(LucideIcons.plus, color: k.onEmber, size: 20),
                    ),
                    const SizedBox(height: 12),
                    Text(t('dashboard.accounts.openLive.title'), style: context.text.headline),
                    const SizedBox(height: 4),
                    Text(
                      t('dashboard.accounts.openLive.text'),
                      textAlign: TextAlign.center,
                      style: context.text.footnote.copyWith(color: k.fg3),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ),
      );
    } else {
      cardArea = AspectRatio(
        aspectRatio: 1.6,
        child: PageView.builder(
          controller: _page,
          itemCount: list.length,
          clipBehavior: Clip.none,
          onPageChanged: (i) {
            KHaptics.selection();
            setState(() => _i = i);
          },
          itemBuilder: (context, i) => Padding(
            padding: const EdgeInsets.symmetric(horizontal: 2),
            child: KPressable(
              pressedScale: 0.985,
              pressedOpacity: 0.92,
              onTap: () => context.go('/accounts/${list[i].login}'),
              semanticLabel: '${list[i].groupName} #${list[i].login}',
              child: AccountCardVisual(account: list[i], hidden: widget.hidden),
            ),
          ),
        ),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KSectionTitle(
          t('dashboard.home.yourAccounts'),
          large: true,
          trailing: widget.readOnly
              ? null
              : KButton(
                  label: t('dashboard.accounts.open'),
                  trailingIcon: LucideIcons.plus,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: () => context.go('/accounts/new'),
                ),
        ),
        const SizedBox(height: 16),
        cardArea,
        if (list.length > 1) ...[
          const SizedBox(height: 14),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              for (var i = 0; i < list.length; i++)
                GestureDetector(
                  onTap: () => _page.animateToPage(i, duration: const Duration(milliseconds: 320), curve: Curves.easeOutCubic),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(horizontal: 3, vertical: 9),
                    child: AnimatedContainer(
                      duration: const Duration(milliseconds: 220),
                      width: i == idx ? 22 : 6,
                      height: 6,
                      decoration: BoxDecoration(color: i == idx ? k.fg : k.fg3.withValues(alpha: 0.45), borderRadius: BorderRadius.circular(3)),
                    ),
                  ),
                ),
              if (widget.extraCount > 0) ...[
                const SizedBox(width: 8),
                KTextButton(label: t('dashboard.accounts.more', {'count': widget.extraCount}), color: k.fg3, onPressed: () => context.go('/accounts')),
              ],
            ],
          ),
        ],
        if (a != null) ...[
          const SizedBox(height: 18),
          Row(
            children: [
              Expanded(child: Text(t('dashboard.home.accountInfo'), style: context.text.title2)),
              KIconButton(
                icon: widget.hidden ? LucideIcons.eye : LucideIcons.eyeOff,
                size: 38,
                semanticLabel: widget.hidden ? t('dashboard.home.showBalances') : t('dashboard.home.hideBalances'),
                onPressed: widget.onToggleHidden,
              ),
            ],
          ),
          const SizedBox(height: 8),
          KCard(
            padding: const EdgeInsets.all(18),
            child: _InfoGrid(a: a, hidden: widget.hidden),
          ),
          if (widget.actions != null) ...[const SizedBox(height: 12), widget.actions!(a)],
        ],
      ],
    );
  }
}

class _InfoGrid extends StatelessWidget {
  const _InfoGrid({required this.a, required this.hidden});
  final EngineAccount a;
  final bool hidden;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    const dots = '••••';
    final login = '${a.login}';
    final cells = <(String, Widget)>[
      (t('dashboard.home.accountName'), Text(a.name.isNotEmpty ? a.name : '${a.groupName} · ${t.dyn('accounts.mode.${a.mode}', fallback: a.mode)}')),
      (
        t('accounts.label.login'),
        Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Flexible(
              child: Text(
                hidden ? '$dots ${login.substring(math.max(0, login.length - 4))}' : login,
                textDirection: TextDirection.ltr,
                style: context.text.mono(14.5, weight: FontWeight.w600),
              ),
            ),
            if (!hidden)
              KPressable(
                minSize: 32,
                onTap: () {
                  Clipboard.setData(ClipboardData(text: login));
                  KHaptics.success();
                },
                semanticLabel: t('common.copy'),
                child: Padding(
                  padding: const EdgeInsets.symmetric(horizontal: 6),
                  child: Icon(LucideIcons.copy, size: 14, color: k.fg3),
                ),
              ),
          ],
        ),
      ),
      (t('common.equity'), Text(hidden ? dots : a.money(a.equity), textDirection: TextDirection.ltr)),
      (t('dashboard.home.leverage'), Text(Fmt.leverage(a.leverage), textDirection: TextDirection.ltr)),
      (t('accounts.label.freeMargin'), Text(hidden ? dots : a.money(a.freeMargin), textDirection: TextDirection.ltr)),
      (t('accounts.label.marginLevel'), Text(Fmt.level(a.marginLevel), textDirection: TextDirection.ltr)),
    ];
    return Column(
      children: [
        for (var r = 0; r < cells.length; r += 2) ...[
          if (r > 0) const SizedBox(height: 16),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: KStat(label: cells[r].$1, value: cells[r].$2),
              ),
              const SizedBox(width: 16),
              Expanded(
                child: KStat(label: cells[r + 1].$1, value: cells[r + 1].$2),
              ),
            ],
          ),
        ],
      ],
    );
  }
}

/// The glossy account card (web AccountVisual + .k-acct-live / .k-acct-demo + the white ribbon).
class AccountCardVisual extends StatelessWidget {
  const AccountCardVisual({super.key, required this.account, this.hidden = false});
  final EngineAccount account;
  final bool hidden;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = account;
    final live = a.live && !a.prop;
    final e = k.ember;
    const white = Colors.white;
    final layers = live
        ? [
            LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [mixOklab(e, white, 0.9), mixOklab(e, const Color(0xFF5A1470), 0.78)]),
            RadialGradient(
              center: Alignment.topLeft,
              radius: 1.3,
              colors: [mixOklab(e, white, 0.52), mixOklab(e, white, 0.52).withValues(alpha: 0)],
              stops: const [0, 0.52],
            ),
            RadialGradient(
              center: Alignment.bottomRight,
              radius: 1.2,
              colors: [mixOklab(e, const Color(0xFF3C0F58), 0.62), mixOklab(e, const Color(0xFF3C0F58), 0.62).withValues(alpha: 0)],
              stops: const [0, 0.62],
            ),
          ]
        : [
            const LinearGradient(begin: Alignment.topLeft, end: Alignment.bottomRight, colors: [Color(0xFF2C2935), Color(0xFF121016)]),
            RadialGradient(
              center: Alignment.topLeft,
              radius: 1.3,
              colors: [mixOklab(e, const Color(0xFF45404F), 0.3), mixOklab(e, const Color(0xFF45404F), 0.3).withValues(alpha: 0)],
              stops: const [0, 0.55],
            ),
            RadialGradient(
              center: Alignment.bottomRight,
              radius: 1.1,
              colors: [mixOklab(e, const Color(0xFF1A1720), 0.22), mixOklab(e, const Color(0xFF1A1720), 0.22).withValues(alpha: 0)],
              stops: const [0, 0.6],
            ),
          ];
    final kind = a.prop ? t('accounts.badge.prop') : (a.live ? t('accounts.badge.live') : t('accounts.badge.demo'));
    // an Options account says so on its card
    final badge = a.isOptions ? '$kind · ${t('accounts.product.chipOptions')}' : kind;
    final title = '${a.groupName} · ${t.dyn('accounts.mode.${a.mode}', fallback: a.mode)}';
    final caption = context.text.micro.copyWith(color: white.withValues(alpha: 0.75), letterSpacing: 1.0);
    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(24),
        boxShadow: [
          BoxShadow(
            color: live ? e.withValues(alpha: 0.55) : Colors.black.withValues(alpha: 0.5),
            offset: const Offset(0, 26),
            blurRadius: 44,
            spreadRadius: -26,
          ),
        ],
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(24),
        child: Stack(
          fit: StackFit.expand,
          children: [
            for (final g in layers) DecoratedBox(decoration: BoxDecoration(gradient: g)),
            CustomPaint(painter: _Ribbon()),
            DecoratedBox(
              decoration: BoxDecoration(
                borderRadius: BorderRadius.circular(24),
                border: Border(
                  top: BorderSide(color: white.withValues(alpha: live ? 0.35 : 0.12)),
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.all(20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      const KLogoMark(size: 26, color: Colors.white),
                      const Spacer(),
                      Container(
                        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                        decoration: BoxDecoration(color: white.withValues(alpha: 0.2), borderRadius: BorderRadius.circular(12)),
                        child: Text(
                          badge,
                          style: context.text.micro.copyWith(color: white, fontWeight: FontWeight.w700, letterSpacing: 0.8),
                        ),
                      ),
                    ],
                  ),
                  const Spacer(),
                  Text(
                    Fmt.spacedLogin('${a.login}'),
                    textDirection: TextDirection.ltr,
                    textAlign: TextAlign.left,
                    style: context.text
                        .mono(20, weight: FontWeight.w600, color: white)
                        .copyWith(
                          letterSpacing: 2.6,
                          shadows: [Shadow(color: Colors.black.withValues(alpha: 0.18), blurRadius: 8, offset: const Offset(0, 1))],
                        ),
                  ),
                  const SizedBox(height: 12),
                  Row(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              (a.name.isNotEmpty ? a.name : t('dashboard.home.tradingAccount')).toUpperCase(),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: caption,
                            ),
                            Text(
                              title,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.label.copyWith(color: white, fontWeight: FontWeight.w600),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(width: 12),
                      Column(
                        crossAxisAlignment: CrossAxisAlignment.end,
                        children: [
                          Text(t('common.balance').toUpperCase(), style: caption),
                          Text(
                            hidden ? '••••••' : a.money(a.balance),
                            textDirection: TextDirection.ltr,
                            style: context.text.figure.copyWith(color: white, fontWeight: FontWeight.w700),
                          ),
                        ],
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
  }
}

/// The decorative white ribbon strokes over the account card (web Ribbon SVG, viewBox 400 x 250).
class _Ribbon extends CustomPainter {
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
    canvas.drawPath(p1, stroke(26, [white.withValues(alpha: 0), white.withValues(alpha: 0.55), white.withValues(alpha: 0.05)], const [0, 0.45, 1]));
    final p2 = Path()
      ..moveTo(-30, 200)
      ..cubicTo(50, 160, 120, 240, 200, 180)
      ..cubicTo(280, 120, 300, 80, 350, 150)
      ..cubicTo(400, 220, 410, 200, 440, 130);
    final g2 = stroke(
      12,
      [white.withValues(alpha: 0.05), white.withValues(alpha: 0.35), white.withValues(alpha: 0)],
      const [0, 0.6, 1],
      begin: Alignment.bottomLeft,
      end: Alignment.topRight,
    );
    canvas.drawPath(p2, g2);
    final p3 = Path()
      ..moveTo(150, -20)
      ..cubicTo(170, 60, 120, 90, 160, 140)
      ..cubicTo(200, 190, 240, 170, 230, 260);
    canvas.drawPath(
      p3,
      stroke(
        40,
        [white.withValues(alpha: 0.025), white.withValues(alpha: 0.17), white.withValues(alpha: 0)],
        const [0, 0.6, 1],
        begin: Alignment.bottomLeft,
        end: Alignment.topRight,
      ),
    );
    canvas.restore();
  }

  @override
  bool shouldRepaint(_Ribbon old) => false;
}

/// A dashed rounded border (web .k-acct-empty: 2px dashed).
class _DashedRRect extends CustomPainter {
  _DashedRRect({required this.color, required this.radius});
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final path = Path()..addRRect(RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius)));
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 2;
    for (final m in path.computeMetrics()) {
      var d = 0.0;
      while (d < m.length) {
        canvas.drawPath(m.extractPath(d, d + 7), paint);
        d += 12;
      }
    }
  }

  @override
  bool shouldRepaint(_DashedRRect old) => old.color != color;
}

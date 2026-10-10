// "Your accounts" (web components/dashboard/home/accounts-panel.tsx): each trading account as a debit card (orange
// live, black demo, gold prop) in a swipeable carousel with dots, then the selected account's actions and the eye
// that hides every amount.
import 'package:flutter/material.dart';
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
      cardArea = const AspectRatio(aspectRatio: 1.586, child: KSkeleton(radius: 22));
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
        aspectRatio: 1.586,
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
          const SizedBox(height: 16),
          Row(
            children: [
              Expanded(child: widget.actions != null ? widget.actions!(a) : const SizedBox.shrink()),
              const SizedBox(width: 4),
              KIconButton(
                key: const ValueKey('accounts-eye'),
                icon: widget.hidden ? LucideIcons.eyeOff : LucideIcons.eye,
                color: k.fg3,
                semanticLabel: widget.hidden ? t('dashboard.home.showBalances') : t('dashboard.home.hideBalances'),
                onPressed: widget.onToggleHidden,
              ),
            ],
          ),
        ],
      ],
    );
  }
}

/// A trading account as a debit card (web AccountVisual): Kalks orange live, black demo, gold prop; the spaced login,
/// equity / free margin / leverage, the name and the balance. Amounts follow the "hide amounts" eye.
class AccountCardVisual extends StatelessWidget {
  const AccountCardVisual({super.key, required this.account, this.hidden = false});
  final EngineAccount account;
  final bool hidden;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final a = account;
    String mask(String v) => hidden ? '••••••' : v;
    return KDebitCard(
      login: '${a.login}',
      finish: a.prop ? KCardFinish.prop : (a.live ? KCardFinish.live : KCardFinish.demo),
      badges: [
        if (a.isOptions) t('accounts.product.chipOptions'),
        a.prop ? t('accounts.badge.prop') : (a.live ? t('accounts.badge.live') : t('accounts.badge.demo')),
      ],
      figures: [
        (t('common.equity'), mask(a.money(a.equity))),
        (t('accounts.label.freeMargin'), mask(a.money(a.freeMargin))),
        (t('dashboard.home.leverage'), Fmt.leverage(a.leverage)),
      ],
      holder: a.name.isNotEmpty ? a.name : t('dashboard.home.tradingAccount'),
      title: '${a.groupName} · ${t.dyn('accounts.mode.${a.mode}', fallback: a.mode)}',
      balanceLabel: t('common.balance'),
      balance: a.money(a.balance),
      hidden: hidden,
    );
  }
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

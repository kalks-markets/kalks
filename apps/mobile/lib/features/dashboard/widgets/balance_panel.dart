// Total balance with the money actions (web components/dashboard/home/balance-panel.tsx BalancePanel): the figure
// in large type, the change chip, then Deposit / Withdraw (near-black ink) and Transfer funds under them.
import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';

class BalancePanel extends StatelessWidget {
  const BalancePanel({super.key, required this.total, required this.loading, this.changePct, this.readOnly = false, this.hidden = false, this.wallet = true});

  /// Live accounts' equity + wallet (USD); null when unknown.
  final double? total;
  final bool loading;

  /// Today's change in percent (from the reports curve).
  final double? changePct;
  final bool readOnly;
  final bool hidden;

  /// The wallet module is on: the money actions (Deposit, Withdraw, Transfer) show.
  final bool wallet;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return Column(
      children: [
        Text(
          t('dashboard.home.totalBalance'),
          style: context.text.headline.copyWith(fontWeight: FontWeight.w500, color: k.fg2),
        ),
        const SizedBox(height: 12),
        SizedBox(
          height: 44,
          child: Center(
            child: total == null
                ? (loading ? const KSkeleton(width: 220, height: 38, radius: 12) : Text('—', style: context.text.moneyXL))
                : FittedBox(
                    fit: BoxFit.scaleDown,
                    child: KMoney(total!, style: context.text.moneyXL, hidden: hidden),
                  ),
          ),
        ),
        if (changePct != null) ...[
          const SizedBox(height: 14),
          KChangeChip('${changePct! >= 0 ? '+' : ''}${changePct!.toStringAsFixed(2)}%', up: changePct! >= 0),
        ],
        const SizedBox(height: 8),
        Text(
          t('dashboard.home.totalBalanceSub'),
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
        ),
        if (!readOnly && wallet) ...[
          const SizedBox(height: 20),
          Row(
            children: [
              Expanded(
                child: _Pill(label: t('common.deposit'), icon: LucideIcons.arrowDownToLine, onTap: () => context.go('/wallet/deposit')),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: _Pill(label: t('common.withdraw'), icon: LucideIcons.arrowUpFromLine, onTap: () => context.go('/wallet/withdraw')),
              ),
            ],
          ),
          const SizedBox(height: 10),
          _Pill(
            label: t('dashboard.home.transferFunds'),
            icon: rtl ? LucideIcons.arrowRightLeft : LucideIcons.arrowLeftRight,
            ink: false,
            leading: true,
            onTap: () => context.go('/wallet/transfer'),
          ),
        ],
      ],
    );
  }
}

/// The money actions as 52 pt fully round pills: ink (Deposit / Withdraw) or white with a soft border (Transfer).
class _Pill extends StatelessWidget {
  const _Pill({required this.label, required this.icon, required this.onTap, this.ink = true, this.leading = false});
  final String label;
  final IconData icon;
  final VoidCallback onTap;
  final bool ink;

  /// The icon before the label (after it by default).
  final bool leading;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final fg = ink ? k.inkFg : k.fg;
    final glyph = Icon(icon, size: 17, color: fg);
    return KPressable(
      onTap: onTap,
      semanticLabel: label,
      child: Container(
        height: 52,
        padding: const EdgeInsets.symmetric(horizontal: 18),
        decoration: BoxDecoration(
          color: ink ? k.ink : k.surface,
          borderRadius: BorderRadius.circular(26),
          border: ink ? null : Border.all(color: k.line),
          boxShadow: ink
              ? [
                  BoxShadow(
                    color: Colors.black.withValues(alpha: k.dark ? 0.3 : 0.22),
                    offset: const Offset(0, 12),
                    blurRadius: 26,
                    spreadRadius: -14,
                  ),
                ]
              : null,
        ),
        child: Row(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            if (leading) ...[glyph, const SizedBox(width: 8)],
            Flexible(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: context.text.headline.copyWith(fontSize: 15, color: fg, height: 1.1),
              ),
            ),
            if (!leading) ...[const SizedBox(width: 8), glyph],
          ],
        ),
      ),
    );
  }
}

/// Round pastel shortcuts and the dashed "+" to open an account (web QuickActions).
class QuickActions extends StatelessWidget {
  const QuickActions({super.key, required this.items});
  final List<({String label, IconData icon, KTone tone, VoidCallback onTap})> items;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    Widget cell({required Widget circle, required String label, required VoidCallback onTap}) => Expanded(
      child: KPressable(
        onTap: onTap,
        child: Column(
          children: [
            circle,
            const SizedBox(height: 8),
            Text(
              label,
              maxLines: 2,
              textAlign: TextAlign.center,
              overflow: TextOverflow.ellipsis,
              style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w600, height: 1.2),
            ),
          ],
        ),
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        KSectionTitle(t('dashboard.home.quickActions')),
        const SizedBox(height: 14),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            for (final q in items.take(4))
              cell(
                circle: KIconTile(icon: q.icon, tone: q.tone, size: 52, circle: true, iconSize: 21),
                label: q.label,
                onTap: q.onTap,
              ),
            cell(
              circle: Container(
                width: 52,
                height: 52,
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(color: k.fg3.withValues(alpha: 0.4), width: 2),
                ),
                child: Icon(LucideIcons.plus, size: 20, color: k.fg3),
              ),
              label: t('dashboard.accounts.open'),
              onTap: () => GoRouter.of(context).go('/accounts/new'),
            ),
          ],
        ),
      ],
    );
  }
}

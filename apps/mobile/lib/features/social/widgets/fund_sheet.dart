// The PAMM fund drawer (NAV history + rollovers) and the fund status chip. Port of FundDetailDrawer /
// FundStatusChip in apps/crm/components/social-live/funds.tsx (GET funds/{id}, 30 s).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/config/app_config.dart';
import '../../../core/format/format.dart';
import '../../../i18n/i18n.dart';
import '../../../shell/nav.dart';
import '../../../ui/ui.dart';
import '../social_api.dart';
import 'bits.dart';
import 'follow_sheet.dart';

class FundStatusChip extends StatelessWidget {
  const FundStatusChip({super.key, required this.status});
  final String status;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return switch (status) {
      'active' => KChip(label: t('social.fundStatus.active'), tone: KChipTone.up, dot: true, small: true),
      'frozen' => KChip(label: t('social.fundStatus.frozen'), tone: KChipTone.down, icon: LucideIcons.snowflake, small: true),
      _ => KChip(label: t('social.fundStatus.closed'), small: true),
    };
  }
}

/// Opens the fund drawer; `onInvest` (when given) shows the Invest button.
Future<void> showFundSheet(BuildContext context, int fundId, {ValueChanged<int>? onInvest}) => showKSheet<void>(
  context,
  expand: true,
  builder: (_) => _FundSheet(fundId: fundId, onInvest: onInvest),
);

class _FundSheet extends ConsumerWidget {
  const _FundSheet({required this.fundId, this.onInvest});
  final int fundId;
  final ValueChanged<int>? onInvest;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final q = ref.watch(fundDetailProvider(fundId));
    // the master's profile is a copy trading page (/social/masters/:id)
    final profileOn = pageOn(ref.watch(configProvider), '/social/masters');
    final d = q.value;
    final f = d?.fund;
    final lf = LocaleFormat(t.locale);
    final series = <(DateTime, double)>[];
    for (final p in d?.navHistory ?? const <({String at, double nav})>[]) {
      final at = DateTime.tryParse(p.at);
      if (at != null && p.nav.isFinite) series.add((at, p.nav));
    }
    series.sort((a, b) => a.$1.compareTo(b.$1));
    final rollovers = [...?d?.rollovers]..sort((a, b) => strOf(b['at']).compareTo(strOf(a['at'])));
    return KSheetContent(
      children: [
        SheetTitle(title: f != null ? f.name : t('social.funds.pammFund'), description: f != null ? t('social.funds.managedBy', {'name': f.masterName}) : null),
        if (d == null || f == null)
          q.hasError ? InfoBox(tone: KChipTone.down, text: socialError(q.error, t)) : const BlockSkeleton(h: 100)
        else ...[
          Row(
            children: [
              FundStatusChip(status: f.status),
              const Spacer(),
              if (profileOn)
                KButton(
                  label: t('social.masterProfile'),
                  trailingIcon: Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: () {
                    final router = GoRouter.of(context);
                    Navigator.of(context).pop();
                    router.push('/social/masters/${f.masterId}');
                  },
                ),
              if (onInvest != null && f.status == 'active') ...[
                const SizedBox(width: 8),
                KButton(
                  label: t('social.invest'),
                  icon: LucideIcons.wallet,
                  variant: KButtonVariant.ink,
                  size: KButtonSize.sm,
                  onPressed: () {
                    Navigator.of(context).pop();
                    onInvest!(f.id);
                  },
                ),
              ],
            ],
          ),
          const SizedBox(height: 14),
          TileGrid(
            tiles: [
              Tile(label: t('social.navPerUnit'), value: Num(nav4(f.nav))),
              Tile(
                label: t('social.returnAll'),
                value: Num(pct(f.returnAll), color: f.returnAll >= 0 ? k.up : k.down),
              ),
              Tile(
                label: t('social.return1m'),
                value: Num(pct(f.return1m), color: f.return1m >= 0 ? k.up : k.down),
              ),
              Tile(label: t('social.aum'), value: Num(compactUsd(f.aum))),
              Tile(label: t('social.investors'), value: Num(lf.number(f.investorCount, 0))),
              Tile(label: t('social.drawdown'), value: Num(ddText(f.drawdownPct))),
            ],
          ),
          const SizedBox(height: 16),
          GroupLabel(t('social.navPerUnit')),
          if (series.length > 1)
            KLineChart(values: [for (final x in series) x.$2], labels: [for (final x in series) lf.date(x.$1)], height: 180, format: nav4)
          else
            EmptyRow(t('social.funds.navEmpty')),
          const SizedBox(height: 10),
          KKeyValues([
            KKV(t('social.rollover'), t('social.funds.rolloverNext', {'period': periodLabel(t, f.period), 'next': serverTime(t, f.nextRolloverAt)})),
            KKV(t('social.funds.lastRollover'), serverTime(t, f.lastRolloverAt)),
            KKV(t('social.performanceFee'), t('social.invest.feeAboveHwm', {'fee': numText(f.perfFeePct)})),
            KKV(t('social.lockIn'), f.lockInDays > 0 ? t('social.invest.lockDays', {'count': f.lockInDays}) : t('common.none')),
            KKV(t('social.funds.minInvestment'), usd(f.minInvestment, 0), mono: true),
            KKV(t('social.drawdownFreeze'), t('social.funds.freezeValue', {'dd': numText(f.maxDdPct), 'peak': nav4(f.navPeak)})),
            KKV(t('social.funds.masterShare'), t('social.funds.masterShareValue', {'pct': f.masterSharePct.toStringAsFixed(1), 'min': numText(f.minOwnPct)})),
            KKV(t('social.created'), fmtDate(t, f.createdAt)),
          ]),
          const SizedBox(height: 12),
          GroupLabel(t('social.funds.rollovers')),
          if (rollovers.isEmpty)
            EmptyRow(t('social.funds.noRollovers'))
          else
            RowsBox(
              children: [
                for (final r in rollovers)
                  DataLine(
                    title: Text(serverTime(t, strOrNull(r['at']))),
                    trailing: Num(nav4(numOf(r['nav']))),
                    trailingSub: Num('${t('social.fees')} ${usd(numOf(r['fees']))}'),
                  ),
              ],
            ),
        ],
      ],
    );
  }
}

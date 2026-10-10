// Copy & PAMM › PAMM funds (/social/pamm): KPIs, how PAMM works, the funds as cards or a table (rollover filter),
// the fund drawer and the invest sheet. Port of the phone layout of apps/crm/components/social-live/funds.tsx
// (LivePammPage, GET funds every 30 s).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../shell/nav.dart';
import '../../ui/ui.dart';
import 'social_api.dart';
import 'widgets/bits.dart';
import 'widgets/fund_sheet.dart';
import 'widgets/invest_sheet.dart';

const List<(IconData, String, String)> _explain = [
  (LucideIcons.coins, 'social.funds.explain.navT', 'social.funds.explain.navS'),
  (LucideIcons.calendarClock, 'social.funds.explain.queueT', 'social.funds.explain.queueS'),
  (LucideIcons.trendingUp, 'social.funds.explain.hwmT', 'social.funds.explain.hwmS'),
  (LucideIcons.shieldAlert, 'social.invest.sl', 'social.funds.explain.slS'),
  (LucideIcons.snowflake, 'social.drawdownFreeze', 'social.funds.explain.freezeS'),
];

class PammScreen extends ConsumerStatefulWidget {
  const PammScreen({super.key});

  @override
  ConsumerState<PammScreen> createState() => _PammScreenState();
}

class _PammScreenState extends ConsumerState<PammScreen> {
  String _view = 'cards';
  String _roll = 'all';
  String _search = '';

  void _invest(int id) => showInvestSheet(context, fundId: id, onDone: () => ref.invalidate(fundsProvider));
  void _open(int id, bool readOnly) => showFundSheet(context, id, onInvest: readOnly ? null : _invest);

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final q = ref.watch(fundsProvider);
    final all = q.value ?? const <FundView>[];
    final loading = q.value == null && !q.hasError;
    // the master application lives under copy trading (/social/master)
    final masterOn = pageOn(ref.watch(configProvider), '/social/master');
    final funds = all.where((f) => _roll == 'all' || f.period == _roll).toList()..sort((a, b) => b.aum.compareTo(a.aum));
    final aum = all.fold<double>(0, (s, f) => s + f.aum);
    final investors = all.fold<int>(0, (s, f) => s + f.investorCount);
    final nextRoll = (all.where((f) => f.status == 'active' && f.nextRolloverAt != null).map((f) => f.nextRolloverAt!).toList()..sort()).firstOrNull;
    final lf = LocaleFormat(t.locale);
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.72;
    final s = _search.trim().toLowerCase();

    return KPageScroll(
      onRefresh: () async {
        ref.invalidate(fundsProvider);
        await ref.read(fundsProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        KPageHeader(title: t('social.funds.title'), subtitle: Text(t('social.funds.subtitle'))),
        const SizedBox(height: 14),
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: KButton(
            label: t('social.myInvestments'),
            trailingIcon: Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight,
            variant: KButtonVariant.surface,
            onPressed: () => context.go('/social/investments'),
          ),
        ),
        const SizedBox(height: 18),
        if (q.hasError && q.value == null)
          SocialErrorCard(error: q.error, onRetry: () => ref.invalidate(fundsProvider))
        else ...[
          SizedBox(
            height: 160,
            child: ListView(
              scrollDirection: Axis.horizontal,
              clipBehavior: Clip.none,
              children: [
                KKpiCard(
                  width: kpiWidth,
                  label: t('social.funds.kpi.funds'),
                  icon: LucideIcons.layoutGrid,
                  value: Num(loading ? '—' : '${all.length}', style: context.text.moneyL),
                  chip: KChip(label: t('social.funds.kpi.openCount', {'count': all.where((f) => f.status == 'active').length})),
                ),
                const SizedBox(width: 12),
                KKpiCard(
                  width: kpiWidth,
                  label: t('social.funds.kpi.totalAum'),
                  icon: LucideIcons.wallet,
                  value: Num(loading ? '—' : compactUsd(aum), style: context.text.moneyL),
                  chip: KChip(label: t('social.funds.kpi.investorCapital')),
                ),
                const SizedBox(width: 12),
                KKpiCard(
                  width: kpiWidth,
                  label: t('social.investors'),
                  icon: LucideIcons.users,
                  value: Num(loading ? '—' : lf.number(investors, 0), style: context.text.moneyL),
                  chip: KChip(label: t('social.funds.kpi.acrossAll')),
                ),
                const SizedBox(width: 12),
                KKpiCard(
                  width: kpiWidth,
                  label: t('social.nextRollover'),
                  icon: LucideIcons.calendarClock,
                  value: Text(nextRoll != null ? serverTime(t, nextRoll, withYear: false) : '—', style: context.text.title1),
                  chip: KChip(label: t('social.serverTime'), tone: KChipTone.ember),
                ),
              ],
            ),
          ),
          const SizedBox(height: 16),
          KCard(
            padding: EdgeInsets.zero,
            child: Column(
              children: [
                for (var i = 0; i < _explain.length; i++) ...[
                  if (i > 0) const KDivider(),
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 13, 16, 13),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Container(
                          width: 32,
                          height: 32,
                          decoration: BoxDecoration(
                            shape: BoxShape.circle,
                            color: k.goldSoft,
                            border: Border.all(color: k.gold.withValues(alpha: 0.3)),
                          ),
                          child: Icon(_explain[i].$1, size: 15, color: k.gold),
                        ),
                        const SizedBox(width: 12),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                t(_explain[i].$2),
                                style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
                              ),
                              const SizedBox(height: 2),
                              Text(
                                t(_explain[i].$3),
                                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, height: 1.4),
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(height: 22),
          Row(
            children: [
              Expanded(
                child: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: '${t('social.funds.count', {'count': funds.length})} '),
                      TextSpan(
                        text: t('social.funds.sortedByAum'),
                        style: TextStyle(color: k.fg3),
                      ),
                    ],
                  ),
                  style: context.text.title1.copyWith(fontSize: 18, fontWeight: FontWeight.w500),
                ),
              ),
              _ViewToggle(view: _view, onChanged: (v) => setState(() => _view = v)),
            ],
          ),
          const SizedBox(height: 10),
          KSegmented<String>(
            values: const ['all', 'daily', 'weekly', 'monthly'],
            labels: [t('social.funds.anyRollover'), t('social.period.daily'), t('social.period.weekly'), t('social.period.monthly')],
            selected: _roll,
            height: 32,
            onChanged: (v) => setState(() => _roll = v),
          ),
          const SizedBox(height: 14),
          if (loading)
            const BlockSkeleton(n: 2, h: 200)
          else if (funds.isEmpty)
            KCard(
              child: KEmptyState(
                art: all.isNotEmpty ? null : KIllustrationName.pammFunds,
                icon: LucideIcons.landmark,
                title: all.isNotEmpty ? t('social.funds.empty.filteredTitle') : t('social.funds.empty.title'),
                text: all.isNotEmpty ? t('social.funds.empty.filteredText') : t('social.funds.empty.text'),
                action: all.isEmpty && masterOn
                    ? KButton(label: t('social.becomeMaster'), icon: LucideIcons.crown, size: KButtonSize.sm, onPressed: () => context.go('/social/master'))
                    : null,
              ),
            )
          else if (_view == 'cards')
            for (final f in funds) ...[
              _FundCard(f: f, readOnly: readOnly, onInvest: () => _invest(f.id), onOpen: () => _open(f.id, readOnly)),
              const SizedBox(height: 14),
            ]
          else
            SectionCard(
              title: t('social.funds.allFunds'),
              subtitle: t('social.funds.allFundsSub'),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  KSearchField(onChanged: (v) => setState(() => _search = v)),
                  const SizedBox(height: 6),
                  RowsBox(
                    boxed: false,
                    children: [
                      for (final f in funds.where((f) => s.isEmpty || '${f.name} ${f.masterName}'.toLowerCase().contains(s)))
                        DataLine(
                          onTap: () => _open(f.id, readOnly),
                          leading: KAvatar(name: f.masterName, size: 34),
                          title: Text(f.masterName, maxLines: 1, overflow: TextOverflow.ellipsis),
                          subtitle: Row(
                            children: [
                              Flexible(child: Text(f.name, maxLines: 1, overflow: TextOverflow.ellipsis)),
                              const SizedBox(width: 6),
                              FundStatusChip(status: f.status),
                            ],
                          ),
                          trailing: Num(pct(f.returnAll, 1), color: f.returnAll >= 0 ? k.up : k.down),
                          trailingSub: Num('${nav4(f.nav)} · ${compactUsd(f.aum)} · ${numText(f.perfFeePct)}%'),
                        ),
                    ],
                  ),
                ],
              ),
            ),
        ],
        const SizedBox(height: 20),
        Text(t('social.funds.disclaimer'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12, height: 1.5)),
      ],
    );
  }
}

class _ViewToggle extends StatelessWidget {
  const _ViewToggle({required this.view, required this.onChanged});
  final String view;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    Widget item(String v, IconData icon) => KPressable(
      minSize: 36,
      onTap: () => onChanged(v),
      child: Container(
        width: 36,
        height: 30,
        decoration: BoxDecoration(color: view == v ? k.ember : Colors.transparent, borderRadius: BorderRadius.circular(15)),
        child: Icon(icon, size: 15, color: view == v ? k.onEmber : k.fg2),
      ),
    );
    return Container(
      padding: const EdgeInsets.all(2),
      decoration: BoxDecoration(
        color: k.surface.withValues(alpha: 0.82),
        borderRadius: BorderRadius.circular(17),
        border: Border.all(color: k.line),
      ),
      child: Row(mainAxisSize: MainAxisSize.min, children: [item('cards', LucideIcons.layoutGrid), item('table', LucideIcons.rows3)]),
    );
  }
}

class _FundCard extends StatelessWidget {
  const _FundCard({required this.f, required this.readOnly, required this.onInvest, required this.onOpen});
  final FundView f;
  final bool readOnly;
  final VoidCallback onInvest;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final lf = LocaleFormat(t.locale);
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return KCard(
      padding: const EdgeInsets.fromLTRB(12, 12, 12, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // a fund as a visiting card (web funds.tsx FundCard: ivory, founder 2026-10-10)
          KPressable(
            pressedScale: 0.99,
            semanticLabel: f.name,
            onTap: onOpen,
            child: KVisitingCard(
              finish: KVisitingFinish.ivory,
              kicker: 'PAMM',
              name: f.name,
              title: t('social.byName', {'name': f.masterName}),
              stats: [
                (label: t('social.navPerUnit'), value: nav4(f.nav), tone: null),
                (label: '1M', value: pct(f.return1m, 1), tone: f.return1m > 0 ? 'up' : (f.return1m < 0 ? 'down' : null)),
                (label: t('social.aum'), value: compactUsd(f.aum), tone: null),
                (label: t('social.drawdown'), value: ddText(f.drawdownPct), tone: f.drawdownPct > 0 ? 'down' : null),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: FundStatusChip(status: f.status),
          ),
          const SizedBox(height: 14),
          TileGrid(
            columns: 3,
            gap: 6,
            tiles: [
              Tile(label: t('social.aum'), value: Num(compactUsd(f.aum))),
              Tile(label: t('social.investors'), value: Num(lf.number(f.investorCount, 0))),
              Tile(
                label: t('social.returnAll'),
                value: Num(pct(f.returnAll, 1), color: f.returnAll >= 0 ? k.up : k.down),
              ),
              Tile(
                label: t('social.drawdown'),
                value: Num(ddText(f.drawdownPct), color: k.down),
              ),
              Tile(label: t('social.funds.perfFeeShort'), value: Num('${numText(f.perfFeePct)}% HWM')),
              Tile(label: t('social.min'), value: Num(usd(f.minInvestment, 0))),
            ],
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              KChip(
                label: '${periodLabel(t, f.period)} · ${serverTime(t, f.nextRolloverAt, withYear: false)}',
                tone: KChipTone.ember,
                icon: LucideIcons.calendarClock,
                small: true,
              ),
              if (f.lockInDays > 0)
                KChip(label: t('social.funds.lockInDays', {'d': f.lockInDays}), tone: KChipTone.warn, icon: LucideIcons.lock, small: true)
              else
                KChip(label: t('social.funds.noLockIn'), small: true),
              KChip(label: t('social.funds.freezeChip', {'dd': numText(f.maxDdPct)}), icon: LucideIcons.snowflake, small: true),
            ],
          ),
          const SizedBox(height: 14),
          Row(
            children: [
              Expanded(
                child: KButton(
                  label: t('common.details'),
                  trailingIcon: rtl ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  expand: true,
                  onPressed: onOpen,
                ),
              ),
              if (!readOnly) ...[
                const SizedBox(width: 8),
                Expanded(
                  child: KButton(
                    label: t('social.invest'),
                    icon: LucideIcons.wallet,
                    size: KButtonSize.sm,
                    expand: true,
                    onPressed: f.status != 'active' ? null : onInvest,
                  ),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

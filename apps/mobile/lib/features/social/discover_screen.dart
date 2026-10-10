// Copy & PAMM › Discover (/social): the leaderboard of masters with filters, sort and search, the compare tray and
// the follow / invest sheets. Port of the phone layout of apps/crm/components/social-live/leaderboard.tsx
// (LiveDiscoverPage): header + actions, hero with the totals, leaderboard card (period, sort, programme, risk,
// track record, more filters, search, rows -> master profile), disclaimer, compare bar.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'social_api.dart';
import 'widgets/bits.dart';
import 'widgets/compare_sheet.dart';
import 'widgets/follow_sheet.dart';
import 'widgets/invest_sheet.dart';

const List<String> _periods = ['1m', '3m', '1y', 'all'];
const Map<String, String> _periodKey = {'1m': 'social.lb.period.1m', '3m': 'social.lb.period.3m', '1y': 'social.lb.period.1y', 'all': 'common.all'};
const List<(int, String)> _track = [
  (0, 'social.lb.track.any'),
  (30, 'social.lb.track.30'),
  (90, 'social.lb.track.90'),
  (180, 'social.lb.track.180'),
  (365, 'social.lb.track.365'),
];
const List<int> _ddSteps = [0, 10, 20, 30];
const List<int> _feeSteps = [0, 10, 20, 30];
const List<int> _followerSteps = [0, 10, 50, 100];

double _retOf(MasterView m, String p) => switch (p) {
  '1m' => m.stats.return1m,
  '3m' => m.stats.return3m,
  '1y' => m.stats.return1y,
  _ => m.stats.returnAll,
};

class DiscoverScreen extends ConsumerStatefulWidget {
  const DiscoverScreen({super.key});

  @override
  ConsumerState<DiscoverScreen> createState() => _DiscoverScreenState();
}

class _DiscoverScreenState extends ConsumerState<DiscoverScreen> {
  String _period = '3m';
  String _sort = 'return';
  String _program = 'all';
  String _risk = 'all';
  int _trackDays = 0;
  int _maxDd = 0, _maxFee = 0, _minFollowers = 0;
  bool _openOnly = false;
  String _search = '';
  int _shown = 15;
  final List<MasterView> _picked = [];

  String get _qs {
    final q = <String, String>{'period': _period, 'program': _program, 'sort': _sort, 'risk': _risk};
    if (_trackDays > 0) q['minDays'] = '$_trackDays';
    if (_maxDd > 0) q['maxDd'] = '$_maxDd';
    if (_maxFee > 0) q['maxFee'] = '$_maxFee';
    if (_minFollowers > 0) q['minFollowers'] = '$_minFollowers';
    if (_openOnly) q['openOnly'] = 'true';
    return Uri(queryParameters: q).query;
  }

  int get _more => (_maxDd > 0 ? 1 : 0) + (_maxFee > 0 ? 1 : 0) + (_minFollowers > 0 ? 1 : 0) + (_openOnly ? 1 : 0);
  bool get _filtered => _program != 'all' || _risk != 'all' || _trackDays > 0 || _more > 0;

  void _clearMore() => setState(() {
    _maxDd = 0;
    _maxFee = 0;
    _minFollowers = 0;
    _openOnly = false;
  });

  void _togglePick(MasterView m) {
    final t = context.t;
    if (_picked.any((x) => x.id == m.id)) {
      setState(() => _picked.removeWhere((x) => x.id == m.id));
      return;
    }
    if (_picked.length >= compareMax) return plainError(ref, t('social.compare.max', {'n': compareMax}));
    setState(() => _picked.add(m));
  }

  Future<void> _pickTrack() async {
    final t = context.t;
    final v = await showKPicker<int>(
      context,
      title: t('social.lb.track.any'),
      selected: _trackDays,
      options: [for (final (d, key) in _track) KPickOption(d, t(key))],
    );
    if (v != null) setState(() => _trackDays = v);
  }

  Future<void> _moreFilters() async {
    await showKSheet<void>(
      context,
      title: context.t('social.lb.more.title'),
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, setLocal) {
          final t = ctx.t;
          void upd(VoidCallback f) {
            setState(f);
            setLocal(() {});
          }

          List<String> steps(List<int> s, String Function(int v) label) => [for (final v in s) v == 0 ? t('social.lb.more.any') : label(v)];
          return KSheetContent(
            children: [
              GroupLabel(t('social.lb.more.maxDd')),
              KSegmented<int>(plain: true, values: _ddSteps, labels: steps(_ddSteps, (v) => '≤$v%'), selected: _maxDd, onChanged: (v) => upd(() => _maxDd = v)),
              const SizedBox(height: 16),
              GroupLabel(t('social.lb.more.maxFee')),
              KSegmented<int>(
                plain: true,
                values: _feeSteps,
                labels: steps(_feeSteps, (v) => '≤$v%'),
                selected: _maxFee,
                onChanged: (v) => upd(() => _maxFee = v),
              ),
              const SizedBox(height: 16),
              GroupLabel(t('social.lb.more.minFollowers')),
              KSegmented<int>(
                plain: true,
                values: _followerSteps,
                labels: steps(_followerSteps, (v) => '$v+'),
                selected: _minFollowers,
                onChanged: (v) => upd(() => _minFollowers = v),
              ),
              const SizedBox(height: 16),
              Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(t('social.lb.more.openOnly'), style: ctx.text.label.copyWith(color: ctx.k.fg2)),
                        Text(t('social.lb.more.openOnlyHint'), style: ctx.text.footnote.copyWith(color: ctx.k.fg3, fontSize: 12)),
                      ],
                    ),
                  ),
                  KSwitch(value: _openOnly, onChanged: (v) => upd(() => _openOnly = v), semanticLabel: t('social.lb.more.openOnly')),
                ],
              ),
              if (_more > 0) ...[
                const SizedBox(height: 14),
                Align(
                  alignment: AlignmentDirectional.centerStart,
                  child: KButton(
                    label: t('social.clearFilters'),
                    icon: LucideIcons.x,
                    variant: KButtonVariant.ghost,
                    size: KButtonSize.sm,
                    onPressed: () {
                      _clearMore();
                      setLocal(() {});
                    },
                  ),
                ),
              ],
            ],
          );
        },
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    // the PAMM filter goes with the PAMM module
    final pammOn = ref.watch(configProvider).moduleOn('pamm');
    if (!pammOn && _program == 'pamm') _program = 'all';
    final provider = leaderboardProvider(_qs);
    final q = ref.watch(provider);
    final data = q.value;
    final rows = data?.items ?? const <MasterView>[];
    final totals = data?.totals;
    final f = LocaleFormat(t.locale);
    final s = _search.trim().toLowerCase();
    final found = s.isEmpty ? rows : rows.where((m) => '${m.nickname} ${m.strategy}'.toLowerCase().contains(s)).toList();
    final anyHouse = rows.any((m) => m.house);

    final hero = pageHero(
      context,
      ref,
      path: '/social',
      title: t('social.lb.title'),
      lead: t('social.lb.subtitle'),
      actions: [
        KHeroButton(label: t('social.mySubscriptions'), icon: LucideIcons.repeat, onPressed: () => context.go('/social/copy')),
        KHeroButton(label: t('social.becomeMaster'), icon: LucideIcons.crown, primary: true, onPressed: () => context.go('/social/master')),
      ],
    );
    final page = KPageScroll(
      onRefresh: () async {
        ref.invalidate(provider);
        await ref.read(provider.future).then((_) {}, onError: (Object _) {});
      },
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, _picked.isNotEmpty ? 96 : 24),
      children: [
        if (hero == null) ...[
          KPageHeader(title: t('social.lb.title'), subtitle: Text(t('social.lb.subtitle'))),
          const SizedBox(height: 14),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              KButton(
                label: t('social.mySubscriptions'),
                icon: LucideIcons.repeat,
                variant: KButtonVariant.surface,
                onPressed: () => context.go('/social/copy'),
              ),
              KButton(label: t('social.becomeMaster'), icon: LucideIcons.crown, onPressed: () => context.go('/social/master')),
            ],
          ),
          const SizedBox(height: 18),
        ],
        // hero
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              KChip(label: anyHouse ? t('social.lb.hero.chipClient') : t('social.lb.hero.chipAll'), tone: KChipTone.ember, icon: LucideIcons.shieldCheck),
              const SizedBox(height: 12),
              Text(t('social.lb.hero.title'), style: context.text.title1.copyWith(fontWeight: FontWeight.w500)),
              const SizedBox(height: 6),
              Text(t('social.lb.hero.text'), style: context.text.callout.copyWith(color: k.fg2)),
              const SizedBox(height: 14),
              TileGrid(
                tiles: [
                  for (final (icon, label, value) in [
                    (LucideIcons.crown, t('social.lb.stat.masters'), totals == null ? null : f.number(intOf(totals['masters']), 0)),
                    (LucideIcons.wallet, t('social.lb.stat.aum'), totals == null ? null : compactUsd(numOf(totals['aum']))),
                    (LucideIcons.users, t('social.lb.stat.followers'), totals == null ? null : f.number(intOf(totals['followers']), 0)),
                    (LucideIcons.landmark, t('social.lb.stat.investors'), totals == null ? null : f.number(intOf(totals['investors']), 0)),
                  ])
                    _HeroStat(icon: icon, label: label, value: value, failed: q.hasError),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        if (q.hasError && data == null)
          SocialErrorCard(error: q.error, onRetry: () => ref.invalidate(provider))
        else
          KCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                KCardHeader(
                  title: t('social.lb.leaderboard'),
                  subtitle: data != null ? t('social.lb.leaderboardSub', {'count': rows.length}) : t('common.loading'),
                  icon: LucideIcons.trophy,
                ),
                const SizedBox(height: 12),
                KSegmented<String>(
                  values: _periods,
                  labels: [for (final p in _periods) t(_periodKey[p]!)],
                  selected: _period,
                  height: 32,
                  onChanged: (v) => setState(() => _period = v),
                ),
                const SizedBox(height: 10),
                KChoiceChips<String>(
                  values: const ['return', 'dd', 'aum', 'followers', 'age'],
                  labels: [t('social.lb.sort.return'), t('social.lb.sort.dd'), t('social.aum'), t('social.followers'), t('social.lb.sort.age')],
                  selected: _sort,
                  onChanged: (v) => setState(() => _sort = v),
                ),
                const SizedBox(height: 6),
                KSegmented<String>(
                  plain: true,
                  height: 32,
                  values: ['all', 'copy', if (pammOn) 'pamm'],
                  labels: [t('common.all'), t('social.program.copy'), if (pammOn) 'PAMM'],
                  selected: _program,
                  onChanged: (v) => setState(() => _program = v),
                ),
                const SizedBox(height: 8),
                KSegmented<String>(
                  plain: true,
                  height: 32,
                  values: const ['all', 'low', 'med', 'high'],
                  labels: [t('social.lb.anyRisk'), '1–3', '4–6', '7–10'],
                  selected: _risk,
                  onChanged: (v) => setState(() => _risk = v),
                ),
                const SizedBox(height: 8),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    KButton(
                      label: t(_track.firstWhere((x) => x.$1 == _trackDays).$2),
                      trailingIcon: LucideIcons.chevronDown,
                      variant: KButtonVariant.surface,
                      size: KButtonSize.sm,
                      onPressed: _pickTrack,
                    ),
                    KButton(
                      label: _more > 0 ? '${t('social.lb.more.title')} · $_more' : t('social.lb.more.title'),
                      icon: LucideIcons.slidersHorizontal,
                      variant: _more > 0 ? KButtonVariant.outline : KButtonVariant.surface,
                      size: KButtonSize.sm,
                      onPressed: _moreFilters,
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                if (q.isLoading && data == null)
                  Column(
                    children: [
                      for (var i = 0; i < 5; i++) ...[if (i > 0) const SizedBox(height: 8), const KSkeleton(height: 56, radius: 14)],
                    ],
                  )
                else if (rows.isEmpty)
                  KEmptyState(
                    compact: true,
                    art: _filtered ? null : KIllustrationName.copyTrading,
                    icon: LucideIcons.trophy,
                    title: _filtered ? t('social.lb.empty.filteredTitle') : t('social.lb.empty.title'),
                    text: _filtered ? t('social.lb.empty.filteredText') : t('social.lb.empty.text'),
                    action: _filtered
                        ? KButton(
                            label: t('social.clearFilters'),
                            variant: KButtonVariant.surface,
                            size: KButtonSize.sm,
                            onPressed: () => setState(() {
                              _program = 'all';
                              _risk = 'all';
                              _trackDays = 0;
                              _maxDd = 0;
                              _maxFee = 0;
                              _minFollowers = 0;
                              _openOnly = false;
                            }),
                          )
                        : KButton(
                            label: t('social.becomeMaster'),
                            icon: LucideIcons.crown,
                            size: KButtonSize.sm,
                            onPressed: () => context.go('/social/master'),
                          ),
                  )
                else ...[
                  KSearchField(placeholder: t('social.lb.searchPlaceholder'), onChanged: (v) => setState(() => _search = v)),
                  const SizedBox(height: 6),
                  if (found.isEmpty)
                    Padding(
                      padding: const EdgeInsets.symmetric(vertical: 20),
                      child: Text(
                        t('common.noResults'),
                        textAlign: TextAlign.center,
                        style: context.text.footnote.copyWith(color: k.fg3),
                      ),
                    ),
                  for (var i = 0; i < found.length && i < _shown; i++) ...[
                    _MasterRow(
                      m: found[i],
                      rank: rows.indexOf(found[i]) + 1,
                      period: _period,
                      sort: _sort,
                      picked: _picked.any((x) => x.id == found[i].id),
                      readOnly: readOnly,
                      onPick: () => _togglePick(found[i]),
                    ),
                  ],
                  if (found.length > _shown)
                    Center(
                      child: KTextButton(label: t('common.showMore'), onPressed: () => setState(() => _shown += 15)),
                    ),
                ],
              ],
            ),
          ),
        const SizedBox(height: 20),
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 2),
              child: Icon(LucideIcons.chartLine, size: 14, color: k.fg3),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: Text(t('social.lb.disclaimer'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12, height: 1.5)),
            ),
          ],
        ),
        if (anyHouse)
          Padding(
            padding: const EdgeInsetsDirectional.only(start: 22, top: 8),
            child: Text(t('social.lb.houseNote'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12, height: 1.5)),
          ),
      ],
    );

    return Stack(
      children: [
        Positioned.fill(child: page),
        if (_picked.isNotEmpty)
          PositionedDirectional(
            start: 16,
            end: 16,
            bottom: MediaQuery.paddingOf(context).bottom + 10,
            child: _CompareBar(
              count: _picked.length,
              onClear: () => setState(_picked.clear),
              onCompare: () => showCompareSheet(context, masters: [..._picked], onRemove: (id) => setState(() => _picked.removeWhere((x) => x.id == id))),
            ),
          ),
      ],
    );
  }
}

class _HeroStat extends StatelessWidget {
  const _HeroStat({required this.icon, required this.label, required this.value, required this.failed});
  final IconData icon;
  final String label;
  final String? value;
  final bool failed;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(icon, size: 13, color: k.fg3),
              const SizedBox(width: 5),
              Expanded(
                child: Text(
                  label,
                  maxLines: 2,
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ),
            ],
          ),
          const SizedBox(height: 4),
          if (value != null)
            Num(value!, style: context.text.title1.copyWith(fontSize: 19))
          else if (failed)
            Text('—', style: context.text.title1.copyWith(fontSize: 19))
          else
            const KSkeleton(width: 64, height: 22),
        ],
      ),
    );
  }
}

class _MasterRow extends StatelessWidget {
  const _MasterRow({
    required this.m,
    required this.rank,
    required this.period,
    required this.sort,
    required this.picked,
    required this.readOnly,
    required this.onPick,
  });
  final MasterView m;
  final int rank;
  final String period;
  final String sort;
  final bool picked;
  final bool readOnly;
  final VoidCallback onPick;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final r = _retOf(m, period);
    final canInvest = m.fund != null && m.program != 'copy' && m.fund!.status == 'active';
    final canCopy = m.program != 'pamm' && !m.frozen;
    // the board as visiting cards (web leaderboard.tsx, founder 2026-10-10): black for copy masters, ivory for PAMM
    final ivory = m.program == 'pamm';
    final program = m.program == 'pamm' ? 'PAMM' : (m.program == 'both' && m.fund != null ? '${t('social.program.copy')} · PAMM' : t('social.program.copy'));
    final risk = m.stats.riskScore.round().clamp(1, 10);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KPressable(
            pressedScale: 0.99,
            semanticLabel: m.nickname,
            onTap: () => context.push('/social/masters/${m.id}'),
            child: KVisitingCard(
              finish: ivory ? KVisitingFinish.ivory : KVisitingFinish.black,
              kicker: '#$rank · $program',
              name: m.nickname,
              title: m.strategy,
              badges: [if (m.house) t('social.house.badge'), if (m.program != 'pamm' && m.acceptingNew == false) t('social.lb.closedChip')],
              // pick up to three to compare
              corner: KPressable(
                minSize: 36,
                semanticLabel: picked ? t('social.compare.remove', {'name': m.nickname}) : t('social.compare.add'),
                onTap: onPick,
                child: AnimatedContainer(
                  duration: const Duration(milliseconds: 150),
                  width: 26,
                  height: 26,
                  decoration: BoxDecoration(
                    color: picked ? k.ember : (ivory ? Colors.black.withValues(alpha: 0.05) : Colors.white.withValues(alpha: 0.1)),
                    shape: BoxShape.circle,
                    border: Border.all(color: picked ? k.ember : (ivory ? Colors.black.withValues(alpha: 0.2) : Colors.white.withValues(alpha: 0.25))),
                  ),
                  child: picked ? Icon(LucideIcons.check, size: 15, color: k.onEmber) : null,
                ),
              ),
              stats: [
                (label: t('social.lb.col.return', {'period': t(_periodKey[period]!)}), value: pct(r, 1), tone: r > 0 ? 'up' : (r < 0 ? 'down' : null)),
                (label: t('social.maxDd'), value: ddText(m.stats.maxDd), tone: m.stats.maxDd > 0 ? 'down' : null),
                (label: t('social.followers'), value: '${m.stats.followers}', tone: null),
                (label: t('social.risk'), value: '$risk/10', tone: null),
              ],
            ),
          ),
          if (m.house) ...[const SizedBox(height: 8), const HouseBadge()],
          if (!readOnly && (canCopy || canInvest)) ...[
            const SizedBox(height: 10),
            Row(
              children: [
                if (canCopy)
                  Expanded(
                    child: KButton(
                      label: t('social.program.copy'),
                      icon: LucideIcons.repeat,
                      size: KButtonSize.sm,
                      expand: true,
                      variant: m.acceptingNew == false ? KButtonVariant.surface : KButtonVariant.ember,
                      onPressed: m.acceptingNew == false ? null : () => showFollowSheet(context, master: m),
                    ),
                  ),
                if (canCopy && canInvest) const SizedBox(width: 8),
                if (canInvest)
                  Expanded(
                    child: KButton(
                      label: t('social.invest'),
                      icon: LucideIcons.wallet,
                      variant: KButtonVariant.surface,
                      size: KButtonSize.sm,
                      expand: true,
                      onPressed: () => showInvestSheet(context, fundId: m.fund!.id),
                    ),
                  ),
                const SizedBox(width: 4),
                KButton(
                  label: t('common.details'),
                  variant: KButtonVariant.ghost,
                  size: KButtonSize.sm,
                  onPressed: () => context.push('/social/masters/${m.id}'),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

class _CompareBar extends StatelessWidget {
  const _CompareBar({required this.count, required this.onClear, required this.onCompare});
  final int count;
  final VoidCallback onClear;
  final VoidCallback onCompare;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return Center(
      child: KFrosted(
        color: k.surface,
        borderRadius: BorderRadius.circular(28),
        border: Border.all(color: k.line),
        child: Padding(
          padding: const EdgeInsetsDirectional.fromSTEB(16, 6, 6, 6),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(t('social.compare.picked', {'n': count, 'max': compareMax}), style: context.text.footnote.copyWith(color: k.fg3)),
              const SizedBox(width: 4),
              KIconButton(icon: LucideIcons.x, size: 32, semanticLabel: t('social.compare.clear'), onPressed: onClear),
              const SizedBox(width: 4),
              KButton(
                label: t('social.compare.button', {'n': count}),
                icon: LucideIcons.columns3,
                size: KButtonSize.sm,
                onPressed: count < 2 ? null : onCompare,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

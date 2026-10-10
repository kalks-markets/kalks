// Contests & Rewards › Contests (/rewards). Port of the web's LiveContestsPage (apps/crm/components/growth/contests.tsx)
// in its phone order:
//   header (Contests + My results)  ·  banner slot  ·  featured contest (or "No contest running")  ·  KPI cards
//   ·  live leaderboard (top 10 + podium)  ·  prize distribution  ·  your rewards shortcuts  ·  open for entry
//   ·  past contests  ·  my results
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../data/client_data.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'rewards_api.dart';
import 'widgets/contest_widgets.dart';
import 'widgets/growth_ui.dart';

/// The featured contest: a running one (joined first, then the biggest pool), else the next to start.
ContestCard? pickFeatured(List<ContestCard> items) {
  final running = items.where((c) => c.contest.running).toList()
    ..sort((a, b) {
      final j = (b.myEntry != null ? 1 : 0) - (a.myEntry != null ? 1 : 0);
      return j != 0 ? j : b.contest.prizePool.compareTo(a.contest.prizePool);
    });
  if (running.isNotEmpty) return running.first;
  final upcoming = items.where((c) => c.contest.upcoming).toList()..sort((a, b) => a.contest.startsAt.compareTo(b.contest.startsAt));
  return upcoming.isEmpty ? null : upcoming.first;
}

class ContestsScreen extends ConsumerStatefulWidget {
  const ContestsScreen({super.key});

  @override
  ConsumerState<ContestsScreen> createState() => _ContestsScreenState();
}

class _ContestsScreenState extends ConsumerState<ContestsScreen> {
  final _scroll = ScrollController();
  final _resultsKey = GlobalKey();

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _reload([String? featuredId]) {
    ref.invalidate(contestsProvider);
    if (featuredId != null) ref.invalidate(contestDetailProvider(featuredId));
    // the dashboard's accounts (a new contest account) and points
    ref.invalidate(accountsProvider);
  }

  Future<void> _refresh(String? featuredId) async {
    _reload(featuredId);
    ref
      ..invalidate(growthRewardsProvider)
      ..invalidate(cashbackProvider)
      ..invalidate(bannersProvider('rewards'));
    await ref.read(contestsProvider.future).then((_) {}, onError: (Object _) {});
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final title = t('rewards.contests.title');
    final subtitle = t('rewards.contests.subtitle');
    final list = ref.watch(contestsProvider);
    final hero = pageHero(context, ref, path: '/rewards', title: title, lead: subtitle);
    if (!list.hasValue) {
      return GrowthFallback(
        hero: hero,
        title: title,
        subtitle: subtitle,
        error: list.hasError ? list.error : null,
        onRetry: () => ref.invalidate(contestsProvider),
        heights: const [320, 150],
      );
    }
    final data = list.requireValue;
    final items = data.items;
    final stats = data.stats;
    final featured = pickFeatured(items);
    final fid = featured == null ? null : '${featured.contest.id}';
    final detail = fid == null ? null : ref.watch(contestDetailProvider(fid)).value;
    final d = detail != null && detail.contest.id == featured!.contest.id ? detail : null;
    final upcoming = items.where((c) => c != featured && (c.contest.upcoming || c.contest.running)).toList();
    final past = items.where((c) => c.contest.past && (c.contest.status != 'cancelled' || c.myEntry != null)).take(8).toList();

    final width = MediaQuery.sizeOf(context).width;
    final kpiWidth = (width - 2 * KSpace.page) * 0.78;
    void reload() => _reload(fid);

    return KPageScroll(
      controller: _scroll,
      onRefresh: () => _refresh(fid),
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        if (hero == null) ...[KPageHeader(title: title, subtitle: Text(subtitle)), const SizedBox(height: 12)],
        Row(
          children: [
            KButton(
              label: t('rewards.results.title'),
              icon: LucideIcons.medal,
              variant: KButtonVariant.surface,
              onPressed: () => scrollToKey(_scroll, _resultsKey),
            ),
          ],
        ),
        const SizedBox(height: 20),
        const BannerSlot(placement: 'rewards'),
        if (featured != null)
          ContestHero(card: featured, detail: d, onJoined: reload)
        else
          KCard(
            child: KEmptyState(art: KIllustrationName.rewards, title: t('rewards.contests.noneTitle'), text: t('rewards.contests.noneText')),
          ),
        const SizedBox(height: kBlockGap),
        SizedBox(
          height: 170,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            physics: const PageScrollPhysics(parent: BouncingScrollPhysics()),
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.contests.kpiEntered'),
                icon: LucideIcons.medal,
                value: Text('${stats.entered}', textDirection: TextDirection.ltr),
                chip: KChip(label: t('rewards.contests.kpiPrizeFinishes', {'count': stats.prizeFinishes}), tone: KChipTone.gold),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.contests.kpiPrizesWon'),
                icon: LucideIcons.gift,
                value: KMoney(stats.prizesWon, style: context.text.moneyL),
                chip: KChip(label: t('rewards.contests.kpiPaidToWallet'), tone: KChipTone.up),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.contests.kpiBest'),
                icon: LucideIcons.trophy,
                value: Text(stats.bestRank != null ? '#${stats.bestRank}' : '—', textDirection: TextDirection.ltr),
                chip: KChip(label: stats.bestRank != null ? t('rewards.contests.kpiAcross') : t('rewards.contests.kpiNotRanked')),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.contests.kpiActive'),
                icon: LucideIcons.flame,
                value: Text('${stats.active}', textDirection: TextDirection.ltr),
                chip: KChip(label: t('rewards.contests.kpiUpcoming', {'count': items.where((c) => c.contest.upcoming).length}), tone: KChipTone.ember),
              ),
            ],
          ),
        ),
        if (featured != null) ...[
          const SizedBox(height: kBlockGap),
          if (d != null) Leaderboard(d: d, limit: 10) else const KSkeletonCard(height: 360, lines: 6),
          const SizedBox(height: kBlockGap),
          PrizeCard(c: featured.contest),
          const SizedBox(height: kBlockGap),
          const RewardsShortcuts(),
        ],
        if (upcoming.isNotEmpty) ...[
          const SizedBox(height: 28),
          GrowthSectionTitle(title: t('rewards.contests.openTitle'), text: t('rewards.contests.openText')),
          for (final c in upcoming) ...[ContestTile(card: c, onJoined: reload), const SizedBox(height: kBlockGap)],
        ],
        if (past.isNotEmpty) ...[
          SizedBox(height: upcoming.isNotEmpty ? 12 : 28),
          GrowthSectionTitle(title: t('rewards.contests.pastTitle'), text: t('rewards.contests.pastText')),
          for (final c in past) ...[ContestTile(card: c, onJoined: reload), const SizedBox(height: kBlockGap)],
        ],
        SizedBox(height: past.isNotEmpty || upcoming.isNotEmpty ? 12 : 28),
        KeyedSubtree(
          key: _resultsKey,
          child: MyResults(items: items),
        ),
      ],
    );
  }
}

// Dashboard › News (web components/news-live/news-page.tsx LiveNewsPage, phone order — the xl grid collapses):
//   1 header: Market news, "{count} stories in the last 48 hours …", positive / negative chips, refresh
//   2 the featured story (pinned first, else the most important) — skeleton / empty card
//   3 Today's market brief (AI): mood, headline, points, calendar note, instruments to watch, disclaimer
//   4 News around the world: dotted map with pins (tap filters by country) + country chips
//   5 Most mentioned (top 5 -> Kalks Trader) + market sessions
//   6 filters: categories, sentiment toggles, the country filter, "{n} stories"
//   7 story cards (cover, sentiment, category, pinned, save), then "Load older stories"
//   story sheet: source, time, tags, title, summary, instruments, read the full story, Save · Copy link · Trade
// The reading list (saved stories) stays on the device like the web's localStorage.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../core/format/format.dart';
import '../../core/notifications/notifications.dart';
import '../../core/prefs.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import '../markets/instruments.dart';
import '../markets/markets_feed.dart';
import 'news_api.dart';
import 'widgets/news_bits.dart';
import 'widgets/world_map.dart';

const List<String> _cats = ['all', 'macro', 'forex', 'metals', 'indices', 'energies', 'crypto', 'stocks'];

/// The reading list (story ids), kept on the device (web localStorage `kalks.news.saved`).
class SavedNews extends Notifier<List<int>> {
  @override
  List<int> build() {
    final j = ref.read(prefsProvider).featureJson('news.saved');
    return [
      for (final x in (j?['ids'] as List? ?? const []))
        if (x is num) x.toInt(),
    ].take(200).toList();
  }

  /// Adds or removes `id`; true when it is now saved.
  bool toggle(int id) {
    final on = state.contains(id);
    state = on
        ? [
            for (final x in state)
              if (x != id) x,
          ]
        : [id, ...state].take(200).toList();
    unawaited(ref.read(prefsProvider).setFeatureJson('news.saved', {'ids': state}));
    return !on;
  }
}

final savedNewsProvider = NotifierProvider<SavedNews, List<int>>(SavedNews.new);

void _toggleSaved(WidgetRef ref, T t, int id) {
  final on = ref.read(savedNewsProvider.notifier).toggle(id);
  ref.read(notificationsProvider.notifier).toast(NotificationKind.success, on ? t('news.saved.added') : t('news.saved.removed'), keep: false);
}

class NewsScreen extends ConsumerStatefulWidget {
  const NewsScreen({super.key});

  @override
  ConsumerState<NewsScreen> createState() => _NewsScreenState();
}

class _NewsScreenState extends ConsumerState<NewsScreen> {
  String _cls = 'all';
  final Set<Sentiment> _sent = {};
  String? _country;
  final List<NewsItem> _more = [];
  String? _next;
  bool _loadingMore = false;
  NewsFeed? _feedSeen;

  String get _query {
    final q = <String, String>{'limit': '36'};
    if (_cls != 'all') q['category'] = _cls;
    if (_country != null) q['country'] = _country!;
    if (_sent.length == 1) q['sentiment'] = _sent.first.name;
    return Uri(queryParameters: q).query;
  }

  Future<void> _loadMore() async {
    final before = _next;
    if (before == null) return;
    setState(() => _loadingMore = true);
    try {
      final r = await loadOlderNews(ref, _query, before);
      if (!mounted) return;
      setState(() {
        _more.addAll(r.items);
        _next = r.next;
      });
    } catch (_) {
      if (mounted) ref.read(notificationsProvider.notifier).toast(NotificationKind.error, context.t('news.list.loadMoreError'));
    } finally {
      if (mounted) setState(() => _loadingMore = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final feedAsync = ref.watch(newsFeedProvider(_query));
    final map = ref.watch(newsMapProvider).value;
    final saved = ref.watch(savedNewsProvider);
    final feed = feedAsync.value;
    // a fresh first page resets "load more" (web: useEffect on feed.data)
    if (feed != null && !identical(feed, _feedSeen)) {
      _feedSeen = feed;
      _more.clear();
      _next = feed.next;
    }

    final all = [...?feed?.items, ..._more].where((n) => _sent.isEmpty || _sent.contains(n.sentiment)).toList();
    final pinned = feed?.pinned ?? const <NewsItem>[];
    NewsItem? featured = pinned.firstOrNull;
    if (featured == null && all.isNotEmpty) {
      final sorted = [...all]..sort((a, b) => b.importance != a.importance ? b.importance.compareTo(a.importance) : b.publishedAt.compareTo(a.publishedAt));
      featured = sorted.first;
    }
    final list = [...pinned.where((p) => p.id != featured?.id), ...all.where((n) => n.id != featured?.id)];
    final pos = map?.countries.fold<int>(0, (s, c) => s + c.bullish) ?? 0;
    final neg = map?.countries.fold<int>(0, (s, c) => s + c.bearish) ?? 0;
    final pins = [
      for (final c in map?.countries ?? const <MapCountry>[])
        MapPin(
          country: c.country,
          count: c.count,
          label: t.dyn('news.country.${c.country}', fallback: c.name.isNotEmpty ? c.name : (kCountryName[c.country] ?? c.country.toUpperCase())),
          tone: c.sentiment > 0.2 ? 'up' : (c.sentiment < -0.2 ? 'down' : 'ember'),
        ),
    ];
    final heat = {
      for (final c in map?.countries ?? const <MapCountry>[])
        if (c.count > 0 && c.sentiment != 0) c.country: (c.sentiment * (c.count / 3).clamp(0.0, 1.0)).clamp(-1.0, 1.0),
    };
    void filterCountry(String c) => setState(() => _country = _country == c ? null : c);

    return KPageScroll(
      onRefresh: () async {
        ref
          ..invalidate(newsFeedProvider(_query))
          ..invalidate(newsMapProvider)
          ..invalidate(newsBriefProvider);
        await ref.read(newsFeedProvider(_query).future).then((_) {}, onError: (Object _) {});
      },
      children: [
        // 1. header
        KPageHeader(title: t('news.page.title'), subtitle: Text(map != null ? t('news.page.subtitleCount', {'count': map.total}) : t('news.page.subtitle'))),
        const SizedBox(height: 12),
        Row(
          children: [
            KChip(label: t('news.page.positive', {'count': pos}), tone: KChipTone.up),
            const SizedBox(width: 8),
            KChip(label: t('news.page.negative', {'count': neg}), tone: KChipTone.down),
            const Spacer(),
            KIconButton(
              icon: LucideIcons.refreshCw,
              filled: true,
              size: 34,
              semanticLabel: t('common.refresh'),
              onPressed: () => ref
                ..invalidate(newsFeedProvider(_query))
                ..invalidate(newsMapProvider),
            ),
          ],
        ),
        const SizedBox(height: 16),
        // 2. featured
        if (feed == null && !feedAsync.hasError)
          const KSkeleton(height: 320, radius: 22)
        else if (featured != null)
          _FeaturedCard(n: featured, onOpen: () => showStorySheet(context, featured!))
        else
          KCard(
            child: SizedBox(
              height: 200,
              child: Center(
                child: Text(
                  feedAsync.hasError ? errorText(feedAsync.error, t) : t('news.page.empty'),
                  textAlign: TextAlign.center,
                  style: context.text.callout.copyWith(color: k.fg3),
                ),
              ),
            ),
          ),
        const SizedBox(height: 16),
        // 3. brief
        const _BriefCard(),
        const SizedBox(height: 16),
        // 4. world
        KCard(
          padding: const EdgeInsets.fromLTRB(18, 18, 18, 18),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(
                title: t('news.world.title'),
                subtitle: t('news.world.subtitle'),
                action: KChip(label: t('news.world.countries', {'count': pins.length}), tone: KChipTone.ember),
              ),
              const SizedBox(height: 10),
              WorldMap(pins: pins, heat: heat, onPin: (p) => filterCountry(p.country)),
              const SizedBox(height: 10),
              Wrap(
                spacing: 6,
                runSpacing: 6,
                children: [
                  for (final p in pins)
                    KPressable(
                      minSize: 32,
                      onTap: () => filterCountry(p.country),
                      child: Container(
                        height: 24,
                        padding: const EdgeInsets.symmetric(horizontal: 8),
                        decoration: BoxDecoration(
                          color: _country == p.country ? k.emberSoft : k.surface2,
                          borderRadius: BorderRadius.circular(12),
                          border: Border.all(color: _country == p.country ? k.ember.withValues(alpha: 0.4) : k.line),
                        ),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            KFlag(p.country, size: 12),
                            const SizedBox(width: 5),
                            Text(p.label, style: context.text.caption.copyWith(fontSize: 11, color: _country == p.country ? k.ember : k.fg2)),
                            const SizedBox(width: 5),
                            Text(
                              '${p.count}',
                              style: context.text.caption.copyWith(fontSize: 11, color: k.fg3, fontFeatures: kTabular),
                            ),
                          ],
                        ),
                      ),
                    ),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        // 5. most mentioned + sessions
        KCard(
          padding: const EdgeInsets.fromLTRB(18, 18, 18, 18),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('news.mentions.title'), subtitle: t('news.mentions.subtitle')),
              const SizedBox(height: 10),
              for (final (i, m) in (map?.mentions ?? const []).take(5).indexed)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: KPressable(
                    pressedScale: 0.99,
                    onTap: () => context.push('/trader?symbol=${m.symbol}'),
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
                      decoration: BoxDecoration(
                        color: k.surface2,
                        borderRadius: BorderRadius.circular(k.rowRadius),
                        border: Border.all(color: k.line),
                      ),
                      child: Row(
                        children: [
                          SizedBox(
                            width: 16,
                            child: Text(
                              '${i + 1}',
                              style: context.text.caption.copyWith(color: k.fg3, fontFeatures: kTabular),
                            ),
                          ),
                          SymbolAvatar(m.symbol, size: 22),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Text(
                              m.symbol,
                              style: context.text.label.copyWith(fontWeight: FontWeight.w600, color: k.fg),
                            ),
                          ),
                          Text(
                            t('news.stories', {'count': m.count}),
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
              if (map != null && map.mentions.isEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 20),
                  child: Text(
                    t('news.mentions.empty'),
                    textAlign: TextAlign.center,
                    style: context.text.footnote.copyWith(color: k.fg3),
                  ),
                ),
              const SizedBox(height: 12),
              const MarketSessions(),
            ],
          ),
        ),
        const SizedBox(height: 22),
        // 6. filters
        KChoiceChips<String>(
          values: _cats,
          labels: [for (final c in _cats) c == 'all' ? t('common.all') : (c == 'macro' ? t('news.category.macroShort') : categoryLabel(c, t))],
          selected: _cls,
          onChanged: (v) => setState(() => _cls = v),
        ),
        const SizedBox(height: 4),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            for (final s in Sentiment.values)
              _SentToggle(s: s, on: _sent.contains(s), onTap: () => setState(() => _sent.contains(s) ? _sent.remove(s) : _sent.add(s))),
            if (_country != null)
              KPressable(
                minSize: 36,
                onTap: () => setState(() => _country = null),
                child: Container(
                  height: 32,
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  decoration: BoxDecoration(
                    color: k.emberSoft,
                    borderRadius: BorderRadius.circular(16),
                    border: Border.all(color: k.ember.withValues(alpha: 0.4)),
                  ),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      KFlag(_country!, size: 14),
                      const SizedBox(width: 6),
                      Text(countryName(_country!, t), style: context.text.caption.copyWith(color: k.ember, fontSize: 12)),
                      const SizedBox(width: 6),
                      Icon(LucideIcons.x, size: 14, color: k.ember),
                    ],
                  ),
                ),
              ),
          ],
        ),
        const SizedBox(height: 6),
        Text(t('news.stories', {'count': list.length}), style: context.text.footnote.copyWith(color: k.fg3)),
        const SizedBox(height: 12),
        // 7. the stories
        if (feed == null && !feedAsync.hasError)
          for (var i = 0; i < 3; i++) ...[if (i > 0) const SizedBox(height: 14), const KSkeleton(height: 260, radius: 22)]
        else if (list.isEmpty)
          KCard(
            child: Padding(
              padding: const EdgeInsets.symmetric(vertical: 40),
              child: Text(
                feedAsync.hasError ? errorText(feedAsync.error, t) : t('news.list.empty'),
                textAlign: TextAlign.center,
                style: context.text.callout.copyWith(color: k.fg3),
              ),
            ),
          )
        else
          for (var i = 0; i < list.length; i++) ...[
            if (i > 0) const SizedBox(height: 14),
            _StoryCard(n: list[i], saved: saved.contains(list[i].id), onOpen: () => showStorySheet(context, list[i])),
          ],
        if (_next != null) ...[
          const SizedBox(height: 18),
          Center(
            child: KButton(
              label: _loadingMore ? t('common.loading') : t('news.list.loadMore'),
              variant: KButtonVariant.surface,
              loading: _loadingMore,
              onPressed: _loadMore,
            ),
          ),
        ],
      ],
    );
  }
}

class _SentToggle extends StatelessWidget {
  const _SentToggle({required this.s, required this.on, required this.onTap});
  final Sentiment s;
  final bool on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final (bg, fg, border) = on
        ? switch (s) {
            Sentiment.bullish => (k.upSoft, k.up, k.up.withValues(alpha: 0.4)),
            Sentiment.bearish => (k.downSoft, k.down, k.down.withValues(alpha: 0.4)),
            Sentiment.neutral => (k.surface3, k.fg, k.fg3.withValues(alpha: 0.4)),
          }
        : (k.surface2, k.fg3, k.line);
    return KPressable(
      minSize: 36,
      onTap: onTap,
      child: Container(
        height: 32,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: border),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(sentimentIcon(s), size: 14, color: fg),
            const SizedBox(width: 6),
            Text(
              sentimentLabel(s, context.t),
              style: context.text.caption.copyWith(color: fg, fontSize: 12, fontWeight: FontWeight.w600),
            ),
          ],
        ),
      ),
    );
  }
}

/* ------------------------------------------------------------------ featured + cards */

class _FeaturedCard extends StatelessWidget {
  const _FeaturedCard({required this.n, required this.onOpen});
  final NewsItem n;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KPressable(
      onTap: onOpen,
      pressedScale: 0.99,
      pressedOpacity: 0.9,
      child: Container(
        constraints: const BoxConstraints(minHeight: 320),
        decoration: BoxDecoration(
          borderRadius: BorderRadius.circular(k.cardRadius),
          border: Border.all(color: k.cardBorder),
          boxShadow: k.shadowCard,
        ),
        clipBehavior: Clip.antiAlias,
        child: Stack(
          children: [
            Positioned.fill(
              child: Image.asset(coverFor(n), fit: BoxFit.cover, alignment: coverPhotoFor(n).focus),
            ),
            Positioned.fill(
              child: DecoratedBox(
                decoration: BoxDecoration(
                  gradient: LinearGradient(
                    begin: Alignment.bottomCenter,
                    end: Alignment.topCenter,
                    colors: [k.bg, k.bg.withValues(alpha: 0.75), k.bg.withValues(alpha: 0.1)],
                    stops: const [0, 0.55, 1],
                  ),
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(20, 120, 20, 20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.end,
                children: [
                  Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    crossAxisAlignment: WrapCrossAlignment.center,
                    children: [
                      KChip(label: n.pinned ? t('news.pinned') : t('news.featured.topStory'), tone: KChipTone.ember, small: true),
                      SentimentChip(n.sentiment),
                      Text(
                        '${n.sourceName} · ${newsAgo(t, n.publishedAt)}',
                        style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w400, fontSize: 12),
                      ),
                    ],
                  ),
                  const SizedBox(height: 10),
                  Text(n.title, style: context.text.title1.copyWith(fontSize: 22, fontWeight: FontWeight.w500, height: 1.2)),
                  if (n.summary.isNotEmpty) ...[
                    const SizedBox(height: 8),
                    Text(
                      n.summary,
                      maxLines: 4,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.callout.copyWith(color: k.fg2, height: 1.5),
                    ),
                  ],
                  if (n.symbols.isNotEmpty) ...[
                    const SizedBox(height: 10),
                    Wrap(spacing: 6, runSpacing: 6, children: [for (final s in n.symbols) SymbolPill(s)]),
                  ],
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _StoryCard extends ConsumerWidget {
  const _StoryCard({required this.n, required this.saved, required this.onOpen});
  final NewsItem n;
  final bool saved;
  final VoidCallback onOpen;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    return KCard(
      padding: EdgeInsets.zero,
      onTap: onOpen,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          SizedBox(
            height: 144,
            child: Stack(
              children: [
                Positioned.fill(
                  child: Opacity(
                    opacity: 0.85,
                    child: Image.asset(coverFor(n), fit: BoxFit.cover, alignment: coverPhotoFor(n).focus),
                  ),
                ),
                Positioned.fill(
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        begin: Alignment.bottomCenter,
                        end: Alignment.topCenter,
                        colors: [k.surface, k.surface.withValues(alpha: 0)],
                        stops: const [0, 0.6],
                      ),
                    ),
                  ),
                ),
                PositionedDirectional(
                  start: 12,
                  top: 12,
                  end: 56,
                  child: Wrap(
                    spacing: 6,
                    runSpacing: 6,
                    children: [
                      SentimentChip(n.sentiment),
                      KChip(label: categoryLabel(n.category, t), small: true),
                      if (n.pinned) KChip(label: t('news.pinned'), tone: KChipTone.ember, icon: LucideIcons.pin, small: true),
                    ],
                  ),
                ),
                PositionedDirectional(
                  end: 6,
                  top: 6,
                  child: KPressable(
                    semanticLabel: saved ? t('news.saved.removeAria') : t('news.saved.addAria'),
                    onTap: () => _toggleSaved(ref, t, n.id),
                    child: Container(
                      width: 32,
                      height: 32,
                      decoration: BoxDecoration(
                        color: k.bg.withValues(alpha: 0.7),
                        shape: BoxShape.circle,
                        border: Border.all(color: k.line),
                      ),
                      child: Icon(saved ? Icons.bookmark_rounded : LucideIcons.bookmark, size: 15, color: saved ? k.ember : k.fg2),
                    ),
                  ),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 10, 18, 16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    for (final c in n.countries.take(3)) ...[KFlag(c, size: 14), const SizedBox(width: 5)],
                    Flexible(
                      child: Text(
                        '${n.sourceName} · ${newsAgo(t, n.publishedAt)}',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 6),
                Text(
                  n.title,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.headline.copyWith(fontWeight: FontWeight.w500),
                ),
                if (n.summary.isNotEmpty) ...[
                  const SizedBox(height: 5),
                  Text(
                    n.summary,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.footnote.copyWith(color: k.fg3, height: 1.45),
                  ),
                ],
                if (n.symbols.isNotEmpty) ...[
                  const SizedBox(height: 10),
                  Wrap(spacing: 6, runSpacing: 6, children: [for (final s in n.symbols) SymbolPill(s)]),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ brief */

const Map<String, (KChipTone, String)> _mood = {
  'risk-on': (KChipTone.up, 'news.mood.riskOn'),
  'risk-off': (KChipTone.down, 'news.mood.riskOff'),
  'mixed': (KChipTone.neutral, 'news.mood.mixed'),
  'cautious': (KChipTone.warn, 'news.mood.cautious'),
};

class _BriefCard extends ConsumerWidget {
  const _BriefCard();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final b = ref.watch(newsBriefProvider);
    final brief = b.value?.brief;
    final watch = (brief?.watch ?? const <String>[]).where(kInstrumentMap.containsKey).toList();
    final quotes = watch.isEmpty ? null : ref.watch(marketsFeedProvider);
    final created = b.value?.createdAt;
    final f = LocaleFormat(t.locale);
    final mood = brief == null ? null : _mood[brief.mood];
    return KCard(
      hot: true,
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(color: k.ember, shape: BoxShape.circle),
                child: Icon(LucideIcons.sparkles, size: 16, color: k.onEmber),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(t('news.brief.title'), style: context.text.headline.copyWith(fontWeight: FontWeight.w500)),
                    Text(
                      created != null ? t('news.brief.written', {'time': f.dateTime(created)}) : t('news.brief.writtenDaily'),
                      style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                    ),
                  ],
                ),
              ),
              if (brief != null) KChip(label: mood == null ? brief.mood : t(mood.$2), tone: mood?.$1 ?? KChipTone.neutral),
            ],
          ),
          const SizedBox(height: 14),
          if (!b.hasValue && !b.hasError)
            KSkeleton.lines(3, height: 13, lastWidthFactor: 0.66)
          else if (brief == null)
            Text(
              b.value?.configured == false ? t('news.brief.off') : t('news.brief.pending'),
              style: context.text.footnote.copyWith(color: k.fg3, height: 1.5, fontSize: 13),
            )
          else ...[
            Text(brief.headline, style: context.text.body.copyWith(fontWeight: FontWeight.w600, height: 1.35)),
            const SizedBox(height: 10),
            for (final p in brief.points)
              Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Container(
                      width: 6,
                      height: 6,
                      margin: const EdgeInsets.only(top: 7),
                      decoration: BoxDecoration(shape: BoxShape.circle, color: p.tone == 'up' ? k.up : (p.tone == 'down' ? k.down : k.warn)),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(p.text, style: context.text.footnote.copyWith(color: k.fg2, height: 1.5, fontSize: 13)),
                    ),
                  ],
                ),
              ),
            if (brief.calendarNote.isNotEmpty) ...[
              const SizedBox(height: 4),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                decoration: BoxDecoration(
                  color: k.surface.withValues(alpha: 0.6),
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: k.line),
                ),
                child: Text(brief.calendarNote, style: context.text.footnote.copyWith(color: k.fg2, height: 1.45)),
              ),
            ],
            if (watch.isNotEmpty) ...[
              const SizedBox(height: 12),
              for (var r = 0; r < watch.length; r += 2) ...[
                if (r > 0) const SizedBox(height: 8),
                Row(
                  children: [
                    for (var c = r; c < r + 2; c++) ...[
                      if (c > r) const SizedBox(width: 8),
                      Expanded(
                        child: c >= watch.length
                            ? const SizedBox.shrink()
                            : KPressable(
                                onTap: () => context.push('/trader?symbol=${watch[c]}'),
                                child: Container(
                                  padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
                                  decoration: BoxDecoration(
                                    color: k.surface.withValues(alpha: 0.6),
                                    borderRadius: BorderRadius.circular(12),
                                    border: Border.all(color: k.line),
                                  ),
                                  child: Row(
                                    children: [
                                      SymbolAvatar(watch[c], size: 18),
                                      const SizedBox(width: 6),
                                      Expanded(
                                        child: Text(
                                          watch[c],
                                          style: context.text.caption.copyWith(fontSize: 12, fontWeight: FontWeight.w600, color: k.fg),
                                        ),
                                      ),
                                      if (quotes?[watch[c]] != null)
                                        Text(
                                          Fmt.percent(quotes![watch[c]]!.change, signed: true),
                                          textDirection: TextDirection.ltr,
                                          style: context.text.caption.copyWith(fontSize: 11, color: quotes[watch[c]]!.change >= 0 ? k.up : k.down),
                                        ),
                                    ],
                                  ),
                                ),
                              ),
                      ),
                    ],
                  ],
                ),
              ],
            ],
            const SizedBox(height: 10),
            Text(
              t('news.brief.disclaimer'),
              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 10.5),
            ),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ story sheet */

Future<void> showStorySheet(BuildContext context, NewsItem n) {
  final router = GoRouter.of(context);
  final f = LocaleFormat(context.t.locale);
  return showKSheet<void>(
    context,
    title: n.sourceName,
    builder: (ctx) => Consumer(
      builder: (ctx, ref, _) {
        final t = ctx.t;
        final k = ctx.k;
        final saved = ref.watch(savedNewsProvider).contains(n.id);
        final tradable = n.symbols.where(kInstrumentMap.containsKey).firstOrNull;
        return KSheetContent(
          footer: Row(
            children: [
              KButton(
                label: saved ? t('news.story.saved') : t('common.save'),
                icon: saved ? Icons.bookmark_rounded : LucideIcons.bookmark,
                variant: KButtonVariant.ghost,
                size: KButtonSize.sm,
                onPressed: () => _toggleSaved(ref, t, n.id),
              ),
              if (n.link.isNotEmpty) ...[
                const SizedBox(width: 6),
                KButton(
                  label: t('news.story.copyLink'),
                  icon: LucideIcons.link2,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: () => kCopy(ctx, n.link, message: t('news.story.linkCopied')),
                ),
              ],
              const Spacer(),
              if (tradable != null)
                Flexible(
                  child: KButton(
                    label: t('news.story.trade', {'symbol': tradable}),
                    size: KButtonSize.sm,
                    onPressed: () {
                      Navigator.of(ctx).pop();
                      router.push('/trader?symbol=$tradable');
                    },
                  ),
                ),
            ],
          ),
          children: [
            Text(
              '${newsAgo(t, n.publishedAt)} · ${f.dateTime(n.publishedAt)}',
              textAlign: TextAlign.center,
              style: ctx.text.footnote.copyWith(color: k.fg3),
            ),
            const SizedBox(height: 14),
            Wrap(
              spacing: 8,
              runSpacing: 6,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                SentimentChip(n.sentiment),
                KChip(label: categoryLabel(n.category, t), small: true),
                for (final c in n.countries)
                  Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      KFlag(c, size: 14),
                      const SizedBox(width: 5),
                      Text(
                        countryName(c, t),
                        style: ctx.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                      ),
                    ],
                  ),
              ],
            ),
            const SizedBox(height: 12),
            Text(n.title, style: ctx.text.title1.copyWith(fontSize: 20, fontWeight: FontWeight.w500)),
            if (n.summary.isNotEmpty) ...[const SizedBox(height: 10), Text(n.summary, style: ctx.text.body.copyWith(color: k.fg2, height: 1.55))],
            if (n.symbols.isNotEmpty) ...[
              const SizedBox(height: 18),
              Text(t('news.story.instruments'), style: ctx.text.label.copyWith(color: k.fg2)),
              const SizedBox(height: 8),
              Wrap(spacing: 6, runSpacing: 6, children: [for (final s in n.symbols) SymbolPill(s)]),
            ],
            if (n.link.isNotEmpty) ...[
              const SizedBox(height: 18),
              KPressable(
                pressedScale: 0.99,
                onTap: () => launchUrl(Uri.parse(n.link), mode: LaunchMode.externalApplication),
                child: Container(
                  padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
                  decoration: BoxDecoration(
                    color: k.surface2,
                    borderRadius: BorderRadius.circular(k.rowRadius),
                    border: Border.all(color: k.line),
                  ),
                  child: Row(
                    children: [
                      Icon(LucideIcons.externalLink, size: 16, color: k.ember),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              t('news.story.readFull', {'source': n.sourceName}),
                              style: ctx.text.label.copyWith(fontWeight: FontWeight.w600, color: k.fg),
                            ),
                            Text(
                              n.link,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              textDirection: TextDirection.ltr,
                              style: ctx.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ],
            const SizedBox(height: 10),
            Text(
              t('news.story.attribution', {'source': n.sourceName}),
              style: ctx.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
          ],
        );
      },
    ),
  );
}

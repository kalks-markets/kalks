// The dashboard's "Markets" section (web live-dashboard.tsx + dashboard/movers.tsx + news-live/dashboard.tsx), in the
// phone order: Top movers, Market heatmap (both behind the live-price guard), Economic calendar, Market news and
// Markets & news around the world.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart' show DateFormat;
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/format/format.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../../markets/instruments.dart';
import '../../markets/markets_feed.dart';
import '../../news/widgets/world_dots.dart';
import '../dashboard_data.dart';

/// Opens Kalks Trader on a market (web `${TERMINAL_URL}/?symbol=`).
void openTraderSymbol(BuildContext context, String symbol) => context.push('/trader?symbol=$symbol');

/// Live builds show prices only from the real feed: until the first prices arrive, a neutral state instead of
/// numbers (web FeedGuard).
class FeedGuard extends ConsumerWidget {
  const FeedGuard({super.key, required this.title, required this.child, this.minHeight = 220});
  final String title;
  final Widget child;
  final double minHeight;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final live = ref.watch(marketsFeedProvider.select((s) => s.live));
    if (live) return child;
    final t = context.t;
    final k = context.k;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: title),
          ConstrainedBox(
            constraints: BoxConstraints(minHeight: minHeight),
            child: Center(
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 24),
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      t('app.dashboard.feed.connecting'),
                      textAlign: TextAlign.center,
                      style: context.text.callout.copyWith(color: k.fg2),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      t('app.dashboard.feed.connectingText'),
                      textAlign: TextAlign.center,
                      style: context.text.footnote.copyWith(color: k.fg3),
                    ),
                  ],
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// A signed percent in up / down colour (web Delta).
class Delta extends StatelessWidget {
  const Delta(this.value, {super.key, this.style});
  final double value;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Text(
      '${value >= 0 ? '+' : ''}${value.toStringAsFixed(2)}%',
      textDirection: TextDirection.ltr,
      style: (style ?? context.text.caption).copyWith(color: value >= 0 ? k.up : k.down, fontFeatures: kTabular, fontWeight: FontWeight.w600),
    );
  }
}

/* ------------------------------------------------------------------ */
/* Top movers                                                          */
/* ------------------------------------------------------------------ */

class MoversCard extends ConsumerStatefulWidget {
  const MoversCard({super.key});

  @override
  ConsumerState<MoversCard> createState() => _MoversCardState();
}

class _MoversCardState extends ConsumerState<MoversCard> {
  bool _gainers = true;
  String _range = '1D';

  /// % change over 1W / 1M for every market, once all their closes are in (web useRangeChanges).
  Map<String, double>? _rangeChanges(MarketsFeedState feed) {
    if (_range == '1D') return null;
    final (tf, n) = kMoverBars[_range]!;
    final out = <String, double>{};
    for (final i in kInstruments) {
      final c = ref.watch(moverClosesProvider((i.symbol, tf, n)));
      if (c.isLoading) return null;
      final closes = c.value;
      if (closes != null && closes.length > 1 && closes.first != 0) out[i.symbol] = (closes.last - closes.first) / closes.first * 100;
    }
    return out;
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final feed = ref.watch(marketsFeedProvider);
    final today = {for (final e in feed.quotes.entries) e.key: e.value.change};
    final list = topMovers(today, gainers: _gainers, rangeChanges: _rangeChanges(feed));
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('dashboard.movers.title'),
            action: SizedBox(
              width: 132,
              child: KSegmented<String>(
                values: const ['1D', '1W', '1M'],
                labels: const ['1D', '1W', '1M'],
                selected: _range,
                onChanged: (v) => setState(() => _range = v),
                height: 30,
                plain: true,
              ),
            ),
          ),
          const SizedBox(height: 12),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: SizedBox(
              width: 200,
              child: KSegmented<bool>(
                values: const [true, false],
                labels: [t('dashboard.movers.gainers'), t('dashboard.movers.losers')],
                selected: _gainers,
                onChanged: (v) => setState(() => _gainers = v),
                height: 30,
                plain: true,
              ),
            ),
          ),
          const SizedBox(height: 8),
          for (final i in list) _MoverRow(instrument: i, range: _range, quote: feed[i.symbol]),
        ],
      ),
    );
  }
}

class _MoverRow extends ConsumerWidget {
  const _MoverRow({required this.instrument, required this.range, required this.quote});
  final Instrument instrument;
  final String range;
  final MarketQuote? quote;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final q = quote;
    double? ch;
    if (range == '1D') {
      ch = q?.change;
    } else {
      final (tf, n) = kMoverBars[range]!;
      final closes = ref.watch(moverClosesProvider((instrument.symbol, tf, n))).value;
      final px = q?.mid;
      if (closes != null && closes.isNotEmpty && closes.first != 0 && px != null) ch = (px - closes.first) / closes.first * 100;
    }
    return KPressable(
      onTap: () => openTraderSymbol(context, instrument.symbol),
      pressedScale: 1,
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 9),
        child: Row(
          children: [
            SymbolAvatar(instrument.symbol, size: 26),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(instrument.symbol, style: context.text.label.copyWith(fontSize: 13.5, color: k.fg)),
                  Text(
                    instrument.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                  ),
                ],
              ),
            ),
            SizedBox(
              width: 96,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Text(
                    q == null ? '—' : Fmt.number(q.bid, instrument.digits),
                    textDirection: TextDirection.ltr,
                    style: context.text.mono(13, weight: FontWeight.w600, color: q == null || q.dir == 0 ? k.fg : (q.dir > 0 ? k.up : k.down)),
                  ),
                  const SizedBox(height: 2),
                  if (ch == null) Text('—', style: context.text.caption.copyWith(color: k.fg3)) else Delta(ch),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/* ------------------------------------------------------------------ */
/* Heatmap                                                             */
/* ------------------------------------------------------------------ */

class HeatmapCard extends ConsumerWidget {
  const HeatmapCard({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final feed = ref.watch(marketsFeedProvider);
    double ch(Instrument i) => feed[i.symbol]?.change ?? 0;
    final sorted = [...kInstruments]..sort((a, b) => ch(b).compareTo(ch(a)));
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('dashboard.heatmap.title'),
            subtitle: t('dashboard.heatmap.subtitle'),
            action: KButton(
              label: t('dashboard.heatmap.allMarkets'),
              size: KButtonSize.sm,
              variant: KButtonVariant.surface,
              onPressed: () => context.go('/markets'),
            ),
          ),
          const SizedBox(height: 14),
          LayoutBuilder(
            builder: (context, c) {
              const gap = 8.0;
              final w = (c.maxWidth - 2 * gap) / 3;
              return Wrap(
                spacing: gap,
                runSpacing: gap,
                children: [
                  for (final i in sorted)
                    Builder(
                      builder: (context) {
                        final v = ch(i);
                        final a = math.min(1.0, v.abs() / 3);
                        final open = isMarketOpen(i);
                        final bg = mixOklab(v >= 0 ? k.up : k.down, k.surface2, (8 + a * 48) / 100);
                        return KPressable(
                          onTap: () => context.go('/markets'),
                          semanticLabel: open ? t('dashboard.heatmap.tipOpen', {'symbol': i.symbol}) : t('dashboard.heatmap.tipClosed', {'symbol': i.symbol}),
                          child: Container(
                            width: w,
                            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                            decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(14)),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Row(
                                  children: [
                                    Flexible(
                                      child: Text(
                                        i.symbol,
                                        maxLines: 1,
                                        overflow: TextOverflow.ellipsis,
                                        style: context.text.caption.copyWith(fontSize: 12.5, fontWeight: FontWeight.w700, color: k.fg),
                                      ),
                                    ),
                                    const SizedBox(width: 6),
                                    Container(
                                      width: 6,
                                      height: 6,
                                      decoration: BoxDecoration(
                                        shape: BoxShape.circle,
                                        color: open ? k.up : null,
                                        border: open ? null : Border.all(color: k.fg3),
                                      ),
                                    ),
                                  ],
                                ),
                                const SizedBox(height: 2),
                                Text(
                                  '${v >= 0 ? '+' : ''}${v.toStringAsFixed(2)}%',
                                  textDirection: TextDirection.ltr,
                                  style: context.text.caption.copyWith(
                                    fontSize: 12,
                                    fontWeight: FontWeight.w600,
                                    fontFeatures: kTabular,
                                    color: a > 0.55 ? k.fg : (v >= 0 ? k.up : k.down),
                                  ),
                                ),
                              ],
                            ),
                          ),
                        );
                      },
                    ),
                ],
              );
            },
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ */
/* Economic calendar                                                   */
/* ------------------------------------------------------------------ */

String _gmt(int offset) => 'GMT${offset >= 0 ? '+' : ''}$offset';

class CalendarCard extends ConsumerWidget {
  const CalendarCard({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final cal = ref.watch(dashCalendarProvider);
    final offset = cal.value?.serverOffset ?? 3;
    final now = DateTime.now();
    final list = cal.hasValue ? upcomingEvents(cal.value!.events, now) : const <DashEvent>[];
    final today = now.toUtc().add(Duration(hours: offset)).toIso8601String().substring(0, 10);
    final tag = intlLocale(t.locale);
    String weekday(String day) {
      final d = DateTime.tryParse('${day}T12:00:00Z');
      return d == null ? '' : latinDigits(DateFormat.E(tag).format(d)).toUpperCase();
    }

    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('news.cal.title'),
            subtitle: t('news.dash.calendar.subtitle', {'tz': _gmt(offset)}),
            action: KButton(label: t('common.viewAll'), size: KButtonSize.sm, variant: KButtonVariant.surface, onPressed: () => context.go('/calendar')),
          ),
          const SizedBox(height: 14),
          if (cal.isLoading && !cal.hasValue)
            for (var i = 0; i < 4; i++) const Padding(padding: EdgeInsets.only(bottom: 8), child: KSkeleton(height: 62, radius: 16)),
          if (!cal.isLoading && list.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 36),
              child: Text(
                cal.hasError ? localizeErrorText(cal.error!, t) : t('news.dash.calendar.empty'),
                textAlign: TextAlign.center,
                style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13),
              ),
            ),
          for (final e in list)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: KPressable(
                onTap: () => context.go('/calendar'),
                pressedScale: 0.99,
                child: Container(
                  decoration: BoxDecoration(
                    color: k.surface2,
                    borderRadius: BorderRadius.circular(k.rowRadius),
                    border: Border.all(color: k.line),
                  ),
                  clipBehavior: Clip.antiAlias,
                  child: IntrinsicHeight(
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Container(
                          width: 3,
                          margin: const EdgeInsets.symmetric(vertical: 8),
                          decoration: BoxDecoration(
                            color: e.impact == 3 ? k.down : k.warn,
                            borderRadius: const BorderRadiusDirectional.horizontal(end: Radius.circular(3)).resolve(Directionality.of(context)),
                          ),
                        ),
                        const SizedBox(width: 14),
                        SizedBox(
                          width: 46,
                          child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              if (e.serverDate != today) Text(weekday(e.serverDate), style: context.text.mono(10, color: k.fg3)),
                              Text(
                                e.serverTime,
                                textDirection: TextDirection.ltr,
                                style: context.text.mono(12, color: k.fg3),
                              ),
                            ],
                          ),
                        ),
                        Expanded(
                          child: Padding(
                            padding: const EdgeInsets.symmetric(vertical: 12),
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text(
                                  e.title,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: context.text.label.copyWith(fontSize: 13.5, color: k.fg),
                                ),
                                const SizedBox(height: 2),
                                Text.rich(
                                  TextSpan(
                                    children: [
                                      if (e.actual.isNotEmpty)
                                        TextSpan(
                                          text: '${t('news.abbr.actual')} ${e.actual} · ',
                                          style: TextStyle(color: e.surprise == 1 ? k.up : (e.surprise == -1 ? k.down : k.fg2)),
                                        ),
                                      TextSpan(
                                        text:
                                            '${t('news.abbr.forecast')} ${e.forecast.isEmpty ? '—' : e.forecast} · ${t('news.abbr.previous')} ${e.previous.isEmpty ? '—' : e.previous}',
                                      ),
                                    ],
                                  ),
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                                ),
                              ],
                            ),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Center(
                          child: _FlagChip(country: e.country, label: e.currency, tone: e.impact == 3 ? KChipTone.down : KChipTone.neutral),
                        ),
                        const SizedBox(width: 12),
                      ],
                    ),
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// Plain text of a failed request (FutureProvider errors are ApiException or anything else).
String localizeErrorText(Object e, T t) => e is ApiException ? localizeError(e, t) : t('common.errorRetry');

class _FlagChip extends StatelessWidget {
  const _FlagChip({required this.country, required this.label, required this.tone});
  final String country, label;
  final KChipTone tone;

  @override
  Widget build(BuildContext context) {
    final (bg, fg, border) = context.k.chip(tone);
    return Container(
      height: 20,
      padding: const EdgeInsets.symmetric(horizontal: 7),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: border),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (country.isNotEmpty) ...[KFlag(country, size: 12), const SizedBox(width: 4)],
          Text(label, style: context.text.caption.copyWith(color: fg, fontSize: 10.5)),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ */
/* Market news + the story sheet                                       */
/* ------------------------------------------------------------------ */

String newsAgoText(T t, DateTime at, [DateTime? now]) {
  final m = math.max(0, ((now ?? DateTime.now()).difference(at).inSeconds / 60).round());
  if (m < 1) return t('news.ago.justNow');
  if (m < 60) return t('news.ago.minutes', {'m': m});
  final h = m ~/ 60;
  if (h < 24) return m % 60 != 0 ? t('news.ago.hoursMinutes', {'h': h, 'm': m % 60}) : t('news.ago.hours', {'h': h});
  final d = h ~/ 24;
  return d == 1 ? t('news.ago.yesterday') : t('news.ago.days', {'count': d});
}

/// A cover per category from the founder's own solid-colour photos (web news-live/shared.tsx COVER, 2026-10-10).
const Map<String, KHeroPhoto> _covers = {
  'macro': KHeroPhoto.news,
  'forex': KHeroPhoto.markets,
  'metals': KHeroPhoto.rewards,
  'indices': KHeroPhoto.portfolio,
  'energies': KHeroPhoto.profile,
  'crypto': KHeroPhoto.copy,
  'stocks': KHeroPhoto.accounts,
  'markets': KHeroPhoto.wallet,
};

/// The photo of a story (by category; publisher images are never copied).
KHeroPhoto newsCoverPhoto(DashNews n) => _covers[n.category] ?? KHeroPhoto.wallet;
String newsCover(DashNews n) => newsCoverPhoto(n).asset;

class _Cover extends StatelessWidget {
  const _Cover(this.n);
  final DashNews n;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return ClipRRect(
      borderRadius: BorderRadius.circular(12),
      child: Image.asset(
        newsCover(n),
        width: 56,
        height: 56,
        fit: BoxFit.cover,
        alignment: newsCoverPhoto(n).focus,
        opacity: const AlwaysStoppedAnimation(0.9),
        errorBuilder: (_, _, _) => Container(
          width: 56,
          height: 56,
          decoration: BoxDecoration(gradient: LinearGradient(colors: [k.emberSoft, k.surface3])),
          child: Icon(LucideIcons.newspaper, size: 20, color: k.fg3),
        ),
      ),
    );
  }
}

class NewsCard extends ConsumerWidget {
  const NewsCard({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final feed = ref.watch(dashNewsProvider);
    final items = feed.value ?? const <DashNews>[];
    final now = DateTime.now();
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('news.dash.news.title'),
            action: KButton(label: t('news.dash.news.allNews'), size: KButtonSize.sm, variant: KButtonVariant.surface, onPressed: () => context.go('/news')),
          ),
          const SizedBox(height: 14),
          if (feed.isLoading && !feed.hasValue)
            for (var i = 0; i < 4; i++) const Padding(padding: EdgeInsets.only(bottom: 8), child: KSkeleton(height: 84, radius: 16)),
          if (!feed.isLoading && items.isEmpty)
            Padding(
              padding: const EdgeInsets.symmetric(vertical: 36),
              child: Text(
                feed.hasError ? localizeErrorText(feed.error!, t) : t('news.dash.news.empty'),
                textAlign: TextAlign.center,
                style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13),
              ),
            ),
          for (final n in items)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: KPressable(
                onTap: () => showNewsStory(context, n),
                pressedScale: 0.99,
                child: Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: k.surface2,
                    borderRadius: BorderRadius.circular(k.rowRadius),
                    border: Border.all(color: k.line),
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      _Cover(n),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Row(
                              children: [
                                if (n.pinned) ...[KChip(label: t('news.pinned'), tone: KChipTone.ember, small: true), const SizedBox(width: 6)],
                                Flexible(
                                  child: Text(
                                    '${n.source} · ${newsAgoText(t, n.publishedAt, now)}',
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                                  ),
                                ),
                              ],
                            ),
                            const SizedBox(height: 4),
                            Text(
                              n.title,
                              maxLines: 2,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.label.copyWith(fontSize: 13.5, color: k.fg, height: 1.3),
                            ),
                            if (n.symbols.isNotEmpty) ...[
                              const SizedBox(height: 6),
                              Wrap(
                                spacing: 6,
                                children: [
                                  for (final s in n.symbols.take(3))
                                    Container(
                                      padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
                                      decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(6)),
                                      child: Text(s, style: context.text.mono(10.5, color: k.fg2)),
                                    ),
                                ],
                              ),
                            ],
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
            ),
        ],
      ),
    );
  }
}

/// The story sheet (web StoryDialog): source and time, tone / category / countries, title, summary, instruments,
/// the full story link and attribution; Copy link and Trade `symbol`.
Future<void> showNewsStory(BuildContext context, DashNews n) {
  final t = context.t;
  final f = LocaleFormat(t.locale);
  return showKSheet<void>(
    context,
    title: n.source,
    builder: (context) {
      final k = context.k;
      final tradable = n.symbols.where(kInstrumentMap.containsKey).firstOrNull;
      final tone = switch (n.sentiment) {
        'bullish' => (KChipTone.up, LucideIcons.trendingUp),
        'bearish' => (KChipTone.down, LucideIcons.trendingDown),
        _ => (KChipTone.neutral, LucideIcons.minus),
      };
      return KSheetContent(
        footer: Row(
          children: [
            if (n.link.isNotEmpty)
              Expanded(
                child: KButton(
                  label: t('news.story.copyLink'),
                  icon: LucideIcons.link2,
                  variant: KButtonVariant.surface,
                  expand: true,
                  onPressed: () => kCopy(context, n.link, message: t('news.story.linkCopied')),
                ),
              ),
            if (tradable != null) ...[
              const SizedBox(width: 8),
              Expanded(
                child: KButton(
                  label: t('news.story.trade', {'symbol': tradable}),
                  expand: true,
                  onPressed: () {
                    Navigator.of(context).pop();
                    openTraderSymbol(context, tradable);
                  },
                ),
              ),
            ],
          ],
        ),
        children: [
          Text(
            '${newsAgoText(t, n.publishedAt)} · ${f.dateTime(n.publishedAt)}',
            textAlign: TextAlign.center,
            style: context.text.footnote.copyWith(color: k.fg3),
          ),
          const SizedBox(height: 14),
          Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              KChip(
                label: t.dyn('news.sentiment.${n.sentiment}', fallback: n.sentiment),
                tone: tone.$1,
                icon: tone.$2,
                small: true,
              ),
              KChip(label: t.dyn('news.category.${n.category}', fallback: n.category), small: true),
              for (final c in n.countries)
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    KFlag(c, size: 14),
                    const SizedBox(width: 5),
                    Text(
                      t.dyn('news.country.$c', fallback: c.toUpperCase()),
                      style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                    ),
                  ],
                ),
            ],
          ),
          const SizedBox(height: 12),
          Text(n.title, style: context.text.title2.copyWith(fontSize: 19, fontWeight: FontWeight.w500, height: 1.25)),
          if (n.summary.isNotEmpty) ...[const SizedBox(height: 10), Text(n.summary, style: context.text.body.copyWith(color: k.fg2, height: 1.55))],
          if (n.symbols.isNotEmpty) ...[
            const SizedBox(height: 18),
            Text(t('news.story.instruments'), style: context.text.label.copyWith(color: k.fg2)),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final s in n.symbols)
                  KPressable(
                    onTap: kInstrumentMap.containsKey(s)
                        ? () {
                            Navigator.of(context).pop();
                            openTraderSymbol(context, s);
                          }
                        : null,
                    minSize: 32,
                    child: Container(
                      padding: const EdgeInsetsDirectional.fromSTEB(3, 3, 9, 3),
                      decoration: BoxDecoration(
                        color: k.surface3,
                        borderRadius: BorderRadius.circular(20),
                        border: Border.all(color: k.line),
                      ),
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          if (kInstrumentMap.containsKey(s)) ...[SymbolAvatar(s, size: 16), const SizedBox(width: 5)],
                          Text(s, style: context.text.mono(11, color: k.fg2)),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
          ],
          if (n.link.isNotEmpty) ...[
            const SizedBox(height: 18),
            KPressable(
              onTap: () => launchUrl(Uri.parse(n.link), mode: LaunchMode.externalApplication),
              pressedScale: 0.99,
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
                          Text(t('news.story.readFull', {'source': n.source}), style: context.text.label.copyWith(color: k.fg)),
                          Text(
                            n.link,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            textDirection: TextDirection.ltr,
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
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
            t('news.story.attribution', {'source': n.source}),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 11),
          ),
        ],
      );
    },
  );
}

/* ------------------------------------------------------------------ */
/* Markets & news around the world                                     */
/* ------------------------------------------------------------------ */

/// Country centroids (lon, lat) for the pins (web COUNTRY_CENTROIDS).
const Map<String, (double, double)> kCountryCentroids = {
  'us': (-98, 39), 'eu': (7.75, 48.6), 'gb': (-2, 54), 'de': (10, 51), 'jp': (138, 36), 'in': (79, 22), 'sa': (45, 24), 'ae': (54, 24), //
  'sg': (103.8, 1.35), 'br': (-51, -10), 'cn': (104, 35), 'au': (134, -25), 'ca': (-106, 56), 'ch': (8, 47), 'za': (24, -29), //
  'ru': (90, 60), 'tr': (35, 39), 'ng': (8, 9), 'mx': (-102, 23), 'eg': (30, 26), 'vn': (106, 16), 'my': (102, 4), 'id': (118, -2), //
  'ph': (122, 12), 'pk': (70, 30), 'ir': (53, 32), 'iq': (44, 33), 'sy': (38, 35), 'kp': (127, 40), 'kr': (128, 36), 'cu': (-79, 21.5), //
  'mm': (96, 21), 'af': (66, 34), 'be': (4.5, 50.8), 'nl': (5.3, 52.1), 'fr': (2.3, 46.5), 'es': (-3.7, 40.2), 'it': (12.5, 42.8), //
  'pt': (-8, 39.6), 'pl': (19, 52), 'ua': (31, 49), 'by': (28, 53.5), 'se': (15, 62), 'no': (9, 61), 'fi': (26, 64), 'dk': (9.5, 56), //
  'ie': (-8, 53.2), 'at': (14.5, 47.5), 'gr': (22, 39), 'cy': (33, 35), 'il': (35, 31.4), 'jo': (36, 31), 'lb': (35.8, 33.9), //
  'kw': (47.6, 29.3), 'qa': (51.2, 25.3), 'bh': (50.6, 26), 'om': (57, 21), 'ye': (48, 15.5), 'th': (101, 15), 'kh': (105, 12.5), //
  'la': (103, 18), 'bd': (90, 24), 'lk': (80.7, 7.8), 'np': (84, 28), 'kz': (67, 48), 'uz': (64, 41), 'hk': (114.2, 22.3), //
  'tw': (121, 23.7), 'nz': (174, -41), 'ar': (-64, -34), 'cl': (-71, -33), 'co': (-73, 4), 'pe': (-75, -9), 've': (-66, 7), //
  'ke': (38, 0.2), 'gh': (-1, 7.9), 'et': (39, 8.6), 'tz': (35, -6), 'ma': (-6, 32), 'dz': (3, 28), 'tn': (9, 34), 'ci': (-5.5, 7.5), //
  'sn': (-14.5, 14.5), 'cm': (12, 5.7), 'ug': (32.3, 1.4), 'zw': (29.8, -19), 'sd': (30, 15), 'ly': (17, 27), 'so': (46, 6), //
  'ss': (30, 7.5), 'cf': (21, 6.6), 'cd': (23, -3), 'ro': (25, 46), 'hu': (19.5, 47.2), 'cz': (15.5, 49.8),
};

/// d3 geoEqualEarth with the map's scale / translate: (lon, lat) in degrees -> map pixels (960 x 470).
Offset equalEarth(double lon, double lat) {
  const a1 = 1.340264, a2 = -0.081106, a3 = 0.000893, a4 = 0.003796;
  final m = math.sqrt(3) / 2;
  final lambda = lon * math.pi / 180, phi = lat * math.pi / 180;
  final l = math.asin(m * math.sin(phi));
  final l2 = l * l, l6 = l2 * l2 * l2;
  final x = lambda * math.cos(l) / (m * (a1 + 3 * a2 * l2 + l6 * (7 * a3 + 9 * a4 * l2)));
  final y = l * (a1 + a2 * l2 + l6 * (a3 + a4 * l2));
  return Offset(kWorldTx + kWorldScale * x, kWorldTy - kWorldScale * y);
}

/// The land dots of the halftone map (web dotsPath), in map pixels.
final List<Offset> kWorldDots = () {
  final out = <Offset>[];
  for (var j = 0; j < kWorldRows.length; j++) {
    final y = kWorldStep / 2 + j * kWorldStep;
    var i = 0;
    var land = false;
    for (final run in kWorldRows[j].split('.')) {
      final n = int.parse(run, radix: 36);
      if (land) {
        for (var k = 0; k < n; k++) {
          out.add(Offset(kWorldStep / 2 + (i + k) * kWorldStep, y));
        }
      }
      i += n;
      land = !land;
    }
  }
  return out;
}();

/// One pin of the map (web MapPin).
typedef MapPin = ({String country, int count, String label, String tone});

/// The dotted world map with glowing story pins and up / down heat (web WorldMap). Always LTR.
class WorldMap extends StatefulWidget {
  const WorldMap({super.key, required this.pins, this.heat = const {}, this.onTap});
  final List<MapPin> pins;
  final Map<String, double> heat;
  final VoidCallback? onTap;

  @override
  State<WorldMap> createState() => _WorldMapState();
}

class _WorldMapState extends State<WorldMap> with SingleTickerProviderStateMixin {
  late final AnimationController _pulse = AnimationController(vsync: this, duration: const Duration(seconds: 3))..repeat();

  @override
  void dispose() {
    _pulse.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return GestureDetector(
      onTap: widget.onTap,
      child: AspectRatio(
        aspectRatio: kWorldW / kWorldH,
        child: AnimatedBuilder(
          animation: _pulse,
          builder: (context, _) => CustomPaint(
            painter: _WorldPainter(
              pins: widget.pins,
              heat: widget.heat,
              pulse: _pulse.value,
              dot: k.fg3.withValues(alpha: 0.55),
              surface: k.surface,
              fg: k.fg,
              ember: kEmber,
              up: k.up,
              down: const Color(0xFFF04438),
              countStyle: context.text.mono(10, weight: FontWeight.w700, color: k.fg),
            ),
          ),
        ),
      ),
    );
  }
}

class _WorldPainter extends CustomPainter {
  _WorldPainter({
    required this.pins,
    required this.heat,
    required this.pulse,
    required this.dot,
    required this.surface,
    required this.fg,
    required this.ember,
    required this.up,
    required this.down,
    required this.countStyle,
  });
  final List<MapPin> pins;
  final Map<String, double> heat;
  final double pulse;
  final Color dot, surface, fg, ember, up, down;
  final TextStyle countStyle;

  @override
  void paint(Canvas canvas, Size size) {
    final s = size.width / kWorldW;
    canvas.save();
    canvas.scale(s);
    for (final e in heat.entries) {
      final c = kCountryCentroids[e.key];
      if (c == null) continue;
      final p = equalEarth(c.$1, c.$2);
      final r = 40 + e.value.abs() * 18;
      final col = e.value >= 0 ? up : down;
      canvas.drawCircle(
        p,
        r,
        Paint()..shader = RadialGradient(colors: [col.withValues(alpha: 0.35), col.withValues(alpha: 0)]).createShader(Rect.fromCircle(center: p, radius: r)),
      );
    }
    final dp = Paint()..color = dot;
    for (final d in kWorldDots) {
      canvas.drawCircle(d, 1.55, dp);
    }
    final pr = 18 + 12 * (0.5 - (pulse - 0.5).abs()) * 2;
    for (final pin in pins) {
      final c = kCountryCentroids[pin.country];
      if (c == null) continue;
      final p = equalEarth(c.$1, c.$2);
      canvas.drawCircle(
        p,
        pr,
        Paint()
          ..shader = RadialGradient(colors: [ember.withValues(alpha: 0.55), ember.withValues(alpha: 0)]).createShader(Rect.fromCircle(center: p, radius: pr)),
      );
      canvas.drawCircle(p, 11, Paint()..color = surface);
      canvas.drawCircle(
        p,
        11,
        Paint()
          ..color = ember
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.5,
      );
      final tp = TextPainter(
        text: TextSpan(text: '${pin.count}', style: countStyle),
        textDirection: TextDirection.ltr,
      )..layout();
      tp.paint(canvas, p - Offset(tp.width / 2, tp.height / 2));
    }
    canvas.restore();
  }

  @override
  bool shouldRepaint(_WorldPainter old) => old.pulse != pulse || old.pins != pins || old.heat != heat || old.dot != dot;
}

class WorldCard extends ConsumerWidget {
  const WorldCard({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final map = ref.watch(dashMapProvider).value;
    final pins = <MapPin>[
      for (final c in map?.countries ?? const <DashMapCountry>[])
        (
          country: c.country,
          count: c.count,
          label: t.dyn('news.country.${c.country}', fallback: c.name.isNotEmpty ? c.name : c.country.toUpperCase()),
          tone: c.sentiment > 0.2 ? 'up' : (c.sentiment < -0.2 ? 'down' : 'ember'),
        ),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('news.dash.world.title'),
            subtitle: t('news.dash.world.subtitle'),
            action: KChip(label: map != null ? t('news.dash.world.storiesToday', {'count': map.total}) : t('common.loading'), tone: KChipTone.ember),
          ),
          const SizedBox(height: 8),
          Semantics(
            label: t('news.dash.world.open'),
            button: true,
            child: WorldMap(pins: pins, heat: heatOf(map?.countries ?? const []), onTap: () => context.go('/news')),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final p in pins.take(8))
                KPressable(
                  onTap: () => context.go('/news'),
                  minSize: 30,
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                    decoration: BoxDecoration(
                      color: k.surface2,
                      borderRadius: BorderRadius.circular(14),
                      border: Border.all(color: k.line),
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        KFlag(p.country, size: 12),
                        const SizedBox(width: 5),
                        Text(
                          p.label,
                          style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w400, fontSize: 11),
                        ),
                        const SizedBox(width: 5),
                        Text(
                          '${p.count}',
                          style: context.text.caption.copyWith(color: k.fg3, fontSize: 11, fontFeatures: kTabular),
                        ),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

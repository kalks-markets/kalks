// Small pieces of the news and calendar pages (web components/news-live/shared.tsx): sentiment chip, symbol pill,
// "5m ago", country and category names, the story cover photos, impact bars, the actual value, the countdown.
import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../../markets/instruments.dart';
import '../news_api.dart';

const Map<String, String> kCountryName = {
  'us': 'United States',
  'eu': 'Euro area',
  'de': 'Germany',
  'fr': 'France',
  'it': 'Italy',
  'es': 'Spain',
  'gb': 'United Kingdom',
  'jp': 'Japan',
  'au': 'Australia',
  'ca': 'Canada',
  'ch': 'Switzerland',
  'nz': 'New Zealand',
  'cn': 'China',
  'in': 'India',
  'sa': 'Saudi Arabia',
  'ru': 'Russia',
  'ir': 'Iran',
  'br': 'Brazil',
  'mx': 'Mexico',
  'kr': 'South Korea',
  'sg': 'Singapore',
};

const Map<String, String> _categoryLabel = {
  'macro': 'Central banks & macro',
  'forex': 'Forex',
  'metals': 'Metals',
  'indices': 'Indices',
  'energies': 'Energy',
  'crypto': 'Crypto',
  'stocks': 'Stocks',
  'markets': 'Markets',
};

String countryName(String c, T t) => t.dyn('news.country.$c', fallback: kCountryName[c] ?? c.toUpperCase());
String categoryLabel(String c, T t) => t.dyn('news.category.$c', fallback: _categoryLabel[c] ?? c);

/// "just now" / "5m ago" / "2h 10m ago" / "yesterday" / "3 days ago" (web `ago`).
String newsAgo(T t, DateTime at, [DateTime? now]) {
  final m = ((now ?? DateTime.now()).difference(at).inSeconds / 60).round().clamp(0, 1 << 30);
  if (m < 1) return t('news.ago.justNow');
  if (m < 60) return t('news.ago.minutes', {'m': m});
  final h = m ~/ 60;
  if (h < 24) return m % 60 != 0 ? t('news.ago.hoursMinutes', {'h': h, 'm': m % 60}) : t('news.ago.hours', {'h': h});
  final d = h ~/ 24;
  return d == 1 ? t('news.ago.yesterday') : t('news.ago.days', {'count': d});
}

/// "2d 04:12:33" / "04:12:33" (web countdown).
String countdownText(T t, Duration left) {
  final s = left.isNegative ? 0 : left.inSeconds;
  final d = s ~/ 86400;
  String two(int n) => n.toString().padLeft(2, '0');
  final time = '${two((s % 86400) ~/ 3600)}:${two((s % 3600) ~/ 60)}:${two(s % 60)}';
  return d > 0 ? t('news.countdown.days', {'d': d, 'time': time}) : time;
}

/// A cover per category from the founder's own solid-colour photos (web news-live/shared.tsx COVER, 2026-10-10: the
/// old stock library is retired; publisher images are never copied).
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

KHeroPhoto coverPhotoFor(NewsItem n) => _covers[n.category] ?? KHeroPhoto.wallet;

String coverFor(NewsItem n) => coverPhotoFor(n).asset;

KChipTone sentimentTone(Sentiment s) => switch (s) {
  Sentiment.bullish => KChipTone.up,
  Sentiment.bearish => KChipTone.down,
  Sentiment.neutral => KChipTone.neutral,
};

IconData sentimentIcon(Sentiment s) => switch (s) {
  Sentiment.bullish => LucideIcons.trendingUp,
  Sentiment.bearish => LucideIcons.trendingDown,
  Sentiment.neutral => LucideIcons.minus,
};

String sentimentLabel(Sentiment s, T t) => t('news.sentiment.${s.name}');

class SentimentChip extends StatelessWidget {
  const SentimentChip(this.s, {super.key});
  final Sentiment s;

  @override
  Widget build(BuildContext context) => KChip(label: sentimentLabel(s, context.t), tone: sentimentTone(s), icon: sentimentIcon(s), small: true);
}

/// A symbol tag; known instruments open Kalks Trader on that symbol.
class SymbolPill extends StatelessWidget {
  const SymbolPill(this.symbol, {super.key});
  final String symbol;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final known = kInstrumentMap.containsKey(symbol);
    final pill = Container(
      height: 22,
      padding: EdgeInsetsDirectional.only(start: known ? 3 : 8, end: 8),
      decoration: BoxDecoration(
        color: k.surface3,
        borderRadius: BorderRadius.circular(11),
        border: Border.all(color: k.line),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (known) ...[SymbolAvatar(symbol, size: 15), const SizedBox(width: 4)],
          Text(symbol, style: context.text.mono(10.5, color: k.fg2)),
        ],
      ),
    );
    if (!known) return pill;
    return KPressable(minSize: 32, onTap: () => context.push('/trader?symbol=$symbol'), child: pill);
  }
}

/// Impact as 1–3 rising bars (red high, amber medium, grey low).
class ImpactBars extends StatelessWidget {
  const ImpactBars(this.impact, {super.key});
  final int impact;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final color = impact == 3 ? k.down : (impact == 2 ? k.warn : k.fg2);
    return Semantics(
      label: impact > 0 ? context.t('news.impact.aria', {'n': impact}) : context.t('news.impact.holiday'),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          for (var i = 1; i <= 3; i++) ...[
            if (i > 1) const SizedBox(width: 3),
            Container(
              width: 4,
              height: 6.0 + i * 3,
              decoration: BoxDecoration(color: i <= impact ? color : k.surface3, borderRadius: BorderRadius.circular(2)),
            ),
          ],
        ],
      ),
    );
  }
}

/// The release's actual figure, green / red when it beat / missed the forecast.
class ActualValue extends StatelessWidget {
  const ActualValue(this.e, {super.key, this.size = 13});
  final CalEvent e;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    if (e.actual.isEmpty) return Text('—', style: context.text.mono(size, color: k.fg3));
    return Text(
      e.actual,
      textDirection: TextDirection.ltr,
      style: context.text.mono(size, weight: FontWeight.w700, color: e.surprise == 1 ? k.up : (e.surprise == -1 ? k.down : k.fg)),
    );
  }
}

/// "GMT+3".
String gmt(int offset) => 'GMT${offset >= 0 ? '+' : ''}$offset';

// Quotes tab (web MWatch, owner design 2026-10): a search and the Simple / Advanced switch, the asset-class chips with
// counts (and ★ favourites), then the markets in ONE rounded card where every row sits on the same fixed column grid
// as the header above it (QuoteGrid): icon · symbol (today's change % and the spread in points) · the 24 h sparkline ·
// SELL (bid) · BUY (ask), MT5-style big figures with the raised pipette, each button flashing up / down on a tick.
// Advanced adds the day's low · high and the last tick time on the same grid. Browsing shows the core markets (and the
// favourites); a search looks through every market the account may trade (live accounts: core + live-enabled
// catalogue; demo: everything), first 60 matches. Only rows on screen stream live (and fetch their sparkline). Tap a
// row: the symbol screen (order book); SELL / BUY: the terminal's order path (one-click on: at once; off: the ticket).
// A delayed price shows a clock and its buttons stay off. Swipe or hold: favourite / chart / new order.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/market_hours.dart';
import '../core/models.dart';
import '../core/sparklines.dart';
import '../core/terminal_controller.dart';
import '../core/trade_math.dart';
import '../core/workspace.dart';
import '../terminal_screen.dart';
import '../widgets/kit.dart';
import 'order_sheet.dart';
import 'room.dart';

bool inSegment(SymbolSpec s, String seg, List<String> favourites) =>
    seg == 'all' || (seg == 'favourites' ? favourites.contains(s.symbol) : s.assetClass == seg);

String segmentLabel(T t, String s) => s == 'all' ? t('common.all') : t.dyn('market.segment.$s', fallback: s);

/// The segment chips with counts (web SegmentChips size md): empty classes are left out; All and ★ always show.
class SegmentChips extends StatelessWidget {
  const SegmentChips({super.key, required this.instruments, required this.value, required this.favourites, required this.onChanged});
  final List<SymbolSpec> instruments;
  final String value;
  final List<String> favourites;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final counts = {for (final s in kSegments) s: instruments.where((i) => inSegment(i, s, favourites)).length};
    final shown = kSegments.where((s) => s == 'favourites' || s == 'all' || (counts[s] ?? 0) > 0 || s == value).toList();
    final current = shown.contains(value) ? value : 'all';
    return SizedBox(
      height: 36,
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: kRoomGutter),
        children: [
          for (final s in shown)
            Padding(
              padding: const EdgeInsetsDirectional.only(end: 6),
              child: KPressable(
                minSize: 36,
                pressedScale: 0.97,
                semanticLabel: t('market.segment.title', {'label': segmentLabel(t, s), 'count': counts[s] ?? 0}),
                onTap: () {
                  KHaptics.selection();
                  onChanged(s);
                },
                child: Container(
                  height: 30,
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  decoration: BoxDecoration(color: s == current ? k.emberSoft : k.surface, borderRadius: BorderRadius.circular(15)),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      if (s == 'favourites')
                        Icon(LucideIcons.star, size: 13, color: s == current ? k.ember : k.fg2)
                      else
                        Text(
                          segmentLabel(t, s),
                          style: context.text.label.copyWith(fontSize: 12.5, fontWeight: FontWeight.w600, color: s == current ? k.ember : k.fg2),
                        ),
                      const SizedBox(width: 5),
                      Text('${counts[s] ?? 0}', style: context.text.mono(11, color: s == current ? k.ember.withValues(alpha: 0.8) : k.fg3)),
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

class WatchlistTab extends ConsumerStatefulWidget {
  const WatchlistTab({super.key});

  @override
  ConsumerState<WatchlistTab> createState() => _WatchlistTabState();
}

class _WatchlistTabState extends ConsumerState<WatchlistTab> {
  String _q = '';

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final book = ref.watch(symbolBookProvider);
    final ws = ref.watch(workspaceProvider);
    final live = ref.watch(terminalProvider.select((s) => s.account?.live ?? true));
    final visible = book.visible(live: live);
    final core = book.core;
    final seg = ws.segment;
    final advanced = ws.advancedQuotes;
    final q = _q.trim().toLowerCase();
    final List<SymbolSpec> list;
    if (q.isNotEmpty) {
      list = visible
          .where((i) => inSegment(i, seg, ws.favourites))
          .where((i) => i.symbol.toLowerCase().contains(q) || i.name.toLowerCase().contains(q))
          .take(60)
          .toList();
    } else if (seg == 'favourites') {
      list = [
        for (final f in ws.favourites)
          if (book.isVisible(f, live: live)) book[f]!,
      ];
    } else {
      list = core.where((i) => inSegment(i, seg, ws.favourites)).toList();
    }
    final head = context.text.micro.copyWith(color: k.fg3, fontSize: 10, letterSpacing: 0.7, fontWeight: FontWeight.w700);
    return LayoutBuilder(
      builder: (context, box) {
        final grid = QuoteGrid.of(box.maxWidth - 2 * kRoomGutter);
        return Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(kRoomGutter, 10, kRoomGutter, 8),
              child: Row(
                children: [
                  Expanded(
                    child: Container(
                      height: 40,
                      padding: const EdgeInsets.symmetric(horizontal: 14),
                      decoration: BoxDecoration(color: k.surface2, borderRadius: BorderRadius.circular(20)),
                      child: Row(
                        children: [
                          Icon(LucideIcons.search, size: 16, color: k.fg3),
                          const SizedBox(width: 8),
                          Expanded(
                            child: TextField(
                              onChanged: (v) => setState(() => _q = v),
                              style: context.text.callout.copyWith(fontSize: 14),
                              cursorColor: k.ember,
                              textInputAction: TextInputAction.search,
                              decoration: InputDecoration(
                                isCollapsed: true,
                                border: InputBorder.none,
                                hintText: t('trader.searchSymbols'),
                                hintStyle: context.text.callout.copyWith(fontSize: 14, color: k.fg3),
                              ),
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  KPressable(
                    key: const ValueKey('quotes-view'),
                    semanticLabel: t('app.trader.quotes.viewAria'),
                    onTap: () {
                      KHaptics.selection();
                      ref.read(workspaceProvider.notifier).update((w) => w.copyWith(advancedQuotes: !w.advancedQuotes));
                    },
                    child: Container(
                      height: 40,
                      padding: const EdgeInsets.symmetric(horizontal: 14),
                      alignment: Alignment.center,
                      decoration: BoxDecoration(color: k.surface2, borderRadius: BorderRadius.circular(20)),
                      child: Text(
                        t(advanced ? 'app.trader.quotes.advanced' : 'app.trader.quotes.simple'),
                        style: context.text.label.copyWith(fontSize: 12, fontWeight: FontWeight.w600, color: k.fg2),
                      ),
                    ),
                  ),
                ],
              ),
            ),
            SegmentChips(
              instruments: core,
              value: seg,
              favourites: ws.favourites,
              onChanged: (s) => ref.read(workspaceProvider.notifier).update((w) => w.copyWith(segment: s)),
            ),
            const SizedBox(height: 8),
            // the header on the rows' own grid
            Padding(
              padding: const EdgeInsets.symmetric(horizontal: kRoomGutter),
              child: SizedBox(
                key: const ValueKey('quotes-header'),
                height: 22,
                child: grid.row(
                  icon: const SizedBox.shrink(),
                  symbol: Text(t('market.col.symbol').toUpperCase(), maxLines: 1, style: head),
                  spark: Text(t('app.trader.quotes.col24h').toUpperCase(), maxLines: 1, textAlign: TextAlign.center, style: head),
                  sell: Text(t('common.sell').toUpperCase(), maxLines: 1, textAlign: TextAlign.center, style: head),
                  buy: Text(t('common.buy').toUpperCase(), maxLines: 1, textAlign: TextAlign.center, style: head),
                ),
              ),
            ),
            Expanded(
              child: book.all.isEmpty
                  ? ListView(
                      padding: const EdgeInsets.symmetric(horizontal: kRoomGutter),
                      children: [for (var i = 0; i < 8; i++) _RowSkeleton(grid: grid)],
                    )
                  : list.isEmpty
                  ? TEmptyLine(seg == 'favourites' && q.isEmpty ? t('trader.mobile.noFavourites') : t('trader.mobile.noSymbols'))
                  : ListView.builder(
                      padding: const EdgeInsets.fromLTRB(kRoomGutter, 0, kRoomGutter, 20),
                      itemCount: list.length,
                      itemExtent: advanced ? QuoteRow.height + QuoteRow.advancedHeight : QuoteRow.height,
                      itemBuilder: (context, i) => QuoteRow(
                        spec: list[i],
                        grid: grid,
                        advanced: advanced,
                        first: i == 0,
                        last: i == list.length - 1,
                        favourite: ws.favourites.contains(list[i].symbol),
                      ),
                    ),
            ),
          ],
        );
      },
    );
  }
}

class _RowSkeleton extends StatelessWidget {
  const _RowSkeleton({required this.grid});
  final QuoteGrid grid;

  @override
  Widget build(BuildContext context) => Container(
    height: QuoteRow.height,
    color: context.k.surface,
    child: grid.row(
      icon: const Center(child: KSkeleton(width: 28, height: 28, circle: true)),
      symbol: const Align(alignment: AlignmentDirectional.centerStart, child: KSkeleton(width: 70, height: 12)),
      spark: const SizedBox.shrink(),
      sell: const KSkeleton(height: 44),
      buy: const KSkeleton(height: 44),
    ),
  );
}

/// The section of a card a row is: rounded at the top of the first and the bottom of the last.
BorderRadius _cardRadius(bool first, bool last) => BorderRadius.vertical(
  top: first ? const Radius.circular(kRoomRadius) : Radius.zero,
  bottom: last ? const Radius.circular(kRoomRadius) : Radius.zero,
);

/// One market on the quotes grid (web MWatchRow): avatar, symbol + today's change % · spread, the 24 h sparkline, SELL
/// (bid) and BUY (ask); Advanced: low · high · last tick time under it.
class QuoteRow extends ConsumerWidget {
  const QuoteRow({
    super.key,
    required this.spec,
    required this.grid,
    this.advanced = false,
    this.first = false,
    this.last = false,
    this.favourite = false,
  });
  final SymbolSpec spec;
  final QuoteGrid grid;
  final bool advanced, first, last, favourite;

  static const double height = 64;
  static const double advancedHeight = 22;

  void _open(WidgetRef ref) => openSymbolScreen(ref, spec.symbol);

  void _chart(WidgetRef ref) {
    ref.read(workspaceProvider.notifier).update((w) => w.copyWith(symbol: spec.symbol));
    ref.read(cfdTabProvider.notifier).set('chart');
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final feed = ref.read(marketFeedProvider);
    final open = isMarketOpen(spec);
    final body = Container(
      decoration: BoxDecoration(color: k.surface, borderRadius: _cardRadius(first, last)),
      // the separator is drawn over the row (it takes no height: every row keeps the grid's exact height)
      foregroundDecoration: last ? null : BoxDecoration(border: Border(bottom: BorderSide(color: k.line, width: 0.6))),
      child: QuoteBuilder(
        symbol: spec.symbol,
        builder: (context, q) {
          final ch = q?.change ?? 0;
          final delayed = q?.delayed ?? false;
          final valid = q != null && q.valid;
          final tradable = valid && open && !delayed && !readOnly;
          final chColor = ch >= 0 ? k.up : k.down;
          final spread = valid ? spreadPoints(spec, q.bid, q.ask) : null;
          Widget price(String side) => QuoteButton(
            key: ValueKey('quote-$side-${spec.symbol}'),
            value: side == 'buy' ? (q?.ask ?? 0) : (q?.bid ?? 0),
            digits: spec.digits,
            enabled: tradable,
            dim: delayed || !open,
            semanticLabel: t(side == 'buy' ? 'app.trader.quotes.buyAt' : 'app.trader.quotes.sellAt', {
              'symbol': spec.symbol,
              'price': valid ? fmtPrice(spec.digits, side == 'buy' ? q.ask : q.bid) : '—',
            }),
            onTap: () => unawaited(tradeFrom(context, ref, spec.symbol, side)),
          );
          final main = SizedBox(
            height: height,
            child: grid.row(
              icon: Center(child: SymbolAvatar(spec.symbol, size: 24)),
              symbol: Semantics(
                button: true,
                label: t('app.trader.quotes.openBook', {'symbol': spec.symbol}),
                excludeSemantics: true,
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  mainAxisAlignment: MainAxisAlignment.center,
                  children: [
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            spec.symbol,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.label.copyWith(fontSize: 15, fontWeight: FontWeight.w700, color: k.fg),
                          ),
                        ),
                        if (favourite) ...[const SizedBox(width: 3), Icon(Icons.star_rounded, size: 11, color: k.gold)],
                        if (delayed) ...[const SizedBox(width: 3), Icon(LucideIcons.clock, size: 11, color: k.warn)],
                      ],
                    ),
                    const SizedBox(height: 3),
                    Text.rich(
                      TextSpan(
                        children: [
                          // Geist with tabular figures (narrower than the mono): the change and the spread fit the column
                          TextSpan(
                            text: '${ch >= 0 ? '+' : ''}${ch.toStringAsFixed(2)}%',
                            style: context.text.caption.copyWith(fontSize: 11, fontWeight: FontWeight.w600, color: chColor, fontFeatures: kTabular),
                          ),
                          if (spread != null)
                            TextSpan(
                              text: ' · ${t('order.unit.pts', {'n': spread})}',
                              style: context.text.caption.copyWith(fontSize: 11, fontWeight: FontWeight.w500, color: k.fg3, fontFeatures: kTabular),
                            ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      textDirection: TextDirection.ltr,
                    ),
                  ],
                ),
              ),
              spark: QuoteSparkline(symbol: spec.symbol, color: chColor),
              sell: price('sell'),
              buy: price('buy'),
            ),
          );
          if (!advanced) return main;
          final day = feed.day(spec.symbol);
          final lo = day == null || !valid ? null : (q.bid < day.low ? q.bid : day.low);
          final hi = day == null || !valid ? null : (q.bid > day.high ? q.bid : day.high);
          final small = context.text.mono(10, color: k.fg2);
          return Column(
            children: [
              main,
              SizedBox(
                key: ValueKey('quote-adv-${spec.symbol}'),
                height: advancedHeight,
                child: Transform.translate(
                  offset: const Offset(0, -5),
                  child: grid.row(
                    icon: const SizedBox.shrink(),
                    symbol: Text(valid && q.time > 0 ? _clock(q.time) : '', maxLines: 1, style: small),
                    spark: const SizedBox.shrink(),
                    sell: Text(
                      lo == null ? '' : t('app.trader.quotes.low', {'price': fmtPrice(spec.digits, lo)}),
                      maxLines: 1,
                      textAlign: TextAlign.center,
                      style: small,
                    ),
                    buy: Text(
                      hi == null ? '' : t('app.trader.quotes.high', {'price': fmtPrice(spec.digits, hi)}),
                      maxLines: 1,
                      textAlign: TextAlign.center,
                      style: small,
                    ),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
    return KSwipeable(
      background: k.bg,
      actions: [
        KSwipeAction(
          label: t('market.segment.favourites'),
          icon: favourite ? Icons.star_rounded : Icons.star_outline_rounded,
          color: k.gold,
          onTap: () => ref.read(workspaceProvider.notifier).toggleFavourite(spec.symbol),
        ),
      ],
      child: KPressable(
        pressedScale: 1,
        pressedOpacity: 0.85,
        haptic: false,
        onTap: () => _open(ref),
        onLongPress: () => unawaited(
          showKActionSheet<void>(
            context,
            title: '${spec.symbol} · ${spec.name}',
            actions: [
              KAction(label: t('trader.mobile.tab.chart'), icon: LucideIcons.candlestickChart, onTap: () => _chart(ref)),
              if (!readOnly)
                KAction(
                  label: t('chart.menu.newOrder'),
                  icon: LucideIcons.shoppingCart,
                  onTap: () => unawaited(showOrderSheet(context, symbol: spec.symbol)),
                ),
              KAction(
                label: t('market.segment.favourites'),
                icon: favourite ? LucideIcons.starOff : LucideIcons.star,
                onTap: () => ref.read(workspaceProvider.notifier).toggleFavourite(spec.symbol),
              ),
            ],
          ),
        ),
        child: body,
      ),
    );
  }

  /// The last tick's time in server time (GMT+3 / +2), HH:mm:ss.
  static String _clock(int ms) {
    final utc = DateTime.fromMillisecondsSinceEpoch(ms, isUtc: true);
    final s = utc.add(Duration(seconds: serverOffsetSeconds(utc)));
    String p(int n) => n.toString().padLeft(2, '0');
    return '${p(s.hour)}:${p(s.minute)}:${p(s.second)}';
  }
}

/// A SELL / BUY price button of the quotes (MT5 big figures): the head small, the two figures big, the pipette raised;
/// it flashes the up / down colour behind the price on every tick and keeps the colour of the last move.
class QuoteButton extends StatefulWidget {
  const QuoteButton({
    super.key,
    required this.value,
    required this.digits,
    required this.onTap,
    this.enabled = true,
    this.dim = false,
    this.semanticLabel,
    this.height = 44,
  });
  final double value;
  final int digits;
  final VoidCallback onTap;
  final bool enabled;

  /// A delayed price or a closed market: the figures grey.
  final bool dim;
  final String? semanticLabel;
  final double height;

  @override
  State<QuoteButton> createState() => _QuoteButtonState();
}

class _QuoteButtonState extends State<QuoteButton> {
  int _dir = 0;
  bool _flash = false;
  Timer? _timer;

  @override
  void didUpdateWidget(QuoteButton old) {
    super.didUpdateWidget(old);
    if (old.value != widget.value && old.value > 0 && widget.value > 0) {
      _dir = widget.value > old.value ? 1 : -1;
      _flash = true;
      _timer?.cancel();
      _timer = Timer(const Duration(milliseconds: 650), () {
        if (mounted) setState(() => _flash = false);
      });
    }
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final v = widget.value;
    final fg = widget.dim ? k.fg3 : (_dir > 0 ? k.up : (_dir < 0 ? k.down : k.fg));
    final bg = _flash && !widget.dim ? (_dir > 0 ? k.upSoft : k.downSoft) : k.surface2;
    final Widget text;
    if (!v.isFinite || v <= 0) {
      text = Text('—', style: context.text.mono(14, color: k.fg3));
    } else {
      final p = splitQuote(v, widget.digits);
      text = FittedBox(
        fit: BoxFit.scaleDown,
        child: Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          textDirection: TextDirection.ltr,
          children: [
            Padding(
              padding: const EdgeInsets.only(top: 5),
              child: Text(p.head, style: context.text.mono(11, weight: FontWeight.w600, color: fg)),
            ),
            Text(p.big, style: context.text.mono(16, weight: FontWeight.w700, color: fg).copyWith(height: 1.15)),
            if (p.sup.isNotEmpty) Text(p.sup, style: context.text.mono(10, weight: FontWeight.w600, color: fg)),
          ],
        ),
      );
    }
    return KPressable(
      minSize: widget.height,
      pressedScale: 0.96,
      semanticLabel: widget.semanticLabel,
      onTap: widget.enabled ? widget.onTap : null,
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 220),
        height: widget.height,
        padding: const EdgeInsets.symmetric(horizontal: 4),
        alignment: Alignment.center,
        decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(12)),
        child: text,
      ),
    );
  }
}

/// The 24 h sparkline of a market (the last 24 hourly closes, real candles, cached ~5 min); nothing when there are
/// none.
class QuoteSparkline extends ConsumerStatefulWidget {
  const QuoteSparkline({super.key, required this.symbol, required this.color, this.width = 40, this.height = 22});
  final String symbol;
  final Color color;
  final double width, height;

  @override
  ConsumerState<QuoteSparkline> createState() => _QuoteSparklineState();
}

class _QuoteSparklineState extends ConsumerState<QuoteSparkline> {
  List<double>? _data;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(QuoteSparkline old) {
    super.didUpdateWidget(old);
    if (old.symbol != widget.symbol) {
      _data = null;
      _load();
    }
  }

  void _load() {
    final cache = ref.read(sparkCacheProvider);
    final hit = cache.peek(widget.symbol);
    if (hit != null) {
      _data = hit;
      return;
    }
    final symbol = widget.symbol;
    unawaited(
      cache.get(symbol).then((d) {
        if (mounted && symbol == widget.symbol) setState(() => _data = d);
      }),
    );
  }

  @override
  Widget build(BuildContext context) {
    final d = _data;
    if (d == null || d.length < 2) return SizedBox(width: widget.width, height: widget.height);
    return ExcludeSemantics(
      child: KSparkline(d, width: widget.width, height: widget.height, color: widget.color, fill: false),
    );
  }
}

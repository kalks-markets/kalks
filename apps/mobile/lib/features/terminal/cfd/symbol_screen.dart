// The symbol screen (owner design 2026-10), opened by tapping a market in the quotes: back · Specs, the name, the big
// mid price with today's change and the spread, a small area chart of the market's recent real candles (M15, the last
// day), the ORDER BOOK (5 asks · spread · 5 bids with volume bars) from the market-data depth stream (the same source
// as the depth-of-market ladder: provider depth when the feed carries it, else the service's indicative ladder,
// labelled so), big SELL / BUY through the terminal's order path, and Open chart. No depth at all: the bid and the ask
// only, with "Depth unavailable · prices only" — volumes are never made up here.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/market_hours.dart';
import '../core/terminal_controller.dart';
import '../core/trade_math.dart';
import '../core/workspace.dart';
import '../terminal_screen.dart';
import '../widgets/kit.dart';
import 'dom_ladder.dart';
import 'room.dart';
import 'spec_sheet.dart';

/// Where the order book's levels come from.
enum BookSource {
  /// Provider depth (`src: feed`): real sizes.
  feed,

  /// The market-data service's ladder from the live bid / ask (`src: indicative`), labelled Indicative.
  indicative,

  /// No depth frame: the bid and the ask only, no sizes.
  none,
}

/// The order book as the symbol screen draws it: asks farthest first (the best ask last, next to the spread), bids
/// best first; (price, lots). Without depth the ask and the bid alone, with no lots.
@immutable
class BookLadder {
  const BookLadder({required this.source, required this.asks, required this.bids});
  final BookSource source;
  final List<(double, double)> asks, bids;

  /// Volumes are shown (a depth frame came).
  bool get sized => source != BookSource.none;

  /// The largest level (the volume bars' 100 %).
  double get maxLots => [0.0, ...asks.map((a) => a.$2), ...bids.map((b) => b.$2)].reduce(math.max);

  /// From the latest depth frame of the stream (`{src, b: [[price, lots]…], a: […]}`, best first) or none.
  static BookLadder of(Map<String, dynamic>? frame, {required double bid, required double ask, int levels = 5}) {
    List<(double, double)> side(Object? v) => [
      for (final x in (v is List ? v : const []))
        if (x is List && x.length >= 2 && x[0] is num && x[1] is num) ((x[0] as num).toDouble(), (x[1] as num).toDouble()),
    ];
    final b = frame == null ? const <(double, double)>[] : side(frame['b']);
    final a = frame == null ? const <(double, double)>[] : side(frame['a']);
    if (b.isEmpty || a.isEmpty) {
      return BookLadder(
        source: BookSource.none,
        asks: [if (ask > 0) (ask, 0)],
        bids: [if (bid > 0) (bid, 0)],
      );
    }
    return BookLadder(
      source: frame!['src'] == 'feed' ? BookSource.feed : BookSource.indicative,
      asks: a.take(levels).toList().reversed.toList(),
      bids: b.take(levels).toList(),
    );
  }
}

class SymbolScreen extends ConsumerStatefulWidget {
  const SymbolScreen({super.key, required this.symbol});
  final String symbol;

  @override
  ConsumerState<SymbolScreen> createState() => _SymbolScreenState();
}

class _SymbolScreenState extends ConsumerState<SymbolScreen> {
  Map<String, dynamic>? _depth;
  void Function()? _unsub;
  List<double> _line = const [];

  @override
  void initState() {
    super.initState();
    _start();
  }

  @override
  void didUpdateWidget(SymbolScreen old) {
    super.didUpdateWidget(old);
    if (old.symbol != widget.symbol) {
      _unsub?.call();
      _depth = null;
      _line = const [];
      _start();
    }
  }

  void _start() {
    final feed = ref.read(marketFeedProvider);
    final symbol = widget.symbol;
    _unsub = feed.subscribeDepth(symbol, (d) {
      if (mounted && d['s'] == widget.symbol) setState(() => _depth = d);
    });
    // the area chart: the last day of M15 closes (real candles; none: no chart)
    unawaited(
      feed.candles(symbol, 'M15', limit: 96).then((bars) {
        if (!mounted || symbol != widget.symbol) return;
        setState(() => _line = [for (final b in bars) if (b.c > 0) b.c]);
      }),
    );
  }

  @override
  void dispose() {
    _unsub?.call();
    super.dispose();
  }

  void _back() => ref.read(cfdTabProvider.notifier).set('watch');

  void _chart() {
    ref.read(workspaceProvider.notifier).update((w) => w.copyWith(symbol: widget.symbol));
    ref.read(cfdTabProvider.notifier).set('chart');
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final symbol = widget.symbol;
    final spec = ref.watch(symbolBookProvider.select((b) => b[symbol]));
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final digits = spec?.digits ?? 5;
    final open = isMarketOpen(spec);
    return ColoredBox(
      color: k.bg,
      child: QuoteBuilder(
        symbol: symbol,
        builder: (context, q) {
          final valid = q != null && q.valid;
          final bid = q?.bid ?? 0, ask = q?.ask ?? 0;
          final ch = q?.change ?? 0;
          final chColor = ch >= 0 ? k.up : k.down;
          final spread = valid && spec != null ? t('order.unit.pts', {'n': spreadPoints(spec, bid, ask)}) : '—';
          final tradable = valid && open && !(q.delayed) && !readOnly && spec != null;
          final line = [..._line, if (valid && _line.isNotEmpty) q.bid];
          return ListView(
            padding: const EdgeInsets.only(bottom: 20),
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(12, 6, 12, 0),
                child: Row(
                  children: [
                    RoomIconButton(icon: LucideIcons.chevronLeft, label: t('common.back'), onTap: _back),
                    const Spacer(),
                    KPressable(
                      key: const ValueKey('symbol-specs'),
                      semanticLabel: t('order.spec.title', {'symbol': symbol}),
                      onTap: spec == null ? null : () => unawaited(showSpecSheet(context, symbol)),
                      child: Container(
                        height: 40,
                        padding: const EdgeInsets.symmetric(horizontal: 14),
                        alignment: Alignment.center,
                        decoration: BoxDecoration(color: k.surface2, borderRadius: BorderRadius.circular(20)),
                        child: Text(t('app.trader.symbol.specs'), style: context.text.label.copyWith(fontSize: 12, fontWeight: FontWeight.w600, color: k.fg2)),
                      ),
                    ),
                  ],
                ),
              ),
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 6, 20, 0),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    if (spec != null) Text(spec.name, style: context.text.footnote.copyWith(fontSize: 13, color: k.fg3)),
                    const SizedBox(height: 4),
                    Row(
                      children: [
                        SymbolAvatar(symbol, size: 18),
                        const SizedBox(width: 8),
                        Text(symbol, style: context.text.label.copyWith(fontSize: 15, fontWeight: FontWeight.w700, color: k.fg)),
                      ],
                    ),
                    const SizedBox(height: 4),
                    FittedBox(
                      fit: BoxFit.scaleDown,
                      alignment: AlignmentDirectional.centerStart,
                      child: Text(
                        valid ? fmtPrice(digits, (bid + ask) / 2) : '—',
                        key: const ValueKey('symbol-mid'),
                        textDirection: TextDirection.ltr,
                        style: context.text.mono(38, weight: FontWeight.w800, color: k.fg).copyWith(letterSpacing: -1, height: 1.1),
                      ),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      t('app.trader.symbol.today', {'change': '${ch >= 0 ? '+' : ''}${ch.toStringAsFixed(2)}%', 'spread': spread}),
                      style: context.text.label.copyWith(fontSize: 13, fontWeight: FontWeight.w600, color: chColor),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 10),
              SizedBox(
                height: 96,
                child: line.length < 2
                    ? null
                    : ExcludeSemantics(
                        child: CustomPaint(painter: _AreaPainter(line, chColor)),
                      ),
              ),
              const SizedBox(height: 14),
              _OrderBook(ladder: BookLadder.of(_depth, bid: bid, ask: ask), digits: digits, spread: spread, symbol: symbol),
              Padding(
                padding: const EdgeInsets.fromLTRB(kRoomGutter, 14, kRoomGutter, 6),
                child: Row(
                  children: [
                    for (final s in const ['sell', 'buy']) ...[
                      if (s == 'buy') const SizedBox(width: 8),
                      Expanded(
                        child: _BigSide(
                          key: ValueKey('symbol-$s'),
                          side: s,
                          price: valid ? fmtPrice(digits, s == 'buy' ? ask : bid) : '—',
                          enabled: tradable,
                          onTap: () => unawaited(tradeFrom(context, ref, symbol, s)),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: kRoomGutter),
                child: KPressable(
                  key: const ValueKey('symbol-chart'),
                  onTap: _chart,
                  child: Container(
                    height: 48,
                    decoration: BoxDecoration(color: k.surface2, borderRadius: BorderRadius.circular(16)),
                    child: Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        Icon(LucideIcons.chartSpline, size: 18, color: k.fg),
                        const SizedBox(width: 8),
                        Text(t('app.trader.symbol.openChart'), style: context.text.label.copyWith(fontSize: 14, fontWeight: FontWeight.w700, color: k.fg)),
                      ],
                    ),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    );
  }
}

/// The 5 asks · spread · 5 bids card (volume bars when the depth carries sizes).
class _OrderBook extends ConsumerWidget {
  const _OrderBook({required this.ladder, required this.digits, required this.spread, required this.symbol});
  final BookLadder ladder;
  final int digits;
  final String spread, symbol;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final max = ladder.maxLots;
    Widget level((double, double) lv, bool ask) {
      final c = ask ? k.down : k.up;
      return SizedBox(
        height: 26,
        child: Stack(
          children: [
            if (ladder.sized && max > 0)
              Positioned.fill(
                top: 3,
                bottom: 3,
                child: Align(
                  alignment: Alignment.centerRight,
                  child: FractionallySizedBox(
                    widthFactor: (lv.$2 / max).clamp(0.0, 1.0),
                    heightFactor: 1,
                    child: Container(
                      decoration: BoxDecoration(color: ask ? k.downSoft : k.upSoft, borderRadius: BorderRadius.circular(5)),
                    ),
                  ),
                ),
              ),
            Positioned.fill(
              child: Row(
                children: [
                  Text(fmtPrice(digits, lv.$1), textDirection: TextDirection.ltr, style: context.text.mono(13, weight: FontWeight.w600, color: c)),
                  const Spacer(),
                  if (ladder.sized) Text(lv.$2.toStringAsFixed(2), style: context.text.mono(13, color: k.fg2)),
                ],
              ),
            ),
          ],
        ),
      );
    }

    final head = context.text.micro.copyWith(color: k.fg3, fontSize: 11, letterSpacing: 0.6, fontWeight: FontWeight.w600);
    return RoomCard(
      key: const ValueKey('symbol-book'),
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Text(t('desk.ob.title').toUpperCase(), style: head),
              const Spacer(),
              if (ladder.sized) Text(t('desk.ob.lots').toUpperCase(), style: head),
            ],
          ),
          const SizedBox(height: 6),
          for (final a in ladder.asks) level(a, true),
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Text(
              '${t('chart.quote.spread')} $spread',
              textAlign: TextAlign.center,
              style: context.text.mono(11, color: k.fg3),
            ),
          ),
          for (final b in ladder.bids) level(b, false),
          const SizedBox(height: 8),
          Row(
            children: [
              if (ladder.source == BookSource.indicative) ...[
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
                  decoration: BoxDecoration(borderRadius: BorderRadius.circular(4), border: Border.all(color: k.line)),
                  child: Text(t('order.dom.indicative').toUpperCase(), style: context.text.micro.copyWith(color: k.fg2, fontSize: 9)),
                ),
                const SizedBox(width: 6),
              ],
              Expanded(
                child: Text(
                  switch (ladder.source) {
                    BookSource.feed => t('order.dom.feed'),
                    BookSource.indicative => t('order.dom.indicativeNote'),
                    BookSource.none => t('order.dom.unavailable'),
                  },
                  key: const ValueKey('symbol-book-note'),
                  style: context.text.caption.copyWith(color: k.fg3),
                ),
              ),
              // the full ladder with click-to-trade (web DOM)
              KPressable(
                semanticLabel: t('order.panel.tabDepth'),
                onTap: () => unawaited(
                  showKSheet<void>(
                    context,
                    title: '${t('order.panel.tabDepth')} · $symbol',
                    builder: (_) => SizedBox(height: 560, child: DomLadder(symbol: symbol)),
                  ),
                ),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(t('order.panel.tabDepth'), style: context.text.caption.copyWith(color: k.ember, fontWeight: FontWeight.w600)),
                    Icon(LucideIcons.chevronRight, size: 14, color: k.ember),
                  ],
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// The symbol screen's big SELL / BUY (54 px): the side and its live price.
class _BigSide extends StatelessWidget {
  const _BigSide({super.key, required this.side, required this.price, required this.enabled, required this.onTap});
  final String side, price;
  final bool enabled;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final buy = side == 'buy';
    return KPressable(
      pressedScale: 0.97,
      haptic: false,
      semanticLabel: '${t(buy ? 'common.buy' : 'common.sell')} $price',
      onTap: enabled ? onTap : null,
      child: Container(
        height: 54,
        width: double.infinity,
        decoration: BoxDecoration(color: enabled ? (buy ? k.buyFill : k.sellFill) : k.surface3, borderRadius: BorderRadius.circular(16)),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Text(
              t(buy ? 'common.buy' : 'common.sell').toUpperCase(),
              style: context.text.micro.copyWith(fontSize: 11, fontWeight: FontWeight.w700, letterSpacing: 0.5, color: enabled ? Colors.white : k.fg3),
            ),
            Text(price, textDirection: TextDirection.ltr, style: context.text.mono(16, weight: FontWeight.w700, color: enabled ? Colors.white : k.fg3)),
          ],
        ),
      ),
    );
  }
}

/// The small area chart: the closes as a line over a soft fill in the day's colour.
class _AreaPainter extends CustomPainter {
  _AreaPainter(this.data, this.color);
  final List<double> data;
  final Color color;

  @override
  void paint(Canvas canvas, Size size) {
    final lo = data.reduce(math.min), hi = data.reduce(math.max);
    final r = hi - lo == 0 ? 1.0 : hi - lo;
    const pad = 8.0;
    Offset at(int i) => Offset(i / (data.length - 1) * size.width, pad + (hi - data[i]) / r * (size.height - 2 * pad));
    final path = Path()..moveTo(at(0).dx, at(0).dy);
    for (var i = 1; i < data.length; i++) {
      path.lineTo(at(i).dx, at(i).dy);
    }
    final area = Path.from(path)
      ..lineTo(size.width, size.height)
      ..lineTo(0, size.height)
      ..close();
    canvas.drawPath(
      area,
      Paint()
        ..shader = LinearGradient(
          begin: Alignment.topCenter,
          end: Alignment.bottomCenter,
          colors: [color.withValues(alpha: 0.22), color.withValues(alpha: 0)],
        ).createShader(Offset.zero & size),
    );
    canvas.drawPath(
      path,
      Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2
        ..strokeJoin = StrokeJoin.round,
    );
  }

  @override
  bool shouldRepaint(_AreaPainter old) => old.color != color || !listEquals(old.data, data);
}

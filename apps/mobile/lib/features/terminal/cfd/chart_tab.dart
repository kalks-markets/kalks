// Chart tab (web MChart, owner design 2026-10): the chart fills the tab (no header, no frame): the symbol pill (tap:
// another market) and the timeframe control (timeframe_picker.dart: the timeframe chip and the starred timeframes)
// float at the top with the chart menu (type, indicators,
// templates), ONE slim pill under them trades: [S bid] − volume + [ask B] (at the top, so the newest candles, the
// time axis and the Kalks mark at the bottom stay clear), and the OHLC legend sits under it. The pill uses the same
// order path as the rest of the terminal (one-click trading on: at once, with a haptic; off: the prefilled order ticket). Trade lines are compact tags: "B 0.50 +12.30", "S 1.11850" (stop), "T 1.12600" (target).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../chart/chart_bridge.dart';
import '../chart/terminal_chart.dart';
import '../core/market.dart';
import '../core/market_hours.dart';
import '../core/models.dart';
import '../core/order.dart';
import '../core/terminal_controller.dart';
import '../core/trade_actions.dart';
import '../core/trade_math.dart';
import '../core/workspace.dart';
import '../widgets/kit.dart';
import 'chart_menu.dart';
import 'order_sheet.dart';
import 'position_sheet.dart';
import 'room.dart';
import 'symbol_search.dart';
import 'timeframe_picker.dart';

/// Room the floating top controls take over the plot (the chart's legend starts below).
const double kChartTopBar = 48;

/// Room the one-click pill takes under the top controls (36 px pill + 3 px padding each side + a 6 px gap).
const double kChartTradePill = 48;

class ChartTab extends ConsumerWidget {
  const ChartTab({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final ws = ref.watch(workspaceProvider);
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final symbol = ws.symbol;
    final glass = k.surface.withValues(alpha: 0.92);
    return Stack(
      children: [
        Positioned.fill(
          child: _Chart(symbol: symbol, tf: ws.tf, topInset: readOnly ? kChartTopBar : kChartTopBar + kChartTradePill),
        ),
        // the symbol pill · timeframes · chart menu, floating over the top of the plot (a 44 px row: the pills are 34 px
        // tall, the timeframes' touch targets the full 44)
        Positioned(
          top: 1,
          left: 12,
          right: 12,
          child: Row(
            children: [
              KPressable(
                key: const ValueKey('chart-symbol'),
                semanticLabel: t('chart.toolbar.symbol'),
                onTap: () => showSymbolSearch(context),
                child: Container(
                  height: 34,
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  decoration: BoxDecoration(color: glass, borderRadius: BorderRadius.circular(17)),
                  child: Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      SymbolAvatar(symbol, size: 15),
                      const SizedBox(width: 6),
                      Text(
                        symbol,
                        style: context.text.label.copyWith(fontWeight: FontWeight.w800, fontSize: 13, color: k.fg),
                      ),
                      const SizedBox(width: 4),
                      Icon(LucideIcons.chevronDown, size: 13, color: k.fg2),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 8),
              // the timeframe on screen (tap: every timeframe) and the starred ones, as many as fit: no sliding
              Flexible(child: TimeframeBar(glass: glass)),
              const SizedBox(width: 6),
              // chart type, indicators and templates (web chart toolbar)
              Container(
                height: 34,
                decoration: BoxDecoration(color: glass, borderRadius: BorderRadius.circular(17)),
                child: ChartMenuButton(symbol: symbol),
              ),
            ],
          ),
        ),
        // one-tap trading (MT5's one-click panel, slim): [S bid] − volume + [ask B], under the top controls
        if (!readOnly)
          Positioned(
            left: 0,
            right: 0,
            top: kChartTopBar,
            child: Center(child: OneClickPill(symbol: symbol)),
          ),
      ],
    );
  }
}

/// The chart with the account's trade lines (web useTradeLines + commitLineDrag).
class _Chart extends ConsumerWidget {
  const _Chart({required this.symbol, required this.tf, this.topInset = 0});
  final String symbol, tf;
  final double topInset;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final st = ref.watch(terminalProvider);
    final alerts = ref.watch(workspaceProvider.select((w) => w.alerts));
    final chart = ref.watch(workspaceProvider.select((w) => w.chartOf(symbol)));
    final ro = st.readOnly;
    final cent = st.account?.cent ?? false;
    final spec = ref.watch(symbolBookProvider.select((b) => b[symbol]));
    // the money on a position's tag (web chart-view: the position's P&L)
    String money(double usd) => accMoney(cent, usd, signed: true);
    String tone(double v) => v >= 0 ? 'up' : 'down';
    // the stops level in price units: how close to the price a stop may go (the S / T handles hold to it)
    final gap = spec == null ? 0.0 : spec.stopsLevelPoints * spec.point;
    final lines = <ChartLine>[
      for (final p in st.positions.where((p) => p.symbol == symbol)) ...[
        ChartLine(
          id: 'pos:${p.ticket}',
          kind: 'pos',
          price: p.openPrice,
          side: p.side,
          // the compact tag "B 0.50" with the P&L after it
          label: sideTag(t, p.side, p.volume),
          draggable: !ro,
          note: money(st.profitOf(p)),
          tone: tone(st.profitOf(p)),
          closable: !ro,
          // S / T: drag out a stop loss / take profit the position does not have yet (web trade-handles.ts)
          handles: ro || spec == null ? '' : '${p.sl == null ? 's' : ''}${p.tp == null ? 't' : ''}',
          pip: spec?.pipSize ?? 0,
          gap: gap,
        ),
        // "S 1.11850" / "T 1.12600": the page puts the line's price after the letter (also while dragged)
        if (p.sl != null)
          ChartLine(id: 'sl:${p.ticket}', kind: 'sl', price: p.sl!, side: p.side, label: t('chart.line.slHandle'), tag: true, draggable: !ro, closable: !ro),
        if (p.tp != null)
          ChartLine(id: 'tp:${p.ticket}', kind: 'tp', price: p.tp!, side: p.side, label: t('chart.line.tpHandle'), tag: true, draggable: !ro, closable: !ro),
      ],
      for (final o in st.orders.where((o) => o.symbol == symbol))
        ChartLine(
          id: 'pnd:${o.ticket}',
          kind: 'pending',
          price: o.price,
          side: o.side,
          label: t(_pendingLine(o.side, o.type), {'lot': fmtVol(o.volume)}),
          draggable: !ro,
          closable: !ro,
        ),
      for (final a in alerts.where((a) => a.symbol == symbol && a.active))
        ChartLine(id: 'alr:${a.id}', kind: 'alert', price: a.price, label: t('chart.line.alert'), draggable: true, closable: true),
    ];
    return TerminalChart(
      symbol: symbol,
      tf: tf,
      lines: lines,
      onLineDragged: (l, price) => _commitDrag(ref, l, price),
      onLineTapped: (l) => _tapLine(context, ref, l),
      onLineClosed: (l) => _closeLine(ref, l),
      chartType: chart.type,
      indicators: chart.indicators,
      onIndicatorTapped: (uid) => unawaited(showIndicatorActions(context, ref, symbol, uid)),
      onLongPress: ro ? null : (price) => _longPress(context, ref, price),
      topInset: topInset,
      legendTitle: false,
    );
  }

  static String _pendingLine(String side, String type) => switch ('$side:$type') {
    'buy:limit' => 'chart.line.buyLimit',
    'sell:limit' => 'chart.line.sellLimit',
    'buy:stop' => 'chart.line.buyStop',
    'sell:stop' => 'chart.line.sellStop',
    'buy:stop-limit' => 'chart.line.buyStopLimit',
    'sell:stop-limit' => 'chart.line.sellStopLimit',
    _ => 'chart.line.buyLimit',
  };

  /// A dropped line (web commitLineDrag). The future says whether the server took it (the chart keeps the line at
  /// the drop price until then). A stop made from an S / T handle comes as an `sl:` / `tp:` line the position does
  /// not have yet: it is always sent (even at the open price: a breakeven stop).
  Future<bool> _commitDrag(WidgetRef ref, ChartLine l, double price) async {
    final actions = ref.read(tradeActionsProvider);
    final digits = ref.read(symbolBookProvider)[symbol]?.digits ?? 5;
    final tick = 1 / (digits <= 0 ? 1 : _pow10(digits));
    final pos = ref.read(terminalProvider).positions.where((x) => x.ticket == l.ref).firstOrNull;
    final current = switch (l.kind) {
      'sl' => pos?.sl,
      'tp' => pos?.tp,
      _ => l.price,
    };
    if (current != null && (price - current).abs() < tick) return false;
    KHaptics.medium();
    switch (l.kind) {
      case 'sl':
        return actions.modifyPosition(l.ref, sl: price);
      case 'tp':
        return actions.modifyPosition(l.ref, tp: price);
      case 'pending':
        return actions.modifyOrder(l.ref, price: price);
      case 'alert':
        ref.read(workspaceProvider.notifier).updateAlert(l.ref, price: price);
        return true;
      case 'pos':
        // dragging a position line sets its SL or TP, by the side of the current price
        final p = ref.read(terminalProvider).positions.where((x) => x.ticket == l.ref).firstOrNull;
        final q = ref.read(marketFeedProvider).quote(symbol);
        if (p == null || q == null) return false;
        final cur = p.buy ? q.bid : q.ask;
        final isSl = p.buy ? price < cur : price > cur;
        return isSl ? actions.modifyPosition(l.ref, sl: price) : actions.modifyPosition(l.ref, tp: price);
    }
    return false;
  }

  static double _pow10(int n) {
    var f = 1.0;
    for (var i = 0; i < n; i++) {
      f *= 10;
    }
    return f;
  }

  /// The × on a chip (web removeLine): close the position, remove its SL / TP, cancel the order, delete the alert.
  void _closeLine(WidgetRef ref, ChartLine l) {
    final actions = ref.read(tradeActionsProvider);
    KHaptics.medium();
    switch (l.kind) {
      case 'pos':
        unawaited(actions.closePosition(l.ref));
      case 'sl':
        unawaited(actions.modifyPosition(l.ref, clearSl: true));
      case 'tp':
        unawaited(actions.modifyPosition(l.ref, clearTp: true));
      case 'pending':
        unawaited(actions.cancelOrder(l.ref));
      case 'alert':
        ref.read(workspaceProvider.notifier).removeAlert(l.ref);
    }
  }

  void _tapLine(BuildContext context, WidgetRef ref, ChartLine l) {
    switch (l.kind) {
      case 'pos':
      case 'sl':
      case 'tp':
        if (!ref.read(terminalProvider).readOnly) unawaited(showPositionSheet(context, l.ref));
      case 'pending':
        if (!ref.read(terminalProvider).readOnly) unawaited(showPendingSheet(context, l.ref));
      case 'alert':
        final t = context.t;
        unawaited(
          showKActionSheet<void>(
            context,
            title: '${t('chart.line.alert')} · ${l.price}',
            actions: [
              KAction(
                label: t('chart.line.remove', {'label': t('chart.line.alert')}),
                destructive: true,
                onTap: () {
                  ref.read(workspaceProvider.notifier).removeAlert(l.ref);
                  ref.read(notificationsProvider.notifier).toast(NotificationKind.neutral, t('order.toast.alertDeleted'));
                },
              ),
            ],
          ),
        );
    }
  }

  /// Long press on the plot (web chart context menu): an alert, or a limit / stop order at that price.
  void _longPress(BuildContext context, WidgetRef ref, double price) {
    final t = context.t;
    final q = ref.read(marketFeedProvider).quote(symbol);
    final spec = ref.read(symbolBookProvider)[symbol];
    if (q == null || spec == null) return;
    final lot = ref.read(workspaceProvider).lot;
    final ps = fmtPrice(spec.digits, price);
    final below = price < q.bid;
    final lots = fmtVol(lot);
    Future<void> order(String side, String type) async {
      final ws = ref.read(workspaceProvider);
      if (ws.oneClick) {
        await ref.read(tradeActionsProvider).placeOrder(OrderRequest(symbol: symbol, side: side, type: type, volume: lot, price: price));
      } else if (context.mounted) {
        await showOrderSheet(context, symbol: symbol, side: side, type: type, price: price);
      }
    }

    unawaited(
      showKActionSheet<void>(
        context,
        title: '$symbol · $ps',
        actions: [
          KAction(
            label: t('chart.menu.alertAt', {'price': ps}),
            onTap: () {
              ref.read(workspaceProvider.notifier).addAlert(symbol, price, q.bid);
              ref
                  .read(notificationsProvider.notifier)
                  .toast(
                    NotificationKind.success,
                    t('order.toast.alertCreated'),
                    description: '$symbol ${t('order.toast.alertCond.${price >= q.bid ? 'above' : 'below'}')} $ps',
                  );
            },
          ),
          if (below) ...[
            KAction(label: t('chart.menu.buyLimitAt', {'lot': lots, 'price': ps}), onTap: () => unawaited(order('buy', 'limit'))),
            KAction(label: t('chart.menu.sellStopAt', {'lot': lots, 'price': ps}), onTap: () => unawaited(order('sell', 'stop'))),
          ] else ...[
            KAction(label: t('chart.menu.sellLimitAt', {'lot': lots, 'price': ps}), onTap: () => unawaited(order('sell', 'limit'))),
            KAction(label: t('chart.menu.buyStopAt', {'lot': lots, 'price': ps}), onTap: () => unawaited(order('buy', 'stop'))),
          ],
          KAction(
            label: t('chart.menu.newOrder'),
            onTap: () => unawaited(showOrderSheet(context, symbol: symbol)),
          ),
        ],
      ),
    );
  }
}

/// One-tap trading on the chart, like MT5's one-click panel, as ONE slim pill at the top centre: [S bid] − volume
/// + [ask B] (~36 px). The same order path as the rest of the terminal: one-click trading on (Account tab) sends a
/// market order at once (haptic); off, the tap opens the prefilled order ticket to confirm. A closed market, a delayed
/// quote or an order on its way: the sides grey out (a small note says why). Tap the volume for quick sizes.
class OneClickPill extends ConsumerStatefulWidget {
  const OneClickPill({super.key, required this.symbol});
  final String symbol;

  @override
  ConsumerState<OneClickPill> createState() => _OneClickPillState();
}

class _OneClickPillState extends ConsumerState<OneClickPill> {
  late double _vol = ref.read(workspaceProvider).lot;
  Timer? _clock;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    // the session can open or close while the screen is on (web useMarketOpen: every 15 s)
    _clock = Timer.periodic(const Duration(seconds: 15), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _clock?.cancel();
    super.dispose();
  }

  void _step(SymbolSpec? spec, int dir) {
    final step = spec == null || spec.lotStep <= 0 ? 0.01 : spec.lotStep;
    final next = spec == null ? (_vol + dir * step).clamp(0.01, 100.0).toDouble() : normalizeVolume(spec, _vol + dir * step);
    if (next == _vol) return;
    KHaptics.selection();
    setState(() => _vol = next);
  }

  Future<void> _pickVolume(SymbolSpec? spec) async {
    final t = context.t;
    const presets = [0.01, 0.1, 0.5, 1.0, 2.0];
    final v = await showKActionSheet<double>(
      context,
      title: t('trader.mobile.volume'),
      actions: [
        for (final p in presets)
          if (spec == null || (p >= spec.lotMin - 1e-9 && p <= spec.lotMax + 1e-9)) KAction(label: fmtVol(p), value: p),
      ],
    );
    if (v != null && mounted) setState(() => _vol = v);
  }

  Future<void> _trade(String side) async {
    if (_busy) return;
    setState(() => _busy = true);
    await tradeFrom(context, ref, widget.symbol, side, volume: _vol);
    if (mounted) setState(() => _busy = false);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final spec = ref.watch(symbolBookProvider.select((b) => b[widget.symbol]));
    final oneClick = ref.watch(workspaceProvider.select((w) => w.oneClick));
    final open = isMarketOpen(spec);
    final digits = spec?.digits ?? 5;
    // every tick of the market's quote rebuilds the prices (the stream's real bid / ask, nothing smoothed)
    return QuoteBuilder(
      symbol: widget.symbol,
      builder: (context, q) {
        final delayed = q?.delayed ?? false;
        final enabled = open && !delayed && q != null && q.valid && !_busy;
        Widget side(String s) {
          final buy = s == 'buy';
          final fg = enabled ? Colors.white : k.fg3;
          final letter = Text(
            sideLetter(t, s),
            style: context.text.mono(11, weight: FontWeight.w800, color: fg),
          );
          final price = PriceText(buy ? (q?.ask ?? 0) : (q?.bid ?? 0), digits: digits, size: 12.5, color: fg, weight: FontWeight.w600);
          return KPressable(
            key: ValueKey('chart-trade-$s'),
            minSize: 36,
            pressedScale: 0.96,
            haptic: false,
            semanticLabel: t(buy ? 'common.buy' : 'common.sell'),
            onTap: enabled ? () => unawaited(_trade(s)) : null,
            child: Container(
              height: 36,
              padding: const EdgeInsets.symmetric(horizontal: 12),
              decoration: BoxDecoration(color: enabled ? (buy ? k.buyFill : k.sellFill) : k.surface3, borderRadius: BorderRadius.circular(18)),
              child: Row(mainAxisSize: MainAxisSize.min, children: buy ? [price, const SizedBox(width: 6), letter] : [letter, const SizedBox(width: 6), price]),
            ),
          );
        }

        Widget stepper(IconData icon, int dir, String label) => KPressable(
          key: ValueKey('chart-vol-${dir > 0 ? 'up' : 'down'}'),
          minSize: 36,
          pressedScale: 0.9,
          haptic: false,
          semanticLabel: label,
          onTap: () => _step(spec, dir),
          child: SizedBox(width: 28, height: 36, child: Icon(icon, size: 15, color: k.fg2)),
        );

        final note = !open ? t('trader.mobile.marketClosed') : (delayed ? t('desk.side.delayedTip') : null);
        final step = fmtVol(spec?.lotStep ?? 0.01);
        return Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            Container(
              padding: const EdgeInsets.all(3),
              decoration: BoxDecoration(
                // a solid fill (no blur over the chart's WebView), slightly see-through like MT5's panel
                color: k.surface.withValues(alpha: 0.94),
                borderRadius: BorderRadius.circular(22),
                border: Border.all(color: Colors.white.withValues(alpha: 0.06)),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  side('sell'),
                  stepper(LucideIcons.minus, -1, t('app.trader.ticket.less', {'n': step})),
                  KPressable(
                    key: const ValueKey('chart-vol'),
                    minSize: 36,
                    pressedScale: 0.96,
                    semanticLabel: t('trader.mobile.volume'),
                    onTap: () => unawaited(_pickVolume(spec)),
                    child: Container(
                      constraints: const BoxConstraints(minWidth: 40),
                      alignment: Alignment.center,
                      child: Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          if (oneClick) Icon(LucideIcons.zap, size: 10, color: k.ember),
                          Text(
                            fmtVol(_vol),
                            style: context.text.mono(13, weight: FontWeight.w700, color: k.fg),
                          ),
                        ],
                      ),
                    ),
                  ),
                  stepper(LucideIcons.plus, 1, t('app.trader.ticket.more', {'n': step})),
                  side('buy'),
                ],
              ),
            ),
            if (note != null)
              Container(
                margin: const EdgeInsets.only(top: 6),
                padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
                decoration: BoxDecoration(color: k.surface.withValues(alpha: 0.92), borderRadius: BorderRadius.circular(10)),
                child: Text(note, style: context.text.caption.copyWith(color: k.warn, fontSize: 11)),
              ),
          ],
        );
      },
    );
  }
}

// Chart tab (web MChart): the symbol and timeframes strip, the chart with its trade lines, and the bottom bar
// SELL (bid) · volume · BUY (ask). One-click trading on: Sell / Buy trade at once (haptic); off: the order sheet.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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
import 'symbol_search.dart';

class ChartTab extends ConsumerWidget {
  const ChartTab({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final ws = ref.watch(workspaceProvider);
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final symbol = ws.symbol;
    return Column(
      children: [
        Container(
          height: 38,
          decoration: BoxDecoration(
            color: k.surface,
            border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
          ),
          child: Row(
            children: [
              Expanded(
                child: ListView(
                  scrollDirection: Axis.horizontal,
                  padding: const EdgeInsetsDirectional.only(start: 6, end: 2),
                  children: [
                    KPressable(
                      minSize: 36,
                      onTap: () => showSymbolSearch(context),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(horizontal: 6),
                        child: Row(
                          children: [
                            SymbolAvatar(symbol, size: 15),
                            const SizedBox(width: 6),
                            Text(symbol, style: context.text.label.copyWith(fontWeight: FontWeight.w600, fontSize: 13)),
                          ],
                        ),
                      ),
                    ),
                    Center(
                      child: Container(width: 0.8, height: 16, color: k.line, margin: const EdgeInsets.symmetric(horizontal: 4)),
                    ),
                    for (final tf in kTimeframes)
                      Center(
                        child: KPressable(
                          minSize: 34,
                          pressedScale: 1,
                          onTap: () {
                            KHaptics.selection();
                            ref.read(workspaceProvider.notifier).update((w) => w.copyWith(tf: tf));
                          },
                          child: Container(
                            height: 27,
                            padding: const EdgeInsets.symmetric(horizontal: 8),
                            alignment: Alignment.center,
                            decoration: BoxDecoration(color: ws.tf == tf ? k.emberSoft : Colors.transparent, borderRadius: BorderRadius.circular(6)),
                            child: Text(
                              tf,
                              style: context.text.mono(11, weight: FontWeight.w600, color: ws.tf == tf ? k.ember : k.fg3),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
              Container(width: 0.8, height: 16, color: k.line),
              // chart type, indicators and templates (web chart toolbar)
              ChartMenuButton(symbol: symbol),
              const SizedBox(width: 2),
            ],
          ),
        ),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.all(4),
            child: ClipRRect(
              borderRadius: BorderRadius.circular(9),
              child: DecoratedBox(
                position: DecorationPosition.foreground,
                decoration: BoxDecoration(
                  border: Border.all(color: k.line),
                  borderRadius: BorderRadius.circular(9),
                ),
                child: _Chart(symbol: symbol, tf: ws.tf),
              ),
            ),
          ),
        ),
        if (!readOnly) _TradeBar(symbol: symbol),
      ],
    );
  }
}

/// The chart with the account's trade lines (web useTradeLines + commitLineDrag).
class _Chart extends ConsumerWidget {
  const _Chart({required this.symbol, required this.tf});
  final String symbol, tf;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final st = ref.watch(terminalProvider);
    final alerts = ref.watch(workspaceProvider.select((w) => w.alerts));
    final chart = ref.watch(workspaceProvider.select((w) => w.chartOf(symbol)));
    final ro = st.readOnly;
    final cent = st.account?.cent ?? false;
    final spec = ref.watch(symbolBookProvider.select((b) => b[symbol]));
    final bidOf = ref.read(marketFeedProvider).bidOf;
    // the money on a chip (web chart-view: the position's P&L, the result at its SL / TP)
    String money(double usd) => accMoney(cent, usd, signed: true);
    String? atStop(TPosition p, double price) =>
        spec == null ? null : money(profitAt(spec, side: p.side, lots: p.volume, open: p.openPrice, close: price, bidOf: bidOf) + p.swap - p.commission);
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
          label: t(p.buy ? 'chart.line.buy' : 'chart.line.sell', {'lot': fmtVol(p.volume)}),
          draggable: !ro,
          note: money(st.profitOf(p)),
          tone: tone(st.profitOf(p)),
          closable: !ro,
          // S / T: drag out a stop loss / take profit the position does not have yet (web trade-handles.ts)
          handles: ro || spec == null ? '' : '${p.sl == null ? 's' : ''}${p.tp == null ? 't' : ''}',
          pip: spec?.pipSize ?? 0,
          gap: gap,
        ),
        if (p.sl != null)
          ChartLine(id: 'sl:${p.ticket}', kind: 'sl', price: p.sl!, side: p.side, label: 'SL', draggable: !ro, note: atStop(p, p.sl!), closable: !ro),
        if (p.tp != null)
          ChartLine(id: 'tp:${p.ticket}', kind: 'tp', price: p.tp!, side: p.side, label: 'TP', draggable: !ro, note: atStop(p, p.tp!), closable: !ro),
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

/// SELL (bid) · volume stepper · BUY (ask) (web MChart bottom bar).
class _TradeBar extends ConsumerStatefulWidget {
  const _TradeBar({required this.symbol});
  final String symbol;

  @override
  ConsumerState<_TradeBar> createState() => _TradeBarState();
}

class _TradeBarState extends ConsumerState<_TradeBar> {
  late String _vol = ref.read(workspaceProvider).lot.toStringAsFixed(2);
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

  Future<void> _trade(String side, bool open, bool delayed) async {
    if (!open || delayed || _busy) return;
    final v = double.tryParse(_vol) ?? 0.01;
    final vol = v < 0.01 ? 0.01 : v;
    final ws = ref.read(workspaceProvider);
    if (ws.oneClick) {
      KHaptics.medium();
      setState(() => _busy = true);
      final ok = await ref.read(tradeActionsProvider).quickTrade(widget.symbol, side, vol);
      if (ok) ref.read(workspaceProvider.notifier).update((w) => w.copyWith(lot: vol));
      if (mounted) setState(() => _busy = false);
    } else {
      await showOrderSheet(context, symbol: widget.symbol, side: side, volume: vol);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final spec = ref.watch(symbolBookProvider.select((b) => b[widget.symbol]));
    final oneClick = ref.watch(workspaceProvider.select((w) => w.oneClick));
    final open = isMarketOpen(spec);
    final digits = spec?.digits ?? 5;
    return Container(
      padding: const EdgeInsets.fromLTRB(8, 7, 8, 7),
      decoration: BoxDecoration(
        color: k.surface,
        border: Border(top: BorderSide(color: k.line, width: 0.6)),
      ),
      child: QuoteBuilder(
        symbol: widget.symbol,
        builder: (context, q) {
          final delayed = q?.delayed ?? false;
          final enabled = open && !delayed && q != null && q.valid && !_busy;
          Widget side(String s) {
            final buy = s == 'buy';
            final bg = enabled ? (buy ? k.buyFill : k.sellFill) : k.surface3;
            final fg = enabled ? Colors.white : k.fg3;
            return Expanded(
              child: KPressable(
                pressedScale: 0.97,
                haptic: false,
                semanticLabel: t(buy ? 'common.buy' : 'common.sell'),
                onTap: enabled ? () => unawaited(_trade(s, open, delayed)) : null,
                child: Container(
                  height: 48,
                  width: double.infinity,
                  padding: const EdgeInsets.symmetric(horizontal: 10),
                  decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(10)),
                  child: Column(
                    crossAxisAlignment: buy ? CrossAxisAlignment.end : CrossAxisAlignment.start,
                    mainAxisAlignment: MainAxisAlignment.center,
                    children: [
                      Text(
                        t(buy ? 'common.buy' : 'common.sell').toUpperCase(),
                        style: context.text.micro.copyWith(color: fg.withValues(alpha: 0.85), fontSize: 9.5, letterSpacing: 1),
                      ),
                      const SizedBox(height: 1),
                      PriceText(buy ? (q?.ask ?? 0) : (q?.bid ?? 0), digits: digits, size: 14, color: fg),
                    ],
                  ),
                ),
              ),
            );
          }

          final caption = !open
              ? t('trader.mobile.marketClosed')
              : (delayed ? t('desk.side.delayedTip') : (oneClick ? t('trader.mobile.oneClick') : t('trader.mobile.confirm')));
          return Row(
            children: [
              side('sell'),
              const SizedBox(width: 6),
              SizedBox(
                width: 112,
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    TStepper(
                      value: _vol,
                      onChanged: (v) => _vol = v,
                      min: spec?.lotMin ?? 0.01,
                      max: spec?.lotMax ?? 100,
                      step: spec?.lotStep ?? 0.01,
                      semanticLabel: t('trader.mobile.volume'),
                    ),
                    const SizedBox(height: 3),
                    Row(
                      mainAxisAlignment: MainAxisAlignment.center,
                      children: [
                        if (delayed)
                          Padding(
                            padding: const EdgeInsets.only(right: 3),
                            child: Icon(Icons.schedule_rounded, size: 10, color: k.warn),
                          ),
                        if (oneClick && open && !delayed)
                          Padding(
                            padding: const EdgeInsets.only(right: 2),
                            child: Icon(Icons.bolt_rounded, size: 11, color: k.ember),
                          ),
                        Flexible(
                          child: Text(
                            caption,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.mono(9.5, color: !open || delayed ? k.warn : k.fg3),
                          ),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 6),
              side('buy'),
            ],
          );
        },
      ),
    );
  }
}

// Trade tab (web MTrade, owner design 2026-10): the equity card (equity, floating P&L, then balance · margin · free
// margin · margin level), the open positions as cards (the side + volume badge "B 0.50", the symbol, open → current
// price, tiny S / T chips only when set, the P&L; tap: the position sheet; swipe: close) with a ⋯ menu to close all /
// the profitable / the losing ones (trade/bulk-close, behind a confirm), and the pending orders as cards with Cancel
// (tap: modify). The depth-of-market ladder lives on the symbol screen now (Watchlist › a market › Depth).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/models.dart';
import '../core/terminal_controller.dart';
import '../core/trade_actions.dart';
import '../core/trade_math.dart';
import '../widgets/kit.dart';
import 'position_sheet.dart';
import 'room.dart';

class TradeTab extends ConsumerStatefulWidget {
  const TradeTab({super.key});

  @override
  ConsumerState<TradeTab> createState() => _TradeTabState();
}

class _TradeTabState extends ConsumerState<TradeTab> {
  /// The ⋯ menu: close all / profitable / losing (web toolbox bulk close), each behind a confirm naming what closes.
  Future<void> _closeMenu() async {
    final t = context.t;
    final filter = await showKActionSheet<String>(
      context,
      title: t('desk.act.closeMenu'),
      actions: [
        KAction(label: t('toolbox.bulk.closeAll'), value: 'all', destructive: true),
        KAction(label: t('toolbox.bulk.closeProfitable'), value: 'profitable'),
        KAction(label: t('toolbox.bulk.closeLosing'), value: 'losing'),
      ],
    );
    if (filter == null || !mounted) return;
    final st = ref.read(terminalProvider);
    final cent = st.account?.cent ?? false;
    final hit = st.positions.where((p) => bulkCloses(filter, st.profitOf(p))).toList();
    final pnl = hit.fold<double>(0, (s, p) => s + st.profitOf(p));
    final ok = await showKAlert<bool>(
      context,
      title: t(switch (filter) {
        'profitable' => 'toolbox.bulk.closeProfitable',
        'losing' => 'toolbox.bulk.closeLosing',
        _ => 'toolbox.bulk.closeAll',
      }),
      message: '${t('trader.mobile.positions', {'count': hit.length})} · ${accMoney(cent, pnl, signed: true)} ${accCcy(cent)}',
      actions: [
        KAction(label: t('common.cancel'), value: false),
        KAction(label: t('desk.pos.close'), value: true, destructive: true, primary: true),
      ],
    );
    if (ok == true) unawaited(ref.read(tradeActionsProvider).bulkClose(filter));
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final st = ref.watch(terminalProvider);
    final a = st.account;
    if (a == null) return const SizedBox.shrink();
    final m = st.metrics;
    final ms = marginState(m.level, a);
    final fl = m.floating;
    final section = context.text.headline.copyWith(fontSize: 15, fontWeight: FontWeight.w800, color: k.fg);
    Widget cell(String label, String value, {Color? color}) => Row(
      children: [
        Expanded(
          child: Text(label, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.footnote.copyWith(fontSize: 12, color: k.fg3)),
        ),
        Text(value, textDirection: TextDirection.ltr, style: context.text.mono(12, weight: FontWeight.w600, color: color ?? k.fg)),
      ],
    );
    return RefreshIndicator.adaptive(
      onRefresh: () => ref.read(terminalProvider.notifier).reload(),
      child: ListView(
        padding: const EdgeInsets.only(top: 10, bottom: 24),
        children: [
          // equity card
          RoomCard(
            key: const ValueKey('trade-equity'),
            padding: const EdgeInsets.all(16),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(t('common.equity'), style: context.text.footnote.copyWith(fontSize: 12, color: k.fg3)),
                const SizedBox(height: 2),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.baseline,
                  textBaseline: TextBaseline.alphabetic,
                  children: [
                    Expanded(
                      child: FittedBox(
                        fit: BoxFit.scaleDown,
                        alignment: AlignmentDirectional.centerStart,
                        child: Text(
                          accMoney(a.cent, m.equity),
                          textDirection: TextDirection.ltr,
                          style: context.text.mono(30, weight: FontWeight.w800, color: k.fg).copyWith(letterSpacing: -0.8),
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      accMoney(a.cent, fl, signed: true),
                      key: const ValueKey('trade-floating'),
                      textDirection: TextDirection.ltr,
                      style: context.text.mono(15, weight: FontWeight.w700, color: fl > 0.004 ? k.up : (fl < -0.004 ? k.down : k.fg2)),
                    ),
                  ],
                ),
                const SizedBox(height: 12),
                Row(
                  children: [
                    Expanded(child: cell(t('common.balance'), accMoney(a.cent, m.balance))),
                    const SizedBox(width: 16),
                    Expanded(child: cell(t('trader.mobile.margin'), accMoney(a.cent, m.margin))),
                  ],
                ),
                const SizedBox(height: 8),
                Row(
                  children: [
                    Expanded(child: cell(t('trader.mobile.freeMargin'), accMoney(a.cent, m.free), color: m.free < 0 ? k.down : null)),
                    const SizedBox(width: 16),
                    Expanded(
                      child: cell(
                        t('trader.status.marginLevel'),
                        m.level.isFinite ? '${m.level.toStringAsFixed(0)}%' : '—',
                        color: ms == 'low' ? k.warn : (ms == 'call' || ms == 'stopout' ? k.down : null),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(kRoomGutter, 10, 8, 4),
            child: Row(
              children: [
                Expanded(child: Text(t('trader.mobile.positions', {'count': st.positions.length}), style: section)),
                if (!st.readOnly && st.positions.isNotEmpty)
                  RoomIconButton(key: const ValueKey('trade-menu'), icon: LucideIcons.ellipsis, label: t('desk.act.closeMenu'), onTap: _closeMenu),
              ],
            ),
          ),
          for (final p in st.positions) PositionCard(position: p, key: ValueKey('p${p.ticket}')),
          if (st.positions.isEmpty) TEmptyLine(t('trader.mobile.noPositions')),
          Padding(
            padding: const EdgeInsets.fromLTRB(kRoomGutter, 10, kRoomGutter, 8),
            child: Text(t('trader.mobile.pendingOrders', {'count': st.orders.length}), style: section),
          ),
          for (final o in st.orders) PendingCard(order: o, key: ValueKey('o${o.ticket}')),
        ],
      ),
    );
  }
}

/// Whether a bulk close with `filter` (all | profitable | losing) closes a position with this P&L (the engine's
/// trade/bulk-close rule: profitable > 0, losing < 0).
bool bulkCloses(String filter, double profit) => switch (filter) {
  'profitable' => profit > 0,
  'losing' => profit < 0,
  _ => true,
};

/// One open position as a card: "B 0.50", the symbol, open → current, the S / T chips when set, the P&L. Tap: the
/// position sheet; swipe: close.
class PositionCard extends ConsumerWidget {
  const PositionCard({super.key, required this.position});
  final TPosition position;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final p = position;
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final cent = ref.watch(terminalProvider.select((s) => s.account?.cent ?? false));
    final profit = ref.watch(terminalProvider.select((s) => s.profitOf(p)));
    final digits = ref.watch(symbolBookProvider.select((b) => b[p.symbol]?.digits ?? 5));
    final card = Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
      decoration: BoxDecoration(color: k.surface, borderRadius: BorderRadius.circular(kRoomRadius)),
      child: Row(
        children: [
          SideBadge(side: p.side, lots: p.volume),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(p.symbol, style: context.text.label.copyWith(fontSize: 15, fontWeight: FontWeight.w700, color: k.fg)),
                const SizedBox(height: 3),
                QuoteBuilder(
                  symbol: p.symbol,
                  throttle: const Duration(milliseconds: 500),
                  builder: (context, q) => Text(
                    '${fmtPrice(digits, p.openPrice)} → ${q == null ? '—' : fmtPrice(digits, p.buy ? q.bid : q.ask)}',
                    textDirection: TextDirection.ltr,
                    style: context.text.mono(11, color: k.fg3),
                  ),
                ),
                if (p.sl != null || p.tp != null) ...[
                  const SizedBox(height: 4),
                  Wrap(
                    spacing: 4,
                    runSpacing: 4,
                    children: [
                      if (p.sl != null) StopChip(kind: 'sl', digits: digits, price: p.sl!),
                      if (p.tp != null) StopChip(kind: 'tp', digits: digits, price: p.tp!),
                    ],
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(width: 8),
          PnlText(profit, cent: cent, size: 16, weight: FontWeight.w800),
        ],
      ),
    );
    final tappable = KPressable(
      pressedScale: 0.99,
      pressedOpacity: 0.8,
      semanticLabel: '${p.symbol} ${sideTag(t, p.side, p.volume)} ${accMoney(cent, profit, signed: true)}',
      onTap: readOnly ? null : () => unawaited(showPositionSheet(context, p.ticket)),
      child: card,
    );
    final body = Padding(padding: const EdgeInsets.fromLTRB(kRoomGutter, 0, kRoomGutter, 8), child: tappable);
    if (readOnly) return body;
    return KSwipeable(
      background: k.bg,
      actions: [
        KSwipeAction(
          label: t('desk.pos.close'),
          icon: LucideIcons.x,
          destructive: true,
          onTap: () => unawaited(ref.read(tradeActionsProvider).closePosition(p.ticket)),
        ),
      ],
      child: body,
    );
  }
}

/// One pending order as a card: the side badge, the symbol and its type, "at price · now current", Cancel. Tap: modify.
class PendingCard extends ConsumerWidget {
  const PendingCard({super.key, required this.order});
  final TOrder order;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final o = order;
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final digits = ref.watch(symbolBookProvider.select((b) => b[o.symbol]?.digits ?? 5));
    final label = t(o.labelKey);
    return Padding(
      padding: const EdgeInsets.fromLTRB(kRoomGutter, 0, kRoomGutter, 8),
      child: KPressable(
        pressedScale: 0.99,
        pressedOpacity: 0.8,
        onTap: readOnly ? null : () => unawaited(showPendingSheet(context, o.ticket)),
        child: Container(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          decoration: BoxDecoration(color: k.surface, borderRadius: BorderRadius.circular(kRoomRadius)),
          child: Row(
            children: [
              SideBadge(side: o.side, lots: o.volume, filled: false),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(
                            text: '${o.symbol} ',
                            style: context.text.label.copyWith(fontSize: 15, fontWeight: FontWeight.w700, color: k.fg),
                          ),
                          TextSpan(
                            text: label,
                            style: context.text.label.copyWith(fontSize: 12, fontWeight: FontWeight.w600, color: k.fg3),
                          ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                    const SizedBox(height: 3),
                    QuoteBuilder(
                      symbol: o.symbol,
                      throttle: const Duration(milliseconds: 500),
                      builder: (context, q) => Text(
                        t('app.trader.trade.atNow', {
                          'price': fmtPrice(digits, o.price),
                          'current': q == null ? '—' : fmtPrice(digits, o.buy ? q.ask : q.bid),
                        }),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.mono(11, color: k.fg3),
                      ),
                    ),
                  ],
                ),
              ),
              if (!readOnly)
                KPressable(
                  key: ValueKey('cancel-${o.ticket}'),
                  semanticLabel: t('trader.mobile.cancelOrder'),
                  onTap: () => unawaited(ref.read(tradeActionsProvider).cancelOrder(o.ticket)),
                  child: Container(
                    height: 34,
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    alignment: Alignment.center,
                    decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(17)),
                    child: Text(t('common.cancel'), style: context.text.label.copyWith(fontSize: 12, fontWeight: FontWeight.w700, color: k.down)),
                  ),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

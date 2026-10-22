// History tab (web MHistory + the toolbox History's periods and totals, components/toolbox/tabs.tsx; owner design
// 2026-10): the period chips (Today · Last 3 days · Last week · Last month · Last 3 months · All history; Last month by
// default), the totals card of the period (profit, trades, win rate, profit factor, gross profit / loss) and the
// account's closed trades as cards with their B / S badge, newest first.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/history.dart';
import '../core/market.dart';
import '../core/models.dart';
import '../core/terminal_controller.dart';
import '../core/trade_math.dart';
import '../widgets/kit.dart';
import 'room.dart';

/// Rows drawn at most (web: the first 300 of the period).
const _maxRows = 300;

class HistoryTab extends ConsumerWidget {
  const HistoryTab({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final period = ref.watch(historyPeriodProvider);
    final live = ref.watch(terminalProvider.select((s) => s.history));
    final synced = ref.watch(terminalProvider.select((s) => s.synced));
    final cent = ref.watch(terminalProvider.select((s) => s.account?.cent ?? false));
    final range = ref.watch(historyRangeProvider(period));
    final since = historySince(period, DateTime.now());
    final loaded = range.value == null ? const <TClosed>[] : mapHistory(range.value!.where((d) => !isOptionEntry(d)).toList(), cent: cent);
    final rows = mergeHistory(loaded, live, since);
    final stats = HistoryStats.of(rows);
    final loading = !synced || (range.isLoading && !range.hasValue && rows.isEmpty);

    Future<void> refresh() async {
      ref.invalidate(historyRangeProvider(period));
      await ref.read(terminalProvider.notifier).reload();
    }

    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 8),
          child: PeriodChips(value: period, onChanged: (p) => ref.read(historyPeriodProvider.notifier).set(p)),
        ),
        const SizedBox(height: 6),
        _Totals(stats: stats, cent: cent),
        const SizedBox(height: 12),
        Expanded(
          child: loading
              ? ListView(
                  children: [for (var i = 0; i < 6; i++) const Padding(padding: EdgeInsets.all(14), child: KSkeleton())],
                )
              : rows.isEmpty
              ? RefreshIndicator.adaptive(
                  onRefresh: refresh,
                  child: LayoutBuilder(
                    builder: (context, c) => SingleChildScrollView(
                      physics: const AlwaysScrollableScrollPhysics(),
                      child: SizedBox(
                        height: c.maxHeight,
                        child: Center(
                          child: KEmptyState(icon: LucideIcons.history, title: t('trader.mobile.tab.history'), text: t('toolbox.history.empty'), compact: true),
                        ),
                      ),
                    ),
                  ),
                )
              : RefreshIndicator.adaptive(
                  onRefresh: refresh,
                  child: ListView.builder(
                    padding: const EdgeInsets.only(bottom: 20),
                    itemCount: rows.length.clamp(0, _maxRows),
                    itemBuilder: (context, i) => _Row(h: rows[i], cent: cent),
                  ),
                ),
        ),
      ],
    );
  }
}

/// The web's period radio group as a row of compact chips (scrolls sideways when the labels are long).
class PeriodChips extends StatelessWidget {
  const PeriodChips({super.key, required this.value, required this.onChanged});
  final String value;
  final ValueChanged<String> onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return SizedBox(
      height: 44,
      child: ListView(
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: kRoomGutter),
        children: [
          for (final (p, _) in kHistoryPeriods)
            Padding(
              padding: const EdgeInsetsDirectional.only(end: 6),
              child: RoomChip(
                key: ValueKey('history-period-$p'),
                label: t('toolbox.history.period.$p'),
                selected: p == value,
                onTap: () {
                  if (p == value) return;
                  KHaptics.selection();
                  onChanged(p);
                },
              ),
            ),
        ],
      ),
    );
  }
}

/// The period's totals as a card (web toolbar totals): profit · trades · win rate · PF · gross profit / loss.
class _Totals extends StatelessWidget {
  const _Totals({required this.stats, required this.cent});
  final HistoryStats stats;
  final bool cent;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    Widget cell(String name, Widget value) => Row(
      children: [
        Expanded(
          child: Text(name, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.footnote.copyWith(fontSize: 12, color: k.fg3)),
        ),
        DefaultTextStyle.merge(style: context.text.mono(12, weight: FontWeight.w700, color: k.fg), child: value),
      ],
    );
    Widget pair(Widget a, Widget b) => Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Row(
        children: [
          Expanded(child: a),
          const SizedBox(width: 16),
          Expanded(child: b),
        ],
      ),
    );
    final net = stats.net;
    return RoomCard(
      key: const ValueKey('history-totals'),
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 10),
      child: Column(
        children: [
          pair(
            cell(
              t('toolbox.history.profit'),
              Text(accMoney(cent, net, signed: true), textDirection: TextDirection.ltr, style: TextStyle(color: net >= 0 ? k.up : k.down)),
            ),
            cell(t('toolbox.history.trades'), Text('${stats.trades}')),
          ),
          pair(
            cell(t('toolbox.history.winRate'), Text('${stats.winRate.toStringAsFixed(1)}%', textDirection: TextDirection.ltr)),
            cell(t('toolbox.history.pf'), Text(stats.pfText)),
          ),
          pair(
            cell(
              t('toolbox.history.gross'),
              Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: accMoney(cent, stats.grossProfit),
                      style: TextStyle(color: k.up),
                    ),
                    TextSpan(
                      text: ' / ',
                      style: TextStyle(color: k.fg3),
                    ),
                    TextSpan(
                      text: accMoney(cent, stats.grossLoss),
                      style: TextStyle(color: k.down),
                    ),
                  ],
                ),
                textDirection: TextDirection.ltr,
              ),
            ),
            const SizedBox.shrink(),
          ),
        ],
      ),
    );
  }
}

/// One closed trade as a card: the B / S badge, the symbol, open → close, the result and when it closed.
class _Row extends ConsumerWidget {
  const _Row({required this.h, required this.cent});
  final TClosed h;
  final bool cent;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final digits = ref.watch(symbolBookProvider.select((b) => b[h.symbol]?.digits ?? 5));
    return Padding(
      padding: const EdgeInsets.fromLTRB(kRoomGutter, 0, kRoomGutter, 8),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
        decoration: BoxDecoration(color: k.surface, borderRadius: BorderRadius.circular(kRoomRadius)),
        child: Row(
          children: [
            SideBadge(side: h.side == 'buy' ? 'buy' : 'sell', lots: h.volume, filled: false),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    h.symbol,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.label.copyWith(fontSize: 15, fontWeight: FontWeight.w700, color: k.fg),
                  ),
                  const SizedBox(height: 3),
                  Text(
                    '${fmtPrice(digits, h.openPrice)} → ${fmtPrice(digits, h.closePrice)}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    textDirection: TextDirection.ltr,
                    style: context.text.mono(11, color: k.fg3),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 8),
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                PnlText(h.profit, cent: cent, size: 15, weight: FontWeight.w800),
                const SizedBox(height: 3),
                Text(
                  fmtServer(h.closeTime, seconds: false),
                  textDirection: TextDirection.ltr,
                  style: context.text.mono(10.5, color: k.fg3),
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

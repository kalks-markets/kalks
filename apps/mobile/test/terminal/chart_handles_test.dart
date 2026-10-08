// The S / T handles on a chart position line (web components/chart/trade-handles.ts; the chart page draws the same):
// a position without a stop loss / take profit shows "S" / "T" after its P&L; a tap places that stop at the order
// tickets' starting distance (max(10 pips, 2 x spread), TP twice that) from the close price, a drag places it where
// the finger lifts, held on the side of the price the trade server accepts; the new line shows at once and the handle
// goes while it exists. Native chart surface (no WebView in tests): the same commands and events as the chart page.
// Also: Kalks Trader's blue = up / buy / profit (the trader tokens and the chart palette), the Client Area keeps green.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/cfd/chart_tab.dart';
import 'package:kalks/features/terminal/chart/chart_bridge.dart';
import 'package:kalks/features/terminal/chart/chart_surface.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/sessions.dart';
import 'package:kalks/features/terminal/core/terminal_controller.dart';
import 'package:kalks/features/terminal/core/trade_actions.dart';
import 'package:kalks/features/terminal/core/workspace.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

/// Records the stops the chart asks for (the position itself does not change).
class RecordingActions extends TradeActions {
  RecordingActions(super.ref);
  final List<Map<String, Object?>> modifies = [];

  @override
  Future<bool> modifyPosition(
    String ticket, {
    double? sl,
    double? tp,
    bool clearSl = false,
    bool clearTp = false,
    int? trailingPoints,
    bool setTrailing = false,
  }) async {
    modifies.add({'ticket': ticket, 'sl': sl, 'tp': tp});
    return true;
  }
}

/// M15 bars swinging between `lo` and `hi` (oldest first, UTC seconds).
List<Candle> swing(double lo, double hi, {int n = 120}) {
  final t0 = DateTime.utc(2026, 10, 9, 8).millisecondsSinceEpoch ~/ 1000 - n * 900;
  final mid = (lo + hi) / 2, amp = (hi - lo) / 2;
  return [
    for (var i = 0; i < n; i++)
      () {
        final o = mid + amp * 0.8 * ((i % 20) - 10) / 10;
        final c = mid + amp * 0.8 * (((i + 1) % 20) - 10) / 10;
        return Candle(t0 + i * 900, o, (o > c ? o : c) + amp * 0.1, (o < c ? o : c) - amp * 0.1, c, 100);
      }(),
  ];
}

/// A market whose history and quote the test sets.
class _Source extends ChartSource {
  _Source(this.candles, this.bid, this.ask);
  final List<Candle> candles;
  final double bid, ask;

  @override
  String get key => 'EURUSD';
  @override
  int get digits => 5;

  @override
  Future<List<Candle>> history(String tf, {int? to}) async => to == null ? candles : const [];

  @override
  void Function()? quotes(void Function(double bid, double ask) onQuote) {
    onQuote(bid, ask);
    return () {};
  }
}

/// Fixed quotes and the given history for every market.
class _BarsFeed extends FixedFeed {
  _BarsFeed(super.quotes, this.bars);
  final List<Candle> bars;

  @override
  Future<List<Candle>> candles(String symbol, String tf, {int limit = 1000, int? to}) async => to == null ? bars : const [];
}

Finder handle(String id) => find.byKey(ValueKey('chart-handle-$id'));

void main() {
  test('Kalks Trader is blue for up / buy / profit, the Client Area keeps green', () {
    final dark = chartPalette(KTokens.traderDark()), light = chartPalette(KTokens.traderLight());
    expect(dark.up, '#2f7bff');
    expect(light.up, '#1f5fe0');
    expect(dark.down, '#f04438');
    expect(KTokens.traderDark().buyFill, const Color(0xFF1F5FE0));
    expect(KTokens.traderLight().buyFill, const Color(0xFF1F5FE0));
    expect(KTokens.traderDark().sellFill, const Color(0xFFDC2626));
    expect(KTokens.clientDark().up, const Color(0xFF22C55E));
    expect(KTokens.clientLight().buyFill, const Color(0xFF15803D));
  });

  group('TerminalChart', () {
    const pos = ChartLine(
      id: 'pos:7',
      kind: 'pos',
      price: 1.0840,
      label: 'BUY 0.10',
      side: 'buy',
      draggable: true,
      note: '+10.00',
      tone: 'up',
      closable: true,
      handles: 'st',
      pip: 0.0001,
      gap: 0.0001,
    );
    late ValueNotifier<List<ChartLine>> lines;
    late List<(ChartLine, double)> drops;
    Completer<bool>? answer;

    /// The chart's drop handler: recorded; the answer is the test's to give.
    Future<bool>? drop(ChartLine l, double price) {
      drops.add((l, price));
      return answer?.future;
    }

    Future<void> show(WidgetTester tester) async {
      await loadFonts();
      TerminalChart.forceNative = true;
      tester.view.physicalSize = const Size(412, 600);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      lines = ValueNotifier([pos]);
      drops = [];
      answer = null;
      final en = catalog('en');
      final src = _Source(swing(1.0800, 1.0900), 1.0850, 1.0851);
      await tester.pumpWidget(
        ProviderScope(
          child: MaterialApp(
            theme: KTheme.trader(Brightness.dark),
            builder: (context, child) => I18nScope(t: T('en', en, en), child: child!),
            home: Scaffold(
              body: ValueListenableBuilder<List<ChartLine>>(
                valueListenable: lines,
                builder: (context, ls, _) => TerminalChart(
                  symbol: 'EURUSD',
                  tf: 'M15',
                  source: src,
                  lines: ls,
                  onLineDragged: drop,
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
    }

    Future<void> done(WidgetTester tester) async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump(const Duration(milliseconds: 100));
    }

    testWidgets('a tap on S sets the stop loss 10 pips under the bid; the new line shows at once and S goes', (tester) async {
      await show(tester);
      expect(handle('sl:7'), findsOneWidget);
      expect(handle('tp:7'), findsOneWidget);
      expect(find.text('S'), findsOneWidget);
      expect(find.text('T'), findsOneWidget);

      answer = Completer<bool>();
      await tester.tap(handle('sl:7'));
      await tester.pump(const Duration(milliseconds: 100));
      expect(drops, hasLength(1));
      final (line, price) = drops.single;
      expect(line.id, 'sl:7');
      expect(line.kind, 'sl');
      expect(line.side, 'buy');
      expect(price, 1.084);
      // held at the drop price while the server answers: the SL chip is drawn, S is gone, T stays
      expect(find.text('SL'), findsOneWidget);
      expect(handle('sl:7'), findsNothing);
      expect(handle('tp:7'), findsOneWidget);

      // the position now carries the stop: the given line replaces the held one
      lines.value = [
        const ChartLine(id: 'pos:7', kind: 'pos', price: 1.0840, label: 'BUY 0.10', side: 'buy', draggable: true, handles: 't', pip: 0.0001, gap: 0.0001),
        const ChartLine(id: 'sl:7', kind: 'sl', price: 1.084, label: 'SL', side: 'buy', draggable: true, note: '0.00', closable: true),
      ];
      answer!.complete(true);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(seconds: 2));
      expect(find.text('SL'), findsOneWidget);
      expect(handle('sl:7'), findsNothing);
      expect(handle('tp:7'), findsOneWidget);
      await done(tester);
    });

    testWidgets('a refused stop goes away and its handle comes back', (tester) async {
      await show(tester);
      answer = Completer<bool>();
      await tester.tap(handle('tp:7'));
      await tester.pump(const Duration(milliseconds: 100));
      expect(drops.single.$1.id, 'tp:7');
      // twice the stop loss distance: 20 pips over the bid
      expect(drops.single.$2, 1.087);
      expect(find.text('TP'), findsOneWidget);
      answer!.complete(false);
      await tester.pump(const Duration(milliseconds: 100));
      await tester.pump(const Duration(milliseconds: 100));
      expect(find.text('TP'), findsNothing);
      expect(handle('tp:7'), findsOneWidget);
      await done(tester);
    });

    testWidgets('dragging T places the take profit where the finger lifts; a stop dragged across the price is held back', (tester) async {
      await show(tester);
      answer = Completer<bool>();
      await tester.drag(handle('tp:7'), const Offset(0, -90));
      await tester.pump(const Duration(milliseconds: 100));
      expect(drops.single.$1.id, 'tp:7');
      expect(drops.single.$2, greaterThan(1.0851));
      expect(find.text('TP'), findsOneWidget);
      answer!.complete(true);
      await tester.pump(const Duration(seconds: 2));

      // S pulled far above the bid: a buy's stop loss stays the stops level under it
      answer = Completer<bool>();
      await tester.drag(handle('sl:7'), const Offset(0, -260));
      await tester.pump(const Duration(milliseconds: 100));
      expect(drops.last.$1.id, 'sl:7');
      expect(drops.last.$2, 1.0849);
      answer!.complete(true);
      await tester.pump(const Duration(seconds: 2));
      await done(tester);
    });

    testWidgets('a read-only position line has no handles', (tester) async {
      await show(tester);
      lines.value = [const ChartLine(id: 'pos:7', kind: 'pos', price: 1.0840, label: 'BUY 0.10', side: 'buy', note: '+10.00')];
      // the chart rebuilds with the new lines, then sends them with the next live frame
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 100));
      expect(handle('sl:7'), findsNothing);
      expect(handle('tp:7'), findsNothing);
      await done(tester);
    });
  });

  group('Chart tab on the preview trade server', () {
    late Harness h;
    late RecordingActions actions;

    Future<void> boot(WidgetTester tester) async {
      await loadFonts();
      TerminalChart.forceNative = true;
      tester.view.physicalSize = const Size(412, 915);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.runAsync(() async {
        h = await Harness.create(
          extra: [
            marketFeedProvider.overrideWith(
              (ref) => _BarsFeed({'EURUSD': q('EURUSD', 1.0845, 1.0846), 'BTCUSD': q('BTCUSD', 63400, 63418), 'XAUUSD': q('XAUUSD', 2654.30, 2654.48)}, swing(1.0780, 1.0900)),
            ),
            tradeActionsProvider.overrideWith((ref) => actions = RecordingActions(ref)),
          ],
        );
        h.container.listen(terminalProvider, (_, _) {});
        await h.container.read(tradeSessionsProvider.notifier).start(preferred: '10042817');
        await h.container.read(symbolsProvider.future);
        await until(() => h.container.read(terminalProvider).synced);
      });
      h.container.read(tradeActionsProvider);
      h.container.read(workspaceProvider.notifier).update((w) => w.copyWith(symbol: 'EURUSD'));
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: h.container,
          child: MaterialApp(
            theme: KTheme.trader(Brightness.dark),
            home: I18nScope(
              t: h.container.read(tProvider),
              child: const Scaffold(body: ChartTab()),
            ),
          ),
        ),
      );
      for (var i = 0; i < 4; i++) {
        await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 20)));
        await tester.pump(const Duration(milliseconds: 100));
      }
    }

    Future<void> done(WidgetTester tester) async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump(const Duration(seconds: 2));
      await tester.runAsync(() async => h.container.dispose());
    }

    testWidgets('the EURUSD sell has a take profit only: S sets its stop loss over the ask through modifyPosition', (tester) async {
      await boot(tester);
      final p = h.container.read(terminalProvider).positions.firstWhere((p) => p.symbol == 'EURUSD');
      expect(p.sl, isNull);
      expect(p.tp, isNotNull);
      expect(handle('sl:${p.ticket}'), findsOneWidget);
      expect(handle('tp:${p.ticket}'), findsNothing);

      // a tap: 10 pips over the ask (the sell closes at the ask)
      await tester.tap(handle('sl:${p.ticket}'));
      await tester.pump(const Duration(milliseconds: 100));
      expect(actions.modifies, hasLength(1));
      expect(actions.modifies.single['ticket'], p.ticket);
      expect(actions.modifies.single['sl'], 1.0856);
      expect(actions.modifies.single['tp'], isNull);
      expect(handle('sl:${p.ticket}'), findsNothing);
      // the recorded action never changes the position: the line goes after the hold and S comes back
      await tester.pump(const Duration(seconds: 2));
      expect(handle('sl:${p.ticket}'), findsOneWidget);

      // dragged down across the price: a sell's stop loss is held the stops level (10 points) over the ask
      await tester.drag(handle('sl:${p.ticket}'), const Offset(0, 300));
      await tester.pump(const Duration(milliseconds: 100));
      expect(actions.modifies, hasLength(2));
      expect(actions.modifies.last['sl'], 1.0847);
      await done(tester);
    });
  });
}

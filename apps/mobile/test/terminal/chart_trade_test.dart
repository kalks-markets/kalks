// One-tap trading on the chart (MT5's one-click panel over the plot): SELL shows the live bid, BUY the live ask; the
// tap goes through the terminal's own order path, so the one-click setting decides: off (the default) opens the
// prefilled order sheet to confirm, on sends the market order at once.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:kalks/core/notifications/notifications.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/terminal_controller.dart';
import 'package:kalks/features/terminal/core/workspace.dart';
import 'package:kalks/features/terminal/terminal_screen.dart';
import 'package:kalks/features/terminal/widgets/kit.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

void main() {
  late Harness h;

  Future<void> frames(WidgetTester tester) async {
    for (var i = 0; i < 12; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 40)));
      await tester.pump(const Duration(milliseconds: 100));
    }
  }

  Future<void> open(WidgetTester tester) async {
    await loadFonts();
    TerminalChart.forceNative = true;
    tester.view.physicalSize = const Size(412, 915);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.runAsync(() async {
      h = await Harness.create(
        extra: [
          marketFeedProvider.overrideWith((ref) => FixedFeed({'BTCUSD': q('BTCUSD', 63400, 63418)})),
        ],
      );
    });
    final router = GoRouter(
      initialLocation: '/trader?login=10042817&symbol=btcusd',
      routes: [
        GoRoute(path: '/', builder: (c, s) => const SizedBox.shrink()),
        GoRoute(path: '/trader', builder: (c, s) => TerminalScreen.fromQuery(s.uri.queryParameters)),
      ],
    );
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: h.container,
        child: MaterialApp.router(
          routerConfig: router,
          theme: KTheme.client(Brightness.light),
          builder: (context, child) => I18nScope(t: h.container.read(tProvider), child: child!),
        ),
      ),
    );
    await frames(tester);
  }

  Future<void> close(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(() async => h.container.dispose());
    await tester.pump(const Duration(seconds: 1));
  }

  double priceOn(WidgetTester tester, String side) =>
      tester.widget<PriceText>(find.descendant(of: find.byKey(ValueKey('chart-trade-$side')), matching: find.byType(PriceText))).value;

  testWidgets('the panel floats over the chart with the live bid and ask', (tester) async {
    await open(tester);
    expect(find.byKey(const ValueKey('chart-trade-sell')), findsOneWidget);
    expect(priceOn(tester, 'sell'), 63400);
    expect(priceOn(tester, 'buy'), 63418);
    // over the plot, not under it
    final chart = tester.getRect(find.byType(TerminalChart));
    final sell = tester.getRect(find.byKey(const ValueKey('chart-trade-sell')));
    expect(chart.contains(sell.center), isTrue);
    expect(sell.top - chart.top, lessThan(20));
    await close(tester);
  });

  testWidgets('one-click off (the default): a tap opens the prefilled order sheet, nothing is sent', (tester) async {
    await open(tester);
    expect(h.container.read(workspaceProvider).oneClick, isFalse);
    final before = h.container.read(terminalProvider).positions.length;
    await tester.tap(find.byKey(const ValueKey('chart-trade-sell')));
    await frames(tester);
    expect(find.text('Sell 0.50 lot BTCUSD at market'), findsOneWidget);
    expect(h.container.read(terminalProvider).positions.length, before);
    expect(h.notes.toasts.where((x) => x.kind == NotificationKind.success), isEmpty);
    await close(tester);
  });

  testWidgets('one-click on: a tap sends the market order at once (no sheet)', (tester) async {
    await open(tester);
    h.container.read(workspaceProvider.notifier).update((w) => w.copyWith(oneClick: true));
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('chart-trade-buy')));
    await frames(tester);
    expect(find.text('Buy 0.50 lot BTCUSD at market'), findsNothing);
    expect(h.notes.toasts.where((x) => x.kind == NotificationKind.success), isNotEmpty);
    await close(tester);
  });
}

// The terminal screen as the router opens it: `/trader?login=&symbol=&side=` (Markets and the Client Area's Trade
// buttons) opens the account, the market's chart and the order sheet with that side; an Options account (with
// `mode=options`, the Options page's Trade button) opens Options.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/workspace.dart';
import 'package:kalks/features/terminal/terminal_screen.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

void main() {
  late Harness h;

  Future<void> open(WidgetTester tester, String location) async {
    await loadFonts();
    TerminalChart.forceNative = true;
    tester.view.physicalSize = const Size(412, 915);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.runAsync(() async {
      h = await Harness.create(
        extra: [
          marketFeedProvider.overrideWith((ref) => FixedFeed({'BTCUSD': q('BTCUSD', 63400, 63418), 'XAUUSD': q('XAUUSD', 2654.30, 2654.48)})),
        ],
      );
    });
    final router = GoRouter(
      initialLocation: location,
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
    // the account opens over the preview trade server (real async), then frames settle
    for (var i = 0; i < 12; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 40)));
      await tester.pump(const Duration(milliseconds: 100));
    }
  }

  Future<void> close(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(() async => h.container.dispose());
    await tester.pump(const Duration(seconds: 1));
  }

  testWidgets('symbol and side open the chart of that market and the order sheet', (tester) async {
    await open(tester, '/trader?login=10042817&symbol=btcusd&side=buy');
    expect(h.container.read(workspaceProvider).symbol, 'BTCUSD');
    expect(find.text('10042817'), findsWidgets);
    expect(find.text('Buy 0.50 lot BTCUSD at market'), findsOneWidget);
    await close(tester);
  });

  testWidgets('mode=options on an Options account opens the Options mode', (tester) async {
    await open(tester, '/trader?login=20019001&mode=options');
    expect(h.container.read(tradeModeProvider), 'options');
    await close(tester);
  });
}

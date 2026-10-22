// The trade room redesign (owner design 2026-10): MT5-style quote figures on one column grid, the colour scheme
// (saved, and applied to the whole terminal and the chart), the symbol screen's order book with its depth fallback,
// the sparkline cache, and the Trade tab's compact B / S / T tags and bulk close.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:kalks/core/prefs.dart';
import 'package:kalks/core/theme_controller.dart';
import 'package:kalks/features/terminal/cfd/account_tab.dart';
import 'package:kalks/features/terminal/cfd/chart_tab.dart';
import 'package:kalks/features/terminal/cfd/room.dart';
import 'package:kalks/features/terminal/cfd/symbol_screen.dart';
import 'package:kalks/features/terminal/cfd/trade_tab.dart';
import 'package:kalks/features/terminal/cfd/watchlist_tab.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/sessions.dart';
import 'package:kalks/features/terminal/core/sparklines.dart';
import 'package:kalks/features/terminal/core/terminal_controller.dart';
import 'package:kalks/features/terminal/core/trade_actions.dart';
import 'package:kalks/features/terminal/core/workspace.dart';
import 'package:kalks/features/terminal/terminal_screen.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'harness.dart';

/// Records bulk closes and cancels instead of sending them.
class _Actions extends TradeActions {
  _Actions(super.ref);
  final List<String> bulk = [];
  final List<String> cancels = [];

  @override
  Future<bool> bulkClose(String filter, {String? symbol}) async {
    bulk.add(filter);
    return true;
  }

  @override
  Future<bool> cancelOrder(String ticket) async {
    cancels.add(ticket);
    return true;
  }
}

/// Fixed quotes plus a depth frame (provider depth) for every market asked.
class _DepthFeed extends FixedFeed {
  _DepthFeed(super.quotes, this.frame);
  final Map<String, dynamic> Function(String symbol) frame;

  @override
  void Function() subscribeDepth(String symbol, void Function(Map<String, dynamic>) fn) {
    scheduleMicrotask(() => fn(frame(symbol)));
    return () {};
  }
}

void main() {
  group('quote figures (MT5 big digits)', () {
    test('3 / 5 digits raise the pipette and enlarge the two figures before it', () {
      expect(splitQuote(1.12249, 5), (head: '1.12', big: '24', sup: '9'));
      expect(splitQuote(151.832, 3), (head: '151.', big: '83', sup: '2'));
    });

    test('2 / 4 digits enlarge the last two; 0 / 1 digit the last three characters', () {
      expect(splitQuote(4127.34, 2), (head: '4127.', big: '34', sup: ''));
      expect(splitQuote(0.6512, 4), (head: '0.65', big: '12', sup: ''));
      expect(splitQuote(6741.2, 1), (head: '674', big: '1.2', sup: ''));
      expect(splitQuote(83879, 0), (head: '83', big: '879', sup: ''));
    });

    test('the column grid leaves the symbol what the fixed columns do not take, narrower buttons on small phones', () {
      final wide = QuoteGrid.of(358);
      expect(wide.button, 80);
      expect(wide.symbolWidth(358), 358 - 10 - 8 - 36 - 40 - 160 - 24);
      expect(QuoteGrid.of(328).button, 72);
      expect(QuoteGrid.of(300).button, 64);
    });

    test('compact tags: "B 0.50", "S 0.10", stops "S 1.11850" / "T 1.12600"', () {
      final t = T('en', catalog('en'), catalog('en'));
      expect(sideTag(t, 'buy', 0.5), 'B 0.50');
      expect(sideTag(t, 'sell', 0.1), 'S 0.10');
      expect(stopTag(t, 'sl', 5, 1.1185), 'S 1.11850');
      expect(stopTag(t, 'tp', 5, 1.126), 'T 1.12600');
    });
  });

  group('sparklines', () {
    test('one fetch per market while fresh (rows asking together share it), refetched after ~5 minutes', () async {
      var now = DateTime(2026, 10, 22, 12);
      var calls = 0;
      final cache = SparkCache((s) async {
        calls++;
        return [for (var i = 0; i < 30; i++) Candle(i, 1, 1, 1, 1.0 + i, 0)];
      }, now: () => now);
      final a = cache.get('EURUSD');
      final b = cache.get('EURUSD');
      expect(await a, hasLength(kSparkBars));
      expect((await b).last, 30.0);
      expect(calls, 1);
      expect(cache.peek('EURUSD'), isNotNull);
      now = now.add(const Duration(minutes: 6));
      expect(cache.peek('EURUSD'), isNull);
      await cache.get('EURUSD');
      expect(calls, 2);
    });

    test('no candles: no line (never a made-up one)', () async {
      final cache = SparkCache((s) async => throw Exception('down'));
      expect(await cache.get('EURUSD'), isEmpty);
    });
  });

  group('the order book depth fallback', () {
    test('no depth frame: the ask and the bid alone, no volumes', () {
      final b = BookLadder.of(null, bid: 1.1, ask: 1.1002);
      expect(b.source, BookSource.none);
      expect(b.sized, isFalse);
      expect(b.asks, [(1.1002, 0.0)]);
      expect(b.bids, [(1.1, 0.0)]);
    });

    test('provider depth: 5 asks (farthest first) and 5 bids (best first) with their lots; indicative is labelled', () {
      final frame = {
        'src': 'feed',
        'b': [for (var i = 0; i < 10; i++) [1.1 - i / 10000, 1.0 + i]],
        'a': [for (var i = 0; i < 10; i++) [1.1002 + i / 10000, 2.0 + i]],
      };
      final b = BookLadder.of(frame, bid: 1.1, ask: 1.1002);
      expect(b.source, BookSource.feed);
      expect(b.asks, hasLength(5));
      expect(b.asks.last, (1.1002, 2.0));
      expect(b.asks.first.$1, closeTo(1.1006, 1e-9));
      expect(b.bids.first, (1.1, 1.0));
      expect(b.maxLots, 6.0);
      expect(BookLadder.of({...frame, 'src': 'indicative'}, bid: 1.1, ask: 1.1002).source, BookSource.indicative);
      // a malformed frame counts as none
      expect(BookLadder.of({'src': 'feed', 'b': 'x'}, bid: 1.1, ask: 1.1002).source, BookSource.none);
    });
  });

  group('the colour scheme', () {
    test('is saved in the preferences and read back (blue by default)', () async {
      SharedPreferences.setMockInitialValues({});
      final prefs = await Prefs.open();
      final c = ProviderContainer(overrides: [prefsProvider.overrideWithValue(prefs)]);
      expect(c.read(traderSchemeProvider), TraderScheme.blue);
      await c.read(traderSchemeProvider.notifier).set(TraderScheme.green);
      expect(prefs.traderScheme, 'green');
      c.dispose();
      final again = ProviderContainer(overrides: [prefsProvider.overrideWithValue(await Prefs.open())]);
      expect(again.read(traderSchemeProvider), TraderScheme.green);
      again.dispose();
    });

    test('green / red only changes the up / buy colours', () {
      final blue = KTokens.traderDark();
      final green = KTokens.traderDark(green: true);
      expect(green.up, kTraderGreen);
      expect(green.buyFill, kTraderGreenFill);
      expect(green.down, blue.down);
      expect(green.sellFill, blue.sellFill);
      expect(blue.bg, const Color(0xFF0B0B0F));
      expect(blue.surface, const Color(0xFF121217));
      expect(KTokens.traderLight(green: true).up, kTraderGreenFill);
    });
  });

  group('screens on the preview trade server', () {
    late Harness h;
    late _Actions actions;

    Future<void> boot(WidgetTester tester, {MarketFeed? feed}) async {
      await loadFonts();
      TerminalChart.forceNative = true;
      tester.view.physicalSize = const Size(390, 844);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.runAsync(() async {
        h = await Harness.create(
          extra: [
            marketFeedProvider.overrideWith(
              (ref) =>
                  feed ??
                  FixedFeed({
                    'XAUUSD': q('XAUUSD', 2654.30, 2654.48),
                    'EURUSD': q('EURUSD', 1.08451, 1.08463),
                    'BTCUSD': q('BTCUSD', 63400, 63418),
                  }),
            ),
            tradeActionsProvider.overrideWith((ref) => actions = _Actions(ref)),
          ],
        );
        h.container.listen(terminalProvider, (_, _) {});
        await h.container.read(tradeSessionsProvider.notifier).start(preferred: '10042817');
        await h.container.read(symbolsProvider.future);
        await until(() => h.container.read(terminalProvider).synced);
      });
      h.container.read(tradeActionsProvider);
    }

    Future<void> show(WidgetTester tester, Widget child) async {
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: h.container,
          child: MaterialApp(
            theme: KTheme.trader(Brightness.dark),
            // above the navigator: the sheets and alerts read their texts too
            builder: (context, nav) => I18nScope(t: h.container.read(tProvider), child: nav!),
            home: Scaffold(body: child),
          ),
        ),
      );
      for (var i = 0; i < 3; i++) {
        await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 20)));
        await tester.pump(const Duration(milliseconds: 50));
      }
    }

    Future<void> done(WidgetTester tester) async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump(const Duration(seconds: 1));
      await tester.runAsync(() async => h.container.dispose());
    }

    testWidgets('quotes: every row on the header\'s grid; SELL / BUY take the order path; a row opens the symbol screen', (tester) async {
      await boot(tester);
      await show(tester, const WatchlistTab());
      final sell = find.byKey(const ValueKey('quote-sell-XAUUSD'));
      final buy = find.byKey(const ValueKey('quote-buy-XAUUSD'));
      expect(sell, findsOneWidget);
      // the header's SELL / BUY sit exactly over the buttons of every row
      final header = find.byKey(const ValueKey('quotes-header'));
      final hs = tester.getCenter(find.descendant(of: header, matching: find.text('SELL')));
      final hb = tester.getCenter(find.descendant(of: header, matching: find.text('BUY')));
      expect(tester.getCenter(sell).dx, closeTo(hs.dx, 0.5));
      expect(tester.getCenter(buy).dx, closeTo(hb.dx, 0.5));
      final eur = find.byKey(const ValueKey('quote-sell-EURUSD'));
      expect(tester.getCenter(eur).dx, closeTo(hs.dx, 0.5));
      // MT5 figures: 1.08 · 45 · raised 1 for the EURUSD bid 1.08451
      expect(find.descendant(of: eur, matching: find.text('1.08')), findsOneWidget);
      expect(find.descendant(of: eur, matching: find.text('45')), findsOneWidget);
      expect(find.descendant(of: eur, matching: find.text('1')), findsOneWidget);
      // 16 px side margins: the card starts 16 px in
      expect(tester.getTopLeft(find.byType(QuoteRow).first).dx, 16);

      // Advanced: low · high · time on the same grid
      await tester.tap(find.byKey(const ValueKey('quotes-view')));
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));
      expect(h.container.read(workspaceProvider).advancedQuotes, isTrue);
      expect(find.text('Advanced'), findsOneWidget);
      // (the forex rows come first, taller now)
      expect(find.byKey(const ValueKey('quote-adv-EURUSD')), findsOneWidget);
      expect(tester.getCenter(find.byKey(const ValueKey('quote-adv-EURUSD'))).dx, closeTo(tester.getCenter(header).dx, 0.5));
      await tester.tap(find.byKey(const ValueKey('quotes-view')));
      await tester.pump();

      // one-click off (the default): SELL opens the prefilled ticket
      await tester.tap(sell);
      await tester.pump(const Duration(milliseconds: 400));
      expect(find.text('Sell 0.50 lot XAUUSD at market'), findsOneWidget);
      Navigator.of(tester.element(find.text('Sell 0.50 lot XAUUSD at market'))).pop();
      await tester.pump(const Duration(milliseconds: 400));

      // a row: the symbol screen of that market
      await tester.tap(find.text('XAUUSD').first);
      await tester.pump();
      expect(h.container.read(cfdTabProvider), 'symbol');
      expect(h.container.read(symbolScreenProvider), 'XAUUSD');
      await done(tester);
    });

    testWidgets('symbol screen without depth: the bid and the ask only, "Depth unavailable · prices only", no volumes', (tester) async {
      await boot(tester);
      await show(tester, const SymbolScreen(symbol: 'XAUUSD'));
      final book = find.byKey(const ValueKey('symbol-book'));
      expect(find.descendant(of: book, matching: find.text('Depth unavailable · prices only')), findsOneWidget);
      expect(find.descendant(of: book, matching: find.text('LOTS')), findsNothing);
      expect(find.descendant(of: book, matching: find.text('2654.48')), findsOneWidget);
      expect(find.descendant(of: book, matching: find.text('2654.30')), findsOneWidget);
      // the mid, SELL / BUY with their prices, Open chart, Specs
      expect(find.text('2654.39'), findsOneWidget);
      expect(find.byKey(const ValueKey('symbol-sell')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('symbol-chart')));
      await tester.pump();
      expect(h.container.read(cfdTabProvider), 'chart');
      expect(h.container.read(workspaceProvider).symbol, 'XAUUSD');
      await done(tester);
    });

    testWidgets('symbol screen with provider depth: 5 asks · spread · 5 bids with their lots', (tester) async {
      await boot(
        tester,
        feed: _DepthFeed(
          {'XAUUSD': q('XAUUSD', 2654.30, 2654.48)},
          (s) => {
            's': s,
            'src': 'feed',
            'b': [for (var i = 0; i < 10; i++) [2654.30 - i / 10, 1.0 + i]],
            'a': [for (var i = 0; i < 10; i++) [2654.48 + i / 10, 2.0 + i]],
          },
        ),
      );
      await show(tester, const SymbolScreen(symbol: 'XAUUSD'));
      final book = find.byKey(const ValueKey('symbol-book'));
      expect(find.descendant(of: book, matching: find.text('Market depth · lots')), findsOneWidget);
      expect(find.descendant(of: book, matching: find.text('LOTS')), findsOneWidget);
      // asks 2.00 … 6.00, bids 1.00 … 5.00
      expect(find.descendant(of: book, matching: find.text('6.00')), findsOneWidget);
      expect(find.descendant(of: book, matching: find.text('1.00')), findsOneWidget);
      expect(find.descendant(of: book, matching: find.text('5.00')), findsNWidgets(2));
      expect(find.descendant(of: book, matching: find.text('2654.88')), findsOneWidget);
      expect(find.descendant(of: book, matching: find.text('2653.90')), findsOneWidget);
      await done(tester);
    });

    testWidgets('trade tab: equity card, positions with "B 0.50" / "S 1.20" badges and S / T chips only when set, ⋯ closes all', (tester) async {
      await boot(tester);
      await show(tester, const TradeTab());
      expect(find.byKey(const ValueKey('trade-equity')), findsOneWidget);
      // the XAUUSD buy has both stops, the EURUSD sell only a take profit
      expect(find.text('B 0.50'), findsOneWidget);
      expect(find.text('S 1.20'), findsOneWidget);
      expect(find.text('S 2630.00'), findsOneWidget);
      expect(find.text('T 2690.00'), findsOneWidget);
      expect(find.text('T 1.07900'), findsOneWidget);
      final eur = h.container.read(terminalProvider).positions.firstWhere((p) => p.symbol == 'EURUSD');
      expect(find.descendant(of: find.byKey(ValueKey('p${eur.ticket}')), matching: find.byType(StopChip)), findsOneWidget);
      // the cards keep 16 px from the screen edges
      expect(tester.getTopLeft(find.descendant(of: find.byKey(const ValueKey('trade-equity')), matching: find.byType(Column)).first).dx, 32);

      Future<void> frames() async {
        for (var i = 0; i < 10; i++) {
          await tester.pump(const Duration(milliseconds: 100));
        }
      }

      // ⋯ → Close all positions → the confirm names how many and the P&L → Close
      await tester.tap(find.byKey(const ValueKey('trade-menu')));
      await frames();
      await tester.tap(find.text('Close all positions'));
      await frames();
      expect(find.textContaining('Positions ('), findsWidgets);
      await tester.tap(find.widgetWithText(KPressable, 'Close').last);
      await frames();
      expect(actions.bulk, ['all']);
      expect(bulkCloses('profitable', 3), isTrue);
      expect(bulkCloses('losing', 3), isFalse);
      await done(tester);
    });

    testWidgets('account tab: the colour cards switch the scheme, saved and applied to the terminal', (tester) async {
      await boot(tester);
      await show(tester, const AccountTab());
      expect(find.byKey(const ValueKey('scheme-blue')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('scheme-green')));
      await tester.pump();
      expect(h.container.read(traderSchemeProvider), TraderScheme.green);
      expect(h.container.read(prefsProvider).traderScheme, 'green');
      await done(tester);
    });
  });

  testWidgets('the terminal paints itself and its chart in the chosen scheme', (tester) async {
    await loadFonts();
    TerminalChart.forceNative = true;
    tester.view.physicalSize = const Size(412, 915);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    late Harness h;
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
    for (var i = 0; i < 12; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 40)));
      await tester.pump(const Duration(milliseconds: 100));
    }
    KTokens tokens() => Theme.of(tester.element(find.byType(ChartTab))).extension<KTokens>()!;
    expect(tokens().up, const Color(0xFF2F7BFF));
    await h.container.read(traderSchemeProvider.notifier).set(TraderScheme.green);
    await tester.pump();
    expect(tokens().up, kTraderGreen);
    expect(tokens().buyFill, kTraderGreenFill);
    // the chart's BUY side of the pill follows
    final pill = tester.widget<Container>(
      find.descendant(of: find.byKey(const ValueKey('chart-trade-buy')), matching: find.byType(Container)).first,
    );
    expect((pill.decoration! as BoxDecoration).color, kTraderGreenFill);
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(() async => h.container.dispose());
    await tester.pump(const Duration(seconds: 1));
  });
}

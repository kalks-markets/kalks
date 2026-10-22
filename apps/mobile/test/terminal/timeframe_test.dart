// The chart's timeframe control (TradingView style): the timeframe chip and the starred timeframes that fit a 360 px
// phone with no sideways scrolling; the chip's sheet lists every timeframe by group, a tap shows it, a star pins it
// (saved with the workspace).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:kalks/core/prefs.dart';
import 'package:kalks/features/terminal/cfd/timeframe_picker.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/workspace.dart';
import 'package:kalks/features/terminal/terminal_screen.dart';
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

  Future<void> open(WidgetTester tester, {double width = 360}) async {
    await loadFonts();
    TerminalChart.forceNative = true;
    tester.view.physicalSize = Size(width, 780);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.runAsync(() async {
      h = await Harness.create(
        extra: [
          marketFeedProvider.overrideWith((ref) => FixedFeed({'XAUUSD': q('XAUUSD', 2654.30, 2654.48)})),
        ],
      );
    });
    final router = GoRouter(
      initialLocation: '/trader?login=10042817&symbol=xauusd',
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
    TerminalChart.forceNative = false;
  }

  String tf() => h.container.read(workspaceProvider).tf;

  testWidgets('a 360 px phone: the timeframe chip and the starred timeframes that fit, nothing to slide', (tester) async {
    await open(tester);
    final chip = find.byKey(const ValueKey('chart-tf'));
    expect(find.descendant(of: chip, matching: find.text('M15')), findsOneWidget);
    final bar = find.byType(TimeframeBar);
    expect(find.descendant(of: bar, matching: find.byType(Scrollable)), findsNothing);
    // the starred timeframes after the chip, in order, the one on screen left out; all of them inside the screen
    final shown = [
      for (final f in kDefaultTfFavourites)
        if (find.byKey(ValueKey('chart-tf-$f')).evaluate().isNotEmpty) f,
    ];
    expect(shown, isNotEmpty);
    expect(shown, kDefaultTfFavourites.where((f) => f != 'M15').take(shown.length).toList());
    for (final f in shown) {
      final r = tester.getRect(find.byKey(ValueKey('chart-tf-$f')));
      expect(r.right, lessThanOrEqualTo(360));
      expect(r.height, greaterThanOrEqualTo(44), reason: 'touch target');
      expect(r.width, greaterThanOrEqualTo(44), reason: 'touch target');
    }
    expect(tester.getRect(chip).height, greaterThanOrEqualTo(44));
    // the menu stays on screen after them
    expect(tester.getRect(find.byKey(const ValueKey('chart-menu'))).right, lessThanOrEqualTo(360));

    // one tap on a starred timeframe shows it
    await tester.tap(find.byKey(ValueKey('chart-tf-${shown.first}')));
    await tester.pump();
    expect(tf(), shown.first);
    expect(find.descendant(of: chip, matching: find.text(shown.first)), findsOneWidget);
    expect(find.byKey(const ValueKey('chart-tf-M15')), findsOneWidget, reason: 'the previous one is a starred timeframe again');
    await close(tester);
  });

  testWidgets('the chip opens every timeframe by group; a star pins one, a tap shows it', (tester) async {
    await open(tester);
    await tester.tap(find.byKey(const ValueKey('chart-tf')));
    await tester.pumpAndSettle();
    expect(find.text('Timeframes'), findsOneWidget);
    expect(find.text('MINUTES'), findsOneWidget);
    expect(find.text('HOURS'), findsOneWidget);
    expect(find.text('DAYS AND LONGER'), findsOneWidget);
    for (final f in kTimeframes) {
      expect(find.byKey(ValueKey('tf-sheet-$f')), findsOneWidget);
    }
    expect(find.text('5 minutes'), findsOneWidget);
    expect(find.text('4 hours'), findsOneWidget);
    expect(find.text('Monthly'), findsOneWidget);

    // star M30, unstar M1: saved with the workspace
    await tester.tap(find.byKey(const ValueKey('tf-star-M30')));
    await tester.tap(find.byKey(const ValueKey('tf-star-M1')));
    await tester.pump();
    expect(h.container.read(workspaceProvider).tfFavourites, ['M5', 'M15', 'M30', 'H1', 'H4', 'D1']);
    expect(h.container.read(prefsProvider).featureJson('trader.workspace')?['tfFavourites'], ['M5', 'M15', 'M30', 'H1', 'H4', 'D1']);

    await tester.tap(find.byKey(const ValueKey('tf-sheet-H4')));
    await tester.pumpAndSettle();
    expect(tf(), 'H4');
    expect(find.text('Timeframes'), findsNothing, reason: 'the sheet closed');
    expect(find.descendant(of: find.byKey(const ValueKey('chart-tf')), matching: find.text('H4')), findsOneWidget);
    await close(tester);
  });

  test('favourites load in timeframe order, unknown ones dropped; an old workspace gets the defaults', () {
    expect(
      Workspace.fromJson({
        'tfFavourites': ['D1', 'X9', 'M1'],
      }).tfFavourites,
      ['M1', 'D1'],
    );
    expect(Workspace.fromJson({'tf': 'H1'}).tfFavourites, kDefaultTfFavourites);
    expect(favouritesThatFit(168), 2);
    expect(favouritesThatFit(10), 0);
  });
}

// CFD / Options account split in Kalks Trader: the workspace follows the account on screen (a CFD account opens the
// CFD workspace, an Options account the options one); `?mode=` picks an account of that product; there is no
// CFD | Options switch: the account switcher changes the account (and offers an Options account to a client without); a CFD market's link on an Options account moves to
// the CFD account; the account switcher lists CFD accounts and Options accounts; the Options module off keeps CFD only.
// The sample client (lib/preview) holds CFD accounts 10042817 (default), 10051123 and 20017734, and the demo Options
// account 20019001.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_riverpod/misc.dart' show Override;
import 'package:flutter_test/flutter_test.dart';
import 'package:go_router/go_router.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/core/models/account.dart';
import 'package:kalks/core/models/trading.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/models.dart';
import 'package:kalks/features/terminal/core/sessions.dart';
import 'package:kalks/features/terminal/terminal_screen.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/preview/c1/preview_accounts.dart';
import 'package:kalks/preview/preview_data.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

/// A config that stays as given (no fetch).
class _Config extends ConfigController {
  _Config(this.config);
  final AppConfig config;

  @override
  AppConfig build() => config;

  @override
  Future<void> refresh() async {}
}

/// The broker with the Options module switched off.
Override optionsOff() => configProvider.overrideWith(
  () => _Config(
    AppConfig.fromJson({
      ...previewConfig,
      'modules': {...previewConfig['modules'] as Map<String, dynamic>, 'options': false},
    }),
  ),
);

EngineAccount _acc(int login, {String? product, String type = 'live', bool def = false, String status = 'active'}) => EngineAccount.fromJson({
  'login': login,
  'type': type,
  'group': product == 'options' ? 'options-standard' : 'standard',
  'status': status,
  'isDefault': def,
  'product': ?product,
});

void main() {
  group('the product of accounts and groups', () {
    test('only "options" reads as an Options account or group; missing or anything else is CFD', () {
      expect(_acc(1).product, 'cfd');
      expect(_acc(1).isOptions, isFalse);
      expect(_acc(1, product: 'options').isOptions, isTrue);
      expect(_acc(1, product: 'OPTIONS').product, 'cfd');
      expect(EngineGroup.fromJson(const {'code': 'options-standard', 'product': 'options'}).isOptions, isTrue);
      expect(EngineGroup.fromJson(const {'code': 'standard'}).product, 'cfd');
      expect(TAccount.fromJson(const {'login': 1, 'product': 'options'}).isOptions, isTrue);
      expect(TAccount.fromJson(const {'login': 1}).product, 'cfd');
    });

    test('the account for a product: the recent one of it, else default-starred, else live, else any', () {
      final all = [
        _acc(10000001),
        _acc(10000002, def: true),
        _acc(50000003, product: 'options', type: 'demo'),
        _acc(10000004, product: 'options'),
        _acc(10000005, product: 'options', status: 'disabled'),
      ];
      expect(pickProductLogin(all, 'cfd'), '10000002');
      expect(pickProductLogin(all, 'cfd', recent: '10000001'), '10000001');
      // the recent login of the other product doesn't count
      expect(pickProductLogin(all, 'cfd', recent: '10000004'), '10000002');
      expect(pickProductLogin(all, 'options'), '10000004');
      expect(pickProductLogin(all, 'options', recent: '50000003'), '50000003');
      // blocked accounts don't count; none of a product -> null
      expect(pickProductLogin(all, 'options', recent: '10000005'), '10000004');
      expect(pickProductLogin([_acc(1)], 'options'), isNull);
      expect(pickProductLogin([_acc(50000001, product: 'options', type: 'demo'), _acc(50000002, product: 'options', type: 'demo')], 'options'), '50000001');
    });
  });

  group('the workspace follows the account', () {
    late Harness h;
    Future<void> make({List<Override> extra = const []}) async => h = await Harness.create(extra: extra);
    tearDown(() async {
      h.container.dispose();
      resetPreviewAccounts();
    });

    test('a CFD account opens the CFD workspace, an Options account the options one', () async {
      await make();
      final s = h.container.read(tradeSessionsProvider.notifier);
      await s.start(preferred: '10042817');
      expect(h.container.read(tradeModeProvider), 'cfd');
      expect(await s.activate('20019001'), isTrue);
      expect(h.container.read(activeProductProvider), 'options');
      expect(h.container.read(tradeModeProvider), 'options');
      expect(await s.activate('20017734'), isTrue);
      expect(h.container.read(tradeModeProvider), 'cfd');
    });

    test('?mode=options opens the client\'s Options account, mode=cfd a CFD account, no mode the default', () async {
      await make();
      final s = h.container.read(tradeSessionsProvider.notifier);
      await s.start(mode: 'options');
      expect(h.container.read(tradeSessionsProvider).active, '20019001');
      expect(h.container.read(tradeModeProvider), 'options');
      // already open: a mode the account on screen doesn't trade switches to that product's account
      await s.start(mode: 'cfd');
      expect(h.container.read(tradeSessionsProvider).active, '10042817');
      expect(h.container.read(tradeModeProvider), 'cfd');
      // and back to the Options account last shown
      expect(await s.loginForProduct('options'), '20019001');
    });

    test('without a mode: the default-starred account, whatever its product', () async {
      await make();
      await h.container.read(apiProvider).post<Map<String, dynamic>>('trading/prefs', body: {'defaultLogin': 20019001});
      await h.container.read(tradeSessionsProvider.notifier).start();
      expect(h.container.read(tradeSessionsProvider).active, '20019001');
      expect(h.container.read(tradeModeProvider), 'options');
    });

    test('the Options module off: CFD only (the default prefers a CFD account, an Options account shows CFD)', () async {
      await make(extra: [optionsOff()]);
      await h.container.read(apiProvider).post<Map<String, dynamic>>('trading/prefs', body: {'defaultLogin': 20019001});
      final s = h.container.read(tradeSessionsProvider.notifier);
      await s.start();
      expect(h.container.read(tradeSessionsProvider).active, '10042817');
      await s.activate('20019001');
      expect(h.container.read(activeProductProvider), 'options');
      expect(h.container.read(tradeModeProvider), 'cfd');
    });
  });

  group('the terminal screen', () {
    late Harness h;

    Future<void> open(WidgetTester tester, String location, {List<Override> extra = const []}) async {
      await loadFonts();
      TerminalChart.forceNative = true;
      tester.view.physicalSize = const Size(412, 915);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      await tester.runAsync(() async {
        h = await Harness.create(
          extra: [
            marketFeedProvider.overrideWith((ref) => FixedFeed({'EURUSD': q('EURUSD', 1.08412, 1.08419), 'XAUUSD': q('XAUUSD', 2654.30, 2654.48)})),
            ...extra,
          ],
        );
      });
      final router = GoRouter(
        initialLocation: location,
        routes: [
          GoRoute(path: '/', builder: (c, s) => const SizedBox.shrink()),
          GoRoute(path: '/trader', builder: (c, s) => TerminalScreen.fromQuery(s.uri.queryParameters)),
          GoRoute(path: '/accounts/new', builder: (c, s) => Text('wizard ${s.uri.query}')),
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
      await _frames(tester);
    }

    Future<void> close(WidgetTester tester) async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(() async => h.container.dispose());
      await tester.pump(const Duration(seconds: 1));
      resetPreviewAccounts();
    }

    String? active() => h.container.read(tradeSessionsProvider).active;

    testWidgets('a CFD account opens CFD; ?mode=options without a login opens the Options account', (tester) async {
      await open(tester, '/trader?login=10042817');
      expect(active(), '10042817');
      expect(h.container.read(tradeModeProvider), 'cfd');
      await close(tester);

      await open(tester, '/trader?mode=options');
      expect(active(), '20019001');
      expect(h.container.read(tradeModeProvider), 'options');
      await close(tester);
    });

    testWidgets('a CFD market\'s link on an Options account moves to the client\'s CFD account', (tester) async {
      await open(tester, '/trader?login=20019001&symbol=eurusd');
      await _frames(tester);
      expect(active(), '10042817');
      expect(h.container.read(tradeModeProvider), 'cfd');
      await close(tester);
    });

    testWidgets('no CFD | Options in the header: the room follows the account picked in the account switcher', (tester) async {
      await open(tester, '/trader?login=10042817');
      expect(find.byKey(const ValueKey('trader-product')), findsNothing);
      expect(find.text('Options'), findsNothing);
      // the account pill opens the switcher; the Options account opens the Options room
      await tester.tap(find.text('10042817').first);
      await _frames(tester);
      await tester.tap(find.byKey(const ValueKey('trader-account-20019001')));
      await _frames(tester);
      expect(active(), '20019001');
      expect(h.container.read(tradeModeProvider), 'options');
      final pill = find.byWidgetPredicate((w) => w.runtimeType.toString() == '_AccountPill');
      expect(
        find.descendant(of: pill, matching: find.text('OPTIONS')),
        findsOneWidget,
        reason: 'the pill says so',
      );
      // and back to the CFD account
      await tester.tap(find.text('20019001').first);
      await _frames(tester);
      await tester.tap(find.byKey(const ValueKey('trader-account-10042817')));
      await _frames(tester);
      expect(active(), '10042817');
      expect(h.container.read(tradeModeProvider), 'cfd');
      await close(tester);
    });

    testWidgets('a client without an Options account is offered one in the account switcher', (tester) async {
      // the sample client without its Options account
      final cfdOnly = ownAccountsProvider.overrideWith((ref) async => [_acc(10042817, def: true), _acc(10051123), _acc(20017734, type: 'demo')]);
      await open(tester, '/trader?login=10042817', extra: [cfdOnly]);
      await tester.tap(find.text('10042817').first);
      await _frames(tester);
      expect(find.text('OPTIONS ACCOUNTS'), findsOneWidget);
      expect(find.byKey(const ValueKey('trader-open-options')), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('trader-open-options')));
      await _frames(tester);
      expect(find.text('wizard product=options'), findsOneWidget, reason: 'the open-account wizard with the product chosen');
      await close(tester);
    });

    testWidgets('the account switcher lists CFD accounts, then Options accounts with their tag', (tester) async {
      await open(tester, '/trader?login=10042817');
      // the account pill in the header (its login)
      await tester.tap(find.text('10042817').first);
      await _frames(tester);
      expect(find.text('CFD ACCOUNTS'), findsOneWidget);
      expect(find.text('OPTIONS ACCOUNTS'), findsOneWidget);
      final options = find.byKey(const ValueKey('trader-account-20019001'));
      expect(options, findsOneWidget);
      expect(find.descendant(of: options, matching: find.text('OPTIONS')), findsOneWidget);
      expect(tester.getTopLeft(find.byKey(const ValueKey('trader-account-20017734'))).dy, lessThan(tester.getTopLeft(options).dy));
      await close(tester);
    });

    testWidgets('the Options module off: no CFD | Options, no Options accounts, CFD workspace', (tester) async {
      await open(tester, '/trader?login=20019001', extra: [optionsOff()]);
      expect(find.byKey(const ValueKey('trader-product')), findsNothing);
      expect(h.container.read(tradeModeProvider), 'cfd');
      await tester.tap(find.text('20019001').first);
      await _frames(tester);
      expect(find.byKey(const ValueKey('trader-account-10042817')), findsOneWidget);
      expect(find.byKey(const ValueKey('trader-account-20019001')), findsNothing);
      expect(find.text('OPTIONS ACCOUNTS'), findsNothing);
      await close(tester);
    });
  });
}

/// The preview trade server answers (real async), then frames settle.
Future<void> _frames(WidgetTester tester) async {
  for (var i = 0; i < 12; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 40)));
    await tester.pump(const Duration(milliseconds: 100));
  }
}

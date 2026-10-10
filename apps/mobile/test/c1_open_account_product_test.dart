// The open-account wizard's Product step (CFD / Options account split): the CFD account and Options account cards,
// the options intro before a first Options account (until its terms are accepted), only the account types of the
// chosen product (never prop or the options market maker), the account limit per kind and product, `?product=`
// preselecting, and no Options card while the broker has the Options module off. On the sample API: CFD types
// Standard, Pro, Cent (live), ECN (live); the Options type Options Standard.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/core/models/account.dart';
import 'package:kalks/core/models/trading.dart';
import 'package:kalks/features/accounts/accounts_screen.dart';
import 'package:kalks/features/accounts/open_account_screen.dart';
import 'package:kalks/features/accounts/widgets/group_card.dart';
import 'package:kalks/features/options_intro/options_screen.dart';
import 'package:kalks/preview/c1/preview_accounts.dart';
import 'package:kalks/preview/c2/options.dart';
import 'package:kalks/preview/preview_data.dart' as sample;
import 'package:kalks/router/router.dart';

import 'helpers/test_app.dart';

EngineGroup _group(String code, {String? product, String types = 'both', bool enabled = true}) =>
    EngineGroup.fromJson({'code': code, 'accountTypes': types, 'enabled': enabled, 'product': ?product, 'maxAccountsPerUser': 2});

EngineAccount _account(int login, String group, {String? product, String type = 'live'}) =>
    EngineAccount.fromJson({'login': login, 'type': type, 'group': group, 'product': ?product});

final Finder _wizard = find.byType(OpenAccountScreen);

/// Scrolls the wizard down (jumps) until `target` is built, then puts it a third of the way down the screen.
Future<void> _reveal(WidgetTester tester, Finder target) async {
  final page = find.descendant(of: _wizard, matching: find.byType(Scrollable)).first;
  final pos = tester.state<ScrollableState>(page).position;
  pos.jumpTo(0);
  await tester.pump(const Duration(milliseconds: 60));
  for (var i = 0; i < 40 && target.evaluate().isEmpty; i++) {
    if (pos.pixels >= pos.maxScrollExtent) break;
    pos.jumpTo(math.min(pos.pixels + 250, pos.maxScrollExtent));
    await tester.pump(const Duration(milliseconds: 60));
  }
  if (target.evaluate().isEmpty) return;
  final top = tester.getRect(target.first).top;
  pos.jumpTo((pos.pixels + top - 300).clamp(0, pos.maxScrollExtent).toDouble());
  await tester.pump(const Duration(milliseconds: 120));
}

/// The account types the Type step shows (every card, scrolling through the page).
Future<Set<String>> _typesShown(WidgetTester tester) async {
  final page = find.descendant(of: _wizard, matching: find.byType(Scrollable)).first;
  final pos = tester.state<ScrollableState>(page).position;
  final out = <String>{};
  pos.jumpTo(0);
  await tester.pump(const Duration(milliseconds: 60));
  for (var i = 0; i < 40; i++) {
    out.addAll(tester.widgetList<EngineGroupCard>(find.byType(EngineGroupCard)).map((c) => c.group.code));
    if (pos.pixels >= pos.maxScrollExtent) break;
    pos.jumpTo(math.min(pos.pixels + 250, pos.maxScrollExtent));
    await tester.pump(const Duration(milliseconds: 60));
  }
  return out;
}

/// Lets the taps' animations end and the sample API answer.
Future<void> _calm(WidgetTester tester) async {
  for (var i = 0; i < 4; i++) {
    await tester.pump(const Duration(milliseconds: 400));
  }
  await settle(tester, frames: 4);
}

Future<void> _tap(WidgetTester tester, Finder target) async {
  await _reveal(tester, target);
  await tester.tap(target.last);
  await _calm(tester);
}

Future<void> _continue(WidgetTester tester) => _tap(tester, find.text('Continue'));

bool _accepted() => ((previewOptions('GET', 'suitability/options', const {}, const {})!.$2 as Map)['disclosureAccepted']) == true;

void main() {
  setUpAll(initializeDateFormatting);
  setUp(() {
    resetPreviewAccounts();
    resetPreviewOptions();
  });

  group('rules', () {
    test('the types of a product: enabled, open to the kind, of that product; never prop or the options market maker', () {
      final cfd = _group('standard');
      final options = _group('options-standard', product: 'options');
      expect(groupOffers(cfd, AccountKind.live, 'cfd'), isTrue);
      expect(groupOffers(cfd, AccountKind.live, 'options'), isFalse);
      expect(groupOffers(options, AccountKind.demo, 'options'), isTrue);
      expect(groupOffers(options, AccountKind.live, 'cfd'), isFalse);
      expect(groupOffers(_group('options-pro', product: 'options', enabled: false), AccountKind.live, 'options'), isFalse);
      expect(groupOffers(_group('options-mm', product: 'options'), AccountKind.live, 'options'), isFalse);
      expect(groupOffers(_group('prop-50k'), AccountKind.live, 'cfd'), isFalse);
      expect(groupOffers(_group('cent', types: 'live'), AccountKind.demo, 'cfd'), isFalse);
    });

    test('the account limit counts the accounts of that kind and the type\'s product', () {
      final g = _group('options-standard', product: 'options');
      final accounts = [
        _account(1, 'options-standard', product: 'options'),
        _account(2, 'options-standard', product: 'options', type: 'demo'),
        // an engine without the product field: a CFD account, not counted against an Options type
        _account(3, 'options-standard'),
      ];
      expect(groupUsed(accounts, g, AccountKind.live), 1);
      expect(groupUsed(accounts, g, AccountKind.demo), 1);
    });

    test('the Options page trades on Options accounts only: active ones, default first, then live', () {
      EngineAccount a(int login, {String? product, String type = 'live', bool def = false, String status = 'active'}) =>
          EngineAccount.fromJson({'login': login, 'type': type, 'group': 'g', 'product': ?product, 'isDefault': def, 'status': status});
      final list = optionsAccounts([
        a(10000001, def: true),
        a(50000002, product: 'options', type: 'demo'),
        a(10000003, product: 'options'),
        a(10000004, product: 'options', def: true, type: 'demo'),
        a(10000005, product: 'options', status: 'archived'),
      ]);
      expect([for (final x in list) x.login], [10000004, 10000003, 50000002]);
    });
  });

  group('My accounts', () {
    testWidgets('accounts by product: CFD accounts, then Options accounts with the OPTIONS tag', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts?tab=demo');
      await settle(tester);
      final page = find.byType(AccountsScreen);
      expect(page, findsOneWidget);
      Future<void> show(Finder f) async {
        final pos = tester.state<ScrollableState>(find.descendant(of: page, matching: find.byType(Scrollable)).first).position;
        for (var i = 0; i < 40 && f.evaluate().isEmpty && pos.pixels < pos.maxScrollExtent; i++) {
          pos.jumpTo(math.min(pos.pixels + 250, pos.maxScrollExtent));
          await tester.pump(const Duration(milliseconds: 60));
        }
      }

      final options = find.byKey(const ValueKey('row-$previewOptionsLogin'));
      await show(options);
      expect(find.byKey(const ValueKey('section-cfd')), findsOneWidget);
      expect(find.byKey(const ValueKey('section-options')), findsOneWidget);
      expect(find.descendant(of: options, matching: find.text('OPTIONS')), findsOneWidget);
      expect(tester.getTopLeft(find.byKey(const ValueKey('row-20017734'))).dy, lessThan(tester.getTopLeft(options).dy));
      expect(find.descendant(of: find.byKey(const ValueKey('row-20017734')), matching: find.text('OPTIONS')), findsNothing);
      await unmount(tester);

      // the live accounts are all CFD accounts: no sections
      final c2 = await pumpApp(tester, signedIn: true);
      c2.read(routerProvider).go('/accounts?tab=live');
      await settle(tester);
      await show(find.byKey(const ValueKey('row-10051123')));
      expect(find.byKey(const ValueKey('row-10051123')), findsOneWidget);
      expect(find.byKey(const ValueKey('section-cfd')), findsNothing);
      await unmount(tester);
    });
  });

  group('Options page', () {
    testWidgets('the Trade button opens the Options account; none -> "Open an Options account" to the wizard', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      previewOptions('POST', 'suitability/options/accept', const {'version': 3}, const {});
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/options');
      await settle(tester);
      expect(find.byType(OptionsScreen), findsOneWidget);
      expect(find.text('Trade options in Kalks Trader'), findsOneWidget);
      expect(find.byKey(const ValueKey('options-no-account')), findsNothing);
      await unmount(tester);

      // the sample client's only Options account deleted (archived): the page offers to open one
      previewAccounts('POST', 'trading/accounts/$previewOptionsLogin/archive', const {}, const {});
      final c2 = await pumpApp(tester, signedIn: true);
      c2.read(routerProvider).go('/options');
      await settle(tester);
      expect(find.byKey(const ValueKey('options-no-account')), findsOneWidget);
      expect(find.text('No Options account yet'), findsOneWidget);
      await tester.tap(find.text('Open an Options account'));
      await settle(tester);
      expect(_wizard, findsOneWidget);
      // the terms are accepted: straight to Live / Demo on the Options product
      expect(find.text('Choose an account'), findsWidgets);
      await _continue(tester);
      expect(await _typesShown(tester), {'options-standard'});
      await unmount(tester);
    });
  });

  group('pages', () {
    testWidgets('the Product step: CFD account and Options account; CFD lists only CFD types', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/new');
      await settle(tester);
      expect(_wizard, findsOneWidget);
      expect(find.text('What will this account trade?'), findsWidgets);
      expect(find.byKey(const ValueKey('wizard-product-cfd')), findsOneWidget);
      expect(find.byKey(const ValueKey('wizard-product-options')), findsOneWidget);
      // the products as card faces (their names in caps)
      expect(find.text('CFD ACCOUNT'), findsWidgets);
      expect(find.text('OPTIONS ACCOUNT'), findsWidgets);

      // CFD (the default): Live / Demo, then the CFD types only
      await _continue(tester);
      expect(find.text('Choose an account'), findsWidgets);
      await _continue(tester);
      expect(find.text('Pick an account type'), findsWidgets);
      expect(await _typesShown(tester), {'standard', 'pro', 'cent', 'ecn'});
      await unmount(tester);
    });

    testWidgets('Options: the intro first while not accepted, accepting it goes on, then only Options types', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/new');
      await settle(tester);
      await _tap(tester, find.byKey(const ValueKey('wizard-product-options')));
      await _continue(tester);
      // the options intro (the Options page's card) before the Live / Demo step
      expect(find.byType(OptionsIntroCard), findsOneWidget);
      expect(find.text('One quick step before your first Options account: the 1-minute options intro.'), findsOneWidget);
      expect(find.text('Choose an account'), findsNothing);
      // Continue waits for "I understand how options work"
      await _continue(tester);
      expect(find.byType(OptionsIntroCard), findsOneWidget);
      expect(_accepted(), isFalse);
      await _tap(tester, find.text('I understand how options work'));
      await _continue(tester);
      expect(_accepted(), isTrue);
      expect(find.text('Choose an account'), findsWidgets);
      await _continue(tester);
      expect(find.text('Pick an account type'), findsWidgets);
      expect(await _typesShown(tester), {'options-standard'});
      // the intro is done: Back goes past it to Live / Demo, then to the Product step
      await _tap(tester, find.text('Back'));
      expect(find.text('Choose an account'), findsWidgets);
      await _tap(tester, find.text('Back'));
      expect(find.text('What will this account trade?'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('?product=options goes past the Product step: the intro until accepted, then Live / Demo', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/new?product=options');
      await settle(tester);
      expect(find.byType(OptionsIntroCard), findsOneWidget);
      await unmount(tester);

      previewOptions('POST', 'suitability/options/accept', const {'version': 3}, const {});
      final c2 = await pumpApp(tester, signedIn: true);
      c2.read(routerProvider).go('/accounts/new?product=options');
      await settle(tester);
      expect(find.byType(OptionsIntroCard), findsNothing);
      expect(find.text('Choose an account'), findsWidgets);
      await _continue(tester);
      expect(await _typesShown(tester), {'options-standard'});
      await unmount(tester);
    });

    testWidgets('?group= of an Options type: the Options product, its type preselected', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      previewOptions('POST', 'suitability/options/accept', const {'version': 3}, const {});
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/new?group=options-standard');
      await settle(tester);
      // straight to Configure (an Options type: no leverage to pick)
      expect(find.text('Configure your account'), findsWidgets);
      expect(find.textContaining('Options Standard'), findsWidgets);
      expect(find.text('Leverage'), findsNothing);
      await unmount(tester);
    });

    testWidgets('the Options module off: no Product step, no Options types', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      final config = AppConfig.fromJson({
        ...sample.previewConfig,
        'modules': {...sample.previewConfig['modules'] as Map<String, dynamic>, 'options': false},
      });
      final c = await pumpApp(tester, signedIn: true, config: config);
      c.read(routerProvider).go('/accounts/new?product=options');
      await settle(tester);
      expect(find.text('What will this account trade?'), findsNothing);
      expect(find.byKey(const ValueKey('wizard-product-options')), findsNothing);
      expect(find.byType(OptionsIntroCard), findsNothing);
      expect(find.text('Choose an account'), findsWidgets);
      await _continue(tester);
      expect(await _typesShown(tester), {'standard', 'pro', 'cent', 'ecn'});
      await unmount(tester);
    });
  });
}

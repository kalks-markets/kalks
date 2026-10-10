// The broker's module switches in the running app (AppConfig.modules): a switched-off module's page shows "Not
// available" (web /unavailable) with the way back; a module switched off while its page is open moves the client off
// it and leaves the navigation; the Dashboard leaves out everything that leads into a switched-off module.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/features/common/system_screens.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/features/options_intro/options_screen.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/shell/more_screen.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

AppConfig _cfg(Map<String, bool> modules) => AppConfig.fromJson({'modules': modules});

Uri _at(ProviderContainer c) => c.read(routerProvider).routerDelegate.currentConfiguration.uri;

void main() {
  testWidgets('a switched-off module\'s page: Not available, the module named, Back to dashboard', (tester) async {
    final c = await pumpApp(tester, signedIn: true, config: _cfg(const {'options': false}));
    c.read(routerProvider).go('/options');
    await settle(tester);
    expect(_at(c).path, '/unavailable');
    expect(_at(c).queryParameters['m'], 'options');
    expect(find.byType(UnavailableScreen), findsOneWidget);
    expect(find.byType(OptionsScreen), findsNothing);
    expect(find.text('Not available'), findsOneWidget);
    expect(find.text("This section isn't offered on your account. Contact support if you think this is a mistake."), findsOneWidget);
    // the module's name in a line of its own once the catalog has the texts (else the page reads as the web's)
    final en = (jsonDecode(File('assets/i18n/en.json').readAsStringSync()) as Map).cast<String, Object?>();
    final named = find.byKey(const ValueKey('unavailable-module'));
    if (en['shell.module.options'] is String && en['shell.system.unavailable.module'] is String) {
      expect(named, findsOneWidget);
      expect(find.textContaining(en['shell.module.options']! as String), findsOneWidget);
    } else {
      expect(named, findsNothing);
    }
    await tester.tap(find.text('Back to dashboard'));
    await settle(tester);
    expect(_at(c).path, '/');
    expect(find.byType(DashboardScreen), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('switched off while its page is open: off the page, out of the navigation; back on: back in', (tester) async {
    final c = await pumpApp(tester, signedIn: true, config: _cfg(const {}));
    c.read(routerProvider).go('/more');
    await settle(tester);
    expect(find.descendant(of: find.byType(MoreScreen), matching: find.text('Options')), findsOneWidget);
    c.read(routerProvider).go('/options');
    await settle(tester);
    expect(_at(c).path, '/options');
    expect(find.byType(OptionsScreen), findsOneWidget);

    setConfig(c, _cfg(const {'options': false}));
    await settle(tester);
    expect(_at(c).toString(), '/unavailable?m=options');
    expect(find.byType(UnavailableScreen), findsOneWidget);
    await tester.tap(find.text('Back to dashboard'));
    await settle(tester);
    c.read(routerProvider).go('/more');
    await settle(tester);
    expect(find.descendant(of: find.byType(MoreScreen), matching: find.text('Options')), findsNothing);

    setConfig(c, _cfg(const {'options': true}));
    await settle(tester);
    expect(find.descendant(of: find.byType(MoreScreen), matching: find.text('Options')), findsOneWidget);
    c.read(routerProvider).go('/options');
    await settle(tester);
    expect(find.byType(OptionsScreen), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('the Dashboard\'s pills drop a switched-off page', (tester) async {
    await pumpApp(tester, signedIn: true, config: _cfg(const {'news': false}));
    final pills = find.byType(KPillNav);
    expect(find.descendant(of: pills, matching: find.text('Overview')), findsOneWidget);
    expect(find.descendant(of: pills, matching: find.text('Markets')), findsOneWidget);
    expect(find.descendant(of: pills, matching: find.text('Calendar')), findsOneWidget);
    expect(find.descendant(of: pills, matching: find.text('News')), findsNothing);
    await unmount(tester);
  });

  testWidgets('the Dashboard leaves out what leads into switched-off modules', (tester) async {
    await pumpApp(
      tester,
      signedIn: true,
      config: _cfg(const {
        'wallet': false,
        'rewards': false,
        'copy_trading': false,
        'ib': false,
        'options': false,
        'markets': false,
        'news': false,
        'calendar': false,
        'ai': false,
      }),
    );
    expect(find.byType(DashboardScreen), findsOneWidget);
    // the photo keeps no AI bar (ai off); Overview alone, so no pills
    expect(find.byType(KHeroPicture), findsOneWidget);
    expect(find.byType(KPillNav), findsNothing);
    expect(find.byKey(const ValueKey('ask-ai-pill')), findsNothing);

    // through the whole page: what stays, in order, and nothing that leads into an off module
    const gone = [
      'Deposit',
      'Withdraw',
      'Transfer',
      'WALLET BALANCE',
      'REWARDS',
      'Partner (IB)',
      'Contests & Rewards',
      'Options',
      'History',
      'Fund account',
    ];
    final page = find.descendant(of: find.byType(DashboardScreen), matching: find.byType(Scrollable)).first;
    void check() {
      for (final s in gone) {
        expect(find.text(s), findsNothing, reason: s);
      }
      expect(find.text('Markets'), findsNothing);
    }

    check();
    for (final s in ['TOTAL BALANCE', 'TOTAL EQUITY', 'Shortcuts', 'Open account', 'Kalks Trader', 'Statements', 'Support', 'Your accounts']) {
      for (var i = 0; i < 80 && find.text(s).hitTestable().evaluate().isEmpty; i++) {
        await tester.drag(page, const Offset(0, -250));
        await tester.pump(const Duration(milliseconds: 50));
        check();
      }
      expect(find.text(s), findsWidgets, reason: s);
    }
    await unmount(tester);
  });
}

// Accounts (agent C1): the web's account rules (refills, flavours, passwords, the transfer and demo-balance limits,
// the ⋯ menu per account state) and the pages on the sample API: My accounts (Live / Demo / Archived), the account
// detail, the Open account wizard, and the money move between accounts behind its emailed code (sent once, with
// the step-up token and a request id).
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:kalks/core/models/account.dart';
import 'package:kalks/features/accounts/account_actions.dart';
import 'package:kalks/features/accounts/account_detail_screen.dart';
import 'package:kalks/features/accounts/accounts_data.dart';
import 'package:kalks/features/accounts/accounts_screen.dart';
import 'package:kalks/features/accounts/open_account_screen.dart';
import 'package:kalks/features/accounts/widgets/account_bits.dart';
import 'package:kalks/features/accounts/widgets/group_card.dart';
import 'package:kalks/preview/c1/preview_accounts.dart';
import 'package:kalks/preview/preview_data.dart' as sample;
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

EngineAccount _account(Map<String, dynamic> over) => EngineAccount.fromJson({
  'login': 10042817,
  'type': 'live',
  'group': 'pro',
  'groupName': 'Pro',
  'mode': 'hedging',
  'currency': 'USD',
  'leverage': 200,
  'status': 'active',
  'name': '',
  'balance': 1000,
  'equity': 1000,
  ...over,
});

/// Scrolls the page of `screen` down (jumps, no fling) until `target` is built, then puts it a third of the way
/// down the screen, clear of the header, the floating chat button and the tab bar.
Future<void> _reveal(WidgetTester tester, Finder screen, Finder target) async {
  final page = find.descendant(of: screen, matching: find.byType(Scrollable)).first;
  final pos = tester.state<ScrollableState>(page).position;
  // not built (scrolled past, above): look from the top
  if (target.evaluate().isEmpty && pos.pixels > 0) {
    pos.jumpTo(0);
    await tester.pump(const Duration(milliseconds: 60));
  }
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

/// Lets the drags' fling end and the sample API answer.
Future<void> _calm(WidgetTester tester) async {
  for (var i = 0; i < 4; i++) {
    await tester.pump(const Duration(milliseconds: 400));
  }
  await settle(tester, frames: 4);
}

void main() {
  setUpAll(initializeDateFormatting);
  setUp(resetPreviewAccounts);

  group('rules (web components/trading/ui.tsx, extras.tsx, archive.tsx)', () {
    test('demo refills: left today, full balance, expired', () {
      final demo = _account({
        'type': 'demo',
        'balance': 4000,
        'demo': {'initialBalance': 10000, 'refillsPerDay': 3, 'refillsUsedToday': 1, 'expiryDays': 30},
      });
      expect(refillsLeft(demo), 2);
      expect(demoTarget(demo), 10000);
      expect(demoFull(demo), isFalse);
      expect(refillDisabled(demo), isFalse);
      expect(refillDisabled(_account({...demo.raw, 'balance': 10000})), isTrue);
      expect(refillDisabled(_account({...demo.raw, 'status': 'expired'})), isTrue);
      expect(
        refillsLeft(
          _account({
            ...demo.raw,
            'demo': {'initialBalance': 10000, 'refillsPerDay': 3, 'refillsUsedToday': 5},
          }),
        ),
        0,
      );
      expect(refillsLeft(_account({})), 0);
    });

    test('copy / PAMM / MAM flavours and the copied strategy', () {
      expect(accountFlavor(_account({'group': 'copy-std'})), 'copy');
      expect(accountFlavor(_account({'group': 'PAMM'})), 'pamm');
      expect(accountFlavor(_account({'group': 'mam-pro'})), 'mam');
      expect(accountFlavor(_account({'group': 'copytrader'})), isNull);
      expect(copyingName(_account({'name': 'Copy · Atlas FX'})), 'Atlas FX');
      expect(copyingName(_account({'name': 'Main'})), isNull);
    });

    test('trading passwords: 8 to 64 characters with a letter and a digit; generated ones pass', () {
      expect(livePasswordOk('abcd1234'), isTrue);
      expect(livePasswordOk('abc123'), isFalse);
      expect(livePasswordOk('abcdefgh'), isFalse);
      expect(livePasswordOk('12345678'), isFalse);
      expect(livePasswordOk('a1${'x' * 63}'), isFalse);
      for (var i = 0; i < 50; i++) {
        final p = generatePassword();
        expect(p.length, 12);
        expect(livePasswordOk(p), isTrue, reason: p);
      }
    });

    test('money between accounts: two accounts, up to two decimals, within the free funds', () {
      expect(transferBetweenOk(from: 1, to: 2, amount: '100.50', availableUsd: 200), isTrue);
      expect(transferBetweenOk(from: 1, to: 1, amount: '10', availableUsd: 200), isFalse);
      expect(transferBetweenOk(from: 1, to: null, amount: '10', availableUsd: 200), isFalse);
      expect(transferBetweenOk(from: 1, to: 2, amount: '10.123', availableUsd: 200), isFalse);
      expect(transferBetweenOk(from: 1, to: 2, amount: '0', availableUsd: 200), isFalse);
      expect(transferBetweenOk(from: 1, to: 2, amount: '200.01', availableUsd: 200), isFalse);
      expect(transferBetweenOk(from: 1, to: 2, amount: '200', availableUsd: 200), isTrue);
      expect(RegExp(r'^[0-9a-f]{32}$').hasMatch(newIdempotencyKey()), isTrue);
      expect(newIdempotencyKey(), isNot(newIdempotencyKey()));
    });

    test('demo balance of your choice: 100 to 1,000,000', () {
      expect(demoBalanceOk('100'), isTrue);
      expect(demoBalanceOk('1000000'), isTrue);
      expect(demoBalanceOk('99.99'), isFalse);
      expect(demoBalanceOk('1000001'), isFalse);
      expect(demoBalanceOk('abc'), isFalse);
    });

    test('the ⋯ menu per account state, in the web order', () {
      List<AccountMenuItem> items(Map<String, dynamic> over, {bool readOnly = false}) => accountMenuItems(_account(over), readOnly: readOnly);
      // own live account: everything but the demo balance
      expect(items({}), [
        AccountMenuItem.details,
        AccountMenuItem.defaultStar,
        AccountMenuItem.leverage,
        AccountMenuItem.changeType,
        AccountMenuItem.moveBetween,
        AccountMenuItem.passwords,
        AccountMenuItem.statements,
        AccountMenuItem.historyZip,
        AccountMenuItem.rename,
        AccountMenuItem.delete,
        AccountMenuItem.close,
      ]);
      // demo: the demo balance, no money moves, no permanent close
      final demo = items({'type': 'demo'});
      expect(demo, contains(AccountMenuItem.demoBalance));
      expect(demo, isNot(contains(AccountMenuItem.moveBetween)));
      expect(demo, isNot(contains(AccountMenuItem.close)));
      // prop challenge: no type change, no money moves, no delete / close
      final prop = items({'group': 'prop-50k'});
      expect(prop, isNot(contains(AccountMenuItem.changeType)));
      expect(prop, isNot(contains(AccountMenuItem.moveBetween)));
      expect(prop, isNot(contains(AccountMenuItem.delete)));
      // copy account: no type change
      expect(items({'group': 'copy'}), isNot(contains(AccountMenuItem.changeType)));
      // archived: statements, the ZIP and Restore; closed: no Restore
      expect(items({'status': 'archived'}), [AccountMenuItem.details, AccountMenuItem.statements, AccountMenuItem.historyZip, AccountMenuItem.restore]);
      expect(items({'status': 'closed'}), isNot(contains(AccountMenuItem.restore)));
      // view-only / read-only staff: nothing that changes the account
      expect(items({}, readOnly: true), isEmpty);
      expect(items({'status': 'archived'}, readOnly: true), isNot(contains(AccountMenuItem.restore)));
    });
  });

  group('pages', () {
    testWidgets('My accounts: Live, Demo and Archived tabs list their accounts', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts');
      await settle(tester);
      expect(find.byType(AccountsScreen), findsOneWidget);
      await _reveal(tester, find.byType(AccountsScreen), find.text('#10042817'));
      expect(find.text('My accounts'), findsWidgets);
      expect(find.textContaining('10042817'), findsWidgets);
      expect(find.textContaining('10051123'), findsWidgets);
      expect(find.textContaining('20017734'), findsNothing);

      // the sample client's demo accounts: Standard and the Options account
      await _reveal(tester, find.byType(AccountsScreen), find.text('Demo 2'));
      await tester.tap(find.text('Demo 2'));
      await _calm(tester);
      expect(find.textContaining('20017734'), findsWidgets);
      expect(find.textContaining('10042817'), findsNothing);

      await _reveal(tester, find.byType(AccountsScreen), find.text('Archived 1'));
      await tester.tap(find.text('Archived 1'));
      await _calm(tester);
      expect(find.textContaining('10038890'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('account detail: the header and the web tabs; ?tab= opens a tab', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/10042817');
      await settle(tester);
      expect(find.byType(AccountDetailScreen), findsOneWidget);
      expect(find.textContaining('10042817'), findsWidgets);
      expect(find.text('Overview'), findsWidgets);
      expect(find.textContaining('Positions'), findsWidgets);
      expect(find.byType(TradeButton), findsWidgets);

      c.read(routerProvider).go('/accounts/10042817?tab=positions');
      await settle(tester);
      // the account's open positions (sample: EURUSD among them)
      expect(find.textContaining('EURUSD'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('the ⋯ menu of a live account offers the web actions', timeout: const Timeout(Duration(minutes: 2)), (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/10042817');
      await settle(tester);
      await _reveal(tester, find.byType(AccountDetailScreen), find.byType(AccountMenuButton));
      await tester.tap(find.byType(AccountMenuButton).first);
      await _calm(tester);
      for (final label in ['Account details', 'Rename', 'Statements (CSV)']) {
        expect(find.text(label), findsWidgets, reason: label);
      }
      await unmount(tester);
    });

    testWidgets('moving money between accounts asks for the emailed code and sends once with the token', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts');
      await settle(tester);
      final ctx = tester.element(find.byType(AccountsScreen));
      final from = EngineAccount.fromJson(((sample.previewAccounts['accounts'] as List).first as Map).cast<String, dynamic>());
      var done = false;
      unawaited(showTransferBetweenSheet(ctx, from: from).then((v) => done = v));
      await _calm(tester);
      expect(find.text('Move money between your accounts'), findsWidgets);

      // To: the cent account
      await tester.tap(find.text('Choose an account').last);
      await _calm(tester);
      await tester.tap(find.textContaining('#10051123').last);
      await _calm(tester);
      await tester.enterText(find.byType(TextField).last, '25.50');
      await tester.pump();
      await tester.tap(find.text('Move money').last);
      await _calm(tester);

      // the code sheet: nothing moved yet
      expect(previewAccountCalls.where((x) => x.path == 'trading/transfers/between'), isEmpty);
      await tester.enterText(find.byType(TextField).last, '123456');
      await _calm(tester);
      final sent = previewAccountCalls.where((x) => x.path == 'trading/transfers/between').toList();
      expect(sent, hasLength(1));
      expect(sent.single.body['stepup_token'], 'preview-stepup');
      expect(sent.single.body['fromLogin'], 10042817);
      expect(sent.single.body['toLogin'], 10051123);
      expect(sent.single.body['amount'], '25.50');
      expect(RegExp(r'^[0-9a-f]{32}$').hasMatch('${sent.single.body['idempotency_key']}'), isTrue);
      await _calm(tester);
      expect(done, isTrue);
      expect(find.text('Move money between your accounts'), findsNothing);
      await unmount(tester);
    });

    testWidgets('Open account: the defaults, the acknowledgement, Open; no password asked, sent or shown', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/new?type=demo');
      await settle(tester);
      final wizard = find.byType(OpenAccountScreen);
      expect(wizard, findsOneWidget);
      // the Product step first (a CFD account by default), then the one Account step
      expect(find.text('What will this account trade?'), findsWidgets);
      await _reveal(tester, wizard, find.text('Continue'));
      await tester.tap(find.text('Continue').last);
      await _calm(tester);
      expect(find.text('Choose an account'), findsWidgets);
      // no password field anywhere on the way, just the note that none is needed
      expect(find.byType(PasswordInput), findsNothing);
      expect(find.text('Choose my own trading password'), findsNothing);
      final note = find.textContaining('Set a trading or investor password anytime');
      await _reveal(tester, wizard, note);
      expect(note, findsOneWidget);
      // the Standard type preselected; Open waits for the acknowledgement
      final selected = tester.widgetList<EngineGroupCard>(find.byType(EngineGroupCard)).where((x) => x.selected).map((x) => x.group.code);
      expect(selected, ['standard']);
      await _reveal(tester, wizard, find.byKey(const ValueKey('wizard-open')));
      expect(tester.widget<KButton>(find.byKey(const ValueKey('wizard-open'))).onPressed, isNull);
      await _reveal(tester, wizard, find.textContaining('I understand that demo results use virtual funds'));
      await tester.tap(find.textContaining('I understand that demo results use virtual funds'));
      await tester.pump();
      await _reveal(tester, wizard, find.text('Open demo account'));
      await tester.tap(find.text('Open demo account').last);
      await _calm(tester);
      final opened = previewAccountCalls.where((x) => x.method == 'POST' && x.path == 'trading/accounts').toList();
      expect(opened, hasLength(1));
      expect(opened.single.body['type'], 'demo');
      expect(opened.single.body['group'], 'standard');
      expect(opened.single.body.containsKey('password'), isFalse);
      expect(opened.single.body.containsKey('investorPassword'), isFalse);

      // Done: the account number and server, Trade now, the optional passwords; no password shown
      expect(find.text('Your account is ready'), findsWidgets);
      expect(find.byKey(const ValueKey('created-trade')), findsOneWidget);
      expect(find.text('Trade now'), findsOneWidget);
      expect(find.text('Account number'), findsOneWidget);
      expect(find.text('Trading password'), findsNothing);
      expect(find.text('Investor password'), findsNothing);
      expect(find.text('Copy all'), findsNothing);
      await _reveal(tester, wizard, find.byKey(const ValueKey('created-set-investor')));
      expect(find.text('Passwords are optional'), findsOneWidget);
      // Set investor password: the account's password sheet (emailed code on Continue)
      await tester.tap(find.byKey(const ValueKey('created-set-investor')));
      await _calm(tester);
      expect(find.text('Set investor (view-only) password'), findsWidgets);
      expect(find.text('New password'), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('Open account: Live | Demo on the Account step, More options folds leverage and the demo balance', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/new?product=cfd');
      await settle(tester);
      final wizard = find.byType(OpenAccountScreen);
      // ?product= goes straight to the Account step; Live by default
      expect(find.text('Choose an account'), findsWidgets);
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('wizard-kind')), matching: find.text('Live')));
      await _calm(tester);
      await _reveal(tester, wizard, find.text('Open live account'));
      expect(find.text('Open live account'), findsOneWidget);
      await _reveal(tester, wizard, find.byKey(const ValueKey('wizard-kind')));
      await tester.tap(find.descendant(of: find.byKey(const ValueKey('wizard-kind')), matching: find.text('Demo')));
      await _calm(tester);
      await _reveal(tester, wizard, find.text('Open demo account'));
      expect(find.text('Open demo account'), findsOneWidget);
      // the advanced choices are folded away
      expect(find.text('Leverage'), findsNothing);
      expect(find.text('Starting balance'), findsNothing);
      await _reveal(tester, wizard, find.byKey(const ValueKey('wizard-more-options')));
      await tester.tap(find.byKey(const ValueKey('wizard-more-options')));
      await _calm(tester);
      expect(find.text('Leverage'), findsOneWidget);
      expect(find.text('Starting balance'), findsOneWidget);
      await _reveal(tester, wizard, find.text(r'$5,000'));
      await tester.tap(find.text(r'$5,000'));
      await tester.pump();
      await _reveal(tester, wizard, find.textContaining('I understand that demo results use virtual funds'));
      await tester.tap(find.textContaining('I understand that demo results use virtual funds'));
      await tester.pump();
      await _reveal(tester, wizard, find.text('Open demo account'));
      await tester.tap(find.text('Open demo account').last);
      await _calm(tester);
      final opened = previewAccountCalls.where((x) => x.method == 'POST' && x.path == 'trading/accounts').single;
      expect(opened.body['initialBalance'], 5000);
      await unmount(tester);
    });

    testWidgets('Credentials tab: Set trading password and Set investor (view-only) password, each explained', timeout: const Timeout(Duration(minutes: 2)), (
      tester,
    ) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/accounts/10042817?tab=credentials');
      await settle(tester);
      final screen = find.byType(AccountDetailScreen);
      await _reveal(tester, screen, find.byKey(const ValueKey('set-investor-password')));
      expect(find.text('Set trading password'), findsOneWidget);
      expect(find.text('Set investor (view-only) password'), findsOneWidget);
      expect(find.text('Full access: log in from another trading app (MT-style) and trade. Not needed on Kalks.'), findsOneWidget);
      expect(find.text('Read-only: share it so someone can view positions and history without trading.'), findsOneWidget);
      expect(find.text('Change'), findsNothing);
      await _reveal(tester, screen, find.byKey(const ValueKey('set-trading-password')));
      await tester.tap(find.byKey(const ValueKey('set-trading-password')));
      await _calm(tester);
      expect(find.text('Set trading password'), findsWidgets);
      expect(find.textContaining('Setting it replaces any earlier one'), findsOneWidget);
      await unmount(tester);
    });
  });
}

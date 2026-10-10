// Copy & PAMM (lib/features/social) on the sample-data API: Discover → master profile → the 4-step follow, pausing a
// copy on My copies, investing in a PAMM fund, and the MAM pages (connect an account on Managed accounts; the
// manager's dashboard with a per-account percent).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/social/copy_screen.dart';
import 'package:kalks/features/social/discover_screen.dart';
import 'package:kalks/features/social/investments_screen.dart';
import 'package:kalks/features/social/mam_screen.dart';
import 'package:kalks/features/social/managed_screen.dart';
import 'package:kalks/features/social/master_profile_screen.dart';
import 'package:kalks/features/social/pamm_screen.dart';
import 'package:kalks/preview/c2/social.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';

import '../helpers/test_app.dart';

/// The page's own vertical scroll view (the first Scrollable under the screen).
Finder _page(Type screen) => find.descendant(of: find.byType(screen), matching: find.byType(Scrollable)).first;

/// Scrolls `f` into view, then to the middle of the screen (clear of the frosted header and tab bar).
Future<void> _scrollTo(WidgetTester tester, Finder f, Finder page, {double delta = 300}) async {
  await tester.scrollUntilVisible(f, delta, scrollable: page);
  await tester.pump();
  unawaited(Scrollable.ensureVisible(tester.element(f.first), alignment: 0.5));
  await settle(tester, frames: 3);
}

/// Brings a widget inside an open sheet into view and taps it.
Future<void> _tapInSheet(WidgetTester tester, Finder f) async {
  await tester.ensureVisible(f.first);
  await settle(tester, frames: 2);
  await tester.tap(f.first);
  await settle(tester);
}

void main() {
  setUp(resetPreviewSocial);

  testWidgets('discover: leaderboard, master profile and the 4-step follow', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    c.read(routerProvider).go('/social');
    await settle(tester);

    expect(find.byType(DiscoverScreen), findsOneWidget);
    expect(find.text('Discover masters'), findsWidgets);
    expect(find.text('Leaderboard'), findsOneWidget);
    final page = _page(DiscoverScreen);
    // the board as visiting cards: the names in caps
    await _scrollTo(tester, find.text('ALPHAWAVE'), page);
    // invite-only masters stay off the leaderboard
    expect(find.text('PRIVATE ALPHA'), findsNothing);
    await tester.tap(find.text('ALPHAWAVE'));
    await settle(tester);

    expect(find.byType(MasterProfileScreen), findsOneWidget);
    expect(find.text('Trend following on the majors, H4'), findsOneWidget);
    expect(find.text('Invest (PAMM)'), findsOneWidget);
    final profile = _page(MasterProfileScreen);
    await _scrollTo(tester, find.text('Risk & statistics'), profile);
    // the Copy button (not the programme's Copy chip above it)
    final copy = find.widgetWithText(KButton, 'Copy');
    await _scrollTo(tester, copy, profile, delta: -300);
    await tester.tap(copy);
    await settle(tester);

    // sizing → risk limits → amount → review
    expect(find.text('Copy AlphaWave'), findsOneWidget);
    for (var i = 0; i < 3; i++) {
      await tester.tap(find.text('Continue'));
      await settle(tester);
    }
    expect(find.text('Confirm & start copying'), findsOneWidget);
    await _tapInSheet(tester, find.text('I understand how copying works'));
    await tester.tap(find.text('Confirm & start copying'));
    await settle(tester);
    expect(find.text('Now copying AlphaWave'), findsWidgets);
    expect(find.text('My subscriptions'), findsOneWidget);
    await tester.tap(find.text('My subscriptions'));
    await settle(tester);

    // the new subscription is on My copies
    expect(find.byType(CopyScreen), findsOneWidget);
    expect(find.text('Current 4'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('my copies: KPIs, the current list and pausing a copy', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    c.read(routerProvider).go('/social/copy');
    await settle(tester);

    expect(find.byType(CopyScreen), findsOneWidget);
    expect(find.text('Copy trading'), findsWidgets);
    expect(find.text('Current 3'), findsOneWidget);
    expect(find.text('Stopped 1'), findsOneWidget);
    final page = _page(CopyScreen);
    await _scrollTo(tester, find.text('Pause').first, page);
    // Steady Carry is paused already
    expect(find.text('Resume'), findsOneWidget);
    await tester.tap(find.text('Pause').first);
    await settle(tester);
    expect(find.text('Copying paused'), findsWidgets);
    await _scrollTo(tester, find.text('Resume').first, page);
    expect(find.text('Resume'), findsNWidgets(2));
    await unmount(tester);
  });

  testWidgets('PAMM: funds, invest from the wallet, then My investments', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    c.read(routerProvider).go('/social/pamm');
    await settle(tester);

    expect(find.byType(PammScreen), findsOneWidget);
    expect(find.text('PAMM funds'), findsWidgets);
    final page = _page(PammScreen);
    // sorted by AUM: Nordic Swing first
    await _scrollTo(tester, find.text('NORDIC SWING FUND'), page);
    await _scrollTo(tester, find.text('Invest').first, page);
    await tester.tap(find.text('Invest').first);
    await settle(tester);

    expect(find.text('Invest in Nordic Swing Fund'), findsOneWidget);
    await _tapInSheet(tester, find.textContaining('PAMM investments can lose value'));
    await tester.tap(find.text('Queue investment'));
    await settle(tester);
    expect(find.text('Investment queued'), findsWidgets);

    c.read(routerProvider).go('/social/investments');
    await settle(tester);
    expect(find.byType(InvestmentsScreen), findsOneWidget);
    expect(find.text('AlphaWave Growth Fund'), findsWidgets);
    await unmount(tester);
  });

  testWidgets('MAM: connect an account on Managed accounts; the manager sets a percent per account', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    c.read(routerProvider).go('/social/managed');
    await settle(tester);

    expect(find.byType(ManagedScreen), findsOneWidget);
    expect(find.text('Managed accounts'), findsWidgets);
    expect(find.text('Your managed accounts'), findsOneWidget);
    expect(find.text('Steady Carry Managed'), findsWidgets);
    final page = _page(ManagedScreen);
    await _scrollTo(tester, find.text('AlphaWave MAM'), page);
    // linked to Steady Carry already; the other programmes can be connected
    expect(find.text('Linked'), findsOneWidget);
    await tester.tap(find.text('Connect').first);
    await settle(tester);

    expect(find.text('Connect to AlphaWave MAM'), findsOneWidget);
    expect(find.text('#10042817'), findsOneWidget);
    await _tapInSheet(tester, find.textContaining('I have read the terms'));
    await tester.tap(find.text('Grant trading authority'));
    await settle(tester);
    expect(find.text('Account #10042817 is now managed by AlphaWave MAM'), findsWidgets);
    expect(find.text('Linked'), findsNWidgets(2));

    c.read(routerProvider).go('/social/mam');
    await settle(tester);
    expect(find.byType(MamManagerScreen), findsOneWidget);
    expect(find.text('MAM manager'), findsWidgets);
    expect(find.text('Arjun Macro MAM'), findsOneWidget);
    final mam = _page(MamManagerScreen);
    await _scrollTo(tester, find.text('Allocation preview'), mam);
    await _scrollTo(tester, find.text('Set').first, mam);
    await tester.tap(find.text('Set').first);
    await settle(tester);
    expect(find.text('Percent of each block'), findsOneWidget);
    await tester.enterText(find.byType(EditableText).last, '80');
    await settle(tester, frames: 2);
    await tester.tap(find.text('Save'));
    await settle(tester);
    expect(find.text('Allocation updated'), findsWidgets);
    await unmount(tester);
  });
}

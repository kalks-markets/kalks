// The module pager (lib/shell/module_pager.dart): a module's pages slide under a finger and from the tabs with the
// URL following, pages visited keep their scroll, the Dashboard's picture page hands over to the frosted header as it
// slides out and takes the hero back as it returns, the same right to left, and a deep link opens the pager on its
// page.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/features/markets/markets_screen.dart';
import 'package:kalks/features/portfolio/ledger_screen.dart';
import 'package:kalks/features/portfolio/portfolio_screen.dart';
import 'package:kalks/features/wallet/deposit_screen.dart';
import 'package:kalks/features/wallet/transfer_screen.dart';
import 'package:kalks/features/wallet/wallet_history_screen.dart';
import 'package:kalks/features/wallet/wallet_screen.dart';
import 'package:kalks/features/wallet/withdraw_screen.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/shell/app_shell.dart';
import 'package:kalks/shell/chrome.dart';
import 'package:kalks/shell/module_pager.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

/// The shell's header in tests: no status bar, no demo strip, the header row and the module's page tabs.
const double _headerH = KSize.header + KSize.subNav;

String _path(ProviderContainer c) => c.read(routerProvider).routerDelegate.currentConfiguration.uri.path;

Future<ProviderContainer> _open(WidgetTester tester, String location, {String locale = 'en'}) async {
  final c = await pumpApp(tester, signedIn: true, locale: locale);
  c.read(routerProvider).go(location);
  await settle(tester);
  return c;
}

/// The module's page tabs in the header (the one row whose labels include `label`).
Finder _tabs(String label) => find.byWidgetPredicate((w) => w is KSubNav && w.labels.contains(label));

Finder _scrollOf(Type screen) => find.descendant(of: find.byType(screen), matching: find.byType(Scrollable)).first;

/// A finger swipe across the pager: `dx` < 0 drags the pages to the left (the next page in a left-to-right layout).
/// `y` picks a spot without a horizontal list of its own (a chip row keeps its drags): 130 is a page's title.
Future<void> _swipe(WidgetTester tester, double dx, {double y = 130}) async {
  await tester.dragFrom(Offset(206, y), Offset(dx, 0));
  await settle(tester, frames: 6);
}

/// Taps a page pill in the sheet of a photo page (scrolled into view first: the row scrolls sideways).
Future<void> _tapPill(WidgetTester tester, String label) async {
  final pill = find.descendant(of: find.byType(KPillNav), matching: find.text(label));
  await tester.dragUntilVisible(pill, find.descendant(of: find.byType(KPillNav), matching: find.byType(Scrollable)).first, const Offset(-60, 0));
  await tester.pump(const Duration(milliseconds: 300));
  await tester.tap(pill);
}

/// Taps a tab of the header's row (scrolled into view first: the row scrolls sideways).
Future<void> _tapTab(WidgetTester tester, String row, String label, {double step = -60}) async {
  final tab = find.descendant(of: _tabs(row), matching: find.text(label));
  await tester.dragUntilVisible(tab, find.descendant(of: _tabs(row), matching: find.byType(Scrollable)).first, Offset(step, 0));
  await tester.pump(const Duration(milliseconds: 300));
  await tester.tap(tab);
}

void main() {
  testWidgets('a swipe on Wallet lands on Deposit: the URL, the screen and the tab follow', (tester) async {
    final c = await _open(tester, '/wallet');
    expect(find.byType(ModulePager), findsOneWidget);
    expect(find.byType(WalletScreen), findsOneWidget);
    expect(find.byType(DepositScreen), findsNothing);
    // the overview opens on the wallet's photo: its pages as pills in the sheet
    expect(tester.widget<KPillNav>(find.byType(KPillNav)).current, 0);

    await _swipe(tester, -320, y: 250);
    expect(_path(c), '/wallet/deposit');
    expect(find.byType(DepositScreen), findsOneWidget);
    expect(tester.getTopLeft(find.byType(DepositScreen)).dx, 0);
    // the overview is off screen (kept alive, not shown)
    expect(find.byType(WalletScreen), findsNothing);
    expect(find.byType(WalletScreen, skipOffstage: false), findsOneWidget);
    expect(tester.widget<KSubNav>(_tabs('Deposit')).current, 1);
    expect(c.read(modulePagerOffsetProvider).value, 1);
    // the page under the header
    expect(MediaQuery.of(tester.element(find.byType(DepositScreen))).padding.top, _headerH);
    await unmount(tester);
  });

  testWidgets('tapping the History tab slides there without building the pages between', (tester) async {
    final c = await _open(tester, '/wallet');
    await _tapPill(tester, 'History');
    await tester.pump();
    expect(_path(c), '/wallet/history');
    // mid-slide (the slide starts after the frame of the route change): the overview leaving, History coming in
    // from the end
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 120));
    expect(find.byType(WalletScreen), findsOneWidget);
    expect(find.byType(WalletHistoryScreen), findsOneWidget);
    expect(tester.getTopLeft(find.byType(WalletHistoryScreen)).dx, greaterThan(0));
    expect(tester.getTopLeft(find.byType(WalletHistoryScreen)).dx, lessThan(412));
    await settle(tester, frames: 6);
    expect(tester.getTopLeft(find.byType(WalletHistoryScreen)).dx, 0);
    expect(find.byType(WalletScreen), findsNothing);
    expect(tester.widget<KSubNav>(_tabs('History')).current, 4);
    expect(c.read(modulePagerOffsetProvider).value, 4);
    // Deposit, Withdraw and Transfer were never built
    expect(find.byType(DepositScreen, skipOffstage: false), findsNothing);
    expect(find.byType(WithdrawScreen, skipOffstage: false), findsNothing);
    expect(find.byType(TransferScreen, skipOffstage: false), findsNothing);
    // and back to the first page from the tab
    await _tapTab(tester, 'History', 'Overview', step: 60);
    await settle(tester, frames: 6);
    expect(_path(c), '/wallet');
    expect(find.byType(WalletScreen), findsOneWidget);
    expect(find.byType(WalletHistoryScreen), findsNothing);
    await unmount(tester);
  });

  testWidgets('swiping away and back keeps the first page where it was scrolled', (tester) async {
    final c = await _open(tester, '/wallet');
    final pos = tester.state<ScrollableState>(_scrollOf(WalletScreen)).position;
    pos.jumpTo(300);
    await tester.pump();
    await _swipe(tester, -320, y: 250);
    expect(_path(c), '/wallet/deposit');
    await _swipe(tester, 320, y: 250);
    expect(_path(c), '/wallet');
    expect(find.byType(WalletScreen), findsOneWidget);
    expect(tester.state<ScrollableState>(_scrollOf(WalletScreen)).position.pixels, 300);
    await unmount(tester);
  });

  testWidgets('the Dashboard hands its picture over to the frosted header as it slides, and takes it back', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    expect(find.byKey(kShellHeroHeader), findsNothing);
    expect(MediaQuery.of(tester.element(find.byType(DashboardScreen))).padding.top, 0);

    // the finger moves the picture a little (the first move only crosses the slop): the header fades in at once,
    // the controls go
    final finger = await tester.startGesture(const Offset(206, 200));
    await finger.moveBy(const Offset(-30, 0));
    await finger.moveBy(const Offset(-70, 0));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));
    expect(_path(c), '/');
    expect(find.byKey(kShellHeroHeader), findsOneWidget);
    expect(find.byKey(kShellHeroControls), findsNothing);
    // past the middle: the URL names Markets, laid out under the header while the picture is still leaving
    await finger.moveBy(const Offset(-250, 0));
    await tester.pump();
    expect(_path(c), '/markets');
    expect(find.byType(MarketsScreen), findsOneWidget);
    expect(find.byType(KHeroPicture), findsOneWidget);
    expect(MediaQuery.of(tester.element(find.byType(MarketsScreen))).padding.top, _headerH);
    await finger.up();
    await settle(tester, frames: 6);
    // settled on Markets: the plain header with the module's tabs, the page below it, no picture
    expect(find.byKey(kShellHeroControls), findsNothing);
    expect(find.byType(KHeroPicture), findsNothing);
    expect(tester.widget<KSubNav>(_tabs('Markets')).current, 1);
    final header = find.descendant(of: find.byType(MarketsScreen), matching: find.byType(KPageHeader));
    expect(tester.getTopLeft(header).dy, greaterThanOrEqualTo(_headerH));
    expect(tester.getTopLeft(find.byType(MarketsScreen)).dx, 0);

    // back onto the picture: the hero returns with the controls (the sheet is at the top)
    await _swipe(tester, 320);
    expect(_path(c), '/');
    expect(find.byType(KHeroPicture), findsOneWidget);
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    expect(find.byKey(kShellHeroHeader), findsNothing);
    expect(MediaQuery.of(tester.element(find.byType(DashboardScreen))).padding.top, 0);

    // with the sheet scrolled up (past the collapse point, the greeting under the finger) the header stays up, away
    // and back, and the sheet keeps its scroll
    tester.state<ScrollableState>(_scrollOf(DashboardScreen)).position.jumpTo(340);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(kShellHeroHeader), findsOneWidget);
    await _swipe(tester, -320, y: 180);
    expect(_path(c), '/markets');
    expect(find.byKey(kShellHeroControls), findsNothing);
    await _swipe(tester, 320);
    expect(_path(c), '/');
    expect(find.byKey(kShellHeroHeader), findsOneWidget);
    expect(find.byKey(kShellHeroControls), findsNothing);
    expect(tester.state<ScrollableState>(_scrollOf(DashboardScreen)).position.pixels, 340);
    await unmount(tester);
  });

  testWidgets('Arabic: the pages run right to left, the tab and the URL still match the page', (tester) async {
    final c = await pumpApp(tester, signedIn: true, locale: 'ar');
    expect(Directionality.of(tester.element(find.byType(DashboardScreen))), TextDirection.rtl);
    // the next page lies to the left: a swipe to the left at the first page goes nowhere
    await _swipe(tester, -320, y: 200);
    expect(_path(c), '/');
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    // a swipe to the right brings Markets (الأسواق) in
    await _swipe(tester, 320, y: 200);
    expect(_path(c), '/markets');
    expect(find.byType(MarketsScreen), findsOneWidget);
    expect(tester.getTopLeft(find.byType(MarketsScreen)).dx, 0);
    expect(find.byKey(kShellHeroControls), findsNothing);
    expect(tester.widget<KSubNav>(_tabs('الأسواق')).current, 1);
    expect(c.read(modulePagerOffsetProvider).value, 1);
    expect(MediaQuery.of(tester.element(find.byType(MarketsScreen))).padding.top, _headerH);
    // the tab after it (الأخبار, News) slides on in the same direction
    await _tapTab(tester, 'الأسواق', 'الأخبار', step: 60);
    await settle(tester, frames: 6);
    expect(_path(c), '/news');
    expect(tester.widget<KSubNav>(_tabs('الأسواق')).current, 2);
    // and back to the picture, one page at a time
    await _swipe(tester, -320);
    expect(_path(c), '/markets');
    await _swipe(tester, -320);
    expect(_path(c), '/');
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a deep link opens the Portfolio pager on Ledger, without the pages before it', (tester) async {
    final c = await _open(tester, '/portfolio/ledger');
    expect(_path(c), '/portfolio/ledger');
    expect(find.byType(LedgerScreen), findsOneWidget);
    expect(tester.getTopLeft(find.byType(LedgerScreen)).dx, 0);
    expect(find.byType(PortfolioScreen, skipOffstage: false), findsNothing);
    expect(tester.widget<KSubNav>(_tabs('Ledger')).current, 3);
    expect(c.read(modulePagerOffsetProvider).value, 3);
    // a swipe back goes to Trade history, the page before it
    await _swipe(tester, 320);
    expect(_path(c), '/portfolio/history');
    expect(tester.widget<KSubNav>(_tabs('Ledger')).current, 2);
    await unmount(tester);
  });
}

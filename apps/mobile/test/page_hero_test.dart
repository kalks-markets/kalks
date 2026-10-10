// The section photo heroes (lib/shell/page_hero.dart + the shell's floating controls, founder 2026-10-10): Home opens
// on its photo with only the Ask Kalks AI bar on it (no greeting), the balance strip, the shortcuts and the accounts
// as debit cards in the sheet; every section's first page opens on its own photo with its title; the frosted header
// once the sheet is up; a white-label broker keeps the plain header and no Kalks picture.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/features/accounts/accounts_screen.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/features/wallet/wallet_screen.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/shell/app_shell.dart';
import 'package:kalks/shell/page_hero.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

void main() {
  Finder page() => find.descendant(of: find.byType(DashboardScreen), matching: find.byType(Scrollable)).first;

  /// Scrolls Home until `f` is built, then brings it to the middle of the screen (clear of the bars).
  Future<void> reveal(WidgetTester tester, Finder f) async {
    await tester.scrollUntilVisible(f, 300, scrollable: page());
    await tester.pump();
    await Scrollable.ensureVisible(tester.element(f.first), alignment: 0.4);
    for (var i = 0; i < 3; i++) {
      await tester.pump(const Duration(milliseconds: 300));
    }
  }

  testWidgets('Home opens on its photo with only the AI bar on it; the strip, shortcuts and accounts sit in the sheet', (tester) async {
    await pumpApp(tester, signedIn: true);
    final picture = find.byType(KHeroPicture);
    expect(picture, findsOneWidget);
    expect(tester.widget<KHeroPicture>(picture).photo, KHeroPhoto.dashboard);
    expect(find.byType(KHeroCopy), findsOneWidget);
    // no title, no greeting: the AI bar alone
    expect(find.byKey(const ValueKey('page-hero-title')), findsNothing);
    expect(find.textContaining(RegExp(r'^Good (morning|afternoon|evening)')), findsNothing);
    expect(find.descendant(of: find.byType(KHeroCopy), matching: find.byKey(const ValueKey('ask-ai-pill'))), findsOneWidget);
    // the floating controls, not the frosted header
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    expect(find.byKey(kShellHeroHeader), findsNothing);
    expect(find.text('Trade'), findsOneWidget);
    expect(find.byType(KBrandAvatar), findsOneWidget);
    // the photo starts at the very top (under the status bar), the sheet's edge under it
    expect(tester.getTopLeft(picture).dy, 0);
    final heroH = pageHeroHeight(MediaQuery.of(tester.element(find.byType(DashboardScreen))));
    expect(heroH, closeTo(420.9, 0.01));
    expect(tester.getTopLeft(find.byType(KPillNav)).dy, greaterThan(heroH - kPageHeroOverlap));
    // the sheet: the balance strip with its eye, then the shortcuts and the accounts
    expect(find.byKey(const ValueKey('balance-strip')), findsOneWidget);
    expect(find.text('TOTAL BALANCE'), findsOneWidget);
    expect(find.byKey(const ValueKey('hide-money')), findsOneWidget);
    await reveal(tester, find.byKey(const ValueKey('shortcut-deposit')));
    expect(find.text('Shortcuts'), findsOneWidget);
    await reveal(tester, find.byType(KDebitCard));
    expect(find.text('Your accounts'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('the eye hides every amount on Home and keeps them hidden', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    expect(find.text('••••••'), findsNothing);
    await tester.tap(find.byKey(const ValueKey('hide-money')));
    await settle(tester, frames: 3);
    expect(c.read(hideBalancesProvider), isTrue);
    expect(find.text('••••••'), findsWidgets);
    await tester.tap(find.byKey(const ValueKey('hide-money')));
    await settle(tester, frames: 3);
    expect(c.read(hideBalancesProvider), isFalse);
    await unmount(tester);
  });

  testWidgets('a shortcut opens its page', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    final deposit = find.byKey(const ValueKey('shortcut-deposit'));
    await reveal(tester, deposit);
    await tester.tap(deposit);
    await settle(tester, frames: 4);
    expect(c.read(routerProvider).routerDelegate.currentConfiguration.uri.path, '/wallet/deposit');
    await unmount(tester);
  });

  testWidgets('the pills under the grabber open the module\'s pages', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    final pills = find.byType(KPillNav);
    expect(find.descendant(of: pills, matching: find.text('Markets')), findsOneWidget);
    await tester.tap(find.descendant(of: pills, matching: find.text('Markets')));
    await settle(tester, frames: 4);
    expect(c.read(routerProvider).routerDelegate.currentConfiguration.uri.path, '/markets');
    // a normal page: the frosted header with the module title, no photo
    expect(find.byType(KHeroPicture), findsNothing);
    expect(find.byKey(kShellHeroControls), findsNothing);
    await unmount(tester);
  });

  testWidgets('scrolling the sheet up swaps the floating controls for the frosted header', (tester) async {
    await pumpApp(tester, signedIn: true);
    tester.state<ScrollableState>(page()).position.jumpTo(600);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(kShellHeroHeader), findsOneWidget);
    expect(find.byKey(kShellHeroControls), findsNothing);
    // the header's title and controls
    expect(find.descendant(of: find.byKey(kShellHeroHeader), matching: find.text('Dashboard')), findsOneWidget);
    expect(find.descendant(of: find.byKey(kShellHeroHeader), matching: find.text('Trade')), findsOneWidget);
    expect(find.descendant(of: find.byKey(kShellHeroHeader), matching: find.text('Overview')), findsOneWidget);
    expect(find.byType(KFrosted), findsWidgets);
    // and back
    tester.state<ScrollableState>(page()).position.jumpTo(0);
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    expect(find.byKey(kShellHeroHeader), findsNothing);
    await unmount(tester);
  });

  testWidgets('Accounts and Wallet open on their own photos with their titles', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    c.read(routerProvider).go('/accounts');
    await settle(tester);
    expect(find.byType(AccountsScreen), findsOneWidget);
    expect(tester.widget<KHeroPicture>(find.byType(KHeroPicture)).photo, KHeroPhoto.accounts);
    expect(find.descendant(of: find.byType(KHeroCopy), matching: find.text('Trading accounts')), findsOneWidget);
    expect(find.byKey(kShellHeroControls), findsOneWidget);
    expect(MediaQuery.of(tester.element(find.byType(AccountsScreen))).padding.top, 0);

    c.read(routerProvider).go('/wallet');
    await settle(tester);
    expect(find.byType(WalletScreen), findsOneWidget);
    expect(tester.widget<KHeroPicture>(find.byType(KHeroPicture)).photo, KHeroPhoto.wallet);
    expect(find.descendant(of: find.byType(KHeroCopy), matching: find.text('Wallet')), findsOneWidget);
    // a page beside it: under the header, the section's photo blurred behind it
    c.read(routerProvider).go('/wallet/history');
    await settle(tester);
    expect(find.byKey(kShellHeroControls), findsNothing);
    expect(tester.widget<KBackdrop>(find.byType(KBackdrop)).photo, KHeroPhoto.wallet);
    await unmount(tester);
  });

  testWidgets('a white-label broker keeps the plain header and no Kalks picture', (tester) async {
    final cfg = AppConfig.fromJson(const {
      'apiVersion': 1,
      'urls': {'app': 'https://app.acme.example', 'terminal': 'https://trade.acme.example'},
      'tenant': {'slug': 'acme', 'name': 'Acme Markets', 'default': false, 'primary': '#2f7fd6'},
    });
    await pumpApp(tester, signedIn: true, config: cfg);
    expect(find.byType(KHeroPicture), findsNothing);
    expect(find.byKey(kShellHeroControls), findsNothing);
    expect(find.byKey(kShellHeroHeader), findsNothing);
    expect(find.byType(KPillNav), findsNothing);
    expect(tester.widget<KBackdrop>(find.byType(KBackdrop)).photo, isNull);
    // the header: the broker's initial, the module title, the sub-nav; the AI pill opens the page
    expect(find.text('A'), findsOneWidget);
    expect(find.text('Dashboard'), findsOneWidget);
    expect(find.byType(KSubNav), findsWidgets);
    expect(find.byKey(const ValueKey('ask-ai-pill')), findsOneWidget);
    await unmount(tester);
  });
}

// The shell and sign-in, end to end on the sample-data API: structure, navigation, the sign-in + code flow, and
// golden images in light, dark and Arabic (right to left).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/app.dart';
import 'package:kalks/core/auth/auth_controller.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/shell/more_screen.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

void main() {
  group('login', () {
    // the welcome page's "Log in" pill opens the sign-in form as a sheet
    Future<void> openSignIn(WidgetTester tester) async {
      await tester.tap(find.text('Log in'));
      await settle(tester, frames: 6);
    }

    testWidgets('signs in with the email code from the sign-in sheet, then opens the Client Area', (tester) async {
      final c = await pumpApp(tester);
      expect(find.text('Options on forex, made simple.'), findsOneWidget);
      expect(find.byType(TextField), findsNothing);
      await openSignIn(tester);
      expect(find.text('Welcome back'), findsOneWidget);
      expect(find.text('Email or viewer ID'), findsOneWidget);
      expect(find.text('Forgot password?'), findsOneWidget);

      await tester.enterText(find.byType(TextField).at(0), 'arjun.mehta@example.com');
      await tester.enterText(find.byType(TextField).at(1), 'Secret#2026');
      await tester.tap(find.text('Sign in'));
      await settle(tester);
      // new device: the 6-digit code step
      expect(find.text("Verify it's you"), findsOneWidget);
      await tester.enterText(find.byType(TextField).first, '123456');
      await settle(tester);
      expect(c.read(authProvider), isA<AuthSignedIn>());
      expect(find.byType(DashboardScreen), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('a wrong password shows the error and clears the password', (tester) async {
      await pumpApp(tester);
      await openSignIn(tester);
      await tester.enterText(find.byType(TextField).at(0), 'wrong@example.com');
      await tester.enterText(find.byType(TextField).at(1), 'nope');
      await tester.tap(find.text('Sign in'));
      await settle(tester);
      expect(find.byType(KFormError), findsOneWidget);
      expect((tester.widget(find.byType(TextField).at(1)) as TextField).controller!.text, isEmpty);
      await unmount(tester);
    });

    for (final (name, theme, locale) in [('light', 'light', 'en'), ('dark', 'dark', 'en'), ('arabic', 'light', 'ar')]) {
      testWidgets('golden: login $name', (tester) async {
        await pumpApp(tester, theme: theme, locale: locale);
        if (locale == 'ar') {
          expect(Directionality.of(tester.element(find.byKey(const ValueKey('welcome-title')))), TextDirection.rtl);
        }
        if (locale != 'ar' || hasArabicFont) await expectLater(find.byType(KalksApp), matchesGoldenFile('goldens/login_$name.png'));
        await unmount(tester);
      });
    }
  });

  group('shell', () {
    testWidgets('bottom bar, header and More follow the web navigation', (tester) async {
      await pumpApp(tester, signedIn: true);
      expect(find.byType(DashboardScreen), findsOneWidget);
      // the ink tab bar: icons only, the labels as tooltips
      for (final tab in ['Dashboard', 'Accounts', 'Wallet', 'Portfolio', 'More']) {
        expect(find.byTooltip(tab), findsOneWidget);
      }
      // the controls over the Dashboard's picture: the Trade button; the module's pages as pills in the sheet
      expect(find.text('Trade'), findsWidgets);
      for (final sub in ['Overview', 'Markets', 'News', 'Calendar']) {
        expect(find.text(sub), findsWidgets);
      }
      // Home's sections, in the web's phone order
      final page = find.descendant(of: find.byType(DashboardScreen), matching: find.byType(Scrollable)).first;
      var lastY = double.negativeInfinity;
      for (final s in ['TOTAL BALANCE', 'TOTAL EQUITY', 'Shortcuts', 'Your accounts']) {
        // jump down (no fling) until the section is built and on screen, then measure after a frame
        final pos = tester.state<ScrollableState>(page).position;
        for (var i = 0; i < 40 && find.text(s).hitTestable().evaluate().isEmpty; i++) {
          pos.jumpTo((pos.pixels + 200).clamp(0, pos.maxScrollExtent).toDouble());
          await tester.pump(const Duration(milliseconds: 50));
        }
        final box = tester.getTopLeft(find.text(s).first);
        final scrolled = tester.state<ScrollableState>(page).position.pixels;
        expect(box.dy + scrolled, greaterThan(lastY), reason: s);
        lastY = box.dy + scrolled;
      }
      await tester.drag(page, const Offset(0, 4000));
      await settle(tester, frames: 4);

      await tester.tap(find.byTooltip('More'));
      await settle(tester);
      expect(find.byType(MoreScreen), findsOneWidget);
      final more = find.descendant(of: find.byType(MoreScreen), matching: find.byType(Scrollable)).first;
      for (final m in [
        'Options',
        'Partner (IB)',
        'Copy & PAMM',
        'Prop Challenges',
        'Contests & Rewards',
        'API & Algo',
        'Academy',
        'Profile & Security',
        'Support',
        'Language',
        'Log out',
      ]) {
        await tester.scrollUntilVisible(find.text(m), 150, scrollable: more);
        expect(find.text(m), findsWidgets, reason: m);
      }
      await unmount(tester);
    });

    testWidgets('Arabic mirrors the shell (right to left)', (tester) async {
      await pumpApp(tester, signedIn: true, locale: 'ar');
      final ctx = tester.element(find.byType(DashboardScreen));
      expect(Directionality.of(ctx), TextDirection.rtl);
      // the brand disc sits on the right, the first tab (Dashboard) too
      final brand = tester.getCenter(find.byType(KBrandAvatar).first);
      expect(brand.dx, greaterThan(206));
      await unmount(tester);
    });

    for (final (name, theme, locale) in [('light', 'light', 'en'), ('dark', 'dark', 'en'), ('arabic', 'light', 'ar')]) {
      testWidgets('golden: shell $name', (tester) async {
        await pumpApp(tester, signedIn: true, theme: theme, locale: locale);
        if (locale != 'ar' || hasArabicFont) await expectLater(find.byType(KalksApp), matchesGoldenFile('goldens/shell_$name.png'));
        await unmount(tester);
      });
    }
  });
}

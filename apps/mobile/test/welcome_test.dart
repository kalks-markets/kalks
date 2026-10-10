// The signed-out welcome page (/login): the headline, the two pills and the demo link; "Log in" opens the sign-in
// sheet, a sign-in from it lands on the Dashboard (or on ?next=); a dead session opens the sheet with its notice; a
// white-label broker gets the flat page with its name, without the picture or the demo.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/auth/auth_controller.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/features/auth/login_screen.dart';
import 'package:kalks/features/auth/register_screen.dart';
import 'package:kalks/features/auth/sign_in_sheet.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/features/wallet/wallet_screen.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

void main() {
  const headline = 'Options on forex, made simple.';
  final welcomePhoto = find.byWidgetPredicate((w) => w is Image && w.image is AssetImage && (w.image as AssetImage).assetName == 'assets/heroes/dashboard.jpg');

  Future<void> openSignIn(WidgetTester tester) async {
    await tester.tap(find.text('Log in'));
    await settle(tester, frames: 6);
  }

  Future<void> signInFromSheet(WidgetTester tester) async {
    await openSignIn(tester);
    await tester.enterText(find.byType(TextField).at(0), 'arjun.mehta@example.com');
    await tester.enterText(find.byType(TextField).at(1), 'Secret#2026');
    await tester.tap(find.text('Sign in'));
    await settle(tester);
    expect(find.text("Verify it's you"), findsOneWidget);
    await tester.enterText(find.byType(TextField).first, '123456');
    await settle(tester);
  }

  testWidgets('the welcome page: headline, Log in, Open account and Try the demo over the picture', (tester) async {
    await pumpApp(tester);
    expect(find.byType(LoginScreen), findsOneWidget);
    expect(find.text(headline), findsOneWidget);
    expect(find.text('Log in'), findsOneWidget);
    expect(find.text('Open account'), findsOneWidget);
    expect(find.text('Try the demo'), findsOneWidget);
    expect(find.textContaining('Client Agreement'), findsOneWidget);
    expect(welcomePhoto, findsOneWidget);
    // the form lives in the sheet, not on the page
    expect(find.byType(SignInForm), findsNothing);
    expect(find.text('Welcome back'), findsNothing);
    expect(find.byType(TextField), findsNothing);
    await unmount(tester);
  });

  testWidgets('Log in opens the sign-in sheet with the email field', (tester) async {
    await pumpApp(tester);
    await openSignIn(tester);
    expect(find.byType(SignInForm), findsOneWidget);
    expect(find.byType(KSheetBody), findsOneWidget);
    expect(find.text('Welcome back'), findsOneWidget);
    expect(find.text('Email or viewer ID'), findsOneWidget);
    expect(find.byType(TextField), findsNWidgets(2));
    expect(find.text('Forgot password?'), findsOneWidget);
    // the page stays behind the sheet
    expect(find.text(headline), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a sign-in from the sheet lands on the Dashboard', (tester) async {
    final c = await pumpApp(tester);
    await signInFromSheet(tester);
    expect(c.read(authProvider), isA<AuthSignedIn>());
    expect(find.byType(DashboardScreen), findsOneWidget);
    expect(find.byType(SignInForm), findsNothing);
    expect(c.read(routerProvider).routerDelegate.currentConfiguration.uri.path, '/');
    await unmount(tester);
  });

  testWidgets('a sign-in from the sheet honours ?next=', (tester) async {
    final c = await pumpApp(tester);
    c.read(routerProvider).go('/login?next=%2Fwallet');
    await settle(tester, frames: 4);
    expect(find.byType(LoginScreen), findsOneWidget);
    await signInFromSheet(tester);
    expect(c.read(authProvider), isA<AuthSignedIn>());
    expect(c.read(routerProvider).routerDelegate.currentConfiguration.uri.path, '/wallet');
    expect(find.byType(WalletScreen), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('Open account goes to sign-up', (tester) async {
    await pumpApp(tester);
    await tester.tap(find.text('Open account'));
    await settle(tester, frames: 6);
    expect(find.byType(RegisterScreen), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a dead session opens the sheet at once, with the Signed out notice', (tester) async {
    final c = await pumpApp(tester, signedIn: true);
    expect(find.byType(DashboardScreen), findsOneWidget);
    await c.read(authProvider.notifier).sessionEnded();
    await settle(tester, frames: 8);
    expect(find.byType(LoginScreen), findsOneWidget);
    expect(find.byType(SignInForm), findsOneWidget);
    expect(find.textContaining('Your session ended'), findsOneWidget);
    await unmount(tester);
  });

  testWidgets('a white-label broker: its name on its colour, no picture, no demo', (tester) async {
    final config = AppConfig.fromJson(const {
      'apiVersion': 1,
      'tenant': {'slug': 'zenith', 'name': 'Zenith Markets', 'default': false, 'primary': '#1d4ed8'},
    });
    await pumpApp(tester, config: config);
    expect(find.text('Zenith Markets'), findsOneWidget);
    expect(find.text(headline), findsNothing);
    expect(welcomePhoto, findsNothing);
    expect(find.text('Try the demo'), findsNothing);
    expect(find.text('Log in'), findsOneWidget);
    expect(find.text('Open account'), findsOneWidget);
    expect(find.textContaining('Client Agreement'), findsOneWidget);
    expect(tester.widget<Scaffold>(find.byType(Scaffold).first).backgroundColor, const Color(0xFF1D4ED8));
    // the sheet still opens
    await openSignIn(tester);
    expect(find.byType(SignInForm), findsOneWidget);
    await unmount(tester);
  });
}

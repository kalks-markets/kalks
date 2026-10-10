// The 2026-10-10 redesign (black + Kalks orange, the founder's photo heroes, debit / visiting cards, books) at a phone
// size: 390 x 844 pt at 3x, with a status bar and a home indicator. Run:
//   flutter test test_shots/redesign_shots_test.dart --update-goldens
// The PNGs land in the session scratchpad (app/<name>.png).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/app.dart';
import 'package:kalks/features/auth/unlock_screen.dart';
import 'package:kalks/router/router.dart';

import '../test/helpers/test_app.dart';

const String _dir = '/private/tmp/claude-501/-Users-shivamsingh-Desktop-kalks/8887654c-7bec-49a8-bfdd-05d6e4ed1bce/scratchpad/app';

/// The phone: 390 x 844 pt at 3x, a 47 pt status bar and a 34 pt home indicator.
Future<void> _phone(WidgetTester tester) async {
  tester.view.physicalSize = const Size(1170, 2532);
  tester.view.devicePixelRatio = 3;
  tester.view.padding = const FakeViewPadding(top: 47 * 3, bottom: 34 * 3);
  tester.view.viewPadding = const FakeViewPadding(top: 47 * 3, bottom: 34 * 3);
  addTearDown(tester.view.reset);
  await settle(tester, frames: 6);
}

/// Opens `location` (signed in, or signed out on the welcome page), runs `before`, scrolls the page by `scroll` pt and
/// saves `<_dir>/<name>.png`.
Future<void> _shot(
  WidgetTester tester,
  String name, {
  String? location,
  bool signedIn = true,
  double scroll = 0,
  Future<void> Function(WidgetTester tester)? before,
}) async {
  final c = await pumpApp(tester, signedIn: signedIn);
  await _phone(tester);
  if (location != null) {
    c.read(routerProvider).go(location);
    await settle(tester);
  }
  if (before != null) {
    await before(tester);
    await settle(tester, frames: 8);
  }
  if (scroll > 0) {
    final page = find.byType(Scrollable).hitTestable().first;
    await tester.drag(page, Offset(0, -scroll));
    await settle(tester, frames: 8);
  }
  await expectLater(find.byType(KalksApp), matchesGoldenFile(Uri.file('$_dir/$name.png')));
  await unmount(tester);
}

void main() {
  testWidgets('login', (tester) => _shot(tester, 'login', signedIn: false));
  testWidgets('login sheet', (tester) => _shot(tester, 'login-sheet', signedIn: false, before: (t) => t.tap(find.text('Log in'))));
  testWidgets('register', (tester) => _shot(tester, 'register', signedIn: false, location: '/register'));
  testWidgets('register scrolled', (tester) => _shot(tester, 'register-2', signedIn: false, location: '/register', scroll: 420));
  testWidgets('forgot', (tester) => _shot(tester, 'forgot', signedIn: false, location: '/forgot'));
  testWidgets('home', (tester) => _shot(tester, 'home'));
  testWidgets('home strip', (tester) => _shot(tester, 'home-2', scroll: 330));
  testWidgets('home shortcuts', (tester) => _shot(tester, 'home-3', scroll: 800));
  testWidgets('home accounts', (tester) => _shot(tester, 'home-4', scroll: 1350));
  testWidgets('accounts', (tester) => _shot(tester, 'accounts', location: '/accounts'));
  testWidgets('accounts cards', (tester) => _shot(tester, 'accounts-2', location: '/accounts', scroll: 600));
  testWidgets('accounts types', (tester) => _shot(tester, 'accounts-3', location: '/accounts', scroll: 2300));
  testWidgets('wallet', (tester) => _shot(tester, 'wallet', location: '/wallet'));
  testWidgets('wallet scrolled', (tester) => _shot(tester, 'wallet-2', location: '/wallet', scroll: 420));
  testWidgets('wallet deposit', (tester) => _shot(tester, 'wallet-deposit', location: '/wallet/deposit'));
  testWidgets('open account', (tester) => _shot(tester, 'open-account', location: '/accounts/new'));
  testWidgets('portfolio', (tester) => _shot(tester, 'portfolio', location: '/portfolio'));
  testWidgets('copy', (tester) => _shot(tester, 'copy', location: '/social'));
  testWidgets('academy', (tester) => _shot(tester, 'academy', location: '/academy', scroll: 380));
  testWidgets('academy books', (tester) => _shot(tester, 'academy-2', location: '/academy', scroll: 1150));
  testWidgets('academy certificate', (tester) => _shot(tester, 'academy-cert', location: '/academy/progress', scroll: 2700));
  testWidgets('news', (tester) => _shot(tester, 'news', location: '/news'));
  testWidgets('profile', (tester) => _shot(tester, 'profile', location: '/profile'));
  testWidgets('more', (tester) => _shot(tester, 'more', location: '/more'));
  testWidgets('account detail', (tester) => _shot(tester, 'account-detail', location: '/accounts/10042817'));
  testWidgets('copy masters', (tester) => _shot(tester, 'copy-2', location: '/social', scroll: 1500));
  testWidgets('pamm funds', (tester) => _shot(tester, 'pamm', location: '/social/pamm', scroll: 700));
  testWidgets('partner', (tester) => _shot(tester, 'partner', location: '/partner'));
  testWidgets('partner card', (tester) => _shot(tester, 'partner-2', location: '/partner', scroll: 900));
  testWidgets('prop', (tester) => _shot(tester, 'prop', location: '/prop'));
  // the lock screen over a signed-in session (the lock itself needs a phone's biometrics)
  testWidgets(
    'unlock',
    (tester) => _shot(
      tester,
      'unlock',
      before: (t) async {
        unawaited(t.state<NavigatorState>(find.byType(Navigator).first).push(MaterialPageRoute<void>(builder: (_) => const UnlockScreen())));
      },
    ),
  );
}

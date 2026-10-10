// The in-app demo: "Try the demo" on the sign-in page opens the Dashboard as the sample client on the sample-data
// transport (never the browser, never the network); Log out ends it and clears the flag. The app keeps its own
// transport choice here (sampleTransport: false), so the switch itself is under test.
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/auth/auth_controller.dart';
import 'package:kalks/core/auth/secure_store.dart';
import 'package:kalks/core/prefs.dart';
import 'package:kalks/features/auth/login_screen.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/preview/preview_adapter.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

void main() {
  Future<void> tryTheDemo(WidgetTester tester) async {
    await tester.ensureVisible(find.text('Try the demo'));
    await tester.tap(find.text('Try the demo'));
    await settle(tester);
  }

  testWidgets('Try the demo opens the Dashboard as the sample client, on sample data, in the app', (tester) async {
    final c = await pumpApp(tester, sampleTransport: false);
    expect(find.text('Options on forex, made simple.'), findsOneWidget);
    // signed out: the live transport (Dio's own adapter)
    expect(c.read(demoModeProvider), isFalse);
    expect(c.read(httpAdapterProvider), isNull);

    await tryTheDemo(tester);
    // no email, password or code: straight to the Dashboard of the sample client
    expect(c.read(authProvider), isA<AuthSignedIn>());
    expect(find.byType(DashboardScreen), findsOneWidget);
    // the sample client's money on Home (no greeting since 2026-10-10: the name is in the header's avatar)
    expect(find.text('TOTAL BALANCE'), findsOneWidget);
    expect(find.text('AM'), findsWidgets);
    // the demo strip over the header
    expect(find.text('Demo · Sample data'), findsOneWidget);
    expect(find.text('Exit demo'), findsOneWidget);
    // every request goes to the sample-data adapter, nothing to the network
    expect(c.read(demoModeProvider), isTrue);
    expect(c.read(prefsProvider).demo, isTrue);
    expect(c.read(httpAdapterProvider), isA<PreviewAdapter>());
    expect(c.read(apiProvider).dio.httpClientAdapter, isA<PreviewAdapter>());
    // the session store is seeded like a real sign-in (a restart lands in the demo again)
    final store = c.read(secureStoreProvider) as MemorySecureStore;
    expect(store.values['kalks.session'], contains('preview_'));
    expect(store.values['kalks.user'], contains('arjun.mehta@example.com'));
    await unmount(tester);
  });

  testWidgets('Log out in the demo returns to sign-in and clears the demo', (tester) async {
    final c = await pumpApp(tester, sampleTransport: false);
    await tryTheDemo(tester);
    expect(find.byType(DashboardScreen), findsOneWidget);

    // the profile menu's Log out
    await tester.tap(find.byType(KAvatar).first);
    await settle(tester, frames: 4);
    await tester.tap(find.text('Log out'));
    await settle(tester);

    expect(find.byType(LoginScreen), findsOneWidget);
    expect(find.text('Demo · Sample data'), findsNothing);
    final auth = c.read(authProvider);
    expect(auth, isA<AuthSignedOut>());
    // a plain sign-out, not an expired session
    expect((auth as AuthSignedOut).reason, isNull);
    expect(c.read(demoModeProvider), isFalse);
    expect(c.read(prefsProvider).demo, isFalse);
    expect(c.read(httpAdapterProvider), isNull);
    final store = c.read(secureStoreProvider) as MemorySecureStore;
    expect(store.values.containsKey('kalks.session'), isFalse);
    expect(store.values.containsKey('kalks.user'), isFalse);
    await unmount(tester);
  });

  testWidgets('a restart inside the demo lands on the Dashboard again; Exit demo ends it', (tester) async {
    final c = await pumpApp(tester, demo: true, sampleTransport: false);
    expect(find.byType(DashboardScreen), findsOneWidget);
    expect(find.text('Demo · Sample data'), findsOneWidget);
    expect(c.read(httpAdapterProvider), isA<PreviewAdapter>());

    await tester.tap(find.text('Exit demo'));
    await settle(tester);
    expect(find.byType(LoginScreen), findsOneWidget);
    expect(c.read(demoModeProvider), isFalse);
    expect(c.read(prefsProvider).demo, isFalse);
    await unmount(tester);
  });
}

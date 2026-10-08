// Pumps the real app (router, shell, theme, i18n) on the sample-data API (lib/preview), with in-memory secrets and
// fonts loaded so golden images show real text. Golden files are generated on macOS (`flutter test --update-goldens`);
// other platforms render text slightly differently, so compare goldens on the machine that made them.
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:kalks/app.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/app_info.dart';
import 'package:kalks/core/auth/biometrics.dart';
import 'package:kalks/core/auth/secure_store.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/core/prefs.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/preview/preview_adapter.dart';
import 'package:kalks/preview/preview_data.dart';
import 'package:shared_preferences/shared_preferences.dart';

bool _fontsLoaded = false;
ProviderContainer? _current;

/// A system font with Arabic glyphs, used as the fallback in tests only (never shipped). Null when absent.
const String _arabicFallback = '/System/Library/Fonts/Supplemental/Arial Unicode.ttf';
bool get hasArabicFont => File(_arabicFallback).existsSync();

Future<void> loadFonts() async {
  if (_fontsLoaded) return;
  _fontsLoaded = true;
  Future<void> family(String name, List<String> files) async {
    final loader = FontLoader(name);
    for (final f in files) {
      loader.addFont(rootBundle.load(f));
    }
    await loader.load();
  }

  const w = ['Regular', 'Medium', 'SemiBold', 'Bold'];
  await family('PlusJakartaSans', [
    for (final x in [...w, 'ExtraBold']) 'assets/fonts/PlusJakartaSans-$x.ttf',
  ]);
  await family('Geist', [for (final x in w) 'assets/fonts/Geist-$x.ttf']);
  await family('GeistMono', [for (final x in w) 'assets/fonts/GeistMono-$x.ttf']);
  await family('packages/lucide_icons_flutter/Lucide', ['packages/lucide_icons_flutter/assets/lucide.ttf']);
  if (hasArabicFont) {
    final bytes = File(_arabicFallback).readAsBytesSync();
    // the app's fallback chain is Geist, Roboto, Noto Sans: give "Roboto" the Arabic glyphs
    final loader = FontLoader('Roboto')..addFont(Future.value(ByteData.view(Uint8List.fromList(bytes).buffer)));
    await loader.load();
  }
}

Messages _catalog(String code) => (jsonDecode(File('assets/i18n/$code.json').readAsStringSync()) as Map).cast<String, Object?>();

/// Pumps the app. `signedIn`: a stored session of the sample client; `demo`: the same inside the in-app demo (the
/// `kalks.demo` flag set, as after a restart in it); `theme`: light | dark; `locale`: any of the 22.
/// `sampleTransport`: every call answered by the sample-data adapter (the default); false leaves the app its own
/// choice (the live transport, pointed at a closed local port, or the sample adapter once the demo is on).
/// `config`: a fixed app config instead of the sample API's (a white-label broker, …).
Future<ProviderContainer> pumpApp(
  WidgetTester tester, {
  bool signedIn = false,
  bool demo = false,
  String theme = 'light',
  String locale = 'en',
  String? location,
  bool sampleTransport = true,
  AppConfig? config,
}) async {
  await loadFonts();
  await initializeDateFormatting();
  // a 412 x 915 pt phone at 1x (small golden files; the web-preview screenshots are 2x)
  tester.view.physicalSize = const Size(412, 915);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);

  SharedPreferences.setMockInitialValues({'kalks.theme': theme, 'kalks.locale': locale, 'kalks.biometric.asked': true, if (demo) 'kalks.demo': true});
  final prefs = await Prefs.open();
  final en = _catalog('en');
  final bundle = I18nBundle(locale: locale, english: en, messages: locale == 'en' ? en : _catalog(locale));
  final secrets = MemorySecureStore({
    if (signedIn || demo) 'kalks.session': jsonEncode(previewSession().toJson()),
    if (signedIn || demo) 'kalks.user': jsonEncode(previewMe),
    'kalks.device': 'test-device-0000000000',
  });

  final container = ProviderContainer(
    overrides: [
      prefsProvider.overrideWithValue(prefs),
      i18nBootProvider.overrideWithValue(bundle),
      appInfoProvider.overrideWithValue(const AppInfo(version: '1.0.0', build: '1', osVersion: 'Android 15', model: 'Pixel 8')),
      secureStoreProvider.overrideWithValue(secrets),
      if (sampleTransport)
        httpAdapterProvider.overrideWithValue(PreviewAdapter(latency: Duration.zero))
      else
        apiBaseProvider.overrideWithValue('http://127.0.0.1:9/api/mobile'),
      biometricsProvider.overrideWithValue(const NoBiometrics()),
      if (config != null) configProvider.overrideWith(() => _FixedConfig(config)),
    ],
  );
  _current = container;
  await tester.pumpWidget(UncontrolledProviderScope(container: container, child: const KalksApp()));
  await settle(tester);
  return container;
}

/// Lets the session restore, the sample API, images and SVGs finish (no pumpAndSettle: skeletons animate forever).
Future<void> settle(WidgetTester tester, {int frames = 12}) async {
  for (var i = 0; i < frames; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 20)));
    await tester.pump(const Duration(milliseconds: 100));
  }
  // decode the PNGs and SVGs that are on screen
  await tester.runAsync(() async {
    for (final e in find.byType(Image).evaluate()) {
      await precacheImage((e.widget as Image).image, e);
    }
  });
  await tester.pump(const Duration(milliseconds: 400));
  await tester.pump(const Duration(milliseconds: 400));
}

/// Removes the app and disposes its providers, so their timers and streams stop before the test ends.
Future<void> unmount(WidgetTester tester) async {
  await tester.pumpWidget(const SizedBox.shrink());
  _current?.dispose();
  _current = null;
  await tester.pump(const Duration(seconds: 1));
}

/// Replaces the fixed config of an app pumped with `config:` (a broker switching a module while the app is open).
void setConfig(ProviderContainer container, AppConfig config) => (container.read(configProvider.notifier) as _FixedConfig).set(config);

/// A config that stays as given (no fetch).
class _FixedConfig extends ConfigController {
  _FixedConfig(this.config);
  final AppConfig config;

  @override
  AppConfig build() => config;

  @override
  Future<void> refresh() async {}

  void set(AppConfig c) => state = c;
}

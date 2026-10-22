import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'prefs.dart';

/// The Client Area theme: dark only since 2026-10-10 (the web forces dark: apps/crm/app/layout.tsx forcedTheme), so
/// there is nothing to switch any more; an old `kalks.theme` preference is ignored. Kalks Trader keeps its own theme.
class ThemeController extends Notifier<ThemeMode> {
  @override
  ThemeMode build() => ThemeMode.dark;
}

final themeModeProvider = NotifierProvider<ThemeController, ThemeMode>(ThemeController.new);

class TraderThemeController extends Notifier<ThemeMode> {
  @override
  ThemeMode build() => ref.read(prefsProvider).traderThemeMode;

  Future<void> set(ThemeMode m) async {
    state = m;
    await ref.read(prefsProvider).setTraderThemeMode(m);
  }
}

final traderThemeModeProvider = NotifierProvider<TraderThemeController, ThemeMode>(TraderThemeController.new);

/// Kalks Trader's colour scheme (Account tab › Colours): blue / red (the default, the web terminal's colours) or
/// green / red. Up / buy / profit / TP take the scheme's colour everywhere in the trade room (quotes, buttons, P&L, the
/// chart's candles and lines); down / sell / loss / SL stay red.
enum TraderScheme { blue, green }

class TraderSchemeController extends Notifier<TraderScheme> {
  @override
  TraderScheme build() => ref.read(prefsProvider).traderScheme == 'green' ? TraderScheme.green : TraderScheme.blue;

  Future<void> set(TraderScheme s) async {
    state = s;
    await ref.read(prefsProvider).setTraderScheme(s.name);
  }
}

final traderSchemeProvider = NotifierProvider<TraderSchemeController, TraderScheme>(TraderSchemeController.new);

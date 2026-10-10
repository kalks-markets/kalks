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

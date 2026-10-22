// Small, non-secret preferences on the device (SharedPreferences): language, theme, the biometric switch and a few
// caches. Secrets (the session, trade tokens, the device id) live in the secure store instead (lib/core/auth).
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';

class Prefs {
  Prefs(this._p);
  final SharedPreferences _p;

  static Future<Prefs> open() async => Prefs(await SharedPreferences.getInstance());

  static const _kLocale = 'kalks.locale';
  static const _kTheme = 'kalks.theme';
  static const _kTraderTheme = 'kalks.trader.theme';
  static const _kTraderScheme = 'kalks.trader.scheme';
  static const _kBiometric = 'kalks.biometric';
  static const _kBiometricAsked = 'kalks.biometric.asked';
  static const _kConfig = 'kalks.config';
  static const _kHideBalances = 'kalks.hideBalances';
  static const _kDemo = 'kalks.demo';

  String? get locale => _p.getString(_kLocale);
  Future<void> setLocale(String code) => _p.setString(_kLocale, code);

  /// Client Area theme: light by default, like the web (`light` | `dark` | `system`).
  ThemeMode get themeMode => _mode(_p.getString(_kTheme), ThemeMode.light);
  Future<void> setThemeMode(ThemeMode m) => _p.setString(_kTheme, m.name);

  /// Kalks Trader theme: dark by default, like the web terminal.
  ThemeMode get traderThemeMode => _mode(_p.getString(_kTraderTheme), ThemeMode.dark);
  Future<void> setTraderThemeMode(ThemeMode m) => _p.setString(_kTraderTheme, m.name);

  /// Kalks Trader's up / buy colour: `blue` (default, blue / red) or `green` (green / red).
  String get traderScheme => _p.getString(_kTraderScheme) == 'green' ? 'green' : 'blue';
  Future<void> setTraderScheme(String v) => _p.setString(_kTraderScheme, v == 'green' ? 'green' : 'blue');

  static ThemeMode _mode(String? v, ThemeMode fallback) => ThemeMode.values.firstWhere((m) => m.name == v, orElse: () => fallback);

  /// Biometric unlock switched on (Profile › Preferences, or the offer after sign-in).
  bool get biometricEnabled => _p.getBool(_kBiometric) ?? false;
  Future<void> setBiometricEnabled(bool v) => _p.setBool(_kBiometric, v);

  /// The "unlock faster next time?" offer was shown once already.
  bool get biometricAsked => _p.getBool(_kBiometricAsked) ?? false;
  Future<void> setBiometricAsked(bool v) => _p.setBool(_kBiometricAsked, v);

  /// The last `GET /config` answer, so the app starts with the broker's branding and URLs while offline.
  Map<String, dynamic>? get cachedConfig => _json(_kConfig);
  Future<void> setCachedConfig(Map<String, dynamic> v) => _p.setString(_kConfig, jsonEncode(v));

  bool get hideBalances => _p.getBool(_kHideBalances) ?? false;
  Future<void> setHideBalances(bool v) => _p.setBool(_kHideBalances, v);

  /// The in-app demo is open ("Try the demo" on the sign-in page): kept across restarts until Log out.
  bool get demo => _p.getBool(_kDemo) ?? false;
  Future<void> setDemo(bool v) => _p.setBool(_kDemo, v);

  /// Per-client local event log (toasts kept in the bell, like the web's event log).
  List<Map<String, dynamic>> eventLog(String userKey) {
    final raw = _p.getString('kalks.events.$userKey');
    if (raw == null) return const [];
    try {
      return (jsonDecode(raw) as List).whereType<Map<String, dynamic>>().toList();
    } catch (_) {
      return const [];
    }
  }

  Future<void> setEventLog(String userKey, List<Map<String, dynamic>> events) => _p.setString('kalks.events.$userKey', jsonEncode(events));

  /// A feature's own settings as one JSON object under `kalks.<key>` (Kalks Trader's workspace: favourites, one-click
  /// trading, volume…).
  Map<String, dynamic>? featureJson(String key) => _json('kalks.$key');
  Future<void> setFeatureJson(String key, Map<String, dynamic> v) => _p.setString('kalks.$key', jsonEncode(v));

  Map<String, dynamic>? _json(String key) {
    final raw = _p.getString(key);
    if (raw == null) return null;
    try {
      return jsonDecode(raw) as Map<String, dynamic>;
    } catch (_) {
      return null;
    }
  }

  /// Sign-out: drop what belongs to the client (the language, theme and biometric switch stay with the device).
  Future<void> clearClientData() async {
    for (final k in _p.getKeys().where((k) => k.startsWith('kalks.events.'))) {
      await _p.remove(k);
    }
    await _p.remove(_kHideBalances);
  }
}

/// Overridden in main() with the opened preferences.
final prefsProvider = Provider<Prefs>((ref) => throw UnimplementedError('prefsProvider is set in main()'));

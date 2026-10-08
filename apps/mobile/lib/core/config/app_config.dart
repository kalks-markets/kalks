// The public app config (`GET /api/mobile/config`, docs/MOBILE-API.md §5): stream URLs, the broker's branding, module
// switches, flags and maintenance. Read at start-up (the cached copy first, so the app opens offline with the right
// brand), refreshed in the background and on every resume.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../ui/color_mix.dart';
import '../../ui/theme.dart';
import '../../ui/tokens.dart';
import '../api/api_providers.dart';
import '../prefs.dart';

class AppConfig {
  const AppConfig({
    required this.apiVersion,
    required this.minAppVersion,
    required this.appUrl,
    required this.terminalUrl,
    required this.marketDataHttp,
    required this.marketDataWs,
    required this.engineStream,
    required this.optionsStream,
    required this.supportStream,
    required this.tenantSlug,
    required this.tenantName,
    required this.tenantDefault,
    required this.logoUrl,
    required this.primary,
    required this.accent,
    required this.supportEmail,
    required this.website,
    required this.modules,
    required this.flags,
    required this.maintenance,
    required this.maintenanceMessage,
    required this.maintenanceUntil,
    this.raw = const {},
  });

  final int apiVersion;

  /// When set, an older app must ask the user to update.
  final String? minAppVersion;
  final String appUrl, terminalUrl, marketDataHttp, marketDataWs, engineStream, optionsStream, supportStream;
  final String tenantSlug, tenantName;

  /// The stock Kalks look (no white-label branding).
  final bool tenantDefault;
  final String? logoUrl;

  /// `primary` re-tints the ember accent, `accent` the gold (#rrggbb).
  final Color? primary, accent;
  final String? supportEmail, website;

  /// Module switches: a module set to false is hidden (its API answers 403 module_disabled). Missing means on.
  final Map<String, bool> modules;
  final Map<String, bool> flags;
  final bool maintenance;
  final String maintenanceMessage;
  final DateTime? maintenanceUntil;
  final Map<String, dynamic> raw;

  bool moduleOn(String key) => modules[key] != false;
  bool flag(String key, {bool fallback = false}) => flags[key] ?? fallback;

  KBrand get brand => KBrand(primary: primary ?? kEmber, accent: accent ?? kGold);

  /// The production defaults (used before the first answer and offline without a cached copy).
  static final AppConfig fallback = AppConfig.fromJson(const {
    'apiVersion': 1,
    'urls': {
      'app': 'https://app.kalkstrade.com',
      'terminal': 'https://trade.kalkstrade.com',
      'marketData': {'http': 'https://api.kalkstrade.com', 'ws': 'wss://api.kalkstrade.com/v1/stream'},
      'streams': {
        'engine': 'wss://trade.kalkstrade.com/engine/stream',
        'options': 'wss://trade.kalkstrade.com/options/stream',
        'support': 'wss://app.kalkstrade.com/support/stream',
      },
    },
    'tenant': {'slug': 'kalks', 'name': 'Kalks', 'default': true},
  });

  static AppConfig fromJson(Map<String, dynamic> j) {
    Map<String, dynamic> m(Object? v) => v is Map ? v.cast<String, dynamic>() : const {};
    final urls = m(j['urls']);
    final md = m(urls['marketData']);
    final streams = m(urls['streams']);
    final tenant = m(j['tenant']);
    final maint = m(j['maintenance']);
    Map<String, bool> bools(Object? v) => {for (final e in m(v).entries) e.key: e.value == true};
    String s(Object? v, String fallback) => v is String && v.isNotEmpty ? v : fallback;
    return AppConfig(
      apiVersion: (j['apiVersion'] as num?)?.toInt() ?? 1,
      minAppVersion: j['minAppVersion'] as String?,
      appUrl: s(urls['app'], 'https://app.kalkstrade.com'),
      terminalUrl: s(urls['terminal'], 'https://trade.kalkstrade.com'),
      marketDataHttp: s(md['http'], 'https://api.kalkstrade.com'),
      marketDataWs: s(md['ws'], 'wss://api.kalkstrade.com/v1/stream'),
      engineStream: s(streams['engine'], 'wss://trade.kalkstrade.com/engine/stream'),
      optionsStream: s(streams['options'], 'wss://trade.kalkstrade.com/options/stream'),
      supportStream: s(streams['support'], 'wss://app.kalkstrade.com/support/stream'),
      tenantSlug: s(tenant['slug'], 'kalks'),
      tenantName: s(tenant['name'], 'Kalks'),
      tenantDefault: tenant['default'] != false,
      logoUrl: tenant['logoUrl'] as String?,
      primary: parseHex(tenant['primary'] as String?),
      accent: parseHex(tenant['accent'] as String?),
      supportEmail: tenant['supportEmail'] as String?,
      website: tenant['website'] as String?,
      modules: bools(j['modules']),
      flags: bools(j['flags']),
      maintenance: maint['active'] == true,
      maintenanceMessage: '${maint['message'] ?? ''}',
      maintenanceUntil: DateTime.tryParse('${maint['until']}'),
      raw: j,
    );
  }
}

/// `1.2.3+4` < `1.10.0`: true when `version` is older than `min` (build numbers ignored).
bool isOlderVersion(String version, String? min) {
  if (min == null || min.isEmpty) return false;
  List<int> parts(String v) => v.split(RegExp(r'[+-]')).first.split('.').map((p) => int.tryParse(p) ?? 0).toList();
  final a = parts(version), b = parts(min);
  for (var i = 0; i < 3; i++) {
    final x = i < a.length ? a[i] : 0, y = i < b.length ? b[i] : 0;
    if (x != y) return x < y;
  }
  return false;
}

class ConfigController extends Notifier<AppConfig> {
  Timer? _retry;
  DateTime? _refusedAt;

  @override
  AppConfig build() {
    ref.onDispose(() => _retry?.cancel());
    final cached = ref.read(prefsProvider).cachedConfig;
    Future.microtask(refresh);
    return cached == null ? AppConfig.fallback : AppConfig.fromJson(cached);
  }

  /// An API call answered 403 module_disabled: a module was switched off since the config was read. Fetches it again
  /// (at most every 30 seconds), so the navigation drops the module and the router moves off its page.
  void moduleRefused() {
    final now = DateTime.now();
    if (_refusedAt != null && now.difference(_refusedAt!) < const Duration(seconds: 30)) return;
    _refusedAt = now;
    unawaited(refresh());
  }

  /// Fetches the config (public, no session); keeps the last good copy on failure and retries in a minute.
  Future<void> refresh() async {
    try {
      final j = await ref.read(apiProvider).get<Map<String, dynamic>>('config', auth: false);
      state = AppConfig.fromJson(j);
      await ref.read(prefsProvider).setCachedConfig(j);
    } catch (_) {
      _retry?.cancel();
      _retry = Timer(const Duration(minutes: 1), refresh);
    }
  }
}

final configProvider = NotifierProvider<ConfigController, AppConfig>(ConfigController.new);

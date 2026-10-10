// Profile & Security › Preferences: port of the web page's live branch (apps/crm/app/(app)/profile/preferences/
// page.tsx, `!IS_DEMO`): Language (the 22 locales), the Notifications link card (dark only since 2026-10-10: no
// Appearance switch, like the web). The app adds its own device preference at the end: biometric unlock (shown where
// the phone supports it).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/auth/biometrics.dart';
import '../../core/prefs.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'widgets/profile_ui.dart';

/// Biometric unlock is possible on this phone (false in the web preview and tests).
final biometricAvailableProvider = FutureProvider.autoDispose<bool>((ref) => ref.watch(biometricsProvider).available());

class PreferencesScreen extends ConsumerWidget {
  const PreferencesScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params).
  final Map<String, String> query;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final locale = ref.watch(localeProvider);
    final bio = ref.watch(biometricAvailableProvider).value ?? false;

    Widget language(LocaleInfo l) {
      final on = l.code == locale;
      return KPressable(
        key: ValueKey('lang-${l.code}'),
        semanticLabel: l.name,
        minSize: 36,
        onTap: () => unawaited(ref.read(i18nProvider.notifier).setLocale(l.code)),
        child: Container(
          height: 36,
          padding: const EdgeInsets.symmetric(horizontal: 10),
          decoration: BoxDecoration(color: on ? k.emberSoft : Colors.transparent, borderRadius: BorderRadius.circular(12)),
          child: Row(
            children: [
              KFlag(l.flag, size: 16),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  l.name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.footnote.copyWith(fontSize: 13, color: on ? k.ember : k.fg2),
                ),
              ),
            ],
          ),
        ),
      );
    }

    final rows = <Widget>[];
    for (var i = 0; i < kLocales.length; i += 2) {
      rows.add(
        Padding(
          padding: const EdgeInsets.only(bottom: 6),
          child: Row(
            children: [
              Expanded(child: language(kLocales[i])),
              const SizedBox(width: 6),
              Expanded(child: i + 1 < kLocales.length ? language(kLocales[i + 1]) : const SizedBox.shrink()),
            ],
          ),
        ),
      );
    }

    return KPageScroll(
      children: [
        PPageHeader(title: t('profile.prefs.title'), subtitle: t('profile.prefs.subtitleLive')),
        // language
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              PCardHeader(title: t('common.language'), subtitle: t('profile.prefs.languageCount', {'count': kLocales.length}), icon: LucideIcons.languages),
              const SizedBox(height: 14),
              ConstrainedBox(
                constraints: const BoxConstraints(maxHeight: 224),
                child: SingleChildScrollView(primary: false, child: Column(children: rows)),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        PLinkCard(icon: LucideIcons.bell, title: t('profile.notifCard.title'), text: t('profile.notifCard.hint'), href: '/profile/notifications'),
        // the app's own device preference: biometric unlock
        if (bio) ...[const SizedBox(height: 16), const _BiometricCard()],
      ],
    );
  }
}

class _BiometricCard extends ConsumerStatefulWidget {
  const _BiometricCard();

  @override
  ConsumerState<_BiometricCard> createState() => _BiometricCardState();
}

class _BiometricCardState extends ConsumerState<_BiometricCard> {
  late bool _on = ref.read(prefsProvider).biometricEnabled;

  Future<void> _set(bool v) async {
    // turning it on asks for the biometric once, so the switch is known to work
    if (v && !await ref.read(biometricsProvider).authenticate(context.t('app.unlock.reason'))) return;
    await ref.read(authProvider.notifier).setBiometric(v);
    if (mounted) setState(() => _on = v);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PCardHeader(title: t('security.sessions.thisDevice'), icon: LucideIcons.smartphone),
          const SizedBox(height: 14),
          Row(
            children: [
              Icon(LucideIcons.fingerprint, size: 18, color: k.fg3),
              const SizedBox(width: 10),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(t('app.biometric.title'), style: context.text.label.copyWith(fontSize: 13.5, color: k.fg)),
                    const SizedBox(height: 2),
                    Text(t('app.biometric.text'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)),
                  ],
                ),
              ),
              const SizedBox(width: 10),
              KSwitch(value: _on, semanticLabel: t('app.biometric.title'), onChanged: (v) => unawaited(_set(v))),
            ],
          ),
        ],
      ),
    );
  }
}

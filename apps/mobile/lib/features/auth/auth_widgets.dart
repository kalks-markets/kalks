import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/config/app_config.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import '../common/pickers.dart';

/// The sign-in pages' frame (web AuthSection on phones, 2026-10-10): Home's photo full-bleed at the top, fading into
/// black, the white Kalks logo and the language over it; the form on a black sheet that rises over the photo's foot
/// and scrolls over it (max 460 wide), the risk warning at the end. A white-label broker gets its name over the plain
/// dark backdrop instead of the photo.
class AuthScaffold extends ConsumerWidget {
  const AuthScaffold({super.key, required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final cfg = ref.watch(configProvider);
    final k = context.k;
    final mq = MediaQuery.of(context);
    final kalks = cfg.tenantDefault;
    // the photo band: the KALKS letters and the figure, then the sheet
    final photoH = (mq.size.width * 1240 / 1080).clamp(280.0, mq.size.height * 0.62);
    final sheetTop = kalks ? (photoH * 0.62).clamp(190.0, 340.0) : mq.padding.top + 64;
    return Scaffold(
      backgroundColor: Colors.black,
      resizeToAvoidBottomInset: true,
      body: Stack(
        children: [
          if (kalks)
            Positioned(
              top: 0,
              left: 0,
              right: 0,
              height: photoH,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  Image.asset(KHeroPhoto.dashboard.asset, fit: BoxFit.cover, alignment: Alignment.topCenter, excludeFromSemantics: true),
                  const DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [Color(0x80000000), Color(0x00000000), Color(0x00000000), Color(0xFF000000)],
                        stops: [0, 0.22, 0.62, 1],
                      ),
                    ),
                  ),
                ],
              ),
            )
          else
            const Positioned.fill(child: KBackdrop()),
          // the logo and the language over the photo, then the sheet; both scroll over the photo
          Positioned.fill(
            child: SingleChildScrollView(
              keyboardDismissBehavior: ScrollViewKeyboardDismissBehavior.onDrag,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  SizedBox(
                    height: sheetTop,
                    child: Align(
                      alignment: Alignment.topCenter,
                      child: Padding(
                        padding: EdgeInsets.fromLTRB(20, mq.padding.top + 10, 12, 0),
                        child: Row(
                          children: [
                            if (kalks) const KLogo(color: Colors.white) else Text(cfg.tenantName, style: context.text.title2),
                            const Spacer(),
                            LanguageButton(glass: kalks),
                          ],
                        ),
                      ),
                    ),
                  ),
                  Container(
                    constraints: BoxConstraints(minHeight: mq.size.height - sheetTop),
                    decoration: BoxDecoration(
                      color: Colors.black.withValues(alpha: 0.94),
                      borderRadius: const BorderRadius.vertical(top: Radius.circular(30)),
                      border: Border(top: BorderSide(color: Colors.white.withValues(alpha: 0.1), width: 0.6)),
                    ),
                    padding: EdgeInsets.fromLTRB(20, 28, 20, 20 + mq.padding.bottom),
                    child: Center(
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 460),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.stretch,
                          children: [
                            AnimatedSwitcher(duration: const Duration(milliseconds: 260), switchInCurve: Curves.easeOutCubic, child: child),
                            const SizedBox(height: 28),
                            Text(
                              context.t('common.riskWarning'),
                              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 11, height: 1.5),
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Title + subtitle of an auth step (web h1 text-3xl font-medium + p).
class AuthTitle extends StatelessWidget {
  const AuthTitle({super.key, required this.title, this.subtitle, this.icon});
  final String title;
  final Widget? subtitle;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (icon != null) ...[
          Container(
            width: 48,
            height: 48,
            decoration: BoxDecoration(
              color: k.emberSoft,
              borderRadius: BorderRadius.circular(16),
              border: Border.all(color: k.ember.withValues(alpha: 0.3)),
            ),
            child: Icon(icon, color: k.ember, size: 24),
          ),
          const SizedBox(height: 18),
        ],
        Text(title, style: context.text.largeTitle.copyWith(fontSize: 30, fontWeight: FontWeight.w500)),
        if (subtitle != null) ...[
          const SizedBox(height: 8),
          DefaultTextStyle.merge(
            style: context.text.body.copyWith(color: k.fg2),
            child: subtitle!,
          ),
        ],
      ],
    );
  }
}

/// The password strength bar (web PasswordStrength): 8+ chars, uppercase, number, symbol.
class PasswordStrength extends StatelessWidget {
  const PasswordStrength(this.value, {super.key});
  final String value;

  static int score(String v) =>
      [v.length >= 8, RegExp(r'[A-Z]').hasMatch(v), RegExp(r'[0-9]').hasMatch(v), RegExp(r'[^A-Za-z0-9]').hasMatch(v)].where((x) => x).length;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final s = score(value);
    final labels = [t('auth.strength.tooWeak'), t('auth.strength.weak'), t('auth.strength.fair'), t('auth.strength.good'), t('auth.strength.strong')];
    final color = s < 2 ? k.down : (s < 4 ? k.warn : k.up);
    return Padding(
      padding: const EdgeInsets.only(top: 8),
      child: Column(
        children: [
          Row(
            children: [
              for (var i = 0; i < 4; i++) ...[
                if (i > 0) const SizedBox(width: 6),
                Expanded(
                  child: AnimatedContainer(
                    duration: const Duration(milliseconds: 200),
                    height: 4,
                    decoration: BoxDecoration(color: i < s ? color : k.surface3, borderRadius: BorderRadius.circular(2)),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 6),
          Row(
            children: [
              Expanded(
                child: Text(
                  t('auth.strength.rule'),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ),
              Text(value.isEmpty ? '' : labels[s], style: context.text.caption.copyWith(color: s >= 4 ? k.up : k.fg3)),
            ],
          ),
        ],
      ),
    );
  }
}

/// Countries offered at sign-up (apps/crm/lib/countries.ts): ISO code, name, dial code.
const List<(String, String, String)> kCountries = [
  ('in', 'India', '+91'),
  ('ae', 'United Arab Emirates', '+971'),
  ('sa', 'Saudi Arabia', '+966'),
  ('qa', 'Qatar', '+974'),
  ('kw', 'Kuwait', '+965'),
  ('om', 'Oman', '+968'),
  ('bh', 'Bahrain', '+973'),
  ('eg', 'Egypt', '+20'),
  ('tr', 'Turkey', '+90'),
  ('vn', 'Vietnam', '+84'),
  ('my', 'Malaysia', '+60'),
  ('id', 'Indonesia', '+62'),
  ('th', 'Thailand', '+66'),
  ('ph', 'Philippines', '+63'),
  ('sg', 'Singapore', '+65'),
  ('bd', 'Bangladesh', '+880'),
  ('lk', 'Sri Lanka', '+94'),
  ('np', 'Nepal', '+977'),
  ('ng', 'Nigeria', '+234'),
  ('ke', 'Kenya', '+254'),
  ('za', 'South Africa', '+27'),
  ('br', 'Brazil', '+55'),
  ('mx', 'Mexico', '+52'),
  ('gb', 'United Kingdom', '+44'),
];

/// Latest date of birth for someone who is 18 today.
DateTime maxDob([DateTime? now]) {
  final n = now ?? DateTime.now();
  return DateTime(n.year - 18, n.month, n.day);
}

/// The country list as an iOS sheet.
Future<String?> showCountrySheet(BuildContext context, String current) => showKSheet<String>(
  context,
  title: context.t('auth.field.country'),
  expand: true,
  builder: (ctx) => ListView(
    padding: const EdgeInsets.fromLTRB(16, 4, 16, 16),
    children: [
      KListSection(
        margin: EdgeInsets.zero,
        children: [
          for (final c in kCountries)
            KListRow(
              dense: true,
              leading: KFlag(c.$1, size: 24),
              title: c.$2,
              value: c.$3,
              selected: c.$1 == current,
              chevron: false,
              trailing: c.$1 == current ? Icon(LucideIcons.check, size: 18, color: ctx.k.ember) : null,
              onTap: () => Navigator.of(ctx).pop(c.$1),
            ),
        ],
      ),
    ],
  ),
);

// "More" (web MobileBar's drawer, as an iOS grouped list): every module outside the bottom bar with its pages (tap
// the chevron to show them), then language, appearance, preferences and log out.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/app_info.dart';
import '../core/auth/auth_controller.dart';
import '../core/config/app_config.dart';
import '../env.dart';
import '../features/common/pickers.dart';
import '../i18n/i18n.dart';
import '../ui/ui.dart';
import 'nav.dart';

class MoreScreen extends ConsumerStatefulWidget {
  const MoreScreen({super.key});

  @override
  ConsumerState<MoreScreen> createState() => _MoreScreenState();
}

class _MoreScreenState extends ConsumerState<MoreScreen> {
  String? _open;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final cfg = ref.watch(configProvider);
    final me = ref.watch(meProvider);
    final nav = navFor(cfg, me).where((m) => !kPrimaryModules.contains(m.key)).toList();
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final locale = localeInfo(ref.watch(localeProvider));
    final info = ref.watch(appInfoProvider);

    // modules in the web's sections (main, grow, build, learn, account), one grouped card per section
    final sections = <String, List<NavModule>>{};
    for (final m in nav) {
      (sections[m.section] ??= []).add(m);
    }

    Widget moduleRow(NavModule m) {
      final subs = m.sub.length > 1 ? m.sub : null;
      final expanded = _open == m.key;
      return Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          KListRow(
            leading: KIconTile(icon: m.icon, tone: KTone.neutral, size: 34, radius: 10),
            title: t(m.labelKey),
            chevron: subs == null,
            onTap: () => context.go(m.href),
            trailing: subs == null
                ? null
                : KPressable(
                    onTap: () => setState(() => _open = expanded ? null : m.key),
                    semanticLabel: t(m.labelKey),
                    // 44 pt target, the glyph flush with the plain rows' chevrons
                    child: SizedBox(
                      width: 44,
                      height: 44,
                      child: Align(
                        alignment: AlignmentDirectional.centerEnd,
                        child: AnimatedRotation(
                          duration: const Duration(milliseconds: 200),
                          turns: expanded ? (rtl ? -0.25 : 0.25) : 0,
                          child: Icon(rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 18, color: k.fg3.withValues(alpha: 0.7)),
                        ),
                      ),
                    ),
                  ),
          ),
          AnimatedSize(
            duration: const Duration(milliseconds: 220),
            curve: Curves.easeOutCubic,
            alignment: Alignment.topCenter,
            child: !expanded || subs == null
                ? const SizedBox(width: double.infinity)
                : Container(
                    color: k.surface2.withValues(alpha: 0.6),
                    padding: const EdgeInsetsDirectional.only(start: 46),
                    child: Column(
                      children: [
                        for (final s in subs)
                          KListRow(
                            dense: true,
                            leading: Icon(s.icon, size: 17, color: k.fg3),
                            title: t(s.labelKey),
                            onTap: () => context.go(s.href),
                          ),
                      ],
                    ),
                  ),
          ),
        ],
      );
    }

    return KPageScroll(
      children: [
        KPageHeader(title: t('shell.more')),
        const SizedBox(height: 18),
        for (final group in sections.values) KListSection(children: [for (final m in group) moduleRow(m)]),
        KListSection(
          children: [
            KListRow(
              leading: const Icon(LucideIcons.languages, size: 19),
              title: t('shell.language'),
              value: locale.name,
              onTap: () => showLanguageSheet(context, ref),
            ),
            KListRow(leading: const Icon(LucideIcons.settings, size: 19), title: t('shell.preferences'), onTap: () => context.go('/profile/preferences')),
          ],
        ),
        if (Env.preview)
          KListSection(
            children: [KListRow(leading: const Icon(LucideIcons.palette, size: 19), title: 'Design system', onTap: () => context.go('/more/gallery'))],
          ),
        KListSection(
          children: [
            KListRow(
              leading: Icon(LucideIcons.logOut, size: 19, color: k.down),
              title: t('shell.logOut'),
              destructive: true,
              chevron: false,
              onTap: () => ref.read(authProvider.notifier).logout(),
            ),
          ],
        ),
        Center(
          child: Text(
            t('app.version', {'version': info.fullVersion}),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
        ),
      ],
    );
  }
}

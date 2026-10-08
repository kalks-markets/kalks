// The floating support chat button (port of apps/crm/components/support/launcher.tsx): on every Client Area page
// except /support, above the bottom bar at the end side (web: bottom 92px, end 16px); opens the live chat in a tall
// sheet; a badge with the agent replies that arrived while it was closed (`conversation` frames' clientUnread).
// View-only and read-only sessions have no chat (the web hides the launcher for viewers), nor does a broker that
// switched the live chat off (module `support_chat`).
// CONTRACT used by the shell — keep these names and parameters:
//   SupportLauncher(path:)        the floating button (hidden on /support). Put it in the shell's Stack as a plain
//                                 (non-Positioned) child, outside the page's MediaQuery override: it aligns itself to
//                                 the bottom end, above the tab bar, and only the button takes touches.
//   openSupportChat(context)      opens the same chat sheet (e.g. "Continue in chat" from Ask Kalks AI)
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'live_chat.dart';
import 'support_data.dart';

/// Opens the support chat sheet (the floating chat; also "Continue in chat" from Ask Kalks AI).
Future<void> openSupportChat(BuildContext context) async {
  final container = ProviderScope.containerOf(context, listen: false);
  final me = container.read(meProvider);
  if (me == null || me.readOnly || !container.read(configProvider).moduleOn('support_chat')) return;
  final launcher = container.read(supportLauncherProvider.notifier)..opened();
  try {
    await showKSheet<void>(
      context,
      expand: true,
      builder: (ctx) => LiveChat(variant: LiveChatVariant.sheet, onClose: () => Navigator.of(ctx).pop()),
    );
  } finally {
    launcher.closed();
  }
}

class SupportLauncher extends ConsumerWidget {
  const SupportLauncher({super.key, required this.path, this.bottom});

  /// The current location's path (the button hides on /support).
  final String path;

  /// Distance from the bottom edge (default: above the shell's floating tab bar, like the web's 92 px).
  final double? bottom;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    if (path == '/support' || path.startsWith('/support/')) return const SizedBox.shrink();
    final me = ref.watch(meProvider);
    if (me == null || me.readOnly || !ref.watch(configProvider).moduleOn('support_chat')) return const SizedBox.shrink();
    final t = context.t;
    final k = context.k;
    final s = ref.watch(supportLauncherProvider);
    final safe = MediaQuery.paddingOf(context).bottom;
    final barBottom = safe < 12 ? 12.0 : safe;
    return Align(
      alignment: AlignmentDirectional.bottomEnd,
      child: Padding(
        padding: EdgeInsetsDirectional.only(end: 16, bottom: bottom ?? barBottom + KSize.tabBar + 12),
        child: KPressable(
          key: const ValueKey('support-launcher'),
          onTap: () => openSupportChat(context),
          semanticLabel: s.open ? t('support.launcher.close') : t('support.launcher.open'),
          child: SizedBox(
            width: 56,
            height: 56,
            child: Stack(
              clipBehavior: Clip.none,
              children: [
                Container(
                  width: 56,
                  height: 56,
                  alignment: Alignment.center,
                  decoration: BoxDecoration(
                    color: k.ember,
                    shape: BoxShape.circle,
                    boxShadow: [
                      BoxShadow(color: Colors.black.withValues(alpha: 0.45), offset: const Offset(0, 12), blurRadius: 30, spreadRadius: -12),
                      BoxShadow(color: k.ember.withValues(alpha: 0.4), offset: const Offset(0, 10), blurRadius: 22, spreadRadius: -12),
                    ],
                  ),
                  child: Icon(s.open ? LucideIcons.x : LucideIcons.messageCircle, size: 20, color: k.onEmber),
                ),
                if (!s.open && s.unread > 0)
                  PositionedDirectional(
                    top: -2,
                    end: -2,
                    child: Container(
                      key: const ValueKey('support-launcher-badge'),
                      constraints: const BoxConstraints(minWidth: 20),
                      height: 20,
                      padding: const EdgeInsets.symmetric(horizontal: 4),
                      alignment: Alignment.center,
                      decoration: BoxDecoration(
                        color: k.fg,
                        borderRadius: BorderRadius.circular(10),
                        border: Border.all(color: k.bg, width: 2),
                      ),
                      child: Text(
                        s.unread > 99 ? '99+' : '${s.unread}',
                        style: context.text.micro.copyWith(color: k.bg, fontSize: 10.5, height: 1, fontFeatures: kTabular),
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

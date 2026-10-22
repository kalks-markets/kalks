// Account tab (web MAccount, owner design 2026-10): the colour scheme (Blue / red, the default, or Green / red, with
// a SELL / BUY preview each; it repaints the whole trade room and the chart), the accounts (one tap switches; add an
// account login), one-click trading, advanced quotes, sound on fills, dark theme, language, demo refill, back to the
// Client Area, log out of this account.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/auth/auth_controller.dart';
import '../../../core/notifications/notifications.dart';
import '../../../core/theme_controller.dart';
import '../../../features/common/pickers.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/sessions.dart';
import '../core/terminal_controller.dart';
import '../core/trade_actions.dart';
import '../core/workspace.dart';
import '../widgets/kit.dart';
import 'account_sheets.dart';
import 'room.dart';

class AccountTab extends ConsumerWidget {
  const AccountTab({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final ws = ref.watch(workspaceProvider);
    final acc = ref.watch(terminalProvider.select((s) => s.account));
    final dark = k.dark;
    final lang = kLocales.where((l) => l.code == t.locale).firstOrNull;
    Widget row(String label, IconData? icon, Widget trailing, {VoidCallback? onTap}) => KPressable(
      pressedScale: 1,
      onTap: onTap,
      child: Container(
        height: 50,
        padding: const EdgeInsets.symmetric(horizontal: 14),
        decoration: BoxDecoration(
          border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
        ),
        child: Row(
          children: [
            SizedBox(width: 22, child: icon == null ? null : Icon(icon, size: 16, color: k.fg3)),
            const SizedBox(width: 6),
            Expanded(
              child: Text(label, style: context.text.callout.copyWith(fontSize: 13.5, color: k.fg)),
            ),
            trailing,
          ],
        ),
      ),
    );
    final scheme = ref.watch(traderSchemeProvider);
    Widget label(String text) => Padding(
      padding: const EdgeInsets.fromLTRB(4, 18, 4, 8),
      child: Text(
        text.toUpperCase(),
        style: context.text.micro.copyWith(color: k.fg3, fontSize: 11, letterSpacing: 0.6),
      ),
    );
    return ListView(
      padding: const EdgeInsets.fromLTRB(kRoomGutter, 0, kRoomGutter, 20),
      children: [
        label(t('app.trader.colors.title')),
        Row(
          children: [
            for (final s in TraderScheme.values) ...[
              if (s != TraderScheme.values.first) const SizedBox(width: 10),
              Expanded(
                child: SchemeCard(
                  scheme: s,
                  selected: s == scheme,
                  onTap: () {
                    KHaptics.selection();
                    unawaited(ref.read(traderSchemeProvider.notifier).set(s));
                  },
                ),
              ),
            ],
          ],
        ),
        label(t('common.accounts')),
        TPanel(
          child: Column(
            children: [
              const AccountList(),
              row(
                t('trader.account.logInAnother'),
                LucideIcons.userPlus,
                Icon(LucideIcons.chevronRight, size: 16, color: k.fg3),
                onTap: () => unawaited(showAddAccountSheet(context)),
              ),
            ],
          ),
        ),
        label(t('desk.set.trading')),
        TPanel(
          child: Column(
            children: [
              row(
                t('trader.oneClick.name'),
                LucideIcons.zap,
                KSwitch(
                  value: ws.oneClick,
                  semanticLabel: t('trader.oneClick.name'),
                  onChanged: (v) => ref.read(workspaceProvider.notifier).update((w) => w.copyWith(oneClick: v)),
                ),
              ),
              row(
                t('app.trader.settings.advancedQuotes'),
                LucideIcons.listTree,
                KSwitch(
                  value: ws.advancedQuotes,
                  semanticLabel: t('app.trader.settings.advancedQuotes'),
                  onChanged: (v) => ref.read(workspaceProvider.notifier).update((w) => w.copyWith(advancedQuotes: v)),
                ),
              ),
              row(
                t('trader.mobile.soundOnFills'),
                LucideIcons.volume2,
                KSwitch(
                  value: ws.sound,
                  semanticLabel: t('trader.mobile.sound'),
                  onChanged: (v) => ref.read(workspaceProvider.notifier).update((w) => w.copyWith(sound: v)),
                ),
              ),
              row(
                t('trader.mobile.darkTheme'),
                dark ? LucideIcons.moon : LucideIcons.sun,
                KSwitch(
                  value: dark,
                  semanticLabel: t('trader.menu.theme'),
                  onChanged: (v) => ref.read(traderThemeModeProvider.notifier).set(v ? ThemeMode.dark : ThemeMode.light),
                ),
              ),
              row(
                t('common.language'),
                LucideIcons.languages,
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(lang?.name ?? t.locale, style: context.text.callout.copyWith(color: k.fg3, fontSize: 13)),
                    Icon(LucideIcons.chevronRight, size: 16, color: k.fg3),
                  ],
                ),
                onTap: () => unawaited(showLanguageSheet(context, ref)),
              ),
              if (acc != null && acc.demo)
                row(
                  t('trader.account.refillDemo', {'count': acc.refillsLeft ?? 0}),
                  LucideIcons.refreshCw,
                  KPressable(
                    minSize: 32,
                    onTap: () => unawaited(ref.read(tradeActionsProvider).refillDemo()),
                    child: Container(
                      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 4),
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.circular(6),
                        border: Border.all(color: k.gold.withValues(alpha: 0.4)),
                      ),
                      child: Text(t('trader.mobile.refill'), style: context.text.caption.copyWith(color: k.gold)),
                    ),
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 12),
        KButton(
          label: t('trader.clientArea'),
          icon: LucideIcons.wallet,
          trailingIcon: LucideIcons.arrowUpRight,
          variant: KButtonVariant.surface,
          expand: true,
          onPressed: () => context.canPop() ? context.pop() : context.go('/'),
        ),
        const SizedBox(height: 8),
        KButton(
          label: t('trader.menu.logOut'),
          icon: LucideIcons.logOut,
          variant: KButtonVariant.danger,
          expand: true,
          onPressed: acc == null
              ? null
              : () async {
                  // the in-app demo ends with its Log out: back to the sign-in page, not just out of the sample account
                  if (ref.read(demoModeProvider)) return ref.read(authProvider.notifier).logout();
                  final login = acc.login;
                  await ref.read(tradeSessionsProvider.notifier).logout(login);
                  ref
                      .read(notificationsProvider.notifier)
                      .toast(NotificationKind.neutral, t('trader.toast.loggedOut'), description: t('order.toast.loggedOut', {'login': login}));
                  if (ref.read(tradeSessionsProvider).current == null && context.mounted) {
                    context.canPop() ? context.pop() : context.go('/');
                  }
                },
        ),
      ],
    );
  }
}

/// A colour scheme's preview card (owner design 2026-10): its name and a SELL / BUY pair in its colours; the chosen
/// one has an ember border.
class SchemeCard extends StatelessWidget {
  const SchemeCard({super.key, required this.scheme, required this.selected, required this.onTap});
  final TraderScheme scheme;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final green = scheme == TraderScheme.green;
    // the scheme's own fills, whatever the current theme
    final tokens = k.dark ? KTokens.traderDark(green: green) : KTokens.traderLight(green: green);
    Widget pill(String text, Color c) => Expanded(
      child: Container(
        height: 30,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: c, borderRadius: BorderRadius.circular(15)),
        child: Text(
          text.toUpperCase(),
          style: context.text.micro.copyWith(fontSize: 11, fontWeight: FontWeight.w800, color: Colors.white),
        ),
      ),
    );
    return Semantics(
      selected: selected,
      child: KPressable(
        key: ValueKey('scheme-${scheme.name}'),
        semanticLabel: t(green ? 'app.trader.colors.green' : 'app.trader.colors.blue'),
        onTap: onTap,
        child: Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            color: k.surface,
            borderRadius: BorderRadius.circular(18),
            border: Border.all(color: selected ? k.ember : k.surface3, width: 2),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                t(green ? 'app.trader.colors.green' : 'app.trader.colors.blue'),
                style: context.text.label.copyWith(fontSize: 13, fontWeight: FontWeight.w700, color: k.fg),
              ),
              const SizedBox(height: 10),
              Row(
                children: [
                  pill(t('common.sell'), tokens.sellFill),
                  const SizedBox(width: 6),
                  pill(t('common.buy'), tokens.buyFill),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }
}

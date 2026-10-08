// The account actions shared by the Dashboard, My accounts, the account detail and the Wallet (port of
// apps/crm/components/trading/ui.tsx AccountActions / TradeButton / FundButton / RefillButton, extras.tsx
// setDefaultAccount / TransferBetweenDialog, archive.tsx RestoreButton and api.ts downloadExport). CONTRACT used by
// other screens — keep these names and parameters:
//   showAccountMenu(context, ref, account, onChanged:)    the full ⋯ menu (action sheet + every dialog)
//   AccountMenuButton(account:, onChanged:)               the round ⋯ button that opens it
//   TradeButton(account:, size:, expand:)                 opens Kalks Trader (/trader?login=)
//   FundButton(account:, size:)                           Fund: wallet -> this live account (nothing when the broker
//                                                         switched the wallet module off)
//   RefillButton(account:, onDone:, size:)                demo refill
//   showTransferBetweenSheet(context, from:)              move money between two trading accounts (step-up)
// Optional extras: showAccountMenu(onTab:) switches the detail page's tab instead of opening the detail again;
// TradeButton(label:, variant:), FundButton(variant:); exportAccountCsv / downloadHistoryZip / refillDemo /
// restoreAccount / setDefaultAccount for other screens.
//
// The ⋯ menu offers what the web menu offers for the account (accountMenuItems in accounts_data.dart): details,
// default star, leverage, account type, move to another account, demo balance, passwords, statements, the history
// ZIP, rename, delete (archive, live: emailed code) and close permanently (survey + emailed code); an archived account
// gets statements, the ZIP and Restore; view-only and read-only staff sessions get none of the changes.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/files.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/notifications/notifications.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'accounts_data.dart';
import 'widgets/account_bits.dart';
import 'widgets/account_sheets.dart';

/// The ⋯ menu entry's label and icon (the web menu's).
({String label, IconData icon, bool destructive}) accountMenuEntry(T t, EngineAccount a, AccountMenuItem item) => switch (item) {
  AccountMenuItem.details => (label: t('accounts.menu.details'), icon: LucideIcons.gauge, destructive: false),
  AccountMenuItem.defaultStar => (
    label: a.isDefault ? t('accounts.default.unset') : t('accounts.default.makeDefault'),
    icon: a.isDefault ? LucideIcons.starOff : LucideIcons.star,
    destructive: false,
  ),
  AccountMenuItem.leverage => (label: t('accounts.menu.changeLeverage'), icon: LucideIcons.gauge, destructive: false),
  AccountMenuItem.changeType => (label: t('accounts.type.menu'), icon: LucideIcons.layers, destructive: false),
  AccountMenuItem.moveBetween => (label: t('accounts.between.menu'), icon: LucideIcons.arrowLeftRight, destructive: false),
  AccountMenuItem.demoBalance => (label: t('accounts.demoBalance.menu'), icon: LucideIcons.coins, destructive: false),
  AccountMenuItem.passwords => (label: t('accounts.menu.passwords'), icon: LucideIcons.keyRound, destructive: false),
  AccountMenuItem.statements => (label: t('accounts.menu.statements'), icon: LucideIcons.arrowDownToLine, destructive: false),
  AccountMenuItem.historyZip => (label: t('accounts.history.zip'), icon: LucideIcons.arrowDownToLine, destructive: false),
  AccountMenuItem.rename => (label: t('accounts.menu.rename'), icon: LucideIcons.pencilLine, destructive: false),
  AccountMenuItem.delete => (label: t('accounts.menu.delete'), icon: LucideIcons.trash2, destructive: true),
  AccountMenuItem.close => (label: t('accounts.close.menu'), icon: LucideIcons.lock, destructive: true),
  AccountMenuItem.restore => (label: t('accounts.archived.restore'), icon: LucideIcons.rotateCcw, destructive: false),
};

/// Opens the account's ⋯ menu. Resolves when the menu (and any dialog it opened) is closed. `onChanged` runs after a
/// change (the caller reloads; every account list and detail is refreshed as well). `onTab` (the detail page) opens
/// one of its tabs (overview | settings | credentials | history) instead of pushing the detail again.
Future<void> showAccountMenu(BuildContext context, WidgetRef ref, EngineAccount account, {VoidCallback? onChanged, ValueChanged<String>? onTab}) async {
  final t = context.t;
  final a = account;
  final readOnly = ref.read(meProvider)?.readOnly ?? false;
  final items = accountMenuItems(a, readOnly: readOnly);
  if (items.isEmpty) return;
  final container = ProviderScope.containerOf(context, listen: false);
  final choice = await showKActionSheet<AccountMenuItem>(
    context,
    title: '${a.groupName} · #${a.login}',
    actions: [
      for (final item in items)
        () {
          final e = accountMenuEntry(t, a, item);
          return KAction(label: e.label, value: item, icon: e.icon, destructive: e.destructive);
        }(),
    ],
  );
  if (choice == null || !context.mounted) return;
  void changed() {
    refreshAccountData(container);
    onChanged?.call();
  }

  void tab(String key) {
    if (onTab != null) {
      onTab(key);
    } else {
      unawaited(GoRouter.of(context).push('/accounts/${a.login}${key == 'overview' ? '' : '?tab=$key'}'));
    }
  }

  switch (choice) {
    case AccountMenuItem.details:
      tab('overview');
    case AccountMenuItem.defaultStar:
      if (await setDefaultAccount(context, a)) changed();
    case AccountMenuItem.leverage:
      tab('settings');
    case AccountMenuItem.changeType:
      if (await showChangeTypeSheet(context, a)) changed();
    case AccountMenuItem.moveBetween:
      if (await showTransferBetweenSheet(context, from: a)) onChanged?.call();
    case AccountMenuItem.demoBalance:
      if (await showDemoBalanceSheet(context, a)) changed();
    case AccountMenuItem.passwords:
      tab('credentials');
    case AccountMenuItem.statements:
      tab('history');
    case AccountMenuItem.historyZip:
      await downloadHistoryZip(context, a.login);
    case AccountMenuItem.rename:
      if (await showRenameSheet(context, a)) changed();
    case AccountMenuItem.delete:
      await showDeleteAccountSheet(context, a);
      changed();
    case AccountMenuItem.close:
      await showCloseAccountSheet(context, a);
      changed();
    case AccountMenuItem.restore:
      if (await restoreAccount(context, a)) changed();
  }
}

/// The round ⋯ button of an account row / card (hidden for view-only and read-only staff sessions).
class AccountMenuButton extends ConsumerWidget {
  const AccountMenuButton({super.key, required this.account, this.onChanged, this.onTab, this.size = KSize.iconButton});
  final EngineAccount account;
  final VoidCallback? onChanged;
  final ValueChanged<String>? onTab;
  final double size;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    if (accountMenuItems(account, readOnly: readOnly).isEmpty) return const SizedBox.shrink();
    return KIconButton(
      icon: LucideIcons.ellipsis,
      filled: true,
      size: size,
      semanticLabel: context.t('accounts.menu.actions'),
      onPressed: () => showAccountMenu(context, ref, account, onChanged: onChanged, onTab: onTab),
    );
  }
}

/// Trade: opens Kalks Trader on this account (off for disabled / expired / archived / closed accounts).
class TradeButton extends StatelessWidget {
  const TradeButton({super.key, required this.account, this.size = KButtonSize.md, this.expand = false, this.label, this.variant = KButtonVariant.ember});
  final EngineAccount account;
  final KButtonSize size;
  final bool expand;

  /// "Open in Kalks Trader", "Watch P&L", "Open", … (default: Trade).
  final String? label;
  final KButtonVariant variant;

  @override
  Widget build(BuildContext context) => KButton(
    label: label ?? context.t('accounts.row.trade'),
    icon: LucideIcons.candlestickChart,
    size: size,
    expand: expand,
    variant: variant,
    onPressed: account.tradeBlocked ? null : () => context.push('/trader?login=${account.login}'),
  );
}

/// Fund: move money from the wallet to this live account (web FundButton: the Fund dialog points to Deposit USDT and
/// Wallet › Transfer with this account chosen).
class FundButton extends ConsumerWidget {
  const FundButton({super.key, required this.account, this.size = KButtonSize.md, this.variant = KButtonVariant.surface, this.expand = false});
  final EngineAccount account;
  final KButtonSize size;
  final KButtonVariant variant;
  final bool expand;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // funding comes from the wallet: none without the wallet module
    if (!ref.watch(configProvider).moduleOn('wallet')) return const SizedBox.shrink();
    return KButton(
      label: context.t('accounts.fund.button'),
      icon: LucideIcons.arrowDownToLine,
      variant: variant,
      size: size,
      expand: expand,
      onPressed: () => showFundSheet(context, account),
    );
  }
}

/// Refill: tops a demo account back up to its starting balance (off when none are left today, the balance is full or
/// the account expired).
class RefillButton extends ConsumerStatefulWidget {
  const RefillButton({super.key, required this.account, this.onDone, this.size = KButtonSize.md, this.label});
  final EngineAccount account;
  final VoidCallback? onDone;
  final KButtonSize size;
  final String? label;

  @override
  ConsumerState<RefillButton> createState() => _RefillButtonState();
}

class _RefillButtonState extends ConsumerState<RefillButton> {
  bool _busy = false;

  Future<void> _run() async {
    setState(() => _busy = true);
    final ok = await refillDemo(context, widget.account);
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) widget.onDone?.call();
  }

  @override
  Widget build(BuildContext context) => KButton(
    label: widget.label ?? context.t('accounts.row.refill'),
    icon: LucideIcons.refreshCcw,
    variant: KButtonVariant.surface,
    size: widget.size,
    loading: _busy,
    onPressed: _busy || refillDisabled(widget.account) ? null : _run,
  );
}

/// Move money between two of the client's trading accounts (step-up `internal_transfer`, target: the from-login).
/// Resolves true when a transfer was made.
Future<bool> showTransferBetweenSheet(BuildContext context, {EngineAccount? from}) async {
  final container = ProviderScope.containerOf(context, listen: false);
  final done = await showKSheet<bool>(
    context,
    title: context.t('accounts.between.title'),
    builder: (_) => TransferBetweenSheet(from: from),
  );
  if (done == true) refreshAccountData(container);
  return done == true;
}

/* ------------------------------------------------------------------ single requests */

/// `POST trading/accounts/{login}/demo-refill` with the web's toasts. Resolves true when refilled.
Future<bool> refillDemo(BuildContext context, EngineAccount a) async {
  final t = context.t;
  final container = ProviderScope.containerOf(context, listen: false);
  try {
    final r = await container.read(apiProvider).post<Map<String, dynamic>>('trading/accounts/${a.login}/demo-refill');
    final balance = (r['balance'] as num?)?.toDouble() ?? demoTarget(a) ?? a.balance;
    if (context.mounted) {
      accountToast(
        context,
        NotificationKind.success,
        t('accounts.refill.done'),
        description: t('accounts.refill.desc', {
          'login': a.login,
          'amount': '${a.currencyPrefix}${Fmt.number(balance)}',
          'count': (refillsLeft(a) - 1).clamp(0, 99),
        }),
      );
    }
    refreshAccountData(container);
    return true;
  } catch (e) {
    if (context.mounted) accountErrorToast(context, t('accounts.refill.error'), e);
    return false;
  }
}

/// Star / unstar the default account (`POST trading/prefs {defaultLogin}`). Resolves true when changed.
Future<bool> setDefaultAccount(BuildContext context, EngineAccount a) async {
  final t = context.t;
  try {
    await apiOf(context).post<Map<String, dynamic>>('trading/prefs', body: {'defaultLogin': a.isDefault ? null : a.login});
    if (context.mounted) {
      accountToast(context, NotificationKind.success, a.isDefault ? t('accounts.default.removed') : t('accounts.default.set'), description: '#${a.login}');
    }
    return true;
  } catch (e) {
    if (context.mounted) accountErrorToast(context, t('accounts.default.failed'), e);
    return false;
  }
}

/// `POST trading/accounts/{login}/restore` (web RestoreButton). Resolves true when restored.
Future<bool> restoreAccount(BuildContext context, EngineAccount a) async {
  final t = context.t;
  try {
    await apiOf(context).post<Map<String, dynamic>>('trading/accounts/${a.login}/restore');
    if (context.mounted) {
      accountToast(context, NotificationKind.success, t('accounts.archived.restored'), description: t('accounts.archived.restoredDesc', {'login': a.login}));
    }
    return true;
  } catch (e) {
    if (context.mounted) accountErrorToast(context, t('accounts.archived.restoreFailed'), e);
    return false;
  }
}

/// The full history ZIP (`GET trading/accounts/{login}/history-zip`: statement PDF + CSV + Excel) to the share sheet.
Future<void> downloadHistoryZip(BuildContext context, int login) async {
  final t = context.t;
  try {
    final f = await apiOf(context).download('trading/accounts/$login/history-zip');
    if (!await shareFile(f, fallbackName: '$login-history.zip') && context.mounted) {
      accountToast(context, NotificationKind.error, t('accounts.toast.exportFailed'));
    }
  } catch (e) {
    if (context.mounted) accountErrorToast(context, t('accounts.toast.exportFailed'), e);
  }
}

/// A trades / ledger CSV (`GET trading/accounts/{login}/export?kind=history|ledger&from&to&instrument`) to the share
/// sheet, with the web's toasts (api.ts downloadExport).
Future<void> exportAccountCsv(BuildContext context, int login, String kind, {String? from, String? to, String instrument = 'all'}) async {
  final t = context.t;
  final description = t('accounts.toast.exportDesc', {'login': login, 'kind': t(kind == 'history' ? 'accounts.export.trades' : 'accounts.export.ledger')});
  try {
    final f = await apiOf(context).download(
      'trading/accounts/$login/export',
      query: {'kind': kind, 'from': ?from, 'to': ?to, if (kind == 'history' && instrument != 'all') 'instrument': instrument},
    );
    final shared = await shareFile(f, fallbackName: '$login-$kind.csv');
    if (!context.mounted) return;
    if (shared) {
      accountToast(context, NotificationKind.success, t('accounts.toast.exportStarted'), description: description);
    } else {
      accountToast(context, NotificationKind.error, t('accounts.toast.exportFailed'), description: description);
    }
  } catch (e) {
    if (context.mounted) {
      accountToast(context, NotificationKind.error, t('accounts.toast.exportFailed'), description: e is ApiException ? localizeError(e, t) : description);
    }
  }
}

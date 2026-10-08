// Accounts of Kalks Trader: the list used by the Account tab and the header's switcher (web MAccountRow: Live / Demo,
// login, group · mode · leverage, equity; one tap switches, opening the client's own account if needed), grouped by
// product (CFD accounts, Options accounts with an OPTIONS tag; Options hidden while the module is off), and the
// MT5-style "Login to trade account" sheet (web EngineLoginForm: login, password, server; the investor password opens
// a read-only session).
import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_error.dart';
import '../../../core/config/app_config.dart';
import '../../../core/models/account.dart';
import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/sessions.dart';
import '../core/terminal_controller.dart';
import '../core/trade_math.dart';
import '../terminal_screen.dart';
import '../widgets/kit.dart';

/// One row of the account list: what is known about a login (its live session, else the Client Area's record).
class AccountEntry {
  const AccountEntry({
    required this.login,
    required this.live,
    required this.group,
    required this.mode,
    required this.leverage,
    required this.equity,
    required this.cent,
    this.readOnly = false,
    this.product = 'cfd',
  });
  final String login;
  final bool live;
  final String group, mode;
  final int leverage;

  /// USD.
  final double equity;
  final bool cent;
  final bool readOnly;

  /// cfd | options.
  final String product;

  bool get isOptions => product == 'options';
}

/// The client's own accounts (not archived) plus the logins added with a password, in that order.
final accountEntriesProvider = Provider.autoDispose<List<AccountEntry>>((ref) {
  final own = ref.watch(ownAccountsProvider).value ?? const <EngineAccount>[];
  final sessions = ref.watch(tradeSessionsProvider.select((s) => s.sessions));
  final out = <AccountEntry>[];
  final seen = <String>{};
  for (final a in own) {
    final l = '${a.login}';
    final s = sessions[l];
    final sa = s?.account;
    seen.add(l);
    out.add(
      AccountEntry(
        login: l,
        live: a.live,
        group: sa?.group ?? a.groupName,
        mode: sa?.mode ?? a.mode,
        leverage: sa?.leverage ?? a.leverage,
        equity: sa?.equity ?? (a.cent ? a.equity / 100 : a.equity),
        cent: a.cent,
        readOnly: s?.readOnly ?? false,
        product: sa?.product ?? a.product,
      ),
    );
  }
  for (final s in sessions.values) {
    if (seen.contains(s.login)) continue;
    final a = s.account;
    out.add(
      AccountEntry(
        login: s.login,
        live: a?.live ?? true,
        group: a?.group ?? '',
        mode: a?.mode ?? 'hedging',
        leverage: a?.leverage ?? 0,
        equity: a?.equity ?? 0,
        cent: a?.cent ?? false,
        readOnly: s.readOnly,
        product: a?.product ?? 'cfd',
      ),
    );
  }
  return out;
});

/// Shows `login` in the terminal (opening the client's own account if needed), with the web's "Switched to …" toast
/// or the reason it couldn't.
Future<bool> switchTradeAccount(BuildContext context, String login, {bool? live}) async {
  final t = context.t;
  final c = ProviderScope.containerOf(context, listen: false);
  final ok = await c.read(tradeSessionsProvider.notifier).activate(login);
  final notes = c.read(notificationsProvider.notifier);
  if (ok) {
    final isLive = live ?? c.read(tradeSessionsProvider).sessions[login]?.account?.live ?? true;
    notes.toast(NotificationKind.info, t(isLive ? 'order.toast.switchedLive' : 'order.toast.switchedDemo', {'login': login}), keep: false);
  } else {
    final err = c.read(tradeSessionsProvider).error;
    if (err != null) notes.toast(NotificationKind.error, localizeTradeLogin(err, t), keep: false);
  }
  return ok;
}

/// The account rows (web MAccountRow), by product: CFD accounts, then Options accounts (OPTIONS tag) with "Open an
/// Options account" when there is none; the Options section is hidden while the broker has the module off. Tap
/// switches.
class AccountList extends ConsumerWidget {
  const AccountList({super.key, this.onSwitched});
  final VoidCallback? onSwitched;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final entries = ref.watch(accountEntriesProvider);
    final sessions = ref.watch(tradeSessionsProvider);
    final activeEquity = ref.watch(terminalProvider.select((s) => s.metrics.equity));
    final optionsOn = ref.watch(configProvider.select((c) => c.moduleOn('options')));
    if (entries.isEmpty) return Padding(padding: const EdgeInsets.all(14), child: KSkeleton.lines(2));

    Widget row(AccountEntry e) => KPressable(
      key: ValueKey('trader-account-${e.login}'),
      pressedScale: 1,
      pressedOpacity: 0.7,
      onTap: () async {
        if (e.login == sessions.active) return;
        KHaptics.selection();
        if (await switchTradeAccount(context, e.login, live: e.live)) onSwitched?.call();
      },
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
        decoration: BoxDecoration(
          color: e.login == sessions.active ? k.ember.withValues(alpha: 0.07) : Colors.transparent,
          border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
        ),
        child: Row(
          children: [
            TBadge(
              t.dyn('trader.accountType.${e.live ? 'live' : 'demo'}', fallback: e.live ? 'live' : 'demo'),
              tone: e.live ? TBadgeTone.ember : TBadgeTone.gold,
              minWidth: 44,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Text(e.login, style: context.text.mono(12.5, weight: FontWeight.w600)),
                      if (e.isOptions) ...[const SizedBox(width: 5), TBadge(t('accounts.product.chipOptions'), tone: TBadgeTone.info)],
                      if (e.readOnly) ...[const SizedBox(width: 5), TBadge(t('trader.badge.readOnly'), tone: TBadgeTone.warn)],
                    ],
                  ),
                  Text(
                    '${e.group} · ${e.mode}${e.leverage > 0 ? ' · 1:${e.leverage}' : ''}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.footnote.copyWith(color: k.fg3, fontSize: 10.5),
                  ),
                ],
              ),
            ),
            if (sessions.busyLogin == e.login)
              CupertinoActivityIndicator(radius: 7, color: k.fg3)
            else
              Text.rich(
                TextSpan(
                  children: [
                    TextSpan(text: accMoney(e.cent, e.login == sessions.active ? activeEquity : e.equity), style: context.text.mono(12)),
                    TextSpan(
                      text: ' ${accCcy(e.cent)}',
                      style: context.text.mono(10, color: k.fg3),
                    ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );

    // "Open a CFD account" / "Open an Options account" under an empty section
    Widget open(String product) => KPressable(
      key: ValueKey('trader-open-$product'),
      pressedScale: 1,
      pressedOpacity: 0.7,
      onTap: () => GoRouter.of(context).go('/accounts/new?product=$product'),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 11),
        decoration: BoxDecoration(
          border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
        ),
        child: Row(
          children: [
            Icon(LucideIcons.plus, size: 15, color: k.ember),
            const SizedBox(width: 8),
            Expanded(
              child: Text(
                t(product == 'options' ? 'trader.acct.openOptions' : 'trader.acct.openCfd'),
                style: context.text.callout.copyWith(fontSize: 13, color: k.ember, fontWeight: FontWeight.w600),
              ),
            ),
          ],
        ),
      ),
    );

    if (!optionsOn) {
      return Column(children: [for (final e in entries.where((e) => !e.isOptions)) row(e)]);
    }
    final cfd = entries.where((e) => !e.isOptions).toList();
    final opt = entries.where((e) => e.isOptions).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TSectionLabel(t('accounts.product.groupCfd'), padding: const EdgeInsets.fromLTRB(12, 10, 12, 4)),
        if (cfd.isEmpty) open('cfd'),
        for (final e in cfd) row(e),
        TSectionLabel(t('accounts.product.groupOptions'), padding: const EdgeInsets.fromLTRB(12, 12, 12, 4)),
        if (opt.isEmpty) open('options'),
        for (final e in opt) row(e),
      ],
    );
  }
}

/// The header's account switcher.
Future<void> showAccountSwitcher(BuildContext context) => showKSheet<void>(
  context,
  title: context.t('trader.account.switch'),
  builder: (ctx) => SingleChildScrollView(
    padding: const EdgeInsets.fromLTRB(12, 0, 12, 12),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TPanel(child: AccountList(onSwitched: () => Navigator.of(ctx).maybePop())),
        const SizedBox(height: 10),
        KButton(
          label: ctx.t('trader.account.logInAnother'),
          icon: LucideIcons.userPlus,
          variant: KButtonVariant.surface,
          expand: true,
          onPressed: () {
            Navigator.of(ctx).pop();
            unawaited(showAddAccountSheet(context));
          },
        ),
      ],
    ),
  ),
);

/// "Login to trade account" (MT5-style).
Future<void> showAddAccountSheet(BuildContext context, {String? login}) => showKSheet<void>(
  context,
  title: context.t('trader.loginDialog.title'),
  builder: (_) => _AddAccountForm(initialLogin: login),
);

/// Demo logins start at 50 000 001, live at 10 000 001 (web serverForLogin).
String? serverForLogin(String login) => RegExp(r'^\d{8}$').hasMatch(login) ? (login.startsWith('5') ? 'Kalks-Demo' : 'Kalks-Live') : null;

class _AddAccountForm extends ConsumerStatefulWidget {
  const _AddAccountForm({this.initialLogin});
  final String? initialLogin;

  @override
  ConsumerState<_AddAccountForm> createState() => _AddAccountFormState();
}

class _AddAccountFormState extends ConsumerState<_AddAccountForm> {
  late final TextEditingController _login = TextEditingController(text: widget.initialLogin ?? '');
  final TextEditingController _password = TextEditingController();
  late String _server = serverForLogin(widget.initialLogin ?? '') ?? 'Kalks-Live';
  bool _show = false;
  bool _busy = false;
  String? _error;

  @override
  void dispose() {
    _login.dispose();
    _password.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final t = context.t;
    if (_busy) return;
    setState(() => _error = null);
    final l = _login.text.trim();
    if (!RegExp(r'^\d{8}$').hasMatch(l)) return setState(() => _error = t('trader.login.error.loginFormat'));
    if (_password.text.isEmpty) return setState(() => _error = t('trader.login.error.passwordRequired'));
    final expected = serverForLogin(l);
    if (expected != null && expected != _server) {
      return setState(() => _error = t('trader.login.error.wrongServer', {'login': l, 'expected': expected, 'server': _server}));
    }
    setState(() => _busy = true);
    try {
      final s = await ref.read(tradeSessionsProvider.notifier).loginWithPassword(login: l, password: _password.text, server: _server);
      if (!mounted) return;
      if (s.readOnly) {
        ref.read(notificationsProvider.notifier).toast(NotificationKind.warning, t('order.toast.readOnly'), description: t('order.toast.investorPassword'));
      } else {
        ref.read(notificationsProvider.notifier).toast(NotificationKind.success, t('trader.account.connectedTo', {'server': _server}), description: l);
      }
      KHaptics.success();
      Navigator.of(context).maybePop();
    } on ApiException catch (e) {
      if (!mounted) return;
      _password.clear();
      KHaptics.error();
      setState(() {
        _busy = false;
        _error = localizeTradeLogin(e, t);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            t('trader.loginDialog.subtitle'),
            textAlign: TextAlign.center,
            style: context.text.footnote.copyWith(color: k.fg3),
          ),
          const SizedBox(height: 14),
          KTextField(
            label: t('trader.login.login'),
            leading: LucideIcons.userRound,
            placeholder: t('trader.login.accountNumber'),
            controller: _login,
            ltr: true,
            keyboardType: TextInputType.number,
            inputFormatters: [FilteringTextInputFormatter.digitsOnly, LengthLimitingTextInputFormatter(8)],
            autofocus: widget.initialLogin == null,
            onChanged: (v) {
              final s = serverForLogin(v);
              if (s != null && s != _server) setState(() => _server = s);
            },
          ),
          const SizedBox(height: 12),
          KTextField(
            label: t('common.password'),
            leading: LucideIcons.lock,
            placeholder: t('trader.login.passwordPlaceholder'),
            controller: _password,
            obscure: !_show,
            autofocus: widget.initialLogin != null,
            textInputAction: TextInputAction.go,
            onSubmitted: (_) => unawaited(_submit()),
            trailing: KIconButton(
              icon: _show ? LucideIcons.eyeOff : LucideIcons.eye,
              size: 34,
              semanticLabel: _show ? t('trader.login.hidePassword') : t('trader.login.showPassword'),
              onPressed: () => setState(() => _show = !_show),
            ),
          ),
          const SizedBox(height: 12),
          Text(t('trader.login.server'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 6),
          KSegmented<String>(
            plain: true,
            values: const ['Kalks-Live', 'Kalks-Demo'],
            labels: const ['Kalks-Live', 'Kalks-Demo'],
            selected: _server,
            onChanged: (v) => setState(() => _server = v),
          ),
          const SizedBox(height: 5),
          Text(
            _server == 'Kalks-Demo' ? t('trader.login.demoAccounts') : t('trader.login.realAccounts'),
            style: context.text.footnote.copyWith(color: k.fg3, fontSize: 11.5),
          ),
          const SizedBox(height: 8),
          Text(t('trader.login.investorNote'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 11.5)),
          if (_error != null) ...[const SizedBox(height: 10), KFormError(_error)],
          const SizedBox(height: 14),
          KButton(
            label: _busy ? t('trader.splash.connecting', {'server': _server}) : t('trader.guest.logIn'),
            size: KButtonSize.lg,
            expand: true,
            loading: _busy,
            onPressed: _busy ? null : () => unawaited(_submit()),
          ),
        ],
      ),
    );
  }
}

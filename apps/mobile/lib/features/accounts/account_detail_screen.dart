// Accounts › account detail. Port of the phone web page /accounts/<login>?tab= (apps/crm/components/trading/
// account-detail.tsx LiveAccountDetail), in its phone order:
//   1 breadcrumb (Accounts / #login)
//   2 header card: badges, title, login (copy), server, leverage, currency; equity; balance / free margin / margin
//     level / floating P&L; ⋯ menu, Fund or Refill, Trade (archived: Restore)
//   3 the closure request banner
//   4 tabs: Overview · Positions · History · Ledger · Analytics · Credentials · Settings (the last two hidden for
//     view-only sessions and archived accounts)
// `GET trading/accounts/{login}` every 3 s (web usePoll), plus the tabs' own calls. Not found (404 or a login that
// isn't 8 digits), unavailable and loading states as on the web.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/models/account.dart';
import '../../core/models/trading.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import '../portfolio/widgets/activity_panels.dart';
import '../portfolio/widgets/analytics_panel.dart';
import 'account_actions.dart';
import 'accounts_data.dart';
import 'widgets/account_bits.dart';
import 'widgets/detail_panels.dart';
import 'widgets/manage_panels.dart';

/// The detail tabs, in the web's order.
const List<String> kAccountTabs = ['overview', 'positions', 'history', 'ledger', 'analytics', 'credentials', 'settings'];

/// The tabs an account shows: view-only sessions and archived accounts never see credentials or settings.
List<String> accountTabs({required bool readOnly, required bool archived}) =>
    readOnly || archived ? kAccountTabs.where((x) => x != 'credentials' && x != 'settings').toList() : kAccountTabs;

class AccountDetailScreen extends ConsumerStatefulWidget {
  const AccountDetailScreen({super.key, required this.login, this.query = const {}});

  /// The account login from the path.
  final String login;

  /// The route's query parameters (`tab`: overview | positions | history | ledger | analytics | credentials | settings).
  final Map<String, String> query;

  @override
  ConsumerState<AccountDetailScreen> createState() => _AccountDetailScreenState();
}

class _AccountDetailScreenState extends ConsumerState<AccountDetailScreen> {
  late String _tab = kAccountTabs.contains(widget.query['tab']) ? widget.query['tab']! : 'overview';
  final _scroll = ScrollController();

  bool get _valid => RegExp(r'^\d{8}$').hasMatch(widget.login);
  int get _login => int.parse(widget.login);

  @override
  void didUpdateWidget(AccountDetailScreen old) {
    super.didUpdateWidget(old);
    final q = widget.query['tab'];
    if (q != old.query['tab'] && kAccountTabs.contains(q)) setState(() => _tab = q!);
  }

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  void _reload() => ref.invalidate(accountDetailProvider(_login));

  void _setTab(String tab) {
    if (tab == _tab) return;
    setState(() => _tab = tab);
  }

  void _back() {
    if (context.canPop()) {
      context.pop();
    } else {
      context.go('/accounts');
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    if (!_valid) return _NotFound(login: widget.login);
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final detail = ref.watch(accountDetailProvider(_login));
    final err = detail.error;
    if (err is ApiException && err.status == 404) return _NotFound(login: widget.login);
    if (detail.hasError && !detail.hasValue) {
      return KPageScroll(
        children: [
          const SizedBox(height: 24),
          KCard(
            child: KEmptyState(
              art: KIllustrationName.connectionLost,
              title: t('accountDetail.unavailable.title'),
              text: errorText(err, t),
              action: KButton(label: t('common.retry'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, onPressed: _reload),
            ),
          ),
        ],
      );
    }
    if (!detail.hasValue) {
      return const KPageScroll(
        children: [
          KSkeleton(width: 190, height: 18),
          SizedBox(height: 16),
          KSkeleton(height: 260, radius: 24),
          SizedBox(height: 22),
          KSkeleton(height: 40, radius: 20),
          SizedBox(height: 18),
          KSkeleton(height: 320, radius: 24),
        ],
      );
    }
    final d = detail.requireValue;
    final a = d.account;
    final tabs = accountTabs(readOnly: readOnly, archived: a.archived);
    final tab = tabs.contains(_tab) ? _tab : 'overview';
    String tabLabel(String key) {
      final label = t('accountDetail.tab.$key');
      return key == 'positions' ? '$label ${a.positions + a.orders}' : label;
    }

    final panel = switch (tab) {
      'positions' => PositionsPanel(account: a, positions: d.positions, orders: d.orders),
      'history' => HistoryPanel(account: a),
      'ledger' => LedgerPanel(account: a),
      'analytics' => AccountAnalyticsPanel(account: a),
      'credentials' => CredentialsPanel(account: a),
      'settings' => SettingsPanel(account: a, onChanged: _reload),
      _ => OverviewPanel(account: a, positions: d.positions, onTab: _setTab),
    };

    return KPageScroll(
      controller: _scroll,
      onRefresh: () async {
        _reload();
        ref.invalidate(recentDealsProvider(_login));
        await ref.read(accountDetailProvider(_login).future).then((_) {}, onError: (Object _) {});
      },
      children: [
        _Breadcrumb(login: a.login, onBack: _back),
        const SizedBox(height: 14),
        _HeaderCard(account: a, readOnly: readOnly, onChanged: _reload, onTab: _setTab),
        ClosureBanner(account: a, onChanged: _reload),
        const SizedBox(height: 20),
        KChoiceChips<String>(
          values: tabs,
          labels: [for (final x in tabs) tabLabel(x)],
          selected: tab,
          onChanged: _setTab,
          padding: const EdgeInsets.symmetric(horizontal: 2),
        ),
        const SizedBox(height: 16),
        AnimatedSwitcher(
          duration: const Duration(milliseconds: 200),
          child: KeyedSubtree(key: ValueKey(tab), child: panel),
        ),
      ],
    );
  }
}

/// "← Accounts / #10042817" (web breadcrumb).
class _Breadcrumb extends StatelessWidget {
  const _Breadcrumb({required this.login, required this.onBack});
  final int login;
  final VoidCallback onBack;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return Row(
      children: [
        KPressable(
          onTap: onBack,
          minSize: 36,
          semanticLabel: t('common.back'),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(rtl ? LucideIcons.arrowRight : LucideIcons.arrowLeft, size: 15, color: k.fg3),
              const SizedBox(width: 6),
              Text(t('accountDetail.breadcrumb.accounts'), style: context.text.label.copyWith(color: k.fg3)),
            ],
          ),
        ),
        const SizedBox(width: 8),
        Text('/', style: context.text.label.copyWith(color: k.fg3)),
        const SizedBox(width: 8),
        Text(
          '#$login',
          textDirection: TextDirection.ltr,
          style: context.text.mono(13, color: k.fg2),
        ),
      ],
    );
  }
}

/// The header card (web Detail's first Card).
class _HeaderCard extends ConsumerWidget {
  const _HeaderCard({required this.account, required this.readOnly, required this.onChanged, required this.onTab});
  final EngineAccount account;
  final bool readOnly;
  final VoidCallback onChanged;
  final ValueChanged<String> onTab;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final a = account;
    final cur = a.cent ? 'USC' : a.currency;
    final copying = accountFlavor(a) == 'copy' ? copyingName(a) : null;
    final archivedOn = a.archivedAt ?? a.closedAt;
    final lt = a.margin > 0 ? levelTone(a.marginLevel) : null;
    Widget stat(String label, Widget value) => Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(label, style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13)),
        const SizedBox(width: 5),
        DefaultTextStyle.merge(
          style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600, fontFeatures: kTabular),
          child: value,
        ),
      ],
    );
    return KCard(
      padding: const EdgeInsets.all(20),
      hot: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              KindBadge(account: a),
              if (a.isOptions) ProductChip(account: a),
              FlavorChip(account: a),
              Text(
                '${a.groupName} · ${t.dyn('accounts.mode.${a.mode}', fallback: modeLabel(a.mode))}',
                style: context.text.title1.copyWith(fontWeight: FontWeight.w500),
              ),
              if (a.name.isNotEmpty) Text('“${a.name}”', style: context.text.callout.copyWith(color: k.fg3)),
              DefaultStar(account: a),
              StatusBadge(account: a),
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 14,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              LoginCopy(login: a.login),
              Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Icon(LucideIcons.server, size: 13, color: k.fg3),
                  const SizedBox(width: 5),
                  Text(a.server, style: context.text.mono(13, color: k.fg2)),
                ],
              ),
              KChip(label: levLabel(a.leverage), small: true),
              Text(a.cent ? t('accountDetail.header.centCurrency') : a.currency, style: context.text.label.copyWith(color: k.fg3)),
              if (copying != null)
                Text(
                  t('accounts.copy.copying', {'name': copying}),
                  style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
                ),
              if (a.archived && archivedOn != null)
                Text(
                  t(a.status == 'closed' ? 'accounts.archived.closedOn' : 'accounts.archived.on', {'date': fmtDate(t, archivedOn)}),
                  style: context.text.label.copyWith(color: k.fg3),
                ),
            ],
          ),
          const SizedBox(height: 18),
          Text(t('common.equity'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 4),
          Wrap(
            spacing: 10,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              FittedBox(
                fit: BoxFit.scaleDown,
                child: KMoney(
                  a.equity,
                  currency: cur,
                  style: context.text.moneyXL.copyWith(fontSize: 40, fontWeight: FontWeight.w600),
                ),
              ),
              if (a.live && !a.prop && a.balance == 0 && a.equity == 0) KChip(label: t('accountDetail.header.notFunded'), tone: KChipTone.warn),
            ],
          ),
          const SizedBox(height: 14),
          Wrap(
            spacing: 18,
            runSpacing: 8,
            children: [
              stat(
                t('common.balance'),
                KMoney(
                  a.balance,
                  currency: cur,
                  style: context.text.label.copyWith(fontWeight: FontWeight.w600),
                ),
              ),
              stat(
                t('accountDetail.stat.freeMargin'),
                KMoney(
                  a.freeMargin,
                  currency: cur,
                  style: context.text.label.copyWith(fontWeight: FontWeight.w600),
                ),
              ),
              stat(
                t('accountDetail.stat.marginLevel'),
                Text(
                  fmtLevel(a.margin > 0 ? a.marginLevel : null),
                  textDirection: TextDirection.ltr,
                  style: TextStyle(color: lt == null ? k.fg : levelColor(context, a.marginLevel)),
                ),
              ),
              stat(
                t('accountDetail.stat.floatingPnl'),
                Text(
                  fmtAmount(a.profit, a.currencyPrefix, signed: true),
                  textDirection: TextDirection.ltr,
                  style: TextStyle(color: pnlColor(context, a.profit)),
                ),
              ),
            ],
          ),
          if (!readOnly && a.archived && a.status == 'archived') ...[const SizedBox(height: 18), _RestoreButton(account: a, onDone: onChanged)],
          if (!readOnly && !a.archived) ...[
            const SizedBox(height: 18),
            Wrap(
              spacing: 8,
              runSpacing: 10,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                AccountMenuButton(account: a, onChanged: onChanged, onTab: onTab),
                if (a.live && !a.prop) FundButton(account: a) else if (!a.live) RefillButton(account: a, onDone: onChanged),
                if (accountFlavor(a) == 'copy') ...[
                  KButton(label: t('accounts.copy.manage'), variant: KButtonVariant.surface, onPressed: () => context.go('/social/copy')),
                  TradeButton(account: a, size: KButtonSize.lg, variant: KButtonVariant.surface, label: t('accounts.copy.watchPnl')),
                ] else
                  TradeButton(account: a, size: KButtonSize.lg),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

class _RestoreButton extends StatefulWidget {
  const _RestoreButton({required this.account, required this.onDone});
  final EngineAccount account;
  final VoidCallback onDone;

  @override
  State<_RestoreButton> createState() => _RestoreButtonState();
}

class _RestoreButtonState extends State<_RestoreButton> {
  bool _busy = false;

  Future<void> _restore() async {
    setState(() => _busy = true);
    final ok = await restoreAccount(context, widget.account);
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) {
      refreshAccountData(ProviderScope.containerOf(context, listen: false));
      widget.onDone();
    }
  }

  @override
  Widget build(BuildContext context) => Align(
    alignment: AlignmentDirectional.centerStart,
    child: KButton(
      label: context.t('accounts.archived.restore'),
      icon: LucideIcons.rotateCcw,
      variant: KButtonVariant.surface,
      size: KButtonSize.sm,
      loading: _busy,
      onPressed: _busy ? null : _restore,
    ),
  );
}

/// "Account #… not found" (a 404 or a login that isn't 8 digits).
class _NotFound extends StatelessWidget {
  const _NotFound({required this.login});
  final String login;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return KPageScroll(
      children: [
        const SizedBox(height: 24),
        KCard(
          child: KEmptyState(
            art: KIllustrationName.market,
            title: t('accountDetail.notFound.title', {'login': login}),
            text: t('accountDetail.notFound.text'),
            action: Wrap(
              spacing: 8,
              runSpacing: 8,
              alignment: WrapAlignment.center,
              children: [
                KButton(
                  label: t('accountDetail.notFound.myAccounts'),
                  icon: rtl ? LucideIcons.arrowRight : LucideIcons.arrowLeft,
                  variant: KButtonVariant.surface,
                  onPressed: () => context.go('/accounts'),
                ),
                KButton(label: t('accountDetail.notFound.openAccount'), onPressed: () => context.go('/accounts/new')),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

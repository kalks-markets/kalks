// Accounts › My accounts. Port of the phone web page /accounts (apps/crm/components/trading/accounts-page.tsx
// LiveAccountsPage), in its phone order:
//   1 the photo hero (Trading accounts, the short line, Open account), page_hero.dart; a white-label broker gets the
//     page header instead
//   2 the figures as one glass strip (web .k-kpi row): live equity, free margin, accounts, demo accounts
//   3 My accounts: Live / Demo / Archived (?tab=), each account as a debit card with its actions (by product, CFD
//     accounts then Options accounts, once the client holds an Options account), "Open a new … account"
//   4 Account types as card faces (the broker's groups -> /accounts/new?group=; Options types only while the module
//     is on)
// `GET trading/accounts` every 5 s (web useAccounts), `GET trading/groups` once.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../data/client_data.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import '../dashboard/widgets/balance_strip.dart';
import 'accounts_data.dart';
import 'widgets/account_bits.dart';
import 'widgets/account_rows.dart';
import 'widgets/group_card.dart';

/// The Live / Demo / Archived tab to show: `?tab=` when valid, else the tab that has accounts (live first).
String accountsTab(String? query, {required int live, required int demo}) {
  if (query == 'live' || query == 'demo' || query == 'archived') return query!;
  return live == 0 && demo > 0 ? 'demo' : 'live';
}

class AccountsScreen extends ConsumerStatefulWidget {
  const AccountsScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params: `tab` = live | demo | archived).
  final Map<String, String> query;

  @override
  ConsumerState<AccountsScreen> createState() => _AccountsScreenState();
}

class _AccountsScreenState extends ConsumerState<AccountsScreen> {
  late String? _tab = _validTab(widget.query['tab']);

  static String? _validTab(String? q) => q == 'live' || q == 'demo' || q == 'archived' ? q : null;

  @override
  void didUpdateWidget(AccountsScreen old) {
    super.didUpdateWidget(old);
    final q = _validTab(widget.query['tab']);
    if (q != null && q != _validTab(old.query['tab'])) setState(() => _tab = q);
  }

  void _reload() {
    ref
      ..invalidate(accountsPageProvider)
      ..invalidate(accountsProvider);
  }

  Future<void> _refresh() async {
    ref
      ..invalidate(accountsPageProvider)
      ..invalidate(groupsProvider);
    await Future.wait<void>([
      ref.read(accountsPageProvider.future).then((_) {}, onError: (Object _) {}),
      ref.read(groupsProvider.future).then((_) {}, onError: (Object _) {}),
    ]);
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final acc = ref.watch(accountsPageProvider);
    final optionsOn = ref.watch(configProvider.select((c) => c.moduleOn('options')));
    final groups = ref.watch(groupsProvider).value?.where((g) => (optionsOn || !g.isOptions) && g.code != 'options-mm').toList();
    final all = acc.value ?? const <EngineAccount>[];
    final loading = !acc.hasValue && !acc.hasError;
    final totals = AccountTotals(all);
    final active = all.where((a) => !a.archived).toList();
    final tab = _tab ?? accountsTab(null, live: totals.live.length, demo: totals.demo.length);
    final list = switch (tab) {
      'demo' => totals.demo,
      'archived' => totals.archived,
      _ => totals.live,
    };
    final refills = totals.demo.fold<int>(0, (s, a) => s + refillsLeft(a));

    // 3. My accounts
    Widget accountsCard;
    if (acc.hasError && !acc.hasValue) {
      final e = acc.error;
      final serverMessage = e is ApiException && !(e.status == 0 || e.status >= 500) ? localizeError(e, t) : null;
      accountsCard = KCard(
        child: KEmptyState(
          art: KIllustrationName.connectionLost,
          title: t('accounts.error.unavailableTitle'),
          text: serverMessage ?? t('accounts.error.unavailableText'),
          action: KButton(label: t('common.retry'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, onPressed: _reload),
        ),
      );
    } else {
      final tabs = <String>['live', 'demo', if (totals.archived.isNotEmpty || tab == 'archived') 'archived'];
      String label(String v) => switch (v) {
        'demo' => '${t('common.demo')} ${totals.demo.length}',
        'archived' => '${t('accounts.tab.archived')} ${totals.archived.length}',
        _ => '${t('common.live')} ${totals.live.length}',
      };
      accountsCard = KCard(
        padding: const EdgeInsets.fromLTRB(16, 18, 16, 16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            KCardHeader(title: t('accounts.list.myAccounts'), subtitle: tab == 'archived' ? t('accounts.list.archivedHint') : t('accounts.list.liveHint')),
            const SizedBox(height: 14),
            KSegmented<String>(values: tabs, labels: [for (final v in tabs) label(v)], selected: tab, onChanged: (v) => setState(() => _tab = v)),
            const SizedBox(height: 14),
            if (loading) ...[const AspectRatio(aspectRatio: 1.586, child: KSkeleton(radius: 22)), const SizedBox(height: 12)],
            if (!loading && list.isEmpty)
              tab == 'archived'
                  ? KEmptyState(compact: true, art: KIllustrationName.welcome, title: t('accounts.archived.none'), text: t('accounts.archived.noneText'))
                  : KEmptyState(
                      compact: true,
                      art: KIllustrationName.welcome,
                      title: tab == 'live' ? t('accounts.empty.noLive') : t('accounts.empty.noDemo'),
                      text: tab == 'live' ? t('accounts.empty.liveText') : t('accounts.empty.demoText'),
                    ),
            // by product once the client holds an Options account: CFD accounts, then Options accounts
            for (final (product, accounts) in byProduct(list)) ...[
              if (list.any((a) => a.isOptions)) ProductSection(key: ValueKey('section-$product'), product: product),
              for (final a in accounts) ...[
                if (tab == 'archived')
                  ArchivedAccountRow(key: ValueKey('arch-${a.login}'), account: a, onChanged: _reload)
                else
                  LiveAccountRow(key: ValueKey('row-${a.login}'), account: a, onChanged: _reload),
                SizedBox(height: tab == 'archived' ? 12 : 26),
              ],
            ],
            if (!readOnly && tab != 'archived') _OpenNewLink(demo: tab == 'demo'),
          ],
        ),
      );
    }

    final openHref = '/accounts/new${tab == 'demo' ? '?type=demo' : ''}';
    final hero = pageHero(
      context,
      ref,
      path: '/accounts',
      title: t('accounts.list.title'),
      lead: t('accounts.list.subtitle'),
      actions: [
        if (!readOnly) KHeroButton(label: t('accounts.list.openAccount'), icon: LucideIcons.plus, primary: true, onPressed: () => context.go(openHref)),
      ],
    );
    Widget skeleton() => const KSkeleton(width: 110, height: 26);

    return KPageScroll(
      onRefresh: _refresh,
      hero: hero,
      padding: const EdgeInsets.fromLTRB(KSpace.page, 18, KSpace.page, 24),
      children: [
        // 1. header (no photo: a white-label broker)
        if (hero == null) ...[
          KPageHeader(title: t('accounts.list.title'), subtitle: Text(t('accounts.list.subtitle'))),
          if (!readOnly) ...[
            const SizedBox(height: 16),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: KButton(label: t('accounts.list.openAccount'), icon: LucideIcons.plus, size: KButtonSize.lg, onPressed: () => context.go(openHref)),
            ),
          ],
          const SizedBox(height: 20),
        ],
        // 2. the figures
        BalanceStrip(
          cells: [
            StripCell(
              key: 'liveEquity',
              secret: true,
              label: t('accounts.kpi.liveEquity'),
              value: (s) => loading ? skeleton() : KMoney(totals.equity, style: s),
              sub: totals.live.isNotEmpty ? t('accounts.kpi.balanceChip', {'amount': '\$${Fmt.number(totals.balance)}'}) : t('accounts.empty.noLive'),
              href: '/portfolio',
            ),
            StripCell(
              key: 'freeMargin',
              secret: true,
              label: t('accounts.label.freeMargin'),
              value: (s) => loading ? skeleton() : KMoney(totals.freeMargin, style: s),
              sub: totals.equity > 0
                  ? t('accounts.kpi.ofEquity', {'pct': (totals.freeMargin / totals.equity * 100).toStringAsFixed(1)})
                  : t('accounts.kpi.liveUsd'),
            ),
            StripCell(
              key: 'accounts',
              label: t('common.accounts'),
              value: (s) => Text(loading ? '—' : '${active.length}', style: s),
              sub:
                  '${t('accounts.kpi.liveCount', {'count': totals.live.length})} · ${t('accounts.kpi.demoCount', {'count': totals.demo.length})} · ${t('accounts.kpi.openPositions', {'count': totals.positions})}',
            ),
            StripCell(
              key: 'demoAccounts',
              label: t('accounts.kpi.demoAccounts'),
              value: (s) => Text(loading ? '—' : '${totals.demo.length}', style: s),
              sub: totals.demo.isNotEmpty ? t('accounts.kpi.refillsLeftToday', {'count': refills}) : t('accounts.kpi.practise'),
            ),
          ],
        ),
        const SizedBox(height: 16),
        // 3. My accounts
        accountsCard,
        // 4. Account types
        if (!readOnly && groups != null && groups.isNotEmpty) ...[
          const SizedBox(height: 16),
          KCard(
            padding: EdgeInsets.zero,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 18, 16, 4),
                  child: KCardHeader(title: t('accounts.types.title'), subtitle: t('accounts.types.subtitle')),
                ),
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 12, 16, 20),
                  child: Column(
                    children: [
                      for (final g in groups) ...[
                        EngineGroupCard(group: g, compact: true, onSelect: () => context.go('/accounts/new?group=${Uri.encodeComponent(g.code)}')),
                        if (g != groups.last) const SizedBox(height: 14),
                      ],
                    ],
                  ),
                ),
                Container(
                  padding: const EdgeInsets.fromLTRB(20, 14, 20, 16),
                  decoration: BoxDecoration(
                    border: Border(top: BorderSide(color: k.line, width: 0.6)),
                  ),
                  child: Text(t('accounts.types.footer'), style: context.text.footnote.copyWith(color: k.fg3)),
                ),
              ],
            ),
          ),
        ],
      ],
    );
  }
}

/// "Open a new live / demo account" (web dashed link under the rows).
class _OpenNewLink extends StatelessWidget {
  const _OpenNewLink({required this.demo});
  final bool demo;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KPressable(
      onTap: () => context.go('/accounts/new?type=${demo ? 'demo' : 'live'}'),
      pressedScale: 0.99,
      child: CustomPaint(
        painter: _DashedBorder(color: k.line, radius: 14),
        child: Padding(
          padding: const EdgeInsets.symmetric(vertical: 16),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(LucideIcons.plus, size: 16, color: k.fg3),
              const SizedBox(width: 8),
              Text(demo ? t('accounts.list.openNewDemo') : t('accounts.list.openNewLive'), style: context.text.callout.copyWith(color: k.fg3)),
            ],
          ),
        ),
      ),
    );
  }
}

class _DashedBorder extends CustomPainter {
  _DashedBorder({required this.color, required this.radius});
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final path = Path()..addRRect(RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius)));
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.2;
    for (final m in path.computeMetrics()) {
      var d = 0.0;
      while (d < m.length) {
        canvas.drawPath(m.extractPath(d, d + 5), paint);
        d += 9;
      }
    }
  }

  @override
  bool shouldRepaint(_DashedBorder old) => old.color != color;
}

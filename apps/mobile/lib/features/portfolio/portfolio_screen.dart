// Portfolio › Overview (port of apps/crm/components/trading/portfolio.tsx LivePortfolio), in the web's phone order:
//   1 header (Portfolio) + Statements / Open account
//   2 KPI cards: live equity · live balance · floating P&L · open positions   (one per row on phones, as the web)
//   3 equity allocation (donut of the live accounts, USD)
//   4 accounts (balance, equity, floating, positions per account) + Manage
//   5 open positions across all accounts (refreshed every 10 s) with Trader
//   6 links: Trade history · Ledger · Statements
// Accounts: GET trading/accounts every 5 s; positions: GET trading/accounts/{login} per account with positions.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/models/trading.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import '../accounts/widgets/account_bits.dart' show ProductChip, ProductSection, byProduct;
import 'portfolio_data.dart';
import 'portfolio_logic.dart';
import 'widgets/portfolio_bits.dart';

class PortfolioScreen extends ConsumerWidget {
  const PortfolioScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params).
  final Map<String, String> query;

  Future<void> _refresh(WidgetRef ref) async {
    ref
      ..invalidate(portfolioAccountsProvider)
      ..invalidate(openPositionsProvider);
    await ref.read(portfolioAccountsProvider.future).then((_) {}, onError: (Object _) {});
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final acc = ref.watch(portfolioAccountsProvider);
    final loading = !acc.hasValue && !acc.hasError;
    final accounts = (acc.value ?? const <EngineAccount>[]).where((a) => !a.archived).toList();
    final totals = AccountTotals(accounts);
    Widget skeleton() => const KSkeleton(width: 130, height: 28);

    return KPageScroll(
      onRefresh: () => _refresh(ref),
      children: [
        KPageHeader(title: t('portfolio.title'), subtitle: Text(t('portfolio.subtitle'))),
        PageActions(
          children: [
            KButton(
              label: t('portfolio.st.title'),
              icon: LucideIcons.fileText,
              variant: KButtonVariant.surface,
              onPressed: () => context.go('/portfolio/statements'),
            ),
            if (!readOnly) KButton(label: t('portfolio.openAccount'), icon: LucideIcons.plus, onPressed: () => context.go('/accounts/new')),
          ],
        ),
        const SizedBox(height: 18),
        if (acc.hasError && !acc.hasValue)
          AccountsError(onRetry: () => ref.invalidate(portfolioAccountsProvider))
        else ...[
          KKpiCard(
            label: t('portfolio.kpi.liveEquity'),
            icon: LucideIcons.trendingUp,
            value: loading ? skeleton() : KMoney(totals.equity, style: context.text.moneyL),
            chip: KChip(label: t('portfolio.kpi.liveAccounts', {'count': totals.live.length}), small: true),
            onTap: () => context.go('/accounts'),
          ),
          const SizedBox(height: 12),
          KKpiCard(
            label: t('portfolio.kpi.liveBalance'),
            icon: LucideIcons.wallet,
            value: loading ? skeleton() : KMoney(totals.balance, style: context.text.moneyL),
            chip: KChip(label: t('portfolio.kpi.excludesFloating'), small: true),
          ),
          const SizedBox(height: 12),
          KKpiCard(
            label: t('portfolio.kpi.floating'),
            icon: LucideIcons.shieldCheck,
            value: loading ? skeleton() : KMoney(totals.profit, signed: true, tone: KMoneyTone.auto, style: context.text.moneyL),
            chip: KChip(label: t('portfolio.kpi.freeMargin', {'amount': Fmt.money(totals.freeMargin)}), small: true),
          ),
          const SizedBox(height: 12),
          KKpiCard(
            label: t('portfolio.kpi.openPositions'),
            icon: LucideIcons.layers,
            value: Text(
              loading ? '—' : '${totals.positions}',
              style: TextStyle(fontFeatures: kTabular, color: k.fg),
            ),
            footer: Wrap(
              spacing: 6,
              children: [
                KChip(label: t('portfolio.kpi.live', {'count': totals.live.length}), tone: KChipTone.ember, small: true),
                KChip(label: t('portfolio.kpi.demo', {'count': totals.demo.length}), tone: KChipTone.gold, small: true),
              ],
            ),
          ),
          if (!loading && accounts.isEmpty) ...[const SizedBox(height: 14), NoAccounts(readOnly: readOnly)],
          if (accounts.isNotEmpty) ...[
            const SizedBox(height: 14),
            _Allocation(totals: totals),
            const SizedBox(height: 14),
            _AccountsCard(accounts: accounts),
            const SizedBox(height: 14),
            const _Positions(),
            const SizedBox(height: 14),
            for (final (href, icon, title, text) in [
              ('/portfolio/history', LucideIcons.history, t('portfolio.history.title'), t('portfolio.links.history')),
              ('/portfolio/ledger', LucideIcons.bookText, t('portfolio.ledger.title'), t('portfolio.links.ledger')),
              ('/portfolio/statements', LucideIcons.fileText, t('portfolio.st.title'), t('portfolio.links.statements')),
            ]) ...[
              KCard(
                padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 14),
                onTap: () => context.go(href),
                child: Row(
                  children: [
                    Container(
                      width: 40,
                      height: 40,
                      decoration: BoxDecoration(
                        color: k.surface2,
                        shape: BoxShape.circle,
                        border: Border.all(color: k.line),
                      ),
                      child: Icon(icon, size: 16, color: k.fg2),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(title, style: context.text.headline.copyWith(fontSize: 14, fontWeight: FontWeight.w500)),
                          Text(text, style: context.text.footnote.copyWith(color: k.fg3)),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 10),
            ],
          ],
        ],
      ],
    );
  }
}

class _Allocation extends StatelessWidget {
  const _Allocation({required this.totals});
  final AccountTotals totals;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final colors = chartColors(k);
    final alloc = allocation(totals);
    final total = alloc.fold<double>(0, (s, d) => s + d.usd);
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('portfolio.alloc.title'), subtitle: t('portfolio.alloc.subtitle')),
          const SizedBox(height: 16),
          if (total > 0) ...[
            Center(
              child: KDonut(
                size: 160,
                thickness: 18,
                segments: [for (var i = 0; i < alloc.length; i++) (alloc[i].usd, colors[i % colors.length])],
                center: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Text(
                      t('common.total'),
                      style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                    ),
                    KMoney(total, decimals: 0, style: context.text.figure.copyWith(fontSize: 17)),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 16),
            for (var i = 0; i < alloc.length; i++)
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Row(
                  children: [
                    Container(
                      width: 10,
                      height: 10,
                      decoration: BoxDecoration(color: colors[i % colors.length], shape: BoxShape.circle),
                    ),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text('#${alloc[i].login}', style: context.text.mono(12.5, color: k.fg2)),
                    ),
                    Text(
                      '${(alloc[i].usd / total * 100).toStringAsFixed(1)}%',
                      textDirection: TextDirection.ltr,
                      style: context.text.label.copyWith(fontWeight: FontWeight.w600, fontFeatures: kTabular),
                    ),
                  ],
                ),
              ),
          ] else
            Text(totals.live.isNotEmpty ? t('portfolio.alloc.noEquity') : t('portfolio.alloc.noLive'), style: context.text.footnote.copyWith(color: k.fg3)),
        ],
      ),
    );
  }
}

class _AccountsCard extends StatelessWidget {
  const _AccountsCard({required this.accounts});
  final List<EngineAccount> accounts;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    Widget cell(String label, String value, {Color? color, bool bold = false, CrossAxisAlignment align = CrossAxisAlignment.start}) => Column(
      crossAxisAlignment: align,
      children: [
        Text(
          label,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
        ),
        const SizedBox(height: 2),
        Text(
          value,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          textDirection: TextDirection.ltr,
          style: context.text.label.copyWith(fontFeatures: kTabular, fontWeight: bold ? FontWeight.w600 : FontWeight.w500, color: color ?? k.fg),
        ),
      ],
    );
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('common.accounts'),
            subtitle: t('portfolio.accounts.subtitle'),
            action: KButton(
              label: t('portfolio.accounts.manage'),
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => context.go('/accounts'),
            ),
          ),
          const SizedBox(height: 12),
          // by product once the client holds an Options account: CFD accounts, then Options accounts
          for (final (product, group) in byProduct(accounts)) ...[
            if (accounts.any((a) => a.isOptions)) ProductSection(product: product, padding: const EdgeInsets.fromLTRB(2, 4, 2, 8)),
            for (final a in group) ...[
              KPressable(
                pressedScale: 0.99,
                semanticLabel: '#${a.login}',
                onTap: () => context.go('/accounts/${a.login}'),
                child: Container(
                  padding: const EdgeInsets.fromLTRB(14, 11, 14, 12),
                  decoration: BoxDecoration(
                    color: k.surface2.withValues(alpha: 0.7),
                    borderRadius: BorderRadius.circular(14),
                    border: Border.all(color: k.line),
                  ),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        children: [
                          KindBadge(account: a),
                          if (a.isOptions) ...[const SizedBox(width: 6), ProductChip(account: a)],
                          const SizedBox(width: 8),
                          Text(
                            '#${a.login}',
                            textDirection: TextDirection.ltr,
                            style: context.text.mono(13, color: k.fg),
                          ),
                          const Spacer(),
                          Icon(Directionality.of(context) == TextDirection.rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 16, color: k.fg3),
                        ],
                      ),
                      const SizedBox(height: 9),
                      Row(
                        children: [
                          Expanded(flex: 5, child: cell(t('common.balance'), fmtAmount(a.balance, a.currencyPrefix))),
                          Expanded(flex: 5, child: cell(t('common.equity'), fmtAmount(a.equity, a.currencyPrefix), bold: true)),
                          Expanded(
                            flex: 5,
                            child: cell(t('portfolio.col.floating'), fmtAmount(a.profit, a.currencyPrefix, signed: true), color: signColor(k, a.profit)),
                          ),
                          Expanded(
                            flex: 3,
                            child: cell(t('portfolio.col.positions'), '${a.positions}', color: k.fg2, align: CrossAxisAlignment.end),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 8),
            ],
          ],
        ],
      ),
    );
  }
}

class _Positions extends ConsumerWidget {
  const _Positions();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final rows = ref.watch(openPositionsProvider);
    final list = rows.value;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('portfolio.kpi.openPositions'), subtitle: t('portfolio.positions.subtitle')),
          const SizedBox(height: 12),
          if (list == null) const KSkeleton(height: 96, radius: 14),
          if (list != null && list.isEmpty) KEmptyState(compact: true, art: KIllustrationName.emptyPosition, title: t('portfolio.positions.empty')),
          if (list != null)
            for (final r in list) ...[_PositionRow(row: r), const SizedBox(height: 8)],
          if (list == null && rows.hasError) Text(errorText(rows.error, t), style: context.text.footnote.copyWith(color: k.fg3)),
        ],
      ),
    );
  }
}

class _PositionRow extends StatelessWidget {
  const _PositionRow({required this.row});
  final PositionRow row;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = row.account, p = row.position;
    final isOpt = p.isOption;
    final prem = isOpt ? positionPremiumsUsd(p, usdFactorOf(a)) : null;
    final buy = p.side == 'buy';
    final side = (buy ? t('common.buy') : t('common.sell')).toUpperCase();
    final size = isOpt ? t('accounts.opt.contracts', {'count': fmtContracts(p.volume)}) : _volume(p.volume);
    final prices = prem != null && prem.open != null && prem.now != null
        ? '${fmtAmount(prem.open!, r'$')} → ${fmtAmount(prem.now!, r'$')} ${t('accounts.opt.perContract')}'
        : '${fmtPrice(p.openPrice)} → ${fmtPrice(p.currentPrice)}';
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 11, 12, 11),
      decoration: BoxDecoration(
        color: k.surface2.withValues(alpha: 0.7),
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Row(
        children: [
          TradeSymbolAvatar(symbol: p.symbol, size: 24),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Wrap(
                  spacing: 6,
                  runSpacing: 4,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    Text(symbolLabel(t, p.symbol, p.option), style: context.text.headline.copyWith(fontSize: 13.5, fontWeight: FontWeight.w500)),
                    SideChip(buy: buy, label: '$side $size'),
                    if (isOpt) const OptionTag(),
                    KindBadge(account: a),
                  ],
                ),
                const SizedBox(height: 3),
                Text(
                  '#${a.login} · $prices',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  textDirection: TextDirection.ltr,
                  textAlign: Directionality.of(context) == TextDirection.rtl ? TextAlign.right : TextAlign.left,
                  style: context.text.mono(11, color: k.fg3),
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Text(
                fmtAmount(p.profit, a.currencyPrefix, signed: true),
                textDirection: TextDirection.ltr,
                style: context.text.figure.copyWith(fontSize: 14, color: signColor(k, p.profit, zero: k.fg)),
              ),
              const SizedBox(height: 6),
              TraderButton(account: a),
            ],
          ),
        ],
      ),
    );
  }

  /// The web prints the volume as JavaScript does (0.5, 1, 0.25).
  static String _volume(double v) => v == v.truncateToDouble() ? '${v.toInt()}' : '$v';
}

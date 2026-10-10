// Wallet › Overview: port of apps/crm/components/wallet-live/wallet-page.tsx (LiveWalletPage) in the phone order:
//   the photo hero (Wallet, the short line, History / Deposit USDT; page_hero.dart), or the header for a white-label
//   broker
//   balance card (available, in progress, total)
//   quick actions (Deposit / Withdraw / Transfer)                      hidden for view-only and read-only sessions
//   KYC notice                                                         idem
//   in progress (deposits being confirmed, open withdrawals)
//   recent activity (8 rows, View all)
//   fund a trading account (live accounts, Top up)                     hidden for view-only and read-only sessions
//   updates (the wallet's notices, Mark all read)                      the account holder only
// Data: overview every 10 s, activity?limit=8 every 15 s, accounts every 15 s, notifications every 30 s.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/models/user.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'wallet_api.dart';
import 'widgets/wallet_ui.dart';

class WalletScreen extends ConsumerWidget {
  const WalletScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params).
  final Map<String, String> query;

  Future<void> _refresh(WidgetRef ref) async {
    refreshWalletData(ref);
    ref.invalidate(walletNoticesProvider);
    await Future.wait<void>([
      ref.read(walletOverviewEveryProvider(10000).future).then((_) {}, onError: (Object _) {}),
      ref.read(walletActivityPageProvider(kRecentActivity).future).then((_) {}, onError: (Object _) {}),
    ]);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final me = ref.watch(meProvider);
    // view-only logins (and read-only staff sessions) see balances and activity, never money actions
    final readOnly = me?.readOnly ?? true;
    final kyc = me?.kycStatus ?? KycStatus.unverified;
    final o = ref.watch(walletOverviewEveryProvider(10000));
    final act = ref.watch(walletActivityPageProvider(kRecentActivity));
    final overview = o.value;

    final hero = pageHero(
      context,
      ref,
      path: '/wallet',
      title: t('wallet.wallet'),
      lead: t('wallet.page.subtitle'),
      actions: [
        KHeroButton(label: t('wallet.history'), icon: LucideIcons.history, onPressed: () => context.go('/wallet/history')),
        if (!readOnly)
          KHeroButton(label: t('wallet.depositUsdt'), icon: LucideIcons.arrowDownToLine, primary: true, onPressed: () => context.go('/wallet/deposit')),
      ],
    );

    return KPageScroll(
      onRefresh: () => _refresh(ref),
      hero: hero,
      padding: const EdgeInsets.fromLTRB(KSpace.page, 18, KSpace.page, 24),
      children: [
        if (hero == null)
          WalletHeader(
            title: t('wallet.wallet'),
            subtitle: t('wallet.page.subtitle'),
            actions: [
              KButton(label: t('wallet.history'), icon: LucideIcons.history, variant: KButtonVariant.surface, onPressed: () => context.go('/wallet/history')),
              if (!readOnly) KButton(label: t('wallet.depositUsdt'), icon: LucideIcons.arrowDownToLine, onPressed: () => context.go('/wallet/deposit')),
            ],
          ),
        if (o.hasError && overview == null)
          WalletUnavailable(onRetry: () => ref.invalidate(walletOverviewEveryProvider(10000)), message: unavailableMessage(o.error, t))
        else ...[
          _BalanceCard(o: overview),
          if (!readOnly) ...[const SizedBox(height: kWalletGap), _QuickActions(kyc: kyc), KycNotice(status: kyc, top: kWalletGap)],
          if (overview != null) _InProgress(o: overview),
          const SizedBox(height: kWalletGap),
          _RecentActivity(act: act, readOnly: readOnly),
          if (!readOnly) ...[
            const SizedBox(height: kWalletGap),
            const _FundAccounts(),
            // wallet notices belong to the account holder, not to a view-only login
            if (me?.viewer == null) const _Updates(),
          ],
        ],
      ],
    );
  }
}

/* ------------------------------------------------------------------ balance */

class _BalanceCard extends StatelessWidget {
  const _BalanceCard({required this.o});
  final WalletOverviewData? o;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final b = o?.usdt;
    return KCard(
      padding: const EdgeInsets.fromLTRB(20, 20, 20, 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(t('wallet.balance.title'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 10),
          SizedBox(
            height: 42,
            child: Align(
              alignment: AlignmentDirectional.centerStart,
              child: b == null
                  ? const KSkeleton(width: 190, height: 36, radius: 12)
                  : FittedBox(
                      fit: BoxFit.scaleDown,
                      alignment: AlignmentDirectional.centerStart,
                      child: Text.rich(
                        TextSpan(
                          children: [
                            TextSpan(text: fmt(b.available)),
                            TextSpan(
                              text: ' USDT',
                              style: TextStyle(fontSize: 18, fontWeight: FontWeight.w500, color: k.fg3, letterSpacing: 0),
                            ),
                          ],
                        ),
                        textDirection: TextDirection.ltr,
                        style: context.text.moneyXL.copyWith(fontWeight: FontWeight.w300, fontSize: 38, letterSpacing: -1.2),
                      ),
                    ),
            ),
          ),
          const SizedBox(height: 8),
          Text(t('wallet.balance.sub'), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13)),
          const SizedBox(height: 18),
          Row(
            children: [
              Expanded(
                child: WalletTile(label: t('wallet.available'), value: b == null ? '—' : fmt(b.available)),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: WalletTile(
                  label: t('wallet.inProgress'),
                  labelWidget: Row(
                    children: [
                      Icon(LucideIcons.lock, size: 11, color: k.fg3),
                      const SizedBox(width: 4),
                      Flexible(
                        child: Text(
                          t('wallet.inProgress').toUpperCase(),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.micro.copyWith(color: k.fg3, letterSpacing: 0.6, fontWeight: FontWeight.w500),
                        ),
                      ),
                    ],
                  ),
                  value: b == null ? '—' : fmt(b.locked),
                ),
              ),
            ],
          ),
          const SizedBox(height: 8),
          WalletTile(label: t('common.total'), value: b == null ? '—' : fmt(b.total)),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ quick actions */

class _QuickActions extends StatelessWidget {
  const _QuickActions({required this.kyc});
  final KycStatus kyc;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final items = [
      ('/wallet/deposit', t('common.deposit'), t('wallet.quick.depositSub'), LucideIcons.arrowDownToLine, true),
      (
        '/wallet/withdraw',
        t('common.withdraw'),
        kyc == KycStatus.verified ? t('wallet.quick.withdrawSub') : t('wallet.quick.withdrawKyc'),
        LucideIcons.arrowUpFromLine,
        false,
      ),
      ('/wallet/transfer', t('common.transfer'), t('wallet.quick.transferSub'), LucideIcons.arrowLeftRight, false),
    ];
    return Column(
      children: [
        for (var i = 0; i < items.length; i++) ...[
          if (i > 0) const SizedBox(height: 10),
          KCard(
            padding: const EdgeInsets.fromLTRB(16, 14, 14, 14),
            onTap: () => context.go(items[i].$1),
            child: Row(
              children: [
                Container(
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: items[i].$5 ? k.ember : k.surface2,
                    border: items[i].$5 ? null : Border.all(color: k.line),
                  ),
                  child: Icon(items[i].$4, size: 18, color: items[i].$5 ? k.onEmber : k.fg2),
                ),
                const SizedBox(width: 14),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(items[i].$2, style: context.text.headline.copyWith(fontWeight: FontWeight.w500)),
                      const SizedBox(height: 1),
                      Text(
                        items[i].$3,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                      ),
                    ],
                  ),
                ),
                Icon(rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 16, color: k.fg3),
              ],
            ),
          ),
        ],
      ],
    );
  }
}

/* ------------------------------------------------------------------ in progress */

class _InProgress extends StatelessWidget {
  const _InProgress({required this.o});
  final WalletOverviewData o;

  @override
  Widget build(BuildContext context) {
    if (o.pendingDeposits.isEmpty && o.openWithdrawals.isEmpty) return const SizedBox.shrink();
    final t = context.t;
    final k = context.k;
    final f = LocaleFormat(t.locale);
    final small = context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400);
    Widget head(String title, WalletStatus? st, String amount, Color? color) => Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Expanded(
          child: Wrap(
            spacing: 6,
            runSpacing: 4,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text(title, style: context.text.label.copyWith(fontSize: 13.5, fontWeight: FontWeight.w500)),
              if (st != null) WalletStatusChip(st),
            ],
          ),
        ),
        const SizedBox(width: 8),
        Text(
          amount,
          textDirection: TextDirection.ltr,
          style: context.text.label.copyWith(fontSize: 14, fontWeight: FontWeight.w600, color: color ?? k.fg, fontFeatures: kTabular),
        ),
      ],
    );
    return Padding(
      padding: const EdgeInsets.only(top: kWalletGap),
      child: KCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            KCardHeader(title: t('wallet.inProgress'), subtitle: t('wallet.progress.subtitle')),
            const SizedBox(height: 14),
            for (final d in o.pendingDeposits) ...[
              WalletRow(
                onTap: () => context.go(d.intentId != null ? '/wallet/deposit?intent=${d.intentId}' : '/wallet/history?type=deposit'),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    head(
                      t('wallet.depositLine', {'network': chainLabel(d.chain).short}),
                      kDepositStatus[d.status],
                      '+${fmt(d.amount ?? d.expectedAmount)} USDT',
                      k.up,
                    ),
                    const SizedBox(height: 4),
                    Row(
                      children: [
                        Text('${f.dateTime(d.createdAt)} · ', style: small),
                        Flexible(
                          child: HashLink(hash: d.txHash, url: d.explorerUrl, size: 11.5),
                        ),
                      ],
                    ),
                    if (d.status != 'review') ...[
                      const SizedBox(height: 10),
                      Confirmations(confirmations: d.confirmations, required: d.requiredConfirmations, status: d.status),
                    ],
                  ],
                ),
              ),
              const SizedBox(height: 8),
            ],
            for (final w in o.openWithdrawals) ...[
              WalletRow(
                onTap: () => context.go('/wallet/withdraw'),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    head(t('wallet.withdrawalLine', {'network': chainLabel(w.chain).short}), kWithdrawalStatus[w.status], '−${fmt(w.amount)} USDT', null),
                    const SizedBox(height: 4),
                    Text('${f.dateTime(w.createdAt)} · ${t('wallet.progress.withdrawalNet', {'net': fmt(w.netAmount), 'fee': fmt(w.fee)})}', style: small),
                  ],
                ),
              ),
              const SizedBox(height: 8),
            ],
          ],
        ),
      ),
    );
  }
}

/* ------------------------------------------------------------------ recent activity */

class _RecentActivity extends StatelessWidget {
  const _RecentActivity({required this.act, required this.readOnly});
  final AsyncValue<WalletPage<ActivityItem>> act;
  final bool readOnly;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final data = act.value;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('wallet.recent.title'),
            subtitle: data == null ? null : t('wallet.recent.count', {'count': data.total}),
            action: KButton(
              label: t('common.viewAll'),
              trailingIcon: rtl ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight,
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => context.go('/wallet/history'),
            ),
          ),
          const SizedBox(height: 14),
          if (data == null && !act.hasError) const KSkeleton(height: 64, radius: 14),
          if (data != null && data.items.isEmpty)
            KEmptyState(
              compact: true,
              art: KIllustrationName.emptyHistory,
              title: t('wallet.recent.emptyTitle'),
              text: t('wallet.recent.emptyText'),
              action: readOnly
                  ? null
                  : KButton(label: t('wallet.recent.firstDeposit'), variant: KButtonVariant.ink, onPressed: () => context.go('/wallet/deposit')),
            ),
          if (data != null)
            for (var i = 0; i < data.items.length; i++) ...[if (i > 0) const SizedBox(height: 8), ActivityRow(data.items[i])],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ fund a trading account */

class _FundAccounts extends ConsumerWidget {
  const _FundAccounts();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final data = ref.watch(walletAccountsProvider(15000)).value;
    final live = (data ?? const <EngineAccount>[]).where((a) => a.live && !a.prop).toList();
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('wallet.fundTradingAccount'),
            subtitle: t('wallet.fund.subtitle'),
            action: KButton(
              label: t('common.transfer'),
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => context.go('/wallet/transfer'),
            ),
          ),
          const SizedBox(height: 14),
          if (data != null && live.isEmpty)
            WalletRow(
              padding: const EdgeInsets.all(14),
              child: KRichText(
                t('wallet.fund.noLive'),
                style: context.text.callout.copyWith(color: k.fg2, fontSize: 13),
                tags: {'link': KTag.link(() => context.go('/accounts/new?type=live'))},
              ),
            ),
          if (data == null) const KSkeleton(height: 64, radius: 14),
          for (var i = 0; i < live.length; i++) ...[
            if (i > 0) const SizedBox(height: 8),
            WalletRow(
              onTap: () => context.go('/wallet/transfer?to=${live[i].login}'),
              child: Row(
                children: [
                  const LiveBadge(),
                  const SizedBox(width: 12),
                  Expanded(child: _AccountLines(a: live[i])),
                  const SizedBox(width: 8),
                  Text(t('wallet.fund.topUp'), style: context.text.label.copyWith(fontSize: 12.5, color: k.fg3)),
                  Icon(rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 16, color: k.fg3),
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

/// "Pro #10042817" and "Balance $12,480.55" (cent accounts: "USC … · ≈ $…").
class _AccountLines extends StatelessWidget {
  const _AccountLines({required this.a});
  final EngineAccount a;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final cent = a.cent || a.currency == 'USC';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text.rich(
          TextSpan(
            children: [
              TextSpan(text: '${a.groupName} '),
              TextSpan(
                text: '#${a.login}',
                style: context.text.mono(12, color: k.fg3),
              ),
            ],
          ),
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: context.text.label.copyWith(fontSize: 13.5, fontWeight: FontWeight.w500),
        ),
        Text(
          '${t('common.balance')} ${cent ? 'USC ' : r'$'}${fmt(a.balance)}${cent ? ' · ≈ \$${fmt(balanceUsd(a))}' : ''}',
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ updates */

class _Updates extends ConsumerWidget {
  const _Updates();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final data = ref.watch(walletNoticesProvider).value;
    final items = data?.items.take(5).toList() ?? const <WalletNotice>[];
    if (items.isEmpty) return const SizedBox.shrink();
    final f = LocaleFormat(t.locale);
    return Padding(
      padding: const EdgeInsets.only(top: kWalletGap),
      child: KCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            KCardHeader(
              icon: LucideIcons.bell,
              title: t('wallet.updates.title'),
              subtitle: data!.unread > 0 ? t('wallet.updates.new', {'count': data.unread}) : t('wallet.updates.allRead'),
              action: data.unread > 0
                  ? KButton(
                      label: t('shell.markAllRead'),
                      variant: KButtonVariant.ghost,
                      size: KButtonSize.sm,
                      onPressed: () async {
                        try {
                          await ref.read(apiProvider).post<Object?>('wallet/notifications/read', body: const <String, Object?>{});
                        } catch (_) {
                          // the list reloads either way (web: .catch(() => {}))
                        }
                        ref.invalidate(walletNoticesProvider);
                      },
                    )
                  : null,
            ),
            const SizedBox(height: 10),
            for (final n in items)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 7),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Container(
                      width: 6,
                      height: 6,
                      margin: const EdgeInsets.only(top: 6),
                      decoration: BoxDecoration(shape: BoxShape.circle, color: n.read ? Colors.transparent : k.ember),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(n.title, style: context.text.label.copyWith(fontWeight: FontWeight.w500)),
                          Text(n.body, style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)),
                          const SizedBox(height: 2),
                          Text(
                            f.dateTime(n.createdAt),
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 11),
                          ),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
          ],
        ),
      ),
    );
  }
}

// Dashboard › Overview, on real data. Port of the web's phone layout (apps/crm/components/dashboard/live-dashboard.tsx
// in OverviewLayout's phone order, max-md:order-*):
//   0 targeted banner (growth, placement "dashboard")
//   1 header (Overview + greeting + Verified), then Ask Kalks AI (the pill; not for view-only / read-only sessions)
//   2 total balance + Deposit / Withdraw / Transfer
//   3 KPI cards (equity, today's P&L, wallet, rewards)
//   4 your accounts (carousel, details, Trade / Fund or Refill / ⋯)
//   5 quick actions
//   6 statistics (equity / P&L curve)
//   7 notifications (prompts + latest)
//   8 activity tabs (History / Funding / Linked)
//   9 getting started checklist
//   then Markets (movers, heatmap, calendar, news, world) and More for you (Kalks Trader, your account, market clock,
//   support).
// The broker's module switches (AppConfig.modules) take out everything that leads into a switched-off module: the
// wallet's money actions, KPI, prompt, step and activity, rewards, copy trading, partner, Ask Kalks AI (`ai`; without
// `support_chat` it has no way into the chat), the Markets section's cards (markets, calendar, news) and the hero's
// options pill (web live-dashboard.tsx `on(...)`).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/models/trading.dart';
import '../../core/models/user.dart';
import '../../core/models/wallet.dart';
import '../../core/notifications/notifications.dart';
import '../../core/prefs.dart';
import '../../data/client_data.dart';
import '../../i18n/i18n.dart';
import '../../shell/nav.dart';
import '../../ui/ui.dart';
import '../accounts/account_actions.dart';
import '../support/ask_ai.dart';
import 'dashboard_data.dart';
import 'dashboard_hero.dart';
import 'widgets/accounts_panel.dart';
import 'widgets/balance_panel.dart';
import 'widgets/banner_slot.dart';
import 'widgets/list_cards.dart';
import 'widgets/markets_cards.dart';
import 'widgets/more_cards.dart';
import 'widgets/notifications_panel.dart';
import 'widgets/statistic_card.dart';

/// Whether balances are hidden (the dashboard's eye), remembered on the device.
class HideBalances extends Notifier<bool> {
  @override
  bool build() => ref.read(prefsProvider).hideBalances;
  void toggle() {
    state = !state;
    unawaited(ref.read(prefsProvider).setHideBalances(state));
  }
}

final hideBalancesProvider = NotifierProvider<HideBalances, bool>(HideBalances.new);

/// The "Verify your identity" step from the real KYC status (web kycStep).
({String state, String text}) kycStep(SessionUser me, T t) {
  if (me.kycStatus == KycStatus.verified) return (state: 'done', text: t('dashboard.steps.kyc.verified'));
  switch (me.kycCaseStatus) {
    case 'more_info':
      return (state: 'todo', text: t('dashboard.steps.kyc.moreInfo'));
    case 'submitted':
    case 'in_review':
      return (state: 'review', text: t('dashboard.steps.kyc.review'));
    case 'draft':
      return (state: 'todo', text: t('dashboard.steps.kyc.draft'));
    case 'rejected':
      return (state: 'rejected', text: t('dashboard.steps.kyc.rejected'));
  }
  if (me.kycStatus == KycStatus.pending) return (state: 'review', text: t('dashboard.steps.kyc.review'));
  if (me.kycStatus == KycStatus.rejected) return (state: 'rejected', text: t('dashboard.steps.kyc.rejected'));
  return (state: 'todo', text: t('dashboard.steps.kyc.todo'));
}

/// A step of "Getting started" (web Step).
typedef DashStep = ({String key, IconData icon, String title, String text, String state, String? href});

/// "Fund your wallet" from the real wallet (web walletStep).
DashStep walletStep(WalletOverview? o, T t) {
  final total = o?.usdt.total ?? 0;
  final title = t('wallet.onboarding.title');
  if (total > 0) {
    return (
      key: 'wallet',
      icon: LucideIcons.wallet,
      title: title,
      text: t('wallet.onboarding.done', {'amount': Fmt.amount(total)}),
      state: 'done',
      href: '/wallet/transfer',
    );
  }
  if (o != null && o.pendingDeposits.isNotEmpty) {
    return (key: 'wallet', icon: LucideIcons.wallet, title: title, text: t('wallet.onboarding.review'), state: 'review', href: '/wallet');
  }
  return (key: 'wallet', icon: LucideIcons.wallet, title: title, text: t('wallet.onboarding.todo'), state: 'todo', href: '/wallet/deposit');
}

/// Every step driven by the real client record (web steps()); "Fund your wallet" only with the wallet module on.
List<DashStep> dashboardSteps(SessionUser me, List<EngineAccount>? accounts, WalletOverview? wallet, T t, LocaleFormat f, {bool walletOn = true}) {
  final live = accounts?.where((a) => a.live).length ?? 0;
  final demo = accounts?.where((a) => !a.live).length ?? 0;
  final opened = live + demo > 0;
  final kyc = kycStep(me, t);
  return [
    (
      key: 'account',
      icon: LucideIcons.userRound,
      title: t('dashboard.steps.account.title'),
      text: t('dashboard.steps.account.text', {'date': f.date(me.createdAt)}),
      state: 'done',
      href: null,
    ),
    (
      key: 'email',
      icon: LucideIcons.mail,
      title: t('dashboard.steps.email.title'),
      text: me.emailVerified ? t('dashboard.steps.email.verified', {'email': me.email}) : t('dashboard.steps.email.confirm', {'email': me.email}),
      state: me.emailVerified ? 'done' : 'todo',
      href: null,
    ),
    (key: 'kyc', icon: LucideIcons.idCard, title: t('dashboard.steps.kyc.title'), text: kyc.text, state: kyc.state, href: '/profile/verification'),
    (
      key: 'account-open',
      icon: LucideIcons.layers,
      title: t('dashboard.steps.accountOpen.title'),
      text: opened ? t('dashboard.steps.accountOpen.opened', {'live': live, 'demo': demo, 'count': live + demo}) : t('dashboard.steps.accountOpen.todo'),
      state: opened ? 'done' : 'todo',
      href: opened ? '/accounts' : '/accounts/new',
    ),
    if (walletOn) walletStep(wallet, t),
  ];
}

/// Step state -> chip (web STATE_CHIP) and row tone (STEP_TONE).
const Map<String, (KChipTone, String)> _stateChip = {
  'done': (KChipTone.up, 'Done'),
  'todo': (KChipTone.ember, 'To do'),
  'review': (KChipTone.warn, 'In review'),
  'rejected': (KChipTone.down, 'Rejected'),
  'soon': (KChipTone.neutral, 'Not started'),
};
const Map<String, KTone> _stepTone = {'account': KTone.accent, 'email': KTone.sky, 'kyc': KTone.amber, 'account-open': KTone.lavender, 'wallet': KTone.mint};

String _greeting(DateTime now) => now.hour < 12 ? 'morning' : (now.hour < 18 ? 'afternoon' : 'evening');

/// The sheet's side gutters.
const double _gutter = 20;

/// The greeting as the page title (web: the Overview title + "Good afternoon, Arjun"): the greeting light, the name
/// bold on its own line, the Verified chip beside it. The web's sentence is split at its {name} placeholder so every
/// language keeps its own word order.
class _Greeting extends StatelessWidget {
  const _Greeting({required this.template, required this.name, required this.verified});
  final String template;
  final String name;
  final bool verified;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final i = template.indexOf('{name}');
    final before = (i < 0 ? template : template.substring(0, i)).trim();
    final after = i < 0 ? '' : template.substring(i + '{name}'.length).trim();
    final light = context.text.largeTitle.copyWith(fontSize: 30, fontWeight: FontWeight.w300, color: k.fg2, height: 1.15, letterSpacing: -0.5);
    final bold = light.copyWith(fontWeight: FontWeight.w700, color: k.fg);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (before.isNotEmpty) Text(before, style: light),
        Wrap(
          spacing: 10,
          runSpacing: 6,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: [
            Text(name, style: bold),
            if (verified) KChip(label: t('common.verified'), tone: KChipTone.up, icon: LucideIcons.badgeCheck, small: true),
          ],
        ),
        if (after.isNotEmpty) Text(after, style: light),
      ],
    );
  }
}

class DashboardScreen extends ConsumerWidget {
  const DashboardScreen({super.key});

  Future<void> _refresh(WidgetRef ref) async {
    ref
      ..invalidate(accountsProvider)
      ..invalidate(walletOverviewProvider)
      ..invalidate(walletActivityProvider)
      ..invalidate(walletConfigProvider)
      ..invalidate(rewardsProvider)
      ..invalidate(equityCurveProvider)
      ..invalidate(dashBannersProvider)
      ..invalidate(dashNewsProvider)
      ..invalidate(dashCalendarProvider)
      ..invalidate(dashMapProvider);
    await Future.wait<void>([
      ref.read(accountsProvider.future).then((_) {}, onError: (Object _) {}),
      ref.read(walletOverviewProvider.future).then((_) {}, onError: (Object _) {}),
      ref.read(notificationsProvider.notifier).load(),
    ]);
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final me = ref.watch(meProvider);
    if (me == null) return const SizedBox.shrink();
    final readOnly = me.readOnly;
    final hidden = ref.watch(hideBalancesProvider);
    final f = LocaleFormat(t.locale);
    // the broker's module switches: nothing below leads into a switched-off module (nor loads its data)
    final config = ref.watch(configProvider);
    final walletOn = config.moduleOn('wallet');
    final rewardsOn = config.moduleOn('rewards');
    final copyOn = pageOn(config, '/social');
    final partnerOn = pageOn(config, '/partner');
    final marketsOn = pageOn(config, '/markets');
    final calendarOn = pageOn(config, '/calendar');
    final newsOn = pageOn(config, '/news');
    final aiOn = config.moduleOn('ai');

    final acc = ref.watch(accountsProvider);
    final all = acc.value;
    // archived / closed accounts live on the Accounts page's Archived tab only
    final accounts = all?.where((a) => !a.archived).toList();
    final totals = AccountTotals(accounts ?? const []);
    final wallet = walletOn ? ref.watch(walletOverviewProvider).value : null;
    final walletTotal = wallet?.usdt.total;
    final rewards = rewardsOn ? ref.watch(rewardsProvider).value : null;

    // today's P&L: today's equity move net of deposits / withdrawals (reports), else the live accounts' floating P&L
    final week = ref.watch(equityCurveProvider(14)).value;
    double? today, todayPct;
    if (week != null && week.length > 1) {
      final last = week[week.length - 1], prev = week[week.length - 2];
      today = last.equity - prev.equity - last.flow;
      todayPct = prev.equity > 0 ? today / prev.equity * 100 : null;
    }
    final hasLive = totals.live.isNotEmpty;

    // carousel: CFD accounts, then Options accounts; each live first, then demo, then prop
    final byKind = [...totals.live, ...totals.demo, ...?accounts?.where((a) => a.prop)];
    final ordered = [...byKind.where((a) => !a.isOptions), ...byKind.where((a) => a.isOptions)];

    // prompts: verification, then funding
    final kyc = kycStep(me, t);
    final prompts = <DashboardPrompt>[
      if (!readOnly && kyc.state != 'done')
        DashboardPrompt(
          id: 'kyc-${kyc.state}',
          title: t('dashboard.steps.kyc.title'),
          text: kyc.text,
          icon: LucideIcons.idCard,
          tone: kyc.state == 'rejected' ? KTone.coral : KTone.amber,
          actionLabel: kyc.state == 'review' ? t('common.details') : t('dashboard.home.verifyNow'),
          href: '/profile/verification',
        ),
      if (!readOnly && wallet != null && walletTotal == 0 && wallet.pendingDeposits.isEmpty)
        DashboardPrompt(
          id: 'fund',
          title: t('dashboard.home.fundTitle'),
          text: t('dashboard.home.fundText'),
          icon: LucideIcons.wallet,
          tone: KTone.mint,
          actionLabel: t('dashboard.home.depositNow'),
          href: '/wallet/deposit',
        ),
    ];

    // Getting started
    final steps = dashboardSteps(me, accounts, wallet, t, f, walletOn: walletOn);
    final done = steps.where((s) => s.state == 'done').length;
    final checklist = [
      for (final s in steps)
        ListRowItem(
          key: s.key,
          icon: s.icon,
          tone: _stepTone[s.key] ?? KTone.accent,
          title: s.title,
          sub: s.text,
          done: s.state == 'done',
          href: s.href,
          status: s.state == 'todo' && s.href != null
              ? null
              : (label: t.dyn('dashboard.steps.state.${s.state}', fallback: _stateChip[s.state]!.$2), tone: _stateChip[s.state]!.$1),
          action: s.state == 'todo' && s.href != null && !readOnly ? (label: t('common.continue'), href: s.href!, external: false) : null,
        ),
    ];

    // activity tabs (History and Funding are the wallet's)
    final activity = walletOn ? ref.watch(walletActivityProvider) : const AsyncValue<List<WalletActivity>>.data([]);
    final cfg = walletOn ? ref.watch(walletConfigProvider) : const AsyncValue<List<ChainConfig>>.data([]);
    final historyRows = activity.hasValue ? [for (final x in activity.value!) _historyRow(context, x, f)] : (activity.hasError ? const <ListRowItem>[] : null);
    final fundingRows = cfg.hasValue
        ? [
            for (final c in cfg.value!)
              ListRowItem(
                key: c.chain,
                tone: KTone.neutral,
                leading: _NetworkCoin(chain: c.chain),
                title: '${c.token} · ${c.network}',
                sub: t('wallet.deposit.amountHint', {'min': Fmt.amount(c.minDeposit)}),
                status: c.depositsEnabled
                    ? (label: t('dashboard.home.connected'), tone: KChipTone.ember)
                    : (label: t('dashboard.home.networkUnavailable'), tone: KChipTone.neutral),
              ),
          ]
        : (cfg.hasError ? const <ListRowItem>[] : null);
    final linkedRows = [
      ListRowItem(
        key: 'trader',
        icon: LucideIcons.candlestickChart,
        tone: KTone.accent,
        title: 'Kalks Trader',
        sub: t('dashboard.trader.chip'),
        action: (label: t('common.open'), href: '/trader', external: false),
      ),
      if (copyOn)
        ListRowItem(
          key: 'copy',
          icon: LucideIcons.copy,
          tone: KTone.pink,
          title: t('shell.nav.copyTrading'),
          sub: t('shell.nav.social'),
          action: (label: t('common.open'), href: '/social', external: false),
        ),
      if (partnerOn)
        ListRowItem(
          key: 'ib',
          icon: LucideIcons.award,
          tone: KTone.amber,
          title: t('shell.nav.partner'),
          sub: t('dashboard.partner.chip'),
          action: (label: t('common.open'), href: '/partner', external: false),
        ),
      if (rewardsOn)
        ListRowItem(
          key: 'loyalty',
          icon: LucideIcons.gift,
          tone: KTone.lavender,
          title: t('shell.nav.loyalty'),
          sub: rewards != null ? t('dashboard.home.points', {'points': Fmt.number(rewards.points, 0)}) : t('shell.nav.rewards'),
          status: rewards != null ? (label: rewards.tierName, tone: KChipTone.ember) : null,
          action: rewards == null ? (label: t('common.open'), href: '/rewards/loyalty', external: false) : null,
        ),
    ];

    // Ask Kalks AI: suggestions answered by the real support bot; account questions also show the client's own figures
    final liveAccts = totals.live;
    final aiChips = [
      if (walletOn)
        AiChip(
          key: 'deposit',
          label: t('dashboard.ai.chip.deposit'),
          extra: AiLink(href: '/wallet/deposit', label: t('common.deposit')),
        ),
      AiChip(
        key: 'freeMargin',
        label: t('dashboard.ai.chip.freeMargin'),
        extra: AiFacts(
          title: t('dashboard.ai.yourAccounts'),
          rows: [for (final a in liveAccts) (label: '#${a.login} · ${a.groupName}', value: a.money(a.freeMargin), tone: null)],
        ),
      ),
      AiChip(
        key: 'marginLevel',
        label: t('dashboard.ai.chip.marginLevel'),
        extra: AiFacts(
          title: t('dashboard.ai.yourAccounts'),
          rows: [for (final a in liveAccts) (label: '#${a.login} · ${a.groupName}', value: fmtLevel(a.marginLevel), tone: levelTone(a.marginLevel))],
        ),
      ),
      AiChip(
        key: 'openAccount',
        label: t('dashboard.ai.chip.openAccount'),
        question: t('dashboard.ai.q.openAccount'),
        extra: AiLink(href: '/accounts/new', label: t('dashboard.accounts.open')),
      ),
    ];

    final width = MediaQuery.sizeOf(context).width;
    final kpiWidth = (width - 2 * _gutter) * 0.78;
    final rtl = Directionality.of(context) == TextDirection.rtl;

    // the module's pages under the sheet's grabber (the shell's sub-nav, in the sheet while the picture shows)
    final subs = moduleOf(navFor(config, me), '/')?.sub ?? const <NavSub>[];
    final hero = dashboardHeroAt('/', config)
        ? KPageHero(
            height: dashboardHeroHeight(MediaQuery.of(context)),
            picture: const DashboardHeroPicture(),
            child: const DashboardHeroCopy(),
            top: subs.length > 1
                ? KPillNav(
                    labels: [for (final s in subs) t(s.labelKey)],
                    icons: [for (final s in subs) s.icon],
                    current: subs.indexOf(activeSub('/', subs) ?? subs.first),
                    onSelect: (i) => context.go(subs[i].href),
                  )
                : null,
          )
        : null;

    return KCardTheme(
      radius: 28,
      border: false,
      padding: const EdgeInsets.all(20),
      shadows: const [BoxShadow(color: Color.fromRGBO(20, 10, 5, 0.06), offset: Offset(0, 10), blurRadius: 30)],
      child: KPageScroll(
        onRefresh: () => _refresh(ref),
        padding: const EdgeInsets.fromLTRB(_gutter, 16, _gutter, 24),
        hero: hero,
        children: [
          // 0. targeted banner
          const DashboardBannerSlot(),
          // 1. header: the greeting as the title, the name in bold
          _Greeting(template: t.dyn('dashboard.greeting.${_greeting(DateTime.now())}'), name: me.firstName, verified: me.kycStatus == KycStatus.verified),
          // 1b. Ask Kalks AI
          if (!readOnly && aiOn) ...[const SizedBox(height: 16), AskAi(chips: aiChips, chat: config.moduleOn('support_chat'))],
          const SizedBox(height: 24),
          // 2. total balance
          BalancePanel(
            total: accounts != null || walletTotal != null ? totals.equity + (walletTotal ?? 0) : null,
            loading: accounts == null && !acc.hasError,
            changePct: todayPct,
            readOnly: readOnly,
            hidden: hidden,
            wallet: walletOn,
          ),
          const SizedBox(height: 24),
          // 3. KPI cards
          SizedBox(
            height: 186,
            child: ListView(
              scrollDirection: Axis.horizontal,
              clipBehavior: Clip.none,
              physics: const PageScrollPhysics(parent: BouncingScrollPhysics()),
              children: [
                KKpiCard(
                  width: kpiWidth,
                  label: t('dashboard.equity.title'),
                  icon: LucideIcons.trendingUp,
                  value: accounts == null ? const Text('—') : KMoney(totals.equity, style: context.text.moneyL, hidden: hidden),
                  chip: KChip(
                    label: hasLive
                        ? t('dashboard.home.accountsChip', {'live': totals.live.length, 'positions': totals.positions})
                        : t('dashboard.accounts.openLive.title'),
                  ),
                  onTap: () => context.go('/accounts'),
                ),
                const SizedBox(width: 12),
                KKpiCard(
                  width: kpiWidth,
                  label: today != null ? t('dashboard.home.todayPnl') : t('dashboard.home.floating'),
                  icon: LucideIcons.chartLine,
                  value: accounts == null
                      ? const Text('—')
                      : KMoney(today ?? totals.profit, signed: true, tone: KMoneyTone.auto, style: context.text.moneyL, hidden: hidden),
                  chip: todayPct == null
                      ? null
                      : KChip(
                          label: t('dashboard.home.todayPct', {'pct': '${todayPct >= 0 ? '+' : ''}${todayPct.toStringAsFixed(2)}'}),
                          tone: (today ?? totals.profit) >= 0 ? KChipTone.up : KChipTone.down,
                        ),
                  onTap: () => context.go('/portfolio/analytics'),
                ),
                if (walletOn) ...[
                  const SizedBox(width: 12),
                  KKpiCard(
                    width: kpiWidth,
                    label: t('dashboard.home.walletBalance'),
                    icon: LucideIcons.wallet,
                    value: walletTotal == null ? const Text('—') : KMoney(walletTotal, style: context.text.moneyL, hidden: hidden),
                    footer: Row(
                      children: [
                        const KCoinIcon('usdt', size: 20),
                        const SizedBox(width: 8),
                        Text(
                          'USDT · TRC20 · BEP20',
                          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w600),
                        ),
                      ],
                    ),
                    onTap: () => context.go('/wallet'),
                  ),
                ],
                if (rewardsOn) ...[
                  const SizedBox(width: 12),
                  KKpiCard(
                    width: kpiWidth,
                    label: t('dashboard.home.rewards'),
                    icon: LucideIcons.award,
                    value: rewards == null ? const Text('—') : KMoney(rewards.points * rewards.pointValue, style: context.text.moneyL, hidden: hidden),
                    chip: KChip(
                      label: rewards == null ? t('shell.nav.loyalty') : t('dashboard.home.points', {'points': Fmt.number(rewards.points, 0)}),
                      tone: KChipTone.gold,
                    ),
                    onTap: () => context.go('/rewards/loyalty'),
                  ),
                ],
              ],
            ),
          ),
          const SizedBox(height: 24),
          // 4. your accounts
          AccountsPanel(
            accounts: accounts == null ? null : ordered.take(8).toList(),
            loading: !acc.hasValue && !acc.hasError,
            failed: acc.hasError,
            onRetry: () => ref.invalidate(accountsProvider),
            hidden: hidden,
            onToggleHidden: ref.read(hideBalancesProvider.notifier).toggle,
            extraCount: (ordered.length - 8).clamp(0, 1 << 20),
            readOnly: readOnly,
            actions: readOnly
                ? null
                : (a) => Row(
                    children: [
                      Expanded(child: TradeButton(account: a, expand: true)),
                      if (a.live ? !a.prop && walletOn : true) const SizedBox(width: 8),
                      if (a.live && !a.prop && walletOn)
                        FundButton(account: a)
                      else if (!a.live)
                        RefillButton(account: a, onDone: () => ref.invalidate(accountsProvider)),
                      const SizedBox(width: 4),
                      AccountMenuButton(account: a, onChanged: () => ref.invalidate(accountsProvider)),
                    ],
                  ),
          ),
          const SizedBox(height: 24),
          // 5. quick actions
          QuickActions(
            items: [
              if (walletOn)
                (
                  label: t('common.transfer'),
                  icon: rtl ? LucideIcons.arrowRightLeft : LucideIcons.arrowLeftRight,
                  tone: KTone.lavender,
                  onTap: () => context.go('/wallet/transfer'),
                ),
              (label: 'Kalks Trader', icon: LucideIcons.candlestickChart, tone: KTone.accent, onTap: () => context.push('/trader')),
              if (copyOn) (label: t('shell.nav.copyTrading'), icon: LucideIcons.copy, tone: KTone.pink, onTap: () => context.go('/social')),
              (label: t('shell.nav.support'), icon: LucideIcons.lifeBuoy, tone: KTone.amber, onTap: () => context.go('/support')),
            ],
          ),
          const SizedBox(height: 24),
          // 6. statistics
          const _StatisticSection(),
          const SizedBox(height: 24),
          // 7. notifications
          if (!readOnly) ...[NotificationsPanel(prompts: prompts), const SizedBox(height: 24)],
          // 8. activity tabs
          ActivityTabs(
            tabs: [
              if (walletOn) ...[
                ActivityTab(
                  key: 'history',
                  label: t('dashboard.home.history'),
                  rows: historyRows,
                  empty: t('wallet.recent.emptyText'),
                  more: (label: t('common.viewAll'), href: '/wallet/history'),
                ),
                ActivityTab(key: 'funding', label: t('dashboard.home.funding'), rows: fundingRows, empty: t('wallet.recent.emptyText')),
              ],
              ActivityTab(key: 'linked', label: t('dashboard.home.linked'), rows: linkedRows, empty: ''),
            ],
          ),
          const SizedBox(height: 24),
          // 9. getting started
          ChecklistCard(title: t('dashboard.steps.title'), subtitle: t('dashboard.steps.subtitle'), rows: checklist, done: done, total: steps.length),
          // Markets: each card with its module (Markets, Calendar, News)
          if (marketsOn || calendarOn || newsOn) ...[
            const SizedBox(height: 36),
            KSectionTitle(t('dashboard.home.marketsTitle'), large: true),
            ..._spaced([
              if (marketsOn) ...[
                FeedGuard(title: t('dashboard.movers.title'), minHeight: 320, child: const MoversCard()),
                FeedGuard(title: t('dashboard.heatmap.title'), minHeight: 320, child: const HeatmapCard()),
              ],
              if (calendarOn) const CalendarCard(),
              if (newsOn) ...[const NewsCard(), const WorldCard()],
            ], first: 16),
          ],
          // More for you
          const SizedBox(height: 36),
          KSectionTitle(t('dashboard.home.moreTitle'), large: true),
          const SizedBox(height: 16),
          const TraderBanner(),
          const SizedBox(height: 24),
          const AccountCard(),
          const SizedBox(height: 24),
          const SessionsCard(),
          const SizedBox(height: 24),
          const SupportCard(),
        ],
      ),
    );
  }

  /// The cards of a section with the page's gaps between them (`first` above the first one).
  static List<Widget> _spaced(List<Widget> cards, {required double first}) => [
    for (var i = 0; i < cards.length; i++) ...[SizedBox(height: i == 0 ? first : 24), cards[i]],
  ];

  ListRowItem _historyRow(BuildContext context, WalletActivity x, LocaleFormat f) {
    final t = context.t;
    final k = context.k;
    final (icon, tone) = switch (x.type) {
      'deposit' => (LucideIcons.arrowDownToLine, KTone.mint),
      'withdrawal' => (LucideIcons.arrowUpFromLine, KTone.coral),
      'transfer' => (Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowRightLeft : LucideIcons.arrowLeftRight, KTone.lavender),
      _ => (LucideIcons.coins, KTone.amber),
    };
    final kind = x.kind == null ? null : _kindLabel[x.kind];
    final title = switch (x.type) {
      'deposit' => t('wallet.txType.deposit'),
      'withdrawal' => t('wallet.txType.withdrawal'),
      'transfer' => t('wallet.txType.transfer'),
      _ => kind != null ? t(kind) : t('wallet.activity.walletTx'),
    };
    final where = x.network != null ? ' · ${x.network}' : (x.login != null ? ' · #${x.login}' : '');
    return ListRowItem(
      key: '${x.type}-${x.id}',
      icon: icon,
      tone: tone,
      title: title,
      sub: '${f.dayMonth(x.createdAt)}$where',
      value: x.amount == null
          ? null
          : Text(
              '${x.direction == 'in' ? '+' : '-'}${Fmt.amount(x.amount)}',
              textDirection: TextDirection.ltr,
              style: TextStyle(color: x.direction == 'in' ? k.up : k.fg),
            ),
      href: '/wallet/history',
    );
  }
}

/// Wallet ledger kinds (web wallet-live/ui.tsx KIND_LABEL).
const Map<String, String> _kindLabel = {
  'commission': 'wallet.kind.commission',
  'ib_payout': 'wallet.kind.ibPayout',
  'prop_purchase': 'wallet.kind.propPurchase',
  'prop_payout': 'wallet.kind.propPayout',
  'pamm_invest': 'wallet.kind.pammInvest',
  'pamm_redeem': 'wallet.kind.pammRedeem',
  'copy_fee': 'wallet.kind.copyFee',
  'mam_fee': 'wallet.kind.mamFee',
  'adjustment': 'wallet.kind.adjustment',
  'adjustment_in': 'wallet.kind.adjustment',
  'adjustment_out': 'wallet.kind.adjustment',
  'manual_deposit': 'wallet.txType.deposit',
  'manual_withdrawal': 'wallet.txType.withdrawal',
  'refund': 'wallet.kind.refund',
};

/// USDT with the network's coin in the corner (BNB Chain / TRON).
class _NetworkCoin extends StatelessWidget {
  const _NetworkCoin({required this.chain});
  final String chain;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: 32,
    height: 30,
    child: Stack(
      clipBehavior: Clip.none,
      children: [
        const KCoinIcon('usdt'),
        PositionedDirectional(
          bottom: -2,
          end: -1,
          child: Container(
            padding: const EdgeInsets.all(1.5),
            decoration: BoxDecoration(color: context.k.surface, shape: BoxShape.circle),
            child: KCoinIcon(chain == 'bsc' ? 'bnb' : 'trx', size: 13),
          ),
        ),
      ],
    ),
  );
}

/// The Statistics card with its own Equity | P&L and range state (web StatisticCard + useCurve(2 × range)).
class _StatisticSection extends ConsumerStatefulWidget {
  const _StatisticSection();

  @override
  ConsumerState<_StatisticSection> createState() => _StatisticSectionState();
}

class _StatisticSectionState extends ConsumerState<_StatisticSection> {
  StatMode _mode = StatMode.equity;
  StatRange _range = StatRange.month;

  @override
  Widget build(BuildContext context) {
    final n = kRangeDays[_range]!;
    final chart = ref.watch(equityCurveProvider(2 * n));
    final curve = chart.value;
    final series = curve != null && curve.length > 1 ? toSeries(curve, n, _mode, _range) : null;
    return StatisticCard(
      mode: _mode,
      onMode: (m) => setState(() => _mode = m),
      range: _range,
      onRange: (r) => setState(() => _range = r),
      points: series?.points ?? (chart.isLoading ? null : const []),
      compare: series?.compare,
      loading: chart.isLoading && !chart.hasValue,
    );
  }
}

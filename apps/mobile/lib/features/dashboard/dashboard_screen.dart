// Dashboard › Overview (Home), on real data. Port of the web's Home as kept short by the founder on 2026-10-10
// (apps/crm/components/dashboard/live-dashboard.tsx):
//   the hero: Home's photo (the KALKS letters behind a figure) with only the Ask Kalks AI bar on it — no greeting, the
//     name is in the header (not for view-only / read-only sessions, nor with the `ai` module off);
//   the balance strip: total balance (with the eye that hides every amount), equity, wallet, today's P&L with the
//     last 7 days, open positions, rewards;
//   the targeted banner (growth, placement "dashboard");
//   the shortcuts (Deposit, Withdraw, Transfer, Open account, Kalks Trader, Markets, Options, Copy & PAMM, Prop,
//     Partner, Rewards, Academy, History, Statements, Verification, Support);
//   your accounts as debit cards (carousel, Trade / Fund or Refill / ⋯, the eye).
// The broker's module switches (AppConfig.modules) take out everything that leads into a switched-off module (web
// live-dashboard.tsx `on(...)`). A white-label broker gets no photo: the AI pill opens the page instead.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart' show DateFormat;
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/models/trading.dart';
import '../../core/models/user.dart';
import '../../core/models/wallet.dart';
import '../../core/notifications/notifications.dart';
import '../../data/client_data.dart';
import '../../i18n/i18n.dart';
import '../../shell/nav.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import '../accounts/account_actions.dart';
import '../common/hide_money.dart';
import '../support/ask_ai.dart';
import 'dashboard_data.dart';
import 'widgets/accounts_panel.dart';
import 'widgets/balance_strip.dart';
import 'widgets/banner_slot.dart';
import 'widgets/shortcuts.dart';

export '../common/hide_money.dart' show HideBalances, hideBalancesProvider;

/// The clock the greeting reads (tests pin it so their screenshots don't change with the time of day).
final greetingClockProvider = Provider<DateTime Function()>((ref) => DateTime.now);

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
    final me = ref.watch(meProvider);
    if (me == null) return const SizedBox.shrink();
    final readOnly = me.readOnly;
    final hidden = ref.watch(hideBalancesProvider);
    // the broker's module switches: nothing below leads into a switched-off module (nor loads its data)
    final config = ref.watch(configProvider);
    final walletOn = config.moduleOn('wallet');
    final rewardsOn = config.moduleOn('rewards');
    final aiOn = config.moduleOn('ai');

    final acc = ref.watch(accountsProvider);
    // archived / closed accounts live on the Accounts page's Archived tab only
    final accounts = acc.value?.where((a) => !a.archived).toList();
    final totals = AccountTotals(accounts ?? const []);
    final wallet = walletOn ? ref.watch(walletOverviewProvider).value : null;
    final walletTotal = wallet?.usdt.total;
    final rewards = rewardsOn ? ref.watch(rewardsProvider).value : null;

    // today's P&L: today's equity move net of deposits / withdrawals (reports), else the live accounts' floating P&L;
    // the bars: the daily P&L of the last 7 days
    final week = ref.watch(equityCurveProvider(14)).value;
    double? today;
    var daily = <({String day, double v})>[];
    if (week != null && week.length > 1) {
      final last = week[week.length - 1], prev = week[week.length - 2];
      today = last.equity - prev.equity - last.flow;
      final last8 = week.sublist(week.length > 8 ? week.length - 8 : 0);
      daily = [for (var i = 1; i < last8.length; i++) (day: last8[i].day, v: last8[i].equity - last8[i - 1].equity - last8[i].flow)];
    }
    if (daily.isEmpty) {
      final now = DateTime.now();
      daily = [for (var i = 6; i >= 0; i--) (day: DateFormat('yyyy-MM-dd').format(now.subtract(Duration(days: i))), v: 0.0)];
    }
    String weekday(String day) {
      final d = DateTime.tryParse('${day}T12:00:00');
      if (d == null) return '';
      try {
        return DateFormat('EEEEE', t.locale).format(d);
      } catch (_) {
        return DateFormat('EEEEE').format(d);
      }
    }

    final hasLive = totals.live.isNotEmpty;
    final total = accounts != null || walletTotal != null ? totals.equity + (walletTotal ?? 0) : null;

    // carousel: CFD accounts, then Options accounts; each live first, then demo, then prop
    final byKind = [...totals.live, ...totals.demo, ...?accounts?.where((a) => a.prop)];
    final ordered = [...byKind.where((a) => !a.isOptions), ...byKind.where((a) => a.isOptions)];

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
    final ai = !readOnly && aiOn;
    final chat = config.moduleOn('support_chat');

    // the hero: only the AI bar on the photo (no greeting; the name is in the header)
    final hero = pageHero(
      context,
      ref,
      path: '/',
      body: ai ? AskAi(chips: aiChips, chat: chat, hero: true) : null,
    );

    final rtl = Directionality.of(context) == TextDirection.rtl;
    void go(String href) => context.go(href);
    final shortcuts = <Shortcut>[
      if (walletOn && !readOnly) ...[
        (key: 'deposit', label: t('common.deposit'), icon: LucideIcons.arrowDownToLine, onTap: () => go('/wallet/deposit')),
        (key: 'withdraw', label: t('common.withdraw'), icon: LucideIcons.arrowUpFromLine, onTap: () => go('/wallet/withdraw')),
        (
          key: 'transfer',
          label: t('common.transfer'),
          icon: rtl ? LucideIcons.arrowRightLeft : LucideIcons.arrowLeftRight,
          onTap: () => go('/wallet/transfer'),
        ),
      ],
      if (!readOnly) (key: 'open', label: t('dashboard.accounts.open'), icon: LucideIcons.layers, onTap: () => go('/accounts/new')),
      (key: 'trader', label: 'Kalks Trader', icon: LucideIcons.candlestickChart, onTap: () => context.push('/trader')),
      if (pageOn(config, '/markets'))
        (key: 'markets', label: t.dyn('shell.nav.markets', fallback: 'Markets'), icon: LucideIcons.globe, onTap: () => go('/markets')),
      if (config.moduleOn('options'))
        (key: 'options', label: t.dyn('shell.nav.options', fallback: 'Options'), icon: LucideIcons.chartSpline, onTap: () => go('/options')),
      if (config.moduleOn('copy_trading') || config.moduleOn('pamm'))
        (key: 'copy', label: t.dyn('shell.nav.copyPamm', fallback: 'Copy & PAMM'), icon: LucideIcons.copy, onTap: () => go('/social')),
      if (config.moduleOn('prop'))
        (key: 'prop', label: t.dyn('shell.nav.propShort', fallback: 'Prop challenges'), icon: LucideIcons.trophy, onTap: () => go('/prop')),
      if (config.moduleOn('ib')) (key: 'ib', label: t('shell.nav.partner'), icon: LucideIcons.award, onTap: () => go('/partner')),
      if (rewardsOn) (key: 'rewards', label: t('shell.nav.rewards'), icon: LucideIcons.gift, onTap: () => go('/rewards')),
      if (config.moduleOn('academy'))
        (key: 'academy', label: t.dyn('shell.nav.academyShort', fallback: 'Academy'), icon: LucideIcons.graduationCap, onTap: () => go('/academy')),
      if (walletOn) (key: 'history', label: t('dashboard.home.history'), icon: LucideIcons.history, onTap: () => go('/wallet/history')),
      (key: 'statements', label: t.dyn('portfolio.st.title', fallback: 'Statements'), icon: LucideIcons.fileText, onTap: () => go('/portfolio/statements')),
      (key: 'kyc', label: t.dyn('shell.nav.verification', fallback: 'Verification'), icon: LucideIcons.idCard, onTap: () => go('/profile/verification')),
      (key: 'support', label: t('shell.nav.support'), icon: LucideIcons.lifeBuoy, onTap: () => go('/support')),
    ];

    Widget money(double v, TextStyle s, {bool signed = false}) => KMoney(v, style: s, signed: signed, tone: signed ? KMoneyTone.auto : null);
    final cells = <StripCell>[
      StripCell(
        key: 'total',
        big: true,
        secret: true,
        label: t('dashboard.home.totalBalance'),
        value: (s) => total == null ? Text('—', style: s) : money(total, s),
        sub: t.dyn('dashboard.hero.totalSub', fallback: 'Live accounts + wallet'),
        href: '/wallet',
      ),
      StripCell(
        key: 'equity',
        secret: true,
        label: t('dashboard.equity.title'),
        value: (s) => accounts == null ? Text('—', style: s) : money(totals.equity, s),
        sub: hasLive ? t('dashboard.home.accountsChip', {'live': totals.live.length, 'positions': totals.positions}) : t('dashboard.accounts.openLive.title'),
        href: '/accounts',
      ),
      if (walletOn)
        StripCell(
          key: 'wallet',
          secret: true,
          label: t('dashboard.home.walletBalance'),
          value: (s) => walletTotal == null ? Text('—', style: s) : Text(Fmt.amount(walletTotal), style: s, textDirection: TextDirection.ltr),
          unit: 'USDT',
          sub: 'TRC20 · BEP20',
          href: '/wallet',
        ),
      StripCell(
        key: 'pnl',
        secret: true,
        label: today != null ? t('dashboard.home.todayPnl') : t('dashboard.home.floating'),
        value: (s) => accounts == null ? Text('—', style: s) : money(today ?? totals.profit, s, signed: true),
        extra: MiniBars(values: [for (final d in daily) d.v], labels: [for (final d in daily) weekday(d.day)]),
        href: '/portfolio/analytics',
      ),
      StripCell(
        key: 'positions',
        label: t.dyn('dashboard.hero.openPositions', fallback: 'Open positions'),
        value: (s) => Text(accounts == null ? '—' : '${totals.positions}', style: s),
        sub: hasLive
            ? (totals.live.length == 1
                  ? t.dyn('dashboard.hero.onLiveOne', fallback: 'on your live account')
                  : t.dyn('dashboard.hero.acrossLive', fallback: 'across {count} live accounts', vars: {'count': totals.live.length}))
            : null,
        href: '/portfolio',
      ),
      if (rewardsOn)
        StripCell(
          key: 'rewards',
          label: t('dashboard.home.rewards'),
          value: (s) => Text(rewards == null ? '—' : Fmt.number(rewards.points, 0), style: s),
          unit: t.dyn('dashboard.hero.points', fallback: 'pts'),
          sub: rewards?.tierName,
          href: '/rewards/loyalty',
        ),
    ];

    return KPageScroll(
      onRefresh: () => _refresh(ref),
      padding: const EdgeInsets.fromLTRB(16, 18, 16, 28),
      hero: hero,
      children: [
        // a white-label broker: no photo, the AI pill opens the page
        if (hero == null && ai) ...[AskAi(chips: aiChips, chat: chat), const SizedBox(height: 18)],
        // the money summary
        BalanceStrip(cells: cells),
        // the targeted banner
        const DashboardBannerSlot(),
        const SizedBox(height: 28),
        // shortcuts
        KSectionTitle(t.dyn('dashboard.home.shortcuts', fallback: 'Shortcuts')),
        const SizedBox(height: 12),
        HomeShortcuts(items: shortcuts),
        const SizedBox(height: 32),
        // your accounts
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
      ],
    );
  }
}

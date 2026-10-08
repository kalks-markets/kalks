// Kalks Trader, full screen (its own route above the Client Area; back returns to it). The web's PHONE layout
// (apps/terminal/components/mobile/mobile-terminal.tsx) in the app's iOS look:
//   header      back · CFD | Options (the account switch by product) · account pill (login, Live / Demo / read-only,
//               equity, floating P&L) · bell
//   CFD         Watchlist · Chart · Trade · History · Account (bottom bar; the chart opens first)
//   Options     its own body and bottom bar (options/options_terminal.dart)
// An account is a CFD account or an Options account (its group's product): the workspace follows the account on
// screen, and CFD | Options switches to the client's last / default account of that product (or offers to open one).
// The Options side is hidden while the broker has the Options module off.
// Like the web's mobile terminal (dir="ltr") the terminal keeps its left-to-right layout in Arabic, Urdu and Persian:
// bid / ask, Sell / Buy and the chart keep their places, only the words are translated.
import 'dart:async';

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_error.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/models/trading.dart' show isOptionSymbol;
import '../../core/notifications/notifications.dart';
import '../../core/realtime/socket.dart';
import '../../core/theme_controller.dart';
import '../../i18n/i18n.dart';
import '../../shell/menus.dart';
import '../../ui/ui.dart';
import 'cfd/account_sheets.dart';
import 'cfd/account_tab.dart';
import 'cfd/alerts.dart';
import 'cfd/chart_tab.dart';
import 'cfd/history_tab.dart';
import 'cfd/order_sheet.dart';
import 'cfd/trade_tab.dart';
import 'cfd/watchlist_tab.dart';
import 'core/market.dart';
import 'core/sessions.dart';
import 'core/terminal_controller.dart';
import 'core/trade_math.dart';
import 'core/workspace.dart';
import 'options/options_terminal.dart';
import 'widgets/kit.dart';

/// What the account on screen trades: cfd | options (its session's account, else the client's own record; CFD while
/// none is open).
final activeProductProvider = Provider.autoDispose<String>((ref) {
  final active = ref.watch(tradeSessionsProvider.select((s) => s.active));
  final product = ref.watch(tradeSessionsProvider.select((s) => s.current?.account?.product));
  if (product != null) return product;
  if (active == null) return 'cfd';
  final own = ref.watch(ownAccountsProvider).value;
  return own?.where((a) => '${a.login}' == active).firstOrNull?.product ?? 'cfd';
});

/// The terminal's workspace (web useTradeMode): cfd | options. Not a choice of its own: an Options account opens the
/// options workspace, a CFD account the CFD one (always CFD while the broker has the Options module off).
final tradeModeProvider = Provider.autoDispose<String>((ref) {
  final optionsOn = ref.watch(configProvider.select((c) => c.moduleOn('options')));
  return optionsOn && ref.watch(activeProductProvider) == 'options' ? 'options' : 'cfd';
});

/// The header's CFD | Options: shows the client's last / default account of that product; none -> offers to open
/// one (the open-account wizard with the product chosen).
Future<void> switchToProduct(BuildContext context, WidgetRef ref, String product) async {
  final sessions = ref.read(tradeSessionsProvider.notifier);
  final login = await sessions.loginForProduct(product);
  if (!context.mounted) return;
  if (login == null) return showOpenProductSheet(context, product);
  if (login == ref.read(tradeSessionsProvider).active) return;
  KHaptics.selection();
  await switchTradeAccount(context, login);
}

/// "Open a CFD account" / "Open an Options account" when the client holds none of that product.
Future<void> showOpenProductSheet(BuildContext context, String product) async {
  final t = context.t;
  final router = GoRouter.of(context);
  final options = product == 'options';
  final go = await showKActionSheet<bool>(
    context,
    title: options ? t('options.account.noneTitle') : null,
    message: options ? t('options.account.noneText') : null,
    actions: [KAction(label: t(options ? 'trader.acct.openOptions' : 'trader.acct.openCfd'), value: true, icon: LucideIcons.plus, primary: true)],
  );
  if (go == true) router.go('/accounts/new?product=$product');
}

/// The CFD bottom-bar tab: watch | chart | trade | history | account.
class CfdTabController extends Notifier<String> {
  @override
  String build() => 'chart';
  void set(String t) => state = t;
}

final cfdTabProvider = NotifierProvider<CfdTabController, String>(CfdTabController.new);

class TerminalScreen extends ConsumerStatefulWidget {
  const TerminalScreen({super.key, this.login, this.symbol, this.side, this.mode});

  /// The account the Trade button was pressed on (null: the last one, else the default account).
  final String? login;

  /// `/trader?symbol=EURUSD`: open that market's chart (Markets, the Client Area's Trade buttons on a market).
  final String? symbol;

  /// `&side=buy|sell`: then the order sheet with that side, once the account is open.
  final String? side;

  /// `?mode=options`: open in Options mode (the Options intro page).
  final String? mode;

  /// The route's query (`/trader?login=&symbol=&side=&mode=`).
  static TerminalScreen fromQuery(Map<String, String> q, {Key? key}) =>
      TerminalScreen(key: key, login: q['login'], symbol: q['symbol']?.toUpperCase(), side: q['side'], mode: q['mode']);

  @override
  ConsumerState<TerminalScreen> createState() => _TerminalScreenState();
}

class _TerminalScreenState extends ConsumerState<TerminalScreen> with WidgetsBindingObserver {
  KBannerController? _banners;
  final List<ProviderSubscription<Object?>> _pendingOrder = [];

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    Future.microtask(() async {
      if (!mounted) return;
      final readOnly = ref.read(authProvider.select((s) => s is AuthSignedIn && s.me.viewer != null));
      _applyRoute();
      if (readOnly) return;
      await ref.read(tradeSessionsProvider.notifier).start(preferred: widget.login, mode: _startMode());
      if (mounted) await _cfdForSymbol();
    });
  }

  /// A CFD market's chart (`?symbol=`, not an option series).
  bool get _cfdLink {
    final s = widget.symbol;
    return s != null && s.isNotEmpty && !isOptionSymbol(s);
  }

  /// The product to open (`?mode=cfd|options`; options only while the module is on). Without a mode, a CFD market's
  /// link without a login opens a CFD account.
  String? _startMode() {
    final m = widget.mode;
    if (m == 'cfd' || (m == 'options' && ref.read(configProvider).moduleOn('options'))) return m;
    return widget.login == null && _cfdLink ? 'cfd' : null;
  }

  /// symbol / side of the route (the workspace itself follows the account: [tradeModeProvider]).
  void _applyRoute() {
    final symbol = widget.symbol;
    if (symbol != null && symbol.isNotEmpty) {
      ref.read(workspaceProvider.notifier).update((w) => w.copyWith(symbol: symbol));
      ref.read(cfdTabProvider.notifier).set('chart');
      final side = widget.side;
      if (side == 'buy' || side == 'sell') _orderWhenReady(symbol, side!);
    }
  }

  /// A CFD market's link while an Options account is on screen: the client's CFD account instead (if any).
  Future<void> _cfdForSymbol() async {
    if (!_cfdLink) return;
    final sessions = ref.read(tradeSessionsProvider.notifier);
    final active = ref.read(tradeSessionsProvider).active;
    if (active == null || await sessions.productOfLogin(active) != 'options') return;
    final login = await sessions.loginForProduct('cfd');
    if (login != null && mounted) await sessions.activate(login);
  }

  /// A new route on the open terminal: its account (or product), then its market.
  Future<void> _follow(TerminalScreen old) async {
    final sessions = ref.read(tradeSessionsProvider.notifier);
    if (widget.login != null && widget.login != old.login) {
      await sessions.activate(widget.login!);
    } else if (widget.mode != old.mode && _startMode() != null) {
      await sessions.start(mode: _startMode());
    }
    if (!mounted) return;
    if (widget.symbol != old.symbol) await _cfdForSymbol();
  }

  /// Opens the order sheet once the account is open and the market is known (and tradable on it).
  void _orderWhenReady(String symbol, String side) {
    _stopWaiting();
    void check() {
      final ready = ref.read(terminalProvider).synced && ref.read(symbolBookProvider)[symbol] != null;
      if (!ready || !mounted) return;
      _stopWaiting();
      final st = ref.read(terminalProvider);
      if (st.readOnly || !ref.read(symbolBookProvider).isVisible(symbol, live: st.account?.live ?? true)) return;
      unawaited(showOrderSheet(context, symbol: symbol, side: side));
    }

    _pendingOrder
      ..add(ref.listenManual<Object?>(terminalProvider.select((s) => s.synced), (_, _) => check()))
      ..add(ref.listenManual<Object?>(symbolBookProvider.select((b) => b[symbol] != null), (_, _) => check()));
    check();
  }

  void _stopWaiting() {
    for (final s in _pendingOrder) {
      s.close();
    }
    _pendingOrder.clear();
  }

  @override
  void didUpdateWidget(TerminalScreen old) {
    super.didUpdateWidget(old);
    final readOnly = ref.read(authProvider.select((s) => s is AuthSignedIn && s.me.viewer != null));
    if (!readOnly) unawaited(_follow(old));
    if (widget.symbol != old.symbol || widget.side != old.side) _applyRoute();
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed) {
      ref.read(marketFeedProvider).resume();
      ref.read(terminalProvider.notifier).resume();
    }
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    _stopWaiting();
    _banners?.theme = null;
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final mode = ref.watch(traderThemeModeProvider);
    final brand = ref.watch(configProvider.select((c) => c.brand));
    final platform = MediaQuery.platformBrightnessOf(context);
    final b = mode == ThemeMode.system ? platform : (mode == ThemeMode.dark ? Brightness.dark : Brightness.light);
    final theme = KTheme.trader(b, brand: brand);
    // engine banners take the terminal's look while it is open
    final banners = ref.read(bannerProvider);
    banners.theme = theme;
    _banners = banners;
    return Theme(
      data: theme,
      child: AnnotatedRegion(value: KTheme.overlay(b), child: const _Terminal()),
    );
  }
}

class _Terminal extends ConsumerWidget {
  const _Terminal();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final mode = ref.watch(tradeModeProvider);
    // keeps the market feed, the symbols and the account stream alive while the terminal is open
    ref.watch(marketFeedProvider);
    ref.watch(symbolsProvider);
    ref.watch(terminalProvider.select((s) => s.login));
    return Scaffold(
      backgroundColor: k.bg,
      resizeToAvoidBottomInset: false,
      body: Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            const _Header(),
            Expanded(child: mode == 'options' ? const _OptionsBody() : const _CfdBody()),
          ],
        ),
      ),
    );
  }
}

/* ---------------- header ---------------- */

class _Header extends ConsumerWidget {
  const _Header();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final unread = ref.watch(notificationsProvider.select((s) => s.unread));
    final mode = ref.watch(tradeModeProvider);
    final optionsOn = ref.watch(configProvider.select((c) => c.moduleOn('options')));
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return KFrosted(
      color: k.bar,
      border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
      child: SafeArea(
        bottom: false,
        child: SizedBox(
          height: 52,
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: 4),
            child: Row(
              children: [
                KIconButton(
                  icon: rtl ? LucideIcons.chevronRight : LucideIcons.chevronLeft,
                  size: 36,
                  semanticLabel: t('common.back'),
                  onPressed: () => context.canPop() ? context.pop() : context.go('/'),
                ),
                // the account switch by product: CFD | Options shows the client's account of that product
                if (optionsOn) ...[
                  SizedBox(
                    width: 118,
                    child: KSegmented<String>(
                      key: const ValueKey('trader-product'),
                      height: 30,
                      values: const ['cfd', 'options'],
                      labels: [t('trader.opt.mode.cfd'), t('trader.opt.mode.options')],
                      selected: mode,
                      onChanged: (v) => unawaited(switchToProduct(context, ref, v)),
                    ),
                  ),
                  const SizedBox(width: 6),
                ],
                const Expanded(child: _AccountPill()),
                KIconButton(
                  icon: LucideIcons.bell,
                  size: 36,
                  badge: unread,
                  semanticLabel: t('dashboard.notifications.title'),
                  onPressed: () => showBellSheet(context),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

/// The account on screen (web header block): login + Live / Demo (+ read-only), equity and floating P&L. Tap: the
/// account switcher.
class _AccountPill extends ConsumerWidget {
  const _AccountPill();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final st = ref.watch(terminalProvider);
    final sessions = ref.watch(tradeSessionsProvider);
    final a = st.account ?? sessions.current?.account;
    final m = st.metrics;
    final login = a?.login ?? sessions.active ?? sessions.busyLogin;
    final streamDown = st.synced && (st.stream == SocketStatus.reconnecting || st.stream == SocketStatus.connecting);
    return KPressable(
      minSize: 40,
      semanticLabel: t('trader.account.switch'),
      onTap: () => showAccountSwitcher(context),
      child: Container(
        height: 40,
        padding: const EdgeInsets.symmetric(horizontal: 9),
        decoration: BoxDecoration(
          color: k.surface2,
          borderRadius: BorderRadius.circular(11),
          border: Border.all(color: k.line),
        ),
        child: Row(
          children: [
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          login ?? t('shell.kalksTrader'),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: login == null ? context.text.label.copyWith(fontWeight: FontWeight.w600) : context.text.mono(12.5, weight: FontWeight.w600),
                        ),
                      ),
                      if (a != null) ...[
                        const SizedBox(width: 4),
                        TBadge(t.dyn('trader.accountType.${a.type}', fallback: a.type), tone: a.live ? TBadgeTone.ember : TBadgeTone.gold),
                      ],
                      if (st.readOnly) ...[const SizedBox(width: 3), TBadge(t('trader.badge.readOnly'), tone: TBadgeTone.warn)],
                      if (streamDown) ...[
                        const SizedBox(width: 4),
                        Container(
                          width: 6,
                          height: 6,
                          decoration: BoxDecoration(color: k.warn, shape: BoxShape.circle),
                        ),
                      ],
                    ],
                  ),
                  if (a != null)
                    Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(
                            text: accMoney(a.cent, m.equity),
                            style: context.text.mono(11, weight: FontWeight.w600),
                          ),
                          const TextSpan(text: ' '),
                          TextSpan(
                            text: accMoney(a.cent, m.floating, signed: true),
                            style: context.text.mono(10.5, color: m.floating > 0.004 ? k.up : (m.floating < -0.004 ? k.down : k.fg3)),
                          ),
                          TextSpan(
                            text: ' ${accCcy(a.cent)}',
                            style: context.text.mono(10, color: k.fg3),
                          ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                    ),
                ],
              ),
            ),
            Icon(LucideIcons.chevronsUpDown, size: 14, color: k.fg3),
          ],
        ),
      ),
    );
  }
}

/* ---------------- bodies ---------------- */

/// While the account opens, when it can't, or for view-only logins.
class _Gate extends ConsumerWidget {
  const _Gate({required this.child});
  final Widget child;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final viewer = ref.watch(authProvider.select((s) => s is AuthSignedIn && s.me.viewer != null));
    final s = ref.watch(tradeSessionsProvider);
    if (viewer) {
      return Center(
        child: KEmptyState(icon: LucideIcons.lock, title: t('order.ticket.readOnlyTitle'), text: t('order.toast.readOnlyDesc')),
      );
    }
    if (s.current != null) return child;
    if (s.phase == TradeSessionsPhase.failed) {
      final e = s.error;
      final text = e == null || e.code == 'no_account' || e.code == 'logged_out' ? t('trader.status.noAccount') : localizeTradeLogin(e, t);
      return Center(
        child: KEmptyState(
          icon: LucideIcons.candlestickChart,
          title: t('trader.toast.serverUnavailable'),
          text: text,
          action: Wrap(
            spacing: 8,
            runSpacing: 8,
            alignment: WrapAlignment.center,
            children: [
              KButton(
                label: t('common.retry'),
                size: KButtonSize.sm,
                variant: KButtonVariant.surface,
                onPressed: () => ref.read(tradeSessionsProvider.notifier).retry(),
              ),
              KButton(label: t('trader.guest.logInToTrade'), size: KButtonSize.sm, onPressed: () => showAddAccountSheet(context)),
            ],
          ),
        ),
      );
    }
    // Opening the account: the web's Splash (brand mark, "Kalks Trader", the connecting line), with the real logo
    final k = context.k;
    final cfg = ref.watch(configProvider);
    return Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          KBrandAvatar(size: 64, letter: cfg.tenantDefault ? null : cfg.tenantName.characters.first.toUpperCase()),
          const SizedBox(height: 18),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              if (cfg.tenantDefault) KLogo(height: 19, color: k.fg) else Text(cfg.tenantName, style: context.text.headline.copyWith(color: k.fg)),
              const SizedBox(width: 7),
              Text(
                'Trader',
                style: context.text.headline.copyWith(color: k.fg2, fontWeight: FontWeight.w400),
              ),
            ],
          ),
          const SizedBox(height: 16),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              CupertinoActivityIndicator(radius: 7, color: k.ember),
              const SizedBox(width: 8),
              Text(
                t('trader.splash.connecting', {'server': s.busyLogin != null ? '#${s.busyLogin}' : 'Kalks'}),
                style: context.text.footnote.copyWith(color: k.fg3, fontFamily: KFonts.mono),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _OptionsBody extends StatelessWidget {
  const _OptionsBody();

  @override
  Widget build(BuildContext context) => const _Gate(child: OptionsTerminal());
}

class _CfdBody extends ConsumerStatefulWidget {
  const _CfdBody();

  @override
  ConsumerState<_CfdBody> createState() => _CfdBodyState();
}

class _CfdBodyState extends ConsumerState<_CfdBody> {
  bool _chartBuilt = false;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final tab = ref.watch(cfdTabProvider);
    final positions = ref.watch(terminalProvider.select((s) => s.positions.length));
    if (tab == 'chart') _chartBuilt = true;
    final tabs = [
      ('watch', LucideIcons.list, t('trader.mobile.tab.watch')),
      ('chart', LucideIcons.candlestickChart, t('trader.mobile.tab.chart')),
      ('trade', LucideIcons.chartNoAxesColumn, t('trader.mobile.tab.trade')),
      ('history', LucideIcons.history, t('trader.mobile.tab.history')),
      ('account', LucideIcons.userRound, t('trader.mobile.tab.account')),
    ];
    final mq = MediaQuery.of(context);
    return Column(
      children: [
        Expanded(
          child: _Gate(
            child: Stack(
              children: [
                // the chart stays alive between tabs (its WebView and history); the other tabs mount when shown, so
                // only rows on screen ask the market feed for live prices
                if (_chartBuilt)
                  Offstage(
                    offstage: tab != 'chart',
                    child: TickerMode(enabled: tab == 'chart', child: const ChartTab()),
                  ),
                if (tab == 'watch') const WatchlistTab(),
                if (tab == 'trade') const TradeTab(),
                if (tab == 'history') const HistoryTab(),
                if (tab == 'account') const AccountTab(),
                const AlertsWatcher(),
                const MarketWants(),
              ],
            ),
          ),
        ),
        KFrosted(
          color: k.bar,
          border: Border(top: BorderSide(color: k.line, width: 0.6)),
          child: Padding(
            padding: EdgeInsets.only(bottom: mq.padding.bottom),
            child: SizedBox(
              height: 54,
              child: Row(
                children: [
                  for (final x in tabs)
                    Expanded(
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTap: () {
                          if (x.$1 == tab) return;
                          KHaptics.selection();
                          ref.read(cfdTabProvider.notifier).set(x.$1);
                        },
                        child: Semantics(
                          selected: x.$1 == tab,
                          button: true,
                          label: x.$3,
                          child: Stack(
                            alignment: Alignment.center,
                            children: [
                              if (x.$1 == tab)
                                Positioned(
                                  top: 0,
                                  left: 18,
                                  right: 18,
                                  child: Container(
                                    height: 2,
                                    decoration: BoxDecoration(color: k.ember, borderRadius: BorderRadius.circular(1)),
                                  ),
                                ),
                              Column(
                                mainAxisAlignment: MainAxisAlignment.center,
                                children: [
                                  Icon(x.$2, size: 19, color: x.$1 == tab ? k.ember : k.fg3),
                                  const SizedBox(height: 3),
                                  Text(
                                    x.$3,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: context.text.micro.copyWith(color: x.$1 == tab ? k.ember : k.fg3, fontWeight: FontWeight.w500),
                                  ),
                                ],
                              ),
                              if (x.$1 == 'trade' && positions > 0)
                                Positioned(
                                  top: 5,
                                  right: 14,
                                  child: Container(
                                    constraints: const BoxConstraints(minWidth: 15),
                                    height: 15,
                                    padding: const EdgeInsets.symmetric(horizontal: 4),
                                    alignment: Alignment.center,
                                    decoration: BoxDecoration(color: k.ember, borderRadius: BorderRadius.circular(8)),
                                    child: Text(
                                      '$positions',
                                      style: context.text.mono(9, weight: FontWeight.w600, color: Colors.white),
                                    ),
                                  ),
                                ),
                            ],
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ),
          ),
        ),
      ],
    );
  }
}

/// What the terminal wants streamed live besides the rows on screen (web priceFeed().want): the account's positions
/// and orders, the favourites, the chart, the alerts. Also points the feed at the account's spread group and gives it
/// the instrument list (the passive subscription).
class MarketWants extends ConsumerStatefulWidget {
  const MarketWants({super.key});

  @override
  ConsumerState<MarketWants> createState() => _MarketWantsState();
}

class _MarketWantsState extends ConsumerState<MarketWants> {
  @override
  Widget build(BuildContext context) {
    final feed = ref.watch(marketFeedProvider);
    final book = ref.watch(symbolBookProvider);
    final group = ref.watch(terminalProvider.select((s) => s.account?.spreadGroup));
    // a stable key (not a new set per equity frame), so this only re-runs when the symbols change
    final posKey = ref.watch(
      terminalProvider.select((s) => ({for (final p in s.positions) p.symbol, for (final o in s.orders) o.symbol}.toList()..sort()).join(',')),
    );
    final pos = posKey.isEmpty ? const <String>[] : posKey.split(',');
    final ws = ref.watch(workspaceProvider);
    if (book.all.isNotEmpty) {
      feed.symbols = book.all.keys.toList();
      if (group != null) feed.setGroup(group);
      feed.start();
    }
    feed.want('account', pos);
    feed.want('favourites', ws.favourites);
    feed.want('chart', [ws.symbol]);
    feed.want('alerts', {for (final a in ws.alerts.where((a) => a.active)) a.symbol});
    return const SizedBox.shrink();
  }
}

/// Login errors in the reader's language (web loginError).
String localizeTradeLogin(ApiException e, T t) => switch (e.code) {
  'invalid_credentials' => t('trader.login.error.invalid'),
  'locked' => t('trader.login.error.locked'),
  'rate_limited' => t('trader.login.error.rateLimited'),
  'unavailable' || 'network' => t('trader.login.error.unavailable'),
  'wrong_server' || 'validation' => e.message,
  _ =>
    e.status == 403
        ? (e.message.isNotEmpty ? e.message : t('trader.login.error.forbidden'))
        : (e.message.isNotEmpty ? e.message : t('trader.login.error.failed')),
};

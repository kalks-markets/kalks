// Kalks Trader, full screen (its own route above the Client Area; back returns to it). The web's PHONE layout
// (apps/terminal/components/mobile/mobile-terminal.tsx) in the app's iOS look:
//   header      back · account pill (login, Live / Demo / Options / read-only, equity, floating P&L; tap: the
//               account switcher) · bell
//   CFD         Watchlist · Chart · Trade · History · Account (bottom bar; the chart opens first)
//   Options     its own body and bottom bar (options/options_terminal.dart)
// An account is a CFD account or an Options account (its group's product) and the room follows the account on
// screen: there is no CFD | Options switch (owner request 2026-10). The account switcher (the header's pill, the
// Account tab) lists the CFD and the Options accounts, Live and Demo, and offers "Open an Options account" when the
// client has none; `?mode=options|cfd` still opens the client's account of that product. The Options side is hidden
// while the broker has the Options module off.
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
import 'cfd/symbol_screen.dart';
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
/// options workspace, a CFD account the CFD one (always CFD while the broker has the Options module off). The client
/// changes it by switching accounts.
final tradeModeProvider = Provider.autoDispose<String>((ref) {
  final optionsOn = ref.watch(configProvider.select((c) => c.moduleOn('options')));
  return optionsOn && ref.watch(activeProductProvider) == 'options' ? 'options' : 'cfd';
});

/// The CFD bottom-bar tab: watch | chart | trade | history | account, and `symbol` (the symbol screen opened from the
/// quotes: the Watchlist tab stays lit, back returns to it).
class CfdTabController extends Notifier<String> {
  @override
  String build() => 'chart';
  void set(String t) => state = t;
}

final cfdTabProvider = NotifierProvider<CfdTabController, String>(CfdTabController.new);

/// The market of the symbol screen (order book).
class SymbolScreenController extends Notifier<String?> {
  @override
  String? build() => null;
  void set(String? s) => state = s;
}

final symbolScreenProvider = NotifierProvider<SymbolScreenController, String?>(SymbolScreenController.new);

/// Opens the symbol screen of `symbol` (a quote row's tap).
void openSymbolScreen(WidgetRef ref, String symbol) {
  ref.read(symbolScreenProvider.notifier).set(symbol);
  ref.read(cfdTabProvider.notifier).set('symbol');
}

/// Tabs drawn without the terminal header: the symbol screen (a sub-screen of the quotes with its own back button).
/// The chart keeps the header (back, the account, the bell: it is the first screen) and fills everything
/// under it edge to edge, its controls floating over the plot.
bool fullScreenTab(String tab) => tab == 'symbol';

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
    final scheme = ref.watch(traderSchemeProvider);
    final brand = ref.watch(configProvider.select((c) => c.brand));
    final platform = MediaQuery.platformBrightnessOf(context);
    final b = mode == ThemeMode.system ? platform : (mode == ThemeMode.dark ? Brightness.dark : Brightness.light);
    final theme = KTheme.trader(b, brand: brand, green: scheme == TraderScheme.green);
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
    // the symbol screen fills the screen with its own back button; the other tabs keep the header
    final full = mode != 'options' && fullScreenTab(ref.watch(cfdTabProvider));
    return Scaffold(
      backgroundColor: k.bg,
      resizeToAvoidBottomInset: false,
      body: Directionality(
        textDirection: TextDirection.ltr,
        child: Column(
          children: [
            if (full) SizedBox(height: MediaQuery.paddingOf(context).top) else const _Header(),
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
                        // the room follows the account: an Options account says so on the pill
                        if (a.isOptions) ...[const SizedBox(width: 3), TBadge(t('accounts.product.chipOptions'), tone: TBadgeTone.info)],
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
    final symbol = ref.watch(symbolScreenProvider);
    if (tab == 'chart') _chartBuilt = true;
    // the symbol screen belongs to the quotes: their tab stays lit
    final lit = tab == 'symbol' ? 'watch' : tab;
    final tabs = [
      ('watch', LucideIcons.arrowDownUp, t('trader.mobile.tab.watch')),
      ('chart', LucideIcons.candlestickChart, t('trader.mobile.tab.chart')),
      ('trade', LucideIcons.trendingUp, t('trader.mobile.tab.trade')),
      ('history', LucideIcons.history, t('trader.mobile.tab.history')),
      ('account', LucideIcons.slidersHorizontal, t('trader.mobile.tab.account')),
    ];
    final mq = MediaQuery.of(context);
    return PopScope(
      // back on the symbol screen returns to the quotes
      canPop: tab != 'symbol',
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && tab == 'symbol') ref.read(cfdTabProvider.notifier).set('watch');
      },
      child: Column(
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
                  if (tab == 'symbol' && symbol != null) SymbolScreen(symbol: symbol),
                  if (tab == 'trade') const TradeTab(),
                  if (tab == 'history') const HistoryTab(),
                  if (tab == 'account') const AccountTab(),
                  const AlertsWatcher(),
                  const MarketWants(),
                ],
              ),
            ),
          ),
          Container(
            decoration: BoxDecoration(
              color: k.bg,
              border: Border(top: BorderSide(color: k.surface2)),
            ),
            padding: EdgeInsets.only(bottom: mq.padding.bottom),
            child: SizedBox(
              height: 60,
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
                          selected: x.$1 == lit,
                          button: true,
                          label: x.$3,
                          excludeSemantics: true,
                          child: Stack(
                            alignment: Alignment.center,
                            children: [
                              Column(
                                mainAxisAlignment: MainAxisAlignment.center,
                                children: [
                                  Icon(x.$2, size: 22, color: x.$1 == lit ? k.ember : k.fg3),
                                  const SizedBox(height: 4),
                                  Text(
                                    x.$3,
                                    maxLines: 1,
                                    overflow: TextOverflow.ellipsis,
                                    style: context.text.micro.copyWith(color: x.$1 == lit ? k.ember : k.fg3, fontSize: 11, fontWeight: FontWeight.w700),
                                  ),
                                ],
                              ),
                              if (x.$1 == 'trade' && positions > 0)
                                Positioned(
                                  top: 6,
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
        ],
      ),
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

// Routes. The Client Area pages use the web's own paths, so links in notifications, e-mails and deep links
// (kalks://app/<path>, https://app.kalkstrade.com/<path>) open the same page as on the web:
//   /login /register /forgot                     sign-in (signed out only)
//   /unlock                                       biometric unlock of a stored session
//   / … (StatefulShellRoute)                      the Client Area: five tabs (Dashboard · Accounts · Wallet · Portfolio
//                                                 · More), each with its own stack; module pages under More
//   /trader?login=                                Kalks Trader, full screen above the shell (any Trade button)
//   /maintenance /update                          system states
//   /unavailable?m=<module>                       a page of a module the broker switched off (web /unavailable)
// Sub-pages of a module are siblings that share one page, the module pager (a finger slides between them, the URL
// follows); detail pages are children (iOS push).
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:url_launcher/url_launcher.dart';

import '../core/api/api_providers.dart';
import '../core/app_info.dart';
import '../core/auth/auth_controller.dart';
import '../core/config/app_config.dart';
import '../core/notifications/notifications.dart';
import '../env.dart';
import '../features/academy/academy_routes.dart';
import '../features/accounts/account_detail_screen.dart';
import '../features/accounts/accounts_screen.dart';
import '../features/accounts/open_account_screen.dart';
import '../features/auth/forgot_screen.dart';
import '../features/auth/login_screen.dart';
import '../features/auth/register_screen.dart';
import '../features/auth/unlock_screen.dart';
import '../features/calendar/calendar_routes.dart';
import '../features/common/stub_screen.dart';
import '../features/common/system_screens.dart';
import '../features/dashboard/dashboard_screen.dart';
import '../features/developer/developer_routes.dart';
import '../features/markets/markets_routes.dart';
import '../features/news/news_routes.dart';
import '../features/options_intro/options_routes.dart';
import '../features/partner/partner_routes.dart';
import '../features/portfolio/analytics_screen.dart';
import '../features/portfolio/ledger_screen.dart';
import '../features/portfolio/portfolio_screen.dart';
import '../features/portfolio/statements_screen.dart';
import '../features/portfolio/trade_history_screen.dart';
import '../features/profile/notification_prefs_screen.dart';
import '../features/profile/preferences_screen.dart';
import '../features/profile/profile_screen.dart';
import '../features/profile/security_screen.dart';
import '../features/profile/verification_screen.dart';
import '../features/profile/viewers_screen.dart';
import '../features/prop/prop_routes.dart';
import '../features/rewards/rewards_routes.dart';
import '../features/social/social_routes.dart';
import '../features/support/support_screen.dart';
import '../features/terminal/terminal_screen.dart';
import '../features/wallet/deposit_screen.dart';
import '../features/wallet/transfer_screen.dart';
import '../features/wallet/wallet_history_screen.dart';
import '../features/wallet/wallet_screen.dart';
import '../features/wallet/withdraw_screen.dart';
import '../preview/gallery_screen.dart';
import '../shell/app_shell.dart';
import '../shell/module_pager.dart';
import '../shell/more_screen.dart';
import '../shell/nav.dart';
import '../ui/ui.dart';

final rootNavigatorKey = GlobalKey<NavigatorState>(debugLabel: 'root');

const Set<String> _authPaths = {'/login', '/register', '/forgot'};
const Set<String> _systemPaths = {'/boot', '/unlock', '/maintenance', '/update'};

/// The "not available" page: a page of a module the broker switched off lands here (`?m=` names the module).
const String kUnavailablePath = '/unavailable';

/// The tab (shell branch) each module's pages live in.
int branchOf(String moduleKey) => switch (moduleKey) {
  'dashboard' => 0,
  'accounts' => 1,
  'wallet' => 2,
  'portfolio' => 3,
  _ => 4,
};

Page<void> _tab(Widget child, GoRouterState s) => NoTransitionPage<void>(key: s.pageKey, child: child);

/// Every sub-page of the module as a sibling route (a stub until its screen is built). A module with several pages
/// gets one page for all of them, the module pager, under one key per module: going from one sub-route to another
/// updates that page in place, so the Navigator keeps the pager's State (its slide position and the pages visited).
/// Sub-routes must therefore never be pushed over each other (always `go`); detail pages in `extra` are pushed.
List<RouteBase> _moduleRoutes(String key, {ModuleScreens screens = const {}, List<RouteBase> extra = const []}) {
  final m = kNav.firstWhere((x) => x.key == key);
  final paths = {if (m.sub.isEmpty) m.href, for (final s in m.sub) s.href};
  final pagerKey = ValueKey('pager:$key');
  return [
    for (final p in paths)
      GoRoute(
        path: p,
        pageBuilder: paths.length > 1
            ? (c, s) => NoTransitionPage<void>(
                key: pagerKey,
                child: ModulePager(key: pagerKey, moduleKey: key, state: s, screens: screens),
              )
            : (c, s) => _tab(moduleScreen(screens, p, s), s),
      ),
    ...extra,
  ];
}

/// Agent C2's pages (Markets, News, Calendar, Options, Copy & PAMM, Partner, Prop, Rewards, Academy, Developer), by
/// web path, and their detail pages (iOS push).
final ModuleScreens _c2Screens = {
  ...marketsScreens,
  ...newsScreens,
  ...calendarScreens,
  ...optionsScreens,
  ...socialScreens,
  ...partnerScreens,
  ...propScreens,
  ...rewardsScreens,
  ...academyScreens,
  ...developerScreens,
};
final List<RouteBase> _c2Routes = [...socialRoutes, ...partnerRoutes, ...propRoutes, ...rewardsRoutes, ...academyRoutes, ...developerRoutes, ...optionsRoutes];

/// Agent C1's pages (Accounts, Wallet, Portfolio, Profile & Security, Support), by web path.
final ModuleScreens _c1Screens = {
  '/accounts': (s) => AccountsScreen(query: s.uri.queryParameters),
  '/accounts/new': (s) => OpenAccountScreen(query: s.uri.queryParameters),
  '/wallet': (s) => WalletScreen(query: s.uri.queryParameters),
  '/wallet/deposit': (s) => DepositScreen(query: s.uri.queryParameters),
  '/wallet/withdraw': (s) => WithdrawScreen(query: s.uri.queryParameters),
  '/wallet/transfer': (s) => TransferScreen(query: s.uri.queryParameters),
  '/wallet/history': (s) => WalletHistoryScreen(query: s.uri.queryParameters),
  '/portfolio': (s) => PortfolioScreen(query: s.uri.queryParameters),
  '/portfolio/analytics': (s) => AnalyticsScreen(query: s.uri.queryParameters),
  '/portfolio/history': (s) => TradeHistoryScreen(query: s.uri.queryParameters),
  '/portfolio/ledger': (s) => LedgerScreen(query: s.uri.queryParameters),
  '/portfolio/statements': (s) => StatementsScreen(query: s.uri.queryParameters),
  '/profile': (s) => ProfileScreen(query: s.uri.queryParameters),
  '/profile/security': (s) => SecurityScreen(query: s.uri.queryParameters),
  '/profile/verification': (s) => VerificationScreen(query: s.uri.queryParameters),
  '/profile/viewers': (s) => ViewersScreen(query: s.uri.queryParameters),
  '/profile/notifications': (s) => NotificationPrefsScreen(query: s.uri.queryParameters),
  '/profile/preferences': (s) => PreferencesScreen(query: s.uri.queryParameters),
  '/support': (s) => SupportScreen(query: s.uri.queryParameters),
};

/// Path and query of a location (deep links arrive with a scheme and host).
String _loc(Uri uri) => '${uri.path.isEmpty ? '/' : uri.path}${uri.hasQuery ? '?${uri.query}' : ''}';

/// Decides where a location may go, from the sign-in state, maintenance, the minimum app version and the broker's
/// module switches (`modules`, AppConfig.modules: a page of a module set to false goes to /unavailable).
String? redirectFor({
  required AuthState auth,
  required bool maintenance,
  required bool updateRequired,
  required Uri uri,
  Map<String, bool> modules = const {},
}) {
  // https://trade.kalkstrade.com/… (Kalks Trader links) -> the terminal
  if (uri.host.startsWith('trade.')) return '/trader';
  final path = uri.path.isEmpty ? '/' : uri.path;
  if (updateRequired) return path == '/update' ? null : '/update';
  if (maintenance) return path == '/maintenance' ? null : '/maintenance';
  if (path == '/maintenance' || path == '/update') return '/';
  switch (auth) {
    case AuthBooting():
      return path == '/boot' ? null : '/boot';
    case AuthLocked():
      // back to the same page after the unlock
      if (path == '/unlock') return null;
      return _systemPaths.contains(path) || _authPaths.contains(path) || path == '/' || path == kUnavailablePath
          ? '/unlock'
          : '/unlock?next=${Uri.encodeComponent(_loc(uri))}';
    case AuthSignedOut():
      if (_authPaths.contains(path)) return null;
      final next = _systemPaths.contains(path) || path == '/' || path == kUnavailablePath ? null : _loc(uri);
      return next == null ? '/login' : '/login?next=${Uri.encodeComponent(next)}';
    case AuthSignedIn(:final me):
      if (_authPaths.contains(path) || _systemPaths.contains(path)) {
        final next = uri.queryParameters['next'];
        return next != null && next.startsWith('/') && !next.startsWith('//') ? next : '/';
      }
      // the "not available" page itself never moves (no loop), whatever the switches and the login's sections
      if (path == kUnavailablePath) return null;
      final off = offModuleOf(modules, path);
      if (off != null) return '$kUnavailablePath?m=$off';
      final v = me.viewer;
      if (v != null && path != '/more' && !viewerPageAllowed(v, path)) {
        return const ['dashboard', 'accounts', 'history', 'wallet', 'partner']
                .where(v.sections.contains)
                .map((s) => const {'dashboard': '/', 'accounts': '/accounts', 'history': '/portfolio/history', 'wallet': '/wallet', 'partner': '/partner'}[s]!)
                .where((p) => offModuleOf(modules, p) == null)
                .firstOrNull ??
            '/more';
      }
      return null;
  }
}

class _RouterRefresh extends ChangeNotifier {
  void ping() => notifyListeners();
}

final routerProvider = Provider<GoRouter>((ref) {
  final refresh = _RouterRefresh();
  ref.listen(authProvider, (_, _) => refresh.ping());
  ref.listen(maintenanceProvider, (_, _) => refresh.ping());
  ref.listen(configProvider, (a, b) {
    // a module switched off (or on) while the app is open: the page in front leaves it, the navigation follows
    if (a?.maintenance != b.maintenance || a?.minAppVersion != b.minAppVersion || !mapEquals(a?.modules, b.modules)) refresh.ping();
  });
  ref.onDispose(refresh.dispose);

  final router = GoRouter(
    navigatorKey: rootNavigatorKey,
    initialLocation: '/boot',
    refreshListenable: refresh,
    redirect: (context, state) {
      final cfg = ref.read(configProvider);
      final to = redirectFor(
        auth: ref.read(authProvider),
        maintenance: cfg.maintenance || ref.read(maintenanceProvider),
        updateRequired: isOlderVersion(ref.read(appInfoProvider).version, cfg.minAppVersion),
        uri: state.uri,
        modules: cfg.modules,
      );
      return to;
    },
    errorPageBuilder: (c, s) => _tab(StubScreen(path: s.uri.path), s),
    routes: [
      GoRoute(path: '/boot', pageBuilder: (c, s) => _tab(const _BootScreen(), s)),
      GoRoute(path: '/login', pageBuilder: (c, s) => _tab(const LoginScreen(), s)),
      GoRoute(
        path: '/register',
        builder: (c, s) => RegisterScreen(referral: s.uri.queryParameters['ref']),
      ),
      GoRoute(path: '/forgot', builder: (c, s) => const ForgotScreen()),
      GoRoute(path: '/unlock', pageBuilder: (c, s) => _tab(const UnlockScreen(), s)),
      GoRoute(path: '/maintenance', pageBuilder: (c, s) => _tab(const MaintenanceScreen(), s)),
      GoRoute(path: '/update', pageBuilder: (c, s) => _tab(const UpdateScreen(), s)),
      GoRoute(
        path: kUnavailablePath,
        pageBuilder: (c, s) => _tab(UnavailableScreen(module: s.uri.queryParameters['m']), s),
      ),
      GoRoute(
        path: '/trader',
        parentNavigatorKey: rootNavigatorKey,
        pageBuilder: (c, s) => CupertinoFullscreenPage(
          key: s.pageKey,
          child: TerminalScreen.fromQuery(s.uri.queryParameters),
        ),
      ),
      StatefulShellRoute.indexedStack(
        builder: (context, state, shell) => AppShell(shell: shell, path: state.uri.path),
        branches: [
          StatefulShellBranch(
            routes: _moduleRoutes(
              'dashboard',
              screens: {'/': (_) => const DashboardScreen(), ..._c2Screens},
              extra: [...marketsRoutes, ...newsRoutes, ...calendarRoutes],
            ),
          ),
          StatefulShellBranch(
            routes: _moduleRoutes(
              'accounts',
              screens: _c1Screens,
              extra: [
                GoRoute(
                  path: '/accounts/:login',
                  builder: (c, s) => AccountDetailScreen(login: s.pathParameters['login']!, query: s.uri.queryParameters),
                ),
              ],
            ),
          ),
          StatefulShellBranch(routes: _moduleRoutes('wallet', screens: _c1Screens)),
          StatefulShellBranch(routes: _moduleRoutes('portfolio', screens: _c1Screens)),
          StatefulShellBranch(
            routes: [
              GoRoute(path: '/more', pageBuilder: (c, s) => _tab(const MoreScreen(), s)),
              // the design system on one page, in development previews only
              if (Env.preview)
                GoRoute(
                  path: '/more/gallery',
                  builder: (c, s) => GalleryScreen(open: s.uri.queryParameters['open']),
                ),
              for (final m in kNav)
                if (branchOf(m.key) == 4) ..._moduleRoutes(m.key, screens: {..._c1Screens, ..._c2Screens}),
              ..._c2Routes,
              // gated in live builds (apps/crm/lib/live.ts), kept reachable for links
              GoRoute(
                path: '/academy/coach',
                pageBuilder: (c, s) => _tab(const StubScreen(path: '/academy/coach'), s),
              ),
            ],
          ),
        ],
      ),
    ],
  );

  // keep the current path for the notification store, and open links from banners through the router
  // (after the frame: the router notifies while widgets build, and providers may not change then)
  void sync() => WidgetsBinding.instance.addPostFrameCallback((_) {
    if (router.routerDelegate.currentConfiguration.isNotEmpty) ref.read(currentPathProvider.notifier).set(router.routerDelegate.currentConfiguration.uri.path);
  });
  router.routerDelegate.addListener(sync);
  ref.read(linkOpenerProvider).handler = (link) {
    if (link.startsWith('/')) {
      router.go(link);
    } else {
      final uri = Uri.tryParse(link);
      if (uri != null) launchUrl(uri, mode: LaunchMode.externalApplication);
    }
  };
  ref.onDispose(() {
    router.routerDelegate.removeListener(sync);
    router.dispose();
  });
  return router;
});

/// The full-screen terminal slides up like an iOS full-screen modal.
class CupertinoFullscreenPage extends CustomTransitionPage<void> {
  CupertinoFullscreenPage({super.key, required super.child})
    : super(
        transitionDuration: const Duration(milliseconds: 380),
        reverseTransitionDuration: const Duration(milliseconds: 300),
        transitionsBuilder: (context, animation, secondary, child) {
          final curved = CurvedAnimation(parent: animation, curve: Curves.easeOutCubic, reverseCurve: Curves.easeInCubic);
          return SlideTransition(
            position: Tween(begin: const Offset(0, 1), end: Offset.zero).animate(curved),
            child: child,
          );
        },
      );
}

/// While the stored session is read (the native splash usually still covers it).
class _BootScreen extends StatelessWidget {
  const _BootScreen();

  @override
  Widget build(BuildContext context) => Scaffold(
    backgroundColor: context.k.bg,
    body: const Center(child: KBrandAvatar(size: 64)),
  );
}

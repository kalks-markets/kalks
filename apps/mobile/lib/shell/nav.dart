// The Client Area navigation, 1:1 with the web (apps/crm/lib/nav.ts): the same modules, sub-pages, order, icons and
// label keys, the same paths (so web links in notifications and deep links open the same page), the broker's
// module switches (navForFeatures) and the view-only rules (navForViewer, lib/viewer.ts).
import 'package:flutter/widgets.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/config/app_config.dart';
import '../core/models/user.dart';
import '../i18n/t.dart';

class NavSub {
  const NavSub(this.href, this.labelKey, this.icon);
  final String href;
  final String labelKey;
  final IconData icon;
}

class NavModule {
  const NavModule({required this.key, required this.labelKey, required this.icon, required this.href, required this.section, this.match, this.sub = const []});
  final String key;
  final String labelKey;
  final IconData icon;
  final String href;

  /// main | grow | build | learn | account
  final String section;

  /// Path prefixes that belong to the module (default: href).
  final List<String>? match;
  final List<NavSub> sub;

  NavModule copyWith({String? href, List<NavSub>? sub}) =>
      NavModule(key: key, labelKey: labelKey, icon: icon, href: href ?? this.href, section: section, match: match, sub: sub ?? this.sub);
}

/// CRM_NAV / LIVE_NAV (live builds: no demo badges, no gated sub-pages such as /academy/coach).
const List<NavModule> kNav = [
  NavModule(
    key: 'dashboard',
    labelKey: 'shell.nav.dashboard',
    icon: LucideIcons.layoutGrid,
    href: '/',
    section: 'main',
    match: ['/', '/markets', '/news', '/calendar'],
    sub: [
      NavSub('/', 'shell.nav.overview', LucideIcons.layoutGrid),
      NavSub('/markets', 'shell.nav.markets', LucideIcons.globe2),
      NavSub('/news', 'shell.nav.news', LucideIcons.newspaper),
      NavSub('/calendar', 'shell.nav.calendar', LucideIcons.calendarDays),
    ],
  ),
  NavModule(
    key: 'accounts',
    labelKey: 'shell.nav.accounts',
    icon: LucideIcons.layers,
    href: '/accounts',
    section: 'main',
    sub: [NavSub('/accounts', 'shell.nav.myAccounts', LucideIcons.layers), NavSub('/accounts/new', 'shell.nav.openAccount', LucideIcons.circlePlus)],
  ),
  NavModule(
    key: 'wallet',
    labelKey: 'shell.nav.wallet',
    icon: LucideIcons.wallet,
    href: '/wallet',
    section: 'main',
    sub: [
      NavSub('/wallet', 'shell.nav.overview', LucideIcons.wallet),
      NavSub('/wallet/deposit', 'shell.nav.deposit', LucideIcons.arrowDownToLine),
      NavSub('/wallet/withdraw', 'shell.nav.withdraw', LucideIcons.arrowUpFromLine),
      NavSub('/wallet/transfer', 'shell.nav.transfer', LucideIcons.arrowLeftRight),
      NavSub('/wallet/history', 'shell.nav.history', LucideIcons.history),
    ],
  ),
  NavModule(
    key: 'portfolio',
    labelKey: 'shell.nav.portfolio',
    icon: LucideIcons.chartPie,
    href: '/portfolio',
    section: 'main',
    sub: [
      NavSub('/portfolio', 'shell.nav.overview', LucideIcons.chartPie),
      NavSub('/portfolio/analytics', 'shell.nav.analytics', LucideIcons.chartColumn),
      NavSub('/portfolio/history', 'shell.nav.tradeHistory', LucideIcons.history),
      NavSub('/portfolio/ledger', 'shell.nav.ledger', LucideIcons.bookText),
      NavSub('/portfolio/statements', 'shell.nav.statements', LucideIcons.fileText),
    ],
  ),
  NavModule(key: 'options', labelKey: 'options.nav.title', icon: LucideIcons.chartSpline, href: '/options', section: 'main'),
  NavModule(
    key: 'partner',
    labelKey: 'shell.nav.partner',
    icon: LucideIcons.handshake,
    href: '/partner',
    section: 'grow',
    sub: [
      NavSub('/partner', 'shell.nav.dashboard', LucideIcons.layoutGrid),
      NavSub('/partner/clients', 'shell.nav.clients', LucideIcons.users),
      NavSub('/partner/network', 'shell.nav.network', LucideIcons.network),
      NavSub('/partner/commissions', 'shell.nav.commissions', LucideIcons.coins),
      NavSub('/partner/links', 'shell.nav.links', LucideIcons.link2),
      NavSub('/partner/payouts', 'shell.nav.payouts', LucideIcons.banknote),
    ],
  ),
  NavModule(
    key: 'social',
    labelKey: 'shell.nav.social',
    icon: LucideIcons.copy,
    href: '/social',
    section: 'grow',
    sub: [
      NavSub('/social', 'shell.nav.discover', LucideIcons.compass),
      NavSub('/social/copy', 'shell.nav.copyTrading', LucideIcons.repeat),
      NavSub('/social/pamm', 'shell.nav.pamm', LucideIcons.landmark),
      NavSub('/social/investments', 'shell.nav.investments', LucideIcons.chartLine),
      NavSub('/social/managed', 'shell.nav.managed', LucideIcons.usersRound),
      NavSub('/social/master', 'shell.nav.becomeMaster', LucideIcons.crown),
      NavSub('/social/mam', 'shell.nav.mamManager', LucideIcons.briefcase),
    ],
  ),
  NavModule(
    key: 'prop',
    labelKey: 'shell.nav.prop',
    icon: LucideIcons.trophy,
    href: '/prop',
    section: 'grow',
    sub: [
      NavSub('/prop', 'shell.nav.challenges', LucideIcons.target),
      NavSub('/prop/mine', 'shell.nav.myChallenges', LucideIcons.trophy),
      NavSub('/prop/payouts', 'shell.nav.payouts', LucideIcons.banknote),
      NavSub('/prop/certificates', 'shell.nav.certificates', LucideIcons.award),
    ],
  ),
  NavModule(
    key: 'rewards',
    labelKey: 'shell.nav.rewards',
    icon: LucideIcons.gift,
    href: '/rewards',
    section: 'grow',
    sub: [
      NavSub('/rewards', 'shell.nav.contests', LucideIcons.medal),
      NavSub('/rewards/loyalty', 'shell.nav.loyalty', LucideIcons.gift),
      NavSub('/rewards/cashback', 'shell.nav.cashback', LucideIcons.percent),
      NavSub('/rewards/promotions', 'shell.nav.promotions', LucideIcons.ticket),
    ],
  ),
  NavModule(
    key: 'developer',
    labelKey: 'shell.nav.developer',
    icon: LucideIcons.code2,
    href: '/developer',
    section: 'build',
    sub: [
      NavSub('/developer', 'shell.nav.apiKeys', LucideIcons.keyRound),
      NavSub('/developer/webhooks', 'shell.nav.webhooks', LucideIcons.webhook),
      NavSub('/developer/strategies', 'shell.nav.strategyBuilder', LucideIcons.workflow),
      NavSub('/developer/deployments', 'shell.nav.runningStrategies', LucideIcons.bot),
      NavSub('/developer/backtests', 'shell.nav.backtests', LucideIcons.flaskConical),
      NavSub('/developer/marketplace', 'shell.nav.marketplace', LucideIcons.store),
      NavSub('/developer/docs', 'shell.nav.docs', LucideIcons.bookOpen),
    ],
  ),
  NavModule(
    key: 'academy',
    labelKey: 'shell.nav.academyLive',
    icon: LucideIcons.graduationCap,
    href: '/academy',
    section: 'learn',
    // /academy/coach is gated in live builds (apps/crm/lib/live.ts LIVE_GATED)
    sub: [
      NavSub('/academy', 'shell.nav.courses', LucideIcons.graduationCap),
      NavSub('/academy/glossary', 'shell.nav.glossary', LucideIcons.bookOpen),
      NavSub('/academy/progress', 'shell.nav.myProgress', LucideIcons.award),
    ],
  ),
  NavModule(
    key: 'profile',
    labelKey: 'shell.nav.profileSecurity',
    icon: LucideIcons.userRound,
    href: '/profile',
    section: 'account',
    sub: [
      NavSub('/profile', 'shell.nav.profile', LucideIcons.userRound),
      NavSub('/profile/security', 'shell.nav.security', LucideIcons.shieldCheck),
      NavSub('/profile/verification', 'shell.nav.verification', LucideIcons.badgeCheck),
      NavSub('/profile/viewers', 'shell.nav.viewers', LucideIcons.eye),
      NavSub('/profile/notifications', 'shell.nav.notifications', LucideIcons.bell),
      NavSub('/profile/preferences', 'shell.nav.preferences', LucideIcons.slidersHorizontal),
    ],
  ),
  NavModule(key: 'support', labelKey: 'shell.nav.support', icon: LucideIcons.lifeBuoy, href: '/support', section: 'account'),
];

/// The four modules of the bottom bar; everything else is under More.
const List<String> kPrimaryModules = ['dashboard', 'accounts', 'wallet', 'portfolio'];

/// Nav path -> module switch (apps/crm/components/tenant-config.tsx PAGE_MODULES); the longest prefix wins, so
/// /social/mam is MAM while the rest of /social is copy trading. Modules without a page of their own gate parts of
/// other pages instead: `ai` (Ask Kalks AI, the options AI explanation) and `support_chat` (the live chat on
/// /support, the floating chat button; the page itself stays for the email channel).
const List<(String, String)> _pageModules = [
  ('/social/pamm', 'pamm'),
  ('/social/investments', 'pamm'),
  ('/social/mam', 'mam'),
  ('/social/managed', 'mam'),
  ('/social', 'copy_trading'),
  ('/prop', 'prop'),
  ('/partner', 'ib'),
  ('/developer/strategies', 'algo'),
  ('/developer/deployments', 'algo'),
  ('/developer/backtests', 'algo'),
  ('/developer/marketplace', 'algo'),
  ('/developer', 'api'),
  ('/academy', 'academy'),
  ('/wallet', 'wallet'),
  ('/rewards', 'rewards'),
  ('/options', 'options'),
  ('/news', 'news'),
  ('/calendar', 'calendar'),
  ('/markets', 'markets'),
];

bool _under(String path, String prefix) => prefix == '/' ? path == '/' : path == prefix || path.startsWith('$prefix/');

String? pageModule(String href) {
  final hits = _pageModules.where((p) => _under(href, p.$1)).toList()..sort((a, b) => b.$1.length.compareTo(a.$1.length));
  return hits.isEmpty ? null : hits.first.$2;
}

/// The switched-off module that `path` belongs to (null when the page has no module or its module is on).
String? offModuleOf(Map<String, bool> modules, String path) {
  final m = pageModule(path);
  return m != null && modules[m] == false ? m : null;
}

/// Whether the page at `href` is offered (its module, if any, is on).
bool pageOn(AppConfig cfg, String href) => offModuleOf(cfg.modules, href) == null;

/// Without the modules this broker switched off.
List<NavModule> navForFeatures(List<NavModule> nav, AppConfig cfg) {
  bool on(String href) => pageOn(cfg, href);

  return [
    for (final m in nav)
      if (m.sub.isEmpty) ...[
        if (on(m.href)) m,
      ] else if (m.sub.where((s) => on(s.href)).isNotEmpty)
        m.copyWith(href: m.sub.firstWhere((s) => on(s.href)).href, sub: m.sub.where((s) => on(s.href)).toList()),
  ];
}

/// View-only logins (D90): the pages of their sections only (apps/crm/lib/viewer.ts VIEWER_SECTIONS).
const Map<String, List<String>> _viewerPages = {
  'dashboard': ['/', '/markets', '/news', '/calendar'],
  'accounts': ['/accounts'],
  'history': ['/portfolio'],
  'wallet': ['/wallet', '/wallet/history'],
  'partner': ['/partner'],
};
const List<String> _viewerBlocked = ['/accounts/new', '/wallet/deposit', '/wallet/withdraw', '/wallet/transfer', '/partner/payouts', '/partner/links'];

bool viewerPageAllowed(ViewerScope scope, String path) {
  if (_viewerBlocked.any((p) => _under(path, p))) return false;
  return scope.sections.any((s) => (_viewerPages[s] ?? const []).any((p) => _under(path, p)));
}

List<NavModule> navForViewer(List<NavModule> nav, ViewerScope scope) => [
  for (final m in nav)
    if (m.sub.isEmpty) ...[
      if (viewerPageAllowed(scope, m.href)) m,
    ] else if (m.sub.any((s) => viewerPageAllowed(scope, s.href)))
      m.copyWith(href: m.sub.firstWhere((s) => viewerPageAllowed(scope, s.href)).href, sub: m.sub.where((s) => viewerPageAllowed(scope, s.href)).toList()),
];

/// The navigation this client sees.
List<NavModule> navFor(AppConfig cfg, SessionUser? me) {
  final nav = navForFeatures(kNav, cfg);
  return me?.viewer == null ? nav : navForViewer(nav, me!.viewer!);
}

/// The module a path belongs to (its `match` prefixes, or its href).
bool isActive(String path, NavModule m) => (m.match ?? [m.href]).any((p) => _under(path, p));

NavModule? moduleOf(List<NavModule> nav, String path) {
  for (final m in nav) {
    if (isActive(path, m)) return m;
  }
  // a module whose root moved (a switched-off first page) still owns its other pages
  for (final m in nav) {
    if (m.sub.any((s) => _under(path, s.href))) return m;
  }
  return null;
}

/// Longest matching sub-page wins, so /wallet and /wallet/deposit never both light up.
NavSub? activeSub(String path, List<NavSub> subs) {
  final hits = subs.where((s) => _under(path, s.href)).toList()..sort((a, b) => b.href.length.compareTo(a.href.length));
  return hits.isEmpty ? null : hits.first;
}

/// The ⌘K palette's entries: every page, grouped by module, in the reader's language.
List<({String group, String label, String href, IconData icon})> navCommands(List<NavModule> nav, T t) => [
  for (final m in nav)
    if (m.sub.isEmpty)
      (group: t(m.labelKey), label: t(m.labelKey), href: m.href, icon: m.icon)
    else
      for (final s in m.sub) (group: t(m.labelKey), label: t(s.labelKey), href: s.href, icon: s.icon),
];

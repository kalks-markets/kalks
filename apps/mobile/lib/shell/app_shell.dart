// The Client Area shell, like the phone web (apps/crm/components/chrome/client-shell.tsx + mobile-nav.tsx):
// - a frosted header: brand disc (-> Dashboard), the module title, search, the bell, the Trade button (opens the
//   full-screen Kalks Trader) and the profile menu; under it the module's pages as text tabs (SubNav);
// - the page, scrolling under both bars (their heights reach the page as MediaQuery padding);
// - a floating ink bottom bar: Dashboard · Accounts · Wallet · Portfolio · More;
// - in the in-app demo, a slim "Demo · Sample data · Exit demo" strip above the header.
// Every section's first page of the stock Kalks brand opens on its photo (page_hero.dart): the page gets no top
// padding, the same controls float over the photo as round glass buttons, and the frosted header fades in once the
// page's sheet reaches the header zone — or as soon as the module pager (module_pager.dart) starts sliding the photo
// page out; the hero chrome stays while the photo is still partly on screen and comes back with it. Behind every page,
// the section's photo blurred under a black wash (KBackdrop.photo, web html[data-photo]).
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../core/api/api_providers.dart';
import '../core/auth/auth_controller.dart';
import '../core/auth/biometrics.dart';
import '../core/config/app_config.dart';
import '../core/models/user.dart';
import '../core/notifications/notifications.dart';
import '../core/prefs.dart';
import '../env.dart';
import '../features/support/launcher.dart';
import '../i18n/i18n.dart';
import '../ui/ui.dart';
import 'chrome.dart';
import 'menus.dart';
import 'nav.dart';
import 'page_hero.dart';
import 'session_keeper.dart';

/// Tab index of each bottom-bar entry (the shell route's branches, in this order).
const List<String> kTabRoots = ['/', '/accounts', '/wallet', '/portfolio', '/more'];

/// The controls floating over a section's photo, and the frosted header that replaces them once the page's sheet is
/// up (tests).
const Key kShellHeroControls = ValueKey('shell-hero-controls');
const Key kShellHeroHeader = ValueKey('shell-hero-header');

class AppShell extends ConsumerStatefulWidget {
  const AppShell({super.key, required this.shell, required this.path});
  final StatefulNavigationShell shell;

  /// The current location's path.
  final String path;

  @override
  ConsumerState<AppShell> createState() => _AppShellState();
}

class _AppShellState extends ConsumerState<AppShell> {
  bool _scrolled = false;

  /// Hero page: the page's sheet has reached the header zone (the frosted header is up, the floating controls gone).
  bool _collapsed = false;

  /// Set by build: whether the hero chrome is up (the page opens on its picture, or the pager is sliding it), and the
  /// picture page's scroll offset where it collapses.
  bool _hero = false;
  double _collapseAt = double.infinity;

  /// The module pager's slide (chrome.dart), and whether it is between the picture page and the next one.
  late final ValueNotifier<double> _slide = ref.read(modulePagerOffsetProvider);
  bool _sliding = false;

  @override
  void initState() {
    super.initState();
    _slide.addListener(_onSlide);
    WidgetsBinding.instance.addPostFrameCallback((_) => _offerBiometric());
  }

  @override
  void dispose() {
    _slide.removeListener(_onSlide);
    super.dispose();
  }

  void _onSlide() {
    final v = _slide.value;
    final s = v > 0 && v < 1;
    if (s != _sliding) setState(() => _sliding = s);
  }

  /// After the first sign-in on this phone: "Unlock faster next time?" (once; not for the demo's sample client).
  Future<void> _offerBiometric() async {
    final prefs = ref.read(prefsProvider);
    if (Env.preview || ref.read(demoModeProvider) || prefs.biometricAsked || prefs.biometricEnabled) return;
    if (!await ref.read(biometricsProvider).available() || !mounted) return;
    final t = context.t;
    final on = await showKAlert<bool>(
      context,
      title: t('app.biometric.offerTitle'),
      message: t('app.biometric.text'),
      actions: [
        KAction(label: t('app.biometric.notNow'), value: false),
        KAction(label: t('app.biometric.enable'), value: true, primary: true),
      ],
    );
    await ref.read(authProvider.notifier).setBiometric(on == true);
  }

  /// The page's own scroll (depth 0, vertical): scroll updates, and the metrics a fresh page reports on its first
  /// layout (a page replaced in its branch starts at the top again without scrolling). From the module pager the
  /// scroll names its page (`path`); the picture page's scroll alone decides the collapse.
  void _onScroll(ScrollMetrics m, {String? path}) {
    final s = m.pixels > 8;
    final c = pageHeroAt(path ?? widget.path, ref.read(configProvider)) ? m.pixels >= _collapseAt : _collapsed;
    if (s == _scrolled && c == _collapsed) return;
    // the module pager reports the page in front again while the route changes, which is during a build: after it
    if (SchedulerBinding.instance.schedulerPhase == SchedulerPhase.persistentCallbacks) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _onScroll(m, path: path);
      });
      return;
    }
    setState(() {
      _scrolled = s;
      _collapsed = c;
    });
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final mq = MediaQuery.of(context);
    final cfg = ref.watch(configProvider);
    final me = ref.watch(meProvider);
    final nav = navFor(cfg, me);
    final module = moduleOf(nav, widget.path);
    final subs = module != null && module.sub.length > 1 ? module.sub : null;
    final demo = ref.watch(demoModeProvider);
    final demoH = demo ? _DemoStrip.height : 0.0;
    final headerH = mq.padding.top + demoH + KSize.header + (subs != null ? KSize.subNav : 0);
    final barBottom = mq.padding.bottom < 12 ? 12.0 : mq.padding.bottom;
    final barH = KSize.tabBar + barBottom + 8;

    // a section's photo (stock Kalks brand): the sheet's edge starts under the photo and collapses the chrome once it
    // reaches the bottom of the header row; while the pager slides the photo page (the module's first) in or out, the
    // hero chrome stays (collapsed by the slide itself)
    final firstPage = module == null ? null : (module.sub.isEmpty ? module.href : module.sub.first.href);
    _hero = pageHeroAt(widget.path, cfg) || (_sliding && firstPage != null && pageHeroAt(firstPage, cfg));
    _collapseAt = pageHeroHeight(mq) - kPageHeroOverlap - (mq.padding.top + demoH + KSize.header);
    final collapsed = _hero && _collapsed;
    final b = Theme.of(context).brightness;
    // light status-bar icons over the picture
    final overlay = _hero && !collapsed
        ? KTheme.overlay(b).copyWith(statusBarIconBrightness: Brightness.light, statusBarBrightness: Brightness.dark)
        : KTheme.overlay(b);

    return SessionKeeper(
      child: Scaffold(
        backgroundColor: k.bg,
        resizeToAvoidBottomInset: false,
        body: AnnotatedRegion<SystemUiOverlayStyle>(
          value: overlay,
          child: Stack(
            children: [
              Positioned.fill(child: KBackdrop(photo: backdropPhotoAt(widget.path, cfg))),
              Positioned.fill(
                child: ShellInsets(
                  headerHeight: headerH,
                  child: MediaQuery(
                    data: mq.copyWith(
                      padding: mq.padding.copyWith(top: _hero ? 0 : headerH, bottom: barH),
                    ),
                    child: NotificationListener<ScrollUpdateNotification>(
                      onNotification: (n) {
                        if (n.depth == 0 && n.metrics.axis == Axis.vertical) _onScroll(n.metrics);
                        return false;
                      },
                      child: NotificationListener<ScrollMetricsNotification>(
                        onNotification: (n) {
                          if (n.depth == 0 && n.metrics.axis == Axis.vertical) _onScroll(n.metrics);
                          return false;
                        },
                        // the page in front of a module pager (its own scroll reaches here at depth 1)
                        child: NotificationListener<ModulePageScrollNotification>(
                          onNotification: (n) {
                            _onScroll(n.metrics, path: n.path);
                            return true;
                          },
                          child: widget.shell,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              Positioned(
                top: 0,
                left: 0,
                right: 0,
                child: _hero
                    ? _HeroChrome(
                        collapsed: collapsed,
                        slide: _slide,
                        controls: _HeroControls(key: kShellHeroControls, nav: nav, demo: demo),
                        header: (fade) =>
                            _Header(key: kShellHeroHeader, module: module, subs: subs, path: widget.path, scrolled: true, nav: nav, demo: demo, fade: fade),
                      )
                    : _Header(module: module, subs: subs, path: widget.path, scrolled: _scrolled, nav: nav, demo: demo),
              ),
              // the floating support chat (web SupportLauncher: every page but /support, never for view-only logins); it
              // places itself above the tab bar at the bottom end and takes touches only on its button. Filled, so it
              // never sizes the stack (it is an empty box on /support).
              if (me != null && me.viewer == null) Positioned.fill(child: SupportLauncher(path: widget.path)),
              Positioned(
                left: 12,
                right: 12,
                bottom: barBottom,
                child: _TabBar(shell: widget.shell, path: widget.path, nav: nav),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Header extends ConsumerWidget {
  const _Header({
    super.key,
    required this.module,
    required this.subs,
    required this.path,
    required this.scrolled,
    required this.nav,
    required this.demo,
    this.fade,
  });
  final NavModule? module;
  final List<NavSub>? subs;
  final String path;
  final bool scrolled;
  final List<NavModule> nav;
  final bool demo;

  /// Over the Dashboard's picture: 0 hidden … 1 the frosted header (the frost and the content come in together; the
  /// blur is never inside an Opacity, which would blur nothing).
  final double? fade;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final me = ref.watch(meProvider);
    final cfg = ref.watch(configProvider);
    final unread = ref.watch(notificationsProvider.select((s) => s.unread));
    final viewer = me?.viewer != null;
    final content = SafeArea(
      bottom: false,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (demo) const _DemoStrip(),
          SizedBox(
            height: KSize.header,
            child: Padding(
              padding: const EdgeInsetsDirectional.only(start: 14, end: 8),
              child: Row(
                children: [
                  KPressable(
                    onTap: () => context.go('/'),
                    semanticLabel: t('shell.nav.dashboard'),
                    child: KBrandAvatar(size: 36, letter: cfg.tenantDefault ? null : cfg.tenantName.characters.first.toUpperCase()),
                  ),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Text(
                      module == null ? (path.startsWith('/more') ? t('shell.more') : '') : t(module!.labelKey),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.title2.copyWith(fontWeight: FontWeight.w700),
                    ),
                  ),
                  KIconButton(icon: LucideIcons.search, size: 38, semanticLabel: t('shell.search'), onPressed: () => showCommandPalette(context, nav)),
                  if (!viewer)
                    KIconButton(
                      icon: LucideIcons.bell,
                      size: 38,
                      badge: unread,
                      semanticLabel: unread > 0 ? t('dashboard.notifications.ariaUnread', {'count': unread}) : t('dashboard.notifications.title'),
                      onPressed: () => showBellSheet(context),
                    ),
                  if (!viewer) ...[
                    const SizedBox(width: 2),
                    KButton(
                      label: t('dashboard.home.trade'),
                      icon: LucideIcons.candlestickChart,
                      size: KButtonSize.sm,
                      onPressed: () => context.push('/trader'),
                    ),
                  ],
                  const SizedBox(width: 2),
                  KPressable(
                    onTap: () => showProfileMenu(context),
                    semanticLabel: t('shell.accountMenu'),
                    child: KAvatar(name: me?.name ?? '', size: 34, verified: me?.kycStatus == KycStatus.verified),
                  ),
                ],
              ),
            ),
          ),
          if (subs != null)
            KSubNav(
              labels: [for (final s in subs!) t(s.labelKey)],
              icons: [for (final s in subs!) s.icon],
              current: subs!.indexOf(activeSub(path, subs!) ?? subs!.first),
              onSelect: (i) => context.go(subs![i].href),
            ),
        ],
      ),
    );
    if (fade != null) {
      final v = fade!;
      return KFrosted(
        color: k.bg.withValues(alpha: 0.82 * v),
        blur: 24 * v,
        border: Border(
          bottom: BorderSide(color: k.line.withValues(alpha: k.line.a * v), width: 0.6),
        ),
        child: Opacity(opacity: v, child: content),
      );
    }
    // frosted once the page scrolls under it (web .k-topbar[data-scrolled])
    return AnimatedContainer(
      duration: const Duration(milliseconds: 200),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: scrolled ? k.line : Colors.transparent, width: 0.6)),
      ),
      child: scrolled ? KFrosted(color: k.bg.withValues(alpha: 0.82), child: content) : content,
    );
  }
}

/// Over the Dashboard's picture: the floating controls while the picture shows, the frosted header once the sheet
/// is up or the picture page starts sliding away under a finger (and back when the slide returns); a 180 ms
/// crossfade, touches only on the layer that is meant to be there.
class _HeroChrome extends StatefulWidget {
  const _HeroChrome({required this.collapsed, required this.slide, required this.controls, required this.header});
  final bool collapsed;

  /// The module pager's slide (chrome.dart): past 0.02 the picture is leaving.
  final ValueListenable<double> slide;
  final Widget controls;
  final Widget Function(double fade) header;

  @override
  State<_HeroChrome> createState() => _HeroChromeState();
}

class _HeroChromeState extends State<_HeroChrome> with SingleTickerProviderStateMixin {
  late bool _up = _target;
  late final AnimationController _fade = AnimationController(vsync: this, duration: const Duration(milliseconds: 180), value: _up ? 1 : 0);

  bool get _target => widget.collapsed || widget.slide.value > 0.02;

  @override
  void initState() {
    super.initState();
    widget.slide.addListener(_sync);
  }

  @override
  void didUpdateWidget(_HeroChrome old) {
    super.didUpdateWidget(old);
    if (old.slide != widget.slide) {
      old.slide.removeListener(_sync);
      widget.slide.addListener(_sync);
    }
    _sync();
  }

  void _sync() {
    final t = _target;
    if (t == _up) return;
    _up = t;
    _fade.animateTo(t ? 1 : 0, curve: Curves.easeOut);
  }

  @override
  void dispose() {
    widget.slide.removeListener(_sync);
    _fade.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _fade,
    builder: (context, _) {
      final v = _fade.value;
      return Stack(
        children: [
          if (v < 1)
            IgnorePointer(
              ignoring: _up,
              child: v == 0 ? widget.controls : Opacity(opacity: 1 - v, child: widget.controls),
            ),
          if (v > 0) IgnorePointer(ignoring: !_up, child: widget.header(v)),
        ],
      );
    },
  );
}

/// The header's controls as round glass buttons over the photo (web html[data-hero="on"]: white on glass): the brand
/// disc at the start; search, the bell, Trade and the avatar at the end (the same rules as the header: viewer sessions
/// get no bell and no Trade). In the demo, the strip stays at the very top.
class _HeroControls extends ConsumerWidget {
  const _HeroControls({super.key, required this.nav, required this.demo});
  final List<NavModule> nav;
  final bool demo;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final me = ref.watch(meProvider);
    final unread = ref.watch(notificationsProvider.select((s) => s.unread));
    final viewer = me?.viewer != null;
    final glass = Colors.white.withValues(alpha: 0.16);
    final shadow = [BoxShadow(color: Colors.black.withValues(alpha: 0.18), offset: const Offset(0, 6), blurRadius: 16)];
    return SafeArea(
      bottom: false,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (demo) KFrosted(color: k.bar, child: const _DemoStrip()),
          Padding(
            padding: const EdgeInsetsDirectional.only(start: 14, end: 12, top: 8),
            child: Row(
              children: [
                KPressable(onTap: () => context.go('/'), semanticLabel: t('shell.nav.dashboard'), child: const KBrandAvatar()),
                const Spacer(),
                KIconButton(
                  icon: LucideIcons.search,
                  size: 42,
                  iconSize: 19,
                  fill: glass,
                  shadows: shadow,
                  color: Colors.white,
                  semanticLabel: t('shell.search'),
                  onPressed: () => showCommandPalette(context, nav),
                ),
                if (!viewer) ...[
                  const SizedBox(width: 4),
                  KIconButton(
                    icon: LucideIcons.bell,
                    size: 42,
                    iconSize: 19,
                    fill: glass,
                    shadows: shadow,
                    color: Colors.white,
                    badge: unread,
                    semanticLabel: unread > 0 ? t('dashboard.notifications.ariaUnread', {'count': unread}) : t('dashboard.notifications.title'),
                    onPressed: () => showBellSheet(context),
                  ),
                  const SizedBox(width: 6),
                  KButton(label: t('dashboard.home.trade'), icon: LucideIcons.candlestickChart, onPressed: () => context.push('/trader')),
                ],
                const SizedBox(width: 6),
                KPressable(
                  onTap: () => showProfileMenu(context),
                  semanticLabel: t('shell.accountMenu'),
                  child: DecoratedBox(
                    decoration: BoxDecoration(shape: BoxShape.circle, boxShadow: shadow),
                    child: KAvatar(name: me?.name ?? '', size: 40, verified: me?.kycStatus == KycStatus.verified),
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/// "Demo · Sample data · Exit demo": the strip over the header while the in-app demo runs, so the sample client is
/// never taken for a live account. Exit = the demo's Log out (the profile menu and More do the same).
class _DemoStrip extends ConsumerWidget {
  const _DemoStrip();

  static const double height = 28;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    return Container(
      height: height,
      color: k.warnSoft,
      padding: const EdgeInsetsDirectional.only(start: 14, end: 6),
      child: Row(
        children: [
          Icon(LucideIcons.flaskConical, size: 13, color: k.warn),
          const SizedBox(width: 6),
          Expanded(
            child: Text(
              '${t('common.demo')} · ${t('app.demo.sampleData')}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.caption.copyWith(color: k.warn, fontWeight: FontWeight.w600),
            ),
          ),
          KTextButton(
            label: t('app.demo.exit'),
            color: k.warn,
            style: context.text.caption.copyWith(fontWeight: FontWeight.w700),
            onPressed: () => ref.read(authProvider.notifier).logout(),
          ),
        ],
      ),
    );
  }
}

/// The floating bar (web .k-mobilebar, dark): five grey icons on a black glass pill, the open tab in a Kalks orange
/// disc with a black icon (web --k-tab-on / --k-tab-on-ic). The labels stay as tooltips and for assistive tech.
class _TabBar extends StatelessWidget {
  const _TabBar({required this.shell, required this.path, required this.nav});
  final StatefulNavigationShell shell;
  final String path;
  final List<NavModule> nav;

  static const Color _ink = Color(0xFF0A0A0B);

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final primary = [for (final key in kPrimaryModules) nav.where((m) => m.key == key).firstOrNull];
    final moreActive = !primary.any((m) => m != null && isActive(path, m));
    Widget item(int index, IconData icon, String label, bool active, {bool enabled = true}) => Expanded(
      child: Semantics(
        selected: active,
        button: true,
        label: label,
        child: Tooltip(
          message: label,
          child: GestureDetector(
            behavior: HitTestBehavior.opaque,
            onTap: !enabled
                ? null
                : () {
                    if (index != shell.currentIndex) KHaptics.selection();
                    // tapping the open tab returns to its first page
                    shell.goBranch(index, initialLocation: index == shell.currentIndex);
                  },
            child: SizedBox(
              height: KSize.tabBar,
              child: Center(
                child: AnimatedContainer(
                  duration: const Duration(milliseconds: 220),
                  curve: Curves.easeOutCubic,
                  width: 44,
                  height: 44,
                  decoration: BoxDecoration(
                    color: active ? k.ember : Colors.transparent,
                    shape: BoxShape.circle,
                    boxShadow: active
                        ? [BoxShadow(color: k.ember.withValues(alpha: 0.5), offset: const Offset(0, 8), blurRadius: 20, spreadRadius: -10)]
                        : null,
                  ),
                  child: Icon(icon, size: 22, color: active ? Colors.black : k.fg2.withValues(alpha: enabled ? 1 : 0.35)),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    final mods = ['dashboard', 'accounts', 'wallet', 'portfolio'];
    final fallbackIcons = [LucideIcons.layoutGrid, LucideIcons.layers, LucideIcons.wallet, LucideIcons.chartPie];
    return KFrosted(
      color: _ink.withValues(alpha: 0.86),
      borderRadius: BorderRadius.circular(32),
      border: Border.all(color: Colors.white.withValues(alpha: 0.1)),
      shadows: k.shadowPop,
      child: Row(
        children: [
          for (var i = 0; i < 4; i++)
            item(
              i,
              primary[i]?.icon ?? fallbackIcons[i],
              t(primary[i]?.labelKey ?? 'shell.nav.${mods[i]}'),
              primary[i] != null && isActive(path, primary[i]!),
              // a module the broker switched off (wallet) or a view-only login without it
              enabled: primary[i] != null,
            ),
          item(4, LucideIcons.layoutGrid, t('shell.more'), moreActive),
        ],
      ),
    );
  }
}

// A module's pages side by side (Overview · Deposit · Withdraw · …, the entries of its tabs) in a PageView: a finger
// slides between them and a tab tap slides the pager; both keep the web's path in the URL, so links and deep links
// stay what they are. The router gives every sub-route of a module the same page under one key (router.dart
// _moduleRoutes), so the Navigator keeps this widget's State and its PageController while the path moves between the
// pages; every page visited stays alive with its scroll and data. A section's first page keeps its photo (no top
// padding) while its neighbours lay out under the header; the shell follows the slide through
// modulePagerOffsetProvider and the page in front's scroll through ModulePageScrollNotification (chrome.dart).
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../core/auth/auth_controller.dart';
import '../core/config/app_config.dart';
import '../core/models/user.dart';
import '../features/common/stub_screen.dart';
import '../ui/ui.dart';
import 'chrome.dart';
import 'nav.dart';
import 'page_hero.dart';

/// A module's screens by web path (the router's registries, `_c1Screens` / `_c2Screens`).
typedef ModuleScreens = Map<String, Widget Function(GoRouterState)>;

/// The screen of a web path from its registry; a path without one is a route stub. The router and the pager both
/// look screens up here.
Widget moduleScreen(ModuleScreens screens, String path, GoRouterState state) => screens[path]?.call(state) ?? StubScreen(path: path);

/// The slide of a tab tap.
const Duration kModulePagerSlide = Duration(milliseconds: 300);

class ModulePager extends ConsumerStatefulWidget {
  const ModulePager({super.key, required this.moduleKey, required this.state, required this.screens});

  /// The module (NavModule.key) whose pages this pager holds.
  final String moduleKey;

  /// The sub-route in front: its path picks the page, its query goes to that page.
  final GoRouterState state;
  final ModuleScreens screens;

  @override
  ConsumerState<ModulePager> createState() => _ModulePagerState();
}

class _ModulePagerState extends ConsumerState<ModulePager> {
  late final PageController _controller;

  /// The pages, in tab order (the navigation this client sees).
  List<NavSub> _subs = const [];

  /// The page the URL names.
  int _index = 0;

  /// During a tab tap across several pages, the page in front is swapped next to the target so the slide crosses no
  /// other page (TabBarView does the same): slot `$1` shows page `$2` and slot `$2` shows page `$1`.
  (int, int)? _swap;

  /// A tab tap's slide is under way: the pages the PageView reports on the way are not the URL's business.
  bool _animating = false;
  int _gen = 0;

  /// On screen: the branch in front and no detail page over it. Only then the shell follows this pager.
  bool _active = false;
  bool _moveScheduled = false;
  bool _pendingAnimate = false;

  /// The route state each page was last opened with (its query), and the last vertical scroll of each page.
  final Map<String, GoRouterState> _states = {};
  final Map<String, ScrollMetrics> _metrics = {};

  /// The slot a finger started dragging from (null for a tab tap's slide).
  int? _dragFrom;

  String get _path => widget.state.uri.path;

  int _subAt(int slot) {
    final s = _swap;
    if (s == null) return slot;
    return slot == s.$1 ? s.$2 : (slot == s.$2 ? s.$1 : slot);
  }

  /// The swap is its own inverse.
  int _slotOf(int sub) => _subAt(sub);

  bool get _ready => _controller.hasClients && _controller.position.hasPixels && _controller.position.hasViewportDimension;

  /// The slot in front (the nearest page boundary).
  int get _frontSlot => _ready ? (_controller.page ?? _slotOf(_index)).round().clamp(0, _subs.length - 1) : _slotOf(_index);

  /// The module's pages for this client, with `path` among them (a page outside the list, a redirect on its way, is
  /// shown alone).
  List<NavSub> _subsFor(String path, AppConfig cfg, SessionUser? me) {
    final subs = navFor(cfg, me).where((m) => m.key == widget.moduleKey).firstOrNull?.sub ?? const <NavSub>[];
    if (subs.any((s) => s.href == path)) return subs;
    final all = kNav.firstWhere((m) => m.key == widget.moduleKey).sub;
    return [all.where((s) => s.href == path).firstOrNull ?? all.first];
  }

  int _indexOf(String path) => _subs.indexWhere((s) => s.href == path).clamp(0, _subs.length - 1);

  @override
  void initState() {
    super.initState();
    _subs = _subsFor(_path, ref.read(configProvider), ref.read(meProvider));
    _index = _indexOf(_path);
    _states[_path] = widget.state;
    _controller = PageController(initialPage: _index);
  }

  @override
  void didUpdateWidget(ModulePager old) {
    super.didUpdateWidget(old);
    _states[_path] = widget.state;
    if (_path != old.state.uri.path) _sync(_subsFor(_path, ref.read(configProvider), ref.read(meProvider)));
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  /// A new path (a tab, a link, a finger that moved the page) or page list: the page the URL names comes in front —
  /// sliding when this pager is on screen, at once when it is not (a link from another tab).
  void _sync(List<NavSub> subs) {
    final changed = !listEquals([for (final s in subs) s.href], [for (final s in _subs) s.href]);
    _subs = subs;
    final target = _indexOf(_path);
    if (target == _index && !changed) return;
    _index = target;
    _report();
    _schedule(animate: _active && !changed);
  }

  /// Moves the pager after this frame: a route change arrives while the tree builds, and the controller must not
  /// jump (nor the slide publish) during a build.
  void _schedule({required bool animate}) {
    _pendingAnimate = animate;
    if (_moveScheduled) return;
    _moveScheduled = true;
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _moveScheduled = false;
      if (mounted) _moveTo(_index, animate: _pendingAnimate);
    });
  }

  void _moveTo(int target, {required bool animate}) {
    if (!_ready) return;
    if (_swap != null) _unswap();
    final from = _frontSlot;
    if (target == from) return;
    if (!animate) {
      _controller.jumpToPage(target);
      return;
    }
    if ((target - from).abs() > 1) {
      // slide across one page only: the page in front moves next to the target, the pager jumps there (nothing moves
      // on screen) and slides one page
      final beside = target > from ? target - 1 : target + 1;
      setState(() => _swap = (from, beside));
      _animating = true;
      _controller.jumpToPage(beside);
    }
    _animate(target);
  }

  Future<void> _animate(int slot) async {
    final gen = ++_gen;
    _animating = true;
    await _controller.animateToPage(slot, duration: kModulePagerSlide, curve: Curves.easeOutCubic);
    if (!mounted || gen != _gen) return;
    _animating = false;
    if (_swap != null) _unswap();
  }

  /// Back to one page per slot. The page in front keeps its place; done mid-slide (a finger caught the pager), the
  /// pager jumps so the page in front stays where it is.
  void _unswap() {
    if (_swap == null || !_ready) return;
    final p = _controller.position;
    final f = p.pixels / p.viewportDimension;
    final r = f.round();
    final sub = _subAt(r);
    setState(() => _swap = null);
    if (sub != r) p.jumpTo(p.pixels + (sub - r) * p.viewportDimension);
    // the slide is measured from the first page's slot, which just moved
    _publish();
  }

  /// The PageView's page changed (a finger past the middle, or a slide): the URL follows a finger.
  void _onPage(int slot) {
    if (_animating) return;
    final sub = _subAt(slot);
    if (sub == _index || sub >= _subs.length) return;
    _index = sub;
    // the slide first (the PageView tells us before the pager's own listener sees this scroll), then the page
    _publish();
    _report();
    context.go(_subs[sub].href);
  }

  /// The shell follows the page in front: its last known scroll, again, when it changes or the pager comes on screen.
  void _report() {
    if (!_active || _index >= _subs.length) return;
    final href = _subs[_index].href;
    final m = _metrics[href];
    if (m != null) ModulePageScrollNotification(href, m).dispatch(context);
  }

  void _onPageScroll(String href, ScrollMetrics m) {
    _metrics[href] = m;
    if (_active && _index < _subs.length && _subs[_index].href == href) ModulePageScrollNotification(href, m).dispatch(context);
  }

  /// The slide, in pages from the module's first page (whatever slot it sits in).
  void _publish() {
    if (!_active || !_ready) return;
    final p = _controller.position;
    ref.read(modulePagerOffsetProvider).value = p.pixels / p.viewportDimension - _slotOf(0);
  }

  bool _onPagerScroll(ScrollNotification n) {
    if (n.depth != 0 || n.metrics.axis != Axis.horizontal) return false;
    if (n is ScrollStartNotification) {
      _dragFrom = n.dragDetails == null ? null : _frontSlot;
    } else if (n is ScrollUpdateNotification) {
      _publish();
    } else if (n is ScrollEndNotification) {
      _publish();
      // a swipe settled on another page (a tab tap had its own tick)
      if (_dragFrom != null && _frontSlot != _dragFrom) KHaptics.tap();
      _dragFrom = null;
    }
    return false;
  }

  GoRouterState _plainState(String href) => GoRouterState(
    GoRouter.of(context).configuration,
    uri: Uri(path: href),
    matchedLocation: href,
    fullPath: href,
    pathParameters: const {},
    pageKey: ValueKey(href),
  );

  @override
  Widget build(BuildContext context) {
    final cfg = ref.watch(configProvider);
    final me = ref.watch(meProvider);
    final subs = _subsFor(_path, cfg, me);
    if (!listEquals([for (final s in subs) s.href], [for (final s in _subs) s.href])) _sync(subs);
    final active = TickerMode.valuesOf(context).enabled && (ModalRoute.of(context)?.isCurrent ?? true);
    if (active && !_active) {
      // back on screen: the shell takes this pager's slide and page from here on
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || !_active) return;
        _publish();
        _report();
      });
    }
    _active = active;
    if (_subs.length <= 1) return moduleScreen(widget.screens, _path, widget.state);

    final headerH = shellHeaderHeight(context);
    final mq = MediaQuery.of(context);
    return NotificationListener<ScrollNotification>(
      onNotification: _onPagerScroll,
      child: PageView.builder(
        controller: _controller,
        onPageChanged: _onPage,
        itemCount: _subs.length,
        findChildIndexCallback: (key) {
          final i = _subs.indexWhere((s) => s.href == (key as ValueKey<String>).value);
          return i < 0 ? null : _slotOf(i);
        },
        itemBuilder: (context, slot) {
          final href = _subs[_subAt(slot)].href;
          return _PagerPage(
            key: ValueKey(href),
            pager: _controller,
            onScroll: (m) => _onPageScroll(href, m),
            child: MediaQuery(
              // a photo page runs under the status bar; every other page sits under the header
              data: mq.copyWith(padding: mq.padding.copyWith(top: pageHeroAt(href, cfg) ? 0 : headerH)),
              child: moduleScreen(widget.screens, href, _states[href] ?? _plainState(href)),
            ),
          );
        },
      ),
    );
  }
}

/// One page of the pager: kept alive once built (its scroll and data stay), its own vertical scroll reported to the
/// pager (depth 0 here: the page's own scroll, not a list inside it).
class _PagerPage extends StatefulWidget {
  const _PagerPage({super.key, required this.pager, required this.onScroll, required this.child});
  final PageController pager;
  final ValueChanged<ScrollMetrics> onScroll;
  final Widget child;

  @override
  State<_PagerPage> createState() => _PagerPageState();
}

class _PagerPageState extends State<_PagerPage> with AutomaticKeepAliveClientMixin {
  @override
  bool get wantKeepAlive => true;

  @override
  Widget build(BuildContext context) {
    super.build(context);
    return NotificationListener<ScrollMetricsNotification>(
      onNotification: (n) {
        if (n.depth == 0 && n.metrics.axis == Axis.vertical) widget.onScroll(n.metrics);
        return false;
      },
      child: NotificationListener<ScrollUpdateNotification>(
        onNotification: (n) {
          if (n.depth == 0 && n.metrics.axis == Axis.vertical) widget.onScroll(n.metrics);
          return false;
        },
        child: _InView(pager: widget.pager, child: widget.child),
      ),
    );
  }
}

/// Tells Scrollable.ensureVisible that the page is already in view sideways: a page scrolling one of its sections
/// into view (Ask Kalks AI, a partner table) moves its own scroll only, never the pager. (ensureVisible asks every
/// viewport above the target, and the pager's would centre the section between two pages.)
class _InView extends SingleChildRenderObjectWidget {
  const _InView({required this.pager, required super.child});
  final PageController pager;

  @override
  RenderObject createRenderObject(BuildContext context) => _RenderInView(pager);

  @override
  void updateRenderObject(BuildContext context, _RenderInView renderObject) => renderObject.pager = pager;
}

class _RenderInView extends RenderProxyBox implements RenderAbstractViewport {
  _RenderInView(this.pager);
  PageController pager;

  @override
  RevealedOffset getOffsetToReveal(RenderObject target, double alignment, {Rect? rect, Axis? axis}) =>
      RevealedOffset(offset: pager.hasClients ? pager.position.pixels : 0, rect: rect ?? target.paintBounds);
}

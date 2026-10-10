import 'package:flutter/cupertino.dart';

import '../tokens.dart';
import 'haptics.dart';

/// A full-bleed picture at the top of a page (a section's photo hero, page_hero.dart): it runs under the status bar,
/// moves at 40 % of the scroll (parallax, stretched over a pull-down) and the page's content sits in a rounded "sheet"
/// with a grabber that slides up over it. The picture ends under the sheet's top corners (it fades out there), so the
/// sheet can be dark glass over the page's backdrop. The shell gives such a page no top padding and swaps its header
/// for floating controls while the picture shows (app_shell.dart).
class KPageHero {
  const KPageHero({required this.height, required this.picture, this.child, this.top, this.overlap = 28, this.radius = 32, this.sheetColor});

  /// The picture's height from the top of the screen; the sheet's edge sits [overlap] above it.
  final double height;

  /// The picture, drawn behind the page: it gets [height] + 40 px so it shows through the sheet's rounded corners.
  final Widget picture;

  /// What scrolls with the page over the picture (a gradient, titles, a button), laid out above the sheet.
  final Widget? child;

  /// The sheet's first row under the grabber (the module's pages).
  final Widget? top;

  /// How far the sheet reaches up over the picture.
  final double overlap;

  /// The sheet's top corners.
  final double radius;

  /// The sheet's fill (default: the page background at 62 %, glass over the backdrop).
  final Color? sheetColor;

  /// Where the picture is scrolled fully under the sheet (the shell's collapse point counts from here).
  double get sheetTop => height - overlap;
}

/// A page body that scrolls under the frosted header and tab bar (their heights come in as MediaQuery padding from
/// the shell), with iOS pull-to-refresh. Use it for every Client Area page so spacing stays the same everywhere.
/// With [hero] the page opens on a picture and its content lives in the sheet over it.
class KPageScroll extends StatelessWidget {
  const KPageScroll({
    super.key,
    required this.children,
    this.onRefresh,
    this.padding = const EdgeInsets.fromLTRB(KSpace.page, 12, KSpace.page, 24),
    this.controller,
    this.hero,
  });

  /// The page's blocks, top to bottom (the web's section order).
  final List<Widget> children;

  /// Pull to refresh (reload the page's data); null disables it.
  final Future<void> Function()? onRefresh;
  final EdgeInsets padding;
  final ScrollController? controller;
  final KPageHero? hero;

  Widget _refresh(BuildContext context, {Color? color}) => CupertinoSliverRefreshControl(
    onRefresh: () async {
      KHaptics.medium();
      await onRefresh!();
    },
    builder: (context, mode, pulled, trigger, indicator) => Center(
      child: Padding(
        padding: const EdgeInsets.only(top: 8),
        child: Opacity(
          opacity: (pulled / trigger).clamp(0.0, 1.0),
          child: CupertinoActivityIndicator(
            color: color ?? context.k.fg3,
            animating: mode == RefreshIndicatorMode.refresh || mode == RefreshIndicatorMode.armed,
          ),
        ),
      ),
    ),
  );

  @override
  Widget build(BuildContext context) {
    final mq = MediaQuery.paddingOf(context);
    final content = SliverPadding(
      padding: padding.copyWith(bottom: padding.bottom + mq.bottom),
      sliver: SliverList.list(children: children),
    );
    if (hero != null) {
      return _HeroScroll(
        hero: hero!,
        controller: controller,
        // the spinner over the picture
        refresh: onRefresh == null ? null : _refresh(context, color: const Color(0xFFFFFFFF)),
        content: content,
      );
    }
    return CustomScrollView(
      controller: controller,
      slivers: [
        SliverPadding(padding: EdgeInsets.only(top: mq.top)),
        if (onRefresh != null) _refresh(context),
        content,
      ],
    );
  }
}

/// The picture behind a transparent lead sliver, then the sheet (a decorated sliver group: grabber, the module's
/// pages, the content). The picture is not a sliver: an earlier sliver paints over later ones, so a picture under
/// the sheet's corners has to sit behind the scroll view.
class _HeroScroll extends StatefulWidget {
  const _HeroScroll({required this.hero, required this.controller, required this.refresh, required this.content});
  final KPageHero hero;
  final ScrollController? controller;
  final Widget? refresh;
  final Widget content;

  @override
  State<_HeroScroll> createState() => _HeroScrollState();
}

class _HeroScrollState extends State<_HeroScroll> {
  /// The page's scroll offset (negative while pulled down).
  final _offset = ValueNotifier<double>(0);

  @override
  void dispose() {
    _offset.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final hero = widget.hero;
    final radius = Radius.circular(hero.radius);
    return Stack(
      children: [
        Positioned(
          top: 0,
          left: 0,
          right: 0,
          child: ValueListenableBuilder<double>(
            valueListenable: _offset,
            child: hero.picture,
            builder: (context, y, picture) {
              // the picture ends under the sheet's corners: fully under the sheet, nothing to draw
              final visible = hero.sheetTop - y + hero.radius;
              if (visible <= 0) return const SizedBox.shrink();
              final pull = y < 0 ? -y : 0.0;
              final fade = hero.radius * 1.4;
              return SizedBox(
                height: visible,
                child: ShaderMask(
                  blendMode: BlendMode.dstIn,
                  shaderCallback: (r) => LinearGradient(
                    begin: Alignment.topCenter,
                    end: Alignment.bottomCenter,
                    colors: const [Color(0xFFFFFFFF), Color(0xFFFFFFFF), Color(0x00FFFFFF)],
                    stops: [0, ((r.height - fade) / r.height).clamp(0.0, 1.0), 1],
                  ).createShader(r),
                  child: Stack(
                    children: [Positioned(top: y > 0 ? -0.4 * y : 0, left: 0, right: 0, height: hero.height + 40 + pull, child: picture!)],
                  ),
                ),
              );
            },
          ),
        ),
        NotificationListener<ScrollUpdateNotification>(
          onNotification: (n) {
            if (n.depth == 0 && n.metrics.axis == Axis.vertical) _offset.value = n.metrics.pixels;
            return false;
          },
          child: CustomScrollView(
            controller: widget.controller,
            slivers: [
              ?widget.refresh,
              SliverToBoxAdapter(
                child: SizedBox(height: hero.sheetTop, child: hero.child),
              ),
              DecoratedSliver(
                decoration: BoxDecoration(
                  color: hero.sheetColor ?? k.bg.withValues(alpha: 0.62),
                  borderRadius: BorderRadius.vertical(top: radius),
                  border: Border(top: BorderSide(color: k.lineTop, width: 0.6)),
                ),
                sliver: SliverMainAxisGroup(
                  slivers: [
                    SliverToBoxAdapter(
                      child: Column(
                        children: [
                          const SizedBox(height: 10),
                          Container(
                            width: 36,
                            height: 5,
                            decoration: BoxDecoration(color: k.fg3.withValues(alpha: 0.4), borderRadius: BorderRadius.circular(3)),
                          ),
                          if (hero.top != null) ...[const SizedBox(height: 8), hero.top!],
                        ],
                      ),
                    ),
                    widget.content,
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

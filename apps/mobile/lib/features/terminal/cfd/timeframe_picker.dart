// The chart's timeframe control, TradingView style (owner request 2026-10: no sliding through a strip): one chip with
// the timeframe on screen ("M15 ▾") and the trader's starred timeframes next to it, as many as fit the phone's width;
// the chip opens every timeframe in a sheet (Minutes · Hours · Days and longer), each with a star that pins it next
// to the chip. Favourites are saved with the workspace (kalks.trader.workspace `tfFavourites`).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/workspace.dart';
import '../widgets/kit.dart';

/// Width of the timeframe chip, and of each starred timeframe's touch target.
const double kTfChipWidth = 60;
const double kTfFavouriteWidth = 44;

/// The timeframes of the sheet, by group (an i18n key each).
const List<(String, List<String>)> kTfGroups = [
  ('app.trader.tf.minutes', ['M1', 'M5', 'M15', 'M30']),
  ('app.trader.tf.hours', ['H1', 'H4']),
  ('app.trader.tf.days', ['D1', 'W1', 'MN']),
];

/// How many starred timeframes fit next to the chip in `width`.
int favouritesThatFit(double width) => ((width - 6 - kTfChipWidth) / kTfFavouriteWidth).floor().clamp(0, kTimeframes.length);

void _pick(WidgetRef ref, String tf) {
  if (ref.read(workspaceProvider).tf == tf) return;
  KHaptics.selection();
  ref.read(workspaceProvider.notifier).update((w) => w.copyWith(tf: tf));
}

/// The chip and the starred timeframes, over the top of the plot (44 px tall touch targets on a 34 px pill).
class TimeframeBar extends ConsumerWidget {
  const TimeframeBar({super.key, required this.glass});

  /// The pill's fill (the chart's floating controls share it).
  final Color glass;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final tf = ref.watch(workspaceProvider.select((w) => w.tf));
    final favs = ref.watch(workspaceProvider.select((w) => w.tfFavourites)).where((x) => x != tf).toList();
    return LayoutBuilder(
      builder: (context, box) {
        final shown = favs.take(favouritesThatFit(box.maxWidth)).toList();
        return SizedBox(
          height: 44,
          child: Stack(
            children: [
              Positioned.fill(
                top: 5,
                bottom: 5,
                child: DecoratedBox(
                  decoration: BoxDecoration(color: glass, borderRadius: BorderRadius.circular(17)),
                ),
              ),
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 3),
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Semantics(
                      selected: true,
                      child: KPressable(
                        key: const ValueKey('chart-tf'),
                        pressedScale: 0.96,
                        semanticLabel: t('app.trader.tf.current', {'tf': tf}),
                        onTap: () => showTimeframeSheet(context),
                        child: Container(
                          width: kTfChipWidth,
                          height: 28,
                          alignment: Alignment.center,
                          decoration: BoxDecoration(color: k.ember, borderRadius: BorderRadius.circular(14)),
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Text(
                                tf,
                                style: context.text.mono(12, weight: FontWeight.w800, color: k.onEmber),
                              ),
                              const SizedBox(width: 3),
                              Icon(LucideIcons.chevronDown, size: 12, color: k.onEmber),
                            ],
                          ),
                        ),
                      ),
                    ),
                    for (final f in shown)
                      Semantics(
                        selected: false,
                        child: KPressable(
                          key: ValueKey('chart-tf-$f'),
                          pressedScale: 1,
                          semanticLabel: f,
                          onTap: () => _pick(ref, f),
                          child: SizedBox(
                            width: kTfFavouriteWidth,
                            height: 28,
                            child: Center(
                              child: Text(
                                f,
                                style: context.text.mono(11.5, weight: FontWeight.w700, color: k.fg2),
                              ),
                            ),
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}

/// Every timeframe, grouped, with a star each: a tap shows it, the star pins it next to the chip.
Future<void> showTimeframeSheet(BuildContext context) =>
    showKSheet<void>(context, title: context.t('chart.menu.timeframes'), builder: (ctx) => const _TimeframeSheet());

class _TimeframeSheet extends ConsumerWidget {
  const _TimeframeSheet();

  /// "1 minute", "4 hours", "Daily" … under the code.
  static String _caption(T t, String tf) {
    final n = int.tryParse(tf.substring(1)) ?? 1;
    return switch (tf[0]) {
      'M' when tf != 'MN' => t('security.idle.minutes', {'count': n}),
      'H' => t('security.idle.hours', {'count': n}),
      'D' => t('partner.schedule.daily'),
      'W' => t('partner.schedule.weekly'),
      _ => t('partner.schedule.monthly'),
    };
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final tf = ref.watch(workspaceProvider.select((w) => w.tf));
    final favs = ref.watch(workspaceProvider.select((w) => w.tfFavourites));
    Widget tile(String x, double width) {
      final selected = x == tf;
      final fav = favs.contains(x);
      return Semantics(
        selected: selected,
        child: KPressable(
          key: ValueKey('tf-sheet-$x'),
          pressedScale: 0.97,
          semanticLabel: '$x, ${_caption(t, x)}',
          onTap: () {
            _pick(ref, x);
            Navigator.of(context).maybePop();
          },
          child: Container(
            width: width,
            height: 72,
            padding: const EdgeInsetsDirectional.only(start: 12, bottom: 10),
            decoration: BoxDecoration(
              color: selected ? k.ember.withValues(alpha: 0.12) : k.surface2,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: selected ? k.ember : Colors.transparent, width: 1.5),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        x,
                        style: context.text.mono(15, weight: FontWeight.w800, color: selected ? k.ember : k.fg),
                      ),
                    ),
                    // the star: its own 44 px target in the tile's corner
                    KPressable(
                      key: ValueKey('tf-star-$x'),
                      pressedScale: 0.85,
                      semanticLabel: '$x: ${t(fav ? 'market.menu.removeFavourite' : 'market.menu.addFavourite')}',
                      onTap: () {
                        KHaptics.selection();
                        ref.read(workspaceProvider.notifier).toggleTfFavourite(x);
                      },
                      child: Icon(fav ? Icons.star_rounded : LucideIcons.star, size: fav ? 21 : 17, color: fav ? k.gold : k.fg3),
                    ),
                  ],
                ),
                Text(
                  _caption(t, x),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.caption.copyWith(color: k.fg3, fontSize: 11),
                ),
              ],
            ),
          ),
        ),
      );
    }

    return SingleChildScrollView(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
      child: LayoutBuilder(
        builder: (context, box) {
          const gap = 8.0;
          final w = (box.maxWidth - 2 * gap) / 3;
          return Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final (label, list) in kTfGroups) ...[
                TSectionLabel(t(label), padding: const EdgeInsets.fromLTRB(4, 10, 4, 8)),
                Wrap(spacing: gap, runSpacing: gap, children: [for (final x in list) tile(x, w)]),
              ],
              const SizedBox(height: 14),
              Text(
                t('app.trader.tf.hint'),
                textAlign: TextAlign.center,
                style: context.text.footnote.copyWith(color: k.fg3),
              ),
            ],
          );
        },
      ),
    );
  }
}

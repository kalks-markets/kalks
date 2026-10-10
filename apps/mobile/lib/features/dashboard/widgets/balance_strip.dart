// Home's money summary under the hero (web components/dashboard/home/balance-strip.tsx): the cells on one glass card
// — total balance (with the eye that hides every amount), equity, wallet, today's P&L with the last 7 days, open
// positions, rewards — two to a row on phones, hairlines between them. Each cell opens its page.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../../ui/ui.dart';
import '../../common/hide_money.dart';

class StripCell {
  const StripCell({
    required this.key,
    required this.label,
    required this.value,
    this.unit,
    this.sub,
    this.extra,
    this.href,
    this.big = false,
    this.secret = false,
  });
  final String key;
  final String label;

  /// The figure, in the cell's style (light, large).
  final Widget Function(TextStyle style) value;
  final String? unit;
  final String? sub;

  /// Under the value (the P&L's last days); left out while amounts are hidden.
  final Widget? extra;
  final String? href;

  /// The total: a larger number, a full row.
  final bool big;

  /// An amount: masked when the client hides amounts.
  final bool secret;
}

class BalanceStrip extends ConsumerWidget {
  const BalanceStrip({super.key, required this.cells});
  final List<StripCell> cells;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final hidden = ref.watch(hideBalancesProvider);
    Widget cell(StripCell c, {bool first = false}) {
      final masked = hidden && c.secret;
      final valueStyle = context.text.largeTitle.copyWith(
        fontSize: c.big ? 34 : 24,
        fontWeight: FontWeight.w300,
        height: 1,
        letterSpacing: c.big ? -1.1 : -0.7,
        fontFeatures: kTabular,
      );
      final body = Container(
        width: double.infinity,
        padding: EdgeInsetsDirectional.fromSTEB(18, 18, first ? 52 : 18, 18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              c.label.toUpperCase(),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.micro.copyWith(color: k.fg3, fontSize: 10.5, letterSpacing: 1.1),
            ),
            const SizedBox(height: 10),
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Flexible(
                  child: FittedBox(
                    fit: BoxFit.scaleDown,
                    alignment: AlignmentDirectional.centerStart,
                    child: masked ? Text(kMoneyMask, style: valueStyle) : c.value(valueStyle),
                  ),
                ),
                if (c.unit != null && !masked) ...[
                  const SizedBox(width: 6),
                  Padding(
                    padding: const EdgeInsets.only(bottom: 2),
                    child: Text(c.unit!.toUpperCase(), style: context.text.micro.copyWith(color: k.fg3, fontSize: 10, letterSpacing: 0.8)),
                  ),
                ],
              ],
            ),
            if (c.sub != null) ...[
              const SizedBox(height: 8),
              Text(
                c.sub!,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12, height: 1.3),
              ),
            ],
            if (c.extra != null && !masked) ...[const SizedBox(height: 10), c.extra!],
          ],
        ),
      );
      final tappable = c.href == null
          ? body
          : KPressable(key: ValueKey('strip-${c.key}'), onTap: () => context.go(c.href!), pressedOpacity: 0.7, pressedScale: 1, child: body);
      if (!first) return tappable;
      return Stack(
        children: [
          tappable,
          const PositionedDirectional(top: 10, end: 8, child: HideMoneyButton()),
        ],
      );
    }

    // the total on its own row, then two to a row
    final big = cells.where((c) => c.big).toList();
    final rest = cells.where((c) => !c.big).toList();
    final rows = <Widget>[
      for (final c in big) cell(c, first: c == cells.first),
      for (var i = 0; i < rest.length; i += 2)
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Expanded(child: cell(rest[i], first: rest[i] == cells.first)),
              Container(width: 0.6, color: k.line),
              Expanded(child: i + 1 < rest.length ? cell(rest[i + 1]) : const SizedBox.shrink()),
            ],
          ),
        ),
    ];
    return KCard(
      key: const ValueKey('balance-strip'),
      padding: EdgeInsets.zero,
      radius: 26,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          for (var i = 0; i < rows.length; i++) ...[if (i > 0) Container(height: 0.6, color: k.line), rows[i]],
        ],
      ),
    );
  }
}

/// Thin bars, the last one orange (web MiniBars): the daily P&L of the last days, with their weekday initials.
class MiniBars extends StatelessWidget {
  const MiniBars({super.key, required this.values, this.labels, this.height = 22});
  final List<double> values;
  final List<String>? labels;
  final double height;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final max = values.fold<double>(1e-9, (m, v) => v.abs() > m ? v.abs() : m);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        SizedBox(
          height: height,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              for (var i = 0; i < values.length; i++) ...[
                if (i > 0) const SizedBox(width: 4),
                Expanded(
                  child: Container(
                    height: (values[i].abs() / max * height).clamp(3.0, height),
                    decoration: BoxDecoration(
                      color: i == values.length - 1 ? k.ember : (values[i] < 0 ? k.fg3.withValues(alpha: 0.4) : k.fg.withValues(alpha: 0.7)),
                      borderRadius: BorderRadius.circular(3),
                    ),
                  ),
                ),
              ],
            ],
          ),
        ),
        if (labels != null) ...[
          const SizedBox(height: 5),
          Row(
            children: [
              for (var i = 0; i < labels!.length; i++) ...[
                if (i > 0) const SizedBox(width: 4),
                Expanded(
                  child: Text(
                    labels![i],
                    textAlign: TextAlign.center,
                    maxLines: 1,
                    style: context.text.micro.copyWith(
                      fontSize: 9.5,
                      color: i == labels!.length - 1 ? k.fg : k.fg3,
                      fontWeight: i == labels!.length - 1 ? FontWeight.w700 : FontWeight.w500,
                    ),
                  ),
                ),
              ],
            ],
          ),
        ],
      ],
    );
  }
}

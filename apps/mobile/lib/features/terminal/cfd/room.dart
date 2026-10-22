// The trade room's look (owner design 2026-10, "MT5 layout but modern"): #121217 cards with radius 16 on the
// near-black ground, 16 px side margins everywhere, Geist Mono tabular prices, and the compact B / S / T tags for
// sides and stops. Also the one order path every SELL / BUY of the room takes (quotes, the symbol screen, the chart's
// pill): one-click trading on sends the market order at once, off opens the prefilled order ticket.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/trade_actions.dart';
import '../core/trade_math.dart';
import '../core/workspace.dart';
import 'order_sheet.dart';

/// Side margins of every card and row.
const double kRoomGutter = 16;

/// Card corner radius.
const double kRoomRadius = 16;

/// "B" / "S": the compact side letter (translatable; Latin in every language like MT5's tags).
String sideLetter(T t, String side) => t(side == 'buy' ? 'app.trader.tag.buy' : 'app.trader.tag.sell');

/// "B 0.50" / "S 0.10": a side with its volume.
String sideTag(T t, String side, double lots) => '${sideLetter(t, side)} ${fmtVol(lots)}';

/// "S 1.11850" / "T 1.12600": a stop loss / take profit with its price.
String stopTag(T t, String kind, int digits, double price) => '${t(kind == 'sl' ? 'chart.line.slHandle' : 'chart.line.tpHandle')} ${fmtPrice(digits, price)}';

/// A price split MT5-style: the head, the big figures and the raised last digit. 3 / 5 digits: the pipette is raised
/// and the two figures before it are big ("1.12" "24" "9"); 2 / 4 digits: the last two are big; 0 / 1 digit: the
/// last three characters ("83" "879", "674" "1.2").
({String head, String big, String sup}) splitQuote(double value, int digits) {
  final s = fmtPrice(digits, value);
  if (digits == 3 || digits == 5) {
    final body = s.substring(0, s.length - 1);
    return (head: body.substring(0, body.length - 2), big: body.substring(body.length - 2), sup: s.substring(s.length - 1));
  }
  final n = digits >= 2 ? 2 : 3;
  if (s.length <= n) return (head: '', big: s, sup: '');
  return (head: s.substring(0, s.length - n), big: s.substring(s.length - n), sup: '');
}

/// The quotes' column grid, shared by the header row, every quote row and the advanced row so they line up:
/// icon · symbol (flexible) · 24 h sparkline · SELL · BUY, with the same gaps and side padding.
@immutable
class QuoteGrid {
  const QuoteGrid({required this.icon, required this.spark, required this.button, required this.gap, required this.padStart, required this.padEnd});

  /// The grid for a card `width` wide: 80 px price buttons from 380 px up, narrower on small phones.
  factory QuoteGrid.of(double width) {
    final button = width >= 348 ? 80.0 : (width >= 320 ? 72.0 : 64.0);
    return QuoteGrid(icon: 36, spark: 40, button: button, gap: 6, padStart: 10, padEnd: 8);
  }

  final double icon, spark, button, gap, padStart, padEnd;

  /// The symbol column of a card `width` wide (what the fixed columns leave).
  double symbolWidth(double width) => width - padStart - padEnd - icon - spark - 2 * button - 4 * gap;

  /// Lays out the five cells on the grid.
  Widget row({required Widget icon, required Widget symbol, required Widget spark, required Widget sell, required Widget buy}) => Padding(
    padding: EdgeInsetsDirectional.only(start: padStart, end: padEnd),
    child: Row(
      children: [
        SizedBox(width: this.icon, child: icon),
        SizedBox(width: gap),
        Expanded(child: symbol),
        SizedBox(width: gap),
        SizedBox(width: this.spark, child: spark),
        SizedBox(width: gap),
        SizedBox(width: button, child: sell),
        SizedBox(width: gap),
        SizedBox(width: button, child: buy),
      ],
    ),
  );
}

/// A room card: #121217, radius 16, 16 px from the screen edges.
class RoomCard extends StatelessWidget {
  const RoomCard({super.key, required this.child, this.padding = EdgeInsets.zero, this.margin = const EdgeInsets.symmetric(horizontal: kRoomGutter), this.onTap});
  final Widget child;
  final EdgeInsetsGeometry padding, margin;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final card = Container(
      margin: onTap == null ? margin : null,
      padding: padding,
      decoration: BoxDecoration(color: context.k.surface, borderRadius: BorderRadius.circular(kRoomRadius)),
      clipBehavior: Clip.antiAlias,
      child: child,
    );
    if (onTap == null) return card;
    return Padding(
      padding: margin,
      child: KPressable(pressedScale: 0.99, pressedOpacity: 0.8, onTap: onTap, child: card),
    );
  }
}

/// The side badge of a position / order / deal: "B 0.50" filled with the side colour (or tinted, `filled: false`).
class SideBadge extends StatelessWidget {
  const SideBadge({super.key, required this.side, required this.lots, this.filled = true});
  final String side;
  final double lots;
  final bool filled;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final c = side == 'buy' ? k.up : k.down;
    return Container(
      height: 22,
      constraints: const BoxConstraints(minWidth: 22),
      padding: const EdgeInsets.symmetric(horizontal: 7),
      decoration: BoxDecoration(color: filled ? c : k.surface3, borderRadius: BorderRadius.circular(11)),
      // centred without stretching (a Wrap or a Column gives it the full width)
      child: Center(
        widthFactor: 1,
        child: Text(
        sideTag(context.t, side, lots),
        maxLines: 1,
        textDirection: TextDirection.ltr,
          style: context.text.mono(11, weight: FontWeight.w800, color: filled ? Colors.white : c),
        ),
      ),
    );
  }
}

/// A tiny stop chip, only shown when the stop is set: "S 1.11850" in the sell colour, "T 1.12600" in the buy colour.
class StopChip extends StatelessWidget {
  const StopChip({super.key, required this.kind, required this.digits, required this.price});

  /// sl | tp
  final String kind;
  final int digits;
  final double price;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      height: 18,
      padding: const EdgeInsets.symmetric(horizontal: 6),
      decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(9)),
      child: Center(
        widthFactor: 1,
        child: Text(
          stopTag(context.t, kind, digits, price),
          maxLines: 1,
          textDirection: TextDirection.ltr,
          style: context.text.mono(10, weight: FontWeight.w700, color: kind == 'sl' ? k.down : k.up),
        ),
      ),
    );
  }
}

/// A round 40 px icon button on the room's surface (44 px touch target, a spoken label).
class RoomIconButton extends StatelessWidget {
  const RoomIconButton({super.key, required this.icon, required this.label, required this.onTap, this.size = 40});
  final IconData icon;
  final String label;
  final VoidCallback? onTap;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KPressable(
      semanticLabel: label,
      onTap: onTap,
      child: Container(
        width: size,
        height: size,
        decoration: BoxDecoration(color: k.surface2, shape: BoxShape.circle),
        child: Icon(icon, size: 18, color: k.fg),
      ),
    );
  }
}

/// A rounded pill chip (period, order type, timeframe): ember when selected.
class RoomChip extends StatelessWidget {
  const RoomChip({super.key, required this.label, required this.selected, required this.onTap, this.height = 34, this.semanticLabel});
  final String label;
  final bool selected;
  final VoidCallback? onTap;
  final double height;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Semantics(
      selected: selected,
      child: KPressable(
        pressedScale: 0.97,
        semanticLabel: semanticLabel,
        onTap: onTap,
        child: Container(
          height: height,
          padding: const EdgeInsets.symmetric(horizontal: 14),
          alignment: Alignment.center,
          decoration: BoxDecoration(color: selected ? k.ember : k.surface2, borderRadius: BorderRadius.circular(height / 2)),
          child: Text(
            label,
            maxLines: 1,
            style: context.text.label.copyWith(fontSize: 12, fontWeight: FontWeight.w700, color: selected ? k.onEmber : k.fg2),
          ),
        ),
      ),
    );
  }
}

/// SELL / BUY from anywhere in the room, through the terminal's one order path: one-click trading on sends the
/// market order at once (with a haptic; the volume is remembered when it fills), off opens the order ticket prefilled
/// with the market, the side and the volume to confirm. True when an order went out and filled.
Future<bool> tradeFrom(BuildContext context, WidgetRef ref, String symbol, String side, {double? volume}) async {
  final ws = ref.read(workspaceProvider);
  final vol = volume ?? ws.lot;
  if (ws.oneClick) {
    KHaptics.medium();
    final ok = await ref.read(tradeActionsProvider).quickTrade(symbol, side, vol);
    if (ok && volume != null) ref.read(workspaceProvider.notifier).update((w) => w.copyWith(lot: vol));
    return ok;
  }
  if (context.mounted) unawaited(showOrderSheet(context, symbol: symbol, side: side, volume: vol));
  return false;
}

/// A stop loss / take profit field labelled just with its letter (S in the sell colour, T in the buy colour): a price
/// (or, in the order ticket, points) typed in Geist Mono. Empty = no stop.
class StopField extends StatefulWidget {
  const StopField({
    super.key,
    required this.kind,
    required this.value,
    required this.onChanged,
    this.placeholder,
    this.semanticLabel,
    this.background,
  });

  /// sl | tp
  final String kind;
  final String value;
  final ValueChanged<String> onChanged;
  final String? placeholder, semanticLabel;
  final Color? background;

  @override
  State<StopField> createState() => _StopFieldState();
}

class _StopFieldState extends State<StopField> {
  late final TextEditingController _c = TextEditingController(text: widget.value);
  final FocusNode _focus = FocusNode();

  @override
  void didUpdateWidget(StopField old) {
    super.didUpdateWidget(old);
    // a nudge, Clear or a unit switch from outside: shown at once (what is typed comes back unchanged, so the caret stays)
    if (widget.value != _c.text) _c.text = widget.value;
  }

  @override
  void dispose() {
    _c.dispose();
    _focus.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final t = context.t;
    final sl = widget.kind == 'sl';
    return Semantics(
      label: widget.semanticLabel,
      textField: true,
      child: Container(
        height: 46,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(color: widget.background ?? k.bg, borderRadius: BorderRadius.circular(14)),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Row(
            children: [
              Text(
                t(sl ? 'chart.line.slHandle' : 'chart.line.tpHandle'),
                style: context.text.mono(11, weight: FontWeight.w800, color: sl ? k.down : k.up),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: TextField(
                  controller: _c,
                  focusNode: _focus,
                  keyboardType: const TextInputType.numberWithOptions(decimal: true),
                  inputFormatters: [FilteringTextInputFormatter.allow(RegExp(r'[0-9.]'))],
                  style: context.text.mono(15, weight: FontWeight.w600, color: k.fg),
                  cursorColor: k.ember,
                  decoration: InputDecoration(
                    isCollapsed: true,
                    border: InputBorder.none,
                    hintText: widget.placeholder,
                    hintStyle: context.text.mono(13, color: k.fg3),
                  ),
                  onChanged: widget.onChanged,
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

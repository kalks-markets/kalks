import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';

import '../tokens.dart';
import '../typography.dart';
import 'haptics.dart';
import 'pressable.dart';

/// Segmented control with a sliding thumb (iOS), in the web's colours (.k-seg / .k-seg-on: the ember thumb).
/// `plain` gives the iOS white thumb on a grey track (forms, sheets).
class KSegmented<T> extends StatelessWidget {
  const KSegmented({
    super.key,
    required this.values,
    required this.labels,
    required this.selected,
    required this.onChanged,
    this.plain = false,
    this.height = 36,
    this.expand = true,
  });

  final List<T> values;
  final List<String> labels;
  final T selected;
  final ValueChanged<T> onChanged;
  final bool plain;
  final double height;

  /// Equal segments over the full width (otherwise each segment fits its label).
  final bool expand;

  @override
  Widget build(BuildContext context) {
    assert(values.length == labels.length && values.isNotEmpty);
    final k = context.k;
    final index = values.indexOf(selected).clamp(0, values.length - 1);
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final trackColor = plain ? k.surface3 : k.surface.withValues(alpha: 0.82);
    final thumbColor = plain ? (k.dark ? k.surface2 : Colors.white) : k.ember;
    final on = plain ? k.fg : k.onEmber;
    return Container(
      height: height,
      padding: const EdgeInsets.all(3),
      decoration: BoxDecoration(
        color: trackColor,
        borderRadius: BorderRadius.circular(height / 2),
        border: plain ? null : Border.all(color: k.line),
      ),
      child: LayoutBuilder(
        builder: (context, c) {
          final w = c.maxWidth / values.length;
          return Stack(
            children: [
              AnimatedPositioned(
                duration: const Duration(milliseconds: 260),
                curve: Curves.easeOutCubic,
                left: (rtl ? values.length - 1 - index : index) * w,
                top: 0,
                bottom: 0,
                width: w,
                child: Container(
                  decoration: BoxDecoration(
                    color: thumbColor,
                    borderRadius: BorderRadius.circular((height - 6) / 2),
                    boxShadow: plain
                        ? const [
                            BoxShadow(color: Color(0x1F000000), offset: Offset(0, 3), blurRadius: 8),
                            BoxShadow(color: Color(0x0A000000), offset: Offset(0, 3), blurRadius: 1),
                          ]
                        : [BoxShadow(color: k.ember.withValues(alpha: 0.5), offset: const Offset(0, 8), blurRadius: 18, spreadRadius: -10)],
                  ),
                ),
              ),
              Row(
                children: [
                  for (var i = 0; i < values.length; i++)
                    Expanded(
                      child: Semantics(
                        selected: i == index,
                        button: true,
                        child: GestureDetector(
                          behavior: HitTestBehavior.opaque,
                          onTap: () {
                            if (i == index) return;
                            KHaptics.selection();
                            onChanged(values[i]);
                          },
                          child: Center(
                            child: AnimatedDefaultTextStyle(
                              duration: const Duration(milliseconds: 200),
                              style: context.text.label.copyWith(fontWeight: FontWeight.w600, color: i == index ? on : k.fg2, fontSize: 13),
                              child: Text(labels[i], maxLines: 1, overflow: TextOverflow.ellipsis),
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ],
          );
        },
      ),
    );
  }
}

/// iOS switch in the brand colour (web Toggle: ember when on), with a selection haptic.
class KSwitch extends StatelessWidget {
  const KSwitch({super.key, required this.value, required this.onChanged, this.semanticLabel});
  final bool value;
  final ValueChanged<bool>? onChanged;
  final String? semanticLabel;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Semantics(
      label: semanticLabel,
      toggled: value,
      child: CupertinoSwitch(
        value: value,
        activeTrackColor: k.ember,
        inactiveTrackColor: k.surface3,
        onChanged: onChanged == null
            ? null
            : (v) {
                KHaptics.selection();
                onChanged!(v);
              },
      ),
    );
  }
}

/// The module's pages as text tabs with a sliding ember underline (web SubNav on phones): scrolls sideways, keeps
/// the current tab in view.
class KSubNav extends StatefulWidget {
  const KSubNav({super.key, required this.labels, required this.current, required this.onSelect, this.icons});
  final List<String> labels;
  final List<IconData>? icons;

  /// Index of the current page (-1 when none).
  final int current;
  final ValueChanged<int> onSelect;

  @override
  State<KSubNav> createState() => _KSubNavState();
}

class _KSubNavState extends State<KSubNav> {
  final _keys = <GlobalKey>[];

  @override
  void didUpdateWidget(KSubNav old) {
    super.didUpdateWidget(old);
    if (old.current != widget.current) _reveal();
  }

  @override
  void initState() {
    super.initState();
    _reveal();
  }

  final _scroll = ScrollController();

  @override
  void dispose() {
    _scroll.dispose();
    super.dispose();
  }

  /// Centres the current tab in this row only (Scrollable.ensureVisible would also scroll the page around a sub-nav
  /// that sits inside a page, e.g. the dashboard's activity tabs, every time it is built).
  void _reveal() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || widget.current < 0 || widget.current >= _keys.length || !_scroll.hasClients) return;
      final box = _keys[widget.current].currentContext?.findRenderObject();
      final viewport = box == null ? null : RenderAbstractViewport.maybeOf(box);
      if (box == null || viewport == null) return;
      final p = _scroll.position;
      final target = viewport.getOffsetToReveal(box, 0.5).offset.clamp(p.minScrollExtent, p.maxScrollExtent);
      if ((target - p.pixels).abs() > 0.5) _scroll.animateTo(target, duration: const Duration(milliseconds: 260), curve: Curves.easeOutCubic);
    });
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    while (_keys.length < widget.labels.length) {
      _keys.add(GlobalKey());
    }
    return SizedBox(
      height: KSize.subNav,
      child: ListView.builder(
        controller: _scroll,
        scrollDirection: Axis.horizontal,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        itemCount: widget.labels.length,
        itemBuilder: (context, i) {
          final on = i == widget.current;
          return KPressable(
            key: _keys[i],
            onTap: () => widget.onSelect(i),
            pressedScale: 1,
            minSize: 40,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 10),
              child: Stack(
                alignment: Alignment.bottomCenter,
                children: [
                  SizedBox(
                    height: KSize.subNav,
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        if (widget.icons != null) ...[Icon(widget.icons![i], size: 16, color: on ? k.ember : k.fg3), const SizedBox(width: 6)],
                        Text(
                          widget.labels[i],
                          style: context.text.label.copyWith(fontSize: 14, fontWeight: FontWeight.w600, color: on ? k.fg : k.fg3),
                        ),
                      ],
                    ),
                  ),
                  AnimatedContainer(
                    duration: const Duration(milliseconds: 220),
                    curve: Curves.easeOutCubic,
                    height: 2.5,
                    margin: const EdgeInsets.only(bottom: 2),
                    width: on ? 28 : 0,
                    decoration: BoxDecoration(color: k.ember, borderRadius: BorderRadius.circular(2)),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

/// The module's pages as pill chips (the dashboard's sheet): the current one in ink with the page colour on it, the
/// others outlined. 38 pt pills in a 44 pt row; scrolls sideways when they don't fit.
class KPillNav extends StatelessWidget {
  const KPillNav({
    super.key,
    required this.labels,
    required this.current,
    required this.onSelect,
    this.icons,
    this.padding = const EdgeInsets.symmetric(horizontal: 20),
  });
  final List<String> labels;
  final List<IconData>? icons;

  /// Index of the current page (-1 when none).
  final int current;
  final ValueChanged<int> onSelect;
  final EdgeInsets padding;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return SizedBox(
      height: KSize.touch,
      child: ListView.separated(
        scrollDirection: Axis.horizontal,
        padding: padding,
        itemCount: labels.length,
        separatorBuilder: (_, _) => const SizedBox(width: 8),
        itemBuilder: (context, i) {
          final on = i == current;
          final fg = on ? k.onEmber : k.fg;
          return KPressable(
            onTap: () => onSelect(i),
            pressedScale: 1,
            child: Container(
              height: 38,
              padding: const EdgeInsets.symmetric(horizontal: 15),
              decoration: BoxDecoration(
                color: on ? k.ember : Colors.transparent,
                borderRadius: BorderRadius.circular(19),
                border: on ? null : Border.all(color: k.line),
              ),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  if (icons != null) ...[Icon(icons![i], size: 15, color: on ? k.onEmber : k.fg2), const SizedBox(width: 6)],
                  Text(
                    labels[i],
                    style: context.text.label.copyWith(fontSize: 13.5, fontWeight: FontWeight.w600, color: fg),
                  ),
                ],
              ),
            ),
          );
        },
      ),
    );
  }
}

/// A − value + stepper (volume, amounts): taps step by `step`, holding repeats; the value is clamped and rounded to
/// `decimals`.
class KStepper extends StatefulWidget {
  const KStepper({
    super.key,
    required this.value,
    required this.onChanged,
    this.step = 0.01,
    this.min = 0,
    this.max = double.infinity,
    this.decimals = 2,
    this.label,
  });
  final double value;
  final ValueChanged<double> onChanged;
  final double step;
  final double min;
  final double max;
  final int decimals;
  final String? label;

  @override
  State<KStepper> createState() => _KStepperState();
}

class _KStepperState extends State<KStepper> {
  bool _repeating = false;

  double _clamp(double v) {
    final f = _pow10(widget.decimals);
    return ((v.clamp(widget.min, widget.max)) * f).roundToDouble() / f;
  }

  static double _pow10(int n) {
    var f = 1.0;
    for (var i = 0; i < n; i++) {
      f *= 10;
    }
    return f;
  }

  void _bump(int dir) {
    final next = _clamp(widget.value + dir * widget.step);
    if (next != widget.value) {
      KHaptics.selection();
      widget.onChanged(next);
    }
  }

  Future<void> _repeat(int dir) async {
    _repeating = true;
    var wait = 300;
    while (_repeating && mounted) {
      _bump(dir);
      await Future<void>.delayed(Duration(milliseconds: wait));
      wait = (wait * 0.8).clamp(60, 300).round();
    }
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    Widget btn(IconData icon, int dir) => GestureDetector(
      onLongPressStart: (_) => _repeat(dir),
      onLongPressEnd: (_) => _repeating = false,
      child: KPressable(
        onTap: () => _bump(dir),
        child: Container(
          width: 40,
          height: 40,
          alignment: Alignment.center,
          decoration: BoxDecoration(color: k.surface3, shape: BoxShape.circle),
          child: Icon(icon, size: 18, color: k.fg),
        ),
      ),
    );
    return Container(
      height: 48,
      padding: const EdgeInsets.symmetric(horizontal: 4),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(24),
        border: Border.all(color: k.line),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          btn(Icons.remove_rounded, -1),
          ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 72),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                if (widget.label != null) Text(widget.label!, style: context.text.micro.copyWith(color: k.fg3)),
                Text(
                  widget.value.toStringAsFixed(widget.decimals),
                  textDirection: TextDirection.ltr,
                  style: context.text.mono(15, weight: FontWeight.w600),
                ),
              ],
            ),
          ),
          btn(Icons.add_rounded, 1),
        ],
      ),
    );
  }
}

/// The wizard steps bar (web Stepper: Details › Verify › Done).
class KStepIndicator extends StatelessWidget {
  const KStepIndicator({super.key, required this.steps, required this.current});
  final List<String> steps;
  final int current;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Row(
      children: [
        for (var i = 0; i < steps.length; i++) ...[
          if (i > 0)
            Expanded(
              child: Container(
                height: 2,
                margin: const EdgeInsets.symmetric(horizontal: 8),
                decoration: BoxDecoration(color: i <= current ? k.ember : k.line, borderRadius: BorderRadius.circular(1)),
              ),
            ),
          Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              AnimatedContainer(
                duration: const Duration(milliseconds: 250),
                width: 24,
                height: 24,
                alignment: Alignment.center,
                decoration: BoxDecoration(shape: BoxShape.circle, color: i <= current ? k.ember : k.surface3),
                child: i < current
                    ? Icon(Icons.check_rounded, size: 14, color: k.onEmber)
                    : Text(
                        '${i + 1}',
                        style: context.text.caption.copyWith(color: i <= current ? k.onEmber : k.fg3, fontWeight: FontWeight.w700, fontFeatures: kTabular),
                      ),
              ),
              const SizedBox(width: 6),
              Text(
                steps[i],
                style: context.text.caption.copyWith(fontSize: 12, color: i <= current ? k.fg : k.fg3, fontWeight: FontWeight.w600),
              ),
            ],
          ),
        ],
      ],
    );
  }
}

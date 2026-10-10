// Home's shortcut buttons (web components/dashboard/home/shortcuts.tsx, founder 2026-10-10: "instead of the extra
// cards, give shortcut buttons"): one glass tile per destination, the icon in an orange-tinted square and a short
// label, three to a row on phones. Modules the broker switched off are left out by the caller.
import 'package:flutter/material.dart';

import '../../../ui/ui.dart';

typedef Shortcut = ({String key, String label, IconData icon, VoidCallback onTap});

class HomeShortcuts extends StatelessWidget {
  const HomeShortcuts({super.key, required this.items});
  final List<Shortcut> items;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    const perRow = 3;
    const gap = 10.0;
    Widget tile(Shortcut s) => KPressable(
      key: ValueKey('shortcut-${s.key}'),
      onTap: s.onTap,
      semanticLabel: s.label,
      pressedScale: 0.97,
      child: Container(
        width: double.infinity,
        height: 116,
        padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 12),
        decoration: BoxDecoration(
          color: k.cardBg,
          borderRadius: BorderRadius.circular(22),
          border: Border.all(color: k.cardBorder),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          children: [
            Container(
              width: 44,
              height: 44,
              alignment: Alignment.center,
              decoration: BoxDecoration(color: k.emberSoft, borderRadius: BorderRadius.circular(14)),
              child: Icon(s.icon, size: 20, color: k.ember2),
            ),
            const SizedBox(height: 8),
            Text(
              s.label,
              textAlign: TextAlign.center,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: context.text.label.copyWith(fontSize: 12.5, fontWeight: FontWeight.w600, height: 1.2, color: k.fg),
            ),
          ],
        ),
      ),
    );
    return Column(
      children: [
        for (var i = 0; i < items.length; i += perRow) ...[
          if (i > 0) const SizedBox(height: gap),
          Row(
            children: [
              for (var j = i; j < i + perRow; j++) ...[
                if (j > i) const SizedBox(width: gap),
                Expanded(child: j < items.length ? tile(items[j]) : const SizedBox.shrink()),
              ],
            ],
          ),
        ],
      ],
    );
  }
}

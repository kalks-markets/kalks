// Two list blocks of the dashboard (web components/dashboard/home/list-cards.tsx) in the reference's row style:
// pastel icon tile, name, grey sub-text, a figure or a status pill, or a filled action — the "Getting started"
// checklist card and the tabbed activity list (History · Funding · Linked).
import 'package:flutter/material.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';

/// One row (web ListRowItem).
class ListRowItem {
  const ListRowItem({
    required this.key,
    required this.tone,
    required this.title,
    this.icon,
    this.leading,
    this.sub,
    this.value,
    this.status,
    this.action,
    this.done = false,
    this.href,
  });
  final String key;
  final IconData? icon;

  /// A custom leading picture instead of the icon tile (e.g. the coin of a network).
  final Widget? leading;
  final KTone tone;
  final String title;
  final String? sub;

  /// Amount or figure.
  final Widget? value;

  /// Status pill on the end (soft tint, like "Connected").
  final ({String label, KChipTone tone})? status;

  /// Filled action on the end (like "Continue" / "Open"); `external` opens the browser.
  final ({String label, String href, bool external})? action;
  final bool done;
  final String? href;
}

void _open(BuildContext context, String href, {bool external = false}) {
  if (external || href.startsWith('http')) {
    launchUrl(Uri.parse(href), mode: LaunchMode.externalApplication);
  } else if (href.startsWith('/trader')) {
    context.push(href);
  } else {
    context.go(href);
  }
}

class ItemRow extends StatelessWidget {
  const ItemRow({super.key, required this.r, this.compact = false});
  final ListRowItem r;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final tile = r.leading != null && !r.done
        ? SizedBox(
            width: compact ? 42 : 48,
            height: compact ? 42 : 48,
            child: Center(child: r.leading),
          )
        : KIconTile(
            icon: r.done ? LucideIcons.check : (r.icon ?? LucideIcons.circle),
            tone: r.done ? KTone.mint : r.tone,
            size: compact ? 42 : 48,
            iconSize: 20,
          );
    final body = Row(
      children: [
        tile,
        const SizedBox(width: 14),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                r.title,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: context.text.headline.copyWith(fontSize: 14.5, fontWeight: FontWeight.w700, color: r.done ? k.fg2 : k.fg),
              ),
              if (r.sub != null && r.sub!.isNotEmpty) ...[
                const SizedBox(height: 2),
                Text(
                  r.sub!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.footnote.copyWith(color: k.fg3),
                ),
              ],
            ],
          ),
        ),
        if (r.value != null) ...[
          const SizedBox(width: 10),
          DefaultTextStyle.merge(
            style: context.text.figure.copyWith(fontWeight: FontWeight.w700),
            child: r.value!,
          ),
        ],
      ],
    );
    final Widget? end = r.action != null
        ? ConstrainedBox(
            constraints: const BoxConstraints(minWidth: 88),
            child: KButton(
              label: r.action!.label,
              size: KButtonSize.sm,
              onPressed: () => _open(context, r.action!.href, external: r.action!.external),
            ),
          )
        : (r.status != null ? _StatusPill(label: r.status!.label, tone: r.status!.tone) : null);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Row(
        children: [
          Expanded(
            child: r.href != null && r.action == null ? KPressable(onTap: () => _open(context, r.href!), pressedScale: 1, child: body) : body,
          ),
          if (end != null) ...[const SizedBox(width: 10), end],
        ],
      ),
    );
  }
}

/// The status pill of a row (web Chip h-8 rounded-[11px]).
class _StatusPill extends StatelessWidget {
  const _StatusPill({required this.label, required this.tone});
  final String label;
  final KChipTone tone;

  @override
  Widget build(BuildContext context) {
    final (bg, fg, border) = context.k.chip(tone);
    return Container(
      height: 32,
      padding: const EdgeInsets.symmetric(horizontal: 12),
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(11),
        border: Border.all(color: border),
      ),
      child: Text(
        label,
        maxLines: 1,
        style: context.text.caption.copyWith(color: fg, fontSize: 12, fontWeight: FontWeight.w600),
      ),
    );
  }
}

/// "Getting started" (web ChecklistCard): title, subtitle, "n of m" progress, the steps.
class ChecklistCard extends StatelessWidget {
  const ChecklistCard({super.key, required this.title, this.subtitle, required this.rows, required this.done, required this.total});
  final String title;
  final String? subtitle;
  final List<ListRowItem> rows;
  final int done, total;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pct = (done / (total < 1 ? 1 : total) * 100).round();
    return KCard(
      padding: const EdgeInsets.fromLTRB(20, 20, 20, 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: context.text.title2.copyWith(fontSize: 18)),
                    if (subtitle != null) ...[const SizedBox(height: 2), Text(subtitle!, style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13))],
                  ],
                ),
              ),
              const SizedBox(width: 12),
              SizedBox(
                width: 132,
                child: Padding(
                  padding: const EdgeInsets.only(top: 4),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Row(
                        children: [
                          Expanded(
                            child: Text(
                              t('dashboard.steps.progress', {'done': done, 'total': total}),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w600),
                            ),
                          ),
                          Text(
                            '$pct%',
                            textDirection: TextDirection.ltr,
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w600, fontFeatures: kTabular),
                          ),
                        ],
                      ),
                      const SizedBox(height: 6),
                      KProgressBar(value: done / (total < 1 ? 1 : total)),
                    ],
                  ),
                ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          for (var i = 0; i < rows.length; i++) ...[if (i > 0) const KDivider(), ItemRow(r: rows[i])],
        ],
      ),
    );
  }
}

/// One tab of [ActivityTabs]; `rows` null while loading.
class ActivityTab {
  const ActivityTab({required this.key, required this.label, required this.rows, required this.empty, this.more});
  final String key;
  final String label;
  final List<ListRowItem>? rows;
  final String empty;
  final ({String label, String href})? more;
}

/// History · Funding · Linked (web ActivityTabs): text tabs over a list, "View all" under it.
class ActivityTabs extends StatefulWidget {
  const ActivityTabs({super.key, required this.tabs});
  final List<ActivityTab> tabs;

  @override
  State<ActivityTabs> createState() => _ActivityTabsState();
}

class _ActivityTabsState extends State<ActivityTabs> {
  int _i = 0;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    if (widget.tabs.isEmpty) return const SizedBox.shrink();
    // the tabs can shrink under the open one (a module switched off)
    final at = _i.clamp(0, widget.tabs.length - 1);
    final cur = widget.tabs[at];
    final rows = cur.rows;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Transform.translate(
          offset: const Offset(-8, 0),
          child: KSubNav(labels: [for (final x in widget.tabs) x.label], current: at, onSelect: (i) => setState(() => _i = i)),
        ),
        ConstrainedBox(
          constraints: const BoxConstraints(minHeight: 200),
          child: rows == null
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 12),
                  child: Column(
                    children: [
                      for (var i = 0; i < 3; i++)
                        const Padding(
                          padding: EdgeInsets.symmetric(vertical: 6),
                          child: Row(
                            children: [
                              KSkeleton(width: 44, height: 44, radius: 14),
                              SizedBox(width: 14),
                              Expanded(child: KSkeleton(height: 12)),
                            ],
                          ),
                        ),
                    ],
                  ),
                )
              : rows.isEmpty
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 40),
                  child: Text(
                    cur.empty,
                    textAlign: TextAlign.center,
                    style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13),
                  ),
                )
              : Column(
                  children: [
                    for (var i = 0; i < rows.length; i++) ...[if (i > 0) const KDivider(), ItemRow(r: rows[i], compact: true)],
                  ],
                ),
        ),
        if (cur.more != null)
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: KTextButton(label: cur.more!.label, onPressed: () => context.go(cur.more!.href)),
          ),
      ],
    );
  }
}

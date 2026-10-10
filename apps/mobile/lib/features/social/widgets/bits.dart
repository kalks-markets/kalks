// Small pieces shared by the Copy & PAMM pages: the port of apps/crm/components/social-live/bits.tsx (RiskBadge,
// HouseBadge, ProgramTags, MasterIdentity, InfoBox, Tile, SocialError, BlockSkeleton) and components/social/controls.tsx
// (RangeSlider, RadioCard, ToggleChip), plus the row lists that stand in for the web's DataTables on phones.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../social_api.dart';

/* ------------------------------------------------------------------ feedback */

void okToast(WidgetRef ref, String title, [String? description]) {
  KHaptics.success();
  ref.read(notificationsProvider.notifier).toast(NotificationKind.success, title, description: description);
}

void errToast(WidgetRef ref, BuildContext context, String title, Object e) {
  ref.read(notificationsProvider.notifier).toast(NotificationKind.error, title, description: socialError(e, context.t));
}

void warnToast(WidgetRef ref, String title, [String? description]) =>
    ref.read(notificationsProvider.notifier).toast(NotificationKind.warning, title, description: description);

/// A plain error banner (the web's toast.error(text) without a description).
void plainError(WidgetRef ref, String text) => ref.read(notificationsProvider.notifier).toast(NotificationKind.error, text, keep: false);

/* ------------------------------------------------------------------ badges */

/// System risk score 1–10 (D72) as a compact badge with a 10-tick meter.
class RiskBadge extends StatelessWidget {
  const RiskBadge({super.key, required this.risk, this.showLabel = false});
  final num risk;
  final bool showLabel;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final r = (risk == 0 ? 1 : risk).round().clamp(1, 10);
    final (bg, fg, border) = context.k.chip(riskTone(r));
    return Semantics(
      label: t('social.riskBadge.tooltip', {'r': r, 'level': riskLabel(t, r)}),
      child: Container(
        height: 24,
        padding: const EdgeInsets.symmetric(horizontal: 8),
        decoration: BoxDecoration(
          color: bg,
          borderRadius: BorderRadius.circular(12),
          border: Border.all(color: border),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              mainAxisSize: MainAxisSize.min,
              children: [
                for (var i = 0; i < 10; i++)
                  Container(
                    width: 2,
                    height: 4 + i * 0.7,
                    margin: const EdgeInsetsDirectional.only(end: 1.5),
                    decoration: BoxDecoration(color: i < r ? fg : context.k.fg3.withValues(alpha: 0.3), borderRadius: BorderRadius.circular(1)),
                  ),
              ],
            ),
            const SizedBox(width: 4),
            Text(
              '$r',
              style: context.text.caption.copyWith(color: fg, fontWeight: FontWeight.w700, fontFeatures: kTabular),
            ),
            if (showLabel) ...[const SizedBox(width: 5), Text(riskLabel(t, r), style: context.text.caption.copyWith(color: fg.withValues(alpha: 0.85)))],
          ],
        ),
      ),
    );
  }
}

/// Disclosure label of a house account (a broker-operated account running an automated strategy).
class HouseBadge extends StatelessWidget {
  const HouseBadge({super.key, this.small = true});
  final bool small;

  @override
  Widget build(BuildContext context) => Semantics(
    label: context.t('social.house.disclosure'),
    child: KChip(label: context.t('social.house.badge'), tone: KChipTone.info, icon: LucideIcons.building2, small: small),
  );
}

/// Copy / PAMM chips of a master's programme.
class ProgramTags extends StatelessWidget {
  const ProgramTags({super.key, required this.program, this.small = true});
  final String program;
  final bool small;

  @override
  Widget build(BuildContext context) => Wrap(
    spacing: 4,
    runSpacing: 4,
    children: [
      if (program == 'copy' || program == 'both') KChip(label: context.t('social.program.copy'), tone: KChipTone.ember, icon: LucideIcons.copy, small: small),
      if (program == 'pamm' || program == 'both') KChip(label: 'PAMM', tone: KChipTone.gold, icon: LucideIcons.landmark, small: small),
    ],
  );
}

/// A status chip (web StatusChip): active / paused / stopped / pending / approved / rejected / suspended / revoked.
class StatusChip extends StatelessWidget {
  const StatusChip({super.key, required this.status, required this.label});
  final String status;
  final String label;

  @override
  Widget build(BuildContext context) {
    final tone = switch (status) {
      'active' || 'approved' || 'done' || 'paid' => KChipTone.up,
      'paused' || 'pending' || 'frozen' => KChipTone.warn,
      'rejected' || 'suspended' || 'failed' => KChipTone.down,
      _ => KChipTone.neutral,
    };
    return KChip(label: label, tone: tone, dot: status == 'active', small: true);
  }
}

/* ------------------------------------------------------------------ identity */

/// Masters are shown by nickname only: an initials avatar, never a photo or personal name.
class MasterIdentity extends StatelessWidget {
  const MasterIdentity({super.key, required this.nickname, this.sub, this.subText, this.size = 40});
  final String nickname;
  final Widget? sub;
  final String? subText;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        KAvatar(name: nickname.isEmpty ? context.t('social.master') : nickname, size: size),
        const SizedBox(width: 12),
        Flexible(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                nickname,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: context.text.headline.copyWith(fontSize: 14, fontWeight: FontWeight.w600),
              ),
              if (sub != null)
                Padding(padding: const EdgeInsets.only(top: 2), child: sub)
              else if (subText != null && subText!.isNotEmpty)
                Text(
                  subText!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                ),
            ],
          ),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ boxes */

/// A soft callout (web InfoBox): neutral grey, gold, warn, down or up tint, with an icon and any content.
class InfoBox extends StatelessWidget {
  const InfoBox({super.key, this.text, this.child, this.tone = KChipTone.neutral, this.icon});
  final String? text;
  final Widget? child;
  final KChipTone tone;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final neutral = tone == KChipTone.neutral;
    final (bg, fg, border) = neutral ? (k.surface2, k.fg3, k.line) : k.chip(tone);
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 11, 14, 11),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: border),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 1),
            child: Icon(icon ?? LucideIcons.info, size: 16, color: fg),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: DefaultTextStyle.merge(
              style: context.text.footnote.copyWith(color: neutral ? k.fg3 : k.fg2, height: 1.5),
              child: child ?? Text(text ?? ''),
            ),
          ),
        ],
      ),
    );
  }
}

/// A small label / value tile (k-row).
class Tile extends StatelessWidget {
  const Tile({super.key, required this.label, required this.value, this.sub});
  final String label;
  final Widget value;
  final String? sub;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
          ),
          const SizedBox(height: 2),
          DefaultTextStyle.merge(
            style: context.text.figure.copyWith(fontSize: 14, fontWeight: FontWeight.w600),
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            child: value,
          ),
          if (sub != null)
            Text(
              sub!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
        ],
      ),
    );
  }
}

/// Tiles in a grid (2 columns on phones by default).
class TileGrid extends StatelessWidget {
  const TileGrid({super.key, required this.tiles, this.columns = 2, this.gap = 8});
  final List<Widget> tiles;
  final int columns;
  final double gap;

  @override
  Widget build(BuildContext context) {
    final rows = <Widget>[];
    for (var i = 0; i < tiles.length; i += columns) {
      if (i > 0) rows.add(SizedBox(height: gap));
      rows.add(
        IntrinsicHeight(
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (var j = 0; j < columns; j++) ...[
                if (j > 0) SizedBox(width: gap),
                Expanded(child: i + j < tiles.length ? tiles[i + j] : const SizedBox.shrink()),
              ],
            ],
          ),
        ),
      );
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, mainAxisSize: MainAxisSize.min, children: rows);
  }
}

/// A figure text: Latin digits, left to right, optionally toned.
class Num extends StatelessWidget {
  const Num(this.text, {super.key, this.color, this.style});
  final String text;
  final Color? color;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) => Text(
    text,
    textDirection: TextDirection.ltr,
    maxLines: 1,
    overflow: TextOverflow.ellipsis,
    style: (style ?? const TextStyle()).copyWith(color: color, fontFeatures: kTabular),
  );
}

/// A percent coloured by sign (web Pct).
class PctText extends StatelessWidget {
  const PctText(this.value, {super.key, this.decimals = 2, this.style});
  final num? value;
  final int decimals;
  final TextStyle? style;

  @override
  Widget build(BuildContext context) => Num(pct(value, decimals), color: toneColor(context, value), style: style);
}

/// The web's "couldn't load" card of the social pages (SocialError).
class SocialErrorCard extends StatelessWidget {
  const SocialErrorCard({super.key, required this.error, required this.onRetry, this.title, this.card = true});
  final Object? error;
  final VoidCallback onRetry;
  final String? title;
  final bool card;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final body = KEmptyState(
      compact: true,
      art: KIllustrationName.connectionLost,
      title: title ?? t('social.errorState.title'),
      text: error == null ? t('social.errorState.text') : socialError(error, t),
      action: KButton(label: t('common.retry'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onRetry),
    );
    return card ? KCard(child: body) : body;
  }
}

/// Rounded loading blocks (BlockSkeleton).
class BlockSkeleton extends StatelessWidget {
  const BlockSkeleton({super.key, this.n = 3, this.h = 120});
  final int n;
  final double h;

  @override
  Widget build(BuildContext context) => Column(
    mainAxisSize: MainAxisSize.min,
    children: [
      for (var i = 0; i < n; i++) ...[if (i > 0) const SizedBox(height: 12), KSkeleton(height: h, radius: 18)],
    ],
  );
}

/// The centred grey note of an empty table or list (k-row px-4 py-8 text-center).
class EmptyRow extends StatelessWidget {
  const EmptyRow(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 26),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Text(
        text,
        textAlign: TextAlign.center,
        style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13),
      ),
    );
  }
}

/// The small medium-weight label above a group of controls.
class GroupLabel extends StatelessWidget {
  const GroupLabel(this.text, {super.key, this.trailing});
  final String text;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Row(
      children: [
        Expanded(
          child: Text(text, style: context.text.label.copyWith(color: context.k.fg2, fontSize: 12.5)),
        ),
        ?trailing,
      ],
    ),
  );
}

/// A small grey note.
class Hint extends StatelessWidget {
  const Hint(this.text, {super.key, this.top = 0});
  final String text;
  final double top;

  @override
  Widget build(BuildContext context) => Padding(
    padding: EdgeInsets.only(top: top),
    child: Text(text, style: context.text.footnote.copyWith(color: context.k.fg3, fontSize: 12)),
  );
}

/* ------------------------------------------------------------------ rows (DataTables on phones) */

/// A list of rows with hairlines inside a rounded box (a table on the web); shows `pageSize` rows and Show more.
class RowsBox extends StatefulWidget {
  const RowsBox({super.key, required this.children, this.pageSize = 10, this.boxed = true});
  final List<Widget> children;
  final int pageSize;
  final bool boxed;

  @override
  State<RowsBox> createState() => _RowsBoxState();
}

class _RowsBoxState extends State<RowsBox> {
  late int _shown = widget.pageSize;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final rows = widget.children.take(_shown).toList();
    final list = Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        for (var i = 0; i < rows.length; i++) ...[if (i > 0) const KDivider(), rows[i]],
      ],
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (widget.boxed)
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            decoration: BoxDecoration(
              color: k.surface2,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: k.line),
            ),
            child: list,
          )
        else
          list,
        if (widget.children.length > _shown)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Center(
              child: KTextButton(label: context.t('common.showMore'), onPressed: () => setState(() => _shown += widget.pageSize)),
            ),
          ),
      ],
    );
  }
}

/// One row of a [RowsBox]: a title and subtitle at the start, values at the end.
class DataLine extends StatelessWidget {
  const DataLine({super.key, required this.title, this.subtitle, this.trailing, this.trailingSub, this.leading, this.onTap});
  final Widget title;
  final Widget? subtitle;
  final Widget? trailing;
  final Widget? trailingSub;
  final Widget? leading;
  final VoidCallback? onTap;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final row = Padding(
      padding: const EdgeInsets.symmetric(vertical: 10),
      child: Row(
        children: [
          if (leading != null) ...[leading!, const SizedBox(width: 10)],
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                DefaultTextStyle.merge(
                  style: context.text.label.copyWith(color: k.fg, fontSize: 13.5),
                  child: title,
                ),
                if (subtitle != null) ...[
                  const SizedBox(height: 2),
                  DefaultTextStyle.merge(
                    style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                    child: subtitle!,
                  ),
                ],
              ],
            ),
          ),
          if (trailing != null || trailingSub != null) ...[
            const SizedBox(width: 10),
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              mainAxisSize: MainAxisSize.min,
              children: [
                if (trailing != null) DefaultTextStyle.merge(style: context.text.figure.copyWith(fontSize: 13.5), child: trailing!),
                if (trailingSub != null) ...[
                  const SizedBox(height: 3),
                  DefaultTextStyle.merge(
                    style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                    child: trailingSub!,
                  ),
                ],
              ],
            ),
          ],
          if (onTap != null) ...[
            const SizedBox(width: 6),
            Icon(Directionality.of(context) == TextDirection.rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, size: 16, color: k.fg3),
          ],
        ],
      ),
    );
    return onTap == null ? row : KPressable(onTap: onTap, pressedScale: 1, child: row);
  }
}

/* ------------------------------------------------------------------ controls */

/// Selectable card for radio-style choices (sizing mode, programme, stop options).
class RadioCard extends StatelessWidget {
  const RadioCard({super.key, required this.selected, required this.onSelect, required this.title, this.text, this.icon, this.dim = false});
  final bool selected;
  final VoidCallback? onSelect;
  final String title;
  final String? text;
  final IconData? icon;
  final bool dim;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Opacity(
      opacity: dim ? 0.5 : 1,
      child: KPressable(
        onTap: onSelect,
        pressedScale: 0.985,
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 160),
          padding: const EdgeInsets.all(12),
          decoration: BoxDecoration(
            color: selected ? k.emberSoft : k.surface2,
            borderRadius: BorderRadius.circular(16),
            border: Border.all(color: selected ? k.ember.withValues(alpha: 0.5) : k.line),
          ),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              if (icon != null) ...[
                Container(
                  width: 34,
                  height: 34,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: selected ? k.ember.withValues(alpha: 0.15) : k.surface3,
                    border: Border.all(color: selected ? k.ember.withValues(alpha: 0.4) : k.line),
                  ),
                  child: Icon(icon, size: 16, color: selected ? k.ember : k.fg2),
                ),
                const SizedBox(width: 12),
              ],
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(title, style: context.text.headline.copyWith(fontSize: 14, fontWeight: FontWeight.w600)),
                    if (text != null && text!.isNotEmpty) ...[
                      const SizedBox(height: 2),
                      Text(text!, style: context.text.footnote.copyWith(color: k.fg3, height: 1.35)),
                    ],
                  ],
                ),
              ),
              const SizedBox(width: 10),
              Container(
                width: 18,
                height: 18,
                margin: const EdgeInsets.only(top: 2),
                decoration: BoxDecoration(
                  shape: BoxShape.circle,
                  border: Border.all(color: selected ? k.ember : k.fg3.withValues(alpha: 0.6), width: 1.5),
                ),
                child: selected
                    ? Center(
                        child: Container(
                          width: 9,
                          height: 9,
                          decoration: BoxDecoration(shape: BoxShape.circle, color: k.ember),
                        ),
                      )
                    : null,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A toggleable pill for multi-select (excluded symbols, quick amounts).
class ToggleChip extends StatelessWidget {
  const ToggleChip({super.key, required this.label, required this.on, required this.onTap, this.down = false, this.removable = false});
  final String label;
  final bool on;
  final VoidCallback onTap;
  final bool down;
  final bool removable;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final c = down ? k.down : k.ember;
    final fg = on ? c : k.fg2;
    return KPressable(
      onTap: onTap,
      minSize: 40,
      pressedScale: 0.96,
      child: Container(
        height: 32,
        padding: const EdgeInsets.symmetric(horizontal: 12),
        decoration: BoxDecoration(
          color: on ? (down ? k.downSoft : k.emberSoft) : k.surface2,
          borderRadius: BorderRadius.circular(16),
          border: Border.all(color: on ? c.withValues(alpha: 0.4) : k.line),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Text(
              label,
              textDirection: TextDirection.ltr,
              style: context.text.label.copyWith(fontSize: 12.5, fontWeight: FontWeight.w600, color: fg, fontFeatures: kTabular),
            ),
            if (removable) ...[const SizedBox(width: 5), Icon(LucideIcons.x, size: 12, color: fg)],
          ],
        ),
      ),
    );
  }
}

/// The web RangeSlider (SocialSlider): the iOS slider plus tappable tick labels under it.
class SocialSlider extends StatelessWidget {
  const SocialSlider({
    super.key,
    required this.value,
    required this.onChanged,
    this.min = 0,
    this.max = 100,
    this.ticks = const [],
    this.format,
    this.enabled = true,
  });
  final double value;
  final ValueChanged<double> onChanged;
  final double min, max;
  final List<double> ticks;
  final String Function(double v)? format;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final divisions = (max - min).round();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        KSlider(value: value, min: min, max: max, divisions: divisions > 0 ? divisions : null, onChanged: enabled ? (v) => onChanged(v.roundToDouble()) : null),
        if (ticks.isNotEmpty)
          Directionality(
            textDirection: TextDirection.ltr,
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 6),
              child: SizedBox(
                height: 22,
                child: LayoutBuilder(
                  builder: (context, c) => Stack(
                    clipBehavior: Clip.none,
                    children: [
                      for (var i = 0; i < ticks.length; i++)
                        Positioned(
                          left: ((ticks[i] - min) / (max - min == 0 ? 1 : max - min)) * c.maxWidth - (i == 0 ? 0 : (i == ticks.length - 1 ? 28 : 14)),
                          top: 0,
                          child: GestureDetector(
                            behavior: HitTestBehavior.opaque,
                            onTap: enabled ? () => onChanged(ticks[i]) : null,
                            child: SizedBox(
                              width: 28,
                              child: Text(
                                format?.call(ticks[i]) ?? numText(ticks[i]),
                                textAlign: i == 0 ? TextAlign.left : (i == ticks.length - 1 ? TextAlign.right : TextAlign.center),
                                style: context.text.micro.copyWith(
                                  color: ticks[i] == value ? k.fg : k.fg3,
                                  fontWeight: FontWeight.w500,
                                  fontFeatures: kTabular,
                                ),
                              ),
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
              ),
            ),
          ),
      ],
    );
  }
}

/// A surface-2 box with a title, hint and switch; its content is dimmed while the switch is off (the drawdown stop,
/// the PAMM stop loss).
class SwitchBox extends StatelessWidget {
  const SwitchBox({super.key, required this.title, required this.hint, required this.on, required this.onChanged, this.child});
  final String title;
  final String hint;
  final bool on;
  final ValueChanged<bool> onChanged;
  final Widget? child;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      title,
                      style: context.text.label.copyWith(fontSize: 13.5, fontWeight: FontWeight.w600, color: k.fg),
                    ),
                    if (hint.isNotEmpty) Text(hint, style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)),
                  ],
                ),
              ),
              const SizedBox(width: 10),
              KSwitch(value: on, onChanged: onChanged, semanticLabel: title),
            ],
          ),
          if (child != null) ...[
            const SizedBox(height: 10),
            IgnorePointer(
              ignoring: !on,
              child: Opacity(opacity: on ? 1 : 0.4, child: child),
            ),
          ],
        ],
      ),
    );
  }
}

/// "Trigger … -30%" line above a slider.
class TriggerLine extends StatelessWidget {
  const TriggerLine({super.key, required this.value});
  final String value;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Expanded(
        child: Text(context.t('social.follow.trigger'), style: context.text.footnote.copyWith(color: context.k.fg3)),
      ),
      Num(
        value,
        color: context.k.down,
        style: context.text.footnote.copyWith(fontWeight: FontWeight.w600),
      ),
    ],
  );
}

/// A unit or currency label at the end of an amount field.
class FieldUnit extends StatelessWidget {
  const FieldUnit(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsetsDirectional.only(end: 13),
    child: Text(text, style: context.text.callout.copyWith(color: context.k.fg3)),
  );
}

/// An amount / number field (web Input type=number with $ and a unit).
class NumberField extends StatelessWidget {
  const NumberField({
    super.key,
    required this.controller,
    this.label,
    this.hint,
    this.error,
    this.dollar = false,
    this.unit,
    this.placeholder,
    this.onChanged,
    this.leading,
    this.autofocus = false,
    this.integer = false,
  });
  final TextEditingController controller;
  final String? label;
  final String? hint;
  final String? error;
  final bool dollar;
  final String? unit;
  final String? placeholder;
  final ValueChanged<String>? onChanged;
  final IconData? leading;
  final bool autofocus;
  final bool integer;

  @override
  Widget build(BuildContext context) => KTextField(
    controller: controller,
    label: label,
    hint: hint == null
        ? null
        : Text(
            hint!,
            style: context.text.caption.copyWith(color: context.k.fg3, fontWeight: FontWeight.w400),
          ),
    error: error,
    leadingText: dollar ? r'$' : null,
    leading: leading,
    trailing: unit == null ? null : FieldUnit(unit!),
    placeholder: placeholder,
    keyboardType: TextInputType.numberWithOptions(decimal: !integer),
    ltr: true,
    onChanged: onChanged,
    autofocus: autofocus,
  );
}

/// A multi-line text input (descriptions, announcements) in the KTextField look.
class TextArea extends StatelessWidget {
  const TextArea({super.key, required this.controller, this.label, this.counter, this.placeholder, this.maxLength, this.lines = 4, this.onChanged});
  final TextEditingController controller;
  final String? label;
  final String? counter;
  final String? placeholder;
  final int? maxLength;
  final int lines;
  final ValueChanged<String>? onChanged;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      mainAxisSize: MainAxisSize.min,
      children: [
        if (label != null)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: Row(
              children: [
                Expanded(
                  child: Text(label!, style: context.text.label.copyWith(color: k.fg2)),
                ),
                if (counter != null)
                  Num(
                    counter!,
                    color: k.fg3,
                    style: context.text.caption.copyWith(fontWeight: FontWeight.w400),
                  ),
              ],
            ),
          ),
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 13, vertical: 10),
          decoration: BoxDecoration(
            color: k.surface2,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: k.line),
          ),
          child: TextField(
            controller: controller,
            minLines: lines,
            maxLines: lines + 4,
            maxLength: maxLength,
            onChanged: onChanged,
            style: context.text.body.copyWith(fontSize: 14),
            cursorColor: k.ember,
            decoration: InputDecoration(
              isCollapsed: true,
              border: InputBorder.none,
              counterText: '',
              hintText: placeholder,
              hintStyle: context.text.body.copyWith(fontSize: 14, color: k.fg3),
            ),
          ),
        ),
      ],
    );
  }
}

/// Opens Kalks Trader on a login (web TradeButton).
class TraderButton extends StatelessWidget {
  const TraderButton({
    super.key,
    required this.login,
    this.label,
    this.size = KButtonSize.sm,
    this.expand = false,
    this.variant = KButtonVariant.surface,
    this.blocked = false,
  });
  final int? login;
  final String? label;
  final KButtonSize size;
  final bool expand;
  final KButtonVariant variant;
  final bool blocked;

  @override
  Widget build(BuildContext context) => KButton(
    label: label ?? context.t('accounts.row.trade'),
    icon: LucideIcons.candlestickChart,
    size: size,
    expand: expand,
    variant: variant,
    onPressed: login == null || blocked ? null : () => context.push('/trader?login=$login'),
  );
}

/// A card section: title row (CardHeader) and its content with the web's spacing.
class SectionCard extends StatelessWidget {
  const SectionCard({super.key, required this.title, this.subtitle, this.icon, this.action, required this.child});
  final String title;
  final String? subtitle;
  final IconData? icon;
  final Widget? action;
  final Widget child;

  @override
  Widget build(BuildContext context) => KCard(
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KCardHeader(title: title, subtitle: subtitle, icon: icon, action: action),
        const SizedBox(height: 14),
        child,
      ],
    ),
  );
}

/// The sheet's subtitle line (the web Dialog description).
class SheetLead extends StatelessWidget {
  const SheetLead(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 14),
    child: Text(
      text,
      textAlign: TextAlign.center,
      style: context.text.footnote.copyWith(color: context.k.fg3),
    ),
  );
}

/// The web's chart palette (packages/ui CHART_COLORS).
const List<Color> kChartColors = [
  kEmber,
  Color(0xFFE9B949),
  Color(0xFF22C55E),
  Color(0xFF38BDF8),
  Color(0xFFF04438),
  Color(0xFFA1A1AA),
  Color(0xFF14B8A6),
  Color(0xFFFF8A3D),
];

/// A donut legend row: colour dot, label, share.
class LegendRow extends StatelessWidget {
  const LegendRow({super.key, required this.color, required this.label, required this.value});
  final Color color;
  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: k.line),
      ),
      child: Row(
        children: [
          Container(
            width: 10,
            height: 10,
            decoration: BoxDecoration(color: color, shape: BoxShape.circle),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              label,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.label.copyWith(color: k.fg),
            ),
          ),
          Num(value, color: k.fg2, style: context.text.footnote),
        ],
      ),
    );
  }
}

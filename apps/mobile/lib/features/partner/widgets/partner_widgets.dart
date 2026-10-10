// The partner pages' shared pieces (web components/partner/live/ui.tsx, qr.tsx, share-buttons.tsx): load states,
// chips, the person cell, mini stats, quiet card empties, the QR code with SVG / PNG export, share buttons, capsule
// bars, the pager and the CSV export of the web's DataTable.
import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:qr_flutter/qr_flutter.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/files.dart';
import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../../markets/instruments.dart' show SymbolAvatar, kInstrumentMap;
import '../partner_api.dart';

/* ------------------------------------------------------------------ load states */

/// The partner profile is created from the sign-up a few seconds after registration (409 not_ready): retries quietly
/// a few times; the button stays for impatient users.
class PartnerSettingUp extends StatefulWidget {
  const PartnerSettingUp({super.key, required this.onRetry});
  final VoidCallback onRetry;

  @override
  State<PartnerSettingUp> createState() => _PartnerSettingUpState();
}

class _PartnerSettingUpState extends State<PartnerSettingUp> {
  Timer? _timer;
  int _n = 0;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 5), (t) {
      _n += 1;
      if (_n > 6) {
        t.cancel();
      } else {
        widget.onRetry();
      }
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return KCard(
      child: KEmptyState(
        art: KIllustrationName.partnerIb,
        title: t('partner.load.settingUpTitle'),
        text: t('partner.load.settingUpText'),
        action: KButton(
          label: t('partner.load.checkAgain'),
          icon: LucideIcons.rotateCw,
          variant: KButtonVariant.surface,
          size: KButtonSize.sm,
          onPressed: widget.onRetry,
        ),
      ),
    );
  }
}

/// The not-ready / error state of a failed load (web LoadProblem).
class PartnerLoadProblem extends StatelessWidget {
  const PartnerLoadProblem({super.key, required this.error, required this.onRetry});
  final Object error;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final e = error;
    if (e is ApiException && e.status == 409 && e.code == 'not_ready') return PartnerSettingUp(onRetry: onRetry);
    return KCard(
      child: KEmptyState(
        compact: true,
        art: KIllustrationName.connectionLost,
        title: t('partner.load.errorTitle'),
        text: e is ApiException ? partnerError(e, t) : t('partner.load.errorText'),
        action: KButton(label: t('common.retry'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onRetry),
      ),
    );
  }
}

/// A page while its first load is in flight or failed (web PageFallback): the header, then the skeleton or the problem.
class PartnerPageFallback extends StatelessWidget {
  const PartnerPageFallback({
    super.key,
    required this.title,
    required this.subtitle,
    this.error,
    required this.onRetry,
    this.skeleton = const [180, 150, 260],
    this.hero,
  });
  final String title, subtitle;
  final Object? error;
  final VoidCallback onRetry;

  /// The page's photo hero (the section's first page): the title is on the photo, not in the page.
  final KPageHero? hero;

  /// Heights of the skeleton cards.
  final List<double> skeleton;

  @override
  Widget build(BuildContext context) => KPageScroll(
    onRefresh: () async => onRetry(),
    hero: hero,
    children: [
      if (hero == null) ...[KPageHeader(title: title, subtitle: Text(subtitle)), const SizedBox(height: 20)] else const SizedBox(height: 6),
      if (error != null)
        PartnerLoadProblem(error: error!, onRetry: onRetry)
      else
        for (final h in skeleton) ...[KSkeletonCard(height: h), const SizedBox(height: 14)],
    ],
  );
}

/* ------------------------------------------------------------------ chips */

class TierChip extends StatelessWidget {
  const TierChip(this.tier, {super.key});
  final int tier;

  @override
  Widget build(BuildContext context) => KChip(
    label: 'L$tier',
    small: true,
    tone: tier == 1
        ? KChipTone.ember
        : tier == 2
        ? KChipTone.gold
        : KChipTone.neutral,
  );
}

class CommissionStatusChip extends StatelessWidget {
  const CommissionStatusChip(this.status, {super.key});
  final String status;

  static const _m = {
    'pending': ('Pending', KChipTone.warn),
    'approved': ('Approved', KChipTone.info),
    'paid': ('Paid', KChipTone.up),
    'rejected': ('Rejected', KChipTone.down),
    'void': ('Void', KChipTone.neutral),
  };

  @override
  Widget build(BuildContext context) {
    final m = _m[status] ?? (status, KChipTone.neutral);
    return KChip(
      label: context.t.dyn('partner.commissionStatus.$status', fallback: m.$1),
      tone: m.$2,
      dot: true,
      small: true,
    );
  }
}

class PayoutStatusChip extends StatelessWidget {
  const PayoutStatusChip(this.status, {super.key});
  final String status;

  static const _m = {
    'awaiting_approval': ('Awaiting approval', KChipTone.warn),
    'processing': ('Processing', KChipTone.info),
    'paid': ('Paid', KChipTone.up),
    'rejected': ('Rejected', KChipTone.down),
  };

  @override
  Widget build(BuildContext context) {
    final m = _m[status] ?? (status.replaceAll('_', ' '), KChipTone.neutral);
    return KChip(
      label: context.t.dyn('partner.payoutStatus.$status', fallback: m.$1),
      tone: m.$2,
      dot: true,
      small: true,
    );
  }
}

class ClientStatusChip extends StatelessWidget {
  const ClientStatusChip(this.status, {super.key});
  final String status;

  static const _m = {'active': ('Active', KChipTone.up), 'funded': ('Funded', KChipTone.info), 'registered': ('Registered', KChipTone.neutral)};

  @override
  Widget build(BuildContext context) {
    final m = _m[status] ?? (status, KChipTone.neutral);
    return KChip(
      label: context.t.dyn('partner.clientStatus.$status', fallback: m.$1),
      tone: m.$2,
      small: true,
    );
  }
}

/* ------------------------------------------------------------------ cells */

/// Initials only (the partner service shares no photos): letters and spaces of the name.
String partnerInitials(String name) {
  final s = name.replaceAll(RegExp(r'[^\p{L}\s]', unicode: true), '').trim();
  return s.isEmpty ? '?' : s;
}

/// Initials avatar with the country flag at its corner.
class PartnerAvatar extends StatelessWidget {
  const PartnerAvatar({super.key, required this.name, this.country, this.size = 32});
  final String name;
  final String? country;
  final double size;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: size + 3,
    height: size + 2,
    child: Stack(
      clipBehavior: Clip.none,
      children: [
        KAvatar(name: partnerInitials(name), size: size),
        if (country != null && country!.isNotEmpty)
          PositionedDirectional(
            end: 0,
            bottom: 0,
            child: Container(
              padding: const EdgeInsets.all(1.5),
              decoration: BoxDecoration(color: context.k.surface, shape: BoxShape.circle),
              child: KFlag(country!, size: 13),
            ),
          ),
      ],
    ),
  );
}

/// Avatar + name + an optional second line (web PersonCell).
class PersonCell extends StatelessWidget {
  const PersonCell({super.key, required this.name, this.country, this.sub, this.size = 32, this.trailing});
  final String name;
  final String? country;
  final String? sub;
  final double size;

  /// Chips after the name.
  final List<Widget>? trailing;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Row(
      children: [
        PartnerAvatar(name: name, country: country, size: size),
        const SizedBox(width: 11),
        Expanded(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Flexible(
                    child: Text(
                      name,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.label.copyWith(fontSize: 13.5, color: k.fg),
                    ),
                  ),
                  for (final w in trailing ?? const <Widget>[]) ...[const SizedBox(width: 6), w],
                ],
              ),
              if (sub != null && sub!.isNotEmpty) ...[
                const SizedBox(height: 2),
                Text(
                  sub!,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}

/// A small figure card (web MiniStat): label, value, a grey note.
class MiniStat extends StatelessWidget {
  const MiniStat({super.key, required this.label, required this.value, this.sub});
  final String label;
  final Widget value;
  final String? sub;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KCard(
      padding: const EdgeInsets.fromLTRB(16, 14, 16, 14),
      radius: 20,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.caption.copyWith(color: k.fg3),
          ),
          const SizedBox(height: 8),
          DefaultTextStyle.merge(style: context.text.figure.copyWith(fontSize: 21, height: 1.05), maxLines: 1, overflow: TextOverflow.ellipsis, child: value),
          if (sub != null) ...[
            const SizedBox(height: 5),
            Text(
              sub!,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
          ],
        ],
      ),
    );
  }
}

/// Quiet inline empty state in a card body (web CardEmpty): dashed border; `art` only where nothing exists yet.
class CardEmpty extends StatelessWidget {
  const CardEmpty({super.key, required this.title, this.text, this.art, this.child, this.height});
  final String title;
  final String? text;
  final KIllustrationName? art;
  final Widget? child;
  final double? height;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return CustomPaint(
      painter: _DashedBorder(k.line, 16),
      child: Container(
        constraints: BoxConstraints(minHeight: height ?? 0),
        width: double.infinity,
        padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 26),
        alignment: Alignment.center,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (art != null) ...[KIllustration(art!, width: 160, maxHeight: 120), const SizedBox(height: 14)],
            Text(
              title,
              textAlign: TextAlign.center,
              style: context.text.label.copyWith(fontSize: 13.5, color: k.fg2),
            ),
            if (text != null) ...[
              const SizedBox(height: 4),
              ConstrainedBox(
                constraints: const BoxConstraints(maxWidth: 300),
                child: Text(
                  text!,
                  textAlign: TextAlign.center,
                  style: context.text.footnote.copyWith(color: k.fg3),
                ),
              ),
            ],
            if (child != null) ...[const SizedBox(height: 14), child!],
          ],
        ),
      ),
    );
  }
}

class _DashedBorder extends CustomPainter {
  _DashedBorder(this.color, this.radius);
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final path = Path()..addRRect(RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius)));
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1;
    for (final m in path.computeMetrics()) {
      for (double d = 0; d < m.length; d += 9) {
        canvas.drawPath(m.extractPath(d, math.min(d + 5, m.length)), paint);
      }
    }
  }

  @override
  bool shouldRepaint(_DashedBorder old) => old.color != color;
}

/// A soft rounded row (web .k-row).
class PartnerRow extends StatelessWidget {
  const PartnerRow({super.key, required this.child, this.padding = const EdgeInsets.symmetric(horizontal: 14, vertical: 10), this.onTap, this.color});
  final Widget child;
  final EdgeInsetsGeometry padding;
  final VoidCallback? onTap;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final box = Container(
      width: double.infinity,
      padding: padding,
      decoration: BoxDecoration(
        color: color ?? k.surface2.withValues(alpha: 0.7),
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: child,
    );
    return onTap == null ? box : KPressable(onTap: onTap, pressedScale: 0.99, child: box);
  }
}

/// A small round icon (rules, steps).
class RoundIcon extends StatelessWidget {
  const RoundIcon(this.icon, {super.key, this.size = 28, this.bg, this.fg, this.border = false});
  final IconData icon;
  final double size;
  final Color? bg, fg;
  final bool border;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: bg ?? k.surface3,
        shape: BoxShape.circle,
        border: border ? Border.all(color: k.line) : null,
      ),
      child: Icon(icon, size: size * 0.5, color: fg ?? k.fg2),
    );
  }
}

/// The web's IconGlyph names of the programme levels (coin, crown, 1st_place_medal, trophy, gem_stone, …).
(IconData, KTone) levelGlyph(String name) => switch (name) {
  'coin' => (LucideIcons.coins, KTone.amber),
  'crown' => (LucideIcons.crown, KTone.amber),
  '1st_place_medal' => (LucideIcons.medal, KTone.amber),
  'trophy' => (LucideIcons.trophy, KTone.amber),
  'gem_stone' => (LucideIcons.gem, KTone.accent),
  'money_bag' => (LucideIcons.handCoins, KTone.amber),
  'rocket' => (LucideIcons.rocket, KTone.accent),
  'fire' => (LucideIcons.flame, KTone.accent),
  'handshake' => (LucideIcons.handshake, KTone.amber),
  'star' => (LucideIcons.star, KTone.amber),
  _ => (LucideIcons.sparkles, KTone.amber),
};

/// Avatar of any engine symbol; an option shows its underlying with a small C / P mark (web TradeSymbolAvatar).
class TradeSymbolAvatar extends StatelessWidget {
  const TradeSymbolAvatar(this.symbol, {super.key, this.size = 24});
  final String symbol;
  final double size;

  static const _ccy = {
    'EUR': 'eu', 'GBP': 'gb', 'USD': 'us', 'JPY': 'jp', 'AUD': 'au', 'CAD': 'ca', 'CHF': 'ch', 'NZD': 'nz', 'INR': 'in', 'SGD': 'sg', //
    'HKD': 'hk', 'ZAR': 'za', 'MXN': 'mx', 'NOK': 'no', 'SEK': 'se', 'DKK': 'dk', 'PLN': 'pl', 'TRY': 'tr', 'CNH': 'cn', 'CNY': 'cn',
    'HUF': 'hu', 'CZK': 'cz',
  };

  Widget _base(BuildContext context, String s) {
    final k = context.k;
    if (kInstrumentMap.containsKey(s)) return SymbolAvatar(s, size: size);
    final b = s.length == 6 ? _ccy[s.substring(0, 3)] : null;
    final q = s.length == 6 ? _ccy[s.substring(3, 6)] : null;
    if (b != null && q != null) {
      Widget ring(Widget c) => Container(
        padding: const EdgeInsets.all(1.5),
        decoration: BoxDecoration(color: k.surface, shape: BoxShape.circle),
        child: c,
      );
      return SizedBox(
        width: size * 1.45,
        height: size,
        child: Stack(
          textDirection: TextDirection.ltr,
          children: [
            Positioned(left: 0, top: 0, child: ring(KFlag(b, size: size - 3))),
            Positioned(right: 0, top: 0, child: ring(KFlag(q, size: size - 3))),
          ],
        ),
      );
    }
    final txt = s.replaceAll(RegExp(r'[^A-Za-z0-9]'), '');
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: k.surface3,
        shape: BoxShape.circle,
        border: Border.all(color: k.line),
      ),
      child: Text(
        (txt.length > 2 ? txt.substring(0, 2) : (txt.isEmpty ? '?' : txt)).toUpperCase(),
        style: TextStyle(fontSize: math.max(8, size * 0.36), fontWeight: FontWeight.w600, color: k.fg2),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final m = RegExp(r'^([A-Za-z0-9]{2,16})-\d{8}-[\d.]+-([CPcp])$').firstMatch(symbol);
    if (m == null) return _base(context, symbol);
    final call = m[2]!.toUpperCase() == 'C';
    final badge = math.max(10.0, (size * 0.5).roundToDouble());
    final k = context.k;
    return Stack(
      clipBehavior: Clip.none,
      children: [
        _base(context, m[1]!.toUpperCase()),
        PositionedDirectional(
          end: -3,
          bottom: -3,
          child: Container(
            width: badge,
            height: badge,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: call ? k.up : k.down,
              shape: BoxShape.circle,
              border: Border.all(color: k.surface, width: 2),
            ),
            child: Text(
              call ? 'C' : 'P',
              style: TextStyle(fontSize: badge * 0.5, fontWeight: FontWeight.w700, color: Colors.white, height: 1),
            ),
          ),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ copy */

/// Copies `text` and shows the web's toast (`title`, with the short link as the description).
Future<void> partnerCopy(BuildContext context, String text, String title, {String? description}) async {
  await Clipboard.setData(ClipboardData(text: text));
  KHaptics.success();
  if (!context.mounted) return;
  ProviderScope.containerOf(
    context,
    listen: false,
  ).read(notificationsProvider.notifier).toast(NotificationKind.success, title, description: description, keep: false);
}

/// The web's small CopyButton: a copy icon that turns into a check for a moment.
class PartnerCopyButton extends StatefulWidget {
  const PartnerCopyButton({super.key, required this.value, this.size = 32, this.iconSize = 15});
  final String value;
  final double size, iconSize;

  @override
  State<PartnerCopyButton> createState() => _PartnerCopyButtonState();
}

class _PartnerCopyButtonState extends State<PartnerCopyButton> {
  bool _done = false;
  Timer? _timer;

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KPressable(
      semanticLabel: context.t('common.copy'),
      onTap: () {
        kCopy(context, widget.value);
        setState(() => _done = true);
        _timer?.cancel();
        _timer = Timer(const Duration(milliseconds: 1400), () {
          if (mounted) setState(() => _done = false);
        });
      },
      child: SizedBox.square(
        dimension: widget.size,
        child: Icon(_done ? LucideIcons.check : LucideIcons.copy, size: widget.iconSize, color: _done ? k.up : k.fg3),
      ),
    );
  }
}

/* ------------------------------------------------------------------ share */

const _waPath =
    'M12.04 2a9.9 9.9 0 0 0-8.5 14.98L2 22l5.16-1.5A9.9 9.9 0 1 0 12.04 2Zm0 18.1a8.2 8.2 0 0 1-4.18-1.14l-.3-.18-3.06.89.9-2.98-.2-.31a8.2 8.2 0 1 1 6.84 3.72Zm4.5-6.14c-.25-.12-1.46-.72-1.69-.8-.23-.08-.39-.12-.55.12-.16.25-.63.8-.78.96-.14.16-.29.18-.53.06a6.7 6.7 0 0 1-3.34-2.92c-.25-.43.25-.4.72-1.33.08-.16.04-.3-.02-.43-.06-.12-.55-1.32-.75-1.8-.2-.48-.4-.41-.55-.42h-.47a.9.9 0 0 0-.65.3 2.74 2.74 0 0 0-.86 2.04 4.76 4.76 0 0 0 1 2.53 10.9 10.9 0 0 0 4.18 3.69c1.55.67 2.16.73 2.94.61.47-.07 1.46-.6 1.66-1.18.2-.58.2-1.07.14-1.18-.06-.1-.22-.16-.47-.28Z';
const _tgPath =
    'M21.94 4.3 18.9 19.1c-.23 1.03-.83 1.28-1.69.8l-4.66-3.44-2.25 2.17c-.25.25-.46.46-.94.46l.34-4.74 8.62-7.79c.37-.33-.08-.52-.58-.19L7.1 13.08 2.5 11.64c-1-.31-1.02-1 .21-1.48l17.95-6.92c.83-.31 1.56.19 1.28 1.06Z';
const _xPath = 'M17.75 3h3.07l-6.7 7.66L22 21h-6.17l-4.83-6.32L5.47 21H2.4l7.17-8.2L2 3h6.33l4.37 5.77L17.75 3Zm-1.08 16.2h1.7L7.4 4.73H5.58L16.67 19.2Z';

/// WhatsApp / Telegram / X / Email with the link pre-filled (web ShareButtons; icons only on phones).
class ShareButtons extends ConsumerWidget {
  const ShareButtons({super.key, required this.url, this.text});
  final String url;
  final String? text;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final msg = text ?? t('partner.share.defaultText');
    const enc = Uri.encodeComponent;
    final items = <(String, Widget, String)>[
      ('WhatsApp', _svg(_waPath, k.fg2, 17), 'https://wa.me/?text=${enc('$msg $url')}'),
      ('Telegram', _svg(_tgPath, k.fg2, 17), 'https://t.me/share/url?url=${enc(url)}&text=${enc(msg)}'),
      ('X', _svg(_xPath, k.fg2, 14), 'https://x.com/intent/tweet?text=${enc(msg)}&url=${enc(url)}'),
      (t('common.email'), Icon(LucideIcons.mail, size: 17, color: k.fg2), 'mailto:?subject=${enc(t('partner.share.emailSubject'))}&body=${enc('$msg $url')}'),
    ];
    return Row(
      children: [
        for (var i = 0; i < items.length; i++) ...[
          if (i > 0) const SizedBox(width: 8),
          Expanded(
            child: KPressable(
              semanticLabel: t('partner.share.shareOn', {'name': items[i].$1}),
              onTap: () async {
                ref
                    .read(notificationsProvider.notifier)
                    .toast(NotificationKind.success, t('partner.share.opening', {'name': items[i].$1}), description: t('partner.share.prefilled'), keep: false);
                var ok = false;
                try {
                  ok = await launchUrl(Uri.parse(items[i].$3), mode: LaunchMode.externalApplication);
                } catch (_) {}
                if (!ok && context.mounted) await kShare(context, '$msg $url');
              },
              child: Container(
                height: 40,
                alignment: Alignment.center,
                decoration: BoxDecoration(
                  color: k.surface2,
                  borderRadius: BorderRadius.circular(20),
                  border: Border.all(color: k.line),
                ),
                child: items[i].$2,
              ),
            ),
          ),
        ],
      ],
    );
  }

  static Widget _svg(String d, Color c, double size) => SvgPicture.string(
    '<svg viewBox="0 0 24 24" xmlns="http://www.w3.org/2000/svg"><path fill="#000" d="$d"/></svg>',
    width: size,
    height: size,
    colorFilter: ColorFilter.mode(c, BlendMode.srcIn),
  );
}

/* ------------------------------------------------------------------ QR */

const _qrDarkFg = Color(0xFF0B0B0E);
const _qrLightBg = Colors.white;
const _qrNightFg = Colors.white;
const _qrNightBg = Color(0xFF111114);
const int kQrExportPx = 1024;

/// A QR code for a link on its own tile (web useQrCode view): light or dark, the Kalks mark in the centre.
class PartnerQr extends StatelessWidget {
  const PartnerQr({super.key, required this.value, this.size = 180, this.dark = false, this.logo = true, this.boundaryKey});
  final String value;
  final double size;
  final bool dark, logo;

  /// Wraps the tile in a RepaintBoundary with this key (PNG export).
  final GlobalKey? boundaryKey;

  @override
  Widget build(BuildContext context) {
    final fg = dark ? _qrNightFg : _qrDarkFg;
    final bg = dark ? _qrNightBg : _qrLightBg;
    final mark = size * 0.2;
    return RepaintBoundary(
      key: boundaryKey,
      child: Container(
        padding: const EdgeInsets.all(14),
        decoration: BoxDecoration(color: bg, borderRadius: BorderRadius.circular(16)),
        child: SizedBox.square(
          dimension: size,
          child: Stack(
            alignment: Alignment.center,
            children: [
              QrImageView(
                data: value,
                size: size,
                padding: EdgeInsets.zero,
                backgroundColor: bg,
                errorCorrectionLevel: QrErrorCorrectLevel.H,
                eyeStyle: QrEyeStyle(eyeShape: QrEyeShape.square, color: fg),
                dataModuleStyle: QrDataModuleStyle(dataModuleShape: QrDataModuleShape.square, color: fg),
              ),
              if (logo)
                Container(
                  width: mark + 6,
                  height: mark + 6,
                  color: bg,
                  alignment: Alignment.center,
                  child: KLogoMark(size: mark * 0.92, color: fg),
                ),
            ],
          ),
        ),
      ),
    );
  }
}

String _hex(Color c) => '#${(c.toARGB32() & 0xFFFFFF).toRadixString(16).padLeft(6, '0')}';

/// The SVG of a QR code with a quiet zone (web export copy: 1024 px, level H, the mark excavated in the centre).
Future<String> qrSvg(String value, {bool dark = false, bool logo = true}) async {
  final fg = _hex(dark ? _qrNightFg : _qrDarkFg);
  final bg = _hex(dark ? _qrNightBg : _qrLightBg);
  final img = QrImage(QrCode.fromData(data: value, errorCorrectLevel: QrErrorCorrectLevel.H));
  final n = img.moduleCount;
  const margin = 4;
  final total = n + margin * 2;
  final box = n * 0.2;
  final lo = margin + (n - box) / 2, hi = lo + box;
  final d = StringBuffer();
  for (var y = 0; y < n; y++) {
    for (var x = 0; x < n; x++) {
      if (!img.isDark(y, x)) continue;
      final px = x + margin, py = y + margin;
      if (logo && px + 1 > lo && px < hi && py + 1 > lo && py < hi) continue;
      d.write('M$px ${py}h1v1h-1z');
    }
  }
  var mark = '';
  if (logo) {
    try {
      final raw = (await rootBundle.loadString('assets/brand/kalks-mark.svg')).replaceAll('currentColor', fg);
      final h = box * 541 / 653;
      mark =
          '<image href="data:image/svg+xml;base64,${base64Encode(utf8.encode(raw))}" x="${lo.toStringAsFixed(3)}" y="${(lo + (box - h) / 2).toStringAsFixed(3)}" '
          'width="${box.toStringAsFixed(3)}" height="${h.toStringAsFixed(3)}"/>';
    } catch (_) {}
  }
  return '<?xml version="1.0" encoding="UTF-8"?>\n'
      '<svg xmlns="http://www.w3.org/2000/svg" width="$kQrExportPx" height="$kQrExportPx" viewBox="0 0 $total $total" shape-rendering="crispEdges">'
      '<rect width="$total" height="$total" fill="$bg"/><path fill="$fg" d="$d"/>$mark</svg>';
}

/// Saves the QR as SVG (share sheet -> Files) with the web's "QR code saved" toast.
Future<void> saveQrSvg(BuildContext context, WidgetRef ref, {required String value, required String fileBase, bool dark = false, bool logo = true}) async {
  final t = context.t;
  final svg = await qrSvg(value, dark: dark, logo: logo);
  final name = '$fileBase.svg';
  final ok = await shareFile((bytes: Uint8List.fromList(utf8.encode(svg)), fileName: name, contentType: 'image/svg+xml'), fallbackName: name);
  if (ok) {
    ref.read(notificationsProvider.notifier).toast(NotificationKind.success, t('partner.qr.saved'), description: name);
  }
}

/// Saves the QR tile under `boundary` as a 1024 px PNG.
Future<void> saveQrPng(BuildContext context, WidgetRef ref, {required GlobalKey boundary, required String fileBase}) async {
  final t = context.t;
  final notes = ref.read(notificationsProvider.notifier);
  try {
    final ro = boundary.currentContext?.findRenderObject();
    if (ro is! RenderRepaintBoundary) throw StateError('qr');
    final image = await ro.toImage(pixelRatio: kQrExportPx / ro.size.width);
    final data = await image.toByteData(format: ui.ImageByteFormat.png);
    if (data == null) throw StateError('png');
    final name = '$fileBase.png';
    final ok = await shareFile((bytes: data.buffer.asUint8List(), fileName: name, contentType: 'image/png'), fallbackName: name);
    if (ok) notes.toast(NotificationKind.success, t('partner.qr.saved'), description: '$name · $kQrExportPx×$kQrExportPx');
  } catch (_) {
    notes.toast(NotificationKind.error, t('partner.qr.pngFailed'), description: t('partner.qr.pngFailedHint'));
  }
}

/// The QR sheet for one link (web QrDialog): the code, the short link, SVG and PNG downloads.
Future<void> showPartnerQrSheet(BuildContext context, {required String value, required String title, required String fileBase}) => showKSheet<void>(
  context,
  title: title,
  builder: (_) => _QrSheet(value: value, fileBase: fileBase),
);

class _QrSheet extends ConsumerStatefulWidget {
  const _QrSheet({required this.value, required this.fileBase});
  final String value, fileBase;

  @override
  ConsumerState<_QrSheet> createState() => _QrSheetState();
}

class _QrSheetState extends ConsumerState<_QrSheet> {
  final _key = GlobalKey();

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KSheetContent(
      footer: Row(
        children: [
          Expanded(
            child: KButton(
              label: 'SVG',
              icon: LucideIcons.download,
              variant: KButtonVariant.surface,
              expand: true,
              onPressed: () => saveQrSvg(context, ref, value: widget.value, fileBase: widget.fileBase),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: KButton(
              label: 'PNG',
              icon: LucideIcons.download,
              expand: true,
              onPressed: () => saveQrPng(context, ref, boundary: _key, fileBase: widget.fileBase),
            ),
          ),
        ],
      ),
      children: [
        Text(
          t('partner.qr.dialogDescription'),
          textAlign: TextAlign.center,
          style: context.text.footnote.copyWith(color: k.fg3),
        ),
        const SizedBox(height: 16),
        Center(
          child: Container(
            padding: const EdgeInsets.all(8),
            decoration: BoxDecoration(
              borderRadius: BorderRadius.circular(22),
              border: Border.all(color: k.line),
            ),
            child: PartnerQr(value: widget.value, size: 220, boundaryKey: _key),
          ),
        ),
        const SizedBox(height: 12),
        Text(
          shortUrl(widget.value),
          textAlign: TextAlign.center,
          textDirection: TextDirection.ltr,
          maxLines: 1,
          overflow: TextOverflow.ellipsis,
          style: context.text.mono(12.5, color: k.fg2),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ charts */

/// Capsule bars with the selected bar's value in a pill above it (web CapsuleBars): the last bar is selected until
/// another is tapped. Labels under the bars (only the ends and the selected one when they don't fit).
class CapsuleBars extends StatefulWidget {
  const CapsuleBars({super.key, required this.labels, required this.values, this.height = 190, required this.format});
  final List<String> labels;
  final List<double> values;
  final double height;
  final String Function(double v) format;

  @override
  State<CapsuleBars> createState() => _CapsuleBarsState();
}

class _CapsuleBarsState extends State<CapsuleBars> {
  int? _sel;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final n = widget.values.length;
    if (n == 0) return SizedBox(height: widget.height);
    final max = widget.values.fold<double>(0, math.max);
    final sel = (_sel ?? n - 1).clamp(0, n - 1);
    return LayoutBuilder(
      builder: (context, c) {
        final slot = c.maxWidth / n;
        final crowded = slot < 40;
        const labelH = 22.0, pillH = 26.0;
        final barArea = widget.height - labelH - pillH;
        return SizedBox(
          height: widget.height,
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              for (var i = 0; i < n; i++)
                Expanded(
                  child: GestureDetector(
                    behavior: HitTestBehavior.opaque,
                    onTap: () {
                      KHaptics.selection();
                      setState(() => _sel = i);
                    },
                    child: Padding(
                      padding: EdgeInsets.symmetric(horizontal: math.min(6, slot * 0.12)),
                      child: Column(
                        mainAxisAlignment: MainAxisAlignment.end,
                        children: [
                          if (i == sel)
                            SizedBox(
                              height: pillH,
                              child: OverflowBox(
                                maxWidth: 120,
                                child: Container(
                                  padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 3),
                                  decoration: BoxDecoration(
                                    color: k.surface2,
                                    borderRadius: BorderRadius.circular(12),
                                    border: Border.all(color: k.line),
                                    boxShadow: k.shadowCard,
                                  ),
                                  child: Text(
                                    widget.format(widget.values[i]),
                                    textDirection: TextDirection.ltr,
                                    style: context.text.mono(11, color: k.fg),
                                  ),
                                ),
                              ),
                            ),
                          const SizedBox(height: 4),
                          AnimatedContainer(
                            duration: const Duration(milliseconds: 500),
                            curve: Curves.easeOutCubic,
                            height: math.max(6, max <= 0 ? 6 : widget.values[i] / max * (barArea - 4)),
                            constraints: const BoxConstraints(maxWidth: 56),
                            decoration: BoxDecoration(
                              borderRadius: BorderRadius.circular(40),
                              border: Border.all(color: i == sel ? k.ember.withValues(alpha: 0.4) : k.line),
                              gradient: LinearGradient(
                                begin: Alignment.topCenter,
                                end: Alignment.bottomCenter,
                                colors: i == sel ? const [Color(0xFFFF8A3D), Color(0xFFB8330F)] : [k.surface3, k.surface2],
                              ),
                              boxShadow: i == sel ? [BoxShadow(color: k.ember.withValues(alpha: 0.45), blurRadius: 24, spreadRadius: -8)] : null,
                            ),
                          ),
                          SizedBox(
                            height: labelH,
                            child: Padding(
                              padding: const EdgeInsets.only(top: 6),
                              child: !crowded || i == sel || i == 0 || i == n - 1
                                  ? OverflowBox(
                                      maxWidth: 90,
                                      child: Text(
                                        widget.labels[i],
                                        maxLines: 1,
                                        softWrap: false,
                                        style: context.text.caption.copyWith(fontSize: 10, color: i == sel ? k.fg : k.fg3, fontWeight: FontWeight.w500),
                                      ),
                                    )
                                  : null,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                ),
            ],
          ),
        );
      },
    );
  }
}

/* ------------------------------------------------------------------ tables */

/// The web DataTable's pager: "1–12 of 30" and previous / next.
class PartnerPager extends StatelessWidget {
  const PartnerPager({super.key, required this.from, required this.to, required this.total, required this.page, required this.pages, this.onPrev, this.onNext});
  final int from, to, total, page, pages;
  final VoidCallback? onPrev, onNext;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final style = context.text.footnote.copyWith(color: k.fg3, fontFeatures: kTabular);
    return Row(
      children: [
        Expanded(child: Text(t('partner.com.range', {'from': from, 'to': to, 'total': total}), style: style)),
        if (pages > 1) ...[
          KIconButton(icon: rtl ? LucideIcons.chevronRight : LucideIcons.chevronLeft, onPressed: onPrev, semanticLabel: t('partner.com.prevPage')),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6),
            child: Text('$page / $pages', style: style),
          ),
          KIconButton(icon: rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight, onPressed: onNext, semanticLabel: t('partner.com.nextPage')),
        ],
      ],
    );
  }
}

/// The web DataTable's CSV export: the file goes to the share sheet (save to Files, send on).
Future<void> exportCsv(BuildContext context, WidgetRef ref, {required String name, required List<String> headers, required List<List<Object?>> rows}) async {
  final t = context.t;
  String esc(Object? v) {
    final s = '${v ?? ''}';
    return RegExp(r'[",\n]').hasMatch(s) ? '"${s.replaceAll('"', '""')}"' : s;
  }

  final csv = [headers.map(esc).join(','), for (final r in rows) r.map(esc).join(',')].join('\n');
  final file = '$name.csv';
  final ok = await shareFile((bytes: Uint8List.fromList(utf8.encode(csv)), fileName: file, contentType: 'text/csv'), fallbackName: file);
  if (ok) {
    ref
        .read(notificationsProvider.notifier)
        .toast(
          NotificationKind.success,
          t('app.partner.csvSaved', {'file': file}),
          description: t('app.partner.csvRows', {'count': rows.length, 'cols': headers.length}),
        );
  }
}

/// The small "CSV" button of the web tables.
class CsvButton extends StatelessWidget {
  const CsvButton({super.key, required this.onPressed});
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) =>
      KButton(label: 'CSV', icon: LucideIcons.download, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onPressed);
}

/// A card section's gap.
const Widget kGap = SizedBox(height: 16);

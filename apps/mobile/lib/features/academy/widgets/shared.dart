// Academy building blocks (web components/academy/live/shared.tsx and the small pieces the pages share): the
// unavailable / not-found state, back link, chapter segments, status dots, track badges, "Practise in Kalks Trader",
// the risk note, phase covers, certificate image and its actions.
import 'package:flutter/cupertino.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/config/app_config.dart';
import '../../../core/files.dart';
import '../../../core/models/account.dart';
import '../../../core/notifications/notifications.dart';
import '../../../data/client_data.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../academy_api.dart';

bool _rtl(BuildContext context) => Directionality.of(context) == TextDirection.rtl;

/// The web's AcademyUnavailable: "The Academy is unavailable" + Try again, or "Not found" + Back to the Academy.
class AcademyUnavailable extends StatelessWidget {
  const AcademyUnavailable({super.key, this.error, this.onRetry, this.notFound = false});
  final Object? error;
  final VoidCallback? onRetry;
  final bool notFound;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final nf = notFound || (error is ApiException && (error as ApiException).status == 404);
    return KCard(
      child: KEmptyState(
        art: nf ? KIllustrationName.market : KIllustrationName.connectionLost,
        title: nf ? t('academy.unavailable.notFoundTitle') : t('academy.unavailable.title'),
        text: nf ? t('academy.unavailable.notFoundText') : (error is ApiException ? localizeError(error as ApiException, t) : t('academy.unavailable.text')),
        action: nf
            ? KButton(
                label: t('academy.backToAcademy'),
                icon: _rtl(context) ? LucideIcons.chevronRight : LucideIcons.chevronLeft,
                variant: KButtonVariant.surface,
                size: KButtonSize.sm,
                onPressed: () => context.go('/academy'),
              )
            : KButton(label: t('common.retry'), icon: LucideIcons.rotateCw, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onRetry),
      ),
    );
  }
}

/// The web's BackLink ("‹ Academy"): back to where the reader came from, else to `href`.
class AcademyBackLink extends StatelessWidget {
  const AcademyBackLink({super.key, required this.href, required this.label});
  final String href;
  final String label;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Align(
      alignment: AlignmentDirectional.centerStart,
      child: KPressable(
        semanticLabel: label,
        onTap: () {
          if (context.canPop()) {
            context.pop();
          } else {
            context.go(href);
          }
        },
        child: Padding(
          padding: const EdgeInsetsDirectional.only(end: 8),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(_rtl(context) ? LucideIcons.chevronRight : LucideIcons.chevronLeft, size: 16, color: k.fg3),
              const SizedBox(width: 4),
              Flexible(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.label.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// The page skeleton while the catalogue loads (web PageSkeleton, phone column).
class AcademyPageSkeleton extends StatelessWidget {
  const AcademyPageSkeleton({super.key});

  @override
  Widget build(BuildContext context) => const Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      KSkeleton(width: 220, height: 30, radius: 12),
      SizedBox(height: 16),
      KSkeleton(height: 300, radius: 24),
      SizedBox(height: 16),
      KSkeleton(height: 280, radius: 24),
      SizedBox(height: 16),
      KSkeleton(height: 280, radius: 24),
    ],
  );
}

/// Small upper-case label (web .k-label).
class AcademyLabel extends StatelessWidget {
  const AcademyLabel(this.text, {super.key, this.color});
  final String text;
  final Color? color;

  @override
  Widget build(BuildContext context) => Text(
    text.toUpperCase(),
    style: context.text.micro.copyWith(color: color ?? context.k.fg3, fontWeight: FontWeight.w600, letterSpacing: 0.8, fontSize: 11),
  );
}

/// Segmented progress bar, one segment per chapter.
class Segments extends StatelessWidget {
  const Segments({super.key, required this.done, required this.total});
  final int done, total;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    if (total <= 0) return const SizedBox(height: 6);
    return Row(
      children: [
        for (var i = 0; i < total; i++) ...[
          if (i > 0) const SizedBox(width: 4),
          Expanded(
            child: Container(
              height: 6,
              decoration: BoxDecoration(color: i < done ? k.ember : k.surface3, borderRadius: BorderRadius.circular(3)),
            ),
          ),
        ],
      ],
    );
  }
}

/// A progress bar in percent (0–100), ember or up (complete).
class AcademyProgress extends StatelessWidget {
  const AcademyProgress(this.pct, {super.key, this.up = false, this.height = 6});
  final int pct;
  final bool up;
  final double height;

  @override
  Widget build(BuildContext context) => KProgressBar(value: pct / 100, color: up ? context.k.up : context.k.ember, height: height);
}

enum DotState { done, open, locked }

/// The round chapter number / check / lock of the chapter lists.
class StatusDot extends StatelessWidget {
  const StatusDot({super.key, required this.state, this.n});
  final DotState state;
  final int? n;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final (bg, fg, border) = switch (state) {
      DotState.done => (k.upSoft, k.up, k.up.withValues(alpha: 0.3)),
      DotState.locked => (k.surface2, k.fg3, k.line),
      DotState.open => (k.surface2, k.fg2, k.line),
    };
    return Container(
      width: 28,
      height: 28,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: bg,
        shape: BoxShape.circle,
        border: Border.all(color: border),
      ),
      child: switch (state) {
        DotState.done => Icon(LucideIcons.check, size: 14, color: fg),
        DotState.locked => Icon(LucideIcons.lock, size: 14, color: fg),
        DotState.open => Text(
          '${n ?? ''}',
          style: context.text.caption.copyWith(color: fg, fontFeatures: kTabular),
          textDirection: TextDirection.ltr,
        ),
      },
    );
  }
}

/// A round soft-tinted icon badge (track badges, exam / certificate marks).
class SoftBadge extends StatelessWidget {
  const SoftBadge({super.key, required this.icon, required this.tone, this.size = 40});
  final IconData icon;
  final KChipTone tone;
  final double size;

  @override
  Widget build(BuildContext context) {
    final (bg, fg, border) = context.k.chip(tone);
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: tone == KChipTone.neutral ? context.k.surface2 : bg,
        shape: BoxShape.circle,
        border: Border.all(color: border),
      ),
      child: Icon(icon, size: size * 0.42, color: tone == KChipTone.neutral ? context.k.fg3 : fg),
    );
  }
}

/// A small stat tile (value over label, optional icon): the stats card and the exam card.
class MiniStat extends StatelessWidget {
  const MiniStat({super.key, required this.value, required this.label, this.icon});
  final String value, label;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          if (icon != null) ...[Icon(icon, size: 14, color: k.fg3), const SizedBox(height: 4)],
          FittedBox(
            fit: BoxFit.scaleDown,
            alignment: AlignmentDirectional.centerStart,
            child: Text(value, style: context.text.figure, textDirection: TextDirection.ltr),
          ),
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w500),
          ),
        ],
      ),
    );
  }
}

/// "Educational content only, not investment advice…" under the Academy pages.
class RiskNote extends StatelessWidget {
  const RiskNote({super.key});

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: 24),
    child: Text(
      context.t('academy.riskNote'),
      style: context.text.caption.copyWith(color: context.k.fg3, fontWeight: FontWeight.w400, height: 1.5),
    ),
  );
}

/// A phase as its hardcover book (web BookCover): the finish per level, the number, the title, a ribbon while
/// reading and the gold seal once certified.
class PhaseBook extends StatelessWidget {
  const PhaseBook({super.key, required this.p});
  final AcademyPhase p;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final done = p.certificate != null;
    return KBookCover(
      finish: bookFinish(p),
      series: t.dyn('academy.book.series', fallback: 'Kalks Academy'),
      kicker: p.elective ? t('academy.elective') : t('academy.phaseN', {'n': p.order}),
      number: p.order,
      title: p.title,
      level: levelLabel(t, p.level),
      minutes: fmtMin(t, p.minutes),
      reading: !done && p.done > 0,
      done: done,
    );
  }
}

/// A soft glow of a book's finish over a card (instead of a photo): the finish colour from the top end, fading out.
class FinishWash extends StatelessWidget {
  const FinishWash({super.key, required this.finish, this.strength = 0.34});
  final KCardFinish finish;
  final double strength;

  @override
  Widget build(BuildContext context) {
    final c = cardFinishGradient(finish, context.k.ember).colors[1];
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: RadialGradient(
          center: AlignmentDirectional.topEnd.resolve(Directionality.of(context)),
          radius: 1.25,
          colors: [
            c.withValues(alpha: strength),
            c.withValues(alpha: 0),
          ],
          stops: const [0, 0.75],
        ),
      ),
    );
  }
}

/// A gradient from the page background (start) to transparent (end) over a cover, like the web's hero washes.
class CoverWash extends StatelessWidget {
  const CoverWash({super.key, this.vertical = false, this.strength = 0.9});
  final bool vertical;
  final double strength;

  @override
  Widget build(BuildContext context) {
    final c = context.k.cardBg;
    final rtl = _rtl(context);
    return DecoratedBox(
      decoration: BoxDecoration(
        gradient: LinearGradient(
          begin: vertical ? Alignment.bottomCenter : (rtl ? Alignment.centerRight : Alignment.centerLeft),
          end: vertical ? Alignment.topCenter : (rtl ? Alignment.centerLeft : Alignment.centerRight),
          colors: [
            c,
            c.withValues(alpha: strength),
            c.withValues(alpha: 0.3),
          ],
          stops: const [0, 0.5, 1],
        ),
      ),
    );
  }
}

/// "Practise in Kalks Trader": opens the client's active demo account in Kalks Trader, or offers a free demo account.
class PracticeButton extends ConsumerWidget {
  const PracticeButton({super.key, this.small = false, this.label});
  final bool small;
  final String? label;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final text = label ?? t('academy.practice.demo');
    final accounts = ref.watch(accountsProvider).value;
    final size = small ? KButtonSize.sm : KButtonSize.md;
    final arrow = _rtl(context) ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight;
    if (accounts == null) {
      return KButton(label: text, trailingIcon: arrow, variant: KButtonVariant.surface, size: size, onPressed: () => context.push('/trader'));
    }
    final demo = accounts.where((a) => a.type == AccountKind.demo && a.status == 'active').firstOrNull;
    if (demo == null) {
      return KButton(
        label: t('academy.practice.openFreeDemo'),
        trailingIcon: arrow,
        variant: KButtonVariant.surface,
        size: size,
        onPressed: () => context.go('/accounts/new?type=demo'),
      );
    }
    return KButton(
      label: '$text  #${demo.login}',
      trailingIcon: arrow,
      variant: KButtonVariant.surface,
      size: size,
      onPressed: () => context.push('/trader?login=${demo.login}'),
    );
  }
}

/* ------------------------------------------------------------------ certificates */

/// The certificate image (A4 landscape SVG on the web's dark plate); tap to see it full size.
class CertificateImage extends ConsumerWidget {
  const CertificateImage({super.key, required this.code, this.semanticLabel, this.radius = 12, this.face});
  final String code;
  final String? semanticLabel;
  final double radius;

  /// Drawn instead of the certificate's picture (the paper certificate, KCertificateCard); a tap still opens the
  /// real one.
  final Widget? face;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final svg = ref.watch(certificateSvgProvider(code));
    final plate =
        face ??
        AspectRatio(
          aspectRatio: 1600 / 1131,
          child: Container(
            decoration: BoxDecoration(
              color: const Color(0xFF0B0B0E),
              borderRadius: BorderRadius.circular(radius),
              border: Border.all(color: k.line),
            ),
            clipBehavior: Clip.antiAlias,
            child: switch (svg) {
              AsyncData(:final value) => SvgPicture.string(
                value,
                semanticsLabel: semanticLabel,
                errorBuilder: (_, _, _) => const Center(child: Icon(LucideIcons.award, color: Color(0xFF8B8B96))),
              ),
              AsyncError() => const Center(child: Icon(LucideIcons.award, color: Color(0xFF8B8B96))),
              _ => const Center(child: CupertinoActivityIndicator(color: Color(0xFF8B8B96))),
            },
          ),
        );
    return KPressable(
      pressedOpacity: 0.85,
      semanticLabel: semanticLabel,
      onTap: svg.hasValue
          ? () => showKSheet<void>(
              context,
              title: semanticLabel,
              builder: (_) => _CertificateViewer(svg: svg.requireValue),
            )
          : null,
      child: plate,
    );
  }
}

class _CertificateViewer extends StatelessWidget {
  const _CertificateViewer({required this.svg});
  final String svg;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(12, 4, 12, 20),
    child: InteractiveViewer(
      maxScale: 5,
      child: AspectRatio(
        aspectRatio: 1600 / 1131,
        child: ClipRRect(
          borderRadius: BorderRadius.circular(10),
          child: ColoredBox(color: const Color(0xFF0B0B0E), child: SvgPicture.string(svg)),
        ),
      ),
    ),
  );
}

/// Download (web `image?download=1`): the SVG file goes to the share sheet, where the client saves or sends it.
Future<void> downloadCertificate(BuildContext context, WidgetRef ref, String code) async {
  final t = context.t;
  final notes = ref.read(notificationsProvider.notifier);
  try {
    final f = await ref.read(academyApiProvider).certificateFile(code);
    final ok = await shareFile(f, fallbackName: 'kalks-academy-$code.svg', subject: t('academy.cert.title'));
    if (!ok) notes.toast(NotificationKind.error, t('common.errorRetry'));
  } on ApiException catch (e) {
    notes.toast(NotificationKind.error, localizeError(e, t));
  } catch (_) {
    notes.toast(NotificationKind.error, t('common.errorRetry'));
  }
}

/// The public verification page (web /certificate/{code}, opened in a new tab).
Future<void> openVerifyPage(WidgetRef ref, String code) async {
  final base = ref.read(configProvider).appUrl.replaceAll(RegExp(r'/+$'), '');
  await launchUrl(Uri.parse('$base/certificate/$code'), mode: LaunchMode.externalApplication);
}

/// Download · (Copy link) · Verify page buttons of a certificate.
class CertificateActions extends ConsumerWidget {
  const CertificateActions({super.key, required this.code, this.verifyUrl, this.verifyLabel});
  final String code;

  /// The verification link to copy (the progress page's tiles); null hides "Copy link".
  final String? verifyUrl;

  /// "Verify" (progress tiles) or "Verify page" (phase card, exam result).
  final String? verifyLabel;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    return Wrap(
      spacing: 8,
      runSpacing: 8,
      children: [
        KButton(
          label: t('common.download'),
          icon: LucideIcons.download,
          variant: KButtonVariant.surface,
          size: KButtonSize.sm,
          onPressed: () => downloadCertificate(context, ref, code),
        ),
        if (verifyUrl != null)
          KButton(
            label: t('academy.cert.copyLink'),
            icon: LucideIcons.copy,
            variant: KButtonVariant.ghost,
            size: KButtonSize.sm,
            onPressed: () => kCopy(context, verifyUrl!, message: t('academy.toast.linkCopied')),
          ),
        KButton(
          label: verifyLabel ?? t('academy.cert.verifyPage'),
          icon: LucideIcons.shieldCheck,
          variant: KButtonVariant.ghost,
          size: KButtonSize.sm,
          onPressed: () => openVerifyPage(ref, code),
        ),
      ],
    );
  }
}

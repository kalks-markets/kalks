// The Client Area's photo heroes (founder 2026-10-10, web apps/crm/components/page-hero.tsx): every main section
// opens on the founder's own solid-colour portrait with the page's title on it, centred near the bottom (the short
// line, the buttons and, on Home, only the Ask Kalks AI bar). Every page of a section shows the same photo blurred
// behind it (KBackdrop.photo), so moving between pages never drops to plain black.
// The pictures are assets/heroes/<section>.jpg at their native size (tool/heroes.mjs); the shell decides which pages
// open on one (lib/shell/page_hero.dart) and KPageScroll lays the page out in a sheet over it.
import 'package:flutter/material.dart';

import '../tokens.dart';
import '../typography.dart';
import 'pressable.dart';

/// One photo per section (web HeroPage).
enum KHeroPhoto { dashboard, accounts, wallet, portfolio, copy, prop, markets, options, partner, rewards, academy, profile, support, news }

extension KHeroPhotoX on KHeroPhoto {
  /// The sharp picture of the hero.
  String get asset => 'assets/heroes/$name.jpg';

  /// A few-pixel blurred copy: the page backdrop of every page of the section.
  String get blurred => 'assets/heroes/blur/$name.jpg';

  /// Where the subject sits (the picture is cover-fitted; phones keep the face between the controls and the title).
  Alignment get focus => switch (this) {
    KHeroPhoto.dashboard => const Alignment(0, -0.35),
    KHeroPhoto.accounts => const Alignment(0.2, -1),
    KHeroPhoto.wallet => const Alignment(0.15, -1),
    KHeroPhoto.portfolio => const Alignment(0.1, -1),
    KHeroPhoto.copy => const Alignment(0, -0.8),
    KHeroPhoto.prop => const Alignment(0.3, 0),
    KHeroPhoto.markets => const Alignment(0, -0.62),
    KHeroPhoto.options => const Alignment(0, -0.2),
    KHeroPhoto.partner => const Alignment(0, -0.55),
    KHeroPhoto.rewards => const Alignment(0, -0.45),
    KHeroPhoto.academy => const Alignment(0, -0.05),
    KHeroPhoto.profile => const Alignment(0, -1),
    KHeroPhoto.support => const Alignment(0, -0.62),
    KHeroPhoto.news => const Alignment(0, -0.75),
  };

  /// The photo's own backdrop colour (web PHOTO_RGB), shown while the picture decodes.
  Color get tone => switch (this) {
    KHeroPhoto.dashboard => const Color(0xFF960A08),
    KHeroPhoto.accounts => const Color(0xFFD60219),
    KHeroPhoto.wallet => const Color(0xFFCA0010),
    KHeroPhoto.portfolio => const Color(0xFF0001BC),
    KHeroPhoto.copy => const Color(0xFF095085),
    KHeroPhoto.prop => const Color(0xFF9D89E4),
    KHeroPhoto.markets => const Color(0xFFE52C01),
    KHeroPhoto.options => const Color(0xFF012576),
    KHeroPhoto.partner => const Color(0xFF020199),
    KHeroPhoto.rewards => const Color(0xFFCB0501),
    KHeroPhoto.academy => const Color(0xFF013C93),
    KHeroPhoto.profile => const Color(0xFFDD4027),
    KHeroPhoto.support => const Color(0xFFE3272D),
    KHeroPhoto.news => const Color(0xFFB0221D),
  };
}

/// The section a web path belongs to, for its photo (web photoForPath): every page of a section shares it, the rest
/// use the Dashboard's.
KHeroPhoto heroPhotoFor(String path) {
  final seg = path.split('/').where((s) => s.isNotEmpty).firstOrNull ?? '';
  return switch (seg) {
    'accounts' => KHeroPhoto.accounts,
    'wallet' => KHeroPhoto.wallet,
    'portfolio' => KHeroPhoto.portfolio,
    'social' => KHeroPhoto.copy,
    'prop' => KHeroPhoto.prop,
    'markets' => KHeroPhoto.markets,
    'options' || 'developer' => KHeroPhoto.options,
    'partner' => KHeroPhoto.partner,
    'rewards' => KHeroPhoto.rewards,
    'academy' => KHeroPhoto.academy,
    'profile' => KHeroPhoto.profile,
    'support' => KHeroPhoto.support,
    'news' || 'calendar' => KHeroPhoto.news,
    _ => KHeroPhoto.dashboard,
  };
}

/// The picture behind a hero page (KPageHero.picture): cover-fitted on its focus, a soft dark top for the status bar
/// and the floating controls.
class KHeroPicture extends StatelessWidget {
  const KHeroPicture(this.photo, {super.key});
  final KHeroPhoto photo;

  @override
  Widget build(BuildContext context) => Stack(
    fit: StackFit.expand,
    children: [
      ColoredBox(color: photo.tone),
      Image.asset(photo.asset, fit: BoxFit.cover, alignment: photo.focus, excludeFromSemantics: true, gaplessPlayback: true),
      const DecoratedBox(
        decoration: BoxDecoration(
          gradient: LinearGradient(
            begin: Alignment.topCenter,
            end: Alignment.bottomCenter,
            colors: [Color(0x8C000000), Color(0x3F000000), Color(0x00000000)],
            stops: [0, 0.16, 0.36],
          ),
        ),
      ),
    ],
  );
}

/// What scrolls with the page over the picture (KPageHero.child): the shade that keeps the text readable, then,
/// centred at the bottom, the title, the short line, the buttons and the body (Home: the Ask Kalks AI bar only).
class KHeroCopy extends StatelessWidget {
  const KHeroCopy({super.key, this.title, this.lead, this.actions = const [], this.body});
  final String? title;
  final String? lead;
  final List<Widget> actions;
  final Widget? body;

  @override
  Widget build(BuildContext context) {
    final shadow = [Shadow(color: Colors.black.withValues(alpha: 0.4), blurRadius: 24, offset: const Offset(0, 2))];
    final hasText = title != null || lead != null || actions.isNotEmpty;
    return Stack(
      fit: StackFit.expand,
      children: [
        const DecoratedBox(
          decoration: BoxDecoration(
            gradient: LinearGradient(
              begin: Alignment.bottomCenter,
              end: Alignment.topCenter,
              colors: [Color(0xA6000000), Color(0x40000000), Color(0x00000000)],
              stops: [0, 0.38, 0.7],
            ),
          ),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(20, 0, 20, 30),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.end,
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              if (title != null)
                Text(
                  title!,
                  key: const ValueKey('page-hero-title'),
                  textAlign: TextAlign.center,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.largeTitle.copyWith(
                    color: Colors.white,
                    fontSize: 36,
                    fontWeight: FontWeight.w700,
                    height: 1.02,
                    letterSpacing: -1.2,
                    shadows: shadow,
                  ),
                ),
              if (lead != null) ...[
                SizedBox(height: title != null ? 10 : 0),
                Text(
                  lead!,
                  textAlign: TextAlign.center,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.callout.copyWith(color: Colors.white.withValues(alpha: 0.88), fontSize: 14.5, height: 1.4, shadows: shadow),
                ),
              ],
              if (actions.isNotEmpty) ...[const SizedBox(height: 18), Wrap(alignment: WrapAlignment.center, spacing: 10, runSpacing: 10, children: actions)],
              if (body != null) ...[if (hasText) const SizedBox(height: 18), body!],
            ],
          ),
        ),
      ],
    );
  }
}

/// A button on the photo (web HeroButton): white glass, or the orange primary.
class KHeroButton extends StatelessWidget {
  const KHeroButton({super.key, required this.label, required this.onPressed, this.icon, this.primary = false});
  final String label;
  final VoidCallback? onPressed;
  final IconData? icon;
  final bool primary;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final fg = primary ? k.onEmber : Colors.white;
    return KPressable(
      onTap: onPressed,
      semanticLabel: label,
      child: Container(
        height: 44,
        padding: const EdgeInsets.symmetric(horizontal: 20),
        decoration: BoxDecoration(
          color: primary ? k.ember : Colors.white.withValues(alpha: 0.14),
          borderRadius: BorderRadius.circular(22),
          border: primary ? null : Border.all(color: Colors.white.withValues(alpha: 0.26)),
          boxShadow: primary ? [BoxShadow(color: k.ember.withValues(alpha: 0.55), offset: const Offset(0, 10), blurRadius: 30, spreadRadius: -12)] : null,
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (icon != null) ...[Icon(icon, size: 17, color: fg), const SizedBox(width: 8)],
            Text(label, style: context.text.headline.copyWith(fontSize: 14, color: fg, height: 1.1)),
          ],
        ),
      ),
    );
  }
}

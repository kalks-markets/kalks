import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

import '../illustrations.g.dart';
import '../tokens.dart';
import '../typography.dart';

/// The Kalks "K" mark (assets/brand/kalks-mark.svg) in any colour; the real monochrome logo takes the text colour by
/// default (web svg[aria-label="Kalks"]: currentColor).
class KLogoMark extends StatelessWidget {
  const KLogoMark({super.key, this.size = 24, this.color});
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) => SvgPicture.asset(
    'assets/brand/kalks-mark.svg',
    width: size,
    height: size * 541 / 653,
    colorFilter: ColorFilter.mode(color ?? context.k.fg, BlendMode.srcIn),
    semanticsLabel: 'Kalks',
  );
}

/// The full "Kalks" word mark (assets/brand/kalks-logo.svg).
class KLogo extends StatelessWidget {
  const KLogo({super.key, this.height = 22, this.color});
  final double height;
  final Color? color;

  @override
  Widget build(BuildContext context) => SvgPicture.asset(
    'assets/brand/kalks-logo.svg',
    height: height,
    colorFilter: ColorFilter.mode(color ?? context.k.fg, BlendMode.srcIn),
    semanticsLabel: 'Kalks',
  );
}

/// The brand disc in the header (web BrandAvatar + .k-brand-disc): the white K on a black disc with a faint ring (the
/// real monochrome logo, 2026-10-10); a white-label broker without a logo gets its first letter on its colour.
class KBrandAvatar extends StatelessWidget {
  const KBrandAvatar({super.key, this.size = 40, this.letter});
  final double size;

  /// The broker's initial (config.tenant.name) when it isn't the stock Kalks brand.
  final String? letter;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final broker = letter != null;
    return Container(
      width: size,
      height: size,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        shape: BoxShape.circle,
        color: broker ? null : Colors.black,
        gradient: broker
            ? RadialGradient(
                center: const Alignment(-0.4, -0.5),
                radius: 0.9,
                colors: [Color.lerp(k.ember, Colors.white, 0.38)!, k.ember],
                stops: const [0, 0.62],
              )
            : null,
        border: broker ? null : Border.all(color: Colors.white.withValues(alpha: 0.16)),
        boxShadow: [
          BoxShadow(
            color: broker ? k.ember.withValues(alpha: 0.55) : Colors.black.withValues(alpha: 0.5),
            offset: const Offset(0, 10),
            blurRadius: 22,
            spreadRadius: -10,
          ),
        ],
      ),
      child: broker
          ? Text(
              letter!,
              style: context.text.headline.copyWith(color: Colors.white, fontSize: size * 0.45, fontWeight: FontWeight.w700, height: 1),
            )
          : KLogoMark(size: size * 0.5, color: Colors.white),
    );
  }
}

/// The client's initials (web Avatar), with an optional verified tick.
class KAvatar extends StatelessWidget {
  const KAvatar({super.key, required this.name, this.size = 36, this.verified = false});
  final String name;
  final double size;
  final bool verified;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final initials = name.trim().split(RegExp(r'\s+')).where((p) => p.isNotEmpty).take(2).map((p) => p.characters.first.toUpperCase()).join();
    return SizedBox(
      width: size,
      height: size,
      child: Stack(
        clipBehavior: Clip.none,
        children: [
          Container(
            width: size,
            height: size,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              color: k.surface3,
              shape: BoxShape.circle,
              border: Border.all(color: k.line),
            ),
            child: Text(
              initials,
              style: context.text.caption.copyWith(fontSize: size * 0.34, fontWeight: FontWeight.w600, color: k.fg2),
            ),
          ),
          if (verified)
            PositionedDirectional(
              end: -1,
              bottom: -1,
              child: Container(
                width: size * 0.36,
                height: size * 0.36,
                decoration: BoxDecoration(
                  color: k.up,
                  shape: BoxShape.circle,
                  border: Border.all(color: k.surface, width: 2),
                ),
                child: Icon(Icons.check_rounded, size: size * 0.22, color: Colors.white),
              ),
            ),
        ],
      ),
    );
  }
}

/// One of the founder's illustrations (assets/illustrations, PNG at 1x/2x/3x), sized by width with a height cap.
class KIllustration extends StatelessWidget {
  const KIllustration(this.name, {super.key, this.width = 200, this.maxHeight});
  final KIllustrationName name;
  final double width;
  final double? maxHeight;

  @override
  Widget build(BuildContext context) {
    var w = width;
    var h = width / name.aspect;
    if (maxHeight != null && h > maxHeight!) {
      h = maxHeight!;
      w = h * name.aspect;
    }
    return Image.asset(name.asset, width: w, height: h, fit: BoxFit.contain, excludeFromSemantics: true);
  }
}

/// A round country flag (web Flag: flag-icons 1x1 as a circle). `country` is the ISO code ("in", "ae", "gb").
class KFlag extends StatelessWidget {
  const KFlag(this.country, {super.key, this.size = 20});
  final String country;
  final double size;

  @override
  Widget build(BuildContext context) => Container(
    width: size,
    height: size,
    decoration: BoxDecoration(
      shape: BoxShape.circle,
      border: Border.all(color: Colors.black.withValues(alpha: 0.12), width: 0.6),
    ),
    child: ClipOval(
      child: Image.asset(
        'assets/flags/${country.toLowerCase()}.png',
        width: size,
        height: size,
        fit: BoxFit.cover,
        errorBuilder: (_, _, _) => ColoredBox(color: context.k.surface3),
      ),
    ),
  );
}

/// A coin or stock logo (`assets/coins/<coin>.svg`, `assets/stocks/<name>.svg`).
class KCoinIcon extends StatelessWidget {
  const KCoinIcon(this.coin, {super.key, this.size = 28, this.stock = false});
  final String coin;
  final double size;
  final bool stock;

  @override
  Widget build(BuildContext context) => ClipOval(
    child: SvgPicture.asset('assets/${stock ? 'stocks' : 'coins'}/${coin.toLowerCase()}.svg', width: size, height: size, semanticsLabel: coin.toUpperCase()),
  );
}

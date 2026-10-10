// Design tokens of "Kalks iOS", taken from the web's stylesheets so both look alike:
// - packages/ui/src/styles.css                 (the Kalks base palette, light and dark)
// - apps/crm/app/globals.css                   (Client Area: pastel light by default, washes mixed from the brand)
// - apps/terminal/app/globals.css              (Kalks Trader: dark by default, frosted panels)
// Every tint is mixed from the tenant brand colour (`ember`, Kalks orange #F2600C; config.tenant.primary for brokers)
// with the same color-mix(in oklab) maths as the web, so a blue brand gets blue-tinted pastels automatically.
import 'package:flutter/material.dart';

import 'color_mix.dart';

/// The Kalks brand defaults: Kalks orange and its amber (web apps/crm/app/kx-orange.css --k-red / --k-yellow).
const Color kEmber = Color(0xFFF2600C);
const Color kGold = Color(0xFFFFA040);

/// The lighter orange (web --k-neon-red / the hover face).
const Color kEmberLight = Color(0xFFFF7A2E);

/// The Vantablack canvas of the Client Area, the icon and the splash (founder 2026-10-10).
const Color kCanvas = Color(0xFF000000);

Color _rgba(int r, int g, int b, double a) => Color.fromRGBO(r, g, b, a);
Color _alpha(Color c, double a) => c.withValues(alpha: a);

/// Pastel icon-tile tones (web .k-tile-*).
enum KTone { accent, amber, coral, pink, lavender, mint, sky, neutral }

/// Chip and status tones (web Chip tones).
enum KChipTone { neutral, ember, up, down, warn, info, gold, solid }

/// The look of one surface family (Client Area or Kalks Trader) in one brightness.
@immutable
class KTokens extends ThemeExtension<KTokens> {
  const KTokens({
    required this.brightness,
    required this.trader,
    required this.bg,
    required this.surface,
    required this.surface2,
    required this.surface3,
    required this.line,
    required this.lineTop,
    required this.fg,
    required this.fg2,
    required this.fg3,
    required this.ember,
    required this.ember2,
    required this.emberSoft,
    required this.onEmber,
    required this.gold,
    required this.goldSoft,
    required this.up,
    required this.upSoft,
    required this.down,
    required this.downSoft,
    required this.warn,
    required this.warnSoft,
    required this.info,
    required this.infoSoft,
    required this.cardBg,
    required this.cardBorder,
    required this.ink,
    required this.inkFg,
    required this.wash1,
    required this.wash2,
    required this.wash3,
    required this.bar,
    required this.sheet,
    required this.scrim,
    required this.buyFill,
    required this.sellFill,
    required this.shadowCard,
    required this.shadowPop,
    required this.tiles,
    required this.cardRadius,
    required this.rowRadius,
  });

  final Brightness brightness;

  /// Kalks Trader tokens (terminal) rather than the Client Area's.
  final bool trader;

  final Color bg, surface, surface2, surface3, line, lineTop, fg, fg2, fg3;
  final Color ember, ember2, emberSoft, onEmber, gold, goldSoft;
  final Color up, upSoft, down, downSoft, warn, warnSoft, info, infoSoft;

  /// Card material (frosted white on the pastel washes; a little more opaque on touch devices, like the web).
  final Color cardBg, cardBorder;

  /// The near-black "ink" buttons (Deposit / Withdraw) and their text.
  final Color ink, inkFg;

  /// The three pastel washes behind every Client Area page.
  final Color wash1, wash2, wash3;

  /// Frosted materials: bars (~90 % opaque + blur) and sheets (~92 %), never see-through.
  final Color bar, sheet;

  /// Dim layer behind sheets and alerts.
  final Color scrim;

  /// Buy / Sell fills with white text (terminal, 5:1 contrast).
  final Color buyFill, sellFill;

  final List<BoxShadow> shadowCard, shadowPop;
  final Map<KTone, (Color bg, Color fg)> tiles;

  /// Web --radius-card / --radius-row (Client Area 24 / 16, terminal 14 / 10).
  final double cardRadius, rowRadius;

  bool get dark => brightness == Brightness.dark;

  (Color bg, Color fg) tile(KTone t) => tiles[t]!;

  /// Chip colours: (background, foreground, border).
  (Color, Color, Color) chip(KChipTone t) => switch (t) {
    KChipTone.neutral => (surface3, fg2, line),
    KChipTone.ember => (emberSoft, ember, _alpha(ember, 0.3)),
    KChipTone.up => (upSoft, up, _alpha(up, 0.25)),
    KChipTone.down => (downSoft, down, _alpha(down, 0.25)),
    KChipTone.warn => (warnSoft, warn, _alpha(warn, 0.25)),
    KChipTone.info => (infoSoft, info, _alpha(info, 0.25)),
    KChipTone.gold => (goldSoft, gold, _alpha(gold, 0.3)),
    KChipTone.solid => (fg, bg, Colors.transparent),
  };

  /* ---------------- Client Area (apps/crm/app/globals.css) ---------------- */

  factory KTokens.clientLight({Color ember = kEmber, Color gold = const Color(0xFFC9971F)}) {
    const surface = Color(0xFFFFFFFF);
    return KTokens(
      brightness: Brightness.light,
      trader: false,
      bg: mixOklab(ember, const Color(0xFFF9F6F5), 0.04),
      surface: surface,
      surface2: mixOklab(ember, const Color(0xFFF6F4F8), 0.03),
      surface3: mixOklab(ember, const Color(0xFFEEEBF1), 0.05),
      line: _rgba(36, 24, 52, 0.075),
      lineTop: _rgba(255, 255, 255, 0.9),
      fg: const Color(0xFF17151D),
      fg2: const Color(0xFF5F5B6B),
      fg3: const Color(0xFF9893A3),
      ember: ember,
      ember2: ember == kEmber ? const Color(0xFFFF8A3D) : mixOklab(ember, const Color(0xFFFFFFFF), 0.78),
      emberSoft: _alpha(ember, 0.12),
      onEmber: const Color(0xFFFFFFFF),
      gold: gold,
      goldSoft: _alpha(gold, 0.12),
      up: const Color(0xFF12A150),
      upSoft: _rgba(18, 161, 80, 0.11),
      down: const Color(0xFFE5484D),
      downSoft: _rgba(229, 72, 77, 0.11),
      warn: const Color(0xFFD98A00),
      warnSoft: _rgba(245, 158, 11, 0.13),
      info: const Color(0xFF2F7FD6),
      infoSoft: _rgba(47, 127, 214, 0.11),
      cardBg: _rgba(255, 255, 255, 0.92),
      cardBorder: _rgba(255, 255, 255, 0.75),
      ink: const Color(0xFF17151D),
      inkFg: const Color(0xFFFFFFFF),
      wash1: mixOklab(ember, const Color(0xFFFFF4EE), 0.30),
      wash2: mixOklab(ember, const Color(0xFFFFD9EA), 0.18),
      wash3: mixOklab(ember, const Color(0xFFE3DDFF), 0.09),
      bar: _alpha(surface, 0.9),
      sheet: _alpha(surface, 0.94),
      scrim: _rgba(42, 29, 58, 0.28),
      buyFill: const Color(0xFF15803D),
      sellFill: const Color(0xFFDC2626),
      shadowCard: [
        BoxShadow(color: _rgba(48, 28, 64, 0.035), offset: const Offset(0, 1), blurRadius: 2),
        BoxShadow(color: mixOklab(ember, _rgba(60, 34, 84, 0.34), 0.24), offset: const Offset(0, 18), blurRadius: 46, spreadRadius: -24),
      ],
      shadowPop: [
        BoxShadow(color: _rgba(48, 28, 64, 0.06), offset: const Offset(0, 2), blurRadius: 6),
        BoxShadow(color: _rgba(48, 28, 64, 0.32), offset: const Offset(0, 26), blurRadius: 60, spreadRadius: -24),
      ],
      tiles: {
        KTone.accent: (mixOklab(ember, surface, 0.14), ember),
        KTone.amber: (const Color(0xFFFFF1C9), const Color(0xFFC08600)),
        KTone.coral: (const Color(0xFFFFE2DA), const Color(0xFFE0533D)),
        KTone.pink: (const Color(0xFFFFDFF0), const Color(0xFFCF3F8A)),
        KTone.lavender: (const Color(0xFFEBE7FF), const Color(0xFF6A52DC)),
        KTone.mint: (const Color(0xFFD9F4E6), const Color(0xFF13935A)),
        KTone.sky: (const Color(0xFFDCEDFF), const Color(0xFF2B78D0)),
        KTone.neutral: (mixOklab(ember, const Color(0xFFEEEBF1), 0.05), const Color(0xFF5F5B6B)),
      },
      cardRadius: 24,
      rowRadius: 16,
    );
  }

  /// The Client Area (founder 2026-10-10, web apps/crm/app/kx-orange.css): dark only, Vantablack grounds, neutral
  /// greys, Kalks orange accents; rising / buy blue and falling / sell red, as in Kalks Trader. Cards are dark glass
  /// over the section's blurred photo (KBackdrop).
  factory KTokens.clientDark({Color ember = kEmber, Color gold = kGold}) {
    const surface = Color(0xFF0A0A0B);
    const surface3 = Color(0xFF161618);
    final stock = ember == kEmber;
    return KTokens(
      brightness: Brightness.dark,
      trader: false,
      bg: const Color(0xFF000000),
      surface: surface,
      surface2: const Color(0xFF0F0F11),
      surface3: surface3,
      line: _rgba(255, 255, 255, 0.075),
      lineTop: _rgba(255, 255, 255, 0.12),
      fg: const Color(0xFFF5F5F6),
      fg2: const Color(0xFFB3B3BA),
      fg3: const Color(0xFF8B8B93),
      ember: ember,
      ember2: stock ? const Color(0xFFFF8A4C) : mixOklab(ember, const Color(0xFFFFFFFF), 0.78),
      emberSoft: _alpha(ember, 0.18),
      onEmber: const Color(0xFFFFFFFF),
      gold: gold,
      goldSoft: _alpha(gold, 0.14),
      // blue = up / buy / profit, red = down / sell / loss (web tokens.ts: "everywhere, Client Area included")
      up: const Color(0xFF4C8DFF),
      upSoft: _rgba(47, 123, 255, 0.15),
      down: const Color(0xFFF5424F),
      downSoft: _rgba(242, 54, 69, 0.15),
      warn: const Color(0xFFFFB347),
      warnSoft: _rgba(255, 179, 71, 0.13),
      info: const Color(0xFF5B97FF),
      infoSoft: _rgba(47, 123, 255, 0.15),
      cardBg: _rgba(10, 10, 11, 0.66),
      cardBorder: _rgba(255, 255, 255, 0.08),
      ink: const Color(0xFFF5F5F6),
      inkFg: const Color(0xFF000000),
      // the blooms behind pages without a photo (web --k-bloom-a / -b)
      wash1: _alpha(ember, 0.22),
      wash2: _alpha(gold, 0.06),
      wash3: _rgba(0, 0, 0, 0),
      bar: _rgba(8, 8, 9, 0.8),
      sheet: _rgba(14, 14, 16, 0.97),
      scrim: _rgba(0, 0, 0, 0.6),
      buyFill: const Color(0xFF1F62EA),
      sellFill: const Color(0xFFE0182F),
      shadowCard: [BoxShadow(color: _rgba(0, 0, 0, 0.8), offset: const Offset(0, 24), blurRadius: 48, spreadRadius: -28)],
      shadowPop: [
        BoxShadow(color: _rgba(0, 0, 0, 0.3), offset: const Offset(0, 2), blurRadius: 6),
        BoxShadow(color: _rgba(0, 0, 0, 0.85), offset: const Offset(0, 30), blurRadius: 80, spreadRadius: -24),
      ],
      tiles: {
        KTone.accent: (_alpha(ember, 0.18), stock ? const Color(0xFFFF8A4C) : ember),
        KTone.amber: (_rgba(255, 160, 64, 0.15), const Color(0xFFFFB366)),
        KTone.coral: (_rgba(255, 120, 95, 0.15), const Color(0xFFFF9A85)),
        KTone.pink: (_rgba(240, 100, 170, 0.15), const Color(0xFFF58CC4)),
        KTone.lavender: (_rgba(140, 120, 255, 0.17), const Color(0xFFB3A6FF)),
        KTone.mint: (_rgba(40, 200, 120, 0.15), const Color(0xFF5FDC9C)),
        KTone.sky: (_rgba(80, 160, 255, 0.15), const Color(0xFF8CC2FF)),
        KTone.neutral: (surface3, const Color(0xFFB3B3BA)),
      },
      cardRadius: 24,
      rowRadius: 16,
    );
  }

  /* ---------------- Kalks Trader (apps/terminal/app/globals.css) ---------------- */

  factory KTokens.traderDark({Color ember = kEmber, Color gold = kGold}) {
    const panel = Color(0xFF0E0E12);
    const panel2 = Color(0xFF15151A);
    return KTokens(
      brightness: Brightness.dark,
      trader: true,
      bg: const Color(0xFF050507),
      surface: panel,
      surface2: panel2,
      surface3: const Color(0xFF1E1E24),
      line: _rgba(255, 255, 255, 0.07),
      lineTop: _rgba(255, 255, 255, 0.14),
      fg: const Color(0xFFF5F5F7),
      fg2: const Color(0xFFA1A1AA),
      fg3: const Color(0xFF8B8B96),
      ember: ember,
      ember2: ember == kEmber ? const Color(0xFFFF8A3D) : mixOklab(ember, const Color(0xFFFFFFFF), 0.78),
      emberSoft: _alpha(ember, 0.12),
      onEmber: const Color(0xFFFFFFFF),
      gold: gold,
      goldSoft: _alpha(gold, 0.12),
      // Kalks Trader: blue = up / buy / profit / TP, red = down / sell / loss / SL (web apps/terminal globals.css)
      up: const Color(0xFF2F7BFF),
      upSoft: _rgba(47, 123, 255, 0.12),
      down: const Color(0xFFF04438),
      downSoft: _rgba(240, 68, 56, 0.12),
      warn: const Color(0xFFF59E0B),
      warnSoft: _rgba(245, 158, 11, 0.12),
      info: const Color(0xFF38BDF8),
      infoSoft: _rgba(56, 189, 248, 0.12),
      cardBg: _alpha(panel, 0.9),
      cardBorder: _rgba(255, 255, 255, 0.07),
      ink: const Color(0xFFF5F5F7),
      inkFg: const Color(0xFF0E0E12),
      wash1: _rgba(255, 90, 31, 0.07),
      wash2: _rgba(120, 120, 160, 0.06),
      wash3: _rgba(0, 0, 0, 0),
      bar: _alpha(panel, 0.9),
      sheet: _alpha(panel2, 0.92),
      scrim: _rgba(0, 0, 0, 0.6),
      buyFill: const Color(0xFF1F5FE0),
      sellFill: const Color(0xFFDC2626),
      shadowCard: [BoxShadow(color: _rgba(0, 0, 0, 0.7), offset: const Offset(0, 12), blurRadius: 32, spreadRadius: -18)],
      shadowPop: [
        BoxShadow(color: _rgba(0, 0, 0, 0.55), offset: const Offset(0, 18), blurRadius: 48, spreadRadius: -12),
        BoxShadow(color: _rgba(0, 0, 0, 0.25), offset: const Offset(0, 2), blurRadius: 6),
      ],
      tiles: {
        KTone.accent: (_alpha(ember, 0.16), ember),
        KTone.amber: (_rgba(245, 190, 60, 0.15), const Color(0xFFF4C55A)),
        KTone.coral: (_rgba(255, 120, 95, 0.15), const Color(0xFFFF9A85)),
        KTone.pink: (_rgba(240, 100, 170, 0.15), const Color(0xFFF58CC4)),
        KTone.lavender: (_rgba(140, 120, 255, 0.17), const Color(0xFFB3A6FF)),
        KTone.mint: (_rgba(40, 200, 120, 0.15), const Color(0xFF5FDC9C)),
        KTone.sky: (_rgba(80, 160, 255, 0.15), const Color(0xFF8CC2FF)),
        KTone.neutral: (const Color(0xFF1E1E24), const Color(0xFFA1A1AA)),
      },
      cardRadius: 14,
      rowRadius: 10,
    );
  }

  factory KTokens.traderLight({Color ember = kEmber}) {
    const panel = Color(0xFFFFFFFF);
    const panel2 = Color(0xFFF5F3F0);
    return KTokens(
      brightness: Brightness.light,
      trader: true,
      bg: const Color(0xFFE7E5E1),
      surface: panel,
      surface2: panel2,
      surface3: const Color(0xFFF1EEE9),
      line: _rgba(15, 15, 20, 0.08),
      lineTop: _rgba(15, 15, 20, 0.1),
      fg: const Color(0xFF0E0E12),
      fg2: const Color(0xFF55555F),
      fg3: const Color(0xFF6B6B75),
      ember: ember,
      ember2: ember == kEmber ? const Color(0xFFFF8A3D) : mixOklab(ember, const Color(0xFFFFFFFF), 0.78),
      emberSoft: _alpha(ember, 0.12),
      onEmber: const Color(0xFFFFFFFF),
      gold: const Color(0xFFA16207),
      goldSoft: _rgba(201, 151, 31, 0.12),
      up: const Color(0xFF1F5FE0),
      upSoft: _rgba(31, 95, 224, 0.12),
      down: const Color(0xFFDC2626),
      downSoft: _rgba(240, 68, 56, 0.12),
      warn: const Color(0xFFB45309),
      warnSoft: _rgba(245, 158, 11, 0.12),
      info: const Color(0xFF0369A1),
      infoSoft: _rgba(56, 189, 248, 0.12),
      cardBg: _alpha(panel, 0.86),
      cardBorder: _rgba(255, 255, 255, 0.7),
      ink: const Color(0xFF0E0E12),
      inkFg: const Color(0xFFFFFFFF),
      wash1: _rgba(255, 90, 31, 0.08),
      wash2: _rgba(90, 110, 160, 0.08),
      wash3: _rgba(0, 0, 0, 0),
      bar: _alpha(panel, 0.86),
      sheet: _alpha(panel, 0.92),
      scrim: _rgba(15, 15, 20, 0.3),
      buyFill: const Color(0xFF1F5FE0),
      sellFill: const Color(0xFFDC2626),
      shadowCard: [BoxShadow(color: _rgba(20, 20, 30, 0.25), offset: const Offset(0, 10), blurRadius: 28, spreadRadius: -18)],
      shadowPop: [
        BoxShadow(color: _rgba(15, 15, 20, 0.22), offset: const Offset(0, 18), blurRadius: 48, spreadRadius: -14),
        BoxShadow(color: _rgba(15, 15, 20, 0.08), offset: const Offset(0, 2), blurRadius: 6),
      ],
      tiles: {
        KTone.accent: (mixOklab(ember, panel, 0.14), ember),
        KTone.amber: (const Color(0xFFFFF1C9), const Color(0xFFC08600)),
        KTone.coral: (const Color(0xFFFFE2DA), const Color(0xFFE0533D)),
        KTone.pink: (const Color(0xFFFFDFF0), const Color(0xFFCF3F8A)),
        KTone.lavender: (const Color(0xFFEBE7FF), const Color(0xFF6A52DC)),
        KTone.mint: (const Color(0xFFD9F4E6), const Color(0xFF13935A)),
        KTone.sky: (const Color(0xFFDCEDFF), const Color(0xFF2B78D0)),
        KTone.neutral: (const Color(0xFFF1EEE9), const Color(0xFF55555F)),
      },
      cardRadius: 14,
      rowRadius: 10,
    );
  }

  @override
  KTokens copyWith() => this;

  @override
  KTokens lerp(KTokens? other, double t) => other == null || t < 0.5 ? this : other;
}

/// Spacing (4-pt grid) and the compact size scale (founder rule: compact, one primary action, touch targets >= 44).
abstract final class KSpace {
  static const double xs = 4, sm = 8, md = 12, lg = 16, xl = 20, xxl = 24, page = 16;
}

abstract final class KSize {
  /// Minimum touch target on phones (visual controls may be smaller; their hit area is not).
  static const double touch = 44;

  /// Button heights: sm 32, md 40, lg 44 (the full-width primary action). Nothing taller.
  static const double buttonSm = 32, buttonMd = 40, buttonLg = 44;

  /// Icon buttons in bars.
  static const double iconButton = 40;

  /// Text fields (web Input h-11).
  static const double field = 44;

  /// List rows (iOS 44, with a subtitle 60).
  static const double row = 48;

  /// The floating tab bar (the ink pill with the five icons; the web's k-mobilebar is 68).
  static const double tabBar = 64;

  /// The header (web h-[68px]) and the module sub-page tabs under it (h-11).
  static const double header = 60, subNav = 40;
}

extension KTokensContext on BuildContext {
  /// The design tokens of the surrounding theme (Client Area or Kalks Trader).
  KTokens get k => Theme.of(this).extension<KTokens>()!;
}

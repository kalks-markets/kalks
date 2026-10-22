// ThemeData built from the Kalks tokens. Material widgets are used only as plumbing (Scaffold, text fields, ink);
// the visible controls come from lib/ui/components and follow iOS: Cupertino page transitions and bouncing scroll
// on Android too, no Material ripples or glow.
import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'tokens.dart';
import 'typography.dart';

/// The tenant's brand colours (config.tenant.primary / accent), defaulting to Kalks ember and gold.
@immutable
class KBrand {
  const KBrand({this.primary = kEmber, this.accent = kGold});
  final Color primary;
  final Color accent;

  @override
  bool operator ==(Object other) => other is KBrand && other.primary == primary && other.accent == accent;

  @override
  int get hashCode => Object.hash(primary, accent);
}

abstract final class KTheme {
  static ThemeData client(Brightness b, {KBrand brand = const KBrand()}) =>
      _build(b == Brightness.light ? KTokens.clientLight(ember: brand.primary) : KTokens.clientDark(ember: brand.primary, gold: brand.accent));

  /// The light Client Area look in this context's brand, for controls floating over a picture (white discs, a white
  /// pill) that stay white in dark mode too.
  static ThemeData lightOf(BuildContext context) {
    final k = Theme.of(context).extension<KTokens>()!;
    return client(
      Brightness.light,
      brand: KBrand(primary: k.ember, accent: k.gold),
    );
  }

  /// Kalks Trader; `green`: the Green / red colour scheme (up / buy / profit / TP green instead of blue).
  static ThemeData trader(Brightness b, {KBrand brand = const KBrand(), bool green = false}) => _build(
    b == Brightness.light ? KTokens.traderLight(ember: brand.primary, green: green) : KTokens.traderDark(ember: brand.primary, gold: brand.accent, green: green),
  );

  static ThemeData _build(KTokens k) {
    final family = k.trader ? KFonts.sans : KFonts.display;
    final text = KText(family, k.fg);
    final scheme = ColorScheme(
      brightness: k.brightness,
      primary: k.ember,
      onPrimary: k.onEmber,
      secondary: k.gold,
      onSecondary: k.inkFg,
      error: k.down,
      onError: Colors.white,
      surface: k.surface,
      onSurface: k.fg,
      onSurfaceVariant: k.fg2,
      outline: k.line,
      outlineVariant: k.line,
      surfaceContainerHighest: k.surface3,
      surfaceContainerHigh: k.surface2,
      surfaceContainer: k.surface2,
      surfaceContainerLow: k.surface,
      surfaceContainerLowest: k.surface,
      shadow: Colors.black,
      scrim: k.scrim,
      inverseSurface: k.ink,
      onInverseSurface: k.inkFg,
    );
    const ios = CupertinoPageTransitionsBuilder();
    return ThemeData(
      useMaterial3: true,
      brightness: k.brightness,
      colorScheme: scheme,
      fontFamily: family,
      fontFamilyFallback: KFonts.fallback,
      scaffoldBackgroundColor: k.bg,
      canvasColor: k.bg,
      dividerColor: k.line,
      splashFactory: NoSplash.splashFactory,
      highlightColor: Colors.transparent,
      splashColor: Colors.transparent,
      hoverColor: Colors.transparent,
      materialTapTargetSize: MaterialTapTargetSize.padded,
      visualDensity: VisualDensity.standard,
      platform: TargetPlatform.android,
      pageTransitionsTheme: const PageTransitionsTheme(
        builders: {TargetPlatform.android: ios, TargetPlatform.iOS: ios, TargetPlatform.linux: ios, TargetPlatform.macOS: ios, TargetPlatform.windows: ios},
      ),
      textTheme: TextTheme(
        displayLarge: text.moneyXL,
        headlineMedium: text.largeTitle,
        titleLarge: text.title1,
        titleMedium: text.title2,
        titleSmall: text.headline,
        bodyLarge: text.body,
        bodyMedium: text.callout,
        bodySmall: text.footnote,
        labelLarge: text.headline,
        labelMedium: text.label,
        labelSmall: text.caption,
      ),
      textSelectionTheme: TextSelectionThemeData(cursorColor: k.ember, selectionColor: k.emberSoft, selectionHandleColor: k.ember),
      iconTheme: IconThemeData(color: k.fg2, size: 20),
      dividerTheme: DividerThemeData(color: k.line, thickness: 0.5, space: 0.5),
      progressIndicatorTheme: ProgressIndicatorThemeData(color: k.ember),
      cupertinoOverrideTheme: CupertinoThemeData(
        brightness: k.brightness,
        primaryColor: k.ember,
        scaffoldBackgroundColor: k.bg,
        barBackgroundColor: k.bar,
        textTheme: CupertinoTextThemeData(primaryColor: k.ember, textStyle: text.body),
      ),
      extensions: [k],
    );
  }

  /// Status and navigation bars: transparent (edge to edge), icons readable on the theme.
  static SystemUiOverlayStyle overlay(Brightness b) => SystemUiOverlayStyle(
    statusBarColor: Colors.transparent,
    systemNavigationBarColor: Colors.transparent,
    systemNavigationBarContrastEnforced: false,
    statusBarIconBrightness: b == Brightness.light ? Brightness.dark : Brightness.light,
    statusBarBrightness: b,
    systemNavigationBarIconBrightness: b == Brightness.light ? Brightness.dark : Brightness.light,
  );
}

/// iOS scrolling everywhere: bounce, no Android glow, scrollbars only while scrolling.
class KScrollBehavior extends MaterialScrollBehavior {
  const KScrollBehavior();

  @override
  ScrollPhysics getScrollPhysics(BuildContext context) => const BouncingScrollPhysics(parent: AlwaysScrollableScrollPhysics());

  @override
  Widget buildOverscrollIndicator(BuildContext context, Widget child, ScrollableDetails details) => child;

  @override
  Set<PointerDeviceKind> get dragDevices => {PointerDeviceKind.touch, PointerDeviceKind.mouse, PointerDeviceKind.stylus, PointerDeviceKind.trackpad};
}

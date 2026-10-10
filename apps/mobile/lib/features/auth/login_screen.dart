// The signed-out welcome page (/login), in the Client Area's look since 2026-10-10 (web apps/crm/app/(auth): the
// photo heroes, dark, orange buttons): Home's photo full-bleed at the top (the KALKS letters behind the figure),
// fading into black, the white Kalks logo and the language over it; then, on black, the headline, the legal line, the
// orange Log in (opens the sign-in sheet), Open account (sign-up) and "Try the demo". A white-label broker gets the
// same page on its own colour with its name as the headline, without the photo or the demo. The route keeps its
// `next` (the router sends a fresh session there).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../../core/auth/auth_api.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import '../common/pickers.dart';
import 'sign_in_sheet.dart';

/// The sign-in pages' photo (web AuthSection AUTH_PHOTO: Home's).
const KHeroPhoto kAuthPhoto = KHeroPhoto.dashboard;

/// The photo's own size (assets/heroes/dashboard.jpg), to lay it full width.
const double _photoAspect = 1080 / 1240;

class LoginScreen extends ConsumerStatefulWidget {
  const LoginScreen({super.key});

  @override
  ConsumerState<LoginScreen> createState() => _LoginScreenState();
}

class _LoginScreenState extends ConsumerState<LoginScreen> {
  bool _sheetOpen = false;

  @override
  void initState() {
    super.initState();
    // a dead session lands here: open the form at once, with its "Signed out" notice
    WidgetsBinding.instance.addPostFrameCallback((_) {
      final auth = ref.read(authProvider);
      if (mounted && auth is AuthSignedOut && auth.reason == 'expired') _openSignIn();
    });
  }

  Future<void> _openSignIn() async {
    if (_sheetOpen) return;
    _sheetOpen = true;
    final r = await showSignInSheet(context);
    _sheetOpen = false;
    if (!mounted) return;
    if (r is SignedIn) {
      KHaptics.success();
      await ref.read(authProvider.notifier).completeSignIn(r);
      // the router takes the client to the Client Area (or the page they came from, ?next=)
    } else if (r == SignInExit.forgot) {
      unawaited(context.push('/forgot'));
    } else if (r == SignInExit.register) {
      context.go('/register');
    }
  }

  void _tryDemo() {
    KHaptics.success();
    unawaited(ref.read(authProvider.notifier).enterDemo());
    // the router takes the demo client to the Dashboard
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final cfg = ref.watch(configProvider);
    final kalks = cfg.tenantDefault;
    final mq = MediaQuery.of(context);
    final w = mq.size.width;
    // a white-label broker: its colour, which may be light (then dark texts and pills)
    final bg = kalks ? Colors.black : k.ember;
    final onDark = kalks || ThemeData.estimateBrightnessForColor(bg) == Brightness.dark;
    final ink = onDark ? Colors.white : const Color(0xFF0C0C0F);
    final demo = kalks && t.has('auth.demo.tryCta');
    const white = Colors.white;

    return Scaffold(
      backgroundColor: bg,
      resizeToAvoidBottomInset: false,
      body: AnnotatedRegion<SystemUiOverlayStyle>(
        value: KTheme.overlay(onDark ? Brightness.dark : Brightness.light),
        child: Stack(
          fit: StackFit.expand,
          children: [
            if (kalks) ...[
              // the photo full width from the very top; its foot is already black
              Positioned(
                top: 0,
                left: 0,
                right: 0,
                height: w / _photoAspect,
                child: Image.asset(kAuthPhoto.asset, fit: BoxFit.cover, alignment: Alignment.topCenter, excludeFromSemantics: true),
              ),
              const Positioned(
                top: 0,
                left: 0,
                right: 0,
                height: 140,
                child: IgnorePointer(
                  child: DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: LinearGradient(begin: Alignment.topCenter, end: Alignment.bottomCenter, colors: [Color(0x99000000), Color(0x00000000)]),
                    ),
                  ),
                ),
              ),
            ],
            // the logo and the language over the photo
            Positioned(
              top: mq.padding.top + 10,
              left: 20,
              right: 12,
              child: Row(
                children: [
                  if (kalks) const KLogo(color: white),
                  const Spacer(),
                  LanguageButton(glass: onDark),
                ],
              ),
            ),
            // the headline in the space between the photo and the actions; the legal line and the buttons at the foot
            Positioned.fill(
              top: kalks ? (w / _photoAspect * 0.66).clamp(0.0, mq.size.height * 0.42) : mq.padding.top + 64,
              child: LayoutBuilder(
                builder: (context, c) => SingleChildScrollView(
                  reverse: true,
                  padding: EdgeInsets.fromLTRB(24, 0, 24, mq.padding.bottom + 16),
                  child: ConstrainedBox(
                    constraints: BoxConstraints(minHeight: c.maxHeight - mq.padding.bottom - 16),
                    child: IntrinsicHeight(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          const Spacer(),
                          Text(
                            kalks ? t('app.welcome.title') : cfg.tenantName,
                            key: const ValueKey('welcome-title'),
                            textAlign: TextAlign.center,
                            style: context.text.largeTitle.copyWith(fontSize: 36, fontWeight: FontWeight.w800, height: 1.02, letterSpacing: -1.1, color: ink),
                          ),
                          if (kalks) ...[
                            const SizedBox(height: 10),
                            Text(
                              t('auth.brand.body'),
                              textAlign: TextAlign.center,
                              maxLines: 3,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.callout.copyWith(color: white.withValues(alpha: 0.66), height: 1.45),
                            ),
                          ],

                          const Spacer(),
                          const SizedBox(height: 22),
                          KRichText(
                            t('auth.register.terms'),
                            textAlign: TextAlign.center,
                            style: context.text.footnote.copyWith(color: ink.withValues(alpha: 0.62), height: 1.4),
                            tags: {
                              'agreement': KTag(
                                style: TextStyle(color: ink, fontWeight: FontWeight.w600),
                              ),
                              'risk': KTag(
                                style: TextStyle(color: ink, fontWeight: FontWeight.w600),
                              ),
                              'privacy': KTag(
                                style: TextStyle(color: ink, fontWeight: FontWeight.w600),
                              ),
                            },
                          ),
                          const SizedBox(height: 16),
                          _Pill(
                            label: t('trader.guest.logIn'),
                            bg: kalks ? k.ember : ink,
                            fg: kalks ? k.onEmber : bg,
                            glow: kalks ? k.ember : null,
                            onTap: _openSignIn,
                          ),
                          const SizedBox(height: 12),
                          _Pill(
                            label: t('trader.guest.openAccount'),
                            bg: kalks ? white.withValues(alpha: 0.1) : ink,
                            fg: kalks ? white : bg,
                            border: kalks ? white.withValues(alpha: 0.22) : null,
                            onTap: () => context.go('/register'),
                          ),
                          if (demo) ...[
                            const SizedBox(height: 6),
                            KPressable(
                              onTap: _tryDemo,
                              semanticLabel: t('auth.demo.tryCta'),
                              child: SizedBox(
                                height: 44,
                                child: Center(
                                  child: Text(
                                    t('auth.demo.tryCta'),
                                    style: context.text.headline.copyWith(fontSize: 15, fontWeight: FontWeight.w600, color: k.ember2),
                                  ),
                                ),
                              ),
                            ),
                          ],
                        ],
                      ),
                    ),
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// The welcome page's 52 pt pill (taller than the app's compact buttons: the page has nothing else to press).
class _Pill extends StatelessWidget {
  const _Pill({required this.label, required this.bg, required this.fg, required this.onTap, this.border, this.glow});
  final String label;
  final Color bg, fg;
  final Color? border, glow;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => KPressable(
    onTap: onTap,
    semanticLabel: label,
    child: Container(
      height: 52,
      alignment: Alignment.center,
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(26),
        border: Border.all(color: border ?? fg.withValues(alpha: 0.12)),
        boxShadow: glow == null ? null : [BoxShadow(color: glow!.withValues(alpha: 0.55), offset: const Offset(0, 14), blurRadius: 34, spreadRadius: -14)],
      ),
      child: Text(
        label,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: context.text.headline.copyWith(fontSize: 16, color: fg),
      ),
    ),
  );
}

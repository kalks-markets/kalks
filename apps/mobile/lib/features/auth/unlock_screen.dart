// Biometric unlock of a stored session (cold start, or back from the background after a minute). The system
// prompt opens at once; "Sign in with password" forgets the stored session. In the sign-in pages' look (2026-10-10):
// Home's photo at the top fading into black, the brand disc, the orange Unlock.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';

class UnlockScreen extends ConsumerStatefulWidget {
  const UnlockScreen({super.key});

  @override
  ConsumerState<UnlockScreen> createState() => _UnlockScreenState();
}

class _UnlockScreenState extends ConsumerState<UnlockScreen> {
  bool _busy = false;
  bool _failed = false;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _unlock());
  }

  Future<void> _unlock() async {
    if (_busy || !mounted) return;
    setState(() {
      _busy = true;
      _failed = false;
    });
    final ok = await ref.read(authProvider.notifier).unlock(context.t('app.unlock.reason'));
    if (!mounted) return;
    if (ok) KHaptics.success();
    setState(() {
      _busy = false;
      _failed = !ok;
    });
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final user = ref.watch(authProvider).user;
    final kalks = ref.watch(configProvider.select((c) => c.tenantDefault));
    final w = MediaQuery.sizeOf(context).width;
    return Scaffold(
      backgroundColor: Colors.black,
      body: Stack(
        children: [
          if (kalks)
            Positioned(
              top: 0,
              left: 0,
              right: 0,
              height: w * 1240 / 1080,
              child: Stack(
                fit: StackFit.expand,
                children: [
                  Image.asset(KHeroPhoto.dashboard.asset, fit: BoxFit.cover, alignment: Alignment.topCenter, excludeFromSemantics: true),
                  const DecoratedBox(
                    decoration: BoxDecoration(
                      gradient: LinearGradient(
                        begin: Alignment.topCenter,
                        end: Alignment.bottomCenter,
                        colors: [Color(0x80000000), Color(0x00000000), Color(0x00000000), Color(0xFF000000)],
                        stops: [0, 0.2, 0.55, 0.9],
                      ),
                    ),
                  ),
                ],
              ),
            )
          else
            const Positioned.fill(child: KBackdrop()),
          SafeArea(
            child: Padding(
              padding: const EdgeInsets.symmetric(horizontal: 28),
              child: Column(
                children: [
                  Spacer(flex: kalks ? 6 : 3),
                  const KBrandAvatar(size: 72),
                  const SizedBox(height: 22),
                  Text(t('app.unlock.title'), textAlign: TextAlign.center, style: context.text.largeTitle.copyWith(fontSize: 26)),
                  if (user != null) ...[
                    const SizedBox(height: 6),
                    Text(
                      user.name,
                      textAlign: TextAlign.center,
                      style: context.text.headline.copyWith(color: k.fg2),
                    ),
                  ],
                  const SizedBox(height: 10),
                  Text(
                    t('app.unlock.text'),
                    textAlign: TextAlign.center,
                    style: context.text.callout.copyWith(color: k.fg3),
                  ),
                  if (_failed) ...[const SizedBox(height: 16), KFormError(t('app.unlock.failed'))],
                  const Spacer(flex: 4),
                  KButton(label: t('app.unlock.button'), icon: LucideIcons.fingerprint, size: KButtonSize.lg, expand: true, loading: _busy, onPressed: _unlock),
                  const SizedBox(height: 8),
                  KTextButton(label: t('app.unlock.usePassword'), color: k.fg2, onPressed: () => ref.read(authProvider.notifier).forgetLocked()),
                  const SizedBox(height: 16),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

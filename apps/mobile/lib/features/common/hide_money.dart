// "Hide amounts" (web components/hide-money.tsx): one eye hides the balance, equity, wallet, P&L and every account
// card at once, on every page, and stays hidden on this device until shown again.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/prefs.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';

/// Whether amounts are hidden, remembered on the device.
class HideBalances extends Notifier<bool> {
  @override
  bool build() => ref.read(prefsProvider).hideBalances;
  void toggle() {
    state = !state;
    unawaited(ref.read(prefsProvider).setHideBalances(state));
  }
}

final hideBalancesProvider = NotifierProvider<HideBalances, bool>(HideBalances.new);

/// What a hidden amount shows (web MASK).
const String kMoneyMask = '••••••';

/// The eye (web HideMoneyButton).
class HideMoneyButton extends ConsumerWidget {
  const HideMoneyButton({super.key, this.size = 34, this.color});
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final hidden = ref.watch(hideBalancesProvider);
    return KIconButton(
      key: const ValueKey('hide-money'),
      icon: hidden ? LucideIcons.eyeOff : LucideIcons.eye,
      size: size,
      iconSize: 17,
      color: color ?? context.k.fg3,
      semanticLabel: hidden ? t('dashboard.home.showBalances') : t('dashboard.home.hideBalances'),
      onPressed: () {
        KHaptics.selection();
        ref.read(hideBalancesProvider.notifier).toggle();
      },
    );
  }
}

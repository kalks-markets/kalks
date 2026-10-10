// Which pages open on their section's photo, and the hero each of them gets (web components/page-hero.tsx): every
// section's first page — Home, Accounts, Wallet, Portfolio, Copy & PAMM, Prop, Partner, Rewards, Academy, Profile,
// Options, Developer — runs its photo under the status bar with the page title on it; the page itself lies in a sheet
// over it (KPageScroll.hero) with the section's pages as pills at its top, and the shell floats its controls over the
// photo until the sheet reaches the header (app_shell.dart). Stock Kalks brand only: a white-label broker never sees
// Kalks imagery.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';

import '../core/auth/auth_controller.dart';
import '../core/config/app_config.dart';
import '../i18n/i18n.dart';
import '../ui/ui.dart';
import 'nav.dart';

/// The hero pages and their photos.
const Map<String, KHeroPhoto> kHeroPages = {
  '/': KHeroPhoto.dashboard,
  '/accounts': KHeroPhoto.accounts,
  '/wallet': KHeroPhoto.wallet,
  '/portfolio': KHeroPhoto.portfolio,
  '/social': KHeroPhoto.copy,
  '/prop': KHeroPhoto.prop,
  '/partner': KHeroPhoto.partner,
  '/rewards': KHeroPhoto.rewards,
  '/academy': KHeroPhoto.academy,
  '/profile': KHeroPhoto.profile,
  '/options': KHeroPhoto.options,
  '/developer': KHeroPhoto.options,
};

/// Whether the page at `path` opens on its photo. The shell (its chrome and the page's top padding), the module pager
/// and the page itself decide by this one rule.
bool pageHeroAt(String path, AppConfig cfg) => cfg.tenantDefault && kHeroPages.containsKey(path);

/// The photo's height: 46 % of the screen, 300–440 px. The shell's collapse point and the page share it.
double pageHeroHeight(MediaQueryData mq) => (mq.size.height * 0.46).clamp(300.0, 440.0);

/// How far the sheet reaches up over the photo (its rounded corners show the photo).
const double kPageHeroOverlap = 28;

/// The page backdrop's photo for `path` (every page of a section shows its photo blurred); none for other brokers.
KHeroPhoto? backdropPhotoAt(String path, AppConfig cfg) => cfg.tenantDefault ? heroPhotoFor(path) : null;

/// The hero of the page at `path`, or null where there is none (another broker, a page that isn't a section's first):
/// the section's photo with the title, the short line, the buttons and the body on it, and the section's pages as
/// pills at the top of the sheet.
KPageHero? pageHero(BuildContext context, WidgetRef ref, {required String path, String? title, String? lead, List<Widget> actions = const [], Widget? body}) {
  final cfg = ref.watch(configProvider);
  final photo = kHeroPages[path];
  if (photo == null || !cfg.tenantDefault) return null;
  final t = context.t;
  final subs = moduleOf(navFor(cfg, ref.watch(meProvider)), path)?.sub ?? const <NavSub>[];
  return KPageHero(
    height: pageHeroHeight(MediaQuery.of(context)),
    picture: KHeroPicture(photo),
    child: KHeroCopy(title: title, lead: lead, actions: actions, body: body),
    top: subs.length > 1
        ? KPillNav(
            labels: [for (final s in subs) t(s.labelKey)],
            icons: [for (final s in subs) s.icon],
            current: subs.indexOf(activeSub(path, subs) ?? subs.first),
            onSelect: (i) => context.go(subs[i].href),
          )
        : null,
  );
}

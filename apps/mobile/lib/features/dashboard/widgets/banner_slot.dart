// Targeted marketing banners from the growth service (web components/growth/banner-slot.tsx, placement "dashboard",
// one at most): nothing when the service is unavailable or no banner targets this client. Impressions are counted
// once per banner, clicks and dismissals too.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';
import 'package:url_launcher/url_launcher.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/config/app_config.dart';
import '../../../i18n/i18n.dart';
import '../../../shell/nav.dart';
import '../../../ui/ui.dart';
import '../dashboard_data.dart';

class DashboardBannerSlot extends ConsumerStatefulWidget {
  const DashboardBannerSlot({super.key, this.max = 1});
  final int max;

  @override
  ConsumerState<DashboardBannerSlot> createState() => _DashboardBannerSlotState();
}

class _DashboardBannerSlotState extends ConsumerState<DashboardBannerSlot> {
  final Set<String> _hidden = {};
  final Set<String> _seen = {};

  @override
  Widget build(BuildContext context) {
    final cfg = ref.watch(configProvider);
    // a banner whose button leads into a module the broker switched off stays out
    bool leadsOn(DashBanner b) {
      final url = b.ctaUrl;
      return url == null || !url.startsWith('/') || pageOn(cfg, Uri.tryParse(url)?.path ?? url);
    }

    final items = (ref.watch(dashBannersProvider).value ?? const <DashBanner>[]).where((b) => !_hidden.contains(b.id) && leadsOn(b)).take(widget.max).toList();
    if (items.isEmpty) return const SizedBox.shrink();
    final api = ref.read(apiProvider);
    for (final b in items) {
      if (_seen.add(b.id)) trackBanner(api, b.id, 'impression');
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: 16),
      child: Column(
        children: [
          for (final b in items)
            _BannerCard(
              b: b,
              onCta: () => trackBanner(api, b.id, 'click'),
              onDismiss: () {
                trackBanner(api, b.id, 'dismiss');
                setState(() => _hidden.add(b.id));
              },
            ),
        ],
      ),
    );
  }
}

class _BannerCard extends StatelessWidget {
  const _BannerCard({required this.b, required this.onCta, required this.onDismiss});
  final DashBanner b;
  final VoidCallback onCta, onDismiss;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final t = context.t;
    final bar = switch (b.tone) {
      'ember' => k.ember,
      'gold' => k.gold,
      'up' => k.up,
      _ => k.fg3,
    };
    final external = b.ctaUrl != null && RegExp(r'^https?://', caseSensitive: false).hasMatch(b.ctaUrl!);
    final rtl = Directionality.of(context) == TextDirection.rtl;
    return KCard(
      padding: EdgeInsets.zero,
      child: IntrinsicHeight(
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            Container(width: 4, color: bar),
            Expanded(
              child: Stack(
                children: [
                  Padding(
                    padding: EdgeInsetsDirectional.fromSTEB(20, 16, b.dismissible ? 40 : 20, 16),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(b.title, style: context.text.headline.copyWith(fontWeight: FontWeight.w500)),
                        if (b.body.isNotEmpty) ...[const SizedBox(height: 2), Text(b.body, style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13))],
                        if (b.ctaUrl != null && b.ctaLabel != null) ...[
                          const SizedBox(height: 14),
                          KButton(
                            label: b.ctaLabel!,
                            trailingIcon: rtl ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight,
                            size: KButtonSize.sm,
                            variant: b.tone == 'ember' ? KButtonVariant.ember : KButtonVariant.surface,
                            onPressed: () {
                              onCta();
                              if (external) {
                                launchUrl(Uri.parse(b.ctaUrl!), mode: LaunchMode.externalApplication);
                              } else {
                                context.go(b.ctaUrl!);
                              }
                            },
                          ),
                        ],
                      ],
                    ),
                  ),
                  if (b.dismissible)
                    PositionedDirectional(
                      top: 2,
                      end: 2,
                      child: KIconButton(icon: LucideIcons.x, size: 32, semanticLabel: t('rewards.banner.dismiss'), onPressed: onDismiss),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

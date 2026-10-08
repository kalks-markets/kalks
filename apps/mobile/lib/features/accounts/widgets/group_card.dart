// An account type (engine group) as a card with its photo and commercial terms (web components/trading/group-card.tsx
// EngineGroupCard): the Account types list and the open-account wizard's Type step.
import 'package:flutter/material.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/format/format.dart';
import '../../../core/models/trading.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../accounts_data.dart';

const Map<String, String> _photo = {
  'standard': 'finance',
  'pro': 'trading-screen',
  'pro-netting': 'charts',
  'ecn': 'analytics',
  'cent': 'money',
  'vip': 'skyscrapers',
  'prop': 'trader',
  'options-standard': 'charts',
  'options-pro': 'analytics',
};

/// The group's photo (web groupPhoto).
String groupPhoto(String code) => 'assets/photos/${_photo[code] ?? 'stock-market'}.jpg';

/// Pricing model from the group's commercial terms (web spreadType).
String spreadType(EngineGroup g, T t) => g.commissionPerLot > 0 ? t('accounts.pricing.rawPlusCommission') : t('accounts.pricing.allIn');

/// "$3.5 / lot" or "None" (web commissionText).
String commissionText(EngineGroup g, T t) {
  if (g.commissionPerLot <= 0) return t('common.none');
  final s = Fmt.number(g.commissionPerLot).replaceAll(RegExp(r'\.00$'), '');
  return t('accounts.unit.perLot', {'amount': '\$$s'});
}

/// The highest leverage the group allows (web maxLeverage).
int maxLeverage(EngineGroup g) => g.leverages.isEmpty ? 0 : g.leverages.reduce((a, b) => a > b ? a : b);

/// The group's selling points (web groupFeatures).
List<String> groupFeatures(EngineGroup g, T t) => [
  spreadType(g, t),
  g.mode == 'hedging' ? t('accounts.feature.hedging') : t('accounts.feature.netting'),
  t('accounts.feature.marginCall', {'marginCall': _pct(g.marginCallPct), 'stopOut': _pct(g.stopOutPct)}),
  g.cent ? t('accounts.feature.cent') : (g.swapFree ? t('accounts.feature.swapFree') : t('accounts.feature.nbp')),
];

String _pct(double v) => v % 1 == 0 ? '${v.toInt()}' : '$v';

/// "50%" (a group's margin percentages).
String pctLabel(double v) => '${_pct(v)}%';

class EngineGroupCard extends StatelessWidget {
  const EngineGroupCard({super.key, required this.group, this.selected = false, this.onSelect, this.used, this.kind, this.compact = false});
  final EngineGroup group;
  final bool selected;
  final VoidCallback? onSelect;

  /// Accounts of this kind the client already holds in the group (the limit chip).
  final int? used;

  /// live | demo
  final String? kind;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final g = group;
    final full = used != null && used! >= g.maxAccountsPerUser;
    const white = Colors.white;
    final header = SizedBox(
      height: compact ? 80 : 112,
      child: Stack(
        fit: StackFit.expand,
        children: [
          Image.asset(
            groupPhoto(g.code),
            fit: BoxFit.cover,
            excludeFromSemantics: true,
            errorBuilder: (_, _, _) => ColoredBox(color: k.surface3),
          ),
          DecoratedBox(
            decoration: BoxDecoration(
              gradient: LinearGradient(
                begin: Alignment.bottomCenter,
                end: Alignment.topCenter,
                colors: [k.surface, Colors.black.withValues(alpha: 0.4), Colors.black.withValues(alpha: 0.1)],
                stops: const [0, 0.5, 1],
              ),
            ),
          ),
          PositionedDirectional(
            start: 16,
            end: 16,
            bottom: 12,
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Text(
                        g.name,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.title1.copyWith(
                          color: white,
                          fontSize: 19,
                          shadows: [Shadow(color: Colors.black.withValues(alpha: 0.35), blurRadius: 8)],
                        ),
                      ),
                      Text.rich(
                        TextSpan(
                          text: t.dyn('accounts.mode.${g.mode}', fallback: modeLabel(g.mode)).toUpperCase(),
                          children: [
                            if (g.cent)
                              TextSpan(
                                text: ' · ${t('accounts.groupCard.uscCentAlt')}',
                                style: TextStyle(color: k.gold),
                              ),
                            // an Options account type
                            if (g.isOptions) TextSpan(text: ' · ${t('accounts.product.chipOptions')}'),
                          ],
                        ),
                        style: context.text.micro.copyWith(color: white.withValues(alpha: 0.8), letterSpacing: 0.8),
                      ),
                    ],
                  ),
                ),
                if (selected)
                  Container(
                    width: 28,
                    height: 28,
                    decoration: BoxDecoration(color: k.ember, shape: BoxShape.circle),
                    child: Icon(LucideIcons.check, size: 16, color: k.onEmber),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
    final body = Padding(
      padding: const EdgeInsets.fromLTRB(16, 12, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(spreadType(g, t), style: context.text.footnote.copyWith(color: k.fg2)),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: _Spec(label: t('accounts.label.commission'), value: commissionText(g, t)),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: _Spec(label: t('accounts.label.minDeposit'), value: g.minDeposit > 0 ? '\$${Fmt.number(g.minDeposit, 0)}' : t('common.none')),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              Expanded(
                child: _Spec(label: t('accounts.label.maxLeverage'), value: levLabel(maxLeverage(g))),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: _Spec(label: t('accounts.label.stopOut'), value: pctLabel(g.stopOutPct)),
              ),
            ],
          ),
          if (!compact) ...[
            const SizedBox(height: 12),
            for (final f in groupFeatures(g, t).skip(1))
              Padding(
                padding: const EdgeInsets.only(bottom: 6),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Padding(
                      padding: const EdgeInsets.only(top: 2),
                      child: Icon(LucideIcons.check, size: 12, color: k.gold),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: Text(f, style: context.text.footnote.copyWith(color: k.fg2, fontSize: 12)),
                    ),
                  ],
                ),
              ),
          ],
          if (used != null) ...[
            const SizedBox(height: 8),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: KChip(
                small: true,
                tone: full ? KChipTone.warn : KChipTone.neutral,
                label: full
                    ? t.dyn('accounts.groupCard.limitReached.${kind ?? 'any'}', vars: {'max': g.maxAccountsPerUser})
                    : t.dyn('accounts.groupCard.used.${kind ?? 'any'}', vars: {'used': used, 'max': g.maxAccountsPerUser}),
              ),
            ),
          ],
        ],
      ),
    );
    final card = AnimatedContainer(
      duration: const Duration(milliseconds: 180),
      decoration: BoxDecoration(
        color: selected ? k.surface : k.cardBg,
        borderRadius: BorderRadius.circular(20),
        border: Border.all(color: selected ? k.ember.withValues(alpha: 0.6) : k.cardBorder),
        boxShadow: selected ? [BoxShadow(color: k.ember.withValues(alpha: 0.12), spreadRadius: 4)] : k.shadowCard,
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(20),
        child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [header, body]),
      ),
    );
    return Opacity(
      opacity: full ? 0.55 : 1,
      child: onSelect == null ? card : KPressable(onTap: full ? null : onSelect, pressedScale: 0.99, semanticLabel: g.name, child: card),
    );
  }
}

class _Spec extends StatelessWidget {
  const _Spec({required this.label, required this.value});
  final String label, value;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 2),
          Text(
            value,
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600, fontFeatures: kTabular),
          ),
        ],
      ),
    );
  }
}

// Prop › Challenges (/prop): the port of LivePropStore (apps/crm/components/prop-live/catalogue.tsx) in the phone
// order: header (+ My challenges with the count of running challenges), the configurator (model, plan, account size,
// your path, the four key rules), the plan card with the price and Buy, Compare the rules, How the rules are
// enforced; Buy opens the checkout sheet (pay from the USDT wallet).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/auth/auth_controller.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'prop_api.dart';
import 'widgets/checkout_sheet.dart';
import 'widgets/prop_ui.dart';

class PropStoreScreen extends ConsumerStatefulWidget {
  const PropStoreScreen({super.key});

  @override
  ConsumerState<PropStoreScreen> createState() => _PropStoreScreenState();
}

class _PropStoreScreenState extends ConsumerState<PropStoreScreen> {
  String? _planId;
  double? _size;

  Future<void> _refresh() async {
    ref
      ..invalidate(propPlansProvider)
      ..invalidate(propChallengesProvider(0));
    await ref.read(propPlansProvider.future).then((_) {}, onError: (Object _) {});
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final plans = ref.watch(propPlansProvider);
    final mine = ref.watch(propChallengesProvider(0)).value;
    final count = mine?.where((c) => c.status == 'active' || c.status == 'funded').length ?? 0;

    final hero = pageHero(
      context,
      ref,
      path: '/prop',
      title: t('prop.store.title'),
      lead: t('prop.store.subtitle'),
      actions: [
        KHeroButton(
          label: count > 0 ? '${t('prop.myChallenges')} · $count' : t('prop.myChallenges'),
          icon: LucideIcons.trophy,
          onPressed: () => context.go('/prop/mine'),
        ),
      ],
    );

    return KPageScroll(
      onRefresh: _refresh,
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        if (hero == null) ...[
          KPageHeader(title: t('prop.store.title'), subtitle: Text(t('prop.store.subtitle'))),
          const SizedBox(height: 14),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: PropHeaderButton(label: t('prop.myChallenges'), icon: LucideIcons.trophy, count: count, onTap: () => context.go('/prop/mine')),
          ),
          const SizedBox(height: 20),
        ],
        KAsync<List<Plan>>(
          value: plans,
          onRetry: () => ref.invalidate(propPlansProvider),
          error: (e) => PropLoadError(error: e, onRetry: () => ref.invalidate(propPlansProvider)),
          loading: const Column(children: [KSkeletonCard(height: 520, lines: 6), SizedBox(height: 16), KSkeletonCard(height: 360, lines: 5)]),
          builder: (list) {
            final plan = list.where((p) => p.id == _planId).firstOrNull ?? list.where((p) => p.type == '2-step').firstOrNull ?? list.firstOrNull;
            final size = plan == null
                ? null
                : (plan.sizes.where((s) => s.size == _size).firstOrNull ??
                      plan.sizes.where((s) => s.size == 50000).firstOrNull ??
                      plan.sizes[(plan.sizes.length - 1) ~/ 2]);
            if (plan == null || size == null) {
              return KCard(
                child: KEmptyState(art: KIllustrationName.propChallenge, title: t('prop.store.emptyTitle'), text: t('prop.store.emptyText')),
              );
            }
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                _Configurator(plans: list, plan: plan, size: size, onPlan: (id) => setState(() => _planId = id), onSize: (n) => setState(() => _size = n)),
                const SizedBox(height: 16),
                _PlanCard(
                  plan: plan,
                  size: size,
                  readOnly: readOnly,
                  onBuy: () => showCheckoutSheet(context, plan: plan, size: size),
                ),
                const SizedBox(height: 16),
                _CompareTable(plans: list, current: plan.id, size: size.size, onPick: (id) => setState(() => _planId = id)),
                const SizedBox(height: 16),
                const _EnforceCard(),
              ],
            );
          },
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ configurator */

class _Configurator extends StatelessWidget {
  const _Configurator({required this.plans, required this.plan, required this.size, required this.onPlan, required this.onSize});
  final List<Plan> plans;
  final Plan plan;
  final PlanSize size;
  final ValueChanged<String> onPlan;
  final ValueChanged<double> onSize;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final types = kPlanTypeOrder.where((x) => plans.any((p) => p.type == x)).toList();
    final sameType = plans.where((p) => p.type == plan.type).toList();
    final typeText = switch (plan.type) {
      '1-step' => t('prop.store.typeText.oneStep'),
      '2-step' => t('prop.store.typeText.twoStep'),
      'instant' => t('prop.store.typeText.instant'),
      _ => null,
    };
    final minDays = plan.phases.isEmpty ? '—' : '${plan.phases.map((p) => p.minDays).reduce((a, b) => a > b ? a : b)}';
    final tiles = [
      (
        LucideIcons.trendingDown,
        t('prop.rule.dailyLoss'),
        pctText(plan.dailyLoss),
        '${usd(size.size * plan.dailyLoss / 100, 0)} · ${basisLabel(t, plan.dailyBasis)}',
      ),
      (LucideIcons.gauge, t('prop.rule.maxDrawdown'), pctText(plan.maxDD), '${ddTypeLabel(t, plan.ddType)} · ${usd(size.size * plan.maxDD / 100, 0)}'),
      (LucideIcons.calendarDays, t('prop.minDays'), minDays, plan.hasTimeLimit ? t('prop.store.timeLimitApplies') : t('prop.noTimeLimit')),
      (
        LucideIcons.percent,
        t('prop.profitSplit'),
        pctText(plan.split),
        plan.splitMax > plan.split ? t('prop.store.scalesTo', {'pct': numText(plan.splitMax)}) : t('prop.store.fixed'),
      ),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(icon: LucideIcons.trophy, title: t('prop.store.chooseTitle'), subtitle: t('prop.store.chooseSubtitle')),
          const SizedBox(height: 18),
          if (types.length > 1)
            KSegmented<String>(
              plain: true,
              values: types,
              labels: [for (final x in types) typeLabel(t, x)],
              selected: plan.type,
              onChanged: (x) => onPlan(plans.firstWhere((p) => p.type == x).id),
            ),
          if (sameType.length > 1) ...[
            const SizedBox(height: 10),
            KChoiceChips<String>(values: [for (final p in sameType) p.id], labels: [for (final p in sameType) p.name], selected: plan.id, onChanged: onPlan),
          ],
          if (typeText != null) ...[const SizedBox(height: 12), Text(typeText, style: context.text.callout.copyWith(color: k.fg2))],
          const SizedBox(height: 18),
          PropLabel(t('prop.accountSize')),
          const SizedBox(height: 10),
          PropGrid(
            children: [for (final s in plan.sizes) _SizeButton(s: s, on: s.size == size.size, onTap: () => onSize(s.size))],
          ),
          const SizedBox(height: 18),
          PropLabel(t('prop.store.yourPath')),
          const SizedBox(height: 10),
          _PhaseTrack(plan: plan),
          const SizedBox(height: 16),
          PropGrid(
            children: [
              for (final x in tiles)
                PropRow(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Row(
                        children: [
                          Icon(x.$1, size: 14, color: k.fg3),
                          const SizedBox(width: 6),
                          Expanded(
                            child: Text(
                              x.$2,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                            ),
                          ),
                        ],
                      ),
                      const SizedBox(height: 4),
                      Text(x.$3, textDirection: TextDirection.ltr, style: context.text.figure.copyWith(fontSize: 18)),
                      Text(
                        x.$4,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.caption.copyWith(color: k.fg3, fontSize: 11, fontWeight: FontWeight.w400),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

class _SizeButton extends StatelessWidget {
  const _SizeButton({required this.s, required this.on, required this.onTap});
  final PlanSize s;
  final bool on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KPressable(
      onTap: onTap,
      semanticLabel: sizeLabel(s.size),
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 160),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 11),
        decoration: BoxDecoration(
          color: on ? k.emberSoft : k.surface2,
          borderRadius: BorderRadius.circular(14),
          border: Border.all(color: on ? k.ember.withValues(alpha: 0.5) : k.line),
        ),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(sizeLabel(s.size), textDirection: TextDirection.ltr, style: context.text.figure.copyWith(fontSize: 17, letterSpacing: -0.3)),
            const SizedBox(height: 2),
            Text(
              feeText(s.fee),
              textDirection: TextDirection.ltr,
              style: context.text.caption.copyWith(color: on ? k.ember : k.fg3, fontFeatures: kTabular),
            ),
          ],
        ),
      ),
    );
  }
}

class _PhaseTrack extends StatelessWidget {
  const _PhaseTrack({required this.plan});
  final Plan plan;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final steps = [
      for (final x in plan.phases)
        (
          name: x.name,
          title: t('prop.track.target', {'pct': numText(x.target)}),
          sub: x.timeLimit > 0
              ? t('prop.track.minMax', {'min': daysText(t, x.minDays), 'max': daysText(t, x.timeLimit)})
              : t('prop.track.min', {'min': daysText(t, x.minDays)}),
          funded: false,
        ),
      (
        name: t('prop.status.funded'),
        title: t('prop.track.split', {'pct': numText(plan.split)}),
        sub: t('prop.track.firstPayout', {'days': daysText(t, plan.firstPayoutDays)}),
        funded: true,
      ),
    ];
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        for (var i = 0; i < steps.length; i++) ...[
          if (i > 0) const SizedBox(height: 8),
          PropRow(
            color: steps[i].funded ? k.goldSoft : null,
            border: steps[i].funded ? k.gold.withValues(alpha: 0.3) : null,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Container(
                      width: 20,
                      height: 20,
                      alignment: Alignment.center,
                      decoration: BoxDecoration(shape: BoxShape.circle, color: steps[i].funded ? k.gold : k.surface3),
                      child: Text(
                        '${i + 1}',
                        style: context.text.micro.copyWith(color: steps[i].funded ? const Color(0xFF1A1204) : k.fg2, fontFeatures: kTabular),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      steps[i].name,
                      style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                    ),
                  ],
                ),
                const SizedBox(height: 6),
                Text(steps[i].title, style: context.text.figure),
                const SizedBox(height: 2),
                Text(
                  steps[i].sub,
                  style: context.text.caption.copyWith(color: k.fg3, fontSize: 11, fontWeight: FontWeight.w400),
                ),
              ],
            ),
          ),
        ],
      ],
    );
  }
}

/* ------------------------------------------------------------------ plan card */

class _PlanCard extends StatelessWidget {
  const _PlanCard({required this.plan, required this.size, required this.readOnly, required this.onBuy});
  final Plan plan;
  final PlanSize size;
  final bool readOnly;
  final VoidCallback onBuy;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final fee = feeText(size.fee);
    final rows = [
      for (final p in plan.phases) KKV(t('prop.rules.phaseTarget', {'phase': p.name}), '${pctText(p.target)} · ${usd(size.size * p.target / 100, 0)}'),
      KKV(t('prop.dailyLossLimit'), '${pctText(plan.dailyLoss)} · ${basisLabel(t, plan.dailyBasis)}'),
      KKV(t('prop.rule.maxDrawdown'), '${pctText(plan.maxDD)} ${ddTypeLabel(t, plan.ddType)}'),
      KKV(t('prop.leverage'), '1:${size.leverage}'),
      KKV(t('prop.profitSplit'), plan.splitMax > plan.split ? '${pctText(plan.split)} → ${pctText(plan.splitMax)}' : pctText(plan.split)),
      KKV(t('prop.feeRefund'), plan.refundFee ? t('prop.store.withFirstPayout') : t('prop.nonRefundable'), tone: plan.refundFee ? k.up : k.fg3),
    ];
    return KCard(
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          PropLabel(plan.name),
          const SizedBox(height: 4),
          Text(t('prop.store.simulatedAccount', {'size': sizeLabel(size.size)}), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13)),
          const SizedBox(height: 12),
          Wrap(
            crossAxisAlignment: WrapCrossAlignment.end,
            spacing: 8,
            runSpacing: 4,
            children: [
              Text(fee, textDirection: TextDirection.ltr, style: context.text.moneyXL.copyWith(fontSize: 40, letterSpacing: -1.2)),
              Padding(
                padding: const EdgeInsets.only(bottom: 4),
                child: Text(t('prop.store.oneTimeUsdt'), style: context.text.footnote.copyWith(color: k.fg3)),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              if (plan.refundFee) KChip(label: t('prop.store.refundable'), tone: KChipTone.gold, small: true),
              KChip(label: t('prop.store.leverageChip', {'leverage': size.leverage}), small: true),
              KChip(label: plan.hasTimeLimit ? t('prop.rule.timeLimit') : t('prop.noTimeLimit'), small: true),
            ],
          ),
          const SizedBox(height: 10),
          KKeyValues(rows, dense: true),
          if (!readOnly) ...[
            const SizedBox(height: 16),
            KButton(label: t('prop.store.buyChallenge', {'fee': fee}), trailingIcon: arrowEnd(context), size: KButtonSize.lg, expand: true, onPressed: onBuy),
          ],
          const SizedBox(height: 10),
          Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(LucideIcons.wallet, size: 14, color: k.fg3),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  t('prop.store.paidFromWallet'),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ compare */

class _CompareTable extends StatelessWidget {
  const _CompareTable({required this.plans, required this.current, required this.size, required this.onPick});
  final List<Plan> plans;
  final String current;
  final double size;
  final ValueChanged<String> onPick;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    PlanSize? feeAt(Plan p) => p.sizes.where((s) => s.size == size).firstOrNull;
    Widget yes(String text) => _Mark(ok: true, text: text);
    Widget no(String text) => _Mark(ok: false, text: text);
    Widget txt(String s, {Color? color}) => Text(s, style: context.text.footnote.copyWith(color: color ?? k.fg, fontSize: 13));
    Widget muted(String main, String rest) => Text.rich(
      TextSpan(
        text: main,
        children: [
          TextSpan(
            text: ' · $rest',
            style: TextStyle(color: k.fg3),
          ),
        ],
      ),
      style: context.text.footnote.copyWith(color: k.fg, fontSize: 13),
    );

    final rows = <(String, IconData, Widget Function(Plan))>[
      (
        t('prop.compare.fee', {'size': sizeLabel(size)}),
        LucideIcons.wallet,
        (p) => feeAt(p) != null
            ? Text(usd(feeAt(p)!.fee, 0), style: context.text.figure.copyWith(fontSize: 13.5))
            : txt(t('prop.compare.notOffered'), color: k.fg3),
      ),
      (t('prop.rule.profitTarget'), LucideIcons.target, (p) => txt(p.phases.isNotEmpty ? p.phases.map((x) => pctText(x.target)).join(' / ') : t('prop.none'))),
      (t('prop.dailyLossLimit'), LucideIcons.trendingDown, (p) => muted(pctText(p.dailyLoss), basisLabel(t, p.dailyBasis))),
      (t('prop.rule.maxDrawdown'), LucideIcons.gauge, (p) => muted(pctText(p.maxDD), ddTypeLabel(t, p.ddType))),
      (t('prop.compare.minTradingDays'), LucideIcons.calendarDays, (p) => txt(p.phases.isNotEmpty ? p.phases.map((x) => x.minDays).join(' / ') : '—')),
      (
        t('prop.rule.timeLimit'),
        LucideIcons.clock,
        (p) => txt(p.hasTimeLimit ? p.phases.map((x) => x.timeLimit > 0 ? daysText(t, x.timeLimit) : t('prop.compare.noneLower')).join(' / ') : t('prop.none')),
      ),
      (t('prop.leverage'), LucideIcons.layers, (p) => txt(feeAt(p) != null ? '1:${feeAt(p)!.leverage}' : '1:${p.sizes.firstOrNull?.leverage ?? '—'}')),
      (t('prop.profitSplit'), LucideIcons.percent, (p) => txt(p.splitMax > p.split ? '${pctText(p.split)} → ${pctText(p.splitMax)}' : pctText(p.split))),
      (t('prop.feeRefund'), LucideIcons.check, (p) => p.refundFee ? yes(t('prop.compare.firstPayout')) : no(t('prop.no'))),
      (
        t('prop.compare.consistencyRule'),
        LucideIcons.shieldCheck,
        (p) => p.consistency > 0 ? txt(t('prop.compare.bestDay', {'pct': numText(p.consistency)})) : no(t('prop.none')),
      ),
      (t('prop.newsTrading'), LucideIcons.zap, (p) => p.newsTrading ? yes(t('prop.allowed')) : no(t('prop.compare.newsBlocked', {'min': p.newsWindow}))),
      (t('prop.rule.weekendHolding'), LucideIcons.calendarDays, (p) => p.weekendHolding ? yes(t('prop.allowed')) : no(t('prop.compare.closedFriday'))),
      (t('prop.expertAdvisors'), LucideIcons.layers, (p) => p.eaAllowed ? yes(t('prop.allowed')) : no(t('prop.notAllowed'))),
      (
        t('prop.bannedStrategies'),
        LucideIcons.ban,
        (p) => Text(
          p.banned.isNotEmpty ? p.banned.map((b) => bannedLabel(t, b)).join(' · ') : t('prop.none'),
          style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w400, fontSize: 12),
        ),
      ),
      (
        t('prop.firstPayout'),
        LucideIcons.clock,
        (p) => txt(t('prop.compare.firstPayoutValue', {'days': daysText(t, p.firstPayoutDays), 'freq': payoutFreqLabel(t, p.payoutFreq)})),
      ),
    ];

    const labelW = 132.0, colW = 168.0;
    final on = k.emberSoft.withValues(alpha: 0.6);
    final onLine = k.ember.withValues(alpha: 0.4);

    Widget cell(Plan p, Widget child, {bool last = false}) {
      final sel = p.id == current;
      return Container(
        width: colW,
        margin: const EdgeInsets.symmetric(horizontal: 3),
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 11),
        decoration: BoxDecoration(
          color: sel ? on : null,
          border: Border(
            left: BorderSide(color: sel ? onLine : k.line),
            right: BorderSide(color: sel ? onLine : k.line),
            // the rounded last cell needs one border colour (Flutter paints radii on uniform colours only)
            top: BorderSide(color: sel ? (last ? onLine : k.ember.withValues(alpha: 0.15)) : k.line),
            bottom: last ? BorderSide(color: sel ? onLine : k.line) : BorderSide.none,
          ),
          borderRadius: last ? const BorderRadius.vertical(bottom: Radius.circular(16)) : null,
        ),
        child: Align(alignment: AlignmentDirectional.topStart, child: child),
      );
    }

    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(icon: LucideIcons.layers, title: t('prop.compare.title'), subtitle: t('prop.compare.subtitle')),
          const SizedBox(height: 16),
          SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                // plan heads (tap to pick)
                Row(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    SizedBox(
                      width: labelW,
                      child: Padding(
                        padding: const EdgeInsets.only(bottom: 10),
                        child: Text(t('prop.compare.rule').toUpperCase(), style: context.text.caption.copyWith(color: k.fg3, letterSpacing: 0.6, fontSize: 11)),
                      ),
                    ),
                    for (final p in plans)
                      KPressable(
                        onTap: () => onPick(p.id),
                        semanticLabel: p.name,
                        child: Container(
                          width: colW,
                          margin: const EdgeInsets.symmetric(horizontal: 3),
                          padding: const EdgeInsets.fromLTRB(12, 10, 10, 10),
                          decoration: BoxDecoration(
                            color: p.id == current ? k.emberSoft : k.surface2,
                            borderRadius: const BorderRadius.vertical(top: Radius.circular(16)),
                            border: Border(
                              left: BorderSide(color: p.id == current ? onLine : k.line),
                              right: BorderSide(color: p.id == current ? onLine : k.line),
                              top: BorderSide(color: p.id == current ? onLine : k.line),
                            ),
                          ),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Row(
                                children: [
                                  Icon(typeIcon(p.type), size: 14, color: k.fg),
                                  const SizedBox(width: 6),
                                  Expanded(
                                    child: Text(p.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.label.copyWith(fontSize: 13.5)),
                                  ),
                                ],
                              ),
                              const SizedBox(height: 4),
                              Row(
                                children: [
                                  Expanded(
                                    child: Text(
                                      t('prop.compare.from', {'fee': usd(p.sizes.map((s) => s.fee).reduce((a, b) => a < b ? a : b), 0)}),
                                      style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 11),
                                    ),
                                  ),
                                  if (p.id == current) KChip(label: t('prop.compare.selected'), tone: KChipTone.ember, small: true),
                                ],
                              ),
                            ],
                          ),
                        ),
                      ),
                  ],
                ),
                for (var ri = 0; ri < rows.length; ri++)
                  IntrinsicHeight(
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Container(
                          width: labelW,
                          padding: const EdgeInsetsDirectional.only(top: 11, bottom: 11, end: 10),
                          decoration: BoxDecoration(
                            border: Border(top: BorderSide(color: k.line)),
                          ),
                          child: Row(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Padding(
                                padding: const EdgeInsets.only(top: 2),
                                child: Icon(rows[ri].$2, size: 13, color: k.fg3),
                              ),
                              const SizedBox(width: 7),
                              Expanded(
                                child: Text(rows[ri].$1, style: context.text.footnote.copyWith(color: k.fg2, fontSize: 12.5)),
                              ),
                            ],
                          ),
                        ),
                        for (final p in plans) cell(p, rows[ri].$3(p), last: ri == rows.length - 1),
                      ],
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _Mark extends StatelessWidget {
  const _Mark({required this.ok, required this.text});
  final bool ok;
  final String text;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Padding(
          padding: const EdgeInsets.only(top: 2),
          child: Icon(ok ? LucideIcons.check : LucideIcons.minus, size: 13, color: ok ? k.up : k.fg3),
        ),
        const SizedBox(width: 6),
        Expanded(
          child: Text(text, style: context.text.footnote.copyWith(color: ok ? k.fg : k.fg3, fontSize: 13)),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ enforcement */

class _EnforceCard extends StatelessWidget {
  const _EnforceCard();

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final items = [
      (t('prop.enforce.monitoring'), t('prop.enforce.monitoringText'), k.info),
      (t('prop.enforce.warnings'), t('prop.enforce.warningsText'), k.warn),
      (t('prop.enforce.breach'), t('prop.enforce.breachText'), k.down),
      (t('prop.enforce.pass'), t('prop.enforce.passText'), k.up),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(icon: LucideIcons.shieldCheck, title: t('prop.enforce.title'), subtitle: t('prop.enforce.subtitle')),
          const SizedBox(height: 14),
          for (var i = 0; i < items.length; i++) ...[
            if (i > 0) const SizedBox(height: 8),
            PropRow(
              padding: EdgeInsets.zero,
              child: IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    Container(
                      width: 3,
                      margin: const EdgeInsets.symmetric(vertical: 8),
                      decoration: BoxDecoration(
                        color: items[i].$3,
                        borderRadius: const BorderRadiusDirectional.horizontal(end: Radius.circular(3)),
                      ),
                    ),
                    Expanded(
                      child: Padding(
                        padding: const EdgeInsetsDirectional.fromSTEB(14, 12, 14, 12),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(items[i].$1, style: context.text.label.copyWith(fontSize: 13.5)),
                            const SizedBox(height: 2),
                            Text(items[i].$2, style: context.text.footnote.copyWith(color: k.fg3)),
                          ],
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

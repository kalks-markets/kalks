// Contests & Rewards › Cashback (/rewards/cashback). Port of the web's LiveCashbackPage
// (apps/crm/components/growth/cashback.tsx) in its phone order:
//   header (Cashback + Wallet)  ·  banner slot  ·  KPI cards (pending, this month, paid, lifetime)
//   ·  daily cashback (30 days)  ·  programmes (enrol)  ·  cashback history  ·  payouts
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/notifications/notifications.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'rewards_api.dart';
import 'widgets/growth_ui.dart';

class CashbackScreen extends ConsumerWidget {
  const CashbackScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    final title = t('rewards.cashback.title');
    final subtitle = t('rewards.cashback.subtitle');
    final walletOn = ref.watch(configProvider).moduleOn('wallet');
    final v = ref.watch(cashbackProvider);
    if (!v.hasValue) {
      return GrowthFallback(
        title: title,
        subtitle: subtitle,
        error: v.hasError ? v.error : null,
        onRetry: () => ref.invalidate(cashbackProvider),
        heights: const [150, 320, 320],
      );
    }
    final data = v.requireValue;
    final labels = [for (final d in data.series) f.day(d.day)];
    final values = [for (final d in data.series) d.amount];
    final total30 = values.fold<double>(0, (s, x) => s + x);
    ({String day, double amount})? best;
    for (final d in data.series) {
      if (best == null || d.amount > best.amount) best = d;
    }
    final lotsMonth = data.programmes.fold<double>(0, (s, p) => s + p.lotsMonth);
    final width = MediaQuery.sizeOf(context).width;
    final kpiWidth = (width - 2 * KSpace.page) * 0.78;
    final lastPayout = data.payouts.isEmpty ? null : data.payouts.first;

    return KPageScroll(
      onRefresh: () async {
        ref
          ..invalidate(cashbackProvider)
          ..invalidate(bannersProvider('rewards'));
        await ref.read(cashbackProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        KPageHeader(title: title, subtitle: Text(subtitle)),
        if (walletOn) ...[
          const SizedBox(height: 12),
          Row(
            children: [
              KButton(label: t('rewards.cashback.wallet'), icon: LucideIcons.wallet, variant: KButtonVariant.surface, onPressed: () => context.go('/wallet')),
            ],
          ),
        ],
        const SizedBox(height: 20),
        const BannerSlot(placement: 'rewards'),
        SizedBox(
          height: 170,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            physics: const PageScrollPhysics(parent: BouncingScrollPhysics()),
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.cashback.kpiPending'),
                icon: LucideIcons.banknote,
                value: KMoney(data.accrued, style: context.text.moneyL),
                chip: KChip(label: t('rewards.cashback.kpiPendingChip'), tone: KChipTone.ember),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.cashback.kpiMonth'),
                icon: LucideIcons.trendingUp,
                value: KMoney(data.month, style: context.text.moneyL),
                chip: KChip(label: t('rewards.value.lots', {'lots': f.lots(lotsMonth)}), tone: KChipTone.up),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.cashback.kpiPaid'),
                icon: LucideIcons.calendarClock,
                value: KMoney(data.paid, style: context.text.moneyL),
                chip: KChip(
                  label: lastPayout != null
                      ? t('rewards.cashback.kpiLast', {'date': f.date(lastPayout.paidAt ?? lastPayout.createdAt, year: false)})
                      : t('rewards.cashback.kpiNoPayouts'),
                ),
                onTap: walletOn ? () => context.go('/wallet') : null,
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.cashback.kpiLifetime'),
                icon: LucideIcons.coins,
                value: KMoney(data.lifetime, style: context.text.moneyL),
                chip: KChip(label: t('rewards.cashback.programmesCount', {'count': data.programmes.length})),
              ),
            ],
          ),
        ),
        const SizedBox(height: kBlockGap),
        // daily cashback
        KCard(
          padding: EdgeInsets.zero,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 18),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    KCardHeader(
                      title: t('rewards.cashback.dailyTitle'),
                      subtitle: t('rewards.cashback.dailySubtitle'),
                      action: KChip(label: t('rewards.cashback.in30', {'amount': f.usd(total30)}), tone: KChipTone.ember),
                    ),
                    const SizedBox(height: 18),
                    if (values.isEmpty || total30 == 0)
                      CardEmpty(title: t('rewards.cashback.dailyEmptyTitle'), text: t('rewards.cashback.dailyEmptyText'), minHeight: 200)
                    else
                      DayBars(labels: labels, values: values, format: f.usd, height: 180),
                  ],
                ),
              ),
              const KDivider(),
              IntrinsicHeight(
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: [
                    for (final (i, (label, value)) in [
                      (t('rewards.cashback.avgDay'), f.usd(total30 / (data.series.isEmpty ? 1 : data.series.length))),
                      (
                        best != null && best.amount > 0 ? t('rewards.cashback.bestDayOn', {'day': f.day(best.day)}) : t('rewards.cashback.bestDay'),
                        best != null && best.amount > 0 ? f.usd(best.amount) : '—',
                      ),
                      (t('rewards.cashback.lotsMonth'), f.lots(lotsMonth)),
                    ].indexed) ...[
                      if (i > 0) const KDivider(vertical: true),
                      Expanded(
                        child: Padding(
                          padding: const EdgeInsets.fromLTRB(14, 14, 10, 14),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(
                                label,
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                              ),
                              const SizedBox(height: 4),
                              Text(
                                value,
                                textDirection: TextDirection.ltr,
                                style: context.text.figure.copyWith(fontSize: 15, fontWeight: FontWeight.w500),
                              ),
                            ],
                          ),
                        ),
                      ),
                    ],
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: kBlockGap),
        // programmes
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(
                title: t('rewards.cashback.programmesTitle'),
                subtitle: t('rewards.cashback.programmesSubtitle'),
                icon: LucideIcons.percent,
                tone: KTone.mint,
              ),
              const SizedBox(height: 14),
              if (data.programmes.isEmpty)
                CardEmpty(title: t('rewards.cashback.programmesEmptyTitle'), text: t('rewards.cashback.programmesEmptyText'))
              else
                for (final p in data.programmes) ...[if (p != data.programmes.first) const SizedBox(height: 8), _ProgrammeRow(p: p)],
            ],
          ),
        ),
        const SizedBox(height: kBlockGap),
        _AccrualsCard(rows: data.accruals),
        const SizedBox(height: kBlockGap),
        // payouts
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('rewards.cashback.payoutsTitle'), subtitle: t('rewards.cashback.payoutsSubtitle')),
              const SizedBox(height: 10),
              PagedRows<CashbackPayout>(
                rows: data.payouts,
                pageSize: 8,
                empty: CardEmpty(title: t('rewards.cashback.kpiNoPayouts'), text: t('rewards.cashback.payoutsEmptyText')),
                itemBuilder: (context, x) => Padding(
                  padding: const EdgeInsets.symmetric(vertical: 11),
                  child: Row(
                    children: [
                      Expanded(
                        child: Text(
                          f.dateTime(x.createdAt),
                          style: context.text.footnote.copyWith(color: k.fg2, fontFeatures: kTabular),
                        ),
                      ),
                      Text(f.usd(x.amount), textDirection: TextDirection.ltr, style: context.text.figure.copyWith(fontSize: 14)),
                      const SizedBox(width: 10),
                      GrowthStatus(x.status),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

String _scope(T t, CashbackProgramme p) {
  final parts = [...p.assetClasses.map(titleCase), ...p.symbols];
  final what = parts.isNotEmpty ? parts.join(', ') : t('rewards.cashback.allInstruments');
  return p.accountGroups.isNotEmpty ? '$what · ${p.accountGroups.join(', ')}' : what;
}

class _ProgrammeRow extends ConsumerStatefulWidget {
  const _ProgrammeRow({required this.p});
  final CashbackProgramme p;

  @override
  ConsumerState<_ProgrammeRow> createState() => _ProgrammeRowState();
}

class _ProgrammeRowState extends ConsumerState<_ProgrammeRow> {
  bool _busy = false;

  Future<void> _enrol() async {
    final t = context.t;
    final f = GrowthFmt(t);
    final p = widget.p;
    final toasts = ref.read(notificationsProvider.notifier);
    setState(() => _busy = true);
    try {
      await growthPost(ref, 'cashback/${p.id}/enrol');
      KHaptics.success();
      toasts.toast(
        NotificationKind.success,
        t('rewards.cashback.toastEnrolled', {'name': p.name}),
        description: t('rewards.cashback.toastEnrolledText', {'amount': f.usd(p.usdPerLot)}),
      );
      ref.invalidate(cashbackProvider);
    } on ApiException catch (e) {
      toasts.toast(NotificationKind.error, t('rewards.cashback.enrolError'), description: growthError(e, t));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    final p = widget.p;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final cap = p.maxPerMonth;
    final capPct = cap != null && cap > 0 ? (p.earnedMonth / cap).clamp(0.0, 1.0) : null;
    final active = !p.optIn || p.enrolled;
    final small = context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400);
    return RowBox(
      key: ValueKey('cashback-programme-${p.id}'),
      border: active ? k.gold.withValues(alpha: 0.25) : null,
      padding: const EdgeInsets.fromLTRB(14, 13, 14, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Wrap(
                      spacing: 8,
                      runSpacing: 4,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        Text(
                          p.name,
                          style: context.text.callout.copyWith(fontWeight: FontWeight.w600, color: k.fg, fontSize: 14),
                        ),
                        if (p.optIn)
                          p.enrolled
                              ? KChip(label: t('rewards.cashback.enrolled'), tone: KChipTone.up, small: true)
                              : KChip(label: t('rewards.cashback.optIn'), tone: KChipTone.warn, small: true)
                        else
                          KChip(label: t('rewards.cashback.automatic'), small: true),
                      ],
                    ),
                    const SizedBox(height: 2),
                    Text(_scope(t, p), maxLines: 1, overflow: TextOverflow.ellipsis, style: small),
                  ],
                ),
              ),
              const SizedBox(width: 10),
              Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Text(f.usd(p.usdPerLot), textDirection: TextDirection.ltr, style: context.text.figure.copyWith(fontSize: 18)),
                  Text(
                    t('rewards.unit.perLot'),
                    style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                  ),
                ],
              ),
            ],
          ),
          if (p.description.isNotEmpty) ...[const SizedBox(height: 8), Text(p.description, style: small.copyWith(fontSize: 12))],
          const SizedBox(height: 10),
          Wrap(
            spacing: 14,
            runSpacing: 4,
            children: [
              Text.rich(
                TextSpan(
                  children: [
                    TextSpan(text: '${t('rewards.cashback.thisMonth')} '),
                    TextSpan(
                      text: f.usd(p.earnedMonth),
                      style: TextStyle(color: k.up, fontWeight: FontWeight.w600, fontFeatures: kTabular),
                    ),
                    const TextSpan(text: ' · '),
                    TextSpan(
                      text: t('rewards.value.lots', {'lots': f.lots(p.lotsMonth)}),
                      style: TextStyle(color: k.fg2, fontFeatures: kTabular),
                    ),
                  ],
                ),
                style: small.copyWith(fontSize: 12),
              ),
              if (p.endsAt != null) Text(t('rewards.cashback.ends', {'date': f.date(p.endsAt)}), style: small.copyWith(fontSize: 12)),
            ],
          ),
          if (capPct != null) ...[
            const SizedBox(height: 8),
            Row(
              children: [
                Expanded(child: Text(t('rewards.cashback.monthlyCap'), style: small.copyWith(fontSize: 11))),
                Text(
                  '${f.usd(p.earnedMonth)} / ${f.usd(cap!)}',
                  textDirection: TextDirection.ltr,
                  style: small.copyWith(fontSize: 11, fontFeatures: kTabular),
                ),
              ],
            ),
            const SizedBox(height: 4),
            KProgressBar(value: capPct, color: capPct >= 1 ? k.warn : k.gold),
          ],
          if (p.optIn && !p.enrolled && !readOnly) ...[
            const SizedBox(height: 12),
            KButton(
              key: ValueKey('cashback-enrol-${p.id}'),
              label: t('rewards.cashback.enrol'),
              size: KButtonSize.sm,
              expand: true,
              loading: _busy,
              onPressed: _enrol,
            ),
          ],
        ],
      ),
    );
  }
}

class _AccrualsCard extends StatefulWidget {
  const _AccrualsCard({required this.rows});
  final List<CashbackAccrual> rows;

  @override
  State<_AccrualsCard> createState() => _AccrualsCardState();
}

class _AccrualsCardState extends State<_AccrualsCard> {
  String _q = '';

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    final q = _q.trim().toLowerCase();
    final rows = widget.rows.where((r) => q.isEmpty || '${r.symbol} ${r.login} ${r.dealId} ${r.programme}'.toLowerCase().contains(q)).toList();
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('rewards.cashback.historyTitle'), subtitle: t('rewards.cashback.historySubtitle')),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: KSearchField(placeholder: t('rewards.cashback.search'), onChanged: (s) => setState(() => _q = s)),
              ),
              const SizedBox(width: 8),
              CsvButton(
                onPressed: rows.isEmpty
                    ? null
                    : () => shareCsv(
                        context,
                        'kalks-cashback',
                        [
                          t('common.date'),
                          t('rewards.cashback.colDeal'),
                          t('rewards.cashback.colSymbol'),
                          t('rewards.cashback.colProgramme'),
                          t('rewards.cashback.colLots'),
                          t('rewards.cashback.colCashback'),
                          t('common.status'),
                        ],
                        [
                          for (final r in rows)
                            [r.createdAt?.toIso8601String(), r.dealId, r.symbol, r.programme, GrowthFmt.plain(r.lots), GrowthFmt.plain(r.amount), r.status],
                        ],
                      ),
              ),
            ],
          ),
          const SizedBox(height: 6),
          PagedRows<CashbackAccrual>(
            rows: rows,
            empty: Padding(
              padding: const EdgeInsets.only(top: 6),
              child: CardEmpty(title: t('rewards.cashback.historyEmptyTitle'), text: t('rewards.cashback.historyEmptyText')),
            ),
            itemBuilder: (context, r) => Padding(
              padding: const EdgeInsets.symmetric(vertical: 10),
              child: Row(
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Row(
                          children: [
                            Text(
                              r.symbol,
                              style: context.text.callout.copyWith(fontWeight: FontWeight.w600, color: k.fg),
                            ),
                            const SizedBox(width: 6),
                            Text(
                              '#${r.login}',
                              textDirection: TextDirection.ltr,
                              style: context.text.mono(11.5, color: k.fg3),
                            ),
                          ],
                        ),
                        const SizedBox(height: 2),
                        Text(
                          '${f.dateTime(r.createdAt)} · ${t('rewards.value.lots', {'lots': f.lots(r.lots)})}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 10),
                  Column(
                    crossAxisAlignment: CrossAxisAlignment.end,
                    children: [
                      Text(
                        '+${f.usd(r.amount)}',
                        textDirection: TextDirection.ltr,
                        style: context.text.figure.copyWith(fontSize: 14, color: k.up),
                      ),
                      const SizedBox(height: 4),
                      GrowthStatus(r.status),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

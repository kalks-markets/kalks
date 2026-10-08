// Partner › Payouts (web components/partner/live/payouts.tsx LivePartnerPayouts), in the phone order: header (the
// schedule) · accruing now (the unbatched balance, the batch close countdown, the minimum) · where payouts go (Open
// wallet) · KPI cards · payout schedule · recent payouts (bars) · payout history (CSV, pages, a batch's details).
// Payouts are batched by the broker on the schedule; there is no payout request on the web either.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'partner_api.dart';
import 'widgets/partner_widgets.dart';

const int _dayMs = 86400000;
const int _pageSize = 10;

/// "5 Oct – 11 Oct 2026" (period boundaries are UTC midnights; the end is exclusive).
String _periodOf(PartnerFmt pf, String schedule, String endIso, [String? startIso]) {
  final end = DateTime.tryParse(endIso)?.millisecondsSinceEpoch ?? 0;
  final start =
      (startIso != null ? DateTime.tryParse(startIso)?.millisecondsSinceEpoch : null) ??
      end -
          (schedule == 'daily'
                  ? 1
                  : schedule == 'monthly'
                  ? 30
                  : 7) *
              _dayMs;
  final last = end - _dayMs;
  DateTime u(int ms) => DateTime.fromMillisecondsSinceEpoch(ms, isUtc: true);
  return last <= start ? pf.utcDay(u(start), withYear: true) : '${pf.utcDay(u(start))} – ${pf.utcDay(u(last), withYear: true)}';
}

class PartnerPayoutsScreen extends ConsumerStatefulWidget {
  const PartnerPayoutsScreen({super.key});

  @override
  ConsumerState<PartnerPayoutsScreen> createState() => _PartnerPayoutsScreenState();
}

class _PartnerPayoutsScreenState extends ConsumerState<PartnerPayoutsScreen> {
  int _page = 0;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final async = ref.watch(partnerPayoutsProvider);
    final data = async.value;
    final subtitle = data == null
        ? t('partner.pay.subtitle.none')
        : t(
            data.schedule == 'daily'
                ? 'partner.pay.subtitle.daily'
                : data.schedule == 'monthly'
                ? 'partner.pay.subtitle.monthly'
                : 'partner.pay.subtitle.weekly',
          );
    if (data == null) {
      return PartnerPageFallback(
        title: t('partner.payouts.title'),
        subtitle: subtitle,
        error: async.hasError ? async.error : null,
        onRetry: () => ref.invalidate(partnerPayoutsProvider),
        skeleton: const [300, 260, 150],
      );
    }

    final paid = data.items.where((p) => p.status == 'paid').toList();
    final paidTotal = paid.fold<double>(0, (s, p) => s + p.amount);
    final inFlight = data.items.where((p) => p.status == 'awaiting_approval' || p.status == 'processing').fold<double>(0, (s, p) => s + p.amount);
    final lastPaid = paid.firstOrNull;
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.78;
    final pages = math.max(1, (data.items.length / _pageSize).ceil());
    final page = _page.clamp(0, pages - 1);
    final view = data.items.skip(page * _pageSize).take(_pageSize).toList();

    return KPageScroll(
      onRefresh: () async {
        ref.invalidate(partnerPayoutsProvider);
        await ref.read(partnerPayoutsProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        KPageHeader(title: t('partner.payouts.title'), subtitle: Text(subtitle)),
        const SizedBox(height: 20),
        _Hero(d: data),
        kGap,
        const _WalletNote(),
        kGap,
        SizedBox(
          height: 186,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            physics: const PageScrollPhysics(parent: BouncingScrollPhysics()),
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('partner.paidAllTime'),
                icon: LucideIcons.circleDollarSign,
                value: KMoney(paidTotal, style: context.text.moneyL),
                chip: KChip(
                  label: lastPaid?.paidAt != null ? t('partner.pay.lastOn', {'date': pf.day(lastPaid!.paidAt)}) : t('partner.pay.noPayouts'),
                  tone: lastPaid != null ? KChipTone.up : KChipTone.neutral,
                ),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.pay.nextCloses'),
                icon: LucideIcons.calendarClock,
                value: Text(pf.day(data.nextClose), style: context.text.moneyL.copyWith(fontSize: 26)),
                chip: KChip(label: t('partner.pay.scheduleChip', {'schedule': scheduleLabel(t, data.schedule)}), tone: KChipTone.ember),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.pay.notBatched'),
                icon: LucideIcons.hourglass,
                value: KMoney(data.unbatched, style: context.text.moneyL),
                chip: KChip(label: t('partner.pendingCommission'), tone: KChipTone.warn),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.pay.inReview'),
                icon: LucideIcons.clock,
                value: KMoney(inFlight, style: context.text.moneyL),
                chip: KChip(label: t('partner.pay.minPayout', {'amount': money0(data.minAmount)})),
              ),
            ],
          ),
        ),
        kGap,
        _ScheduleCard(d: data),
        kGap,
        _HistoryChart(items: data.items, schedule: data.schedule),
        kGap,
        KCard(
          padding: const EdgeInsets.fromLTRB(14, 18, 14, 16),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 2),
                child: KCardHeader(
                  title: t('partner.pay.history'),
                  subtitle: t('partner.pay.historySubtitle'),
                  action: data.items.isEmpty
                      ? null
                      : CsvButton(
                          onPressed: () => exportCsv(
                            context,
                            ref,
                            name: 'kalks-ib-payouts',
                            headers: [
                              t('partner.pay.batch'),
                              t('partner.pay.lines'),
                              t('common.amount'),
                              t('common.status'),
                              t('partner.commissionStatus.paid'),
                              t('partner.pay.destination'),
                            ],
                            rows: [
                              for (final p in data.items) [p.batchId, p.lines, p.amount, p.status, p.paidAt ?? '', p.destination],
                            ],
                          ),
                        ),
                ),
              ),
              const SizedBox(height: 8),
              if (data.items.isEmpty)
                Padding(
                  padding: const EdgeInsets.symmetric(vertical: 10),
                  child: CardEmpty(title: t('partner.pay.noPayouts'), text: t('partner.pay.tableEmptyText', {'amount': money0(data.minAmount)})),
                )
              else
                for (final (i, p) in view.indexed) ...[
                  if (i > 0) const KDivider(),
                  KPressable(
                    onTap: () => _openPayout(context, p),
                    pressedScale: 1,
                    child: Padding(
                      padding: const EdgeInsets.symmetric(vertical: 11),
                      child: Row(
                        children: [
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text('#${p.batchId}', style: context.text.mono(12.5, color: k.fg)),
                                const SizedBox(height: 2),
                                Text(
                                  _periodOf(pf, p.schedule, p.periodEnd, p.periodStart),
                                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                                ),
                              ],
                            ),
                          ),
                          Column(
                            crossAxisAlignment: CrossAxisAlignment.end,
                            children: [
                              KMoney(
                                p.amount,
                                style: context.text.label.copyWith(fontWeight: FontWeight.w600, color: k.fg),
                              ),
                              const SizedBox(height: 4),
                              PayoutStatusChip(p.status),
                            ],
                          ),
                        ],
                      ),
                    ),
                  ),
                ],
              if (pages > 1) ...[
                const SizedBox(height: 10),
                PartnerPager(
                  from: page * _pageSize + 1,
                  to: math.min(data.items.length, (page + 1) * _pageSize),
                  total: data.items.length,
                  page: page + 1,
                  pages: pages,
                  onPrev: page == 0 ? null : () => setState(() => _page = page - 1),
                  onNext: page >= pages - 1 ? null : () => setState(() => _page = page + 1),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }

  /// A batch with the web table's phone-hidden columns (lines, paid at, destination).
  void _openPayout(BuildContext context, PPayout p) {
    final t = context.t;
    final pf = PartnerFmt(t);
    showKSheet<void>(
      context,
      title: '${t('partner.pay.batch')} #${p.batchId}',
      builder: (_) => KSheetContent(
        children: [
          KKeyValues([
            KKV(t('partner.pay.batch'), '#${p.batchId}', mono: true),
            KKV(t('common.date'), _periodOf(pf, p.schedule, p.periodEnd, p.periodStart)),
            KKV(t('partner.pay.lines'), '${p.lines}', mono: true),
            KKV(t('common.amount'), Fmt.money(p.amount), mono: true),
            KKV(t('common.status'), null, valueWidget: PayoutStatusChip(p.status)),
            KKV(t('partner.commissionStatus.paid'), p.paidAt != null ? pf.dateTime(p.paidAt) : '—', mono: true),
            KKV(t('partner.pay.destination'), p.destination),
          ]),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ hero */

class _Hero extends StatelessWidget {
  const _Hero({required this.d});
  final PPayouts d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final current = _periodOf(pf, d.schedule, d.nextClose);
    final below = d.unbatched > 0 && d.unbatched < d.minAmount;
    return KCard(
      hot: true,
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            t('partner.pay.accruingNow', {'period': current}),
            style: context.text.caption.copyWith(color: k.ember, fontWeight: FontWeight.w600),
          ),
          const SizedBox(height: 8),
          KMoney(d.unbatched, style: context.text.moneyXL.copyWith(fontSize: 38, fontWeight: FontWeight.w600)),
          const SizedBox(height: 8),
          Text(t('partner.pay.accruingHint'), style: context.text.footnote.copyWith(color: k.fg2)),
          const SizedBox(height: 12),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: KChip(label: t('partner.schedulePayouts', {'schedule': scheduleLabel(t, d.schedule)}), tone: KChipTone.gold),
          ),
          const SizedBox(height: 20),
          Row(
            children: [
              Icon(LucideIcons.calendarClock, size: 16, color: k.ember),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  t('partner.batchCloses', {'date': '${pf.day(d.nextClose)}, ${pf.time(d.nextClose)}'}),
                  style: context.text.footnote.copyWith(color: k.fg2),
                ),
              ),
            ],
          ),
          const SizedBox(height: 10),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: _Countdown(target: d.nextClose),
          ),
          const SizedBox(height: 16),
          Text(
            below ? t('partner.pay.belowMin', {'amount': money0(d.minAmount)}) : t('partner.pay.minNote', {'amount': money0(d.minAmount)}),
            style: context.text.caption.copyWith(fontSize: 12, color: k.fg3, fontWeight: FontWeight.w400, height: 1.4),
          ),
        ],
      ),
    );
  }
}

/// Days : hours : minutes to the batch close, refreshed every 30 seconds (web Countdown).
class _Countdown extends StatefulWidget {
  const _Countdown({required this.target});
  final String target;

  @override
  State<_Countdown> createState() => _CountdownState();
}

class _CountdownState extends State<_Countdown> {
  Timer? _timer;

  @override
  void initState() {
    super.initState();
    _timer = Timer.periodic(const Duration(seconds: 30), (_) {
      if (mounted) setState(() {});
    });
  }

  @override
  void dispose() {
    _timer?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final target = DateTime.tryParse(widget.target);
    final left = target == null ? null : math.max(0, target.difference(DateTime.now()).inMilliseconds);
    final parts = left == null ? ['--', '--', '--'] : [left ~/ _dayMs, (left ~/ 3600000) % 24, (left ~/ 60000) % 60].map((v) => '$v'.padLeft(2, '0')).toList();
    const labels = ['partner.pay.days', 'partner.pay.hrs', 'partner.pay.min'];
    return Directionality(
      textDirection: TextDirection.ltr,
      child: Row(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var i = 0; i < 3; i++) ...[
            Column(
              children: [
                Container(
                  constraints: const BoxConstraints(minWidth: 48),
                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 8),
                  decoration: BoxDecoration(
                    color: k.surface2.withValues(alpha: 0.7),
                    borderRadius: BorderRadius.circular(12),
                    border: Border.all(color: k.line),
                  ),
                  child: Text(
                    parts[i],
                    textAlign: TextAlign.center,
                    style: context.text.mono(22, weight: FontWeight.w600, color: k.fg).copyWith(height: 1),
                  ),
                ),
                const SizedBox(height: 4),
                Text(
                  t(labels[i]),
                  style: context.text.caption.copyWith(fontSize: 11, color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ],
            ),
            if (i < 2)
              Padding(
                padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 10),
                child: Text(':', style: context.text.mono(16, color: k.fg3)),
              ),
          ],
        ],
      ),
    );
  }
}

class _WalletNote extends ConsumerWidget {
  const _WalletNote();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final walletOn = ref.watch(configProvider).moduleOn('wallet');
    final steps = [
      (LucideIcons.layers, t('partner.pay.accrues'), t('partner.pay.accruesText')),
      (LucideIcons.gavel, t('partner.pay.closesReviewed'), t('partner.pay.closesReviewedText')),
      (LucideIcons.banknote, t('partner.pay.creditedYour'), t('partner.pay.creditedYourText')),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('partner.pay.whereTitle'), subtitle: t('partner.pay.whereSubtitle'), icon: LucideIcons.wallet),
          const SizedBox(height: 16),
          for (final (icon, title, text) in steps)
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  RoundIcon(icon, size: 32, bg: k.surface2, border: true),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(title, style: context.text.label.copyWith(color: k.fg)),
                        Text(
                          text,
                          style: context.text.caption.copyWith(fontSize: 12, color: k.fg3, fontWeight: FontWeight.w400),
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
          if (walletOn) ...[
            const SizedBox(height: 4),
            KButton(
              label: t('partner.pay.openWallet'),
              icon: LucideIcons.wallet,
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              expand: true,
              onPressed: () => context.go('/wallet'),
            ),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ schedule, history */

class _ScheduleCard extends StatelessWidget {
  const _ScheduleCard({required this.d});
  final PPayouts d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final period = _periodOf(pf, d.schedule, d.nextClose);
    final steps = [
      (
        LucideIcons.layers,
        t('partner.pay.accrues'),
        t.dyn('partner.pay.through.${d.schedule}', fallback: t('partner.pay.through.period', {'period': period}), vars: {'period': period}),
        true,
      ),
      (LucideIcons.clock, t('partner.pay.batchCloses'), '${pf.day(d.nextClose)}, ${pf.time(d.nextClose)}', false),
      (LucideIcons.gavel, t('partner.pay.brokerApproval'), t('partner.pay.brokerApprovalText'), false),
      (LucideIcons.wallet, t('partner.pay.creditedWallet'), t('partner.pay.creditedWalletText'), false),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('partner.pay.scheduleTitle'),
            subtitle: t('partner.pay.scheduleSubtitle', {'schedule': scheduleLabel(t, d.schedule)}),
            icon: LucideIcons.calendarClock,
          ),
          const SizedBox(height: 18),
          for (final (i, (icon, title, text, current)) in steps.indexed)
            Stack(
              children: [
                if (i < steps.length - 1) PositionedDirectional(start: 17.5, top: 38, bottom: 2, child: Container(width: 1, color: k.line)),
                Padding(
                  padding: EdgeInsets.only(bottom: i < steps.length - 1 ? 18 : 0),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      RoundIcon(icon, size: 36, bg: current ? k.emberSoft : k.surface2, fg: current ? k.ember : k.fg3, border: true),
                      const SizedBox(width: 14),
                      Expanded(
                        child: Padding(
                          padding: const EdgeInsets.only(top: 3),
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Wrap(
                                spacing: 8,
                                crossAxisAlignment: WrapCrossAlignment.center,
                                children: [
                                  Text(title, style: context.text.label.copyWith(fontSize: 13.5, color: k.fg)),
                                  if (current) KChip(label: t('partner.pay.now'), tone: KChipTone.ember, small: true),
                                ],
                              ),
                              const SizedBox(height: 2),
                              Text(
                                text,
                                style: context.text.caption.copyWith(fontSize: 12, color: k.fg3, fontWeight: FontWeight.w400),
                              ),
                            ],
                          ),
                        ),
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

class _HistoryChart extends StatelessWidget {
  const _HistoryChart({required this.items, required this.schedule});
  final List<PPayout> items;
  final String schedule;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final pf = PartnerFmt(t);
    final data = items.where((p) => p.status != 'rejected').take(12).toList().reversed.toList();
    final avg = data.isEmpty ? 0.0 : data.fold<double>(0, (s, p) => s + p.amount) / data.length;
    String label(PPayout p) {
      final end = DateTime.tryParse(p.periodEnd)?.millisecondsSinceEpoch ?? 0;
      return '${pf.utcDay(DateTime.fromMillisecondsSinceEpoch(end - _dayMs, isUtc: true))} #${p.id}';
    }

    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('partner.pay.recent'),
            subtitle: data.isNotEmpty
                ? t.dyn('partner.pay.avgPer.$schedule', fallback: t('partner.pay.avgPer.batch', {'amount': Fmt.money(avg)}), vars: {'amount': Fmt.money(avg)})
                : t('partner.pay.last12'),
          ),
          const SizedBox(height: 22),
          if (data.length >= 2)
            CapsuleBars(labels: [for (final p in data) label(p)], values: [for (final p in data) p.amount], format: money0)
          else
            CardEmpty(height: 190, title: data.isNotEmpty ? t('partner.pay.onePayout') : t('partner.pay.noPayouts'), text: t('partner.pay.historyEmptyText')),
        ],
      ),
    );
  }
}

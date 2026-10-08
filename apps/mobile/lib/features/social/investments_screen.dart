// Copy & PAMM › My investments (/social/investments): KPIs, holdings (redeem, stop loss, add funds, statement,
// pending requests with cancel), allocation by fund and every request. Port of the phone layout of
// apps/crm/components/social-live/investments.tsx (LiveInvestmentsPage, GET investments every 10 s + GET funds).
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../i18n/i18n.dart';
import '../../shell/nav.dart';
import '../../ui/ui.dart';
import 'social_api.dart';
import 'widgets/bits.dart';
import 'widgets/copy_sheets.dart';
import 'widgets/follow_sheet.dart';
import 'widgets/fund_sheet.dart';
import 'widgets/invest_sheet.dart';

const Map<String, KChipTone> _reqTone = {'pending': KChipTone.warn, 'done': KChipTone.up, 'rejected': KChipTone.down, 'cancelled': KChipTone.neutral};

String _requestText(T t, RequestView r) {
  if (r.kind == 'invest') return usd(r.amount ?? 0);
  if (r.all) return t('social.inv.allUnits');
  if (r.units != null) return t('social.inv.unitsValue', {'units': units4(r.units)});
  return usd(r.amount ?? 0);
}

class InvestmentsScreen extends ConsumerStatefulWidget {
  const InvestmentsScreen({super.key});

  @override
  ConsumerState<InvestmentsScreen> createState() => _InvestmentsScreenState();
}

class _InvestmentsScreenState extends ConsumerState<InvestmentsScreen> {
  int? _cancelling;

  void _reload() => ref.invalidate(investmentsProvider);

  Future<void> _cancel(RequestView r) async {
    final t = context.t;
    setState(() => _cancelling = r.id);
    try {
      await socialPost(ref, 'requests/${r.id}/cancel');
      okToast(
        ref,
        t('social.inv.toast.cancelled'),
        r.kind == 'invest' ? t('social.inv.toast.backToWallet', {'amount': usd(r.amount ?? 0)}) : t('social.inv.toast.unitsStay'),
      );
      _reload();
    } on ApiException catch (e) {
      if (mounted) errToast(ref, context, t('social.inv.toast.cancelFailed'), e);
    } finally {
      if (mounted) setState(() => _cancelling = null);
    }
  }

  void _add(int fundId) => showInvestSheet(context, fundId: fundId, onDone: _reload);

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final q = ref.watch(investmentsProvider);
    final funds = ref.watch(fundsProvider).value ?? const <FundView>[];
    final copyOn = pageOn(ref.watch(configProvider), '/social/copy');
    final data = q.value;
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.72;

    final children = <Widget>[
      KPageHeader(title: t('social.myInvestments'), subtitle: Text(t('social.inv.subtitle'))),
      const SizedBox(height: 14),
      Wrap(
        spacing: 8,
        runSpacing: 8,
        children: [
          if (copyOn)
            KButton(
              label: t('social.inv.copySubscriptions'),
              icon: LucideIcons.repeat,
              variant: KButtonVariant.surface,
              onPressed: () => context.go('/social/copy'),
            ),
          KButton(label: t('social.funds.title'), icon: LucideIcons.landmark, onPressed: () => context.go('/social/pamm')),
        ],
      ),
      const SizedBox(height: 18),
    ];

    if (q.hasError && data == null) {
      children.add(SocialErrorCard(error: q.error, onRetry: _reload));
    } else if (data == null) {
      children.add(const BlockSkeleton(h: 140));
    } else if (data.items.isEmpty && data.requests.isEmpty) {
      children.add(
        KCard(
          child: KEmptyState(
            art: KIllustrationName.pammFunds,
            title: t('social.inv.empty.title'),
            text: t('social.inv.empty.text'),
            action: KButton(
              label: t('social.inv.browseFunds'),
              icon: LucideIcons.compass,
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => context.go('/social/pamm'),
            ),
          ),
        ),
      );
    } else {
      final items = data.items;
      final requests = data.requests;
      final value = items.fold<double>(0, (s, i) => s + i.value);
      final invested = items.fold<double>(0, (s, i) => s + i.netInvested);
      final pnl = items.fold<double>(0, (s, i) => s + i.pnl);
      final fees = items.fold<double>(0, (s, i) => s + i.feesPaid);
      final pendingN = requests.where((r) => r.status == 'pending').length;
      final names = <int, String>{for (final f in funds) f.id: f.name, for (final i in items) i.fundId: i.fund.name};
      children.addAll([
        SizedBox(
          height: 160,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('social.inv.kpi.currentValue'),
                icon: LucideIcons.landmark,
                value: KMoney(value, style: context.text.moneyL),
                chip: KChip(label: t('social.inv.kpi.currentValueChip', {'count': items.length})),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.inv.netInvested'),
                icon: LucideIcons.wallet,
                value: KMoney(invested, style: context.text.moneyL),
                chip: KChip(label: t('social.inv.kpi.netInvestedChip')),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.pnl'),
                icon: LucideIcons.repeat,
                value: KMoney(pnl, signed: true, tone: KMoneyTone.auto, style: context.text.moneyL),
                chip: KChip(label: invested > 0 ? pct(pnl / invested * 100) : '—', tone: pnl >= 0 ? KChipTone.up : KChipTone.down),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.inv.kpi.feesPaid'),
                icon: LucideIcons.shieldAlert,
                value: KMoney(fees, style: context.text.moneyL),
                chip: KChip(label: t('social.inv.kpi.feesPaidChip')),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.inv.kpi.pending'),
                icon: LucideIcons.calendarClock,
                value: Num('$pendingN', style: context.text.moneyL),
                chip: KChip(label: t('social.inv.kpi.pendingChip'), tone: pendingN > 0 ? KChipTone.warn : KChipTone.neutral),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: t('social.inv.holdings'),
          subtitle: t('social.inv.holdingsSub'),
          icon: LucideIcons.landmark,
          child: items.isEmpty
              ? EmptyRow(t('social.inv.noUnits'))
              : Column(
                  children: [
                    for (final inv in items) ...[
                      _HoldingRow(
                        inv: inv,
                        readOnly: readOnly,
                        cancelling: _cancelling,
                        onCancel: _cancel,
                        onOpen: () => showFundSheet(context, inv.fundId, onInvest: readOnly ? null : _add),
                        onRedeem: () => showKSheet<void>(
                          context,
                          builder: (_) => _RedeemSheet(inv: inv, onDone: _reload),
                        ),
                        onStopLoss: () => showKSheet<void>(
                          context,
                          builder: (_) => _StopLossSheet(inv: inv, onDone: _reload),
                        ),
                        onStatement: () => showKSheet<void>(context, expand: true, builder: (_) => _StatementSheet(fund: inv.fund)),
                        onAdd: () => _add(inv.fundId),
                      ),
                      const SizedBox(height: 10),
                    ],
                  ],
                ),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: t('social.allocation'),
          subtitle: t('social.inv.byFund'),
          child: items.isEmpty || value <= 0
              ? Text(t('social.inv.allocationEmpty'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13))
              : Column(
                  children: [
                    KDonut(
                      size: 170,
                      thickness: 18,
                      segments: [for (var i = 0; i < items.length; i++) (items[i].value, kChartColors[i % kChartColors.length])],
                      center: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Num(usd(value, 0), style: context.text.headline),
                          Text(
                            t('social.inv.totalValue'),
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(height: 14),
                    for (var i = 0; i < items.length; i++) ...[
                      if (i > 0) const SizedBox(height: 6),
                      LegendRow(
                        color: kChartColors[i % kChartColors.length],
                        label: items[i].fund.name,
                        value: '${(items[i].value / value * 100).toStringAsFixed(1)}%',
                      ),
                    ],
                  ],
                ),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: t('social.inv.requests'),
          subtitle: t('social.inv.requestsSub'),
          icon: LucideIcons.calendarClock,
          child: RequestRows(requests: requests, names: names, onCancel: readOnly ? null : _cancel, busy: _cancelling),
        ),
      ]);
    }

    return KPageScroll(
      onRefresh: () async {
        _reload();
        ref.invalidate(fundsProvider);
        await ref.read(investmentsProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: children,
    );
  }
}

/// The request rows (web RequestsTable): created, fund, kind, amount, status (or Pending + Cancel).
class RequestRows extends StatelessWidget {
  const RequestRows({super.key, required this.requests, this.names, this.onCancel, this.busy});
  final List<RequestView> requests;
  final Map<int, String>? names;
  final ValueChanged<RequestView>? onCancel;
  final int? busy;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    if (requests.isEmpty) return EmptyRow(t('social.inv.noRequests'));
    final sorted = [...requests]..sort((a, b) => b.createdAt.compareTo(a.createdAt));
    return RowsBox(
      children: [
        for (final r in sorted)
          DataLine(
            title: Text(
              '${t.dyn('social.inv.kind.${r.kind}', fallback: r.kind)}'
              '${names != null ? ' · ${names![r.fundId] ?? t('social.inv.fundNo', {'id': r.fundId})}' : ''}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
            subtitle: Text(serverTime(t, r.createdAt, withYear: false)),
            trailing: Num(_requestText(t, r)),
            trailingSub: r.status == 'pending' && onCancel != null
                ? Row(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      KChip(label: t('common.pending'), tone: KChipTone.warn, small: true),
                      const SizedBox(width: 4),
                      KTextButton(label: busy == r.id ? t('social.inv.cancelling') : t('common.cancel'), onPressed: busy == r.id ? null : () => onCancel!(r)),
                    ],
                  )
                : KChip(
                    label: t.dyn('social.inv.reqStatus.${r.status}', fallback: r.status),
                    tone: _reqTone[r.status] ?? KChipTone.neutral,
                    small: true,
                  ),
          ),
      ],
    );
  }
}

class _HoldingRow extends StatelessWidget {
  const _HoldingRow({
    required this.inv,
    required this.readOnly,
    required this.cancelling,
    required this.onCancel,
    required this.onOpen,
    required this.onRedeem,
    required this.onStopLoss,
    required this.onStatement,
    required this.onAdd,
  });
  final InvestmentView inv;
  final bool readOnly;
  final int? cancelling;
  final ValueChanged<RequestView> onCancel;
  final VoidCallback onOpen, onRedeem, onStopLoss, onStatement, onAdd;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = inv.fund;
    final figures = <(String, Widget)>[
      (t('social.inv.units'), Num(units4(inv.units))),
      ('NAV', Num(nav4(inv.nav))),
      (t('social.value'), Num(usd(inv.value), style: const TextStyle(fontWeight: FontWeight.w700))),
      (t('social.inv.netInvested'), Num(usd(inv.netInvested))),
      (t('social.pnl'), Num(usd(inv.pnl, 2, true), color: toneColor(context, inv.pnl))),
      (t('social.pnlPct'), Num(pct(inv.pnlPct), color: toneColor(context, inv.pnlPct))),
      ('HWM NAV', Num(nav4(inv.hwmNav))),
      (
        t('social.invest.stopLoss'),
        KPressable(
          minSize: 24,
          onTap: readOnly ? null : onStopLoss,
          child: Text(
            inv.stopLossPct != null ? '-${numText(inv.stopLossPct!)}%' : t('common.off'),
            style: TextStyle(color: inv.stopLossPct != null ? k.down : k.fg, decoration: TextDecoration.underline, decorationStyle: TextDecorationStyle.dotted),
          ),
        ),
      ),
    ];
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 14, 14, 12),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KPressable(
            pressedScale: 1,
            onTap: onOpen,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(f.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.headline),
                Text(
                  t('social.inv.holdingSub', {
                    'name': f.masterName,
                    'period': periodLabel(t, f.period).toLowerCase(),
                    'next': serverTime(t, f.nextRolloverAt, withYear: false),
                  }),
                  style: context.text.footnote.copyWith(color: k.fg3),
                ),
              ],
            ),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              if (inv.locked)
                KChip(label: t('social.inv.lockedTo', {'date': fmtDate(t, inv.lockedUntil)}), tone: KChipTone.warn, icon: LucideIcons.lock, small: true),
              FundStatusChip(status: f.status),
            ],
          ),
          const SizedBox(height: 10),
          const KDivider(),
          const SizedBox(height: 10),
          KStatGrid(spacing: 10, items: figures),
          if (inv.pending.isNotEmpty) ...[
            const SizedBox(height: 10),
            for (final p in inv.pending)
              Container(
                margin: const EdgeInsets.only(bottom: 6),
                padding: const EdgeInsets.fromLTRB(10, 8, 6, 8),
                decoration: BoxDecoration(
                  color: k.surface.withValues(alpha: 0.6),
                  borderRadius: BorderRadius.circular(12),
                  border: Border.all(color: k.line),
                ),
                child: Row(
                  children: [
                    Icon(LucideIcons.calendarClock, size: 14, color: k.ember),
                    const SizedBox(width: 6),
                    KChip(
                      label: p.kind == 'invest' ? t('social.invest') : t('social.inv.redeem'),
                      tone: p.kind == 'invest' ? KChipTone.up : KChipTone.warn,
                      small: true,
                    ),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Num(_requestText(t, p), style: context.text.footnote.copyWith(fontWeight: FontWeight.w600)),
                          Text(
                            t('social.inv.pendingUntil', {'next': serverTime(t, f.nextRolloverAt, withYear: false)}),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                          ),
                        ],
                      ),
                    ),
                    if (!readOnly)
                      KTextButton(
                        label: cancelling == p.id ? t('social.inv.cancelling') : t('social.inv.cancelRequest'),
                        color: k.fg3,
                        onPressed: cancelling == p.id ? null : () => onCancel(p),
                      ),
                  ],
                ),
              ),
          ],
          const SizedBox(height: 10),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            alignment: WrapAlignment.end,
            children: [
              KButton(
                label: t('social.inv.statement'),
                icon: LucideIcons.fileText,
                variant: KButtonVariant.ghost,
                size: KButtonSize.sm,
                onPressed: onStatement,
              ),
              if (!readOnly) ...[
                KButton(
                  label: t('social.invest.stopLoss'),
                  icon: LucideIcons.shieldAlert,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: onStopLoss,
                ),
                KButton(
                  label: t('social.inv.addFunds'),
                  icon: LucideIcons.plus,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: f.status != 'active' ? null : onAdd,
                ),
                KButton(
                  label: t('social.inv.redeem'),
                  icon: LucideIcons.arrowDownToLine,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: inv.units <= 0 ? null : onRedeem,
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ redeem */

class _RedeemSheet extends ConsumerStatefulWidget {
  const _RedeemSheet({required this.inv, required this.onDone});
  final InvestmentView inv;
  final VoidCallback onDone;

  @override
  ConsumerState<_RedeemSheet> createState() => _RedeemSheetState();
}

class _RedeemSheetState extends ConsumerState<_RedeemSheet> {
  String _by = 'amount';
  final _val = TextEditingController();
  bool _busy = false;

  @override
  void dispose() {
    _val.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final inv = widget.inv;
    final f = inv.fund;
    final v = parseAmount(_val.text) ?? 0;
    final units = _by == 'all' ? inv.units : (_by == 'units' ? v : (f.nav > 0 ? v / f.nav : 0.0));
    final err = _by == 'all' ? null : (!(v > 0) ? t('social.inv.err.enterValue') : (units > inv.units + 1e-8 ? t('social.inv.err.moreThanHeld') : null));
    final shown = units < inv.units ? units : inv.units;

    Future<void> submit() async {
      if (err != null) return plainError(ref, err);
      setState(() => _busy = true);
      try {
        await socialPost(ref, 'funds/${f.id}/redeem', switch (_by) {
          'all' => {'all': true},
          'units' => {'units': v},
          _ => {'amount': v},
        });
        okToast(ref, t('social.inv.toast.redeemQueued'), t('social.inv.toast.redeemQueuedDesc', {'next': serverTime(t, f.nextRolloverAt)}));
        widget.onDone();
        if (context.mounted) Navigator.of(context).pop();
      } on ApiException catch (e) {
        if (context.mounted) errToast(ref, context, t('social.inv.toast.redeemFailed'), e);
      } finally {
        if (mounted) setState(() => _busy = false);
      }
    }

    final (bg, _, border) = context.k.chip(KChipTone.gold);
    return KSheetContent(
      footer: SheetFooter(label: t('social.inv.redeem.queue'), busy: _busy, onPressed: err != null ? null : submit),
      children: [
        SheetTitle(title: t('social.inv.redeem.title', {'name': f.name}), description: t('social.inv.redeem.description')),
        TileGrid(
          columns: 3,
          gap: 6,
          tiles: [
            Tile(label: t('social.inv.unitsHeld'), value: Num(units4(inv.units))),
            Tile(label: t('social.funds.col.navUnit'), value: Num(nav4(f.nav))),
            Tile(label: t('social.value'), value: Num(usd(inv.value))),
          ],
        ),
        if (inv.locked) ...[
          const SizedBox(height: 12),
          InfoBox(
            tone: KChipTone.warn,
            icon: LucideIcons.lock,
            child: KRichText(t('social.inv.redeem.locked', {'date': fmtDate(t, inv.lockedUntil)}), tags: const {'b': KTag()}),
          ),
        ],
        const SizedBox(height: 14),
        KSegmented<String>(
          plain: true,
          values: const ['amount', 'units', 'all'],
          labels: [t('social.inv.redeem.byAmount'), t('social.inv.redeem.byUnits'), t('social.inv.redeem.everything')],
          selected: _by,
          onChanged: (x) => setState(() {
            _by = x;
            _val.clear();
          }),
        ),
        if (_by != 'all') ...[
          const SizedBox(height: 14),
          NumberField(
            controller: _val,
            label: _by == 'amount' ? t('social.inv.amountUsd') : t('social.inv.units'),
            dollar: _by == 'amount',
            unit: _by == 'amount' ? 'USD' : t('social.inv.unitsUnit'),
            error: _val.text.isNotEmpty ? err : null,
            onChanged: (_) => setState(() {}),
          ),
        ],
        const SizedBox(height: 14),
        Container(
          padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(14),
            border: Border.all(color: border),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(t('social.inv.redeem.unitsToRedeem'), style: context.text.callout.copyWith(color: context.k.fg2)),
                  ),
                  Num(units4(shown), style: context.text.callout.copyWith(fontWeight: FontWeight.w600)),
                ],
              ),
              const SizedBox(height: 4),
              Row(
                children: [
                  Expanded(
                    child: Text(t('social.inv.redeem.estPayout'), style: context.text.callout.copyWith(color: context.k.fg2)),
                  ),
                  Num(usd(shown * f.nav), style: context.text.callout.copyWith(fontWeight: FontWeight.w700)),
                ],
              ),
              const SizedBox(height: 6),
              Text(
                t('social.inv.redeem.note', {'next': serverTime(t, f.nextRolloverAt)}),
                style: context.text.caption.copyWith(color: context.k.fg3, fontWeight: FontWeight.w400),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ stop loss */

class _StopLossSheet extends ConsumerStatefulWidget {
  const _StopLossSheet({required this.inv, required this.onDone});
  final InvestmentView inv;
  final VoidCallback onDone;

  @override
  ConsumerState<_StopLossSheet> createState() => _StopLossSheetState();
}

class _StopLossSheetState extends ConsumerState<_StopLossSheet> {
  late bool _on = widget.inv.stopLossPct != null;
  late double _sl = widget.inv.stopLossPct ?? 20;
  bool _busy = false;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final inv = widget.inv;

    Future<void> save() async {
      setState(() => _busy = true);
      try {
        await socialPatch(ref, 'investments/${inv.fundId}', {'stopLossPct': _on ? _sl.round() : null});
        okToast(ref, _on ? t('social.inv.sl.set', {'sl': _sl.round()}) : t('social.inv.sl.removed'));
        widget.onDone();
        if (context.mounted) Navigator.of(context).pop();
      } on ApiException catch (e) {
        if (context.mounted) errToast(ref, context, t('social.inv.sl.failed'), e);
      } finally {
        if (mounted) setState(() => _busy = false);
      }
    }

    return KSheetContent(
      footer: SheetFooter(label: t('common.save'), busy: _busy, onPressed: save),
      children: [
        SheetTitle(title: t('social.invest.sl'), description: inv.fund.name),
        SwitchBox(
          title: t('social.invest.stopLoss'),
          hint: t('social.inv.sl.text', {'amount': usd(inv.netInvested)}),
          on: _on,
          onChanged: (v) => setState(() => _on = v),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              TriggerLine(value: '-${_sl.round()}% · ${t('social.invest.atValue', {'amount': usd(inv.netInvested * (1 - _sl / 100), 0)})}'),
              SocialSlider(
                value: _sl,
                min: 5,
                max: 90,
                ticks: const [5, 10, 20, 50, 90],
                format: (v) => '${v.round()}%',
                enabled: _on,
                onChanged: (v) => setState(() => _sl = v),
              ),
            ],
          ),
        ),
        const SizedBox(height: 12),
        InfoBox(text: t('social.inv.sl.note')),
      ],
    );
  }
}

/* ------------------------------------------------------------------ statement */

class _StatementSheet extends ConsumerWidget {
  const _StatementSheet({required this.fund});
  final FundView fund;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final q = ref.watch(fundStatementProvider(fund.id));
    final data = q.value;
    final items = listOf(data?['items']);
    final requests = [for (final r in listOf(data?['requests'])) RequestView(r)];
    return KSheetContent(
      children: [
        SheetTitle(title: t('social.inv.statement'), description: t('social.inv.statementSub', {'name': fund.name})),
        if (data == null)
          q.hasError ? InfoBox(tone: KChipTone.down, text: socialError(q.error, t)) : const BlockSkeleton(n: 2, h: 100)
        else ...[
          GroupLabel(t('social.inv.unitLedger')),
          if (items.isEmpty)
            EmptyRow(t('social.inv.noMovements'))
          else
            RowsBox(
              pageSize: 15,
              children: [
                for (final i in items)
                  DataLine(
                    title: Text(t.dyn('social.inv.kind.${strOf(i['kind'])}', fallback: strOf(i['kind']))),
                    subtitle: Text(serverTime(t, strOrNull(i['at']))),
                    trailing: Num(usd(numOf(i['amount']))),
                    trailingSub: Num(
                      '${numOf(i['units']) > 0 ? '+' : ''}${units4(numOf(i['units']))}',
                      color: numOf(i['units']) > 0 ? context.k.up : (numOf(i['units']) < 0 ? context.k.down : null),
                    ),
                  ),
              ],
            ),
          const SizedBox(height: 16),
          GroupLabel(t('social.inv.requests')),
          RequestRows(requests: requests),
        ],
      ],
    );
  }
}

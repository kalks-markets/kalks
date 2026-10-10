// Partner › Dashboard (web components/partner/live/dashboard.tsx LivePartnerDashboard), in the phone order:
//   header (Payouts, Copy referral link) · level hero (progress, ladder, perks) · referral link (copy, QR, share,
//   funnel) · KPI cards · lifetime earnings chart · commission by week · top clients · recent commission · CPA bonus.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'partner_api.dart';
import 'widgets/partner_widgets.dart';

const int _day = 86400;

/// Seconds of the UTC midnight of `iso` (date-only strings are UTC days, as in JS Date.parse).
int? utcDaySec(String iso) {
  final d = DateTime.tryParse(iso.length == 10 ? '${iso}T00:00:00Z' : iso);
  if (d == null) return null;
  return (d.millisecondsSinceEpoch ~/ 1000) ~/ _day * _day;
}

int _todaySec() => (DateTime.now().millisecondsSinceEpoch ~/ 1000) ~/ _day * _day;

/// Daily points for the last 180 days (days without accruals carry the running total).
List<({int time, double value, double volume})> dailySeries(PDashboard d) {
  final today = _todaySec();
  final byDay = <int, ({double amount, double cumulative})>{};
  for (final s in d.series) {
    final t = utcDaySec(s.date);
    if (t != null) byDay[t] = (amount: s.amount, cumulative: s.cumulative);
  }
  final first = d.series.isEmpty ? null : d.series.first;
  var acc = first != null ? first.cumulative - first.amount : d.lifetime;
  return [
    for (var i = 179; i >= 0; i--)
      () {
        final t = today - i * _day;
        final hit = byDay[t];
        if (hit != null) acc = hit.cumulative;
        return (time: t, value: double.parse(acc.toStringAsFixed(2)), volume: hit?.amount ?? 0.0);
      }(),
  ];
}

/// The last `n` weeks (Monday UTC starts), zero-filled.
List<({int t, double value})> lastWeeks(PDashboard d, int n) {
  final today = _todaySec();
  final dow = DateTime.fromMillisecondsSinceEpoch(today * 1000, isUtc: true).weekday - 1;
  final monday = today - dow * _day;
  final byWeek = <int, double>{};
  for (final w in d.weekly) {
    final t = utcDaySec(w.week);
    if (t != null) byWeek[t] = w.amount;
  }
  return [for (var i = 0; i < n; i++) (t: monday - (n - 1 - i) * 7 * _day, value: byWeek[monday - (n - 1 - i) * 7 * _day] ?? 0)];
}

DateTime _utc(int sec) => DateTime.fromMillisecondsSinceEpoch(sec * 1000, isUtc: true);

class PartnerDashboardScreen extends ConsumerWidget {
  const PartnerDashboardScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final f = LocaleFormat(t.locale);
    final pf = PartnerFmt(t);
    final async = ref.watch(partnerDashboardProvider);
    final d = async.value;
    final hero = pageHero(context, ref, path: '/partner', title: t('partner.dash.title'), lead: t('partner.dash.subtitle'));
    if (d == null) {
      return PartnerPageFallback(
        hero: hero,
        title: t('partner.dash.title'),
        subtitle: t('partner.dash.subtitle'),
        error: async.hasError ? async.error : null,
        onRetry: () => ref.invalidate(partnerDashboardProvider),
        skeleton: const [380, 330, 180, 360],
      );
    }

    final link = referralLink(d.linkBase, d.code);
    final next = d.next;
    final mon = pf.monthName(d.month);
    final prevMon = pf.monthName(d.month, -1);
    final lotsCh = d.prevMonthLots > 0 ? (d.monthlyLots - d.prevMonthLots) / d.prevMonthLots * 100 : null;
    final due = d.pending + d.approved;
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.78;

    return KPageScroll(
      onRefresh: () async {
        ref.invalidate(partnerDashboardProvider);
        await ref.read(partnerDashboardProvider.future).then((_) {}, onError: (Object _) {});
      },
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        if (hero == null) ...[KPageHeader(title: t('partner.dash.title'), subtitle: Text(t('partner.dash.subtitle'))), const SizedBox(height: 14)],
        Row(
          children: [
            KButton(
              label: t('partner.payouts.title'),
              icon: LucideIcons.banknote,
              variant: KButtonVariant.surface,
              onPressed: () => context.go('/partner/payouts'),
            ),
            const SizedBox(width: 8),
            Flexible(
              child: KButton(
                label: t('partner.copyReferralLink'),
                icon: LucideIcons.copy,
                onPressed: () => partnerCopy(context, link, t('partner.toast.linkCopied'), description: shortUrl(link)),
              ),
            ),
          ],
        ),
        const SizedBox(height: 20),
        _LevelHero(d: d),
        kGap,
        _ReferralCard(d: d),
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
                label: t('partner.dash.totalReferrals'),
                icon: LucideIcons.userPlus,
                value: Text(f.number(d.referrals, 0)),
                chip: KChip(
                  label: t('partner.dash.plusThisMonth', {'n': d.referralsThisMonth}),
                  tone: d.referralsThisMonth > 0 ? KChipTone.up : KChipTone.neutral,
                ),
                onTap: () => context.go('/partner/clients'),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.dash.activeClients'),
                icon: LucideIcons.users,
                value: Text('${d.activeClients}'),
                chip: KChip(
                  label: next != null ? t('partner.dash.neededFor', {'n': next.minActiveClients, 'name': next.name}) : t('partner.dash.topLevel'),
                  tone: KChipTone.gold,
                ),
                onTap: () => context.go('/partner/network'),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.dash.networkLots', {'month': mon}),
                icon: LucideIcons.layers,
                value: Text(f.number(d.monthlyLots, 1)),
                chip: KChip(
                  label: lotsCh != null
                      ? t('partner.dash.vsMonth', {'pct': '${lotsCh >= 0 ? '+' : ''}${lotsCh.toStringAsFixed(1)}', 'month': prevMon})
                      : t('partner.dash.lotsInMonth', {'lots': pf.lots(d.prevMonthLots, 1), 'month': prevMon}),
                  tone: lotsCh == null
                      ? KChipTone.neutral
                      : lotsCh >= 0
                      ? KChipTone.up
                      : KChipTone.down,
                ),
                onTap: () => context.go('/partner/network'),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.pendingCommission'),
                icon: LucideIcons.coins,
                value: KMoney(due, style: context.text.moneyL),
                chip: KChip(label: t('partner.batchCloses', {'date': pf.day(d.programme.nextClose)}), tone: KChipTone.ember),
                onTap: () => context.go('/partner/payouts'),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('partner.paidAllTime'),
                icon: LucideIcons.banknote,
                value: KMoney(d.paid, style: context.text.moneyL),
                chip: KChip(label: t('partner.dash.intoWallet')),
                onTap: () => context.go('/partner/payouts'),
              ),
            ],
          ),
        ),
        kGap,
        _EarningsCard(d: d),
        kGap,
        _WeeklyCard(d: d),
        kGap,
        _TopClientsCard(d: d),
        kGap,
        _RecentCard(d: d),
        kGap,
        _CpaCard(d: d),
      ],
    );
  }
}

/* ------------------------------------------------------------------ level */

class _LevelHero extends StatelessWidget {
  const _LevelHero({required this.d});
  final PDashboard d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final levels = [...d.levels]..sort((a, b) => a.rank.compareTo(b.rank));
    final cur = d.level ?? (levels.isEmpty ? null : levels.first);
    final curIdx = math.max(0, levels.indexWhere((l) => l.key == cur?.key));
    final next = d.next;
    final clientsPct = next == null ? 100.0 : (next.minActiveClients > 0 ? d.activeClients / next.minActiveClients * 100 : 100.0);
    final lotsPct = next == null ? 100.0 : (next.minMonthlyLots > 0 ? d.monthlyLots / next.minMonthlyLots * 100 : 100.0);
    final ladderPct = levels.length > 1
        ? (curIdx + (next != null ? math.min(math.min(clientsPct, lotsPct), 100) / 100 : 0)) / (levels.length - 1) * 100
        : 100.0;
    final resets = pf.date(d.monthEnds, withYear: false);
    final suspended = d.status != 'active';
    final (icon, tone) = levelGlyph(cur?.icon ?? 'coin');

    return KCard(
      hot: true,
      padding: const EdgeInsets.all(18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              KIconTile(icon: icon, tone: tone, size: 56, iconSize: 26),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(
                      t('partner.dash.levelOf', {'n': curIdx + 1, 'total': levels.isEmpty ? 1 : levels.length}),
                      style: context.text.caption.copyWith(color: k.ember, fontWeight: FontWeight.w600),
                    ),
                    const SizedBox(height: 4),
                    Text(
                      cur?.name ?? t('partner.dash.partnerFallback'),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.largeTitle.copyWith(fontSize: 27, height: 1.05),
                    ),
                    const SizedBox(height: 6),
                    Wrap(
                      spacing: 6,
                      runSpacing: 2,
                      crossAxisAlignment: WrapCrossAlignment.center,
                      children: [
                        Text(t('partner.dash.since', {'date': pf.month(d.joinedAt)}), style: context.text.footnote.copyWith(color: k.fg2)),
                        Text('·', style: context.text.footnote.copyWith(color: k.fg3)),
                        Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(text: '${t('partner.dash.code')} '),
                              TextSpan(
                                text: d.code,
                                style: context.text.mono(12.5, color: k.fg),
                              ),
                            ],
                          ),
                          style: context.text.footnote.copyWith(color: k.fg2),
                        ),
                      ],
                    ),
                  ],
                ),
              ),
            ],
          ),
          const SizedBox(height: 14),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              if (suspended)
                KChip(label: t('partner.dash.suspended'), tone: KChipTone.down, dot: true)
              else
                KChip(
                  label: t('partner.schedulePayouts', {'schedule': scheduleLabel(t, d.programme.schedule)}),
                  tone: KChipTone.gold,
                  icon: LucideIcons.circleCheck,
                ),
              KButton(
                label: t('partner.dash.rateCard'),
                trailingIcon: Directionality.of(context) == TextDirection.rtl ? LucideIcons.chevronLeft : LucideIcons.chevronRight,
                variant: KButtonVariant.surface,
                size: KButtonSize.sm,
                onPressed: () => context.go('/partner/commissions'),
              ),
            ],
          ),
          if (suspended) ...[
            const SizedBox(height: 12),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 11),
              decoration: BoxDecoration(
                color: k.downSoft,
                borderRadius: BorderRadius.circular(14),
                border: Border.all(color: k.down.withValues(alpha: 0.3)),
              ),
              child: Text(t('partner.dash.suspendedNote'), style: context.text.footnote.copyWith(color: k.fg2)),
            ),
          ],
          const SizedBox(height: 16),
          _ProgressBlock(
            icon: LucideIcons.users,
            label: t('partner.dash.activeClients'),
            value: '${d.activeClients}',
            target: next == null ? null : '${next.minActiveClients}',
            pct: clientsPct,
            nextName: next?.name,
            hint: next != null
                ? (next.minActiveClients > d.activeClients
                      ? t('partner.dash.moreClients', {'count': next.minActiveClients - d.activeClients})
                      : t('partner.dash.targetMetMonth'))
                : t('partner.dash.clientsTraded'),
          ),
          const SizedBox(height: 12),
          _ProgressBlock(
            icon: LucideIcons.layers,
            label: t('partner.dash.monthlyLots'),
            value: pf.figure(d.monthlyLots),
            target: next == null ? null : pf.figure(next.minMonthlyLots),
            pct: lotsPct,
            nextName: next?.name,
            hint: next != null
                ? (next.minMonthlyLots > d.monthlyLots
                      ? t('partner.dash.lotsToGo', {'lots': pf.lots(next.minMonthlyLots - d.monthlyLots, 1), 'date': resets})
                      : t('partner.dash.targetMetResets', {'date': resets}))
                : t('partner.dash.allTiersResets', {'date': resets}),
          ),
          if (levels.length > 1) ...[
            const SizedBox(height: 18),
            _Ladder(levels: levels, curIdx: curIdx, pct: ladderPct),
            const SizedBox(height: 14),
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
              decoration: BoxDecoration(
                color: k.surface2.withValues(alpha: 0.6),
                borderRadius: BorderRadius.circular(14),
                border: Border.all(color: k.line),
              ),
              child: Wrap(
                spacing: 6,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  if (next != null) ...[
                    Text(t('partner.dash.atLevel', {'name': next.name}), style: context.text.footnote.copyWith(color: k.fg2)),
                    if (next.perks.isNotEmpty)
                      for (final p in next.perks) KChip(label: p, tone: KChipTone.gold, small: true)
                    else
                      Text(t('partner.dash.higherRates'), style: context.text.footnote.copyWith(color: k.fg3)),
                  ] else ...[
                    Text(t('partner.dash.topReached'), style: context.text.footnote.copyWith(color: k.fg2)),
                    for (final p in cur?.perks ?? const <String>[]) KChip(label: p, tone: KChipTone.gold, small: true),
                  ],
                ],
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _ProgressBlock extends StatelessWidget {
  const _ProgressBlock({required this.icon, required this.label, required this.value, this.target, required this.pct, this.nextName, required this.hint});
  final IconData icon;
  final String label, value, hint;
  final String? target, nextName;
  final double pct;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return Container(
      padding: const EdgeInsets.fromLTRB(16, 13, 16, 13),
      decoration: BoxDecoration(
        color: k.surface2.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Icon(icon, size: 15, color: k.fg2),
              const SizedBox(width: 7),
              Expanded(
                child: Text(
                  label,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.footnote.copyWith(color: k.fg2),
                ),
              ),
              if (target != null && nextName != null)
                Text(
                  t('partner.dash.pctTo', {'pct': math.min(100, pct.floor()), 'name': nextName}),
                  style: context.text.footnote.copyWith(color: k.fg3, fontFeatures: kTabular),
                ),
            ],
          ),
          const SizedBox(height: 8),
          Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              Text(value, textDirection: TextDirection.ltr, style: context.text.figure.copyWith(fontSize: 26, height: 1)),
              if (target != null) ...[
                const SizedBox(width: 6),
                Text(
                  '/ $target',
                  textDirection: TextDirection.ltr,
                  style: context.text.callout.copyWith(color: k.fg3, fontFeatures: kTabular),
                ),
              ],
            ],
          ),
          if (target != null) ...[
            const SizedBox(height: 11),
            KProgressBar(
              value: pct / 100,
              height: 8,
              color: pct >= 100
                  ? k.up
                  : pct > 70
                  ? k.gold
                  : k.ember,
            ),
          ],
          const SizedBox(height: 8),
          Text(
            hint,
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
        ],
      ),
    );
  }
}

/// The level ladder: one stop per level, the line filled up to the progress towards the next level.
class _Ladder extends StatelessWidget {
  const _Ladder({required this.levels, required this.curIdx, required this.pct});
  final List<PLevel> levels;
  final int curIdx;
  final double pct;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final n = levels.length;
    return LayoutBuilder(
      builder: (context, c) {
        final slot = c.maxWidth / n;
        final span = c.maxWidth - slot;
        return Stack(
          children: [
            PositionedDirectional(
              start: slot / 2,
              end: slot / 2,
              top: 19.5,
              child: Container(height: 1, color: k.line),
            ),
            PositionedDirectional(
              start: slot / 2,
              top: 19,
              child: Container(
                width: math.min(100, pct) / 100 * span,
                height: 2,
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(1),
                  gradient: LinearGradient(colors: [k.ember, k.gold], begin: AlignmentDirectional.centerStart, end: AlignmentDirectional.centerEnd),
                ),
              ),
            ),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (var i = 0; i < n; i++)
                  Expanded(
                    child: Tooltip(
                      message: levels[i].rank == 1
                          ? t('partner.dash.levelStarting', {'name': levels[i].name})
                          : t('partner.dash.levelNeeds', {
                              'name': levels[i].name,
                              'clients': levels[i].minActiveClients,
                              'lots': PartnerFmt(t).figure(levels[i].minMonthlyLots, 2),
                            }),
                      triggerMode: TooltipTriggerMode.tap,
                      child: Column(
                        children: [
                          Container(
                            width: 40,
                            height: 40,
                            alignment: Alignment.center,
                            decoration: BoxDecoration(
                              color: k.surface,
                              shape: BoxShape.circle,
                              border: Border.all(
                                color: i == curIdx
                                    ? k.ember.withValues(alpha: 0.6)
                                    : i < curIdx
                                    ? k.gold.withValues(alpha: 0.4)
                                    : k.line,
                              ),
                            ),
                            child: Icon(
                              levelGlyph(levels[i].icon).$1,
                              size: 18,
                              color: i == curIdx
                                  ? k.ember
                                  : i < curIdx
                                  ? k.gold
                                  : k.fg3,
                            ),
                          ),
                          const SizedBox(height: 6),
                          Text(
                            levels[i].name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.caption.copyWith(
                              color: i == curIdx
                                  ? k.fg
                                  : i < curIdx
                                  ? k.gold
                                  : k.fg3,
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
              ],
            ),
          ],
        );
      },
    );
  }
}

/* ------------------------------------------------------------------ referral link */

class _ReferralCard extends StatelessWidget {
  const _ReferralCard({required this.d});
  final PDashboard d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = LocaleFormat(t.locale);
    final link = referralLink(d.linkBase, d.code);
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('partner.dash.referralTitle'), subtitle: t('partner.dash.referralSubtitle'), icon: LucideIcons.link2),
          const SizedBox(height: 16),
          // the partner's visiting card (web partner/live/dashboard.tsx, founder 2026-10-10): name, level, link, code
          KVisitingCard(
            finish: KVisitingFinish.orange,
            kicker: t.dyn('partner.card.kicker', fallback: 'Kalks Partner'),
            name: d.name,
            title: '${d.level?.name ?? t('partner.dash.partnerFallback')} · ${t('partner.dash.since', {'date': PartnerFmt(t).month(d.joinedAt)})}',
            footer: Row(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        shortUrl(link),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        textDirection: TextDirection.ltr,
                        style: context.text.mono(12.5, weight: FontWeight.w600, color: Colors.white),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        t('partner.referralLink').toUpperCase(),
                        style: context.text.micro.copyWith(color: Colors.white.withValues(alpha: 0.75), fontSize: 9, letterSpacing: 1.2),
                      ),
                    ],
                  ),
                ),
                const SizedBox(width: 12),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Text(
                      d.code,
                      textDirection: TextDirection.ltr,
                      style: context.text.mono(16, weight: FontWeight.w700, color: Colors.white).copyWith(letterSpacing: 1.4),
                    ),
                    const SizedBox(height: 2),
                    Text(
                      t('partner.referralCode').toUpperCase(),
                      style: context.text.micro.copyWith(color: Colors.white.withValues(alpha: 0.75), fontSize: 9, letterSpacing: 1.2),
                    ),
                  ],
                ),
              ],
            ),
          ),
          const SizedBox(height: 16),
          Container(
            padding: const EdgeInsetsDirectional.fromSTEB(14, 4, 6, 4),
            decoration: BoxDecoration(
              color: k.emberSoft.withValues(alpha: 0.6),
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: k.ember.withValues(alpha: 0.3)),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    shortUrl(link),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    textDirection: TextDirection.ltr,
                    textAlign: Directionality.of(context) == TextDirection.rtl ? TextAlign.right : TextAlign.left,
                    style: context.text.mono(13, color: k.fg),
                  ),
                ),
                PartnerCopyButton(value: link),
                KButton(
                  label: 'QR',
                  icon: LucideIcons.qrCode,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: () => showPartnerQrSheet(context, value: link, title: t('partner.dash.referralQr'), fileBase: 'kalks-${d.code}-qr'),
                ),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Container(
            padding: const EdgeInsetsDirectional.fromSTEB(14, 9, 6, 9),
            decoration: BoxDecoration(
              color: k.surface2,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: k.line),
            ),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        t('partner.referralCode'),
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        d.code,
                        textDirection: TextDirection.ltr,
                        style: context.text.mono(15, weight: FontWeight.w600, color: k.fg).copyWith(letterSpacing: 1.2),
                      ),
                    ],
                  ),
                ),
                PartnerCopyButton(value: d.code),
              ],
            ),
          ),
          const SizedBox(height: 14),
          Text(
            t('partner.dash.share'),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 8),
          ShareButtons(url: link, text: t('partner.dash.shareText')),
          const SizedBox(height: 14),
          Row(
            children: [
              for (final (i, (label, v)) in [
                (t('partner.clicks'), d.clicks),
                (t('partner.signups'), d.signups),
                (t('partner.firstDeposits'), d.ftds),
              ].indexed) ...[
                if (i > 0) const SizedBox(width: 8),
                Expanded(
                  child: PartnerRow(
                    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
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
                        Text(f.number(v, 0), style: context.text.figure.copyWith(fontSize: 15, fontWeight: FontWeight.w500)),
                      ],
                    ),
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 6),
          KPressable(
            onTap: () => context.go('/partner/links'),
            pressedScale: 1,
            child: SizedBox(
              height: 40,
              child: Row(
                children: [
                  Expanded(
                    child: Text(t('partner.dash.campaignLinks'), style: context.text.footnote.copyWith(color: k.fg2)),
                  ),
                  Icon(Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowUpLeft : LucideIcons.arrowUpRight, size: 16, color: k.fg2),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ earnings */

class _EarningsCard extends StatefulWidget {
  const _EarningsCard({required this.d});
  final PDashboard d;

  @override
  State<_EarningsCard> createState() => _EarningsCardState();
}

class _EarningsCardState extends State<_EarningsCard> {
  static const _ranges = {'1M': 30, '3M': 90, '6M': 180};
  String _range = '3M';

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final d = widget.d;
    final all = dailySeries(d);
    final data = all.sublist(all.length - _ranges[_range]!);
    final earned = data.fold<double>(0, (s, p) => s + p.volume);
    final shown = data.last;
    final empty = d.lifetime == 0 && d.series.isEmpty;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(t('partner.dash.lifetime'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 8),
          Wrap(
            spacing: 10,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              KMoney(shown.value, style: context.text.moneyL.copyWith(fontSize: 30)),
              if (!empty)
                KChip(
                  label: t('partner.dash.earnedIn', {'amount': '${earned >= 0 ? '+' : ''}${Fmt.money(earned)}', 'range': _range}),
                  tone: earned > 0 ? KChipTone.up : KChipTone.neutral,
                ),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            t('partner.dash.lifetimeHint'),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          if (!empty) ...[
            const SizedBox(height: 14),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: SizedBox(
                width: 180,
                child: KSegmented<String>(
                  values: const ['1M', '3M', '6M'],
                  labels: const ['1M', '3M', '6M'],
                  selected: _range,
                  height: 32,
                  onChanged: (v) => setState(() => _range = v),
                ),
              ),
            ),
          ],
          const SizedBox(height: 12),
          if (empty)
            CardEmpty(height: 220, title: t('partner.noCommission'), text: t('partner.dash.earningsEmpty'))
          else ...[
            KLineChart(
              values: [for (final p in data) p.value],
              labels: [for (final p in data) pf.utcDay(_utc(p.time))],
              height: 190,
              color: k.ember,
              format: Fmt.money,
            ),
            const SizedBox(height: 6),
            KBarChart(values: [for (final p in data) p.volume], height: 34, color: k.gold.withValues(alpha: 0.8)),
          ],
        ],
      ),
    );
  }
}

class _WeeklyCard extends StatelessWidget {
  const _WeeklyCard({required this.d});
  final PDashboard d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final data = lastWeeks(d, 8);
    final last = data.last, prev = data[data.length - 2];
    final ch = prev.value > 0 ? (last.value - prev.value) / prev.value * 100 : null;
    final any = data.any((w) => w.value != 0);
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('partner.dash.byWeek'),
            subtitle: t('partner.dash.byWeekSubtitle'),
            action: ch == null
                ? null
                : KChip(label: t('partner.dash.wow', {'pct': '${ch >= 0 ? '+' : ''}${ch.toStringAsFixed(1)}'}), tone: ch >= 0 ? KChipTone.up : KChipTone.down),
          ),
          const SizedBox(height: 14),
          KMoney(last.value, style: context.text.moneyL.copyWith(fontSize: 26)),
          const SizedBox(height: 4),
          Text(
            t('partner.dash.weekOf', {'week': pf.utcDay(_utc(last.t)), 'date': pf.day(d.programme.nextClose)}),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 18),
          if (any)
            CapsuleBars(labels: [for (final w in data) pf.utcDay(_utc(w.t))], values: [for (final w in data) math.max(0, w.value)], format: money0)
          else
            CardEmpty(height: 190, title: t('partner.dash.weeklyEmpty'), text: t('partner.dash.weeklyEmptyText')),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ clients & feed */

class _TopClientsCard extends StatelessWidget {
  const _TopClientsCard({required this.d});
  final PDashboard d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final top = d.topClients;
    final max = top.fold<double>(0.0001, (m, c) => math.max(m, c.lotsMonth));
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('partner.dash.topClients'),
            subtitle: t('partner.dash.topClientsSubtitle', {'month': pf.monthName(d.month)}),
            action: KButton(
              label: t('partner.dash.allClients'),
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => context.go('/partner/clients'),
            ),
          ),
          const SizedBox(height: 14),
          if (top.isEmpty)
            CardEmpty(title: t('partner.dash.topEmpty'), text: t('partner.dash.topEmptyText'))
          else
            for (final (i, c) in top.indexed) ...[
              if (i > 0) const SizedBox(height: 8),
              PartnerRow(
                onTap: () => context.go('/partner/clients'),
                child: Row(
                  children: [
                    SizedBox(
                      width: 16,
                      child: Text(
                        '${i + 1}',
                        textAlign: TextAlign.center,
                        style: context.text.mono(11, color: k.fg3),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: PersonCell(name: c.name, country: c.country),
                    ),
                    const SizedBox(width: 8),
                    SizedBox(
                      width: 92,
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.end,
                        children: [
                          Text(t('partner.lotsN', {'lots': pf.lots(c.lotsMonth)}), style: context.text.label.copyWith(fontFeatures: kTabular)),
                          const SizedBox(height: 5),
                          KProgressBar(value: c.lotsMonth / max, color: k.gold, height: 4),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ],
        ],
      ),
    );
  }
}

class _RecentCard extends StatelessWidget {
  const _RecentCard({required this.d});
  final PDashboard d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pf = PartnerFmt(t);
    final events = d.recent.take(7).toList();
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('partner.dash.recent'),
            subtitle: t('partner.dash.recentSubtitle'),
            action: KButton(
              label: t('partner.dash.ledger'),
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              onPressed: () => context.go('/partner/commissions'),
            ),
          ),
          const SizedBox(height: 14),
          if (events.isEmpty)
            CardEmpty(title: t('partner.noCommission'), text: t('partner.dash.recentEmptyText'))
          else
            for (final (i, e) in events.indexed) ...[
              if (i > 0) const SizedBox(height: 8),
              PartnerRow(
                child: Row(
                  children: [
                    Expanded(
                      child: PersonCell(name: e.clientName, country: e.clientCountry, sub: '${commissionLine(t, pf, e)} · ${pf.rel(e.createdAt)}'),
                    ),
                    const SizedBox(width: 8),
                    Column(
                      crossAxisAlignment: CrossAxisAlignment.end,
                      children: [
                        Text(
                          '${e.amount >= 0 ? '+' : '-'}${Fmt.money(e.amount.abs())}',
                          textDirection: TextDirection.ltr,
                          style: context.text.label.copyWith(
                            fontSize: 13.5,
                            fontWeight: FontWeight.w600,
                            color: e.amount >= 0 ? k.up : k.down,
                            fontFeatures: kTabular,
                          ),
                        ),
                        const SizedBox(height: 4),
                        CommissionStatusChip(e.status),
                      ],
                    ),
                  ],
                ),
              ),
            ],
        ],
      ),
    );
  }
}

class _CpaCard extends StatelessWidget {
  const _CpaCard({required this.d});
  final PDashboard d;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final p = d.programme;
    final cur = d.level;
    final better =
        (d.levels.where((l) => cur != null && l.rank > cur.rank && l.cpaAmount > cur.cpaAmount).toList()..sort((a, b) => a.rank.compareTo(b.rank))).firstOrNull;
    final mins = (p.minTradeSeconds / 60).round();
    final rules = <(IconData, String)>[
      (LucideIcons.target, t('partner.cpa.ruleDeposit', {'amount': money0(p.cpaMinFirstDeposit)})),
      if (p.cpaRequireFirstTrade)
        (
          LucideIcons.timer,
          t('partner.cpa.ruleTrade', {
            'duration': mins >= 1 ? t('partner.unit.min', {'n': mins}) : t('partner.unit.sec', {'n': p.minTradeSeconds}),
          }),
        ),
      (LucideIcons.hourglass, t('partner.cpa.ruleHold', {'count': p.cpaHoldDays})),
    ];
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('partner.cpa.title'),
            subtitle: t('partner.cpa.subtitleDash'),
            action: p.cpaEnabled
                ? KChip(label: t('partner.cpa.earnedCount', {'n': d.cpaCount}), tone: KChipTone.gold)
                : KChip(label: t('partner.cpa.notOffered')),
          ),
          const SizedBox(height: 14),
          KMoney(d.cpaEarned, style: context.text.moneyL.copyWith(fontSize: 28)),
          const SizedBox(height: 4),
          Text(
            d.cpaWaiting > 0 ? t('partner.cpa.waiting', {'count': d.cpaWaiting}) : t('partner.cpa.earnedFrom'),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 14),
          if (p.cpaEnabled) ...[
            PartnerRow(
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      t('partner.cpa.perClientAt', {'name': cur?.name ?? t('partner.yourLevel')}),
                      style: context.text.footnote.copyWith(color: k.fg2),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Flexible(
                    child: Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(text: money0(cur?.cpaAmount ?? 0)),
                          if (better != null)
                            TextSpan(
                              text: ' · ${t('partner.cpa.amountAt', {'amount': money0(better.cpaAmount), 'name': better.name})}',
                              style: TextStyle(fontWeight: FontWeight.w400, color: k.fg3),
                            ),
                        ],
                      ),
                      textAlign: TextAlign.end,
                      style: context.text.footnote.copyWith(fontWeight: FontWeight.w600, color: k.fg, fontFeatures: kTabular),
                    ),
                  ),
                ],
              ),
            ),
            const SizedBox(height: 10),
            for (final (icon, text) in rules)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 4),
                child: Row(
                  children: [
                    RoundIcon(icon, size: 24),
                    const SizedBox(width: 10),
                    Expanded(
                      child: Text(text, style: context.text.footnote.copyWith(color: k.fg2)),
                    ),
                  ],
                ),
              ),
          ] else
            Text(t('partner.cpa.offText'), style: context.text.footnote.copyWith(color: k.fg3)),
          const SizedBox(height: 16),
          PartnerRow(
            padding: const EdgeInsetsDirectional.fromSTEB(16, 11, 10, 11),
            child: Row(
              children: [
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        t('partner.dash.rebateSplit'),
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                      ),
                      const SizedBox(height: 2),
                      Text(
                        '${fmtPct(d.rebatePct)} · ${fmtPct(d.splitPct)}',
                        textDirection: TextDirection.ltr,
                        style: context.text.figure.copyWith(fontSize: 14, fontWeight: FontWeight.w500),
                      ),
                    ],
                  ),
                ),
                KButton(
                  label: t('partner.dash.adjust'),
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: () => context.go('/partner/commissions?section=rebates'),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

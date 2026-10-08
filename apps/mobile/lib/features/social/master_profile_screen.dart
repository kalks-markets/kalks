// Copy & PAMM › master profile (/social/masters/:id, iOS push): identity, returns and the Copy / Invest actions,
// growth chart, risk & statistics, monthly returns, instruments, trade history and fee terms. Port of the phone
// layout of apps/crm/components/social-live/master-profile.tsx (GET masters/{id}?invite, 60 s).
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'social_api.dart';
import 'widgets/bits.dart';
import 'widgets/follow_sheet.dart';
import 'widgets/invest_sheet.dart';

const Map<String, int> _ranges = {'1M': 31, '3M': 92, '1Y': 366, 'All': 1000000000};
const Map<String, String> _rangeKey = {'1M': 'social.lb.period.1m', '3M': 'social.lb.period.3m', '1Y': 'social.lb.period.1y', 'All': 'common.all'};

class MasterProfileScreen extends ConsumerWidget {
  const MasterProfileScreen({super.key, required this.id, this.query = const {}});
  final String id;
  final Map<String, String> query;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final valid = RegExp(r'^\d{1,12}$').hasMatch(id);
    final rawInvite = (query['invite'] ?? '').trim();
    final invite = inviteRe.hasMatch(rawInvite) ? rawInvite : null;
    final back = Align(
      alignment: AlignmentDirectional.centerStart,
      child: KPressable(
        onTap: () => context.canPop() ? context.pop() : context.go('/social'),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            Icon(Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowRight : LucideIcons.arrowLeft, size: 16, color: context.k.fg2),
            const SizedBox(width: 6),
            Text(t('social.profile.discover'), style: context.text.label.copyWith(color: context.k.fg2)),
          ],
        ),
      ),
    );
    if (!valid) return KPageScroll(children: [back, const SizedBox(height: 12), _notFound(context)]);
    final provider = masterProfileProvider((int.parse(id), invite));
    final q = ref.watch(provider);
    final p = q.value;
    final err = q.error;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;

    Future<void> refresh() async {
      ref.invalidate(provider);
      await ref.read(provider.future).then((_) {}, onError: (Object _) {});
    }

    if (p == null) {
      if (err is ApiException && (err.status == 404 || err.status == 400)) {
        return KPageScroll(children: [back, const SizedBox(height: 12), _notFound(context)]);
      }
      return KPageScroll(
        onRefresh: refresh,
        children: [
          back,
          const SizedBox(height: 12),
          if (err != null)
            SocialErrorCard(error: err, onRetry: () => ref.invalidate(provider))
          else ...[
            const KSkeleton(height: 220, radius: 20),
            const SizedBox(height: 16),
            const KSkeleton(height: 320, radius: 20),
          ],
        ],
      );
    }

    final m = p.master;
    final s = m.stats;
    final accepting = m.acceptingNew != false;
    final canCopy = m.program != 'pamm' && m.status == 'approved' && !m.frozen && !(m.house && m.hidden) && accepting;
    // investing in the master's fund is PAMM's
    final canInvest = m.fund != null && m.program != 'copy' && m.fund!.status == 'active' && ref.watch(configProvider).moduleOn('pamm');
    void copy() => showFollowSheet(
      context,
      // terms carry the effective minimum (the broker's floor or the master's, whichever is higher)
      master: m.withMinAllocation(p.termsMinAllocation),
      suggested: [for (final x in p.symbols) x.symbol],
      inviteCode: invite,
      onDone: () => ref.invalidate(provider),
    );
    void invest() => showInvestSheet(context, fundId: m.fund!.id);
    final k = context.k;
    final f = LocaleFormat(t.locale);

    return KPageScroll(
      onRefresh: refresh,
      children: [
        back,
        const SizedBox(height: 12),
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              KAvatar(name: m.nickname, size: 72),
              const SizedBox(height: 14),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Text(m.nickname, style: context.text.largeTitle.copyWith(fontSize: 24, fontWeight: FontWeight.w500)),
                  if (m.house) const HouseBadge() else KChip(label: t('social.profile.approved'), tone: KChipTone.up, small: true),
                  if (m.frozen) KChip(label: t('social.profile.frozen'), tone: KChipTone.down, icon: LucideIcons.snowflake, small: true),
                  if (m.inviteOnly) KChip(label: t('social.inviteOnly'), tone: KChipTone.gold, icon: LucideIcons.link2, small: true),
                ],
              ),
              const SizedBox(height: 4),
              Text(m.strategy, style: context.text.body.copyWith(fontSize: 15.5)),
              if (m.description.isNotEmpty) ...[
                const SizedBox(height: 10),
                Text(m.description, style: context.text.callout.copyWith(color: k.fg2, height: 1.55)),
              ],
              if (m.house) ...[const SizedBox(height: 10), InfoBox(icon: LucideIcons.building2, text: t('social.house.disclosure'))],
              const SizedBox(height: 12),
              ProgramTags(program: m.program, small: false),
              const SizedBox(height: 12),
              Wrap(
                spacing: 16,
                runSpacing: 4,
                children: [
                  _Meta(icon: LucideIcons.calendarClock, text: t('social.profile.since', {'date': fmtDate(t, m.since), 'age': formatAge(t, m.ageDays)})),
                  _Meta(
                    icon: LucideIcons.users,
                    text:
                        '${t('social.profile.followersCount', {'count': s.followers, 'n': f.number(s.followers, 0)})}'
                        '${m.fund != null ? ' · ${t('social.profile.investorsCount', {'count': s.investors, 'n': f.number(s.investors, 0)})}' : ''}',
                  ),
                  _Meta(text: '${t('social.aum')} ${compactUsd(s.aum)}'),
                ],
              ),
              const SizedBox(height: 16),
              Row(
                children: [
                  for (final (key, v) in [('social.lb.period.1m', s.return1m), ('social.lb.period.1y', s.return1y), ('common.all', s.returnAll)]) ...[
                    if (key != 'social.lb.period.1m') const SizedBox(width: 8),
                    Expanded(
                      child: Container(
                        padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 10),
                        decoration: BoxDecoration(
                          color: k.surface2,
                          borderRadius: BorderRadius.circular(14),
                          border: Border.all(color: k.line),
                        ),
                        child: Column(
                          children: [
                            Text(
                              t('social.lb.col.return', {'period': t(key)}),
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                            ),
                            const SizedBox(height: 2),
                            Num(pct(v, 1), color: toneColor(context, v), style: context.text.figure.copyWith(fontSize: 16)),
                          ],
                        ),
                      ),
                    ),
                  ],
                ],
              ),
              if (!readOnly && (m.program != 'pamm' || canInvest)) ...[
                const SizedBox(height: 14),
                Row(
                  children: [
                    if (m.program != 'pamm')
                      Expanded(
                        child: KButton(
                          label: t('social.program.copy'),
                          icon: LucideIcons.copy,
                          size: KButtonSize.lg,
                          expand: true,
                          onPressed: canCopy ? copy : null,
                        ),
                      ),
                    if (m.program != 'pamm' && canInvest) const SizedBox(width: 8),
                    if (canInvest)
                      Expanded(
                        child: KButton(
                          label: t('social.profile.investPamm'),
                          icon: LucideIcons.landmark,
                          variant: m.program == 'pamm' ? KButtonVariant.ember : KButtonVariant.surface,
                          size: KButtonSize.lg,
                          expand: true,
                          onPressed: invest,
                        ),
                      ),
                  ],
                ),
              ],
              if (m.program != 'pamm' && !accepting) ...[
                const SizedBox(height: 10),
                Row(
                  children: [
                    Icon(LucideIcons.userX, size: 14, color: k.warn),
                    const SizedBox(width: 6),
                    Expanded(
                      child: Text(
                        t('social.notAccepting'),
                        style: context.text.footnote.copyWith(color: k.warn, fontWeight: FontWeight.w600),
                      ),
                    ),
                  ],
                ),
              ],
              const SizedBox(height: 10),
              Text(
                '${t('social.profile.feeLine', {'fee': numText(p.termsFee), 'min': usd(p.termsMinAllocation, 0)})}'
                '${m.maxFollowers != null && accepting ? ' · ${t('social.profile.spotsLeft', {'count': math.max(0, m.maxFollowers! - s.followers)})}' : ''}',
                style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        _EquityCard(points: p.equity),
        const SizedBox(height: 16),
        _StatsCard(p: p),
        const SizedBox(height: 16),
        _MonthlyCard(monthly: p.monthly),
        const SizedBox(height: 16),
        _SymbolsCard(symbols: p.symbols),
        const SizedBox(height: 16),
        _TradesCard(p: p),
        const SizedBox(height: 16),
        _FeesCard(p: p, onInvest: readOnly ? null : invest),
        const SizedBox(height: 20),
        InfoBox(text: t('social.profile.disclaimer')),
      ],
    );
  }

  Widget _notFound(BuildContext context) {
    final t = context.t;
    return KCard(
      child: KEmptyState(
        icon: LucideIcons.searchX,
        title: t('social.profile.notFoundTitle'),
        text: t('social.profile.notFoundText'),
        action: KButton(label: t('social.profile.backToDiscover'), size: KButtonSize.sm, onPressed: () => context.go('/social')),
      ),
    );
  }
}

class _Meta extends StatelessWidget {
  const _Meta({required this.text, this.icon});
  final String text;
  final IconData? icon;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (icon != null) ...[Icon(icon, size: 13, color: k.fg3), const SizedBox(width: 4)],
        Flexible(
          child: Text(text, style: context.text.footnote.copyWith(color: k.fg3)),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ growth */

class _EquityCard extends StatefulWidget {
  const _EquityCard({required this.points});
  final List<({String day, double index})> points;

  @override
  State<_EquityCard> createState() => _EquityCardState();
}

class _EquityCardState extends State<_EquityCard> {
  String _range = 'All';

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final all = <(DateTime, double)>[];
    for (final p in widget.points) {
      final d = DateTime.tryParse(p.day.length == 10 ? '${p.day}T00:00:00Z' : p.day);
      final v = double.parse((p.index * 10000).toStringAsFixed(2));
      if (d == null || !v.isFinite) continue;
      all.add((d, v));
    }
    all.sort((a, b) => a.$1.compareTo(b.$1));
    final dedup = <(DateTime, double)>[];
    for (final x in all) {
      if (dedup.isEmpty || x.$1.isAfter(dedup.last.$1)) dedup.add(x);
    }
    final data = dedup.isEmpty ? dedup : dedup.where((x) => !x.$1.isBefore(dedup.last.$1.subtract(Duration(days: _ranges[_range]!)))).toList();
    if (data.length < 2) {
      return SectionCard(
        title: t('social.profile.growth'),
        subtitle: t('social.profile.growthSub'),
        child: KEmptyState(compact: true, icon: LucideIcons.trendingUp, title: t('social.profile.growthEmptyTitle'), text: t('social.profile.growthEmptyText')),
      );
    }
    final first = data.first.$2;
    final last = data.last.$2;
    final g = (last / first - 1) * 100;
    final f = LocaleFormat(t.locale);
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(t('social.profile.growth').toUpperCase(), style: context.text.micro.copyWith(color: k.fg3, letterSpacing: 0.6)),
          const SizedBox(height: 8),
          Wrap(
            spacing: 10,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              KMoney(last, style: context.text.moneyL),
              KChip(label: '${pct(g)} · ${t(_rangeKey[_range]!)}', tone: g >= 0 ? KChipTone.up : KChipTone.down),
            ],
          ),
          const SizedBox(height: 4),
          Text(
            t('social.profile.growthHint'),
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 12),
          KSegmented<String>(
            values: _ranges.keys.toList(),
            labels: [for (final r in _ranges.keys) t(_rangeKey[r]!)],
            selected: _range,
            height: 32,
            onChanged: (v) => setState(() => _range = v),
          ),
          const SizedBox(height: 8),
          KLineChart(
            values: [for (final x in data) x.$2],
            labels: [for (final x in data) f.date(x.$1)],
            height: 200,
            color: g >= 0 ? k.up : k.down,
            format: usd,
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ risk & stats */

class _StatsCard extends StatelessWidget {
  const _StatsCard({required this.p});
  final MasterProfile p;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final s = p.master.stats;
    final f = LocaleFormat(t.locale);
    return SectionCard(
      title: t('social.profile.riskStats'),
      subtitle: t('social.profile.riskStatsSub'),
      action: RiskBadge(risk: s.riskScore, showLabel: true),
      child: Column(
        children: [
          RiskGauge(value: s.riskScore, label: t('social.profile.riskLevel', {'level': riskLabel(t, s.riskScore)})),
          const SizedBox(height: 12),
          TileGrid(
            tiles: [
              Tile(label: t('social.profile.winRate'), value: Num(s.trades > 0 ? '${s.winRate.toStringAsFixed(1)}%' : '—')),
              Tile(label: t('social.profile.closedTrades'), value: Num(f.number(s.trades, 0))),
              Tile(
                label: t('social.follow.maxDrawdown'),
                value: Num(ddText(s.maxDd), color: s.maxDd > 0 ? k.down : k.fg2),
              ),
              Tile(
                label: t('social.profile.currentDd'),
                value: Num(ddText(s.currentDd), color: s.currentDd > 10 ? k.down : k.fg),
              ),
              Tile(label: t('social.profile.volatility'), value: Num('${s.volatility.toStringAsFixed(1)}%')),
              Tile(label: t('social.profile.masterEquity'), value: Num(compactUsd(s.equity))),
            ],
          ),
        ],
      ),
    );
  }
}

/// A half-circle gauge of the 1–10 risk score (web Gauge).
class RiskGauge extends StatelessWidget {
  const RiskGauge({super.key, required this.value, required this.label, this.size = 170});
  final int value;
  final String label;
  final double size;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final tone = riskTone(value);
    final color = tone == KChipTone.up ? k.up : (tone == KChipTone.warn ? k.warn : k.down);
    return SizedBox(
      width: size,
      height: size * 0.62,
      child: CustomPaint(
        painter: _GaugePainter(value.clamp(0, 10) / 10, color, k.surface3),
        child: Align(
          alignment: Alignment.bottomCenter,
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Num('$value/10', style: context.text.title1.copyWith(fontSize: 24)),
              Text(
                label,
                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _GaugePainter extends CustomPainter {
  _GaugePainter(this.fraction, this.color, this.track);
  final double fraction;
  final Color color, track;

  @override
  void paint(Canvas canvas, Size size) {
    const stroke = 14.0;
    final r = size.width / 2 - stroke / 2;
    final rect = Rect.fromCircle(center: Offset(size.width / 2, size.width / 2), radius: r);
    final base = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = stroke
      ..strokeCap = StrokeCap.round
      ..color = track;
    canvas.drawArc(rect, math.pi, math.pi, false, base);
    if (fraction > 0) canvas.drawArc(rect, math.pi, math.pi * fraction, false, base..color = color);
  }

  @override
  bool shouldRepaint(_GaugePainter old) => old.fraction != fraction || old.color != color;
}

/* ------------------------------------------------------------------ monthly returns */

class _MonthlyCard extends StatelessWidget {
  const _MonthlyCard({required this.monthly});
  final List<({String month, double returnPct})> monthly;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final parsed = <({int year, int month, double ret})>[];
    for (final m in monthly) {
      if (m.month.length < 7) continue;
      final y = int.tryParse(m.month.substring(0, 4));
      final mo = int.tryParse(m.month.substring(5, 7));
      if (y == null || mo == null || mo < 1 || mo > 12 || !m.returnPct.isFinite) continue;
      parsed.add((year: y, month: mo - 1, ret: m.returnPct));
    }
    if (parsed.isEmpty) {
      return SectionCard(
        title: t('social.profile.monthly'),
        subtitle: t('social.profile.monthlySub'),
        child: KEmptyState(compact: true, icon: LucideIcons.calendar, title: t('social.profile.monthlyEmptyTitle'), text: t('social.profile.monthlyEmptyText')),
      );
    }
    final years = parsed.map((x) => x.year).toSet().toList()..sort((a, b) => b.compareTo(a));
    ({int year, int month, double ret})? cell(int y, int mo) => parsed.where((x) => x.year == y && x.month == mo).firstOrNull;
    const cw = 44.0, ch = 34.0;
    final rets = [for (final x in parsed) x.ret];
    final best = parsed.reduce((a, b) => b.ret > a.ret ? b : a);
    final worst = parsed.reduce((a, b) => b.ret < a.ret ? b : a);
    final positive = rets.where((r) => r > 0).length;
    return SectionCard(
      title: t('social.profile.monthly'),
      subtitle: t('social.profile.monthlySub'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Directionality(
            textDirection: TextDirection.ltr,
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      const SizedBox(width: 42),
                      for (var mo = 0; mo < 12; mo++)
                        SizedBox(
                          width: cw + 4,
                          child: Text(
                            monthName(t, mo),
                            textAlign: TextAlign.center,
                            style: context.text.caption.copyWith(color: k.fg3),
                          ),
                        ),
                      SizedBox(
                        width: 56,
                        child: Text(
                          t('social.profile.year'),
                          textAlign: TextAlign.right,
                          style: context.text.caption.copyWith(color: k.fg3),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 4),
                  for (final y in years)
                    Padding(
                      padding: const EdgeInsets.only(bottom: 4),
                      child: Row(
                        children: [
                          SizedBox(
                            width: 42,
                            child: Num('$y', color: k.fg2, style: context.text.mono(12)),
                          ),
                          for (var mo = 0; mo < 12; mo++)
                            Padding(
                              padding: const EdgeInsets.symmetric(horizontal: 2),
                              child: () {
                                final c = cell(y, mo);
                                if (c == null) {
                                  return Container(
                                    width: cw,
                                    height: ch,
                                    decoration: BoxDecoration(color: k.surface2.withValues(alpha: 0.5), borderRadius: BorderRadius.circular(8)),
                                  );
                                }
                                final a = math.min(1.0, c.ret.abs() / 8);
                                final tone = c.ret >= 0 ? k.up : k.down;
                                return Container(
                                  width: cw,
                                  height: ch,
                                  alignment: Alignment.center,
                                  decoration: BoxDecoration(
                                    color: tone.withValues(alpha: 0.1 + a * 0.55),
                                    borderRadius: BorderRadius.circular(8),
                                  ),
                                  child: Text(
                                    pct(c.ret, 1).replaceAll('%', ''),
                                    style: context.text.caption.copyWith(
                                      color: a > 0.55 ? Colors.white : tone,
                                      fontWeight: FontWeight.w600,
                                      fontFeatures: kTabular,
                                    ),
                                  ),
                                );
                              }(),
                            ),
                          SizedBox(
                            width: 56,
                            child: () {
                              final yr = parsed.where((x) => x.year == y).fold<double>(1, (acc, x) => acc * (1 + x.ret / 100));
                              final ytd = (yr - 1) * 100;
                              return Text(
                                pct(ytd, 1),
                                textAlign: TextAlign.right,
                                style: context.text.footnote.copyWith(color: ytd >= 0 ? k.up : k.down, fontWeight: FontWeight.w700, fontFeatures: kTabular),
                              );
                            }(),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
          ),
          if (parsed.length > 1) ...[
            const SizedBox(height: 12),
            TileGrid(
              tiles: [
                Tile(
                  label: t('social.profile.bestMonth'),
                  value: Num(pct(best.ret, 1), color: k.up),
                  sub: '${monthName(t, best.month)} ${best.year}',
                ),
                Tile(
                  label: t('social.profile.worstMonth'),
                  value: Num(pct(worst.ret, 1), color: k.down),
                  sub: '${monthName(t, worst.month)} ${worst.year}',
                ),
                Tile(
                  label: t('social.profile.positiveMonths'),
                  value: Num('${(positive / rets.length * 100).round()}%'),
                  sub: t('social.profile.nOfTotal', {'n': positive, 'total': rets.length}),
                ),
                Tile(label: t('social.profile.avgMonth'), value: Num(pct(rets.reduce((a, b) => a + b) / rets.length)), sub: t('social.profile.arithmeticMean')),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ instruments */

class _SymbolsCard extends StatelessWidget {
  const _SymbolsCard({required this.symbols});
  final List<({String symbol, int trades, double share})> symbols;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final other = t('social.profile.other');
    if (symbols.isEmpty) {
      return SectionCard(
        title: t('social.profile.instruments'),
        subtitle: t('social.profile.instrumentsSub'),
        child: KEmptyState(compact: true, icon: LucideIcons.chartPie, title: t('social.profile.noClosedTrades')),
      );
    }
    final total = symbols.fold<double>(0, (s, x) => s + x.share);
    final rows = [...symbols]..sort((a, b) => b.share.compareTo(a.share));
    final rest = rows.skip(5).fold<double>(0, (s, x) => s + x.share);
    final slices = [for (final x in rows.take(5)) (x.symbol, x.share), if (rest > 0) (other, rest)];
    final k = context.k;
    return SectionCard(
      title: t('social.profile.instruments'),
      subtitle: t('social.profile.instrumentsSub'),
      child: Column(
        children: [
          KDonut(
            size: 160,
            thickness: 18,
            segments: [for (var i = 0; i < slices.length; i++) (slices[i].$2, kChartColors[i % kChartColors.length])],
            center: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                Num('${symbols.length}', style: context.text.title1),
                Text(
                  t('social.profile.symbols'),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ],
            ),
          ),
          const SizedBox(height: 14),
          for (var i = 0; i < slices.length; i++) ...[
            if (i > 0) const SizedBox(height: 6),
            LegendRow(
              color: kChartColors[i % kChartColors.length],
              label: slices[i].$1,
              value: '${(slices[i].$2 / (total == 0 ? 1 : total) * 100).toStringAsFixed(1)}%',
            ),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ trades */

class _TradesCard extends StatelessWidget {
  const _TradesCard({required this.p});
  final MasterProfile p;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return SectionCard(
      title: t('social.profile.tradeHistory'),
      subtitle: t('social.profile.tradeHistorySub'),
      // only when trades are published with a delay; "0 min delay" says nothing
      action: p.tradeDelayMinutes > 0
          ? KChip(label: t('social.profile.delay', {'n': p.tradeDelayMinutes}), tone: KChipTone.warn, icon: LucideIcons.clock)
          : null,
      child: p.trades.isEmpty
          ? KEmptyState(
              compact: true,
              icon: LucideIcons.hourglass,
              title: t('social.profile.tradesEmptyTitle'),
              text: p.tradeDelayMinutes > 0 ? t('social.profile.tradesEmptyText', {'n': p.tradeDelayMinutes}) : null,
            )
          : RowsBox(
              pageSize: 12,
              boxed: false,
              children: [
                for (final x in p.trades)
                  DataLine(
                    title: Row(
                      children: [
                        Flexible(child: Text(x.symbol, maxLines: 1, overflow: TextOverflow.ellipsis)),
                        const SizedBox(width: 6),
                        KChip(
                          label: t.dyn('common.${x.side}', fallback: x.side).toUpperCase(),
                          tone: x.side == 'buy' ? KChipTone.up : KChipTone.down,
                          small: true,
                        ),
                      ],
                    ),
                    subtitle: Text(serverTime(t, x.closeTime, withYear: false)),
                    trailing: Num(usd(x.profit, 2, true), color: toneColor(context, x.profit)),
                    trailingSub: Num('${x.volume.toStringAsFixed(2)} ${t('social.lotsUnit')}', color: k.fg3),
                  ),
              ],
            ),
    );
  }
}

/* ------------------------------------------------------------------ fee terms */

class _FeesCard extends StatelessWidget {
  const _FeesCard({required this.p, required this.onInvest});
  final MasterProfile p;
  final VoidCallback? onInvest;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = p.master.fund;
    final rowStyle = context.text.callout.copyWith(fontWeight: FontWeight.w600);
    return SectionCard(
      title: t('social.profile.feeTerms'),
      subtitle: t('social.profile.feeTermsSub'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.baseline,
            textBaseline: TextBaseline.alphabetic,
            children: [
              Num('${numText(p.termsFee)}%', color: k.gold, style: context.text.moneyL.copyWith(fontSize: 32)),
              const SizedBox(width: 8),
              Text(t('social.profile.performanceFeeLower'), style: context.text.callout.copyWith(color: k.fg2)),
            ],
          ),
          KKeyValues([
            KKV(
              t('social.funds.explain.hwmT'),
              null,
              valueWidget: KChip(label: p.termsHwm ? t('common.yes') : t('common.no'), tone: p.termsHwm ? KChipTone.up : KChipTone.neutral, small: true),
            ),
            KKV(t('social.profile.feeSettlement'), periodLabel(t, p.termsPeriod)),
            KKV(t('social.profile.minAllocation'), usd(p.termsMinAllocation, 0), mono: true),
            if (f != null) ...[
              KKV(t('social.funds.pammFund'), f.name),
              KKV(t('social.navPerUnit'), nav4(f.nav), mono: true),
              KKV(t('social.rollover'), periodLabel(t, f.period)),
              KKV(t('social.profile.fundFee'), '${numText(f.perfFeePct)}%', mono: true),
              KKV(
                t('social.lockIn'),
                null,
                valueWidget: f.lockInDays > 0
                    ? Row(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(LucideIcons.lock, size: 13, color: k.warn),
                          const SizedBox(width: 4),
                          Text(t('social.profile.days', {'count': f.lockInDays}), style: rowStyle),
                        ],
                      )
                    : Text(t('common.none'), style: rowStyle),
              ),
              KKV(t('social.funds.minInvestment'), usd(f.minInvestment, 0), mono: true),
            ],
          ]),
          if (t.dyn('social.profile.feeNote.${p.termsPeriod}', fallback: '').isNotEmpty)
            Text(
              t.dyn('social.profile.feeNote.${p.termsPeriod}', fallback: ''),
              style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12, height: 1.5),
            ),
          if (f != null && f.status == 'active' && p.master.program != 'copy' && onInvest != null) ...[
            const SizedBox(height: 14),
            KButton(
              label: t('social.invest.title', {'name': f.name}),
              icon: LucideIcons.landmark,
              variant: KButtonVariant.surface,
              expand: true,
              onPressed: onInvest,
            ),
          ],
        ],
      ),
    );
  }
}

// Contests & Rewards › Promotions (/rewards/promotions). Port of the web's LivePromotionsPage
// (apps/crm/components/growth/promotions.tsx) in its phone order:
//   header (Promotions + Deposit)  ·  banner slot  ·  KPI cards (to release, released, offers)  ·  offers (claim, terms)
//   ·  my bonuses (release progress)  ·  promo code  ·  promo code history
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../core/models/account.dart';
import '../../core/notifications/notifications.dart';
import '../../data/client_data.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'rewards_api.dart';
import 'widgets/growth_ui.dart';

String _headline(GrowthFmt f, Campaign c) =>
    c.deposit ? f.t('rewards.promo.headlineDeposit', {'pct': GrowthFmt.plain(c.pct)}) : f.t('rewards.promo.headlineFixed', {'amount': f.usd0(c.fixedAmount)});

int _lotsFor(Campaign c, double amount) => c.releasePerLot > 0 ? (amount / c.releasePerLot).ceil() : 0;

List<KKV> _termsRows(GrowthFmt f, Campaign c) {
  final t = f.t;
  final total = c.deposit ? c.cap : c.fixedAmount;
  return [
    if (c.deposit) ...[
      KKV(t('rewards.terms.bonus'), t('rewards.terms.bonusDeposit', {'pct': GrowthFmt.plain(c.pct), 'cap': f.usd0(c.cap)})),
      KKV(t('rewards.terms.minDeposit'), f.usd0(c.minDeposit)),
      KKV(t('rewards.terms.depositWithin'), t('rewards.terms.daysOfClaiming', {'count': c.claimWindowDays})),
    ] else
      KKV(t('rewards.terms.bonus'), f.usd0(c.fixedAmount)),
    KKV(
      t('rewards.terms.release'),
      c.releasePerLot > 0
          ? t('rewards.terms.releaseText', {'amount': f.usd(c.releasePerLot), 'lots': f.count(_lotsFor(c, total)), 'total': f.usd0(total)})
          : t('rewards.terms.notReleased'),
    ),
    KKV(t('rewards.terms.expires'), t('rewards.terms.expiresText', {'count': c.expiryDays})),
    KKV(t('rewards.terms.withdrawals'), c.forfeitOnWithdrawal ? t('rewards.terms.forfeit') : t('rewards.terms.kept')),
    if (c.endsAt != null) KKV(t('rewards.terms.offerEnds'), f.date(c.endsAt)),
  ];
}

class PromotionsScreen extends ConsumerWidget {
  const PromotionsScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    final title = t('rewards.promo.title');
    final subtitle = t('rewards.promo.subtitle');
    final walletOn = ref.watch(configProvider).moduleOn('wallet');
    final v = ref.watch(promotionsProvider);
    if (!v.hasValue) {
      return GrowthFallback(
        title: title,
        subtitle: subtitle,
        error: v.hasError ? v.error : null,
        onRetry: () => ref.invalidate(promotionsProvider),
        heights: const [150, 320, 320],
      );
    }
    final data = v.requireValue;
    final running = data.grants.where((g) => g.status == 'active' || g.status == 'pending').toList();
    final outstanding = running.fold<double>(0, (s, g) => s + g.remaining);
    final released = data.grants.fold<double>(0, (s, g) => s + g.released);
    final width = MediaQuery.sizeOf(context).width;
    final kpiWidth = (width - 2 * KSpace.page) * 0.78;
    void reload() => ref.invalidate(promotionsProvider);

    return KPageScroll(
      onRefresh: () async {
        ref
          ..invalidate(promotionsProvider)
          ..invalidate(bannersProvider('rewards'));
        await ref.read(promotionsProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        KPageHeader(title: title, subtitle: Text(subtitle)),
        if (walletOn) ...[
          const SizedBox(height: 12),
          Row(
            children: [
              KButton(label: t('common.deposit'), icon: LucideIcons.wallet, variant: KButtonVariant.surface, onPressed: () => context.go('/wallet/deposit')),
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
                label: t('rewards.promo.kpiToRelease'),
                icon: LucideIcons.gift,
                value: KMoney(outstanding, style: context.text.moneyL),
                chip: KChip(label: t('rewards.promo.kpiActive', {'count': running.length}), tone: KChipTone.ember),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.promo.kpiReleased'),
                icon: LucideIcons.circleCheck,
                value: KMoney(released, style: context.text.moneyL),
                chip: KChip(label: t('rewards.promo.kpiReleasedChip'), tone: KChipTone.up),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('rewards.promo.kpiOffers'),
                icon: LucideIcons.gift,
                value: Text('${data.campaigns.where((c) => c.eligible && !c.claimed).length}', textDirection: TextDirection.ltr),
                chip: KChip(label: t('rewards.promo.kpiRunning', {'count': data.campaigns.length})),
              ),
            ],
          ),
        ),
        const SizedBox(height: 28),
        GrowthSectionTitle(title: t('rewards.promo.offersTitle'), text: t('rewards.promo.offersText')),
        if (data.campaigns.isEmpty)
          KCard(
            child: KEmptyState(art: KIllustrationName.rewards, title: t('rewards.promo.noOffersTitle'), text: t('rewards.promo.noOffersText')),
          )
        else
          for (final c in data.campaigns) ...[if (c != data.campaigns.first) const SizedBox(height: kBlockGap), _CampaignCard(c: c, onClaimed: reload)],
        const SizedBox(height: 28),
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('rewards.promo.myBonuses'), subtitle: t('rewards.promo.myBonusesSubtitle'), icon: LucideIcons.gift, tone: KTone.coral),
              const SizedBox(height: 14),
              if (data.grants.isEmpty)
                CardEmpty(title: t('rewards.promo.noBonusesTitle'), text: t('rewards.promo.noBonusesText'))
              else
                for (final g in data.grants) ...[if (g != data.grants.first) const SizedBox(height: 8), _GrantRow(g: g)],
            ],
          ),
        ),
        const SizedBox(height: kBlockGap),
        _PromoCard(onApplied: reload),
        const SizedBox(height: kBlockGap),
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('rewards.promo.historyTitle'), subtitle: t('rewards.promo.historySubtitle')),
              const SizedBox(height: 10),
              PagedRows<PromoUse>(
                rows: data.promoHistory,
                pageSize: 8,
                empty: CardEmpty(title: t('rewards.promo.historyEmptyTitle'), text: t('rewards.promo.historyEmptyText')),
                itemBuilder: (context, p) => Padding(
                  padding: const EdgeInsets.symmetric(vertical: 11),
                  child: Row(
                    children: [
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              p.code,
                              textDirection: TextDirection.ltr,
                              style: context.text.mono(13, weight: FontWeight.w600, color: k.fg).copyWith(letterSpacing: 0.8),
                            ),
                            const SizedBox(height: 2),
                            Text(
                              f.dateTime(p.createdAt),
                              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                            ),
                          ],
                        ),
                      ),
                      GrowthStatus(p.status),
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

/* ------------------------------------------------------------------ campaigns */

class _CampaignCard extends ConsumerWidget {
  const _CampaignCard({required this.c, required this.onClaimed});
  final Campaign c;
  final VoidCallback onClaimed;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final lots = _lotsFor(c, c.deposit ? c.cap : c.fixedAmount);
    Widget cell(String label, String value) => RowBox(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 9),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            label,
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 2),
          Text(
            value,
            style: context.text.footnote.copyWith(fontWeight: FontWeight.w600, color: k.fg, fontFeatures: kTabular),
          ),
        ],
      ),
    );
    return KCard(
      key: ValueKey('bonus-campaign-${c.id}'),
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Container(
            padding: const EdgeInsets.fromLTRB(18, 14, 18, 14),
            decoration: BoxDecoration(
              color: k.surface2.withValues(alpha: 0.6),
              border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    KChip(
                      label: c.deposit ? t('rewards.card.badgeDeposit') : t('rewards.card.badgeBonus'),
                      tone: c.deposit ? KChipTone.ember : KChipTone.gold,
                      small: true,
                    ),
                    const Spacer(),
                    if (c.endsAt != null) ...[
                      Icon(LucideIcons.clock, size: 12, color: k.fg3),
                      const SizedBox(width: 4),
                      Text(
                        t('rewards.card.until', {'date': f.date(c.endsAt, year: false)}),
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                      ),
                    ],
                  ],
                ),
                const SizedBox(height: 10),
                Text(
                  c.deposit ? '${GrowthFmt.plain(c.pct)}%' : f.usd0(c.fixedAmount),
                  textDirection: TextDirection.ltr,
                  style: context.text.figure.copyWith(fontSize: 26, fontWeight: FontWeight.w700, color: k.gold, height: 1.2),
                ),
                Text(
                  c.deposit ? t('rewards.card.upTo', {'cap': f.usd0(c.cap), 'min': f.usd0(c.minDeposit)}) : t('rewards.card.credited'),
                  style: context.text.footnote.copyWith(color: k.fg2),
                ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(18, 12, 18, 18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text(c.name, style: context.text.headline.copyWith(fontSize: 16, fontWeight: FontWeight.w500)),
                if (c.description.isNotEmpty) ...[const SizedBox(height: 2), Text(c.description, style: context.text.footnote.copyWith(color: k.fg3))],
                const SizedBox(height: 12),
                TileGrid(
                  children: [
                    cell(t('rewards.card.release'), c.releasePerLot > 0 ? t('rewards.card.perLot', {'amount': f.usd(c.releasePerLot)}) : '—'),
                    cell(t('rewards.card.fullRelease'), lots > 0 ? t('rewards.value.lots', {'lots': f.count(lots)}) : '—'),
                    cell(t('rewards.card.expires'), t('rewards.value.days', {'count': c.expiryDays})),
                    cell(t('rewards.card.onWithdrawal'), c.forfeitOnWithdrawal ? t('rewards.card.forfeited') : t('rewards.card.kept')),
                  ],
                ),
                if (!c.eligible && !c.claimed && c.reason != null && c.reason!.isNotEmpty) ...[
                  const SizedBox(height: 10),
                  RowBox(
                    padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                    border: k.line,
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Padding(
                          padding: const EdgeInsets.only(top: 1),
                          child: Icon(LucideIcons.circleAlert, size: 14, color: k.fg3),
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(c.reason!, style: context.text.footnote.copyWith(color: k.fg3)),
                        ),
                      ],
                    ),
                  ),
                ],
                const SizedBox(height: 14),
                Row(
                  children: [
                    Expanded(
                      child: c.claimed
                          ? DonePill(label: t('rewards.card.claimed'), icon: LucideIcons.circleCheck, expand: true, height: 32)
                          : c.eligible
                          ? (readOnly
                                ? const SizedBox.shrink()
                                : KButton(
                                    key: ValueKey('bonus-claim-${c.id}'),
                                    label: t('rewards.claim.button'),
                                    size: KButtonSize.sm,
                                    expand: true,
                                    onPressed: () => showKSheet<void>(
                                      context,
                                      builder: (_) => _ClaimSheet(c: c, onClaimed: onClaimed),
                                    ),
                                  ))
                          : KButton(label: t('rewards.card.notEligible'), size: KButtonSize.sm, variant: KButtonVariant.surface, expand: true, onPressed: null),
                    ),
                    const SizedBox(width: 8),
                    KButton(
                      label: t('rewards.claim.terms'),
                      icon: LucideIcons.fileText,
                      size: KButtonSize.sm,
                      variant: KButtonVariant.surface,
                      onPressed: () => showKSheet<void>(
                        context,
                        title: t('rewards.claim.termsTitle', {'name': c.name}),
                        builder: (_) => _TermsSheet(c: c),
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _TermsSheet extends StatelessWidget {
  const _TermsSheet({required this.c});
  final Campaign c;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    return KSheetContent(
      footer: KButton(
        label: t('common.close'),
        variant: KButtonVariant.surface,
        size: KButtonSize.lg,
        expand: true,
        onPressed: () => Navigator.of(context).maybePop(),
      ),
      children: [
        Text(
          _headline(f, c),
          textAlign: TextAlign.center,
          style: context.text.footnote.copyWith(color: k.fg3),
        ),
        const SizedBox(height: 10),
        KKeyValues(_termsRows(f, c), dense: true),
        if (c.terms.isNotEmpty) ...[const SizedBox(height: 12), Text(c.terms, style: context.text.footnote.copyWith(color: k.fg2, height: 1.5))],
      ],
    );
  }
}

class _ClaimSheet extends ConsumerStatefulWidget {
  const _ClaimSheet({required this.c, required this.onClaimed});
  final Campaign c;
  final VoidCallback onClaimed;

  @override
  ConsumerState<_ClaimSheet> createState() => _ClaimSheetState();
}

class _ClaimSheetState extends ConsumerState<_ClaimSheet> {
  int? _login;
  bool _busy = false;

  Campaign get c => widget.c;
  bool get _fixed => !c.deposit;

  Future<void> _claim() async {
    final t = context.t;
    final f = GrowthFmt(t);
    final toasts = ref.read(notificationsProvider.notifier);
    final nav = Navigator.of(context);
    setState(() => _busy = true);
    try {
      final r = await growthPost(ref, 'bonuses/${c.id}/claim', _fixed ? {'login': _login} : {});
      final g = Grant.fromJson(r['grant'] is Map ? (r['grant'] as Map).cast<String, dynamic>() : const {});
      KHaptics.success();
      nav.maybePop();
      widget.onClaimed();
      ref.invalidate(accountsProvider);
      toasts.toast(
        NotificationKind.success,
        t('rewards.claim.toastClaimed', {'name': c.name}),
        description: g.status == 'awaiting_deposit'
            ? (g.claimDeadline != null
                  ? t('rewards.claim.toastDepositBy', {'amount': f.usd0(c.minDeposit), 'date': f.date(g.claimDeadline)})
                  : t('rewards.claim.toastDeposit', {'amount': f.usd0(c.minDeposit)}))
            : (g.login != null
                  ? t('rewards.claim.toastBonusOn', {'amount': f.usd0(g.amount), 'login': g.login})
                  : t('rewards.claim.toastBonus', {'amount': f.usd0(g.amount)})),
      );
    } on ApiException catch (e) {
      toasts.toast(NotificationKind.error, t('rewards.claim.error'), description: growthError(e, t));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final f = GrowthFmt(t);
    return KSheetContent(
      footer: KButton(
        key: const ValueKey('bonus-claim-confirm'),
        label: t('rewards.claim.confirm'),
        size: KButtonSize.lg,
        expand: true,
        loading: _busy,
        onPressed: _fixed && _login == null ? null : _claim,
      ),
      children: [
        Text(t('rewards.claim.title', {'name': c.name}), textAlign: TextAlign.center, style: context.text.title2),
        const SizedBox(height: 4),
        Text(
          _fixed ? t('rewards.claim.descFixed') : t('rewards.claim.descDeposit'),
          textAlign: TextAlign.center,
          style: context.text.footnote.copyWith(color: k.fg3),
        ),
        const SizedBox(height: 12),
        KKeyValues(_termsRows(f, c), dense: true),
        if (c.terms.isNotEmpty) ...[
          const SizedBox(height: 10),
          RowBox(
            border: k.line,
            child: Text(
              c.terms,
              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, height: 1.5),
            ),
          ),
        ],
        if (_fixed) ...[
          const SizedBox(height: 14),
          Text(t('rewards.picker.label'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 8),
          LiveAccountPicker(value: _login, onChanged: (v) => setState(() => _login = v), hint: t('rewards.claim.accountHint')),
        ],
        const SizedBox(height: 12),
        Text(t('rewards.claim.note'), style: context.text.footnote.copyWith(color: k.fg3)),
      ],
    );
  }
}

/* ------------------------------------------------------------------ my bonuses */

class _GrantRow extends ConsumerWidget {
  const _GrantRow({required this.g});
  final Grant g;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final walletOn = ref.watch(configProvider).moduleOn('wallet');
    final f = GrowthFmt(t);
    final pct = g.lotsRequired > 0 ? (g.lotsTraded / g.lotsRequired * 100).clamp(0.0, 100.0) : g.progressPct;
    final ended = const ['completed', 'forfeited', 'expired', 'cancelled', 'failed'].contains(g.status);
    final small = context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400);
    final row = RowBox(
      key: ValueKey('bonus-grant-${g.id}'),
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
                          g.campaign,
                          style: context.text.callout.copyWith(fontWeight: FontWeight.w600, color: k.fg, fontSize: 14),
                        ),
                        GrowthStatus(g.status),
                      ],
                    ),
                    const SizedBox(height: 2),
                    Text(
                      '${g.login != null ? '#${g.login}' : t('rewards.grant.noAccount')}'
                      '${(g.depositAmount ?? 0) > 0 ? t('rewards.grant.onDeposit', {'amount': f.usd(g.depositAmount!)}) : ''}'
                      '${t('rewards.grant.claimed', {'date': f.date(g.claimedAt)})}'
                      '${g.source.isNotEmpty && g.source != 'claim' ? ' · ${titleCase(g.source)}' : ''}',
                      style: small,
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 10),
              Column(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Text(
                    g.status == 'awaiting_deposit' ? '—' : f.usd(g.amount),
                    textDirection: TextDirection.ltr,
                    style: context.text.figure.copyWith(fontSize: 18),
                  ),
                  Text(
                    t('rewards.unit.bonus'),
                    style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                  ),
                ],
              ),
            ],
          ),
          const SizedBox(height: 10),
          if (g.status == 'awaiting_deposit')
            Container(
              padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
              decoration: BoxDecoration(
                color: k.warnSoft,
                borderRadius: BorderRadius.circular(12),
                border: Border.all(color: k.warn.withValues(alpha: 0.25)),
              ),
              child: Row(
                children: [
                  Expanded(
                    child: Text(
                      g.claimDeadline != null ? t('rewards.grant.depositPromptBy', {'date': f.date(g.claimDeadline)}) : t('rewards.grant.depositPrompt'),
                      style: context.text.footnote.copyWith(color: k.warn),
                    ),
                  ),
                  if (walletOn) ...[
                    const SizedBox(width: 8),
                    KTextButton(label: t('rewards.grant.deposit'), color: k.warn, onPressed: () => context.go('/wallet/deposit')),
                  ],
                ],
              ),
            )
          else ...[
            Wrap(
              alignment: WrapAlignment.spaceBetween,
              spacing: 8,
              runSpacing: 2,
              children: [
                Text(
                  t('rewards.grant.lotsProgress', {'traded': f.lots(g.lotsTraded), 'required': f.lots(g.lotsRequired), 'amount': f.usd(g.releasePerLot)}),
                  style: small.copyWith(fontFeatures: kTabular),
                ),
                Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(
                        text: t('rewards.grant.released', {'amount': f.usd(g.released)}),
                        style: TextStyle(color: k.up),
                      ),
                      TextSpan(text: t('rewards.grant.remaining', {'amount': f.usd(g.remaining)})),
                    ],
                  ),
                  style: small.copyWith(fontFeatures: kTabular),
                ),
              ],
            ),
            const SizedBox(height: 6),
            KProgressBar(value: pct / 100, color: g.status == 'completed' ? k.up : k.gold, height: 8),
            const SizedBox(height: 6),
            Row(
              children: [
                Expanded(child: Text(t('rewards.grant.pctReleased', {'pct': pct.round()}), style: small)),
                Flexible(
                  child: Text(
                    ended
                        ? '${g.endReason != null ? titleCase(g.endReason!) : t.dyn('rewards.status.${g.status}', fallback: titleCase(g.status))} ${f.date(g.endedAt)}'
                        : (g.expiresAt != null ? t('rewards.grant.expires', {'date': f.dateTime(g.expiresAt)}) : ''),
                    textAlign: TextAlign.end,
                    style: small,
                  ),
                ),
              ],
            ),
          ],
        ],
      ),
    );
    return ended ? Opacity(opacity: 0.7, child: row) : row;
  }
}

/* ------------------------------------------------------------------ promo code */

class _PromoCard extends ConsumerStatefulWidget {
  const _PromoCard({required this.onApplied});
  final VoidCallback onApplied;

  @override
  ConsumerState<_PromoCard> createState() => _PromoCardState();
}

class _PromoCardState extends ConsumerState<_PromoCard> {
  final _code = TextEditingController();
  int? _login;
  bool _busy = false;
  ({bool ok, String message})? _result;

  @override
  void dispose() {
    _code.dispose();
    super.dispose();
  }

  Future<void> _submit() async {
    final t = context.t;
    final code = _code.text.trim();
    if (code.isEmpty) return;
    FocusScope.of(context).unfocus();
    setState(() {
      _busy = true;
      _result = null;
    });
    try {
      final r = await growthPost(ref, 'promo', _login != null ? {'code': code, 'login': _login} : {'code': code});
      final res = r['result'] is Map ? (r['result'] as Map).cast<String, dynamic>() : const <String, dynamic>{};
      final msg = res['message'] is String && (res['message'] as String).isNotEmpty ? res['message'] as String : t('rewards.code.applied');
      KHaptics.success();
      _code.clear();
      if (mounted) setState(() => _result = (ok: true, message: msg));
      widget.onApplied();
      ref
        ..invalidate(growthRewardsProvider)
        ..invalidate(vouchersProvider);
    } on ApiException catch (e) {
      KHaptics.error();
      if (mounted) setState(() => _result = (ok: false, message: growthError(e, t)));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final live = [
      for (final a in ref.watch(accountsProvider).value ?? const <EngineAccount>[])
        if (a.live && !a.archived) a,
    ];
    final res = _result;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('rewards.code.title'), subtitle: t('rewards.code.subtitle'), icon: LucideIcons.ticket, tone: KTone.lavender),
          const SizedBox(height: 14),
          KTextField(
            key: const ValueKey('promo-input'),
            controller: _code,
            placeholder: t('rewards.code.placeholder'),
            ltr: true,
            textCapitalization: TextCapitalization.characters,
            inputFormatters: [
              FilteringTextInputFormatter.deny(RegExp(r'\s')),
              LengthLimitingTextInputFormatter(40),
              TextInputFormatter.withFunction((o, n) => n.copyWith(text: n.text.toUpperCase())),
            ],
            textInputAction: TextInputAction.done,
            onChanged: (_) => setState(() {}),
            onSubmitted: (_) => _submit(),
            enabled: !readOnly,
          ),
          if (live.isNotEmpty) ...[
            const SizedBox(height: 10),
            KPickerField(
              label: t('rewards.code.account'),
              value: _login == null ? t('rewards.code.anyAccount') : '#$_login · ${live.where((a) => a.login == _login).firstOrNull?.groupName ?? ''}',
              onTap: readOnly
                  ? null
                  : () async {
                      final v = await showKPicker<int>(
                        context,
                        title: t('rewards.code.account'),
                        selected: _login ?? 0,
                        options: [KPickOption(0, t('rewards.code.anyAccount')), for (final a in live) KPickOption(a.login, '#${a.login} · ${a.groupName}')],
                      );
                      if (v != null && mounted) setState(() => _login = v == 0 ? null : v);
                    },
            ),
          ],
          const SizedBox(height: 12),
          if (!readOnly)
            KButton(
              key: const ValueKey('promo-submit'),
              label: t('rewards.code.apply'),
              variant: KButtonVariant.outline,
              expand: true,
              loading: _busy,
              onPressed: _code.text.trim().isEmpty ? null : _submit,
            ),
          if (res != null) ...[
            const SizedBox(height: 10),
            KNotice(
              key: ValueKey('promo-result-${res.ok}'),
              text: res.message,
              tone: res.ok ? KChipTone.up : KChipTone.down,
              icon: res.ok ? LucideIcons.circleCheck : LucideIcons.circleAlert,
            ),
          ],
        ],
      ),
    );
  }
}

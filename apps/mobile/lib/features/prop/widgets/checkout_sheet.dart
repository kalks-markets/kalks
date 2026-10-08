// The buy flow (web CheckoutDialog in components/prop-live/catalogue.tsx): the fee from the USDT wallet and the
// wallet balance, every rule of the plan at this size, the agreement, Pay; then the outcome with the trading
// credentials (shown once). One idempotency key per sheet: a retry after an error reuses it, so the fee is never
// charged twice, and nothing is ever retried on its own.
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/config/app_config.dart';
import '../../../core/format/format.dart';
import '../../../data/client_data.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../prop_api.dart';
import 'prop_ui.dart';

/// Every rule of a plan at one size, as label / value rows (web planRules).
List<KKV> planRules(T t, Plan p, PlanSize s) {
  final rows = <KKV>[];
  for (final ph in p.phases) {
    rows
      ..add(KKV(t('prop.rules.phaseTarget', {'phase': ph.name}), '${pctText(ph.target)} · ${usd(s.size * ph.target / 100, 0)}'))
      ..add(KKV(t('prop.rules.phaseMinDays', {'phase': ph.name}), daysText(t, ph.minDays)))
      ..add(KKV(t('prop.rules.phaseTimeLimit', {'phase': ph.name}), ph.timeLimit > 0 ? daysText(t, ph.timeLimit) : t('prop.noTimeLimit')));
  }
  if (p.phases.isEmpty) rows.add(KKV(t('prop.rules.evaluation'), t('prop.rules.evaluationNone')));
  final ddBase = '${pctText(p.maxDD)} ${ddTypeLabel(t, p.ddType)}';
  final dd = p.ddType == 'trailing' && p.trailingLock ? t('prop.rules.ddLocks', {'dd': ddBase}) : ddBase;
  rows
    ..add(
      KKV(
        t('prop.dailyLossLimit'),
        t(p.dailyBasis == 'equity' ? 'prop.rules.dailyLossEquity' : 'prop.rules.dailyLossBalance', {
          'pct': numText(p.dailyLoss),
          'amount': usd(s.size * p.dailyLoss / 100, 0),
        }),
      ),
    )
    ..add(KKV(t('prop.rule.maxDrawdown'), '$dd · ${usd(s.size * p.maxDD / 100, 0)}'))
    ..add(
      KKV(t('prop.rule.consistency'), p.consistency > 0 ? t('prop.rules.consistencyValue', {'pct': numText(p.consistency)}) : t('prop.rules.noConsistency')),
    )
    ..add(
      KKV(
        t('prop.newsTrading'),
        p.newsTrading ? t('prop.allowed') : t(p.newsBreachFails ? 'prop.rules.newsBlockedFails' : 'prop.rules.newsBlocked', {'min': p.newsWindow}),
      ),
    )
    ..add(KKV(t('prop.rule.weekendHolding'), p.weekendHolding ? t('prop.allowed') : t('prop.rules.weekendClosed')))
    ..add(KKV(t('prop.expertAdvisors'), p.eaAllowed ? t('prop.allowed') : t('prop.notAllowed')))
    ..add(KKV(t('prop.bannedStrategies'), p.banned.isNotEmpty ? p.banned.map((b) => bannedLabel(t, b)).join(', ') : t('prop.none')))
    ..add(
      KKV(
        t('prop.profitSplit'),
        p.splitMax > p.split ? t('prop.rules.splitScaling', {'split': numText(p.split), 'max': numText(p.splitMax)}) : pctText(p.split),
      ),
    )
    ..add(
      KKV(
        t('prop.firstPayout'),
        t('prop.rules.firstPayoutValue', {'days': daysText(t, p.firstPayoutDays), 'freq': payoutFreqLabel(t, p.payoutFreq), 'min': usd(p.minPayout, 0)}),
      ),
    )
    ..add(KKV(t('prop.feeRefund'), p.refundFee ? t('prop.rules.refunded') : t('prop.nonRefundable')))
    ..add(KKV(t('prop.leverage'), '1:${s.leverage}'));
  return rows;
}

String _newKey() {
  const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
  final r = math.Random.secure();
  return 'k${DateTime.now().millisecondsSinceEpoch.toRadixString(36)}${List.generate(16, (_) => chars[r.nextInt(chars.length)]).join()}';
}

/// Opens the buy sheet. Not dismissible by a swipe: a payment in flight and the one-time passwords must not be lost.
Future<void> showCheckoutSheet(BuildContext context, {required Plan plan, required PlanSize size}) => showKSheet<void>(
  context,
  dismissible: false,
  builder: (_) => CheckoutSheet(plan: plan, size: size),
);

class CheckoutSheet extends ConsumerStatefulWidget {
  const CheckoutSheet({super.key, required this.plan, required this.size});
  final Plan plan;
  final PlanSize size;

  @override
  ConsumerState<CheckoutSheet> createState() => _CheckoutSheetState();
}

class _CheckoutSheetState extends ConsumerState<CheckoutSheet> {
  final String _key = _newKey();
  bool _agree = false;
  bool _busy = false;
  Object? _err;
  PurchaseResult? _done;

  Future<void> _pay() async {
    setState(() {
      _busy = true;
      _err = null;
    });
    try {
      final r = await buyChallenge(ref.read(apiProvider), planId: widget.plan.id, size: widget.size.size, key: _key);
      KHaptics.success();
      ref
        ..invalidate(propChallengesProvider)
        ..invalidate(walletOverviewProvider);
      if (mounted) setState(() => _done = r);
    } on Object catch (e) {
      KHaptics.error();
      if (mounted) setState(() => _err = e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _close() => Navigator.of(context).pop();

  void _go(String href) {
    final router = GoRouter.of(context);
    _close();
    router.go(href);
  }

  @override
  Widget build(BuildContext context) => _done != null ? _outcome(context, _done!) : _form(context);

  Widget _header(BuildContext context, String title, String description) {
    final k = context.k;
    return Padding(
      padding: const EdgeInsetsDirectional.fromSTEB(20, 0, 8, 12),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: context.text.title2),
                const SizedBox(height: 3),
                Text(description, style: context.text.footnote.copyWith(color: k.fg3)),
              ],
            ),
          ),
          KIconButton(icon: LucideIcons.x, size: 36, semanticLabel: context.t('common.close'), onPressed: _busy ? null : _close),
        ],
      ),
    );
  }

  Widget _form(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final plan = widget.plan, size = widget.size;
    final fee = feeText(size.fee);
    final wallet = ref.watch(walletOverviewProvider).value;
    final available = wallet == null ? null : double.tryParse(wallet.usdt.available) ?? 0;
    final short = available != null && available < size.fee;
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _header(context, t('prop.checkout.title', {'plan': plan.name}), t('prop.checkout.description', {'size': sizeLabel(size.size), 'fee': fee})),
        Flexible(
          child: KSheetContent(
            footer: Row(
              children: [
                KButton(label: t('common.cancel'), variant: KButtonVariant.surface, size: KButtonSize.lg, onPressed: _busy ? null : _close),
                const SizedBox(width: 10),
                Expanded(
                  child: KButton(
                    label: _err != null ? t('prop.checkout.retry', {'fee': fee}) : t('prop.checkout.pay', {'fee': fee}),
                    icon: LucideIcons.wallet,
                    size: KButtonSize.lg,
                    expand: true,
                    loading: _busy,
                    onPressed: !_agree || short ? null : _pay,
                  ),
                ),
              ],
            ),
            children: [
              PropRow(
                child: Row(
                  children: [
                    Container(
                      width: 36,
                      height: 36,
                      decoration: BoxDecoration(
                        shape: BoxShape.circle,
                        color: k.surface3,
                        border: Border.all(color: k.line),
                      ),
                      child: Icon(LucideIcons.wallet, size: 16, color: k.fg2),
                    ),
                    const SizedBox(width: 12),
                    Expanded(
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(t('prop.store.paidFromWallet'), style: context.text.label.copyWith(fontSize: 13.5)),
                          const SizedBox(height: 2),
                          Text(
                            plan.refundFee ? t('prop.checkout.chargedOnceRefund') : t('prop.checkout.chargedOnceNoRefund'),
                            style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                          ),
                        ],
                      ),
                    ),
                    const SizedBox(width: 10),
                    Text(fee, textDirection: TextDirection.ltr, style: context.text.figure.copyWith(fontSize: 18)),
                  ],
                ),
              ),
              if (available != null) ...[
                const SizedBox(height: 10),
                if (short)
                  KNotice(
                    tone: KChipTone.warn,
                    text: t('prop.checkout.short', {'balance': Fmt.amount(available), 'missing': Fmt.amount(size.fee - available)}),
                    action: ref.watch(configProvider).moduleOn('wallet')
                        ? KButton(
                            label: t('prop.checkout.deposit'),
                            trailingIcon: arrowEnd(context),
                            variant: KButtonVariant.surface,
                            size: KButtonSize.sm,
                            onPressed: () => _go('/wallet/deposit'),
                          )
                        : null,
                  )
                else
                  Padding(
                    padding: const EdgeInsetsDirectional.only(start: 4),
                    child: Text(t('prop.checkout.walletBalance', {'balance': Fmt.amount(available)}), style: context.text.footnote.copyWith(color: k.fg3)),
                  ),
              ],
              const SizedBox(height: 18),
              PropLabel(t('prop.checkout.rulesTitle')),
              const SizedBox(height: 8),
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 14),
                decoration: BoxDecoration(
                  borderRadius: BorderRadius.circular(14),
                  border: Border.all(color: k.line),
                ),
                child: KKeyValues(planRules(t, plan, size), dense: true),
              ),
              const SizedBox(height: 8),
              Text(
                t('prop.checkout.limitsNote'),
                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
              ),
              const SizedBox(height: 14),
              KCheckRow(
                value: _agree,
                onChanged: _busy ? (_) {} : (v) => setState(() => _agree = v),
                child: Text(t('prop.checkout.agree'), style: context.text.footnote.copyWith(color: k.fg2, height: 1.4)),
              ),
              if (_err != null) ...[const SizedBox(height: 12), PropErrorNote(error: _err, onNavigate: _go)],
              if (_err != null && propErrorCode(_err) == 'provisioning') ...[
                const SizedBox(height: 4),
                Align(
                  alignment: AlignmentDirectional.centerStart,
                  child: KTextButton(label: t('prop.checkout.goToMine'), color: k.ember, onPressed: () => _go('/prop/mine')),
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }

  Widget _outcome(BuildContext context, PurchaseResult done) {
    final t = context.t;
    final k = context.k;
    final plan = widget.plan, size = widget.size;
    final fee = feeText(size.fee);
    final creds = done.credentials;
    final login = creds?.login ?? done.challenge.current?.login;
    final phase = done.challenge.current?.phase ?? (plan.phases.isNotEmpty ? plan.phases.first.name : t('prop.status.funded'));
    return Column(
      mainAxisSize: MainAxisSize.min,
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        _header(context, t('prop.checkout.readyTitle'), '${plan.name} · ${sizeLabel(size.size)} · $phase'),
        Flexible(
          child: KSheetContent(
            footer: Row(
              children: [
                Expanded(
                  child: KButton(
                    label: t('prop.myChallenges'),
                    trailingIcon: arrowEnd(context),
                    variant: KButtonVariant.surface,
                    size: KButtonSize.lg,
                    expand: true,
                    onPressed: () => _go('/prop/mine'),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: PropTradeButton(login: login, label: t('prop.tradeNow'), size: KButtonSize.lg, onBeforeOpen: _close),
                ),
              ],
            ),
            children: [
              KNotice(tone: KChipTone.up, icon: LucideIcons.check, text: t('prop.checkout.paidText', {'fee': fee, 'size': sizeLabel(size.size)})),
              const SizedBox(height: 16),
              if (creds != null) ...[
                CredentialField(label: t('prop.cred.login'), value: '${creds.login}'),
                const SizedBox(height: 12),
                CredentialField(label: t('prop.cred.server'), value: 'Kalks-Live', mono: false),
                const SizedBox(height: 12),
                CredentialField(label: t('prop.cred.password'), value: creds.password, secret: true),
                const SizedBox(height: 12),
                CredentialField(label: t('prop.cred.investorPassword'), value: creds.investorPassword, secret: true),
                const SizedBox(height: 14),
                Text(t('prop.checkout.savePasswords'), style: context.text.footnote.copyWith(color: k.fg3)),
              ] else
                Text(t('prop.checkout.passwordsShown'), style: context.text.callout.copyWith(color: k.fg2)),
            ],
          ),
        ),
      ],
    );
  }
}

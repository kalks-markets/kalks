// The account detail's Credentials and Settings tabs (web components/trading/manage.tsx CredentialsPanel /
// SettingsPanel: LeverageCard with the emailed code, DemoFundsCard, Account details).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/format/format.dart';
import '../../../core/models/account.dart';
import '../../../core/models/trading.dart';
import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../account_actions.dart';
import '../accounts_data.dart';
import 'account_bits.dart';
import 'account_sheets.dart';
import 'detail_panels.dart';

/* ------------------------------------------------------------------ Credentials */

class CredentialsPanel extends StatelessWidget {
  const CredentialsPanel({super.key, required this.account});
  final EngineAccount account;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = account;
    Widget pwRow({required IconData icon, required bool ember, required String title, required Widget chip, required String desc, required String kind}) =>
        RowBox(
          children: [
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 40,
                  height: 40,
                  decoration: BoxDecoration(
                    shape: BoxShape.circle,
                    color: ember ? k.emberSoft : k.surface3,
                    border: Border.all(color: ember ? k.ember.withValues(alpha: 0.3) : k.line),
                  ),
                  child: Icon(icon, size: 16, color: ember ? k.ember : k.fg2),
                ),
                const SizedBox(width: 12),
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
                            title,
                            style: context.text.label.copyWith(color: k.fg, fontSize: 14, fontWeight: FontWeight.w600),
                          ),
                          chip,
                        ],
                      ),
                      const SizedBox(height: 3),
                      Text(desc, style: context.text.footnote.copyWith(color: k.fg3)),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 12),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: KButton(
                key: ValueKey('set-$kind-password'),
                label: t(kind == 'trading' ? 'accountDetail.creds.setTrading' : 'accountDetail.creds.setInvestor'),
                icon: LucideIcons.keyRound,
                variant: KButtonVariant.surface,
                size: KButtonSize.sm,
                onPressed: () => unawaited(showChangePasswordSheet(context, a, kind)),
              ),
            ),
          ],
        );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        DetailCard(
          title: t('accountDetail.creds.title'),
          subtitle: t('accountDetail.creds.subtitleOptional'),
          icon: LucideIcons.lock,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              SecretField(label: t('accountDetail.info.login'), value: '${a.login}'),
              const SizedBox(height: 12),
              SecretField(label: t('accountDetail.info.server'), value: a.server, hint: 'GMT+3 / GMT+2'),
              const SizedBox(height: 14),
              pwRow(
                icon: LucideIcons.lock,
                ember: true,
                title: t('accountDetail.pw.trading'),
                chip: KChip(label: t('accountDetail.creds.fullAccess'), small: true),
                desc: t('accountDetail.creds.tradingWhy'),
                kind: 'trading',
              ),
              const SizedBox(height: 10),
              pwRow(
                icon: LucideIcons.eye,
                ember: false,
                title: t('accountDetail.pw.investor'),
                chip: KChip(label: t('accountDetail.creds.readOnly'), tone: KChipTone.info, small: true),
                desc: t('accountDetail.creds.investorWhy'),
                kind: 'investor',
              ),
              const SizedBox(height: 12),
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Padding(
                    padding: const EdgeInsets.only(top: 1),
                    child: Icon(LucideIcons.triangleAlert, size: 14, color: k.warn),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(t('accountDetail.creds.securityNote'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)),
                  ),
                ],
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        DetailCard(
          title: t('accountDetail.investor.title'),
          subtitle: t('accountDetail.investor.subtitle'),
          icon: LucideIcons.users,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              for (final (i, step) in [
                t('accountDetail.investor.step1'),
                t('accountDetail.investor.step2', {'login': a.login, 'server': a.server}),
                t('accountDetail.investor.step3'),
                t('accountDetail.investor.step4'),
              ].indexed)
                Padding(
                  padding: const EdgeInsets.only(bottom: 10),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Container(
                        width: 24,
                        height: 24,
                        alignment: Alignment.center,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: k.surface3,
                          border: Border.all(color: k.line),
                        ),
                        child: Text(
                          '${i + 1}',
                          style: context.text.caption.copyWith(color: k.fg2, fontFeatures: kTabular),
                        ),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(step, style: context.text.callout.copyWith(color: k.fg2)),
                        ),
                      ),
                    ],
                  ),
                ),
              const SizedBox(height: 4),
              RowBox(
                padding: const EdgeInsets.fromLTRB(14, 10, 10, 10),
                children: [
                  Row(
                    children: [
                      Container(
                        width: 36,
                        height: 36,
                        decoration: BoxDecoration(
                          shape: BoxShape.circle,
                          color: k.surface3,
                          border: Border.all(color: k.line),
                        ),
                        child: Icon(LucideIcons.globe, size: 16, color: k.fg2),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              'Kalks Trader',
                              style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
                            ),
                            Text(
                              t('accountDetail.investor.webTerminal'),
                              style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                            ),
                          ],
                        ),
                      ),
                      TradeButton(account: a, size: KButtonSize.sm, label: t('common.open')),
                    ],
                  ),
                ],
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ Settings */

class SettingsPanel extends StatelessWidget {
  const SettingsPanel({super.key, required this.account, required this.onChanged});
  final EngineAccount account;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final a = account;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        LeverageCard(account: a, onChanged: onChanged),
        if (!a.live && a.demo != null) ...[const SizedBox(height: 16), DemoFundsCard(account: a, onChanged: onChanged)],
        const SizedBox(height: 16),
        DetailCard(
          title: t('accountDetail.details.title'),
          child: KKeyValues([
            KKV(t('accountDetail.info.login'), '${a.login}', mono: true),
            KKV(t('common.type'), '${a.live ? t('common.live') : t('common.demo')} · ${a.groupName}'),
            KKV(t('accountDetail.info.positionMode'), t.dyn('accounts.mode.${a.mode}', fallback: modeLabel(a.mode))),
            KKV(t('common.currency'), a.cent ? t('accountDetail.info.uscCents') : a.currency),
            KKV(t('accountDetail.info.marginCallStopOut'), '${a.marginCallLevel}% / ${a.stopOutLevel}%', mono: true),
            KKV(t('common.status'), null, valueWidget: StatusChip(status: a.status)),
            KKV(t('accountDetail.info.opened'), fmtDate(t, a.createdAt)),
          ]),
        ),
      ],
    );
  }
}

/// Leverage with the emailed code (web LeverageCard): locked while positions are open.
class LeverageCard extends ConsumerStatefulWidget {
  const LeverageCard({super.key, required this.account, required this.onChanged});
  final EngineAccount account;
  final VoidCallback onChanged;

  @override
  ConsumerState<LeverageCard> createState() => _LeverageCardState();
}

class _LeverageCardState extends ConsumerState<LeverageCard> {
  late int _lev = widget.account.leverage;

  @override
  void didUpdateWidget(LeverageCard old) {
    super.didUpdateWidget(old);
    if (old.account.leverage != widget.account.leverage) _lev = widget.account.leverage;
  }

  /// After the emailed code: the change itself.
  Future<void> _apply(String token) async {
    final t = context.t;
    final a = widget.account;
    try {
      final r = await apiOf(context).post<Map<String, dynamic>>('trading/accounts/${a.login}/leverage', body: {'leverage': _lev, 'stepup_token': token});
      if (!mounted) return;
      final from = (r['from'] as num?)?.toInt() ?? a.leverage;
      final to = (r['leverage'] as num?)?.toInt() ?? _lev;
      accountToast(context, NotificationKind.success, t('accountDetail.leverage.changed'), description: '#${a.login}: ${levLabel(from)} → ${levLabel(to)}');
      refreshAccountData(ProviderScope.containerOf(context, listen: false));
      widget.onChanged();
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accountDetail.leverage.changeError'), e);
    }
  }

  Future<void> _confirm() async {
    final t = context.t;
    final a = widget.account;
    await showStepUpSheet(
      context,
      action: 'leverage',
      target: '${a.login}',
      title: t('accountDetail.leverage.confirmTitle'),
      description: '#${a.login}: ${levLabel(a.leverage)} → ${levLabel(_lev)}',
      what: t('accountDetail.leverage.stepUpWhat', {'login': a.login, 'value': levLabel(_lev)}),
      confirmLabel: t('accountDetail.leverage.confirmApply'),
      onConfirmed: _apply,
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = widget.account;
    final locked = a.positions > 0;
    final levs = a.leverages.isEmpty ? [a.leverage] : a.leverages;
    return DetailCard(
      title: t('accountDetail.leverage.title'),
      subtitle: t('accountDetail.leverage.available', {'group': a.groupName, 'list': levs.map(levLabel).join(' · ')}),
      action: KChip(label: t('accountDetail.leverage.current', {'value': levLabel(a.leverage)}), tone: KChipTone.ember, small: true),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (locked) ...[
            KNotice(
              tone: KChipTone.warn,
              icon: LucideIcons.lock,
              title: t('accountDetail.leverage.lockedTitle'),
              text: t('accountDetail.leverage.lockedTextTrader', {'count': a.positions}),
            ),
            const SizedBox(height: 14),
          ],
          LeveragePills(values: levs, selected: _lev, enabled: !locked, onSelect: (l) => setState(() => _lev = l)),
          const SizedBox(height: 14),
          Row(
            children: [
              Expanded(
                child: Text(t('accountDetail.leverage.hint'), style: context.text.footnote.copyWith(color: k.fg3)),
              ),
              const SizedBox(width: 12),
              KButton(label: t('common.apply'), size: KButtonSize.sm, onPressed: locked || _lev == a.leverage ? null : _confirm),
            ],
          ),
        ],
      ),
    );
  }
}

/// Demo funds: the balance, refills left today and Refill (web DemoFundsCard).
class DemoFundsCard extends StatelessWidget {
  const DemoFundsCard({super.key, required this.account, required this.onChanged});
  final EngineAccount account;
  final VoidCallback onChanged;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = account;
    final left = refillsLeft(a);
    final total = (a.demo?['refillsPerDay'] as num?)?.toInt() ?? 0;
    final target = demoTarget(a) ?? 0;
    final full = demoFull(a);
    final days = a.demo?['expiryDays'] ?? 0;
    return DetailCard(
      title: t('accountDetail.demoFunds.title'),
      subtitle: t('accountDetail.demoFunds.subtitle', {'amount': '${a.currencyPrefix}${Fmt.number(target, 0)}'}),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KMoney(a.balance, currency: a.cent ? 'USC' : a.currency, style: context.text.moneyL),
          const SizedBox(height: 16),
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    KRichText(
                      t('accountDetail.demoFunds.refillsLeft', {'left': left, 'total': total}),
                      style: context.text.footnote.copyWith(color: k.fg2),
                      tags: const {'n': KTag()},
                    ),
                    const SizedBox(height: 6),
                    Row(
                      children: [
                        for (var i = 0; i < total; i++)
                          Container(
                            width: 28,
                            height: 6,
                            margin: const EdgeInsetsDirectional.only(end: 4),
                            decoration: BoxDecoration(color: i < left ? k.gold : k.surface3, borderRadius: BorderRadius.circular(3)),
                          ),
                      ],
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              RefillButton(account: a, onDone: onChanged, label: t('accountDetail.header.refill')),
            ],
          ),
          const SizedBox(height: 14),
          Text(
            '${full ? '${t('accountDetail.demoFunds.full')} ' : ''}${t('accountDetail.demoFunds.resetNote', {'days': days})}',
            style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
          ),
        ],
      ),
    );
  }
}

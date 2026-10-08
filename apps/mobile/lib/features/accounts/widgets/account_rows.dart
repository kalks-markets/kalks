// Account rows of My accounts (web components/trading/ui.tsx LiveAccountRow, archive.tsx ArchivedAccountRow), in
// the web's phone layout: badges and the title, balance / equity / margin level, the actions (⋯, Fund or Refill,
// Trade), then the positions line.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/auth/auth_controller.dart';
import '../../../core/models/account.dart';
import '../../../core/models/trading.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../account_actions.dart';
import '../accounts_data.dart';
import 'account_bits.dart';

/// A row's frame (web k-row): surface-2 rounded box with a hairline, the whole row opens the account.
class _RowFrame extends StatelessWidget {
  const _RowFrame({required this.child, required this.onTap, required this.label});
  final Widget child;
  final VoidCallback onTap;
  final String label;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return KPressable(
      onTap: onTap,
      pressedScale: 0.995,
      pressedOpacity: 0.85,
      semanticLabel: label,
      child: Container(
        padding: const EdgeInsets.all(16),
        decoration: BoxDecoration(
          color: k.surface2.withValues(alpha: k.dark ? 0.7 : 0.85),
          borderRadius: BorderRadius.circular(18),
          border: Border.all(color: k.line),
        ),
        child: child,
      ),
    );
  }
}

class _Figure extends StatelessWidget {
  const _Figure({required this.label, required this.child});
  final String label;
  final Widget child;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    mainAxisSize: MainAxisSize.min,
    children: [
      Text(
        label,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: context.text.footnote.copyWith(color: context.k.fg3, fontSize: 12),
      ),
      const SizedBox(height: 4),
      FittedBox(fit: BoxFit.scaleDown, alignment: AlignmentDirectional.centerStart, child: child),
    ],
  );
}

/// A live / demo / prop account (web LiveAccountRow).
class LiveAccountRow extends ConsumerWidget {
  const LiveAccountRow({super.key, required this.account, this.onChanged});
  final EngineAccount account;
  final VoidCallback? onChanged;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final a = account;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final flavor = accountFlavor(a);
    final copying = flavor == 'copy' ? copyingName(a) : null;
    final cur = a.cent ? 'USC' : a.currency;
    final title = '${a.groupName} · ${t.dyn('accounts.mode.${a.mode}', fallback: a.mode)}';
    final footer = <Widget>[
      if (copying != null)
        Text(
          t('accounts.copy.copying', {'name': copying}),
          style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w600),
        ),
      if (a.positions > 0 || a.orders > 0)
        Text.rich(
          TextSpan(
            text:
                '${t('accounts.row.openPositions', {'count': a.positions})} · ${t('accounts.row.pendingOrders', {'count': a.orders})} · ${t('accounts.row.floating')} ',
            children: [
              TextSpan(
                text: fmtAmount(a.profit, a.currencyPrefix, signed: true),
                style: TextStyle(color: pnlColor(context, a.profit), fontWeight: FontWeight.w600, fontFeatures: kTabular),
              ),
            ],
          ),
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
        )
      else
        Text(
          t('accounts.row.noPositions'),
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
        ),
      if (a.live && a.balance == 0 && a.equity == 0) Text(t('accounts.row.notFunded'), style: context.text.caption.copyWith(color: k.warn)),
      if (!a.live && a.demo != null)
        Text(
          t('accounts.row.demoRefills', {'left': refillsLeft(a), 'total': a.demo!['refillsPerDay'] ?? 0, 'days': a.demo!['expiryDays'] ?? 0}),
          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
        ),
    ];
    return _RowFrame(
      label: '$title #${a.login}',
      onTap: () => context.push('/accounts/${a.login}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  crossAxisAlignment: WrapCrossAlignment.center,
                  children: [
                    KindBadge(account: a),
                    if (a.isOptions) ProductChip(account: a),
                    FlavorChip(account: a),
                    Text(title, style: context.text.headline.copyWith(fontWeight: FontWeight.w500)),
                    LoginCopy(login: a.login),
                    if (a.name.isNotEmpty)
                      Text(
                        '“${a.name}”',
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.label.copyWith(color: k.fg3),
                      ),
                    if (a.cent) const KChip(label: 'USC', tone: KChipTone.gold, small: true),
                    DefaultStar(account: a),
                    StatusBadge(account: a),
                    if (a.closureStatus == 'pending') KChip(label: t('accounts.close.pendingChip'), tone: KChipTone.info, small: true),
                    if (a.dormant) KChip(label: t('accounts.dormant.chip'), tone: KChipTone.warn, small: true),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              KChip(label: levLabel(a.leverage), small: true),
            ],
          ),
          const SizedBox(height: 14),
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                child: _Figure(
                  label: t('common.balance'),
                  child: KMoney(a.balance, currency: cur, style: context.text.figure.copyWith(fontSize: 16)),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: _Figure(
                  label: t('common.equity'),
                  child: KMoney(a.equity, currency: cur, style: context.text.figure.copyWith(fontSize: 16)),
                ),
              ),
              const SizedBox(width: 12),
              _Figure(
                label: t('accounts.label.marginLevel'),
                child: Text(
                  fmtLevel(a.marginLevel),
                  textDirection: TextDirection.ltr,
                  style: context.text.figure.copyWith(fontSize: 16, color: levelColor(context, a.marginLevel)),
                ),
              ),
            ],
          ),
          if (!readOnly) ...[
            const SizedBox(height: 14),
            Wrap(
              alignment: WrapAlignment.end,
              spacing: 8,
              runSpacing: 8,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: [
                AccountMenuButton(account: a, onChanged: onChanged, size: 36),
                if (a.live && !a.prop)
                  FundButton(account: a, size: KButtonSize.sm)
                else if (!a.live)
                  RefillButton(account: a, onDone: onChanged, size: KButtonSize.sm),
                if (flavor == 'copy') ...[
                  KButton(
                    label: t('accounts.copy.manage'),
                    icon: LucideIcons.users,
                    variant: KButtonVariant.surface,
                    size: KButtonSize.sm,
                    onPressed: () => context.go('/social/copy'),
                  ),
                  TradeButton(account: a, size: KButtonSize.sm, variant: KButtonVariant.surface, label: t('accounts.copy.watchPnl')),
                ] else
                  TradeButton(account: a, size: KButtonSize.sm),
              ],
            ),
          ],
          const SizedBox(height: 10),
          Wrap(spacing: 12, runSpacing: 4, children: footer),
        ],
      ),
    );
  }
}

/// An archived / closed account (web ArchivedAccountRow): final balance, when, statements, the ZIP and Restore.
class ArchivedAccountRow extends ConsumerStatefulWidget {
  const ArchivedAccountRow({super.key, required this.account, this.onChanged});
  final EngineAccount account;
  final VoidCallback? onChanged;

  @override
  ConsumerState<ArchivedAccountRow> createState() => _ArchivedAccountRowState();
}

class _ArchivedAccountRowState extends ConsumerState<ArchivedAccountRow> {
  bool _busy = false;

  Future<void> _restore() async {
    setState(() => _busy = true);
    final ok = await restoreAccount(context, widget.account);
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) {
      refreshAccountData(ProviderScope.containerOf(context, listen: false));
      widget.onChanged?.call();
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = widget.account;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final when = a.archivedAt ?? a.closedAt ?? a.updatedAt;
    return _RowFrame(
      label: '${a.groupName} #${a.login}',
      onTap: () => context.push('/accounts/${a.login}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Wrap(
            spacing: 8,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              KindBadge(account: a),
              if (a.isOptions) ProductChip(account: a),
              FlavorChip(account: a),
              Text(
                a.groupName,
                style: context.text.headline.copyWith(fontWeight: FontWeight.w500, color: k.fg2),
              ),
              LoginCopy(login: a.login, color: k.fg3),
              if (a.name.isNotEmpty)
                Text(
                  '“${a.name}”',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.label.copyWith(color: k.fg3),
                ),
              StatusBadge(account: a),
            ],
          ),
          const SizedBox(height: 14),
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              _Figure(
                label: t('accounts.archived.finalBalance'),
                child: KMoney(
                  a.balance,
                  currency: a.cent ? 'USC' : a.currency,
                  style: context.text.figure.copyWith(fontSize: 17, color: k.fg2),
                ),
              ),
              const SizedBox(width: 16),
              if (when != null)
                Expanded(
                  child: Text(
                    t(a.status == 'closed' ? 'accounts.archived.closedOn' : 'accounts.archived.on', {'date': fmtDate(t, when)}),
                    style: context.text.footnote.copyWith(color: k.fg3),
                  ),
                ),
            ],
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 4,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              KButton(
                label: t('accounts.archived.statements'),
                icon: LucideIcons.arrowDownToLine,
                variant: KButtonVariant.ghost,
                size: KButtonSize.sm,
                onPressed: () => context.push('/accounts/${a.login}?tab=history'),
              ),
              KButton(
                label: t('accounts.history.zip'),
                icon: LucideIcons.arrowDownToLine,
                variant: KButtonVariant.ghost,
                size: KButtonSize.sm,
                onPressed: () => unawaited(downloadHistoryZip(context, a.login)),
              ),
              if (!readOnly && a.status == 'archived')
                KButton(
                  label: t('accounts.archived.restore'),
                  icon: LucideIcons.rotateCcw,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  loading: _busy,
                  onPressed: _busy ? null : _restore,
                ),
            ],
          ),
        ],
      ),
    );
  }
}

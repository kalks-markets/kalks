// The account dialogs as iOS sheets (web Dialogs of apps/crm/components/trading):
//   FundSheet              ui.tsx FundDialog            Deposit USDT / Transfer from wallet
//   RenameSheet            archive.tsx RenameDialog     PATCH trading/accounts/{login} {name}
//   DeleteAccountSheet     archive.tsx                  archive-check, archive (live: step-up account_archive)
//   CloseAccountSheet      closure.tsx                  closure (survey, step-up account_close), ClosureNotice
//   ChangeTypeSheet        extras.tsx                   group-options, group
//   DemoBalanceSheet       extras.tsx                   demo-balance
//   TransferBetweenSheet   extras.tsx                   transfers/between (step-up internal_transfer, request id)
//   ChangePasswordSheet    manage.tsx                   passwords (step-up trading_password / investor_password)
// Each sheet resolves true when it changed something (the caller then reloads, the web's onDone).
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/format/format.dart';
import '../../../core/models/account.dart';
import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../accounts_data.dart';
import 'account_bits.dart';

/// The dialog's description under the sheet title.
class SheetDescription extends StatelessWidget {
  const SheetDescription(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 16),
    child: Text(
      text,
      textAlign: TextAlign.center,
      style: context.text.callout.copyWith(color: context.k.fg2),
    ),
  );
}

/// Cancel + the dialog's action, side by side (web Dialog footer).
class SheetFooter extends StatelessWidget {
  const SheetFooter({super.key, required this.primary, this.cancelLabel, this.onCancel});
  final Widget primary;
  final String? cancelLabel;
  final VoidCallback? onCancel;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Expanded(
        child: KButton(
          label: cancelLabel ?? context.t('common.cancel'),
          variant: KButtonVariant.surface,
          size: KButtonSize.lg,
          expand: true,
          onPressed: onCancel ?? () => Navigator.of(context).pop(false),
        ),
      ),
      const SizedBox(width: 10),
      Expanded(child: primary),
    ],
  );
}

/* ------------------------------------------------------------------ Fund (ui.tsx FundDialog) */

Future<void> showFundSheet(BuildContext context, EngineAccount a) {
  final t = context.t;
  final router = GoRouter.of(context);
  return showKSheet<void>(
    context,
    title: t('accounts.fund.title', {'login': a.login}),
    builder: (ctx) {
      final k = ctx.k;
      void go(String to) {
        Navigator.of(ctx).pop();
        router.go(to);
      }

      return KSheetContent(
        footer: Row(
          children: [
            Expanded(
              child: KButton(
                label: t('accounts.fund.depositUsdt'),
                variant: KButtonVariant.surface,
                size: KButtonSize.lg,
                expand: true,
                onPressed: () => go('/wallet/deposit'),
              ),
            ),
            const SizedBox(width: 10),
            Expanded(
              child: KButton(
                label: t('accounts.fund.transferFromWallet'),
                size: KButtonSize.lg,
                expand: true,
                onPressed: () => go('/wallet/transfer?to=${a.login}'),
              ),
            ),
          ],
        ),
        children: [
          SheetDescription('${a.groupName}${a.cent ? ' · ${t('accounts.fund.centAccount')}' : ''}'),
          Container(
            padding: const EdgeInsets.all(14),
            decoration: BoxDecoration(
              color: k.surface2,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: k.line),
            ),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Container(
                  width: 36,
                  height: 36,
                  decoration: BoxDecoration(
                    color: k.emberSoft,
                    shape: BoxShape.circle,
                    border: Border.all(color: k.ember.withValues(alpha: 0.3)),
                  ),
                  child: Icon(LucideIcons.wallet, size: 16, color: k.ember),
                ),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        t('accounts.fund.fromWallet'),
                        style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
                      ),
                      const SizedBox(height: 3),
                      Text(t(a.cent ? 'accounts.fund.textCent' : 'accounts.fund.text'), style: context.text.footnote.copyWith(color: k.fg3)),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      );
    },
  );
}

/* ------------------------------------------------------------------ Rename (archive.tsx RenameDialog) */

Future<bool> showRenameSheet(BuildContext context, EngineAccount a) async =>
    await showKSheet<bool>(
      context,
      title: context.t('accounts.rename.title', {'login': a.login}),
      builder: (_) => _RenameSheet(a: a),
    ) ==
    true;

class _RenameSheet extends StatefulWidget {
  const _RenameSheet({required this.a});
  final EngineAccount a;

  @override
  State<_RenameSheet> createState() => _RenameSheetState();
}

class _RenameSheetState extends State<_RenameSheet> {
  late final TextEditingController _c = TextEditingController(text: widget.a.name);
  bool _busy = false;

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  String get _name => _c.text.trim();
  bool get _tooLong => _name.runes.length > 32;

  Future<void> _save() async {
    final t = context.t;
    final a = widget.a;
    setState(() => _busy = true);
    try {
      await apiOf(context).patch<Map<String, dynamic>>('trading/accounts/${a.login}', body: {'name': _name});
      if (!mounted) return;
      accountToast(context, NotificationKind.success, t('accounts.rename.saved'), description: '#${a.login}${_name.isNotEmpty ? ' · $_name' : ''}');
      Navigator.of(context).pop(true);
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.rename.failed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final unchanged = _name == widget.a.name.trim();
    return KSheetContent(
      footer: SheetFooter(
        primary: KButton(
          label: t('accounts.rename.save'),
          size: KButtonSize.lg,
          expand: true,
          loading: _busy,
          onPressed: _busy || _tooLong || unchanged ? null : _save,
        ),
      ),
      children: [
        KTextField(
          label: t('accounts.rename.label'),
          controller: _c,
          autofocus: true,
          hint: Text(
            '${_name.runes.length}/32',
            style: context.text.footnote.copyWith(color: _tooLong ? k.down : k.fg3, fontFeatures: kTabular),
          ),
          inputFormatters: [LengthLimitingTextInputFormatter(64)],
          textInputAction: TextInputAction.done,
          onChanged: (_) => setState(() {}),
          onSubmitted: (_) {
            if (!_busy && !_tooLong && !unchanged) unawaited(_save());
          },
        ),
        const SizedBox(height: 8),
        Text(t('accounts.rename.hint'), style: context.text.footnote.copyWith(color: k.fg3)),
      ],
    );
  }
}

/* ------------------------------------------------------------------ Delete = archive (archive.tsx DeleteAccountDialog) */

Future<bool> showDeleteAccountSheet(BuildContext context, EngineAccount a) async =>
    await showKSheet<bool>(
      context,
      title: context.t('accounts.delete.title', {'login': a.login}),
      builder: (_) => _DeleteSheet(a: a),
    ) ==
    true;

class _DeleteSheet extends StatefulWidget {
  const _DeleteSheet({required this.a});
  final EngineAccount a;

  @override
  State<_DeleteSheet> createState() => _DeleteSheetState();
}

class _DeleteSheetState extends State<_DeleteSheet> {
  ArchiveCheck? _check;
  String? _checkErr;
  bool _ack = false, _busy = false, _changed = false;
  RunResult? _result;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    try {
      final j = await apiOf(context).get<Map<String, dynamic>>('trading/accounts/${widget.a.login}/archive-check');
      if (mounted) setState(() => _check = ArchiveCheck.fromJson(j));
    } catch (e) {
      if (mounted) setState(() => _checkErr = errorText(e, context.t));
    }
  }

  bool get _demo => (_check?.kind ?? widget.a.type.name) == 'demo';

  Future<void> _run([String? token]) async {
    final t = context.t;
    final a = widget.a;
    final c = _check!;
    setState(() => _busy = true);
    try {
      final j = await apiOf(context).post<Map<String, dynamic>>(
        'trading/accounts/${a.login}/archive',
        body: {'empty': c.needsEmpty, 'ackForfeit': c.forfeit > 0 && _ack, 'stepup_token': ?token},
      );
      if (!mounted) return;
      final r = RunResult.fromJson(j);
      setState(() {
        _result = r;
        _changed = true;
      });
      if (r.ok) {
        accountToast(context, NotificationKind.success, t('accounts.delete.done'), description: t('accounts.delete.doneDesc', {'login': a.login}));
      } else {
        accountToast(context, NotificationKind.error, t('accounts.delete.partial'));
      }
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.delete.failed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _confirm() async {
    final t = context.t;
    final a = widget.a;
    if (_demo) return _run();
    // a live account may move money and close trades: the emailed code first (step-up account_archive)
    await showStepUpSheet(
      context,
      action: 'account_archive',
      target: '${a.login}',
      title: t('accounts.delete.stepUpTitle'),
      description: '#${a.login}',
      what: t('accounts.delete.stepUpWhat', {'login': a.login}),
      confirmLabel: _check!.needsEmpty ? t('accounts.delete.confirmAll') : t('accounts.delete.confirm'),
      onConfirmed: _run,
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = widget.a;
    final c = _check;
    String money(double v) => '${a.currencyPrefix}${Fmt.number(v)}';
    final blocked = c != null && c.blocked;
    final canConfirm = c != null && !blocked && !_busy && _result == null && (c.forfeit <= 0 || _ack);
    final children = <Widget>[SheetDescription(t('accounts.delete.subtitle'))];
    if (c == null && _checkErr == null) {
      children.add(_Checking(text: t('accounts.delete.checking')));
    }
    if (_checkErr != null) {
      children.add(KNotice(tone: KChipTone.down, title: t('accounts.delete.checkFailed'), text: _checkErr!.isEmpty ? t('common.errorRetry') : _checkErr!));
    }
    if (c != null && blocked) children.add(BlockersBox(title: t('accounts.delete.blocked'), blockers: c.blockers));
    if (c != null && !blocked && _result == null) {
      if (c.needsEmpty) {
        var n = 0;
        children.add(
          RowBox(
            title: t('accounts.delete.stepsTitle'),
            children: [
              if (c.trades > 0) StepLine(n: ++n, text: t('accounts.delete.stepClose', {'count': c.trades})),
              if (!_demo && c.balance > 0) StepLine(n: ++n, text: t('accounts.delete.stepMove', {'amount': money(c.balance)})),
              StepLine(n: ++n, text: t('accounts.delete.stepArchive')),
            ],
          ),
        );
      } else {
        children.add(Text(t('accounts.delete.ready'), style: context.text.callout.copyWith(color: k.fg2)));
      }
      if (_demo) children.addAll([const SizedBox(height: 10), Text(t('accounts.delete.demoNote'), style: context.text.footnote.copyWith(color: k.fg3))]);
      if (c.forfeit > 0) {
        children.addAll([
          const SizedBox(height: 12),
          _ForfeitBox(text: t('accounts.delete.forfeitText', {'amount': money(c.forfeit)}), ack: _ack, onAck: (v) => setState(() => _ack = v)),
        ]);
      }
    }
    if (_result != null) {
      children.add(ResultSteps(ok: _result!.ok, title: _result!.ok ? t('accounts.delete.done') : t('accounts.delete.partial'), steps: _result!.steps));
    }
    return KSheetContent(
      footer: _result != null
          ? KButton(
              label: t('common.close'),
              variant: KButtonVariant.surface,
              size: KButtonSize.lg,
              expand: true,
              onPressed: () => Navigator.of(context).pop(_changed),
            )
          : SheetFooter(
              onCancel: () => Navigator.of(context).pop(_changed),
              primary: blocked
                  ? const SizedBox.shrink()
                  : KButton(
                      label: c?.needsEmpty == true ? t('accounts.delete.confirmAll') : t('accounts.delete.confirm'),
                      variant: KButtonVariant.sell,
                      size: KButtonSize.lg,
                      expand: true,
                      loading: _busy,
                      onPressed: canConfirm ? _confirm : null,
                    ),
            ),
      children: children,
    );
  }
}

class _Checking extends StatelessWidget {
  const _Checking({required this.text});
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.symmetric(vertical: 10),
    child: Row(
      children: [
        SizedBox.square(dimension: 16, child: CircularProgressIndicator(strokeWidth: 2, color: context.k.fg3)),
        const SizedBox(width: 10),
        Text(text, style: context.text.callout.copyWith(color: context.k.fg3)),
      ],
    ),
  );
}

/// "Bonus and credit will be lost" with the acknowledgement box.
class _ForfeitBox extends StatelessWidget {
  const _ForfeitBox({required this.text, required this.ack, required this.onAck});
  final String text;
  final bool ack;
  final ValueChanged<bool> onAck;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    return KNotice(
      tone: KChipTone.warn,
      title: t('accounts.delete.forfeitTitle'),
      text: text,
      action: KCheckRow(
        value: ack,
        onChanged: onAck,
        child: Text(t('accounts.delete.forfeitAck'), style: TextStyle(color: context.k.fg)),
      ),
    );
  }
}

/* ------------------------------------------------------------------ Close permanently (closure.tsx) */

Future<bool> showCloseAccountSheet(BuildContext context, EngineAccount a) async =>
    await showKSheet<bool>(
      context,
      title: context.t('accounts.close.title', {'login': a.login}),
      builder: (_) => _CloseSheet(a: a),
    ) ==
    true;

class _CloseSheet extends StatefulWidget {
  const _CloseSheet({required this.a});
  final EngineAccount a;

  @override
  State<_CloseSheet> createState() => _CloseSheetState();
}

class _CloseSheetState extends State<_CloseSheet> {
  ClosureStatus? _st;
  String? _loadErr;
  String _reason = '';
  final List<String> _more = [];
  final _comment = TextEditingController();
  bool _ack = false, _understood = false, _busy = false, _changed = false;
  RunResult? _result;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  @override
  void dispose() {
    _comment.dispose();
    super.dispose();
  }

  Future<void> _load() async {
    try {
      final j = await apiOf(context).get<Map<String, dynamic>>('trading/accounts/${widget.a.login}/closure');
      if (mounted) setState(() => _st = ClosureStatus.fromJson(j));
    } catch (e) {
      if (mounted) setState(() => _loadErr = errorText(e, context.t));
    }
  }

  Future<void> _run(String token) async {
    final t = context.t;
    final a = widget.a;
    final st = _st!;
    setState(() => _busy = true);
    try {
      final j = await apiOf(context).post<Map<String, dynamic>>(
        'trading/accounts/${a.login}/closure',
        body: {
          'reasonCode': _reason,
          'survey': {
            'reasons': [_reason, ..._more.where((x) => x != _reason)],
            'comment': _comment.text.trim(),
          },
          'empty': st.needsEmpty,
          'ackForfeit': st.forfeit > 0 && _ack,
          'stepup_token': token,
        },
      );
      if (!mounted) return;
      final r = RunResult.fromJson(j);
      setState(() {
        _result = r;
        _changed = true;
      });
      if (r.ok) {
        accountToast(context, NotificationKind.success, t('accounts.close.sent'), description: t('accounts.close.sentDesc', {'login': a.login}));
      } else {
        accountToast(context, NotificationKind.error, t('accounts.delete.partial'));
      }
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.close.failed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _confirm() async {
    final t = context.t;
    final a = widget.a;
    await showStepUpSheet(
      context,
      action: 'account_close',
      target: '${a.login}',
      title: t('accounts.close.stepUpTitle'),
      description: '#${a.login}',
      what: t('accounts.close.stepUpWhat', {'login': a.login}),
      confirmLabel: t('accounts.close.confirm'),
      onConfirmed: _run,
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = widget.a;
    final st = _st;
    String money(double v) => '${a.currencyPrefix}${Fmt.number(v)}';
    final blocked = st != null && st.blocked;
    final pending = st != null && st.pending;
    final canSend =
        st != null && !blocked && !pending && st.canRequest && _reason.isNotEmpty && _understood && (st.forfeit <= 0 || _ack) && !_busy && _result == null;
    final children = <Widget>[SheetDescription(t('accounts.close.subtitle'))];
    if (st == null && _loadErr == null) children.add(_Checking(text: t('accounts.delete.checking')));
    if (_loadErr != null) {
      children.add(KNotice(tone: KChipTone.down, title: t('accounts.delete.checkFailed'), text: _loadErr!.isEmpty ? t('common.errorRetry') : _loadErr!));
    }
    if (st != null && blocked) children.add(BlockersBox(title: t('accounts.close.blocked'), blockers: st.blockers));
    if (st != null && pending && _result == null) {
      children.add(ClosureNotice(request: st.request!, login: a.login, onChanged: () => _changed = true));
    }
    if (st != null && !blocked && !pending && _result == null) {
      var n = 0;
      children.addAll([
        RowBox(
          children: [
            Row(
              children: [
                Icon(LucideIcons.lock, size: 16, color: k.fg3),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    t('accounts.close.finalTitle'),
                    style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
                  ),
                ),
              ],
            ),
            const SizedBox(height: 6),
            for (final key in ['accounts.close.final1', 'accounts.close.final2', 'accounts.close.final3'])
              Padding(
                padding: const EdgeInsets.only(top: 3),
                child: Text('•  ${t(key)}', style: context.text.footnote.copyWith(color: k.fg2)),
              ),
          ],
        ),
        const SizedBox(height: 16),
        Text(
          t('accounts.close.whyTitle').toUpperCase(),
          style: context.text.caption.copyWith(color: k.fg3, letterSpacing: 0.5, fontWeight: FontWeight.w600),
        ),
        const SizedBox(height: 8),
        for (final r in kClosureReasons)
          Padding(
            padding: const EdgeInsets.only(bottom: 6),
            child: ChoiceBox(
              selected: _reason == r,
              onTap: () => setState(() => _reason = r),
              child: Text(t('accounts.close.reason.$r'), style: context.text.callout.copyWith(color: _reason == r ? k.fg : k.fg2)),
            ),
          ),
        if (_reason.isNotEmpty) ...[
          const SizedBox(height: 6),
          Text(t('accounts.close.alsoTitle'), style: context.text.footnote.copyWith(color: k.fg3)),
          const SizedBox(height: 6),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final r in kClosureReasons.where((r) => r != _reason && r != 'other'))
                KPressable(
                  minSize: 32,
                  onTap: () => setState(() => _more.contains(r) ? _more.remove(r) : _more.add(r)),
                  child: KChip(label: t('accounts.close.reason.$r'), tone: _more.contains(r) ? KChipTone.ember : KChipTone.neutral, small: true),
                ),
            ],
          ),
        ],
        const SizedBox(height: 12),
        KTextArea(controller: _comment, label: t('accounts.close.commentLabel'), placeholder: t('accounts.close.commentPlaceholder')),
        if (st.needsEmpty) ...[
          const SizedBox(height: 12),
          RowBox(
            title: t('accounts.delete.stepsTitle'),
            children: [
              if (st.trades > 0) StepLine(n: ++n, text: t('accounts.delete.stepClose', {'count': st.trades})),
              if (st.balance > 0) StepLine(n: ++n, text: t('accounts.delete.stepMove', {'amount': money(st.balance)})),
              StepLine(n: ++n, text: t('accounts.close.stepRequest')),
            ],
          ),
        ],
        if (st.forfeit > 0) ...[
          const SizedBox(height: 12),
          _ForfeitBox(text: t('accounts.close.forfeitText', {'amount': money(st.forfeit)}), ack: _ack, onAck: (v) => setState(() => _ack = v)),
        ],
        const SizedBox(height: 8),
        KCheckRow(
          value: _understood,
          onChanged: (v) => setState(() => _understood = v),
          child: Text(t('accounts.close.understand'), style: TextStyle(color: k.fg)),
        ),
      ]);
    }
    if (_result != null) {
      children.add(
        ResultSteps(
          ok: _result!.ok,
          title: _result!.ok ? t('accounts.close.sent') : t('accounts.delete.partial'),
          text: _result!.ok ? t('accounts.close.sentDesc', {'login': a.login}) : null,
          steps: _result!.steps,
          showState: false,
        ),
      );
    }
    final onlyClose = _result != null || pending || blocked;
    return KSheetContent(
      footer: onlyClose
          ? KButton(
              label: t('common.close'),
              variant: KButtonVariant.surface,
              size: KButtonSize.lg,
              expand: true,
              onPressed: () => Navigator.of(context).pop(_changed),
            )
          : SheetFooter(
              onCancel: () => Navigator.of(context).pop(_changed),
              primary: KButton(
                label: st?.needsEmpty == true ? t('accounts.close.confirmAll') : t('accounts.close.confirm'),
                variant: KButtonVariant.sell,
                size: KButtonSize.lg,
                expand: true,
                loading: _busy,
                onPressed: canSend ? _confirm : null,
              ),
            ),
      children: children,
    );
  }
}

/// The latest closure request: pending (with Cancel request), rejected (the broker's reason) or closed
/// (web ClosureNotice).
class ClosureNotice extends StatefulWidget {
  const ClosureNotice({super.key, required this.request, required this.login, this.onChanged});
  final ClosureRequest request;
  final int login;
  final VoidCallback? onChanged;

  @override
  State<ClosureNotice> createState() => _ClosureNoticeState();
}

class _ClosureNoticeState extends State<ClosureNotice> {
  bool _busy = false, _gone = false;

  Future<void> _cancel() async {
    final t = context.t;
    setState(() => _busy = true);
    try {
      await apiOf(context).post<Map<String, dynamic>>('trading/accounts/${widget.login}/closure/cancel');
      if (!mounted) return;
      accountToast(context, NotificationKind.success, t('accounts.close.cancelled'));
      setState(() => _gone = true);
      widget.onChanged?.call();
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.close.cancelFailed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final r = widget.request;
    if (_gone || r.status == 'cancelled') return const SizedBox.shrink();
    final t = context.t;
    final k = context.k;
    final (bg, border) = switch (r.status) {
      'pending' => (k.infoSoft, k.info.withValues(alpha: 0.25)),
      'rejected' => (k.warnSoft, k.warn.withValues(alpha: 0.25)),
      _ => (k.surface2, k.line),
    };
    final title = switch (r.status) {
      'pending' => t('accounts.close.pendingTitle'),
      'rejected' => t('accounts.close.rejectedTitle'),
      _ => t('accounts.close.approvedTitle'),
    };
    return Container(
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      decoration: BoxDecoration(
        color: bg,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: border),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(LucideIcons.clock, size: 16, color: k.fg3),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  title,
                  style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
                ),
              ),
              Text(fmtDate(t, r.decidedAt ?? r.createdAt), style: context.text.footnote.copyWith(color: k.fg3)),
            ],
          ),
          const SizedBox(height: 4),
          Text(r.status == 'pending' ? t('accounts.close.pendingText') : (r.message ?? ''), style: context.text.footnote.copyWith(color: k.fg2)),
          if (r.status == 'pending' && r.source == 'client') ...[
            const SizedBox(height: 10),
            KButton(
              label: t('accounts.close.cancelRequest'),
              variant: KButtonVariant.surface,
              size: KButtonSize.sm,
              loading: _busy,
              onPressed: _busy ? null : _cancel,
            ),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ Change account type (extras.tsx ChangeTypeDialog) */

Future<bool> showChangeTypeSheet(BuildContext context, EngineAccount a) async =>
    await showKSheet<bool>(
      context,
      title: context.t('accounts.type.title', {'login': a.login}),
      builder: (_) => _ChangeTypeSheet(a: a),
    ) ==
    true;

class _ChangeTypeSheet extends StatefulWidget {
  const _ChangeTypeSheet({required this.a});
  final EngineAccount a;

  @override
  State<_ChangeTypeSheet> createState() => _ChangeTypeSheetState();
}

class _ChangeTypeSheetState extends State<_ChangeTypeSheet> {
  List<GroupOption>? _opts;
  String? _err;
  String _pick = '';
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    unawaited(_load());
  }

  Future<void> _load() async {
    try {
      final j = await apiOf(context).get<Map<String, dynamic>>('trading/accounts/${widget.a.login}/group-options');
      if (!mounted) return;
      setState(() => _opts = [for (final g in (j['groups'] as List? ?? const [])) GroupOption.fromJson((g as Map).cast<String, dynamic>())]);
    } catch (e) {
      if (mounted) setState(() => _err = errorText(e, context.t));
    }
  }

  Future<void> _save(GroupOption chosen) async {
    final t = context.t;
    final a = widget.a;
    setState(() => _busy = true);
    try {
      await apiOf(context).post<Map<String, dynamic>>('trading/accounts/${a.login}/group', body: {'group': _pick});
      if (!mounted) return;
      accountToast(
        context,
        NotificationKind.success,
        t('accounts.type.changed'),
        description: t('accounts.type.changedDesc', {'login': a.login, 'name': chosen.name}),
      );
      Navigator.of(context).pop(true);
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.type.failed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final opts = _opts;
    final chosen = opts?.where((g) => g.code == _pick).firstOrNull;
    return KSheetContent(
      footer: SheetFooter(
        primary: KButton(
          label: t('accounts.type.confirm'),
          size: KButtonSize.lg,
          expand: true,
          loading: _busy,
          onPressed: chosen == null || !chosen.allowed || _busy ? null : () => _save(chosen),
        ),
      ),
      children: [
        SheetDescription(t('accounts.type.subtitle')),
        if (opts == null && _err == null) const KSkeleton(height: 120, radius: 14),
        if (_err != null) Text(_err!.isEmpty ? t('common.errorRetry') : _err!, style: context.text.callout.copyWith(color: k.down)),
        if (opts != null && opts.isEmpty) Text(t('accounts.type.none'), style: context.text.callout.copyWith(color: k.fg3)),
        for (final g in opts ?? const <GroupOption>[])
          Padding(
            padding: const EdgeInsets.only(bottom: 8),
            child: ChoiceBox(
              selected: _pick == g.code,
              enabled: g.allowed,
              onTap: () => setState(() => _pick = g.code),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text.rich(
                          TextSpan(
                            text: g.name,
                            children: [
                              TextSpan(
                                text: ' · ${t.dyn('accounts.mode.${g.mode}', fallback: g.mode)}',
                                style: TextStyle(color: k.fg3),
                              ),
                            ],
                          ),
                          style: context.text.label.copyWith(fontSize: 14, color: k.fg, fontWeight: FontWeight.w600),
                        ),
                        const SizedBox(height: 2),
                        Text(
                          t('accounts.type.facts', {
                            'min': Fmt.number(g.minDeposit, 0),
                            'commission': Fmt.number(g.commissionPerLot, g.commissionPerLot % 1 == 0 ? 0 : 2),
                          }),
                          style: context.text.footnote.copyWith(color: k.fg3),
                        ),
                        if (g.blocker != null) ...[
                          const SizedBox(height: 3),
                          Text(
                            t.dyn('accounts.type.blocker.${g.blocker!.code}', fallback: g.blocker!.message),
                            style: context.text.footnote.copyWith(color: k.warn),
                          ),
                        ],
                      ],
                    ),
                  ),
                  if (_pick == g.code) Icon(LucideIcons.check, size: 16, color: k.ember),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ Demo balance (extras.tsx DemoBalanceDialog) */

const List<int> kDemoPresets = [1000, 5000, 10000, 50000, 100000];

Future<bool> showDemoBalanceSheet(BuildContext context, EngineAccount a) async =>
    await showKSheet<bool>(
      context,
      title: context.t('accounts.demoBalance.title', {'login': a.login}),
      builder: (_) => _DemoBalanceSheet(a: a),
    ) ==
    true;

class _DemoBalanceSheet extends StatefulWidget {
  const _DemoBalanceSheet({required this.a});
  final EngineAccount a;

  @override
  State<_DemoBalanceSheet> createState() => _DemoBalanceSheetState();
}

class _DemoBalanceSheetState extends State<_DemoBalanceSheet> {
  final _c = TextEditingController(text: '10000');
  bool _busy = false;

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    final t = context.t;
    final a = widget.a;
    final n = double.parse(_c.text);
    setState(() => _busy = true);
    try {
      await apiOf(context).post<Map<String, dynamic>>('trading/accounts/${a.login}/demo-balance', body: {'amount': n % 1 == 0 ? n.toInt() : n});
      if (!mounted) return;
      accountToast(context, NotificationKind.success, t('accounts.demoBalance.done'), description: '#${a.login} · \$${Fmt.number(n, n % 1 == 0 ? 0 : 2)}');
      Navigator.of(context).pop(true);
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.demoBalance.failed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = widget.a;
    final n = double.tryParse(_c.text);
    final flat = a.positions == 0 && a.orders == 0;
    return KSheetContent(
      footer: SheetFooter(
        primary: KButton(
          label: t('accounts.demoBalance.confirm'),
          size: KButtonSize.lg,
          expand: true,
          loading: _busy,
          onPressed: !demoBalanceOk(_c.text) || !flat || _busy ? null : _save,
        ),
      ),
      children: [
        SheetDescription(t('accounts.demoBalance.subtitle')),
        Wrap(
          spacing: 6,
          runSpacing: 6,
          children: [
            for (final p in kDemoPresets)
              KPressable(
                minSize: 32,
                onTap: () => setState(() => _c.text = '$p'),
                child: KChip(label: '\$${Fmt.number(p, 0)}', tone: n == p ? KChipTone.ember : KChipTone.neutral, small: true),
              ),
          ],
        ),
        const SizedBox(height: 14),
        KTextField(
          label: t('accounts.demoBalance.label'),
          hint: Text(t('accounts.demoBalance.range'), style: context.text.footnote.copyWith(color: k.fg3)),
          controller: _c,
          leadingText: r'$',
          ltr: true,
          keyboardType: const TextInputType.numberWithOptions(decimal: true),
          inputFormatters: [FilteringTextInputFormatter.allow(RegExp(r'[\d.]'))],
          onChanged: (_) => setState(() {}),
        ),
        if (!flat) ...[const SizedBox(height: 10), Text(t('accounts.demoBalance.closeFirst'), style: context.text.footnote.copyWith(color: k.warn))],
        const SizedBox(height: 10),
        Text(t('accounts.demoBalance.note'), style: context.text.footnote.copyWith(color: k.fg3)),
      ],
    );
  }
}

/* ------------------------------------------------------------------ Move money between own live accounts (extras.tsx) */

class TransferBetweenSheet extends ConsumerStatefulWidget {
  const TransferBetweenSheet({super.key, this.from});
  final EngineAccount? from;

  @override
  ConsumerState<TransferBetweenSheet> createState() => _TransferBetweenSheetState();
}

class _TransferBetweenSheetState extends ConsumerState<TransferBetweenSheet> {
  late int? _src = widget.from?.login;
  int? _dst;
  final _amount = TextEditingController();
  bool _busy = false;

  /// The request id of this transfer: kept across a failed try (the wallet de-duplicates), new after an answer.
  String _key = newIdempotencyKey();

  @override
  void dispose() {
    _amount.dispose();
    super.dispose();
  }

  Future<void> _run(String token) async {
    final t = context.t;
    final n = double.parse(_amount.text);
    setState(() => _busy = true);
    try {
      final r = await ref
          .read(apiProvider)
          .post<Map<String, dynamic>>(
            'trading/transfers/between',
            body: {'fromLogin': _src, 'toLogin': _dst, 'amount': _amount.text, 'idempotency_key': _key, 'stepup_token': token},
          );
      if (!mounted) return;
      final status = r['status'];
      if (status == 'completed') {
        accountToast(
          context,
          NotificationKind.success,
          t('accounts.between.done'),
          description: t('accounts.between.doneDesc', {'amount': '\$${n.toStringAsFixed(2)}', 'from': _src, 'to': _dst}),
        );
      } else if (status == 'in_wallet') {
        final err = r['error'] is Map ? (r['error'] as Map)['message'] as String? : null;
        accountToast(context, NotificationKind.warning, t('accounts.between.inWallet'), description: err);
      } else {
        accountToast(context, NotificationKind.neutral, t('accounts.between.processing'));
      }
      _key = newIdempotencyKey();
      Navigator.of(context).pop(true);
    } catch (e) {
      if (mounted) accountErrorToast(context, t('accounts.between.failed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _confirm() async {
    final t = context.t;
    final src = _src!, dst = _dst!;
    await showStepUpSheet(
      context,
      action: 'internal_transfer',
      target: '$src',
      title: t('accounts.between.stepUpTitle'),
      description: '#$src → #$dst',
      what: t('accounts.between.stepUpWhat', {'from': src, 'to': dst}),
      confirmLabel: t('accounts.between.confirm'),
      onConfirmed: _run,
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final all = ref.watch(accountsOnceProvider).value ?? const <EngineAccount>[];
    final live = all.where((x) => x.live && !x.prop && !x.archived).toList();
    final s = live.where((x) => x.login == _src).firstOrNull;
    final avail = s == null ? 0.0 : s.usd(s.withdrawable);
    final ok = s != null && transferBetweenOk(from: _src, to: _dst, amount: _amount.text, availableUsd: avail);
    String srcLabel(EngineAccount x) => '#${x.login} · ${x.groupName} · ${x.currencyPrefix}${Fmt.number(x.withdrawable)}';
    String dstLabel(EngineAccount x) => '#${x.login} · ${x.groupName}';
    final dst = live.where((x) => x.login == _dst).firstOrNull;
    return KSheetContent(
      footer: SheetFooter(
        primary: KButton(label: t('accounts.between.confirm'), size: KButtonSize.lg, expand: true, loading: _busy, onPressed: ok && !_busy ? _confirm : null),
      ),
      children: [
        SheetDescription(t('accounts.between.subtitle')),
        if (live.length < 2)
          Text(t('accounts.between.needTwo'), style: context.text.callout.copyWith(color: k.fg3))
        else ...[
          KPickerField(
            label: t('accounts.between.from'),
            value: s == null ? null : srcLabel(s),
            placeholder: t('accounts.between.choose'),
            onTap: () async {
              final v = await showKPicker<int>(
                context,
                title: t('accounts.between.from'),
                selected: _src,
                options: [for (final x in live) KPickOption(x.login, srcLabel(x))],
              );
              if (v != null && mounted) setState(() => _src = v);
            },
          ),
          Padding(
            padding: const EdgeInsets.symmetric(vertical: 6),
            child: Icon(LucideIcons.arrowDown, size: 16, color: k.fg3),
          ),
          KPickerField(
            label: t('accounts.between.to'),
            value: dst == null || dst.login == _src ? null : dstLabel(dst),
            placeholder: t('accounts.between.choose'),
            onTap: () async {
              final v = await showKPicker<int>(
                context,
                title: t('accounts.between.to'),
                selected: _dst,
                options: [for (final x in live.where((x) => x.login != _src)) KPickOption(x.login, dstLabel(x))],
              );
              if (v != null && mounted) setState(() => _dst = v);
            },
          ),
          const SizedBox(height: 12),
          KTextField(
            label: t('accounts.between.amount'),
            hint: s == null
                ? null
                : Text(t('accounts.between.available', {'amount': '\$${avail.toStringAsFixed(2)}'}), style: context.text.footnote.copyWith(color: k.fg3)),
            controller: _amount,
            leadingText: r'$',
            ltr: true,
            placeholder: '0.00',
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            inputFormatters: [FilteringTextInputFormatter.allow(RegExp(r'[\d.]'))],
            onChanged: (_) => setState(() {}),
            trailing: s == null
                ? null
                : KTextButton(
                    label: t('accounts.between.max'),
                    onPressed: () => setState(() => _amount.text = ((avail * 100).floor() / 100).toStringAsFixed(2)),
                  ),
          ),
          const SizedBox(height: 10),
          Text(t('accounts.between.note'), style: context.text.footnote.copyWith(color: k.fg3)),
        ],
      ],
    );
  }
}

/* ------------------------------------------------------------------ Set password (manage.tsx ChangePasswordDialog) */

/// Sets the trading or investor password (replacing any earlier one) after the emailed code. `kind`: trading |
/// investor. No password is needed to trade on Kalks: these are for other trading apps and for sharing view-only
/// access. Also opened from the open-account wizard's Done step.
Future<void> showChangePasswordSheet(BuildContext context, EngineAccount a, String kind) => showKSheet<void>(
  context,
  title: context.t(kind == 'trading' ? 'accountDetail.creds.setTrading' : 'accountDetail.creds.setInvestor'),
  builder: (_) => _ChangePasswordSheet(a: a, kind: kind),
);

class _ChangePasswordSheet extends StatefulWidget {
  const _ChangePasswordSheet({required this.a, required this.kind});
  final EngineAccount a;
  final String kind;

  @override
  State<_ChangePasswordSheet> createState() => _ChangePasswordSheetState();
}

class _ChangePasswordSheetState extends State<_ChangePasswordSheet> {
  String _pw = '', _confirm = '';
  String? _formErr, _done;
  bool _busy = false;

  bool get _trading => widget.kind == 'trading';

  /// After the emailed code: the change itself (the confirmation is spent either way).
  Future<void> _set(String token) async {
    final t = context.t;
    final a = widget.a;
    final pw = _pw;
    setState(() => _busy = true);
    try {
      final r = await apiOf(context)
          .post<Map<String, dynamic>>('trading/accounts/${a.login}/passwords', body: {'kind': widget.kind, 'password': pw, 'stepup_token': token});
      if (!mounted) return;
      final revoked = (r['sessionsRevoked'] as num?)?.toInt() ?? 0;
      setState(() {
        _done = pw;
        _pw = '';
        _confirm = '';
      });
      accountToast(
        context,
        NotificationKind.success,
        t(_trading ? 'accountDetail.pw.saved.trading' : 'accountDetail.pw.saved.investor'),
        description: '#${a.login}${revoked > 0 ? ' · ${t('accountDetail.pw.sessionsSignedOut', {'count': revoked})}' : ''}',
      );
    } on ApiException catch (e) {
      if (!mounted) return;
      if (e.status == 422 || e.isStepUp) {
        setState(() => _formErr = localizeError(e, t));
      } else {
        accountErrorToast(context, t(_trading ? 'accountDetail.pw.changeError.trading' : 'accountDetail.pw.changeError.investor'), e);
      }
    } catch (e) {
      if (mounted) accountErrorToast(context, t(_trading ? 'accountDetail.pw.changeError.trading' : 'accountDetail.pw.changeError.investor'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _continue() async {
    final t = context.t;
    final a = widget.a;
    setState(() => _formErr = null);
    await showStepUpSheet(
      context,
      action: _trading ? 'trading_password' : 'investor_password',
      target: '${a.login}',
      title: t(_trading ? 'accountDetail.creds.setTrading' : 'accountDetail.creds.setInvestor'),
      description: t('accountDetail.pw.confirmWithCode'),
      what: t(_trading ? 'accountDetail.pw.stepUpWhat.trading' : 'accountDetail.pw.stepUpWhat.investor', {'login': a.login}),
      confirmLabel: t('accountDetail.pw.confirmSet'),
      onConfirmed: _set,
    );
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = widget.a;
    if (_done != null) {
      return KSheetContent(
        footer: KButton(label: t('common.done'), size: KButtonSize.lg, expand: true, onPressed: () => Navigator.of(context).pop()),
        children: [
          SecretField(label: t(_trading ? 'accountDetail.pw.newLabel.trading' : 'accountDetail.pw.newLabel.investor'), value: _done!, secret: true),
          const SizedBox(height: 14),
          KNotice(tone: KChipTone.warn, text: t('accountDetail.pw.shownOnce')),
        ],
      );
    }
    final ok = livePasswordOk(_pw) && _pw == _confirm;
    return KSheetContent(
      footer: SheetFooter(
        primary: KButton(label: t('common.continue'), size: KButtonSize.lg, expand: true, loading: _busy, onPressed: ok && !_busy ? _continue : null),
      ),
      children: [
        SheetDescription(t(_trading ? 'accountDetail.pw.descTradingSet' : 'accountDetail.pw.descInvestorSet', {'login': a.login})),
        if (_formErr != null) ...[KFormError(_formErr), const SizedBox(height: 12)],
        PasswordInput(label: t('accountDetail.pw.new'), value: _pw, generate: true, onChanged: (v) => setState(() => _pw = v)),
        const SizedBox(height: 10),
        PasswordRules(password: _pw),
        const SizedBox(height: 12),
        PasswordInput(
          label: t('accountDetail.pw.confirmNew'),
          value: _confirm,
          placeholder: t('accountDetail.pw.repeat'),
          error: _confirm.isNotEmpty && _confirm != _pw ? t('accountDetail.pw.mismatch') : null,
          onChanged: (v) => setState(() => _confirm = v),
        ),
        const SizedBox(height: 10),
        Text(t('accountDetail.pw.differentHint'), style: context.text.footnote.copyWith(color: k.fg3)),
      ],
    );
  }
}

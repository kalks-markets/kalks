// Copy & PAMM › MAM manager (/social/mam): an approved master opens a MAM programme (a dedicated MAM master
// account), sees the linked accounts, sets per-account multipliers / percents, previews how a block is allocated and
// reviews every allocation and fee. Port of the phone layout of apps/crm/components/social-live/mam-manager.tsx
// (LiveMamManagerPage: GET mam/manager every 10 s, POST / PATCH mam/manager, PATCH mam/manager/links/{id},
// GET mam/manager/preview).
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/config/app_config.dart';
import '../../i18n/i18n.dart';
import '../../shell/nav.dart';
import '../../ui/ui.dart';
import 'mam_api.dart';
import 'managed_screen.dart' show termsParagraphs;
import 'social_api.dart';
import 'widgets/bits.dart';
import 'widgets/copy_sheets.dart';
import 'widgets/follow_sheet.dart' show SheetTitle;
import 'widgets/master_dashboard.dart' show SecretRow;

const List<String> _methods = ['equity', 'balance', 'multiplier', 'percent'];
const List<String> _periods = ['daily', 'weekly', 'monthly'];
const Map<String, IconData> _methodIcon = {
  'equity': LucideIcons.scale,
  'balance': LucideIcons.wallet,
  'multiplier': LucideIcons.layers,
  'percent': LucideIcons.percent,
};

bool _share(String method) => method == 'equity' || method == 'balance';

class MamManagerScreen extends ConsumerStatefulWidget {
  const MamManagerScreen({super.key});

  @override
  ConsumerState<MamManagerScreen> createState() => _MamManagerScreenState();
}

class _MamManagerScreenState extends ConsumerState<MamManagerScreen> {
  void _reload() => ref.invalidate(mamManagerMeProvider);

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final readOnly = ref.watch(meProvider)?.readOnly ?? false;
    final q = ref.watch(mamManagerMeProvider);
    final data = q.value;
    final approved = data?.master?['status'] == 'approved';
    // the master application lives under copy trading (/social/master)
    final masterOn = pageOn(ref.watch(configProvider), '/social/master');
    return KPageScroll(
      onRefresh: () async {
        _reload();
        await ref.read(mamManagerMeProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        KPageHeader(title: t('social.mm.page.title'), subtitle: Text(t('social.mm.page.subtitle'))),
        const SizedBox(height: 18),
        if (q.hasError && data == null)
          SocialErrorCard(error: q.error, title: t('social.mm.page.unavailable'), onRetry: _reload)
        else if (data == null)
          const BlockSkeleton(h: 140)
        else if (data.manager != null)
          _Dashboard(me: data, readOnly: readOnly, reload: _reload)
        else if (!approved)
          KCard(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(t('social.mm.page.approvedOnly'), style: context.text.headline),
                const SizedBox(height: 6),
                Text(
                  '${t('social.mm.page.approvedOnlyText')} '
                  '${data.master != null ? t('social.mm.page.profileStatus', {'status': t.dyn('social.masterStatus.${strOf(data.master!['status'])}', fallback: strOf(data.master!['status'])).toLowerCase()}) : t('social.mm.page.applyFirst')}',
                  style: context.text.footnote.copyWith(color: context.k.fg3, fontSize: 13, height: 1.5),
                ),
                if (masterOn) ...[
                  const SizedBox(height: 14),
                  KButton(
                    label: data.master != null ? t('social.md.title') : t('social.becomeMaster'),
                    icon: LucideIcons.crown,
                    onPressed: () => context.go('/social/master'),
                  ),
                ],
              ],
            ),
          )
        else
          SectionCard(
            title: t('social.mm.page.openTitle'),
            subtitle: t('social.mm.page.openSub', {'name': strOf(data.master?['nickname'])}),
            icon: LucideIcons.plus,
            child: ProgrammeForm(
              me: data,
              readOnly: readOnly,
              onDone: (creds) {
                _reload();
                if (creds != null) showMamCredentialsSheet(context, creds);
              },
            ),
          ),
      ],
    );
  }
}

/* ------------------------------------------------------------------ programme form (create / edit) */

/// The programme's name, description, allocation method and fees: opens a programme (POST mam/manager, `m` null) or
/// edits it (PATCH mam/manager, inside a sheet).
class ProgrammeForm extends ConsumerStatefulWidget {
  const ProgrammeForm({super.key, required this.me, this.m, required this.onDone, this.inSheet = false, this.readOnly = false});
  final ManagerMe me;
  final ManagerView? m;

  /// After a save; the new master account's credentials when a programme was opened.
  final void Function(Map<String, dynamic>? credentials) onDone;
  final bool inSheet;
  final bool readOnly;

  @override
  ConsumerState<ProgrammeForm> createState() => _ProgrammeFormState();
}

class _ProgrammeFormState extends ConsumerState<ProgrammeForm> {
  ManagerView? get m => widget.m;
  late final _name = TextEditingController(text: m?.name ?? '');
  late final _desc = TextEditingController(text: m?.description ?? '');
  late String _method = m?.method ?? 'equity';
  late final _perf = TextEditingController(text: inputText(m?.perfFeePct ?? 20));
  late final _mgmt = TextEditingController(text: inputText(m?.mgmtFeePct ?? 0));
  late String _period = m?.feePeriod ?? 'monthly';
  late final _minEquity = TextEditingController(text: inputText(m?.minEquity ?? 100));
  final _seed = TextEditingController();
  bool _busy = false;

  bool get _locked => m != null && m!.accounts > 0;

  @override
  void dispose() {
    for (final c in [_name, _desc, _perf, _mgmt, _minEquity, _seed]) {
      c.dispose();
    }
    super.dispose();
  }

  String? _err(T t) {
    final me = widget.me;
    final perf = parseAmount(_perf.text);
    final mgmt = parseAmount(_mgmt.text);
    final seed = parseAmount(_seed.text);
    final name = _name.text.trim();
    if (name.length < 3 || name.length > 60) return t('social.mm.err.name');
    if (perf == null || perf < me.feeMinPct || perf > me.feeMaxPct) {
      return t('social.mm.err.perf', {'min': numText(me.feeMinPct), 'max': numText(me.feeMaxPct)});
    }
    if (mgmt != null && (mgmt < 0 || mgmt > me.mgmtMaxPct)) return t('social.mm.err.mgmt', {'max': numText(me.mgmtMaxPct)});
    if (_seed.text.trim().isNotEmpty && !(seed != null && seed > 0)) return t('social.mm.err.seed');
    return null;
  }

  Future<void> _submit() async {
    final t = context.t;
    final err = _err(t);
    if (err != null) return plainError(ref, err);
    setState(() => _busy = true);
    final body = <String, Object?>{
      'name': _name.text.trim(),
      'description': _desc.text.trim(),
      'method': _method,
      'perfFeePct': parseAmount(_perf.text),
      'mgmtFeePct': parseAmount(_mgmt.text) ?? 0,
      'feePeriod': _period,
      'minEquity': parseAmount(_minEquity.text) ?? 0,
    };
    try {
      if (m != null) {
        if (_locked) body.remove('method');
        await socialPatch(ref, 'mam/manager', body);
        okToast(ref, t('social.mm.toast.updated'), t('social.mm.toast.updatedDesc'));
        widget.onDone(null);
      } else {
        final seed = parseAmount(_seed.text);
        final r = await socialPost(ref, 'mam/manager', {...body, 'seed': ?seed});
        okToast(ref, t('social.mm.toast.opened'));
        widget.onDone(r['credentials'] is Map ? mapOf(r['credentials']) : null);
      }
    } on ApiException catch (e) {
      if (mounted) errToast(ref, context, m != null ? t('social.mm.toast.updateFailed') : t('social.mm.toast.openFailed'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final me = widget.me;
    final err = _err(t);
    final submit = KButton(
      label: m != null ? t('social.mm.save') : t('social.mm.open'),
      icon: m != null ? null : LucideIcons.briefcase,
      size: KButtonSize.lg,
      expand: true,
      loading: _busy,
      onPressed: err != null || widget.readOnly ? null : _submit,
    );
    final fields = <Widget>[
      KTextField(
        controller: _name,
        label: t('social.mm.name'),
        placeholder: t('social.mm.namePh'),
        inputFormatters: [LengthLimitingTextInputFormatter(60)],
        onChanged: (_) => setState(() {}),
      ),
      const SizedBox(height: 12),
      NumberField(controller: _minEquity, label: t('social.mm.minEquity'), hint: t('social.mm.minEquityHint'), dollar: true, onChanged: (_) => setState(() {})),
      const SizedBox(height: 12),
      TextArea(
        controller: _desc,
        label: t('social.md.description'),
        counter: t('social.mm.shownToClients'),
        placeholder: t('social.mm.descriptionPh'),
        maxLength: 1000,
        lines: 3,
        onChanged: (_) => setState(() {}),
      ),
      const SizedBox(height: 16),
      GroupLabel(
        t('social.mm.method'),
        trailing: _locked ? Text(t('social.mm.methodLocked'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)) : null,
      ),
      for (final x in _methods) ...[
        RadioCard(
          selected: _method == x,
          icon: _methodIcon[x],
          title: methodLabel(t, x),
          text: methodHint(t, x),
          dim: _locked && _method != x,
          onSelect: _locked ? null : () => setState(() => _method = x),
        ),
        const SizedBox(height: 8),
      ],
      const SizedBox(height: 8),
      NumberField(
        controller: _perf,
        label: t('social.performanceFee'),
        hint: '${numText(me.feeMinPct)}–${numText(me.feeMaxPct)}%',
        unit: '%',
        onChanged: (_) => setState(() {}),
      ),
      const SizedBox(height: 12),
      NumberField(
        controller: _mgmt,
        label: t('social.mm.mgmtFee'),
        hint: t('social.mm.mgmtHint', {'max': numText(me.mgmtMaxPct)}),
        unit: '%/y',
        onChanged: (_) => setState(() {}),
      ),
      const SizedBox(height: 12),
      GroupLabel(t('social.mm.feePeriod')),
      KSegmented<String>(
        plain: true,
        values: _periods,
        labels: [for (final p in _periods) periodLabel(t, p)],
        selected: _period,
        onChanged: (v) => setState(() => _period = v),
      ),
      if (m == null) ...[
        const SizedBox(height: 12),
        NumberField(
          controller: _seed,
          label: t('social.mm.fund'),
          hint: t('common.optional'),
          placeholder: '0.00',
          dollar: true,
          onChanged: (_) => setState(() {}),
        ),
      ],
      const SizedBox(height: 14),
      InfoBox(text: t('social.mm.feeNote', {'period': periodLabel(t, _period).toLowerCase(), 'cut': numText(me.platformCutPct)})),
    ];
    if (widget.inSheet) {
      return KSheetContent(
        footer: SheetFooter(label: t('social.mm.save'), busy: _busy, onPressed: err != null || widget.readOnly ? null : _submit),
        children: [
          SheetTitle(title: t('social.mm.edit.title'), description: t('social.mm.edit.description')),
          ...fields,
        ],
      );
    }
    return Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: [...fields, const SizedBox(height: 16), submit]);
  }
}

/// The new MAM master account's credentials, shown once (web CredentialsDialog).
Future<void> showMamCredentialsSheet(BuildContext context, Map<String, dynamic> creds) => showKSheet<void>(
  context,
  dismissible: false,
  builder: (ctx) {
    final t = ctx.t;
    final funding = mapOf(creds['funding']);
    return KSheetContent(
      footer: KButton(label: t('common.done'), size: KButtonSize.lg, expand: true, onPressed: () => Navigator.of(ctx).pop()),
      children: [
        SheetTitle(title: t('social.mm.creds.title'), description: t('social.mm.creds.description')),
        SecretRow(label: t('social.md.fund.login'), value: strOf(creds['login'])),
        SecretRow(label: t('social.md.fund.tradingPassword'), value: strOf(creds['password']), secret: true),
        SecretRow(label: t('social.mm.creds.investorPassword'), value: strOf(creds['investorPassword']), secret: true),
        if (funding['status'] == 'failed') ...[
          const SizedBox(height: 6),
          InfoBox(tone: KChipTone.warn, icon: LucideIcons.shieldAlert, text: strOf(funding['message'])),
        ],
        const SizedBox(height: 10),
        Hint(t('social.mm.creds.note')),
      ],
    );
  },
);

/* ------------------------------------------------------------------ dashboard */

class _Dashboard extends ConsumerWidget {
  const _Dashboard({required this.me, required this.readOnly, required this.reload});
  final ManagerMe me;
  final bool readOnly;
  final VoidCallback reload;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final m = me.manager!;
    final tot = me.totals;
    final perAccount = !_share(m.method);
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.72;
    final feesPending = numOf(tot['feesPending']);
    final mamResult = numOf(tot['mamResult']);
    final statusChip = StatusChip(
      status: m.status == 'active' ? 'active' : (m.status == 'frozen' ? 'suspended' : 'stopped'),
      label: m.status == 'frozen' ? t('social.mm.frozen') : t.dyn('social.managerStatus.${m.status}', fallback: m.status),
    );

    void edit() => showKSheet<void>(
      context,
      expand: true,
      builder: (ctx) => ProgrammeForm(
        me: me,
        m: m,
        inSheet: true,
        onDone: (_) {
          Navigator.of(ctx).pop();
          reload();
        },
      ),
    );
    void terms() => showKSheet<void>(
      context,
      builder: (ctx) => KSheetContent(
        children: [
          SheetTitle(title: ctx.t('social.mm.termsTitle'), description: ctx.t('social.mm.termsDesc')),
          for (final p in termsParagraphs(me.termsText))
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Text(p, style: ctx.text.footnote.copyWith(color: ctx.k.fg2, fontSize: 12.5, height: 1.5)),
            ),
        ],
      ),
    );

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 52,
                height: 52,
                decoration: BoxDecoration(
                  color: k.surface2,
                  borderRadius: BorderRadius.circular(16),
                  border: Border.all(color: k.line),
                ),
                child: Icon(LucideIcons.briefcase, size: 22, color: k.fg2),
              ),
              const SizedBox(height: 12),
              Wrap(
                spacing: 8,
                runSpacing: 6,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  Text(m.name, style: context.text.title1.copyWith(fontSize: 20, fontWeight: FontWeight.w500)),
                  statusChip,
                  KChip(label: methodLabel(t, m.method), small: true),
                ],
              ),
              const SizedBox(height: 8),
              Wrap(
                spacing: 14,
                runSpacing: 4,
                children: [
                  if (m.login != null)
                    Text.rich(
                      TextSpan(
                        children: [
                          TextSpan(text: '${t('social.mm.masterAccount')} '),
                          TextSpan(
                            text: '#${m.login}',
                            style: context.text.mono(12, color: k.fg2),
                          ),
                        ],
                      ),
                      style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                    ),
                  Text(
                    '${t('social.fees')} ${numText(m.perfFeePct)}%${m.mgmtFeePct > 0 ? ' + ${numText(m.mgmtFeePct)}%/y' : ''} · ${periodLabel(t, m.feePeriod).toLowerCase()}',
                    style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                  ),
                  Text(t('social.mm.minEquityLine', {'amount': usd(m.minEquity, 0)}), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)),
                  Text(t('social.mm.sinceLine', {'date': fmtDate(t, m.createdAt)}), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12)),
                ],
              ),
              if (m.freezeReason != null && m.freezeReason!.isNotEmpty) ...[
                const SizedBox(height: 6),
                Text(m.freezeReason!, style: context.text.footnote.copyWith(color: k.down)),
              ],
              const SizedBox(height: 14),
              Row(
                children: [
                  Expanded(
                    child: KButton(
                      label: t('social.mm.terms'),
                      icon: LucideIcons.fileText,
                      variant: KButtonVariant.surface,
                      size: KButtonSize.sm,
                      expand: true,
                      onPressed: terms,
                    ),
                  ),
                  if (!readOnly) ...[
                    const SizedBox(width: 8),
                    Expanded(
                      child: KButton(
                        label: t('common.edit'),
                        icon: LucideIcons.pencil,
                        variant: KButtonVariant.surface,
                        size: KButtonSize.sm,
                        expand: true,
                        onPressed: edit,
                      ),
                    ),
                  ],
                ],
              ),
              if (m.login != null) ...[
                const SizedBox(height: 8),
                TraderButton(login: m.login, label: t('social.mm.tradeMaster'), variant: KButtonVariant.ember, size: KButtonSize.md, expand: true),
              ],
            ],
          ),
        ),
        const SizedBox(height: 16),
        SizedBox(
          height: 160,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('social.mm.linkedAccounts'),
                icon: LucideIcons.users,
                value: Num('${intOf(tot['accounts'])}', style: context.text.moneyL),
                chip: KChip(label: methodLabel(t, m.method)),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.mm.kpi.equity'),
                icon: LucideIcons.wallet,
                value: Num(compactUsd(numOf(tot['equity'])), style: context.text.moneyL),
                chip: KChip(label: t('social.mm.kpi.equityChip')),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.mam.result'),
                icon: LucideIcons.layers,
                value: Num(usd(mamResult, 2, true), color: toneColor(context, mamResult), style: context.text.moneyL),
                chip: KChip(label: t('social.mm.kpi.resultChip')),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('social.fees'),
                icon: LucideIcons.percent,
                value: KMoney(numOf(tot['feesPaid']), style: context.text.moneyL),
                chip: KChip(label: t('social.mm.kpi.feesChip', {'amount': usd(feesPending)}), tone: feesPending > 0 ? KChipTone.warn : KChipTone.neutral),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        _PreviewCard(m: m),
        const SizedBox(height: 16),
        SectionCard(
          title: t('social.mm.linkedAccounts'),
          subtitle: t('social.mm.linkedSub'),
          icon: LucideIcons.users,
          child: me.links.isEmpty
              ? EmptyRow(t('social.mm.noLinks'))
              : RowsBox(
                  boxed: false,
                  children: [
                    for (final l in me.links)
                      _ManagedLinkRow(
                        l: l,
                        method: m.method,
                        onSet: perAccount && l.status == 'active' && !readOnly ? () => _showValueSheet(context, m, l, onSaved: reload) : null,
                      ),
                  ],
                ),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: t('social.mm.audit'),
          subtitle: t('social.mm.auditSub'),
          icon: LucideIcons.clock,
          child: me.allocations.isEmpty
              ? EmptyRow(t('social.mm.noBlocks'))
              : RowsBox(
                  boxed: false,
                  children: [
                    for (final a in me.allocations)
                      DataLine(
                        onTap: () => _showAllocationSheet(context, a),
                        title: Text.rich(
                          TextSpan(
                            children: [
                              TextSpan(
                                text: '#${strOf(a['masterTicket'])} ',
                                style: TextStyle(color: k.fg3, fontWeight: FontWeight.w400),
                              ),
                              TextSpan(text: '${strOf(a['symbol'])} '),
                              TextSpan(
                                text: t.dyn('common.${strOf(a['side'])}', fallback: strOf(a['side'])).toLowerCase(),
                                style: TextStyle(color: a['side'] == 'buy' ? k.up : k.down),
                              ),
                              if (a['action'] != 'open')
                                TextSpan(
                                  text: ' · ${t.dyn('social.logAction.${strOf(a['action'])}', fallback: strOf(a['action'])).toLowerCase()}',
                                  style: TextStyle(color: k.fg3, fontWeight: FontWeight.w400),
                                ),
                            ],
                          ),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        subtitle: Text(serverTime(t, strOrNull(a['at']), withYear: false)),
                        trailing: Num(lots(numOrNull(a['allocated']))),
                        trailingSub: Num('${t('social.mm.block')} ${lots(numOrNull(a['block']))}'),
                      ),
                  ],
                ),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: t('social.fees'),
          subtitle: t('social.md.perfFeesSub'),
          icon: LucideIcons.percent,
          child: me.fees.isEmpty
              ? EmptyRow(t('social.mam.noFees'))
              : RowsBox(
                  boxed: false,
                  children: [
                    for (final f in me.fees)
                      DataLine(
                        title: Num(f.login, style: context.text.mono(12.5)),
                        subtitle: Text(fmtDate(t, f.periodEnd)),
                        trailing: Num(usd(f.masterAmount)),
                        trailingSub: KChip(
                          label: t.dyn('social.feeStatus.${f.status}', fallback: f.status),
                          tone: feeStatusTone[f.status] ?? KChipTone.neutral,
                          small: true,
                        ),
                      ),
                  ],
                ),
        ),
      ],
    );
  }
}

class _ManagedLinkRow extends StatelessWidget {
  const _ManagedLinkRow({required this.l, required this.method, this.onSet});
  final LinkView l;
  final String method;
  final VoidCallback? onSet;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final perAccount = !_share(method);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 10),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Num(l.login, style: context.text.mono(13, color: k.fg)),
                    const SizedBox(width: 8),
                    StatusChip(
                      status: l.status,
                      label: t.dyn('social.linkStatus.${l.status}', fallback: l.status),
                    ),
                  ],
                ),
                const SizedBox(height: 3),
                Num(
                  [
                    usd(l.equity),
                    if (perAccount) '${method == 'percent' ? t('social.mm.percent') : t('social.sizing.multiplier')} ${valueText(method, l.allocValue)}',
                  ].join(' · '),
                  color: k.fg3,
                  style: context.text.footnote.copyWith(fontSize: 12),
                ),
              ],
            ),
          ),
          const SizedBox(width: 8),
          Column(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Num(usd(l.mamResult, 2, true), color: toneColor(context, l.mamResult), style: context.text.figure.copyWith(fontSize: 13.5)),
              Text(
                t('social.mam.result'),
                style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
              ),
            ],
          ),
          if (onSet != null) ...[
            const SizedBox(width: 8),
            KButton(label: t('social.mm.set'), icon: LucideIcons.pencil, variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onSet),
          ],
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ allocation preview */

class _PreviewCard extends ConsumerStatefulWidget {
  const _PreviewCard({required this.m});
  final ManagerView m;

  @override
  ConsumerState<_PreviewCard> createState() => _PreviewCardState();
}

class _PreviewCardState extends ConsumerState<_PreviewCard> {
  final _symbol = TextEditingController(text: 'EURUSD');
  final _volume = TextEditingController(text: '1');
  (String, double) _q = ('EURUSD', 1);

  @override
  void dispose() {
    _symbol.dispose();
    _volume.dispose();
    super.dispose();
  }

  void _apply() {
    final s = _symbol.text.trim().toUpperCase();
    final v = parseAmount(_volume.text);
    if (!RegExp(r'^[A-Z0-9._]{2,20}$').hasMatch(s) || v == null || v <= 0) return plainError(ref, context.t('social.mm.preview.err'));
    FocusScope.of(context).unfocus();
    setState(() => _q = (s, v));
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final m = widget.m;
    final q = ref.watch(mamPreviewProvider(_q));
    final data = q.value;
    final rows = listOf(data?['rows']);
    final share = _share(m.method);
    return SectionCard(
      title: t('social.mm.preview.title'),
      subtitle: t('social.mm.preview.subtitle', {'method': methodLabel(t, m.method)}),
      icon: LucideIcons.calculator,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.end,
            children: [
              Expanded(
                child: KTextField(
                  controller: _symbol,
                  label: t('social.col.symbol'),
                  ltr: true,
                  textCapitalization: TextCapitalization.characters,
                  inputFormatters: [LengthLimitingTextInputFormatter(20), _Upper()],
                  onSubmitted: (_) => _apply(),
                ),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: NumberField(controller: _volume, label: t('social.mm.block'), unit: t('social.lotsUnit')),
              ),
            ],
          ),
          const SizedBox(height: 10),
          KButton(label: t('social.mm.preview.button'), icon: LucideIcons.calculator, variant: KButtonVariant.surface, expand: true, onPressed: _apply),
          const SizedBox(height: 12),
          if (q.hasError && data == null)
            EmptyRow(socialError(q.error, t))
          else if (data == null)
            const BlockSkeleton(n: 1, h: 80)
          else if (rows.isEmpty)
            EmptyRow(t('social.mm.preview.empty'))
          else ...[
            RowsBox(
              children: [
                for (final r in rows)
                  DataLine(
                    title: Num(strOf(r['account']), style: context.text.mono(12.5)),
                    subtitle: Num(
                      '${usd(numOf(r[m.method == 'balance' ? 'balance' : 'equity']))} · '
                      '${share ? '${(numOf(r['basis']) * 100).toStringAsFixed(2)}%' : valueText(m.method, numOrNull(r['value']))}',
                    ),
                    trailing: Num(lots(numOrNull(r['volume'])), style: const TextStyle(fontWeight: FontWeight.w700)),
                    trailingSub: r['reason'] != null ? Text(reasonText(t, strOrNull(r['reason']))) : null,
                  ),
              ],
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 16,
              runSpacing: 4,
              children: [
                _Fig(label: t('social.mm.block'), value: '${lots(numOrNull(data['block']))} ${strOf(data['symbol'])}'),
                _Fig(label: t('social.mm.allocated'), value: lots(numOrNull(data['allocated'])), strong: true),
                if (share) _Fig(label: t('social.mm.leftByRounding'), value: lots(numOrNull(data['unallocated']))),
                Text(
                  t('social.mm.lotStep', {'step': numText(numOf(data['lotStep'])), 'min': numText(numOf(data['lotMin']))}),
                  style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12.5),
                ),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

class _Upper extends TextInputFormatter {
  @override
  TextEditingValue formatEditUpdate(TextEditingValue oldValue, TextEditingValue newValue) => newValue.copyWith(text: newValue.text.toUpperCase());
}

class _Fig extends StatelessWidget {
  const _Fig({required this.label, required this.value, this.strong = false});
  final String label;
  final String value;
  final bool strong;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Text.rich(
      TextSpan(
        children: [
          TextSpan(text: '$label '),
          TextSpan(
            text: value,
            style: TextStyle(color: strong ? k.fg : k.fg2, fontFeatures: kTabular, fontWeight: strong ? FontWeight.w600 : null),
          ),
        ],
      ),
      style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12.5),
    );
  }
}

/* ------------------------------------------------------------------ per-account value, allocation detail */

Future<void> _showValueSheet(BuildContext context, ManagerView m, LinkView link, {required VoidCallback onSaved}) => showKSheet<void>(
  context,
  builder: (_) => _ValueSheet(m: m, link: link, onSaved: onSaved),
);

class _ValueSheet extends ConsumerStatefulWidget {
  const _ValueSheet({required this.m, required this.link, required this.onSaved});
  final ManagerView m;
  final LinkView link;
  final VoidCallback onSaved;

  @override
  ConsumerState<_ValueSheet> createState() => _ValueSheetState();
}

class _ValueSheetState extends ConsumerState<_ValueSheet> {
  late final _v = TextEditingController(text: inputText(widget.link.allocValue));
  bool _busy = false;

  bool get _pct => widget.m.method == 'percent';

  @override
  void dispose() {
    _v.dispose();
    super.dispose();
  }

  Future<void> _save(double v) async {
    final t = context.t;
    final l = widget.link;
    setState(() => _busy = true);
    try {
      await socialPatch(ref, 'mam/manager/links/${l.id}', {'value': v});
      okToast(
        ref,
        t('social.mm.toast.allocUpdated'),
        _pct
            ? t('social.mm.toast.allocPercent', {'login': l.login, 'value': numText(v)})
            : t('social.mm.toast.allocMultiplier', {'login': l.login, 'value': numText(v)}),
      );
      widget.onSaved();
      if (mounted) Navigator.of(context).pop();
    } on ApiException catch (e) {
      if (mounted) errToast(ref, context, t('social.mm.toast.couldntUpdate'), e);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final v = parseAmount(_v.text);
    final err = v == null || v < 0.01 || v > (_pct ? 1000 : 100) ? (_pct ? t('social.mm.err.percent') : t('social.mm.err.multiplier')) : null;
    return KSheetContent(
      footer: SheetFooter(label: t('common.save'), busy: _busy, onPressed: err != null ? null : () => _save(v!)),
      children: [
        SheetTitle(
          title: _pct ? t('social.mm.percentOfBlock') : t('social.sizing.multiplier'),
          description: t('social.mm.accountNo', {'login': widget.link.login}),
        ),
        NumberField(
          controller: _v,
          label: _pct ? t('social.mm.percent') : t('social.sizing.multiplier'),
          unit: _pct ? '%' : '×',
          error: _v.text.trim().isNotEmpty ? err : null,
          onChanged: (_) => setState(() {}),
        ),
        Hint(t('social.mm.valueNote'), top: 10),
      ],
    );
  }
}

Future<void> _showAllocationSheet(BuildContext context, Map<String, dynamic> a) => showKSheet<void>(
  context,
  builder: (ctx) {
    final t = ctx.t;
    final k = ctx.k;
    final method = strOf(a['method']);
    final action = switch (strOf(a['action'])) {
      'order' => t('social.mm.alloc.order'),
      'add' => t('social.mm.alloc.add'),
      _ => t('social.logAction.open'),
    };
    return KSheetContent(
      children: [
        SheetTitle(
          title:
              '$action #${strOf(a['masterTicket'])} · ${strOf(a['symbol'])} ${t.dyn('common.${strOf(a['side'])}', fallback: strOf(a['side'])).toLowerCase()}',
          description:
              '${serverTime(t, strOrNull(a['at']))} · ${t('social.mm.alloc.block', {'lots': lots(numOrNull(a['block']))})} · ${methodLabel(t, method)}',
        ),
        RowsBox(
          pageSize: 50,
          children: [
            for (final d in listOf(a['details']))
              DataLine(
                title: Num(strOf(d['login']), style: ctx.text.mono(12.5)),
                subtitle: Text(
                  [
                    _share(method) ? '${(numOf(d['basis']) * 100).toStringAsFixed(2)}%' : valueText(method, numOrNull(d['value'])),
                    t('social.mm.alloc.exact', {'value': numOf(d['raw']).toStringAsFixed(4)}),
                    if (strOf(d['message']).isNotEmpty || strOf(d['reason']).isNotEmpty)
                      strOf(d['message']).isNotEmpty ? strOf(d['message']) : reasonText(t, strOrNull(d['reason'])),
                  ].join(' · '),
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
                trailing: Num(
                  t('social.lotsValue', {'lots': lots(numOrNull(d['volume']))}),
                  style: TextStyle(color: k.fg, fontWeight: FontWeight.w600),
                ),
                trailingSub: KChip(
                  label: t.dyn('social.logStatus.${strOf(d['status'])}', fallback: strOf(d['status'])),
                  tone: d['status'] == 'done' ? KChipTone.up : (d['status'] == 'failed' ? KChipTone.down : KChipTone.neutral),
                  small: true,
                ),
              ),
          ],
        ),
      ],
    );
  },
);

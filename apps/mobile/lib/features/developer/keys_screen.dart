// API & Algo › API keys (/developer): port of apps/crm/components/algo/keys-page.tsx (LiveKeysPage), phone order:
//   header (API docs, Create API key) · KPIs (requests, errors, latency, active keys) · the new key's secret (once)
//   · your API keys (activity, revoke) · quickstart · safety (kill switch link).
// Create key sheet (name, account, scopes, IP whitelist, expiry) and the 24 h activity sheet.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'developer_api.dart';
import 'widgets/algo_widgets.dart';

class DeveloperKeysScreen extends ConsumerStatefulWidget {
  const DeveloperKeysScreen({super.key});

  @override
  ConsumerState<DeveloperKeysScreen> createState() => _DeveloperKeysScreenState();
}

class _DeveloperKeysScreenState extends ConsumerState<DeveloperKeysScreen> {
  ({String keyId, String secret, String name})? _secret;

  Future<void> _refresh() async {
    ref
      ..invalidate(apiKeysProvider)
      ..invalidate(algoAccountsProvider);
    await ref.read(apiKeysProvider.future).then((_) {}, onError: (Object _) {});
  }

  Future<void> _create(List<AlgoAccount> accounts) async {
    final r = await showKSheet<({String keyId, String secret, String name})>(
      context,
      title: context.t('developer.keys.create'),
      builder: (_) => _CreateKeySheet(accounts: accounts.where((a) => a.active).toList()),
    );
    if (r == null || !mounted) return;
    setState(() => _secret = r);
    ref.invalidate(apiKeysProvider);
  }

  Future<void> _revoke(ApiKey k) async {
    final t = context.t;
    final ok = await showKAlert<bool>(
      context,
      title: t('developer.keys.revoke'),
      message: t('developer.keys.revokeConfirm', {'name': k.name}),
      actions: [
        KAction(label: t('common.cancel'), value: false),
        KAction(label: t('developer.keys.revoke'), value: true, destructive: true, primary: true),
      ],
    );
    if (ok != true || !mounted) return;
    try {
      await ref.read(apiProvider).algoPost('keys/${k.id}/revoke');
      algoOk(ref, t('developer.keys.revoked'));
      ref.invalidate(apiKeysProvider);
    } on Object catch (e) {
      algoFail(ref, t, t('developer.keys.revokeFailed'), e);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final readOnly = ref.watch(meProvider)?.readOnly ?? true;
    final keys = ref.watch(apiKeysProvider);
    final accounts = ref.watch(algoAccountsProvider).value ?? const <AlgoAccount>[];
    final data = keys.value;
    final items = data?.items ?? const <ApiKey>[];
    final active = items.where((x) => x.status == 'active').length;
    final base = data?.baseUrl ?? '';
    final f = LocaleFormat(t.locale);
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.78;

    final hero = pageHero(
      context,
      ref,
      path: '/developer',
      title: t('developer.keys.pageTitle'),
      lead: t('developer.keys.pageSubtitle'),
      actions: [
        KHeroButton(label: t('developer.docs.title'), icon: LucideIcons.bookOpen, onPressed: () => context.go('/developer/docs')),
        if (!readOnly)
          KHeroButton(label: t('developer.keys.create'), icon: LucideIcons.plus, primary: true, onPressed: accounts.isEmpty ? null : () => _create(accounts)),
      ],
    );

    return KPageScroll(
      onRefresh: _refresh,
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        if (hero == null) ...[
          KPageHeader(title: t('developer.keys.pageTitle'), subtitle: Text(t('developer.keys.pageSubtitle'))),
          const SizedBox(height: 14),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              KButton(
                label: t('developer.docs.title'),
                icon: LucideIcons.bookOpen,
                variant: KButtonVariant.surface,
                onPressed: () => context.go('/developer/docs'),
              ),
              if (!readOnly) KButton(label: t('developer.keys.create'), icon: LucideIcons.plus, onPressed: accounts.isEmpty ? null : () => _create(accounts)),
            ],
          ),
          const SizedBox(height: 20),
        ],
        // KPIs
        SizedBox(
          height: 186,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            physics: const PageScrollPhysics(parent: BouncingScrollPhysics()),
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('developer.keys.requests24h'),
                icon: LucideIcons.activity,
                value: Text(f.number(data?.requests24h ?? 0, 0)),
                footer: Row(
                  children: [
                    Flexible(child: KChip(label: t('developer.keys.ordersCloses', {'n': data?.writes24h ?? 0}))),
                    const SizedBox(width: 10),
                    if ((data?.hourly.length ?? 0) > 1)
                      SizedBox(
                        width: 84,
                        child: KBarChart(values: data!.hourly, height: 24, color: k.fg3, highlightLast: true),
                      ),
                  ],
                ),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('developer.keys.errors24h'),
                icon: LucideIcons.circleAlert,
                value: Text('${data?.errors24h ?? 0}'),
                chip: KChip(label: t('developer.keys.rateLimited', {'n': data?.rateLimited24h ?? 0})),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('developer.keys.latencyP50'),
                icon: LucideIcons.gauge,
                value: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: '${(data?.p50 ?? 0).round()}'),
                      TextSpan(
                        text: ' ${t('developer.unit.ms')}',
                        style: TextStyle(color: k.fg3, fontSize: 20),
                      ),
                    ],
                  ),
                ),
                chip: KChip(label: 'p99 ${(data?.p99 ?? 0).round()} ${t('developer.unit.ms')}'),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('developer.keys.activeKeys'),
                icon: LucideIcons.keyRound,
                value: Text.rich(
                  TextSpan(
                    children: [
                      TextSpan(text: '$active'),
                      TextSpan(
                        text: '/${items.length}',
                        style: TextStyle(color: k.fg3),
                      ),
                    ],
                  ),
                ),
                chip: KChip(label: t('developer.keys.max', {'n': 20})),
              ),
            ],
          ),
        ),
        if (_secret != null) ...[const SizedBox(height: 16), _SecretCard(secret: _secret!, base: base, onDone: () => setState(() => _secret = null))],
        const SizedBox(height: 16),
        // your API keys
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('developer.keys.title'), subtitle: t('developer.keys.subtitle')),
              const SizedBox(height: 8),
              KAsync(
                value: keys,
                onRetry: () => ref.invalidate(apiKeysProvider),
                loading: Padding(padding: const EdgeInsets.only(top: 8), child: KSkeleton.lines(3)),
                error: (e) => KLoadError(error: e, card: false, onRetry: () => ref.invalidate(apiKeysProvider)),
                builder: (d) => d.items.isEmpty
                    ? Padding(
                        padding: const EdgeInsets.symmetric(vertical: 28),
                        child: Text(
                          t('developer.keys.none'),
                          textAlign: TextAlign.center,
                          style: context.text.footnote.copyWith(color: k.fg3),
                        ),
                      )
                    : Column(
                        crossAxisAlignment: CrossAxisAlignment.stretch,
                        children: [
                          for (var i = 0; i < d.items.length; i++) ...[
                            if (i > 0) const KDivider(),
                            _KeyRow(
                              k: d.items[i],
                              readOnly: readOnly,
                              onActivity: () => showKSheet<void>(
                                context,
                                title: t('developer.keys.activityTitle'),
                                builder: (_) => _ActivitySheet(id: d.items[i].id),
                              ),
                              onRevoke: () => _revoke(d.items[i]),
                            ),
                          ],
                        ],
                      ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        // quickstart
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('developer.keys.quickstart'), subtitle: base),
              const SizedBox(height: 14),
              CodeBlock(_quickstart(base)),
            ],
          ),
        ),
        const SizedBox(height: 16),
        // safety
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('developer.keys.safety'), subtitle: t('developer.keys.safetySub')),
              const SizedBox(height: 12),
              for (final line in [
                t('developer.keys.safety1'),
                t('developer.keys.safety2'),
                t('developer.keys.safety3'),
                t('developer.keys.safety4'),
                t('developer.keys.safety5'),
              ])
                Padding(
                  padding: const EdgeInsets.only(bottom: 8),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Padding(
                        padding: const EdgeInsets.only(top: 1),
                        child: Icon(LucideIcons.shieldCheck, size: 16, color: k.up),
                      ),
                      const SizedBox(width: 8),
                      Expanded(
                        child: Text(line, style: context.text.callout.copyWith(color: k.fg2)),
                      ),
                    ],
                  ),
                ),
              const SizedBox(height: 4),
              KRichText(
                t('developer.keys.killHint'),
                style: context.text.footnote.copyWith(color: k.fg3),
                tags: {'link': KTag.link(() => context.go('/developer/deployments'))},
              ),
            ],
          ),
        ),
      ],
    );
  }
}

String _quickstart(String base) =>
    '''# read the account
curl $base/account -H "Authorization: Bearer \$KEY_ID:\$SECRET"

# market order with stop and target (scope: trade)
curl -X POST $base/orders -H "Authorization: Bearer \$KEY_ID:\$SECRET" \\
  -H "content-type: application/json" \\
  -d '{"symbol":"XAUUSD","side":"sell","volume":0.1,"sl":2710,"tp":2650}'

# close a position
curl -X POST $base/positions/1000123/close -H "Authorization: Bearer \$KEY_ID:\$SECRET"''';

/// One key (web DataTable row on a phone: key, account, scopes, last used, status, Activity / Revoke).
class _KeyRow extends StatelessWidget {
  const _KeyRow({required this.k, required this.readOnly, required this.onActivity, required this.onRevoke});
  final ApiKey k;
  final bool readOnly;
  final VoidCallback onActivity, onRevoke;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final c = context.k;
    final activeKey = k.status == 'active';
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 12),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(k.name, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.headline.copyWith(fontSize: 14.5)),
                    const SizedBox(height: 2),
                    Text(
                      k.keyId,
                      textDirection: TextDirection.ltr,
                      style: context.text.mono(11.5, color: c.fg3),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: 8),
              KChip(
                label: t.dyn('developer.keyStatus.${k.status}', fallback: k.status),
                tone: activeKey ? KChipTone.up : KChipTone.neutral,
                dot: activeKey,
                small: true,
              ),
            ],
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            crossAxisAlignment: WrapCrossAlignment.center,
            children: [
              Text('${accountTypeLabel(t, k.accountType)} #${k.login}', style: context.text.mono(12, color: c.fg2)),
              for (final s in k.scopes)
                KChip(
                  label: t.dyn('developer.scope.$s', fallback: s),
                  tone: s == 'trade' ? KChipTone.ember : KChipTone.up,
                  small: true,
                ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '${t('developer.keys.lastUsed')}: ${algoAgo(t, k.lastUsedAt)}${k.lastIp != null ? ' · ${k.lastIp}' : ''}',
            style: context.text.caption.copyWith(color: c.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(height: 8),
          Row(
            children: [
              KButton(label: t('developer.keys.activity'), variant: KButtonVariant.ghost, size: KButtonSize.sm, onPressed: onActivity),
              if (activeKey && !readOnly) ...[
                const SizedBox(width: 6),
                KButton(label: t('developer.keys.revoke'), variant: KButtonVariant.danger, size: KButtonSize.sm, onPressed: onRevoke),
              ],
            ],
          ),
        ],
      ),
    );
  }
}

/// The new key's id and secret, shown once (web "created" card with the curl example).
class _SecretCard extends StatelessWidget {
  const _SecretCard({required this.secret, required this.base, required this.onDone});
  final ({String keyId, String secret, String name}) secret;
  final String base;
  final VoidCallback onDone;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final auth = '${secret.keyId}:${secret.secret}';
    final curl =
        'curl $base/account \\\n  -H "Authorization: Bearer $auth"\n\ncurl -X POST $base/orders \\\n  -H "Authorization: Bearer $auth" \\\n  -H "content-type: application/json" \\\n  -d \'{"symbol":"EURUSD","side":"buy","volume":0.01,"sl":null}\'';
    Widget line(String label, String value, Color color) => Container(
      padding: const EdgeInsetsDirectional.only(start: 12, end: 2),
      decoration: BoxDecoration(color: k.dark ? Colors.black.withValues(alpha: 0.3) : k.surface2, borderRadius: BorderRadius.circular(12)),
      child: Row(
        children: [
          Text(
            label,
            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(
              value,
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              textDirection: TextDirection.ltr,
              style: context.text.mono(12.5, color: color),
            ),
          ),
          CopyIcon(value, label: label),
        ],
      ),
    );
    return Container(
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(k.cardRadius),
        border: Border.all(color: k.ember.withValues(alpha: 0.4)),
      ),
      child: KCard(
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            KCardHeader(icon: LucideIcons.shieldCheck, title: t('developer.keys.created', {'name': secret.name}), subtitle: t('developer.keys.createdText')),
            const SizedBox(height: 14),
            line(t('developer.keys.keyId'), secret.keyId, k.fg),
            const SizedBox(height: 8),
            line(t('developer.keys.secret'), secret.secret, k.ember),
            const SizedBox(height: 10),
            CodeBlock(curl, copy: false),
            const SizedBox(height: 12),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: KButton(label: t('developer.keys.stored'), variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: onDone),
            ),
          ],
        ),
      ),
    );
  }
}

/// Create API key (web CreateKeyDialog).
class _CreateKeySheet extends ConsumerStatefulWidget {
  const _CreateKeySheet({required this.accounts});
  final List<AlgoAccount> accounts;

  @override
  ConsumerState<_CreateKeySheet> createState() => _CreateKeySheetState();
}

class _CreateKeySheetState extends ConsumerState<_CreateKeySheet> {
  late final TextEditingController _name = TextEditingController(text: context.t('developer.keys.defaultName'));
  final TextEditingController _ips = TextEditingController();
  int? _login;
  bool _trade = true;
  int _days = 90;
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    if (widget.accounts.isNotEmpty) _login = (widget.accounts.where((a) => a.type == 'demo').firstOrNull ?? widget.accounts.first).login;
  }

  @override
  void dispose() {
    _name.dispose();
    _ips.dispose();
    super.dispose();
  }

  Future<void> _create() async {
    final t = context.t;
    setState(() => _busy = true);
    try {
      final r = await ref.read(apiProvider).algoPost('keys', {
        'name': _name.text,
        'login': _login,
        'scopes': _trade ? ['read', 'trade'] : ['read'],
        'ipWhitelist': _ips.text.split(RegExp(r'[\s,]+')).where((s) => s.isNotEmpty).toList(),
        if (_days > 0) 'expiresInDays': _days,
      });
      if (!mounted) return;
      KHaptics.success();
      Navigator.of(context).pop((keyId: jS(r['keyId']), secret: jS(r['secret']), name: jS(r['name'])));
    } on Object catch (e) {
      algoFail(ref, t, t('developer.keys.createFailed'), e);
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final acct = widget.accounts.where((a) => a.login == _login).firstOrNull;
    return KSheetContent(
      footer: KButton(
        label: t('developer.keys.createKey'),
        icon: LucideIcons.keyRound,
        expand: true,
        size: KButtonSize.lg,
        loading: _busy,
        onPressed: _login == null ? null : _create,
      ),
      children: [
        Text(
          t('developer.keys.createText'),
          textAlign: TextAlign.center,
          style: context.text.footnote.copyWith(color: k.fg3),
        ),
        const SizedBox(height: 16),
        KTextField(label: t('common.name'), controller: _name),
        const SizedBox(height: 16),
        FieldLabel(t('developer.tradingAccount')),
        PillChoice<int>(
          values: [for (final a in widget.accounts) a.login],
          labels: [for (final a in widget.accounts) '${accountTypeLabel(t, a.type)} #${a.login}'],
          isSelected: (v) => v == _login,
          onTap: (v) => setState(() => _login = v),
        ),
        const SizedBox(height: 16),
        FieldLabel(t('developer.keys.scopes')),
        Row(
          children: [
            KChip(label: t('developer.scope.read'), tone: KChipTone.up),
            const SizedBox(width: 8),
            KPressable(
              semanticLabel: t('developer.scope.trade'),
              onTap: () => setState(() => _trade = !_trade),
              child: KChip(label: _trade ? t('developer.scope.trade') : '+ ${t('developer.scope.trade')}', tone: _trade ? KChipTone.ember : KChipTone.neutral),
            ),
          ],
        ),
        const SizedBox(height: 12),
        CodeField(
          controller: _ips,
          label: t('developer.keys.ipWhitelist'),
          hint: Text(
            acct?.live == true && _trade ? t('developer.keys.ipRequired') : t('developer.keys.ipOptional'),
            style: context.text.caption.copyWith(color: acct?.live == true && _trade ? k.warn : k.fg3),
          ),
          placeholder: '203.0.113.10, 198.51.100.0/24',
          maxLines: 4,
        ),
        const SizedBox(height: 16),
        FieldLabel(t('developer.keys.expires')),
        PillChoice<int>(
          values: const [30, 90, 365, 0],
          labels: [t('developer.keys.days30'), t('developer.keys.days90'), t('developer.keys.year1'), t('developer.keys.never')],
          isSelected: (v) => v == _days,
          onTap: (v) => setState(() => _days = v),
        ),
      ],
    );
  }
}

/// The key's requests in the last 24 h (web Activity24 drawer).
class _ActivitySheet extends ConsumerWidget {
  const _ActivitySheet({required this.id});
  final int id;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final a = ref.watch(keyActivityProvider(id));
    return KSheetContent(
      children: [
        Text(
          t('developer.keys.activityText'),
          textAlign: TextAlign.center,
          style: context.text.footnote.copyWith(color: k.fg3),
        ),
        const SizedBox(height: 12),
        KAsync(
          value: a,
          onRetry: () => ref.invalidate(keyActivityProvider(id)),
          loading: KSkeleton.lines(5),
          builder: (rows) => rows.isEmpty
              ? Padding(
                  padding: const EdgeInsets.symmetric(vertical: 32),
                  child: Text(
                    t('developer.keys.noRequests'),
                    textAlign: TextAlign.center,
                    style: context.text.footnote.copyWith(color: k.fg3),
                  ),
                )
              : Directionality(
                  textDirection: TextDirection.ltr,
                  child: Column(
                    children: [
                      for (final r in rows)
                        Container(
                          padding: const EdgeInsets.symmetric(vertical: 7),
                          decoration: BoxDecoration(
                            border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
                          ),
                          child: Row(
                            children: [
                              Text(_short(fmtDateTime(r.at)), style: context.text.mono(11, color: k.fg3)),
                              const SizedBox(width: 8),
                              Text('${r.status}', style: context.text.mono(11, color: r.status >= 400 ? k.down : k.up)),
                              const SizedBox(width: 8),
                              Text(r.method, style: context.text.mono(11, color: k.fg2)),
                              const SizedBox(width: 8),
                              Expanded(
                                child: Text(
                                  r.path,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: context.text.mono(11, color: k.fg),
                                ),
                              ),
                              Text('${r.ms}ms', style: context.text.mono(11, color: k.fg3)),
                            ],
                          ),
                        ),
                    ],
                  ),
                ),
        ),
      ],
    );
  }
}

String _short(String dt) => dt.length > 5 ? dt.substring(5) : dt;

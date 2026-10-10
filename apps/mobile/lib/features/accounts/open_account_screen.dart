// Accounts › Open account. Port of the phone web page /accounts/new (apps/crm/components/trading/open-account.tsx
// LiveOpenAccount), the wizard in its phone layout (the desktop's sticky summary is hidden on phones):
//   0 Product: a CFD account or an Options account (?product=options; only CFD while the broker has the Options
//     module off, then the step is skipped). An Options account first shows the options intro (the three ideas,
//     "I understand", the terms: suitability/options) while the client hasn't accepted it yet.
//   1 Account: Live or Demo (?type=demo; demo only while the broker allows demo accounts)
//   2 Type: the broker's groups of that product for that kind, with the per-type account limit
//   3 Configure: leverage, demo starting balance, nickname, currency, own trading password (rules + generate)
//   4 Confirm: summary tiles, what happens, the risk acknowledgement
//   5 Done: the credentials (shown once: copy all, show / hide), Trade, Fund, View account
// `?group=<code>` preselects a type (and its product) and jumps to Configure; `?product=` preselects the product and
// goes on to the next step. `GET trading/groups` and `GET trading/accounts` once;
// `POST trading/accounts {type, group, leverage, name?, password?, initialBalance?}` opens it.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/config/app_config.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/models/trading.dart';
import '../../core/notifications/notifications.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import '../options_intro/options_screen.dart';
import 'account_actions.dart';
import 'accounts_data.dart';
import 'widgets/account_bits.dart';
import 'widgets/group_card.dart';

/// The wizard's steps. The options intro belongs to the Product step (the step indicator stays on it).
enum WizardStep { product, intro, account, type, configure, confirm, done }

/// The step indicator's labels (the Product step only while the broker has the Options module on).
const Map<WizardStep, String> kWizardSteps = {
  WizardStep.product: 'accounts.wizard.step.product',
  WizardStep.account: 'accounts.wizard.step.account',
  WizardStep.type: 'accounts.wizard.step.type',
  WizardStep.configure: 'accounts.wizard.step.configure',
  WizardStep.confirm: 'accounts.wizard.step.confirm',
  WizardStep.done: 'accounts.wizard.step.done',
};
const List<int> kDemoBalances = [1000, 5000, 10000, 25000, 50000, 100000];

/// Groups the wizard offers for a kind (and a product, cfd | options): enabled, not prop, open to that kind and of
/// that product (web `offers`).
bool groupOffers(EngineGroup g, AccountKind kind, [String? product]) =>
    g.enabled && clientGroup(g) && g.offers(kind) && (product == null || g.product == product);

/// Groups a client opens accounts in: not prop challenges, not the options market maker's.
bool clientGroup(EngineGroup g) {
  final c = g.code.toLowerCase();
  return !c.startsWith('prop') && c != 'options-mm';
}

/// Accounts of this kind (and the group's product) the client already holds in the group (web usedIn): the
/// per-type account limit.
int groupUsed(List<EngineAccount> accounts, EngineGroup g, AccountKind kind) =>
    accounts.where((a) => a.group == g.code && a.type == kind && a.product == g.product).length;

/// "$10,000" / "USC 1,000,000" (web money: a cent group shows the amount in cents).
String wizardMoney(num v, bool cent) => cent ? 'USC ${Fmt.number(v * 100, 0)}' : '\$${Fmt.number(v, 0)}';

class OpenAccountScreen extends ConsumerStatefulWidget {
  const OpenAccountScreen({super.key, this.query = const {}});

  /// The route's query parameters (the web page's search params: `type` = live | demo, `group` = a group code,
  /// `product` = cfd | options).
  final Map<String, String> query;

  @override
  ConsumerState<OpenAccountScreen> createState() => _OpenAccountScreenState();
}

class _OpenAccountScreenState extends ConsumerState<OpenAccountScreen> {
  final _scroll = ScrollController();
  late final bool _optionsOn = ref.read(configProvider).moduleOn('options');
  late WizardStep _step = _optionsOn ? WizardStep.product : WizardStep.account;
  late String _product = _optionsOn && widget.query['product'] == 'options' ? 'options' : 'cfd';

  /// `?product=` / `?group=`: where to go once the groups (and, for Options, the intro state) are known.
  late WizardStep? _jump = _optionsOn && (widget.query['product'] == 'options' || widget.query['product'] == 'cfd') ? WizardStep.account : null;
  late AccountKind _kind = widget.query['type'] == 'demo' && _demoOn ? AccountKind.demo : AccountKind.live;
  late String _group = widget.query['group'] ?? '';
  int _leverage = 0;
  final _nickname = TextEditingController();
  double _demoBalance = 10000;
  bool _ownPassword = false, _agree = false, _busy = false, _booted = false;

  /// The options intro: "I understand" ticked, the acceptance being recorded, recorded here.
  bool _understood = false, _accepting = false, _accepted = false;
  String _password = '', _confirm = '';
  OpenResult? _created;

  bool get _demoOn => ref.read(configProvider).flag('demo_accounts', fallback: true);

  @override
  void dispose() {
    _scroll.dispose();
    _nickname.dispose();
    super.dispose();
  }

  /// The intro comes before a first Options account (terms not accepted yet; unknown when the check failed: skipped).
  bool _needsIntro(AsyncValue<Suitability>? suit) => _product == 'options' && !_accepted && suit?.value != null && !suit!.value!.accepted;

  /// The steps in order for the chosen product.
  List<WizardStep> _flow(AsyncValue<Suitability>? suit) => [
    if (_optionsOn) WizardStep.product,
    if (_step == WizardStep.intro || _needsIntro(suit)) WizardStep.intro,
    WizardStep.account,
    WizardStep.type,
    WizardStep.configure,
    WizardStep.confirm,
    WizardStep.done,
  ];

  void _go(int d, AsyncValue<Suitability>? suit) {
    final flow = _flow(suit);
    final i = flow.indexOf(_step);
    setState(() => _step = flow[(i + d).clamp(0, flow.length - 1)]);
    if (_scroll.hasClients) unawaited(_scroll.animateTo(0, duration: const Duration(milliseconds: 260), curve: Curves.easeOutCubic));
  }

  void _pickGroup(EngineGroup g) => setState(() {
    _group = g.code;
    _leverage = g.leverages.contains(_leverage) ? _leverage : g.defaultLeverage;
    if (_demoBalance == 0) _demoBalance = g.demoInitialBalance;
  });

  /// Once groups load: honour ?group= (its product, then Configure), otherwise preselect the first group of the
  /// product (web boot effect).
  void _boot(List<EngineGroup> groups) {
    if (_booted || groups.isEmpty) return;
    _booted = true;
    final want = groups.where((x) => x.code == widget.query['group']).firstOrNull;
    if (want != null && (want.product == 'cfd' || _optionsOn)) _product = want.product;
    final pick = want != null && groupOffers(want, _kind, _product) ? want : groups.where((x) => groupOffers(x, _kind, _product)).firstOrNull;
    if (pick == null) return;
    _group = pick.code;
    _leverage = pick.defaultLeverage;
    _demoBalance = pick.demoInitialBalance;
    if (want != null && want.code == pick.code) _jump = WizardStep.configure;
  }

  /// Records the options terms from the intro step, then goes on.
  Future<void> _acceptIntro(Suitability data) async {
    final d = data.disclosure;
    if (d == null || _accepting) return;
    setState(() => _accepting = true);
    final ok = await acceptOptionsTerms(ref, context.t, d.version);
    if (!mounted) return;
    setState(() {
      _accepting = false;
      _accepted = ok || _accepted;
    });
    if (ok) _go(1, null);
  }

  Future<void> _create(EngineGroup g) async {
    final t = context.t;
    setState(() => _busy = true);
    final name = _nickname.text.trim();
    try {
      final j = await ref
          .read(apiProvider)
          .post<Map<String, dynamic>>(
            'trading/accounts',
            body: {
              'type': _kind.name,
              'group': g.code,
              'leverage': _leverage,
              if (name.isNotEmpty) 'name': name,
              if (_ownPassword) 'password': _password,
              if (_kind == AccountKind.demo) 'initialBalance': _demoBalance % 1 == 0 ? _demoBalance.toInt() : _demoBalance,
            },
          );
      if (!mounted) return;
      final res = OpenResult.fromJson(j);
      setState(() {
        _created = res;
        _password = '';
        _confirm = '';
      });
      setState(() => _step = WizardStep.done);
      refreshAccountData(ProviderScope.containerOf(context, listen: false));
      accountToast(
        context,
        NotificationKind.success,
        t(_kind == AccountKind.live ? 'accounts.wizard.openedLive' : 'accounts.wizard.openedDemo', {'login': res.login}),
        description: '${g.name} · ${t.dyn('accounts.mode.${g.mode}', fallback: modeLabel(g.mode))} · 1:$_leverage',
      );
    } on ApiException catch (e) {
      if (!mounted) return;
      accountToast(context, NotificationKind.error, t('accounts.wizard.openFailed'), description: localizeError(e, t));
      if (e.field == 'password' || e.field == 'investorPassword') setState(() => _step = WizardStep.configure);
    } catch (_) {
      if (mounted) accountToast(context, NotificationKind.error, t('accounts.wizard.openFailed'), description: t('common.errorRetry'));
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final groupsQ = ref.watch(groupsProvider);
    final accounts = ref.watch(accountsOnceProvider).value ?? const <EngineAccount>[];
    final demoOn = ref.watch(configProvider).flag('demo_accounts', fallback: true);
    // the options intro's state, loaded as soon as the Options product is chosen
    final suit = _product == 'options' ? ref.watch(suitabilityProvider) : null;
    final suitLoading = suit != null && !suit.hasValue && !suit.hasError;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    final header = [
      KPageHeader(title: t('accounts.wizard.title'), subtitle: Text(t('accounts.wizard.subtitle'))),
      const SizedBox(height: 14),
      Align(
        alignment: AlignmentDirectional.centerStart,
        child: KButton(
          label: t('accounts.list.myAccounts'),
          icon: rtl ? LucideIcons.arrowRight : LucideIcons.arrowLeft,
          variant: KButtonVariant.surface,
          onPressed: () => context.go('/accounts'),
        ),
      ),
      const SizedBox(height: 18),
    ];

    if (groupsQ.hasError && !groupsQ.hasValue) {
      return KPageScroll(
        controller: _scroll,
        children: [
          KPageHeader(title: t('accounts.wizard.title')),
          const SizedBox(height: 18),
          KCard(
            child: KEmptyState(
              art: KIllustrationName.connectionLost,
              title: t('accounts.wizard.unavailableTitle'),
              text: t('accounts.wizard.unavailableText'),
              action: KButton(
                label: t('common.retry'),
                icon: LucideIcons.rotateCw,
                variant: KButtonVariant.surface,
                onPressed: () => ref.invalidate(groupsProvider),
              ),
            ),
          ),
        ],
      );
    }

    final groups = groupsQ.value ?? const <EngineGroup>[];
    _boot(groups);
    // ?product= / ?group=: past the Product step (to the intro first while it isn't accepted) once that is known
    if (_jump != null && groupsQ.hasValue && !suitLoading) {
      _step = _needsIntro(suit) ? WizardStep.intro : _jump!;
      _jump = null;
    }
    final available = groups.where((g) => groupOffers(g, _kind, _product)).toList();
    final g = groups.where((x) => x.code == _group && groupOffers(x, _kind, _product)).firstOrNull ?? available.firstOrNull;
    if (_booted && g != null && g.code != _group) {
      // keep the chosen group valid for the chosen product and account kind
      _group = g.code;
      _leverage = g.leverages.contains(_leverage) ? _leverage : g.defaultLeverage;
    }
    final full = g != null && groupUsed(accounts, g, _kind) >= g.maxAccountsPerUser;
    final pwOk = !_ownPassword || (livePasswordOk(_password) && _password == _confirm);
    final intro = suit?.value;
    final canNext = switch (_step) {
      WizardStep.product => !suitLoading,
      WizardStep.intro => _understood && intro?.disclosure != null && !optionsTermsReadOnly(ref) && !_accepting,
      WizardStep.type => g != null && !full,
      WizardStep.configure => g != null && g.leverages.contains(_leverage) && pwOk,
      WizardStep.confirm => _agree && pwOk,
      _ => true,
    };

    Widget body;
    if (!groupsQ.hasValue) {
      body = const KSkeleton(height: 420, radius: 24);
    } else {
      body = switch (_step) {
        WizardStep.product => _productStep(),
        WizardStep.intro => _introStep(suit),
        WizardStep.account => _kindStep(groups, demoOn),
        WizardStep.type => _typeStep(available, accounts, g),
        WizardStep.configure when g != null => _configureStep(g),
        WizardStep.confirm when g != null => _reviewStep(g),
        WizardStep.done when _created != null && g != null => _CreatedCard(res: _created!, ownPassword: _ownPassword, g: g),
        _ => const SizedBox.shrink(),
      };
    }

    final steps = [
      for (final s in kWizardSteps.keys)
        if (_optionsOn || s != WizardStep.product) s,
    ];
    final current = steps.indexOf(_step == WizardStep.intro ? WizardStep.product : _step);
    final first = _flow(suit).first == _step;
    return KPageScroll(
      controller: _scroll,
      children: [
        ...header,
        KCard(
          padding: const EdgeInsets.symmetric(horizontal: 18, vertical: 16),
          child: KStepIndicator(steps: [for (var i = 0; i < steps.length; i++) i == current ? t(kWizardSteps[steps[i]]!) : ''], current: current),
        ),
        const SizedBox(height: 16),
        AnimatedSwitcher(
          duration: const Duration(milliseconds: 220),
          child: KeyedSubtree(key: ValueKey(_step), child: body),
        ),
        if (_step != WizardStep.done && groupsQ.hasValue) ...[
          const SizedBox(height: 16),
          Row(
            children: [
              KButton(
                label: t('common.back'),
                icon: rtl ? LucideIcons.arrowRight : LucideIcons.arrowLeft,
                variant: KButtonVariant.ghost,
                onPressed: first || _busy || _accepting ? null : () => _go(-1, suit),
              ),
              const Spacer(),
              if (_step != WizardStep.confirm)
                KButton(
                  label: t('common.continue'),
                  trailingIcon: rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight,
                  size: KButtonSize.lg,
                  loading: (_step == WizardStep.product && suitLoading) || _accepting,
                  onPressed: !canNext ? null : (_step == WizardStep.intro ? () => _acceptIntro(intro!) : () => _go(1, suit)),
                )
              else
                Flexible(
                  child: KButton(
                    label: _busy ? t('accounts.wizard.opening') : (_kind == AccountKind.live ? t('accounts.wizard.openLive') : t('accounts.wizard.openDemo')),
                    trailingIcon: _busy ? null : LucideIcons.check,
                    size: KButtonSize.lg,
                    loading: _busy,
                    onPressed: canNext && !_busy && g != null ? () => _create(g) : null,
                  ),
                ),
            ],
          ),
        ],
      ],
    );
  }

  /* ---------------------------------------------------------------- steps */

  Widget _productStep() {
    final t = context.t;
    return _StepCard(
      title: t('accounts.wizard.productTitle'),
      subtitle: t('accounts.wizard.productSubtitle'),
      child: Column(
        children: [
          for (final p in ['cfd', 'options']) ...[
            if (p == 'options') const SizedBox(height: 14),
            _ProductCard(
              product: p,
              selected: _product == p,
              onSelect: () => setState(() {
                _product = p;
                if (p == 'cfd') _understood = false;
              }),
            ),
          ],
        ],
      ),
    );
  }

  /// The options intro before a first Options account (the Options page's card; Continue records the acceptance).
  Widget _introStep(AsyncValue<Suitability>? suit) {
    final t = context.t;
    final data = suit?.value;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KNotice(icon: LucideIcons.sparkles, text: t('accounts.wizard.introFirst')),
        const SizedBox(height: 14),
        if (data == null)
          const KSkeletonCard(height: 360, lines: 6)
        else
          OptionsIntroCard(
            data: data,
            readOnly: optionsTermsReadOnly(ref),
            understood: _understood,
            busy: _accepting,
            onUnderstood: (v) => setState(() => _understood = v),
            onTerms: () => showOptionsTerms(context, data),
          ),
      ],
    );
  }

  Widget _kindStep(List<EngineGroup> groups, bool demoOn) {
    final t = context.t;
    final demoRef = groups.where((x) => groupOffers(x, AccountKind.demo, _product)).firstOrNull;
    return _StepCard(
      title: t('accounts.wizard.chooseTitle'),
      subtitle: t('accounts.wizard.chooseSubtitle'),
      child: Column(
        children: [
          _KindCard(kind: AccountKind.live, selected: _kind == AccountKind.live, onSelect: () => setState(() => _kind = AccountKind.live)),
          if (demoOn) ...[
            const SizedBox(height: 14),
            _KindCard(
              kind: AccountKind.demo,
              selected: _kind == AccountKind.demo,
              demoGroup: demoRef,
              onSelect: () => setState(() => _kind = AccountKind.demo),
            ),
          ],
        ],
      ),
    );
  }

  Widget _typeStep(List<EngineGroup> available, List<EngineAccount> accounts, EngineGroup? g) {
    final t = context.t;
    return _StepCard(
      title: t('accounts.wizard.pickTitle'),
      subtitle: t(_kind == AccountKind.live ? 'accounts.wizard.pickSubtitleLive' : 'accounts.wizard.pickSubtitleDemo', {'count': available.length}),
      child: Column(
        children: [
          if (available.isEmpty) KEmptyState(compact: true, icon: LucideIcons.layers, title: t('accounts.wizard.noGroups')),
          for (final x in available) ...[
            EngineGroupCard(group: x, kind: _kind.name, used: groupUsed(accounts, x, _kind), selected: g?.code == x.code, onSelect: () => _pickGroup(x)),
            const SizedBox(height: 14),
          ],
        ],
      ),
    );
  }

  Widget _configureStep(EngineGroup g) {
    final t = context.t;
    final k = context.k;
    final mode = t.dyn('accounts.mode.${g.mode}', fallback: modeLabel(g.mode));
    final balances = {...kDemoBalances.map((b) => b.toDouble()), g.demoInitialBalance}.toList()..sort();
    return _StepCard(
      title: t('accounts.wizard.configureTitle'),
      subtitle: '${g.name} · $mode · ${_kind == AccountKind.live ? t('common.live') : t('common.demo')}',
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // leverage doesn't apply to options (premiums are paid in cash): an Options type keeps its one value
          if (!g.isOptions) ...[
            Text(t('accounts.label.leverage'), style: context.text.label.copyWith(color: k.fg2)),
            const SizedBox(height: 2),
            Text(t('accounts.wizard.leverageHint'), style: context.text.footnote.copyWith(color: k.fg3)),
            const SizedBox(height: 10),
            LeveragePills(values: g.leverages, selected: _leverage, onSelect: (l) => setState(() => _leverage = l)),
          ],
          if (!g.isOptions && _leverage >= 1000) ...[
            const SizedBox(height: 8),
            Row(
              children: [
                Icon(LucideIcons.info, size: 14, color: k.warn),
                const SizedBox(width: 6),
                Expanded(
                  child: Text(t('accounts.wizard.highLeverage'), style: context.text.footnote.copyWith(color: k.warn)),
                ),
              ],
            ),
          ],
          if (_kind == AccountKind.demo) ...[
            if (!g.isOptions) const SizedBox(height: 22),
            Text(t('accounts.wizard.startingBalance'), style: context.text.label.copyWith(color: k.fg2)),
            const SizedBox(height: 10),
            TileGrid(
              children: [
                for (final b in balances)
                  ChoiceBox(
                    selected: _demoBalance == b,
                    tone: k.gold,
                    onTap: () => setState(() => _demoBalance = b),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          wizardMoney(b, g.cent),
                          textDirection: TextDirection.ltr,
                          style: context.text.figure.copyWith(fontSize: 16.5, color: _demoBalance == b ? k.gold : k.fg),
                        ),
                        Text(
                          t('accounts.wizard.virtualFunds'),
                          style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                        ),
                      ],
                    ),
                  ),
              ],
            ),
            const SizedBox(height: 8),
            Text(
              t('accounts.wizard.refillNote', {'count': g.demoRefillsPerDay, 'days': g.demoExpiryDays}),
              style: context.text.footnote.copyWith(color: k.fg3),
            ),
          ],
          if (!g.isOptions || _kind == AccountKind.demo) const SizedBox(height: 22),
          KTextField(
            label: t('accounts.label.nickname'),
            hint: Text(t('accounts.wizard.nicknameHint'), style: context.text.footnote.copyWith(color: k.fg3)),
            controller: _nickname,
            placeholder: t('accounts.wizard.nicknamePlaceholder'),
            inputFormatters: [LengthLimitingTextInputFormatter(32)],
          ),
          const SizedBox(height: 14),
          _ReadOnlyField(
            label: t('accounts.label.accountCurrency'),
            icon: LucideIcons.wallet,
            value: g.cent ? t('accounts.currency.uscLong') : t('accounts.currency.usdLong'),
          ),
          const SizedBox(height: 18),
          RowBox(
            children: [
              Row(
                children: [
                  Container(
                    width: 40,
                    height: 40,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: k.surface3,
                      border: Border.all(color: k.line),
                    ),
                    child: Icon(LucideIcons.keyRound, size: 16, color: k.fg2),
                  ),
                  const SizedBox(width: 12),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          t('accounts.wizard.ownPassword'),
                          style: context.text.label.copyWith(color: k.fg, fontSize: 14, fontWeight: FontWeight.w600),
                        ),
                        const SizedBox(height: 2),
                        Text(t('accounts.wizard.ownPasswordHint'), style: context.text.footnote.copyWith(color: k.fg3)),
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),
                  KSwitch(value: _ownPassword, semanticLabel: t('accounts.wizard.ownPasswordToggle'), onChanged: (v) => setState(() => _ownPassword = v)),
                ],
              ),
              if (_ownPassword) ...[
                const SizedBox(height: 16),
                PasswordInput(label: t('accounts.label.tradingPassword'), value: _password, generate: true, onChanged: (v) => setState(() => _password = v)),
                const SizedBox(height: 10),
                PasswordRules(password: _password),
                const SizedBox(height: 12),
                PasswordInput(
                  label: t('accounts.label.confirmPassword'),
                  value: _confirm,
                  placeholder: t('accounts.wizard.repeatPassword'),
                  error: _confirm.isNotEmpty && _confirm != _password ? t('accounts.wizard.passwordsMismatch') : null,
                  onChanged: (v) => setState(() => _confirm = v),
                ),
              ],
            ],
          ),
        ],
      ),
    );
  }

  Widget _reviewStep(EngineGroup g) {
    final t = context.t;
    final k = context.k;
    final mode = t.dyn('accounts.mode.${g.mode}', fallback: modeLabel(g.mode));
    Widget point(String text) => Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(top: 2),
            child: Icon(LucideIcons.check, size: 14, color: k.up),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: Text(text, style: context.text.callout.copyWith(color: k.fg2)),
          ),
        ],
      ),
    );
    final start = _kind == AccountKind.demo ? wizardMoney(_demoBalance, g.cent) : (g.cent ? 'USC 0.00' : r'$0.00');
    return _StepCard(
      title: t('accounts.wizard.reviewTitle'),
      subtitle: t('accounts.wizard.reviewSubtitle'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          TileGrid(
            children: [
              StatTile(label: t('common.account'), child: Text(_kind == AccountKind.live ? t('common.live') : t('common.demo'))),
              StatTile(label: t('common.type'), child: Text(g.name)),
              if (g.isOptions)
                StatTile(label: t('accounts.wizard.step.product'), child: Text(t('accounts.product.options')))
              else
                StatTile(
                  label: t('accounts.label.leverage'),
                  child: Text(levLabel(_leverage), textDirection: TextDirection.ltr),
                ),
              StatTile(
                label: t('accounts.label.startBalance'),
                child: Text(start, textDirection: TextDirection.ltr),
              ),
            ],
          ),
          const SizedBox(height: 18),
          point(_ownPassword ? t('accounts.wizard.review.ownPassword') : t('accounts.wizard.review.generated')),
          if (_kind == AccountKind.live)
            point(
              '${t('accounts.wizard.review.zeroBalance')} ${g.minDeposit > 0 ? t('accounts.wizard.review.minDeposit', {'amount': '\$${Fmt.number(g.minDeposit, 0)}'}) : ''}'
                  .trim(),
            ),
          point(t('accounts.wizard.review.fixed', {'mode': mode, 'currency': g.cent ? 'USC' : 'USD'})),
          const SizedBox(height: 8),
          KCheckRow(
            value: _agree,
            onChanged: (v) => setState(() => _agree = v),
            child: Text(
              _kind == AccountKind.live
                  ? t(_product == 'options' ? 'accounts.wizard.agreeLiveOptions' : 'accounts.wizard.agreeLive')
                  : t('accounts.wizard.agreeDemo'),
              style: context.text.callout.copyWith(color: k.fg2),
            ),
          ),
        ],
      ),
    );
  }
}

/// A read-only input (web Input readOnly, faded): the account currency.
class _ReadOnlyField extends StatelessWidget {
  const _ReadOnlyField({required this.label, required this.icon, required this.value});
  final String label;
  final IconData icon;
  final String value;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Padding(
          padding: const EdgeInsets.only(bottom: 6),
          child: Text(label, style: context.text.label.copyWith(color: k.fg2)),
        ),
        Opacity(
          opacity: 0.8,
          child: Container(
            height: KSize.field,
            padding: const EdgeInsets.symmetric(horizontal: 13),
            decoration: BoxDecoration(
              color: k.surface2,
              borderRadius: BorderRadius.circular(14),
              border: Border.all(color: k.line),
            ),
            child: Row(
              children: [
                Icon(icon, size: 17, color: k.fg3),
                const SizedBox(width: 10),
                Expanded(
                  child: Text(value, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.body.copyWith(fontSize: 14.5)),
                ),
              ],
            ),
          ),
        ),
      ],
    );
  }
}

/// A wizard step's card (web Card + CardHeader).
class _StepCard extends StatelessWidget {
  const _StepCard({required this.title, required this.subtitle, required this.child});
  final String title, subtitle;
  final Widget child;

  @override
  Widget build(BuildContext context) => KCard(
    padding: const EdgeInsets.fromLTRB(16, 20, 16, 20),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KCardHeader(title: title, subtitle: subtitle),
        const SizedBox(height: 18),
        child,
      ],
    ),
  );
}

/// CFD account / Options account choice (web open-account.tsx ProductCard): the product as a card face (orange CFD,
/// plum Options) with its markets, the white tick while chosen, the short text under it.
class _ProductCard extends StatelessWidget {
  const _ProductCard({required this.product, required this.selected, required this.onSelect});
  final String product;
  final bool selected;
  final VoidCallback onSelect;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final options = product == 'options';
    final title = options ? t('accounts.product.options') : t('accounts.product.cfd');
    return KPressable(
      key: ValueKey('wizard-product-$product'),
      onTap: onSelect,
      pressedScale: 0.99,
      semanticLabel: title,
      child: _ChoiceFrame(
        selected: selected,
        children: [
          KCardFace(
            finish: options ? KCardFinish.options : KCardFinish.standard,
            name: title,
            badge: options ? t('accounts.product.chipOptions') : 'CFD',
            left: (
              t.dyn('accounts.product.markets', fallback: 'Markets'),
              options
                  ? t.dyn('accounts.product.optionsMarkets', fallback: 'FX · Gold · Oil')
                  : t.dyn('accounts.product.cfdMarkets', fallback: 'FX · Metals · Crypto'),
            ),
            selected: selected,
          ),
          const SizedBox(height: 14),
          Text(options ? t('accounts.product.optionsText') : t('accounts.product.cfdText'), style: context.text.callout.copyWith(color: k.fg2)),
        ],
      ),
    );
  }
}

/// A choice's frame (web k-card + the selected ember ring).
class _ChoiceFrame extends StatelessWidget {
  const _ChoiceFrame({required this.selected, required this.children});
  final bool selected;
  final List<Widget> children;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return AnimatedContainer(
      duration: const Duration(milliseconds: 200),
      padding: const EdgeInsets.fromLTRB(12, 12, 12, 18),
      decoration: BoxDecoration(
        color: k.cardBg,
        borderRadius: BorderRadius.circular(24),
        border: Border.all(color: selected ? k.ember.withValues(alpha: 0.7) : k.cardBorder),
        boxShadow: selected ? [BoxShadow(color: k.ember.withValues(alpha: 0.18), spreadRadius: 4)] : null,
      ),
      child: Column(crossAxisAlignment: CrossAxisAlignment.stretch, children: children),
    );
  }
}

/// Live / Demo choice (web KindCard): a card face (orange live, black demo) with its money, the text and the points.
class _KindCard extends StatelessWidget {
  const _KindCard({required this.kind, required this.selected, required this.onSelect, this.demoGroup});
  final AccountKind kind;
  final bool selected;
  final VoidCallback onSelect;
  final EngineGroup? demoGroup;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final live = kind == AccountKind.live;
    final tone = live ? k.ember : k.gold;
    final demoMoney = demoGroup != null ? '\$${Fmt.number(demoGroup!.demoInitialBalance, 0)}' : r'$10,000';
    final points = live
        ? [t('accounts.kind.live.point1'), t('accounts.kind.live.point2'), t('accounts.kind.live.point3')]
        : [
            t('accounts.kind.demo.virtualFunds', {'amount': demoMoney}),
            t('accounts.kind.demo.refill', {'count': demoGroup?.demoRefillsPerDay ?? 3}),
            t('accounts.kind.demo.expires', {'days': demoGroup?.demoExpiryDays ?? 10}),
          ];
    return KPressable(
      onTap: onSelect,
      pressedScale: 0.99,
      semanticLabel: live ? t('accounts.kind.liveTitle') : t('accounts.kind.demoTitle'),
      child: _ChoiceFrame(
        selected: selected,
        children: [
          KCardFace(
            finish: live ? KCardFinish.standard : KCardFinish.pro,
            name: live ? t('accounts.kind.liveTitle') : t('accounts.kind.demoTitle'),
            badge: live ? t('accounts.badge.live') : t('accounts.badge.demo'),
            left: (t.dyn('accounts.kind.money', fallback: 'Money'), live ? t.dyn('accounts.kind.real', fallback: 'Real · USDT') : demoMoney),
            selected: selected,
          ),
          const SizedBox(height: 14),
          Text(live ? t('accounts.kind.liveText') : t('accounts.kind.demoText'), style: context.text.callout.copyWith(color: k.fg2)),
          const SizedBox(height: 12),
          for (final p in points)
            Padding(
              padding: const EdgeInsets.only(bottom: 7),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Padding(
                    padding: const EdgeInsets.only(top: 2),
                    child: Icon(LucideIcons.check, size: 14, color: tone),
                  ),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(p, style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13)),
                  ),
                ],
              ),
            ),
        ],
      ),
    );
  }
}

/// The done step: the new account and its credentials, shown once (web Created).
class _CreatedCard extends StatelessWidget {
  const _CreatedCard({required this.res, required this.ownPassword, required this.g});
  final OpenResult res;
  final bool ownPassword;
  final EngineGroup g;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final a = res.account;
    final login = '${res.login}';
    final server = a.server;
    final text = a.live
        ? t('accounts.created.liveText')
        : '${t('accounts.created.demoText', {'amount': '${a.cent ? 'USC ' : r'$'}${Fmt.number(a.balance)}'})}${a.demo != null ? ' ${t('accounts.created.demoExpires', {'days': a.demo!['expiryDays'] ?? 0})}' : ''}';
    Future<void> copyAll() async {
      final all = [
        '${t('accounts.label.login')}: $login',
        '${t('accounts.label.server')}: $server',
        if (res.password != null) '${t('accounts.label.tradingPassword')}: ${res.password}',
        if (res.investorPassword != null) '${t('accounts.label.investorPassword')}: ${res.investorPassword}',
      ].join('\n');
      try {
        await Clipboard.setData(ClipboardData(text: all));
        KHaptics.success();
        if (context.mounted) accountToast(context, NotificationKind.success, t('accounts.created.copied'), description: t('accounts.created.copiedDesc'));
      } catch (_) {
        if (context.mounted) accountToast(context, NotificationKind.error, t('accounts.created.copyFailed'));
      }
    }

    return KCard(
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: Container(
              width: 56,
              height: 56,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                color: k.upSoft,
                border: Border.all(color: k.up.withValues(alpha: 0.3)),
              ),
              child: Icon(LucideIcons.check, size: 28, color: k.up),
            ),
          ),
          const SizedBox(height: 18),
          Text(t('accounts.created.title'), style: context.text.largeTitle.copyWith(fontSize: 25)),
          const SizedBox(height: 8),
          Text(text, style: context.text.body.copyWith(color: k.fg2)),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              KChip(label: a.live ? t('accounts.badge.live') : t('accounts.badge.demo'), tone: a.live ? KChipTone.ember : KChipTone.gold),
              KChip(label: '${g.name} · ${t.dyn('accounts.mode.${a.mode}', fallback: modeLabel(a.mode))}'),
              KChip(label: levLabel(a.leverage)),
              if (a.cent) const KChip(label: 'USC', tone: KChipTone.gold),
            ],
          ),
          const SizedBox(height: 20),
          Wrap(
            spacing: 8,
            runSpacing: 10,
            children: [
              TradeButton(account: a, size: KButtonSize.lg, label: t('accounts.created.openInTrader')),
              if (a.live) FundButton(account: a, size: KButtonSize.lg),
              KButton(
                label: t('accounts.created.viewAccount'),
                variant: KButtonVariant.surface,
                size: KButtonSize.lg,
                onPressed: () => context.push('/accounts/${a.login}'),
              ),
            ],
          ),
          const SizedBox(height: 22),
          Container(
            padding: const EdgeInsets.all(16),
            decoration: BoxDecoration(
              color: k.surface2.withValues(alpha: 0.6),
              borderRadius: BorderRadius.circular(20),
              border: Border.all(color: k.line),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Row(
                  children: [
                    Icon(LucideIcons.keyRound, size: 16, color: k.fg3),
                    const SizedBox(width: 8),
                    Expanded(child: Text(t('accounts.created.credentials'), style: context.text.headline)),
                    KButton(
                      label: t('accounts.created.copyAll'),
                      icon: LucideIcons.copy,
                      variant: KButtonVariant.surface,
                      size: KButtonSize.sm,
                      onPressed: copyAll,
                    ),
                  ],
                ),
                const SizedBox(height: 14),
                SecretField(label: t('accounts.label.login'), value: login),
                const SizedBox(height: 12),
                SecretField(label: t('accounts.label.server'), value: server, hint: 'GMT+3 / GMT+2'),
                if (res.password != null) ...[
                  const SizedBox(height: 12),
                  SecretField(label: t('accounts.label.tradingPassword'), value: res.password!, secret: true, hint: t('accounts.hint.fullAccess')),
                ],
                if (res.investorPassword != null) ...[
                  const SizedBox(height: 12),
                  SecretField(label: t('accounts.label.investorPassword'), value: res.investorPassword!, secret: true, hint: t('accounts.hint.readOnly')),
                ],
                const SizedBox(height: 14),
                Container(
                  padding: const EdgeInsets.all(12),
                  decoration: BoxDecoration(
                    color: k.warnSoft,
                    borderRadius: BorderRadius.circular(14),
                    border: Border.all(color: k.warn.withValues(alpha: 0.25)),
                  ),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Icon(LucideIcons.triangleAlert, size: 16, color: k.warn),
                      const SizedBox(width: 8),
                      Expanded(
                        child: KRichText(
                          t('accounts.created.onceWarning'),
                          style: context.text.footnote.copyWith(color: k.fg2),
                          tags: const {'b': KTag()},
                        ),
                      ),
                    ],
                  ),
                ),
                if (ownPassword) ...[
                  const SizedBox(height: 10),
                  Text(t('accounts.created.ownPasswordNote'), style: context.text.footnote.copyWith(color: k.fg3)),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

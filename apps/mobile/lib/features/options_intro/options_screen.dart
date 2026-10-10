// Options (web apps/crm/app/(app)/options/page.tsx -> components/options/live.tsx + ui.tsx, phone order):
//   1 header: Kalks FX Options (+ "Ready to trade" once accepted)
//   2 hero: eyebrow, title, text; accepted: "Trade options in Kalks Trader" + How options work; else Get started
//     (scrolls to the intro) + Options course; the 13 underlyings by class; four features
//   3 not accepted yet: "Options in three simple ideas" (Buy a Call / Buy a Put / limited risk, payoff sketches),
//     "I understand how options work", the terms link, "Start trading options" (records the acceptance through
//     suitability, then opens Kalks Trader on the chosen Options account; none -> the open-account wizard on the
//     Options product). The open-account wizard shows the same intro before a first Options account.
// Options trade in an Options account only (the account's product): the Trade button lists those, and offers
// "Open an Options account" when the client has none.
//   4 How Kalks FX Options work (facts) and New to options? (the Academy course)
//   sheets: the full terms (key points translated + the binding English text), How options work
// API: GET suitability/options · POST suitability/options/accept {version}; accounts from trading/accounts.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/auth/auth_controller.dart';
import '../../core/format/format.dart';
import '../../core/models/account.dart';
import '../../core/notifications/notifications.dart';
import '../../data/client_data.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import '../markets/instruments.dart';
import 'widgets/markdown_text.dart';

/// The Academy course on options.
const String kOptionsCourseHref = '/academy/phase/phase-9';

class Disclosure {
  const Disclosure({required this.version, required this.title, required this.bodyMd, required this.publishedAt});
  final int version;
  final String title, bodyMd;
  final DateTime? publishedAt;
}

class Suitability {
  const Suitability({required this.disclosure, required this.accepted, required this.acceptedVersion, required this.acceptedAt, required this.eligible});
  final Disclosure? disclosure;
  final bool accepted, eligible;
  final int? acceptedVersion;
  final DateTime? acceptedAt;

  factory Suitability.fromJson(Map<String, dynamic> j) {
    final d = j['disclosure'] is Map ? (j['disclosure'] as Map).cast<String, dynamic>() : null;
    return Suitability(
      disclosure: d == null
          ? null
          : Disclosure(
              version: (d['version'] as num?)?.toInt() ?? 1,
              title: '${d['title'] ?? ''}',
              bodyMd: '${d['bodyMd'] ?? ''}',
              publishedAt: DateTime.tryParse('${d['publishedAt']}'),
            ),
      accepted: j['disclosureAccepted'] == true,
      acceptedVersion: (j['acceptedVersion'] as num?)?.toInt(),
      acceptedAt: DateTime.tryParse('${j['acceptedAt']}'),
      eligible: j['eligible'] == true,
    );
  }
}

/// `GET suitability/options` (the web refetches when the tab comes back; here on pull to refresh and resume).
final suitabilityProvider = FutureProvider.autoDispose<Suitability>(
  (ref) async => Suitability.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('suitability/options')),
);

/// Accounts options can be traded on: Options accounts that are active, not prop, not a copy / PAMM / MAM account;
/// default first, live first.
List<EngineAccount> optionsAccounts(List<EngineAccount> all) {
  bool flavor(EngineAccount a) {
    final g = a.group.toLowerCase();
    return ['copy', 'pamm', 'mam'].any((c) => g == c || g.startsWith('$c-'));
  }

  final list = all.where((a) => a.isOptions && a.status == 'active' && !a.prop && !flavor(a)).toList()
    ..sort((a, b) {
      if (a.isDefault != b.isDefault) return a.isDefault ? -1 : 1;
      if (a.live != b.live) return a.live ? -1 : 1;
      return a.login.compareTo(b.login);
    });
  return list;
}

/// View-only logins and staff sessions (even full access) can't accept the options terms for the client.
bool optionsTermsReadOnly(WidgetRef ref) {
  final me = ref.read(meProvider);
  return me == null || me.readOnly || me.raw['impersonation'] != null;
}

/// Records the acceptance of the options terms `version` (`POST suitability/options/accept`); false (after its own
/// banner) when it failed.
Future<bool> acceptOptionsTerms(WidgetRef ref, T t, int version) async {
  final toast = ref.read(notificationsProvider.notifier);
  try {
    await ref.read(apiProvider).post<Map<String, dynamic>>('suitability/options/accept', body: {'version': version});
    KHaptics.success();
    toast.toast(NotificationKind.success, t('options.intro.toastStarted'));
    ref.invalidate(suitabilityProvider);
    return true;
  } on ApiException catch (e) {
    if (e.code == 'disclosure_outdated') {
      toast.toast(NotificationKind.warning, t('options.intro.toastUpdated'));
      ref.invalidate(suitabilityProvider);
    } else {
      toast.toast(NotificationKind.error, t('options.intro.toastFailed'), description: localizeError(e, t));
    }
    return false;
  }
}

class OptionsScreen extends ConsumerStatefulWidget {
  const OptionsScreen({super.key});

  @override
  ConsumerState<OptionsScreen> createState() => _OptionsScreenState();
}

class _OptionsScreenState extends ConsumerState<OptionsScreen> {
  final _introKey = GlobalKey();
  bool _understood = false;
  bool _busy = false;

  bool get _readOnly => optionsTermsReadOnly(ref);

  /// Records the acceptance of `version`; false (after its own banner) when it failed.
  Future<bool> _accept(int version) => acceptOptionsTerms(ref, context.t, version);

  /// Opens Kalks Trader on an Options account: one directly, several through a choice, none -> the open-account
  /// wizard on the Options product (the web's TraderButton). `before` runs first and must succeed.
  Future<void> _trade({Future<bool> Function()? before}) async {
    final t = context.t;
    final router = GoRouter.of(context);
    final acc = ref.read(accountsProvider);
    final accounts = acc.value == null ? null : optionsAccounts(acc.value!);
    EngineAccount? pick;
    if (accounts != null && accounts.length > 1) {
      pick = await showKActionSheet<EngineAccount>(
        context,
        title: t('options.trade.chooseAccount'),
        actions: [
          for (final a in accounts)
            KAction(
              label: '#${a.login} · ${a.name.isNotEmpty ? a.name : a.groupName} · ${t(a.live ? 'options.trade.live' : 'options.trade.demo')}',
              value: a,
              icon: LucideIcons.candlestickChart,
            ),
        ],
      );
      if (pick == null) return;
    } else if (accounts != null && accounts.length == 1) {
      pick = accounts.first;
    }
    setState(() => _busy = true);
    try {
      if (before != null && !await before()) return;
      if (accounts != null && accounts.isEmpty) {
        router.go('/accounts/new?product=options');
      } else if (pick != null) {
        unawaited(router.push('/trader?login=${pick.login}&mode=options'));
      } else {
        // the accounts couldn't be listed: the terminal signs in by itself
        unawaited(router.push('/trader?mode=options'));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final s = ref.watch(suitabilityProvider);
    final data = s.value;
    final eligible = data?.eligible ?? false;
    final acc = ref.watch(accountsProvider);
    final accounts = acc.value == null ? null : optionsAccounts(acc.value!);
    final readOnly = _readOnly;
    final accountsLoading = accounts == null && !acc.hasError;

    final noAccount = accounts != null && accounts.isEmpty;
    final tradeLabel = noAccount ? t('options.account.open') : t('options.trade.cta');

    final hero = pageHero(context, ref, path: '/options', title: t('options.page.title'), lead: t('options.page.subtitle'));
    return KPageScroll(
      onRefresh: () async {
        ref
          ..invalidate(suitabilityProvider)
          ..invalidate(accountsProvider);
        await ref.read(suitabilityProvider.future).then((_) {}, onError: (Object _) {});
      },
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        // 1. header (on the photo, or the page header for a white-label broker)
        if (hero == null) KPageHeader(title: t('options.page.title'), subtitle: Text(t('options.page.subtitle'))),
        if (eligible) ...[
          if (hero == null) const SizedBox(height: 10),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: KChip(label: t('options.page.statusReady'), tone: KChipTone.up, dot: true),
          ),
        ],
        const SizedBox(height: 20),
        // 2. hero
        KCard(
          hot: true,
          padding: EdgeInsets.zero,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 22, 20, 20),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    KChip(label: t('options.hero.eyebrow').toUpperCase(), tone: KChipTone.ember, small: true),
                    const SizedBox(height: 12),
                    Text(t('options.hero.title'), style: context.text.title1.copyWith(fontSize: 24, fontWeight: FontWeight.w500, height: 1.15)),
                    const SizedBox(height: 10),
                    Text(t('options.hero.text'), style: context.text.body.copyWith(color: k.fg2, height: 1.55)),
                    const SizedBox(height: 20),
                    if (eligible) ...[
                      if (noAccount) ...[
                        KNotice(
                          key: const ValueKey('options-no-account'),
                          icon: LucideIcons.wallet,
                          title: t('options.account.noneTitle'),
                          text: t('options.account.noneText'),
                        ),
                        const SizedBox(height: 14),
                      ],
                      KButton(
                        label: tradeLabel,
                        icon: noAccount ? LucideIcons.plus : LucideIcons.candlestickChart,
                        size: KButtonSize.lg,
                        loading: _busy || accountsLoading,
                        onPressed: _trade,
                      ),
                      const SizedBox(height: 6),
                      KPressable(
                        onTap: () => _showHowItWorks(context, data!),
                        child: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            Icon(LucideIcons.lightbulb, size: 16, color: k.fg2),
                            const SizedBox(width: 6),
                            Text(
                              t('options.hero.howItWorks'),
                              style: context.text.label.copyWith(color: k.fg2, fontWeight: FontWeight.w600),
                            ),
                          ],
                        ),
                      ),
                      const SizedBox(height: 6),
                      Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Padding(
                            padding: const EdgeInsets.only(top: 1),
                            child: Icon(LucideIcons.circleCheck, size: 16, color: k.up),
                          ),
                          const SizedBox(width: 8),
                          Expanded(
                            child: Text(
                              t(noAccount ? 'options.trade.noAccount' : 'options.trade.ready'),
                              style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13),
                            ),
                          ),
                        ],
                      ),
                    ] else
                      Wrap(
                        spacing: 8,
                        runSpacing: 8,
                        children: [
                          if (data != null)
                            KButton(
                              label: t('options.hero.start'),
                              trailingIcon: Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight,
                              onPressed: () {
                                final ctx = _introKey.currentContext;
                                if (ctx != null) Scrollable.ensureVisible(ctx, duration: const Duration(milliseconds: 400), curve: Curves.easeOutCubic);
                              },
                            )
                          else if (!s.hasError)
                            const KSkeleton(width: 140, height: 40, radius: 20),
                          KButton(
                            label: t('options.page.learnCourse'),
                            icon: LucideIcons.graduationCap,
                            variant: KButtonVariant.surface,
                            onPressed: () => context.push(kOptionsCourseHref),
                          ),
                        ],
                      ),
                    const SizedBox(height: 20),
                    for (final g in _underlyings) ...[
                      Container(
                        width: double.infinity,
                        margin: const EdgeInsets.only(bottom: 10),
                        padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
                        decoration: BoxDecoration(
                          color: k.surface.withValues(alpha: 0.6),
                          borderRadius: BorderRadius.circular(16),
                          border: Border.all(color: k.line),
                        ),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(t('options.hero.class.${g.cls}'), style: context.text.label.copyWith(color: k.fg2)),
                            const SizedBox(height: 9),
                            Wrap(spacing: 6, runSpacing: 6, children: [for (final sym in g.symbols) _UnderlyingPill(sym)]),
                          ],
                        ),
                      ),
                    ],
                  ],
                ),
              ),
              Container(
                padding: const EdgeInsets.fromLTRB(20, 18, 20, 20),
                decoration: BoxDecoration(
                  border: Border(top: BorderSide(color: k.line.withValues(alpha: 0.7))),
                ),
                child: Column(
                  children: [
                    for (final (i, f) in _features.indexed) ...[if (i > 0) const SizedBox(height: 10), _Feature(icon: f.$1, title: t(f.$2), text: t(f.$3))],
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        // 3. the intro (until the terms are accepted)
        if (!eligible)
          KeyedSubtree(
            key: _introKey,
            child: data != null
                ? OptionsIntroCard(
                    data: data,
                    readOnly: readOnly,
                    understood: _understood,
                    busy: _busy,
                    onUnderstood: (v) => setState(() => _understood = v),
                    onTerms: () => _showTerms(context, data),
                    onStart: () => _trade(before: () => data.disclosure == null ? Future.value(false) : _accept(data.disclosure!.version)),
                  )
                : s.hasError
                ? KCard(
                    child: KEmptyState(
                      compact: true,
                      icon: LucideIcons.triangleAlert,
                      title: '${t('options.error.load')} ${errorText(s.error, t)}',
                      action: KButton(
                        label: t('options.error.retry'),
                        icon: LucideIcons.rotateCcw,
                        variant: KButtonVariant.surface,
                        size: KButtonSize.sm,
                        onPressed: () => ref.invalidate(suitabilityProvider),
                      ),
                    ),
                  )
                : const KSkeletonCard(height: 360, lines: 6),
          ),
        if (!eligible) const SizedBox(height: 16),
        // 4. facts + learn
        KCard(
          padding: const EdgeInsets.fromLTRB(20, 20, 20, 20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('options.facts.title'), icon: LucideIcons.scale),
              const SizedBox(height: 14),
              for (final f in _facts)
                Padding(
                  padding: const EdgeInsets.only(bottom: 10),
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Container(
                        width: 24,
                        height: 24,
                        decoration: BoxDecoration(
                          color: k.surface2,
                          shape: BoxShape.circle,
                          border: Border.all(color: k.line),
                        ),
                        child: Icon(f.$1, size: 13, color: k.fg3),
                      ),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Text(t(f.$2), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13, height: 1.45)),
                      ),
                    ],
                  ),
                ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        KCard(
          padding: const EdgeInsets.all(20),
          child: Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Container(
                width: 40,
                height: 40,
                decoration: BoxDecoration(
                  color: k.goldSoft,
                  shape: BoxShape.circle,
                  border: Border.all(color: k.gold.withValues(alpha: 0.3)),
                ),
                child: Icon(LucideIcons.graduationCap, size: 20, color: k.gold),
              ),
              const SizedBox(width: 14),
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Text(t('options.learn.title'), style: context.text.headline.copyWith(fontWeight: FontWeight.w500)),
                    const SizedBox(height: 4),
                    Text(t('options.learn.text'), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13, height: 1.45)),
                    const SizedBox(height: 12),
                    KButton(
                      label: t('options.learn.cta'),
                      trailingIcon: Directionality.of(context) == TextDirection.rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight,
                      variant: KButtonVariant.surface,
                      size: KButtonSize.sm,
                      onPressed: () => context.push(kOptionsCourseHref),
                    ),
                  ],
                ),
              ),
            ],
          ),
        ),
      ],
    );
  }
}

const List<({String cls, List<String> symbols})> _underlyings = [
  (cls: 'forex', symbols: ['EURUSD', 'GBPUSD', 'USDJPY', 'AUDUSD', 'USDCAD', 'USDCHF', 'NZDUSD', 'EURJPY', 'GBPJPY']),
  (cls: 'metals', symbols: ['XAUUSD', 'XAGUSD']),
  (cls: 'energies', symbols: ['USOIL', 'UKOIL']),
];

const List<(IconData, String, String)> _features = [
  (LucideIcons.layers2, 'options.hero.feature.underlyings.title', 'options.hero.feature.underlyings.text'),
  (LucideIcons.calendarClock, 'options.hero.feature.expiries.title', 'options.hero.feature.expiries.text'),
  (LucideIcons.coins, 'options.hero.feature.settlement.title', 'options.hero.feature.settlement.text'),
  (LucideIcons.scale, 'options.hero.feature.sides.title', 'options.hero.feature.sides.text'),
];

const List<(IconData, String)> _facts = [
  (LucideIcons.calendarClock, 'options.facts.style'),
  (LucideIcons.coins, 'options.facts.premium'),
  (LucideIcons.layers2, 'options.facts.contracts'),
  (LucideIcons.arrowUpRight, 'options.facts.close'),
  (LucideIcons.timer, 'options.facts.cutoff'),
  (LucideIcons.shieldAlert, 'options.facts.margin'),
  (LucideIcons.wallet, 'options.trade.cashOnly'),
];

const Map<String, String> _ccyFlag = {'EUR': 'eu', 'GBP': 'gb', 'USD': 'us', 'JPY': 'jp', 'AUD': 'au', 'CAD': 'ca', 'CHF': 'ch', 'NZD': 'nz'};

class _UnderlyingPill extends StatelessWidget {
  const _UnderlyingPill(this.symbol);
  final String symbol;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    Widget avatar;
    if (kInstrumentMap.containsKey(symbol)) {
      avatar = SymbolAvatar(symbol, size: 18);
    } else {
      // pairs the instrument list doesn't carry (NZDUSD): two flags
      final base = _ccyFlag[symbol.substring(0, 3)], quote = _ccyFlag[symbol.substring(3, 6)];
      avatar = SizedBox(
        width: 18 * 1.45,
        height: 18,
        child: Stack(
          textDirection: TextDirection.ltr,
          children: [
            if (base != null) Positioned(left: 0, child: KFlag(base, size: 18)),
            if (quote != null) Positioned(right: 0, child: KFlag(quote, size: 18)),
          ],
        ),
      );
    }
    return Container(
      height: 28,
      padding: const EdgeInsetsDirectional.only(start: 5, end: 10),
      decoration: BoxDecoration(
        color: k.surface2,
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: k.line),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          avatar,
          const SizedBox(width: 6),
          Text(symbol, style: context.text.mono(12, weight: FontWeight.w600)),
        ],
      ),
    );
  }
}

class _Feature extends StatelessWidget {
  const _Feature({required this.icon, required this.title, required this.text});
  final IconData icon;
  final String title, text;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
      decoration: BoxDecoration(
        color: k.surface.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 32,
                height: 32,
                decoration: BoxDecoration(
                  color: k.emberSoft,
                  shape: BoxShape.circle,
                  border: Border.all(color: k.ember.withValues(alpha: 0.3)),
                ),
                child: Icon(icon, size: 16, color: k.ember),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(
                  title,
                  style: context.text.label.copyWith(fontSize: 14, fontWeight: FontWeight.w600, color: k.fg),
                ),
              ),
            ],
          ),
          const SizedBox(height: 7),
          Text(text, style: context.text.footnote.copyWith(color: k.fg2, height: 1.4)),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ the three ideas */

enum _Idea { call, put, limited }

class _IdeaCards extends StatelessWidget {
  const _IdeaCards();

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    Widget card(_Idea idea, IconData icon, Color tone, String title, String text) => Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(14, 12, 14, 10),
      decoration: BoxDecoration(
        color: k.surface.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(16),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Container(
                width: 32,
                height: 32,
                decoration: BoxDecoration(
                  color: tone.withValues(alpha: 0.12),
                  shape: BoxShape.circle,
                  border: Border.all(color: tone.withValues(alpha: 0.3)),
                ),
                child: Icon(icon, size: 16, color: tone),
              ),
              const SizedBox(width: 10),
              Expanded(
                child: Text(t(title), style: context.text.headline.copyWith(fontWeight: FontWeight.w500)),
              ),
            ],
          ),
          const SizedBox(height: 7),
          Text(t(text), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13)),
          const SizedBox(height: 10),
          SizedBox(
            height: 60,
            width: double.infinity,
            child: CustomPaint(
              painter: _PayoffPainter(idea, up: k.up, down: k.down, axis: k.fg3),
            ),
          ),
        ],
      ),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        card(_Idea.call, LucideIcons.trendingUp, k.up, 'options.intro.call.title', 'options.intro.call.text'),
        const SizedBox(height: 10),
        card(_Idea.put, LucideIcons.trendingDown, k.down, 'options.intro.put.title', 'options.intro.put.text'),
        const SizedBox(height: 10),
        card(_Idea.limited, LucideIcons.shieldCheck, k.ember, 'options.intro.risk.title', 'options.intro.risk.text'),
        const SizedBox(height: 10),
        Wrap(
          spacing: 18,
          runSpacing: 6,
          children: [
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 16,
                  height: 2.5,
                  decoration: BoxDecoration(color: k.up, borderRadius: BorderRadius.circular(2)),
                ),
                const SizedBox(width: 8),
                Text(
                  t('options.intro.legend.result'),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                ),
              ],
            ),
            Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Container(
                  width: 16,
                  height: 10,
                  decoration: BoxDecoration(color: k.down.withValues(alpha: 0.2), borderRadius: BorderRadius.circular(3)),
                ),
                const SizedBox(width: 8),
                Text(
                  t('options.intro.legend.cost'),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                ),
              ],
            ),
          ],
        ),
      ],
    );
  }
}

/// Result at expiry against the price at expiry (viewBox 160 x 68, always left to right): a bought call, a bought put,
/// or a bought call next to a sold one (dashed). The shaded band is the price paid.
class _PayoffPainter extends CustomPainter {
  _PayoffPainter(this.kind, {required this.up, required this.down, required this.axis});
  final _Idea kind;
  final Color up, down, axis;

  @override
  void paint(Canvas canvas, Size size) {
    final sx = size.width / 160, sy = size.height / 68;
    Offset p(double x, double y) => Offset(x * sx, y * sy);
    final put = kind == _Idea.put;
    final strike = put ? 90.0 : 70.0;
    final dash = Paint()
      ..color = axis.withValues(alpha: 0.5)
      ..strokeWidth = 1;
    for (double x = 4; x < 156; x += 7) {
      canvas.drawLine(p(x, 34), p((x + 3).clamp(4, 156), 34), dash);
    }
    canvas.drawLine(
      p(strike, 6),
      p(strike, 62),
      Paint()
        ..color = axis.withValues(alpha: 0.3)
        ..strokeWidth = 1,
    );
    canvas.drawRect(Rect.fromPoints(p(put ? 90 : 4, 34), p((put ? 90 : 4) + 66, 44)), Paint()..color = down.withValues(alpha: 0.15));
    if (kind == _Idea.limited) {
      final sold = [p(4, 24), p(70, 24), p(156, 64)];
      final paint = Paint()
        ..color = down.withValues(alpha: 0.6)
        ..strokeWidth = 1.75
        ..strokeCap = StrokeCap.round;
      for (var i = 0; i < sold.length - 1; i++) {
        final a = sold[i], b = sold[i + 1];
        final len = (b - a).distance;
        for (double d = 0; d < len; d += 8) {
          canvas.drawLine(Offset.lerp(a, b, d / len)!, Offset.lerp(a, b, ((d + 4) / len).clamp(0, 1))!, paint);
        }
      }
    }
    final bought = put ? [p(4, 4), p(90, 44), p(156, 44)] : [p(4, 44), p(70, 44), p(156, 4)];
    canvas.drawPath(
      Path()..addPolygon(bought, false),
      Paint()
        ..color = up
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2.25
        ..strokeJoin = StrokeJoin.round
        ..strokeCap = StrokeCap.round,
    );
  }

  @override
  bool shouldRepaint(_PayoffPainter old) => old.kind != kind || old.up != up;
}

/// The options intro: the three ideas, "I understand how options work", the terms link and the start button (the
/// Options page; the open-account wizard before a first Options account, where its own Continue records the
/// acceptance: `onStart` null hides the button).
class OptionsIntroCard extends StatelessWidget {
  const OptionsIntroCard({
    super.key,
    required this.data,
    required this.readOnly,
    required this.understood,
    required this.busy,
    required this.onUnderstood,
    required this.onTerms,
    this.onStart,
  });
  final Suitability data;
  final bool readOnly, understood, busy;
  final ValueChanged<bool> onUnderstood;
  final VoidCallback onTerms;
  final VoidCallback? onStart;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final d = data.disclosure;
    return KCard(
      padding: const EdgeInsets.fromLTRB(18, 20, 18, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('options.intro.title'), subtitle: t('options.intro.subtitle'), icon: LucideIcons.sparkles),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: KButton(
              label: t('options.intro.quiz'),
              icon: LucideIcons.graduationCap,
              variant: KButtonVariant.ghost,
              size: KButtonSize.sm,
              onPressed: () => context.push(kOptionsCourseHref),
            ),
          ),
          const SizedBox(height: 8),
          const _IdeaCards(),
          const SizedBox(height: 16),
          Container(
            padding: const EdgeInsets.fromLTRB(14, 14, 14, 14),
            decoration: BoxDecoration(
              color: k.surface2.withValues(alpha: 0.5),
              borderRadius: BorderRadius.circular(18),
              border: Border.all(color: k.line),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (readOnly) ...[KNotice(tone: KChipTone.warn, icon: LucideIcons.lock, text: t('options.readOnly')), const SizedBox(height: 12)],
                Opacity(
                  opacity: readOnly || d == null ? 0.6 : 1,
                  child: KCheckRow(
                    value: understood,
                    onChanged: readOnly || d == null ? (_) {} : onUnderstood,
                    child: Text(
                      t('options.intro.confirm'),
                      style: context.text.headline.copyWith(fontWeight: FontWeight.w500, color: k.fg),
                    ),
                  ),
                ),
                Padding(
                  padding: const EdgeInsetsDirectional.only(start: 32, top: 2),
                  child: d == null
                      ? Text(t('options.terms.unavailable'), style: context.text.footnote.copyWith(color: k.fg3))
                      : Wrap(
                          crossAxisAlignment: WrapCrossAlignment.center,
                          children: [
                            Text('${t('options.intro.consent')} ', style: context.text.footnote.copyWith(color: k.fg3)),
                            KPressable(
                              onTap: onTerms,
                              minSize: 32,
                              child: Row(
                                mainAxisSize: MainAxisSize.min,
                                children: [
                                  Icon(LucideIcons.scrollText, size: 14, color: k.ember),
                                  const SizedBox(width: 4),
                                  Text(
                                    t('options.intro.terms'),
                                    style: context.text.footnote.copyWith(color: k.ember, fontWeight: FontWeight.w600),
                                  ),
                                ],
                              ),
                            ),
                          ],
                        ),
                ),
                if (onStart != null) ...[
                  const SizedBox(height: 14),
                  KButton(
                    label: t('options.intro.start'),
                    icon: LucideIcons.candlestickChart,
                    size: KButtonSize.lg,
                    expand: true,
                    loading: busy,
                    onPressed: !understood || d == null || readOnly ? null : onStart,
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ sheets */

void _showHowItWorks(BuildContext context, Suitability data) {
  final router = GoRouter.of(context);
  showKSheet<void>(
    context,
    title: context.t('options.hero.howItWorks'),
    builder: (ctx) {
      final t = ctx.t;
      return KSheetContent(
        footer: Row(
          children: [
            if (data.disclosure != null)
              KTextButton(
                label: t('options.intro.terms'),
                color: ctx.k.fg2,
                onPressed: () {
                  Navigator.of(ctx).pop();
                  _showTerms(context, data);
                },
              ),
            const SizedBox(width: 10),
            Flexible(
              child: KTextButton(
                label: t('options.intro.quiz'),
                color: ctx.k.fg2,
                onPressed: () {
                  Navigator.of(ctx).pop();
                  router.push(kOptionsCourseHref);
                },
              ),
            ),
            const Spacer(),
            KButton(label: t('options.intro.gotIt'), size: KButtonSize.sm, onPressed: () => Navigator.of(ctx).pop()),
          ],
        ),
        children: const [_IdeaCards()],
      );
    },
  );
}

/// The full options terms (key points translated + the binding English text).
void showOptionsTerms(BuildContext context, Suitability data) => _showTerms(context, data);

void _showTerms(BuildContext context, Suitability data) {
  final d = data.disclosure;
  if (d == null) return;
  showKSheet<void>(
    context,
    title: context.t('options.terms.title'),
    expand: true,
    builder: (ctx) {
      final t = ctx.t;
      final k = ctx.k;
      final f = LocaleFormat(t.locale);
      final en = t.locale == 'en';
      return KSheetContent(
        footer: Row(
          children: [
            if (data.accepted && data.acceptedVersion != null && data.acceptedAt != null)
              Expanded(
                child: Row(
                  children: [
                    Icon(LucideIcons.circleCheck, size: 16, color: k.up),
                    const SizedBox(width: 6),
                    Flexible(
                      child: Text(
                        t('options.terms.acceptedOn', {'version': data.acceptedVersion, 'date': f.date(data.acceptedAt!)}),
                        style: ctx.text.footnote.copyWith(color: k.up),
                      ),
                    ),
                  ],
                ),
              )
            else
              const Spacer(),
            KButton(label: t('options.terms.close'), variant: KButtonVariant.surface, size: KButtonSize.sm, onPressed: () => Navigator.of(ctx).pop()),
          ],
        ),
        children: [
          Text(
            t('options.terms.version', {'version': d.version, 'date': d.publishedAt == null ? '—' : f.date(d.publishedAt!)}),
            textAlign: TextAlign.center,
            style: ctx.text.footnote.copyWith(color: k.fg3),
          ),
          const SizedBox(height: 14),
          if (!en) ...[
            Container(
              padding: const EdgeInsets.fromLTRB(14, 12, 14, 12),
              decoration: BoxDecoration(
                color: k.surface2.withValues(alpha: 0.6),
                borderRadius: BorderRadius.circular(16),
                border: Border.all(color: k.line),
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(t('options.terms.inShort'), style: ctx.text.label.copyWith(color: k.fg2)),
                  const SizedBox(height: 8),
                  for (final p in const [
                    (LucideIcons.shieldCheck, 'options.terms.point.buy'),
                    (LucideIcons.scale, 'options.terms.point.sell'),
                    (LucideIcons.bookOpen, 'options.terms.point.prices'),
                    (LucideIcons.coins, 'options.terms.point.settle'),
                  ])
                    Padding(
                      padding: const EdgeInsets.only(bottom: 7),
                      child: Row(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Icon(p.$1, size: 16, color: k.ember),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Text(t(p.$2), style: ctx.text.callout.copyWith(color: k.fg)),
                          ),
                        ],
                      ),
                    ),
                ],
              ),
            ),
            const SizedBox(height: 18),
          ],
          Text(d.title, style: ctx.text.title2),
          if (!en) ...[
            const SizedBox(height: 4),
            Text(
              t('options.terms.englishNote'),
              style: ctx.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
            ),
          ],
          const SizedBox(height: 10),
          const KDivider(),
          const SizedBox(height: 10),
          Directionality(textDirection: TextDirection.ltr, child: MarkdownText(d.bodyMd)),
        ],
      );
    },
  );
}

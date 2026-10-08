// Quick trade (web: components/options/simple.tsx): the guided way to an option for traders who know CFDs but not
// options.
//   1. the market (underlying)      4. how far it will go (the strike: close to today's price, or further away =
//   2. Up or Down? (call / put)        cheaper and less likely), with its price and chance of profit
//   3. by when (the expiry)         5. how many contracts, with the total
// then "What happens" in plain words and one confirm button. Only buying (risk limited to the price paid); selling and
// strategies are in the order ticket and the strategy builder. While the order book is live the confirm hands the
// option to the order ticket, which trades it on the book. "Explain it to me" asks Kalks AI for a short explanation in
// the reader's language (`trade/options/explain`), with a built-in explanation when the AI isn't configured.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/cupertino.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../../core/api/api_error.dart';
import '../../../../core/config/app_config.dart';
import '../../../../core/notifications/notifications.dart';
import '../../../../i18n/i18n.dart';
import '../../../../ui/ui.dart';
import '../../core/terminal_controller.dart';
import '../../widgets/kit.dart';
import '../core/errors.dart';
import '../core/format.dart';
import '../core/math.dart';
import '../core/models.dart';
import '../core/preview_loop.dart';
import '../core/store.dart';
import 'actions.dart';
import 'bits.dart';
import 'header.dart';
import 'outcome.dart';

/// One strike offered for "How far": the row, the price of one contract (USD) and the chance of profit.
class QuickTarget {
  const QuickTarget({required this.i, required this.row, required this.right, required this.priceUsd, required this.premium, required this.pop});
  final int i;
  final OptionChainRow row;
  final String right;
  final double priceUsd, premium, pop;
}

/// The at-the-money strike, then one and two steps further in the chosen direction (web targetsOf).
List<QuickTarget> quickTargets(OptionChain chain, String view, {int? nowMs}) {
  final rows = chain.rows;
  if (rows.isEmpty) return const [];
  final i0 = atmIndex(chain);
  final right = view == 'up' ? 'call' : 'put';
  final dir = view == 'up' ? 1 : -1;
  final usdU = usdPerUnitOf(chain);
  final now = nowMs ?? DateTime.now().millisecondsSinceEpoch;
  final years = math.max(1 / 8760, (chain.cutMs - now) / (365 * 86400000));
  final fwd = chain.atmStrike ?? chain.spot?.mid ?? rows[i0].strike;
  final out = <QuickTarget>[];
  for (final k in const [0, 1, 2]) {
    final i = i0 + dir * k;
    if (i < 0 || i >= rows.length || out.any((x) => x.i == i)) continue;
    final row = rows[i];
    final q = row.of(right);
    if (q == null) continue;
    final premium = fillOf(q, 'buy');
    final leg = PayLeg(right: right, strike: row.strike, side: 'buy', contracts: 1, premium: premium, iv: q.iv);
    final iv = q.iv > 0 ? q.iv : 0.1;
    out.add(
      QuickTarget(
        i: i,
        row: row,
        right: right,
        priceUsd: q.askUsd > 0 ? q.askUsd : premium * usdU,
        premium: premium,
        pop: probProfit([leg], usdU, fwd, iv, years),
      ),
    );
  }
  return out;
}

class QuickTrade extends ConsumerStatefulWidget {
  const QuickTrade({super.key, required this.onDone, required this.onTicket});

  /// "See my positions"
  final VoidCallback onDone;

  /// The full order ticket.
  final VoidCallback onTicket;

  @override
  ConsumerState<QuickTrade> createState() => _QuickTradeState();
}

class _QuickTradeState extends ConsumerState<QuickTrade> {
  String? _view;
  int _reach = 1;
  int _contracts = 1;
  bool _busy = false;
  ({String code, String message})? _err;
  String? _done;
  List<QuickTarget> _targets = const [];
  int _lastAt = 0;
  String _lastKey = '';
  String _resetKey = '';
  late final PreviewLoop _preview = PreviewLoop(ref, () {
    if (mounted) setState(() {});
  });

  @override
  void dispose() {
    _preview.dispose();
    super.dispose();
  }

  List<QuickTarget> _targetsOf(OptionChain? chain) {
    if (chain == null || _view == null) return const [];
    final key = '${chain.underlying}|${chain.expiry}|$_view';
    final now = DateTime.now().millisecondsSinceEpoch;
    if (key == _lastKey && now - _lastAt < 1000 && _targets.isNotEmpty) return _targets;
    _lastKey = key;
    _lastAt = now;
    _targets = quickTargets(chain, _view!);
    return _targets;
  }

  Future<void> _confirm({required OptionChain chain, required QuickTarget target, required OptionQuote q, required bool book}) async {
    final t = context.t;
    final ctl = ref.read(optionsProvider.notifier);
    final u = chain.underlying;
    final leg = TicketLeg(
      id: '',
      series: q.code,
      u: u,
      expiry: chain.expiry,
      right: target.right,
      strike: target.row.strike,
      strikeLabel: target.row.strikeLabel,
      side: 'buy',
      contracts: _contracts,
    );
    if (book) {
      ctl.setLegs([leg]);
      ctl.setTicket((x) => x.copyWith(bookType: 'market'));
      ctl.setPrefs((p) => p.copyWith(panel: 'ticket'));
      widget.onTicket();
      return;
    }
    final api = ref.read(optionsApiProvider);
    if (api == null) return;
    setState(() {
      _busy = true;
      _err = null;
    });
    final what = orderWhat(t, side: 'buy', n: _contracts, u: u, strikeLabel: target.row.strikeLabel, right: target.right, expiry: chain.expiry);
    try {
      final r = await api.order({
        'legs': [
          {'series': q.code, 'side': 'buy', 'contracts': _contracts},
        ],
        'type': 'market',
        'clientOrderId': clientOrderId('opq'),
      });
      if (!mounted) return;
      KHaptics.success();
      optToast(ref, NotificationKind.success, r['status'] == 'placed' ? t('trader.opt.toast.placed') : t('trader.opt.toast.filled'), description: what);
      setState(() {
        _busy = false;
        _done = what;
      });
    } on ApiException catch (e) {
      if (!mounted) return;
      KHaptics.error();
      setState(() {
        _busy = false;
        _err = (code: optCode(e), message: e.message);
      });
      if (!needsOnboarding(e.code)) optToast(ref, NotificationKind.error, t('trader.opt.toast.rejected'), description: '$what · ${errText(t, e)}');
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final s = ref.watch(optionsProvider);
    final chain = s.chain != null && s.chain!.underlying == s.u && s.chain!.expiry == s.expiry ? s.chain : null;
    final u = s.u;
    final cur = s.underlyingOf();
    final book = s.isBookLive;
    final readOnly = ref.watch(terminalProvider.select((x) => x.readOnly));
    final live = ref.watch(terminalProvider.select((x) => x.account?.live ?? false));
    final hidden = ref.watch(introHiddenProvider);
    final targets = _targetsOf(chain);
    final resetKey = '$u|$_view|$_reach|$_contracts|${chain?.expiry}';
    if (resetKey != _resetKey) {
      _resetKey = resetKey;
      _err = null;
      _done = null;
    }
    final target = targets.isEmpty ? null : targets[math.min(_reach, targets.length - 1)];
    final q = target?.row.of(target.right);
    final usdU = chain == null ? 0.0 : usdPerUnitOf(chain);
    _preview.update(
      target != null && q != null && chain != null
          ? [LegSpec(series: q.code, u: u, right: target.right, strike: target.row.strike, side: 'buy', contracts: _contracts)]
          : const [],
      enabled: !book,
    );
    final pv = _preview.state.preview;
    final minC = cur?.minContracts ?? 1, maxC = cur?.maxContracts ?? 100, stepC = cur?.contractStep ?? 1;
    final premium = (pv != null && pv.legs.isNotEmpty ? pv.legs.first.price : null) ?? (q != null ? fillOf(q, 'buy') : 0.0);
    final pay = target != null && q != null
        ? [PayLeg(right: target.right, strike: target.row.strike, side: 'buy', contracts: _contracts.toDouble(), premium: premium, iv: q.iv)]
        : const <PayLeg>[];
    final commission = commissionOf(chain, pay, usdU, book: book);
    final total = pv != null ? pv.netPremium.abs() + pv.commission : premium * usdU * _contracts + commission;
    final blocked = target == null || q == null || _busy || readOnly || q.state != 'open' || (!book && (pv == null || !pv.ok || (pv.estimate && live)));
    final rightWord = target?.right == 'call' ? t('trader.opt.call') : t('trader.opt.put');

    return ListView(
      padding: const EdgeInsets.fromLTRB(12, 12, 12, 24),
      children: [
        if (!hidden) ...[const IntroCard(), const SizedBox(height: 16)],
        _Step(
          n: 1,
          title: t('trader.opt.guide.s1'),
          done: true,
          child: _MarketPicker(u: u, name: cur?.name ?? u, chainSpot: chain?.spot?.mid),
        ),
        _Step(
          n: 2,
          title: t('trader.opt.guide.s2', iso({'u': u})),
          done: _view != null,
          child: Container(
            padding: const EdgeInsets.all(4),
            decoration: BoxDecoration(
              color: k.surface2.withValues(alpha: 0.6),
              borderRadius: BorderRadius.circular(11),
              border: Border.all(color: k.line),
            ),
            child: Row(
              children: [
                for (final v in const ['up', 'down']) ...[
                  if (v == 'down') const SizedBox(width: 4),
                  Expanded(
                    child: _UpDown(up: v == 'up', on: _view == v, onTap: () => setState(() => _view = v)),
                  ),
                ],
              ],
            ),
          ),
        ),
        _Step(n: 3, title: t('trader.opt.guide.s3'), help: const Explain('expiry', size: 12), done: _view != null, child: const _WhenPicker()),
        if (_view != null)
          _Step(
            n: 4,
            title: _view == 'up' ? t('trader.opt.guide.s4up', iso({'u': u})) : t('trader.opt.guide.s4down', iso({'u': u})),
            help: const Explain('strike', size: 12),
            done: target != null,
            child: chain == null || targets.isEmpty
                ? Column(
                    children: [
                      for (var i = 0; i < 3; i++)
                        const Padding(
                          padding: EdgeInsets.only(bottom: 6),
                          child: SizedBox(height: 52, child: KSkeleton(radius: 10)),
                        ),
                    ],
                  )
                : Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      for (var i = 0; i < targets.length; i++)
                        Padding(
                          padding: const EdgeInsets.only(bottom: 6),
                          child: _TargetTile(
                            x: targets[i],
                            index: i,
                            up: _view == 'up',
                            on: i == math.min(_reach, targets.length - 1),
                            onTap: () => setState(() => _reach = i),
                          ),
                        ),
                      Row(
                        children: [
                          const SizedBox(width: 4),
                          Flexible(
                            child: Text(
                              t('trader.opt.guide.perContract'),
                              style: context.text.caption.copyWith(fontSize: 10.5, color: k.fg3, fontWeight: FontWeight.w400),
                            ),
                          ),
                          const SizedBox(width: 2),
                          const Explain('chance', size: 11),
                        ],
                      ),
                    ],
                  ),
          ),
        if (_view != null && target != null)
          _Step(
            n: 5,
            title: t('trader.opt.guide.s5'),
            help: const Explain('contracts', size: 12),
            done: true,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    SizedBox(
                      width: 132,
                      child: TStepper(
                        height: 36,
                        value: '$_contracts',
                        step: stepC.toDouble(),
                        min: minC.toDouble(),
                        max: maxC.toDouble(),
                        decimals: 0,
                        semanticLabel: t('trader.opt.ticket.contracts'),
                        onChanged: (v) => setState(() => _contracts = clampContracts(double.tryParse(v) ?? minC, min: minC, max: maxC, step: stepC)),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Expanded(
                      child: TQuickStrip<int>(
                        options: const [1, 2, 5, 10],
                        labels: const ['1', '2', '5', '10'],
                        selected: _contracts,
                        onPick: (v) => setState(() => _contracts = math.min(maxC, v)),
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 6),
                Text(
                  t('trader.opt.ticket.notional', {
                    'n': qty((cur?.contractSize ?? chain?.contractSize ?? 0) * _contracts),
                    'unit': cur?.contractUnit ?? chain?.contractUnit ?? '',
                  }),
                  style: context.text.caption.copyWith(fontSize: 11, color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ],
            ),
          ),
        if (_view != null && target != null && chain != null) ...[
          Row(
            children: [
              Icon(LucideIcons.circleCheckBig, size: 16, color: k.ember),
              const SizedBox(width: 8),
              Text(t('trader.opt.guide.what'), style: context.text.label.copyWith(fontWeight: FontWeight.w600)),
            ],
          ),
          const SizedBox(height: 10),
          if (pay.isNotEmpty && premium > 0)
            OutcomeCard(
              u: u,
              legs: pay,
              usdPerUnit: usdU,
              digits: chain.digits,
              cutMs: chain.cutMs,
              spot: chain.spot?.mid,
              preview: book ? null : pv,
              commission: commission,
              loading: _preview.state.loading,
            )
          else
            const OutcomeSkeleton(),
          if (!book && pv != null) ...[
            const SizedBox(height: 10),
            PreviewSummary(state: _preview.state, digits: chain.digits, collapsible: true, showGreeks: false),
          ],
          if (_err != null) ...[const SizedBox(height: 10), ErrorNote(code: _err!.code, message: _err!.message)],
          const SizedBox(height: 10),
          if (_done != null)
            _DoneCard(text: _done!, onPositions: widget.onDone, onAgain: () => setState(() => _done = null))
          else if (readOnly)
            const ReadOnlyBox()
          else
            OptPrimaryButton(
              label: _busy
                  ? t('trader.opt.ticket.sending')
                  : (s.tradingSoon && live
                        ? t('trader.opt.ticket.soon')
                        : (book
                              ? t('trader.opt.guide.continue')
                              : t('trader.opt.guide.confirm', iso({'n': _contracts, 'u': u, 'right': rightWord, 'strike': target.row.strikeLabel})))),
              amount: !book && total > 0 ? t('trader.opt.ticket.payAmount', iso({'amount': money(total)})) : null,
              trailingIcon: book ? LucideIcons.arrowRight : null,
              onTap: blocked ? null : () => unawaited(_confirm(chain: chain, target: target, q: q, book: book)),
            ),
          const SizedBox(height: 10),
          _ExplainIdea(chain: chain, target: target, contracts: _contracts, view: _view!, total: total, premium: premium, usdU: usdU),
        ],
        const SizedBox(height: 16),
        Container(height: 0.8, color: k.line),
        const SizedBox(height: 12),
        Text(
          t('trader.opt.simple.disclaimer'),
          style: context.text.caption.copyWith(fontSize: 11, color: k.fg3, fontWeight: FontWeight.w400, height: 1.5),
        ),
        const SizedBox(height: 6),
        Align(
          alignment: AlignmentDirectional.centerStart,
          child: KTextButton(
            label: t('trader.opt.guide.fullTicket'),
            color: k.fg2,
            onPressed: () {
              ref.read(optionsProvider.notifier).setPrefs((p) => p.copyWith(panel: 'ticket'));
              widget.onTicket();
            },
          ),
        ),
      ],
    );
  }
}

class _Step extends StatelessWidget {
  const _Step({required this.n, required this.title, required this.child, this.help, this.done = false});
  final int n;
  final String title;
  final Widget child;
  final Widget? help;
  final bool done;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Container(
            width: 22,
            height: 22,
            alignment: Alignment.center,
            decoration: BoxDecoration(
              shape: BoxShape.circle,
              color: done ? k.ember : k.surface2,
              border: done ? null : Border.all(color: k.line),
            ),
            child: Text(
              '$n',
              style: context.text.mono(11, weight: FontWeight.w600, color: done ? Colors.white : k.fg2),
            ),
          ),
          const SizedBox(width: 10),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                ConstrainedBox(
                  constraints: const BoxConstraints(minHeight: 22),
                  child: Row(
                    children: [
                      Flexible(
                        child: Text(title, style: context.text.label.copyWith(fontWeight: FontWeight.w600)),
                      ),
                      if (help != null) ...[const SizedBox(width: 2), help!],
                    ],
                  ),
                ),
                const SizedBox(height: 8),
                child,
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _MarketPicker extends ConsumerWidget {
  const _MarketPicker({required this.u, required this.name, this.chainSpot});
  final String u, name;
  final double? chainSpot;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    return KPressable(
      pressedScale: 0.99,
      semanticLabel: t('trader.opt.pickUnderlying'),
      onTap: () => showKSheet<void>(context, title: t('trader.opt.pickUnderlying'), builder: (_) => const _MarketList()),
      child: Container(
        height: 44,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: k.surface2,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: k.line),
        ),
        child: Row(
          children: [
            OptAvatar(u, size: 22),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(u, style: context.text.label.copyWith(fontSize: 14, fontWeight: FontWeight.w600, height: 1.1)),
                  Text(
                    name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.caption.copyWith(fontSize: 11, color: k.fg3, fontWeight: FontWeight.w400),
                  ),
                ],
              ),
            ),
            Column(
              mainAxisAlignment: MainAxisAlignment.center,
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                SpotPrice(u, fallback: chainSpot),
                FeedChange(u),
              ],
            ),
            const SizedBox(width: 6),
            Icon(LucideIcons.chevronDown, size: 16, color: k.fg3),
          ],
        ),
      ),
    );
  }
}

class _MarketList extends ConsumerWidget {
  const _MarketList();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final k = context.k;
    final list = ref.watch(optionsProvider.select((s) => s.underlyings));
    final u = ref.watch(optionsProvider.select((s) => s.u));
    return ListView(
      shrinkWrap: true,
      padding: const EdgeInsets.fromLTRB(12, 0, 12, 16),
      children: [
        for (final x in list)
          KPressable(
            minSize: 48,
            pressedScale: 1,
            onTap: () {
              ref.read(optionsProvider.notifier).selectUnderlying(x.symbol);
              Navigator.of(context).pop();
            },
            child: Container(
              height: 48,
              padding: const EdgeInsets.symmetric(horizontal: 8),
              decoration: BoxDecoration(color: x.symbol == u ? k.emberSoft : null, borderRadius: BorderRadius.circular(10)),
              child: Row(
                children: [
                  OptAvatar(x.symbol, size: 18),
                  const SizedBox(width: 10),
                  Expanded(
                    child: Column(
                      mainAxisAlignment: MainAxisAlignment.center,
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(x.symbol, style: context.text.label.copyWith(fontWeight: FontWeight.w600)),
                        Text(
                          x.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.caption.copyWith(fontSize: 10.5, color: k.fg3, fontWeight: FontWeight.w400),
                        ),
                      ],
                    ),
                  ),
                  SpotPrice(x.symbol, size: 12),
                ],
              ),
            ),
          ),
      ],
    );
  }
}

class _UpDown extends StatelessWidget {
  const _UpDown({required this.up, required this.on, required this.onTap});
  final bool up, on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final k = context.k;
    final t = context.t;
    final tone = up ? k.up : k.down;
    return KPressable(
      minSize: 46,
      onTap: () {
        KHaptics.selection();
        onTap();
      },
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        height: 46,
        padding: const EdgeInsets.symmetric(horizontal: 10),
        decoration: BoxDecoration(
          color: on ? (up ? k.upSoft : k.downSoft) : Colors.transparent,
          borderRadius: BorderRadius.circular(8),
          border: Border.all(color: on ? tone.withValues(alpha: 0.45) : Colors.transparent),
        ),
        child: Column(
          mainAxisAlignment: MainAxisAlignment.center,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(up ? LucideIcons.trendingUp : LucideIcons.trendingDown, size: 14, color: tone),
                const SizedBox(width: 6),
                Text(
                  up ? t('trader.opt.simple.up') : t('trader.opt.simple.down'),
                  style: context.text.label.copyWith(fontWeight: FontWeight.w600, color: tone),
                ),
              ],
            ),
            Text(
              up ? t('trader.opt.guide.upSub') : t('trader.opt.guide.downSub'),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.caption.copyWith(fontSize: 11.5, color: k.fg3, fontWeight: FontWeight.w400),
            ),
          ],
        ),
      ),
    );
  }
}

class _WhenPicker extends ConsumerWidget {
  const _WhenPicker();

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final list = ref.watch(optionsProvider.select((s) => s.expiries));
    final expiry = ref.watch(optionsProvider.select((s) => s.expiry));
    return NowBuilder(
      builder: (context, now) {
        final open = list.where((e) => expiryOpen(e, now)).toList()..sort((a, b) => a.cutMs.compareTo(b.cutMs));
        if (open.isEmpty) return const SizedBox(height: 48, child: KSkeleton(radius: 10));
        final first = open.take(4).toList();
        final rest = open.skip(4).toList();
        final picked = rest.where((e) => e.date == expiry).firstOrNull;
        Widget chip({required String title, required String sub, required bool on, required VoidCallback onTap, bool dashed = false, bool more = false}) =>
            KPressable(
              onTap: onTap,
              child: Container(
                padding: const EdgeInsets.fromLTRB(10, 6, 10, 6),
                decoration: BoxDecoration(
                  color: on ? k.emberSoft : (dashed ? Colors.transparent : k.surface2),
                  borderRadius: BorderRadius.circular(10),
                  border: Border.all(color: on ? k.ember : k.line, width: on ? 1.5 : 1),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      children: [
                        Flexible(
                          child: Text(
                            title,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.label.copyWith(fontSize: 12, fontWeight: FontWeight.w600, color: on ? k.fg : k.fg2),
                          ),
                        ),
                        if (more) ...[const SizedBox(width: 3), Icon(LucideIcons.chevronDown, size: 12, color: k.fg3)],
                      ],
                    ),
                    Text(
                      sub,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: context.text.caption.copyWith(fontSize: 10.5, color: on ? k.ember : k.fg3, fontWeight: FontWeight.w400),
                    ),
                  ],
                ),
              ),
            );
        final chips = [
          for (final e in first)
            chip(
              title: dayLabel(t, e.date, t.locale, now),
              sub: t('trader.opt.plain.in', iso({'left': countdown(e.cutMs, now)})),
              on: e.date == expiry,
              onTap: () => ref.read(optionsProvider.notifier).selectExpiry(e.date),
            ),
          if (rest.isNotEmpty)
            chip(
              title: picked != null ? expiryLabel(picked.date, t.locale) : t('trader.opt.guide.later'),
              sub: picked != null
                  ? t('trader.opt.plain.in', iso({'left': countdown(picked.cutMs, now)}))
                  : t('trader.opt.guide.moreDates', {'count': rest.length}),
              on: picked != null,
              dashed: picked == null,
              more: true,
              onTap: () => showKSheet<void>(
                context,
                title: t('trader.opt.guide.later'),
                builder: (ctx) => GridView.count(
                  crossAxisCount: 2,
                  shrinkWrap: true,
                  childAspectRatio: 3.2,
                  mainAxisSpacing: 6,
                  crossAxisSpacing: 6,
                  padding: const EdgeInsets.fromLTRB(16, 0, 16, 16),
                  children: [
                    for (final e in rest)
                      KPressable(
                        minSize: 40,
                        onTap: () {
                          ref.read(optionsProvider.notifier).selectExpiry(e.date);
                          Navigator.of(ctx).pop();
                        },
                        child: Container(
                          padding: const EdgeInsets.symmetric(horizontal: 10),
                          decoration: BoxDecoration(
                            color: e.date == expiry ? ctx.k.emberSoft : ctx.k.surface2,
                            borderRadius: BorderRadius.circular(8),
                            border: Border.all(color: e.date == expiry ? ctx.k.ember.withValues(alpha: 0.5) : ctx.k.line),
                          ),
                          child: Column(
                            mainAxisAlignment: MainAxisAlignment.center,
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(expiryLabel(e.date, t.locale), style: ctx.text.caption.copyWith(color: ctx.k.fg)),
                              Text(countdown(e.cutMs, now), style: ctx.text.mono(10, color: ctx.k.fg3)),
                            ],
                          ),
                        ),
                      ),
                  ],
                ),
              ),
            ),
        ];
        return LayoutBuilder(
          builder: (context, c) {
            final perRow = c.maxWidth >= 330 ? 3 : 2;
            final w = (c.maxWidth - (perRow - 1) * 6) / perRow;
            return Wrap(
              spacing: 6,
              runSpacing: 6,
              children: [for (final x in chips) SizedBox(width: w, child: x)],
            );
          },
        );
      },
    );
  }
}

class _TargetTile extends StatelessWidget {
  const _TargetTile({required this.x, required this.index, required this.up, required this.on, required this.onTap});
  final QuickTarget x;
  final int index;
  final bool up, on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KPressable(
      minSize: 52,
      pressedScale: 0.99,
      onTap: () {
        KHaptics.selection();
        onTap();
      },
      child: AnimatedContainer(
        duration: const Duration(milliseconds: 150),
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
        decoration: BoxDecoration(
          color: on ? k.emberSoft : k.surface2,
          borderRadius: BorderRadius.circular(10),
          border: Border.all(color: on ? k.ember : k.line, width: on ? 1.5 : 1),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Container(
              width: 16,
              height: 16,
              margin: const EdgeInsets.only(top: 2),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                border: Border.all(color: on ? k.ember : k.fg3.withValues(alpha: 0.6)),
              ),
              child: on
                  ? Container(
                      width: 8,
                      height: 8,
                      decoration: BoxDecoration(color: k.ember, shape: BoxShape.circle),
                    )
                  : null,
            ),
            const SizedBox(width: 10),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Expanded(
                        child: Text(
                          up ? t('trader.opt.guide.above', iso({'price': x.row.strikeLabel})) : t('trader.opt.guide.below', iso({'price': x.row.strikeLabel})),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.label.copyWith(fontSize: 12.5, fontWeight: FontWeight.w600),
                        ),
                      ),
                      Text(
                        money(x.priceUsd),
                        textDirection: TextDirection.ltr,
                        style: context.text.mono(13, weight: FontWeight.w600),
                      ),
                    ],
                  ),
                  const SizedBox(height: 4),
                  Row(
                    children: [
                      SizedBox(
                        width: 40,
                        height: 4,
                        child: ClipRRect(
                          borderRadius: BorderRadius.circular(2),
                          child: LinearProgressIndicator(value: x.pop.clamp(0, 1), backgroundColor: k.surface3, color: k.up),
                        ),
                      ),
                      const SizedBox(width: 6),
                      Expanded(
                        child: Text(
                          t('trader.opt.guide.chance', iso({'pct': pct(x.pop, 0)})),
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: context.text.caption.copyWith(fontSize: 10.5, color: k.fg2, fontWeight: FontWeight.w400),
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 3),
                  Text(
                    t(index == 0 ? 'trader.opt.guide.reach0' : (index == 1 ? 'trader.opt.guide.reach1' : 'trader.opt.guide.reach2')),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.caption.copyWith(fontSize: 10.5, color: k.fg3, fontWeight: FontWeight.w400, height: 1.35),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _DoneCard extends StatelessWidget {
  const _DoneCard({required this.text, required this.onPositions, required this.onAgain});
  final String text;
  final VoidCallback onPositions, onAgain;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: k.upSoft,
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: k.up.withValues(alpha: 0.35)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Icon(LucideIcons.circleCheckBig, size: 16, color: k.up),
              const SizedBox(width: 8),
              Text(t('trader.opt.guide.doneTitle'), style: context.text.label.copyWith(fontWeight: FontWeight.w600)),
            ],
          ),
          const SizedBox(height: 4),
          Text(text, style: context.text.footnote.copyWith(color: k.fg2)),
          const SizedBox(height: 10),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              KButton(label: t('trader.opt.guide.seePositions'), trailingIcon: LucideIcons.arrowRight, size: KButtonSize.sm, onPressed: onPositions),
              KButton(label: t('trader.opt.guide.again'), size: KButtonSize.sm, variant: KButtonVariant.surface, onPressed: onAgain),
            ],
          ),
        ],
      ),
    );
  }
}

/// "Explain it to me": Kalks AI explains the idea in the reader's language (a built-in explanation without it).
class _ExplainIdea extends ConsumerStatefulWidget {
  const _ExplainIdea({
    required this.chain,
    required this.target,
    required this.contracts,
    required this.view,
    required this.total,
    required this.premium,
    required this.usdU,
  });
  final OptionChain chain;
  final QuickTarget target;
  final int contracts;
  final String view;
  final double total, premium, usdU;

  @override
  ConsumerState<_ExplainIdea> createState() => _ExplainIdeaState();
}

class _ExplainIdeaState extends ConsumerState<_ExplainIdea> {
  ({bool busy, String? text, bool ai, String? error})? _x;
  String _key = '';

  Future<void> _ask() async {
    final t = context.t;
    final c = widget.chain;
    final x = widget.target;
    final q = x.row.of(x.right)!;
    final be = x.right == 'call' ? x.row.strike + widget.premium : x.row.strike - widget.premium;
    final plain = t(widget.view == 'up' ? 'trader.opt.simple.plainCall' : 'trader.opt.simple.plainPut', {
      'cost': usd(widget.total),
      'u': c.underlying,
      'be': px(be, c.digits),
      'date': expiryLabel(c.expiry, t.locale),
      'time': c.cutTime,
    });
    setState(() => _x = (busy: true, text: null, ai: false, error: null));
    final body = {
      'locale': t.locale,
      'strategy': {
        'name': x.right == 'call' ? t('trader.opt.simple.buyCall') : t('trader.opt.simple.buyPut'),
        'view': widget.view,
        'underlying': c.underlying,
        'expiry': c.expiry,
        'cut': '${c.cutTime} ${c.cutZone}',
        'spot': c.spot?.mid,
        'contractSize': c.contractSize,
        'contractUnit': c.contractUnit,
        'legs': [
          {
            'side': 'buy',
            'right': x.right,
            'strike': x.row.strike,
            'contracts': widget.contracts,
            'premiumUsd': q.askUsd > 0 ? q.askUsd : widget.premium * widget.usdU,
            'iv': q.iv,
            'delta': q.delta,
          },
        ],
        'netPremium': double.parse(widget.total.toStringAsFixed(2)),
        'maxProfit': x.right == 'call' ? null : double.parse(math.max(0, (x.row.strike - widget.premium) * widget.usdU * widget.contracts).toStringAsFixed(2)),
        'maxLoss': double.parse(widget.total.toStringAsFixed(2)),
        'breakevens': [be],
        'probProfit': double.parse(x.pop.toStringAsFixed(3)),
      },
    };
    // the broker switched Kalks AI off (module `ai`): the built-in explanation, no call
    final api = ref.read(configProvider).moduleOn('ai') ? ref.read(optionsApiProvider) : null;
    try {
      final r = api == null ? const <String, dynamic>{'configured': false} : await api.explain(body);
      if (!mounted) return;
      final text = r['text'];
      if (text is String && text.isNotEmpty) {
        setState(() => _x = (busy: false, text: text, ai: true, error: null));
      } else if (r['configured'] == false) {
        setState(() => _x = (busy: false, text: '$plain ${t('trader.opt.simple.basicMore')}', ai: false, error: null));
      } else {
        setState(() => _x = (busy: false, text: null, ai: false, error: t('trader.opt.simple.explainFailed')));
      }
    } on ApiException catch (e) {
      if (!mounted) return;
      if (e.isModuleDisabled) {
        setState(() => _x = (busy: false, text: '$plain ${t('trader.opt.simple.basicMore')}', ai: false, error: null));
        return;
      }
      final denied = switch (e.code) {
        'signin' => t('desk.ai.signin'),
        'rate_minute' => t('desk.ai.rateMinute', {'n': 10}),
        'rate_day' => t('desk.ai.rateDay', {'n': 200}),
        'unavailable' => t('desk.ai.unavailable'),
        'forbidden' => t('desk.ai.forbidden'),
        _ => null,
      };
      setState(() => _x = (busy: false, text: null, ai: false, error: denied ?? (e.message.isNotEmpty ? e.message : t('trader.opt.simple.explainFailed'))));
    }
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final key = '${widget.target.row.strike}|${widget.view}|${widget.chain.expiry}|${widget.chain.underlying}';
    if (key != _key) {
      _key = key;
      _x = null;
    }
    final x = _x;
    if (x == null) {
      return KPressable(
        minSize: 36,
        onTap: () => unawaited(_ask()),
        child: Container(
          height: 34,
          decoration: BoxDecoration(
            borderRadius: BorderRadius.circular(8),
            border: Border.all(color: k.line),
          ),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              Icon(LucideIcons.sparkles, size: 14, color: k.ember),
              const SizedBox(width: 6),
              Text(t('trader.opt.guide.explain'), style: context.text.label.copyWith(fontSize: 12.5, color: k.fg2)),
            ],
          ),
        ),
      );
    }
    return Container(
      padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
      decoration: BoxDecoration(
        color: k.surface2.withValues(alpha: 0.5),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: k.line),
      ),
      child: x.busy
          ? Row(
              children: [
                CupertinoActivityIndicator(radius: 7, color: k.fg3),
                const SizedBox(width: 8),
                Text(t('trader.opt.simple.explaining'), style: context.text.caption.copyWith(color: k.fg3)),
              ],
            )
          : x.error != null
          ? Text(x.error!, style: context.text.caption.copyWith(color: k.down))
          : Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Icon(x.ai ? LucideIcons.sparkles : LucideIcons.messageSquareText, size: 12, color: x.ai ? k.ember : k.fg3),
                    const SizedBox(width: 4),
                    Text(
                      (x.ai ? t('trader.opt.simple.byClaude') : t('trader.opt.simple.basic')).toUpperCase(),
                      style: context.text.micro.copyWith(fontSize: 10, color: k.fg3, letterSpacing: 0.8),
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                Text(x.text ?? '', style: context.text.footnote.copyWith(color: k.fg2, height: 1.5)),
              ],
            ),
    );
  }
}

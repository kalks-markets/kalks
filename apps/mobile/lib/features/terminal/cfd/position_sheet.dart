// The position sheet (web PositionDialog, owner design 2026-10): the "B 0.50" badge, the symbol and the floating
// profit, prices, swap / commission; the stop loss and take profit as two fields labelled just S / T (±10 / 25 / 50 pip
// nudges and Clear, the money at each), then Modify · Close ½ · Close; below, closing part of it (25 / 50 / 100 %), the
// trailing stop and breakeven (sent with Modify), the source, and Close By against an opposite position (hedging
// accounts). And the pending order sheet (web PendingDialog): price, SL / TP, modify or delete.
import 'dart:async';
import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/models.dart';
import '../core/terminal_controller.dart';
import '../core/trade_actions.dart';
import '../core/trade_math.dart';
import '../widgets/kit.dart';
import 'room.dart';

Future<void> showPositionSheet(BuildContext context, String ticket) => showKSheet<void>(context, expand: true, builder: (_) => PositionForm(ticket: ticket));

Future<void> showPendingSheet(BuildContext context, String ticket) => showKSheet<void>(context, builder: (_) => PendingForm(ticket: ticket));

const List<String> _trailOptions = ['none', '15', '20', '30', '50', '100', 'custom'];

class PositionForm extends ConsumerStatefulWidget {
  const PositionForm({super.key, required this.ticket});
  final String ticket;

  @override
  ConsumerState<PositionForm> createState() => _PositionFormState();
}

class _PositionFormState extends ConsumerState<PositionForm> {
  String _vol = '';
  String _sl = '';
  String _tp = '';
  String _trail = 'none';
  String _trailCustom = '25';
  bool _init = false;
  bool _busy = false;

  void _setup(TPosition p, SymbolSpec s) {
    if (_init) return;
    _init = true;
    _vol = fmtVol(p.volume);
    _sl = p.sl == null ? '' : fmtPrice(s.digits, p.sl!);
    _tp = p.tp == null ? '' : fmtPrice(s.digits, p.tp!);
    final pips = p.trailingPoints == null ? null : (p.trailingPoints! * s.point / s.pipSize).round();
    final initial = pips == null ? 'none' : '$pips';
    _trail = _trailOptions.contains(initial) ? initial : 'custom';
    if (pips != null) _trailCustom = '$pips';
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final st = ref.watch(terminalProvider);
    final p = st.positions.where((x) => x.ticket == widget.ticket).firstOrNull;
    final spec = ref.watch(symbolBookProvider.select((b) => p == null ? null : b[p.symbol]));
    final a = st.account;
    if (p == null || spec == null || a == null) {
      // closed meanwhile: the sheet goes away
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).maybePop();
      });
      return const SizedBox(height: 120);
    }
    _setup(p, spec);
    final profit = st.profitOf(p);
    final pip = spec.pipSize;
    final hedging = a.hedging;
    final opp = st.positions.where((x) => x.symbol == p.symbol && x.side != p.side).toList();
    return QuoteBuilder(
      symbol: p.symbol,
      throttle: const Duration(milliseconds: 400),
      builder: (context, q) {
        final cur = q == null ? (p.currentPrice ?? p.openPrice) : (p.buy ? q.bid : q.ask);
        final bidOf = ref.read(marketFeedProvider).bidOf;
        final v = math.min(p.volume, math.max(0.01, double.tryParse(_vol) ?? 0));
        final partial = v < p.volume - 1e-9;
        // half of the position on the volume step (all of it when it can't be split)
        final half = normalizeVolume(spec, p.volume / 2);
        final canHalve = half >= spec.lotMin - 1e-9 && half < p.volume - 1e-9;
        String pipsFrom(double price) => ((p.buy ? price - p.openPrice : p.openPrice - price) / pip).toStringAsFixed(1);
        double? usdAt(String s) {
          final x = double.tryParse(s);
          return x == null ? null : profitAt(spec, side: p.side, lots: p.volume, open: p.openPrice, close: x, bidOf: bidOf);
        }

        void nudge(String which, int pips) {
          final dir = (which == 'tp' ? 1 : -1) * (p.buy ? 1 : -1);
          final val = fmtPrice(spec.digits, cur + dir * pips * pip);
          setState(() => which == 'sl' ? _sl = val : _tp = val);
        }

        Future<void> close(double volume) async {
          setState(() => _busy = true);
          final ok = await ref.read(tradeActionsProvider).closePosition(p.ticket, volume: volume);
          if (!mounted) return;
          setState(() => _busy = false);
          final whole = volume >= p.volume - 1e-9;
          if (ok && whole && context.mounted) Navigator.of(context).maybePop();
          if (ok && !whole) setState(() => _vol = fmtVol(math.max(0.01, p.volume - volume)));
        }

        Future<void> modify() async {
          final sl = double.tryParse(_sl);
          final tp = double.tryParse(_tp);
          final trailPips = _trail == 'none' ? 0.0 : (_trail == 'custom' ? (double.tryParse(_trailCustom) ?? 0) : double.parse(_trail));
          setState(() => _busy = true);
          final ok = await ref
              .read(tradeActionsProvider)
              .modifyPosition(
                p.ticket,
                sl: sl,
                tp: tp,
                clearSl: _sl.isEmpty,
                clearTp: _tp.isEmpty,
                setTrailing: true,
                trailingPoints: trailPips > 0 ? (trailPips * pip / spec.point).round() : null,
              );
          if (!mounted) return;
          setState(() => _busy = false);
          if (ok && context.mounted) Navigator.of(context).maybePop();
        }

        Widget action(String label, VoidCallback? onTap, {Color? fill, Color? fg, Key? key}) => Expanded(
          child: KPressable(
            key: key,
            pressedScale: 0.97,
            onTap: _busy ? null : onTap,
            child: Container(
              height: 50,
              alignment: Alignment.center,
              padding: const EdgeInsets.symmetric(horizontal: 6),
              decoration: BoxDecoration(color: fill ?? k.surface3, borderRadius: BorderRadius.circular(16)),
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: context.text.label.copyWith(fontSize: 14, fontWeight: FontWeight.w700, color: fg ?? k.fg),
              ),
            ),
          ),
        );

        return ListView(
          padding: const EdgeInsets.fromLTRB(16, 0, 16, 20),
          children: [
            // the side + volume badge, the symbol, the P&L
            Row(
              children: [
                SideBadge(side: p.side, lots: p.volume),
                const SizedBox(width: 8),
                Expanded(
                  child: Text(
                    p.symbol,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.headline.copyWith(fontSize: 18, fontWeight: FontWeight.w800),
                  ),
                ),
                Column(
                  crossAxisAlignment: CrossAxisAlignment.end,
                  children: [
                    Text(t('order.position.floatingProfit'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 11)),
                    PnlText(profit, cent: a.cent, size: 22, weight: FontWeight.w800),
                  ],
                ),
              ],
            ),
            const SizedBox(height: 6),
            Text(
              '#${p.ticket} · ${a.login} · ${t.dyn('order.mode.${a.mode}', fallback: a.mode)} · ${t('order.unit.pips', {'n': pipsFrom(cur)})}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: context.text.mono(11.5, color: k.fg3),
            ),
            const SizedBox(height: 8),
            Wrap(
              spacing: 12,
              runSpacing: 2,
              children: [
                _kv(context, t('order.position.openPrice'), Text(fmtPrice(spec.digits, p.openPrice))),
                _kv(context, t('order.position.current'), PriceText(cur, digits: spec.digits, size: 12, dir: q?.dir ?? 0)),
                _kv(context, t('order.position.swapCommission'), Text('${accMoney(a.cent, p.swap)} · ${accMoney(a.cent, -p.commission)}')),
              ],
            ),
            const SizedBox(height: 14),
            // S / T
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                for (final which in ['sl', 'tp']) ...[
                  if (which == 'tp') const SizedBox(width: 8),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        StopField(
                          key: ValueKey('pos-$which'),
                          kind: which,
                          value: which == 'sl' ? _sl : _tp,
                          placeholder: t('order.ticket.notSet'),
                          semanticLabel: which == 'sl' ? t('order.ticket.stopLoss') : t('order.ticket.takeProfit'),
                          onChanged: (x) => setState(() => which == 'sl' ? _sl = x : _tp = x),
                        ),
                        const SizedBox(height: 4),
                        Wrap(
                          spacing: 2,
                          children: [
                            for (final n in [10, 25, 50])
                              KPressable(
                                minSize: 32,
                                onTap: () => nudge(which, n),
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 3),
                                  child: Text(t('order.unit.pipsShort', {'n': n}), style: context.text.mono(11, color: k.fg3)),
                                ),
                              ),
                            if ((which == 'sl' ? _sl : _tp).isNotEmpty)
                              KPressable(
                                minSize: 32,
                                onTap: () => setState(() => which == 'sl' ? _sl = '' : _tp = ''),
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(horizontal: 5, vertical: 3),
                                  child: Text(t('order.position.clear'), style: context.text.caption.copyWith(color: k.fg3)),
                                ),
                              ),
                          ],
                        ),
                        Row(
                          children: [
                            Expanded(
                              child: Text(
                                (which == 'sl' ? _sl : _tp).isEmpty
                                    ? '—'
                                    : t('order.unit.pips', {'n': pipsFrom(double.tryParse(which == 'sl' ? _sl : _tp) ?? cur)}),
                                maxLines: 1,
                                overflow: TextOverflow.ellipsis,
                                style: context.text.mono(11, color: k.fg3),
                              ),
                            ),
                            if (usdAt(which == 'sl' ? _sl : _tp) case final usd?)
                              Text(
                                accMoney(a.cent, usd + p.swap - p.commission, signed: true),
                                style: context.text.mono(11, color: usd >= 0 ? k.up : k.down),
                              ),
                          ],
                        ),
                      ],
                    ),
                  ),
                ],
              ],
            ),
            const SizedBox(height: 10),
            // Modify · Close ½ · Close
            Row(
              children: [
                action(t('order.position.modify'), () => unawaited(modify()), key: const ValueKey('pos-modify')),
                const SizedBox(width: 8),
                action(t('app.trader.position.closeHalf'), () => unawaited(close(canHalve ? half : p.volume)), key: const ValueKey('pos-half')),
                const SizedBox(width: 8),
                action(t('desk.pos.close'), () => unawaited(close(p.volume)), fill: k.ember, fg: k.onEmber, key: const ValueKey('pos-close')),
              ],
            ),
            const SizedBox(height: 18),
            // close part of it
            _title(context, LucideIcons.scissors, partial ? t('order.position.closePartially') : t('order.position.closePosition')),
            Row(
              children: [
                SizedBox(
                  width: 140,
                  child: TStepper(
                    value: _vol,
                    height: 36,
                    min: spec.lotMin,
                    max: p.volume,
                    step: spec.lotStep,
                    onChanged: (x) => setState(() => _vol = x),
                    semanticLabel: t('order.position.closeVolume'),
                  ),
                ),
                const SizedBox(width: 8),
                for (final f in [0.25, 0.5, 1.0]) ...[
                  Expanded(
                    child: KPressable(
                      minSize: 36,
                      onTap: () => setState(() => _vol = fmtVol(math.max(0.01, (p.volume * f * 100).floorToDouble() / 100))),
                      child: Container(
                        height: 34,
                        alignment: Alignment.center,
                        decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(17)),
                        child: Text('${(f * 100).round()}%', style: context.text.mono(12, color: k.fg2)),
                      ),
                    ),
                  ),
                  if (f != 1.0) const SizedBox(width: 4),
                ],
              ],
            ),
            const SizedBox(height: 8),
            KPressable(
              onTap: _busy ? null : () => unawaited(close(v)),
              child: Container(
                height: 44,
                padding: const EdgeInsets.symmetric(horizontal: 14),
                decoration: BoxDecoration(color: p.buy ? k.sellFill : k.buyFill, borderRadius: BorderRadius.circular(14)),
                child: Row(
                  children: [
                    Expanded(
                      child: Text(
                        t('order.position.closeButton', {
                          'ticket': p.ticket,
                          'side': t('order.side.${p.side}'),
                          'volume': fmtVol(v),
                          'symbol': p.symbol,
                          'price': fmtPrice(spec.digits, cur),
                        }),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.headline.copyWith(color: Colors.white, fontSize: 13.5),
                      ),
                    ),
                    Text(
                      accMoney(a.cent, profitAt(spec, side: p.side, lots: v, open: p.openPrice, close: cur, bidOf: bidOf), signed: true),
                      style: context.text.mono(13, weight: FontWeight.w600, color: Colors.white),
                    ),
                  ],
                ),
              ),
            ),
            const SizedBox(height: 18),
            // trailing stop and breakeven (sent with Modify)
            _title(context, LucideIcons.pencil, t('order.position.trailing')),
            Row(
              children: [
                KPressable(
                  minSize: 36,
                  semanticLabel: t('order.position.trailingAria'),
                  onTap: () async {
                    final v = await showKActionSheet<String>(
                      context,
                      title: t('order.position.trailing'),
                      actions: [
                        for (final o in _trailOptions)
                          KAction(
                            label: o == 'none'
                                ? t('order.position.trailNone')
                                : (o == 'custom' ? t('order.position.trailCustom') : t('order.unit.pips', {'n': o})),
                            value: o,
                          ),
                      ],
                    );
                    if (v != null && mounted) setState(() => _trail = v);
                  },
                  child: Container(
                    height: 36,
                    padding: const EdgeInsets.symmetric(horizontal: 12),
                    decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(18)),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Text(
                          _trail == 'none'
                              ? t('order.position.trailNone')
                              : (_trail == 'custom' ? t('order.position.trailCustom') : t('order.unit.pips', {'n': _trail})),
                          style: context.text.callout.copyWith(fontSize: 13),
                        ),
                        const SizedBox(width: 4),
                        Icon(LucideIcons.chevronDown, size: 14, color: k.fg3),
                      ],
                    ),
                  ),
                ),
                if (_trail == 'custom') ...[
                  const SizedBox(width: 6),
                  SizedBox(
                    width: 100,
                    child: TStepper(
                      value: _trailCustom,
                      height: 36,
                      step: 1,
                      decimals: 0,
                      min: 1,
                      onChanged: (x) => setState(() => _trailCustom = x),
                      semanticLabel: t('order.position.trailCustomAria'),
                    ),
                  ),
                ],
                const Spacer(),
              ],
            ),
            const SizedBox(height: 8),
            Align(
              alignment: AlignmentDirectional.centerStart,
              child: KPressable(
                minSize: 36,
                onTap: () => setState(() => _sl = fmtPrice(spec.digits, p.openPrice)),
                child: Container(
                  height: 32,
                  padding: const EdgeInsets.symmetric(horizontal: 12),
                  decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(16)),
                  child: Center(
                    widthFactor: 1,
                    child: Text(
                      t('order.position.breakeven', {'price': fmtPrice(spec.digits, p.openPrice)}),
                      style: context.text.footnote.copyWith(color: k.fg2, fontSize: 12),
                    ),
                  ),
                ),
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(2, 10, 2, 0),
              child: Row(
                children: [
                  Text(fmtServer(p.openTime), style: context.text.mono(11, color: k.fg3)),
                  const Spacer(),
                  Text(
                    t.dyn('order.source.${p.source}', fallback: p.source),
                    style: context.text.footnote.copyWith(color: k.fg3, fontSize: 11.5),
                  ),
                ],
              ),
            ),
            if (hedging) ...[
              const SizedBox(height: 18),
              _title(context, LucideIcons.arrowLeftRight, t('order.position.closeBy')),
              if (opp.isEmpty)
                Container(
                  padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                  decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(12)),
                  child: Text(
                    p.buy ? t('order.position.noOppositeSell', {'symbol': p.symbol}) : t('order.position.noOppositeBuy', {'symbol': p.symbol}),
                    style: context.text.footnote.copyWith(color: k.fg3),
                  ),
                )
              else
                for (final o in opp)
                  Container(
                    margin: const EdgeInsets.only(bottom: 6),
                    padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
                    decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(12)),
                    child: Row(
                      children: [
                        SideBadge(side: o.side, lots: o.volume, filled: false),
                        const SizedBox(width: 6),
                        Text('#${o.ticket}', style: context.text.mono(12)),
                        const SizedBox(width: 6),
                        Expanded(
                          child: Text(
                            t('order.position.volumeAt', {'volume': fmtVol(o.volume), 'price': fmtPrice(spec.digits, o.openPrice)}),
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                            style: context.text.mono(12, color: k.fg2),
                          ),
                        ),
                        KButton(
                          label: t('order.position.closeByTicket', {'ticket': o.ticket}),
                          size: KButtonSize.sm,
                          variant: KButtonVariant.surface,
                          onPressed: () async {
                            final ok = await ref.read(tradeActionsProvider).closeBy(p.ticket, o.ticket);
                            if (ok && context.mounted) Navigator.of(context).maybePop();
                          },
                        ),
                      ],
                    ),
                  ),
            ],
          ],
        );
      },
    );
  }

  Widget _title(BuildContext context, IconData icon, String text) => Padding(
    padding: const EdgeInsets.only(bottom: 8),
    child: Row(
      children: [
        Icon(icon, size: 15, color: context.k.fg3),
        const SizedBox(width: 7),
        Text(
          text,
          style: context.text.label.copyWith(color: context.k.fg, fontWeight: FontWeight.w600, fontSize: 13),
        ),
      ],
    ),
  );

  Widget _kv(BuildContext context, String k, Widget v) => Padding(
    padding: const EdgeInsets.only(bottom: 2),
    child: Row(
      mainAxisSize: MainAxisSize.min,
      children: [
        Text(k, style: context.text.footnote.copyWith(color: context.k.fg3, fontSize: 11)),
        const SizedBox(width: 6),
        DefaultTextStyle.merge(
          style: context.text.mono(12, color: context.k.fg2),
          child: v,
        ),
      ],
    ),
  );
}

/// Modify or delete a pending order (web PendingDialog).
class PendingForm extends ConsumerStatefulWidget {
  const PendingForm({super.key, required this.ticket});
  final String ticket;

  @override
  ConsumerState<PendingForm> createState() => _PendingFormState();
}

class _PendingFormState extends ConsumerState<PendingForm> {
  String _price = '', _sl = '', _tp = '';
  bool _init = false;
  bool _busy = false;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final o = ref.watch(terminalProvider.select((s) => s.orders.where((x) => x.ticket == widget.ticket).firstOrNull));
    final spec = ref.watch(symbolBookProvider.select((b) => o == null ? null : b[o.symbol]));
    if (o == null || spec == null) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) Navigator.of(context).maybePop();
      });
      return const SizedBox(height: 120);
    }
    if (!_init) {
      _init = true;
      _price = fmtPrice(spec.digits, o.price);
      _sl = o.sl == null ? '' : fmtPrice(spec.digits, o.sl!);
      _tp = o.tp == null ? '' : fmtPrice(spec.digits, o.tp!);
    }
    final expiry = o.expiry == 'Date' ? (o.expiryDate ?? '') : (o.expiry == 'GTC' ? t('order.expiry.gtc') : t('order.expiry.today'));
    return Padding(
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 12),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Text(
            t('order.pendingDialog.title', {'ticket': o.ticket, 'label': t(o.labelKey), 'volume': fmtVol(o.volume), 'symbol': o.symbol}),
            style: context.text.headline,
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              SymbolAvatar(o.symbol, size: 18),
              const SizedBox(width: 8),
              Text(
                o.symbol,
                style: context.text.label.copyWith(color: k.fg, fontWeight: FontWeight.w600),
              ),
              const Spacer(),
              QuoteBuilder(
                symbol: o.symbol,
                builder: (context, q) => KRichText(
                  t('order.pendingDialog.bidAsk', {'bid': fmtPrice(spec.digits, q?.bid ?? 0), 'ask': fmtPrice(spec.digits, q?.ask ?? 0)}),
                  style: context.text.mono(12, color: k.fg3),
                  tags: {
                    'b': KTag(style: TextStyle(color: k.fg)),
                    'a': KTag(style: TextStyle(color: k.fg)),
                  },
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Text(t('order.pendingDialog.price'), style: context.text.label.copyWith(color: k.fg2)),
          const SizedBox(height: 5),
          TStepper(
            value: _price,
            height: 40,
            step: priceStep(spec),
            decimals: spec.digits,
            onChanged: (v) => _price = v,
            semanticLabel: t('order.ticket.orderPrice'),
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              for (final which in ['sl', 'tp']) ...[
                if (which == 'tp') const SizedBox(width: 8),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Text(
                        which == 'sl' ? t('order.position.stopLoss') : t('order.position.takeProfit'),
                        style: context.text.label.copyWith(color: which == 'sl' ? k.down : k.up),
                      ),
                      const SizedBox(height: 5),
                      TStepper(
                        value: which == 'sl' ? _sl : _tp,
                        height: 36,
                        step: spec.pipSize,
                        decimals: spec.digits,
                        tone: which == 'sl' ? k.down : k.up,
                        placeholder: t('order.ticket.notSet'),
                        onChanged: (v) => which == 'sl' ? _sl = v : _tp = v,
                        semanticLabel: which == 'sl' ? t('order.ticket.stopLoss') : t('order.ticket.takeProfit'),
                      ),
                    ],
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 10),
          Text(
            '${t('order.pendingDialog.meta', {'expiry': expiry, 'placed': fmtServer(o.placed)})} ${o.oco != null ? t('order.pendingDialog.ocoLinked') : ''}',
            style: context.text.mono(11.5, color: k.fg3),
          ),
          const SizedBox(height: 14),
          Row(
            children: [
              KButton(
                label: t('order.pendingDialog.delete'),
                icon: LucideIcons.x,
                variant: KButtonVariant.danger,
                onPressed: _busy
                    ? null
                    : () async {
                        final ok = await ref.read(tradeActionsProvider).cancelOrder(o.ticket);
                        if (ok && context.mounted) Navigator.of(context).maybePop();
                      },
              ),
              const Spacer(),
              KButton(
                label: t('order.pendingDialog.modify'),
                loading: _busy,
                onPressed: _busy
                    ? null
                    : () async {
                        setState(() => _busy = true);
                        final ok = await ref
                            .read(tradeActionsProvider)
                            .modifyOrder(
                              o.ticket,
                              price: double.tryParse(_price),
                              sl: double.tryParse(_sl),
                              tp: double.tryParse(_tp),
                              clearSl: _sl.isEmpty,
                              clearTp: _tp.isEmpty,
                            );
                        if (!mounted) return;
                        setState(() => _busy = false);
                        if (ok && context.mounted) Navigator.of(context).maybePop();
                      },
              ),
            ],
          ),
        ],
      ),
    );
  }
}

// The order ticket = the web order form (apps/terminal/components/order/order-ticket.tsx, dialog variant) in the trade
// room's look (owner design 2026-10, MT5 mobile's "Sell by Market / Buy by Market"): the market and its spread · the
// order type as chips (Market, Buy / Sell Limit, Buy / Sell Stop, Buy / Sell Stop limit, each explained) · price,
// limit price, expiry · the volume stepper (±0.01 / ±0.1, presets, sizing by risk) · S and T with ONE Price / Points
// toggle (the money at each, the side checks) · margin and free margin after · More options (trailing stop, OCO,
// comment, max price change, the full summary) · then big SELL / BUY by market (each names the trade it sends) or
// Place for a pending order. The same checks before sending (validateOrder), the same engine rules and banners.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../../core/notifications/notifications.dart';
import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/market_hours.dart';
import '../core/models.dart';
import '../core/order.dart';
import '../core/terminal_controller.dart';
import '../core/trade_actions.dart';
import '../core/trade_math.dart';
import '../core/workspace.dart';
import '../widgets/kit.dart';
import 'room.dart';
import 'symbol_search.dart';

/// Opens the order ticket for `symbol` with an optional side / type / price filled in (SELL / BUY with one-click off,
/// the ladder, a chart long-press, the quotes).
Future<void> showOrderSheet(BuildContext context, {required String symbol, String? side, String type = 'market', double? price, double? volume}) =>
    showKSheet<void>(
      context,
      expand: true,
      builder: (_) => OrderForm(symbol: symbol, side: side, type: type, price: price, volume: volume),
    );

const List<double> _presets = [0.01, 0.1, 0.5, 1, 2];

/// The ticket's order type chips: (type, side); Market takes its side from the SELL / BUY buttons.
const List<(String, String?)> kTicketTypes = [
  ('market', null),
  ('limit', 'buy'),
  ('limit', 'sell'),
  ('stop', 'buy'),
  ('stop', 'sell'),
  ('stop-limit', 'buy'),
  ('stop-limit', 'sell'),
];

/// A type chip's label: "Market", "Buy limit", "Sell stop limit" (the web's own words, first letter up).
String ticketTypeLabel(T t, String type, String? side) {
  if (type == 'market' || side == null) return t('order.type.market');
  final s = t('order.pending.$side.$type');
  return s.isEmpty ? s : s[0].toUpperCase() + s.substring(1);
}

class OrderForm extends ConsumerStatefulWidget {
  const OrderForm({super.key, required this.symbol, this.side, this.type = 'market', this.price, this.volume, this.onDone});
  final String symbol;
  final String? side;
  final String type;
  final double? price;
  final double? volume;
  final VoidCallback? onDone;

  @override
  ConsumerState<OrderForm> createState() => _OrderFormState();
}

class _OrderFormState extends ConsumerState<OrderForm> {
  late String _symbol = widget.symbol;
  late String _type = widget.type;
  late String? _side = widget.side;
  late String _volume = (widget.volume ?? ref.read(workspaceProvider).lot).toStringAsFixed(2);
  late final TextEditingController _volC = TextEditingController(text: _volume);
  final FocusNode _volFocus = FocusNode();
  String _price = '';
  String _stopLimit = '';

  /// The stops' unit, one for both: price | points.
  String _unit = 'price';
  StopInput _sl = const StopInput(mode: 'price');
  StopInput _tp = const StopInput(mode: 'price');
  bool _trailing = false;
  String _trailPips = '20';
  String _expiry = 'GTC';
  late String _expiryDate = _tomorrow();
  bool _oco = false;
  String _ocoPrice = '';
  String _comment = '';
  bool _more = false;
  bool _calcOpen = false;
  String _riskMode = 'pct';
  String _risk = '1';
  String _riskPips = '25';
  bool _busy = false;

  @override
  void initState() {
    super.initState();
    final spec = ref.read(symbolBookProvider)[_symbol];
    if (widget.price != null && spec != null) _price = fmtPrice(spec.digits, widget.price!);
  }

  @override
  void dispose() {
    _volC.dispose();
    _volFocus.dispose();
    super.dispose();
  }

  static String _tomorrow() {
    final d = DateTime.now().add(const Duration(days: 1));
    return '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';
  }

  bool get _pending => _type != 'market';
  double get _vol {
    final v = double.tryParse(_volume) ?? 0;
    return v < 0.01 ? 0.01 : v;
  }

  void _setVolume(String v) {
    _volume = v;
    if (_volC.text != v && !_volFocus.hasFocus) _volC.text = v;
  }

  /// ±0.01 / ±0.1 on the market's volume step, within its limits.
  void _stepVolume(SymbolSpec s, double d) {
    final next = normalizeVolume(s, _vol + d);
    KHaptics.selection();
    setState(() {
      _volume = fmtVol(next);
      _volC.text = _volume;
    });
  }

  void _changeSymbol(String s) {
    if (s == _symbol) return;
    setState(() {
      _symbol = s;
      _price = '';
      _stopLimit = '';
      _ocoPrice = '';
      if (_unit == 'price') {
        _sl = _sl.copyWith(value: '', on: false);
        _tp = _tp.copyWith(value: '', on: false);
      }
      if (!_pending) _side = null;
    });
  }

  double _entry(String side, TQuote q) {
    if (_pending) {
      final p = double.tryParse(_type == 'stop-limit' && _stopLimit.isNotEmpty ? _stopLimit : _price);
      if (p != null && p > 0) return p;
    }
    return side == 'buy' ? q.ask : q.bid;
  }

  double _pipValue(SymbolSpec s, TQuote q) => pipValuePerLot(s, q.bid, bidOf: ref.read(marketFeedProvider).bidOf) * _vol;

  /// Money at a stop for the current volume (USD), signed: negative for a loss (web stopMoney). By price it needs the
  /// side (the entry); by points it doesn't.
  double? _stopMoney(StopInput st, String which, SymbolSpec s, TQuote q, bool cent) {
    if (!st.on) return null;
    final pv = _pipValue(s, q);
    if (st.mode == 'price') {
      if (_side == null) return null;
      final p = double.tryParse(st.value);
      if (p == null || p <= 0) return null;
      final d = (_side == 'buy' ? p - _entry(_side!, q) : _entry(_side!, q) - p) / s.pipSize;
      return d * pv;
    }
    final d = stopDistancePips(st, pipValue: pv, cent: cent, pipPoints: pipPointsOf(s));
    if (d == null) return null;
    return (which == 'sl' ? -1 : 1) * d * pv;
  }

  bool _wrongSide(StopInput st, String which, TQuote q) {
    if (_side == null || !st.on || st.mode != 'price') return false;
    final p = double.tryParse(st.value);
    if (p == null || p <= 0) return false;
    final below = p < _entry(_side!, q);
    return which == 'sl' ? (_side == 'buy' ? !below : below) : (_side == 'buy' ? below : !below);
  }

  /// A stop typed in the S / T field: on while it has a value.
  void _setStop(String which, String v) => setState(() {
    final next = StopInput(on: v.trim().isNotEmpty, mode: _unit, value: v);
    if (which == 'sl') {
      _sl = next;
    } else {
      _tp = next;
    }
  });

  /// Price ⇄ Points for both stops: what is typed is converted (by the chosen side, else as a buy), else cleared.
  void _flipUnit(SymbolSpec s, TQuote q, bool cent) {
    final to = _unit == 'price' ? 'points' : 'price';
    final side = _side ?? 'buy';
    final pv = _pipValue(s, q);
    final entry = _entry(side, q);
    StopInput conv(StopInput st, String which) {
      if (!st.on) return StopInput(mode: to);
      final p = stopPrice(st, which, side, entry, s, pipValue: pv, cent: cent);
      if (p == null) return StopInput(mode: to);
      if (to == 'price') return StopInput(on: true, mode: to, value: fmtPrice(s.digits, p));
      final pts = ((p - entry).abs() / s.point).round();
      return pts > 0 ? StopInput(on: true, mode: to, value: '$pts') : StopInput(mode: to);
    }

    KHaptics.selection();
    setState(() {
      _sl = conv(_sl, 'sl');
      _tp = conv(_tp, 'tp');
      _unit = to;
    });
  }

  Future<void> _submit(String side) async {
    final spec = ref.read(symbolBookProvider)[_symbol];
    final q = ref.read(marketFeedProvider).quote(_symbol);
    if (spec == null || q == null || _busy) return;
    final t = context.t;
    final st = ref.read(terminalProvider);
    setState(() => _side = side);
    if (_pending && _price.isEmpty) {
      ref.read(notificationsProvider.notifier).toast(NotificationKind.error, t('order.toast.enterPendingPrice'));
      return;
    }
    final cent = st.account?.cent ?? false;
    final pv = pipValuePerLot(spec, q.bid, bidOf: ref.read(marketFeedProvider).bidOf) * _vol;
    final entry = _entry(side, q);
    final req = OrderRequest(
      symbol: _symbol,
      side: side,
      type: _type,
      volume: _vol,
      price: _pending ? double.tryParse(_price) : null,
      stopLimit: _type == 'stop-limit' ? (double.tryParse(_stopLimit) ?? double.tryParse(_price)) : null,
      sl: stopPrice(_sl, 'sl', side, entry, spec, pipValue: pv, cent: cent),
      tp: stopPrice(_tp, 'tp', side, entry, spec, pipValue: pv, cent: cent),
      trailingPips: _trailing ? double.tryParse(_trailPips) : null,
      expiry: _pending ? _expiry : 'GTC',
      expiryDate: _pending && _expiry == 'Date' ? _expiryDate : null,
      comment: _comment.isEmpty ? null : _comment,
      ocoPrice: _pending && _oco && _type != 'stop-limit' && _ocoPrice.isNotEmpty ? double.tryParse(_ocoPrice) : null,
    );
    final bad = validateOrder(
      req,
      spec,
      bid: q.bid,
      ask: q.ask,
      account: st.account,
      readOnly: st.readOnly,
      marketOpen: isMarketOpen(spec),
      delayed: q.delayed,
    );
    if (bad != null) {
      KHaptics.error();
      ref.read(notificationsProvider.notifier).toast(NotificationKind.error, t(bad));
      return;
    }
    setState(() => _busy = true);
    final ok = await ref.read(tradeActionsProvider).placeOrder(req);
    if (!mounted) return;
    setState(() => _busy = false);
    if (ok) {
      ref.read(workspaceProvider.notifier).update((w) => w.copyWith(lot: _vol));
      widget.onDone?.call();
      if (widget.onDone == null) Navigator.of(context).maybePop();
    }
  }

  Future<void> _pickExpiry() async {
    final t = context.t;
    final v = await showKActionSheet<String>(
      context,
      title: t('desk.op.expiry'),
      actions: [
        KAction(label: t('order.expiry.gtc'), value: 'GTC'),
        KAction(label: t('order.expiry.today'), value: 'Today'),
        KAction(label: t('order.expiry.date'), value: 'Date'),
      ],
    );
    if (v == null || !mounted) return;
    setState(() => _expiry = v);
    if (v == 'Date') await _pickDate();
  }

  Future<void> _pickDate() async {
    final now = DateTime.now();
    final d = await showDatePicker(
      context: context,
      initialDate: DateTime.tryParse(_expiryDate) ?? now.add(const Duration(days: 1)),
      firstDate: now,
      lastDate: now.add(const Duration(days: 365)),
    );
    if (d != null && mounted) setState(() => _expiryDate = '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}');
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final spec = ref.watch(symbolBookProvider.select((b) => b[_symbol]));
    final acc = ref.watch(terminalProvider.select((s) => s.account));
    final readOnly = ref.watch(terminalProvider.select((s) => s.readOnly));
    final oneClick = ref.watch(workspaceProvider.select((w) => w.oneClick));
    if (spec == null || acc == null) return const SizedBox(height: 200);
    if (readOnly) {
      return Padding(
        padding: const EdgeInsets.all(24),
        child: KEmptyState(icon: LucideIcons.lock, title: t('order.ticket.readOnlyTitle'), text: t('order.ticket.readOnlyText', {'login': acc.login})),
      );
    }
    final open = isMarketOpen(spec);
    return QuoteBuilder(
      symbol: _symbol,
      throttle: const Duration(milliseconds: 600),
      builder: (context, q) {
        if (q == null || !q.valid) {
          return const Center(
            child: Padding(padding: EdgeInsets.all(40), child: KSkeleton(width: 200)),
          );
        }
        return Column(
          children: [
            Padding(
              padding: const EdgeInsets.fromLTRB(16, 0, 16, 6),
              child: Row(
                children: [
                  Expanded(
                    child: KPressable(
                      key: const ValueKey('ticket-symbol'),
                      pressedScale: 1,
                      semanticLabel: t('chart.toolbar.symbol'),
                      onTap: () => showSymbolSearch(context, onPick: _changeSymbol),
                      child: Row(
                        children: [
                          SymbolAvatar(_symbol),
                          const SizedBox(width: 10),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Row(
                                  children: [
                                    Text(_symbol, style: context.text.headline.copyWith(fontSize: 18, fontWeight: FontWeight.w800)),
                                    const SizedBox(width: 4),
                                    Icon(LucideIcons.chevronDown, size: 14, color: k.fg3),
                                  ],
                                ),
                                Text(
                                  '${spec.name} · ${acc.login} · ${t.dyn('order.mode.${acc.mode}', fallback: acc.mode)}',
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: context.text.footnote.copyWith(fontSize: 11, color: k.fg3),
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  Text(
                    '${t('desk.op.spread')} ${t('order.unit.pts', {'n': spreadPoints(spec, q.bid, q.ask)})}',
                    style: context.text.mono(12, color: k.fg3),
                  ),
                ],
              ),
            ),
            Expanded(
              child: ListView(padding: const EdgeInsets.fromLTRB(16, 0, 16, 12), children: _body(context, spec, acc, q, open)),
            ),
            _footer(context, spec, q, open, oneClick),
          ],
        );
      },
    );
  }

  List<Widget> _body(BuildContext context, SymbolSpec spec, TAccount acc, TQuote q, bool open) {
    final t = context.t;
    final k = context.k;
    final explain = _pending
        ? (_side != null ? t.dyn('desk.op.explain.$_side.${_type == 'stop-limit' ? 'stopLimit' : _type}') : t('desk.op.explain.pickSide'))
        : t('desk.op.marketTip');
    final cent = acc.cent;
    final slMoney = _stopMoney(_sl, 'sl', spec, q, cent);
    final tpMoney = _stopMoney(_tp, 'tp', spec, q, cent);
    final stepPrice = priceStep(spec);
    final m = ref.watch(terminalProvider.select((s) => s.metrics));
    final margin = marginRequired(spec, _vol, q.ask, acc.leverage, bidOf: ref.read(marketFeedProvider).bidOf);
    final short = margin > m.free;
    Widget volButton(String label, double d) => KPressable(
      minSize: 40,
      pressedScale: 0.94,
      semanticLabel: t(d < 0 ? 'app.trader.ticket.less' : 'app.trader.ticket.more', {'n': d.abs() < 0.05 ? '0.01' : '0.1'}),
      onTap: () => _stepVolume(spec, d),
      child: Container(
        width: 44,
        height: 40,
        alignment: Alignment.center,
        decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(12)),
        child: Text(label, style: context.text.mono(12, weight: FontWeight.w700, color: k.fg)),
      ),
    );
    return [
      // order type chips
      SizedBox(
        height: 44,
        child: ListView(
          scrollDirection: Axis.horizontal,
          children: [
            for (final (ty, sd) in kTicketTypes)
              Padding(
                padding: const EdgeInsetsDirectional.only(end: 6),
                child: RoomChip(
                  key: ValueKey('ticket-type-$ty${sd == null ? '' : '-$sd'}'),
                  height: 32,
                  label: ticketTypeLabel(t, ty, sd),
                  selected: _type == ty && (ty == 'market' || _side == sd),
                  onTap: () {
                    KHaptics.selection();
                    setState(() {
                      _type = ty;
                      if (sd != null) _side = sd;
                    });
                  },
                ),
              ),
          ],
        ),
      ),
      if (_pending) ...[
        const SizedBox(height: 4),
        Row(
          children: [
            Expanded(child: Text(explain, style: context.text.footnote.copyWith(color: k.fg2, fontSize: 12))),
            THelp(title: t('desk.op.typeTitle'), text: explain),
          ],
        ),
      ],
      if (!open) ...[
        const SizedBox(height: 8),
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          decoration: BoxDecoration(color: k.warnSoft, borderRadius: BorderRadius.circular(12)),
          child: Row(
            children: [
              Icon(LucideIcons.lock, size: 13, color: k.warn),
              const SizedBox(width: 7),
              Expanded(
                child: Text(t('order.ticket.marketClosedNote', {'symbol': _symbol}), style: context.text.footnote.copyWith(color: k.warn, fontSize: 12)),
              ),
            ],
          ),
        ),
      ],
      if (_pending) ...[
        const SizedBox(height: 8),
        TFieldRow(
          label: Text(_type == 'limit' ? t('desk.op.orderPrice') : t('desk.op.stopPrice')),
          child: SizedBox(
            width: 150,
            child: TStepper(
              value: _price,
              onChanged: (v) => setState(() => _price = v),
              step: stepPrice,
              decimals: spec.digits,
              placeholder: fmtPrice(spec.digits, _side == 'sell' ? q.bid : q.ask),
              semanticLabel: t('order.ticket.orderPrice'),
            ),
          ),
        ),
        if (_type == 'stop-limit') ...[
          const SizedBox(height: 6),
          TFieldRow(
            label: Text(t('desk.op.limitPrice')),
            child: SizedBox(
              width: 150,
              child: TStepper(
                value: _stopLimit,
                onChanged: (v) => setState(() => _stopLimit = v),
                step: stepPrice,
                decimals: spec.digits,
                placeholder: _price.isNotEmpty ? _price : fmtPrice(spec.digits, q.ask),
                semanticLabel: t('order.ticket.stopLimitPrice'),
              ),
            ),
          ),
        ],
        const SizedBox(height: 6),
        TFieldRow(
          label: Text(t('desk.op.expiry')),
          child: KPressable(
            minSize: 32,
            onTap: _pickExpiry,
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                Text(
                  _expiry == 'Date' ? _expiryDate : (_expiry == 'Today' ? t('order.expiry.today') : t('order.expiry.gtc')),
                  style: context.text.mono(12.5, color: k.fg),
                ),
                const SizedBox(width: 4),
                Icon(LucideIcons.chevronDown, size: 14, color: k.fg3),
              ],
            ),
          ),
        ),
      ],
      const SizedBox(height: 8),
      // volume: −0.1 −.01 [lots] +.01 +0.1
      Container(
        key: const ValueKey('ticket-volume'),
        padding: const EdgeInsets.all(6),
        decoration: BoxDecoration(color: k.bg, borderRadius: BorderRadius.circular(16)),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: Row(
            children: [
              volButton('−0.1', -0.1),
              const SizedBox(width: 6),
              volButton('−.01', -0.01),
              Expanded(
                child: Column(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    TextField(
                      controller: _volC,
                      focusNode: _volFocus,
                      textAlign: TextAlign.center,
                      keyboardType: const TextInputType.numberWithOptions(decimal: true),
                      style: context.text.mono(20, weight: FontWeight.w800, color: k.fg),
                      cursorColor: k.ember,
                      decoration: const InputDecoration(isCollapsed: true, border: InputBorder.none),
                      onChanged: (v) => setState(() => _volume = v),
                    ),
                    Text(t('desk.ob.lots'), style: context.text.caption.copyWith(fontSize: 10, color: k.fg3)),
                  ],
                ),
              ),
              volButton('+.01', 0.01),
              const SizedBox(width: 6),
              volButton('+0.1', 0.1),
            ],
          ),
        ),
      ),
      const SizedBox(height: 6),
      Row(
        children: [
          Expanded(
            child: TQuickStrip<double>(
              options: _presets,
              labels: [for (final p in _presets) p < 1 ? '$p' : p.toStringAsFixed(0)],
              selected: _presets.where((p) => (p - _vol).abs() < 1e-9).firstOrNull,
              onPick: (v) => setState(() => _setVolume(v.toStringAsFixed(2))),
            ),
          ),
          const SizedBox(width: 6),
          KPressable(
            minSize: 32,
            semanticLabel: t('desk.op.byRisk'),
            onTap: () => setState(() => _calcOpen = !_calcOpen),
            child: Container(
              width: 32,
              height: 30,
              decoration: BoxDecoration(
                color: _calcOpen ? k.emberSoft : k.surface2,
                borderRadius: BorderRadius.circular(8),
                border: Border.all(color: k.line),
              ),
              child: Icon(LucideIcons.calculator, size: 15, color: _calcOpen ? k.ember : k.fg2),
            ),
          ),
        ],
      ),
      if (_calcOpen) ...[const SizedBox(height: 8), _calculator(context, spec, acc, q)],
      const SizedBox(height: 10),
      // S · T · Price / Points
      Row(
        children: [
          Expanded(
            child: StopField(
              key: const ValueKey('ticket-sl'),
              kind: 'sl',
              value: _sl.value,
              placeholder: _unit == 'price' ? t('desk.op.mode.price') : t('news.instrument.swapType.points'),
              semanticLabel: t('order.ticket.stopLoss'),
              onChanged: (v) => _setStop('sl', v),
            ),
          ),
          const SizedBox(width: 6),
          Expanded(
            child: StopField(
              key: const ValueKey('ticket-tp'),
              kind: 'tp',
              value: _tp.value,
              placeholder: _unit == 'price' ? t('desk.op.mode.price') : t('news.instrument.swapType.points'),
              semanticLabel: t('order.ticket.takeProfit'),
              onChanged: (v) => _setStop('tp', v),
            ),
          ),
          const SizedBox(width: 6),
          KPressable(
            key: const ValueKey('ticket-unit'),
            pressedScale: 0.95,
            semanticLabel: t('app.trader.ticket.unitAria'),
            onTap: () => _flipUnit(spec, q, cent),
            child: Container(
              height: 46,
              padding: const EdgeInsets.symmetric(horizontal: 10),
              alignment: Alignment.center,
              decoration: BoxDecoration(color: k.surface3, borderRadius: BorderRadius.circular(14)),
              child: Text(
                _unit == 'price' ? t('desk.op.mode.price') : t('news.instrument.swapType.points'),
                style: context.text.label.copyWith(fontSize: 11, fontWeight: FontWeight.w700, color: k.fg2),
              ),
            ),
          ),
        ],
      ),
      _stopNote(context, 'sl', spec, q, cent, slMoney),
      _stopNote(context, 'tp', spec, q, cent, tpMoney),
      Padding(
        padding: const EdgeInsets.fromLTRB(4, 6, 4, 0),
        child: Text(
          t('app.trader.ticket.marginFree', {'margin': accMoney(cent, margin), 'free': accMoney(cent, m.free - margin)}),
          key: const ValueKey('ticket-margin'),
          textAlign: TextAlign.end,
          style: context.text.mono(11, color: short ? k.down : k.fg3),
        ),
      ),
      if (short)
        Padding(
          padding: const EdgeInsets.fromLTRB(4, 2, 4, 0),
          child: Text(
            t('desk.op.notEnough'),
            textAlign: TextAlign.end,
            style: context.text.footnote.copyWith(color: k.down, fontWeight: FontWeight.w600, fontSize: 12),
          ),
        ),
      const SizedBox(height: 6),
      // more options
      KPressable(
        pressedScale: 1,
        onTap: () => setState(() => _more = !_more),
        child: SizedBox(
          height: 40,
          child: Row(
            children: [
              Icon(_more ? LucideIcons.chevronDown : LucideIcons.chevronRight, size: 14, color: k.fg3),
              const SizedBox(width: 6),
              Text(
                t('desk.op.more'),
                style: context.text.label.copyWith(color: k.fg2, fontWeight: FontWeight.w600),
              ),
              const SizedBox(width: 6),
              Expanded(
                child: Text(
                  [t('desk.op.trailing'), if (_pending && _type != 'stop-limit') 'OCO', t('desk.op.comment'), t('desk.op.maxDev')].join(' · '),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.footnote.copyWith(color: k.fg3, fontSize: 12),
                ),
              ),
            ],
          ),
        ),
      ),
      if (_more) ...[
        TFieldRow(
          label: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              TSmallSwitch(value: _trailing, onChanged: (v) => setState(() => _trailing = v), semanticLabel: t('desk.op.trailing')),
              const SizedBox(width: 8),
              Flexible(child: Text(t('desk.op.trailing'), maxLines: 1, overflow: TextOverflow.ellipsis)),
            ],
          ),
          help: THelp(title: t('desk.op.trailing'), text: t('desk.op.trailingHint')),
          child: _trailing
              ? SizedBox(
                  width: 110,
                  child: TStepper(
                    value: _trailPips,
                    onChanged: (v) => setState(() => _trailPips = v),
                    step: 1,
                    min: 1,
                    decimals: 0,
                    semanticLabel: t('order.ticket.trailingPips'),
                  ),
                )
              : Text(t('desk.op.off'), style: context.text.footnote.copyWith(color: k.fg3)),
        ),
        if (_pending && _type != 'stop-limit') ...[
          const SizedBox(height: 6),
          TFieldRow(
            label: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                TSmallSwitch(value: _oco, onChanged: (v) => setState(() => _oco = v), semanticLabel: t('desk.op.oco')),
                const SizedBox(width: 8),
                const Text('OCO'),
              ],
            ),
            help: THelp(title: 'OCO', text: t('desk.op.oco')),
            child: _oco
                ? SizedBox(
                    width: 140,
                    child: TStepper(
                      value: _ocoPrice,
                      onChanged: (v) => setState(() => _ocoPrice = v),
                      step: stepPrice,
                      decimals: spec.digits,
                      placeholder: fmtPrice(spec.digits, q.bid),
                      semanticLabel: t('desk.op.ocoPrice'),
                    ),
                  )
                : Text(t('desk.op.off'), style: context.text.footnote.copyWith(color: k.fg3)),
          ),
        ],
        const SizedBox(height: 6),
        TFieldRow(
          label: Text(t('desk.op.comment')),
          child: SizedBox(
            width: 160,
            child: TextField(
              maxLength: 31,
              textAlign: TextAlign.end,
              style: context.text.callout.copyWith(fontSize: 13),
              decoration: InputDecoration(
                isCollapsed: true,
                border: InputBorder.none,
                counterText: '',
                hintText: t('common.optional'),
                hintStyle: context.text.callout.copyWith(fontSize: 13, color: k.fg3),
              ),
              onChanged: (v) => _comment = v,
            ),
          ),
        ),
        const SizedBox(height: 6),
        Consumer(
          builder: (context, ref, _) {
            final dev = ref.watch(workspaceProvider.select((w) => w.maxDeviation));
            return TFieldRow(
              label: Text(t('desk.op.maxDev')),
              help: THelp(title: t('desk.set.maxDeviation'), text: t('desk.set.maxDeviationHint')),
              child: SizedBox(
                width: 130,
                child: TStepper(
                  value: dev == null ? '' : '$dev',
                  placeholder: t('order.ticket.anyPrice'),
                  step: 1,
                  decimals: 0,
                  semanticLabel: t('order.ticket.maxDeviation'),
                  onChanged: (v) => ref
                      .read(workspaceProvider.notifier)
                      .update(
                        (w) =>
                            v.trim().isEmpty ? w.copyWith(clearDeviation: true) : w.copyWith(maxDeviation: (double.tryParse(v) ?? 0).round().clamp(0, 1 << 20)),
                      ),
                ),
              ),
            );
          },
        ),
        const SizedBox(height: 10),
        const KDivider(),
        const SizedBox(height: 6),
        _summary(context, spec, acc, q),
        if (_sl.on && _tp.on && slMoney != null && tpMoney != null && slMoney < 0)
          TSummaryRow(
            label: t('desk.op.ratioLabel'),
            value: Text.rich(
              TextSpan(
                children: [
                  TextSpan(
                    text: accMoney(cent, slMoney, signed: true),
                    style: TextStyle(color: k.down),
                  ),
                  TextSpan(
                    text: ' / ',
                    style: TextStyle(color: k.fg3),
                  ),
                  TextSpan(
                    text: accMoney(cent, tpMoney, signed: true),
                    style: TextStyle(color: k.up),
                  ),
                  TextSpan(
                    text: '  · ${(tpMoney / slMoney.abs()).toStringAsFixed(1)}×',
                    style: TextStyle(color: k.fg2),
                  ),
                ],
              ),
            ),
          ),
      ],
    ];
  }

  /// Under S / T: the money at the stop, its price (points) or its distance (price), and the side checks.
  Widget _stopNote(BuildContext context, String which, SymbolSpec spec, TQuote q, bool cent, double? money) {
    final t = context.t;
    final k = context.k;
    final st = which == 'sl' ? _sl : _tp;
    if (!st.on) return const SizedBox.shrink();
    final tone = which == 'sl' ? k.down : k.up;
    final pv = _pipValue(spec, q);
    final at = _side == null ? null : stopPrice(st, which, _side!, _entry(_side!, q), spec, pipValue: pv, cent: cent);
    final d = st.mode == 'price'
        ? (at != null && _side != null ? (at - _entry(_side!, q)).abs() / spec.pipSize : null)
        : stopDistancePips(st, pipValue: pv, cent: cent, pipPoints: pipPointsOf(spec));
    final bad = _wrongSide(st, which, q);
    return Padding(
      padding: const EdgeInsets.fromLTRB(4, 5, 4, 0),
      child: Wrap(
        spacing: 6,
        children: [
          Text(t(which == 'sl' ? 'chart.line.slHandle' : 'chart.line.tpHandle'), style: context.text.mono(11.5, weight: FontWeight.w800, color: tone)),
          if (money != null)
            Text(
              which == 'sl'
                  ? t('desk.op.risk', {'amount': accMoney(cent, money, signed: true)})
                  : t('desk.op.reward', {'amount': accMoney(cent, money, signed: true)}),
              style: context.text.mono(11.5, weight: FontWeight.w600, color: tone),
            ),
          if (at != null && st.mode != 'price')
            Text('· ${t('desk.op.atPrice', {'price': fmtPrice(spec.digits, at)})}', style: context.text.mono(11.5, color: k.fg3))
          else if (_side == null && st.mode == 'price')
            Text('· ${t('desk.op.pickSideForPrice')}', style: context.text.footnote.copyWith(color: k.fg3, fontSize: 11.5)),
          if (d != null) Text('· ${t('desk.op.pipsAway', {'n': d.toStringAsFixed(1)})}', style: context.text.footnote.copyWith(color: k.fg3, fontSize: 11.5)),
          if (bad)
            SizedBox(
              width: double.infinity,
              child: Text(
                t.dyn('desk.op.check.$which${_side == 'buy' ? 'Buy' : 'Sell'}'),
                style: context.text.footnote.copyWith(color: k.down, fontSize: 11.5),
              ),
            ),
        ],
      ),
    );
  }

  Widget _footer(BuildContext context, SymbolSpec spec, TQuote q, bool open, bool oneClick) {
    final t = context.t;
    final k = context.k;
    final delayed = q.delayed && !_pending;
    String marketText(String side) =>
        t('desk.op.confirm.market', {'side': side == 'buy' ? t('desk.pos.buy') : t('desk.pos.sell'), 'lots': fmtVol(_vol), 'symbol': _symbol});
    // the line over the buttons says what will be sent (or why nothing can be)
    final String caption;
    if (delayed) {
      caption = t('desk.side.delayedTip');
    } else if (!open) {
      caption = t('desk.op.confirm.closed');
    } else if (_busy) {
      caption = t('desk.op.confirm.sending');
    } else if (_side == null) {
      caption = t('desk.op.confirm.pickSide');
    } else if (_pending && _price.isEmpty) {
      caption = t('desk.op.confirm.enterPrice');
    } else if (_pending) {
      caption = t('desk.op.confirm.pending', {
        'label': t('order.pending.$_side.$_type'),
        'lots': fmtVol(_vol),
        'symbol': _symbol,
        'price': fmtPrice(spec.digits, double.tryParse(_price) ?? 0),
      });
    } else {
      caption = marketText(_side!);
    }
    final canMarket = !delayed && open && !_busy;
    final canPlace = open && !_busy && _side != null && _price.isNotEmpty;
    Widget big(String side) {
      final buy = side == 'buy';
      final chosen = _side == side;
      return Expanded(
        child: KPressable(
          key: ValueKey('ticket-$side'),
          pressedScale: 0.97,
          haptic: false,
          semanticLabel: marketText(side),
          onTap: canMarket
              ? () {
                  KHaptics.medium();
                  unawaited(_submit(side));
                }
              : null,
          child: Container(
            height: 56,
            width: double.infinity,
            decoration: BoxDecoration(
              color: canMarket ? (buy ? k.buyFill : k.sellFill) : k.surface3,
              borderRadius: BorderRadius.circular(18),
              border: Border.all(color: chosen && canMarket ? Colors.white.withValues(alpha: 0.7) : Colors.transparent, width: 1.5),
            ),
            child: Column(
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  t(buy ? 'common.buy' : 'common.sell').toUpperCase(),
                  style: context.text.micro.copyWith(fontSize: 11, fontWeight: FontWeight.w800, letterSpacing: 0.4, color: canMarket ? Colors.white : k.fg3),
                ),
                PriceText(buy ? q.ask : q.bid, digits: spec.digits, size: 15, weight: FontWeight.w700, color: canMarket ? Colors.white : k.fg3),
              ],
            ),
          ),
        ),
      );
    }

    return Container(
      padding: const EdgeInsets.fromLTRB(16, 8, 16, 8),
      decoration: BoxDecoration(
        border: Border(top: BorderSide(color: k.line, width: 0.6)),
      ),
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (!_pending) ...[
            Text(
              caption,
              key: const ValueKey('ticket-caption'),
              textAlign: TextAlign.center,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: context.text.footnote.copyWith(color: k.fg2, fontSize: 12),
            ),
            const SizedBox(height: 6),
          ],
          if (_pending)
            KPressable(
              key: const ValueKey('ticket-place'),
              semanticLabel: caption,
              onTap: canPlace ? () => unawaited(_submit(_side!)) : null,
              child: Container(
                height: 56,
                width: double.infinity,
                alignment: Alignment.center,
                padding: const EdgeInsets.symmetric(horizontal: 12),
                decoration: BoxDecoration(
                  color: canPlace ? (_side == 'sell' ? k.sellFill : k.buyFill) : k.surface3,
                  borderRadius: BorderRadius.circular(18),
                ),
                child: _busy
                    ? const SizedBox(width: 18, height: 18, child: CircularProgressIndicator.adaptive(strokeWidth: 2))
                    : Text(
                        caption,
                        key: const ValueKey('ticket-caption'),
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: context.text.headline.copyWith(fontSize: 15, fontWeight: FontWeight.w800, color: canPlace ? Colors.white : k.fg3),
                      ),
              ),
            )
          else
            Row(children: [big('sell'), const SizedBox(width: 8), big('buy')]),
          const SizedBox(height: 4),
          Row(
            children: [
              TSmallSwitch(
                value: oneClick,
                semanticLabel: t('desk.op.oneClick'),
                onChanged: (v) => ref.read(workspaceProvider.notifier).update((w) => w.copyWith(oneClick: v)),
              ),
              const SizedBox(width: 6),
              Text(
                t('desk.op.oneClick'),
                style: context.text.footnote.copyWith(color: k.fg2, fontWeight: FontWeight.w500),
              ),
              const SizedBox(width: 4),
              THelp(title: t('desk.op.oneClick'), text: oneClick ? t('desk.op.oneClickOn') : t('desk.op.oneClickOff')),
              const Spacer(),
              Text(oneClick ? t('trader.oneClick.on') : t('trader.oneClick.off'), style: context.text.caption.copyWith(color: oneClick ? k.ember : k.fg3)),
            ],
          ),
        ],
      ),
    );
  }

  Widget _calculator(BuildContext context, SymbolSpec spec, TAccount acc, TQuote q) {
    final t = context.t;
    final k = context.k;
    final cent = acc.cent;
    final m = ref.read(terminalProvider).metrics;
    final riskUsd = _riskMode == 'pct' ? m.balance * (double.tryParse(_risk) ?? 0) / 100 : (double.tryParse(_risk) ?? 0) / (cent ? 100 : 1);
    final pvLot = pipValuePerLot(spec, q.bid, bidOf: ref.read(marketFeedProvider).bidOf);
    final lots = riskLots(riskUsd: riskUsd, pips: double.tryParse(_riskPips) ?? 1, pipValuePerLot: pvLot);
    return Container(
      padding: const EdgeInsets.all(10),
      decoration: BoxDecoration(
        color: k.surface2.withValues(alpha: 0.6),
        borderRadius: BorderRadius.circular(10),
        border: Border.all(color: k.line),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Text(
                t('desk.op.calc.title'),
                style: context.text.label.copyWith(fontWeight: FontWeight.w600, color: k.fg),
              ),
              const SizedBox(width: 4),
              THelp(text: t('desk.op.calc.text')),
            ],
          ),
          const SizedBox(height: 6),
          TFieldRow(
            label: Text(t('order.calc.risk')),
            child: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                SizedBox(
                  width: 74,
                  child: KSegmented<String>(
                    plain: true,
                    height: 28,
                    values: const ['pct', 'usd'],
                    labels: ['%', cent ? '¢' : r'$'],
                    selected: _riskMode,
                    onChanged: (v) => setState(() => _riskMode = v),
                  ),
                ),
                const SizedBox(width: 6),
                SizedBox(
                  width: 96,
                  child: TStepper(
                    value: _risk,
                    onChanged: (v) => setState(() => _risk = v),
                    step: _riskMode == 'pct' ? 0.25 : 25,
                    decimals: _riskMode == 'pct' ? 2 : 0,
                    semanticLabel: t('order.calc.risk'),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 6),
          TFieldRow(
            label: Text(t('order.calc.slDistancePips')),
            child: SizedBox(
              width: 96,
              child: TStepper(
                value: _riskPips,
                onChanged: (v) => setState(() => _riskPips = v),
                step: 1,
                decimals: 0,
                semanticLabel: t('order.calc.slDistance'),
              ),
            ),
          ),
          TSummaryRow(label: t('order.calc.lots'), value: Text(lots.toStringAsFixed(2))),
          const SizedBox(height: 4),
          KButton(
            label: t('order.calc.apply', {'lots': lots.toStringAsFixed(2), 'sl': _riskPips, 'tp': ((double.tryParse(_riskPips) ?? 0) * 2).toStringAsFixed(0)}),
            variant: KButtonVariant.surface,
            size: KButtonSize.sm,
            expand: true,
            onPressed: () {
              // the stops go in as points (the ticket's Price / Points toggle): the stop-loss distance and twice it
              final pts = pipPointsOf(spec);
              final slPts = ((double.tryParse(_riskPips) ?? 0) * pts).round();
              setState(() {
                _setVolume(lots.toStringAsFixed(2));
                _volC.text = _volume;
                _unit = 'points';
                _sl = StopInput(on: slPts > 0, mode: 'points', value: slPts > 0 ? '$slPts' : '');
                _tp = StopInput(on: slPts > 0, mode: 'points', value: slPts > 0 ? '${slPts * 2}' : '');
                _calcOpen = false;
              });
              ref
                  .read(notificationsProvider.notifier)
                  .toast(
                    NotificationKind.success,
                    t('order.toast.volumeSet', {'lots': lots.toStringAsFixed(2)}),
                    description: t('order.toast.volumeSetDesc', {'amount': accMoney(cent, riskUsd), 'ccy': accCcy(cent), 'pips': _riskPips}),
                  );
            },
          ),
        ],
      ),
    );
  }

  /// Margin, pip value, size, free margin after, swap (web LiveSummary).
  Widget _summary(BuildContext context, SymbolSpec spec, TAccount acc, TQuote q) {
    final t = context.t;
    final k = context.k;
    final m = ref.watch(terminalProvider.select((s) => s.metrics));
    final bidOf = ref.read(marketFeedProvider).bidOf;
    final margin = marginRequired(spec, _vol, q.ask, acc.leverage, bidOf: bidOf);
    final pipV = pipValuePerLot(spec, q.bid, bidOf: bidOf) * _vol;
    final pct = m.free > 0 ? margin / m.free * 100 : double.infinity;
    final short = margin > m.free;
    final ccy = accCcy(acc.cent);
    final size = _vol * spec.contractSize;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TSummaryRow(
          label: t('desk.op.summary.margin'),
          help: THelp(title: t('desk.g.margin.t'), text: t('desk.g.margin')),
          value: Text.rich(
            TextSpan(
              children: [
                TextSpan(
                  text: '${accMoney(acc.cent, margin)} $ccy',
                  style: TextStyle(color: short ? k.down : k.fg),
                ),
                TextSpan(
                  text: '  · ${pct.isFinite ? '${pct < 0.1 ? pct.toStringAsFixed(2) : pct.toStringAsFixed(1)}%' : '—'}',
                  style: TextStyle(color: k.fg3),
                ),
              ],
            ),
          ),
        ),
        TSummaryRow(
          label: t('desk.op.summary.pip'),
          help: THelp(title: t('desk.g.pip.t'), text: t('desk.g.pip')),
          value: Text('${accMoney(acc.cent, pipV)} $ccy'),
        ),
        TSummaryRow(
          label: t('desk.op.summary.size'),
          value: Text.rich(
            TextSpan(
              children: [
                TextSpan(text: Fmt2.compactNumber(size)),
                TextSpan(
                  text: ' ${unitLabel(t, spec)}',
                  style: TextStyle(color: k.fg3),
                ),
              ],
            ),
          ),
        ),
        TSummaryRow(
          label: t('desk.op.summary.free'),
          value: Text('${accMoney(acc.cent, m.free - margin)} $ccy', style: TextStyle(color: short ? k.down : null)),
        ),
        TSummaryRow(
          label: t('desk.sw.title'),
          ltr: false,
          help: THelp(title: t('desk.g.swap.t'), text: t('desk.g.swap')),
          value: Text(swapSummary(t, spec), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 11.5)),
        ),
        if (short)
          Padding(
            padding: const EdgeInsets.only(top: 4),
            child: Text(
              t('desk.op.notEnough'),
              style: context.text.footnote.copyWith(color: k.down, fontWeight: FontWeight.w600),
            ),
          ),
      ],
    );
  }
}


/// "100,000" / "1,250.5" (the order summary's position size).
abstract final class Fmt2 {
  static String compactNumber(double v) {
    final whole = v == v.roundToDouble();
    final s = v.toStringAsFixed(whole ? 0 : 2).replaceAll(RegExp(r'0+$'), '').replaceAll(RegExp(r'\.$'), '');
    final parts = s.split('.');
    final i = parts[0].replaceAllMapped(RegExp(r'(\d)(?=(\d{3})+$)'), (m) => '${m[1]},');
    return parts.length > 1 ? '$i.${parts[1]}' : i;
  }
}

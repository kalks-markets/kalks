// The symbol's specification (web SpecDialog / SymbolInfo, apps/terminal/components/order/right-panel.tsx): today's
// low · high · open · range · spread, the contract (size, digits · pip, tick size, volume min / max / step, margin ·
// profit currency, stops level, execution, expiration) and swaps & sessions, all from the contract specs of
// `GET trade/symbols` and the market feed's day snapshot. Opened by the symbol screen's "Specs".
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../i18n/i18n.dart';
import '../../../ui/ui.dart';
import '../core/market.dart';
import '../core/trade_math.dart';
import '../widgets/kit.dart';
import 'order_sheet.dart' show Fmt2;

Future<void> showSpecSheet(BuildContext context, String symbol) =>
    showKSheet<void>(context, title: context.t('order.spec.title', {'symbol': symbol}), builder: (_) => SpecSheet(symbol: symbol));

class SpecSheet extends ConsumerWidget {
  const SpecSheet({super.key, required this.symbol});
  final String symbol;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final spec = ref.watch(symbolBookProvider.select((b) => b[symbol]));
    if (spec == null) return const SizedBox(height: 120);
    final day = ref.read(marketFeedProvider).day(symbol);
    Widget section(String title) => Padding(
      padding: const EdgeInsets.fromLTRB(2, 14, 2, 4),
      child: Text(
        title.toUpperCase(),
        style: context.text.micro.copyWith(color: k.fg3, letterSpacing: 0.6, fontSize: 11),
      ),
    );
    Widget kv(String key, String value, {Color? color}) => Container(
      constraints: const BoxConstraints(minHeight: 40),
      padding: const EdgeInsets.symmetric(horizontal: 2, vertical: 8),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: k.line, width: 0.6)),
      ),
      child: Row(
        children: [
          Expanded(
            child: Text(key, style: context.text.callout.copyWith(fontSize: 14, color: k.fg3)),
          ),
          const SizedBox(width: 12),
          Flexible(
            child: Text(
              value,
              textAlign: TextAlign.end,
              textDirection: TextDirection.ltr,
              style: context.text.mono(13.5, weight: FontWeight.w600, color: color ?? k.fg),
            ),
          ),
        ],
      ),
    );
    final swapEveryNight = spec.tripleSwapDay == null;
    return ListView(
      shrinkWrap: true,
      padding: const EdgeInsets.fromLTRB(16, 0, 16, 20),
      children: [
        Text('${spec.symbol} · ${spec.name}', style: context.text.footnote.copyWith(color: k.fg3)),
        section(t('order.info.today')),
        QuoteBuilder(
          symbol: symbol,
          throttle: const Duration(milliseconds: 500),
          builder: (context, q) {
            final bid = q?.bid ?? 0;
            final lo = day == null ? null : (bid > 0 && bid < day.low ? bid : day.low);
            final hi = day == null ? null : (bid > day.high ? bid : day.high);
            return Column(
              children: [
                if (lo != null && hi != null) ...[
                  kv(t('order.tick.low'), fmtPrice(spec.digits, lo), color: k.down),
                  kv(t('order.tick.high'), fmtPrice(spec.digits, hi), color: k.up),
                  kv(t('order.info.open'), fmtPrice(spec.digits, day!.open)),
                  kv(t('order.info.range'), t('order.unit.pips', {'n': ((hi - lo) / spec.pipSize).toStringAsFixed(1)})),
                ],
                if (q != null && q.valid) kv(t('order.tick.spread'), t('order.unit.pts', {'n': spreadPoints(spec, q.bid, q.ask)})),
              ],
            );
          },
        ),
        section(t('order.info.contract')),
        kv(t('order.info.contractSize'), Fmt2.compactNumber(spec.contractSize)),
        kv(t('order.info.digitsPip'), '${spec.digits} · ${_num(spec.pipSize)}'),
        kv(t('order.info.tickSize'), _num(spec.point)),
        kv(t('order.info.volumeMinMaxStep'), '${_num(spec.lotMin)} / ${_num(spec.lotMax)} / ${_num(spec.lotStep)}'),
        kv(t('order.info.marginProfitCcy'), '${spec.baseCurrency} · ${spec.profitCurrency}'),
        kv(t('news.instrument.spec.maxLeverage'), '1:${spec.maxLeverage}'),
        kv(t('order.info.stopsLevel'), t('order.unit.pts', {'n': spec.stopsLevelPoints})),
        kv(t('order.info.execution'), t('order.info.executionMarket')),
        kv(t('order.info.expiration'), t('order.info.expirationValue')),
        section(t('order.info.swapsSessions')),
        kv(t('order.info.swapLong'), swapRateText(t, spec.swapLong, spec.swapUnit), color: spec.swapLong >= 0 ? k.up : k.down),
        kv(t('order.info.swapShort'), swapRateText(t, spec.swapShort, spec.swapUnit), color: spec.swapShort >= 0 ? k.up : k.down),
        kv(t('order.info.tripleSwap'), swapEveryNight ? t('desk.sw.everyNight') : t.dyn('order.info.tripleSwapDay.${spec.tripleSwapDay}', fallback: spec.tripleSwapDay!)),
        kv(t('order.info.serverTime'), 'GMT+${serverOffsetSeconds(DateTime.now().toUtc()) ~/ 3600}'),
      ],
    );
  }

  /// A spec number without trailing zeros ("0.0001", "100", "0.01").
  static String _num(double v) {
    var s = v.toStringAsFixed(8);
    s = s.replaceAll(RegExp(r'0+$'), '').replaceAll(RegExp(r'\.$'), '');
    return s;
  }
}

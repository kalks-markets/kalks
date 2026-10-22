// An order as the order sheet builds it, its checks before sending, and the body of `POST trade/orders` (web:
// apps/terminal/lib/engine/actions.ts toBody / placeOrder, components/order/order-ticket.tsx).
import 'package:flutter/foundation.dart';

import 'models.dart';
import 'trade_math.dart';

/// market | limit | stop | stop-limit
const List<String> kOrderTypes = ['market', 'limit', 'stop', 'stop-limit'];

@immutable
class OrderRequest {
  const OrderRequest({
    required this.symbol,
    required this.side,
    required this.type,
    required this.volume,
    this.price,
    this.stopLimit,
    this.sl,
    this.tp,
    this.trailingPips,
    this.expiry = 'GTC',
    this.expiryDate,
    this.comment,
    this.ocoPrice,
  });

  final String symbol;

  /// buy | sell
  final String side;

  /// market | limit | stop | stop-limit
  final String type;
  final double volume;

  /// Pending: the order (stop) price; stop-limit: the limit price.
  final double? price, stopLimit;
  final double? sl, tp;

  /// Trailing stop distance in pips (0 / null = off).
  final double? trailingPips;

  /// GTC | Today | Date (pending orders)
  final String expiry;

  /// YYYY-MM-DD when expiry is Date.
  final String? expiryDate;
  final String? comment;

  /// Pending (limit / stop): an OCO twin on the other side at this price.
  final double? ocoPrice;

  bool get pending => type != 'market';
  bool get buy => side == 'buy';

  /// The engine's order type name.
  String get engineType => type == 'stop-limit' ? 'stop_limit' : type;
}

/// The first reason an order can't be sent, as a translation key (null when it can). Mirrors the engine's checks the
/// app can know before sending: volume limits and step, a pending price, stops on the right side of the entry, the
/// comment length (31), the account's restrictions and the market state.
String? validateOrder(
  OrderRequest o,
  SymbolSpec s, {
  required double bid,
  required double ask,
  TAccount? account,
  bool readOnly = false,
  bool marketOpen = true,
  bool delayed = false,
}) {
  if (readOnly) return 'order.reject.read_only';
  if (account != null) {
    if (account.tradingDisabled) return 'order.reject.trading_disabled';
    if (account.closeOnly) return 'order.reject.close_only';
  }
  if (!marketOpen) return 'order.reject.market_closed';
  if (delayed && !o.pending) return 'order.reject.stale_price';
  if (!o.volume.isFinite || o.volume < s.lotMin - 1e-9 || o.volume > s.lotMax + 1e-9) return 'order.reject.invalid_volume';
  final maxLot = account?.maxLot;
  if (maxLot != null && maxLot > 0 && o.volume > maxLot + 1e-9) return 'order.reject.max_lot';
  final step = s.lotStep > 0 ? s.lotStep : 0.01;
  final steps = o.volume / step;
  if ((steps - steps.roundToDouble()).abs() > 1e-6) return 'order.reject.invalid_volume';
  if (o.pending) {
    final p = o.price;
    if (p == null || !(p > 0)) return 'order.toast.enterPendingPrice';
    if (o.type == 'stop-limit' && o.stopLimit != null && !(o.stopLimit! > 0)) return 'order.reject.invalid_stop_limit';
    if (o.expiry == 'Date' && (o.expiryDate == null || DateTime.tryParse(o.expiryDate!) == null)) return 'order.reject.invalid_expiry';
  }
  final entry = entryPrice(o, bid: bid, ask: ask);
  if (o.sl != null && o.sl! > 0) {
    if (o.buy ? o.sl! >= entry : o.sl! <= entry) return 'order.reject.invalid_sl';
  }
  if (o.tp != null && o.tp! > 0) {
    if (o.buy ? o.tp! <= entry : o.tp! >= entry) return 'order.reject.invalid_tp';
  }
  if ((o.comment ?? '').length > 31) return 'order.reject.validation';
  return null;
}

/// The price the order opens at: the ask for a buy, the bid for a sell; a pending order's own (stop-limit: limit) price.
double entryPrice(OrderRequest o, {required double bid, required double ask}) {
  if (o.pending) {
    final p = o.type == 'stop-limit' ? (o.stopLimit ?? o.price) : o.price;
    if (p != null && p > 0) return p;
  }
  return o.buy ? ask : bid;
}

/// The body of `POST trade/orders` (web toBody): prices rounded to the digits, trailing in points, the comment cut to
/// 31 characters, `requestedPrice` + `deviationPoints` for market orders when a max deviation is set (MT5 deviation:
/// the engine requotes instead of filling past it).
Map<String, Object?> orderBody(
  OrderRequest o,
  SymbolSpec s, {
  required String clientOrderId,
  int? deviationPoints,
  double? bid,
  double? ask,
  String source = 'manual',
}) {
  double? r(double? v) => v == null || !v.isFinite || v <= 0 ? null : roundPrice(s.digits, v);
  final b = <String, Object?>{
    'symbol': o.symbol,
    'side': o.side,
    'type': o.engineType,
    'volume': double.parse(o.volume.toStringAsFixed(2)),
    'sl': ?r(o.sl),
    'tp': ?r(o.tp),
    if (o.trailingPips != null && o.trailingPips! > 0) 'trailingPoints': (o.trailingPips! * s.pipSize / s.point).round().clamp(1, 1 << 30),
    if (o.comment != null && o.comment!.isNotEmpty) 'comment': o.comment!.length > 31 ? o.comment!.substring(0, 31) : o.comment,
    'source': source,
    'clientOrderId': clientOrderId,
  };
  if (o.pending) {
    b['price'] = r(o.price);
    if (o.type == 'stop-limit') b['stopLimit'] = r(o.stopLimit ?? o.price);
    b['expiry'] = o.expiry == 'Date' && o.expiryDate != null ? o.expiryDate : o.expiry;
  } else if (deviationPoints != null && bid != null && ask != null) {
    b['deviationPoints'] = deviationPoints;
    b['requestedPrice'] = o.buy ? ask : bid;
  }
  return b;
}

/// The SL / TP of the order sheet, set by price, pips or money (web StopState).
@immutable
class StopInput {
  const StopInput({this.on = false, this.mode = 'pips', this.value = ''});
  final bool on;

  /// price | points | pips | money (the order ticket types price or points; the risk calculator sets pips)
  final String mode;
  final String value;

  StopInput copyWith({bool? on, String? mode, String? value}) => StopInput(on: on ?? this.on, mode: mode ?? this.mode, value: value ?? this.value);
}

/// Distance in pips of a stop typed as points, pips or money (null for price mode). `pipValue` = USD per pip for the
/// volume; `pipPoints` = points in a pip (pip size / point, 10 on 5-digit FX); money is typed in the account currency
/// (USC on cent accounts).
double? stopDistancePips(StopInput st, {required double pipValue, required bool cent, double pipPoints = 10}) {
  final n = double.tryParse(st.value);
  if (n == null || !(n > 0)) return null;
  if (st.mode == 'pips') return n;
  if (st.mode == 'points') return n / (pipPoints <= 0 ? 1 : pipPoints);
  if (st.mode == 'money') return n / (cent ? 100 : 1) / (pipValue <= 0 ? 1e-9 : pipValue);
  return null;
}

/// The price of an SL ('sl') or TP ('tp') for `side` opening at `entry` (web stopPrice).
double? stopPrice(StopInput st, String which, String side, double entry, SymbolSpec s, {required double pipValue, required bool cent}) {
  if (!st.on) return null;
  if (st.mode == 'price') {
    final p = double.tryParse(st.value);
    return p != null && p > 0 ? p : null;
  }
  final d = stopDistancePips(st, pipValue: pipValue, cent: cent, pipPoints: pipPointsOf(s));
  if (d == null) return null;
  final dir = (which == 'tp' ? 1 : -1) * (side == 'buy' ? 1 : -1);
  return roundPrice(s.digits, entry + dir * d * s.pipSize);
}

/// Lots for a risk of `riskUsd` over `pips` (web order calculator): floor to 0.01, at least 0.01.
double riskLots({required double riskUsd, required double pips, required double pipValuePerLot}) {
  final per = (pips <= 0 ? 1 : pips) * pipValuePerLot;
  if (per <= 0) return 0.01;
  final lots = (riskUsd / per * 100).floorToDouble() / 100;
  return lots < 0.01 ? 0.01 : lots;
}

/// Points in one pip of a market (pip size / point: 10 on 5-digit FX, 1 when the pip is the point).
double pipPointsOf(SymbolSpec s) => s.point > 0 && s.pipSize > 0 ? s.pipSize / s.point : 1;

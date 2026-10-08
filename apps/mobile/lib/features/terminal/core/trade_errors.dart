// Rejected trade requests in the reader's language (web: apps/terminal/lib/engine/map.ts rejectReason + actions.ts
// fail): a catalogue market on a live account says so plainly (`symbol_demo_only` -> desk.trade.demoOnly), engine codes
// map to `order.reject.<code>` (Market closed, Not enough money, No prices for stale_price…), the engine's own sentence
// is the detail ("Stop loss must be below 83235.95").
import '../../../core/api/api_error.dart';
import '../../../i18n/t.dart';

/// MT5-style English reasons (web REJECT), for the journal and as the fallback text.
const Map<String, String> rejectReasons = {
  'market_closed': 'Market closed',
  'no_price': 'No prices',
  'stale_price': 'No prices',
  'no_money': 'Not enough money',
  'insufficient_funds': 'Not enough money',
  'invalid_volume': 'Invalid volume',
  'invalid_price': 'Invalid price',
  'invalid_sl': 'Invalid stops',
  'invalid_tp': 'Invalid stops',
  'invalid_expiry': 'Invalid expiration',
  'invalid_oco': 'Invalid OCO order',
  'max_lot': 'Volume limit reached',
  'close_only': 'Only position closing is allowed',
  'trading_disabled': 'Trading is disabled',
  'account_status': 'Account is disabled',
  'symbol_halted': 'Trading is halted on this symbol',
  'symbol_close_only': 'Only position closing is allowed on this symbol',
  'not_hedging': 'Close By needs a hedging account',
  'product_mismatch': "This account doesn't trade this product",
  'invalid_close_by': 'Invalid Close By',
  'requote': 'Requote',
  'read_only': 'Trading is disabled (investor password)',
  'forbidden': 'Not allowed',
  'positions_open': 'Positions are open',
  'refill_limit': 'No refills left today',
  'refill_not_needed': 'Balance is already at the initial amount',
  'demo_account': 'Demo accounts only',
  'rate_limited': 'Too many requests',
  'unavailable': 'No connection with the trade server',
  'network': 'No connection with the trade server',
  'engine_error': 'Trade server error',
  'session_expired': 'Session expired',
  'unauthorized': 'Not logged in',
  'not_found': 'Not found',
  'no_change': 'No changes',
  'validation': 'Invalid request',
  'bad_request': 'Invalid request',
  'duplicate': 'Duplicate request',
  'copy_managed': 'Copied position',
  'copy_account': 'Copy account',
  'pamm_account': 'PAMM fund account',
};

/// A rejection as the terminal shows it: a short title and the detail under it.
class TradeRejection {
  const TradeRejection(this.code, this.title, this.detail);
  final String code;
  final String title;

  /// The engine's own sentence when it adds something ("Stop loss must be below 83235.95"), else empty.
  final String detail;
}

/// The title of a rejected request (web actions.ts fail): `symbol_demo_only` -> "Live trading for this market isn't
/// enabled yet"; `symbol_unavailable` -> "This market isn't available on your account"; others `order.reject.<code>`.
TradeRejection tradeRejection(ApiException e, T t) {
  final code = e.code == 'network' ? 'unavailable' : e.code;
  final reason = rejectReasons[code] ?? (e.message.isNotEmpty ? e.message : code.replaceAll('_', ' '));
  final String title;
  if (code == 'symbol_demo_only') {
    title = t('desk.trade.demoOnly');
  } else if (code == 'symbol_unavailable' || code == 'symbol_not_found') {
    title = t('desk.trade.unavailable');
  } else {
    title = t.dyn('order.reject.$code', fallback: reason);
  }
  final m = e.message;
  final detail = m.isNotEmpty && m != reason && m != title && !RegExp(r'^HTTP \d+$').hasMatch(m) && code != 'unavailable' ? m : '';
  return TradeRejection(code, title, detail);
}

// Engine / options-service rejections → what the client reads (web: apps/terminal/lib/options/errors.ts). The codes
// come from the engine's options API and the options service; unknown codes fall back to the server's message.
import '../../../../core/api/api_error.dart';
import '../../../../i18n/t.dart';

const Set<String> optionErrorCodes = {
  'options_disabled',
  'not_eligible',
  'market_closed',
  'cutoff',
  'closed',
  'series_halted',
  'close_only',
  'limit_contracts',
  'insufficient_cash',
  'insufficient_margin',
  'stale_prices',
  'no_price',
  'read_only',
  'unavailable',
  'session_expired',
  'price_out_of_band',
  'would_take',
  'reduce_only',
  'fok_not_filled',
  'self_trade',
  'settling',
  'rate_limited',
  'limit_orders',
  'invalid_price',
  'invalid_trigger',
  'post_only_gtc',
  'bad_expiry',
  'no_liquidity',
  'book_closed',
  'quote_expired',
  'rfq_expired',
  'price_moved',
  'ioc_remainder',
  'expiry',
  'session_reset',
  'not_found',
  'series_cancel_only',
  'series_closed',
  'rfq_underlyings',
  'kalks_quoted',
  'mixed_venue',
  'book_venue',
  // an option order on a CFD account (options trade in an Options account)
  'product_mismatch',
};

/// Rejections of a strategy request for quote that read better in that context.
const Set<String> _rfqErrorCodes = {'quote_expired', 'price_moved', 'rfq_expired', 'reduce_only', 'self_trade', 'no_price', 'no_liquidity', 'rejected'};

/// The app's network errors read like the web's "unavailable".
String optCode(ApiException e) => e.code == 'network' || e.status == 0 ? 'unavailable' : e.code;

/// Translated text of a rejection code (or the server's message when the code is unknown).
String optionErrorText(T t, String code, [String? message]) {
  if (optionErrorCodes.contains(code)) return t.dyn('trader.opt.err.$code', fallback: message ?? code);
  return (message != null && message.isNotEmpty) ? message : code.replaceAll('_', ' ');
}

String errText(T t, ApiException e) => optionErrorText(t, optCode(e), e.message);

/// A strategy (RFQ / strategy close) rejection in plain words.
String rfqErrorText(T t, String code, [String? message]) {
  if (_rfqErrorCodes.contains(code)) return t.dyn('trader.opt.rfq.err.$code', fallback: message ?? code);
  return optionErrorText(t, code, message);
}

/// The price only went stale: a fresh quote fixes it.
bool rfqRequotable(String code) => code == 'quote_expired' || code == 'price_moved' || code == 'no_price' || code == 'no_liquidity';

/// The rejection asks the client to read the options intro in the Client Area first (a friendly note).
bool needsOnboarding(String code) => code == 'not_eligible';

/// The module isn't available here yet: switched off for this broker / account kind (404 options_disabled), or the
/// engine doesn't serve its options routes yet (404 / 405 / 501).
bool isLaunchingSoon(ApiException e) {
  if (e.code == 'options_disabled') return true;
  if (e.code == 'not_eligible') return false;
  return e.status == 404 || e.status == 405 || e.status == 501;
}

/// The engine doesn't serve the order book (yet): keep the house-priced flow.
bool bookMissing(ApiException e) {
  if (e.code == 'book_disabled' || e.code == 'book_not_active') return true;
  return (e.status == 404 && (e.code == 'not_found' || e.code == 'engine_error' || e.code == 'error')) || e.status == 405 || e.status == 501;
}

/// A fresh client order id (the engine's idempotency key, 8–64 characters).
String clientOrderId(String prefix) {
  final now = DateTime.now().microsecondsSinceEpoch;
  final r = (now * 2654435761) & 0xffffffffff;
  return '$prefix${now.toRadixString(36)}${r.toRadixString(36)}';
}

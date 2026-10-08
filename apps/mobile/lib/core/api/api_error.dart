// Errors of the mobile API (docs/MOBILE-API.md §3): every error body is {"error": {"code", "message", …}}, sometimes
// with `field`, `attempts_left` or `retry_after`. Branch on `code`; the server's `message` is plain English and safe
// to show, and the codes the web knows are shown in the reader's language (localizeError, the port of
// apps/crm/lib/auth-client.ts).
import '../../i18n/t.dart';

class ApiException implements Exception {
  const ApiException({required this.status, required this.code, required this.message, this.field, this.retryAfter, this.attemptsLeft, this.data});

  /// HTTP status (0: no answer — network or timeout).
  final int status;
  final String code;

  /// Plain-English text from the server (or the app's own for network errors).
  final String message;

  /// The invalid input (422 validation).
  final String? field;

  /// Seconds to wait (429 rate_limited, 409 locked).
  final int? retryAfter;

  /// Wrong code: tries left.
  final int? attemptsLeft;

  /// The whole error body, for codes with extra fields.
  final Map<String, dynamic>? data;

  /// No answer from the server.
  bool get isNetwork => status == 0;

  /// The session is dead (expired, signed out elsewhere, blocked): go to sign-in.
  bool get isUnauthorized => status == 401 && code == 'unauthorized';

  /// Run the step-up dialog, then retry with `stepup_token`.
  bool get isStepUp => code == 'stepup_required' || code == 'stepup_invalid';

  /// The broker's maintenance mode: show the maintenance screen.
  bool get isMaintenance => status == 503 && code == 'maintenance';

  /// The broker switched the module off: hide it.
  bool get isModuleDisabled => code == 'module_disabled';

  /// A view-only login or a read-only staff session tried a change.
  bool get isReadOnly => code == 'viewer_read_only' || code == 'viewer_scope' || code == 'staff_read_only' || code == 'read_only';

  bool get isRateLimited => status == 429;

  /// Kalks Trader: open the account again (`trade/sessions`) or ask for the password.
  bool get isTradeSessionEnded => code == 'session_expired' || code == 'trade_session_required';

  /// Kalks Trader: the stored trade token belongs to someone else; drop it.
  bool get isTradeSessionForeign => code == 'trade_session_foreign';

  @override
  String toString() => 'ApiException($status $code: $message)';

  /// Parses an error body; anything unexpected becomes a generic error with the status.
  factory ApiException.fromResponse(int status, Object? body, {String? retryAfterHeader}) {
    Map<String, dynamic>? err;
    if (body is Map && body['error'] is Map) err = (body['error'] as Map).cast<String, dynamic>();
    final code = err?['code'] is String ? err!['code'] as String : (status >= 500 ? 'unavailable' : (status == 401 ? 'unauthorized' : 'error'));
    final message = err?['message'] is String ? err!['message'] as String : _defaultMessage(status);
    final retry = _int(err?['retry_after']) ?? int.tryParse(retryAfterHeader ?? '');
    return ApiException(
      status: status,
      code: code,
      message: message,
      field: err?['field'] as String?,
      retryAfter: retry,
      attemptsLeft: _int(err?['attempts_left']),
      data: err,
    );
  }

  static int? _int(Object? v) => v is num ? v.round() : (v is String ? int.tryParse(v) : null);

  static String _defaultMessage(int status) => switch (status) {
    401 => 'Please sign in.',
    403 => 'Not allowed.',
    404 => 'Not found.',
    429 => 'Too many requests. Please wait a moment.',
    >= 500 => 'This service is temporarily unavailable. Please try again shortly.',
    _ => 'Something went wrong. Please try again.',
  };

  static const ApiException network = ApiException(status: 0, code: 'network', message: "Can't reach Kalks. Check your connection and try again.");
}

/// Gateway validation texts -> message keys (apps/crm/lib/auth-client.ts MESSAGE_KEYS).
const Map<String, String> _messageKeys = {
  'Enter your email address.': 'auth.apiError.emailRequired',
  'Email address is too long.': 'auth.apiError.emailTooLong',
  'Enter a valid email address.': 'auth.apiError.emailInvalid',
  'Use at least 8 characters.': 'auth.apiError.pwMin',
  'Use at most 128 characters.': 'auth.apiError.pwMax',
  'Add an uppercase letter.': 'auth.apiError.pwUpper',
  'Add a lowercase letter.': 'auth.apiError.pwLower',
  'Add a number.': 'auth.apiError.pwNumber',
  'Add a symbol such as ! # @ or %.': 'auth.apiError.pwSymbol',
  'Choose a country dial code.': 'auth.apiError.dialCode',
  'Choose your country of residence.': 'auth.apiError.country',
  'Enter a valid date of birth.': 'auth.apiError.dob',
  'You must be at least 18 years old to open an account.': 'auth.apiError.age',
  'Enter a valid phone number.': 'auth.apiError.phoneInvalid',
  'Enter your phone number.': 'auth.apiError.phoneRequired',
  'Name is too long.': 'auth.apiError.nameTooLong',
  'Use letters only.': 'auth.apiError.nameLetters',
  "Referral code looks wrong. Leave it empty if you don't have one.": 'auth.apiError.referral',
  'Enter your password.': 'auth.apiError.passwordRequired',
  'Please confirm you are over 18 and accept the terms.': 'auth.apiError.terms',
  'This code was sent for a different change. Request a new code.': 'auth.apiError.codeOtherChange',
  'Use the reset form for this code.': 'auth.apiError.codeUseReset',
};

/// Friendly texts for service codes the web translates (trading / wallet FRIENDLY maps, common).
const Map<String, String> _codeKeys = {
  'positions_open': 'accounts.error.positions_open',
  'refill_limit': 'accounts.error.refill_limit',
  'refill_not_needed': 'accounts.error.refill_not_needed',
  'account_limit': 'accounts.error.account_limit',
  'invalid_leverage': 'accounts.error.invalid_leverage',
  'insufficient_funds': 'wallet.error.insufficientFunds',
};

/// The message to show for `e` in the reader's language (port of localizeError + the BFF clients' FRIENDLY maps).
/// Known codes and validation texts map to catalog keys; anything unknown keeps the server's wording.
String localizeError(ApiException e, T t) {
  final n = e.attemptsLeft ?? 0;
  final secs = (e.retryAfter ?? 0) < 1 ? 1 : e.retryAfter!;
  switch (e.code) {
    case 'invalid_credentials':
    case 'account_disabled':
    case 'account_suspended':
    case 'email_taken':
    case 'code_expired':
    case 'unauthorized':
    case 'internal':
    case 'unknown':
      return t.dyn('auth.apiError.${e.code}', fallback: e.message);
    case 'network':
      return t('auth.apiError.network');
    case 'invalid_code':
      return n > 0 ? t.dyn('auth.apiError.invalidCode', fallback: e.message, vars: {'count': n}) : t.dyn('auth.apiError.tooManyCodes', fallback: e.message);
    case 'locked':
      final minutes = (secs / 60).ceil() < 1 ? 1 : (secs / 60).ceil();
      return t.dyn('auth.apiError.locked', fallback: e.message, vars: {'count': minutes, 'minutes': minutes});
    case 'rate_limited':
      return t.dyn('auth.apiError.rateLimited', fallback: e.message, vars: {'seconds': secs});
    case 'unavailable':
      return t('common.unavailable');
    case 'module_disabled':
      // the server's own sentence (the module it refused), else the "not available" page's text
      return e.message.trim().isNotEmpty ? e.message : t('shell.system.unavailable.text');
  }
  final codeKey = _codeKeys[e.code];
  if (codeKey != null && t.has(codeKey)) return t(codeKey);
  final key = _messageKeys[e.message];
  if (key != null) return t.dyn(key, fallback: e.message);
  return e.message;
}

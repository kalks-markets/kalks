// The API client against docs/MOBILE-API.md: the headers every call carries (and never a cookie), error mapping,
// a dead session, maintenance and a switched-off module reported once, and the localized messages.
import 'dart:convert';
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/api/api_client.dart';
import 'package:kalks/core/api/api_error.dart';
import 'package:kalks/i18n/t.dart';

/// Records the request and answers with a fixed status and body.
class FakeAdapter implements HttpClientAdapter {
  FakeAdapter(this.respond);
  final (int, Object?, Map<String, List<String>>) Function(RequestOptions o) respond;
  final List<RequestOptions> requests = [];
  bool fail = false;

  @override
  Future<ResponseBody> fetch(RequestOptions o, Stream<Uint8List>? requestStream, Future<void>? cancelFuture) async {
    requests.add(o);
    if (fail) throw DioException.connectionError(requestOptions: o, reason: 'offline');
    final (status, body, headers) = respond(o);
    return ResponseBody.fromString(
      body == null ? '' : jsonEncode(body),
      status,
      headers: {
        Headers.contentTypeHeader: [Headers.jsonContentType],
        ...headers,
      },
    );
  }

  @override
  void close({bool force = false}) {}
}

Map<String, Object?> _err(String code, String message, [Map<String, Object?> extra = const {}]) => {
  'error': {'code': code, 'message': message, ...extra},
};

void main() {
  String? token = 'a' * 43;
  late FakeAdapter adapter;
  final dead = <ApiException>[];
  final maintenance = <ApiException>[];
  final disabled = <ApiException>[];
  final minted = <String>[];

  ApiClient client() => ApiClient(
    baseUrl: 'https://app.kalkstrade.com/api/mobile',
    adapter: adapter,
    context: ApiContext(
      token: () => token,
      deviceId: () async => 'device-1234567890abcdef',
      locale: () => 'ar',
      appVersion: '1.0.0+1',
      userAgent: 'KalksApp/1.0.0 (Android 15; Pixel 8)',
    ),
    onSessionDead: dead.add,
    onMaintenance: maintenance.add,
    onModuleDisabled: disabled.add,
    onDeviceMinted: minted.add,
  );

  setUp(() {
    token = 'a' * 43;
    dead.clear();
    maintenance.clear();
    disabled.clear();
    minted.clear();
  });

  test('every call carries the bearer, device, platform, version and locale headers, never a cookie', () async {
    adapter = FakeAdapter((_) => (200, {'accounts': []}, const {}));
    final j = await client().get<Map<String, dynamic>>('trading/accounts', query: {'limit': 5, 'skip': null});
    expect(j['accounts'], isEmpty);
    final o = adapter.requests.single;
    expect(o.uri.toString(), 'https://app.kalkstrade.com/api/mobile/trading/accounts?limit=5');
    expect(o.headers['Authorization'], 'Bearer ${'a' * 43}');
    expect(o.headers['X-Kalks-Device'], 'device-1234567890abcdef');
    expect(o.headers['X-Kalks-Platform'], 'android');
    expect(o.headers['X-Kalks-App-Version'], '1.0.0+1');
    expect(o.headers['X-Kalks-Locale'], 'ar');
    expect(o.headers['User-Agent'], 'KalksApp/1.0.0 (Android 15; Pixel 8)');
    expect(o.headers.keys.map((k) => k.toLowerCase()), isNot(contains('cookie')));
  });

  test('sign-in calls go without the bearer; trade and step-up tokens ride as headers', () async {
    adapter = FakeAdapter((_) => (200, {'status': 'otp_required', 'device': 'minted-device-abcdef12'}, const {}));
    await client().post<Map<String, dynamic>>('auth/login', body: {'email': 'a@b.c', 'password': 'x'}, auth: false);
    expect(adapter.requests.last.headers.containsKey('Authorization'), isFalse);
    expect(adapter.requests.last.headers['content-type'], contains('application/json'));
    expect(minted, ['minted-device-abcdef12']);
    await client().post<Map<String, dynamic>>('trade/orders', body: {'symbol': 'EURUSD'}, tradeToken: 'kt1.s.x', stepupToken: 'su1');
    expect(adapter.requests.last.headers['X-Kalks-Trade'], 'kt1.s.x');
    expect(adapter.requests.last.headers['X-Kalks-Stepup'], 'su1');
  });

  test('errors map to ApiException with code, field, retry_after and attempts_left', () async {
    adapter = FakeAdapter(
      (o) => switch (o.path) {
        'auth/verify-email' => (400, _err('invalid_code', 'Wrong code.', {'attempts_left': 3}), const {}),
        'auth/register' => (422, _err('validation', 'Enter a valid email address.', {'field': 'email'}), const {}),
        'auth/resend' => (
          429,
          _err('rate_limited', 'Slow down.'),
          const {
            'retry-after': ['42'],
          },
        ),
        'wallet/withdrawals' => (403, _err('stepup_required', 'Confirm with the code.'), const {}),
        _ => (502, null, const {}),
      },
    );
    final c = client();
    Future<ApiException> fail(Future<Object?> f) async {
      try {
        await f;
      } on ApiException catch (e) {
        return e;
      }
      throw StateError('no error');
    }

    final code = await fail(c.post<Object?>('auth/verify-email', auth: false));
    expect([code.status, code.code, code.attemptsLeft], [400, 'invalid_code', 3]);
    final v = await fail(c.post<Object?>('auth/register', auth: false));
    expect([v.code, v.field], ['validation', 'email']);
    final r = await fail(c.post<Object?>('auth/resend', auth: false));
    expect([r.isRateLimited, r.retryAfter], [true, 42]);
    final s = await fail(c.post<Object?>('wallet/withdrawals'));
    expect(s.isStepUp, isTrue);
    final u = await fail(c.get<Object?>('anything'));
    expect([u.status, u.code], [502, 'unavailable']);
  });

  test('a dead session on a signed-in call is reported; a wrong password is not', () async {
    adapter = FakeAdapter(
      (o) => o.path == 'auth/login'
          ? (401, _err('invalid_credentials', 'Wrong email or password.'), const {})
          : (401, _err('unauthorized', 'Please sign in.'), const {}),
    );
    final c = client();
    await expectLater(c.post<Object?>('auth/login', auth: false), throwsA(isA<ApiException>()));
    expect(dead, isEmpty);
    await expectLater(c.get<Object?>('auth/me'), throwsA(isA<ApiException>().having((e) => e.isUnauthorized, 'isUnauthorized', true)));
    expect(dead, hasLength(1));
    // no bearer (already signed out): nothing to report
    token = null;
    await expectLater(c.get<Object?>('auth/me'), throwsA(isA<ApiException>()));
    expect(dead, hasLength(1));
  });

  test('maintenance (503) is reported; no network becomes a network error', () async {
    adapter = FakeAdapter((_) => (503, _err('maintenance', 'Back soon.'), const {}));
    await expectLater(client().get<Object?>('wallet/overview'), throwsA(isA<ApiException>().having((e) => e.isMaintenance, 'isMaintenance', true)));
    expect(maintenance, hasLength(1));
    adapter.fail = true;
    await expectLater(client().get<Object?>('wallet/overview'), throwsA(isA<ApiException>().having((e) => e.isNetwork, 'isNetwork', true)));
  });

  test('a switched-off module (403 module_disabled) is reported, with the server\'s sentence', () async {
    adapter = FakeAdapter((_) => (403, _err('module_disabled', 'Options trading is not available on your account.'), const {}));
    await expectLater(
      client().get<Object?>('trade/options/chain'),
      throwsA(isA<ApiException>().having((e) => e.isModuleDisabled, 'isModuleDisabled', true).having((e) => e.status, 'status', 403)),
    );
    expect(disabled.single.message, 'Options trading is not available on your account.');
    expect(maintenance, isEmpty);
    expect(dead, isEmpty);
    // any other 403 is not
    adapter = FakeAdapter((_) => (403, _err('viewer_read_only', 'View-only.'), const {}));
    await expectLater(client().post<Object?>('wallet/withdrawals'), throwsA(isA<ApiException>()));
    expect(disabled, hasLength(1));
  });

  group('localizeError (apps/crm/lib/auth-client.ts)', () {
    final t = T('en', null, {
      'auth.apiError.invalid_credentials': 'Wrong email or password.',
      'auth.apiError.invalidCode': {'one': 'Wrong code. {count} try left.', 'other': 'Wrong code. {count} tries left.'},
      'auth.apiError.tooManyCodes': 'Too many wrong codes.',
      'auth.apiError.rateLimited': 'Too many requests. Try again in {seconds} s.',
      'auth.apiError.locked': {'one': 'Locked for {count} minute.', 'other': 'Locked for {count} minutes.'},
      'auth.apiError.emailInvalid': 'Enter a valid email address (translated).',
      'auth.apiError.network': "Can't reach Kalks.",
      'accounts.error.positions_open': 'Close the positions first.',
      'common.unavailable': 'Temporarily unavailable.',
      'shell.system.unavailable.text': "This section isn't offered on your account.",
    });
    ApiException e(String code, String message, {int? retry, int? attempts}) =>
        ApiException(status: 400, code: code, message: message, retryAfter: retry, attemptsLeft: attempts);

    test('known codes and validation texts are translated, the rest keeps the server wording', () {
      expect(localizeError(e('invalid_credentials', 'x'), t), 'Wrong email or password.');
      expect(localizeError(e('invalid_code', 'x', attempts: 1), t), 'Wrong code. 1 try left.');
      expect(localizeError(e('invalid_code', 'x', attempts: 0), t), 'Too many wrong codes.');
      expect(localizeError(e('rate_limited', 'x', retry: 42), t), 'Too many requests. Try again in 42 s.');
      expect(localizeError(e('locked', 'x', retry: 90), t), 'Locked for 2 minutes.');
      expect(localizeError(e('validation', 'Enter a valid email address.'), t), 'Enter a valid email address (translated).');
      expect(localizeError(e('positions_open', 'x'), t), 'Close the positions first.');
      expect(localizeError(e('unavailable', 'x'), t), 'Temporarily unavailable.');
      expect(localizeError(ApiException.network, t), "Can't reach Kalks.");
      expect(localizeError(e('wrong_server', 'This login is on Kalks-Demo.'), t), 'This login is on Kalks-Demo.');
      // a switched-off module: the server's sentence, else the "not available" text
      expect(localizeError(e('module_disabled', 'MAM is not available on your account.'), t), 'MAM is not available on your account.');
      expect(localizeError(e('module_disabled', ' '), t), "This section isn't offered on your account.");
    });
  });
}

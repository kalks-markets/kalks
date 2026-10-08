// The one HTTP client of the app (docs/MOBILE-API.md). Every call goes to <base>/api/mobile/... with:
//   Authorization: Bearer <gateway session>   (signed-in calls; never with cookies — Dio keeps no cookie jar)
//   X-Kalks-Device, X-Kalks-Platform: android, X-Kalks-App-Version, X-Kalks-Locale, User-Agent
//   X-Kalks-Trade (Kalks Trader account calls), X-Kalks-Stepup (step-up protected writes, or `stepup_token` in the body)
// Errors become ApiException (api_error.dart). A dead session (401 unauthorized) and maintenance (503) are reported
// to the app once, so it can show sign-in or the maintenance screen; a switched-off module (403 module_disabled), so
// it can fetch the broker's module switches again.
import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/foundation.dart';

import '../../env.dart';
import 'api_error.dart';

/// What the client needs from the rest of the app (session, device, language, version).
class ApiContext {
  const ApiContext({required this.token, required this.deviceId, required this.locale, required this.appVersion, required this.userAgent});

  /// The current gateway session token (null when signed out).
  final String? Function() token;
  final Future<String> Function() deviceId;
  final String Function() locale;
  final String appVersion;
  final String userAgent;
}

class ApiClient {
  ApiClient({
    required String baseUrl,
    required this.context,
    HttpClientAdapter? adapter,
    this.onSessionDead,
    this.onMaintenance,
    this.onModuleDisabled,
    this.onDeviceMinted,
  }) : dio = Dio(
         BaseOptions(
           baseUrl: baseUrl.endsWith('/') ? baseUrl : '$baseUrl/',
           connectTimeout: const Duration(seconds: 15),
           receiveTimeout: const Duration(seconds: 30),
           sendTimeout: const Duration(seconds: 30),
           // every status comes back as a response: errors are mapped below, never thrown by Dio
           validateStatus: (_) => true,
         ),
       ) {
    if (adapter != null) dio.httpClientAdapter = adapter;
    dio.interceptors.add(InterceptorsWrapper(onRequest: _headers));
  }

  final Dio dio;
  final ApiContext context;

  /// 401 `unauthorized` on a signed-in call: the session is gone (expired, signed out elsewhere, blocked).
  final void Function(ApiException e)? onSessionDead;

  /// 503 `maintenance`: the broker's maintenance mode.
  final void Function(ApiException e)? onMaintenance;

  /// 403 `module_disabled`: the broker switched off the module this call belongs to.
  final void Function(ApiException e)? onModuleDisabled;

  /// A sign-in answer carried a freshly minted `device` id.
  final void Function(String device)? onDeviceMinted;

  Future<void> _headers(RequestOptions o, RequestInterceptorHandler h) async {
    final auth = o.extra['auth'] != false;
    final token = auth ? context.token() : null;
    o.headers.remove('cookie');
    if (token != null) o.headers['Authorization'] = 'Bearer $token';
    o.headers['X-Kalks-Device'] = await context.deviceId();
    o.headers['X-Kalks-Platform'] = Env.platform;
    o.headers['X-Kalks-App-Version'] = context.appVersion;
    o.headers['X-Kalks-Locale'] = context.locale();
    // browsers refuse to let scripts set the User-Agent (web preview only)
    if (!kIsWeb) o.headers['User-Agent'] = context.userAgent;
    o.extra['bearer'] = token != null;
    h.next(o);
  }

  Future<T> get<T>(String path, {Map<String, Object?>? query, String? tradeToken, CancelToken? cancel, bool auth = true}) =>
      _send<T>('GET', path, query: query, tradeToken: tradeToken, cancel: cancel, auth: auth);

  Future<T> post<T>(String path, {Object? body, Map<String, Object?>? query, String? tradeToken, String? stepupToken, CancelToken? cancel, bool auth = true}) =>
      _send<T>(
        'POST',
        path,
        body: body ?? const <String, Object?>{},
        query: query,
        tradeToken: tradeToken,
        stepupToken: stepupToken,
        cancel: cancel,
        auth: auth,
      );

  Future<T> patch<T>(String path, {Object? body, String? tradeToken, String? stepupToken, CancelToken? cancel}) =>
      _send<T>('PATCH', path, body: body ?? const <String, Object?>{}, tradeToken: tradeToken, stepupToken: stepupToken, cancel: cancel);

  Future<T> put<T>(String path, {Object? body, String? stepupToken, CancelToken? cancel}) =>
      _send<T>('PUT', path, body: body ?? const <String, Object?>{}, stepupToken: stepupToken, cancel: cancel);

  Future<T> delete<T>(String path, {Object? body, String? tradeToken, CancelToken? cancel}) =>
      _send<T>('DELETE', path, body: body, tradeToken: tradeToken, cancel: cancel);

  /// A file (statements, CSV, attachments): the bytes and the server's file name (Content-Disposition).
  Future<({Uint8List bytes, String? fileName, String? contentType})> download(String path, {Map<String, Object?>? query, CancelToken? cancel}) async {
    final r = await _raw('GET', path, query: query, cancel: cancel, responseType: ResponseType.bytes);
    if (r.statusCode == null || r.statusCode! >= 400) throw _error(r);
    final disposition = r.headers.value('content-disposition');
    final name = disposition == null ? null : RegExp(r'''filename\*?=(?:UTF-8'')?"?([^";]+)"?''').firstMatch(disposition)?.group(1);
    return (
      bytes: Uint8List.fromList(r.data as List<int>),
      fileName: name == null ? null : Uri.decodeComponent(name),
      contentType: r.headers.value('content-type'),
    );
  }

  /// Raw-body upload (support attachments: the bytes with their Content-Type and X-File-Name).
  Future<T> upload<T>(String path, {required Object data, required String contentType, Map<String, String>? headers, CancelToken? cancel}) =>
      _send<T>('POST', path, body: data, contentType: contentType, extraHeaders: headers, cancel: cancel);

  Future<T> _send<T>(
    String method,
    String path, {
    Object? body,
    Map<String, Object?>? query,
    String? tradeToken,
    String? stepupToken,
    CancelToken? cancel,
    bool auth = true,
    String? contentType,
    Map<String, String>? extraHeaders,
  }) async {
    final r = await _raw(
      method,
      path,
      body: body,
      query: query,
      tradeToken: tradeToken,
      stepupToken: stepupToken,
      cancel: cancel,
      auth: auth,
      contentType: contentType,
      extraHeaders: extraHeaders,
    );
    final status = r.statusCode ?? 0;
    if (status >= 400 || status == 0) throw _error(r);
    final data = r.data;
    if (data is Map && data['device'] is String) onDeviceMinted?.call(data['device'] as String);
    return (data ?? const <String, Object?>{}) as T;
  }

  Future<Response<Object?>> _raw(
    String method,
    String path, {
    Object? body,
    Map<String, Object?>? query,
    String? tradeToken,
    String? stepupToken,
    CancelToken? cancel,
    bool auth = true,
    ResponseType? responseType,
    String? contentType,
    Map<String, String>? extraHeaders,
  }) async {
    try {
      return await dio.request<Object?>(
        path.startsWith('/') ? path.substring(1) : path,
        data: body,
        queryParameters: query == null
            ? null
            : {
                for (final e in query.entries)
                  if (e.value != null) e.key: e.value,
              },
        cancelToken: cancel,
        options: Options(
          method: method,
          responseType: responseType,
          contentType: body == null ? null : (contentType ?? Headers.jsonContentType),
          headers: {'X-Kalks-Trade': ?tradeToken, 'X-Kalks-Stepup': ?stepupToken, ...?extraHeaders},
          extra: {'auth': auth},
        ),
      );
    } on DioException catch (e) {
      if (e.type == DioExceptionType.cancel) rethrow;
      throw ApiException.network;
    }
  }

  ApiException _error(Response<Object?> r) {
    final e = ApiException.fromResponse(r.statusCode ?? 0, r.data, retryAfterHeader: r.headers.value('retry-after'));
    if (e.isUnauthorized && r.requestOptions.extra['bearer'] == true) onSessionDead?.call(e);
    if (e.isMaintenance) onMaintenance?.call(e);
    if (e.isModuleDisabled) onModuleDisabled?.call(e);
    return e;
  }
}

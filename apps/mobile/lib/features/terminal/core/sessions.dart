// Kalks Trader sessions (docs/MOBILE-API.md §6 "Trade tokens"): one engine session per trading account, held as a
// trade token (X-Kalks-Trade) in the Keystore next to the gateway session.
// - An own account opens with `POST trade/sessions {login}` (the Client Area Trade button's SSO);
// - any account can be added MT5-style with `POST trade/login {login, password, server}` (investor password =
//   read-only);
// - on start `POST trade/sessions/check` says which stored tokens are still alive (the account switcher);
// - `POST trade/logout` ends one.
// The active login drives the terminal (terminal_controller.dart); the switcher lists the client's own accounts
// (trading/accounts, one tap opens them) plus the other logins added with a password.
import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/api/api_providers.dart';
import '../../../core/auth/auth_controller.dart';
import '../../../core/auth/secure_store.dart';
import '../../../core/config/app_config.dart';
import '../../../core/models/account.dart';
import '../../../data/client_data.dart';
import 'models.dart';
import 'workspace.dart';

@immutable
class TradeSession {
  const TradeSession({required this.login, required this.token, required this.readOnly, this.account, this.expiresAt});
  final String login;
  final String token;

  /// Investor password: trading is disabled.
  final bool readOnly;
  final TAccount? account;
  final DateTime? expiresAt;

  /// Opened through the client's own account (SSO) rather than a password: it can be re-opened silently.
  bool get own => token.startsWith('kt1.s.');

  TradeSession withAccount(TAccount a, {bool? readOnly}) =>
      TradeSession(login: login, token: token, readOnly: readOnly ?? this.readOnly, account: a, expiresAt: expiresAt);

  static TradeSession fromAnswer(Map<String, dynamic> j, {String? token}) {
    final acc = j['account'] is Map ? TAccount.fromJson((j['account'] as Map).cast<String, dynamic>()) : null;
    return TradeSession(
      login: '${j['login'] ?? acc?.login ?? ''}',
      token: token ?? '${j['token'] ?? ''}',
      readOnly: j['readOnly'] == true,
      account: acc,
      expiresAt: DateTime.tryParse('${j['expiresAt']}'),
    );
  }
}

enum TradeSessionsPhase { starting, ready, failed }

/// The account to show for a product (cfd | options) among the client's own: `recent` when it trades that product,
/// else the default-starred one, else a live one, else any. Archived and blocked accounts don't count; null when the
/// client holds no usable account of that product.
String? pickProductLogin(List<EngineAccount> accounts, String product, {String? recent}) {
  final list = accounts.where((a) => !a.archived && !a.tradeBlocked && a.product == product).toList();
  if (list.isEmpty) return null;
  if (recent != null && list.any((a) => '${a.login}' == recent)) return recent;
  final def = list.where((a) => a.isDefault);
  if (def.isNotEmpty) return '${def.first.login}';
  final live = list.where((a) => a.type == AccountKind.live);
  return '${(live.isNotEmpty ? live.first : list.first).login}';
}

@immutable
class TradeSessionsState {
  const TradeSessionsState({this.phase = TradeSessionsPhase.starting, this.sessions = const {}, this.active, this.error, this.busyLogin});
  final TradeSessionsPhase phase;

  /// Alive sessions by login.
  final Map<String, TradeSession> sessions;

  /// The account on screen.
  final String? active;

  /// Why the terminal couldn't open an account (shown with a retry).
  final ApiException? error;

  /// The account being opened right now (switcher spinner).
  final String? busyLogin;

  TradeSession? get current => active == null ? null : sessions[active];

  TradeSessionsState copyWith({
    TradeSessionsPhase? phase,
    Map<String, TradeSession>? sessions,
    String? active,
    ApiException? error,
    bool clearError = false,
    String? busyLogin,
    bool clearBusy = false,
  }) => TradeSessionsState(
    phase: phase ?? this.phase,
    sessions: sessions ?? this.sessions,
    active: active ?? this.active,
    error: clearError ? null : (error ?? this.error),
    busyLogin: clearBusy ? null : (busyLogin ?? this.busyLogin),
  );
}

class TradeSessionsController extends Notifier<TradeSessionsState> {
  @override
  TradeSessionsState build() {
    // a new client (sign-out, another sign-in) starts from nothing: trade tokens belong to the client they were issued to
    ref.watch(authProvider.select((s) => s is AuthSignedIn ? s.me.id : null));
    _started = false;
    _recent.clear();
    return const TradeSessionsState();
  }

  ApiClient get _api => ref.read(apiProvider);
  SessionStore get _store => ref.read(sessionStoreProvider);
  bool _started = false;

  /// The last account shown per product (cfd | options), for the header's CFD | Options switch.
  final Map<String, String> _recent = {};

  /// Opens the terminal: the stored sessions still alive, then `preferred` (the Trade button's account), else for a
  /// `mode` (cfd | options, `/trader?mode=`) an account of that product, else the last one used, else the default
  /// own account. Already open: only a `mode` the account on screen doesn't trade switches accounts.
  Future<void> start({String? preferred, String? mode}) async {
    final product = mode == 'cfd' || mode == 'options' ? mode : null;
    if (_started && preferred == null && state.current != null) {
      if (product == null || await productOfLogin(state.active!) == product) return;
      final pick = await loginForProduct(product);
      if (pick != null) await activate(pick);
      return;
    }
    _started = true;
    state = state.copyWith(phase: TradeSessionsPhase.starting, clearError: true);
    final alive = <String, TradeSession>{...state.sessions};
    try {
      final stored = await _store.tradeTokens();
      final entries = stored.entries.toList();
      for (var i = 0; i < entries.length; i += 8) {
        final chunk = entries.sublist(i, (i + 8).clamp(0, entries.length));
        final r = await _api.post<Map<String, dynamic>>('trade/sessions/check', body: {'tokens': chunk.map((e) => e.value).toList()});
        final list = (r['sessions'] as List?) ?? const [];
        for (var k = 0; k < chunk.length && k < list.length; k++) {
          final s = list[k] is Map ? (list[k] as Map).cast<String, dynamic>() : const <String, dynamic>{};
          if (s['alive'] == true) {
            final sess = TradeSession.fromAnswer({...s, 'login': s['login'] ?? chunk[k].key}, token: chunk[k].value);
            alive[sess.login] = sess;
          } else {
            await _store.setTradeToken(chunk[k].key, null);
          }
        }
      }
    } on ApiException {
      // offline or the check failed: try to open the account below anyway
    }
    state = state.copyWith(sessions: alive);
    final ws = ref.read(workspaceProvider);
    String? login = preferred ?? (product == null ? ws.lastLogin : await _defaultOwnLogin(product: product));
    if (login == null || (!alive.containsKey(login) && !(await _isOwn(login)))) {
      login = await _defaultOwnLogin() ?? (alive.keys.isEmpty ? null : alive.keys.first);
    }
    if (login == null) {
      state = state.copyWith(
        phase: TradeSessionsPhase.failed,
        error: const ApiException(status: 404, code: 'no_account', message: 'No trading account.'),
      );
      return;
    }
    await activate(login);
  }

  /// The client's accounts (kept alive while read: the provider is autoDispose).
  Future<List<EngineAccount>> _ownAccounts() async {
    final sub = ref.listen(accountsProvider.future, (_, _) {});
    try {
      return await sub.read();
    } finally {
      sub.close();
    }
  }

  Future<bool> _isOwn(String login) async {
    try {
      final list = await _ownAccounts();
      return list.any((a) => '${a.login}' == login && !a.archived);
    } catch (_) {
      return false;
    }
  }

  /// The default own account: of `product` when given (the last one used of it, else default-starred, live, any);
  /// otherwise the default-starred one, else a live one, else the first (CFD accounts only while the broker has the
  /// Options module off).
  Future<String?> _defaultOwnLogin({String? product}) async {
    try {
      final own = await _ownAccounts();
      if (product != null) return pickProductLogin(own, product, recent: _recent[product] ?? ref.read(workspaceProvider).lastLogin);
      var list = own.where((a) => !a.archived && !a.tradeBlocked).toList();
      if (!ref.read(configProvider).moduleOn('options') && list.any((a) => !a.isOptions)) list = list.where((a) => !a.isOptions).toList();
      if (list.isEmpty) return null;
      final def = list.where((a) => a.isDefault);
      if (def.isNotEmpty) return '${def.first.login}';
      final live = list.where((a) => a.type == AccountKind.live);
      return '${(live.isNotEmpty ? live.first : list.first).login}';
    } catch (_) {
      return null;
    }
  }

  /// What `login` trades (cfd | options): its session's account, else the client's own record, else CFD.
  Future<String> productOfLogin(String login) async {
    final p = state.sessions[login]?.account?.product;
    if (p != null) return p;
    try {
      return (await _ownAccounts()).where((a) => '${a.login}' == login).firstOrNull?.product ?? 'cfd';
    } catch (_) {
      return 'cfd';
    }
  }

  /// The account to switch to for a product (the header's CFD | Options): the last one shown of it, else the
  /// client's default / live / any own account of it, else a login added with a password that trades it. Null: the
  /// client has none (the terminal offers to open one).
  Future<String?> loginForProduct(String product) async {
    final recent = _recent[product];
    if (recent != null && state.sessions[recent]?.account?.product == product) return recent;
    final own = await _defaultOwnLogin(product: product);
    if (own != null) return own;
    for (final s in state.sessions.values) {
      if (s.account?.product == product) return s.login;
    }
    return null;
  }

  /// Shows `login`: its alive session, else opens the client's own account (one tap in the switcher).
  Future<bool> activate(String login) async {
    if (state.sessions.containsKey(login)) {
      _setActive(login);
      return true;
    }
    state = state.copyWith(busyLogin: login, clearError: true);
    try {
      final s = await openOwn(login);
      state = state.copyWith(sessions: {...state.sessions, login: s}, clearBusy: true);
      _setActive(login);
      return true;
    } on ApiException catch (e) {
      state = state.copyWith(clearBusy: true, error: e, phase: state.current == null ? TradeSessionsPhase.failed : state.phase);
      return false;
    }
  }

  void _setActive(String login) {
    final product = state.sessions[login]?.account?.product;
    if (product != null) _recent[product] = login;
    state = state.copyWith(active: login, phase: TradeSessionsPhase.ready, clearError: true);
    ref.read(workspaceProvider.notifier).update((w) => w.copyWith(lastLogin: login));
  }

  /// `POST trade/sessions {login}`: an engine session for one of the client's own accounts.
  Future<TradeSession> openOwn(String login) async {
    final r = await _api.post<Map<String, dynamic>>('trade/sessions', body: {'login': login});
    final s = TradeSession.fromAnswer({...r, 'login': r['login'] ?? login});
    if (s.token.isEmpty) throw const ApiException(status: 502, code: 'unavailable', message: 'No session.');
    await _store.setTradeToken(s.login, s.token);
    return s;
  }

  /// MT5-style login (Account tab "add account"): a trading password gives full access, an investor password
  /// read-only. Adds the session and switches to it.
  Future<TradeSession> loginWithPassword({required String login, required String password, required String server}) async {
    final r = await _api.post<Map<String, dynamic>>('trade/login', body: {'login': login, 'password': password, 'server': server});
    final s = TradeSession.fromAnswer({...r, 'login': r['login'] ?? login});
    if (s.token.isEmpty) throw const ApiException(status: 502, code: 'unavailable', message: 'No session.');
    await _store.setTradeToken(s.login, s.token);
    state = state.copyWith(sessions: {...state.sessions, s.login: s});
    _setActive(s.login);
    return s;
  }

  /// The engine session ended (expiry, password change, sign-out): an own account re-opens silently, a password
  /// login is dropped (its password is needed again). Returns the fresh session, or null.
  Future<TradeSession?> sessionEnded(String login) async {
    final old = state.sessions[login];
    await _store.setTradeToken(login, null);
    final rest = {...state.sessions}..remove(login);
    state = state.copyWith(sessions: rest);
    if (old != null && !old.own) {
      if (state.active == login) {
        state = TradeSessionsState(
          phase: rest.isEmpty ? TradeSessionsPhase.failed : TradeSessionsPhase.ready,
          sessions: rest,
          active: rest.keys.isEmpty ? null : rest.keys.first,
        );
      }
      return null;
    }
    try {
      final s = await openOwn(login);
      state = state.copyWith(sessions: {...state.sessions, login: s});
      if (state.active == login) state = state.copyWith(active: login);
      return s;
    } on ApiException catch (e) {
      if (state.active == login) state = state.copyWith(error: e, phase: TradeSessionsPhase.failed);
      return null;
    }
  }

  /// The latest account view of a session (stream `account` / `snapshot` frames).
  void updateAccount(String login, TAccount a, {bool? readOnly}) {
    final s = state.sessions[login];
    if (s == null) return;
    state = state.copyWith(
      sessions: {
        ...state.sessions,
        login: s.withAccount(a, readOnly: readOnly),
      },
    );
  }

  /// Ends one engine session (`POST trade/logout`) and forgets its token.
  Future<void> logout(String login) async {
    final s = state.sessions[login];
    if (s != null) {
      unawaited(_api.post<Object?>('trade/logout', tradeToken: s.token).then<void>((_) {}, onError: (Object _) {}));
    }
    await _store.setTradeToken(login, null);
    final rest = {...state.sessions}..remove(login);
    state = TradeSessionsState(
      phase: rest.isEmpty ? TradeSessionsPhase.failed : TradeSessionsPhase.ready,
      sessions: rest,
      active: state.active == login ? (rest.keys.isEmpty ? null : rest.keys.first) : state.active,
      error: rest.isEmpty ? const ApiException(status: 401, code: 'logged_out', message: '') : null,
    );
  }

  /// Retry after a failure.
  Future<void> retry() async {
    _started = false;
    await start(preferred: state.active);
  }
}

final tradeSessionsProvider = NotifierProvider<TradeSessionsController, TradeSessionsState>(TradeSessionsController.new);

/// The session on screen (null while opening or when none).
final activeTradeSessionProvider = Provider<TradeSession?>((ref) => ref.watch(tradeSessionsProvider.select((s) => s.current)));

/// Calls of the active trading account: the API client with its trade token (X-Kalks-Trade) filled in.
class TradeApi {
  TradeApi(this.api, this.session);
  final ApiClient api;
  final TradeSession session;

  String get login => session.login;
  String get token => session.token;

  Future<T> get<T>(String path, {Map<String, Object?>? query}) => api.get<T>(path, query: query, tradeToken: token);
  Future<T> post<T>(String path, {Object? body}) => api.post<T>(path, body: body, tradeToken: token);
  Future<T> patch<T>(String path, {Object? body}) => api.patch<T>(path, body: body, tradeToken: token);
  Future<T> delete<T>(String path, {Object? body}) => api.delete<T>(path, body: body, tradeToken: token);
}

/// The active account's trade API (null while no account is open).
final tradeApiProvider = Provider<TradeApi?>((ref) {
  final s = ref.watch(activeTradeSessionProvider);
  return s == null ? null : TradeApi(ref.watch(apiProvider), s);
});

/// The client's own accounts for the switcher (not archived), from the Client Area.
final ownAccountsProvider = FutureProvider.autoDispose<List<EngineAccount>>((ref) async {
  final list = await ref.watch(accountsProvider.future);
  return list.where((a) => !a.archived).toList();
});

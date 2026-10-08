// State of the options mode (web: apps/terminal/lib/options-store.tsx): the selected underlying and expiry (the
// nearest of a Daily / Weekly / Monthly kind, rolled to the next date after its cut, or a date picked from the list),
// the live chain (REST snapshot, then the stream's changed rows; REST polling while the socket is down), quotes of
// series outside the chain on screen (open positions, ticket legs, the selected option), view preferences, the
// selected option, the order ticket and the strategy builder.
//
// Order book (docs/OPTIONS-EXCHANGE.md): when the chain says the broker's book is live (`chain.book.active`) and the
// engine serves the book routes, `bookLive` turns on: chain quotes carry the best bid / offer with sizes, the selected
// series' depth and trade tape stream in, and the ticket sends book orders. Otherwise everything stays on the
// house-priced flow.
import 'dart:async';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../../core/api/api_providers.dart';
import '../../../../core/prefs.dart';
import '../../../../core/realtime/socket.dart';
import '../../../../core/realtime/trade_streams.dart';
import '../../../../env.dart';
import '../../core/models.dart' show isOptionEntry;
import '../../core/sessions.dart';
import '../../core/terminal_controller.dart';
import '../options_preview.dart';
import 'api.dart';
import 'errors.dart';
import 'models.dart';
import 'pricer.dart';

/* ------------------------------------------------------------------ */
/* Types                                                               */
/* ------------------------------------------------------------------ */

/// loading → ready; soon = the module is off here (options_disabled); error = service down.
enum OptAvail { loading, ready, soon, error }

/// A column of the option chain (per side).
const List<String> allChainCols = ['bid', 'ask', 'last', 'mark', 'iv', 'delta', 'gamma', 'theta', 'vega', 'prob', 'be', 'oi', 'vol'];

/// Column sets: Simple (the price only), Standard (+ sell price, chance, breakeven), Pro (everything).
const Map<String, List<String>> colPresets = {
  'simple': ['ask'],
  'standard': ['bid', 'ask', 'prob', 'be'],
  'pro': allChainCols,
};

const List<String> expiryKinds = ['daily', 'weekly', 'monthly'];

/// Timeframes of the options charts (the premium candles service serves these).
const List<String> optionTfs = ['M1', 'M5', 'M15', 'M30', 'H1', 'H4', 'D1'];
const Map<String, int> tfMinutes = {'M1': 1, 'M5': 5, 'M15': 15, 'M30': 30, 'H1': 60, 'H4': 240, 'D1': 1440};

@immutable
class OptPrefs {
  const OptPrefs({
    this.u = 'EURUSD',
    this.view = 'both',
    this.colPreset = 'simple',
    this.cols = const ['ask'],
    this.range = 10,
    this.tf = 'M15',
    this.panel = 'simple',
    this.expKind = 'daily',
    this.chartMode = 'premium',
  });

  final String u;

  /// both | calls | puts
  final String view;

  /// simple | standard | pro | custom
  final String colPreset;
  final List<String> cols;

  /// strikes each side of ATM (0 = all)
  final int range;
  final String tf;

  /// simple (Quick trade) | ticket
  final String panel;

  /// daily | weekly | monthly, null = a date picked from the list
  final String? expKind;

  /// premium | underlying
  final String chartMode;

  OptPrefs copyWith({
    String? u,
    String? view,
    String? colPreset,
    List<String>? cols,
    int? range,
    String? tf,
    String? panel,
    Object? expKind = _keep,
    String? chartMode,
  }) => OptPrefs(
    u: u ?? this.u,
    view: view ?? this.view,
    colPreset: colPreset ?? this.colPreset,
    cols: cols ?? this.cols,
    range: range ?? this.range,
    tf: tf ?? this.tf,
    panel: panel ?? this.panel,
    expKind: identical(expKind, _keep) ? this.expKind : expKind as String?,
    chartMode: chartMode ?? this.chartMode,
  );

  Map<String, Object?> toJson() => {
    'u': u,
    'view': view,
    'colPreset': colPreset,
    'cols': cols,
    'range': range,
    'tf': tf,
    'panel': panel,
    'expKind': expKind,
    'chartMode': chartMode,
  };

  static OptPrefs fromJson(Map<String, dynamic>? j) {
    if (j == null) return const OptPrefs();
    const d = OptPrefs();
    final tf = optionTfs.contains(j['tf']) ? j['tf'] as String : d.tf;
    final kind = j.containsKey('expKind') ? (j['expKind'] == null ? null : (expiryKinds.contains(j['expKind']) ? j['expKind'] as String : 'daily')) : 'daily';
    var preset = j['colPreset'] is String ? j['colPreset'] as String : (j['greeks'] == true ? 'pro' : (j['extra'] == true ? 'standard' : 'simple'));
    if (!const ['simple', 'standard', 'pro', 'custom'].contains(preset)) preset = 'simple';
    var cols = preset != 'custom'
        ? colPresets[preset]!
        : [
            for (final c in (j['cols'] as List? ?? const <Object?>[]))
              if (allChainCols.contains(c)) '$c',
          ];
    if (cols.isEmpty) cols = colPresets['simple']!;
    return OptPrefs(
      u: j['u'] is String ? j['u'] as String : d.u,
      view: const ['both', 'calls', 'puts'].contains(j['view']) ? j['view'] as String : d.view,
      colPreset: preset,
      cols: cols,
      range: (j['range'] is num) ? (j['range'] as num).toInt() : d.range,
      tf: tf,
      panel: j['panel'] == 'ticket' ? 'ticket' : 'simple',
      expKind: kind,
      chartMode: j['chartMode'] == 'underlying' ? 'underlying' : 'premium',
    );
  }
}

const Object _keep = Object();

@immutable
class TicketLeg {
  const TicketLeg({
    required this.id,
    required this.series,
    required this.u,
    required this.expiry,
    required this.right,
    required this.strike,
    required this.strikeLabel,
    required this.side,
    required this.contracts,
    this.barrier,
  });
  final String id, series, u, expiry;
  final OptionRight right;
  final double strike;
  final String strikeLabel;
  final String side;
  final int contracts;

  /// a barrier leg (Kalks-quoted): `{kind, level, rebate?}`
  final Map<String, Object?>? barrier;

  TicketLeg copyWith({String? id, String? series, String? expiry, OptionRight? right, double? strike, String? strikeLabel, String? side, int? contracts}) =>
      TicketLeg(
        id: id ?? this.id,
        series: series ?? this.series,
        u: u,
        expiry: expiry ?? this.expiry,
        right: right ?? this.right,
        strike: strike ?? this.strike,
        strikeLabel: strikeLabel ?? this.strikeLabel,
        side: side ?? this.side,
        contracts: contracts ?? this.contracts,
        barrier: barrier,
      );
}

@immutable
class Ticket {
  const Ticket({
    this.legs = const [],
    this.armed = false,
    this.adding = false,
    this.type = 'market',
    this.limit = '',
    this.sl = '',
    this.tp = '',
    this.trigger = false,
    this.triggerOp = 'above',
    this.triggerPrice = '',
    this.tif = 'gtc',
    this.bookType = 'limit',
    this.stopKind = 'market',
    this.bookTif = 'gtc',
    this.gtd,
    this.postOnly = false,
    this.reduceOnly = false,
    this.trigSource = 'mark',
    this.trigOp = 'below',
    this.trigPrice = '',
  });

  final List<TicketLeg> legs;

  /// single option: the trader chose Buy or Sell
  final bool armed;

  /// "Add leg": the next strike picked in the chain is added
  final bool adding;

  /// market | limit (house ticket)
  final String type;

  /// USD per contract texts
  final String limit, sl, tp;
  final bool trigger;
  final String triggerOp, triggerPrice;

  /// gtc | day
  final String tif;

  /// order book: limit | market | stop; stop market | limit; gtc | ioc | fok | gtd
  final String bookType, stopKind, bookTif;
  final DateTime? gtd;
  final bool postOnly, reduceOnly;

  /// mark | underlying; above | below
  final String trigSource, trigOp, trigPrice;

  TicketLeg? get single => legs.length == 1 ? legs.first : null;

  Ticket copyWith({
    List<TicketLeg>? legs,
    bool? armed,
    bool? adding,
    String? type,
    String? limit,
    String? sl,
    String? tp,
    bool? trigger,
    String? triggerOp,
    String? triggerPrice,
    String? tif,
    String? bookType,
    String? stopKind,
    String? bookTif,
    Object? gtd = _keep,
    bool? postOnly,
    bool? reduceOnly,
    String? trigSource,
    String? trigOp,
    String? trigPrice,
  }) => Ticket(
    legs: legs ?? this.legs,
    armed: armed ?? this.armed,
    adding: adding ?? this.adding,
    type: type ?? this.type,
    limit: limit ?? this.limit,
    sl: sl ?? this.sl,
    tp: tp ?? this.tp,
    trigger: trigger ?? this.trigger,
    triggerOp: triggerOp ?? this.triggerOp,
    triggerPrice: triggerPrice ?? this.triggerPrice,
    tif: tif ?? this.tif,
    bookType: bookType ?? this.bookType,
    stopKind: stopKind ?? this.stopKind,
    bookTif: bookTif ?? this.bookTif,
    gtd: identical(gtd, _keep) ? this.gtd : gtd as DateTime?,
    postOnly: postOnly ?? this.postOnly,
    reduceOnly: reduceOnly ?? this.reduceOnly,
    trigSource: trigSource ?? this.trigSource,
    trigOp: trigOp ?? this.trigOp,
    trigPrice: trigPrice ?? this.trigPrice,
  );

  /// What a fresh ticket keeps of the previous one: the order type and time in force the trader works with.
  Ticket fresh({List<TicketLeg> legs = const [], bool armed = false}) => Ticket(
    legs: legs,
    armed: armed,
    type: type,
    tif: tif,
    bookType: bookType,
    stopKind: stopKind,
    bookTif: bookTif == 'gtd' ? 'gtc' : bookTif,
    trigSource: trigSource,
  );
}

@immutable
class OptState {
  const OptState({
    this.avail = OptAvail.loading,
    this.availMsg,
    this.underlyings = const [],
    this.expiries = const [],
    required this.u,
    this.expiry,
    this.chain,
    this.chainLoading = false,
    this.index = const {},
    this.quotes = const {},
    this.stream = 'connecting',
    this.tradingSoon = false,
    required this.prefs,
    this.ticket = const Ticket(),
    this.focus,
    this.sel,
    this.bookLive = false,
    this.bookOff = false,
    this.depth = const {},
    this.tape = const {},
  });

  final OptAvail avail;
  final String? availMsg;
  final List<OptionUnderlying> underlyings;
  final List<OptionExpiry> expiries;
  final String u;
  final String? expiry;
  final OptionChain? chain;
  final bool chainLoading;

  /// series code → quote, for the chain on screen
  final Map<String, OptionQuote> index;

  /// series code → quote, outside the chain on screen (positions, legs of other expiries)
  final Map<String, OptionQuote> quotes;

  /// connecting | open | reconnecting | closed | unavailable | polling
  final String stream;

  /// the engine doesn't take option orders yet (preview fell back to the estimate)
  final bool tradingSoon;
  final OptPrefs prefs;
  final Ticket ticket;

  /// position ticket whose lines the chart highlights
  final String? focus;

  /// the selected option (series code): highlighted in the chain, charted, traded by the ticket
  final String? sel;

  /// the broker's order book is live here (chain header)
  final bool bookLive;

  /// the engine answered a book route with 404 / 405 / 501: keep the house-priced flow
  final bool bookOff;

  /// series → depth / latest trades (newest first), the selected series while the book is live
  final Map<String, SeriesDepth> depth;
  final Map<String, List<TapeTrade>> tape;

  /// The order book is live for this account: book prices, depth, book orders (else house prices).
  bool get isBookLive => bookLive && !bookOff;

  /// The quote of a series wherever it is (chain on screen, or the series subscription).
  OptionQuote? quoteOf(String? code) => code == null ? null : (index[code] ?? quotes[code]);

  OptionUnderlying? underlyingOf([String? sym]) {
    final s = sym ?? u;
    for (final x in underlyings) {
      if (x.symbol == s) return x;
    }
    return null;
  }

  /// The chain on screen when it is the selected underlying's and expiry's.
  OptionChain? get chainOnScreen => chain != null && chain!.underlying == u && chain!.expiry == expiry ? chain : null;

  OptState copyWith({
    OptAvail? avail,
    Object? availMsg = _keep,
    List<OptionUnderlying>? underlyings,
    List<OptionExpiry>? expiries,
    String? u,
    Object? expiry = _keep,
    Object? chain = _keep,
    bool? chainLoading,
    Map<String, OptionQuote>? index,
    Map<String, OptionQuote>? quotes,
    String? stream,
    bool? tradingSoon,
    OptPrefs? prefs,
    Ticket? ticket,
    Object? focus = _keep,
    Object? sel = _keep,
    bool? bookLive,
    bool? bookOff,
    Map<String, SeriesDepth>? depth,
    Map<String, List<TapeTrade>>? tape,
  }) => OptState(
    avail: avail ?? this.avail,
    availMsg: identical(availMsg, _keep) ? this.availMsg : availMsg as String?,
    underlyings: underlyings ?? this.underlyings,
    expiries: expiries ?? this.expiries,
    u: u ?? this.u,
    expiry: identical(expiry, _keep) ? this.expiry : expiry as String?,
    chain: identical(chain, _keep) ? this.chain : chain as OptionChain?,
    chainLoading: chainLoading ?? this.chainLoading,
    index: index ?? this.index,
    quotes: quotes ?? this.quotes,
    stream: stream ?? this.stream,
    tradingSoon: tradingSoon ?? this.tradingSoon,
    prefs: prefs ?? this.prefs,
    ticket: ticket ?? this.ticket,
    focus: identical(focus, _keep) ? this.focus : focus as String?,
    sel: identical(sel, _keep) ? this.sel : sel as String?,
    bookLive: bookLive ?? this.bookLive,
    bookOff: bookOff ?? this.bookOff,
    depth: depth ?? this.depth,
    tape: tape ?? this.tape,
  );
}

/* ------------------------------------------------------------------ */
/* Expiries                                                            */
/* ------------------------------------------------------------------ */

/// Still tradable: before its cut, listed, not stopped.
bool expiryOpen(OptionExpiry e, [int? now]) =>
    e.cutMs > (now ?? DateTime.now().millisecondsSinceEpoch) && (e.status.isEmpty || e.status == 'listed') && e.state != 'closed';

/// The nearest open expiry of a kind.
OptionExpiry? nearestExpiry(List<OptionExpiry> list, String kind, [int? now]) {
  OptionExpiry? best;
  for (final e in list) {
    if (e.kinds.contains(kind) && expiryOpen(e, now) && (best == null || e.cutMs < best.cutMs)) best = e;
  }
  return best;
}

/// The tab a date shows under: the kind whose nearest expiry it is (the preferred kind first), else null (picked).
String? kindOfExpiry(List<OptionExpiry> list, String date, String? prefer, [int? now]) {
  final order = prefer != null ? [prefer, ...expiryKinds.where((x) => x != prefer)] : expiryKinds;
  for (final k in order) {
    if (nearestExpiry(list, k, now)?.date == date) return k;
  }
  return null;
}

/// The expiry to show from a fresh list: the one on screen while it trades, else the nearest of the tab's kind.
({String? date, String? kind}) pickExpiry(List<OptionExpiry> list, {String? keep, String? prev, String? kind, String? want, int? now}) {
  final t = now ?? DateTime.now().millisecondsSinceEpoch;
  if (want != null) {
    for (final e in list) {
      if (e.date == want && expiryOpen(e, t)) return (date: e.date, kind: kindOfExpiry(list, e.date, kind, t));
    }
  }
  OptionExpiry? kept;
  if (keep != null) {
    for (final e in list) {
      if (e.date == keep) kept = e;
    }
  }
  if (kept != null && expiryOpen(kept, t)) return (date: kept.date, kind: kind);
  if (kind == null && prev != null) {
    for (final e in list) {
      if (e.date == prev && expiryOpen(e, t)) return (date: e.date, kind: null);
    }
  }
  final first = kind ?? (kept != null && kept.kinds.isNotEmpty ? kept.kinds.first : 'daily');
  for (final k in [first, ...expiryKinds.where((x) => x != first)]) {
    final e = nearestExpiry(list, k, t);
    if (e != null) return (date: e.date, kind: k);
  }
  for (final e in list) {
    if (expiryOpen(e, t)) return (date: e.date, kind: kind);
  }
  return (date: list.isEmpty ? null : list.first.date, kind: kind);
}

Map<String, OptionQuote> indexOf(List<OptionChainRow> rows) {
  final out = <String, OptionQuote>{};
  for (final r in rows) {
    if (r.call != null) out[r.call!.code] = r.call!;
    if (r.put != null) out[r.put!.code] = r.put!;
  }
  return out;
}

/// Rows around ATM (`range` strikes each side; 0 = all).
List<OptionChainRow> visibleRows(OptionChain chain, int range, int atm) {
  if (range == 0 || chain.rows.length <= range * 2 + 1) return chain.rows;
  final from = (atm - range).clamp(0, chain.rows.length - (range * 2 + 1));
  return chain.rows.sublist(from, from + range * 2 + 1);
}

/* ------------------------------------------------------------------ */
/* Stream frames                                                       */
/* ------------------------------------------------------------------ */

/// One frame of the options stream applied to the state (web: options-store onFrame + stream.ts parseFrame).
/// `reload`: a strike was added, take the full chain.
({OptState state, bool reload, bool chainChanged}) applyOptFrame(OptState s, Map<String, dynamic> f) {
  switch (f['type']) {
    case 'chain':
      if (f['underlying'] != s.u || f['expiry'] != s.expiry) return (state: s, reload: false, chainChanged: false);
      final chain = OptionChain.fromJson(f);
      final live = chain.book?.active ?? false;
      return (state: s.copyWith(chain: chain, index: indexOf(chain.rows), chainLoading: false, bookLive: live), reload: false, chainChanged: true);
    case 'rows':
      final c = s.chain;
      if (c == null || f['u'] != s.u || f['expiry'] != s.expiry) return (state: s, reload: false, chainChanged: false);
      final byLabel = {for (final r in jsonList(f['rows'])) '${r['strikeLabel'] ?? r['strike']}': OptionChainRow.fromJson(r)};
      var missing = byLabel.length;
      final rows = <OptionChainRow>[];
      for (final r in c.rows) {
        final n = byLabel[r.strikeLabel];
        if (n != null) missing--;
        rows.add(n ?? r);
      }
      if (missing > 0) return (state: s, reload: true, chainChanged: false);
      final next = c.copyWith(rows: rows, spot: ChainSpot.fromJson(f['spot']) ?? c.spot, state: f['state'] is String ? f['state'] as String : c.state);
      return (state: s.copyWith(chain: next, index: indexOf(rows)), reload: false, chainChanged: false);
    case 'series':
      final quotes = {...s.quotes};
      for (final raw in (f['quotes'] as List? ?? const <Object?>[])) {
        final q = OptionQuote.fromJson(raw);
        if (q != null) quotes[q.code] = q;
      }
      return (state: s.copyWith(quotes: quotes), reload: false, chainChanged: false);
    case 'depth':
      final d = SeriesDepth.fromJson(f);
      if (d == null) return (state: s, reload: false, chainChanged: false);
      return (state: s.copyWith(depth: {...s.depth, d.series: d}), reload: false, chainChanged: false);
    case 'tape':
    case 'trade':
    case 'trades':
      final raw = f['trades'] is List ? f['trades'] as List : [f['trade'] ?? f];
      final trades = [for (final t in raw) ?TapeTrade.fromJson(t)];
      if (trades.isEmpty) return (state: s, reload: false, chainChanged: false);
      final tape = {...s.tape};
      for (final t in trades) {
        final list = tape[t.series] ?? const <TapeTrade>[];
        if (list.any((x) => x.id == t.id)) continue;
        final next = [t, ...list]..sort((a, b) => b.t.compareTo(a.t));
        tape[t.series] = next.take(120).toList();
      }
      return (state: s.copyWith(tape: tape), reload: false, chainChanged: false);
  }
  return (state: s, reload: false, chainChanged: false);
}

/* ------------------------------------------------------------------ */
/* The account's option positions (engine state, terminalProvider)    */
/* ------------------------------------------------------------------ */

@immutable
class OptBookView {
  const OptBookView({this.positions = const [], this.orders = const [], this.deals = const [], this.cent = false, this.loaded = false});
  final List<OptPosition> positions;
  final List<OptOrder> orders;

  /// raw option deals (entries and exits), newest first
  final List<Map<String, dynamic>> deals;
  final bool cent;
  final bool loaded;
}

/// Previews and the in-app demo: the sample option book of the preview trade server's account (lib/features/terminal/
/// options/options_preview.dart), merged into the engine state's option entries. Off on a live account.
final optionsPreviewOverlayProvider = Provider<bool>((ref) => Env.preview || ref.watch(demoModeProvider));

/// The options stream's socket in previews, the demo and tests (the fake socket); null = the real WebSocket.
final optionsConnectorProvider = Provider<SocketConnector?>((ref) => Env.preview || ref.watch(demoModeProvider) ? PreviewOptions.instance.connector : null);

final _previewTickProvider = StreamProvider.autoDispose<int>(
  (ref) => ref.watch(optionsPreviewOverlayProvider) ? PreviewOptions.instance.changes : const Stream.empty(),
);

final optBookProvider = Provider.autoDispose<OptBookView>((ref) {
  final login = ref.watch(terminalProvider.select((s) => s.login));
  final cent = ref.watch(terminalProvider.select((s) => s.account?.cent ?? false));
  final synced = ref.watch(terminalProvider.select((s) => s.synced));
  var pos = ref.watch(terminalProvider.select((s) => s.optPositions));
  var ord = ref.watch(terminalProvider.select((s) => s.optOrders));
  var deals = ref.watch(terminalProvider.select((s) => s.optDeals));
  if (ref.watch(optionsPreviewOverlayProvider)) {
    ref.watch(_previewTickProvider);
    final p = PreviewOptions.instance.bookOf(login);
    final tickets = {for (final x in pos) '${x['ticket']}'};
    pos = [...pos, ...p.positions.where((x) => !tickets.contains('${x['ticket']}'))];
    final ot = {for (final x in ord) '${x['ticket']}'};
    ord = [...ord, ...p.orders.where((x) => !ot.contains('${x['ticket']}'))];
    final ids = {for (final x in deals) '${x['id']}'};
    deals = [...deals, ...p.deals.where((x) => !ids.contains('${x['id']}'))];
  }
  return OptBookView(
    positions: [for (final p in pos.where(isOptionEntry)) OptPosition.fromJson(p, cent: cent)],
    orders: [for (final o in ord.where(isOptionEntry)) OptOrder.fromJson(o)],
    deals: deals,
    cent: cent,
    loaded: synced || pos.isNotEmpty,
  );
});

/* ------------------------------------------------------------------ */
/* The controller                                                      */
/* ------------------------------------------------------------------ */

final optionsApiProvider = Provider.autoDispose<OptionsApi?>((ref) {
  final api = ref.watch(tradeApiProvider);
  return api == null ? null : OptionsApi(api, local: ref.watch(optionsPreviewOverlayProvider));
});

class _Remap {
  const _Remap(this.strike, this.right, this.prevCode);
  final double strike;
  final OptionRight right;
  final String prevCode;
}

class OptionsController extends Notifier<OptState> {
  OptionsStream? _stream;
  Timer? _poll, _expiryTimer, _rollTimer, _retry, _keep;
  int _seq = 0;
  int _gen = 0;
  int _downSince = 0;
  String? _wantExpiry;
  _Remap? _remap;
  int _uid = 0;

  OptionsApi? get _api => ref.read(optionsApiProvider);

  @override
  OptState build() {
    final key = ref.watch(activeTradeSessionProvider.select((s) => s == null ? null : (s.login, s.token)));
    final gen = ++_gen;
    // keep the data a moment after the screen leaves: switching tabs or modes back and forth shouldn't reconnect
    final link = ref.keepAlive();
    ref.onCancel(() {
      _keep?.cancel();
      _keep = Timer(const Duration(seconds: 15), link.close);
    });
    ref.onResume(() => _keep?.cancel());
    ref.onDispose(() {
      _keep?.cancel();
      _keep = null;
      _teardown();
    });
    ref.listen(optBookProvider, (_, _) => _syncSeries());
    final prefs = OptPrefs.fromJson(_prefs?.featureJson('options.prefs'));
    final s0 = OptState(u: prefs.u, prefs: prefs);
    if (key != null) {
      Future.microtask(() {
        if (gen == _gen) unawaited(_boot(gen));
      });
    }
    return s0;
  }

  Prefs? get _prefs {
    try {
      return ref.read(prefsProvider);
    } catch (_) {
      return null;
    }
  }

  void _set(OptState next) {
    final wasBook = state.isBookLive;
    state = next;
    if (wasBook != next.isBookLive) _syncBookFeed();
  }

  void _teardown() {
    // detached first: stopping reports `closed`, which must not write state while the provider is being disposed
    final stream = _stream;
    _stream = null;
    stream?.stop();
    for (final t in [_poll, _expiryTimer, _rollTimer, _retry]) {
      t?.cancel();
    }
    _poll = _expiryTimer = _rollTimer = _retry = null;
    _remap = null;
  }

  void _savePrefs(OptPrefs p) => unawaited(_prefs?.setFeatureJson('options.prefs', p.toJson()));

  /* ---------------- loading ---------------- */

  Future<void> _boot(int gen) async {
    _retry?.cancel();
    _retry = null;
    final api = _api;
    if (api == null) return;
    List<OptionUnderlying> list;
    try {
      list = await api.underlyings();
    } on ApiException catch (e) {
      if (gen != _gen) return;
      final soon = isLaunchingSoon(e);
      _set(state.copyWith(avail: soon ? OptAvail.soon : OptAvail.error, availMsg: e.message));
      _retry = Timer(Duration(seconds: soon ? 120 : 15), () => unawaited(_boot(gen)));
      return;
    }
    if (gen != _gen) return;
    if (list.isEmpty) {
      _set(state.copyWith(avail: OptAvail.soon, underlyings: const []));
      return;
    }
    final u = list.any((x) => x.symbol == state.u) ? state.u : list.first.symbol;
    _set(state.copyWith(underlyings: list, u: u, avail: OptAvail.ready, chainLoading: true));
    await _loadExpiries();
    if (gen != _gen) return;
    await _loadChain();
    if (gen != _gen) return;
    _syncStream();
    _startTimers();
  }

  Future<void> _loadChain() async {
    final u = state.u;
    final expiry = state.expiry;
    final my = ++_seq;
    final api = _api;
    if (api == null) return;
    try {
      final chain = await api.chain(u, expiry);
      if (my != _seq || u != state.u || expiry != state.expiry) return;
      _set(
        state.copyWith(
          chain: chain,
          index: indexOf(chain.rows),
          chainLoading: false,
          bookLive: chain.book?.active ?? false,
          expiry: expiry ?? (chain.expiry.isEmpty ? null : chain.expiry),
        ),
      );
      _applyRemap();
      _syncSeries();
    } on ApiException catch (e) {
      if (my != _seq || u != state.u || expiry != state.expiry) return;
      if (isLaunchingSoon(e) && e.code != 'not_found') {
        _set(state.copyWith(avail: OptAvail.soon, availMsg: e.message, chainLoading: false));
      } else {
        _set(state.copyWith(chainLoading: false));
      }
    }
  }

  /// Load the expiry list and pick the expiry to show (the one on screen rolls when it stopped trading).
  Future<void> _loadExpiries({bool keepExpiry = true, String? prevDate}) async {
    final u = state.u;
    final api = _api;
    if (api == null) return;
    List<OptionExpiry> list;
    try {
      list = await api.expiries(u);
    } on ApiException catch (e) {
      if (u != state.u) return;
      if (e.code == 'options_disabled') _set(state.copyWith(avail: OptAvail.soon, availMsg: e.message));
      return;
    }
    if (u != state.u) return;
    final want = _wantExpiry;
    _wantExpiry = null;
    final pick = pickExpiry(list, keep: keepExpiry ? state.expiry : null, prev: keepExpiry ? null : prevDate, kind: state.prefs.expKind, want: want);
    _applyKind(pick.kind);
    final cur = state.expiry;
    if (keepExpiry && cur != null && pick.date != null && pick.date != cur) {
      _set(state.copyWith(expiries: list));
      _changeExpiry(pick.date!);
      return;
    }
    _set(state.copyWith(expiries: list, expiry: pick.date));
  }

  void _applyKind(String? kind) {
    if (kind == state.prefs.expKind) return;
    final prefs = state.prefs.copyWith(expKind: kind);
    _savePrefs(prefs);
    _set(state.copyWith(prefs: prefs));
  }

  /// Show another expiry: its chain replaces the one on screen; the selected option moves to the same strike there.
  void _changeExpiry(String date) {
    if (date == state.expiry) return;
    final p = parseSeriesCode(state.sel);
    _remap = p != null && state.expiry != null && p.date == state.expiry && p.underlying == state.u ? _Remap(p.strike, p.right, state.sel!) : null;
    _set(state.copyWith(expiry: date, chain: null, index: const {}, chainLoading: true));
    unawaited(_loadChain());
    _syncStream();
  }

  /// The new expiry's chain arrived: select the same strike and right there (a single-option ticket follows).
  void _applyRemap() {
    final m = _remap;
    final c = state.chain;
    if (m == null || c == null || c.expiry != state.expiry || c.underlying != state.u) return;
    _remap = null;
    OptionChainRow? row;
    for (final r in c.rows) {
      if ((r.strike - m.strike).abs() < 1e-9) row = r;
    }
    final q = row?.of(m.right);
    if (row == null || q == null) {
      if (state.sel == m.prevCode) _set(state.copyWith(sel: null));
      return;
    }
    final t = state.ticket;
    final one = t.legs.length == 1 && t.legs.first.series == m.prevCode ? t.legs.first : null;
    _set(
      state.copyWith(
        sel: state.sel == m.prevCode ? q.code : state.sel,
        // another expiry is another price: the trader chooses Buy or Sell again
        ticket: one == null
            ? t
            : t.copyWith(
                armed: false,
                limit: '',
                sl: '',
                tp: '',
                legs: [one.copyWith(series: q.code, expiry: c.expiry, strike: row.strike, strikeLabel: row.strikeLabel)],
              ),
      ),
    );
    _syncSeries();
  }

  /// Once a second: the expiry on screen passed its cut (or stopped trading) → roll to the next one of its kind.
  void _checkRoll() {
    if (state.avail != OptAvail.ready || state.expiry == null || state.expiries.isEmpty) return;
    OptionExpiry? cur;
    for (final e in state.expiries) {
      if (e.date == state.expiry) cur = e;
    }
    if (cur == null || expiryOpen(cur)) return;
    final pick = pickExpiry(state.expiries, keep: state.expiry, kind: state.prefs.expKind);
    if (pick.date == null || pick.date == state.expiry) return;
    _applyKind(pick.kind);
    _changeExpiry(pick.date!);
    Timer(const Duration(seconds: 5), () => unawaited(_loadExpiries()));
  }

  void _startTimers() {
    _expiryTimer ??= Timer.periodic(const Duration(seconds: 60), (_) => unawaited(_loadExpiries()));
    _rollTimer ??= Timer.periodic(const Duration(seconds: 1), (_) => _checkRoll());
    _poll ??= Timer.periodic(const Duration(seconds: 2), (_) {
      // the socket is down for a while: poll the chain instead
      if (state.avail != OptAvail.ready || state.stream == 'open' || _downSince == 0) return;
      if (DateTime.now().millisecondsSinceEpoch - _downSince < 3000) return;
      final life = WidgetsBinding.instance.lifecycleState;
      if (life != null && life != AppLifecycleState.resumed) return;
      if (state.stream != 'polling' && state.stream != 'unavailable') _set(state.copyWith(stream: 'polling'));
      unawaited(_loadChain());
      unawaited(_pollBookFeed());
    });
  }

  /* ---------------- streaming ---------------- */

  void _syncStream() {
    if (state.avail != OptAvail.ready) return;
    if (_stream == null) {
      final session = ref.read(activeTradeSessionProvider);
      if (session == null) return;
      final s = OptionsStream(
        api: ref.read(apiProvider),
        tradeToken: () => ref.read(activeTradeSessionProvider)?.token ?? session.token,
        connector: ref.read(optionsConnectorProvider),
        onFrame: _onFrame,
        onStatus: (st) {
          final now = DateTime.now().millisecondsSinceEpoch;
          if (st == SocketStatus.open) {
            _downSince = 0;
          } else if (_downSince == 0) {
            _downSince = now;
          }
          final name = switch (st) {
            SocketStatus.open => 'open',
            SocketStatus.unavailable => 'unavailable',
            SocketStatus.closed => 'closed',
            SocketStatus.reconnecting => _poll != null ? 'polling' : 'reconnecting',
            _ => 'connecting',
          };
          if (ref.mounted && _stream != null && name != state.stream) _set(state.copyWith(stream: name));
        },
      );
      _stream = s;
      s.start();
    }
    _stream!.setChains([if (state.expiry != null) (u: state.u, expiry: state.expiry!)]);
    _syncSeries();
    _syncBookFeed();
  }

  void _onFrame(Map<String, dynamic> f) {
    if (!ref.mounted) return;
    final r = applyOptFrame(state, f);
    if (r.reload) {
      unawaited(_loadChain());
      return;
    }
    if (!identical(r.state, state)) _set(r.state);
    if (r.chainChanged) {
      _applyRemap();
      _syncSeries();
      _syncBookFeed();
    }
  }

  /// Series to stream outside the chain on screen: open positions, book orders, ticket legs, the selected option.
  void _syncSeries() {
    final s = _stream;
    if (s == null || !ref.mounted) return;
    final codes = <String>{};
    for (final p in ref.read(optBookProvider).positions) {
      codes.add(p.option.series);
    }
    for (final l in state.ticket.legs) {
      codes.add(l.series);
    }
    if (state.sel != null) codes.add(state.sel!);
    s.setSeries(codes.where((c) => !state.index.containsKey(c)));
  }

  /// Depth and trades of the selected series while the book is live.
  void _syncBookFeed() {
    final s = _stream;
    if (s == null) return;
    final codes = state.isBookLive && state.sel != null ? [state.sel!] : const <String>[];
    s.setDepth(codes);
    s.setTape(codes);
  }

  /// The stream is down: poll the selected series' depth and trades (public market data).
  Future<void> _pollBookFeed() async {
    final code = state.sel;
    final api = _api;
    if (code == null || !state.isBookLive || api == null) return;
    SeriesDepth? d;
    List<TapeTrade>? tr;
    try {
      d = await api.depth(code);
    } on ApiException {
      d = null;
    }
    try {
      tr = await api.trades(code);
    } on ApiException {
      tr = null;
    }
    if (!ref.mounted || state.sel != code) return;
    _set(
      state.copyWith(
        depth: d == null ? null : {...state.depth, code: d},
        tape: tr == null
            ? null
            : {
                ...state.tape,
                code: ([...tr]..sort((a, b) => b.t.compareTo(a.t))).take(120).toList(),
              },
      ),
    );
  }

  /// Back in the foreground: reconnect at once.
  void resume() => _stream?.resume();

  /* ---------------- actions ---------------- */

  void retry() {
    _set(state.copyWith(avail: OptAvail.loading));
    unawaited(_boot(_gen));
  }

  void selectUnderlying(String u) {
    if (u == state.u && state.chain != null) return;
    final prefs = state.prefs.copyWith(u: u);
    _savePrefs(prefs);
    final prev = state.expiry;
    _remap = null;
    // like the CFD order panel following the symbol: a single option of the old underlying leaves the ticket
    final t = state.ticket;
    final single = t.legs.length == 1 && t.legs.first.u != u;
    _set(state.copyWith(u: u, prefs: prefs, expiry: null, chain: null, index: const {}, chainLoading: true, sel: null, ticket: single ? t.fresh() : t));
    _syncBookFeed();
    unawaited(() async {
      await _loadExpiries(keepExpiry: false, prevDate: prev);
      await _loadChain();
      _syncStream();
    }());
  }

  /// A date from the list (the expiry chips, the strategy builder): its kind lights up when it is a kind's nearest.
  void selectExpiry(String date) {
    if (date == state.expiry) return;
    _applyKind(kindOfExpiry(state.expiries, date, state.prefs.expKind));
    _changeExpiry(date);
  }

  /// "Show on the chart" of a position: its underlying, expiry and premium chart.
  bool showSeries(String code) {
    final p = parseSeriesCode(code);
    if (p == null) return false;
    if (p.underlying != state.u) {
      _wantExpiry = p.date;
      selectUnderlying(p.underlying);
    } else if (p.date != state.expiry) {
      _set(state.copyWith(sel: null));
      selectExpiry(p.date);
    }
    _set(state.copyWith(sel: code));
    _syncSeries();
    _syncBookFeed();
    return true;
  }

  void setPrefs(OptPrefs Function(OptPrefs p) patch) {
    final prefs = patch(state.prefs);
    _savePrefs(prefs);
    _set(state.copyWith(prefs: prefs));
  }

  void setTradingSoon(bool v) {
    if (state.tradingSoon != v) _set(state.copyWith(tradingSoon: v));
  }

  /// The engine doesn't serve the book routes: fall back to the house-priced flow for this session.
  void setBookOff([bool v = true]) {
    if (state.bookOff == v) return;
    _set(state.copyWith(bookOff: v));
    _syncBookFeed();
  }

  void focus(String? ticket) => _set(state.copyWith(focus: ticket));

  String _id() => 'l${DateTime.now().microsecondsSinceEpoch.toRadixString(36)}${(_uid++).toRadixString(36)}';

  /// A strike's call or put picked in the chain: the selected option and the ticket's option, without a side until
  /// the trader chooses Buy or Sell. `add` ("Add leg", or a strategy already in the ticket) adds it as a leg.
  void select(OptionChainRow row, OptionRight right, {bool add = false}) {
    final q = row.of(right);
    final expiry = state.expiry;
    if (q == null || expiry == null) return;
    final t0 = state.ticket;
    final adding = t0.legs.isNotEmpty && (add || t0.adding || t0.legs.length > 1);
    final leg = TicketLeg(
      id: _id(),
      series: q.code,
      u: state.u,
      expiry: expiry,
      right: right,
      strike: row.strike,
      strikeLabel: row.strikeLabel,
      side: 'buy',
      contracts: t0.legs.isEmpty ? 1 : t0.legs.first.contracts,
    );
    Ticket ticket;
    if (adding) {
      if (t0.legs.any((l) => l.series == q.code)) {
        ticket = t0.copyWith(adding: false);
      } else {
        var legs = [...t0.legs, leg];
        if (legs.length > 8) legs = legs.sublist(legs.length - 8);
        // another underlying in the ticket: start over (one strategy = one underlying)
        if (legs.any((l) => l.u != leg.u)) legs = [leg];
        ticket = t0.copyWith(legs: legs, adding: false, armed: legs.length > 1 || t0.armed, type: legs.length > 1 ? 'market' : t0.type);
      }
    } else if (t0.legs.length == 1 && t0.legs.first.series == q.code) {
      ticket = t0.copyWith(adding: false);
    } else {
      ticket = t0.fresh(legs: [leg]);
    }
    _set(
      state.copyWith(
        sel: q.code,
        ticket: ticket,
        prefs: state.prefs.panel == 'simple' ? state.prefs.copyWith(panel: 'ticket') : null,
      ),
    );
    _syncSeries();
    _syncBookFeed();
  }

  /// A price level tapped in the depth: a limit order at that price, the side that trades with it.
  void prefillLimit(String series, String side, double priceUsd) {
    OptionChainRow? row;
    for (final r in state.chain?.rows ?? const <OptionChainRow>[]) {
      if (r.call?.code == series || r.put?.code == series) row = r;
    }
    final one = state.ticket.legs.length == 1 && state.ticket.legs.first.series == series;
    if (!one) {
      if (row == null) return;
      select(row, row.call?.code == series ? 'call' : 'put');
    }
    final t = state.ticket;
    _set(
      state.copyWith(
        ticket: t.copyWith(
          armed: true,
          bookType: 'limit',
          bookTif: t.bookTif == 'ioc' || t.bookTif == 'fok' ? t.bookTif : 'gtc',
          limit: priceUsd.toStringAsFixed(2),
          legs: [for (final l in t.legs) l.copyWith(side: side)],
        ),
        prefs: state.prefs.panel == 'simple' ? state.prefs.copyWith(panel: 'ticket') : null,
      ),
    );
  }

  /// Buy or Sell on the ticket of a single option.
  void arm(String side) {
    final t = state.ticket;
    if (t.legs.length != 1) return;
    _set(
      state.copyWith(
        ticket: t.copyWith(armed: true, legs: [t.legs.first.copyWith(side: side)]),
      ),
    );
  }

  /// Call ↔ put at the same strike (the ticket's Call / Put switch).
  void flipRight(OptionRight right) {
    final base = state.ticket.single;
    final p = parseSeriesCode(base?.series ?? state.sel);
    final c = state.chain;
    if (p == null || c == null || p.date != c.expiry || p.underlying != c.underlying || p.right == right) return;
    OptionChainRow? row;
    for (final r in c.rows) {
      if ((r.strike - p.strike).abs() < 1e-9) row = r;
    }
    final q = row?.of(right);
    if (row == null || q == null) return;
    final t = state.ticket;
    _set(
      state.copyWith(
        sel: q.code,
        ticket: base == null
            ? t
            : t.copyWith(
                armed: false,
                limit: '',
                sl: '',
                tp: '',
                trigPrice: '',
                legs: [base.copyWith(id: _id(), series: q.code, right: right, strike: row.strike, strikeLabel: row.strikeLabel)],
              ),
      ),
    );
    _syncSeries();
    _syncBookFeed();
  }

  /// "Add leg": the next strike picked in the chain is added to the ticket.
  void setAdding(bool v) {
    if (state.ticket.adding != v) _set(state.copyWith(ticket: state.ticket.copyWith(adding: v)));
  }

  /// After a fill: a single option stays selected (no side, protection cleared); a strategy leaves the ticket.
  void afterFill() {
    final t = state.ticket;
    _set(
      state.copyWith(
        ticket: t.legs.length == 1 ? t.fresh(legs: t.legs) : Ticket(tif: t.tif, bookTif: t.bookTif == 'gtd' ? 'gtc' : t.bookTif),
      ),
    );
    _syncSeries();
  }

  /// Legs from the strategy builder or Quick trade (their sides are chosen).
  void setLegs(List<TicketLeg> legs) {
    final t = state.ticket;
    _set(
      state.copyWith(
        sel: legs.isEmpty ? state.sel : legs.first.series,
        ticket: t.fresh(armed: true, legs: [for (final l in legs.take(8)) l.copyWith(id: _id())]),
      ),
    );
    _syncSeries();
    _syncBookFeed();
  }

  void updateLeg(String id, {String? side, int? contracts}) {
    final t = state.ticket;
    _set(
      state.copyWith(
        ticket: t.copyWith(
          legs: [for (final l in t.legs) l.id == id ? l.copyWith(side: side, contracts: contracts) : l],
        ),
      ),
    );
  }

  void removeLeg(String id) {
    final t = state.ticket;
    // the leg left of a strategy keeps the side it had
    _set(
      state.copyWith(
        ticket: t.copyWith(armed: t.legs.length > 1 || t.armed, legs: t.legs.where((l) => l.id != id).toList()),
      ),
    );
    _syncSeries();
  }

  void setTicket(Ticket Function(Ticket t) patch) => _set(state.copyWith(ticket: patch(state.ticket)));

  void clearTicket() {
    _set(state.copyWith(ticket: state.ticket.fresh()));
    _syncSeries();
  }

  /// Book orders changed: their series stay streamed.
  void syncOrderSeries(Iterable<String> codes) {
    final s = _stream;
    if (s == null) return;
    final all = <String>{...codes};
    for (final p in ref.read(optBookProvider).positions) {
      all.add(p.option.series);
    }
    for (final l in state.ticket.legs) {
      all.add(l.series);
    }
    if (state.sel != null) all.add(state.sel!);
    s.setSeries(all.where((c) => !state.index.containsKey(c)));
  }
}

final optionsProvider = NotifierProvider.autoDispose<OptionsController, OptState>(OptionsController.new);

/* ------------------------------------------------------------------ */
/* Working book orders (`GET …/book/orders?status=open`, every 3 s)    */
/* ------------------------------------------------------------------ */

@immutable
class BookOrdersView {
  const BookOrdersView({this.open = const [], this.loaded = false});
  final List<BookOrder> open;
  final bool loaded;
}

class BookOrdersController extends Notifier<BookOrdersView> {
  Timer? _timer;
  bool _inflight = false;

  @override
  BookOrdersView build() {
    final live = ref.watch(optionsProvider.select((s) => s.isBookLive));
    ref.watch(activeTradeSessionProvider.select((s) => s?.login));
    _timer?.cancel();
    ref.onDispose(() => _timer?.cancel());
    if (!live) return const BookOrdersView();
    Future.microtask(refresh);
    _timer = Timer.periodic(const Duration(seconds: 3), (_) {
      final life = WidgetsBinding.instance.lifecycleState;
      if (life == null || life == AppLifecycleState.resumed) unawaited(refresh());
    });
    return const BookOrdersView();
  }

  Future<void> refresh() async {
    final api = ref.read(optionsApiProvider);
    if (api == null || _inflight || !ref.mounted) return;
    _inflight = true;
    try {
      final list = await api.bookOrders();
      if (!ref.mounted) return;
      list.sort((a, b) => msOf(b.createdAt).compareTo(msOf(a.createdAt)));
      state = BookOrdersView(open: list, loaded: true);
      ref.read(optionsProvider.notifier).syncOrderSeries(list.map((o) => o.series));
    } on ApiException catch (e) {
      if (ref.mounted && bookMissing(e)) ref.read(optionsProvider.notifier).setBookOff();
    } finally {
      _inflight = false;
    }
  }
}

final bookOrdersProvider = NotifierProvider.autoDispose<BookOrdersController, BookOrdersView>(BookOrdersController.new);

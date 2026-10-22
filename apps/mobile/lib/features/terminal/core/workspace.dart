// Kalks Trader workspace on this phone (web: lib/store.tsx Workspace, saved per browser): favourites, the watchlist
// segment, one-click trading, sounds, the default volume, max deviation, the chart's symbol and timeframe, price
// alerts and the last account. Kept in the app's preferences (not secret).
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../../../core/prefs.dart';
import '../chart/indicators.dart';

/// A local price alert (web PriceAlert): fires once when the bid crosses `price`.
@immutable
class PriceAlert {
  const PriceAlert({required this.id, required this.symbol, required this.cond, required this.price, this.active = true});
  final String id, symbol;

  /// above | below
  final String cond;
  final double price;
  final bool active;

  PriceAlert copyWith({double? price, bool? active}) =>
      PriceAlert(id: id, symbol: symbol, cond: cond, price: price ?? this.price, active: active ?? this.active);

  Map<String, dynamic> toJson() => {'id': id, 'symbol': symbol, 'cond': cond, 'price': price, 'active': active};
  static PriceAlert? fromJson(Object? j) {
    if (j is! Map) return null;
    final p = j['price'];
    if (p is! num || j['symbol'] is! String) return null;
    return PriceAlert(
      id: '${j['id']}',
      symbol: j['symbol'] as String,
      cond: j['cond'] == 'below' ? 'below' : 'above',
      price: p.toDouble(),
      active: j['active'] != false,
    );
  }
}

/// A symbol's chart (web ChartTab type + indicators): the chart type and the indicators, saved per symbol.
@immutable
class ChartSettings {
  const ChartSettings({this.type = 'candles', this.indicators = const []});

  /// candles | bars | line | area
  final String type;
  final List<IndInstance> indicators;

  ChartSettings copyWith({String? type, List<IndInstance>? indicators}) => ChartSettings(type: type ?? this.type, indicators: indicators ?? this.indicators);

  Map<String, dynamic> toJson() => {
    'type': type,
    'indicators': [for (final i in indicators) i.toJson()],
  };

  @override
  bool operator ==(Object other) => other is ChartSettings && other.type == type && listEquals(other.indicators, indicators);

  @override
  int get hashCode => Object.hash(type, Object.hashAll(indicators));

  static ChartSettings? fromJson(Object? j) {
    if (j is! Map) return null;
    final type = j['type'];
    return ChartSettings(
      type: kChartTypes.contains(type) ? type as String : 'candles',
      indicators: [for (final i in (j['indicators'] is List ? j['indicators'] as List : const [])) ?IndInstance.fromJson(i)],
    );
  }
}

/// The defaults are the web's (store.tsx defaultWorkspace): XAUUSD M15, volume 0.50, one-click off, sounds on.
@immutable
class Workspace {
  const Workspace({
    this.favourites = const ['XAUUSD', 'EURUSD', 'NAS100', 'BTCUSD', 'GBPUSD'],
    this.segment = 'all',
    this.oneClick = false,
    this.sound = true,
    this.lot = 0.5,
    this.maxDeviation,
    this.symbol = 'XAUUSD',
    this.tf = 'M15',
    this.alerts = const [],
    this.lastLogin,
    this.charts = const {},
    this.indicatorFavourites = kDefaultIndicatorFavourites,
    this.advancedQuotes = false,
  });

  final List<String> favourites;

  /// Watchlist segment: all | forex | metals | indices | energies | crypto | stocks | favourites
  final String segment;
  final bool oneClick;

  /// Sound on fills.
  final bool sound;

  /// The default (one-click) volume.
  final double lot;

  /// Max price change (points) from the price on screen; null = any price.
  final int? maxDeviation;

  /// The chart's market and timeframe (M1 … MN).
  final String symbol, tf;
  final List<PriceAlert> alerts;
  final String? lastLogin;

  /// Chart type and indicators per symbol (a symbol without its own: candles with EMA 50 and SMA 20).
  final Map<String, ChartSettings> charts;

  /// Starred indicator types of the indicators list.
  final List<String> indicatorFavourites;

  /// The quotes' Advanced view: a low · high · last tick time line under every market (Simple: prices only).
  final bool advancedQuotes;

  /// The chart of a symbol (the web's default chart until changed).
  ChartSettings chartOf(String symbol) => charts[symbol] ?? ChartSettings(indicators: _defaultIndicators);

  Workspace copyWith({
    List<String>? favourites,
    String? segment,
    bool? oneClick,
    bool? sound,
    double? lot,
    int? maxDeviation,
    bool clearDeviation = false,
    String? symbol,
    String? tf,
    List<PriceAlert>? alerts,
    String? lastLogin,
    Map<String, ChartSettings>? charts,
    List<String>? indicatorFavourites,
    bool? advancedQuotes,
  }) => Workspace(
    favourites: favourites ?? this.favourites,
    segment: segment ?? this.segment,
    oneClick: oneClick ?? this.oneClick,
    sound: sound ?? this.sound,
    lot: lot ?? this.lot,
    maxDeviation: clearDeviation ? null : (maxDeviation ?? this.maxDeviation),
    symbol: symbol ?? this.symbol,
    tf: tf ?? this.tf,
    alerts: alerts ?? this.alerts,
    lastLogin: lastLogin ?? this.lastLogin,
    charts: charts ?? this.charts,
    indicatorFavourites: indicatorFavourites ?? this.indicatorFavourites,
    advancedQuotes: advancedQuotes ?? this.advancedQuotes,
  );

  Map<String, dynamic> toJson() => {
    'favourites': favourites,
    'segment': segment,
    'oneClick': oneClick,
    'sound': sound,
    'lot': lot,
    'maxDeviation': maxDeviation,
    'symbol': symbol,
    'tf': tf,
    'alerts': [for (final a in alerts) a.toJson()],
    'lastLogin': lastLogin,
    'charts': {for (final e in charts.entries) e.key: e.value.toJson()},
    'indicatorFavourites': indicatorFavourites,
    'advancedQuotes': advancedQuotes,
  };

  static Workspace fromJson(Map<String, dynamic>? j) {
    if (j == null) return const Workspace();
    const d = Workspace();
    final fav = j['favourites'];
    final lot = j['lot'];
    final dev = j['maxDeviation'];
    final charts = j['charts'];
    final indFav = j['indicatorFavourites'];
    return Workspace(
      favourites: fav is List ? fav.whereType<String>().toList() : d.favourites,
      segment: j['segment'] is String ? j['segment'] as String : d.segment,
      oneClick: j['oneClick'] == true,
      sound: j['sound'] != false,
      lot: lot is num && lot > 0 ? lot.toDouble() : d.lot,
      maxDeviation: dev is num ? dev.toInt() : null,
      symbol: j['symbol'] is String ? j['symbol'] as String : d.symbol,
      tf: j['tf'] is String ? j['tf'] as String : d.tf,
      alerts: [for (final a in (j['alerts'] as List? ?? const [])) ?PriceAlert.fromJson(a)],
      lastLogin: j['lastLogin'] as String?,
      charts: {
        if (charts is Map)
          for (final e in charts.entries) '${e.key}': ?ChartSettings.fromJson(e.value),
      },
      indicatorFavourites: indFav is List ? indFav.whereType<String>().toList() : d.indicatorFavourites,
      advancedQuotes: j['advancedQuotes'] == true,
    );
  }
}

/// The default chart's indicators, made once so their ids stay put until the client changes the chart.
final List<IndInstance> _defaultIndicators = defaultIndicators();

class WorkspaceController extends Notifier<Workspace> {
  static const _key = 'trader.workspace';

  @override
  Workspace build() => Workspace.fromJson(ref.read(prefsProvider).featureJson(_key));

  void update(Workspace Function(Workspace w) f) {
    state = f(state);
    ref.read(prefsProvider).setFeatureJson(_key, state.toJson()).ignore();
  }

  void toggleFavourite(String symbol) =>
      update((w) => w.copyWith(favourites: w.favourites.contains(symbol) ? (w.favourites.where((s) => s != symbol).toList()) : [...w.favourites, symbol]));

  void addAlert(String symbol, double price, double bid) => update(
    (w) => w.copyWith(
      alerts: [
        PriceAlert(id: DateTime.now().microsecondsSinceEpoch.toRadixString(36), symbol: symbol, cond: price >= bid ? 'above' : 'below', price: price),
        ...w.alerts,
      ],
    ),
  );

  void updateAlert(String id, {double? price, bool? active}) => update(
    (w) => w.copyWith(
      alerts: [for (final a in w.alerts) a.id == id ? a.copyWith(price: price, active: active) : a],
    ),
  );

  void removeAlert(String id) => update((w) => w.copyWith(alerts: w.alerts.where((a) => a.id != id).toList()));

  /// Changes one symbol's chart (type and / or indicators); saved per symbol.
  void updateChart(String symbol, ChartSettings Function(ChartSettings c) f) => update((w) => w.copyWith(charts: {...w.charts, symbol: f(w.chartOf(symbol))}));

  void toggleIndicatorFavourite(String type) => update(
    (w) => w.copyWith(
      indicatorFavourites: w.indicatorFavourites.contains(type) ? w.indicatorFavourites.where((x) => x != type).toList() : [...w.indicatorFavourites, type],
    ),
  );
}

final workspaceProvider = NotifierProvider<WorkspaceController, Workspace>(WorkspaceController.new);

/// Timeframes of the chart (web TIMEFRAMES) and their length in seconds.
const List<String> kTimeframes = ['M1', 'M5', 'M15', 'M30', 'H1', 'H4', 'D1', 'W1', 'MN'];
const Map<String, int> kTfSeconds = {'M1': 60, 'M5': 300, 'M15': 900, 'M30': 1800, 'H1': 3600, 'H4': 14400, 'D1': 86400, 'W1': 604800, 'MN': 2592000};

/// Watchlist segments (web SEGMENTS).
const List<String> kSegments = ['all', 'forex', 'metals', 'indices', 'energies', 'crypto', 'stocks', 'favourites'];

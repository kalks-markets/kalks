// The chart's session cache of candle history (instant timeframe switching): merging a background refresh into
// what is drawn, the neighbours fetched ahead, one request per series, and the chart window using it.
import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/chart/history_cache.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/workspace.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

Candle c(int t, [double close = 1]) => Candle(t, 1, 2, 0.5, close, 1);
List<Candle> run(int from, int to) => [for (var t = from; t <= to; t += 60) c(t)];
List<int> times(List<Candle> l) => [for (final x in l) x.t];

/// A chart source the test drives: history answers when the test completes them, live bars on demand.
class FakeSource extends ChartSource {
  FakeSource(this.key);
  @override
  final String key;
  @override
  int get digits => 2;
  @override
  bool get prefetch => true;

  final Map<String, List<Completer<List<Candle>>>> asked = {};
  final Map<String, void Function(Candle)> live = {};
  final List<String> events = [];
  final List<void Function()> resync = [];

  @override
  Future<List<Candle>> recent(String tf, int count) {
    events.add('recent $tf $count');
    final c = Completer<List<Candle>>();
    (asked[tf] ??= []).add(c);
    return c.future;
  }

  @override
  void Function()? resyncs(void Function() onResync) {
    resync.add(onResync);
    return () => resync.remove(onResync);
  }

  @override
  Future<List<Candle>> history(String tf, {int? to}) {
    events.add('history $tf');
    final c = Completer<List<Candle>>();
    (asked[tf] ??= []).add(c);
    return c.future;
  }

  @override
  void Function()? bars(String tf, void Function(Candle bar) onBar) {
    events.add('bars $tf');
    live[tf] = onBar;
    return () => live.remove(tf);
  }

  void answer(String tf, List<Candle> bars) {
    for (final c in asked[tf] ?? const <Completer<List<Candle>>>[]) {
      if (!c.isCompleted) c.complete(bars);
    }
  }
}

void main() {
  setUp(ChartHistoryCache.instance.clear);

  group('mergeHistory', () {
    test('nothing shown: the page is drawn whole', () {
      final m = mergeHistory(const [], run(0, 600));
      expect(times(m.bars), times(run(0, 600)));
      expect(m.tail, isNull);
    });

    test('a cache that lines up: only its last bar and the newer ones go to the chart', () {
      final shown = run(0, 600);
      final fresh = [...run(120, 540), c(600, 1.7), ...run(660, 780)];
      final m = mergeHistory(shown, fresh);
      expect(times(m.bars), times(run(0, 780)));
      expect(times(m.tail!), [600, 660, 720, 780]);
      // the cached forming bar is replaced by its closed version from the server
      expect(m.tail!.first.c, 1.7);
    });

    test('older pages scrolled back to and a forming bar newer than the page are kept', () {
      final shown = [...run(0, 600), c(660, 1.9)];
      final m = mergeHistory(shown, run(300, 600));
      expect(times(m.bars), times(run(0, 660)));
      expect(m.bars.last.c, 1.9);
      expect(times(m.tail!), [660]);
    });

    test('a gap or a different candle grid redraws everything', () {
      // the cache missed bars (a stale cache and a live bar after it)
      final stale = [...run(0, 300), c(900)];
      expect(mergeHistory(stale, run(0, 900)).tail, isNull);
      // too many new bars to send one by one
      expect(mergeHistory(run(0, 600), run(0, 600 + 60 * 100)).tail, isNull);
    });

    test('an empty answer changes nothing', () {
      final shown = run(0, 600);
      final m = mergeHistory(shown, const []);
      expect(m.bars, same(shown));
      expect(m.tail, isEmpty);
    });
  });

  group('refreshBars', () {
    Candle k(int t, double o, double h, double l, double cl) => Candle(t, o, h, l, cl, 1);
    test('a bar that closed takes its stored values; the forming bar keeps its live close', () {
      final shown = [k(0, 1, 2, 0.5, 1.5), k(60, 1.5, 1.8, 1.4, 1.6), k(120, 1.62, 1.62, 1.62, 1.65)];
      // the server's final values of the bar at 60 (a late tick), and the forming bar as it was a moment ago
      final fresh = [k(60, 1.5, 1.9, 1.4, 1.62), k(120, 1.62, 1.7, 1.6, 1.63)];
      final r = refreshBars(shown, fresh)!;
      expect(times(r.bars), [0, 60, 120]);
      expect([r.bars[1].h, r.bars[1].c], [1.9, 1.62]);
      expect([r.bars[2].h, r.bars[2].l, r.bars[2].c], [1.7, 1.6, 1.65]);
      expect(r.tail, isNull, reason: 'a bar before the last changed: redraw');
      expect(refreshBars(r.bars, fresh), isNull, reason: 'nothing changed');
    });

    test('only the forming bar changed: an update in place', () {
      final shown = [c(0), c(60)];
      final r = refreshBars(shown, [const Candle(60, 1, 3, 0.5, 1, 1)])!;
      expect(times(r.tail!), [60]);
    });

    test('after a reconnect the latest page fills the bars missed; a page past a longer gap replaces them', () {
      final shown = run(0, 600);
      final r = refreshBars(shown, run(300, 1200), page: true)!;
      expect(times(r.bars), times(run(0, 1200)));
      expect(r.tail, isNotNull);
      final far = refreshBars(shown, run(6000, 6600), page: true)!;
      expect(times(far.bars), times(run(6000, 6600)));
      expect(far.tail, isNull);
    });
  });

  group('ChartHistoryCache', () {
    test('keeps the most recently used series up to its capacity, the live bar keeps it current', () {
      final cache = ChartHistoryCache(capacity: 2);
      cache.put('A|M1', run(0, 120));
      cache.put('A|M5', run(0, 120));
      cache.get('A|M1');
      cache.put('A|M15', run(0, 120));
      expect(cache.has('A|M1'), isTrue);
      expect(cache.has('A|M5'), isFalse);
      cache.upsert('A|M1', c(120, 3));
      cache.upsert('A|M1', c(180, 4));
      cache.upsert('A|M1', c(60, 9));
      expect(times(cache.get('A|M1')!), [0, 60, 120, 180]);
      expect(cache.get('A|M1')![2].c, 3);
      expect(cache.get('A|M1')![1].c, 1);
    });

    test('one request per series: a chart opening a timeframe being fetched ahead waits for that answer', () async {
      final cache = ChartHistoryCache();
      var calls = 0;
      final done = Completer<List<Candle>>();
      Future<List<Candle>> load() {
        calls++;
        return done.future;
      }

      final ahead = cache.prefetch('A|H1', load);
      final open = cache.fetch('A|H1', load);
      expect(calls, 1);
      done.complete(run(0, 120));
      expect(times(await open), [0, 60, 120]);
      await ahead;
      expect(cache.has('A|H1'), isTrue);
      // cached: no request ahead of time any more
      await cache.prefetch('A|H1', load);
      expect(calls, 1);
    });

    test('the neighbours of a timeframe on the strip', () {
      expect(neighbourTimeframes('M15', kTimeframes), ['M30', 'M5']);
      expect(neighbourTimeframes('M1', kTimeframes), ['M5']);
      expect(neighbourTimeframes('MN', kTimeframes), ['W1']);
      expect(neighbourTimeframes('X', kTimeframes), isEmpty);
    });
  });

  group('the chart window', () {
    Future<void> pump(WidgetTester tester, ProviderContainer container, Widget chart) async {
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: container,
          child: MaterialApp(
            theme: KTheme.trader(Brightness.dark),
            builder: (context, child) => I18nScope(t: container.read(tProvider), child: child!),
            home: SizedBox(width: 400, height: 500, child: chart),
          ),
        ),
      );
      await tester.pump();
    }

    testWidgets('a timeframe seen before draws from the cache and ticks before its refresh answers', (tester) async {
      TerminalChart.forceNative = true;
      late Harness h;
      await tester.runAsync(() async {
        h = await Harness.create(extra: [marketFeedProvider.overrideWith((ref) => FixedFeed(const {}))]);
      });
      final src = FakeSource('FAKE');
      await pump(tester, h.container, TerminalChart(symbol: 'FAKE', tf: 'M5', source: src));
      // first visit: live bars wait for the history
      expect(src.events, ['history M5']);
      src.answer('M5', run(0, 600));
      await tester.pump();
      // then live bars, and the strip's neighbours are fetched ahead
      expect(src.events, ['history M5', 'bars M5', 'history M15', 'history M1']);
      src.answer('M15', run(0, 600));
      src.answer('M1', run(0, 600));
      await tester.pump();
      expect(ChartHistoryCache.instance.has('FAKE|M15'), isTrue);

      // switching to a prefetched timeframe: live at once, the refresh behind it
      src.events.clear();
      await pump(tester, h.container, TerminalChart(symbol: 'FAKE', tf: 'M15', source: src));
      expect(src.events.take(2), ['bars M15', 'history M15']);
      // a tick before the refresh answers goes into the cache too
      src.live['M15']!(c(660, 1.3));
      src.answer('M15', run(0, 660));
      await tester.pump();
      expect(ChartHistoryCache.instance.get('FAKE|M15')!.last.t, 660);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(() async => h.container.dispose());
      TerminalChart.forceNative = false;
    });

    testWidgets('a closed bar takes its stored values; a reconnect brings the bars missed meanwhile', (tester) async {
      TerminalChart.forceNative = true;
      late Harness h;
      await tester.runAsync(() async {
        h = await Harness.create(extra: [marketFeedProvider.overrideWith((ref) => FixedFeed(const {}))]);
      });
      final src = FakeSource('LIVE');
      await pump(tester, h.container, TerminalChart(symbol: 'LIVE', tf: 'M1', source: src));
      src.answer('M1', run(0, 600));
      await tester.pump();
      final key = ChartHistoryCache.keyOf('LIVE', 'M1');

      // the minute at 600 closes at 1.4 as far as the stream showed, the next one starts
      src.live['M1']!(c(600, 1.4));
      src.live['M1']!(c(660, 1.41));
      src.events.clear();
      await tester.pump(TerminalChart.settleDelay);
      expect(src.events, ['recent M1 3']);
      // stored: the minute at 600 closed at 1.45 (a last tick the stream did not deliver)
      src.answer('M1', [c(540), c(600, 1.45), c(660, 1.40)]);
      await tester.pump();
      var cached = ChartHistoryCache.instance.get(key)!;
      expect(cached.firstWhere((x) => x.t == 600).c, 1.45);
      expect(cached.last.c, 1.41, reason: 'the forming bar keeps its live close');

      // the stream was down for three minutes: the bars it missed come with the latest page
      src.events.clear();
      for (final r in [...src.resync]) {
        r();
      }
      expect(src.events, ['history M1']);
      src.answer('M1', run(0, 900));
      await tester.pump();
      cached = ChartHistoryCache.instance.get(key)!;
      expect(times(cached), times(run(0, 900)));

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(() async => h.container.dispose());
      TerminalChart.forceNative = false;
    });
  });
}

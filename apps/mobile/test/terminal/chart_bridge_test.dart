// The message codec between the app and the chart page (assets/chart/chart.html): commands in, gestures out.
import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/chart/chart_bridge.dart';
import 'package:kalks/features/terminal/chart/chart_surface.dart';
import 'package:kalks/features/terminal/chart/indicators.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';

void main() {
  const palette = ChartPalette(
    dark: true,
    bg: '#0a0a0d',
    grid: 'rgba(255,255,255,0.035)',
    line: 'rgba(255,255,255,0.07)',
    up: '#2f7bff',
    down: '#f04438',
    gold: '#e9b949',
    warn: '#f59e0b',
    ember: '#ff5a1f',
    fg: '#f5f5f7',
    fg2: '#a1a1aa',
    fg3: '#8b8b96',
    label: '#26262e',
    panel: 'rgba(21,21,26,1.000)',
  );

  Map<String, dynamic> dec(String s) => jsonDecode(s) as Map<String, dynamic>;

  group('commands', () {
    test('init carries the precision, the time axis, the colours, the chart type and the indicators', () {
      const ema = IndInstance(uid: 'a1', type: 'ema', params: {'period': 50, 'source': 'close'});
      final m = dec(
        ChartCmd.init(
          digits: 5,
          intraday: true,
          palette: palette,
          symbol: 'EURUSD',
          tf: 'M15',
          step: 900,
          chartType: 'area',
          indicators: [ema.toJson()],
          texts: const {'more': '+{count} more'},
        ),
      );
      expect(m['symbol'], 'EURUSD');
      expect(m['tf'], 'M15');
      expect(m['step'], 900);
      expect(m['chartType'], 'area');
      expect(m['indicators'], [
        {
          'uid': 'a1',
          'type': 'ema',
          'params': {'period': 50, 'source': 'close'},
          'visible': true,
        },
      ]);
      expect(m['texts'], {'more': '+{count} more'});
      expect(m['type'], 'init');
      expect(m['digits'], 5);
      expect(m['intraday'], isTrue);
      expect((m['palette'] as Map)['up'], '#2f7bff');
      expect((m['palette'] as Map)['dark'], isTrue);
    });

    test('bars are compact arrays [t, o, h, l, c, v]', () {
      final m = dec(ChartCmd.bars([(t: 1000, o: 1.1, h: 1.2, l: 1.0, c: 1.15, v: 42)]));
      expect(m['type'], 'bars');
      expect(m['bars'], [
        [1000, 1.1, 1.2, 1.0, 1.15, 42.0],
      ]);
      expect(dec(ChartCmd.bar((t: 1060, o: 1, h: 2, l: 0.5, c: 1.5, v: 3)))['bar'], [1060, 1.0, 2.0, 0.5, 1.5, 3.0]);
      final older = dec(ChartCmd.older(const [], exhausted: true));
      expect(older['bars'], isEmpty);
      expect(older['exhausted'], isTrue);
    });

    test('quote and trade lines', () {
      expect(dec(ChartCmd.quote(1.0845, 1.0846)), {'type': 'quote', 'bid': 1.0845, 'ask': 1.0846});
      final m = dec(
        ChartCmd.lines(const [
          ChartLine(id: 'sl:7', kind: 'sl', price: 1.08, label: 'SL', side: 'buy', draggable: true, note: '-12.40', tone: 'down', closable: true),
        ]),
      );
      expect(m['lines'], [
        {'id': 'sl:7', 'kind': 'sl', 'price': 1.08, 'label': 'SL', 'side': 'buy', 'drag': true, 'note': '-12.40', 'tone': 'down', 'close': true},
      ]);
    });

    test('indicators and chart type', () {
      const rsi = IndInstance(
        uid: 'r',
        type: 'rsi',
        visible: false,
        style: {'rsi': IndStyle(color: 'info', width: 2)},
        levels: [80, 20],
      );
      final m = dec(ChartCmd.indicators([rsi.toJson()]));
      expect(m['type'], 'indicators');
      expect(m['indicators'], [
        {
          'uid': 'r',
          'type': 'rsi',
          'params': <String, Object>{},
          'visible': false,
          'style': {
            'rsi': {'color': 'info', 'width': 2},
          },
          'levels': [80.0, 20.0],
        },
      ]);
      expect(dec(ChartCmd.chartType('bars')), {'type': 'chartType', 'chartType': 'bars'});
      // an instance survives a round trip through the workspace JSON
      expect(IndInstance.fromJson(jsonDecode(jsonEncode(rsi.toJson()))), rsi);
    });

    test('a position line carries its S / T handles (with the pip size and stops level); other lines do not', () {
      final m = dec(
        ChartCmd.lines(const [
          ChartLine(id: 'pos:7', kind: 'pos', price: 1.085, label: 'BUY 0.10', side: 'buy', draggable: true, handles: 'st', pip: 0.0001, gap: 0.0001),
          ChartLine(id: 'tp:8', kind: 'tp', price: 1.09, label: 'TP', side: 'buy'),
        ]),
      );
      final pos = (m['lines'] as List).first as Map;
      expect(pos['handles'], 'st');
      expect(pos['pip'], 0.0001);
      expect(pos['gap'], 0.0001);
      expect(((m['lines'] as List).last as Map).containsKey('handles'), isFalse);
      // a line at another price keeps everything else
      const a = ChartLine(id: 'pos:7', kind: 'pos', price: 1.085, label: 'BUY 0.10', side: 'buy', handles: 's', pip: 0.0001);
      expect(a.at(1.08).price, 1.08);
      expect(a.at(1.08).handles, 's');
      expect(a.at(1.085), a);
      expect(a == a.at(1.085), isTrue);
      expect(a == const ChartLine(id: 'pos:7', kind: 'pos', price: 1.085, label: 'BUY 0.10', side: 'buy', handles: 't', pip: 0.0001), isFalse);
    });

    test('an S / T handle makes the missing stop line of its position, only while the position shows that handle', () {
      const pos = ChartLine(id: 'pos:7', kind: 'pos', price: 1.085, label: 'SELL 0.10', side: 'sell', draggable: true, handles: 't');
      final tp = lineForHandle(const [pos], 'tp:7');
      expect(tp, isNotNull);
      expect(tp!.kind, 'tp');
      expect(tp.label, 'TP');
      expect(tp.side, 'sell');
      expect(tp.price, 1.085);
      expect(tp.ref, '7');
      // the position has a stop loss already (no S), another ticket, not a stop, malformed
      expect(lineForHandle(const [pos], 'sl:7'), isNull);
      expect(lineForHandle(const [pos], 'tp:8'), isNull);
      expect(lineForHandle(const [pos], 'pnd:7'), isNull);
      expect(lineForHandle(const [pos], 'tp7'), isNull);
    });

    test('stops from a handle: the order tickets\' starting distance, held on the valid side (web trade-handles.ts)', () {
      // the same cases as apps/terminal/tests/trade-handles.test.mjs
      expect(defaultStop('sl', 'buy', bid: 1.085, ask: 1.0851, pip: 0.0001, gap: 0, digits: 5), 1.084);
      expect(defaultStop('tp', 'buy', bid: 1.085, ask: 1.0851, pip: 0.0001, gap: 0, digits: 5), 1.087);
      expect(defaultStop('sl', 'sell', bid: 1.085, ask: 1.0851, pip: 0.0001, gap: 0, digits: 5), 1.0861);
      expect(defaultStop('tp', 'sell', bid: 1.085, ask: 1.0851, pip: 0.0001, gap: 0, digits: 5), 1.0831);
      expect(defaultStop('sl', 'buy', bid: 2650.0, ask: 2650.8, pip: 0.1, gap: 0, digits: 2), 2648.4);
      expect(defaultStop('tp', 'sell', bid: 2650.0, ask: 2650.8, pip: 0.1, gap: 0, digits: 2), 2647.6);
      expect(defaultStop('sl', 'buy', bid: 100.0, ask: 100.02, pip: 0.01, gap: 0.5, digits: 2), 99.5);
      expect(clampStop('sl', 'buy', 1.0899, bid: 1.085, ask: 1.0851, gap: 0.0001, digits: 5), 1.0849);
      expect(clampStop('sl', 'buy', 1.08, bid: 1.085, ask: 1.0851, gap: 0.0001, digits: 5), 1.08);
      expect(clampStop('tp', 'buy', 1.08, bid: 1.085, ask: 1.0851, gap: 0.0001, digits: 5), 1.0851);
      expect(clampStop('sl', 'sell', 1.08, bid: 1.085, ask: 1.0851, gap: 0.0001, digits: 5), 1.0852);
      expect(clampStop('tp', 'sell', 1.09, bid: 1.085, ask: 1.0851, gap: 0.0001, digits: 5), 1.085);
      expect(clampStop('sl', 'buy', 1.085, bid: 1.085, ask: 1.0851, gap: 0, digits: 5), 1.08499);
    });

    test('a line knows its ticket and compares by value', () {
      const a = ChartLine(id: 'pnd:49434302', kind: 'pending', price: 2628.5, label: 'BUY LIMIT 0.30');
      expect(a.ref, '49434302');
      expect(a, const ChartLine(id: 'pnd:49434302', kind: 'pending', price: 2628.5, label: 'BUY LIMIT 0.30'));
      expect(a == const ChartLine(id: 'pnd:49434302', kind: 'pending', price: 2629, label: 'BUY LIMIT 0.30'), isFalse);
    });
  });

  group('events', () {
    test('ready, drag, tap, older, long press', () {
      expect(ChartEvent.decode('{"type":"ready"}'), isA<ChartReady>());
      final d = ChartEvent.decode('{"type":"drag","id":"tp:9","price":2690.5}') as ChartLineDragged;
      expect(d.id, 'tp:9');
      expect(d.price, 2690.5);
      expect((ChartEvent.decode('{"type":"tap","id":"pos:9"}') as ChartLineTapped).id, 'pos:9');
      expect((ChartEvent.decode('{"type":"close","id":"sl:9"}') as ChartLineClosed).id, 'sl:9');
      expect((ChartEvent.decode('{"type":"older","before":1700000000}') as ChartNeedsOlder).before, 1700000000);
      expect((ChartEvent.decode('{"type":"long","price":1.0832}') as ChartLongPress).price, 1.0832);
      expect((ChartEvent.decode('{"type":"dragstart","id":"sl:9"}') as ChartDragStarted).id, 'sl:9');
      expect((ChartEvent.decode('{"type":"ind","uid":"k2"}') as ChartIndicatorTapped).uid, 'k2');
    });

    test('malformed or unknown messages are ignored', () {
      expect(ChartEvent.decode('not json'), isNull);
      expect(ChartEvent.decode('[1,2]'), isNull);
      expect(ChartEvent.decode('{"type":"drag","id":"x"}'), isNull);
      expect(ChartEvent.decode('{"type":"drag","id":3,"price":1}'), isNull);
      expect(ChartEvent.decode('{"type":"nope"}'), isNull);
    });
  });

  test('the controller queues commands until the page is ready, and replays init', () {
    final c = ChartSurfaceController();
    final got = <String>[];
    c.send(ChartCmd.init(digits: 2, intraday: false, palette: palette));
    c.send(ChartCmd.quote(1, 2));
    c.attach(got.addAll);
    expect(got.length, 2);
    expect(dec(got.first)['type'], 'init');
    c.detach();
    c.send(ChartCmd.quote(3, 4));
    final again = <String>[];
    c.attach(again.addAll);
    // a re-attached page (rebuilt WebView) gets the init again, then what was queued
    expect(again.map((s) => dec(s)['type']), ['init', 'quote']);
    c.dispose();
  });

  testWidgets('live updates are coalesced to one delivery per frame: the last quote, bar and lines win', (tester) async {
    final c = ChartSurfaceController();
    final calls = <List<String>>[];
    c.attach(calls.add);
    for (var i = 0; i < 10; i++) {
      c.send(ChartCmd.quote(1.0 + i, 2.0 + i));
      c.send(ChartCmd.bar((t: 600, o: 1, h: 2, l: 0.5, c: 1.0 + i, v: 3)));
      c.send(ChartCmd.lines([ChartLine(id: 'sl:1', kind: 'sl', price: 1.0 + i, label: 'SL')]));
    }
    c.send(ChartCmd.bar((t: 660, o: 1, h: 2, l: 0.5, c: 9, v: 3)));
    expect(calls, isEmpty);
    await tester.pump(const Duration(milliseconds: 70));
    expect(calls, hasLength(1));
    final batch = calls.single.map(dec).toList();
    expect(batch.map((m) => m['type']), ['quote', 'bar', 'lines', 'bar']);
    expect(batch[0]['bid'], 10.0);
    expect((batch[1]['bar'] as List)[4], 10.0);
    expect(((batch[2]['lines'] as List).single as Map)['price'], 10.0);
    expect((batch[3]['bar'] as List).first, 660);
    // history, indicators and the chart type go out at once
    c.send(ChartCmd.chartType('line'));
    await tester.pump();
    expect(calls, hasLength(2));
    expect(dec(calls.last.single)['type'], 'chartType');
    c.dispose();
  });

  testWidgets('while a chip is dragged, trade-line updates wait and the latest goes out when it is dropped', (tester) async {
    final c = ChartSurfaceController();
    final calls = <List<String>>[];
    c.attach(calls.add);
    c.holdLines = true;
    c.send(ChartCmd.lines([const ChartLine(id: 'sl:1', kind: 'sl', price: 1, label: 'SL')]));
    c.send(ChartCmd.quote(1, 2));
    // while dragging, live updates go out less often (the finger's moves come first)
    await tester.pump(const Duration(milliseconds: 70));
    expect(calls, isEmpty);
    await tester.pump(const Duration(milliseconds: 120));
    expect(calls.expand((b) => b).map((s) => dec(s)['type']), ['quote']);
    c.send(ChartCmd.lines([const ChartLine(id: 'sl:1', kind: 'sl', price: 2, label: 'SL')]));
    c.holdLines = false;
    await tester.pump();
    final lines = calls.expand((b) => b).map(dec).where((m) => m['type'] == 'lines').toList();
    expect(lines, hasLength(1));
    expect(((lines.single['lines'] as List).single as Map)['price'], 2.0);
    c.dispose();
  });

  test('chart time is broker server time (GMT+3 in US summer, GMT+2 in winter) and converts back', () {
    final summer = DateTime.utc(2026, 7, 1, 12).millisecondsSinceEpoch ~/ 1000;
    final winter = DateTime.utc(2026, 1, 15, 12).millisecondsSinceEpoch ~/ 1000;
    expect(toChartTime(summer) - summer, 3 * 3600);
    expect(toChartTime(winter) - winter, 2 * 3600);
    expect(fromChartTime(toChartTime(summer)), summer);
    expect(fromChartTime(toChartTime(winter)), winter);
  });
}

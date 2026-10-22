// What the chart page (assets/chart/chart.html) draws for the app's commands, run in Node with a stand-in for
// lightweight-charts (chart_page_harness.mjs). Skipped where Node is not installed.
import 'dart:convert';
import 'dart:io';

import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/chart/chart_bridge.dart';

const _palette = ChartPalette(
  dark: true,
  bg: '#0a0a0d',
  grid: '#111111',
  line: '#222222',
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

String? _node() {
  for (final p in ['/usr/local/bin/node', '/opt/homebrew/bin/node', '${Platform.environment['HOME']}/.local/node/bin/node']) {
    if (File(p).existsSync()) return p;
  }
  try {
    final r = Process.runSync('which', ['node']);
    final p = '${r.stdout}'.trim();
    return r.exitCode == 0 && p.isNotEmpty ? p : null;
  } catch (_) {
    return null;
  }
}

ChartBar b(int t, double o, double h, double l, double c) => (t: t, o: o, h: h, l: l, c: c, v: 1);

/// Runs the page with `steps` (a command JSON string, a list of them = one batch, or an int = that many animation
/// frames 16 ms apart) and returns the drawn candles: time → [open, high, low, close].
Map<int, List<double>> draw(String node, List<Object> steps) {
  final dir = Directory.systemTemp.createTempSync('chart_page');
  try {
    final f = File('${dir.path}/steps.json')
      ..writeAsStringSync(
        jsonEncode([
          for (final s in steps)
            if (s is int)
              {'frames': s, 'dt': 16}
            else if (s is List)
              {'batch': (s).map((x) => jsonDecode(x as String)).toList()}
            else
              {'recv': jsonDecode(s as String)},
        ]),
      );
    final r = Process.runSync(node, ['test/terminal/chart_page_harness.mjs', f.path]);
    if (r.exitCode != 0) fail('chart page: ${r.stderr}');
    final m = (jsonDecode('${r.stdout}') as Map).cast<String, dynamic>();
    return {
      for (final e in m.entries) int.parse(e.key): [for (final x in e.value as List) (x as num).toDouble()],
    };
  } finally {
    dir.deleteSync(recursive: true);
  }
}

void main() {
  final node = _node();
  final skip = node == null ? 'Node is not installed' : null;
  final init = ChartCmd.init(digits: 2, intraday: true, palette: _palette, tf: 'M1');
  final history = [for (var t = 0; t < 600; t += 60) b(t, 10, 10.5, 9.5, 10.2)];

  group('the chart page', () {
    test('a bar that closes while its close is still gliding is drawn at its real close', () {
      final drawn = draw(node!, [
        init,
        ChartCmd.bars(history),
        ChartCmd.quote(10.2, 10.3),
        3,
        // the last tick of the minute, one frame of the glide, then the next minute's first tick
        ChartCmd.bar(b(540, 10, 10.9, 9.5, 10.9)),
        1,
        ChartCmd.bar(b(600, 10.85, 10.85, 10.85, 10.85)),
        30,
      ]);
      expect(drawn[540], [10, 10.9, 9.5, 10.9]);
      expect(drawn[600], [10.85, 10.85, 10.85, 10.85]);
    }, skip: skip);

    test('the last update of a bar and the first of the next in one frame', () {
      final drawn = draw(node!, [
        init,
        ChartCmd.bars(history),
        ChartCmd.quote(10.2, 10.3),
        3,
        [ChartCmd.bar(b(540, 10, 10.6, 9.4, 9.45)), ChartCmd.bar(b(600, 9.5, 9.5, 9.5, 9.5))],
        30,
      ]);
      expect(drawn[540], [10, 10.6, 9.4, 9.45]);
      expect(drawn[600], [9.5, 9.5, 9.5, 9.5]);
    }, skip: skip);

    test('corrected history (a closed bar\'s stored values, bars missed while offline) replaces what is drawn', () {
      final drawn = draw(node!, [
        init,
        ChartCmd.bars(history),
        ChartCmd.quote(10.2, 10.3),
        ChartCmd.bar(b(600, 10.2, 10.2, 10.2, 10.2)),
        3,
        ChartCmd.redraw([...history.take(9), b(540, 10, 10.7, 9.5, 10.25), b(600, 10.2, 10.3, 10.1, 10.3), b(660, 10.3, 10.3, 10.3, 10.3)]),
        30,
      ]);
      expect(drawn.keys, [for (var t = 0; t <= 660; t += 60) t]);
      expect(drawn[540], [10, 10.7, 9.5, 10.25]);
      expect(drawn[660], [10.3, 10.3, 10.3, 10.3]);
    }, skip: skip);
  });
}

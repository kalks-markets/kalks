// Chart time (broker server time, GMT+3 in US summer time, GMT+2 otherwise) for the candles of every timeframe:
// strictly increasing (lightweight-charts refuses a series whose times go back or repeat, and draws nothing) and
// back to UTC exactly (scroll-back pages ask the server with it), also across both US DST switches.
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/chart/terminal_chart.dart';

int utc(String iso) => DateTime.parse(iso).millisecondsSinceEpoch ~/ 1000;

void main() {
  test('server time outside the switches', () {
    expect(toChartTime(utc('2026-07-01T12:00:00Z')), utc('2026-07-01T15:00:00Z'));
    expect(toChartTime(utc('2026-01-15T12:00:00Z')), utc('2026-01-15T14:00:00Z'));
    // the autumn switch day, before and after its hour
    expect(toChartTime(utc('2026-11-01T04:59:00Z')), utc('2026-11-01T07:59:00Z'));
    expect(toChartTime(utc('2026-11-01T07:00:00Z')), utc('2026-11-01T09:00:00Z'));
    // a daily bar opens at New York close: server midnight
    expect(toChartTime(utc('2026-10-31T21:00:00Z')), utc('2026-11-01T00:00:00Z'));
    expect(toChartTime(utc('2026-11-01T22:00:00Z')), utc('2026-11-02T00:00:00Z'));
  });

  for (final (name, from, to) in [
    ('the autumn switch (clocks go back an hour, crypto trades through it)', '2026-10-31T18:00:00Z', '2026-11-01T18:00:00Z'),
    ('the spring switch', '2026-03-07T18:00:00Z', '2026-03-08T18:00:00Z'),
  ]) {
    test('M1 / M5 / M15 / H1 bars across $name: in order, and back to UTC', () {
      for (final step in [60, 300, 900, 3600]) {
        var prev = -1;
        for (var t = utc(from); t < utc(to); t += step) {
          final c = toChartTime(t);
          expect(c, greaterThan(prev), reason: 'bar ${DateTime.fromMillisecondsSinceEpoch(t * 1000, isUtc: true)} (step $step)');
          expect(fromChartTime(c), t);
          prev = c;
        }
      }
    });
  }

  test('the 4-hour bars of the autumn switch day (market-data: 05:00 UTC holds the repeated hour)', () {
    final h4 = ['2026-11-01T01:00:00Z', '2026-11-01T05:00:00Z', '2026-11-01T10:00:00Z', '2026-11-01T14:00:00Z'].map(utc).toList();
    final chart = h4.map(toChartTime).toList();
    expect(chart[0], utc('2026-11-01T04:00:00Z'));
    expect(chart[1], utc('2026-11-01T08:00:00Z'));
    expect(chart[2], utc('2026-11-01T12:00:00Z'));
    for (var i = 1; i < chart.length; i++) {
      expect(chart[i], greaterThan(chart[i - 1]));
    }
  });
}

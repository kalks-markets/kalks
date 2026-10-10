// Portfolio module (agent C1): unit tests for the ranges, periods, statement queries, option premiums and labels,
// and widget tests of the five pages and the shared panels on the sample-data API (lib/preview/c1/
// preview_portfolio.dart, answered first through a Dio interceptor that also records every request).
import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/files.dart';
import 'package:kalks/core/models/account.dart';
import 'package:kalks/core/models/trading.dart';
import 'package:kalks/data/client_data.dart';
import 'package:kalks/features/portfolio/analytics_model.dart';
import 'package:kalks/features/portfolio/analytics_screen.dart';
import 'package:kalks/features/portfolio/ledger_screen.dart';
import 'package:kalks/features/portfolio/portfolio_data.dart';
import 'package:kalks/features/portfolio/portfolio_logic.dart';
import 'package:kalks/features/portfolio/portfolio_screen.dart';
import 'package:kalks/features/portfolio/statements_screen.dart';
import 'package:kalks/features/portfolio/trade_history_screen.dart';
import 'package:kalks/features/portfolio/widgets/activity_panels.dart';
import 'package:kalks/features/portfolio/widgets/analytics_panel.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/preview/c1/preview_portfolio.dart';
import 'package:kalks/preview/preview_data.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';

import 'helpers/test_app.dart';

/* ------------------------------------------------------------------ helpers */

/// Answers this module's paths from its preview file and records every request.
class _Net {
  final List<RequestOptions> requests = [];

  static String pathOf(RequestOptions o) => o.uri.path.replaceFirst(RegExp(r'^.*/api/mobile/'), '').replaceFirst(RegExp('^/'), '');

  List<RequestOptions> to(String path) => [
    for (final r in requests)
      if (pathOf(r) == path) r,
  ];

  Map<String, String> lastQuery(String path) => to(path).last.uri.queryParameters;
}

_Net _route(ProviderContainer c) {
  final net = _Net();
  c
      .read(apiProvider)
      .dio
      .interceptors
      .add(
        InterceptorsWrapper(
          onRequest: (o, h) {
            net.requests.add(o);
            final body = o.data is Map ? (o.data as Map).cast<String, dynamic>() : const <String, dynamic>{};
            final r = previewPortfolio(o.method, _Net.pathOf(o), body, o.uri.queryParameters);
            if (r == null) return h.next(o);
            final text = jsonEncode(r.$2);
            h.resolve(
              Response<Object?>(requestOptions: o, statusCode: r.$1, data: o.responseType == ResponseType.bytes ? utf8.encode(text) : jsonDecode(text)),
            );
          },
        ),
      );
  return net;
}

final List<({DownloadedFile file, String name})> _shared = [];

Future<ProviderContainer> _open(WidgetTester tester, String location, {String locale = 'en'}) async {
  final c = await pumpApp(tester, signedIn: true, locale: locale);
  c.read(routerProvider).go(location);
  return c;
}

/// Scrolls through the page and checks that `texts` appear in reading order: top to bottom, and left to right
/// within a row (the web's two-column KPI grids on phones).
Future<void> _expectOrder(WidgetTester tester, Type screen, List<String> texts) async {
  final page = find.descendant(of: find.byType(screen), matching: find.byType(Scrollable)).first;
  var lastY = double.negativeInfinity, lastX = double.negativeInfinity;
  for (final s in texts) {
    final f = find.text(s, findRichText: true).first;
    await tester.scrollUntilVisible(f, 250, scrollable: page);
    await tester.pump(const Duration(milliseconds: 50));
    final tl = tester.getTopLeft(f);
    final y = tl.dy + tester.state<ScrollableState>(page).position.pixels;
    if ((y - lastY).abs() < 1) {
      expect(tl.dx, greaterThan(lastX), reason: '"$s" should come after the previous tile in its row');
    } else {
      expect(y, greaterThan(lastY), reason: '"$s" should come after the previous section');
    }
    lastY = y;
    lastX = tl.dx;
  }
  await tester.drag(page, const Offset(0, 6000));
  await settle(tester, frames: 4);
}

/// Scrolls until `f` is built, then to the middle of the page: scrollUntilVisible stops with `f` at the top edge,
/// under the frosted top bar, where a tap would land on the bar.
Future<void> _scrollTo(WidgetTester tester, Type screen, Finder f) async {
  final page = find.descendant(of: find.byType(screen), matching: find.byType(Scrollable)).first;
  await tester.scrollUntilVisible(f, 250, scrollable: page);
  // stop the drag's fling, then put the target a third of the way down the screen (clear of the frosted top bar,
  // the floating chat button and the tab bar)
  final pos = tester.state<ScrollableState>(page).position;
  pos.jumpTo(pos.pixels);
  await tester.pump();
  final top = tester.getRect(f.first).top;
  pos.jumpTo((pos.pixels + top - 300).clamp(0, pos.maxScrollExtent).toDouble());
  await settle(tester, frames: 3);
}

/// Like `settle`, without precaching the images on screen: the share card's preview is a network image, which
/// flutter_test answers with HTTP 400 (the page shows its error placeholder; a precache would report the error).
Future<void> _settleNoImages(WidgetTester tester, {int frames = 12}) async {
  for (var i = 0; i < frames; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 20)));
    await tester.pump(const Duration(milliseconds: 100));
  }
  await tester.pump(const Duration(milliseconds: 400));
}

/// Scrolls `f` (already built) to the middle of its page, clear of the frosted top bar.
Future<void> _reveal(WidgetTester tester, Finder f) async {
  await Scrollable.ensureVisible(tester.element(f.first), alignment: 0.5);
  await settle(tester, frames: 4);
}

Finder _button(String label) => find.widgetWithText(KButton, label);
Finder _iconButton(String label) => find.byWidgetPredicate((w) => w is KIconButton && w.semanticLabel == label);

T _en() {
  final en = (jsonDecode(File('assets/i18n/en.json').readAsStringSync()) as Map).cast<String, Object?>();
  return T('en', en, en);
}

EngineAccount _acc(int login) =>
    EngineAccount.fromJson((previewAccounts['accounts'] as List).cast<Map<String, dynamic>>().firstWhere((a) => a['login'] == login));

void main() {
  setUpAll(() async => initializeDateFormatting());
  setUp(() {
    _shared.clear();
    portfolioShareFile = (file, {required fallbackName, subject}) async {
      _shared.add((file: file, name: fallbackName));
      return true;
    };
  });
  tearDown(() => portfolioShareFile = shareFile);

  /* ---------------------------------------------------------------- units */

  group('ranges', () {
    final now = DateTime(2026, 10, 8, 15);

    test('presets send only `from`, today counted (web rangeQuery)', () {
      expect(rangeQuery(const ActivityRange(RangePreset.d7), now: now), (from: '2026-10-02', to: null));
      expect(rangeQuery(const ActivityRange(RangePreset.d30), now: now), (from: '2026-09-09', to: null));
      expect(rangeQuery(const ActivityRange(RangePreset.d90), now: now), (from: '2026-07-11', to: null));
      expect(rangeQuery(const ActivityRange(RangePreset.all), now: now), (from: null, to: null));
    });

    test('custom ranges: inclusive `to` becomes the next day; incomplete or reversed ranges are invalid', () {
      const r = ActivityRange(RangePreset.custom, from: '2026-09-01', to: '2026-09-30');
      expect(rangeQuery(r), (from: '2026-09-01', to: '2026-10-01'));
      expect(r.invalid, isFalse);
      expect(const ActivityRange(RangePreset.custom, from: '2026-09-30', to: '2026-09-01').invalid, isTrue);
      expect(const ActivityRange(RangePreset.custom, from: '2026-09-30').invalid, isTrue);
      expect(nextDay('2026-12-31'), '2027-01-01');
      final custom = selectPreset(RangePreset.custom, const ActivityRange(RangePreset.d30), now: now);
      expect((custom.from, custom.to), ('2026-09-09', '2026-10-08'));
      expect(selectPreset(RangePreset.d7, custom).from, isNull);
    });

    test('history / ledger / export queries', () {
      expect(historyQuery(const ActivityRange(RangePreset.all), 2, 'all'), {'from': null, 'to': null, 'page': 2, 'limit': 25});
      expect(historyQuery(const ActivityRange(RangePreset.d7), 1, 'option', now: now)['instrument'], 'option');
      expect(ledgerQuery(const ActivityRange(RangePreset.d7), 1, now: now), {'from': '2026-10-02', 'to': null, 'page': 1, 'limit': 25});
      expect(exportQuery('history', const ActivityRange(RangePreset.all), instrument: 'cfd'), {
        'kind': 'history',
        'from': null,
        'to': null,
        'instrument': 'cfd',
      });
      expect(exportQuery('ledger', const ActivityRange(RangePreset.all), instrument: 'cfd').containsKey('instrument'), isFalse);
      expect(pageCount(51, 25), 3);
      expect(pageCount(0, 25), 1);
    });

    test('the net result takes commission off whatever its sign', () {
      const p = HistoryPage(deals: [], orders: [], page: 1, limit: 25, total: 0, profit: 100, swap: -5, commission: 7);
      const q = HistoryPage(deals: [], orders: [], page: 1, limit: 25, total: 0, profit: 100, swap: -5, commission: -7);
      expect(historyNet(p), 88);
      expect(historyNet(q), 88);
    });

    test('the account a picker page opens with', () {
      final all = [
        for (final l in [20017734, 10042817, 10051123]) _acc(l),
      ];
      expect(pickAccount(all, 10051123)!.login, 10051123);
      expect(pickAccount(all, null)!.login, 10042817);
      expect(pickAccount(const [], 1), isNull);
      expect(wantedLogin({'account': '10051123'}), 10051123);
      expect(wantedLogin({'login': '20017734'}), 20017734);
    });
  });

  group('analytics and statements', () {
    final t = _en();
    final now = DateTime(2026, 10, 8);

    test('periods: from inclusive, to = tomorrow (web periodRange) and their labels', () {
      expect(periodRange(AnPeriod.d90, now: now), (from: '2026-07-11', to: '2026-10-09'));
      expect(periodRange(AnPeriod.d7, now: now), (from: '2026-10-02', to: '2026-10-09'));
      expect(periodLabel(t, AnPeriod.d30), 'Last 30 days');
      expect(periodLabel(t, AnPeriod.y1), 'Last 12 months');
      expect(periodLabel(t, AnPeriod.all), 'All time');
    });

    test('holding times (web fmtHold)', () {
      expect(fmtHold(t, 0), '—');
      expect(fmtHold(t, 29), '29s');
      expect(fmtHold(t, 30), '1m'); // JS Math.round(0.5) == 1, like Dart's round
      expect(fmtHold(t, 45 * 60), '45m');
      expect(fmtHold(t, 3 * 3600 + 12 * 60), '3h 12m');
      expect(fmtHold(t, 2 * 86400 + 4 * 3600), '2d 4h');
    });

    test('statement periods (web stRange)', () {
      ({String from, String to, String label})? r(StPeriod p, {String day = '', String month = '', String year = '2026', String from = '', String to = ''}) =>
          stRange(t, p, day: day, month: month, year: year, from: from, to: to);
      expect(r(StPeriod.day, day: '2026-09-24')!.to, '2026-09-25');
      expect(r(StPeriod.month, month: '2026-12'), (from: '2026-12-01', to: '2027-01-01', label: 'December 2026'));
      expect(r(StPeriod.year), (from: '2026-01-01', to: '2027-01-01', label: 'Year 2026'));
      expect(r(StPeriod.custom, from: '2026-09-01', to: '2026-09-30')!.to, '2026-10-01');
      expect(r(StPeriod.custom, from: '2026-09-30', to: '2026-09-01'), isNull);
      expect(r(StPeriod.day), isNull);
      expect(r(StPeriod.month, month: '2026'), isNull);
    });

    test('statement path and query (web stUrl): sections left out are sent as 0', () {
      expect(statementPath(10042817), 'reports/accounts/10042817/statement');
      expect(statementQuery('2026-09-01', '2026-10-01', StFormat.pdf), {'from': '2026-09-01', 'to': '2026-10-01', 'format': 'pdf'});
      expect(statementQuery('a', 'b', StFormat.xlsx, open: false, deals: false), {'from': 'a', 'to': 'b', 'format': 'xlsx', 'open': '0', 'deals': '0'});
      expect(statementYears(DateTime(2024, 5), now: now), ['2026', '2025', '2024']);
    });

    test('the analytics answer parses, and the preview keeps the dashboard curve', () {
      final j = previewPortfolioAnalytics({'login': 'all', 'from': '2026-09-25', 'to': '2026-10-09'});
      for (final p in (j['curve'] as Map)['points'] as List) {
        expect((p as Map).keys, containsAll(['day', 'balance', 'equity', 'flow']));
      }
      final a = Analytics.fromJson(jsonDecode(jsonEncode(j)) as Map<String, dynamic>);
      expect(a.stats.trades, 42);
      expect(a.stats.profitFactor, 2.03);
      expect(a.hourHeatmap.length, 7);
      expect(a.hourHeatmap.first.length, 24);
      expect(a.bySession.map((s) => s.session), ['Asia', 'London', 'New York']);
      expect(a.behaviour.insights.length, 3);
      expect(a.empty, isFalse);
      expect(Analytics.fromJson(const {}).empty, isTrue);
    });
  });

  group('options in trade lists', () {
    final t = _en();

    test('series codes become readable terms and labels', () {
      final o = optionTerms('EURUSD-20261002-1.1000-C', null)!;
      expect((o.underlying, o.right, o.strikeLabel, o.expiry, o.quoteCurrency), ('EURUSD', 'call', '1.1000', '2026-10-02', 'USD'));
      expect(optionLabel(t, o), startsWith('EURUSD 1.1000 Call · '));
      expect(symbolLabel(t, 'XAUUSD'), 'XAUUSD');
      expect(symbolLabel(t, 'USDJPY-20261002-150.5-P'), startsWith('USDJPY 150.5 Put'));
      expect(optionTerms('X', {'underlying': 'gbpusd', 'right': 'p', 'strike': 1.25, 'expiry': '2026-11-20T00:00:00Z'})!.strikeLabel, '1.25');
      expect(reasonLabel(t, 'knock_out'), 'Knocked out');
      expect(reasonLabel(t, 'something_new'), 'something new');
    });

    test('deal premiums per contract (web dealPremiumsUsd)', () {
      final deals = previewDeals(10042817).where((d) => d['instrument'] == 'option').map(EngineDeal.fromJson).toList();
      final exit = deals.firstWhere((d) => d.entry == 'out'), entry = deals.firstWhere((d) => d.entry == 'in');
      // exit: 0.0042 x 10,000 x 1 = $42; opened at |24 - 84| / 2 = $30
      final e = dealPremiumsUsd(exit, 1);
      expect(e.own, closeTo(42, 1e-9));
      expect(e.open, closeTo(30, 1e-9));
      final en = dealPremiumsUsd(entry, 1);
      expect(en.own, closeTo(30, 1e-9));
      expect(en.open, isNull);
      // without the contract size: |cash| / contracts, in USD on a cent account
      final cent = EngineDeal.fromJson({
        'id': 1,
        'entry': 'out',
        'volume': 2,
        'price': 0.0042,
        'profit': 2400,
        'option': {'cash': 8400},
      });
      expect(dealPremiumsUsd(cent, 100), (own: 42.0, open: 30.0));
      final pos = EnginePosition.fromJson({'volume': 2, 'premium': -60, 'markValue': 82});
      expect(positionPremiumsUsd(pos, 1), (open: 30.0, now: 41.0));
      expect(fmtContracts(2), '2');
      expect(fmtContracts(-2.5), '2.5');
      expect(fmtContracts(10), '10');
    });

    test('server times carry the year and Latin digits', () {
      final s = serverTimeLabel(DateTime.utc(2026, 9, 24, 11, 3), 'ar');
      expect(s, contains('2026'));
      expect(s, contains('14:03'));
      expect(serverTimeLabel(null, 'en'), '—');
    });

    test('the preview history pages, filters and totals like the BFF', () {
      final all = previewPortfolio('GET', 'trading/accounts/10042817/history', const {}, const {'page': '1', 'limit': '25'})!.$2 as Map;
      expect(all['total'], 34);
      expect((all['deals'] as List).length, 25);
      final p2 = previewPortfolio('GET', 'trading/accounts/10042817/history', const {}, const {'page': '2', 'limit': '25'})!.$2 as Map;
      expect((p2['deals'] as List).length, 9);
      final opt = previewPortfolio('GET', 'trading/accounts/10042817/history', const {}, const {'instrument': 'option'})!.$2 as Map;
      expect(opt['total'], 2);
      final page = HistoryPage.fromJson((jsonDecode(jsonEncode(opt)) as Map).cast<String, dynamic>());
      expect(page.deals.every((d) => d.isOption), isTrue);
      expect(page.profit, 24);
      expect(historyNet(page), closeTo(22.8, 1e-9));
    });
  });

  /* ---------------------------------------------------------------- pages */

  group('Portfolio overview', () {
    testWidgets('KPIs, allocation, accounts, open positions and links in the phone order', (tester) async {
      final c = await _open(tester, '/portfolio');
      final net = _route(c);
      await settle(tester);
      expect(find.byType(PortfolioScreen), findsOneWidget);
      // live equity: 12,893.40 + 251,980.20 USC / 100
      expect(find.text(r'$15,413.20', findRichText: true), findsOneWidget);
      await _expectOrder(tester, PortfolioScreen, [
        'Live equity',
        'Live balance',
        'Floating P&L',
        'Equity allocation',
        'Equity and margin per account',
        'Across all accounts, refreshed every 10 seconds',
        'Every deal, per account',
        'Balance movements, per account',
        'Monthly and custom CSV exports',
      ]);
      // one detail call per account with positions (web useOpenPositions)
      expect({
        for (final r in net.requests) _Net.pathOf(r),
      }, containsAll(['trading/accounts/10042817', 'trading/accounts/10051123', 'trading/accounts/20017734']));
      // the page is a lazy sliver list: bring the open positions back into view before reading them
      await _scrollTo(tester, PortfolioScreen, find.textContaining('EURUSD 1.1000 Call'));
      expect(find.textContaining('EURUSD 1.1000 Call'), findsOneWidget);
      expect(find.textContaining(r'$30.00 → $41.00 per contract'), findsOneWidget);
      expect(find.text('Trader'), findsNWidgets(6));
      await unmount(tester);
    });

    testWidgets('Statements opens the statements page', (tester) async {
      final c = await _open(tester, '/portfolio');
      _route(c);
      await settle(tester);
      // on the section's photo since 2026-10-10
      await tester.tap(find.widgetWithText(KHeroButton, 'Statements').first);
      await settle(tester);
      expect(find.byType(StatementsScreen), findsOneWidget);
      await unmount(tester);
    });
  });

  group('Analytics', () {
    testWidgets('sections in the phone order; period and account refetch; Export shares a statement', (tester) async {
      final c = await _open(tester, '/portfolio/analytics');
      final net = _route(c);
      await settle(tester);
      expect(find.byType(AnalyticsScreen), findsOneWidget);
      expect(net.lastQuery('reports/analytics'), {'login': 'all', ...periodRange(AnPeriod.d90).toMap()});
      await _expectOrder(tester, AnalyticsScreen, [
        'Win rate',
        'Profit factor',
        'Avg R:R',
        'Max drawdown',
        'Avg hold time',
        'Equity vs balance',
        'Trade statistics',
        'Performance by symbol',
        'By weekday',
        'Long vs short',
        'By hour of day',
        'By session',
        'Money flow',
        'Commission, swap and fees paid',
        'Behaviour insights',
      ]);
      expect(find.text('London is your best session'), findsOneWidget);

      await tester.tap(find.text('30D'));
      await settle(tester);
      expect(net.lastQuery('reports/analytics')['from'], periodRange(AnPeriod.d30).from);

      // one account: the menu, then Export (PDF / XLSX / CSV statement of the period)
      await tester.tap(_button('All live accounts'));
      await settle(tester);
      await tester.tap(find.text('#10042817').last);
      await settle(tester);
      expect(net.lastQuery('reports/analytics')['login'], '10042817');
      await tester.tap(_button('Export'));
      await settle(tester);
      await tester.tap(find.text('Statement (XLSX)'));
      await settle(tester);
      final q = net.lastQuery('reports/accounts/10042817/statement');
      expect(q, {'from': periodRange(AnPeriod.d30).from, 'to': periodRange(AnPeriod.d30).to, 'format': 'xlsx'});
      expect(_shared.single.name, 'statement-10042817.xlsx');
      await unmount(tester);
    });

    testWidgets('the account Analytics tab (AccountAnalyticsPanel) and its PDF statement', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      final net = _route(c);
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: c,
          child: MaterialApp(
            theme: KTheme.client(Brightness.light),
            home: I18nScope(
              t: c.read(tProvider),
              child: Scaffold(
                body: ListView(
                  padding: const EdgeInsets.all(16),
                  children: [AccountAnalyticsPanel(account: _acc(10042817))],
                ),
              ),
            ),
          ),
        ),
      );
      await settle(tester);
      expect(net.lastQuery('reports/analytics')['login'], '10042817');
      expect(find.text('Win rate'), findsOneWidget);
      expect(find.text('Equity vs balance'), findsOneWidget);
      await tester.tap(_button('PDF statement'));
      await settle(tester);
      expect(net.lastQuery('reports/accounts/10042817/statement')['format'], 'pdf');
      expect(_shared.single.name, 'statement-10042817.pdf');
      await unmount(tester);
    });
  });

  group('Trade history', () {
    testWidgets('picker, filters, pager and CSV export', (tester) async {
      final c = await _open(tester, '/portfolio/history');
      final net = _route(c);
      await settle(tester);
      expect(find.byType(TradeHistoryScreen), findsOneWidget);
      expect(find.text('Trade history · #10042817'), findsOneWidget);
      expect(net.lastQuery('trading/accounts/10042817/history'), {'from': rangeQuery(const ActivityRange(RangePreset.d30)).from, 'page': '1', 'limit': '25'});
      expect(find.byType(DealRow), findsWidgets);

      // instrument: Options
      await tester.tap(find.text('Options'));
      await settle(tester);
      expect(net.lastQuery('trading/accounts/10042817/history')['instrument'], 'option');
      expect(find.byType(DealRow), findsNWidgets(2));
      expect(find.textContaining(r'$42.00 per contract'), findsOneWidget);
      expect(find.text(r'Opened at $30.00'), findsOneWidget);
      expect(find.text('Option prices are premiums in USD per contract.'), findsOneWidget);

      // back to all instruments, 90 days: 34 deals over two pages
      await tester.tap(find.descendant(of: find.byType(InstrumentPicker), matching: find.text('All')));
      await settle(tester);
      await tester.tap(find.text('90D'));
      await settle(tester);
      expect(net.lastQuery('trading/accounts/10042817/history'), {'from': rangeQuery(const ActivityRange(RangePreset.d90)).from, 'page': '1', 'limit': '25'});
      await _scrollTo(tester, TradeHistoryScreen, _iconButton('Next page'));
      expect(find.text('1–25 of 34'), findsOneWidget);
      await tester.tap(_iconButton('Next page'));
      await settle(tester);
      expect(net.lastQuery('trading/accounts/10042817/history')['page'], '2');
      expect(find.byType(DealRow), findsNWidgets(9));

      // CSV: the export with the same range, through the share sheet (scrolled to, not flung: an overscroll past
      // the whole lazy list would rebuild the panel with its default filters)
      await _reveal(tester, _button('CSV'));
      await tester.tap(_button('CSV'));
      await settle(tester);
      expect(net.lastQuery('trading/accounts/10042817/export'), {'kind': 'history', 'from': rangeQuery(const ActivityRange(RangePreset.d90)).from});
      expect(_shared.single.name, '10042817-history.csv');
      expect(find.text('Statement export started'), findsWidgets);

      // another account
      await tester.tap(find.text('#10051123'));
      await settle(tester);
      expect(find.text('Trade history · #10051123'), findsOneWidget);
      expect(net.to('trading/accounts/10051123/history'), isNotEmpty);
      expect(find.textContaining('USC '), findsWidgets);
      await unmount(tester);
    });

    testWidgets('?account= opens that account; the share card sheet creates a card', (tester) async {
      final c = await _open(tester, '/portfolio/history?account=20017734');
      final net = _route(c);
      await settle(tester);
      expect(find.text('Trade history · #20017734'), findsOneWidget);
      await tester.tap(_button('Share period P&L'));
      await settle(tester);
      expect(find.text('Show amounts'), findsOneWidget);
      await tester.tap(_button('Create share card'));
      await _settleNoImages(tester);
      final body = net.to('growth/shares').single.data as Map;
      expect(body['kind'], 'period');
      expect(body['login'], 20017734);
      expect(body['showAmounts'], false);
      expect(find.text('Download PNG'), findsOneWidget);
      expect(find.textContaining('app.kalkstrade.com/s/P20017734'), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('a custom range asks for both days and sends `to` as the next day', (tester) async {
      final c = await _open(tester, '/portfolio/history');
      final net = _route(c);
      await settle(tester);
      await tester.tap(find.text('Custom'));
      await settle(tester);
      final r = selectPreset(RangePreset.custom, const ActivityRange(RangePreset.d30));
      expect(find.text(r.from!), findsOneWidget);
      expect(find.text(r.to!), findsOneWidget);
      expect(net.lastQuery('trading/accounts/10042817/history'), {'from': r.from, 'to': nextDay(r.to!), 'page': '1', 'limit': '25'});
      await unmount(tester);
    });
  });

  group('Ledger', () {
    testWidgets('entries, pager and CSV export', (tester) async {
      final c = await _open(tester, '/portfolio/ledger');
      final net = _route(c);
      await settle(tester);
      expect(find.byType(LedgerScreen), findsOneWidget);
      expect(find.text('Balance ledger · #10042817'), findsOneWidget);
      expect(net.lastQuery('trading/accounts/10042817/ledger'), {'page': '1', 'limit': '25'});
      expect(find.byType(LedgerRow), findsNWidgets(25));
      expect(find.text('Deposit from wallet'), findsWidgets);
      await tester.tap(_button('CSV'));
      await settle(tester);
      expect(net.lastQuery('trading/accounts/10042817/export'), {'kind': 'ledger'});
      expect(_shared.single.name, '10042817-ledger.csv');
      await tester.tap(find.text('7D'));
      await settle(tester);
      expect(net.lastQuery('trading/accounts/10042817/ledger')['from'], rangeQuery(const ActivityRange(RangePreset.d7)).from);
      await _scrollTo(tester, LedgerScreen, find.text('Trade result').first);
      await unmount(tester);
    });

    testWidgets('a cent account shows USC amounts', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      _route(c);
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: c,
          child: MaterialApp(
            theme: KTheme.client(Brightness.dark),
            home: I18nScope(
              t: c.read(tProvider),
              child: Scaffold(
                body: ListView(
                  padding: const EdgeInsets.all(16),
                  children: [LedgerPanel(account: _acc(10051123))],
                ),
              ),
            ),
          ),
        ),
      );
      await settle(tester);
      expect(find.text('Balance ledger'), findsOneWidget);
      expect(find.textContaining('USC 254,300.00', findRichText: true), findsOneWidget);
      expect(find.text('+USC 250,000.00'), findsOneWidget);
      await unmount(tester);
    });
  });

  group('Statements', () {
    testWidgets('sections; year, format and sections build the statement query; monthly files', (tester) async {
      final c = await _open(tester, '/portfolio/statements');
      final net = _route(c);
      await settle(tester);
      expect(find.byType(StatementsScreen), findsOneWidget);
      await _expectOrder(tester, StatementsScreen, ['Generate a statement', 'Period', 'Format', "What's in your statement", 'Monthly statements']);
      expect(net.to('reports/accounts/10042817/months'), isNotEmpty);

      final year = '${DateTime.now().year}';
      await tester.tap(find.text('Year'));
      await settle(tester);
      expect(find.text(year), findsOneWidget);
      await tester.tap(find.text('Excel'));
      await settle(tester);
      final deals = find.ancestor(of: find.text('Every deal (entries and exits)'), matching: find.byType(Row)).first;
      // clear of the floating chat button at the bottom end
      await _scrollTo(tester, StatementsScreen, find.text('Every deal (entries and exits)'));
      await tester.tap(find.descendant(of: deals, matching: find.byType(KSwitch)));
      await settle(tester);
      expect(find.text('Year $year · Excel', findRichText: true), findsOneWidget);
      await _scrollTo(tester, StatementsScreen, _button('Download statement'));
      await tester.tap(_button('Download statement'));
      await settle(tester);
      expect(net.lastQuery('reports/accounts/10042817/statement'), {
        'from': '$year-01-01',
        'to': '${int.parse(year) + 1}-01-01',
        'format': 'xlsx',
        'deals': '0',
      });
      expect(_shared.single.name, 'statement-10042817-$year-01-01.xlsx');
      expect(find.text('Statement download started'), findsWidgets);

      // a monthly row's CSV: the whole month, every section
      final now = DateTime.now();
      final first = isoDay(DateTime(now.year, now.month));
      await _scrollTo(tester, StatementsScreen, _button('CSV').first);
      await tester.tap(_button('CSV').first);
      await settle(tester);
      expect(net.lastQuery('reports/accounts/10042817/statement'), {'from': first, 'to': isoDay(DateTime(now.year, now.month + 1)), 'format': 'csv'});
      expect(_shared.length, 2);
      await unmount(tester);
    });

    testWidgets('a failed download says so and shares nothing', (tester) async {
      final c = await _open(tester, '/portfolio/statements?account=99999999');
      _route(c);
      await settle(tester);
      // an unknown login falls back to the first live account
      expect(find.textContaining('#10042817 · Pro'), findsOneWidget);
      c
          .read(apiProvider)
          .dio
          .interceptors
          .insert(
            0,
            InterceptorsWrapper(
              onRequest: (o, h) => _Net.pathOf(o).endsWith('/statement')
                  ? h.resolve(
                      Response<Object?>(
                        requestOptions: o,
                        statusCode: 503,
                        data: utf8.encode(
                          jsonEncode({
                            'error': {'code': 'unavailable', 'message': 'Reports are down.'},
                          }),
                        ),
                      ),
                    )
                  : h.next(o),
            ),
          );
      await _scrollTo(tester, StatementsScreen, _button('Download statement'));
      await tester.tap(_button('Download statement'));
      await settle(tester);
      expect(_shared, isEmpty);
      expect(find.text("Couldn't download the statement"), findsWidgets);
      await unmount(tester);
    });
  });

  group('shared data', () {
    testWidgets('the dashboard equity curve still reads curve.points from the richer analytics answer', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      final sub = c.listen(equityCurveProvider(14).future, (_, _) {});
      final pts = await tester.runAsync(sub.read);
      sub.close();
      expect(pts, hasLength(14));
      expect(pts!.last.equity, greaterThan(0));
      await unmount(tester);
    });
  });
}

extension on ({String from, String to}) {
  Map<String, String> toMap() => {'from': from, 'to': to};
}

// Dashboard (agent C1): the web's rules for the statistics series, the getting-started steps, the market clock, the
// movers, the calendar rows and the news map; and Home itself on the sample API, in the web's phone order (since
// 2026-10-10: the balance strip, the shortcuts and the accounts under the photo).
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:intl/date_symbol_data_local.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/core/format/format.dart';
import 'package:kalks/core/models/account.dart';
import 'package:kalks/core/models/user.dart';
import 'package:kalks/core/models/wallet.dart';
import 'package:kalks/data/client_data.dart';
import 'package:kalks/features/dashboard/dashboard_data.dart';
import 'package:kalks/features/dashboard/dashboard_screen.dart';
import 'package:kalks/features/dashboard/widgets/markets_cards.dart';
import 'package:kalks/features/dashboard/widgets/more_cards.dart';
import 'package:kalks/features/dashboard/widgets/statistic_card.dart';
import 'package:kalks/features/markets/instruments.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/preview/preview_data.dart';

import 'helpers/test_app.dart';

T _t() => T('en', const {}, const {});

String _day(DateTime d) => '${d.year}-${d.month.toString().padLeft(2, '0')}-${d.day.toString().padLeft(2, '0')}';

List<CurvePoint> _curve(int days, {double start = 1000, double step = 10, double flowOn = -1}) {
  final now = DateTime.now();
  return [
    for (var i = days - 1; i >= 0; i--)
      (
        day: _day(now.subtract(Duration(days: i))),
        balance: start + (days - 1 - i) * step,
        equity: start + (days - 1 - i) * step,
        flow: (days - 1 - i) == flowOn ? 100 : 0,
      ),
  ];
}

void main() {
  setUpAll(initializeDateFormatting);

  group('statistics series (web toSeries)', () {
    test('equity: this period and the previous one', () {
      final s = toSeries(_curve(14), 7, StatMode.equity, StatRange.week);
      expect(s.points, hasLength(7));
      expect(s.points.first.v, 1070);
      expect(s.points.last.v, 1130);
      expect(s.compare, [1000, 1010, 1020, 1030, 1040, 1050, 1060]);
    });

    test('P&L is cumulative from the start of the period and nets deposits', () {
      // +10 a day, and a deposit of 100 on day 10 (equity jumps by 10 only, so that day is -90)
      final curve = [for (final p in _curve(14, flowOn: 10)) (day: p.day, balance: p.balance, equity: p.equity, flow: p.flow)];
      final s = toSeries(curve, 7, StatMode.pnl, StatRange.week);
      expect(s.points.first.v, 0);
      expect(s.points.last.v, 6 * 10 - 100);
    });

    test('the last year keeps one point a week and the last one', () {
      final s = toSeries(_curve(30), 30, StatMode.equity, StatRange.year);
      expect(s.points.length, 6); // 0, 7, 14, 21, 28 + the last (29)
      expect(s.points.last.v, 1000 + 29 * 10);
    });

    test('axis ticks and labels', () {
      expect(niceTicks(1003, 1130), [1000, 1050, 1100, 1150]);
      expect(niceTicks(5, 5).length, greaterThan(1));
      expect(compactMoney(950), r'$950');
      expect(compactMoney(1250), r'$1.3k');
      expect(compactMoney(-25300), r'-$25k');
      expect(compactMoney(2400000), r'$2.4M');
    });
  });

  group('getting started (web steps / walletStep / kycStep)', () {
    final me = SessionUser.fromJson(previewMe);
    final t = _t();
    final f = LocaleFormat('en');
    WalletOverview wallet(String available, {bool pending = false}) => WalletOverview.fromJson({
      'balances': [
        {'currency': 'USDT', 'available': available, 'locked': '0'},
      ],
      'pending_deposits': pending ? [<String, dynamic>{}] : [],
      'open_withdrawals': [],
    });

    test('five steps, in the web order, from the real record', () {
      final accounts = [for (final a in previewAccounts['accounts'] as List) EngineAccount.fromJson((a as Map).cast<String, dynamic>())];
      final steps = dashboardSteps(me, accounts, wallet('3250.40'), t, f);
      expect([for (final s in steps) s.key], ['account', 'email', 'kyc', 'account-open', 'wallet']);
      expect([for (final s in steps) s.state], ['done', 'done', 'done', 'done', 'done']);
      expect(steps[3].href, '/accounts');
      expect(steps[4].href, '/wallet/transfer');
    });

    test('no accounts, unverified, empty wallet: to-do steps with their links', () {
      final raw = {
        ...previewMe,
        'user': {...(previewMe['user'] as Map), 'kyc_status': 'unverified', 'email_verified': false},
      };
      final steps = dashboardSteps(SessionUser.fromJson(raw), const [], wallet('0'), t, f);
      expect(steps[1].state, 'todo');
      expect(steps[2].state, 'todo');
      expect(steps[2].href, '/profile/verification');
      expect(steps[3].href, '/accounts/new');
      expect(steps[4].state, 'todo');
      expect(steps[4].href, '/wallet/deposit');
      expect(walletStep(wallet('0', pending: true), t).state, 'review');
    });

    test('KYC case states', () {
      SessionUser user(String status, String? kase) => SessionUser.fromJson({
        ...previewMe,
        'user': {...(previewMe['user'] as Map), 'kyc_status': status, 'kyc_case_status': kase},
      });
      expect(kycStep(user('unverified', 'more_info'), t).state, 'todo');
      expect(kycStep(user('unverified', 'in_review'), t).state, 'review');
      expect(kycStep(user('rejected', null), t).state, 'rejected');
      expect(kycStep(user('pending', null), t).state, 'review');
      expect(kycStep(user('verified', 'more_info'), t).state, 'done');
    });
  });

  group('markets', () {
    test('market clock: open sessions and time left (server time)', () {
      final london = kSessions[2];
      expect(sessionState(london, 12).open, isTrue);
      expect(sessionState(london, 12).until, 7);
      expect(sessionState(london, 8).open, isFalse);
      expect(sessionState(london, 8).until, 2);
      // Sydney opens at 00:00: from 23:00 it is one hour away
      expect(sessionState(kSessions[0], 23).until, 1);
    });

    test('movers: 1D by today\'s change, 1W / 1M by the range change', () {
      final today = {for (final i in kInstruments) i.symbol: 0.0, 'SOLUSD': 4.6, 'TSLA': -3.2, 'XAUUSD': 0.8};
      expect(topMovers(today, gainers: true).first.symbol, 'SOLUSD');
      expect(topMovers(today, gainers: false).first.symbol, 'TSLA');
      expect(topMovers(today, gainers: true), hasLength(6));
      final range = {'EURUSD': 1.0, 'BTCUSD': 9.0, 'NFLX': -5.0};
      expect([for (final i in topMovers(today, gainers: true, rangeChanges: range)) i.symbol], ['BTCUSD', 'EURUSD', 'NFLX']);
      expect(topMovers(today, gainers: false, rangeChanges: range).first.symbol, 'NFLX');
    });

    test('calendar: events still ahead (or under two hours old), five at most', () {
      final now = DateTime.utc(2026, 10, 8, 12);
      DashEvent e(int id, Duration d, {bool allDay = false}) => DashEvent(
        id: id,
        title: 'E$id',
        currency: 'USD',
        country: 'us',
        startsAt: now.add(d),
        serverDate: '',
        serverTime: '',
        allDay: allDay,
        impact: 3,
        forecast: '',
        previous: '',
        actual: '',
        surprise: 0,
      );
      final list = upcomingEvents([
        e(1, const Duration(hours: -3)),
        e(2, const Duration(hours: -1)),
        e(3, const Duration(hours: 1), allDay: true),
        for (var i = 4; i < 12; i++) e(i, Duration(hours: i)),
      ], now);
      expect([for (final x in list) x.id], [2, 4, 5, 6, 7]);
    });

    test('news map: heat and the Equal Earth projection', () {
      final heat = heatOf(const [
        DashMapCountry(country: 'us', name: '', count: 22, sentiment: 0.32),
        DashMapCountry(country: 'gb', name: '', count: 1, sentiment: -0.9),
        DashMapCountry(country: 'au', name: '', count: 3, sentiment: 0),
      ]);
      expect(heat['us'], closeTo(0.32, 1e-9));
      expect(heat['gb'], closeTo(-0.3, 1e-9));
      expect(heat.containsKey('au'), isFalse);
      // (0, 0) sits at the map's translate point; east is right, north is up
      expect(equalEarth(0, 0), const Offset(480, 233.93069668213545));
      expect(equalEarth(10, 0).dx, greaterThan(480));
      expect(equalEarth(0, 50).dy, lessThan(233));
      expect(kWorldDots.length, 2343);
    });
  });

  group('page', () {
    testWidgets('Home in the web\'s phone order: the strip, the shortcuts, the accounts', (tester) async {
      await pumpApp(tester, signedIn: true);
      expect(find.byType(DashboardScreen), findsOneWidget);
      final page = find.descendant(of: find.byType(DashboardScreen), matching: find.byType(Scrollable)).first;
      var lastY = double.negativeInfinity;
      // two cells to a row: the strip's rows, then the sections under it
      expect(find.text('WALLET BALANCE'), findsOneWidget);
      expect(find.text('OPEN POSITIONS'), findsOneWidget);
      for (final s in ['TOTAL BALANCE', 'TOTAL EQUITY', "TODAY'S P&L", 'REWARDS', 'Shortcuts', 'Your accounts']) {
        for (var i = 0; i < 80 && find.text(s).hitTestable().evaluate().isEmpty; i++) {
          await tester.drag(page, const Offset(0, -250));
          await tester.pump(const Duration(milliseconds: 50));
        }
        final y = tester.getTopLeft(find.text(s).first).dy + tester.state<ScrollableState>(page).position.pixels;
        expect(y, greaterThan(lastY), reason: s);
        lastY = y;
      }
      await unmount(tester);
    });
  });

  test('the support address falls back to Kalks\' own', () {
    expect(supportEmailOf(AppConfig.fallback), 'support@kalkstrade.com');
  });
}

// The order sheet (web order form) and the position sheet (web PositionDialog) on the preview trade server, with
// fixed quotes and the trade actions recorded instead of sent.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/cfd/order_sheet.dart';
import 'package:kalks/features/terminal/cfd/position_sheet.dart';
import 'package:kalks/features/terminal/cfd/room.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/order.dart';
import 'package:kalks/features/terminal/core/sessions.dart';
import 'package:kalks/features/terminal/core/terminal_controller.dart';
import 'package:kalks/features/terminal/core/trade_actions.dart';
import 'package:kalks/features/terminal/widgets/kit.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

/// Records what the sheets ask for.
class RecordingActions extends TradeActions {
  RecordingActions(super.ref);
  final List<OrderRequest> orders = [];
  final List<(String, double?)> closes = [];
  final List<Map<String, Object?>> modifies = [];

  @override
  Future<bool> placeOrder(OrderRequest o) async {
    orders.add(o);
    return true;
  }

  @override
  Future<bool> closePosition(String ticket, {double? volume}) async {
    closes.add((ticket, volume));
    return true;
  }

  @override
  Future<bool> modifyPosition(
    String ticket, {
    double? sl,
    double? tp,
    bool clearSl = false,
    bool clearTp = false,
    int? trailingPoints,
    bool setTrailing = false,
  }) async {
    modifies.add({'ticket': ticket, 'sl': sl, 'tp': tp, 'clearSl': clearSl, 'clearTp': clearTp, 'trailing': trailingPoints});
    return true;
  }
}

void main() {
  late Harness h;
  late RecordingActions actions;

  Future<void> boot(WidgetTester tester) async {
    await loadFonts();
    tester.view.physicalSize = const Size(412, 915);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    await tester.runAsync(() async {
      h = await Harness.create(
        extra: [
          marketFeedProvider.overrideWith(
            (ref) => FixedFeed({'BTCUSD': q('BTCUSD', 63400, 63418), 'XAUUSD': q('XAUUSD', 2654.30, 2654.48), 'EURUSD': q('EURUSD', 1.0845, 1.0846)}),
          ),
          tradeActionsProvider.overrideWith((ref) => actions = RecordingActions(ref)),
        ],
      );
      h.container.listen(terminalProvider, (_, _) {});
      await h.container.read(tradeSessionsProvider.notifier).start(preferred: '10042817');
      await h.container.read(symbolsProvider.future);
      await until(() => h.container.read(terminalProvider).synced);
    });
    h.container.read(tradeActionsProvider);
  }

  Future<void> show(WidgetTester tester, Widget child) async {
    final t = h.container.read(tProvider);
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: h.container,
        child: MaterialApp(
          theme: KTheme.trader(Brightness.dark),
          home: I18nScope(
            t: t,
            child: Scaffold(body: child),
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 50));
  }

  Future<void> done(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(() async => h.container.dispose());
  }

  Finder stopInput(String key) => find.descendant(of: find.byKey(ValueKey(key)), matching: find.byType(TextField));

  testWidgets('order ticket: the volume stepper, S by points, SELL / BUY by market send the named trade', (tester) async {
    await boot(tester);
    await show(tester, const OrderForm(symbol: 'BTCUSD'));
    // no side yet: the line over SELL / BUY says to choose, both carry their live price
    expect(find.text('Choose Sell or Buy'), findsOneWidget);
    expect(find.byKey(const ValueKey('ticket-sell')), findsOneWidget);
    expect(find.byKey(const ValueKey('ticket-buy')), findsOneWidget);
    expect(find.byKey(const ValueKey('ticket-type-market')), findsOneWidget);

    // ±0.1 / ±0.01 from the default 0.50
    await tester.tap(find.text('−0.1'));
    await tester.pump();
    await tester.tap(find.text('+.01'));
    await tester.pump();
    expect(find.text('0.41'), findsOneWidget);
    // a preset
    await tester.tap(find.text('0.1'));
    await tester.pump();
    expect(find.text('0.10'), findsOneWidget);

    // one Price / Points toggle for both stops; S in points shows the money at risk
    expect(find.descendant(of: find.byKey(const ValueKey('ticket-unit')), matching: find.text('Price')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('ticket-unit')));
    await tester.pump();
    expect(find.descendant(of: find.byKey(const ValueKey('ticket-unit')), matching: find.text('Points')), findsOneWidget);
    await tester.enterText(stopInput('ticket-sl'), '1000');
    await tester.pump();
    expect(find.textContaining('Risk -'), findsOneWidget);
    // margin and free margin after the trade
    expect(find.byKey(const ValueKey('ticket-margin')), findsOneWidget);

    // BUY by market sends it at once (the button is the confirmation and names the trade)
    expect(tester.widget<KPressable>(find.byKey(const ValueKey('ticket-buy'))).semanticLabel, 'Buy 0.10 lot BTCUSD at market');
    await tester.tap(find.byKey(const ValueKey('ticket-buy')));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));
    expect(actions.orders, hasLength(1));
    final o = actions.orders.single;
    expect(o.side, 'buy');
    expect(o.type, 'market');
    expect(o.volume, 0.1);
    expect(o.sl, isNotNull);
    expect(o.sl!, lessThan(63418));
    expect(o.tp, isNull);
    await done(tester);
  });

  testWidgets('order ticket: S in points is that far under the entry, with the money at risk', (tester) async {
    await boot(tester);
    await show(tester, const OrderForm(symbol: 'XAUUSD', side: 'buy'));
    // a side came with the ticket: the line names the trade
    expect(find.text('Buy 0.50 lot XAUUSD at market'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('ticket-unit')));
    await tester.pump();
    // ask 2654.48, 100 points (10 pips of 0.10) under it, 50.00 USD at risk on 0.50 lot (the web's default volume)
    await tester.enterText(stopInput('ticket-sl'), '100');
    await tester.pump();
    expect(find.textContaining('2653.48'), findsOneWidget);
    expect(find.text('Risk -50.00'), findsOneWidget);
    // back to price: the stop is converted, not lost
    await tester.tap(find.byKey(const ValueKey('ticket-unit')));
    await tester.pump();
    expect(find.text('2653.48'), findsOneWidget);
    await done(tester);
  });

  testWidgets('order ticket: a limit order asks for its price first; the type chips pick type and side; Place sends it', (tester) async {
    await boot(tester);
    await show(tester, const OrderForm(symbol: 'BTCUSD', side: 'sell', type: 'limit'));
    expect(find.text('Enter the order price'), findsOneWidget);
    expect(find.text('Order price'), findsOneWidget);
    expect(find.text('Expires'), findsOneWidget);
    expect(find.text('Sell limit'), findsOneWidget);
    expect(tester.widget<RoomChip>(find.byKey(const ValueKey('ticket-type-limit-sell'))).selected, isTrue);

    // a buy stop instead
    await tester.ensureVisible(find.byKey(const ValueKey('ticket-type-stop-buy')));
    await tester.tap(find.byKey(const ValueKey('ticket-type-stop-buy')));
    await tester.pump();
    expect(find.text('Stop price'), findsOneWidget);
    await tester.enterText(find.descendant(of: find.byType(TStepper).first, matching: find.byType(TextField)), '64000');
    await tester.pump();
    expect(find.text('Place buy stop 0.50 lot BTCUSD at 64000.00'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('ticket-place')));
    await tester.pump(const Duration(milliseconds: 100));
    final o = actions.orders.single;
    expect((o.type, o.side, o.price), ('stop', 'buy', 64000.0));
    await done(tester);
  });

  testWidgets('position sheet: close part of it, nudge the stop loss, breakeven, modify', (tester) async {
    await boot(tester);
    final p = h.container.read(terminalProvider).positions.firstWhere((x) => x.symbol == 'XAUUSD');
    await show(tester, PositionForm(ticket: p.ticket));
    expect(find.text('Floating profit'), findsOneWidget);

    await tester.tap(find.text('50%'));
    await tester.pump();
    final closeLabel = find.textContaining('Close #${p.ticket} buy 0.25 XAUUSD');
    expect(closeLabel, findsOneWidget);
    await tester.tap(closeLabel);
    await tester.pump(const Duration(milliseconds: 100));
    expect(actions.closes.single, (p.ticket, 0.25));

    // 10 pips under the bid (a buy's stop loss is below the price)
    await tester.tap(find.text('10p').first);
    await tester.pump();
    await tester.tap(find.text('Modify').last);
    await tester.pump(const Duration(milliseconds: 100));
    expect(actions.modifies.last['sl'], closeTo(2654.30 - 10 * 0.1, 1e-9));

    await tester.tap(find.textContaining('Move SL to breakeven'));
    await tester.pump();
    await tester.tap(find.text('Modify').last);
    await tester.pump(const Duration(milliseconds: 100));
    expect(actions.modifies.last['sl'], p.openPrice);

    // the stops are fields labelled just S / T; Close ½ closes half the volume, Close all of it
    expect(find.descendant(of: find.byKey(const ValueKey('pos-sl')), matching: find.text('S')), findsOneWidget);
    expect(find.descendant(of: find.byKey(const ValueKey('pos-tp')), matching: find.text('T')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('pos-half')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(actions.closes.last, (p.ticket, 0.25));
    await tester.tap(find.byKey(const ValueKey('pos-close')));
    await tester.pump(const Duration(milliseconds: 100));
    expect(actions.closes.last, (p.ticket, p.volume));
    await done(tester);
  });
}

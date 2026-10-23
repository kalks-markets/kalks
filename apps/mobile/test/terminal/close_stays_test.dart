// Closing from the Trade tab keeps the client in the trade room. The position sheet (and the pending order sheet) used
// to pop twice when what it showed went away: once after the successful close, once from its "closed meanwhile" check
// when the stream removed the position. Both popped the root navigator, so the second pop took the trade room's own
// route (/trader) with it and the client landed back in the app shell.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/features/terminal/cfd/position_sheet.dart';
import 'package:kalks/features/terminal/cfd/trade_tab.dart';
import 'package:kalks/features/terminal/core/market.dart';
import 'package:kalks/features/terminal/core/sessions.dart';
import 'package:kalks/features/terminal/core/terminal_controller.dart';
import 'package:kalks/features/terminal/core/trade_actions.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/ui/ui.dart';

import 'harness.dart';

/// Trade actions that succeed and, like the engine, push the removal on the account stream (before the reply when
/// `streamFirst`, after it otherwise).
class StreamingActions extends TradeActions {
  StreamingActions(super.ref, this.h, {this.streamFirst = true});
  final Harness h;
  final bool streamFirst;

  Future<bool> _done(Map<String, Object?> frame) async {
    if (streamFirst) {
      h.engines.last.push(frame);
      await Future<void>.delayed(Duration.zero);
    } else {
      Future<void>.delayed(const Duration(milliseconds: 30), () => h.engines.last.push(frame)).ignore();
    }
    return true;
  }

  @override
  Future<bool> closePosition(String ticket, {double? volume}) => _done({'type': 'position', 'op': 'remove', 'ticket': int.parse(ticket)});

  @override
  Future<bool> closeBy(String ticket, String by) async {
    h.engines.last.push({'type': 'position', 'op': 'remove', 'ticket': int.parse(by)});
    return _done({'type': 'position', 'op': 'remove', 'ticket': int.parse(ticket)});
  }

  @override
  Future<bool> cancelOrder(String ticket) => _done({'type': 'order', 'op': 'remove', 'ticket': int.parse(ticket), 'status': 'cancelled'});
}

void main() {
  late Harness h;

  Future<void> boot(WidgetTester tester, {bool streamFirst = true}) async {
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
          tradeActionsProvider.overrideWith((ref) => StreamingActions(ref, h, streamFirst: streamFirst)),
        ],
      );
      h.container.listen(terminalProvider, (_, _) {});
      await h.container.read(tradeSessionsProvider.notifier).start(preferred: '10042817');
      await h.container.read(symbolsProvider.future);
      await until(() => h.container.read(terminalProvider).synced && h.engines.isNotEmpty);
    });
    // the app shell, with the trade room pushed on the root navigator (as /trader is)
    await tester.pumpWidget(
      UncontrolledProviderScope(
        container: h.container,
        child: MaterialApp(
          theme: KTheme.trader(Brightness.dark),
          builder: (context, child) => I18nScope(t: h.container.read(tProvider), child: child!),
          home: Builder(
            builder: (context) => Scaffold(
              body: Center(
                child: TextButton(
                  key: const ValueKey('open-room'),
                  onPressed: () => Navigator.of(context).push(MaterialPageRoute<void>(builder: (_) => const Scaffold(body: TradeTab()))),
                  child: const Text('APP SHELL'),
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.byKey(const ValueKey('open-room')));
    await tester.pumpAndSettle();
    expect(find.byType(TradeTab), findsOneWidget);
  }

  Future<void> settle(WidgetTester tester) async {
    for (var i = 0; i < 10; i++) {
      await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 10)));
      await tester.pump(const Duration(milliseconds: 100));
    }
  }

  void stillInRoom() {
    expect(find.byType(TradeTab), findsOneWidget, reason: 'the trade room is still on screen');
    expect(find.text('APP SHELL'), findsNothing, reason: 'not back in the app shell');
    expect(find.byType(PositionForm), findsNothing, reason: 'the sheet is dismissed');
    expect(find.byType(PendingForm), findsNothing, reason: 'the sheet is dismissed');
  }

  Future<void> done(WidgetTester tester) async {
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.runAsync(() async => h.container.dispose());
  }

  Future<String> openPosition(WidgetTester tester) async {
    final p = h.container.read(terminalProvider).positions.first;
    await tester.tap(find.byKey(ValueKey('p${p.ticket}')));
    await tester.pumpAndSettle();
    expect(find.byType(PositionForm), findsOneWidget);
    return p.ticket;
  }

  for (final streamFirst in [true, false]) {
    final when = streamFirst ? 'stream before the reply' : 'stream after the reply';

    testWidgets('position sheet Close: the sheet goes, the trade room stays ($when)', (tester) async {
      await boot(tester, streamFirst: streamFirst);
      final ticket = await openPosition(tester);
      await tester.tap(find.byKey(const ValueKey('pos-close')));
      await settle(tester);
      expect(h.container.read(terminalProvider).positions.any((p) => p.ticket == ticket), isFalse);
      stillInRoom();
      await done(tester);
    });

    testWidgets('pending sheet Cancel: the sheet goes, the trade room stays ($when)', (tester) async {
      await boot(tester, streamFirst: streamFirst);
      final o = h.container.read(terminalProvider).orders.first;
      await tester.tap(find.byKey(ValueKey('o${o.ticket}')));
      await tester.pumpAndSettle();
      expect(find.byType(PendingForm), findsOneWidget);
      await tester.tap(find.text('Delete order'));
      await settle(tester);
      expect(h.container.read(terminalProvider).orders.any((x) => x.ticket == o.ticket), isFalse);
      stillInRoom();
      await done(tester);
    });
  }

  testWidgets('a position closed elsewhere while its sheet is open: the sheet goes, the trade room stays', (tester) async {
    await boot(tester);
    final ticket = await openPosition(tester);
    h.engines.last.push({'type': 'position', 'op': 'remove', 'ticket': int.parse(ticket)});
    await settle(tester);
    stillInRoom();
    await done(tester);
  });

  testWidgets('the card Cancel of a pending order stays in the trade room', (tester) async {
    await boot(tester);
    final st = h.container.read(terminalProvider);
    final o = st.orders.first;
    await tester.tap(find.byKey(ValueKey('cancel-${o.ticket}')));
    await settle(tester);
    expect(h.container.read(terminalProvider).orders.any((x) => x.ticket == o.ticket), isFalse);
    stillInRoom();
    await done(tester);
  });
}

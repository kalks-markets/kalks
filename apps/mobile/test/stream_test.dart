// Reconnect logic of the streams (lib/core/realtime): backoff with jitter, a fresh ticket per attempt, resubscribe
// after every (re)connect, the silent-socket watchdog, fatal 401s, the engine's snapshot / resync / ended frames,
// and the market-data demand rules (passive everything, active only what is wanted, sent as diffs).
import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/realtime/backoff.dart';
import 'package:kalks/core/realtime/market_stream.dart';
import 'package:kalks/core/realtime/socket.dart';
import 'package:web_socket_channel/web_socket_channel.dart';

class FakeSink implements WebSocketSink {
  FakeSink(this.onClose);
  final void Function() onClose;
  final List<Object?> sent = [];
  final _done = Completer<void>();

  @override
  void add(Object? data) => sent.add(data);
  @override
  void addError(Object error, [StackTrace? stackTrace]) {}
  @override
  Future<void> addStream(Stream<Object?> stream) async {}
  @override
  Future<void> close([int? closeCode, String? closeReason]) async {
    onClose();
    if (!_done.isCompleted) _done.complete();
  }

  @override
  Future<void> get done => _done.future;
}

class FakeChannel implements WebSocketChannel {
  FakeChannel(this.url) {
    sink = FakeSink(_in.close);
  }
  final Uri url;
  // closed by the sink or drop()
  // ignore: close_sinks
  final _in = StreamController<Object?>();
  @override
  // the socket under test closes it
  // ignore: close_sinks
  late final FakeSink sink;
  @override
  Stream<Object?> get stream => _in.stream;
  @override
  Future<void> get ready => Future<void>.value();
  @override
  String? get protocol => null;
  @override
  int? get closeCode => null;
  @override
  String? get closeReason => null;

  /// The server sends a frame.
  void push(Map<String, Object?> frame) => _in.add(jsonEncode(frame));

  /// The connection drops.
  void drop() => _in.close();

  List<Map<String, dynamic>> get frames => [for (final s in sink.sent) jsonDecode(s! as String) as Map<String, dynamic>];

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _NoJitter implements math.Random {
  @override
  double nextDouble() => 0.5;
  @override
  int nextInt(int max) => 0;
  @override
  bool nextBool() => false;
}

void main() {
  group('Backoff', () {
    test('doubles from the first delay up to the cap, then resets', () {
      final b = Backoff(random: _NoJitter());
      expect([for (var i = 0; i < 7; i++) b.next().inMilliseconds], [1000, 2000, 4000, 8000, 16000, 20000, 20000]);
      b.reset();
      expect(b.next(), const Duration(seconds: 1));
    });

    test('jitter stays within ±25 %', () {
      final b = Backoff(random: math.Random(7));
      for (var i = 0; i < 40; i++) {
        final ms = b.next().inMilliseconds;
        final base = math.min(1000 * math.pow(2, math.min(i, 16)), 20000);
        expect(ms, inInclusiveRange((base * 0.75).floor(), (base * 1.25).ceil()));
      }
    });
  });

  group('ReconnectingSocket', () {
    test('reconnects with backoff, a fresh ticket each time, and resubscribes', () {
      fakeAsync((async) {
        final channels = <FakeChannel>[];
        var tickets = 0;
        var opens = 0;
        final frames = <Map<String, dynamic>>[];
        final s = ReconnectingSocket(
          name: 'test',
          resolveUrl: () async => Uri.parse('wss://x/stream?ticket=${++tickets}'),
          onFrame: frames.add,
          onOpen: () => opens++,
          backoff: Backoff(random: _NoJitter()),
          connector: (u) => (channels..add(FakeChannel(u))).last,
        )..start();
        async.flushMicrotasks();
        expect(channels.single.url.queryParameters['ticket'], '1');
        expect(s.status, SocketStatus.open);
        expect(opens, 1);
        channels.single.push({'type': 'hello', 'unread': 2});
        async.flushMicrotasks();
        expect(frames.single['unread'], 2);

        channels.single.drop();
        async.flushMicrotasks();
        expect(s.status, SocketStatus.reconnecting);
        async.elapse(const Duration(milliseconds: 999));
        expect(channels, hasLength(1));
        async.elapse(const Duration(milliseconds: 2));
        expect(channels, hasLength(2));
        expect(channels.last.url.queryParameters['ticket'], '2');
        expect(opens, 2);

        // opening reset the backoff: the next drop waits 1 s again
        channels.last.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(milliseconds: 1001));
        expect(channels, hasLength(3));
        s.stop();
        expect(s.status, SocketStatus.closed);
      });
    });

    test('a failing ticket call backs off longer each time', () {
      fakeAsync((async) {
        var calls = 0;
        final s = ReconnectingSocket(
          name: 'test',
          resolveUrl: () async {
            calls++;
            throw StateError('service down');
          },
          onFrame: (_) {},
          backoff: Backoff(random: _NoJitter()),
          connector: FakeChannel.new,
        )..start();
        async.flushMicrotasks();
        expect(calls, 1);
        async.elapse(const Duration(seconds: 1));
        expect(calls, 2);
        async.elapse(const Duration(seconds: 2));
        expect(calls, 3);
        async.elapse(const Duration(seconds: 3));
        expect(calls, 3);
        async.elapse(const Duration(seconds: 1));
        expect(calls, 4);
        s.stop();
      });
    });

    test('a silent socket is dropped by the watchdog and reopened', () {
      fakeAsync((async) {
        final channels = <FakeChannel>[];
        final s = ReconnectingSocket(
          name: 'test',
          resolveUrl: () async => Uri.parse('wss://x'),
          onFrame: (_) {},
          backoff: Backoff(random: _NoJitter()),
          connector: (u) => (channels..add(FakeChannel(u))).last,
        )..start();
        async.flushMicrotasks();
        // heartbeats keep it alive
        for (var i = 0; i < 6; i++) {
          async.elapse(const Duration(seconds: 5));
          channels.last.push({'type': 'hb', 't': i});
          async.flushMicrotasks();
        }
        expect(channels, hasLength(1));
        // then silence
        async.elapse(const Duration(seconds: 21));
        async.elapse(const Duration(seconds: 1));
        expect(channels, hasLength(2));
        s.stop();
      });
    });

    test('401 from the ticket call stops the socket and reports it', () {
      fakeAsync((async) {
        final fatal = <String>[];
        var calls = 0;
        final s = ReconnectingSocket(
          name: 'test',
          resolveUrl: () async {
            calls++;
            throw const SocketFatal('session_expired');
          },
          onFrame: (_) {},
          onFatal: fatal.add,
          connector: FakeChannel.new,
        )..start();
        async.flushMicrotasks();
        async.elapse(const Duration(minutes: 5));
        expect(fatal, ['session_expired']);
        expect(calls, 1);
        expect(s.status, SocketStatus.closed);
      });
    });

    test('a feature that is off is retried quietly later', () {
      fakeAsync((async) {
        var calls = 0;
        final statuses = <SocketStatus>[];
        final s = ReconnectingSocket(
          name: 'options',
          resolveUrl: () async {
            calls++;
            throw const SocketUnavailable();
          },
          onFrame: (_) {},
          onStatus: statuses.add,
          connector: FakeChannel.new,
        )..start();
        async.flushMicrotasks();
        expect(statuses.last, SocketStatus.unavailable);
        async.elapse(const Duration(seconds: 59));
        expect(calls, 1);
        async.elapse(const Duration(seconds: 2));
        expect(calls, 2);
        s.stop();
      });
    });

    test('isReady: the backoff resets only on the ready frame (engine snapshot)', () {
      fakeAsync((async) {
        final channels = <FakeChannel>[];
        final s = ReconnectingSocket(
          name: 'engine',
          resolveUrl: () async => Uri.parse('wss://x'),
          onFrame: (_) {},
          isReady: (f) => f['type'] == 'snapshot',
          backoff: Backoff(random: _NoJitter()),
          connector: (u) => (channels..add(FakeChannel(u))).last,
        )..start();
        async.flushMicrotasks();
        channels.last.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(seconds: 1));
        expect(channels, hasLength(2));
        // opened, but no snapshot yet: the next wait is 2 s
        channels.last.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(milliseconds: 1500));
        expect(channels, hasLength(2));
        async.elapse(const Duration(milliseconds: 600));
        expect(channels, hasLength(3));
        channels.last.push({'type': 'snapshot'});
        async.flushMicrotasks();
        channels.last.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(milliseconds: 1001));
        expect(channels, hasLength(4));
        s.stop();
      });
    });
  });

  group('MarketStream demand (packages/mock/src/prices.ts rules)', () {
    test('passive everything on open, active only what is listened to or wanted, sent as diffs', () {
      fakeAsync((async) {
        final channels = <FakeChannel>[];
        final m = MarketStream(
          wsUrl: 'wss://api/v1/stream',
          allSymbols: ['EURUSD', 'XAUUSD', 'BTCUSD', 'US500'],
          group: 'pro',
          connector: (u) => (channels..add(FakeChannel(u))).last,
        )..start();
        async.flushMicrotasks();
        final ch = channels.single;
        expect(ch.url.queryParameters['group'], 'pro');
        expect(ch.frames.first, {
          'op': 'subscribe',
          'symbols': ['EURUSD', 'XAUUSD', 'BTCUSD', 'US500'],
          'passive': true,
        });

        final quotes = <Quote>[];
        final off = m.subscribe(['EURUSD', 'XAUUSD'], quotes.add);
        m.want('positions', ['BTCUSD']);
        async.elapse(const Duration(milliseconds: 699));
        expect(ch.frames, hasLength(1));
        async.elapse(const Duration(milliseconds: 2));
        expect(ch.frames.last['op'], 'subscribe');
        expect(ch.frames.last.containsKey('passive'), isFalse);
        expect((ch.frames.last['symbols'] as List).toSet(), {'EURUSD', 'XAUUSD', 'BTCUSD'});

        ch.push({'type': 'quote', 's': 'EURUSD', 'b': 1.1, 'a': 1.1002, 't': 1});
        ch.push({'type': 'quote', 's': 'EURUSD', 'b': 1.1001, 'a': 1.1003, 't': 2, 'd': 1});
        async.flushMicrotasks();
        expect(quotes.map((q) => q.dir), [0, 1]);
        expect(quotes.last.delayed, isTrue);

        off();
        async.elapse(const Duration(seconds: 1));
        expect(ch.frames.last, {
          'op': 'subscribe',
          'symbols': ['EURUSD', 'XAUUSD'],
          'passive': true,
        });
        expect(m.activeSent, {'BTCUSD'});

        // a reconnect sends the passive list and the whole active demand again
        ch.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(seconds: 1));
        final again = channels.last.frames;
        expect(again[0]['passive'], isTrue);
        expect(again[1], {
          'op': 'subscribe',
          'symbols': ['BTCUSD'],
        });
        m.stop();
      });
    });

    test('a chart hears of every reconnect (not the first connect) and re-asks for its bars', () {
      fakeAsync((async) {
        final channels = <FakeChannel>[];
        final m = MarketStream(wsUrl: 'wss://api/v1/stream', allSymbols: ['EURUSD'], connector: (u) => (channels..add(FakeChannel(u))).last)..start();
        async.flushMicrotasks();
        var resyncs = 0;
        final off = m.onReconnect(() => resyncs++);
        m.subscribeBars('EURUSD', 'M1', (_) {});
        expect(resyncs, 0);
        channels.last.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(seconds: 1));
        expect(channels, hasLength(2));
        expect(channels.last.frames.any((f) => f['op'] == 'bars' && f['symbol'] == 'EURUSD' && f['tf'] == 'M1'), isTrue);
        expect(resyncs, 1);
        off();
        channels.last.drop();
        async.flushMicrotasks();
        async.elapse(const Duration(seconds: 1));
        expect(resyncs, 1);
        m.stop();
      });
    });
  });
}

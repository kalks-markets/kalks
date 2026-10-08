// Support module: the Ask Kalks AI routing rules (lib/ask-ai.ts, as apps/crm/tests/ask-ai.test.mjs), attachment
// checks, the bot's markdown, the API shapes, the chat's and Ask Kalks AI's stream frames, the attachment upload
// path; and on the sample-data API: the Support page in the web's phone order, sending a message, attaching a file
// with an injected picker, the Ask Kalks AI sheet (suggestion -> question -> answer with its extra), the open request
// for a person (hold, send to the team, close it and ask), and the floating launcher.
import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/app.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/config/app_config.dart';
import 'package:kalks/core/notifications/notifications.dart';
import 'package:kalks/features/support/ask_ai.dart';
import 'package:kalks/features/support/ask_ai_engine.dart';
import 'package:kalks/features/support/ask_ai_rules.dart';
import 'package:kalks/features/support/launcher.dart';
import 'package:kalks/features/support/live_chat_controller.dart';
import 'package:kalks/features/support/support_data.dart';
import 'package:kalks/features/support/support_models.dart';
import 'package:kalks/i18n/t.dart';
import 'package:kalks/preview/c1/preview_support.dart';
import 'package:kalks/preview/preview_adapter.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import 'helpers/test_app.dart';

/// The sample-data API that also records every request (path, headers, body).
class _Recorder implements HttpClientAdapter {
  final PreviewAdapter _inner = PreviewAdapter(latency: Duration.zero);
  final List<RequestOptions> calls = [];

  @override
  Future<ResponseBody> fetch(RequestOptions o, Stream<Uint8List>? requestStream, Future<void>? cancelFuture) {
    calls.add(o);
    return _inner.fetch(o, requestStream, cancelFuture);
  }

  @override
  void close({bool force = false}) {}

  List<RequestOptions> to(String path) => calls.where((o) => o.uri.path.endsWith('/api/mobile/$path')).toList();
}

ApiClient _api(_Recorder r) => ApiClient(
  baseUrl: 'http://test/api/mobile',
  adapter: r,
  context: ApiContext(token: () => 'token', deviceId: () async => 'device', locale: () => 'en', appVersion: '1.0.0', userAgent: 'test'),
);

T _en() {
  final en = (jsonDecode(File('assets/i18n/en.json').readAsStringSync()) as Map).cast<String, Object?>();
  return T('en', en, en);
}

/// Lets the sample API answer (one call takes about six event-loop turns through Dio).
Future<void> _flush() async {
  for (var i = 0; i < 30; i++) {
    await Future<void>.delayed(Duration.zero);
  }
}

ProviderContainer _container(WidgetTester tester) => ProviderScope.containerOf(tester.element(find.byType(KalksApp)));

/// Opens a page with the given body on top of the app (the Dashboard hosts AskAi once wired; the shell the launcher).
Future<void> _page(WidgetTester tester, Widget body) async {
  unawaited(
    rootNavigatorKey.currentState!.push(
      MaterialPageRoute<void>(
        builder: (context) => Scaffold(
          backgroundColor: context.k.bg,
          body: Stack(
            children: [
              const Positioned.fill(child: KBackdrop()),
              SafeArea(
                child: Padding(padding: const EdgeInsets.all(16), child: body),
              ),
            ],
          ),
        ),
      ),
    ),
  );
  await settle(tester);
}

/// Lets the chat's / Ask AI's 2.5 s poll run once or twice (fake time) and the sample API answer.
Future<void> _poll(WidgetTester tester) async {
  for (var i = 0; i < 3; i++) {
    await tester.pump(const Duration(milliseconds: 2600));
    await settle(tester, frames: 4);
  }
}

/// A widget's top in the page's content (screen position + the page's scroll offset).
double _y(WidgetTester tester, Finder f) => tester.getTopLeft(f).dy + Scrollable.of(tester.element(f)).position.pixels;

/// Scrolls the page so [f] sits between the shell's header and tab bar (the page scrolls under both; a drag in the
/// middle of the screen would scroll the chat's messages instead).
Future<void> _reveal(WidgetTester tester, Finder f) async {
  final page = find.byType(KPageScroll);
  final pad = MediaQuery.paddingOf(tester.element(page));
  final view = tester.getRect(page);
  final pos = tester.state<ScrollableState>(find.descendant(of: page, matching: find.byType(Scrollable)).first).position;
  final r = tester.getRect(f);
  final top = view.top + pad.top + 8, bottom = view.bottom - pad.bottom - 8;
  final dy = r.top < top ? r.top - top : (r.bottom > bottom ? math.min(r.bottom - bottom, r.top - top) : 0.0);
  pos.jumpTo((pos.pixels + dy).clamp(pos.minScrollExtent, pos.maxScrollExtent));
  await settle(tester, frames: 4);
}

const _chips = [
  AiChip(
    key: 'freeMargin',
    label: "What's my free margin?",
    extra: AiFacts(
      title: 'Your live accounts',
      rows: [(label: '#10042817 · Pro', value: r'$12,071.40', tone: null), (label: '#10051123 · Cent', value: r'$3,010.25', tone: 'warn')],
    ),
  ),
  AiChip(key: 'deposit', label: 'How do I deposit?'),
];

void main() {
  setUp(() => PreviewSupport.reset('agent'));

  group('Ask Kalks AI routing (lib/ask-ai.ts)', () {
    test('a question goes to the bot when there is no open request for a person', () {
      expect(askRoute(null), AskRoute.bot);
      expect(askRoute('bot'), AskRoute.bot);
      // a resolved conversation is closed: the next message opens a new one, which the bot owns
      expect(askRoute('resolved'), AskRoute.bot);
    });

    test('a question is held while a request for a person is open (the bot never answers there)', () {
      expect(askRoute('waiting'), AskRoute.person);
      expect(askRoute('assigned'), AskRoute.person);
    });

    test('an open request for a person is shown only for waiting / assigned conversations', () {
      expect(['bot', 'waiting', 'assigned', 'resolved', null].map(withPerson).toList(), [false, true, true, false, false]);
    });

    test('waiting for the bot stops when the conversation is handed over to the team', () {
      expect(passedToTeam(true, 'waiting'), isTrue);
      expect(passedToTeam(true, 'assigned'), isTrue);
      expect(passedToTeam(true, 'bot'), isFalse);
      expect(passedToTeam(false, 'waiting'), isFalse);
    });
  });

  group('chat rules', () {
    test('attachments: images and PDF up to the broker limit (web onFile)', () {
      expect(attachmentProblem(size: 1024, mime: 'image/png'), isNull);
      expect(attachmentProblem(size: 1024, mime: 'image/jpeg'), isNull);
      expect(attachmentProblem(size: 1024, mime: 'image/jpg'), isNull);
      expect(attachmentProblem(size: 1024, mime: 'image/webp'), isNull);
      expect(attachmentProblem(size: 10 * 1024 * 1024, mime: 'application/pdf'), isNull);
      expect(attachmentProblem(size: 10 * 1024 * 1024 + 1, mime: 'application/pdf'), AttachmentProblem.tooLarge);
      expect(attachmentProblem(size: 3 * 1024 * 1024, mime: 'image/png', maxMb: 2), AttachmentProblem.tooLarge);
      expect(attachmentProblem(size: 1024, mime: 'image/svg+xml'), AttachmentProblem.unsupported);
      expect(attachmentProblem(size: 1024, mime: 'application/zip'), AttachmentProblem.unsupported);
      expect(supportMimeOf('Receipt.JPG'), 'image/jpeg');
      expect(supportMimeOf('scan.pdf'), 'application/pdf');
      expect(supportMimeOf('notes.txt'), 'application/octet-stream');
    });

    test('the bot markdown: bold, bullets, paragraphs (web Rich)', () {
      final b = parseChatRich('Check **Wallet → History**.\nIt shows the status.\n\n- **Pending**: confirming\n- Review\n\nDone');
      expect(b.length, 3);
      expect(b[0].list, isFalse);
      expect(b[0].lines.length, 2);
      expect(b[0].lines[0], [(text: 'Check ', bold: false), (text: 'Wallet → History', bold: true), (text: '.', bold: false)]);
      expect(b[1].list, isTrue);
      expect(b[1].lines.map((l) => l.map((r) => r.text).join()), ['Pending: confirming', 'Review']);
      expect(b[1].lines[0].first, (text: 'Pending', bold: true));
      expect(b[2].lines.single.single.text, 'Done');
    });

    test('greeting, status chips and the API shapes', () {
      expect(greetingRest("Hi there. I'm Kalks AI. Ask me anything."), "I'm Kalks AI. Ask me anything.");
      expect(greetingRest('Welcome to support.'), 'Welcome to support.');
      expect(supportStatus('waiting'), (label: 'support.status.waiting', tone: KChipTone.warn));
      expect(supportStatus('resolved').tone, KChipTone.neutral);
      final m = SupportMessage.fromJson(const {
        'id': 5,
        'conversationId': 2,
        'author': 'bot',
        'body': 'x',
        'attachment': {'id': 9, 'name': 'a.png', 'mime': 'image/png', 'size': 2048},
        'meta': {
          'cites': [
            {'slug': 's', 'title': 'T'},
          ],
        },
        'createdAt': '2026-10-08T10:00:00Z',
      });
      expect(m.attachment!.image, isTrue);
      expect(m.cites.single.title, 'T');
      final h = SupportHome.fromJson(const {
        'settings': {'botName': 'Ava', 'maxAttachmentMb': 5, 'ai': false},
        'conversation': {'id': 2, 'status': 'assigned', 'assigneeName': 'Mei', 'clientUnread': 1, 'createdAt': '2026-10-08T10:00:00Z'},
        'messages': [
          {'id': 7, 'conversationId': 2, 'author': 'agent', 'body': 'b'},
          {'id': 6, 'conversationId': 2, 'author': 'client', 'body': 'a'},
        ],
      });
      expect(h.settings.botName, 'Ava');
      expect(h.settings.ai, isFalse);
      expect(h.settings.maxAttachmentMb, 5);
      expect(h.conversation!.human, isTrue);
      expect(h.messages.map((x) => x.id), [6, 7]);
    });
  });

  group('live chat controller', () {
    late _Recorder rec;
    late SupportFrames frames;
    late List<(NotificationKind, String, String?)> toasts;
    PickedSupportFile? picked;

    LiveChatController make() => LiveChatController(
      api: _api(rec),
      frames: frames,
      t: _en,
      toast: (kind, title, {description}) => toasts.add((kind, title, description)),
      picker: () =>
          () async => picked,
    );

    setUp(() {
      rec = _Recorder();
      frames = SupportFrames(null);
      toasts = [];
      picked = null;
    });

    test('stream frames: the bot answer streams in, agent typing, other conversations ignored', () async {
      PreviewSupport.reset('bot');
      final c = make()..start();
      await _flush();
      expect(c.conv!.id, 7801);
      expect(c.msgs.length, 2);
      c.onFrame({'type': 'bot.typing', 'conversationId': 7801, 'streamId': 's1'});
      expect(c.stream, (id: 's1', text: ''));
      c
        ..onFrame({'type': 'bot.delta', 'conversationId': 7801, 'streamId': 's1', 'text': 'Hello '})
        ..onFrame({'type': 'bot.delta', 'conversationId': 7801, 'streamId': 's1', 'text': 'there'})
        ..onFrame({'type': 'bot.delta', 'conversationId': 9, 'streamId': 'x', 'text': 'nope'});
      expect(c.stream!.text, 'Hello there');
      c.onFrame({
        'type': 'message',
        'message': {'id': 99999, 'conversationId': 7801, 'author': 'bot', 'body': 'Hello there', 'meta': <String, Object>{}},
      });
      expect(c.stream, isNull);
      expect(c.msgs.last.body, 'Hello there');
      c.onFrame({
        'type': 'message',
        'message': {'id': 100000, 'conversationId': 1, 'author': 'agent', 'body': 'elsewhere'},
      });
      expect(c.msgs.length, 3);
      c.onFrame({
        'type': 'conversation',
        'conversation': {'id': 7801, 'status': 'assigned', 'assigneeName': 'Mei Lin', 'createdAt': '2026-10-08T10:00:00Z'},
      });
      expect(c.status, 'assigned');
      expect(c.human, isTrue);
      c.onFrame({'type': 'typing', 'conversationId': 7801, 'from': 'agent'});
      expect(c.agentTyping, isTrue);
      c.dispose();
    });

    test('a reconnect reloads, and unread replies are marked read', () async {
      final c = make()..start();
      await _flush();
      expect(c.conv!.status, 'assigned');
      expect(rec.to('support/read').length, 1);
      c.onFrame({'type': 'reconnected'});
      await _flush();
      expect(rec.to('support/me').length, 2);
      c.dispose();
    });

    test('attachment: checked, uploaded raw with its type and name, then sent with the composer text', () async {
      final c = make()..start();
      await _flush();
      picked = (name: 'TX receipt.pdf', mime: 'application/pdf', size: 3, read: () async => Uint8List.fromList([1, 2, 3]));
      c.input.text = 'Here is the receipt';
      await c.attach();
      final up = rec.to('support/attachments').single;
      expect(up.method, 'POST');
      expect(up.contentType, 'application/pdf');
      expect(up.headers['X-File-Name'], 'TX%20receipt.pdf');
      expect(up.data, [1, 2, 3]);
      final msg = rec.to('support/messages').single;
      expect(msg.data, {'body': 'Here is the receipt', 'attachmentId': PreviewSupport.uploads.single['id']});
      expect(c.input.text, isEmpty);
      expect(c.msgs.last.attachment, isNotNull);

      // too large / not an image or PDF: refused before any upload
      picked = (name: 'big.pdf', mime: 'application/pdf', size: 11 * 1024 * 1024, read: () async => Uint8List(0));
      await c.attach();
      picked = (name: 'a.zip', mime: 'application/zip', size: 10, read: () async => Uint8List(0));
      await c.attach();
      expect(rec.to('support/attachments').length, 1);
      expect(toasts.map((x) => x.$2), ['File too large', 'Unsupported file']);
      expect(toasts.first.$3, 'Files can be up to 10 MB.');
      c.dispose();
    });

    test('a resolved chat starts a new conversation with the next message; rating after the end', () async {
      PreviewSupport.reset('bot');
      final c = make()..start();
      await _flush();
      await c.endChat();
      expect(c.resolved, isTrue);
      expect(c.canRate, isTrue);
      c.setRating(4);
      await c.rate();
      expect(c.conv!.csat!.rating, 4);
      expect(toasts.last.$2, 'Thanks for your feedback');
      await c.send(raw: 'Another question');
      expect(c.conv!.id, isNot(7801));
      expect(c.conv!.status, 'bot');
      expect(c.msgs.single.body, 'Another question');
      expect(c.stream, isNotNull);
      c.dispose();
    });
  });

  group('Ask Kalks AI engine', () {
    late _Recorder rec;
    late SupportFrames frames;

    AskAiEngine make({Duration poll = const Duration(milliseconds: 20)}) => AskAiEngine(api: _api(rec), frames: frames, t: _en, pollEvery: poll)..start();

    setUp(() {
      rec = _Recorder();
      frames = SupportFrames(null);
    });

    test('streams the answer from the frames; a hand-over by the bot stops the waiting', () async {
      PreviewSupport.reset('none');
      final e = make(poll: const Duration(minutes: 1));
      await _flush();
      expect(e.openRequest, isFalse);
      e.ask('What is a stop-out?');
      await _flush();
      expect(e.waiting, isTrue);
      expect(e.streaming, '');
      final conv = e.conv!.id;
      e
        ..onFrame({'type': 'bot.typing', 'conversationId': conv, 'streamId': 'a'})
        ..onFrame({'type': 'bot.delta', 'conversationId': conv, 'streamId': 'a', 'text': 'A **stop-out**'});
      expect(e.streaming, 'A **stop-out**');
      // the bot hands over: no bot answer is coming
      e.onFrame({
        'type': 'conversation',
        'conversation': {'id': conv, 'status': 'waiting', 'createdAt': '2026-10-08T10:00:00Z'},
      });
      expect(e.waiting, isFalse);
      expect(e.streaming, isNull);
      expect(e.withTeam, isTrue);
      e.dispose();
    });

    test('an open request for a person holds the question until the client chooses', () async {
      PreviewSupport.reset('waiting');
      final e = make();
      await _flush();
      expect(e.openRequest, isTrue);
      e.ask('How do I deposit?');
      await _flush();
      expect(e.blocked, isTrue);
      expect(rec.to('support/messages'), isEmpty);
      await e.sendToTeam();
      expect(rec.to('support/messages').length, 1);
      expect(e.blocked, isFalse);
      expect(e.waiting, isFalse);
      expect(e.withTeam, isTrue);
      e.dispose();
    });

    test('close it and ask: the request is resolved, then the bot answers in a new conversation', () async {
      PreviewSupport.reset('agent');
      final e = make();
      await _flush();
      e.ask('Explain margin level', chip: 'marginLevel');
      await _flush();
      expect(e.blocked, isTrue);
      await e.closeAndAsk();
      expect(rec.to('support/conversations/7801/resolve').length, 1);
      expect(rec.to('support/messages').length, 1);
      expect(e.status, 'bot');
      expect(e.waiting, isTrue);
      // the poll reads the conversation: the answer arrives
      await Future<void>.delayed(const Duration(milliseconds: 120));
      expect(e.waiting, isFalse);
      expect(e.answered, isTrue);
      expect(e.turns.first.chip, 'marginLevel');
      expect(e.turns.last.text, contains('Margin level'));
      e.dispose();
    });
  });

  group('screens', () {
    testWidgets('Support page: the web phone order (chat, conversations, email, notice)', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/support');
      await settle(tester);
      expect(
        find.text('Chat with Kalks AI for instant answers. Ask for a person at any time and our team takes over with the full conversation.'),
        findsOneWidget,
      );
      // the chat with Mei Lin: header, the bot answer, the agent's reply, the attachment
      final chat = find.byKey(const ValueKey('support-chat'));
      expect(chat, findsOneWidget);
      expect(find.text('Mei Lin'), findsWidgets);
      expect(find.text('Live agent'), findsOneWidget);
      expect(find.text('tx-receipt.pdf'), findsOneWidget);
      expect(find.text('Message Mei…'), findsOneWidget);
      expect(_y(tester, chat), lessThan(400));
      // the composer sits above the tab bar on arrival
      final pad = MediaQuery.paddingOf(tester.element(find.byType(KPageScroll)));
      expect(tester.getBottomLeft(find.byKey(const ValueKey('support-composer'))).dy, lessThan(915 - pad.bottom));
      final history = find.byKey(const ValueKey('support-history'));
      await _reveal(tester, find.text('Your conversations', skipOffstage: false));
      expect(find.text('Your conversations'), findsOneWidget);
      expect(find.text('How do I change my leverage?'), findsOneWidget);
      expect(find.text('With an agent'), findsOneWidget);
      final historyY = _y(tester, history);
      expect(historyY, greaterThan(_y(tester, chat)));
      await _reveal(tester, find.byKey(const ValueKey('support-email'), skipOffstage: false));
      final email = find.byKey(const ValueKey('support-email'));
      expect(find.text('Prefer email?'), findsOneWidget);
      expect(find.text('support@kalkstrade.com'), findsOneWidget);
      expect(find.text('Write to support'), findsOneWidget);
      expect(find.text('Copy client ID'), findsOneWidget);
      expect(_y(tester, email), greaterThan(historyY));
      await _reveal(tester, find.textContaining('Replies from our team also appear', skipOffstage: false));
      final notice = find.textContaining('Replies from our team also appear');
      expect(_y(tester, notice), greaterThan(_y(tester, email)));

      // a past conversation opens its transcript
      await _reveal(tester, find.text('How do I change my leverage?', skipOffstage: false));
      await tester.tap(find.text('How do I change my leverage?'));
      await settle(tester);
      expect(find.textContaining('Change leverage', findRichText: true), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('send a message: shown at once, the reply arrives (poll without a live stream)', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/support');
      await settle(tester);
      await tester.enterText(find.byKey(const ValueKey('support-composer')), 'Is it credited yet?');
      // a frame for the send button to enable with the text
      await settle(tester, frames: 2);
      await tester.tap(find.byIcon(LucideIcons.sendHorizontal));
      await settle(tester);
      expect(find.text('Is it credited yet?', findRichText: true), findsOneWidget);
      expect(PreviewSupport.sent.single['body'], 'Is it credited yet?');
      await _poll(tester);
      expect(find.text("Thanks, I'm checking this for you now.", findRichText: true), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('a new chat with the bot: quick reply, the answer arrives, then hand over to a person', (tester) async {
      PreviewSupport.reset('none');
      final c = await pumpApp(tester, signedIn: true);
      c.read(routerProvider).go('/support');
      await settle(tester);
      expect(find.textContaining('Hi Arjun.'), findsOneWidget);
      expect(find.text('Ask Kalks AI anything…'), findsOneWidget);
      // the quick replies scroll sideways (web: overflow-x-auto)
      await tester.scrollUntilVisible(
        find.text('What is a stop-out?'),
        150,
        scrollable: find.descendant(of: find.byKey(const ValueKey('support-quick')), matching: find.byType(Scrollable)),
      );
      // (that also scrolled the page: bring the pill back between the bars)
      await _reveal(tester, find.text('What is a stop-out?'));
      await tester.tap(find.text('What is a stop-out?'));
      // at once: the question and the bot's typing dots (the 2.5 s poll may bring the answer in a longer wait)
      await settle(tester, frames: 2);
      expect(PreviewSupport.sent.single['body'], 'What is a stop-out?');
      expect(find.byKey(const ValueKey('bot-streaming')), findsOneWidget);
      await _poll(tester);
      expect(find.byKey(const ValueKey('bot-streaming')), findsNothing);
      expect(find.textContaining('stop-out', findRichText: true), findsWidgets);
      // the menu: talk to a person
      await _reveal(tester, find.byKey(const ValueKey('support-chat')));
      await tester.tap(find.byIcon(LucideIcons.ellipsis));
      await settle(tester);
      expect(find.text('End chat'), findsOneWidget);
      await tester.tap(find.text('Talk to a person').last);
      await settle(tester);
      expect(find.text('Support team'), findsOneWidget);
      expect(find.text('In queue'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('attach a file with the injected picker: uploaded, then sent', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      c
          .read(supportPickerProvider.notifier)
          .set(() async => (name: 'receipt.pdf', mime: 'application/pdf', size: 3, read: () async => Uint8List.fromList([1, 2, 3])));
      c.read(routerProvider).go('/support');
      await settle(tester);
      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await settle(tester);
      expect(PreviewSupport.uploads.length, 1);
      expect(PreviewSupport.sent.single['attachmentId'], PreviewSupport.uploads.single['id']);
      expect(find.text('attachment.pdf'), findsOneWidget);

      // a file over the limit is refused with the web's message
      c
          .read(supportPickerProvider.notifier)
          .set(() async => (name: 'scan.pdf', mime: 'application/pdf', size: 20 * 1024 * 1024, read: () async => Uint8List(0)));
      await tester.tap(find.byIcon(LucideIcons.paperclip));
      await settle(tester);
      expect(find.text('File too large'), findsOneWidget);
      expect(PreviewSupport.uploads.length, 1);
      await unmount(tester);
    });

    testWidgets('Ask Kalks AI: pill -> sheet -> suggestion -> answer with its extra', (tester) async {
      PreviewSupport.reset('none');
      await pumpApp(tester, signedIn: true);
      await _page(tester, const AskAi(chips: _chips));
      expect(find.text('Ask Kalks AI'), findsOneWidget);
      expect(find.text('Ask Kalks AI anything…'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('ask-ai-pill')));
      await settle(tester);
      expect(find.text('Instant answers about your account, deposits and trading.'), findsOneWidget);
      expect(find.text('How do I deposit?'), findsOneWidget);
      await tester.tap(find.text("What's my free margin?"));
      await settle(tester);
      expect(PreviewSupport.sent.single['body'], "What's my free margin?");
      expect(find.byKey(const ValueKey('ai-streaming')), findsOneWidget);
      // the client's own figures under the question
      expect(find.text('Your live accounts'), findsOneWidget);
      expect(find.text(r'$12,071.40'), findsOneWidget);
      await _poll(tester);
      expect(find.byKey(const ValueKey('ai-streaming')), findsNothing);
      // in the thread (the pill behind the sheet shows the last answer on one line too)
      expect(find.descendant(of: find.byKey(const ValueKey('ai-thread')), matching: find.textContaining('Margin level', findRichText: true)), findsOneWidget);
      expect(find.text('Continue in chat'), findsOneWidget);
      expect(find.text('Talk to a person'), findsOneWidget);
      expect(find.text('New question'), findsOneWidget);
      expect(find.text('Ask a follow-up question…'), findsOneWidget);
      // New question clears the thread; the suggestions come back
      await tester.tap(find.text('New question'));
      await settle(tester);
      expect(find.text('How do I deposit?'), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('Ask Kalks AI with an open request for a person: hold, then send to our team', (tester) async {
      PreviewSupport.reset('waiting');
      await pumpApp(tester, signedIn: true);
      await _page(tester, const AskAi(chips: _chips));
      await tester.tap(find.byKey(const ValueKey('ask-ai-pill')));
      await settle(tester);
      expect(find.text('Your request for a person is still open.'), findsOneWidget);
      expect(find.text('View'), findsOneWidget);
      await tester.tap(find.text('How do I deposit?'));
      await settle(tester);
      expect(find.byKey(const ValueKey('ai-blocked')), findsOneWidget);
      expect(
        find.text("Kalks AI can't answer here while your request for a person is open. Close that request to ask Kalks AI, or send this to our team."),
        findsOneWidget,
      );
      expect(find.text('Close it and ask Kalks AI'), findsOneWidget);
      expect(PreviewSupport.sent, isEmpty);
      await tester.tap(find.text('Send to our team'));
      await settle(tester);
      expect(PreviewSupport.sent.single['body'], 'How do I deposit?');
      expect(find.text("Passed to our support team. They'll reply in your chat."), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('Ask Kalks AI: close the request and ask, the bot answers; Continue in chat opens the chat', (tester) async {
      PreviewSupport.reset('agent');
      await pumpApp(tester, signedIn: true);
      await _page(tester, const AskAi(chips: _chips));
      await tester.tap(find.byKey(const ValueKey('ask-ai-pill')));
      await settle(tester);
      expect(find.text("You're chatting with Mei Lin. Replies appear in your support chat."), findsOneWidget);
      await tester.enterText(find.byKey(const ValueKey('ai-input')), 'How do I verify my identity?');
      await tester.testTextInput.receiveAction(TextInputAction.send);
      await settle(tester);
      await tester.tap(find.text('Close it and ask Kalks AI'));
      await settle(tester);
      await _poll(tester);
      expect(
        find.descendant(of: find.byKey(const ValueKey('ai-thread')), matching: find.textContaining('Profile → Verification', findRichText: true)),
        findsOneWidget,
      );
      await tester.tap(find.text('Continue in chat'));
      await settle(tester);
      expect(find.byKey(const ValueKey('support-chat')), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('live chat switched off (support_chat): email-only page, no launcher, Ask Kalks AI without the chat', (tester) async {
      PreviewSupport.reset('none');
      final c = await pumpApp(
        tester,
        signedIn: true,
        config: AppConfig.fromJson(const {
          'modules': {'support_chat': false},
        }),
      );
      // the floating chat is gone from every page, and comes back when the broker switches the chat on again
      expect(find.byKey(const ValueKey('support-launcher')), findsNothing);
      setConfig(c, AppConfig.fromJson(const {'modules': <String, bool>{}}));
      await settle(tester, frames: 4);
      expect(find.byKey(const ValueKey('support-launcher')), findsOneWidget);
      setConfig(
        c,
        AppConfig.fromJson(const {
          'modules': {'support_chat': false},
        }),
      );
      await settle(tester, frames: 4);
      expect(find.byKey(const ValueKey('support-launcher')), findsNothing);

      // the Support page keeps the email channel and the note (web live-support.tsx)
      c.read(routerProvider).go('/support');
      await settle(tester);
      expect(find.byKey(const ValueKey('support-chat')), findsNothing);
      expect(find.byKey(const ValueKey('support-history')), findsNothing);
      expect(find.byKey(const ValueKey('support-email')), findsOneWidget);
      expect(find.text('Prefer email?'), findsOneWidget);
      expect(find.text('Write to support'), findsOneWidget);
      expect(find.textContaining('Replies from our team also appear'), findsOneWidget);

      // Ask Kalks AI still answers, with no way into the chat or to a person (web AskAi chat={false})
      await _page(tester, const AskAi(chips: _chips, chat: false));
      await tester.tap(find.byKey(const ValueKey('ask-ai-pill')));
      await settle(tester);
      final panel = find.byKey(const ValueKey('ask-ai-panel'));
      expect(find.descendant(of: panel, matching: find.byIcon(LucideIcons.messageCircle)), findsNothing);
      await tester.tap(find.text("What's my free margin?"));
      await settle(tester);
      await _poll(tester);
      expect(find.descendant(of: find.byKey(const ValueKey('ai-thread')), matching: find.textContaining('Margin level', findRichText: true)), findsOneWidget);
      expect(find.text('New question'), findsOneWidget);
      expect(find.text('Continue in chat'), findsNothing);
      expect(find.text('Talk to a person'), findsNothing);
      await unmount(tester);
    });

    testWidgets('launcher: hidden on /support, a badge for agent replies, opens the chat sheet', (tester) async {
      final c = await pumpApp(tester, signedIn: true);
      await _page(tester, const Stack(children: [SupportLauncher(path: '/support')]));
      expect(find.byKey(const ValueKey('support-launcher')), findsNothing);
      await _page(tester, const Stack(children: [SupportLauncher(path: '/wallet')]));
      expect(find.byKey(const ValueKey('support-launcher')), findsOneWidget);
      _container(tester).read(supportFramesProvider).add({
        'type': 'conversation',
        'conversation': {'id': 7801, 'status': 'assigned', 'clientUnread': 2, 'createdAt': '2026-10-08T10:00:00Z'},
      });
      await settle(tester, frames: 4);
      expect(find.byKey(const ValueKey('support-launcher-badge')), findsOneWidget);
      expect(find.text('2'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('support-launcher')));
      await settle(tester);
      expect(find.byKey(const ValueKey('support-chat')), findsOneWidget);
      expect(c.read(supportLauncherProvider).unread, 0);
      expect(find.text('Mei Lin'), findsWidgets);
      await unmount(tester);
    });
  });
}

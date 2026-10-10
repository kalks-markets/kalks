// Profile & Security module: the KYC instant checks on synthetic images (checks.ts), the slot / wizard rules
// (verification/api.ts + tracker.tsx), the upload's multipart shape (uploadDocument), the view-only login rules
// (lib/viewer.ts), device labels from user agents (parseDevice), the password rules; and on the sample-data API:
// each page's sections in the web's phone order, the password change and a new viewer confirmed with an emailed
// code, a KYC document fed through the injected picker (checks shown, uploaded, sent), notification switches and the
// theme / language switch.
//
// PROGRESS (agent C1 sub-agent; keep current):
//   done: lib/preview/c1/preview_profile.dart (security/*, kyc*, notifications/prefs, auth/password, auth/marketing);
//         kycPickerProvider made injectable (KycPickerController.use); notification prefs no longer read
//         translations in initState; history chips full width; KYC reference chip no longer stretched;
//         39 tests passing (29 unit + 10 widget); format + analyze clean; 53 screenshots
//         (test_shots/c1_profile_shots_test.dart).
//   next: nothing pending in this module.
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:dio/dio.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:kalks/core/api/api_providers.dart';
import 'package:kalks/core/theme_controller.dart';
import 'package:kalks/features/profile/kyc/kyc_checks.dart';
import 'package:kalks/features/profile/kyc/kyc_models.dart';
import 'package:kalks/features/profile/kyc/kyc_upload.dart';
import 'package:kalks/features/profile/notification_prefs_screen.dart';
import 'package:kalks/features/profile/security_data.dart';
import 'package:kalks/features/profile/viewers_screen.dart';
import 'package:kalks/features/profile/widgets/change_password_card.dart';
import 'package:kalks/i18n/i18n.dart';
import 'package:kalks/preview/c1/preview_profile.dart';
import 'package:kalks/preview/preview_adapter.dart';
import 'package:kalks/router/router.dart';
import 'package:kalks/ui/ui.dart';

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

/// A w × h grayscale image from a pixel function.
Gray _gray(int w, int h, double Function(int x, int y) px) {
  final g = Float32List(w * h);
  for (var y = 0; y < h; y++) {
    for (var x = 0; x < w; x++) {
      g[y * w + x] = px(x, y);
    }
  }
  return Gray(g, w, h);
}

/// A sharp, well-lit document-like pattern: 8 px squares of 60 / 200.
double _checker(int x, int y) => ((x ~/ 8) + (y ~/ 8)).isEven ? 60 : 200;

/// The preview's `GET kyc` as the app reads it.
KycState _kyc() => KycState.fromJson(PreviewProfile.answer('GET', 'kyc', const {}, const {})!.$2 as Map<String, dynamic>);
KycState _kycPost(String action, Map<String, dynamic> body) {
  final (status, data) = PreviewProfile.answer('POST', 'kyc/$action', body, const {})!;
  expect(status, 200, reason: '$action: $data');
  final j = data as Map<String, dynamic>;
  return KycState.fromJson(action == 'documents' ? (j['state'] as Map).cast<String, dynamic>() : j);
}

/// An encoded PNG (a checkerboard, or flat gray when `flat`), drawn with dart:ui.
Future<Uint8List> pngBytes(int w, int h, {bool flat = false}) async {
  final rec = ui.PictureRecorder();
  final canvas = ui.Canvas(rec);
  canvas.drawRect(ui.Rect.fromLTWH(0, 0, w.toDouble(), h.toDouble()), ui.Paint()..color = const ui.Color(0xFF7A7A7A));
  if (!flat) {
    final dark = ui.Paint()..color = const ui.Color(0xFF303030);
    final light = ui.Paint()..color = const ui.Color(0xFFD8D8D8);
    const s = 24.0;
    for (var y = 0.0; y < h; y += s) {
      for (var x = 0.0; x < w; x += s) {
        canvas.drawRect(ui.Rect.fromLTWH(x, y, s, s), (((x + y) / s).round()).isEven ? dark : light);
      }
    }
  }
  final img = await rec.endRecording().toImage(w, h);
  final data = await img.toByteData(format: ui.ImageByteFormat.png);
  img.dispose();
  return data!.buffer.asUint8List();
}

/// A picker that hands over fixed bytes (the phone's camera or file chooser in tests).
class _FakePicker implements KycPicker {
  _FakePicker(this.bytes, {this.name = 'photo.png'});
  final Uint8List bytes;
  final String name;
  final String mime = 'image/png';
  int cameraCalls = 0, fileCalls = 0;

  @override
  Future<KycPicked?> camera({required bool selfie}) async {
    cameraCalls++;
    return KycPicked(bytes: bytes, name: name, mime: mime, origin: 'camera');
  }

  @override
  Future<KycPicked?> file({required bool allowPdf}) async {
    fileCalls++;
    return KycPicked(bytes: bytes, name: name, mime: mime, origin: 'file');
  }
}

/// The page's own (vertical) scroll view.
Finder _page() => find.byWidgetPredicate((w) => w is Scrollable && w.axisDirection == AxisDirection.down).first;

/// The page-absolute top of every shown match of `f` (scroll offset + position on screen).
List<double> _tops(WidgetTester tester, Finder f) {
  final state = tester.state<ScrollableState>(_page());
  return [
    for (final e in f.evaluate())
      if (e.renderObject is RenderBox && (e.renderObject! as RenderBox).attached)
        state.position.pixels + (e.renderObject! as RenderBox).localToGlobal(Offset.zero).dy,
  ];
}

/// Scrolls down (from where the page is) until a match of `f` lies below `below`; its page-absolute top, or null.
Future<double?> _scrollTo(WidgetTester tester, Finder f, {double below = double.negativeInfinity}) async {
  final state = tester.state<ScrollableState>(_page());
  for (var i = 0; i < 80; i++) {
    final hits = _tops(tester, find.descendant(of: _page(), matching: f)).where((y) => y > below + 0.5).toList()..sort();
    if (hits.isNotEmpty) {
      // bring it on screen
      final target = (hits.first - 200).clamp(0.0, state.position.maxScrollExtent);
      if ((target - state.position.pixels).abs() > 1 && hits.first - state.position.pixels > 700) {
        state.position.jumpTo(target);
        await tester.pump(const Duration(milliseconds: 50));
      }
      return hits.first;
    }
    if (state.position.pixels >= state.position.maxScrollExtent) return null;
    state.position.jumpTo((state.position.pixels + 300).clamp(0.0, state.position.maxScrollExtent));
    await tester.pump(const Duration(milliseconds: 50));
  }
  return null;
}

/// Checks the texts come one under the other on the page (the web's phone order); a text that also appears higher up
/// (a row with the same words) is matched below the previous one.
Future<void> _expectOrder(WidgetTester tester, List<String> texts) async {
  final state = tester.state<ScrollableState>(_page());
  state.position.jumpTo(0);
  await tester.pump(const Duration(milliseconds: 50));
  var prev = double.negativeInfinity;
  for (var i = 0; i < texts.length; i++) {
    final y = await _scrollTo(tester, find.text(texts[i]), below: prev);
    expect(y, isNotNull, reason: '"${texts[i]}" should come after "${i == 0 ? 'the top' : texts[i - 1]}"');
    prev = y!;
  }
  state.position.jumpTo(0);
  await tester.pump(const Duration(milliseconds: 50));
}

Future<ProviderContainer> _open(WidgetTester tester, String location) async {
  final c = await pumpApp(tester, signedIn: true);
  c.read(routerProvider).go(location);
  await settle(tester);
  return c;
}

/// Taps a widget after scrolling the page to it.
Future<void> _tapIn(WidgetTester tester, Finder f) async {
  if (f.evaluate().isEmpty) await _scrollTo(tester, f);
  // to the middle of the screen (clear of the header and the tab bar)
  await tester.runAsync(() => Scrollable.ensureVisible(f.evaluate().first, alignment: 0.45));
  await tester.pump(const Duration(milliseconds: 50));
  await tester.tap(f);
  await settle(tester, frames: 6);
}

/// Pumps (with real time for image decoding) until `f` shows, at most ~5 s.
Future<void> _until(WidgetTester tester, Finder f) async {
  for (var i = 0; i < 50 && f.evaluate().isEmpty; i++) {
    await tester.runAsync(() => Future<void>.delayed(const Duration(milliseconds: 100)));
    await tester.pump(const Duration(milliseconds: 100));
  }
}

/// A KYC check row (label, two spaces, detail in one rich text).
Finder _checkRow(String label) => find.textContaining(RegExp('^${RegExp.escape(label)}  '), findRichText: true);

List<(String, String, Map<String, dynamic>)> _writes(String path) => PreviewProfile.calls.where((c) => c.$2 == path).toList();

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final t = _en();

  group('KYC instant checks (checks.ts)', () {
    test('a sharp, well-lit, framed document passes every check', () {
      final c = analyzeGray(_gray(400, 300, _checker), 2400, 1800, KycPurpose.id, origin: 'camera');
      expect(c.resolutionOk, isTrue);
      expect(c.resolutionMin, 600);
      expect(c.blurOk, isTrue);
      expect(c.brightnessOk, isTrue);
      expect(c.glareOk, isTrue);
      expect(c.fillOk, isTrue);
      expect(c.face, isNull);
      final rows = checkRows(c, KycPurpose.id, t);
      expect(rows.map((r) => r.key), ['resolution', 'blur', 'glare', 'brightness', 'fill']);
      expect(rows.every((r) => r.state == CheckState.ok), isTrue);
      expect(rows.first.detail, '2400 × 1800 px');
      final j = c.toJson();
      expect(j['source'], 'camera');
      expect((j['blur'] as Map)['ok'], true);
      expect(j.containsKey('mrz'), isFalse);
    });

    test('a flat (blurred) photo warns about sharpness and framing, a small one fails on resolution', () {
      final c = analyzeGray(_gray(200, 200, (_, _) => 128), 500, 400, KycPurpose.id, origin: 'file');
      expect(c.blurOk, isFalse);
      expect(c.fillOk, isFalse);
      expect(c.resolutionOk, isFalse);
      final rows = {for (final r in checkRows(c, KycPurpose.id, t)) r.key: r};
      expect(rows['blur']!.state, CheckState.warn);
      expect(rows['blur']!.detail, t('kyc.check.blurry'));
      expect(rows['fill']!.state, CheckState.warn);
      expect(rows['resolution']!.state, CheckState.fail);
      expect(rows['resolution']!.detail, t('kyc.check.resolutionLow', {'px': 400}));
    });

    test('glare and lighting: white is over-exposed with glare, black is too dark', () {
      final white = analyzeGray(_gray(100, 100, (_, _) => 255), 1200, 900, KycPurpose.id, origin: 'camera');
      expect(white.glareOk, isFalse);
      expect(white.glarePct, 100);
      expect(white.brightnessOk, isFalse);
      final wr = {for (final r in checkRows(white, KycPurpose.id, t)) r.key: r};
      expect(wr['glare']!.detail, t('kyc.check.glareFound'));
      expect(wr['brightness']!.detail, t('kyc.check.overExposed'));
      final dark = analyzeGray(_gray(100, 100, (_, _) => 20), 1200, 900, KycPurpose.id, origin: 'camera');
      expect(dark.brightnessOk, isFalse);
      expect({for (final r in checkRows(dark, KycPurpose.id, t)) r.key: r}['brightness']!.detail, t('kyc.check.tooDark'));
      // paper documents (proof of address) are white by nature: no glare check
      final poa = analyzeGray(_gray(100, 100, (_, _) => 255), 1200, 900, KycPurpose.poa, origin: 'file');
      expect(poa.glareOk, isNull);
      expect(poa.toJson().containsKey('glare'), isFalse);
    });

    test('passport: the machine-readable lines are looked for (and missing on a blank page)', () {
      final c = analyzeGray(_gray(200, 140, (_, _) => 150), 1600, 1100, KycPurpose.id, passport: true, origin: 'camera');
      expect(c.mrzFound, isFalse);
      expect(c.mrzLines, 0);
      final rows = {for (final r in checkRows(c, KycPurpose.id, t, passport: true)) r.key: r};
      expect(rows['mrz']!.state, CheckState.warn);
      expect(rows['mrz']!.detail, t('kyc.check.mrzMissing'));
      // not a passport: no MRZ row
      expect(checkRows(c, KycPurpose.id, t).any((r) => r.key == 'mrz'), isFalse);
    });

    test('selfie: a textured face in the oval is found, a blank frame is not; selfies need 480 px', () {
      // dark background, a textured bright oval in the middle
      final face = analyzeGray(
        _gray(160, 200, (x, y) {
          final d = ((x - 80) / (160 * 0.26)) * ((x - 80) / (160 * 0.26)) + ((y - 100) / (200 * 0.36)) * ((y - 100) / (200 * 0.36));
          return d <= 1 ? _checker(x, y) * 0.9 + 20 : 30;
        }),
        720,
        960,
        KycPurpose.selfie,
        origin: 'camera',
      );
      expect(face.face!.found, isTrue);
      expect(face.face!.method, 'heuristic');
      expect(face.resolutionMin, 480);
      expect(face.fillOk, isNull);
      expect({for (final r in checkRows(face, KycPurpose.selfie, t)) r.key: r}['face']!.detail, t('kyc.check.faceInOval'));
      final blank = analyzeGray(_gray(160, 200, (_, _) => 128), 720, 960, KycPurpose.selfie, origin: 'camera');
      expect(blank.face!.found, isFalse);
      expect({for (final r in checkRows(blank, KycPurpose.selfie, t)) r.key: r}['face']!.state, CheckState.warn);
    });

    test('proof of address issue date: 92 days at most, never in the future', () {
      final now = DateTime(2026, 10, 8);
      expect(ageDays('2026-10-01', now), 7);
      expect(ageDays('2026-10-10', now), -2);
      final ok = ClientChecks(source: 'file')..withIssueDate('2026-09-01', now);
      expect(ok.issueOk, isTrue);
      expect(ok.issueAgeDays, 37);
      final old = ClientChecks(source: 'file')..withIssueDate('2026-06-01', now);
      expect(old.issueOk, isFalse);
      expect({for (final r in checkRows(old, KycPurpose.poa, t)) r.key: r}['issue_date']!.state, CheckState.fail);
      final future = ClientChecks(source: 'file')..withIssueDate('2026-11-01', now);
      expect(future.issueOk, isFalse);
    });

    test('skipped formats show one info row (PDF, HEIC, unknown image)', () {
      final c = ClientChecks(source: 'file')..skipped = kSkippedPdf;
      final rows = checkRows(c, KycPurpose.poa, t);
      expect(rows.single.state, CheckState.info);
      expect(rows.single.detail, t('kyc.check.skipped.pdf'));
      expect(c.toJson()['skipped'], kSkippedPdf);
    });

    test('analysis size: at most 800 px on the long side, at least 8', () {
      expect(analysisSize(4000, 3000), (800, 600));
      expect(analysisSize(600, 400), (600, 400));
      expect(analysisSize(4000, 20), (800, 8));
    });

    testWidgets('analyzeImage decodes real PNG bytes (dart:ui) and checks them', (tester) async {
      await tester.runAsync(() async {
        final sharp = await pngBytes(1200, 800);
        final c = (await analyzeImage(sharp, KycPurpose.id, origin: 'file'))!;
        expect(c.width, 1200);
        expect(c.height, 800);
        expect(c.resolutionOk, isTrue);
        expect(c.blurOk, isTrue);
        expect(c.brightnessOk, isTrue);
        final flat = (await analyzeImage(await pngBytes(500, 400, flat: true), KycPurpose.id, origin: 'file'))!;
        expect(flat.blurOk, isFalse);
        expect(flat.resolutionOk, isFalse);
        // not an image: null (checked by the team after upload)
        expect(await analyzeImage(Uint8List.fromList(utf8.encode('%PDF-1.7')), KycPurpose.poa, origin: 'file'), isNull);
      });
    });
  });

  group('KYC slots and the wizard rules (verification/api.ts)', () {
    test('slot keys and equality (web slotKey)', () {
      const a = KycSlot('id_document', 'front');
      expect(a.key, 'id_document:front:');
      expect(a, const KycSlot('id_document', 'front', ''));
      expect(a == const KycSlot('id_document', 'back'), isFalse);
      expect(const KycSlot('party_id', 'front', 'p1').key, 'party_id:front:p1');
      expect(<KycSlot>{a, KycSlot(a.kind, a.side, '')}.length, 1, reason: 'no party and an empty party are the same slot');
    });

    test('the mode: start without a case, wizard while a draft, tracker once sent, the requested documents on more_info', () {
      expect(kycModeOf(null), KycMode.loading);
      PreviewProfile.reset('none');
      expect(kycModeOf(_kyc()), KycMode.start);
      PreviewProfile.reset('draft');
      expect(kycModeOf(_kyc()), KycMode.wizard);
      PreviewProfile.reset('in_review');
      expect(kycModeOf(_kyc()), KycMode.tracker);
      PreviewProfile.reset('approved');
      final approved = _kyc();
      expect(kycModeOf(approved), KycMode.tracker);
      expect(approved.canStart, isFalse);
      expect(kycModeOf(approved, restart: true), KycMode.tracker);
      PreviewProfile.reset('more_info');
      final more = _kyc();
      expect(kycModeOf(more), KycMode.moreInfo);
      expect(more.editable, isTrue);
      expect(more.kase!.requested, [const KycSlot('proof_of_address', 'single')]);
      expect(more.required.where((r) => r.requested).single.label, 'Proof of address');
      PreviewProfile.reset('rejected');
      final rejected = _kyc();
      expect(rejected.canStart, isTrue);
      expect(kycModeOf(rejected, restart: true), KycMode.start);
      expect(rejected.kase!.decisionLabel, 'Document unreadable');
      PreviewProfile.reset('approved');
    });

    test('firstIncomplete walks details › id › proof of address › selfie › review; a passport needs one side', () {
      PreviewProfile.reset('none');
      var s = _kycPost('start', {'kind': 'individual'});
      expect(s.kase!.status, 'draft');
      expect(s.kase!.reference, startsWith('KYC-'));
      expect(firstIncomplete(s), 0);
      s = _kycPost('details', {
        'address': {'line1': '14 Marine Drive', 'line2': '', 'city': 'Mumbai', 'postcode': '400020', 'country': 'in'},
      });
      expect(s.kase!.address!.city, 'Mumbai');
      expect(firstIncomplete(s), 1);
      s = _kycPost('details', {'id_doc_type': 'passport'});
      expect(s.required.map((r) => r.slot.key), ['id_document:front:', 'proof_of_address:single:', 'selfie:single:']);
      expect(s.required.first.label, 'Passport photo page');
      PreviewProfile.uploadSlot = {'kind': 'id_document', 'side': 'front'};
      s = _kycPost('documents', {});
      expect(firstIncomplete(s), 2);
      expect(docFor(s, const KycSlot('id_document', 'front')), isNotNull);
      s = _kycPost('documents', {}); // the first missing slot: proof of address
      expect(firstIncomplete(s), 3);
      // submitting early is refused with the missing document
      final early = PreviewProfile.answer('POST', 'kyc/submit', {'confirm': true}, const {})!;
      expect(early.$1, 422);
      s = _kycPost('documents', {});
      expect(firstIncomplete(s), 4);
      s = _kycPost('submit', {'confirm': true});
      expect(s.kase!.status, 'submitted');
      expect(s.kycStatus, 'pending');
      expect(kycModeOf(s), KycMode.tracker);
      expect(s.timeline.last.kind, 'submitted');
      PreviewProfile.reset('approved');
    });

    test('a national ID needs front and back; changing the type supersedes the photos in', () {
      PreviewProfile.reset('draft');
      _kycPost('details', {
        'address': {'line1': 'a', 'line2': '', 'city': 'b', 'postcode': 'c', 'country': 'in'},
      });
      var s = _kycPost('details', {'id_doc_type': 'national_id'});
      expect(s.required.where((r) => r.slot.kind == 'id_document').map((r) => r.label), ['National ID card (front)', 'National ID card (back)']);
      s = _kycPost('documents', {});
      expect(firstIncomplete(s), 1, reason: 'the back is still missing');
      s = _kycPost('details', {'id_doc_type': 'passport'});
      expect(docFor(s, const KycSlot('id_document', 'front')), isNull);
      expect(firstIncomplete(s), 1);
      PreviewProfile.reset('approved');
    });

    test('docFor keeps the latest current document; rejected and superseded ones do not count', () {
      final s = KycState.fromJson({
        'documents': [
          {'id': 1, 'kind': 'selfie', 'side': 'single', 'status': 'rejected'},
          {'id': 2, 'kind': 'selfie', 'side': 'single', 'status': 'uploaded'},
          {'id': 3, 'kind': 'selfie', 'side': 'single', 'status': 'accepted'},
          {'id': 4, 'kind': 'selfie', 'side': 'single', 'status': 'superseded'},
        ],
      });
      expect(docFor(s, const KycSlot('selfie', 'single'))!.id, 3);
      expect(docFor(s, const KycSlot('proof_of_address', 'single')), isNull);
      expect(s.maxBytes, 10 * 1024 * 1024);
      expect(s.poaMaxAgeDays, 92);
    });

    test('the checking sequence flags what the client checks flagged (tracker.tsx submissionChecks)', () {
      PreviewProfile.reset('in_review');
      final good = _kyc();
      final steps = submissionChecks(good, t);
      expect(steps.first.key, 'received');
      expect(steps.last.key, 'send');
      expect(steps.map((s) => s.key), containsAll(['quality', 'resolution', 'glare', 'framing', 'poa', 'face']));
      expect(steps.where((s) => s.warn), isEmpty);
      expect(clientChecksFlagged(good.documents.first), isFalse);
      final blurred = KycDocument.fromJson({
        'id': 9,
        'kind': 'id_document',
        'side': 'front',
        'status': 'uploaded',
        'checks': {
          'client': {
            'blur': {'score': 12.0, 'ok': false},
            'brightness': {'mean': 140, 'ok': true},
          },
        },
      });
      expect(clientChecksFlagged(blurred), isTrue);
      final s = KycState.fromJson({
        'documents': [
          {'id': 9, 'kind': 'id_document', 'side': 'front', 'status': 'uploaded', 'checks': blurred.checks},
        ],
      });
      expect(submissionChecks(s, t).firstWhere((x) => x.key == 'quality').warn, isTrue);
      PreviewProfile.reset('approved');
    });
  });

  group('KYC upload: multipart/form-data like the web uploadDocument', () {
    test('the form: file + kind, side, party, issue_date, doc_type and the checks as JSON', () {
      final checks = ClientChecks(source: 'camera', width: 1200, height: 800, blurScore: 210, blurOk: true);
      final form = kycUploadForm(
        const KycSlot('party_id', 'back', 'p2'),
        Uint8List.fromList([1, 2, 3, 4]),
        name: 'id-back.jpg',
        mime: 'image/jpeg',
        checks: checks,
        issueDate: '2026-09-01',
        docType: 'utility_bill',
      );
      final fields = {for (final f in form.fields) f.key: f.value};
      expect(fields['kind'], 'party_id');
      expect(fields['side'], 'back');
      expect(fields['party'], 'p2');
      expect(fields['issue_date'], '2026-09-01');
      expect(fields['doc_type'], 'utility_bill');
      expect((jsonDecode(fields['checks']!) as Map)['blur'], {'score': 210.0, 'ok': true});
      final file = form.files.single;
      expect(file.key, 'file');
      expect(file.value.filename, 'id-back.jpg');
      expect(file.value.contentType.toString(), 'image/jpeg');
      expect(file.value.length, 4);
      // no party / dates / type: those fields are left out
      final plain = kycUploadForm(const KycSlot('selfie', 'single'), Uint8List(1), name: 's.jpg', mime: 'image/jpeg');
      expect(plain.fields.map((f) => f.key), ['kind', 'side']);
    });

    test('POST kyc/documents goes as multipart and answers the new state', () async {
      PreviewProfile.reset('draft');
      _kycPost('details', {
        'address': {'line1': 'a', 'line2': '', 'city': 'b', 'postcode': 'c', 'country': 'in'},
        'id_doc_type': 'passport',
      });
      final r = _Recorder();
      final s = await uploadKycDocument(
        _api(r),
        const KycSlot('id_document', 'front'),
        Uint8List.fromList(List.filled(64, 7)),
        name: 'passport.jpg',
        mime: 'image/jpeg',
        checks: ClientChecks(source: 'file', width: 1600, height: 1100),
      );
      final o = r.to('kyc/documents').single;
      expect(o.method, 'POST');
      expect(o.data, isA<FormData>());
      expect('${o.contentType ?? o.headers['content-type']}', contains('multipart/form-data'));
      expect({for (final f in (o.data as FormData).fields) f.key: f.value}['kind'], 'id_document');
      expect(docFor(s, const KycSlot('id_document', 'front')), isNotNull);
      expect(PreviewProfile.calls.last.$2, 'kyc/documents');
      PreviewProfile.reset('approved');
    });

    test('mime types from file names (the web file.type)', () {
      expect(kycMimeOf('scan.JPG'), 'image/jpeg');
      expect(kycMimeOf('bill.pdf'), 'application/pdf');
      expect(kycMimeOf('IMG_0001.HEIC'), 'image/heic');
      expect(kycMimeOf('x.bin'), 'application/octet-stream');
    });
  });

  group('view-only logins (lib/viewer.ts)', () {
    test('the viewer ID is kept lowercase, [a-z0-9._-] only, 32 characters at most', () {
      expect(cleanViewerId('Priya.Tax!@# 01'), 'priya.tax01');
      expect(cleanViewerId('a' * 40).length, 32);
    });

    test('form errors: a name, at least one section, a valid ID when given', () {
      expect(viewerFormErrors(label: '', username: '', sections: const []), {'label': 'security.form.errName', 'sections': 'security.form.errSections'});
      expect(viewerFormErrors(label: 'Tax', username: 'ab', sections: const ['accounts']), {'username': 'security.form.errUsername'});
      expect(viewerFormErrors(label: 'Tax', username: '', sections: const ['accounts']), isEmpty);
      expect(viewerFormErrors(label: 'Tax', username: 'priya.tax', sections: const ['accounts']), isEmpty);
      expect(viewerFormErrors(label: 'Tax', username: '.priya', sections: const ['accounts']), contains('username'));
    });

    test('the expiry is the end of the chosen day (local), none when empty', () {
      expect(viewerExpiry(''), isNull);
      final e = DateTime.parse(viewerExpiry('2026-12-31')!).toLocal();
      expect((e.year, e.month, e.day, e.hour, e.minute, e.second), (2026, 12, 31, 23, 59, 59));
    });

    test('create and edit bodies (the step-up token is added by the caller)', () {
      final d = ViewerDraft(label: '  Accountant  ', expires: '2026-12-31');
      expect(d.sections, ['accounts', 'history'], reason: 'the web form starts with these two');
      final body = d.createBody();
      expect(body['label'], 'Accountant');
      expect(body.containsKey('username'), isFalse);
      expect(body['accounts'], isEmpty);
      expect(body['expires_at'], viewerExpiry('2026-12-31'));
      d.username = 'acc.view';
      expect(d.createBody()['username'], 'acc.view');
      expect(d.patchBody().keys, ['label', 'accounts', 'sections', 'expires_at']);
      final v = ViewerLogin.fromJson({
        'id': 1,
        'label': 'L',
        'username': 'u.ser',
        'accounts': [10042817],
        'sections': ['wallet'],
        'expires_at': null,
      });
      final e = ViewerDraft.of(v);
      expect(e.accounts, ['10042817']);
      expect(e.expires, '');
    });

    test('sections, pages and activity read like the web', () {
      expect(kViewerSections.map((s) => s.$1), ['dashboard', 'accounts', 'history', 'wallet', 'partner']);
      expect(viewerSectionLabel(t, 'history'), isNotEmpty);
      expect(viewerPageName(t, '/'), t('security.page.dashboard'));
      expect(viewerPageName(t, '/accounts/10042817'), t('security.page.account', {'n': '10042817'}));
      expect(viewerPageName(t, '/portfolio/statements'), t('security.page.statements'));
      expect(viewerPageName(t, '/elsewhere'), '/elsewhere');
      expect(viewerActivityLabel(t, 'viewer.login'), t('security.activity.login'));
      expect(viewerActivityLabel(t, 'something.else'), 'something.else');
    });

    test('the preview refuses viewer writes without the emailed code', () {
      PreviewProfile.reset();
      final r = PreviewProfile.answer('POST', 'security/viewers', {
        'label': 'X',
        'sections': ['accounts'],
      }, const {})!;
      expect(r.$1, 403);
      expect(((r.$2 as Map)['error'] as Map)['code'], 'stepup_required');
      final p = PreviewProfile.answer('POST', 'security/viewers/11/password', const {}, const {})!;
      expect(p.$1, 403);
      final ok = PreviewProfile.answer('POST', 'security/viewers/11/password', {'stepup_token': 'x'}, const {})!;
      expect(ok.$1, 200);
      expect((ok.$2 as Map)['username'], 'priya.tax');
    });
  });

  group('devices from user agents (web parseDevice)', () {
    test('browser, OS and form factor', () {
      Device d(String ua) => parseDevice(ua, t);
      final chromeWin = d('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36');
      expect((chromeWin.browser, chromeWin.os, chromeWin.kind), ('Chrome', 'Windows', DeviceKind.desktop));
      final iphone = d(
        'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1',
      );
      expect((iphone.browser, iphone.os, iphone.kind), ('Safari', 'iOS', DeviceKind.mobile));
      final ipad = d('Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/129.0 Mobile/15E148 Safari/604.1');
      expect((ipad.browser, ipad.os, ipad.kind), ('Chrome', 'iPadOS', DeviceKind.tablet));
      final android = d('Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Mobile Safari/537.36');
      expect((android.browser, android.os, android.kind), ('Chrome', 'Android', DeviceKind.mobile));
      final tab = d('Mozilla/5.0 (Linux; Android 14; SM-X710) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36');
      expect(tab.kind, DeviceKind.tablet);
      final edge = d('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0 Safari/537.36 Edg/129.0');
      expect((edge.browser, edge.os, edge.kind), ('Edge', 'macOS', DeviceKind.desktop));
      expect(d('Mozilla/5.0 (X11; Linux x86_64; rv:131.0) Gecko/20100101 Firefox/131.0').browser, 'Firefox');
      expect(d('SamsungBrowser/26.0 (Linux; Android 14) Mobile').browser, 'Samsung Internet');
      expect(d('Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 Chrome/129.0 Safari/537.36').os, 'ChromeOS');
      expect(d('curl/8.4.0').browser, t('security.device.apiClient'));
      final none = parseDevice(null, t);
      expect((none.browser, none.os, none.kind), (t('security.device.unknownBrowser'), t('security.device.unknownDevice'), DeviceKind.unknown));
      expect(deviceIcon(DeviceKind.mobile), isNotNull);
    });

    test('time texts: ago and idle', () {
      final now = DateTime(2026, 10, 8, 12);
      expect(secAgo(t, now.subtract(const Duration(seconds: 30)), now), t('security.ago.now'));
      expect(secAgo(t, now.subtract(const Duration(minutes: 12)), now), t('security.ago.min', {'n': 12}));
      expect(secAgo(t, now.subtract(const Duration(hours: 3)), now), t('security.ago.hours', {'n': 3}));
      expect(secAgo(t, now.subtract(const Duration(days: 2)), now), t('security.ago.days', {'n': 2}));
      expect(secAgo(t, null), '—');
      expect(idleLabel(1440, t), t('security.idle.days', {'count': 1}));
      expect(idleLabel(120, t), t('security.idle.hours', {'count': 2}));
      expect(idleLabel(45, t), t('security.idle.minutes', {'count': 45}));
    });

    test('sign-in results and request statuses have the web tones', () {
      expect(kLoginResultTone['failed'], isNotNull);
      expect(loginResultLabel(t, 'unknown_thing', 'Kalks'), 'unknown_thing');
      expect(kRequestStatus.keys, ['open', 'in_progress', 'completed', 'rejected', 'cancelled']);
      expect(ClientRequest.fromJson({'id': 1, 'kind': 'closure', 'status': 'in_progress'}).pending, isTrue);
    });
  });

  group('password and notifications', () {
    test('the gateway password rules, in order', () {
      expect(passwordProblem('short'), 'profile.password.rule.min');
      expect(passwordProblem('alllowercase1!'), 'profile.password.rule.upper');
      expect(passwordProblem('ALLUPPERCASE1!'), 'profile.password.rule.lower');
      expect(passwordProblem('NoNumbers!!'), 'profile.password.rule.number');
      expect(passwordProblem('NoSymbol123'), 'profile.password.rule.symbol');
      expect(passwordProblem('Str0ng!Pass'), isNull);
      expect(passwordFormReady(current: 'Old!Pass1', next: 'Str0ng!Pass', repeat: 'Str0ng!Pass'), isTrue);
      expect(passwordFormReady(current: 'Str0ng!Pass', next: 'Str0ng!Pass', repeat: 'Str0ng!Pass'), isFalse);
      expect(passwordFormReady(current: 'Old!Pass1', next: 'Str0ng!Pass', repeat: 'Str0ng!Pas'), isFalse);
    });

    test('notification prefs: missing topics read as in-app on, email off', () {
      final p = parseNotifPrefs({
        'wallet': {'inApp': false, 'email': true},
        'kyc': {'email': false},
        'bad': 3,
      });
      expect(p['wallet'], (inApp: false, email: true));
      expect(p['kyc'], (inApp: true, email: false));
      expect(p.containsKey('bad'), isFalse);
    });
  });

  group('pages on the sample-data API', () {
    setUp(() => PreviewProfile.reset('approved'));

    testWidgets('Profile: identity, personal information, password, sign-in, verification and notifications in the web order', (tester) async {
      await _open(tester, '/profile');
      expect(find.text(t('profile.title')), findsWidgets);
      await _expectOrder(tester, [
        t('profile.personal.title'),
        t('profile.password.title'),
        t('profile.signin.title'),
        t('profile.kycCard.title'),
        t('profile.notifCard.title'),
      ]);
      await unmount(tester);
    });

    testWidgets('Security: protection, password, sessions, history, data requests in the web order', (tester) async {
      await _open(tester, '/profile/security');
      expect(find.text(t('security.page.title')), findsWidgets);
      expect(find.text(t('security.viewerBar.title')), findsWidgets);
      await _expectOrder(tester, [
        t('security.protect.title'),
        t('profile.password.title'),
        t('security.sessions.title'),
        t('security.history.title'),
        t('security.requests.title'),
      ]);
      // the sessions from security/sessions: this device first, the viewer's session marked
      final sessions = await _scrollTo(tester, find.text(t('security.sessions.title')));
      expect(await _scrollTo(tester, find.text(t('security.sessions.thisDevice')), below: sessions!), isNotNull);
      expect(await _scrollTo(tester, find.text('Accountant (Priya)'), below: sessions), isNotNull);
      await unmount(tester);
    });

    testWidgets('View-only access: logins, activity, investor passwords in the web order', (tester) async {
      await _open(tester, '/profile/viewers');
      expect(find.text(t('security.viewers.new')), findsWidgets);
      await _expectOrder(tester, [t('security.viewers.title'), t('security.activity.title'), t('security.investor.title')]);
      expect(find.text('priya.tax'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('Verification (approved): the tracker, then the levels', (tester) async {
      await _open(tester, '/profile/verification');
      expect(find.byKey(const ValueKey('kyc-main-tracker')), findsOneWidget);
      expect(find.text('KYC-001288'), findsWidgets);
      await _expectOrder(tester, [t('kyc.tracker.approved.title'), t('kyc.levels.title')]);
      await unmount(tester);
    });

    testWidgets('Notifications and Preferences: their cards in the web order', (tester) async {
      final c = await _open(tester, '/profile/notifications');
      await _expectOrder(tester, [t('profile.notifications.channels'), 'Security', t.dyn('profile.notifications.cat.marketing', fallback: 'News and offers')]);
      c.read(routerProvider).go('/profile/preferences');
      await settle(tester);
      // dark only since 2026-10-10: no Appearance card
      expect(find.text(t('profile.prefs.appearance')), findsNothing);
      await _expectOrder(tester, [t('common.language'), t('profile.notifCard.title')]);
      await unmount(tester);
    });

    testWidgets('change password: the emailed code first, then one write with the step-up token', (tester) async {
      await _open(tester, '/profile/security');
      final page = _page();
      final fields = find.descendant(of: page, matching: find.byType(TextField));
      await _scrollTo(tester, find.text(t('profile.password.title')));
      await tester.enterText(fields.at(0), 'Old!Pass1');
      await tester.enterText(fields.at(1), 'Str0ng!Pass');
      await tester.enterText(fields.at(2), 'Str0ng!Pass');
      await tester.pump();
      // the server refuses the change without a code (the gateway's stepup_required)
      expect(PreviewProfile.answer('POST', 'auth/password', {'current': 'Old!Pass1', 'new': 'Str0ng!Pass'}, const {})!.$1, 403);
      PreviewProfile.calls.clear();
      final submit = find.widgetWithText(KButton, t('profile.password.submit'));
      await _tapIn(tester, submit);
      // the code sheet is open and nothing was written yet
      expect(find.text(t('profile.password.stepupConfirm')), findsOneWidget);
      expect(_writes('auth/password'), isEmpty);
      await tester.enterText(find.byType(TextField).last, '123456');
      await settle(tester);
      final w = _writes('auth/password');
      expect(w.length, 1);
      expect(w.single.$3['stepup_token'], 'preview-stepup');
      expect(w.single.$3['current'], 'Old!Pass1');
      expect(w.single.$3['new'], 'Str0ng!Pass');
      expect(w.single.$3['sign_out_others'], true);
      expect(find.text(t('profile.password.changed')), findsWidgets);
      await unmount(tester);
    });

    testWidgets('new viewer: the form, the emailed code, then the sign-in details shown once', (tester) async {
      await _open(tester, '/profile/viewers');
      await tester.tap(find.widgetWithText(KButton, t('security.viewers.new')).first);
      await settle(tester, frames: 6);
      await tester.enterText(find.widgetWithText(KTextField, t('security.form.name')), 'Tax adviser');
      await tester.pump();
      await tester.tap(find.widgetWithText(KButton, t('common.continue')));
      await settle(tester, frames: 6);
      expect(find.text(t('security.stepup.createConfirm')), findsOneWidget);
      expect(_writes('security/viewers'), isEmpty);
      await tester.enterText(find.byType(TextField).last, '123456');
      await settle(tester);
      final w = _writes('security/viewers');
      expect(w.length, 1);
      expect(w.single.$3['stepup_token'], 'preview-stepup');
      expect(w.single.$3['label'], 'Tax adviser');
      expect(w.single.$3['sections'], ['accounts', 'history']);
      expect(find.text(t('security.creds.title')), findsWidgets);
      expect(find.text('Kx7-mPq4-Wz9r'), findsWidgets);
      await tester.tap(find.text(t('security.creds.saved')));
      await settle(tester, frames: 6);
      expect(find.text('Tax adviser'), findsWidgets);
      await unmount(tester);
    });

    testWidgets('KYC: a selfie from the injected camera is checked, uploaded, then the case is sent', (tester) async {
      // a started case with the address, a passport and the proof of address in: the wizard opens at the selfie
      PreviewProfile.reset('draft');
      _kycPost('details', {
        'address': {'line1': '14 Marine Drive', 'line2': '', 'city': 'Mumbai', 'postcode': '400020', 'country': 'in'},
        'id_doc_type': 'passport',
      });
      _kycPost('documents', {});
      _kycPost('documents', {});
      PreviewProfile.calls.clear();
      final bytes = (await tester.runAsync(() => pngBytes(960, 1280)))!;
      final picker = _FakePicker(bytes, name: 'selfie.png');
      final c = await pumpApp(tester, signedIn: true);
      c.read(kycPickerProvider.notifier).use(picker);
      c.read(routerProvider).go('/profile/verification');
      await settle(tester);
      expect(find.byKey(const ValueKey('kyc-main-wizard')), findsOneWidget);
      expect(find.text(t('kyc.selfie.title')), findsWidgets);

      await _tapIn(tester, find.byKey(const ValueKey('camera-selfie-single-')));
      expect(picker.cameraCalls, 1);
      await _until(tester, _checkRow(t('kyc.check.resolution')));
      // the instant checks are shown before anything is sent
      expect(_checkRow(t('kyc.check.resolution')), findsOneWidget);
      expect(_checkRow(t('kyc.check.sharpness')), findsOneWidget);
      expect(_checkRow(t('kyc.check.face')), findsOneWidget);
      expect(_writes('kyc/documents'), isEmpty);

      await _tapIn(tester, find.byKey(const ValueKey('use-selfie-single-')));
      await settle(tester);
      expect(_writes('kyc/documents').length, 1);
      expect(find.text(t('kyc.slot.received')), findsOneWidget);

      await _tapIn(tester, find.byKey(const ValueKey('step-next')));
      expect(find.text(t('kyc.review.title')), findsWidgets);
      await _tapIn(tester, find.text(t('kyc.review.consent')));
      await _tapIn(tester, find.byKey(const ValueKey('step-next')));
      for (var i = 0; i < 12; i++) {
        await tester.pump(const Duration(seconds: 1));
      }
      await settle(tester);
      final submit = _writes('kyc/submit');
      expect(submit.length, 1);
      expect(submit.single.$3, {'confirm': true});
      expect(find.byKey(const ValueKey('kyc-main-tracker')), findsOneWidget);
      await unmount(tester);
    });

    testWidgets('notifications: a switch saves that topic and channel; marketing emails follow the consent', (tester) async {
      await _open(tester, '/profile/notifications');
      final wallet = find.descendant(of: find.byKey(const ValueKey('pref-wallet')), matching: find.byType(KSwitch));
      expect(wallet, findsNWidgets(2));
      await _tapIn(tester, wallet.last);
      final w = _writes('notifications/prefs');
      expect(w.length, 1);
      expect(w.single.$1, 'PUT');
      expect(w.single.$3, {
        'prefs': {
          'wallet': {'email': false},
        },
      });
      expect(find.text(t('profile.notifications.saved')), findsWidgets);
      // the locked topic has no switches
      expect(find.descendant(of: find.byKey(const ValueKey('pref-security')), matching: find.byType(KSwitch)), findsNothing);
      final marketing = find.descendant(of: find.byKey(const ValueKey('pref-marketing')), matching: find.byType(KSwitch));
      await _tapIn(tester, marketing.last);
      final m = _writes('auth/marketing');
      expect(m.single.$1, 'PUT');
      expect(m.single.$3, {'consent': false});
      await unmount(tester);
    });

    testWidgets('preferences: dark only (no theme switch); the language switches at once', (tester) async {
      final c = await _open(tester, '/profile/preferences');
      expect(c.read(themeModeProvider), ThemeMode.dark);
      expect(find.text(t('profile.prefs.dark')), findsNothing);
      expect(find.text(t('profile.prefs.light')), findsNothing);
      await _tapIn(tester, find.byKey(const ValueKey('lang-de')));
      await settle(tester);
      expect(c.read(localeProvider), 'de');
      final de = (jsonDecode(File('assets/i18n/de.json').readAsStringSync()) as Map)['profile.prefs.title'] as String;
      expect(find.text(de), findsWidgets);
      await unmount(tester);
    });
  });
}

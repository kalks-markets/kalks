// Academy data: the web's components/academy/live/api.ts on the mobile API (`/api/mobile/academy/*` = the web's
// /api/academy/* BFF, same answers). Quizzes and exams are graded by the service; answers never reach the app before
// it submits.
//   GET  academy/catalog                    phases > sections > chapters with my progress, exam state, certificates
//   GET  academy/chapters/{slug}            chapter (markdown body, takeaways, quiz without answers), prev / next
//   POST academy/chapters/{slug}/progress   {read_pct}
//   POST academy/chapters/{slug}/quiz       {answers: (int|null)[]}  instant feedback; recorded when all answered
//   GET  academy/exams/{phase}              final exam (no answers) + unlocked + attempts + certificate
//   POST academy/exams/{phase}              {answers: int[]}  a pass issues the phase certificate
//   GET  academy/me/certificates
//   GET  academy/glossary
//   GET  academy/certificates/{code}/image  the certificate (SVG, public)
import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/api/api_providers.dart';
import '../../core/files.dart';
import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../ui/ui.dart';

/* ------------------------------------------------------------------ parsing helpers */

Map<String, dynamic> _map(Object? v) => v is Map ? v.cast<String, dynamic>() : const <String, dynamic>{};
List<Map<String, dynamic>> _list(Object? v) => v is List ? [for (final x in v) _map(x)] : const [];
int _int(Object? v) => (v as num?)?.round() ?? 0;
int? _intOrNull(Object? v) => (v as num?)?.round();
String _str(Object? v) => v == null ? '' : '$v';
DateTime? _date(Object? v) => v is String ? DateTime.tryParse(v) : null;

/* ------------------------------------------------------------------ models */

class ChapterProgress {
  const ChapterProgress({this.readPct = 0, this.quizBest, this.quizTotal, this.completed = false, this.completedAt});
  final int readPct;
  final int? quizBest;
  final int? quizTotal;
  final bool completed;
  final DateTime? completedAt;

  factory ChapterProgress.fromJson(Map<String, dynamic> j) => ChapterProgress(
    readPct: _int(j['read_pct']),
    quizBest: _intOrNull(j['quiz_best']),
    quizTotal: _intOrNull(j['quiz_total']),
    completed: j['completed'] == true,
    completedAt: _date(j['completed_at']),
  );
}

class ChapterCard {
  const ChapterCard({
    required this.slug,
    required this.title,
    required this.summary,
    required this.minutes,
    required this.order,
    required this.questions,
    required this.progress,
  });
  final String slug, title, summary;
  final int minutes, order, questions;
  final ChapterProgress progress;

  factory ChapterCard.fromJson(Map<String, dynamic> j) => ChapterCard(
    slug: _str(j['slug']),
    title: _str(j['title']),
    summary: _str(j['summary']),
    minutes: _int(j['minutes']),
    order: _int(j['order']),
    questions: _int(j['questions']),
    progress: ChapterProgress.fromJson(_map(j['progress'])),
  );
}

class AcademySection {
  const AcademySection({required this.slug, required this.track, required this.title, required this.summary, required this.chapters});
  final String slug, track, title, summary;
  final List<ChapterCard> chapters;

  int get done => chapters.where((c) => c.progress.completed).length;

  factory AcademySection.fromJson(Map<String, dynamic> j) => AcademySection(
    slug: _str(j['slug']),
    track: _str(j['track']),
    title: _str(j['title']),
    summary: _str(j['summary']),
    chapters: [for (final c in _list(j['chapters'])) ChapterCard.fromJson(c)],
  );
}

class ExamState {
  const ExamState({required this.questions, required this.passMark, required this.unlocked, this.bestPct, required this.passed, required this.attempts});
  final int questions, passMark;
  final bool unlocked;
  final int? bestPct;
  final bool passed;
  final int attempts;

  factory ExamState.fromJson(Map<String, dynamic> j) => ExamState(
    questions: _int(j['questions']),
    passMark: _int(j['pass_mark']),
    unlocked: j['unlocked'] == true,
    bestPct: _intOrNull(j['best_pct']),
    passed: j['passed'] == true,
    attempts: _int(j['attempts']),
  );
}

class CertRef {
  const CertRef({required this.code, required this.issuedAt, this.scorePct});
  final String code;
  final DateTime? issuedAt;
  final int? scorePct;

  factory CertRef.fromJson(Map<String, dynamic> j) => CertRef(code: _str(j['code']), issuedAt: _date(j['issued_at']), scorePct: _intOrNull(j['score_pct']));
}

class AcademyPhase {
  const AcademyPhase({
    required this.slug,
    required this.order,
    required this.title,
    required this.level,
    required this.summary,
    required this.elective,
    required this.minutes,
    required this.done,
    required this.total,
    required this.sections,
    this.exam,
    this.certificate,
  });
  final String slug, title, level, summary;
  final int order, minutes, done, total;

  /// Product (elective) phase, e.g. phase 9 "Kalks FX Options".
  final bool elective;
  final List<AcademySection> sections;
  final ExamState? exam;
  final CertRef? certificate;

  bool get allDone => done == total && total > 0;

  factory AcademyPhase.fromJson(Map<String, dynamic> j) {
    final p = _map(j['progress']);
    return AcademyPhase(
      slug: _str(j['slug']),
      order: _int(j['order']),
      title: _str(j['title']),
      level: _str(j['level']),
      summary: _str(j['summary']),
      elective: j['elective'] == true,
      minutes: _int(j['minutes']),
      done: _int(p['done']),
      total: _int(p['total']),
      sections: [for (final s in _list(j['sections'])) AcademySection.fromJson(s)],
      exam: j['exam'] is Map ? ExamState.fromJson(_map(j['exam'])) : null,
      certificate: j['certificate'] is Map ? CertRef.fromJson(_map(j['certificate'])) : null,
    );
  }
}

/// Where to continue (`me.continue`).
class ContinueRef {
  const ContinueRef({
    required this.slug,
    required this.title,
    required this.minutes,
    required this.readPct,
    required this.started,
    required this.phaseSlug,
    required this.phaseOrder,
    required this.phaseTitle,
  });
  final String slug, title, phaseSlug, phaseTitle;
  final int minutes, readPct, phaseOrder;
  final bool started;

  factory ContinueRef.fromJson(Map<String, dynamic> j) {
    final p = _map(j['phase']);
    return ContinueRef(
      slug: _str(j['slug']),
      title: _str(j['title']),
      minutes: _int(j['minutes']),
      readPct: _int(j['read_pct']),
      started: j['started'] == true,
      phaseSlug: _str(p['slug']),
      phaseOrder: _int(p['order']),
      phaseTitle: _str(p['title']),
    );
  }
}

class AcademyMe {
  const AcademyMe({
    required this.chaptersDone,
    required this.chaptersTotal,
    required this.minutesDone,
    required this.minutesTotal,
    this.quizAvg,
    required this.certificates,
    required this.streak,
    required this.activeDays,
    this.next,
  });
  final int chaptersDone, chaptersTotal, minutesDone, minutesTotal;
  final int? quizAvg;
  final int certificates, streak;

  /// UTC days (yyyy-mm-dd) of the last week with study activity.
  final List<String> activeDays;

  /// `continue` (null when every chapter is complete).
  final ContinueRef? next;

  factory AcademyMe.fromJson(Map<String, dynamic> j) => AcademyMe(
    chaptersDone: _int(j['chapters_done']),
    chaptersTotal: _int(j['chapters_total']),
    minutesDone: _int(j['minutes_done']),
    minutesTotal: _int(j['minutes_total']),
    quizAvg: _intOrNull(j['quiz_avg']),
    certificates: _int(j['certificates']),
    streak: _int(j['streak']),
    activeDays: [for (final d in (j['active_days'] as List? ?? const [])) '$d'],
    next: j['continue'] is Map ? ContinueRef.fromJson(_map(j['continue'])) : null,
  );
}

class AcademyCatalog {
  const AcademyCatalog({required this.phases, required this.me});
  final List<AcademyPhase> phases;
  final AcademyMe me;

  factory AcademyCatalog.fromJson(Map<String, dynamic> j) =>
      AcademyCatalog(phases: [for (final p in _list(j['phases'])) AcademyPhase.fromJson(p)], me: AcademyMe.fromJson(_map(j['me'])));
}

class Question {
  const Question({required this.question, required this.options});
  final String question;
  final List<String> options;

  factory Question.fromJson(Map<String, dynamic> j) =>
      Question(question: _str(j['question']), options: [for (final o in (j['options'] as List? ?? const [])) '$o']);
}

class Practice {
  const Practice({required this.label, this.symbol});
  final String label;
  final String? symbol;
}

class PhaseRef {
  const PhaseRef({required this.slug, required this.order, required this.title, required this.level, required this.elective});
  final String slug, title, level;
  final int order;
  final bool elective;

  factory PhaseRef.fromJson(Map<String, dynamic> j) =>
      PhaseRef(slug: _str(j['slug']), order: _int(j['order']), title: _str(j['title']), level: _str(j['level']), elective: j['elective'] == true);
}

class ChapterView {
  const ChapterView({
    required this.slug,
    required this.title,
    required this.summary,
    required this.body,
    required this.takeaways,
    this.practice,
    required this.minutes,
    required this.quiz,
    required this.phase,
    required this.track,
    required this.sectionTitle,
    required this.index,
    required this.count,
    this.prev,
    this.next,
    required this.progress,
  });
  final String slug, title, summary, body;
  final List<String> takeaways;
  final Practice? practice;
  final int minutes;
  final List<Question> quiz;
  final PhaseRef phase;
  final String track, sectionTitle;

  /// Chapter n of count in its section.
  final int index, count;
  final ({String slug, String title})? prev, next;
  final ChapterProgress progress;

  factory ChapterView.fromJson(Map<String, dynamic> j) {
    final c = _map(j['chapter']);
    final s = _map(j['section']);
    final pr = c['practice'];
    ({String slug, String title})? nav(Object? v) => v is Map ? (slug: _str(v['slug']), title: _str(v['title'])) : null;
    return ChapterView(
      slug: _str(c['slug']),
      title: _str(c['title']),
      summary: _str(c['summary']),
      body: _str(c['body']),
      takeaways: [for (final x in (c['takeaways'] as List? ?? const [])) '$x'],
      practice: pr is Map && _str(pr['label']).isNotEmpty ? Practice(label: _str(pr['label']), symbol: pr['symbol'] == null ? null : _str(pr['symbol'])) : null,
      minutes: _int(c['minutes']),
      quiz: [for (final q in _list(c['quiz'])) Question.fromJson(q)],
      phase: PhaseRef.fromJson(_map(j['phase'])),
      track: _str(s['track']),
      sectionTitle: _str(s['title']),
      index: _int(s['index']),
      count: _int(s['count']),
      prev: nav(j['prev']),
      next: nav(j['next']),
      progress: ChapterProgress.fromJson(_map(j['progress'])),
    );
  }
}

class QuizResult {
  const QuizResult({required this.index, required this.choice, required this.correct, required this.answer, required this.explanation});
  final int index, choice, answer;
  final bool correct;
  final String explanation;

  factory QuizResult.fromJson(Map<String, dynamic> j) => QuizResult(
    index: _int(j['index']),
    choice: _int(j['choice']),
    correct: j['correct'] == true,
    answer: _int(j['answer']),
    explanation: _str(j['explanation']),
  );
}

class QuizReply {
  const QuizReply({
    required this.results,
    required this.score,
    required this.total,
    required this.allAnswered,
    required this.passed,
    required this.passPct,
    required this.completed,
    required this.completedNow,
    required this.phaseDone,
    required this.phaseTotal,
    required this.examUnlocked,
  });
  final List<QuizResult> results;
  final int score, total, passPct, phaseDone, phaseTotal;
  final bool allAnswered, passed, completed, completedNow, examUnlocked;

  factory QuizReply.fromJson(Map<String, dynamic> j) {
    final p = _map(j['phase']);
    return QuizReply(
      results: [for (final r in _list(j['results'])) QuizResult.fromJson(r)],
      score: _int(j['score']),
      total: _int(j['total']),
      allAnswered: j['all_answered'] == true,
      passed: j['passed'] == true,
      passPct: _int(j['pass_pct']),
      completed: j['completed'] == true,
      completedNow: j['completed_now'] == true,
      phaseDone: _int(p['done']),
      phaseTotal: _int(p['total']),
      examUnlocked: p['exam_unlocked'] == true,
    );
  }
}

class ExamView {
  const ExamView({
    required this.phase,
    required this.passMark,
    required this.questions,
    required this.unlocked,
    required this.chaptersDone,
    required this.chaptersTotal,
    required this.attempts,
    this.certificate,
  });
  final PhaseRef phase;
  final int passMark;
  final List<Question> questions;
  final bool unlocked;
  final int chaptersDone, chaptersTotal;
  final List<({int pct, bool passed, DateTime? at})> attempts;
  final CertRef? certificate;

  factory ExamView.fromJson(Map<String, dynamic> j) {
    final e = _map(j['exam']);
    return ExamView(
      phase: PhaseRef.fromJson(_map(j['phase'])),
      passMark: _int(e['pass_mark']),
      questions: [for (final q in _list(e['questions'])) Question.fromJson(q)],
      unlocked: j['unlocked'] == true,
      chaptersDone: _int(j['chapters_done']),
      chaptersTotal: _int(j['chapters_total']),
      attempts: [for (final a in _list(j['attempts'])) (pct: _int(a['pct']), passed: a['passed'] == true, at: _date(a['at']))],
      certificate: j['certificate'] is Map ? CertRef.fromJson(_map(j['certificate'])) : null,
    );
  }
}

class ExamReply {
  const ExamReply({
    required this.score,
    required this.total,
    required this.pct,
    required this.passMark,
    required this.passed,
    required this.results,
    this.certificate,
    required this.certificateIssued,
  });
  final int score, total, pct, passMark;
  final bool passed, certificateIssued;
  final List<QuizResult> results;
  final CertRef? certificate;

  factory ExamReply.fromJson(Map<String, dynamic> j) => ExamReply(
    score: _int(j['score']),
    total: _int(j['total']),
    pct: _int(j['pct']),
    passMark: _int(j['pass_mark']),
    passed: j['passed'] == true,
    results: [for (final r in _list(j['results'])) QuizResult.fromJson(r)],
    certificate: j['certificate'] is Map ? CertRef.fromJson(_map(j['certificate'])) : null,
    certificateIssued: j['certificate_issued'] == true,
  );
}

class AcademyCertificate {
  const AcademyCertificate({
    required this.code,
    required this.phase,
    required this.phaseOrder,
    required this.phaseTitle,
    required this.level,
    required this.scorePct,
    required this.issuedAt,
    required this.learnerName,
    required this.verifyUrl,
  });
  final String code, phase, phaseTitle, level, learnerName, verifyUrl;
  final int phaseOrder, scorePct;
  final DateTime? issuedAt;

  factory AcademyCertificate.fromJson(Map<String, dynamic> j) => AcademyCertificate(
    code: _str(j['code']),
    phase: _str(j['phase']),
    phaseOrder: _int(j['phase_order']),
    phaseTitle: _str(j['phase_title']),
    level: _str(j['level']),
    scorePct: _int(j['score_pct']),
    issuedAt: _date(j['issued_at']),
    learnerName: _str(j['learner_name']),
    verifyUrl: _str(j['verify_url']),
  );
}

class GlossaryTerm {
  const GlossaryTerm({required this.slug, required this.term, required this.category, required this.definition, required this.related});
  final String slug, term, category, definition;
  final List<({String slug, String term})> related;

  /// The term's first letter, upper case (the A–Z index).
  String get letter => term.isEmpty ? '' : term.characters.first.toUpperCase();

  factory GlossaryTerm.fromJson(Map<String, dynamic> j) => GlossaryTerm(
    slug: _str(j['slug']),
    term: _str(j['term']),
    category: _str(j['category']),
    definition: _str(j['definition']),
    related: [for (final r in _list(j['related'])) (slug: _str(r['slug']), term: _str(r['term']))],
  );
}

class AcademyGlossary {
  const AcademyGlossary({required this.terms, required this.categories, required this.total});
  final List<GlossaryTerm> terms;
  final List<({String name, int count})> categories;
  final int total;

  factory AcademyGlossary.fromJson(Map<String, dynamic> j) => AcademyGlossary(
    terms: [for (final t in _list(j['terms'])) GlossaryTerm.fromJson(t)],
    categories: [for (final c in _list(j['categories'])) (name: _str(c['name']), count: _int(c['count']))],
    total: _int(j['total']),
  );
}

/* ------------------------------------------------------------------ providers (web useAcademy: loads once, again on reload) */

final academyCatalogProvider = FutureProvider.autoDispose<AcademyCatalog>((ref) async {
  return AcademyCatalog.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('academy/catalog'));
});

final academyChapterProvider = FutureProvider.autoDispose.family<ChapterView, String>((ref, slug) async {
  return ChapterView.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('academy/chapters/$slug'));
});

final academyExamProvider = FutureProvider.autoDispose.family<ExamView, String>((ref, phase) async {
  return ExamView.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('academy/exams/$phase'));
});

final academyCertificatesProvider = FutureProvider.autoDispose<List<AcademyCertificate>>((ref) async {
  final j = await ref.watch(apiProvider).get<Map<String, dynamic>>('academy/me/certificates');
  return [for (final c in _list(j['certificates'])) AcademyCertificate.fromJson(c)];
});

final academyGlossaryProvider = FutureProvider.autoDispose<AcademyGlossary>((ref) async {
  return AcademyGlossary.fromJson(await ref.watch(apiProvider).get<Map<String, dynamic>>('academy/glossary'));
});

/// The certificate image (public SVG, `academy/certificates/{code}/image`), kept while the app runs.
final certificateSvgProvider = FutureProvider.family<String, String>((ref, code) async {
  final f = await ref.watch(apiProvider).download('academy/certificates/$code/image');
  return _svgText(f.bytes);
});

/// The SVG text of an image answer (a JSON-encoded string in the sample-data previews).
String _svgText(Uint8List bytes) {
  final s = utf8.decode(bytes, allowMalformed: true).trim();
  if (s.startsWith('"')) {
    final v = jsonDecode(s);
    if (v is String) return v;
  }
  return s;
}

/* ------------------------------------------------------------------ writes */

class AcademyApi {
  AcademyApi(this.api);
  final ApiClient api;

  /// Reading progress (max so far, 0–100). Best effort: failures are ignored, as on the web.
  Future<void> progress(String slug, int readPct) async {
    try {
      await api.post<Map<String, dynamic>>('academy/chapters/$slug/progress', body: {'read_pct': readPct.clamp(0, 100)});
    } catch (_) {}
  }

  Future<QuizReply> quiz(String slug, List<int?> answers) async =>
      QuizReply.fromJson(await api.post<Map<String, dynamic>>('academy/chapters/$slug/quiz', body: {'answers': answers}));

  Future<ExamReply> exam(String phase, List<int?> answers) async =>
      ExamReply.fromJson(await api.post<Map<String, dynamic>>('academy/exams/$phase', body: {'answers': answers}));

  /// The certificate as a file (`?download=1`), for the share sheet.
  Future<DownloadedFile> certificateFile(String code) async {
    final f = await api.download('academy/certificates/$code/image', query: {'download': '1'});
    final svg = _svgText(f.bytes);
    return (bytes: Uint8List.fromList(utf8.encode(svg)), fileName: f.fileName ?? 'kalks-academy-$code.svg', contentType: 'image/svg+xml');
  }
}

final academyApiProvider = Provider.autoDispose<AcademyApi>((ref) => AcademyApi(ref.watch(apiProvider)));

/* ------------------------------------------------------------------ labels and rules (api.ts) */

const List<String> kTracks = ['fundamental', 'technical', 'options'];

bool isTrack(String k) => kTracks.contains(k);

/// Full track name ("Technical analysis"); an unknown track shows its raw key.
String trackLabel(T t, String k) => t.dyn('academy.track.$k', fallback: k);

/// Short track name ("Technical").
String trackShort(T t, String k) => t.dyn('academy.trackShort.$k', fallback: k);

/// "{count} technical" for the phase cards.
String trackCount(T t, String k, int count) => t.dyn('academy.phaseCard.$k', fallback: '$count $k', vars: {'count': count});

/// Chip tone per track (fundamental info, technical ember, options gold).
KChipTone trackTone(String k) => switch (k) {
  'fundamental' => KChipTone.info,
  'technical' => KChipTone.ember,
  'options' => KChipTone.gold,
  _ => KChipTone.neutral,
};

IconData trackIcon(String k) => switch (k) {
  'fundamental' => LucideIcons.landmark,
  'technical' => LucideIcons.chartColumn,
  'options' => LucideIcons.chartSpline,
  _ => LucideIcons.bookOpen,
};

/// Level chip tone (Beginner up, Intermediate gold, Advanced ember, Professional info).
KChipTone levelTone(String l) => switch (l) {
  'Beginner' => KChipTone.up,
  'Intermediate' => KChipTone.gold,
  'Advanced' => KChipTone.ember,
  'Professional' => KChipTone.info,
  _ => KChipTone.neutral,
};

/// Translated level label (levels are English enums from the service).
String levelLabel(T t, String l) => l.isEmpty ? '' : t.dyn('academy.level.${l.toLowerCase()}', fallback: l);

typedef TrackTally = ({String track, int done, int total});

/// Chapters per track of a phase (sections on the same track are added up), in track display order.
List<TrackTally> trackTallies(Iterable<AcademySection> sections) {
  final by = <String, TrackTally>{};
  for (final s in sections) {
    final row = by[s.track] ?? (track: s.track, done: 0, total: 0);
    by[s.track] = (track: s.track, done: row.done + s.done, total: row.total + s.chapters.length);
  }
  int rank(String k) {
    final i = kTracks.indexOf(k);
    return i < 0 ? kTracks.length : i;
  }

  return by.values.toList()..sort((a, b) => rank(a.track) - rank(b.track));
}

/// Distinct tracks across sections, in display order.
List<String> tracksOf(Iterable<AcademySection> sections) => [for (final x in trackTallies(sections)) x.track];

/// The finish of a phase's book (web academy/live/book.tsx FINISH, the account cards' finishes): Beginner orange,
/// Intermediate copper, Advanced black, Professional gold; electives plum.
KCardFinish bookFinish(AcademyPhase p) {
  if (p.elective) return KCardFinish.options;
  return switch (p.level) {
    'Intermediate' => KCardFinish.cent,
    'Advanced' => KCardFinish.pro,
    'Professional' => KCardFinish.vip,
    _ => KCardFinish.standard,
  };
}

/// "45 min", "2h", "2h 15m".
String fmtMin(T t, int m) {
  if (m >= 60) {
    return m % 60 != 0 ? t('academy.duration.hoursMin', {'h': m ~/ 60, 'm': m % 60}) : t('academy.duration.hours', {'h': m ~/ 60});
  }
  return t('academy.duration.min', {'count': m});
}

String fmtDay(T t, DateTime? d) => d == null ? '' : LocaleFormat(t.locale).date(d);

int pctOf(int a, int b) => b > 0 ? (a / b * 100).round() : 0;

/// The phase card's state chip.
({String label, KChipTone tone}) phaseState(AcademyPhase p, T t) {
  if (p.certificate != null) return (label: t('academy.state.certified'), tone: KChipTone.up);
  if (p.allDone) return (label: t('academy.state.examReady'), tone: KChipTone.gold);
  if (p.done > 0) return (label: t('academy.state.inProgress'), tone: KChipTone.ember);
  return (label: t('academy.state.notStarted'), tone: KChipTone.neutral);
}

/// The answer letters of quiz and exam options.
const List<String> kLetters = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J'];

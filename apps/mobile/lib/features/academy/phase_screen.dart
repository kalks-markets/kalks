// Academy › a phase (/academy/phase/:phase): port of the web's LivePhase (components/academy/live/phase.tsx):
//   back link · hero (chips, title, summary, meta, progress, Continue / Take the final exam / Next phase) · chapters
//   (track filter, section cards with chapter rows) · final exam card · certificate card · previous / next phase.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'academy_api.dart';
import 'widgets/shared.dart';

class AcademyPhaseScreen extends ConsumerStatefulWidget {
  const AcademyPhaseScreen({super.key, required this.slug});
  final String slug;

  @override
  ConsumerState<AcademyPhaseScreen> createState() => _AcademyPhaseScreenState();
}

class _AcademyPhaseScreenState extends ConsumerState<AcademyPhaseScreen> {
  String _track = 'all';

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final cat = ref.watch(academyCatalogProvider);
    return KPageScroll(
      onRefresh: () async {
        ref.invalidate(academyCatalogProvider);
        await ref.read(academyCatalogProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        AcademyBackLink(href: '/academy', label: t('academy.title')),
        const SizedBox(height: 12),
        KAsync(
          value: cat,
          loading: const AcademyPageSkeleton(),
          error: (e) => AcademyUnavailable(error: e, onRetry: () => ref.invalidate(academyCatalogProvider)),
          builder: (data) {
            final p = data.phases.where((x) => x.slug == widget.slug).firstOrNull;
            if (p == null) return AcademyUnavailable(notFound: true, onRetry: () => ref.invalidate(academyCatalogProvider));
            return _body(context, data, p);
          },
        ),
      ],
    );
  }

  Widget _body(BuildContext context, AcademyCatalog data, AcademyPhase p) {
    final t = context.t;
    final k = context.k;
    final next = p.sections.expand((s) => s.chapters).where((c) => !c.progress.completed).firstOrNull;
    final prev = data.phases.where((x) => x.order == p.order - 1).firstOrNull;
    final nextPhase = data.phases.where((x) => x.order == p.order + 1).firstOrNull;
    // the filter offers the tracks this phase actually has (an elective has one, so it shows no filter)
    final tracks = tracksOf(p.sections);
    final shown = _track != 'all' && tracks.contains(_track) ? _track : 'all';
    final sections = p.sections.where((s) => shown == 'all' || s.track == shown).toList();
    final coreCount = data.phases.where((x) => !x.elective).length;
    final meta = context.text.footnote.copyWith(color: k.fg3, fontFeatures: kTabular);
    final rtl = Directionality.of(context) == TextDirection.rtl;

    Widget? action;
    if (next != null) {
      action = KButton(
        label: p.done > 0 ? t('common.continue') : t('academy.phase.start'),
        icon: LucideIcons.circlePlay,
        onPressed: () => context.push('/academy/chapter/${next.slug}'),
      );
    } else if (p.exam != null && !p.exam!.passed) {
      action = KButton(label: t('academy.phase.takeExam'), icon: LucideIcons.graduationCap, onPressed: () => context.push('/academy/phase/${p.slug}/exam'));
    } else if (nextPhase != null) {
      action = KButton(
        label: t('academy.phase.next'),
        trailingIcon: rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight,
        onPressed: () => context.pushReplacement('/academy/phase/${nextPhase.slug}'),
      );
    }

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        // hero
        KCard(
          padding: EdgeInsets.zero,
          child: Stack(
            children: [
              Positioned.fill(child: FinishWash(finish: bookFinish(p))),
              Padding(
                padding: const EdgeInsets.all(22),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(
                          child: Wrap(
                            spacing: 8,
                            runSpacing: 6,
                            children: [
                              // core phases count among themselves ("Phase 3 of 8"); an elective stands alone
                              KChip(
                                label: p.elective ? t('academy.phaseN', {'n': p.order}) : t('academy.phase.ofTotal', {'n': p.order, 'total': coreCount}),
                                tone: KChipTone.ember,
                              ),
                              KChip(label: levelLabel(t, p.level), tone: levelTone(p.level)),
                              if (p.elective) KChip(label: t('academy.elective')),
                              if (p.certificate != null) KChip(label: t('academy.state.certified'), tone: KChipTone.up, dot: true),
                            ],
                          ),
                        ),
                        const SizedBox(width: 14),
                        SizedBox(width: 92, child: PhaseBook(p: p)),
                      ],
                    ),
                    const SizedBox(height: 16),
                    Text(p.title, style: context.text.largeTitle.copyWith(fontSize: 28, fontWeight: FontWeight.w500, height: 1.2, letterSpacing: -0.5)),
                    const SizedBox(height: 8),
                    Text(p.summary, style: context.text.callout.copyWith(color: k.fg2, fontSize: 14, height: 1.55)),
                    const SizedBox(height: 14),
                    Wrap(
                      spacing: 18,
                      runSpacing: 4,
                      children: [
                        Text(t('academy.phase.chapters', {'count': p.total}), style: meta),
                        Text(t('academy.phase.reading', {'time': fmtMin(t, p.minutes)}), style: meta),
                        if (p.exam != null) Text(t('academy.phaseCard.finalExam', {'count': p.exam!.questions}), style: meta),
                      ],
                    ),
                    const SizedBox(height: 22),
                    Row(
                      children: [
                        Expanded(
                          child: Text(
                            t('academy.phase.complete', {'done': p.done, 'total': p.total}),
                            style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                          ),
                        ),
                        Text(
                          '${pctOf(p.done, p.total)}%',
                          style: context.text.caption.copyWith(fontFeatures: kTabular),
                          textDirection: TextDirection.ltr,
                        ),
                      ],
                    ),
                    const SizedBox(height: 8),
                    AcademyProgress(pctOf(p.done, p.total), up: p.done == p.total),
                    if (action != null) ...[const SizedBox(height: 18), action],
                  ],
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 28),
        // chapters
        Text(t('academy.phase.chaptersTitle'), style: context.text.title1.copyWith(fontSize: 19)),
        const SizedBox(height: 3),
        Text(t('academy.phase.chaptersText'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13)),
        if (tracks.length > 1) ...[
          const SizedBox(height: 12),
          KSegmented<String>(
            values: ['all', ...tracks],
            labels: [t(tracks.length == 2 ? 'academy.phase.bothTracks' : 'academy.phase.allTracks'), for (final x in tracks) trackShort(t, x)],
            selected: shown,
            height: 34,
            onChanged: (v) => setState(() => _track = v),
          ),
        ],
        const SizedBox(height: 14),
        for (final s in sections) ...[_SectionCard(s: s), const SizedBox(height: 16)],
        _ExamCard(p: p),
        const SizedBox(height: 16),
        _CertificateCard(p: p),
        const SizedBox(height: 20),
        if (prev != null)
          _PhaseLink(
            label: '${rtl ? '→' : '←'} ${t('academy.phaseLink', {'n': prev.order, 'title': prev.title})}',
            onTap: () => context.pushReplacement('/academy/phase/${prev.slug}'),
          ),
        if (nextPhase != null)
          Align(
            alignment: AlignmentDirectional.centerEnd,
            child: _PhaseLink(
              label: '${t('academy.phaseLink', {'n': nextPhase.order, 'title': nextPhase.title})} ${rtl ? '←' : '→'}',
              onTap: () => context.pushReplacement('/academy/phase/${nextPhase.slug}'),
            ),
          ),
        const RiskNote(),
      ],
    );
  }
}

class _PhaseLink extends StatelessWidget {
  const _PhaseLink({required this.label, required this.onTap});
  final String label;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => KPressable(
    onTap: onTap,
    semanticLabel: label,
    child: Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Text(
        label,
        style: context.text.label.copyWith(color: context.k.fg3, fontWeight: FontWeight.w400),
      ),
    ),
  );
}

KChipTone _badgeTone(String track) => isTrack(track) ? trackTone(track) : KChipTone.neutral;

class _SectionCard extends StatelessWidget {
  const _SectionCard({required this.s});
  final AcademySection s;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final done = s.done;
    final firstOpen = s.chapters.indexWhere((c) => !c.progress.completed);
    return KCard(
      key: ValueKey('section-${s.track}'),
      padding: const EdgeInsets.fromLTRB(14, 18, 14, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                SoftBadge(icon: trackIcon(s.track), tone: _badgeTone(s.track)),
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      AcademyLabel(trackLabel(t, s.track)),
                      const SizedBox(height: 2),
                      Text(s.title, style: context.text.title2),
                      const SizedBox(height: 2),
                      Text(s.summary, style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13)),
                    ],
                  ),
                ),
                const SizedBox(width: 8),
                Text(
                  '$done/${s.chapters.length}',
                  textDirection: TextDirection.ltr,
                  style: context.text.footnote.copyWith(color: k.fg2, fontFeatures: kTabular),
                ),
              ],
            ),
          ),
          const SizedBox(height: 12),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 6),
            child: AcademyProgress(pctOf(done, s.chapters.length), up: done == s.chapters.length),
          ),
          const SizedBox(height: 14),
          for (var i = 0; i < s.chapters.length; i++) ...[
            if (i > 0) const SizedBox(height: 6),
            _ChapterRow(c: s.chapters[i], n: i + 1, highlight: i == firstOpen),
          ],
        ],
      ),
    );
  }
}

class _ChapterRow extends StatelessWidget {
  const _ChapterRow({required this.c, required this.n, required this.highlight});
  final ChapterCard c;
  final int n;
  final bool highlight;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final pr = c.progress;
    final q = pr.quizTotal != null && pr.quizTotal! > 0 ? '${pr.quizBest ?? 0}/${pr.quizTotal}' : '–';
    return KPressable(
      key: ValueKey('chapter-link-${c.slug}'),
      pressedScale: 0.99,
      semanticLabel: c.title,
      onTap: () => context.push('/academy/chapter/${c.slug}'),
      child: Container(
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          color: k.surface2,
          borderRadius: BorderRadius.circular(k.rowRadius),
          border: Border.all(color: highlight ? k.ember.withValues(alpha: 0.3) : k.line),
        ),
        child: Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            StatusDot(state: pr.completed ? DotState.done : DotState.open, n: n),
            const SizedBox(width: 12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(c.title, style: context.text.label.copyWith(fontSize: 13.5, fontWeight: FontWeight.w600, height: 1.3)),
                  const SizedBox(height: 2),
                  Text(
                    c.summary,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                    style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 12),
                  ),
                ],
              ),
            ),
            const SizedBox(width: 8),
            Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(LucideIcons.clock, size: 12, color: k.fg3),
                    const SizedBox(width: 3),
                    Text(
                      t('academy.duration.min', {'count': c.minutes}),
                      style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontSize: 11),
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                if (pr.completed)
                  KChip(label: t('academy.phase.quizScore', {'score': q}), tone: KChipTone.up, small: true)
                else if (pr.readPct > 0)
                  Text(
                    t('academy.readPct', {'pct': pr.readPct}),
                    style: context.text.micro.copyWith(color: k.fg2, fontWeight: FontWeight.w400, fontSize: 11),
                  ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}

class _ExamCard extends StatelessWidget {
  const _ExamCard({required this.p});
  final AcademyPhase p;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final e = p.exam;
    if (e == null) return const SizedBox.shrink();
    final left = p.total - p.done;
    final oneTrack = tracksOf(p.sections).length < 2;
    return KCard(
      key: const ValueKey('exam-card'),
      padding: const EdgeInsets.all(22),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    AcademyLabel(t('academy.exam.final')),
                    const SizedBox(height: 4),
                    Text(t('academy.exam.phaseExam', {'n': p.order}), style: context.text.title2.copyWith(fontSize: 18)),
                  ],
                ),
              ),
              SoftBadge(icon: e.unlocked ? LucideIcons.graduationCap : LucideIcons.lock, tone: e.unlocked ? KChipTone.gold : KChipTone.neutral, size: 44),
            ],
          ),
          const SizedBox(height: 16),
          Row(
            children: [
              Expanded(
                child: MiniStat(value: '${e.questions}', label: t('academy.exam.questions')),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: MiniStat(value: '${e.passMark}%', label: t('academy.exam.passMark')),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: MiniStat(value: e.bestPct == null ? '–' : '${e.bestPct}%', label: t('academy.exam.bestScore')),
              ),
            ],
          ),
          const SizedBox(height: 16),
          Text(
            e.passed
                ? t('academy.exam.passedText')
                : e.unlocked
                ? t(oneTrack ? 'academy.exam.unlockedTextOneTrack' : 'academy.exam.unlockedText')
                : t('academy.exam.lockedText', {'count': left}),
            style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13, height: 1.5),
          ),
          const SizedBox(height: 18),
          Align(
            alignment: AlignmentDirectional.centerStart,
            child: e.unlocked
                ? KButton(
                    label: e.passed
                        ? t('academy.exam.retake')
                        : e.attempts > 0
                        ? t('common.retry')
                        : t('academy.exam.start'),
                    icon: LucideIcons.graduationCap,
                    variant: KButtonVariant.surface,
                    onPressed: () => context.push('/academy/phase/${p.slug}/exam'),
                  )
                : KButton(label: t('academy.exam.locked'), icon: LucideIcons.lock, variant: KButtonVariant.surface, onPressed: null),
          ),
        ],
      ),
    );
  }
}

class _CertificateCard extends StatelessWidget {
  const _CertificateCard({required this.p});
  final AcademyPhase p;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final c = p.certificate;
    if (c == null) {
      return KCard(
        padding: const EdgeInsets.all(22),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            AcademyLabel(t('academy.cert.title')),
            const SizedBox(height: 14),
            CustomPaint(
              painter: _DashedBorder(color: k.line, radius: 16),
              child: Container(
                padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 28),
                decoration: BoxDecoration(color: k.surface2.withValues(alpha: 0.5), borderRadius: BorderRadius.circular(16)),
                child: Column(
                  children: [
                    Icon(LucideIcons.award, size: 28, color: k.fg3),
                    const SizedBox(height: 8),
                    ConstrainedBox(
                      constraints: const BoxConstraints(maxWidth: 240),
                      child: Text(
                        t('academy.cert.placeholder', {'n': p.order}),
                        textAlign: TextAlign.center,
                        style: context.text.footnote.copyWith(color: k.fg3),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ],
        ),
      );
    }
    return KCard(
      key: const ValueKey('certificate-card'),
      padding: const EdgeInsets.all(22),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(
            children: [
              Expanded(child: AcademyLabel(t('academy.cert.title'))),
              KChip(label: t('academy.cert.issued', {'date': fmtDay(t, c.issuedAt)}), tone: KChipTone.up, dot: true),
            ],
          ),
          const SizedBox(height: 14),
          CertificateImage(code: c.code, semanticLabel: t('academy.cert.alt', {'n': p.order})),
          const SizedBox(height: 14),
          CertificateActions(code: c.code),
        ],
      ),
    );
  }
}

/// The web's dashed placeholder border.
class _DashedBorder extends CustomPainter {
  _DashedBorder({required this.color, required this.radius});
  final Color color;
  final double radius;

  @override
  void paint(Canvas canvas, Size size) {
    final paint = Paint()
      ..color = color
      ..style = PaintingStyle.stroke
      ..strokeWidth = 1.2;
    final path = Path()..addRRect(RRect.fromRectAndRadius(Offset.zero & size, Radius.circular(radius)));
    for (final m in path.computeMetrics()) {
      var d = 0.0;
      while (d < m.length) {
        canvas.drawPath(m.extractPath(d, d + 5), paint);
        d += 9;
      }
    }
  }

  @override
  bool shouldRepaint(_DashedBorder old) => old.color != color || old.radius != radius;
}

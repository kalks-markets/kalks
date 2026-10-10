// Academy › My progress (/academy/progress): port of the web's LiveProgress (components/academy/live/progress.tsx):
//   back link · header · KPI cards (chapters, certificates, quiz average, streak) · by phase (the web's table as rows:
//   per-track progress, exam state, certificate code) · certificates (image, level, score, Download / Copy link /
//   Verify) or the empty card.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../i18n/i18n.dart';
import '../../ui/ui.dart';
import 'academy_api.dart';
import 'widgets/shared.dart';

class AcademyProgressScreen extends ConsumerWidget {
  const AcademyProgressScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final cat = ref.watch(academyCatalogProvider);
    return KPageScroll(
      onRefresh: () async {
        ref
          ..invalidate(academyCatalogProvider)
          ..invalidate(academyCertificatesProvider);
        await ref.read(academyCatalogProvider.future).then((_) {}, onError: (Object _) {});
      },
      children: [
        AcademyBackLink(href: '/academy', label: t('academy.title')),
        const SizedBox(height: 12),
        KAsync(
          value: cat,
          loading: const AcademyPageSkeleton(),
          error: (e) => AcademyUnavailable(error: e, onRetry: () => ref.invalidate(academyCatalogProvider)),
          builder: (data) => _Body(cat: data),
        ),
      ],
    );
  }
}

class _Body extends ConsumerWidget {
  const _Body({required this.cat});
  final AcademyCatalog cat;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final k = context.k;
    final me = cat.me;
    final phases = cat.phases;
    // one column per track any phase has (core tracks, then product tracks such as options)
    final tracks = tracksOf(phases.expand((p) => p.sections));
    final certs = ref.watch(academyCertificatesProvider).value;
    final kpiWidth = (MediaQuery.sizeOf(context).width - 2 * KSpace.page) * 0.78;
    Widget figure(String s) => Text(s, textDirection: TextDirection.ltr);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        KPageHeader(title: t('academy.home.myProgress'), subtitle: Text(t('academy.progress.subtitle'))),
        const SizedBox(height: 22),
        SizedBox(
          height: 176,
          child: ListView(
            scrollDirection: Axis.horizontal,
            clipBehavior: Clip.none,
            physics: const PageScrollPhysics(parent: BouncingScrollPhysics()),
            children: [
              KKpiCard(
                width: kpiWidth,
                label: t('academy.progress.chaptersComplete'),
                icon: LucideIcons.bookOpen,
                value: figure('${me.chaptersDone} / ${me.chaptersTotal}'),
                chip: KChip(label: t('academy.progress.ofCourse', {'pct': pctOf(me.chaptersDone, me.chaptersTotal)}), tone: KChipTone.ember),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('academy.certs.title'),
                icon: LucideIcons.award,
                value: figure('${me.certificates} / ${phases.length}'),
                chip: KChip(label: t('academy.progress.onePerPhase'), tone: KChipTone.up),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('academy.stats.quizAvg'),
                icon: LucideIcons.target,
                value: figure(me.quizAvg == null ? '–' : '${me.quizAvg}%'),
                chip: KChip(label: t('academy.progress.bestPerChapter')),
              ),
              const SizedBox(width: 12),
              KKpiCard(
                width: kpiWidth,
                label: t('academy.stats.streak'),
                icon: LucideIcons.flame,
                value: Text(t('academy.progress.streakDays', {'count': me.streak})),
                chip: KChip(label: t('academy.progress.studied', {'time': fmtMin(t, me.minutesDone)})),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        KCard(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              KCardHeader(title: t('academy.progress.byPhase'), subtitle: t('academy.progress.byPhaseSub'), icon: LucideIcons.graduationCap),
              const SizedBox(height: 8),
              for (var i = 0; i < phases.length; i++) ...[if (i > 0) const KDivider(), _PhaseRow(p: phases[i], tracks: tracks)],
            ],
          ),
        ),
        const SizedBox(height: 28),
        Text(t('academy.certs.title'), style: context.text.title1.copyWith(fontSize: 19)),
        const SizedBox(height: 3),
        Text(t('academy.progress.certsText'), style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13)),
        const SizedBox(height: 14),
        if (certs != null && certs.isNotEmpty)
          for (final c in certs) ...[_CertificateTile(c: c), const SizedBox(height: 16)]
        else
          KCard(
            padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 40),
            child: Column(
              children: [
                Icon(LucideIcons.award, size: 28, color: k.fg3),
                const SizedBox(height: 8),
                Text(t('academy.progress.noCerts'), style: context.text.headline.copyWith(fontSize: 14.5)),
                const SizedBox(height: 4),
                Text(
                  t('academy.progress.noCertsText'),
                  textAlign: TextAlign.center,
                  style: context.text.footnote.copyWith(color: k.fg3, fontSize: 13),
                ),
                const SizedBox(height: 14),
                KButton(
                  label: t('academy.hero.continue'),
                  icon: LucideIcons.clock,
                  variant: KButtonVariant.surface,
                  size: KButtonSize.sm,
                  onPressed: () => context.go('/academy'),
                ),
              ],
            ),
          ),
      ],
    );
  }
}

/// One row of the web's "By phase" table: phase, per-track progress, exam state, certificate code.
class _PhaseRow extends StatelessWidget {
  const _PhaseRow({required this.p, required this.tracks});
  final AcademyPhase p;
  final List<String> tracks;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final tally = {for (final x in trackTallies(p.sections)) x.track: x};
    final e = p.exam;
    final small = context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400);
    final Widget exam = e == null
        ? Text(t('academy.exam.locked'), style: small)
        : e.passed
        ? KChip(label: t('academy.progress.examPassed', {'pct': e.bestPct ?? 0}), tone: KChipTone.up, small: true)
        : e.unlocked
        ? KPressable(
            onTap: () => context.push('/academy/phase/${p.slug}/exam'),
            semanticLabel: t('academy.progress.ready'),
            child: KChip(label: t('academy.progress.ready'), tone: KChipTone.gold, small: true),
          )
        : e.attempts > 0
        ? KChip(label: t('academy.progress.best', {'pct': e.bestPct ?? 0}), tone: KChipTone.warn, small: true)
        : Text(t('academy.exam.locked'), style: small);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KPressable(
            onTap: () => context.push('/academy/phase/${p.slug}'),
            semanticLabel: p.title,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('${p.order}. ${p.title}', style: context.text.label.copyWith(fontWeight: FontWeight.w600)),
                const SizedBox(height: 1),
                Text('${levelLabel(t, p.level)}${p.elective ? ' · ${t('academy.elective')}' : ''}', style: small),
              ],
            ),
          ),
          const SizedBox(height: 10),
          Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              for (var i = 0; i < tracks.length; i++) ...[
                if (i > 0) const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(trackShort(t, tracks[i]).toUpperCase(), style: context.text.micro.copyWith(color: k.fg3, fontSize: 10, letterSpacing: 0.6)),
                      const SizedBox(height: 3),
                      if (tally[tracks[i]] case final x?) ...[
                        Text(
                          '${x.done}/${x.total}',
                          textDirection: TextDirection.ltr,
                          style: small.copyWith(color: k.fg2, fontFeatures: kTabular),
                        ),
                        const SizedBox(height: 4),
                        AcademyProgress(pctOf(x.done, x.total), up: x.done == x.total && x.total > 0, height: 5),
                      ] else
                        // a phase without this track (e.g. the options elective has no technical section)
                        Text('–', style: small),
                    ],
                  ),
                ),
              ],
            ],
          ),
          const SizedBox(height: 10),
          Row(
            children: [
              Text('${t('academy.progress.col.exam')}  ', style: small),
              exam,
              const Spacer(),
              Text('${t('academy.progress.col.certificate')}  ', style: small),
              Text(
                p.certificate?.code ?? '–',
                textDirection: TextDirection.ltr,
                style: p.certificate == null ? small : context.text.mono(11.5, color: k.fg2),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

class _CertificateTile extends StatelessWidget {
  const _CertificateTile({required this.c});
  final AcademyCertificate c;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KCard(
      key: const ValueKey('certificate-tile'),
      padding: EdgeInsets.zero,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          // the paper certificate (web CertificateCard); a tap opens the real one
          Padding(
            padding: const EdgeInsets.fromLTRB(14, 14, 14, 0),
            child: CertificateImage(
              code: c.code,
              semanticLabel: t('academy.cert.alt', {'n': c.phaseOrder}),
              radius: 0,
              face: KCertificateCard(
                series: t.dyn('academy.book.series', fallback: 'Kalks Academy'),
                heading: t.dyn('academy.cert.face.title', fallback: 'Certificate of Completion'),
                certifies: t.dyn('academy.cert.face.certifies', fallback: 'This certifies that'),
                name: c.learnerName,
                text: t.dyn(
                  'academy.cert.face.completed',
                  fallback: 'has completed Phase {n}: {title}, with an exam score of {pct}%',
                  vars: {'n': c.phaseOrder, 'title': c.phaseTitle, 'pct': c.scorePct},
                ),
                date: fmtDay(t, c.issuedAt),
                dateLabel: t.dyn('academy.cert.face.issued', fallback: 'Date issued'),
                code: c.code,
                codeLabel: t.dyn('academy.cert.face.id', fallback: 'Certificate ID'),
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.all(18),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Wrap(
                  spacing: 8,
                  runSpacing: 6,
                  children: [
                    KChip(label: levelLabel(t, c.level), tone: levelTone(c.level)),
                    KChip(label: t('academy.cert.examScore', {'pct': c.scorePct}), tone: KChipTone.up, dot: true),
                  ],
                ),
                const SizedBox(height: 10),
                Text(t('academy.phaseTitle', {'n': c.phaseOrder, 'title': c.phaseTitle}), style: context.text.headline),
                const SizedBox(height: 2),
                Text(
                  t('academy.cert.codeIssued', {'code': c.code, 'date': fmtDay(t, c.issuedAt)}),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                ),
                const SizedBox(height: 14),
                CertificateActions(code: c.code, verifyUrl: c.verifyUrl, verifyLabel: t('academy.cert.verify')),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

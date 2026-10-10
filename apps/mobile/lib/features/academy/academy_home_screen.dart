// Academy › Courses (/academy): port of the web's LiveAcademyHome (components/academy/live/home.tsx), phone order:
//   header (Glossary · My progress) · continue hero · learning stats · your learning path (core phase cards) ·
//   electives · glossary teaser · certificates · practise in Kalks Trader · risk note.
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:intl/intl.dart' show DateFormat;
import 'package:lucide_icons_flutter/lucide_icons.dart';

import '../../core/format/format.dart';
import '../../i18n/i18n.dart';
import '../../shell/page_hero.dart';
import '../../ui/ui.dart';
import 'academy_api.dart';
import 'widgets/shared.dart';

class AcademyHomeScreen extends ConsumerWidget {
  const AcademyHomeScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final t = context.t;
    final cat = ref.watch(academyCatalogProvider);
    final hero = pageHero(
      context,
      ref,
      path: '/academy',
      title: t('academy.title'),
      lead: cat.hasValue ? t('academy.home.subtitle', {'count': cat.requireValue.me.chaptersTotal}) : null,
      actions: [
        KHeroButton(label: t('academy.home.glossary'), icon: LucideIcons.library, onPressed: () => context.go('/academy/glossary')),
        KHeroButton(label: t('academy.home.myProgress'), icon: LucideIcons.award, onPressed: () => context.go('/academy/progress')),
      ],
    );
    return KPageScroll(
      onRefresh: () async {
        ref.invalidate(academyCatalogProvider);
        await ref.read(academyCatalogProvider.future).then((_) {}, onError: (Object _) {});
      },
      hero: hero,
      padding: EdgeInsets.fromLTRB(KSpace.page, hero == null ? 12 : 18, KSpace.page, 24),
      children: [
        KAsync(
          value: cat,
          loading: const AcademyPageSkeleton(),
          error: (e) => AcademyUnavailable(error: e, onRetry: () => ref.invalidate(academyCatalogProvider)),
          builder: (data) {
            final core = data.phases.where((p) => !p.elective).toList();
            final electives = data.phases.where((p) => p.elective).toList();
            return Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                if (hero == null) ...[
                  KPageHeader(title: t('academy.title'), subtitle: Text(t('academy.home.subtitle', {'count': data.me.chaptersTotal}))),
                  const SizedBox(height: 14),
                  Wrap(
                    spacing: 8,
                    runSpacing: 8,
                    children: [
                      KButton(
                        label: t('academy.home.glossary'),
                        icon: LucideIcons.library,
                        variant: KButtonVariant.surface,
                        size: KButtonSize.sm,
                        onPressed: () => context.go('/academy/glossary'),
                      ),
                      KButton(
                        label: t('academy.home.myProgress'),
                        icon: LucideIcons.award,
                        variant: KButtonVariant.surface,
                        size: KButtonSize.sm,
                        onPressed: () => context.go('/academy/progress'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 20),
                ],
                _ContinueHero(cat: data),
                const SizedBox(height: 16),
                _StatsCard(me: data.me),
                const SizedBox(height: 28),
                _BlockTitle(title: t('academy.home.pathTitle'), text: t('academy.home.pathText')),
                const SizedBox(height: 14),
                for (final p in core) ...[_PhaseCard(p: p), const SizedBox(height: 16)],
                if (electives.isNotEmpty) ...[
                  const SizedBox(height: 12),
                  _BlockTitle(title: t('academy.home.electivesTitle'), text: t('academy.home.electivesText')),
                  const SizedBox(height: 14),
                  for (final p in electives) ...[_PhaseCard(p: p), const SizedBox(height: 16)],
                ],
                const _GlossaryTeaser(),
                const SizedBox(height: 16),
                _CertificatesCard(cat: data),
                const SizedBox(height: 16),
                const _PracticeCard(),
                const RiskNote(),
              ],
            );
          },
        ),
      ],
    );
  }
}

class _BlockTitle extends StatelessWidget {
  const _BlockTitle({required this.title, required this.text});
  final String title, text;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Text(title, style: context.text.title1.copyWith(fontSize: 19)),
      const SizedBox(height: 3),
      Text(text, style: context.text.footnote.copyWith(color: context.k.fg3, fontSize: 13)),
    ],
  );
}

/* ------------------------------------------------------------------ continue hero */

class _ContinueHero extends StatelessWidget {
  const _ContinueHero({required this.cat});
  final AcademyCatalog cat;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final c = cat.me.next;
    final phase = c != null ? cat.phases.where((p) => p.slug == c.phaseSlug).firstOrNull : cat.phases.firstOrNull;
    if (phase == null) return const SizedBox.shrink();
    final allDone = c == null;
    final chip = allDone
        ? t('academy.hero.allDone')
        : c.started
        ? t('academy.hero.continue')
        : cat.me.chaptersDone > 0
        ? t('academy.hero.upNext')
        : t('academy.hero.startHere');
    return KCard(
      padding: EdgeInsets.zero,
      child: Stack(
        children: [
          Positioned.fill(child: FinishWash(finish: bookFinish(phase))),
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
                          KChip(label: chip, tone: KChipTone.ember, dot: true),
                          KChip(label: levelLabel(t, phase.level), tone: levelTone(phase.level)),
                          KChip(label: t('academy.phaseN', {'n': phase.order})),
                        ],
                      ),
                    ),
                    const SizedBox(width: 14),
                    SizedBox(width: 96, child: PhaseBook(p: phase)),
                  ],
                ),
                const SizedBox(height: 16),
                Text(
                  allDone ? t('academy.hero.allDoneTitle') : c.title,
                  style: context.text.largeTitle.copyWith(fontSize: 26, fontWeight: FontWeight.w500, height: 1.2, letterSpacing: -0.5),
                ),
                const SizedBox(height: 6),
                Text(
                  allDone
                      ? t('academy.hero.allDoneText')
                      : [
                          phase.title,
                          t('academy.hero.minRead', {'time': fmtMin(t, c.minutes)}),
                          if (c.readPct > 0) t('academy.readPct', {'pct': c.readPct}),
                        ].join(' · '),
                  style: context.text.callout.copyWith(color: k.fg2),
                ),
                const SizedBox(height: 22),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        t('academy.hero.phaseProgress', {'n': phase.order, 'done': phase.done, 'total': phase.total}),
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                      ),
                    ),
                    Text(
                      '${pctOf(phase.done, phase.total)}%',
                      style: context.text.caption.copyWith(fontFeatures: kTabular),
                      textDirection: TextDirection.ltr,
                    ),
                  ],
                ),
                const SizedBox(height: 8),
                Segments(done: phase.done, total: phase.total),
                const SizedBox(height: 24),
                Wrap(
                  spacing: 10,
                  runSpacing: 10,
                  children: [
                    if (allDone)
                      KButton(
                        label: t('academy.hero.myCertificates'),
                        icon: LucideIcons.award,
                        size: KButtonSize.lg,
                        onPressed: () => context.go('/academy/progress'),
                      )
                    else
                      KButton(
                        label: c.started ? t('academy.hero.resume') : t('academy.hero.start'),
                        icon: LucideIcons.circlePlay,
                        size: KButtonSize.lg,
                        onPressed: () => context.push('/academy/chapter/${c.slug}'),
                      ),
                    KButton(
                      label: t('academy.hero.overview'),
                      variant: KButtonVariant.surface,
                      size: KButtonSize.lg,
                      onPressed: () => context.push('/academy/phase/${phase.slug}'),
                    ),
                  ],
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ learning streak */

class _StatsCard extends StatelessWidget {
  const _StatsCard({required this.me});
  final AcademyMe me;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final days = me.activeDays.toSet();
    final now = DateTime.now();
    final fmt = DateFormat.EEEEE(intlLocale(t.locale));
    final week = [
      for (var i = 0; i < 7; i++)
        () {
          final d = now.subtract(Duration(days: 6 - i));
          final u = d.toUtc();
          final key = '${u.year.toString().padLeft(4, '0')}-${u.month.toString().padLeft(2, '0')}-${u.day.toString().padLeft(2, '0')}';
          return (key: key, label: latinDigits(fmt.format(d)));
        }(),
    ];
    return KCard(
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
                    AcademyLabel(t('academy.stats.streak')),
                    const SizedBox(height: 8),
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.baseline,
                      textBaseline: TextBaseline.alphabetic,
                      children: [
                        Text(
                          '${me.streak}',
                          style: context.text.moneyXL.copyWith(fontSize: 44, fontWeight: FontWeight.w600),
                          textDirection: TextDirection.ltr,
                        ),
                        const SizedBox(width: 8),
                        Text(t('academy.stats.days', {'count': me.streak}), style: context.text.body.copyWith(color: k.fg2)),
                      ],
                    ),
                  ],
                ),
              ),
              const SoftBadge(icon: LucideIcons.flame, tone: KChipTone.ember, size: 44),
            ],
          ),
          const SizedBox(height: 16),
          Row(
            children: [
              for (final d in week)
                Expanded(
                  child: Column(
                    children: [
                      Container(
                        width: 28,
                        height: 28,
                        decoration: BoxDecoration(
                          color: days.contains(d.key) ? k.ember : k.surface2,
                          shape: BoxShape.circle,
                          border: Border.all(color: days.contains(d.key) ? k.ember.withValues(alpha: 0.4) : k.line),
                        ),
                        child: days.contains(d.key) ? Icon(LucideIcons.flame, size: 14, color: k.onEmber) : null,
                      ),
                      const SizedBox(height: 4),
                      Text(
                        d.label,
                        style: context.text.micro.copyWith(color: k.fg3, fontWeight: FontWeight.w500, fontSize: 10),
                      ),
                    ],
                  ),
                ),
            ],
          ),
          const SizedBox(height: 20),
          Row(
            children: [
              Expanded(
                child: Text(
                  t('academy.stats.courseProgress'),
                  style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400),
                ),
              ),
              Text(
                t('academy.stats.chapters', {'done': me.chaptersDone, 'total': me.chaptersTotal}),
                style: context.text.caption.copyWith(fontWeight: FontWeight.w400, fontFeatures: kTabular),
              ),
            ],
          ),
          const SizedBox(height: 6),
          AcademyProgress(pctOf(me.chaptersDone, me.chaptersTotal)),
          const SizedBox(height: 20),
          Row(
            children: [
              Expanded(
                child: MiniStat(icon: LucideIcons.award, value: '${me.certificates}', label: t('academy.stats.certificates', {'count': me.certificates})),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: MiniStat(icon: LucideIcons.target, value: me.quizAvg == null ? '–' : '${me.quizAvg}%', label: t('academy.stats.quizAvg')),
              ),
              const SizedBox(width: 8),
              Expanded(
                child: MiniStat(icon: LucideIcons.clock, value: fmtMin(t, me.minutesDone), label: t('academy.stats.studied')),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ phase cards */

class _PhaseCard extends StatelessWidget {
  const _PhaseCard({required this.p});
  final AcademyPhase p;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final s = phaseState(p, t);
    final tracks = trackTallies(p.sections);
    final done = p.allDone;
    final footer = p.certificate != null
        ? t('academy.phaseCard.certificate', {'date': fmtDay(t, p.certificate!.issuedAt)})
        : p.exam != null
        ? t('academy.phaseCard.finalExam', {'count': p.exam!.questions})
        : '';
    final meta = context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular);
    return KCard(
      key: ValueKey('phase-card-${p.slug}'),
      padding: EdgeInsets.zero,
      onTap: () => context.push('/academy/phase/${p.slug}'),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Stack(
            children: [
              Positioned.fill(child: FinishWash(finish: bookFinish(p), strength: 0.26)),
              Padding(
                padding: const EdgeInsets.fromLTRB(18, 18, 18, 4),
                child: Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    SizedBox(width: 120, child: PhaseBook(p: p)),
                    const SizedBox(width: 16),
                    Expanded(
                      child: Wrap(
                        spacing: 6,
                        runSpacing: 6,
                        children: [
                          KChip(label: s.label, tone: s.tone, dot: true, small: true),
                          KChip(label: levelLabel(t, p.level), tone: levelTone(p.level), small: true),
                          if (p.elective) KChip(label: t('academy.elective'), small: true),
                        ],
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(20, 12, 20, 20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                AcademyLabel(t('academy.phaseN', {'n': p.order})),
                const SizedBox(height: 3),
                Text(p.title, style: context.text.headline.copyWith(fontSize: 15.5, fontWeight: FontWeight.w600)),
                const SizedBox(height: 4),
                Text(
                  p.summary,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: context.text.footnote.copyWith(color: k.fg3),
                ),
                const SizedBox(height: 12),
                Wrap(
                  spacing: 12,
                  runSpacing: 4,
                  children: [
                    for (final x in tracks) _IconText(icon: LucideIcons.bookOpen, text: trackCount(t, x.track, x.total), style: meta),
                    _IconText(icon: LucideIcons.clock, text: fmtMin(t, p.minutes), style: meta),
                  ],
                ),
                const SizedBox(height: 16),
                Row(
                  children: [
                    Expanded(child: Text(footer, style: meta)),
                    Text(
                      '${p.done}/${p.total}',
                      textDirection: TextDirection.ltr,
                      style: meta.copyWith(color: done ? k.up : (p.done > 0 ? k.fg2 : k.fg3)),
                    ),
                  ],
                ),
                const SizedBox(height: 6),
                AcademyProgress(pctOf(p.done, p.total), up: done),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class _IconText extends StatelessWidget {
  const _IconText({required this.icon, required this.text, required this.style});
  final IconData icon;
  final String text;
  final TextStyle style;

  @override
  Widget build(BuildContext context) => Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      Icon(icon, size: 14, color: style.color),
      const SizedBox(width: 4),
      Text(text, style: style),
    ],
  );
}

/* ------------------------------------------------------------------ glossary teaser */

const List<String> _teaserTerms = [
  'Pip',
  'Spread',
  'Leverage',
  'Margin level',
  'Stop-out',
  'Swap',
  'Support',
  'RSI',
  'CPI',
  'Yield curve',
  'Drawdown',
  'Expectancy',
];

String _glossaryHref(String q) => q.trim().isEmpty ? '/academy/glossary' : '/academy/glossary?q=${Uri.encodeQueryComponent(q.trim())}';

class _GlossaryTeaser extends StatefulWidget {
  const _GlossaryTeaser();

  @override
  State<_GlossaryTeaser> createState() => _GlossaryTeaserState();
}

class _GlossaryTeaserState extends State<_GlossaryTeaser> {
  final _c = TextEditingController();

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(title: t('academy.home.glossary'), subtitle: t('academy.glossaryTeaser.subtitle'), icon: LucideIcons.library),
          const SizedBox(height: 14),
          Container(
            height: 40,
            decoration: BoxDecoration(
              color: k.surface2,
              borderRadius: BorderRadius.circular(20),
              border: Border.all(color: k.line),
            ),
            child: Row(
              children: [
                Padding(
                  padding: const EdgeInsetsDirectional.only(start: 13, end: 4),
                  child: Icon(LucideIcons.search, size: 15, color: k.fg3),
                ),
                Expanded(
                  child: Semantics(
                    label: t('academy.glossaryTeaser.aria'),
                    textField: true,
                    child: TextField(
                      controller: _c,
                      textInputAction: TextInputAction.search,
                      autocorrect: false,
                      onSubmitted: (v) => context.go(_glossaryHref(v)),
                      style: context.text.callout.copyWith(fontSize: 14),
                      cursorColor: k.ember,
                      decoration: InputDecoration(
                        isCollapsed: true,
                        border: InputBorder.none,
                        hintText: t('academy.glossaryTeaser.placeholder'),
                        hintStyle: context.text.callout.copyWith(fontSize: 14, color: k.fg3),
                        contentPadding: const EdgeInsets.symmetric(horizontal: 6, vertical: 11),
                      ),
                    ),
                  ),
                ),
              ],
            ),
          ),
          const SizedBox(height: 14),
          Wrap(
            spacing: 6,
            runSpacing: 6,
            children: [
              for (final term in _teaserTerms)
                KPressable(
                  minSize: 36,
                  semanticLabel: term,
                  onTap: () => context.go(_glossaryHref(term)),
                  child: Container(
                    padding: const EdgeInsets.symmetric(horizontal: 11, vertical: 5),
                    decoration: BoxDecoration(
                      color: k.surface2,
                      borderRadius: BorderRadius.circular(14),
                      border: Border.all(color: k.line),
                    ),
                    child: Text(
                      term,
                      style: context.text.caption.copyWith(color: k.fg2, fontWeight: FontWeight.w400, fontSize: 12),
                    ),
                  ),
                ),
            ],
          ),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ certificates */

class _CertificatesCard extends StatelessWidget {
  const _CertificatesCard({required this.cat});
  final AcademyCatalog cat;

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    final certs = cat.phases.where((p) => p.certificate != null).toList();
    final ready = cat.phases.where((p) => p.certificate == null && (p.exam?.unlocked ?? false)).firstOrNull;
    final rtl = Directionality.of(context) == TextDirection.rtl;
    Widget row({required Widget badge, required String title, required String sub, required VoidCallback onTap, bool arrow = false, bool mono = false}) =>
        KPressable(
          onTap: onTap,
          semanticLabel: title,
          child: Container(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 10),
            decoration: BoxDecoration(
              color: k.surface2,
              borderRadius: BorderRadius.circular(k.rowRadius),
              border: Border.all(color: k.line),
            ),
            child: Row(
              children: [
                badge,
                const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(title, maxLines: 1, overflow: TextOverflow.ellipsis, style: context.text.label),
                      const SizedBox(height: 1),
                      Text(
                        sub,
                        style: context.text.caption.copyWith(color: k.fg3, fontWeight: FontWeight.w400, fontFeatures: kTabular),
                      ),
                    ],
                  ),
                ),
                if (arrow) Icon(rtl ? LucideIcons.arrowLeft : LucideIcons.arrowRight, size: 16, color: k.fg3),
              ],
            ),
          ),
        );
    return KCard(
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          KCardHeader(
            title: t('academy.certs.title'),
            subtitle: t('academy.certs.subtitle'),
            icon: LucideIcons.award,
            action: KTextButton(label: t('common.viewAll'), onPressed: () => context.go('/academy/progress')),
          ),
          const SizedBox(height: 14),
          for (final p in certs.take(3)) ...[
            row(
              badge: const SoftBadge(icon: LucideIcons.circleCheck, tone: KChipTone.up, size: 32),
              title: t('academy.phaseTitle', {'n': p.order, 'title': p.title}),
              sub: '${p.certificate!.code} · ${fmtDay(t, p.certificate!.issuedAt)}',
              onTap: () => context.go('/academy/progress'),
            ),
            const SizedBox(height: 8),
          ],
          if (ready != null)
            row(
              badge: const SoftBadge(icon: LucideIcons.graduationCap, tone: KChipTone.gold, size: 32),
              title: t('academy.certs.examReady', {'n': ready.order}),
              sub: t('academy.certs.examMeta', {'count': ready.exam!.questions, 'pass': ready.exam!.passMark}),
              arrow: true,
              onTap: () => context.push('/academy/phase/${ready.slug}/exam'),
            ),
          if (certs.isEmpty && ready == null) Text(t('academy.certs.empty'), style: context.text.footnote.copyWith(color: k.fg3, height: 1.5)),
        ],
      ),
    );
  }
}

/* ------------------------------------------------------------------ practise */

class _PracticeCard extends StatelessWidget {
  const _PracticeCard();

  @override
  Widget build(BuildContext context) {
    final t = context.t;
    final k = context.k;
    return KCard(
      padding: EdgeInsets.zero,
      child: Stack(
        children: [
          const Positioned.fill(child: FinishWash(finish: KCardFinish.standard, strength: 0.3)),
          Padding(
            padding: const EdgeInsets.all(22),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                KChip(label: t('academy.practiceCard.chip'), tone: KChipTone.gold),
                const SizedBox(height: 12),
                Text(t('academy.practice.inTrader'), style: context.text.title2.copyWith(fontSize: 19, fontWeight: FontWeight.w500)),
                const SizedBox(height: 6),
                Text(t('academy.practiceCard.text'), style: context.text.footnote.copyWith(color: k.fg2, fontSize: 13, height: 1.5)),
                const SizedBox(height: 20),
                const PracticeButton(),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

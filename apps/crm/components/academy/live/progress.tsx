"use client";

import * as React from "react";
import Link from "next/link";
import { Award, BookOpen, Clock, Copy, Download, Flame, GraduationCap, ShieldCheck, Target } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, KpiCard, PageHeader, Progress, Reveal, cn } from "@/components/kit";
import { tr, useT } from "@kalks/i18n/react";
import { LEVEL_TONE, fmtDay, fmtMin, isElective, levelLabel, pct, trackShort, trackTallies, tracksOf, useAcademy, type Catalog, type Certificate } from "./api";
import { AcademyUnavailable, BackLink, PageSkeleton } from "./shared";
import { CertificateCard } from "./book";

function CertificateTile({ c }: { c: Certificate }) {
  const t = useT();
  const img = `/api/academy/certificates/${c.code}/image`;
  return (
    <Card className="flex flex-col overflow-hidden rounded-[26px]" data-testid="certificate-tile">
      {/* the certificate as paper (the full image opens on click and downloads below) */}
      <a href={img} target="_blank" rel="noopener" aria-label={t("academy.cert.alt", { n: c.phase_order })} className="block p-4 pb-0 transition-transform hover:-translate-y-0.5">
        <CertificateCard c={c} />
      </a>
      <div className="flex flex-1 flex-col p-5">
        <div className="flex flex-wrap items-center gap-2">
          <Chip tone={LEVEL_TONE[c.level]}>{levelLabel(c.level)}</Chip>
          <Chip tone="up" dot>
            {t("academy.cert.examScore", { pct: c.score_pct })}
          </Chip>
        </div>
        <div className="mt-2 text-[15px] font-medium">
          {t("academy.phaseTitle", { n: c.phase_order, title: c.phase_title })}
        </div>
        <div className="k-num text-[12px] text-fg-3">
          {t("academy.cert.codeIssued", { code: c.code, date: fmtDay(c.issued_at) })}
        </div>
        <div className="mt-auto flex flex-wrap gap-2 pt-4">
          <a href={`${img}?download=1`}>
            <Button size="sm" variant="surface">
              <Download /> {t("common.download")}
            </Button>
          </a>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              navigator.clipboard?.writeText(c.verify_url).then(
                () => toast.success(tr("academy.toast.linkCopied")),
                () => toast.error(tr("academy.toast.copyFailed")),
              );
            }}
          >
            <Copy /> {t("academy.cert.copyLink")}
          </Button>
          <Link href={`/certificate/${c.code}`} target="_blank">
            <Button size="sm" variant="ghost">
              <ShieldCheck /> {t("academy.cert.verify")}
            </Button>
          </Link>
        </div>
      </div>
    </Card>
  );
}

export function LiveProgress() {
  const t = useT();
  const cat = useAcademy<Catalog>("catalog");
  const certs = useAcademy<{ certificates: Certificate[] }>("me/certificates");
  if (cat.error) return <AcademyUnavailable error={cat.error} onRetry={cat.reload} />;
  if (!cat.data) return <PageSkeleton />;
  const { me, phases } = cat.data;
  // one column per track any phase has (core tracks, then product tracks such as options)
  const tracks = tracksOf(phases.flatMap((p) => p.sections));
  return (
    <div className="pb-16">
      <BackLink href="/academy">{t("academy.title")}</BackLink>
      <PageHeader title={t("academy.home.myProgress")} subtitle={t("academy.progress.subtitle")} />
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label={t("academy.progress.chaptersComplete")} icon={<BookOpen />} value={<span className="k-num">{me.chapters_done} / {me.chapters_total}</span>} chip={t("academy.progress.ofCourse", { pct: pct(me.chapters_done, me.chapters_total) })} chipTone="ember" />
        <KpiCard label={t("academy.certs.title")} icon={<Award />} value={<span className="k-num">{me.certificates} / {phases.length}</span>} chip={t("academy.progress.onePerPhase")} chipTone="up" delay={0.05} />
        <KpiCard label={t("academy.stats.quizAvg")} icon={<Target />} value={<span className="k-num">{me.quiz_avg === null ? "–" : `${me.quiz_avg}%`}</span>} chip={t("academy.progress.bestPerChapter")} delay={0.1} />
        <KpiCard label={t("academy.stats.streak")} icon={<Flame />} value={<span className="k-num">{t("academy.progress.streakDays", { count: me.streak })}</span>} chip={t("academy.progress.studied", { time: fmtMin(me.minutes_done) })} delay={0.15} />
      </div>

      <Reveal delay={0.1} className="mt-4">
        <Card>
          <CardHeader title={t("academy.progress.byPhase")} subtitle={t("academy.progress.byPhaseSub")} icon={<GraduationCap />} />
          <div className="overflow-x-auto px-2 pb-4 pt-3 sm:px-4">
            <table className={cn("w-full text-[13px]", tracks.length > 2 ? "min-w-[860px]" : "min-w-[720px]")} data-testid="progress-table">
              <thead>
                <tr className="text-start text-[11.5px] uppercase tracking-wider text-fg-3">
                  <th className="px-3 py-2 font-medium">{t("academy.progress.col.phase")}</th>
                  {tracks.map((tk) => (
                    <th key={tk} className="px-3 py-2 font-medium">
                      {trackShort(t, tk)}
                    </th>
                  ))}
                  <th className="px-3 py-2 font-medium">{t("academy.progress.col.exam")}</th>
                  <th className="px-3 py-2 font-medium">{t("academy.progress.col.certificate")}</th>
                </tr>
              </thead>
              <tbody>
                {phases.map((p) => {
                  const tally = new Map(trackTallies(p.sections).map((x) => [x.track, x]));
                  return (
                    <tr key={p.slug} className="border-t border-line">
                      <td className="px-3 py-3">
                        <Link href={`/academy/phase/${p.slug}`} className="font-medium hover:text-ember">
                          {p.order}. {p.title}
                        </Link>
                        <div className="text-[11.5px] text-fg-3">
                          {levelLabel(p.level)}
                          {isElective(p) && ` · ${t("academy.elective")}`}
                        </div>
                      </td>
                      {tracks.map((tk) => {
                        const x = tally.get(tk);
                        // a phase without this track (e.g. the options elective has no technical section)
                        if (!x)
                          return (
                            <td key={tk} className="px-3 py-3 text-fg-3">
                              –
                            </td>
                          );
                        return (
                          <td key={tk} className="px-3 py-3">
                            <div className="k-num mb-1 text-[12px] text-fg-2">
                              {x.done}/{x.total}
                            </div>
                            <Progress value={pct(x.done, x.total)} tone={x.done === x.total && x.total > 0 ? "up" : "ember"} className="max-w-[140px]" />
                          </td>
                        );
                      })}
                      <td className="px-3 py-3">
                        {p.exam?.passed ? (
                          <Chip tone="up">{t("academy.progress.examPassed", { pct: p.exam.best_pct })}</Chip>
                        ) : p.exam?.unlocked ? (
                          <Link href={`/academy/phase/${p.slug}/exam`}>
                            <Chip tone="gold">{t("academy.progress.ready")}</Chip>
                          </Link>
                        ) : p.exam?.attempts ? (
                          <Chip tone="warn">{t("academy.progress.best", { pct: p.exam.best_pct })}</Chip>
                        ) : (
                          <span className="text-fg-3">{t("academy.exam.locked")}</span>
                        )}
                      </td>
                      <td className={cn("px-3 py-3", !p.certificate && "text-fg-3")}>
                        {p.certificate ? <span className="k-num text-fg-2">{p.certificate.code}</span> : "–"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </Card>
      </Reveal>

      <Reveal delay={0.15} className="mt-8">
        <h2 className="text-[19px] font-medium tracking-tight">{t("academy.certs.title")}</h2>
        <p className="mb-4 text-[13px] text-fg-3">{t("academy.progress.certsText")}</p>
        {certs.data && certs.data.certificates.length > 0 ? (
          <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
            {certs.data.certificates.map((c) => (
              <CertificateTile key={c.code} c={c} />
            ))}
          </div>
        ) : (
          <Card className="flex flex-col items-center gap-2 py-12 text-center">
            <Award className="size-7 text-fg-3" />
            <div className="text-[14.5px] font-medium">{t("academy.progress.noCerts")}</div>
            <p className="max-w-md text-[13px] text-fg-3">{t("academy.progress.noCertsText")}</p>
            <Link href="/academy" className="mt-2">
              <Button variant="surface" size="sm">
                <Clock /> {t("academy.hero.continue")}
              </Button>
            </Link>
          </Card>
        )}
      </Reveal>
    </div>
  );
}

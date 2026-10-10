"use client";

// The Academy book reader (founder 2026-10-10: "as I open a book … show the user reading from the book, like a
// Kindle: page change, proper book opening and closing animations").
//
// A phase is a hardcover book. Opening one flies the cover from the shelf to the middle of the screen, then the
// cover swings open around the spine onto the first spread (or the page you stopped at). Pages are cream paper:
// desktop shows a two-page spread, phones one page. Chapters flow into pages with CSS columns (./book-flow), and a
// page turn is a real leaf rotating around the spine in 3D with its front and back faces, light and cast shadows.
// Closing reverses it: the cover swings shut and the book flies back to its place on the shelf.
//
// Content and progress use the same Academy API as the chapter page: GET chapters/{slug} (loaded lazily: the
// chapters on screen plus the next one), POST chapters/{slug}/progress with the furthest page read as read_pct,
// the chapter quiz is the reader's own ChapterQuiz, and the phase exam links to /academy/phase/{slug}/exam.

import * as React from "react";
import { createPortal } from "react-dom";
import Link from "next/link";
import { AnimatePresence, animate, motion, useMotionValue, useReducedMotion, useTransform, type AnimationPlaybackControls, type MotionValue } from "motion/react";
import { ArrowRight, Check, ChevronLeft, ChevronRight, GraduationCap, LayoutList, List, Lock, RotateCw, X } from "lucide-react";
import { Button, LogoMark, cn } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { readCached, writeCached } from "@kalks/ui/swr-cache";
import { useSession } from "@/components/session";
import { academyApi, fmtMin, isElective, levelLabel, pct, trackLabel, type Catalog, type ChapterCard, type ChapterView, type PhaseT, type QuizReply, type SectionT } from "./api";
import { CertificateCard, CoverFace } from "./book";
import { FlowWindow, Measure, PAPER_CSS, PAPER_VARS, computeLayout, wordsPerPage, type Layout } from "./book-flow";
import { ChapterQuiz } from "./quiz";
import { PracticeButton } from "./shared";

/* ------------------------------------------------------------------ model ------------------------------------------------------------------ */

type Flat = ChapterCard & { section: SectionT; si: number; n: number };
type Pg =
  | { k: "endpaper"; back: boolean }
  | { k: "title" }
  | { k: "toc"; i: number }
  | { k: "ch"; c: number; p: number }
  | { k: "quiz"; c: number }
  | { k: "exam" }
  | { k: "blank" };
/** Where the reader is, independent of page counts (they change as chapters are measured or the window resizes). */
type Anchor = { k: "ch"; c: number; f: number } | { k: "key"; key: string };
type Face = Pg | "cover" | "paper" | null;
type Turn = { dir: 1 | -1; left: Pg | null; right: Pg | null; front: Face; back: Face };
type Stage = "fly-in" | "opening" | "open" | "closing" | "fly-out";
type TocItem = { kind: "part"; s: number } | { kind: "row"; c: number } | { kind: "exam" };
type Rect = { x: number; y: number; w: number; h: number };

const keyOf = (g: Pg) => (g.k === "ch" ? `ch:${g.c}:${g.p}` : g.k === "quiz" ? `quiz:${g.c}` : g.k === "toc" ? `toc:${g.i}` : g.k === "endpaper" ? `end:${g.back ? 1 : 0}` : g.k);
const FINISH: Record<PhaseT["level"], string> = { Beginner: "k-face-standard", Intermediate: "k-face-cent", Advanced: "k-face-pro", Professional: "k-face-vip" };
const finishOf = (p: PhaseT) => (isElective(p) ? "k-face-options" : (FINISH[p.level] ?? "k-face-standard"));
const EASE_TURN = [0.45, 0.05, 0.3, 1] as const;
const EASE_OPEN = [0.6, 0.02, 0.22, 1] as const;
const EASE_FLY = [0.22, 1, 0.36, 1] as const;
/** page counts measured this session, per chapter and page size (reopening a book keeps its page numbers) */
const MEASURED = new Map<string, number>();

/** Contents sizes: rows shrink (down to 28 px) so the contents fit on one page when they can. */
function tocSizes(L: Layout, phase: PhaseT, chapters: number) {
  const head = L.mode === "spread" ? 104 : 92;
  const part = L.mode === "spread" ? 52 : 48;
  const parts = phase.sections.length;
  const fixed = head + parts * part + (phase.exam ? 18 : 0);
  const rows = chapters + (phase.exam ? 1 : 0);
  const row = Math.max(26, Math.min(34, Math.floor((L.ch - fixed) / Math.max(1, rows))));
  return { head, part, row, exam: row + 18 };
}

/** Contents split into pages: rows have fixed heights, so the split is exact. */
function paginateToc(phase: PhaseT, flat: Flat[], L: Layout): TocItem[][] {
  const z = tocSizes(L, phase, flat.length);
  const items: { it: TocItem; h: number }[] = [];
  const multi = phase.sections.length > 1;
  phase.sections.forEach((s, si) => {
    if (multi || s.title) items.push({ it: { kind: "part", s: si }, h: z.part });
    flat.forEach((c, ci) => c.si === si && items.push({ it: { kind: "row", c: ci }, h: z.row }));
  });
  if (phase.exam) items.push({ it: { kind: "exam" }, h: z.exam });
  const pages: TocItem[][] = [[]];
  let room = L.ch - z.head;
  for (const { it, h } of items) {
    if (h > room && pages[pages.length - 1]!.length) {
      pages.push([]);
      room = L.ch;
    }
    pages[pages.length - 1]!.push(it);
    room -= h;
  }
  return pages;
}

function buildSeq(flat: Flat[], tocN: number, hasExam: boolean, counts: number[], spread: boolean): Pg[] {
  const s: Pg[] = [];
  if (spread) s.push({ k: "endpaper", back: false });
  s.push({ k: "title" });
  for (let i = 0; i < tocN; i++) s.push({ k: "toc", i });
  flat.forEach((c, ci) => {
    for (let p = 0; p < counts[ci]!; p++) s.push({ k: "ch", c: ci, p });
    if (c.questions > 0) s.push({ k: "quiz", c: ci });
  });
  if (hasExam) s.push({ k: "exam" });
  if (spread) {
    if (s.length % 2 === 0) s.push({ k: "blank" });
    s.push({ k: "endpaper", back: true });
  }
  return s;
}

/** Where a book opens: the chapter the learner is on (catalog `continue`, else one in progress), at the page they
 *  reached; a fresh or finished book opens at the title page. */
function resumeOf(cat: Catalog, phase: PhaseT, flat: Flat[]): { anchor: Anchor; wait: number | null } {
  const cont = cat.me.continue && cat.me.continue.phase.slug === phase.slug && cat.me.continue.started ? cat.me.continue.slug : null;
  let ci = cont ? flat.findIndex((c) => c.slug === cont) : -1;
  if (ci < 0) ci = flat.findIndex((c) => !c.progress.completed && c.progress.read_pct > 0);
  if (ci < 0 && flat.some((c) => c.progress.completed)) ci = flat.findIndex((c) => !c.progress.completed);
  if (ci < 0) return { anchor: { k: "key", key: "title" }, wait: null };
  const read = flat[ci]!.progress.read_pct;
  if (read >= 100 && flat[ci]!.questions > 0) return { anchor: { k: "key", key: `quiz:${ci}` }, wait: ci };
  return { anchor: { k: "ch", c: ci, f: Math.max(0, Math.min(0.999, read / 100 - 0.001)) }, wait: ci };
}

/* ------------------------------------------------------------------ context ------------------------------------------------------------------ */

type Ctx = {
  L: Layout;
  phase: PhaseT;
  flat: Flat[];
  seq: Pg[];
  index: Map<string, number>;
  counts: number[];
  toc: TocItem[][];
  views: Record<string, ChapterView>;
  failed: Record<string, boolean>;
  done: Record<string, { completed: boolean; best: number | null; total: number | null }>;
  read: Record<string, number>;
  current: number | null;
  learner: string;
  examUnlocked: boolean;
  folio: (i: number) => number;
  goTo: (i: number) => void;
  openQuiz: (c: number) => void;
  retry: (c: number) => void;
};
const ReaderCtx = React.createContext<Ctx | null>(null);
const useCtx = () => React.useContext(ReaderCtx)!;

/* ------------------------------------------------------------------ paper & pages ------------------------------------------------------------------ */

function Paper({ side, children, header, folio, className }: { side: "left" | "right" | "single"; children?: React.ReactNode; header?: React.ReactNode; folio?: React.ReactNode; className?: string }) {
  const { L } = useCtx();
  const padL = side === "right" ? L.padIn : L.padOut;
  const padR = side === "left" ? L.padIn : L.padOut;
  return (
    <div className={cn("kb-paper absolute inset-0 overflow-hidden [container-type:inline-size]", side === "left" ? "rounded-s-[3px]" : side === "right" ? "rounded-e-[3px]" : "rounded-[3px]", className)} style={PAPER_VARS}>
      <div aria-hidden className="kb-grain absolute inset-0" />
      {side === "left" && <div aria-hidden className="absolute inset-y-0 end-0 w-[16%] bg-[linear-gradient(to_left,rgba(70,45,10,.28),rgba(70,45,10,.09)_20%,transparent)] rtl:bg-[linear-gradient(to_right,rgba(70,45,10,.28),rgba(70,45,10,.09)_20%,transparent)]" />}
      {side === "right" && <div aria-hidden className="absolute inset-y-0 start-0 w-[16%] bg-[linear-gradient(to_right,rgba(70,45,10,.28),rgba(70,45,10,.09)_20%,transparent)] rtl:bg-[linear-gradient(to_left,rgba(70,45,10,.28),rgba(70,45,10,.09)_20%,transparent)]" />}
      {side === "single" && <div aria-hidden className="absolute inset-y-0 start-0 w-[6%] bg-[linear-gradient(to_right,rgba(70,45,10,.2),transparent)] rtl:bg-[linear-gradient(to_left,rgba(70,45,10,.2),transparent)]" />}
      {header != null && (
        <div className="absolute flex items-center justify-center" style={{ top: 0, height: L.padT * 0.8, left: padL, right: padR }}>
          <span className="truncate font-sans text-[9.5px] font-semibold uppercase tracking-[.22em] text-[#8a7858]">{header}</span>
        </div>
      )}
      <div className="absolute" style={{ top: L.padT, left: padL, width: L.cw, height: L.ch }}>
        {children}
      </div>
      {folio != null && (
        <div className="k-num absolute text-center font-serif text-[12px] text-[#7a6950]" style={{ bottom: Math.round(L.padB * 0.3), left: padL, right: padR }}>
          {folio}
        </div>
      )}
    </div>
  );
}

function Endpaper({ back }: { back: boolean }) {
  const { phase, learner } = useCtx();
  const t = useT();
  return (
    <div className={cn("absolute inset-0 overflow-hidden", back ? "rounded-e-[3px]" : "rounded-s-[3px]", finishOf(phase))}>
      <div aria-hidden className="absolute inset-0 bg-black/45" />
      <div aria-hidden className="absolute inset-0 opacity-[.16] [background-image:radial-gradient(circle_at_center,rgba(255,255,255,.9)_0_1.2px,transparent_1.6px),radial-gradient(circle_at_center,rgba(255,255,255,.5)_0_.8px,transparent_1.2px)] [background-position:0_0,11px_11px] [background-size:22px_22px]" />
      <div aria-hidden className="absolute inset-0 bg-[radial-gradient(90%_70%_at_50%_40%,transparent,rgba(0,0,0,.45))]" />
      {!back ? (
        // ex libris bookplate
        <div className="absolute inset-0 grid place-items-center p-[12%]">
          <div className="w-full max-w-[260px] rounded-[6px] bg-[#f6f1e6] p-[6px] text-center text-[#2b2216] shadow-[0_10px_30px_-12px_rgba(0,0,0,.8)]" style={PAPER_VARS}>
            <div className="rounded-[3px] border border-[#b8893a]/70 px-4 py-5">
              <LogoMark size={20} className="mx-auto text-[#2b2216]" />
              <div className="mt-2 font-sans text-[9.5px] font-bold uppercase tracking-[.32em] text-[#7a5a22]">{t.dyn("academy.reader.exLibris", "Ex libris")}</div>
              <div className="mt-2 truncate border-b border-[#b8893a]/60 pb-1 font-serif text-[19px] italic leading-tight">{learner}</div>
              <div className="mt-2 font-sans text-[9px] uppercase tracking-[.2em] text-[#6b5a40]">{t.dyn("academy.book.series", "Kalks Academy")}</div>
            </div>
          </div>
        </div>
      ) : (
        <div className="absolute inset-x-0 bottom-[9%] flex flex-col items-center gap-2 text-white/70">
          <LogoMark size={22} className="text-white/80" />
          <div className="font-sans text-[9.5px] font-bold uppercase tracking-[.32em]">{t.dyn("academy.book.series", "Kalks Academy")}</div>
        </div>
      )}
    </div>
  );
}

function TitlePage({ side }: { side: "left" | "right" | "single" }) {
  const { phase, flat, done } = useCtx();
  const t = useT();
  const nDone = flat.filter((c) => done[c.slug]?.completed).length;
  return (
    <Paper side={side}>
      <div className="flex h-full flex-col items-center text-center">
        <div className="flex items-center gap-2 font-sans text-[10px] font-bold uppercase tracking-[.32em] text-[#7a5a22]">
          <LogoMark size={14} className="text-[#241c13]" />
          {t.dyn("academy.book.series", "Kalks Academy")}
        </div>
        <div className="mt-[12%] font-sans text-[10.5px] font-semibold uppercase tracking-[.3em] text-[#b04509]">{isElective(phase) ? t("academy.elective") : t("academy.phaseN", { n: phase.order })}</div>
        <div className="k-num mt-2 text-[clamp(54px,17cqw,112px)] font-extralight leading-[.9] tracking-[-.045em] text-[#241c13]">{String(phase.order).padStart(2, "0")}</div>
        <h1 className="mt-[6%] max-w-[92%] font-[family-name:var(--kb-serif)] text-[clamp(24px,6.6cqw,40px)] font-semibold leading-[1.08] tracking-[-.01em] text-[#1c150e]">{phase.title}</h1>
        <div aria-hidden className="mt-5 flex w-[42%] items-center gap-2 text-[#b04509]">
          <span className="h-px flex-1 bg-[rgba(74,52,20,.3)]" />
          <span className="text-[10px]">◆</span>
          <span className="h-px flex-1 bg-[rgba(74,52,20,.3)]" />
        </div>
        <p className="mt-4 max-w-[86%] font-[family-name:var(--kb-serif)] text-[clamp(13px,3.3cqw,16px)] italic leading-[1.5] text-[#4a3d2a]">{phase.summary}</p>
        <div className="mt-auto w-full">
          <div className="flex flex-wrap items-center justify-center gap-x-3 gap-y-1 font-sans text-[10px] font-semibold uppercase tracking-[.16em] text-[#7a6950]">
            <span>{levelLabel(phase.level)}</span>
            <span aria-hidden>·</span>
            <span className="k-num">{t("academy.phase.chapters", { count: flat.length })}</span>
            <span aria-hidden>·</span>
            <span className="k-num">{fmtMin(phase.minutes)}</span>
            {phase.exam && (
              <>
                <span aria-hidden>·</span>
                <span className="k-num">{t("academy.exam.final")}</span>
              </>
            )}
          </div>
          <div className="mx-auto mt-4 max-w-[78%]">
            <div className="mb-1.5 flex items-center justify-between font-sans text-[10.5px] text-[#7a6950]">
              <span className="k-num">{t("academy.phase.complete", { done: nDone, total: flat.length })}</span>
              <span className="k-num font-semibold text-[#241c13]">{pct(nDone, flat.length)}%</span>
            </div>
            <div className="h-[3px] overflow-hidden rounded-full bg-[rgba(74,52,20,.14)]">
              <div className="h-full rounded-full bg-[#c2500c]" style={{ width: `${pct(nDone, flat.length)}%` }} />
            </div>
          </div>
          {phase.certificate && (
            <div className="mt-3 inline-flex items-center gap-1.5 rounded-full border border-[#2f7d4f]/40 bg-[rgba(47,125,79,.08)] px-2.5 py-1 font-sans text-[10.5px] font-semibold text-[#2a6f46]">
              <Check className="size-3" strokeWidth={3} /> {t("academy.state.certified")} · <span className="k-num">{phase.certificate.code}</span>
            </div>
          )}
        </div>
      </div>
    </Paper>
  );
}

function StateMark({ state }: { state: "done" | "reading" | "new" }) {
  if (state === "done")
    return (
      <span className="grid size-[17px] shrink-0 place-items-center rounded-full bg-[#2f7d4f] text-[#f6f1e6]">
        <Check className="size-[11px]" strokeWidth={3.2} />
      </span>
    );
  if (state === "reading") return <span className="size-[17px] shrink-0 rounded-full border-[1.5px] border-[#c2500c] bg-[conic-gradient(#c2500c_0_50%,transparent_50%_100%)]" />;
  return <span className="size-[17px] shrink-0 rounded-full border-[1.5px] border-[rgba(74,52,20,.35)]" />;
}

function TocPage({ i, side, folio }: { i: number; side: "left" | "right" | "single"; folio: number }) {
  const c = useCtx();
  const t = useT();
  const z = tocSizes(c.L, c.phase, c.flat.length);
  const items = c.toc[i] ?? [];
  const chapterStart = (ci: number) => c.index.get(`ch:${ci}:0`) ?? 0;
  return (
    <Paper side={side} header={i > 0 ? t.dyn("academy.reader.contents", "Contents") : undefined} folio={folio}>
      {i === 0 && (
        <div style={{ height: z.head }} className="flex flex-col justify-start">
          <div className="font-sans text-[10px] font-bold uppercase tracking-[.26em] text-[#b04509]">{isElective(c.phase) ? t("academy.elective") : t("academy.phaseN", { n: c.phase.order })}</div>
          <h2 className="mt-1.5 font-[family-name:var(--kb-serif)] text-[34px] font-semibold leading-none tracking-[-.01em] text-[#1c150e]">{t.dyn("academy.reader.contents", "Contents")}</h2>
          <div aria-hidden className="mt-4 h-px w-16 bg-[#c2500c]" />
        </div>
      )}
      {items.map((it) => {
        if (it.kind === "part") {
          const s = c.phase.sections[it.s]!;
          return (
            <div key={`p${it.s}`} style={{ height: z.part }} className="flex flex-col justify-end pb-1.5">
              <div className="font-sans text-[9.5px] font-bold uppercase tracking-[.22em] text-[#b04509]">{trackLabel(t, s.track)}</div>
              <div className="truncate font-[family-name:var(--kb-serif)] text-[15px] font-semibold leading-tight text-[#1c150e]">{s.title}</div>
            </div>
          );
        }
        if (it.kind === "row") {
          const ch = c.flat[it.c]!;
          const st = c.done[ch.slug]?.completed ? "done" : (c.read[ch.slug] ?? 0) > 0 ? "reading" : "new";
          const here = c.current === it.c;
          return (
            <button
              key={`r${it.c}`}
              type="button"
              data-testid={`toc-${ch.slug}`}
              onClick={() => c.goTo(chapterStart(it.c))}
              style={{ height: z.row }}
              className={cn("group relative flex w-full items-center gap-2.5 text-start font-[family-name:var(--kb-serif)] text-[14.5px] text-[#2b2216] transition-colors hover:text-[#b04509]", here && "font-semibold")}
            >
              {here && <span aria-hidden className="absolute -start-[14px] top-1/2 h-[18px] w-[5px] -translate-y-1/2 rounded-e-[2px] bg-[#c2500c]" />}
              <StateMark state={st} />
              <span className="k-num w-5 shrink-0 font-sans text-[11.5px] text-[#8a7858]">{ch.n}</span>
              <span className="min-w-0 truncate">{ch.title}</span>
              <span aria-hidden className="h-[.95em] min-w-3 flex-1 border-b border-dotted border-[rgba(74,52,20,.45)]" />
              <span className="k-num shrink-0 font-sans text-[12px] text-[#5c4d38]">{c.folio(chapterStart(it.c))}</span>
            </button>
          );
        }
        const at = c.index.get("exam") ?? 0;
        return (
          <button key="exam" type="button" onClick={() => c.goTo(at)} style={{ height: z.exam }} className="flex w-full items-end gap-2.5 pb-[7px] text-start font-[family-name:var(--kb-serif)] text-[14.5px] font-semibold text-[#1c150e] hover:text-[#b04509]">
            <span className="grid size-[17px] shrink-0 place-items-center rounded-full bg-[#a6761d] text-[#f6f1e6]">
              <GraduationCap className="size-[11px]" strokeWidth={2.6} />
            </span>
            <span className="min-w-0 truncate">{t("academy.exam.phaseExam", { n: c.phase.order })}</span>
            <span aria-hidden className="mb-[5px] h-px min-w-3 flex-1 border-b border-dotted border-[rgba(74,52,20,.45)]" />
            <span className="k-num shrink-0 font-sans text-[12px] font-normal text-[#5c4d38]">{c.folio(at)}</span>
          </button>
        );
      })}
    </Paper>
  );
}

function PageSkeleton({ failed, onRetry }: { failed?: boolean; onRetry?: () => void }) {
  const t = useT();
  if (failed)
    return (
      <div className="grid h-full place-items-center text-center font-sans text-[13px] text-[#5c4d38]">
        <div>
          <p>{t("academy.unavailable.text")}</p>
          <button type="button" onClick={onRetry} className="mt-3 inline-flex items-center gap-1.5 rounded-full border border-[rgba(74,52,20,.3)] px-3 py-1.5 text-[12.5px] font-medium text-[#241c13] hover:bg-[rgba(120,90,40,.08)]">
            <RotateCw className="size-3.5" /> {t("common.retry")}
          </button>
        </div>
      </div>
    );
  return (
    <div className="space-y-3 pt-2" aria-busy>
      {[92, 100, 96, 100, 64, 0, 100, 97, 100, 88, 100, 45].map((w, i) => (w ? <div key={i} className="h-3 animate-pulse rounded-full bg-[rgba(120,90,40,.12)]" style={{ width: `${w}%` }} /> : <div key={i} className="h-3" />))}
    </div>
  );
}

function ChapterPage({ c: ci, p, side, folio }: { c: number; p: number; side: "left" | "right" | "single"; folio: number }) {
  const c = useCtx();
  const ch = c.flat[ci]!;
  const view = c.views[ch.slug];
  const header = side === "left" ? c.phase.title : ch.title;
  return (
    <Paper side={side} header={p === 0 ? undefined : header} folio={folio}>
      {view ? <FlowWindow view={view} n={ch.n} L={c.L} page={p} /> : <PageSkeleton failed={c.failed[ch.slug]} onRetry={() => c.retry(ci)} />}
    </Paper>
  );
}

function QuizPage({ c: ci, side, folio }: { c: number; side: "left" | "right" | "single"; folio: number }) {
  const c = useCtx();
  const t = useT();
  const ch = c.flat[ci]!;
  const st = c.done[ch.slug];
  const view = c.views[ch.slug];
  const next = c.flat[ci + 1];
  const nextAt = next ? c.index.get(`ch:${ci + 1}:0`) : c.index.get("exam");
  return (
    <Paper side={side} header={ch.title} folio={folio}>
      <div className="flex h-full flex-col">
        <div className="pt-[8%] font-sans text-[10px] font-bold uppercase tracking-[.24em] text-[#b04509]">{t.dyn("academy.reader.endOfChapter", "End of chapter {n}", { n: ch.n })}</div>
        <h2 className="mt-2 font-[family-name:var(--kb-serif)] text-[clamp(24px,6.5cqw,32px)] font-semibold leading-[1.1] tracking-[-.01em] text-[#1c150e]">{t("academy.quiz.eyebrow")}</h2>
        <p className="mt-3 font-[family-name:var(--kb-serif)] text-[15px] leading-[1.55] text-[#3b3125]">{t("academy.quiz.meta", { count: ch.questions })}</p>
        <div className={cn("mt-5 flex items-center gap-3 rounded-[10px] border px-3.5 py-3 font-sans text-[13px]", st?.completed ? "border-[#2f7d4f]/35 bg-[rgba(47,125,79,.08)] text-[#2a6f46]" : "border-[rgba(74,52,20,.2)] bg-[rgba(120,90,40,.06)] text-[#5c4d38]")}>
          {st?.completed ? <Check className="size-4 shrink-0" strokeWidth={3} /> : <GraduationCap className="size-4 shrink-0" />}
          <span>
            {st?.completed
              ? `${t("academy.quiz.passedComplete")}${st.best !== null && st.total ? ` ${t("academy.quiz.score", { score: st.best, total: st.total })}` : ""}`
              : st?.best !== null && st?.best !== undefined && st.total
                ? t.dyn("academy.reader.bestScore", "Best so far: {score}/{total}. Pass the quiz to complete this chapter.", { score: st.best, total: st.total })
                : t("academy.phase.chaptersText")}
          </span>
        </div>
        <div className="mt-5 flex flex-wrap items-center gap-2.5">
          <Button variant="ember" onClick={() => c.openQuiz(ci)} disabled={!view} data-testid="book-take-quiz">
            <GraduationCap /> {st?.completed ? t("academy.quiz.retake") : t.dyn("academy.reader.takeQuiz", "Take the quiz")}
          </Button>
          <Link href={`/academy/chapter/${ch.slug}`} className="font-sans text-[12.5px] font-medium text-[#b04509] underline decoration-[#b04509]/40 underline-offset-2 hover:decoration-[#b04509]">
            {t.dyn("academy.reader.chapterPage", "Open as a web page")}
          </Link>
        </div>
        {view?.chapter.practice && (
          <div className="mt-6 border-t border-[rgba(74,52,20,.16)] pt-4">
            <div className="font-sans text-[10px] font-bold uppercase tracking-[.2em] text-[#7a6950]">{t("academy.practice.inTrader")}</div>
            <div className="mt-2.5">
              <PracticeButton size="sm" label={t("academy.practice.openDemo")} />
            </div>
          </div>
        )}
        {nextAt !== undefined && (
          <button type="button" onClick={() => c.goTo(nextAt)} className="group mt-auto flex items-center gap-3 border-t border-[rgba(74,52,20,.16)] pt-4 text-start">
            <div className="min-w-0 flex-1">
              <div className="font-sans text-[10px] font-bold uppercase tracking-[.2em] text-[#7a6950]">{t("common.next")}</div>
              <div className="truncate font-[family-name:var(--kb-serif)] text-[15.5px] font-semibold text-[#1c150e] group-hover:text-[#b04509]">
                {next ? `${t.dyn("academy.reader.chapterN", "Chapter {n}", { n: next.n })} · ${next.title}` : t("academy.exam.phaseExam", { n: c.phase.order })}
              </div>
            </div>
            <ArrowRight className="size-4 shrink-0 text-[#b04509] rtl:-scale-x-100" />
          </button>
        )}
      </div>
    </Paper>
  );
}

function ExamPage({ side, folio }: { side: "left" | "right" | "single"; folio: number }) {
  const c = useCtx();
  const t = useT();
  const p = c.phase;
  const e = p.exam!;
  const nDone = c.flat.filter((x) => c.done[x.slug]?.completed).length;
  const left = c.flat.length - nDone;
  const oneTrack = new Set(p.sections.map((s) => s.track)).size < 2;
  return (
    <Paper side={side} header={p.title} folio={folio}>
      <div className="flex h-full flex-col">
        <div className="pt-[6%] font-sans text-[10px] font-bold uppercase tracking-[.24em] text-[#b04509]">{t("academy.exam.final")}</div>
        <h2 className="mt-2 font-[family-name:var(--kb-serif)] text-[clamp(24px,6.5cqw,32px)] font-semibold leading-[1.1] text-[#1c150e]">{t("academy.exam.phaseExam", { n: p.order })}</h2>
        <div className="mt-4 grid grid-cols-3 gap-2 font-sans">
          {(
            [
              [String(e.questions), t("academy.exam.questions")],
              [`${e.pass_mark}%`, t("academy.exam.passMark")],
              [e.best_pct === null ? "–" : `${e.best_pct}%`, t("academy.exam.bestScore")],
            ] as const
          ).map(([v, l]) => (
            <div key={l} className="rounded-[8px] border border-[rgba(74,52,20,.18)] bg-[rgba(120,90,40,.05)] px-2.5 py-2">
              <div className="k-num text-[15px] font-semibold text-[#241c13]">{v}</div>
              <div className="text-[10px] text-[#7a6950]">{l}</div>
            </div>
          ))}
        </div>
        <p className="mt-4 font-[family-name:var(--kb-serif)] text-[14.5px] leading-[1.55] text-[#3b3125]">
          {e.passed ? t("academy.exam.passedText") : c.examUnlocked ? t(oneTrack ? "academy.exam.unlockedTextOneTrack" : "academy.exam.unlockedText") : t("academy.exam.lockedText", { count: left })}
        </p>
        <div className="mt-4 flex flex-wrap items-center gap-2.5">
          {c.examUnlocked ? (
            <Link href={`/academy/phase/${p.slug}/exam`} data-testid="book-exam-link">
              <Button variant={e.passed ? "surface" : "ember"}>
                <GraduationCap /> {e.passed ? t("academy.exam.retake") : e.attempts ? t("common.retry") : t("academy.exam.start")}
              </Button>
            </Link>
          ) : (
            <Button variant="surface" disabled>
              <Lock /> {t("academy.exam.locked")}
            </Button>
          )}
        </div>
        <div className="mt-auto">
          {p.certificate ? (
            <CertificateCard c={{ code: p.certificate.code, phase_order: p.order, phase_title: p.title, score_pct: e.best_pct ?? 0, issued_at: p.certificate.issued_at, learner_name: c.learner }} className="shadow-[0_14px_30px_-18px_rgba(40,25,5,.7)] ring-1 ring-[rgba(74,52,20,.2)]" />
          ) : (
            <div className="font-sans">
              <div className="mb-1.5 flex items-center justify-between text-[11px] text-[#7a6950]">
                <span className="k-num">{t("academy.phase.complete", { done: nDone, total: c.flat.length })}</span>
                <span className="k-num font-semibold text-[#241c13]">{pct(nDone, c.flat.length)}%</span>
              </div>
              <div className="h-[3px] overflow-hidden rounded-full bg-[rgba(74,52,20,.14)]">
                <div className="h-full rounded-full bg-[#c2500c]" style={{ width: `${pct(nDone, c.flat.length)}%` }} />
              </div>
              <p className="mt-3 text-[12px] leading-relaxed text-[#7a6950]">{t("academy.cert.placeholder", { n: p.order })}</p>
            </div>
          )}
        </div>
      </div>
    </Paper>
  );
}

/** One page of the book, whatever it is. */
function PageFace({ pg, side }: { pg: Pg; side: "left" | "right" | "single" }) {
  const c = useCtx();
  const i = c.index.get(keyOf(pg)) ?? 0;
  const folio = c.folio(i);
  switch (pg.k) {
    case "endpaper":
      return <Endpaper back={pg.back} />;
    case "title":
      return <TitlePage side={side} />;
    case "toc":
      return <TocPage i={pg.i} side={side} folio={folio} />;
    case "ch":
      return <ChapterPage c={pg.c} p={pg.p} side={side} folio={folio} />;
    case "quiz":
      return <QuizPage c={pg.c} side={side} folio={folio} />;
    case "exam":
      return <ExamPage side={side} folio={folio} />;
    case "blank":
      return <Paper side={side} />;
  }
}

/* ------------------------------------------------------------------ the turning leaf ------------------------------------------------------------------ */

function LeafFace({ face, side }: { face: Face; side: "left" | "right" | "single" }) {
  const { phase } = useCtx();
  if (face === "cover") return <CoverFace p={phase} className="absolute inset-0 !shadow-none" />;
  if (face === "paper" || face === null)
    return (
      <div className="kb-paper absolute inset-0 rounded-[3px]">
        <div className="kb-grain absolute inset-0" />
      </div>
    );
  return <PageFace pg={face} side={side} />;
}

function Leaf({ rot, front, back, spread, bleed }: { rot: MotionValue<number>; front: Face; back: Face; spread: boolean; bleed: number }) {
  const { L, phase } = useCtx();
  const cover = front === "cover";
  const left = spread ? L.pw : 0;
  const top = cover ? -bleed : 0;
  const w = cover ? L.pw + bleed : L.pw;
  const h = cover ? L.ph + 2 * bleed : L.ph;
  const frontShade = useTransform(rot, [0, -90], [0, 0.55]);
  const sheen = useTransform(rot, [0, -40, -90], [0, 0.5, 0]);
  const backShade = useTransform(rot, [-90, -180], [0.55, 0]);
  const backSheen = useTransform(rot, [-90, -140, -180], [0, 0.35, 0]);
  const shadow = useTransform(rot, (r) => {
    const s = Math.sin((Math.min(180, Math.max(0, -r)) / 180) * Math.PI);
    return `0 0 ${Math.round(8 + s * 34)}px rgba(0,0,0,${(s * 0.32).toFixed(3)})`;
  });
  // one page at a time: the turned page leaves the screen to the left
  const fade = useTransform(rot, [-105, -170], [1, 0]);
  return (
    <motion.div
      className="absolute z-30"
      style={{ left, top, width: w, height: h, rotateY: rot, transformOrigin: "0% 50%", transformStyle: "preserve-3d", opacity: spread ? 1 : fade }}
      data-testid="book-leaf"
    >
      <motion.div className="absolute inset-0 overflow-hidden rounded-e-[10px] [backface-visibility:hidden]" style={{ boxShadow: shadow }}>
        <LeafFace face={front} side={spread ? "right" : "single"} />
        <motion.div aria-hidden className="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,rgba(0,0,0,.04),rgba(0,0,0,.42))]" style={{ opacity: frontShade }} />
        <motion.div aria-hidden className="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,transparent_45%,rgba(255,255,255,.55)_78%,transparent_96%)]" style={{ opacity: sheen }} />
      </motion.div>
      <motion.div className="absolute inset-0 overflow-hidden rounded-s-[10px] [backface-visibility:hidden] [transform:rotateY(180deg)]" style={{ boxShadow: shadow }}>
        {cover && spread ? (
          // the inside of the front board: board rim, then the left page
          <div className={cn("absolute inset-0", finishOf(phase))}>
            <div className="absolute inset-0 bg-black/30" />
            <div className="absolute" style={{ left: bleed, top: bleed, width: L.pw, height: L.ph }}>
              <LeafFace face={back} side="left" />
            </div>
          </div>
        ) : cover ? (
          <div className={cn("absolute inset-0", finishOf(phase))}>
            <div className="absolute inset-0 bg-black/40" />
          </div>
        ) : (
          <LeafFace face={back} side={spread ? "left" : "single"} />
        )}
        <motion.div aria-hidden className="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,rgba(0,0,0,.42),rgba(0,0,0,.04))]" style={{ opacity: backShade }} />
        <motion.div aria-hidden className="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,transparent_4%,rgba(255,255,255,.45)_22%,transparent_55%)]" style={{ opacity: backSheen }} />
      </motion.div>
    </motion.div>
  );
}

/* ------------------------------------------------------------------ the reader ------------------------------------------------------------------ */

const nextFrame = () => new Promise<void>((r) => requestAnimationFrame(() => requestAnimationFrame(() => r())));
const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function faceRect(el: HTMLElement | null): Rect | null {
  if (!el || !el.isConnected) return null;
  const face = (el.matches("[data-book-face]") ? el : el.querySelector("[data-book-face]")) as HTMLElement | null;
  const r = (face ?? el).getBoundingClientRect();
  if (r.width < 4 || r.bottom < 0 || r.top > window.innerHeight || r.right < 0 || r.left > window.innerWidth) return null;
  return { x: r.left, y: r.top, w: r.width, h: r.height };
}

export function BookReader({ cat, phase, origin, fromUrl, onClosed }: { cat: Catalog; phase: PhaseT; origin: HTMLElement | null; fromUrl?: boolean; onClosed: () => void }) {
  const t = useT();
  const me = useSession();
  const reduced = !!useReducedMotion();
  const flat = React.useMemo<Flat[]>(() => {
    let n = 0;
    return phase.sections.flatMap((s, si) => s.chapters.map((c) => ({ ...c, section: s, si, n: ++n })));
  }, [phase]);

  /* layout */
  const [L, setL] = React.useState<Layout>(() => computeLayout(window.innerWidth, window.innerHeight));
  React.useEffect(() => {
    let tm: ReturnType<typeof setTimeout> | null = null;
    const on = () => {
      if (tm) clearTimeout(tm);
      tm = setTimeout(() => setL((prev) => {
        const next = computeLayout(window.innerWidth, window.innerHeight);
        return next.key === prev.key && next.vw === prev.vw && next.vh === prev.vh ? prev : next;
      }), 120);
    };
    window.addEventListener("resize", on);
    return () => {
      window.removeEventListener("resize", on);
      if (tm) clearTimeout(tm);
    };
  }, []);
  const spread = L.mode === "spread";
  const bleed = spread ? 8 : 5;

  /* chapters: lazy content + page counts */
  const [views, setViews] = React.useState<Record<string, ChapterView>>({});
  const [failed, setFailed] = React.useState<Record<string, boolean>>({});
  const [measured, setMeasured] = React.useState<Record<string, number>>(() => Object.fromEntries(MEASURED));
  const loading = React.useRef(new Set<string>());
  const viewsRef = React.useRef(views);
  viewsRef.current = views;
  const ensure = React.useCallback(
    (ci: number) => {
      const c = flat[ci];
      if (!c || viewsRef.current[c.slug] || loading.current.has(c.slug)) return;
      const cached = readCached<ChapterView>(`academy:chapters/${c.slug}`);
      if (cached && cached.chapter?.slug === c.slug) {
        setViews((v) => ({ ...v, [c.slug]: cached }));
        return;
      }
      loading.current.add(c.slug);
      setFailed((f) => (f[c.slug] ? { ...f, [c.slug]: false } : f));
      academyApi<ChapterView>(`chapters/${c.slug}`)
        .then((v) => {
          writeCached(`academy:chapters/${c.slug}`, v);
          setViews((prev) => ({ ...prev, [c.slug]: v }));
        })
        .catch(() => setFailed((f) => ({ ...f, [c.slug]: true })))
        .finally(() => loading.current.delete(c.slug));
    },
    [flat],
  );
  const mkey = (slug: string) => `${slug}@${L.key}`;
  const counts = React.useMemo(() => {
    let words = 0;
    let pages = 0;
    for (const c of flat) {
      const m = measured[`${c.slug}@${L.key}`];
      const v = views[c.slug];
      if (m && v) {
        words += v.chapter.words;
        pages += m;
      }
    }
    const wpp = pages > 0 && words > 0 ? words / pages : wordsPerPage(L);
    return flat.map((c) => measured[`${c.slug}@${L.key}`] ?? Math.max(2, Math.round(((views[c.slug]?.chapter.words ?? c.minutes * 200) + 140) / wpp)));
  }, [flat, measured, views, L]);
  const toc = React.useMemo(() => paginateToc(phase, flat, L), [phase, flat, L]);
  const seq = React.useMemo(() => buildSeq(flat, toc.length, !!phase.exam, counts, spread), [flat, toc.length, phase.exam, counts, spread]);
  const index = React.useMemo(() => new Map(seq.map((g, i) => [keyOf(g), i])), [seq]);
  const total = spread ? seq.length - 2 : seq.length;
  const folio = React.useCallback((i: number) => Math.max(1, Math.min(total, spread ? i : i + 1)), [spread, total]);

  /* progress state (local mirror of what the service has) */
  const [done, setDone] = React.useState<Ctx["done"]>(() => Object.fromEntries(flat.map((c) => [c.slug, { completed: c.progress.completed, best: c.progress.quiz_best, total: c.progress.quiz_total }])));
  const [read, setRead] = React.useState<Record<string, number>>(() => Object.fromEntries(flat.map((c) => [c.slug, c.progress.read_pct])));
  const prog = React.useRef<Record<string, { sent: number; max: number; visited: boolean }>>({});
  const flushTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const flush = React.useCallback((keepalive = false) => {
    if (flushTimer.current) clearTimeout(flushTimer.current);
    flushTimer.current = null;
    for (const [slug, st] of Object.entries(prog.current)) {
      if (st.max > st.sent) {
        st.sent = st.max;
        academyApi(`chapters/${slug}/progress`, { body: { read_pct: st.max }, keepalive }).catch(() => {});
      }
    }
  }, []);
  const report = React.useCallback(
    (slug: string, p: number, base: number) => {
      const st = (prog.current[slug] ??= { sent: base, max: base, visited: false });
      if (!st.visited) {
        st.visited = true;
        // register the visit (as the chapter page does), so "Continue" points here
        if (st.sent === 0 && p === 0) academyApi(`chapters/${slug}/progress`, { body: { read_pct: 0 } }).catch(() => {});
      }
      if (p > st.max) {
        st.max = p;
        setRead((r) => ((r[slug] ?? 0) >= p ? r : { ...r, [slug]: p }));
        if (!flushTimer.current) flushTimer.current = setTimeout(() => flush(), 1200);
      }
    },
    [flush],
  );
  React.useEffect(() => {
    const hide = () => document.visibilityState === "hidden" && flush(true);
    document.addEventListener("visibilitychange", hide);
    return () => {
      document.removeEventListener("visibilitychange", hide);
      flush(true);
    };
  }, [flush]);

  /* position */
  const initial = React.useMemo(() => resumeOf(cat, phase, flat), []); // eslint-disable-line react-hooks/exhaustive-deps
  const [anchor, setAnchor] = React.useState<Anchor>(initial.anchor);
  const resolve = React.useCallback(
    (a: Anchor) => {
      let i = 0;
      if (a.k === "ch") {
        const n = counts[a.c] ?? 1;
        const p = Math.max(0, Math.min(n - 1, Math.floor(a.f * n)));
        i = index.get(`ch:${a.c}:${p}`) ?? 0;
      } else i = index.get(a.key) ?? (a.key === "end" ? seq.length - 1 : 0);
      return spread ? i - (i % 2) : i;
    },
    [counts, index, seq.length, spread],
  );
  const pos = resolve(anchor);
  const anchorAt = React.useCallback(
    (i: number): Anchor => {
      const g = seq[i];
      if (!g) return { k: "key", key: "title" };
      if (g.k === "ch") return { k: "ch", c: g.c, f: (g.p + 0.5) / Math.max(1, counts[g.c] ?? 1) };
      return { k: "key", key: keyOf(g) };
    },
    [seq, counts],
  );

  /* stage + motion values */
  const [stage, setStageState] = React.useState<Stage>("fly-in");
  const stageRef = React.useRef<Stage>("fly-in");
  const setStage = (s: Stage) => {
    stageRef.current = s;
    setStageState(s);
  };
  const [turn, setTurn] = React.useState<Turn | null>(null);
  const turnRef = React.useRef<Turn | null>(null);
  const turnAnim = React.useRef<AnimationPlaybackControls | null>(null);
  const queued = React.useRef<0 | 1 | -1>(0);
  /** the last move was a jump (contents, progress bar, Home/End) or a step back, not reading on: it doesn't count */
  const jumped = React.useRef(false);
  const rot = useMotionValue(0);
  const closedShift = spread ? -(L.pw + bleed) / 2 : 0;
  const shift = useMotionValue(closedShift);
  const backdrop = useMotionValue(0);
  const chrome = useMotionValue(0);
  const stageOp = useMotionValue(1);
  const fx = useMotionValue(0);
  const fy = useMotionValue(0);
  const fw = useMotionValue(0);
  const fh = useMotionValue(0);
  const flyOp = useMotionValue(0);
  const flyScale = useMotionValue(1);
  const leftShadow = useTransform(rot, [0, -90, -155, -180], [0, 0, 0.5, 0]);
  const rightShadow = useTransform(rot, [0, -25, -90, -180], [0, 0.5, 0.22, 0]);
  const singleShadow = useTransform(rot, [0, -30, -100, -180], [0, 0.45, 0.15, 0]);

  const posRef = React.useRef(pos);
  posRef.current = pos;
  const seqRef = React.useRef(seq);
  seqRef.current = seq;
  const LRef = React.useRef(L);
  LRef.current = L;

  /* which chapters to have loaded: the ones on screen (and turning), the next, and the previous at a chapter start */
  const visible: Pg[] = React.useMemo(() => {
    const out: (Pg | null | undefined)[] = spread ? [seq[pos], seq[pos + 1]] : [seq[pos]];
    if (turn) out.push(turn.left, turn.right, typeof turn.front === "object" ? turn.front : null, typeof turn.back === "object" ? turn.back : null);
    return out.filter((g): g is Pg => !!g && typeof g === "object");
  }, [seq, pos, spread, turn]);
  React.useEffect(() => {
    const want = new Set<number>();
    for (const g of visible) {
      if (g.k === "ch") {
        want.add(g.c);
        want.add(g.c + 1);
        if (g.p === 0) want.add(g.c - 1);
      } else if (g.k === "quiz") {
        want.add(g.c);
        want.add(g.c + 1);
      } else if (g.k === "title" || g.k === "toc") want.add(0);
    }
    if (initial.wait !== null) want.add(initial.wait);
    want.forEach((ci) => ci >= 0 && ensure(ci));
  }, [visible, ensure, initial.wait]);

  /* reading progress: the furthest page of a chapter on screen, once the page has settled (its last page = 100%).
     Only turning forward counts: landing somewhere by a jump or a step back just registers the visit, and the quiz page after a chapter
     doesn't count (it can share a spread with the next chapter's opening page). */
  React.useEffect(() => {
    if (stage !== "open" || turn) return;
    const tm = setTimeout(() => {
      const on: Pg[] = spread ? [seq[pos], seq[pos + 1]].filter(Boolean) as Pg[] : seq[pos] ? [seq[pos]] : [];
      for (const g of on) {
        if (g.k === "ch") {
          const c = flat[g.c]!;
          if (!views[c.slug] || measured[mkey(c.slug)] === undefined) continue;
          const reached = jumped.current && g.p > 0 ? 0 : Math.round(((g.p + 1) / counts[g.c]!) * 100);
          report(c.slug, reached, Math.max(c.progress.read_pct, views[c.slug]!.progress.read_pct));
        }
      }
    }, 450);
    return () => clearTimeout(tm);
  }, [stage, turn, pos, seq, spread, flat, views, measured, counts, report]); // eslint-disable-line react-hooks/exhaustive-deps

  /* turning pages */
  const startTurn = React.useCallback(
    (target: number) => {
      if (stageRef.current !== "open") return;
      const s = seqRef.current;
      const step = LRef.current.mode === "spread" ? 2 : 1;
      target = Math.max(0, Math.min(s.length - step, target));
      if (step === 2) target -= target % 2;
      const from = posRef.current;
      if (target === from) return;
      if (turnRef.current) {
        queued.current = target > from ? 1 : -1;
        return;
      }
      const dir: 1 | -1 = target > from ? 1 : -1;
      const nextAnchor = anchorAt(target);
      jumped.current = Math.abs(target - from) > step || dir === -1;
      if (reduced) {
        setAnchor(nextAnchor);
        return;
      }
      const T: Turn =
        step === 2
          ? dir === 1
            ? { dir, left: s[from] ?? null, right: s[target + 1] ?? null, front: s[from + 1] ?? null, back: s[target] ?? null }
            : { dir, left: s[target] ?? null, right: s[from + 1] ?? null, front: s[target + 1] ?? null, back: s[from] ?? null }
          : dir === 1
            ? { dir, left: null, right: s[target] ?? null, front: s[from] ?? null, back: "paper" }
            : { dir, left: null, right: s[from] ?? null, front: s[target] ?? null, back: "paper" };
      rot.set(dir === 1 ? 0 : -180);
      turnRef.current = T;
      setTurn(T);
      const far = Math.abs(target - from) > step;
      const ctl = animate(rot, dir === 1 ? -180 : 0, { duration: far ? 0.82 : step === 2 ? 0.7 : 0.6, ease: EASE_TURN });
      turnAnim.current = ctl;
      ctl.then(() => {
        turnAnim.current = null;
        turnRef.current = null;
        setAnchor(nextAnchor);
        setTurn(null);
        const q = queued.current;
        queued.current = 0;
        if (q) requestAnimationFrame(() => startTurn(posRef.current + q * (LRef.current.mode === "spread" ? 2 : 1)));
      });
    },
    [anchorAt, reduced, rot],
  );
  const next = React.useCallback(() => startTurn(posRef.current + (LRef.current.mode === "spread" ? 2 : 1)), [startTurn]);
  const prev = React.useCallback(() => startTurn(posRef.current - (LRef.current.mode === "spread" ? 2 : 1)), [startTurn]);

  /* drawers */
  const [drawer, setDrawer] = React.useState(false);
  const [quizFor, setQuizFor] = React.useState<number | null>(null);
  const openQuiz = React.useCallback(
    (ci: number) => {
      ensure(ci);
      setQuizFor(ci);
    },
    [ensure],
  );
  const onQuizDone = (ci: number, r: QuizReply) => {
    const slug = flat[ci]!.slug;
    setDone((d) => {
      const cur = d[slug] ?? { completed: false, best: null, total: null };
      return { ...d, [slug]: { completed: cur.completed || r.completed, best: Math.max(cur.best ?? 0, r.score), total: r.total } };
    });
  };

  /* open / close */
  const pushed = React.useRef(false);
  const closeQueued = React.useRef(false);
  const alive = React.useRef(true);
  const coverRect = React.useCallback((): Rect => {
    const l = LRef.current;
    const b = l.mode === "spread" ? 8 : 5;
    const bookW = l.mode === "spread" ? 2 * l.pw : l.pw;
    const x0 = (l.vw - bookW) / 2;
    const cx = l.mode === "spread" ? x0 + l.pw - (l.pw + b) / 2 : x0;
    return { x: cx, y: l.stageTop - b, w: l.pw + b, h: l.ph + 2 * b };
  }, []);
  const readyRef = React.useRef(false);
  readyRef.current = initial.wait === null || measured[mkey(flat[initial.wait]!.slug)] !== undefined || !!failed[flat[initial.wait]!.slug];

  const finish = React.useCallback(() => {
    if (origin) origin.style.visibility = "";
    onClosed();
  }, [origin, onClosed]);

  const beginClose = React.useCallback(async () => {
    const st = stageRef.current;
    if (st === "closing" || st === "fly-out") return;
    if (st !== "open") {
      closeQueued.current = true;
      return;
    }
    if (turnAnim.current) {
      turnAnim.current.complete();
      await nextFrame();
    }
    setDrawer(false);
    setQuizFor(null);
    flush(true);
    animate(chrome, 0, { duration: 0.15 });
    if (reduced) {
      stageRef.current = "closing"; // the book just fades out as it is
      await Promise.all([animate(stageOp, 0, { duration: 0.2 }), animate(backdrop, 0, { duration: 0.25 })]);
      finish();
      return;
    }
    const l = LRef.current;
    rot.set(-180);
    shift.set(0);
    setStage("closing");
    await nextFrame();
    if (!alive.current) return;
    await Promise.all([animate(rot, 0, { duration: 0.95, ease: EASE_OPEN }), l.mode === "spread" ? animate(shift, -(l.pw + 8) / 2, { duration: 0.95, ease: EASE_OPEN }) : Promise.resolve()]);
    if (!alive.current) return;
    const C = coverRect();
    fx.set(C.x);
    fy.set(C.y);
    fw.set(C.w);
    fh.set(C.h);
    flyOp.set(1);
    flyScale.set(1);
    setStage("fly-out");
    const home = origin?.isConnected ? origin : (document.querySelector(`[data-testid="phase-card-${phase.slug}"] [data-book-root]`) as HTMLElement | null);
    const S = faceRect(home);
    if (S) {
      await Promise.all([
        animate(fx, S.x, { duration: 0.6, ease: EASE_FLY }),
        animate(fy, S.y, { duration: 0.6, ease: EASE_FLY }),
        animate(fw, S.w, { duration: 0.6, ease: EASE_FLY }),
        animate(fh, S.h, { duration: 0.6, ease: EASE_FLY }),
        animate(backdrop, 0, { duration: 0.55 }),
      ]);
    } else {
      await Promise.all([animate(flyOp, 0, { duration: 0.35 }), animate(flyScale, 0.86, { duration: 0.35 }), animate(backdrop, 0, { duration: 0.4 })]);
    }
    finish();
  }, [backdrop, chrome, coverRect, finish, flush, flyOp, flyScale, fh, fw, fx, fy, origin, phase.slug, reduced, rot, shift, stageOp]);

  const requestClose = React.useCallback(() => {
    if (stageRef.current === "closing" || stageRef.current === "fly-out") return;
    if (pushed.current) {
      window.history.back(); // popstate closes the book
      return;
    }
    // opened from a link: drop ?book= and close
    const u = new URL(window.location.href);
    if (u.searchParams.has("book")) {
      u.searchParams.delete("book");
      window.history.replaceState(null, "", u.pathname + u.search + u.hash);
    }
    void beginClose();
  }, [beginClose]);

  // history: the open book is a history entry, so Back closes it
  React.useEffect(() => {
    if (!fromUrl && !pushed.current) {
      const u = new URL(window.location.href);
      u.searchParams.set("book", phase.slug);
      window.history.pushState(null, "", u.pathname + u.search + u.hash);
      pushed.current = true;
    }
    const onPop = () => {
      if (new URLSearchParams(window.location.search).get("book") === phase.slug) return;
      pushed.current = false;
      void beginClose();
    };
    window.addEventListener("popstate", onPop);
    return () => window.removeEventListener("popstate", onPop);
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // the opening sequence (a token per run: StrictMode runs effects twice in development)
  React.useEffect(() => {
    const run = { live: true };
    alive.current = true;
    const html = document.documentElement;
    const prevOverflow = html.style.overflow;
    html.style.overflow = "hidden";
    if (origin) origin.style.visibility = "hidden";
    (async () => {
      animate(backdrop, 1, { duration: reduced ? 0.2 : 0.4 });
      const l = LRef.current;
      const C = coverRect();
      const waitReady = async () => {
        const t0 = Date.now();
        while (run.live && !readyRef.current && Date.now() - t0 < 2500) await sleep(40);
      };
      if (reduced) {
        stageOp.set(0);
        shift.set(0);
        await waitReady();
        if (!run.live) return;
        setStage("open");
        await Promise.all([animate(stageOp, 1, { duration: 0.25 }), animate(chrome, 1, { duration: 0.25 })]);
        return;
      }
      const S = faceRect(origin);
      fx.set(S?.x ?? C.x);
      fy.set(S?.y ?? C.y);
      fw.set(S?.w ?? C.w);
      fh.set(S?.h ?? C.h);
      flyScale.set(S ? 1 : 0.86);
      flyOp.set(S ? 1 : 0);
      if (S) {
        await Promise.all([
          animate(fx, C.x, { duration: 0.62, ease: EASE_FLY }),
          animate(fy, C.y, { duration: 0.62, ease: EASE_FLY }),
          animate(fw, C.w, { duration: 0.62, ease: EASE_FLY }),
          animate(fh, C.h, { duration: 0.62, ease: EASE_FLY }),
        ]);
      } else {
        await Promise.all([animate(flyOp, 1, { duration: 0.35 }), animate(flyScale, 1, { duration: 0.45, ease: EASE_FLY })]);
      }
      // wait for the page the book opens at (content + measured), up to 2.5 s
      await waitReady();
      if (!run.live) return;
      rot.set(0);
      shift.set(l.mode === "spread" ? -(l.pw + 8) / 2 : 0);
      setStage("opening");
      await nextFrame();
      if (!run.live) return;
      flyOp.set(0);
      await Promise.all([animate(rot, -180, { duration: 1.15, ease: EASE_OPEN }), l.mode === "spread" ? animate(shift, 0, { duration: 1.15, ease: EASE_OPEN }) : Promise.resolve()]);
      if (!run.live) return;
      setStage("open");
      animate(chrome, 1, { duration: 0.35 });
      if (closeQueued.current) {
        closeQueued.current = false;
        void beginClose();
      }
    })();
    return () => {
      run.live = false;
      alive.current = false;
      html.style.overflow = prevOverflow;
      if (origin) origin.style.visibility = "";
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  /* keyboard */
  const rootRef = React.useRef<HTMLDivElement | null>(null);
  React.useEffect(() => {
    rootRef.current?.focus({ preventScroll: true });
    const on = (e: KeyboardEvent) => {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === "Escape") {
        e.preventDefault();
        if (quizFor !== null) setQuizFor(null);
        else if (drawer) setDrawer(false);
        else requestClose();
        return;
      }
      if (quizFor !== null || drawer) return;
      if (e.key === "ArrowRight" || e.key === "PageDown" || (e.key === " " && !e.shiftKey)) {
        e.preventDefault();
        next();
      } else if (e.key === "ArrowLeft" || e.key === "PageUp" || (e.key === " " && e.shiftKey)) {
        e.preventDefault();
        prev();
      } else if (e.key === "Home") {
        e.preventDefault();
        startTurn(0);
      } else if (e.key === "End") {
        e.preventDefault();
        startTurn(seqRef.current.length - 1);
      }
    };
    window.addEventListener("keydown", on);
    return () => window.removeEventListener("keydown", on);
  }, [next, prev, startTurn, requestClose, quizFor, drawer]);

  /* swipe and tap */
  const touch = React.useRef<{ x: number; y: number; t: number } | null>(null);
  const onPointerDown = (e: React.PointerEvent) => {
    if (e.pointerType === "mouse") return;
    touch.current = { x: e.clientX, y: e.clientY, t: Date.now() };
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const s = touch.current;
    touch.current = null;
    if (!s || stageRef.current !== "open") return;
    const dx = e.clientX - s.x;
    const dy = e.clientY - s.y;
    const dt = Date.now() - s.t;
    if (Math.abs(dx) > 40 && Math.abs(dx) > Math.abs(dy) * 1.3 && dt < 800) {
      if (dx < 0) next();
      else prev();
      return;
    }
    // tap on the outer thirds turns the page (not on links and buttons)
    if (Math.abs(dx) < 10 && Math.abs(dy) < 10 && dt < 400 && !(e.target as HTMLElement).closest("a,button,input,label")) {
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const fxr = (e.clientX - r.left) / r.width;
      if (fxr > 0.68) next();
      else if (fxr < 0.32) prev();
    }
  };

  /* what is on the stage */
  const step = spread ? 2 : 1;
  const closedish = stage === "fly-in" || stage === "opening" || stage === "closing" || stage === "fly-out";
  let baseLeft: Pg | null = null;
  let baseRight: Pg | null = null;
  let leaf: { front: Face; back: Face } | null = null;
  if (closedish) {
    baseRight = spread ? (seq[pos + 1] ?? null) : (seq[pos] ?? null);
    leaf = { front: "cover", back: spread ? (seq[pos] ?? null) : "paper" };
  } else if (turn) {
    baseLeft = turn.left;
    baseRight = turn.right;
    leaf = { front: turn.front, back: turn.back };
  } else {
    baseLeft = spread ? (seq[pos] ?? null) : null;
    baseRight = spread ? (seq[pos + 1] ?? null) : (seq[pos] ?? null);
  }

  /* where we are */
  const onScreen = (spread ? [seq[pos], seq[pos + 1]] : [seq[pos]]).filter(Boolean) as Pg[];
  const chOn = onScreen.filter((g): g is Extract<Pg, { k: "ch" }> => g.k === "ch");
  const quizOn = onScreen.find((g): g is Extract<Pg, { k: "quiz" }> => g.k === "quiz");
  const current = chOn[0]?.c ?? quizOn?.c ?? null;
  const firstContent = onScreen.findIndex((g) => g.k !== "endpaper");
  const pageNow = folio(firstContent < 0 ? (pos === 0 ? 1 : seq.length - 2) : pos + firstContent);
  const lastCh = chOn[chOn.length - 1];
  const minLeft = lastCh ? Math.ceil(((counts[lastCh.c]! - 1 - lastCh.p) / counts[lastCh.c]!) * flat[lastCh.c]!.minutes) : null;
  const progressPct = Math.min(100, Math.round(((pos + step) / seq.length) * 100));
  const examUnlocked = !!phase.exam && (phase.exam.unlocked || flat.every((c) => done[c.slug]?.completed));

  const ctx: Ctx = {
    L,
    phase,
    flat,
    seq,
    index,
    counts,
    toc,
    views,
    failed,
    done,
    read,
    current,
    learner: me.name,
    examUnlocked,
    folio,
    goTo: (i: number) => {
      setDrawer(false);
      startTurn(i);
    },
    openQuiz,
    retry: (ci: number) => ensure(ci),
  };

  const bookW = spread ? 2 * L.pw : L.pw;
  const x0 = Math.round((L.vw - bookW) / 2);
  const thick = (frac: number) => 1 + Math.round(Math.max(0, Math.min(1, frac)) * 5);
  const frac = seq.length > 1 ? pos / (seq.length - 1) : 0;
  const quizView = quizFor !== null ? views[flat[quizFor]!.slug] : undefined;
  const open = stage === "open";

  return createPortal(
    <ReaderCtx.Provider value={ctx}>
      <div ref={rootRef} tabIndex={-1} role="dialog" aria-modal="true" aria-label={t("academy.phaseTitle", { n: phase.order, title: phase.title })} className="kb-root fixed inset-0 z-[200] select-none overflow-hidden outline-none" data-testid="book-reader" data-stage={stage}>
        <style>{PAPER_CSS}</style>

        {/* the reading room */}
        <motion.div aria-hidden className="absolute inset-0 bg-[#060504]" style={{ opacity: backdrop }}>
          <div className="absolute inset-0 bg-[radial-gradient(70%_60%_at_50%_45%,rgba(242,96,12,.10),transparent_70%)]" />
          <div className="absolute inset-0 bg-[radial-gradient(120%_90%_at_50%_50%,transparent_55%,rgba(0,0,0,.7))]" />
        </motion.div>

        {/* top bar */}
        <motion.div className="absolute inset-x-0 top-0 z-40 flex items-center gap-2 px-3 text-white sm:px-5" style={{ height: L.top, opacity: chrome, pointerEvents: open ? "auto" : "none" }}>
          <button type="button" onClick={requestClose} aria-label={t("common.close")} data-testid="book-close" className="grid size-9 shrink-0 place-items-center rounded-full bg-white/[.07] text-white/80 transition-colors hover:bg-white/[.14] hover:text-white">
            <X className="size-[18px]" />
          </button>
          <div className="min-w-0 flex-1 text-center">
            <div className="truncate text-[13px] font-medium text-white/90">{phase.title}</div>
            <div className="truncate text-[11px] text-white/45">
              {isElective(phase) ? t("academy.elective") : t("academy.phaseN", { n: phase.order })} · {levelLabel(phase.level)}
            </div>
          </div>
          <Link href={`/academy/phase/${phase.slug}`} className="hidden h-9 shrink-0 items-center gap-1.5 rounded-full bg-white/[.07] px-3.5 text-[12.5px] text-white/80 transition-colors hover:bg-white/[.14] hover:text-white sm:inline-flex" data-testid="book-overview">
            <LayoutList className="size-4" /> {t("academy.hero.overview")}
          </Link>
          <button type="button" onClick={() => setDrawer(true)} aria-label={t.dyn("academy.reader.contents", "Contents")} data-testid="book-contents" className="grid size-9 shrink-0 place-items-center rounded-full bg-white/[.07] text-white/80 transition-colors hover:bg-white/[.14] hover:text-white">
            <List className="size-[18px]" />
          </button>
        </motion.div>

        {/* the book */}
        <motion.div
          className="absolute touch-pan-y"
          style={{ left: x0, top: L.stageTop, width: bookW, height: L.ph, x: shift, opacity: stageOp, perspective: Math.max(spread ? 3600 : 2400, L.pw * (spread ? 7 : 5)), visibility: stage === "fly-in" || stage === "fly-out" ? "hidden" : "visible" }}
          onPointerDown={onPointerDown}
          onPointerUp={onPointerUp}
        >
          {/* back board (and the front board, once open) */}
          <div aria-hidden className={cn("absolute rounded-e-[12px] rounded-s-[3px] shadow-[0_50px_90px_-40px_rgba(0,0,0,.95),0_20px_40px_-20px_rgba(0,0,0,.8)]", finishOf(phase))} style={{ left: spread ? L.pw : 0, top: -bleed, width: L.pw + bleed, height: L.ph + 2 * bleed }}>
            <div className="absolute inset-0 rounded-[inherit] bg-black/35" />
          </div>
          {spread && !closedish && (
            <div aria-hidden className={cn("absolute rounded-e-[3px] rounded-s-[12px] shadow-[0_50px_90px_-40px_rgba(0,0,0,.95)]", finishOf(phase))} style={{ left: -bleed, top: -bleed, width: L.pw + bleed, height: L.ph + 2 * bleed }}>
              <div className="absolute inset-0 rounded-[inherit] bg-black/35" />
            </div>
          )}
          {/* page block edges: thicker on the side you have read */}
          {spread && !closedish && baseLeft?.k !== "endpaper" && <div aria-hidden className="kb-edge-v absolute rounded-s-[3px]" style={{ left: -thick(frac), top: 1, width: L.pw, height: L.ph - 2 + thick(frac) * 0.5 }} />}
          {baseRight?.k !== "endpaper" && <div aria-hidden className="kb-edge-v absolute rounded-e-[3px]" style={{ left: spread ? L.pw : 0, top: 1, width: L.pw + thick(1 - frac), height: L.ph - 2 + thick(1 - frac) * 0.5 }} />}

          {/* pages */}
          <motion.div key={reduced ? pos : "pages"} className="absolute inset-0" initial={reduced ? { opacity: 0.2 } : false} animate={{ opacity: 1 }} transition={{ duration: 0.18 }}>
            {baseLeft && (
              <div className="absolute" style={{ left: 0, top: 0, width: L.pw, height: L.ph }} data-testid="book-page-left">
                <PageFace pg={baseLeft} side="left" />
              </div>
            )}
            {baseRight && (
              <div className="absolute" style={{ left: spread ? L.pw : 0, top: 0, width: L.pw, height: L.ph }} data-testid={spread ? "book-page-right" : "book-page"}>
                <PageFace pg={baseRight} side={spread ? "right" : "single"} />
              </div>
            )}
          </motion.div>

          {/* gutter */}
          {spread && baseLeft && <div aria-hidden className="pointer-events-none absolute inset-y-0 z-10 bg-[linear-gradient(90deg,transparent,rgba(50,30,5,.16)_42%,rgba(25,15,2,.34)_50%,rgba(50,30,5,.16)_58%,transparent)]" style={{ left: L.pw - 22, width: 44 }} />}

          {/* shadows cast by a turning page */}
          {leaf && spread && baseLeft && <motion.div aria-hidden className="pointer-events-none absolute inset-y-0 z-20 bg-[linear-gradient(to_left,rgba(20,12,2,.55),rgba(20,12,2,.12)_35%,transparent_70%)]" style={{ left: 0, width: L.pw, opacity: leftShadow }} />}
          {leaf && spread && baseRight && <motion.div aria-hidden className="pointer-events-none absolute inset-y-0 z-20 bg-[linear-gradient(to_right,rgba(20,12,2,.55),rgba(20,12,2,.12)_35%,transparent_70%)]" style={{ left: L.pw, width: L.pw, opacity: rightShadow }} />}
          {leaf && !spread && baseRight && <motion.div aria-hidden className="pointer-events-none absolute inset-0 z-20 bg-[linear-gradient(to_right,rgba(20,12,2,.5),rgba(20,12,2,.1)_40%,transparent_80%)]" style={{ opacity: singleShadow }} />}

          {/* page edges you can click (desktop) */}
          {open && spread && !turn && (
            <>
              <button type="button" aria-label={t("academy.reader.previous")} onClick={prev} disabled={pos === 0} className="group absolute inset-y-0 start-0 z-20 cursor-pointer disabled:cursor-default" style={{ width: L.padOut - 6 }} data-testid="book-edge-prev">
                <span className="pointer-events-none absolute inset-0 rounded-s-[3px] bg-gradient-to-r from-black/[.10] to-transparent opacity-0 transition-opacity group-hover:opacity-100 group-disabled:hidden" />
              </button>
              <button type="button" aria-label={t("common.next")} onClick={next} disabled={pos + step >= seq.length} className="group absolute inset-y-0 end-0 z-20 cursor-pointer disabled:cursor-default" style={{ width: L.padOut - 6 }} data-testid="book-edge-next">
                <span className="pointer-events-none absolute inset-0 rounded-e-[3px] bg-gradient-to-l from-black/[.10] to-transparent opacity-0 transition-opacity group-hover:opacity-100 group-disabled:hidden" />
                <span className="pointer-events-none absolute bottom-0 end-0 size-0 rounded-tl-[6px] bg-[linear-gradient(135deg,#fffaf0,#e4d9c2_48%,rgba(0,0,0,.25)_50%,transparent_58%)] shadow-[-3px_-3px_8px_-4px_rgba(0,0,0,.4)] transition-all duration-200 group-hover:size-9 group-disabled:hidden" />
              </button>
            </>
          )}

          {leaf && <Leaf rot={rot} front={leaf.front} back={leaf.back} spread={spread} bleed={bleed} />}
        </motion.div>

        {/* the cover flying to and from the shelf */}
        <motion.div aria-hidden className="pointer-events-none fixed z-30" style={{ left: fx, top: fy, width: fw, height: fh, opacity: flyOp, scale: flyScale, visibility: stage === "fly-in" || stage === "fly-out" ? "visible" : "hidden" }}>
          <CoverFace p={phase} className="absolute inset-0 shadow-[0_40px_80px_-30px_rgba(0,0,0,.95)]" />
        </motion.div>

        {/* turn buttons beside the book */}
        {spread && (
          <motion.div className="pointer-events-none absolute inset-0 z-40" style={{ opacity: chrome }}>
            <button type="button" onClick={prev} disabled={!open || pos === 0} aria-label={t("academy.reader.previous")} data-testid="book-prev" className="pointer-events-auto absolute grid size-11 place-items-center rounded-full bg-white/[.06] text-white/75 transition-colors hover:bg-white/[.14] hover:text-white disabled:pointer-events-none disabled:opacity-20" style={{ left: x0 - 62, top: L.stageTop + L.ph / 2 - 22 }}>
              <ChevronLeft className="size-5" />
            </button>
            <button type="button" onClick={next} disabled={!open || pos + step >= seq.length} aria-label={t("common.next")} data-testid="book-next" className="pointer-events-auto absolute grid size-11 place-items-center rounded-full bg-white/[.06] text-white/75 transition-colors hover:bg-white/[.14] hover:text-white disabled:pointer-events-none disabled:opacity-20" style={{ left: x0 + bookW + 18, top: L.stageTop + L.ph / 2 - 22 }}>
              <ChevronRight className="size-5" />
            </button>
          </motion.div>
        )}

        {/* location */}
        <motion.div className="absolute inset-x-0 bottom-0 z-40 flex items-center justify-center gap-3 px-3" style={{ height: L.bottom, opacity: chrome, pointerEvents: open ? "auto" : "none" }}>
          {!spread && (
            <button type="button" onClick={prev} disabled={pos === 0} aria-label={t("academy.reader.previous")} data-testid="book-prev" className="grid size-9 shrink-0 place-items-center rounded-full bg-white/[.07] text-white/75 disabled:opacity-25">
              <ChevronLeft className="size-[18px]" />
            </button>
          )}
          <div className="flex min-w-0 flex-col items-center gap-2" style={{ width: spread ? Math.min(bookW, 640) : undefined, flex: spread ? undefined : 1 }}>
            <button
              type="button"
              aria-label={t.dyn("academy.reader.position", "Reading position")}
              className="relative h-[14px] w-full cursor-pointer"
              onClick={(e) => {
                const r = e.currentTarget.getBoundingClientRect();
                const f = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
                startTurn(Math.round(f * (seq.length - 1)));
              }}
            >
              <span className="absolute inset-x-0 top-1/2 h-[3px] -translate-y-1/2 rounded-full bg-white/[.12]" />
              <span className="absolute start-0 top-1/2 h-[3px] -translate-y-1/2 rounded-full bg-[var(--k-red)] transition-[width] duration-300" style={{ width: `${progressPct}%` }} data-testid="book-progress" />
              {flat.map((c, ci) => {
                const at = index.get(`ch:${ci}:0`);
                if (at === undefined) return null;
                return <span key={c.slug} className={cn("absolute top-1/2 h-[7px] w-px -translate-y-1/2", done[c.slug]?.completed ? "bg-[#4fbf7f]" : "bg-white/30")} style={{ left: `${(at / seq.length) * 100}%` }} />;
              })}
            </button>
            <div className="k-num max-w-full truncate text-[11.5px] text-white/55" data-testid="book-location">
              {t.dyn("academy.reader.pageOf", "Page {n} of {total}", { n: pageNow, total })}
              {current !== null && ` · ${t.dyn("academy.reader.chapterN", "Chapter {n}", { n: flat[current]!.n })}`}
              {minLeft !== null && minLeft > 0 && ` · ${t.dyn("academy.reader.minLeft", "{count} min left in chapter", { count: minLeft })}`}
            </div>
          </div>
          {!spread && (
            <button type="button" onClick={next} disabled={pos + 1 >= seq.length} aria-label={t("common.next")} data-testid="book-next" className="grid size-9 shrink-0 place-items-center rounded-full bg-white/[.07] text-white/75 disabled:opacity-25">
              <ChevronRight className="size-[18px]" />
            </button>
          )}
        </motion.div>

        {/* contents drawer */}
        <AnimatePresence>
          {drawer && (
            <>
              <motion.div key="dim" className="absolute inset-0 z-50 bg-black/55" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={() => setDrawer(false)} />
              <motion.aside
                key="drawer"
                className="absolute inset-y-0 end-0 z-50 flex w-[min(400px,92vw)] flex-col border-s border-white/10 bg-[#0b0a09] text-white shadow-[0_0_60px_rgba(0,0,0,.6)]"
                initial={{ x: "100%" }}
                animate={{ x: 0 }}
                exit={{ x: "100%" }}
                transition={{ type: "tween", duration: 0.28, ease: EASE_FLY }}
                data-testid="book-drawer"
              >
                <div className="flex items-center justify-between gap-3 border-b border-white/10 px-5 py-4">
                  <div className="min-w-0">
                    <div className="text-[11px] font-semibold uppercase tracking-[.18em] text-[var(--k-red)]">{t.dyn("academy.reader.contents", "Contents")}</div>
                    <div className="truncate text-[15px] font-medium">{phase.title}</div>
                  </div>
                  <button type="button" onClick={() => setDrawer(false)} aria-label={t("common.close")} className="grid size-8 place-items-center rounded-full bg-white/[.07] text-white/75 hover:bg-white/[.14]">
                    <X className="size-4" />
                  </button>
                </div>
                <div className="flex-1 overflow-y-auto px-3 py-3 [touch-action:pan-y]">
                  <button type="button" onClick={() => ctx.goTo(index.get("title") ?? 0)} className="flex w-full items-center justify-between rounded-[12px] px-3 py-2.5 text-start text-[13px] text-white/70 hover:bg-white/[.05]">
                    <span>{t.dyn("academy.reader.titlePage", "Title page")}</span>
                    <span className="k-num text-white/40">{folio(index.get("title") ?? 0)}</span>
                  </button>
                  {phase.sections.map((s, si) => (
                    <div key={s.slug} className="mt-2">
                      <div className="px-3 pb-1 pt-2 text-[10.5px] font-semibold uppercase tracking-[.16em] text-white/40">{trackLabel(t, s.track)}</div>
                      {flat
                        .map((c, ci) => ({ c, ci }))
                        .filter(({ c }) => c.si === si)
                        .map(({ c, ci }) => {
                          const at = index.get(`ch:${ci}:0`) ?? 0;
                          const st = done[c.slug]?.completed ? "done" : (read[c.slug] ?? 0) > 0 ? "reading" : "new";
                          return (
                            <button key={c.slug} type="button" onClick={() => ctx.goTo(at)} className={cn("flex w-full items-center gap-3 rounded-[12px] px-3 py-2.5 text-start transition-colors hover:bg-white/[.05]", current === ci && "bg-white/[.06]")}>
                              <span className={cn("k-num grid size-7 shrink-0 place-items-center rounded-full border text-[11.5px] font-medium", st === "done" ? "border-[#4fbf7f]/40 bg-[#4fbf7f]/15 text-[#7ad9a2]" : st === "reading" ? "border-[var(--k-red)]/50 bg-[var(--k-red)]/15 text-[#ff8a4c]" : "border-white/15 text-white/60")}>
                                {st === "done" ? <Check className="size-3.5" /> : c.n}
                              </span>
                              <span className="min-w-0 flex-1">
                                <span className="block truncate text-[13.5px] text-white/90">{c.title}</span>
                                <span className="k-num block text-[11px] text-white/40">
                                  {st === "done" ? t("common.completed") : st === "reading" ? t("academy.readPct", { pct: read[c.slug] ?? 0 }) : t("academy.state.notStarted")} · {t("academy.duration.min", { count: c.minutes })}
                                </span>
                              </span>
                              <span className="k-num shrink-0 text-[12px] text-white/40">{folio(at)}</span>
                            </button>
                          );
                        })}
                    </div>
                  ))}
                  {phase.exam && (
                    <button type="button" onClick={() => ctx.goTo(index.get("exam") ?? 0)} className="mt-2 flex w-full items-center gap-3 rounded-[12px] px-3 py-2.5 text-start hover:bg-white/[.05]">
                      <span className={cn("grid size-7 shrink-0 place-items-center rounded-full border", examUnlocked ? "border-[#ffb35c]/40 bg-[#ffb35c]/15 text-[#ffb35c]" : "border-white/15 text-white/50")}>{examUnlocked ? <GraduationCap className="size-3.5" /> : <Lock className="size-3.5" />}</span>
                      <span className="min-w-0 flex-1 truncate text-[13.5px] text-white/90">{t("academy.exam.phaseExam", { n: phase.order })}</span>
                      <span className="k-num shrink-0 text-[12px] text-white/40">{folio(index.get("exam") ?? 0)}</span>
                    </button>
                  )}
                </div>
                <div className="border-t border-white/10 p-4">
                  <Link href={`/academy/phase/${phase.slug}`} className="flex h-10 items-center justify-center gap-2 rounded-full bg-white/[.07] text-[13px] text-white/85 hover:bg-white/[.12]">
                    <LayoutList className="size-4" /> {t("academy.hero.overview")}
                  </Link>
                </div>
              </motion.aside>
            </>
          )}
        </AnimatePresence>

        {/* the chapter quiz, over the book */}
        <AnimatePresence>
          {quizFor !== null && (
            <motion.div key="quiz" className="absolute inset-0 z-[60] flex items-start justify-center overflow-y-auto overscroll-contain bg-black/75 px-3 py-6 backdrop-blur-[2px]" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={(e) => e.target === e.currentTarget && setQuizFor(null)} data-testid="book-quiz">
              <motion.div className="relative my-auto w-full max-w-[680px] select-text rounded-[22px] bg-[#0c0b0a] shadow-[0_30px_80px_-20px_rgba(0,0,0,.9)]" initial={{ y: 24, opacity: 0 }} animate={{ y: 0, opacity: 1 }} exit={{ y: 16, opacity: 0 }} transition={{ duration: 0.25, ease: EASE_FLY }}>
                <button type="button" onClick={() => setQuizFor(null)} aria-label={t("common.close")} className="absolute end-3 top-3 z-10 grid size-8 place-items-center rounded-full bg-white/[.07] text-white/75 hover:bg-white/[.14]">
                  <X className="size-4" />
                </button>
                {quizView ? (
                  <ChapterQuiz key={quizView.chapter.slug} slug={quizView.chapter.slug} questions={quizView.chapter.quiz} passedBefore={!!done[quizView.chapter.slug]?.completed} onDone={(r) => onQuizDone(quizFor, r)} />
                ) : (
                  <div className="grid h-40 place-items-center rounded-[20px] bg-surface text-fg-3">…</div>
                )}
              </motion.div>
            </motion.div>
          )}
        </AnimatePresence>

        {/* off-screen typesetting: page counts per chapter for this page size */}
        {flat.map((c) => {
          const v = views[c.slug];
          if (!v || measured[mkey(c.slug)] !== undefined) return null;
          return (
            <Measure
              key={`${c.slug}@${L.key}`}
              view={v}
              n={c.n}
              L={L}
              onCount={(n) => {
                const k = `${c.slug}@${L.key}`;
                MEASURED.set(k, n);
                setMeasured((m) => (m[k] === n ? m : { ...m, [k]: n }));
              }}
            />
          );
        })}
      </div>
    </ReaderCtx.Provider>,
    document.body,
  );
}

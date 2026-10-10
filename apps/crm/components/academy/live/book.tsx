"use client";

// Academy as a library (founder 2026-10-10: "like the debit cards for accounts, education as books and certificates"):
// every phase is a hardcover book (spine, foil frame, chapter number, page edges, a ribbon while you read, a seal
// once certified) and every certificate looks like the real paper one (cream sheet, gold frame, seal, name, ID).

import * as React from "react";
import { BadgeCheck } from "lucide-react";
import { LogoMark, cn } from "@/components/kit";
import { useT } from "@kalks/i18n/react";
import { fmtDay, fmtMin, isElective, levelLabel, type Certificate, type Level, type PhaseT } from "./api";

/** cover finish per level (the account cards' finishes); product courses are plum */
const FINISH: Record<Level, string> = {
  Beginner: "k-face-standard",
  Intermediate: "k-face-cent",
  Advanced: "k-face-pro",
  Professional: "k-face-vip",
};

/** The front board of a phase's book (finish per level, spine, foil frame, number, title, ribbon, seal). Sized by the
 *  caller; text scales with its width (container units), so the reader can show it at any size. */
export function CoverFace({ p, className, style }: { p: PhaseT; className?: string; style?: React.CSSProperties }) {
  const t = useT();
  const done = !!p.certificate;
  const reading = !done && p.progress.done > 0;
  const finish = isElective(p) ? "k-face-options" : FINISH[p.level];
  return (
    <div data-book-face className={cn("k-face relative overflow-hidden rounded-e-[12px] rounded-s-[4px] text-white [container-type:inline-size]", finish, className)} style={style}>
      {/* cloth grain + light from the top */}
      <div aria-hidden className="absolute inset-0 bg-[radial-gradient(120%_70%_at_30%_0%,rgba(255,255,255,.22),transparent_60%)]" />
      <div aria-hidden className="absolute inset-0 opacity-[.12] [background-image:repeating-linear-gradient(45deg,#fff_0_1px,transparent_1px_4px)]" />
      {/* spine and hinge */}
      <div aria-hidden className="absolute inset-y-0 start-0 w-[8cqw] bg-gradient-to-r from-black/55 via-black/20 to-transparent rtl:bg-gradient-to-l" />
      <div aria-hidden className="absolute inset-y-0 start-[8cqw] w-px bg-white/25" />
      <div aria-hidden className="absolute inset-y-0 start-[8cqw] ms-[2px] w-[3px] bg-black/15" />
      {/* foil frame */}
      <div aria-hidden className="absolute bottom-[6cqw] end-[6cqw] start-[13cqw] top-[6cqw] rounded-[5px] border border-white/30" />
      <div aria-hidden className="absolute bottom-[7.5cqw] end-[7.5cqw] start-[14.5cqw] top-[7.5cqw] rounded-[3px] border border-white/12" />

      {/* reading ribbon */}
      {reading && <div aria-hidden className="absolute end-[14cqw] top-0 h-[24cqw] w-[7cqw] bg-white/90 shadow-[0_4px_10px_rgba(0,0,0,.35)] [clip-path:polygon(0_0,100%_0,100%_100%,50%_80%,0_100%)]" />}

      <div className="relative flex h-full flex-col pb-[11cqw] pe-[11cqw] ps-[18cqw] pt-[11cqw]">
        <div className="flex items-center gap-[2cqw] text-[clamp(8px,3.4cqw,11px)] font-bold uppercase tracking-[0.22em] text-white/80">
          <LogoMark size={14} className="shrink-0 text-white" />
          <span className="truncate">{t.dyn("academy.book.series", "Kalks Academy")}</span>
        </div>
        <div className="mt-[7cqw] text-[clamp(9px,3.6cqw,12px)] font-semibold uppercase tracking-[0.18em] text-white/70">{isElective(p) ? t("academy.elective") : t("academy.phaseN", { n: p.order })}</div>
        <div className="k-num text-[clamp(44px,30cqw,92px)] font-extralight leading-[0.9] tracking-[-0.04em] text-white/95">{String(p.order).padStart(2, "0")}</div>
        <div className="k-display mt-auto line-clamp-3 text-[clamp(15px,8cqw,24px)] font-semibold leading-[1.1] tracking-[-0.015em] [text-shadow:0_1px_14px_rgba(0,0,0,.25)]">{p.title}</div>
        <div className="mt-[3cqw] h-[0.8cqw] w-[12cqw] rounded-full bg-white/70" />
        <div className="mt-[3cqw] flex items-center justify-between gap-2 text-[clamp(9px,3.5cqw,12px)] font-semibold uppercase tracking-[0.1em] text-white/75">
          <span className="truncate">{levelLabel(p.level)}</span>
          <span className="k-num shrink-0">{fmtMin(p.minutes)}</span>
        </div>
      </div>

      {/* certified: a gold seal on the cover */}
      {done && (
        <span className="absolute bottom-[20cqw] end-[8cqw] grid size-[17cqw] place-items-center rounded-full bg-[radial-gradient(circle_at_35%_30%,#ffe7a3,#d7a23e_55%,#8a5a12)] text-[#4a3005] shadow-[0_6px_16px_rgba(0,0,0,.45)]">
          <BadgeCheck className="size-[9cqw]" strokeWidth={2.2} />
        </span>
      )}
    </div>
  );
}

export function BookCover({ p }: { p: PhaseT }) {
  return (
    <div data-book-root className="relative pb-[6px] pe-[6px] transition-transform duration-300 [perspective:900px] group-hover:-translate-y-1">
      {/* the page block, showing at the fore-edge and the bottom */}
      <div aria-hidden className="absolute bottom-0 end-0 start-3 top-3 rounded-e-[12px] rounded-s-[4px] bg-[linear-gradient(90deg,#d8cfbd,#f4efe5_35%,#e7dfcf)] shadow-[0_18px_30px_-16px_rgba(0,0,0,.9)]" />
      <div aria-hidden className="absolute bottom-[2px] end-[2px] start-4 top-4 rounded-e-[11px] rounded-s-[3px] bg-[repeating-linear-gradient(0deg,rgba(0,0,0,.07)_0_1px,transparent_1px_3px)]" />
      <CoverFace
        p={p}
        className="aspect-[3/4] transition-transform duration-300 [transform-origin:left_center] group-hover:[transform:rotateY(-9deg)] rtl:[transform-origin:right_center] rtl:group-hover:[transform:rotateY(9deg)]"
      />
    </div>
  );
}

/** a certificate as the paper one: cream sheet, gold double frame, the learner's name, phase, score, seal, ID */
export function CertificateCard({ c, className }: { c: Pick<Certificate, "code" | "phase_order" | "phase_title" | "score_pct" | "issued_at" | "learner_name">; className?: string }) {
  const t = useT();
  return (
    <div
      className={cn(
        "relative aspect-[1.414/1] w-full overflow-hidden rounded-[8px] bg-[#f6f1e6] text-[#2b2216] shadow-[0_24px_50px_-28px_rgba(0,0,0,.95)] [container-type:inline-size]",
        className,
      )}
    >
      {/* paper */}
      <div aria-hidden className="absolute inset-0 bg-[radial-gradient(90%_70%_at_50%_45%,#fffaf0,transparent_70%),radial-gradient(60%_50%_at_100%_100%,rgba(184,137,58,.14),transparent)]" />
      {/* guilloche band + frames */}
      <div aria-hidden className="absolute inset-[2.6cqw] rounded-[4px] border-[0.5cqw] border-[#b8893a]" />
      <div aria-hidden className="absolute inset-[3.8cqw] rounded-[2px] border border-[#b8893a]/55" />
      <div aria-hidden className="absolute inset-[4.6cqw] rounded-[2px] opacity-[.07] [background-image:repeating-radial-gradient(circle_at_50%_50%,#8a5a12_0_1px,transparent_1px_6px)]" />
      {(["start-[2.2cqw] top-[2.2cqw]", "end-[2.2cqw] top-[2.2cqw]", "bottom-[2.2cqw] start-[2.2cqw]", "bottom-[2.2cqw] end-[2.2cqw]"] as const).map((pos) => (
        <span key={pos} aria-hidden className={cn("absolute size-[3cqw] rotate-45 border-[0.4cqw] border-[#b8893a] bg-[#f6f1e6]", pos)} />
      ))}

      <div className="relative flex h-full flex-col items-center px-[10cqw] pb-[7.5cqw] pt-[8cqw] text-center">
        <div className="flex items-center gap-[1.4cqw] text-[clamp(7px,1.9cqw,11px)] font-bold uppercase tracking-[0.32em] text-[#7a5a22]">
          <LogoMark size={12} className="text-[#2b2216]" />
          {t.dyn("academy.book.series", "Kalks Academy")}
        </div>
        <div className="mt-[2.4cqw] font-serif text-[clamp(14px,5.2cqw,30px)] leading-none tracking-[0.01em]">{t.dyn("academy.cert.face.title", "Certificate of Completion")}</div>
        <div className="mt-[2.6cqw] text-[clamp(7px,1.8cqw,11px)] uppercase tracking-[0.2em] text-[#6b5a40]">{t.dyn("academy.cert.face.certifies", "This certifies that")}</div>
        <div className="mt-[1.2cqw] max-w-full truncate border-b border-[#b8893a]/60 px-[3cqw] pb-[0.8cqw] font-serif text-[clamp(15px,6cqw,34px)] italic leading-tight">{c.learner_name}</div>
        <div className="mt-[2cqw] max-w-[88%] text-[clamp(8px,2.1cqw,12.5px)] leading-snug text-[#4a3d2a]">
          {t.dyn("academy.cert.face.completed", "has completed Phase {n}: {title}, with an exam score of {pct}%", { n: c.phase_order, title: c.phase_title, pct: c.score_pct })}
        </div>
        <div className="mt-auto grid w-full grid-cols-[1fr_auto_1fr] items-end gap-[2cqw]">
          <div className="text-start">
            <div className="k-num border-b border-[#2b2216]/40 pb-[0.6cqw] text-[clamp(8px,2.1cqw,12.5px)] font-semibold">{fmtDay(c.issued_at)}</div>
            <div className="mt-[0.6cqw] text-[clamp(6.5px,1.5cqw,9.5px)] uppercase tracking-[0.16em] text-[#6b5a40]">{t.dyn("academy.cert.face.issued", "Date issued")}</div>
          </div>
          {/* the seal */}
          <div className="relative grid size-[13cqw] place-items-center">
            <span aria-hidden className="absolute -bottom-[3.6cqw] start-[3.2cqw] h-[6cqw] w-[2.6cqw] -rotate-12 bg-[#a33a1c] [clip-path:polygon(0_0,100%_0,100%_100%,50%_78%,0_100%)]" />
            <span aria-hidden className="absolute -bottom-[3.6cqw] end-[3.2cqw] h-[6cqw] w-[2.6cqw] rotate-12 bg-[#a33a1c] [clip-path:polygon(0_0,100%_0,100%_100%,50%_78%,0_100%)]" />
            <span className="relative grid size-full place-items-center rounded-full bg-[radial-gradient(circle_at_35%_30%,#ffe7a3,#d7a23e_55%,#8a5a12)] shadow-[0_3px_10px_rgba(90,60,10,.45)]">
              <span className="grid size-[78%] place-items-center rounded-full border border-dashed border-[#5a3a08]/60">
                <LogoMark size={18} className="text-[#3d2705]" />
              </span>
            </span>
          </div>
          <div className="text-end">
            <div className="k-num truncate border-b border-[#2b2216]/40 pb-[0.6cqw] text-[clamp(8px,2.1cqw,12.5px)] font-semibold">{c.code}</div>
            <div className="mt-[0.6cqw] text-[clamp(6.5px,1.5cqw,9.5px)] uppercase tracking-[0.16em] text-[#6b5a40]">{t.dyn("academy.cert.face.id", "Certificate ID")}</div>
          </div>
        </div>
      </div>
    </div>
  );
}

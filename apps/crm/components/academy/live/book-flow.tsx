"use client";

// The book reader's paper: a chapter set in print style (serif, dark ink on cream) and flowed into pages with CSS
// multi-column layout. A chapter is one strip of fixed-size columns (one column = one page); a page is a window onto
// the strip translated by page × (column width + gap). Diagrams carry their aspect ratio from the SVG viewBox, so
// the column count is right on the first layout (no image-load jank), and they never break across pages.
// Parsing and inline markdown are the Academy reader's own (./markdown): parseBlocks + Inline.

import * as React from "react";
import { Check } from "lucide-react";
import type { MessageKey } from "@kalks/i18n";
import { useT } from "@kalks/i18n/react";
import { cn } from "@/components/kit";
import { Inline, parseBlocks } from "./markdown";
import { trackLabel, type ChapterView } from "./api";

/** Page geometry of the reader, computed from the viewport. */
export type Layout = {
  mode: "spread" | "single";
  vw: number;
  vh: number;
  /** page size */
  pw: number;
  ph: number;
  /** top bar / bottom bar heights and the stage's top edge */
  top: number;
  bottom: number;
  stageTop: number;
  /** page margins: top, bottom, spine side, outer side (single: both sides = out) */
  padT: number;
  padB: number;
  padIn: number;
  padOut: number;
  /** text block = one column */
  cw: number;
  ch: number;
  gap: number;
  /** body font size (px) */
  fs: number;
  key: string;
};

export function computeLayout(vw: number, vh: number): Layout {
  const wide = vw >= 768;
  if (wide) {
    const top = 60;
    const bottom = 64;
    const availH = vh - top - bottom - 20;
    const availW = vw - 2 * 76;
    let ph = Math.min(availH, 880);
    let pw = Math.round(ph * 0.75);
    if (pw * 2 > availW) {
      pw = Math.floor(availW / 2);
      ph = Math.round(pw / 0.75);
    }
    if (pw >= 330) {
      const padT = Math.round(Math.max(44, ph * 0.075));
      const padB = padT;
      const padIn = Math.round(Math.max(36, pw * 0.09));
      const padOut = Math.round(Math.max(32, pw * 0.08));
      const cw = pw - padIn - padOut;
      const ch = ph - padT - padB;
      const fs = cw >= 440 ? 17 : cw >= 370 ? 16.5 : 15.5;
      const stageTop = Math.round(top + (vh - top - bottom - ph) / 2);
      return { mode: "spread", vw, vh, pw, ph, top, bottom, stageTop, padT, padB, padIn, padOut, cw, ch, gap: 64, fs, key: `s${pw}x${ph}` };
    }
  }
  // one page at a time: phones, and narrow / portrait windows
  const top = 52;
  const bottom = 58;
  const ph = Math.round(vh - top - bottom - 14);
  const pw = Math.round(Math.min(vw - 16, ph * 0.75));
  const padOut = pw >= 520 ? 48 : pw >= 400 ? 34 : 24;
  const padT = pw >= 520 ? 56 : 44;
  const padB = pw >= 520 ? 52 : 40;
  const cw = pw - 2 * padOut;
  const ch = ph - padT - padB;
  const fs = cw >= 420 ? 17 : 16;
  const stageTop = Math.round(top + (vh - top - bottom - ph) / 2);
  return { mode: "single", vw, vh, pw, ph, top, bottom, stageTop, padT, padB, padIn: padOut, padOut, cw, ch, gap: 48, fs, key: `p${pw}x${ph}` };
}

/** Rough words per page for a layout, before a chapter has been measured. */
export const wordsPerPage = (L: Layout) => Math.max(50, (0.135 * L.cw * L.ch) / (L.fs * L.fs));

/** Ink and paper: re-points the app's colour tokens inside the paper, so shared pieces (Inline's code, links, bold)
 *  print in ink. */
export const PAPER_VARS = {
  "--k-fg": "#241c13",
  "--k-fg-2": "#3b3125",
  "--k-fg-3": "#7a6950",
  "--k-border": "rgba(74,52,20,.16)",
  "--k-border-2": "rgba(74,52,20,.26)",
  "--k-surface": "#fbf7ee",
  "--k-surface-2": "rgba(120,90,40,.075)",
  "--k-surface-3": "rgba(120,90,40,.13)",
  "--k-ember": "#c2500c",
  "--k-ember-tx": "#b04509",
  "--k-ember-soft": "rgba(194,80,12,.1)",
  "--k-up": "#2f7d4f",
  "--k-up-tx": "#2a6f46",
  "--k-up-soft": "rgba(47,125,79,.1)",
  "--k-down": "#b3261e",
  "--k-down-tx": "#9f241c",
  "--k-down-soft": "rgba(179,38,30,.08)",
  "--k-gold": "#a6761d",
  "--k-gold-tx": "#8a5f12",
  "--k-gold-soft": "rgba(166,118,29,.12)",
  "--k-info": "#2c5d8f",
  "--k-info-tx": "#2c5d8f",
  "--k-info-soft": "rgba(44,93,143,.08)",
  color: "#241c13",
} as React.CSSProperties;

/** Scoped styles of the reader (print typography inside .kb-flow, paper, page edges). */
export const PAPER_CSS = `
.kb-root{--kb-serif:"Iowan Old Style","Palatino Linotype",Palatino,"Book Antiqua","New York",Georgia,Cambria,"Times New Roman",serif}
.kb-paper{background:#f6f1e6}
.kb-grain{background:radial-gradient(120% 90% at 50% 40%,rgba(255,252,244,.9),transparent 70%),radial-gradient(80% 60% at 100% 100%,rgba(176,140,80,.10),transparent 70%),radial-gradient(70% 50% at 0% 0%,rgba(176,140,80,.07),transparent 70%)}
.kb-cols{column-fill:auto;orphans:2;widows:2}
.kb-flow,.kb-flow h1,.kb-flow h2,.kb-flow h3{font-family:var(--kb-serif)}
.kb-flow{line-height:1.62;color:#241c13;font-kerning:normal;text-rendering:optimizeLegibility}
.kb-flow p,.kb-flow li{text-align:justify;hyphens:auto;-webkit-hyphens:auto}
.kb-flow.kb-ragged p,.kb-flow.kb-ragged li{text-align:start}
.kb-flow p{margin:0 0 .8em}
.kb-flow h2{font-size:1.3em;line-height:1.25;font-weight:600;margin:1.25em 0 .45em;break-after:avoid;letter-spacing:-.005em;color:#1c150e}
.kb-flow h3{font-size:1.08em;line-height:1.3;font-weight:600;margin:1.05em 0 .35em;break-after:avoid;color:#1c150e}
.kb-flow ul,.kb-flow ol{margin:0 0 .85em;padding:0;list-style:none}
.kb-flow li{position:relative;padding-inline-start:1.25em;margin:0 0 .35em}
.kb-flow ul>li::before{content:"";position:absolute;inset-inline-start:.3em;top:.68em;width:.34em;height:.34em;border-radius:50%;background:#c2500c}
.kb-flow ol>li>.kb-n{position:absolute;inset-inline-start:0;top:0;font-weight:600;color:#b04509;font-variant-numeric:tabular-nums}
.kb-flow .kb-avoid{break-inside:avoid}
.kb-flow .kb-first::first-letter{float:left;font-size:3.35em;line-height:.86;padding:.07em .09em 0 0;margin-inline-end:.02em;color:#b04509;font-weight:600}
.kb-flow table{width:100%;border-collapse:collapse;font-family:var(--k-font-text);font-size:.74em;line-height:1.38;margin:.2em 0 1.1em;text-align:start}
.kb-flow th{font-weight:600;text-transform:uppercase;letter-spacing:.05em;font-size:.88em;color:#5c4d38;border-bottom:1.5px solid #3b3125;padding:.35em .55em .35em 0;text-align:start;vertical-align:bottom}
.kb-flow td{border-bottom:1px solid rgba(74,52,20,.16);padding:.42em .55em .42em 0;vertical-align:top;overflow-wrap:anywhere;color:#3b3125}
.kb-flow td:first-child{color:#241c13;font-weight:500}
.kb-flow tr{break-inside:avoid}
.kb-flow pre{white-space:pre-wrap;overflow-wrap:anywhere;font-family:var(--k-font-mono);font-size:.74em;line-height:1.5;background:rgba(120,90,40,.075);border:1px solid rgba(74,52,20,.14);border-radius:8px;padding:.7em .85em;margin:.2em 0 1em;text-align:start}
.kb-flow hr{border:0;margin:.6em 0 1em;text-align:center;height:1.4em}
.kb-flow hr::after{content:"\\2766";color:#b04509;font-size:1.05em}
.kb-flow>:last-child{margin-bottom:0}
.kb-edge{background:repeating-linear-gradient(0deg,#efe7d6 0 1px,#dcd1bb 1px 2px)}
.kb-edge-v{background:repeating-linear-gradient(90deg,#efe7d6 0 1px,#dcd1bb 1px 2px)}
`;

// callouts in print colours; labels are the Academy's own
const CALLOUT: Record<string, { cls: string; label: MessageKey }> = {
  "risk warning": { cls: "border-[#b3261e] bg-[rgba(179,38,30,.06)] [&_.kb-lbl]:text-[#9f241c]", label: "academy.callout.riskWarning" },
  warning: { cls: "border-[#b3261e] bg-[rgba(179,38,30,.06)] [&_.kb-lbl]:text-[#9f241c]", label: "academy.callout.warning" },
  example: { cls: "border-[#2c5d8f] bg-[rgba(44,93,143,.06)] [&_.kb-lbl]:text-[#2c5d8f]", label: "academy.callout.example" },
  tip: { cls: "border-[#2f7d4f] bg-[rgba(47,125,79,.07)] [&_.kb-lbl]:text-[#2a6f46]", label: "academy.callout.tip" },
  note: { cls: "border-[#7a6950] bg-[rgba(120,90,40,.07)] [&_.kb-lbl]:text-[#5c4d38]", label: "academy.callout.note" },
  "in kalks trader": { cls: "border-[#c2500c] bg-[rgba(194,80,12,.07)] [&_.kb-lbl]:text-[#b04509]", label: "academy.callout.inKalksTrader" },
};

function PaperCallout({ text }: { text: string }) {
  const t = useT();
  const m = /^\*\*([^*:]+):?\*\*:?\s*(.*)$/.exec(text);
  const known = CALLOUT[m?.[1]?.trim().toLowerCase() ?? ""];
  if (!m) {
    return (
      <blockquote className="mb-[.9em] border-s-2 border-[rgba(74,52,20,.3)] ps-[1em] italic text-[#3b3125]">
        <Inline text={text} />
      </blockquote>
    );
  }
  return (
    <aside role="note" className={cn("kb-avoid mb-[.95em] rounded-e-[8px] border-s-[3px] px-[.9em] py-[.6em] text-[.93em] leading-[1.55]", known?.cls ?? CALLOUT.note!.cls)}>
      <div className="kb-lbl mb-[.2em] font-sans text-[.68em] font-bold uppercase tracking-[.12em]">{known ? t(known.label) : m[1]!.trim()}</div>
      <Inline text={m[2]!} />
    </aside>
  );
}

function PaperDiagram({ svg, label, maxH }: { svg: string; label: string; maxH: number }) {
  const src = React.useMemo(() => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg.trim())}`, [svg]);
  const vb = /viewBox\s*=\s*"\s*[-\d.]+[\s,]+[-\d.]+[\s,]+([\d.]+)[\s,]+([\d.]+)\s*"/i.exec(svg);
  const ratio = vb ? Number(vb[1]) / Math.max(1, Number(vb[2])) : 16 / 9;
  const title = /<text[^>]*>([^<]{4,80})<\/text>/.exec(svg)?.[1] ?? label;
  return (
    <figure className="kb-avoid mb-[1em] mt-[.2em]">
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img
        src={src}
        alt={title}
        draggable={false}
        className="mx-auto block w-full rounded-[6px] object-contain shadow-[0_1px_0_rgba(255,255,255,.6),0_6px_16px_-10px_rgba(40,25,5,.6)] ring-1 ring-[rgba(74,52,20,.25)]"
        style={{ aspectRatio: String(ratio), maxHeight: maxH }}
      />
      <figcaption className="mt-[.45em] text-center font-sans text-[.68em] uppercase tracking-[.12em] text-[#7a6950]">{label}</figcaption>
    </figure>
  );
}

/** One chapter in print: opener, body, takeaways and the practice note. */
export const ChapterContent = React.memo(function ChapterContent({ view, n, L }: { view: ChapterView; n: number; L: Layout }) {
  const t = useT();
  const c = view.chapter;
  const blocks = React.useMemo(() => parseBlocks(c.body), [c.body]);
  const firstP = blocks.findIndex((b) => b.t === "p");
  let fig = 0;
  return (
    <div className={cn("kb-flow", L.cw < 380 && "kb-ragged")} style={{ fontSize: L.fs }}>
      {/* chapter opener */}
      <header className="kb-avoid" style={{ paddingTop: Math.round(L.ch * 0.06) }}>
        <div className="font-sans text-[.66em] font-bold uppercase tracking-[.2em] text-[#b04509]">
          {t.dyn("academy.reader.chapterN", "Chapter {n}", { n })} · {trackLabel(t, view.section.track)}
        </div>
        <h1 className="mt-[.5em] text-[1.85em] font-semibold leading-[1.1] tracking-[-.012em] text-[#1c150e]">{c.title}</h1>
        {c.summary && <p className="!mb-0 mt-[.6em] !text-start text-[1.02em] italic leading-[1.45] text-[#4a3d2a]">{c.summary}</p>}
        <div className="mt-[.9em] flex items-center gap-[.6em] font-sans text-[.68em] uppercase tracking-[.12em] text-[#7a6950]">
          <span>{t("academy.reader.minRead", { count: c.minutes })}</span>
          {c.quiz.length > 0 && (
            <>
              <span aria-hidden>·</span>
              <span>{t("academy.reader.quizLength", { count: c.quiz.length })}</span>
            </>
          )}
        </div>
        <div aria-hidden className="mb-[1.3em] mt-[1em] flex items-center gap-[.6em] text-[#b04509]">
          <span className="h-px flex-1 bg-[rgba(74,52,20,.22)]" />
          <span className="text-[.8em]">◆</span>
          <span className="h-px flex-1 bg-[rgba(74,52,20,.22)]" />
        </div>
      </header>

      {blocks.map((b, i) => {
        switch (b.t) {
          case "h":
            return b.level === 2 ? (
              <h2 key={i}>
                <Inline text={b.text} />
              </h2>
            ) : (
              <h3 key={i}>
                <Inline text={b.text} />
              </h3>
            );
          case "p":
            return (
              <p key={i} className={i === firstP ? "kb-first" : undefined}>
                <Inline text={b.text} />
              </p>
            );
          case "ul":
            return (
              <ul key={i}>
                {b.items.map((it, j) => (
                  <li key={j}>
                    <Inline text={it} />
                  </li>
                ))}
              </ul>
            );
          case "ol":
            return (
              <ol key={i}>
                {b.items.map((it, j) => (
                  <li key={j}>
                    <span className="kb-n">{b.start + j}.</span>
                    <Inline text={it} />
                  </li>
                ))}
              </ol>
            );
          case "quote":
            return <PaperCallout key={i} text={b.text} />;
          case "code":
            if (b.lang === "svg") {
              fig += 1;
              return <PaperDiagram key={i} svg={b.text} maxH={Math.round(L.ch * 0.82)} label={t.dyn("academy.reader.figure", "Figure {n}", { n: `${n}.${fig}` })} />;
            }
            return <pre key={i}>{b.text}</pre>;
          case "table":
            return (
              <table key={i}>
                <thead>
                  <tr>
                    {b.head.map((h, j) => (
                      <th key={j}>
                        <Inline text={h} />
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {b.rows.map((r, j) => (
                    <tr key={j}>
                      {r.map((cell, x) => (
                        <td key={x}>
                          <Inline text={cell} />
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            );
          case "hr":
            return <hr key={i} />;
        }
      })}

      {c.takeaways.length > 0 && (
        <section className="mt-[1.4em] rounded-[10px] border border-[rgba(74,52,20,.2)] bg-[rgba(166,118,29,.07)] px-[1em] pb-[.5em] pt-[.8em]">
          <h2 className="!mt-0 !text-[1.08em]">{t("academy.reader.takeaways")}</h2>
          <ul className="!mb-[.3em]">
            {c.takeaways.map((tk, j) => (
              <li key={j} className="kb-avoid !ps-[1.5em] text-[.95em] before:!hidden">
                <Check className="absolute start-0 top-[.3em] size-[1em] text-[#2a6f46]" strokeWidth={2.6} />
                {tk}
              </li>
            ))}
          </ul>
        </section>
      )}

      {c.practice && (
        <aside className="kb-avoid mt-[1em] rounded-[10px] border border-dashed border-[rgba(194,80,12,.45)] px-[1em] py-[.75em] text-[.93em] leading-[1.5]">
          <div className="mb-[.2em] font-sans text-[.68em] font-bold uppercase tracking-[.12em] text-[#b04509]">
            {t("academy.practice.inTrader")}
            {c.practice.symbol ? ` · ${c.practice.symbol}` : ""}
          </div>
          {c.practice.label}
        </aside>
      )}
    </div>
  );
});

/** A chapter as a strip of page-sized columns. The strip is left-to-right in every language (page maths); the text
 *  inside keeps the document's direction. */
function Strip({ L, ref, children }: { L: Layout; ref?: React.Ref<HTMLDivElement>; children: React.ReactNode }) {
  return (
    <div ref={ref} dir="ltr" className="kb-cols" style={{ width: L.cw, height: L.ch, columnWidth: L.cw, columnGap: L.gap }}>
      <div dir={typeof document !== "undefined" ? document.documentElement.dir || "ltr" : "ltr"}>{children}</div>
    </div>
  );
}

/** Page `page` of a chapter: a window onto its strip. */
export function FlowWindow({ view, n, L, page }: { view: ChapterView; n: number; L: Layout; page: number }) {
  return (
    <div className="relative overflow-hidden" style={{ width: L.cw, height: L.ch }}>
      <div style={{ transform: `translateX(${-page * (L.cw + L.gap)}px)` }}>
        <Strip L={L}>
          <ChapterContent view={view} n={n} L={L} />
        </Strip>
      </div>
    </div>
  );
}

/** Lays a chapter out off-screen once and reports how many pages it fills. */
export function Measure({ view, n, L, onCount }: { view: ChapterView; n: number; L: Layout; onCount: (pages: number) => void }) {
  const ref = React.useRef<HTMLDivElement | null>(null);
  const cb = React.useRef(onCount);
  cb.current = onCount;
  React.useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let alive = true;
    const measure = () => {
      if (!alive || !ref.current) return;
      const pages = Math.max(1, Math.round((el.scrollWidth + L.gap) / (L.cw + L.gap)));
      cb.current(pages);
    };
    measure();
    document.fonts?.ready.then(measure).catch(() => {});
    return () => {
      alive = false;
    };
  }, [view, n, L]);
  return (
    <div aria-hidden className="kb-paper" style={{ position: "absolute", left: -100000, top: 0, visibility: "hidden", pointerEvents: "none", ...PAPER_VARS }}>
      <Strip L={L} ref={ref}>
        <ChapterContent view={view} n={n} L={L} />
      </Strip>
    </div>
  );
}

"use client";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import { Mic, Sparkles, ArrowUp, X } from "lucide-react";
import { cn } from "../lib/cn";

/**
 * Docked "Ask Kalks AI" prompt bar with suggestion chips (Kalks 2: a yellow-edged card, black spark disc).
 * Answers are mocked until the Claude API integration is wired.
 */
export function AiPromptBar({ suggestions, answer }: { suggestions: string[]; answer: (q: string) => string }) {
  const [q, setQ] = React.useState("");
  const [thread, setThread] = React.useState<{ q: string; a: string } | null>(null);
  const [typing, setTyping] = React.useState("");
  const [open, setOpen] = React.useState(false);

  function ask(text: string) {
    if (!text.trim()) return;
    const full = answer(text);
    setThread({ q: text, a: full });
    setQ("");
    setTyping("");
    let i = 0;
    const t = setInterval(() => {
      i += 3;
      setTyping(full.slice(0, i));
      if (i >= full.length) clearInterval(t);
    }, 16);
  }

  return (
    <div className="pointer-events-none fixed inset-x-0 bottom-24 z-30 flex justify-center px-4 lg:bottom-6 lg:pl-[100px]">
      <div className="pointer-events-auto w-full max-w-[680px]" onMouseEnter={() => setOpen(true)} onMouseLeave={() => setOpen(false)} onFocus={() => setOpen(true)}>
        <AnimatePresence>
          {thread && (
            <motion.div initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: 12 }} className="k-card mb-3 bg-surface p-5">
              <div className="flex items-start justify-between gap-4">
                <p className="text-[13px] text-fg-3">{thread.q}</p>
                <button onClick={() => setThread(null)} className="text-fg-3 hover:text-fg" aria-label="Close answer">
                  <X className="size-4" />
                </button>
              </div>
              <p className="mt-2 whitespace-pre-line text-[14px] leading-relaxed text-fg">
                {typing}
                {typing.length < thread.a.length && <span className="ml-0.5 inline-block h-4 w-1.5 animate-pulse bg-yellow align-middle" />}
              </p>
            </motion.div>
          )}
        </AnimatePresence>
        <div className="relative rounded-[24px] bg-yellow p-px shadow-[2px_2px_0_var(--k-yellow-edge),4px_4px_0_var(--k-yellow-edge)]">
          <div className="rounded-[23px] bg-surface/95 p-1.5 backdrop-blur-xl">
            <div className={cn("flex gap-2 overflow-x-auto px-1 [scrollbar-width:none] transition-all duration-300", open ? "max-h-12 pb-2 pt-1 opacity-100" : "max-h-0 opacity-0")}>
              {suggestions.map((s) => (
                <button key={s} onClick={() => ask(s)} className="shrink-0 rounded-full border border-line bg-surface-2 px-3.5 py-1.5 text-[12.5px] text-fg-2 transition-colors hover:border-line-2 hover:text-fg">
                  {s}
                </button>
              ))}
            </div>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                ask(q);
              }}
              className="flex items-center gap-2 rounded-[18px] border border-line bg-surface-2 p-1.5"
            >
              <span className="grid size-10 shrink-0 place-items-center rounded-full bg-k-black text-k-yellow">
                <Sparkles className="size-[18px]" />
              </span>
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Ask Kalks AI anything about markets or your accounts…" className="h-10 min-w-0 flex-1 bg-transparent px-1 text-[14px] outline-none placeholder:text-fg-3" />
              <button type="submit" className={cn("grid size-10 shrink-0 place-items-center rounded-full border border-line transition-colors", q ? "bg-fg text-bg" : "bg-surface-3 text-fg-2")} aria-label={q ? "Send" : "Voice"}>
                {q ? <ArrowUp className="size-4" /> : <Mic className="size-4" />}
              </button>
            </form>
          </div>
        </div>
      </div>
    </div>
  );
}

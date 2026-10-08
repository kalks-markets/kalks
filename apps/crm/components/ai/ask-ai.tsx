"use client";

// "Ask Kalks AI" on the Overview: a card on tablets and desktops, a compact pill on phones that opens a bottom sheet
// (in the page flow, so it never covers the content or the bottom bar). Questions go to the real support bot in live
// builds (components/ai/engine.ts) and to canned answers in demo builds; the floating support chat continues the
// same conversation.

import * as React from "react";
import { createPortal } from "react-dom";
import Link from "next/link";
import { AnimatePresence, motion } from "motion/react";
import { ArrowUp, ChevronRight, MessageCircle, RotateCcw, Sparkles, UserRound, X } from "lucide-react";
import { Avatar, Button, cn } from "@/components/kit";
import { IS_DEMO } from "@kalks/mock";
import { SUPPORT_AGENT, agentAnswer, botAnswer } from "@kalks/mock/support-extra";
import { useSession } from "@/components/session";
import { Rich } from "@/components/support/live-chat";
import { openSupportChat } from "@/components/support/launcher";
import { useT } from "@kalks/i18n/react";
import { useDemoAi, useLiveAi, type AiEngine, type Turn } from "./engine";

export type AiChip = { key: string; label: string; question?: string; extra?: React.ReactNode };

/* ------------------------------------------------------------------ */
/* Pieces                                                              */
/* ------------------------------------------------------------------ */

function Spark({ size = 40 }: { size?: number }) {
  return (
    <span className="k-brand-disc grid shrink-0 place-items-center rounded-full text-white" style={{ width: size, height: size }}>
      <Sparkles style={{ width: size * 0.46, height: size * 0.46 }} strokeWidth={2} />
    </span>
  );
}

function Dots({ label }: { label: string }) {
  return (
    <span className="flex items-center gap-1 py-1" role="status" aria-label={label}>
      {[0, 1, 2].map((i) => (
        <motion.span key={i} className="size-1.5 rounded-full bg-fg-3" animate={{ opacity: [0.25, 1, 0.25], y: [0, -2, 0] }} transition={{ duration: 0.9, repeat: Infinity, delay: i * 0.15 }} />
      ))}
    </span>
  );
}

function Bubble({ turn, botName, extra }: { turn: Turn; botName: string; extra?: React.ReactNode }) {
  if (turn.role === "system")
    return (
      <div className="flex justify-center">
        <span className="max-w-[92%] rounded-full bg-surface-2 px-3 py-1 text-center text-[11.5px] text-fg-3">{turn.text}</span>
      </div>
    );
  if (turn.role === "you")
    return (
      <div className="flex flex-col items-end gap-2">
        <div className="max-w-[85%] rounded-[18px] rounded-ee-md bg-ember px-3.5 py-2 text-[13.5px] leading-relaxed text-[var(--k-on-ember)]">{turn.text}</div>
        {extra}
      </div>
    );
  return (
    <div className="flex items-start gap-2.5">
      {turn.role === "agent" ? <Avatar name={turn.name ?? "Support"} size={28} /> : <Spark size={28} />}
      <div className="min-w-0 max-w-[88%]">
        <div className="mb-1 text-[11.5px] font-semibold text-fg-2">{turn.role === "agent" ? turn.name : botName}</div>
        <div className="rounded-[18px] rounded-ss-md bg-surface-2 px-3.5 py-2.5 text-[13.5px] leading-relaxed text-fg-2">
          <Rich text={turn.text} />
        </div>
        {turn.cites && turn.cites.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-1.5">
            {turn.cites.map((c) => (
              <span key={c.slug} className="rounded-full bg-surface-2 px-2 py-0.5 text-[10.5px] text-fg-3">
                {c.title}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/** Label / value rows shown under a question (e.g. the client's real free margin per account). */
export function AiFacts({ title, rows }: { title: string; rows: { label: React.ReactNode; value: React.ReactNode; tone?: "up" | "warn" | "down" }[] }) {
  if (!rows.length) return null;
  return (
    <div className="w-full max-w-[85%] rounded-[16px] border border-line bg-surface px-3.5 py-2.5">
      <div className="text-[11.5px] font-semibold text-fg-3">{title}</div>
      <div className="mt-1 divide-y divide-line">
        {rows.map((r, i) => (
          <div key={i} className="flex items-center justify-between gap-3 py-1.5 text-[12.5px]">
            <span className="min-w-0 truncate text-fg-2">{r.label}</span>
            <span dir="ltr" className={cn("k-num shrink-0 font-semibold", r.tone === "up" ? "text-up" : r.tone === "warn" ? "text-warn" : r.tone === "down" ? "text-down" : "text-fg")}>
              {r.value}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** "Your request for a person is still open · View" and similar team notes. */
function TeamNote({ text, onView }: { text: string; onView: () => void }) {
  const t = useT();
  return (
    <div className="mt-3 flex items-center gap-2.5 rounded-[14px] bg-info-soft px-3.5 py-2 text-[12.5px] text-fg-2" data-testid="ai-team-note">
      <UserRound className="size-4 shrink-0 text-info" />
      <span className="min-w-0 flex-1">{text}</span>
      <button type="button" onClick={onView} className="flex h-8 shrink-0 items-center gap-1 rounded-full px-2.5 font-semibold text-info hover:bg-info-soft">
        {t("dashboard.ai.view")} <ChevronRight className="size-3.5 rtl:-scale-x-100" />
      </button>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Panel (card body and sheet body)                                    */
/* ------------------------------------------------------------------ */

function Panel({ e, chips, variant, onOpenChat, chat, onClose, inputRef }: { e: AiEngine; chips: AiChip[]; variant: "card" | "sheet"; onOpenChat: () => void; chat: boolean; onClose?: () => void; inputRef: React.RefObject<HTMLInputElement | null> }) {
  const t = useT();
  const [q, setQ] = React.useState("");
  const scroller = React.useRef<HTMLDivElement>(null);
  const sheet = variant === "sheet";
  const has = e.turns.length > 0 || e.streaming !== null;
  const human = e.status === "waiting" || e.status === "assigned";
  const byKey = new Map(chips.map((c) => [c.key, c]));
  const lastYou = [...e.turns].reverse().find((x) => x.role === "you");
  const answered = has && !e.waiting && !e.blocked && e.turns[e.turns.length - 1]?.role !== "you";

  // keep the latest question at the top of the thread, so a long answer is read from its first line
  const started = e.streaming !== null;
  React.useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const qs = el.querySelectorAll<HTMLElement>("[data-q]");
    const last = qs[qs.length - 1];
    el.scrollTo({ top: last ? Math.max(0, last.offsetTop - 4) : el.scrollHeight, behavior: "smooth" });
  }, [e.turns.length, started]);

  const submit = (text: string, chip?: string) => {
    if (!text.trim()) return;
    e.ask(text, chip);
    setQ("");
  };

  return (
    <div className={cn("flex min-h-0 flex-col", sheet && "h-full")}>
      {/* header */}
      <div className="flex items-start gap-3">
        <Spark size={sheet ? 40 : 44} />
        <div className="min-w-0 flex-1">
          <h3 className="k-display text-[17px] font-semibold tracking-[-0.015em] sm:text-[18px]">{t("dashboard.ai.title", { name: e.botName })}</h3>
          <p className="mt-0.5 text-[12.5px] leading-snug text-fg-3">{t("dashboard.ai.subtitle")}</p>
        </div>
        {chat && (
          <button type="button" onClick={onOpenChat} className="k-surface-btn flex h-9 shrink-0 items-center gap-1.5 rounded-full px-3 text-[12.5px] font-semibold text-fg-2 hover:text-fg">
            <MessageCircle className="size-4" />
            <span className="hidden sm:inline">{t("dashboard.ai.openChat")}</span>
          </button>
        )}
        {onClose && (
          <button type="button" onClick={onClose} aria-label={t("common.close")} className="grid size-9 shrink-0 place-items-center rounded-full text-fg-3 hover:bg-surface-3 hover:text-fg">
            <X className="size-[18px]" />
          </button>
        )}
      </div>

      {/* an open request for a person (the bot doesn't answer there): say so, and open it in the chat */}
      {e.openRequest && !e.blocked && <TeamNote text={e.agentName ? t("dashboard.ai.withAgent", { name: e.agentName }) : t("dashboard.ai.openRequest")} onView={onOpenChat} />}

      {/* thread */}
      <AnimatePresence initial={false}>
        {has && (
          <motion.div initial={{ opacity: 0, height: 0 }} animate={{ opacity: 1, height: "auto" }} exit={{ opacity: 0, height: 0 }} transition={{ duration: 0.25 }} className={cn("min-h-0 overflow-hidden", sheet && "flex-1")}>
            <div ref={scroller} aria-live="polite" className={cn("relative mt-4 space-y-3.5 overflow-y-auto overscroll-contain pe-1", sheet ? "h-full max-h-none" : "max-h-[340px]")}>
              {e.turns.map((turn) => (
                <motion.div key={turn.id} data-q={turn.role === "you" || undefined} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.18 }}>
                  <Bubble turn={turn} botName={e.botName} extra={turn.role === "you" && turn.chip ? byKey.get(turn.chip)?.extra : undefined} />
                </motion.div>
              ))}
              {e.streaming !== null && (
                <div className="flex items-start gap-2.5" data-testid="ai-streaming">
                  <Spark size={28} />
                  <div className="min-w-0 max-w-[88%]">
                    <div className="mb-1 text-[11.5px] font-semibold text-fg-2">{e.botName}</div>
                    <div className="rounded-[18px] rounded-ss-md bg-surface-2 px-3.5 py-2.5 text-[13.5px] leading-relaxed text-fg-2">{e.streaming ? <Rich text={e.streaming} /> : <Dots label={t("dashboard.ai.thinking", { name: e.botName })} />}</div>
                  </div>
                </div>
              )}
              {/* a question held while a request for a person is open */}
              {e.blocked && (
                <div className="rounded-[16px] border border-line bg-surface px-3.5 py-3" data-testid="ai-blocked">
                  <p className="text-[12.5px] leading-snug text-fg-2">{t("dashboard.ai.blocked", { name: e.botName })}</p>
                  <div className="mt-2.5 flex flex-wrap gap-2">
                    <Button size="sm" variant="ember" onClick={e.closeAndAsk} disabled={e.sending}>
                      {t("dashboard.ai.closeAndAsk", { name: e.botName })}
                    </Button>
                    <Button size="sm" variant="surface" onClick={e.sendToTeam} disabled={e.sending}>
                      {t("dashboard.ai.sendToTeam")}
                    </Button>
                    <Button size="sm" variant="ghost" onClick={onOpenChat}>
                      {t("dashboard.ai.view")}
                    </Button>
                  </div>
                </div>
              )}
              {/* handed over to people (by the bot or the client): no bot answer is coming */}
              {e.withTeam && !e.waiting && <TeamNote text={e.agentName ? t("dashboard.ai.withAgent", { name: e.agentName }) : t("dashboard.ai.passed")} onView={onOpenChat} />}
              {e.slow && <div className="rounded-[14px] bg-warn-soft px-3.5 py-2 text-[12px] text-fg-2">{t("dashboard.ai.slow")}</div>}
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {e.error && <div className="mt-3 rounded-[14px] bg-down-soft px-3.5 py-2 text-[12.5px] text-down">{e.error}</div>}

      {/* follow-up actions once an answer is in */}
      {answered && (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {chat && (
            <Button size="sm" variant="ink" onClick={onOpenChat}>
              <MessageCircle /> {t("dashboard.ai.continueChat")}
            </Button>
          )}
          {chat && !human && (
            <Button size="sm" variant="surface" onClick={e.handover}>
              <UserRound /> {t("support.menu.talkToPerson")}
            </Button>
          )}
          <Button size="sm" variant="ghost" onClick={e.reset}>
            <RotateCcw /> {t("dashboard.ai.newQuestion")}
          </Button>
        </div>
      )}

      {/* composer */}
      <form
        onSubmit={(ev) => {
          ev.preventDefault();
          submit(q);
        }}
        className={cn("flex items-center gap-2 rounded-[16px] border border-line bg-surface p-1.5 ps-4 shadow-[0_1px_2px_rgba(48,28,64,0.04)] transition-colors focus-within:border-ember/50 focus-within:ring-4 focus-within:ring-ember/10", has ? "mt-3" : "mt-4")}
      >
        <input
          ref={inputRef}
          value={q}
          onChange={(ev) => setQ(ev.target.value)}
          maxLength={4000}
          enterKeyHint="send"
          placeholder={lastYou ? t("dashboard.ai.followUp") : t("dashboard.ai.placeholder")}
          aria-label={t("dashboard.ai.title", { name: e.botName })}
          className="h-9 min-w-0 flex-1 bg-transparent text-[14px] outline-none placeholder:text-fg-3"
        />
        <button type="submit" disabled={!q.trim() || e.sending || e.blocked} aria-label={t("common.send")} className="k-accent-btn grid size-9 shrink-0 place-items-center rounded-[12px] transition-opacity disabled:opacity-40">
          <ArrowUp className="size-4" strokeWidth={2.4} />
        </button>
      </form>

      {/* suggestions before the first question */}
      {!has && (
        <div className="mt-3 flex flex-wrap gap-2">
          {chips.map((c) => (
            <button
              key={c.key}
              type="button"
              onClick={() => submit(c.question ?? c.label, c.key)}
              className="rounded-full bg-ember-soft px-3 py-1.5 text-[12.5px] font-semibold text-ember transition-colors hover:bg-[color-mix(in_oklab,var(--k-ember)_18%,transparent)]"
            >
              {c.label}
            </button>
          ))}
        </div>
      )}
      {has && <p className="mt-2 text-center text-[10.5px] text-fg-3">{t("support.disclaimer", { name: e.botName })}</p>}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Phone bottom sheet                                                  */
/* ------------------------------------------------------------------ */

function Sheet({ open, onClose, label, tall, children }: { open: boolean; onClose: () => void; label: string; tall: boolean; children: React.ReactNode }) {
  const [mounted, setMounted] = React.useState(false);
  React.useEffect(() => setMounted(true), []);
  React.useEffect(() => {
    if (!open) return;
    const onKey = (ev: KeyboardEvent) => ev.key === "Escape" && onClose();
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", onKey);
    return () => {
      document.body.style.overflow = prev;
      window.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);
  if (!mounted) return null;
  return createPortal(
    <AnimatePresence>
      {open && (
        <div className="fixed inset-0 z-50 md:hidden">
          <motion.div className="absolute inset-0 bg-[color-mix(in_oklab,#2a1d3a_58%,transparent)] backdrop-blur-[18px]" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} onClick={onClose} />
          <motion.div
            role="dialog"
            aria-modal="true"
            aria-label={label}
            className={cn("absolute inset-x-0 bottom-0 flex max-h-[min(86dvh,680px)] flex-col rounded-t-[28px] bg-surface px-4 pb-[max(16px,env(safe-area-inset-bottom))] pt-2.5 shadow-[var(--k-shadow-pop)] transition-[height] duration-300", tall ? "h-[min(86dvh,680px)]" : "h-auto")}
            initial={{ y: "100%" }}
            animate={{ y: 0 }}
            exit={{ y: "100%" }}
            transition={{ type: "spring", bounce: 0.12, duration: 0.42 }}
          >
            <span aria-hidden className="mx-auto mb-3 block h-1.5 w-10 shrink-0 rounded-full bg-surface-3" />
            {children}
          </motion.div>
        </div>
      )}
    </AnimatePresence>,
    document.body,
  );
}

/* ------------------------------------------------------------------ */
/* Entry                                                               */
/* ------------------------------------------------------------------ */

function View({ e, chips, chat }: { e: AiEngine; chips: AiChip[]; chat: boolean }) {
  const t = useT();
  const [sheet, setSheet] = React.useState(false);
  const cardInput = React.useRef<HTMLInputElement>(null);
  const sheetInput = React.useRef<HTMLInputElement>(null);
  const openChat = () => {
    setSheet(false);
    openSupportChat();
  };
  React.useEffect(() => {
    if (sheet) setTimeout(() => sheetInput.current?.focus(), 350);
  }, [sheet]);
  const label = t("dashboard.ai.title", { name: e.botName });
  return (
    <section aria-label={label}>
      {/* tablets and desktops: the card */}
      <div className="k-card k-ai-card hidden p-5 md:block sm:p-6">
        <Panel e={e} chips={chips} variant="card" onOpenChat={openChat} chat={chat} inputRef={cardInput} />
      </div>
      {/* phones: a compact pill in the page flow that opens a bottom sheet */}
      <button type="button" onClick={() => setSheet(true)} aria-haspopup="dialog" className="k-card k-ai-card flex h-14 w-full items-center gap-3 rounded-full py-2 pe-2 ps-2 text-start md:hidden">
        <Spark size={40} />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[14px] font-semibold text-fg">{label}</span>
          <span className="block truncate text-[11.5px] text-fg-3">{e.turns.length ? e.turns[e.turns.length - 1]!.text : t("support.composer.ask", { name: e.botName })}</span>
        </span>
        <span className="k-accent-btn grid size-10 shrink-0 place-items-center rounded-full">
          <ArrowUp className="size-[18px]" strokeWidth={2.4} />
        </span>
      </button>
      <Sheet open={sheet} onClose={() => setSheet(false)} label={label} tall={e.turns.length > 0 || e.streaming !== null}>
        <Panel e={e} chips={chips} variant="sheet" onOpenChat={openChat} chat={chat} onClose={() => setSheet(false)} inputRef={sheetInput} />
      </Sheet>
    </section>
  );
}

function LiveAskAi({ chips, chat }: { chips: AiChip[]; chat: boolean }) {
  const t = useT();
  const e = useLiveAi("Kalks AI", t("support.unavailable"));
  return <View e={e} chips={chips} chat={chat} />;
}

function DemoAskAi({ chips, answer }: { chips: AiChip[]; answer: (q: string, chip?: string) => string }) {
  const t = useT();
  const me = useSession();
  const e = useDemoAi("Kalks AI", answer, { name: SUPPORT_AGENT.name, reply: (q) => `Hi ${me.first_name}, ${SUPPORT_AGENT.name.split(" ")[0]} here from Client Support. ${agentAnswer(q)}` }, t("dashboard.ai.connecting"));
  return <View e={e} chips={chips} chat />;
}

/** The Overview's AI entry: the real support bot in live builds, canned answers (`demoAnswer`) in demo builds.
 *  `chat` false (the broker switched the support chat off): no way into the chat or to a person from here. */
export function AskAi({ chips, demoAnswer, chat = true }: { chips: AiChip[]; demoAnswer?: (q: string, chip?: string) => string; chat?: boolean }) {
  return IS_DEMO ? <DemoAskAi chips={chips} answer={demoAnswer ?? ((q) => botAnswer(q).text)} /> : <LiveAskAi chips={chips} chat={chat} />;
}

/** Link used in answers' extras (e.g. "Open account"). */
export function AiLink({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <Link href={href} className="inline-flex h-9 items-center gap-1.5 rounded-[12px] bg-ember-soft px-3 text-[12.5px] font-semibold text-ember">
      {children} <ChevronRight className="size-4 rtl:-scale-x-100" />
    </Link>
  );
}

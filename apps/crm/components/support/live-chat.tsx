"use client";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import { toast } from "sonner";
import { ArrowUp, FileText, Loader2, MoreHorizontal, Paperclip, RotateCcw, SendHorizontal, Sparkles, Star, UserRound, X, XCircle } from "lucide-react";
import { Avatar, Button, Chip, IconButton, Menu, cn } from "@/components/kit";
import { useSession } from "@/components/session";
import { realtime, type Frame } from "@/lib/realtime";
import { intlTag } from "@kalks/i18n/locales";
import { tr, useT } from "@kalks/i18n/react";

/* ------------------------------------------------------------------ */
/* Types (services/support client API)                                  */
/* ------------------------------------------------------------------ */

export type ConvStatus = "bot" | "waiting" | "assigned" | "resolved";
export type Conversation = {
  id: number;
  subject: string;
  status: ConvStatus;
  assigneeName: string | null;
  handedOverAt: string | null;
  clientUnread: number;
  csat: { rating: number; comment: string | null } | null;
  resolvedAt: string | null;
  createdAt: string;
  lastMessageAt: string;
  preview: string;
};
export type Attachment = { id: number; name: string; mime: string; size: number };
export type Message = {
  id: number;
  conversationId: number;
  author: "client" | "bot" | "agent" | "system";
  authorName: string | null;
  body: string;
  attachment: Attachment | null;
  meta: { kind?: string; cites?: { slug: string; title: string }[]; streamId?: string; rating?: number; agentName?: string };
  createdAt: string;
};
type Settings = { botName: string; greeting: string; ai: boolean; agentsOnline: number; maxAttachmentMb: number };
type Home = { settings: Settings; conversation: Conversation | null; messages: Message[] };

async function api<T>(path: string, init?: { method?: string; body?: unknown }): Promise<{ ok: boolean; status: number; data: T }> {
  try {
    const r = await fetch(`/api/support/${path}`, {
      method: init?.method ?? (init?.body !== undefined ? "POST" : "GET"),
      headers: init?.body !== undefined ? { "content-type": "application/json" } : undefined,
      body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
      cache: "no-store",
    });
    return { ok: r.ok, status: r.status, data: (await r.json().catch(() => ({}))) as T };
  } catch {
    return { ok: false, status: 0, data: {} as T };
  }
}

export function errMsg(d: unknown, fallback?: string) {
  const e = (d as { error?: { message?: string } })?.error;
  return e?.message || fallback || tr("common.errorRetry");
}

export function hhmm(iso: string) {
  const d = new Date(iso);
  return d.toLocaleTimeString(intlTag(tr.locale), { hour: "2-digit", minute: "2-digit" });
}

/* ------------------------------------------------------------------ */
/* Rendering helpers                                                    */
/* ------------------------------------------------------------------ */

/** Minimal safe markdown: **bold**, "- " bullets, paragraphs. Text only, never HTML. */
export function Rich({ text }: { text: string }) {
  const blocks = text.split(/\n{2,}/);
  const inline = (s: string, k: string) =>
    s.split("**").map((p, i) =>
      i % 2 ? (
        <strong key={`${k}-${i}`} className="font-semibold text-fg">
          {p}
        </strong>
      ) : (
        <React.Fragment key={`${k}-${i}`}>{p}</React.Fragment>
      ),
    );
  return (
    <>
      {blocks.map((b, bi) => {
        const lines = b.split("\n");
        const list = lines.every((l) => /^\s*[-•]\s+/.test(l) || !l.trim());
        if (list)
          return (
            <ul key={bi} className={cn("list-disc space-y-0.5 ps-4", bi > 0 && "mt-2")}>
              {lines.filter((l) => l.trim()).map((l, li) => (
                <li key={li}>{inline(l.replace(/^\s*[-•]\s+/, ""), `${bi}-${li}`)}</li>
              ))}
            </ul>
          );
        return (
          <p key={bi} className={cn(bi > 0 && "mt-2")}>
            {lines.map((l, li) => (
              <React.Fragment key={li}>
                {li > 0 && <br />}
                {inline(l, `${bi}-${li}`)}
              </React.Fragment>
            ))}
          </p>
        );
      })}
    </>
  );
}

export function BotAvatar({ size = 32 }: { size?: number }) {
  return (
    <span className="grid shrink-0 place-items-center rounded-full bg-ember text-white" style={{ width: size, height: size }}>
      <Sparkles style={{ width: size * 0.45, height: size * 0.45 }} />
    </span>
  );
}

function AttachmentView({ a, mine }: { a: Attachment; mine: boolean }) {
  const t = useT();
  const url = `/api/support/attachments/${a.id}`;
  if (a.mime.startsWith("image/"))
    return (
      <a href={url} target="_blank" rel="noopener" className="mt-1 block">
        {/* eslint-disable-next-line @next/next/no-img-element */}
        <img src={url} alt={a.name} className="max-h-48 max-w-[240px] rounded-xl border border-line object-cover" />
      </a>
    );
  return (
    <a href={url} target="_blank" rel="noopener" className={cn("mt-1 inline-flex items-center gap-3 rounded-2xl border px-3.5 py-2.5 text-start", mine ? "border-ember/30 bg-ember-soft" : "border-line bg-surface-2")}>
      <span className="grid size-9 place-items-center rounded-xl bg-surface-3 text-ember">
        <FileText className="size-4" />
      </span>
      <span>
        <span className="block max-w-[180px] truncate text-[13px] font-medium text-fg">{a.name}</span>
        <span className="block text-[11px] text-fg-3">{t("support.attachmentSize", { size: Math.max(1, Math.round(a.size / 1024)) })}</span>
      </span>
    </a>
  );
}

function Dots() {
  return (
    <span className="flex items-center gap-1 px-1 py-1">
      {[0, 1, 2].map((i) => (
        <motion.span key={i} className="size-1.5 rounded-full bg-fg-3" animate={{ opacity: [0.3, 1, 0.3] }} transition={{ duration: 1, repeat: Infinity, delay: i * 0.18 }} />
      ))}
    </span>
  );
}

export function MessageRow({ m, botName, meName, room }: { m: Message; botName: string; meName: string; room?: boolean }) {
  const t = useT();
  if (m.author === "system") {
    if (m.meta.kind === "join")
      return (
        <div className="flex justify-center">
          <div className="flex items-center gap-2 rounded-full border border-up/25 bg-up-soft py-1 ps-1 pe-3.5 text-[12px] text-fg-2">
            <Avatar name={m.meta.agentName ?? t("support.agent")} size={22} />
            <span>{m.body}</span>
            <span className="font-mono text-[10.5px] text-fg-3">{hhmm(m.createdAt)}</span>
          </div>
        </div>
      );
    return (
      <div className="flex justify-center">
        <span className="max-w-[90%] rounded-full bg-surface-2 px-3 py-1 text-center text-[11.5px] text-fg-3">{m.body}</span>
      </div>
    );
  }
  const mine = m.author === "client";
  const name = mine ? t("support.you") : m.author === "bot" ? botName : (m.authorName ?? t("support.supportName"));
  if (room) {
    if (mine)
      return (
        <div className="flex justify-end">
          <div className="max-w-[78%] text-start">
            {m.body && (
              <div className="rounded-[22px] bg-surface-3 px-4 py-2.5 text-[15px] leading-7 text-fg">
                <Rich text={m.body} />
              </div>
            )}
            {m.attachment && <AttachmentView a={m.attachment} mine />}
          </div>
        </div>
      );
    return (
      <div className="flex items-start gap-3">
        {m.author === "bot" ? <BotAvatar size={30} /> : <Avatar name={m.authorName ?? t("support.agent")} size={30} />}
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center gap-2 text-[12px] text-fg-3">
            <span className="font-semibold text-fg-2">{name}</span>
            <span className="font-mono">{hhmm(m.createdAt)}</span>
          </div>
          {m.body && (
            <div className="text-[15px] leading-7 text-fg">
              <Rich text={m.body} />
            </div>
          )}
          {m.attachment && <AttachmentView a={m.attachment} mine={false} />}
          {m.author === "bot" && m.meta.cites && m.meta.cites.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1.5">
              {m.meta.cites.map((c) => (
                <span key={c.slug} className="rounded-full border border-line px-2.5 py-0.5 text-[11px] text-fg-3">
                  {c.title}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    );
  }
  return (
    <div className={cn("flex items-end gap-2.5", mine && "flex-row-reverse")}>
      {m.author === "bot" && <BotAvatar size={28} />}
      {m.author === "agent" && <Avatar name={m.authorName ?? t("support.agent")} size={28} />}
      {mine && <Avatar name={meName} size={28} />}
      <div className={cn("max-w-[80%]", mine && "text-end")}>
        <div className={cn("mb-1 flex items-center gap-2 text-[11px] text-fg-3", mine && "justify-end")}>
          <span className="font-medium text-fg-2">{name}</span>
          <span className="font-mono">{hhmm(m.createdAt)}</span>
        </div>
        {m.body && (
          <div
            className={cn(
              "inline-block rounded-2xl px-4 py-2.5 text-start text-[13.5px] leading-relaxed",
              mine ? "rounded-ee-md bg-ember text-white [&_strong]:text-white" : m.author === "bot" ? "rounded-es-md border border-ember/20 bg-surface-2 text-fg-2" : "rounded-es-md border border-line bg-surface-3 text-fg",
            )}
          >
            <Rich text={m.body} />
          </div>
        )}
        {m.attachment && <AttachmentView a={m.attachment} mine={mine} />}
        {m.author === "bot" && m.meta.cites && m.meta.cites.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-1.5">
            {m.meta.cites.map((c) => (
              <span key={c.slug} className="rounded-full border border-line bg-surface-2 px-2 py-0.5 text-[10.5px] text-fg-3">
                {c.title}
              </span>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Live chat                                                            */
/* ------------------------------------------------------------------ */

// the glass suggestion chips on the Support photo (same as Home's AI bar)
const HERO_CHIP = "border border-white/20 bg-white/12 text-[12.5px] font-semibold text-white backdrop-blur-xl hover:bg-white/22";

// suggested first questions (translation keys; the translated text is sent as the message)
const QUICK = ["support.quick.verify", "support.quick.deposit", "support.quick.withdrawal", "support.quick.stopOut"] as const;

/**
 * Live support chat: the AI bot answers first (streamed), hands over to a human agent on request or when unsure,
 * agents reply live, attachments (images / PDF), end the chat and rate it. Used by /support and the floating widget.
 */
export function LiveChat({ variant = "page", onClose, onUnread }: { variant?: "page" | "widget" | "room" | "hero"; onClose?: () => void; onUnread?: (n: number) => void }) {
  // "room": a Claude / ChatGPT-style chat room. "hero": the Support page like Home (founder 2026-10-10): one slim glass
  // bar on the photo, the conversation in a glass panel above it, nothing else.
  const hero = variant === "hero";
  const room = variant === "room" || hero;
  const t = useT();
  const me = useSession();
  const widget = variant === "widget";
  const [home, setHome] = React.useState<Home | null>(null);
  const [failed, setFailed] = React.useState(false);
  const [conv, setConv] = React.useState<Conversation | null>(null);
  const [msgs, setMsgs] = React.useState<Message[]>([]);
  const [stream, setStream] = React.useState<{ id: string; text: string } | null>(null);
  const [agentTyping, setAgentTyping] = React.useState(false);
  const [input, setInput] = React.useState("");
  const [sending, setSending] = React.useState(false);
  const [uploading, setUploading] = React.useState(false);
  const [rating, setRating] = React.useState(0);
  const [comment, setComment] = React.useState("");
  const scroller = React.useRef<HTMLDivElement>(null);
  const fileRef = React.useRef<HTMLInputElement>(null);
  const convRef = React.useRef<Conversation | null>(null);
  const typingTimer = React.useRef<ReturnType<typeof setTimeout> | null>(null);
  const lastTypingSent = React.useRef(0);
  convRef.current = conv;

  const load = React.useCallback(async () => {
    const r = await api<Home>("me");
    if (!r.ok) {
      setFailed(true);
      return;
    }
    setFailed(false);
    setHome(r.data);
    setConv(r.data.conversation);
    setMsgs(r.data.messages);
  }, []);

  React.useEffect(() => {
    void load();
  }, [load]);

  const addMsg = React.useCallback((m: Message) => {
    setMsgs((xs) => (xs.some((x) => x.id === m.id) ? xs : [...xs, m].sort((a, b) => a.id - b.id)));
  }, []);

  React.useEffect(
    () =>
      realtime().subscribe((f: Frame) => {
        const cur = convRef.current;
        switch (f.type) {
          case "reconnected":
            void load();
            break;
          case "message": {
            const m = f.message as Message;
            if (cur && m.conversationId !== cur.id) return;
            if (!cur) return void load();
            addMsg(m);
            if (m.author === "bot") setStream(null);
            if (m.author === "agent") setAgentTyping(false);
            break;
          }
          case "conversation": {
            const c = f.conversation as Conversation;
            if (!cur || c.id === cur.id) setConv(c);
            break;
          }
          case "bot.typing":
            if (cur && f.conversationId === cur.id) setStream({ id: String(f.streamId), text: "" });
            break;
          case "bot.delta":
            if (cur && f.conversationId === cur.id) setStream((s) => (s && s.id === f.streamId ? { ...s, text: s.text + String(f.text) } : { id: String(f.streamId), text: String(f.text) }));
            break;
          case "typing":
            if (cur && f.conversationId === cur.id && f.from === "agent") {
              setAgentTyping(true);
              if (typingTimer.current) clearTimeout(typingTimer.current);
              typingTimer.current = setTimeout(() => setAgentTyping(false), 4000);
            }
            break;
        }
      }),
    [load, addMsg],
  );

  // unread badge for the floating button; the open chat marks everything read
  React.useEffect(() => {
    onUnread?.(conv?.clientUnread ?? 0);
    if (conv && conv.clientUnread > 0 && document.visibilityState === "visible") void api("read", { body: {} });
  }, [conv, onUnread]);

  React.useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [msgs.length, stream?.text, agentTyping, conv?.status]);

  const settings = home?.settings;
  const botName = settings?.botName ?? "Kalks AI";
  const status = conv?.status;
  const resolved = status === "resolved";
  const human = status === "waiting" || status === "assigned";

  const send = async (raw?: string, attachmentId?: number) => {
    const text = (raw ?? input).trim();
    if ((!text && !attachmentId) || sending) return;
    setSending(true);
    if (raw === undefined) setInput("");
    if (resolved) {
      setConv(null);
      setMsgs([]);
      setRating(0);
    }
    const r = await api<{ conversation: Conversation; message: Message }>("messages", { body: { body: text, attachmentId } });
    setSending(false);
    if (!r.ok) {
      toast.error(t("support.toast.notSent"), { description: errMsg(r.data) });
      if (raw === undefined) setInput(text);
      return;
    }
    setConv(r.data.conversation);
    addMsg(r.data.message);
    if (r.data.conversation.status === "bot") setStream((s) => s ?? { id: "pending", text: "" });
  };

  const handover = async () => {
    const r = await api<{ conversation: Conversation }>("handover", { body: {} });
    if (!r.ok) return toast.error(t("support.toast.teamUnreachable"), { description: errMsg(r.data) });
    setConv(r.data.conversation);
    if (!convRef.current) void load();
  };

  const endChat = async () => {
    if (!conv) return;
    const r = await api<{ conversation: Conversation }>(`conversations/${conv.id}/resolve`, { body: {} });
    if (!r.ok) return toast.error(t("support.toast.endFailed"), { description: errMsg(r.data) });
    setConv(r.data.conversation);
  };

  const rate = async () => {
    if (!conv || !rating) return;
    const r = await api<{ conversation: Conversation }>(`conversations/${conv.id}/rate`, { body: { rating, comment } });
    if (!r.ok) return toast.error(t("support.toast.rateFailed"), { description: errMsg(r.data) });
    setConv(r.data.conversation);
    toast.success(t("support.toast.thanks"));
  };

  const newChat = () => {
    setConv(null);
    setMsgs([]);
    setRating(0);
    setComment("");
    setStream(null);
  };

  const onFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    e.target.value = "";
    if (!f) return;
    const max = (settings?.maxAttachmentMb ?? 10) * 1024 * 1024;
    if (f.size > max) return toast.error(t("support.toast.fileTooLarge"), { description: t("support.toast.fileTooLargeText", { mb: settings?.maxAttachmentMb ?? 10 }) });
    if (!/^image\/(png|jpe?g|gif|webp)$/.test(f.type) && f.type !== "application/pdf") return toast.error(t("support.toast.unsupported"), { description: t("support.toast.unsupportedText") });
    setUploading(true);
    try {
      const r = await fetch("/api/support/attachments", { method: "POST", headers: { "content-type": f.type, "x-file-name": encodeURIComponent(f.name) }, body: f });
      const d = (await r.json().catch(() => ({}))) as { attachment?: Attachment };
      if (!r.ok || !d.attachment) throw new Error(errMsg(d, t("support.error.uploadFailed")));
      await send(input.trim(), d.attachment.id);
      setInput("");
    } catch (err) {
      toast.error(t("support.toast.uploadFailed"), { description: (err as Error).message });
    } finally {
      setUploading(false);
    }
  };

  const onType = (v: string) => {
    setInput(v);
    if (human && Date.now() - lastTypingSent.current > 3000) {
      lastTypingSent.current = Date.now();
      void api("typing", { body: {} });
    }
  };

  // the Support page's sidebar "New chat" button
  React.useEffect(() => {
    const on = () => newChat();
    window.addEventListener("kalks:support-new-chat", on);
    return () => window.removeEventListener("kalks:support-new-chat", on);
  });

  const agentName = conv?.assigneeName;
  const headerName = status === "assigned" && agentName ? agentName : status === "waiting" ? t("support.header.supportTeam") : botName;
  const headerSub =
    status === "assigned" ? t("support.header.agentSub") : status === "waiting" ? (settings?.agentsOnline ? t("support.header.connecting") : t("support.header.replySoon")) : settings?.ai === false ? t("support.header.helpCentre") : t("support.header.instant");
  const showGreeting = !conv || msgs.length === 0;
  const canRate = resolved && !conv?.csat && msgs.some((m) => m.author === "agent" || m.author === "bot");

  const menu = (
    <Menu
      trigger={
        <IconButton size="sm" aria-label={t("support.menu.aria")}>
          <MoreHorizontal />
        </IconButton>
      }
      items={[
        { label: t("support.menu.talkToPerson"), icon: <UserRound />, onSelect: () => (human ? toast.info(status === "assigned" ? t("support.toast.chattingWith", { name: agentName }) : t("support.toast.inQueue")) : void handover()) },
        ...(conv && !resolved ? [{ label: t("support.menu.endChat"), icon: <XCircle />, onSelect: () => void endChat() }] : []),
        "sep" as const,
        { label: t("support.menu.newChat"), icon: <RotateCcw />, onSelect: newChat },
      ]}
    />
  );
  const statusChip = (
    <Chip size="sm" tone={status === "assigned" ? "up" : status === "waiting" ? "warn" : resolved ? "neutral" : "ember"} dot>
      {status === "assigned" ? t("support.chip.liveAgent") : status === "waiting" ? t("support.status.waiting") : resolved ? t("support.status.resolved") : t("support.status.bot")}
    </Chip>
  );
  const threadOpen = !!home && !showGreeting;

  return (
    <div className={cn("flex flex-col", hero ? "mx-auto w-full max-w-[780px] text-start" : cn("overflow-hidden", room ? "h-full min-h-0" : cn("k-card", widget ? "h-full rounded-[18px]" : "h-[calc(100vh-180px)] min-h-[560px]")))} data-testid="support-chat">
      {/* Header */}
      {!hero && (
      <div className={cn("flex items-center gap-3 border-b border-line", room ? "px-4 py-3 sm:px-6" : "px-5 py-4")}>
        {status === "assigned" && agentName ? (
          <div className="relative flex items-center">
            <BotAvatar size={28} />
            <Avatar name={agentName} size={36} online className="-ms-2.5 rounded-full ring-2 ring-surface" />
          </div>
        ) : (
          <BotAvatar size={36} />
        )}
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2 text-[15px] font-medium">
            <span className="truncate">{headerName}</span>
            {statusChip}
          </div>
          <div className="truncate text-[12px] text-fg-3">{headerSub}</div>
        </div>
        {menu}
        {widget && onClose && (
          <IconButton size="sm" aria-label={t("support.closeChat")} onClick={onClose}>
            <X />
          </IconButton>
        )}
      </div>
      )}

      {/* hero: who is answering + the menu, on top of the glass conversation */}
      {hero && threadOpen && (
        <div className="mb-2 flex items-center gap-2 px-1">
          <span className="truncate text-[13px] font-semibold text-white">{headerName}</span>
          {statusChip}
          <span className="ms-auto">{menu}</span>
        </div>
      )}

      {/* Messages */}
      <div
        ref={scroller}
        className={cn(
          "flex flex-col overflow-y-auto",
          hero
            ? cn("mb-3 max-h-[min(56vh,560px)] overscroll-contain rounded-[24px] border border-white/15 bg-black/55 px-4 py-5 backdrop-blur-2xl sm:px-6", !threadOpen && !failed && "hidden")
            : cn("flex-1", room ? "px-4 py-6 sm:px-8" : "px-4 py-5 sm:px-5"),
        )}
      >
        <div className={cn("mt-auto", hero ? "space-y-6" : room ? "mx-auto w-full max-w-[780px] space-y-7" : "space-y-4")}>
          {!home && !failed && !hero && (
            <div className="flex justify-center py-10 text-fg-3">
              <Loader2 className="size-5 animate-spin" />
            </div>
          )}
          {failed && (
            <div className="py-10 text-center text-[13px] text-fg-3">
              {t("support.unavailable")}{" "}
              <button className="text-ember hover:underline" onClick={() => void load()}>
                {t("common.retry")}
              </button>
            </div>
          )}
          {home && showGreeting && room && !hero && (
            <div className="flex flex-col items-center py-10 text-center">
              <BotAvatar size={52} />
              <h2 className="k-display mt-5 text-[30px] font-semibold tracking-[-0.025em] sm:text-[36px]">{t.dyn("support.room.title", "How can we help, {name}?", { name: me.first_name })}</h2>
              <p className="mt-2 max-w-[520px] text-[14.5px] leading-relaxed text-fg-2">{settings?.greeting.replace(/^Hi[^.]*\.\s*/, "") || t("support.header.instant")}</p>
            </div>
          )}
          {home && showGreeting && !room && (
            <div className="flex items-end gap-2.5">
              <BotAvatar size={28} />
              <div className="max-w-[80%]">
                <div className="mb-1 text-[11px] font-medium text-fg-2">{botName}</div>
                <div className="inline-block rounded-2xl rounded-es-md border border-ember/20 bg-surface-2 px-4 py-2.5 text-[13.5px] leading-relaxed text-fg-2">
                  {t("support.greeting", { name: me.first_name })} {settings?.greeting.replace(/^Hi[^.]*\.\s*/, "")}
                </div>
              </div>
            </div>
          )}
          <AnimatePresence initial={false}>
            {msgs.map((m) => (
              <motion.div key={m.id} initial={{ opacity: 0, y: 6 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.2 }}>
                <MessageRow m={m} botName={botName} meName={me.name} room={room} />
              </motion.div>
            ))}
          </AnimatePresence>
          {stream && status === "bot" && room && (
            <div className="flex items-start gap-3" data-testid="bot-streaming">
              <BotAvatar size={30} />
              <div className="min-w-0 flex-1">
                <div className="mb-1 text-[12px] font-semibold text-fg-2">{botName}</div>
                <div className="text-[15px] leading-7 text-fg">{stream.text ? <Rich text={stream.text} /> : <Dots />}</div>
              </div>
            </div>
          )}
          {stream && status === "bot" && !room && (
            <div className="flex items-end gap-2.5" data-testid="bot-streaming">
              <BotAvatar size={28} />
              <div className="max-w-[80%]">
                <div className="mb-1 text-[11px] font-medium text-fg-2">{botName}</div>
                <div className="inline-block rounded-2xl rounded-es-md border border-ember/20 bg-surface-2 px-4 py-2.5 text-[13.5px] leading-relaxed text-fg-2">{stream.text ? <Rich text={stream.text} /> : <Dots />}</div>
              </div>
            </div>
          )}
          {agentTyping && status === "assigned" && (
            <div className="flex items-end gap-2.5">
              <Avatar name={agentName ?? t("support.agent")} size={28} />
              <div className="rounded-2xl rounded-es-md border border-line bg-surface-3 px-3 py-2">
                <Dots />
              </div>
            </div>
          )}
          {canRate && (
            <div className="mx-auto max-w-xs rounded-2xl border border-line bg-surface-2 px-4 py-3 text-center" data-testid="csat">
              <div className="text-[12.5px] text-fg-2">{t("support.csat.question")}</div>
              <div className="mt-1.5 flex justify-center gap-1">
                {[1, 2, 3, 4, 5].map((n) => (
                  <button key={n} aria-label={t("support.csat.stars", { count: n })} onClick={() => setRating(n)} className={cn("transition-colors", n <= rating ? "text-gold" : "text-fg-3 hover:text-gold")}>
                    <Star className={cn("size-5", n <= rating && "fill-current")} />
                  </button>
                ))}
              </div>
              {rating > 0 && (
                <div className="mt-2 space-y-2">
                  <textarea value={comment} onChange={(e) => setComment(e.target.value)} rows={2} maxLength={1000} placeholder={t("support.csat.placeholder")} className="w-full resize-none rounded-xl border border-line bg-surface px-3 py-2 text-[12.5px] outline-none focus:border-ember/50" />
                  <Button size="xs" variant="ember" onClick={() => void rate()}>
                    {t("support.csat.send")}
                  </Button>
                </div>
              )}
            </div>
          )}
          {resolved && conv?.csat && (
            <div className="flex justify-center">
              <span className="flex items-center gap-1 rounded-full bg-surface-2 px-3 py-1 text-[11.5px] text-fg-3">
                {t("support.csat.rated", { rating: conv.csat.rating })} <Star className="size-3 fill-current text-gold" />
              </span>
            </div>
          )}
        </div>
      </div>

      {/* Composer */}
      <div className={cn(hero ? "" : room ? "px-4 pb-4 pt-2 sm:px-8" : "border-t border-line px-4 pb-4 pt-3 sm:px-5")}>
        <div className={cn(room && "mx-auto w-full max-w-[780px]", hero && "flex flex-col")}>
        {(showGreeting || (status === "bot" && msgs.length < 3 && !hero)) && (
          <div className={cn("mb-3 flex gap-1.5 overflow-x-auto pb-0.5", room && "flex-wrap justify-center overflow-visible", hero && "order-last mb-0 mt-4 gap-2")}>
            {QUICK.map((q) => (
              <button key={q} onClick={() => void send(t(q))} disabled={sending || !home} className={cn("shrink-0 rounded-full px-3 py-1.5 transition-colors disabled:opacity-50", hero ? HERO_CHIP : "border border-line bg-surface-2 text-[12px] text-fg-2 hover:border-ember/40 hover:text-fg")}>
                {t(q)}
              </button>
            ))}
            {!human && (
              <button onClick={() => void handover()} disabled={!home} className={cn("shrink-0 rounded-full px-3 py-1.5 transition-colors", hero ? HERO_CHIP : "border border-line bg-surface-2 text-[12px] text-fg-2 hover:border-ember/40 hover:text-fg")}>
                {t("support.menu.talkToPerson")}
              </button>
            )}
          </div>
        )}
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void send();
          }}
          className={cn(
            "flex items-center gap-2 border transition-colors",
            hero
              ? "h-[60px] rounded-full border-white/25 bg-white/14 p-2 ps-3 text-white shadow-[0_24px_70px_-28px_rgba(0,0,0,.75)] backdrop-blur-2xl backdrop-saturate-150 focus-within:border-white/55 focus-within:bg-white/20"
              : room
                ? "min-h-[60px] rounded-[28px] border-line-2 bg-surface p-2 ps-3 shadow-[0_18px_50px_-30px_rgba(0,0,0,.8)] focus-within:border-ember/50"
                : "rounded-[18px] border-line bg-surface-2 p-1.5 ps-2 focus-within:border-ember/50",
          )}
        >
          <input ref={fileRef} type="file" className="hidden" onChange={onFile} accept="image/png,image/jpeg,image/gif,image/webp,application/pdf" />
          <button type="button" onClick={() => fileRef.current?.click()} disabled={uploading || !home} aria-label={t("support.composer.attach")} className={cn("grid size-9 shrink-0 place-items-center rounded-full transition-colors disabled:opacity-50", hero ? "text-white/75 hover:bg-white/15 hover:text-white" : "text-fg-3 hover:bg-surface-3 hover:text-fg")}>
            {uploading ? <Loader2 className="size-4 animate-spin" /> : <Paperclip className="size-4" />}
          </button>
          <input
            value={input}
            onChange={(e) => onType(e.target.value)}
            maxLength={4000}
            placeholder={status === "assigned" && agentName ? t("support.composer.messageTo", { name: agentName.split(" ")[0] }) : resolved ? t("support.composer.newChat") : t("support.composer.ask", { name: botName })}
            aria-label={t("support.composer.aria")}
            className={cn("h-9 min-w-0 flex-1 bg-transparent outline-none", hero ? "text-[15.5px] text-white placeholder:text-white/65" : cn("placeholder:text-fg-3", room ? "text-[15.5px]" : "text-[14px]"))}
          />
          <button type="submit" disabled={!input.trim() || sending || !home} aria-label={t("common.send")} className={cn("grid shrink-0 place-items-center rounded-full transition-opacity disabled:opacity-40", hero ? "k-accent-btn size-11" : "k-ember-btn size-9")}>
            {sending ? <Loader2 className="size-4 animate-spin" /> : hero ? <ArrowUp className="size-4" strokeWidth={2.4} /> : <SendHorizontal className="size-4 rtl:-scale-x-100" />}
          </button>
        </form>
        {(!hero || threadOpen) && <div className={cn("mt-2 text-center text-[10.5px]", hero ? "text-white/60" : "text-fg-3")}>{t("support.disclaimer", { name: botName })}</div>}
        </div>
      </div>
    </div>
  );
}

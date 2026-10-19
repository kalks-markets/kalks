"use client";

import * as React from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  ArrowRightLeft,
  BookOpen,
  Check,
  CheckCheck,
  ChevronDown,
  Clock,
  FileText,
  Lock,
  MessageSquareText,
  MoreHorizontal,
  Paperclip,
  PanelRight,
  RefreshCw,
  Search,
  SendHorizontal,
  Smile,
  UserPlus,
  Wand2,
} from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Chip, Flag, IconButton, Kbd, Menu, Popover, Segmented, Tooltip, cn } from "@kalks/ui";
import { SUP_AGENTS, SUP_CANNED, type SupConversation, type SupMessage } from "@kalks/mock/admin-growth-support";
import { AiSpark, ChannelBadge, ConvStatusChip, SlaCountdown, VarText, closeFloating, fillVars } from "./shared";

/* ------------------------------------------------------------------ */
/* Messages                                                            */
/* ------------------------------------------------------------------ */

function MessageView({ m, conv }: { m: SupMessage; conv: SupConversation }) {
  if (m.from === "handoff") {
    return (
      <div className="flex items-center gap-3 py-2">
        <span className="h-px flex-1 bg-gradient-to-r from-transparent to-warn/40" />
        <span className="flex items-center gap-2 rounded-full border border-warn/30 bg-warn-soft px-3 py-1 text-[11.5px] text-warn">
          <ArrowRightLeft className="size-3.5" />
          {m.author ? (
            <>
              Handed to
              <Avatar src={m.authorPhoto} name={m.author} size={16} />
              <span className="font-medium text-fg">{m.author}</span>
            </>
          ) : (
            <span className="font-medium">Handed to human queue</span>
          )}
          <span className="text-warn/70">· reason: {m.text}</span>
          <span className="font-mono text-[10.5px] text-fg-3">{m.time}</span>
        </span>
        <span className="h-px flex-1 bg-gradient-to-l from-transparent to-warn/40" />
      </div>
    );
  }
  if (m.from === "system") {
    return (
      <div className="flex justify-center py-1">
        <span className="flex items-center gap-1.5 rounded-full bg-surface-2 px-3 py-1 text-[11.5px] text-fg-3">
          <CheckCheck className="size-3.5 text-up" /> {m.text} · <span className="font-mono">{m.time}</span>
        </span>
      </div>
    );
  }
  if (m.from === "note") {
    return (
      <div className="flex justify-end">
        <div className="max-w-[85%] rounded-[18px] rounded-tr-md border border-warn/30 bg-warn-soft px-4 py-3">
          <div className="mb-1 flex items-center gap-1.5 text-[11px] font-medium text-warn">
            <Lock className="size-3" /> Internal note · {m.author} · staff only
          </div>
          <p className="text-[13.5px] leading-relaxed text-fg">{m.text}</p>
          <div className="mt-1 text-right font-mono text-[10.5px] text-fg-3">{m.time}</div>
        </div>
      </div>
    );
  }
  if (m.from === "client") {
    return (
      <div className="flex items-end gap-2.5">
        <Avatar src={conv.client.photo} name={conv.client.name} size={28} />
        <div className="max-w-[78%]">
          <div className="rounded-[18px] rounded-bl-md border border-line bg-surface-2 px-4 py-2.5 text-[13.5px] leading-relaxed text-fg">
            {m.text}
            {m.attachment && (
              <button onClick={() => toast.info(`Opening ${m.attachment!.name}`)} className="mt-2 flex w-full items-center gap-2.5 rounded-xl border border-line bg-surface-3 px-3 py-2 text-left hover:border-fg-3">
                <FileText className="size-4 text-ember" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[12.5px] font-medium">{m.attachment.name}</span>
                  <span className="block text-[11px] text-fg-3">{m.attachment.size}</span>
                </span>
              </button>
            )}
          </div>
          <div className="mt-1 pl-1 font-mono text-[10.5px] text-fg-3">{m.time}</div>
        </div>
      </div>
    );
  }
  if (m.from === "ai") {
    return (
      <div className="flex items-end justify-end gap-2.5">
        <div className="max-w-[78%]">
          <div className="mb-1 flex items-center justify-end gap-1.5 text-[11px] text-fg-3">
            {m.confidence !== undefined && <span className={cn("k-num", m.confidence >= 85 ? "text-up" : m.confidence >= 70 ? "text-gold" : "text-warn")}>{m.confidence}% confident</span>}
            <span>·</span>
            <span className="font-medium text-gold">Claude · AI</span>
          </div>
          <div className="relative rounded-[18px] rounded-br-md border border-gold/25 bg-[linear-gradient(135deg,rgba(255,90,31,0.10),rgba(233,185,73,0.07))] px-4 py-2.5 text-[13.5px] leading-relaxed text-fg shadow-[inset_0_1px_0_rgba(233,185,73,0.18)]">
            {m.text}
            {m.cites && m.cites.length > 0 && (
              <div className="mt-2 flex flex-wrap gap-1.5 border-t border-gold/15 pt-2">
                {m.cites.map((c) => (
                  <button key={c} onClick={() => toast.info(`Knowledge article: ${c}`)} className="inline-flex items-center gap-1 rounded-full border border-line bg-surface/60 px-2 py-0.5 text-[10.5px] text-fg-2 hover:text-fg">
                    <BookOpen className="size-3 text-gold" />
                    {c}
                  </button>
                ))}
              </div>
            )}
          </div>
          <div className="mt-1 pr-1 text-right font-mono text-[10.5px] text-fg-3">{m.time}</div>
        </div>
        <AiSpark size={28} />
      </div>
    );
  }
  // agent
  return (
    <div className="flex items-end justify-end gap-2.5">
      <div className="max-w-[78%]">
        <div className="mb-1 text-right text-[11px] font-medium text-fg-2">{m.author}</div>
        <div className="rounded-[18px] rounded-br-md border border-ember/30 bg-ember/[0.14] px-4 py-2.5 text-[13.5px] leading-relaxed text-fg">{m.text}</div>
        <div className="mt-1 flex items-center justify-end gap-1 pr-1 font-mono text-[10.5px] text-fg-3">
          {m.time} <CheckCheck className="size-3 text-info" />
        </div>
      </div>
      <Avatar src={m.authorPhoto} name={m.author ?? "Agent"} size={28} />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Canned reply picker                                                 */
/* ------------------------------------------------------------------ */

function CannedPicker({ onPick }: { onPick: (body: string) => void }) {
  const [q, setQ] = React.useState("");
  const list = SUP_CANNED.filter((c) => !q || `${c.shortcut} ${c.title} ${c.body}`.toLowerCase().includes(q.toLowerCase()));
  return (
    <Popover
      align="start"
      width={400}
      trigger={
        <button className="flex h-8 items-center gap-1.5 rounded-full border border-line bg-surface-2 px-3 text-[12px] text-fg-2 hover:bg-surface-3 hover:text-fg">
          <MessageSquareText className="size-3.5" /> Canned replies
          <ChevronDown className="size-3" />
        </button>
      }
    >
      <div className="border-b border-line p-3">
        <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3">
          <Search className="size-3.5 text-fg-3" />
          <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search or type /shortcut…" className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-3" />
        </div>
      </div>
      <div className="max-h-[320px] space-y-1 overflow-y-auto p-2">
        {list.map((c) => (
          <button
            key={c.id}
            onClick={() => {
              onPick(c.body);
              closeFloating();
            }}
            className="block w-full rounded-xl px-3 py-2.5 text-left hover:bg-surface-3"
          >
            <span className="flex items-center gap-2">
              <span className="font-mono text-[11.5px] text-ember">{c.shortcut}</span>
              <span className="truncate text-[12.5px] font-medium text-fg">{c.title}</span>
              <span className="ml-auto shrink-0 rounded bg-surface-3 px-1.5 text-[10px] font-medium text-fg-3">{c.language}</span>
            </span>
            <span className="mt-1 line-clamp-2 block text-[12px] leading-snug text-fg-3" dir={c.language === "AR" ? "rtl" : undefined}>
              {c.body}
            </span>
          </button>
        ))}
        {list.length === 0 && <div className="py-8 text-center text-[12.5px] text-fg-3">No canned reply matches “{q}”.</div>}
      </div>
      <div className="flex items-center justify-between border-t border-line px-4 py-2.5 text-[11px] text-fg-3">
        <span>
          Variables auto-fill from client context
        </span>
        <a href="/support/canned" className="text-fg-2 hover:text-ember">
          Manage →
        </a>
      </div>
    </Popover>
  );
}

/* ------------------------------------------------------------------ */
/* Thread                                                              */
/* ------------------------------------------------------------------ */

const SUGGESTIONS: Record<string, string> = {
  "CV-20931": "Thanks Lucas, that helps. Our payments desk has confirmed the review is a routine check on newly whitelisted addresses. I've asked them to prioritise it and expect release within the next 60 minutes — I'll message you here as soon as it's sent.",
  "CV-20929": "Hi Daniel, I've reviewed the execution log for position #5829114. During the PCE release at 15:30 all our liquidity providers widened XAUUSD spreads for 11 seconds and your stop out was filled within that range. I understand this is frustrating — as a one-time goodwill gesture I can offer a $100 trading credit.",
  "CV-20926": "Hi Sofia, thanks for your patience. Because your bill shows your married name, could you upload a marriage certificate or a bank statement in the name 'Sofia Rossi'? I'll approve it as soon as it arrives.",
  "CV-20923": "Merhaba Hassan, güvenliğiniz için hesabınızı açmadan önce kısa bir selfie doğrulaması gerekiyor. Size şimdi bir bağlantı gönderiyorum; tamamlandığında hesabınızı hemen açacağım.",
};

export function ChatThread({
  conv,
  me,
  onSend,
  onTakeOver,
  onResolve,
  onAssign,
  onOpenContext,
}: {
  conv: SupConversation;
  me: { name: string; photo: string };
  onSend: (text: string, note: boolean) => void;
  onTakeOver: () => void;
  onResolve: () => void;
  onAssign: (name: string, photo: string) => void;
  onOpenContext: () => void;
}) {
  const [mode, setMode] = React.useState<"reply" | "note">("reply");
  const [text, setText] = React.useState("");
  const [typing, setTyping] = React.useState(false);
  const scroller = React.useRef<HTMLDivElement>(null);
  const ta = React.useRef<HTMLTextAreaElement>(null);
  const note = mode === "note";
  const firstName = conv.client.name.split(" ")[0]!;
  const vars: Record<string, string> = {
    first_name: firstName,
    login: conv.context.accounts[0]?.login ?? "",
    agent_name: me.name,
    tenant_name: conv.context.tenant,
    amount: conv.context.lastWithdrawal ? `${conv.context.lastWithdrawal.amount.toLocaleString("en-US", { minimumFractionDigits: 2 })} USDT` : "{{amount}}",
    withdrawal_id: conv.context.lastWithdrawal?.id ?? "{{withdrawal_id}}",
    eta: "4 business hours",
    kb_link: "kalkstrade.com/help/news-spreads",
    leverage: "1:500",
    deposit_id: "TX904412",
  };

  React.useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [conv.messages.length, conv.id, typing]);

  React.useEffect(() => {
    setText("");
    setMode("reply");
  }, [conv.id]);

  const send = () => {
    const t = text.trim();
    if (!t) {
      toast.error("Type a message first");
      return;
    }
    onSend(t, note);
    setText("");
    if (!note) {
      setTyping(true);
      setTimeout(() => setTyping(false), 2600);
    }
  };

  const insert = (s: string) => {
    setText((prev) => (prev ? `${prev} ${fillVars(s, vars)}` : fillVars(s, vars)));
    requestAnimationFrame(() => ta.current?.focus());
  };

  const suggestion = SUGGESTIONS[conv.id];
  const human = conv.status === "agent" || conv.status === "needs_agent";

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/* Header */}
      <div className="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3.5">
        <Avatar src={conv.client.photo} name={conv.client.name} size={40} verified={conv.context.kyc === "verified"} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="truncate text-[15px] font-medium">{conv.client.name}</span>
            <Flag country={conv.client.country} className="size-3.5" />
            <span className="font-mono text-[11px] text-fg-3">{conv.id}</span>
          </div>
          <div className="mt-0.5 flex items-center gap-2 text-[12px] text-fg-3">
            <span className="truncate">{conv.subject}</span>
          </div>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <ConvStatusChip status={conv.status} size="md" />
          <Chip size="md">
            <ChannelBadge channel={conv.channel} withLabel className="text-fg-2" />
          </Chip>
          {human && conv.slaSeconds !== 0 && (
            <Tooltip content="First-response SLA remaining">
              <span>
                <SlaCountdown seconds={conv.slaSeconds} className="px-2 py-0.5 text-[11.5px]" />
              </span>
            </Tooltip>
          )}
          {(conv.status === "ai" || conv.status === "needs_agent") && (
            <Button size="sm" variant="ember" onClick={onTakeOver}>
              <UserPlus /> Take over
            </Button>
          )}
          {conv.status !== "resolved" && (
            <Button size="sm" variant="up-outline" onClick={onResolve}>
              <Check /> Resolve
            </Button>
          )}
          <Menu
            width={240}
            trigger={
              <IconButton size="sm" aria-label="Assign">
                <ArrowRightLeft />
              </IconButton>
            }
            header={<div className="text-[11px] uppercase tracking-wider text-fg-3">Assign to agent</div>}
            items={SUP_AGENTS.map((a) => ({
              label: a.person.name,
              icon: <Avatar src={a.person.photo} name={a.person.name} size={20} online={a.status === "online"} />,
              hint: <span className="text-[10.5px]">{a.open} open</span>,
              onSelect: () => onAssign(a.person.name, a.person.photo),
            }))}
          />
          <Menu
            trigger={
              <IconButton size="sm" aria-label="More">
                <MoreHorizontal />
              </IconButton>
            }
            items={[
              { label: "Snooze 1 hour", icon: <Clock />, onSelect: () => toast.success("Snoozed until 15:32 GMT+3") },
              { label: "Escalate to Finance desk", icon: <ArrowRightLeft />, onSelect: () => toast.success("Escalated to Finance desk", { description: `${conv.id} · priority ${conv.priority}` }) },
              { label: "Export transcript", icon: <FileText />, onSelect: () => toast.success(`${conv.id}-transcript.pdf exported`) },
              "sep",
              { label: "Mark as spam", danger: true, onSelect: () => toast.error("Marked as spam", { description: "Sender muted for 30 days" }) },
            ]}
          />
          <IconButton size="sm" className="lg:inline-flex xl:hidden hidden" onClick={onOpenContext} aria-label="Client context">
            <PanelRight />
          </IconButton>
        </div>
      </div>

      {/* Messages */}
      <div ref={scroller} className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-5">
        <div className="flex justify-center">
          <span className="rounded-full border border-line bg-surface-2 px-3 py-1 text-[11px] text-fg-3">Today · 24 Sep 2026 · GMT+3</span>
        </div>
        <AnimatePresence initial={false}>
          {conv.messages.map((m) => (
            <motion.div key={m.id} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.25 }}>
              <MessageView m={m} conv={conv} />
            </motion.div>
          ))}
          {typing && (
            <motion.div key="typing" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }} className="flex items-end gap-2.5">
              <Avatar src={conv.client.photo} name={conv.client.name} size={28} />
              <span className="flex items-center gap-1 rounded-[18px] rounded-bl-md border border-line bg-surface-2 px-4 py-3">
                {[0, 1, 2].map((i) => (
                  <motion.span key={i} className="size-1.5 rounded-full bg-fg-3" animate={{ opacity: [0.3, 1, 0.3] }} transition={{ duration: 1, repeat: Infinity, delay: i * 0.18 }} />
                ))}
              </span>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {/* AI suggestion */}
      {human && suggestion && (
        <div className="mx-4 mb-2 flex items-start gap-3 rounded-[14px] border border-gold/20 bg-[linear-gradient(135deg,rgba(255,90,31,0.07),rgba(233,185,73,0.05))] px-3.5 py-2.5">
          <AiSpark size={22} className="mt-0.5" />
          <div className="min-w-0 flex-1">
            <div className="text-[11px] font-medium text-gold">Claude suggests a reply · 88% match to policy</div>
            <p className="mt-0.5 line-clamp-2 text-[12.5px] leading-snug text-fg-2">{suggestion}</p>
          </div>
          <div className="flex shrink-0 gap-1.5">
            <Button size="xs" variant="surface" onClick={() => toast.success("New suggestion generated", { description: "Tone: empathetic · cites 2 articles" })}>
              <RefreshCw className="!size-3" />
            </Button>
            <Button size="xs" variant="gold" onClick={() => { setMode("reply"); setText(suggestion); }}>
              Use
            </Button>
          </div>
        </div>
      )}

      {/* Composer */}
      <div className="px-4 pb-4">
        <div className={cn("rounded-[18px] border transition-colors", note ? "border-warn/40 bg-warn-soft" : "border-line bg-surface-2 focus-within:border-ember/40")}>
          <div className="flex items-center justify-between gap-2 px-3 pt-2.5">
            <Segmented
              size="xs"
              value={mode}
              onChange={setMode}
              options={[
                { value: "reply", label: <>Reply · <ChannelBadge channel={conv.channel} /></> },
                { value: "note", label: <><Lock className="size-3" /> Internal note</> },
              ]}
            />
            {note ? <span className="text-[11px] text-warn">Only staff can see notes</span> : <span className="hidden text-[11px] text-fg-3 sm:inline">Replying as {me.name}</span>}
          </div>
          <textarea
            ref={ta}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                send();
              }
            }}
            rows={3}
            placeholder={note ? "Add a private note for the team… use @ to mention" : `Reply to ${firstName}…  (Enter to send, Shift+Enter for new line)`}
            className="block w-full resize-none bg-transparent px-4 py-2.5 text-[13.5px] leading-relaxed text-fg outline-none placeholder:text-fg-3"
          />
          <div className="flex flex-wrap items-center gap-1.5 px-3 pb-2.5">
            <CannedPicker onPick={insert} />
            <Tooltip content="Attach file (max 20 MB)">
              <IconButton size="sm" onClick={() => toast.info("Choose a file to attach", { description: "PDF, PNG, JPG up to 20 MB" })} aria-label="Attach">
                <Paperclip />
              </IconButton>
            </Tooltip>
            <Tooltip content="Emoji & reactions">
              <IconButton size="sm" onClick={() => toast.info("Reactions are off for this channel")} aria-label="Emoji">
                <Smile />
              </IconButton>
            </Tooltip>
            <Tooltip content="Rewrite with Claude">
              <IconButton
                size="sm"
                onClick={() => {
                  if (!text.trim()) {
                    toast.info("Write a draft first — Claude will polish the tone");
                    return;
                  }
                  setText((t) => `${t.trim().replace(/\s+$/, "")}${/[.!?]$/.test(t.trim()) ? "" : "."} Please let me know if there's anything else I can help with.`);
                  toast.success("Draft rewritten", { description: "Tone: professional · empathetic" });
                }}
                aria-label="AI rewrite"
              >
                <Wand2 />
              </IconButton>
            </Tooltip>
            <span className="ml-auto hidden items-center gap-1 text-[11px] text-fg-3 md:flex">
              <Kbd>/</Kbd> shortcuts
            </span>
            <Button size="sm" variant={note ? "gold" : "ember"} onClick={send}>
              {note ? "Add note" : "Send"} <SendHorizontal />
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

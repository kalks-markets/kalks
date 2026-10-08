"use client";

import * as React from "react";
import { toast } from "sonner";
import { ChevronRight, Copy, History, Mail, MessageSquareText, Star } from "lucide-react";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, PageHeader, Reveal } from "@/components/kit";
import { useSession } from "@/components/session";
import { realtime } from "@/lib/realtime";
import { SUPPORT_EMAIL } from "@/lib/live";
import type { MessageKey } from "@kalks/i18n";
import { Trans, tr, useFormat, useT } from "@kalks/i18n/react";
import { LiveChat, MessageRow, errMsg, type Conversation, type Message } from "@/components/support/live-chat";
import { useModules } from "@/components/tenant-config";

function copy(text: string, what: string) {
  navigator.clipboard?.writeText(text).then(
    () => toast.success(tr("support.toast.copied", { what })),
    () => toast.error(tr("support.toast.copyFailed")),
  );
}

// label = translation key
const STATUS: Record<Conversation["status"], { label: MessageKey; tone: "ember" | "warn" | "up" | "neutral" }> = {
  bot: { label: "support.status.bot", tone: "ember" },
  waiting: { label: "support.status.waiting", tone: "warn" },
  assigned: { label: "support.status.assigned", tone: "up" },
  resolved: { label: "support.status.resolved", tone: "neutral" },
};

/** Past conversations with a read-only transcript. */
function HistoryCard() {
  const t = useT();
  const f = useFormat();
  const day = (iso: string) => f.date(iso);
  const me = useSession();
  const [items, setItems] = React.useState<Conversation[] | null>(null);
  const [open, setOpen] = React.useState<{ c: Conversation; msgs: Message[] } | null>(null);
  const load = React.useCallback(async () => {
    const r = await fetch("/api/support/conversations", { cache: "no-store" }).catch(() => null);
    const d = r ? ((await r.json().catch(() => ({}))) as { items?: Conversation[] }) : {};
    setItems(d.items ?? []);
  }, []);
  React.useEffect(() => {
    void load();
    let t: ReturnType<typeof setTimeout> | null = null;
    // refresh when a conversation opens, changes status or is rated
    const off = realtime().subscribe((f) => {
      if (f.type !== "conversation" && f.type !== "reconnected") return;
      if (t) clearTimeout(t);
      t = setTimeout(() => void load(), 400);
    });
    return () => {
      off();
      if (t) clearTimeout(t);
    };
  }, [load]);
  const view = async (c: Conversation) => {
    const r = await fetch(`/api/support/conversations/${c.id}`, { cache: "no-store" });
    const d = (await r.json().catch(() => ({}))) as { conversation?: Conversation; messages?: Message[] };
    if (!r.ok || !d.conversation) return toast.error(t("support.toast.openFailed"), { description: errMsg(d) });
    setOpen({ c: d.conversation, msgs: d.messages ?? [] });
  };
  return (
    <Card>
      <CardHeader title={t("support.history.title")} subtitle={t("support.history.subtitle")} />
      <div className="space-y-1 px-4 pb-5 pt-3 sm:px-6">
        {items === null && <div className="py-6 text-center text-[13px] text-fg-3">{t("common.loading")}</div>}
        {items?.length === 0 && <EmptyState illustration="robot" title={t("support.history.emptyTitle")} text={t("support.history.emptyText")} className="py-6" />}
        {items?.map((c) => (
          <button key={c.id} onClick={() => void view(c)} className="group flex w-full items-center gap-3 rounded-xl px-2 py-2.5 text-start transition-colors hover:bg-surface-2">
            <span className="grid size-9 shrink-0 place-items-center rounded-full bg-surface-3 text-fg-2">
              <MessageSquareText className="size-4" />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-[13.5px] text-fg">{c.subject || t("support.conversation")}</span>
              <span className="mt-0.5 flex items-center gap-2 text-[11.5px] text-fg-3">
                {day(c.createdAt)}
                {c.csat && (
                  <span className="flex items-center gap-0.5">
                    · {c.csat.rating} <Star className="size-3 fill-current text-gold" />
                  </span>
                )}
              </span>
            </span>
            <Chip size="sm" tone={STATUS[c.status].tone}>
              {t(STATUS[c.status].label)}
            </Chip>
            <ChevronRight className="size-4 text-fg-3 rtl:-scale-x-100" />
          </button>
        ))}
      </div>
      <Dialog open={!!open} onOpenChange={(o) => !o && setOpen(null)} title={open?.c.subject || t("support.conversation")} description={open ? `${day(open.c.createdAt)} · ${t(STATUS[open.c.status].label)}` : undefined} width={620}>
        <div className="max-h-[60vh] space-y-4 overflow-y-auto pe-1">
          {open?.msgs.map((m) => (
            <MessageRow key={m.id} m={m} botName="Kalks AI" meName={me.name} />
          ))}
        </div>
      </Dialog>
    </Card>
  );
}

/** Live builds: AI chat that hands over to our team, conversation history and the email channel. */
export function LiveSupport() {
  const t = useT();
  const me = useSession();
  // the broker switched the support chat off: the page keeps the email channel only
  const chat = useModules()("support_chat");
  const id = `KL-${String(me.id).padStart(6, "0")}`;
  const mailto = `mailto:${SUPPORT_EMAIL}?subject=${encodeURIComponent(`Support request · ${id}`)}`;
  return (
    <div className="pb-16">
      <PageHeader title={t("support.page.title")} subtitle={t("support.page.subtitle")} />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        {chat && (
          <Reveal className="xl:col-span-7">
            <LiveChat />
          </Reveal>
        )}

        <div className={chat ? "space-y-4 xl:col-span-5" : "space-y-4 xl:col-span-7"}>
          {chat && (
            <Reveal delay={0.05}>
              <HistoryCard />
            </Reveal>
          )}
          <Reveal delay={0.1}>
            <Card className="p-6">
              <div className="flex items-start gap-4">
                <span className="grid size-11 shrink-0 place-items-center rounded-full border border-ember/30 bg-ember-soft text-ember">
                  <Mail className="size-[18px]" />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="text-[13px] text-fg-3">{t("support.email.prefer")}</div>
                  <div className="mt-0.5 break-all font-mono text-[15px] font-medium" dir="ltr">{SUPPORT_EMAIL}</div>
                  <p className="mt-1.5 text-[12.5px] leading-relaxed text-fg-2">
                    <Trans
                      k="support.email.writeFrom"
                      vars={{ email: me.email, id }}
                      tags={{
                        email: (c) => <span className="text-fg">{c}</span>,
                        id: (c) => (
                          <span className="k-num font-mono text-fg" dir="ltr">
                            {c}
                          </span>
                        ),
                      }}
                    />
                  </p>
                  <div className="mt-4 flex flex-wrap gap-2">
                    <a href={mailto}>
                      <Button size="sm" variant="surface">
                        <Mail /> {t("support.email.write")}
                      </Button>
                    </a>
                    <Button size="sm" variant="ghost" onClick={() => copy(id, t("support.clientId"))}>
                      <Copy /> {t("support.email.copyId")}
                    </Button>
                  </div>
                </div>
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.15}>
            <Card className="flex items-center gap-3 px-6 py-4 text-[12.5px] text-fg-2">
              <History className="size-4 shrink-0 text-fg-3" />
              {t("support.notice")}
            </Card>
          </Reveal>
        </div>
      </div>
    </div>
  );
}

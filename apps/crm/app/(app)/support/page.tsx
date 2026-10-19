"use client";

import * as React from "react";
import { toast } from "sonner";
import { ArrowUpRight, ChevronRight, Clock3, Eye, Mail, MessageCircle, Phone, Search } from "lucide-react";
import { Card, CardHeader, Chip, Dialog, Icon3D, Input, PageHeader, Reveal, Button, cn } from "@/components/kit";
import { HELP_CATEGORIES, POPULAR_ARTICLES, SUPPORT_AGENT } from "@kalks/mock/support-extra";
import { ChatPanel } from "@/components/support/chat-panel";
import { LiveSupport } from "@/components/support/live-support";
import { IS_DEMO } from "@kalks/mock";

const CONTACTS = [
  { key: "email", icon: <Mail />, title: "Email", value: "support@kalkstrade.com", hours: "Reply within 2 hours · 24/7", action: "Copy", copy: "support@kalkstrade.com" },
  { key: "wa", icon: <MessageCircle />, title: "WhatsApp", value: "+971 4 568 2210", hours: "Mon–Fri 08:00–22:00 GMT+3", action: "Open", copy: "" },
  { key: "phone", icon: <Phone />, title: "Phone", value: "+44 20 3808 4412", hours: "Mon–Fri 09:00–21:00 GMT+3", action: "Call", copy: "" },
];

function DemoSupport() {
  const [q, setQ] = React.useState("");
  const [article, setArticle] = React.useState<(typeof POPULAR_ARTICLES)[number] | null>(null);
  const qq = q.trim().toLowerCase();
  const articles = POPULAR_ARTICLES.filter((a) => !qq || a.title.toLowerCase().includes(qq) || a.category.toLowerCase().includes(qq));

  return (
    <div className="pb-24">
      <PageHeader
        title="Support centre"
        subtitle="Chat with Kalks AI for instant answers. A human agent can join any time, 24/7."
        actions={
          <div className="flex items-center gap-3 rounded-full border border-line bg-surface-2 py-1.5 pl-1.5 pr-4">
            <div className="flex -space-x-2">
              {[SUPPORT_AGENT.photo, "/assets/people/men-11.jpg", "/assets/people/women-22.jpg"].map((p) => (
                // eslint-disable-next-line @next/next/no-img-element
                <img key={p} src={p} alt="" className="size-7 rounded-full object-cover ring-2 ring-surface-2" />
              ))}
            </div>
            <div className="text-[12px] leading-tight">
              <div className="flex items-center gap-1.5 font-medium">
                <span className="size-1.5 animate-pulse rounded-full bg-up" /> 14 agents online
              </div>
              <div className="text-fg-3">Avg. wait under 1 min</div>
            </div>
          </div>
        }
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal className="h-full xl:col-span-7">
          <ChatPanel />
        </Reveal>

        <div className="space-y-4 xl:col-span-5">
          <Reveal delay={0.05}>
            <Card hot className="relative overflow-hidden px-6 py-5">
              <div className="relative">
                <div className="text-[16px] font-medium">How can we help?</div>
                <Input className="mt-3 bg-surface/70" leading={<Search />} placeholder="Search help articles…" value={q} onChange={(e) => setQ(e.target.value)} />
                <div className="mt-2.5 flex flex-wrap gap-1.5 text-[11.5px]">
                  {["withdrawal", "leverage", "swap", "KYC"].map((t) => (
                    <button key={t} onClick={() => setQ(t)} className="rounded-full border border-line bg-surface/50 px-2.5 py-0.5 text-fg-2 hover:text-fg">
                      {t}
                    </button>
                  ))}
                </div>
              </div>
            </Card>
          </Reveal>

          <Reveal delay={0.1}>
            <Card>
              <CardHeader title="Help categories" subtitle="112 articles, updated weekly" />
              <div className="grid grid-cols-2 gap-2.5 px-4 pb-5 pt-4 sm:grid-cols-3 sm:px-6">
                {HELP_CATEGORIES.map((c) => (
                  <button key={c.key} onClick={() => setQ(c.title)} className="k-row group flex flex-col items-start px-3.5 py-3.5 text-left transition-colors hover:border-[var(--k-border-top)] hover:bg-surface-3/60">
                    <Icon3D name={c.icon} size={40} />
                    <div className="mt-2.5 text-[13.5px] font-medium">{c.title}</div>
                    <div className="mt-0.5 line-clamp-2 text-[11px] leading-snug text-fg-3">{c.text}</div>
                    <div className="k-num mt-2 text-[11px] text-fg-3">{c.articles} articles</div>
                  </button>
                ))}
              </div>
            </Card>
          </Reveal>

          <Reveal delay={0.15}>
            <Card>
              <CardHeader title={qq ? `Results for “${q}”` : "Popular articles"} action={qq ? <Button size="xs" variant="ghost" onClick={() => setQ("")}>Clear</Button> : undefined} />
              <div className="space-y-1 px-4 pb-5 pt-3 sm:px-6">
                {articles.length === 0 && <div className="py-6 text-center text-[13px] text-fg-3">No articles found. Ask Kalks AI in the chat instead.</div>}
                {articles.map((a) => (
                  <button key={a.id} onClick={() => setArticle(a)} className="group flex w-full items-center gap-3 rounded-xl px-2 py-2.5 text-left transition-colors hover:bg-surface-2">
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13.5px] text-fg group-hover:text-ember">{a.title}</div>
                      <div className="mt-0.5 flex items-center gap-2 text-[11px] text-fg-3">
                        <span>{a.category}</span>·
                        <span className="flex items-center gap-1">
                          <Eye className="size-3" /> {a.views}
                        </span>
                      </div>
                    </div>
                    <ChevronRight className="size-4 text-fg-3 transition-transform group-hover:translate-x-0.5" />
                  </button>
                ))}
              </div>
            </Card>
          </Reveal>
        </div>
      </div>

      <Reveal delay={0.1} className="mt-4">
        <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
          {CONTACTS.map((c) => (
            <Card key={c.key} className="flex items-center gap-4 px-5 py-4">
              <span className={cn("grid size-11 shrink-0 place-items-center rounded-full border [&_svg]:size-[18px]", c.key === "wa" ? "border-up/30 bg-up-soft text-up" : "border-ember/30 bg-ember-soft text-ember")}>{c.icon}</span>
              <div className="min-w-0 flex-1">
                <div className="text-[12px] text-fg-3">{c.title}</div>
                <div className="k-num truncate font-mono text-[14px] font-medium">{c.value}</div>
                <div className="mt-0.5 flex items-center gap-1 text-[11px] text-fg-3">
                  <Clock3 className="size-3" /> {c.hours}
                </div>
              </div>
              <Button
                size="sm"
                variant="surface"
                onClick={() => {
                  if (c.copy) navigator.clipboard?.writeText(c.copy).catch(() => {});
                  toast.success(c.key === "email" ? "Email copied" : c.key === "wa" ? "Opening WhatsApp…" : "Calling +44 20 3808 4412…");
                }}
              >
                {c.action}
              </Button>
            </Card>
          ))}
        </div>
      </Reveal>

      <Dialog
        open={!!article}
        onOpenChange={(o) => !o && setArticle(null)}
        title={article?.title ?? ""}
        description={article ? `${article.category} · ${article.views} views · updated 3 days ago` : undefined}
        width={620}
        footer={
          <>
            <Button variant="ghost" onClick={() => toast.success("Thanks! Glad it helped.")}>
              Helpful
            </Button>
            <Button variant="surface" onClick={() => toast.info("We'll improve this article")}>
              Not helpful
            </Button>
          </>
        }
      >
        {article && (
          <div className="space-y-3 text-[14px] leading-relaxed text-fg-2">
            <Chip tone="ember">{article.category}</Chip>
            <p>
              Most requests are handled automatically in minutes. USDT (TRC20) transactions need 20 network confirmations, which usually takes around one minute. All times in the Client Area use server time (GMT+3).
            </p>
            <p>
              You can follow every step from <span className="text-fg">Wallet → History</span>, where each transaction shows its status, network hash and confirmations. If something looks stuck for more than 30 minutes, start a chat and Kalks AI will check it for you instantly.
            </p>
            <div className="k-row flex items-center justify-between px-4 py-3 text-[13px]">
              <span>Still need help?</span>
              <button className="flex items-center gap-1 font-medium text-ember" onClick={() => setArticle(null)}>
                Ask in chat <ArrowUpRight className="size-3.5" />
              </button>
            </div>
          </div>
        )}
      </Dialog>
    </div>
  );
}

/** Demo builds: AI chat and help centre on sample data. Live builds: the real email channel. */
export default function SupportPage() {
  return IS_DEMO ? <DemoSupport /> : <LiveSupport />;
}

"use client";

import * as React from "react";
import { Braces, Clock, Copy, Languages, MessageSquareText, MoreHorizontal, Pencil, Plus, Search, Trash2, TrendingUp, Users } from "lucide-react";
import { toast } from "sonner";
import {
  Avatar,
  Button,
  Card,
  CardHeader,
  Chip,
  Dialog,
  DialogClose,
  Field,
  IconButton,
  Input,
  KpiCard,
  Menu,
  PageHeader,
  Reveal,
  Segmented,
  Toggle,
  cn,
  formatNumber,
} from "@kalks/ui";
import { PEOPLE } from "@kalks/mock";
import { SUP_CANNED, SUP_VARIABLES, type SupCanned } from "@kalks/mock/admin-growth-support";
import { VarText, fillVars } from "@/components/support/shared";
import { IS_DEMO as IS_DEMO_MODE } from "@kalks/mock/mode";
import { LiveCanned } from "@/components/support-live/canned";

const CATS = ["All", "Withdrawals", "Deposits", "KYC", "Trading", "Accounts", "Partners", "Security", "General", "Prop"] as const;
const LANGS = ["EN", "ES", "AR", "PT", "HI", "VI"] as const;
const SAMPLE: Record<string, string> = {
  first_name: "Lucas",
  login: "80412337",
  amount: "2,500.00 USDT",
  withdrawal_id: "WD-904375",
  eta: "4 business hours",
  agent_name: "Priya Nair",
  kb_link: "kalkstrade.com/help/news-spreads",
  tenant_name: "Kalks Markets",
  deposit_id: "TX904412",
  leverage: "1:500",
};
const photoOf = (name: string) => PEOPLE.find((p) => p.name === name)?.photo;

function Editor({ open, onOpenChange, initial, onSave }: { open: boolean; onOpenChange: (o: boolean) => void; initial: SupCanned | null; onSave: (c: SupCanned) => void }) {
  const blank: SupCanned = { id: `cr${Date.now() % 10000}`, shortcut: "/", title: "", body: "", category: "General", language: "EN", usage: 0, updatedBy: "Priya Nair", updated: "24 Sep 2026", shared: true };
  const [d, setD] = React.useState<SupCanned>(initial ?? blank);
  const ta = React.useRef<HTMLTextAreaElement>(null);
  React.useEffect(() => {
    if (open) setD(initial ?? blank);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, initial]);

  const insertVar = (v: string) => {
    const el = ta.current;
    const token = `{{${v}}}`;
    if (!el) return setD((x) => ({ ...x, body: x.body + token }));
    const s = el.selectionStart ?? d.body.length;
    const e = el.selectionEnd ?? d.body.length;
    const body = d.body.slice(0, s) + token + d.body.slice(e);
    setD({ ...d, body });
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(s + token.length, s + token.length);
    });
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={720}
      title={initial ? "Edit canned reply" : "New canned reply"}
      description="Agents insert it with its /shortcut. Variables auto-fill from the client's context."
      footer={
        <>
          <DialogClose asChild>
            <Button variant="ghost" size="sm">
              Cancel
            </Button>
          </DialogClose>
          <Button
            variant="ember"
            size="sm"
            onClick={() => {
              if (!d.title.trim() || d.shortcut.length < 3 || !d.body.trim()) return toast.error("Shortcut, title and body are required");
              onSave({ ...d, shortcut: d.shortcut.startsWith("/") ? d.shortcut : `/${d.shortcut}`, updated: "24 Sep 2026", updatedBy: "Priya Nair" });
              onOpenChange(false);
            }}
          >
            Save reply
          </Button>
        </>
      }
    >
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <Field label="Shortcut" hint="lowercase, dashes">
          <Input value={d.shortcut} onChange={(e) => setD({ ...d, shortcut: e.target.value.replace(/\s+/g, "-").toLowerCase() })} inputClassName="font-mono text-ember" />
        </Field>
        <Field label="Title">
          <Input value={d.title} onChange={(e) => setD({ ...d, title: e.target.value })} placeholder="Withdrawal in manual review" />
        </Field>
      </div>
      <div className="mt-4 grid grid-cols-1 gap-4 sm:grid-cols-2">
        <div>
          <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Language</div>
          <Segmented size="xs" value={d.language as (typeof LANGS)[number]} onChange={(v) => setD({ ...d, language: v })} options={LANGS} />
        </div>
        <div>
          <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Category</div>
          <div className="flex flex-wrap gap-1">
            {CATS.slice(1, 7).map((c) => (
              <button key={c} onClick={() => setD({ ...d, category: c })} className={cn("rounded-full border px-2 py-0.5 text-[11.5px]", d.category === c ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2")}>
                {c}
              </button>
            ))}
          </div>
        </div>
      </div>
      <div className="mt-4">
        <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
          Body <span className="font-normal text-fg-3">Click a variable to insert at the cursor</span>
        </div>
        <div className="mb-2 flex flex-wrap gap-1.5">
          {SUP_VARIABLES.map((v) => (
            <button key={v} onClick={() => insertVar(v)} className="inline-flex items-center gap-1 rounded-md border border-ember/25 bg-ember-soft px-1.5 py-0.5 font-mono text-[11px] text-ember hover:bg-ember/20">
              <Braces className="size-3" />
              {v}
            </button>
          ))}
        </div>
        <textarea
          ref={ta}
          value={d.body}
          onChange={(e) => setD({ ...d, body: e.target.value })}
          rows={5}
          dir={d.language === "AR" ? "rtl" : undefined}
          placeholder="Hi {{first_name}}, …"
          className="w-full resize-none rounded-[14px] border border-line bg-surface-2 px-3.5 py-3 text-[13.5px] leading-relaxed text-fg outline-none placeholder:text-fg-3 focus:border-ember/50 focus:ring-4 focus:ring-ember/10"
        />
      </div>
      <div className="mt-4 rounded-[14px] border border-line bg-surface-2/60 p-3.5">
        <div className="k-label mb-1.5 !text-fg-3">Preview with sample client</div>
        <p className="text-[13px] leading-relaxed text-fg-2" dir={d.language === "AR" ? "rtl" : undefined}>
          {d.body ? fillVars(d.body, SAMPLE) : <span className="text-fg-3">Start typing to preview…</span>}
        </p>
      </div>
      <div className="mt-4 flex items-center justify-between">
        <div className="text-[13px]">
          <div className="font-medium">Shared with all agents</div>
          <div className="text-[12px] text-fg-3">Off = only visible to you</div>
        </div>
        <Toggle checked={d.shared} onChange={(v) => setD({ ...d, shared: v })} label="Shared" />
      </div>
    </Dialog>
  );
}

function DemoCannedRepliesPage() {
  const [items, setItems] = React.useState<SupCanned[]>(SUP_CANNED);
  const [cat, setCat] = React.useState<string>("All");
  const [lang, setLang] = React.useState<string>("All");
  const [q, setQ] = React.useState("");
  const [open, setOpen] = React.useState(false);
  const [editing, setEditing] = React.useState<SupCanned | null>(null);

  const list = items.filter((c) => (cat === "All" || c.category === cat) && (lang === "All" || c.language === lang) && (!q || `${c.shortcut} ${c.title} ${c.body}`.toLowerCase().includes(q.toLowerCase())));
  const top = [...items].sort((a, b) => b.usage - a.usage).slice(0, 6);
  const max = top[0]!.usage;
  const totalUse = items.reduce((s, c) => s + c.usage, 0);

  const openEditor = (c: SupCanned | null) => {
    setEditing(c);
    setOpen(true);
  };

  return (
    <div className="pb-16">
      <PageHeader
        title="Canned replies"
        subtitle="Reusable answers with variables. Type / in any composer to insert."
        actions={
          <>
            <Button variant="surface" onClick={() => toast.success("Imported 4 replies from Zendesk macros", { description: "Variables mapped automatically" })}>
              Import macros
            </Button>
            <Button variant="ember" onClick={() => openEditor(null)}>
              <Plus /> New reply
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4">
        <KpiCard label="Replies" value={<span className="k-num">{items.length}</span>} icon={<MessageSquareText />} chip={`${items.filter((c) => c.shared).length} shared`} />
        <KpiCard label="Inserted · 30d" value={<span className="k-num">{formatNumber(totalUse, 0)}</span>} icon={<TrendingUp />} chip="+9.2% vs Aug" chipTone="up" delay={0.05} />
        <KpiCard label="Languages" value={<span className="k-num">{new Set(items.map((c) => c.language)).size}</span>} icon={<Languages />} chip="auto-translated on insert" delay={0.1} />
        <KpiCard label="Agent time saved" value={<span className="k-num">212h</span>} icon={<Clock />} hot illustration="hourglass_not_done" chip="≈ 1.1 FTE this month" chipTone="ember" delay={0.15} />
      </div>

      <div className="mt-4 grid grid-cols-1 gap-4 xl:grid-cols-12">
        <Reveal delay={0.05} className="xl:col-span-9">
          <Card className="h-full">
            <CardHeader
              title="Library"
              subtitle={`${list.length} of ${items.length} replies`}
              action={
                <div className="hidden h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5 sm:flex">
                  <Search className="size-3.5 text-fg-3" />
                  <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search shortcut or text…" className="w-44 bg-transparent text-[13px] outline-none placeholder:text-fg-3" />
                </div>
              }
            />
            <div className="mt-4 flex flex-wrap items-center gap-2 px-4 sm:px-6">
              <div className="-mx-1 max-w-full overflow-x-auto px-1 [scrollbar-width:none]">
                <Segmented size="xs" value={cat} onChange={setCat} options={CATS} />
              </div>
              <Segmented size="xs" value={lang} onChange={setLang} options={["All", "EN", "ES", "AR"]} className="ml-auto" />
            </div>
            <div className="mt-4 grid grid-cols-1 gap-3 px-4 pb-6 sm:px-6 md:grid-cols-2 2xl:grid-cols-3">
              {list.map((c) => (
                <div key={c.id} className="k-row group flex flex-col px-4 py-3.5 transition-colors hover:border-[var(--k-border-top)]">
                  <div className="flex items-start gap-2">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="rounded-md bg-ember-soft px-1.5 py-0.5 font-mono text-[11.5px] text-ember">{c.shortcut}</span>
                        <Chip size="sm">{c.language}</Chip>
                        {!c.shared && <Chip size="sm" tone="info">Private</Chip>}
                      </div>
                      <div className="mt-2 truncate text-[14px] font-medium">{c.title}</div>
                    </div>
                    <Menu
                      width={190}
                      trigger={
                        <IconButton size="sm" aria-label="More">
                          <MoreHorizontal />
                        </IconButton>
                      }
                      items={[
                        { label: "Edit", icon: <Pencil />, onSelect: () => openEditor(c) },
                        { label: "Duplicate", icon: <Copy />, onSelect: () => { setItems((xs) => [{ ...c, id: `${c.id}-copy`, shortcut: `${c.shortcut}-2`, usage: 0 }, ...xs]); toast.success(`Duplicated ${c.shortcut}`); } },
                        { label: "Translate to 21 languages", icon: <Languages />, onSelect: () => toast.success("Translations queued", { description: `${c.shortcut} · machine translation + review` }) },
                        "sep",
                        { label: "Delete", icon: <Trash2 />, danger: true, onSelect: () => { setItems((xs) => xs.filter((x) => x.id !== c.id)); toast.success(`${c.shortcut} deleted`); } },
                      ]}
                    />
                  </div>
                  <p className="mt-1.5 line-clamp-3 flex-1 text-[12.5px] leading-relaxed text-fg-2" dir={c.language === "AR" ? "rtl" : undefined}>
                    <VarText text={c.body} />
                  </p>
                  <div className="mt-3 flex items-center gap-2 border-t border-line pt-2.5 text-[11.5px] text-fg-3">
                    <Avatar src={photoOf(c.updatedBy)} name={c.updatedBy} size={18} />
                    <span className="truncate">{c.updatedBy} · {c.updated}</span>
                    <span className="ml-auto flex items-center gap-1">
                      <span className="k-num font-medium text-fg-2">{formatNumber(c.usage, 0)}</span> uses
                    </span>
                    <button onClick={() => openEditor(c)} className="text-fg-3 opacity-0 transition-opacity hover:text-ember group-hover:opacity-100" aria-label="Edit">
                      <Pencil className="size-3.5" />
                    </button>
                  </div>
                </div>
              ))}
              {list.length === 0 && <div className="col-span-full py-12 text-center text-[13px] text-fg-3">No replies match these filters.</div>}
            </div>
          </Card>
        </Reveal>

        <div className="flex flex-col gap-4 xl:col-span-3">
          <Reveal delay={0.1}>
            <Card>
              <CardHeader title="Most used · 30d" icon={<TrendingUp />} />
              <div className="mt-4 space-y-3 px-4 pb-5 sm:px-6">
                {top.map((c, i) => (
                  <div key={c.id}>
                    <div className="flex items-center justify-between text-[12px]">
                      <span className="truncate font-mono text-fg-2">{c.shortcut}</span>
                      <span className="k-num text-fg-3">{formatNumber(c.usage, 0)}</span>
                    </div>
                    <div className="mt-1 h-2 overflow-hidden rounded-full bg-surface-3">
                      <div className={cn("h-full rounded-full", i === 0 ? "bg-gradient-to-r from-ember to-[#ff8a3d] shadow-[0_0_12px_rgba(255,90,31,0.6)]" : "bg-fg-3/50")} style={{ width: `${(c.usage / max) * 100}%` }} />
                    </div>
                  </div>
                ))}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.15}>
            <Card>
              <CardHeader title="Variables" subtitle="Filled from client context" icon={<Braces />} />
              <div className="mt-3 divide-y divide-line px-4 pb-4 sm:px-6">
                {SUP_VARIABLES.map((v) => (
                  <button key={v} onClick={() => { navigator.clipboard?.writeText(`{{${v}}}`).catch(() => {}); toast.success(`{{${v}}} copied`); }} className="flex w-full items-center justify-between gap-2 py-2 text-left">
                    <span className="font-mono text-[11.5px] text-ember">{`{{${v}}}`}</span>
                    <span className="truncate text-[11.5px] text-fg-3">{SAMPLE[v]}</span>
                  </button>
                ))}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.2}>
            <Card>
              <CardHeader title="Suggested by Claude" subtitle="Agents typed these 20+ times this week" icon={<Users />} />
              <div className="mt-3 space-y-2 px-4 pb-5 sm:px-6">
                {[
                  ["/mt5-password", "Resetting the MT5 master password", 34],
                  ["/binance-pay", "Binance Pay is not supported yet", 27],
                ].map(([sc, t, n]) => (
                  <div key={sc as string} className="k-row px-3.5 py-2.5">
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-mono text-[11.5px] text-ember">{sc}</span>
                      <span className="k-num text-[11px] text-fg-3">{n}× typed</span>
                    </div>
                    <div className="mt-1 text-[12.5px] text-fg-2">{t}</div>
                    <Button
                      size="xs"
                      variant="surface"
                      className="mt-2 w-full"
                      onClick={() => openEditor({ id: `cr-${sc}`, shortcut: sc as string, title: t as string, body: "Hi {{first_name}}, ", category: "General", language: "EN", usage: 0, updatedBy: "Priya Nair", updated: "24 Sep 2026", shared: true })}
                    >
                      <Plus className="!size-3" /> Create reply
                    </Button>
                  </div>
                ))}
              </div>
            </Card>
          </Reveal>
        </div>
      </div>

      <Editor
        open={open}
        onOpenChange={setOpen}
        initial={editing}
        onSave={(c) => {
          setItems((xs) => (xs.some((x) => x.id === c.id) ? xs.map((x) => (x.id === c.id ? c : x)) : [c, ...xs]));
          toast.success(editing ? `${c.shortcut} updated` : `${c.shortcut} created`, { description: c.shared ? "Available to all agents" : "Private to you" });
        }}
      />
    </div>
  );
}

/** Demo builds: the mock showcase. Live builds: services/support. */
export default function CannedRepliesPage() {
  return IS_DEMO_MODE ? <DemoCannedRepliesPage /> : <LiveCanned />;
}

"use client";

import * as React from "react";
import { AlertTriangle, Braces, ChevronDown, Eye, History, Languages, Monitor, RotateCcw, Save, Search, Send, Smartphone, Sparkles, Wand2 } from "lucide-react";
import { toast } from "sonner";
import { Avatar, Button, Card, Chip, Dialog, DialogClose, Field, Flag, Icon3D, Input, PageHeader, Reveal, Segmented, Toggle, cn } from "@kalks/ui";
import { EML_LANGS, EML_TEMPLATES, EML_VARIABLES, type EmlContent, type EmlLang, type EmlTemplate, type EmlVersion } from "@kalks/mock/admin-email-templates";
import { PEOPLE } from "@kalks/mock";
import { EmailPreview } from "@/components/content/email-preview";

const SAMPLES = Object.fromEntries(EML_VARIABLES.map((v) => [v.key, v.sample]));
const GROUPS: EmlTemplate["group"][] = ["Account", "Security", "Funding", "Compliance", "Trading", "Partners"];
const fmtAt = (iso: string) => {
  const d = new Date(new Date(iso).getTime() + 3 * 3600_000);
  return `${String(d.getUTCDate()).padStart(2, "0")} ${["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][d.getUTCMonth()]}, ${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
};

type Drafts = Record<string, EmlContent>; // `${id}:${lang}`

export default function EmailTemplatesPage() {
  const [id, setId] = React.useState(EML_TEMPLATES[0]!.id);
  const [lang, setLang] = React.useState<EmlLang>("en");
  const [drafts, setDrafts] = React.useState<Drafts>({});
  const [saved, setSaved] = React.useState<Drafts>({});
  const [versions, setVersions] = React.useState<Record<string, EmlVersion[]>>(() => Object.fromEntries(EML_TEMPLATES.map((t) => [t.id, t.versions])));
  const [q, setQ] = React.useState("");
  const [raw, setRaw] = React.useState(false);
  const [device, setDevice] = React.useState<"desktop" | "mobile">("desktop");
  const [testOpen, setTestOpen] = React.useState(false);
  const [histOpen, setHistOpen] = React.useState(false);
  const [testEmail, setTestEmail] = React.useState("priya.nair@kalkstrade.com");
  const [langOpen, setLangOpen] = React.useState(false);

  const bodyRef = React.useRef<HTMLTextAreaElement>(null);
  const subjRef = React.useRef<HTMLInputElement>(null);
  const lastFocus = React.useRef<"subject" | "body">("body");

  const tpl = EML_TEMPLATES.find((t) => t.id === id)!;
  const langMeta = EML_LANGS.find((l) => l.code === lang)!;
  const k = `${id}:${lang}`;
  const translated = !!tpl.content[lang];
  const base: EmlContent = saved[k] ?? tpl.content[lang] ?? tpl.content.en;
  const cur: EmlContent = drafts[k] ?? base;
  const dirty = !!drafts[k] && JSON.stringify(drafts[k]) !== JSON.stringify(base);

  const update = (patch: Partial<EmlContent>) => setDrafts((d) => ({ ...d, [k]: { ...cur, ...patch } }));

  const used = Array.from(new Set(`${cur.subject} ${cur.body} ${cur.cta ?? ""}`.match(/\{\{\s*[a-z0-9_]+\s*\}\}/gi) ?? [])).map((s) => s.replace(/[{}\s]/g, ""));
  const unknown = used.filter((v) => !(v in SAMPLES));

  const insertVar = (key: string) => {
    const token = `{{${key}}}`;
    if (lastFocus.current === "subject" && subjRef.current) {
      const el = subjRef.current;
      const s = el.selectionStart ?? cur.subject.length;
      const e = el.selectionEnd ?? s;
      update({ subject: cur.subject.slice(0, s) + token + cur.subject.slice(e) });
      requestAnimationFrame(() => {
        el.focus();
        el.setSelectionRange(s + token.length, s + token.length);
      });
      return;
    }
    const el = bodyRef.current;
    const s = el?.selectionStart ?? cur.body.length;
    const e = el?.selectionEnd ?? s;
    update({ body: cur.body.slice(0, s) + token + cur.body.slice(e) });
    requestAnimationFrame(() => {
      el?.focus();
      el?.setSelectionRange(s + token.length, s + token.length);
    });
  };

  const saveVersion = () => {
    if (unknown.length) {
      toast.error("Unknown variables", { description: `${unknown.map((u) => `{{${u}}}`).join(", ")} can't be resolved. Fix before saving.` });
      return;
    }
    const list = versions[id]!;
    const v = Math.max(...list.map((x) => x.v)) + 1;
    setVersions((m) => ({ ...m, [id]: [{ v, by: PEOPLE[4]!, at: new Date().toISOString(), note: `${langMeta.name}: ${dirty ? "edited subject/body" : "re-published"}`, live: true }, ...list.map((x) => ({ ...x, live: false }))] }));
    setSaved((m) => ({ ...m, [k]: cur }));
    setDrafts((d) => {
      const n = { ...d };
      delete n[k];
      return n;
    });
    toast.success(`${tpl.name} · v${v} published`, { description: `${langMeta.name} · takes effect for the next “${tpl.trigger}” event` });
  };

  const list = EML_TEMPLATES.filter((t) => !q || `${t.name} ${t.trigger}`.toLowerCase().includes(q.toLowerCase()));

  return (
    <div className="pb-16">
      <PageHeader
        title="Email templates"
        subtitle="Transactional emails in every client language, with live branded preview and versioning."
        actions={
          <>
            <Button variant="surface" onClick={() => setHistOpen(true)}>
              <History /> Version history
            </Button>
            <Button variant="surface" onClick={() => setTestOpen(true)}>
              <Send /> Send test
            </Button>
            <Button variant="ember" shimmer onClick={saveVersion}>
              <Save /> Save version
            </Button>
          </>
        }
      />

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[260px_minmax(0,1fr)] 2xl:grid-cols-[264px_minmax(0,1fr)_minmax(0,500px)]">
        {/* ---------------- Template list ---------------- */}
        <Reveal className="lg:row-span-2 2xl:row-span-1">
          <Card className="flex h-full flex-col lg:sticky lg:top-24 lg:max-h-[calc(100vh-7rem)]">
            <div className="px-4 pt-4">
              <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3">
                <Search className="size-3.5 text-fg-3" />
                <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search templates…" className="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-fg-3" />
              </div>
            </div>
            <div className="mt-3 min-h-0 flex-1 overflow-y-auto px-2 pb-3">
              {GROUPS.map((g) => {
                const items = list.filter((t) => t.group === g);
                if (!items.length) return null;
                return (
                  <div key={g} className="mb-2">
                    <div className="k-label px-2.5 pb-1 pt-2">{g}</div>
                    {items.map((t) => {
                      const on = t.id === id;
                      const hasDraft = Object.keys(drafts).some((x) => x.startsWith(`${t.id}:`));
                      return (
                        <button
                          key={t.id}
                          type="button"
                          onClick={() => setId(t.id)}
                          className={cn("group relative flex w-full items-center gap-2.5 rounded-[12px] px-2.5 py-2 text-left transition-colors", on ? "bg-surface-3" : "hover:bg-surface-2")}
                        >
                          {on && <span className="absolute inset-y-2 left-0 w-[3px] rounded-full bg-ember" />}
                          <Icon3D name={t.icon} size={26} />
                          <span className="min-w-0 flex-1">
                            <span className={cn("block truncate text-[13px] font-medium", on ? "text-fg" : "text-fg-2")}>{t.name}</span>
                            <span className="block truncate font-mono text-[10.5px] text-fg-3">{t.trigger}</span>
                          </span>
                          {hasDraft ? <span className="size-1.5 rounded-full bg-ember" title="Unsaved changes" /> : t.status === "draft" ? <Chip size="sm">Draft</Chip> : null}
                        </button>
                      );
                    })}
                  </div>
                );
              })}
            </div>
            <div className="border-t border-line px-4 py-3 text-[11.5px] text-fg-3">
              {EML_TEMPLATES.length} templates · {EML_LANGS.length} languages
            </div>
          </Card>
        </Reveal>

        {/* ---------------- Editor ---------------- */}
        <Reveal delay={0.05} className="min-w-0">
          <Card className="h-full">
            <div className="flex flex-wrap items-start justify-between gap-3 px-6 pt-5">
              <div className="flex min-w-0 items-center gap-3">
                <Icon3D name={tpl.icon} size={40} />
                <div className="min-w-0">
                  <h3 className="truncate text-[17px] font-medium tracking-tight">{tpl.name}</h3>
                  <p className="truncate text-[12.5px] text-fg-3">
                    Trigger <span className="font-mono text-fg-2">{tpl.trigger}</span> · {tpl.sent30d.toLocaleString("en-US")} sent · {tpl.openRate}% open rate
                  </p>
                </div>
              </div>
              {/* Language select */}
              <div className="relative">
                <button type="button" onClick={() => setLangOpen((o) => !o)} className="flex h-10 items-center gap-2 rounded-full border border-line bg-surface-2 pl-2 pr-3 text-[13px] hover:bg-surface-3" aria-label="Language">
                  <Flag country={langMeta.flag} />
                  <span className="font-medium">{langMeta.name}</span>
                  {langMeta.rtl && <Chip size="sm" tone="info">RTL</Chip>}
                  <ChevronDown className="size-3.5 text-fg-3" />
                </button>
                {langOpen && (
                  <>
                    <div className="fixed inset-0 z-30" onClick={() => setLangOpen(false)} />
                    <div className="k-card z-40 max-h-[min(420px,60vh)] w-[250px] max-w-[calc(100vw-24px)] overflow-y-auto overscroll-contain rounded-2xl bg-surface p-1.5 shadow-[0_24px_60px_-20px_rgba(0,0,0,0.7)] max-sm:fixed max-sm:inset-x-3 max-sm:bottom-3 max-sm:w-auto max-sm:max-w-none sm:absolute sm:right-0 sm:mt-2">
                      {EML_LANGS.map((l) => {
                        const has = !!tpl.content[l.code];
                        return (
                          <button
                            key={l.code}
                            type="button"
                            onClick={() => {
                              setLang(l.code);
                              setLangOpen(false);
                            }}
                            className={cn("flex w-full items-center gap-2.5 rounded-xl px-2.5 py-2 text-left text-[13px] hover:bg-surface-3", l.code === lang && "bg-surface-3")}
                          >
                            <Flag country={l.flag} className="size-4" />
                            <span className="flex-1">
                              {l.name} <span className="text-fg-3">· {l.native}</span>
                            </span>
                            {l.rtl && <span className="text-[10px] font-semibold text-info">RTL</span>}
                            <span className={cn("size-1.5 rounded-full", has ? "bg-up" : "bg-warn")} title={has ? "Translated" : "Falls back to English"} />
                          </button>
                        );
                      })}
                    </div>
                  </>
                )}
              </div>
            </div>

            {/* language coverage strip */}
            <div className="mt-4 flex gap-1.5 overflow-x-auto px-6 pb-1">
              {EML_LANGS.map((l) => {
                const has = !!tpl.content[l.code];
                return (
                  <button
                    key={l.code}
                    type="button"
                    onClick={() => setLang(l.code)}
                    className={cn(
                      "inline-flex h-7 shrink-0 items-center gap-1.5 rounded-full border px-2.5 text-[11.5px] font-medium uppercase transition-colors",
                      l.code === lang ? "border-ember/40 bg-ember-soft text-ember" : has ? "border-line bg-surface-2 text-fg-2 hover:text-fg" : "border-dashed border-warn/40 text-warn/80 hover:text-warn",
                    )}
                  >
                    <Flag country={l.flag} className="size-3.5" />
                    {l.code}
                  </button>
                );
              })}
            </div>

            {!translated && (
              <div className="mx-6 mt-4 flex flex-wrap items-center gap-3 rounded-[14px] border border-warn/25 bg-warn-soft px-4 py-3 text-[12.5px]">
                <Languages className="size-4 shrink-0 text-warn" />
                <span className="min-w-0 flex-1 text-fg-2">
                  No {langMeta.name} version yet. Clients with this language currently receive the English template.
                </span>
                <Button
                  size="xs"
                  variant="surface"
                  onClick={() => {
                    update({ subject: `[${lang.toUpperCase()}] ${tpl.content.en.subject}`, body: tpl.content.en.body });
                    toast.success(`Machine translation drafted`, { description: `${langMeta.name} draft created from English. Review before publishing.` });
                  }}
                >
                  <Wand2 /> Machine-translate
                </Button>
              </div>
            )}

            <div className="space-y-4 px-4 pb-6 pt-5 sm:px-6" dir={langMeta.rtl ? "rtl" : "ltr"}>
              <Field label="Subject" hint={<span dir="ltr">{cur.subject.length}/120</span>}>
                <Input ref={subjRef} value={cur.subject} maxLength={120} onFocus={() => (lastFocus.current = "subject")} onChange={(e) => update({ subject: e.target.value })} />
              </Field>
              <div className="grid grid-cols-1 gap-3 sm:grid-cols-[1fr_220px]">
                <Field label="Preheader">
                  <Input value={cur.preheader} onChange={(e) => update({ preheader: e.target.value })} />
                </Field>
                <Field label="Button label">
                  <Input value={cur.cta ?? ""} placeholder="No button" onChange={(e) => update({ cta: e.target.value || undefined })} />
                </Field>
              </div>

              <div>
                <div className="mb-1.5 flex items-center justify-between text-[12.5px] font-medium text-fg-2">
                  <span>Body</span>
                  <span className="font-normal text-fg-3" dir="ltr">
                    Plain text · blank line = new paragraph
                  </span>
                </div>
                <textarea
                  ref={bodyRef}
                  value={cur.body}
                  onFocus={() => (lastFocus.current = "body")}
                  onChange={(e) => update({ body: e.target.value })}
                  spellCheck={false}
                  rows={13}
                  className={cn("w-full resize-y rounded-[14px] border border-line bg-surface-2 px-4 py-3.5 leading-[1.7] text-fg outline-none transition-colors focus:border-ember/50 focus:ring-4 focus:ring-ember/10", langMeta.rtl ? "font-sans text-[14px]" : "font-mono text-[12.5px]")}
                />
              </div>

              <div dir="ltr">
                <div className="mb-2 flex items-center gap-2">
                  <Braces className="size-3.5 text-fg-3" />
                  <span className="k-label">Variables</span>
                  <span className="text-[11.5px] text-fg-3">click to insert at the cursor in subject or body</span>
                </div>
                <div className="flex flex-wrap gap-1.5">
                  {EML_VARIABLES.map((v) => {
                    const inUse = used.includes(v.key);
                    return (
                      <button
                        key={v.key}
                        type="button"
                        title={`${v.label} · e.g. ${v.sample}`}
                        onMouseDown={(e) => e.preventDefault()}
                        onClick={() => insertVar(v.key)}
                        className={cn(
                          "inline-flex h-7 items-center rounded-full border px-2.5 font-mono text-[11.5px] transition-all",
                          inUse ? "border-ember/35 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:border-fg-3/40 hover:text-fg",
                        )}
                      >
                        {`{{${v.key}}}`}
                      </button>
                    );
                  })}
                </div>
                {unknown.length > 0 && (
                  <div className="mt-3 flex items-center gap-2 text-[12px] text-down">
                    <AlertTriangle className="size-3.5" /> Unknown: {unknown.map((u) => `{{${u}}}`).join(", ")}
                  </div>
                )}
              </div>

              <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4" dir="ltr">
                {dirty ? (
                  <Chip tone="ember" dot>
                    Unsaved changes
                  </Chip>
                ) : (
                  <Chip tone="up" dot>
                    v{versions[id]![0]!.v} live
                  </Chip>
                )}
                <span className="text-[12px] text-fg-3">
                  Last edit by {versions[id]![0]!.by.name} · {fmtAt(versions[id]![0]!.at)}
                </span>
                <div className="ml-auto flex gap-2">
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={!dirty}
                    onClick={() => {
                      setDrafts((d) => {
                        const n = { ...d };
                        delete n[k];
                        return n;
                      });
                      toast("Changes discarded");
                    }}
                  >
                    <RotateCcw /> Discard
                  </Button>
                  <Button size="sm" variant="ember" onClick={saveVersion}>
                    <Save /> Save version
                  </Button>
                </div>
              </div>
            </div>
          </Card>
        </Reveal>

        {/* ---------------- Preview ---------------- */}
        <Reveal delay={0.1} className="min-w-0 lg:col-start-2 2xl:col-start-3">
          <Card className="2xl:sticky 2xl:top-24">
            <div className="flex flex-wrap items-center justify-between gap-2 px-5 pt-5">
              <div className="flex items-center gap-2">
                <Eye className="size-4 text-fg-3" />
                <span className="text-[15px] font-medium">Live preview</span>
                {langMeta.rtl && (
                  <Chip size="sm" tone="info">
                    Right-to-left
                  </Chip>
                )}
              </div>
              <div className="flex items-center gap-2">
                <label className="flex items-center gap-2 text-[12px] text-fg-3">
                  Raw
                  <Toggle checked={raw} onChange={setRaw} label="Show raw variables" />
                </label>
                <Segmented
                  size="xs"
                  value={device}
                  onChange={setDevice}
                  options={[
                    { value: "desktop", label: <Monitor className="size-3.5" /> },
                    { value: "mobile", label: <Smartphone className="size-3.5" /> },
                  ]}
                />
              </div>
            </div>
            <div className="p-4 sm:p-5">
              <EmailPreview subject={cur.subject} preheader={cur.preheader} body={cur.body} cta={cur.cta} samples={SAMPLES} rtl={langMeta.rtl} raw={raw} width={device} />
            </div>
            <div className="flex items-center gap-2 border-t border-line px-5 py-3 text-[11.5px] text-fg-3">
              <Sparkles className="size-3.5 text-gold" /> Sample data: Arjun Mehta · login 80412337 · USDT TRC20
            </div>
          </Card>
        </Reveal>
      </div>

      {/* ---------------- Send test ---------------- */}
      <Dialog
        open={testOpen}
        onOpenChange={setTestOpen}
        title="Send test email"
        description={`${tpl.name} · ${langMeta.name} · rendered with sample data`}
        width={460}
        footer={
          <>
            <DialogClose asChild>
              <Button size="sm" variant="ghost">
                Cancel
              </Button>
            </DialogClose>
            <Button
              size="sm"
              variant="ember"
              disabled={!/^\S+@\S+\.\S+$/.test(testEmail)}
              onClick={() => {
                setTestOpen(false);
                toast.success(`Test sent to ${testEmail}`, { description: `via Amazon SES · “${cur.subject.replace(/\{\{\s*([a-z0-9_]+)\s*\}\}/gi, (_, x: string) => SAMPLES[x] ?? x)}”` });
              }}
            >
              <Send /> Send test
            </Button>
          </>
        }
      >
        <div className="space-y-4">
          <Field label="Recipient">
            <Input value={testEmail} onChange={(e) => setTestEmail(e.target.value)} type="email" />
          </Field>
          <div className="flex flex-wrap gap-1.5">
            {["priya.nair@kalkstrade.com", "qa@kalkstrade.com", "compliance@kalkstrade.com"].map((e) => (
              <button key={e} type="button" onClick={() => setTestEmail(e)} className={cn("rounded-full border px-2.5 py-1 text-[12px]", testEmail === e ? "border-ember/40 bg-ember-soft text-ember" : "border-line bg-surface-2 text-fg-2 hover:text-fg")}>
                {e}
              </button>
            ))}
          </div>
          <p className="text-[12px] text-fg-3">Test emails are tagged [TEST] in the subject and excluded from open-rate statistics.</p>
        </div>
      </Dialog>

      {/* ---------------- Version history ---------------- */}
      <Dialog open={histOpen} onOpenChange={setHistOpen} side="right" title="Version history" description={`${tpl.name} · every publish is kept and can be restored`}>
        <div className="relative space-y-3">
          <span className="absolute bottom-4 left-[17px] top-4 w-px bg-line" />
          {versions[id]!.map((v) => (
            <div key={v.v} className="relative flex gap-3">
              <Avatar src={v.by.photo} name={v.by.name} size={36} className="ring-4 ring-surface" />
              <div className={cn("k-row min-w-0 flex-1 px-4 py-3", v.live && "border-up/25")}>
                <div className="flex items-center gap-2">
                  <span className="font-mono text-[13px] font-semibold">v{v.v}</span>
                  {v.live && (
                    <Chip size="sm" tone="up" dot>
                      Live
                    </Chip>
                  )}
                  <span className="ml-auto text-[11.5px] text-fg-3">{fmtAt(v.at)}</span>
                </div>
                <div className="mt-1 text-[13px] text-fg-2">{v.note}</div>
                <div className="mt-0.5 text-[11.5px] text-fg-3">{v.by.name}</div>
                {!v.live && (
                  <div className="mt-2.5 flex gap-2">
                    <Button size="xs" variant="surface" onClick={() => toast.info(`Diff v${v.v} → v${versions[id]![0]!.v}`, { description: "3 lines changed in body · subject unchanged" })}>
                      Compare
                    </Button>
                    <Button
                      size="xs"
                      variant="ghost"
                      onClick={() => {
                        setHistOpen(false);
                        toast.success(`v${v.v} restored as draft`, { description: "Review and save to publish it again." });
                      }}
                    >
                      <RotateCcw /> Restore
                    </Button>
                  </div>
                )}
              </div>
            </div>
          ))}
        </div>
      </Dialog>
    </div>
  );
}

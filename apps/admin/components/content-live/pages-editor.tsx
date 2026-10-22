"use client";

// Content → Pages: every editable slot of the website, the Client Area, Kalks Trader and emails (services/content).
// Left: surface → page → slot tree (the catalogue of known slots merged with what the service holds). Right: the
// slot editor with 22 language tabs, draft / publish / unpublish, version history with rollback, and previews.

import * as React from "react";
import Link from "next/link";
import { ChevronDown, ChevronRight, Eye, FileText, History, Image as ImageIcon, Images, Link2, RotateCcw, RotateCw, Rocket, Save, Search, Trash2, TextCursorInput, Type, Undo2 } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, CardHeader, Chip, Dialog, EmptyState, PageHeader, Reveal, Segmented, Skeleton, cn } from "@kalks/ui";
import { useConfirm } from "@/components/confirm";
import { Markdown } from "@/components/academy-live/markdown";
import { inputCls } from "@/components/academy-live/quiz-editor";
import { CATALOGUE, pageLabel } from "@/lib/content-catalogue";
import {
  ContentError,
  LOCALES,
  LOCALE_NAME,
  SURFACE_LABEL,
  cnt,
  dirOf,
  fmtWhen,
  openInNewTab,
  summarize,
  useCnt,
  useContentMe,
  type Asset,
  type Entry,
  type Kind,
  type LocaleState,
  type Me,
  type SlotView,
  type Surface,
  type Tree,
  type Value,
  type Version,
} from "./api";
import { MediaPicker, ThemePair } from "./media";

type Sel = { surface: Surface; page: string; slot: string };
type MergedSlot = { slot: string; kind: Kind; label: string; help?: string; default?: string; inService: boolean; known: boolean; locales: Record<string, LocaleState> };
type MergedPage = { page: string; slots: MergedSlot[] };

const KIND_ICON: Record<Kind, React.ReactNode> = {
  text: <Type className="size-3.5" />,
  rich_text: <FileText className="size-3.5" />,
  image: <ImageIcon className="size-3.5" />,
  link: <Link2 className="size-3.5" />,
  placeholder: <TextCursorInput className="size-3.5" />,
};
const KIND_LABEL: Record<Kind, string> = { text: "Text", rich_text: "Rich text", image: "Image", link: "Link", placeholder: "Placeholder" };
const ACTION_TONE = { publish: "up", rollback: "info", unpublish: "warn" } as const;

const anyOf = (l: Record<string, LocaleState>, k: "draft" | "published") => Object.values(l).some((x) => x[k]);

/** Status dots of a slot (or a locale): green published, amber draft pending, grey built-in default. */
function Dots({ published, draft }: { published: boolean; draft: boolean }) {
  if (!published && !draft) return <span className="size-1.5 shrink-0 rounded-full bg-fg-3/40" title="Built-in default" />;
  return (
    <span className="flex shrink-0 items-center gap-0.5">
      {published && <span className="size-1.5 rounded-full bg-up" title="Published" />}
      {draft && <span className="size-1.5 rounded-full bg-warn" title="Draft not published yet" />}
    </span>
  );
}

function mergeTree(surface: Surface, tree: Tree | null): MergedPage[] {
  const pages = new Map<string, Map<string, MergedSlot>>();
  for (const c of CATALOGUE.filter((c) => c.surface === surface)) {
    if (!pages.has(c.page)) pages.set(c.page, new Map());
    pages.get(c.page)!.set(c.slot, { slot: c.slot, kind: c.kind, label: c.label, help: c.help, default: c.default, inService: false, known: true, locales: {} });
  }
  for (const p of tree?.pages.filter((p) => p.surface === surface) ?? []) {
    if (!pages.has(p.page)) pages.set(p.page, new Map());
    const m = pages.get(p.page)!;
    for (const s of p.slots) {
      const known = m.get(s.slot);
      m.set(s.slot, known ? { ...known, kind: s.kind, inService: true, locales: s.locales } : { slot: s.slot, kind: s.kind, label: s.label || s.slot, inService: true, known: false, locales: s.locales });
    }
  }
  return [...pages.entries()].map(([page, slots]) => ({ page, slots: [...slots.values()] }));
}

/* ------------------------------------------------------------------ */
/* Tree                                                                */
/* ------------------------------------------------------------------ */

function SlotTree({ surface, pages, sel, onSelect, q }: { surface: Surface; pages: MergedPage[]; sel: Sel | null; onSelect: (s: Sel) => void; q: string }) {
  const [closed, setClosed] = React.useState<Set<string>>(new Set());
  const needle = q.trim().toLowerCase();
  const shown = pages
    .map((p) => ({
      ...p,
      slots: needle ? p.slots.filter((s) => `${p.page} ${pageLabel(surface, p.page)} ${s.slot} ${s.label}`.toLowerCase().includes(needle)) : p.slots,
    }))
    .filter((p) => p.slots.length);
  if (!shown.length) return <p className="px-2 py-6 text-center text-[12.5px] text-fg-3">{needle ? "No slots match." : "No slots on this surface yet."}</p>;
  return (
    <div className="space-y-1.5" data-testid="content-tree">
      {shown.map((p) => {
        const open = needle !== "" || !closed.has(p.page) || sel?.page === p.page;
        const drafts = p.slots.filter((s) => anyOf(s.locales, "draft")).length;
        return (
          <div key={p.page} className="rounded-[16px] border border-line bg-surface-2/40">
            <button
              type="button"
              onClick={() => setClosed((c) => (c.has(p.page) ? new Set([...c].filter((x) => x !== p.page)) : new Set([...c, p.page])))}
              className="flex w-full items-center gap-2 px-3 py-2.5 text-left"
            >
              <span className="grid size-5 place-items-center text-fg-3">{open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}</span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[13.5px] font-medium">{pageLabel(surface, p.page)}</span>
                <span className="k-num block font-mono text-[11px] text-fg-3">
                  {p.page} · {p.slots.length} {p.slots.length === 1 ? "slot" : "slots"}
                </span>
              </span>
              {drafts > 0 && (
                <Chip size="sm" tone="warn">
                  {drafts} draft{drafts === 1 ? "" : "s"}
                </Chip>
              )}
            </button>
            {open && (
              <ul className="space-y-0.5 border-t border-line px-2 py-2">
                {p.slots.map((s) => {
                  const on = sel?.page === p.page && sel.slot === s.slot;
                  return (
                    <li key={s.slot}>
                      <button
                        type="button"
                        onClick={() => onSelect({ surface, page: p.page, slot: s.slot })}
                        className={cn("flex w-full items-center gap-2 rounded-[10px] px-2 py-1.5 text-left", on ? "bg-ember-soft" : "hover:bg-surface-3/60")}
                        data-testid={`tree-slot-${p.page}-${s.slot}`}
                      >
                        <Dots published={anyOf(s.locales, "published")} draft={anyOf(s.locales, "draft")} />
                        <span className="shrink-0 text-fg-3">{KIND_ICON[s.kind]}</span>
                        <span className="min-w-0 flex-1">
                          <span className={cn("block truncate text-[12.5px]", on ? "text-fg" : "text-fg-2")}>{s.label}</span>
                          <span className="block truncate font-mono text-[10.5px] text-fg-3">{s.slot}</span>
                        </span>
                        {!s.known && <span className="shrink-0 text-[10px] font-medium text-gold">Custom</span>}
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Value fields                                                        */
/* ------------------------------------------------------------------ */

const EMPTY: Record<Kind, Value> = { text: { text: "" }, placeholder: { text: "" }, rich_text: { markdown: "" }, link: { label: "", href: "" }, image: {} };

function same(a: Value | null | undefined, b: Value | null | undefined) {
  return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
}

function ValueField({
  kind,
  value,
  onChange,
  locale,
  readOnly,
  meta,
  media,
  site,
  canUpload,
  onPicked,
  fallback,
}: {
  kind: Kind;
  value: Value;
  onChange: (v: Value) => void;
  locale: string;
  readOnly: boolean;
  meta: MergedSlot;
  media: Record<string, Asset>;
  site?: string;
  canUpload: boolean;
  onPicked: (a: Asset) => void;
  /** What readers get in this locale when nothing is set here (English or the built-in default). */
  fallback: string;
}) {
  const dir = dirOf(locale);
  const [view, setView] = React.useState<"write" | "split" | "preview">("split");
  const [picking, setPicking] = React.useState(false);

  if (kind === "image") {
    const asset = value.media ? media[value.media] : undefined;
    const def = meta.default ? `${site ?? ""}${meta.default}` : undefined;
    return (
      <div className="space-y-3">
        {value.media ? (
          asset ? (
            <ThemePair asset={asset} />
          ) : (
            <p className="rounded-[12px] border border-warn/40 bg-warn-soft px-3 py-2 text-[12.5px] text-warn">Image {value.media} is no longer in the media library; the app shows its default.</p>
          )
        ) : (
          <div>
            <div className="mb-1.5 text-[12px] text-fg-3">{def ? "Nothing set: the built-in default shows" : "Nothing set: the built-in illustration shows"}</div>
            {def ? <ThemePair asset={null} fallback={def} /> : <ThemePair asset={null} />}
          </div>
        )}
        <div className="flex flex-wrap items-center gap-2">
          {!readOnly && (
            <Button size="sm" variant="surface" onClick={() => setPicking(true)} data-testid="choose-image">
              <Images /> Choose from library
            </Button>
          )}
          {asset && (
            <Link href={`/content/media?asset=${asset.id}`} className="text-[12.5px] text-fg-3 underline-offset-2 hover:text-fg hover:underline">
              Edit “{asset.name}” in the media library (dark file, alt text)
            </Link>
          )}
        </div>
        {asset && (
          <p className="text-[12px] text-fg-3">
            Alt text ({locale.toUpperCase()}): <span className="text-fg-2">{asset.alt[locale] ?? asset.alt.en ?? "none yet"}</span>
          </p>
        )}
        <MediaPicker
          open={picking}
          onOpenChange={setPicking}
          canUpload={canUpload}
          selected={value.media}
          onPick={(a) => {
            onPicked(a);
            onChange({ media: a.id });
            setPicking(false);
            toast.success("Image chosen", { description: `${a.name} · save the draft to keep it` });
          }}
        />
      </div>
    );
  }
  if (kind === "link") {
    return (
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <label className="block">
          <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Label</span>
          <input dir={dir} value={value.label ?? ""} disabled={readOnly} placeholder={fallback} onChange={(e) => onChange({ ...value, label: e.target.value })} className={inputCls} data-testid="field-label" />
        </label>
        <label className="block">
          <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Link</span>
          <input dir="ltr" value={value.href ?? ""} disabled={readOnly} placeholder="/register, #section, https://…" onChange={(e) => onChange({ ...value, href: e.target.value })} className={cn(inputCls, "font-mono")} data-testid="field-href" />
        </label>
        <p className="text-[11.5px] text-fg-3 md:col-span-2">Links can be a /path, a #section, an https:// or http:// address, or mailto:.</p>
      </div>
    );
  }
  if (kind === "rich_text") {
    return (
      <div>
        <div className="mb-2 flex justify-end">
          <Segmented
            size="xs"
            value={view}
            onChange={setView}
            options={[
              { value: "write", label: "Write" },
              { value: "split", label: "Split" },
              { value: "preview", label: <><Eye className="size-3" /> Preview</> },
            ]}
          />
        </div>
        <div className={cn("grid gap-3", view === "split" && "xl:grid-cols-2")}>
          {view !== "preview" && (
            <textarea dir={dir} value={value.markdown ?? ""} disabled={readOnly} placeholder={fallback} onChange={(e) => onChange({ markdown: e.target.value })} className={cn(inputCls, "h-[360px] resize-y font-mono text-[12.5px] leading-relaxed")} data-testid="field-markdown" />
          )}
          {view !== "write" && (
            <div dir={dir} className="h-[360px] overflow-y-auto rounded-[12px] border border-line bg-surface px-5 py-4">
              <Markdown src={value.markdown || fallback} className="text-[14px]" />
            </div>
          )}
        </div>
        <p className="mt-1.5 text-[11.5px] text-fg-3">Markdown: ## headings, lists, **bold**, links, tables. Up to 20,000 characters.</p>
      </div>
    );
  }
  const max = kind === "placeholder" ? 200 : 2000;
  const len = (value.text ?? "").length;
  return (
    <div>
      {kind === "placeholder" ? (
        <input dir={dir} value={value.text ?? ""} disabled={readOnly} maxLength={max} placeholder={fallback} onChange={(e) => onChange({ text: e.target.value })} className={inputCls} data-testid="field-text" />
      ) : (
        <textarea dir={dir} value={value.text ?? ""} disabled={readOnly} maxLength={max} rows={3} placeholder={fallback} onChange={(e) => onChange({ text: e.target.value })} className={inputCls} data-testid="field-text" />
      )}
      <div className="k-num mt-1 text-end text-[11px] text-fg-3">
        {len} / {max}
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* History                                                             */
/* ------------------------------------------------------------------ */

function HistoryDrawer({ open, onOpenChange, sel, locale, kind, canPublish, media, onRolledBack }: { open: boolean; onOpenChange: (o: boolean) => void; sel: Sel; locale: string; kind: Kind; canPublish: boolean; media: Record<string, Asset>; onRolledBack: () => void }) {
  const [ask, confirmDialog] = useConfirm();
  const qs = `surface=${sel.surface}&page=${sel.page}&slot=${encodeURIComponent(sel.slot)}&locale=${locale}`;
  const v = useCnt<{ versions: Version[]; current: number }>(open ? `versions?${qs}` : null);
  const rollback = async (ver: Version) => {
    const ok = await ask({ title: `Roll back to version ${ver.version}?`, text: `${ver.value ? `“${summarize(kind, ver.value, media)}”` : "The built-in default"} goes live again in ${LOCALE_NAME[locale as keyof typeof LOCALE_NAME] ?? locale} as a new version. Drafts are kept.`, confirm: "Roll back" });
    if (!ok) return;
    try {
      const r = await cnt<{ version: number }>("rollback", { body: { ...sel, locale, version: ver.version } });
      toast.success("Rolled back", { description: `Version ${ver.version} is live again as version ${r.version}.` });
      v.reload();
      onRolledBack();
    } catch (e) {
      toast.error("Couldn't roll back", { description: (e as Error).message });
    }
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange} side="right" title="Version history" description={`${sel.page} / ${sel.slot} · ${LOCALE_NAME[locale as keyof typeof LOCALE_NAME] ?? locale} · the last 50 versions`}>
      {confirmDialog}
      {v.error ? (
        <EmptyState illustration="satellite_antenna" title="Couldn't load the history" text={v.error.message} action={<Button variant="surface" onClick={v.reload}><RotateCw /> Try again</Button>} />
      ) : !v.data ? (
        <Skeleton className="h-64 w-full rounded-[16px]" />
      ) : v.data.versions.length === 0 ? (
        <EmptyState illustration="hourglass_not_done" title="Nothing published yet" text="Versions appear when this language is published, rolled back or unpublished." />
      ) : (
        <ol className="space-y-2" data-testid="versions">
          {v.data.versions.map((x) => {
            const current = x.version === v.data!.current;
            return (
              <li key={x.version} className={cn("k-row px-3 py-2.5", current && "border-up/40")}>
                <div className="flex flex-wrap items-center gap-2">
                  <span className="k-num font-mono text-[12px] text-fg-2">v{x.version}</span>
                  <Chip size="sm" tone={ACTION_TONE[x.action as keyof typeof ACTION_TONE] ?? "neutral"}>
                    {x.action}
                    {x.from_version ? ` of v${x.from_version}` : ""}
                  </Chip>
                  {current && (
                    <Chip size="sm" tone="up" dot>
                      Live
                    </Chip>
                  )}
                  <span className="ms-auto text-[11.5px] text-fg-3">
                    {fmtWhen(x.at)} · {x.by}
                  </span>
                </div>
                <div className="mt-1.5 flex items-end gap-2">
                  <p dir={dirOf(locale)} className="min-w-0 flex-1 break-words text-[12.5px] text-fg-2">
                    {summarize(kind, x.value, media)}
                  </p>
                  {canPublish && !current && (
                    <Button size="xs" variant="surface" onClick={() => rollback(x)} data-testid={`rollback-${x.version}`}>
                      <RotateCcw /> Roll back
                    </Button>
                  )}
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Slot editor                                                         */
/* ------------------------------------------------------------------ */

function SlotEditor({ sel, meta, me, initialLocale, onChanged }: { sel: Sel; meta: MergedSlot; me: Me; initialLocale?: string; onChanged: () => void }) {
  const [ask, confirmDialog] = useConfirm();
  const qs = `surface=${sel.surface}&page=${sel.page}&slot=${encodeURIComponent(sel.slot)}`;
  const view = useCnt<SlotView>(meta.inService ? `slot?${qs}` : null);
  const [locale, setLocale] = React.useState<string>(initialLocale && (LOCALES as readonly string[]).includes(initialLocale) ? initialLocale : "en");
  const [edits, setEdits] = React.useState<Record<string, Value>>({});
  const [picked, setPicked] = React.useState<Record<string, Asset>>({});
  const [busy, setBusy] = React.useState(false);
  const [history, setHistory] = React.useState(false);

  const missing = view.error?.status === 404;
  if (view.error && !missing) return <EmptyState illustration="satellite_antenna" title="Couldn't load this slot" text={view.error.message} action={<Button variant="surface" onClick={view.reload}><RotateCw /> Try again</Button>} />;
  if (meta.inService && !view.data && !missing) return <Skeleton className="h-[460px] w-full rounded-[18px]" />;

  const kind = meta.kind;
  // (a slot deleted or never saved has no service data; a stale fetch from before a delete is ignored)
  const fresh = meta.inService && !missing ? view.data : null;
  const entries: Record<string, Entry> = fresh?.entries ?? {};
  const media = { ...(fresh?.media ?? {}), ...picked };
  const entry = entries[locale];
  const stored = entry?.draft ?? entry?.published ?? null;
  const value = edits[locale] ?? stored ?? EMPTY[kind];
  const dirty = edits[locale] !== undefined && !same(edits[locale], stored ?? EMPTY[kind]);
  const ro = !me.write;
  const en = entries.en?.published;
  const builtIn = meta.default ? (kind === "image" ? "the built-in image" : meta.default) : "";
  const fallbackText = locale !== "en" && en ? summarize(kind, en, media) : builtIn;
  const localeName = LOCALE_NAME[locale as keyof typeof LOCALE_NAME] ?? locale;

  const refresh = () => {
    view.reload();
    onChanged();
  };
  const run = async (fn: () => Promise<void>) => {
    setBusy(true);
    try {
      await fn();
    } finally {
      setBusy(false);
    }
  };
  const problem = (): string | null => {
    if (kind === "image" && !value.media) return "Choose an image first.";
    if (kind === "link" && (!value.label?.trim() || !value.href?.trim())) return "A link needs a label and an address.";
    return null;
  };
  const saveDraft = async (quiet = false) => {
    const p = problem();
    if (p) {
      toast.error("Not saved", { description: p });
      return false;
    }
    try {
      await cnt("entry", { method: "PUT", body: { ...sel, kind, label: meta.label, locale, value } });
      setEdits(({ [locale]: _drop, ...rest }) => rest);
      if (!quiet) toast.success("Draft saved", { description: `${meta.label} · ${localeName}. Publish it to show it to clients.` });
      return true;
    } catch (e) {
      toast.error("Not saved", { description: (e as ContentError).message });
      return false;
    }
  };
  const onSave = () =>
    run(async () => {
      if (await saveDraft()) refresh();
    });
  const onDiscard = async () => {
    if (!entry?.draft) {
      setEdits(({ [locale]: _drop, ...rest }) => rest);
      toast.success("Changes discarded");
      return;
    }
    if (!(await ask({ title: "Discard this draft?", text: `The unpublished ${localeName} draft of “${meta.label}” is deleted. What's live stays as it is.`, confirm: "Discard draft", tone: "danger" }))) return;
    await run(async () => {
      try {
        await cnt("discard", { body: { ...sel, locale } });
        setEdits(({ [locale]: _drop, ...rest }) => rest);
        toast.success("Draft discarded");
        refresh();
      } catch (e) {
        toast.error("Couldn't discard", { description: (e as Error).message });
      }
    });
  };
  const onPublish = () =>
    run(async () => {
      if (dirty && !(await saveDraft(true))) return;
      try {
        const r = await cnt<{ published: unknown[] }>("publish", { body: { ...sel, locale } });
        if (r.published.length) toast.success("Published", { description: `${meta.label} · ${localeName} is live.` });
        else toast.info("Nothing to publish", { description: "This language has no draft." });
        refresh();
      } catch (e) {
        toast.error("Couldn't publish", { description: (e as Error).message });
        refresh();
      }
    });
  const onUnpublish = async () => {
    if (!(await ask({ title: `Unpublish ${localeName}?`, text: locale === "en" ? "Clients see the built-in default again (other languages without their own value too)." : "Clients reading this language see the English value (or the built-in default) again.", confirm: "Unpublish", tone: "danger" }))) return;
    await run(async () => {
      try {
        await cnt("unpublish", { body: { ...sel, locale } });
        toast.success("Unpublished", { description: `${meta.label} · ${localeName}` });
        refresh();
      } catch (e) {
        toast.error("Couldn't unpublish", { description: (e as Error).message });
      }
    });
  };
  const onDelete = async () => {
    if (!(await ask({ title: "Delete this slot?", text: "Every language, draft and version of it is deleted and the app shows its built-in default again. This can't be undone.", confirm: "Delete slot", tone: "danger" }))) return;
    await run(async () => {
      try {
        await cnt(`slot?${qs}`, { method: "DELETE" });
        setEdits({});
        toast.success("Slot deleted", { description: `${sel.page} / ${sel.slot}` });
        onChanged();
      } catch (e) {
        toast.error("Couldn't delete", { description: (e as Error).message });
      }
    });
  };

  const published = !!entry?.published;
  const hasDraft = !!entry?.draft;
  return (
    <Card className="overflow-hidden" data-testid="slot-editor">
      {confirmDialog}
      <div className="flex flex-wrap items-start justify-between gap-3 border-b border-line px-6 py-5">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <Chip size="sm">
              {KIND_ICON[kind]} {KIND_LABEL[kind]}
            </Chip>
            {anyOf(meta.locales, "published") ? (
              <Chip size="sm" tone="up" dot>
                Published
              </Chip>
            ) : (
              <Chip size="sm" dot>
                Built-in default
              </Chip>
            )}
            {anyOf(meta.locales, "draft") && (
              <Chip size="sm" tone="warn" dot>
                Draft pending
              </Chip>
            )}
            {!meta.known && (
              <Chip size="sm" tone="gold">
                Custom slot
              </Chip>
            )}
            {dirty && (
              <Chip size="sm" tone="warn">
                Unsaved changes
              </Chip>
            )}
          </div>
          <h2 className="mt-2 truncate text-[19px] font-medium tracking-tight">{meta.label}</h2>
          <div className="mt-0.5 font-mono text-[12px] text-fg-3">
            {SURFACE_LABEL[sel.surface]} · {sel.page} / {sel.slot}
          </div>
          {meta.help && <p className="mt-1.5 max-w-xl text-[12.5px] text-fg-3">{meta.help}</p>}
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="ghost" onClick={() => setHistory(true)} disabled={!meta.inService} data-testid="open-history">
            <History /> History
          </Button>
          {me.publish && meta.inService && (
            <Button size="sm" variant="ghost" onClick={onDelete} disabled={busy} data-testid="delete-slot">
              <Trash2 /> Delete slot
            </Button>
          )}
        </div>
      </div>

      {/* languages */}
      <div className="border-b border-line px-4 py-3 sm:px-6">
        <div className="-mx-1 flex gap-1 overflow-x-auto px-1 pb-1 [scrollbar-width:thin]" role="tablist" aria-label="Language" data-testid="locale-tabs">
          {LOCALES.map((l) => {
            const e = entries[l];
            const on = l === locale;
            return (
              <button
                key={l}
                type="button"
                role="tab"
                aria-selected={on}
                title={`${LOCALE_NAME[l]}${e?.published ? " · published" : ""}${e?.draft ? " · draft" : ""}`}
                onClick={() => setLocale(l)}
                className={cn("flex shrink-0 items-center gap-1.5 rounded-full border px-2.5 py-1 font-mono text-[11.5px] uppercase transition-colors", on ? "border-ember/50 bg-ember-soft text-fg" : "border-line bg-surface-2 text-fg-3 hover:text-fg")}
                data-testid={`locale-${l}`}
              >
                {l}
                {(e?.published || e?.draft) && <Dots published={!!e?.published} draft={!!e?.draft} />}
                {edits[l] !== undefined && <span className="text-warn">•</span>}
              </button>
            );
          })}
        </div>
      </div>

      <div className="space-y-4 px-6 py-5">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-fg-3">
          <span className="font-medium text-fg-2">{localeName}</span>
          {published ? (
            <span>
              Live v{entry!.version} · {fmtWhen(entry!.published_at)} by {entry!.published_by}
            </span>
          ) : locale === "en" ? (
            <span>Not published: clients see {meta.default ? "the built-in default" : kind === "image" ? "the built-in image" : "nothing (built-in default)"}</span>
          ) : (
            <span>Not published: clients see {en ? "the English value" : "the built-in default"}</span>
          )}
          {hasDraft && (
            <span className="text-warn">
              Draft {fmtWhen(entry!.draft_at)} by {entry!.draft_by}
            </span>
          )}
        </div>

        <ValueField
          key={`${locale}:${kind}`}
          kind={kind}
          value={value}
          onChange={(v) => setEdits((x) => ({ ...x, [locale]: v }))}
          locale={locale}
          readOnly={ro}
          meta={meta}
          media={media}
          site={me.sites[sel.surface]}
          canUpload={me.write}
          onPicked={(a) => setPicked((p) => ({ ...p, [a.id]: a }))}
          fallback={fallbackText}
        />

        {published && hasDraft && kind !== "image" && (
          <div className="rounded-[12px] border border-line bg-surface-2/60 px-3 py-2 text-[12px] text-fg-3">
            Live now: <span dir={dirOf(locale)} className="text-fg-2">{summarize(kind, entry!.published, media)}</span>
          </div>
        )}
        {ro && <p className="text-[12px] text-fg-3">Read-only: your role can view site content but not edit it.</p>}
      </div>

      {(me.write || me.publish) && (
        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-line px-6 py-4">
          {me.publish && published && (
            <Button size="sm" variant="ghost" onClick={onUnpublish} disabled={busy} data-testid="unpublish">
              <Undo2 /> Unpublish
            </Button>
          )}
          {me.write && (hasDraft || dirty) && (
            <Button size="sm" variant="ghost" onClick={onDiscard} disabled={busy} data-testid="discard">
              <RotateCcw /> Discard draft
            </Button>
          )}
          {me.write && (
            <Button size="sm" variant="surface" onClick={onSave} disabled={busy || !dirty} data-testid="save-draft">
              <Save /> Save draft
            </Button>
          )}
          {me.publish && (
            <Button size="sm" variant="ember" onClick={onPublish} disabled={busy || (!dirty && !hasDraft)} data-testid="publish">
              <Rocket /> Publish {locale.toUpperCase()}
            </Button>
          )}
        </div>
      )}

      {meta.inService && <HistoryDrawer open={history} onOpenChange={setHistory} sel={sel} locale={locale} kind={kind} canPublish={me.publish} media={media} onRolledBack={refresh} />}
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Page                                                                */
/* ------------------------------------------------------------------ */

type AuditRow = { staff: string; action: string; target: string; detail: Record<string, unknown>; at: string };

export function LiveContentPages() {
  const { me } = useContentMe();
  const [surface, setSurface] = React.useState<Surface>("website");
  const [q, setQ] = React.useState("");
  const [sel, setSel] = React.useState<Sel | null>(null);
  const [deepLocale, setDeepLocale] = React.useState<string | undefined>();
  const tree = useCnt<Tree>(me?.read ? "tree" : null);
  const audit = useCnt<{ audit: AuditRow[] }>(me?.read ? "audit" : null);
  const [busy, setBusy] = React.useState(false);
  const [ask, confirmDialog] = useConfirm();

  // deep links from the media library: ?surface&page&slot&locale
  React.useEffect(() => {
    const sp = new URLSearchParams(window.location.search);
    const s = sp.get("surface") as Surface | null;
    if (s && s in SURFACE_LABEL) {
      setSurface(s);
      const page = sp.get("page");
      const slot = sp.get("slot");
      if (page && slot) setSel({ surface: s, page, slot });
      setDeepLocale(sp.get("locale") ?? undefined);
    }
  }, []);

  const pages = React.useMemo(() => mergeTree(surface, tree.data), [surface, tree.data]);
  React.useEffect(() => {
    // wait for the tree so a deep link (set on mount) isn't replaced by the first slot
    if (!tree.data || sel?.surface === surface) return;
    const first = pages[0];
    setSel(first?.slots[0] ? { surface, page: first.page, slot: first.slots[0].slot } : null);
  }, [pages, surface, sel, tree.data]);

  if (!me) return <Skeleton className="h-[640px] w-full rounded-[18px]" />;
  if (!me.read) return <EmptyState illustration="identification_card" title="No access" text="Your role doesn't include site content." />;

  const refresh = () => {
    tree.reload();
    audit.reload();
  };
  const selPage = sel ? pages.find((p) => p.page === sel.page) : undefined;
  const meta = sel && selPage ? selPage.slots.find((s) => s.slot === sel.slot) : undefined;
  const pageDrafts = selPage?.slots.reduce((n, s) => n + Object.values(s.locales).filter((l) => l.draft).length, 0) ?? 0;
  const counts = Object.fromEntries(
    (Object.keys(SURFACE_LABEL) as Surface[]).map((s) => {
      const m = mergeTree(s, tree.data);
      return [s, m.reduce((n, p) => n + p.slots.length, 0)];
    }),
  ) as Record<Surface, number>;

  const preview = () =>
    sel &&
    openInNewTab(async () => (await cnt<{ url: string }>("preview", { body: { surface: sel.surface, page: sel.page } })).url)
      .then(() => toast.success("Preview opened", { description: "Drafts show in place of published content for 30 minutes." }))
      .catch((e) => toast.error("Couldn't open the preview", { description: (e as Error).message }));
  const publishPage = () =>
    sel &&
    selPage &&
    (async () => {
      const drafted = selPage.slots
        .map((sl) => ({ slot: sl.slot, label: sl.label, langs: Object.entries(sl.locales).filter(([, l]) => l.draft).map(([k]) => k) }))
        .filter((x) => x.langs.length);
      const langs = [...new Set(drafted.flatMap((x) => x.langs))].sort();
      const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? "" : "s"}`;
      const ok = await ask({
        title: `Publish all of ${pageLabel(sel.surface, sel.page)}?`,
        text: (
          <div className="space-y-2.5">
            <p>
              This publishes <b className="text-fg">every draft on this page</b>: {plural(pageDrafts, "draft")} in {plural(drafted.length, "slot")} across {plural(langs.length, "language")} (
              {langs.map((l) => l.toUpperCase()).join(", ")}), not just the language or slot you&apos;re editing. Drafts saved by colleagues are included.
            </p>
            <ul className="max-h-40 space-y-0.5 overflow-y-auto text-[12.5px] text-fg-3">
              {drafted.map((x) => (
                <li key={x.slot}>
                  {x.label} · {x.langs.map((l) => l.toUpperCase()).join(", ")}
                </li>
              ))}
            </ul>
            <p>Clients see the new content within about a minute. Unsaved edits in the editor aren&apos;t included.</p>
          </div>
        ),
        confirm: `Publish ${plural(pageDrafts, "draft")}`,
      });
      if (!ok) return;
      setBusy(true);
      try {
        const r = await cnt<{ published: unknown[] }>("publish", { body: { surface: sel.surface, page: sel.page } });
        toast.success(r.published.length ? "Page published" : "Nothing to publish", { description: r.published.length ? `${r.published.length} draft${r.published.length === 1 ? "" : "s"} of ${pageLabel(sel.surface, sel.page)} went live.` : "This page has no drafts." });
        refresh();
      } catch (e) {
        toast.error("Couldn't publish the page", { description: (e as Error).message });
      } finally {
        setBusy(false);
      }
    })();

  return (
    <div className="pb-16">
      {confirmDialog}
      <PageHeader
        title="Pages"
        subtitle="Texts, images and links of the website, the Client Area, Kalks Trader and emails, in 22 languages. Save drafts, preview them, then publish."
        actions={
          sel && (
            <>
              {sel.surface !== "email" && (
                <Button variant="surface" onClick={preview} data-testid="preview">
                  <Eye /> Preview
                </Button>
              )}
              {me.publish && (
                <Button variant="ember" onClick={publishPage} disabled={busy || pageDrafts === 0} data-testid="publish-page">
                  <Rocket /> Publish page{pageDrafts ? ` (${pageDrafts})` : ""}
                </Button>
              )}
            </>
          )
        }
      />

      {tree.error ? (
        <EmptyState illustration="satellite_antenna" title="The content service is unavailable" text={tree.error.message} action={<Button variant="surface" onClick={tree.reload}><RotateCw /> Try again</Button>} />
      ) : (
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-12">
          <Reveal className="xl:col-span-4">
            <Card className="h-full">
              <CardHeader title="Surfaces" subtitle={tree.data ? `${pages.length} pages · ${counts[surface]} slots` : "Loading"} />
              <div className="space-y-3 px-3 pt-3 sm:px-4">
                <div className="-mx-1 overflow-x-auto px-1 [scrollbar-width:none]">
                  <Segmented
                    size="sm"
                    value={surface}
                    onChange={(s) => {
                      setSurface(s);
                      setSel(null);
                    }}
                    options={(Object.keys(SURFACE_LABEL) as Surface[]).map((s) => ({ value: s, label: SURFACE_LABEL[s] }))}
                    aria-label="Surface"
                  />
                </div>
                <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
                  <Search className="size-3.5 text-fg-3" />
                  <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search pages and slots…" className="w-full bg-transparent text-[13px] outline-none placeholder:text-fg-3" data-testid="tree-search" />
                </div>
                <div className="flex flex-wrap gap-3 px-1 text-[11px] text-fg-3">
                  <span className="flex items-center gap-1.5"><span className="size-1.5 rounded-full bg-up" /> Published</span>
                  <span className="flex items-center gap-1.5"><span className="size-1.5 rounded-full bg-warn" /> Draft</span>
                  <span className="flex items-center gap-1.5"><span className="size-1.5 rounded-full bg-fg-3/40" /> Built-in default</span>
                </div>
              </div>
              <div className="max-h-[980px] overflow-y-auto px-3 pb-4 pt-3 sm:px-4">
                {tree.data ? <SlotTree surface={surface} pages={pages} sel={sel} onSelect={setSel} q={q} /> : <Skeleton className="h-[420px] w-full rounded-[16px]" />}
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.05} className="min-w-0 xl:col-span-8">
            {!tree.data ? (
              <Skeleton className="h-[520px] w-full rounded-[18px]" />
            ) : sel && meta ? (
              <SlotEditor key={`${sel.surface}/${sel.page}/${sel.slot}`} sel={sel} meta={meta} me={me} initialLocale={deepLocale} onChanged={refresh} />
            ) : (
              <Card>
                <EmptyState illustration="package" title="No slots here yet" text={surface === "email" ? "Email texts appear here once email templates read from the content service." : "Slots appear here once the site reads them or a draft is saved."} />
              </Card>
            )}
            <Card className="mt-4">
              <CardHeader title="Recent changes" subtitle="Publishing, rollbacks and media changes by your team" icon={<History />} />
              <div className="max-h-[260px] space-y-1 overflow-y-auto px-4 pb-5 pt-3 sm:px-6" data-testid="content-audit">
                {!audit.data && !audit.error && <Skeleton className="h-16 w-full rounded-[12px]" />}
                {audit.error && <p className="text-[12.5px] text-fg-3">{audit.error.message}</p>}
                {audit.data?.audit.length === 0 && <p className="text-[12.5px] text-fg-3">Nothing published yet: every page shows its built-in content.</p>}
                {audit.data?.audit.map((a, i) => (
                  <div key={i} className="flex items-baseline gap-2 border-b border-line py-1.5 text-[12px] last:border-0">
                    <span className="k-num shrink-0 text-fg-3">{fmtWhen(a.at)}</span>
                    <span className="min-w-0 truncate text-fg-2">
                      <span className="text-fg">{a.staff}</span> {a.action.replace(".", " ").replace("_", " ")} <span className="font-mono text-fg">{a.target}</span>
                    </span>
                  </div>
                ))}
              </div>
            </Card>
          </Reveal>
        </div>
      )}
    </div>
  );
}

"use client";

// Content → Media library: the tenant's images (services/content media). Upload (button or drag and drop, many at
// once), search, and per image: light and dark files, resized variants, alt text per language, usage, delete.

import * as React from "react";
import Link from "next/link";
import { Copy, ExternalLink, Moon, RotateCw, Save, Search, Trash2, Upload } from "lucide-react";
import { toast } from "sonner";
import { Button, Card, Chip, Dialog, EmptyState, PageHeader, Reveal, Skeleton, cn } from "@kalks/ui";
import { useConfirm } from "@/components/confirm";
import { inputCls } from "@/components/academy-live/quiz-editor";
import { pageLabel } from "@/lib/content-catalogue";
import { ContentError, LOCALES, LOCALE_NAME, SURFACE_LABEL, cnt, dirOf, fmtBytes, fmtWhen, thumbOf, uploadImage, uploadProblem, useCnt, useContentMe, type Asset, type MediaFile, type Me, type Usage } from "./api";
import { DropZone, FileButton, ThemeTile, UploadJobs, useUploader } from "./media";

const PAGE = 60;

function UsageList({ usage }: { usage: Usage[] }) {
  if (!usage.length) return <p className="text-[12.5px] text-fg-3">Not used in any content yet.</p>;
  return (
    <ul className="space-y-1" data-testid="usage">
      {usage.map((u, i) => (
        <li key={i}>
          <Link
            href={`/content/pages?surface=${u.surface}&page=${encodeURIComponent(u.page)}&slot=${encodeURIComponent(u.slot)}&locale=${u.locale}`}
            className="k-row flex items-center gap-2 px-3 py-2 text-[12.5px] hover:border-[var(--k-border-top)]"
          >
            <span className="min-w-0 flex-1 truncate">
              {SURFACE_LABEL[u.surface] ?? u.surface} · {pageLabel(u.surface, u.page)} · <span className="font-mono text-fg-3">{u.slot}</span>
            </span>
            <span className="font-mono text-[11px] uppercase text-fg-3">{u.locale}</span>
            {u.published && (
              <Chip size="sm" tone="up" dot>
                Live
              </Chip>
            )}
            {u.draft && (
              <Chip size="sm" tone="warn" dot>
                Draft
              </Chip>
            )}
          </Link>
        </li>
      ))}
    </ul>
  );
}

function Variants({ file, label }: { file: MediaFile; label: string }) {
  const rows: [string, string, number | null, number | null | undefined, number | null | undefined][] = [
    ...(["sm", "md", "lg"] as const).flatMap((k) => {
      const v = file.variants[k];
      return v ? [[k, v.url, v.width, v.height, v.bytes] as [string, string, number, number | null, number | null]] : [];
    }),
    ["original", file.url, file.width, file.height, file.bytes],
  ];
  return (
    <div>
      <div className="mb-1 text-[11.5px] font-medium uppercase tracking-wider text-fg-3">{label}</div>
      <ul className="divide-y divide-line rounded-[12px] border border-line">
        {rows.map(([k, url, w, h, b]) => (
          <li key={k} className="flex items-center gap-3 px-3 py-1.5 text-[12px]">
            <span className="w-16 font-mono text-fg-2">{k}</span>
            <span className="k-num flex-1 text-fg-3">
              {w ? `${w}${h ? `×${h}` : ""} px` : file.mime === "image/svg+xml" ? "vector" : "–"} · {fmtBytes(b)}
              {k !== "original" && " · WebP"}
            </span>
            <a href={url} target="_blank" rel="noreferrer" className="inline-flex items-center gap-1 text-fg-3 hover:text-fg">
              Open <ExternalLink className="size-3" />
            </a>
          </li>
        ))}
      </ul>
    </div>
  );
}

function AssetDrawer({ id, me, onClose, onChanged }: { id: string | null; me: Me; onClose: () => void; onChanged: (a: Asset | null) => void }) {
  const [ask, confirmDialog] = useConfirm();
  const d = useCnt<{ asset: Asset; usage: Usage[] }>(id ? `media/${id}` : null);
  const asset = d.data?.asset.id === id ? d.data.asset : null;
  const usage = asset ? d.data!.usage : [];
  const [name, setName] = React.useState("");
  const [alt, setAlt] = React.useState<Record<string, string>>({});
  const [allLocales, setAllLocales] = React.useState(false);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [blocked, setBlocked] = React.useState<Usage[] | null>(null);

  React.useEffect(() => {
    if (asset) {
      setName(asset.name);
      setAlt({ ...asset.alt });
    }
    setBlocked(null);
  }, [asset]);

  const live = usage.some((u) => u.published);
  // replacing / removing a file of an image shown on a live page needs content.publish (the service enforces it)
  const canFile = me.write && (!live || me.publish);
  const altDirty = asset ? LOCALES.some((l) => (alt[l] ?? "").trim() !== (asset.alt[l] ?? "")) : false;
  const nameDirty = asset ? name.trim() !== asset.name : false;

  const update = (a: Asset) => {
    d.setData((x) => (x ? { ...x, asset: a } : x));
    onChanged(a);
  };
  const save = async () => {
    if (!asset) return;
    setBusy("save");
    try {
      const body: { name?: string; alt?: Record<string, string> } = {};
      if (nameDirty) body.name = name;
      if (altDirty) body.alt = Object.fromEntries(LOCALES.filter((l) => (alt[l] ?? "").trim() !== (asset.alt[l] ?? "")).map((l) => [l, (alt[l] ?? "").trim()]));
      const r = await cnt<{ asset: Asset }>(`media/${asset.id}`, { method: "PUT", body });
      update(r.asset);
      toast.success("Saved", { description: [nameDirty && "name", altDirty && "alt text"].filter(Boolean).join(" and ") });
    } catch (e) {
      toast.error("Not saved", { description: (e as Error).message });
    } finally {
      setBusy(null);
    }
  };
  const replace = async (theme: "light" | "dark", files: FileList) => {
    if (!asset || !files[0]) return;
    const f = files[0];
    const bad = uploadProblem(f);
    if (bad) return toast.error("Not uploaded", { description: bad });
    if (live && !(await ask({ title: theme === "light" ? "Replace the image on live pages?" : "Change the dark file on live pages?", text: `“${asset.name}” is published on ${usage.filter((u) => u.published).length} slot(s). Clients see the new file right away.`, confirm: "Replace" }))) return;
    setBusy(theme);
    try {
      const r = await uploadImage<{ asset: Asset }>(`media/${asset.id}/file?theme=${theme}`, f);
      update(r.asset);
      toast.success(theme === "light" ? "Image replaced" : asset.dark ? "Dark file replaced" : "Dark file added", { description: f.name });
    } catch (e) {
      toast.error("Couldn't upload", { description: (e as Error).message });
    } finally {
      setBusy(null);
    }
  };
  const removeDark = async () => {
    if (!asset) return;
    if (!(await ask({ title: "Remove the dark file?", text: "The light image then shows in dark mode too.", confirm: "Remove", tone: "danger" }))) return;
    setBusy("dark");
    try {
      const r = await cnt<{ asset: Asset }>(`media/${asset.id}/file?theme=dark`, { method: "DELETE" });
      update(r.asset);
      toast.success("Dark file removed");
    } catch (e) {
      toast.error("Couldn't remove", { description: (e as Error).message });
    } finally {
      setBusy(null);
    }
  };
  const del = async () => {
    if (!asset) return;
    if (!(await ask({ title: `Delete “${asset.name}”?`, text: "It leaves the library. Images that are still used in content can't be deleted.", confirm: "Delete", tone: "danger" }))) return;
    setBusy("delete");
    try {
      await cnt(`media/${asset.id}`, { method: "DELETE" });
      toast.success("Image deleted", { description: asset.name });
      onChanged(null);
      onClose();
    } catch (e) {
      const ce = e as ContentError;
      if (ce.code === "in_use") setBlocked(ce.detail?.usage ?? []);
      toast.error("Couldn't delete", { description: ce.message });
    } finally {
      setBusy(null);
    }
  };
  const copy = async () => {
    if (!asset) return;
    try {
      await navigator.clipboard.writeText(asset.light.url);
      toast.success("URL copied", { description: asset.light.url });
    } catch {
      toast.error("Couldn't copy", { description: asset.light.url });
    }
  };

  const locales = allLocales ? LOCALES : LOCALES.filter((l) => l === "en" || alt[l] || asset?.alt[l]);
  return (
    <Dialog
      open={!!id}
      onOpenChange={(o) => !o && onClose()}
      side="right"
      title={asset?.name ?? "Image"}
      description={asset ? `${asset.id} · updated ${fmtWhen(asset.updated_at)} by ${asset.updated_by}` : undefined}
      footer={
        asset && (
          <>
            {me.publish && (
              <Button variant="down-outline" size="sm" onClick={del} disabled={!!busy} data-testid="delete-asset">
                <Trash2 /> Delete
              </Button>
            )}
            <Button variant="ghost" size="sm" onClick={copy}>
              <Copy /> Copy URL
            </Button>
            {me.write && (
              <Button variant="ember" size="sm" onClick={save} disabled={!!busy || (!altDirty && !nameDirty)} data-testid="save-asset">
                <Save /> Save
              </Button>
            )}
          </>
        )
      }
    >
      {confirmDialog}
      {d.error ? (
        <EmptyState illustration="satellite_antenna" title="Couldn't load this image" text={d.error.message} action={<Button variant="surface" onClick={d.reload}><RotateCw /> Try again</Button>} />
      ) : !asset ? (
        <Skeleton className="h-[420px] w-full rounded-[16px]" />
      ) : (
        <div className="space-y-5" data-testid="asset-drawer">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <div>
              <ThemeTile theme="light" file={asset.light} />
              {me.write && (
                <div className="mt-1.5">
                  <FileButton onFiles={(f) => replace("light", f)} size="xs" disabled={!canFile || !!busy} testId="replace-light">
                    <Upload /> {busy === "light" ? "Uploading…" : "Replace"}
                  </FileButton>
                </div>
              )}
            </div>
            <div>
              {asset.dark ? <ThemeTile theme="dark" file={asset.dark} /> : <ThemeTile theme="dark" file={null} fallback={asset.light.variants.md?.url ?? asset.light.url} />}
              {!asset.dark && <p className="mt-1 text-[11px] text-fg-3">No dark file: the light image is used in dark mode.</p>}
              {me.write && (
                <div className="mt-1.5 flex flex-wrap gap-1.5">
                  <FileButton onFiles={(f) => replace("dark", f)} size="xs" disabled={!canFile || !!busy} testId="upload-dark">
                    <Moon /> {busy === "dark" ? "Uploading…" : asset.dark ? "Replace dark" : "Upload dark file"}
                  </FileButton>
                  {asset.dark && (
                    <Button size="xs" variant="ghost" onClick={removeDark} disabled={!canFile || !!busy} data-testid="remove-dark">
                      Remove dark
                    </Button>
                  )}
                </div>
              )}
            </div>
          </div>
          {me.write && live && !me.publish && <p className="text-[12px] text-warn">This image is on a live page: replacing its files needs the publish permission.</p>}

          <label className="block">
            <span className="mb-1.5 block text-[12.5px] font-medium text-fg-2">Name</span>
            <input value={name} disabled={!me.write} onChange={(e) => setName(e.target.value)} className={inputCls} data-testid="asset-name" />
          </label>

          <div>
            <div className="mb-1.5 flex items-center justify-between gap-2">
              <span className="text-[12.5px] font-medium text-fg-2">Alt text</span>
              <button type="button" onClick={() => setAllLocales((x) => !x)} className="text-[11.5px] text-fg-3 hover:text-fg">
                {allLocales ? "Only languages with text" : `All 22 languages`}
              </button>
            </div>
            <div className="space-y-1.5">
              {locales.map((l) => (
                <label key={l} className="flex items-center gap-2">
                  <span className="w-9 shrink-0 font-mono text-[11.5px] uppercase text-fg-3" title={LOCALE_NAME[l]}>
                    {l}
                  </span>
                  <input dir={dirOf(l)} value={alt[l] ?? ""} disabled={!me.write} maxLength={300} placeholder={l === "en" ? "Describe the image for screen readers" : `${LOCALE_NAME[l]} (English is used when empty)`} onChange={(e) => setAlt((a) => ({ ...a, [l]: e.target.value }))} className={cn(inputCls, "py-1.5")} data-testid={`alt-${l}`} />
                </label>
              ))}
            </div>
          </div>

          <div>
            <div className="mb-1.5 text-[12.5px] font-medium text-fg-2">Used in</div>
            <UsageList usage={usage} />
            {blocked && (
              <div className="mt-2 rounded-[12px] border border-down/40 bg-down-soft px-3 py-2 text-[12px] text-down">
                Can't delete: used in {blocked.length} slot{blocked.length === 1 ? "" : "s"}. Choose another image there first.
              </div>
            )}
          </div>

          <div className="space-y-3">
            <Variants file={asset.light} label="Light · sizes" />
            {asset.dark && <Variants file={asset.dark} label="Dark · sizes" />}
          </div>
        </div>
      )}
    </Dialog>
  );
}

export function LiveMediaLibrary() {
  const { me } = useContentMe();
  const [q, setQ] = React.useState("");
  const [debounced, setDebounced] = React.useState("");
  const [limit, setLimit] = React.useState(PAGE);
  const [open, setOpen] = React.useState<string | null>(null);
  React.useEffect(() => {
    const t = setTimeout(() => {
      setDebounced(q.trim());
      setLimit(PAGE);
    }, 250);
    return () => clearTimeout(t);
  }, [q]);
  React.useEffect(() => {
    const a = new URLSearchParams(window.location.search).get("asset");
    if (a && /^m[0-9a-f]{18}$/.test(a)) setOpen(a);
  }, []);
  const list = useCnt<{ assets: Asset[]; total: number }>(me?.read ? `media?limit=${Math.min(limit, 200)}${debounced ? `&q=${encodeURIComponent(debounced)}` : ""}` : null);
  const reload = list.reload;
  const { jobs, upload, clear } = useUploader(React.useCallback(() => reload(), [reload]));

  if (!me) return <Skeleton className="h-[640px] w-full rounded-[18px]" />;
  if (!me.read) return <EmptyState illustration="identification_card" title="No access" text="Your role doesn't include site content." />;

  const assets = list.data?.assets ?? [];
  return (
    <div className="pb-16">
      <PageHeader
        title="Media library"
        subtitle="Images used on the website, the Client Area, Kalks Trader and in emails. Each image can have a dark-mode file and alt text in every language; resized WebP versions are made automatically."
        actions={
          me.write && (
            <FileButton onFiles={upload} multiple variant="ember" size="md" testId="upload">
              <Upload /> Upload images
            </FileButton>
          )
        }
      />

      <Reveal>
        <Card>
          <div className="flex flex-wrap items-center justify-between gap-3 px-4 pt-5 sm:px-6">
            <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
              <Search className="size-3.5 text-fg-3" />
              <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search by name…" className="w-52 bg-transparent text-[13px] outline-none placeholder:text-fg-3" data-testid="media-search" />
            </div>
            <span className="k-num text-[12px] text-fg-3">{list.data ? `${list.data.total} image${list.data.total === 1 ? "" : "s"}` : "Loading"}</span>
          </div>
          <div className="space-y-3 px-4 pb-6 pt-4 sm:px-6">
            <UploadJobs jobs={jobs} onClear={clear} />
            <DropZone onFiles={upload} disabled={!me.write} className="p-3">
              {me.write && <p className="pb-3 text-center text-[12px] text-fg-3">Drop PNG, JPEG, WebP or SVG images here (up to 10 MB each).</p>}
              {list.error ? (
                <EmptyState illustration="satellite_antenna" title="The content service is unavailable" text={list.error.message} action={<Button variant="surface" onClick={list.reload}><RotateCw /> Try again</Button>} />
              ) : !list.data ? (
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6">
                  {Array.from({ length: 12 }, (_, i) => (
                    <Skeleton key={i} className="aspect-[4/3] rounded-[14px]" />
                  ))}
                </div>
              ) : assets.length === 0 ? (
                <EmptyState illustration="package" title={debounced ? "No images match" : "No images yet"} text={debounced ? "Try another name." : me.write ? "Upload images or drop them here." : "Nothing has been uploaded yet."} />
              ) : (
                <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4 xl:grid-cols-6" data-testid="media-grid">
                  {assets.map((a) => (
                    <button key={a.id} type="button" onClick={() => setOpen(a.id)} className="group overflow-hidden rounded-[14px] border border-line bg-surface-2/40 text-left transition-colors hover:border-[var(--k-border-top)]" data-testid={`asset-${a.id}`}>
                      <div className="relative aspect-[4/3] bg-surface-3">
                        {/* eslint-disable-next-line @next/next/no-img-element */}
                        <img src={thumbOf(a.light)} alt={a.alt.en ?? ""} loading="lazy" className="size-full object-cover transition-transform duration-500 group-hover:scale-[1.03]" />
                        {a.dark && (
                          <span className="absolute end-2 top-2 inline-flex items-center gap-1 rounded-full bg-black/60 px-1.5 py-0.5 text-[10px] text-white/85 backdrop-blur" title="Has a dark-mode file">
                            <Moon className="size-3" /> Dark
                          </span>
                        )}
                      </div>
                      <div className="px-2.5 py-2">
                        <div className="truncate text-[12.5px] font-medium">{a.name}</div>
                        <div className="k-num truncate text-[11px] text-fg-3">
                          {a.light.width && a.light.height ? `${a.light.width}×${a.light.height}` : "SVG"} · {fmtBytes(a.light.bytes)}
                        </div>
                      </div>
                    </button>
                  ))}
                </div>
              )}
              {list.data && list.data.total > assets.length && limit < 200 && (
                <div className="pt-4 text-center">
                  <Button variant="surface" size="sm" onClick={() => setLimit((l) => Math.min(200, l + PAGE))}>
                    Show more
                  </Button>
                </div>
              )}
            </DropZone>
          </div>
        </Card>
      </Reveal>

      <AssetDrawer id={open} me={me} onClose={() => setOpen(null)} onChanged={() => list.reload()} />
    </div>
  );
}

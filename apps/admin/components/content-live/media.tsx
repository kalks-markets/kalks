"use client";

// Media widgets shared by the Pages editor and the Media library: light / dark previews, the upload queue and
// the "Choose from library" dialog.

import * as React from "react";
import { Check, ImageOff, LoaderCircle, Moon, RotateCw, Search, Sun, Upload } from "lucide-react";
import { toast } from "sonner";
import { Button, Dialog, EmptyState, Progress, Skeleton, cn } from "@kalks/ui";
import { UPLOAD_ACCEPT, fmtBytes, thumbOf, uploadImage, uploadProblem, useCnt, type Asset, type MediaFile } from "./api";

/** A checkerboard-free neutral tile for one theme's file (or a hint when the theme has none). */
export function ThemeTile({ theme, file, fallback, empty, className }: { theme: "light" | "dark"; file: MediaFile | null | undefined; fallback?: string; empty?: React.ReactNode; className?: string }) {
  const src = file ? (file.variants.md?.url ?? file.url) : fallback;
  return (
    <figure className={cn("min-w-0", className)}>
      <div className={cn("relative grid aspect-[16/10] place-items-center overflow-hidden rounded-[14px] border border-line", theme === "dark" ? "bg-black" : "bg-white")}>
        {src ? (
          // eslint-disable-next-line @next/next/no-img-element
          <img src={src} alt="" className={cn("size-full object-contain", !file && "opacity-90")} loading="lazy" />
        ) : (
          <div className={cn("px-4 text-center text-[12px]", theme === "dark" ? "text-white/60" : "text-black/55")}>{empty ?? <ImageOff className="mx-auto size-5" />}</div>
        )}
        <span className={cn("absolute start-2 top-2 inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10.5px] font-medium backdrop-blur", theme === "dark" ? "bg-black/60 text-white/80" : "bg-white/80 text-black/70")}>
          {theme === "dark" ? <Moon className="size-3" /> : <Sun className="size-3" />}
          {theme === "dark" ? "Dark" : "Light"}
        </span>
      </div>
      {file && (
        <figcaption className="k-num mt-1 truncate text-[11px] text-fg-3">
          {file.width && file.height ? `${file.width}×${file.height}` : file.mime === "image/svg+xml" ? "SVG" : "–"} · {fmtBytes(file.bytes)}
        </figcaption>
      )}
    </figure>
  );
}

/** Light and dark side by side. With no dark file the light one shows in dark mode (said so on the tile). */
export function ThemePair({ asset, fallback }: { asset: Asset | null | undefined; fallback?: string }) {
  if (!asset)
    return (
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <ThemeTile theme="light" file={null} fallback={fallback} empty="No image" />
        <ThemeTile theme="dark" file={null} fallback={fallback} empty="No image" />
      </div>
    );
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <ThemeTile theme="light" file={asset.light} />
      {asset.dark ? (
        <ThemeTile theme="dark" file={asset.dark} />
      ) : (
        <div>
          <ThemeTile theme="dark" file={null} fallback={asset.light.variants.md?.url ?? asset.light.url} />
          <p className="mt-1 text-[11px] text-fg-3">No dark file: the light image is used in dark mode.</p>
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Upload queue                                                        */
/* ------------------------------------------------------------------ */

type Job = { id: number; name: string; pct: number; state: "up" | "done" | "error"; message?: string };

/** Uploads files as new library entries, one at a time, with progress rows and a toast per file. */
export function useUploader(onUploaded: (a: Asset, existing: boolean) => void) {
  const [jobs, setJobs] = React.useState<Job[]>([]);
  const seq = React.useRef(0);
  const upload = React.useCallback(
    async (files: FileList | File[]) => {
      for (const f of Array.from(files)) {
        const id = ++seq.current;
        const bad = uploadProblem(f);
        if (bad) {
          toast.error("Not uploaded", { description: bad });
          continue;
        }
        setJobs((j) => [...j, { id, name: f.name, pct: 0, state: "up" }]);
        try {
          const r = await uploadImage<{ asset: Asset; existing: boolean }>("media", f, (pct) => setJobs((j) => j.map((x) => (x.id === id ? { ...x, pct } : x))));
          setJobs((j) => j.map((x) => (x.id === id ? { ...x, pct: 100, state: "done" } : x)));
          toast.success(r.existing ? "Already in the library" : "Uploaded", { description: r.existing ? `${f.name} is the same image as “${r.asset.name}”.` : r.asset.name });
          onUploaded(r.asset, r.existing);
        } catch (e) {
          setJobs((j) => j.map((x) => (x.id === id ? { ...x, state: "error", message: (e as Error).message } : x)));
          toast.error(`Couldn't upload ${f.name}`, { description: (e as Error).message });
        }
      }
      // finished rows fade from the list after a moment; failed ones stay until the next batch
      setTimeout(() => setJobs((j) => j.filter((x) => x.state === "error")), 2500);
    },
    [onUploaded],
  );
  const clear = () => setJobs([]);
  return { jobs, upload, clear };
}

export function UploadJobs({ jobs, onClear }: { jobs: Job[]; onClear?: () => void }) {
  if (!jobs.length) return null;
  return (
    <div className="space-y-1.5" data-testid="upload-jobs">
      {jobs.map((j) => (
        <div key={j.id} className="k-row flex items-center gap-3 px-3 py-2 text-[12.5px]">
          {j.state === "up" ? <LoaderCircle className="size-4 shrink-0 animate-spin text-fg-3" /> : j.state === "done" ? <Check className="size-4 shrink-0 text-up" /> : <ImageOff className="size-4 shrink-0 text-down" />}
          <div className="min-w-0 flex-1">
            <div className="truncate">{j.name}</div>
            {j.state === "error" ? <div className="text-[11.5px] text-down">{j.message}</div> : <Progress value={j.pct} tone={j.state === "done" ? "up" : "accent"} className="mt-1" />}
          </div>
          <span className="k-num shrink-0 text-[11px] text-fg-3">{j.state === "error" ? "Failed" : `${j.pct}%`}</span>
        </div>
      ))}
      {onClear && jobs.some((j) => j.state === "error") && (
        <button type="button" onClick={onClear} className="text-[11.5px] text-fg-3 hover:text-fg">
          Clear failed uploads
        </button>
      )}
    </div>
  );
}

/** Drop zone + file button; `children` renders inside the zone. */
export function DropZone({ onFiles, disabled, children, className }: { onFiles: (f: FileList) => void; disabled?: boolean; children?: React.ReactNode; className?: string }) {
  const [over, setOver] = React.useState(false);
  return (
    <div
      onDragOver={(e) => {
        if (disabled) return;
        e.preventDefault();
        setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        if (disabled) return toast.error("Your role can't upload images.");
        if (e.dataTransfer.files.length) onFiles(e.dataTransfer.files);
      }}
      className={cn("rounded-[16px] border border-dashed border-line transition-colors", over && "border-ember/60 bg-ember-soft", className)}
      data-testid="drop-zone"
    >
      {children}
    </div>
  );
}

export function FileButton({ onFiles, multiple, children, variant = "surface", size = "sm", disabled, testId }: { onFiles: (f: FileList) => void; multiple?: boolean; children: React.ReactNode; variant?: "surface" | "ember" | "ghost"; size?: "xs" | "sm" | "md"; disabled?: boolean; testId?: string }) {
  const ref = React.useRef<HTMLInputElement>(null);
  return (
    <>
      <input
        ref={ref}
        type="file"
        accept={UPLOAD_ACCEPT}
        multiple={multiple}
        hidden
        data-testid={testId ? `${testId}-input` : undefined}
        onChange={(e) => {
          if (e.target.files?.length) onFiles(e.target.files);
          e.target.value = "";
        }}
      />
      <Button variant={variant} size={size} disabled={disabled} onClick={() => ref.current?.click()} data-testid={testId}>
        {children}
      </Button>
    </>
  );
}

/* ------------------------------------------------------------------ */
/* Choose from library                                                 */
/* ------------------------------------------------------------------ */

export function MediaPicker({ open, onOpenChange, onPick, canUpload, selected }: { open: boolean; onOpenChange: (o: boolean) => void; onPick: (a: Asset) => void; canUpload: boolean; selected?: string | null }) {
  const [q, setQ] = React.useState("");
  const [debounced, setDebounced] = React.useState("");
  React.useEffect(() => {
    const t = setTimeout(() => setDebounced(q.trim()), 250);
    return () => clearTimeout(t);
  }, [q]);
  const list = useCnt<{ assets: Asset[]; total: number }>(open ? `media?limit=120${debounced ? `&q=${encodeURIComponent(debounced)}` : ""}` : null);
  const { jobs, upload, clear } = useUploader(
    React.useCallback(
      (a: Asset) => {
        list.reload();
        onPick(a);
      },
      // eslint-disable-next-line react-hooks/exhaustive-deps
      [onPick],
    ),
  );
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      width={860}
      title="Choose from library"
      description="Pick an image, or upload one (PNG, JPEG, WebP or SVG up to 10 MB). Uploading picks it right away."
      footer={
        <>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          {canUpload && (
            <FileButton onFiles={upload} variant="ember" size="md" testId="picker-upload">
              <Upload /> Upload image
            </FileButton>
          )}
        </>
      }
    >
      <div className="space-y-3">
        <div className="flex h-9 items-center gap-2 rounded-full border border-line bg-surface-2 px-3.5">
          <Search className="size-3.5 text-fg-3" />
          <input value={q} onChange={(e) => setQ(e.target.value)} placeholder="Search by name…" className="w-full bg-transparent text-[13px] outline-none placeholder:text-fg-3" data-testid="picker-search" />
        </div>
        <UploadJobs jobs={jobs} onClear={clear} />
        <DropZone onFiles={upload} disabled={!canUpload} className="p-2">
          {list.error ? (
            <EmptyState
              illustration="satellite_antenna"
              title="Couldn't load the library"
              text={list.error.message}
              action={
                <Button variant="surface" onClick={list.reload}>
                  <RotateCw /> Try again
                </Button>
              }
            />
          ) : !list.data ? (
            <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
              {Array.from({ length: 8 }, (_, i) => (
                <Skeleton key={i} className="aspect-[4/3] rounded-[12px]" />
              ))}
            </div>
          ) : list.data.assets.length === 0 ? (
            <EmptyState illustration="package" title={debounced ? "No images match" : "The library is empty"} text={canUpload ? "Drop images here or use Upload image." : "Nothing has been uploaded yet."} />
          ) : (
            <div className="grid max-h-[52vh] grid-cols-2 gap-2 overflow-y-auto sm:grid-cols-4" data-testid="picker-grid">
              {list.data.assets.map((a) => (
                <button
                  key={a.id}
                  type="button"
                  onClick={() => onPick(a)}
                  className={cn("group overflow-hidden rounded-[12px] border text-left transition-colors", selected === a.id ? "border-ember" : "border-line hover:border-[var(--k-border-top)]")}
                >
                  {/* light and dark side by side (with no dark file the light one shows on black, as the apps do) */}
                  <div className="relative grid aspect-[4/3] grid-cols-2 bg-surface-3">
                    <div className="relative overflow-hidden bg-white">
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img src={thumbOf(a.light)} alt="" loading="lazy" className="size-full object-cover" />
                      <Sun className="absolute bottom-1 start-1 size-3 text-black/60" aria-label="Light" />
                    </div>
                    <div className="relative overflow-hidden bg-black">
                      {/* eslint-disable-next-line @next/next/no-img-element */}
                      <img src={thumbOf(a.dark ?? a.light)} alt="" loading="lazy" className={cn("size-full object-cover", !a.dark && "opacity-80")} />
                      <Moon className="absolute bottom-1 start-1 size-3 text-white/70" aria-label={a.dark ? "Dark file" : "No dark file: light image"} />
                    </div>
                    {selected === a.id && <span className="absolute start-1.5 top-1.5 rounded-full bg-ember px-1.5 text-[10px] font-medium text-white">Current</span>}
                  </div>
                  <div className="truncate px-2 py-1.5 text-[11.5px] text-fg-2">{a.name}</div>
                </button>
              ))}
            </div>
          )}
        </DropZone>
      </div>
    </Dialog>
  );
}

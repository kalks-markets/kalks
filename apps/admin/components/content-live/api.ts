"use client";

// Browser client for the Back Office content BFF (/api/content/*, see app/api/content/[...path]/route.ts and
// lib/content-routes.ts). Shapes follow services/content (src/content.rs, src/media.rs).

import * as React from "react";

export type Surface = "website" | "app" | "trader" | "email";
export type Kind = "text" | "rich_text" | "image" | "link" | "placeholder";

export const SURFACE_LABEL: Record<Surface, string> = { website: "Website", app: "Client Area", trader: "Kalks Trader", email: "Email" };

/** The 22 platform locales, English first (the fallback). */
export const LOCALES = ["en", "hi", "ar", "ur", "fa", "es", "pt", "fr", "de", "it", "ru", "tr", "id", "ms", "vi", "th", "zh", "ja", "ko", "bn", "ta", "sw"] as const;
export type Locale = (typeof LOCALES)[number];
export const LOCALE_NAME: Record<Locale, string> = {
  en: "English",
  hi: "Hindi",
  ar: "Arabic",
  ur: "Urdu",
  fa: "Persian",
  es: "Spanish",
  pt: "Portuguese",
  fr: "French",
  de: "German",
  it: "Italian",
  ru: "Russian",
  tr: "Turkish",
  id: "Indonesian",
  ms: "Malay",
  vi: "Vietnamese",
  th: "Thai",
  zh: "Chinese",
  ja: "Japanese",
  ko: "Korean",
  bn: "Bengali",
  ta: "Tamil",
  sw: "Swahili",
};
export const RTL = new Set<string>(["ar", "ur", "fa"]);
export const dirOf = (l: string) => (RTL.has(l) ? "rtl" : "ltr");

export type Value = { text?: string; markdown?: string; label?: string; href?: string; media?: string };

export type MediaFile = {
  id: string;
  mime: string;
  bytes: number;
  width: number | null;
  height: number | null;
  url: string;
  srcset: string;
  variants: Partial<Record<"sm" | "md" | "lg", { url: string; width: number; height: number | null; bytes: number | null }>>;
};
export type Asset = { id: string; name: string; alt: Record<string, string>; light: MediaFile; dark: MediaFile | null; created_by: string; created_at: string; updated_by: string; updated_at: string };
export type Usage = { surface: Surface; page: string; slot: string; locale: string; published: boolean; draft: boolean };

export type LocaleState = { draft: boolean; published: boolean; version: number; draft_at: string | null; published_at: string | null };
export type TreeSlot = { slot: string; kind: Kind; label: string; locales: Record<string, LocaleState> };
export type Tree = { tenant: string; pages: { surface: Surface; page: string; slots: TreeSlot[] }[] };

export type Entry = { draft: Value | null; draft_by: string | null; draft_at: string | null; published: Value | null; version: number; published_by: string | null; published_at: string | null };
export type SlotView = { slot: { surface: Surface; page: string; slot: string; kind: Kind; label: string }; entries: Record<string, Entry>; media: Record<string, Asset> };
export type Version = { version: number; action: "publish" | "unpublish" | "rollback" | string; from_version: number | null; value: Value | null; by: string; at: string };

export type Me = { read: boolean; write: boolean; publish: boolean; sites: Partial<Record<Surface, string>> };

export class ContentError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public detail?: { usage?: Usage[] } & Record<string, unknown>,
  ) {
    super(message);
  }
}

export async function cnt<T>(path: string, init?: { method?: "GET" | "POST" | "PUT" | "DELETE"; body?: unknown; signal?: AbortSignal }): Promise<T> {
  const method = init?.method ?? (init?.body !== undefined ? "POST" : "GET");
  let res: Response;
  try {
    res = await fetch(`/api/content/${path}`, {
      method,
      headers: init?.body !== undefined ? { "content-type": "application/json" } : undefined,
      body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
      cache: "no-store",
      signal: init?.signal,
    });
  } catch (e) {
    if ((e as Error).name === "AbortError") throw e;
    throw new ContentError(0, "network", "Network error. Check your connection and try again.");
  }
  const data = (await res.json().catch(() => ({}))) as { error?: { code?: string; message?: string; detail?: Record<string, unknown> } };
  if (!res.ok) throw new ContentError(res.status, data.error?.code ?? "error", data.error?.message ?? "Something went wrong.", data.error?.detail as ContentError["detail"]);
  return data as T;
}

export function useCnt<T>(path: string | null) {
  const [data, setData] = React.useState<T | null>(null);
  const [error, setError] = React.useState<ContentError | null>(null);
  const [tick, setTick] = React.useState(0);
  const reload = React.useCallback(() => setTick((t) => t + 1), []);
  React.useEffect(() => {
    if (!path) return;
    const ctl = new AbortController();
    cnt<T>(path, { signal: ctl.signal })
      .then((d) => {
        setData(d);
        setError(null);
      })
      .catch((e) => {
        if ((e as Error).name !== "AbortError") setError(e instanceof ContentError ? e : new ContentError(0, "error", "Something went wrong."));
      });
    return () => ctl.abort();
  }, [path, tick]);
  return { data, error, reload, setData };
}

const NO_ACCESS: Me = { read: false, write: false, publish: false, sites: {} };

/** The staff member's content permissions (and the consumer sites, for previews and default images). */
export function useContentMe() {
  const { data, error } = useCnt<Me>("me");
  return { me: data ?? (error ? NO_ACCESS : null), error };
}

export const UPLOAD_MAX = 10 * 1024 * 1024;
export const UPLOAD_ACCEPT = "image/png,image/jpeg,image/webp,image/svg+xml";

/** A client-side check before uploading (the service sniffs and validates again). */
export function uploadProblem(f: File): string | null {
  if (f.size > UPLOAD_MAX) return `${f.name} is ${fmtBytes(f.size)}; images can be up to 10 MB.`;
  if (f.type && !UPLOAD_ACCEPT.split(",").includes(f.type)) return `${f.name} isn't a PNG, JPEG, WebP or SVG image.`;
  if (f.size === 0) return `${f.name} is empty.`;
  return null;
}

/** Raw image upload with progress: a new library entry (`media`) or a file of one (`media/{id}/file?theme=`). */
export function uploadImage<T>(path: string, file: File, onProgress?: (pct: number) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    const x = new XMLHttpRequest();
    x.open("POST", `/api/content/${path}`);
    x.setRequestHeader("content-type", file.type || "application/octet-stream");
    x.setRequestHeader("x-file-name", encodeURIComponent(file.name));
    x.upload.onprogress = (e) => e.lengthComputable && onProgress?.(Math.round((e.loaded / e.total) * 100));
    x.onerror = () => reject(new ContentError(0, "network", "Network error. Check your connection and try again."));
    x.onload = () => {
      let data: { error?: { code?: string; message?: string } } = {};
      try {
        data = JSON.parse(x.responseText);
      } catch {
        /* empty */
      }
      if (x.status >= 200 && x.status < 300) resolve(data as T);
      else reject(new ContentError(x.status, data.error?.code ?? "error", data.error?.message ?? (x.status === 413 ? "Images can be up to 10 MB." : "Upload failed.")));
    };
    x.send(file);
  });
}

export const fmtWhen = (iso: string | null | undefined) => (iso ? new Date(iso).toLocaleString("en-GB", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" }) : "–");
export const fmtBytes = (n: number | null | undefined) => (n == null ? "–" : n < 1024 ? `${n} B` : n < 1024 * 1024 ? `${(n / 1024).toFixed(0)} KB` : `${(n / 1024 / 1024).toFixed(1)} MB`);
export const thumbOf = (f: MediaFile) => f.variants.sm?.url ?? f.variants.md?.url ?? f.url;

/** One-line summary of a value (history rows, live value hints). */
export function summarize(kind: Kind, v: Value | null | undefined, media?: Record<string, Asset>): string {
  if (!v) return "Empty (built-in default)";
  if (kind === "image") return v.media ? (media?.[v.media]?.name ?? v.media) : "–";
  if (kind === "link") return `${v.label ?? ""} → ${v.href ?? ""}`;
  const t = (kind === "rich_text" ? v.markdown : v.text) ?? "";
  return t ? (t.length > 120 ? `${t.slice(0, 120)}…` : t) : "Empty text";
}

/** Open a link in a new tab from an async action without the popup blocker: open first, navigate later. */
export async function openInNewTab(get: () => Promise<string>): Promise<void> {
  const w = window.open("about:blank", "_blank");
  if (w) w.opener = null;
  try {
    const url = await get();
    if (w) w.location.href = url;
    else window.open(url, "_blank", "noopener");
  } catch (e) {
    w?.close();
    throw e;
  }
}

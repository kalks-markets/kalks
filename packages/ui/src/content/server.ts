// Server-only: published content of a surface from the content service (services/content, public read
// GET /v1/public/content). Cached per (service, tenant, surface, locale) for a minute in the server process, with
// concurrent loads of the same key sharing one request (like the apps' tenantConfig). Never throws: an unreachable
// service gives the last good copy, else empty pages, and every slot then shows the app's built-in default.
// Previews (a valid preview token) bypass the cache and return drafts.

import { sanitizeContent, type ContentBundle, type ContentSurface } from "./types";

// packages/ui has no Node types; only this server module reads the environment
declare const process: { env: Record<string, string | undefined> };

const TTL_MS = 60_000;
const MAX_KEYS = 500;
const cache = new Map<string, { at: number; bundle: ContentBundle | null }>();
const inflight = new Map<string, Promise<ContentBundle | null>>();

/** Service base URL (CONTENT_URL, default the local service). */
export const contentServiceUrl = () => (process.env.CONTENT_URL || "http://127.0.0.1:8105").replace(/\/+$/, "");

/** Name of the cookie a preview link sets (each app's /api/content-preview route). */
export const CONTENT_PREVIEW_COOKIE = "kalks_content_preview";

export type ContentQuery = {
  surface: ContentSurface;
  /** broker slug (tenant-config); "kalks" when unknown */
  tenant?: string | null;
  locale: string;
  /** a preview token from the preview cookie: drafts, never cached */
  preview?: string | null;
  timeoutMs?: number;
};

async function load(q: ContentQuery, tenant: string): Promise<ContentBundle | null> {
  const p = new URLSearchParams({ surface: q.surface, locale: q.locale, tenant });
  if (q.preview) p.set("preview", q.preview);
  try {
    const res = await fetch(`${contentServiceUrl()}/v1/public/content?${p}`, { cache: "no-store", signal: AbortSignal.timeout(q.timeoutMs ?? 2500) });
    if (!res.ok) return null;
    const body = (await res.json()) as { locale?: unknown; preview?: unknown };
    return { surface: q.surface, locale: typeof body.locale === "string" ? body.locale : q.locale, preview: body.preview === true, pages: sanitizeContent(body) };
  } catch {
    return null;
  }
}

/** Every published page of a surface for one locale (missing translations already fall back to English). */
export async function fetchContent(q: ContentQuery): Promise<ContentBundle> {
  const tenant = q.tenant && /^[a-z0-9_-]{1,64}$/.test(q.tenant) ? q.tenant : "kalks";
  const empty: ContentBundle = { surface: q.surface, locale: q.locale, preview: false, pages: {} };
  if (q.preview) {
    // an expired preview link falls back to what clients see
    return (await load(q, tenant)) ?? (await fetchContent({ ...q, preview: null }));
  }
  const key = `${contentServiceUrl()}|${tenant}|${q.surface}|${q.locale}`;
  const hit = cache.get(key);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.bundle ?? empty;
  const pending = inflight.get(key);
  if (pending) return (await pending) ?? empty;
  const run = (async () => {
    const b = await load(q, tenant);
    if (cache.size >= MAX_KEYS) cache.clear();
    // keep serving the last good copy while the service is down (retried after the TTL)
    cache.set(key, { at: Date.now(), bundle: b ?? hit?.bundle ?? null });
    return cache.get(key)!.bundle;
  })().finally(() => inflight.delete(key));
  inflight.set(key, run);
  return (await run) ?? empty;
}

/** Validates a preview link's `path` (same-site relative path only) for the apps' /api/content-preview routes. */
export function safePreviewPath(path: string | null | undefined): string {
  if (!path || !path.startsWith("/") || path.startsWith("//") || path.includes("\\") || path.length > 500) return "/";
  return path;
}

/** Preview tokens are `<base64url>.<base64url>`; anything else is ignored. */
export function isPreviewToken(t: string | null | undefined): t is string {
  return !!t && t.length < 400 && /^[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+$/.test(t);
}

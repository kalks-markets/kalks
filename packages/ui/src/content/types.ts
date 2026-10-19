// Published content from the content service (services/content, GET /v1/public/content) as the apps hold it, and the
// sanitiser that turns a service response into it. Pure: shared by the server fetch (content/server.ts) and the client
// hooks (content/react.tsx). Slot catalogue: docs/CONTENT.md.

export type ContentSurface = "website" | "app" | "trader" | "email";

export type ContentImageFile = { url: string; srcset: string; width: number | null; height: number | null };
export type ContentImage = ContentImageFile & { media: string; alt: string; dark: ContentImageFile | null };
export type ContentLink = { label: string; href: string };

/** One slot's published (or, in preview, draft) value. */
export type ContentSlot =
  | { kind: "text" | "placeholder"; locale: string; text: string }
  | { kind: "rich_text"; locale: string; markdown: string }
  | { kind: "link"; locale: string; link: ContentLink }
  | { kind: "image"; locale: string; image: ContentImage };

/** page → slot → value. */
export type ContentPages = Record<string, Record<string, ContentSlot>>;

export type ContentBundle = { surface: ContentSurface; locale: string; preview: boolean; pages: ContentPages };

const KEY = /^[a-z0-9][a-z0-9_.-]{0,95}$/;

function str(v: unknown, max: number): string | null {
  if (typeof v !== "string") return null;
  // control characters out (newlines stay: texts can be multi-line)
  const s = v.replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, "").trim();
  return s.length > max ? s.slice(0, max) : s;
}

function httpUrl(v: unknown): string | null {
  if (typeof v !== "string" || v.length > 2000) return null;
  try {
    const u = new URL(v);
    return u.protocol === "https:" || u.protocol === "http:" ? u.toString() : null;
  } catch {
    return null;
  }
}

/** A link target an app may render: same-site path, #anchor, http(s) or mailto. */
export function safeHref(v: unknown): string | null {
  if (typeof v !== "string" || v.length > 1000 || /\s/.test(v)) return null;
  if ((v.startsWith("/") && !v.startsWith("//")) || v.startsWith("#")) return v;
  if (/^mailto:/i.test(v)) return v;
  return httpUrl(v);
}

/** srcset entries whose URLs are http(s) ("<url> <n>w", comma-separated). */
function srcset(v: unknown): string {
  if (typeof v !== "string") return "";
  return v
    .split(",")
    .map((p) => p.trim().split(/\s+/))
    .filter((p) => p.length === 2 && httpUrl(p[0]) && /^\d{1,5}w$/.test(p[1]!))
    .map((p) => p.join(" "))
    .join(", ");
}

const dim = (v: unknown) => (typeof v === "number" && Number.isFinite(v) && v > 0 && v < 100_000 ? Math.round(v) : null);

function file(v: unknown): ContentImageFile | null {
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  const url = httpUrl(o.url);
  return url ? { url, srcset: srcset(o.srcset), width: dim(o.width), height: dim(o.height) } : null;
}

function slot(raw: unknown): ContentSlot | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const locale = typeof r.locale === "string" && /^[a-z]{2}$/.test(r.locale) ? r.locale : "en";
  const v = (r.value && typeof r.value === "object" ? r.value : {}) as Record<string, unknown>;
  switch (r.kind) {
    case "text":
    case "placeholder": {
      const text = str(v.text, r.kind === "text" ? 2000 : 200);
      return text === null ? null : { kind: r.kind, locale, text };
    }
    case "rich_text": {
      const markdown = str(v.markdown, 20_000);
      return markdown === null ? null : { kind: "rich_text", locale, markdown };
    }
    case "link": {
      const label = str(v.label, 200);
      const href = safeHref(v.href);
      return label === null || !href ? null : { kind: "link", locale, link: { label, href } };
    }
    case "image": {
      const light = file(v);
      if (!light || typeof v.media !== "string") return null;
      return { kind: "image", locale, image: { ...light, media: v.media.slice(0, 40), alt: str(v.alt, 300) ?? "", dark: file(v.dark) } };
    }
    default:
      return null;
  }
}

/** The service's response → ContentPages (anything unexpected is dropped, never thrown). */
export function sanitizeContent(body: unknown): ContentPages {
  const out: ContentPages = {};
  const pages = body && typeof body === "object" ? (body as { pages?: unknown }).pages : null;
  if (!pages || typeof pages !== "object") return out;
  for (const [page, slots] of Object.entries(pages as Record<string, unknown>)) {
    if (!KEY.test(page) || !slots || typeof slots !== "object") continue;
    const p: Record<string, ContentSlot> = {};
    for (const [key, raw] of Object.entries(slots as Record<string, unknown>)) {
      const s = KEY.test(key) ? slot(raw) : null;
      if (s) p[key] = s;
    }
    out[page] = p;
  }
  return out;
}

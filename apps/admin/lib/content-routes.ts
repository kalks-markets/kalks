// Route table of the content BFF (app/api/content/[...path]/route.ts), kept pure (no imports) so it can be
// tested on its own. Maps a browser request onto a services/content path plus the permission it needs.
//
//   GET    me                                       {read, write, publish, sites}  (BFF only)
//   GET    meta · audit                             read
//   GET    tree?surface                             read
//   GET    slot?surface&page&slot                   read
//   GET    versions?surface&page&slot&locale        read
//   PUT    entry                                    write    (draft)
//   POST   discard                                  write
//   POST   publish {surface, page, slot?, locale?}  publish
//   POST   unpublish · rollback                     publish
//   POST   import                                   write (publish when body.publish; ≤ 2 MB)
//   POST   preview-token {surface}                  read
//   POST   preview {surface, page}                  read     → {url} (BFF builds the consumer app's preview link)
//   DELETE slot?surface&page&slot                   publish
//   GET    media?q&limit&offset                     read
//   POST   media                                    write    (raw image, X-File-Name, ≤ 10 MB)
//   GET    media/{id}                               read
//   PUT    media/{id}                               write    {name?, alt?}
//   DELETE media/{id}                               publish
//   POST   media/{id}/file?theme=light|dark         write    (raw image; the service asks for publish when the image is live)
//   DELETE media/{id}/file?theme=dark               write    (same)

export type Perm = "content.read" | "content.write" | "content.publish";
export type Method = "GET" | "POST" | "PUT" | "DELETE";
export type Route =
  | { kind: "me" }
  | { kind: "preview"; perm: "content.read" }
  /** `body`: none (GET / DELETE), json (≤ maxBytes), upload (raw image ≤ maxBytes). `publishIf`: a JSON body flag that raises the permission to content.publish. */
  | { kind: "proxy"; perm: Perm; target: string; body: "none" | "json" | "upload"; maxBytes?: number; publishIf?: string };

export const SURFACES = ["website", "app", "trader", "email"] as const;
export const LOCALES = ["en", "hi", "ar", "ur", "fa", "es", "pt", "fr", "de", "it", "ru", "tr", "id", "ms", "vi", "th", "zh", "ja", "ko", "bn", "ta", "sw"] as const;
export const JSON_MAX = 300 * 1024;
export const IMPORT_MAX = 2 * 1024 * 1024;
export const UPLOAD_MAX = 10 * 1024 * 1024;

export const isPage = (s: string) => /^[a-z0-9][a-z0-9_-]{0,63}$/.test(s);
export const isSlot = (s: string) => /^[a-z0-9][a-z0-9._-]{0,95}$/.test(s);
export const isAsset = (s: string) => /^m[0-9a-f]{18}$/.test(s);
const isSurface = (s: string) => (SURFACES as readonly string[]).includes(s);
const isLocale = (s: string) => (LOCALES as readonly string[]).includes(s);

type Check = (v: string) => boolean;
const CHECKS: Record<string, Check> = {
  surface: isSurface,
  page: isPage,
  slot: isSlot,
  locale: isLocale,
  theme: (v) => v === "light" || v === "dark",
  q: (v) => v.length <= 80,
  limit: (v) => /^\d{1,3}$/.test(v),
  offset: (v) => /^\d{1,6}$/.test(v),
};

/**
 * Query string with only `keys` (each validated); `required` keys must be present. Returns null when a value is
 * invalid or a required key is missing (the route answers 422 / 404).
 */
export function pickQuery(sp: URLSearchParams, keys: string[], required: string[] = []): string | null {
  const out = new URLSearchParams();
  for (const k of keys) {
    const v = sp.get(k);
    if (v === null || v === "") {
      if (required.includes(k)) return null;
      continue;
    }
    if (!CHECKS[k]!(v)) return null;
    out.set(k, v);
  }
  const s = out.toString();
  return s ? `?${s}` : "";
}

/** "bad" = known route with invalid parameters (422), null = unknown route (404). */
export function matchRoute(method: Method, path: string[], sp: URLSearchParams): Route | "bad" | null {
  const key = path.join("/");
  const C = "/v1/admin/content";
  const q = (keys: string[], required: string[] = keys) => pickQuery(sp, keys, required);
  const proxy = (perm: Perm, target: string | null, body: "none" | "json" | "upload" = "none", extra: { maxBytes?: number; publishIf?: string } = {}): Route | "bad" =>
    target === null ? "bad" : { kind: "proxy", perm, target, body, ...extra };
  const qs = (base: string, keys: string[], required?: string[]) => {
    const s = q(keys, required);
    return s === null ? null : base + s;
  };

  if (method === "GET") {
    if (key === "me") return { kind: "me" };
    if (key === "meta" || key === "audit") return proxy("content.read", `${C}/${key}`);
    if (key === "tree") return proxy("content.read", qs(`${C}/tree`, ["surface"], []));
    if (key === "slot") return proxy("content.read", qs(`${C}/slot`, ["surface", "page", "slot"]));
    if (key === "versions") return proxy("content.read", qs(`${C}/versions`, ["surface", "page", "slot", "locale"]));
    if (key === "media") return proxy("content.read", qs("/v1/admin/media", ["q", "limit", "offset"], []));
    if (path.length === 2 && path[0] === "media") return isAsset(path[1]!) ? proxy("content.read", `/v1/admin/media/${path[1]}`) : "bad";
    return null;
  }
  if (method === "PUT") {
    if (key === "entry") return proxy("content.write", `${C}/entry`, "json", { maxBytes: JSON_MAX });
    if (path.length === 2 && path[0] === "media") return isAsset(path[1]!) ? proxy("content.write", `/v1/admin/media/${path[1]}`, "json", { maxBytes: JSON_MAX }) : "bad";
    return null;
  }
  if (method === "POST") {
    if (key === "discard") return proxy("content.write", `${C}/discard`, "json", { maxBytes: JSON_MAX });
    if (key === "publish" || key === "unpublish" || key === "rollback") return proxy("content.publish", `${C}/${key}`, "json", { maxBytes: JSON_MAX });
    if (key === "import") return proxy("content.write", `${C}/import`, "json", { maxBytes: IMPORT_MAX, publishIf: "publish" });
    if (key === "preview-token") return proxy("content.read", `${C}/preview-token`, "json", { maxBytes: JSON_MAX });
    if (key === "preview") return { kind: "preview", perm: "content.read" };
    if (key === "media") return proxy("content.write", "/v1/admin/media", "upload", { maxBytes: UPLOAD_MAX });
    if (path.length === 3 && path[0] === "media" && path[2] === "file") {
      if (!isAsset(path[1]!)) return "bad";
      return proxy("content.write", qs(`/v1/admin/media/${path[1]}/file`, ["theme"]), "upload", { maxBytes: UPLOAD_MAX });
    }
    return null;
  }
  if (method === "DELETE") {
    if (key === "slot") return proxy("content.publish", qs(`${C}/slot`, ["surface", "page", "slot"]));
    if (path.length === 2 && path[0] === "media") return isAsset(path[1]!) ? proxy("content.publish", `/v1/admin/media/${path[1]}`) : "bad";
    if (path.length === 3 && path[0] === "media" && path[2] === "file") {
      if (!isAsset(path[1]!) || sp.get("theme") !== "dark") return "bad";
      return proxy("content.write", `/v1/admin/media/${path[1]}/file?theme=dark`);
    }
    return null;
  }
  return null;
}

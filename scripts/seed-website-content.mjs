#!/usr/bin/env node
// Imports the website's current bundled copy and images into the content service (surface "website"), under exactly
// the page / slot keys the website reads, so the Back Office starts from what the site shows today.
//
// WHERE THE COPY COMES FROM
//   The website keeps its slot catalogue in apps/trader/src/content/manifest.ts (buildManifest(): every slot with its
//   default, built from src/content/* and the screenshot lists). This script imports that TypeScript module directly:
//   Node 24 strips the types natively, and a resolve hook (module.registerHooks) maps the website's "@/…" alias to
//   apps/trader/src and adds the ".ts" extension to extensionless imports. The manifest's module graph is kept free
//   of JSX, React and Next for this reason (the website's manifest.test.ts checks the keys and that every image file
//   exists). Nothing is duplicated here, so the seed can never drift from the keys the site reads.
//
// WHAT IT DOES
//   1. Images: uploads public/<src> of every image slot to the media library (POST /v1/admin/media; the service
//      dedupes identical bytes and returns the existing asset). A "<name>.dark.<ext>" file next to it becomes the
//      asset's dark file (POST /v1/admin/media/{id}/file?theme=dark) when the asset has none yet; a
//      "<name>.light.<ext>" file, when present, is used as the light file instead of "<name>.<ext>". English alt
//      text is set when the asset has none (PUT /v1/admin/media/{id}).
//   2. Content: POST /v1/admin/content/import in chunks of ≤ 500 entries (text / link / image values). Importing an
//      unchanged value is a no-op (the service clears a draft equal to the published value), so re-runs are safe.
//
// USAGE
//   node scripts/seed-website-content.mjs [website repo] [--drafts] [--dry-run] [--url URL] [--tenant SLUG]
//     website repo   default: $WEBSITE_DIR, else ../kalks-website next to the platform repo (on the VPS
//                    ~/kalks-website)
//     --drafts       save drafts only (publish them from the Back Office); default: publish at once
//     --publish      accepted for clarity (the default)
//     --dry-run      print what it would do, change nothing (no service needed)
//     --url          default $CONTENT_URL, else http://127.0.0.1:8105
//     --tenant       default $CONTENT_TENANT, else kalks
//   Token: $CONTENT_INTERNAL_TOKEN, else CONTENT_INTERNAL_TOKEN from the platform repo's .env.local.
//   The import runs as staff "seed@kalks" ($CONTENT_SEED_STAFF) with content.read / write / publish.
//
//   import { loadWebsiteManifest } from "./seed-website-content.mjs" gives the manifest to other scripts
//   (scripts/gen-website-catalogue.mjs).

import { existsSync, readFileSync } from "node:fs";
import { registerHooks } from "node:module";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

// the website's package.json has no "type": loading its .ts modules as ESM is expected, so don't warn about it
const emitWarning = process.emitWarning;
process.emitWarning = (w, ...rest) => (/Reparsing as ES module|MODULE_TYPELESS_PACKAGE_JSON/.test(String(w) + JSON.stringify(rest)) ? undefined : emitWarning.call(process, w, ...rest));

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");

/* ── website location and manifest ───────────────────────────────────────── */

/** the website repo: an explicit path, $WEBSITE_DIR, or the sibling clone of the platform repo (worktrees too) */
export function websiteDir(arg) {
  const cands = [arg, process.env.WEBSITE_DIR, path.resolve(REPO, "../kalks-website")];
  // inside a git worktree (…/kalks/.claude/worktrees/x) the sibling clone sits next to the main checkout
  const wt = REPO.split(`${path.sep}.claude${path.sep}worktrees${path.sep}`)[0];
  if (wt !== REPO) cands.push(path.resolve(wt, "../kalks-website"));
  cands.push(path.join(process.env.HOME || "", "kalks-website"));
  for (const c of cands.filter(Boolean)) if (existsSync(path.join(c, "apps/trader/src/content/manifest.ts"))) return path.resolve(c);
  throw new Error(`Website repo not found (tried ${cands.filter(Boolean).join(", ")}). Pass its path or set WEBSITE_DIR.`);
}

let hooked = null;
/** buildManifest() and PAGE_PATHS from the website's src/content/manifest.ts */
export async function loadWebsiteManifest(dir) {
  const src = path.join(dir, "apps/trader/src");
  if (!hooked) {
    hooked = src;
    const tryFile = (base) => {
      for (const ext of ["", ".ts", ".tsx", "/index.ts"]) if (existsSync(base + ext) && (ext || /\.[cm]?[jt]sx?$/.test(base))) return base + ext;
      return null;
    };
    registerHooks({
      resolve(specifier, context, next) {
        let base = null;
        if (specifier.startsWith("@/")) base = path.join(hooked, specifier.slice(2));
        else if ((specifier.startsWith("./") || specifier.startsWith("../")) && context.parentURL?.startsWith("file:") && fileURLToPath(context.parentURL).startsWith(hooked))
          base = path.resolve(path.dirname(fileURLToPath(context.parentURL)), specifier);
        const f = base && tryFile(base);
        return f ? { url: pathToFileURL(f).href, shortCircuit: true } : next(specifier, context);
      },
    });
  }
  const mod = await import(pathToFileURL(path.join(src, "content/manifest.ts")).href);
  return { entries: mod.buildManifest(), pagePaths: mod.PAGE_PATHS, pageLabels: mod.PAGE_LABELS, publicDir: path.join(dir, "apps/trader/public") };
}

/* ── service ──────────────────────────────────────────────────────────────── */

function token() {
  if (process.env.CONTENT_INTERNAL_TOKEN) return process.env.CONTENT_INTERNAL_TOKEN;
  // the platform repo's .env.local (a worktree has none: walk up to the main checkout)
  for (let d = REPO; d !== path.dirname(d); d = path.dirname(d)) {
    const f = path.join(d, ".env.local");
    if (!existsSync(f)) continue;
    const m = /^\s*CONTENT_INTERNAL_TOKEN\s*=\s*(.*)$/m.exec(readFileSync(f, "utf8"));
    if (m) return m[1].trim().replace(/^(['"])(.*)\1$/, "$2");
  }
  return "";
}

const MIME = { ".webp": "image/webp", ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".svg": "image/svg+xml" };

function client(url, tenant, tok) {
  const headers = {
    "x-kalks-internal": tok,
    "x-kalks-tenant": tenant,
    "x-kalks-staff": process.env.CONTENT_SEED_STAFF || "seed@kalks",
    "x-kalks-staff-perms": "content.read,content.write,content.publish",
  };
  return async (method, p, body, extra = {}) => {
    const res = await fetch(url + p, { method, headers: { ...headers, ...extra }, body, signal: AbortSignal.timeout(60_000) });
    const text = await res.text();
    let json = null;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      /* not JSON */
    }
    if (!res.ok) throw new Error(`${method} ${p}: ${res.status} ${json?.error?.message ?? text.slice(0, 200)}`);
    return json;
  };
}

/** the light and (optional) dark file of an image src under public/ */
function imageFiles(publicDir, src) {
  const file = path.join(publicDir, src.replace(/^\/+/, ""));
  const ext = path.extname(file);
  const stem = file.slice(0, -ext.length);
  const light = existsSync(`${stem}.light${ext}`) ? `${stem}.light${ext}` : file;
  const dark = existsSync(`${stem}.dark${ext}`) ? `${stem}.dark${ext}` : null;
  if (!existsSync(light)) throw new Error(`Image file missing: ${light}`);
  return { light, dark };
}

/* ── main ─────────────────────────────────────────────────────────────────── */

async function main() {
  const argv = process.argv.slice(2);
  const flag = (n) => argv.includes(n);
  const opt = (n) => {
    const i = argv.indexOf(n);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  const positional = argv.filter((a, i) => !a.startsWith("--") && !["--url", "--tenant"].includes(argv[i - 1]));
  const dry = flag("--dry-run");
  const publish = !flag("--drafts");
  const url = (opt("--url") || process.env.CONTENT_URL || "http://127.0.0.1:8105").replace(/\/+$/, "");
  const tenant = opt("--tenant") || process.env.CONTENT_TENANT || "kalks";

  const dir = websiteDir(positional[0]);
  const { entries, publicDir } = await loadWebsiteManifest(dir);
  const images = entries.filter((e) => e.kind === "image");
  console.log(`Website ${dir}: ${entries.length} slots on ${new Set(entries.map((e) => e.page)).size} pages (${images.length} images) → ${url} tenant ${tenant}${publish ? ", publishing" : ", drafts only"}${dry ? " [dry run]" : ""}`);

  const tok = token();
  if (!tok && !dry) console.warn("No CONTENT_INTERNAL_TOKEN (env or .env.local): only works against a service with the check disabled.");
  const api = client(url, tenant, tok);

  // 1. images: one asset per distinct src (heroes are reused by several pages)
  const assetBySrc = new Map();
  const stats = { uploaded: 0, existing: 0, dark: 0, alt: 0 };
  for (const e of images) {
    const { src, alt } = e.value;
    if (assetBySrc.has(src)) continue;
    const { light, dark } = imageFiles(publicDir, src);
    const name = path.basename(src, path.extname(src));
    if (dry) {
      console.log(`  image ${src}${dark ? " (+ dark)" : ""}  alt "${alt}"`);
      assetBySrc.set(src, "m_dry_run");
      continue;
    }
    const mime = MIME[path.extname(light).toLowerCase()] || "application/octet-stream";
    const up = await api("POST", `/v1/admin/media?name=${encodeURIComponent(name)}`, readFileSync(light), { "content-type": mime, "x-file-name": encodeURIComponent(path.basename(light)) });
    const asset = up.asset;
    stats[up.existing ? "existing" : "uploaded"]++;
    if (dark && !asset.dark) {
      await api("POST", `/v1/admin/media/${asset.id}/file?theme=dark`, readFileSync(dark), { "content-type": MIME[path.extname(dark).toLowerCase()] || mime, "x-file-name": encodeURIComponent(path.basename(dark)) });
      stats.dark++;
    }
    if (alt && !asset.alt?.en) {
      await api("PUT", `/v1/admin/media/${asset.id}`, JSON.stringify({ alt: { en: alt } }), { "content-type": "application/json" });
      stats.alt++;
    }
    assetBySrc.set(src, asset.id);
  }

  // 2. content entries
  const out = entries.map((e) => ({
    page: e.page,
    slot: e.slot,
    kind: e.kind,
    label: e.label,
    locale: "en",
    value: e.kind === "image" ? { media: assetBySrc.get(e.value.src) } : e.value,
  }));
  if (dry) {
    for (const e of out) console.log(`  ${e.page.padEnd(24)} ${e.slot.padEnd(44)} ${e.kind.padEnd(5)} ${JSON.stringify(e.value).slice(0, 90)}`);
    console.log(`Dry run: would upload ${assetBySrc.size} images and import ${out.length} entries.`);
    return;
  }
  let saved = 0;
  let published = 0;
  for (let i = 0; i < out.length; i += 500) {
    const r = await api("POST", "/v1/admin/content/import", JSON.stringify({ surface: "website", publish, entries: out.slice(i, i + 500) }), { "content-type": "application/json" });
    saved += r.saved ?? 0;
    published += r.published?.length ?? 0;
  }
  console.log(`Images: ${stats.uploaded} uploaded, ${stats.existing} already in the library, ${stats.dark} dark files set, ${stats.alt} alt texts set.`);
  console.log(`Content: ${saved} entries saved${publish ? `, ${published} published (unchanged values are not republished)` : " as drafts (publish them in the Back Office, or re-run without --drafts)"}.`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch((e) => {
    console.error(e.message || e);
    process.exit(1);
  });
}

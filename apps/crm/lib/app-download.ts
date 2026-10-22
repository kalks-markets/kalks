// Server-only: the Android app's release manifest for /download (deploy/README.md, "Android app downloads").
//
// The APK is not in git. deploy/publish-app.sh uploads `kalks-<version>.apk` and `latest.json` to the server's
// downloads directory, which Caddy serves at https://app.kalkstrade.com/downloads/*. This module reads latest.json
// (APP_DOWNLOAD_MANIFEST_URL, default <this origin>/downloads/latest.json), validates it, and caches the answer for a
// minute. Anything missing or wrong gives `null`: the page then says "coming soon" instead of a broken link.

/** Release manifest (latest.json), as written by deploy/publish-app.sh. */
export type AppManifest = {
  /** Version name, e.g. "1.4.0". */
  version: string;
  /** Android versionCode. */
  build: number;
  /** APK file name next to latest.json, e.g. "kalks-1.4.0.apk" (a bare name: no paths). */
  file: string;
  /** APK size in bytes. */
  size: number;
  /** Lowercase hex SHA-256 of the APK. */
  sha256: string;
  /** Lowest Android version the app installs on, e.g. "7.0". */
  minAndroid: string;
  /** ISO 8601 release time. */
  releasedAt: string;
};

/** A validated release with the APK's absolute download URL. */
export type AppRelease = AppManifest & { url: string };

/** The Android version the app supports when latest.json doesn't say (the app's minSdk 24). */
export const DEFAULT_MIN_ANDROID = "7.0";

const VERSION = /^\d{1,4}\.\d{1,4}\.\d{1,4}(?:[-+][0-9A-Za-z.-]{1,32})?$/;
const FILE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}\.apk$/;
const SHA256 = /^[0-9a-f]{64}$/i;
const ANDROID = /^\d{1,2}(?:\.\d{1,2}){0,2}$/;
/** Sanity cap: an APK above 1 GB is a broken manifest, not a release. */
const MAX_SIZE = 1024 * 1024 * 1024;

/**
 * Validate a parsed latest.json. Returns the manifest (sha256 lowercased, minAndroid defaulted) or null when any
 * field is missing or malformed, so the page never offers a link it can't stand behind.
 */
export function parseManifest(raw: unknown): AppManifest | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const m = raw as Record<string, unknown>;
  const { version, build, file, size, sha256, releasedAt } = m;
  const minAndroid = m.minAndroid ?? DEFAULT_MIN_ANDROID;
  if (typeof version !== "string" || !VERSION.test(version)) return null;
  if (typeof build !== "number" || !Number.isSafeInteger(build) || build < 1) return null;
  if (typeof file !== "string" || !FILE.test(file) || file.includes("..")) return null;
  if (typeof size !== "number" || !Number.isSafeInteger(size) || size < 1 || size > MAX_SIZE) return null;
  if (typeof sha256 !== "string" || !SHA256.test(sha256)) return null;
  if (typeof minAndroid !== "string" || !ANDROID.test(minAndroid)) return null;
  if (typeof releasedAt !== "string" || !/^\d{4}-\d{2}-\d{2}/.test(releasedAt) || Number.isNaN(Date.parse(releasedAt))) return null;
  return { version, build, file, size, sha256: sha256.toLowerCase(), minAndroid, releasedAt };
}

/** The manifest plus the APK's URL, resolved next to the manifest (same directory). Null when invalid. */
export function toRelease(raw: unknown, manifestUrl: string): AppRelease | null {
  const m = parseManifest(raw);
  if (!m) return null;
  try {
    return { ...m, url: new URL(m.file, manifestUrl).href };
  } catch {
    return null;
  }
}

/** Where latest.json lives: APP_DOWNLOAD_MANIFEST_URL, else /downloads/latest.json on this site's origin. */
export function manifestUrl(origin: string): string {
  const env = process.env.APP_DOWNLOAD_MANIFEST_URL?.trim();
  return env || `${origin.replace(/\/$/, "")}/downloads/latest.json`;
}

const TTL_MS = 60_000;
const cache = new Map<string, { at: number; v: AppRelease | null }>();

/**
 * The current Android release, or null ("coming soon") when latest.json is missing, unreachable or invalid.
 * Cached for a minute per manifest URL (a failure too, so a missing file isn't fetched on every visit).
 */
export async function latestRelease(origin: string, fetcher: typeof fetch = fetch): Promise<AppRelease | null> {
  const url = manifestUrl(origin);
  const hit = cache.get(url);
  if (hit && Date.now() - hit.at < TTL_MS) return hit.v;
  let v: AppRelease | null = null;
  try {
    const r = await fetcher(url, { cache: "no-store", signal: AbortSignal.timeout(3000), headers: { accept: "application/json" } });
    if (r.ok) v = toRelease(await r.json(), url);
  } catch {
    v = null;
  }
  cache.set(url, { at: Date.now(), v });
  return v;
}

/** Test-only: forget the cached manifests. */
export function clearReleaseCache() {
  cache.clear();
}

/** Size for people: "48.2 MB" (decimal megabytes, as Android's file manager shows them). */
export function formatSize(bytes: number, format: (v: number, decimals?: number) => string = (v, d) => v.toFixed(d)): string {
  if (bytes >= 1_000_000) return `${format(bytes / 1_000_000, 1)} MB`;
  return `${format(Math.max(1, Math.round(bytes / 1000)), 0)} KB`;
}

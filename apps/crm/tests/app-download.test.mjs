// Android app page (/download, lib/app-download.ts): `node --test apps/crm/tests`.
//
// What it guards: latest.json (written by deploy/publish-app.sh) is validated field by field before the page offers a
// link; the APK URL is resolved next to the manifest and can't be pointed elsewhere; a missing, unreachable or invalid
// manifest gives the honest "coming soon" state (null), and the answer is cached briefly. Also: /download is public.

import { test } from "node:test";
import assert from "node:assert/strict";
import "./helpers/ts-hooks.mjs";

const lib = await import("../lib/app-download.ts");

const GOOD = {
  version: "1.4.0",
  build: 42,
  file: "kalks-1.4.0.apk",
  size: 48_213_504,
  sha256: "C200ECAAF657DC17975DDE54B329B1A82FAFC993660AD860A5BA377A7180CB53",
  minAndroid: "7.0",
  releasedAt: "2026-10-20T09:30:00Z",
};

test("a valid manifest parses (sha256 lowercased) and minAndroid defaults to 7.0", () => {
  const m = lib.parseManifest(GOOD);
  assert.deepEqual(m, { ...GOOD, sha256: GOOD.sha256.toLowerCase() });
  const { minAndroid: _, ...noMin } = GOOD;
  assert.equal(lib.parseManifest(noMin)?.minAndroid, "7.0");
  assert.equal(lib.parseManifest({ ...GOOD, version: "2.0.0-beta.1" })?.version, "2.0.0-beta.1");
});

test("an invalid manifest is refused field by field", () => {
  for (const raw of [null, undefined, "x", 42, [], [GOOD]]) assert.equal(lib.parseManifest(raw), null, String(raw));
  const bad = {
    version: [undefined, "", "1.4", "v1.4.0", "1.4.0 ", 140],
    build: [undefined, 0, -1, 1.5, "42", Number.MAX_SAFE_INTEGER + 1],
    file: [undefined, "", "kalks.zip", "../kalks.apk", "a/kalks.apk", "https://evil.test/kalks.apk", ".kalks.apk", "kalks..apk/x.apk", "kalks 1.apk"],
    size: [undefined, 0, -5, 1.5, "48000000", 2 * 1024 * 1024 * 1024],
    sha256: [undefined, "", "abc", "z".repeat(64), "a".repeat(63), "a".repeat(65)],
    minAndroid: ["", "seven", "7.0.0.1", 7],
    releasedAt: [undefined, "", "yesterday", "2026-13-45T00:00:00Z", 1760000000],
  };
  for (const [field, values] of Object.entries(bad)) {
    for (const v of values) assert.equal(lib.parseManifest({ ...GOOD, [field]: v }), null, `${field}=${String(v)}`);
  }
});

test("the APK URL sits next to the manifest", () => {
  const r = lib.toRelease(GOOD, "https://app.kalkstrade.com/downloads/latest.json");
  assert.equal(r?.url, "https://app.kalkstrade.com/downloads/kalks-1.4.0.apk");
  assert.equal(lib.toRelease({ ...GOOD, file: "//evil.test/x.apk" }, "https://app.kalkstrade.com/downloads/latest.json"), null);
});

test("manifest URL: this origin by default, APP_DOWNLOAD_MANIFEST_URL when set", () => {
  const saved = process.env.APP_DOWNLOAD_MANIFEST_URL;
  delete process.env.APP_DOWNLOAD_MANIFEST_URL;
  assert.equal(lib.manifestUrl("https://app.broker.test"), "https://app.broker.test/downloads/latest.json");
  assert.equal(lib.manifestUrl("https://app.broker.test/"), "https://app.broker.test/downloads/latest.json");
  process.env.APP_DOWNLOAD_MANIFEST_URL = "https://app.kalkstrade.com/downloads/latest.json";
  assert.equal(lib.manifestUrl("https://app.broker.test"), "https://app.kalkstrade.com/downloads/latest.json");
  if (saved === undefined) delete process.env.APP_DOWNLOAD_MANIFEST_URL;
  else process.env.APP_DOWNLOAD_MANIFEST_URL = saved;
});

/** A fetch stub answering one status / body, counting calls. */
function stub(status, body) {
  const calls = [];
  const fn = async (url) => {
    calls.push(String(url));
    if (status === "throw") throw new TypeError("fetch failed");
    return new Response(typeof body === "string" ? body : JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
  };
  return Object.assign(fn, { calls });
}

test("latestRelease: a published manifest gives the release with its APK URL", async () => {
  lib.clearReleaseCache();
  const f = stub(200, GOOD);
  const r = await lib.latestRelease("https://app.kalkstrade.com", f);
  assert.equal(f.calls[0], "https://app.kalkstrade.com/downloads/latest.json");
  assert.equal(r?.version, "1.4.0");
  assert.equal(r?.url, "https://app.kalkstrade.com/downloads/kalks-1.4.0.apk");
});

test("latestRelease: missing, unreachable, non-JSON or invalid manifest = coming soon (null)", async () => {
  for (const [status, body] of [[404, "Not Found"], [500, GOOD], ["throw", null], [200, "<html>not json</html>"], [200, { ...GOOD, sha256: "nope" }], [200, {}]]) {
    lib.clearReleaseCache();
    assert.equal(await lib.latestRelease("https://app.kalkstrade.com", stub(status, body)), null, `${status} ${JSON.stringify(body)}`);
  }
});

test("latestRelease: answers (a miss too) are cached briefly per manifest URL", async () => {
  lib.clearReleaseCache();
  const miss = stub(404, "Not Found");
  await lib.latestRelease("https://a.test", miss);
  await lib.latestRelease("https://a.test", miss);
  assert.equal(miss.calls.length, 1);
  const hit = stub(200, GOOD);
  assert.equal((await lib.latestRelease("https://b.test", hit))?.build, 42);
  assert.equal((await lib.latestRelease("https://b.test", hit))?.build, 42);
  assert.equal(hit.calls.length, 1);
  lib.clearReleaseCache();
});

test("formatSize: decimal megabytes like Android shows them", () => {
  assert.equal(lib.formatSize(48_213_504), "48.2 MB");
  assert.equal(lib.formatSize(512_000), "512 KB");
  assert.equal(lib.formatSize(10), "1 KB");
});

test("/download is public: signed-out visitors aren't sent to /login", async () => {
  const { NextRequest } = await import("next/server");
  const { proxy } = await import("../proxy.ts");
  // no broker config reachable: the gate falls through, only the sign-in rule applies
  process.env.GATEWAY_URL ??= "http://127.0.0.1:9";
  const res = await proxy(new NextRequest("https://app.kalkstrade.com/download"));
  assert.equal(res.headers.get("location"), null);
  const other = await proxy(new NextRequest("https://app.kalkstrade.com/wallet"));
  assert.match(other.headers.get("location") ?? "", /\/login\?next=%2Fwallet$/);
});

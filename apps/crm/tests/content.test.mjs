// Editable content (packages/ui/src/content, lib/content.ts, app/api/content-preview): `node --test apps/crm/tests`.
// The server fetch runs against a stub content service on a loopback port.
//
// What it guards: nothing changes when the service is down (empty pages, never a throw), the last good copy is kept,
// copies are cached per tenant + locale, previews bypass the cache, the sanitiser drops unsafe links and image URLs,
// and the preview route only accepts a token-shaped value and a same-site path.

import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { createServer } from "node:http";
import "./helpers/ts-hooks.mjs";

let service;
let calls = [];
let up = true;

const BODY = {
  tenant: "kalks",
  surface: "app",
  locale: "de",
  preview: false,
  pages: {
    auth: {
      "side.headline": { kind: "text", locale: "de", value: { text: "Hallo\u0007 Welt" } },
      "side.image": {
        kind: "image",
        locale: "en",
        value: { media: "m0123456789abcdef01", alt: "Desk", url: "https://api.test/v1/public/media/aa/orig", srcset: "https://api.test/x 480w, javascript:alert(1) 800w", width: 1200, height: 600, dark: { url: "https://api.test/v1/public/media/bb/orig", srcset: "" } },
      },
      "bad.image": { kind: "image", locale: "en", value: { media: "m1", url: "javascript:alert(1)" } },
      "cta": { kind: "link", locale: "en", value: { label: "Go", href: "javascript:alert(1)" } },
      "ok.cta": { kind: "link", locale: "en", value: { label: "Go", href: "/register" } },
      "Bad Key": { kind: "text", locale: "en", value: { text: "x" } },
    },
  },
};

before(async () => {
  service = createServer((req, res) => {
    const url = new URL(req.url, "http://x");
    calls.push(url.search);
    if (!up) {
      req.socket.destroy();
      return;
    }
    const preview = url.searchParams.get("preview");
    if (preview && preview !== "good.token") {
      res.writeHead(403, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: { code: "preview_expired" } }));
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ ...BODY, preview: !!preview, locale: url.searchParams.get("locale") }));
  });
  await new Promise((resolve) => service.listen(0, "127.0.0.1", resolve));
  process.env.CONTENT_URL = `http://127.0.0.1:${service.address().port}`;
});

after(() => service?.close());

const load = async () => ({ server: await import("@kalks/ui/content/server"), route: await import("../app/api/content-preview/route.ts"), next: await import("next/server") });

test("fetches, sanitises and caches published content per tenant and locale", async () => {
  const { server } = await load();
  calls = [];
  const b = await server.fetchContent({ surface: "app", tenant: "kalks", locale: "de" });
  assert.equal(b.locale, "de");
  const auth = b.pages.auth;
  assert.deepEqual(auth["side.headline"], { kind: "text", locale: "de", text: "Hallo Welt" });
  assert.equal(auth["side.image"].image.url, "https://api.test/v1/public/media/aa/orig");
  assert.equal(auth["side.image"].image.srcset, "https://api.test/x 480w", "unsafe srcset entries dropped");
  assert.equal(auth["side.image"].image.dark.url, "https://api.test/v1/public/media/bb/orig");
  assert.equal(auth["bad.image"], undefined);
  assert.equal(auth.cta, undefined, "javascript: links dropped");
  assert.equal(auth["ok.cta"].link.href, "/register");
  assert.equal(auth["Bad Key"], undefined);
  assert.match(calls[0], /surface=app/);
  assert.match(calls[0], /tenant=kalks/);
  // cached: a second read within the minute doesn't call the service
  await server.fetchContent({ surface: "app", tenant: "kalks", locale: "de" });
  assert.equal(calls.length, 1);
  // another broker / language is its own entry; a bad slug reads as kalks
  await server.fetchContent({ surface: "app", tenant: "acme", locale: "de" });
  await server.fetchContent({ surface: "app", tenant: "../x", locale: "fr" });
  assert.equal(calls.length, 3);
  assert.match(calls[2], /tenant=kalks/);
});

test("previews bypass the cache; an expired token falls back to the published copy", async () => {
  const { server } = await load();
  calls = [];
  const p = await server.fetchContent({ surface: "app", tenant: "kalks", locale: "de", preview: "good.token" });
  assert.equal(p.preview, true);
  const p2 = await server.fetchContent({ surface: "app", tenant: "kalks", locale: "de", preview: "good.token" });
  assert.equal(p2.preview, true);
  assert.equal(calls.length, 2, "never cached");
  const expired = await server.fetchContent({ surface: "app", tenant: "kalks", locale: "de", preview: "old.token" });
  assert.equal(expired.preview, false);
  assert.ok(expired.pages.auth["side.headline"]);
});

test("an unreachable service keeps the last good copy, else empty pages", async () => {
  const { server } = await load();
  up = false;
  try {
    const fresh = await server.fetchContent({ surface: "trader", tenant: "kalks", locale: "ja", timeoutMs: 500 });
    assert.deepEqual(fresh.pages, {}, "nothing cached: built-in defaults everywhere");
    const old = await server.fetchContent({ surface: "app", tenant: "kalks", locale: "de" });
    assert.ok(old.pages.auth, "cached copy still served");
  } finally {
    up = true;
  }
});

test("the preview route stores a token-shaped value and redirects to a same-site path only", async () => {
  const { route, next } = await load();
  const call = (q) => route.GET(new next.NextRequest(`http://app.test/api/content-preview?${q}`));
  let res = call("token=abc.def&path=/login");
  assert.equal(res.status, 307);
  assert.equal(res.headers.get("location"), "http://app.test/login");
  assert.match(res.headers.get("set-cookie"), /kalks_content_preview=abc\.def/);
  assert.match(res.headers.get("set-cookie"), /HttpOnly/i);
  res = call("token=abc.def&path=//evil.example/x");
  assert.equal(res.headers.get("location"), "http://app.test/", "protocol-relative paths refused");
  res = call("token=abc.def&path=https://evil.example/");
  assert.equal(res.headers.get("location"), "http://app.test/");
  assert.equal(call("token=<script>&path=/").status, 400);
  res = call("exit=1&path=/");
  assert.match(res.headers.get("set-cookie"), /kalks_content_preview=;/);
});

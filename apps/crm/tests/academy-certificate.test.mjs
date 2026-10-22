// Public Academy certificate image (app/certificate/[code]/image, lib/academy.ts): `node --test apps/crm/tests`.
//
// What it guards: the verification page's image keeps working while the broker has the Academy module off (the
// /api/academy BFF is refused then, but certificate checks stay public); unknown codes are 404 without calling the
// service, and a service outage is 503.

import { test } from "node:test";
import assert from "node:assert/strict";
import "./helpers/ts-hooks.mjs";

const { moduleOff } = await import("@kalks/ui/sections");
const { GET } = await import("../app/certificate/[code]/image/route.ts");
const { NextRequest } = await import("next/server");

const CODE = "KA-ABCDE-FGHJK";
const call = (code, qs = "") => GET(new NextRequest(`https://app.kalkstrade.com/certificate/${code}/image${qs}`), { params: Promise.resolve({ code }) });

test("the verification image isn't behind the Academy module switch (the BFF image is)", () => {
  const off = { academy: false };
  assert.equal(moduleOff(off, `/certificate/${CODE}/image`), false);
  assert.equal(moduleOff(off, `/certificate/${CODE}`), false);
  assert.equal(moduleOff(off, `/api/academy/certificates/${CODE}/image`), true);
});

test("serves the service's SVG; unknown codes are 404, an outage is 503", async (t) => {
  const seen = [];
  let status = 200;
  t.mock.method(globalThis, "fetch", async (url) => {
    seen.push(String(url));
    return new Response(status === 200 ? "<svg/>" : "{}", { status, headers: { "content-type": "image/svg+xml" } });
  });
  const ok = await call(CODE, "?download=1");
  assert.equal(ok.status, 200);
  assert.equal(await ok.text(), "<svg/>");
  assert.match(ok.headers.get("content-type"), /^image\/svg\+xml/);
  assert.match(ok.headers.get("content-disposition"), /kalks-academy-KA-ABCDE-FGHJK\.svg/);
  assert.match(seen[0], /\/v1\/public\/certificates\/KA-ABCDE-FGHJK\/svg$/);

  assert.equal((await call("../etc/passwd")).status, 404);
  assert.equal(seen.length, 1, "an invalid code never reaches the service");
  status = 404;
  assert.equal((await call(CODE)).status, 404);
  status = 500;
  assert.equal((await call(CODE)).status, 503);
});

// Redirects from route handlers (staff "Log in as client", expired sessions, content preview, the Trader's /sso):
// behind Caddy the request URL is Next's own listen address, so redirects must use the forwarded host.
// `node --test apps/crm/tests` (Node 22.6+ strips the TypeScript types).

import { test } from "node:test";
import assert from "node:assert/strict";
import { publicUrl } from "../../../packages/ui/src/lib/public-url.ts";

test("redirects go to the public host, not the server's listen address", () => {
  // what production's route handlers see: req.url on localhost, the browser's host in the proxy headers
  const req = { url: "https://localhost:3000/api/auth/impersonate?ticket=x", headers: new Headers({ host: "app.kalkstrade.com", "x-forwarded-host": "app.kalkstrade.com", "x-forwarded-proto": "https" }) };
  assert.equal(publicUrl(req, "/").href, "https://app.kalkstrade.com/");
  assert.equal(publicUrl(req, "/staff-session?state=expired").href, "https://app.kalkstrade.com/staff-session?state=expired");
  const login = publicUrl({ url: "https://localhost:3001/api/auth/expired", headers: new Headers({ host: "admin.kalkstrade.com" }) }, "/login");
  login.searchParams.set("next", "/clients");
  assert.equal(login.href, "https://admin.kalkstrade.com/login?next=%2Fclients");
});

test("development keeps the request's own origin", () => {
  assert.equal(publicUrl({ url: "http://localhost:3000/api/auth/expired", headers: new Headers({ host: "localhost:3000" }) }, "/login").href, "http://localhost:3000/login");
});

test("a malformed forwarded host falls back to the request origin", () => {
  assert.equal(publicUrl({ url: "http://localhost:3000/a", headers: new Headers({ "x-forwarded-host": "evil.com/x?" }) }, "/").href, "http://localhost:3000/");
});

/**
 * Public origin of this app as the browser sees it. Behind Caddy that comes from X-Forwarded-Proto/Host
 * (app.kalkstrade.com); in development it is the request's own origin (http://localhost:3000).
 *
 * Route handlers must build redirects from this, not from `req.url` / `req.nextUrl`: in the production server those
 * carry the address Next listens on (https://localhost:3000), so `new URL("/", req.url)` sends the browser to
 * localhost. Caddy only forwards hosts it serves, so the host header can't point a redirect elsewhere.
 */
export function publicOrigin(headers: Headers, requestUrl: string): string {
  const url = new URL(requestUrl);
  const first = (v: string | null) => v?.split(",")[0]?.trim() || "";
  const host = first(headers.get("x-forwarded-host")) || first(headers.get("host")) || url.host;
  let proto = first(headers.get("x-forwarded-proto")) || url.protocol.replace(/:$/, "");
  if (proto !== "https" && proto !== "http") proto = "https";
  if (!/^[a-z0-9.-]+(:\d{1,5})?$/i.test(host)) return url.origin;
  return `${proto}://${host}`;
}

/** `path` (with its query) on this app's public origin, for redirects from route handlers. */
export function publicUrl(req: { headers: Headers; url: string }, path: string): URL {
  return new URL(path, publicOrigin(req.headers, req.url));
}

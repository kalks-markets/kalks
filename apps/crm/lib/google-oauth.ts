// "Continue with Google" (OAuth 2.0 authorization code + PKCE, OpenID Connect), server side only.
//
//   /api/auth/google/start     sets a signed, HttpOnly, 10-minute state cookie (state, PKCE verifier, nonce,
//                              next, ref, redirect_uri) and redirects to Google.
//   /api/auth/google/callback  checks state, exchanges the code with the client secret + PKCE verifier,
//                              verifies the ID token (Google JWKS signature, iss, aud, exp, nonce, email_verified)
//                              and hands the verified identity to the gateway (/v1/auth/google).
//
// Everything here is plain TypeScript with no Next or "@/" imports so it can be unit-tested with
// `node --test apps/crm/tests` (the tests inject their own JWKS / token exchange / gateway; production never does).

import { createHash, createHmac, randomBytes, timingSafeEqual } from "node:crypto";
import { createRemoteJWKSet, errors as joseErrors, jwtVerify, type JWTVerifyGetKey } from "jose";

export const GOOGLE_AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth";
export const GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token";
export const GOOGLE_JWKS_URL = "https://www.googleapis.com/oauth2/v3/certs";
export const GOOGLE_ISSUERS = ["https://accounts.google.com", "accounts.google.com"];

export const STATE_COOKIE = "kalks_google_state";
export const TICKET_COOKIE = "kalks_google_ticket";
export const STATE_TTL_SECONDS = 600;
export const CALLBACK_PATH = "/api/auth/google/callback";

export type GoogleConfig = { clientId: string; clientSecret: string };
export type Mode = "login" | "register";

/** Reads GOOGLE_CLIENT_ID / GOOGLE_CLIENT_SECRET (server-only env). Null when Google sign-in isn't configured. */
export function googleConfig(env: Record<string, string | undefined> = process.env): GoogleConfig | null {
  const clientId = env.GOOGLE_CLIENT_ID?.trim();
  const clientSecret = env.GOOGLE_CLIENT_SECRET?.trim();
  return clientId && clientSecret ? { clientId, clientSecret } : null;
}

// ---------- small helpers ----------

export function randomToken(bytes = 32): string {
  return randomBytes(bytes).toString("base64url");
}

/** PKCE S256 challenge for a verifier. */
export function pkceChallenge(verifier: string): string {
  return createHash("sha256").update(verifier).digest("base64url");
}

/** Only same-app relative paths are allowed as post-login redirects (mirrors safeNext in lib/gateway.ts). */
export function safePath(next: string | null | undefined, fallback = "/"): string {
  if (!next || !next.startsWith("/") || next.startsWith("//") || next.startsWith("/\\") || next.startsWith("/api/")) return fallback;
  return next;
}

/** Referral code from ?ref= (same shape the gateway accepts), else "". */
export function cleanRef(ref: string | null | undefined): string {
  const v = (ref ?? "").trim();
  return /^[A-Za-z0-9]{3,24}$/.test(v) ? v : "";
}

// Public origin as the browser sees it (Google only redirects to URIs registered on the OAuth client, so a forged
// host cannot receive a code).
export { publicOrigin } from "@kalks/ui/public-url";

// ---------- state cookie ----------

export type OAuthState = {
  v: 1;
  state: string;
  verifier: string;
  nonce: string;
  mode: Mode;
  next: string;
  ref: string;
  redirectUri: string;
  /** issued at, unix seconds */
  iat: number;
};

function stateKey(cfg: GoogleConfig): Buffer {
  return createHmac("sha256", cfg.clientSecret).update("kalks-google-state-cookie-v1").digest();
}

export function sealState(s: OAuthState, cfg: GoogleConfig): string {
  const payload = Buffer.from(JSON.stringify(s)).toString("base64url");
  const mac = createHmac("sha256", stateKey(cfg)).update(payload).digest("base64url");
  return `${payload}.${mac}`;
}

export function openState(value: string | undefined, cfg: GoogleConfig, nowSec = Math.floor(Date.now() / 1000)): OAuthState | null {
  if (!value || value.length > 4096) return null;
  const [payload, mac, extra] = value.split(".");
  if (!payload || !mac || extra !== undefined) return null;
  const want = createHmac("sha256", stateKey(cfg)).update(payload).digest();
  const got = Buffer.from(mac, "base64url");
  if (got.length !== want.length || !timingSafeEqual(got, want)) return null;
  try {
    const s = JSON.parse(Buffer.from(payload, "base64url").toString("utf8")) as OAuthState;
    if (s.v !== 1 || typeof s.state !== "string" || typeof s.verifier !== "string" || typeof s.nonce !== "string") return null;
    if (typeof s.iat !== "number" || nowSec - s.iat > STATE_TTL_SECONDS || s.iat - nowSec > 60) return null;
    return s;
  } catch {
    return null;
  }
}

function sameString(a: string, b: string): boolean {
  const x = Buffer.from(a);
  const y = Buffer.from(b);
  return x.length === y.length && timingSafeEqual(x, y);
}

// ---------- start ----------

export function startFlow(input: { mode: Mode; next: string; ref: string; origin: string }, cfg: GoogleConfig, nowSec = Math.floor(Date.now() / 1000)) {
  const s: OAuthState = {
    v: 1,
    state: randomToken(24),
    verifier: randomToken(48),
    nonce: randomToken(24),
    mode: input.mode,
    next: safePath(input.next, "/"),
    ref: cleanRef(input.ref),
    redirectUri: `${input.origin}${CALLBACK_PATH}`,
    iat: nowSec,
  };
  const url = new URL(GOOGLE_AUTH_URL);
  url.search = new URLSearchParams({
    client_id: cfg.clientId,
    redirect_uri: s.redirectUri,
    response_type: "code",
    scope: "openid email profile",
    state: s.state,
    nonce: s.nonce,
    code_challenge: pkceChallenge(s.verifier),
    code_challenge_method: "S256",
    prompt: "select_account",
    include_granted_scopes: "true",
  }).toString();
  return { url: url.toString(), cookie: sealState(s, cfg), state: s };
}

// ---------- token exchange + ID token ----------

export type GoogleIdentity = {
  sub: string;
  email: string;
  email_verified: boolean;
  given_name?: string;
  family_name?: string;
  picture?: string;
};

export class GoogleAuthError extends Error {
  code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

/** Exchanges the authorization code for tokens (server to server, client secret + PKCE verifier). Returns the ID token. */
export async function exchangeCode(code: string, verifier: string, redirectUri: string, cfg: GoogleConfig, fetchImpl: typeof fetch = fetch): Promise<string> {
  let res: Response;
  try {
    res = await fetchImpl(GOOGLE_TOKEN_URL, {
      method: "POST",
      headers: { "content-type": "application/x-www-form-urlencoded", accept: "application/json" },
      body: new URLSearchParams({
        grant_type: "authorization_code",
        code,
        code_verifier: verifier,
        redirect_uri: redirectUri,
        client_id: cfg.clientId,
        client_secret: cfg.clientSecret,
      }).toString(),
      cache: "no-store",
      signal: AbortSignal.timeout(10_000),
    });
  } catch {
    throw new GoogleAuthError("unavailable", "Google did not respond");
  }
  const data = (await res.json().catch(() => ({}))) as { id_token?: unknown; error?: unknown };
  if (!res.ok || typeof data.id_token !== "string") {
    // invalid_grant = code already used / expired: treat like an expired attempt
    throw new GoogleAuthError(data.error === "invalid_grant" ? "expired" : "failed", `token exchange failed (${res.status} ${String(data.error ?? "")})`);
  }
  return data.id_token;
}

let remoteJwks: JWTVerifyGetKey | null = null;

/** Google's signing keys, fetched on demand and cached in memory by jose (refetched on unknown kid, at most every 30s). */
export function googleJwks(): JWTVerifyGetKey {
  remoteJwks ??= createRemoteJWKSet(new URL(GOOGLE_JWKS_URL), { cacheMaxAge: 6 * 3600_000, cooldownDuration: 30_000, timeoutDuration: 5_000 });
  return remoteJwks;
}

/** Verifies a Google ID token: RS256 signature against the JWKS, iss, aud (= our client id), exp/iat, azp, nonce, email_verified. */
export async function verifyIdToken(idToken: string, opts: { clientId: string; nonce: string; jwks: JWTVerifyGetKey; now?: Date }): Promise<GoogleIdentity> {
  let payload: Record<string, unknown>;
  try {
    ({ payload } = await jwtVerify(idToken, opts.jwks, {
      issuer: GOOGLE_ISSUERS,
      audience: opts.clientId,
      algorithms: ["RS256"],
      requiredClaims: ["iss", "aud", "exp", "iat", "sub"],
      clockTolerance: 60,
      currentDate: opts.now,
    }));
  } catch (e) {
    if (e instanceof joseErrors.JWKSTimeout) throw new GoogleAuthError("unavailable", "Google keys unavailable");
    throw new GoogleAuthError("failed", `id token rejected: ${e instanceof Error ? e.message : "invalid"}`);
  }
  if (typeof payload.nonce !== "string" || !sameString(payload.nonce, opts.nonce)) throw new GoogleAuthError("failed", "id token nonce mismatch");
  if (payload.azp !== undefined && payload.azp !== opts.clientId) throw new GoogleAuthError("failed", "id token azp mismatch");
  if (typeof payload.sub !== "string" || !payload.sub || typeof payload.email !== "string" || !payload.email) throw new GoogleAuthError("failed", "id token has no sub/email");
  if (payload.email_verified !== true && payload.email_verified !== "true") throw new GoogleAuthError("unverified", "google email not verified");
  const str = (v: unknown) => (typeof v === "string" && v ? v : undefined);
  return {
    sub: payload.sub,
    email: payload.email,
    email_verified: true,
    given_name: str(payload.given_name),
    family_name: str(payload.family_name),
    picture: str(payload.picture),
  };
}

// ---------- callback ----------

type GatewayReply = { status: number; data: Record<string, unknown> };
export type Session = { token: string; expires_at: string };

export type CallbackDeps = {
  /** code -> ID token (production: exchangeCode against Google) */
  exchange: (code: string, verifier: string, redirectUri: string) => Promise<string>;
  jwks: JWTVerifyGetKey;
  /** POST /v1/auth/google on the gateway */
  signIn: (body: GoogleIdentity & { ref: string | null }) => Promise<GatewayReply>;
  now?: Date;
  log?: (msg: string) => void;
};

export type CallbackOutcome = {
  /** same-app path to redirect to */
  redirect: string;
  session?: Session;
  ticket?: { value: string; maxAge: number };
  error?: string;
};

/** Maps a failure to the page that shows it: /login?google_error=... or /register?google_error=... */
export function errorRedirect(code: string, mode: Mode, next = "/", ref = ""): CallbackOutcome {
  const q = new URLSearchParams({ google_error: code });
  if (mode === "login" && next !== "/") q.set("next", next);
  if (mode === "register" && ref) q.set("ref", ref);
  return { redirect: `/${mode}?${q.toString()}`, error: code };
}

const GATEWAY_ERRORS: Record<string, string> = {
  google_unverified: "unverified",
  google_conflict: "conflict",
  account_disabled: "disabled",
  account_suspended: "suspended",
  rate_limited: "rate_limited",
  unavailable: "unavailable",
};

export async function handleCallback(query: URLSearchParams, stateCookie: string | undefined, cfg: GoogleConfig, deps: CallbackDeps): Promise<CallbackOutcome> {
  const s = openState(stateCookie, cfg, Math.floor((deps.now?.getTime() ?? Date.now()) / 1000));
  const mode: Mode = s?.mode ?? "login";
  const fail = (code: string) => errorRedirect(code, mode, s?.next ?? "/", s?.ref ?? "");

  const gErr = query.get("error");
  if (gErr) return fail(gErr === "access_denied" ? "cancelled" : "failed");
  const code = query.get("code");
  const state = query.get("state");
  if (!s || !state || !sameString(state, s.state)) return fail("expired");
  if (!code || code.length > 2048) return fail("failed");

  let identity: GoogleIdentity;
  try {
    const idToken = await deps.exchange(code, s.verifier, s.redirectUri);
    identity = await verifyIdToken(idToken, { clientId: cfg.clientId, nonce: s.nonce, jwks: deps.jwks, now: deps.now });
  } catch (e) {
    const c = e instanceof GoogleAuthError ? e.code : "failed";
    deps.log?.(`google sign-in: ${e instanceof Error ? e.message : String(e)}`);
    return fail(c);
  }

  const r = await deps.signIn({ ...identity, ref: s.ref || null });
  const data = r.data;
  if (r.status === 200 && data.status === "ok" && data.session) {
    return { redirect: s.next, session: data.session as Session };
  }
  if (r.status === 200 && data.status === "profile_required" && typeof data.ticket === "string") {
    const maxAge = typeof data.expires_in === "number" ? data.expires_in : 1800;
    const q = s.next !== "/" ? `?${new URLSearchParams({ next: s.next })}` : "";
    return { redirect: `/register/complete${q}`, ticket: { value: data.ticket, maxAge } };
  }
  const errCode = ((data.error as { code?: string } | undefined)?.code ?? "") as string;
  deps.log?.(`google sign-in: gateway ${r.status} ${errCode}`);
  return fail(GATEWAY_ERRORS[errCode] ?? (r.status >= 500 ? "unavailable" : "failed"));
}

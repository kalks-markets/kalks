# Client Area (apps/crm)

Next.js app on http://localhost:3000. Live builds (default) show real data only. Demo builds (`NEXT_PUBLIC_KALKS_MODE=demo`) show the mock showcase. Design rules are in `docs/CONVENTIONS.md`.

## Trading accounts (live builds)

`/accounts`, `/accounts/new`, `/accounts/[login]`, `/portfolio`, `/portfolio/history`, `/portfolio/ledger` and `/portfolio/statements` read from the trading engine (`services/trading`, see its README "Client Area API"). They go through the BFF at `app/api/trading/[...path]/route.ts`:

- The client is resolved on the server from the HttpOnly `kalks_session` cookie, using the gateway's `/v1/auth/me`. The engine receives that id in `X-Kalks-User-Id`. The BFF never uses a user id sent by the browser.
- CSRF protection: POSTs must be JSON and carry a same-origin `Origin` header.
- Dealer-only fields (route, book, execution delay, markup) are stripped before responses reach the browser.
- The engine's internal token stays on the server.

| BFF route | Engine |
|---|---|
| `GET groups` | `GET /v1/groups` |
| `GET / POST accounts` | `GET / POST /v1/accounts` (generated passwords are returned once and never stored) |
| `GET accounts/{login}` | `GET /v1/accounts/{login}` |
| `GET accounts/{login}/history`, `…/ledger` (`from`, `to`, `page`, `limit`) | same paths |
| `GET accounts/{login}/export?kind=history\|ledger&from&to` | CSV statement built from the paged engine routes |
| `POST accounts/{login}/demo-refill` | same path |
| `POST accounts/{login}/passwords`, `…/leverage` (need `stepup_token`, see below) | same paths |
| `POST accounts/{login}/sso` | `POST /v1/accounts/{login}/sso`, returns `{url, expiresAt}` |

### Email-code confirmation (D20)

Trading and investor password changes, leverage changes and the Client Area password change need a 6-digit code emailed to the client, even inside a session:

1. `POST /api/auth/stepup {action, target}` emails the code (`action`: `trading_password`, `investor_password`, `leverage`, `account_password`, `withdrawal`, …; `target`: the account login where it applies). Resend with `POST /api/auth/stepup-resend {challenge}`.
2. `POST /api/auth/stepup-verify {challenge, code, action, target}` returns `stepup_token`: single use, valid for 5 minutes, bound to the client, the action and the target.
3. The change request carries `stepup_token` (body field, or the `X-Kalks-Stepup` header). The trading BFF checks the request against the account first, then redeems the token with the gateway (`POST /v1/auth/stepup/consume`, server to server only) and only then calls the engine. Without a valid token it answers 403 `stepup_required` / `stepup_invalid`.

The Client Area password is changed with `POST /api/auth/password {current, new, stepup_token, sign_out_others}` (gateway `POST /v1/auth/password`). UI: `components/stepup.tsx` (`useStepUp`, `StepUpCode`, `StepUpDialog`).

### Kalks Trader SSO

The Trade button opens `NEXT_PUBLIC_TERMINAL_URL + "/?sso=<token>"`. The token is one-time and valid for 60 s. Kalks Trader redeems it through its own BFF with `POST /v1/terminal/sso {token}`, stores the resulting session and removes `sso` from the URL.

## Mobile app API (`/api/mobile/*`)

The Flutter app talks only to this Client Area, with the gateway session as `Authorization: Bearer <token>` and no cookies. The full contract (paths, headers, auth, errors, streams) is in `docs/MOBILE-API.md`.

- **Rewrites.** `proxy.ts` rewrites `/api/mobile/<family>/*` (trading, wallet, news, notifications, kyc, security, support, status, growth, partner, social, prop, academy, reports, algo, suitability; `auth/heartbeat|impersonation|marketing`) onto the cookie routes above (`lib/mobile.ts`). The bearer becomes the rewritten request's session cookie, so every policy is reused unchanged: viewer scope, staff read-only, module switches, step-up, maintenance.
- **Cookies.** Browser cookies on `/api/mobile/*` are dropped. A bearer token together with cookies is refused (`400 bearer_with_cookies`). The same-origin check is satisfied only for bearer requests without cookies.
- **Native routes:**
  - `app/api/mobile/auth/[action]`: the session comes back in JSON, and a device id is minted when the app has none.
  - `app/api/mobile/config`: public service URLs, stream URLs, branding, modules, maintenance.
  - `app/api/mobile/trade/*`: the engine session, minted server-side through the account SSO or an MT5-style login, and handed out as a trade token bound to the client (`lib/mobile-trade.ts`). Also everything Kalks Trader's BFF exposes (`lib/trade-bodies.ts`), plus the AI routes with Kalks Trader's budget rules (`lib/mobile-ai.ts`).
- **Tests:** `node --test tests/` (`mobile.test.mjs`, `mobile-trade.test.mjs`).

## Prop challenges (live builds)

`/prop` (catalogue and checkout), `/prop/mine` (live rule dashboard, polled every 2 s while the tab is visible), `/prop/payouts` and `/prop/certificates` read from the prop service (`services/prop`, see its README). Components: `components/prop-live/*`; demo builds keep the mock pages. BFF: `app/api/prop/[...path]/route.ts` (server helper `lib/prop.ts`), same session, CSRF and 401 handling as the trading BFF. It forwards `X-Kalks-User-Id`, `X-Kalks-User-Name` (percent-encoded, used on certificates), `X-Kalks-User-Kyc` and `X-Kalks-Tenant`.

| BFF route | Prop service |
|---|---|
| `GET plans` | `GET /v1/plans` |
| `GET challenges` | `GET /v1/challenges` |
| `POST challenges` `{planId, size, idempotencyKey}` | `POST /v1/challenges` (fee from the USDT wallet; trading passwords returned once, never stored) |
| `GET challenges/{id}` | `GET /v1/challenges/{id}` (live rules, payout quote, events, certificates) |
| `GET challenges/{id}/equity?phase&limit`, `…/events?limit`, `…/trades?phase` | same paths |
| `POST challenges/{id}/payouts` | same path |
| `GET payouts`, `GET certificates`, `GET notifications`, `POST notifications/read` | same paths |

The checkout keeps one `idempotencyKey` per dialog session, so a retry after a network error or `payment_pending` never charges twice. The Trade button uses the trading BFF's SSO (`POST /api/trading/accounts/{login}/sso`).

Public certificate verification (no sign-in, outside the `(app)` shell; `proxy.ts` lets `/verify/**` through): `/verify/<code>` reads `GET /v1/public/certificates/{code}` server-side and `/verify/<code>/image` renders the 1200 × 675 PNG with `next/og` (`?download=1` for an attachment). Share links are built from the Client Area origin.

## Environment

These go in `apps/crm/.env.local` for local runs, or `apps/crm/.env.production.local` in production:

| Variable | |
|---|---|
| `GATEWAY_URL`, `GATEWAY_INTERNAL_TOKEN` | gateway (sign-in, session check) |
| `TRADING_URL` | trading engine, default `http://127.0.0.1:8090` |
| `TRADING_INTERNAL_TOKEN` | same value as the engine's `TRADING_INTERNAL_TOKEN` (repo-root `.env.local` / server env) |
| `NEXT_PUBLIC_TERMINAL_URL` | Kalks Trader origin, for example `https://trade.kalkstrade.com` (build time) |
| `PROP_URL`, `PROP_INTERNAL_TOKEN` | prop service, default `http://127.0.0.1:8097`; token = the service's `PROP_INTERNAL_TOKEN` |
| `OPTIONS_URL`, `OPTIONS_INTERNAL_TOKEN` | options service (the mobile app's options reads), default `http://127.0.0.1:8104` |
| `ANTHROPIC_API_KEY` | the mobile app's AI routes (`trade/ai-trader`, `trade/options/explain`); unset = `{configured: false}` |
| `MOBILE_MIN_APP_VERSION`, `MOBILE_TRADE_SECRET`, `MOBILE_*_URL` | optional mobile settings, see `docs/MOBILE-API.md` §9 |
| `APP_DOWNLOAD_MANIFEST_URL` | Android app page `/download`: the release manifest (`latest.json`); default `<this host>/downloads/latest.json`, see `deploy/README.md` |

## Security, sessions and view-only access (live builds)

`/profile/security` and `/profile/viewers` go through the security BFF `app/api/security/[...path]/route.ts` (gateway `/v1/auth/*`, `services/gateway/src/client_security.rs`):

- Sessions: `GET sessions` (live sessions, current marked, approximate country from `CF-IPCountry`), `POST sessions/{id}/revoke`, `POST sessions/revoke-others`; `GET logins` (90-day sign-in history). The shell's `SessionGuard` checks the session every 30 s and on focus, so a signed-out device leaves at once, and signs out after the broker's idle time (Back Office → Sessions) with a one-minute warning.
- View-only logins (D90/D93): `GET/POST viewers`, `PATCH viewers/{id}`, `POST viewers/{id}/password`, `POST viewers/{id}/revoke`. Creating one or setting a new password needs an emailed code (step-up action `viewer_access`); the password is shown once. Viewers sign in on `/login` with their viewer ID (no `@`). Their session token starts with `v.`: the proxy holds such sessions to GET requests of their sections (`lib/viewer.ts`) and refuses everything else with 403 `viewer_read_only`; `sessionUser` refuses viewer writes again, and the gateway refuses every change made with a viewer session. The trading and reports BFFs return only the accounts the viewer was given.
- Closure / data export (D94): `GET/POST requests`, `POST requests/{id}/cancel`, `GET requests/{id}/export` (JSON download once staff complete it). Staff process requests on the Back Office client page.


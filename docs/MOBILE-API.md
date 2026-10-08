# Kalks mobile app API (`/api/mobile/*`)

The Flutter app (`apps/mobile`, Android first) talks to **one** server: the Client Area (`apps/crm`), at
`https://app.kalkstrade.com` in production (or a broker's own `app.` domain). Every call goes to
`<base>/api/mobile/...`. The app never holds an internal service token and never calls the gateway, the trading
engine or any other service directly. The only exceptions are the WebSocket streams and market data, whose URLs come
from `GET /api/mobile/config`.

Server code: `apps/crm/lib/mobile.ts` (routing, headers, URLs), `apps/crm/proxy.ts` (the bearer branch),
`apps/crm/app/api/mobile/**` (native routes), `apps/crm/lib/mobile-trade.ts` (trade tokens),
`apps/crm/lib/trade-bodies.ts` (trade validation) and `apps/crm/lib/mobile-ai.ts` (AI budget).
Tests: `node --test apps/crm/tests` (`mobile.test.mjs`, `mobile-trade.test.mjs`).

## 1. Auth model

- **Gateway session.** Signing in returns a session `{token, expires_at}` in the JSON body. Sessions are opaque and last
  7 days; there is no refresh token. Keep the token in the Android Keystore (biometric unlock). Send it on every
  signed-in call as `Authorization: Bearer <token>`. When it expires (`401 unauthorized`), ask the user to sign in
  again.
- **No cookies, ever.** The app must not send a `Cookie` header to `/api/mobile/*`. A request with a bearer token
  **and** cookies is refused with `400 bearer_with_cookies`. Cookies sent without a bearer are dropped, so they never
  authenticate anything. Do not attach a cookie jar to the HTTP client (Dio).
- **Device id.** Send `X-Kalks-Device` on every call. If the app has no id yet, the first sign-in call (`login`,
  `register`, `verify-email`, `resend`, `forgot`, `reset`) mints one and returns it as `device`. Store it next to the
  session and keep it for the life of the install. The gateway trusts a device
  after its first email code, so a known device skips the new-device code at the next sign-in.
- **Trade token.** Kalks Trader calls need a second token, `X-Kalks-Trade`, for the trading account being used (see
  section 6). It is bound to the signed-in client and useless with anyone else's session.
- **Policies are the web's.** Most paths are rewrites of the Client Area's cookie routes, so the same rules apply
  unchanged: view-only logins (D90), read-only staff sessions, module switches, maintenance mode, step-up codes and the
  gateway's rate limits.

## 2. Headers

| Header | Where | Value |
|---|---|---|
| `Authorization` | signed-in calls | `Bearer <gateway session token>` (view-only tokens start with `v.`, staff sessions with `i.` / `s.`) |
| `X-Kalks-Device` | every call | base64url, 16–128 characters; minted by `auth/*` when missing |
| `X-Kalks-Platform` | every call | `android` or `ios` (default `android`). Recorded on orders as `Android` / `iOS`, and on sign-up attribution |
| `X-Kalks-App-Version` | every call | e.g. `1.0.0+12`; compare with `config.minAppVersion` |
| `X-Kalks-Locale` | every call | one of the 22 locales (`en`, `hi`, `ar`, …). The gateway writes codes, emails and some errors in it |
| `X-Kalks-Trade` | `trade/*` account calls | the trade token from `trade/sessions` or `trade/login` |
| `X-Kalks-Stepup` | step-up protected writes | alternative to the `stepup_token` body field |
| `User-Agent` | every call | please send something descriptive, e.g. `KalksApp/1.0.0 (Android 15; Pixel 8)`. The Security page shows it in the session list |
| `Content-Type` | writes | `application/json`, except the two uploads (section 8) |

## 3. Errors

Every error looks like `{"error": {"code": "...", "message": "..."}}`, sometimes with extra fields (`field`,
`attempts_left`, `retry_after`). The `message` is plain English and safe to show. Branch on `code`, and translate
the codes you know.

| Status | Code | Meaning / what the app does |
|---|---|---|
| 400 | `bearer_with_cookies` | A cookie was sent with the bearer token. This is a bug in the app. |
| 400 | `bad_request` | Bad query or body |
| 401 | `unauthorized` | No session, or it is dead (expired, signed out elsewhere, blocked). Go to sign-in. |
| 403 | `forbidden` | Not allowed |
| 403 | `viewer_read_only` / `viewer_scope` | A view-only login tried a change, or a section it was not given |
| 403 | `staff_read_only` | A read-only staff session ("log in as client") tried a change |
| 403 | `module_disabled` | The broker switched the module off. Hide it (see `config.modules`). |
| 403 | `stepup_required` / `stepup_invalid` | Run the step-up dialog (section 4), then retry with `stepup_token` |
| 404 | `not_found` | Unknown path or object |
| 409 | various | Business conflicts: `wrong_server`, `needs_empty`, `positions_open`, … |
| 410 | `code_expired` | An email code expired. Request a new one. |
| 413 | `too_large` | Upload over the limit |
| 415 | `bad_request` | Wrong `Content-Type` |
| 422 | `validation` (+ `field`) | Invalid input |
| 429 | `rate_limited` (+ `retry_after` seconds) | The gateway's limits on sign-in, codes and resends |
| 503 | `maintenance` | The broker's maintenance mode: show the maintenance screen (`config.maintenance`) |
| 503 | `unavailable` | An upstream service is down. Retry later. |

Trade-specific codes are listed in section 6.

## 4. Auth — `/api/mobile/auth/<action>` (native)

These make the same gateway calls as the web's `/api/auth/<action>`, but the session comes back in the body. No Google
sign-in yet.

| Call | Body | Answer |
|---|---|---|
| `POST login` | `{email, password}` (a view-only login uses its viewer ID as `email`) | `{status: "ok", user, session:{token, expires_at}, device?}`, or `{status: "otp_required", challenge, purpose, email_masked, expires_in, resend_in, device?}` for a new device or an unverified email |
| `POST verify-email` | `{challenge, code}` | `{status: "ok", user, session, device?}` |
| `POST resend` | `{challenge}` | a fresh challenge |
| `POST register` | the web form's fields: `{first_name, last_name, email, password, country, phone_dial, phone, date_of_birth, referral_code?, referral_campaign?, accept_terms, marketing_consent}` | `{status: "otp_required", challenge, …}`, then `verify-email` returns the session |
| `POST forgot` | `{email}` | `{challenge, …}` |
| `POST reset` | `{challenge, code, password}` | `{status: "ok"}` (sign in again afterwards) |
| `POST logout` | (bearer, no body needed) | `{status: "ok"}`. Then delete the session, every trade token and the cached data. |
| `GET me` | (bearer) | `{user, viewer, session:{id, idle_minutes, expires_at}, restrictions, …}`, the same as the web. Poll it every 30 s while the app is in the foreground. |
| `POST stepup` | (bearer) `{action, target}` | `{challenge, email_masked, expires_in, …}`: a 6-digit code is emailed |
| `POST stepup-resend` | (bearer) `{challenge}` | |
| `POST stepup-verify` | (bearer) `{challenge, code, action, target}` | `{stepup_token}`: single use, 5 minutes, bound to the action and target |
| `POST password` | (bearer) `{current, new, stepup_token, sign_out_others}` | `{sessions_revoked}` |
| `POST heartbeat` | (bearer) `{}` | presence plus the current restrictions. Call it every 45 s while in the foreground. This is the cookie route `/api/auth/heartbeat`. |
| `POST marketing` | (bearer) | the cookie route `/api/auth/marketing` (the email-preferences switch) |

**Step-up actions:** `trading_password`, `investor_password`, `leverage` and `account_archive` / `account_close`
(target: the login); `withdrawal`, `internal_transfer` (target: the from-login), `account_password`, `profile_email`,
`profile_phone` and `viewer_access`. The protected request then carries `stepup_token` in its body, or the
`X-Kalks-Stepup` header.

**Rate limits:** the gateway's own limits, applied per client IP and per email exactly as for the web
(`429 rate_limited` with `retry_after`). A wrong code answers `400 invalid_code` with `attempts_left`.

## 5. Config — `GET /api/mobile/config` (public)

No session needed. Read it at start-up and refresh it on resume. It still answers during maintenance.

```json
{
  "apiVersion": 1,
  "minAppVersion": null,
  "urls": {
    "app": "https://app.kalkstrade.com",
    "terminal": "https://trade.kalkstrade.com",
    "marketData": { "http": "https://api.kalkstrade.com", "ws": "wss://api.kalkstrade.com/v1/stream" },
    "streams": {
      "engine": "wss://trade.kalkstrade.com/engine/stream",
      "options": "wss://trade.kalkstrade.com/options/stream",
      "support": "wss://app.kalkstrade.com/support/stream"
    }
  },
  "tenant": { "slug": "kalks", "name": "Kalks", "default": true, "logoUrl": null, "primary": null, "accent": null, "supportEmail": null, "website": null },
  "modules": { "wallet": true, "prop": true, "ib": true, "academy": true, "copy_trading": true, "pamm": true, "mam": true, "algo": true, "api": true, "rewards": true, "options": true, "news": true, "calendar": true, "markets": true, "ai": true, "support_chat": true },
  "flags": { "demo_accounts": true },
  "maintenance": { "active": false, "message": "", "until": null }
}
```

- `tenant`: the broker's branding, from the same source as the web's `brandCss`. `default: true` means the stock
  Kalks look. `primary` re-tints the ember accent and `accent` the gold. Both are `#rrggbb` or null.
- `modules`: a module set to `false` is hidden, and its API answers `403 module_disabled`. Missing means on. The
  native trade routes follow `options` (`trade/options*`), `mam` (`trade/mam`) and `ai` (`trade/ai-trader`;
  `trade/options/explain` needs both `options` and `ai`); `support_chat` closes the chat routes of `support/*` only
  while `ai` is off too (Ask Kalks AI asks through them), the stream ticket stays open.
- `minAppVersion`: when set (env `MOBILE_MIN_APP_VERSION`), an older app must ask the user to update.
- A white-label broker's `terminal` and `engine` / `options` streams use its own trade domain.

## 6. Kalks Trader — `/api/mobile/trade/*` (native)

The same routes and validation as Kalks Trader's web BFF (`apps/terminal/app/api/engine/*`, `app/api/options/*`).

### Trade tokens

1. **Open one of the client's own accounts** (the Client Area Trade button):
   `POST trade/sessions {login}` with the bearer token. The server mints the engine session through the account SSO and
   returns `{token, expiresAt, readOnly, login, account}`. `account` is the engine's account view, without dealer
   fields and with `spreadGroup` added.
2. **Or log in MT5-style** (the Account tab's "add account login"):
   `POST trade/login {login, password, server?}` with the bearer token. `server` is `Kalks-Live` or `Kalks-Demo`.
   - A trading password gives full access, and an investor password gives read-only access (`readOnly: true`).
   - Any account works, as in MT5.
   - The answer has the same shape as `trade/sessions`.
   - A wrong server answers `409 wrong_server`.
   - A wrong password answers the engine's `401 invalid_credentials`, and repeated failures give `409 locked`.
3. Store the `token` (e.g. `kt1.s.…`) per login in the Keystore. Send it as `X-Kalks-Trade`, **together with**
   `Authorization: Bearer`, on every account call. It is bound to the signed-in client: with another client's session
   it answers `403 trade_session_foreign`.
4. `POST trade/sessions/check {tokens: [≤8]}` returns which stored tokens are still alive:
   `{sessions: [{alive, login?, readOnly?, expiresAt?, account?}]}`, in the same order. Use it for the account
   switcher on start.
5. `POST trade/logout` (with `X-Kalks-Trade`) ends that engine session.

Signing out of the gateway stops trading at once, because every trade call re-checks the bearer session.

| Status | Code | What the app does |
|---|---|---|
| 401 | `trade_session_required` | No `X-Kalks-Trade`: open the account first |
| 401 | `session_expired` | The engine session ended (expiry, password change, sign-out): call `trade/sessions` (own account) again, or ask for the password |
| 403 | `trade_session_foreign` | The token belongs to another client (or its SSO account changed owner): drop it |
| 403 | `read_only` | Investor session: trading is disabled |
| 4xx | engine codes | Kept as they are: `market_closed`, `no_money`, `invalid_sl`, `invalid_tp`, `invalid_volume`, `symbol_demo_only`, `trading_disabled`, `close_only`, … Show the message. |
| 422 | `product_mismatch` | CFD / Options account split: a CFD order on an Options account, or an option order on a CFD account. Show the message; the app should never send one (the active account's product decides the workspace). |

### CFD and Options accounts

Every account trades one product: `account.product` is `"cfd"` or `"options"` on every account view (`trade/state`,
`trade/sessions`, `trade/login`, `trade/sessions/check`, `trading/accounts`, `trading/accounts/{login}`), and every
group of `trading/groups` has `product` too (missing on an older server = `"cfd"`). The app opens the CFD workspace
for a CFD account and the options workspace for an Options account; its CFD | Options control switches between the
client's accounts of each product. The open-account wizard starts with the product (CFD account | Options account)
and lists only that product's groups; the account limit counts per (live / demo, product). A deep link
`/trader?mode=options` picks the client's Options account. While `config.modules.options` is `false`, no Options
account is offered and `POST trading/accounts` with an Options group answers `403 module_disabled`.

### Routes

| Call | Notes |
|---|---|
| `GET trade/symbols?tier=core\|catalogue&symbols=A,B` | **Public.** `{symbols: [contract specs: digits, point, pipSize, contractSize, lotMin/Max/Step, marginPct, maxLeverage, swapLong/Short + swapUnit (points \| percent_per_year), tripleSwapDay, session, open, core, liveTrading, …], live: [...], off: [...]}`. Live accounts and guests only see markets that are core or in `live`. Cached for 60 s. |
| `GET trade/notifications?before&limit&unread` · `POST trade/notifications/read {ids?\|all}` | The client's inbox: the same as the bell (`/api/mobile/notifications`). Bearer only. |
| `GET trade/state?historyLimit=0..500` | `{account (+spreadGroup), positions, orders, history:{deals}, readOnly, restrictions, staff, serverTime, expiresAt}` |
| `GET trade/history?from&to&page&limit` | closed deals + done pending orders |
| `GET trade/controls` | the broker's restrictions on the account (`tradingDisabled`, `closeOnly`, …) |
| `GET trade/mam?symbol&volume` | MAM role + allocation summary |
| `POST trade/orders` | `{symbol, side: buy\|sell, type: market\|limit\|stop\|stop_limit, volume, price?, stopLimit?, sl?, tp?, trailingPoints?, expiry?, expiryAt?, requestedPrice?, deviationPoints?, ocoWith?, comment? (≤31), clientOrderId?, source?: manual\|ai}`. `platform` is set from `X-Kalks-Platform`. |
| `PATCH trade/orders/{ticket}` · `DELETE trade/orders/{ticket}` | `{price?, stopLimit?, volume?, sl?, tp?, trailingPoints?, expiry?, expiryAt?}` (null clears) |
| `POST trade/positions/{ticket}/close` | `{volume?, deviationPoints?, requestedPrice?}`. Without `volume` the whole position closes. |
| `PATCH trade/positions/{ticket}` | `{sl?, tp?, trailingPoints?}` (null clears) |
| `POST trade/positions/close-by` | `{ticket, by}` (hedging accounts) |
| `POST trade/bulk-close` | `{filter: all\|profitable\|losing\|pending\|buys\|sells, symbol?}` |
| `POST trade/demo-refill` | demo accounts only |
| `POST trade/stream-ticket` | `{ticket, expiresIn: 30, url}`. Open `url?ticket=<ticket>` within 30 s (section 7). |
| `POST trade/options/preview` · `POST trade/options/orders` | `{legs:[{series, side, contracts, barrier?}], type: market\|limit, limitPremium?, sl?, tp?, trigger?, tif?, clientOrderId}` (orders need `clientOrderId`, 8–64 characters) |
| `POST trade/options/combos/{comboId}/close` · `GET trade/options/settlements?limit` | |
| `POST trade/options/book/preview` · `POST\|GET\|DELETE trade/options/book/orders` · `PATCH\|DELETE trade/options/book/orders/{id}` · `GET trade/options/book/fills` | the order book (while the book is on) |
| `POST trade/options/rfq` · `GET\|DELETE trade/options/rfq/{id}` · `POST trade/options/rfq/{id}/accept` | combo RFQ |
| `GET trade/options/underlyings` · `expiries?u=` · `chain?u=&expiry=` · `series/{code}` · `candles?series&tf&limit&to` · `smile?u=&expiry=` | the options service, priced for the account's group. While the module is off for the account kind, these answer `404 options_disabled` ("launching soon"). |
| `POST trade/options/stream-ticket` | `{ticket, expiresIn, url}` for the chain stream |
| `GET trade/options/public/book/{series}` · `public/trades/{series}?limit` · `public/stats/{u}` | order-book market data (bearer only, no trade token) |
| `POST trade/options/explain` | "Explain it to me": `{locale, strategy}` → `{configured, text}` |
| `POST trade/ai-trader` | `{prompt, symbol?, timeframe?}` → `{configured, result: {status, questions, assumptions, strategy}, warnings}` |

AI calls cost money. They need a live, non-view-only session and are limited to 10 a minute and 200 a day per
client:
- `401 signin`;
- `403 forbidden`;
- `429 rate_minute` / `rate_day` with `retry-after`.

Without a server key they answer `{configured: false}`, and the app uses its built-in text or local parser. A `GET` on
either route returns `{configured, model}`.

## 7. Streams

| Stream | URL | Auth |
|---|---|---|
| Quotes | `config.urls.marketData.ws` + `?group=<account.spreadGroup>` | none. Subscribe passively or actively like the web (`packages/mock/src/prices.ts`). Only visible rows subscribe actively. |
| Account (engine) | the `url` from `POST trade/stream-ticket` (= `config.urls.streams.engine`) + `?ticket=` | one-time ticket, 30 s. Frames: snapshot / position / order / deal / ledger / account / notification / equity / hb / resync / ended. On `ended`, get a new ticket; on `401` from the ticket call, re-open the session. |
| Options chain | the `url` from `POST trade/options/stream-ticket` + `?ticket=` | one-time ticket |
| Support chat + bell | `config.urls.streams.support` + `?ticket=` | `POST /api/mobile/support/stream-ticket` → `{ticket, url}`. In production `url` is `null`, so always use `config.urls.streams.support`. Frames as on the web (`apps/crm/lib/realtime.ts`): `ping`, `resync`, chat and notification frames. |

Reconnect with backoff (1 s → 20 s), getting a fresh ticket each time.

## 8. The Client Area routes (rewrites)

`/api/mobile/<family>/<rest>` is served by the web's cookie route `/api/<family>/<rest>`, with the bearer session.
The request and answer are the same as the web's, and so are the policies. The documentation of each family is the
header comment of its route file in `apps/crm/app/api/<family>/…/route.ts`.

| Family | What |
|---|---|
| `trading` | accounts (list, open, detail, history, ledger, CSV export, history ZIP), groups, passwords / leverage (step-up), rename / archive / restore / close, group change, demo balance / refill, prefs (default account), transfers between accounts, `accounts/{login}/sso` (web URL only; the app uses `trade/sessions`) |
| `wallet` | config, overview, deposits (addresses, hash submit), withdrawals (quote, create with step-up), transfers, activity, ledger, notifications |
| `news` | feed, article, map, sources, brief, economic calendar and reminders |
| `notifications` | the bell: inbox, `read`, `clear`, `prefs` |
| `kyc` | status, `start`, `details`, `documents` (upload), `submit` |
| `security` | sessions, sign-in history, view-only logins, closure / data-export requests |
| `support` | chat (`me`, `messages`, `handover`, conversations, `read`, `typing`, rate / resolve), attachments, `stream-ticket`. **Ask Kalks AI** is the support assistant in this chat: post a message and the reply streams over the support stream. |
| `status` | public status page data |
| `growth` | rewards, points, redeem, cashback, promotions, bonuses, promo codes, contests, banners, share cards |
| `partner` | IB dashboard, programme, campaigns, clients, network, commissions, payouts, settings |
| `social` | copy trading (leaderboard, masters, subscriptions) and PAMM (funds, investments) |
| `prop` | prop plans, challenges, payouts, certificates, notifications |
| `academy` | catalogue, chapters, quizzes, exams, certificates, glossary |
| `reports` | analytics, monthly figures, statements (PDF / CSV / XLSX downloads) |
| `algo` | strategies, backtests, deployments, marketplace, AI, API keys, webhooks, kill switch |
| `suitability` | Kalks FX Options onboarding: disclosure, accept, quiz |

**Uploads:**
- KYC: `POST /api/mobile/kyc/documents` as `multipart/form-data` with `file`, `kind`, `side?`, `party?`, `issue_date?`,
  `doc_type?` and `checks?`. Up to 10 MB.
- Support attachments: `POST /api/mobile/support/attachments` with the raw file bytes as the body, `Content-Type` set
  to the file's type (`image/*` or `application/pdf`), and the `X-File-Name: <percent-encoded name>` header. Up to
  10 MB. The answer's `attachment.id` goes into `POST messages {body, attachmentId}`.

**Downloads:** statements, CSV, ZIP and attachments come back as files, with `Content-Disposition`.

## 9. Local development

- Run the stack with `scripts/dev-services.sh` (Postgres :5433, services, apps). The crm runs on :3000.
- On a USB phone, use `adb reverse tcp:3000 tcp:3000` and do the same for the service ports in the config (8081
  market-data, 8090 engine stream, 8104 options, 8100 support). On a local stack, `config.urls` points at
  `localhost` / `127.0.0.1` ports, so the phone reaches them through the reverse tunnels.
- The email codes are in the gateway log (`~/.kalks-local/gateway.log`), or in the dev mail sink when one is
  configured.
- Server env (apps/crm), all optional:
  - `MOBILE_MIN_APP_VERSION`;
  - `MOBILE_MARKET_DATA_URL`, `MOBILE_ENGINE_STREAM_URL`, `MOBILE_OPTIONS_STREAM_URL` and `MOBILE_SUPPORT_STREAM_URL`
    (pin a URL);
  - `MOBILE_TRADE_SECRET` (the trade-token binding key; default: derived from the internal tokens);
  - `ANTHROPIC_API_KEY` (the AI routes);
  - `OPTIONS_URL` / `OPTIONS_INTERNAL_TOKEN` (the options service; already set by the deploy script).

## 10. Security notes

- The bearer token is the only credential. A browser can't add `Authorization` to a cross-site request without a CORS
  preflight, and the app grants no CORS. Cookies on `/api/mobile/*` are dropped, and refused next to a bearer token.
- Rewritten requests get the bearer as their session cookie and pass the cookie routes' same-origin check. This
  happens **only** for bearer requests that carry no cookies; without a bearer, the cookie routes still refuse
  cross-site writes.
- Trade tokens are `kt1.<s|p>.<engine session>.<HMAC>`, keyed with a server-only secret. They are bound to the client
  id, and SSO sessions are also re-checked against the account owner (cached 60 s).
- Tokens are never logged by the BFF. Engine and options internal tokens never leave the server. Dealer fields
  (book, route, owner ids, ledger ids) are stripped from every answer.

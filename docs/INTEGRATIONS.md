# Kalks: integrations checklist (founder guide)

This is every external service Kalks connects to, what the Super Admin has to paste in, where to get it, and whether you need it on launch day.

It mirrors **Back Office → Settings → Integrations** (`/settings/integrations`), where each item has a card with the same fields, a **Test connection** button and a docs link. The page data lives in `packages/mock/src/admin-platform-settings.ts` (`SET_INTEGRATIONS`), so keep both in sync.

**How secrets are handled:**
- All secrets are encrypted at rest (AES-256, KMS envelope keys) and shown masked.
- Every change is written to the immutable admin audit log.
- White-label tenants inherit these defaults. Enterprise tenants can bring their own email, SMS and KYC credentials.

**Launch levels:**
- **Required:** you cannot go live without it.
- **Recommended:** launch works without it, but in a degraded or fallback mode.
- **Optional:** nice to have.
- **Post-launch:** intentionally off at launch.

---

## Summary

| # | Integration | Provider (default) | Launch | Status in mock |
|---|---|---|---|---|
| 1 | Market data feed | Infoways (+ backup) | **Required** | Connected |
| 2 | Transactional email | Google Workspace SMTP relay (in use); SES / SendGrid / Postmark also supported | **Required** | Connected |
| 3 | SMS & WhatsApp OTP | Twilio / MSG91 / Gupshup | **Required** | Connected |
| 4 | KYC verification | Sumsub | **Required** | Connected |
| 5 | USDT-TRC20 blockchain | TronGrid + own wallets | **Required** | Connected |
| 6 | Object storage | AWS S3 / MinIO | **Required** | Connected |
| 7 | AI assistant | Anthropic Claude | Recommended | Connected |
| 8 | Error monitoring | Sentry | Recommended | Connected |
| 9 | Economic calendar & news | FXStreet / Trading Economics / Finnhub + RSS | Recommended | Error (1 RSS feed returns 403) |
| 10 | Web push | VAPID keys | Recommended | Not configured |
| 11 | Google sign-in | Google OAuth 2.0 | Optional | Connected |
| 12 | Web analytics & pixels | GA4 + Meta Pixel / CAPI | Optional | Not configured |
| 13 | Liquidity provider | FIX 4.4 session | Post-launch | Disabled (B-book at launch) |
| 14 | Crypto payments | NOWPayments | Optional | Live setting: Finance → Wallet settings (off until keys are pasted) |

**At launch: 9 of 14 configured, all 6 required integrations ready.**

Charts are not a third-party integration: the Kalks Trader terminal, Client Area and Back Office use Kalks' own in-house chart, fed by the Kalks market-data service (`services/market-data`). No chart licence is needed.

---

## 1. Market data feed: Infoways · Required
Real-time quotes for all asset classes. The feed drives the pricing engine, the terminals and the Client Area.

| Field | Example / notes |
|---|---|
| API key (secret) | `iw_live_…`. Needs a commercial redistribution licence. |
| WebSocket URL | `wss://stream.infoways.io/v2/quotes` |
| REST URL | `https://api.infoways.io/v2` |
| Symbols mapping | Managed in **Brokers → Global symbols** (Kalks symbol ↔ feed symbol). |
| Backup feed provider | None / Twelve Data / Polygon.io / Finnhub |
| Backup feed API key (secret) | From the backup provider's dashboard |

**Where to get it:** Infoways client portal → API → Keys. Sign the redistribution agreement first.

## 2. Transactional email · Required
Handles the Welcome, OTP, deposit, withdrawal, KYC and margin-call emails. The templates themselves are in **Content → Email templates**.

**In production today:** Google Workspace for kalkstrade.com.
- MX is `smtp.google.com`. SPF is `include:_spf.google.com`, plus Google DKIM and DMARC (`p=none` for now).
- The services send through `smtp-relay.gmail.com:587` with STARTTLS. The relay trusts the server's IP, so there is no `SMTP_USER` / `SMTP_PASSWORD`.
- Mail goes from `no-reply@kalkstrade.com`, with replies to `support@kalkstrade.com`.
- The server IP must stay allowed in Google Admin → Apps → Gmail → Routing → SMTP relay. Update it when the server changes.

Other providers work through the same `SMTP_*` settings. Choose **SMTP** or **API** mode.

| Field | Example / notes |
|---|---|
| SMTP host / port | `email-smtp.eu-central-1.amazonaws.com` / `587` |
| Username / password (secret) | SES SMTP credentials |
| STARTTLS | On |
| *or* API provider + API key (secret) | Amazon SES / SendGrid / Postmark |
| From name / From email | `Kalks Markets` / `no-reply@kalkstrade.com` |

**Where to get it:**
1. In the AWS console, go to SES and verify the domain `kalkstrade.com`.
2. Go to SMTP settings and create SMTP credentials.
3. In your DNS, add the SPF, DKIM and DMARC records. The card shows whether each one passes.

## 3. SMS & WhatsApp OTP · Required
Sends the codes for sign-up, withdrawals and the 2FA fallback. WhatsApp goes first; SMS is the fallback.

| Field | Example / notes |
|---|---|
| Provider | Twilio / MSG91 / Gupshup |
| Account SID | `AC…` |
| Auth token (secret) | Provider console |
| Sender ID | `KALKS` (register it in countries that require it, e.g. India DLT) |
| WhatsApp template | `kalks_otp_v2`, approved in Meta Business Manager |
| WhatsApp number | The business number linked to the provider |

**Where to get it:**
- Twilio console → Account info (SID and token).
- Twilio → Messaging → Senders.
- Meta Business Manager → WhatsApp templates.

## 4. KYC verification: Sumsub · Required
Runs ID document, liveness and proof-of-address checks. The results arrive in **Clients → KYC queue**.

| Field | Example / notes |
|---|---|
| App token (secret) | `prd:…` |
| Secret key (secret) | Paired with the app token |
| Webhook secret (secret) | From the webhook you create in Sumsub |
| Level name | `kalks-basic-kyc-level` |
| Webhook URL (read-only) | Paste `https://api.kalkstrade.com/webhooks/sumsub` into Sumsub |

**Where to get it:**
- Sumsub dashboard → Dev space → App tokens.
- Sumsub → Webhooks.
- Sumsub → Levels.

## 5. USDT-TRC20 blockchain: TronGrid · Required
Covers the whole USDT flow:
- Per-client HD deposit addresses
- The confirmation watcher
- Sweeps to the hot wallet
- Withdrawals

| Field | Example / notes |
|---|---|
| TronGrid API key (secret) | trongrid.io dashboard |
| Full node URL | `https://api.trongrid.io`, or your own node |
| Hot wallet address | `TQ7x…9KfE`. Keep TRX in it for energy/bandwidth. |
| HD xpub (secret) | Extended **public** key used to derive deposit addresses. Never paste a private key or seed. |
| Confirmations required | `20` blocks |
| Sweep threshold | `500` USDT |
| Cold wallet address | Treasury address that receives overflow above the hot-wallet cap |

**Where to get it:**
- The API key comes from trongrid.io.
- The hot wallet and xpub come from your custody setup (hardware wallet or MPC provider).
- The cold wallet address comes from treasury.

Changes to wallet fields require four-eyes approval.

## 6. Object storage: S3 / MinIO · Required
Stores KYC documents, statements, invoices, avatars and report exports. Encryption is server-side.

| Field | Example / notes |
|---|---|
| Endpoint | `https://s3.eu-central-1.amazonaws.com` or your MinIO URL |
| Bucket | `kalks-prod-private` (private, versioning on) |
| Access key | IAM user limited to this bucket |
| Secret key (secret) | Paired with the access key |

**Where to get it:** AWS IAM (or the MinIO console). Create the bucket, then create an access key scoped to that bucket.

**Website and app images (content service):** the media library (Back Office → Content → Media library) stores its images on the server's disk today (`CONTENT_STORAGE_DIR`). To move them to Cloudflare R2 (or any S3-compatible bucket), set in `~/kalks/.env.local`: `CONTENT_STORAGE=r2`, `R2_ENDPOINT` (`https://<account id>.r2.cloudflarestorage.com`), `R2_BUCKET`, `R2_ACCESS_KEY_ID`, `R2_SECRET_ACCESS_KEY` (an R2 API token with Object Read & Write on that bucket), optional `R2_REGION` (default `auto`). Copy the existing files into the bucket with the same keys before switching. Optional; disk works for launch. See docs/CONTENT.md.

## 7. AI assistant: Anthropic Claude · Recommended
Powers three features:
- The support bot, which answers first and hands over to human agents.
- The AI coach.
- Knowledge-base search.

| Field | Example / notes |
|---|---|
| Claude API key (secret) | `sk-ant-api03-…` |
| Model | `claude-sonnet-5` (default) |
| Monthly budget | e.g. `3000` USD. The bot hands off to human agents when the budget is reached. |

**Where to get it:** console.anthropic.com → Settings → API keys. Also set a workspace spend limit there.

**Without it:** support runs human-only and AI features are hidden.

## 8. Error monitoring: Sentry · Recommended
| Field | Example / notes |
|---|---|
| Sentry DSN (secret) | `https://…@o44810.ingest.sentry.io/451872` |
| Environment | production / staging |
| Traces sample rate | `0.2` |

**Where to get it:** sentry.io → Project → Settings → Client Keys (DSN).

## 9. Economic calendar & news · Recommended
| Field | Example / notes |
|---|---|
| Calendar provider | FXStreet / Trading Economics / Finnhub |
| Provider API key (secret) | From provider sales or dashboard |
| RSS feed URLs (list) | e.g. FXStreet, Investing.com, Cointelegraph. Feeds that return errors are flagged red on the card. |

News is curated (pin/hide/tags) in **Content → News**.

## 10. Web push: VAPID · Recommended
Browser/PWA notifications for price alerts, margin calls and deposit confirmations.

| Field | Example / notes |
|---|---|
| VAPID public key | Generated on the card ("Generate key pair") |
| VAPID private key (secret) | Generated together with the public key |
| Subject | `mailto:ops@kalkstrade.com` |

**Where to get it:** no external account is needed. Generate the keys on the card or with `npx web-push generate-vapid-keys`.

## 11. Google sign-in · Optional
"Continue with Google" on the Client Area sign-in and sign-up pages (D27). New Google users finish a short profile step (country, phone, date of birth, referral code, terms) before the account is created; their email counts as verified.

| Field | Example / notes |
|---|---|
| Client ID | `…apps.googleusercontent.com` → `GOOGLE_CLIENT_ID` in `apps/crm/.env.production.local` |
| Client secret (secret) | `GOCSPX-…` → `GOOGLE_CLIENT_SECRET` in `apps/crm/.env.production.local` |
| Show the button | `NEXT_PUBLIC_GOOGLE_LOGIN=1` in the same file (read at build time, so rebuild after changing it) |
| Authorised redirect URI | `https://app.kalkstrade.com/api/auth/google/callback` (add `http://localhost:3000/api/auth/google/callback` too if you want to test locally) |
| Authorised JavaScript origin | not needed (the flow is server-side) |

**Where to get it:** Google Cloud console → APIs & Services → Credentials → OAuth client ID (Web application). On the OAuth consent screen, use the scopes `openid`, `email` and `profile` only, add the app name, logo, support email, and the privacy policy and terms links, then publish the app ("In production"). While it is in "Testing", only the listed test users can sign in.

## 12. Web analytics & pixels · Optional
Sends sign-up, KYC and FTD conversions with UTM parameters. This feeds **Marketing → Campaigns** attribution.

| Field | Example / notes |
|---|---|
| GA4 measurement ID | `G-XXXXXXXXXX` |
| Meta Pixel ID | numeric |
| Conversions API token (secret) | Meta Events Manager |

## 13. Liquidity provider: FIX 4.4 · Post-launch (disabled)
**Kalks runs 100% B-book at launch.** When an LP or prime-of-prime is onboarded:
1. Enable the session on the card.
2. Fill in the fields below.
3. Set the A-book rules in **Trading → Book & routing**.

| Field | Example / notes |
|---|---|
| FIX host / port | From the LP's session sheet |
| SenderCompID | e.g. `KALKS_UAT`, then `KALKS_PROD` |
| TargetCompID | e.g. `LPPRIME` |

## 14. Crypto payments: NOWPayments · Optional
Lets clients deposit in BTC, ETH, LTC, TRX, BNB and USDT on TRC20 / BEP20 / ERC20 (and any other coin you enable); the USD amount is credited to their wallet as USDT. Deposits only (no payouts). The direct USDT BEP20 / TRC20 deposits stay as they are. This one is live, not mock: it is configured in **Back Office → Finance → Wallet settings → NOWPayments** (not on the Integrations page). Details: `services/wallet/README.md#nowpayments`.

| Field | Example / notes |
|---|---|
| API key (secret) | NOWPayments dashboard → Settings → Payments → API keys. Write-only: the Back Office shows "Set ✓ …1234". |
| IPN secret (secret) | NOWPayments dashboard → Settings → Payments → Instant payment notifications → IPN secret key |
| IPN callback URL (copy) | `https://app.kalkstrade.com/api/payments/nowpayments/ipn`: paste it in the same NOWPayments page |
| Coins | Picked from the coins enabled in the merchant account ("Test connection") |
| Minimum / maximum deposit | `10` / `50,000` USD |
| Client pays the fees | On (NOWPayments' network and service fees are added to what the client pays) |
| Sandbox | api-sandbox.nowpayments.io with sandbox keys, for a test run before going live |

**Where to get it:** sign up at nowpayments.io, finish the merchant verification, set the payout (outcome) wallet, enable the coins, then create the API key and the IPN secret. Keys are encrypted at rest with the wallet's `WALLET_SECRETS_KEY` and every change is audited without the secret values.

---

## Go-live checklist (also shown on the page)
- [x] Domains & SSL: `app.` / `trade.` / `admin.` / `api.kalkstrade.com` verified (Settings → General)
- [x] Market data streaming: 28 symbols
- [x] Email sender authenticated: SPF, DKIM, DMARC pass
- [x] OTP delivery tested: WhatsApp and SMS fallback
- [x] KYC level and webhook live
- [x] Hot and cold wallets set, with TRX in the hot wallet for energy
- [x] Payment limits reviewed: USDT-TRC20 min/max and auto-approve (Settings → Payment methods)
- [x] Legal documents published (Content → Legal)
- [ ] Fix the news RSS feed error
- [ ] Generate web push VAPID keys
- [ ] `status.kalkstrade.com` DNS record

## Other settings that are not third-party integrations but must be filled in
- **General & branding:**
  - Logo, mark and colours
  - Legal entity, registration number, regulator/licence and address
  - Risk warning text
  - Support email, phone and WhatsApp
  - Server timezone (GMT+3) and default language
- **Payment methods:** USDT-TRC20 limits, withdrawal fee, auto-approve threshold, and per-group limits.
- **Countries:** blocked and restricted jurisdictions, plus IP-geo and VPN detection.
- **Webhooks:** outgoing endpoints for your CRM or data warehouse (optional).
- **Maintenance:** staff bypass IPs and the weekly maintenance window.

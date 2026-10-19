# Kalks

White-label multi-asset trading platform: three Next.js apps (Client Area CRM, Back Office admin, Kalks Trader terminal) plus a Rust market-data service for live prices, candles and spreads.
Live builds (default) show only real data; `NEXT_PUBLIC_KALKS_MODE=demo` builds show the full mock showcase (see `packages/mock/src/mode.ts`).

## Run locally

Requires Node 22+ and pnpm.

```bash
pnpm install        # also copies shared assets into each app
pnpm dev            # starts all apps
```

| App | URL |
|---|---|
| Client Area | http://localhost:3000 (sign-in at `/login`) |
| Back Office | http://localhost:3001 (staff sign-in at `/login`) |
| Kalks Trader (trading room) | http://localhost:3002 (opened from any **Trade** button, or MT5-style login at `/login`) |

Run a single app with `pnpm dev:crm`, `pnpm dev:admin` or `pnpm dev:terminal` (or `pnpm --filter @kalks/<crm|admin|terminal> dev`). Build with `pnpm build`.

## Market data

The Rust market-data service serves prices, candles and spreads to every app. It needs Rust (cargo) and PostgreSQL 16 listening on port 5433.

```bash
cargo run -p market-data            # http://127.0.0.1:8081
```

It creates its database and runs migrations on first start. Provider keys (`INFOWAY_*`) are read from `.env.local` at the repo root; never commit that file. Details, endpoints and settings: [services/market-data/README.md](services/market-data/README.md).

## Sign-in (gateway)

The Rust gateway handles client and staff sign-in, sessions and the audit log. It uses PostgreSQL on port 5433 (database `kalks_core`, created and migrated on first start).

```bash
cargo run -p gateway                # http://127.0.0.1:8080 (only the apps call it, never the browser)
```

Its settings (`GATEWAY_*`, `SESSION_SECRET`, `SUPER_ADMIN_*`) live in `.env.local` at the repo root, and each app's `.env.local` holds `GATEWAY_URL` and `GATEWAY_INTERNAL_TOKEN`. The super-admin account is created once on first start. The Back Office reads clients, the audit log, staff and sessions through the gateway's staff-authenticated `/v1/admin/*` endpoints (`services/gateway/src/admin.rs`), and edits spread markups through market-data with `MARKET_DATA_URL` / `MARKET_DATA_ADMIN_TOKEN` in `apps/admin/.env.local` (same token as `MARKET_DATA_ADMIN_TOKEN` in the root `.env.local`; see `apps/admin/.env.example`). Email sending is not configured yet, so in development the sign-in codes appear on screen and in the gateway log.

"Continue with Google" runs in the Client Area's route handlers (`apps/crm/app/api/auth/google/*`, `apps/crm/lib/google-oauth.ts`): state + PKCE + nonce, server-side code exchange, ID-token verification against Google's keys, then the gateway's `/v1/auth/google` endpoints. It needs `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` and `NEXT_PUBLIC_GOOGLE_LOGIN=1` in `apps/crm/.env.local` (see `apps/crm/.env.example`), and the Google client must list `<origin>/api/auth/google/callback` as a redirect URI (`http://localhost:3000/...` for local testing). Tests: `cargo test -p gateway` and `pnpm --filter ./apps/crm test`.

## Identity verification (KYC)

Manual review by compliance staff with automatic pre-checks, in the gateway (`services/gateway/src/kyc.rs`, `kyc/staff.rs`). Clients verify at `/profile/verification` (Client Area BFF `apps/crm/app/api/kyc`): details, ID (camera with a frame guide or file), proof of address (< 3 months), selfie; corporate adds company details, directors / UBOs and their IDs. The browser checks sharpness, glare, lighting, framing, the passport MRZ and the face before upload; the gateway sniffs the file type, checks size, resolution and dates, flags files already used by another client, and stores each file AES-256-GCM encrypted under `KYC_STORAGE_DIR` (default `~/.kalks-data/kyc`) with `KYC_ENCRYPTION_KEY` (64 hex chars; development falls back to a key derived from `SESSION_SECRET`). Staff review at `/clients/kyc` (permissions `kyc.read` / `kyc.review`: admins and Compliance); documents stream only through the authenticated Back Office endpoint. Decisions set `users.kyc_status` (`verified` unlocks withdrawals, and locks name and date of birth), are audited, and email the client (logged as `DEV email` when SMTP isn't configured). `gateway kyc-erase-user <id>` removes a client's KYC files and rows.

## Trading engine

The Rust trading engine runs accounts, orders, positions, margin, swaps, the double-entry ledger and the dealing desk. It uses PostgreSQL on port 5433 (database `kalks_trading`, created and migrated on first start) and takes prices from market-data.

```bash
cargo run -p trading                # http://127.0.0.1:8090 (BFFs and internal services only)
cargo test -p trading
```

Its settings (`TRADING_*`) live in `.env.local` at the repo root. API, architecture and integration guide: [services/trading/README.md](services/trading/README.md).

## IB / referral programme

Every client is also an IB (partner). The Rust IB service mirrors the referral tree from the gateway, reads closed live deals from the trading engine, and computes multi-tier per-lot commissions, CPA bonuses, level upgrades, campaign funnels and payout batches that the Back Office approves into client wallets. It uses PostgreSQL on port 5433 (database `kalks_ib`, created and migrated on first start).

```bash
cargo run -p ib                    # http://127.0.0.1:8096 (BFFs and internal services only)
cargo test -p ib
```

Its settings (`IB_*`) live in `.env.local` at the repo root; the Client Area and Back Office need `IB_URL` / `IB_INTERNAL_TOKEN` in their `.env.local`. Partner links are `/r/CODE[/campaign]` on the Client Area. Rules, API and integration guide: [services/ib/README.md](services/ib/README.md).

## Support and notifications

Live chat in the Client Area (Support page and a floating button on every page) with a Claude-powered help bot that answers from the knowledge base and hands over to agents in the Back Office inbox (Support), plus the notification centre: the bell in both apps, preferences under Profile -> Notifications, emails, Back Office broadcasts (Content -> Notifications) and `POST /v1/notify` for other services. PostgreSQL on port 5433 (database `kalks_support`, created and migrated on first start).

```bash
cargo run -p support               # http://127.0.0.1:8100 (BFFs and internal services only)
cargo test -p support
```

Its settings (`SUPPORT_*`) live in `.env.local` at the repo root and the Claude key in `.env.claude`; the Client Area and Back Office need `SUPPORT_URL` / `SUPPORT_INTERNAL_TOKEN` in their `.env.local`. API, notification types and integration guide: [services/support/README.md](services/support/README.md).

## Rewards and marketing (growth)

The Rust growth service runs loyalty points per lot with tiers and a redemption catalogue, cashback programmes, demo / live trading contests with live leaderboards and prizes, deposit / credit bonus campaigns that release per lot (posted to the engine's bonus sub-ledger), promo codes, targeted banners and share P&L cards. It reads closed deals from the trading engine and pays cash rewards through the wallet. It uses PostgreSQL on port 5433 (database `kalks_growth`, created and migrated on first start).

```bash
cargo run -p growth                # http://127.0.0.1:8101 (BFFs and internal services only)
cargo test -p growth
```

Its settings (`GROWTH_*`) live in `.env.local` at the repo root; the Client Area and Back Office need `GROWTH_URL` / `GROWTH_INTERNAL_TOKEN` in their `.env.local`. Client pages are under `/rewards` (contests, loyalty, cashback, promotions), public share cards at `/s/CODE`; staff pages under `/marketing` (permissions `marketing.read|write|approve`). Rules, API and integration guide: [services/growth/README.md](services/growth/README.md).

## Content and media library

Editable copy and pictures for the website, the Client Area and Kalks Trader: the Rust content service keeps a media library (PNG / JPEG / WebP / SVG up to 10 MB, a light and an optional dark-theme file, resized WebP variants, alt text per language) and content slots per surface / page / slot / locale with draft → publish versions, rollback (last 50 kept) and English fallback. Staff edit in the Back Office under Content → Pages and Content → Media library (permissions `content.read|write|publish`); the apps read the public endpoint server-side and keep their built-in copy for every empty slot, so nothing changes until something is published. It uses PostgreSQL on port 5433 (database `kalks_content`, created and migrated on first start) and stores files under `CONTENT_STORAGE_DIR` (default `~/.kalks-data/content`; Cloudflare R2 with `CONTENT_STORAGE=r2`).

```bash
cargo run -p content               # http://127.0.0.1:8105 (public read + BFFs)
cargo test -p content
```

Its settings (`CONTENT_*`, `R2_*`) live in `.env.local` at the repo root; the apps need `CONTENT_URL` and the Back Office also `CONTENT_INTERNAL_TOKEN`. API, slot catalogue and how to add a slot: [docs/CONTENT.md](docs/CONTENT.md).

## Layout

```
apps/crm        Client Area — Next.js 16 (port 3000)
apps/admin      Back Office — Next.js 16 (port 3001)
apps/terminal   Kalks Trader — standalone MT5/cTrader-style trading room (port 3002)
packages/ui     Design system: tokens, shell, charts, tables, effects
packages/mock   Mock data + price feed client (live from market-data, simulator fallback)
services/market-data  Rust market-data service (prices, candles, spreads) — :8081
services/gateway      Rust sign-in service (clients, staff, sessions, audit log) — :8080
services/trading      Rust trading engine (accounts, orders, margin, ledger, dealing) — :8090
services/ib           Rust IB / referral programme (tree, commissions, payouts) — :8096
services/growth       Rust rewards + marketing (loyalty, cashback, contests, bonuses, promos, banners, share cards) — :8101
services/support      Rust support chat + AI help bot + notifications — :8100
services/content      Rust content + media library (editable pages, light / dark images, versions) — :8105
config/         Instrument catalogue + trading contract specs
brand/          Logo sources
scripts/        Asset sync
assets/         Shared assets: app, brand, coins, stocks, people, photos
docs/           Conventions, integrations list, website content PDF
.superdesign/   Design system spec + reference screenshots
```

## Docs

- `docs/CONVENTIONS.md`: how pages and components are built
- `docs/INTEGRATIONS.md`: every key and credential the Super Admin needs to go live
- `docs/CONTENT.md`: content service API, slot catalogue, preview links, storage (disk / R2)
- `docs/Kalks-Website-Content.pdf`: full website copy and build spec for kalkstrade.com

# ib

The Kalks IB / referral programme (Round 3, D53–D64). Every client is an IB from sign-up. The service mirrors the referral tree from the gateway, reads closed live deals from the trading engine, computes multi-tier per-lot commissions with client rebates and sub-IB splits, pays CPA bonuses, upgrades levels monthly, tracks campaign links, raises fraud flags, and pays approved batches into client wallets. It is a Rust service (axum 0.8, sqlx 0.9, PostgreSQL) on `127.0.0.1:8096`.

- [Run locally](#run-locally)
- [How it works](#how-it-works)
- [Programme rules](#programme-rules)
- [Data model](#data-model)
- [API](#api)
- [How the apps integrate](#how-the-apps-integrate)
- [Environment](#environment)
- [Tests](#tests)
- [Known gaps](#known-gaps)

## Run locally

You need PostgreSQL on `127.0.0.1:5433` (user `postgres`, trust auth), the gateway on `:8080` and the trading engine on `:8090`. The wallet on `:8095` is optional: without it, approved payouts wait in `transfer_pending` and are retried.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p ib
(cd services/ib && IB_LOG_FORMAT=pretty nohup ../../target/debug/ib > ~/.kalks-local/ib.log 2>&1 &)
curl -s localhost:8096/health
cargo test -p ib
```

On first start the service creates the `kalks_ib` database, runs `migrations/`, and seeds the default programme (settings and the Bronze → Diamond levels) for tenant `kalks`. It reads `IB_*`, `GATEWAY_INTERNAL_TOKEN`, `TRADING_INTERNAL_TOKEN` and `WALLET_INTERNAL_TOKEN` from the repo-root `.env.local`.

## How it works

```
 gateway /v1/internal/referrals/users ──(keyset cursor, every 10 s)──► members (tree, signals)
 engine  /v1/dealing/deals ──(close-time cursor + 2 min overlap, every 5 s)──► deals ─► commission lines
 engine  /v1/admin/accounts, /v1/accounts/{login}/ledger ──► account kind/group/cent, first deposit (CPA)
 copy/PAMM service ──POST /v1/ib/events/lots──► deals (source pamm|copy)
 wallet service    ──POST /v1/ib/events/deposit──► first deposit (CPA)
 scheduler ──► payout batch (pending approval) ──admin approve──► POST wallet /v1/wallets/transfers (ib_payout)
```

| Loop | Period | What it does |
|---|---|---|
| gateway-sync | `IB_SYNC_SECS` (10 s) | pulls new and changed clients; sets upline and campaign once (permanent attribution); re-checks self-referral signals against the upline |
| engine-deals | `IB_DEALS_SECS` (5 s) | polls closing deals per tenant, processes each once |
| deposits | 60 s | first live deposit of referred clients who don't have one yet (each client at most every 5 min) |
| reversals | 10 min | deals the dealing desk reopened in the last 14 days: void or claw back their commission |
| auto-batches | 5 min | when a payout period has closed, creates its batch for approval (once per period) |
| payouts | 30 s, or at once on approval | credits approved payouts to wallets; retries with backoff (1, 2, 4 … 60 min) |
| levels | hourly | on the first run of a month, evaluates last month and promotes IBs |

**Engine deals feed (gap).** The engine has no "deals since cursor" endpoint. `GET /v1/dealing/deals?from&to&limit` returns closing deals newest first, at most 2000, without the account type or cent flag. Each deal carries `source`, the order source that opened the position (`copy`, `mam`, `pamm`, `manual`, `api`, `dealer`, `system` …): `copy` / `mam` / `pamm` become the deal's ledger label (`origin`), anything else, or a feed without `source`, is `engine`. The poller keeps the last close time it saw, re-reads a 2-minute overlap (a deal's time is stamped before its shard commits), and pages backwards with `to` when a window is full. Idempotency is per deal, so overlap is free. Account type / group / cent come from `GET /v1/admin/accounts/{login}` (cached for an hour). Reopened deals are found by a 14-day re-scan. A dedicated engine feed — `GET /v1/dealing/deals?after_id=&limit=` in id order, including `accountType`, `cent` and `group`, plus reopen events — would replace the overlap, the per-login lookups and the re-scan.

## Programme rules

| Area | Rule |
|---|---|
| Membership (D53) | Every gateway client is a member at the entry level (rank 1) with a referral code (the gateway's `referral_code`) |
| Attribution (D63) | The upline is the gateway's `referred_by` at sign-up, set once. Only an admin reassignment changes it (audited, loop-checked). The campaign (`referral_campaign`) is attributed when it is one of that IB's campaign slugs |
| Qualifying deal (D54, D59) | Closed deal on a **live** account, not in an excluded group (setting, default `prop`) nor in a system group (`mam` → `mam_master`, `pamm` → `pamm_fund`, hard-coded in `calc.rs`, nothing to configure: their volume is paid on the linked accounts / pushed as PAMM lots), not reopened, not a price-correction deal, held ≥ `minTradeSeconds` (default 120 s), client has an upline and no self-referral block, symbol maps to a symbol group |
| Lots | Deal volume in lots; cent accounts × `centLotFactor` (0.01) |
| Options (O34) | A Kalks FX Options deal (`instrument: "option"` / `option` in the engine feed, or an option series symbol `EURUSD-20261009-1.1650-C`) is paid **per contract**: tier *k* IB earns `contracts × optionsRate(its level) × tiers[k] %`, with the same rebates, splits, caps and filters as CFD deals. `optionsRate` (USD per contract, round turn, paid on the closing deal: close, expiry or knock-out) is set per level in the Back Office and is **0 by default**, so options earn nothing until the broker sets it. CFD per-lot rates are never applied to an option deal. Contracts are not scaled on cent accounts and are **never lots**: option deals have 0 lots, so they never count toward `minMonthlyLots` or lot statistics (they do make a client active) |
| Symbol groups | Explicit symbol lists first (forex majors), then the asset class from `config/instruments.json` |
| Tier amounts (D55) | Tier *k* IB: `lots × rate(its level, symbol group) × tiers[k] %`, rounded to cents. Default tiers 100 / 20 / 10 |
| Splits (D60) | An IB at tier *k ≥ 2* passes `splitPct` of its own tier amount to the IB below it in that chain |
| Rebates (D60) | The tier-1 IB passes `rebatePct` of its tier-1 amount to the trading client |
| Caps | Rebate and split are capped by `maxRebatePct` / `maxSplitPct`; splits and rebates only move money between lines, the total equals the sum of the tier amounts |
| Suspended IB | Earns nothing, gives nothing |
| CPA (D57) | Once per client, to the tier-1 IB: first live deposit ≥ `cpa.minFirstDeposit` and (if required) a first qualifying trade. Amount = the IB level's `cpaAmount`. Payable after `holdDays` |
| First deposit | The earliest real-money deposit: the oldest `deposit` / `transfer_in` on any of the client's live accounts (engine ledger), or a deposit event pushed by the wallet (USDT credited to the wallet on chain, or a Back Office external deposit). Only one first deposit is kept (time + amount), never a sum, so wallet money later moved to a trading account (`transfer_in`) is not counted twice: the earlier wallet event wins |
| Levels (D58) | On the 1st of each month: highest level whose `minActiveClients` (tier-1 clients with a qualifying deal that month) **and** `minMonthlyLots` (qualifying lots of the whole network, all tiers) are met. Promotions only unless `allowDemotion`; `levelLocked` members are skipped |
| PAMM / copy (D64) | Lots allocated to a referred investor are pushed with `POST /v1/ib/events/lots` and earn like the investor's own trades |
| Payouts (D56) | Pending lines available before the period end are grouped per payee into a batch (payees below `payout.minAmount` or net ≤ 0 carry over). Approve → lines `approved`, payouts `transfer_pending` → wallet credit (`kind: ib_payout`, idempotency key `ib:payout:<tenant>:<batch>:<user>`) → `paid`. Reject → lines back to the pending pool |
| Reversal | A reopened deal's unbatched pending lines are voided; lines already batched or paid get a negative `clawback` line that nets off the next payout |
| Self-referral (D59) | Identity (keyed hash of name + date of birth, and of the phone), device hash and IP (loopback ignored) of the client vs its upline. Each signal: `block` (flag + no commission from that client until an admin dismisses), `flag`, or `off` |
| Wash trading | Opposite, similar-volume (±10 %) trades on the same symbol by the IB or two clients of the same IB, opened and closed within 60 s of each other → flag. ≥ 10 sub-minimum trades making ≥ 50 % of a client's trades in 24 h → `short_trades` flag |
| Visibility (D61) | `clientVisibility: full` shows IBs client names, emails, first deposit and trades (consent in the sign-up T&C); `masked` shows initials and totals only |

Money is `rust_decimal` everywhere and `NUMERIC` in the database; every line is rounded to cents half away from zero.

## Data model

Database `kalks_ib` (`migrations/0001_ib.sql`). Every row has `tenant` (gateway tenant slug).

| Table | Contents |
|---|---|
| `settings` | programme settings per tenant (JSON, versioned) |
| `levels` | name, rank, rates per symbol group, `options_rate` (USD per option contract, default 0), CPA amount, upgrade targets, perks |
| `members` | every client: upline (`parent_id`, source signup/admin), campaign, level, rebate/split %, status, self-referral state, identity/device/IP signals, first deposit / first trade |
| `campaigns`, `clicks` | campaign links per IB; clicks with a keyed visitor hash (no raw IP), unique per visitor per 24 h |
| `accounts` | engine account kind / group / cent cache |
| `deals` | every processed deal, qualified or not, with the reason; `(source, deal_id, user_id)` primary key = idempotency (`source` = the feed: `engine` \| `pamm` \| `copy`); `origin` = the ledger label (`copy` / `mam` / `pamm` / `engine`, `migrations/0005_deal_origin.sql`); `instrument` (`cfd` / `option`), `lots` (0 for options) and `contracts` |
| `commissions` | lines: `lot`, `split`, `rebate`, `cpa`, `clawback`, `adjustment`; status `pending → approved → paid` or `rejected` / `void`; unique per (`deal_source`, deal, client, beneficiary, kind) and one CPA per client; `origin` is the deal's label (clawbacks keep it). The ledger APIs return the label as `source` (falling back to the feed for older lines) and the feed as `feed`. Option lines: `symbol_group = 'options'`, `lots` 0, `contracts`, `rate` = the per-contract rate |
| `payout_batches`, `payouts` | batches and one payout per payee, with transfer attempts, last error, wallet txn |
| `fraud_flags` | open / confirmed / dismissed, deduplicated |
| `reassignments`, `level_history` | append-only history |
| `audit_log` | append-only (trigger), every admin change and money decision |
| `cursors` | poller cursors and once-per-period markers |
| `deposit_events` | deposit events by `(tenant, idempotency_key)`: user, USD amount, currency, time, source, whether it became the first deposit |

## API

Every route except `GET /health` needs `X-Kalks-Internal: $IB_INTERNAL_TOKEN`. Tenant: `X-Kalks-Tenant` (default `kalks`). JSON camelCase; money and lots are JSON numbers (programme settings return decimals as strings and accept numbers or strings). Errors: `{"error": {"code", "message", "field"?}}` with 400 `bad_request`, 401 `unauthorized`, 403 `forbidden`, 404 `not_found`, 409 `not_ready` | `exists` | `invalid_state` | `nothing_to_pay` | `no_change` | `limit`, 422 `validation`, 503 `unavailable`.

**Public / services**

| Method & path | Body | Response |
|---|---|---|
| `POST /v1/ib/clicks` | `{code, campaign?, ip, userAgent, referer?, landing?}` | `{valid, code, campaign, unique}` (the CRM proxy calls it) |
| `POST /v1/ib/events/deposit` | `{userId, amount, currency?: USD\|USDT\|USC, at?, idempotencyKey?, source?}` | `{status: recorded\|duplicate, firstDeposit}` — confirmed real-money deposit (the wallet sends `idempotencyKey: wallet:deposit:<tenant>:<txn>` and `at` = its ledger time). A repeated key is `duplicate` and changes nothing; the same key with another user / amount / time is `409 exists`; an unknown client is `404` (not mirrored yet: the wallet retries). USDT is USD 1:1, USC ÷ 100 |
| `POST /v1/ib/events/lots` | `{source: pamm\|copy, dealId, userId, symbol, side?, lots, openTime, closeTime, login?, reversed?}` | `{status: recorded\|duplicate, qualified, reason, lines}` |

**Client Area** (`X-Kalks-User-Id: <gateway user id>`, set by the CRM BFF from the session)

| Method & path | Response |
|---|---|
| `GET /v1/ib/me` | `member` (code, level, rebate/split), `progress` (active clients, monthly lots, next level, all levels), `earnings` (pending/approved/paid/rejected, month, lifetime, CPA), `counts`, `funnel`, `series` (daily + cumulative, 180 d), `weekly`, `topClients`, `recent`, `programme` |
| `GET /v1/ib/me/programme` | levels + rates, symbol groups, tiers, CPA, min trade seconds, payout schedule + next close, max rebate/split, visibility |
| `GET/POST /v1/ib/me/campaigns`, `PATCH /v1/ib/me/campaigns/{id}` | campaigns with clicks, unique clicks, sign-ups, FTDs, deposits, lots, 30-day click trend (plus the default link); create `{name, slug?, landing?, utmSource?, utmMedium?, utmCampaign?}`; `{active?, name?}` |
| `GET /v1/ib/me/clients?tier&q` | network clients to N tiers (masked per visibility) |
| `GET /v1/ib/me/clients/{id}/trades` | a network client's processed deals and what they earned the caller (full visibility only) |
| `GET /v1/ib/me/network` | `{root, nodes[{id, parentId, tier, name, level, lotsMonth, earnedMonth}], tiers}` |
| `GET /v1/ib/me/commissions?status&kind&page&limit` | lines + totals by status |
| `GET /v1/ib/me/payouts` | payouts (awaiting_approval / processing / paid / rejected), unbatched amount, schedule, next close |
| `PUT /v1/ib/me/settings` | `{rebatePct, splitPct}` within the programme maximums |

**Back Office** (`X-Kalks-Staff-Id`, `X-Kalks-Staff-Name` (percent-encoded), `X-Kalks-Staff-Role`, set by the admin BFF). Reads: any role. Writes: `platform_owner`, `super_admin`, `admin`, `partner_manager`. Money decisions: `platform_owner`, `super_admin`, `admin`, `finance`. Writes need `reason` (3–500 chars).

| Method & path | |
|---|---|
| `GET /v1/ib/admin/overview` | KPIs, 12 months by tier / split / rebate / CPA / clawback + lots, 30-day funnel, level distribution, top partners |
| `GET/PUT /v1/ib/admin/settings` | `{settings, version, updatedAt, updatedBy, nextPayoutClose}`; PUT `{settings, reason}` |
| `GET/PUT /v1/ib/admin/levels` | levels with member counts (each with `optionsRate`, USD per option contract); PUT `{levels, reason}` replaces the table (members on removed levels move to the entry level). A level sent without `optionsRate` keeps its current options rate (0–1000) |
| `GET /v1/ib/admin/partners?q&level&scope=ibs\|all&page&limit` | partners with clients, active clients, commission month / pending / paid, open flags |
| `GET/PATCH /v1/ib/admin/partners/{id}` | detail: upline chain, tree, commissions, payouts, flags, reassignments, level history; PATCH `{level?, levelLocked?, status?, rebatePct?, splitPct?, reason}` |
| `POST /v1/ib/admin/partners/{id}/reassign` | `{parentId \| null, reason}` |
| `GET /v1/ib/admin/commissions?status&kind&ib&client&batch&from&to&page&limit` | ledger + `sum` |
| `POST /v1/ib/admin/commissions/{id}/reject` | `{reason}` — pending, unbatched lines only |
| `GET/POST /v1/ib/admin/batches` | batches + transfer counts + unbatched amount; POST `{periodEnd?}` creates a batch now |
| `GET /v1/ib/admin/batches/{id}` | payees with breakdown, transfer status, open flags |
| `POST /v1/ib/admin/batches/{id}/approve \| reject \| retry` | `{note?}` / `{reason}` / – |
| `GET /v1/ib/admin/flags?status`, `POST /v1/ib/admin/flags/{id}/resolve` | `{action: dismiss\|confirm, note}` |
| `GET /v1/ib/admin/audit?limit&before&target` | audit entries |
| `POST /v1/ib/admin/run/{sync\|deals\|deposits\|reversals\|payouts\|levels}` | run a loop now (`levels` previews the current month) |

## How the apps integrate

| App / service | Integration |
|---|---|
| **Gateway** | `GET /v1/internal/referrals/users?since&after_id&limit` (internal token) in `(changed_at, id)` order. Sign-up (`/v1/auth/register`, `/v1/auth/google/complete`) takes `referral_campaign` next to `referral_code` |
| **Client Area** | `proxy.ts`: `/r/CODE[/campaign]` and any `?ref=CODE&c=campaign` record a click (`POST /v1/ib/clicks`, 1.5 s budget), set the first-party `kalks_ref` cookie and send signed-out visitors to `/register?ref=…`. The register and Google-complete BFFs add `referral_campaign` from the cookie when the code matches. `/api/partner/*` → `/v1/ib/me/*` |
| **Back Office** | `/api/partners/*` → `/v1/ib/admin/*` with permissions `partners.read / write / approve` (`apps/admin/lib/partners-perms.ts`) |
| **Wallet** | The IB service calls `POST /v1/wallets/transfers` (`direction: credit`, `kind: ib_payout`, `currency: USDT`). The wallet calls `POST /v1/ib/events/deposit` for every real deposit credited to a wallet (on-chain `deposit`, Back Office `manual_deposit`), from an outbox written with the credit and retried with backoff (`services/wallet/src/ib.rs`); IB payouts, commissions, bonuses, refunds, other adjustments and transfers from trading are never reported |
| **Support** | After each payout step, paid payouts not yet announced go to `POST $SUPPORT_URL/v1/notify` as `ib.commission_paid` (link `/partner/payouts`, `dedupeKey ib:payout:<id>:paid`): bell in the Client Area and Kalks Trader, email per the partner's `ib` preference. Retries back off (`payouts.notify_attempts`, `notify_error`) and never hold a payout back |
| **Copy / PAMM** | Push investor allocations with `POST /v1/ib/events/lots` when the master deal closes (not for trades on the investor's own account, which are read from the engine) |
| **Website** | Partner links should point to the Client Area (`https://app.<domain>/r/CODE/campaign`), or pass `?ref=&c=` through to it |

## Environment

| Variable | Default | |
|---|---|---|
| `IB_BIND` | `127.0.0.1:8096` | |
| `IB_DATABASE_URL` | `postgres://postgres@127.0.0.1:5433/kalks_ib` | created and migrated on first start |
| `IB_INTERNAL_TOKEN` | – | required when `IB_ENV=production`; also set in the CRM and admin `.env*.local`, and read by the wallet (deposit events) from the shared `.env.local` |
| `IB_ENV` | `development` | |
| `GATEWAY_URL` / `GATEWAY_INTERNAL_TOKEN` | `http://127.0.0.1:8080` | referral feed |
| `TRADING_URL` / `TRADING_INTERNAL_TOKEN` | `http://127.0.0.1:8090` | deals, accounts, ledgers |
| `WALLET_URL` / `WALLET_INTERNAL_TOKEN` | `http://127.0.0.1:8095` | payout credits |
| `SUPPORT_URL` / `SUPPORT_INTERNAL_TOKEN` | `http://127.0.0.1:8100` / – | `ib.commission_paid` notifications; empty URL = none |
| `INSTRUMENTS_FILE` | `config/instruments.json` | asset classes for symbol groups |
| `IB_SYNC_SECS` / `IB_DEALS_SECS` | `10` / `5` | poll periods |
| `IB_WORKERS` | `true` | `false` serves the API only |
| `IB_LOG_FORMAT` | `json` | `json` or `pretty` |

Production runs `deploy/systemd/kalks-ib.service`. `deploy/deploy.sh` builds it, generates `IB_INTERNAL_TOKEN` once, derives `IB_DATABASE_URL` from `GATEWAY_DATABASE_URL` with the database `kalks_ib`, and writes `IB_URL` / `IB_INTERNAL_TOKEN` into the CRM and admin `.env.production.local`.

## Tests

```bash
cargo test -p ib
```

- **Unit** (`calc`, `model`): option deals per contract and never lots, the options rate (default 0, validation, kept when absent), option series codes, per-lot amounts, three tiers with shares, rebate + split within the IB's own amount, caps, suspended IBs, rounding conservation, tiny deals, anti-abuse filters (demo, excluded group, reopened, price correction, minimum duration), cent lots, level evaluation (both targets, lock, demotion), wash-pair matching, payout periods, backoff, symbol groups, settings validation.
- **Database** (`tests/programme.rs`, throw-away `kalks_ib_test_*` database, skipped without PostgreSQL): option deals paid per contract only (rate 0 by default = no lines; the Back Office rate card through `PUT levels`, kept when a PUT omits it; tier %, rebate; short / demo / reopened filters; 0 lots, so level upgrades ignore them; a series pushed as PAMM / copy lots; feed parsing), multi-tier lines with rebate and split, the same deal twice (no double pay), a three-tier chain, short / demo / prop / no-referrer deals, PAMM lots, deals that arrive before the client is mirrored; origin labels from the feed's `source` (copy / MAM fills labelled on the deal, lines, ledger API and clawback; keyed by the `engine` feed, so a re-read without `source` is still a duplicate; a missing or non-social source = `engine`); self-referral block and admin clearing, loopback IPs ignored, wash-pair flag; CPA only after a deposit ≥ minimum and a trade, once; wallet deposit events (recorded, a retry is `duplicate`, a reused key with another amount is refused, a later `transfer_in` of the same money neither replaces nor adds to the first deposit, CPA once, USC ÷ 100, other currencies refused, unknown client 404); batch creation with carry-over, approval, a wallet that fails once (pending transfer, same idempotency key on retry, then paid), clawback after a paid deal is reopened, void of an unpaid one; batch rejection releasing lines; click tracking (unique per visitor, unknown campaign, unknown code).

## Known gaps

- **Engine feed.** See [How it works](#how-it-works): polling with overlap, per-login account lookups and a 14-day reversal re-scan instead of an ordered deals feed with the account type.
- **Deposits.** First deposits come from engine ledgers (`deposit`, `transfer_in`) and from the wallet's `POST /v1/ib/events/deposit` push, so a deposit that stays in the wallet counts toward CPA and FTD. The wallet backfills deposits booked before the push existed (its sweep), which can move an older client's first deposit earlier and, if that deposit meets the minimum, earn a CPA that was missed (held `holdDays`, approved with the batch as usual).
- **KYC identity.** The identity signal is a keyed hash of name + date of birth and of the phone number from the gateway, not the verified document number; the KYC service can add document hashes to the referral feed later.
- **Origin labels.** Lines written before `0005_deal_origin.sql` keep their feed (`engine`) as the label, even for copy-trading or MAM fills; only new deals are labelled `copy` / `mam`.
- **Banners / landing pages.** Campaign links point to a Client Area path; tenant banner and landing-page assets are not managed here.
- **Scale.** Network lots and client lists use recursive queries per request; fine for thousands of members, a materialised network table is the next step.

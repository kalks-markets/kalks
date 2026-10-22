# prop

The Kalks prop firm service (module 15, D147–D150): challenge plans, purchases paid from the USDT wallet, the phase state machine (Phase 1 → Phase 2 → Funded, or Failed), a real-time rule evaluator on top of the trading engine, funded payouts with profit split, the scaling plan, certificates with a public verify link, and banned-strategy heuristics for the risk desk. It is a Rust service (axum 0.8, sqlx 0.9, PostgreSQL) on `127.0.0.1:8097`.

Funded accounts are simulated (B-book, D150): the capital is a balance adjustment on an engine account in the plan's group, never client money.

- [Run locally](#run-locally)
- [How it works](#how-it-works)
- [Rules](#rules)
- [Data model](#data-model)
- [API](#api)
- [Integrations](#integrations)
- [Environment](#environment)
- [Tests](#tests)
- [Known gaps](#known-gaps)

## Run locally

You need PostgreSQL on `127.0.0.1:5433` and the trading engine on `:8090` (see `services/trading/README.md`). The wallet (`:8095`) is needed for purchases and payouts.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p prop
(cd services/prop && nohup ../../target/debug/prop > ~/.kalks-local/prop.log 2>&1 &)
curl -s localhost:8097/health
cargo test -p prop
```

On first start the service creates the `kalks_prop` database, runs `migrations/` and seeds three active plans (Classic 2-Step, Rapid 1-Step, Instant Funding). It reads `PROP_*`, `TRADING_*` and `WALLET_*` from the repo-root `.env.local`.

## How it works

```
 CRM BFF ──(X-Kalks-User-Id)──┐                     ┌── wallet :8095  POST /v1/wallets/transfers (debit fee, credit payouts)
 Admin BFF ─(X-Kalks-Staff-*)─┤→ prop :8097 ────────┤
                              │   ├ plans / challenges / payouts / certificates / flags (kalks_prop)
                              │   └ evaluator (every PROP_POLL_MS) ─┴── trading :8090  admin accounts, dealing close-all, balance adjustments
```

1. **Purchase.** `POST /v1/challenges` → wallet debit of the fee (`kind: prop_purchase`, key `prop-purchase-<id>`) → `POST /v1/accounts` on the engine (live account, plan group and leverage, owned by the client) → balance adjustment of the account size (`PRP-01`, key `prop-fund-<phase account>`) → rules armed. The generated trading and investor passwords are returned once in the response and never stored. The Trade button uses the engine's one-time SSO, so a password is never needed for later phases.
2. **Evaluator.** Every active phase account is polled (see [latency](#latency)). Each observation runs the pure rule functions in `src/rules.rs`, stores the live state for the dashboards and writes rule events.
3. **Breach** → the phase account is claimed as `failed`, then on the engine: status `close_only`, every position force-closed and every pending order cancelled (dealing bulk close, `PRP-02`), status `disabled`. The challenge fails, the trader is notified. If the engine can't confirm (for example a closed market), the reconciler retries until it does.
4. **Pass** (target + minimum days + consistency) → the account is flattened and set `read_only`, a certificate is issued, and the next account opens: Phase 2, or the funded account (with a "Funded trader" certificate).
5. **Payouts** on the funded account: request after `firstPayoutDays` (then every payout cycle) → the profit is taken off the account at once (so it can't be traded away while in review) → staff approve → wallet credit of the trader's split (`kind: prop_payout`) plus the fee refund on the first payout when the plan refunds fees → payout certificate → scaling review. A rejection puts the profit back on the account.

Every write is idempotent (wallet and engine keys derived from row ids), every decision is in `audit_log`, and a reconciler (every 20 s) finishes anything a crash or an unavailable upstream left half-way: unconfirmed wallet charges, unopened accounts, unenforced fails/passes, unconfirmed payout credits.

### Latency

The engine has no server-to-server account stream yet: the terminal stream needs a trader session, and the dealing stream carries per-position P&L but not account equity. The evaluator therefore polls `GET /v1/admin/accounts/{login}` for every active prop account every `PROP_POLL_MS` (default 1000 ms, `PROP_POLL_CONCURRENCY` = 16 requests in parallel). That returns the engine's live equity marked to the current quote, the balance, the open positions and the event version; closing deals are fetched only when the version changed. A breach is acted on at most one poll interval plus one request after equity crossed the limit (about 1–1.2 s locally), and close-all follows immediately. Equity spikes between two polls are not seen, which only ever favours the trader (trailing high-water mark). The evaluator logs a warning when a pass takes longer than the interval.

## Rules

All limits are a percentage of the phase's initial balance. The trading day rolls at **17:00 New York** (00:00 server time, GMT+3 during US DST and GMT+2 otherwise, the same clock as the engine's swap rollover).

| Rule | Definition | On breach |
|---|---|---|
| Daily loss | reference = balance at the last reset (`dailyBasis: balance`), or the higher of balance and equity at the reset (`equity`). Breach when equity ≤ reference − limit. Warnings at 50 / 75 / 90% | fail + close all |
| Max drawdown, static | equity ≤ initial − limit | fail + close all |
| Max drawdown, trailing | floor = high-water mark of balance/equity − limit; with `trailingLock` it stops at the initial balance | fail + close all |
| Profit target | closed balance and equity both ≥ initial + target | pass (with the next two) |
| Minimum trading days | distinct server days with a position opened | holds the pass |
| Consistency | best day's closed profit ≤ `consistency`% of total profit (0 = off) | holds the pass / payout |
| Time limit | not passed within `timeLimit` days of the phase start (0 = none) | fail |
| Weekend holding | when not allowed: positions still open from Friday 16:45 New York are closed | violation + close all |
| News window | when not allowed: a position opened or closed within ±`newsWindow` min of an admin calendar event on an affected instrument (currency match, or the event's explicit symbol list) | violation + that position closed, or fail when `newsBreachFails` |
| Banned strategies | heuristics in `src/heuristics.rs` (below) | flagged for review; a reviewer can confirm and fail |

Heuristics (only for strategies the plan lists in `banned`):

| kind | signal |
|---|---|
| `tick_scalping` | ≥ 10 closed trades and ≥ 50% of them held under 60 s |
| `hft` | ≥ 10 trades opened inside any 60 s window, or ≥ 200 in one day |
| `latency_arbitrage` | ≥ 10 trades held under 120 s with a win rate ≥ 90% |
| `cross_account_copying` | ≥ 3 same-side opens on one symbol within 2 s on two prop accounts |
| `cross_account_hedging` | ≥ 3 opposite-side opens on one symbol within 2 s on two prop accounts |

Late first look of a day (service restart): the balance at the reset is reconstructed as balance now − closed P&L since the reset.

## Data model

Database `kalks_prop` (`migrations/0001_prop.sql`). Every table has `tenant` and an RLS policy on `current_setting('kalks.tenant')`; money is `NUMERIC` / `rust_decimal`.

| Table | Contents |
|---|---|
| `plans`, `plan_sizes` | plan builder: type, status (`draft` / `active` / `paused` / `archived`), version (bumped on every save), engine group, phases, every rule, split, scaling, payout terms; sizes with fee and leverage |
| `challenges` | a purchase: plan snapshot (`rules`, the full plan JSON at purchase), size, fee, split, status (`pending_payment` → `provisioning` → `active` → `funded` / `failed` / `closed`, or `payment_failed`), current phase index |
| `phase_accounts` | one engine account per phase: login, status (`provisioning` / `active` / `passed` / `failed` / `closed`), terms, live rule state (day, reset balance/equity, high-water mark, balance, equity, trading days, engine version) and `stats` (last evaluation + trading statistics) |
| `equity_points` | equity curve samples (≤ 1 per minute, plus every trade) |
| `rule_events` | breaches, violations, warnings, passes (deduplicated per account) |
| `strategy_flags` | banned-strategy heuristics and their review |
| `payouts` | requests, split, fee refund, KYC status at request, decision, wallet reference |
| `scaling_events` | scale-ups |
| `certificates` | public code, kind (`pass` / `funded` / `payout`), public trader name (first name + initial), amounts, revoked |
| `news_events` | admin calendar for the news rule |
| `notifications` | trader alerts (warnings, breach, pass, payout decisions) |
| `wallet_ops` | every wallet call with its idempotency key and outcome |
| `audit_log` | append-only (trigger) audit of staff and system decisions |

## API

Base `http://127.0.0.1:8097`, JSON, camelCase. Every route except `GET /health` needs `X-Kalks-Internal: $PROP_INTERNAL_TOKEN`. Tenant: `X-Kalks-Tenant` (default `kalks`). Errors: `{"error": {"code", "message", "field"?}}` — `400 bad_request`, `401 unauthorized`, `403 forbidden`, `404 not_found`, `409 exists | already_decided | idempotency_conflict | not_active`, `422 validation | insufficient_funds | payment_failed | plan_unavailable | kyc_required | not_yet_eligible | below_minimum | positions_open | payout_pending | consistency | not_funded | account_unavailable | wallet_rejected`, `502 payment_pending | provisioning | wallet_pending | engine_*`.

Money is a JSON number (requests also accept numeric strings).

### Plan object

```json
{"id": "classic-2-step", "name": "Kalks Classic 2-Step", "type": "1-step | 2-step | instant", "status": "draft | active | paused | archived",
 "version": 3, "group": "prop",
 "sizes": [{"size": 10000, "fee": 89, "leverage": 100, "enabled": true}],
 "phases": [{"name": "Phase 1", "target": 8, "minDays": 4, "timeLimit": 0}, {"name": "Phase 2", "target": 5, "minDays": 4, "timeLimit": 0}],
 "dailyLoss": 5, "dailyBasis": "balance | equity", "maxDD": 10, "ddType": "static | trailing", "trailingLock": true,
 "consistency": 0, "newsTrading": true, "newsWindow": 2, "newsBreachFails": false, "weekendHolding": true, "eaAllowed": true,
 "banned": ["hft", "latency_arbitrage", "tick_scalping", "cross_account_copying", "cross_account_hedging"],
 "split": 80, "splitMax": 90, "scalingEvery": 4, "scalingIncrease": 25, "scalingProfit": 10, "scalingCap": 2000000,
 "refundFee": true, "payoutFreq": "weekly | bi-weekly | monthly | on-demand", "firstPayoutDays": 14, "minPayout": 50,
 "updatedAt": "…", "updatedBy": "…"}
```

`type` fixes the number of phases (1-step: 1, 2-step: 2, instant: 0). `maxDD` ≥ `dailyLoss`; `splitMax` ≥ `split`; the engine group must exist and offer every size's leverage.

### Challenge object

```json
{"id": 12, "userId": 42, "traderName": "…", "planId": "…", "planName": "…", "type": "2-step", "size": 10000, "fee": 89, "leverage": 100,
 "group": "prop", "status": "active", "phaseIndex": 0, "feeRefunded": false, "split": 80, "failureReason": null, "createdAt": "…",
 "plan": {Plan snapshot at purchase},
 "phases": [PhaseAccount…], "current": PhaseAccount}
```

PhaseAccount: `id, challengeId, phaseIndex, phase ("Phase 1" | "Phase 2" | "Evaluation" | "Funded"), funded, login, status, initialBalance, targetPct, minDays, timeLimitDays, startedAt, endedAt, endReason, balance, equity, openPositions, tradingDays, lastEvalAt, lastPayoutAt, scaledAt, rules, stats`.

`rules` is the live rule dashboard from the last evaluation:

```json
{"day": "2026-09-28", "dailyLimit": 500, "dailyRef": 10000, "dailyFloor": 9500, "dailyUsed": 120.5,
 "ddLimit": 1000, "ddFloor": 9000, "ddUsed": 120.5, "hwm": 10000, "profit": -40, "targetAmount": 800, "targetReached": false,
 "tradingDays": 1, "minDays": 4, "daysOk": false, "bestDay": 12.5, "consistencyLimit": null, "consistencyOk": true,
 "deadline": null, "verdict": {"kind": "ok | pass | breach", "rule"?, "message"?, "threshold"?}, "warn": null,
 "nextReset": "2026-09-28T21:00:00Z", "weekendWindow": false, "equity": 9879.5, "balance": 9960, "at": "…"}
```

`stats`: `trades, open, wins, losses, winRate, avgWin, avgLoss, profitFactor, lots, bestDay {day, profit}, days [..], dayProfits [{day, profit}], opens [..]`.

### Client routes (Client Area BFF)

The BFF forwards the signed-in gateway user: `X-Kalks-User-Id` (required), `X-Kalks-User-Name` (percent-encoded, used on certificates) and `X-Kalks-User-Kyc` (the user's `kyc_status`; used for the payout gate when the service has no gateway DB connection).

| Method & path | Body / query | Response |
|---|---|---|
| `GET /v1/plans` | – | `{plans: Plan[]}` (active plans, enabled sizes) |
| `POST /v1/challenges` | `{planId, size, idempotencyKey}` (key: 1–80 of `[A-Za-z0-9_-]`, reuse it on retry) | `{challenge, credentials: {login, password, investorPassword} \| null}` (credentials once) |
| `GET /v1/challenges` | – | `{challenges: Challenge[]}` |
| `GET /v1/challenges/{id}` | – | Challenge + `payout` (quote, funded only), `events` (last 20), `certificates` |
| `GET /v1/challenges/{id}/equity?phase=&limit=` | – | `{accountId, login, initialBalance, points: [{at, balance, equity}]}` |
| `GET /v1/challenges/{id}/events?limit=` | – | `{events: [{id, accountId, login, rule, severity, at, equity, balance, threshold, message, details}]}` |
| `GET /v1/challenges/{id}/trades?phase=` | – | `{login, trades: [{ticket, symbol, side, volume, openTime, closeTime, openPrice, closePrice, profit, durationSecs}]}` (newest first) |
| `POST /v1/challenges/{id}/payouts` | – | `{payout}` (whole current profit; see quote) |
| `GET /v1/payouts` | – | `{payouts: Payout[], funded: [{challengeId, planName, size, login, balance, equity, quote, refundFee, feeRefunded}], kycStatus}` |
| `GET /v1/certificates` | – | `{certificates: Certificate[]}` |
| `GET /v1/notifications` / `POST /v1/notifications/read` | – | `{notifications: [{id, challengeId, kind, title, body, at, read}]}` / `{status, read}` |

Payout quote: `{eligibleFrom, profit, split, traderAmount, firmAmount, feeRefund, total, minPayout, blockers: ["not_yet_eligible" | "below_minimum" | "positions_open" | "payout_pending" | "consistency"], eligible}`.

Payout: `{id, challengeId, accountId, userId, login, profit, split, traderAmount, firmAmount, feeRefund, total, status (pending | approved | paid | rejected | failed), kycStatus, requestedAt, decidedAt, decidedBy, note, error, planName, size, traderName}`.

Certificate: `{code, kind (pass | funded | payout), title, traderName, planName, size, amount, phase, issuedAt, revoked, challengeId, userId, verifyUrl}`.

### Public routes (through the Client Area BFF, still with the internal token)

| Method & path | Response |
|---|---|
| `GET /v1/public/certificates/{code}` | `{code, kind, title, traderName, planName, size, amount, phase, issuedAt, valid, verifyUrl}` (no user ids) |
| `GET /v1/public/certificates/{code}/image.svg` | branded 1200×675 SVG |

### Staff routes (Back Office BFF)

The BFF forwards the verified staff session: `X-Kalks-Staff-Id`, `X-Kalks-Staff-Name` (percent-encoded), `X-Kalks-Staff-Role`, `X-Kalks-Staff-Perms` (the gateway permission keys). The service checks the `prop.*` key from that list, so a custom role gets exactly what it was granted; without the header (older callers) it checks the role against the map below (the admin BFF keeps the same map in `apps/admin/lib/prop-perms.ts`). Super Admin and Platform Owner are always allowed:

| permission | allows | roles |
|---|---|---|
| `prop.read` | every GET | every staff role except marketing, partner_manager |
| `prop.write` | plan builder, manual pass/fail, flag review, news calendar, scaling, certificate revoke | platform_owner, super_admin, admin, risk_manager, dealer |
| `prop.approve` | payout approve / reject | platform_owner, super_admin, admin, finance, risk_manager |

| Method & path | Body / query | Response |
|---|---|---|
| `GET /v1/admin/overview` | – | `{activeChallenges, funded, failed, passRate, fees30d, sold30d, paid30d, payoutsPending, payoutsPendingAmount, flagsOpen, breaches24h, fundedCapital, breachReasons: [{rule, count}]}` |
| `GET /v1/admin/plans` | – | `{plans: [Plan + stats {active, sold30d, revenue30d, passRate}]}` (archived included) |
| `POST /v1/admin/plans` | Plan + `reason` | `{plan}` (409 `exists`) |
| `PUT /v1/admin/plans/{id}` | Plan + `reason` | `{plan}` (version + 1; running challenges keep their snapshot) |
| `POST /v1/admin/plans/{id}/status` | `{status, reason}` | `{plan}` |
| `GET /v1/admin/engine-groups` | – | `{groups: [{code, name, leverages, enabled, maxAccountsPerUser}]}` |
| `GET /v1/admin/challenges?status=&plan=&q=&user_id=&page=&limit=` | – | `{items: [Challenge + flags (open count)], page, limit, total}`; `q` matches trader name, user id, challenge id or login |
| `GET /v1/admin/challenges/{id}` | – | Challenge + `events` (200), `flags`, `payouts`, `audit`, `payout` (quote) |
| `POST /v1/admin/challenges/{id}/override` | `{action: "pass" \| "fail", reason, note}` | Challenge (audited `override.pass` / `override.fail`) |
| `POST /v1/admin/accounts/{phaseAccountId}/scale` | `{toSize?, reason}` | `{scaledTo}` |
| `GET /v1/admin/events?severity=&rule=&login=&limit=&before=` | – | `{events: [event + challengeId, userId, traderName, planName, size, phase]}` (breaches log) |
| `GET /v1/admin/payouts?status=` | – | `{payouts: Payout[]}` (open ones first) |
| `POST /v1/admin/payouts/{id}/approve` | `{note?, kycStatus?}` | `{payout}` (KYC must be `verified`) |
| `POST /v1/admin/payouts/{id}/reject` | `{note}` | `{payout}` |
| `GET /v1/admin/certificates` | – | `{certificates}` |
| `POST /v1/admin/certificates/{code}/revoke` | `{reason}` | `{certificate}` |
| `GET /v1/admin/flags?status=open\|cleared\|confirmed` | – | `{flags: [{id, accountId, challengeId, userId, login, kind, score, summary, evidence, relatedLogin, status, createdAt, updatedAt, reviewedBy, reviewedAt, reviewNote, traderName, planName, phase, accountStatus}]}` |
| `POST /v1/admin/flags/{id}/review` | `{decision: "clear" \| "confirm", failAccount?, note}` | `{flag}` |
| `GET /v1/admin/news` / `POST /v1/admin/news` / `DELETE /v1/admin/news/{id}` | `{at, title, currency, impact?, symbols?}` | `{events}` / `{id}` / `{status}` |
| `GET /v1/admin/audit?entity=&entityId=&limit=&before=` | – | `{items: [{id, at, actor, actorName, actorRole, action, entity, entityId, before, after, reason, note}]}` |

```bash
H='-H x-kalks-internal:'$PROP_INTERNAL_TOKEN' -H content-type:application/json'
curl -s localhost:8097/v1/plans $H -H x-kalks-user-id:42
curl -s -X POST localhost:8097/v1/challenges $H -H x-kalks-user-id:42 -d '{"planId":"classic-2-step","size":10000,"idempotencyKey":"buy-1"}'
curl -s -X POST localhost:8097/v1/admin/payouts/1/approve $H -H x-kalks-staff-id:1 -H x-kalks-staff-role:finance -d '{"note":"ok"}'
```

## Integrations

| With | How |
|---|---|
| Trading engine | As system staff (`X-Kalks-Staff-Id: prop-service`, role `admin`) with `TRADING_INTERNAL_TOKEN`: `POST /v1/accounts` (open, owned by the client), `POST /v1/admin/accounts/{login}/balance` type `adjustment` (capital, payouts, scaling; reason codes `PRP-01`…`PRP-06`), `GET /v1/admin/accounts/{login}` (evaluator), `GET /v1/dealing/deals?login=&from=` (closed trades), `POST /v1/dealing/positions/bulk` + `/v1/dealing/orders/cancel` (close-all), `POST /v1/dealing/positions/{ticket}/close` (news), `POST /v1/admin/accounts/{login}/status`, `GET /v1/admin/groups` |
| Wallet | `POST /v1/wallets/transfers` `{idempotency_key, user_id, currency: "USDT", amount, direction, kind, ref, note}` with `WALLET_INTERNAL_TOKEN`: fee debit `prop_purchase`, purchase refund `refund` credit, payouts and fee refunds `prop_payout` credit. 2xx = booked, 4xx = refused, 5xx / network = unknown and retried with the same key |
| Gateway | Optional read-only `PROP_GATEWAY_DATABASE_URL` (defaults to `GATEWAY_DATABASE_URL`) for `users.kyc_status`: payouts need `verified` at request and at approval. Without it the service uses the status the BFF forwards |
| Client Area | `/api/prop/*` BFF (session → `X-Kalks-User-*`), live pages `/prop`, `/prop/mine`, `/prop/payouts`, `/prop/certificates`, public `/verify/<code>` with a PNG image route |
| Back Office | `/api/prop/*` BFF (staff session → `X-Kalks-Staff-*`, `lib/prop-perms.ts`), live pages under `/prop` |

## Environment

| Variable | Default | |
|---|---|---|
| `PROP_BIND` | `127.0.0.1:8097` | |
| `PROP_DATABASE_URL` | `postgres://postgres@127.0.0.1:5433/kalks_prop` | created and migrated on first start |
| `PROP_INTERNAL_TOKEN` | – | required when `PROP_ENV=production` |
| `PROP_ENV` | `development` | |
| `TRADING_URL` / `TRADING_INTERNAL_TOKEN` | `http://127.0.0.1:8090` / – | engine |
| `WALLET_URL` / `WALLET_INTERNAL_TOKEN` | `http://127.0.0.1:8095` / – | wallet |
| `SUPPORT_URL` / `SUPPORT_INTERNAL_TOKEN` | `http://127.0.0.1:8100` / – | notification push (bell, realtime, email); empty URL = prop inbox only |
| `PROP_GATEWAY_DATABASE_URL` | `GATEWAY_DATABASE_URL` | KYC status (read-only) |
| `PROP_POLL_MS` | `1000` | evaluator interval (200 – 60 000) |
| `PROP_POLL_CONCURRENCY` | `16` | parallel engine requests |
| `PROP_EVALUATOR` | `true` | only one instance may run the evaluator |
| `PROP_VERIFY_BASE_URL` | `http://localhost:3000/verify` | public certificate link base, e.g. `https://app.kalkstrade.com/verify` |
| `PROP_LOG_FORMAT` | `json` | |

Production runs `deploy/systemd/kalks-prop.service`. `deploy/deploy.sh` builds it, generates `PROP_INTERNAL_TOKEN` on first deploy and derives `PROP_DATABASE_URL` from `GATEWAY_DATABASE_URL` (database `kalks_prop`).

## Tests

```bash
cargo test -p prop
```

- **Rule math** (`src/rules.rs`): daily loss on the balance and equity basis, exactly-at-limit breach, 50/75/90% warnings; the daily reset at 17:00 New York in summer and winter and a late first look reconstructing the reset balance; static and trailing drawdown (peak tracking, lock at the initial balance, trailing breach above the static floor); profit target with minimum days and equity; consistency holding the pass; time limit; funded accounts never pass; payout rebase; payout split rounding and schedule; scaling due / cap; news-window matching.
- **Server time** (`src/time.rs`): NY-close day boundaries, the DST switch instants in March and November, the weekend window.
- **Heuristics**: tick scalping, latency arbitrage, HFT bursts, cross-account copying and hedging with thresholds.
- **Plans**: JSON round trip and validation.
- **Flow test** (`tests/flow.rs`): runs the real service logic against a throw-away database `kalks_prop_test_<pid>` with an in-process mock engine and mock wallet (skipped when Postgres is unreachable): purchase → daily-loss breach → close-all + disabled; purchase → profit target → Phase 2 opened → Funded opened; funded payout request → approval → wallet credit with the fee refund; rejected payout returns the profit; insufficient wallet funds.

## Known gaps

- **Options.** The engine refuses Kalks FX Options on prop groups (`prop*`), so prop accounts are CFD only. Defensively, option deals and positions (engine `option` / `instrument: "option"`, or an option series symbol) are flagged and their contracts never count as lots in the trading statistics; their P&L (real money on the account) still counts in equity, day profits and the rules.

- **Polling, not streaming** (see [latency](#latency)). A server-to-server account stream in the engine would cut detection to the tick.
- **Wallet transfers out of prop accounts.** The engine lets the wallet move `withdrawable` funds from any live account. The wallet must refuse transfers for accounts in prop groups (group `prop`, or any plan's `group`), otherwise simulated capital could be withdrawn.
- **Account limit per group.** The engine's `maxAccountsPerUser` applies to prop groups too (seeded `prop`: 5). Raise it for the prop group in Back Office → Config → Account groups; a refused open refunds the fee.
- **Heuristics** cover the five listed strategies; martingale / grid or other labels on a plan are shown to traders but not detected. Cross-account checks only see prop accounts.
- **News calendar** is managed by staff in the Back Office; there is no feed ingest yet.
- **Notifications** are stored in the prop inbox and pushed by `src/notifier.rs` (with the evaluator instance) to the support service: `prop.passed`, `prop.failed` (breach), `prop.funded`, `prop.phase_started`, `prop.scaled`, `prop.loss_warning`, `prop.violation`, `prop.payout_requested`, `prop.payout_paid`, `prop.payout_rejected`, links `/prop/mine` / `/prop/payouts`, `dedupeKey prop:n:<id>`. Support shows them in the Client Area and Kalks Trader bells and emails them per the trader's `prop` preference; delivery retries with backoff and never blocks the evaluator or a payout.
- **Certificates** use first name + last initial; there is no opt-out setting yet.

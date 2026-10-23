# wallet

The Kalks central client wallet (D3): per-user multi-currency balances on a double-entry ledger, USDT deposits on BNB Chain (BEP20) and TRON (TRC20) verified on-chain, crypto deposits in other coins through [NOWPayments](#nowpayments), withdrawals with admin approval, and transfers between the wallet and the client's own trading accounts. It is a Rust service (axum 0.8, sqlx 0.9, PostgreSQL) on `127.0.0.1:8095`.

Other services (IB, copy/PAMM, prop) move money in and out of client wallets **only** through [`POST /v1/wallets/transfers`](#wallet-api-contract-for-other-services).

- [Run locally](#run-locally)
- [Wallet API contract (for other services)](#wallet-api-contract-for-other-services)
- [Conventions](#conventions)
- [Ledger](#ledger)
- [Client API (CRM BFF)](#client-api-crm-bff)
- [Deposits](#deposits)
- [NOWPayments](#nowpayments)
- [Withdrawals](#withdrawals)
- [Wallet and trading accounts](#wallet-and-trading-accounts)
- [Back Office API](#back-office-api)
- [Balance & credit (manual adjustments)](#balance--credit-manual-adjustments)
- [Environment](#environment)
- [Tests](#tests)
- [Production checklist](#production-checklist)

## Run locally

You need PostgreSQL on `127.0.0.1:5433` (user `postgres`, trust auth), the gateway on `:8080` (KYC status) and the trading engine on `:8090` (wallet ↔ trading transfers). The database `kalks_wallet` is created and migrated on first start.

```bash
cargo run -p wallet          # reads the repo-root .env.local (WALLET_*) and .env.tron (TRONGRID_API_KEY)
cargo test -p wallet         # unit + PostgreSQL integration tests (skipped when Postgres is not reachable)
curl -s localhost:8095/health
```

## Wallet API contract (for other services)

Every call sends `X-Kalks-Internal: $WALLET_INTERNAL_TOKEN`, optionally `X-Kalks-Tenant: <slug>` (default `kalks`) and `X-Kalks-Service: <ib|prop|pamm|copy|...>` (recorded as the actor). `user_id` is the gateway user id.

### `GET /v1/wallets/{user_id}`

```json
{ "user_id": 42, "balances": [ { "currency": "USDT", "available": "125.500000", "locked": "0" } ] }
```

USDT is always listed, with zeros when the user has never had funds. `available` can be spent or withdrawn. `locked` is held by a pending withdrawal or an in-flight transfer to a trading account.

### `POST /v1/wallets/transfers`

| Field | Type | Notes |
|---|---|---|
| `idempotency_key` | string, 1–128 | Unique per tenant. Use something stable from your side, e.g. `ib:payout:2026-09:42` |
| `user_id` | integer | |
| `currency` | `"USDT"` | |
| `amount` | decimal string (a JSON number is accepted) | > 0, at most 6 decimals |
| `direction` | `"credit"` \| `"debit"` | credit adds to the user's available balance, debit takes from it |
| `kind` | `commission` \| `ib_payout` \| `prop_purchase` \| `prop_payout` \| `pamm_invest` \| `pamm_redeem` \| `copy_fee` \| `adjustment` \| `refund` | |
| `ref` | string ≤ 128, optional | Your own reference (payout id, challenge id, fund id) |
| `note` | string ≤ 500, optional | Shown on the client's wallet history |

Response `200`:

```json
{ "status": "completed", "txn_id": 381, "user_id": 42, "currency": "USDT", "amount": "25", "direction": "credit",
  "kind": "ib_payout", "ref": "payout-2026-09", "note": null, "balance": { "available": "150.5", "locked": "0" },
  "created_at": "2026-09-28T10:00:00Z", "replayed": false }
```

- **Idempotent.** Repeating the same `idempotency_key` with the same body returns the original result with `"replayed": true` and books nothing. The same key with a different body (`user_id`, `currency`, `amount`, `direction`, `kind`, `ref`, `note`) returns `409 idempotency_conflict`. Concurrent duplicates are resolved by a unique index, so retries after a timeout are always safe.
- **Debit.** A debit beyond `available` returns `422 insufficient_funds` and books nothing. Locked funds can't be debited.
- Each kind has its own counter-account (`sys:USDT:<kind>`), so the IB/prop/PAMM books can be reconciled against the wallet.

`GET /v1/wallets/transfers/{idempotency_key}` returns the stored result for a key (`404` if unknown), so a caller that lost the response can look it up.

### `GET /v1/wallets/{user_id}/ledger?page=1&limit=50`

```json
{ "items": [ { "txn_id": 381, "kind": "ib_payout", "currency": "USDT", "amount": "25", "available_delta": "25", "locked_delta": "0",
               "ref": "payout-2026-09", "note": null, "created_at": "2026-09-28T10:00:00Z" } ],
  "page": 1, "limit": 50, "total": 1 }
```

Newest first. `amount` is the signed change to the client's total (available + locked). A withdrawal request shows `available_delta -100, locked_delta +100, amount 0`.

## Conventions

- **Base URL** `http://127.0.0.1:8095`. JSON in and out, snake_case keys. Every route except `GET /health` needs `X-Kalks-Internal`.
- **Money.** Amounts are decimal **strings** in responses. Requests accept a string or a JSON number. The service never uses floats. USDT is USD 1:1 (D35).
- **Staff.** Back Office routes need the staff identity that the admin BFF verified with the gateway: `X-Kalks-Staff-Id`, `X-Kalks-Staff-Name` (percent-encoded), `X-Kalks-Staff-Role`, `X-Kalks-Staff-Perms` (the gateway permission keys). The service checks the permission again (see [Back Office API](#back-office-api)).
- **Client context.** Forward the client IP as `X-Forwarded-For` and the user agent as `User-Agent`. Withdrawals record both.
- **Errors.** `{"error": {"code", "message", "field"?}}`

| HTTP | code |
|---|---|
| 400 | `bad_request` |
| 403 | `forbidden`, `kyc_required` |
| 404 | `not_found` |
| 409 | `idempotency_conflict`, `tx_already_used`, `invalid_state`, `in_progress` |
| 422 | `validation` (+`field`), `insufficient_funds`, `below_minimum`, `above_maximum`, `daily_limit`, `deposit_cooldown`, `chain_disabled`, `intent_expired`, `not_own_account` |
| 502 / 503 | `engine_error`, `unavailable` |

## Ledger

Double-entry and append-only. Each change is one `ledger_txns` row with a unique `(tenant_id, idempotency_key)` and two or more `ledger_postings` whose amounts sum to zero per currency. A trigger rejects UPDATE and DELETE on both tables.

| Account code | Meaning |
|---|---|
| `user:<id>:<ccy>:available` | spendable client balance |
| `user:<id>:<ccy>:locked` | pending withdrawals and in-flight transfers to trading |
| `sys:<ccy>:deposit:<chain>` | on-chain money received (negative = received) |
| `sys:USDT:deposit:nowpayments` | USD credited for NOWPayments crypto deposits (negative = received; the coins sit in the NOWPayments account) |
| `sys:<ccy>:withdrawal:<chain>` | on-chain money paid out |
| `sys:<ccy>:fees` | withdrawal fees earned |
| `sys:<ccy>:trading` | net money moved into trading accounts |
| `sys:<ccy>:<kind>` | counter-account per transfer kind (IB, prop, PAMM, copy, adjustments, refunds) |

`wallet_balances` is a projection updated in the same transaction as the postings, with `CHECK (available >= 0 AND locked >= 0)`. On start the service recomputes every balance from the postings and refuses to start on any mismatch. `GET /v1/admin/reconciliation` runs the same checks on demand.

## Client API (CRM BFF)

The CRM BFF resolves the user from the session cookie and passes `user_id`. For withdrawals it redeems the step-up token (`withdrawal`) with the gateway **before** calling the service.

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/config` | – | `{chains:[{chain, network, token, token_contract, decimals, confirmations, deposits_enabled, withdrawals_enabled, min_deposit, withdraw_fee}], limits:{withdraw_min, withdraw_max, withdraw_daily_max, withdraw_fee_flat, withdraw_fee_pct, deposit_cooldown_hours, intent_ttl_minutes}}` |
| `GET /v1/wallets/{user_id}/overview` | – | `{balances, pending_deposits[], open_withdrawals[], limits:{used_today, remaining_today, cooldown_until}, notifications_unread}` |
| `GET /v1/wallets/{user_id}/activity?page&limit&type=deposit\|withdrawal\|transfer\|all` | – | `{items:[{type, id, status, amount, currency, chain?, tx_hash?, login?, direction?, created_at, ...}], total}` |
| `POST /v1/deposits/intents` | `{user_id, chain:"bsc"\|"tron", amount}` | `{intent}`: `{id, chain, currency, amount, address, token_contract, decimals, chain_id?, expires_at, status}` |
| `GET /v1/deposits/intents/{id}?user_id=` | – | `{intent, deposit?}` |
| `POST /v1/deposits/submit` | `{user_id, intent_id, tx_hash, from_address?}` | `{deposit}` (409 `tx_already_used`) |
| `GET /v1/deposits/{id}?user_id=` | – | `{deposit}` with `confirmations`, `required_confirmations`, `status` |
| `POST /v1/withdrawals/quote` | `{user_id, amount, chain, to_address}` | `{quote:{amount, fee, net_amount, used_today, daily_max, available}}`: every check of a request, nothing locked (call it before the step-up code) |
| `POST /v1/withdrawals` | `{user_id, amount, chain, to_address, idempotency_key?}` | `{withdrawal}` |
| `GET /v1/withdrawals?user_id=` | – | `{items}` |
| `POST /v1/withdrawals/{id}/cancel` | `{user_id}` | `{withdrawal}` (only while `requested`) |
| `POST /v1/wallets/{user_id}/to-trading` | `{login, amount, idempotency_key}` | `{transfer}` |
| `POST /v1/wallets/{user_id}/from-trading` | `{login, amount, idempotency_key}` | `{transfer}` |
| `GET /v1/wallets/{user_id}/notifications` / `POST .../notifications/read` | – / `{ids?}` | `{items, unread}` |
| `GET /v1/nowpayments/config` | – | `{enabled:false}` or `{enabled, sandbox, min_deposit_usd, max_deposit_usd, fee_paid_by_user, coins:[{code, ticker, name, network, icon, chain_icon, needs_extra_id, min_usd}]}` ([NOWPayments](#nowpayments)) |
| `POST /v1/nowpayments/payments` | `{user_id, amount (USD, 2 dp), pay_currency}` | `{payment}`: `{id, status, price_amount, pay_currency, coin, pay_amount, pay_address, payin_extra_id, expires_at, …}` |
| `GET /v1/nowpayments/payments?user_id` / `GET /v1/nowpayments/payments/{id}?user_id=` | – | `{items, total}` / `{payment}` (a stale open payment is re-checked with NOWPayments in the background) |

## Deposits

1. **Intent.** `POST /v1/deposits/intents` returns the company receiving address for the chain (an audited admin setting), the USDT contract, the amount and an expiry (`intent_ttl_minutes`, default 60).
2. **Payment.** The CRM pays with MetaMask (`eth_sendTransaction` of `transfer(address,uint256)` on chain 56) or TronLink (`triggerSmartContract`), and gets the tx hash straight away. The manual fallback shows address, QR and amount, and the client pastes the hash.
3. **Submit.** `POST /v1/deposits/submit {intent_id, tx_hash}` stores the deposit as `pending`. A tx hash is unique per chain, so a second claim gets `409 tx_already_used`.
4. **Watcher** (every `WALLET_POLL_SECS`):
   - BSC: `eth_getTransactionReceipt`, `eth_blockNumber` and `eth_getBlockByNumber` through the configured RPC list, with fallbacks.
   - TRON: TronGrid `gettransactioninfobyid` and `getnowblock`.

   It then checks the following:
   - The transaction succeeded.
   - A `Transfer` log of the configured USDT contract pays one of the tenant's receiving addresses for that chain. The sender (the log's `from`) is recorded.
   - Confirmations reach the per-chain setting (defaults: BSC 15, TRON 20).
5. **Auto-credit rules.** The deposit is credited automatically (`kind deposit`, notification recorded) only when all of these hold:
   - the on-chain amount equals the intent amount;
   - the block time is between intent creation − 10 min and expiry + 24 h;
   - the sender address was never used by another client.

   Otherwise it goes to `review` in the Back Office unmatched queue, and staff credit it or reject it. The rule stops one client claiming another client's transfer by pasting its hash.
6. **Scanner** (every `WALLET_SCAN_SECS`). It reads incoming USDT transfers to the receiving addresses: BSC `eth_getLogs` filtered by `to`, TRON TronGrid `/v1/accounts/{addr}/transactions/trc20?only_to=true`.
   - Transfers nobody has claimed become `unmatched` deposits.
   - The unmatched queue suggests the client whose earlier deposits came from the same sender address.
   - Staff assign a deposit to a client (audited, then credited after confirmations) or reject it.

Deposit statuses: `pending` (submitted, not yet found on chain) → `confirming` (found, `confirmations`/`required_confirmations`) → `credited`. The other end states are `failed` (reverted, wrong token or recipient, not found after 24 h), `review`, `unmatched` and `rejected`.

### IB deposit events

The IB service decides a client's first deposit (FTD, CPA) from the engine ledgers and from deposit events. The wallet reports every **real** deposit credited to a wallet, so a client who deposits but has not moved the money to a trading account yet still counts:

| Ledger kind | Source | Reported |
|---|---|---|
| `deposit` | on-chain USDT deposit credited (auto, or after staff assign / approve) | yes |
| `manual_deposit` | Back Office add funds, reason "Deposit (external payment received)" | yes |
| `adjustment_in`, `commission`, `ib_payout`, `refund`, prop / PAMM / copy kinds, transfers from trading | – | no |

(A Back Office external deposit to a *trading account* is booked by the engine as `deposit`, which the IB service reads from the engine ledger.)

- **Outbox.** `src/ib.rs`: an `ib_deposit_events` row (`migrations/0004_ib_deposit_events.sql`, unique per ledger txn) is written in the same transaction as the credit. A worker (`WALLET_WORKERS=true`) sends it after the commit: `POST $IB_URL/v1/ib/events/deposit` with `X-Kalks-Internal: $IB_INTERNAL_TOKEN`, `X-Kalks-Tenant`, `X-Kalks-Service: wallet` and `{userId, amount, currency: "USDT", at, idempotencyKey, source: "wallet", kind}`. It never blocks or rolls back a deposit.
- **Idempotency.** `idempotencyKey = wallet:deposit:<tenant_id>:<ledger txn id>` and `at` = the ledger txn time, so a retry is answered `duplicate` by the IB service. The IB service keeps only the earliest deposit as the first one, so the later engine `transfer_in` of the same money is not counted again.
- **Retries.** Timeouts, 5xx, 401 / 403 and 404 (client not mirrored in the IB service yet) back off 15 s doubling to 1 h, for 60 attempts (`push_attempts`, `push_next_at`, `push_error`); 400 / 409 / 422 are not retried. The IB answer `firstDeposit` is stored on the row.
- **Reconciliation.** Every 10 minutes (and at start) the worker queues any `deposit` / `manual_deposit` ledger txn that has no row yet, which also backfills deposits booked before this existed. `GET /v1/admin/reconciliation` shows `ib_deposit_events: {enabled, reported, queued, failed}`; to resend failed rows, set `push_attempts = 0, push_next_at = NULL` on them (the key makes it safe).

## NOWPayments

Crypto deposits in other coins (BTC, ETH, LTC, TRX, BNB, USDT on TRC20 / BEP20 / ERC20, …) through [NOWPayments](https://nowpayments.io), next to the direct USDT deposits above (which stay as they are). The client chooses an amount in **USD** and a coin; NOWPayments gives an address (plus a memo / destination tag for the coins that need one) and the exact amount to pay; once NOWPayments reports the payment `finished`, the USD amount is credited to the client's wallet as USDT (1:1). Deposits only: no payouts through NOWPayments. Code: `src/nowpayments.rs` (API client, IPN signature), `src/ops/nowpayments.rs` (rules), `src/api/nowpayments.rs` (routes), `migrations/0005_nowpayments.sql`, `src/secrets.rs` (keys at rest).

### Setup (the owner)

1. **NOWPayments dashboard** (account.nowpayments.io; for tests the sandbox at account-sandbox.nowpayments.io has its own keys):
   - *Settings → Payments → API keys*: create an API key.
   - *Settings → Payments → Instant payment notifications*: generate the **IPN secret key** and set the IPN callback URL to `https://app.kalkstrade.com/api/payments/nowpayments/ipn` (the service also sends it with every payment).
   - *Coins settings*: enable the coins you want to accept, and set the outcome (payout) wallet / currency where NOWPayments sends the money.
2. **Back Office → Finance → Wallet settings** (`/finance/settings`), card **NOWPayments · crypto deposits** (needs `finance.settings`):
   - paste the **API key** and the **IPN secret** (password fields, write-only: afterwards the page shows only "Set ✓ …1234" and who changed it when);
   - **Test connection** (`GET /status` + `GET /merchant/coins` with the pasted or the stored key) and pick the coins to offer from the coins enabled in the merchant account (suggested: `usdttrc20, usdtbsc, usdterc20, btc, eth, ltc, trx, bnbbsc`);
   - minimum (default 10 USD) and maximum (default 50,000 USD) per deposit, **Client pays the fees** (default on: `is_fee_paid_by_user`, which NOWPayments always runs at a fixed rate), **Sandbox** (api-sandbox.nowpayments.io);
   - copy the **IPN callback URL** (it can be overridden per broker; empty = `WALLET_NOWPAYMENTS_IPN_URL`);
   - switch **Enabled** on (refused until both keys are set) and save with a reason (audited).
3. Clients then see **Crypto (NOWPayments)** next to "USDT direct" on the deposit page, with the offered coins that are also enabled in the merchant account (`GET /merchant/coins`, cached 10 minutes) (Client Area `/wallet/deposit`, and the app's Wallet → Deposit).

### Flow

1. **Create.** `POST /v1/nowpayments/payments` checks: switched on with both keys, the client's `deposits` restriction, the coin is offered, the amount (2 decimals, ≥ the broker minimum and NOWPayments' own minimum for the coin from `GET /min-amount?…&fiat_equivalent=usd`, cached 10 min, ≤ the maximum), at most 5 payments waiting for money and 20 new payments per hour per client. It stores the row (`creating`), then calls `POST /payment {price_amount, price_currency:"usd", pay_currency, order_id:"np_<24 hex>", order_description, ipn_callback_url, is_fee_paid_by_user, is_fixed_rate}` and keeps `payment_id`, `pay_address`, `payin_extra_id`, `pay_amount`, `expiration_estimate_date` (`open`). A refused create is kept as `error` (not shown to the client) and answered `502 provider_error`.
2. **IPN.** NOWPayments POSTs the payment to `https://app.kalkstrade.com/api/payments/nowpayments/ipn`. The Client Area route (`apps/crm/lib/nowpayments-ipn.ts`; public, open in maintenance mode, JSON ≤ 64 KB, 120 requests / min per IP; Caddy caps the body too) forwards the **raw body** and `x-nowpayments-sig` to `POST /v1/nowpayments/ipn` with the internal token. The service finds the payment (by `payment_id`; by our `order_id` when the create answer was lost; by `parent_payment_id` for a repeated deposit), takes **that broker's** IPN secret and checks `x-nowpayments-sig` = hex HMAC-SHA512 over the body re-serialised with sorted keys, exactly as NOWPayments' documented Node code does (`JSON.stringify(sortObject(body))`, nested objects and arrays included; the PHP / Python variants of the docs are accepted too), compared in constant time. Unknown payment → 404, no IPN secret → 503, bad or missing signature → 401 (nothing stored). A valid IPN is stored (`last_ipn`) and only **wakes** the check below: the body is never trusted for money.
3. **Check** (`ops::nowpayments::sync`, after every IPN, from the poller and from staff "Re-check"): `GET /payment/{id}` with the broker's key (and the payment's own sandbox / live environment). The answer is recorded (`np_status`, `actually_paid`, outcome, pay-in hash) and must be the payment we created (payment id, `order_id`, `price_amount`, `price_currency = usd`, `pay_currency`), else it goes to review.
4. **Credit.** `finished` with `pay_amount ≤ actually_paid ≤ pay_amount × 1.01` → `price_amount` USD credited as USDT in one transaction: row locked (`FOR UPDATE`, must still be `open`), ledger txn `deposit:nowpayments:<payment_id>` (kind `deposit`, unique per tenant: an IPN, a duplicate IPN, the poller and staff can race, it books once), `user available +X / sys:USDT:deposit:nowpayments −X`, row `credited`, the IB deposit event (it is a real deposit), the client notification `deposit.credited`, an audit entry. The withdrawal cooldown (`deposit_cooldown_hours`) counts it like an on-chain deposit.
5. **Review** (staff decide): `partially_paid`; overpaid by more than 1%; `finished` with less than asked; paid after the payment had expired / failed; a mismatch with what we created; a **repeated deposit** to the same address (NOWPayments `parent_payment_id`, recorded as its own row linked to the first payment). `failed` / `expired` / `refunded` without money → `closed`. A credited payment NOWPayments later reports `refunded` is logged as an error for staff.
6. **Poller** (the IPN fallback, `WALLET_WORKERS`, every `WALLET_POLL_SECS`): open payments are re-checked at most once a minute for 48 hours (7 days once money is on the way); unpaid ones are closed after 48 hours; a create call that never answered becomes `error` after 10 minutes. The client's status page also triggers a background re-check when the payment was not checked for 20 s.

States (`nowpayments_payments.status`): `creating` → `open` → `credited` | `review` → (`approval` →) `credited` | `rejected`; `closed`, `error`. NOWPayments' own status is kept in `np_status`: `waiting`, `confirming`, `confirmed`, `sending`, `partially_paid`, `finished`, `failed`, `refunded`, `expired`. The client sees `waiting`, `confirming`, `credited`, `review`, `expired`, `failed`, `refunded`, `rejected`; activity lists credited / pending / review rows as deposits with `kind: "nowpayments"` and the coin in `note`.

### Review, four-eyes, permissions

| Permission | Allows |
|---|---|
| `finance.read` | settings (without secrets), payments, raw IPN and API answers |
| `finance.settings` | change the settings and keys, Test connection |
| `finance.write` | approve & credit a payment in review (any USD amount, suggested: price × paid / asked), reject it (the client is notified), Re-check |
| `finance.adjust_approve` | the second approval above the four-eyes threshold |

The four-eyes threshold is the one of manual adjustments (`tenant_settings.adjust_approval_usd`, Finance → Adjustments). Above it the first approver's amount waits as `approval`; another staff member with `finance.adjust_approve` credits exactly that amount (the proposer gets 403, can only withdraw the proposal back to review). Audit actions: `wallet.settings.nowpayments` (before / after with secrets as `{set, last4}` only), `wallet.nowpayments.credited` (system or staff), `.approval_requested`, `.approval_withdrawn`, `.rejected`.

| Method & path (Back Office) | Body |
|---|---|
| `GET /v1/admin/nowpayments/settings` | – (`{settings, secrets_storage, default_coins, history}`) |
| `PUT /v1/admin/nowpayments/settings` | `{enabled?, sandbox?, api_key?, ipn_secret?, clear_api_key?, clear_ipn_secret?, coins?, min_deposit_usd?, max_deposit_usd?, fee_paid_by_user?, ipn_url?, reason}` (empty key = unchanged) |
| `POST /v1/admin/nowpayments/test` | `{api_key?, sandbox?}` → `{ok, status, merchant_coins, missing}` |
| `GET /v1/admin/nowpayments/payments?status=queue\|open\|credited\|review\|approval\|rejected\|closed\|error&user_id&q&page&limit` | – (`{items, total, counts}`) |
| `GET /v1/admin/nowpayments/payments/{id}` | – (with `last_ipn`, `np_payment`, `history`, `threshold_usd`, `can`) |
| `POST /v1/admin/nowpayments/payments/{id}/approve` | `{amount?, reason}` |
| `POST /v1/admin/nowpayments/payments/{id}/reject` | `{reason}` |
| `POST /v1/admin/nowpayments/payments/{id}/refresh` | `{}` |

The Back Office shows them under **Finance → Deposits → NOWPayments** (review queue, details with the raw IPN, approve / reject) and the settings under **Finance → Wallet settings**.

### Security

- The API key and IPN secret are AES-256-GCM encrypted with `WALLET_SECRETS_KEY` (bound to the broker and the setting), never returned by any API, never logged, and never part of an audit entry (only "set" and the last 4 characters). Without `WALLET_SECRETS_KEY` in production nothing can be saved. Rotating the key means pasting the NOWPayments keys again.
- Money is credited only on NOWPayments' own API answer, once per NOWPayments payment (unique ledger key + row lock), only for the amount the client asked for, and only automatically when the payment matches exactly; everything else is a staff decision with an audit trail.
- The IPN endpoint is public but useless without the broker's IPN secret: bad signatures are refused before anything is stored or NOWPayments is called.

## Withdrawals

`requested` → `approved` / `rejected` (staff, reason) → `paid` (staff enter the payout tx hash) → `completed`. The client can `cancel` while the withdrawal is `requested`.

- **Request.** `POST /v1/withdrawals` locks `amount` (available → locked).
  - Fee = `withdraw_fee_flat + amount × withdraw_fee_pct / 100` plus the chain's `withdraw_fee`. The client receives `amount − fee`.
  - Checks, in order:
    - `to_address` format (BSC `0x` + 40 hex, TRON base58check `T…`);
    - the chain is enabled;
    - `withdraw_min` / `withdraw_max`;
    - the daily total (server day, GMT+3) against `withdraw_daily_max`;
    - no credited deposit within `deposit_cooldown_hours`;
    - the available balance.
  - **KYC gate (D6).** The service asks the gateway (`GET /v1/internal/users/{id}`). `kyc_status` other than `verified` returns `403 kyc_required`.
- **Reject or cancel.** The funds are unlocked. Only a `requested` withdrawal can be rejected (`409 invalid_state` otherwise): payouts are sent by hand, so from `approved` on the money may already have left (a failed payout check also returns to `approved`; fix the hash instead).
- **Paid.** Staff enter the payout tx hash. The watcher checks the transaction on chain:
  - a USDT `Transfer` from a company address (a receiving or payout address of that chain) to `to_address` of exactly `amount − fee`;
  - it succeeded;
  - it has the required confirmations.

  Then it completes the withdrawal. Ledger: locked → `sys:withdrawal:<chain>` (net) and `sys:fees` (fee). A failed check returns the withdrawal to `approved` with `payout_error`. A payout hash can be used once.
- **Risk checklist** (`GET /v1/admin/withdrawals/{id}`):
  - KYC status;
  - last deposit time against the cooldown;
  - deposits, withdrawals and trading P&L proxy (money back from trading − money into trading);
  - request IP against the client's earlier IPs, and other clients using the same IP;
  - whether the destination address is new for this client or used by another client.

## Wallet and trading accounts

`to-trading` / `from-trading` call the engine's `POST /v1/ledger/transfers` (`services/trading/README.md`), only for live accounts of this user (engine `GET /v1/accounts?user_id=`, D24). Amounts use at most 2 decimals, the engine's precision.

- **to-trading.**
  1. Reserve: available → locked, and the transfer is recorded as `pending`.
  2. The engine `direction:"in"` with idempotency key `wallet-<tenant>-<transfer id>`.
  3. On success, commit (locked → `sys:trading`). On a definite engine rejection (4xx), roll back (locked → available).
- **from-trading.** The transfer is recorded `pending`, the engine is called with `direction:"out"` (free-margin check in the engine), and on success the wallet is credited.
- **Unknown outcome** (timeout, 5xx). The transfer stays `pending`. A recovery loop asks the engine `GET /v1/ledger/transfers/{key}` and commits, or re-sends the same idempotent request. Money is never booked twice and never lost.
- The client's `idempotency_key` makes the request itself idempotent: the same key returns the same transfer.

## Client restrictions

The Back Office can restrict a client (gateway `client_controls.rs`); the wallet reads the client's effective restrictions from `GET /v1/internal/users/{id}` (`restrictions`, with `freeze` expanded) on every operation it enforces and answers 403 `restricted` with a readable message:

| Restriction | Refused |
|---|---|
| `deposits` | `POST /v1/deposits/intents`, `POST /v1/deposits/submit` |
| `withdrawals` | `POST /v1/withdrawals`, `POST /v1/withdrawals/quote` |
| `transfers` | `POST /v1/wallets/{user_id}/to-trading`, `from-trading` (also the copy-trading allocations that use them) |
| `ib` | `POST /v1/wallets/transfers` credits of kind `commission` / `ib_payout` |

An unreachable gateway refuses these operations (503 `unavailable`); a replayed idempotent request still returns its stored result.

## Back Office API

The service checks the `finance.*` permission key on every staff route, from `X-Kalks-Staff-Perms` (so a custom role gets exactly what it was granted). Without that header (older callers) it checks the role with the map below; Super Admin and Platform Owner are always allowed. The admin BFF checks the same keys first (`apps/admin/lib/wallet-perms.ts`).

| Permission | Roles | Routes |
|---|---|---|
| `finance.read` | platform_owner, super_admin, admin, finance, compliance, risk_manager | all `GET /v1/admin/*` |
| `finance.write` | platform_owner, super_admin, admin, finance | assign / reject / recheck deposits, mark paid |
| `finance.adjust`, `finance.credit`, `finance.adjust_approve`, `finance.adjust_force` | see [Balance & credit](#balance--credit-manual-adjustments) | manual adjustments |
| `finance.approve` | platform_owner, super_admin, admin, finance | approve / reject withdrawals |
| `finance.settings` | platform_owner, super_admin, admin | receiving addresses, confirmations, limits, fees |

| Method & path | Body |
|---|---|
| `GET /v1/admin/summary` | – (queue counts, today's deposits and withdrawals, liabilities) |
| `GET /v1/admin/deposits?status&chain&user_id&q&page&limit` | – |
| `GET /v1/admin/deposits/{id}` | – (with suggested client for unmatched) |
| `POST /v1/admin/deposits/{id}/assign` | `{user_id, reason}` |
| `POST /v1/admin/deposits/{id}/reject` | `{reason}` |
| `POST /v1/admin/deposits/{id}/recheck` | `{}` |
| `GET /v1/admin/withdrawals?status&chain&user_id&page&limit` | – |
| `GET /v1/admin/withdrawals/{id}` | – (with `risk`) |
| `POST /v1/admin/withdrawals/{id}/approve` | `{note?}` |
| `POST /v1/admin/withdrawals/{id}/reject` | `{reason}` |
| `POST /v1/admin/withdrawals/{id}/paid` | `{tx_hash}` |
| `GET /v1/admin/wallets?user_ids=1,2&min_balance&page&limit` | – |
| `GET /v1/admin/wallets/{user_id}` | – (balances, ledger, deposits, withdrawals, transfers) |
| `POST /v1/admin/adjustments` | see [Balance & credit](#balance--credit-manual-adjustments) (the older `{idempotency_key, user_id, currency, amount, direction, reason}` body still works: wallet add / deduct, reason "correction") |
| `GET /v1/admin/settings` / `PUT /v1/admin/settings` | `{limits?, chains?: [{chain, receiving_address, payout_address?, confirmations, deposits_enabled, withdrawals_enabled, min_deposit, withdraw_fee}]}` |
| `GET /v1/admin/reconciliation` | – |
| `GET /v1/admin/audit?page&limit` | – |

The service writes every staff action to its append-only `audit_log`, with the actor, before and after values, reason, IP and user agent. Receiving-address changes are validated and audited, and are also visible as a settings history.

## Balance & credit (manual adjustments)

Back Office "Balance & credit" (client 360, Trading → Accounts, Finance → Wallets and Finance → Adjustments): add or deduct funds on the client's wallet or a trading account (live or demo), give or take credit on a trading account. Code: `src/ops/adjustments.rs`, routes `src/api/adjust.rs`, table `adjustments` (`migrations/0003_adjustments.sql`).

| Permission | Allows |
|---|---|
| `finance.adjust` | add / deduct funds |
| `finance.credit` | give / take credit (trading accounts only) |
| `finance.adjust_approve` | approve / reject requests above the four-eyes threshold |
| `finance.adjust_force` | force a trading-account deduction or credit take-back past the free margin (Super Admin) |
| `finance.settings` | the four-eyes threshold |

The admin BFF forwards the staff member's `finance.*` keys as `X-Kalks-Staff-Perms`; the service enforces the exact key (without the header it falls back to the role: finance roles for adjust / credit / approve, `super_admin` / `platform_owner` for force).

**Request** (`POST /v1/admin/adjustments`, and `POST /v1/admin/adjustments/preview` without `idempotency_key`):

```json
{ "idempotency_key": "<one per submit>", "user_id": 42, "target": "wallet | trading", "login": 10000001, "currency": "USDT",
  "op": "add | deduct | credit_in | credit_out", "category": "deposit | withdrawal | correction | compensation | bonus | fee | chargeback | other",
  "amount": "100", "comment": "internal, audit only (required)", "client_note": "shown on the client's statement (optional)",
  "notify": true, "force": false }
```

- **Preview** returns `{preview: {ok, before, after, limits, error?, amount_usd, threshold_usd, needs_approval, marginCall?, stopOut?}}`: the dialog's before → after step. Nothing is booked.
- **Create** validates, runs the same preview (a refused request is audited as `adjustment.refused` and returns `422` with the reason), then records the row. At or below the threshold it is booked at once (`applied`); above it the row is `pending`. Returns `{adjustment}`. The same `idempotency_key` returns the original adjustment with `replayed: true` (a double click books once); the same key with another request is `409 idempotency_conflict`.
- **Wallet target** (USDT, up to 6 decimals). Booked here in one transaction with the row, the audit entry and the client notification: `manual_deposit` (reason "Deposit (external payment received)", counter-account `sys:USDT:manual_deposit`), `manual_withdrawal` ("Withdrawal (paid externally)", `sys:USDT:manual_withdrawal`), otherwise `adjustment_in` / `adjustment_out` (`sys:USDT:adjustment`). A wallet never goes below 0 (the balance CHECK): a deduction above `available` is refused and `force` does not apply. Credit exists on trading accounts only.
- **Trading target** (account currency: USD, or USC for cent accounts; up to 2 decimals). The account must belong to the client. The engine books it (`POST /v1/admin/accounts/{login}/adjust`, key `wallet-adj-<tenant>-<id>`; ledger kinds `deposit` / `withdrawal` / `adjustment` / `credit`, limits and the force / negative-balance-protection rules in `services/trading/README.md#balance--credit`). The row is `processing` while the call is in flight; when the answer is lost the recovery loop (every `WALLET_POLL_SECS`, rows older than 30 s) re-sends the same key and the engine replays the booking, so nothing is booked twice.
- **Real money.** Only the two external reasons are deposits / withdrawals: the reports service counts `manual_deposit` / `manual_withdrawal` rows (wallet) and engine `deposit` / `withdrawal` (accounts) as money in / out and FTDs; every other reason is an adjustment.
- **Four-eyes.** `tenant_settings.adjust_approval_usd` (USD value: USDT 1:1, USC ÷ 100). `null` = off. `GET/PUT /v1/admin/adjustments/settings {approval_threshold_usd}` (PUT needs `finance.settings`, audited as `wallet.settings.adjust_threshold`). A pending request is booked by `POST /v1/admin/adjustments/{id}/approve {note?}` from another staff member with `finance.adjust_approve` (the requester gets 403). The booking re-checks the limits at that moment; if it can no longer be booked the row becomes `failed` with the reason. `POST .../{id}/reject {reason}` (approver, reason required) and `POST .../{id}/cancel` (the requester) book nothing.
- **Client.** The ledger note (wallet) / statement note (engine) is `client_note`, or a neutral label ("Balance adjustment", "Deposit", "Credit"). With `notify`, a notification row `adjustment.<wallet|account|credit>_<in|out>` is written in the booking transaction and pushed to support (bell + email per the client's `wallet` preference; link `/wallet/history` or `/accounts`).
- **Audit.** `adjustment.requested`, `adjustment.approved`, `adjustment.rejected`, `adjustment.cancelled`, `adjustment.applied` (before / after balances, ledger txn and kind, requester and approver), `adjustment.failed`, `adjustment.refused`. The engine writes its own `account.balance` / `account.credit` audit for trading targets.

| Method & path | |
|---|---|
| `GET /v1/admin/adjustments?user_id&login&staff&category&op&target&status&from&to&page&limit` | `{items, total, totals:{added_usd, deducted_usd, credit_in_usd, credit_out_usd, net_balance_usd, net_credit_usd, external_deposits_usd, external_withdrawals_usd, applied, pending, pending_usd, declined, failed}, staff[], threshold_usd, categories[]}`; `staff` matches the requester or the approver; `from` inclusive, `to` exclusive; `limit` up to 2000 (CSV export) |
| `GET /v1/admin/adjustments/{id}` | `{adjustment}` |
| `GET /v1/admin/adjustments/targets/{user_id}` | `{wallet, accounts (engine views, live and demo), engine_available, open[], recent[], threshold_usd, can:{adjust, credit, approve, force}}` |

## Environment

| Variable | Default | |
|---|---|---|
| `WALLET_BIND` | `127.0.0.1:8095` | |
| `WALLET_DATABASE_URL` | `postgres://postgres@127.0.0.1:5433/kalks_wallet` | created on first start |
| `WALLET_INTERNAL_TOKEN` | – | required in production |
| `WALLET_ENV` | `development` | `production` enforces the token |
| `WALLET_LOG_FORMAT` | `json` | `text` for local |
| `WALLET_WORKERS` | `true` | watcher, scanner, recovery (run on one instance only) |
| `WALLET_POLL_SECS` / `WALLET_SCAN_SECS` | `10` / `60` | |
| `WALLET_BSC_RPC_URLS` | `https://bsc-rpc.publicnode.com,https://bsc-dataseed.binance.org,https://bsc-dataseed1.defibit.io` | tried in order. The Binance dataseeds refuse `eth_getLogs`, so keep a getLogs-capable node first |
| `WALLET_TRONGRID_URL` | `https://api.trongrid.io` | |
| `TRONGRID_API_KEY` | – | read from the env or repo-root `.env.tron` |
| `WALLET_BSC_ADDRESS` / `WALLET_TRON_ADDRESS` | – | seed the receiving addresses on first start only. After that, the admin setting wins |
| `WALLET_BSC_CONFIRMATIONS` / `WALLET_TRON_CONFIRMATIONS` | `15` / `20` | seed values |
| `TRADING_URL` / `TRADING_INTERNAL_TOKEN` | `http://127.0.0.1:8090` | engine |
| `GATEWAY_URL` / `GATEWAY_INTERNAL_TOKEN` | `http://127.0.0.1:8080` | KYC status |
| `SUPPORT_URL` / `SUPPORT_INTERNAL_TOKEN` | `http://127.0.0.1:8100` / – | notification push (bell, realtime, email); empty URL = in-app only |
| `WALLET_SECRETS_KEY` | – (development: a fixed key) | 64 hex: encrypts the NOWPayments API key / IPN secret at rest. Required to save or use them in production; `deploy.sh` generates it once. Back it up with the database |
| `WALLET_NOWPAYMENTS_IPN_URL` | `https://app.kalkstrade.com/api/payments/nowpayments/ipn` | default IPN callback sent with each payment (Back Office override per broker) |
| `WALLET_NOWPAYMENTS_API_URL` | – | overrides the NOWPayments API base (tests and local mocks only); empty = `https://api.nowpayments.io/v1`, or `https://api-sandbox.nowpayments.io/v1` with Sandbox on |
| `IB_URL` / `IB_INTERNAL_TOKEN` | empty in development, `http://127.0.0.1:8096` in production / – | real deposits reported to the IB service ([IB deposit events](#ib-deposit-events)); empty URL = not sent (events stay queued and go out once it is set) |

The apps need `WALLET_URL` (default `http://127.0.0.1:8095`) and `WALLET_INTERNAL_TOKEN` (server-only) in `apps/crm` and `apps/admin`.

## Tests

`cargo test -p wallet` runs:

- **Unit tests:** money parsing and scale, base58check TRON addresses, ABI log decoding, fee maths, address validation, config redaction, secrets at rest (round trip, bound to tenant and setting, production needs a key), the NOWPayments IPN signature (vectors from NOWPayments' documented Node code: valid, tampered, wrong secret, nested objects and arrays, JS number formatting, PHP / Python variants).
- **NOWPayments** (`tests/nowpayments.rs`, a mock NOWPayments API on a random port): disabled until switched on with both keys, keys write-only and encrypted (never in an answer or the audit log), settings permissions, Test connection, limits and minimums, the 5-open cap, the deposits restriction; create → signed IPN → re-read → credited once (an IPN claiming `finished` while the API says `waiting` books nothing; duplicate IPNs, the poller and six concurrent re-checks still book once; IB event and notification); bad / missing / tampered signatures, unknown payment, no IPN secret; partially paid → review → four-eyes (proposer can't approve, another amount refused, second approver credits); reject notifies; overpaid → review, within 1% credited; expired (poller) then paid late → review; repeated deposit → its own row in review; a refused create.
- **PostgreSQL integration tests** (`tests/flows.rs`, throw-away database) with a mock chain and a mock engine:
  - transfer idempotency (replay, conflict, concurrent duplicates);
  - insufficient funds;
  - deposit intent → submit → confirmations → credit;
  - auto-credit rules sending deposits to review;
  - unmatched scan and assign;
  - withdrawal limits, cooldown, KYC gate, approve → paid → verify → completed, reject and cancel unlock;
  - to-trading / from-trading with engine success, rejection and timeout recovery;
  - real deposits reported to a mock IB service: only on-chain `deposit` and `manual_deposit` are queued (not bonuses, commissions, IB payouts or transfers from trading), nothing sent while `IB_URL` is empty, IB down → back-off, a 404 client retried without holding the others, each event delivered once with the key from the ledger txn and the ledger time, the sweep re-queuing a missing row with the same key, the reconciliation counts;
  - the ledger invariants after every flow (Σ postings = 0 per txn, balances = Σ postings, append-only).
- **Balance & credit** (`tests/adjustments.rs`): wallet add / deduct with the balance and ledger after each step, the notification and history note, a double submit booking once and a key conflict, over-deduction refused and audited, the external kinds; four-eyes (threshold set by `finance.settings` only, pending books nothing, the requester can't approve, a second staff member approves, double approval, reject / cancel, an approval that can no longer be booked fails); permissions (403 without `finance.adjust` / `finance.credit` / `finance.adjust_force`, role fallback without the header); trading accounts through the mock engine (free-margin refusal, force, credit take-back limits, another client's account refused, a lost engine answer settled by recovery with one booking, four-eyes on credit).

## Production checklist

- Addresses:
  - Set the receiving and payout addresses for each tenant in Back Office → Finance → Settings, and check them against the hardware wallet.
  - Payouts are manual for now: staff send from the company wallet and enter the hash. Hot-wallet signing, HD deposit addresses per client, and sweeps with TRX gas (D39) are a later phase.
- Paid RPC:
  - Use a paid BSC RPC (or a self-hosted node) with an `eth_getLogs` allowance; public endpoints rate-limit.
  - Use a TronGrid key per environment.
- Notifications: every row of `notifications` (deposit credited / rejected, withdrawal requested / approved / rejected / completed, transfers, credits) is written in the same transaction as the money change and then pushed by `src/notifier.rs` to the support service (`POST $SUPPORT_URL/v1/notify`, type `wallet.<kind>`, link `/wallet/history`, `dedupeKey wallet:n:<id>`). Support shows it in the Client Area and Kalks Trader bells, on the realtime stream, and emails it per the client's `wallet` preference. The push runs after the commit (woken at once, else every 5 s), retries with backoff (15 s doubling to 1 h, 12 attempts, `push_attempts` / `push_error`), never blocks or rolls back the money transaction, and support's wallet polling adapter uses the same dedupe key, so nothing is shown or emailed twice.
- RLS: every table carries `tenant_id` and queries are tenant-scoped, but Row-Level Security policies are not enabled yet (same as the trading engine).
- Workers: run `WALLET_WORKERS=true` on exactly one instance.
- NOWPayments: `WALLET_SECRETS_KEY` set (deploy.sh) and backed up; keys pasted and **Test connection** green; the IPN URL also set in the NOWPayments dashboard; try one payment with Sandbox on first; the reports service does not read NOWPayments deposits yet (they are in the wallet ledger as kind `deposit` and in the IB deposit events).

# trading

The Kalks trading engine: trading accounts, orders and positions (netting and hedging, cent), margin, margin call and stop-out, swaps, the double-entry ledger for balance / credit / bonus, wallet transfers, the Back Office dealing desk, and [Kalks FX Options](#kalks-fx-options) (European cash-settled options, traded in Options accounts: see [CFD and Options accounts](#cfd-and-options-accounts)). It is a Rust service (axum 0.8, sqlx 0.9, PostgreSQL) on `127.0.0.1:8090`.

The engine executes B-book only. A/B routing is decided and recorded on every ticket. A-book trades are passed to an LP adapter, which is a stub until an LP is signed (D2, D25).

- [Run locally](#run-locally)
- [Architecture](#architecture)
- [Data model](#data-model)
- [Trading rules](#trading-rules)
- [API conventions](#api-conventions)
- [Terminal API](#terminal-api)
- [Client Area API](#client-area-api)
- [Wallet transfers](#wallet-transfers)
- [Dealing desk API](#dealing-desk-api)
- [Admin account API](#admin-account-api)
- [Copy trading and PAMM](#copy-trading-and-pamm)
- [MAM (multi-account manager)](#mam-multi-account-manager)
- [Client controls](#client-controls)
- [CFD and Options accounts](#cfd-and-options-accounts)
- [Kalks FX Options](#kalks-fx-options)
- [Options order book](#options-order-book)
- [Streams](#streams)
- [How the apps integrate](#how-the-apps-integrate)
- [Environment](#environment)
- [Tests](#tests)
- [Known gaps](#known-gaps)

## Run locally

Before you start, you need PostgreSQL 16 on `127.0.0.1:5433` (user `postgres`, trust auth) and market-data on `:8081`. The repo-root README shows how to start both.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
cargo build -p trading
# background, logs as JSON lines
(cd services/trading && nohup ../../target/debug/trading > ~/.kalks-local/trading.log 2>&1 &)
curl -s localhost:8090/health
cargo test -p trading
```

On first start the engine creates the `kalks_trading` database and runs `migrations/`. It reads `TRADING_*` from the repo-root `.env.local`. `TRADING_SESSION_SECRET` and `TRADING_INTERNAL_TOKEN` are generated there and are never committed.

## Architecture

```
            market-data :8081 ──WS /v1/stream?group=standard|pro|ecn|cent (group spread applied)
                     │
               feed (1 socket per spread group) ── QuoteBook (spread group, symbol) → bid/ask
                     │ tick
   ┌─────────────────┼──────────────────────────┐
 shard 0          shard 1   …               shard N-1      (TRADING_SHARDS, default 8; login % N)
 single writer    single writer             single writer
 accounts in memory (AccountState)
   │  command = closure over (Tx, Env)  →  events
   │  commit: events + projections + ledger + audit rows in ONE Postgres transaction
   │  then swap the new state in, publish stream frames, call the LP hook for A-book fills
   ▼
 PostgreSQL kalks_trading: events (source of truth) + projections
```

- **Single writer per account.** Every request for an account is sent to that account's shard task as a closure. The shard runs it against a copy of the state (`engine::Tx`). It commits the resulting events in one database transaction and only then replaces the in-memory state. If the commit fails, memory stays unchanged. Commands are served before price ticks.
- **Event sourced.** `events` holds one ordered stream per trading account. `AccountState::apply` is the only code that changes state, and it runs both live and during replay. Events carry every computed value (fill prices, P&L, ledger legs, ids), so replay needs no market data. On start, every stream is replayed. The engine then compares the rebuilt balances with the ledger projection (`ledger_accounts`) and checks that every ledger transaction balances. It refuses to start on any mismatch.
- **Pure engine core.** `src/engine/` (trade, risk, funds, dealing) does no IO. It reads an `Env` (specs, tenant config, group, quotes, clock, id allocator) and emits events.
- **Price feed (decision).** The engine opens one WebSocket per spread group to market-data (`/v1/stream?group=<spread_group>`), subscribed to every symbol. market-data already applies each group's markup (`/v1/admin/spreads`). The engine therefore fills at exactly the bid/ask the client sees in the terminal and on charts. There is one source of truth for markups, edited in the Back Office, and markup logic is never duplicated in two services. The alternative was one raw feed plus a copy of the markups read with `MARKET_DATA_ADMIN_TOKEN`. That would have needed a second copy of the spread maths and could drift from what clients see. A new spread group gets its socket within 5 s. Each tick goes to the shards, and a shard evaluates only accounts that have a position or pending order on that (spread group, symbol).
- **Dealer markup.** The per-account dealer markup (`markupPips`) is added by the engine on top of the group quote. Split half on the bid, half on the ask.

Source layout:

| Path | |
|---|---|
| `src/specs.rs` | contract specs from `config/instruments.json` + `config/trading-specs.json`, sessions, server time (GMT+2/+3), rollover instants |
| `src/model.rs`, `src/state.rs` | domain types, events, `AccountState::apply` / replay |
| `src/engine/` | pure decision logic (`trade`, `risk`, `funds`, `dealing`), P&L / margin / metrics |
| `src/rules.rs` | groups, tenant policy, symbol controls, A/B routing rules |
| `src/persist.rs` | migrations, replay, one-transaction commit of events + projections + ledger + audit |
| `src/shard.rs` | shard tasks, `Hub` handle, stream fan-out, LP adapter trait (`NullLp`) |
| `src/feed.rs` | market-data sockets, `QuoteBook` |
| `src/api/` | HTTP handlers (`terminal`, `accounts`, `ledger`, `dealing`, `admin`, `stream`) |
| `src/views.rs` | JSON views (terminal and Back Office shapes) |
| `src/social/` | copy trading and PAMM: `math` (sizing, HWM fees, NAV, statistics), `mirror` (follower side of a master event), `copier` (event tap, catch-up, guard, scheduler), `pamm`, `stats`, `wallet` (client + outbox) |
| `src/api/social.rs`, `src/api/social_admin.rs` | Client Area and Back Office social routes |
| `src/options/` | Kalks FX Options link: `OptionsCtx` (snapshot poller, raw spots, mark / scenario caches, suitability), `snapshot` (parsed reference data, gates), `pricing` (same maths as services/options), `settle` (expiry scheduler), `hedger` (house delta hedge) |
| `src/engine/options.rs`, `src/api/options.rs` | option orders, combos, closes, knocks, settlement, re-run, void, scenario margin, stop-out by units; the options HTTP routes |
| `src/book/` | the options **order book** (docs/OPTIONS-EXCHANGE.md): `types`, `matching` (pure price-time matching), `actor` (one per tenant / kind / underlying, group commit), `journal` (persistence, load, replay audit), `outbox` (fills and removals applied to the accounts), `reserve` (order margin, `AccountState.book`), `md` (top of book, depth, feed), `entry` (`submit`, the one way in) |
| `src/engine/options_book.rs`, `src/api/options_book.rs`, `src/api/book_feed.rs` | book entry gates and reservations, `apply_fill` / `apply_done`, stops and SL / TP; the terminal book routes; the internal market-data feed |

## Data model

Database `kalks_trading` (`migrations/0001_trading.sql`). Every table has `tenant_id` and an RLS policy on `current_setting('kalks.tenant_id')`. The engine connects as the table owner and filters by tenant itself. The policies protect every other database role.

| Table | Kind | Contents |
|---|---|---|
| `events` | source of truth, append-only | `(login, version)` unique; `kind`, `actor` (`client`, `user:<id>`, `staff:<id>`, `wallet`, `system`), `payload` (the event JSON) |
| `accounts` | projection | login, tenant, user_id, kind, group, mode, cent, currency (USD/USC), leverage, status, dealer controls, demo config, balance/credit/bonus, version, `last_activity_at` |
| `account_credentials` | auth | argon2id trading + investor hashes, lockout counters (not in the event stream) |
| `orders` | projection | every pending order and its outcome (`pending`, `filled`, `cancelled`, `expired`, `rejected`) |
| `positions` | projection | open / closed / voided positions, full JSON in `data` (route history, book carry, trailing) |
| `deals` | projection | entry (`in`) and exit (`out`, `out_by`) deals with profit, swap, commission, reason, book, ledger txn, and the position snapshot for "reopen" |
| `ledger_txns` / `ledger_postings` | projection, append-only | double entry. `UNIQUE (tenant_id, idempotency_key)`. A deferred constraint trigger rejects any transaction whose postings do not sum to 0 per currency |
| `ledger_accounts` | projection | running balance per ledger account (`acct:<login>:balance|credit|bonus`, `house:<name>:<ccy>`) |
| `audit_log` | append-only | dealing and account-ops audit (staff, action, tickets, before/after, reason code, note, flags) |
| `groups`, `tenant_policies`, `symbol_controls`, `routing_rules` | config | edited through the dealing/admin API, every change audited |
| `rollovers` | ops | swap rollovers done per day (catch-up after downtime) |
| `terminal_sessions`, `sso_tokens` | auth | HMAC-SHA256 hashes of opaque tokens only |

Ledger accounts and legs (amounts in the account currency; cent accounts use USC = USD × 100):

| Transaction | Legs |
|---|---|
| wallet → account (`transfer_in`) | `acct:L:balance +X` · `house:wallet_clearing:USD −X` (cent: `+100X USC` on the account, `house:fx:USC −100X`, `house:fx:USD +X`, `house:wallet_clearing:USD −X`) |
| account → wallet (`transfer_out`) | the reverse |
| close (`trade_pnl`) | `acct:L:balance +(profit+swap)` · `house:trading_pnl −profit` · `house:swap −swap` |
| commission | `acct:L:balance −c` · `house:commission +c` |
| staff deposit / withdrawal / adjustment | `acct:L:balance ±x` · `house:external` / `house:adjustments` |
| credit / bonus | `acct:L:credit|bonus ±x` · `house:credit_issued|bonus_issued` |
| negative balance protection (`nbp`) | `acct:L:balance +|neg|` · `house:nbp −|neg|` |
| demo funding / refill | `acct:L:balance +x` · `house:demo_funding −x` |
| reopen deal / void (`reversal`) | exact negation of the original legs |
| option premium (`option_premium`) | buy: `acct:L:balance −P` · `house:options_premium +P`; sell / close: the reverse direction |
| option expiry payout, knock-out rebate (`option_settlement`) | house venue: `acct:L:balance ±X` · `house:options_settlement ∓X` (key `settle:{SYMBOL:DATE}:{run}:{ticket}` / `knock:{ticket}`); order book position: against `house:options_clearing.{U}.{YYYYMMDD}:USD` like its fills (cent accounts in the 4-leg form through `house:fx:USC/USD`, the USD amount rounded once) |
| clearing rounding sweep (no account) | `house:options_clearing.{U}.{YYYYMMDD}:USD ∓r` · `house:options_rounding:USD ±r`, `r` ≤ 0.005 USD × settled book positions (key `clrsweep:{tenant}:{kind}:{SYMBOL:DATE}:{run}`, login 0) |
| option settlement re-run, option void (`reversal`) | exact negation (`settle-rev:…`, `void:{ticket}:{deal}`), then the new settlement |

Balance = Σ postings on `acct:L:balance`. Every transaction sums to 0 per currency; this is enforced by the engine, the tests, and the database trigger.

Ids: logins are 8 digits (live 10 000 001+, demo 50 000 001+). Order and position tickets share one sequence from 1 000 001. A position takes the ticket of the order that opened it (MT5 convention). Deals count from 2 000 001.

## Trading rules

| Area | Rule |
|---|---|
| Money | `rust_decimal` everywhere. Balances are rounded to 0.01 of the account currency, half away from zero. JSON floats are converted through their shortest decimal text |
| Fills | Buy at ask, sell at bid, from the account's spread-group quote (+ dealer markup). Limit orders fill at the market once reached (never worse than the limit). Stops fill at the market |
| Order types | market, limit, stop, stop-limit (stop triggers → limit at `stopLimit`). Expiry `GTC`, `Today` (next 00:00 server time) or a date. Server-side trailing stop. OCO pairs (`ocoWith`: when one fills, the other is cancelled). One-cancels-other applies to fills only |
| Deviation | `requestedPrice` + `deviationPoints` on market orders and closes: if the fill is further away than that, the request is rejected with code `requote` and the current bid/ask (D106) |
| Netting | One position per symbol. The same side adds to it (volume-weighted price). The opposite side reduces or closes it. A larger opposite order reverses it: long 1, sell 3 gives short 2 (D18). The new position records `reversedFrom` |
| Hedging | Every fill is a new position. Close By closes the overlap of two opposite positions at the open price of the second, with no spread paid |
| Partial close | Volume on the lot step, and the remainder must stay ≥ the minimum lot. Swap and commission are split pro rata |
| Commission | Round turn per lot (group `commissionPerLot`, a symbol can override it). Charged when exposure is opened, and shown per deal |
| Margin | notional (contract × price, converted to USD) × `margin_pct` / min(account leverage, symbol max leverage). Hedged volume is charged at the group's `hedgedMarginPct` for both legs (D14). Recomputed on every tick at mid price |
| Free-margin check | An order that increases margin is rejected (`no_money`) when equity − commission − new margin < 0 |
| Margin call / stop-out | Margin level = equity / margin × 100. At or below the group's margin call % the engine emits a notification. It clears at 5 points above the level. At or below stop-out % it closes the largest losing position first, repeating until the level is above stop-out (D16) |
| NBP | A negative balance with no open positions is reset to 0 with a ledger posting (D16) |
| Swaps | Charged at 00:00 server time (GMT+3 during US DST, GMT+2 otherwise, same rule as market-data). The day that just ended decides: FX, metals, indices, energies and stocks Mon–Fri nights, crypto every night. The triple day comes from the spec (FX/metals Wednesday, indices/energies/stocks Friday). Swap-free groups are skipped. Swaps accrue on the position and are realised on close |
| Sessions | Orders and closes are rejected with `market_closed` outside the session: crypto 24/7; FX, metals, indices and energies closed Saturday and Sunday server time; US stocks 09:30–16:00 New York. Same rules as market-data |
| Stale feed | Quotes older than `TRADING_MAX_QUOTE_AGE_SECS` are not tradable (`stale_price`) |
| Controls | Account status `active` / `close_only` / `read_only` / `disabled` / `expired`. Dealer controls: trading disabled, close-only, max lot, execution delay (≤ 500 ms, only if the tenant allows it), markup pips. Symbol controls: `halt` / `close-only` per group or all (D115, D140). SL/TP, stop-out and expiry still run on halted symbols |
| Routing | Dealer override → first matching enabled rule (fields Login, Group, Symbol, Lot size; others do not match yet) → account default → group route. Book transfer moves a whole ticket or splits the moved volume into a child ticket with `parentTicket` |
| Demo | Initial balance (group default or chosen, 100–1 000 000). A refill tops the balance back to the initial amount, at most N per server day. The account expires after `expiry_days` with no terminal login (D8) |
| Leverage | From the group list. Clients can change it only when no positions are open (D15); staff can change it any time |
| Product | Every account trades ONE product, its group's `product`: a **CFD account** opens CFD positions and orders only, an **Options account** option positions and orders only (house prices and the order book). Anything else is refused with `product_mismatch`, for dealers too; closing, SL / TP and stop-out never are. See [CFD and Options accounts](#cfd-and-options-accounts) |

## API conventions

- **Base URL.** Base `http://127.0.0.1:8090`. JSON in and out, camelCase keys.
- **Internal token.** Every route except `GET /health`, `GET /v1/terminal/stream` and `GET /v1/dealing/stream` needs the header `X-Kalks-Internal: $TRADING_INTERNAL_TOKEN`. Only the apps' BFFs and internal services hold it.
- **Tenant.** Set it with `X-Kalks-Tenant: <slug>`; the default is `kalks`. Forward the client IP as `X-Forwarded-For` and the user agent as `User-Agent`.
- **Numbers.**
  - Money, prices and volumes are JSON numbers, and requests also accept numeric strings.
  - Terminal and client tickets are numbers. Dealing tickets are strings, because the Back Office contract uses strings.
  - Times are RFC 3339 UTC.
  - Date filters (`from`/`to`) accept `YYYY-MM-DD` or RFC 3339. `from` is inclusive and `to` exclusive.
- **Errors.** Errors return `{"error": {"code", "message", ...}}`:

| HTTP | code | when |
|---|---|---|
| 400 | `bad_request` | malformed JSON / ids |
| 401 | `unauthorized` | no / expired terminal session, bad stream ticket |
| 403 | `forbidden`, `read_only` | missing internal token, staff role not allowed; investor session writing |
| 404 | `not_found` | unknown account / ticket / deal |
| 409 | `requote` (+`bid`,`ask`), `invalid_credentials`, `locked`, `account_limit`, `idempotency_conflict`, `duplicate_idempotency_key`, `exists` | |
| 422 | `validation` (+`field`) or an engine code: `product_mismatch`, `market_closed`, `no_price`, `stale_price`, `no_money`, `invalid_volume`, `invalid_price`, `invalid_sl`, `invalid_tp`, `invalid_expiry`, `invalid_oco`, `max_lot`, `close_only`, `trading_disabled`, `account_status`, `symbol_halted`, `symbol_close_only`, `not_hedging`, `invalid_close_by`, `positions_open`, `invalid_leverage`, `insufficient_funds`, `refill_limit`, `refill_not_needed`, `demo_account`, `off_market`, `same_book`, `already_reversed`, `no_change`, … | |
| 429 | `rate_limited` (+`retryAfter`) | terminal login / SSO throttling |

Write responses can carry `notifications: [{kind, message, data}]` (fill, close, sl, tp, nbp, margin_call, stop_out, order_filled, …). The same notifications also go out on the stream.

The examples below use `H='-H x-kalks-internal:$TOK -H content-type:application/json'`.

## Terminal API

Terminal requests use `Authorization: Bearer <session token>` (from login or SSO). An investor session has `readOnly: true`; every write returns `403 read_only` (D107).

| Method & path | Body / query | Response |
|---|---|---|
| `POST /v1/terminal/login` | `{login, password}` (trading or investor password) | `{token, expiresAt, readOnly, account}`. Wrong password: 409 `invalid_credentials`; 10 failures lock the login for 15 min (409 `locked`); expired demo: 403 |
| `POST /v1/terminal/sso` | `{token}` from `POST /v1/accounts/{login}/sso` (one-time, 60 s) | same as login (`readOnly: false`) |
| `POST /v1/terminal/logout` | – | `{status:"ok"}` |
| `GET /v1/terminal/state?historyLimit=50` | – | `{account, positions[], orders[], history:{deals[]}, readOnly, serverTime}` |
| `GET /v1/terminal/history?from&to&page&limit` | – | same shape as `GET /v1/accounts/{login}/history` |
| `POST /v1/terminal/orders` | order body (below) | filled: `{status:"filled", orderTicket, positionTicket, price, book, deals[], delayMs?, notifications}`; pending: `{status:"placed", ticket, price, book}`; repeated `clientOrderId`: `{status:"duplicate", ticket}` |
| `PATCH /v1/terminal/orders/{ticket}` | `{price?, stopLimit?, volume?, sl?, tp?, trailingPoints?, expiry?, expiryAt?}` (`null` clears sl/tp/trailing) | `{order}` |
| `DELETE /v1/terminal/orders/{ticket}` | – | `{status:"cancelled", ticket}` |
| `POST /v1/terminal/positions/{ticket}/close` | `{volume?, deviationPoints?, requestedPrice?}` (no body = full close) | `{status:"closed", dealId, profit}`; `profit` = price P&L + swap share booked |
| `PATCH /v1/terminal/positions/{ticket}` | `{sl?, tp?, trailingPoints?}` (`null` clears) | `{position}` |
| `POST /v1/terminal/positions/close-by` | `{ticket, by}` | `{status:"closed", deals:[id,id]}` |
| `POST /v1/terminal/bulk-close` | `{filter: "all"|"profitable"|"losing"|"pending"|"buys"|"sells", symbol?}` | `{done[], failed[{ticket,error}], profit}` |
| `POST /v1/terminal/stream-ticket` | – | `{ticket, expiresIn:30}`, see [Streams](#streams) |

Order body:

```json
{
  "symbol": "EURUSD", "side": "buy", "type": "market | limit | stop | stop_limit", "volume": 0.10,
  "price": 1.1300,          // limit / stop price (stop trigger for stop_limit)
  "stopLimit": 1.1310,      // stop_limit only
  "sl": 1.1250, "tp": 1.1400, "trailingPoints": 150,
  "expiry": "GTC | Today | Date | 2026-10-02 | <RFC 3339>", "expiryAt": "<RFC 3339 when expiry=Date>",
  "requestedPrice": 1.13672, "deviationPoints": 20,   // market: requote if the fill moves further
  "ocoWith": 1000003,       // pending: link with an existing pending order
  "source": "manual | api | fix | webhook | strategy | copy | pamm | ai",   // D84, default manual
  "platform": "Web | iOS | Android | API", "comment": "…", "clientOrderId": "uuid-1"
}
```

Position view (terminal / client):

```json
{"ticket":1000001,"login":50000001,"symbol":"BTCUSD","side":"buy","volume":0.1,"openPrice":83370.08,"openTime":"…",
 "sl":82000.0,"tp":86000.0,"trailingPoints":null,"swap":0.0,"commission":0.0,"currentPrice":83419.95,"profit":4.99,
 "source":"manual","platform":"Web","comment":"","book":"B","parentTicket":null,"childTickets":[],"priceCorrected":false,"reversedFrom":null}
```

The order view has `ticket, login, symbol, side, type, volume, price, stopLimit, triggered, sl, tp, trailingPoints, expiry, expiryAt, oco, source, platform, comment, book, placedAt, clientOrderId`.

The deal view has `id, login, positionTicket, orderTicket, symbol, side, positionSide, entry (in|out|out_by), volume, price, profit, swap, commission, reason (client|dealer|sl|tp|stop_out|close_by|pending_fill|force|price_correction), book, time, openPrice, openTime, source, comment, priceCorrection, ledgerTxn, reversed`.

The account view (terminal, CRM, admin) has:

```json
{"login":50000001,"userId":42,"type":"demo","group":"standard","groupName":"Standard","product":"cfd","mode":"hedging","cent":false,
 "currency":"USD","baseCurrency":"USD","leverage":500,"leverages":[50,100,200,500,1000],"status":"active","name":"…",
 "route":"B","marginCall":false,"marginCallLevel":100.0,"stopOutLevel":50.0,"positions":1,"orders":2,
 "controls":{"tradingDisabled":false,"closeOnly":false,"maxLot":null,"execDelayMs":0,"markupPips":0.0},
 "balance":10000.0,"credit":0.0,"bonus":0.0,"profit":-0.02,"swap":0.0,"equity":9999.98,"margin":834.14,
 "freeMargin":9165.84,"marginLevel":1198.84,"withdrawable":9165.84,
 "demo":{"initialBalance":10000.0,"refillsPerDay":3,"refillsUsedToday":0,"expiryDays":10},
 "createdAt":"…","version":18}
```

Cent accounts report `currency: "USC"`: every amount is USD × 100 and lot sizes are unchanged (D30).

## Client Area API

The CRM BFF calls these after its own session check. It passes the gateway user id in `X-Kalks-User-Id: <id>` (or `?user_id=`). Every `/v1/accounts/{login}/*` route returns 404 unless the account belongs to that user and tenant.

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/groups` | – | `{groups[]}`: enabled groups (code, name, **product** (`cfd` \| `options`), mode, cent, accountTypes, leverages, defaultLeverage, marginCallPct, stopOutPct, hedgedMarginPct, minDeposit, swapFree, commissionPerLot, route, spreadGroup, maxAccountsPerUser, demo*) |
| `GET /v1/symbols` | – | `{symbols[]}`: contract specs (digits, point, pipSize, contractSize, profitCurrency, lot min/max/step, marginPct, maxLeverage, swapLong/Short in points, tripleSwapDay, session, open now) |
| `POST /v1/accounts` | `{userId?, type:"live"|"demo", group, leverage?, name?, password?, investorPassword?, initialBalance? (demo)}` | `{account, credentials:{login, password?, investorPassword?}}`. Missing passwords are generated and returned once. Passwords are 8–64 chars with letters and digits, and the two must differ. The group's `maxAccountsPerUser` is enforced per (type, product): the client's live CFD accounts, demo CFD accounts, live Options accounts and demo Options accounts count separately; archived / closed accounts and platform-managed ones (copy, PAMM, MAM, prop, market maker) don't count (409 `account_limit`) |
| `GET /v1/accounts?user_id=` | – | `{accounts:[account view…]}` |
| `GET /v1/accounts/{login}` | – | `{account, positions[], orders[]}` |
| `POST /v1/accounts/{login}/demo-refill` | – | `{status, amount, balance}` |
| `POST /v1/accounts/{login}/passwords` | `{kind:"trading"|"investor", password}` | `{status, sessionsRevoked}`; the CRM does the email OTP first (D20) |
| `POST /v1/accounts/{login}/leverage` | `{leverage}` | `{status, from, leverage}` (only when flat) |
| `GET /v1/accounts/{login}/history?from&to&page&limit` | – | `{deals[], orders[] (done pending orders), page, limit, total, totals:{profit, swap, commission}}` |
| `GET /v1/accounts/{login}/ledger?from&to&page&limit` | – | `{items:[{txn, kind, subLedger, amount, currency, reference, reasonCode, note, at}], page, limit, total}` |
| `POST /v1/accounts/{login}/sso` | – | `{token, expiresAt, login}`: one-time, 60 s; the terminal BFF redeems it with `POST /v1/terminal/sso` |

```bash
curl -s -X POST localhost:8090/v1/accounts $H -d '{"userId":42,"type":"demo","group":"standard","leverage":100,"password":"Trade2026x","investorPassword":"Watch2026x"}'
curl -s -X POST localhost:8090/v1/terminal/login $H -d '{"login":50000001,"password":"Trade2026x"}'
curl -s -X POST localhost:8090/v1/terminal/orders $H -H "authorization: Bearer $TOKEN" -d '{"symbol":"BTCUSD","side":"buy","type":"market","volume":0.1}'
```

## Wallet transfers

This API is for the wallet service (D3, D36). Amounts are in USD; a cent account is credited × 100 in USC.

| Method & path | Body | Response |
|---|---|---|
| `POST /v1/ledger/transfers` | `{idempotencyKey, login, amount, direction:"in"|"out", ref?}` | `{status:"completed", txn, amount, currency, balance, login, userId, direction, replayed:false}` |
| `GET /v1/ledger/transfers/{idempotencyKey}` | – | `{txn, kind, login, reference, amount, currency, at, request}` |

- Resending the same key with the same `login`, `amount` and `direction` returns the original result with `replayed: true`, and nothing is booked twice.
- The same key with a different request returns `409 idempotency_conflict`. A concurrent duplicate is caught by the database's unique key.
- `out` is limited to the withdrawable amount: min(balance, free margin − credit − bonus). Anything more returns `422 insufficient_funds`.
- Demo accounts return `422 demo_account`.

## Dealing desk API

This implements the contract at the top of `apps/admin/lib/trading-desk/store.ts`. The Back Office BFF verifies the staff session with the gateway and then forwards the staff identity:

```
X-Kalks-Staff-Id: 12
X-Kalks-Staff-Name: Julia%20Novak      (percent-encoded UTF-8)
X-Kalks-Staff-Role: dealer             (gateway staff role)
```

- **Roles.**
  - Dealing writes: `platform_owner`, `super_admin`, `admin`, `dealer`, `risk_manager`.
  - Balance, credit and bonus: `platform_owner`, `super_admin`, `admin`, `finance`.
  - Groups and tenant policy: `platform_owner`, `super_admin`, `admin`.
  - Reads: any staff role.
- **Reason codes.** Every write body includes `reasonCode` and `note`:
  - `reasonCode` is required.
  - `DLR-99 …` needs a note.
  - A price correction and a close at a given price need `DLR-02 …` and a note.
  - A void needs `DLR-02` or `DLR-06` and a note.
  - A manual market fill price needs a note.
- **Responses.**
  - A successful write returns `{ "data": …, "audit": [AuditEntry…] }`.
  - A failure returns `{ "error": {code, message}, "audit": [...] }`. When the engine refuses an attempt, the refusal is itself audited as `trade.rejected`.

| Method & path | Body | `data` |
|---|---|---|
| `GET /v1/dealing/state` | – | `{positions: DeskPosition[], orders: DeskOrder[], deals: DeskDeal[], symbolControls, accountControls, routingRules, tenant, groups}` |
| `GET /v1/dealing/positions?book=&group=&symbol=&source=&login=` | – | `DeskPosition[]` (plain array) |
| `GET /v1/dealing/orders?group=&symbol=&source=&login=` | – | `DeskOrder[]` |
| `GET /v1/dealing/deals?login=&symbol=&from=&to=&limit=` | – | `DeskDeal[]` (closing deals, newest first) |
| `POST /v1/dealing/trades` | `CreateTradeInput` (`{login, symbol, side, type:"market"|"limit"|"stop"|"stop-limit", volume, price?, stopLimit?, sl?, tp?, book?, comment?, expiry?}`) | `{ticket, kind:"position"|"order", price, book, delayMs}` |
| `PATCH /v1/dealing/positions/{ticket}` | `{sl?, tp?}` (`null` clears) | `null` |
| `POST /v1/dealing/positions/{ticket}/close` | `{volume?, price?, force?, stopOut?}` | `{dealId, profit}`. `volume` < position = partial; `price` = price correction (DLR-02); `force` overrides a symbol halt |
| `POST /v1/dealing/positions/{ticket}/add` | `{volume}` | `null` |
| `POST /v1/dealing/positions/{ticket}/price-correction` | `{openPrice}` | `null` (the position is flagged `priceCorrected`, "price correction" on the statement) |
| `POST /v1/dealing/positions/{ticket}/charges` | `{swap?, commission?}` | `null` (commission differences are booked on the ledger) |
| `POST /v1/dealing/positions/{ticket}/void` | – | `null` (no P&L; the entry commission is refunded) |
| `POST /v1/dealing/deals/{id}/reopen` | – | `{ticket}` (the booked P&L + swap is reversed on the ledger; the volume reopens or merges back) |
| `POST /v1/dealing/book-transfers` | `{tickets[], to:"A"|"B", volume? | pct?}` | `{done[], failed[{ticket,error}], created[]}` (partial = child ticket) |
| `POST /v1/dealing/positions/bulk` | `{tickets[], op:"close"|"modify", force?, slPct?, tpPct?, clear?:"sl"|"tp"|"both"}` | `{done[], failed[], profit?}` |
| `PATCH /v1/dealing/orders/{ticket}` | `OrderPatch` (`{price?, stopLimit?, volume?, sl?, tp?, expiry?}`) | `null` |
| `POST /v1/dealing/orders/cancel` | `{tickets[]}` | `{done[], failed[]}` |
| `POST /v1/dealing/orders/{ticket}/fill` | – | `{ticket}` (the new position) |
| `GET /v1/dealing/controls` | – | `{symbolControls, accountControls, tenant}` |
| `PUT /v1/dealing/controls/symbols/{symbol}` | `{group:"all"|<group code>, mode:"halt"|"close-only"|null}` | `null` |
| `PUT /v1/dealing/controls/accounts/{login}` | `{tradingDisabled?, closeOnly?, maxLot?, execDelayMs? (0–cap), markupPips?}` | `null` |
| `PUT /v1/dealing/controls/tenant` | `{execDelayEnabled?, execDelayCapMs? (≤500), marginCallPct?, stopOutPct?}` | `null` |
| `GET /v1/dealing/routing/rules` | – | `RoutingRule[]` |
| `PUT /v1/dealing/routing/rules` | `{rules: RoutingRule[], summary}` | `null` |
| `PUT /v1/dealing/routing/quick` | `{login | group, book:"A"|"B"|null}` | `null` (adds or removes `RQ-L<login>` / `RQ-G<group>` rules ahead of the rule set) |
| `GET /v1/dealing/audit?staff=&action=&ticket=&login=&from=&to=&limit=&before=` | – | `AuditEntry[]`, newest first; `before` = id cursor |
| `POST /v1/dealing/stream-ticket` | – | `{ticket, expiresIn:30}` |

Shapes follow `apps/admin/lib/trading-desk/types.ts`:

- **DeskPosition.** `ticket, login, clientId (gateway user id), symbol, side, volume, openPrice, sl, tp, swap, commission, openTime, source, platform, group (code), groupName, route, parentTicket, comment, bookSince, bookPrice, bookCarry{A,B}, routeHistory[{at, kind, from, to, volume, price, staff, reason, relatedTicket}], childTickets, priceCorrected`, plus `currentPrice, profit, trailingPoints, currency`.
- **DeskOrder.** `ticket, login, clientId, symbol, type ("Buy Limit" …), volume, price, stopLimit, sl, tp, placed, expiry ("GTC"|"Today"|ISO), group, source, comment, book, triggered, oco`.
- **DeskDeal.** `id, ticket, login, clientId, symbol, side, volume, openPrice, closePrice, openTime, closeTime, profit (price P&L + swap − commission share), priceProfit, swap, commission, book, kind (close|partial|force|stop-out|price-correction), reason, priceCorrection, reversed, staff, reasonCode`.
- **AuditEntry.** `{id:"AUD-000001", at, staff:{id,name,role}, action, tickets[], login, symbol, before, after, reasonCode, note, flags}`.
- **Audit actions.**
  - Contract actions: `position.open|modify|partial_close|close|force_close|stop_out|add_volume|price_correction|adjust_charges|void`, `deal.reopen`, `book.transfer|split`, `order.place|modify|cancel|fill`, `control.symbol|account|tenant`, `routing.rule`, `trade.rejected`.
  - Account ops add `account.balance|credit|status|group|leverage|rejected` and `group.create|update`.

```bash
S='-H x-kalks-staff-id:1 -H x-kalks-staff-name:Julia%20Novak -H x-kalks-staff-role:dealer'
curl -s -X POST localhost:8090/v1/dealing/trades $H $S -d '{"login":"10000001","symbol":"EURUSD","side":"buy","type":"market","volume":1,"reasonCode":"DLR-01 · Client request","note":"client called desk"}'
curl -s -X POST localhost:8090/v1/dealing/book-transfers $H $S -d '{"tickets":["1000006"],"to":"A","volume":0.4,"reasonCode":"DLR-03 · Risk management","note":"hedge"}'
```

## Admin account API

These use the same staff headers, reason rules and response shape as the dealing API.

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/admin/accounts?q=&group=&type=&status=&user_id=&page=&limit=` | – | `{items:[account view], page, limit, total}` (`q` matches login, name or user id) |
| `GET /v1/admin/accounts/{login}` | – | `{account, positions: DeskPosition[], orders: DeskOrder[], lastActivityAt}` |
| `POST /v1/admin/accounts/{login}/balance` | `{type:"deposit"|"withdrawal"|"adjustment"|"credit"|"bonus", amount (signed, account currency), idempotencyKey?, reasonCode, note}` | `{data:{balance, credit, bonus, txn, type, amount}, audit}`. A note is required; withdrawals are limited to the withdrawable amount; credit/bonus cannot go below 0. Used by services (prop, growth); the Back Office uses it for bonus only and books balance and credit through `/adjust` |
| `POST /v1/admin/accounts/{login}/adjust` | `{op:"add"|"deduct"|"credit_in"|"credit_out", category, amount (positive, account currency), idempotencyKey, force?, statementNote?, dryRun?, approvedBy?, requestId?, reasonCode, note}` | Back Office **Balance & credit**, called by the wallet service (which owns the request, four-eyes and the client notice). `{data:{txn, kind, op, category, amount, currency, before, after, login, userId, replayed}, audit}`; `dryRun` returns `{ok, before, after?, limits:{max, maxForced}, marginCall, stopOut, error?}` and books nothing. See [Balance & credit](#balance--credit) |
| `POST /v1/admin/accounts/{login}/status` | `{status:"active"|"disabled"|"close_only"|"read_only"|"expired", reasonCode, note}` | `{data:{status}, audit}` |
| `POST /v1/admin/accounts/{login}/group` | `{group, reasonCode, note}` | `{data:{group}, audit}`. Netting ↔ hedging only while flat; cent ↔ standard and CFD ↔ Options (`product_mismatch`) never |
| `POST /v1/admin/accounts/{login}/leverage` | `{leverage, reasonCode, note}` | `{data:{leverage}, audit}` (allowed with open positions; margin is re-checked at once) |
| `GET /v1/admin/groups` | – | `{groups:[Group + accounts count]}` |
| `POST /v1/admin/groups` | `Group` (all fields, camelCase) + `reasonCode, note` | `{data: Group, audit}` |
| `PUT /v1/admin/groups/{code}` | `Group` + reason | `{data: Group, audit}`. Mode, cent and product are fixed once the group has accounts (`validation` on `mode` / `product`); a body without `product` keeps the group's |
| `GET /v1/admin/ledger/accounts` | – | `{house:[{code, currency, balance}], netByCurrency:[{currency, net}]}` (net is always 0) |

### Balance & credit

Manual adjustments from the Back Office go through `POST /v1/admin/accounts/{login}/adjust` (`funds::staff_adjust`). The wallet service calls it (`services/wallet`, `ops/adjustments.rs`) with the staff identity of the requester and their permission keys in `X-Kalks-Staff-Perms`; the engine checks them again.

| Operation | Permission | Ledger kind (house account) | Limit |
|---|---|---|---|
| `add` | `finance.adjust` | `deposit` (`external`) for category `deposit`, else `adjustment` (`adjustments`) | – |
| `deduct` | `finance.adjust` | `withdrawal` (`external`) for category `withdrawal`, else `adjustment` (`adjustments`) | the free own funds (`withdrawable`) |
| `credit_in` | `finance.credit` | `credit` (`credit_issued`) | – |
| `credit_out` | `finance.credit` | `credit` (`credit_issued`) | the credit held, and the free margin |

- Categories: `deposit`, `withdrawal`, `correction`, `compensation`, `bonus`, `fee`, `chargeback`, `other`. Only `deposit` on add and `withdrawal` on deduct are real money: the reports count ledger kinds `deposit` / `withdrawal` as client deposits, withdrawals and FTDs, and never `adjustment` or `credit`. `deposit` can't be a deduction and `withdrawal` can't add funds.
- Demo accounts book every leg against `house:demo_funding` as `adjustment` / `credit`: never real money.
- **Force** (`force: true`, permission `finance.adjust_force`, Super Admin only) lifts the free-margin limit of a deduction or of taking credit back. Negative balance protection (D16) is always on in Kalks, so even a forced deduction can take at most the balance (`422 negative_balance`: the balance never goes below 0 by a staff action), and credit taken back can never exceed the credit held (`422 insufficient_credit`). After every adjustment the margin level is re-checked, so a forced deduction can raise the margin call or stop out positions at once; the stop-out's realised loss is then covered by NBP as usual.
- Refusals: `insufficient_funds` (above the free funds / free margin), `insufficient_credit`, `negative_balance`, `invalid_category`, `invalid_amount` (≤ 0 or more than 2 decimals), `pamm_account` (fund accounts move money only through invest / redeem). A refusal is audited as `account.rejected`.
- The ledger `note` is the client-visible `statementNote` (a neutral label such as "Balance adjustment" or "Credit" when empty); the internal comment (`note` in the body) goes to the audit only. `reasonCode` is `ADJ-<CAT> · <label>`.
- Idempotent on `idempotencyKey` (stored as `adj:<key>`): the same key and body returns the original booking with `replayed: true`; the same key with another body is `409 idempotency_conflict`.
- Audit: `account.balance` / `account.credit` with before and after (balance, credit, equity, margin, free margin, withdrawable, margin level), the operation, category, statement note, the approver for four-eyes requests (`approvedBy`) and flags `ledger`, `manual_adjustment`, `forced`, `four_eyes`.

The Group object has `code, name, product ("cfd"|"options", default "cfd"), mode, cent, accountTypes ("live"|"demo"|"both"), leverages[], defaultLeverage, marginCallPct, stopOutPct, hedgedMarginPct, minDeposit, swapFree, commissionPerLot, route, spreadGroup, maxAccountsPerUser, demoInitialBalance, demoRefillsPerDay, demoExpiryDays, enabled`.

These groups are seeded: `standard`, `pro`, `pro-netting`, `ecn` (7 USD/lot), `cent`, `vip`, `prop`.

## Copy trading and PAMM

Social trading (D65–D76, D125) lives in `src/social/`. A **master** is a client whose live account was approved as a strategy provider. Followers **copy** the master into a dedicated copy account per subscription (D71). Investors buy units of a master's **PAMM fund**, a pooled trading account valued by NAV per unit (D65). Masters can run both (D75).

### How mirroring works

```
master account shard ── commit (events) ──► event tap (only logins with active followers)
                                              │  unbounded, in commit order, + master equity at commit
                                              ▼
                                      copier task (one, sequential)
                                              │  for every active subscription of that master:
                                              │  plan → op on the follower's shard (single writer)
                                              ▼
                        follower copy account: open / add / partial close / close / SL-TP / pending
```

- **Tap.** After an account's events are committed, the shard hands them to the copier when the login is a watched master. The copier gets them in commit order, with the master's equity at that moment.
- **Ordering.** One copier task processes masters' commits one at a time and awaits every follower op, so each follower sees the master's actions in the master's order.
- **Idempotency and links.** Every mirrored action carries a key derived from the master stream:
  - opens use `clientOrderId = cp<sub>:<master position ticket>`;
  - pending orders use `co<sub>:<master order ticket>`;
  - the (at most one) exit a master event causes stamps `cx<sub>:<master event version>` on the follower's exit deal.

  The follower state remembers these keys (the same duplicate guard as the terminal's `clientOrderId`). The master → follower ticket links are derived from them, so they are part of the follower's own event stream and replay with it: there is no separate link table. A repeated or replayed master event never executes twice.
- **Catch-up.** A cursor per watched login (`copy_cursors`) records the last event version processed. After a restart the copier catches up from the `events` table. During catch-up it still applies closes, SL/TP changes and cancels, but skips opens older than 60 s.
- **Stops win.** A stop sets an in-memory flag before it closes anything. A mirrored action that is already queued on the follower's shard checks the flag first, so it can never reopen a stopped copy.
- **What is mirrored (D73).**

| Master event | Follower action |
|---|---|
| market fill / pending fill (`position_opened` with a deal) | market order, same side, sized volume, same SL/TP, source `copy`, comment `copy #<master ticket>` |
| volume added (netting add, dealer add) | market order for the sized extra volume (netting) or a dealer-style add on the linked position (hedging) |
| partial close | closes the same **fraction** of the linked follower position (rounded down to the lot step; the whole position if the remainder would fall below the minimum lot) |
| full close (client, SL, TP, stop-out, Close By, dealer, void) | closes the linked follower position at market |
| SL / TP / trailing change | same levels on the linked follower position |
| pending placed / modified / cancelled / expired | same order type, prices, SL/TP, expiry and sized volume on the follower; when the master's order fills, a still-pending follower order is replaced by a market fill |
| netting reversal | close + open, like the master |

- **Sizing (D69).** `v = master volume × factor`, then clamped to the follower's max lot and the symbol's max lot, rounded **down** to the lot step. A result below the minimum lot is skipped and logged (`skipped: below min lot`).

| mode | factor |
|---|---|
| `equity` | follower equity ÷ master equity (both in USD, at the moment of the master's trade) |
| `allocation` | fixed allocation (USD) ÷ master equity |
| `multiplier` | `value` (for example 0.5 or 2) |
| `fixed_lot` | every open is `value` lots; adds and partial closes stay proportional |

- **Follower controls (D70).**
  - Symbols in `excludedSymbols` are never copied.
  - `maxLot` caps each copied trade.
  - `equityStop` (USD) and `maxDdPct` (from the subscription's peak equity) are checked every 2 s by the guard. A breach stops the subscription and closes every copied position and order.
  - `stopReason` is `client` (the follower stopped), `equity_stop`, `max_dd` or `admin`.
  - Copied positions and orders cannot be closed, modified or cancelled one by one in the terminal. The terminal API returns `422 copy_managed` with the message "This position is copied from <master>. It closes when the master closes it. To exit, stop copying in the Client Area (Social → My subscriptions)." Manual orders on an actively copying account return `422 copy_account`. Stopping the subscription (`POST …/stop`) closes everything and, when `returnFunds` is set, moves the balance back to the wallet. With `closePositions: false` the client stops copying but keeps the copied positions and orders: mirroring stops at once (the master's later closes no longer reach them), the guard no longer applies, and they are ordinary trades the client manages; `returnFunds` then moves only the withdrawable balance (free margin). The amount is rounded down to the cent (never above the free funds) and, while positions are open, a refusal for insufficient funds is retried once on a fresh reading. Stopping an already stopped subscription (a repeated client call, a stale screen, a Back Office stop) never closes anything and keeps the first stop's reason and time: kept positions stay the client's own; only `returnFunds` still applies.
- **Copy account (D71).** The copy account is a live account owned by the follower in group `copy` (hedging masters) or `copy-netting` (netting masters). Both groups are seeded disabled, so they never appear in the open-account wizard. Money arrives through the wallet (`to-trading`). Every deposit and withdrawal on the account adjusts the high-water mark.
- **Performance fee, copy (D66).**
  - It is settled at the master's fee period end (`daily`, `weekly` or `monthly`, at the server-day rollover) by `settle_copy`: `hwm' = hwm + net deposits since the last settlement`, and `fee = pct × max(0, equity − hwm')`.
  - The fee is debited from the copy account (`perf_fee` ledger txn: `acct:L:balance −fee` · `house:perf_fees:USD +fee`) and `hwm = equity − fee`.
  - The fee is recorded in `social_fees` as `pending`. After admin approval (D76), the wallet pays the master `fee − platform cut` (`kind: copy_fee`, direction `credit`). The platform cut stays in `house:perf_fees`.
  - The fee % is locked on the subscription when it starts. A later change by the master applies to new subscriptions only.

### PAMM (D65–D67, D74)

- **Fund.**
  - A fund is a live account in group `pamm` owned by the master, who trades it in Kalks Trader with the credentials returned at creation. Orders on it are tagged source `pamm`.
  - Wallet ↔ account transfers on a fund login are refused (`422 pamm_account`): money moves only through invest/redeem.
  - `NAV = fund equity ÷ total units`. The first NAV is 1.00: the master's seed capital buys the first units.
- **Requests.**
  - An invest request debits the investor's wallet at once (`kind: pamm_invest`, direction `debit`) and waits for the next rollover.
  - A redeem request waits for the rollover (lock-in: `lockInDays` from the investor's first investment).
  - A pending request can be cancelled; a cancelled invest is refunded to the wallet (`pamm_redeem`, `credit`).
- **Rollover.** At the end of the fund's period (daily / weekly / monthly, at 00:00 server time; weekly = the rollover into Monday; monthly = into the 1st), or on demand from the Back Office:
  1. `nav = equity ÷ units`.
  2. **Fees.** For each investor (the master pays none) with `nav > hwm`: `fee = pct × (nav − hwm) × units`. The fee is taken as units at NAV (`units −= fee ÷ nav`, so NAV is unchanged) and `hwm = nav`. The total fee is debited from the fund account (`perf_fee` txn) and recorded as a pending `social_fees` row per investor.
  3. **Redemptions** at `nav`: `amount = units × nav`, debited from the fund (`transfer_out`, ref `pamm:redeem:<id>`), then credited to the wallet (`pamm_redeem`). A redemption the fund's free margin cannot cover stays pending (`insufficient_free_margin`). The master cannot redeem below `minOwnPct` of units (D68).
  4. **Investments** at `nav`: `units = amount ÷ nav`. The fund is credited (`transfer_in`, ref `pamm:invest:<id>`). The investor's HWM becomes the unit-weighted blend of the old HWM and `nav`. An investment that would push the master's share below `minOwnPct` is rejected and refunded.
  5. A `pamm_rollovers` row records NAV, equity, units, fees, inflows and outflows.
- **Unit ledger.** `pamm_unit_ledger` is append-only: `seed | invest | redeem | fee | stop_loss` with ±units and NAV. The holdings in `pamm_investors` always equal Σ of the ledger (checked by the tests).
- **Protection (D74).**
  - **Investor stop-loss.** If `value ≤ net invested × (1 − stopLossPct)`, the guard redeems that investor at once at the current NAV (fee rules applied; free margin permitting).
  - **Fund max drawdown.** If NAV falls `maxDdPct` below its peak, the fund is frozen. All positions and orders are closed, the account becomes `close_only`, and invests are refused. Redemptions still run at rollover. The Back Office unfreezes it.
- **Money precision.** NAV and units have 8 decimals. Amounts are rounded to 0.01 USD.
- **Consistency.** Rollovers, freezes and stop-loss redemptions hold one PAMM lock. The guard skips a fund while a rollover holds it, so it never reads a NAV between the ledger move and the unit update.
- **IB lots (D64).** Every closed deal on a fund account is split across the holders by units and pushed to the IB service (`POST {IB_URL}/v1/ib/events/lots`, `source: "pamm"`, idempotent on deal + user, best effort with retries). The IB poller also sees the fund account's own deal (owner = the master). Add the `pamm` group to the IB programme's excluded groups so fund volume is not counted twice. Copy trades need no push: they are ordinary deals with source `copy` on the follower's own account.

### Statistics, leaderboard and risk score (D72)

- `social_snapshots (login, day)` holds the end-of-day equity (USD) and the day's net external flow (transfers, deposits, withdrawals, demo funding) for every master account and fund account.
- Snapshots are written:
  - at every server-day rollover;
  - on `POST /v1/social/admin/snapshots`;
  - on approval, which also backfills the account's history from the ledger (end-of-day balance; past floating P&L is not known).
- **Return index** (time-weighted, flows removed): `I₀ = 1`, `I_t = I_{t−1} × (E_t − F_t) ÷ E_{t−1}`. Today's live equity is the last point.
- **Returns.**
  - Period return = `I_now ÷ I_(period start) − 1`.
  - Monthly returns chain the index at month ends.
  - Max drawdown = max of `1 − I_t ÷ max_{s≤t} I_s`.
  - Volatility = stdev of daily index returns × √252.
- **Risk score 1–10.** `raw = 0.6 × min(maxDD ÷ 50%, 1) + 0.4 × min(volatility ÷ 100%, 1)`; `score = clamp(1 + round(9 × raw), 1, 10)`. Maximum drawdown weighs more than day-to-day volatility. A 10% drawdown with 20% volatility scores 3; a 40% drawdown with 80% volatility scores 8; 50% / 100% scores 10.
- **Delayed trade history.** The master profile shows closed deals older than `tradeDelayMinutes` (tenant setting, default 30).

### Social API

Same conventions as the rest of the engine: the internal token, `X-Kalks-Tenant`, camelCase JSON, money in USD, percentages as numbers (`12.5` = 12.5 %). Client routes need `X-Kalks-User-Id` (the signed-in gateway user). The CRM BFF also sends `X-Kalks-Kyc: unverified|pending|verified|rejected` from the gateway profile (D68). Staff routes need the staff headers.

**Shapes**

```jsonc
// MasterView (public card; private fields only on /master/me and admin)
{"id":3,"nickname":"Gold Swing","strategy":"Gold swing","description":"…","program":"copy|pamm|both",
 "perfFeePct":20,"feePeriod":"daily|weekly|monthly","minAllocation":100,
 "status":"pending|approved|rejected|suspended","hidden":false,"frozen":false,"since":"<approvedAt>","ageDays":412,
 "stats":{"return1m":2.1,"return3m":8.4,"return1y":31.0,"returnAll":44.2,"maxDd":7.9,"currentDd":1.2,"volatility":14.1,
          "riskScore":3,"equity":25310.5,"aum":120400.0,"followers":14,"investors":6,"trades":212,"winRate":58.4,"spark":[1,1.01,…]},
 "fund":{"id":2,"name":"…","nav":1.0842,"period":"weekly","perfFeePct":20,"lockInDays":30,"minInvestment":100,"status":"active"} | null,
 // private: "login","kycVerified","reviewNote","reviewedBy","createdAt","userId"
}
// SubscriptionView
{"id":7,"masterId":3,"master":{"id":3,"nickname":"…","strategy":"…","riskScore":3,"frozen":false,"status":"approved"},"login":10000042,
 "status":"active|paused|stopped","stopReason":null,"sizing":{"mode":"equity|allocation|multiplier|fixed_lot","value":1},
 "maxLot":null,"equityStop":null,"maxDdPct":30,"excludedSymbols":["BTCUSD"],"perfFeePct":20,"feePeriod":"weekly",
 "allocation":1000,"netDeposits":1000,"hwm":1000,"peakEquity":1043.2,"feesPaid":0,"feesPending":0,
 "balance":1012.3,"equity":1043.2,"profit":43.2,"returnPct":4.32,"positions":2,"orders":0,
 "createdAt":"…","stoppedAt":null,"nextFeeAt":"…"}
// FundView
{"id":2,"masterId":3,"master":{"id":3,"nickname":"…"},"name":"…","status":"active|frozen|closed","period":"weekly",
 "perfFeePct":20,"lockInDays":30,"minInvestment":100,"maxDdPct":35,"minOwnPct":5,
 "nav":1.0842,"units":10234.5,"equity":11096.3,"aum":9500.1,"investors":6,"masterSharePct":14.2,"navPeak":1.1,"drawdownPct":1.4,
 "returnAll":8.42,"return1m":1.2,"lastRolloverAt":"…","nextRolloverAt":"…","createdAt":"…", "login": 10000050 /* owner/admin only */}
// InvestmentView
{"fundId":2,"fund":FundView,"units":920.4,"nav":1.0842,"value":997.9,"netInvested":950,"pnl":47.9,"pnlPct":5.04,"hwmNav":1.07,
 "stopLossPct":20,"lockedUntil":"…","feesPaid":3.1,"pending":[RequestView]}
// RequestView
{"id":11,"fundId":2,"kind":"invest|redeem","amount":500,"units":null,"all":false,"status":"pending|done|rejected|cancelled",
 "reason":null,"createdAt":"…","executedAt":null,"nav":null,"unitsDelta":null,"amountOut":null,"fee":null}
// FeeView
{"id":5,"source":"copy|pamm","masterId":3,"master":"Gold Swing","subscriptionId":7,"fundId":null,"payerUserId":42,"login":10000042,
 "amount":8.64,"platformCut":1.73,"masterAmount":6.91,"periodStart":"…","periodEnd":"…","hwmBefore":1000,"hwmAfter":1034.56,
 "equity":1043.2,"status":"pending|approved|paid|rejected|failed","reviewedBy":null,"note":null,"createdAt":"…","paidAt":null}
```

**Public and client routes** (`X-Kalks-User-Id`)

| Method & path | Body / query | Response |
|---|---|---|
| `GET /v1/social/leaderboard` | `?period=1m\|3m\|1y\|all&program=all\|copy\|pamm&sort=return\|dd\|aum\|followers\|age&risk=all\|low\|med\|high&minDays=` | `{items: MasterView[], totals:{masters, aum, followers, investors}}`: approved, not hidden |
| `GET /v1/social/masters/{id}` | – | `{master: MasterView, equity:[{day, equity, index}], monthly:[{month:"2026-09", returnPct}], trades:[{id, symbol, side, volume, openPrice, closePrice, openTime, closeTime, profit}], symbols:[{symbol, trades, share}], tradeDelayMinutes, terms:{perfFeePct, feePeriod, hwm:true, minAllocation, platformCutPct}}` |
| `GET /v1/social/master/me` | – | `{master: MasterView+private \| null, settings:{feeMinPct, feeMaxPct, minTrackDays, minOwnCapitalPct, minMasterEquity, platformCutPct, minAllocation}, candidates:[{login, group, equity, ageDays, eligible, checks:[{key:"kyc"\|"live"\|"track"\|"equity"\|"free", ok, label, detail}]}]}` |
| `POST /v1/social/master/apply` | `{login, nickname, strategy, description, program, perfFeePct, feePeriod, minAllocation?}` | `{master}` (status `pending`); 422 `requirements` when a check fails (`checks` in the error) |
| `PATCH /v1/social/master/me` | `{nickname?, strategy?, description?, perfFeePct?, feePeriod?, minAllocation?}` | `{master}` |
| `GET /v1/social/master/dashboard` | – | `{master, followers:[{subscriptionId, since, status, sizing, equity, profit}], funds:[FundView + {investors:[{investorId, units, value, since}], pending}], fees: FeeView[], totals:{followers, aum, feesPending, feesPaid}}` |
| `POST /v1/social/subscriptions` | `{masterId, sizing:{mode, value}, allocation, maxLot?, equityStop?, maxDdPct?, excludedSymbols?[]}` | `{subscription, account, funding:{status:"done"\|"failed", message?}}`. Opens the copy account and pulls `allocation` from the wallet (`to-trading`) |
| `GET /v1/social/subscriptions` | – | `{items: SubscriptionView[]}` |
| `GET /v1/social/subscriptions/{id}` | – | `{subscription, positions[], orders[], log:[{at, action, masterTicket, followerTicket, volume, status, message}], fees: FeeView[]}` |
| `PATCH /v1/social/subscriptions/{id}` | `{sizing?, maxLot?, equityStop?, maxDdPct?, excludedSymbols?, paused?}` (`null` clears a limit) | `{subscription}` |
| `POST /v1/social/subscriptions/{id}/stop` | `{returnFunds?, closePositions?}` (both default `true`) | `{subscription, closed:[tickets], failed:[{ticket, error}], returned: amount \| null, returnError}` |
| `GET /v1/social/funds` | – | `{items: FundView[]}` (active and frozen) |
| `GET /v1/social/funds/{id}` | – | `{fund, master, navHistory:[{at, nav}], rollovers:[{at, nav, invested, redeemed, fees}]}` |
| `POST /v1/social/funds` | master only: `{name, period, perfFeePct, lockInDays, minInvestment, maxDdPct, seed}` | `{fund, credentials:{login, password, investorPassword}}`. `seed` comes from the master's wallet at NAV 1 |
| `PATCH /v1/social/funds/{id}` | owner: `{name?, period?, perfFeePct?, lockInDays?, minInvestment?, maxDdPct?}` | `{fund}` |
| `POST /v1/social/funds/{id}/invest` | `{amount, stopLossPct?}` | `{request}` (the wallet is debited now; units at the next rollover) |
| `POST /v1/social/funds/{id}/redeem` | `{units?} \| {amount?} \| {all:true}` | `{request}` |
| `POST /v1/social/requests/{id}/cancel` | – | `{request}` |
| `GET /v1/social/investments` | – | `{items: InvestmentView[], requests: RequestView[]}` |
| `PATCH /v1/social/investments/{fundId}` | `{stopLossPct: number\|null}` | `{investment}` |
| `GET /v1/social/funds/{id}/statement` | – | `{items:[{at, kind, units, nav, amount}], requests: RequestView[]}` (the caller's own) |

**Back Office routes** (staff headers). Reads are open to every staff role. Writes (`suspend`, `hide`, `emergency`, `freeze`, `rollover`, `snapshots`, `settings`) need `platform_owner`, `super_admin`, `admin` or `risk_manager`. Approvals (masters, fee payouts) need `platform_owner`, `super_admin`, `admin` or `compliance`. Every write needs a `note` and is written to `audit_log` as `social.*`.

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/social/admin/overview` | – | `{masters:{pending, approved, suspended}, subscriptions:{active, stopped}, funds:{active, frozen}, aum, feesPending:{count, amount}, settings}` |
| `GET /v1/social/admin/masters?status=` | – | `{items: MasterView+private[]}` |
| `POST /v1/social/admin/masters/{id}/review` | `{decision:"approve"\|"reject", note}` | `{master}` |
| `POST /v1/social/admin/masters/{id}/status` | `{action:"suspend"\|"reinstate"\|"hide"\|"unhide", note}` | `{master}` |
| `POST /v1/social/admin/masters/{id}/emergency` | `{freeze: bool, closePositions?: bool, note}` | `{master, closed, failed}`: stops mirroring for every follower (D125) |
| `GET /v1/social/admin/subscriptions?masterId=&status=` | – | `{items: SubscriptionView[] + userId}` |
| `POST /v1/social/admin/subscriptions/{id}/stop` | `{note}` | `{subscription}` |
| `GET /v1/social/admin/funds` | – | `{items: FundView[] + {login, pending}}` |
| `POST /v1/social/admin/funds/{id}/freeze` | `{freeze: bool, closePositions?: bool, note}` | `{fund}` |
| `POST /v1/social/admin/funds/{id}/rollover` | `{note}` | `{rollover}`: runs the fund's rollover now |
| `POST /v1/social/admin/rollover` | `{note, force?: bool}` | `{funds, subscriptions, fees}`: everything due (or all with `force`) |
| `POST /v1/social/admin/snapshots` | `{note}` | `{written}` |
| `GET /v1/social/admin/settings` / `PUT` | `{feeMinPct, feeMaxPct, platformCutPct, minTrackDays, minOwnCapitalPct, minMasterEquity, minAllocation, tradeDelayMinutes, note}` | `{settings}` |
| `GET /v1/social/admin/fees?status=` | – | `{items: FeeView[], totals:{pending, approved, paid}}` |
| `POST /v1/social/admin/fees/{id}/review` | `{decision:"approve"\|"reject", note}` | `{fee}`: approve pays the master through the wallet (`copy_fee`) |
| `GET /v1/social/admin/audit?limit=&before=` | – | `AuditEntry[]` (`social.*` actions) |

**House accounts** (driven by the ALGO service, services/algo README "House accounts"). A house master is a platform-owned live account running an automated strategy; `social_masters.is_house` marks it and every master view (and a subscription's `master`) carries `"house": true` so the apps show the "House strategy · Operated by Kalks" label. A hidden house master takes no new followers (`master_status`). Staff headers, `ROLES_SOCIAL_WRITE`, a note on every write, audited as `social.house.*`:

| Method & path | Body | Response |
|---|---|---|
| `POST /v1/social/admin/house` | `{userId, nickname, strategy?, description?, group? ("standard"), capital, perfFeePct? (0), feePeriod? ("monthly"), minAllocation?, key?, note}` | `{master, login, created}`: opens a live account for the house user, books `capital` as ledger kind `house_capital` (`house:house_capital` ↔ balance, never a deposit) and inserts an approved master with `is_house`. Idempotent per `userId` |
| `POST /v1/social/admin/house/{id}/capital` | `{amount (signed), key?, note}` | `{balance, txn}`: top-up or withdrawal (limited to the withdrawable amount) of house capital |
| `POST /v1/social/admin/house/{id}/retire` | `{note, withdrawCapital?}` | stops every follower (copied positions closed), hides the master and closes it (status `rejected`, "Retired house account"); with `withdrawCapital` the free balance goes back to house capital and the account is disabled |

House capital counts as an external flow in the return index (like a deposit), so it never shows as performance.

Errors use the standard shape. Social codes: `not_master`, `master_status`, `requirements`, `fee_out_of_range`, `own_subscription`, `min_allocation`, `wallet_unavailable`, `wallet_rejected`, `fund_frozen`, `min_investment`, `locked`, `insufficient_units`, `request_done`, `copy_managed`, `copy_account`, `pamm_account`.

## MAM (multi-account manager)

A MAM manager is an approved social master (same application, KYC and review as copy / PAMM) who runs a **MAM programme**: one dedicated **MAM master account** and any number of **linked client accounts**. Code: `src/social/mam.rs` (lifecycle, allocation, fees, guard, views), `src/social/allocation.rs` (pure maths), `src/api/mam.rs` (routes), `migrations/20260929190000_mam.sql`.

- **Master account.** Opening a programme opens a live account for the manager in the system group `mam` (hedging, not offered in the open-account wizard), optionally funded from the manager's wallet. The manager trades it in Kalks Trader like any account. Every **opening** trade on it is a **block**: a market fill, a pending order, or volume added. The master account needs its own margin for the block (decision: it is a real, funded account, so the manager has capital at risk and the whole existing execution path is reused; a virtual block account would need a second execution model).
- **Linking (consent).** A client links one of their **own live hedging accounts** in the Client Area. They must send the SHA-256 `termsHash` of the programme's current terms and `accept: true`; the engine refuses a stale hash (`terms_changed`). The link stores the full consent text (terms + account + user + time), the hash, IP and user agent, and the fee terms the client accepted (later programme changes apply to new links only). Netting accounts, demo accounts, system accounts (`copy`, `copy-netting`, `pamm`, `mam`), copy-trading master accounts and accounts already managed cannot be linked. The programme's `minEquity` applies. A manager cannot link to their own programme.
- **Authority.** The manager has trading authority only. No MAM route moves money, and the engine's free-margin rule keeps every withdrawal above the margin of open positions. The client keeps trading their own positions next to the MAM trades.
- **Allocation.** The copier taps the master account's committed events (the same tap as copy trading). For each block it reads every active link's equity and balance, computes the split with `allocation::allocate`, then mirrors the whole master transaction into each linked account's shard (`mirror::mirror` with `MirrorCfg::mam`: key prefixes `mp`/`mo`/`mx`, source `mam`, platform `MAM`). The methods are:

  | Method | Lots for account *i* |
  |---|---|
  | `equity` | block × equity*ᵢ* ÷ Σ equity (accounts with equity ≤ 0 get nothing and are left out of the sum) |
  | `balance` | block × balance*ᵢ* ÷ Σ balance |
  | `multiplier` | block × the link's multiplier (0.01–100, set by the manager per account) |
  | `percent` | block × the link's percent ÷ 100 (0.01–1000, set by the manager per account) |

  Every result is capped at the link's max lot and the symbol's max lot and rounded **down** to the lot step. Below the symbol's minimum lot the account is skipped for that block. For `equity` / `balance` the rounding remainder is reported as `unallocated` and not redistributed (no account ever gets more than its share). Closes, partial closes (same fraction of each account's own position, `math::close_volume`), SL/TP / trailing changes, pending-order price / expiry changes and cancels follow exactly as in copy trading. The volume of an allocated pending order is not changed when the manager changes the master order's volume. A link created after a master event gets nothing from it.
- **Audit.** Each block writes one `mam_allocations` row: action, master ticket, symbol, side, block, method, executed volume, and per account `{linkId, login, equity, balance, value, maxLot, basis, raw, volume, reason, status, ticket, message}`. Every step on every linked account is in `mam_log`. Both tables are append-only.
- **Terminal guard.** On a linked account, a terminal close / modify / cancel of a MAM position or order, a Close By involving one, an OCO with one, and a bulk close while MAM trades are open are refused with 422 `mam_managed` and a readable message naming the programme. Once the link has ended, the leftover MAM trades are ordinary trades again.
- **Risk.** Each link has a max lot per trade and an equity stop. The guard loop (every 2 s) closes the link's MAM trades and stops the link (`stopped`, `equity_stop`) when equity ≤ the stop. Client trades are never touched. **Emergency stop** (Back Office) freezes a programme: no new blocks are allocated (closes on the master still close the MAM trades), optionally closing every MAM trade on every linked account now.
- **Revoke.** The client revokes at any time. The per-link flag is cleared first, so an allocation already queued behind it does nothing. The client chooses to close the MAM trades at market or keep them, and the fees due up to that moment are settled.
- **Fees** (per link, on the terms the client accepted, settled by the scheduler at the period end at 00:00 server time, and on revoke / stop):
  - performance fee = pct × max(0, R − HWM), where R is the cumulative **MAM result** of the account since the link: closed MAM deals (price P&L + swap − commission) plus floating P&L of open MAM positions (USD). Then HWM = max(HWM, R). The fee is not a MAM trade, so it does not lower R: the next period pays only on new gains. The client's own trades never count.
  - management fee = pct a year × equity × elapsed seconds ÷ (365 days), from the last settlement.
  - The total is capped at the account's free margin (withdrawable). The cap is applied to the performance fee first; the HWM still moves to R.
  - Both are debited from the client account (`perf_fee` ledger kind, `house:perf_fees` / `house:mgmt_fees`) and recorded in `social_fees` with `source='mam'`, `link_id`, `perf_amount`, `mgmt_amount`. They go through the same approval as copy / PAMM fees: approve pays the manager's wallet (wallet kind `mam_fee`) minus the platform cut; reject refunds the client's wallet.
- **IB.** The IB service never pays commission on group `mam` (reason `mam_master`, hard-coded in `services/ib/src/calc.rs`). The block traded on the master account is traded again on the linked client accounts, and those deals are what IBs are paid on. Counting both would pay the same volume twice.

**Client Area routes** (`X-Kalks-User-Id` from the BFF):

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/social/mam/managers` | – | `{items: ManagerView[]}` (active, visible programmes; `track` = the master's own track record) |
| `GET /v1/social/mam/managers/{id}` | – | `{manager, terms:{text, hash}, accounts: Candidate[], own}` |
| `GET /v1/social/mam/links` | – | `{items: LinkView[], accounts: Candidate[]}` (the caller's links) |
| `POST /v1/social/mam/links` | `{managerId, login, termsHash, accept:true, maxLot?, equityStop?}` | `{link}`; `terms_changed`, `not_eligible`, `own_programme`, `manager_status` |
| `GET /v1/social/mam/links/{id}` | – | `{link, positions, orders, deals, log, fees, terms}` (MAM trades only; `terms` = the consent text) |
| `PATCH /v1/social/mam/links/{id}` | `{maxLot?, equityStop?}` (`null` clears) | `{link}` |
| `POST /v1/social/mam/links/{id}/revoke` | `{closePositions?}` | `{closed, failed, fee, link}` |
| `GET /v1/social/mam/manager` | – | `{master, settings, manager, totals, links (logins masked), allocations, fees, terms}` |
| `POST /v1/social/mam/manager` | `{name, description?, method, perfFeePct, mgmtFeePct?, feePeriod, minEquity?, seed?}` | `{manager, credentials:{login, password, investorPassword, funding}}` |
| `PATCH /v1/social/mam/manager` | `{name?, description?, method?, perfFeePct?, mgmtFeePct?, feePeriod?, minEquity?}` | `{manager}` (method only while no account is linked) |
| `PATCH /v1/social/mam/manager/links/{id}` | `{value}` | `{link}` (multiplier / percent programmes; audited `social.mam.value`) |
| `GET /v1/social/mam/manager/preview?symbol&volume` | – | `{symbol, block, method, lotStep, lotMin, allocated, unallocated, rows[]}` |
| `GET /v1/social/mam/manager/allocations?limit` | – | `{items: Allocation[]}` |
| `GET /v1/terminal/mam?symbol&volume` (terminal session) | – | `{role:"manager", manager, accounts, equity, preview, recent}` \| `{role:"client", link, manager}` \| `{role:null}` |

**Back Office routes** (staff headers; writes need `ROLES_SOCIAL_WRITE` and a `note`, audited as `social.mam.*`):

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/social/admin/mam/managers` | – | `{items: ManagerView + {login, userId, totals}[], feesPending}` |
| `GET /v1/social/admin/mam/links?managerId&status&limit` | – | `{items: LinkView + {userId, consent:{ip, userAgent, hash, at}}[]}` |
| `GET /v1/social/admin/mam/allocations?managerId&limit` | – | `{items: Allocation[]}` (full logins) |
| `POST /v1/social/admin/mam/managers/{id}/emergency` | `{freeze, closePositions?, note}` | `{manager, result:{closed, failed}}` |
| `POST /v1/social/admin/mam/links/{id}/stop` | `{closePositions?, note}` | `{link, result}` |

MAM fees appear in `GET /v1/social/admin/fees` (`source:"mam"`, `linkId`, `perfAmount`, `mgmtAmount`) and are approved with `POST /v1/social/admin/fees/{id}/review`. Client consent and revocation are also written to `audit_log` (`social.mam.link`, `social.mam.revoke`, actor `user:<id>`).

## Client controls

The Back Office sets per-client restrictions in the gateway (`services/gateway/src/client_controls.rs`); the engine enforces its part (`src/controls.rs`, `src/api/controls.rs`):

- **Cache.** Every active restriction is held in memory (`Shared.restrictions`), reloaded from the gateway every 10 s (`GET /v1/internal/restrictions`, `GATEWAY_URL` + `GATEWAY_INTERNAL_TOKEN`) and refreshed for one client at once by the Back Office BFF (`POST /v1/internal/restrictions/refresh {userId}`). Expiry is checked on every use. If the gateway is unreachable the last known set stays in force.
- **`trading`**: `trade::gate` refuses every client order, modification and close with `trading_disabled` ("Trading is disabled on your account. Contact support."). Dealers still act; SL, TP and stop-out still run.
- **`close_only`**: new exposure is refused with `close_only` ("Your account is in close-only mode: you can close positions but not open new ones."); closes, reductions and SL/TP changes pass.
- **`login`**: terminal sign-in, SSO (the Client Area's Trade button) and every request of an existing session answer 403 `account_suspended`; the client's terminal sessions are revoked when the block arrives and open streams close with `{"type":"ended","reason":"suspended"}`.
- **`social`**: new copy subscriptions, PAMM investments and funds, master applications, MAM links and programmes answer 422 `restricted`.
- **Presence.** Every Kalks Trader stream of the client's own session is reported to the gateway (`POST /v1/internal/presence/trader`, every 15 s and ~1 s after a change). Staff sessions are never reported.
- **Staff sessions ("log in as client").** `POST /v1/admin/accounts/{login}/staff-sso {userId, readOnly, minutes}` (staff headers; the BFF checked `clients.impersonate` / `clients.impersonate_full` with the gateway and audited it) returns a one-time SSO token. The session it opens is read-only unless full access was granted, lasts `minutes` (30), carries `staff` in `/v1/terminal/state` and `GET /v1/terminal/controls`, and full-access trades are recorded with the actor `staff:<id>`.

## CFD and Options accounts

An account trades **one product**, decided by its group (`groups.product`: `cfd` | `options`, migration
`20261022120000_account_products.sql`). The engine reads the group on every operation, so the product is never in an
event and old streams replay unchanged. A client opens a CFD account, an Options account or both.

- **Groups.** Every group from before the split is a CFD group; `options-mm` (the market maker's) is an Options group;
  every broker gets **Options Standard** (`options-standard`, live + demo, hedging, no minimum deposit, enabled) and
  **Options Pro** (`options-pro`, seeded **disabled**: a placeholder minimum deposit of 1 000 USD and lower per-contract
  fees in the options service's `group_settings` — the founder sets the numbers in Back Office › Groups and Options ›
  Pricing, then enables it). Options groups have one leverage (100): options don't use it. A new broker copies the
  platform broker's groups with their products.
- **Gates.** CFD side (`trade::cfd_product_gate`): new CFD exposure on an Options account — market and pending orders,
  a pending order's volume increase, a dealer's trade, volume a dealer adds — is refused with `product_mismatch`
  ("This is an Options account: CFDs trade in a CFD account"). Options side (`options::module_gate_for`): every option
  order on a CFD account — house market / pending / preview, order-book orders and stops, RFQ legs — is refused with
  `product_mismatch` ("This is a CFD account: options trade in an Options account"), after the system-group refusal
  (`options_disabled`) and before the module switches. The Kalks market maker's own account (`is_lp`) is exempt.
  Closing, SL / TP, stop-out, settlement and the liquidator are never refused for the product.
- **Margin.** An Options account's option margin never takes CFD offsets; a CFD account carries no option margin.
- **Group moves.** `funds::change_group` (staff) and the client's account-type change refuse a move to another
  product (`product_mismatch`); the client's list of account types (`group-options`) offers the same product only.
- **Account limit.** Counted per (live / demo, product), see `POST /v1/accounts`.
- **Views.** `product` on every account view (terminal, Client Area, Back Office, streams' account frames) and on
  every group; `GET /v1/admin/accounts?product=cfd|options`.
- **House accounts.** The delta-hedge account is always opened in a CFD group (`options/hedger.rs`); the market
  maker's in `options-mm`, else the broker's first USD Options group (`book/mm.rs`).
- **Copy, PAMM, MAM.** An Options account can't become a copy-trading master (master candidates show a `cfd` check)
  or be linked to a MAM (`not_eligible`): those trade CFDs only.
- **Track 2 hook.** `options::module_gate_for` has a marked place where the gateway's `options` module switch will
  refuse with `module_disabled`.

## Kalks FX Options

European, cash-settled (USD) options on FX, metals and oil, B-book, traded in **Options accounts** (accounts of an Options group; a CFD account refuses them with `product_mismatch`, see [CFD and Options accounts](#cfd-and-options-accounts)). The options service (`services/options`, :8104) owns the reference data (underlyings, holidays, rates, vol surfaces, series, fixings) and publishes a versioned snapshot; the engine is the system of record for the money: premiums, positions, margin, settlement. Both price with `crates/optmath`, with the same conventions, so a fill equals the chain the client saw.

### How it works

- **Snapshot.** `GET {OPTIONS_URL}/v1/internal/options/snapshot` every 2 s with `If-None-Match` (`X-Kalks-Internal: OPTIONS_INTERNAL_TOKEN`). The last good snapshot stays in memory and in `option_snapshot`, so a restart while the options service is down still has it. Once the last successful poll is older than `staleAfterSecs`, options are **close-only** (`stale_prices`).
- **Prices.** Raw mids from a second market-data socket (`group=raw`, kept in the QuoteBook under `raw`). Fills, SL/TP and limit checks are always **priced fresh**; marks (equity, margin, views, streams) are cached for at most 250 ms (the cache key carries the spot, the USD rate and the snapshot version). Bid/ask follow the group's vol spread and minimum USD spread (snapshot `groups`). A position is never dropped from `metrics()`: without a model price it is valued at its intrinsic value (a short at least at its premium), after the cut at the payoff at the fixing.
- **Units.** A position's `symbol` is the series code (`EURUSD-20261009-1.1650-C`), `volume` the contracts, `openPrice` / `currentPrice` / `mark` the premium **per unit of the underlying in its quote currency** (the chain's `bid`/`ask`/`mark`). One contract = `contractSize` units (EURUSD 10 000 EUR). Money (premium, P&L, margin) is in the account currency.
- **Premium in cash.** A buy pays the full premium at once, a sell receives it (`option_premium` ↔ `house:options_premium`). Equity = balance + credit + bonus + CFD P&L + swap + **`optionValue`** (the options at their mark, long +, short −). `profit` includes the options' unrealised P&L (`optionPnl` = value + premium basis). `Position.premium` is the premium cash of the remaining contracts; realised P&L = exit cash + that basis.
- **Commission.** `min(commissionPerContract × contracts, commissionCapPct % × premium)` on every trade (open and close), none on expiry or knock-out.
- **Margin.** Per underlying the optmath 16-scenario grid (`priceScan`, `volScan`, extreme move, one business day) of the option legs. An Options account takes **no CFD offsets** (the scenario gets 0 CFD units). Only an account from before the CFD / Options split holding both would still have same-underlying CFD positions as offsets that **can only reduce** the option margin: `clamp(worst(options + CFD) − worst(CFD), 0, worst(options))`. Long options carry **no margin** (an underlying without a short option has none). Fridays (New York) and weekends add `weekendMarginPct`.
- **Cash only.** Premium debits + commission must fit `min(balance, free margin − credit − bonus)` (`insufficient_cash`); the margin of new short exposure must be covered by equity and by own funds (equity − credit − bonus) (`insufficient_margin`).
- **Orders.** Market, limit on the premium (`limitPremium`: a single leg buys at or below / sells at or above it; a multi-leg order fills when its net debit per combo unit — legs scaled to the smallest leg — is at or below it; an all-sell order when its net credit is at or above it), and underlying triggers (`trigger {symbol, op, price}` on the raw mid; then market, or the limit). Pending orders expire at `tif` (`day` = end of the server day) and at the latest when opening ends before the cut. Every order **opens** new positions (one per leg, never netted). A multi-leg order (1–8 legs, one underlying) fills in **one transaction, all or nothing**; its legs share a `comboId` and `POST …/combos/{comboId}/close` closes them together. SL/TP are on the premium (single-leg).
- **Gates** (opening): never on system-managed groups — prop (`prop*`), copy-trading followers (`copy`, `copy-netting`, `copy-demo`, `copy-*`), PAMM funds (`pamm*`) and MAM block accounts (`mam*`) → `options_disabled`, whatever the snapshot's group settings say; tenant switch for the account kind (`tenants[].enabledLive / enabledDemo`), underlying allow-list and group setting (`options_disabled`); eligibility, live and demo (`not_eligible`: the client has not accepted the options intro yet); Back Office client limits (`blocked` → `not_eligible`, `closeOnly` → `close_only`, `maxContracts` / `maxShortContracts` and the group's `maxContractsPerClient` across the client's accounts → `limit_contracts`); the underlying's session (`market_closed`); the series state: no opens in the last `noOpenMinutes` (15) before the cut, no trading at all from `closeOnlyMinutes` (1) before it (`cutoff`), controls `halt` (`series_halted`, closes too; the engine's own closes still run) and `close_only` (`close_only`); stale snapshot or stale spot (`stale_prices`); and the usual account status, dealer controls, client restrictions and CFD symbol controls of the underlying.
- **Eligibility.** A client may open options once they accepted the options terms: the 1-minute options intro in the Client Area (Options). No quiz and no KYC step for options. The engine asks `GET {GATEWAY_URL}/v1/internal/suitability/{userId}?product=options` → `{eligible, …}` (eligible = the intro was accepted), cached 60 s. A 404 (not deployed) or an error = not eligible. The rule applies to **live and demo** accounts alike; closing is never blocked. The refusal is `not_eligible` with the message "One quick step: read the 1-minute options intro in the Client Area (Options)".
- **Barriers.** Any listed series can be bought / sold with `barrier {kind: UO|DO|UI|DI, level, rebate?}` (rebate per unit, quote currency), priced with Reiner-Rubinstein on the smile vol at the strike. The raw mid is watched on **every** tick (never throttled): a knock-out closes at its rebate (`knock_out`, `option_settlement`), a knock-in becomes its vanilla (`knockedIn`). Each knock happens once and is recorded in `option_knocks`. A barrier already reached, or a knock-out that can never pay, is refused (`invalid_barrier`).
- **Settlement.** Every 15 s (the instance running the rollover) every expiry with open positions whose cut has passed and whose fixing is published (`expiries[].fixing`, status `fixed`; older expiries through `/v1/internal/options/fixings`) settles in every shard (`Cmd::Settle`): the payoff at the fixing is paid or charged (`expiry` deals, key `settle:{SYMBOL:DATE}:{run}:{ticket}`). Settled positions are gone and the key is unique, so a crash in the middle is caught up **exactly once**. Each pass is recorded per tenant in `option_settlement_runs`, and clients are notified (`options.settlement`). Payouts stay out of `withdrawable` for an hour (`settlementHold`), the re-run window. **Venues (docs/OPTIONS-EXCHANGE.md §9):** a house-venue position settles against `house:options_settlement`; an order book position against its expiry's clearing account in USD (cent accounts through `house:fx`), so the clearing account nets to 0 across USD and cent accounts; after the pass (every book position of the expiry settled, its outbox empty) what is left per tenant and account kind — per-position rounding, at most 0.005 USD each — is swept to `house:options_rounding` once (`clrsweep:` key); anything larger stays and logs `ALERT`. **Crosses** (EURJPY, GBPJPY: quote neither USD nor USDxxx) convert at one rate per expiry: the conversion pair's own fixing of the same date (USDJPY, same cut and TWAP window), else the live mid when the pass starts; a position with no rate at all waits for the next pass (never paid a JPY amount as USD).
- **Re-run.** After the options service re-fixed an expiry (Back Office there), `POST /v1/admin/options/settlements/{SYMBOL:DATE}/rerun {reason}` (within 1 h of the first settlement) reverses every settlement deal of the expiry (`settle-rev:`, against the account it was settled against: the clearing account for a book position), reopens the position from the deal's snapshot and settles it again at the new fixing: each balance moves by exactly the difference; the clearing account is swept again for the new run. Audited (`options.settlement_rerun`), clients notified (`options.settlement_rerun`).
- **Void.** `POST /v1/admin/options/trades/{ticket}/void {reasonCode, note}` reverses every cash flow of the trade (premium, proceeds, payouts) and every commission it paid, marks its deals reversed (corrections on the statement) and removes the position if it is open.
- **Stop-out.** An account holding options closes by **units** — a whole strategy (all legs), one option position or one CFD position — the unit that frees the most margin first, until the margin level is above the group's stop-out level or nothing closable frees margin. Nothing is recorded when nothing can be closed (closed market, series past its cut). Accounts without options keep the CFD rule (largest loser first).
- **Interest and throttling.** Accounts with options follow the raw mid of their underlyings (and their triggers' symbols). Pending option orders, premium SL/TP and the margin check run at most every 250 ms per account and underlying (a skipped tick is evaluated on the next 250 ms timer); every 5 s all accounts with options are evaluated again (time decay).
- **House delta hedge.** Every 10 s, per tenant with live options on, the house's delta per underlying (minus the live clients' option delta, in units of the underlying) plus the CFD position of the tenant's **hedge account** is brought back to zero with a CFD market order when it is worth more than `OPTIONS_HEDGE_LIMIT_USD`. The hedge account is a normal live account of the house user `OPTIONS_HEDGE_USER_ID`, opened on first use with house capital (`house_capital`, never a client deposit). Orders are recorded in `option_hedges`.
- **Copy / PAMM / MAM.** Option trades of a master are logged as skipped ("options are not copied") and never mirrored; followers are not alerted for it.

### Options API

Terminal (Kalks Trader session):

| Route | Body → answer |
|---|---|
| `POST /v1/terminal/options/preview` | `{legs: [{series, side: "buy"\|"sell", contracts, barrier?: {kind, level, rebate?}}], type: "market"\|"limit", limitPremium?, sl?, tp?}` → `{ok, reasons: [{code, message}], legs: [{series, side, contracts, price, premium, commission, bid, ask, mark, iv, state, option}], netPremium, commission, marginBefore, marginAfter, freeMarginAfter, cashAfter, maxProfit, maxLoss, breakevens: [], greeks: {delta, gamma, theta, vega}, currency}`. `netPremium` + = the client pays (debit), − = receives. `maxProfit` / `maxLoss` include the commission; `null` = unlimited (barrier legs: path-dependent, not computed). Never changes the account. |
| `POST /v1/terminal/options/orders` | the preview body + `trigger?: {symbol, op: "above"\|"below", price}`, `tif?: "gtc"\|"day"`, **`clientOrderId`** (required) → `{status: "filled", comboId, positions: [...], fills: [{ticket, dealId, series, side, contracts, price, premium, commission}]}` or `{status: "pending", order: {...}}`; a repeated `clientOrderId` answers the same with `duplicate: true`. |
| `POST /v1/terminal/positions/{ticket}/close {volume?}` | the generic close route closes an option position (partial allowed) at the bid (long) / ask (short). |
| `POST /v1/terminal/options/combos/{comboId}/close` | `{status: "closed", comboId, legs: [{ticket, dealId, profit}], profit}` — all legs or none. |
| `GET /v1/terminal/options/settlements?from&to&limit` | `{items: [{ticket, dealId, series, underlying, expiry, side, contracts, fixing, payout, profit, at, run, reversed}]}` (`payout` = the cash booked). |
| `PATCH /v1/terminal/positions/{ticket}`, `PATCH` / `DELETE /v1/terminal/orders/{ticket}` | premium SL/TP of a position; the limit premium, SL/TP, expiry of a pending option order; cancel. |

Position JSON (terminal, account detail, dealing, streams) gains `option: {series, underlying, right, strike, expiry, expiryAt, style: "vanilla"\|"barrier", barrier?: {kind, level, rebate, knockedIn, knockedAt}, contractSize, quoteCurrency}`, `mark` (per unit), `markValue` (signed, account currency), `premium` (basis), `greeks: {delta, gamma, theta, vega}`, `comboId`, `iv`, `underlyingPrice`, `state` (`open` / `close_only` / `halted` / `closed`). CFD positions carry `option: null, mark: null, greeks: null, comboId: null`. Greeks: delta = delta-weighted contracts, gamma = change of that delta per 1 % spot move, vega = USD per vol point, theta = USD per day. Orders gain `option: {legs, limitPremium}`, `trigger`, `comboId`; deals gain `option: {series, underlying, right, strike, expiry, style, cash, usdPerQuote, spot, fixing, run, comboId, commissionCharged}` and `instrument: "option" | "cfd"` (every deal view: client history, `GET /v1/dealing/deals`, streams), so downstream consumers (IB, growth, reports, prop, algo) never read contracts as lots. Account metrics gain `optionValue`, `optionPnl`, `optionMargin`, `settlementHold`.

Error codes: `options_disabled`, `not_eligible`, `market_closed`, `cutoff`, `series_halted`, `close_only`, `limit_contracts`, `insufficient_cash`, `insufficient_margin`, `stale_prices`, plus `unknown_series`, `invalid_volume`, `invalid_barrier`, `invalid_order`, `invalid_price`, `invalid_sl` / `invalid_tp`, `invalid_trigger`, `no_price`, `not_found` (HTTP 422, `not_found` 404).

Back Office (staff headers; `X-Kalks-Staff-Perms` checked when sent):

| Route | Permission (role fallback) | |
|---|---|---|
| `GET /v1/admin/options/book?kind=live\|demo\|all` | `options.read` (dealing roles) | `{underlyings: [{symbol, netDelta, netDeltaUnits, clientDelta, gamma, vega, theta, longContracts, shortContracts, clients, hedgeContracts, hedgeUnits, deltaAfterHedgeUnits}], topClients: [{userId, login, pnl, contracts, todayPnl}], settlements: [runs], snapshot, hedgeAccount}`. Greeks are the **house's** (minus the clients' sum), same units as positions; `pnl` = clients' open option P&L (USD), `todayPnl` = their realised option P&L of the server day. Default `kind=live`. |
| `GET /v1/admin/options/settlements?limit` | `options.read` | settlement runs |
| `POST /v1/admin/options/settlements/{SYMBOL:DATE}/rerun {reason}` | `options.settle` (owner, super admin, admin, risk manager) | `{expiry, run, fixing, accounts, positions, cashChange, failed, audit}`; 409 `not_fixed`, `nothing_to_rerun`, `rerun_window_closed` |
| `POST /v1/admin/options/trades/{ticket}/void {reasonCode, note}` | `options.dealing` (dealing roles) | `{data: {ticket, deals, cashReversed, commissionRefunded, wasOpen}, audit}` |
| `GET /v1/admin/options/status` | dealing / config roles | snapshot version, staleness, switches, spots |

Data (`migrations/20261003000000_options.sql`): `option`, `combo_id` (and `trigger` on orders) columns on `positions` / `orders` / `deals`; `option_knocks`, `option_settlement_runs`, `option_hedge_accounts`, `option_hedges`, `option_snapshot`. No new event types: option terms ride on `Position.option`, `Order.option`, `Deal.option` (`serde(default)`, absent on CFD events, so old streams replay unchanged). **Do not roll the engine back below this version once an option has been traded**: older code cannot read the new deal reasons and ledger kinds and refuses to start.

## Options order book

The exchange of docs/OPTIONS-EXCHANGE.md (decision O49): clients trade options **with each other** in a price-time priority book per series; the Kalks market maker quotes every listed series both sides through the very same entry path under the same rules; strategies trade by combo RFQ with atomic fills; stop-out liquidates book positions on the book first, then to the Kalks backstop. **Dormant by default:** a tenant's live or demo accounts trade on the book only once its `option_book_venues` row exists (the enable / novation flow writes it); without it every option keeps trading at the house price exactly as described above, and the book routes answer 422 `book_disabled`. Once it exists, `POST /v1/terminal/options/orders` refuses orders with a listed (vanilla) leg (422 `book_venue`); barrier-only orders stay Kalks-quoted.

### How it works

- **Units.** Prices are integer ticks of the underlying's `premiumTick` (snapshot field; default pip / 10: EURUSD 0.00001 = $0.10 per contract), quantities integer steps of `contractStep`. Matching never touches decimals; a fill's premium in USD is `px × tick × qty × step × contractSize × usdPerQuote`, rounded **once** to cents.
- **Entry** (`book::entry::submit(hub, login, req)`, used by the terminal, closes, stops, SL / TP and the MM): `hub.exec(login, engine::options_book::enter)` runs the gates in the account shard — module / client / account / session, eligibility, stale prices, the cut-offs (the no-open rule of the last `noOpenMinutes` applies to the **opening** quantity only; liquidity-provider accounts — group `options-mm` or user `OPTIONS_MM_USER_ID` — are exempt until cut − 1 min), 50 working orders per series and 200 per account (MM quotes excepted), contract limits counting working opening quantity, the tick, the price band — then reserves (below) and keeps the working order in `AccountState.book` (in memory, `#[serde(skip)]`; the shard swaps the state in on `tx.book_dirty` even without events). The command then goes to the book actor; the HTTP handler waits up to 1 s for the account's own fills to be booked.
- **Order types.** `limit` (gtc / gtd `expireAt` / ioc / fok), `postOnly` (gtc / gtd; `would_take` if it would cross), `market` (an IOC limit at the band, stamped in the shard), `reduceOnly` (clipped to the net book position at every match, taker and maker), `stop_market` / `stop_limit` (`trigger {source: mark|underlying, op: above|below, price}`, an account `Order`; nothing is reserved until it fires on a raw tick or the 5 s timer, then it is entered like any order with the stop's ticket as id — refused for funds = `order_rejected` notification). Premium SL / TP on a book position becomes a reduce-only market order when the **mark** reaches it. `POST /v1/terminal/positions/{ticket}/close` on a book position is a reduce-only market IOC.
- **Bands.** An aggressive limit must be within `mark × (1 ± limitBandPct %) ± bandMinTicks` (`price_out_of_band`); the passive side may be any price ≥ 1 tick. A market order becomes an IOC at `max(mark × (1 + marketBandPct %), mark + bandMinTicks)` for a buy, mirrored (≥ 1 tick) for a sell. (The doc writes `min(…, mark + k ticks)`; with k = bandMinTicks that would make market orders on any series with a spread above 5 ticks fail, so the band is at least `bandMinTicks` wide. Defaults: market 10 %, limit 50 %, 5 ticks.)
- **Matching** (`book::matching::apply`, pure, no clock, ordered maps only): price-time priority, FIFO per level, trades print at the resting price; self-trade prevention by **user id** (two accounts of one client never match: the incoming remainder is cancelled, the resting order stays); FOK checks fillability on a copy first; IOC never rests; nothing rests crossing the book. Every order leaving the book produces exactly one `Done`. Amend: a size reduction keeps priority, a price change or size increase loses it (an amend that crosses trades like a new order); increases are reserved in the shard first (an extra hold), decreases go to the book first and release when the amend applies.
- **Order margin** (`book::reserve`): a buy reserves `left × limit × contractSize × usdPerQuote × (1 + 2 % if the premium currency is not USD)` + the worst-case fee, checked against free cash (against the balance when it only closes a short); a sell reserves the fee plus, for its **opening** quantity, the standalone scenario margin (optmath grid, no offsets, weekend add-on included); per series the reserve is max(Σ buys, Σ sells). `Metrics.order_reserve` (`orderReserve` in every account view) is subtracted from free margin, free cash and withdrawable (CFD and house option orders see it too); the margin level stays equity / position margin. Released pro rata on fills and fully on cancel, expiry or rejection — a test asserts 0 when nothing works.
- **Book actor** (one per tenant × kind × underlying; demo and live never match): drains up to 256 commands, applies them to a copy-on-write working copy, **group-commits one Postgres transaction** (`book_journal`, `book_orders`, `book_positions`, `book_series`, `book_fills`, `book_outbox`), and only then swaps the state, publishes market data, hands the outbox to the dispatcher and replies. A failed commit (the journal decides when the result is unknown) rolls back and releases the batch's reservations (`Done{rejected}` / `Release`). MM mass quotes are ephemeral: not in `book_orders`, journaled in `book_quote_journal` (monthly partitions) within 1 s or with the next durable batch (so every journaled fill is replayable), gone after a restart.
- **Outbox** (per actor, seq order per login, logins in parallel): each fill = a maker and a taker item, each `hub.exec(login, apply_fill)`; removals = `apply_done`. Retries back off from 100 ms to 30 s; a login with a fill unapplied for more than 2 s gets `settling` on new orders; after 10 failures the row is `failed`, the underlying goes cancel-only (`book_halts`) and an `ALERT` is logged; items are never dropped.
- **`apply_fill`** never refuses (the money was reserved) and is idempotent (`st.book.applied`, rebuilt from the deals' `option.fill` on replay, plus the unique ledger keys). Premium through the expiry's **clearing account**: buyer `acct:B:balance −P / house:options_clearing.{U}.{YYYYMMDD}:USD +P`, seller the reverse (cent accounts: the 4-leg form through `house:fx:USC/USD`); key `fill:{fillId}:{login}:prem`. Fees: `min(|rate| × contracts, commissionCapPct % × premium)` with the group's `takerFeePerContract` / `makerFeePerContract` stamped at entry (absent = `commissionPerContract`; a negative maker rate is a rebate): fee `acct −f / house:commission +f` (`…:fee`), rebate `acct +r / house:options_rebates −r` (`…:rebate`, ledger kind `option_rebate`). Positions: FIFO netting against the account's **book-venue** positions in the series (novated legacy positions included; house-venue positions are never touched by a book fill), the rest opens or adds to one netted book position (VWAP price, summed premium basis). Deals carry `option.fill = {id, role, kind, combo, order}` and `option.rebate`; `commission` = the fee charged. Then the pro-rata release and the margin check.
- **Book positions** carry `venue: "book"` (absent = house). The house hedger and the Back Office house exposure ignore clients' book positions (another account is the counterparty) but count the market maker's (the house's own side); the house-priced close, combo close and void refuse them (`book_venue`, bust the fill instead); stop-out hands them to the liquidator (below).
- **Mark** (docs §6): the actors publish the top of book into `OptionsCtx.top` (`book::md::Top`, versioned); `engine::options::mark_of` clamps the cached model mark inside it with `optmath::mark::clamp_mark` (both sides ≥ `markMinQty` and spread ≤ `markMaxSpreadMult` × model spread → clamp; one side → max(model, bid) / min(model, ask); else the model). The clamp is applied on every read (never cached), so it never lags the book. It drives equity, margin valuation, stop-out, stop triggers, SL / TP and the bands.
- **Expiry and housekeeping** (`book::spawn_scheduler`, with the rollover job): GTD expiry every second, deadman switches, `Expire` at cut − `closeOnlyMinutes` (cancels the expiry's orders, closes its series), `OpenCheck` at session open (cancels resting orders outside the band against the new mark), the throttled feed (250 ms), the nightly **replay audit** (`book::replay_audit`: every journal re-run byte for byte; `ALERT` on a difference). After a settlement pass without failures the books drop the expiry's series and positions (`Expire{purge}`).
- **Kalks market maker** (`book::mm`, docs §4): one house account per (tenant, kind) — user `OPTIONS_MM_USER_ID`, group `options-mm` (else `standard`), house capital `OPTIONS_MM_CAPITAL` (ledger kind `house_capital`; demo: its demo funding) — `option_mm_accounts`. A single-instance loop (every 250 ms per enabled venue) prices every listed vanilla series from the model (one pricing context per expiry; the snapshot's `mm[]` settings, most specific row wins): bid / ask at σ ∓ the tenor spread (0DTE / ≤ 7 d / ≤ 30 d / longer) on ticks, at least `minSpreadTicks` apart, vol skewed by −`skewVol` × (expiry vega / `maxVega`), prices shifted by −`skewTicksPerContract` × inventory, size `baseSize` × moneyness × room to the limits; a side that would push `maxNetDelta`, `maxGamma`, `maxVega` or `maxContractsPerSeries` further is withdrawn; it never crosses other participants (one tick inside the public book). Quotes go through `entry::mass_quote` → `enter` like any order (post-only, ephemeral, the 0 / 0 market-maker fee tier, no per-client contract limit — its own limits apply); a series is requoted only when a side moved by max(1 tick, 25 % of the half-spread) or its size changed; near-the-money series ≤ 7 d every 250 ms, the rest every 2 s on a spot move, everything every 5 s and after a snapshot change or an own fill; at most 400 series per pass (nearest the money first) so its shard stays responsive. It pulls an underlying's quotes when the spot is older than 10 s (the relay has multi-second gaps; docs say 3 s), the snapshot is stale, the market is closed, a series reaches its cut-off, or the desk pauses it (`option_mm_pauses`); a 5 s deadman cancels its quotes if the loop stalls. It reads only the public book (`md::Top`) and its own account (a grep test enforces it); its quotes are firm (nothing asks it before a client trades). Its delta counts in the house hedger.
- **Combo RFQ** (`book::rfq`, docs §5): `{legs[{series, side, ratio}], qty, reduceOnly?}` (1–8 listed vanilla series of one underlying, each once; barrier legs answer `kalks_quoted`), open 30 s. The market maker answers at once with a firm net bid / ask per combo unit (summed theos ± 60 % of the summed half-spreads, at least `minSpreadTicks`), valid `rfqQuoteTtlSecs`, holding its worst side's reserve (`BookState::rfq_holds`, part of `order_reserve`) until the quote is used or lapses; a lapsed quote is replaced on the next read. Accept `{quoteId, side, limitNet}` runs every leg through `enter` (gates, limits, a reservation at its split price), then `Cmd::RfqAccept` fills every leg in ONE journal entry, or none (`quote_expired`, `price_moved`, `reduce_only`, `self_trade`, …). Leg prices: the theos shifted pro rata (ratio × theo) to sum to the net, on ticks, the remainder on the largest leg, never below 0 (`matching::rfq_split`; when no whole-tick split exists the last tick goes the taker's way). Each account gets ONE outbox item holding all its legs (`Fills`, atomic per account); fills are kind `rfq` with the combo id; the tape gets the legs plus one `combo` print; outright levels and the last trade are not touched (positions and volume are).
- **Liquidation** (`book::liquidator`, docs §8): stop-out closes what closes at the house (CFDs, house-priced options); when an order-book unit would free the most margin it flags the transaction and the shard hands the account to the liquidator (single instance, one run per account at a time, 5 s cooldown). A run cancels the account's book orders, then repeatedly takes the book unit that frees the most margin: an option is a reduce-only IOC at mark × (1 ∓ `liqBandPct`) (its fills print `liquidation`), a strategy a reduce-only combo RFQ auto-accepted at the MM's quote (legged when that fails); what is left goes to `Cmd::Backstop`: the market maker takes it at mark ∓ max(`liqFeePct` × mark, 1 tick), outside its quoting limits (`backstop`). It stops above the stop-out level or when nothing more closes (a closed market waits for the next tick). Every step is a row of `option_liquidations`.
- **Bust** (four-eyes, `Cmd::Bust` + `apply_bust`): the fill is traded back on both accounts at its price — premium, fee refunded, rebate taken back, positions — with keys `bust:{fillId}:{login}:prem|fee|rebate`, shown to both clients as a correction; the book's positions move back; `book_fills.busted_at` / `bust` record it. A settled (expired) series cannot be busted.
- **Enable / novation** (`book::enable`, docs §11; four-eyes, forward-only, every step idempotent): (1) the venue goes on in memory (house opens of listed series stop), (2) pending house option orders are cancelled with a notice, (3) the market maker's account and quoting passes until 90 % coverage (a closed market is a warning), (4) per series a `Seed` (clients' steps and the MM's opposite, Σ = 0), the MM's mirror position (deal reason `novation`) with the premium the house took for those positions (`house:options_premium → MM balance`, key `novate:{tenant}:{kind}:{series}:cash`) and the clients' positions moved to `venue = book` (they keep price, premium and P&L), barriers stay house, (5) the `option_book_venues` row. Run again, it completes a crashed enable without doing anything twice.
- **Crash recovery** (`book::recover`, main.rs after the account replay): load every stored book from `book_series` + open `book_orders` (by priority) + `book_positions` and the last seq (a `RestartCancel` is journaled: MM quotes are gone), rebuild the shards' reservations, reconcile actor positions with the accounts' book positions plus the outbox items not applied yet (a mismatch puts that underlying in cancel-only and alerts; CFD trading stays up), then re-dispatch the outbox and resubmit stops that fired but never reached their book.

### Order book API

Terminal (Kalks Trader session; add these to the terminal BFF allow-list). Errors as everywhere: 422 with an engine code — `book_disabled`, `settling`, `price_out_of_band`, `invalid_price` (tick), `invalid_volume`, `reduce_only`, `too_many_orders`, `insufficient_cash`, `insufficient_margin`, `not_eligible`, `cutoff`, `close_only`, `series_halted`, `market_closed`, `stale_prices`, `no_price`, `amend_pending`, `no_liquidity`; 429 `rate_limited` (20 orders / s per login); 503 `book_unavailable`.

| Route | Body → answer |
|---|---|
| `POST /v1/terminal/options/book/orders` | `{series, side, type: limit\|market\|stop_market\|stop_limit, qty, price?, tif: gtc\|ioc\|fok\|gtd, expireAt?, postOnly?, reduceOnly?, trigger?: {source: mark\|underlying, op: above\|below, price}, clientOrderId}` → `{status: working\|filled\|partially_filled\|cancelled\|rejected, order: {id, series, side, qty, filled, left, avgPrice, price, tif, flags[], reserved, createdAt, type}, fills: [{fillId, price, qty, role, fee, rebate, positionTicket}], reason?, message?, settling?}`. `qty` / `filled` / `left` in contracts, prices premium per unit. A repeated `clientOrderId` answers the order as it is now with `duplicate: true`. Stops answer `{status: "working", order: {…, type, trigger}}`. |
| `PATCH /v1/terminal/options/book/orders/{id}` | `{price?, qty?}` (new total qty) → the same answer shape with the order's current state |
| `DELETE /v1/terminal/options/book/orders/{id}` | `{status: "cancelled", order}` (also cancels a book stop that has not fired) |
| `DELETE /v1/terminal/options/book/orders?series=&underlying=` | `{status: "cancelled", cancelled: [ids]}` (orders and book stops) |
| `GET /v1/terminal/options/book/orders?status=open\|history&series=&limit=` | open: `{orders: [working orders + stops], reserved}`; history: `{orders: [{id, series, side, type, qty, filled, left, avgPrice, price, tif, flags, status, reason, createdAt, doneAt, clientOrderId}]}` |
| `GET /v1/terminal/options/book/fills?from&to&limit` | `{fills: [{fillId, series, side, role, price, qty, premiumUsd, fee, rebate, positionTicket, orderId, kind, at}]}` |
| `POST /v1/terminal/options/book/preview` | the order body → `{ok, reasons: [{code, message}], reserve, orderReserveAfter, freeMarginAfter, price, tick, step, side, series, underlying, feeTaker, feeMaker, contractSize, currency, mark, estFilled, estAvgPrice, fee}` (estimated from the depth, stopping at the caller's own orders) |
| `POST /v1/terminal/options/book/deadman` | `{timeoutMs}` (1000–600000, 0 = off; call again as the heartbeat) → `{timeoutMs, expiresAt}`; when it lapses every book order of the account is cancelled |
| `POST /v1/terminal/options/book/mass-quote` | market-maker programme accounts only: `{quotes: [{series, bid?: {price, qty}, ask?: {price, qty}}]}` → `{books: [{underlying, seq, rested: [ids], replaced, rejected: [{id, series, side, code}]}], refused: [{series, side, code, message}]}`; replaces the account's quotes in every listed series (post-only, ephemeral) |
| `POST /v1/terminal/options/rfq` | `{legs: [{series, side, ratio}], qty, reduceOnly?}` → `{rfq: {id, expiresAt, legs, qty, status, underlying, reduceOnly, comboId}, quotes: [{quoteId, responder: "kalks-mm", bid, ask, qty, validUntil}], note}` (ids are strings; nets per unit of the underlying, quote currency; 422 `rfq_underlyings`, `kalks_quoted`, `invalid_volume`) |
| `GET /v1/terminal/options/rfq/{id}` | `{rfq, quotes}` — a lapsed quote is replaced by a new firm one while the request is open; `rfq.status` open / filled / cancelled / expired |
| `POST /v1/terminal/options/rfq/{id}/accept` | `{quoteId, side: buy\|sell, limitNet}` → `{status: "filled", comboId, net, fills: [{fillId, series, side, role: "taker", price, qty, fee, rebate, positionTicket, kind: "rfq", comboId, at}], settling?}`; refused with 422 `quote_expired`, `price_moved`, `rfq_expired`, `reduce_only`, `self_trade`, `series_cancel_only`, … (nothing fills) |
| `DELETE /v1/terminal/options/rfq/{id}` | `{status, rfq}` |
| `POST /v1/terminal/options/combos/{comboId}/close` | on a strategy held on the book: one reduce-only combo RFQ to the market maker, accepted at its firm quote (every leg at once or none) → `{status: "closed", comboId, venue: "book", legs: [{ticket, dealId, profit, fillId, series, price, qty}], profit, net, rfq}` (422 `mixed_venue` when legs are on both venues, `no_liquidity` without a quote); a house strategy closes at the house price as before |
| `POST /v1/terminal/positions/{ticket}/close` | on a book position: `{volume?}` → `{status: filled\|partial, filled, avgPrice, left, orderId, fills}` (reduce-only market IOC; 422 `no_liquidity` when nothing traded) |

Internal market data (`X-Kalks-Internal`; consumed by the options service, `api/book_feed.rs`):

| Route | |
|---|---|
| `GET /v1/internal/options/book/stream?tenant=&kind=` (WebSocket) | first a `depth` and a `top` frame per series, then `{type:"top", tenant, kind, underlying, series, bid, bidQty, ask, askQty, last, lastQty, mark, oi, vol, state, seq}` and `{type:"depth", tenant, kind, underlying, series, bids: [{price, qty, orders}], asks, seq}` (10 levels) for changed series at most every 250 ms, `{type:"trade", tenant, kind, underlying, series, fillId, price, qty, side, tradeKind, combo, at, seq}` at once (`side` = the aggressor), `{type:"hb", t}` every 5 s, `{type:"resync", skipped}` when the consumer lags. `oi` = Σ long contracts, `vol` = contracts traded this server day, `mark` = the tenant's default-group model mark clamped in the book. |
| `GET /v1/internal/options/book/{tenant}/{kind}/snapshot` | `{tenant, kind, enabled, books: [{underlying, seq, series: [{series, underlying, expiry, state, bids, asks, last, lastQty, oi, vol, seq, mark}]}], at}` (`tenant` = slug or id) |
| `GET /v1/internal/options/book/{tenant}/{kind}/trades?series=&underlying=&since=&limit=` | `{tenant, kind, trades: [{fillId, underlying, series, price, qty, side, tradeKind, combo, at, seq}]}` newest first |

Back Office (staff headers; permissions `options.read` / `options.dealing` / `options.settle` with the dealing-role fallback; api/book_admin.rs):

| Route | |
|---|---|
| `GET /v1/admin/options/books?kind=` | `{kind, enabled, enabledAt, replay: {ok, at, mismatches}, books: [{underlying, state, seq, restingOrders, restingContracts, clientOrders, mmCoveragePct, seriesQuoted, seriesTotal, avgSpreadTicks, oi, volume, volumeUsd, outbox: {pending, failed, oldestMs}, clearingUsd, lastTradeAt, openRfqQuotes}], halts: [...]}` |
| `GET /v1/admin/options/books/{series}?kind=` | `options.dealing`; depth WITH OWNERS (`orders: [{id, login, userId, qty, left, at, flags, mm}]` per level), mark, theo, tick, the last 100 fills (`busted`); the view is written to the audit log |
| `POST /v1/admin/options/books/halt` | `{kind, scope: all\|underlying\|expiry\|series, target, mode: halt\|cancel_only, reason}` — halt cancels the resting orders in scope (reservations released) and accepts cancels only; cancel-only keeps them → `{halt}`; `DELETE …/books/halt/{id}?reason=` resumes |
| `GET /v1/admin/options/mm?kind=` | `{status, startedAt, uptimeSecs, uptimePct, latency: {p50Us, p99Us}, coveragePct, quotesLive, lastQuoteAt, account, greeks, pauses, underlyings: [{symbol, status, coveragePct, seriesQuoted, seriesTotal, inventoryContracts, netDelta, gamma, vega, theta, limits, withdrawnSides, lastRequoteAt}]}` |
| `POST /v1/admin/options/mm/pause\|resume` | `{kind, scope: all\|underlying\|expiry, target, reason}` (resuming `all` lifts every pause of the kind) |
| `GET /v1/admin/options/liquidations?kind&from&to&login&limit` | `{items: [{id, at, login, userId, step, unit, series, qty, price, route: book\|rfq\|backstop, marginLevelBefore, marginLevelAfter, freedMarginUsd, status, note, runId}]}` |
| `GET /v1/admin/options/clearing?kind&expiry&u` | `{items: [{account, underlying, expiry, balanceUsd, pendingOutbox, fills, lastFillAt}]}` |
| `GET /v1/admin/options/rfqs?kind=` | `{open: [rfq + login, quotes], recent: [the last 200 of book_rfqs], stats}` |
| `POST /v1/admin/options/fills/{fillId}/bust` | `options.settle`, four-eyes: `{reason}` → `{status: "pending_approval", approval}`; a different staff member with `{reason, approvalId}` → `{status: "busted", fill, reversed: [{login, amountUsd}], approvedBy}` (409 `four_eyes`, `already_busted`) |
| `GET /v1/admin/options/approvals?status&kind` | `{items: [{id, action: fill_bust\|book_enable, target, kind, reason, requestedBy, requestedAt, status, approvedBy, approvedAt}]}` |
| `GET /v1/admin/options/book/enable/plan?kind=` | the dry run: `{enabled, mmCoverage, steps, legacyPendingOrders, novation: {positions, clients, contracts, premiumUsd}, barriersStayHouse, warnings, blockers, pending}` |
| `POST /v1/admin/options/book/enable` | `options.settle`, four-eyes: `{kind, reason}` → pending; `{kind, reason, approvalId}` by another staff member → `{status: "enabled", enabledAt, report}` |

Building blocks: `book::entry::submit` / `entry::mass_quote` / `entry::call(hub, login, key, cmd)`, `Books::actor(hub, key)` → `Handle::call(cmd)` / `read(f)`, the commands `Seed {series, spec, entries: [(login, steps)]}` (Σ = 0), `Backstop {series, spec, login, stp, side, qty, px, counterparty, counter_stp, usdPerQuote}` (reduce-only on the liquidated side, kind `backstop`), `Halt {scope: all|series|expiry, mode: halt|cancel_only|resume}`, `Expire {expiry, purge}`, `RfqQuote {quote}` / `RfqAccept {rfq, quote, login, stp, side, limitNet, orders}`, `Bust {fill, reason}`. `Books::enable_venue`, `book::record_halt`, `book::replay_audit`, `DealReason::Novation`.

Data (`migrations/20261012000000_options_book.sql` and `20261014120000_options_book_mm.sql` — `option_mm_accounts`, `option_mm_pauses`, `option_approvals`, the `options-mm` group — all with RLS): `option_book_venues`, `book_journal` (append-only), `book_quote_journal` (monthly partitions), `book_orders` (open + history), `book_positions`, `book_series` (state, contract units, last trade, day volume), `book_fills` (the tape; every fill holds the full resting-order state), `book_outbox`, `book_rfqs`, `book_halts`, `option_liquidations`, `book_snapshots`.

## Stock corporate actions

Splits and cash dividends of every stock (the 5 core US stocks and the catalogue's US / Hong Kong / Tokyo stocks),
in `src/corporate` (scheduler, EODHD import, provider cross-check), `src/engine/corporate.rs` (per account) and
`src/api/corporate.rs` (Back Office › Trading › Corporate actions).

- **Life cycle.** proposed (Back Office entry, or the EODHD import) → approved → applying → applied (or rejected).
  Four-eyes: a split, or a dividend of 2 % of the price or more (or of unknown size), must be approved by someone
  other than its proposer. An edit after approval needs approval again; so does an upstream EODHD change.
- **When.** At `apply_at` = 00:00 of the ex-date in the exchange's time zone (New York, Hong Kong, Tokyo), while the
  market is closed. The scheduler (with `TRADING_ROLLOVER`) checks every 20 s. Positions and pending orders that
  existed then are adjusted, in every account of every broker, live and demo (copy / PAMM / MAM followers hold their
  own positions, so each is adjusted on its own account; proportional mirroring stays exact).
- **Split** `from`-for-`to`: volume × k, open price ÷ k (exact), SL / TP and order prices ÷ k (rounded to the
  symbol's digits), trailing distances ÷ k. Value and P&L at any price are unchanged; no cash moves.
- **Dividend**: a ledger entry `dividend` (`house:dividends`) per position: longs are credited the gross amount per
  share × lots × contract size less the withholding (default by listing: US 30 %, Tokyo 15.315 %, Hong Kong 0 %;
  editable per action), shorts are debited the gross amount, converted to the account currency. Statements show it
  as "Dividend adjustment"; `GET /v1/accounts/{login}/corporate-actions` lists a client's applied actions.
- **Safety.** Idempotent per account: a `corporate_action` event marks it done in the account's stream (replayed like
  everything else) and dividend entries carry the key `corp:{action}:{ticket}`. A crash part-way resumes on the next
  pass and skips the accounts already done. Between `apply_at` and an account's adjustment the symbol does not trade
  for that account (`corporate_action` rejection) and its margin is not stop-out checked.
- **Provider cross-check.** Hourly, for actions applied in the last 10 days: the Infoway adjustment factors around
  the ex-date (through market-data `GET /v1/admin/adjustment-factors`) must jump by the split factor / the dividend's
  share of the price. A mismatch is logged as an error, audited and flagged in the Back Office; it changes nothing.

### EODHD import

The daily import (06:00 UTC, and "Refresh from EODHD" in the Back Office) reads the upcoming-splits calendar
(`/calendar/splits`, next 120 days, one request) and each stock's dividends from today (`/div/{TICKER}`), 250 ms
apart, and stores **proposed** actions (nothing applies without an approval). Tickers: `AAPL` → `AAPL.US`, `A.US` →
`A.US`, `BRK.B` → `BRK-B.US`, `00700.HK` → `0700.HK`, `7203.JP` → `7203.TSE`.

**The key:** add `EODHD_API_KEY=<key>` to the repo-root `.env.local` on the server (the file every service reads;
`chmod 600`), then restart the engine: `sudo systemctl restart kalks-trading`. It is never logged: request errors are
reported without their URL. Without it the import stays idle and the Back Office shows "EODHD not configured: add
EODHD_API_KEY"; manual entry keeps working.

**Test it:** in the Back Office › Trading › Corporate actions, press "Refresh from EODHD": the toast shows how many
upcoming actions were found and created; the page lists them as Proposed with source EODHD. Or with a staff session:
`POST /api/trading/admin/corporate-actions/import`. The unit tests (`cargo test -p trading corporate`) parse recorded
EODHD responses in `tests/fixtures/eodhd/` and never call EODHD.

## Streams

Browsers connect directly with a one-time ticket, so the internal token never reaches the browser. The flow is:

1. The BFF calls `POST /v1/terminal/stream-ticket` (with the session bearer) or `POST /v1/dealing/stream-ticket` (with the staff headers) and gets back `{ticket, expiresIn:30}`.
2. The browser opens `wss://trade.<domain>/engine/stream?ticket=…` or `wss://admin.<domain>/engine/stream?ticket=…`. Caddy maps these to `/v1/terminal/stream` and `/v1/dealing/stream`. Locally, use `ws://127.0.0.1:8090/v1/terminal/stream?ticket=…`.

The terminal stream sends these frames:

| Frame | When / contents |
|---|---|
| `{type:"snapshot", readOnly, account, positions[], orders[]}` | first frame |
| `{type:"position", op:"upsert", position}` / `{type:"position", op:"remove", ticket}` | open, modify, partial close, swap, trailing move / close |
| `{type:"order", op:"upsert", order}` / `{type:"order", op:"remove", ticket, status, reason}` | placed, modified, triggered / filled, cancelled, expired, rejected |
| `{type:"deal", deal}` | every entry or exit deal |
| `{type:"ledger", txn:{id, kind, amount, at}}` | every balance / credit / bonus change |
| `{type:"account", account}` | after every change |
| `{type:"notification", kind, message, data}` | `fill`, `close`, `sl`, `tp`, `order_triggered`, `order_filled`, `order_cancelled`, `order_rejected`, `order_expired`, `margin_call`, `stop_out`, `nbp`, `swap`, `balance`, `close_by` |
| `{type:"equity", login, balance, credit, bonus, profit, swap, equity, margin, freeMargin, marginLevel, withdrawable, positions:[{ticket, price, profit, swap}]}` | at most every 250 ms while prices move |
| `{type:"book_orders", orders:[…], reserved}` | the account's working options order-book orders (same shape as `GET /v1/terminal/options/book/orders`) after every change to them: entered, filled, amended, cancelled, expired |
| `{type:"hb", t}` | every 5 s |
| `{type:"resync", skipped}` | the client fell behind; reload `GET /v1/terminal/state` |

The dealing stream sends `snapshot` (`positions` as DeskPosition[], `orders` as DeskOrder[]), `position` / `order` deltas in desk shapes, `deal` (DeskDeal), `audit` (AuditEntry), and `{type:"pnl", items:[{ticket, price, profit}]}` every second.

## How the apps integrate

| App | Integration |
|---|---|
| **Kalks Trader (terminal)** | `/login` posts `{login, password}` through its BFF to `POST /v1/terminal/login` and keeps the token in an HttpOnly cookie. The `sso?token=` route calls `POST /v1/terminal/sso`. It loads `GET /v1/terminal/state`, then opens the stream with a ticket. Trading actions map 1:1 to `/v1/terminal/*`. When `readOnly` is set, the UI hides trade actions; the server rejects them anyway. Prices for charts still come from market-data with the account's `groupName`/spread group. |
| **Client Area (CRM)** | Open account wizard: `GET /v1/groups`, `POST /v1/accounts`. Accounts page: `GET /v1/accounts?user_id=`. Portfolio pages: `/history`, `/ledger`. Demo refill, password change (after email OTP), leverage. The Trade button calls `POST /v1/accounts/{login}/sso` and redirects to `trade.<domain>/sso?token=`. Always send the signed-in gateway user id. |
| **Back Office** | A `RestTradingDesk` implementing `TradingDeskApi` maps each method to the dealing routes above, forwarding the staff headers from the gateway session. Hydrate with `GET /v1/dealing/state` + the dealing stream. The audit page uses `GET /v1/dealing/audit`. Accounts pages use `/v1/admin/accounts*`. The group builder uses `/v1/admin/groups`. Spread markups stay in market-data (`/v1/admin/spreads`); a group's `spreadGroup` is the market-data group code. |
| **Wallet service (future)** | `POST /v1/ledger/transfers` with its own idempotency keys (for example the wallet ledger entry id). Treat `409 idempotency_conflict` as a bug and `422 insufficient_funds` as a user error. Look up `GET /v1/ledger/transfers/{key}` when the outcome is unknown after a timeout. |

## Environment

| Variable | Default | |
|---|---|---|
| `TRADING_BIND` | `127.0.0.1:8090` | |
| `TRADING_DATABASE_URL` | `postgres://postgres@127.0.0.1:5433/kalks_trading` | created and migrated on first start |
| `TRADING_INTERNAL_TOKEN` | – | required when `TRADING_ENV=production` |
| `TRADING_SESSION_SECRET` | – | ≥ 32 chars; HMAC key for session / SSO / stream-ticket hashes |
| `TRADING_ENV` | `development` | `production` requires the internal token |
| `MARKET_DATA_WS_URL` | `ws://127.0.0.1:8081/v1/stream` | `?group=` is appended per spread group |
| `INSTRUMENTS_FILE` / `TRADING_SPECS_FILE` | `config/instruments.json` / `config/trading-specs.json` | |
| `TRADING_SHARDS` | `8` | account shards (single-writer tasks) |
| `TRADING_MAX_QUOTE_AGE_SECS` | `300` | 0 disables the stale-price check |
| `TRADING_SESSION_TTL_HOURS` | `12` | terminal sessions |
| `TRADING_LOG_FORMAT` | `json` | `json` (structured) or `pretty` |
| `TRADING_ROLLOVER` | `true` | only one engine instance may run rollovers |
| `WALLET_URL` | `http://127.0.0.1:8095` | wallet service (copy allocations, PAMM invest / redeem, fee payouts) |
| `WALLET_INTERNAL_TOKEN` | – | sent as `X-Kalks-Internal` to the wallet |
| `IB_URL` / `IB_INTERNAL_TOKEN` | `http://127.0.0.1:8096` / – | IB service (PAMM lots allocated to investors) |
| `GATEWAY_URL` / `GATEWAY_INTERNAL_TOKEN` | `http://127.0.0.1:8080` / – | client restrictions, presence, options suitability |
| `OPTIONS_URL` / `OPTIONS_INTERNAL_TOKEN` | – / – | Kalks FX Options service (snapshot, fixings); empty = options off. `deploy.sh` writes both |
| `OPTIONS_HEDGER` | `true` | house delta hedger (with `TRADING_ROLLOVER`; needs `OPTIONS_URL`) |
| `OPTIONS_HEDGE_USER_ID` / `OPTIONS_HEDGE_GROUP` | `0` / `standard` | the house user and the group of the per-tenant hedge accounts |
| `OPTIONS_HEDGE_CAPITAL` | `1000000` | house capital (USD) booked on a new hedge account |
| `OPTIONS_HEDGE_LIMIT_USD` | `250000` | house delta (USD notional) per underlying carried before hedging |
| `OPTIONS_MM_USER_ID` | `0` | the Kalks market-maker user: its accounts are options order book liquidity providers (mass quotes, no-open exemption until cut − 1 min); the `options-mm` group always is |
| `OPTIONS_MM_CAPITAL` | `25000000` | house capital (USD) booked on a new market-maker account (demo: its demo funding); it must cover the order reserve of a full-chain quote (max(bid premium, ask margin) per series) |
| `EODHD_API_KEY` | – | corporate-actions import (EODHD All-in-One); empty = import idle, manual entry only |
| `EODHD_URL` | `https://eodhd.com/api` | |
| `MARKET_DATA_URL` / `MARKET_DATA_ADMIN_TOKEN` | `http://127.0.0.1:8081` / – | the Infoway adjustment-factor cross-check of corporate actions |
| `RUST_LOG` | `info,sqlx=warn` | |

The config is logged at start with every secret and the DB password redacted.

Production runs `deploy/systemd/kalks-trading.service`, which reads the root `.env.local` and binds to 127.0.0.1:8090. `deploy/deploy.sh` builds and restarts it. On first deploy it generates the missing `TRADING_*` secrets on the server and derives `TRADING_DATABASE_URL` from `GATEWAY_DATABASE_URL`, using the database `kalks_trading`.

## Tests

```bash
cargo test -p trading
```

- **Unit tests.** Specs and sessions (the same weekend and US-equity cut-offs as market-data), DST rollover instants, and swap nights including the triple day.
- **Engine tests.**
  - P&L and hedging: hedging P&L and margin, hedged margin %, USDJPY / cross conversion, cent USC maths.
  - Netting: add, reduce and reversal; close-only reduction.
  - Closing: partial close (swap and commission split), Close By, SL/TP, trailing stop.
  - Orders: limit / stop / stop-limit, OCO, Today expiry, weekend `market_closed` while BTC trades, requote, free-margin check.
  - Risk: margin call, stop-out closing the largest loser first, then NBP. Swaps: triple Wednesday, weekend, crypto daily, swap-free, and a position opened after the rollover.
  - Dealing and accounts: book transfer (full + split), reopen / void / price correction, halt / close-only / max-lot / disabled gates, duplicate `clientOrderId`, demo refill cap, leverage only when flat, withdrawable with credit.
  - Balance & credit: the ledger kind per operation and category, balance = Σ postings after every step, deductions refused above the free funds, force past the free margin (margin call, stop-out) but never below a zero balance, credit take-back limited to the credit held and the free margin, demo accounts against `demo_funding`, replay.
- **Property tests (proptest).** Random operation sequences run on hedging, netting and cent accounts. After every step they check:
  - every ledger transaction balances, and Σ of all postings is 0 per currency;
  - balance = Σ postings;
  - no flat account has a negative balance;
  - netting accounts hold one position per symbol;
  - replaying the event log gives exactly the live state.
  They also check that a close done in two parts matches a close done at once, within 0.01.
- **API tests.** Investor sessions are read-only, order body parsing (clients cannot claim the `dealer` source), and PATCH null semantics.
- **Social tests.**
  - `social::math`: sizing modes, proportional adds and partial closes, HWM copy fee with deposits and withdrawals, NAV / units, the rollover plan (fee as units, NAV unchanged, blended HWM, master share, deferred redemptions), return index, drawdown, monthly returns, risk score anchors.
  - `social::tests`: a master and a follower through the real engine: opens with SL/TP, SL change, partial close, full close, pending place / modify / cancel / fill both ways, netting add and reversal, exclusions, pause, fixed-lot and multiplier with max lot, idempotency (the same master event twice executes once), equity stop / drawdown and close-all, and follower replay + ledger.
  - `social::allocation`: equity / balance share, rounding down to the lot step with the remainder reported, the minimum lot, accounts without equity, account and symbol max lot, multiplier and percent, the MAM performance fee above the HWM and the pro-rata management fee.
  - `tests/mam.rs` (PostgreSQL): a MAM programme through the real shards, tap and copier. Stale consent refused; a 0.50 block split 0.30 / 0.20 by equity with the allocation audit row; the terminal guard refuses MAM tickets and a bulk close but allows the client's own trade; partial and full close follow; exact performance fee (20 % of +300 = 60) with the balance after the debit; revoke settles the other link's fee and the next block goes only to the remaining account; the equity stop closes the MAM trade and stops the link; replay of every account and balanced ledger.
  - `tests/social.rs` (PostgreSQL): shards + tap + copier + a mock wallet. It covers mirroring with the right size, partial close, the fee above HWM, stop and return of funds, a stop that keeps the copied position (no guard, no more mirroring; a second stop closes nothing and returns only the free margin), a PAMM seed → invest → rollover → profit → fee + redemption with exact figures, units = Σ unit ledger, and replay of every account from `events`.
- **Options (engine, `src/engine/tests_options.rs`, a `FixedPricer` over a test snapshot).** Premium, commission and realised P&L of buy / partial close / close; a short's premium and scenario margin; premiums from cash only (credit refused); short margin from own funds; long options without margin; an Options account takes no CFD offsets while a pre-split CFD-group account's offsets never raise the option margin (covered call lower); each account trades only its own product (options refused on a CFD account — market, pending, preview — and CFDs on an Options account — market, pending, dealer trade, dealer volume — while pre-split positions still close); group moves stay within the product; combos all or nothing (fill and close); no opens in the last 15 min, closes until 1 min before the cut; gates (switch, eligibility on live and demo, blocked, contract limits incl. other accounts, halt, close-only, stale, weekend, account status); prop / copy / PAMM / MAM groups refused (market, pending, preview) while look-alike group names still trade; option deals flagged `instrument: "option"` with `option` in the dealing feed and the client history, CFD deals `"cfd"`; pending limit and underlying-trigger orders; premium TP; knock-out once at the rebate, knock-in once; settlement idempotent, re-run nets the difference, the one-hour hold; a short ITM settlement; void; USDJPY premium in USD; preview = fill; stop-out by units keeps strategies whole and records nothing when nothing can close; `metrics()` counts every position (fallback valuation); old CFD events unchanged; a proptest over random option sequences (ledger balanced, equity identity, replay).
- **Option settlement through clearing (PostgreSQL, `tests/settle_clearing.rs`, no market data).** A USD short against three cent longs on the real book actor / outbox / shards: each side settles against the expiry's clearing account (cent in the 4-leg form), nothing against `house:options_settlement`; a crash after one account leaves the sweep waiting; the catch-up settles the rest once; the 0.01 USD rounding is swept to `house:options_rounding` exactly once; a re-run reverses both sides through the same accounts and is swept again; ledger balanced, `ledger_accounts` = Σ postings, replay = live.
- **Expiry on live prices (`tests/expiry_e2e.rs`, `--ignored`, real market-data / options service / PostgreSQL).** Phases `open` → `partial` → `settle` → `hold` (`EXPIRY_E2E_PHASE`) around a real TWAP fixing of an ad-hoc expiry the local options service lists a few minutes ahead (`OPTIONS_TEST_EXPIRIES=1`, never in production): house B-book positions (vanilla, a knock-out barrier, a cent short) on live USD and cent accounts, a USD vs cent trade on the demo order book, the engine crashed (`abort`) right after the cut and again in the middle of the settlement pass, restarted through the main.rs boot (replay, ledger check, book recovery), the scheduler catching up exactly once, a second pass and a direct second settle as no-ops, payouts = the fixing payoff, clearing 0, the hold and its release, a re-run at a re-fixed price, replay = live and the book journal replayed.
- **Options (PostgreSQL, `tests/options.rs`, mock options service + gateway).** Eligibility 404 = refused (live and demo), preview vs fill, the position JSON contract, duplicate `clientOrderId`, combo fill and close, all-or-nothing, partial close through the generic route, a raw tick knocking a barrier out once (`option_knocks`, one `knock:` ledger key), void through the Back Office, the options book, the delta hedger, the settlement scheduler after a simulated crash (exactly one `settle:` key per ticket, idempotent, run record, hold), the client settlement list, re-run (window, nets, once, audited), replay + verify_balances + ledger nets. The snapshot poller: ETag / 304, staleness.
- **Options order book.**
  - `src/book/tests.rs` (pure): random command streams (new / cancel / amend, GTC / IOC / FOK, post-only, reduce-only, four accounts of three clients) against a naive O(n²) reference — identical fills, removals and resting orders — and after every step: never crossed, levels = orders, quantity conserved, trades at the resting price and never through the limit, FOK all-or-nothing, IOC never rests, post-only never takes, no self trade, reduce-only never grows |position|, positions = before + fills and net to 0, one `Done` per order that does not rest; the journal replays to byte-identical outputs and state. Focused cases: priority and fill ids, STP across one client's accounts, amend priority, halt / expire / seed / backstop / timer / restart-cancel, RFQ `not_implemented`, and a grep test that matching and the actor never mention the market maker.
  - `src/engine/tests_book.rs` (pure engine harness): reservations and free margin, a trade with premium through clearing (nets to 0), maker rebate / taker fee, positions on the book venue, the same fill twice books once, FIFO netting with realised P&L, cent accounts' 4-leg form, entry gates (tick, band, reduce-only, eligibility, cash, post-only market), market = IOC at the band, a mark-triggered stop firing with its ticket; a proptest of random flows between three accounts (one cent): every transaction balances, clearing 0 after every command, account positions = book positions, Σ long = Σ short, working orders = the book's, reserve 0 once idle, replay identical.
  - `tests/book.rs` (PostgreSQL, real actor / journal / outbox / shards, no market data): a trade (projections, clearing 0, reserve 0 when idle), journal replay = live, **kill point 1** (the batch committed, the actor dies before dispatching) and **kill point 2** (one side applied, the dispatcher dies before marking it) each booked exactly once after a restart with a clean reconcile, a resting order's reservation rebuilt after a restart, the replay audit, a corrupted position caught by the reconcile (cancel-only, `book_halts`), ledger nets and account replay.
  - `tests/book_e2e.rs` (`--ignored`, the **real** market-data :8081, options service :8104 and PostgreSQL; how to run is in the file): dormant until enabled, a resting sell under the model mid clamps the mark, duplicate `clientOrderId`, the internal WebSocket (`depth`, `top`, `trade`), a market buy filled at the resting price with its position ticket, snapshot / tape / OI = positions, fills and preview, a close through the generic close route on the book, clearing 0, reserve 0, journal replay, ledger and account replay; with `BOOK_E2E_FIXING_WAIT_SECS` it also trades the 0DTE expiry and settles it at the real fixing.
  - `tests/book_mm_e2e.rs` (`--ignored`, the **real** market-data, options service and PostgreSQL): enable / novation through the Back Office handlers (plan counts, four-eyes refusal for the same staff member, a pending house order cancelled, the client's house position moved to the book, the MM mirror position and the house premium, reconcile clean, a second enable changes nothing); the market maker quoting the real chain (coverage, bid < model < ask, ephemeral quotes in the quote journal), a client taking its ask at the quoted price (no last look, MM fee 0, taker fee); a call-spread RFQ answered by the MM (worst-side hold), a limit below the ask refused, the accept filling both legs at once (one `fills` outbox item per account, hold released, legs sum to the net, second accept refused); a liquidation (book first inside the band against a resting offer, printed `liquidation`, the rest to the backstop); a four-eyes bust (both sides reversed with `bust:` keys, positions back, reconcile clean); the monitors, halt / resume and MM pause / resume; reserves 0, clearing 0, ledger, account replay and journal replay.
  - `tests/book_load.rs` (`--ignored`, `--release`, real services): the full-chain MM at 4 Hz plus 200 clients (passive limits inside the MM spread, marketable IOCs, cancels, reduce-only closes) for `LOAD_SECS`; targets actor batch p99 < 5 ms and outbox lag p99 < 50 ms; then everything cancelled, reserves 0, clearing 0, ledger and replays identical.
- **CFD / Options account split.** `src/engine/tests_options.rs` (product gates both ways, pre-split positions still close, group moves within the product, no CFD offsets on an Options account, no `product` in any event), `src/engine/tests_book.rs` (`book_orders_need_an_options_account`: a CFD account never enters the book, the market maker's account is exempt, nothing reserved), `tests/lifecycle.rs` (the limit per (kind, product): full CFD groups, an Options account still opens; `product` on the account list; a CFD account's type change offers CFD groups only and refuses an Options group; the product of a group with accounts is locked and an update without `product` keeps it), `tests/tenants.rs` (the migration's products and seeded Options groups, copied to a new broker). The option integration tests open their accounts in `options-standard` (cent: an `options-cent` group the test adds).
- **Integration test** (`tests/replay.rs`). This runs against a throw-away database `kalks_trading_test_<pid>` on the local Postgres; it is skipped when Postgres is unreachable. It runs trades, reversal, pending fills, a partial close, a book split, swaps, a demo refill, credit and manual adjustments (a repeated adjustment key books once) through the shards. It then checks that `replay_all` from the `events` table equals the live state. It also checks the database guarantees: a reused ledger idempotency key is refused, an unbalanced transaction cannot commit, and `events` / `ledger_postings` are append-only.

## Known gaps

- **Social.**
  - The copier is one task that mirrors followers one after another. That is fine for hundreds of followers per master; fan-out per follower shard is the next step.
  - A hedging master's dealer "add volume" is mirrored as a dealer-style add, so that deal carries source `dealer`, not `copy`.
  - Book splits of a master position (A/B transfer of part of a ticket) are not mirrored.
  - A PAMM rollover posts its ledger in the fund's shard, then writes the unit ledger in a second database transaction. If the engine stops between the two, that rollover must be reconciled by hand. The error is logged with the plan.
  - Master KYC comes from the CRM BFF (`X-Kalks-Kyc`), not from a call to the gateway.
- **MAM.**
  - Linked accounts must be hedging accounts. A netting account would net the manager's trades with the client's own on the same symbol.
  - Allocation reads each linked account's equity once per block, one account after another, and executes the accounts one after another (like copy trading). The shares therefore come from a snapshot taken a few milliseconds before execution.
  - The rounding remainder of `equity` / `balance` allocations is not redistributed (reported as `unallocated`).
  - The MAM result counts MAM positions opened after the link started. MAM trades left open after an earlier link to another manager are not part of the new link's result.
  - Changing the volume of a master pending order does not resize the allocated pending orders.

- **Options.**
  - Copy / PAMM / MAM mirroring of option trades, IB per-contract rates and the public API scopes are later milestones (M9): masters' option trades are skipped by followers.
  - Per-client contract limits read the client's other accounts from the in-memory index when the order is placed; two simultaneous orders on two accounts of one client can both pass. Pending orders recheck limits on their own account only.
  - The settlement re-run must be triggered here after the fixing was re-run in the options service (two steps, both audited).
  - Suitability is checked when an order is placed (a pending order is not re-checked when it fills).
  - Short-option minimum margin floors and event-vol bumps (top risk 1) are not implemented; the scenario grid and the weekend add-on are.
  - Statements (services/reports) still label `option_premium` / `option_settlement` as "Other" / adjustments until they learn the new kinds; contests (growth) treat them as trading, as intended.
- **Options order book** (milestones after the core; see "For the next milestones" above).
  - The Kalks market maker (`book/mm.rs`), combo RFQ (`Cmd::RfqQuote` / `RfqAccept` answer `not_implemented`), the liquidator (stop-out skips book positions until it exists), the enable / novation flow and the admin routes (books monitor, halts, MM, liquidations, clearing, fill bust) are not built yet. A venue is switched on by writing `option_book_venues` (`Books::enable_venue`).
  - Settlement still posts book positions against `house:options_settlement` (correct in total: long payouts = short charges); the settlement-venue milestone moves it to the expiry's clearing account and adds the rounding sweep.
  - The no-open exemption for liquidity providers and the 200-orders cap exemption for MM quotes are by group / user (`options-mm`, `OPTIONS_MM_USER_ID`).
  - The reserve of an opening sell includes the weekend margin add-on (the position's margin will).
  - Mass-quote replacement reserves the new quotes before the replaced ones are released (briefly double-reserved).
  - Load targets (actor p99 < 5 ms, outbox lag < 50 ms with a full-chain MM at 4 Hz) are not measured yet.
- **A-book.** A-book routing is recorded and the LP adapter is called, but the only adapter is `NullLp` (not connected), so every trade is executed internally.
- **Routing conditions.** Rules on risk score, hold time, win rate, news window, country or equity never match yet.
- **Swaps.** Swaps are in points only (no percentage or money mode). There is no admin fee for swap-free groups (D19 optional fee). There is no holiday calendar, and sessions do not cover NSE/MCX.
- **Snapshots.** Replay reads the whole event table on start; periodic snapshots are the next step for large books.
- **Tick cost.** A tick clones the account state for every account holding that symbol. That is fine at current scale; a read-only pre-check would avoid the clone.
- **Bonus rules.** Bonus is a separate sub-ledger that counts toward equity. Lot-based bonus release (D29) is not implemented, and credit is not removed on stop-out.
- **Excluded features.** Prop-firm rules, the public API key auth (the source tag is recorded), FIX, dynamic margin schedules (weekend or news), exposure limits, and price-freeze / spike filter hooks (D116) are not included.
- **Demo expiry.** A demo account expires by inactivity (last terminal login). The admin can also set `expired` or `active` directly.
- **Scaling.** Commands are served before ticks. This is correct for a single engine instance, but there is no multi-instance leader election yet: run exactly one engine per database.

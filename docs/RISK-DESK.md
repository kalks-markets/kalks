# Dealer risk desk

Kalks launches 100 % B-book: there is no liquidity provider yet, so the house is the counterparty of every client
trade. The risk desk gives the dealers the standard broker tools for that: server-side exposure, risk limits, client
profiling, A/B routing recommendations and alerts. It lives in the trading engine (`services/trading/src/risk`,
`src/api/risk.rs`) and the Back Office (Trading › Exposure, Risk limits, Client scores, Routing suggestions, the live
monitor on the Dealer desk, Command Center › Live risk).

**Out of scope, by design.** Nothing in the risk desk changes, delays or synthesises prices, quotes or candles. Every
client fills at the same bid / ask as everyone in their group (market-data applies the group spread; the engine adds
only the per-account dealer markup that already existed). The desk observes the book, refuses new orders that break a
limit the broker configured (only in `enforce` mode, with a clear reason code), and tells staff.

## Exposure

`GET /v1/dealing/exposure?kind=live|demo` (default live). Per symbol: long / short / net lots and notional USD (client
side, + = clients net long), per book (A / B), client floating P&L and the house floating P&L on the B-book, positions
and accounts, the limit usage, and the top 5 exposed clients. Totals: gross / net, house floating, realised today
(server day, client view per book and the house B-book result) and the **house daily P&L** = −(B-book realised today)
− (B-book client floating now − floating at the first scan of the server day). After a restart in the middle of the
day the baseline is the first scan after the restart (`baselineAt` says when). Per group: net USD, lots, positions.

The exposure loop (`risk/exposure.rs`) scans the live accounts every second (the order gate's index) and every 5 s
publishes the full view on the dealing stream as `{type:"exposure", exposure}` and evaluates the alerts. CFD
positions only (options have their own desk). Cent accounts are converted to USD.

## Risk limits

Back Office › Trading › Risk limits; `GET|PUT /v1/dealing/risk/limits` (table `risk_limits`, one row per limit, every
save audited as `risk.limits` with before / after). Each limit has a mode: **`alert`** (default; the plan runs
alert-only for the first two weeks) or **`enforce`**. Enforcement is `engine::trade::risk_gate`, called from
`place_order` (market orders and pending placement), when a pending order triggers (`risk::trigger_fill`: a refused
order is rejected with the reason) and for a dealer's fill of a pending order. Closing, reducing, SL / TP and
stop-out never go through it; a dealer's forced trade is not checked.

| Limit | Alert | Enforce (reason code) |
|---|---|---|
| Net exposure cap per symbol (`*` = default row), notional USD, on the **B-book** net (live accounts) | amber from `warnPct`, red at 100 % | a new order that takes the B-book net further past the cap is refused: `exposure_limit`. An order that reduces the net magnitude always passes |
| House daily loss threshold | amber from `warnPct`, red at the threshold | alert only (no automatic action) |
| Per group: max lot per order (an account's own max lot wins) and max open positions per client (across the client's accounts) | "would have been refused" alerts | `group_max_lot`, `group_max_positions` |
| News windows: N minutes before / after an event of impact ≥ `minImpact` on a currency of the symbol | amber alert while a window is open on a held symbol | margin for **new** exposure × `marginMultiplier` (the `no_money` message says so) and / or no new **pending** orders: `news_window` |

The gate reads the last scan plus the exposure of orders it let through since (in-flight), so a burst of orders can't
all slip under the cap. A refused or would-be-refused order is reported to the alert engine (`limit_reject` alerts,
grouped per code and symbol). In alert mode the dealers see exactly what enforcement would have done.

Notes: the margin multiplier applies to new exposure only (existing positions' margin level is not changed, so a news
window never margin-calls anyone); the exposure check uses the book the order would be routed to (A-book trades are
not house exposure); demo accounts are not house exposure.

## News calendar

The engine polls the news service (`services/news`, `GET /v1/calendar`, every 5 minutes; `NEWS_URL`,
`NEWS_INTERNAL_TOKEN`). Without it the windows are empty and `GET /v1/dealing/risk/news` reports `configured: false`.

## Client profitability score

`risk/score.rs`. Per live account with trading in the last 90 days, on a schedule (`scoreIntervalMins`, default 6 h)
and on demand (`POST /v1/dealing/risk/scores/run {login?}`). Every result is kept in `client_scores` (history).

Score 0-100 = how likely the client is to win against the house: 50 + confidence × Σ factor points, confidence =
n / (n + 20) closed trades. Each factor is a "why" entry with the numbers behind it:

- history: win rate, profit factor, average win / average loss, consistency (Sharpe-like ratio of daily P&L), net P&L
  vs deposits, the trend of the last 14 days;
- style: average hold time, share of scalps (< 60 s), trades within 5 min of high-impact news (news calendar), lot
  size, instruments;
- toxic-flow signals (any one → category `toxic`, score ≥ 75):
  - **latency arbitrage**: ≥ 10 trades held < 30 s, ≥ 70 % profitable, ≥ 40 % of all trades, most opened right after a
    one-minute price jump (range ≥ 3 × the median of the 30 bars before, from market-data `/v1/candles`; without
    market-data the pattern alone, stricter, and the reason says the jump check was unavailable);
  - **gap trading**: ≥ half the trades opened just before a session close and closed right after the reopen (or in the
    first minutes after it), mostly profitable;
  - **copied trades / multi-accounting**: ≥ 30 % of the client's own opens matched by an identical open (symbol, side,
    volume within 2 s) on another login (copy / PAMM / MAM / dealer trades excluded on both sides);
  - **bonus abuse**: a bonus followed by a withdrawal within 7 days with < 5 trades, or ≥ 3 opposite trades across the
    client's own accounts within 60 s;
- account facts: deposits / withdrawals / bonus (engine ledger), account age, country (gateway
  `/v1/internal/users/{id}`); the IB source is read from the gateway response when it carries it (it doesn't yet: shown
  as unknown).

Categories: `toxic`, `likely_profitable` (≥ 65), `likely_losing` (≤ 35), `neutral`. A newly toxic account raises an
amber alert. Shown in Trading › Client scores (sortable, filter, click → why), the account drawer and the client
profile.

## Routing

`resolve_route` now matches every condition of the Back Office rule builder: Login, Group, Symbol, Lot size, **Risk
score** (0-100), **Risk category**, **Avg hold time** (`60s`, `5m`), **Win rate (30d)**, **Equity** (USD), **Country**,
**News window** (`±2m` in the value, or the configured window). Without data (no score yet, no calendar) a condition
never matches. A rule's percentage is a **real split**, deterministic by ticket (splitmix64 bucket): the same ticket
always lands in the same book, `pct` % of tickets go to the rule's book.

Tenant setting **routing mode** (`PUT /v1/dealing/risk/settings`, dealing.policy, audited `risk.settings`):

- `recommend` (default at launch): rules only **label** the ticket ("Default route · rule RR-2 recommends A-book") and
  the trade keeps its account / group book. The dealer's quick routes (`RQ-…`) still apply.
- `execute`: rules route new trades.

The suggestion queue (`routing_suggestions`, `risk/routing.rs`, every 30 s) proposes position moves (an open position
whose matching rule names the other book) and account moves (toxic / likely profitable accounts with B-book positions
→ A; likely losing with A-book positions → B), with the reasons. A dealer approves (the existing `transfer_book`, audited
`book.transfer` + `routing.suggestion`; an account move also adds a quick route for its new trades) or rejects (never
proposed again for that position; for an account not within 7 days). Suggestions that no longer apply expire. The LP
adapter (`shard::LpAdapter`, `NullLp`) stays the seam for the A-book: no LP code yet.

## Alerts

`risk_alerts`: one open alert per key (`exposure:SYMBOL`, `daily_loss`, `news:ID`, `reject:CODE:SYMBOL`,
`toxic:LOGIN`, `routing:pending`), red / amber / info. It escalates (re-opening an acknowledged one), refreshes its
numbers at most once a minute, is acknowledged by a dealer (`POST /v1/dealing/risk/alerts/{id}/ack`, audited
`risk.alert_ack`), and resolves when its condition clears (refusal / routing notices after an hour of quiet). Every new,
escalated, acknowledged or resolved alert is pushed on the dealing stream as `{type:"risk_alert", alert}`.

Telegram (`risk/telegram.rs`): red and amber alerts to `TRADING_TELEGRAM_CHAT_ID` with `TRADING_TELEGRAM_BOT_TOKEN`
(no-op when either is unset). Rate limited: one message per alert key and level per 15 minutes, at most 20 a minute.
A message says what, where and how much; a client appears by login only.

## Negative balance protection

Always on (D16): an account left flat with a negative balance is reset to 0 against `house:nbp`. `apply_nbp` runs
after every close path: client / dealer close, Close By, netting reduction, SL / TP (including gaps through the stop),
stop-out (CFD and options), option settlement, copy-trading mirror closes, and now also after options order-book fills
(a liquidation / backstop close). Tests: `src/engine/tests_risk.rs` (`nbp_*`, gaps of 50 to 2 500 pips on hedging,
netting and cent accounts).

## API

| Method & path | Body | Response |
|---|---|---|
| `GET /v1/dealing/exposure?kind=` | – | exposure view (above) |
| `GET /v1/dealing/risk/limits` | – | `{limits, settings, updatedAt, updatedBy, alertOnly}` |
| `PUT /v1/dealing/risk/limits` | `{limits, reasonCode, note}` | `{data:{limits}, audit}` (dealing.write) |
| `PUT /v1/dealing/risk/settings` | `{routingMode?, scoreIntervalMins?, reasonCode, note}` | `{data:{settings}, audit}` (dealing.policy) |
| `GET /v1/dealing/risk/alerts?status=open\|acked\|active\|resolved\|all&limit=` | – | `RiskAlert[]` |
| `POST /v1/dealing/risk/alerts/{id}/ack` | `{note?}` | `{data: RiskAlert, audit}` |
| `GET /v1/dealing/risk/scores?category=&q=&userId=&limit=` | – | `{items, computedAt, running}` |
| `GET /v1/dealing/risk/scores/{login}` | – | `{current, history}` |
| `POST /v1/dealing/risk/scores/run` | `{login?}` | `{data:{queued}, audit:[]}` |
| `GET /v1/dealing/risk/news` | – | `{windows[], configured, lastOkAt}` |
| `GET /v1/dealing/routing/suggestions?status=` | – | `RoutingSuggestion[]` |
| `POST /v1/dealing/routing/suggestions/{id}/approve` | `{reasonCode, note}` | `{data:{suggestion, done, failed}, audit}` |
| `POST /v1/dealing/routing/suggestions/{id}/reject` | `{reasonCode, note}` | `{data:{suggestion}, audit}` |

Permissions follow the dealing permissions: reads dealing.read; limits, alerts, scores runs and suggestions
dealing.write (dealer, risk manager, admins); the routing mode dealing.policy.

## Environment

| Variable | Default | |
|---|---|---|
| `NEWS_URL` / `NEWS_INTERNAL_TOKEN` | `http://127.0.0.1:8103` / – | economic calendar for news windows and the news factor |
| `TRADING_TELEGRAM_BOT_TOKEN` / `TRADING_TELEGRAM_CHAT_ID` | – | risk alerts to Telegram (both needed) |
| `MARKET_DATA_URL` | `http://127.0.0.1:8081` | one-minute candles for the price-jump check |
| `GATEWAY_URL` / `GATEWAY_INTERNAL_TOKEN` | | client country for scores and the Country condition |

## Known gaps

- The exposure index is refreshed every second; orders the gate passed since the last scan are added as in-flight, but
  two engine instances would each have their own (run exactly one engine per database, as for the rest).
- The score's cross-account queries (identical opens, own-account hedges) self-join 90 days of entry deals: fine at
  launch volume, an index or a materialised feed later.
- IB source: the gateway's internal user endpoint does not return the referrer yet.
- Daily loss and news windows are alert / margin / pending-order actions only; there is no automatic close-only.

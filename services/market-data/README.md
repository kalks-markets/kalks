# market-data

Rust service that owns Kalks price data: live Infoway feed → broker-standard candles in our own Postgres → charts and quotes for every app.

## How candles are kept identical to the market

- **Raw price only.** Candles are built from the raw provider price. Group spreads are applied to outgoing quotes only, never to stored bars. This matches every broker's charts.
- **M1–H1** open on exact UTC boundaries.
- **H4, D1, W1, MN** are cut at New York close: server time is GMT+3 while US DST is active, GMT+2 otherwise, and weeks start Sunday 00:00 server time. This is the MT4/MT5 broker standard. They are aggregated from our H1 bars (W1/MN from D1). On a DST switch day the server day (and week, month, 4-hour bar) holding the switch is one bar, an hour shorter or longer: a bar opens at the UTC instant its server-time boundary had (crypto trades through both switches).
- **Reconciler.** Each closed M1/M5/M15/M30/H1 bar is replaced by the provider's final OHLC, 4s and again 40s after it closes (source = 3, final). Late or missed ticks can therefore never leave a candle different from the market. Closed H4/D1/W1/MN buckets are then rebuilt from the corrected bars.
- **Backfill.** Any provider bar that has already closed is stored as final, so restarts and gaps heal themselves.
- **Stocks** only count ticks during the regular session (09:30–16:00 New York), like the exchange candles. Extended-hours prints are ignored.

Verified 2026-09-25 against Infoway klines for all 27 symbols:
- M1: 518/518 identical
- M15: 194/194 identical
- H1: 140/140 identical
- H4/D1/W1: 637/637 equal to the aggregate of their source bars

## Run locally

```bash
# Postgres: any PostgreSQL 16 listening on port 5433, e.g. a dedicated local cluster
pg_ctl -D <datadir> -o "-p 5433" -l <datadir>/pg.log start
# service (reads INFOWAY_* from the repo-root .env.local); creates the DB + runs migrations on first start
cargo run -p market-data            # http://127.0.0.1:8081
cargo test -p market-data
```

Config env vars (with defaults) are listed in `src/config.rs`. The instrument list is in `config/instruments.json`. USDINR is not carried by Infoway.

## Instrument catalogue (1,389 instruments)

`config/instruments.json` holds the 28 hand-maintained **core** instruments (unchanged, first in the file) and
1,361 **catalogue** rows (`"tier": "catalogue"`) generated from the provider:

```bash
ssh kalks-vps 'cd ~/kalks && python3 scripts/infoway-snapshot.py fetch' > config/provider/infoway-snapshot.json
node scripts/gen-catalogue.mjs          # rewrites the catalogue rows (core rows byte-identical) + HKEX calendar
node scripts/gen-catalogue.mjs --check  # CI: files match the snapshot
```

| Class | Catalogue | Rule |
|---|---|---|
| forex | 54 | every provider pair whose profit currency converts to USD with a provider price; one direction per pair |
| metals / energies | 14 / 2 | every provider metal / energy |
| indices | 28 | every index with a price and a USD-convertible currency; cash indices on their exchange hours |
| crypto | 163 | spot USDT pairs as `XXXUSD` (no stablecoins, tokenized stocks or coins without a live price) |
| stocks | 1,100 | US top 800 (NYSE / Nasdaq / NYSE American, ETFs included), Hong Kong top 150, Tokyo top 150, by turnover |

Each row carries name, provider code, digits, a typical spread, session (`fx`, `24x7`, `us_equity`, `hk_equity`,
`jp_equity`), holiday calendar (`config/holidays`, exchange calendars in `config/holidays/exchanges`), base / quote
currency and the trading engine's spec template. Sessions and calendars are shared with the trading engine and ALGO
(crate `markethours`). Raise the stock counts with `--us N --hk N --jp N`.

Why a generated JSON file and not a database table: every service (market-data, trading, ALGO, reports, IB, growth)
reads the same file at startup with no runtime dependency on market-data or the provider; the engine's startup and
replay stay deterministic; a new tradable instrument is a reviewed git diff; 1,389 rows are ~380 KB (one line per
catalogue row). The "refresh job" is the snapshot script, `scripts/stream-check.mjs` (live ticks and spreads,
`config/provider/stream-check.json`) and `gen-catalogue.mjs`.

Live trading: forex, metals, energies, indices and crypto rows carry `"live": true` (stocks follow once corporate
actions are handled); `"live_off": "<reason>"` keeps a row off live trading (restricted currencies MYR / TWD / CNY /
RUB / TRY, no live ticks or zero / very wide spreads at the stream check). Typical spreads (`base_spread`) are the
median raw spreads measured on the stream; crypto and index contract sizes make one lot worth 1,000-10,000 USD.

## Streaming within the plan (demand.rs)

The provider streams a limited number of symbols, so subscriptions follow demand:

- **hold**: the trading engine's open positions and pending orders plus the pairs converting their profit currency
  (`{"op":"hold"}` from its feed). Never dropped for anything else.
- **always**: `MARKET_DATA_ALWAYS_ON` (default: the 28 core instruments).
- **focus**: an open chart (`bars`) or depth ladder (`depth`).
- **watch**: a quote subscription (and the USD conversion pair of its symbol). `"passive": true` subscriptions (the
  trading engine's own feed, relays) take whatever streams without asking for anything.

References are counted per connection and released when it closes; an unwanted symbol stays subscribed for
`MARKET_DATA_IDLE_GRACE_SECS` (300) before it is released. Over the limit, slots go hold > always > focus > watch,
then by number of clients. Plan changes reach the provider at most every 10 s per connection (unsubscribe 11000 /
11001 first, then the full list on 10000 / 10003; the provider allows 60 frames a minute). `GET /v1/streaming` shows
the limits, what streams and the demand that waits.

Symbols that are not streamed still have a price: a delayed snapshot from the provider's daily bar (`"d":1` in
stream frames, `"d":true` in `/v1/quotes`; refreshed every `MARKET_DATA_SNAPSHOT_SECS`, 900, and fetched on demand).
Delayed prices are shown, never traded on: the trading engine ignores them.

History of catalogue instruments is fetched on first view (`/v1/candles` waits up to 8 s for the timeframe asked
for), again when a symbol starts streaming after a pause, and deep history follows in the background. Provider REST
calls share one rate gate (`INFOWAY_RPS`, default 3/s) with priorities: reconciliation, someone waiting, snapshots,
deep history.

| Setting | Default | |
|---|---|---|
| `INFOWAY_MAX_SYMBOLS` | 600 | symbols per provider market connection (`INFOWAY_MAX_SYMBOLS_<MARKET>` per market) |
| `INFOWAY_MAX_SYMBOLS_TOTAL` | 780 | over all connections (plan: 800) |
| `INFOWAY_MARKETS` | `common,crypto,stock` | markets that may stream; Tokyo stocks (`japan`) get REST history and delayed prices until added |
| `INFOWAY_BATCH_CODES` | 100 | codes per batch kline request (provider maximum) |
| `INFOWAY_RPS` | 3 | provider REST requests per second (plan: 10) |
| `MARKET_DATA_ALWAYS_ON` | core | comma-separated symbols, or `none` |
| `MARKET_DATA_IDLE_GRACE_SECS` | 300 | |
| `MARKET_DATA_SNAPSHOT_SECS` | 900 | 0 = no periodic snapshots |
| `MARKET_DATA_UPSTREAM_STALL_SECS` | 20 | relay mode: no frame from the upstream for this long = dead connection, reconnect |

Relay mode (development) subscribes to the upstream passively for every symbol and actively for the symbols wanted
locally; it has no provider REST, so catalogue charts need the upstream to have their history. It pings the
upstream every 5 s (the upstream also sends its own heartbeat), so a connection left half-dead by sleep or a network
change (no close frame, no error) is dropped and reconnected after `MARKET_DATA_UPSTREAM_STALL_SECS` of silence.

## API

| Endpoint | |
|---|---|
| `GET /health` | provider streams, ticking symbols, stale symbols |
| `GET /v1/instruments?class=&tier=&symbols=&q=` | instrument catalogue (core + provider catalogue), optional filters |
| `GET /v1/quotes?symbols=EURUSD,XAUUSD&group=standard` | latest bid/ask with the group's spread markup (`"d":true` = delayed snapshot) |
| `GET /v1/streaming` | plan limits, streamed symbols per market, demand by tier and over the limit |
| `GET /v1/candles?symbol=EURUSD&tf=H1&limit=500&to=<unix>` | ascending bars `{t,o,h,l,c,v}` (t = unix secs); the latest page includes the forming bar |
| `GET /v1/history/status` | stored bars per symbol/timeframe |
| `GET/PUT /v1/admin/spreads` | spread markups per group/symbol (`Authorization: Bearer $MARKET_DATA_ADMIN_TOKEN`); body `{group_code, symbol ("*" = all), markup_points, min_spread_points}`; any well-formed spread group code except `raw` (a new one, e.g. a white-label broker's `<broker>-standard`, is added to `account_groups`); which groups a broker may edit is checked by the Back Office BFF |
| `GET /v1/depth?symbol=XAUUSD&group=standard&levels=10` | depth of market `{src, t, bids, asks}` (levels best first, `[price, lots]`) |
| `WS /v1/stream?group=pro` | send `{"op":"subscribe","symbols":[..],"passive"?:true}` → `{"type":"quote","s","b","a","l","t","d"?:1}`; `{"op":"hold","symbols":[..]}` (trading engine) pins symbols; `{"op":"bars","symbol","tf"}` → `{"type":"bar","s","tf","t","o","h","l","c","v"}`; `{"op":"depth","symbols":[..],"levels"?:10}` → `{"type":"depth","s","src","t","b":[[p,lots]..],"a":[..]}` on every quote change (`unsubscribe` / `unbars` / `undepth` to stop) |

Timeframes: `M1 M5 M15 M30 H1 H4 D1 W1 MN`.

## Provider plan (checked 2026-10-07 with `/package/info`)

**Premium**: 10 REST requests/s, 2 WebSocket connections, 600 symbols per connection, 800 in total, 2 years of
kline history; renews 2026-10-25. The provider lists ~40,000 symbols (FX 85, metals 17, energy 4, indices 51,
futures 259, crypto 422, US stocks 14,805, HK 4,143, A-shares 5,646, Japan 3,926, Korea 2,798, India 5,644,
Taiwan 2,343).

## History depth (core instruments; catalogue instruments on first view)

| Timeframe | Default depth | Setting |
|---|---|---|
| M1 | 14 days | `BACKFILL_DAYS_M1` |
| M5 | 60 days | `BACKFILL_DAYS_M5` |
| M15 | 180 days | `BACKFILL_DAYS_M15` |
| M30 | 1 year | `BACKFILL_DAYS_M30` |
| H1 | 3 years | `BACKFILL_DAYS_H1` |
| D1 | back to 2012 | `BACKFILL_DAILY_FROM` |

- **Phase 1:** the latest 500 bars per timeframe for all symbols (about 3 minutes).
- **Phase 2:** deep history, resumable, in the background.
- **After that:** the live feed and reconciler keep every timeframe growing permanently.

## Depth of market (D97)

Kalks Trader's ladder is served here (`src/depth.rs`), with the account group's spread markup applied like quotes:

- **Feed** (`src: "feed"`): when the provider's depth stream carries several priced levels (and they are under 5 s old), those levels are shown, moved outwards by the group markup.
- **Indicative** (`src: "indicative"`): otherwise (Infoway sends only the top of book for FX and metals) levels step out from the live bid/ask by half the instrument's typical spread; sizes follow a fixed per-asset-class liquidity profile, scaled down when the raw spread is wider than typical. Deterministic: the same quote gives the same ladder. The terminal labels it Indicative.
- Relay mode (local development) asks the upstream for provider depth only (`"src":"feed"`) and never opens a provider connection.


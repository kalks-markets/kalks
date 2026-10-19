# Challenges: contests, missions, XP and leaderboards

Phase 4 of the rewards programme. Everything runs in the growth service (`services/growth`, see its README for every
route and shape); the Back Office, the Client Area, the app and Kalks Trader reach it through their BFFs, and the
public website reads one cached public endpoint.

- [What clients get](#what-clients-get)
- [Setting it up in the Back Office](#setting-it-up-in-the-back-office)
- [Missions](#missions)
- [XP, levels and streaks](#xp-levels-and-streaks)
- [Payouts](#payouts)
- [Leaderboards and privacy](#leaderboards-and-privacy)
- [Public API for the website](#public-api-for-the-website)
- [App (mobile API) and Kalks Trader](#app-mobile-api-and-kalks-trader)
- [Live updates](#live-updates)
- [Decisions to review](#decisions-to-review)
- [Known gaps](#known-gaps)

## What clients get

| Where | What |
|---|---|
| Client Area `/rewards` | The Rewards hub on top: level badge with XP progress, trading and login streaks, missions with progress bars and reset timers, recent rewards (XP, prizes, mission rewards). Below it the contests as before (featured contest, leaderboard, open and past contests, my results) |
| `/rewards/leaderboard` | XP this week / this month / all time, or one running contest; nickname + level badge per row, the client's own row marked and pinned |
| `/rewards/contests/{id}` | Contest detail; the board shows nicknames and level badges |
| `/profile` | Level strip and the leaderboard nickname: rename it, and choose whether it shows on the public website |
| Dashboard | A slim level strip (level, XP progress, trading streak, open missions) linking to the hub |
| Kalks Trader | A rank chip in the screen bar: "#3 of 120" in the acting account's running contest, trades still needed, time left, level; opens the Client Area leaderboard |
| App | The same data through `/api/mobile/growth/*` |

Launch scope (founder): free-entry **live** contests with a broker-funded prize pool and verified KYC; daily / weekly
missions; streaks and levels. Demo contests still work in the code but no template creates one.

## Setting it up in the Back Office

Marketing › Contests & challenges (`/marketing/contests`), tabs:

| Tab | What |
|---|---|
| Contests | The list and the contest drawer (leaderboard, flags, refresh, finalize, pay) as before, plus **Duplicate** (a draft of the same contest for the next window of the same length) and **Save as template** |
| Templates | The wizard's starting points: scoring, schedule, length, minimum trades, asset classes, KYC, minimum equity, prize pool, split and payout, rules, anti-cheat. Seeded per broker: *Weekly live contest – best % return* (next Monday, 7 days, $1,000, 50/30/20, wallet), *Monthly lots race* (next calendar month, $2,500, top-10 ladder, trading credit), *Weekend crypto sprint* (Saturday–Sunday, crypto CFDs only, $500, top 5, wallet) |
| Missions | Create / edit missions; completions, completions this week and participants per mission |
| Levels | XP thresholds, names and badges (Rookie 0 → Legend 12,000 seeded) and how many clients sit on each |
| Payouts | Mission rewards and contest prizes, "Needs approval" first; approve / reject mission rewards (marketing.approve), open the contest for prizes |
| Settings | Auto-pay threshold, auto-finalize, public leaderboards, public-by-default, streak grace, mission anti-abuse, XP awards |

**New contest wizard** (3 steps):
1. **Pick a template** (or a blank contest).
2. **Dates and prizes**: name, start / end (pre-filled from the template's schedule in server time GMT+3), prize pool,
   split preset (winner takes all, 50 / 30 / 20, top 5, top-10 ladder, top 20) with a live calculator of the
   per-rank amounts, cash to wallet or trading credit, and a picker of live account groups (free text when the staff
   member can't read groups). **More options** holds everything else: description, account type, instrument (CFD /
   Kalks FX Options), scoring, minimum trades, maximum entrants, minimum equity / demo balance and group, asset
   classes, KYC, manual prize rows, rules, anti-cheat.
3. **Review and publish** as scheduled or draft; it says which prizes will be paid automatically and which need
   approval.

The calculator splits exactly like the service: each rank gets pool × share ÷ ranks in the band, rounded down to the
cent, and the cents lost go to rank 1, so the rows always add up to the pool.

## Missions

| Kind | Progress | Source | Schedules |
|---|---|---|---|
| `kyc` | 1 once KYC is verified | gateway users feed and the Client Area session (`profiles.kyc_status`) | one-off |
| `first_deposit` | 1 once the first deposit is credited | reports `GET /v1/internal/client-facts` (`profiles.first_deposit_at`, every 60 s) | one-off |
| `deposit` | largest single deposit in the period (USD) | engine ledger of the client's live accounts (`transfer_in` / `deposit`), at most every 2 min per client | any |
| `lots` | lots closed in the period | closed deals from the engine dealing feed | any |
| `trading_days` | server-time days with a closed trade | same | any |
| `hold` | longest hold of a closed trade, hours | same | any |
| `join_contest` | live contests joined in the period | contest entries | any |
| `referral` | referred friends whose first deposit landed in the period | gateway feed `referred_by` + client facts | any |

Periods follow server time (GMT+3): daily from 00:00, weekly from Monday 00:00 (ISO week), one-off from the mission
start. **Anti-abuse:** only live CFD trades count (no demo, no options, no reversed deals or price corrections), at
least `missionMinLots` (0.01) lots and `missionMinHoldSeconds` (60 s) held, per mission overridable plus an
asset-class filter. **Once per period:** progress rows are unique per mission, client and period and completion is
decided under a row lock; XP is booked with ref `mission:<id>:<period>`. A source that is down leaves progress where
it was. Seeded missions (XP only): Verify your identity (100), First deposit (150), Daily trader 0.5 lots (20),
Five-day week (100), Weekly volume 5 lots (150), Join a contest (50).

Rewards: XP always; optionally cash to the wallet or trading credit on the client's first live account. A cash
reward waits for verified KYC by default (`kycRequired`), then follows the payout rules below.

## XP, levels and streaks

- **XP** is an append-only ledger (`xp_ledger`, UPDATE / DELETE rejected by trigger), unique per source and ref:
  missions, joining a live contest (`xpContestJoin`, 25, once per contest), every 7 days of a trading streak
  (`xpStreakWeek`, 50), staff adjustments (audited).
- **Levels** come from the XP total against the broker's `levels` table; changing thresholds never removes XP. A
  level-up sends an in-app notification once per level.
- **Streaks**: *trading* = a day with a qualifying live trade (weekends never break it), *login* = a day the client
  opened the Client Area or the app. Up to `streakGraceDays` (1) missed days are forgiven once a week; a longer gap
  restarts at 1. Late data never rewinds a streak.

## Payouts

`autoPayBelow` (USD, per broker, Settings tab; **0 = off**): when a contest is finalized, prizes **below** the
threshold are paid at once (audited `contest.autopay` by the system) and the rest are set to `approval`; mission
rewards below it are paid when the mission completes, the rest wait under Payouts. Staff with `marketing.approve`
pay the rest (contest: *Pay prizes*; mission: *Approve*). A contest becomes `paid` once nothing waits for approval.
`autoFinalize` (off by default) lets the service finalize contests `autoFinalizeDelayMinutes` after the end when no
anti-cheat flag is open. Every payout is idempotent: wallet keys `growth:prize:<entry>` / `growth:mission:<progress>`,
engine keys with `:c`; retries never pay twice.

## Leaderboards and privacy

Other people only ever see a **nickname**, rank, score and level badge. Nicknames are generated on first use
("SwiftFalcon482", from a hash, never from the name, email or login), unique per broker; clients can rename (3–20
letters, digits, `_ - .`; no `@`, not only digits, reserved words refused). In-app boards list everyone by nickname;
the client's own row is marked. **Public boards** leave out clients who opted out, and, when the broker turns
`nicknamePublicDefault` off, everyone who didn't opt in. Staff views in the Back Office keep real names.

## Public API for the website

Base URL `https://api.kalkstrade.com` (Caddy proxies these two paths to the growth service, GET only). No sign-in,
no API key. CORS `Access-Control-Allow-Origin: *`. Responses carry `Cache-Control: public, max-age=15, s-maxage=15`;
the service caches each board for 15 s. White-label brokers pass `?tenant=<slug>` (default `kalks`).

### `GET /v1/public/leaderboard`

| Query | |
|---|---|
| `contest` | a contest id or slug (running, upcoming or ended; not drafts or cancelled) |
| `period` | `week` (default) · `month` · `all`: XP earned in that period (server time) |
| `limit` | 1–100, default 50 |
| `tenant` | broker slug, default `kalks` |

```json
{
  "board": "contest",
  "contest": { "id": 12, "slug": "weekly-live-contest-best-return-k4f2", "name": "Weekly live contest – best % return",
               "status": "running", "kind": "live", "scoring": "return_pct", "startsAt": "2026-10-25T21:00:00Z",
               "endsAt": "2026-11-01T21:00:00Z", "prizePool": 1000, "entrants": 148,
               "prizes": [{ "rankFrom": 1, "rankTo": 1, "amount": 500, "payout": "wallet" }] },
  "items": [
    { "rank": 1, "nickname": "SwiftFalcon482", "score": 18.42, "level": { "level": 5, "name": "Elite", "badge": "gold" } },
    { "rank": 3, "nickname": "PipHunter", "score": 11.07, "level": { "level": 2, "name": "Trader", "badge": "bronze" } }
  ],
  "updatedAt": "2026-10-27T10:15:02Z"
}
```

XP boards: `{"board": "xp", "period": "week", "since": "...", "scoring": "xp", "items": [{rank, nickname, score (XP), level}], "updatedAt"}`.
`score` follows the contest's `scoring`: `return_pct` (percent), `profit` (USD), `lots`, `contracts`, or XP. Ranks
are the real ranks: a client hidden from public boards leaves a gap. Only ranked entries are listed (disqualified
entries never are).

### `GET /v1/public/contests`

Running, upcoming and recently ended contests (last 30 days), no entrant data:
`{"items": [{id, slug, name, description, status, kind, instrument, scoring, startsAt, endsAt, prizePool, prizes, entrants, kycRequired, minTrades}], "updatedAt"}`.

Errors: `404 not_found` (unknown contest, or the broker switched public leaderboards or the rewards module off),
`422 validation` (bad `period` / `contest`), `400 bad_request` (bad `tenant`). Never returned: names, emails,
logins, user ids, countries, profit amounts of individual traders.

The website repo is separate (`kalks-website`); this API is its only contract.

## App (mobile API) and Kalks Trader

The app calls `/api/mobile/growth/{hub, missions, xp, nickname, leaderboard}` with its bearer session (docs/MOBILE-API.md);
shapes as in the growth README. Kalks Trader's rank chip uses `GET /api/growth/rank` (the terminal BFF resolves the
engine session's owner, refuses investor logins, and keeps only the widget fields), which calls the growth service's
`GET /v1/growth/me/rank?login=`; it hides itself when the broker's rewards module is off.

## Live updates

Mission completions (`rewards.mission`), level-ups (`rewards.level_up`), prizes (`contest.prize`, `reward.paid`,
`rewards.paid`) are in-app notifications through the support service (`POST /v1/notify`, deduplicated); the Client
Area's support stream pushes them to every open tab, and the hub, the leaderboard and the contest pages reload on
them (and after a reconnect). Polling stays as the fallback: contest pages 15 s, leaderboard 30 s, hub 60 s.

## Decisions to review

- `autoPayBelow` defaults to **0** (every payout needs staff, as before) and `autoFinalize` to **off**: a broker turns
  automation on deliberately. Seeded missions carry XP only, never cash.
- Public boards: clients are listed by default (`nicknamePublicDefault: true`, opt-out in the profile). Set it off
  for opt-in.
- In-app contest boards now show nicknames to other clients (no names or countries); staff still see names.
- The trading streak skips weekends; the login streak counts every day the Client Area or the app calls the
  rewards routes.
- "Trading bonus credit" prizes use the engine's `credit` sub-ledger (as contest credit prizes did), not the bonus
  campaign sub-ledger with per-lot release.
- Days follow server time GMT+3 (fixed offset, as the trading day).

## Known gaps

- Score changes on contest boards are polled (15–30 s); the support stream has no silent "data changed" event, so
  only notifications push.
- `deposit` missions read the engine ledger per live account; with the engine down their progress waits.
- `referral` needs the gateway users feed's `referred_by` (present today) and the reports client facts.
- Other languages carry the English strings of the new `challenges` catalog until translated.

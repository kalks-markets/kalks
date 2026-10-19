# growth

Kalks rewards and marketing (D29, D121, D135, D136, D144, O36): loyalty points per lot with tiers and a redemption catalogue, cashback programmes, trading contests (CFD or Kalks FX Options) with live leaderboards and prizes, deposit / credit bonus campaigns with per-lot release, promo codes, targeted banners, and share P&L cards (incl. options share cards); challenges (phase 4, [docs/CHALLENGES.md](../../docs/CHALLENGES.md)): contest templates, missions, XP / levels / streaks, nicknames, leaderboards (incl. a public one for the website) and the auto-pay threshold. It is a Rust service (axum 0.8, sqlx 0.9, PostgreSQL) on `127.0.0.1:8101`, database `kalks_growth`.

- [Run locally](#run-locally)
- [How it works](#how-it-works)
- [Rules](#rules)
- [API](#api)
- [Permissions](#permissions)
- [How the apps integrate](#how-the-apps-integrate)
- [Environment](#environment)
- [Tests](#tests)
- [Known gaps](#known-gaps)

## Run locally

PostgreSQL on `127.0.0.1:5433`, the gateway on `:8080`, the trading engine on `:8090` and the wallet on `:8095`. Without the wallet, cashback / prize / points payouts stay `pending` and are retried.

```bash
cargo build -p growth
(cd services/growth && GROWTH_LOG_FORMAT=pretty nohup ../../target/debug/growth > ~/.kalks-local/growth.log 2>&1 &)
curl -s localhost:8101/health
cargo test -p growth
```

On first start it creates `kalks_growth`, runs `migrations/`, and seeds tenant `kalks` with default settings, tiers (Bronze → Platinum), earning rules per asset class and a starter catalogue. It reads `GROWTH_*`, `GATEWAY_INTERNAL_TOKEN`, `TRADING_INTERNAL_TOKEN` and `WALLET_INTERNAL_TOKEN` from the repo-root `.env.local`.

## How it works

```
 gateway /v1/internal/referrals/users ──(every 30 s)──► profiles (name, country, KYC, sign-up date, referral code)
 engine  /v1/dealing/deals ──(close-time cursor + 2 min overlap, every 5 s)──► deals ─┬─► loyalty points (earn)
                                                                                     ├─► cashback accruals
                                                                                     ├─► bonus release per lot
                                                                                     └─► contest trades
 engine  /v1/accounts/{login}/ledger ──► bonus deposit trigger, removal on withdrawal, contest anti-cheat
 engine  POST /v1/admin/accounts/{login}/balance (bonus | credit | adjustment, reason code GRW-…) ◄── bonus grant / release / removal, credit prizes
 engine  POST /v1/accounts (demo) ◄── demo contest accounts
 wallet  POST /v1/wallets/transfers (refund = cashback, adjustment = prizes and points cashback) ◄── payouts
```

| Loop | Period | What it does |
|---|---|---|
| profiles | `GROWTH_SYNC_SECS` (30 s) | gateway users feed (keyset cursor) into `profiles` |
| deals | `GROWTH_DEALS_SECS` (5 s) | closing deals per tenant, each processed once (`deals.deal_id`) |
| bonus | 15 s | awaiting-deposit grants look for the qualifying deposit; posts pending grant / release / removal legs to the engine; expiry; removal on withdrawal |
| contests | `GROWTH_LEADERBOARD_SECS` (15 s) | start snapshots, live scores and ranks, anti-cheat flags, end freeze; with `autoFinalize`, finalizes ended contests without open flags (then auto-pay) |
| missions | 30 s | trading streaks from the deals processed since the last step; missions of clients with fresh activity (deals, profile changes, deposits, referrals, contest entries, logins) re-evaluated; rewards waiting for KYC paid once the client is verified |
| payouts | 30 s | cashback payouts, prize and redemption wallet credits (idempotent keys, retried) |
| reversals | 10 min | reopened deals of the last 14 days: points reversed, unpaid cashback voided, contest trade removed |
| points expiry | hourly | points older than `pointsExpiryMonths` with no activity since are expired |
| journey facts | 60 s | reports `GET /v1/internal/client-facts` (first deposit, first live account, first live trade) into `profiles` |
| journeys | 15 s | enrol clients whose trigger fired since each live journey went live, then walk due enrolments (see [Journeys](#journeys)) |

Every engine / wallet write carries an idempotency key derived from a row id (`growth:bonus:<event>:<leg>`, `growth:cashback:<payout>`, `growth:prize:<entry>`, `growth:redeem:<redemption>`), so a retry after a timeout never books twice. The engine answers a reused key with `409 duplicate_idempotency_key`, which counts as booked.

## Rules

| Area | Rule |
|---|---|
| Points (D135) | Per closed deal on a **live** account (demo when a rule says `accountType: "any"`), held ≥ `minHoldSeconds` (default 60 s), not a price correction. `points = floor(lots × pointsPerLot × tier multiplier)`. The first active rule that matches wins (priority ascending): symbol list, then asset class, then any; account groups filter (empty = all). Cent accounts: lots × 0.01 |
| Tiers | By points earned in the last 12 months (earn + bonus + promo). Multiplier applies to trading points only |
| Point value | `pointValue` USD per point (default 0.01) for display and the cost report |
| Expiry | Earned points expire after `pointsExpiryMonths` (default 12) without any earn |
| Redemption | Balance ≥ cost, tier ≥ `minTier`, stock > 0. `cashback` credits the wallet (`adjustment`), `bonus_credit` opens a bonus grant on a chosen live account with the item's release rate, `fee_discount` issues a voucher (see internal API). Points are deducted in the same transaction as the redemption row; a failed wallet credit is retried, never refunded silently |
| Cashback | Programme = USD per lot on matching symbols / asset classes / groups, live accounts only. Opt-in programmes need enrolment. `maxPerMonth` caps a client's monthly accrual. Accruals older than `cashbackHoldHours` (default 24; 0 in development) are paid to the wallet as one `refund` transfer per client |
| Bonus (D29) | `deposit` campaigns: bonus = min(deposit × pct / 100, cap), first qualifying `transfer_in` / `deposit` ≥ `minDeposit` on a live account after the claim (within `claimWindowDays`). `fixed` campaigns: a fixed amount on a chosen live account. The credit is posted to the engine's **bonus** sub-ledger (counts toward equity and margin, never withdrawable). Each closed lot releases `releasePerLot` USD: bonus −x and balance +x (`adjustment`), capped at the remaining amount. Lots required = amount ÷ releasePerLot. At `expiresAt` or on a withdrawal / transfer out (when `forfeitOnWithdrawal`), the unreleased remainder is removed (clamped to the account's bonus). One active grant per account |
| Promo codes (D144) | Kinds `bonus` (claims a campaign), `points`, `discount` (voucher). Active, inside the validity window, `uses < maxUses`, the client's uses < `perUserLimit`, segment rules (new users within `newUsersDays`, countries, KYC). Checked and counted under a row lock, so limits hold under concurrency. Blocked attempts are logged |
| Contests (D135) | `demo`: joining opens a dedicated demo account with `startingBalance` (credentials returned once). `live`: the client picks a live account (groups filter, `minEquity`). Scoring `return_pct` = (realised + floating) ÷ start equity × 100, `profit` = realised + floating, `lots` = lots closed. Only deals closed inside the window on the entered account count. Entrants with fewer than `minTrades` trades rank after everyone who qualifies. Ties: earlier join first |
| Anti-cheat | Flags (open for review): `balance_change` (deposit, withdrawal, transfer, refill or staff adjustment on the account during the contest; auto-disqualifies when `antiCheat.disqualifyOnBalanceChange`), `single_trade` (one trade > `maxSingleTradePct` of positive profit, with ≥ 3 trades), `short_holds` (> 50% of trades held < `minHoldSeconds`). Disqualified entries keep their row but get no rank and no prize |
| Prizes | Admin finalizes after the end (ranks frozen), then pays: `wallet` = wallet credit (`adjustment`), `credit` = engine `credit` on the entered live account (or the client's first live account) |
| Contest templates (phase 4) | Seeded per broker (weekly % return, monthly lots race, weekend crypto sprint; all live, free entry, KYC). `schedule` places the window in server time (GMT+3): `next_monday` (+ `durationDays`), `next_month`, `next_weekend` (Sat 00:00 – Mon 00:00), `tomorrow`. `split` = a preset (`winner`, `top3` 50/30/20, `top5`, `top10` ladder, `top20`) or bands `{rankFrom, rankTo, pct}` adding up to 100 %. Prizes = pool × pct ÷ 100 ÷ ranks in the band, rounded down to the cent, the cents lost go to rank 1 (rows add up to the pool). A template becomes an ordinary contest input, validated like a hand-made one |
| Asset classes (phase 4) | `contests.asset_classes` (empty = every CFD): only deals of those classes (`config/instruments.json`) count |
| Clone | `duplicate` copies a contest as a draft for the next window of the same length (from the original's end, or tomorrow 00:00 when that is past), `clonedFrom` set |
| Missions (phase 4) | Kinds `kyc`, `first_deposit` (one-off only), `deposit` (largest single deposit in the period, engine ledger), `lots`, `trading_days`, `hold` (hours), `join_contest` (live), `referral` (friends whose first deposit landed in the period). Periods: daily, weekly (ISO, Monday) in server time, or once. Trades count when live, CFD, not reversed / price correction, ≥ `missionMinLots` and held ≥ `missionMinHoldSeconds` (per mission `params.minLots`, `params.minHoldSeconds`, `params.assetClasses`). Completion is once per (mission, client, period) under a row lock; XP ref `mission:<id>:<period>`. Cash / credit rewards: `kyc` until verified (when `kycRequired`), then `pending` (below `autoPayBelow`, paid at once) or `approval` |
| XP, levels, streaks (phase 4) | `xp_ledger` is append-only and unique per (source, ref): missions, `xpContestJoin` per live contest joined, `xpStreakWeek` per 7 days of trading streak, staff adjustments. Level = highest `levels.min_xp` ≤ total XP (seeded Rookie 0 … Legend 12,000). Streaks: `trading` (a day with a qualifying live trade; weekends never break it) and `login` (a day with a Client Area / app call), up to `streakGraceDays` missed days forgiven once a week, otherwise restart at 1; same or earlier days change nothing |
| Auto-pay (phase 4) | `autoPayBelow` USD (0 = off): at finalize, prizes below it → `pending` and paid at once (`contest.autopay`, system), the rest → `approval`; staff `pay` pays pending, approval and failed; the contest is `paid` once nothing waits for approval. Mission rewards follow the same threshold |
| Nicknames, leaderboards (phase 4) | Clients see other clients by nickname and level badge only (no name, country, login or user id); a deterministic auto nickname until the client picks one (3–20 letters / digits / `_ - .`, no `@`, not all digits, unique per broker). Public boards leave out `public = false` (and `NULL` when `nicknamePublicDefault` is off); ranks stay real (gaps). Staff views keep names |
| Banners (D121) | Placement `dashboard` / `wallet` / `rewards` / `terminal`. Targeting: countries (ISO-2, empty = all), KYC statuses, account types (`live`, `demo`, `none` = no account), new users within N days. Active inside the window, highest priority first, dismissed ones hidden for that client |
| Kalks FX Options (O34) | Option deals (`instrument: "option"` / an `option` object in the engine feed, or an option series symbol such as `EURUSD-20261009-1.1650-C`) earn **no** points, cashback or bonus lot-release, and never count in a CFD contest. They are recorded once with 0 lots (`deals.instrument = 'option'`, plus `premium` = opening premium in USD and `fill_id` = the order-book fill when the engine sends one). CFD contests use CFD P&L only: floating = equity − balance − credit − bonus − `optionValue`, and the start / minimum equity exclude the options. `option_premium` / `option_settlement` ledger postings are trading flows: never a contest balance change, a deposit (deposit bonus) or a withdrawal (bonus forfeiture). Share cards show option contracts as `contracts`, never as lots |
| Options contests (O36) | `instrument: "options"` (default `cfd`). Only option exits count (manual closes, stop-outs, expiry settlements, knock-outs) closed inside the window on the entered account and opened at or after the start; CFD deals on that account don't. Scoring `return_pct` = realised option P&L ÷ start equity × 100, `profit` = realised option P&L (net of commission, USD), `contracts` = contracts of the trades that count (no floating part: no model prices). Join: the client must be options-eligible (gateway `GET /v1/internal/suitability/{user}?product=options` → `eligible`, else 409 `options_intro_required`; gateway down = 503) and a live account must not be in a copy / PAMM / MAM / prop group (the engine never lets those trade options). Anti-abuse: `minPremium` (USD per trade, admin-set) = a trade whose opening premium (|price P&L − exit cash|) is below it adds no contracts and no trade count, but its P&L still counts so a loss can't be hidden; **self-trades** between the client's own accounts of the same type (live / demo) count for nothing and raise a `self_trade` flag (high): the same order-book fill on both accounts, the opposite side of the same series held at the same time (a hedge), or the opposite direction in the same series within 2 s of this trade's open or close (a cross). Closed legs come from the deals feed (re-checked every refresh); legs still open are read from the engine (`GET /v1/dealing/positions?login=`) once the contest has ended and before finalize (an engine error stops the refresh, so finalize answers 503 rather than freeze unchecked scores). `minTrades` counts only trades that fully count |
| Share cards (D136) | Snapshot of one closed trade or a period on one account. Without `showAmounts` only the symbol, side, prices, % move / % return, trade count and win rate are stored; money amounts are stored only when the client opts in. Carries the client's referral code; the public page and image are at `/s/<code>` on the Client Area |
| Options share cards (O36) | A closed option trade's card also stores `option`: underlying, strike, call / put, expiry, side, contracts, entry → exit premium in USD per contract (entry = |price P&L − exit cash| ÷ contracts, exit = |exit cash| ÷ contracts), `pnlPct` = price P&L ÷ opening premium (also the card's `movePct`), why it closed (`closed`, `expired`, `knocked_out`, `stop_out`, `sl`, `tp`), and `breakeven` / `settle` (fixing or spot) for the payoff sketch. Never the account balance; the USD P&L only with `showAmounts` |

## Journeys

Marketing automation (D144, `src/journeys.rs`, `src/api/journeys.rs`). A journey = a trigger + up to 20 steps; editing, launching, pausing, archiving and test sends are audited.

| Trigger | Fires when (per client) | Enrols |
|---|---|---|
| `signed_up` | `profiles.signed_up_at` | once |
| `email_verified` / `kyc_approved` | gateway feed `email_verified_at` / `kyc_verified_at` (latest approved KYC case) | once |
| `first_deposit` / `account_opened` / `first_trade` | reports facts: first credited deposit, first live account, first closed live deal | once |
| `no_deposit` + `days` | sign-up + N days, still no deposit | once |
| `inactive` + `days` | last sign-in + N days | once per inactivity period (occurrence = last sign-in day) |
| `birthday` | date of birth MM-DD = today (UTC) | once a year |

Only trigger events at or after `live_since` (the first launch) enrol, so launching never mails the back catalogue. Blocked or closed clients never enrol.

| Step | Does |
|---|---|
| `wait {amount, unit: minutes|hours|days}` | parks the enrolment until then (max 365 days) |
| `email {subject, preheader, heading, body, buttonLabel, buttonUrl}` | gateway `POST /v1/internal/mail/marketing`: the transactional email design with the tenant's brand (name, logo, colours) and a signed unsubscribe link; the gateway skips clients who unsubscribed (`email_suppressed`, the journey carries on). Without SMTP the email is logged (`email_logged`) |
| `inapp {title, body, link}` | support `POST /v1/notify` type `marketing.journey`, `email: false`, dedupe key `journey:<enrolment>:<step>` (the client's News and offers in-app setting applies) |
| `condition {check, expect}` | `email_verified`, `kyc_approved`, `has_deposit`, `has_live_account`, `has_traded`, `marketing_consent`; not matching = the enrolment exits |

Placeholders in email and in-app text: `{{first_name}}` (or "there"), `{{last_name}}`, `{{name}}`, `{{country}}`, `{{referral_code}}`.

The runner leases due enrolments (`next_run_at + 5 min`, `FOR UPDATE SKIP LOCKED`), so several instances never double-send; after each step the position is saved at once. A failed email / in-app call is retried with back-off (1, 2, 4, 8, 16 min) and fails the enrolment after 6 attempts. Paused journeys keep their enrolments where they are; resuming carries on and enrols triggers that fired meanwhile. Archiving exits everyone. `journey_events` is the enrolment log and the per-step stats source (sent, logged, suppressed, retries, condition passed / exited).

## API

Every route except `GET /health` and the public website routes (`GET /v1/public/leaderboard`, `GET /v1/public/contests`) needs `X-Kalks-Internal: $GROWTH_INTERNAL_TOKEN` and takes `X-Kalks-Tenant` (default `kalks`). JSON in and out, camelCase, money in USD as JSON numbers, times RFC 3339. Errors: `{"error": {"code", "message", "field"?}}` with 400 `bad_request`, 401 `unauthorized`, 403 `forbidden`, 404 `not_found`, 409 (`limit_reached`, `already_joined`, `already_claimed`, `insufficient_points`, `out_of_stock`, `not_eligible`, `contest_closed`, `state`), 422 `validation` (+`field`), 503 `unavailable`.

### Client routes (Client Area BFF)

Headers: `X-Kalks-User-Id` (gateway user id, required), plus segment headers the BFF takes from the gateway session: `X-Kalks-Country`, `X-Kalks-Kyc` (`unverified|pending|verified|rejected`), `X-Kalks-Created-At` (RFC 3339), `X-Kalks-Name` (percent-encoded "First Last"), `X-Kalks-Referral-Code`. They also refresh the client's profile row.

| Method & path | Body / query | Response |
|---|---|---|
| `GET /v1/growth/me/rewards` | – | `Rewards` (below) |
| `GET /v1/growth/me/points?kind=&page=&limit=` | kind `earn|redeem|bonus|promo|expire|adjust|reversal` | `{items: PointsTx[], page, limit, total}` |
| `POST /v1/growth/me/redeem` | `{itemId, login?}` (login required for `bonus_credit`) | `{redemption: Redemption, balance}` |
| `GET /v1/growth/me/redemptions` | – | `{items: Redemption[]}` |
| `GET /v1/growth/me/vouchers` | – | `{items: Voucher[]}` |
| `GET /v1/growth/me/cashback` | – | `CashbackMe` |
| `POST /v1/growth/me/cashback/{programmeId}/enrol` | `{}` | `{enrolled: true}` |
| `GET /v1/growth/me/promotions` | – | `{campaigns: CampaignPublic[], grants: Grant[], promoHistory: PromoUse[]}` |
| `POST /v1/growth/me/bonuses/{campaignId}/claim` | `{login?}` (required for `fixed`) | `{grant: Grant}` |
| `POST /v1/growth/me/promo` | `{code, login?}` | `{result: {kind, message, grant?: Grant, points?: number, voucher?: Voucher}}` |
| `GET /v1/growth/me/contests` | – | `{items: ContestCard[], stats: {entered, prizesWon, prizeFinishes, bestRank|null, active}}` |
| `GET /v1/growth/me/contests/{id}` | – | `{contest: Contest, leaderboard: Standing[] (top 100), myEntry: Standing|null, entrants}` |
| `POST /v1/growth/me/contests/{id}/join` | `{login?}` (required for live) | `{entry: Standing, credentials?: {login, password, investorPassword}}` (demo only, shown once) |
| `GET /v1/growth/me/hub` | – | `{level: Level, streaks: {trading: Streak, login: Streak, weekXp}, missions: MissionView[], contests: ContestCard[] (scheduled / running), recent: RecentReward[], nickname: Nickname}` |
| `GET /v1/growth/me/missions` | – | `{items: MissionView[]}` (evaluates first) |
| `GET /v1/growth/me/xp` | – | `{level: Level, streaks, history: {id, xp, source, description, createdAt}[]}` |
| `GET /v1/growth/me/nickname` · `POST` | `{nickname?, public?}` | `Nickname`; 409 `nickname_taken`, 422 `validation` |
| `GET /v1/growth/me/leaderboard?contest=<id|slug>|period=week|month|all&limit=` | – | `Board` with `me` (the client's own row, also when outside the limit) |
| `GET /v1/growth/me/rank?login=` | – | `{contests: {id, name, scoring, endsAt, rank, ranked, score, trades, minTrades, login, running}[], level: Level}` (Kalks Trader rank widget) |
| `GET /v1/growth/me/banners?placement=` | – | `{items: BannerView[]}` |
| `POST /v1/growth/me/banners/{id}/events` | `{kind: "impression"|"click"|"dismiss"}` | `{ok: true}` |
| `POST /v1/growth/me/shares` | `{kind: "trade", login, dealId, showAmounts}` or `{kind: "period", login, from, to, showAmounts}` | `{share: Share}` |
| `GET /v1/growth/me/shares` | – | `{items: Share[]}` |

Shapes:

```ts
Rewards = {
  points: { balance, lifetime, earnedThisMonth, lotsThisMonth, earned12m, expiringSoon: { points, at } | null },
  tier: { key, name, rank, multiplier, minPoints, perks: string[] },
  nextTier: { key, name, minPoints, pointsToGo } | null,
  tiers: Tier[],                      // {key, name, rank, minPoints, multiplier, perks[]}
  rules: EarnRule[],                  // {id, name, assetClass|null, symbols[], accountGroups[], accountType, pointsPerLot}
  pointValue, minHoldSeconds, pointsExpiryMonths,
  catalogue: CatalogueItem[],         // {id, name, description, kind, costPoints, value, minTier|null, stock|null, active, params}
  recent: PointsTx[],                 // last 10
  series: { day: "YYYY-MM-DD", points }[]   // last 30 days earned
}
PointsTx = { id, kind, points, description, login|null, dealId|null, createdAt }
Redemption = { id, itemId, itemName, kind, points, value, status: "pending"|"completed"|"failed", login|null, voucherCode|null, grantId|null, error|null, createdAt, completedAt|null }
Voucher = { id, code, kind: "fee_discount", pct, appliesTo: "any"|"prop"|"commission", status: "active"|"used"|"expired", expiresAt, usedAt|null, source }
CashbackMe = {
  programmes: { id, name, description, assetClasses[], symbols[], accountGroups[], usdPerLot, maxPerMonth|null, optIn, enrolled, startsAt, endsAt|null, lotsMonth, earnedMonth }[],
  totals: { accrued, paid, month, lifetime },   // accrued = not yet paid
  accruals: { id, programmeId, programme, dealId, login, symbol, lots, amount, status, createdAt }[],   // last 100
  payouts: { id, amount, status: "pending"|"paid"|"failed", createdAt, paidAt|null }[],
  series: { day, amount }[]                      // last 30 days
}
CampaignPublic = { id, name, description, terms, kind: "deposit"|"fixed", pct, cap, fixedAmount, minDeposit, releasePerLot, expiryDays, forfeitOnWithdrawal, claimWindowDays, startsAt, endsAt|null, eligible, reason|null, claimed }
Grant = { id, campaignId, campaign, status: "awaiting_deposit"|"pending"|"active"|"completed"|"forfeited"|"expired"|"cancelled"|"failed",
          source, login|null, depositAmount|null, amount, released, remaining, lotsTraded, lotsRequired, releasePerLot, progressPct,
          claimedAt, grantedAt|null, expiresAt|null, endedAt|null, endReason|null, claimDeadline|null }
PromoUse = { id, code, kind, status: "applied"|"blocked", reason|null, createdAt }
ContestCard = Contest & { myEntry: Standing|null }
Contest = { id, slug, name, description, kind: "demo"|"live", instrument: "cfd"|"options", minPremium|null (options, USD per trade),
            status: "draft"|"scheduled"|"running"|"ended"|"finalized"|"paid"|"cancelled",
            startsAt, endsAt, scoring: "return_pct"|"profit"|"lots" (cfd)|"contracts" (options), minTrades, maxEntrants|null, entrants, startingBalance|null,
            demoGroup|null, accountGroups[], kycRequired, minEquity|null, prizes: {rankFrom, rankTo, amount, payout: "wallet"|"credit"}[],
            prizePool, rules, antiCheat: {minHoldSeconds, maxSingleTradePct, disqualifyOnBalanceChange}, updatedAt }
Standing = { entryId, userId (own entry only, else null), name ("Arjun K."), country, login (own entry only, else null), rank|null,
             score, returnPct, profit, lots, contracts, trades, qualified, status: "active"|"disqualified", prize|null, prizeStatus, me: boolean, updatedAt,
             selfTrades|null, smallTrades|null (options contests; own entry and staff only: trades left out as self-trades / below the minimum premium) }
BannerView = { id, title, body, ctaLabel|null, ctaUrl|null, imageUrl|null, tone: "ember"|"gold"|"neutral"|"up", placement, dismissible }
Share = { code, kind: "trade"|"period", login, showAmounts, createdAt, views, url: "/s/<code>", data: ShareData }
ShareData = { name, symbol|null, side|null, openPrice|null, closePrice|null, openTime|null, closeTime|null, movePct|null,
              returnPct|null, trades|null, winRate|null, lots|null, profit|null (only with showAmounts), currency, from|null, to|null,
              referralCode|null, brand: "Kalks",
              // option trades (O36): instrument: "option", contracts, lots: null, and
              option?: { series, underlying, right: "call"|"put", strike, expiry, style, side, contracts, entryPremium, exitPremium (USD per contract),
                         pnlPct, reason, openPremiumUnit, closePremiumUnit, breakeven, settle } }
```

Challenges shapes (phase 4):

```ts
Level = { xp, level, name, badge, minXp, next: {level, name, badge, minXp, xpToGo} | null, progressPct, levels: {level, name, minXp, badge}[] }
Streak = { current (0 when broken), best, lastDay: "YYYY-MM-DD" | null, graceDays }
MissionView = Mission & { period: "2026-10-19" | "2026-W43" | "once", resetsAt | null, progress, completedAt | null, done,
                          rewardStatus: "none"|"kyc"|"approval"|"pending"|"paid"|"failed"|"rejected" }
Mission = { id, name, description, kind, schedule: "daily"|"weekly"|"once", target, params: {minLots?, minHoldSeconds?, assetClasses?},
            xp, rewardAmount | null, rewardPayout: "wallet"|"credit"|null, kycRequired, active, startsAt, endsAt | null, sort, updatedAt }
RecentReward = { kind: "xp"|"prize"|"mission", title, xp | null, amount | null, payout?, status, at }
Nickname = { nickname, custom, public (effective), publicChoice: boolean | null, publicDefault, suggestion }
Board = { board: "contest"|"xp", contest?: {id, slug, name, status, kind, scoring, startsAt, endsAt, prizePool, entrants, prizes},
          period?, since?, scoring?, items: {rank, nickname, score, level: {level, name, badge}, me}[], me: row | null, updatedAt }
```

Client-facing `Standing`s (contest detail, leaderboard) carry the nickname in `name` and `level: {level, name, badge}`; another client's `country`, `login` and `userId` are null.

### Public website routes (no internal token)

| Method & path | Query | Response |
|---|---|---|
| `GET /v1/public/leaderboard` | `contest=<id|slug>` or `period=week|month|all` (default week), `limit` 1–100 (50), `tenant` (default `kalks`) | `Board` without `me` / `me` flags: `{rank, nickname, score, level}` rows only; opted-out clients left out. 404 when the broker has `publicLeaderboards` off or the rewards module off |
| `GET /v1/public/contests` | `tenant` | `{items: {id, slug, name, description, status, kind, instrument, scoring, startsAt, endsAt, prizePool, prizes, entrants, kycRequired, minTrades}[]}` (running, upcoming, ended in the last 30 days) |

Both answer `Cache-Control: public, max-age=15, s-maxage=15` and `Access-Control-Allow-Origin: *`; boards are cached 15 s per (broker, board, limit) in the service (a nickname change clears the broker's cache). On production Caddy serves them on `api.kalkstrade.com` (GET only, internal headers stripped); see docs/CHALLENGES.md for the website contract.

### Public routes (Client Area server only)

| `GET /v1/growth/public/shares/{code}` | – | `Share` (no login, no user id); counts a view unless `?view=0` |
|---|---|---|

### Internal routes (other services)

| Method & path | Body | Response |
|---|---|---|
| `POST /v1/growth/internal/vouchers/redeem` | `{userId, code, appliesTo, ref}` | `{pct, voucher}`; 404 unknown / 409 `used` / 409 `not_eligible` (other scope or expired). Idempotent on (code, ref) |
| `GET /v1/growth/internal/vouchers?user_id=` | – | `{items: Voucher[]}` active only |

### Back Office routes

Headers: `X-Kalks-Staff-Id`, `X-Kalks-Staff-Name` (percent-encoded), `X-Kalks-Staff-Role` and `X-Kalks-Staff-Perms`, set by the admin BFF after it verified the staff session. Writes are audited.

| Method & path | Perm | Body / query | Response |
|---|---|---|---|
| `GET /v1/growth/admin/overview` | read | – | `{loyalty:{members, pointsIssued30d, pointsRedeemed30d, liabilityPoints, liabilityUsd}, cashback:{accrued, paid30d}, bonus:{activeCampaigns, activeGrants, issued30d, released30d, forfeited30d, outstanding}, contests:{running, scheduled, entrants}, promos:{active, redemptions30d, blocked30d}, banners:{active, impressions30d, clicks30d}}` |
| `GET /v1/growth/admin/settings` · `PUT` | read · write | `Settings` `{pointValue, minHoldSeconds, pointsExpiryMonths, cashbackHoldHours, demoPoints, autoPayBelow (0), autoFinalize (false), autoFinalizeDelayMinutes (60), publicLeaderboards (true), nicknamePublicDefault (true), streakGraceDays (1), missionMinLots (0.01), missionMinHoldSeconds (60), xpContestJoin (25), xpStreakWeek (50)}` (partial PUT merges) | `Settings` |
| `GET /v1/growth/admin/tiers` · `PUT` | read · write | `{tiers: Tier[]}` (full replace, ranks 1..n, minPoints ascending) | `{tiers}` |
| `GET /v1/growth/admin/rules` · `POST` | read · write | `EarnRule` + `{active, priority}` | `{items}` / `{rule}` |
| `PATCH /v1/growth/admin/rules/{id}` | write | partial | `{rule}` |
| `GET /v1/growth/admin/catalogue` · `POST` | read · write | `CatalogueItem` | `{items}` / `{item}` |
| `PATCH /v1/growth/admin/catalogue/{id}` | write | partial | `{item}` |
| `GET /v1/growth/admin/redemptions?status=&page=&limit=` | read | – | `{items: (Redemption & {userId, name})[], total, page, limit}` |
| `POST /v1/growth/admin/redemptions/{id}/retry` | approve | `{}` | `{redemption}` |
| `GET /v1/growth/admin/members?q=&page=&limit=` | read | – | `{items: {userId, name, email, country, balance, lifetime, earned12m, tier, lastEarnAt}[], total}` |
| `POST /v1/growth/admin/points/adjust` | approve | `{userId, points (signed), note}` | `{balance}` |
| `GET /v1/growth/admin/cashback/programmes` · `POST` | read · write | `Programme` `{name, description, assetClasses[], symbols[], accountGroups[], usdPerLot, maxPerMonth|null, optIn, active, startsAt?, endsAt?}` | `{items: (Programme & {enrolled, accrued, paid, lots30d})[]}` / `{programme}` |
| `PATCH /v1/growth/admin/cashback/programmes/{id}` | write | partial | `{programme}` |
| `GET /v1/growth/admin/cashback/accruals?programme=&user=&status=&page=&limit=` | read | – | `{items, total}` |
| `GET /v1/growth/admin/cashback/payouts?status=&page=&limit=` | read | – | `{items: {id, userId, name, amount, status, attempts, error, createdAt, paidAt}[], total}` |
| `POST /v1/growth/admin/cashback/payouts/run` | approve | `{}` | `{created, amount}` (pays everything past the hold now) |
| `GET /v1/growth/admin/bonuses/campaigns` · `POST` | read · write | `Campaign` `{name, description, terms, kind, pct, cap, fixedAmount, minDeposit, releasePerLot, expiryDays, forfeitOnWithdrawal, claimWindowDays, accountGroups[], maxClaims|null, perUserLimit, newUsersDays|null, kycRequired, visibility: "public"|"code_only", status: "draft"|"active"|"paused"|"ended", startsAt?, endsAt?}` | `{items: (Campaign & {claims, active, issued, released, forfeited, outstanding})[]}` / `{campaign}` |
| `PATCH /v1/growth/admin/bonuses/campaigns/{id}` | write | partial | `{campaign}` |
| `GET /v1/growth/admin/bonuses/grants?campaign=&status=&user=&page=&limit=` | read | – | `{items: (Grant & {userId, name})[], total}` |
| `POST /v1/growth/admin/bonuses/grants` | approve | `{campaignId, userId, login, amount?, note}` | `{grant}` (manual grant; amount defaults to the campaign's fixed amount) |
| `POST /v1/growth/admin/bonuses/grants/{id}/cancel` | approve | `{note}` | `{grant}` (removes the unreleased remainder) |
| `GET /v1/growth/admin/promos` · `POST` | read · write | `Promo` `{code, description, kind: "bonus"|"points"|"discount", campaignId|null, points|null, discountPct|null, discountAppliesTo, maxUses|null, perUserLimit, newUsersDays|null, countries[], kycRequired, active, startsAt?, endsAt?}` | `{items: (Promo & {uses, blocked, lastUsedAt})[]}` / `{promo}` |
| `PATCH /v1/growth/admin/promos/{id}` | write | partial (not `code`) | `{promo}` |
| `GET /v1/growth/admin/promos/redemptions?promo=&status=&page=&limit=` | read | – | `{items: (PromoUse & {userId, name, promoId})[], total}` |
| `GET /v1/growth/admin/banners` · `POST` | read · write | `Banner` `{title, body, ctaLabel, ctaUrl, imageUrl, tone, placement, countries[], kyc[], accountTypes[], newUsersDays|null, priority, dismissible, active, startsAt?, endsAt?}` | `{items: (Banner & {impressions, clicks, dismissals, ctr})[]}` / `{banner}` |
| `PATCH /v1/growth/admin/banners/{id}` | write | partial | `{banner}` |
| `GET /v1/growth/admin/banners/preview?placement=&country=&kyc=&accountType=&signupDays=` | read | – | `{items: BannerView[], audience}` (what that segment sees; `audience` = profiles matching country/KYC/sign-up) |
| `GET /v1/growth/admin/contests` · `POST` | read · write | `Contest` input (`slug?`, name, description, kind, instrument ("cfd" default | "options"), startsAt, endsAt, scoring (cfd: return_pct|profit|lots, options: return_pct|profit|contracts), minTrades, minPremium (options), maxEntrants, startingBalance, demoGroup, accountGroups, kycRequired, minEquity, prizes, rules, antiCheat, status: "draft"|"scheduled"). Options contests refuse copy / PAMM / MAM / prop demo or account groups | `{items: Contest[]}` / `{contest}` |
| `GET /v1/growth/admin/contests/{id}` | read | – | `{contest, leaderboard: (Standing & {userId, login, flags: string[], disqualifyReason})[], flags: Flag[]}` |
| `PATCH /v1/growth/admin/contests/{id}` | write | partial (kind, instrument, timing, scoring, minTrades, minPremium, eligibility and prizes only before the start) | `{contest}` |
| `POST /v1/growth/admin/contests/{id}/cancel` | write | `{note}` | `{contest}` |
| `POST /v1/growth/admin/contests/{id}/refresh` | write | `{}` | `{contest}` (recomputes scores now) |
| `POST /v1/growth/admin/contests/{id}/finalize` | write | `{}` | `{contest, leaderboard}` (after the end; ranks and prizes frozen) |
| `POST /v1/growth/admin/contests/{id}/pay` | approve | `{}` | `{paid, failed, amount}` |
| `POST /v1/growth/admin/contests/{id}/entries/{entryId}/disqualify` · `/reinstate` | write | `{reason}` | `{entry}` |
| `POST /v1/growth/admin/contests/{id}/flags/{flagId}/resolve` | write | `{action: "clear"|"disqualify", note}` | `{flag}` |
| `GET /v1/growth/admin/reports?from=&to=` | read | dates | `{from, to, totals: {bonusIssued, bonusReleased, bonusForfeited, cashbackPaid, cashbackAccrued, prizesPaid, pointsRedeemedUsd, redemptionCashPaid, promoRedemptions, total}, byCampaign: {id, name, issued, released, forfeited, claims}[], byProgramme: {id, name, accrued, paid, lots}[], byContest: {id, name, prizes, entrants}[], series: {day, bonus, cashback, prizes, points}[]}` (`total` = released bonus + cashback + prizes + redeemed points value; issued bonus is shown separately, it is not cash until released) |
| `GET /v1/growth/admin/audit?page=&limit=` | read | – | `{items: {id, at, actor, actorName, action, target, before, after, note}[], total}` |
| `POST /v1/growth/admin/run/{job}` | write | job `profiles|deals|bonus|contests|payouts|reversals|expiry|journeys|missions` | `{ok, result}` |
| `GET /v1/growth/admin/contest-templates` | read | – | `{items: Template[], presets: {key, label, bands}[], autoPayBelow, assetClasses}` (seeds the defaults on first use) |
| `POST /v1/growth/admin/contest-templates` · `PATCH …/{id}` | write | `Template` `{key?, name, description, kind, instrument, scoring, schedule, durationDays, minTrades, maxEntrants, accountGroups, assetClasses, kycRequired, minEquity, prizePool, split, payout, rules, antiCheat, active, sort}` | `{template}` |
| `POST /v1/growth/admin/contest-templates/{id}/contest` | write | `{name?, startsAt?, endsAt?, prizePool?, split?, payout?, status?}` | `{contest}` (window from the schedule, prizes from the pool and split, `templateId` set) |
| `POST /v1/growth/admin/contests/{id}/duplicate` | write | `{}` | `{contest}` (a draft for the next window, `clonedFrom`) |
| `POST /v1/growth/admin/contests/{id}/save-template` | write | `{name?}` | `{template}` (the prize table as shares of the pool) |
| `GET /v1/growth/admin/missions` · `POST` · `PATCH …/{id}` | read · write | `Mission` input (validated: kinds, one-off for `kyc` / `first_deposit`, XP or a reward) | `{items: (Mission & {completions, completions7d, participants})[], kinds, defaults}` / `{mission}` |
| `GET /v1/growth/admin/payouts?status=approval|pending|failed|paid|kyc|rejected&page=&limit=` | read | – | `{items: {source: "mission"|"contest", id, contestId, userId, name, amount, payout, status, title, error, at}[], total, autoPayBelow}` |
| `POST /v1/growth/admin/missions/rewards/{id}/approve` · `/reject` | approve | `{note}` (required to reject) | `{reward}`; 409 `state` when not waiting |
| `GET /v1/growth/admin/levels` · `PUT` | read · write | `{levels: {level, name, minXp, badge}[]}` (1..n, level 1 at 0 XP, increasing; badges `bronze silver gold platinum diamond crown rocket gem`) | `{levels (with clients per level), badges}` |
| `POST /v1/growth/admin/xp/adjust` | write | `{userId, xp (signed), note}` | `{xp}` (total) |
| `GET /v1/growth/admin/journeys` | read | – | `{items: Journey[] (with stats), archived}` |
| `GET /v1/growth/admin/journeys/meta` | read | – | `{triggers, checks, placeholders}` |
| `POST /v1/growth/admin/journeys` · `PATCH …/journeys/{id}` | write | `{name, description, trigger: {kind, days?}, steps: Step[]}` | `{journey}` (created as `draft`) |
| `GET /v1/growth/admin/journeys/{id}` | read | – | `{journey}` with `stats` and `stepStats: {stepId, kind, counts: {event: n}, pending}[]` |
| `POST /v1/growth/admin/journeys/{id}/status` | write | `{status: "live"|"paused"|"archived"}` | `{journey}` |
| `GET /v1/growth/admin/journeys/{id}/enrollments?status=&user=&page=&limit=` | read | – | `{items: {id, userId, name, email, status, stepIndex, currentStep, nextRunAt, lastEvent, lastError}[], total}` |
| `GET /v1/growth/admin/journeys/{id}/events?enrollment=&page=&limit=` | read | – | `{items: {id, enrollmentId, userId, name, stepId, kind, detail, at}[], total}` |
| `POST /v1/growth/admin/journeys/{id}/test` | write | `{to, steps?}` (the admin BFF sets `to` to the signed-in staff member's address) | `{results: {stepId, kind, status}[]}`: emails to `to` with `[Test]` and sample data, in-app to the staff bell |

## Permissions

The gateway RBAC (`services/gateway/src/rbac.rs`) defines `marketing.read`, `marketing.write` and `marketing.approve`. The Back Office BFF (`apps/admin/lib/marketing-perms.ts`) checks them per route and forwards the resolved list in `X-Kalks-Staff-Perms`; the service checks it again. Without that header (older sessions, other callers) the service falls back to these role lists:

| permission | allows | fallback roles |
|---|---|---|
| `marketing.read` | every GET | platform_owner, super_admin, admin, marketing, finance, compliance, support, risk_manager, partner_manager, viewer |
| `marketing.write` | campaigns, promos, banners, contests (create, edit, duplicate, finalize, disqualify), contest templates, missions, levels, XP adjustments, rules, tiers, catalogue, programmes, settings, jobs | platform_owner, super_admin, admin, marketing |
| `marketing.approve` | money out: pay contest prizes (incl. those above the auto-pay threshold), approve or reject mission rewards, manual bonus grants and cancellations, points adjustments, run cashback payouts, retry redemptions | platform_owner, super_admin, admin, finance |

## How the apps integrate

| App | Integration |
|---|---|
| Client Area | BFF `app/api/growth/[[...path]]` → `/v1/growth/me/*` with the session user and segment headers. Pages: `/rewards` (the Rewards hub: level, streaks, missions, recent rewards; then contests, with an OPTIONS badge, contracts and the options rules on options contests), `/rewards/leaderboard`, `/rewards/contests/[id]`, the nickname and level on `/profile`, the level strip on the dashboard, `/rewards/loyalty`, `/rewards/cashback`, `/rewards/promotions`. Banner slots (`components/growth/banner-slot.tsx`) on the dashboard and wallet. "Share P&L" on trade history. Public share page `/s/[code]` and PNG `/s/[code]/image` (next/og; option trades get the contract, premiums per contract and a payoff sketch). |
| Kalks Trader (rank widget) | `GET /api/growth/rank` (owner of the acting engine session; not investor logins) → `GET /v1/growth/me/rank?login=`; the chip in the screen bar shows the running contest's rank. |
| Kalks Trader | `POST /api/growth/shares {dealId, showAmounts}` (owner of the acting engine session; not investor or staff sessions) → `POST /v1/growth/me/shares`. The Share button on closed option trades (Options › Closed, History option rows) opens the card, served by the Client Area. |
| Back Office | BFF `app/api/marketing/[...path]` → `/v1/growth/admin/*` with permissions from `lib/marketing-perms.ts`. Pages under `/marketing`: bonuses, promo codes, banners, contests & challenges (3-step wizard from templates, templates, missions, levels, payouts, challenge settings), rewards (rules, tiers, catalogue, redemptions), cashback, reports, automation (journeys) and campaigns (UTM attribution, served by the reports service `GET /v1/admin/campaigns`). |
| Other services | `POST /v1/growth/internal/vouchers/redeem` for fee-discount vouchers (prop checkout, commission rebates). |
| Notifications | Best effort `POST $NOTIFY_URL/v1/notify` `{userId, type, title, body, link}` (header `X-Kalks-Service: growth`) for bonus granted / released / forfeited, prize paid, redemption completed; journeys' in-app steps; challenges: `rewards.mission` (mission complete), `rewards.level_up`, `rewards.paid`, in-app only with a dedupe key. The Client Area reloads its rewards views when one of these arrives on the support stream. |
| Public website | `GET /v1/public/leaderboard`, `GET /v1/public/contests` through Caddy on `api.kalkstrade.com` (docs/CHALLENGES.md). |

## Environment

| Variable | Default | |
|---|---|---|
| `GROWTH_BIND` | `127.0.0.1:8101` | |
| `GROWTH_DATABASE_URL` | `postgres://postgres@127.0.0.1:5433/kalks_growth` | created and migrated on first start |
| `GROWTH_INTERNAL_TOKEN` | – | required when `GROWTH_ENV=production` |
| `GROWTH_ENV` | `development` | |
| `GROWTH_WORKERS` | `true` | tests turn the loops off |
| `GROWTH_DEALS_SECS` / `GROWTH_SYNC_SECS` / `GROWTH_LEADERBOARD_SECS` | 5 / 30 / 15 | |
| `GROWTH_LOG_FORMAT` | `json` | or `pretty` |
| `GATEWAY_URL` / `GATEWAY_INTERNAL_TOKEN` | `http://127.0.0.1:8080` | profiles feed |
| `TRADING_URL` / `TRADING_INTERNAL_TOKEN` | `http://127.0.0.1:8090` | deals, accounts, ledgers, bonus / credit postings |
| `WALLET_URL` / `WALLET_INTERNAL_TOKEN` | `http://127.0.0.1:8095` | payouts |
| `NOTIFY_URL` / `SUPPORT_INTERNAL_TOKEN` | `http://127.0.0.1:8100` | support notifications (journey in-app steps) |
| `REPORTS_URL` / `REPORTS_INTERNAL_TOKEN` | `http://127.0.0.1:8102` | client facts for journey triggers |
| `INSTRUMENTS_FILE` | `config/instruments.json` | symbol → asset class |

Production runs `deploy/systemd/kalks-growth.service`. `deploy/deploy.sh` builds it, generates `GROWTH_INTERNAL_TOKEN` once, derives `GROWTH_DATABASE_URL` (database `kalks_growth`) from the gateway's, and writes `GROWTH_URL` / `GROWTH_INTERNAL_TOKEN` into the Client Area and Back Office env files.

## Tests

```bash
cargo test -p growth
```

- Unit (`src/calc.rs`): points per lot with rule matching, tier multiplier and cent lots; cashback with the monthly cap; bonus amount (pct + cap, min deposit) and release per lot (partial, capped, completion); promo eligibility (window, limits, per-user, segments); contest scoring, ranking (min trades, ties, disqualified) and prize allocation; anti-cheat flags; options contests: scoring per instrument, system groups, opening premium of an exit, exclusions (self-trade over minimum premium, unknown premium) and self-trade detection (shared fill, hedge, cross within 2 s, and what is not one).
- Journeys (`tests/journeys.rs`, gateway mailer and support notify mocked): enrolment once per client after `live_since`, email → condition → in-app → wait, suppressed email for an unsubscribed client, retry after a mailer outage, per-step stats, paused journeys don't move, `no_deposit` after N days.
- Integration (`tests/growth.rs`, throw-away database `kalks_growth_test_<pid>_<n>`, skipped without PostgreSQL; engine and wallet are mock HTTP servers): deal ingest → points / cashback / bonus release / contest trades once per deal; promo limits under 20 concurrent redemptions; bonus grant → release legs → completion; redemption → wallet credit with idempotent retry; option deals earn no points / cashback / bonus release / CFD-contest trade (also when only the series code identifies them), contest floating and start equity exclude `optionValue`, and option premium / settlement postings neither disqualify a contest entry nor forfeit a bonus; options contests (eligibility via the gateway suitability mock and the prop group, realised option P&L, contracts, the minimum premium, self-trades by hedge / shared fill / a leg still open at the end, CFD trades kept out, ranks and prizes after finalize); options share cards (terms, premiums per contract, P&L on premium, expiry of a short put from the series code only, no balance, amounts only on opt-in).

- Challenges (`tests/challenges.rs`, database `kalks_growth_ch_<pid>_<n>`, the router served on a loopback port): templates → contests (split, schedule, refused split), duplicate and save as template, the crypto-only asset filter; missions count only qualifying trades (scalps, demo, micro lots left out), complete once per period also under concurrent evaluation, a new period counts again, the hub shows them; rewards wait for KYC, auto-pay below the threshold, staff approval above it (marketing.approve only, decided once), audit; one-off missions from profile facts (KYC, first deposit, referral); XP once per ref, the append-only ledger, level thresholds validated, trading streak with the 7-day XP once, login streak once per day; contest auto-pay (threshold 0 pays nothing, below paid by the system, above waits, staff pay the rest once, contest `paid` after), auto-finalize; leaderboard privacy (public board without token, nicknames only, no names / emails / logins / user ids / countries, opt-out left out, cache headers, switch-off 404; in-app boards with nicknames, staff with names; rank widget).
- Unit (`src/challenge_calc.rs`, `src/templates.rs`): split presets add up and split exactly (cents to rank 1), template windows in server time (Monday, month, weekend, year end), clone windows, mission periods (ISO week at year end), streaks (grace once a week, weekends, late data), levels, nickname rules.

## Known gaps

- Challenges: contest score changes are polled (the support stream has no silent "data changed" event; only notifications push). `deposit` missions read the engine ledger per live account (throttled 2 min per client). Days use a fixed GMT+3 offset.

- The engine has no deals-since-id feed (same gap as the IB service): the poller re-reads a 2-minute overlap and caches account type / group per login.
- Bonus release reversal: when the desk reopens a deal after its lots released bonus, the release stays (logged for review).
- Credit is not removed on stop-out by the engine; the bonus grant is capped at the account's actual bonus when removed.
- Journey emails report sent / logged / suppressed; opens and clicks are not tracked (no pixel or link redirect).
- Journey triggers poll (15 s, facts every 60 s), so a step can run up to a minute after the event.
- Wallet transfer kinds: cashback uses `refund`, prizes and points cashback use `adjustment` (no dedicated `cashback` / `prize` kind yet).
- Options contests, self-trades: the shared-fill rule needs `option.fill.id` on the engine's dealing feed (read when present; the engine's DeskDeal doesn't carry it yet), so today a cross is caught by the same-series timing rules, and on the order book the engine's own self-trade prevention (per client across accounts) already stops it. A leg on another account that is still open is read from the engine only once the contest has ended; while it runs, such a trade counts until that leg closes.
- Options contests score realised P&L only: an option still open at the end doesn't count (no model prices in the score).

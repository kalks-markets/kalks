# Kalks Circle — the trader community

A social network for traders inside Kalks: profiles, follows, posts, stories, chat, rooms and video, wired into
trading, copy trading, Academy and Rewards. Decided by the founder on 2026-10-09 (51 questions); built in the
Kalks 2 design (`docs/design/KALKS2.md`).

## Decisions (founder, 2026-10-09)

| # | Topic | Decision |
|---|---|---|
| Q1 | Who joins | All clients (demo too); KYC-verified traders get a badge and more reach |
| Q2 | Name | **Kalks Circle** |
| Q3 | Where | A main tab in the Android app and the Client Area, plus public profile / post pages on the website |
| Q4 | Goal | Community & learning, copy-trading growth and marketing reach equally |
| Q5 | Badges | Verified (KYC), Live trader, Master / PAMM manager, Kalks team / mentor |
| Q6 | Stats | Opt-in engine-verified stats on the profile (win rate, monthly %, max drawdown, trades) |
| Q7 | Names | Unique @handle + any display name; real name private |
| Q8 | Privacy | Public or private profiles (private = follow requests) |
| Q9 | Posts | Text + up to 10 photos, chart snapshots from Kalks Trader, verified trade cards, polls, video |
| Q10 | Trade detail | The trader chooses per post: full (entry, SL, TP, lots, money P&L) or % / pips only |
| Q11 | Copy button | "Copy this trade" opens a prefilled order ticket (symbol, side, SL, TP; reader sets size) |
| Q12 | Tags | $cashtags (live price chip + Trade), #hashtags (topic feeds), @mentions |
| Q13 | Stories | Photo / video, chart snapshot, trade result, sentiment stickers (bull/bear poll, question) |
| Q14 | Story life | 24 hours |
| Q15 | Highlights | Yes, named highlights on the profile |
| Q16 / Q49 | Video | **At launch**: video in posts and stories, and any user can post videos to any topic for everyone to watch (topic video tabs) |
| Q17 | Feed | Two tabs: Following (newest first) and For you (ranked) |
| Q18 | Explore rank | Verified performance (risk-adjusted) blended with engagement |
| Q19 | Live data | Live price chips on $cashtags, live P&L on open trade cards, a Trending symbols strip |
| Q20 | Leaderboards | Top traders (verified %), Top creators |
| Q21 | Reactions | Like + Bull / Bear vote on symbol posts (crowd sentiment per post) |
| Q22 | Comments | Threaded; the author can pin one, limit comments to followers or turn them off |
| Q23 | Reposts | Repost and quote |
| Q24 | Saves | Save into named collections |
| Q25 | DMs | Anyone can message; strangers land in Requests until accepted |
| Q26 | Groups | Private group chats (≤ 100), public symbol rooms (#gold, #eurusd…), masters' follower rooms. **No paid rooms** |
| Q27 | Chat media | Photos, chart snapshots, trade cards, voice notes, files |
| Q28 | Retention | Messages stored; Kalks compliance reads a chat only when it is reported or for a legal request (audited) |
| Q29 | Live rooms | Phase 2 (after launch) |
| Q30 | Earning | Only through copy trading / PAMM fees and IB commissions (no paid subscriptions) |
| Q31 | Tips | No |
| Q32 | Creators | Rewards points for helpful posts, creator badge + featured on Explore, IB link in bio, fee discounts for top creators |
| Q33 | Moderation | AI check on every upload + user reports + staff queue in the Back Office |
| Q34 | Links | External links blocked except an allow-list (Kalks pages, YouTube, TradingView, …) |
| Q35 | P&L claims | Profit claims only through engine-verified trade cards; P&L screenshots from other apps are detected and blocked |
| Q36 | Disclaimer | Automatic small line on posts with a trade card, cashtag or chart: "Not investment advice. Trading involves risk." |
| Q37 | User controls | Block / mute / restrict, report anything, hidden words, close friends list for stories |
| Q38 | Staff tools | Moderation queue, ban & shadow-hide, keyword & link rules, feature & pin + announcements |
| Q39 | Countries | Same rule as trading: restricted countries can't use Circle |
| Q40 | Brokers | **One shared community** across Kalks and white-label brokers |
| Q44 | Broker privacy | The broker is never shown; IB/referral links and copy buttons work only between clients of the same broker |
| Q41 | Push | DMs & mentions, follows & likes (batched), followed traders' posts/stories (bell on), followed trader shares a trade |
| Q42 | Trade alerts | Only trades a trader chooses to share (nothing automatic) |
| Q43 | Gamification | Achievement badges and trader levels |
| Q45 | Copy trading | Follow ≠ copy; masters get a Copy button that opens the existing copy flow with its risk preview |
| Q46 | Academy | Mentor posts & lessons, a discussion thread per Academy chapter, certificate badges on profiles, user videos on any topic |
| Q47 | AI | Translate any post / comment (22 languages), per-symbol community sentiment, post helper (caption / explanation), daily feed digest |
| Q48 | Languages | One global feed; Translate button; the feed prefers the reader's language |
| Q50 | Release | **Everything at once** (one launch with all of the above except live rooms) |
| Q51 | Platforms | App and Client Area together on day one; public pages on the website |

## How it is built

**A new Rust service `services/circle`** (axum + sqlx like the others, port :8105, database `kalks_circle`,
Caddy routes `/circle/*` on app/trade and a WebSocket `/circle/stream`). It owns everything social; trading data
comes from the existing services, never copied:
- **Identity & badges:** client id from the gateway session (BFF), KYC status and country from the gateway
  (restricted countries refused), live-account and broker from the engine, master / PAMM status from the engine's
  social module, staff / mentor from gateway roles. The broker (tenant) is stored but never exposed; IB links and
  copy buttons check same-tenant.
- **Content:** profiles (handle, display name, bio, avatar, privacy, opt-in stats), follows + requests, posts
  (text, photo sets, chart snapshot, trade card, poll, video), comments (one-level threads, pin, author controls),
  reactions (like, bull, bear), reposts / quotes, saves + collections, stories (24 h, stickers, close friends,
  highlights), hashtags / cashtags / mentions with their own feeds.
- **Verified trade cards:** created from an engine deal or position (internal read API); the card stores the
  engine ids and a signed snapshot, shows live P&L from market-data while open, and offers "Copy this trade"
  (prefilled ticket in Kalks Trader web + app).
- **Chart snapshots:** Kalks Trader web + app export the chart (symbol, timeframe, drawings, image); tapping opens the
  same chart.
- **Media:** photos resized to 3 sizes (WebP); **video** uploaded resumably, transcoded by an ffmpeg worker to HLS
  (360p / 720p) with a poster frame, max 60 s for posts/stories and longer for topic videos (limit to decide). Files
  and voice notes in chats. Stored in S3-compatible object storage behind a CDN.
- **Chat:** DMs with a Requests inbox, private groups (≤ 100), public symbol rooms, masters' follower rooms; typing,
  read receipts, media, voice notes, trade cards; stored server-side, staff access only via a report or a legal
  request (audited). Realtime over the circle WebSocket (same ticket pattern as the support stream).
- **Feeds & discovery:** Following (newest first), For you (ranking = interest graph + engagement + verified
  performance + language + freshness, with spam / low-quality demotion), Explore (traders, creators, trending
  cashtags / hashtags, topic videos), leaderboards (top traders by verified risk-adjusted return, top creators).
- **Safety:** every upload checked before it is shown — text, images (Claude vision: nudity, scams, "guaranteed
  profit", P&L screenshots from other platforms), video (sampled frames + audio transcript); link allow-list;
  hidden words; block / mute / restrict; reports; automatic risk line on trading posts. Back Office › Circle:
  moderation queue, ban / shadow-hide, keyword & link rules, feature & pin, announcements, audit log.
- **Engagement:** notifications to the bell (support `/v1/notify`) and **push (FCM)** for the events in Q41, batched;
  achievement badges and trader levels; Rewards points for helpful posts; creator badge + Explore features;
  fee discounts for top creators (growth service).
- **AI (Claude, server key):** translate, per-symbol sentiment, post helper, daily digest, moderation.
- **Integrations:** Academy (mentor posts, chapter threads, certificate badges), copy trading (Copy button →
  existing flow), IB (link in bio, same broker only), Rewards (points), module switch `circle` (Track 2).
- **Clients:** Client Area (web) and the Android app get a Circle tab (feed, stories bar, explore, profile, chat,
  composer, story camera / editor, video player); public profile and post pages on the website (read-only, "Join
  Kalks Circle").

## What the founder needs to provide
1. **Object storage + CDN** for photos / video / files: Cloudflare R2 + its CDN recommended (no egress fees);
   or Backblaze B2 + Cloudflare. Bucket keys go in the server's `.env.local`.
2. **Firebase project** (FCM) for Android push: `google-services.json` + a server key.
3. **Video capacity:** the VPS transcodes with ffmpeg; heavy video use may need a bigger server or a managed
   transcoder (Cloudflare Stream / Mux, paid per minute). Decide when usage grows.
4. Community rules / terms text (a draft is written with the build) and the link allow-list.
5. Limits: max topic-video length (proposal: 10 minutes), upload sizes.

## Size
"Everything at once" is a large build: the service, moderation, media and video pipeline, chat, feeds and ranking,
two full client UIs (web + app), Back Office tools and push. Built in parallel milestones (service core → media &
video → chat → feeds & explore → safety & Back Office → web UI → app UI → AI & gamification), tested end to end on
demo, then launched in one release.

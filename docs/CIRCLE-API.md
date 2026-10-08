# Kalks Circle API

The contract of **Kalks Circle**, the trader community (spec and the founder's 51 decisions:
[`docs/social/KALKS-CIRCLE.md`](social/KALKS-CIRCLE.md)). The web (Client Area), the Android app and the Back Office
build on this document. Service: `services/circle` (Rust, axum + sqlx, `127.0.0.1:8105`, database `kalks_circle`,
created and migrated on first start).

Contents: [1 Architecture](#1-architecture) · [2 Conventions](#2-conventions) · [3 Objects](#3-objects) ·
[4 Me & profiles](#4-me-profiles-and-the-social-graph) · [5 Posts](#5-posts-comments-reactions-saves) ·
[6 Trade cards](#6-verified-trade-cards-and-copy-this-trade) · [7 Feeds & discovery](#7-feeds-and-discovery) ·
[8 Stories](#8-stories-and-highlights) · [9 Chat](#9-chat) · [10 Media uploads](#10-media-uploads) ·
[11 Realtime stream](#11-realtime-stream) · [12 Notifications](#12-notifications-activity-bell-push) ·
[13 Safety](#13-safety-moderation-and-reports) · [14 AI](#14-ai) · [15 Back Office](#15-back-office) ·
[16 Public pages](#16-public-pages-website) · [17 Configuration & deployment](#17-configuration-and-deployment) ·
[18 Error codes](#18-error-codes)

---

## 1. Architecture

```
Browser ── /api/circle/<route> ─────────┐                               ┌─ gateway   (modules, blocked countries)
App ────── /api/mobile/circle/<route> ──┤  Client Area BFF (apps/crm)   ├─ trading   (accounts, positions, deals, masters)
                                        └─ session → X-Kalks-* ──────▶ circle :8105 ─ market-data (quotes, instruments)
Browser/App ── wss://<app>/circle/stream?ticket= (Caddy) ────────────▶  │           ├─ academy   (certificates)
Browser/App ── PUT https://<app>/circle/upload/{id}?token= (Caddy) ──▶  │           ├─ support   (POST /v1/notify: the bell)
Media ──────── https://<app>/circle/media/<key> (Caddy, local disk) or the R2 CDN    ├─ growth    (Rewards points)
Back Office ── admin BFF (staff headers) ─────────────────────────────▶  │           ├─ Claude    (moderation, AI)
Website ────── https://api.kalkstrade.com/circle/public/* (Caddy) ────▶  ┘           └─ FCM       (push)
```

- **Client routes** are reached only through the BFF: `/api/circle/<route>` (web, cookie session) or
  `/api/mobile/circle/<route>` (app, `Authorization: Bearer <session token>`; the proxy rewrites it onto the same
  handler, see `docs/MOBILE-API.md`). The BFF maps `<route>` to the service's `/v1/circle/<route>`: **every route in
  §4–§14 is written below as its BFF path** (e.g. `GET /api/circle/feed/for-you` → service
  `GET /v1/circle/feed/for-you`). Query strings and JSON bodies pass through unchanged.
- **Identity**: the BFF resolves the gateway session and sends `X-Kalks-Internal`, `X-Kalks-User-Id`,
  `X-Kalks-Tenant` (broker), `X-Kalks-User-Name` (percent-encoded), `X-Kalks-Country`, `X-Kalks-Kyc`,
  `X-Kalks-Referral-Code`, `X-Kalks-Locale` (cookie `kalks_locale` / app header `X-Kalks-Locale`) and
  `X-Kalks-Ip-Country` (edge `CF-IPCountry`). Ids sent by the browser are never trusted.
- **One shared community across brokers** (Q40): members of Kalks and of white-label brokers see each other. The
  broker is stored but never shown (Q44). "Copy this trade", master Copy buttons and IB links work only between
  clients of the same broker.
- **Module switch `circle`** (Back Office › Owner › modules): off for a broker → its clients get
  `403 module_disabled` on every Circle route (the BFF proxy refuses `/circle*` and `/api/circle*` already; the
  service checks too). Their existing content stays visible to the rest of the community.
- **Restricted countries** (Q39): the registration country or the request's country in
  `CIRCLE_RESTRICTED_COUNTRIES`, in the Back Office list (settings) or in the broker's blocked countries (gateway)
  → `403 restricted_country`.
- **Who joins** (Q1): every client, demo too. KYC-verified members get the `verified` badge and more reach.
- First call of any client route creates the member's profile (handle suggested from the name, display name = first
  name). The UI shows the onboarding sheet (handle, rules) while `settings.onboarded` is false.

## 2. Conventions

- JSON in and out (`content-type: application/json`), camelCase keys, times ISO 8601 UTC, ids are integers
  (member ids = gateway user ids).
- Mutations (`POST/PATCH/PUT/DELETE`) through the web BFF must be same-origin; a body is always a JSON object
  (`{}` when empty), except upload chunks.
- **Errors**: `{"error": {"code": "...", "message": "...", "field": "..."?}}`. `message` is human-readable English
  (show it, or translate by `code`). Statuses: 400 bad_request, 401 unauthorized, 403 forbidden / specific codes,
  404 not_found, 409 conflict codes, 413 too_large, 415 unsupported_type, 422 validation / rule codes,
  429 rate_limited, 503 unavailable. Full list in §18.
- **Pagination**: `?cursor=<opaque>&limit=<n>` → `{"items": [...], "nextCursor": "..." | null}`. Pass
  `nextCursor` back unchanged; `null` = end.
- **Statuses of member content**: `pending` (being checked, visible to the author only) → `published` |
  `review` (held for a moderator, author only) | `rejected` (with `reason`, author only) | `removed` (by staff) |
  `deleted` (by the author). Others only ever receive `published` content; `status` / `reason` are present only on
  the author's own items.
- **Risk line** (Q36): items with a trade card, a $cashtag or a chart carry
  `riskLine: "Not investment advice. Trading involves risk."` (null otherwise). Show it small under the item;
  translate it with the key `circle.riskLine`.
- Badge keys, achievement keys and notification kinds are stable strings: translate them as
  `circle.badge.<key>`, `circle.achievement.<key>`, `circle.activity.<kind>`.

## 3. Objects

### 3.1 ProfileCard (embedded everywhere)

```json
{ "id": 101, "handle": "ben", "displayName": "Ben", "avatar": {"thumb": "/circle/media/m/2026/9f…/thumb.webp", "small": "…", "large": "…"} | null,
  "badges": ["mentor", "verified", "master", "pamm", "live", "creator", "academy"], "level": 4, "private": false }
```

Badges (Q5, Q32, Q46), in display order: `team` (Kalks team) · `mentor` · `verified` (KYC) · `master` (copy-trading
master) · `pamm` (PAMM manager) · `live` (has a live account) · `creator` · `academy` (holds an Academy certificate).
A deleted / unknown member renders as `{"id": 9, "handle": null, "displayName": "Kalks member", ...}`.

### 3.2 Profile (`GET /api/circle/profiles/{handle}`, `GET /api/circle/me`)

ProfileCard plus:

```json
{
  "bio": "Gold scalper. Not advice.", "cover": {"thumb": "…", "small": "…", "large": "…"} | null, "joinedAt": "2026-10-09T…",
  "counts": {"followers": 120, "following": 80, "posts": 42},
  "xp": 230, "levelProgress": {"level": 4, "xp": 230, "levelStart": 225, "nextLevel": 400},
  "achievements": [{"key": "first_post", "awardedAt": "…"}],
  "certificates": [{"code": "C1", "phase": "basics", "title": "Basics", "level": "beginner", "issuedAt": "…", "verifyUrl": "…"}],
  "stats": Stats | null, "showStats": true,
  "relationship": {"following": true, "requested": false, "followedBy": false, "blocked": false, "muted": false, "restricted": false, "closeFriend": false, "bell": false} | null,
  "isMe": false, "visible": true, "pinnedPost": 77 | null,
  "copy": {"masterId": 7, "program": "copy|pamm|both"} | null,
  "ibLink": {"referralCode": "ARJ42"} | null
}
```

- `visible: false` → a private profile the viewer doesn't follow (show the header and a Follow / Requested button,
  no posts).
- `copy`: the member is a live copy-trading master / PAMM manager **of the viewer's broker** → show the Copy button,
  which opens the existing copy flow `/social/masters/{masterId}` (web) / the app's master screen, with its risk
  preview (Q45). Following is not copying.
- `ibLink`: the member shows their referral link and the viewer is a client of the same broker → render
  `https://<app>/register?ref=<referralCode>` (Q32, Q44).
- On `isMe` (and `GET /me`) there is also `settings`:
  `{"showIbLink", "commentsDefault": "everyone|followers|off", "dmPolicy": "everyone|following|none", "lang", "onboarded", "hasReferralCode", "feeDiscountEligible", "warnings"}`.

**Stats** (Q6, opt-in, engine-verified from the member's live accounts, last 180 days of closed trades):

```json
{ "winRate": 61.5, "monthlyReturnPct": 4.2, "maxDrawdownPct": 8.1, "trades": 214, "returnPct90d": 9.8, "riskAdjusted": 1.21,
  "monthly": [{"month": "2026-05", "returnPct": 1.2, "trades": 30}, …6 months], "computedAt": "…", "verified": true }
```

Returns are closed-trade P&L over the balance (deposits / withdrawals are not performance).
`riskAdjusted` = 90-day return ÷ max(max drawdown, 1 %) ranks the traders leaderboard.

### 3.3 Post

```json
{
  "id": 501, "kind": "post|repost|quote|video", "author": ProfileCard, "body": "Long $XAUUSD above 2400 #gold", "lang": "en",
  "media": [Media], "tradeCard": TradeCard | null,
  "poll": {"options": ["Up", "Down"], "counts": [3, 1], "total": 4, "endsAt": "…", "closed": false, "myVote": 0 | null} | null,
  "repostOf": Post | {"id": 3, "unavailable": true} | null, "quoteOf": Post | {"id": 3, "unavailable": true} | null,
  "topic": "gold" | null, "academyChapter": "basics-leverage" | null, "visibility": "public|followers",
  "comments": {"mode": "everyone|followers|off", "pinned": 88 | null},
  "counts": {"likes": 12, "bulls": 7, "bears": 2, "comments": 4, "reposts": 1, "quotes": 0, "saves": 3, "views": 230},
  "viewer": {"liked": true, "vote": "bull|bear" | null, "saved": false, "reposted": false, "isAuthor": false},
  "hashtags": ["gold"], "cashtags": ["XAUUSD"], "mentions": [{"id": 102, "handle": "chen"}],
  "riskLine": "Not investment advice. Trading involves risk." | null,
  "helpful": false, "featured": false, "copyable": true,
  "createdAt": "…", "publishedAt": "…" | null, "editedAt": "…" | null, "url": "/circle/post/501",
  "status": "published", "reason": null          // author only
}
```

- Text: render `#tag` → hashtag feed, `$SYMBOL` → price chip (live price from `prices`, tap → cashtag page with
  a Trade button that opens Kalks Trader on the symbol), `@handle` → profile. Mentions resolved at publication
  are in `mentions`; other `@words` are plain text.
- `copyable`: the post has a trade card of another member of the viewer's broker → show "Copy this trade" (§6).
- `kind: repost` has no body; render `repostOf`. `quote` renders `body` + the quoted post.
- Bull / bear (Q21) exist only on posts with a cashtag or a trade card; `counts.bulls/bears` is the crowd sentiment.

### 3.4 Media

```json
{ "id": 61, "kind": "photo|video|voice|file|chart", "status": "ready", "width": 2048, "height": 1024, "durationMs": null,
  "urls": {"thumb": "…/thumb.webp", "small": "…/small.webp", "large": "…/large.webp"},
  "chart": {"symbol": "XAUUSD", "timeframe": "H1", "drawings": [...], "indicators": [...], "price": 2410.5} | null,
  "name": "plan.pdf", "size": 120334,        // file / voice only
  "reason": null, "purpose": "post"          // uploader only
}
```

`urls` by kind: photo / chart → `thumb` (320 px; avatars 96), `small` (1080; avatars 256), `large` (2048; avatars
512), all WebP; video → `hls` (`…/hls/master.m3u8`, 360p + 720p) and `poster` (WebP); voice → `audio` (`.m4a`
AAC, or the original format when the server has no ffmpeg); file → `file`. URLs are relative to the app host on
local storage (`/circle/media/...`) or absolute CDN URLs on R2: use them as they are. Chart snapshots: tapping
opens Kalks Trader on `chart.symbol` / `chart.timeframe` with `chart.drawings` (the format the terminal exported).

### 3.5 TradeCard

```json
{ "id": 31, "verified": true, "source": "position|deal", "state": "open|closed", "symbol": "XAUUSD", "side": "buy|sell",
  "accountType": "live|demo", "detail": "full|percent", "openTime": "…", "closeTime": "…" | null,
  "pnlPct": 0.42, "pips": 100.0, "option": false, "refreshedAt": "…",
  "volume": 0.5, "openPrice": 2400.0, "closePrice": null, "currentPrice": 2405.0, "sl": 2390.0, "tp": 2450.0, "profit": 247.0, "currency": "USD",
  "live": {"price": 2410.0 | null, "pnlPct": 0.42, "pips": 100.0} | null }
```

- `volume / openPrice / closePrice / currentPrice / sl / tp / profit / currency` are present only on `full` cards
  (and always for the owner). `percent` cards show `pnlPct` and `pips` only (Q10).
- `live` (open cards): the % / pips at the current market-data price (`price` only on full cards). Refresh the
  engine figures with `GET /api/circle/trade-cards/{id}/live` (≤ every 5 s per card).
- `verified: false` → the stored snapshot doesn't match its signature: render the card greyed "Unverified".
- `accountType: demo` → label the card "Demo".

### 3.6 Comment

```json
{ "id": 88, "postId": 501, "parentId": null | 80, "author": ProfileCard, "body": "Agree", "deleted": false, "lang": "en",
  "media": Media | null, "likes": 2, "replies": 3, "liked": false, "restricted": false, "createdAt": "…", "editedAt": null,
  "status": "published", "reason": null }      // author only
```

One level of threads: `parentId` is always a top-level comment (a reply to a reply is attached to its top-level
comment). A deleted comment with replies stays as a tombstone (`deleted: true`, empty body). `restricted: true` is
visible only to its author and the post's author.

### 3.7 Story

```json
{ "id": 9, "author": ProfileCard, "kind": "photo|video|chart|trade|text", "body": "Bull or bear?", "background": "#1a1a1a" | null,
  "media": Media | null, "tradeCard": TradeCard | null,
  "stickers": [{"type": "sentiment", "symbol": "XAUUSD", "question": "Bull or bear?", "x": 0.5, "y": 0.7},
               {"type": "question", "prompt": "Your target?", "x": 0.5, "y": 0.3},
               {"type": "cashtag", "symbol": "EURUSD", "x": 0.2, "y": 0.2}, {"type": "mention", "handle": "ana", "x": 0.8, "y": 0.9}],
  "sentiment": {"bulls": 12, "bears": 4, "myVote": "bull" | null}, "audience": "everyone|close_friends", "seen": false,
  "createdAt": "…", "expiresAt": "…", "riskLine": "…" | null,
  "status": "published", "reason": null, "views": 120 }    // author only
```

### 3.8 Conversation and Message

```json
Conversation {
  "id": 12, "kind": "dm|group|room|master_room", "title": "Ben" , "about": "", "avatar": Media | null,
  "symbol": "XAUUSD" | null, "slug": "gold" | null, "status": "active|locked|archived", "memberCount": 2,
  "other": ProfileCard | null, "otherOnline": true | null,                       // DMs
  "members": [{"user": ProfileCard, "role": "owner|admin|member", "state": "active|request", "lastReadId": 340}] | null,   // DMs and groups
  "me": {"role": "member", "state": "active|request", "muted": false, "unread": 2, "lastReadId": 338} | null,
  "lastMessage": Message | null, "lastMessageAt": "…" | null, "createdAt": "…" }

Message {
  "id": 340, "conversationId": 12, "sender": ProfileCard,
  "kind": "text|photo|video|voice|file|chart|trade_card|post|story_reply|system",
  "body": "Hi", "media": Media | null, "tradeCard": TradeCard | null, "postId": 501 | null,   // post id, or the story id for story_reply
  "replyTo": {"id": 330, "kind": "text", "preview": "…", "handle": "ana", "deleted": false} | null,
  "deleted": false, "createdAt": "…", "editedAt": null, "clientId": "c-1",
  "status": "sent|pending|hidden" }                                                        // sender only
```

Read receipts: a message is read by a member when their `lastReadId >= message.id` (DMs and groups; not in public
rooms).

### 3.9 Activity item (the Circle notifications list)

```json
{ "id": 77, "kind": "follow|follow_request|follow_accept|like|vote|comment|reply|comment_like|mention|repost|quote|dm_request|group_invite|post|story|trade|story_answer|achievement|helpful|moderation",
  "actor": ProfileCard | null, "postId": 501 | null, "commentId": null, "storyId": null, "conversationId": null,
  "data": {"preview": "…", "reaction": "like", "symbol": "XAUUSD", "side": "buy", "key": "first_post", "title": "…", "reason": "…", "points": 50}, "read": false, "createdAt": "…" }
```

## 4. Me, profiles and the social graph

| Method & path | Body / query | Answer |
|---|---|---|
| `GET /api/circle/me` | — | Profile (with `settings`) + `counters` {`activityUnread`, `followRequests`, `chatUnread`, `chatRequests`} + `banned` {`until`, `reason`} \| null + `limits` + `features` {`ai`, `push`, `video`, `storage`} + `riskLine`. Banned members get 200 here and `403 banned` everywhere else. |
| `PATCH /api/circle/me` | any of `handle`, `displayName` (1–40), `bio` (≤ 160, rules + AI checked), `avatarMediaId` / `coverMediaId` (a `ready` photo uploaded with purpose `avatar` / `cover`; `null` removes), `private`, `lang` (one of the 22), `showStats`, `showIbLink`, `commentsDefault`, `dmPolicy`, `onboarded: true`, `pinnedPostId` | `{"profile": Profile}`. Errors: `409 handle_taken`, `409 media_processing` / `media_review`, `422 media_rejected`, `422 validation`. Going public accepts all pending follow requests. `showStats: true` computes the stats in the background. |
| `GET /api/circle/handles/check?handle=` | — | `{"handle", "valid", "available", "message"}` |
| `GET /api/circle/profiles/{handle}` | `{handle}` = handle, `@handle` or the member id | `{"profile": Profile}`; 404 when blocked by them, banned or shadow-hidden |
| `GET /api/circle/profiles/{handle}/posts` | `?tab=posts\|media\|trades\|videos\|reposts&cursor&limit` | page of Post; first page of `posts` has `pinned: Post \| null`. `403 private_profile` |
| `GET /api/circle/profiles/{handle}/followers` · `/following` | `?cursor&limit` | page of ProfileCard + `following` (does the viewer follow them) |
| `GET /api/circle/profiles/{handle}/highlights` | — | `{"items": [{"id", "title", "cover": url \| null, "position", "count"}]}` |
| `POST /api/circle/profiles/{handle}/follow` | — | `{"status": "active\|requested", "relationship"}`. Private profiles → request (Q8). |
| `DELETE /api/circle/profiles/{handle}/follow` | — | unfollow / cancel the request → `{"relationship"}` |
| `POST /api/circle/profiles/{handle}/bell` | `{"on": true}` | notify me of every post / story (must follow; `409 not_following`) |
| `POST` · `DELETE /api/circle/profiles/{handle}/block` | — | block removes follows both ways and close friends; DMs refused; content hidden both ways (Q37) |
| `POST` · `DELETE /api/circle/profiles/{handle}/mute` | `{"posts": true, "stories": true}` | hidden from my feeds / story tray (they don't know) |
| `POST` · `DELETE /api/circle/profiles/{handle}/restrict` | — | their comments on my posts are visible only to them and me; no notifications from them |
| `POST` · `DELETE /api/circle/profiles/{handle}/close-friend` | — | for close-friends stories; must be my follower (`409 not_follower`) |
| `GET /api/circle/me/requests` | — | `{"items": [{"user": ProfileCard, "requestedAt"}]}` |
| `POST /api/circle/me/requests/{userId}/accept` · `/decline` | — | `{"status": "ok"}` |
| `POST /api/circle/me/followers/{userId}/remove` | — | remove a follower |
| `GET /api/circle/me/lists/{list}` | `blocked\|muted\|restricted\|close-friends` | `{"items": [ProfileCard]}` |
| `GET` · `PUT /api/circle/me/hidden-words` | PUT `{"words": ["pump", "free money"]}` (≤ 200, ≤ 60 chars) | `{"words": [...]}`. Posts and comments containing a hidden word (whole word / phrase, case-insensitive) are left out of my feeds and comment lists. |
| `GET /api/circle/me/stats` | — | `{"stats": Stats \| null, "showStats", "computedAt"}` |
| `POST /api/circle/me/stats/refresh` | — | recompute from the engine now (6 / hour) → `{"stats", "showStats"}` |
| `GET /api/circle/me/activity` | `?cursor&limit` | page of Activity + `unread` |
| `POST /api/circle/me/activity/read` | `{"ids": [77]}` or `{}` (all) | `{"unread"}` |
| `GET` · `PUT /api/circle/me/notification-prefs` | PUT `{"prefs": {"likes": {"bell": false, "push": true}}}` | `{"prefs": {"follows": {"label", "bell", "push", "locked"}, …}}` — groups: `follows, likes, comments, mentions, messages, posts, achievements, moderation` (moderation's bell is locked on) |
| `POST /api/circle/me/devices` | `{"token": "<FCM registration token>", "platform": "android\|ios\|web", "locale": "hi", "appVersion": "1.0.2"}` | register for push (call after sign-in and whenever FCM rotates the token) → `{"status": "ok", "push": configured}` |
| `DELETE /api/circle/me/devices` | `{"token": "…"}` | unregister (on sign-out) |

Levels and achievements (Q43): XP — published post +5, trade card shared +10, like / vote received +1, comment
received +2, new follower +3, helpful post +50; level = ⌊√(XP/25)⌋ + 1 (max 50). Achievement keys: `first_post`,
`first_trade_card`, `first_story`, `first_video`, `followers_10`, `followers_100`, `followers_1000`, `helpful_1`,
`helpful_10`, `verified`, `live_trader`, `academy_certificate`, `top_creator`, `level_10`.

## 5. Posts, comments, reactions, saves

| Method & path | Body / query | Answer |
|---|---|---|
| `POST /api/circle/posts` | see below | `{"post": Post}` with `status: "pending"` (or `"review"`) |
| `GET /api/circle/posts/{id}` | — | `{"post": Post, "comments": first 3 comments page}` |
| `PATCH /api/circle/posts/{id}` | `{"body"?, "comments"?: "everyone\|followers\|off"}` (author) | `{"post"}`; changed text is checked again (`pending` → `published`, no new notifications) |
| `DELETE /api/circle/posts/{id}` | — | author; `{"status": "deleted"}` (its media files are deleted) |
| `POST /api/circle/posts/{id}/react` | `{"kind": "like\|bull\|bear"}` | `{"counts": Counts}`; bull and bear are exclusive; bull / bear only on symbol posts (`422`) |
| `DELETE /api/circle/posts/{id}/react?kind=like` | — | `{"counts"}` |
| `GET /api/circle/posts/{id}/reactions?kind=like\|bull\|bear` | `cursor, limit` | page of ProfileCard |
| `POST` · `DELETE /api/circle/posts/{id}/repost` | — | `{"repostId", "counts"}` / `{"counts"}`; public posts only (`403 not_public`); twice → `409 already_reposted` |
| `POST /api/circle/posts/{id}/save` | `{"collectionId"?: 4}` | `{"saved": true, "counts"}` (moving between collections = save again) |
| `DELETE /api/circle/posts/{id}/save` | — | `{"saved": false, "counts"}` |
| `POST /api/circle/posts/{id}/vote` | `{"option": 0}` (0-based) | `{"poll"}`; once (`409 already_voted`), not after `endsAt` (`409 poll_closed`) |
| `POST` · `DELETE /api/circle/posts/{id}/pin` | — | pin on my profile → `{"pinnedPost"}` |
| `GET /api/circle/posts/{id}/copy` | — | §6 |
| `POST /api/circle/posts/{id}/translate` | `{"lang"?: "hi"}` (default: my language) | `{"translation": {"lang", "text"}}` (§14) |
| `POST /api/circle/posts/views` | `{"ids": [501, 502]}` (≤ 100) | `{"status": "ok"}` — send the ids that were on screen; counts views and pushes seen posts down in For you |
| `GET /api/circle/posts/{id}/comments` | `?parent=<commentId>&cursor&limit` | page of Comment (newest first); top level (no `parent`) first page also has `pinned: Comment \| null` and `mode` |
| `POST /api/circle/posts/{id}/comments` | `{"body": "…", "parentId"?: 88, "mediaId"?: 61}` (≤ 1000 chars; photo / chart uploaded with purpose `comment`) | `{"comment"}` (`pending`). `403 comments_off` / `comments_followers` |
| `PATCH /api/circle/comments/{id}` | `{"body"}` (author) | `{"comment"}` (checked again) |
| `DELETE /api/circle/comments/{id}` | — | the comment's author **or the post's author** |
| `POST` · `DELETE /api/circle/comments/{id}/like` | — | `{"likes", "liked"}` |
| `POST` · `DELETE /api/circle/posts/{id}/comments/{commentId}/pin` | — | post author pins one top-level comment |
| `POST /api/circle/comments/{id}/translate` | `{"lang"?}` | `{"translation"}` |
| `GET /api/circle/me/saved` | `?collection=<id>&cursor&limit` | page of Post (most recently saved first) |
| `GET` · `POST /api/circle/me/collections` | POST `{"name": "Gold ideas"}` (≤ 40, ≤ 100 collections) | `{"all": 12, "items": [{"id", "name", "posts", "createdAt"}]}` / `{"collection"}`; `409 exists` |
| `PATCH` · `DELETE /api/circle/me/collections/{id}` | PATCH `{"name"}` | deleting keeps the posts saved (in "All") |

**Create a post** (`POST /api/circle/posts`):

```json
{ "body": "Long $XAUUSD above 2400 #gold",          // ≤ 3000 chars
  "mediaIds": [61, 62],                               // ≤ 10 photos / chart snapshots, or exactly 1 video (uploaded with purpose "post", or "topic_video" with topic)
  "tradeCardId": 31,                                  // a card of my own (§6)
  "poll": {"options": ["Up", "Down", "Flat"], "durationHours": 24},   // 2–4 options ≤ 60 chars, 1–168 h
  "quoteOf": 480,                                     // quote another visible post
  "topic": "gold",                                    // topic video (needs one video uploaded with purpose "topic_video")
  "academyChapter": "basics-leverage",                // Academy chapter discussion (§7)
  "visibility": "public|followers", "comments": "everyone|followers|off", "lang": "hi" }
```

At least one of body / media / trade card / poll / quote. Rules applied at once: links outside the allow-list →
`422 link_not_allowed`; a blocking keyword → `422 content_blocked`; a review keyword → `status: "review"`; rejected
media → `422 media_rejected`; unfinished upload → `422 validation`. 20 posts / hour (settings). Media still
processing is fine: the post waits (`pending`) until its media passed the check. When published: mentions are
notified; a post with a trade card notifies every follower ("shared a trade", Q41/Q42), other posts the followers
with the bell on. The UI shows the author's pending post with a "Checking…" state and listens for `post.status`
(§11).

## 6. Verified trade cards and "Copy this trade"

| Method & path | Body | Answer |
|---|---|---|
| `GET /api/circle/trade-cards/sources` | — | `{"accounts": [{"login", "type": "live\|demo", "name", "positions": [{"ticket", "symbol", "side", "volume", "openPrice", "sl", "tp", "profit", "pnlPct", "openTime"}], "deals": [{"dealId", "symbol", "side", "volume", "openPrice", "closePrice", "profit", "pnlPct", "closeTime"}]}]}` — open positions and closing deals of the last 30 days of the member's own accounts (engine) |
| `POST /api/circle/trade-cards` | `{"login": 10000001, "ticket": 555}` (open position) or `{"login", "dealId": 900}` (closed trade), `"detail": "full\|percent"` (default percent) | `{"tradeCard": TradeCard}`; `422` when it isn't the member's account / position / closing deal; `503 unavailable` when the engine is down |
| `GET /api/circle/trade-cards/{id}` | — | `{"tradeCard": TradeCard + "copyable"}` (owner, or anyone who can see a post / story / chat message carrying it) |
| `GET /api/circle/trade-cards/{id}/live` | — | fresh engine figures (an open card becomes `closed` with the final result once the position is closed; SL / TP kept) |
| `GET /api/circle/posts/{id}/copy` | — | `{"copy": {"symbol": "XAUUSD", "side": "buy", "sl": 2390 \| null, "tp": 2450 \| null, "state": "open", "terminalPath": "/?symbol=XAUUSD&side=buy&sl=2390&tp=2450", "note": "Set your own size. Not investment advice."}}`; `403 other_broker`; `422` own trade / no card |

Profit claims are only possible this way (Q35): cards are made from engine data of the member's own account and
signed (HMAC); a P&L screenshot from another platform is detected and blocked (§13). "Copy this trade" (Q11) opens
a prefilled order ticket: web → Kalks Trader `terminal URL + terminalPath` (the terminal reads `symbol` and `side`;
`sl` / `tp` prefill the protection fields), app → the native order ticket with symbol, side, SL, TP; the reader sets
the size. SL / TP are given only on `full` cards.

## 7. Feeds and discovery

| Method & path | Query | Answer |
|---|---|---|
| `GET /api/circle/feed/following` | `cursor, limit (≤ 50)` | page of Post: my posts and the members I follow, newest first (Q17); muted members and hidden words left out |
| `GET /api/circle/feed/for-you` | `cursor, limit` | page of Post, ranked (below); first page also has `announcements` |
| `GET /api/circle/tags/{tag}` | `cursor, limit` | page of Post with `#tag`, newest first + `tag` {`tag`, `posts`} |
| `GET /api/circle/cashtags/{symbol}` | `cursor, limit` | page of Post with `$SYMBOL`; first page + `symbol` {`symbol`, `name`, `quote` {bid, ask, last, t, o, h, l}, `known`, `crowd` {bulls, bears, bullPct} (24 h), `roomId` (the symbol room, if any), `riskLine`} |
| `GET /api/circle/topics` | — | `{"items": [{"key", "title", "description", "videos"}], "maxVideoSecs": 600}` |
| `GET /api/circle/topics/{key}/videos` | `sort=new\|top, cursor, limit` | page of video Post + `topic` (Q16/Q49: any member can post videos to any topic) |
| `GET /api/circle/explore` | — | `{"announcements", "featuredPosts": [Post], "traders": [{"profile", "stats", "followers"}], "creators": [{"profile", "followers", "featured"}], "trendingHashtags": [{"tag", "posts", "score"}], "trendingCashtags": [{"tag", "posts", "score", "quote"}], "topics": [{"key", "title", "description", "videos", "latest": Post \| null}], "suggested": [ProfileCard]}` |
| `GET /api/circle/search` | `q, type=all\|users\|hashtags\|cashtags\|posts` | `{"users": [ProfileCard], "hashtags": [{"tag", "posts"}], "cashtags": [{"symbol", "name"}], "posts": [Post]}` (`@`, `#`, `$` prefixes are ignored) |
| `GET /api/circle/leaderboards/traders` | — | `{"board", "metric": "riskAdjusted", "items": [{"rank", "profile", "stats", "followers"}], "note"}` — opted-in public members with enough closed trades (Q20) |
| `GET /api/circle/leaderboards/creators` | — | `{"board", "metric": "engagement30d", "items": [{"rank", "profile", "followers", "featured"}]}` |
| `GET /api/circle/prices` | `symbols=XAUUSD,EURUSD` (≤ 50) | `{"quotes": {"XAUUSD": {"bid", "ask", "last", "t", "o", "h", "l"}}}` — price chips; for streaming chips use market-data's public stream (`config.urls.marketData`) |
| `GET /api/circle/announcements` | — | `{"items": [{"id", "title", "body", "link", "pinned", "startsAt"}]}` |
| `GET /api/circle/academy/{chapter}/thread` | `cursor, limit` | page of Post of that Academy chapter (mentors and the Kalks team first) + `chapter` {chapter, posts} (Q46). Post with `academyChapter` to join the thread. |
| `GET /api/circle/rules` | — | `{"rules": text, "allowedLinks": ["kalkstrade.com", …], "riskLine", "reportReasons": [...]}` — show in the onboarding sheet and the composer |

**For you ranking** (Q17, Q18, Q48), candidates = public / visible posts of the last 72 h (not mine, not reposts):
affinity (follow 1.0, followed-by-people-I-follow 0.35, + recent reactions to the author) + engagement (log of
likes, votes, 2×comments, 3×reposts/quotes, 2×saves) + the author's verified risk-adjusted return + quality
(media / trade card / substance) + my language + featured / helpful; × freshness (18-hour decay) × the KYC
`verifiedBoost`; − spam (reports, the author's warnings, empty posts); posts I've seen are pushed down; at most two
posts of one author in a row. The ranked list is kept 10 minutes, so paging is stable; pull-to-refresh = request
without `cursor`. A quiet start falls back to the latest public posts.

**Trending** hashtags / cashtags are recomputed every 5 minutes from the last 24 hours (decayed engagement).

## 8. Stories and highlights

| Method & path | Body / query | Answer |
|---|---|---|
| `POST /api/circle/stories` | `{"kind": "photo\|video\|chart\|trade\|text", "mediaId"? (purpose "story", kind must match), "tradeCardId"? (kind trade), "body"? (≤ 500; required for text), "background"?: "#1a1a1a", "stickers"?: [≤ 3, see 3.7: `sentiment` {symbol?, question?}, `question` {prompt}, `cashtag` {symbol}, `mention` {handle}; x / y 0–1], "audience": "everyone\|close_friends"}` | `{"story"}` (`pending`); lives 24 h after creation (Q14); 50 / day |
| `GET /api/circle/stories/tray` | — | `{"items": [{"author": ProfileCard, "latestAt", "count", "allSeen", "isMe"}]}` — me first, then followed members with live stories, unseen first |
| `GET /api/circle/stories/users/{handle}` | — | `{"author", "items": [Story]}` live stories I may see, oldest first |
| `GET /api/circle/stories/{id}` | — | `{"story"}` (live, or through one of the author's highlights) |
| `DELETE /api/circle/stories/{id}` | — | author |
| `POST /api/circle/stories/{id}/view` | — | mark seen (once per viewer) |
| `GET /api/circle/stories/{id}/viewers` | — | author: `{"count", "items": [{"user", "at", "vote"}]}` |
| `POST /api/circle/stories/{id}/vote` | `{"choice": "bull\|bear"}` | `{"sentiment"}`; once (`409 already_voted`) |
| `POST /api/circle/stories/{id}/answer` | `{"body"}` (≤ 300) | question sticker answer → the author's activity |
| `GET /api/circle/stories/{id}/answers` | — | author: `{"items": [{"id", "user", "body", "at"}]}` |
| `POST /api/circle/stories/{id}/reply` | `{"body", "clientId"?}` | replies in the author's DMs (a request if they don't follow me) → `{"conversationId", "message"}` (kind `story_reply`, `postId` = story id) |
| `GET /api/circle/me/stories/archive` | `cursor, limit` | all my stories, newest first |
| `POST /api/circle/highlights` | `{"title": "Gold calls" (≤ 30), "storyIds": [9, 10] (my published stories, any age), "coverStoryId"?}` | `{"highlight": {"id", "title", "cover", "position", "items": [Story]}}` (≤ 50 highlights) |
| `GET /api/circle/highlights/{id}` | — | `{"highlight", "owner"}` |
| `PATCH /api/circle/highlights/{id}` | `{"title"?, "storyIds"? (replaces), "coverStoryId"?, "position"?}` | `{"highlight"}` |
| `DELETE /api/circle/highlights/{id}` | — | |

Close-friends stories are shown only to the members on my close-friends list. Followers with the bell on get a push
for a new story.

## 9. Chat

| Method & path | Body / query | Answer |
|---|---|---|
| `GET /api/circle/chat/conversations` | `box=inbox\|requests\|rooms, cursor, limit` | `{"items": [Conversation], "nextCursor", "requests": <open requests>, "unread": <unread DM / group messages>}` |
| `POST /api/circle/chat/dm` | `{"handle": "ana"}` | `{"conversation"}` (existing or new). Strangers (the other member doesn't follow me) land in their **Requests** (Q25); their `dmPolicy`: `none` → `403 dms_off`, `following` → `403 dms_limited`; blocked → `403 blocked`; 50 new requests / day |
| `POST /api/circle/chat/groups` | `{"title" (≤ 60), "members": ["ben", "chen"]}` | `{"conversation"}`; ≤ 100 members (settings); members who don't follow me get the group as a request; 20 groups / day |
| `GET /api/circle/chat/rooms` | — | `{"symbolRooms": [{"id", "kind": "room", "title": "#gold", "about", "symbol", "slug", "memberCount", "joined", "lastMessageAt", "quote"}], "masterRooms": [{… "master": ProfileCard}], "paidRooms": false}` (masters' rooms of the traders I follow; no paid rooms, Q26) |
| `POST /api/circle/chat/master-room` | — | the master's own follower room (created once): `{"conversation"}`; `403 not_master` unless a live copy-trading master / PAMM manager |
| `GET /api/circle/chat/conversations/{id}` | — | `{"conversation"}` (public rooms can be previewed before joining) |
| `PATCH /api/circle/chat/conversations/{id}` | `{"title"?, "about"?, "avatarMediaId"?, "locked"?}` | group / master-room admins; broadcasts `conversation` |
| `GET /api/circle/chat/conversations/{id}/members` | `cursor, limit` | page of `{"id", "user", "role", "online"}` |
| `POST /api/circle/chat/conversations/{id}/members` | `{"members": ["dara"]}` | group admins → `{"added": [ids]}` |
| `DELETE /api/circle/chat/conversations/{id}/members/{userId}` | — | admins remove; anyone removes themselves |
| `POST /api/circle/chat/conversations/{id}/admins/{userId}` | — | owner makes an admin |
| `POST /api/circle/chat/conversations/{id}/accept` · `/decline` | decline: `{"block"?: true}` | answer a request (the sender isn't told about a decline) |
| `POST /api/circle/chat/conversations/{id}/join` · `/leave` | — | join a symbol room or a followed master's room (`403 follow_required`) / leave a group or room (a leaving group owner hands over) |
| `POST /api/circle/chat/conversations/{id}/mute` · `/unmute` | — | no push / unread badge |
| `POST /api/circle/chat/conversations/{id}/read` | `{"messageId"?}` (default: latest) | read receipt → stream `read` (DMs, groups) |
| `POST /api/circle/chat/conversations/{id}/typing` | — | stream `typing` (throttled 2 s; not in public rooms) |
| `GET /api/circle/chat/conversations/{id}/messages` | `before=<id>` (older, default) or `after=<id>` (newer), `limit ≤ 100` | `{"items": [Message], "more", "nextBefore" \| "nextAfter"}` (newest first for `before`, oldest first for `after`). Group members see the history from 7 days before joining; rooms and DMs everything. A public room not joined answers the latest messages with `"preview": true`. |
| `POST /api/circle/chat/conversations/{id}/messages` | `{"body"? (≤ 4000), "mediaId"? (purpose "chat": photo, video, voice, file, chart), "tradeCardId"? (mine), "postId"? (share a visible post), "replyTo"?, "clientId"? (idempotency, ≤ 64)}` | `{"message"}`; the same `clientId` returns the same message. Sending in a request accepts it. A message with media still being checked is `pending` and delivered when the media is ready. 30 / minute. `422 link_not_allowed` / `content_blocked`. |
| `PATCH /api/circle/chat/messages/{id}` | `{"body"}` | sender, within 15 minutes (`409 edit_window`) |
| `DELETE /api/circle/chat/messages/{id}` | — | sender or a group admin; shown as deleted to everyone (stored for compliance, Q28) |

Push / bell: a new DM → push (batched per conversation for a few seconds; not pushed while I'm connected to the
stream), a first message request → bell + push, being added to a group → bell + push; public rooms don't push.
Text is AI-checked right after delivery; a message that breaks the rules is hidden for everyone (stream
`message.hidden`) and the sender is told.

## 10. Media uploads

1. **Declare**: `POST /api/circle/uploads`
   `{"kind": "photo|video|voice|file|chart", "purpose": "post|story|topic_video|chat|avatar|cover|comment", "mime": "image/jpeg", "size": 2304511, "name": "IMG_1.jpg", "chart"?: {"symbol": "XAUUSD", "timeframe": "H1", "drawings": [...], "indicators": [...], "price": 2410.5}}`
   → `{"upload": {"id": 61, "token": "<48 hex, one-time>", "chunkSize": 8388608, "size", "offset": 0, "directUrl": "/circle/upload/61", "expiresIn": 86400}}`.
2. **Send the bytes in order**, chunks of at most `chunkSize`, each with header `Upload-Offset: <byte offset>`:
   - small files (photos, voice, files): `PUT /api/circle/uploads/{id}` through the BFF (raw body);
   - videos: `PUT <directUrl>?token=<token>` straight to the service through Caddy (no Next.js in between; the app
     uses `config.urls.uploads.circle + "/" + id`).
   Answer: `{"id", "offset", "size", "complete", "status": "uploading|processing"}`. A wrong offset → `409
   offset_mismatch` (message has the expected offset). **Resume** after a dropped connection: `GET
   /api/circle/uploads/{id}` (`offset`) or `HEAD <directUrl>?token=` (`Upload-Offset` / `Upload-Length` headers),
   then continue from that offset. Uploads not completed within 24 h expire.
3. **Processing** starts at the last byte: `GET /api/circle/uploads/{id}` → `{"id", "status", "offset", "size", "media": Media}`; the stream sends `media` frames. Statuses: `processing` → `ready` | `review` (a moderator decides; usable in a post, which then waits) | `rejected` (`media.reason`, can't be attached) | `failed` (`media.reason`, e.g. unreadable, too long).
4. **Attach** by id (`mediaIds`, `mediaId`, `avatarMediaId`). Unattached uploads are deleted after 7 days.
`GET /api/circle/media/{id}` → `{"media"}` (others only see `ready` media).

| Kind | Types | Max size (env) | Processing |
|---|---|---|---|
| photo, chart | JPEG, PNG, WebP, GIF (first frame) | 20 MB (`CIRCLE_MAX_PHOTO_MB`) | EXIF orientation applied, metadata dropped, 3 WebP sizes; AI check (vision) |
| video | MP4, MOV, WebM, MKV, 3GP | 500 MB (`CIRCLE_MAX_VIDEO_MB`) | ffmpeg → HLS 360p + 720p (shorter side, never upscaled) + poster; ≤ 60 s for posts / stories / chat (`CIRCLE_MAX_VIDEO_SECS`), ≤ 10 min for topic videos (`CIRCLE_MAX_TOPIC_VIDEO_SECS`); AI check of 4–8 sampled frames (the audio is not transcribed: no speech-to-text on the server) |
| voice | AAC/M4A, Ogg/Opus, MP3, WebM | 10 MB, ≤ 300 s | ffmpeg → mono AAC `.m4a`; not AI-checked (chat reports cover it) |
| file (chat) | pdf, txt, csv, docx, xlsx, pptx, png, jpg, webp | 25 MB (`CIRCLE_MAX_FILE_MB`) | content sniffed against the extension; PDFs / texts / images AI-checked; downloaded as attachments |

Without ffmpeg on the server, video uploads end `failed` ("Video uploads aren't available yet") and voice notes are
kept in their original format.

## 11. Realtime stream

1. `POST /api/circle/stream-ticket` → `{"ticket", "url", "expiresIn": 30}`. `url` is null in production: connect to
   `wss://<app host>/circle/stream` (web) or `config.urls.streams.circle` (app).
2. Open `<url>?ticket=<ticket>` within 30 s (one-time). On close, get a new ticket and reconnect (back-off); on
   `resync` reload the open screens.

Server → client frames (JSON, field `type`):

| type | Fields | When |
|---|---|---|
| `hello` | `who: "user"`, `activityUnread`, `conversations` | on connect |
| `ping` | — | every 25 s |
| `resync` | `missed` | the socket fell behind: reload |
| `message` | `conversationId`, `message` (Message; `status` omitted) | a message delivered in one of my conversations (I'm a member, active or request) |
| `message.updated` | `conversationId`, `message` \| `messageId` + `status: "hidden"` | edited / a pending message was refused |
| `message.deleted` | `conversationId`, `messageId` | deleted by its sender / an admin |
| `message.hidden` | `conversationId`, `messageId` | removed by moderation |
| `typing` | `conversationId`, `user` {id, handle, displayName} | someone else is typing (DMs, groups) |
| `read` | `conversationId`, `userId`, `messageId` | read receipt |
| `request.accepted` | `conversationId`, `userId` | my DM request was accepted |
| `conversation` | `conversation` | group details changed |
| `conversation.joined` · `conversation.left` | `conversationId`, `state`? | membership changed (the socket follows it) |
| `activity` | `item` (Activity), `unread` | a new notification for the Circle bell |
| `activity.read` | `unread` | read on another device |
| `media` | `media` (Media, own) | an upload finished processing |
| `post.status` · `comment.status` · `story.status` | `postId` / `commentId` / `storyId` / `id`, `status`, `reason`? | my content was published / held / rejected |

Client → server: `{"type": "ping"}`, `{"type": "typing", "conversationId"}`, `{"type": "read", "conversationId",
"messageId"}` (same as the HTTP routes).

## 12. Notifications (activity, bell, push)

Every event lands in the Circle activity list (`GET /api/circle/me/activity`, stream `activity`). Per kind and the
member's preferences it also goes to **the bell** (support service `POST /v1/notify`, type `circle.<kind>`,
in-app only, `link` = a Client Area path such as `/circle/post/501`) and as a **push** (FCM, data `{kind, link}`,
Android channel `circle`):

| Event | Activity | Bell | Push |
|---|---|---|---|
| new follower, follow request, request accepted | ✓ | ✓ | ✓ |
| likes / bull-bear votes on my post (batched: one per post per 10 min, "Ana and 12 others reacted to your post") | ✓ | ✓ | ✓ |
| comment on my post, reply to my comment, mention | ✓ | ✓ | ✓ |
| comment like (batched) | ✓ | ✓ | — |
| DM | — (chat unread) | — | ✓ (batched, not while connected) |
| message request, added to a group | ✓ | ✓ | ✓ |
| a followed trader (bell on) posts | ✓ | ✓ | ✓ |
| a followed trader (bell on) adds a story | ✓ | — | ✓ |
| a followed trader shares a trade (every follower) | ✓ | ✓ | ✓ |
| repost, quote, story answer | ✓ | — | — |
| achievement, helpful post (+ Rewards points) | ✓ | ✓ | ✓ |
| moderation decision (post / comment / story rejected or removed, warning, suspension) | ✓ | ✓ (always) | — |

Links the UI must route: `/circle/post/{id}`, `/circle/@{handle}`, `/circle/requests`, `/circle/chat/{id}`,
`/circle/chat?box=requests`, `/circle/stories/@{handle}`, `/circle/story/{id}`, `/circle/me`, `/circle/rules`.
Push needs `FCM_SERVICE_ACCOUNT_FILE` (and the app's `google-services.json`); until then pushes are skipped.

## 13. Safety: moderation and reports

- **Before anything is shown** (Q33): new and edited posts, comments, stories, bios and every upload are checked —
  rules first (links, keywords), then Claude (text; images by vision; video by sampled frames; PDFs). Decisions:
  allow → published; review → held for the staff queue (invisible to others); block → rejected with a reason. Chat
  text is delivered at once and checked right after (hidden if it breaks the rules); chat media is delivered only
  after its check.
- **Detected**: nudity / sexual content, violence, hate / harassment, scams (money, passwords, account
  management, impersonating Kalks), guaranteed / risk-free profit promises, **P&L or balance screenshots from other
  platforms** (Q35: profit claims only through verified trade cards), off-platform solicitation (Telegram / WhatsApp
  signals), spam, personal data, self-harm, illegal goods.
- **Links** (Q34): only the allow-list (seeded: kalkstrade.com, kalks.com, youtube.com, youtu.be,
  tradingview.com; staff edit it) — anything else is refused with `422 link_not_allowed` in posts, comments, bios,
  stories and chat.
- **Without an AI key**: text passes on the rules; photos / videos follow `CIRCLE_MODERATION_FALLBACK` (`review` =
  every upload waits for a moderator, the default; `publish`).
- **User controls** (Q37): block, mute, restrict, hidden words, close friends (§4); report anything:

`POST /api/circle/reports` `{"targetKind": "post|comment|story|message|profile|conversation", "targetId": 501, "reason": "spam|scam|abuse|hate|nudity|violence|pnl_claim|impersonation|personal_data|self_harm|other", "note"?: "…"}`
→ `{"status": "received", "reportId", "message"}`. One open report per member and target; 30 / hour. A post with 3
reports is held until a moderator looks at it. A reported chat message lets compliance open that conversation (§15).

Staff actions: remove, warn, suspend (ban, with an end date or permanent), shadow-hide (the member keeps posting,
nobody else sees it), feature, mark helpful (Q38).

## 14. AI

| Method & path | Body / query | Answer |
|---|---|---|
| `POST /api/circle/posts/{id}/translate` · `/api/circle/comments/{id}/translate` | `{"lang"?}` | `{"translation": {"lang", "text"}}` — 22 languages, cached; 120 / hour; `503 unavailable` without AI (hide the Translate button when `features.ai` is false). Show the button when `post.lang` ≠ the reader's language. |
| `GET /api/circle/ai/sentiment/{symbol}` | — | `{"symbol", "crowd": {"bulls", "bears", "posts24h", "bullPct"}, "summary": {"mood": "bullish\|bearish\|mixed\|neutral", "summary", "points": [...]} \| null, "riskLine"}` — crowd votes always; the AI summary with ≥ 3 posts in 24 h (cached hourly per symbol) |
| `POST /api/circle/ai/caption` | `{"tradeCardId"?, "mediaId"? (a chart snapshot), "notes"?, "lang"?}` | post helper (Q47): `{"suggestion": {"caption", "explanation", "hashtags"}, "riskLine"}` — the member edits it before posting; 20 / hour |
| `GET /api/circle/ai/digest` | — | `{"digest": {"headline", "items": [{"postId", "line"}]} \| null, "trending": [...], "reason"?: "quiet\|ai_unavailable"}` — the member's daily digest of the people they follow (cached per day) |

Claude: model `CIRCLE_AI_MODEL` (default `claude-opus-5-5`, moderation `CIRCLE_MODERATION_MODEL`), JSON-schema
constrained answers, effort `low`, server-side refusal fallback; a refused classification goes to the staff queue.
The AI never gives advice or predictions (system prompts) and only sees the content being checked / summarised.

## 15. Back Office

Service routes `/v1/circle/admin/*` (no BFF prefix rewriting: the admin BFF calls them directly) with
`X-Kalks-Internal: $CIRCLE_INTERNAL_TOKEN`, `X-Kalks-Tenant: <staff's broker>`, `X-Kalks-Staff-Id`,
`X-Kalks-Staff-Name` (percent-encoded), `X-Kalks-Staff-Role`, `X-Kalks-Staff-Perms` (comma list from the gateway
RBAC, as for the support service). Permissions (gateway module `circle`): `circle.read` (view), `circle.moderate`
(remove / warn / ban / shadow / resolve), `circle.content` (feature, announcements, topics, badges),
`circle.admin` (rules, settings, legal chat requests, audit, outbox), `circle.chat_access` (open a reported chat).
Presets: admins all; compliance read + moderate + chat_access; support read + moderate; marketing read + content;
viewer read. **Scope**: staff of the platform broker (`CIRCLE_PLATFORM_TENANT`, default `kalks`) and the Platform
Owner act on the whole community; other brokers' staff see and act on their own clients only (others → 404).
Staff stream: `POST /v1/stream/ticket` with the staff headers (needs `circle.read`) → frames `hello` {queue},
`queue` {kind, id}, `report` {kind, id}.

| Method & path | Perm | Body / query → answer |
|---|---|---|
| `GET /v1/circle/admin/overview` | read | `{queue, reports, posts24h, active24h, members, banned, shadowHidden, rejected24h, outboxFailed, mediaProcessing, scope, ai, storage, push}` |
| `GET /v1/circle/admin/queue` | read | `?status=open\|approved\|removed\|dismissed&kind=post\|comment\|story\|message\|media\|profile&cursor&limit` → page of `{id, targetKind, targetId, owner, source: ai\|rules\|report\|fallback, categories, verdict, excerpt, status, decidedBy, note, createdAt, decidedAt}`. AI-blocked items are listed as `removed` (decided by `ai`) for appeals. |
| `GET /v1/circle/admin/queue/{id}` | read | the item + `target` (the content, media visible whatever its status; messages only through chat access) + `reports` |
| `POST /v1/circle/admin/queue/{id}/approve\|remove\|dismiss` | moderate | `{note?, reason? (shown to the member), warn?: true}` → `{status}`; closes the target's open reports |
| `GET /v1/circle/admin/reports` | read | `?status=open\|actioned\|dismissed` → page of `{id, reporter, targetKind, targetId, targetOwner, reason, note, status, resolution, createdAt}` |
| `POST /v1/circle/admin/reports/{id}/resolve` | moderate | `{action: dismiss\|remove\|warn\|ban\|shadow, reason?, days?, note?}` |
| `GET /v1/circle/admin/content/{kind}/{id}` | read | post / comment / story / media (not messages: `403`) |
| `POST /v1/circle/admin/content/{kind}/{id}/{action}` | moderate (content for feature) | `remove\|restore` (post, comment, story, message, media), `feature {hours}\|unfeature\|helpful\|shadow\|unshadow` (posts) |
| `GET /v1/circle/admin/users` | read | `?q=<handle / name / id>&status=active\|banned\|shadow&cursor` → page of member (card + status, bannedUntil, banReason, shadowHidden, warnings, staffBadge, creator, feeDiscount, kycStatus, tenant, counts, joinedAt) |
| `GET /v1/circle/admin/users/{userId}` | read | + bio, stats, sanctions, reportsAgainst, recentPosts |
| `POST /v1/circle/admin/users/{userId}/warn\|ban\|unban\|shadow\|unshadow` | moderate | `{reason (required for warn / ban), days?}` |
| `POST /v1/circle/admin/users/{userId}/badge` | content | `{badge: "team"\|"mentor"\|null}` (Kalks team / mentor, Q5) |
| `POST /v1/circle/admin/users/{userId}/creator` · `/fee-discount` | content · admin | `{on}` · `{eligible}` |
| `GET` · `POST /v1/circle/admin/rules` | read · admin | `{kind: keyword\|link_allow, pattern, action: block\|review (keywords), note}`; `PATCH /v1/circle/admin/rules/{id}` `{active, action, note}`; `DELETE` |
| `GET` · `POST /v1/circle/admin/announcements` | read · content | `{title, body, link (app path or https), pinned, startsAt, endsAt}`; `PATCH`, `DELETE` (= end now) `/v1/circle/admin/announcements/{id}` |
| `GET` · `POST /v1/circle/admin/topics` | read · content | upsert `{key, title, description, position, active}` |
| `GET` · `POST /v1/circle/admin/features` · `DELETE /v1/circle/admin/features/{userId}` | read · content | featured creators on Explore `{userId, days?}` |
| `GET /v1/circle/admin/creators` | read | top 100 creators (30 days) with fee-discount flags + `feeDiscountTop` |
| `GET` · `PUT /v1/circle/admin/settings` | read · admin | `{settings: {restrictedCountries, helpfulThreshold, helpfulPoints, helpfulDailyCap, creatorMinFollowers, feeDiscountTop, leaderboardMinTrades, verifiedBoost, postsPerHour, messagesPerMinute, maxGroupMembers (≤ 100), dmRequestsPerDay, rulesText}, env: {...read-only}}`; PUT merges |
| `GET` · `POST /v1/circle/admin/chat-access` | chat_access | grant `{conversationId, basis: "report", reportId, reason}` or `{conversationId, basis: "legal", legalReference, reason}` (legal also needs `circle.admin`), `hours` 1–72 (default 24) → `{grant}`; the report must be about that conversation or one of its messages (90 days) |
| `POST /v1/circle/admin/chat-access/{id}/revoke` | chat_access | |
| `GET /v1/circle/admin/conversations/{id}/messages` | chat_access + an active grant of this staff member | `?before&limit` → `{conversation, grant, items: [{id, sender, kind, body, media, status, moderation, createdAt, editedAt, deletedAt}]}` (hidden and deleted messages included); **every read is audited** (Q28) |
| `GET /v1/circle/admin/audit` | admin | `?action=<prefix>&cursor` → page of `{id, actor, actorName, action, target, before, after, note, at}` (append-only) |
| `GET /v1/circle/admin/outbox` · `POST /v1/circle/admin/outbox/{id}/retry` | admin | Rewards points deliveries `{id, kind, userId, tenant, payload, status: pending\|delivered\|failed, attempts, error}` |

## 16. Public pages (website)

No token, public content of public profiles only (Q3, Q51); the broker, relationships and private data are never
returned. Through Caddy at `https://api.kalkstrade.com/circle/public/*` (→ service `/v1/public/*`):

- `GET /circle/public/profiles/{handle}` → `{"profile": ProfileCard + bio, cover, counts, stats (if shown), joinedAt, url, join: {title: "Join Kalks Circle", url: "/register"}}` (private profiles: header only)
- `GET /circle/public/profiles/{handle}/posts?cursor&limit` → page of Post (without `viewer`, `copyable`); `private: true` for private profiles
- `GET /circle/public/posts/{id}` → `{"post", "join"}`; 404 unless public

Media URLs on local storage are app-host relative: the website prefixes them with `https://app.kalkstrade.com`
(also served at `https://api.kalkstrade.com/circle/media/*`).

## 17. Configuration and deployment

Service env (repo-root `.env.local` in development; `deploy/deploy.sh` generates the secrets on the server):

| Variable | Default | |
|---|---|---|
| `CIRCLE_BIND` | `127.0.0.1:8105` | |
| `CIRCLE_DATABASE_URL` | `postgres://postgres@127.0.0.1:5433/kalks_circle` | created + migrated on first start |
| `CIRCLE_INTERNAL_TOKEN` | — (required in production) | BFFs send it as `X-Kalks-Internal` |
| `CIRCLE_ENV` | `development` | `production` enforces the token |
| `CIRCLE_CARD_SECRET` | the internal token | HMAC key of trade-card snapshots |
| `CIRCLE_STORAGE` | `local` | `local` or `s3` |
| `CIRCLE_MEDIA_DIR` · `CIRCLE_MEDIA_URL` | `~/.kalks-data/circle/media` · `/circle/media` | server: `/srv/kalks/circle-media` (Caddy serves it) |
| `CIRCLE_WORK_DIR` | `~/.kalks-data/circle/work` | upload parts, transcoding |
| `CIRCLE_S3_ENDPOINT`, `CIRCLE_S3_BUCKET`, `CIRCLE_S3_REGION` (`auto`), `CIRCLE_S3_ACCESS_KEY`, `CIRCLE_S3_SECRET_KEY`, `CIRCLE_S3_PUBLIC_URL` | — | Cloudflare R2: endpoint `https://<account>.r2.cloudflarestorage.com`, public URL = the bucket's CDN domain. Switching from local: copy `/srv/kalks/circle-media/m` to the bucket root (same keys), set the variables, restart. |
| `CIRCLE_FFMPEG`, `CIRCLE_FFPROBE` | `ffmpeg`, `ffprobe` | server: `apt install ffmpeg` |
| `CIRCLE_MAX_PHOTO_MB` 20 · `CIRCLE_MAX_VIDEO_MB` 500 · `CIRCLE_MAX_VOICE_MB` 10 · `CIRCLE_MAX_FILE_MB` 25 · `CIRCLE_CHUNK_MB` 8 | | upload limits |
| `CIRCLE_MAX_VIDEO_SECS` 60 · `CIRCLE_MAX_TOPIC_VIDEO_SECS` 600 · `CIRCLE_MAX_VOICE_SECS` 300 | | length limits |
| `ANTHROPIC_API_KEY` (`.env.claude`), `ANTHROPIC_API_URL` | | Claude |
| `CIRCLE_AI_MODEL`, `CIRCLE_MODERATION_MODEL` | `claude-opus-5-5` | a cheaper model can be set for moderation |
| `CIRCLE_MODERATION_FALLBACK` | `review` | images / video without AI: `review` or `publish` |
| `CIRCLE_RESTRICTED_COUNTRIES` | `kp,ir,sy,cu` | plus the Back Office list and the broker's blocked countries |
| `CIRCLE_PLATFORM_TENANT` | `kalks` | staff of this broker moderate the whole community |
| `FCM_SERVICE_ACCOUNT_FILE`, `FCM_PROJECT_ID`, `FCM_API_URL` | — | push (Firebase service-account JSON path; project from the file) |
| `GATEWAY_URL/_INTERNAL_TOKEN`, `TRADING_URL/_INTERNAL_TOKEN`, `MARKET_DATA_URL`, `ACADEMY_URL/_INTERNAL_TOKEN`, `GROWTH_URL/_INTERNAL_TOKEN`, `SUPPORT_URL` + `SUPPORT_INTERNAL_TOKEN` | local ports | upstreams |
| `CIRCLE_WORKERS` | `true` | background loops |

Client Area env: `CIRCLE_URL`, `CIRCLE_INTERNAL_TOKEN`, optional `CIRCLE_STREAM_URL`, `CIRCLE_UPLOAD_URL`,
`MOBILE_CIRCLE_STREAM_URL`, `MOBILE_CIRCLE_UPLOAD_URL`. The app reads `config.urls.streams.circle` and
`config.urls.uploads.circle` from `GET /api/mobile/config`.

Edge (`deploy/Caddyfile`, snippet `circle` on the app and trade hosts): `/circle/stream` → `:8105/v1/stream`,
`/circle/upload/*` → `:8105/v1/upload/*` (9 MB bodies), `/circle/media/*` → `/srv/kalks/circle-media` (immutable
cache, sandbox CSP, nosniff, attachments for files, HLS content types); api host: `/circle/public/*`,
`/circle/media/*`. Unit `deploy/systemd/kalks-circle.service`; `deploy/deploy.sh` builds, provisions secrets and
directories, installs ffmpeg when possible, restarts and health-checks `:8105/health`
(`{status, db, service, ai, storage, ffmpeg, push}`); the watchdog (`deploy/healthcheck.sh`) restarts it after 3
failed minutes.

Background work (in the service): media processing (one item at a time), safety checks (every 3 s and on demand),
bell / push / Rewards delivery with retries, trending (5 min), verified stats of opted-in members (every 6 h),
creator badges and fee-discount eligibility (hourly; `GET /v1/internal/creators/fee-discounts` lists eligible
members for the fee side), clean-up (daily).

## 18. Error codes

| Status | `code` | Meaning |
|---|---|---|
| 400 | `bad_request` | malformed JSON / headers |
| 401 | `unauthorized` | no session (BFF) / no identity headers (service) |
| 403 | `forbidden` | not allowed (role, not the author, …) |
| 403 | `module_disabled` | Kalks Circle is off for the member's broker |
| 403 | `restricted_country` | not available in the member's country |
| 403 | `banned` | the member is suspended (`GET /me` shows until / reason) |
| 403 | `private_profile` | follow to see this profile's posts / lists |
| 403 | `comments_off` · `comments_followers` | the author limited comments |
| 403 | `blocked` · `dms_off` · `dms_limited` | can't message this member |
| 403 | `locked` | the group / room is locked by its admins |
| 403 | `follow_required` · `not_master` · `not_public` · `other_broker` | room / repost / copy conditions |
| 404 | `not_found` | doesn't exist or isn't visible to you (blocked, private, held) |
| 409 | `handle_taken` · `exists` | |
| 409 | `already_reposted` · `already_voted` · `poll_closed` · `already_helpful` | |
| 409 | `not_following` · `not_follower` · `not_published` | |
| 409 | `media_processing` · `media_review` | the photo isn't ready yet |
| 409 | `offset_mismatch` · `upload_complete` · `upload_expired` | resumable upload state |
| 409 | `edit_window` | chat messages can be edited for 15 minutes |
| 413 | `too_large` | file / chunk / body too large |
| 415 | `unsupported_type` | media type not accepted |
| 422 | `validation` (+ `field`) | invalid input |
| 422 | `link_not_allowed` · `content_blocked` · `media_rejected` | content rules (show the message) |
| 429 | `rate_limited` | slow down |
| 503 | `unavailable` | an upstream (engine, AI) is unavailable |

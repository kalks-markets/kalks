-- Kalks Circle: the trader community (docs/social/KALKS-CIRCLE.md, API in docs/CIRCLE-API.md).
-- One shared community across brokers: `tenant` is stored for module switches, notifications, IB links and copy
-- buttons (same broker only) and is never returned to other clients.

-- ---------------------------------------------------------------- profiles & graph

CREATE TABLE profiles (
    user_id           BIGINT PRIMARY KEY,              -- gateway user id (global)
    tenant            TEXT NOT NULL,                   -- the broker: private
    handle            TEXT NOT NULL,                   -- lower-case [a-z0-9_.], 3..24
    display_name      TEXT NOT NULL,
    bio               TEXT NOT NULL DEFAULT '',
    avatar_media      BIGINT,
    avatar            JSONB,                           -- variant -> storage key (denormalised for cards)
    cover_media       BIGINT,
    cover             JSONB,
    private           BOOLEAN NOT NULL DEFAULT false,
    lang              TEXT NOT NULL DEFAULT 'en',
    country           TEXT NOT NULL DEFAULT '',        -- private (restricted-country rule)
    kyc_status        TEXT NOT NULL DEFAULT 'unverified',
    live_trader       BOOLEAN NOT NULL DEFAULT false,
    master_id         BIGINT,                          -- engine social master id (same broker copy button)
    master_program    TEXT,                            -- copy | pamm | both
    staff_badge       TEXT,                            -- team | mentor
    creator           BOOLEAN NOT NULL DEFAULT false,
    certificates      JSONB NOT NULL DEFAULT '[]',
    badges_at         TIMESTAMPTZ,
    show_stats        BOOLEAN NOT NULL DEFAULT false,
    stats             JSONB NOT NULL DEFAULT '{}',
    stats_at          TIMESTAMPTZ,
    risk_score        DOUBLE PRECISION,                -- verified risk-adjusted return (leaderboards, ranking)
    referral_code     TEXT,                            -- IB link in bio (same broker only)
    show_ib_link      BOOLEAN NOT NULL DEFAULT false,
    comments_default  TEXT NOT NULL DEFAULT 'everyone',-- everyone | followers | off
    dm_policy         TEXT NOT NULL DEFAULT 'everyone',-- everyone (strangers -> Requests) | following | none
    xp                BIGINT NOT NULL DEFAULT 0,
    level             INT NOT NULL DEFAULT 1,
    status            TEXT NOT NULL DEFAULT 'active',  -- active | banned
    banned_until      TIMESTAMPTZ,
    ban_reason        TEXT,
    shadow_hidden     BOOLEAN NOT NULL DEFAULT false,
    warnings          INT NOT NULL DEFAULT 0,
    fee_discount      BOOLEAN NOT NULL DEFAULT false,
    onboarded         BOOLEAN NOT NULL DEFAULT false,
    followers_count   BIGINT NOT NULL DEFAULT 0,
    following_count   BIGINT NOT NULL DEFAULT 0,
    posts_count       BIGINT NOT NULL DEFAULT 0,
    pinned_post       BIGINT,
    last_seen_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX profiles_handle ON profiles (handle);
CREATE INDEX profiles_tenant ON profiles (tenant);
CREATE INDEX profiles_risk ON profiles (risk_score DESC NULLS LAST) WHERE show_stats;
CREATE INDEX profiles_name_prefix ON profiles (lower(display_name) text_pattern_ops);

CREATE TABLE follows (
    follower    BIGINT NOT NULL,
    followee    BIGINT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'active',        -- active | requested
    bell        BOOLEAN NOT NULL DEFAULT false,        -- notify me about every post / story
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (follower, followee)
);
CREATE INDEX follows_followee ON follows (followee, status, created_at DESC);

CREATE TABLE blocks (blocker BIGINT NOT NULL, blocked BIGINT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (blocker, blocked));
CREATE INDEX blocks_blocked ON blocks (blocked);
CREATE TABLE mutes (muter BIGINT NOT NULL, muted BIGINT NOT NULL, posts BOOLEAN NOT NULL DEFAULT true, stories BOOLEAN NOT NULL DEFAULT true, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (muter, muted));
CREATE TABLE restricts (restrictor BIGINT NOT NULL, restricted BIGINT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (restrictor, restricted));
CREATE TABLE close_friends (owner BIGINT NOT NULL, friend BIGINT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (owner, friend));
CREATE TABLE hidden_words (user_id BIGINT NOT NULL, word TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (user_id, word));

CREATE TABLE achievements (user_id BIGINT NOT NULL, key TEXT NOT NULL, awarded_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (user_id, key));

-- ---------------------------------------------------------------- media

CREATE TABLE media (
    id             BIGSERIAL PRIMARY KEY,
    key            TEXT NOT NULL UNIQUE,               -- random, part of every stored path
    owner          BIGINT NOT NULL,
    kind           TEXT NOT NULL,                      -- photo | video | voice | file | chart
    purpose        TEXT NOT NULL,                      -- post | story | topic_video | chat | avatar | cover | comment
    mime           TEXT NOT NULL,
    name           TEXT NOT NULL DEFAULT '',
    declared_size  BIGINT NOT NULL,
    received       BIGINT NOT NULL DEFAULT 0,
    status         TEXT NOT NULL DEFAULT 'uploading',  -- uploading | processing | ready | review | rejected | failed | deleted
    upload_token   TEXT NOT NULL,                      -- sha256 of the direct-upload token
    width          INT,
    height         INT,
    duration_ms    INT,
    variants       JSONB NOT NULL DEFAULT '{}',        -- variant -> storage key
    files          TEXT[] NOT NULL DEFAULT '{}',       -- every stored key (deletion)
    chart          JSONB,                              -- chart snapshot: symbol, timeframe, drawings
    moderation     JSONB,
    reason         TEXT,
    attempts       INT NOT NULL DEFAULT 0,
    attached       BOOLEAN NOT NULL DEFAULT false,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    processed_at   TIMESTAMPTZ
);
CREATE INDEX media_owner ON media (owner, created_at DESC);
CREATE INDEX media_status ON media (status, updated_at) WHERE status IN ('processing', 'uploading');

-- ---------------------------------------------------------------- trade cards

CREATE TABLE trade_cards (
    id           BIGSERIAL PRIMARY KEY,
    owner        BIGINT NOT NULL,
    tenant       TEXT NOT NULL,
    source       TEXT NOT NULL,                        -- position | deal
    login        BIGINT NOT NULL,
    account_type TEXT NOT NULL,                        -- live | demo
    ticket       BIGINT,                               -- position ticket
    deal_id      BIGINT,
    symbol       TEXT NOT NULL,
    side         TEXT NOT NULL,
    detail       TEXT NOT NULL DEFAULT 'percent',      -- full | percent
    state        TEXT NOT NULL,                        -- open | closed
    snapshot     JSONB NOT NULL,                       -- the engine figures (signed)
    signature    TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    refreshed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX trade_cards_owner ON trade_cards (owner, created_at DESC);

-- ---------------------------------------------------------------- posts

CREATE TABLE posts (
    id              BIGSERIAL PRIMARY KEY,
    author          BIGINT NOT NULL,
    kind            TEXT NOT NULL DEFAULT 'post',      -- post | repost | quote | video
    body            TEXT NOT NULL DEFAULT '',
    lang            TEXT NOT NULL DEFAULT 'en',
    media           BIGINT[] NOT NULL DEFAULT '{}',
    trade_card      BIGINT REFERENCES trade_cards (id),
    poll            JSONB,                             -- {options: [..], endsAt, counts: [..]}
    repost_of       BIGINT REFERENCES posts (id),
    quote_of        BIGINT REFERENCES posts (id),
    topic           TEXT,                              -- topic video tab
    academy_chapter TEXT,                              -- Academy chapter discussion thread
    visibility      TEXT NOT NULL DEFAULT 'public',    -- public | followers
    comments_mode   TEXT NOT NULL DEFAULT 'everyone',  -- everyone | followers | off
    pinned_comment  BIGINT,
    status          TEXT NOT NULL DEFAULT 'pending',   -- pending | published | review | rejected | removed | deleted
    shadow          BOOLEAN NOT NULL DEFAULT false,
    risk_line       BOOLEAN NOT NULL DEFAULT false,
    hashtags        TEXT[] NOT NULL DEFAULT '{}',
    cashtags        TEXT[] NOT NULL DEFAULT '{}',
    mentions        BIGINT[] NOT NULL DEFAULT '{}',
    links           TEXT[] NOT NULL DEFAULT '{}',
    likes           BIGINT NOT NULL DEFAULT 0,
    bulls           BIGINT NOT NULL DEFAULT 0,
    bears           BIGINT NOT NULL DEFAULT 0,
    comments        BIGINT NOT NULL DEFAULT 0,
    reposts         BIGINT NOT NULL DEFAULT 0,
    quotes          BIGINT NOT NULL DEFAULT 0,
    saves           BIGINT NOT NULL DEFAULT 0,
    views           BIGINT NOT NULL DEFAULT 0,
    reports         INT NOT NULL DEFAULT 0,
    featured_until  TIMESTAMPTZ,
    helpful         BOOLEAN NOT NULL DEFAULT false,
    moderation      JSONB,
    reason          TEXT,
    attempts        INT NOT NULL DEFAULT 0,
    notified        BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at    TIMESTAMPTZ,
    edited_at       TIMESTAMPTZ,
    deleted_at      TIMESTAMPTZ
);
CREATE INDEX posts_author ON posts (author, created_at DESC);
CREATE INDEX posts_published ON posts (published_at DESC) WHERE status = 'published';
CREATE INDEX posts_pending ON posts (created_at) WHERE status = 'pending';
CREATE INDEX posts_hashtags ON posts USING GIN (hashtags);
CREATE INDEX posts_cashtags ON posts USING GIN (cashtags);
CREATE INDEX posts_topic ON posts (topic, published_at DESC) WHERE topic IS NOT NULL;
CREATE INDEX posts_chapter ON posts (academy_chapter, published_at DESC) WHERE academy_chapter IS NOT NULL;
CREATE INDEX posts_repost ON posts (repost_of) WHERE repost_of IS NOT NULL;
CREATE UNIQUE INDEX posts_one_repost ON posts (author, repost_of) WHERE kind = 'repost' AND status <> 'deleted';

CREATE TABLE reactions (
    post_id    BIGINT NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    user_id    BIGINT NOT NULL,
    kind       TEXT NOT NULL,                          -- like | bull | bear (bull / bear exclusive)
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (post_id, user_id, kind)
);
CREATE INDEX reactions_user ON reactions (user_id, created_at DESC);

CREATE TABLE poll_votes (post_id BIGINT NOT NULL REFERENCES posts (id) ON DELETE CASCADE, user_id BIGINT NOT NULL, option INT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (post_id, user_id));

CREATE TABLE comments (
    id          BIGSERIAL PRIMARY KEY,
    post_id     BIGINT NOT NULL REFERENCES posts (id) ON DELETE CASCADE,
    author      BIGINT NOT NULL,
    parent_id   BIGINT REFERENCES comments (id),       -- one level of replies
    body        TEXT NOT NULL,
    lang        TEXT NOT NULL DEFAULT 'en',
    media       BIGINT,
    mentions    BIGINT[] NOT NULL DEFAULT '{}',
    status      TEXT NOT NULL DEFAULT 'pending',       -- pending | published | review | rejected | removed | deleted
    shadow      BOOLEAN NOT NULL DEFAULT false,
    restricted  BOOLEAN NOT NULL DEFAULT false,        -- the post author restricted the commenter
    likes       BIGINT NOT NULL DEFAULT 0,
    replies     BIGINT NOT NULL DEFAULT 0,
    moderation  JSONB,
    reason      TEXT,
    attempts    INT NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at   TIMESTAMPTZ,
    deleted_at  TIMESTAMPTZ
);
CREATE INDEX comments_post ON comments (post_id, parent_id, created_at);
CREATE INDEX comments_pending ON comments (created_at) WHERE status = 'pending';
CREATE TABLE comment_likes (comment_id BIGINT NOT NULL REFERENCES comments (id) ON DELETE CASCADE, user_id BIGINT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (comment_id, user_id));

CREATE TABLE collections (id BIGSERIAL PRIMARY KEY, user_id BIGINT NOT NULL, name TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now());
CREATE UNIQUE INDEX collections_name ON collections (user_id, lower(name));
CREATE TABLE saves (user_id BIGINT NOT NULL, post_id BIGINT NOT NULL REFERENCES posts (id) ON DELETE CASCADE, collection_id BIGINT REFERENCES collections (id) ON DELETE SET NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (user_id, post_id));

CREATE TABLE seen (user_id BIGINT NOT NULL, post_id BIGINT NOT NULL, at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (user_id, post_id));
CREATE INDEX seen_at ON seen (at);

-- ---------------------------------------------------------------- stories

CREATE TABLE stories (
    id          BIGSERIAL PRIMARY KEY,
    author      BIGINT NOT NULL,
    kind        TEXT NOT NULL,                         -- photo | video | chart | trade | text
    media       BIGINT,
    body        TEXT NOT NULL DEFAULT '',
    background  TEXT,
    trade_card  BIGINT REFERENCES trade_cards (id),
    stickers    JSONB NOT NULL DEFAULT '[]',           -- [{type: sentiment|question, ...}]
    audience    TEXT NOT NULL DEFAULT 'everyone',      -- everyone | close_friends
    status      TEXT NOT NULL DEFAULT 'pending',       -- pending | published | review | rejected | removed | deleted
    shadow      BOOLEAN NOT NULL DEFAULT false,
    views       BIGINT NOT NULL DEFAULT 0,
    moderation  JSONB,
    reason      TEXT,
    attempts    INT NOT NULL DEFAULT 0,
    notified    BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL
);
CREATE INDEX stories_author ON stories (author, created_at DESC);
CREATE INDEX stories_live ON stories (expires_at) WHERE status = 'published';
CREATE TABLE story_views (story_id BIGINT NOT NULL REFERENCES stories (id) ON DELETE CASCADE, viewer BIGINT NOT NULL, at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (story_id, viewer));
CREATE TABLE story_votes (story_id BIGINT NOT NULL REFERENCES stories (id) ON DELETE CASCADE, user_id BIGINT NOT NULL, choice TEXT NOT NULL, at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (story_id, user_id));
CREATE TABLE story_answers (id BIGSERIAL PRIMARY KEY, story_id BIGINT NOT NULL REFERENCES stories (id) ON DELETE CASCADE, user_id BIGINT NOT NULL, body TEXT NOT NULL, at TIMESTAMPTZ NOT NULL DEFAULT now());
CREATE TABLE highlights (id BIGSERIAL PRIMARY KEY, owner BIGINT NOT NULL, title TEXT NOT NULL, cover_story BIGINT, position INT NOT NULL DEFAULT 0, created_at TIMESTAMPTZ NOT NULL DEFAULT now());
CREATE INDEX highlights_owner ON highlights (owner, position);
CREATE TABLE highlight_items (highlight_id BIGINT NOT NULL REFERENCES highlights (id) ON DELETE CASCADE, story_id BIGINT NOT NULL REFERENCES stories (id) ON DELETE CASCADE, position INT NOT NULL DEFAULT 0, PRIMARY KEY (highlight_id, story_id));

-- ---------------------------------------------------------------- chat

CREATE TABLE conversations (
    id               BIGSERIAL PRIMARY KEY,
    kind             TEXT NOT NULL,                    -- dm | group | room | master_room
    title            TEXT NOT NULL DEFAULT '',
    about            TEXT NOT NULL DEFAULT '',
    avatar_media     BIGINT,
    symbol           TEXT,                             -- public symbol rooms
    slug             TEXT UNIQUE,                      -- rooms: gold, eurusd, ...
    master_user      BIGINT,                           -- masters' follower rooms
    master_tenant    TEXT,
    owner            BIGINT,
    dm_key           TEXT UNIQUE,                      -- "<low>:<high>" user ids
    member_count     INT NOT NULL DEFAULT 0,
    status           TEXT NOT NULL DEFAULT 'active',   -- active | locked | archived
    last_message_id  BIGINT,
    last_message_at  TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX conversations_master ON conversations (master_user) WHERE kind = 'master_room';

CREATE TABLE conv_members (
    conversation_id BIGINT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    user_id         BIGINT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'member',    -- owner | admin | member
    state           TEXT NOT NULL DEFAULT 'active',    -- active | request | declined | left | removed
    muted           BOOLEAN NOT NULL DEFAULT false,
    last_read_id    BIGINT NOT NULL DEFAULT 0,
    unread          INT NOT NULL DEFAULT 0,
    joined_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (conversation_id, user_id)
);
CREATE INDEX conv_members_user ON conv_members (user_id, state);

CREATE TABLE messages (
    id              BIGSERIAL PRIMARY KEY,
    conversation_id BIGINT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    sender          BIGINT NOT NULL,
    kind            TEXT NOT NULL DEFAULT 'text',      -- text | photo | video | voice | file | chart | trade_card | post | system
    body            TEXT NOT NULL DEFAULT '',
    media           BIGINT,
    trade_card      BIGINT REFERENCES trade_cards (id),
    post_id         BIGINT,
    reply_to        BIGINT,
    client_id       TEXT,
    status          TEXT NOT NULL DEFAULT 'sent',      -- pending (media check) | sent | hidden (moderation)
    moderation      JSONB,
    checked         BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    edited_at       TIMESTAMPTZ,
    deleted_at      TIMESTAMPTZ
);
CREATE INDEX messages_conv ON messages (conversation_id, id DESC);
CREATE INDEX messages_unchecked ON messages (id) WHERE NOT checked;
CREATE UNIQUE INDEX messages_client ON messages (conversation_id, sender, client_id) WHERE client_id IS NOT NULL;

-- staff access to a chat: only through a report or a legal request, time-limited, every read audited
CREATE TABLE chat_access (
    id              BIGSERIAL PRIMARY KEY,
    conversation_id BIGINT NOT NULL REFERENCES conversations (id),
    staff_id        TEXT NOT NULL,
    staff_name      TEXT NOT NULL,
    staff_tenant    TEXT NOT NULL,
    basis           TEXT NOT NULL,                     -- report | legal
    report_id       BIGINT,
    legal_ref       TEXT,
    reason          TEXT NOT NULL,
    expires_at      TIMESTAMPTZ NOT NULL,
    revoked_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------- safety

CREATE TABLE reports (
    id           BIGSERIAL PRIMARY KEY,
    reporter     BIGINT NOT NULL,
    target_kind  TEXT NOT NULL,                        -- post | comment | story | message | profile | conversation
    target_id    BIGINT NOT NULL,
    target_owner BIGINT,
    tenant       TEXT NOT NULL,                        -- the reported owner's broker (staff scope)
    reason       TEXT NOT NULL,
    note         TEXT NOT NULL DEFAULT '',
    status       TEXT NOT NULL DEFAULT 'open',         -- open | actioned | dismissed
    resolution   TEXT,
    resolved_by  TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at  TIMESTAMPTZ
);
CREATE INDEX reports_open ON reports (status, created_at);
CREATE UNIQUE INDEX reports_once ON reports (reporter, target_kind, target_id) WHERE status = 'open';

CREATE TABLE mod_queue (
    id          BIGSERIAL PRIMARY KEY,
    target_kind TEXT NOT NULL,                         -- post | comment | story | message | media | profile
    target_id   BIGINT NOT NULL,
    owner       BIGINT NOT NULL,
    tenant      TEXT NOT NULL,
    source      TEXT NOT NULL,                         -- ai | rules | report | fallback
    categories  TEXT[] NOT NULL DEFAULT '{}',
    verdict     JSONB NOT NULL DEFAULT '{}',
    excerpt     TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'open',          -- open | approved | removed | dismissed
    decided_by  TEXT,
    note        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at  TIMESTAMPTZ
);
CREATE INDEX mod_queue_open ON mod_queue (status, created_at);
CREATE UNIQUE INDEX mod_queue_once ON mod_queue (target_kind, target_id) WHERE status = 'open';

CREATE TABLE rules (
    id          BIGSERIAL PRIMARY KEY,
    kind        TEXT NOT NULL,                         -- keyword | link_allow
    pattern     TEXT NOT NULL,
    action      TEXT NOT NULL DEFAULT 'block',         -- block | review (keywords); allow (links)
    note        TEXT NOT NULL DEFAULT '',
    active      BOOLEAN NOT NULL DEFAULT true,
    created_by  TEXT NOT NULL DEFAULT 'system',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX rules_unique ON rules (kind, lower(pattern));

CREATE TABLE sanctions (
    id         BIGSERIAL PRIMARY KEY,
    user_id    BIGINT NOT NULL,
    kind       TEXT NOT NULL,                          -- warn | ban | unban | shadow | unshadow
    reason     TEXT NOT NULL DEFAULT '',
    until      TIMESTAMPTZ,
    staff      TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX sanctions_user ON sanctions (user_id, created_at DESC);

-- ---------------------------------------------------------------- staff content

CREATE TABLE announcements (
    id         BIGSERIAL PRIMARY KEY,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    link       TEXT,
    pinned     BOOLEAN NOT NULL DEFAULT true,
    starts_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    ends_at    TIMESTAMPTZ,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE topics (
    key         TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    position    INT NOT NULL DEFAULT 0,
    active      BOOLEAN NOT NULL DEFAULT true
);

CREATE TABLE features (
    kind       TEXT NOT NULL,                          -- profile
    target     BIGINT NOT NULL,
    until      TIMESTAMPTZ,
    created_by TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (kind, target)
);

CREATE TABLE trending (kind TEXT NOT NULL, tag TEXT NOT NULL, posts BIGINT NOT NULL, score DOUBLE PRECISION NOT NULL, computed_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY (kind, tag));

CREATE TABLE settings (key TEXT PRIMARY KEY, data JSONB NOT NULL, updated_by TEXT NOT NULL DEFAULT 'system', updated_at TIMESTAMPTZ NOT NULL DEFAULT now());

-- ---------------------------------------------------------------- notifications

CREATE TABLE activity (
    id              BIGSERIAL PRIMARY KEY,
    user_id         BIGINT NOT NULL,                   -- recipient
    kind            TEXT NOT NULL,
    actor           BIGINT,
    post_id         BIGINT,
    comment_id      BIGINT,
    story_id        BIGINT,
    conversation_id BIGINT,
    data            JSONB NOT NULL DEFAULT '{}',
    read            BOOLEAN NOT NULL DEFAULT false,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX activity_user ON activity (user_id, id DESC);

CREATE TABLE notify_outbox (
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT NOT NULL,
    tenant      TEXT NOT NULL,
    kind        TEXT NOT NULL,                         -- circle.follow, circle.like, ...
    title       TEXT NOT NULL,
    body        TEXT NOT NULL DEFAULT '',
    link        TEXT,
    data        JSONB NOT NULL DEFAULT '{}',
    batch_key   TEXT,
    actors      BIGINT[] NOT NULL DEFAULT '{}',
    bell        BOOLEAN NOT NULL DEFAULT true,
    push        BOOLEAN NOT NULL DEFAULT true,
    status      TEXT NOT NULL DEFAULT 'pending',       -- pending | sent | failed | skipped
    attempts    INT NOT NULL DEFAULT 0,
    error       TEXT,
    next_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    sent_at     TIMESTAMPTZ
);
CREATE INDEX notify_outbox_due ON notify_outbox (next_at) WHERE status = 'pending';
CREATE UNIQUE INDEX notify_outbox_batch ON notify_outbox (batch_key) WHERE status = 'pending' AND batch_key IS NOT NULL;

CREATE TABLE devices (
    token        TEXT PRIMARY KEY,
    user_id      BIGINT NOT NULL,
    platform     TEXT NOT NULL,                        -- android | ios | web
    locale       TEXT,
    app_version  TEXT,
    disabled     BOOLEAN NOT NULL DEFAULT false,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX devices_user ON devices (user_id) WHERE NOT disabled;

CREATE TABLE notif_prefs (user_id BIGINT PRIMARY KEY, prefs JSONB NOT NULL DEFAULT '{}', updated_at TIMESTAMPTZ NOT NULL DEFAULT now());

-- events for other services (Rewards points, creator fee discounts): delivered by a worker, retried
CREATE TABLE outbox (
    id           BIGSERIAL PRIMARY KEY,
    kind         TEXT NOT NULL,                        -- rewards.points | creator.fee_discount
    tenant       TEXT NOT NULL,
    user_id      BIGINT NOT NULL,
    dedupe_key   TEXT NOT NULL UNIQUE,
    payload      JSONB NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',      -- pending | delivered | failed | held
    attempts     INT NOT NULL DEFAULT 0,
    error        TEXT,
    next_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    delivered_at TIMESTAMPTZ
);
CREATE INDEX outbox_due ON outbox (next_at) WHERE status = 'pending';

-- ---------------------------------------------------------------- AI cache

CREATE TABLE ai_cache (
    key        TEXT PRIMARY KEY,                       -- sha256 of kind + inputs
    kind       TEXT NOT NULL,                          -- translate | sentiment | caption | digest
    result     JSONB NOT NULL,
    model      TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ
);

-- ---------------------------------------------------------------- audit (append-only)

CREATE TABLE audit_log (
    id         BIGSERIAL PRIMARY KEY,
    actor      TEXT NOT NULL,                          -- staff:<id> | user:<id> | system
    actor_name TEXT,
    actor_tenant TEXT,
    action     TEXT NOT NULL,
    target     TEXT,
    before     JSONB,
    after      JSONB,
    note       TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX audit_log_at ON audit_log (created_at DESC);

CREATE FUNCTION audit_log_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit_log is append-only';
END $$;
CREATE TRIGGER audit_log_no_change BEFORE UPDATE OR DELETE ON audit_log FOR EACH ROW EXECUTE FUNCTION audit_log_immutable();

-- ---------------------------------------------------------------- seed

INSERT INTO rules (kind, pattern, action, note) VALUES
    ('link_allow', 'kalkstrade.com', 'allow', 'Kalks pages'),
    ('link_allow', 'kalks.com', 'allow', 'Kalks pages'),
    ('link_allow', 'youtube.com', 'allow', 'YouTube'),
    ('link_allow', 'youtu.be', 'allow', 'YouTube short links'),
    ('link_allow', 'tradingview.com', 'allow', 'TradingView'),
    ('keyword', 'guaranteed profit', 'block', 'Profit promises are not allowed'),
    ('keyword', 'guaranteed returns', 'block', 'Profit promises are not allowed'),
    ('keyword', 'risk free profit', 'block', 'Profit promises are not allowed'),
    ('keyword', '100% win rate', 'review', 'Unverifiable performance claim'),
    ('keyword', 'double your money', 'block', 'Scam wording'),
    ('keyword', 'send me your password', 'block', 'Credential phishing'),
    ('keyword', 'account management service', 'review', 'Off-platform account management'),
    ('keyword', 'whatsapp me', 'review', 'Off-platform contact for signals'),
    ('keyword', 'telegram me', 'review', 'Off-platform contact for signals');

INSERT INTO topics (key, title, description, position) VALUES
    ('basics', 'Trading basics', 'First steps: orders, lots, leverage, margin.', 1),
    ('forex', 'Forex', 'Currency pairs and the macro behind them.', 2),
    ('gold', 'Gold & metals', 'XAUUSD, silver and the metals market.', 3),
    ('indices', 'Indices', 'US30, NAS100, GER40 and other indices.', 4),
    ('crypto', 'Crypto', 'Bitcoin, Ethereum and the crypto market.', 5),
    ('strategy', 'Strategies', 'Setups, systems and backtests.', 6),
    ('analysis', 'Chart analysis', 'Technical analysis walkthroughs.', 7),
    ('risk', 'Risk management', 'Position sizing, stops and drawdowns.', 8),
    ('psychology', 'Trading psychology', 'Discipline, habits and mindset.', 9),
    ('options', 'Options', 'FX options on Kalks.', 10),
    ('platform', 'Kalks Trader', 'Tips for the Kalks platform and app.', 11);

INSERT INTO conversations (kind, title, about, symbol, slug) VALUES
    ('room', '#gold', 'Gold (XAUUSD): ideas, news and live talk.', 'XAUUSD', 'gold'),
    ('room', '#eurusd', 'EUR/USD: ideas, news and live talk.', 'EURUSD', 'eurusd'),
    ('room', '#gbpusd', 'GBP/USD: ideas, news and live talk.', 'GBPUSD', 'gbpusd'),
    ('room', '#usdjpy', 'USD/JPY: ideas, news and live talk.', 'USDJPY', 'usdjpy'),
    ('room', '#btcusd', 'Bitcoin: ideas, news and live talk.', 'BTCUSD', 'btcusd'),
    ('room', '#nas100', 'Nasdaq 100: ideas, news and live talk.', 'NAS100', 'nas100'),
    ('room', '#us30', 'Dow Jones 30: ideas, news and live talk.', 'US30', 'us30'),
    ('room', '#oil', 'Crude oil: ideas, news and live talk.', 'USOIL', 'oil');

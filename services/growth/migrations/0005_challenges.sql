-- Challenges (phase 4): contest templates, missions with per-period progress, XP (append-only ledger), levels,
-- daily streaks, leaderboard nicknames and the automatic payout threshold. Additive only: existing contests keep
-- scoring and paying exactly as before (auto-pay is off until a broker sets `autoPayBelow`).

-- ------------------------------------------------------------------ contest templates

-- A ready-made contest the Back Office wizard starts from. `schedule` places the window: next_monday (a week from
-- Monday 00:00 server time), next_month (the next calendar month), next_weekend (Saturday 00:00 to Monday 00:00),
-- tomorrow (`duration_days` from tomorrow 00:00). `split` = prize-pool split preset key or explicit bands.
CREATE TABLE contest_templates (
    id              BIGSERIAL PRIMARY KEY,
    tenant          TEXT NOT NULL,
    key             TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    kind            TEXT NOT NULL DEFAULT 'live' CHECK (kind IN ('demo', 'live')),
    instrument      TEXT NOT NULL DEFAULT 'cfd' CHECK (instrument IN ('cfd', 'options')),
    scoring         TEXT NOT NULL CHECK (scoring IN ('return_pct', 'profit', 'lots', 'contracts')),
    schedule        TEXT NOT NULL DEFAULT 'next_monday' CHECK (schedule IN ('next_monday', 'next_month', 'next_weekend', 'tomorrow')),
    duration_days   INT NOT NULL DEFAULT 7 CHECK (duration_days BETWEEN 1 AND 366),
    min_trades      INT NOT NULL DEFAULT 0,
    max_entrants    INT,
    account_groups  TEXT[] NOT NULL DEFAULT '{}',
    asset_classes   TEXT[] NOT NULL DEFAULT '{}',
    kyc_required    BOOLEAN NOT NULL DEFAULT true,
    min_equity      NUMERIC,
    prize_pool      NUMERIC NOT NULL DEFAULT 0 CHECK (prize_pool >= 0),
    split           JSONB NOT NULL DEFAULT '"top3"'::jsonb,
    payout          TEXT NOT NULL DEFAULT 'wallet' CHECK (payout IN ('wallet', 'credit')),
    rules           TEXT NOT NULL DEFAULT '',
    anti_cheat      JSONB NOT NULL DEFAULT '{}'::jsonb,
    active          BOOLEAN NOT NULL DEFAULT true,
    sort            INT NOT NULL DEFAULT 100,
    created_by      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant, key)
);

-- contests: the template it came from, the contest it was cloned from, and an asset-class filter (empty = every
-- CFD; "Weekend crypto sprint" counts crypto deals only)
ALTER TABLE contests ADD COLUMN IF NOT EXISTS template_id BIGINT REFERENCES contest_templates(id);
ALTER TABLE contests ADD COLUMN IF NOT EXISTS cloned_from BIGINT REFERENCES contests(id);
ALTER TABLE contests ADD COLUMN IF NOT EXISTS asset_classes TEXT[] NOT NULL DEFAULT '{}';

-- prizes at or above the broker's auto-pay threshold wait for staff approval (marketing.approve)
ALTER TABLE contest_entries DROP CONSTRAINT IF EXISTS contest_entries_prize_status_check;
ALTER TABLE contest_entries ADD CONSTRAINT contest_entries_prize_status_check CHECK (prize_status IN ('none', 'pending', 'approval', 'paid', 'failed'));

-- ------------------------------------------------------------------ missions

CREATE TABLE missions (
    id             BIGSERIAL PRIMARY KEY,
    tenant         TEXT NOT NULL,
    name           TEXT NOT NULL,
    description    TEXT NOT NULL DEFAULT '',
    kind           TEXT NOT NULL CHECK (kind IN ('kyc', 'first_deposit', 'deposit', 'lots', 'trading_days', 'hold', 'join_contest', 'referral')),
    schedule       TEXT NOT NULL DEFAULT 'once' CHECK (schedule IN ('daily', 'weekly', 'once')),
    -- lots (lots), trading_days (days), hold (hours), deposit (USD), referral (friends); 1 for the rest
    target         NUMERIC NOT NULL DEFAULT 1 CHECK (target > 0),
    -- anti-abuse and filters: {minLots, minHoldSeconds, assetClasses[]} (defaults from settings)
    params         JSONB NOT NULL DEFAULT '{}'::jsonb,
    xp             INT NOT NULL DEFAULT 0 CHECK (xp >= 0 AND xp <= 100000),
    reward_amount  NUMERIC CHECK (reward_amount IS NULL OR reward_amount > 0),
    reward_payout  TEXT CHECK (reward_payout IS NULL OR reward_payout IN ('wallet', 'credit')),
    kyc_required   BOOLEAN NOT NULL DEFAULT true,     -- for the cash / credit reward (XP is always given)
    active         BOOLEAN NOT NULL DEFAULT true,
    starts_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    ends_at        TIMESTAMPTZ,
    sort           INT NOT NULL DEFAULT 100,
    created_by     TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX missions_tenant_idx ON missions (tenant, active, sort);

-- One row per client, mission and period ('2026-10-19' daily, '2026-W43' weekly, 'once'): the reward is booked once
-- per period because the row is unique and `completed_at` is set under a row lock.
CREATE TABLE mission_progress (
    id             BIGSERIAL PRIMARY KEY,
    tenant         TEXT NOT NULL,
    mission_id     BIGINT NOT NULL REFERENCES missions(id),
    user_id        BIGINT NOT NULL,
    period         TEXT NOT NULL,
    progress       NUMERIC NOT NULL DEFAULT 0,
    target         NUMERIC NOT NULL,
    completed_at   TIMESTAMPTZ,
    xp             INT NOT NULL DEFAULT 0,
    reward_amount  NUMERIC,
    reward_payout  TEXT,
    -- none | kyc (waiting for verified KYC) | approval (staff) | pending (queued) | paid | failed | rejected
    reward_status  TEXT NOT NULL DEFAULT 'none' CHECK (reward_status IN ('none', 'kyc', 'approval', 'pending', 'paid', 'failed', 'rejected')),
    reward_ref     TEXT,
    reward_error   TEXT,
    decided_by     TEXT,
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (mission_id, user_id, period)
);
CREATE INDEX mission_progress_user_idx ON mission_progress (tenant, user_id, updated_at DESC);
CREATE INDEX mission_progress_reward_idx ON mission_progress (tenant, reward_status) WHERE reward_status IN ('kyc', 'approval', 'pending', 'failed');

-- ------------------------------------------------------------------ XP, levels, streaks

CREATE TABLE xp_ledger (
    id           BIGSERIAL PRIMARY KEY,
    tenant       TEXT NOT NULL,
    user_id      BIGINT NOT NULL,
    xp           INT NOT NULL,
    source       TEXT NOT NULL CHECK (source IN ('mission', 'contest', 'streak', 'adjust')),
    ref          TEXT NOT NULL,
    description  TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant, source, ref)
);
CREATE INDEX xp_ledger_user_idx ON xp_ledger (tenant, user_id, created_at DESC);
CREATE TRIGGER xp_ledger_no_update BEFORE UPDATE OR DELETE ON xp_ledger FOR EACH ROW EXECUTE FUNCTION growth_append_only();

CREATE TABLE levels (
    tenant   TEXT NOT NULL,
    level    INT NOT NULL CHECK (level BETWEEN 1 AND 100),
    name     TEXT NOT NULL,
    min_xp   BIGINT NOT NULL CHECK (min_xp >= 0),
    badge    TEXT NOT NULL DEFAULT 'bronze',
    PRIMARY KEY (tenant, level)
);

-- Daily streaks per client: `trading` (a day with a qualifying live trade; weekends never break it) and `login`
-- (a day the client opened the Client Area / app). Missing up to `streakGraceDays` days is forgiven once a week.
CREATE TABLE streaks (
    tenant       TEXT NOT NULL,
    user_id      BIGINT NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('trading', 'login')),
    current      INT NOT NULL DEFAULT 0,
    best         INT NOT NULL DEFAULT 0,
    last_day     DATE,
    grace_used   DATE,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant, user_id, kind)
);

-- ------------------------------------------------------------------ leaderboard nicknames

-- What other people see on every leaderboard (never the name, email or login). Auto-generated on first use; the
-- client can change it and choose whether it shows on the public website (`public` NULL = the broker's default).
CREATE TABLE nicknames (
    tenant      TEXT NOT NULL,
    user_id     BIGINT NOT NULL,
    nickname    TEXT NOT NULL,
    public      BOOLEAN,
    custom      BOOLEAN NOT NULL DEFAULT false,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant, user_id)
);
CREATE UNIQUE INDEX nicknames_unique_idx ON nicknames (tenant, lower(nickname));

-- referral missions: who referred this client (gateway users feed `referred_by`)
ALTER TABLE profiles ADD COLUMN IF NOT EXISTS referred_by BIGINT;
CREATE INDEX IF NOT EXISTS profiles_referred_idx ON profiles (tenant, referred_by) WHERE referred_by IS NOT NULL;

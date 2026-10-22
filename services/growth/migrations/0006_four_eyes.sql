-- Four-eyes (maker-checker) on marketing money out: contest prizes, mission cash / credit rewards, cashback payouts
-- and loyalty cashback redemptions. A payout at or above the broker's `fourEyesThreshold` (settings JSON, USD,
-- default 1000, 0 = every payout) needs two different staff members: the first approval moves it to
-- `awaiting_second_approval` (nothing is paid), a second staff member's approval releases it. The same staff member
-- is refused (409 four_eyes_same_staff). Auto-pay never releases an amount at or above the threshold.

-- contest prizes (+ rejected: staff can now reject a prize that waits for approval)
ALTER TABLE contest_entries DROP CONSTRAINT IF EXISTS contest_entries_prize_status_check;
ALTER TABLE contest_entries ADD CONSTRAINT contest_entries_prize_status_check
    CHECK (prize_status IN ('none', 'pending', 'approval', 'awaiting_second_approval', 'paid', 'failed', 'rejected'));

-- mission rewards
ALTER TABLE mission_progress DROP CONSTRAINT IF EXISTS mission_progress_reward_status_check;
ALTER TABLE mission_progress ADD CONSTRAINT mission_progress_reward_status_check
    CHECK (reward_status IN ('none', 'kyc', 'approval', 'awaiting_second_approval', 'pending', 'paid', 'failed', 'rejected'));

-- cashback payouts: at or above the threshold the batch creates them in `approval` (the payout loop only pays
-- `pending`); a rejected payout marks its accruals `rejected` (never paid, never re-batched)
ALTER TABLE cashback_payouts DROP CONSTRAINT IF EXISTS cashback_payouts_status_check;
ALTER TABLE cashback_payouts ADD CONSTRAINT cashback_payouts_status_check
    CHECK (status IN ('approval', 'awaiting_second_approval', 'pending', 'paid', 'failed', 'rejected'));
ALTER TABLE cashback_payouts ADD COLUMN IF NOT EXISTS decided_by TEXT;
ALTER TABLE cashback_payouts ADD COLUMN IF NOT EXISTS decided_at TIMESTAMPTZ;
ALTER TABLE cashback_payouts ADD COLUMN IF NOT EXISTS reject_reason TEXT;
ALTER TABLE cashback_accruals DROP CONSTRAINT IF EXISTS cashback_accruals_status_check;
ALTER TABLE cashback_accruals ADD CONSTRAINT cashback_accruals_status_check CHECK (status IN ('accrued', 'paid', 'void', 'rejected'));

-- loyalty redemptions: a cashback redemption at or above the threshold waits for staff (`approval`); a rejected
-- one gives the points back
ALTER TABLE redemptions DROP CONSTRAINT IF EXISTS redemptions_status_check;
ALTER TABLE redemptions ADD CONSTRAINT redemptions_status_check
    CHECK (status IN ('approval', 'awaiting_second_approval', 'pending', 'completed', 'failed', 'rejected'));
ALTER TABLE redemptions ADD COLUMN IF NOT EXISTS decided_by TEXT;

-- the first approval on every kind of payout
ALTER TABLE contest_entries
    ADD COLUMN IF NOT EXISTS first_approved_by_id   TEXT,         -- staff id (X-Kalks-Staff-Id) of the first approver
    ADD COLUMN IF NOT EXISTS first_approved_by_name TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS first_approval_note    TEXT,
    ADD COLUMN IF NOT EXISTS four_eyes_threshold    NUMERIC;      -- the threshold in force at the first approval
ALTER TABLE mission_progress
    ADD COLUMN IF NOT EXISTS first_approved_by_id   TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_by_name TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS first_approval_note    TEXT,
    ADD COLUMN IF NOT EXISTS four_eyes_threshold    NUMERIC;
ALTER TABLE cashback_payouts
    ADD COLUMN IF NOT EXISTS first_approved_by_id   TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_by_name TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS first_approval_note    TEXT,
    ADD COLUMN IF NOT EXISTS four_eyes_threshold    NUMERIC;
ALTER TABLE redemptions
    ADD COLUMN IF NOT EXISTS first_approved_by_id   TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_by_name TEXT,
    ADD COLUMN IF NOT EXISTS first_approved_at      TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS first_approval_note    TEXT,
    ADD COLUMN IF NOT EXISTS four_eyes_threshold    NUMERIC;

ALTER TABLE contest_entries ADD CONSTRAINT contest_entries_first_approval_check
    CHECK (prize_status <> 'awaiting_second_approval' OR first_approved_by_id IS NOT NULL);
ALTER TABLE mission_progress ADD CONSTRAINT mission_progress_first_approval_check
    CHECK (reward_status <> 'awaiting_second_approval' OR first_approved_by_id IS NOT NULL);
ALTER TABLE cashback_payouts ADD CONSTRAINT cashback_payouts_first_approval_check
    CHECK (status <> 'awaiting_second_approval' OR first_approved_by_id IS NOT NULL);
ALTER TABLE redemptions ADD CONSTRAINT redemptions_first_approval_check
    CHECK (status <> 'awaiting_second_approval' OR first_approved_by_id IS NOT NULL);

DROP INDEX IF EXISTS mission_progress_reward_idx;
CREATE INDEX mission_progress_reward_idx ON mission_progress (tenant, reward_status)
    WHERE reward_status IN ('kyc', 'approval', 'awaiting_second_approval', 'pending', 'failed');
CREATE INDEX IF NOT EXISTS cashback_payouts_open_idx ON cashback_payouts (tenant, status) WHERE status IN ('approval', 'awaiting_second_approval');
CREATE INDEX IF NOT EXISTS redemptions_open_idx ON redemptions (tenant, status) WHERE status IN ('approval', 'awaiting_second_approval');

-- Four-eyes (maker-checker) on payout batches. A batch whose total is at or above the programme's
-- `fourEyesThreshold` (settings JSON, USD, default 1000, 0 = every batch) needs two different staff members:
-- the first approval moves it to `awaiting_second_approval` (nothing is paid), a second staff member's approval
-- releases it (`approved` → wallet transfers). The same staff member is refused (409 four_eyes_same_staff).

ALTER TABLE payout_batches DROP CONSTRAINT IF EXISTS payout_batches_status_check;
ALTER TABLE payout_batches ADD CONSTRAINT payout_batches_status_check
    CHECK (status IN ('pending_approval', 'awaiting_second_approval', 'approved', 'paid', 'partially_paid', 'rejected'));

ALTER TABLE payout_batches
    ADD COLUMN first_approved_by_id   TEXT,          -- staff:<id> of the first approver
    ADD COLUMN first_approved_by_name TEXT,
    ADD COLUMN first_approved_at      TIMESTAMPTZ,
    ADD COLUMN first_approval_note    TEXT,
    ADD COLUMN four_eyes_threshold    NUMERIC,       -- the threshold in force at the first approval
    ADD COLUMN decided_by_id          TEXT;          -- staff:<id> (or system) who released / rejected the batch

ALTER TABLE payout_batches ADD CONSTRAINT payout_batches_first_approval_check
    CHECK (status <> 'awaiting_second_approval' OR first_approved_by_id IS NOT NULL);

CREATE INDEX payout_batches_open_idx ON payout_batches (tenant, status) WHERE status IN ('pending_approval', 'awaiting_second_approval');

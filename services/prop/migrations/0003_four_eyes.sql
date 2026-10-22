-- Four-eyes (maker-checker) approval on funded payouts.
-- A payout whose USD total (trader share + fee refund) is at or above the tenant's `four_eyes_threshold` needs two
-- different staff members with prop.approve: the first approval moves it to `awaiting_second_approval` (no money
-- moves), the second one releases the wallet credit. Below the threshold one approval still pays.

-- Per-tenant prop settings (one row per tenant; a missing row means the defaults).
CREATE TABLE settings (
    tenant               TEXT PRIMARY KEY,
    four_eyes_threshold  NUMERIC NOT NULL DEFAULT 1000 CHECK (four_eyes_threshold >= 0),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by           TEXT
);
ALTER TABLE settings ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON settings USING (tenant = current_setting('kalks.tenant', true));

ALTER TABLE payouts
    ADD COLUMN first_approved_by_id   TEXT,
    ADD COLUMN first_approved_by_name TEXT,
    ADD COLUMN first_approved_at      TIMESTAMPTZ,
    ADD COLUMN first_approval_note    TEXT,
    ADD COLUMN four_eyes_threshold    NUMERIC;

ALTER TABLE payouts DROP CONSTRAINT payouts_status_check;
ALTER TABLE payouts ADD CONSTRAINT payouts_status_check
    CHECK (status IN ('pending', 'awaiting_second_approval', 'approved', 'paid', 'rejected', 'failed'));

-- still one open request per funded account
DROP INDEX payouts_one_pending;
CREATE UNIQUE INDEX payouts_one_pending ON payouts (account_id) WHERE status IN ('pending', 'awaiting_second_approval', 'approved');

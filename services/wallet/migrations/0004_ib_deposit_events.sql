-- Outbox of real-money deposits reported to the IB service (POST $IB_URL/v1/ib/events/deposit) for the
-- client's first deposit (FTD / CPA). One row per wallet ledger txn of kind `deposit` (on-chain deposit
-- credited) or `manual_deposit` (Back Office "Deposit (external payment received)"), written in the same
-- transaction as the credit; `src/ib.rs` delivers it after the commit with the idempotency key
-- `wallet:deposit:<tenant_id>:<txn_id>` and retries with backoff. Transfers from trading, IB payouts, bonuses,
-- refunds and other adjustments are never written here.
-- Deposits booked before this migration are queued by the worker's sweep (`ib::sweep`), so clients who already
-- funded their wallet are reported too.
CREATE TABLE ib_deposit_events (
    id             BIGSERIAL PRIMARY KEY,
    tenant_id      BIGINT NOT NULL REFERENCES tenants(id),
    user_id        BIGINT NOT NULL,
    txn_id         BIGINT NOT NULL UNIQUE REFERENCES ledger_txns(id),
    kind           TEXT NOT NULL CHECK (kind IN ('deposit', 'manual_deposit')),
    currency       TEXT NOT NULL,
    amount         NUMERIC NOT NULL CHECK (amount > 0),
    occurred_at    TIMESTAMPTZ NOT NULL,          -- the ledger txn time (sent as `at`, stable across retries)
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    pushed_at      TIMESTAMPTZ,
    push_attempts  INT NOT NULL DEFAULT 0,
    push_next_at   TIMESTAMPTZ,
    push_error     TEXT,
    first_deposit  BOOLEAN                        -- IB's answer: this deposit became the client's first deposit
);
CREATE INDEX ib_deposit_events_push_idx ON ib_deposit_events (id) WHERE pushed_at IS NULL;
CREATE INDEX ledger_txns_real_deposit_idx ON ledger_txns (id) WHERE kind IN ('deposit', 'manual_deposit');

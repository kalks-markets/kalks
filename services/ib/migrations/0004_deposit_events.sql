-- Deposit events pushed by other services (the wallet: `POST /v1/ib/events/deposit` with the idempotency key
-- `wallet:deposit:<tenant>:<txn>`). One row per key: a retry is answered `duplicate` and changes nothing, the
-- same key with another body is refused. The member's first deposit stays the earliest one, whichever path
-- (event or engine ledger `deposit` / `transfer_in`) saw it, so wallet money later moved to a trading account
-- is never counted twice.
CREATE TABLE IF NOT EXISTS deposit_events (
    tenant           TEXT NOT NULL,
    idempotency_key  TEXT NOT NULL,
    user_id          BIGINT NOT NULL,
    amount           NUMERIC NOT NULL,              -- USD (USDT 1:1, USC ÷ 100)
    currency         TEXT NOT NULL,                 -- as sent
    at               TIMESTAMPTZ NOT NULL,
    source           TEXT NOT NULL,
    first_deposit    BOOLEAN NOT NULL,              -- it became (or moved earlier) the member's first deposit
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant, idempotency_key)
);
CREATE INDEX IF NOT EXISTS deposit_events_user_idx ON deposit_events (tenant, user_id);

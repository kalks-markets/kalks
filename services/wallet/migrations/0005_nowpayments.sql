-- NOWPayments (nowpayments.io) crypto deposits, next to the direct USDT BEP20 / TRC20 deposits.
--
-- The client picks an amount in USD and a coin; NOWPayments gives an address (and a memo / tag for the coins that
-- need one) and the exact amount to pay. When NOWPayments reports the payment `finished` (IPN, or the poller), the
-- service re-reads it from the NOWPayments API and credits `price_amount` USD to the client's wallet (USDT 1:1),
-- once: the ledger key `deposit:nowpayments:<np_payment_id>` is unique per tenant and the row is locked while it is
-- credited. Partial, over- and late payments wait in `review` for staff (four-eyes above the adjustments threshold).
-- See services/wallet/README.md#nowpayments and src/ops/nowpayments.rs.

-- One row per broker. Secrets are AES-256-GCM encrypted with WALLET_SECRETS_KEY (src/secrets.rs) and never returned
-- by the API: the Back Office sees "set", the last 4 characters and who changed them when.
CREATE TABLE nowpayments_settings (
    tenant_id               BIGINT PRIMARY KEY REFERENCES tenants(id),
    enabled                 BOOLEAN NOT NULL DEFAULT false,
    sandbox                 BOOLEAN NOT NULL DEFAULT false,
    api_key_enc             TEXT,
    api_key_last4           TEXT,
    api_key_updated_at      TIMESTAMPTZ,
    api_key_updated_by      TEXT,
    ipn_secret_enc          TEXT,
    ipn_secret_last4        TEXT,
    ipn_secret_updated_at   TIMESTAMPTZ,
    ipn_secret_updated_by   TEXT,
    coins                   TEXT[] NOT NULL DEFAULT ARRAY['usdttrc20', 'usdtbsc', 'usdterc20', 'btc', 'eth', 'ltc', 'trx', 'bnbbsc'],
    min_deposit_usd         NUMERIC NOT NULL DEFAULT 10 CHECK (min_deposit_usd > 0),
    max_deposit_usd         NUMERIC NOT NULL DEFAULT 50000 CHECK (max_deposit_usd >= min_deposit_usd),
    -- NOWPayments' network / service fees are added to what the client pays (`is_fee_paid_by_user`, fixed rate)
    fee_paid_by_user        BOOLEAN NOT NULL DEFAULT true,
    -- NULL = WALLET_NOWPAYMENTS_IPN_URL (default https://app.kalkstrade.com/api/payments/nowpayments/ipn)
    ipn_url                 TEXT,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by              TEXT
);

CREATE TABLE nowpayments_payments (
    id                      BIGSERIAL PRIMARY KEY,
    tenant_id               BIGINT NOT NULL REFERENCES tenants(id),
    user_id                 BIGINT NOT NULL,
    -- our id, sent to NOWPayments as order_id ("np_" + 24 hex)
    order_id                TEXT NOT NULL UNIQUE,
    -- NOWPayments' payment_id; NULL until NOWPayments answered the create call
    np_payment_id           TEXT UNIQUE,
    -- a repeated deposit to the address of an earlier payment (NOWPayments `parent_payment_id`): always reviewed
    parent_id               BIGINT REFERENCES nowpayments_payments(id),
    sandbox                 BOOLEAN NOT NULL DEFAULT false,
    -- what the client asked to deposit, in USD; credited 1:1 as USDT when the payment finishes
    price_amount            NUMERIC NOT NULL CHECK (price_amount > 0),
    price_currency          TEXT NOT NULL DEFAULT 'usd',
    pay_currency            TEXT NOT NULL,
    pay_amount              NUMERIC,
    pay_address             TEXT,
    payin_extra_id          TEXT,                    -- memo / destination tag the client must send with the coin
    network                 TEXT,
    fee_paid_by_user        BOOLEAN NOT NULL DEFAULT true,
    expires_at              TIMESTAMPTZ,             -- expiration_estimate_date / valid_until
    -- NOWPayments' own status: waiting, confirming, confirmed, sending, partially_paid, finished, failed, refunded, expired
    np_status               TEXT,
    actually_paid           NUMERIC,
    actually_paid_at_fiat   NUMERIC,
    outcome_amount          NUMERIC,
    outcome_currency        TEXT,
    payin_hash              TEXT,
    -- ours: creating (create call in flight) → open → credited | review → approval (four-eyes) → credited | rejected;
    -- closed = failed / expired / refunded without money credited; error = NOWPayments refused the create call
    status                  TEXT NOT NULL CHECK (status IN ('creating', 'open', 'credited', 'review', 'approval', 'rejected', 'closed', 'error')),
    review_reason           TEXT,
    failure_reason          TEXT,
    -- what was credited (USD = USDT), the ledger txn and who booked it (system or staff)
    credit_amount           NUMERIC,
    ledger_txn_id           BIGINT REFERENCES ledger_txns(id),
    credited_at             TIMESTAMPTZ,
    credited_by             TEXT,
    -- four-eyes: the first staff member's proposal, waiting for a second one with finance.adjust_approve
    approval_amount         NUMERIC,
    approval_reason         TEXT,
    approval_by_id          TEXT,
    approval_by_name        TEXT,
    approval_at             TIMESTAMPTZ,
    decided_by              TEXT,
    decided_at              TIMESTAMPTZ,
    decision_note           TEXT,
    -- the last NOWPayments answer (GET /payment/{id}) and the last IPN body, as received (signature verified)
    np_payment              JSONB,
    last_ipn                JSONB,
    last_ipn_at             TIMESTAMPTZ,
    ipn_count               INT NOT NULL DEFAULT 0,
    checks                  INT NOT NULL DEFAULT 0,
    last_checked_at         TIMESTAMPTZ,
    check_error             TEXT,
    ip                      TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX nowpayments_payments_open_idx ON nowpayments_payments (last_checked_at NULLS FIRST) WHERE status IN ('open', 'creating');
CREATE INDEX nowpayments_payments_tenant_idx ON nowpayments_payments (tenant_id, created_at DESC);
CREATE INDEX nowpayments_payments_user_idx ON nowpayments_payments (tenant_id, user_id, created_at DESC);
CREATE INDEX nowpayments_payments_queue_idx ON nowpayments_payments (tenant_id) WHERE status IN ('review', 'approval');

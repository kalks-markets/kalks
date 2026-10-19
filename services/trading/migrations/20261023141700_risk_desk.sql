-- Dealer risk desk (docs/RISK-DESK.md): risk limits and settings per broker, client profitability scores (history
-- kept), routing suggestions the dealer approves or rejects, and risk alerts. Standard broker risk management only:
-- nothing here touches prices.

-- One row per limit: kind `exposure` (scope = symbol or '*'), `daily_loss` (scope ''), `group` (scope = group code),
-- `news` (scope ''). `params` is the limit as the API shows it; `mode` alert (default) or enforce.
CREATE TABLE IF NOT EXISTS risk_limits (
    tenant_id   BIGINT NOT NULL REFERENCES tenants(id),
    kind        TEXT NOT NULL CHECK (kind IN ('exposure', 'daily_loss', 'group', 'news')),
    scope       TEXT NOT NULL DEFAULT '',
    params      JSONB NOT NULL,
    mode        TEXT NOT NULL DEFAULT 'alert' CHECK (mode IN ('alert', 'enforce')),
    enabled     BOOLEAN NOT NULL DEFAULT true,
    updated_by  TEXT NOT NULL DEFAULT '',
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, kind, scope)
);

-- Routing mode: `recommend` (launch: rules label and recommend, every trade keeps its default book) or `execute`.
CREATE TABLE IF NOT EXISTS risk_settings (
    tenant_id            BIGINT PRIMARY KEY REFERENCES tenants(id),
    routing_mode         TEXT NOT NULL DEFAULT 'recommend' CHECK (routing_mode IN ('recommend', 'execute')),
    score_interval_mins  INT NOT NULL DEFAULT 360,
    updated_by           TEXT NOT NULL DEFAULT '',
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Every computed score is kept (the latest per login is the current one).
CREATE TABLE IF NOT EXISTS client_scores (
    id           BIGSERIAL PRIMARY KEY,
    tenant_id    BIGINT NOT NULL REFERENCES tenants(id),
    login        BIGINT NOT NULL,
    user_id      BIGINT NOT NULL,
    computed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    score        NUMERIC NOT NULL,
    category     TEXT NOT NULL CHECK (category IN ('likely_profitable', 'neutral', 'likely_losing', 'toxic')),
    trades       INT NOT NULL,
    data         JSONB NOT NULL,
    trigger      TEXT NOT NULL DEFAULT 'schedule'
);
CREATE INDEX IF NOT EXISTS client_scores_login ON client_scores (tenant_id, login, computed_at DESC);

CREATE TABLE IF NOT EXISTS routing_suggestions (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   BIGINT NOT NULL REFERENCES tenants(id),
    kind        TEXT NOT NULL CHECK (kind IN ('position', 'account')),
    login       BIGINT NOT NULL,
    user_id     BIGINT NOT NULL,
    ticket      BIGINT,
    symbol      TEXT,
    volume      NUMERIC,
    from_book   CHAR(1) NOT NULL,
    to_book     CHAR(1) NOT NULL,
    reasons     JSONB NOT NULL,
    rule        TEXT,
    score       NUMERIC,
    category    TEXT,
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'rejected', 'expired')),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    decided_at  TIMESTAMPTZ,
    decided_by  TEXT,
    note        TEXT,
    result      JSONB
);
-- one open suggestion per position / account
CREATE UNIQUE INDEX IF NOT EXISTS routing_suggestions_one_pending ON routing_suggestions (tenant_id, kind, login, COALESCE(ticket, 0)) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS routing_suggestions_status ON routing_suggestions (tenant_id, status, created_at DESC);

-- Risk alerts: one open alert per key (it escalates, is acknowledged, and resolves when the condition clears).
CREATE TABLE IF NOT EXISTS risk_alerts (
    id           BIGSERIAL PRIMARY KEY,
    tenant_id    BIGINT NOT NULL REFERENCES tenants(id),
    key          TEXT NOT NULL,
    kind         TEXT NOT NULL,
    level        TEXT NOT NULL CHECK (level IN ('red', 'amber', 'info')),
    title        TEXT NOT NULL,
    message      TEXT NOT NULL,
    symbol       TEXT,
    login        BIGINT,
    data         JSONB NOT NULL DEFAULT '{}',
    count        INT NOT NULL DEFAULT 1,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    acked_at     TIMESTAMPTZ,
    acked_by     TEXT,
    resolved_at  TIMESTAMPTZ
);
CREATE UNIQUE INDEX IF NOT EXISTS risk_alerts_one_open ON risk_alerts (tenant_id, key) WHERE resolved_at IS NULL;
CREATE INDEX IF NOT EXISTS risk_alerts_recent ON risk_alerts (tenant_id, created_at DESC);

DO $$
DECLARE t TEXT;
BEGIN
    FOREACH t IN ARRAY ARRAY['risk_limits', 'risk_settings', 'client_scores', 'routing_suggestions', 'risk_alerts'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
        IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = t AND policyname = 'tenant_isolation') THEN
            EXECUTE format('CREATE POLICY tenant_isolation ON %I USING (tenant_id = NULLIF(current_setting(''kalks.tenant_id'', true), '''')::bigint)', t);
        END IF;
    END LOOP;
END $$;

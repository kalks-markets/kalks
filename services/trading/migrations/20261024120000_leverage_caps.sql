-- Risk leverage caps (docs/RISK-DESK.md › Leverage control): the dealing desk caps the leverage clients get when the
-- broker is at risk. One row per scope (all accounts, an account group, an asset class or one account); disabling keeps
-- the row (history). Effective leverage = min(account leverage, symbol max leverage, the matching enabled caps).
-- `applies_to` new_positions: new exposure only (positions keep the leverage they opened at); all_positions: open
-- positions are re-margined at once.
CREATE TABLE IF NOT EXISTS leverage_caps (
    id            BIGSERIAL PRIMARY KEY,
    tenant_id     BIGINT NOT NULL REFERENCES tenants(id),
    scope         TEXT NOT NULL CHECK (scope IN ('all', 'group', 'asset_class', 'account')),
    target        TEXT NOT NULL DEFAULT '',
    max_leverage  INT NOT NULL CHECK (max_leverage >= 1),
    applies_to    TEXT NOT NULL DEFAULT 'new_positions' CHECK (applies_to IN ('new_positions', 'all_positions')),
    enabled       BOOLEAN NOT NULL DEFAULT true,
    reason_code   TEXT NOT NULL DEFAULT '',
    note          TEXT NOT NULL DEFAULT '',
    created_by    TEXT NOT NULL DEFAULT '',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_by    TEXT NOT NULL DEFAULT '',
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, scope, target)
);

ALTER TABLE leverage_caps ENABLE ROW LEVEL SECURITY;
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_policies WHERE tablename = 'leverage_caps' AND policyname = 'tenant_isolation') THEN
        CREATE POLICY tenant_isolation ON leverage_caps USING (tenant_id = NULLIF(current_setting('kalks.tenant_id', true), '')::bigint);
    END IF;
END $$;

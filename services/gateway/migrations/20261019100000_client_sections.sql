-- Client Area sections and targeting rules (docs/SECTIONS.md).
--
-- * feature_flags gets a third kind, `section`: every Client Area page / menu entry and every dashboard block
--   (`page.wallet.deposit`, `dashboard.wallet_card`, ...; dots allowed in keys). Built-in rows are upserted by the
--   gateway on start (sections.rs BUILTIN_SECTIONS, same keys as packages/ui/src/lib/sections.ts).
-- * feature_rules: the broker's switches for a section, for everyone (`all`) or targeted by the client's country,
--   the account group of any of their trading accounts, or the client (user id). Precedence: user > group >
--   country > all > catalogue default; a section inside a switched-off module stays off.
-- * tenants.features_version: bumped on every module / flag / section / rule change, so the apps' per-client caches
--   know when to reload (it rides along with /v1/public/tenant-config).

ALTER TABLE feature_flags DROP CONSTRAINT IF EXISTS feature_flags_kind_check;
ALTER TABLE feature_flags ADD CONSTRAINT feature_flags_kind_check CHECK (kind IN ('module', 'flag', 'section'));
ALTER TABLE feature_flags DROP CONSTRAINT IF EXISTS feature_flags_key_check;
ALTER TABLE feature_flags ADD CONSTRAINT feature_flags_key_check CHECK (length(key) BETWEEN 2 AND 64 AND key ~ '^[a-z][a-z0-9_]*(\.[a-z0-9_]+)*$');

ALTER TABLE tenants ADD COLUMN features_version BIGINT NOT NULL DEFAULT 0;

CREATE TABLE feature_rules (
    id          BIGSERIAL PRIMARY KEY,
    tenant_id   BIGINT NOT NULL REFERENCES tenants(id),
    key         TEXT NOT NULL REFERENCES feature_flags(key) ON DELETE CASCADE,
    scope       TEXT NOT NULL CHECK (scope IN ('all', 'country', 'group', 'user')),
    -- '' for all, an ISO country code (lowercase), an account group code, or a user id
    value       TEXT NOT NULL DEFAULT '',
    enabled     BOOLEAN NOT NULL,
    updated_by  BIGINT REFERENCES staff(id) ON DELETE SET NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (scope = 'all' AND value = '')
        OR (scope = 'country' AND value ~ '^[a-z]{2}$')
        OR (scope = 'group' AND value ~ '^[a-z0-9-]{1,40}$')
        OR (scope = 'user' AND value ~ '^[1-9][0-9]{0,17}$')
    ),
    UNIQUE (tenant_id, key, scope, value)
);
CREATE INDEX feature_rules_tenant_idx ON feature_rules (tenant_id, scope, value);

-- row-level security, like the other tenant-scoped tables (20260929180000_tenant_domains_rls.sql)
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'kalks_tenant') THEN
        EXECUTE 'GRANT SELECT, INSERT, UPDATE, DELETE ON feature_rules TO kalks_tenant';
        EXECUTE 'GRANT USAGE, SELECT ON SEQUENCE feature_rules_id_seq TO kalks_tenant';
    END IF;
END $$;
ALTER TABLE feature_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE feature_rules FORCE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS tenant_isolation ON feature_rules;
CREATE POLICY tenant_isolation ON feature_rules USING (kalks_tenant_row(tenant_id)) WITH CHECK (kalks_tenant_row(tenant_id));

-- CFD / Options account split (Kalks 2, Track 1). An account trades ONE product, decided by its group:
-- a CFD group's accounts open CFD positions only, an Options group's accounts option positions only (house prices
-- and the order book). The engine reads the group on every operation, so no event changes and replay is untouched.
--
-- Live data before this migration (2026-10-09): no open option positions and no pending option orders anywhere, so
-- every existing group becomes a CFD group; the market maker's group `options-mm` becomes an Options group, and two
-- client-facing Options groups are added for every broker: Options Standard (enabled) and Options Pro (disabled
-- until the founder sets its numbers in Back Office › Groups and Options › Pricing).

ALTER TABLE groups ADD COLUMN IF NOT EXISTS product TEXT NOT NULL DEFAULT 'cfd';
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'groups_product_check') THEN
        ALTER TABLE groups ADD CONSTRAINT groups_product_check CHECK (product IN ('cfd', 'options'));
    END IF;
END $$;

UPDATE groups SET product = 'options' WHERE code = 'options-mm' AND product <> 'options';

-- Options groups of every broker (a broker other than the platform broker prices from its own spread groups,
-- `<slug>-standard`, like the groups it was provisioned with). Leverage does not apply to options (premiums are paid
-- in cash, sold options carry scenario margin), so one fixed value.
INSERT INTO groups (tenant_id, code, name, mode, cent, account_types, leverages, default_leverage, margin_call_pct, stop_out_pct,
                    hedged_margin_pct, min_deposit, commission_per_lot, route, spread_group, max_accounts_per_user, enabled, product)
SELECT t.id, 'options-standard', 'Options Standard', 'hedging', false, 'both', '{100}', 100, 100, 50, 50, 0, 0, 'B',
       CASE WHEN t.id = 1 THEN 'standard' ELSE t.slug || '-standard' END, 5, true, 'options'
  FROM tenants t
ON CONFLICT DO NOTHING;

-- Options Pro: lower per-contract fees (options service `group_settings`, placeholder 0.15 USD) for a higher minimum
-- deposit (placeholder 1 000 USD). Seeded disabled: never offered until the founder reviews the numbers and enables it.
INSERT INTO groups (tenant_id, code, name, mode, cent, account_types, leverages, default_leverage, margin_call_pct, stop_out_pct,
                    hedged_margin_pct, min_deposit, commission_per_lot, route, spread_group, max_accounts_per_user, enabled, product)
SELECT t.id, 'options-pro', 'Options Pro', 'hedging', false, 'both', '{100}', 100, 100, 50, 50, 1000, 0, 'B',
       CASE WHEN t.id = 1 THEN 'standard' ELSE t.slug || '-standard' END, 5, false, 'options'
  FROM tenants t
ON CONFLICT DO NOTHING;

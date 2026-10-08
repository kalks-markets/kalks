-- CFD / Options account split (Kalks 2, Track 1): the engine's new Options groups `options-standard` (the platform
-- default row `*` applies) and `options-pro`, whose lower per-contract fees are a PLACEHOLDER until the founder sets
-- them in Back Office › Options › Pricing (the engine group is seeded disabled, so nobody trades on it before then).
INSERT INTO group_settings (tenant, group_code, symbol, commission_per_contract, maker_fee_per_contract, taker_fee_per_contract, updated_by)
VALUES ('kalks', 'options-pro', '*', 0.15, -0.05, 0.15, 'seed')
ON CONFLICT DO NOTHING;

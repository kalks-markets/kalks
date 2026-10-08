-- CFD / Options account split (Kalks 2, Track 1): the product an account trades, mirrored from the engine's account
-- view (`product`: its group's product). Every account synced before the split is a CFD account.
ALTER TABLE accounts ADD COLUMN IF NOT EXISTS product TEXT NOT NULL DEFAULT 'cfd';

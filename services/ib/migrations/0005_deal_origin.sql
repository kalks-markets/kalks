-- Where a deal came from, for the ledger label: `copy` (a copy-trading follower fill), `mam` (a MAM allocation),
-- `pamm` (a PAMM fund share pushed by the engine) or `engine` (any other engine deal, and older feeds without a
-- source). `deals.source` / `commissions.deal_source` stay the feed that delivered the deal (`engine` | `pamm` |
-- `copy`): they are the idempotency key, so a deal already processed as `engine` is never paid again as `copy`.

ALTER TABLE deals ADD COLUMN IF NOT EXISTS origin TEXT;
UPDATE deals SET origin = source WHERE origin IS NULL;
ALTER TABLE deals ALTER COLUMN origin SET DEFAULT 'engine';
ALTER TABLE deals ALTER COLUMN origin SET NOT NULL;

ALTER TABLE commissions ADD COLUMN IF NOT EXISTS origin TEXT;   -- NULL on lines without a deal (CPA, adjustments)
UPDATE commissions SET origin = deal_source WHERE origin IS NULL AND deal_source IS NOT NULL;

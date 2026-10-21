-- Erasing a closed client's personal data (GDPR right to erasure, client_erase.rs).
--
-- A regulated broker keeps trading, wallet and IB records for years, so a client is never deleted: staff with
-- `clients.erase` replace the personal columns with placeholders and remove sign-in data, KYC files and the rest
-- (the full list is in client_erase.rs). users.erased_at marks the record. The id, the referral code and the
-- referral links stay, so the records other services keep under the user id stay consistent.
ALTER TABLE users ADD COLUMN IF NOT EXISTS erased_at TIMESTAMPTZ;

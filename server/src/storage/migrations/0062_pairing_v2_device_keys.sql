-- CDXC:RemotePairing 2026-11-08 DECISION:
-- User (GitHub #495, Q1721 "device keys + pending + match"): every phone pairs with its own
-- device key, and its device token is bound to that key and lapses unless the key renews it.
-- Phones paired before 11.0.1 have no device key: they become `legacy` rows, their API tokens
-- stop working at once, and gxserver removes their unrestricted SSH key lines at start
-- (remote_access/legacy_pairings.rs). The rows stay listed as "pair again" until the phone pairs
-- again or the user removes them.
ALTER TABLE remote_paired_devices ADD COLUMN status TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE remote_paired_devices ADD COLUMN deviceKeyAlgorithm TEXT;
ALTER TABLE remote_paired_devices ADD COLUMN deviceKeyPublic TEXT;
ALTER TABLE remote_paired_devices ADD COLUMN deviceKeyId TEXT;
ALTER TABLE remote_paired_devices ADD COLUMN apiTokenExpiresAt TEXT;
ALTER TABLE remote_paired_devices ADD COLUMN approverKeyId TEXT;
-- 1 once the row's SSH key line is gone (or it never had a restricted one).
ALTER TABLE remote_paired_devices ADD COLUMN legacyKeyLineRemoved INTEGER NOT NULL DEFAULT 0;
UPDATE remote_paired_devices SET apiTokenHash = NULL WHERE status = 'legacy';

PRAGMA user_version = 62;

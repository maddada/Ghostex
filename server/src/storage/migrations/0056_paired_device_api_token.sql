-- CDXC:RemotePairing 2026-10-25 WHY:
-- Each phone paired through Easy Connect gets its own API token, stored only as a SHA-256 hash,
-- so removing the phone from Paired devices also revokes its API access. Phones paired before
-- this have no token and keep reading the computer's main token over SSH.
ALTER TABLE remote_paired_devices ADD COLUMN apiTokenHash TEXT;

PRAGMA user_version = 56;

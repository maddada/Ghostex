-- CDXC:RemotePairing 2026-11-06 DECISION:
-- User (Q1490 B): Easy Connect's second address uses Ghostex's own relay by default, with the
-- public relays behind it; Settings can pick public relays only or a custom relay hostname
-- (server/src/tailcat/types.rs, `TailcatRelaySetting`). A computer that had Easy Connect before
-- this gets the default, which adds the second address and leaves the first one as it was.
ALTER TABLE tailcat_state ADD COLUMN relayMode TEXT NOT NULL DEFAULT 'ghostex';
ALTER TABLE tailcat_state ADD COLUMN relayCustomHost TEXT NOT NULL DEFAULT '';

PRAGMA user_version = 61;

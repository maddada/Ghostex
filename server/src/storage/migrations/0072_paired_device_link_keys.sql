-- CDXC:RemotePairing 2026-11-10 WHY:
-- A phone or computer with several Easy Connect computers dialed every one of them with its one
-- tailcat client key, and a relay delivers a key's traffic only to its newest connection, so each
-- new tunnel starved the others for 15 to 30 s (measured 2026-11-09 on Ghostex's relay). A paired
-- device may now register a second key of its own for this computer (`/api/registerEasyConnectClientKey`),
-- which goes on the allow-list beside its pairing key and leaves with its row. One per device.
CREATE TABLE IF NOT EXISTS remote_paired_device_link_keys (
  deviceId TEXT PRIMARY KEY NOT NULL CHECK (deviceId <> ''),
  clientKey TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

PRAGMA user_version = 72;

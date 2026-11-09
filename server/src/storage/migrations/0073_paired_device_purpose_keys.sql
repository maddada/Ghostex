-- CDXC:RemotePairing 2026-11-10 WHY:
-- A paired computer reaches this one with two tailcat clients of its own (its desktop app's forward
-- and its gxserver's tunnels), and two clients sharing a key stall each other, so it registers one
-- key per purpose (`app`, `server`), signed with the pairing device key it already has
-- (`/api/registerComputerEasyConnectKeys`). They go on the allow-list beside its pairing key, which
-- stays valid, and leave with its row. A phone's own key stays in remote_paired_device_link_keys.
CREATE TABLE IF NOT EXISTS remote_paired_device_purpose_keys (
  deviceId TEXT NOT NULL CHECK (deviceId <> ''),
  purpose TEXT NOT NULL CHECK (purpose <> ''),
  clientKey TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  PRIMARY KEY (deviceId, purpose)
);

PRAGMA user_version = 73;

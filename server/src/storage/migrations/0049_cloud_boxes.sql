-- CDXC:CloudBoxes 2026-11-02 WHY:
-- Home gxserver's Cloud Boxes registry. The provider's own box list stays the source of truth
-- (every box carries Ghostex's metadata, so any client can rebuild it); these tables keep what a
-- provider can't say: progress-free box rows with their cached sessions, cost and delete dates,
-- the sealed provider keys, the project snapshots with their Default, and this month's spend.
-- Each row is its wire object as JSON (`rowJson`, gx-protocol `cloud_box`) plus the columns the
-- schedulers look up by; secrets live only in the `*Sealed` columns, sealed with the vault.
CREATE TABLE IF NOT EXISTS cloud_providers (
  provider TEXT PRIMARY KEY NOT NULL,
  rowJson TEXT NOT NULL,
  keySealed TEXT,
  refreshSealed TEXT,
  keyId TEXT,
  updatedAt TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cloud_boxes (
  boxId TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL,
  providerBoxId TEXT,
  state TEXT NOT NULL,
  homeProjectId TEXT,
  idempotencyKey TEXT,
  deleteAfter TEXT,
  tokenSealed TEXT,
  rowJson TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cloud_snapshots (
  snapshotId TEXT PRIMARY KEY NOT NULL,
  provider TEXT NOT NULL,
  projectKey TEXT NOT NULL,
  isDefault INTEGER NOT NULL DEFAULT 0 CHECK (isDefault IN (0, 1)),
  expiresAt TEXT,
  rowJson TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cloud_spend (
  month TEXT NOT NULL,
  provider TEXT NOT NULL,
  rowJson TEXT NOT NULL,
  readAt TEXT NOT NULL,
  PRIMARY KEY (month, provider)
);

PRAGMA user_version = 49;

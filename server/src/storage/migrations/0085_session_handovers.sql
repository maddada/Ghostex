-- Additive session handover lineage table for cross-session handovers (CLI <-> ACP <-> ACP)
-- and delta handovers into an existing live session. No foreign keys so pruned or rebuilt
-- sessions never cascade-delete handover history.
CREATE TABLE IF NOT EXISTS session_handovers (
  handoverId TEXT PRIMARY KEY,
  fromProjectId TEXT NOT NULL,
  fromSessionId TEXT NOT NULL,
  toProjectId TEXT NOT NULL,
  toSessionId TEXT NOT NULL,
  fromAgent TEXT NOT NULL,
  toAgent TEXT NOT NULL,
  mode TEXT NOT NULL,
  preset TEXT NOT NULL DEFAULT 'default',
  fromTurnCount INTEGER NOT NULL DEFAULT 0,
  exportedTurns INTEGER NOT NULL DEFAULT 0,
  exportPath TEXT,
  createdAt TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_handovers_from
  ON session_handovers(fromProjectId, fromSessionId, createdAt);

CREATE INDEX IF NOT EXISTS idx_session_handovers_to
  ON session_handovers(toProjectId, toSessionId, createdAt);

PRAGMA user_version = 85;

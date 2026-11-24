-- Additive: when gxserver restarted a session's agent because its Native host died mid-turn, so
-- the chat can mark the cut-off turn ("Stopped when the agent restarted",
-- server/src/session_chat_agent_restarts.rs). No foreign keys, like session_handovers.
CREATE TABLE IF NOT EXISTS session_agent_restarts (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  restartedAtMs INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_agent_restarts_session
  ON session_agent_restarts (projectId, sessionId, restartedAtMs);

PRAGMA user_version = 87;

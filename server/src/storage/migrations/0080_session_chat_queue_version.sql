-- Monotonic per-session queue version and per-prompt version for unambiguous queue ordering.
CREATE TABLE IF NOT EXISTS session_chat_queue_versions (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  version INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (projectId, sessionId)
);

ALTER TABLE session_chat_queued_prompts ADD COLUMN version INTEGER NOT NULL DEFAULT 1;

PRAGMA user_version = 80;

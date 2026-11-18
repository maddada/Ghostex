-- A new coordinator's first message, typed into its agent's input box but not sent until the
-- user presses Start (session_chat_first_message.rs). One row per session; gone once it is sent.
CREATE TABLE IF NOT EXISTS session_chat_first_messages (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  text TEXT NOT NULL,
  state TEXT NOT NULL DEFAULT 'waiting',
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  PRIMARY KEY (projectId, sessionId)
);

PRAGMA user_version = 82;

-- Message IDs (chat_message_numbers.rs): the number each message the user wrote got in its chat
-- (12 for M12), with its words for `ghostex message show` and the quote a later reference sends.
-- Numbers are never handed out twice in a chat; a fork's copied messages take their source's.
CREATE TABLE IF NOT EXISTS chat_message_numbers (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  number INTEGER NOT NULL,
  messageId TEXT NOT NULL,
  sentAt INTEGER,
  textHash TEXT NOT NULL DEFAULT '',
  text TEXT NOT NULL DEFAULT '',
  createdAt TEXT NOT NULL,
  PRIMARY KEY (projectId, sessionId, number)
);

-- New table, so a unique index is safe to go back from (ai/database-rules.md, rule 3).
CREATE UNIQUE INDEX IF NOT EXISTS chat_message_numbers_message
  ON chat_message_numbers (projectId, sessionId, messageId);

-- A fork's copy of a message is found by when it was sent and what it says.
CREATE INDEX IF NOT EXISTS chat_message_numbers_copy
  ON chat_message_numbers (sentAt, textHash);

PRAGMA user_version = 83;

-- Message IDs (chat_message_numbers.rs): the number each message the user wrote got in its chat
-- (12 for M12). Only the start of its words is kept (about 1 KB, for the quote a later reference
-- sends, with the whole message's line and character counts); `ghostex message show` reads the
-- whole message from the session's transcript. Numbers are never handed out twice in a chat; a
-- fork's copied messages take their source's. The rows go with their session.
CREATE TABLE IF NOT EXISTS chat_message_numbers (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  number INTEGER NOT NULL,
  messageId TEXT NOT NULL,
  sentAt INTEGER NOT NULL,
  textHash TEXT NOT NULL,
  quote TEXT NOT NULL DEFAULT '',
  lineCount INTEGER NOT NULL DEFAULT 0,
  charCount INTEGER NOT NULL DEFAULT 0,
  createdAt TEXT NOT NULL,
  PRIMARY KEY (projectId, sessionId, number)
);

-- One message, one number: a resumed agent conversation that copies a message under a new id is
-- still the same message (same send time, same words). New table, so a unique index is safe to go
-- back from (ai/database-rules.md, rule 3).
CREATE UNIQUE INDEX IF NOT EXISTS chat_message_numbers_message
  ON chat_message_numbers (projectId, sessionId, sentAt, textHash);

-- A row of the transcript to its number.
CREATE INDEX IF NOT EXISTS chat_message_numbers_id
  ON chat_message_numbers (projectId, sessionId, messageId);

-- A fork's copy of a message is found by when it was sent and what it says.
CREATE INDEX IF NOT EXISTS chat_message_numbers_copy
  ON chat_message_numbers (sentAt, textHash);

PRAGMA user_version = 83;

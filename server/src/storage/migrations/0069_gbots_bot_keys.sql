-- CDXC:GBots 2026-11-09 WHY:
-- The GBot API (gbots/api/): an app or script on this computer messages a GBot with that GBot's
-- key and gets its reply. `gbots_bot_keys` keeps each key's SHA-256 only (the key is shown once);
-- a GBot's API is on while it has a key that is not revoked, so "New key" revokes the old row and
-- adds a new one, and turning it off revokes the live one. `gbots_api_messages` is one row per
-- message an app sent: the chat it went to and the marker its message carries in that chat, so
-- the app can ask for the reply later (`202 working`) and after a gxserver restart. `shareMemory`
-- lets the key's chats read the GBot's memory and the shared preferences (off unless the user
-- turns it on).
CREATE TABLE IF NOT EXISTS gbots_bot_keys (
  keyId TEXT PRIMARY KEY,
  botId TEXT NOT NULL,
  hash TEXT NOT NULL UNIQUE,
  label TEXT NOT NULL DEFAULT '',
  createdAt TEXT NOT NULL,
  createdBy TEXT NOT NULL DEFAULT 'user',
  lastUsedAt TEXT,
  uses INTEGER NOT NULL DEFAULT 0,
  revokedAt TEXT,
  shareMemory INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_gbots_bot_keys_bot ON gbots_bot_keys (botId, revokedAt);

CREATE TABLE IF NOT EXISTS gbots_api_messages (
  messageId TEXT PRIMARY KEY,
  keyId TEXT NOT NULL,
  botId TEXT NOT NULL,
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  marker TEXT NOT NULL,
  kind TEXT NOT NULL DEFAULT 'message',
  createdAt TEXT NOT NULL,
  reply TEXT,
  repliedAt TEXT
);

CREATE INDEX IF NOT EXISTS idx_gbots_api_messages_key ON gbots_api_messages (keyId, createdAt);

PRAGMA user_version = 69;

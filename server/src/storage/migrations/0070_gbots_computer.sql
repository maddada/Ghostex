-- CDXC:GBots 2026-11-09 WHY:
-- A computer per GBot (server/src/gbots/computer/): `gbots_bots.computer` (from 0065) says where
-- the GBot's chats run (`this`, `computer:<serverId>`, `cloudbox:<boxId>`); these columns keep
-- the name shown while that computer is away, the folder its chats work in there, and whether it
-- may use the computer's screen and apps. `gbots_remote_chats` is one row per chat of a GBot that
-- runs on another computer: only this computer knows whose chat that session is, so it is what
-- lets that chat read its brief and memory here and what nests it under the GBot in Main Hub.
ALTER TABLE gbots_bots ADD COLUMN computerName TEXT;
ALTER TABLE gbots_bots ADD COLUMN computerFolder TEXT;
ALTER TABLE gbots_bots ADD COLUMN computerUse INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS gbots_remote_chats (
  serverId TEXT NOT NULL,
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  botId TEXT NOT NULL,
  openedBy TEXT NOT NULL,
  purpose TEXT NOT NULL DEFAULT 'chat',
  fromKey TEXT,
  standing INTEGER NOT NULL DEFAULT 0,
  title TEXT NOT NULL DEFAULT '',
  createdAt TEXT NOT NULL,
  closedAt TEXT,
  PRIMARY KEY (serverId, projectId, sessionId)
);

PRAGMA user_version = 70;

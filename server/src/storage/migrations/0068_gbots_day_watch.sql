-- CDXC:GBots 2026-11-09 WHY:
-- "Keep an eye on my day" (gbots/day_watch/): `appsReadOnly` marks a GBot whose chats may only read
-- the user's connected apps; gxserver refuses any connector call of theirs that would send, post,
-- create, change or delete, and gives them no `gh`/`gws` sign-ins. `gbots_day_watch` is the one
-- row that remembers the day watch on this computer: its GBot, its two routines (the morning brief
-- and the watch), and when Chief of Staff offered it, so it is offered once.
ALTER TABLE gbots_bots ADD COLUMN appsReadOnly INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS gbots_day_watch (
  id TEXT PRIMARY KEY,
  botId TEXT,
  briefRoutineId TEXT,
  watchRoutineId TEXT,
  offeredAt TEXT,
  setUpAt TEXT,
  setUpBy TEXT,
  updatedAt TEXT NOT NULL
);

PRAGMA user_version = 68;

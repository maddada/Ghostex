-- CDXC:GBots 2026-11-09 WHY:
-- One row per problem a GBot's chat hit that Chief of Staff hears about (gbots/problems/): a chat
-- that could not start, failed (Ghostex's own restarts gave up), stalled with no output, a routine
-- run that failed, or a healthy chat waiting on something only a person or Chief of Staff can lift
-- (`needsYou`: a question or approval, or a flag the GBot raised itself). `attempts` counts Chief of
-- Staff's retries and re-briefs (two at most); `ghostexRetried` is Ghostex's own one retry of a plain
-- failure before Chief of Staff hears of it. `promptKey` is the prompt a `needsYou` row waits on, so
-- one prompt is reported once. `reportedTo` is Chief of Staff's chat, `user` (a notification, when
-- there is no Chief of Staff to tell) or `muted`; `reportDueAt` holds a report Ghostex's own retry
-- may still make unneeded, or one waiting to be sent again.
CREATE TABLE IF NOT EXISTS gbots_problems (
  problemId TEXT PRIMARY KEY,
  botId TEXT NOT NULL,
  chatSessionKey TEXT,
  automationId TEXT,
  kind TEXT NOT NULL,
  source TEXT,
  detail TEXT NOT NULL DEFAULT '',
  userOnly INTEGER NOT NULL DEFAULT 0,
  promptKey TEXT,
  attempts INTEGER NOT NULL DEFAULT 0,
  ghostexRetried INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'open',
  reportedTo TEXT,
  reportedAt TEXT,
  reportDueAt TEXT,
  reportTries INTEGER NOT NULL DEFAULT 0,
  retriedAt TEXT,
  closedBy TEXT,
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

PRAGMA user_version = 67;

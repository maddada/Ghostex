-- CDXC:GBots 2026-11-09 WHY:
-- A GBot's routines are ordinary Automations in the bot's project, so the scheduler, run history and
-- the Automate view keep working for them; these columns only add what a routine needs on top.
-- `ownerBotId` makes each run a chat of that GBot, `precheckJson` is the cheap check that runs as
-- plain code before any model turn (a quiet tick starts nothing), `deliver` says where a result
-- goes (`chat`, or `chief` for Chief of Staff's chat), `lastCheckFingerprint` is what the check saw
-- last and `failureStreak` backs off a check that keeps failing. Every column is optional, so an
-- existing automation is untouched. `gbots_routine_checks` holds the rest of a routine's moving
-- state: how its chats continue, its standing chat, and its last check.
ALTER TABLE automations ADD COLUMN ownerBotId TEXT;
ALTER TABLE automations ADD COLUMN precheckJson TEXT;
ALTER TABLE automations ADD COLUMN deliver TEXT;
ALTER TABLE automations ADD COLUMN lastCheckFingerprint TEXT;
ALTER TABLE automations ADD COLUMN failureStreak INTEGER NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS gbots_routine_checks (
  automationId TEXT PRIMARY KEY,
  continuity TEXT NOT NULL DEFAULT 'new',
  standingChatKey TEXT,
  lastCheckAt TEXT,
  lastCheckOutcome TEXT,
  lastCheckNote TEXT,
  quietChecks INTEGER NOT NULL DEFAULT 0,
  retryAt TEXT,
  createdBy TEXT NOT NULL DEFAULT 'user',
  FOREIGN KEY (automationId) REFERENCES automations(automationId) ON DELETE CASCADE
);

PRAGMA user_version = 66;

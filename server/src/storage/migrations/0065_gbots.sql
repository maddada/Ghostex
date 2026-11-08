-- CDXC:GBots 2026-11-08 DECISION:
-- User: "we are doing our own bots thing." A GBot is a lasting persona (name, title, standing
-- instructions, look, agent, approval level) whose chats are ordinary coordinator sessions in the
-- bot's own project at its folder (server/src/gbots/). This row is the truth for the persona;
-- `SOUL.md` in the bot folder only mirrors `soul`. Departments, assignments (work handed to a bot)
-- and changes (what was added or edited, for Undo) are reserved here so later steps add no table.
-- Bot names are unique per computer, case-blind (checked in Rust for non-ASCII letters too).
CREATE TABLE IF NOT EXISTS gbots_bots (
  botId TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  description TEXT NOT NULL DEFAULT '',
  soul TEXT NOT NULL DEFAULT '',
  icon TEXT NOT NULL DEFAULT 'robot',
  color TEXT NOT NULL DEFAULT 'periwinkle',
  agentId TEXT NOT NULL,
  model TEXT,
  effort TEXT,
  approval TEXT NOT NULL DEFAULT 'ask',
  departmentId TEXT,
  isLead INTEGER NOT NULL DEFAULT 0,
  computer TEXT NOT NULL DEFAULT 'this',
  projectId TEXT NOT NULL DEFAULT '',
  workFolder TEXT,
  memoryEnabled INTEGER NOT NULL DEFAULT 1,
  pausedAt TEXT,
  createdBy TEXT NOT NULL,
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_gbots_bots_name ON gbots_bots(name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS gbots_departments (
  departmentId TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  icon TEXT NOT NULL DEFAULT '',
  leadBotId TEXT,
  instructions TEXT NOT NULL DEFAULT '',
  sortOrder INTEGER NOT NULL DEFAULT 0,
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS gbots_assignments (
  assignmentId TEXT PRIMARY KEY,
  rootId TEXT NOT NULL,
  parentId TEXT,
  depth INTEGER NOT NULL DEFAULT 0,
  fromSessionKey TEXT NOT NULL,
  botId TEXT NOT NULL,
  chatSessionKey TEXT NOT NULL,
  mode TEXT NOT NULL DEFAULT 'work',
  brief TEXT NOT NULL DEFAULT '',
  doneWhen TEXT NOT NULL DEFAULT '',
  reworks INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'open',
  createdAt TEXT NOT NULL,
  settledAt TEXT
);

CREATE INDEX IF NOT EXISTS idx_gbots_assignments_bot ON gbots_assignments(botId);
CREATE INDEX IF NOT EXISTS idx_gbots_assignments_root ON gbots_assignments(rootId);

CREATE TABLE IF NOT EXISTS gbots_changes (
  changeId TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  subjectId TEXT NOT NULL DEFAULT '',
  beforeJson TEXT,
  afterJson TEXT,
  byWho TEXT NOT NULL,
  at TEXT NOT NULL,
  undoneAt TEXT
);

CREATE INDEX IF NOT EXISTS idx_gbots_changes_at ON gbots_changes(at);

PRAGMA user_version = 65;

-- CDXC:LeadLinks 2026-11-20 WHY:
-- GitHub #678: an ordinary session that leads others (not a coordinator) gets its linked sessions'
-- turn-end reports and quiet notes without the coordinator role. A side table keyed by the linked
-- session, like `coordinator_threads` (0041) and for the same reason: no foreign key, so a pruned
-- or rebuilt `sessions` row never takes a link with it. It is not a coordinator thread row on
-- purpose: forty-odd readers treat that row as "a coordinator's thread" (work items, needs-you,
-- relayed cards, auto-sleep), and a linked session stays an ordinary session for all of them.
-- `owedSince` is when the lead last sent it a message: a report is owed only for a turn the lead
-- started (the user's own turns in it are never forwarded); a Finished report clears it.
-- `observedWorking`, `reportedPromptKey` and `reportedQuietSince` are the watcher's bookkeeping,
-- written after a report was delivered so a restart never repeats one.
CREATE TABLE IF NOT EXISTS lead_links (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  leadProjectId TEXT NOT NULL,
  leadSessionId TEXT NOT NULL,
  owedSince TEXT,
  observedWorking INTEGER NOT NULL DEFAULT 0 CHECK (observedWorking IN (0, 1)),
  reportedAt TEXT,
  reportedPromptKey TEXT,
  reportedQuietSince INTEGER,
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  PRIMARY KEY (projectId, sessionId)
);

CREATE INDEX IF NOT EXISTS idx_lead_links_lead ON lead_links(leadProjectId, leadSessionId);

PRAGMA user_version = 84;

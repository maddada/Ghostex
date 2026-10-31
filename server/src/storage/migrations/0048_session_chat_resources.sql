-- CDXC:SessionChat 2026-10-30 WHY:
-- The Resources pill counted only the transcript rows the chat had loaded (the newest 300
-- messages), so a long conversation showed 42 resources where it held several hundred. This table
-- is the session's resource index: every question form, page and chart the conversation held, one
-- JSON array per session, kept apart from the sessions row because the sessions table is read all
-- the time and the index is not. entriesJson is the whole index; pagesJson is its pages only,
-- which a coordinator's chat lists for each of its threads without parsing every thread's whole
-- index. Entries are only added or updated, never dropped when a transcript is compacted or
-- rewritten (gx-chat-core questions/resource_index.rs `merge`).
CREATE TABLE IF NOT EXISTS session_chat_resources (
  projectId TEXT NOT NULL,
  sessionId TEXT NOT NULL,
  entriesJson TEXT NOT NULL,
  pagesJson TEXT NOT NULL,
  entryCount INTEGER NOT NULL,
  updatedAt TEXT NOT NULL,
  PRIMARY KEY (projectId, sessionId)
);

PRAGMA user_version = 48;

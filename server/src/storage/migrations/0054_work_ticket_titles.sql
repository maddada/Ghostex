-- CDXC:WorkMode 2026-11-02 WHY:
-- A ticket command's tickets keep their last title on a session's card while the command can't
-- be asked (moved away, cleared, a required credential unset), after a gxserver restart too. The
-- titles are their own rows, not part of the ticket command record (a metadata row that also
-- holds up to 128 icons), so a changed title is one row's write. At most 2,000 rows are kept, by
-- when their title last changed (`updatedAt`), not when it was last read: a ticket shown every day
-- whose title never changes goes first. Deleting a workspace deletes its rows.
CREATE TABLE IF NOT EXISTS work_ticket_titles (
  workspaceId TEXT NOT NULL,
  sourceId TEXT NOT NULL,
  ticketId TEXT NOT NULL,
  title TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  PRIMARY KEY (workspaceId, sourceId, ticketId)
);

PRAGMA user_version = 54;

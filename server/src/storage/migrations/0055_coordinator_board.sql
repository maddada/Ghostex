-- CDXC:Coordinators 2026-11-05 WHY:
-- A coordinator's board lists its lines of work by who has the ball (server/src/coordinators/board.rs).
-- Threads land on it from their own state; this table holds what the coordinator places by hand:
-- items that are not threads (help text held for a release, a sign-in only the user can do, the
-- next release, an idea), and a thread it moved to a group its state cannot tell (`threadSessionId`
-- set: Blocked on a CI token, Needs you for a test). `itemNumber` is the item's B<number> and is
-- never reused within a coordinator. Done items stay for a while, so a board done today can be
-- read again, and are pruned after two weeks.
CREATE TABLE IF NOT EXISTS coordinator_board_items (
  coordinatorProjectId TEXT NOT NULL,
  coordinatorSessionId TEXT NOT NULL,
  itemNumber INTEGER NOT NULL,
  title TEXT NOT NULL DEFAULT '',
  boardGroup TEXT NOT NULL,
  reason TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  link TEXT NOT NULL DEFAULT '',
  release TEXT NOT NULL DEFAULT '',
  untested INTEGER NOT NULL DEFAULT 0 CHECK (untested IN (0, 1)),
  threadServerId TEXT NOT NULL DEFAULT '',
  threadProjectId TEXT NOT NULL DEFAULT '',
  threadSessionId TEXT NOT NULL DEFAULT '',
  createdAt TEXT NOT NULL,
  updatedAt TEXT NOT NULL,
  doneAt TEXT,
  PRIMARY KEY (coordinatorProjectId, coordinatorSessionId, itemNumber)
);

PRAGMA user_version = 55;

-- CDXC:Coordinators 2026-11-05 WHY:
-- The board's two-click × moves a row into a folded Removed group, from which Restore brings it
-- back (server/src/coordinators/board.rs). `removedAt` marks an item, or a thread's row, removed;
-- a thread's row then exists only to hide it, with an empty `boardGroup` when it was never moved.
-- Removed rows are deleted after seven days.
ALTER TABLE coordinator_board_items ADD COLUMN removedAt TEXT;

PRAGMA user_version = 56;

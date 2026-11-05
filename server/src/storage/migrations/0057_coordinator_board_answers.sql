-- CDXC:Coordinators 2026-11-05 WHY:
-- A board item that asks the user something carries its answers, which its card shows as one-click
-- choices (server/src/coordinators/board.rs): comma-separated labels, the recommended one marked
-- with a trailing `*`.
ALTER TABLE coordinator_board_items ADD COLUMN answers TEXT NOT NULL DEFAULT '';

PRAGMA user_version = 57;

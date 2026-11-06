-- CDXC:Coordinators 2026-11-06 DECISION:
-- User, of the coordinator board's cards: "i like the tags we already have for sidebar sessions
-- want the same applied here just to tag stuff as i want u know?" An item carries one of the
-- sidebar's session tags (a built-in id or a `custom-` id); a thread's card shows its session's own
-- `sessionTag` instead (server/src/coordinators/board.rs, op `tag`).
ALTER TABLE coordinator_board_items ADD COLUMN tag TEXT NOT NULL DEFAULT '';

PRAGMA user_version = 60;

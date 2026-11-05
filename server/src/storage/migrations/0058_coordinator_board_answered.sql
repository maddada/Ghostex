-- CDXC:Coordinators 2026-11-05 WHY:
-- One click on a board card's answer sent the coordinator "B7: Yes" once per click, three times
-- for one decision. The item records the answer it got, so gxserver refuses a second one from any
-- client, and every card shows it locked (server/src/coordinators/board.rs, op `answer`).
ALTER TABLE coordinator_board_items ADD COLUMN answered TEXT NOT NULL DEFAULT '';

PRAGMA user_version = 58;

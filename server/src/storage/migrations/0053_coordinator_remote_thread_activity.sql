-- CDXC:RemoteMachines 2026-11-03 WHY:
-- A thread on another computer counts as having moved since its last finished report when that
-- computer's own activity time for it is newer than the one the last check saw. Comparing that
-- computer's time with `reportedAt` (this computer's clock) missed short turns whenever the other
-- computer's clock ran behind. `checkedActivityMs` is the other computer's activity time (ms) the
-- last finished check covered, reported or not.
ALTER TABLE coordinator_remote_threads ADD COLUMN checkedActivityMs INTEGER;

PRAGMA user_version = 53;

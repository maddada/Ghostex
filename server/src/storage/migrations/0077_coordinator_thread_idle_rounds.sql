-- CDXC:Coordinators 2026-11-12 WHY:
-- An idle thread's watch (coordinators/idle_wait.rs) kept the rounds it had reported, and the
-- outage it held through, only in memory, so after a gxserver restart it reported the current
-- round again, and nudged the thread again when that round was the first (GitHub #554). How far
-- the stretch got is kept on the thread's row: the stretch it belongs to (the turn end the watch
-- counts from, in ms), where its rounds count from (that turn end, or the end of an outage after
-- it, in ms) and the rounds already reported. Older gxservers never read these columns.
ALTER TABLE coordinator_threads ADD COLUMN idleSinceMs INTEGER;
ALTER TABLE coordinator_threads ADD COLUMN idleClockFromMs INTEGER;
ALTER TABLE coordinator_threads ADD COLUMN idleReportedRounds INTEGER;

PRAGMA user_version = 77;

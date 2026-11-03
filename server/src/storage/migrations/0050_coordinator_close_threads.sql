-- CDXC:Coordinators 2026-11-02 WHY:
-- Closing a coordinator closes the threads that were open under it, and restoring the coordinator
-- restores them. `closedWithCoordinatorAt` marks the threads that close took with it, so the
-- restore knows which ones to bring back and the supervisor does not report them as closed (which
-- would mark them done) in the meantime.
ALTER TABLE coordinator_threads ADD COLUMN closedWithCoordinatorAt TEXT;

PRAGMA user_version = 50;

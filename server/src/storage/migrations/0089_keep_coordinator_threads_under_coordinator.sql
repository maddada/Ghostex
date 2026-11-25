-- CDXC:Coordinators 2026-11-25 DECISION in server/src/coordinators/thread_shelves.rs:
-- User: "i want threads that are under a coordinator to not be able to be away from it. they
-- always appear below it. and no parking/pinning for those, no need." gxserver no longer parks,
-- pins or snoozes a coordinator's thread, so this once takes the threads already parked, pinned
-- or snoozed back under their coordinator. Data only: an older gxserver reads the same columns.
UPDATE sessions
SET isParked = 0,
    isPinned = 0,
    snoozedAt = NULL,
    snoozedUntil = NULL
WHERE (isParked = 1 OR isPinned = 1 OR snoozedUntil IS NOT NULL)
  AND EXISTS (
    SELECT 1
    FROM coordinator_threads
    WHERE coordinator_threads.projectId = sessions.projectId
      AND coordinator_threads.sessionId = sessions.sessionId
  );

PRAGMA user_version = 89;

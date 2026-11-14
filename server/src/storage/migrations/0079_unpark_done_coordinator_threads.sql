-- CDXC:Coordinators 2026-11-14 DECISION:
-- User: "i want u to stop parking sessions that are created by the coordinator after they're
-- done. they should just always appear under the agent that created them, never get moved to
-- parked". Ghostex no longer parks a coordinator's thread (coordinators/endpoint.rs `set_parked`).
-- Earlier versions parked a thread when it was marked done (`resolve --keep-open`, and every
-- resolve before 2026-10-01), so this once moves those done threads back under their coordinator.
-- Ghostex only ever parked a thread it marked done, so a parked thread that is not done was parked
-- by the user and stays parked. Nothing records who parked a done one; it is unparked.
UPDATE sessions
SET isParked = 0
WHERE isParked = 1
  AND EXISTS (
    SELECT 1
    FROM coordinator_threads
    WHERE coordinator_threads.projectId = sessions.projectId
      AND coordinator_threads.sessionId = sessions.sessionId
      AND coordinator_threads.resolvedAt IS NOT NULL
  );

PRAGMA user_version = 79;

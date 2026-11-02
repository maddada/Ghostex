-- CDXC:RemoteMachines 2026-11-03 WHY:
-- A thread on another computer is reported finished when its last assistant message is a new one,
-- not only after a poll happened to see it working: a short follow-up turn between two polls, or
-- one that ended while its computer was away, was never reported. `reportedMessageId` is the
-- message the last finished report carried.
ALTER TABLE coordinator_remote_threads ADD COLUMN reportedMessageId TEXT;

PRAGMA user_version = 52;

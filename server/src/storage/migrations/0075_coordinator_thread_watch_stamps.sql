-- CDXC:Coordinators 2026-11-10 WHY:
-- `resolvedAt` keeps the first close (the board and the phone show it as the done time), so it
-- cannot say whether the user marked a thread done again while a message to it was still on its
-- way. `lastResolvedAt` is stamped by every Mark Done, and a message reopens a done thread only
-- when that stamp is no later than the message (coordinators/reopen.rs).
ALTER TABLE coordinator_threads ADD COLUMN lastResolvedAt TEXT;
-- The `sendRequestId` of the watched message (`pendingMessage`), so the sender's confirmation that
-- it arrived acknowledges that message and no other (coordinators/endpoint.rs).
ALTER TABLE coordinator_threads ADD COLUMN pendingMessageSendId TEXT;

PRAGMA user_version = 75;

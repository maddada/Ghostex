-- CDXC:Notifications 2026-11-10 WHY:
-- The notification feed overlays each item's live agent icon by finding its session with
-- `WHERE sessionId = ?1` (notification_feed/store.rs), and the sessions table is keyed by
-- (projectId, sessionId), so every lookup scanned the whole registry: 10,034 rows on the profiled
-- Mac, after every `notificationFeedChanged`, 2.6% of gxserver's busy time. The item's own
-- projectId cannot key it: the item keeps the project it was written under and a session can move
-- since (notification_feed/thread_read.rs). This index makes those lookups, and the other
-- session-id-only lookups (gbots routine runs and day watch, the feed's thread read), indexed
-- without changing any answer. Older gxservers never read it and leave it alone.
CREATE INDEX IF NOT EXISTS idx_sessions_session_id ON sessions(sessionId);

PRAGMA user_version = 76;

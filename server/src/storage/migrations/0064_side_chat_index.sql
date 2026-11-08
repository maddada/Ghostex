-- CDXC:StateSync 2026-11-08 WHY:
-- The coordinator tick finds every side chat every two seconds with
-- `instr(runtimeSettingsJson, '"sideChatParentSessionId"') > 0` (side_chats/panel.rs), which read
-- the JSON of every session row: 11 MB on a registry of 9,700 sessions holding one side chat,
-- about 1.3% of a core with nothing happening. This partial index holds only the side chats, and
-- SQLite uses it because the query repeats its `WHERE` word for word; change both together.
CREATE INDEX IF NOT EXISTS idx_sessions_side_chats ON sessions(createdAt)
  WHERE instr(runtimeSettingsJson, '"sideChatParentSessionId"') > 0;

PRAGMA user_version = 64;

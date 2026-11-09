-- CDXC:GBots 2026-11-09 WHY:
-- Calls with a GBot (server/src/gbots/call.rs, server/src/voice_agent/gbot_call/): each GBot
-- answers a call in its own voice, one of the voice agent provider's voices, picked in GBot
-- Settings. Empty means the voice agent's own voice.
ALTER TABLE gbots_bots ADD COLUMN voice TEXT;

PRAGMA user_version = 71;

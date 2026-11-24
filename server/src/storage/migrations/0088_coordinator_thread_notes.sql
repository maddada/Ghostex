-- Additive: latest push note a thread sent its coordinator via `ghostex coordinator note`,
-- recorded on the thread row so `ghostex coordinator status` shows what landed since the last
-- report. Older gxservers never read these columns.
ALTER TABLE coordinator_threads ADD COLUMN lastNote TEXT;
ALTER TABLE coordinator_threads ADD COLUMN lastNoteAt TEXT;

PRAGMA user_version = 88;

-- Track whether a notification row has been seen in the bell panel separately from whether it has been read.
-- Opening the bell marks rows seen (clearing the badge across devices) while keeping them unread (highlighted) until marked read or the panel closes.
ALTER TABLE notification_feed ADD COLUMN seenAt TEXT;

PRAGMA user_version = 86;

-- CDXC:StateSync 2026-09-13 WHY:
-- Fork projections need the entire registry, but ordinary title/status writes do not change its lineage.
-- Transactional triggers cover direct SQL and other processes; a random token cannot alias a rolled-back generation.
INSERT INTO metadata (key, value, updatedAt)
VALUES ('sessionForkRevision', lower(hex(randomblob(16))), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

CREATE TRIGGER session_fork_insert AFTER INSERT ON sessions BEGIN
  UPDATE metadata SET value = lower(hex(randomblob(16))) WHERE key = 'sessionForkRevision';
END;
CREATE TRIGGER session_fork_delete AFTER DELETE ON sessions BEGIN
  UPDATE metadata SET value = lower(hex(randomblob(16))) WHERE key = 'sessionForkRevision';
END;
CREATE TRIGGER session_fork_update
AFTER UPDATE OF projectId, sessionId, lifecycleState, restoredFromSessionId,
                providerStateJson, launchSettingsJson, runtimeSettingsJson ON sessions
WHEN OLD.projectId IS NOT NEW.projectId
  OR OLD.sessionId IS NOT NEW.sessionId
  OR OLD.lifecycleState IS NOT NEW.lifecycleState
  OR OLD.restoredFromSessionId IS NOT NEW.restoredFromSessionId
  OR (OLD.providerStateJson IS NOT NEW.providerStateJson AND
      json_extract(OLD.providerStateJson, '$.lifecycleState') IS NOT json_extract(NEW.providerStateJson, '$.lifecycleState'))
  OR (OLD.launchSettingsJson IS NOT NEW.launchSettingsJson AND
      json_extract(OLD.launchSettingsJson, '$.forkedFromSessionId') IS NOT json_extract(NEW.launchSettingsJson, '$.forkedFromSessionId'))
  OR (OLD.runtimeSettingsJson IS NOT NEW.runtimeSettingsJson AND (
      json_extract(OLD.runtimeSettingsJson, '$.forkedFromSessionId') IS NOT json_extract(NEW.runtimeSettingsJson, '$.forkedFromSessionId')
      OR json_extract(OLD.runtimeSettingsJson, '$.agentSessionId') IS NOT json_extract(NEW.runtimeSettingsJson, '$.agentSessionId')
      OR json_extract(OLD.runtimeSettingsJson, '$.sessionPersistenceProvider') IS NOT json_extract(NEW.runtimeSettingsJson, '$.sessionPersistenceProvider')
      OR json_extract(OLD.runtimeSettingsJson, '$.previousAgentSessionIds') IS NOT json_extract(NEW.runtimeSettingsJson, '$.previousAgentSessionIds')))
BEGIN
  UPDATE metadata SET value = lower(hex(randomblob(16))) WHERE key = 'sessionForkRevision';
END;
PRAGMA user_version = 37;

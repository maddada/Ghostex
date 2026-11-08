-- CDXC:StateSync 2026-11-08 WHY:
-- Idle gxserver spent about 2.5% of a core parsing the projects table again and again: the
-- coordinator tick, settings sync, session links, the git pass and lifecycle sweeps each read all
-- 318 projects (about 830 KB of JSON columns) or one project per session, although projects
-- change a few times a day. Every write to `projects` now moves `projectsRevision`, and
-- `DomainRepository` keeps the parsed projects under it (domain/repository/project_cache.rs).
-- Transactional triggers cover direct SQL and other processes; a random token cannot alias a
-- rolled-back generation. A migration that rebuilds the `projects` table must create these
-- triggers again, or the cache would keep serving the rows it read before.
INSERT OR IGNORE INTO metadata (key, value, updatedAt)
VALUES ('projectsRevision', lower(hex(randomblob(16))), strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

CREATE TRIGGER IF NOT EXISTS projects_revision_insert AFTER INSERT ON projects BEGIN
  UPDATE metadata SET value = lower(hex(randomblob(16))) WHERE key = 'projectsRevision';
END;
CREATE TRIGGER IF NOT EXISTS projects_revision_update AFTER UPDATE ON projects BEGIN
  UPDATE metadata SET value = lower(hex(randomblob(16))) WHERE key = 'projectsRevision';
END;
CREATE TRIGGER IF NOT EXISTS projects_revision_delete AFTER DELETE ON projects BEGIN
  UPDATE metadata SET value = lower(hex(randomblob(16))) WHERE key = 'projectsRevision';
END;

PRAGMA user_version = 63;

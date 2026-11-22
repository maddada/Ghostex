use super::error;
use crate::domain::DomainStateError;
use rusqlite::Connection;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use std::{collections::HashMap, fs, io, path::Path};

#[derive(PartialEq)]
struct FileStamp {
    modified: String,
    size: u64,
}

impl FileStamp {
    fn read(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        let modified: chrono::DateTime<chrono::Utc> = metadata.modified()?.into();
        Ok(Self {
            modified: modified.to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
            size: metadata.len(),
        })
    }
}

struct CachedFile {
    stamp: FileStamp,
    payload: Value,
}

/// CDXC:Sessions 2026-09-09 WHY:
/// A scan-wide timestamp misses older transcripts copied in with preserved mtimes.
/// Remember each path's nanosecond mtime and size, including rejected transcripts and Codex's title index, across server restarts.
/// Failed reads and files modified during a read must be retried; cache writes commit together with discovery receipts.
pub(super) struct ScanCache {
    files: HashMap<String, CachedFile>,
    changed: HashMap<String, CachedFile>,
}

impl ScanCache {
    pub(super) fn load(db: &Connection) -> Result<Self, DomainStateError> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS external_session_scan_cache (
                path TEXT PRIMARY KEY, modified TEXT NOT NULL,
                size INTEGER NOT NULL, payload TEXT NOT NULL
            )",
        )
        .map_err(error)?;
        let mut statement = db
            .prepare("SELECT path, modified, size, payload FROM external_session_scan_cache")
            .map_err(error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(error)?;
        let mut files = HashMap::new();
        for row in rows {
            let (path, modified, size, payload) = row.map_err(error)?;
            files.insert(
                path,
                CachedFile {
                    stamp: FileStamp { modified, size },
                    payload: serde_json::from_str(&payload).map_err(error)?,
                },
            );
        }
        Ok(Self {
            files,
            changed: HashMap::new(),
        })
    }

    pub(super) fn read<T: Serialize + DeserializeOwned>(
        &mut self,
        path: &Path,
        read: impl FnOnce() -> io::Result<Option<T>>,
    ) -> Result<Option<T>, DomainStateError> {
        let Ok(stamp) = FileStamp::read(path) else {
            return Ok(None);
        };
        let key = path.to_string_lossy().into_owned();
        if let Some(cached) = self.changed.get(&key).or_else(|| self.files.get(&key)) {
            if cached.stamp == stamp {
                return serde_json::from_value(cached.payload.clone()).map_err(error);
            }
        }
        let Ok(value) = read() else {
            return Ok(None);
        };
        if FileStamp::read(path).is_ok_and(|after| after == stamp) {
            self.changed.insert(
                key,
                CachedFile {
                    stamp,
                    payload: serde_json::to_value(&value).map_err(error)?,
                },
            );
        }
        Ok(value)
    }

    pub(super) fn save(&self, db: &Connection) -> Result<(), DomainStateError> {
        if self.changed.is_empty() {
            return Ok(());
        }
        let mut statement = db
            .prepare_cached(
                "INSERT INTO external_session_scan_cache (path, modified, size, payload)
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(path) DO UPDATE SET
             modified = excluded.modified, size = excluded.size, payload = excluded.payload",
            )
            .map_err(error)?;
        for (path, file) in &self.changed {
            statement
                .execute((
                    path,
                    &file.stamp.modified,
                    file.stamp.size,
                    file.payload.to_string(),
                ))
                .map_err(error)?;
        }
        Ok(())
    }

    pub(super) fn has_changes(&self) -> bool {
        !self.changed.is_empty()
    }
}

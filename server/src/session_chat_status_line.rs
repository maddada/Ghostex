//! The status line of this computer's own chat: each agent's context details record as the
//! desktop app saved it, which every chat read carries (`statusLinePins`), so the phone draws the
//! same line under the chat box as the desktop.
//!
//! The desktop keeps the records in its client storage (`client-storage.sqlite3` in Ghostex's
//! state folder, the `preferences` table, `apps/desktop/src/app/gx_chat/storage_backend.rs`).
//! gxserver opens it read-only and never writes it. A computer with no such file (no desktop app
//! there, a test server's own folder) sends nothing, and the phone keeps its own line.
//!
//! CDXC:SessionChat 2026-11-12 SEE-ALSO: the user's decision and the core's half are in
//! packages/gx-chat-core/src/menus/context/computer_pins.rs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags};
use serde_json::Value;

use crate::paths::GxserverPaths;

/// Each agent's record: its chat-core store and the key the desktop saves it under. Keep in step
/// with `packages/client-storage/catalog.ts` and `packages/client-storage-native/src/storage_catalog.rs`.
const RECORDS: [(&str, &str); 6] = [
    ("claudeContext", "ghostex.chat.context-details.v1"),
    ("codexContext", "ghostex.chat.context-details.codex.v1"),
    ("cursorContext", "ghostex.chat.context-details.cursor.v1"),
    ("hermesContext", "ghostex.chat.context-details.hermes.v1"),
    ("piContext", "ghostex.chat.context-details.pi.v1"),
    ("basicContext", "ghostex.chat.context-details.basic.v1"),
];

/// The desktop's client storage, beside gxserver's own state.
fn client_storage_file(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join("client-storage.sqlite3")
}

/// When and how big the file and its write-ahead log were: the records change only with them.
type Stamp = [(Option<SystemTime>, u64); 2];

fn stamp(file: &Path) -> Option<Stamp> {
    let of = |path: &Path| {
        std::fs::metadata(path)
            .ok()
            .map(|meta| (meta.modified().ok(), meta.len()))
    };
    Some([of(file)?, of(&wal_file(file)).unwrap_or((None, 0))])
}

/// The last answer, kept while the file is unchanged: every chat read (and the phone's catch-ups)
/// asks for it.
static LAST: Mutex<Option<(PathBuf, Stamp, Option<BTreeMap<String, Value>>)>> = Mutex::new(None);

/// The records to send, or `None` when this computer has no desktop chat storage to read.
pub(crate) fn status_line_pins(paths: &GxserverPaths) -> Option<BTreeMap<String, Value>> {
    let file = client_storage_file(paths);
    let stamp = stamp(&file)?;
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some((path, seen, pins)) = last.as_ref() {
        if *path == file && *seen == stamp {
            return pins.clone();
        }
    }
    let pins = read_pins(&file);
    *last = Some((file, stamp, pins.clone()));
    pins
}

/// Reads the six records; one that is missing or not a JSON object is left out (its agent pins
/// nothing on the desktop either). A file that cannot be opened or read sends nothing.
///
/// CDXC:SessionChat 2026-11-12 WHY: the desktop's storage is in write-ahead-log mode, and a
/// read-only connection cannot open it while its `-shm` file is missing, which is how the desktop
/// leaves it when it quits. With no `-wal` file either, every record is in the main file, so it is
/// then read as immutable (no locks, no side files written); with a `-wal` but no `-shm` (a crash)
/// nothing is sent until the desktop opens it again.
fn read_pins(file: &Path) -> Option<BTreeMap<String, Value>> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(file, flags)
        .ok()
        .and_then(|connection| read_from(&connection))
        .or_else(|| {
            if wal_file(file).exists() {
                return None;
            }
            let connection = Connection::open_with_flags(
                immutable_uri(file)?,
                flags | OpenFlags::SQLITE_OPEN_URI,
            )
            .ok()?;
            read_from(&connection)
        })
}

fn wal_file(file: &Path) -> PathBuf {
    let mut wal = file.as_os_str().to_owned();
    wal.push("-wal");
    PathBuf::from(wal)
}

/// `file:<path>?immutable=1`, with the characters a URI path cannot hold escaped.
fn immutable_uri(file: &Path) -> Option<String> {
    let path = file.to_str()?;
    let mut uri = String::from("file:");
    for character in path.chars() {
        match character {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3f"),
            '#' => uri.push_str("%23"),
            _ => uri.push(character),
        }
    }
    uri.push_str("?immutable=1");
    Some(uri)
}

fn read_from(connection: &Connection) -> Option<BTreeMap<String, Value>> {
    let mut statement = connection
        .prepare("SELECT value FROM preferences WHERE key=?1")
        .ok()?;
    let mut pins = BTreeMap::new();
    for (store, key) in RECORDS {
        let raw: Option<String> = match statement.query_row([key], |row| row.get(0)) {
            Ok(raw) => Some(raw),
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(_) => return None,
        };
        if let Some(record) = raw
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .filter(Value::is_object)
        {
            pins.insert(store.to_string(), record);
        }
    }
    Some(pins)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storage(dir: &Path, rows: &[(&str, &str)]) -> PathBuf {
        let file = dir.join("client-storage.sqlite3");
        let connection = Connection::open(&file).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; CREATE TABLE preferences (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .unwrap();
        for (key, value) in rows {
            connection
                .execute("INSERT INTO preferences VALUES (?1, ?2)", [key, value])
                .unwrap();
        }
        file
    }

    #[test]
    fn sends_each_saved_record_under_its_store() {
        let dir = tempfile::tempdir().unwrap();
        let file = storage(
            dir.path(),
            &[
                (
                    "ghostex.chat.context-details.codex.v1",
                    r#"{"starred":{"sevenDayLimit":true},"starredOrder":["sevenDayLimit"]}"#,
                ),
                ("ghostex.chat.context-details.v1", "not json"),
                ("ghostex.sidebar.other", r#"{"starred":{}}"#),
            ],
        );
        let pins = read_pins(&file).unwrap();
        assert_eq!(pins.keys().collect::<Vec<_>>(), ["codexContext"]);
        assert_eq!(pins["codexContext"]["starredOrder"][0], "sevenDayLimit");
    }

    #[test]
    fn a_storage_the_desktop_closed_is_read_without_its_side_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = storage(
            dir.path(),
            &[(
                "ghostex.chat.context-details.v1",
                r#"{"starred":{"repo":true}}"#,
            )],
        );
        for side in ["-wal", "-shm"] {
            let mut path = file.as_os_str().to_owned();
            path.push(side);
            let _ = std::fs::remove_file(PathBuf::from(path));
        }
        let pins = read_pins(&file).unwrap();
        assert_eq!(pins["claudeContext"]["starred"]["repo"], true);
        let mut shm = file.as_os_str().to_owned();
        shm.push("-shm");
        assert!(!PathBuf::from(shm).exists());
    }

    #[test]
    fn a_storage_with_no_records_sends_an_empty_map() {
        let dir = tempfile::tempdir().unwrap();
        let file = storage(dir.path(), &[]);
        assert_eq!(read_pins(&file), Some(BTreeMap::new()));
    }

    #[test]
    fn no_storage_sends_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(stamp(&dir.path().join("client-storage.sqlite3")), None);
        assert_eq!(read_pins(&dir.path().join("missing.sqlite3")), None);
    }
}

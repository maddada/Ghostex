//! Opening another program's SQLite database (an agent's store: Codex, ZCode, Hermes, OpenCode,
//! Cursor, Empryo, Antigravity) to read it. Every such read in gxserver, the `ghostex` CLI and the
//! prompt-history scan goes through [`open_agent_store`].
//!
//! CDXC:Sessions 2026-10-29 WHY:
//! Since 2026-10-23 the macOS builds link macOS's own SQLite, which cannot open a WAL-mode database with a read-only connection once its `-wal` and `-shm` files are gone, as they are after the agent quits cleanly: every query fails with "unable to open database file". That emptied Quick Access's Closed list (ZCode) and silently dropped other agents' history. A WAL-mode store is therefore opened read-write so SQLite may create those two files beside it, and guards keep the agent's data untouched: no create flag (a missing database stays missing), `query_only` plus an authorizer that refuses every statement but a read (`query_only` alone still lets `PRAGMA journal_mode = DELETE` rewrite the header), and no checkpoint on close (Ghostex never copies the agent's WAL into its database). Any other store opens read-only, so a rollback journal an agent left behind is never rolled back by Ghostex.

use std::{fs::File, io::Read, path::Path};

use rusqlite::{
    config::DbConfig,
    hooks::{AuthAction, AuthContext, Authorization},
    Connection, OpenFlags,
};

/// Opens the SQLite database at `path`, which belongs to another program, for reading. Errors when
/// it does not exist; it is never created.
pub fn open_agent_store(path: &Path) -> rusqlite::Result<Connection> {
    if !is_wal_mode(path) {
        return Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        );
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
    connection.pragma_update(None, "query_only", true)?;
    connection.authorizer(Some(allow_reads_only))?;
    Ok(connection)
}

/// Pragmas that only report something. A pragma is otherwise refused even without a value:
/// `PRAGMA wal_checkpoint` would copy the agent's WAL into its database.
const REPORTING_PRAGMAS: &[&str] = &[
    "table_info",
    "table_xinfo",
    "table_list",
    "index_list",
    "index_info",
    "index_xinfo",
    "foreign_key_list",
    "user_version",
    "schema_version",
    "data_version",
    "page_count",
    "page_size",
    "freelist_count",
    "encoding",
    "journal_mode",
];

/// Pragmas whose value names the table or index to describe rather than a setting.
const DESCRIBING_PRAGMAS: &[&str] = &[
    "table_info",
    "table_xinfo",
    "table_list",
    "index_list",
    "index_info",
    "index_xinfo",
    "foreign_key_list",
];

/// Queries, the functions and transactions they use, and the reporting pragmas.
fn allow_reads_only(context: AuthContext<'_>) -> Authorization {
    match context.action {
        AuthAction::Select
        | AuthAction::Read { .. }
        | AuthAction::Function { .. }
        | AuthAction::Recursive
        | AuthAction::Transaction { .. }
        | AuthAction::Savepoint { .. } => Authorization::Allow,
        AuthAction::Pragma {
            pragma_name,
            pragma_value,
        } if REPORTING_PRAGMAS.contains(&pragma_name)
            && (pragma_value.is_none() || DESCRIBING_PRAGMAS.contains(&pragma_name)) =>
        {
            Authorization::Allow
        }
        _ => Authorization::Deny,
    }
}

/// The database header's file format versions (bytes 18 and 19) are 2 in WAL mode.
fn is_wal_mode(path: &Path) -> bool {
    let mut header = [0u8; 20];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok_and(|()| {
            header.starts_with(b"SQLite format 3\0") && header[18] == 2 && header[19] == 2
        })
}

#[cfg(test)]
mod tests {
    use super::open_agent_store;
    use rusqlite::{config::DbConfig, Connection};
    use std::{fs, path::Path};

    fn count(connection: &Connection) -> i64 {
        connection
            .query_row("SELECT count(*) FROM rows", [], |row| row.get(0))
            .unwrap()
    }

    fn sidecar(path: &Path, suffix: &str) -> std::path::PathBuf {
        format!("{}{suffix}", path.display()).into()
    }

    /// A WAL-mode store with one row, left the way an agent that quit cleanly leaves it.
    fn closed_wal_store(path: &Path) {
        let agent = Connection::open(path).unwrap();
        agent
            .execute_batch(
                "PRAGMA journal_mode = WAL; CREATE TABLE rows (value TEXT); \
                 INSERT INTO rows VALUES ('one'); PRAGMA wal_checkpoint(TRUNCATE);",
            )
            .unwrap();
        drop(agent);
        for suffix in ["-wal", "-shm"] {
            let _ = fs::remove_file(sidecar(path, suffix));
        }
    }

    #[test]
    fn a_wal_store_whose_agent_quit_cleanly_is_read_without_changing_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        closed_wal_store(&path);
        let before = fs::read(&path).unwrap();

        let store = open_agent_store(&path).expect("the store opens");
        assert_eq!(count(&store), 1);
        assert!(store
            .execute("INSERT INTO rows VALUES ('two')", [])
            .is_err());
        assert!(store.execute_batch("PRAGMA journal_mode = DELETE").is_err());
        assert!(store.execute_batch("PRAGMA query_only = OFF").is_err());
        assert!(store.execute_batch("ATTACH 'other.db' AS other").is_err());
        let columns: Vec<String> = store
            .prepare("SELECT name FROM pragma_table_info('rows')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(columns, ["value"]);
        drop(store);

        assert_eq!(fs::read(&path).unwrap(), before);
    }

    #[test]
    fn the_agents_unsaved_wal_is_read_but_never_copied_into_its_database() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        closed_wal_store(&path);
        let before = fs::read(&path).unwrap();
        // The agent wrote a second row and stopped before checkpointing it.
        let agent = Connection::open(&path).unwrap();
        agent
            .set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)
            .unwrap();
        agent
            .execute_batch("PRAGMA wal_autocheckpoint = 0; INSERT INTO rows VALUES ('two');")
            .unwrap();
        drop(agent);
        let wal = fs::read(sidecar(&path, "-wal")).unwrap();

        let store = open_agent_store(&path).unwrap();
        assert_eq!(count(&store), 2);
        assert!(store.execute_batch("PRAGMA wal_checkpoint").is_err());
        assert!(store.execute_batch("PRAGMA optimize").is_err());
        let version: i64 = store
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 0);
        drop(store);

        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(fs::read(sidecar(&path, "-wal")).unwrap(), wal);
    }

    #[test]
    fn a_missing_store_is_an_error_and_is_not_created() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");

        assert!(open_agent_store(&path).is_err());

        assert!(fs::read_dir(temp.path()).unwrap().next().is_none());
    }

    #[test]
    fn a_rollback_journal_store_opens_read_only() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE rows (value TEXT); INSERT INTO rows VALUES ('one');")
            .unwrap();
        let before = fs::read(&path).unwrap();

        let store = open_agent_store(&path).unwrap();
        assert_eq!(count(&store), 1);
        assert!(store
            .execute("INSERT INTO rows VALUES ('two')", [])
            .is_err());
        drop(store);

        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(!sidecar(&path, "-wal").exists());
    }
}

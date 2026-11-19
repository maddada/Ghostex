//! Step 3: everything Ghostex keeps that names the old folder now names the new one, by path
//! prefix across every project and session, not by project id (risk 5).

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension as _};
use serde_json::{json, Value};

use super::journal::Journal;
use super::path_match::MovedRoot;
use crate::domain::DomainStateError;

/// The tables and text columns that can hold a folder path, as a path or inside JSON. A missing
/// table or column (an older database) is skipped.
const PATH_COLUMNS: &[(&str, &[&str])] = &[
    (
        "projects",
        &[
            "path",
            "worktreeJson",
            "launchSettingsJson",
            "runtimeSettingsJson",
            "projectBoardConfigJson",
            "previousSessionHistoryJson",
            "customCommandsJson",
            "customAgentsJson",
            "gitConfigJson",
        ],
    ),
    (
        "sessions",
        &[
            "cwd",
            "providerStateJson",
            "launchSettingsJson",
            "runtimeSettingsJson",
            "worktreeJson",
        ],
    ),
    ("automations", &["precheckJson", "executionModeJson"]),
    ("automation_runs", &["worktreeJson"]),
    ("stashed_prompts", &["cwd"]),
    ("global_sidebar_commands", &["cwd"]),
];

/// The session runtime key a session whose folder moved carries, so a Codex resume names the new
/// folder with `-C` (agents/resume_plan.rs `with_codex_removed_worktree_cwd`, risk 1).
pub(crate) const MOVED_FOLDER_SESSION_RUNTIME_KEY: &str = "movedProjectFolder";

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    }
}

fn columns_of(db: &Connection, table: &str) -> Result<HashSet<String>, DomainStateError> {
    let mut statement = db
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sql_error)?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(sql_error)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(sql_error)?;
    Ok(names)
}

/// What the rewrite changes, by table, and the sessions whose folder moved.
#[derive(Default)]
pub(crate) struct Rewrite {
    pub(crate) rows_by_table: Vec<(String, usize)>,
    pub(crate) moved_sessions: Vec<(String, String)>,
}

impl Rewrite {
    pub(crate) fn summary(&self) -> Value {
        Value::Object(
            self.rows_by_table
                .iter()
                .filter(|(_, rows)| *rows > 0)
                .map(|(table, rows)| (table.clone(), json!(rows)))
                .collect(),
        )
    }
}

/// Rewrites (or with `apply` false only counts) every stored path under the old folder.
pub(crate) fn rewrite_database(
    db: &Connection,
    root: &MovedRoot,
    apply: bool,
) -> Result<Rewrite, DomainStateError> {
    let mut rewrite = Rewrite::default();
    for (table, wanted) in PATH_COLUMNS {
        let present = columns_of(db, table)?;
        let columns: Vec<&str> = wanted
            .iter()
            .copied()
            .filter(|column| present.contains(*column))
            .collect();
        if columns.is_empty() {
            continue;
        }
        let key_columns = if *table == "sessions" {
            ", projectId, sessionId"
        } else {
            ""
        };
        let select = format!(
            "SELECT rowid, {}{key_columns} FROM {table}",
            columns.join(", ")
        );
        let mut changed_rows: Vec<(
            i64,
            Vec<(usize, String)>,
            Option<(String, String)>,
            Option<String>,
        )> = Vec::new();
        {
            let mut statement = db.prepare(&select).map_err(sql_error)?;
            let mut rows = statement.query([]).map_err(sql_error)?;
            while let Some(row) = rows.next().map_err(sql_error)? {
                let rowid: i64 = row.get(0).map_err(sql_error)?;
                let mut changes = Vec::new();
                let mut old_cwd = None;
                for (index, column) in columns.iter().enumerate() {
                    let value: Option<String> = row.get(index + 1).ok().flatten();
                    let Some(value) = value else { continue };
                    if let Some(moved) = root.rebase_column(&value) {
                        if *column == "cwd" {
                            old_cwd = Some(value.clone());
                        }
                        changes.push((index, moved));
                    }
                }
                if changes.is_empty() {
                    continue;
                }
                let session_key = if *table == "sessions" {
                    let project_id: Option<String> = row.get(columns.len() + 1).ok();
                    let session_id: Option<String> = row.get(columns.len() + 2).ok();
                    project_id.zip(session_id)
                } else {
                    None
                };
                changed_rows.push((rowid, changes, session_key, old_cwd));
            }
        }
        rewrite
            .rows_by_table
            .push((table.to_string(), changed_rows.len()));
        for (rowid, changes, session_key, old_cwd) in changed_rows {
            if let (Some(key), Some(_)) = (&session_key, &old_cwd) {
                rewrite.moved_sessions.push(key.clone());
            }
            if !apply {
                continue;
            }
            for (index, moved) in changes {
                db.execute(
                    &format!(
                        "UPDATE {table} SET {} = ?1 WHERE rowid = ?2",
                        columns[index]
                    ),
                    rusqlite::params![moved, rowid],
                )
                .map_err(sql_error)?;
            }
            // A session whose folder moved is marked for its next resume (risk 1).
            if let (Some(old_cwd), true) = (old_cwd, *table == "sessions") {
                let runtime: Option<String> = db
                    .query_row(
                        "SELECT runtimeSettingsJson FROM sessions WHERE rowid = ?1",
                        [rowid],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(sql_error)?;
                let mut runtime: Value = runtime
                    .and_then(|text| serde_json::from_str(&text).ok())
                    .filter(Value::is_object)
                    .unwrap_or_else(|| json!({}));
                runtime[MOVED_FOLDER_SESSION_RUNTIME_KEY] =
                    json!({ "from": old_cwd, "at": crate::domain::now_iso() });
                db.execute(
                    "UPDATE sessions SET runtimeSettingsJson = ?1 WHERE rowid = ?2",
                    rusqlite::params![runtime.to_string(), rowid],
                )
                .map_err(sql_error)?;
            }
        }
    }
    Ok(rewrite)
}

/// The JSON files outside the database that can hold a folder path: Ghostex's own remembered
/// folder trust, visual pages' source files, and the Files view's chat-file grants.
pub(crate) fn path_files(root_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = vec![root_dir.join("workspace-trust.json")];
    let mut add_folder = |folder: std::path::PathBuf| {
        if let Ok(entries) = std::fs::read_dir(folder) {
            files.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == "json")
            }));
        }
    };
    add_folder(root_dir.join("visual-pages"));
    add_folder(
        ghostex_paths::GhostexPaths::resolve()
            .state_dir
            .join("docs-chat-files"),
    );
    files
}

/// Rewrites (or counts) the paths in `path_files`. Safe to run twice: a rewritten path is no
/// longer under the old folder.
pub(crate) fn rewrite_files(root_dir: &Path, root: &MovedRoot, apply: bool) -> Vec<String> {
    let mut changed = Vec::new();
    for file in path_files(root_dir) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(moved) = root.rebase_json(&value) else {
            continue;
        };
        if apply {
            let bytes = serde_json::to_vec_pretty(&moved).unwrap_or_default();
            if super::journal::write_durably(&file, &bytes).is_err() {
                continue;
            }
        }
        changed.push(file.to_string_lossy().to_string());
    }
    changed
}

/// Step 3 whole: the database in one IMMEDIATE transaction, then the files and the former paths.
/// Every part is safe to run again after a crash.
pub(crate) fn record(
    db: &Connection,
    root_dir: &Path,
    journal: &Journal,
) -> Result<Value, DomainStateError> {
    let root = journal.root();
    // Before the transaction, in the lock order (placement, then SQLite), as the worktree rename does.
    let placement = crate::domain::repository::project::hold_project_placement()?;
    let transaction =
        rusqlite::Transaction::new_unchecked(db, rusqlite::TransactionBehavior::Immediate)
            .map_err(sql_error)?;
    let rewrite = rewrite_database(&transaction, &root, true)?;
    transaction.commit().map_err(sql_error)?;
    drop(placement);
    for project_id in &journal.project_ids {
        crate::work_mode::forget_project_placement(project_id);
    }
    let files = rewrite_files(root_dir, &root, true);
    let _ = super::registry::record_former_path(
        root_dir,
        &journal.project_id,
        &root,
        &journal.old_path,
    );
    Ok(
        json!({ "rows": rewrite.summary(), "sessionsMoved": rewrite.moved_sessions.len(), "files": files }),
    )
}

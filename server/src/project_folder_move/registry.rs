//! What is moving right now (so nothing starts in the old folder meanwhile), and where moved
//! folders used to be (so lookups and the external-session import find them).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use super::path_match::MovedRoot;
use crate::domain::DomainStateError;

struct Moving {
    project_ids: Vec<String>,
    root: MovedRoot,
}

fn moving() -> &'static Mutex<Vec<Moving>> {
    static MOVING: OnceLock<Mutex<Vec<Moving>>> = OnceLock::new();
    MOVING.get_or_init(Mutex::default)
}

/// Marks the projects and the folder as moving until `end_moving` with the first id.
pub(crate) fn begin_moving(project_ids: Vec<String>, root: MovedRoot) {
    if let Ok(mut list) = moving().lock() {
        list.retain(|entry| entry.project_ids.first() != project_ids.first());
        list.push(Moving { project_ids, root });
    }
}

pub(crate) fn end_moving(project_id: &str) {
    if let Ok(mut list) = moving().lock() {
        list.retain(|entry| entry.project_ids.first().map(String::as_str) != Some(project_id));
    }
}

/// Whether `path` is in a folder that is moving.
pub(crate) fn path_is_moving(path: &str) -> bool {
    moving()
        .lock()
        .map(|list| list.iter().any(|entry| entry.root.contains(path)))
        .unwrap_or(false)
}

/// CDXC:Projects 2026-11-19 WHY: a session started, woken or created in a project while its folder moves would run in the old folder (or one that is gone half-way), so every start, wake and new session of a project the move touches, or in a folder inside it, is refused until the move ends (risk 12 of docs/2026-11-19/project-move-folder/PLAN.md). A move that crashed and could not be finished keeps the projects refused until `ghostex move-project-folder --resume` or `--abandon`.
pub(crate) fn refuse_if_moving(
    project_id: &str,
    cwd: Option<&str>,
) -> Result<(), DomainStateError> {
    let Ok(list) = moving().lock() else {
        return Ok(());
    };
    let hit = list.iter().any(|entry| {
        entry.project_ids.iter().any(|id| id == project_id)
            || cwd.is_some_and(|cwd| entry.root.contains(cwd))
    });
    if hit {
        return Err(DomainStateError {
            code: "conflict",
            message: "This project's folder is moving. Try again in a moment; if it stays like this, run ghostex move-project-folder --resume.".to_string(),
        });
    }
    Ok(())
}

/// `<gxserver state>/folder-moves/`: the journals and the former-paths file.
pub(crate) fn moves_dir(root_dir: &std::path::Path) -> PathBuf {
    root_dir.join("folder-moves")
}

fn former_paths_file(root_dir: &std::path::Path) -> PathBuf {
    moves_dir(root_dir).join("former-paths.json")
}

/// One moved project: where its folder is now and where it was.
#[derive(Clone, Debug)]
pub(crate) struct FormerPaths {
    pub(crate) project_id: String,
    pub(crate) path: String,
    pub(crate) former: Vec<String>,
}

pub(crate) fn read_former_paths(root_dir: &std::path::Path) -> Vec<FormerPaths> {
    let Ok(text) = std::fs::read_to_string(former_paths_file(root_dir)) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    value["moves"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(FormerPaths {
                project_id: entry["projectId"].as_str()?.to_string(),
                path: entry["path"].as_str()?.to_string(),
                former: entry["formerPaths"]
                    .as_array()?
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_string))
                    .collect(),
            })
        })
        .collect()
}

/// Records that `project_id`'s folder moved from `root`'s old folder to its new one. Entries of
/// other projects whose folder was inside the moved one follow it too.
///
/// CDXC:Projects 2026-11-19 WHY: a JSON file in gxserver's state folder rather than a new database column: `gxserver resume-lookup` runs as its own process inside a session's restore script and reads it without opening the database, and nothing here needs a migration.
pub(crate) fn record_former_path(
    root_dir: &std::path::Path,
    project_id: &str,
    root: &MovedRoot,
    old_path: &str,
) -> std::io::Result<()> {
    let mut moves = read_former_paths(root_dir);
    for entry in &mut moves {
        if let Some(moved) = root.rebase(&entry.path) {
            entry.path = moved;
        }
    }
    match moves
        .iter_mut()
        .find(|entry| entry.project_id == project_id)
    {
        Some(entry) => {
            entry.path = root.new_path.clone();
            if !entry.former.iter().any(|path| path == old_path) {
                entry.former.push(old_path.to_string());
            }
        }
        None => moves.push(FormerPaths {
            project_id: project_id.to_string(),
            path: root.new_path.clone(),
            former: vec![old_path.to_string()],
        }),
    }
    let value = json!({
        "version": 1,
        "moves": moves.iter().map(|entry| json!({
            "projectId": entry.project_id,
            "path": entry.path,
            "formerPaths": entry.former,
        })).collect::<Vec<_>>(),
    });
    super::journal::write_durably(
        &former_paths_file(root_dir),
        serde_json::to_string_pretty(&value)
            .unwrap_or_default()
            .as_bytes(),
    )
}

/// The paths `path` had before its folder moved, newest move first: for a path at or inside a
/// moved folder, the same place inside each of the folder's former paths.
pub(crate) fn former_equivalents(root_dir: &std::path::Path, path: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in read_former_paths(root_dir) {
        for former in entry.former.iter().rev() {
            // Map back: the current folder plays the old one, the former one the new one.
            let back = MovedRoot {
                old_spellings: vec![entry.path.clone()],
                new_path: former.clone(),
                ignore_case: cfg!(any(windows, target_os = "macos")),
            };
            if let Some(old) = back.rebase(path) {
                if !found.contains(&old) {
                    found.push(old);
                }
            }
        }
    }
    found
}

/// Where a path inside a former folder is now, or `None` when it is in no moved folder.
pub(crate) fn current_for_former(root_dir: &std::path::Path, path: &str) -> Option<String> {
    read_former_paths(root_dir).into_iter().find_map(|entry| {
        entry.former.iter().rev().find_map(|former| {
            MovedRoot {
                old_spellings: vec![former.clone()],
                new_path: entry.path.clone(),
                ignore_case: cfg!(any(windows, target_os = "macos")),
            }
            .rebase(path)
        })
    })
}

/// The state folder `gxserver resume-lookup` and the import read the former paths from.
pub(crate) fn default_root_dir() -> PathBuf {
    crate::paths::get_gxserver_paths(None).root_dir
}

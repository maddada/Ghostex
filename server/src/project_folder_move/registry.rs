//! What is moving right now (so nothing starts in the old folder meanwhile), and where moved
//! folders used to be (so lookups and the external-session import find them).

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError, TryLockError};

use serde_json::{Value, json};

use super::path_match::MovedRoot;
use crate::domain::DomainStateError;

struct Moving {
    project_ids: Vec<String>,
    root: MovedRoot,
}

impl Moving {
    /// The folder before and after the move, as full paths.
    fn places(&self) -> Vec<&str> {
        self.root
            .old_spellings
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(self.root.new_path.as_str()))
            .filter(|place| !place.trim().is_empty())
            .collect()
    }

    /// `path` is at or inside the folder, before or after the move.
    fn touches(&self, path: &str) -> bool {
        self.places()
            .iter()
            .any(|place| within(path, place, self.root.ignore_case))
    }
}

/// `path` is `folder` or inside it.
fn within(path: &str, folder: &str, ignore_case: bool) -> bool {
    MovedRoot {
        old_spellings: vec![folder.to_string()],
        new_path: String::new(),
        ignore_case,
    }
    .contains(path)
}

/// Two moves share a project, or one's folder (before or after) is the other's or inside it.
fn overlaps(a: &Moving, b: &Moving) -> bool {
    let ignore_case = a.root.ignore_case || b.root.ignore_case;
    a.project_ids.iter().any(|id| b.project_ids.contains(id))
        || a.places().iter().any(|p| {
            b.places()
                .iter()
                .any(|q| within(p, q, ignore_case) || within(q, p, ignore_case))
        })
}

fn moving() -> &'static Mutex<Vec<Moving>> {
    static MOVING: OnceLock<Mutex<Vec<Moving>>> = OnceLock::new();
    MOVING.get_or_init(Mutex::default)
}

fn moving_list() -> std::sync::MutexGuard<'static, Vec<Moving>> {
    moving().lock().unwrap_or_else(PoisonError::into_inner)
}

/// CDXC:Projects 2026-11-20 WHY: two moves at once could delete each other's journal and lift each other's block (Opus review of move-project-folder, finding 2), so a move, `--resume`, `--abandon` and the startup recovery each hold this lock from before the checks to the end, and a second one is refused instead of waiting behind a move that runs git for minutes.
pub(crate) fn lock_moves() -> Result<MutexGuard<'static, ()>, DomainStateError> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(Mutex::default).try_lock() {
        Ok(guard) => Ok(guard),
        Err(TryLockError::Poisoned(poisoned)) => Ok(poisoned.into_inner()),
        Err(TryLockError::WouldBlock) => Err(DomainStateError {
            code: "conflict",
            message: "Another project folder move is running. Try again when it has finished."
                .to_string(),
        }),
    }
}

/// Marks the projects and the folder as moving until `end_moving` with the first id. Refused
/// when any entry overlaps it: a move that stopped and is still unfinished keeps its entry, and
/// nothing may move into, out of or around it until it is resumed or abandoned.
pub(crate) fn begin_moving(
    project_ids: Vec<String>,
    root: MovedRoot,
) -> Result<(), DomainStateError> {
    let mut list = moving_list();
    let candidate = Moving { project_ids, root };
    if let Some(other) = list.iter().find(|entry| overlaps(entry, &candidate)) {
        return Err(DomainStateError {
            code: "conflict",
            message: format!(
                "The move of {} to {} hasn't finished and touches this folder. Finish it with ghostex move-project-folder --project-id {} --resume (or --abandon) first.",
                other
                    .root
                    .old_spellings
                    .first()
                    .map(String::as_str)
                    .unwrap_or_default(),
                other.root.new_path,
                other
                    .project_ids
                    .first()
                    .map(String::as_str)
                    .unwrap_or_default(),
            ),
        });
    }
    list.push(candidate);
    Ok(())
}

/// The startup recovery's `begin_moving`: its journals are what is moving, checked or not.
pub(crate) fn keep_moving(project_ids: Vec<String>, root: MovedRoot) {
    moving_list().push(Moving { project_ids, root });
}

pub(crate) fn end_moving(project_id: &str) {
    moving_list().retain(|entry| entry.project_ids.first().map(String::as_str) != Some(project_id));
}

/// Whether `path` is in a folder that is moving, at its old place or its new one.
pub(crate) fn path_is_moving(path: &str) -> bool {
    moving_list().iter().any(|entry| entry.touches(path))
}

/// CDXC:Projects 2026-11-19 WHY: a session started, woken or created in a project while its folder moves would run in the old folder (or one that is gone half-way), so every start, wake and new session of a project the move touches, or in a folder inside it, is refused until the move ends (risk 12 of docs/2026-11-19/project-move-folder/PLAN.md). A move that crashed and could not be finished keeps the projects refused until `ghostex move-project-folder --resume` or `--abandon`. Rename Worktree, Locate Folder, remove-project and deleting a worktree ask too (Opus review, finding 2).
pub(crate) fn refuse_if_moving(
    project_id: &str,
    cwd: Option<&str>,
) -> Result<(), DomainStateError> {
    let hit = moving_list().iter().any(|entry| {
        entry.project_ids.iter().any(|id| id == project_id)
            || cwd.is_some_and(|cwd| entry.touches(cwd))
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

/// CDXC:Projects 2026-11-20 WHY: a former path counts only while nothing else is there: once a new folder exists at the old place (a fresh clone of the same repository, say), conversations from that path are the new folder's own, and sending them to the moved project would resume an agent in the wrong repository (Opus review of move-project-folder, finding 3). The same folder under other capitals (a case-only move) still counts.
fn former_is_free(former: &str, current: &str) -> bool {
    std::fs::symlink_metadata(former).is_err()
        || super::fs_ops::same_folder_on_disk(
            std::path::Path::new(former),
            std::path::Path::new(current),
        )
}

/// The paths `path` had before its folder moved, newest move first: for a path at or inside a
/// moved folder, the same place inside each of the folder's former paths that nothing else
/// occupies now.
pub(crate) fn former_equivalents(root_dir: &std::path::Path, path: &str) -> Vec<String> {
    let mut found = Vec::new();
    for entry in read_former_paths(root_dir) {
        for former in entry.former.iter().rev() {
            if !former_is_free(former, &entry.path) {
                continue;
            }
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

/// Where a path inside a former folder is now, or `None` when it is in no moved folder or
/// something else is at that former folder now.
pub(crate) fn current_for_former(root_dir: &std::path::Path, path: &str) -> Option<String> {
    read_former_paths(root_dir).into_iter().find_map(|entry| {
        entry.former.iter().rev().find_map(|former| {
            let moved = MovedRoot {
                old_spellings: vec![former.clone()],
                new_path: entry.path.clone(),
                ignore_case: cfg!(any(windows, target_os = "macos")),
            }
            .rebase(path)?;
            former_is_free(former, &entry.path).then_some(moved)
        })
    })
}

/// The state folder `gxserver resume-lookup` and the import read the former paths from.
pub(crate) fn default_root_dir() -> PathBuf {
    crate::paths::get_gxserver_paths(None).root_dir
}

/// Tests that take the move lock run one at a time.
#[cfg(test)]
pub(crate) fn test_serial() -> MutexGuard<'static, ()> {
    static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();
    SERIAL
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(old: &str, new: &str) -> MovedRoot {
        MovedRoot {
            old_spellings: vec![old.to_string()],
            new_path: new.to_string(),
            ignore_case: false,
        }
    }

    #[test]
    fn a_moving_folder_refuses_starts_in_it_and_overlapping_moves() {
        let ids = vec!["reg-a".to_string(), "reg-a-worktree".to_string()];
        begin_moving(ids, root("/reg-test/App", "/reg-test/Kiwi")).unwrap();

        // Its projects, and paths at or inside the folder before and after.
        assert!(refuse_if_moving("reg-a", None).is_err());
        assert!(refuse_if_moving("reg-a-worktree", None).is_err());
        assert!(refuse_if_moving("other", Some("/reg-test/App/src")).is_err());
        assert!(refuse_if_moving("other", Some("/reg-test/Kiwi")).is_err());
        assert!(path_is_moving("/reg-test/App"));
        // A sibling whose name only starts the same is free.
        assert!(refuse_if_moving("other", Some("/reg-test/App-feat")).is_ok());
        assert!(!path_is_moving("/reg-test/App-feat"));

        // A second move of a project it touches, of a folder inside it, of the folder around it,
        // or into it, is refused.
        for (ids, old, new) in [
            (vec!["reg-a"], "/reg-test/Elsewhere", "/reg-test/Else2"),
            (vec!["reg-b"], "/reg-test/App/inner", "/reg-test/Inner"),
            (vec!["reg-c"], "/reg-test", "/reg-test2"),
            (vec!["reg-d"], "/reg-test/Pear", "/reg-test/Kiwi/Pear"),
        ] {
            let ids = ids.into_iter().map(str::to_string).collect();
            assert!(
                begin_moving(ids, root(old, new)).is_err(),
                "{old} -> {new} should be refused"
            );
        }
        // One that touches nothing of it goes ahead.
        begin_moving(
            vec!["reg-e".into()],
            root("/reg-test/Pear", "/reg-test/Plum"),
        )
        .unwrap();
        end_moving("reg-e");

        end_moving("reg-a");
        assert!(refuse_if_moving("reg-a", Some("/reg-test/App/src")).is_ok());
        assert!(!path_is_moving("/reg-test/Kiwi"));
    }

    #[test]
    fn only_one_move_holds_the_lock() {
        let _serial = test_serial();
        let first = lock_moves().unwrap();
        assert_eq!(lock_moves().unwrap_err().code, "conflict");
        drop(first);
        assert!(lock_moves().is_ok());
    }

    #[test]
    fn a_former_path_counts_only_while_nothing_else_is_there() {
        let state = tempfile::tempdir().unwrap();
        let disk = tempfile::tempdir().unwrap();
        let old = disk.path().join("App").to_string_lossy().to_string();
        let new = disk.path().join("Kiwi").to_string_lossy().to_string();
        std::fs::create_dir(&new).unwrap();
        record_former_path(state.path(), "p1", &root(&old, &new), &old).unwrap();

        let inside_old = format!("{old}/src");
        let inside_new = format!("{new}/src");
        assert_eq!(
            current_for_former(state.path(), &inside_old).as_deref(),
            Some(inside_new.as_str())
        );
        assert_eq!(
            former_equivalents(state.path(), &inside_new),
            vec![inside_old.clone()]
        );

        // A fresh clone at the old place: its conversations are its own.
        std::fs::create_dir(&old).unwrap();
        assert_eq!(current_for_former(state.path(), &inside_old), None);
        assert!(former_equivalents(state.path(), &inside_new).is_empty());
    }
}
